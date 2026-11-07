//! bc-commands — rejestr poleceń BurnCraft (sekcja 1.3 specyfikacji).
//!
//! Każda operacja jest poleceniem z `id`, opisem, schematem JSON parametrów
//! i wyniku, flagami (`undoable`, `requires_device`, `safety_tier`), domyślnym
//! skrótem i listą wymagań, które realizuje. GUI, skrypty, CLI i MCP wywołują
//! wyłącznie [`Session::execute`] / [`Session::batch`].

mod builtin;
mod deform;
mod edit;
mod files;
pub mod gcode;
mod machine;
mod nodes;
mod panels;
mod plan;
pub mod prefs;
mod shaping;
mod structure;
mod ui;
mod window;
pub mod workbench;

pub use machine::JobMap;
pub use workbench::Workbench;
pub mod inspect;

use std::collections::BTreeMap;
use std::path::PathBuf;

use bc_doc::{history::History, Document, ObjectId};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Poziom bezpieczeństwa polecenia (ADR 0005). Kolejność = rosnące ryzyko.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "lowercase")]
pub enum SafetyTier {
    Read,
    Design,
    Motion,
    Config,
    Fire,
}

impl SafetyTier {
    pub fn as_str(self) -> &'static str {
        match self {
            SafetyTier::Read => "read",
            SafetyTier::Design => "design",
            SafetyTier::Motion => "motion",
            SafetyTier::Config => "config",
            SafetyTier::Fire => "fire",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error, Serialize)]
#[error("{code}: {message}")]
pub struct CmdError {
    /// unknown_command | invalid_params | not_found | locked | no_device | invalid_state | failed
    pub code: &'static str,
    pub message: String,
}

impl CmdError {
    pub fn new(code: &'static str, message: impl Into<String>) -> Self {
        CmdError {
            code,
            message: message.into(),
        }
    }
}

impl From<bc_doc::DocError> for CmdError {
    fn from(e: bc_doc::DocError) -> Self {
        match e {
            bc_doc::DocError::NotFound(id) => {
                CmdError::new("not_found", format!("nie znaleziono obiektu {id}"))
            }
            other => CmdError::new("failed", other.to_string()),
        }
    }
}

pub type Handler = fn(&mut Session, Value) -> Result<Value, CmdError>;

/// Polecenie poziomu obszaru roboczego (wiele dokumentów, schowek, autozapis, preferencje).
pub type BenchHandler = fn(&mut Workbench, Value) -> Result<Value, CmdError>;

#[derive(Clone, Copy)]
pub enum Exec {
    /// Działa na aktywnym dokumencie.
    Session(Handler),
    /// Działa na obszarze roboczym (wymaga [`Workbench`]).
    Bench(BenchHandler),
    /// Wykonuje je okno GUI (widok, narzędzia, okna dialogowe); rdzeń przekazuje
    /// je przez [`WindowHook`] (REQ-01-029).
    Window,
}

/// Kto wykonuje polecenie (pole `executor` opisu polecenia).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Executor {
    /// Rdzeń, aktywny dokument.
    Document,
    /// Rdzeń, obszar roboczy (zakładki, schowek, preferencje).
    Workbench,
    /// Okno GUI.
    Window,
}

/// Przekazanie polecenia okna GUI: (id, parametry) → wynik albo błąd okna.
/// Dostarcza go host GUI; bez okna polecenia okna zwracają `no_window`.
#[derive(Clone)]
#[allow(clippy::type_complexity)]
pub struct WindowHook(
    pub std::sync::Arc<dyn Fn(&str, Value) -> Result<Value, CmdError> + Send + Sync>,
);

impl std::fmt::Debug for WindowHook {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("WindowHook")
    }
}

/// Postęp i anulowanie długiego polecenia (REQ-16-006). Ustawia go wywołujący
/// (host GUI); polecenia zgłaszają postęp przez [`Session::tick`].
#[derive(Clone)]
#[allow(clippy::type_complexity)]
pub struct Progress {
    /// Zgłoszenie postępu 0–1 (wywołujący sam ogranicza częstotliwość).
    pub report: std::sync::Arc<dyn Fn(f64) + Send + Sync>,
    /// Ustawiona flaga przerywa polecenie błędem `cancelled` (bez zmian w dokumencie).
    pub cancel: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

impl std::fmt::Debug for Progress {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Progress")
    }
}

pub(crate) fn no_window(id: &str) -> CmdError {
    CmdError::new(
        "no_window",
        format!("{id} wykonuje okno GUI BurnCraft, a żadne okno nie jest podłączone"),
    )
}

/// Opis polecenia w rejestrze.
#[derive(Debug, Clone, Serialize)]
pub struct CommandSpec {
    /// Wykonawca: dokument, obszar roboczy albo okno GUI (ustawia rejestr).
    pub executor: Executor,
    pub id: &'static str,
    pub title: &'static str,
    pub description: &'static str,
    pub params_schema: Value,
    pub result_schema: Value,
    pub undoable: bool,
    pub requires_device: bool,
    pub safety_tier: SafetyTier,
    pub shortcut: Option<&'static str>,
    pub requirements: &'static [&'static str],
}

pub struct Command {
    pub spec: CommandSpec,
    pub exec: Exec,
}

/// Schemat JSON typu (parametrów lub wyniku) jako `serde_json::Value`.
pub fn schema_of<T: JsonSchema>() -> Value {
    let mut v = serde_json::to_value(schemars::schema_for!(T)).expect("schemat serializowalny");
    if let Some(m) = v.as_object_mut() {
        m.remove("$schema");
    }
    v
}

#[derive(Default)]
pub struct Registry {
    commands: BTreeMap<&'static str, Command>,
}

impl Registry {
    /// Rejestr z wbudowanymi poleceniami.
    pub fn builtin() -> Self {
        let mut r = Registry::default();
        builtin::register_all(&mut r);
        edit::register_all(&mut r);
        nodes::register_all(&mut r);
        deform::register_all(&mut r);
        ui::register_all(&mut r);
        panels::register_all(&mut r);
        structure::register_all(&mut r);
        files::register_all(&mut r);
        shaping::register_all(&mut r);
        plan::register_all(&mut r);
        gcode::register_all(&mut r);
        machine::register_all(&mut r);
        window::register_all(&mut r);
        workbench::register_all(&mut r);
        r
    }

    fn insert(&mut self, mut spec: CommandSpec, exec: Exec) {
        assert!(
            !self.commands.contains_key(spec.id),
            "polecenie {} zarejestrowane dwukrotnie",
            spec.id
        );
        spec.executor = match exec {
            Exec::Session(_) => Executor::Document,
            Exec::Bench(_) => Executor::Workbench,
            Exec::Window => Executor::Window,
        };
        self.commands.insert(spec.id, Command { spec, exec });
    }

    pub fn register(&mut self, spec: CommandSpec, handler: Handler) {
        self.insert(spec, Exec::Session(handler));
    }

    pub fn register_bench(&mut self, spec: CommandSpec, handler: BenchHandler) {
        self.insert(spec, Exec::Bench(handler));
    }

    /// Polecenie wykonywane przez okno GUI.
    pub fn register_window(&mut self, spec: CommandSpec) {
        self.insert(spec, Exec::Window);
    }

    pub fn get(&self, id: &str) -> Option<&Command> {
        self.commands.get(id)
    }

    /// Polecenia, których id, tytuł lub opis zawiera `filter` (bez rozróżniania wielkości liter).
    pub fn list(&self, filter: Option<&str>) -> Vec<&CommandSpec> {
        let f = filter.map(|s| s.to_lowercase());
        self.commands
            .values()
            .map(|c| &c.spec)
            .filter(|s| match &f {
                None => true,
                Some(f) => {
                    s.id.to_lowercase().contains(f)
                        || s.title.to_lowercase().contains(f)
                        || s.description.to_lowercase().contains(f)
                }
            })
            .collect()
    }
}

/// Stan sesji edycji: dokument, zaznaczenie, historia, plik, urządzenie.
pub struct Session {
    pub doc: Document,
    pub selection: Vec<ObjectId>,
    pub history: History,
    pub path: Option<PathBuf>,
    pub dirty: bool,
    /// Podłączone urządzenie (sterowniki od M2). `None` = brak.
    pub device: Option<String>,
    /// Identyfikator sesji (nazwa pliku autozapisu).
    pub uid: String,
    /// Licznik zmian dokumentu (każdy zapisany krok historii, cofnięcie, ponowienie).
    pub revision: u64,
    /// Stan autozapisu (zarządza nim [`Workbench`]).
    pub autosave: workbench::AutosaveState,
    /// Postęp i anulowanie bieżącego polecenia (REQ-16-006).
    pub progress: Option<Progress>,
    /// Planer z pamięcią podręczną warstw (REQ-08-019).
    pub planner: bc_plan::Planner,
    /// Program G-code wczytany do podglądu i uruchomienia (REQ-02-089).
    pub gcode: Option<gcode::LoadedGcode>,
    /// Urządzenia (profile, połączenia) — współdzielone przez zakładki (ADR 0010).
    pub machines: bc_drivers::MachinesHandle,
    /// Przebiegi bieżącego zadania na urządzeniu (licznik przejść, REQ-03-054).
    pub job_map: Option<machine::JobMap>,
}

impl Default for Session {
    fn default() -> Self {
        Session::new(Document::default())
    }
}

/// Krok wsadu.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct BatchStep {
    /// Id polecenia.
    pub id: String,
    /// Parametry polecenia.
    #[serde(default)]
    pub params: Value,
}

impl Session {
    pub fn new(doc: Document) -> Self {
        Session {
            doc,
            selection: Vec::new(),
            history: History::default(),
            path: None,
            dirty: false,
            device: None,
            uid: workbench::new_uid(),
            revision: 0,
            autosave: Default::default(),
            progress: None,
            planner: bc_plan::Planner::new(),
            gcode: None,
            machines: bc_drivers::Machines::default().handle(),
            job_map: None,
        }
    }

    /// Nowa sesja dziedziczy urządzenia (połączenia i wybór) po `old`.
    pub fn with_machines_of(mut self, old: &Session) -> Self {
        self.machines = old.machines.clone();
        self.device = old.device.clone();
        self
    }

    /// Krok długiej operacji: zgłasza postęp `done / total` (co 64 kroki) i przerywa
    /// polecenie błędem `cancelled`, gdy użytkownik je anulował. Polecenia są atomowe,
    /// więc przerwanie nie zostawia zmian.
    pub fn tick(&self, done: usize, total: usize) -> Result<(), CmdError> {
        if let Some(p) = &self.progress {
            if p.cancel.load(std::sync::atomic::Ordering::Relaxed) {
                return Err(CmdError::new("cancelled", "anulowano"));
            }
            if total > 0 && (done.is_multiple_of(64) || done + 1 >= total) {
                (p.report)((done as f64 / total as f64).clamp(0.0, 1.0));
            }
        }
        Ok(())
    }

    pub(crate) fn touch(&mut self) {
        self.dirty = true;
        self.revision += 1;
    }

    /// Wykonuje polecenie; polecenia `undoable` zapisują jeden krok historii.
    pub fn execute(&mut self, reg: &Registry, id: &str, params: Value) -> Result<Value, CmdError> {
        let cmd = self.check(reg, id)?;
        if matches!(cmd.exec, Exec::Window) {
            return Err(no_window(id));
        }
        let Exec::Session(handler) = cmd.exec else {
            return Err(CmdError::new(
                "invalid_state",
                format!(
                    "{id} działa na obszarze roboczym (Workbench), nie na pojedynczym dokumencie"
                ),
            ));
        };
        let before = cmd
            .spec
            .undoable
            .then(|| (self.doc.clone(), self.selection.clone()));
        let out = match handler(self, normalize(params)) {
            Ok(v) => v,
            Err(e) => {
                // polecenia są atomowe: błąd w trakcie nie zostawia częściowych zmian
                if let Some((d, sel)) = before {
                    self.doc = d;
                    self.selection = sel;
                }
                return Err(e);
            }
        };
        let before = before.map(|(d, _)| d);
        if let Some(b) = before {
            if b != self.doc {
                self.history.record(id, b);
                self.touch();
            }
        }
        Ok(out)
    }

    /// Wykonuje sekwencję poleceń atomowo: błąd dowolnego kroku przywraca stan
    /// sprzed wsadu; sukces zapisuje jeden krok historii (REQ-01-038).
    pub fn batch(
        &mut self,
        reg: &Registry,
        steps: &[BatchStep],
    ) -> Result<Vec<Value>, (usize, CmdError)> {
        for (i, s) in steps.iter().enumerate() {
            let cmd = self.check(reg, &s.id).map_err(|e| (i, e))?;
            if matches!(cmd.exec, Exec::Bench(_) | Exec::Window)
                || (!cmd.spec.undoable && cmd.spec.safety_tier != SafetyTier::Read)
            {
                return Err((
                    i,
                    CmdError::new(
                        "invalid_params",
                        format!("polecenia {} nie można użyć we wsadzie", s.id),
                    ),
                ));
            }
        }
        let before_doc = self.doc.clone();
        let before_sel = self.selection.clone();
        let mut out = Vec::with_capacity(steps.len());
        for (i, s) in steps.iter().enumerate() {
            let Exec::Session(handler) = reg.get(&s.id).expect("sprawdzone wyżej").exec else {
                unreachable!()
            };
            if let Err(e) = self.tick(i, steps.len()) {
                self.doc = before_doc;
                self.selection = before_sel;
                return Err((i, e));
            }
            match handler(self, normalize(s.params.clone())) {
                Ok(v) => out.push(v),
                Err(e) => {
                    self.doc = before_doc;
                    self.selection = before_sel;
                    return Err((i, e));
                }
            }
        }
        if before_doc != self.doc {
            self.history.record("batch", before_doc);
            self.touch();
        }
        Ok(out)
    }

    fn check<'r>(&self, reg: &'r Registry, id: &str) -> Result<&'r Command, CmdError> {
        let cmd = reg
            .get(id)
            .ok_or_else(|| CmdError::new("unknown_command", format!("nieznane polecenie {id}")))?;
        if cmd.spec.requires_device && self.device.is_none() {
            return Err(CmdError::new(
                "no_device",
                format!("{id} wymaga podłączonego urządzenia (sterowniki od etapu M2)"),
            ));
        }
        Ok(cmd)
    }

    /// Obiekty docelowe: jawne `ids` albo bieżące zaznaczenie (REQ-01-037).
    pub fn targets(&self, ids: &Option<Vec<ObjectId>>) -> Result<Vec<ObjectId>, CmdError> {
        let t = match ids {
            Some(v) => v.clone(),
            None => self.selection.clone(),
        };
        if t.is_empty() {
            return Err(CmdError::new(
                "invalid_params",
                "brak obiektów: podaj ids albo zaznacz obiekty",
            ));
        }
        for id in &t {
            if self.doc.find(*id).is_none() {
                return Err(CmdError::new(
                    "not_found",
                    format!("nie znaleziono obiektu {id}"),
                ));
            }
        }
        Ok(t)
    }

    /// Usuwa z zaznaczenia obiekty, których już nie ma.
    pub fn prune_selection(&mut self) {
        let doc = &self.doc;
        self.selection.retain(|id| doc.find(*id).is_some());
    }
}

/// `null` -> `{}`, żeby polecenia bez parametrów przyjmowały brak `params`.
pub(crate) fn normalize(v: Value) -> Value {
    if v.is_null() {
        Value::Object(Default::default())
    } else {
        v
    }
}

/// Parsuje parametry polecenia; błąd -> `invalid_params`.
pub fn parse<T: serde::de::DeserializeOwned>(v: Value) -> Result<T, CmdError> {
    serde_json::from_value(v).map_err(|e| CmdError::new("invalid_params", e.to_string()))
}

pub fn ok<T: Serialize>(v: T) -> Result<Value, CmdError> {
    serde_json::to_value(v).map_err(|e| CmdError::new("failed", e.to_string()))
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
mod tests_m14;
#[cfg(test)]
mod tests_m14b;
#[cfg(test)]
mod tests_m14c;
#[cfg(test)]
mod tests_m1close;
#[cfg(test)]
mod tests_m1core;
#[cfg(test)]
mod tests_m2;
