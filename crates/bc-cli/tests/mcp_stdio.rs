//! Test end-to-end: proces `burncraft-cli mcp` rozmawia po stdio jak klient MCP.

use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};

use serde_json::{json, Value};

fn rpc(lines: &[Value], extra_args: &[&str]) -> Vec<Value> {
    let mut child = Command::new(env!("CARGO_BIN_EXE_burncraft-cli"))
        .arg("mcp")
        .arg("--no-audit-log")
        .args(extra_args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("uruchomienie burncraft-cli");
    {
        let mut stdin = child.stdin.take().unwrap();
        for l in lines {
            writeln!(stdin, "{l}").unwrap();
        }
    } // zamknięcie stdin kończy serwer
    let out = BufReader::new(child.stdout.take().unwrap());
    let replies: Vec<Value> = out
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
fn req_20_012_cli_mcp_stdio_four_tools() {
    let replies = rpc(
        &[
            init(),
            json!({"jsonrpc": "2.0", "method": "notifications/initialized"}),
            tool(2, "list_commands", json!({"filter": "shape"})),
            tool(
                3,
                "execute",
                json!({"id": "shape.rect", "params": {"x": 20, "y": 20, "w": 120, "h": 80, "radius": 4}}),
            ),
            tool(4, "inspect_document", json!({})),
            tool(5, "render_preview", json!({"max_size": 128})),
        ],
        &[],
    );
    assert_eq!(replies.len(), 5, "powiadomienie nie dostaje odpowiedzi");
    assert_eq!(replies[0]["result"]["serverInfo"]["name"], "burncraft");
    assert!(
        replies[1]["result"]["structuredContent"]["commands"]
            .as_array()
            .unwrap()
            .len()
            >= 3
    );
    assert_eq!(replies[2]["result"]["isError"], false);
    assert_eq!(
        replies[3]["result"]["structuredContent"]["stats"]["objects"],
        1
    );
    assert_eq!(replies[4]["result"]["content"][0]["mimeType"], "image/png");
}

#[test]
fn req_01_044_connect_without_window_runs_headless() {
    // port, na którym nic nie słucha -> sesja bezgłowa zamiast błędu
    let replies = rpc(
        &[init(), tool(2, "inspect_document", json!({}))],
        &["--connect", "1"],
    );
    assert_eq!(replies.len(), 2);
    assert_eq!(
        replies[1]["result"]["structuredContent"]["stats"]["objects"],
        0
    );
}

#[test]
fn req_15_001_headless_stdio_and_malformed_input() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_burncraft-cli"))
        .args(["mcp", "--no-audit-log"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    {
        let mut stdin = child.stdin.take().unwrap();
        writeln!(stdin, "to nie json").unwrap();
        writeln!(
            stdin,
            "{}",
            json!({"jsonrpc": "2.0", "id": 7, "method": "ping"})
        )
        .unwrap();
    }
    let lines: Vec<Value> = BufReader::new(child.stdout.take().unwrap())
        .lines()
        .map(|l| serde_json::from_str(&l.unwrap()).unwrap())
        .collect();
    child.wait().unwrap();
    assert_eq!(lines[0]["error"]["code"], -32700);
    assert_eq!(lines[1]["id"], 7);
}

#[test]
fn req_14_011_cli_open_prints_summary() {
    let dir = std::env::temp_dir().join(format!("bc-cli-open-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("t.bcraft");
    let r = rpc(
        &[
            init(),
            tool(
                2,
                "execute",
                json!({"id": "shape.ellipse", "params": {"cx": 5, "cy": 5, "rx": 2, "ry": 2}}),
            ),
            tool(
                3,
                "execute",
                json!({"id": "file.save", "params": {"path": file.to_str().unwrap()}}),
            ),
        ],
        &[],
    );
    assert_eq!(r[2]["result"]["isError"], false, "{}", r[2]);
    let out = Command::new(env!("CARGO_BIN_EXE_burncraft-cli"))
        .args(["open"])
        .arg(&file)
        .output()
        .unwrap();
    assert!(out.status.success());
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["stats"]["objects"], 1);
    // ten sam plik otwierany przez MCP
    let r2 = rpc(
        &[init(), tool(2, "inspect_document", json!({}))],
        &["--open", file.to_str().unwrap()],
    );
    assert_eq!(
        r2[1]["result"]["structuredContent"]["objects"][0]["type"],
        "ellipse"
    );
    std::fs::remove_dir_all(&dir).ok();
}
