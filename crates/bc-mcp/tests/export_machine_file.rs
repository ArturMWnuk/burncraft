//! Narzędzie MCP export_machine_file {driver, path} (REQ-15-057) i podgląd programu
//! G-code (render_preview source=gcode, REQ-02-089) przez klienta JSON-RPC.

use bc_commands::Session;
use bc_mcp::{Config, Server};
use serde_json::{json, Value};

fn server() -> Server {
    let mut s = Server::new(Session::default(), Config::default());
    s.handle(json!({"jsonrpc": "2.0", "id": 0, "method": "initialize", "params": {"protocolVersion": "2025-06-18", "clientInfo": {"name": "t", "version": "1"}}}));
    s
}

fn raw(s: &mut Server, name: &str, args: Value) -> Value {
    s.handle(json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": {"name": name, "arguments": args}}))
        .unwrap()
}

fn call(s: &mut Server, name: &str, args: Value) -> Value {
    let res = raw(s, name, args)["result"].clone();
    assert_eq!(
        res["isError"], false,
        "{name}: {}",
        res["content"][0]["text"]
    );
    res["structuredContent"].clone()
}

fn tmp(name: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("bc-mcp-emf-{}", std::process::id()));
    std::fs::create_dir_all(&d).unwrap();
    d.join(name)
}

#[test]
fn req_15_057_export_machine_file_tool() {
    let mut s = server();
    // narzędzie w tools/list ze schematem
    let list = s
        .handle(json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}))
        .unwrap();
    let tool = list["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["name"] == "export_machine_file")
        .expect("export_machine_file w tools/list")
        .clone();
    assert_eq!(tool["inputSchema"]["required"], json!(["driver", "path"]));
    // scenariusz: projekt → plan → eksport G-code
    call(
        &mut s,
        "draw_shape",
        json!({"type": "rect", "params": {"x": 10, "y": 10, "w": 100, "h": 60}, "layer": 0}),
    );
    let plan = call(&mut s, "plan_job", json!({}));
    assert!(plan["cut_length"].as_f64().unwrap() > 300.0);
    for (driver, file) in [("grbl", "pudelko.gcode"), ("gcode", "pudelko.nc")] {
        let path = tmp(file);
        let r = call(
            &mut s,
            "export_machine_file",
            json!({"driver": driver, "path": path.to_str().unwrap()}),
        );
        assert_eq!(r["driver"], driver);
        let text = std::fs::read_to_string(&path).unwrap();
        assert_eq!(r["bytes"].as_u64().unwrap() as usize, text.len());
        assert!(text.contains("G1 X110"), "{text}");
        assert_eq!(text.contains("M4"), driver == "grbl");
    }
    // sterownik spoza M2.2 i brak parametrów
    let r = raw(
        &mut s,
        "export_machine_file",
        json!({"driver": "ruida", "path": tmp("x.rd").to_str().unwrap()}),
    );
    assert_eq!(r["result"]["isError"], true);
    assert!(r["result"]["content"][0]["text"]
        .as_str()
        .unwrap()
        .contains("unsupported_driver"));
    let r = raw(&mut s, "export_machine_file", json!({"path": "x.gcode"}));
    assert!(
        r.get("error").is_some() || r["result"]["isError"] == true,
        "{r}"
    );
}

#[test]
fn req_02_089_mcp_import_and_preview_gcode() {
    let mut s = server();
    let path = tmp("podglad.gcode");
    std::fs::write(
        &path,
        "G21 G90\nM4 S0\nG0 X10 Y10\nG1 X60 S800 F1200\nY40\nM5\n",
    )
    .unwrap();
    let r = call(
        &mut s,
        "execute",
        json!({"id": "gcode.import", "params": {"path": path.to_str().unwrap(), "to_document": false}}),
    );
    assert!((r["result"]["cut_length"].as_f64().unwrap() - 80.0).abs() < 1e-6);
    let res = raw(
        &mut s,
        "render_preview",
        json!({"mode": "toolpath", "source": "gcode", "max_size": 256}),
    )["result"]
        .clone();
    assert_eq!(res["isError"], false, "{res}");
    assert_eq!(res["content"][0]["mimeType"], "image/png");
    // bez wczytanego programu — błąd
    let mut s2 = server();
    let res = raw(
        &mut s2,
        "render_preview",
        json!({"mode": "toolpath", "source": "gcode"}),
    )["result"]
        .clone();
    assert_eq!(res["isError"], true);
}
