//! Klienci MCP do testów M2.4: stdio (rury, `Server::run_io` w wątku) i Streamable HTTP
//! (surowe HTTP/1.1 po TCP, odczyt SSE z kodowaniem chunked). Urządzenia na emulatorze.

#![allow(dead_code)]

use std::collections::{HashMap, VecDeque};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use bc_commands::{SafetyTier, Session};
use bc_drivers::emu::{EmuConfig, GrblEmulator};
use bc_drivers::grbl::{GrblDriver, GrblOptions};
use bc_drivers::machines::DeviceProfile;
use bc_drivers::transport::EmuTransport;
use bc_drivers::MachinesHandle;
use bc_mcp::policy::Policy;
use bc_mcp::{Config, Server};
use serde_json::{json, Value};

pub const WAIT: Duration = Duration::from_secs(20);

pub fn tmp(name: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("bc-mcp24-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// Konfiguracja testu: preferencje w katalogu tymczasowym, polityka `auto_allow`.
pub fn config(name: &str, auto_allow: SafetyTier) -> Config {
    Config {
        prefs_dir: Some(tmp(name)),
        audit_log: Some(tmp(&format!("{name}-audit")).join("audit.jsonl")),
        policy: Policy {
            auto_allow,
            remote: false,
        },
        ..Default::default()
    }
}

pub fn server(cfg: Config) -> (Server, MachinesHandle) {
    let s = Server::new(Session::default(), cfg);
    let m = s.session().machines.clone();
    (s, m)
}

/// Emulator współdzielony z testem (E-stop, bazowanie) dołączony jako urządzenie `name`.
pub fn attach_emu(m: &MachinesHandle, name: &str, cfg: EmuConfig) -> Arc<Mutex<GrblEmulator>> {
    let emu = Arc::new(Mutex::new(GrblEmulator::new(cfg)));
    let d = GrblDriver::connect(
        name,
        Box::new(EmuTransport::new(emu.clone())),
        GrblOptions {
            poll_ms: 20,
            ..Default::default()
        },
    )
    .expect("połączenie z emulatorem");
    let p: DeviceProfile = serde_json::from_value(
        json!({"name": name, "transport": {"kind": "emulator", "variant": "grbl11"}}),
    )
    .unwrap();
    m.lock().unwrap().attach(p, Box::new(d));
    emu
}

/// Stan zadania urządzenia bez MCP (bezpośrednio przez sterownik).
pub fn job_of(m: &MachinesHandle, dev: &str) -> bc_drivers::driver::JobState {
    m.lock().unwrap().driver(dev).expect("urządzenie").job()
}

pub fn state_of(m: &MachinesHandle, dev: &str) -> bc_drivers::driver::MachineState {
    m.lock().unwrap().driver(dev).expect("urządzenie").status()
}

pub fn wait_until(what: &str, timeout: Duration, mut f: impl FnMut() -> bool) {
    let end = Instant::now() + timeout;
    while !f() {
        assert!(Instant::now() < end, "przekroczony czas: {what}");
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// Profil emulatora (`speed` = mnożnik czasu, brak = natychmiast).
pub fn profile_json(name: &str, speed: Option<f64>) -> Value {
    let mut t = json!({"kind": "emulator", "variant": "grbl11"});
    if let Some(s) = speed {
        t["speed"] = json!(s);
    }
    json!({"profile": {"name": name, "transport": t}})
}

// ---------------------------------------------------------------- stdio

pub struct Stdio {
    input: Option<std::io::PipeWriter>,
    rx: mpsc::Receiver<Value>,
    pub thread: Option<std::thread::JoinHandle<std::io::Result<()>>>,
    /// Powiadomienia i żądania serwera odebrane przy oczekiwaniu na odpowiedzi.
    pub inbox: VecDeque<Value>,
    next: u64,
}

impl Stdio {
    /// Uruchamia `Server::run_io` w wątku i wykonuje `initialize` z podanymi możliwościami klienta.
    pub fn start(mut server: Server, capabilities: Value) -> Self {
        let (in_r, in_w) = std::io::pipe().unwrap();
        let (out_r, out_w) = std::io::pipe().unwrap();
        let thread = std::thread::spawn(move || server.run_io(BufReader::new(in_r), out_w));
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(out_r).lines() {
                let Ok(line) = line else { break };
                let v: Value = serde_json::from_str(&line)
                    .unwrap_or_else(|e| panic!("serwer wysłał nie-JSON: {line}: {e}"));
                if tx.send(v).is_err() {
                    break;
                }
            }
        });
        let mut c = Stdio {
            input: Some(in_w),
            rx,
            thread: Some(thread),
            inbox: VecDeque::new(),
            next: 100,
        };
        let r = c.request(
            "initialize",
            json!({"protocolVersion": "2025-06-18", "capabilities": capabilities,
                   "clientInfo": {"name": "test-stdio", "version": "1"}}),
        );
        assert!(r.get("result").is_some(), "initialize: {r}");
        c.send(json!({"jsonrpc": "2.0", "method": "notifications/initialized"}));
        c
    }

    pub fn send(&mut self, v: Value) {
        let w = self.input.as_mut().expect("wejście otwarte");
        writeln!(w, "{v}").unwrap();
        w.flush().unwrap();
    }

    pub fn next_id(&mut self) -> u64 {
        self.next += 1;
        self.next
    }

    /// Następna wiadomość od serwera (z `inbox` albo z rury).
    pub fn recv(&mut self, timeout: Duration) -> Option<Value> {
        if let Some(v) = self.inbox.pop_front() {
            return Some(v);
        }
        self.rx.recv_timeout(timeout).ok()
    }

    /// Czeka na odpowiedź o danym id; pozostałe wiadomości trafiają do `inbox`.
    pub fn wait_response(&mut self, id: &Value) -> Value {
        let end = Instant::now() + WAIT;
        let mut kept = VecDeque::new();
        // najpierw zaległe w inbox
        while let Some(v) = self.inbox.pop_front() {
            if v.get("id") == Some(id) && v.get("method").is_none() {
                self.inbox = kept;
                return v;
            }
            kept.push_back(v);
        }
        self.inbox = kept;
        loop {
            let left = end.saturating_duration_since(Instant::now());
            let v = self
                .rx
                .recv_timeout(left)
                .unwrap_or_else(|_| panic!("brak odpowiedzi na id {id}"));
            if v.get("id") == Some(id) && v.get("method").is_none() {
                return v;
            }
            self.inbox.push_back(v);
        }
    }

    pub fn request(&mut self, method: &str, params: Value) -> Value {
        let id = json!(self.next_id());
        self.send(json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}));
        self.wait_response(&id)
    }

    /// Wynik `tools/call` (pole `result`).
    pub fn call_raw(&mut self, tool: &str, args: Value) -> Value {
        let r = self.request("tools/call", json!({"name": tool, "arguments": args}));
        r.get("result")
            .cloned()
            .unwrap_or_else(|| panic!("{tool}: błąd protokołu {r}"))
    }

    /// `structuredContent` udanego wywołania.
    pub fn call(&mut self, tool: &str, args: Value) -> Value {
        let r = self.call_raw(tool, args);
        assert_eq!(r["isError"], false, "{tool}: {}", r["content"][0]["text"]);
        r["structuredContent"].clone()
    }

    pub fn exec(&mut self, id: &str, params: Value) -> Value {
        self.call("execute", json!({"id": id, "params": params}))["result"].clone()
    }

    /// Powiadomienia o danej metodzie zebrane w `inbox` (usuwa je).
    pub fn take(&mut self, method: &str) -> Vec<Value> {
        let (hit, rest): (VecDeque<Value>, VecDeque<Value>) =
            self.inbox.drain(..).partition(|v| v["method"] == method);
        self.inbox = rest;
        hit.into_iter().collect()
    }

    /// Czeka na powiadomienie/żądanie serwera spełniające warunek.
    pub fn wait_for(&mut self, what: &str, timeout: Duration, f: impl Fn(&Value) -> bool) -> Value {
        if let Some(i) = self.inbox.iter().position(&f) {
            return self.inbox.remove(i).unwrap();
        }
        let end = Instant::now() + timeout;
        loop {
            let left = end.saturating_duration_since(Instant::now());
            let v = self
                .rx
                .recv_timeout(left)
                .unwrap_or_else(|_| panic!("nie nadeszło: {what}"));
            if f(&v) {
                return v;
            }
            self.inbox.push_back(v);
        }
    }

    /// Zbiera wiadomości przez `d` (do `inbox`).
    pub fn drain_for(&mut self, d: Duration) {
        let end = Instant::now() + d;
        while let Ok(v) = self
            .rx
            .recv_timeout(end.saturating_duration_since(Instant::now()))
        {
            self.inbox.push_back(v);
        }
    }

    /// Zamyka wejście (EOF = utrata klienta) bez czekania na koniec serwera.
    pub fn close_input(&mut self) {
        self.input = None;
    }

    /// Zamyka wejście i czeka na koniec `run_io`.
    pub fn finish(mut self) -> std::io::Result<()> {
        self.input = None;
        self.thread.take().unwrap().join().expect("wątek serwera")
    }
}

// ---------------------------------------------------------------- HTTP

pub struct Http {
    pub addr: SocketAddr,
    pub session: Option<String>,
    next: u64,
}

pub struct Resp {
    pub status: u16,
    pub headers: HashMap<String, String>,
    reader: BufReader<TcpStream>,
    chunked: bool,
    body: Vec<u8>,
    buf: String,
    ended: bool,
}

impl Resp {
    pub fn header(&self, k: &str) -> Option<&str> {
        self.headers
            .get(&k.to_ascii_lowercase())
            .map(String::as_str)
    }

    pub fn is_sse(&self) -> bool {
        self.header("content-type")
            .is_some_and(|c| c.starts_with("text/event-stream"))
    }

    fn read_chunk(&mut self) -> bool {
        if self.ended {
            return false;
        }
        if !self.chunked {
            let mut b = [0u8; 4096];
            return match self.reader.read(&mut b) {
                Ok(0) | Err(_) => {
                    self.ended = true;
                    false
                }
                Ok(n) => {
                    self.body.extend_from_slice(&b[..n]);
                    true
                }
            };
        }
        let mut line = String::new();
        if self.reader.read_line(&mut line).unwrap_or(0) == 0 {
            self.ended = true;
            return false;
        }
        let n = usize::from_str_radix(line.trim().split(';').next().unwrap_or("0"), 16)
            .unwrap_or_else(|_| panic!("zły rozmiar chunk: {line:?}"));
        if n == 0 {
            self.ended = true;
            return false;
        }
        let mut b = vec![0u8; n + 2];
        self.reader.read_exact(&mut b).expect("chunk");
        self.body.extend_from_slice(&b[..n]);
        true
    }

    /// Całe ciało (do końca odpowiedzi).
    pub fn text(mut self) -> String {
        if let Some(n) = self
            .header("content-length")
            .and_then(|v| v.parse::<usize>().ok())
        {
            let mut b = vec![0u8; n];
            self.reader.read_exact(&mut b).expect("ciało");
            return String::from_utf8(b).unwrap();
        }
        while self.read_chunk() {}
        String::from_utf8(self.body).unwrap()
    }

    pub fn json(self) -> Value {
        let t = self.text();
        serde_json::from_str(&t).unwrap_or_else(|e| panic!("nie-JSON: {t}: {e}"))
    }

    /// Następne zdarzenie SSE (pole data jako JSON); `None` = koniec strumienia albo czas minął.
    pub fn next_event(&mut self, timeout: Duration) -> Option<Value> {
        let _ = self.reader.get_ref().set_read_timeout(Some(timeout));
        let end = Instant::now() + timeout;
        loop {
            self.buf
                .push_str(&String::from_utf8_lossy(&std::mem::take(&mut self.body)));
            let norm = self.buf.replace("\r\n", "\n");
            if let Some(i) = norm.find("\n\n") {
                let ev = norm[..i].to_string();
                self.buf = norm[i + 2..].to_string();
                let data: Vec<&str> = ev
                    .lines()
                    .filter_map(|l| l.strip_prefix("data:"))
                    .map(str::trim_start)
                    .collect();
                if data.is_empty() {
                    continue; // komentarz / keep-alive
                }
                return Some(serde_json::from_str(&data.join("\n")).expect("JSON w SSE"));
            }
            self.buf = norm;
            if Instant::now() >= end || !self.read_chunk() {
                return None;
            }
        }
    }
}

impl Http {
    pub fn new(addr: SocketAddr) -> Self {
        Http {
            addr,
            session: None,
            next: 1000,
        }
    }

    pub fn next_id(&mut self) -> u64 {
        self.next += 1;
        self.next
    }

    /// Surowe żądanie HTTP/1.1 (Connection: close).
    pub fn raw(&self, method: &str, body: Option<&str>, headers: &[(&str, &str)]) -> Resp {
        let mut s = TcpStream::connect(self.addr).expect("połączenie HTTP");
        s.set_read_timeout(Some(WAIT)).unwrap();
        let mut req = format!("{method} /mcp HTTP/1.1\r\nConnection: close\r\n");
        if !headers.iter().any(|(k, _)| k.eq_ignore_ascii_case("host")) {
            req.push_str(&format!("Host: {}\r\n", self.addr));
        }
        for (k, v) in headers {
            req.push_str(&format!("{k}: {v}\r\n"));
        }
        if let Some(b) = body {
            req.push_str(&format!(
                "Content-Type: application/json\r\nContent-Length: {}\r\n",
                b.len()
            ));
        }
        req.push_str("\r\n");
        if let Some(b) = body {
            req.push_str(b);
        }
        s.write_all(req.as_bytes()).unwrap();
        let mut reader = BufReader::new(s);
        let mut line = String::new();
        reader.read_line(&mut line).expect("wiersz statusu");
        let status: u16 = line
            .split_whitespace()
            .nth(1)
            .unwrap_or_else(|| panic!("zły status: {line:?}"))
            .parse()
            .unwrap();
        let mut headers = HashMap::new();
        loop {
            let mut h = String::new();
            reader.read_line(&mut h).unwrap();
            let h = h.trim_end();
            if h.is_empty() {
                break;
            }
            if let Some((k, v)) = h.split_once(':') {
                headers.insert(k.trim().to_ascii_lowercase(), v.trim().to_string());
            }
        }
        let chunked = headers
            .get("transfer-encoding")
            .is_some_and(|v| v.contains("chunked"));
        Resp {
            status,
            headers,
            reader,
            chunked,
            body: Vec::new(),
            buf: String::new(),
            ended: false,
        }
    }

    fn std_headers(&self) -> Vec<(&str, &str)> {
        let mut h = vec![
            ("Accept", "application/json, text/event-stream"),
            ("MCP-Protocol-Version", "2025-06-18"),
        ];
        if let Some(s) = &self.session {
            h.push(("Mcp-Session-Id", s.as_str()));
        }
        h
    }

    pub fn post(&self, msg: &Value) -> Resp {
        let h = self.std_headers();
        self.raw("POST", Some(&msg.to_string()), &h)
    }

    /// POST z dodatkowymi nagłówkami (zastępują domyślne o tej samej nazwie).
    pub fn post_with(&self, msg: &Value, extra: &[(&str, &str)]) -> Resp {
        let mut h = self.std_headers();
        h.retain(|(k, _)| !extra.iter().any(|(e, _)| e.eq_ignore_ascii_case(k)));
        h.extend_from_slice(extra);
        self.raw("POST", Some(&msg.to_string()), &h)
    }

    pub fn initialize(&mut self, capabilities: Value) -> Value {
        let id = self.next_id();
        let r = self.raw(
            "POST",
            Some(
                &json!({"jsonrpc": "2.0", "id": id, "method": "initialize",
                    "params": {"protocolVersion": "2025-06-18", "capabilities": capabilities,
                               "clientInfo": {"name": "test-http", "version": "1"}}})
                .to_string(),
            ),
            &[("Accept", "application/json, text/event-stream")],
        );
        assert_eq!(r.status, 200);
        self.session = Some(
            r.header("mcp-session-id")
                .expect("nagłówek Mcp-Session-Id")
                .to_string(),
        );
        let v = read_reply(r, id);
        let n = self.post(&json!({"jsonrpc": "2.0", "method": "notifications/initialized"}));
        assert_eq!(n.status, 202);
        v
    }

    /// Żądanie; odpowiedź JSON albo SSE (powiadomienia przed odpowiedzią są pomijane).
    pub fn request(&mut self, method: &str, params: Value) -> Value {
        let id = self.next_id();
        let r = self.post(&json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}));
        assert_eq!(r.status, 200, "{method}");
        read_reply(r, id)
    }

    pub fn call_raw(&mut self, tool: &str, args: Value) -> Value {
        let r = self.request("tools/call", json!({"name": tool, "arguments": args}));
        r.get("result")
            .cloned()
            .unwrap_or_else(|| panic!("{tool}: błąd protokołu {r}"))
    }

    pub fn call(&mut self, tool: &str, args: Value) -> Value {
        let r = self.call_raw(tool, args);
        assert_eq!(r["isError"], false, "{tool}: {}", r["content"][0]["text"]);
        r["structuredContent"].clone()
    }

    pub fn exec(&mut self, id: &str, params: Value) -> Value {
        self.call("execute", json!({"id": id, "params": params}))["result"].clone()
    }

    pub fn get_stream(&self) -> Resp {
        let mut h = vec![("Accept", "text/event-stream")];
        if let Some(s) = &self.session {
            h.push(("Mcp-Session-Id", s.as_str()));
        }
        self.raw("GET", None, &h)
    }

    pub fn delete(&self) -> Resp {
        let mut h = vec![];
        if let Some(s) = &self.session {
            h.push(("Mcp-Session-Id", s.as_str()));
        }
        self.raw("DELETE", None, &h)
    }
}

/// Odpowiedź o danym id z odpowiedzi JSON albo strumienia SSE.
pub fn read_reply(mut r: Resp, id: u64) -> Value {
    if r.is_sse() {
        while let Some(v) = r.next_event(WAIT) {
            if v["id"] == id && v.get("method").is_none() {
                return v;
            }
        }
        panic!("strumień SSE bez odpowiedzi na {id}");
    }
    r.json()
}
