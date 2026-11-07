use super::*;
use crate::{
    bbox, is_closed, parse_svg_path, path_length, polyline_path, rect_path, subpath_count,
};
use std::f64::consts::PI;

fn close(a: f64, b: f64, eps: f64) -> bool {
    (a - b).abs() <= eps
}

fn sq(x: f64, y: f64, s: f64) -> BezPath {
    rect_path(x, y, s, s, 0.0)
}

fn pts(p: &BezPath) -> Vec<Point> {
    crate::flatten(p, 0.001).into_iter().flatten().collect()
}

// ---------- kształty parametryczne ----------

#[test]
fn req_02_008_regular_polygon() {
    let hex = regular_polygon(0., 0., 10., 6, 0.).unwrap();
    assert!(is_closed(&hex));
    assert!(close(path_length(&hex), 60.0, 1e-9), "bok sześciokąta = r");
    let p = pts(&hex);
    assert!(
        close(p[0].x, 10.0, 1e-9) && close(p[0].y, 0.0, 1e-9),
        "pierwszy wierzchołek na 0°"
    );
    let rotated = regular_polygon(0., 0., 10., 4, 45.).unwrap();
    let b = bbox(&rotated).unwrap();
    assert!(
        close(b.width(), 10.0 * 2f64.sqrt(), 1e-9),
        "kwadrat obrócony o 45° stoi na boku"
    );
    assert!(regular_polygon(0., 0., 10., 2, 0.).is_err());
    assert!(regular_polygon(0., 0., -1., 5, 0.).is_err());
}

#[test]
fn req_02_011_polygon_presets_triangle_pentagon_octagon() {
    for (n, side) in [
        (3u32, 2.0 * 10.0 * (PI / 3.0).sin()),
        (5, 2.0 * 10.0 * (PI / 5.0).sin()),
        (8, 2.0 * 10.0 * (PI / 8.0).sin()),
    ] {
        let p = regular_polygon(0., 0., 10., n, 90.).unwrap();
        assert!(close(path_length(&p), side * n as f64, 1e-9), "{n}-kąt");
        assert_eq!(
            crate::node_count(&p),
            n as usize,
            "{n} wierzchołków, domknięcie przez ClosePath"
        );
    }
}

#[test]
fn req_02_009_star() {
    let s = star(0., 0., 10., 4., 5, 90.).unwrap();
    assert!(is_closed(&s));
    let p = pts(&s);
    assert_eq!(p.len(), 11, "10 wierzchołków + domknięcie");
    let r: Vec<f64> = p[..10].iter().map(|q| q.to_vec2().hypot()).collect();
    for (i, v) in r.iter().enumerate() {
        assert!(
            close(*v, if i % 2 == 0 { 10.0 } else { 4.0 }, 1e-9),
            "{i}: {v}"
        );
    }
    assert!(close(p[0].y, 10.0, 1e-9), "pierwsze ramię w górę przy 90°");
    assert!(star(0., 0., 4., 10., 5, 0.).is_err(), "r_inner < r_outer");
}

#[test]
fn req_02_010_double_star() {
    let s = double_star(0., 0., 10., 7., 3., 4, 0.).unwrap();
    let p = pts(&s);
    assert_eq!(p.len(), 17, "4·4 wierzchołki + domknięcie");
    let r: Vec<f64> = p[..16].iter().map(|q| q.to_vec2().hypot()).collect();
    let expect = [10.0, 3.0, 7.0, 3.0];
    for (i, v) in r.iter().enumerate() {
        assert!(close(*v, expect[i % 4], 1e-9), "{i}: {v}");
    }
    assert!(
        double_star(0., 0., 10., 2., 3., 4, 0.).is_err(),
        "r_inner < r_middle < r_outer"
    );
}

#[test]
fn req_02_003_arc_segments() {
    let a = arc(0., 0., 10., 0., 90.).unwrap();
    assert!(!is_closed(&a));
    assert!(close(path_length(&a), PI * 10.0 / 2.0, 1e-4));
    let p = pts(&a);
    assert!(close(p[0].x, 10.0, 1e-9) && close(p.last().unwrap().y, 10.0, 1e-6));
    let cw = arc(0., 0., 10., 0., -180.).unwrap();
    assert!(
        bbox(&cw).unwrap().y0 < -9.99,
        "ujemna rozwartość = zgodnie z ruchem wskazówek zegara"
    );
    assert!(arc(0., 0., 10., 0., 0.).is_err());
}

#[test]
fn req_04_003_arc_through_three_points() {
    let (c, r, start, sweep) = arc_through(
        Point::new(10., 0.),
        Point::new(0., 10.),
        Point::new(-10., 0.),
    )
    .unwrap();
    assert!(close(c.x, 0.0, 1e-9) && close(c.y, 0.0, 1e-9) && close(r, 10.0, 1e-9));
    assert!(
        close(start, 0.0, 1e-9) && close(sweep, 180.0, 1e-9),
        "{start} {sweep}"
    );
    let (_, _, _, sweep_cw) = arc_through(
        Point::new(10., 0.),
        Point::new(0., -10.),
        Point::new(-10., 0.),
    )
    .unwrap();
    assert!(close(sweep_cw, -180.0, 1e-9));
    assert!(arc_through(Point::new(0., 0.), Point::new(1., 1.), Point::new(2., 2.)).is_none());
}

// ---------- booleany (REQ-19-001: zbiór trudnych przypadków) ----------

#[test]
fn req_04_064_union() {
    let r = boolean(&[sq(0., 0., 10.)], &[sq(5., 0., 10.)], BoolOp::Union).unwrap();
    assert_eq!(r.len(), 1);
    assert!(close(area(&r), 150.0, 1e-3));
    assert!(close(bbox(&r[0]).unwrap().width(), 15.0, 1e-3));
}

#[test]
fn req_04_065_difference_makes_hole_or_notch() {
    let notch = boolean(&[sq(0., 0., 10.)], &[sq(5., 5., 10.)], BoolOp::Difference).unwrap();
    assert!(close(area(&notch), 75.0, 1e-3));
    let hole = boolean(&[sq(0., 0., 10.)], &[sq(3., 3., 4.)], BoolOp::Difference).unwrap();
    assert!(close(area(&hole), 84.0, 1e-3));
    assert_eq!(
        hole.iter().map(subpath_count).sum::<usize>(),
        2,
        "obrys + otwór"
    );
}

#[test]
fn req_04_066_intersection() {
    let r = boolean(&[sq(0., 0., 10.)], &[sq(5., 5., 10.)], BoolOp::Intersection).unwrap();
    assert!(close(area(&r), 25.0, 1e-3));
    let none = boolean(&[sq(0., 0., 1.)], &[sq(5., 5., 1.)], BoolOp::Intersection).unwrap();
    assert!(none.is_empty());
}

#[test]
fn req_04_067_xor() {
    let r = boolean(&[sq(0., 0., 10.)], &[sq(5., 5., 10.)], BoolOp::Xor).unwrap();
    assert!(close(area(&r), 150.0, 1e-3));
}

#[test]
fn req_19_001_boolean_hard_cases() {
    // styczne krawędzie: suma dwóch kwadratów stykających się bokiem = prostokąt
    let touch = boolean(&[sq(0., 0., 10.)], &[sq(10., 0., 10.)], BoolOp::Union).unwrap();
    assert_eq!(touch.len(), 1, "styczne kwadraty łączą się w jeden kształt");
    assert!(close(area(&touch), 200.0, 1e-3));
    // wspólny tylko wierzchołek: dwa obszary, pole sumaryczne
    let corner = boolean(&[sq(0., 0., 10.)], &[sq(10., 10., 10.)], BoolOp::Union).unwrap();
    assert!(close(area(&corner), 200.0, 1e-3));
    // krzywe: koło minus koło współśrodkowe = pierścień
    let ring = boolean(
        &[crate::ellipse_path(0., 0., 10., 10.)],
        &[crate::ellipse_path(0., 0., 5., 5.)],
        BoolOp::Difference,
    )
    .unwrap();
    // spłaszczenie (0,001 mm) zaniża pole o ~⅔·tol·obwód: ≈0,04 obrys − 0,02 otwór
    assert!(close(area(&ring), PI * 75.0, 0.03), "{}", area(&ring));
    // kształt z otworem jako podmiot: even-odd
    let donut = parse_svg_path("M0 0 L20 0 L20 20 L0 20 Z M5 5 L15 5 L15 15 L5 15 Z").unwrap();
    let cut = boolean(
        std::slice::from_ref(&donut),
        &[sq(-1., 8., 22.)],
        BoolOp::Intersection,
    )
    .unwrap();
    // pas y 8..20 przez pierścień: 20·12 − 10·7
    assert!(close(area(&cut), 240.0 - 70.0, 1e-3));
    // samoprzecięcie (ósemka) – even-odd liczy oba płaty
    let bow = polyline_path(
        &[
            Point::new(0., 0.),
            Point::new(10., 10.),
            Point::new(10., 0.),
            Point::new(0., 10.),
        ],
        true,
    );
    let u = boolean(&[bow], &[], BoolOp::Union).unwrap();
    assert!(close(area(&u), 50.0, 1e-3));
    // zdegenerowany (zerowe pole) — pusty wynik, bez paniki
    let flat = polyline_path(
        &[Point::new(0., 0.), Point::new(10., 0.), Point::new(5., 0.)],
        true,
    );
    assert!(boolean(&[flat], &[], BoolOp::Union).unwrap().is_empty());
    // duże i małe współrzędne
    let big = boolean(
        &[sq(100_000., 100_000., 0.5)],
        &[sq(100_000.25, 100_000., 0.5)],
        BoolOp::Union,
    )
    .unwrap();
    assert!(close(area(&big), 0.375, 1e-4));
    // ścieżka otwarta -> błąd
    let open = polyline_path(&[Point::new(0., 0.), Point::new(1., 1.)], false);
    assert_eq!(
        boolean(&[open], &[sq(0., 0., 1.)], BoolOp::Union),
        Err(GeomError::OpenPath)
    );
}

#[test]
fn req_04_070_stacked_union_of_many() {
    let many: Vec<BezPath> = (0..5).map(|i| sq(i as f64 * 5.0, 0., 10.)).collect();
    let r = boolean(&many, &[], BoolOp::Union).unwrap();
    assert_eq!(r.len(), 1);
    assert!(close(area(&r), 30.0 * 10.0, 1e-3));
}

// ---------- offsety (REQ-19-002) ----------

#[test]
fn req_04_058_offset_outward() {
    let r = offset(&[sq(0., 0., 10.)], 1.0, Join::Miter).unwrap();
    let b = bbox(&r[0]).unwrap();
    assert!(close(b.x0, -1.0, 1e-3) && close(b.x1, 11.0, 1e-3));
    assert!(close(area(&r), 144.0, 1e-2), "ostre narożniki");
}

#[test]
fn req_04_057_offset_inward() {
    let r = offset(&[sq(0., 0., 10.)], -2.0, Join::Miter).unwrap();
    assert!(close(area(&r), 36.0, 1e-2));
    assert!(
        offset(&[sq(0., 0., 10.)], -6.0, Join::Miter)
            .unwrap()
            .is_empty(),
        "offset większy niż kształt znika"
    );
}

#[test]
fn req_04_061_offset_round_corners() {
    let r = offset(&[sq(0., 0., 10.)], 1.0, Join::Round).unwrap();
    assert!(close(area(&r), 100.0 + 40.0 + PI, 0.01), "{}", area(&r));
}

#[test]
fn req_04_062_offset_bevel_corners() {
    let r = offset(&[sq(0., 0., 10.)], 1.0, Join::Bevel).unwrap();
    // każdy narożnik traci trójkąt o polu 1/2 względem miter
    assert!(close(area(&r), 144.0 - 2.0, 0.01), "{}", area(&r));
}

#[test]
fn req_04_060_offset_miter_limit_on_sharp_angle() {
    // ostry kąt 10°: miter ograniczony (4×), a nie kolec na kilometr
    let spike = polyline_path(
        &[
            Point::new(0., 0.),
            Point::new(100., 0.),
            Point::new(0., 100f64 * (10f64).to_radians().tan()),
        ],
        true,
    );
    let r = offset(&[spike], 1.0, Join::Miter).unwrap();
    let b = bbox(&r[0]).unwrap();
    assert!(b.x1 < 106.0, "limit ostrza: {}", b.x1);
}

#[test]
fn req_19_002_offset_hard_cases() {
    // otwór rośnie do wewnątrz przy offsecie zewnętrznym
    let donut = parse_svg_path("M0 0 L20 0 L20 20 L0 20 Z M5 5 L15 5 L15 15 L5 15 Z").unwrap();
    let r = offset(&[donut], 1.0, Join::Miter).unwrap();
    assert!(
        close(area(&r), 22.0 * 22.0 - 8.0 * 8.0, 1e-2),
        "{}",
        area(&r)
    );
    // dwa kształty blisko siebie zlewają się po offsecie
    let merged = offset(&[sq(0., 0., 10.), sq(11., 0., 10.)], 1.0, Join::Miter).unwrap();
    assert_eq!(merged.iter().map(subpath_count).sum::<usize>(), 1);
    // krzywe
    let circle = offset(&[crate::ellipse_path(0., 0., 10., 10.)], 2.0, Join::Round).unwrap();
    assert!(close(area(&circle), PI * 144.0, 0.05), "{}", area(&circle));
    // kierunek ścieżki nie ma znaczenia
    let ccw = sq(0., 0., 10.);
    let cw = reverse(&ccw);
    assert!(close(
        area(&offset(&[cw], 1.0, Join::Miter).unwrap()),
        144.0,
        1e-2
    ));
    // otwarta -> błąd, delta 0 -> kopia
    assert_eq!(
        offset(
            &[polyline_path(
                &[Point::new(0., 0.), Point::new(1., 0.)],
                false
            )],
            1.0,
            Join::Round
        ),
        Err(GeomError::OpenPath)
    );
    assert!(close(
        area(&offset(&[sq(0., 0., 10.)], 0.0, Join::Round).unwrap()),
        100.0,
        1e-3
    ));
}

// ---------- operacje na ścieżkach ----------

#[test]
fn req_04_052_convex_hull() {
    let l = parse_svg_path("M0 0 L10 0 L10 2 L2 2 L2 10 L0 10 Z").unwrap();
    let h = convex_hull(&[l]).unwrap();
    assert!(is_closed(&h));
    assert!(
        close(area(&[h]), 100.0 - 32.0, 1e-6),
        "trójkątny ubytek 8·8/2"
    );
}

#[test]
fn req_04_031_reverse_direction() {
    let p = polyline_path(
        &[Point::new(0., 0.), Point::new(10., 0.), Point::new(10., 5.)],
        false,
    );
    let r = reverse(&p);
    let q = pts(&r);
    assert_eq!(q[0], Point::new(10., 5.));
    assert_eq!(*q.last().unwrap(), Point::new(0., 0.));
    assert!(close(path_length(&r), path_length(&p), 1e-12));
    assert!(same_geometry(&p, &r, 1e-9));
}

#[test]
fn req_04_030_set_start_point() {
    let s = sq(0., 0., 10.);
    let moved = set_start(&s, 2).unwrap();
    assert_eq!(pts(&moved)[0], Point::new(10., 10.));
    assert!(is_closed(&moved));
    assert!(close(path_length(&moved), 40.0, 1e-9));
    assert!(set_start(&s, 99).is_err());
}

#[test]
fn req_04_038_close_with_tolerance() {
    let almost = polyline_path(
        &[
            Point::new(0., 0.),
            Point::new(10., 0.),
            Point::new(10., 10.),
            Point::new(0.05, 0.02),
        ],
        false,
    );
    let (c, n) = close_with_tolerance(&almost, 0.1);
    assert_eq!(n, 1);
    assert!(is_closed(&c));
    let (_, n2) = close_with_tolerance(&almost, 0.01);
    assert_eq!(n2, 0, "poza tolerancją");
}

#[test]
fn req_04_037_auto_join_with_tolerance() {
    let a = polyline_path(&[Point::new(0., 0.), Point::new(10., 0.)], false);
    let b = polyline_path(&[Point::new(10.02, 0.), Point::new(10., 10.)], false);
    // c odwrócone: koniec przy początku a
    let c = polyline_path(&[Point::new(0., 10.), Point::new(0., 0.01)], false);
    let j = join_open(&[a, b, c], 0.05);
    assert_eq!(j.len(), 1, "{j:?}");
    assert!(close(path_length(&j[0]), 30.0, 0.05));
    let far = join_open(
        &[
            polyline_path(&[Point::new(0., 0.), Point::new(1., 0.)], false),
            polyline_path(&[Point::new(5., 0.), Point::new(6., 0.)], false),
        ],
        0.05,
    );
    assert_eq!(far.len(), 2);
}

#[test]
fn req_04_042_reduce_nodes() {
    let mut zig = Vec::new();
    for i in 0..=100 {
        zig.push(Point::new(i as f64, if i % 2 == 0 { 0.0 } else { 0.004 }));
    }
    let p = polyline_path(&zig, false);
    let r = reduce_nodes(&p, 0.01);
    assert_eq!(
        crate::node_count(&r),
        2,
        "drgania poniżej tolerancji znikają"
    );
    let l = parse_svg_path("M0 0 L10 0 L10 10").unwrap();
    assert_eq!(
        crate::node_count(&reduce_nodes(&l, 0.01)),
        3,
        "prawdziwy narożnik zostaje"
    );
}

#[test]
fn req_04_053_chamfer() {
    let s = sq(0., 0., 10.);
    let c = chamfer(&s, 1, 2.0).unwrap(); // narożnik (10,0)
    assert!(close(area(std::slice::from_ref(&c)), 100.0 - 2.0, 1e-9));
    assert_eq!(crate::node_count(&c), crate::node_count(&s) + 1);
    assert!(chamfer(&s, 1, 6.0).is_err(), "faza dłuższa niż połowa boku");
}

#[test]
fn req_04_054_fillet() {
    let s = sq(0., 0., 10.);
    let f = fillet(&s, 1, 2.0).unwrap();
    // narożnik traci kwadrat r² i zyskuje ćwiartkę koła; pole liczone po spłaszczeniu
    // z tolerancją 0,001 mm zaniża łuk o ~⅔·0,001·πr/2 ≈ 0,002 mm²
    assert!(
        close(area(std::slice::from_ref(&f)), 100.0 - 4.0 + PI, 5e-3),
        "{}",
        area(&[f])
    );
    assert!(fillet(&s, 1, 0.0).is_err());
}

#[test]
fn req_04_039_same_geometry_detects_duplicates() {
    let a = sq(0., 0., 10.);
    assert!(same_geometry(&a, &set_start(&a, 2).unwrap(), 1e-6));
    assert!(same_geometry(&a, &reverse(&a), 1e-6));
    assert!(!same_geometry(&a, &sq(0., 0., 10.5), 1e-6));
}

// ---------- ograniczenia rozmiaru (regresja z fuzzingu) ----------

#[test]
fn huge_shapes_have_bounded_segment_count() {
    let e = crate::ellipse_path(0., 0., 1e60, 1e60);
    assert!(e.elements().len() < 400, "{}", e.elements().len());
    let r = crate::rect_path(0., 0., 1e60, 1e60, 1e59);
    assert!(r.elements().len() < 400);
    let a = arc(0., 0., 1e60, 0., 270.).unwrap();
    assert!(a.elements().len() < 400);
    // małe kształty nadal dokładne
    let small = crate::ellipse_path(0., 0., 10., 10.);
    assert!((path_length(&small) - 2.0 * PI * 10.0).abs() < 1e-4);
}

#[test]
fn coordinate_limits() {
    assert!(crate::within_limits(&sq(0., 0., 10.)));
    assert!(crate::within_limits(&sq(-9.9e6, -9.9e6, 1.)));
    assert!(!crate::within_limits(&sq(0., 0., 2e7)));
    assert!(
        !crate::within_limits(&parse_svg_path("M0 0 C 0 1e60 1 1 1 0").unwrap()),
        "liczą się też punkty kontrolne"
    );
}
