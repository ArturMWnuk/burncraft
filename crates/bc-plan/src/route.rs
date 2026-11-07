//! Przejazdy i strefy zakazane (REQ-08-008…010): objazd przeszkód po grafie
//! widoczności narożników poszerzonych prostokątów, wycinanie cięcia ze stref
//! zakazanych dla wiązki.

use bc_geom::Point;

use crate::poly::{Poly, EPS};

/// Margines objazdu strefy zakazanej, mm.
const ZONE_MARGIN: f64 = 0.5;
/// Margines objazdu wyciętej części, mm.
const PART_MARGIN: f64 = 1.0;

/// Przeszkoda dla przejazdów.
#[derive(Debug, Clone)]
pub(crate) enum Obstacle {
    Zone([f64; 4]),
    Part(Poly, [f64; 4]),
}

impl Obstacle {
    fn bbox(&self) -> [f64; 4] {
        match self {
            Obstacle::Zone(r) => *r,
            Obstacle::Part(_, b) => *b,
        }
    }

    fn margin(&self) -> f64 {
        match self {
            Obstacle::Zone(_) => ZONE_MARGIN,
            Obstacle::Part(..) => PART_MARGIN,
        }
    }

    /// Punkt ściśle wewnątrz (punkt na brzegu można objechać).
    fn holds(&self, p: Point) -> bool {
        match self {
            Obstacle::Zone(r) => {
                p.x > r[0] + 1e-7 && p.x < r[2] - 1e-7 && p.y > r[1] + 1e-7 && p.y < r[3] - 1e-7
            }
            Obstacle::Part(poly, _) => poly.contains(p) && poly.project(p).1 > 1e-6,
        }
    }

    /// Czy odcinek przechodzi przez wnętrze przeszkody.
    pub fn crossed_by(&self, a: Point, b: Point) -> bool {
        match self {
            Obstacle::Zone(r) => seg_in_rect(a, b, *r),
            Obstacle::Part(poly, bb) => {
                if !seg_touches_box(a, b, *bb) {
                    return false;
                }
                for (p, q) in poly.segments() {
                    if proper_intersection(a, b, p, q) {
                        return true;
                    }
                }
                let m = a.midpoint(b);
                poly.contains(m) && poly.project(m).1 > 1e-6
            }
        }
    }
}

fn seg_touches_box(a: Point, b: Point, r: [f64; 4]) -> bool {
    !(a.x.max(b.x) < r[0] || a.x.min(b.x) > r[2] || a.y.max(b.y) < r[1] || a.y.min(b.y) > r[3])
}

/// Część odcinka [t0, t1] wewnątrz prostokąta (Liang–Barsky) albo brak.
pub(crate) fn clip_rect(a: Point, b: Point, r: [f64; 4]) -> Option<(f64, f64)> {
    let d = b - a;
    let mut t0: f64 = 0.0;
    let mut t1: f64 = 1.0;
    for (p, q) in [
        (-d.x, a.x - r[0]),
        (d.x, r[2] - a.x),
        (-d.y, a.y - r[1]),
        (d.y, r[3] - a.y),
    ] {
        if p.abs() < 1e-15 {
            if q < 0.0 {
                return None;
            }
        } else {
            let t = q / p;
            if p < 0.0 {
                t0 = t0.max(t);
            } else {
                t1 = t1.min(t);
            }
        }
    }
    (t1 > t0).then_some((t0, t1))
}

/// Odcinek przechodzi przez wnętrze prostokąta (nie tylko po brzegu).
pub(crate) fn seg_in_rect(a: Point, b: Point, r: [f64; 4]) -> bool {
    let s = 1e-7;
    let inner = [r[0] + s, r[1] + s, r[2] - s, r[3] - s];
    match clip_rect(a, b, inner) {
        Some((t0, t1)) => (t1 - t0) * a.distance(b) > 1e-9,
        None => false,
    }
}

fn proper_intersection(a: Point, b: Point, p: Point, q: Point) -> bool {
    let r = b - a;
    let s = q - p;
    let den = r.cross(s);
    if den.abs() < 1e-15 {
        return false;
    }
    let t = (p - a).cross(s) / den;
    let u = (p - a).cross(r) / den;
    t > 1e-9 && t < 1.0 - 1e-9 && u > -1e-12 && u < 1.0 + 1e-12
}

/// Wynik wyznaczenia przejazdu.
pub(crate) enum Route {
    /// Punkty pośrednie i końcowy (bez początkowego).
    Path(Vec<Point>),
    /// Brak objazdu: przejazd z podniesieniem; `true` = przeszkodą jest strefa zakazana.
    Lift(bool),
}

/// Przejazd z `a` do `b` omijający przeszkody.
pub(crate) fn route(a: Point, b: Point, obstacles: &[Obstacle]) -> Route {
    if a.distance(b) < EPS || obstacles.is_empty() {
        return Route::Path(vec![b]);
    }
    // przeszkody zawierające koniec przejazdu nie mogą być ominięte
    let usable: Vec<&Obstacle> = obstacles
        .iter()
        .filter(|o| !o.holds(a) && !o.holds(b))
        .collect();
    let blocked_zone = obstacles.iter().any(|o| {
        matches!(o, Obstacle::Zone(_)) && (o.holds(a) || o.holds(b)) && o.crossed_by(a, b)
    });
    let mut active: Vec<usize> = (0..usable.len())
        .filter(|&i| usable[i].crossed_by(a, b))
        .collect();
    if active.is_empty() {
        return if blocked_zone {
            Route::Lift(true)
        } else {
            Route::Path(vec![b])
        };
    }
    for _ in 0..12 {
        let path = match shortest(a, b, &usable, &active) {
            Some(p) => p,
            None => {
                let zone = active
                    .iter()
                    .any(|&i| matches!(usable[i], Obstacle::Zone(_)));
                return Route::Lift(zone || blocked_zone);
            }
        };
        // nowe przeszkody na trasie
        let mut prev = a;
        let mut added = false;
        for &p in &path {
            for (i, o) in usable.iter().enumerate() {
                if !active.contains(&i) && o.crossed_by(prev, p) {
                    active.push(i);
                    added = true;
                }
            }
            prev = p;
        }
        if !added {
            return if blocked_zone {
                Route::Lift(true)
            } else {
                Route::Path(path)
            };
        }
    }
    Route::Lift(
        active
            .iter()
            .any(|&i| matches!(usable[i], Obstacle::Zone(_))),
    )
}

/// Najkrótsza droga po grafie widoczności narożników aktywnych przeszkód.
fn shortest(a: Point, b: Point, obs: &[&Obstacle], active: &[usize]) -> Option<Vec<Point>> {
    let mut nodes = vec![a, b];
    for &i in active {
        let r = obs[i].bbox();
        let m = obs[i].margin();
        nodes.extend([
            Point::new(r[0] - m, r[1] - m),
            Point::new(r[2] + m, r[1] - m),
            Point::new(r[2] + m, r[3] + m),
            Point::new(r[0] - m, r[3] + m),
        ]);
    }
    let n = nodes.len();
    let visible = |i: usize, j: usize| {
        active
            .iter()
            .all(|&k| !obs[k].crossed_by(nodes[i], nodes[j]))
    };
    let mut dist = vec![f64::MAX; n];
    let mut prev = vec![usize::MAX; n];
    let mut done = vec![false; n];
    dist[0] = 0.0;
    loop {
        let mut u = usize::MAX;
        for i in 0..n {
            if !done[i] && dist[i] < f64::MAX && (u == usize::MAX || dist[i] < dist[u]) {
                u = i;
            }
        }
        if u == usize::MAX {
            return None;
        }
        if u == 1 {
            break;
        }
        done[u] = true;
        for v in 0..n {
            if done[v] || v == u {
                continue;
            }
            let nd = dist[u] + nodes[u].distance(nodes[v]);
            if nd < dist[v] - 1e-12 && visible(u, v) {
                dist[v] = nd;
                prev[v] = u;
            }
        }
    }
    let mut path = Vec::new();
    let mut c = 1;
    while c != 0 {
        path.push(nodes[c]);
        c = prev[c];
    }
    path.reverse();
    Some(path)
}

/// Fragmenty łamanej poza strefami zakazanymi dla wiązki; `None` = bez zmian.
pub(crate) fn clip_out(pts: &[Point], zones: &[[f64; 4]]) -> Option<Vec<Vec<Point>>> {
    let mut changed = false;
    let mut out: Vec<Vec<Point>> = Vec::new();
    let mut cur: Vec<Point> = Vec::new();
    for w in pts.windows(2) {
        let (a, b) = (w[0], w[1]);
        // przedziały t wewnątrz stref
        let mut ins: Vec<(f64, f64)> = zones
            .iter()
            .filter_map(|z| clip_rect(a, b, *z))
            .filter(|(t0, t1)| (t1 - t0) * a.distance(b) > 1e-9)
            .collect();
        if ins.is_empty() {
            if cur.is_empty() {
                cur.push(a);
            }
            cur.push(b);
            continue;
        }
        changed = true;
        ins.sort_by(|x, y| x.0.total_cmp(&y.0));
        let mut t = 0.0;
        for (t0, t1) in ins {
            if t0 > t + 1e-12 {
                let p0 = a + (b - a) * t;
                let p1 = a + (b - a) * t0;
                if cur.is_empty() {
                    cur.push(p0);
                }
                cur.push(p1);
            }
            if cur.len() > 1 {
                out.push(std::mem::take(&mut cur));
            } else {
                cur.clear();
            }
            t = t.max(t1);
        }
        if t < 1.0 - 1e-12 {
            cur.push(a + (b - a) * t);
            cur.push(b);
        }
    }
    if cur.len() > 1 {
        out.push(cur);
    }
    changed.then_some(out)
}
