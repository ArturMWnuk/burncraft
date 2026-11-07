//! Polilinie (po spłaszczeniu krzywych) i operacje na nich: długość, punkt
//! na długości, wycinki, zawieranie, pole.

use bc_geom::{BezPath, Point, Vec2};

pub(crate) const EPS: f64 = 1e-9;

/// Polilinia. Zamknięta nie powtarza pierwszego punktu na końcu.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Poly {
    pub pts: Vec<Point>,
    pub closed: bool,
}

impl Poly {
    pub fn new(mut pts: Vec<Point>, closed: bool) -> Self {
        pts.dedup_by(|a, b| a.distance(*b) < EPS);
        let mut closed = closed;
        if pts.len() > 1 && pts[0].distance(pts[pts.len() - 1]) < 1e-6 {
            pts.pop();
            closed = true;
        }
        if pts.len() < 3 {
            closed = false;
        }
        Poly { pts, closed }
    }

    /// Polilinie ścieżki (po jednej na podścieżkę).
    pub fn from_path(p: &BezPath, tol: f64) -> Vec<Poly> {
        bc_geom::flatten(p, tol)
            .into_iter()
            .map(|v| Poly::new(v, false))
            .filter(|p| p.pts.len() >= 2)
            .collect()
    }

    /// Kolejne odcinki (dla zamkniętej z domknięciem).
    pub fn segments(&self) -> impl Iterator<Item = (Point, Point)> + '_ {
        let n = self.pts.len();
        let m = if self.closed { n } else { n.saturating_sub(1) };
        (0..m).map(move |i| (self.pts[i], self.pts[(i + 1) % n]))
    }

    pub fn length(&self) -> f64 {
        self.segments().map(|(a, b)| a.distance(b)).sum()
    }

    /// Punkty w kolejności przejścia (zamknięta: z powrotem do pierwszego).
    pub fn walk(&self) -> Vec<Point> {
        let mut v = self.pts.clone();
        if self.closed {
            v.push(self.pts[0]);
        }
        v
    }

    /// Pole ze znakiem (CCW > 0); otwarta = 0.
    pub fn signed_area(&self) -> f64 {
        if !self.closed {
            return 0.0;
        }
        self.segments()
            .map(|(a, b)| a.to_vec2().cross(b.to_vec2()))
            .sum::<f64>()
            / 2.0
    }

    pub fn bbox(&self) -> [f64; 4] {
        let mut b = [f64::MAX, f64::MAX, f64::MIN, f64::MIN];
        for p in &self.pts {
            b[0] = b[0].min(p.x);
            b[1] = b[1].min(p.y);
            b[2] = b[2].max(p.x);
            b[3] = b[3].max(p.y);
        }
        b
    }

    /// Punkt wewnątrz (even-odd); tylko dla zamkniętej.
    pub fn contains(&self, q: Point) -> bool {
        if !self.closed {
            return false;
        }
        let mut inside = false;
        for (a, b) in self.segments() {
            if (a.y > q.y) != (b.y > q.y) {
                let x = a.x + (q.y - a.y) * (b.x - a.x) / (b.y - a.y);
                if x > q.x {
                    inside = !inside;
                }
            }
        }
        inside
    }

    pub fn reversed(&self) -> Poly {
        let mut pts = self.pts.clone();
        if self.closed {
            pts[1..].reverse();
        } else {
            pts.reverse();
        }
        Poly {
            pts,
            closed: self.closed,
        }
    }

    /// Zamknięta zaczynająca się od wierzchołka `i`.
    pub fn rotated(&self, i: usize) -> Poly {
        let mut pts = self.pts.clone();
        pts.rotate_left(i % self.pts.len().max(1));
        Poly {
            pts,
            closed: self.closed,
        }
    }

    /// Punkt na długości `s` od początku, indeks odcinka i kierunek (jednostkowy).
    pub fn point_at(&self, s: f64) -> (Point, usize, Vec2) {
        let mut acc = 0.0;
        let mut last = (self.pts[0], 0, Vec2::new(1.0, 0.0));
        for (i, (a, b)) in self.segments().enumerate() {
            let l = a.distance(b);
            if l < EPS {
                continue;
            }
            let dir = (b - a) / l;
            last = (b, i, dir);
            if acc + l >= s - EPS {
                let t = ((s - acc) / l).clamp(0.0, 1.0);
                return (a + (b - a) * t, i, dir);
            }
            acc += l;
        }
        last
    }

    /// Fragment od długości `s0` do `s1` (zamknięta: `s1` może przekroczyć długość — zawija).
    pub fn sub(&self, s0: f64, s1: f64) -> Vec<Point> {
        let total = self.length();
        let mut out = Vec::new();
        if s1 <= s0 + EPS {
            return out;
        }
        let laps = if self.closed { 2 } else { 1 };
        let mut acc = 0.0;
        let segs: Vec<(Point, Point)> = self.segments().collect();
        for _ in 0..laps {
            for &(a, b) in &segs {
                let l = a.distance(b);
                let (lo, hi) = (acc, acc + l);
                acc = hi;
                if l < EPS || hi < s0 - EPS || lo > s1 + EPS {
                    continue;
                }
                let pa = a + (b - a) * ((s0 - lo) / l).clamp(0.0, 1.0);
                let pb = a + (b - a) * ((s1 - lo) / l).clamp(0.0, 1.0);
                if out.is_empty() {
                    out.push(pa);
                }
                if out.last().is_none_or(|q: &Point| q.distance(pb) > EPS) {
                    out.push(pb);
                }
            }
            if acc >= s1 || !self.closed || total <= EPS {
                break;
            }
        }
        out
    }

    /// Najbliższe położenie (długość od początku) punktu `q` na polilinii i odległość.
    pub fn project(&self, q: Point) -> (f64, f64) {
        let mut best = (0.0, f64::MAX);
        let mut acc = 0.0;
        for (a, b) in self.segments() {
            let ab = b - a;
            let l2 = ab.hypot2();
            let t = if l2 < EPS {
                0.0
            } else {
                ((q - a).dot(ab) / l2).clamp(0.0, 1.0)
            };
            let p = a + ab * t;
            let d = p.distance(q);
            if d < best.1 - 1e-12 {
                best = (acc + t * l2.sqrt(), d);
            }
            acc += l2.sqrt();
        }
        best
    }

    /// Indeks wierzchołka najbliższego `q`.
    pub fn nearest_vertex(&self, q: Point) -> usize {
        let mut best = (0, f64::MAX);
        for (i, p) in self.pts.iter().enumerate() {
            let d = p.distance(q);
            if d < best.1 - 1e-12 {
                best = (i, d);
            }
        }
        best.0
    }
}

/// Długość łamanej.
pub(crate) fn polyline_len(pts: &[Point]) -> f64 {
    pts.windows(2).map(|w| w[0].distance(w[1])).sum()
}

/// Dzieli łamaną w podanych długościach (rosnąco); zwraca kawałki.
pub(crate) fn split_polyline(pts: &[Point], cuts: &[f64]) -> Vec<Vec<Point>> {
    let mut out = Vec::new();
    let mut cur = vec![pts[0]];
    let mut acc = 0.0;
    let mut ci = 0;
    for w in pts.windows(2) {
        let (a, b) = (w[0], w[1]);
        let l = a.distance(b);
        while ci < cuts.len() && cuts[ci] <= acc + EPS {
            // cięcie dokładnie w wierzchołku
            if cur.len() > 1 && (cuts[ci] - acc).abs() <= EPS {
                out.push(std::mem::replace(&mut cur, vec![a]));
            }
            ci += 1;
        }
        while ci < cuts.len() && cuts[ci] < acc + l - EPS {
            let p = a + (b - a) * ((cuts[ci] - acc) / l);
            cur.push(p);
            out.push(std::mem::replace(&mut cur, vec![p]));
            ci += 1;
        }
        cur.push(b);
        acc += l;
    }
    if cur.len() > 1 {
        out.push(cur);
    }
    out
}

/// Scala współliniowe kolejne odcinki (REQ-08-012).
pub(crate) fn merge_collinear(pts: &[Point]) -> Vec<Point> {
    if pts.len() < 3 {
        return pts.to_vec();
    }
    let mut out = vec![pts[0]];
    for i in 1..pts.len() - 1 {
        let a = *out.last().unwrap();
        let b = pts[i];
        let c = pts[i + 1];
        let ab = b - a;
        let bc = c - b;
        let collinear = ab.cross(bc).abs() <= 1e-9 * ab.hypot().max(1.0) * bc.hypot().max(1.0)
            && ab.dot(bc) > 0.0;
        if !collinear {
            out.push(b);
        }
    }
    out.push(pts[pts.len() - 1]);
    out
}
