//! Przycinanie do przecięć, gumka i wypaczanie siatką (M1.4b2b,
//! REQ-04-021, REQ-04-034, REQ-04-045…047).

use clipper2_rust::{ClipType, Clipper64, EndType, FillRule, JoinType, Paths64, Point64};
use kurbo::{BezPath, Line, ParamCurve, ParamCurveNearest, PathEl, PathSeg, Point, Rect};

use crate::ops::{GeomError, BOOLEAN_TOLERANCE};

/// Skala całkowitoliczbowa Clippera (jak w `ops`): 1 jednostka = 0,1 µm.
const SCALE: f64 = 10_000.0;

/// Liczba kawałków, na które dzielony jest każdy segment przy wypaczaniu.
const WARP_PIECES: usize = 8;

fn invalid(m: impl Into<String>) -> GeomError {
    GeomError::Invalid(m.into())
}

// ---------- przycinanie ----------

struct Sub {
    segs: Vec<PathSeg>,
    closed: bool,
    path: BezPath,
}

fn subs(p: &BezPath) -> Vec<Sub> {
    crate::subpaths(p)
        .into_iter()
        .map(|path| Sub {
            segs: path
                .segments()
                .filter(|s| !matches!(s, PathSeg::Line(l) if l.p0 == l.p1))
                .collect(),
            closed: crate::is_closed(&path),
            path,
        })
        .filter(|s| !s.segs.is_empty())
        .collect()
}

fn push_seg(out: &mut BezPath, s: PathSeg) {
    match s {
        PathSeg::Line(l) => out.line_to(l.p1),
        PathSeg::Quad(q) => out.quad_to(q.p1, q.p2),
        PathSeg::Cubic(c) => out.curve_to(c.p1, c.p2, c.p3),
    }
}

/// Fragment konturu od pozycji `a` do `b` (pozycja = indeks segmentu + t;
/// `b` może przekraczać liczbę segmentów w konturze zamkniętym).
fn piece(segs: &[PathSeg], a: f64, b: f64, out: &mut BezPath) {
    let n = segs.len();
    let mut started = false;
    let mut k = a.floor() as usize;
    while (k as f64) < b - 1e-12 {
        let r0 = (a - k as f64).max(0.0);
        let r1 = (b - k as f64).min(1.0);
        if r1 - r0 > 1e-12 {
            let s = segs[k % n].subsegment(r0..r1);
            if !started {
                out.move_to(s.start());
                started = true;
            }
            push_seg(out, s);
        }
        k += 1;
    }
}

/// Usuwa fragment ścieżki między najbliższymi przecięciami wokół punktu `at`
/// (przecięcia z `cutters` i z innymi konturami samej ścieżki). Krzywe zostają
/// krzywymi; brak przecięć = błąd.
pub fn trim(target: &BezPath, cutters: &[BezPath], at: Point) -> Result<BezPath, GeomError> {
    let ss = subs(target);
    // kontur i pozycja najbliższe kliknięciu
    let mut best: Option<(f64, usize, f64)> = None;
    for (si, s) in ss.iter().enumerate() {
        for (i, seg) in s.segs.iter().enumerate() {
            let n = seg.nearest(at, 1e-9);
            if best.is_none_or(|b| n.distance_sq < b.0) {
                best = Some((n.distance_sq, si, i as f64 + n.t));
            }
        }
    }
    let (_, si, click) = best.ok_or_else(|| invalid("ścieżka bez segmentów"))?;
    // odcinki tnące: inne obiekty i pozostałe kontury tej ścieżki
    let mut lines = Vec::new();
    let others = ss
        .iter()
        .enumerate()
        .filter(|(j, _)| *j != si)
        .map(|(_, s)| &s.path);
    for c in cutters.iter().chain(others) {
        for poly in crate::flatten(c, BOOLEAN_TOLERANCE) {
            lines.extend(poly.windows(2).map(|w| Line::new(w[0], w[1])));
        }
    }
    let s = &ss[si];
    let n = s.segs.len() as f64;
    let mut hits: Vec<f64> = Vec::new();
    for (i, seg) in s.segs.iter().enumerate() {
        for l in &lines {
            for x in seg.intersect_line(*l) {
                if (-1e-9..=1.0 + 1e-9).contains(&x.line_t) {
                    hits.push(i as f64 + x.segment_t.clamp(0.0, 1.0));
                }
            }
        }
    }
    hits.sort_by(f64::total_cmp);
    hits.dedup_by(|a, b| (*a - *b).abs() < 1e-7);
    if s.closed && hits.len() >= 2 {
        // pozycja n to ta sama co 0
        if (hits[hits.len() - 1] - n).abs() < 1e-7 && hits[0].abs() < 1e-7 {
            hits.pop();
        }
    }
    let before = hits.iter().copied().rev().find(|h| *h < click);
    let after = hits.iter().copied().find(|h| *h > click);
    let mut out = BezPath::new();
    for (j, o) in ss.iter().enumerate() {
        if j != si {
            out.extend(o.path.iter());
        }
    }
    if s.closed {
        if hits.len() < 2 {
            return Err(invalid(
                "kształt zamknięty trzeba przeciąć w co najmniej dwóch miejscach",
            ));
        }
        let b = before.unwrap_or(hits[hits.len() - 1] - n);
        let a = after.unwrap_or(hits[0] + n);
        // zostaje część od `a` dookoła do `b`
        piece(&s.segs, a, b + n, &mut out);
    } else {
        if hits.iter().all(|h| *h <= 1e-9 || *h >= n - 1e-9) {
            return Err(invalid("brak przecięć z innymi kształtami"));
        }
        if let Some(b) = before.filter(|b| *b > 1e-9) {
            piece(&s.segs, 0.0, b, &mut out);
        }
        if let Some(a) = after.filter(|a| *a < n - 1e-9) {
            piece(&s.segs, a, n, &mut out);
        }
    }
    Ok(out)
}

// ---------- gumka ----------

fn to_c(p: Point) -> Point64 {
    Point64::new((p.x * SCALE).round() as i64, (p.y * SCALE).round() as i64)
}

/// Gumka: usuwa części ścieżki pod pociągnięciem `stroke` o szerokości `width`.
/// Wynik to polilinie otwarte (pusta ścieżka = wszystko wymazane).
pub fn erase(target: &BezPath, stroke: &[Point], width: f64) -> Result<BezPath, GeomError> {
    if !(width.is_finite() && width > 0.0) {
        return Err(invalid("szerokość gumki musi być dodatnia"));
    }
    if stroke.is_empty() || stroke.iter().any(|p| !p.x.is_finite() || !p.y.is_finite()) {
        return Err(invalid("pociągnięcie gumki wymaga skończonych punktów"));
    }
    let mut st: Vec<Point64> = stroke.iter().map(|p| to_c(*p)).collect();
    if st.len() == 1 {
        st.push(st[0]);
    }
    let region = clipper2_rust::inflate_paths_64(
        &vec![st],
        width / 2.0 * SCALE,
        JoinType::Round,
        EndType::Round,
        2.0,
        0.0005 * SCALE,
    );
    let mut subjects: Paths64 = Vec::new();
    for sub in crate::subpaths(target) {
        let closed = crate::is_closed(&sub);
        for mut pts in crate::flatten(&sub, BOOLEAN_TOLERANCE) {
            pts.dedup_by(|a, b| a.distance(*b) < 1e-12);
            if closed && pts.len() > 1 && pts[0].distance(pts[pts.len() - 1]) > 1e-9 {
                pts.push(pts[0]); // kontur zamknięty jako polilinia z bokiem powrotnym
            }
            if pts.len() >= 2 {
                subjects.push(pts.into_iter().map(to_c).collect());
            }
        }
    }
    let mut c = Clipper64::new();
    c.add_open_subject(&subjects);
    c.add_clip(&region);
    let mut closed_out = Vec::new();
    let mut open_out = Vec::new();
    if !c.execute(
        ClipType::Difference,
        FillRule::NonZero,
        &mut closed_out,
        Some(&mut open_out),
    ) {
        return Err(GeomError::Clipper(
            "gumka: operacja Clippera nie powiodła się".into(),
        ));
    }
    let pts: Vec<Vec<Point>> = open_out
        .iter()
        .map(|p| {
            p.iter()
                .map(|q| Point::new(q.x as f64 / SCALE, q.y as f64 / SCALE))
                .collect()
        })
        .collect();
    let mut out = BezPath::new();
    for poly in pts.iter().filter(|p| p.len() >= 2) {
        out.extend(crate::polyline_path(poly, false).iter());
    }
    Ok(out)
}

// ---------- wypaczanie ----------

struct Mesh<'a> {
    b: Rect,
    cols: usize,
    rows: usize,
    g: &'a [Point],
}

impl Mesh<'_> {
    fn at(&self, i: usize, j: usize) -> Point {
        self.g[j * (self.cols + 1) + i]
    }

    /// Odwzorowanie biliniowe w komórce zawierającej punkt (poza siatką — ekstrapolacja skrajnej komórki).
    fn map(&self, p: Point) -> Point {
        let u = (p.x - self.b.x0) / self.b.width() * self.cols as f64;
        let v = (p.y - self.b.y0) / self.b.height() * self.rows as f64;
        let i = (u.floor().max(0.0) as usize).min(self.cols - 1);
        let j = (v.floor().max(0.0) as usize).min(self.rows - 1);
        let (fu, fv) = (u - i as f64, v - j as f64);
        let (a, b, c, d) = (
            self.at(i, j),
            self.at(i + 1, j),
            self.at(i, j + 1),
            self.at(i + 1, j + 1),
        );
        let x = (1.0 - fu) * (1.0 - fv) * a.x
            + fu * (1.0 - fv) * b.x
            + (1.0 - fu) * fv * c.x
            + fu * fv * d.x;
        let y = (1.0 - fu) * (1.0 - fv) * a.y
            + fu * (1.0 - fv) * b.y
            + (1.0 - fu) * fv * c.y
            + fu * fv * d.y;
        Point::new(x, y)
    }
}

/// Wypaczanie siatką: równomierna siatka `cols × rows` komórek na `bounds`
/// przechodzi w punkty `grid` (wierszami od dołu, (cols+1)·(rows+1) punktów);
/// w komórce odwzorowanie biliniowe. 1×1 = wypaczanie 4-punktowe.
pub fn mesh_warp(
    p: &BezPath,
    bounds: Rect,
    cols: usize,
    rows: usize,
    grid: &[Point],
) -> Result<BezPath, GeomError> {
    if cols == 0 || rows == 0 || cols > 64 || rows > 64 {
        return Err(invalid("siatka: 1–64 kolumn i wierszy"));
    }
    if grid.len() != (cols + 1) * (rows + 1) {
        return Err(invalid(format!(
            "siatka {cols}×{rows} wymaga {} punktów, podano {}",
            (cols + 1) * (rows + 1),
            grid.len()
        )));
    }
    if !(bounds.width() > 1e-9 && bounds.height() > 1e-9) {
        return Err(invalid(
            "obszar wypaczania musi mieć dodatnią szerokość i wysokość",
        ));
    }
    if grid.iter().any(|q| !q.x.is_finite() || !q.y.is_finite()) {
        return Err(invalid("punkty siatki muszą być skończone"));
    }
    let m = Mesh {
        b: bounds,
        cols,
        rows,
        g: grid,
    };
    // każdy segment (także niejawny bok zamykający) dzielony na kawałki,
    // których punkty kontrolne przechodzą przez odwzorowanie
    let warp_seg = |out: &mut BezPath, seg: PathSeg| {
        for k in 0..WARP_PIECES {
            let t0 = k as f64 / WARP_PIECES as f64;
            let t1 = (k + 1) as f64 / WARP_PIECES as f64;
            let c = seg.subsegment(t0..t1).to_cubic();
            out.curve_to(m.map(c.p1), m.map(c.p2), m.map(c.p3));
        }
    };
    let mut out = BezPath::new();
    let mut last = Point::ZERO;
    let mut start = Point::ZERO;
    for el in p.elements() {
        let seg = match *el {
            PathEl::MoveTo(q) => {
                out.move_to(m.map(q));
                last = q;
                start = q;
                continue;
            }
            PathEl::ClosePath => {
                if last.distance(start) > 1e-12 {
                    warp_seg(&mut out, PathSeg::Line(Line::new(last, start)));
                }
                out.close_path();
                last = start;
                continue;
            }
            PathEl::LineTo(b) => PathSeg::Line(Line::new(last, b)),
            PathEl::QuadTo(c, b) => PathSeg::Quad(kurbo::QuadBez::new(last, c, b)),
            PathEl::CurveTo(c1, c2, b) => PathSeg::Cubic(kurbo::CubicBez::new(last, c1, c2, b)),
        };
        last = seg.end();
        warp_seg(&mut out, seg);
    }
    Ok(out)
}

#[cfg(test)]
#[path = "cut_tests.rs"]
mod tests;
