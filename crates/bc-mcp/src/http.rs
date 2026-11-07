//! MCP przez Streamable HTTP na 127.0.0.1 (REQ-15-003, ADR 0012).
//!
//! Jeden punkt końcowy `/mcp`:
//! - `POST` — jedna wiadomość JSON-RPC. Powiadomienia i odpowiedzi klienta → `202`.
//!   `initialize` → odpowiedź JSON z nagłówkiem `Mcp-Session-Id`. `tools/call` → strumień
//!   SSE (powiadomienia o postępie, żądania `elicitation/create`, na końcu odpowiedź),
//!   pozostałe żądania → `application/json`.
//! - `GET` (`Accept: text/event-stream`) — strumień SSE powiadomień serwera
//!   (`notifications/resources/updated`).
//! - `DELETE` — koniec sesji (utrata klienta: jog anulowany, zadanie trwa).
//!
//! Wszystkie sesje działają na jednym [`Server`] (wspólny dokument i urządzenia), każda ma
//! własne subskrypcje i watchdog joga. Ochrona przed DNS rebinding: nasłuch tylko na
//! 127.0.0.1, sprawdzanie nagłówków `Host` i `Origin`. `stop_job` i odpowiedzi klienta
//! omijają zamek serwera (REQ-15-106).

use std::collections::HashMap;
use std::convert::Infallible;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc as std_mpsc;
use std::sync::{Arc, Mutex};
use std::task::{Context as TaskCx, Poll};
use std::time::{Duration, Instant};

use axum::body::Bytes;
use axum::extract::State;
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::Router;
use serde_json::{json, Value};
use tokio::sync::{mpsc, oneshot};

use crate::stdio::{reply_result, TICK};
use crate::{err, id_key, is_cancelled, Channel, Ctx, Emergency, PeerId, Server, CONFIRM_TIMEOUT};

/// Domyślny port MCP HTTP (host GUI: 7984).
pub const DEFAULT_PORT: u16 = 7985;

/// Sesja bez żadnej aktywności dłużej niż ten czas jest zamykana (utrata klienta).
const SESSION_IDLE: Duration = Duration::from_secs(30 * 60);

/// Działający serwer HTTP (w osobnym wątku z własnym środowiskiem tokio).
pub struct HttpHandle {
    pub addr: SocketAddr,
    stop: Option<oneshot::Sender<()>>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl HttpHandle {
    /// Adres punktu końcowego MCP, np. `http://127.0.0.1:7985/mcp`.
    pub fn url(&self) -> String {
        format!("http://{}/mcp", self.addr)
    }

    /// Zatrzymuje serwer i czeka na zamknięcie.
    pub fn shutdown(mut self) {
        if let Some(s) = self.stop.take() {
            let _ = s.send(());
        }
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }

    /// Czeka, aż serwer zakończy pracę.
    pub fn wait(mut self) {
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

struct Sess {
    peer: PeerId,
    client: String,
    /// Parametry `initialize` do zarejestrowania w serwerze przy pierwszym żądaniu.
    init: Option<Value>,
    /// Strumień GET (powiadomienia serwera).
    stream: Option<mpsc::UnboundedSender<String>>,
    last_seen: Instant,
}

struct St {
    server: Mutex<Server>,
    sessions: Mutex<HashMap<String, Sess>>,
    /// Żądania w toku: (klient, id) → flaga anulowania.
    inflight: Mutex<HashMap<(PeerId, String), Arc<AtomicBool>>>,
    /// Żądania serwera do klientów (elicitation) czekające na odpowiedź.
    pending: Mutex<HashMap<String, std_mpsc::Sender<Value>>>,
    next_peer: AtomicU64,
    next_req: AtomicU64,
    emergency: Emergency,
    port: u16,
}

/// Uruchamia serwer MCP HTTP na `127.0.0.1:port` (0 = dowolny wolny port).
pub fn serve(server: Server, port: u16) -> Result<HttpHandle, String> {
    let listener = std::net::TcpListener::bind(("127.0.0.1", port))
        .map_err(|e| format!("127.0.0.1:{port}: {e}"))?;
    listener.set_nonblocking(true).map_err(|e| e.to_string())?;
    let addr = listener.local_addr().map_err(|e| e.to_string())?;
    let emergency = server.emergency();
    let st = Arc::new(St {
        server: Mutex::new(server),
        sessions: Mutex::new(HashMap::new()),
        inflight: Mutex::new(HashMap::new()),
        pending: Mutex::new(HashMap::new()),
        next_peer: AtomicU64::new(1),
        next_req: AtomicU64::new(1),
        emergency,
        port: addr.port(),
    });
    let (stop_tx, stop_rx) = oneshot::channel::<()>();
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .map_err(|e| e.to_string())?;
    let thread = std::thread::Builder::new()
        .name("mcp-http".into())
        .spawn(move || {
            rt.block_on(async move {
                let listener = match tokio::net::TcpListener::from_std(listener) {
                    Ok(l) => l,
                    Err(e) => {
                        tracing::error!("MCP HTTP: {e}");
                        return;
                    }
                };
                let ticker = {
                    let st = st.clone();
                    tokio::spawn(async move {
                        let mut iv = tokio::time::interval(TICK);
                        loop {
                            iv.tick().await;
                            let st = st.clone();
                            let _ = tokio::task::spawn_blocking(move || tick(&st)).await;
                        }
                    })
                };
                let app = Router::new()
                    .route("/mcp", post(on_post).get(on_get).delete(on_delete))
                    .with_state(st.clone());
                tokio::select! {
                    r = axum::serve(listener, app) => {
                        if let Err(e) = r {
                            tracing::error!("MCP HTTP: {e}");
                        }
                    }
                    _ = stop_rx => {}
                }
                ticker.abort();
                // koniec serwera = utrata wszystkich klientów (jog anulowany, zadania trwają)
                let st2 = st.clone();
                let _ = tokio::task::spawn_blocking(move || {
                    let peers: Vec<PeerId> = st2
                        .sessions
                        .lock()
                        .expect("sesje")
                        .drain()
                        .map(|(_, s)| s.peer)
                        .collect();
                    let mut srv = st2.server.lock().expect("serwer");
                    for p in peers {
                        srv.client_lost(p);
                    }
                })
                .await;
            });
            rt.shutdown_timeout(Duration::from_secs(1));
        })
        .map_err(|e| e.to_string())?;
    tracing::info!(%addr, "MCP Streamable HTTP nasłuchuje");
    Ok(HttpHandle {
        addr,
        stop: Some(stop_tx),
        thread: Some(thread),
    })
}

/// Okresowo: watchdog joga, powiadomienia o zmianach zasobów, wygaszanie nieaktywnych sesji.
fn tick(st: &St) {
    let now = Instant::now();
    let expired: Vec<(String, PeerId)> = st
        .sessions
        .lock()
        .expect("sesje")
        .iter()
        .filter(|(_, s)| now.saturating_duration_since(s.last_seen) > SESSION_IDLE)
        .map(|(k, s)| (k.clone(), s.peer))
        .collect();
    // zajęty serwer (długie polecenie, oczekiwanie na potwierdzenie) — następnym razem
    let Ok(mut srv) = st.server.try_lock() else {
        return;
    };
    for (k, peer) in expired {
        st.sessions.lock().expect("sesje").remove(&k);
        tracing::info!(peer, "sesja MCP wygasła");
        srv.client_lost(peer);
    }
    let streams: Vec<(PeerId, Instant, Option<mpsc::UnboundedSender<String>>)> = st
        .sessions
        .lock()
        .expect("sesje")
        .values()
        .map(|s| (s.peer, s.last_seen, s.stream.clone()))
        .collect();
    for (peer, seen, _) in &streams {
        srv.touch(*peer, *seen);
    }
    srv.watchdog_tick(now);
    for (peer, _, stream) in streams {
        let Some(tx) = stream else { continue };
        if tx.is_closed() {
            continue;
        }
        for n in srv.poll_updates(peer) {
            let _ = tx.send(n.to_string());
        }
    }
}

struct HttpChannel {
    tx: mpsc::UnboundedSender<String>,
    st: Arc<St>,
}

impl Channel for HttpChannel {
    fn notify(&self, msg: Value) {
        let _ = self.tx.send(msg.to_string());
    }

    fn request(&self, method: &str, params: Value, cancel: &AtomicBool) -> Result<Value, String> {
        let id = json!(format!(
            "bc-h{}",
            self.st.next_req.fetch_add(1, Ordering::Relaxed)
        ));
        let (tx, rx) = std_mpsc::channel();
        self.st
            .pending
            .lock()
            .expect("oczekujące")
            .insert(id_key(&id), tx);
        let res = (|| {
            self.tx
                .send(
                    json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params})
                        .to_string(),
                )
                .map_err(|_| "klient rozłączony".to_string())?;
            let end = Instant::now() + CONFIRM_TIMEOUT;
            loop {
                if is_cancelled(cancel) {
                    return Err("anulowano".to_string());
                }
                if self.tx.is_closed() {
                    return Err("klient rozłączony".to_string());
                }
                match rx.recv_timeout(TICK) {
                    Ok(v) => return reply_result(v),
                    Err(std_mpsc::RecvTimeoutError::Timeout) if Instant::now() < end => continue,
                    Err(_) => return Err("brak odpowiedzi".to_string()),
                }
            }
        })();
        self.st
            .pending
            .lock()
            .expect("oczekujące")
            .remove(&id_key(&id));
        res
    }
}

/// Strumień SSE z kanału tokio.
struct RxStream(mpsc::UnboundedReceiver<String>);

impl futures_core::Stream for RxStream {
    type Item = Result<Event, Infallible>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut TaskCx<'_>) -> Poll<Option<Self::Item>> {
        self.0
            .poll_recv(cx)
            .map(|o| o.map(|s| Ok(Event::default().data(s))))
    }
}

fn sse(rx: mpsc::UnboundedReceiver<String>) -> Response {
    Sse::new(RxStream(rx))
        .keep_alive(KeepAlive::new().interval(Duration::from_secs(15)))
        .into_response()
}

fn json_resp(status: StatusCode, v: &Value) -> Response {
    (
        status,
        [(axum::http::header::CONTENT_TYPE, "application/json")],
        v.to_string(),
    )
        .into_response()
}

fn plain(status: StatusCode, msg: &str) -> Response {
    (status, msg.to_string()).into_response()
}

/// Host i Origin muszą wskazywać na pętlę zwrotną (ochrona przed DNS rebinding).
// Err = gotowa odpowiedź HTTP (rozmiar bez znaczenia, raz na żądanie)
#[allow(clippy::result_large_err)]
fn check_local(h: &HeaderMap, port: u16) -> Result<(), Response> {
    let host = h
        .get(axum::http::header::HOST)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    let host_ok = ["127.0.0.1", "localhost", "[::1]"]
        .iter()
        .any(|n| host == *n || host == format!("{n}:{port}"));
    if !host_ok {
        tracing::warn!(host, "MCP HTTP: odrzucono obcy Host");
        return Err(plain(StatusCode::FORBIDDEN, "niedozwolony Host"));
    }
    if let Some(o) = h.get(axum::http::header::ORIGIN) {
        let o = o.to_str().unwrap_or("");
        let rest = o
            .strip_prefix("http://")
            .or_else(|| o.strip_prefix("https://"))
            .unwrap_or("");
        let name = if rest.starts_with('[') {
            rest.split(']')
                .next()
                .map(|s| format!("{s}]"))
                .unwrap_or_default()
        } else {
            rest.split(':').next().unwrap_or("").to_string()
        };
        if !matches!(name.as_str(), "127.0.0.1" | "localhost" | "[::1]") {
            tracing::warn!(origin = o, "MCP HTTP: odrzucono obcy Origin");
            return Err(plain(StatusCode::FORBIDDEN, "niedozwolony Origin"));
        }
    }
    Ok(())
}

fn header<'a>(h: &'a HeaderMap, k: &str) -> Option<&'a str> {
    h.get(k).and_then(|v| v.to_str().ok())
}

/// Sesja z nagłówka `Mcp-Session-Id`.
struct SessRef {
    sid: String,
    peer: PeerId,
    client: String,
    init: Option<Value>,
}

impl SessRef {
    /// Kontekst żądania; przy pierwszym użyciu rejestruje klienta w serwerze.
    fn prepare(&self, srv: &mut Server) {
        if let Some(p) = &self.init {
            srv.ensure_peer(self.peer, p);
        }
    }
}

#[allow(clippy::result_large_err)]
fn session(st: &St, h: &HeaderMap) -> Result<SessRef, Response> {
    let Some(sid) = header(h, "mcp-session-id") else {
        return Err(json_resp(
            StatusCode::BAD_REQUEST,
            &err(
                Value::Null,
                -32600,
                "brak nagłówka Mcp-Session-Id (najpierw initialize)",
            ),
        ));
    };
    if let Some(v) = header(h, "mcp-protocol-version") {
        if !crate::PROTOCOL_VERSIONS.contains(&v) {
            return Err(json_resp(
                StatusCode::BAD_REQUEST,
                &err(
                    Value::Null,
                    -32600,
                    &format!("nieobsługiwana wersja protokołu {v}"),
                ),
            ));
        }
    }
    let mut ss = st.sessions.lock().expect("sesje");
    match ss.get_mut(sid) {
        Some(s) => {
            s.last_seen = Instant::now();
            Ok(SessRef {
                sid: sid.to_string(),
                peer: s.peer,
                client: s.client.clone(),
                init: s.init.clone(),
            })
        }
        None => Err(json_resp(
            StatusCode::NOT_FOUND,
            &err(Value::Null, -32001, "nieznana lub zakończona sesja"),
        )),
    }
}

fn new_session_id() -> String {
    let mut b = [0u8; 24];
    if getrandom::fill(&mut b).is_err() {
        // awaryjnie: czas + licznik (sesja i tak działa tylko na 127.0.0.1)
        let t = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        b[..16].copy_from_slice(&t.to_le_bytes());
    }
    b.iter().map(|x| format!("{x:02x}")).collect()
}

async fn blocking<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> T {
    tokio::task::spawn_blocking(f)
        .await
        .expect("zadanie blokujące")
}

async fn on_post(State(st): State<Arc<St>>, h: HeaderMap, body: Bytes) -> Response {
    if let Err(r) = check_local(&h, st.port) {
        return r;
    }
    if let Some(a) = header(&h, "accept") {
        if !(a.contains("application/json") || a.contains("text/event-stream") || a.contains("*/*"))
        {
            return plain(
                StatusCode::NOT_ACCEPTABLE,
                "Accept: application/json, text/event-stream",
            );
        }
    }
    let msg: Value = match serde_json::from_slice(&body) {
        Ok(v) => v,
        Err(e) => {
            return json_resp(
                StatusCode::BAD_REQUEST,
                &err(Value::Null, -32700, &format!("błąd parsowania: {e}")),
            )
        }
    };
    if !msg.is_object() {
        return json_resp(
            StatusCode::BAD_REQUEST,
            &err(
                Value::Null,
                -32600,
                "oczekiwano jednego obiektu JSON-RPC (bez wsadów)",
            ),
        );
    }
    let method = msg.get("method").and_then(Value::as_str).map(String::from);
    let id = msg.get("id").cloned();

    if method.as_deref() == Some("initialize") && id.is_some() {
        // bez zamka serwera: nowy klient może się połączyć, gdy serwer jest zajęty
        let peer = st.next_peer.fetch_add(1, Ordering::Relaxed);
        let params = msg.get("params").cloned().unwrap_or(Value::Null);
        let reply =
            json!({"jsonrpc": "2.0", "id": id, "result": Server::initialize_reply(&params)});
        let sid = new_session_id();
        st.sessions.lock().expect("sesje").insert(
            sid.clone(),
            Sess {
                peer,
                client: crate::client_label(&params).unwrap_or_else(|| "unknown".into()),
                init: Some(params),
                stream: None,
                last_seen: Instant::now(),
            },
        );
        let mut r = json_resp(StatusCode::OK, &reply);
        if let Ok(v) = HeaderValue::from_str(&sid) {
            r.headers_mut().insert("mcp-session-id", v);
        }
        return r;
    }

    let sref = match session(&st, &h) {
        Ok(s) => s,
        Err(r) => return r,
    };
    let peer = sref.peer;

    let Some(method) = method else {
        // odpowiedź klienta na żądanie serwera (elicitation)
        if let Some(id) = &id {
            if let Some(p) = st.pending.lock().expect("oczekujące").remove(&id_key(id)) {
                let _ = p.send(msg);
            }
        }
        return StatusCode::ACCEPTED.into_response();
    };
    let Some(id) = id else {
        // powiadomienie klienta
        if method == "notifications/cancelled" {
            if let Some(rid) = msg.pointer("/params/requestId") {
                if let Some(f) = st
                    .inflight
                    .lock()
                    .expect("w toku")
                    .get(&(peer, id_key(rid)))
                {
                    f.store(true, Ordering::SeqCst);
                }
            }
        }
        return StatusCode::ACCEPTED.into_response();
    };

    if method == "tools/call" {
        if msg.pointer("/params/name").and_then(Value::as_str) == Some("stop_job") {
            // stop poza kolejką i bez zamka serwera (REQ-15-072, 106)
            let em = st.emergency.clone();
            let params = msg.get("params").cloned().unwrap_or(Value::Null);
            let client = sref.client.clone();
            let r = blocking(move || em.stop_call(&client, &params)).await;
            return json_resp(
                StatusCode::OK,
                &json!({"jsonrpc": "2.0", "id": id, "result": r}),
            );
        }
        // strumień SSE: postęp, potwierdzenia, odpowiedź
        let (tx, rx) = mpsc::unbounded_channel::<String>();
        let cancel = Arc::new(AtomicBool::new(false));
        let key = (peer, id_key(&id));
        st.inflight
            .lock()
            .expect("w toku")
            .insert(key.clone(), cancel.clone());
        let st2 = st.clone();
        tokio::task::spawn_blocking(move || {
            let ctx = Ctx {
                peer,
                channel: Some(Arc::new(HttpChannel {
                    tx: tx.clone(),
                    st: st2.clone(),
                })),
                cancel: cancel.clone(),
            };
            let reply = {
                let mut srv = st2.server.lock().expect("serwer");
                sref.prepare(&mut srv);
                srv.handle_ctx(msg, &ctx)
            };
            st2.inflight.lock().expect("w toku").remove(&key);
            if let Some(r) = reply.filter(|_| !is_cancelled(&cancel)) {
                let _ = tx.send(r.to_string());
            }
        });
        return sse(rx);
    }

    let st2 = st.clone();
    let reply = blocking(move || {
        let mut srv = st2.server.lock().expect("serwer");
        sref.prepare(&mut srv);
        srv.handle_ctx(
            msg,
            &Ctx {
                peer,
                ..Default::default()
            },
        )
    })
    .await;
    match reply {
        Some(r) => json_resp(StatusCode::OK, &r),
        None => StatusCode::ACCEPTED.into_response(),
    }
}

async fn on_get(State(st): State<Arc<St>>, h: HeaderMap) -> Response {
    if let Err(r) = check_local(&h, st.port) {
        return r;
    }
    if !header(&h, "accept").is_some_and(|a| a.contains("text/event-stream")) {
        return plain(
            StatusCode::METHOD_NOT_ALLOWED,
            "GET wymaga Accept: text/event-stream",
        );
    }
    let sref = match session(&st, &h) {
        Ok(s) => s,
        Err(r) => return r,
    };
    let (tx, rx) = mpsc::unbounded_channel::<String>();
    if let Some(s) = st.sessions.lock().expect("sesje").get_mut(&sref.sid) {
        s.stream = Some(tx);
    }
    sse(rx)
}

async fn on_delete(State(st): State<Arc<St>>, h: HeaderMap) -> Response {
    if let Err(r) = check_local(&h, st.port) {
        return r;
    }
    let sref = match session(&st, &h) {
        Ok(s) => s,
        Err(r) => return r,
    };
    let peer = sref.peer;
    st.sessions.lock().expect("sesje").remove(&sref.sid);
    let st2 = st.clone();
    blocking(move || st2.server.lock().expect("serwer").client_lost(peer)).await;
    tracing::info!(peer, "sesja MCP zakończona przez klienta");
    StatusCode::OK.into_response()
}
