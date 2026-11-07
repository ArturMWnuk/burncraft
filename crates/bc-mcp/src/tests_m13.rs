//! Testy MCP M1.3: open_file, save_file, import_file, export_file.

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
    let res = r["result"].clone();
    assert_eq!(
        res["isError"], false,
        "{name}: {}",
        res["content"][0]["text"]
    );
    res["structuredContent"].clone()
}

fn dir(n: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("bc-mcp-m13-{n}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

#[test]
fn m13_file_tools_listed() {
    let mut s = server();
    let r = s
        .handle(json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}))
        .unwrap();
    let names: Vec<String> = r["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap().to_string())
        .collect();
    for n in ["open_file", "save_file", "import_file", "export_file"] {
        assert!(names.contains(&n.to_string()), "brak {n}");
    }
}

#[test]
fn req_15_027_028_open_and_save_file() {
    let mut s = server();
    let d = dir("open");
    call(
        &mut s,
        "draw_shape",
        json!({"type": "rect", "params": {"x": 0, "y": 0, "w": 5, "h": 5}}),
    );
    let p = d.join("a.bcraft");
    let saved = call(&mut s, "save_file", json!({"path": p.to_str().unwrap()}));
    assert_eq!(saved["objects"], 1);
    call(&mut s, "execute", json!({"id": "document.new"}));
    let opened = call(&mut s, "open_file", json!({"path": p.to_str().unwrap()}));
    assert_eq!(opened["objects"], 1);
    assert_eq!(s.session().doc.objects.len(), 1);
}

#[test]
fn req_15_029_030_import_and_export_file() {
    let mut s = server();
    let d = dir("imp");
    let svg = d.join("in.svg");
    std::fs::write(&svg, r##"<svg xmlns="http://www.w3.org/2000/svg" width="50mm" height="50mm" viewBox="0 0 50 50"><circle cx="25" cy="25" r="5" fill="none" stroke="#000"/></svg>"##).unwrap();
    let imp = call(
        &mut s,
        "import_file",
        json!({"path": svg.to_str().unwrap(), "options": {"group": false}}),
    );
    assert_eq!(imp["ids"].as_array().unwrap().len(), 1);
    let out = d.join("out.dxf");
    let ex = call(
        &mut s,
        "export_file",
        json!({"path": out.to_str().unwrap(), "format": "dxf-r12"}),
    );
    assert_eq!(ex["format"], "dxf-r12");
    assert!(std::fs::read_to_string(&out).unwrap().contains("AC1009"));
    let audit: Vec<String> = s
        .audit
        .lock()
        .unwrap()
        .recent
        .iter()
        .filter_map(|e| e["tool"].as_str().map(String::from))
        .collect();
    let audit: Vec<&str> = audit.iter().map(String::as_str).collect();
    assert!(audit.contains(&"import_file") && audit.contains(&"export_file"));
}
