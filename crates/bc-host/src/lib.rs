//! bc-host — host GUI BurnCraft (ADR 0007).
//!
//! Serwuje frontend (`web/dist`) i kanał JSON-RPC 2.0 po WebSocket (`/rpc`) na
//! 127.0.0.1. Właścicielem stanu jest jedna instancja [`bc_mcp::Server`]
//! (z `Workbench`): GUI wywołuje polecenia rejestru (`gui.*`), agent MCP
//! podłączony przez `burncraft-cli mcp --connect` — metodę `mcp`.

mod scene;
mod web;

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{mpsc as std_mpsc, Arc, Mutex};
use std::time::Duration;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use bc_commands::{BatchStep, CmdError, Registry, Session};
use serde_json::{json, Value};
use tokio::sync::{mpsc, oneshot};

/// Domyślny port kanału sterowania (spec. §1.4).
pub const DEFAULT_PORT: u16 = 7984;

/// Znacznik w `index.html` zastępowany tokenem sesji.
pub const TOKEN_PLACEHOLDER: &str = "%BURNCRAFT_TOKEN%";

/// Czas oczekiwania na zrzut ekranu od okna.
const SCREENSHOT_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, thiserror::Error)]
pub enum HostError {
    #[error("nie można uruchomić hosta: {0}")]
    Start(String),
}

#[derive(Default)]
pub struct HostConfig {
    /// Port na 127.0.0.1; 0 = dowolny wolny.
    pub port: u16,
    /// Katalog zbudowanego frontendu (index.html + zasoby).
    pub web_dir: Option<PathBuf>,
    /// Plik, do którego zapisać token sesji (dla `burncraft-cli mcp --connect`).
    pub token_file: Option<PathBuf>,
    /// Konfiguracja serwera MCP (audyt, autozapis, preferencje, polityka).
    pub mcp: bc_mcp::Config,
}

/// Działający host (w osobnym wątku z własnym środowiskiem tokio).
pub struct HostHandle {
    pub addr: SocketAddr,
    pub token: String,
    stop: Option<oneshot::Sender<()>>,
    thread: Option<std::thread::JoinHandle<()>>,
    token_file: Option<PathBuf>,
}

impl HostHandle {
    pub fn url(&self) -> String {
        format!("http://{}/", self.addr)
    }

    pub fn ws_url(&self) -> String {
        format!("ws://{}/rpc?token={}", self.addr, self.token)
    }

    /// Zatrzymuje host i czeka na zamknięcie.
    pub fn shutdown(mut self) {
        if let Some(s) = self.stop.take() {
            let _ = s.send(());
        }
        self.join();
    }

    /// Czeka, aż host zakończy pracę (tryb `serve`).
    pub fn wait(mut self) {
        self.join();
    }

    fn join(&mut self) {
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
        if let Some(f) = self.token_file.take() {
            let _ = std::fs::remove_file(f);
        }
    }
}

/// Podłączeni klienci i żądania serwera do okna (np. `ui.screenshot`).
#[derive(Default)]
struct Hub {
    clients: Mutex<HashMap<u64, Client>>,
    pending: Mutex<HashMap<String, std_mpsc::Sender<Value>>>,
    next: AtomicU64,
}

struct Client {
    tx: mpsc::UnboundedSender<String>,
    window: bool,
}

impl Hub {
    fn broadcast(&self, msg: &Value) {
        let text = msg.to_string();
        for c in self.clients.lock().unwrap().values() {
            let _ = c.tx.send(text.clone());
        }
    }

    /// Wysyła żądanie do pierwszego okna i czeka na odpowiedź (wątek blokujący).
    fn ask_window(&self, method: &str, params: Value) -> Result<Value, String> {
        let id = format!("s{}", self.next.fetch_add(1, Ordering::Relaxed));
        let (tx, rx) = std_mpsc::channel();
        {
            let clients = self.clients.lock().unwrap();
            let Some(win) = clients.values().find(|c| c.window) else {
                return Err("no_window".into());
            };
            self.pending.lock().unwrap().insert(id.clone(), tx);
            let req = json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params});
            let _ = win.tx.send(req.to_string());
        }
        let r = rx.recv_timeout(SCREENSHOT_TIMEOUT);
        self.pending.lock().unwrap().remove(&id);
        let r = r.map_err(|_| "okno nie odpowiedziało".to_string())?;
        match r.get("error") {
            Some(e) => Err(e
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("błąd okna")
                .to_string()),
            None => Ok(r.get("result").cloned().unwrap_or(Value::Null)),
        }
    }
}

/// Po tym czasie od startu polecenia (i nie częściej) host wysyła `gui.progress` (REQ-16-006).
const PROGRESS_DELAY: Duration = Duration::from_millis(100);

/// Postęp jednego polecenia wysyłany klientowi, który je wywołał. Polecenie zgłasza
/// postęp (`report`), a wątek nadzorujący co [`PROGRESS_DELAY`] wysyła go oknu — także
/// w fazach bez zgłoszeń, więc pasek postępu widać przy każdej dłuższej operacji.
struct ProgressJob {
    command: String,
    tx: Option<mpsc::UnboundedSender<String>>,
    /// (postęp 0–1, czy pokazano pasek, czy zakończono) — pod jednym zamkiem, żeby
    /// powiadomienie końcowe zawsze było ostatnie.
    state: Mutex<(f64, bool, bool)>,
}

impl ProgressJob {
    fn start(command: String, tx: Option<mpsc::UnboundedSender<String>>) -> Arc<Self> {
        let job = Arc::new(ProgressJob {
            command,
            tx,
            state: Mutex::new((0.0, false, false)),
        });
        let j = job.clone();
        std::thread::spawn(move || loop {
            std::thread::sleep(PROGRESS_DELAY);
            let mut st = j.state.lock().unwrap();
            if st.2 {
                break;
            }
            st.1 = true;
            j.send(st.0, false);
        });
        job
    }

    fn report(&self, fraction: f64) {
        let mut st = self.state.lock().unwrap();
        st.0 = st.0.max(fraction.clamp(0.0, 1.0));
    }

    fn send(&self, fraction: f64, done: bool) {
        if let Some(tx) = &self.tx {
            let _ = tx.send(
                json!({"jsonrpc": "2.0", "method": "gui.progress",
                    "params": {"command": self.command, "fraction": fraction, "done": done}})
                .to_string(),
            );
        }
    }

    /// Kończy nadzór i zamyka pasek postępu (przed odpowiedzią na polecenie), jeśli był pokazany.
    fn finish(&self) {
        let mut st = self.state.lock().unwrap();
        st.2 = true;
        if st.1 {
            self.send(1.0, true);
        }
    }
}

struct Shared {
    server: Mutex<Core>,
    hub: Arc<Hub>,
    /// Trwa polecenie z postępem (gui.execute / gui.batch / mcp).
    busy: AtomicBool,
    /// Anulowanie bieżącego polecenia (`gui.cancel`).
    cancel: Arc<AtomicBool>,
    token: String,
    port: u16,
    web_dir: Option<PathBuf>,
}

/// Stan pod jednym zamkiem: serwer MCP, rejestr dla GUI, licznik zmian.
struct Core {
    mcp: bc_mcp::Server,
    reg: Registry,
    revision: u64,
    fingerprint: u64,
}

impl Core {
    /// Skrót stanu widocznego dla klientów; zmiana = `doc.changed`.
    fn fingerprint(&self) -> u64 {
        let b = self.mcp.workbench();
        let mut h = std::collections::hash_map::DefaultHasher::new();
        b.active.hash(&mut h);
        for d in &b.docs {
            d.uid.hash(&mut h);
            d.revision.hash(&mut h);
            d.selection.hash(&mut h);
            d.dirty.hash(&mut h);
            // wybrane urządzenie: scena niesie możliwość PWM (pole mocy, REQ-13-045)
            d.device.hash(&mut h);
        }
        h.finish()
    }

    /// Zwraca powiadomienie, jeśli stan się zmienił.
    fn changed(&mut self) -> Option<Value> {
        let fp = self.fingerprint();
        if fp == self.fingerprint {
            return None;
        }
        self.fingerprint = fp;
        self.revision += 1;
        Some(json!({"jsonrpc": "2.0", "method": "doc.changed",
            "params": {"revision": self.revision, "active": self.mcp.workbench().active}}))
    }
}

/// Uruchamia host w tle.
pub fn spawn(session: Session, cfg: HostConfig) -> Result<HostHandle, HostError> {
    let listener = std::net::TcpListener::bind(("127.0.0.1", cfg.port))
        .map_err(|e| HostError::Start(format!("127.0.0.1:{}: {e}", cfg.port)))?;
    listener
        .set_nonblocking(true)
        .map_err(|e| HostError::Start(e.to_string()))?;
    let addr = listener
        .local_addr()
        .map_err(|e| HostError::Start(e.to_string()))?;
    let token = new_token()?;
    if let Some(f) = &cfg.token_file {
        if let Some(dir) = f.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        std::fs::write(f, &token).map_err(|e| HostError::Start(format!("{}: {e}", f.display())))?;
    }

    let hub = Arc::new(Hub::default());
    let mut mcp_cfg = cfg.mcp;
    let h = hub.clone();
    mcp_cfg.screenshot = Some(bc_mcp::ScreenshotHook(Arc::new(move || {
        let r = h.ask_window("ui.screenshot", json!({}))?;
        r.get("png")
            .and_then(Value::as_str)
            .map(str::to_string)
            .ok_or_else(|| "okno nie zwróciło PNG".to_string())
    })));
    let mut core = Core {
        mcp: bc_mcp::Server::new(session, mcp_cfg),
        reg: Registry::builtin(),
        revision: 0,
        fingerprint: 0,
    };
    // polecenia okna (widok, narzędzia) wykonuje okno GUI (REQ-01-029)
    let h = hub.clone();
    core.mcp.workbench_mut().window = Some(bc_commands::WindowHook(Arc::new(move |id, params| {
        h.ask_window("ui.execute", json!({"id": id, "params": params}))
            .map_err(|e| {
                if e == "no_window" {
                    CmdError::new(
                        "no_window",
                        format!(
                            "{id} wykonuje okno GUI BurnCraft, a żadne okno nie jest podłączone"
                        ),
                    )
                } else {
                    CmdError::new("invalid_params", e)
                }
            })
    })));
    core.fingerprint = core.fingerprint();
    let shared = Arc::new(Shared {
        server: Mutex::new(core),
        hub,
        busy: AtomicBool::new(false),
        cancel: Arc::new(AtomicBool::new(false)),
        token: token.clone(),
        port: addr.port(),
        web_dir: cfg.web_dir,
    });

    let (stop_tx, stop_rx) = oneshot::channel::<()>();
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .map_err(|e| HostError::Start(e.to_string()))?;
    let thread = std::thread::Builder::new()
        .name("bc-host".into())
        .spawn(move || {
            rt.block_on(async move {
                let listener = match tokio::net::TcpListener::from_std(listener) {
                    Ok(l) => l,
                    Err(e) => {
                        tracing::error!("host: {e}");
                        return;
                    }
                };
                let app = Router::new()
                    .route("/rpc", get(rpc_upgrade))
                    .fallback(get(web::serve))
                    .with_state(shared);
                tokio::select! {
                    r = axum::serve(listener, app) => {
                        if let Err(e) = r {
                            tracing::error!("host: {e}");
                        }
                    }
                    _ = stop_rx => {}
                }
            });
            rt.shutdown_timeout(Duration::from_secs(1));
        })
        .map_err(|e| HostError::Start(e.to_string()))?;
    tracing::info!(%addr, "host GUI nasłuchuje");
    Ok(HostHandle {
        addr,
        token,
        stop: Some(stop_tx),
        thread: Some(thread),
        token_file: cfg.token_file,
    })
}

fn new_token() -> Result<String, HostError> {
    let mut b = [0u8; 32];
    getrandom::fill(&mut b).map_err(|e| HostError::Start(format!("losowanie tokenu: {e}")))?;
    Ok(b.iter().map(|x| format!("{x:02x}")).collect())
}

/// Porównanie w czasie niezależnym od treści.
fn token_eq(a: &str, b: &str) -> bool {
    a.len() == b.len()
        && a.bytes()
            .zip(b.bytes())
            .fold(0u8, |acc, (x, y)| acc | (x ^ y))
            == 0
}

fn origin_allowed(origin: Option<&str>, port: u16) -> bool {
    let Some(o) = origin else {
        return true; // klienci natywni (CLI, testy)
    };
    let own = [
        format!("http://127.0.0.1:{port}"),
        format!("http://localhost:{port}"),
    ];
    own.iter().any(|x| x == o)
        || matches!(
            o,
            "tauri://localhost" | "http://tauri.localhost" | "https://tauri.localhost"
        )
}

async fn rpc_upgrade(
    ws: WebSocketUpgrade,
    Query(q): Query<HashMap<String, String>>,
    headers: HeaderMap,
    State(sh): State<Arc<Shared>>,
) -> Response {
    let ok = q.get("token").is_some_and(|t| token_eq(t, &sh.token));
    if !ok {
        return (StatusCode::UNAUTHORIZED, "brak lub zły token").into_response();
    }
    let origin = headers.get("origin").and_then(|v| v.to_str().ok());
    if !origin_allowed(origin, sh.port) {
        tracing::warn!(?origin, "odrzucono połączenie z obcego origin");
        return (StatusCode::FORBIDDEN, "niedozwolony origin").into_response();
    }
    ws.on_upgrade(move |s| connection(s, sh))
}

async fn connection(mut socket: WebSocket, sh: Arc<Shared>) {
    let id = sh.hub.next.fetch_add(1, Ordering::Relaxed);
    let (tx, mut rx) = mpsc::unbounded_channel::<String>();
    sh.hub.clients.lock().unwrap().insert(
        id,
        Client {
            tx: tx.clone(),
            window: false,
        },
    );
    // Żądania klienta wykonują się po kolei (zaznacz → przesuń musi zachować kolejność),
    // w wątku blokującym; pętla poniżej czyta dalej, więc odpowiedź okna na
    // `ui.screenshot` dociera nawet wtedy, gdy kolejka czeka na zamek serwera.
    let (req_tx, mut req_rx) = mpsc::unbounded_channel::<Value>();
    let worker = {
        let sh = sh.clone();
        let tx = tx.clone();
        tokio::spawn(async move {
            while let Some(v) = req_rx.recv().await {
                let sh = sh.clone();
                let r = tokio::task::spawn_blocking(move || handle(&sh, id, v)).await;
                if let Ok(Some(r)) = r {
                    let _ = tx.send(r.to_string());
                }
            }
        })
    };
    loop {
        tokio::select! {
            out = rx.recv() => {
                let Some(text) = out else { break };
                if socket.send(Message::Text(text.into())).await.is_err() {
                    break;
                }
            }
            msg = socket.recv() => {
                let text = match msg {
                    Some(Ok(Message::Text(t))) => t.to_string(),
                    Some(Ok(Message::Close(_))) | None | Some(Err(_)) => break,
                    Some(Ok(_)) => continue,
                };
                let v: Value = match serde_json::from_str(&text) {
                    Ok(v) => v,
                    Err(e) => {
                        let _ = tx.send(rpc_err(Value::Null, -32700, &format!("błąd parsowania: {e}"), None).to_string());
                        continue;
                    }
                };
                if v.get("method").is_none() {
                    // odpowiedź okna na żądanie serwera
                    if let Some(rid) = v.get("id").and_then(Value::as_str) {
                        if let Some(p) = sh.hub.pending.lock().unwrap().remove(rid) {
                            let _ = p.send(v);
                        }
                    }
                    continue;
                }
                // anulowanie omija kolejkę: kolejka czeka właśnie na anulowane polecenie (REQ-16-006)
                if v.get("method").and_then(Value::as_str) == Some("gui.cancel") {
                    let busy = sh.busy.load(Ordering::SeqCst);
                    if busy {
                        sh.cancel.store(true, Ordering::SeqCst);
                    }
                    if let Some(rid) = v.get("id") {
                        let _ = tx.send(json!({"jsonrpc": "2.0", "id": rid, "result": {"cancelled": busy}}).to_string());
                    }
                    continue;
                }
                let _ = req_tx.send(v);
            }
        }
    }
    sh.hub.clients.lock().unwrap().remove(&id);
    drop(req_tx);
    worker.abort();
}

fn rpc_err(id: Value, code: i64, message: &str, data: Option<Value>) -> Value {
    let mut e = json!({"code": code, "message": message});
    if let Some(d) = data {
        e["data"] = d;
    }
    json!({"jsonrpc": "2.0", "id": id, "error": e})
}

fn cmd_err(id: Value, e: CmdError, step: Option<usize>) -> Value {
    let mut data = json!({"code": e.code});
    if let Some(i) = step {
        data["step"] = json!(i);
    }
    rpc_err(id, -32000, &e.message, Some(data))
}

/// Obsługuje jedno żądanie klienta (wątek blokujący).
fn handle(sh: &Shared, client: u64, msg: Value) -> Option<Value> {
    let id = msg.get("id").cloned();
    let method = msg.get("method").and_then(Value::as_str).unwrap_or("");
    let params = msg.get("params").cloned().unwrap_or(Value::Null);
    let reply = |r: Result<Value, Value>| -> Option<Value> {
        let id = id.clone()?;
        Some(match r {
            Ok(v) => json!({"jsonrpc": "2.0", "id": id, "result": v}),
            Err(mut e) => {
                e["id"] = id;
                e
            }
        })
    };
    let rid = id.clone().unwrap_or(Value::Null);
    match method {
        "hello" => {
            if params.get("role").and_then(Value::as_str) == Some("window") {
                if let Some(c) = sh.hub.clients.lock().unwrap().get_mut(&client) {
                    c.window = true;
                }
            }
            let rev = sh.server.lock().unwrap().revision;
            reply(Ok(
                json!({"server": bc_mcp::SERVER_NAME, "version": bc_mcp::SERVER_VERSION,
                "revision": rev, "client": client}),
            ))
        }
        "gui.scene" => {
            let core = sh.server.lock().unwrap();
            let tol = params
                .get("tolerance")
                .and_then(Value::as_f64)
                .filter(|t| *t > 0.0)
                .unwrap_or(0.02);
            let have = params.get("have").and_then(Value::as_object);
            let have_tree = params.get("have_tree").and_then(Value::as_str);
            reply(Ok(scene::build(
                core.mcp.workbench(),
                core.revision,
                tol,
                have,
                have_tree,
            )))
        }
        "gui.status" => {
            let clients = sh.hub.clients.lock().unwrap();
            let windows = clients.values().filter(|c| c.window).count();
            reply(Ok(
                json!({"clients": clients.len(), "windows": windows, "port": sh.port}),
            ))
        }
        "gui.commands" => {
            let core = sh.server.lock().unwrap();
            let list: Vec<Value> = core
                .reg
                .list(params.get("filter").and_then(Value::as_str))
                .into_iter()
                .map(|c| {
                    json!({"id": c.id, "executor": c.executor, "title": c.title, "description": c.description,
                    "shortcut": c.shortcut, "safety_tier": c.safety_tier, "undoable": c.undoable,
                    "params_schema": c.params_schema})
                })
                .collect();
            reply(Ok(json!({"commands": list})))
        }
        "gui.execute" | "gui.batch" | "mcp" => {
            let mut core = sh.server.lock().unwrap();
            let core = &mut *core;
            // postęp i anulowanie długich poleceń (REQ-16-006)
            let job = ProgressJob::start(
                match method {
                    "gui.execute" => params
                        .get("id")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string(),
                    "gui.batch" => "batch".to_string(),
                    _ => "mcp".to_string(),
                },
                sh.hub
                    .clients
                    .lock()
                    .unwrap()
                    .get(&client)
                    .map(|c| c.tx.clone()),
            );
            sh.cancel.store(false, Ordering::SeqCst);
            sh.busy.store(true, Ordering::SeqCst);
            let j = job.clone();
            core.mcp.workbench_mut().progress = Some(bc_commands::Progress {
                report: Arc::new(move |f| j.report(f)),
                cancel: sh.cancel.clone(),
            });
            let r = match method {
                "gui.execute" => match params.get("id").and_then(Value::as_str) {
                    None => Err(rpc_err(rid.clone(), -32602, "brak id", None)),
                    Some(cid) => {
                        let p = params.get("params").cloned().unwrap_or(json!({}));
                        core.mcp
                            .workbench_mut()
                            .execute(&core.reg, cid, p)
                            .map_err(|e| cmd_err(rid.clone(), e, None))
                    }
                },
                "gui.batch" => {
                    match serde_json::from_value::<Vec<BatchStep>>(
                        params.get("steps").cloned().unwrap_or(Value::Null),
                    ) {
                        Err(e) => Err(rpc_err(rid.clone(), -32602, &format!("steps: {e}"), None)),
                        Ok(steps) if steps.is_empty() => Err(rpc_err(
                            rid.clone(),
                            -32602,
                            "steps: co najmniej jeden krok",
                            None,
                        )),
                        Ok(steps) => core
                            .mcp
                            .workbench_mut()
                            .batch(&core.reg, &steps)
                            .map(|r| json!({"results": r}))
                            .map_err(|(i, e)| cmd_err(rid.clone(), e, Some(i))),
                    }
                }
                _ => Ok(core.mcp.handle(params).unwrap_or(Value::Null)),
            };
            core.mcp.workbench_mut().progress = None;
            sh.busy.store(false, Ordering::SeqCst);
            job.finish();
            if method != "mcp" {
                let b = core.mcp.workbench_mut();
                if b.autosave.is_some() {
                    if let Err(e) = b.autosave_tick(std::time::SystemTime::now()) {
                        tracing::warn!("autozapis nie powiódł się: {e}");
                    }
                }
            }
            if let Some(n) = core.changed() {
                sh.hub.broadcast(&n);
            }
            reply(r)
        }
        other => reply(Err(rpc_err(
            rid,
            -32601,
            &format!("nieznana metoda {other}"),
            None,
        ))),
    }
}
