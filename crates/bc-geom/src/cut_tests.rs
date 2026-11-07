use super::*;
use crate::{bbox, is_closed, parse_svg_path, path_length, polyline_path, rect_path, subpaths};

fn p(x: f64, y: f64) -> Point {
    Point::new(x, y)
}

fn line(a: (f64, f64), b: (f64, f64)) -> BezPath {
    polyline_path(&[p(a.0, a.1), p(b.0, b.1)], false)
}

fn close(a: f64, b: f64, eps: f64) -> bool {
    (a - b).abs() <= eps
}

fn boxes(path: &BezPath) -> Vec<Rect> {
    let mut v: Vec<Rect> = subpaths(path).iter().filter_map(bbox).collect();
    v.sort_by(|a, b| a.x0.partial_cmp(&b.x0).unwrap());
    v
}

// ---------- przycinanie (REQ-04-034) ----------

#[test]
fn req_04_034_trim_line_between_two_cutters() {
    let target = line((0., 0.), (30., 0.));
    let cutters = [line((10., -5.), (10., 5.)), line((20., -5.), (20., 5.))];
    let mid = trim(&target, &cutters, p(15., 0.3)).unwrap();
    let b = boxes(&mid);
    assert_eq!(b.len(), 2, "zostają dwa kawałki");
    assert!(close(b[0].x0, 0., 1e-9) && close(b[0].x1, 10., 1e-9));
    assert!(close(b[1].x0, 20., 1e-9) && close(b[1].x1, 30., 1e-9));
    // od początku do pierwszego przecięcia
    let head = trim(&target, &cutters, p(4., 0.)).unwrap();
    let b = boxes(&head);
    assert_eq!(b.len(), 1);
    assert!(close(b[0].x0, 10., 1e-9) && close(b[0].x1, 30., 1e-9));
    assert!(
        trim(&target, &[line((50., -5.), (50., 5.))], p(15., 0.)).is_err(),
        "brak przecięć"
    );
}

#[test]
fn req_04_034_trim_closed_shape_and_curves() {
    let sq = rect_path(0., 0., 10., 10., 0.);
    let cut = [line((5., -5.), (5., 15.))];
    let r = trim(&sq, &cut, p(0., 5.)).unwrap();
    assert!(
        !is_closed(&r),
        "zamknięty kształt po przycięciu jest otwarty"
    );
    let b = bbox(&r).unwrap();
    assert!(close(b.x0, 5., 1e-9) && close(b.x1, 10., 1e-9));
    assert!(
        close(path_length(&r), 20., 1e-6),
        "prawa połowa: 5 + 10 + 5"
    );
    // krzywa: koniec dokładnie na linii tnącej, nadal krzywa
    let arc = parse_svg_path("M0 0 C0 20 20 20 20 0").unwrap();
    let r = trim(&arc, &[line((5., -1.), (5., 30.))], p(1., 3.)).unwrap();
    let start = r.elements()[0];
    let kurbo::PathEl::MoveTo(s) = start else {
        panic!("{start:?}")
    };
    assert!(close(s.x, 5., 1e-6), "{s:?}");
    assert!(r
        .elements()
        .iter()
        .any(|e| matches!(e, kurbo::PathEl::CurveTo(..))));
}

#[test]
fn req_04_034_trim_uses_own_other_contours() {
    let two = parse_svg_path("M0 0 L30 0 M10 -5 L10 5").unwrap();
    let r = trim(&two, &[], p(20., 0.)).unwrap();
    let b = boxes(&r);
    // zostaje (0,0)-(10,0) i linia pionowa
    assert!(b.iter().all(|r| r.x1 <= 10. + 1e-9), "{b:?}");
}

// ---------- gumka (REQ-04-021) ----------

#[test]
fn req_04_021_erase_splits_line_under_stroke() {
    let l = line((0., 0.), (30., 0.));
    let r = erase(&l, &[p(15., -5.), p(15., 5.)], 2.0).unwrap();
    let b = boxes(&r);
    assert_eq!(b.len(), 2);
    assert!(
        close(b[0].x0, 0., 1e-3) && close(b[0].x1, 14., 1e-3),
        "{b:?}"
    );
    assert!(
        close(b[1].x0, 16., 1e-3) && close(b[1].x1, 30., 1e-3),
        "{b:?}"
    );
    // pociągnięcie obok: bez zmian
    let same = erase(&l, &[p(15., 5.), p(20., 5.)], 2.0).unwrap();
    assert!(close(path_length(&same), 30., 1e-6));
    // pełne zakrycie: nic nie zostaje
    let gone = erase(&line((0., 0.), (2., 0.)), &[p(-1., 0.), p(3., 0.)], 4.0).unwrap();
    assert!(gone.elements().is_empty());
    // zamknięty kwadrat przecięty z boku staje się otwarty
    let sq = erase(
        &rect_path(0., 0., 10., 10., 0.),
        &[p(-2., 5.), p(2., 5.)],
        2.0,
    )
    .unwrap();
    assert!(!is_closed(&sq));
    assert!(close(path_length(&sq), 38., 1e-3), "{}", path_length(&sq));
    assert!(erase(&l, &[p(0., 0.)], 0.0).is_err(), "szerokość > 0");
}

// ---------- wypaczanie (REQ-04-045…047) ----------

#[test]
fn req_04_045_four_point_warp() {
    let sq = rect_path(0., 0., 10., 10., 0.);
    let b = Rect::new(0., 0., 10., 10.);
    let same = mesh_warp(
        &sq,
        b,
        1,
        1,
        &[p(0., 0.), p(10., 0.), p(0., 10.), p(10., 10.)],
    )
    .unwrap();
    let bb = bbox(&same).unwrap();
    assert!(close(bb.x1, 10., 1e-9) && close(bb.y1, 10., 1e-9));
    let wide = mesh_warp(
        &sq,
        b,
        1,
        1,
        &[p(0., 0.), p(20., 0.), p(0., 10.), p(20., 10.)],
    )
    .unwrap();
    let bb = bbox(&wide).unwrap();
    assert!(close(bb.x1, 20., 1e-9));
    // trapez: górna krawędź zwężona, lewy bok przechodzi na prostą (0,0)-(2,10)
    let trap = mesh_warp(
        &sq,
        b,
        1,
        1,
        &[p(0., 0.), p(10., 0.), p(2., 10.), p(8., 10.)],
    )
    .unwrap();
    let pts: Vec<Point> = crate::flatten(&trap, 0.001).into_iter().flatten().collect();
    assert!(
        pts.iter()
            .any(|q| close(q.x, 1., 1e-6) && close(q.y, 5., 1e-6)),
        "środek lewego boku"
    );
    assert!(pts
        .iter()
        .all(|q| q.y <= 10. + 1e-9 && q.x >= -1e-9 && q.x <= 10. + 1e-9));
    assert!(is_closed(&trap));
    assert!(
        mesh_warp(&sq, b, 1, 1, &[p(0., 0.)]).is_err(),
        "zła liczba punktów siatki"
    );
}

#[test]
fn req_04_046_047_mesh_and_envelope_warp() {
    // siatka 2×2: środek przesunięty w górę ciągnie środek kształtu
    let l = line((0., 5.), (10., 5.));
    let b = Rect::new(0., 0., 10., 10.);
    let mut g = Vec::new();
    for j in 0..3 {
        for i in 0..3 {
            g.push(p(i as f64 * 5., j as f64 * 5.));
        }
    }
    g[4] = p(5., 8.);
    let r = mesh_warp(&l, b, 2, 2, &g).unwrap();
    let pts: Vec<Point> = crate::flatten(&r, 0.001).into_iter().flatten().collect();
    assert!(pts
        .iter()
        .any(|q| close(q.x, 5., 1e-6) && close(q.y, 8., 1e-6)));
    assert!(
        pts.iter()
            .any(|q| close(q.x, 0., 1e-9) && close(q.y, 5., 1e-9)),
        "brzegi bez zmian"
    );
    // obwiednia: 1 wiersz × 2 kolumny, góra wygięta w łuk
    let top = [p(0., 10.), p(5., 14.), p(10., 10.)];
    let bottom = [p(0., 0.), p(5., 0.), p(10., 0.)];
    let env: Vec<Point> = bottom.iter().chain(top.iter()).copied().collect();
    let r = mesh_warp(&rect_path(0., 0., 10., 10., 0.), b, 2, 1, &env).unwrap();
    let bb = bbox(&r).unwrap();
    assert!(close(bb.y1, 14., 1e-6) && close(bb.y0, 0., 1e-9));
}
