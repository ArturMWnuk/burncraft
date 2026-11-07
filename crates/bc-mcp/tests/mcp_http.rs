//! M2.4: MCP przez Streamable HTTP na 127.0.0.1 (REQ-15-003, ADR 0012): sesje, nagłówki,
//! odpowiedzi JSON i SSE, strumień GET dla powiadomień serwera, postęp i anulowanie.

mod common;

use std::time::Duration;

use bc_commands::SafetyTier;
use common::*;
use serde_json::{json, Value};

fn serve(name: &str, tier: SafetyTier) -> (bc_mcp::http::HttpHandle, bc_drivers::MachinesHandle) {
    let (s, m) = server(config(name, tier));
    let h = bc_mcp::http::serve(s, 0).expect("serwer HTTP");
    (h, m)
}

#[test]
fn req_15_003_streamable_http_on_localhost() {
    let (h, _) = serve("http-basic", SafetyTier::Design);
    assert!(h.addr.ip().is_loopback(), "{}", h.addr);
    assert_eq!(h.url(), format!("http://{}/mcp", h.addr));
    let mut c = Http::new(h.addr);
    let init = c.initialize(json!({}));
    let res = &init["result"];
    assert_eq!(res["serverInfo"]["name"], "burncraft");
    assert_eq!(res["protocolVersion"], "2025-06-18");
    let sid = c.session.clone().unwrap();
    assert!(sid.len() >= 16 && sid.chars().all(|ch| ch.is_ascii_graphic()));
    // narzędzia i polecenia przez HTTP
    let tools = c.request("tools/list", json!({}));
    assert!(tools["result"]["tools"].as_array().unwrap().len() > 20);
    c.exec("shape.rect", json!({"x": 0, "y": 0, "w": 10, "h": 10}));
    // druga sesja widzi ten sam dokument
    let mut c2 = Http::new(h.addr);
    c2.initialize(json!({}));
    assert_ne!(c2.session, c.session);
    let doc = c2.call("inspect_document", json!({}));
    assert_eq!(doc["objects"].as_array().unwrap().len(), 1);
    // brak sesji → 400, nieznana sesja → 404
    let ping = json!({"jsonrpc": "2.0", "id": 1, "method": "ping"});
    let r = Http::new(h.addr).post(&ping);
    assert_eq!(r.status, 400);
    let mut bad = Http::new(h.addr);
    bad.session = Some("nieznana-sesja".into());
    assert_eq!(bad.post(&ping).status, 404);
    // obcy Origin (DNS rebinding) → 403; własny origin → OK
    assert_eq!(
        c.post_with(&ping, &[("Origin", "http://evil.example")])
            .status,
        403
    );
    let own = format!("http://localhost:{}", h.addr.port());
    let r = c.post_with(&ping, &[("Origin", own.as_str())]);
    assert_eq!(r.status, 200);
    assert_eq!(r.json()["result"], json!({}));
    // obcy Host → 403
    let r = c.raw(
        "POST",
        Some(&ping.to_string()),
        &[
            ("Accept", "application/json, text/event-stream"),
            ("Mcp-Session-Id", sid.as_str()),
            ("Host", "evil.example"),
        ],
    );
    assert!(r.status == 403 || r.status == 400, "{}", r.status);
    // nieobsługiwana wersja protokołu → 400
    assert_eq!(
        c.post_with(&ping, &[("MCP-Protocol-Version", "1999-01-01")])
            .status,
        400
    );
    // powiadomienie i odpowiedź klienta → 202 bez treści
    let n = c.post(&json!({"jsonrpc": "2.0", "method": "notifications/initialized"}));
    assert_eq!(n.status, 202);
    // zły JSON → 400 z błędem JSON-RPC
    let r = c.raw(
        "POST",
        Some("{nie json"),
        &[
            ("Accept", "application/json, text/event-stream"),
            ("Mcp-Session-Id", sid.as_str()),
        ],
    );
    assert_eq!(r.status, 400);
    assert_eq!(r.json()["error"]["code"], -32700);
    // GET bez Accept: text/event-stream → 405
    let r = c.raw("GET", None, &[("Mcp-Session-Id", sid.as_str())]);
    assert_eq!(r.status, 405);
    // DELETE kończy sesję
    assert!(matches!(c.delete().status, 200 | 204));
    assert_eq!(c.post(&ping).status, 404);
    h.shutdown();
}

#[test]
fn req_15_003_tool_call_answers_with_sse_stream() {
    let (h, _) = serve("http-sse", SafetyTier::Design);
    let mut c = Http::new(h.addr);
    c.initialize(json!({}));
    let id = c.next_id();
    let r = c.post(&json!({"jsonrpc": "2.0", "id": id, "method": "tools/call",
        "params": {"name": "inspect_document", "arguments": {}}}));
    assert_eq!(r.status, 200);
    let ct = r.header("content-type").unwrap_or("").to_string();
    assert!(
        ct.starts_with("text/event-stream") || ct.starts_with("application/json"),
        "{ct}"
    );
    let v = read_reply(r, id);
    assert_eq!(v["result"]["isError"], false);
    h.shutdown();
}

#[test]
fn req_15_008_http_progress_over_sse() {
    let (h, _) = serve("http-progress", SafetyTier::Design);
    let mut c = Http::new(h.addr);
    c.initialize(json!({}));
    c.exec(
        "shape.polygon",
        json!({"cx": 1, "cy": 1, "r": 0.4, "sides": 64}),
    );
    let id = c.next_id();
    let mut r = c.post(&json!({"jsonrpc": "2.0", "id": id, "method": "tools/call", "params": {
        "name": "execute", "arguments": {"id": "array.grid", "params": {"columns": 300, "rows": 300, "dx": 1, "dy": 1}},
        "_meta": {"progressToken": "p-http"}}}));
    assert!(r.is_sse(), "odpowiedź z postępem musi być strumieniem SSE");
    let mut progress = Vec::new();
    let reply = loop {
        let v = r.next_event(WAIT).expect("zdarzenie SSE");
        if v["method"] == "notifications/progress" {
            assert_eq!(v["params"]["progressToken"], "p-http");
            progress.push(v["params"]["progress"].as_f64().unwrap());
        } else if v["id"] == id {
            break v;
        }
    };
    assert_eq!(reply["result"]["isError"], false);
    assert!(!progress.is_empty());
    assert!(progress.windows(2).all(|w| w[0] < w[1]), "{progress:?}");
    h.shutdown();
}

#[test]
fn req_15_009_http_cancel_request() {
    let (h, _) = serve("http-cancel", SafetyTier::Design);
    let mut c = Http::new(h.addr);
    c.initialize(json!({}));
    c.exec(
        "shape.polygon",
        json!({"cx": 1, "cy": 1, "r": 0.4, "sides": 64}),
    );
    let id = c.next_id();
    let mut r = c.post(&json!({"jsonrpc": "2.0", "id": id, "method": "tools/call", "params": {
        "name": "execute", "arguments": {"id": "array.grid", "params": {"columns": 300, "rows": 300, "dx": 1, "dy": 1}},
        "_meta": {"progressToken": 1}}}));
    let first = r.next_event(WAIT).expect("pierwsze zdarzenie");
    assert_eq!(first["method"], "notifications/progress", "{first}");
    let n = c.post(
        &json!({"jsonrpc": "2.0", "method": "notifications/cancelled",
        "params": {"requestId": id, "reason": "stop"}}),
    );
    assert_eq!(n.status, 202);
    // strumień kończy się bez odpowiedzi na anulowane żądanie
    while let Some(v) = r.next_event(WAIT) {
        assert!(
            v.get("method").is_some(),
            "odpowiedź na anulowane żądanie: {v}"
        );
    }
    let doc = c.call("inspect_document", json!({}));
    assert_eq!(doc["objects"].as_array().unwrap().len(), 1);
    h.shutdown();
}

#[test]
fn req_15_006_http_subscription_on_get_stream() {
    let (h, _) = serve("http-sub", SafetyTier::Design);
    let mut c = Http::new(h.addr);
    c.initialize(json!({}));
    let mut events = c.get_stream();
    assert_eq!(events.status, 200);
    assert!(events.is_sse());
    let r = c.request(
        "resources/subscribe",
        json!({"uri": "burncraft://document"}),
    );
    assert_eq!(r["result"], json!({}));
    c.exec("shape.rect", json!({"x": 0, "y": 0, "w": 3, "h": 3}));
    let n: Value = loop {
        let v = events
            .next_event(WAIT)
            .expect("powiadomienie na strumieniu GET");
        if v["method"] == "notifications/resources/updated" {
            break v;
        }
    };
    assert_eq!(n["params"]["uri"], "burncraft://document");
    // inna sesja nie dostaje powiadomień o cudzych subskrypcjach
    let mut c2 = Http::new(h.addr);
    c2.initialize(json!({}));
    let mut ev2 = c2.get_stream();
    c.exec("shape.rect", json!({"x": 5, "y": 0, "w": 3, "h": 3}));
    assert!(ev2.next_event(Duration::from_millis(600)).is_none());
    h.shutdown();
}
