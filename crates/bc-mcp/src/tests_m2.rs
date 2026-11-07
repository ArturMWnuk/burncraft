//! Testy MCP M2.1: plan_job (statystyki, ostrzeżenia, koszt) i render_preview
//! w trybie toolpath (ADR 0009).

use super::*;
use base64::Engine;
use serde_json::json;

fn server_with(cfg: Config) -> Server {
    let mut s = Server::new(Session::default(), cfg);
    s.handle(json!({"jsonrpc": "2.0", "id": 0, "method": "initialize", "params": {"protocolVersion": "2025-06-18", "clientInfo": {"name": "t", "version": "1"}}}));
    s
}

fn raw(s: &mut Server, name: &str, args: Value) -> Value {
    s.handle(json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": {"name": name, "arguments": args}}))
        .unwrap()["result"]
        .clone()
}

fn call(s: &mut Server, name: &str, args: Value) -> Value {
    let res = raw(s, name, args);
    assert_eq!(
        res["isError"], false,
        "{name}: {}",
        res["content"][0]["text"]
    );
    res["structuredContent"].clone()
}

fn exec(s: &mut Server, id: &str, params: Value) -> Value {
    call(s, "execute", json!({"id": id, "params": params}))["result"].clone()
}

fn codes(v: &Value) -> Vec<String> {
    v["warnings"]
        .as_array()
        .unwrap()
        .iter()
        .map(|w| w["code"].as_str().unwrap().to_string())
        .collect()
}

#[test]
fn req_15_051_plan_job_tool_stats() {
    let mut s = server_with(Config::default());
    let r = s
        .handle(json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}))
        .unwrap();
    let tool = r["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["name"] == "plan_job")
        .expect("narzędzie plan_job")
        .clone();
    assert_eq!(tool["annotations"]["readOnlyHint"], true);
    for k in [
        "time",
        "cut_length",
        "travel_length",
        "passes",
        "warnings",
        "cost",
    ] {
        assert!(tool["outputSchema"]["properties"][k].is_object(), "{k}");
    }
    exec(
        &mut s,
        "shape.rect",
        json!({"x": 10, "y": 10, "w": 30, "h": 20}),
    );
    exec(&mut s, "layer.set", json!({"index": 0, "passes": 2}));
    let v = call(&mut s, "plan_job", json!({"rate_per_hour": 60}));
    assert!(
        (v["cut_length"].as_f64().unwrap() - 200.0).abs() < 1e-6,
        "{v}"
    );
    assert!(v["travel_length"].as_f64().unwrap() > 0.0);
    assert_eq!(v["passes"], 2);
    let t = v["time"]["total_s"].as_f64().unwrap();
    assert!(t > 0.0);
    assert!((v["cost"]["machine_cost"].as_f64().unwrap() - t / 60.0).abs() < 1e-9);
    assert!(codes(&v).is_empty());
}

#[test]
fn req_15_052_to_056_plan_job_tool_warnings() {
    let mut s = server_with(Config::default());
    // 052: otwarty kształt w warstwie Fill (warstwa 1)
    exec(
        &mut s,
        "shape.path",
        json!({"d": "M10 10 L30 10 L30 30", "layer": 1}),
    );
    exec(
        &mut s,
        "layer.set",
        json!({"index": 1, "mode": "fill", "power_max": 90}),
    );
    // 053: poza obszarem roboczym
    exec(
        &mut s,
        "shape.rect",
        json!({"x": -20, "y": 10, "w": 10, "h": 10}),
    );
    // 054: duplikaty
    exec(
        &mut s,
        "shape.rect",
        json!({"x": 100, "y": 100, "w": 10, "h": 10}),
    );
    exec(
        &mut s,
        "shape.rect",
        json!({"x": 100, "y": 100, "w": 10, "h": 10}),
    );
    // 055: cięcie (warstwa 0) przed grawerem (warstwa 1) — wymaga obiektu wypełnianego
    exec(
        &mut s,
        "shape.rect",
        json!({"x": 200, "y": 200, "w": 10, "h": 10, "layer": 1}),
    );
    // 056: moc powyżej limitu (warstwa 1: 90 %)
    let v = call(&mut s, "plan_job", json!({"power_limit": 50}));
    let c = codes(&v);
    for code in [
        "open_shape_in_fill",
        "outside_workspace",
        "duplicate",
        "cut_before_engrave",
        "power_above_limit",
    ] {
        assert!(c.iter().any(|x| x == code), "brak {code}: {c:?}");
    }
    for w in v["warnings"].as_array().unwrap() {
        assert!(!w["message"].as_str().unwrap().is_empty());
    }
}

#[test]
fn req_15_056_power_limit_from_preferences() {
    let dir = std::env::temp_dir().join(format!("bc-mcp-m2-prefs-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let mut s = server_with(Config {
        prefs_dir: Some(dir.clone()),
        ..Default::default()
    });
    exec(
        &mut s,
        "shape.rect",
        json!({"x": 10, "y": 10, "w": 10, "h": 10}),
    );
    exec(&mut s, "layer.set", json!({"index": 0, "power_max": 40}));
    assert!(codes(&call(&mut s, "plan_job", json!({}))).is_empty());
    // prefs.set ma poziom config (wymaga potwierdzenia w GUI) — limit ustawia użytkownik
    bc_commands::prefs::PrefStore::new(&dir)
        .save(&json!({"limits": {"max_power": 30}}))
        .unwrap();
    assert!(codes(&call(&mut s, "plan_job", json!({}))).contains(&"power_above_limit".to_string()));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn req_15_023_render_preview_toolpath() {
    let mut s = server_with(Config::default());
    exec(
        &mut s,
        "shape.rect",
        json!({"x": 50, "y": 50, "w": 100, "h": 50, "layer": 2}),
    );
    let res = raw(
        &mut s,
        "render_preview",
        json!({"mode": "toolpath", "max_size": 400}),
    );
    assert_eq!(res["isError"], false, "{}", res["content"][0]["text"]);
    assert_eq!(res["content"][0]["type"], "image");
    assert_eq!(res["structuredContent"]["mode"], "toolpath");
    assert_eq!(res["structuredContent"]["width"], 400);
    let png = base64::engine::general_purpose::STANDARD
        .decode(res["content"][0]["data"].as_str().unwrap())
        .unwrap();
    assert_eq!(&png[1..4], b"PNG");
    // burn_sim nadal niedostępny (M4)
    let res = raw(&mut s, "render_preview", json!({"mode": "burn_sim"}));
    assert_eq!(res["isError"], true);
}
