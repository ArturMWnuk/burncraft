use super::*;
use serde_json::json;

fn server() -> Server {
    let mut s = Server::new(Session::default(), Config::default());
    let r = s
        .handle(json!({"jsonrpc": "2.0", "id": 0, "method": "initialize",
            "params": {"protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": {"name": "test-client", "version": "1"}}}))
        .unwrap();
    assert_eq!(r["result"]["protocolVersion"], "2025-06-18");
    assert!(s
        .handle(json!({"jsonrpc": "2.0", "method": "notifications/initialized"}))
        .is_none());
    s
}

fn call(s: &mut Server, name: &str, args: Value) -> Value {
    let r = s.handle(json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": {"name": name, "arguments": args}})).unwrap();
    assert!(r.get("error").is_none(), "{r}");
    r["result"].clone()
}

#[test]
fn req_15_011_instructions_describe_units_flow_safety() {
    let mut s = Server::new(Session::default(), Config::default());
    let r = s.handle(json!({"jsonrpc": "2.0", "id": 0, "method": "initialize", "params": {"protocolVersion": "1999-01-01"}})).unwrap();
    assert_eq!(
        r["result"]["protocolVersion"], PROTOCOL_VERSIONS[0],
        "nieznana wersja -> najnowsza obsługiwana"
    );
    let i = r["result"]["instructions"].as_str().unwrap();
    for needle in [
        "milimetry",
        "Y w górę",
        "list_commands",
        "batch",
        "read < design < motion < config < fire",
    ] {
        assert!(i.contains(needle), "instrukcje bez: {needle}");
    }
}

#[test]
fn req_15_005_tools_have_output_schema_and_structured_content() {
    let mut s = server();
    let r = s
        .handle(json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}))
        .unwrap();
    let tools = r["result"]["tools"].as_array().unwrap();
    let names: Vec<_> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
    for n in [
        "list_commands",
        "describe_command",
        "execute",
        "batch",
        "undo",
        "redo",
        "inspect_document",
        "render_preview",
    ] {
        assert!(names.contains(&n), "brak narzędzia {n}");
    }
    for t in tools {
        assert_eq!(t["inputSchema"]["type"], "object", "{}", t["name"]);
        assert_eq!(t["outputSchema"]["type"], "object", "{}", t["name"]);
    }
    let res = call(&mut s, "inspect_document", json!({}));
    assert!(res["structuredContent"].is_object());
    assert_eq!(res["content"][0]["type"], "text");
}

#[test]
fn req_15_013_list_commands_with_filter() {
    let mut s = server();
    let all = call(&mut s, "list_commands", json!({}));
    let n_all = all["structuredContent"]["commands"]
        .as_array()
        .unwrap()
        .len();
    let shapes = call(&mut s, "list_commands", json!({"filter": "shape."}));
    let list = shapes["structuredContent"]["commands"].as_array().unwrap();
    assert!(!list.is_empty() && list.len() < n_all);
    assert!(
        list.iter()
            .all(|c| c["id"].as_str().unwrap().starts_with("shape.")
                && c.get("safety_tier").is_some())
    );
}

#[test]
fn req_15_014_describe_command() {
    let mut s = server();
    let r = call(&mut s, "describe_command", json!({"id": "shape.rect"}));
    let c = &r["structuredContent"];
    assert_eq!(c["safety_tier"], "design");
    assert_eq!(c["undoable"], true);
    assert!(c["params_schema"]["properties"]["w"].is_object());
    assert!(c["requirements"]
        .as_array()
        .unwrap()
        .iter()
        .any(|x| x == "REQ-02-006"));
    let bad = call(&mut s, "describe_command", json!({"id": "nie.ma"}));
    assert_eq!(bad["isError"], true);
}

#[test]
fn req_15_015_execute_and_req_15_019_inspect() {
    let mut s = server();
    let r = call(
        &mut s,
        "execute",
        json!({"id": "shape.rect", "params": {"x": 10, "y": 10, "w": 100, "h": 60, "radius": 5, "layer": 1}}),
    );
    assert_eq!(r["isError"], false);
    let id = r["structuredContent"]["result"]["id"].as_u64().unwrap();
    let doc = call(&mut s, "inspect_document", json!({}));
    let o = &doc["structuredContent"]["objects"][0];
    assert_eq!(o["id"], id);
    assert_eq!(o["layer"], 1);
    assert_eq!(o["closed"], true);
    assert_eq!(doc["structuredContent"]["selection"], json!([id]));
    // błąd polecenia = wynik z isError, nie błąd protokołu
    let e = call(
        &mut s,
        "execute",
        json!({"id": "shape.rect", "params": {"x": 0}}),
    );
    assert_eq!(e["isError"], true);
}

#[test]
fn req_15_016_batch_single_step_and_req_15_017_018_undo_redo() {
    let mut s = server();
    let r = call(
        &mut s,
        "batch",
        json!({"steps": [
            {"id": "shape.rect", "params": {"x": 0, "y": 0, "w": 100, "h": 60}},
            {"id": "shape.ellipse", "params": {"cx": 50, "cy": 30, "rx": 10, "ry": 10, "layer": 2}},
            {"id": "selection.all"},
            {"id": "object.transform", "params": {"translate": [20, 20]}}
        ]}),
    );
    assert_eq!(r["isError"], false, "{r}");
    assert_eq!(s.session().history.undo_labels(), vec!["batch"]);
    let u = call(&mut s, "undo", json!({}));
    assert_eq!(u["structuredContent"]["label"], "batch");
    assert!(s.session().doc.objects.is_empty());
    let rr = call(&mut s, "redo", json!({}));
    assert_eq!(rr["structuredContent"]["can_undo"], true);
    assert_eq!(s.session().doc.objects.len(), 2);
    // nieudany krok -> numer kroku w wyniku, dokument bez zmian
    let bad = call(
        &mut s,
        "batch",
        json!({"steps": [{"id": "shape.rect", "params": {"x":0,"y":0,"w":1,"h":1}}, {"id": "brak.polecenia"}]}),
    );
    assert_eq!(bad["isError"], true);
    assert_eq!(s.session().doc.objects.len(), 2);
}

#[test]
fn req_15_022_render_preview_png() {
    let mut s = server();
    call(
        &mut s,
        "execute",
        json!({"id": "shape.ellipse", "params": {"cx": 200, "cy": 200, "rx": 50, "ry": 30}}),
    );
    let r = call(&mut s, "render_preview", json!({"max_size": 256}));
    assert_eq!(r["content"][0]["type"], "image");
    assert_eq!(r["content"][0]["mimeType"], "image/png");
    let png = base64::engine::general_purpose::STANDARD
        .decode(r["content"][0]["data"].as_str().unwrap())
        .unwrap();
    assert_eq!(&png[1..4], b"PNG");
    assert_eq!(r["structuredContent"]["width"], 256);
    let cropped = call(
        &mut s,
        "render_preview",
        json!({"bbox": [150, 170, 250, 230], "max_size": 200}),
    );
    assert_eq!(cropped["structuredContent"]["height"], 120);
    let tp = call(&mut s, "render_preview", json!({"mode": "burn_sim"}));
    assert_eq!(
        tp["isError"], true,
        "burn_sim dopiero w M4 (toolpath od M2.1)"
    );
    use base64::Engine;
}

#[test]
fn req_15_093_policy_denies_motion_and_fire_and_req_15_105_audits() {
    let mut s = server();
    let r = call(
        &mut s,
        "execute",
        json!({"id": "device.jog", "params": {"axis": "x", "distance": 10}}),
    );
    assert_eq!(r["isError"], true);
    assert!(r["content"][0]["text"]
        .as_str()
        .unwrap()
        .contains("confirmation_required"));
    let f = call(
        &mut s,
        "batch",
        json!({"steps": [{"id": "device.test_fire", "params": {"power": 10, "ms": 50}}]}),
    );
    assert_eq!(f["isError"], true);
    call(&mut s, "list_commands", json!({}));
    let log = s.audit.lock().unwrap().recent.clone();
    assert_eq!(log.len(), 3);
    assert_eq!(log[0]["outcome"], "denied");
    assert_eq!(log[0]["client"], "test-client 1");
    assert_eq!(log[0]["tool"], "execute");
    assert_eq!(log[0]["args"]["id"], "device.jog");
    assert_eq!(log[2]["outcome"], "ok");
    assert!(log[0]["ts_ms"].as_u64().unwrap() > 0);
}

#[test]
fn req_15_105_audit_written_to_file() {
    let p = std::env::temp_dir().join(format!("bc-audit-{}.jsonl", std::process::id()));
    let _ = std::fs::remove_file(&p);
    let mut s = Server::new(
        Session::default(),
        Config {
            audit_log: Some(p.clone()),
            ..Default::default()
        },
    );
    s.handle(json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": {"name": "undo", "arguments": {}}}));
    let text = std::fs::read_to_string(&p).unwrap();
    let line: Value = serde_json::from_str(text.lines().next().unwrap()).unwrap();
    assert_eq!(line["tool"], "undo");
    std::fs::remove_file(&p).ok();
}

#[test]
fn req_15_078_to_082_resources() {
    let mut s = server();
    let id = call(
        &mut s,
        "execute",
        json!({"id": "shape.rect", "params": {"x": 0, "y": 0, "w": 5, "h": 5}}),
    )["structuredContent"]["result"]["id"]
        .as_u64()
        .unwrap();
    let read = |s: &mut Server, uri: &str| {
        s.handle(
            json!({"jsonrpc": "2.0", "id": 9, "method": "resources/read", "params": {"uri": uri}}),
        )
        .unwrap()
    };
    let list = s
        .handle(json!({"jsonrpc": "2.0", "id": 9, "method": "resources/list"}))
        .unwrap();
    assert_eq!(list["result"]["resources"].as_array().unwrap().len(), 2);
    let t = s
        .handle(json!({"jsonrpc": "2.0", "id": 9, "method": "resources/templates/list"}))
        .unwrap();
    assert_eq!(
        t["result"]["resourceTemplates"].as_array().unwrap().len(),
        5 // + device/{id}/status i job/{id} (M2.4)
    );
    for uri in [
        "burncraft://document",
        "burncraft://commands",
        &format!("burncraft://object/{id}"),
        "burncraft://layer/0",
        "burncraft://layer/%23000000",
        "burncraft://command/shape.rect",
    ] {
        let r = read(&mut s, uri);
        assert!(r.get("result").is_some(), "{uri}: {r}");
        let text = r["result"]["contents"][0]["text"].as_str().unwrap();
        serde_json::from_str::<Value>(text).unwrap();
    }
    assert_eq!(
        read(&mut s, "burncraft://object/999")["error"]["code"],
        -32002
    );
}

#[test]
fn protocol_errors() {
    let mut s = server();
    assert_eq!(
        s.handle(json!({"jsonrpc": "2.0", "id": 5, "method": "nope"}))
            .unwrap()["error"]["code"],
        -32601
    );
    let r = s.handle(json!({"jsonrpc": "2.0", "id": 5, "method": "tools/call", "params": {"name": "nope", "arguments": {}}})).unwrap();
    assert_eq!(r["error"]["code"], -32602);
    assert_eq!(
        s.handle(json!({"jsonrpc": "2.0", "id": 6, "method": "ping"}))
            .unwrap()["result"],
        json!({})
    );
}
