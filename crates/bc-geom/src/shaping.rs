//! Operacje kształtowania M1: okręgi styczne do trzech okręgów (problem
//! Apoloniusza), usuwanie nakładających się odcinków, wygładzanie, zamiana
//! odcinków na łuki, próbkowanie wzdłuż ścieżki, zmiana rozmiaru wpustów
//! i czopów oraz pakowanie prostokątów (Quick Nest).

use kurbo::{
    BezPath, CubicBez, Line, ParamCurve, ParamCurveArclen, ParamCurveDeriv, PathEl, PathSeg, Point,
    Vec2,
};

use crate::ops::{arc_through, GeomError};

// ---------- okręgi styczne ----------

/// Rodzaj styczności okręgu-wyniku z okręgiem danym.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tangency {
    /// Styczność zewnętrzna (okręgi leżą obok siebie).
    External,
    /// Wynik obejmuje okrąg dany.
    Encloses,
    /// Wynik leży wewnątrz okręgu danego.
    Inside,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TangentCircle {
    pub center: Point,
    pub r: f64,
    pub tangency: [Tangency; 3],
}

/// Wszystkie okręgi styczne do trzech danych okręgów (środek, promień > 0):
/// do 8 rozwiązań, posortowane po promieniu, potem X, potem Y. Rozwiązania
/// nieskończone (proste) są pomijane.
pub fn tangent_circles(given: [(Point, f64); 3]) -> Vec<TangentCircle> {
    // normalizacja: środek ciężkości w zerze, skala ~1 (lepsza dokładność)
    let o = Point::new(
        given.iter().map(|g| g.0.x).sum::<f64>() / 3.0,
        given.iter().map(|g| g.0.y).sum::<f64>() / 3.0,
    );
    let scale = given
        .iter()
        .map(|g| (g.0 - o).hypot() + g.1)
        .fold(0.0, f64::max);
    if !(scale.is_finite() && scale > 0.0) {
        return Vec::new();
    }
    let c: Vec<(f64, f64, f64)> = given
        .iter()
        .map(|g| ((g.0.x - o.x) / scale, (g.0.y - o.y) / scale, g.1 / scale))
        .collect();
    let k = |i: usize| c[i].0 * c[i].0 + c[i].1 * c[i].1 - c[i].2 * c[i].2;
    let mut found: Vec<(f64, f64, f64)> = Vec::new();
    for signs in 0..8u32 {
        let s = |i: usize| if signs & (1 << i) != 0 { -1.0 } else { 1.0 };
        // eq_i - eq_0: liniowe w (x, y, r)
        let row = |i: usize| {
            [
                2.0 * (c[i].0 - c[0].0),
                2.0 * (c[i].1 - c[0].1),
                2.0 * (s(i) * c[i].2 - s(0) * c[0].2),
            ]
        };
        let (m1, m2) = (row(1), row(2));
        let (b1, b2) = (k(1) - k(0), k(2) - k(0));
        let n = [
            m1[1] * m2[2] - m1[2] * m2[1],
            m1[2] * m2[0] - m1[0] * m2[2],
            m1[0] * m2[1] - m1[1] * m2[0],
        ];
        let nn = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
        if nn < 1e-12 {
            continue;
        }
        // rozwiązanie o najmniejszej normie: p = Mᵀ (M Mᵀ)⁻¹ b
        let dot = |a: [f64; 3], b: [f64; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
        let (g11, g12, g22) = (dot(m1, m1), dot(m1, m2), dot(m2, m2));
        let det = g11 * g22 - g12 * g12;
        if det.abs() < 1e-18 {
            continue;
        }
        let l1 = (g22 * b1 - g12 * b2) / det;
        let l2 = (g11 * b2 - g12 * b1) / det;
        let p = [
            m1[0] * l1 + m2[0] * l2,
            m1[1] * l1 + m2[1] * l2,
            m1[2] * l1 + m2[2] * l2,
        ];
        let n = [n[0] / nn, n[1] / nn, n[2] / nn];
        // podstawienie do eq_0
        let (dx, dy, dr) = (p[0] - c[0].0, p[1] - c[0].1, p[2] + s(0) * c[0].2);
        let a = n[0] * n[0] + n[1] * n[1] - n[2] * n[2];
        let b = 2.0 * (dx * n[0] + dy * n[1] - dr * n[2]);
        let cc = dx * dx + dy * dy - dr * dr;
        let mut ts = Vec::new();
        if a.abs() < 1e-12 {
            if b.abs() > 1e-12 {
                ts.push(-cc / b);
            }
        } else {
            let disc = b * b - 4.0 * a * cc;
            if disc >= -1e-14 {
                let sq = disc.max(0.0).sqrt();
                ts.push((-b + sq) / (2.0 * a));
                ts.push((-b - sq) / (2.0 * a));
            }
        }
        for t in ts {
            let (x, y, r) = (p[0] + t * n[0], p[1] + t * n[1], p[2] + t * n[2]);
            if !(r > 1e-9 && x.is_finite() && y.is_finite() && r < 1e6) {
                continue;
            }
            let tangent = c.iter().all(|g| {
                let d = ((x - g.0).powi(2) + (y - g.1).powi(2)).sqrt();
                (d - (r + g.2)).abs() < 1e-7 || (d - (r - g.2).abs()).abs() < 1e-7
            });
            let dup = found.iter().any(|f| {
                (f.0 - x).abs() < 1e-9 && (f.1 - y).abs() < 1e-9 && (f.2 - r).abs() < 1e-9
            });
            if tangent && !dup {
                found.push((x, y, r));
            }
        }
    }
    let mut out: Vec<TangentCircle> = found
        .into_iter()
        .map(|(x, y, r)| {
            let center = Point::new(o.x + x * scale, o.y + y * scale);
            let r = r * scale;
            let tangency = [0, 1, 2].map(|i| {
                let (gc, gr) = given[i];
                let d = center.distance(gc);
                if (d - (r + gr)).abs() <= (d - (r - gr).abs()).abs() {
                    Tangency::External
                } else if r > gr {
                    Tangency::Encloses
                } else {
                    Tangency::Inside
                }
            });
            TangentCircle {
                center,
                r,
                tangency,
            }
        })
        .collect();
    out.sort_by(|a, b| {
        (a.r, a.center.x, a.center.y)
            .partial_cmp(&(b.r, b.center.x, b.center.y))
            .unwrap()
    });
    out
}

// ---------- nakładające się odcinki ----------

fn seg_len(s: &PathSeg) -> f64 {
    s.arclen(1e-6)
}

fn same_curve(a: &PathSeg, b: &PathSeg, tol: f64) -> bool {
    let pts = |s: &PathSeg| -> Vec<Point> {
        let c = s.to_cubic();
        vec![c.p0, c.p1, c.p2, c.p3]
    };
    let (pa, pb) = (pts(a), pts(b));
    let fwd = pa.iter().zip(&pb).all(|(x, y)| x.distance(*y) <= tol);
    let rev = pa
        .iter()
        .zip(pb.iter().rev())
        .all(|(x, y)| x.distance(*y) <= tol);
    fwd || rev
}

/// Przedziały parametru odcinka `l` (0–1) pokryte współliniowym odcinkiem `by`.
fn covered(l: Line, by: Line, tol: f64) -> Option<(f64, f64)> {
    let len = (by.p1 - by.p0).hypot();
    let ll = (l.p1 - l.p0).hypot();
    if len < tol || ll < tol {
        return None;
    }
    let u = (by.p1 - by.p0) / len;
    if u.cross(l.p0 - by.p0).abs() > tol || u.cross(l.p1 - by.p0).abs() > tol {
        return None;
    }
    let (ta, tb) = (u.dot(l.p0 - by.p0), u.dot(l.p1 - by.p0));
    let lo = ta.min(tb).max(0.0);
    let hi = ta.max(tb).min(len);
    if hi - lo <= tol {
        return None;
    }
    let to_t = |v: f64| ((v - ta) / (tb - ta)).clamp(0.0, 1.0);
    let (a, b) = (to_t(lo), to_t(hi));
    Some((a.min(b), a.max(b)))
}

/// Łączy kawałki w łańcuchy (koniec = początek następnego).
fn chains(pieces: Vec<PathSeg>) -> Vec<Vec<PathSeg>> {
    let mut out: Vec<Vec<PathSeg>> = Vec::new();
    for p in pieces {
        match out.last_mut() {
            Some(ch) if ch.last().unwrap().end().distance(p.start()) < 1e-9 => ch.push(p),
            _ => out.push(vec![p]),
        }
    }
    out
}

fn push_seg(p: &mut BezPath, s: &PathSeg) {
    match s {
        PathSeg::Line(l) => p.line_to(l.p1),
        PathSeg::Quad(q) => p.quad_to(q.p1, q.p2),
        PathSeg::Cubic(c) => p.curve_to(c.p1, c.p2, c.p3),
    }
}

fn chain_path(out: &mut BezPath, ch: &[PathSeg]) {
    out.move_to(ch[0].start());
    for s in ch {
        push_seg(out, s);
    }
}

/// Usuwa fragmenty odcinków pokrywające się z wcześniejszymi (w kolejności listy,
/// także w obrębie jednej ścieżki). Odcinki prostoliniowe są przycinane do części
/// niepokrytych, krzywe usuwane tylko w całości (identyczne w dowolnym kierunku).
/// Zwraca nowe ścieżki (pusta = obiekt całkowicie pokryty) i usuniętą długość (mm).
pub fn remove_overlaps(paths: &[BezPath], tol: f64) -> (Vec<BezPath>, f64) {
    let mut accepted_lines: Vec<Line> = Vec::new();
    let mut accepted_curves: Vec<PathSeg> = Vec::new();
    let mut removed = 0.0;
    let mut out = Vec::new();
    for path in paths {
        let mut res = BezPath::new();
        for sub in crate::subpaths(path) {
            let closed = crate::is_closed(&sub);
            let mut pieces: Vec<PathSeg> = Vec::new();
            let mut intact = true;
            for seg in sub.segments() {
                match seg {
                    PathSeg::Line(l) => {
                        let ll = (l.p1 - l.p0).hypot();
                        if ll < 1e-12 {
                            continue;
                        }
                        let mut cov: Vec<(f64, f64)> = accepted_lines
                            .iter()
                            .filter_map(|a| covered(l, *a, tol))
                            .collect();
                        cov.sort_by(|a, b| a.partial_cmp(b).unwrap());
                        let mut t = 0.0;
                        let mut keep = Vec::new();
                        for (a, b) in cov {
                            if a > t {
                                keep.push((t, a));
                            }
                            t = t.max(b);
                        }
                        if t < 1.0 {
                            keep.push((t, 1.0));
                        }
                        keep.retain(|(a, b)| (b - a) * ll > tol);
                        let kept: f64 = keep.iter().map(|(a, b)| (b - a) * ll).sum();
                        if (kept - ll).abs() > 1e-12 {
                            intact = false;
                            removed += ll - kept;
                        }
                        for (a, b) in keep {
                            let (p, q) = (l.eval(a), l.eval(b));
                            let p = if a == 0.0 { l.p0 } else { p };
                            let q = if b == 1.0 { l.p1 } else { q };
                            pieces.push(PathSeg::Line(Line::new(p, q)));
                        }
                        accepted_lines.push(l);
                    }
                    curve => {
                        if accepted_curves.iter().any(|a| same_curve(a, &curve, tol)) {
                            intact = false;
                            removed += seg_len(&curve);
                        } else {
                            pieces.push(curve);
                            accepted_curves.push(curve);
                        }
                    }
                }
            }
            if intact {
                for el in sub.elements() {
                    res.push(*el);
                }
                continue;
            }
            let mut chs = chains(pieces);
            // zamknięta ścieżka: łańcuch kończący się w punkcie startowym łączy się z pierwszym
            if closed && chs.len() > 1 {
                let start = sub.segments().next().map(|s| s.start());
                let (first, last) = (chs[0][0].start(), chs.last().unwrap().last().unwrap().end());
                if start.is_some_and(|s| s.distance(first) < 1e-9 && s.distance(last) < 1e-9) {
                    let mut tail = chs.pop().unwrap();
                    tail.extend(chs.remove(0));
                    chs.insert(0, tail);
                }
            }
            for ch in chs {
                chain_path(&mut res, &ch);
            }
        }
        out.push(res);
    }
    (out, removed)
}

// ---------- wygładzanie ----------

fn dir_at_start(s: &PathSeg) -> Vec2 {
    let c = s.to_cubic();
    for v in [c.p1 - c.p0, c.p2 - c.p0, c.p3 - c.p0] {
        if v.hypot() > 1e-12 {
            return v.normalize();
        }
    }
    Vec2::ZERO
}

fn dir_at_end(s: &PathSeg) -> Vec2 {
    let c = s.to_cubic();
    for v in [c.p3 - c.p2, c.p3 - c.p1, c.p3 - c.p0] {
        if v.hypot() > 1e-12 {
            return v.normalize();
        }
    }
    Vec2::ZERO
}

/// Segmenty podścieżki bez odcinków zerowej długości i informacja o zamknięciu.
fn segs_of(sub: &BezPath) -> (Vec<PathSeg>, bool) {
    let closed = crate::is_closed(sub);
    let segs = sub
        .segments()
        .filter(|s| !matches!(s, PathSeg::Line(l) if (l.p1 - l.p0).hypot() < 1e-12))
        .collect();
    (segs, closed)
}

fn emit(out: &mut BezPath, segs: &[PathSeg], closed: bool) {
    if segs.is_empty() {
        return;
    }
    out.move_to(segs[0].start());
    for s in segs {
        push_seg(out, s);
    }
    if closed {
        out.close_path();
    }
}

/// Zamienia odcinki prostoliniowe na krzywe gładko przechodzące przez te same
/// węzły (styczna w węźle równoległa do cięciwy między sąsiadami). Węzły, w których
/// kierunek zmienia się o więcej niż `corner_deg`, oraz końce ścieżek otwartych
/// zostają narożnikami. `smoothness` (0–1] skaluje długość uchwytów.
pub fn smooth(p: &BezPath, smoothness: f64, corner_deg: f64) -> BezPath {
    let corner = corner_deg.to_radians();
    let mut out = BezPath::new();
    for sub in crate::subpaths(p) {
        let (segs, closed) = segs_of(&sub);
        let n = segs.len();
        if n == 0 {
            continue;
        }
        // styczna w węźle i (początek segmentu i); None = narożnik
        let tangent = |i: usize| -> Option<Vec2> {
            let prev = if i > 0 {
                Some(i - 1)
            } else if closed {
                Some(n - 1)
            } else {
                None
            };
            let next = if i < n {
                Some(i)
            } else if closed {
                Some(0)
            } else {
                None
            };
            let (pi, ni) = (prev?, next?);
            let (din, dout) = (dir_at_end(&segs[pi]), dir_at_start(&segs[ni]));
            let turn = din.cross(dout).atan2(din.dot(dout)).abs();
            if turn > corner {
                return None;
            }
            let chord = segs[ni].end() - segs[pi].start();
            let t = if chord.hypot() > 1e-12 {
                chord.normalize()
            } else {
                din + dout
            };
            (t.hypot() > 1e-12).then(|| t.normalize())
        };
        let new: Vec<PathSeg> = segs
            .iter()
            .enumerate()
            .map(|(i, s)| match s {
                PathSeg::Line(l) => {
                    let end_node = if i + 1 < n {
                        i + 1
                    } else if closed {
                        0
                    } else {
                        n
                    };
                    let h = (l.p1 - l.p0).hypot() / 3.0 * smoothness;
                    let (t0, t1) = (tangent(i), tangent(end_node));
                    if t0.is_none() && t1.is_none() {
                        *s
                    } else {
                        let c1 = t0.map_or(l.p0, |t| l.p0 + t * h);
                        let c2 = t1.map_or(l.p1, |t| l.p1 - t * h);
                        PathSeg::Cubic(CubicBez::new(l.p0, c1, c2, l.p1))
                    }
                }
                other => *other,
            })
            .collect();
        emit(&mut out, &new, closed);
    }
    out
}

// ---------- łuki z odcinków ----------

/// Czy punkty `pts` leżą na jednym łuku (≤ 180°) w tolerancji; zwraca łuk.
fn fit_arc(pts: &[Point], tol: f64) -> Option<kurbo::Arc> {
    let (a, m, b) = (pts[0], pts[pts.len() / 2], *pts.last().unwrap());
    let (c, r, start, sweep) = arc_through(a, m, b)?;
    if sweep.abs() > 180.0 + 1e-6 || r > 1e5 {
        return None;
    }
    let sw = sweep.to_radians();
    let st = start.to_radians();
    let mut prev = 0.0;
    for (i, q) in pts.iter().enumerate() {
        if (q.distance(c) - r).abs() > tol {
            return None;
        }
        if i == 0 {
            continue;
        }
        // kąt wzdłuż łuku, monotonicznie rosnący
        let ang = (q.y - c.y).atan2(q.x - c.x) - st;
        let mut rel = if sw > 0.0 { ang } else { -ang };
        rel = rel.rem_euclid(std::f64::consts::TAU);
        if i == pts.len() - 1 && rel < 1e-9 {
            rel = std::f64::consts::TAU;
        }
        if rel <= prev + 1e-12 || rel > sw.abs() + 1e-9 {
            return None;
        }
        prev = rel;
        // strzałka łuku nad cięciwą nie może przekroczyć tolerancji
        let half = pts[i - 1].distance(*q) / 2.0;
        if r - (r * r - half * half).max(0.0).sqrt() > tol {
            return None;
        }
    }
    Some(kurbo::Arc {
        center: c,
        radii: Vec2::new(r, r),
        start_angle: st,
        sweep_angle: sw,
        x_rotation: 0.0,
    })
}

fn arc_segs(arc: &kurbo::Arc, from: Point, to: Point, tol: f64) -> Vec<PathSeg> {
    let mut out = Vec::new();
    let mut last = from;
    for el in arc.append_iter(tol.max(crate::arc_tolerance(arc.radii.x))) {
        if let PathEl::CurveTo(a, b, c) = el {
            out.push(PathSeg::Cubic(CubicBez::new(last, a, b, c)));
            last = c;
        }
    }
    if let Some(PathSeg::Cubic(c)) = out.last_mut() {
        c.p3 = to;
    }
    out
}

/// Zastępuje ciągi co najmniej trzech odcinków leżących na okręgu (w tolerancji
/// `tol`, także strzałka nad każdym odcinkiem) łukami ≤ 180°; łuki są przybliżane krzywymi z dokładnością `tol`/10. Zwraca ścieżkę
/// i liczbę utworzonych łuków.
pub fn lines_to_arcs(p: &BezPath, tol: f64) -> (BezPath, usize) {
    let mut out = BezPath::new();
    let mut arcs = 0;
    for sub in crate::subpaths(p) {
        let (segs, closed) = segs_of(&sub);
        let mut new: Vec<PathSeg> = Vec::new();
        let mut i = 0;
        while i < segs.len() {
            if !matches!(segs[i], PathSeg::Line(_)) {
                new.push(segs[i]);
                i += 1;
                continue;
            }
            // ciąg odcinków od i
            let mut end = i;
            while end < segs.len() && matches!(segs[end], PathSeg::Line(_)) {
                end += 1;
            }
            let mut pts: Vec<Point> = segs[i..end].iter().map(|s| s.start()).collect();
            pts.push(segs[end - 1].end());
            let mut k = 0;
            let m = pts.len() - 1;
            while k < m {
                let mut best = None;
                let mut j = k + 3;
                while j <= m {
                    match fit_arc(&pts[k..=j], tol) {
                        Some(a) => best = Some((j, a)),
                        None => break,
                    }
                    j += 1;
                }
                match best {
                    Some((j, a)) => {
                        new.extend(arc_segs(&a, pts[k], pts[j], tol / 10.0));
                        arcs += 1;
                        k = j;
                    }
                    None => {
                        new.push(segs[i + k]);
                        k += 1;
                    }
                }
            }
            i = end;
        }
        emit(&mut out, &new, closed);
    }
    (out, arcs)
}

// ---------- próbkowanie wzdłuż ścieżki ----------

/// Punkt i kierunek (radiany) ścieżki w odległości `s` mm od początku
/// (kolejne segmenty wszystkich podścieżek liczone po kolei).
pub fn point_at_length(p: &BezPath, s: f64) -> Option<(Point, f64)> {
    let segs: Vec<PathSeg> = segs_all(p);
    let mut acc = 0.0;
    for (i, seg) in segs.iter().enumerate() {
        let l = seg_len(seg);
        if s <= acc + l || i == segs.len() - 1 {
            let t = if l > 0.0 {
                seg.inv_arclen((s - acc).clamp(0.0, l), 1e-9)
            } else {
                0.0
            };
            let d = seg.to_cubic().deriv().eval(t).to_vec2();
            let d = if d.hypot() > 1e-12 {
                d
            } else {
                seg.end() - seg.start()
            };
            return Some((seg.eval(t), d.atan2()));
        }
        acc += l;
    }
    None
}

fn segs_all(p: &BezPath) -> Vec<PathSeg> {
    crate::subpaths(p)
        .iter()
        .flat_map(|sub| segs_of(sub).0)
        .collect()
}

/// Długość ścieżki z pominięciem odcinków zerowych.
pub fn length(p: &BezPath) -> f64 {
    segs_all(p).iter().map(seg_len).sum()
}

// ---------- wpusty i czopy ----------

/// Wynik [`resize_slots`].
#[derive(Debug, Clone, PartialEq)]
pub struct SlotResize {
    pub path: BezPath,
    /// Zmienione wpusty (wycięcia w materiale: nacięcia krawędzi i otwory).
    pub slots: usize,
    /// Zmienione czopy (wypusty materiału).
    pub tabs: usize,
}

/// Parzysto-nieparzysta przynależność punktu do obszaru ścieżki.
fn inside(polys: &[Vec<Point>], q: Point) -> bool {
    let mut odd = false;
    for poly in polys {
        let n = poly.len();
        for i in 0..n {
            let (a, b) = (poly[i], poly[(i + 1) % n]);
            if (a.y > q.y) != (b.y > q.y) {
                let x = a.x + (q.y - a.y) / (b.y - a.y) * (b.x - a.x);
                if x > q.x {
                    odd = !odd;
                }
            }
        }
    }
    odd
}

/// Zmienia rozmiar wpustów i czopów zbudowanych z trzech odcinków (ramię, dno,
/// ramię; ramiona równoległe w przeciwnych kierunkach, dno prostopadłe):
/// * wpust (wnętrze litery U poza materiałem) o szerokości dna `from` ± `match_tol`
///   dostaje szerokość `to + tolerance`, symetrycznie względem osi;
/// * czop (wnętrze U w materiale) o ramionach długości `from` ± `match_tol`
///   dostaje długość `to` (przesuwa się dno czopa).
///
/// Obsługiwane są podścieżki złożone wyłącznie z odcinków; pozostałe bez zmian.
pub fn resize_slots(
    p: &BezPath,
    from: f64,
    to: f64,
    tolerance: f64,
    match_tol: f64,
) -> Result<SlotResize, GeomError> {
    let width = to + tolerance;
    if !(from > 0.0 && to > 0.0 && width > 0.0 && match_tol >= 0.0) {
        return Err(GeomError::Invalid(
            "from, to i to + tolerance muszą być > 0".into(),
        ));
    }
    let polys: Vec<Vec<Point>> = crate::flatten(p, 1e-3)
        .into_iter()
        .filter(|v| v.len() > 2)
        .collect();
    let (mut slots, mut tabs) = (0, 0);
    let mut out = BezPath::new();
    for sub in crate::subpaths(p) {
        let (segs, closed) = segs_of(&sub);
        if segs.is_empty() || !segs.iter().all(|s| matches!(s, PathSeg::Line(_))) {
            for el in sub.elements() {
                out.push(*el);
            }
            continue;
        }
        let n = segs.len();
        let v: Vec<Point> = segs.iter().map(|s| s.start()).collect();
        let end = segs[n - 1].end();
        let dir: Vec<Vec2> = segs
            .iter()
            .map(|s| (s.end() - s.start()).normalize())
            .collect();
        let len: Vec<f64> = segs.iter().map(|s| (s.end() - s.start()).hypot()).collect();
        let mut shift: Vec<Option<Vec2>> = vec![None; n];
        let mut seen: Vec<(usize, usize, bool)> = Vec::new();
        let count = if closed { n } else { n.saturating_sub(2) };
        let eps = 0.5f64.to_radians();
        for k in 0..count {
            let (a, b, c) = (k, (k + 1) % n, (k + 2) % n);
            if dir[a].dot(dir[b]).abs() > eps.sin() || dir[a].dot(dir[c]) > -eps.cos() {
                continue;
            }
            let center =
                segs[b].start().midpoint(segs[b].end()) + dir[c] * (len[a].min(len[c]) / 2.0);
            let tab = inside(&polys, center);
            let mut plan: Vec<(usize, Vec2)> = Vec::new();
            if !tab && (len[b] - from).abs() <= match_tol {
                let d = (width - len[b]) / 2.0;
                plan.push((a, -dir[b] * d));
                plan.push((c, dir[b] * d));
            } else if tab
                && (len[a] - from).abs() <= match_tol
                && (len[c] - from).abs() <= match_tol
            {
                let d = to - (len[a] + len[c]) / 2.0;
                plan.push((b, -dir[c] * d));
            } else {
                continue;
            }
            let conflict = plan
                .iter()
                .any(|(i, s)| shift[*i].is_some_and(|o| (o - *s).hypot() > 1e-9));
            if conflict {
                continue;
            }
            for (i, s) in plan {
                shift[i] = Some(s);
            }
            let key = (a.min(c), a.max(c), tab);
            if !seen.contains(&key) {
                seen.push(key);
                if tab {
                    tabs += 1;
                } else {
                    slots += 1;
                }
            }
        }
        // nowe wierzchołki: przecięcia przesuniętych prostych sąsiednich odcinków
        let moved = |i: usize| -> Point {
            let cur = if i < n { Some(i) } else { None };
            let prev = if i > 0 {
                Some(i - 1)
            } else if closed {
                Some(n - 1)
            } else {
                None
            };
            let base = if i < n { v[i] } else { end };
            let sc = cur.and_then(|c| shift[c]).unwrap_or(Vec2::ZERO);
            let sp = prev.and_then(|c| shift[c]).unwrap_or(Vec2::ZERO);
            match (prev, cur) {
                (Some(pi), Some(ci)) => {
                    let den = dir[ci].cross(dir[pi]);
                    if den.abs() > 1e-9 {
                        let t = (sp - sc).cross(dir[pi]) / den;
                        base + sc + dir[ci] * t
                    } else if sc != Vec2::ZERO {
                        base + sc
                    } else {
                        base + sp
                    }
                }
                (None, Some(_)) => base + sc,
                (Some(_), None) => base + sp,
                (None, None) => base,
            }
        };
        let total = if closed { n } else { n + 1 };
        for i in 0..total {
            let q = moved(i);
            if i == 0 {
                out.move_to(q);
            } else {
                out.line_to(q);
            }
        }
        if closed {
            out.close_path();
        }
    }
    Ok(SlotResize {
        path: out,
        slots,
        tabs,
    })
}

// ---------- Quick Nest ----------

/// Położenie prostokąta po pakowaniu: lewy dolny narożnik i obrót o 90°.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Placement {
    pub x: f64,
    pub y: f64,
    pub rotated: bool,
}

/// Ocena dopasowania: krótszy i dłuższy zapas boku, potem Y i X.
type Score = (f64, f64, f64, f64);

#[derive(Clone, Copy)]
struct Free {
    x: f64,
    y: f64,
    w: f64,
    h: f64,
}

/// Pakuje prostokąty (w, h) w arkusz `sheet_w × sheet_h` (od lewego dolnego
/// narożnika) metodą MaxRects z wyborem najlepiej dopasowanego krótszego boku.
/// Kolejność: malejące pole, potem indeks. `spacing` = odstęp między częściami
/// (nie od krawędzi arkusza). Wynik deterministyczny; `None` = nie mieści się.
pub fn pack_rects(
    sizes: &[(f64, f64)],
    sheet_w: f64,
    sheet_h: f64,
    spacing: f64,
    allow_rotation: bool,
) -> Vec<Option<Placement>> {
    let eps = 1e-9;
    let mut free = vec![Free {
        x: 0.0,
        y: 0.0,
        w: sheet_w + spacing,
        h: sheet_h + spacing,
    }];
    let mut order: Vec<usize> = (0..sizes.len()).collect();
    order.sort_by(|a, b| {
        let (pa, pb) = (sizes[*a].0 * sizes[*a].1, sizes[*b].0 * sizes[*b].1);
        pb.partial_cmp(&pa).unwrap().then(a.cmp(b))
    });
    let mut out = vec![None; sizes.len()];
    for i in order {
        let (w0, h0) = sizes[i];
        let mut best: Option<(Score, Free, bool)> = None;
        for rot in [false, true] {
            if rot && (!allow_rotation || (w0 - h0).abs() < eps) {
                continue;
            }
            let (w, h) = if rot {
                (h0 + spacing, w0 + spacing)
            } else {
                (w0 + spacing, h0 + spacing)
            };
            for f in &free {
                if w <= f.w + eps && h <= f.h + eps {
                    let (dw, dh) = (f.w - w, f.h - h);
                    let score = (dw.min(dh), dw.max(dh), f.y, f.x);
                    if best.as_ref().is_none_or(|b| score < b.0) {
                        best = Some((
                            score,
                            Free {
                                x: f.x,
                                y: f.y,
                                w,
                                h,
                            },
                            rot,
                        ));
                    }
                }
            }
        }
        let Some((_, used, rot)) = best else { continue };
        out[i] = Some(Placement {
            x: used.x,
            y: used.y,
            rotated: rot,
        });
        // podział wolnych prostokątów nachodzących na zajęty
        let mut next = Vec::new();
        for f in free {
            let overlap = used.x < f.x + f.w - eps
                && used.x + used.w > f.x + eps
                && used.y < f.y + f.h - eps
                && used.y + used.h > f.y + eps;
            if !overlap {
                next.push(f);
                continue;
            }
            if used.x > f.x + eps {
                next.push(Free {
                    w: used.x - f.x,
                    ..f
                });
            }
            if used.x + used.w < f.x + f.w - eps {
                next.push(Free {
                    x: used.x + used.w,
                    w: f.x + f.w - used.x - used.w,
                    ..f
                });
            }
            if used.y > f.y + eps {
                next.push(Free {
                    h: used.y - f.y,
                    ..f
                });
            }
            if used.y + used.h < f.y + f.h - eps {
                next.push(Free {
                    y: used.y + used.h,
                    h: f.y + f.h - used.y - used.h,
                    ..f
                });
            }
        }
        // usuń prostokąty zawarte w innych
        let contained = |a: &Free, b: &Free| {
            a.x >= b.x - eps
                && a.y >= b.y - eps
                && a.x + a.w <= b.x + b.w + eps
                && a.y + a.h <= b.y + b.h + eps
        };
        let mut pruned: Vec<Free> = Vec::new();
        for (k, a) in next.iter().enumerate() {
            let dominated = next
                .iter()
                .enumerate()
                .any(|(j, b)| j != k && contained(a, b) && !(contained(b, a) && j > k));
            if !dominated {
                pruned.push(*a);
            }
        }
        free = pruned;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn req_04_019_apollonius_equal_circles() {
        let g = [
            (Point::new(0., 0.), 1.0),
            (Point::new(10., 0.), 1.0),
            (Point::new(5., 8.), 1.0),
        ];
        let sol = tangent_circles(g);
        assert_eq!(sol.len(), 8);
        assert!(sol.windows(2).all(|w| w[0].r <= w[1].r));
    }

    #[test]
    fn req_04_044_fit_arc_on_circle() {
        let pts: Vec<Point> = (0..=6)
            .map(|i| {
                let a = (i as f64 * 5.0).to_radians();
                Point::new(20.0 * a.cos(), 20.0 * a.sin())
            })
            .collect();
        assert!(fit_arc(&pts, 0.05).is_some());
        let p = crate::polyline_path(&pts, false);
        assert_eq!(lines_to_arcs(&p, 0.05).1, 1);
        let ring: Vec<Point> = (0..72)
            .map(|i| {
                let a = (i as f64 * 5.0).to_radians();
                Point::new(100.0 + 20.0 * a.cos(), 100.0 + 20.0 * a.sin())
            })
            .collect();
        let p = crate::polyline_path(&ring, true);
        assert_eq!(lines_to_arcs(&p, 0.05).1, 2);
    }

    #[test]
    fn req_04_095_pack_rects_fills_sheet() {
        let r = pack_rects(&[(50., 50.); 4], 100., 100., 0., true);
        assert!(r.iter().all(Option::is_some));
        let r = pack_rects(&[(20., 90.)], 100., 30., 0., false);
        assert_eq!(r, vec![None]);
    }
}
