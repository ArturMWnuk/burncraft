//! M2.4: protokół MCP po stdio — subskrypcje zasobów, prompty, powiadomienia o postępie,
//! anulowanie żądań (§15.1, §15.3).

mod common;

use std::time::Duration;

use bc_commands::SafetyTier;
use common::*;
use serde_json::{json, Value};

fn start(name: &str) -> Stdio {
    let (s, _) = server(config(name, SafetyTier::Design));
    Stdio::start(s, json!({}))
}

fn capabilities(name: &str) -> Value {
    let (s, _) = server(config(name, SafetyTier::Design));
    let mut c = Stdio::start(s, json!({}));
    // ponowne initialize zwraca te same możliwości
    let r = c.request(
        "initialize",
        json!({"protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": {"name": "x", "version": "1"}}),
    );
    c.finish().unwrap();
    r["result"]["capabilities"].clone()
}

fn objects(c: &mut Stdio) -> usize {
    c.call("inspect_document", json!({}))["objects"]
        .as_array()
        .unwrap()
        .len()
}

#[test]
fn req_15_006_resources_subscribe_document() {
    let caps = capabilities("caps-sub");
    assert_eq!(caps["resources"]["subscribe"], true, "{caps}");
    let mut c = start("sub-doc");
    let uri = "burncraft://document";
    let r = c.request("resources/subscribe", json!({"uri": uri}));
    assert_eq!(r["result"], json!({}), "{r}");
    c.exec("shape.rect", json!({"x": 0, "y": 0, "w": 5, "h": 5}));
    let n = c.wait_for("aktualizacja dokumentu", WAIT, |v| {
        v["method"] == "notifications/resources/updated"
    });
    assert_eq!(n["params"]["uri"], uri);
    // bez zmian — bez powiadomień
    c.drain_for(Duration::from_millis(400));
    assert!(c.take("notifications/resources/updated").is_empty());
    // obiekt też jest zasobem z subskrypcją
    let obj_uri = "burncraft://object/1";
    c.request("resources/subscribe", json!({"uri": obj_uri}));
    c.call("transform", json!({"ids": [1], "translate": [3, 0]}));
    c.wait_for("aktualizacja obiektu", WAIT, |v| {
        v["method"] == "notifications/resources/updated" && v["params"]["uri"] == obj_uri
    });
    // odsubskrybowanie
    c.request("resources/unsubscribe", json!({"uri": uri}));
    c.request("resources/unsubscribe", json!({"uri": obj_uri}));
    c.drain_for(Duration::from_millis(300));
    c.take("notifications/resources/updated");
    c.exec("shape.rect", json!({"x": 9, "y": 9, "w": 5, "h": 5}));
    c.drain_for(Duration::from_millis(400));
    assert!(c.take("notifications/resources/updated").is_empty());
    // nieznany zasób
    let e = c.request("resources/subscribe", json!({"uri": "burncraft://nic"}));
    assert_eq!(e["error"]["code"], -32002, "{e}");
    let e = c.request("resources/subscribe", json!({}));
    assert_eq!(e["error"]["code"], -32602, "{e}");
    c.finish().unwrap();
}

#[test]
fn req_15_007_prompts_list_and_get() {
    let caps = capabilities("caps-prompts");
    assert!(caps["prompts"].is_object(), "{caps}");
    let mut c = start("prompts");
    let l = c.request("prompts/list", json!({}));
    let p = l["result"]["prompts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["name"] == "prepare_cut_file")
        .expect("prompt prepare_cut_file")
        .clone();
    assert!(p["description"].as_str().unwrap().len() > 20);
    assert!(p["title"].is_string());
    let e = c.request("prompts/get", json!({"name": "nie_ma"}));
    assert_eq!(e["error"]["code"], -32602, "{e}");
    let e = c.request("prompts/get", json!({}));
    assert_eq!(e["error"]["code"], -32602, "{e}");
    c.finish().unwrap();
}

#[test]
fn req_15_087_prompt_prepare_cut_file() {
    let mut c = start("prepare");
    // otwarta ścieżka w trybie wypełnienia, duplikat, grawer i cięcie
    c.exec(
        "shape.path",
        json!({"points": [[0, 0], [20, 0], [20, 20]], "closed": false, "layer": 1}),
    );
    c.exec("layer.set", json!({"index": 1, "mode": "fill"}));
    c.exec(
        "shape.rect",
        json!({"x": 40, "y": 40, "w": 10, "h": 10, "layer": 0}),
    );
    c.exec(
        "shape.rect",
        json!({"x": 40, "y": 40, "w": 10, "h": 10, "layer": 0}),
    );
    let r = c.request(
        "prompts/get",
        json!({"name": "prepare_cut_file", "arguments": {}}),
    );
    let res = &r["result"];
    assert!(res["description"].is_string(), "{r}");
    let msgs = res["messages"].as_array().unwrap();
    assert_eq!(msgs[0]["role"], "user");
    assert_eq!(msgs[0]["content"]["type"], "text");
    let text = msgs
        .iter()
        .map(|m| m["content"]["text"].as_str().unwrap_or(""))
        .collect::<Vec<_>>()
        .join("\n")
        .to_lowercase();
    for k in ["otwart", "duplikat", "kerf", "kolejno"] {
        assert!(text.contains(k), "brak „{k}” w: {text}");
    }
    // stan bieżącego dokumentu: ostrzeżenia planera
    for w in ["open_shape_in_fill", "duplicate"] {
        assert!(text.contains(w), "brak ostrzeżenia {w} w: {text}");
    }
    // narzędzia, które agent ma wywołać
    for t in ["plan_job", "render_preview"] {
        assert!(text.contains(t), "brak {t}");
    }
    c.finish().unwrap();
}

fn heavy(c: &mut Stdio) {
    c.exec(
        "shape.polygon",
        json!({"cx": 1, "cy": 1, "r": 0.4, "sides": 64}),
    );
}

#[test]
fn req_15_008_progress_notifications() {
    let mut c = start("progress");
    heavy(&mut c);
    let id = json!(c.next_id());
    c.send(json!({"jsonrpc": "2.0", "id": id, "method": "tools/call", "params": {
        "name": "execute", "arguments": {"id": "array.grid", "params": {"columns": 300, "rows": 300, "dx": 1, "dy": 1}},
        "_meta": {"progressToken": "siatka"}}}));
    let r = c.wait_response(&id);
    assert_eq!(r["result"]["isError"], false, "{r}");
    let notes = c.take("notifications/progress");
    assert!(!notes.is_empty(), "brak powiadomień o postępie");
    let mut last = -1.0;
    for n in &notes {
        assert_eq!(n["params"]["progressToken"], "siatka");
        let p = n["params"]["progress"].as_f64().unwrap();
        assert!(p > last, "postęp musi rosnąć: {notes:?}");
        last = p;
        if let Some(t) = n["params"]["total"].as_f64() {
            assert!(p <= t);
        }
    }
    // bez progressToken — bez powiadomień
    c.exec("edit.undo", json!({}));
    let id = json!(c.next_id());
    c.send(json!({"jsonrpc": "2.0", "id": id, "method": "tools/call", "params": {
        "name": "execute", "arguments": {"id": "array.grid", "params": {"columns": 300, "rows": 300, "dx": 1, "dy": 1}}}}));
    c.wait_response(&id);
    assert!(c.take("notifications/progress").is_empty());
    c.finish().unwrap();
}

#[test]
fn req_15_009_cancel_request() {
    let mut c = start("cancel");
    heavy(&mut c);
    let id = json!(c.next_id());
    c.send(json!({"jsonrpc": "2.0", "id": id, "method": "tools/call", "params": {
        "name": "execute", "arguments": {"id": "array.grid", "params": {"columns": 300, "rows": 300, "dx": 1, "dy": 1}},
        "_meta": {"progressToken": 7}}}));
    c.wait_for("pierwszy postęp", WAIT, |v| {
        v["method"] == "notifications/progress"
    });
    c.send(
        json!({"jsonrpc": "2.0", "method": "notifications/cancelled",
        "params": {"requestId": id, "reason": "użytkownik przerwał"}}),
    );
    // anulowane żądanie nie dostaje odpowiedzi: następna odpowiedź to ping
    let ping = json!(c.next_id());
    c.send(json!({"jsonrpc": "2.0", "id": ping, "method": "ping"}));
    let first = c.wait_for("odpowiedź", WAIT, |v| v.get("method").is_none());
    assert_eq!(first["id"], ping, "odpowiedź na anulowane żądanie: {first}");
    // polecenie atomowe: dokument bez zmian
    assert_eq!(objects(&mut c), 1);
    // anulowanie nieznanego / zakończonego żądania jest ignorowane
    c.send(json!({"jsonrpc": "2.0", "method": "notifications/cancelled", "params": {"requestId": 424242}}));
    assert_eq!(c.request("ping", json!({}))["result"], json!({}));
    c.finish().unwrap();
}
