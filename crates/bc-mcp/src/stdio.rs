//! Transport stdio (REQ-15-001): jedna wiadomość JSON-RPC na linię.
//!
//! Wątek czytający rozdziela wiadomości: odpowiedzi klienta trafiają do oczekujących żądań
//! serwera (elicitation), `notifications/cancelled` ustawia flagę anulowania, a `stop_job`
//! wykonuje się od razu ścieżką awaryjną — także gdy wątek główny jest zajęty (REQ-15-106).
//! Wątek główny wykonuje żądania po kolei, co [`TICK`] wysyła powiadomienia o zmianach
//! subskrybowanych zasobów i sprawdza watchdog joga. EOF = utrata klienta: jog anulowany,
//! a serwer czeka, aż zadania na urządzeniach się skończą (REQ-15-107).

use std::collections::HashMap;
use std::io::{BufRead, Write};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use crate::{err, id_key, is_cancelled, Channel, Ctx, Server, CONFIRM_TIMEOUT};

/// Okres sprawdzania subskrypcji i watchdoga.
pub(crate) const TICK: Duration = Duration::from_millis(100);

type Out = Arc<Mutex<Box<dyn Write + Send>>>;
type Pending = Arc<Mutex<HashMap<String, mpsc::Sender<Value>>>>;

fn send(out: &Out, v: &Value) -> std::io::Result<()> {
    let mut w = out.lock().expect("stdout");
    serde_json::to_writer(&mut *w, v)?;
    w.write_all(b"\n")?;
    w.flush()
}

struct StdioChannel {
    out: Out,
    pending: Pending,
    next: AtomicU64,
}

impl Channel for StdioChannel {
    fn notify(&self, msg: Value) {
        let _ = send(&self.out, &msg);
    }

    fn request(&self, method: &str, params: Value, cancel: &AtomicBool) -> Result<Value, String> {
        let id = json!(format!("bc-{}", self.next.fetch_add(1, Ordering::Relaxed)));
        let (tx, rx) = mpsc::channel();
        self.pending
            .lock()
            .expect("oczekujące")
            .insert(id_key(&id), tx);
        let res = (|| {
            send(
                &self.out,
                &json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}),
            )
            .map_err(|e| format!("zapis: {e}"))?;
            let end = Instant::now() + CONFIRM_TIMEOUT;
            loop {
                if is_cancelled(cancel) {
                    return Err("anulowano".to_string());
                }
                match rx.recv_timeout(TICK) {
                    Ok(v) => return reply_result(v),
                    Err(RecvTimeoutError::Timeout) if Instant::now() < end => continue,
                    Err(RecvTimeoutError::Timeout) => return Err("brak odpowiedzi".into()),
                    Err(RecvTimeoutError::Disconnected) => return Err("klient rozłączony".into()),
                }
            }
        })();
        self.pending
            .lock()
            .expect("oczekujące")
            .remove(&id_key(&id));
        res
    }
}

/// Wynik odpowiedzi klienta na żądanie serwera.
pub(crate) fn reply_result(v: Value) -> Result<Value, String> {
    match v.get("error") {
        Some(e) => Err(e
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("błąd klienta")
            .to_string()),
        None => Ok(v.get("result").cloned().unwrap_or(Value::Null)),
    }
}

pub(crate) fn run<R, W>(s: &mut Server, input: R, output: W) -> std::io::Result<()>
where
    R: BufRead + Send + 'static,
    W: Write + Send + 'static,
{
    let out: Out = Arc::new(Mutex::new(Box::new(output)));
    let pending: Pending = Arc::new(Mutex::new(HashMap::new()));
    let inflight: Arc<Mutex<HashMap<String, Arc<AtomicBool>>>> = Arc::default();
    let client = Arc::new(Mutex::new(s.client_name(0)));
    let (tx, rx) = mpsc::channel::<Value>();
    let emergency = s.emergency();

    let reader = {
        let (out, pending, inflight, client) = (
            out.clone(),
            pending.clone(),
            inflight.clone(),
            client.clone(),
        );
        std::thread::Builder::new()
            .name("mcp-stdin".into())
            .spawn(move || {
                for line in input.lines() {
                    let Ok(line) = line else { break };
                    // niektóre powłoki (np. Windows PowerShell 5.1) poprzedzają strumień znakiem BOM
                    let line = line.trim_start_matches('\u{feff}');
                    if line.trim().is_empty() {
                        continue;
                    }
                    let v = match serde_json::from_str::<Value>(line) {
                        Ok(v) if v.is_object() => v,
                        Ok(_) => {
                            let _ = send(
                                &out,
                                &err(Value::Null, -32600, "oczekiwano obiektu JSON-RPC"),
                            );
                            continue;
                        }
                        Err(e) => {
                            let _ = send(
                                &out,
                                &err(Value::Null, -32700, &format!("błąd parsowania: {e}")),
                            );
                            continue;
                        }
                    };
                    let method = v.get("method").and_then(Value::as_str);
                    match method {
                        None => {
                            // odpowiedź klienta na żądanie serwera
                            if let Some(id) = v.get("id") {
                                if let Some(p) =
                                    pending.lock().expect("oczekujące").remove(&id_key(id))
                                {
                                    let _ = p.send(v);
                                }
                            }
                            continue;
                        }
                        Some("notifications/cancelled") => {
                            if let Some(rid) = v.pointer("/params/requestId") {
                                if let Some(f) = inflight.lock().expect("w toku").get(&id_key(rid))
                                {
                                    f.store(true, Ordering::SeqCst);
                                }
                            }
                            continue;
                        }
                        Some("tools/call")
                            if v.pointer("/params/name").and_then(Value::as_str)
                                == Some("stop_job")
                                && v.get("id").is_some() =>
                        {
                            // stop poza kolejką (REQ-15-072, 106)
                            let name = client.lock().expect("klient").clone();
                            let r =
                                emergency.stop_call(&name, v.get("params").unwrap_or(&Value::Null));
                            let _ =
                                send(&out, &json!({"jsonrpc": "2.0", "id": v["id"], "result": r}));
                            continue;
                        }
                        _ => {}
                    }
                    if tx.send(v).is_err() {
                        break;
                    }
                }
                // EOF: oczekujące żądania serwera (potwierdzenia) kończą się od razu
                pending.lock().expect("oczekujące").clear();
            })?
    };

    let chan: Arc<dyn Channel> = Arc::new(StdioChannel {
        out: out.clone(),
        pending,
        next: AtomicU64::new(1),
    });
    let mut last_tick = Instant::now();
    let mut lost = false;
    loop {
        match rx.recv_timeout(TICK) {
            Ok(msg) => {
                let key = msg
                    .get("id")
                    .filter(|_| msg.get("method").is_some())
                    .map(id_key);
                let cancel = Arc::new(AtomicBool::new(false));
                if let Some(k) = &key {
                    inflight
                        .lock()
                        .expect("w toku")
                        .insert(k.clone(), cancel.clone());
                }
                let ctx = Ctx {
                    peer: 0,
                    channel: Some(chan.clone()),
                    cancel: cancel.clone(),
                };
                let reply = s.handle_ctx(msg, &ctx);
                if let Some(k) = &key {
                    inflight.lock().expect("w toku").remove(k);
                }
                *client.lock().expect("klient") = s.client_name(0);
                // anulowane żądanie nie dostaje odpowiedzi (specyfikacja MCP)
                if let Some(r) = reply.filter(|_| !is_cancelled(&cancel)) {
                    if send(&out, &r).is_err() {
                        lost = true;
                        break;
                    }
                }
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => break,
        }
        if last_tick.elapsed() >= TICK {
            last_tick = Instant::now();
            s.watchdog_tick(Instant::now());
            for n in s.poll_updates(0) {
                if send(&out, &n).is_err() {
                    lost = true;
                }
            }
            if lost {
                break;
            }
        }
    }
    // utrata klienta: jog anulowany, zadanie trwa (zachowanie jak w GUI)
    tracing::info!(write_failed = lost, "koniec wejścia MCP (utrata klienta)");
    s.client_lost(0);
    if s.emergency.jobs_active() {
        tracing::info!("czekam na zakończenie zadania na urządzeniu");
        while s.emergency.jobs_active() {
            std::thread::sleep(TICK);
        }
    }
    drop(reader);
    Ok(())
}
