//! Model węzłów ścieżki do edycji (REQ-04-023…035): kontury z węzłami
//! i uchwytami Béziera.
//!
//! Numeracja: węzły kolejno we wszystkich konturach (jak w `gui.scene`),
//! segment `k` zaczyna się w węźle `k` (w konturze zamkniętym ostatni segment
//! wraca do pierwszego węzła). Odcinek prosty nie ma uchwytów; węzeł jest
//! gładki, gdy oba uchwyty leżą na jednej prostej po przeciwnych stronach.

use kurbo::{BezPath, CubicBez, Line, ParamCurve, ParamCurveNearest, PathEl, Point, Vec2};

use crate::ops::GeomError;

/// Dokładność wyszukiwania najbliższego punktu na krzywej (mm).
const NEAREST_ACCURACY: f64 = 1e-9;

#[derive(Debug, Clone, PartialEq)]
pub struct Node {
    pub p: Point,
    /// Uchwyt wejściowy (od poprzedniego segmentu).
    pub hin: Option<Point>,
    /// Uchwyt wyjściowy (do następnego segmentu).
    pub hout: Option<Point>,
}

impl Node {
    fn at(p: Point) -> Self {
        Node {
            p,
            hin: None,
            hout: None,
        }
    }

    /// Oba uchwyty współliniowe po przeciwnych stronach węzła.
    pub fn smooth(&self) -> bool {
        let (Some(a), Some(b)) = (self.hin, self.hout) else {
            return false;
        };
        let (u, v) = (a - self.p, b - self.p);
        let (lu, lv) = (u.hypot(), v.hypot());
        if lu < 1e-12 || lv < 1e-12 {
            return false;
        }
        (u.cross(v) / (lu * lv)).abs() < 1e-6 && u.dot(v) < 0.0
    }

    fn shift(&mut self, d: Vec2) {
        self.p += d;
        self.hin = self.hin.map(|h| h + d);
        self.hout = self.hout.map(|h| h + d);
    }

    fn reversed(&self) -> Self {
        Node {
            p: self.p,
            hin: self.hout,
            hout: self.hin,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Contour {
    pub nodes: Vec<Node>,
    pub closed: bool,
}

impl Contour {
    /// Liczba segmentów (zamknięty: także segment powrotny).
    fn segments(&self) -> usize {
        match (self.nodes.len(), self.closed) {
            (0, _) => 0,
            (n, true) => n,
            (n, false) => n - 1,
        }
    }

    fn reverse(&mut self) {
        self.nodes.reverse();
        for n in &mut self.nodes {
            *n = n.reversed();
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    In,
    Out,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Axis {
    X,
    Y,
}

fn invalid(m: impl Into<String>) -> GeomError {
    GeomError::Invalid(m.into())
}

fn same(a: Point, b: Point) -> bool {
    a.distance(b) < 1e-9
}

pub fn contours(p: &BezPath) -> Vec<Contour> {
    let mut out: Vec<Contour> = Vec::new();
    let mut cur: Option<Contour> = None;
    let finish = |c: Option<Contour>, out: &mut Vec<Contour>| {
        if let Some(c) = c {
            if c.nodes.len() >= 2 {
                out.push(c);
            }
        }
    };
    let mut last = Point::ZERO;
    for el in p.elements() {
        match *el {
            PathEl::MoveTo(q) => {
                finish(cur.take(), &mut out);
                cur = Some(Contour {
                    nodes: vec![Node::at(q)],
                    closed: false,
                });
                last = q;
            }
            PathEl::LineTo(b) => {
                let c = cur.get_or_insert_with(|| Contour {
                    nodes: vec![Node::at(last)],
                    closed: false,
                });
                if !same(b, last) {
                    c.nodes.push(Node::at(b));
                    last = b;
                }
            }
            PathEl::QuadTo(q, b) => {
                let c1 = last + (q - last) * (2.0 / 3.0);
                let c2 = b + (q - b) * (2.0 / 3.0);
                push_curve(&mut cur, &mut last, c1, c2, b);
            }
            PathEl::CurveTo(c1, c2, b) => push_curve(&mut cur, &mut last, c1, c2, b),
            PathEl::ClosePath => {
                if let Some(c) = cur.as_mut() {
                    c.closed = true;
                    let n = c.nodes.len();
                    if n > 1 && same(c.nodes[n - 1].p, c.nodes[0].p) {
                        let end = c.nodes.pop().expect("n > 1");
                        c.nodes[0].hin = end.hin;
                    }
                    last = c.nodes[0].p;
                }
                finish(cur.take(), &mut out);
            }
        }
    }
    finish(cur, &mut out);
    out
}

fn push_curve(cur: &mut Option<Contour>, last: &mut Point, c1: Point, c2: Point, b: Point) {
    let c = cur.get_or_insert_with(|| Contour {
        nodes: vec![Node::at(*last)],
        closed: false,
    });
    if same(b, *last) && same(c1, *last) && same(c2, *last) {
        return; // segment zdegenerowany do punktu
    }
    // punkt kontrolny w samym węźle = brak uchwytu (tak zapisuje go `to_path`)
    let hout = (!same(c1, *last)).then_some(c1);
    let hin = (!same(c2, b)).then_some(c2);
    c.nodes.last_mut().expect("kontur ma węzeł startowy").hout = hout;
    c.nodes.push(Node {
        p: b,
        hin,
        hout: None,
    });
    *last = b;
}

fn segment_to(path: &mut BezPath, a: &Node, b: &Node) {
    if a.hout.is_none() && b.hin.is_none() {
        path.line_to(b.p);
    } else {
        path.curve_to(a.hout.unwrap_or(a.p), b.hin.unwrap_or(b.p), b.p);
    }
}

pub fn to_path(cs: &[Contour]) -> BezPath {
    let mut path = BezPath::new();
    for c in cs.iter().filter(|c| !c.nodes.is_empty()) {
        let n = &c.nodes;
        path.move_to(n[0].p);
        for i in 1..n.len() {
            segment_to(&mut path, &n[i - 1], &n[i]);
        }
        if c.closed {
            let (a, b) = (&n[n.len() - 1], &n[0]);
            // odcinek powrotny prosty domyka ClosePath
            if a.hout.is_some() || b.hin.is_some() {
                segment_to(&mut path, a, b);
            }
            path.close_path();
        }
    }
    path
}

/// Liczba węzłów we wszystkich konturach.
pub fn count(cs: &[Contour]) -> usize {
    cs.iter().map(|c| c.nodes.len()).sum()
}

/// (kontur, indeks w konturze) dla globalnego indeksu węzła.
pub fn locate(cs: &[Contour], node: usize) -> Option<(usize, usize)> {
    let mut k = node;
    for (ci, c) in cs.iter().enumerate() {
        if k < c.nodes.len() {
            return Some((ci, k));
        }
        k -= c.nodes.len();
    }
    None
}

fn locate_node(cs: &[Contour], node: usize) -> Result<(usize, usize), GeomError> {
    locate(cs, node).ok_or_else(|| {
        invalid(format!(
            "węzeł {node} poza zakresem 0–{}",
            count(cs).saturating_sub(1)
        ))
    })
}

fn locate_segment(cs: &[Contour], seg: usize) -> Result<(usize, usize), GeomError> {
    match locate(cs, seg) {
        Some((c, i)) if i < cs[c].segments() => Ok((c, i)),
        _ => Err(invalid(format!("segment {seg} nie istnieje"))),
    }
}

/// Punkty kontrolne segmentu (kubiczne; odcinek prosty ma uchwyty w końcach).
fn seg_points(c: &Contour, i: usize) -> (Point, Point, Point, Point) {
    let a = &c.nodes[i];
    let b = &c.nodes[(i + 1) % c.nodes.len()];
    (a.p, a.hout.unwrap_or(a.p), b.hin.unwrap_or(b.p), b.p)
}

fn is_line(c: &Contour, i: usize) -> bool {
    c.nodes[i].hout.is_none() && c.nodes[(i + 1) % c.nodes.len()].hin.is_none()
}

/// Najbliższy punkt na ścieżce: (segment, parametr t, punkt).
pub fn nearest(cs: &[Contour], pt: Point) -> Option<(usize, f64, Point)> {
    let mut best: Option<(f64, usize, f64, Point)> = None;
    let mut base = 0;
    for c in cs {
        for i in 0..c.segments() {
            let (p0, p1, p2, p3) = seg_points(c, i);
            let (t, q) = if is_line(c, i) {
                let l = Line::new(p0, p3);
                let n = l.nearest(pt, NEAREST_ACCURACY);
                (n.t, l.eval(n.t))
            } else {
                let cb = CubicBez::new(p0, p1, p2, p3);
                let n = cb.nearest(pt, NEAREST_ACCURACY);
                (n.t, cb.eval(n.t))
            };
            let d = q.distance_squared(pt);
            if best.is_none_or(|b| d < b.0) {
                best = Some((d, base + i, t, q));
            }
        }
        base += c.nodes.len();
    }
    best.map(|(_, s, t, q)| (s, t, q))
}

/// Przesuwa węzeł do `to` razem z uchwytami.
pub fn move_node(cs: &mut [Contour], node: usize, to: Point) -> Result<(), GeomError> {
    let (c, i) = locate_node(cs, node)?;
    let n = &mut cs[c].nodes[i];
    let d = to - n.p;
    n.shift(d);
    n.p = to; // bez błędu zaokrąglenia p + (to - p)
    Ok(())
}

/// Ustawia uchwyt; w węźle gładkim przeciwny uchwyt zachowuje kierunek (lustro, własna długość).
pub fn set_handle(cs: &mut [Contour], node: usize, side: Side, to: Point) -> Result<(), GeomError> {
    let (c, i) = locate_node(cs, node)?;
    let n = &mut cs[c].nodes[i];
    let smooth = n.smooth();
    let (this, other) = match side {
        Side::In => (&mut n.hin, &mut n.hout),
        Side::Out => (&mut n.hout, &mut n.hin),
    };
    *this = Some(to);
    let dir = to - n.p;
    if smooth && dir.hypot() > 1e-12 {
        if let Some(o) = other.as_mut() {
            let len = (*o - n.p).hypot();
            *o = n.p - dir.normalize() * len;
        }
    }
    Ok(())
}

/// Dodaje węzeł w najbliższym punkcie ścieżki (kształt bez zmian); zwraca jego indeks.
pub fn insert_at(cs: &mut [Contour], pt: Point) -> Result<usize, GeomError> {
    let (seg, t, q) = nearest(cs, pt).ok_or_else(|| invalid("ścieżka bez segmentów"))?;
    if t < 1e-9 {
        return Ok(seg);
    }
    let (c, i) = locate_segment(cs, seg)?;
    let len = cs[c].nodes.len();
    let j = (i + 1) % len;
    if t > 1.0 - 1e-9 {
        return Ok(seg - i + j);
    }
    let contour = &mut cs[c];
    let new = if is_line(contour, i) {
        Node::at(q)
    } else {
        let (p0, p1, p2, p3) = seg_points(contour, i);
        let lerp = |a: Point, b: Point| a.lerp(b, t);
        let (a1, m12, b2) = (lerp(p0, p1), lerp(p1, p2), lerp(p2, p3));
        let (a2, b1) = (lerp(a1, m12), lerp(m12, b2));
        let m = lerp(a2, b1);
        contour.nodes[i].hout = Some(a1);
        contour.nodes[j].hin = Some(b2);
        Node {
            p: m,
            hin: Some(a2),
            hout: Some(b1),
        }
    };
    contour.nodes.insert(i + 1, new);
    Ok(seg + 1)
}

fn group_by_contour(cs: &[Contour], nodes: &[usize]) -> Result<Vec<Vec<usize>>, GeomError> {
    let mut by = vec![Vec::new(); cs.len()];
    for &n in nodes {
        let (c, i) = locate_node(cs, n)?;
        if !by[c].contains(&i) {
            by[c].push(i);
        }
    }
    Ok(by)
}

/// Usuwa węzły; kontur z mniej niż dwoma węzłami znika.
pub fn delete_nodes(cs: &mut Vec<Contour>, nodes: &[usize]) -> Result<(), GeomError> {
    let by = group_by_contour(cs, nodes)?;
    for (c, mut idx) in by.into_iter().enumerate() {
        idx.sort_unstable_by(|a, b| b.cmp(a));
        for i in idx {
            cs[c].nodes.remove(i);
        }
    }
    cs.retain(|c| c.nodes.len() >= 2);
    Ok(())
}

/// Przełamuje ścieżkę w węźle: kontur zamknięty staje się otwarty (start i koniec
/// w tym węźle), otwarty dzieli się na dwa.
pub fn break_at(cs: &mut Vec<Contour>, node: usize) -> Result<(), GeomError> {
    let (c, i) = locate_node(cs, node)?;
    let contour = &mut cs[c];
    if contour.closed {
        contour.nodes.rotate_left(i);
        let mut end = contour.nodes[0].clone();
        end.hout = None;
        contour.nodes[0].hin = None;
        contour.nodes.push(end);
        contour.closed = false;
        return Ok(());
    }
    let len = contour.nodes.len();
    if i == 0 || i + 1 == len {
        return Err(invalid(format!(
            "węzeł {node} jest końcem ścieżki — nie ma czego przełamać"
        )));
    }
    let mut tail: Vec<Node> = contour.nodes.split_off(i + 1);
    let mut start = contour.nodes[i].clone();
    contour.nodes[i].hout = None;
    start.hin = None;
    tail.insert(0, start);
    cs.insert(
        c + 1,
        Contour {
            nodes: tail,
            closed: false,
        },
    );
    Ok(())
}

/// Czy węzeł jest początkiem (`Some(true)`) albo końcem (`Some(false)`) otwartego konturu.
fn endpoint(cs: &[Contour], c: usize, i: usize) -> Option<bool> {
    let k = &cs[c];
    if k.closed {
        None
    } else if i == 0 {
        Some(true)
    } else if i + 1 == k.nodes.len() {
        Some(false)
    } else {
        None
    }
}

/// Łączy dwa końce otwartych konturów: ten sam kontur — zamyka go; różne — scala.
/// Końce bliżej niż `tol` są spawane w jeden węzeł, dalsze łączy odcinek.
pub fn join_ends(cs: &mut Vec<Contour>, a: usize, b: usize, tol: f64) -> Result<(), GeomError> {
    let (ca, ia) = locate_node(cs, a)?;
    let (cb, ib) = locate_node(cs, b)?;
    let not_end = |n: usize| invalid(format!("węzeł {n} nie jest końcem otwartej ścieżki"));
    let a_start = endpoint(cs, ca, ia).ok_or_else(|| not_end(a))?;
    let b_start = endpoint(cs, cb, ib).ok_or_else(|| not_end(b))?;
    if ca == cb {
        if a_start == b_start {
            return Err(invalid("węzły muszą być różnymi końcami ścieżki"));
        }
        let k = &mut cs[ca];
        let n = k.nodes.len();
        if n >= 3 && k.nodes[0].p.distance(k.nodes[n - 1].p) <= tol {
            let end = k.nodes.pop().expect("n ≥ 3");
            k.nodes[0].p = k.nodes[0].p.midpoint(end.p);
            k.nodes[0].hin = end.hin;
        }
        k.closed = true;
        return Ok(());
    }
    // kierunek konturu `a` zostaje; kontur `b` w razie potrzeby odwrócony
    let ka = cs[ca].clone();
    let mut kb = cs[cb].clone();
    let (mut first, mut second) = match (a_start, b_start) {
        (false, true) => (ka, kb),
        (true, false) => (kb, ka),
        (true, true) => {
            kb.reverse();
            (kb, ka)
        }
        (false, false) => {
            kb.reverse();
            (ka, kb)
        }
    };
    if !second.nodes.is_empty() {
        let end = first.nodes.last().expect("kontur").p;
        if end.distance(second.nodes[0].p) <= tol {
            let s0 = second.nodes.remove(0);
            let last = first.nodes.last_mut().expect("kontur");
            last.p = last.p.midpoint(s0.p);
            last.hout = s0.hout;
        }
        first.nodes.extend(second.nodes);
    }
    let (lo, hi) = (ca.min(cb), ca.max(cb));
    cs.remove(hi);
    cs[lo] = first;
    Ok(())
}

fn neighbors(c: &Contour, i: usize) -> (Option<Point>, Option<Point>) {
    let n = c.nodes.len();
    let prev = if i > 0 {
        Some(c.nodes[i - 1].p)
    } else if c.closed {
        Some(c.nodes[n - 1].p)
    } else {
        None
    };
    let next = if i + 1 < n {
        Some(c.nodes[i + 1].p)
    } else if c.closed {
        Some(c.nodes[0].p)
    } else {
        None
    };
    (prev, next)
}

/// Wygładza węzły: uchwyty współliniowe (styczna z sąsiadów, długość 1/3 odcinków).
pub fn make_smooth(cs: &mut [Contour], nodes: &[usize]) -> Result<(), GeomError> {
    for (c, idx) in group_by_contour(cs, nodes)?.into_iter().enumerate() {
        for i in idx {
            let (prev, next) = neighbors(&cs[c], i);
            let p = cs[c].nodes[i].p;
            let t = match (prev, next) {
                (Some(a), Some(b)) => b - a,
                (Some(a), None) => p - a,
                (None, Some(b)) => b - p,
                (None, None) => continue,
            };
            if t.hypot() < 1e-12 {
                continue;
            }
            let t = t.normalize();
            let n = &mut cs[c].nodes[i];
            n.hin = prev.map(|a| p - t * (p.distance(a) / 3.0));
            n.hout = next.map(|b| p + t * (p.distance(b) / 3.0));
        }
    }
    Ok(())
}

/// Narożnik: usuwa uchwyty węzłów (ostre załamanie).
pub fn make_corner(cs: &mut [Contour], nodes: &[usize]) -> Result<(), GeomError> {
    for (c, idx) in group_by_contour(cs, nodes)?.into_iter().enumerate() {
        for i in idx {
            cs[c].nodes[i].hin = None;
            cs[c].nodes[i].hout = None;
        }
    }
    Ok(())
}

/// Segment jako odcinek prosty (`curve = false`) albo krzywa (uchwyty w 1/3 i 2/3).
pub fn set_segment_kind(cs: &mut [Contour], segment: usize, curve: bool) -> Result<(), GeomError> {
    let (c, i) = locate_segment(cs, segment)?;
    let k = &mut cs[c];
    let j = (i + 1) % k.nodes.len();
    let (a, b) = (k.nodes[i].p, k.nodes[j].p);
    if curve {
        if k.nodes[i].hout.is_none() && k.nodes[j].hin.is_none() {
            k.nodes[i].hout = Some(a.lerp(b, 1.0 / 3.0));
            k.nodes[j].hin = Some(a.lerp(b, 2.0 / 3.0));
        }
    } else {
        k.nodes[i].hout = None;
        k.nodes[j].hin = None;
    }
    Ok(())
}

/// Wyrównuje węzły w osi do `value` (domyślnie średnia).
pub fn align(
    cs: &mut [Contour],
    nodes: &[usize],
    axis: Axis,
    value: Option<f64>,
) -> Result<(), GeomError> {
    if nodes.is_empty() {
        return Err(invalid("brak węzłów do wyrównania"));
    }
    let coord = |p: Point| if axis == Axis::X { p.x } else { p.y };
    let mut sum = 0.0;
    for &n in nodes {
        let (c, i) = locate_node(cs, n)?;
        sum += coord(cs[c].nodes[i].p);
    }
    let v = value.unwrap_or(sum / nodes.len() as f64);
    for &n in nodes {
        let (c, i) = locate_node(cs, n)?;
        let p = cs[c].nodes[i].p;
        let to = if axis == Axis::X {
            Point::new(v, p.y)
        } else {
            Point::new(p.x, v)
        };
        move_node(cs, n, to)?;
    }
    Ok(())
}

/// Nożyczki: przecina ścieżkę w najbliższym punkcie (dodaj węzeł + przełam).
pub fn cut_at(cs: &mut Vec<Contour>, pt: Point) -> Result<(), GeomError> {
    let n = insert_at(cs, pt)?;
    break_at(cs, n)
}

#[cfg(test)]
#[path = "nodes_tests.rs"]
mod tests;
