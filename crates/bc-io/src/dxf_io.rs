//! Import DXF R12–2018 i eksport AC1009/AC1021 (crate `dxf`, ADR 0008).

use std::f64::consts::{PI, TAU};

use bc_doc::{Document, Kind};
use bc_geom::{Affine, BezPath, PathEl, Point, Vec2};
use dxf::entities::{Entity, EntityType};
use dxf::enums::{AcadVersion, Units};
use dxf::Drawing;
use kurbo::PathSeg;

use crate::{export_paths, hex, ExportOptions, ImportOptions, Imported, IoError, Item, LayerKey};

/// Maksymalne zagnieżdżenie bloków (INSERT w INSERT).
const MAX_BLOCK_DEPTH: u8 = 16;
/// Tolerancja upraszczania próbkowanych splajnów (mm).
const SPLINE_TOL: f64 = 0.005;
/// Najwięcej kopii z jednej tablicy INSERT (wiersze × kolumny).
const MAX_INSERT_ARRAY: i64 = 10_000;
/// Budżet encji rozwijanych z bloków na cały plik.
const MAX_PRIMITIVES: usize = 2_000_000;

/// Tolerancja spłaszczania krzywych przy eksporcie R12 (mm).
const R12_TOL: f64 = 0.01;

// ---------- kolory ACI ----------

/// Przybliżona paleta AutoCAD Color Index: 1–9 dokładnie, 10–249 koło barw, 250–255 szarości.
pub fn aci_rgb(i: u8) -> (u8, u8, u8) {
    match i {
        1 => (255, 0, 0),
        2 => (255, 255, 0),
        3 => (0, 255, 0),
        4 => (0, 255, 255),
        5 => (0, 0, 255),
        6 => (255, 0, 255),
        // 7 = biały/czarny zależnie od tła — na jasnym obszarze roboczym czarny
        0 | 7 => (0, 0, 0),
        8 => (128, 128, 128),
        9 => (192, 192, 192),
        250..=255 => {
            let g = [51u8, 91, 132, 173, 214, 255][(i - 250) as usize];
            (g, g, g)
        }
        _ => {
            let hue = ((i / 10) as f64 - 1.0) * 15.0;
            let v = i % 10;
            let bright = [1.0, 1.0, 0.65, 0.65, 0.5, 0.5, 0.3, 0.3, 0.15, 0.15][v as usize];
            let sat = if v.is_multiple_of(2) { 1.0 } else { 0.5 };
            let (r, g, b) = hsv(hue, sat, bright);
            (r, g, b)
        }
    }
}

fn hsv(h: f64, s: f64, v: f64) -> (u8, u8, u8) {
    let c = v * s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let m = v - c;
    let (r, g, b) = match (h / 60.0) as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    (
        ((r + m) * 255.0).round() as u8,
        ((g + m) * 255.0).round() as u8,
        ((b + m) * 255.0).round() as u8,
    )
}

fn parse_hex(c: &str) -> (u8, u8, u8) {
    let v = u32::from_str_radix(c.trim_start_matches('#'), 16).unwrap_or(0);
    ((v >> 16) as u8, (v >> 8) as u8, v as u8)
}

/// Najbliższy kolor ACI (1–255) dla `#rrggbb`.
pub fn nearest_aci(c: &str) -> u8 {
    let (r, g, b) = parse_hex(c);
    (1..=255u8)
        .min_by_key(|i| {
            let (ar, ag, ab) = aci_rgb(*i);
            let d = |a: u8, b: u8| (a as i32 - b as i32).pow(2);
            d(ar, r) + d(ag, g) + d(ab, b)
        })
        .unwrap_or(7)
}

// ---------- import ----------

fn unit_scale(u: Units, opt: &ImportOptions) -> f64 {
    match u {
        Units::Inches => 25.4,
        Units::Feet => 304.8,
        Units::Millimeters => 1.0,
        Units::Centimeters => 10.0,
        Units::Meters => 1000.0,
        Units::Decimeters => 100.0,
        Units::Microns => 0.001,
        Units::Mils => 0.0254,
        Units::Yards => 914.4,
        _ => opt.units.map(|u| u.mm()).unwrap_or(1.0),
    }
}

fn p2(p: &dxf::Point) -> Point {
    Point::new(p.x, p.y)
}

/// Łuk z wybrzuszeniem (bulge) między dwoma wierzchołkami polilinii.
fn bulge_to(path: &mut BezPath, from: Point, to: Point, b: f64) {
    if b.abs() < 1e-12 || !b.is_finite() || from.distance(to) < 1e-12 {
        path.line_to(to);
        return;
    }
    let theta = 4.0 * b.atan();
    let r = from.distance(to) / (2.0 * (theta / 2.0).sin().abs());
    let arc = kurbo::SvgArc {
        from,
        to,
        radii: Vec2::new(r, r),
        x_rotation: 0.0,
        large_arc: theta.abs() > PI,
        sweep: b > 0.0,
    };
    match kurbo::Arc::from_svg_arc(&arc) {
        Some(a) => {
            for el in a.append_iter(bc_geom::arc_tolerance(r)) {
                path.push(el);
            }
        }
        None => path.line_to(to),
    }
}

fn polyline(verts: &[(Point, f64)], closed: bool) -> BezPath {
    let mut p = BezPath::new();
    let Some(first) = verts.first() else { return p };
    p.move_to(first.0);
    for w in verts.windows(2) {
        bulge_to(&mut p, w[0].0, w[1].0, w[0].1);
    }
    if closed && verts.len() > 1 {
        let last = verts.last().unwrap();
        bulge_to(&mut p, last.0, first.0, last.1);
        p.close_path();
    }
    p
}

/// Punkty splajnu (NURBS) algorytmem de Boora.
fn spline_points(sp: &dxf::entities::Spline) -> Option<Vec<Point>> {
    let cps = &sp.control_points;
    if cps.is_empty() {
        return (sp.fit_points.len() >= 2).then(|| sp.fit_points.iter().map(p2).collect());
    }
    let p = sp.degree_of_curve.max(1) as usize;
    let n = cps.len();
    let k = &sp.knot_values;
    if n < p + 1
        || k.len() != n + p + 1
        || k.iter().any(|v| !v.is_finite())
        || k.windows(2).any(|w| w[1] < w[0])
    {
        return None;
    }
    let w: Vec<f64> = if sp.weight_values.len() == n {
        sp.weight_values.clone()
    } else {
        vec![1.0; n]
    };
    let (u0, u1) = (k[p], k[n]);
    if u1 <= u0 || w.iter().any(|x| !x.is_finite() || *x <= 0.0) {
        return None;
    }
    let samples = (n * 24).clamp(16, 5000);
    let mut out = Vec::with_capacity(samples + 1);
    for s in 0..=samples {
        let u = u0 + (u1 - u0) * s as f64 / samples as f64;
        let span = (p..n)
            .rev()
            .find(|i| k[*i] <= u && (u < k[*i + 1] || *i == n - 1))?;
        let mut d: Vec<(f64, f64, f64)> = (0..=p)
            .map(|j| {
                let c = &cps[j + span - p];
                let ww = w[j + span - p];
                (c.x * ww, c.y * ww, ww)
            })
            .collect();
        for r in 1..=p {
            for j in (r..=p).rev() {
                let den = k[j + 1 + span - r] - k[j + span - p];
                let a = if den.abs() < 1e-300 {
                    0.0
                } else {
                    (u - k[j + span - p]) / den
                };
                d[j] = (
                    (1.0 - a) * d[j - 1].0 + a * d[j].0,
                    (1.0 - a) * d[j - 1].1 + a * d[j].1,
                    (1.0 - a) * d[j - 1].2 + a * d[j].2,
                );
            }
        }
        let (x, y, ww) = d[p];
        out.push(Point::new(x / ww, y / ww));
    }
    Some(out)
}

struct Ctx<'a> {
    drawing: &'a Drawing,
    opt: &'a ImportOptions,
    text_warned: bool,
    /// Ile prymitywów jeszcze wolno rozwinąć.
    budget: usize,
    budget_warned: bool,
    range_warned: bool,
    unsupported: Vec<&'static str>,
    warnings: Vec<String>,
}

fn entity_type_name(e: &EntityType) -> &'static str {
    match e {
        EntityType::Solid(_) => "SOLID",
        EntityType::Image(_) => "IMAGE",
        EntityType::RotatedDimension(_) | EntityType::RadialDimension(_) => "DIMENSION",
        EntityType::Leader(_) => "LEADER",
        EntityType::ModelPoint(_) => "POINT",
        EntityType::Face3D(_) => "3DFACE",
        _ => "inne",
    }
}

fn sane(v: f64) -> bool {
    v.is_finite() && v.abs() <= crate::RAW_LIMIT
}

fn sane_pt(p: &dxf::Point) -> bool {
    sane(p.x) && sane(p.y)
}

/// Czy wartości encji mieszczą się w zakresie (przed budowaniem geometrii — łuki
/// o ogromnym promieniu kończyły się brakiem pamięci).
fn entity_in_range(e: &EntityType) -> bool {
    match e {
        EntityType::Line(l) => sane_pt(&l.p1) && sane_pt(&l.p2),
        EntityType::LwPolyline(lw) => lw
            .vertices
            .iter()
            .all(|v| sane(v.x) && sane(v.y) && sane(v.bulge)),
        EntityType::Polyline(pl) => pl.vertices().all(|v| sane_pt(&v.location) && sane(v.bulge)),
        EntityType::Circle(c) => sane_pt(&c.center) && sane(c.radius),
        EntityType::Arc(a) => {
            sane_pt(&a.center) && sane(a.radius) && sane(a.start_angle) && sane(a.end_angle)
        }
        EntityType::Ellipse(el) => {
            sane_pt(&el.center)
                && sane(el.major_axis.x)
                && sane(el.major_axis.y)
                && sane(el.minor_axis_ratio)
                && sane(el.start_parameter)
                && sane(el.end_parameter)
        }
        EntityType::Spline(sp) => {
            sp.control_points
                .iter()
                .chain(sp.fit_points.iter())
                .all(sane_pt)
                && sp
                    .knot_values
                    .iter()
                    .chain(sp.weight_values.iter())
                    .all(|v| sane(*v))
        }
        EntityType::Insert(i) => {
            sane_pt(&i.location)
                && [
                    i.x_scale_factor,
                    i.y_scale_factor,
                    i.rotation,
                    i.column_spacing,
                    i.row_spacing,
                ]
                .iter()
                .all(|v| sane(*v))
                && i.x_scale_factor.abs() <= 1e6
                && i.y_scale_factor.abs() <= 1e6
        }
        _ => true,
    }
}

fn collect(e: &Entity, t: Affine, depth: u8, ctx: &mut Ctx, out: &mut BezPath) {
    if !entity_in_range(&e.specific) {
        if !ctx.range_warned {
            ctx.warnings.push(format!(
                "pominięto encje z wartościami poza zakresem (|x| > {})",
                crate::RAW_LIMIT
            ));
            ctx.range_warned = true;
        }
        return;
    }
    if ctx.budget == 0 {
        if !ctx.budget_warned {
            ctx.warnings.push(format!(
                "osiągnięto limit {MAX_PRIMITIVES} encji rozwijanych z bloków — resztę pominięto"
            ));
            ctx.budget_warned = true;
        }
        return;
    }
    ctx.budget -= 1;
    let mut push = |mut p: BezPath| {
        p.apply_affine(t);
        for el in p.elements() {
            out.push(*el);
        }
    };
    match &e.specific {
        EntityType::Line(l) => {
            let mut p = BezPath::new();
            p.move_to(p2(&l.p1));
            p.line_to(p2(&l.p2));
            push(p);
        }
        EntityType::LwPolyline(lw) => {
            let v: Vec<(Point, f64)> = lw
                .vertices
                .iter()
                .map(|v| (Point::new(v.x, v.y), v.bulge))
                .collect();
            push(polyline(&v, lw.flags & 1 == 1));
        }
        EntityType::Polyline(pl) => {
            let v: Vec<(Point, f64)> = pl.vertices().map(|v| (p2(&v.location), v.bulge)).collect();
            push(polyline(&v, pl.flags & 1 == 1));
        }
        EntityType::Circle(c) if c.radius > 0.0 => push(bc_geom::ellipse_path(
            c.center.x, c.center.y, c.radius, c.radius,
        )),
        EntityType::Arc(a) if a.radius > 0.0 => {
            let mut sweep = (a.end_angle - a.start_angle).rem_euclid(360.0);
            if sweep < 1e-9 {
                sweep = 360.0;
            }
            if let Ok(p) = bc_geom::ops::arc(a.center.x, a.center.y, a.radius, a.start_angle, sweep)
            {
                push(p);
            }
        }
        EntityType::Ellipse(el) => {
            let major = Vec2::new(el.major_axis.x, el.major_axis.y);
            let a = major.hypot();
            let b = a * el.minor_axis_ratio;
            if a > 0.0 && b > 0.0 && b.is_finite() {
                let rot = major.atan2();
                let mut sweep = (el.end_parameter - el.start_parameter).rem_euclid(TAU);
                if sweep < 1e-9 {
                    sweep = TAU;
                }
                let c = p2(&el.center);
                if (sweep - TAU).abs() < 1e-9 {
                    push(kurbo::Shape::to_path(
                        &kurbo::Ellipse::new(c, (a, b), rot),
                        bc_geom::arc_tolerance(a),
                    ));
                } else {
                    let arc = kurbo::Arc::new(c, (a, b), el.start_parameter, sweep, rot);
                    let mut p = BezPath::new();
                    let start = c
                        + (Affine::rotate(rot)
                            * Point::new(
                                a * el.start_parameter.cos(),
                                b * el.start_parameter.sin(),
                            ))
                        .to_vec2();
                    p.move_to(start);
                    for x in arc.append_iter(bc_geom::arc_tolerance(a)) {
                        p.push(x);
                    }
                    push(p);
                }
            }
        }
        EntityType::Spline(sp) => match spline_points(sp) {
            Some(pts) => {
                let world: Vec<Point> = pts.iter().map(|q| t * *q).collect();
                let poly = bc_geom::polyline_path(&world, sp.flags & 1 == 1);
                let reduced = bc_geom::ops::reduce_nodes(&poly, SPLINE_TOL);
                for el in reduced.elements() {
                    out.push(*el);
                }
            }
            None => ctx
                .warnings
                .push("splajn z niepoprawnymi węzłami — pominięto".into()),
        },
        EntityType::Insert(ins) => {
            if depth >= MAX_BLOCK_DEPTH {
                ctx.warnings.push(format!(
                    "blok {} zagnieżdżony zbyt głęboko — pominięto",
                    ins.name
                ));
                return;
            }
            let Some(block) = ctx.drawing.blocks().find(|b| b.name == ins.name) else {
                ctx.warnings
                    .push(format!("brak definicji bloku {}", ins.name));
                return;
            };
            let copies = i64::from(ins.row_count.max(1)) * i64::from(ins.column_count.max(1));
            if copies > MAX_INSERT_ARRAY {
                ctx.warnings.push(format!(
                    "tablica bloku {} ma {copies} kopii — limit {MAX_INSERT_ARRAY}; pominięto",
                    ins.name
                ));
                return;
            }
            let sx = if ins.x_scale_factor == 0.0 {
                1.0
            } else {
                ins.x_scale_factor
            };
            let sy = if ins.y_scale_factor == 0.0 {
                1.0
            } else {
                ins.y_scale_factor
            };
            for r in 0..ins.row_count.max(1) {
                for c in 0..ins.column_count.max(1) {
                    let m = t
                        * Affine::translate(Vec2::new(ins.location.x, ins.location.y))
                        * Affine::rotate(ins.rotation.to_radians())
                        * Affine::translate(Vec2::new(
                            c as f64 * ins.column_spacing,
                            r as f64 * ins.row_spacing,
                        ))
                        * Affine::scale_non_uniform(sx, sy)
                        * Affine::translate(Vec2::new(-block.base_point.x, -block.base_point.y));
                    for be in &block.entities {
                        collect(be, m, depth + 1, ctx, out);
                    }
                }
            }
        }
        EntityType::Text(_) | EntityType::MText(_) => {
            if !ctx.opt.ignore_text && !ctx.text_warned {
                ctx.warnings.push("pominięto tekst DXF — import tekstu wymaga fontów (M4); zamień tekst na krzywe w programie źródłowym".into());
                ctx.text_warned = true;
            }
        }
        other => {
            let n = entity_type_name(other);
            if !ctx.unsupported.contains(&n) {
                ctx.unsupported.push(n);
            }
        }
    }
}

fn entity_color(e: &Entity, d: &Drawing) -> Option<String> {
    if e.common.color_24_bit > 0 {
        let v = e.common.color_24_bit as u32;
        return Some(hex((v >> 16) as u8, (v >> 8) as u8, v as u8));
    }
    let idx = if e.common.color.is_by_layer() {
        d.layers()
            .find(|l| l.name == e.common.layer)
            .and_then(|l| l.color.index())
    } else {
        e.common.color.index()
    };
    idx.map(|i| {
        let (r, g, b) = aci_rgb(i);
        hex(r, g, b)
    })
}

pub fn import(data: &[u8], opt: &ImportOptions) -> Result<Imported, IoError> {
    let drawing = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        Drawing::load(&mut std::io::Cursor::new(data))
    }))
    .map_err(|_| IoError::Parse("DXF: uszkodzony plik".into()))?
    .map_err(|e| IoError::Parse(format!("DXF: {e}")))?;
    let scale = unit_scale(drawing.header.default_drawing_units, opt);
    let mut ctx = Ctx {
        drawing: &drawing,
        opt,
        text_warned: false,
        budget: MAX_PRIMITIVES,
        budget_warned: false,
        range_warned: false,
        unsupported: Vec::new(),
        warnings: Vec::new(),
    };
    let mut out = Imported::default();
    for e in drawing.entities() {
        if e.common.is_in_paper_space {
            continue;
        }
        let mut path = BezPath::new();
        collect(e, Affine::scale(scale), 0, &mut ctx, &mut path);
        if path.elements().len() < 2
            || path.elements().iter().any(|el| {
                el.end_point()
                    .is_some_and(|p| !p.x.is_finite() || !p.y.is_finite())
            })
        {
            continue;
        }
        out.items.push(Item {
            kind: Kind::Path { d: path.to_svg() },
            transform: Affine::IDENTITY,
            color_key: entity_color(e, &drawing).map(LayerKey::Color),
            name_key: Some(LayerKey::Name(e.common.layer.clone())),
        });
    }
    crate::retain_in_range(&mut out);
    out.warnings.append(&mut ctx.warnings);
    if !ctx.unsupported.is_empty() {
        out.warnings.push(format!(
            "DXF: pominięto nieobsługiwane encje: {}",
            ctx.unsupported.join(", ")
        ));
    }
    Ok(out)
}

// ---------- eksport ----------

fn sanitize_layer(name: &str) -> String {
    let s: String = name
        .chars()
        .map(|c| {
            if "<>/\\\":;?*|=`".contains(c) || c.is_control() {
                '_'
            } else {
                c
            }
        })
        .collect();
    if s.trim().is_empty() {
        "0".into()
    } else {
        s
    }
}

fn has_curves(p: &BezPath) -> bool {
    p.elements()
        .iter()
        .any(|e| matches!(e, PathEl::QuadTo(..) | PathEl::CurveTo(..)))
}

fn dp(p: Point) -> dxf::Point {
    dxf::Point::new(p.x, p.y, 0.0)
}

/// Odcinki ścieżki jako jeden B-splajn stopnia 3 (dokładnie, bez spłaszczania).
fn spline_of(sub: &BezPath) -> Option<dxf::entities::Spline> {
    let segs: Vec<PathSeg> = sub.segments().collect();
    let first = segs.first()?;
    use kurbo::ParamCurve;
    let mut cps = vec![dp(first.start())];
    for s in &segs {
        let (c1, c2, p3) = match *s {
            PathSeg::Line(l) => (l.p0.lerp(l.p1, 1.0 / 3.0), l.p0.lerp(l.p1, 2.0 / 3.0), l.p1),
            PathSeg::Quad(q) => (
                q.p0 + (q.p1 - q.p0) * (2.0 / 3.0),
                q.p2 + (q.p1 - q.p2) * (2.0 / 3.0),
                q.p2,
            ),
            PathSeg::Cubic(c) => (c.p1, c.p2, c.p3),
        };
        cps.extend([dp(c1), dp(c2), dp(p3)]);
    }
    let m = segs.len();
    let mut knots = vec![0.0; 4];
    for i in 1..m {
        knots.extend([i as f64; 3]);
    }
    knots.extend([m as f64; 4]);
    Some(dxf::entities::Spline {
        degree_of_curve: 3,
        knot_values: knots,
        control_points: cps,
        ..Default::default()
    })
}

pub fn export(doc: &Document, r12: bool, opt: &ExportOptions) -> Result<Vec<u8>, IoError> {
    let mut d = Drawing::new();
    d.header.version = if r12 {
        AcadVersion::R12
    } else {
        AcadVersion::R2007
    };
    d.header.default_drawing_units = Units::Millimeters;
    for (layer, paths) in export_paths(doc, opt)? {
        let name = sanitize_layer(&layer.name);
        if !d.layers().any(|l| l.name == name) {
            d.add_layer(dxf::tables::Layer {
                name: name.clone(),
                color: dxf::Color::from_index(nearest_aci(&layer.color)),
                ..Default::default()
            });
        }
        for p in paths {
            for sub in bc_geom::subpaths(&p) {
                let closed = bc_geom::is_closed(&sub);
                let specific = if !r12 && has_curves(&sub) {
                    match spline_of(&sub) {
                        Some(s) => EntityType::Spline(s),
                        None => continue,
                    }
                } else {
                    let mut pts: Vec<Point> = bc_geom::flatten(&sub, R12_TOL)
                        .into_iter()
                        .flatten()
                        .collect();
                    pts.dedup_by(|a, b| a.distance(*b) < 1e-9);
                    if closed
                        && pts.len() > 1
                        && pts.first().unwrap().distance(*pts.last().unwrap()) < 1e-9
                    {
                        pts.pop();
                    }
                    if pts.len() < 2 {
                        continue;
                    }
                    if r12 {
                        let mut pl = dxf::entities::Polyline {
                            flags: if closed { 1 } else { 0 },
                            ..Default::default()
                        };
                        for q in &pts {
                            pl.add_vertex(&mut d, dxf::entities::Vertex::new(dp(*q)));
                        }
                        EntityType::Polyline(pl)
                    } else {
                        let mut lw = dxf::entities::LwPolyline {
                            flags: if closed { 1 } else { 0 },
                            ..Default::default()
                        };
                        lw.vertices = pts
                            .iter()
                            .map(|q| dxf::LwPolylineVertex {
                                x: q.x,
                                y: q.y,
                                ..Default::default()
                            })
                            .collect();
                        EntityType::LwPolyline(lw)
                    }
                };
                let mut e = Entity::new(specific);
                e.common.layer = name.clone();
                d.add_entity(e);
            }
        }
    }
    let mut buf = Vec::new();
    d.save(&mut buf)
        .map_err(|e| IoError::Io(format!("DXF: {e}")))?;
    Ok(buf)
}
