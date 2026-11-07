use super::*;
use crate::{bbox, is_closed, parse_svg_path, rect_path, subpath_count};

fn p(x: f64, y: f64) -> Point {
    Point::new(x, y)
}

fn near(a: Point, b: Point) -> bool {
    a.distance(b) < 1e-9
}

fn pts(cs: &[Contour]) -> Vec<(f64, f64)> {
    cs.iter()
        .flat_map(|c| c.nodes.iter().map(|n| (n.p.x, n.p.y)))
        .collect()
}

fn square() -> Vec<Contour> {
    contours(&rect_path(0., 0., 10., 10., 0.))
}

#[test]
fn contours_round_trip_lines_curves_and_subpaths() {
    let sq = square();
    assert_eq!(sq.len(), 1);
    assert!(sq[0].closed);
    assert_eq!(
        pts(&sq),
        vec![(0., 0.), (10., 0.), (10., 10.), (0., 10.)],
        "bez duplikatu punktu zamknięcia"
    );
    assert!(sq[0]
        .nodes
        .iter()
        .all(|n| n.hin.is_none() && n.hout.is_none()));
    let d = "M0 0 C0 5 10 5 10 0 L20 0 M30 0 L40 0 L40 10 Z";
    let cs = contours(&parse_svg_path(d).unwrap());
    assert_eq!(cs.len(), 2);
    assert!(!cs[0].closed && cs[1].closed);
    assert_eq!(cs[0].nodes[0].hout, Some(p(0., 5.)));
    assert_eq!(cs[0].nodes[1].hin, Some(p(10., 5.)));
    assert_eq!(cs[0].nodes[1].hout, None, "odcinek prosty bez uchwytów");
    let back = to_path(&cs);
    assert_eq!(contours(&back), cs, "to_path ∘ contours = id");
    assert_eq!(subpath_count(&back), 2);
    assert_eq!(count(&cs), 6);
    assert_eq!(locate(&cs, 4), Some((1, 1)));
    assert_eq!(locate(&cs, 6), None);
    // gładkość
    let s = contours(&parse_svg_path("M0 0 C0 5 5 5 10 5 C15 5 20 5 20 0").unwrap());
    assert!(s[0].nodes[1].smooth());
    assert!(!sq[0].nodes[1].smooth());
}

#[test]
fn req_04_032_move_node_numerically_with_handles() {
    let mut cs = contours(&parse_svg_path("M0 0 C0 5 5 5 10 5 C15 5 20 5 20 0").unwrap());
    move_node(&mut cs, 1, p(10., 15.)).unwrap();
    let n = &cs[0].nodes[1];
    assert!(near(n.p, p(10., 15.)));
    assert!(
        near(n.hin.unwrap(), p(5., 15.)) && near(n.hout.unwrap(), p(15., 15.)),
        "uchwyty jadą z węzłem"
    );
    assert!(move_node(&mut cs, 9, p(0., 0.)).is_err());
}

#[test]
fn set_handle_keeps_smooth_node_smooth() {
    let mut cs = contours(&parse_svg_path("M0 0 C0 5 5 5 10 5 C15 5 20 5 20 0").unwrap());
    set_handle(&mut cs, 1, Side::Out, p(10., 8.)).unwrap();
    let n = &cs[0].nodes[1];
    assert!(near(n.hout.unwrap(), p(10., 8.)));
    // przeciwny uchwyt: ten sam kierunek (przeciwny), własna długość 5
    assert!(near(n.hin.unwrap(), p(10., 0.)), "{:?}", n.hin);
    assert!(n.smooth());
    // w narożniku uchwyty są niezależne
    let mut c2 = contours(&parse_svg_path("M0 0 C0 5 5 5 10 5 C10 9 20 5 20 0").unwrap());
    let before = c2[0].nodes[1].hin;
    set_handle(&mut c2, 1, Side::Out, p(12., 9.)).unwrap();
    assert_eq!(c2[0].nodes[1].hin, before);
}

#[test]
fn req_04_023_insert_node_keeps_shape() {
    let mut cs = square();
    let i = insert_at(&mut cs, p(4., 0.3)).unwrap();
    assert_eq!(i, 1);
    assert_eq!(pts(&cs)[1], (4., 0.));
    assert_eq!(count(&cs), 5);
    // na krzywej: kształt bez zmian (de Casteljau)
    let path = parse_svg_path("M0 0 C0 10 10 10 10 0").unwrap();
    let mut c = contours(&path);
    insert_at(&mut c, p(5., 8.)).unwrap();
    assert_eq!(count(&c), 3);
    let b0 = bbox(&path).unwrap();
    let b1 = bbox(&to_path(&c)).unwrap();
    assert!((b0.y1 - b1.y1).abs() < 1e-9 && (b0.x0 - b1.x0).abs() < 1e-9);
    assert!(
        near(c[0].nodes[1].p, p(5., 7.5)),
        "środek krzywej: {:?}",
        c[0].nodes[1].p
    );
    assert!(c[0].nodes[1].smooth());
}

#[test]
fn req_04_024_delete_nodes() {
    let mut cs = square();
    delete_nodes(&mut cs, &[2]).unwrap();
    assert_eq!(pts(&cs), vec![(0., 0.), (10., 0.), (0., 10.)]);
    assert!(cs[0].closed);
    delete_nodes(&mut cs, &[0, 1]).unwrap();
    assert!(cs.is_empty(), "kontur z jednym węzłem znika");
    assert!(delete_nodes(&mut square(), &[7]).is_err());
}

#[test]
fn req_04_025_break_at_node() {
    let mut cs = square();
    break_at(&mut cs, 2).unwrap();
    assert_eq!(cs.len(), 1);
    assert!(!cs[0].closed);
    assert_eq!(
        pts(&cs),
        vec![(10., 10.), (0., 10.), (0., 0.), (10., 0.), (10., 10.)],
        "otwarty od węzła do węzła"
    );
    // otwarty kontur dzieli się na dwa
    let mut open = contours(&parse_svg_path("M0 0 L10 0 L20 0 L30 0").unwrap());
    break_at(&mut open, 2).unwrap();
    assert_eq!(open.len(), 2);
    assert_eq!(
        pts(&open),
        vec![(0., 0.), (10., 0.), (20., 0.), (20., 0.), (30., 0.)]
    );
    // nożyczki (REQ-04-035): przecięcie w dowolnym punkcie
    let mut sc = square();
    cut_at(&mut sc, p(5., 10.2)).unwrap();
    assert!(!sc[0].closed);
    assert_eq!(pts(&sc).first(), Some(&(5., 10.)));
    assert_eq!(pts(&sc).last(), Some(&(5., 10.)));
}

#[test]
fn req_04_026_join_ends() {
    // ten sam kontur — zamknięcie
    let mut cs = contours(&parse_svg_path("M0 0 L10 0 L10 10 L0 0.001").unwrap());
    join_ends(&mut cs, 0, 3, 0.01).unwrap();
    assert!(cs[0].closed);
    assert_eq!(count(&cs), 3, "końce spawane w jeden węzeł");
    // różne kontury — scalenie z odcinkiem
    let mut two = contours(&parse_svg_path("M0 0 L10 0 M20 0 L30 0").unwrap());
    join_ends(&mut two, 1, 2, 0.01).unwrap();
    assert_eq!(two.len(), 1);
    assert_eq!(pts(&two), vec![(0., 0.), (10., 0.), (20., 0.), (30., 0.)]);
    // początek z początkiem — drugi kontur odwrócony
    let mut rev = contours(&parse_svg_path("M0 0 L10 0 M0 5 L10 5").unwrap());
    join_ends(&mut rev, 0, 2, 0.01).unwrap();
    assert_eq!(pts(&rev), vec![(10., 5.), (0., 5.), (0., 0.), (10., 0.)]);
    // węzeł środkowy nie jest końcem
    assert!(join_ends(
        &mut contours(&parse_svg_path("M0 0 L5 0 L10 0").unwrap()),
        1,
        2,
        0.01
    )
    .is_err());
}

#[test]
fn req_04_027_028_smooth_and_corner() {
    let mut cs = square();
    make_smooth(&mut cs, &[1]).unwrap();
    let n = &cs[0].nodes[1];
    assert!(n.smooth() && n.hin.is_some() && n.hout.is_some());
    // styczna równoległa do przekątnej (0,0)→(10,10)
    let t = n.hout.unwrap() - n.hin.unwrap();
    assert!((t.x - t.y).abs() < 1e-9, "{t:?}");
    make_corner(&mut cs, &[1]).unwrap();
    assert!(cs[0].nodes[1].hin.is_none() && cs[0].nodes[1].hout.is_none());
    assert_eq!(contours(&to_path(&cs)), square(), "z powrotem kwadrat");
}

#[test]
fn req_04_029_line_to_curve_and_back() {
    let mut cs = square();
    set_segment_kind(&mut cs, 0, true).unwrap();
    assert!(near(cs[0].nodes[0].hout.unwrap(), p(10. / 3., 0.)));
    assert!(near(cs[0].nodes[1].hin.unwrap(), p(20. / 3., 0.)));
    set_segment_kind(&mut cs, 0, false).unwrap();
    assert!(cs[0].nodes[0].hout.is_none() && cs[0].nodes[1].hin.is_none());
    // segment zamykający: ostatni węzeł → pierwszy
    set_segment_kind(&mut cs, 3, true).unwrap();
    assert!(cs[0].nodes[3].hout.is_some() && cs[0].nodes[0].hin.is_some());
    assert!(is_closed(&to_path(&cs)));
    assert!(
        set_segment_kind(
            &mut contours(&parse_svg_path("M0 0 L1 0").unwrap()),
            1,
            true
        )
        .is_err(),
        "otwarty: brak segmentu 1"
    );
}

#[test]
fn req_04_033_align_nodes() {
    let mut cs = contours(&parse_svg_path("M0 0 L10 2 L20 4").unwrap());
    align(&mut cs, &[0, 1, 2], Axis::Y, None).unwrap();
    assert_eq!(pts(&cs), vec![(0., 2.), (10., 2.), (20., 2.)]);
    align(&mut cs, &[0, 2], Axis::X, Some(5.)).unwrap();
    assert_eq!(pts(&cs), vec![(5., 2.), (10., 2.), (5., 2.)]);
}

#[test]
fn nearest_point_on_segments() {
    let cs = square();
    let (seg, t, q) = nearest(&cs, p(10.5, 4.)).unwrap();
    assert_eq!(seg, 1);
    assert!((t - 0.4).abs() < 1e-9);
    assert!(near(q, p(10., 4.)));
    let (seg, _, q) = nearest(&cs, p(-1., 5.)).unwrap();
    assert_eq!(seg, 3, "segment zamykający");
    assert!(near(q, p(0., 5.)));
}
