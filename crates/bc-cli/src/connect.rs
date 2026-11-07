//! `burncraft-cli mcp --connect`: przezroczyste proxy stdio ↔ kanał sterowania
//! hosta (metoda `mcp`, ADR 0007). Stan i polityka żyją w hoście.

use std::io::{BufRead, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde_json::{json, Value};
use tungstenite::client::IntoClientRequest;
use tungstenite::{Message, WebSocket};

pub type Ws = WebSocket<TcpStream>;

/// Domyślny plik tokenu hosta na danym porcie.
pub fn default_token_file(data_dir: Option<PathBuf>, port: u16) -> Option<PathBuf> {
    data_dir.map(|d| d.join(format!("control-{port}.token")))
}

/// `Ok(None)` — nikt nie słucha na porcie (sesja bezgłowa); `Err` — okno jest,
/// ale połączenie odrzucono (np. zły token).
pub fn connect(port: u16, token_file: Option<&Path>) -> Result<Option<Ws>, String> {
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let Ok(stream) = TcpStream::connect_timeout(&addr, Duration::from_millis(500)) else {
        return Ok(None);
    };
    let token = match std::env::var("BURNCRAFT_TOKEN") {
        Ok(t) if !t.trim().is_empty() => t.trim().to_string(),
        _ => {
            let f = token_file.ok_or("nieznany katalog danych — podaj --token-file")?;
            std::fs::read_to_string(f)
                .map_err(|e| format!("token hosta {}: {e}", f.display()))?
                .trim()
                .to_string()
        }
    };
    let req = format!("ws://{addr}/rpc?token={token}")
        .into_client_request()
        .map_err(|e| e.to_string())?;
    match tungstenite::client(req, stream) {
        Ok((ws, _)) => Ok(Some(ws)),
        Err(tungstenite::HandshakeError::Failure(tungstenite::Error::Http(r))) => Err(format!(
            "host BurnCraft na {addr} odrzucił połączenie (HTTP {})",
            r.status().as_u16()
        )),
        Err(e) => Err(format!("{addr}: {e}")),
    }
}

/// Pętla proxy: jedna wiadomość MCP na linię stdin → odpowiedź na stdout.
pub fn run(mut ws: Ws) -> Result<(), String> {
    let stdin = std::io::stdin();
    let mut out = std::io::stdout().lock();
    let mut seq: u64 = 0;
    for line in stdin.lock().lines() {
        let line = line.map_err(|e| e.to_string())?;
        let line = line.trim_start_matches('\u{feff}');
        if line.trim().is_empty() {
            continue;
        }
        let msg: Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(e) => {
                let r = json!({"jsonrpc": "2.0", "id": null, "error": {"code": -32700, "message": format!("błąd parsowania: {e}")}});
                writeln!(out, "{r}").map_err(|e| e.to_string())?;
                out.flush().map_err(|e| e.to_string())?;
                continue;
            }
        };
        seq += 1;
        let orig_id = msg.get("id").cloned();
        let wrapped = json!({"jsonrpc": "2.0", "id": seq, "method": "mcp", "params": msg});
        ws.send(Message::Text(wrapped.to_string().into()))
            .map_err(|e| format!("kanał sterowania: {e}"))?;
        let reply = loop {
            let m = ws.read().map_err(|e| format!("kanał sterowania: {e}"))?;
            let Message::Text(t) = m else { continue };
            let Ok(v) = serde_json::from_str::<Value>(&t) else {
                continue;
            };
            if v.get("method").is_some() {
                // powiadomienia hosta (doc.changed) i żądania do okien — nie dotyczą agenta
                if let Some(id) = v.get("id") {
                    let e = json!({"jsonrpc": "2.0", "id": id, "error": {"code": -32601, "message": "klient MCP nie jest oknem"}});
                    let _ = ws.send(Message::Text(e.to_string().into()));
                }
                continue;
            }
            if v.get("id") == Some(&json!(seq)) {
                break v;
            }
        };
        let response = match (reply.get("result"), reply.get("error")) {
            (Some(Value::Null) | None, None) => None,
            (Some(r), None) => Some(r.clone()),
            (_, Some(e)) => orig_id.map(|id| json!({"jsonrpc": "2.0", "id": id, "error": e})),
        };
        if let Some(r) = response {
            writeln!(out, "{r}").map_err(|e| e.to_string())?;
            out.flush().map_err(|e| e.to_string())?;
        }
    }
    let _ = ws.close(None);
    Ok(())
}
