//! Testy poleceń M1.1 (przez rejestr, tak jak wywołuje je GUI i MCP).

use super::*;
use crate::inspect::Detail;
use bc_geom::ops;
use serde_json::{json, Value};

fn setup() -> (Registry, Session) {
    (Registry::builtin(), Session::default())
}

fn run(r: &Registry, s: &mut Session, id: &str, p: Value) -> Value {
    s.execute(r, id, p.clone())
        .unwrap_or_else(|e| panic!("{id} {p}: {e}"))
}

fn id_of(v: &Value) -> u64 {
    v["id"].as_u64().unwrap()
}

fn rect(r: &Registry, s: &mut Session, x: f64, y: f64, w: f64, h: f64) -> u64 {
    id_of(&run(
        r,
        s,
        "shape.rect",
        json!({"x": x, "y": y, "w": w, "h": h}),
    ))
}

fn bbox(s: &Session, id: u64) -> [f64; 4] {
    inspect::report(s, Detail::Summary)
        .unwrap()
        .objects
        .iter()
        .find(|o| o.id == id)
        .unwrap()
        .bbox
        .unwrap()
}

fn world(s: &Session, id: u64) -> bc_geom::BezPath {
    let mut out = Vec::new();
    let pa = s.doc.parent_affine(id).unwrap();
    s.doc
        .find(id)
        .unwrap()
        .world_paths(&s.doc, pa, &mut out)
        .unwrap();
    let mut p = bc_geom::BezPath::new();
    for (_, _, q) in out {
        for el in q.elements() {
            p.push(*el);
        }
    }
    p
}

fn area(s: &Session, id: u64) -> f64 {
    ops::area(&[world(s, id)])
}

fn close(a: f64, b: f64, eps: f64) -> bool {
    (a - b).abs() <= eps
}

// ---------- kształty ----------

#[test]
fn req_02_008_shape_polygon_command() {
    let (r, mut s) = setup();
    let id = id_of(&run(
        &r,
        &mut s,
        "shape.polygon",
        json!({"cx": 50, "cy": 50, "r": 10, "sides": 6, "layer": 2}),
    ));
    let o = s.doc.find(id).unwrap();
    assert_eq!((o.kind.type_name(), o.layer), ("polygon", 2));
    assert!(close(bbox(&s, id)[2] - bbox(&s, id)[0], 20.0, 1e-9));
    assert_eq!(
        s.execute(
            &r,
            "shape.polygon",
            json!({"cx": 0, "cy": 0, "r": 1, "sides": 2})
        )
        .unwrap_err()
        .code,
        "invalid_params"
    );
}

#[test]
fn req_02_009_010_shape_star_and_double_star() {
    let (r, mut s) = setup();
    let a = id_of(&run(
        &r,
        &mut s,
        "shape.star",
        json!({"cx": 0, "cy": 0, "r_outer": 10, "r_inner": 4, "points": 5}),
    ));
    let b = id_of(&run(
        &r,
        &mut s,
        "shape.star",
        json!({"cx": 30, "cy": 0, "r_outer": 10, "r_inner": 3, "r_middle": 6, "points": 4}),
    ));
    assert_eq!(bc_geom::node_count(&world(&s, a)), 10);
    assert_eq!(bc_geom::node_count(&world(&s, b)), 16);
}

#[test]
fn req_04_004_shape_arc_center_radius_angles() {
    let (r, mut s) = setup();
    let id = id_of(&run(
        &r,
        &mut s,
        "shape.arc",
        json!({"cx": 0, "cy": 0, "r": 10, "start": 0, "sweep": 90}),
    ));
    let b = bbox(&s, id);
    assert!(close(b[2], 10.0, 1e-6) && close(b[3], 10.0, 1e-6) && close(b[0], 0.0, 1e-6));
    assert!(!inspect::report(&s, Detail::Summary).unwrap().objects[0].closed);
}

#[test]
fn req_04_003_shape_arc_three_points() {
    let (r, mut s) = setup();
    let id = id_of(&run(
        &r,
        &mut s,
        "shape.arc",
        json!({"through": [[10, 0], [0, 10], [-10, 0]]}),
    ));
    assert!(
        matches!(s.doc.find(id).unwrap().kind, bc_doc::Kind::Arc { r, sweep, .. } if close(r, 10.0, 1e-9) && close(sweep, 180.0, 1e-9))
    );
    assert!(s
        .execute(
            &r,
            "shape.arc",
            json!({"through": [[0, 0], [1, 1], [2, 2]]})
        )
        .is_err());
}

// ---------- booleany ----------

#[test]
fn req_04_064_boolean_union_command() {
    let (r, mut s) = setup();
    let a = rect(&r, &mut s, 0., 0., 10., 10.);
    let b = rect(&r, &mut s, 5., 0., 10., 10.);
    let res = run(
        &r,
        &mut s,
        "path.boolean",
        json!({"ids": [a, b], "op": "union"}),
    );
    let ids: Vec<u64> = serde_json::from_value(res["ids"].clone()).unwrap();
    assert_eq!(ids.len(), 1);
    assert_eq!(s.doc.objects.len(), 1, "oryginały usunięte");
    assert!(close(area(&s, ids[0]), 150.0, 1e-3));
    assert_eq!(
        s.history.undo_labels(),
        vec!["shape.rect", "shape.rect", "path.boolean"]
    );
    run(&r, &mut s, "edit.undo", json!({}));
    assert_eq!(s.doc.objects.len(), 2);
}

#[test]
fn req_04_065_boolean_difference_first_minus_rest() {
    let (r, mut s) = setup();
    let a = rect(&r, &mut s, 0., 0., 10., 10.);
    let b = rect(&r, &mut s, 3., 3., 4., 4.);
    let c = rect(&r, &mut s, 8., 8., 4., 4.);
    let ids = run(
        &r,
        &mut s,
        "path.boolean",
        json!({"ids": [a, b, c], "op": "difference"}),
    )["ids"]
        .clone();
    let id = ids[0].as_u64().unwrap();
    assert!(close(area(&s, id), 100.0 - 16.0 - 4.0, 1e-3));
    assert_eq!(
        s.doc.find(id).unwrap().layer,
        0,
        "warstwa pierwszego obiektu"
    );
}

#[test]
fn req_04_066_067_boolean_intersection_and_xor() {
    let (r, mut s) = setup();
    let a = rect(&r, &mut s, 0., 0., 10., 10.);
    let b = rect(&r, &mut s, 5., 5., 10., 10.);
    let i = run(
        &r,
        &mut s,
        "path.boolean",
        json!({"ids": [a, b], "op": "intersection"}),
    )["ids"][0]
        .as_u64()
        .unwrap();
    assert!(close(area(&s, i), 25.0, 1e-3));
    let c = rect(&r, &mut s, 0., 0., 10., 10.);
    let d = rect(&r, &mut s, 5., 5., 10., 10.);
    let x = run(
        &r,
        &mut s,
        "path.boolean",
        json!({"ids": [c, d], "op": "xor"}),
    )["ids"]
        .clone();
    let total: f64 = x
        .as_array()
        .unwrap()
        .iter()
        .map(|v| area(&s, v.as_u64().unwrap()))
        .sum();
    assert!(close(total, 150.0, 1e-3));
}

#[test]
fn req_04_068_boolean_cut_shapes_keeps_all_pieces() {
    let (r, mut s) = setup();
    let a = rect(&r, &mut s, 0., 0., 10., 10.);
    let b = rect(&r, &mut s, 5., 0., 10., 10.);
    // wycięcie: kształty dzielą się na wszystkie rozłączne części nakładania
    let ids = run(
        &r,
        &mut s,
        "path.boolean",
        json!({"ids": [a, b], "op": "cut"}),
    )["ids"]
        .clone();
    let ids: Vec<u64> = serde_json::from_value(ids).unwrap();
    assert_eq!(ids.len(), 3, "lewa część, część wspólna, prawa część");
    let mut areas: Vec<f64> = ids.iter().map(|id| area(&s, *id)).collect();
    areas.sort_by(|x, y| x.partial_cmp(y).unwrap());
    for v in areas {
        assert!(close(v, 50.0, 1e-3));
    }
}

#[test]
fn req_04_070_stacked_boolean_on_many_objects() {
    let (r, mut s) = setup();
    let ids: Vec<u64> = (0..6)
        .map(|i| rect(&r, &mut s, i as f64 * 5.0, 0., 10., 10.))
        .collect();
    run(&r, &mut s, "selection.set", json!({"ids": ids}));
    let res = run(&r, &mut s, "path.boolean", json!({"op": "union"}));
    assert_eq!(res["ids"].as_array().unwrap().len(), 1);
    assert_eq!(
        s.history.undo_labels().last().unwrap(),
        "path.boolean",
        "jeden krok historii"
    );
}

#[test]
fn boolean_rejects_open_paths_and_single_object() {
    let (r, mut s) = setup();
    let a = rect(&r, &mut s, 0., 0., 10., 10.);
    let open = id_of(&run(
        &r,
        &mut s,
        "shape.path",
        json!({"points": [[0, 0], [5, 5]]}),
    ));
    let e = s
        .execute(&r, "path.boolean", json!({"ids": [a, open], "op": "union"}))
        .unwrap_err();
    assert_eq!(e.code, "invalid_params");
    assert!(e.message.contains(&open.to_string()), "{}", e.message);
    assert!(s
        .execute(&r, "path.boolean", json!({"ids": [a], "op": "difference"}))
        .is_err());
}

// ---------- offset ----------

#[test]
fn req_04_057_058_059_offset_directions() {
    let (r, mut s) = setup();
    let a = rect(&r, &mut s, 0., 0., 10., 10.);
    let out = run(
        &r,
        &mut s,
        "path.offset",
        json!({"ids": [a], "distance": 1, "direction": "outward", "corners": "sharp"}),
    )["ids"][0]
        .as_u64()
        .unwrap();
    assert!(close(area(&s, out), 144.0, 1e-2));
    assert!(s.doc.find(a).is_some(), "oryginał zostaje domyślnie");
    let inw = run(
        &r,
        &mut s,
        "path.offset",
        json!({"ids": [a], "distance": 1, "direction": "inward", "corners": "sharp"}),
    )["ids"][0]
        .as_u64()
        .unwrap();
    assert!(close(area(&s, inw), 64.0, 1e-2));
    let both = run(
        &r,
        &mut s,
        "path.offset",
        json!({"ids": [a], "distance": 1, "direction": "both", "corners": "sharp"}),
    )["ids"]
        .clone();
    assert_eq!(both.as_array().unwrap().len(), 2);
}

#[test]
fn req_04_060_061_062_offset_corners() {
    let (r, mut s) = setup();
    let a = rect(&r, &mut s, 0., 0., 10., 10.);
    let rd = run(
        &r,
        &mut s,
        "path.offset",
        json!({"ids": [a], "distance": 1, "corners": "round"}),
    )["ids"][0]
        .as_u64()
        .unwrap();
    assert!(close(area(&s, rd), 140.0 + std::f64::consts::PI, 0.01));
    let bv = run(
        &r,
        &mut s,
        "path.offset",
        json!({"ids": [a], "distance": 1, "corners": "bevel"}),
    )["ids"][0]
        .as_u64()
        .unwrap();
    assert!(close(area(&s, bv), 142.0, 0.01));
}

#[test]
fn req_04_063_offset_delete_original() {
    let (r, mut s) = setup();
    let a = rect(&r, &mut s, 0., 0., 10., 10.);
    run(
        &r,
        &mut s,
        "path.offset",
        json!({"ids": [a], "distance": 2, "delete_original": true}),
    );
    assert!(s.doc.find(a).is_none());
    assert_eq!(s.doc.objects.len(), 1);
}

// ---------- operacje na ścieżkach ----------

#[test]
fn req_04_036_convert_to_path_bakes_geometry() {
    let (r, mut s) = setup();
    let a = rect(&r, &mut s, 0., 0., 20., 10.);
    run(
        &r,
        &mut s,
        "object.transform",
        json!({"ids": [a], "translate": [5, 5], "rotate": 90}),
    );
    let before = bbox(&s, a);
    run(&r, &mut s, "path.convert", json!({"ids": [a]}));
    let o = s.doc.find(a).unwrap();
    assert_eq!(
        o.kind.type_name(),
        "path",
        "ten sam obiekt (id), nowa geometria"
    );
    assert_eq!(o.transform, bc_geom::Affine::IDENTITY.as_coeffs());
    let after = bbox(&s, a);
    for k in 0..4 {
        assert!(close(before[k], after[k], 1e-6));
    }
}

#[test]
fn req_04_031_030_reverse_and_set_start() {
    let (r, mut s) = setup();
    let a = id_of(&run(
        &r,
        &mut s,
        "shape.path",
        json!({"points": [[0, 0], [10, 0], [10, 10], [0, 10]], "closed": true}),
    ));
    run(&r, &mut s, "path.set_start", json!({"id": a, "node": 2}));
    let p = world(&s, a);
    assert_eq!(
        bc_geom::flatten(&p, 0.01)[0][0],
        bc_geom::Point::new(10., 10.)
    );
    run(&r, &mut s, "path.reverse", json!({"ids": [a]}));
    assert!(ops::same_geometry(&p, &world(&s, a), 1e-9));
}

#[test]
fn req_04_041_break_apart() {
    let (r, mut s) = setup();
    let a = id_of(&run(
        &r,
        &mut s,
        "shape.path",
        json!({"d": "M0 0 L10 0 L10 10 Z M20 0 L30 0 L30 10 Z", "layer": 3}),
    ));
    let ids = run(&r, &mut s, "path.break_apart", json!({"ids": [a]}))["ids"].clone();
    let ids: Vec<u64> = serde_json::from_value(ids).unwrap();
    assert_eq!(ids.len(), 2);
    assert!(ids.iter().all(|i| s.doc.find(*i).unwrap().layer == 3));
    assert!(s.doc.find(a).is_none());
}

#[test]
fn req_04_038_037_close_and_join_with_tolerance() {
    let (r, mut s) = setup();
    let a = id_of(&run(
        &r,
        &mut s,
        "shape.path",
        json!({"points": [[0, 0], [10, 0], [10, 10], [0.05, 0]]}),
    ));
    let res = run(
        &r,
        &mut s,
        "path.close",
        json!({"ids": [a], "tolerance": 0.1}),
    );
    assert_eq!(res["closed"], 1);
    assert!(bc_geom::is_closed(&world(&s, a)));
    let b = id_of(&run(
        &r,
        &mut s,
        "shape.path",
        json!({"points": [[50, 0], [60, 0]]}),
    ));
    let c = id_of(&run(
        &r,
        &mut s,
        "shape.path",
        json!({"points": [[60.01, 0], [60, 10]]}),
    ));
    let j = run(
        &r,
        &mut s,
        "path.join",
        json!({"ids": [b, c], "tolerance": 0.05}),
    );
    assert_eq!(j["ids"].as_array().unwrap().len(), 1);
    assert!(s.doc.find(b).is_none() && s.doc.find(c).is_none());
}

#[test]
fn req_04_039_remove_duplicates() {
    let (r, mut s) = setup();
    let a = rect(&r, &mut s, 0., 0., 10., 10.);
    let b = id_of(&run(
        &r,
        &mut s,
        "shape.path",
        json!({"points": [[10, 10], [0, 10], [0, 0], [10, 0]], "closed": true}),
    ));
    let c = rect(&r, &mut s, 20., 0., 10., 10.);
    run(&r, &mut s, "selection.all", json!({}));
    let res = run(&r, &mut s, "path.remove_duplicates", json!({}));
    assert_eq!(res["ids"], json!([b]), "usuwa późniejszy duplikat");
    assert!(s.doc.find(a).is_some() && s.doc.find(c).is_some());
}

#[test]
fn req_04_042_reduce_nodes_command() {
    let (r, mut s) = setup();
    let pts: Vec<[f64; 2]> = (0..=50)
        .map(|i| [i as f64, if i % 2 == 0 { 0.0 } else { 0.003 }])
        .collect();
    let a = id_of(&run(&r, &mut s, "shape.path", json!({"points": pts})));
    let res = run(
        &r,
        &mut s,
        "path.reduce_nodes",
        json!({"ids": [a], "tolerance": 0.01}),
    );
    assert_eq!(res["nodes_before"], 51);
    assert_eq!(res["nodes_after"], 2);
}

#[test]
fn req_04_052_convex_hull_command_creates_new_object() {
    let (r, mut s) = setup();
    let a = id_of(&run(
        &r,
        &mut s,
        "shape.path",
        json!({"d": "M0 0 L10 0 L10 2 L2 2 L2 10 L0 10 Z"}),
    ));
    let h = id_of(&run(&r, &mut s, "path.convex_hull", json!({"ids": [a]})));
    assert!(s.doc.find(a).is_some());
    assert!(close(area(&s, h), 68.0, 1e-6));
}

#[test]
fn req_04_053_054_chamfer_and_fillet_commands() {
    let (r, mut s) = setup();
    let a = id_of(&run(
        &r,
        &mut s,
        "shape.path",
        json!({"points": [[0, 0], [10, 0], [10, 10], [0, 10]], "closed": true}),
    ));
    run(
        &r,
        &mut s,
        "path.chamfer",
        json!({"id": a, "node": 1, "distance": 2}),
    );
    assert!(close(area(&s, a), 98.0, 1e-6));
    run(
        &r,
        &mut s,
        "path.fillet",
        json!({"id": a, "node": 3, "radius": 2}),
    );
    assert!(close(area(&s, a), 98.0 - 4.0 + std::f64::consts::PI, 5e-3));
    // na prostokącie parametrycznym: konwersja do ścieżki następuje automatycznie
    let b = rect(&r, &mut s, 20., 0., 10., 10.);
    run(
        &r,
        &mut s,
        "path.chamfer",
        json!({"id": b, "node": 0, "distance": 1}),
    );
    assert_eq!(s.doc.find(b).unwrap().kind.type_name(), "path");
}

// ---------- rozmieszczanie ----------

#[test]
fn req_04_086_align_to_selection() {
    let (r, mut s) = setup();
    let a = rect(&r, &mut s, 0., 0., 10., 10.);
    let b = rect(&r, &mut s, 30., 5., 20., 10.);
    run(
        &r,
        &mut s,
        "object.align",
        json!({"ids": [a, b], "mode": "right"}),
    );
    assert!(close(bbox(&s, a)[2], 50.0, 1e-9) && close(bbox(&s, b)[2], 50.0, 1e-9));
    run(
        &r,
        &mut s,
        "object.align",
        json!({"ids": [a, b], "mode": "vcenter"}),
    );
    assert!(close((bbox(&s, a)[1] + bbox(&s, a)[3]) / 2.0, 7.5, 1e-9));
}

#[test]
fn req_04_087_align_to_first_or_last() {
    let (r, mut s) = setup();
    let a = rect(&r, &mut s, 0., 0., 10., 10.);
    let b = rect(&r, &mut s, 30., 5., 20., 10.);
    run(
        &r,
        &mut s,
        "object.align",
        json!({"ids": [a, b], "mode": "left", "to": "last"}),
    );
    assert!(close(bbox(&s, a)[0], 30.0, 1e-9) && close(bbox(&s, b)[0], 30.0, 1e-9));
    run(
        &r,
        &mut s,
        "object.align",
        json!({"ids": [a, b], "mode": "top", "to": "first"}),
    );
    assert!(close(bbox(&s, b)[3], 10.0, 1e-9));
}

#[test]
fn req_04_088_align_to_page() {
    let (r, mut s) = setup();
    let a = rect(&r, &mut s, 10., 10., 20., 20.);
    run(
        &r,
        &mut s,
        "object.align",
        json!({"ids": [a], "mode": "hcenter", "to": "page"}),
    );
    run(
        &r,
        &mut s,
        "object.align",
        json!({"ids": [a], "mode": "vcenter", "to": "page"}),
    );
    let b = bbox(&s, a);
    assert!(
        close(b[0], 190.0, 1e-9) && close(b[1], 190.0, 1e-9),
        "{b:?}"
    );
}

#[test]
fn req_04_090_distribute_evenly() {
    let (r, mut s) = setup();
    let a = rect(&r, &mut s, 0., 0., 10., 10.);
    let b = rect(&r, &mut s, 12., 0., 10., 10.);
    let c = rect(&r, &mut s, 90., 0., 10., 10.);
    run(
        &r,
        &mut s,
        "object.distribute",
        json!({"ids": [a, b, c], "axis": "horizontal", "by": "centers"}),
    );
    assert!(
        close(bbox(&s, b)[0], 45.0, 1e-9),
        "środek b w połowie: {:?}",
        bbox(&s, b)
    );
    let d = rect(&r, &mut s, 0., 0., 10., 30.);
    let e = rect(&r, &mut s, 0., 40., 10., 5.);
    let f = rect(&r, &mut s, 0., 100., 10., 10.);
    run(
        &r,
        &mut s,
        "object.distribute",
        json!({"ids": [d, e, f], "axis": "vertical", "by": "gaps"}),
    );
    // przerwy równe: (110 - 30 - 5 - 10) / 2 = 32.5
    assert!(close(bbox(&s, e)[1], 62.5, 1e-9), "{:?}", bbox(&s, e));
}

#[test]
fn req_04_092_move_by_value() {
    let (r, mut s) = setup();
    let a = rect(&r, &mut s, 0., 0., 10., 10.);
    run(
        &r,
        &mut s,
        "object.transform",
        json!({"ids": [a], "translate": [2.5, -1]}),
    );
    assert_eq!(bbox(&s, a), [2.5, -1.0, 12.5, 9.0]);
}

#[test]
fn req_04_093_draw_order() {
    let (r, mut s) = setup();
    let a = rect(&r, &mut s, 0., 0., 1., 1.);
    let b = rect(&r, &mut s, 0., 0., 1., 1.);
    let c = rect(&r, &mut s, 0., 0., 1., 1.);
    let order = |s: &Session| s.doc.objects.iter().map(|o| o.id).collect::<Vec<_>>();
    run(
        &r,
        &mut s,
        "object.order",
        json!({"ids": [a], "to": "front"}),
    );
    assert_eq!(order(&s), vec![b, c, a]);
    run(
        &r,
        &mut s,
        "object.order",
        json!({"ids": [a], "to": "back"}),
    );
    assert_eq!(order(&s), vec![a, b, c]);
    run(
        &r,
        &mut s,
        "object.order",
        json!({"ids": [a], "to": "forward"}),
    );
    assert_eq!(order(&s), vec![b, a, c]);
    run(
        &r,
        &mut s,
        "object.order",
        json!({"ids": [c], "to": "backward"}),
    );
    assert_eq!(order(&s), vec![b, c, a]);
}

#[test]
fn req_04_080_081_equalize_width_and_height() {
    let (r, mut s) = setup();
    let a = rect(&r, &mut s, 0., 0., 20., 10.);
    let b = rect(&r, &mut s, 50., 0., 5., 5.);
    run(
        &r,
        &mut s,
        "object.equalize",
        json!({"ids": [a, b], "dimension": "width"}),
    );
    let bb = bbox(&s, b);
    assert!(
        close(bb[2] - bb[0], 20.0, 1e-9) && close(bb[3] - bb[1], 5.0, 1e-9),
        "bez proporcji: {bb:?}"
    );
    let c = rect(&r, &mut s, 0., 50., 10., 4.);
    run(
        &r,
        &mut s,
        "object.equalize",
        json!({"ids": [a, c], "dimension": "height", "keep_aspect": true}),
    );
    let cb = bbox(&s, c);
    assert!(
        close(cb[3] - cb[1], 10.0, 1e-9) && close(cb[2] - cb[0], 25.0, 1e-9),
        "{cb:?}"
    );
}

#[test]
fn req_04_056_mirror() {
    let (r, mut s) = setup();
    let a = id_of(&run(
        &r,
        &mut s,
        "shape.path",
        json!({"points": [[0, 0], [10, 0], [0, 5]], "closed": true}),
    ));
    run(
        &r,
        &mut s,
        "object.transform",
        json!({"ids": [a], "mirror": "horizontal"}),
    );
    let p = world(&s, a);
    let first = bc_geom::flatten(&p, 0.01)[0][0];
    assert_eq!(
        first,
        bc_geom::Point::new(10., 0.),
        "lustro względem środka zaznaczenia"
    );
    assert_eq!(bbox(&s, a), [0.0, 0.0, 10.0, 5.0]);
}

// ---------- tablice ----------

#[test]
fn req_04_071_072_grid_array_with_row_shift() {
    let (r, mut s) = setup();
    let a = rect(&r, &mut s, 0., 0., 10., 10.);
    let res = run(
        &r,
        &mut s,
        "array.grid",
        json!({"ids": [a], "columns": 3, "rows": 2, "dx": 15, "dy": 20, "row_shift": 5}),
    );
    let ids: Vec<u64> = serde_json::from_value(res["ids"].clone()).unwrap();
    assert_eq!(ids.len(), 5, "kopie bez oryginału");
    let mut xs: Vec<(f64, f64)> = ids
        .iter()
        .map(|i| (bbox(&s, *i)[0], bbox(&s, *i)[1]))
        .collect();
    xs.sort_by(|p, q| p.partial_cmp(q).unwrap());
    assert_eq!(
        xs,
        vec![
            (5.0, 20.0),
            (15.0, 0.0),
            (20.0, 20.0),
            (30.0, 0.0),
            (35.0, 20.0)
        ]
    );
    assert_eq!(s.history.undo_labels().last().unwrap(), "array.grid");
}

#[test]
fn req_04_077_circular_array() {
    let (r, mut s) = setup();
    let a = id_of(&run(
        &r,
        &mut s,
        "shape.ellipse",
        json!({"cx": 10, "cy": 0, "rx": 1, "ry": 1}),
    ));
    let res = run(
        &r,
        &mut s,
        "array.circular",
        json!({"ids": [a], "count": 4, "center": [0, 0]}),
    );
    let ids: Vec<u64> = serde_json::from_value(res["ids"].clone()).unwrap();
    assert_eq!(ids.len(), 3);
    let centers: Vec<(f64, f64)> = ids
        .iter()
        .map(|i| {
            let b = bbox(&s, *i);
            (
                ((b[0] + b[2]) / 2.0 * 1e6).round() / 1e6,
                ((b[1] + b[3]) / 2.0 * 1e6).round() / 1e6,
            )
        })
        .collect();
    assert!(
        centers.contains(&(0.0, 10.0))
            && centers.contains(&(-10.0, 0.0))
            && centers.contains(&(0.0, -10.0)),
        "{centers:?}"
    );
}

// ---------- pomiar ----------

#[test]
fn req_04_011_012_measure() {
    let (r, mut s) = setup();
    let d = run(
        &r,
        &mut s,
        "measure.distance",
        json!({"a": [0, 0], "b": [3, 4]}),
    );
    assert_eq!(d["distance"], 5.0);
    assert_eq!(d["dx"], 3.0);
    let a = run(
        &r,
        &mut s,
        "measure.angle",
        json!({"a": [10, 0], "vertex": [0, 0], "b": [0, 10]}),
    );
    assert!(close(a["degrees"].as_f64().unwrap(), 90.0, 1e-9));
    assert!(
        s.history.undo_labels().is_empty(),
        "pomiar nie zmienia dokumentu"
    );
}

// ---------- atrybuty i warstwy ----------

#[test]
fn req_02_029_030_032_object_attributes_command() {
    let (r, mut s) = setup();
    let a = rect(&r, &mut s, 0., 0., 1., 1.);
    run(
        &r,
        &mut s,
        "object.set",
        json!({"ids": [a], "note": "test", "priority": 3, "tags": ["x", "y"]}),
    );
    let o = s.doc.find(a).unwrap();
    assert_eq!(
        (o.note.as_deref(), o.priority, o.tags.len()),
        (Some("test"), 3, 2)
    );
}

#[test]
fn req_02_025_026_lock_position_and_lock_size() {
    let (r, mut s) = setup();
    let a = rect(&r, &mut s, 0., 0., 10., 10.);
    run(
        &r,
        &mut s,
        "object.set",
        json!({"ids": [a], "lock_size": true}),
    );
    run(
        &r,
        &mut s,
        "object.transform",
        json!({"ids": [a], "translate": [5, 0], "rotate": 45}),
    );
    assert_eq!(
        s.execute(&r, "object.transform", json!({"ids": [a], "scale": [2, 2]}))
            .unwrap_err()
            .code,
        "locked"
    );
    run(
        &r,
        &mut s,
        "object.set",
        json!({"ids": [a], "lock_size": false, "locked": true}),
    );
    assert_eq!(
        s.execute(
            &r,
            "object.transform",
            json!({"ids": [a], "translate": [1, 0]})
        )
        .unwrap_err()
        .code,
        "locked"
    );
}

#[test]
fn req_02_037_038_layer_color_and_name() {
    let (r, mut s) = setup();
    run(
        &r,
        &mut s,
        "layer.set",
        json!({"index": 5, "color": "#ABCDEF", "name": "Cięcie 3mm"}),
    );
    let l = s.doc.layer(5).unwrap();
    assert_eq!(
        (l.color.as_str(), l.name.as_str()),
        ("#abcdef", "Cięcie 3mm")
    );
}

#[test]
fn req_02_039_custom_palette_color() {
    let (r, mut s) = setup();
    let res = run(
        &r,
        &mut s,
        "layer.add",
        json!({"color": "#ff00aa", "name": "Mój róż"}),
    );
    assert_eq!(res["index"], bc_doc::FIRST_CUSTOM_INDEX);
    let res2 = run(&r, &mut s, "layer.add", json!({"color": "#00ff00"}));
    assert_eq!(res2["index"], bc_doc::FIRST_CUSTOM_INDEX + 1);
    let id = id_of(&run(
        &r,
        &mut s,
        "shape.rect",
        json!({"x": 0, "y": 0, "w": 1, "h": 1, "layer": 32}),
    ));
    assert_eq!(s.doc.find(id).unwrap().layer, 32);
    assert_eq!(
        s.execute(&r, "layer.add", json!({"color": "zielony"}))
            .unwrap_err()
            .code,
        "invalid_params"
    );
}

#[test]
fn absurd_coordinates_are_rejected() {
    let (r, mut s) = setup();
    for (id, p) in [
        (
            "shape.ellipse",
            json!({"cx": 0, "cy": 0, "rx": 1e60, "ry": 1}),
        ),
        ("shape.rect", json!({"x": 0, "y": 0, "w": 1e9, "h": 1})),
        (
            "shape.polygon",
            json!({"cx": 1e12, "cy": 0, "r": 1, "sides": 5}),
        ),
        ("shape.path", json!({"d": "M0 0 L1e30 0"})),
        (
            "shape.arc",
            json!({"cx": 0, "cy": 0, "r": 1e60, "start": 0, "sweep": 90}),
        ),
    ] {
        let e = s.execute(&r, id, p.clone()).unwrap_err();
        assert_eq!(e.code, "invalid_params", "{id} {p}");
    }
    let a = rect(&r, &mut s, 0., 0., 1., 1.);
    assert_eq!(
        s.execute(
            &r,
            "object.transform",
            json!({"ids": [a], "scale": [1e12, 1]})
        )
        .unwrap_err()
        .code,
        "invalid_params",
        "transformacja też nie może wyprowadzić poza zakres"
    );
    assert!(s.doc.objects.len() == 1);
}
