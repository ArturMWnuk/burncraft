//! Tryb Line (i pierścienie Offset Fill): kerf, mostki, lead-in/out, overcut,
//! perforacja, tryb punktowy i rampy mocy (ADR 0009 §4, kroki 3, 5, 6, 9).

use bc_doc::{LayerParams, LeadKind, ObjectId};
use bc_geom::ops::Join;
use bc_geom::{BezPath, Point, Vec2};

use crate::collect::Item;
use crate::job::Move;
use crate::poly::{polyline_len, split_polyline, Poly, EPS};
use crate::spec::Spec;
use crate::PlanError;

/// Tolerancja spłaszczania krzywych (ADR 0009 §1).
pub(crate) const TOL: f64 = bc_geom::DEFAULT_TOLERANCE;

/// Część do wycięcia: jeden kontur albo kawałki konturu między mostkami.
#[derive(Debug, Clone)]
pub(crate) struct Part {
    pub polys: Vec<Poly>,
    /// Jeden kontur zamknięty bez mostków: można zmienić start i kierunek; dostaje lead-in/out i overcut.
    pub free: bool,
    pub id: ObjectId,
    pub group: usize,
    pub priority: i32,
    pub order: usize,
    /// Zagnieżdżenie w innych konturach zamkniętych warstwy (0 = zewnętrzny).
    pub depth: u32,
    /// Indeks konturu, z którego powstała część (do relacji zawierania).
    pub contour: usize,
}

/// Kontury warstwy po kerfie, z zagnieżdżeniem.
#[derive(Debug, Clone)]
pub(crate) struct Contour {
    pub poly: Poly,
    pub item: usize,
    pub depth: u32,
    /// Pole bez znaku (zamknięty) albo 0.
    pub area: f64,
}

/// Kerf X/Y (ADR 0009 §4.3): przy różnych wartościach skalowanie Y o kx/ky,
/// offset o kx i skalowanie odwrotne.
pub(crate) fn kerf_offset(closed: &[BezPath], kx: f64, ky: f64) -> Result<Vec<BezPath>, PlanError> {
    let geom = |e: bc_geom::ops::GeomError| PlanError::Invalid(format!("kerf: {e}"));
    if (kx - ky).abs() < 1e-12 {
        return bc_geom::ops::offset(closed, kx, Join::Miter).map_err(geom);
    }
    let fix = |v: f64, other: f64| {
        if v.abs() < 1e-4 {
            1e-4f64.copysign(other)
        } else {
            v
        }
    };
    let (kx, ky) = (fix(kx, ky), fix(ky, kx));
    if kx.signum() != ky.signum() {
        return Err(PlanError::Invalid(
            "kerf_x i kerf_y muszą mieć ten sam znak".into(),
        ));
    }
    let s = kx / ky;
    let scaled: Vec<BezPath> = closed
        .iter()
        .map(|p| {
            let mut q = p.clone();
            q.apply_affine(bc_geom::Affine::scale_non_uniform(1.0, s));
            q
        })
        .collect();
    let mut out = bc_geom::ops::offset(&scaled, kx, Join::Miter).map_err(geom)?;
    for p in &mut out {
        p.apply_affine(bc_geom::Affine::scale_non_uniform(1.0, 1.0 / s));
    }
    Ok(out)
}

/// Rozdziela ścieżkę na podścieżki zamknięte i otwarte.
pub(crate) fn split_closed(p: &BezPath) -> (BezPath, Vec<BezPath>) {
    let mut closed = BezPath::new();
    let mut open = Vec::new();
    for s in bc_geom::subpaths(p) {
        if s.elements().len() < 2 {
            continue;
        }
        if bc_geom::is_closed(&s) {
            for el in s.elements() {
                closed.push(*el);
            }
        } else {
            open.push(s);
        }
    }
    (closed, open)
}

/// Kontury warstwy (po kerfie) z zagnieżdżeniem.
pub(crate) fn contours(items: &[Item], p: &LayerParams) -> Result<Vec<Contour>, PlanError> {
    let kx = p.kerf_x.unwrap_or(p.kerf);
    let ky = p.kerf_y.unwrap_or(p.kerf);
    let mut out: Vec<Contour> = Vec::new();
    for (ii, it) in items.iter().enumerate() {
        let (closed, open) = split_closed(&it.path);
        let mut paths: Vec<BezPath> = Vec::new();
        if !closed.elements().is_empty() {
            if kx != 0.0 || ky != 0.0 {
                paths.extend(kerf_offset(&[closed], kx, ky)?);
            } else {
                paths.push(closed);
            }
        }
        paths.extend(open);
        for path in &paths {
            for poly in Poly::from_path(path, TOL) {
                let area = poly.signed_area().abs();
                out.push(Contour {
                    poly,
                    item: ii,
                    depth: 0,
                    area,
                });
            }
        }
    }
    nest(&mut out);
    Ok(out)
}

fn inside_bbox(a: &[f64; 4], b: &[f64; 4]) -> bool {
    a[0] >= b[0] - EPS && a[1] >= b[1] - EPS && a[2] <= b[2] + EPS && a[3] <= b[3] + EPS
}

/// Czy kontur `outer` zawiera kontur `inner`.
pub(crate) fn contains(outer: &Contour, ob: &[f64; 4], inner: &Contour, ib: &[f64; 4]) -> bool {
    outer.poly.closed
        && outer.area > inner.area + EPS
        && inside_bbox(ib, ob)
        && inner
            .poly
            .pts
            .iter()
            .take(4)
            .all(|q| outer.poly.contains(*q))
}

fn nest(cs: &mut [Contour]) {
    let boxes: Vec<[f64; 4]> = cs.iter().map(|c| c.poly.bbox()).collect();
    // większe najpierw: kontur może być zawarty tylko w większym
    let mut idx: Vec<usize> = (0..cs.len()).collect();
    idx.sort_by(|&a, &b| cs[b].area.total_cmp(&cs[a].area).then(a.cmp(&b)));
    let mut depth = vec![0u32; cs.len()];
    for (k, &i) in idx.iter().enumerate() {
        for &j in &idx[..k] {
            if contains(&cs[j], &boxes[j], &cs[i], &boxes[i]) {
                depth[i] += 1;
            }
        }
    }
    for (c, d) in cs.iter_mut().zip(depth) {
        c.depth = d;
    }
}

/// Części do wycięcia z mostkami ręcznymi i automatycznymi (REQ-07-031…034, REQ-04-083/084).
pub(crate) fn parts(items: &[Item], cs: &[Contour], p: &LayerParams) -> Vec<Part> {
    let mut out = Vec::new();
    for (ci, c) in cs.iter().enumerate() {
        let it = &items[c.item];
        let len = c.poly.length();
        if len < EPS {
            continue;
        }
        let mut gaps: Vec<(f64, f64)> = Vec::new();
        // ręczne: na najbliższym konturze obiektu
        for (at, size) in &it.tabs {
            let best = cs
                .iter()
                .enumerate()
                .filter(|(_, o)| o.item == c.item)
                .map(|(k, o)| (k, o.poly.project(*at)))
                .min_by(|a, b| a.1 .1.total_cmp(&b.1 .1).then(a.0.cmp(&b.0)));
            if let Some((k, (s, _))) = best {
                if k == ci {
                    let w = size.unwrap_or(p.tabs.size);
                    gaps.push((s - w / 2.0, s + w / 2.0));
                }
            }
        }
        if p.tabs.enabled && c.poly.closed && !(p.tabs.skip_inner && c.depth > 0) {
            let n = if p.tabs.count > 0 {
                p.tabs.count as usize
            } else {
                ((len / p.tabs.spacing).floor() as usize).max(1)
            };
            for k in 0..n {
                let s = (k as f64 + 0.5) * len / n as f64;
                gaps.push((s - p.tabs.size / 2.0, s + p.tabs.size / 2.0));
            }
        }
        let base = Part {
            polys: Vec::new(),
            free: false,
            id: it.id,
            group: it.group,
            priority: it.priority,
            order: it.order,
            depth: c.depth,
            contour: ci,
        };
        if gaps.is_empty() {
            out.push(Part {
                polys: vec![c.poly.clone()],
                free: c.poly.closed,
                ..base
            });
            continue;
        }
        let pieces = cut_gaps(&c.poly, len, gaps);
        if pieces.is_empty() {
            continue;
        }
        out.push(Part {
            polys: pieces,
            ..base
        });
    }
    out
}

/// Kawałki konturu poza przerwami.
fn cut_gaps(poly: &Poly, len: f64, mut gaps: Vec<(f64, f64)>) -> Vec<Poly> {
    let mut keep: Vec<(f64, f64)> = Vec::new();
    if poly.closed {
        for g in &mut gaps {
            let shift = (g.0 / len).floor() * len;
            g.0 -= shift;
            g.1 -= shift;
        }
        gaps.sort_by(|a, b| a.0.total_cmp(&b.0));
        // scal nakładające się przerwy
        let mut merged: Vec<(f64, f64)> = Vec::new();
        for g in gaps {
            match merged.last_mut() {
                Some(m) if g.0 <= m.1 + EPS => m.1 = m.1.max(g.1),
                _ => merged.push(g),
            }
        }
        if merged.len() > 1 {
            let first = merged[0];
            let last = merged.last_mut().unwrap();
            if last.1 >= first.0 + len - EPS {
                last.1 = last.1.max(first.1 + len);
                merged.remove(0);
            }
        }
        if merged.last().is_some_and(|g| g.1 - g.0 >= len - EPS) {
            return Vec::new();
        }
        for k in 0..merged.len() {
            let a = merged[k].1;
            let b = if k + 1 < merged.len() {
                merged[k + 1].0
            } else {
                merged[0].0 + len
            };
            if b > a + EPS {
                keep.push((a, b));
            }
        }
    } else {
        gaps.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut at = 0.0;
        for (a, b) in gaps {
            let a = a.clamp(0.0, len);
            let b = b.clamp(0.0, len);
            if a > at + EPS {
                keep.push((at, a));
            }
            at = at.max(b);
        }
        if len > at + EPS {
            keep.push((at, len));
        }
    }
    keep.into_iter()
        .map(|(a, b)| Poly::new(poly.sub(a, b), false))
        .filter(|p| p.pts.len() >= 2)
        .collect()
}

/// Normalna po stronie odpadu dla kierunku `t` (kontur zewnętrzny: na zewnątrz,
/// otwór: do wnętrza otworu).
fn waste_normal(t: Vec2, ccw: bool, depth: u32) -> Vec2 {
    let right = Vec2::new(t.y, -t.x);
    let outside = if ccw { right } else { -right };
    if depth.is_multiple_of(2) {
        outside
    } else {
        -outside
    }
}

fn lead_points(
    kind: LeadKind,
    len: f64,
    angle: f64,
    at: Point,
    t: Vec2,
    n: Vec2,
    inward: bool,
) -> Vec<Point> {
    let a = angle.to_radians();
    match kind {
        LeadKind::None => Vec::new(),
        LeadKind::Line => {
            if inward {
                vec![at + (-t * a.cos() + n * a.sin()) * len]
            } else {
                vec![at + (t * a.cos() + n * a.sin()) * len]
            }
        }
        LeadKind::Arc => {
            let c = at + n * len;
            let steps = ((angle.abs() / 5.0).ceil() as usize).max(4);
            let pt = |phi: f64| c + (-n * phi.cos() + t * phi.sin()) * len;
            if inward {
                // od −kąt do 0 (bez punktu 0 = początek konturu)
                (0..steps)
                    .map(|i| pt(-a + a * i as f64 / steps as f64))
                    .collect()
            } else {
                (1..=steps)
                    .map(|i| pt(a * i as f64 / steps as f64))
                    .collect()
            }
        }
    }
}

/// Łamana do wycięcia dla kawałka części: lead-in + kontur + overcut + lead-out.
pub(crate) fn part_polyline(poly: &Poly, free: bool, depth: u32, p: &LayerParams) -> Vec<Point> {
    let mut pts = poly.walk();
    if !(free && poly.closed) {
        return pts;
    }
    let ccw = poly.signed_area() > 0.0;
    if p.overcut > 0.0 {
        let extra = poly.sub(0.0, p.overcut.min(poly.length() * 2.0));
        pts.extend(extra.into_iter().skip(1));
    }
    let mut out = Vec::new();
    if p.lead_in.kind != LeadKind::None && pts.len() >= 2 {
        let t = (pts[1] - pts[0]).normalize();
        let n = waste_normal(t, ccw, depth);
        out.extend(lead_points(
            p.lead_in.kind,
            p.lead_in.length,
            p.lead_in.angle,
            pts[0],
            t,
            n,
            true,
        ));
    }
    out.extend_from_slice(&pts);
    if p.lead_out.kind != LeadKind::None && pts.len() >= 2 {
        let k = pts.len();
        let t = (pts[k - 1] - pts[k - 2]).normalize();
        let n = waste_normal(t, ccw, depth);
        out.extend(lead_points(
            p.lead_out.kind,
            p.lead_out.length,
            p.lead_out.angle,
            pts[k - 1],
            t,
            n,
            false,
        ));
    }
    out
}

/// Rozwinięcie mocy łamanej: tryb punktowy, perforacja, rampa (REQ-07-039…041).
/// Dodaje ruchy (z przejazdem do początku) i zwraca pozycję końcową.
pub(crate) fn expand(
    pts: &[Point],
    spec: &Spec,
    ramp: bool,
    cur: Point,
    out: &mut Vec<Move>,
) -> Point {
    if pts.len() < 2 {
        return cur;
    }
    let p = &spec.params;
    let (pmax, pmin, speed) = (spec.power_max, spec.power_min, spec.speed);
    let total = polyline_len(pts);
    let closed = pts[0].distance(pts[pts.len() - 1]) < 1e-6;
    let mut at = cur;
    let travel = |to: Point, at: &mut Point, out: &mut Vec<Move>| {
        if at.distance(to) > EPS {
            out.push(Move::Travel { to, lift: false });
            *at = to;
        }
    };
    if let Some(d) = p.dot_mode {
        let mut s = 0.0;
        while s < total - if closed { EPS } else { -EPS } {
            let q = point_on(pts, s);
            travel(q, &mut at, out);
            out.push(Move::Dwell {
                s: d.dwell_ms / 1000.0,
                power: pmax,
            });
            s += d.spacing;
        }
        return at;
    }
    let mut cuts: Vec<f64> = Vec::new();
    if let Some(perf) = p.perforation {
        let period = perf.cut + perf.gap;
        let mut s = 0.0;
        while s < total {
            cuts.push(s + perf.cut);
            cuts.push(s + period);
            s += period;
        }
    }
    let ramp_len = match (ramp, p.ramp) {
        (true, Some(r)) => r.length.min(total / 2.0),
        _ => 0.0,
    };
    const STEPS: usize = 8;
    if ramp_len > 0.0 {
        for i in 1..=STEPS {
            cuts.push(ramp_len * i as f64 / STEPS as f64);
            cuts.push(total - ramp_len * i as f64 / STEPS as f64);
        }
    }
    cuts.retain(|s| *s > EPS && *s < total - EPS);
    cuts.sort_by(f64::total_cmp);
    cuts.dedup_by(|a, b| (*a - *b).abs() < EPS);
    travel(pts[0], &mut at, out);
    let mut s0 = 0.0;
    for piece in split_polyline(pts, &cuts) {
        let l = polyline_len(&piece);
        let mid = s0 + l / 2.0;
        s0 += l;
        let on = match p.perforation {
            Some(perf) => mid.rem_euclid(perf.cut + perf.gap) < perf.cut,
            None => true,
        };
        let power = if !on {
            0.0
        } else if ramp_len > 0.0 {
            let step = |s: f64| ((s / ramp_len * STEPS as f64).floor() + 0.5) / STEPS as f64;
            let f = step(mid).min(step(total - mid)).min(1.0);
            pmin + (pmax - pmin) * f
        } else {
            pmax
        };
        for q in piece.into_iter().skip(1) {
            out.push(Move::Cut {
                to: q,
                power,
                speed,
            });
            at = q;
        }
    }
    at
}

fn point_on(pts: &[Point], s: f64) -> Point {
    let mut acc = 0.0;
    for w in pts.windows(2) {
        let l = w[0].distance(w[1]);
        if acc + l >= s - EPS && l > EPS {
            return w[0] + (w[1] - w[0]) * ((s - acc) / l).clamp(0.0, 1.0);
        }
        acc += l;
    }
    pts[pts.len() - 1]
}

/// Pierścienie Offset Fill (REQ-07-006): co interwał od połowy interwału do środka.
pub(crate) fn offset_rings(items: &[Item], interval: f64) -> Result<Vec<Part>, PlanError> {
    let mut out = Vec::new();
    for it in items {
        let (closed, _) = split_closed(&it.path);
        if closed.elements().is_empty() {
            continue;
        }
        let mut polys = Vec::new();
        let mut k = 0usize;
        loop {
            let d = interval / 2.0 + k as f64 * interval;
            let rings = bc_geom::ops::offset(std::slice::from_ref(&closed), -d, Join::Miter)
                .map_err(|e| PlanError::Invalid(format!("offset fill: {e}")))?;
            let mut any = false;
            for r in &rings {
                for poly in Poly::from_path(r, TOL) {
                    if poly.closed && poly.length() > EPS {
                        any = true;
                        polys.push(poly);
                    }
                }
            }
            if !any || k > 100_000 {
                break;
            }
            k += 1;
        }
        if polys.is_empty() {
            continue;
        }
        out.push(Part {
            polys,
            free: false,
            id: it.id,
            group: it.group,
            priority: it.priority,
            order: it.order,
            depth: 0,
            contour: usize::MAX,
        });
    }
    Ok(out)
}
