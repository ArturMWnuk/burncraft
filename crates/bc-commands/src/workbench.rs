//! Obszar roboczy: wiele dokumentów w zakładkach, wspólny schowek, autozapis
//! z odzyskiwaniem po awarii i preferencje z kopiami zapasowymi (M1.2).

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime};

use bc_doc::{Document, Layer, Object, ObjectId};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::builtin::{top_most, Ids, IdsResult, NoParams};
use crate::prefs::{BackupInfo, PrefChange, PrefStore};
use crate::{
    normalize, ok, parse, schema_of, BatchStep, BenchHandler, CmdError, CommandSpec, Exec,
    Registry, SafetyTier, Session,
};

/// Stan autozapisu jednego dokumentu.
#[derive(Debug, Clone, Default)]
pub struct AutosaveState {
    /// Plik autozapisu (gdy powstał).
    pub file: Option<PathBuf>,
    /// Rewizja dokumentu zapisana w ostatnim autozapisie.
    pub saved_rev: u64,
    /// Początek odliczania do następnego autozapisu.
    pub base: Option<SystemTime>,
}

/// Unikalny identyfikator sesji (nazwa pliku autozapisu).
pub fn new_uid() -> String {
    static N: AtomicU64 = AtomicU64::new(0);
    let t = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!(
        "{:x}-{}-{}",
        t,
        std::process::id(),
        N.fetch_add(1, Ordering::Relaxed)
    )
}

fn sidecar(file: &Path) -> PathBuf {
    file.with_extension("json")
}

/// Usuwa plik autozapisu sesji (po zapisie, zamknięciu albo zastąpieniu dokumentu).
pub fn discard_autosave(s: &mut Session) {
    if let Some(f) = s.autosave.file.take() {
        let _ = std::fs::remove_file(sidecar(&f));
        let _ = std::fs::remove_file(f);
    }
    s.autosave.saved_rev = s.revision;
    s.autosave.base = None;
}

fn ms(t: SystemTime) -> u64 {
    t.duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Konfiguracja autozapisu.
#[derive(Debug, Clone)]
pub struct Autosave {
    pub dir: PathBuf,
    pub interval: Duration,
}

/// Metadane autozapisu (plik `.json` obok `.bcraft`).
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct RecoverInfo {
    /// Plik autozapisu do `file.recover`.
    pub path: String,
    pub name: String,
    /// Oryginalny plik dokumentu (jeśli był zapisany).
    pub original_path: Option<String>,
    pub saved_ms: u64,
}

pub struct Workbench {
    pub docs: Vec<Session>,
    pub active: usize,
    /// Schowek: kopie obiektów (współrzędne świata, klony zmaterializowane) — działa między dokumentami.
    pub clipboard: Vec<Object>,
    /// Warstwy używane przez obiekty w schowku (dla dokumentów, które ich nie mają).
    pub clipboard_layers: Vec<Layer>,
    pub autosave: Option<Autosave>,
    pub prefs: Option<PrefStore>,
    /// Okno GUI wykonujące polecenia okna (widok, narzędzia; REQ-01-029).
    pub window: Option<crate::WindowHook>,
    /// Postęp i anulowanie bieżącego polecenia (REQ-16-006); przekazywany aktywnemu dokumentowi.
    pub progress: Option<crate::Progress>,
}

impl Default for Workbench {
    fn default() -> Self {
        Workbench::new(Session::default())
    }
}

impl Workbench {
    pub fn new(s: Session) -> Self {
        Workbench {
            docs: vec![s],
            active: 0,
            clipboard: Vec::new(),
            clipboard_layers: Vec::new(),
            autosave: None,
            prefs: None,
            window: None,
            progress: None,
        }
    }

    pub fn active(&self) -> &Session {
        &self.docs[self.active]
    }

    pub fn active_mut(&mut self) -> &mut Session {
        &mut self.docs[self.active]
    }

    /// Wykonuje polecenie: polecenia dokumentu trafiają do aktywnej zakładki,
    /// polecenia obszaru roboczego — tutaj. Polecenia `undoable` zapisują krok
    /// historii aktywnego dokumentu.
    pub fn execute(&mut self, reg: &Registry, id: &str, params: Value) -> Result<Value, CmdError> {
        let idx = self.active;
        self.docs[idx].progress = self.progress.clone();
        let r = self.execute_inner(reg, id, params);
        for d in &mut self.docs {
            d.progress = None;
        }
        r
    }

    fn execute_inner(
        &mut self,
        reg: &Registry,
        id: &str,
        params: Value,
    ) -> Result<Value, CmdError> {
        let cmd = reg
            .get(id)
            .ok_or_else(|| CmdError::new("unknown_command", format!("nieznane polecenie {id}")))?;
        match cmd.exec {
            Exec::Session(_) => self.active_mut().execute(reg, id, params),
            Exec::Window => match &self.window {
                Some(w) => (w.0)(id, normalize(params)),
                None => Err(crate::no_window(id)),
            },
            Exec::Bench(h) => {
                let idx = self.active;
                let before = cmd.spec.undoable.then(|| self.docs[idx].doc.clone());
                let out = match h(self, normalize(params)) {
                    Ok(v) => v,
                    Err(e) => {
                        if let Some(b) = before {
                            if self.active == idx && idx < self.docs.len() {
                                self.docs[idx].doc = b;
                            }
                        }
                        return Err(e);
                    }
                };
                if let Some(b) = before {
                    if self.active == idx && idx < self.docs.len() && b != self.docs[idx].doc {
                        let s = &mut self.docs[idx];
                        s.history.record(id, b);
                        s.touch();
                    }
                }
                Ok(out)
            }
        }
    }

    /// Wsad na aktywnym dokumencie (polecenia obszaru roboczego nie wchodzą do wsadu).
    pub fn batch(
        &mut self,
        reg: &Registry,
        steps: &[BatchStep],
    ) -> Result<Vec<Value>, (usize, CmdError)> {
        let p = self.progress.clone();
        let s = self.active_mut();
        s.progress = p;
        let r = s.batch(reg, steps);
        s.progress = None;
        r
    }

    /// Autozapis zmienionych dokumentów (REQ-02-054): odliczanie startuje przy pierwszej
    /// niezapisanej zmianie; po upływie `interval` dokument trafia do `<dir>/autosave-<uid>.bcraft`.
    /// Zwraca zapisane pliki.
    pub fn autosave_tick(&mut self, now: SystemTime) -> Result<Vec<PathBuf>, CmdError> {
        let Some(cfg) = self.autosave.clone() else {
            return Ok(Vec::new());
        };
        let mut written = Vec::new();
        for s in &mut self.docs {
            if s.revision == s.autosave.saved_rev {
                continue;
            }
            let base = *s.autosave.base.get_or_insert(now);
            if now.duration_since(base).unwrap_or(Duration::ZERO) < cfg.interval {
                continue;
            }
            std::fs::create_dir_all(&cfg.dir)
                .map_err(|e| CmdError::new("failed", format!("autozapis: {e}")))?;
            let file = cfg.dir.join(format!("autosave-{}.bcraft", s.uid));
            bc_doc::bcraft::save(&s.doc, &bc_doc::bcraft::Extras::default(), &file)?;
            let info = RecoverInfo {
                path: file.display().to_string(),
                name: s.doc.name.clone(),
                original_path: s.path.as_ref().map(|p| p.display().to_string()),
                saved_ms: ms(now),
            };
            std::fs::write(
                sidecar(&file),
                serde_json::to_vec_pretty(&info).unwrap_or_default(),
            )
            .map_err(|e| CmdError::new("failed", format!("autozapis: {e}")))?;
            s.autosave.file = Some(file.clone());
            s.autosave.saved_rev = s.revision;
            s.autosave.base = Some(now);
            written.push(file);
        }
        Ok(written)
    }

    fn open_tab(&mut self, s: Session) -> usize {
        // urządzenia wspólne dla wszystkich zakładek
        let s = s.with_machines_of(self.active());
        self.docs.push(s);
        self.active = self.docs.len() - 1;
        self.active
    }
}

fn invalid(m: impl Into<String>) -> CmdError {
    CmdError::new("invalid_params", m)
}

// ---------- zakładki ----------

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct DocNewParams {
    #[serde(default)]
    name: Option<String>,
}

#[derive(Serialize, JsonSchema)]
struct TabResult {
    /// Indeks aktywnej zakładki.
    index: usize,
    /// Liczba otwartych dokumentów.
    count: usize,
}

fn doc_new(w: &mut Workbench, p: Value) -> Result<Value, CmdError> {
    let p: DocNewParams = parse(p)?;
    let idx = w.open_tab(Session::new(Document::new(
        p.name.as_deref().unwrap_or("Bez tytułu"),
    )));
    ok(TabResult {
        index: idx,
        count: w.docs.len(),
    })
}

#[derive(Serialize, JsonSchema)]
struct TabInfo {
    index: usize,
    name: String,
    file: Option<String>,
    modified: bool,
    objects: usize,
}

#[derive(Serialize, JsonSchema)]
struct TabList {
    active: usize,
    docs: Vec<TabInfo>,
}

fn doc_list(w: &mut Workbench, p: Value) -> Result<Value, CmdError> {
    let _: NoParams = parse(p)?;
    let docs = w
        .docs
        .iter()
        .enumerate()
        .map(|(i, s)| TabInfo {
            index: i,
            name: s.doc.name.clone(),
            file: s.path.as_ref().map(|p| p.display().to_string()),
            modified: s.dirty,
            objects: s.doc.objects.len(),
        })
        .collect();
    ok(TabList {
        active: w.active,
        docs,
    })
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct SwitchParams {
    /// Indeks zakładki (doc.list).
    index: usize,
}

fn doc_switch(w: &mut Workbench, p: Value) -> Result<Value, CmdError> {
    let p: SwitchParams = parse(p)?;
    if p.index >= w.docs.len() {
        return Err(invalid(format!(
            "nie ma zakładki {} (otwartych: {})",
            p.index,
            w.docs.len()
        )));
    }
    w.active = p.index;
    ok(TabResult {
        index: w.active,
        count: w.docs.len(),
    })
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct CloseParams {
    /// Zakładka do zamknięcia (domyślnie aktywna).
    #[serde(default)]
    index: Option<usize>,
    /// Zamknij mimo niezapisanych zmian.
    #[serde(default)]
    force: bool,
}

fn doc_close(w: &mut Workbench, p: Value) -> Result<Value, CmdError> {
    let p: CloseParams = parse(p)?;
    let idx = p.index.unwrap_or(w.active);
    if idx >= w.docs.len() {
        return Err(invalid(format!("nie ma zakładki {idx}")));
    }
    if w.docs[idx].dirty && !p.force {
        return Err(CmdError::new(
            "unsaved_changes",
            format!(
                "dokument „{}” ma niezapisane zmiany — zapisz go albo użyj force",
                w.docs[idx].doc.name
            ),
        ));
    }
    let mut closed = w.docs.remove(idx);
    discard_autosave(&mut closed);
    if w.docs.is_empty() {
        w.docs.push(Session::default().with_machines_of(&closed));
    }
    if w.active >= w.docs.len() || w.active > idx {
        w.active = w.active.saturating_sub(1).min(w.docs.len() - 1);
    }
    ok(TabResult {
        index: w.active,
        count: w.docs.len(),
    })
}

// ---------- schowek ----------

#[derive(Serialize, JsonSchema)]
struct CopyResult {
    /// Liczba obiektów w schowku.
    count: usize,
}

fn copy_to_clipboard(
    w: &mut Workbench,
    ids: &Option<Vec<ObjectId>>,
) -> Result<Vec<ObjectId>, CmdError> {
    let s = w.active();
    let ids = top_most(&s.doc, s.targets(ids)?);
    let mut objs = Vec::new();
    let mut layers: Vec<Layer> = Vec::new();
    for id in &ids {
        let o = s.doc.find(*id).expect("sprawdzone");
        let mut m = s.doc.materialize(o)?;
        let pa = s
            .doc
            .parent_affine(*id)
            .unwrap_or(bc_geom::Affine::IDENTITY);
        m.transform = (pa * m.affine()).as_coeffs();
        let mut used = Vec::new();
        collect_layers(&m, &mut used);
        for l in used {
            if !layers.iter().any(|x| x.index == l) {
                if let Ok(layer) = s.doc.layer(l) {
                    layers.push(layer.clone());
                }
            }
        }
        objs.push(m);
    }
    w.clipboard = objs;
    w.clipboard_layers = layers;
    Ok(ids)
}

fn collect_layers(o: &Object, out: &mut Vec<u8>) {
    out.push(o.layer);
    for c in o.children() {
        collect_layers(c, out);
    }
}

fn edit_copy(w: &mut Workbench, p: Value) -> Result<Value, CmdError> {
    let p: Ids = parse(p)?;
    copy_to_clipboard(w, &p.ids)?;
    ok(CopyResult {
        count: w.clipboard.len(),
    })
}

fn edit_cut(w: &mut Workbench, p: Value) -> Result<Value, CmdError> {
    let p: Ids = parse(p)?;
    let ids = copy_to_clipboard(w, &p.ids)?;
    let s = w.active_mut();
    if let Some(id) = ids
        .iter()
        .find(|id| s.doc.find(**id).is_some_and(|o| o.locked))
    {
        return Err(CmdError::new(
            "locked",
            format!("obiekt {id} jest zablokowany"),
        ));
    }
    for id in &ids {
        s.doc.remove(*id)?;
    }
    s.prune_selection();
    ok(CopyResult { count: ids.len() })
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct PasteParams {
    /// Przesunięcie wklejanych obiektów [dx, dy] w mm.
    #[serde(default)]
    offset: Option<[f64; 2]>,
}

fn edit_paste(w: &mut Workbench, p: Value) -> Result<Value, CmdError> {
    let p: PasteParams = parse(p)?;
    if w.clipboard.is_empty() {
        return Err(CmdError::new("invalid_state", "schowek jest pusty"));
    }
    let t = match p.offset {
        Some([dx, dy]) if dx.is_finite() && dy.is_finite() => bc_geom::Affine::translate((dx, dy)),
        Some(_) => return Err(invalid("offset musi być skończony")),
        None => bc_geom::Affine::IDENTITY,
    };
    let clip = w.clipboard.clone();
    let layers = w.clipboard_layers.clone();
    let s = w.active_mut();
    for l in layers {
        if s.doc.layer(l.index).is_err() {
            s.doc.layers.push(l);
        }
    }
    let mut ids = Vec::new();
    for o in clip {
        let mut c = crate::edit::clone_new_ids(&mut s.doc, &o);
        c.transform = (t * c.affine()).as_coeffs();
        ids.push(c.id);
        s.doc.objects.push(c);
    }
    s.selection = ids.clone();
    crate::builtin::ensure_in_range(&s.doc, &ids)?;
    ok(IdsResult { ids })
}

// ---------- odzyskiwanie po awarii ----------

#[derive(Serialize, JsonSchema)]
struct RecoverList {
    /// Autozapisy, od najnowszego.
    files: Vec<RecoverInfo>,
}

fn autosave_dir(w: &Workbench) -> Result<PathBuf, CmdError> {
    w.autosave
        .as_ref()
        .map(|a| a.dir.clone())
        .ok_or_else(|| CmdError::new("invalid_state", "autozapis nie jest skonfigurowany"))
}

fn file_recover_list(w: &mut Workbench, p: Value) -> Result<Value, CmdError> {
    let _: NoParams = parse(p)?;
    let dir = autosave_dir(w)?;
    let open: Vec<PathBuf> = w
        .docs
        .iter()
        .filter_map(|s| s.autosave.file.clone())
        .collect();
    let mut files: Vec<RecoverInfo> = std::fs::read_dir(&dir)
        .map(|rd| {
            rd.filter_map(|e| e.ok())
                .map(|e| e.path())
                .filter(|p| {
                    p.extension().is_some_and(|x| x == "json")
                        && p.file_name()
                            .is_some_and(|n| n.to_string_lossy().starts_with("autosave-"))
                })
                .filter_map(|p| {
                    serde_json::from_slice::<RecoverInfo>(&std::fs::read(&p).ok()?).ok()
                })
                .filter(|i| Path::new(&i.path).exists())
                .collect()
        })
        .unwrap_or_default();
    // autozapisy dokumentów otwartych w tej sesji nie są „po awarii”
    files.retain(|i| !open.iter().any(|o| Path::new(&i.path) == o.as_path()));
    files.sort_by_key(|f| std::cmp::Reverse(f.saved_ms));
    ok(RecoverList { files })
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct RecoverParams {
    /// Plik autozapisu (z file.recover_list).
    path: String,
}

#[derive(Serialize, JsonSchema)]
struct RecoverResult {
    index: usize,
    original_path: Option<String>,
}

fn file_recover(w: &mut Workbench, p: Value) -> Result<Value, CmdError> {
    let p: RecoverParams = parse(p)?;
    let dir = autosave_dir(w)?;
    let path = PathBuf::from(&p.path);
    let inside = path.parent().and_then(|d| d.canonicalize().ok()) == dir.canonicalize().ok();
    if !inside
        || !path
            .file_name()
            .is_some_and(|n| n.to_string_lossy().starts_with("autosave-"))
    {
        return Err(invalid("to nie jest plik autozapisu"));
    }
    let info: Option<RecoverInfo> = std::fs::read(sidecar(&path))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok());
    let (doc, _) = bc_doc::bcraft::load(&path)?;
    let mut s = Session::new(doc);
    s.touch();
    // plik autozapisu zostaje do pierwszego zapisu odzyskanego dokumentu
    s.autosave.file = Some(path);
    s.autosave.saved_rev = s.revision;
    let idx = w.open_tab(s);
    ok(RecoverResult {
        index: idx,
        original_path: info.and_then(|i| i.original_path),
    })
}

// ---------- preferencje ----------

fn store(w: &Workbench) -> Result<&PrefStore, CmdError> {
    w.prefs.as_ref().ok_or_else(|| {
        CmdError::new(
            "invalid_state",
            "magazyn preferencji nie jest skonfigurowany",
        )
    })
}

fn key_path(key: &str) -> Result<Vec<&str>, CmdError> {
    let parts: Vec<&str> = key.split('.').collect();
    if key.is_empty() || parts.iter().any(|p| p.is_empty()) {
        return Err(invalid("key: segmenty oddzielone kropką, np. ui.theme"));
    }
    Ok(parts)
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct PrefGetParams {
    /// Klucz z kropkami (np. ui.theme); brak = całe preferencje.
    #[serde(default)]
    key: Option<String>,
}

#[derive(Serialize, JsonSchema)]
struct PrefValue {
    value: Value,
}

fn prefs_get(w: &mut Workbench, p: Value) -> Result<Value, CmdError> {
    let p: PrefGetParams = parse(p)?;
    let all = store(w)?.load()?;
    let value = match p.key {
        None => all,
        Some(k) => key_path(&k)?
            .iter()
            .try_fold(&all, |v, part| v.get(*part))
            .cloned()
            .unwrap_or(Value::Null),
    };
    ok(PrefValue { value })
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct PrefSetParams {
    key: String,
    value: Value,
}

fn prefs_set(w: &mut Workbench, p: Value) -> Result<Value, CmdError> {
    let p: PrefSetParams = parse(p)?;
    let st = store(w)?;
    let mut all = st.load()?;
    let parts = key_path(&p.key)?;
    let mut cur = &mut all;
    for part in &parts[..parts.len() - 1] {
        if !cur.get(*part).is_some_and(Value::is_object) {
            cur[*part] = json!({});
        }
        cur = cur.get_mut(*part).expect("utworzone");
    }
    cur[*parts.last().unwrap()] = p.value;
    st.save(&all)?;
    ok(PrefValue { value: all })
}

#[derive(Serialize, JsonSchema)]
struct BackupList {
    backups: Vec<BackupInfo>,
}

fn prefs_backups(w: &mut Workbench, p: Value) -> Result<Value, CmdError> {
    let _: NoParams = parse(p)?;
    ok(BackupList {
        backups: store(w)?.backups()?,
    })
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct PrefRestoreParams {
    /// Nazwa kopii z prefs.backups.
    name: String,
}

#[derive(Serialize, JsonSchema)]
struct BackupDiff {
    name: String,
    created_ms: u64,
    /// Różnice kopii względem bieżących preferencji.
    changes: Vec<PrefChange>,
}

fn prefs_backup_diff(w: &mut Workbench, p: Value) -> Result<Value, CmdError> {
    let p: PrefRestoreParams = parse(p)?;
    let st = store(w)?;
    let (info, backup) = st.read_backup(&p.name)?;
    ok(BackupDiff {
        changes: crate::prefs::diff(&backup, &st.load()?),
        name: info.name,
        created_ms: info.created_ms,
    })
}

fn prefs_restore(w: &mut Workbench, p: Value) -> Result<Value, CmdError> {
    let p: PrefRestoreParams = parse(p)?;
    ok(PrefValue {
        value: store(w)?.restore(&p.name)?,
    })
}

// ---------- rejestracja ----------

#[allow(clippy::too_many_arguments)]
fn add<P: JsonSchema, R: JsonSchema>(
    r: &mut Registry,
    id: &'static str,
    title: &'static str,
    description: &'static str,
    tier: SafetyTier,
    undoable: bool,
    shortcut: Option<&'static str>,
    requirements: &'static [&'static str],
    handler: BenchHandler,
) {
    r.register_bench(
        CommandSpec {
            executor: crate::Executor::Document,
            id,
            title,
            description,
            params_schema: schema_of::<P>(),
            result_schema: schema_of::<R>(),
            undoable,
            requires_device: false,
            safety_tier: tier,
            shortcut,
            requirements,
        },
        handler,
    );
}

pub(crate) fn register_all(r: &mut Registry) {
    use SafetyTier::*;
    add::<DocNewParams, TabResult>(
        r,
        "doc.new",
        "Nowa zakładka",
        "Otwiera nowy pusty dokument w nowej zakładce i ją aktywuje.",
        Design,
        false,
        Some("Ctrl+T"),
        &["REQ-02-060"],
        doc_new,
    );
    add::<NoParams, TabList>(
        r,
        "doc.list",
        "Zakładki",
        "Lista otwartych dokumentów.",
        Read,
        false,
        None,
        &["REQ-02-060"],
        doc_list,
    );
    add::<SwitchParams, TabResult>(
        r,
        "doc.switch",
        "Przełącz zakładkę",
        "Aktywuje dokument o podanym indeksie.",
        Read,
        false,
        Some("Ctrl+Tab"),
        &["REQ-02-060"],
        doc_switch,
    );
    add::<CloseParams, TabResult>(
        r,
        "doc.close",
        "Zamknij zakładkę",
        "Zamyka dokument (z niezapisanymi zmianami tylko z force).",
        Design,
        false,
        Some("Ctrl+W"),
        &["REQ-02-060"],
        doc_close,
    );
    add::<Ids, CopyResult>(
        r,
        "edit.copy",
        "Kopiuj",
        "Kopiuje obiekty do schowka (działa między zakładkami; klony stają się samodzielne).",
        Read,
        false,
        Some("Ctrl+C"),
        &["REQ-02-061"],
        edit_copy,
    );
    add::<Ids, CopyResult>(
        r,
        "edit.cut",
        "Wytnij",
        "Kopiuje obiekty do schowka i usuwa je z dokumentu.",
        Design,
        true,
        Some("Ctrl+X"),
        &["REQ-02-061"],
        edit_cut,
    );
    add::<PasteParams, IdsResult>(r, "edit.paste", "Wklej", "Wkleja schowek do aktywnego dokumentu (te same współrzędne + offset); brakujące warstwy są dodawane.", Design, true, Some("Ctrl+V"), &["REQ-02-061"], edit_paste);
    add::<NoParams, RecoverList>(
        r,
        "file.recover_list",
        "Do odzyskania",
        "Autozapisy pozostawione przez poprzednie sesje (np. po awarii).",
        Read,
        false,
        None,
        &["REQ-02-055"],
        file_recover_list,
    );
    add::<RecoverParams, RecoverResult>(
        r,
        "file.recover",
        "Odzyskaj",
        "Otwiera autozapis w nowej zakładce jako niezapisany dokument.",
        Design,
        false,
        None,
        &["REQ-02-055"],
        file_recover,
    );
    add::<PrefGetParams, PrefValue>(
        r,
        "prefs.get",
        "Preferencje",
        "Odczyt preferencji (całość albo klucz z kropkami).",
        Read,
        false,
        None,
        &["REQ-02-056"],
        prefs_get,
    );
    add::<PrefSetParams, PrefValue>(
        r,
        "prefs.set",
        "Zmień preferencję",
        "Zapisuje preferencję; poprzednia wersja trafia do kopii zapasowej.",
        Config,
        false,
        None,
        &["REQ-02-056"],
        prefs_set,
    );
    add::<NoParams, BackupList>(
        r,
        "prefs.backups",
        "Kopie preferencji",
        "Kopie zapasowe preferencji, od najnowszej (przeglądarka kopii).",
        Read,
        false,
        None,
        &["REQ-02-056"],
        prefs_backups,
    );
    add::<PrefRestoreParams, BackupDiff>(
        r,
        "prefs.backup_diff",
        "Różnice kopii preferencji",
        "Porównuje kopię zapasową z bieżącymi preferencjami (klucze z kropkami, wartości w kopii i bieżące).",
        Read,
        false,
        None,
        &["REQ-02-056"],
        prefs_backup_diff,
    );
    add::<PrefRestoreParams, PrefValue>(
        r,
        "prefs.restore",
        "Przywróć preferencje",
        "Przywraca kopię zapasową (bieżące preferencje też trafiają do kopii).",
        Config,
        false,
        None,
        &["REQ-02-056"],
        prefs_restore,
    );
}
