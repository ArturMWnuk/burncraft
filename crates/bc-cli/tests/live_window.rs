//! Test end-to-end: `burncraft-cli serve` (host GUI bez okna) i dwa procesy
//! `burncraft-cli mcp --connect` działające na tym samym żywym dokumencie.

use std::io::{BufRead, BufReader, Write};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

use serde_json::{json, Value};

struct Serve {
    child: Child,
    port: u16,
}

impl Drop for Serve {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn serve(token_file: &std::path::Path) -> Serve {
    let mut child = Command::new(env!("CARGO_BIN_EXE_burncraft-cli"))
        .args([
            "serve",
            "--port",
            "0",
            "--no-audit-log",
            "--autosave-minutes",
            "0",
            "--token-file",
        ])
        .arg(token_file)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("uruchomienie burncraft-cli serve");
    // pierwsza linia stdout: adres hosta
    let out = child.stdout.take().unwrap();
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        for l in BufReader::new(out).lines().map_while(Result::ok) {
            let _ = tx.send(l);
        }
    });
    let line = rx
        .recv_timeout(Duration::from_secs(30))
        .expect("serve wypisuje adres");
    let port = line
        .trim()
        .trim_end_matches('/')
        .rsplit(':')
        .next()
        .and_then(|p| p.parse().ok())
        .unwrap_or_else(|| panic!("adres w {line:?}"));
    Serve { child, port }
}

fn mcp_connect(port: u16, token_file: &std::path::Path, lines: &[Value]) -> Vec<Value> {
    let mut child = Command::new(env!("CARGO_BIN_EXE_burncraft-cli"))
        .args([
            "mcp",
            "--no-audit-log",
            "--connect",
            &port.to_string(),
            "--token-file",
        ])
        .arg(token_file)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("uruchomienie burncraft-cli mcp --connect");
    {
        let mut stdin = child.stdin.take().unwrap();
        for l in lines {
            writeln!(stdin, "{l}").unwrap();
        }
    }
    let replies: Vec<Value> = BufReader::new(child.stdout.take().unwrap())
        .lines()
        .map(|l| serde_json::from_str(&l.unwrap()).expect("stdout zawiera tylko JSON-RPC"))
        .collect();
    assert!(child.wait().unwrap().success());
    replies
}

fn init() -> Value {
    json!({"jsonrpc": "2.0", "id": 1, "method": "initialize",
        "params": {"protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": {"name": "e2e", "version": "0"}}})
}

fn tool(id: u64, name: &str, args: Value) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "method": "tools/call", "params": {"name": name, "arguments": args}})
}

#[test]
fn req_01_043_mcp_connect_works_on_live_host() {
    let dir = std::env::temp_dir().join(format!("bc-cli-live-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let token = dir.join("control.token");
    let host = serve(&token);
    assert!(std::fs::read_to_string(&token).unwrap().trim().len() >= 32);

    let a = mcp_connect(
        host.port,
        &token,
        &[
            init(),
            json!({"jsonrpc": "2.0", "method": "notifications/initialized"}),
            tool(
                2,
                "execute",
                json!({"id": "shape.rect", "params": {"x": 1, "y": 2, "w": 30, "h": 40}}),
            ),
        ],
    );
    assert_eq!(a.len(), 2, "powiadomienie bez odpowiedzi: {a:?}");
    assert_eq!(a[0]["id"], 1);
    assert_eq!(a[1]["result"]["isError"], false, "{a:?}");

    // drugi agent widzi ten sam dokument — stan żyje w hoście, nie w procesie CLI
    let b = mcp_connect(
        host.port,
        &token,
        &[init(), tool(2, "inspect_document", json!({}))],
    );
    let objs = &b[1]["result"]["structuredContent"]["objects"];
    assert_eq!(objs.as_array().map(Vec::len), Some(1), "{b:?}");

    // zły token: CLI nie przechodzi po cichu na sesję bezgłową
    let bad = dir.join("bad.token");
    std::fs::write(&bad, "0".repeat(64)).unwrap();
    let st = Command::new(env!("CARGO_BIN_EXE_burncraft-cli"))
        .args([
            "mcp",
            "--no-audit-log",
            "--connect",
            &host.port.to_string(),
            "--token-file",
        ])
        .arg(&bad)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .unwrap();
    assert!(
        !st.success(),
        "odrzucony token = błąd, a nie cicha sesja bezgłowa"
    );
    drop(host);
    let _ = std::fs::remove_dir_all(&dir);
}
