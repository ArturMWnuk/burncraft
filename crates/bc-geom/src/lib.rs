//! bc-geom — geometria 2D BurnCraft.
//!
//! Jednostki: milimetry, oś Y w górę (ADR 0003). Ścieżki to [`kurbo::BezPath`]
//! z segmentami liniowymi, kwadratowymi i kubicznymi Béziera; łuki są
//! zamieniane na kubiczne Béziery przy tworzeniu.
//! Booleany, offsety i indeks przestrzenny dochodzą w M1.

pub mod cut;
pub mod nodes;
pub mod ops;
pub mod shaping;

pub use kurbo::{Affine, BezPath, ParamCurve, PathEl, Point, Rect, Shape, Vec2};

/// Tolerancja spłaszczania krzywych w mm (dokładność podglądu i długości).
pub const DEFAULT_TOLERANCE: f64 = 0.01;

/// Tolerancja aproksymacji łuków krzywymi Béziera (mm). Cztery krzywe na okrąg
/// zawyżają promień o ~0,027% — dla dużych okręgów to dziesiątki µm, więc łuki
/// są dzielone gęściej.
pub const ARC_TOLERANCE: f64 = 1e-5;

/// Największa dopuszczalna wartość współrzędnej (mm) — 10 km. Większe wartości to
/// uszkodzone lub złośliwe dane (fuzzing: brak pamięci przy aproksymacji łuków).
pub const MAX_COORD: f64 = 1.0e7;

/// Czy wszystkie punkty ścieżki (także kontrolne) mieszczą się w ±[`MAX_COORD`].
pub fn within_limits(p: &BezPath) -> bool {
    let ok = |q: &Point| {
        q.x.is_finite() && q.y.is_finite() && q.x.abs() <= MAX_COORD && q.y.abs() <= MAX_COORD
    };
    p.elements().iter().all(|e| match e {
        PathEl::MoveTo(a) | PathEl::LineTo(a) => ok(a),
        PathEl::QuadTo(a, b) => ok(a) && ok(b),
        PathEl::CurveTo(a, b, c) => ok(a) && ok(b) && ok(c),
        PathEl::ClosePath => true,
    })
}

/// Tolerancja aproksymacji łuku o rozmiarze `size`: [`ARC_TOLERANCE`], ale nie mniej niż
/// 1e-9 rozmiaru — liczba segmentów pozostaje ograniczona przy dowolnej skali.
pub fn arc_tolerance(size: f64) -> f64 {
    ARC_TOLERANCE.max(size.abs() * 1e-9)
}

/// Prostokąt o lewym dolnym narożniku (x, y) z opcjonalnym promieniem narożników.
pub fn rect_path(x: f64, y: f64, w: f64, h: f64, radius: f64) -> BezPath {
    let r = radius.max(0.0).min(w.abs() / 2.0).min(h.abs() / 2.0);
    let rect = Rect::new(x, y, x + w, y + h);
    let tol = arc_tolerance(w.abs().max(h.abs()));
    if r > 0.0 {
        kurbo::RoundedRect::from_rect(rect, r).to_path(tol)
    } else {
        rect.to_path(tol)
    }
}

/// Elipsa o środku (cx, cy) i półosiach rx, ry.
pub fn ellipse_path(cx: f64, cy: f64, rx: f64, ry: f64) -> BezPath {
    kurbo::Ellipse::new((cx, cy), (rx, ry), 0.0).to_path(arc_tolerance(rx.abs().max(ry.abs())))
}

/// Polilinia z listy punktów; `closed` dodaje ClosePath.
pub fn polyline_path(points: &[Point], closed: bool) -> BezPath {
    let mut p = BezPath::new();
    for (i, pt) in points.iter().enumerate() {
        if i == 0 {
            p.move_to(*pt);
        } else {
            p.line_to(*pt);
        }
    }
    if closed && points.len() > 2 {
        p.close_path();
    }
    p
}

/// Parsuje dane ścieżki w składni SVG `d` (M/L/H/V/C/S/Q/T/A/Z).
pub fn parse_svg_path(d: &str) -> Result<BezPath, String> {
    BezPath::from_svg(d).map_err(|e| format!("niepoprawne dane ścieżki: {e:?}"))
}

/// Liczba podścieżek (każde MoveTo zaczyna nową).
pub fn subpath_count(p: &BezPath) -> usize {
    p.elements()
        .iter()
        .filter(|e| matches!(e, PathEl::MoveTo(_)))
        .count()
}

/// Czy każda podścieżka jest zamknięta (jawnie przez Z lub geometrycznie).
pub fn is_closed(p: &BezPath) -> bool {
    let mut any = false;
    for sub in subpaths(p) {
        any = true;
        let els = sub.elements();
        let explicit = matches!(els.last(), Some(PathEl::ClosePath));
        let geometric = match (els.first(), sub.segments().last()) {
            (Some(PathEl::MoveTo(a)), Some(seg)) => a.distance(seg.end()) < 1e-6,
            _ => false,
        };
        if !(explicit || geometric) {
            return false;
        }
    }
    any
}

/// Rozbija ścieżkę na podścieżki.
pub fn subpaths(p: &BezPath) -> Vec<BezPath> {
    let mut out: Vec<BezPath> = Vec::new();
    for el in p.elements() {
        if matches!(el, PathEl::MoveTo(_)) || out.is_empty() {
            out.push(BezPath::new());
        }
        out.last_mut().unwrap().push(*el);
    }
    out
}

/// Długość ścieżki w mm.
pub fn path_length(p: &BezPath) -> f64 {
    use kurbo::ParamCurveArclen;
    p.segments()
        .map(|s| s.arclen(DEFAULT_TOLERANCE * 0.1))
        .sum()
}

/// Bounding box (dokładny, z ekstremów krzywych). `None` dla pustej ścieżki.
pub fn bbox(p: &BezPath) -> Option<Rect> {
    if p.elements().is_empty() {
        None
    } else {
        Some(p.bounding_box())
    }
}

/// Spłaszcza ścieżkę do polilinii (po jednej na podścieżkę).
pub fn flatten(p: &BezPath, tolerance: f64) -> Vec<Vec<Point>> {
    let mut out: Vec<Vec<Point>> = Vec::new();
    kurbo::flatten(p.iter(), tolerance, |el| match el {
        PathEl::MoveTo(pt) => out.push(vec![pt]),
        PathEl::LineTo(pt) => {
            if let Some(last) = out.last_mut() {
                last.push(pt)
            }
        }
        PathEl::ClosePath => {
            if let Some(last) = out.last_mut() {
                if let Some(first) = last.first().copied() {
                    last.push(first)
                }
            }
        }
        _ => {}
    });
    out
}

/// Liczba węzłów (punktów końcowych segmentów) — statystyka dokumentu.
pub fn node_count(p: &BezPath) -> usize {
    p.elements()
        .iter()
        .filter(|e| !matches!(e, PathEl::ClosePath))
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64, eps: f64) -> bool {
        (a - b).abs() < eps
    }

    #[test]
    fn req_02_002_line_segments() {
        let p = polyline_path(
            &[
                Point::new(0., 0.),
                Point::new(30., 0.),
                Point::new(30., 40.),
            ],
            false,
        );
        assert!(close(path_length(&p), 70.0, 1e-9));
        assert!(!is_closed(&p));
    }

    #[test]
    fn req_02_004_cubic_bezier() {
        let p = parse_svg_path("M0 0 C 0 10 10 10 10 0").unwrap();
        assert!(matches!(p.elements()[1], PathEl::CurveTo(..)));
        let b = bbox(&p).unwrap();
        assert!(
            close(b.y1, 7.5, 1e-6),
            "dokładny bbox z ekstremum krzywej, mam {}",
            b.y1
        );
    }

    #[test]
    fn req_02_005_quadratic_bezier() {
        let p = parse_svg_path("M0 0 Q 5 10 10 0").unwrap();
        assert!(matches!(p.elements()[1], PathEl::QuadTo(..)));
        assert!(close(bbox(&p).unwrap().y1, 5.0, 1e-6));
    }

    #[test]
    fn req_02_001_multiple_subpaths() {
        let p = parse_svg_path("M0 0 L10 0 L10 10 Z M20 20 L30 20 L30 30 Z").unwrap();
        assert_eq!(subpath_count(&p), 2);
        assert!(is_closed(&p));
        assert_eq!(flatten(&p, 0.01).len(), 2);
    }

    #[test]
    fn req_02_006_rounded_rect() {
        let sharp = rect_path(0., 0., 100., 50., 0.);
        assert!(close(path_length(&sharp), 300.0, 1e-9));
        let round = rect_path(0., 0., 100., 50., 10.);
        // obwód: 2*(100-20) + 2*(50-20) + 2*pi*10
        let expect = 160.0 + 60.0 + 2.0 * std::f64::consts::PI * 10.0;
        assert!(
            close(path_length(&round), expect, 0.01),
            "{}",
            path_length(&round)
        );
        assert!(is_closed(&round));
    }

    #[test]
    fn req_02_007_ellipse() {
        let e = ellipse_path(0., 0., 10., 10.);
        assert!(close(
            path_length(&e),
            2.0 * std::f64::consts::PI * 10.0,
            0.01
        ));
        let b = bbox(&e).unwrap();
        // bbox dokładny do tolerancji aproksymacji łuków
        assert!(
            close(b.width(), 20.0, 2.0 * ARC_TOLERANCE)
                && close(b.height(), 20.0, 2.0 * ARC_TOLERANCE),
            "{b:?}"
        );
    }

    #[test]
    fn invalid_svg_path_is_error() {
        assert!(parse_svg_path("M 0 0 L foo").is_err());
    }
}
