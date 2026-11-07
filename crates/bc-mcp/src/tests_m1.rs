//! Testy narzędzi MCP M1.1 (skróty nad poleceniami rejestru, §15.2).

use super::*;
use serde_json::json;

fn server() -> Server {
    let mut s = Server::new(Session::default(), Config::default());
    s.handle(json!({"jsonrpc": "2.0", "id": 0, "method": "initialize", "params": {"protocolVersion": "2025-06-18", "clientInfo": {"name": "t", "version": "1"}}}));
    s
}

fn call(s: &mut Server, name: &str, args: Value) -> Value {
    let r = s.handle(json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": {"name": name, "arguments": args}})).unwrap();
    assert!(r.get("error").is_none(), "{name}: {r}");
    r["result"].clone()
}

fn ok(s: &mut Server, name: &str, args: Value) -> Value {
    let r = call(s, name, args.clone());
    assert_eq!(
        r["isError"], false,
        "{name} {args}: {}",
        r["content"][0]["text"]
    );
    r["structuredContent"].clone()
}

fn new_id(v: &Value) -> u64 {
    v["result"]["id"]
        .as_u64()
        .or_else(|| v["id"].as_u64())
        .unwrap()
}

#[test]
fn m1_tools_are_listed_with_schemas() {
    let mut s = server();
    let r = s
        .handle(json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}))
        .unwrap();
    let tools = r["result"]["tools"].as_array().unwrap();
    for n in [
        "query_objects",
        "get_object",
        "draw_shape",
        "draw_path",
        "transform",
        "boolean",
        "offset",
        "array_grid",
        "array_circular",
        "align",
        "distribute",
        "set_layer",
        "assign_layer",
    ] {
        let t = tools
            .iter()
            .find(|t| t["name"] == n)
            .unwrap_or_else(|| panic!("brak narzędzia {n}"));
        assert_eq!(t["inputSchema"]["type"], "object");
        assert_eq!(t["outputSchema"]["type"], "object");
    }
}

#[test]
fn req_15_031_draw_shape() {
    let mut s = server();
    for (ty, params) in [
        ("rect", json!({"x": 0, "y": 0, "w": 10, "h": 5})),
        ("ellipse", json!({"cx": 0, "cy": 0, "rx": 3, "ry": 2})),
        ("polygon", json!({"cx": 0, "cy": 0, "r": 5, "sides": 6})),
        (
            "star",
            json!({"cx": 0, "cy": 0, "r_outer": 5, "r_inner": 2, "points": 5}),
        ),
        (
            "arc",
            json!({"cx": 0, "cy": 0, "r": 5, "start": 0, "sweep": 180}),
        ),
    ] {
        let v = ok(
            &mut s,
            "draw_shape",
            json!({"type": ty, "params": params, "layer": 4}),
        );
        let id = new_id(&v);
        assert_eq!(s.session().doc.find(id).unwrap().kind.type_name(), ty);
        assert_eq!(s.session().doc.find(id).unwrap().layer, 4);
    }
    assert_eq!(
        call(
            &mut s,
            "draw_shape",
            json!({"type": "trapez", "params": {}})
        )["isError"],
        true
    );
}

#[test]
fn req_15_032_draw_path() {
    let mut s = server();
    let v = ok(
        &mut s,
        "draw_path",
        json!({"points": [[0, 0], [10, 0], [10, 10]], "closed": true, "layer": 1}),
    );
    let id = new_id(&v);
    assert!(bc_geom::is_closed(
        &s.session().doc.find(id).unwrap().local_path().unwrap()
    ));
    let v2 = ok(&mut s, "draw_path", json!({"d": "M0 0 C 0 10 10 10 10 0"}));
    assert_eq!(s.session().doc.find(new_id(&v2)).unwrap().layer, 0);
}

#[test]
fn req_15_020_query_objects() {
    let mut s = server();
    let a = new_id(&ok(
        &mut s,
        "draw_shape",
        json!({"type": "rect", "params": {"x": 0, "y": 0, "w": 10, "h": 10}, "layer": 1}),
    ));
    let b = new_id(&ok(
        &mut s,
        "draw_shape",
        json!({"type": "ellipse", "params": {"cx": 100, "cy": 100, "rx": 5, "ry": 5}, "layer": 2}),
    ));
    ok(
        &mut s,
        "execute",
        json!({"id": "object.set", "params": {"ids": [b], "name": "Logo klienta", "tags": ["logo"]}}),
    );
    let ids = |v: Value| -> Vec<u64> {
        v["objects"]
            .as_array()
            .unwrap()
            .iter()
            .map(|o| o["id"].as_u64().unwrap())
            .collect()
    };
    assert_eq!(
        ids(ok(&mut s, "query_objects", json!({"layer": 1}))),
        vec![a]
    );
    assert_eq!(
        ids(ok(&mut s, "query_objects", json!({"type": "ellipse"}))),
        vec![b]
    );
    assert_eq!(
        ids(ok(
            &mut s,
            "query_objects",
            json!({"bbox": [90, 90, 200, 200]})
        )),
        vec![b]
    );
    assert_eq!(
        ids(ok(&mut s, "query_objects", json!({"text": "logo"}))),
        vec![b],
        "nazwa/tagi/notatka bez rozróżniania wielkości liter"
    );
    assert_eq!(ids(ok(&mut s, "query_objects", json!({}))).len(), 2);
}

#[test]
fn req_15_021_get_object() {
    let mut s = server();
    let a = new_id(&ok(
        &mut s,
        "draw_shape",
        json!({"type": "polygon", "params": {"cx": 10, "cy": 10, "r": 5, "sides": 4, "rotation": 45}}),
    ));
    let o = ok(&mut s, "get_object", json!({"id": a}));
    assert_eq!(o["object"]["type"], "polygon");
    assert_eq!(o["object"]["sides"], 4);
    assert!(o["report"]["bbox"].is_array());
    assert_eq!(
        call(&mut s, "get_object", json!({"id": 999}))["isError"],
        true
    );
}

#[test]
fn req_15_037_transform_tool() {
    let mut s = server();
    let a = new_id(&ok(
        &mut s,
        "draw_shape",
        json!({"type": "rect", "params": {"x": 0, "y": 0, "w": 10, "h": 10}}),
    ));
    ok(
        &mut s,
        "transform",
        json!({"ids": [a], "translate": [5, 0], "scale": [2, 1], "anchor": "origin"}),
    );
    let b = s
        .session()
        .doc
        .find(a)
        .unwrap()
        .world_bbox(&s.session().doc, bc_geom::Affine::IDENTITY)
        .unwrap()
        .unwrap();
    assert_eq!((b.x0, b.x1), (5.0, 25.0));
}

#[test]
fn req_15_038_boolean_tool() {
    let mut s = server();
    let a = new_id(&ok(
        &mut s,
        "draw_shape",
        json!({"type": "rect", "params": {"x": 0, "y": 0, "w": 10, "h": 10}}),
    ));
    let b = new_id(&ok(
        &mut s,
        "draw_shape",
        json!({"type": "rect", "params": {"x": 5, "y": 0, "w": 10, "h": 10}}),
    ));
    let v = ok(&mut s, "boolean", json!({"ids": [a, b], "op": "union"}));
    assert_eq!(v["ids"].as_array().unwrap().len(), 1);
    assert_eq!(s.session().doc.objects.len(), 1);
}

#[test]
fn req_15_039_offset_tool() {
    let mut s = server();
    let a = new_id(&ok(
        &mut s,
        "draw_shape",
        json!({"type": "rect", "params": {"x": 0, "y": 0, "w": 10, "h": 10}}),
    ));
    let v = ok(
        &mut s,
        "offset",
        json!({"ids": [a], "distance": 1, "direction": "outward", "corners": "round"}),
    );
    assert_eq!(v["ids"].as_array().unwrap().len(), 1);
    assert_eq!(s.session().doc.objects.len(), 2);
}

#[test]
fn req_15_040_041_array_tools() {
    let mut s = server();
    let a = new_id(&ok(
        &mut s,
        "draw_shape",
        json!({"type": "rect", "params": {"x": 0, "y": 0, "w": 5, "h": 5}}),
    ));
    let g = ok(
        &mut s,
        "array_grid",
        json!({"ids": [a], "columns": 4, "rows": 3, "dx": 10, "dy": 10}),
    );
    assert_eq!(g["ids"].as_array().unwrap().len(), 11);
    let c = ok(
        &mut s,
        "array_circular",
        json!({"ids": [a], "count": 8, "center": [50, 50]}),
    );
    assert_eq!(c["ids"].as_array().unwrap().len(), 7);
}

#[test]
fn req_15_042_043_align_and_distribute_tools() {
    let mut s = server();
    let a = new_id(&ok(
        &mut s,
        "draw_shape",
        json!({"type": "rect", "params": {"x": 0, "y": 0, "w": 10, "h": 10}}),
    ));
    let b = new_id(&ok(
        &mut s,
        "draw_shape",
        json!({"type": "rect", "params": {"x": 20, "y": 30, "w": 10, "h": 10}}),
    ));
    let c = new_id(&ok(
        &mut s,
        "draw_shape",
        json!({"type": "rect", "params": {"x": 100, "y": 0, "w": 10, "h": 10}}),
    ));
    ok(&mut s, "align", json!({"ids": [a, b, c], "mode": "bottom"}));
    ok(
        &mut s,
        "distribute",
        json!({"ids": [a, b, c], "axis": "horizontal"}),
    );
    let bb = s
        .session()
        .doc
        .find(b)
        .unwrap()
        .world_bbox(&s.session().doc, bc_geom::Affine::IDENTITY)
        .unwrap()
        .unwrap();
    assert_eq!((bb.x0, bb.y0), (50.0, 0.0));
}

#[test]
fn req_15_046_047_set_and_assign_layer_tools() {
    let mut s = server();
    let a = new_id(&ok(
        &mut s,
        "draw_shape",
        json!({"type": "rect", "params": {"x": 0, "y": 0, "w": 10, "h": 10}}),
    ));
    ok(
        &mut s,
        "set_layer",
        json!({"color": 6, "settings": {"name": "Cięcie", "speed": 15, "power_max": 80, "mode": "line"}}),
    );
    let l = s.session().doc.layer(6).unwrap().clone();
    assert_eq!(
        (l.name.as_str(), l.speed, l.power_max),
        ("Cięcie", 15.0, 80.0)
    );
    // kolor warstwy można też podać jako #rrggbb
    let hex = l.color.clone();
    ok(&mut s, "assign_layer", json!({"ids": [a], "color": hex}));
    assert_eq!(s.session().doc.find(a).unwrap().layer, 6);
    ok(&mut s, "assign_layer", json!({"ids": [a], "color": 2}));
    assert_eq!(s.session().doc.find(a).unwrap().layer, 2);
    assert_eq!(
        call(
            &mut s,
            "assign_layer",
            json!({"ids": [a], "color": "#010203"})
        )["isError"],
        true
    );
}

#[test]
fn m1_tools_are_audited_and_undoable() {
    let mut s = server();
    let a = new_id(&ok(
        &mut s,
        "draw_shape",
        json!({"type": "rect", "params": {"x": 0, "y": 0, "w": 10, "h": 10}}),
    ));
    ok(&mut s, "offset", json!({"ids": [a], "distance": 1}));
    ok(&mut s, "undo", json!({}));
    assert_eq!(s.session().doc.objects.len(), 1);
    assert!(s
        .audit
        .lock()
        .unwrap()
        .recent
        .iter()
        .any(|e| e["tool"] == "offset" && e["outcome"] == "ok"));
}
