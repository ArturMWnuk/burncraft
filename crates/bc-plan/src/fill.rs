//! Wypełnienia skanowaniem: Fill (razem / grupy / osobno), Flood Fill, kąt,
//! cross-hatch, interwał, overscan, scan offset, pasy rastra (§7.1, §7.2, REQ-08-015).

use bc_doc::{FillGrouping, Kinematics, ObjectId, Overscan};
use bc_geom::ops::{BoolOp, Join};
use bc_geom::{Affine, BezPath, Point, Vec2};

use crate::collect::Item;
use crate::job::Move;
use crate::poly::{Poly, EPS};
use crate::spec::Spec;
use crate::vector::{split_closed, TOL};
use crate::PlanError;

/// Linia skanowania w układzie obróconym: y i odcinki [x0, x1].
type Line = (f64, Vec<(f64, f64)>);

/// Jeden kierunek kreskowania: pasy, w pasie linie rosnąco po y.
#[derive(Debug, Clone)]
pub(crate) struct Hatch {
    pub angle: f64,
    pub bands: Vec<Vec<Line>>,
}

/// Grupa wypełniana razem.
#[derive(Debug, Clone)]
pub(crate) struct FillGroup {
    pub hatches: Vec<Hatch>,
    pub order: usize,
}

/// Grupy wypełnienia warstwy; drugi element = obiekty z otwartymi kształtami (pomijane).
pub(crate) fn groups(
    items: &[Item],
    spec: &Spec,
    flood: bool,
    band: f64,
) -> Result<(Vec<FillGroup>, Vec<ObjectId>), PlanError> {
    let p = &spec.params;
    let mut open_ids = Vec::new();
    let mut shapes: Vec<(usize, BezPath)> = Vec::new(); // (indeks itemu, zamknięte)
    for (i, it) in items.iter().enumerate() {
        let (closed, open) = split_closed(&it.path);
        if !open.is_empty() {
            open_ids.push(it.id);
        }
        if !closed.elements().is_empty() {
            shapes.push((i, closed));
        }
    }
    // klucz grupy -> kształty
    let mut sets: Vec<(usize, Vec<usize>)> = Vec::new();
    for (k, (i, _)) in shapes.iter().enumerate() {
        let key = match (flood, p.fill_grouping) {
            (true, _) | (false, FillGrouping::All) => 0,
            (false, FillGrouping::Groups) => items[*i].group,
            (false, FillGrouping::Each) => *i,
        };
        match sets.iter_mut().find(|(g, _)| *g == key) {
            Some((_, v)) => v.push(k),
            None => sets.push((key, vec![k])),
        }
    }
    let geom = |e: bc_geom::ops::GeomError| PlanError::Invalid(format!("wypełnienie: {e}"));
    let mut out = Vec::new();
    for (_, members) in sets {
        let paths: Vec<BezPath> = members.iter().map(|&k| shapes[k].1.clone()).collect();
        let order = members
            .iter()
            .map(|&k| items[shapes[k].0].order)
            .min()
            .unwrap_or(0);
        let mut region = bc_geom::ops::boolean(&paths, &[], BoolOp::Union).map_err(geom)?;
        if p.fill_kerf != 0.0 && !region.is_empty() {
            region = bc_geom::ops::offset(&region, -p.fill_kerf, Join::Miter).map_err(geom)?;
        }
        let regions: Vec<Vec<BezPath>> = if flood {
            region.into_iter().map(|r| vec![r]).collect()
        } else {
            vec![region]
        };
        for r in regions {
            let polys: Vec<Poly> = r
                .iter()
                .flat_map(|q| Poly::from_path(q, TOL))
                .filter(|q| q.closed)
                .collect();
            if polys.is_empty() {
                continue;
            }
            let mut angles = vec![p.scan_angle];
            if p.cross_hatch {
                angles.push(p.scan_angle + 90.0);
            }
            let hatches: Vec<Hatch> = angles
                .into_iter()
                .map(|a| Hatch {
                    angle: a,
                    bands: scan(&polys, a, p.interval, band),
                })
                .filter(|h| h.bands.iter().any(|b| !b.is_empty()))
                .collect();
            if !hatches.is_empty() {
                out.push(FillGroup { hatches, order });
            }
        }
    }
    Ok((out, open_ids))
}

fn rot(deg: f64) -> Affine {
    Affine::rotate(deg.to_radians())
}

/// Linie skanowania wielokątów (even-odd) pod kątem `angle`, co `interval`,
/// podzielone na pasy szerokości `band` (0 = jeden pas).
fn scan(polys: &[Poly], angle: f64, interval: f64, band: f64) -> Vec<Vec<Line>> {
    let r = rot(-angle);
    let rp: Vec<Vec<Point>> = polys
        .iter()
        .map(|p| p.pts.iter().map(|q| r * *q).collect())
        .collect();
    let (mut ymin, mut ymax) = (f64::MAX, f64::MIN);
    for p in &rp {
        for q in p {
            ymin = ymin.min(q.y);
            ymax = ymax.max(q.y);
        }
    }
    let mut lines: Vec<Line> = Vec::new();
    let k0 = (ymin / interval - 0.5).ceil() as i64;
    let mut k = k0;
    loop {
        let y = (k as f64 + 0.5) * interval;
        if y > ymax {
            break;
        }
        k += 1;
        if y < ymin {
            continue;
        }
        let mut xs: Vec<f64> = Vec::new();
        for p in &rp {
            let n = p.len();
            for i in 0..n {
                let (a, b) = (p[i], p[(i + 1) % n]);
                if (a.y <= y) != (b.y <= y) {
                    xs.push(a.x + (y - a.y) * (b.x - a.x) / (b.y - a.y));
                }
            }
        }
        xs.sort_by(f64::total_cmp);
        let spans: Vec<(f64, f64)> = xs
            .as_chunks::<2>()
            .0
            .iter()
            .map(|c| (c[0], c[1]))
            .filter(|(a, b)| b - a > EPS)
            .collect();
        if !spans.is_empty() {
            lines.push((y, spans));
        }
    }
    if band <= 0.0 {
        return vec![lines];
    }
    // pasy: podział odcinków na granicach k·band
    let mut bands: std::collections::BTreeMap<i64, Vec<Line>> = Default::default();
    for (y, spans) in lines {
        let mut per: std::collections::BTreeMap<i64, Vec<(f64, f64)>> = Default::default();
        for (a, b) in spans {
            let mut x = a;
            while x < b - EPS {
                let bi = (x / band + EPS).floor() as i64;
                let end = ((bi + 1) as f64 * band).min(b);
                per.entry(bi).or_default().push((x, end));
                x = end;
            }
        }
        for (bi, s) in per {
            bands.entry(bi).or_default().push((y, s));
        }
    }
    bands.into_values().collect()
}

/// Długość overscanu w kierunku skanowania (REQ-07-047/048, REQ-08-018).
pub(crate) fn overscan(spec: &Spec, angle: f64, k: &Kinematics) -> f64 {
    match spec.params.overscan {
        Overscan::Off => 0.0,
        Overscan::Percent { percent } => percent / 100.0 * spec.speed,
        Overscan::Auto => {
            let u = Vec2::new(angle.to_radians().cos(), angle.to_radians().sin());
            let (v, a) = axis_limits(u, spec.speed, k);
            v * v / (2.0 * a)
        }
    }
}

/// Prędkość i przyspieszenie dopuszczalne w kierunku `u` (osobno X i Y).
pub(crate) fn axis_limits(u: Vec2, feed: f64, k: &Kinematics) -> (f64, f64) {
    let mut v = feed;
    let mut a = f64::MAX;
    if u.x.abs() > 1e-9 {
        v = v.min(k.max_speed_x / u.x.abs());
        a = a.min(k.accel_x / u.x.abs());
    }
    if u.y.abs() > 1e-9 {
        v = v.min(k.max_speed_y / u.y.abs());
        a = a.min(k.accel_y / u.y.abs());
    }
    if a == f64::MAX {
        a = k.accel_x.min(k.accel_y);
    }
    (v, a)
}

/// Punkt wejścia grupy (początek pierwszej linii, bez overscanu).
pub(crate) fn entry(g: &FillGroup) -> Point {
    let h = &g.hatches[0];
    let line = h.bands.iter().find(|b| !b.is_empty()).map(|b| &b[0]);
    match line {
        Some((y, spans)) => rot(h.angle) * Point::new(spans[0].0, *y),
        None => Point::ORIGIN,
    }
}

/// Ruchy wypełnienia grupy; zwraca pozycję końcową.
pub(crate) fn emit(
    g: &FillGroup,
    spec: &Spec,
    k: &Kinematics,
    cur: Point,
    out: &mut Vec<Move>,
) -> Point {
    let p = &spec.params;
    let offset = p.scan_offset_at(spec.speed);
    let mut at = cur;
    for h in &g.hatches {
        let r = rot(h.angle);
        let d = overscan(spec, h.angle, k);
        for lines in &h.bands {
            for (li, (y, spans)) in lines.iter().enumerate() {
                let fwd = !p.bidirectional || li % 2 == 0;
                let segs: Vec<(f64, f64)> = if fwd {
                    spans.clone()
                } else {
                    spans
                        .iter()
                        .rev()
                        .map(|(a, b)| (b + offset, a + offset))
                        .collect()
                };
                let dir = if fwd { 1.0 } else { -1.0 };
                let w = |x: f64| r * Point::new(x, *y);
                let start = w(segs[0].0 - d * dir);
                if at.distance(start) > EPS {
                    out.push(Move::Travel {
                        to: start,
                        lift: false,
                    });
                }
                let cut = |to: Point, power: f64, out: &mut Vec<Move>| {
                    out.push(Move::Cut {
                        to,
                        power,
                        speed: spec.speed,
                    })
                };
                if d > 0.0 {
                    cut(w(segs[0].0), 0.0, out);
                }
                for (i, (a, b)) in segs.iter().enumerate() {
                    if i > 0 {
                        cut(w(*a), 0.0, out);
                    }
                    cut(w(*b), spec.power_max, out);
                }
                let mut end = w(segs[segs.len() - 1].1);
                if d > 0.0 {
                    end = w(segs[segs.len() - 1].1 + d * dir);
                    cut(end, 0.0, out);
                }
                at = end;
            }
        }
    }
    at
}
