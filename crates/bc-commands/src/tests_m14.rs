//! Testy M1.4b2 na poziomie poleceń: edycja węzłów (node.*) i nożyczki.
//! Współrzędne węzłów w poleceniach są światowe (mm), także w grupach.

use super::*;
use serde_json::{json, Value};

fn run(r: &Registry, s: &mut Session, id: &str, p: Value) -> Value {
    s.execute(r, id, p.clone())
        .unwrap_or_else(|e| panic!("{id} {p}: {e}"))
}

fn rect(r: &Registry, s: &mut Session) -> u64 {
    run(
        r,
        s,
        "shape.rect",
        json!({"x": 0, "y": 0, "w": 10, "h": 10}),
    )["id"]
        .as_u64()
        .unwrap()
}

/// Węzły obiektu z node.list jako (x, y) zaokrąglone do 1 µm.
fn nodes(r: &Registry, s: &mut Session, id: u64) -> Vec<(f64, f64)> {
    let l = run(r, s, "node.list", json!({"id": id}));
    let mut out = Vec::new();
    for c in l["contours"].as_array().unwrap() {
        for n in c["nodes"].as_array().unwrap() {
            let q = |v: &Value| (v.as_f64().unwrap() * 1000.0).round() / 1000.0;
            out.push((q(&n["p"][0]), q(&n["p"][1])));
        }
    }
    out
}

#[test]
fn node_list_reports_contours_handles_and_smoothness() {
    let (r, mut s) = (Registry::builtin(), Session::default());
    let id = run(
        &r,
        &mut s,
        "shape.path",
        json!({"d": "M0 0 C0 5 5 5 10 5 C15 5 20 5 20 0"}),
    )["id"]
        .as_u64()
        .unwrap();
    let l = run(&r, &mut s, "node.list", json!({"id": id}));
    let c = &l["contours"][0];
    assert_eq!(c["closed"], false);
    assert_eq!(c["nodes"].as_array().unwrap().len(), 3);
    assert_eq!(c["nodes"][1]["smooth"], true);
    assert_eq!(c["nodes"][1]["hin"], json!([5.0, 5.0]));
    assert_eq!(c["nodes"][0]["hin"], Value::Null);
    // odczyt nie zmienia dokumentu ani historii
    assert!(
        !s.history.can_undo()
            || s.history.undo_labels().last().map(String::as_str) != Some("node.list")
    );
}

#[test]
fn req_04_032_node_move_converts_shape_and_undoes_in_one_step() {
    let (r, mut s) = (Registry::builtin(), Session::default());
    let id = rect(&r, &mut s);
    run(
        &r,
        &mut s,
        "node.move",
        json!({"id": id, "node": 2, "to": [12, 15]}),
    );
    assert_eq!(
        nodes(&r, &mut s, id),
        vec![(0., 0.), (10., 0.), (12., 15.), (0., 10.)]
    );
    assert!(
        matches!(s.doc.find(id).unwrap().kind, bc_doc::Kind::Path { .. }),
        "kształt zamieniony na ścieżkę"
    );
    run(
        &r,
        &mut s,
        "node.move",
        json!({"id": id, "node": 0, "by": [-1, -1]}),
    );
    assert_eq!(nodes(&r, &mut s, id)[0], (-1., -1.));
    run(&r, &mut s, "edit.undo", json!({}));
    run(&r, &mut s, "edit.undo", json!({}));
    assert!(
        matches!(s.doc.find(id).unwrap().kind, bc_doc::Kind::Rect { .. }),
        "undo przywraca prostokąt"
    );
    let e = s
        .execute(&r, "node.move", json!({"id": id, "node": 9, "to": [0, 0]}))
        .unwrap_err();
    assert_eq!(e.code, "invalid_params");
}

#[test]
fn node_commands_use_world_coordinates_inside_groups() {
    let (r, mut s) = (Registry::builtin(), Session::default());
    let a = rect(&r, &mut s);
    let b = run(
        &r,
        &mut s,
        "shape.rect",
        json!({"x": 20, "y": 0, "w": 10, "h": 10}),
    )["id"]
        .as_u64()
        .unwrap();
    run(&r, &mut s, "object.group", json!({"ids": [a, b]}));
    run(
        &r,
        &mut s,
        "object.transform",
        json!({"translate": [100, 50]}),
    );
    assert_eq!(nodes(&r, &mut s, a)[0], (100., 50.), "node.list w świecie");
    run(
        &r,
        &mut s,
        "node.move",
        json!({"id": a, "node": 0, "to": [95, 45]}),
    );
    assert_eq!(nodes(&r, &mut s, a)[0], (95., 45.));
    let bb = s
        .doc
        .find(a)
        .unwrap()
        .world_bbox(&s.doc, s.doc.parent_affine(a).unwrap())
        .unwrap()
        .unwrap();
    assert!((bb.x0 - 95.0).abs() < 1e-9 && (bb.y0 - 45.0).abs() < 1e-9);
}

#[test]
fn req_04_023_024_node_add_and_delete() {
    let (r, mut s) = (Registry::builtin(), Session::default());
    let id = rect(&r, &mut s);
    let res = run(&r, &mut s, "node.add", json!({"id": id, "at": [4, 0.2]}));
    assert_eq!(res["node"], 1);
    assert_eq!(nodes(&r, &mut s, id)[1], (4., 0.));
    run(
        &r,
        &mut s,
        "node.delete",
        json!({"id": id, "nodes": [1, 3]}),
    );
    assert_eq!(nodes(&r, &mut s, id), vec![(0., 0.), (10., 0.), (0., 10.)]);
    // usunięcie wszystkich węzłów usuwa obiekt
    run(
        &r,
        &mut s,
        "node.delete",
        json!({"id": id, "nodes": [0, 1, 2]}),
    );
    assert!(s.doc.find(id).is_none());
}

#[test]
fn req_04_025_026_and_req_04_035_break_join_and_scissors() {
    let (r, mut s) = (Registry::builtin(), Session::default());
    let id = rect(&r, &mut s);
    run(&r, &mut s, "node.break", json!({"id": id, "node": 2}));
    let l = run(&r, &mut s, "node.list", json!({"id": id}));
    assert_eq!(l["contours"][0]["closed"], false);
    run(&r, &mut s, "node.join", json!({"id": id, "a": 0, "b": 4}));
    let l = run(&r, &mut s, "node.list", json!({"id": id}));
    assert_eq!(l["contours"][0]["closed"], true);
    assert_eq!(nodes(&r, &mut s, id).len(), 4);
    run(&r, &mut s, "path.cut", json!({"id": id, "at": [5, 10.3]}));
    let n = nodes(&r, &mut s, id);
    assert_eq!((n[0], *n.last().unwrap()), ((5., 10.), (5., 10.)));
}

#[test]
fn req_04_027_028_029_smooth_corner_and_segment_kind() {
    let (r, mut s) = (Registry::builtin(), Session::default());
    let id = rect(&r, &mut s);
    run(&r, &mut s, "node.smooth", json!({"id": id, "nodes": [1]}));
    assert_eq!(
        run(&r, &mut s, "node.list", json!({"id": id}))["contours"][0]["nodes"][1]["smooth"],
        true
    );
    run(&r, &mut s, "node.corner", json!({"id": id, "nodes": [1]}));
    assert_eq!(
        run(&r, &mut s, "node.list", json!({"id": id}))["contours"][0]["nodes"][1]["hin"],
        Value::Null
    );
    run(
        &r,
        &mut s,
        "node.segment",
        json!({"id": id, "segment": 0, "kind": "curve"}),
    );
    let l = run(&r, &mut s, "node.list", json!({"id": id}));
    assert!(l["contours"][0]["nodes"][0]["hout"].is_array());
    run(
        &r,
        &mut s,
        "node.set_handle",
        json!({"id": id, "node": 0, "side": "out", "to": [3, -4]}),
    );
    let l = run(&r, &mut s, "node.list", json!({"id": id}));
    assert_eq!(l["contours"][0]["nodes"][0]["hout"], json!([3.0, -4.0]));
}

#[test]
fn req_04_033_node_align() {
    let (r, mut s) = (Registry::builtin(), Session::default());
    let id = run(
        &r,
        &mut s,
        "shape.path",
        json!({"points": [[0, 0], [10, 2], [20, 4]]}),
    )["id"]
        .as_u64()
        .unwrap();
    run(
        &r,
        &mut s,
        "node.align",
        json!({"id": id, "nodes": [0, 1, 2], "axis": "y"}),
    );
    assert_eq!(nodes(&r, &mut s, id), vec![(0., 2.), (10., 2.), (20., 2.)]);
}

#[test]
fn node_commands_respect_lock() {
    let (r, mut s) = (Registry::builtin(), Session::default());
    let id = rect(&r, &mut s);
    run(
        &r,
        &mut s,
        "object.set",
        json!({"ids": [id], "locked": true}),
    );
    let e = s
        .execute(&r, "node.move", json!({"id": id, "node": 0, "to": [1, 1]}))
        .unwrap_err();
    assert_eq!(e.code, "locked");
}

// ---------- M1.4b2b: przycinanie, gumka, wypaczanie ----------

fn bbox_of(s: &Session, id: u64) -> bc_geom::Rect {
    s.doc
        .find(id)
        .unwrap()
        .world_bbox(&s.doc, s.doc.parent_affine(id).unwrap())
        .unwrap()
        .unwrap()
}

#[test]
fn req_04_034_path_trim_cuts_against_other_objects() {
    let (r, mut s) = (Registry::builtin(), Session::default());
    let sq = rect(&r, &mut s);
    run(
        &r,
        &mut s,
        "shape.path",
        json!({"points": [[5, -5], [5, 15]]}),
    );
    run(&r, &mut s, "path.trim", json!({"id": sq, "at": [0, 5]}));
    let b = bbox_of(&s, sq);
    assert!(
        (b.x0 - 5.0).abs() < 1e-9 && (b.x1 - 10.0).abs() < 1e-9,
        "{b:?}"
    );
    // obiekty pomocnicze (prowadnice) nie tną
    let (r2, mut s2) = (Registry::builtin(), Session::default());
    let sq2 = rect(&r2, &mut s2);
    run(
        &r2,
        &mut s2,
        "shape.guide",
        json!({"p0": [5, -5], "p1": [5, 15]}),
    );
    assert_eq!(
        s2.execute(&r2, "path.trim", json!({"id": sq2, "at": [0, 5]}))
            .unwrap_err()
            .code,
        "invalid_params"
    );
}

#[test]
fn req_04_021_path_erase_splits_and_removes() {
    let (r, mut s) = (Registry::builtin(), Session::default());
    let l = run(
        &r,
        &mut s,
        "shape.path",
        json!({"points": [[0, 0], [30, 0]]}),
    )["id"]
        .as_u64()
        .unwrap();
    let small = run(
        &r,
        &mut s,
        "shape.path",
        json!({"points": [[0, 20], [2, 20]]}),
    )["id"]
        .as_u64()
        .unwrap();
    let res = run(
        &r,
        &mut s,
        "path.erase",
        json!({"points": [[15, -5], [15, 5]], "width": 2}),
    );
    assert_eq!(
        res["ids"],
        json!([l]),
        "zmienione tylko obiekty pod pociągnięciem"
    );
    let n = run(&r, &mut s, "node.list", json!({"id": l}));
    assert_eq!(n["contours"].as_array().unwrap().len(), 2);
    run(
        &r,
        &mut s,
        "path.erase",
        json!({"ids": [small], "points": [[-1, 20], [3, 20]], "width": 4}),
    );
    assert!(
        s.doc.find(small).is_none(),
        "wymazany w całości obiekt znika"
    );
    run(&r, &mut s, "edit.undo", json!({}));
    assert!(s.doc.find(small).is_some());
}

#[test]
fn req_04_045_path_warp_four_points_one_undo_step() {
    let (r, mut s) = (Registry::builtin(), Session::default());
    let id = rect(&r, &mut s);
    run(
        &r,
        &mut s,
        "path.warp",
        json!({"ids": [id], "cols": 1, "rows": 1,
        "points": [[0, 0], [20, 0], [0, 10], [20, 10]]}),
    );
    let b = bbox_of(&s, id);
    assert!((b.x1 - 20.0).abs() < 1e-9, "{b:?}");
    run(&r, &mut s, "edit.undo", json!({}));
    assert!((bbox_of(&s, id).x1 - 10.0).abs() < 1e-9);
    // domyślne bounds = ramka zaznaczenia; zła liczba punktów = błąd
    let e = s
        .execute(
            &r,
            "path.warp",
            json!({"ids": [id], "cols": 2, "rows": 2, "points": [[0, 0]]}),
        )
        .unwrap_err();
    assert_eq!(e.code, "invalid_params");
}
