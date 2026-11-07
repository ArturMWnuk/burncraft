//! Test end-to-end: `burncraft-cli mcp --http 0` wystawia MCP przez Streamable HTTP na
//! 127.0.0.1 (REQ-15-003, ADR 0012); pierwszy wiersz stdout to adres punktu końcowego.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::process::{Command, Stdio};
use std::time::Duration;

use serde_json::{json, Value};

/// POST z treścią JSON; zwraca (status, nagłówki w małych literach, ciało).
fn post(addr: &str, session: Option<&str>, body: &Value) -> (u16, Vec<(String, String)>, String) {
    let mut s = TcpStream::connect(addr).unwrap();
    s.set_read_timeout(Some(Duration::from_secs(20))).unwrap();
    let b = body.to_string();
    let mut req = format!(
        "POST /mcp HTTP/1.1\r\nHost: {addr}\r\nConnection: close\r\nAccept: application/json, text/event-stream\r\nContent-Type: application/json\r\nContent-Length: {}\r\n",
        b.len()
    );
    if let Some(id) = session {
        req.push_str(&format!(
            "Mcp-Session-Id: {id}\r\nMCP-Protocol-Version: 2025-06-18\r\n"
        ));
    }
    req.push_str("\r\n");
    req.push_str(&b);
    s.write_all(req.as_bytes()).unwrap();
    let mut raw = Vec::new();
    let _ = s.read_to_end(&mut raw);
    let text = String::from_utf8_lossy(&raw).to_string();
    let (head, rest) = text.split_once("\r\n\r\n").expect("odpowiedź HTTP");
    let mut lines = head.lines();
    let status = lines
        .next()
        .unwrap()
        .split_whitespace()
        .nth(1)
        .unwrap()
        .parse()
        .unwrap();
    let headers: Vec<(String, String)> = lines
        .filter_map(|l| l.split_once(':'))
        .map(|(k, v)| (k.trim().to_ascii_lowercase(), v.trim().to_string()))
        .collect();
    (status, headers, rest.to_string())
}

/// Pierwszy obiekt JSON-RPC z ciała (JSON albo zdarzenia SSE, także w kodowaniu chunked).
fn first_reply(body: &str) -> Value {
    if let Ok(v) = serde_json::from_str::<Value>(body.trim()) {
        return v;
    }
    body.lines()
        .filter_map(|l| l.strip_prefix("data:"))
        .filter_map(|d| serde_json::from_str::<Value>(d.trim()).ok())
        .find(|v| v.get("id").is_some())
        .unwrap_or_else(|| panic!("brak odpowiedzi w: {body}"))
}

#[test]
fn req_15_003_cli_mcp_http_endpoint() {
    let prefs = std::env::temp_dir().join(format!("bc-cli-mcp-http-{}", std::process::id()));
    let mut child = Command::new(env!("CARGO_BIN_EXE_burncraft-cli"))
        .args([
            "mcp",
            "--http",
            "0",
            "--no-audit-log",
            "--autosave-minutes",
            "0",
        ])
        .arg("--prefs-dir")
        .arg(&prefs)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("uruchomienie burncraft-cli");
    let mut out = BufReader::new(child.stdout.take().unwrap());
    let mut url = String::new();
    out.read_line(&mut url).unwrap();
    let url = url.trim().to_string();
    let addr = url
        .strip_prefix("http://")
        .and_then(|u| u.strip_suffix("/mcp"))
        .unwrap_or_else(|| panic!("adres MCP: {url:?}"))
        .to_string();
    assert!(addr.starts_with("127.0.0.1:"), "{addr}");
    let (st, h, body) = post(
        &addr,
        None,
        &json!({"jsonrpc": "2.0", "id": 1, "method": "initialize",
            "params": {"protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": {"name": "e2e", "version": "0"}}}),
    );
    assert_eq!(st, 200, "{body}");
    let sid = h
        .iter()
        .find(|(k, _)| k == "mcp-session-id")
        .map(|(_, v)| v.clone())
        .expect("Mcp-Session-Id");
    assert_eq!(
        first_reply(&body)["result"]["serverInfo"]["name"],
        "burncraft"
    );
    let (st, _, body) = post(
        &addr,
        Some(&sid),
        &json!({"jsonrpc": "2.0", "id": 2, "method": "tools/call", "params": {"name": "list_devices", "arguments": {}}}),
    );
    assert_eq!(st, 200);
    let r = first_reply(&body);
    assert_eq!(r["result"]["isError"], false, "{r}");
    assert_eq!(
        r["result"]["structuredContent"]["profiles"][0]["name"],
        "Symulator"
    );
    let _ = child.kill();
    let _ = child.wait();
}
