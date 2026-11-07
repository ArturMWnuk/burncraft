//! bc-mcp — serwer Model Context Protocol BurnCraft.
//!
//! Transporty: stdio (JSON-RPC 2.0, jedna wiadomość na linię, [`Server::run_io`]) i
//! Streamable HTTP na 127.0.0.1 ([`http`], ADR 0012). Narzędzia są cienką warstwą nad
//! rejestrem poleceń ([`bc_commands`]); każde wywołanie przechodzi przez politykę
//! bezpieczeństwa ([`policy`]) i trafia do dziennika audytu ([`audit`]). Zasoby mają
//! subskrypcje, są prompty, powiadomienia o postępie i anulowanie żądań. `stop_job`
//! omija kolejkę żądań ([`Emergency`]), a watchdog zatrzymuje jog milczącego klienta.

pub mod audit;
mod devices;
pub mod http;
pub mod policy;
mod prompts;
mod resources;
mod shortcuts;
mod stdio;
mod tools;

use std::collections::{BTreeMap, HashMap};
use std::hash::{Hash, Hasher};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use bc_commands::workbench::Autosave;
use bc_commands::{prefs::PrefStore, Registry, Session, Workbench};
use serde_json::{json, Value};

pub const SERVER_NAME: &str = "burncraft";
pub const SERVER_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Obsługiwane wersje protokołu MCP, od najnowszej.
pub const PROTOCOL_VERSIONS: &[&str] = &["2025-11-25", "2025-06-18", "2025-03-26", "2024-11-05"];

/// Domyślny limit ciszy klienta, po którym watchdog anuluje jog (REQ-15-108).
pub const DEFAULT_WATCHDOG: Duration = Duration::from_secs(5);

/// Najdłuższe oczekiwanie na odpowiedź użytkownika (elicitation).
pub const CONFIRM_TIMEOUT: Duration = Duration::from_secs(300);

pub const INSTRUCTIONS: &str = "BurnCraft — projektowanie i przygotowanie zadań dla laserów (CAD/CAM 2D).\n\
Jednostki: milimetry. Układ współrzędnych: oś Y w górę, (0,0) = lewy dolny róg obszaru roboczego maszyny (ADR 0003). \
Kąty w stopniach, dodatnie przeciwnie do ruchu wskazówek zegara.\n\
Każda akcja to polecenie z rejestru: list_commands (filtr) -> describe_command (schemat parametrów) -> execute {id, params} \
albo batch {steps} (jeden krok historii, atomowo). Polecenia działają na jawnych `ids` albo na bieżącym zaznaczeniu; \
nowo utworzony obiekt zostaje zaznaczony. undo/redo cofają kroki.\n\
Typowy przepływ: inspect_document -> execute shape.rect / shape.ellipse / shape.path (warstwa = indeks 0-29) -> \
object.transform -> layer.set / layer.params (parametry cięcia) -> render_preview (sprawdź wynik) -> \
plan_job (czas, ostrzeżenia) i render_preview mode=toolpath (ścieżki) -> execute file.save {path: \"x.bcraft\"}. \
Prompt prepare_cut_file prowadzi przez kontrolę przed cięciem.\n\
Maszyna: list_devices -> connect {name} -> device_status (także zasób burncraft://device/{id}/status z subskrypcją) -> \
home / jog / move_to / set_origin / frame -> pause_job / resume_job / stop_job; postęp zadania: burncraft://job/{id}.\n\
Bezpieczeństwo: poziomy read < design < motion < config < fire. read i design działają bez pytań; motion wymaga potwierdzenia \
użytkownika (elicitation albo zgoda przy uruchomieniu serwera), config i fire — jawnego potwierdzenia w GUI BurnCraft. \
stop_job jest zawsze dozwolony. Jog jest anulowany, gdy klient milczy dłużej niż kilka sekund (watchdog) — przy długim jogu \
wywołuj device_status albo ping. Wszystkie wywołania trafiają do dziennika audytu.";

/// Zrzut okna GUI: zwraca PNG w base64 albo kod błędu (np. `no_window`).
/// Dostarcza go host GUI (`bc-host`); w trybie bezgłowym brak haka.
#[derive(Clone)]
pub struct ScreenshotHook(pub std::sync::Arc<dyn Fn() -> Result<String, String> + Send + Sync>);

impl std::fmt::Debug for ScreenshotHook {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ScreenshotHook")
    }
}

/// Konfiguracja serwera.
#[derive(Debug, Clone, Default)]
pub struct Config {
    pub audit_log: Option<std::path::PathBuf>,
    pub policy: policy::Policy,
    /// Autozapis dokumentów (sprawdzany po każdym wywołaniu narzędzia).
    pub autosave: Option<Autosave>,
    /// Katalog preferencji (prefs.json + kopie zapasowe).
    pub prefs_dir: Option<std::path::PathBuf>,
    /// Zrzut okna dla narzędzia `screenshot` (REQ-15-026).
    pub screenshot: Option<ScreenshotHook>,
    /// Limit ciszy klienta dla watchdoga joga (brak = [`DEFAULT_WATCHDOG`]).
    pub watchdog: Option<Duration>,
}

/// Identyfikator klienta (sesji MCP) w serwerze: stdio = 0, HTTP — kolejne sesje.
pub type PeerId = u64;

/// Kanał do klienta bieżącego żądania: powiadomienia i żądania serwera.
pub trait Channel: Send + Sync {
    fn notify(&self, msg: Value);
    /// Żądanie serwera do klienta (np. `elicitation/create`); czeka na odpowiedź klienta,
    /// aż do [`CONFIRM_TIMEOUT`] albo ustawienia `cancel`.
    fn request(&self, method: &str, params: Value, cancel: &AtomicBool) -> Result<Value, String>;
}

/// Kontekst jednego żądania nadany przez transport.
#[derive(Clone, Default)]
pub struct Ctx {
    pub peer: PeerId,
    pub channel: Option<Arc<dyn Channel>>,
    /// Ustawiona przez `notifications/cancelled` — przerywa polecenie i wstrzymuje odpowiedź.
    pub cancel: Arc<AtomicBool>,
}

/// Stan jednego klienta MCP.
pub(crate) struct Peer {
    pub(crate) client: String,
    /// Klient obsługuje elicitation (potwierdzenia motion).
    pub(crate) elicitation: bool,
    /// Subskrybowane zasoby: uri → skrót ostatniej treści.
    pub(crate) subs: BTreeMap<String, u64>,
    /// Urządzenia, którym ten klient zlecił jog (z czasem zlecenia).
    pub(crate) jogging: BTreeMap<String, Instant>,
    pub(crate) last_seen: Instant,
    /// Zarejestrowany z parametrów `initialize`.
    pub(crate) known: bool,
}

impl Peer {
    fn new() -> Self {
        Peer {
            client: "unknown".into(),
            elicitation: false,
            subs: BTreeMap::new(),
            jogging: BTreeMap::new(),
            last_seen: Instant::now(),
            known: false,
        }
    }
}

fn registry() -> &'static Registry {
    static R: OnceLock<Registry> = OnceLock::new();
    R.get_or_init(Registry::builtin)
}

/// Ścieżka awaryjna poza kolejką żądań: `stop_job` i anulowanie joga działają także wtedy,
/// gdy serwer jest zajęty (długie polecenie, oczekiwanie na potwierdzenie) — REQ-15-106.
/// Wykonuje te same polecenia rejestru (`job.stop`, `device.jog_cancel`) na urządzeniach
/// współdzielonych z sesją.
#[derive(Clone)]
pub struct Emergency {
    machines: bc_drivers::MachinesHandle,
    selected: Arc<Mutex<Option<String>>>,
    audit: Arc<Mutex<audit::Audit>>,
}

impl Emergency {
    fn session(&self, device: Option<String>) -> Session {
        Session {
            machines: self.machines.clone(),
            device: device.or_else(|| self.selected.lock().expect("wybór").clone()),
            ..Default::default()
        }
    }

    /// `tools/call stop_job` poza kolejką: wynik narzędzia (z wpisem w dzienniku audytu).
    pub fn stop_call(&self, client: &str, params: &Value) -> Value {
        let args = params
            .get("arguments")
            .cloned()
            .filter(Value::is_object)
            .unwrap_or_else(|| json!({}));
        let device = args.get("device").and_then(Value::as_str).map(String::from);
        let mut s = self.session(device);
        let outcome = match s.execute(registry(), "job.stop", args.clone()) {
            Ok(r) => tools::Outcome::ok(r),
            Err(e) => tools::Outcome::cmd_error(e, None),
        };
        self.audit
            .lock()
            .expect("audyt")
            .record(client, "stop_job", &args, &outcome);
        outcome.into_result()
    }

    /// Polecenie rejestru na wskazanym urządzeniu (zasoby urządzeń i zadań).
    pub(crate) fn exec(
        &self,
        device: &str,
        id: &str,
        p: Value,
    ) -> Result<Value, bc_commands::CmdError> {
        let mut s = self.session(Some(device.to_string()));
        s.execute(registry(), id, p)
    }

    /// Anuluje jog urządzenia (watchdog).
    pub fn jog_cancel(&self, device: &str) {
        let mut s = self.session(Some(device.to_string()));
        if let Err(e) = s.execute(registry(), "device.jog_cancel", json!({"device": device})) {
            tracing::warn!(device, "watchdog: anulowanie joga nie powiodło się: {e}");
        }
    }

    /// Czy urządzenie jest w trakcie joga.
    pub fn is_jogging(&self, device: &str) -> bool {
        let mut m = self.machines.lock().expect("urządzenia");
        m.driver(device)
            .is_some_and(|d| d.status().state == bc_drivers::driver::RunState::Jog)
    }

    /// Czy na którymś urządzeniu trwa zadanie (także wstrzymane).
    pub fn jobs_active(&self) -> bool {
        use bc_drivers::driver::JobStatus;
        let mut m = self.machines.lock().expect("urządzenia");
        let names = m.connected();
        names.iter().any(|n| {
            m.driver(n).is_some_and(|d| {
                let j = d.job();
                matches!(j.status, JobStatus::Running | JobStatus::Paused) || j.queued > 0
            })
        })
    }
}

pub struct Server {
    reg: Registry,
    bench: Workbench,
    policy: policy::Policy,
    audit: Arc<Mutex<audit::Audit>>,
    screenshot: Option<ScreenshotHook>,
    peers: HashMap<PeerId, Peer>,
    /// Klient bieżącego żądania.
    cur: PeerId,
    watchdog: Duration,
    emergency: Emergency,
}

/// Błąd protokołu JSON-RPC.
pub(crate) struct RpcError {
    code: i64,
    message: String,
}

impl RpcError {
    pub(crate) fn invalid_params(m: impl Into<String>) -> Self {
        RpcError {
            code: -32602,
            message: m.into(),
        }
    }
}

impl Server {
    pub fn new(session: Session, cfg: Config) -> Self {
        let bench = {
            let mut b = Workbench::new(session);
            b.autosave = cfg.autosave;
            if let Some(d) = &cfg.prefs_dir {
                // profile urządzeń i makra w katalogu preferencji (ADR 0010)
                if let Err(e) = b.active().machines.lock().expect("urządzenia").set_store(d) {
                    tracing::warn!("urządzenia: {e}");
                }
            }
            b.prefs = cfg.prefs_dir.map(PrefStore::new);
            b
        };
        let audit = Arc::new(Mutex::new(audit::Audit::new(cfg.audit_log)));
        let emergency = Emergency {
            machines: bench.active().machines.clone(),
            selected: Arc::new(Mutex::new(bench.active().device.clone())),
            audit: audit.clone(),
        };
        let mut peers = HashMap::new();
        peers.insert(0, Peer::new());
        Server {
            reg: Registry::builtin(),
            bench,
            policy: cfg.policy,
            audit,
            screenshot: cfg.screenshot,
            peers,
            cur: 0,
            watchdog: cfg.watchdog.unwrap_or(DEFAULT_WATCHDOG),
            emergency,
        }
    }

    /// Aktywny dokument.
    pub fn session(&self) -> &Session {
        self.bench.active()
    }

    pub fn workbench(&self) -> &Workbench {
        &self.bench
    }

    /// Dostęp lokalnego GUI (bez polityki MCP — działa użytkownik, nie agent).
    pub fn workbench_mut(&mut self) -> &mut Workbench {
        &mut self.bench
    }

    pub fn registry(&self) -> &Registry {
        &self.reg
    }

    /// Ścieżka awaryjna (stop, anulowanie joga) dla transportów.
    pub fn emergency(&self) -> Emergency {
        self.emergency.clone()
    }

    /// Nazwa klienta (z `clientInfo`) dla dziennika audytu.
    pub fn client_name(&self, peer: PeerId) -> String {
        self.peers
            .get(&peer)
            .map(|p| p.client.clone())
            .unwrap_or_else(|| "unknown".into())
    }

    pub(crate) fn peer(&self) -> &Peer {
        self.peers.get(&self.cur).expect("bieżący klient")
    }

    pub(crate) fn peer_mut(&mut self) -> &mut Peer {
        self.peers.entry(self.cur).or_insert_with(Peer::new)
    }

    /// Ostatnia aktywność klienta zarejestrowana przez transport (żądania poza kolejką).
    pub fn touch(&mut self, peer: PeerId, at: Instant) {
        let p = self.peers.entry(peer).or_insert_with(Peer::new);
        if at > p.last_seen {
            p.last_seen = at;
        }
    }

    /// Obsługuje jedną wiadomość JSON-RPC; `None` dla powiadomień.
    pub fn handle(&mut self, msg: Value) -> Option<Value> {
        self.handle_ctx(msg, &Ctx::default())
    }

    /// Obsługuje wiadomość klienta `ctx.peer` z kanałem transportu (postęp, potwierdzenia).
    pub fn handle_ctx(&mut self, msg: Value, ctx: &Ctx) -> Option<Value> {
        self.cur = ctx.peer;
        self.peer_mut().last_seen = Instant::now();
        let reply = self.dispatch(msg, ctx);
        *self.emergency.selected.lock().expect("wybór") = self.bench.active().device.clone();
        reply
    }

    fn dispatch(&mut self, msg: Value, ctx: &Ctx) -> Option<Value> {
        let id = msg.get("id").cloned();
        let Some(method) = msg.get("method").and_then(Value::as_str) else {
            // odpowiedzi klienta trafiają do transportu (żądania serwera), tu — błąd
            return id.map(|id| err(id, -32600, "brak pola method"));
        };
        let params = msg.get("params").cloned().unwrap_or(Value::Null);
        tracing::debug!(method, id = ?id, "żądanie MCP");
        let Some(id) = id else {
            // notifications/initialized, notifications/cancelled (obsługuje transport) itp.
            return None;
        };
        let res = match method {
            "initialize" => Ok(self.initialize(&params)),
            "ping" => Ok(json!({})),
            "tools/list" => {
                let mut list = tools::definitions();
                list.extend(shortcuts::definitions(&self.reg));
                list.extend(devices::definitions(&self.reg));
                Ok(json!({ "tools": list }))
            }
            "tools/call" => self.call_tool(&params, ctx),
            "resources/list" => Ok(resources::list(self)),
            "resources/templates/list" => Ok(resources::templates()),
            "resources/read" => resources::read(self, &params),
            "resources/subscribe" => self.subscribe(&params),
            "resources/unsubscribe" => {
                let uri = params
                    .get("uri")
                    .and_then(Value::as_str)
                    .ok_or_else(|| RpcError::invalid_params("brak uri"));
                uri.map(|u| {
                    let u = u.to_string();
                    self.peer_mut().subs.remove(&u);
                    json!({})
                })
            }
            "prompts/list" => Ok(prompts::list()),
            "prompts/get" => prompts::get(self, &params),
            other => Err(RpcError {
                code: -32601,
                message: format!("nieznana metoda {other}"),
            }),
        };
        Some(match res {
            Ok(r) => json!({ "jsonrpc": "2.0", "id": id, "result": r }),
            Err(e) => err(id, e.code, &e.message),
        })
    }

    fn initialize(&mut self, params: &Value) -> Value {
        let peer = self.cur;
        self.register_peer(peer, params);
        Self::initialize_reply(params)
    }

    /// Rejestruje klienta z parametrów `initialize` (nazwa, obsługa elicitation).
    pub fn register_peer(&mut self, peer: PeerId, params: &Value) {
        let elicitation = params
            .pointer("/capabilities/elicitation")
            .is_some_and(Value::is_object);
        let p = self.peers.entry(peer).or_insert_with(Peer::new);
        if let Some(c) = client_label(params) {
            p.client = c;
        }
        p.elicitation = elicitation;
        p.known = true;
        tracing::info!(peer, client = %p.client, elicitation, "MCP initialize");
    }

    /// Rejestruje klienta, jeśli jeszcze nie jest znany (transport HTTP).
    pub fn ensure_peer(&mut self, peer: PeerId, params: &Value) {
        if !self.peers.get(&peer).is_some_and(|p| p.known) {
            self.register_peer(peer, params);
        }
    }

    /// Odpowiedź na `initialize` (negocjacja wersji, możliwości, instrukcje) — bez stanu,
    /// więc transport HTTP odpowiada nawet wtedy, gdy serwer jest zajęty.
    pub fn initialize_reply(params: &Value) -> Value {
        let requested = params
            .get("protocolVersion")
            .and_then(Value::as_str)
            .unwrap_or("");
        let version = PROTOCOL_VERSIONS
            .iter()
            .find(|v| **v == requested)
            .copied()
            .unwrap_or(PROTOCOL_VERSIONS[0]);
        json!({
            "protocolVersion": version,
            "capabilities": {
                "tools": { "listChanged": false },
                "resources": { "subscribe": true, "listChanged": false },
                "prompts": { "listChanged": false }
            },
            "serverInfo": { "name": SERVER_NAME, "title": "BurnCraft", "version": SERVER_VERSION },
            "instructions": INSTRUCTIONS
        })
    }

    fn call_tool(&mut self, params: &Value, ctx: &Ctx) -> Result<Value, RpcError> {
        let name = params
            .get("name")
            .and_then(Value::as_str)
            .ok_or_else(|| RpcError::invalid_params("brak name"))?
            .to_string();
        let args = params
            .get("arguments")
            .cloned()
            .unwrap_or_else(|| json!({}));
        // postęp i anulowanie (REQ-15-008, 009): transport daje kanał i flagę anulowania
        let installed = if let Some(ch) = &ctx.channel {
            let token = params.pointer("/_meta/progressToken").cloned();
            self.bench.progress = Some(bc_commands::Progress {
                report: progress_reporter(ch.clone(), token),
                cancel: ctx.cancel.clone(),
            });
            true
        } else {
            false
        };
        let outcome = tools::call(self, &name, &args, ctx);
        if installed {
            self.bench.progress = None;
        }
        let outcome = outcome?;
        let client = self.peer().client.clone();
        self.audit
            .lock()
            .expect("audyt")
            .record(&client, &name, &args, &outcome);
        if self.bench.autosave.is_some() {
            if let Err(e) = self.bench.autosave_tick(std::time::SystemTime::now()) {
                tracing::warn!("autozapis nie powiódł się: {e}");
            }
        }
        Ok(outcome.into_result())
    }

    // ---------- subskrypcje (REQ-15-006, 062, 083) ----------

    fn subscribe(&mut self, params: &Value) -> Result<Value, RpcError> {
        let uri = params
            .get("uri")
            .and_then(Value::as_str)
            .ok_or_else(|| RpcError::invalid_params("brak uri"))?
            .to_string();
        // zasób musi istnieć
        resources::read(self, &json!({ "uri": uri }))?;
        let h = self.fingerprint(&uri);
        self.peer_mut().subs.insert(uri, h);
        Ok(json!({}))
    }

    fn fingerprint(&mut self, uri: &str) -> u64 {
        let mut h = std::collections::hash_map::DefaultHasher::new();
        if uri == "burncraft://document" {
            // skrót stanu zamiast pełnego raportu (dokument może być duży)
            let b = &self.bench;
            b.active.hash(&mut h);
            let s = b.active();
            s.uid.hash(&mut h);
            s.revision.hash(&mut h);
            s.selection.hash(&mut h);
            s.dirty.hash(&mut h);
        } else {
            match resources::read(self, &json!({ "uri": uri })) {
                Ok(v) => v.to_string().hash(&mut h),
                Err(e) => e.message.hash(&mut h),
            }
        }
        h.finish()
    }

    /// Powiadomienia `notifications/resources/updated` o zmienionych zasobach klienta.
    pub fn poll_updates(&mut self, peer: PeerId) -> Vec<Value> {
        let Some(p) = self.peers.get(&peer) else {
            return Vec::new();
        };
        let subs: Vec<(String, u64)> = p.subs.iter().map(|(u, h)| (u.clone(), *h)).collect();
        let mut out = Vec::new();
        for (uri, old) in subs {
            let h = self.fingerprint(&uri);
            if h != old {
                if let Some(p) = self.peers.get_mut(&peer) {
                    p.subs.insert(uri.clone(), h);
                }
                out.push(
                    json!({"jsonrpc": "2.0", "method": "notifications/resources/updated",
                    "params": {"uri": uri}}),
                );
            }
        }
        out
    }

    // ---------- watchdog joga i utrata klienta (REQ-15-107, 108) ----------

    /// Anuluje jog klientów milczących dłużej niż limit watchdoga.
    pub fn watchdog_tick(&mut self, now: Instant) {
        let limit = self.watchdog;
        let em = self.emergency.clone();
        for (id, p) in self.peers.iter_mut() {
            if p.jogging.is_empty() {
                continue;
            }
            let silent = now.saturating_duration_since(p.last_seen) > limit;
            let mut done = Vec::new();
            for (dev, at) in &p.jogging {
                let jogging = em.is_jogging(dev);
                if silent && jogging {
                    tracing::warn!(peer = id, device = %dev, "watchdog: klient milczy — anuluję jog");
                    em.jog_cancel(dev);
                    done.push(dev.clone());
                } else if !jogging && now.saturating_duration_since(*at) > Duration::from_secs(1) {
                    done.push(dev.clone()); // jog się skończył
                }
            }
            for d in done {
                p.jogging.remove(&d);
            }
        }
    }

    /// Utrata klienta (EOF stdio, koniec sesji HTTP): jog anulowany, zadania trwają dalej.
    pub fn client_lost(&mut self, peer: PeerId) {
        let Some(p) = self.peers.remove(&peer) else {
            return;
        };
        for dev in p.jogging.keys() {
            if self.emergency.is_jogging(dev) {
                tracing::warn!(device = %dev, "utrata klienta MCP — anuluję jog");
                self.emergency.jog_cancel(dev);
            }
        }
        if peer == 0 {
            self.peers.insert(0, Peer::new());
        }
        if self.emergency.jobs_active() {
            tracing::info!("utrata klienta MCP — zadanie na urządzeniu trwa dalej");
        }
    }

    /// Pętla stdio: jedna wiadomość JSON na linię, logi wyłącznie na stderr.
    pub fn run_stdio(&mut self) -> std::io::Result<()> {
        self.run_io(std::io::BufReader::new(std::io::stdin()), std::io::stdout())
    }

    /// Transport stdio na dowolnych strumieniach (testy, potoki).
    pub fn run_io<R, W>(&mut self, input: R, output: W) -> std::io::Result<()>
    where
        R: std::io::BufRead + Send + 'static,
        W: std::io::Write + Send + 'static,
    {
        stdio::run(self, input, output)
    }
}

/// Powiadomienia `notifications/progress` (co najmniej co 50 ms, rosnąco).
fn progress_reporter(ch: Arc<dyn Channel>, token: Option<Value>) -> Arc<dyn Fn(f64) + Send + Sync> {
    let last = Mutex::new((-1.0f64, Instant::now() - Duration::from_secs(1)));
    Arc::new(move |f: f64| {
        let Some(t) = &token else { return };
        let mut l = last.lock().expect("postęp");
        let now = Instant::now();
        if f <= l.0 || (f < 1.0 && now.duration_since(l.1) < Duration::from_millis(50)) {
            return;
        }
        *l = (f, now);
        ch.notify(json!({"jsonrpc": "2.0", "method": "notifications/progress",
            "params": {"progressToken": t, "progress": f, "total": 1.0}}));
    })
}

/// Nazwa klienta z `clientInfo` (`nazwa wersja`).
pub fn client_label(params: &Value) -> Option<String> {
    params
        .pointer("/clientInfo/name")
        .and_then(Value::as_str)
        .map(|c| {
            let ver = params
                .pointer("/clientInfo/version")
                .and_then(Value::as_str)
                .unwrap_or("");
            format!("{c} {ver}").trim().to_string()
        })
}

pub(crate) fn err(id: Value, code: i64, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

/// Klucz żądania (id JSON-RPC jako tekst).
pub(crate) fn id_key(id: &Value) -> String {
    id.to_string()
}

pub(crate) fn is_cancelled(c: &AtomicBool) -> bool {
    c.load(Ordering::SeqCst)
}

#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_m1;
#[cfg(test)]
mod tests_m12;
#[cfg(test)]
mod tests_m13;
#[cfg(test)]
mod tests_m2;
