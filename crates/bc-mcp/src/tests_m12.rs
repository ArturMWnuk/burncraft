//! Testy MCP M1.2: zakładki dokumentów, schowek między dokumentami, autozapis,
//! polityka dla preferencji, klony i tablice wirtualne przez MCP.

use super::*;
use serde_json::json;

fn server_with(cfg: Config) -> Server {
    let mut s = Server::new(Session::default(), cfg);
    s.handle(json!({"jsonrpc": "2.0", "id": 0, "method": "initialize", "params": {"protocolVersion": "2025-06-18", "clientInfo": {"name": "t", "version": "1"}}}));
    s
}

fn exec(s: &mut Server, id: &str, params: Value) -> Value {
    let r = s.handle(json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": {"name": "execute", "arguments": {"id": id, "params": params}}})).unwrap();
    r["result"].clone()
}

fn ok(s: &mut Server, id: &str, params: Value) -> Value {
    let r = exec(s, id, params.clone());
    assert_eq!(
        r["isError"], false,
        "{id} {params}: {}",
        r["content"][0]["text"]
    );
    r["structuredContent"]["result"].clone()
}

#[test]
fn req_02_060_tabs_through_mcp() {
    let mut s = server_with(Config::default());
    ok(
        &mut s,
        "shape.rect",
        json!({"x": 0, "y": 0, "w": 10, "h": 10}),
    );
    ok(&mut s, "doc.new", json!({"name": "B"}));
    assert_eq!(
        s.session().doc.objects.len(),
        0,
        "session() = aktywna zakładka"
    );
    let r = s.handle(json!({"jsonrpc": "2.0", "id": 2, "method": "tools/call", "params": {"name": "inspect_document", "arguments": {}}})).unwrap();
    assert_eq!(r["result"]["structuredContent"]["name"], "B");
    ok(&mut s, "doc.switch", json!({"index": 0}));
    let doc = s.handle(json!({"jsonrpc": "2.0", "id": 3, "method": "resources/read", "params": {"uri": "burncraft://document"}})).unwrap();
    let text = doc["result"]["contents"][0]["text"].as_str().unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(text).unwrap()["stats"]["objects"],
        1
    );
}

#[test]
fn req_02_061_copy_paste_between_tabs_through_mcp() {
    let mut s = server_with(Config::default());
    let a = ok(
        &mut s,
        "shape.ellipse",
        json!({"cx": 50, "cy": 50, "rx": 5, "ry": 5}),
    )["id"]
        .as_u64()
        .unwrap();
    ok(&mut s, "edit.copy", json!({"ids": [a]}));
    ok(&mut s, "doc.new", json!({}));
    let pasted = ok(&mut s, "edit.paste", json!({}));
    assert_eq!(pasted["ids"].as_array().unwrap().len(), 1);
    assert_eq!(s.session().doc.objects[0].kind.type_name(), "ellipse");
}

#[test]
fn preferences_changes_need_confirmation_over_mcp() {
    let dir = std::env::temp_dir().join(format!("bc-mcp-prefs-{}", std::process::id()));
    let mut s = server_with(Config {
        prefs_dir: Some(dir),
        ..Default::default()
    });
    let r = exec(
        &mut s,
        "prefs.set",
        json!({"key": "ui.theme", "value": "dark"}),
    );
    assert_eq!(r["isError"], true);
    assert!(r["content"][0]["text"]
        .as_str()
        .unwrap()
        .contains("confirmation_required"));
    let g = ok(&mut s, "prefs.get", json!({}));
    assert!(g["value"].is_object(), "odczyt preferencji jest dozwolony");
}

#[test]
fn req_02_054_server_autosaves_after_changes() {
    let dir = std::env::temp_dir().join(format!("bc-mcp-autosave-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let cfg = Config {
        autosave: Some(bc_commands::workbench::Autosave {
            dir: dir.clone(),
            interval: std::time::Duration::ZERO,
        }),
        ..Default::default()
    };
    let mut s = server_with(cfg);
    ok(
        &mut s,
        "shape.rect",
        json!({"x": 0, "y": 0, "w": 10, "h": 10}),
    );
    ok(
        &mut s,
        "shape.rect",
        json!({"x": 20, "y": 0, "w": 10, "h": 10}),
    );
    let files: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|x| x == "bcraft"))
        .collect();
    assert_eq!(files.len(), 1, "jeden plik autozapisu na dokument");
    let own = ok(&mut s, "file.recover_list", json!({}));
    assert_eq!(
        own["files"].as_array().unwrap().len(),
        0,
        "autozapis otwartego dokumentu to nie pozostałość po awarii"
    );
    // nowa sesja (jak po restarcie programu) widzi plik do odzyskania
    let cfg2 = Config {
        autosave: Some(bc_commands::workbench::Autosave {
            dir: dir.clone(),
            interval: std::time::Duration::ZERO,
        }),
        ..Default::default()
    };
    let mut s2 = server_with(cfg2);
    let list = ok(&mut s2, "file.recover_list", json!({}));
    assert_eq!(list["files"].as_array().unwrap().len(), 1);
}

#[test]
fn clones_and_virtual_arrays_through_mcp() {
    let mut s = server_with(Config::default());
    let a = ok(
        &mut s,
        "shape.rect",
        json!({"x": 0, "y": 0, "w": 10, "h": 10}),
    )["id"]
        .as_u64()
        .unwrap();
    let c = ok(
        &mut s,
        "object.clone",
        json!({"ids": [a], "offset": [50, 0]}),
    )["ids"][0]
        .as_u64()
        .unwrap();
    let q = s.handle(json!({"jsonrpc": "2.0", "id": 4, "method": "tools/call", "params": {"name": "query_objects", "arguments": {"type": "clone"}}})).unwrap();
    assert_eq!(q["result"]["structuredContent"]["objects"][0]["id"], c);
    let arr = ok(&mut s, "array.virtual", json!({"ids": [a], "layout": {"kind": "grid", "columns": 2, "rows": 2, "dx": 15, "dy": 15}}))["id"].as_u64().unwrap();
    let png = s.handle(json!({"jsonrpc": "2.0", "id": 5, "method": "tools/call", "params": {"name": "render_preview", "arguments": {"max_size": 64}}})).unwrap();
    assert_eq!(png["result"]["isError"], false);
    assert_eq!(s.session().doc.paths_of(arr).unwrap().len(), 4);
}
