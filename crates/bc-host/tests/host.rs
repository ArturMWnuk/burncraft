//! Testy kanału sterowania (ADR 0007): token, Origin, polecenia GUI, scena,
//! powiadomienia, MCP przez kanał, zrzut ekranu okna, statyczny frontend.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::Duration;

use bc_commands::Session;
use bc_host::{spawn, HostConfig, HostHandle};
use serde_json::{json, Value};
use tungstenite::client::IntoClientRequest;
use tungstenite::stream::MaybeTlsStream;
use tungstenite::{Message, WebSocket};

type Ws = WebSocket<MaybeTlsStream<TcpStream>>;

fn host() -> HostHandle {
    spawn(
        Session::default(),
        HostConfig {
            port: 0,
            ..Default::default()
        },
    )
    .expect("host startuje")
}

fn connect(h: &HostHandle, token: Option<&str>, origin: Option<&str>) -> Result<Ws, u16> {
    let url = match token {
        Some(t) => format!("ws://{}/rpc?token={t}", h.addr),
        None => format!("ws://{}/rpc", h.addr),
    };
    let mut req = url.into_client_request().unwrap();
    if let Some(o) = origin {
        req.headers_mut().insert("Origin", o.parse().unwrap());
    }
    match tungstenite::connect(req) {
        Ok((ws, _)) => {
            if let MaybeTlsStream::Plain(s) = ws.get_ref() {
                s.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
            }
            Ok(ws)
        }
        Err(tungstenite::Error::Http(r)) => Err(r.status().as_u16()),
        Err(e) => panic!("połączenie: {e}"),
    }
}

fn ws(h: &HostHandle) -> Ws {
    connect(h, Some(&h.token), None).unwrap()
}

/// Wysyła żądanie i czeka na odpowiedź o tym id; zwraca (odpowiedź, otrzymane powiadomienia).
fn call(w: &mut Ws, id: u64, method: &str, params: Value) -> (Value, Vec<Value>) {
    w.send(Message::Text(
        json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params})
            .to_string()
            .into(),
    ))
    .unwrap();
    let mut notes = Vec::new();
    loop {
        let m = w.read().unwrap();
        let Message::Text(t) = m else { continue };
        let v: Value = serde_json::from_str(&t).unwrap();
        if v.get("id") == Some(&json!(id)) && v.get("method").is_none() {
            return (v, notes);
        }
        notes.push(v);
    }
}

fn ok(w: &mut Ws, id: u64, method: &str, params: Value) -> Value {
    let (r, _) = call(w, id, method, params.clone());
    assert!(r.get("error").is_none(), "{method} {params}: {r}");
    r["result"].clone()
}

#[test]
fn req_01_042_channel_requires_token_and_checks_origin() {
    let h = host();
    assert_eq!(connect(&h, None, None).err(), Some(401), "bez tokenu");
    assert_eq!(connect(&h, Some("zly"), None).err(), Some(401), "zły token");
    assert_eq!(
        connect(&h, Some(&h.token), Some("http://evil.example")).err(),
        Some(403),
        "obca strona (CSWSH)"
    );
    let own = format!("http://{}", h.addr);
    assert!(
        connect(&h, Some(&h.token), Some(&own)).is_ok(),
        "frontend z hosta"
    );
    assert!(
        connect(&h, Some(&h.token), Some("tauri://localhost")).is_ok(),
        "okno Tauri"
    );
    let mut w = ws(&h);
    let hello = ok(&mut w, 1, "hello", json!({"role": "client"}));
    assert_eq!(hello["server"], "burncraft");
    assert!(h.addr.ip().is_loopback(), "tylko 127.0.0.1");
    h.shutdown();
}

#[test]
fn req_01_036_gui_uses_command_registry_and_scene() {
    let h = host();
    let mut w = ws(&h);
    let r = ok(
        &mut w,
        1,
        "gui.execute",
        json!({"id": "shape.rect", "params": {"x": 10, "y": 10, "w": 30, "h": 20, "layer": 2}}),
    );
    let id = r["id"].as_u64().unwrap();
    let scene = ok(&mut w, 2, "gui.scene", json!({}));
    assert_eq!(scene["workspace"], json!([400.0, 400.0]));
    let o = scene["objects"]
        .as_array()
        .unwrap()
        .iter()
        .find(|o| o["id"] == id)
        .expect("obiekt w scenie");
    assert_eq!(o["layer"], 2);
    let poly = o["polylines"][0].as_array().unwrap();
    assert!(poly.len() >= 8, "płaska tablica x,y: {poly:?}");
    assert_eq!(scene["selection"], json!([id]));
    let layer2 = scene["layers"]
        .as_array()
        .unwrap()
        .iter()
        .find(|l| l["index"] == 2)
        .unwrap();
    assert!(layer2["color"].as_str().unwrap().starts_with('#'));
    // błąd polecenia = błąd JSON-RPC z kodem rejestru
    let (e, _) = call(&mut w, 3, "gui.execute", json!({"id": "nie.ma"}));
    assert_eq!(e["error"]["data"]["code"], "unknown_command");
    // wsad i cofanie
    ok(
        &mut w,
        4,
        "gui.batch",
        json!({"steps": [{"id": "shape.ellipse", "params": {"cx": 0, "cy": 0, "rx": 1, "ry": 1}}, {"id": "selection.all"}]}),
    );
    ok(&mut w, 5, "gui.execute", json!({"id": "edit.undo"}));
    assert_eq!(
        ok(&mut w, 6, "gui.scene", json!({}))["objects"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    h.shutdown();
}

#[test]
fn req_03_008_010_scene_has_snap_nodes_and_midpoints() {
    let h = host();
    let mut w = ws(&h);
    ok(
        &mut w,
        1,
        "gui.execute",
        json!({"id": "shape.rect", "params": {"x": 10, "y": 20, "w": 40, "h": 10}}),
    );
    ok(
        &mut w,
        2,
        "gui.execute",
        json!({"id": "shape.ellipse", "params": {"cx": 100, "cy": 100, "rx": 10, "ry": 10}}),
    );
    let scene = ok(&mut w, 3, "gui.scene", json!({}));
    let pts = |v: &Value| -> Vec<(i64, i64)> {
        let mm = |x: &Value| (x.as_f64().unwrap() * 1000.0).round() as i64;
        let mut p: Vec<(i64, i64)> = v
            .as_array()
            .unwrap()
            .chunks(2)
            .map(|c| (mm(&c[0]), mm(&c[1])))
            .collect();
        p.sort();
        p
    };
    let rect = &scene["objects"][0];
    // węzły: 4 narożniki bez duplikatu punktu zamknięcia (płaska tablica x,y)
    assert_eq!(
        pts(&rect["nodes"]),
        vec![
            (10000, 20000),
            (10000, 30000),
            (50000, 20000),
            (50000, 30000)
        ]
    );
    // środki odcinków: 4 krawędzie
    assert_eq!(
        pts(&rect["mids"]),
        vec![
            (10000, 25000),
            (30000, 20000),
            (30000, 30000),
            (50000, 25000)
        ]
    );
    // okrąg: środek do przyciągania = środek bbox; węzły na okręgu
    let circle = &scene["objects"][1];
    assert!(circle["nodes"].as_array().unwrap().len() >= 8);
    for p in pts(&circle["nodes"]) {
        let (x, y) = (p.0 as f64 / 1000.0 - 100.0, p.1 as f64 / 1000.0 - 100.0);
        assert!(
            ((x * x + y * y).sqrt() - 10.0).abs() < 1e-3,
            "węzeł na okręgu: {p:?}"
        );
    }
    h.shutdown();
}

#[test]
fn req_02_040_scene_has_object_tree() {
    let h = host();
    let mut w = ws(&h);
    let a = ok(
        &mut w,
        1,
        "gui.execute",
        json!({"id": "shape.rect", "params": {"x": 0, "y": 0, "w": 1, "h": 1}}),
    )["id"]
        .clone();
    let b = ok(
        &mut w,
        2,
        "gui.execute",
        json!({"id": "shape.ellipse", "params": {"cx": 5, "cy": 5, "rx": 1, "ry": 1}}),
    )["id"]
        .clone();
    let g = ok(
        &mut w,
        3,
        "gui.execute",
        json!({"id": "object.group", "params": {"ids": [a, b]}}),
    )["id"]
        .clone();
    let scene = ok(&mut w, 4, "gui.scene", json!({}));
    let tree = scene["tree"].as_array().unwrap();
    assert_eq!(tree.len(), 1);
    assert_eq!(tree[0]["id"], g);
    assert_eq!(tree[0]["type"], "group");
    let kids = tree[0]["children"].as_array().unwrap();
    assert_eq!(kids.len(), 2);
    assert_eq!(kids[1]["type"], "ellipse");
    assert_eq!(kids[0]["children"], json!([]));
    h.shutdown();
}

#[test]
fn pipelined_requests_from_one_client_run_in_order() {
    let h = host();
    let mut w = ws(&h);
    ok(
        &mut w,
        1,
        "gui.execute",
        json!({"id": "shape.rect", "params": {"x": 0, "y": 0, "w": 10, "h": 10}}),
    );
    ok(&mut w, 2, "gui.execute", json!({"id": "selection.clear"}));
    // GUI wysyła „zaznacz” i „przesuń” bez czekania na odpowiedź — skutki muszą zachować kolejność
    for round in 0..20u64 {
        let base = 100 + round * 3;
        let send = |w: &mut Ws, id: u64, cmd: &str, p: Value| {
            w.send(Message::Text(json!({"jsonrpc": "2.0", "id": id, "method": "gui.execute", "params": {"id": cmd, "params": p}}).to_string().into())).unwrap();
        };
        send(&mut w, base, "selection.set", json!({"ids": [1]}));
        send(
            &mut w,
            base + 1,
            "object.transform",
            json!({"translate": [1, 0]}),
        );
        send(&mut w, base + 2, "selection.clear", json!({}));
        let mut ids = Vec::new();
        while ids.len() < 3 {
            let Message::Text(t) = w.read().unwrap() else {
                continue;
            };
            let v: Value = serde_json::from_str(&t).unwrap();
            if v.get("method").is_some() {
                continue;
            }
            assert!(v.get("error").is_none(), "runda {round}: {v}");
            ids.push(v["id"].as_u64().unwrap());
        }
        assert_eq!(
            ids,
            vec![base, base + 1, base + 2],
            "odpowiedzi w kolejności żądań"
        );
    }
    let scene = ok(&mut w, 3, "gui.scene", json!({}));
    assert_eq!(scene["objects"][0]["bbox"][0], json!(20.0));
    h.shutdown();
}

#[test]
fn gui_status_counts_window_clients() {
    let h = host();
    let mut a = ws(&h);
    let st = ok(&mut a, 1, "gui.status", json!({}));
    assert_eq!(st["clients"], 1);
    assert_eq!(st["windows"], 0);
    assert_eq!(st["port"], h.addr.port());
    let mut win = ws(&h);
    ok(&mut win, 1, "hello", json!({"role": "window"}));
    let st = ok(&mut a, 2, "gui.status", json!({}));
    assert_eq!(
        (st["clients"].as_u64(), st["windows"].as_u64()),
        (Some(2), Some(1))
    );
    drop(win);
    // rozłączone okno znika z licznika
    let mut n = 0;
    for i in 3..60 {
        n = ok(&mut a, i, "gui.status", json!({}))["windows"]
            .as_u64()
            .unwrap();
        if n == 0 {
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    assert_eq!(n, 0);
    h.shutdown();
}

#[test]
fn req_16_006_incremental_scene_sends_only_changed_geometry() {
    let h = host();
    let mut w = ws(&h);
    for i in 0..3 {
        ok(
            &mut w,
            10 + i,
            "gui.execute",
            json!({"id": "shape.rect", "params": {"x": i * 20, "y": 0, "w": 10, "h": 10}}),
        );
    }
    let full = ok(&mut w, 20, "gui.scene", json!({}));
    let objs = full["objects"].as_array().unwrap();
    assert!(objs
        .iter()
        .all(|o| o["hash"].is_string() && o["polylines"].is_array()));
    let have: serde_json::Map<String, Value> = objs
        .iter()
        .map(|o| (o["id"].to_string(), o["hash"].clone()))
        .collect();
    // przesunięcie jednego obiektu: pozostałe przychodzą bez geometrii
    ok(
        &mut w,
        21,
        "gui.execute",
        json!({"id": "object.transform", "params": {"ids": [2], "translate": [5, 0]}}),
    );
    let inc = ok(&mut w, 22, "gui.scene", json!({"have": have}));
    let objs = inc["objects"].as_array().unwrap();
    assert_eq!(objs.len(), 3, "kolejność i lista obiektów zawsze pełna");
    for o in objs {
        if o["id"] == 2 {
            assert!(o["polylines"].is_array(), "zmieniony obiekt z geometrią");
            assert_ne!(o["hash"], have["2"]);
        } else {
            assert_eq!(o["same"], true, "{o}");
            assert!(o.get("polylines").is_none(), "bez geometrii: {o}");
        }
    }
    // zmiana warstwy (kolor) też zmienia skrót obiektu
    ok(
        &mut w,
        23,
        "gui.execute",
        json!({"id": "object.assign_layer", "params": {"ids": [1], "layer": 4}}),
    );
    let inc2 = ok(&mut w, 24, "gui.scene", json!({"have": have}));
    let o1 = inc2["objects"]
        .as_array()
        .unwrap()
        .iter()
        .find(|o| o["id"] == 1)
        .unwrap()
        .clone();
    assert!(o1.get("same").is_none() && o1["layer"] == 4);
    // drzewo tylko po zmianie struktury
    let th = inc2["tree_hash"].clone();
    let same_tree = ok(
        &mut w,
        25,
        "gui.scene",
        json!({"have": have, "have_tree": th}),
    );
    assert!(same_tree["tree"].is_null());
    ok(
        &mut w,
        26,
        "gui.execute",
        json!({"id": "object.set", "params": {"ids": [3], "name": "Pokrywa"}}),
    );
    let new_tree = ok(
        &mut w,
        27,
        "gui.scene",
        json!({"have": have, "have_tree": th}),
    );
    assert!(new_tree["tree"].is_array() && new_tree["tree_hash"] != th);
    // współrzędne zaokrąglone do 0,1 µm (krótszy JSON)
    let poly = full["objects"][0]["polylines"][0].as_array().unwrap();
    assert!(poly
        .iter()
        .all(|v| (v.as_f64().unwrap() * 1e4).fract().abs() < 1e-6));
    h.shutdown();
}

#[test]
fn doc_changed_is_broadcast_to_other_clients() {
    let h = host();
    let mut a = ws(&h);
    let mut b = ws(&h);
    ok(&mut b, 1, "hello", json!({"role": "window"}));
    ok(
        &mut a,
        1,
        "gui.execute",
        json!({"id": "shape.rect", "params": {"x": 0, "y": 0, "w": 1, "h": 1}}),
    );
    // b dostaje powiadomienie przed odpowiedzią na swoje kolejne żądanie
    let (_, notes) = call(&mut b, 2, "hello", json!({"role": "window"}));
    let n = notes
        .iter()
        .find(|n| n["method"] == "doc.changed")
        .expect("powiadomienie doc.changed");
    assert!(n["params"]["revision"].as_u64().unwrap() > 0);
    // odczyt nie generuje powiadomień
    ok(&mut a, 2, "gui.scene", json!({}));
    let (_, notes) = call(&mut b, 3, "hello", json!({"role": "window"}));
    assert!(
        notes.iter().all(|n| n["method"] != "doc.changed"),
        "{notes:?}"
    );
    h.shutdown();
}

#[test]
fn req_15_002_mcp_through_channel_shares_live_document() {
    let h = host();
    let mut agent = ws(&h);
    let mut gui = ws(&h);
    let init = ok(
        &mut agent,
        1,
        "mcp",
        json!({"jsonrpc": "2.0", "id": 10, "method": "initialize", "params": {"protocolVersion": "2025-06-18", "clientInfo": {"name": "agent", "version": "1"}}}),
    );
    assert_eq!(init["result"]["serverInfo"]["name"], "burncraft");
    let r = ok(
        &mut agent,
        2,
        "mcp",
        json!({"jsonrpc": "2.0", "id": 11, "method": "tools/call", "params": {"name": "draw_shape", "arguments": {"type": "ellipse", "params": {"cx": 50, "cy": 50, "rx": 5, "ry": 5}}}}),
    );
    assert_eq!(r["result"]["isError"], false, "{r}");
    let scene = ok(&mut gui, 1, "gui.scene", json!({}));
    assert_eq!(
        scene["objects"].as_array().unwrap().len(),
        1,
        "okno widzi obiekt agenta"
    );
    let denied = ok(
        &mut agent,
        3,
        "mcp",
        json!({"jsonrpc": "2.0", "id": 12, "method": "tools/call", "params": {"name": "execute", "arguments": {"id": "device.jog", "params": {"axis": "x", "distance": 1}}}}),
    );
    assert_eq!(
        denied["result"]["isError"], true,
        "polityka MCP obowiązuje także w żywym oknie"
    );
    // powiadomienia MCP (bez id) nie mają odpowiedzi
    let note = ok(
        &mut agent,
        4,
        "mcp",
        json!({"jsonrpc": "2.0", "method": "notifications/initialized"}),
    );
    assert!(note.is_null());
    h.shutdown();
}

#[test]
fn req_15_026_screenshot_comes_from_window() {
    let h = host();
    let mut agent = ws(&h);
    ok(
        &mut agent,
        1,
        "mcp",
        json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {"protocolVersion": "2025-06-18"}}),
    );
    let shot = |agent: &mut Ws, id: u64| {
        ok(
            agent,
            id,
            "mcp",
            json!({"jsonrpc": "2.0", "id": id, "method": "tools/call", "params": {"name": "screenshot", "arguments": {}}}),
        )
    };
    let none = shot(&mut agent, 2);
    assert_eq!(none["result"]["isError"], true);
    assert!(none["result"]["content"][0]["text"]
        .as_str()
        .unwrap()
        .contains("no_window"));
    // okno: odpowiada na ui.screenshot małym PNG
    let mut window = ws(&h);
    ok(&mut window, 1, "hello", json!({"role": "window"}));
    let png = base64_png();
    let t = std::thread::spawn(move || loop {
        let Ok(Message::Text(m)) = window.read() else {
            break;
        };
        let v: Value = serde_json::from_str(&m).unwrap();
        if v["method"] == "ui.screenshot" {
            window
                .send(Message::Text(
                    json!({"jsonrpc": "2.0", "id": v["id"], "result": {"png": png}})
                        .to_string()
                        .into(),
                ))
                .unwrap();
            break;
        }
    });
    let r = shot(&mut agent, 3);
    t.join().unwrap();
    assert_eq!(r["result"]["isError"], false, "{r}");
    assert_eq!(r["result"]["content"][0]["type"], "image");
    assert_eq!(r["result"]["content"][0]["mimeType"], "image/png");
    h.shutdown();
}

#[test]
fn req_01_029_window_commands_are_forwarded_to_window() {
    let h = host();
    let mut agent = ws(&h);
    ok(
        &mut agent,
        1,
        "mcp",
        json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {"protocolVersion": "2025-06-18"}}),
    );
    let exec = |agent: &mut Ws, id: u64, cmd: &str, params: Value| {
        ok(
            agent,
            id,
            "mcp",
            json!({"jsonrpc": "2.0", "id": id, "method": "tools/call",
                "params": {"name": "execute", "arguments": {"id": cmd, "params": params}}}),
        )
    };
    // bez okna: błąd no_window
    let none = exec(&mut agent, 2, "view.zoom_page", json!({}));
    assert_eq!(none["result"]["isError"], true, "{none}");
    assert!(none.to_string().contains("no_window"), "{none}");
    // okno wykonuje polecenie i odsyła wynik
    let mut window = ws(&h);
    ok(&mut window, 1, "hello", json!({"role": "window"}));
    let t = std::thread::spawn(move || {
        let mut seen = Vec::new();
        while seen.len() < 2 {
            let Ok(Message::Text(m)) = window.read() else {
                break;
            };
            let v: Value = serde_json::from_str(&m).unwrap();
            if v["method"] == "ui.execute" {
                seen.push(v["params"].clone());
                window
                    .send(Message::Text(
                        json!({"jsonrpc": "2.0", "id": v["id"], "result": {"zoom": 250.0, "echo": v["params"]}})
                            .to_string()
                            .into(),
                    ))
                    .unwrap();
            }
        }
        seen
    });
    let r = exec(&mut agent, 3, "view.zoom", json!({"factor": 2.5}));
    assert_eq!(r["result"]["isError"], false, "{r}");
    assert_eq!(
        r["result"]["structuredContent"]["result"]["zoom"], 250.0,
        "{r}"
    );
    // to samo przez gui.execute (np. drugi klient GUI)
    let g = ok(
        &mut agent,
        4,
        "gui.execute",
        json!({"id": "tool.select", "params": {"tool": "rect"}}),
    );
    assert_eq!(g["echo"]["id"], "tool.select", "{g}");
    let seen = t.join().unwrap();
    assert_eq!(
        seen[0],
        json!({"id": "view.zoom", "params": {"factor": 2.5}})
    );
    assert_eq!(
        seen[1],
        json!({"id": "tool.select", "params": {"tool": "rect"}})
    );
    h.shutdown();
}

fn send(w: &mut Ws, id: u64, method: &str, params: Value) {
    w.send(Message::Text(
        json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params})
            .to_string()
            .into(),
    ))
    .unwrap();
}

fn read(w: &mut Ws) -> Value {
    loop {
        if let Message::Text(t) = w.read().unwrap() {
            return serde_json::from_str(&t).unwrap();
        }
    }
}

#[test]
fn req_16_006_long_command_reports_progress_and_completes() {
    let h = host();
    let mut c = ws(&h);
    ok(
        &mut c,
        1,
        "gui.execute",
        json!({"id": "shape.polygon", "params": {"cx": 1, "cy": 1, "r": 0.4, "sides": 64}}),
    );
    send(
        &mut c,
        2,
        "gui.execute",
        json!({"id": "array.grid", "params": {"columns": 300, "rows": 300, "dx": 1, "dy": 1}}),
    );
    let mut notes = Vec::new();
    let reply = loop {
        let v = read(&mut c);
        if v["method"] == "gui.progress" {
            assert_eq!(v["params"]["command"], "array.grid", "{v}");
            notes.push(v["params"].clone());
        } else if v["id"] == 2 {
            break v;
        }
    };
    assert!(reply.get("error").is_none(), "{reply}");
    // ostatnie powiadomienie (przed odpowiedzią) zamyka pasek postępu
    let last = notes
        .pop()
        .expect("powiadomienia o postępie dłuższej operacji");
    assert_eq!(last["done"], true, "{last}");
    let fractions: Vec<f64> = notes
        .iter()
        .map(|n| n["fraction"].as_f64().unwrap())
        .collect();
    assert!(!fractions.is_empty(), "postęp w trakcie operacji");
    assert!(
        fractions.windows(2).all(|w| w[0] <= w[1])
            && fractions.iter().all(|f| (0.0..=1.0).contains(f)),
        "{fractions:?}"
    );
    h.shutdown();
}

#[test]
fn req_16_006_long_command_can_be_cancelled() {
    let h = host();
    let mut c = ws(&h);
    ok(
        &mut c,
        1,
        "gui.execute",
        json!({"id": "shape.polygon", "params": {"cx": 1, "cy": 1, "r": 0.4, "sides": 64}}),
    );
    send(
        &mut c,
        2,
        "gui.execute",
        json!({"id": "array.grid", "params": {"columns": 300, "rows": 300, "dx": 1, "dy": 1}}),
    );
    // pierwsze powiadomienie o postępie → anulowanie z tego samego klienta (kolejka jest zajęta)
    loop {
        let v = read(&mut c);
        if v["method"] == "gui.progress" {
            break;
        }
        assert_ne!(
            v["id"], 2,
            "operacja skończyła się przed pierwszym postępem: {v}"
        );
    }
    send(&mut c, 3, "gui.cancel", json!({}));
    let (mut exec, mut cancel) = (None, None);
    while exec.is_none() || cancel.is_none() {
        let v = read(&mut c);
        if v["id"] == 2 {
            exec = Some(v);
        } else if v["id"] == 3 {
            cancel = Some(v);
        }
    }
    let (exec, cancel) = (exec.unwrap(), cancel.unwrap());
    assert_eq!(cancel["result"]["cancelled"], true, "{cancel}");
    assert_eq!(exec["error"]["data"]["code"], "cancelled", "{exec}");
    // polecenie atomowe: dokument bez zmian, historia bez kroku
    let s = ok(&mut c, 4, "gui.scene", json!({}));
    assert_eq!(s["objects"].as_array().unwrap().len(), 1);
    // gui.cancel bez trwającej operacji nic nie robi
    let idle = ok(&mut c, 5, "gui.cancel", json!({}));
    assert_eq!(idle["cancelled"], false);
    h.shutdown();
}

fn base64_png() -> String {
    // 1×1 px PNG
    "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR4nGP4z8DwHwAFAAH/iZk9HQAAAABJRU5ErkJggg==".into()
}

fn http_get(h: &HostHandle, path: &str) -> String {
    let mut s = TcpStream::connect(h.addr).unwrap();
    write!(
        s,
        "GET {path} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\n\r\n",
        h.addr
    )
    .unwrap();
    let mut out = String::new();
    s.read_to_string(&mut out).unwrap();
    out
}

#[test]
fn serves_frontend_with_injected_token_and_token_file() {
    let dir = std::env::temp_dir().join(format!("bc-host-web-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("assets")).unwrap();
    std::fs::write(
        dir.join("index.html"),
        format!(
            "<html><script>window.TOKEN='{}'</script></html>",
            bc_host::TOKEN_PLACEHOLDER
        ),
    )
    .unwrap();
    std::fs::write(dir.join("assets/app.js"), "console.log('ok')").unwrap();
    let token_file = dir.join("control.token");
    let h = spawn(
        Session::default(),
        HostConfig {
            port: 0,
            web_dir: Some(dir.clone()),
            token_file: Some(token_file.clone()),
            ..Default::default()
        },
    )
    .unwrap();
    let index = http_get(&h, "/");
    assert!(index.starts_with("HTTP/1.1 200"), "{index}");
    assert!(index.contains(&h.token) && !index.contains(bc_host::TOKEN_PLACEHOLDER));
    assert!(http_get(&h, "/assets/app.js").contains("console.log"));
    assert!(
        http_get(&h, "/../Cargo.toml").starts_with("HTTP/1.1 404")
            || http_get(&h, "/../Cargo.toml").starts_with("HTTP/1.1 400"),
        "brak wyjścia poza katalog"
    );
    assert_eq!(
        std::fs::read_to_string(&token_file).unwrap().trim(),
        h.token
    );
    h.shutdown();
    let _ = std::fs::remove_dir_all(&dir);
}
