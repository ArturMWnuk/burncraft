//! Test end-to-end okna (REQ-01-042, REQ-01-017, REQ-01-021): `burncraft
//! --control <port>` uruchamia okno Tauri, którego frontend podłącza się do
//! kanału sterowania jako okno; agent dostaje prawdziwy zrzut kanwy.
//! Na Linuksie wymaga serwera X (w CI: xvfb-run).

use std::io::{BufRead, BufReader};
use std::net::TcpStream;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use serde_json::{json, Value};
use tungstenite::client::IntoClientRequest;
use tungstenite::stream::MaybeTlsStream;
use tungstenite::{Message, WebSocket};

type Ws = WebSocket<MaybeTlsStream<TcpStream>>;

struct App(Child);

impl Drop for App {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

fn call(w: &mut Ws, id: u64, method: &str, params: Value) -> Value {
    w.send(Message::Text(
        json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params})
            .to_string()
            .into(),
    ))
    .unwrap();
    loop {
        let Message::Text(t) = w.read().unwrap() else {
            continue;
        };
        let v: Value = serde_json::from_str(&t).unwrap();
        if v.get("id") == Some(&json!(id)) && v.get("method").is_none() {
            assert!(v.get("error").is_none(), "{method}: {v}");
            return v["result"].clone();
        }
    }
}

#[test]
fn req_01_017_021_042_control_port_starts_window_connected_to_channel() {
    let dir = std::env::temp_dir().join(format!("bc-app-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let token_file = dir.join("control.token");
    let port = free_port();
    let mut child = Command::new(env!("CARGO_BIN_EXE_burncraft"))
        .args([
            "--control",
            &port.to_string(),
            "--no-audit-log",
            "--autosave-minutes",
            "0",
            "--prefs-dir",
        ])
        .arg(dir.join("prefs"))
        .arg("--token-file")
        .arg(&token_file)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("uruchomienie burncraft");
    // pierwsza linia stdout: adres kanału sterowania
    let out = BufReader::new(child.stdout.take().unwrap());
    let app = App(child);
    let first = out.lines().next().expect("adres na stdout").unwrap();
    assert!(first.contains(&format!("127.0.0.1:{port}")), "{first}");
    let token = std::fs::read_to_string(&token_file).unwrap();
    let req = format!("ws://127.0.0.1:{port}/rpc?token={}", token.trim())
        .into_client_request()
        .unwrap();
    let (mut w, _) = tungstenite::connect(req).expect("kanał sterowania okna");
    if let MaybeTlsStream::Plain(s) = w.get_ref() {
        s.set_read_timeout(Some(Duration::from_secs(30))).unwrap();
    }
    // frontend w oknie Tauri podłącza się jako okno
    let start = Instant::now();
    let mut windows = 0;
    let mut id = 1;
    while start.elapsed() < Duration::from_secs(90) {
        windows = call(&mut w, id, "gui.status", json!({}))["windows"]
            .as_u64()
            .unwrap();
        id += 1;
        if windows >= 1 {
            break;
        }
        std::thread::sleep(Duration::from_millis(250));
    }
    assert_eq!(windows, 1, "okno nie podłączyło się w 90 s");
    // agent MCP przez ten sam kanał: kształt i zrzut prawdziwej kanwy
    call(
        &mut w,
        id,
        "gui.execute",
        json!({"id": "shape.rect", "params": {"x": 10, "y": 10, "w": 100, "h": 60}}),
    );
    let r = call(
        &mut w,
        id + 1,
        "mcp",
        json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": {"name": "screenshot", "arguments": {}}}),
    );
    assert_eq!(r["result"]["isError"], false, "{r}");
    let png = &r["result"]["content"][0]["data"];
    assert!(png.as_str().unwrap().len() > 1000, "niepusty PNG");
    drop(app);
}
