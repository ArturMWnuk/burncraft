//! Operacje geometryczne (ADR 0006): kształty parametryczne, booleany, offsety
//! i operacje na ścieżkach. Booleany i offsety działają na kształtach zamkniętych
//! spłaszczonych z tolerancją [`BOOLEAN_TOLERANCE`] i zwracają ścieżki liniowe.

use clipper2_rust::{
    EndType, FillRule, JoinType, Path64 as CPath, Paths64 as CPaths, Point64, PointInPolygonResult,
};
use kurbo::{BezPath, PathEl, PathSeg, Point, Vec2};

/// Tolerancja spłaszczania krzywych przed booleanem/offsetem (mm).
pub const BOOLEAN_TOLERANCE: f64 = 0.001;

/// Limit ostrza dla narożników miter (krotność odległości offsetu).
const MITER_LIMIT: f64 = 4.0;

/// Skala całkowitoliczbowa Clippera: 1 jednostka = 0,1 µm.
const SCALE: f64 = 10_000.0;

/// Tolerancja łuków przy offsecie Round (mm).
const ARC_TOLERANCE: f64 = 0.0005;

#[derive(Debug, Clone, PartialEq)]
pub enum GeomError {
    /// Operacja wymaga kształtów zamkniętych.
    OpenPath,
    /// Parametry poza dopuszczalnym zakresem.
    Invalid(String),
    /// Błąd biblioteki Clipper2.
    Clipper(String),
}

impl std::fmt::Display for GeomError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GeomError::OpenPath => write!(f, "operacja wymaga kształtów zamkniętych"),
            GeomError::Invalid(m) => write!(f, "{m}"),
            GeomError::Clipper(m) => write!(f, "Clipper2: {m}"),
        }
    }
}

impl std::error::Error for GeomError {}

fn invalid(m: impl Into<String>) -> GeomError {
    GeomError::Invalid(m.into())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BoolOp {
    Union,
    Difference,
    Intersection,
    Xor,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Join {
    /// Narożniki ostre (miter, limit 4×).
    Miter,
    /// Narożniki zaokrąglone.
    Round,
    /// Narożniki ścięte.
    Bevel,
}

// ---------- kształty parametryczne ----------

fn closed_polygon(points: &[Point]) -> BezPath {
    let mut p = BezPath::new();
    for (i, q) in points.iter().enumerate() {
        if i == 0 {
            p.move_to(*q);
        } else {
            p.line_to(*q);
        }
    }
    p.close_path();
    p
}

fn on_circle(cx: f64, cy: f64, r: f64, deg: f64) -> Point {
    let a = deg.to_radians();
    Point::new(cx + r * a.cos(), cy + r * a.sin())
}

fn check_center(cx: f64, cy: f64, rot: f64) -> Result<(), GeomError> {
    if cx.is_finite() && cy.is_finite() && rot.is_finite() {
        Ok(())
    } else {
        Err(invalid("współrzędne i kąt muszą być skończone"))
    }
}

fn positive(name: &str, v: f64) -> Result<(), GeomError> {
    if v.is_finite() && v > 0.0 {
        Ok(())
    } else {
        Err(invalid(format!("{name} musi być > 0")))
    }
}

/// Wielokąt foremny wpisany w okrąg o promieniu `r`; pierwszy wierzchołek na
/// kierunku `rotation_deg` (0° = oś +X).
pub fn regular_polygon(
    cx: f64,
    cy: f64,
    r: f64,
    sides: u32,
    rotation_deg: f64,
) -> Result<BezPath, GeomError> {
    check_center(cx, cy, rotation_deg)?;
    positive("r", r)?;
    if !(3..=10_000).contains(&sides) {
        return Err(invalid("liczba boków: 3–10000"));
    }
    let pts: Vec<Point> = (0..sides)
        .map(|i| on_circle(cx, cy, r, rotation_deg + 360.0 * i as f64 / sides as f64))
        .collect();
    Ok(closed_polygon(&pts))
}

/// Gwiazda: `points` ramion, wierzchołki naprzemiennie na `r_outer` i `r_inner`.
pub fn star(
    cx: f64,
    cy: f64,
    r_outer: f64,
    r_inner: f64,
    points: u32,
    rotation_deg: f64,
) -> Result<BezPath, GeomError> {
    check_center(cx, cy, rotation_deg)?;
    positive("r_outer", r_outer)?;
    positive("r_inner", r_inner)?;
    if r_inner >= r_outer {
        return Err(invalid("r_inner musi być mniejszy niż r_outer"));
    }
    if !(2..=5_000).contains(&points) {
        return Err(invalid("liczba ramion: 2–5000"));
    }
    let n = points * 2;
    let pts: Vec<Point> = (0..n)
        .map(|i| {
            on_circle(
                cx,
                cy,
                if i % 2 == 0 { r_outer } else { r_inner },
                rotation_deg + 360.0 * i as f64 / n as f64,
            )
        })
        .collect();
    Ok(closed_polygon(&pts))
}

/// Podwójna gwiazda: między każdą parą ramion zewnętrznych (`r_outer`) leży
/// krótsze ramię (`r_middle`); wcięcia na `r_inner`. Daje 4·`points` wierzchołków.
pub fn double_star(
    cx: f64,
    cy: f64,
    r_outer: f64,
    r_middle: f64,
    r_inner: f64,
    points: u32,
    rotation_deg: f64,
) -> Result<BezPath, GeomError> {
    check_center(cx, cy, rotation_deg)?;
    for (n, v) in [
        ("r_outer", r_outer),
        ("r_middle", r_middle),
        ("r_inner", r_inner),
    ] {
        positive(n, v)?;
    }
    if !(r_inner < r_middle && r_middle < r_outer) {
        return Err(invalid("wymagane r_inner < r_middle < r_outer"));
    }
    if !(2..=2_500).contains(&points) {
        return Err(invalid("liczba ramion: 2–2500"));
    }
    let n = points * 4;
    let radii = [r_outer, r_inner, r_middle, r_inner];
    let pts: Vec<Point> = (0..n)
        .map(|i| {
            on_circle(
                cx,
                cy,
                radii[(i % 4) as usize],
                rotation_deg + 360.0 * i as f64 / n as f64,
            )
        })
        .collect();
    Ok(closed_polygon(&pts))
}

/// Łuk okręgu (otwarty): środek, promień, kąt początkowy i rozwartość w stopniach (CCW > 0).
pub fn arc(cx: f64, cy: f64, r: f64, start_deg: f64, sweep_deg: f64) -> Result<BezPath, GeomError> {
    check_center(cx, cy, start_deg)?;
    positive("r", r)?;
    if !sweep_deg.is_finite() || sweep_deg == 0.0 || sweep_deg.abs() > 360.0 {
        return Err(invalid("rozwartość łuku: różna od 0, |sweep| ≤ 360°"));
    }
    let a = kurbo::Arc::new(
        (cx, cy),
        (r, r),
        start_deg.to_radians(),
        sweep_deg.to_radians(),
        0.0,
    );
    let mut p = BezPath::new();
    p.move_to(on_circle(cx, cy, r, start_deg));
    for el in a.append_iter(crate::arc_tolerance(r) * 0.1) {
        p.push(el);
    }
    // koniec dokładnie na okręgu (bez błędu zaokrągleń aproksymacji)
    let end = on_circle(cx, cy, r, start_deg + sweep_deg);
    if let Some(PathEl::CurveTo(a1, a2, _)) = p.elements().last().copied() {
        p.pop();
        p.curve_to(a1, a2, end);
    }
    Ok(p)
}

/// Parametry łuku przez trzy punkty: (środek, promień, kąt startu, rozwartość) w stopniach.
/// `None`, gdy punkty są współliniowe.
pub fn arc_through(p1: Point, p2: Point, p3: Point) -> Option<(Point, f64, f64, f64)> {
    let cross = (p2 - p1).cross(p3 - p2);
    let scale = (p2 - p1).hypot() * (p3 - p2).hypot();
    if scale == 0.0 || cross.abs() < 1e-12 * scale {
        return None;
    }
    // środek okręgu opisanego
    let (ax, ay, bx, by, cx, cy) = (p1.x, p1.y, p2.x, p2.y, p3.x, p3.y);
    let d = 2.0 * (ax * (by - cy) + bx * (cy - ay) + cx * (ay - by));
    let a2 = ax * ax + ay * ay;
    let b2 = bx * bx + by * by;
    let c2 = cx * cx + cy * cy;
    let ux = (a2 * (by - cy) + b2 * (cy - ay) + c2 * (ay - by)) / d;
    let uy = (a2 * (cx - bx) + b2 * (ax - cx) + c2 * (bx - ax)) / d;
    let c = Point::new(ux, uy);
    let r = c.distance(p1);
    let ang = |p: Point| (p.y - c.y).atan2(p.x - c.x);
    let start = ang(p1);
    let mut sweep = ang(p3) - start;
    let tau = std::f64::consts::TAU;
    if cross > 0.0 {
        while sweep <= 0.0 {
            sweep += tau;
        }
    } else {
        while sweep >= 0.0 {
            sweep -= tau;
        }
    }
    let clean = |v: f64| if v.abs() < 1e-12 { 0.0 } else { v };
    Some((
        Point::new(clean(c.x), clean(c.y)),
        r,
        clean(start.to_degrees()),
        sweep.to_degrees(),
    ))
}

// ---------- konwersja do/z Clipper2 ----------

/// Wierzchołki podścieżek po spłaszczeniu; `closed` zgodnie z [`crate::is_closed`].
fn rings(p: &BezPath, tol: f64) -> Vec<(Vec<Point>, bool)> {
    crate::subpaths(p)
        .iter()
        .map(|sub| {
            let closed = crate::is_closed(sub);
            let mut pts: Vec<Point> = crate::flatten(sub, tol).into_iter().flatten().collect();
            pts.dedup_by(|a, b| a.distance(*b) < 1e-12);
            if closed && pts.len() > 1 && pts.first().unwrap().distance(*pts.last().unwrap()) < 1e-9
            {
                pts.pop();
            }
            (pts, closed)
        })
        .filter(|(pts, _)| !pts.is_empty())
        .collect()
}

fn to_cpaths(p: &BezPath) -> Result<CPaths, GeomError> {
    let mut out = Vec::new();
    for (pts, closed) in rings(p, BOOLEAN_TOLERANCE) {
        if !closed {
            return Err(GeomError::OpenPath);
        }
        if pts.len() >= 3 {
            out.push(
                pts.iter()
                    .map(|q| {
                        Point64::new((q.x * SCALE).round() as i64, (q.y * SCALE).round() as i64)
                    })
                    .collect::<CPath>(),
            );
        }
    }
    Ok(out)
}

/// Normalizuje jeden obiekt: even-odd wewnątrz obiektu, wynik zorientowany
/// (obrysy CCW, otwory CW), więc kolejne operacje mogą używać NonZero.
fn normalize(p: &BezPath) -> Result<CPaths, GeomError> {
    let cp = to_cpaths(p)?;
    if cp.is_empty() {
        return Ok(cp);
    }
    Ok(clipper2_rust::union_subjects_64(&cp, FillRule::EvenOdd))
}

fn normalize_all(ps: &[BezPath]) -> Result<CPaths, GeomError> {
    let mut all = Vec::new();
    for p in ps {
        all.extend(normalize(p)?);
    }
    Ok(all)
}

fn cpath_to_points(c: &CPath) -> Vec<Point> {
    c.iter()
        .map(|q| Point::new(q.x as f64 / SCALE, q.y as f64 / SCALE))
        .collect()
}

/// Grupuje wynik Clippera w kształty: obrys (pole > 0) z otworami (pole < 0) wewnątrz.
fn from_cpaths(c: &CPaths) -> Vec<BezPath> {
    let mut outers: Vec<(f64, CPath, Vec<CPath>)> = Vec::new();
    let mut holes = Vec::new();
    for p in c.iter() {
        if p.len() < 3 {
            continue;
        }
        let a = clipper2_rust::area(p);
        if a > 0.0 {
            outers.push((a, p.clone(), Vec::new()));
        } else if a < 0.0 {
            holes.push(p.clone());
        }
    }
    for h in holes {
        let probe = h[0];
        let host = outers
            .iter_mut()
            .filter(|(_, o, _)| {
                clipper2_rust::point_in_polygon(probe, o) != PointInPolygonResult::IsOutside
            })
            .min_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
        match host {
            Some((_, _, hs)) => hs.push(h),
            None => outers.push((0.0, h, Vec::new())),
        }
    }
    outers
        .into_iter()
        .map(|(_, o, hs)| {
            let mut p = closed_polygon(&cpath_to_points(&o));
            for h in hs {
                for el in closed_polygon(&cpath_to_points(&h)).elements() {
                    p.push(*el);
                }
            }
            p
        })
        .collect()
}

/// Boolean dwóch zbiorów kształtów zamkniętych. Każdy obiekt (element listy)
/// jest wypełniany regułą even-odd; obiekty łączą się sumą.
pub fn boolean(
    subject: &[BezPath],
    clip: &[BezPath],
    op: BoolOp,
) -> Result<Vec<BezPath>, GeomError> {
    let s = normalize_all(subject)?;
    let c = normalize_all(clip)?;
    if s.is_empty() && c.is_empty() {
        return Ok(Vec::new());
    }
    let fr = FillRule::NonZero;
    let res = match op {
        BoolOp::Union => clipper2_rust::union_64(&s, &c, fr),
        BoolOp::Difference => clipper2_rust::difference_64(&s, &c, fr),
        BoolOp::Intersection => clipper2_rust::intersect_64(&s, &c, fr),
        BoolOp::Xor => clipper2_rust::xor_64(&s, &c, fr),
    };
    Ok(from_cpaths(&res))
}

/// Offset kształtów zamkniętych: `delta` > 0 na zewnątrz, < 0 do wewnątrz.
pub fn offset(paths: &[BezPath], delta: f64, join: Join) -> Result<Vec<BezPath>, GeomError> {
    if !delta.is_finite() {
        return Err(invalid("delta musi być skończona"));
    }
    let base = clipper2_rust::union_subjects_64(&normalize_all(paths)?, FillRule::NonZero);
    if delta == 0.0 || base.is_empty() {
        return Ok(from_cpaths(&base));
    }
    let jt = match join {
        Join::Miter => JoinType::Miter,
        Join::Round => JoinType::Round,
        Join::Bevel => JoinType::Bevel,
    };
    let res = clipper2_rust::inflate_paths_64(
        &base,
        delta * SCALE,
        jt,
        EndType::Polygon,
        MITER_LIMIT,
        ARC_TOLERANCE * SCALE,
    );
    Ok(from_cpaths(&res))
}

/// Pole obszaru (każdy obiekt even-odd, obiekty sumowane) w mm². Ścieżki otwarte są pomijane.
pub fn area(paths: &[BezPath]) -> f64 {
    let closed: Vec<BezPath> = paths
        .iter()
        .map(|p| {
            let mut q = BezPath::new();
            for s in crate::subpaths(p).into_iter().filter(crate::is_closed) {
                for el in s.elements() {
                    q.push(*el);
                }
            }
            q
        })
        .collect();
    match boolean(&closed, &[], BoolOp::Union) {
        Ok(r) => r.iter().map(signed_area_polys).sum(),
        Err(_) => 0.0,
    }
}

fn signed_area_polys(p: &BezPath) -> f64 {
    rings(p, BOOLEAN_TOLERANCE)
        .iter()
        .map(|(pts, _)| {
            let n = pts.len();
            (0..n)
                .map(|i| pts[i].to_vec2().cross(pts[(i + 1) % n].to_vec2()))
                .sum::<f64>()
                / 2.0
        })
        .sum()
}

// ---------- operacje na ścieżkach ----------

/// Otoczka wypukła wszystkich punktów (po spłaszczeniu).
pub fn convex_hull(paths: &[BezPath]) -> Result<BezPath, GeomError> {
    let mut pts: Vec<Point> = paths
        .iter()
        .flat_map(|p| rings(p, BOOLEAN_TOLERANCE))
        .flat_map(|(v, _)| v)
        .collect();
    pts.sort_by(|a, b| {
        a.x.partial_cmp(&b.x)
            .unwrap()
            .then(a.y.partial_cmp(&b.y).unwrap())
    });
    pts.dedup_by(|a, b| a.distance(*b) < 1e-12);
    if pts.len() < 3 {
        return Err(invalid("otoczka wymaga co najmniej 3 punktów"));
    }
    let cross = |o: Point, a: Point, b: Point| (a - o).cross(b - o);
    let mut lower: Vec<Point> = Vec::new();
    for &p in &pts {
        while lower.len() >= 2 && cross(lower[lower.len() - 2], lower[lower.len() - 1], p) <= 0.0 {
            lower.pop();
        }
        lower.push(p);
    }
    let mut upper: Vec<Point> = Vec::new();
    for &p in pts.iter().rev() {
        while upper.len() >= 2 && cross(upper[upper.len() - 2], upper[upper.len() - 1], p) <= 0.0 {
            upper.pop();
        }
        upper.push(p);
    }
    lower.pop();
    upper.pop();
    lower.extend(upper);
    if lower.len() < 3 {
        return Err(invalid("punkty współliniowe — brak otoczki"));
    }
    Ok(closed_polygon(&lower))
}

/// Odwraca kierunek każdej podścieżki.
pub fn reverse(p: &BezPath) -> BezPath {
    p.reverse_subpaths()
}

fn push_seg(p: &mut BezPath, s: PathSeg) {
    match s {
        PathSeg::Line(l) => p.line_to(l.p1),
        PathSeg::Quad(q) => p.quad_to(q.p1, q.p2),
        PathSeg::Cubic(c) => p.curve_to(c.p1, c.p2, c.p3),
    }
}

/// Przesuwa punkt startowy zamkniętej, pojedynczej podścieżki na węzeł `node` (0 = obecny start).
pub fn set_start(p: &BezPath, node: usize) -> Result<BezPath, GeomError> {
    if crate::subpath_count(p) != 1 || !crate::is_closed(p) {
        return Err(invalid(
            "punkt startowy można zmienić tylko w pojedynczym zamkniętym kształcie",
        ));
    }
    let segs: Vec<PathSeg> = p
        .segments()
        .filter(|s| !matches!(s, PathSeg::Line(l) if l.p0 == l.p1))
        .collect();
    if node >= segs.len() {
        return Err(invalid(format!(
            "węzeł {node} poza zakresem 0–{}",
            segs.len().saturating_sub(1)
        )));
    }
    let mut out = BezPath::new();
    use kurbo::ParamCurve;
    out.move_to(segs[node].start());
    let n = segs.len();
    for k in 0..n {
        let s = segs[(node + k) % n];
        // ostatni odcinek prosty domyka ClosePath
        if k == n - 1 && matches!(s, PathSeg::Line(_)) {
            break;
        }
        push_seg(&mut out, s);
    }
    out.close_path();
    Ok(out)
}

/// Zamyka otwarte podścieżki, których końce są bliżej niż `tol`. Zwraca (ścieżka, liczba zamkniętych).
pub fn close_with_tolerance(p: &BezPath, tol: f64) -> (BezPath, usize) {
    let mut out = BezPath::new();
    let mut n = 0;
    for sub in crate::subpaths(p) {
        let explicit = matches!(sub.elements().last(), Some(PathEl::ClosePath));
        let ends = match (sub.elements().first(), sub.elements().last()) {
            (Some(PathEl::MoveTo(a)), Some(el)) if !explicit => el.end_point().map(|b| (*a, b)),
            _ => None,
        };
        for el in sub.elements() {
            out.push(*el);
        }
        if let Some((a, b)) = ends {
            if sub.elements().len() > 2 && a.distance(b) <= tol {
                out.close_path();
                n += 1;
            }
        }
    }
    (out, n)
}

fn endpoints(p: &BezPath) -> Option<(Point, Point)> {
    let a = match p.elements().first()? {
        PathEl::MoveTo(a) => *a,
        _ => return None,
    };
    let b = p.elements().iter().rev().find_map(|e| e.end_point())?;
    Some((a, b))
}

fn concat(a: &BezPath, b: &BezPath) -> BezPath {
    let mut out = a.clone();
    let (bs, _) = endpoints(b).unwrap();
    let (_, ae) = endpoints(a).unwrap();
    if ae != bs {
        out.line_to(bs);
    }
    for el in &b.elements()[1..] {
        out.push(*el);
    }
    out
}

/// Łączy otwarte ścieżki, których końce leżą bliżej niż `tol` (także po odwróceniu).
/// Łańcuch, którego końce się spotkały, zostaje zamknięty.
pub fn join_open(paths: &[BezPath], tol: f64) -> Vec<BezPath> {
    let mut closed = Vec::new();
    let mut open: Vec<BezPath> = Vec::new();
    for p in paths {
        for sub in crate::subpaths(p) {
            if crate::is_closed(&sub) || endpoints(&sub).is_none() {
                closed.push(sub);
            } else {
                open.push(sub);
            }
        }
    }
    let mut merged_any = vec![false; 0];
    loop {
        let mut found = None;
        'outer: for i in 0..open.len() {
            for j in 0..open.len() {
                if i == j {
                    continue;
                }
                let (si, ei) = endpoints(&open[i]).unwrap();
                let (sj, ej) = endpoints(&open[j]).unwrap();
                let cand = if ei.distance(sj) <= tol {
                    Some(concat(&open[i], &open[j]))
                } else if ei.distance(ej) <= tol {
                    Some(concat(&open[i], &open[j].reverse_subpaths()))
                } else if si.distance(sj) <= tol {
                    Some(concat(&open[i].reverse_subpaths(), &open[j]))
                } else {
                    None
                };
                if let Some(c) = cand {
                    found = Some((i, j, c));
                    break 'outer;
                }
            }
        }
        let Some((i, j, c)) = found else { break };
        let (lo, hi) = if i < j { (i, j) } else { (j, i) };
        open.remove(hi);
        open.remove(lo);
        open.push(c);
        merged_any.push(true);
    }
    for p in open.iter_mut() {
        let (s, e) = endpoints(p).unwrap();
        if !merged_any.is_empty() && s.distance(e) <= tol && p.elements().len() > 2 {
            p.close_path();
        }
    }
    closed.extend(open);
    closed
}

fn rdp(pts: &[Point], tol: f64, out: &mut Vec<Point>) {
    if pts.len() < 3 {
        return;
    }
    let (a, b) = (pts[0], *pts.last().unwrap());
    let ab = b - a;
    let len = ab.hypot();
    let (mut best, mut idx) = (0.0, 0);
    for (i, p) in pts.iter().enumerate().take(pts.len() - 1).skip(1) {
        let d = if len == 0.0 {
            p.distance(a)
        } else {
            ((*p - a).cross(ab) / len).abs()
        };
        if d > best {
            best = d;
            idx = i;
        }
    }
    if best > tol {
        rdp(&pts[..=idx], tol, out);
        out.push(pts[idx]);
        rdp(&pts[idx..], tol, out);
    }
}

/// Redukcja węzłów: spłaszczenie i uproszczenie Ramera–Douglasa–Peuckera z tolerancją `tol`.
pub fn reduce_nodes(p: &BezPath, tol: f64) -> BezPath {
    let mut out = BezPath::new();
    for (pts, closed) in rings(p, (tol * 0.25).max(1e-6)) {
        let mut src = pts.clone();
        if closed {
            src.push(pts[0]);
        }
        let mut keep = vec![src[0]];
        rdp(&src, tol, &mut keep);
        keep.push(*src.last().unwrap());
        if closed {
            keep.pop();
        }
        for (i, q) in keep.iter().enumerate() {
            if i == 0 {
                out.move_to(*q);
            } else {
                out.line_to(*q);
            }
        }
        if closed {
            out.close_path();
        }
    }
    out
}

/// Wierzchołki pojedynczej polilinii (bez krzywych).
fn polyline_vertices(p: &BezPath) -> Result<(Vec<Point>, bool), GeomError> {
    if crate::subpath_count(p) != 1 {
        return Err(invalid("operacja na narożniku wymaga pojedynczej ścieżki"));
    }
    if p.elements()
        .iter()
        .any(|e| matches!(e, PathEl::QuadTo(..) | PathEl::CurveTo(..)))
    {
        return Err(invalid("narożnik musi łączyć odcinki proste"));
    }
    let r = rings(p, BOOLEAN_TOLERANCE);
    let (pts, closed) = r
        .into_iter()
        .next()
        .ok_or_else(|| invalid("pusta ścieżka"))?;
    Ok((pts, closed))
}

enum Corner {
    Line(Point, Point),
    Arc(Point, Point, kurbo::Arc),
}

fn corner_geometry(
    pts: &[Point],
    closed: bool,
    node: usize,
) -> Result<(Point, Vec2, Vec2, f64, f64), GeomError> {
    let n = pts.len();
    if node >= n || (!closed && (node == 0 || node == n - 1)) {
        return Err(invalid(format!("węzeł {node} nie jest narożnikiem")));
    }
    let v = pts[node];
    let u = pts[(node + n - 1) % n];
    let w = pts[(node + 1) % n];
    let (du, dw) = (u - v, w - v);
    let (lu, lw) = (du.hypot(), dw.hypot());
    if lu == 0.0 || lw == 0.0 {
        return Err(invalid("zerowa długość odcinka przy narożniku"));
    }
    Ok((v, du / lu, dw / lw, lu, lw))
}

fn rebuild(pts: &[Point], closed: bool, node: usize, c: Corner) -> BezPath {
    let entry = |i: usize| match (&c, i == node) {
        (Corner::Line(a, _), true) | (Corner::Arc(a, _, _), true) => *a,
        _ => pts[i],
    };
    let exit = |i: usize| match (&c, i == node) {
        (Corner::Line(_, b), true) | (Corner::Arc(_, b, _), true) => *b,
        _ => pts[i],
    };
    let curve = |p: &mut BezPath, i: usize| {
        if i == node {
            match &c {
                Corner::Line(_, b) => p.line_to(*b),
                Corner::Arc(_, b, a) => {
                    for el in a.append_iter(crate::arc_tolerance(a.radii.x) * 0.1) {
                        p.push(el);
                    }
                    if let Some(PathEl::CurveTo(a1, a2, _)) = p.elements().last().copied() {
                        p.pop();
                        p.curve_to(a1, a2, *b);
                    }
                }
            }
        }
    };
    let mut p = BezPath::new();
    p.move_to(exit(0));
    for i in 1..pts.len() {
        p.line_to(entry(i));
        curve(&mut p, i);
    }
    if closed {
        if node == 0 {
            p.line_to(entry(0));
            curve(&mut p, 0);
        }
        p.close_path();
    }
    p
}

/// Fazowanie narożnika polilinii w węźle `node` na odległość `dist` od wierzchołka.
pub fn chamfer(p: &BezPath, node: usize, dist: f64) -> Result<BezPath, GeomError> {
    positive("dist", dist)?;
    let (pts, closed) = polyline_vertices(p)?;
    let (v, eu, ew, lu, lw) = corner_geometry(&pts, closed, node)?;
    if dist > lu / 2.0 + 1e-12 || dist > lw / 2.0 + 1e-12 {
        return Err(invalid(
            "faza nie może przekraczać połowy sąsiednich odcinków",
        ));
    }
    Ok(rebuild(
        &pts,
        closed,
        node,
        Corner::Line(v + eu * dist, v + ew * dist),
    ))
}

/// Zaokrąglenie narożnika polilinii w węźle `node` łukiem o promieniu `radius`.
pub fn fillet(p: &BezPath, node: usize, radius: f64) -> Result<BezPath, GeomError> {
    positive("radius", radius)?;
    let (pts, closed) = polyline_vertices(p)?;
    let (v, eu, ew, lu, lw) = corner_geometry(&pts, closed, node)?;
    let theta = eu.dot(ew).clamp(-1.0, 1.0).acos(); // kąt wewnętrzny narożnika
    if theta < 1e-9 || (std::f64::consts::PI - theta) < 1e-9 {
        return Err(invalid("narożnik zdegenerowany (odcinki współliniowe)"));
    }
    let t = radius / (theta / 2.0).tan();
    if t > lu / 2.0 + 1e-12 || t > lw / 2.0 + 1e-12 {
        return Err(invalid("promień zbyt duży dla sąsiednich odcinków"));
    }
    let t1 = v + eu * t;
    let t2 = v + ew * t;
    let bis = (eu + ew).normalize();
    let center = v + bis * (radius / (theta / 2.0).sin());
    let a1 = (t1 - center).atan2();
    let a2 = (t2 - center).atan2();
    let mut sweep = a2 - a1;
    while sweep > std::f64::consts::PI {
        sweep -= std::f64::consts::TAU;
    }
    while sweep < -std::f64::consts::PI {
        sweep += std::f64::consts::TAU;
    }
    let arc = kurbo::Arc::new(center, (radius, radius), a1, sweep, 0.0);
    Ok(rebuild(&pts, closed, node, Corner::Arc(t1, t2, arc)))
}

/// Czy dwie ścieżki są geometrycznie tożsame (te same punkty po spłaszczeniu, z tolerancją),
/// niezależnie od kierunku i punktu startowego.
pub fn same_geometry(a: &BezPath, b: &BezPath, tol: f64) -> bool {
    let ra = rings(a, BOOLEAN_TOLERANCE);
    let mut rb = rings(b, BOOLEAN_TOLERANCE);
    if ra.len() != rb.len() {
        return false;
    }
    let eq = |x: &[Point], y: &[Point]| x.iter().zip(y).all(|(p, q)| p.distance(*q) <= tol);
    'next: for (pa, ca) in &ra {
        for k in 0..rb.len() {
            let (pb, cb) = &rb[k];
            if ca != cb || pa.len() != pb.len() {
                continue;
            }
            let n = pa.len();
            let mut rev = pb.clone();
            rev.reverse();
            let matched = if *ca {
                (0..n).any(|s| {
                    let rot: Vec<Point> = (0..n).map(|i| pb[(i + s) % n]).collect();
                    let rot_r: Vec<Point> = (0..n).map(|i| rev[(i + s) % n]).collect();
                    eq(pa, &rot) || eq(pa, &rot_r)
                })
            } else {
                eq(pa, pb) || eq(pa, &rev)
            };
            if matched {
                rb.remove(k);
                continue 'next;
            }
        }
        return false;
    }
    true
}

#[cfg(test)]
#[path = "ops_tests.rs"]
mod tests;
