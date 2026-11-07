//! Polecenia G-code (M2.2, ADR 0011): kompilacja, eksport, zapis pliku, import programu
//! (podgląd i uruchamianie), ustawienia kompilatora i ustawienia startu zadania.

use std::sync::Arc;

use bc_doc::{GcodeSettings, JobOrigin, Kind, StartFrom, StartSettings};
use bc_drivers::{CompileError, MachineFile};
use bc_plan::Move;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::plan::{run_plan, PlanJobParams};
use crate::{
    ok, parse, schema_of, CmdError, CommandSpec, Executor, Handler, Registry, SafetyTier, Session,
};

fn invalid(m: impl Into<String>) -> CmdError {
    CmdError::new("invalid_params", m)
}

/// Rozszerzenia plików G-code (import i eksport, §2.4).
pub const GCODE_EXTENSIONS: &[&str] = &["nc", "gc", "gcode"];

/// Czy ścieżka wskazuje plik G-code.
pub fn is_gcode_path(p: &std::path::Path) -> bool {
    p.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| GCODE_EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
}

/// Program G-code wczytany do sesji: podgląd i uruchamianie (wysyłka w M2.3).
#[derive(Debug, Clone)]
pub struct LoadedGcode {
    pub path: std::path::PathBuf,
    /// Treść programu bez zmian — strumień do maszyny.
    pub text: Arc<str>,
    /// Tor z interpretera (podgląd ścieżek, czas, długości).
    pub job: bc_plan::Job,
    pub report: bc_drivers::gcode::ImportReport,
}

fn compile_err(e: CompileError) -> CmdError {
    match e {
        CompileError::UnsupportedDriver(m) => CmdError::new(
            "unsupported_driver",
            format!("nieobsługiwany sterownik {m}"),
        ),
        CompileError::Invalid(m) => invalid(m),
        CompileError::Cancelled => CmdError::new("cancelled", "anulowano"),
    }
}

fn d_driver() -> String {
    "grbl".into()
}

// ---------- gcode.compile / gcode.export ----------

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct CompileParams {
    /// Sterownik: grbl (domyślnie) albo gcode (ogólny CNC).
    #[serde(default = "d_driver")]
    driver: String,
    /// Tylko zaznaczone obiekty.
    #[serde(default)]
    selection_only: bool,
    /// Zwróć treść pliku w polu text.
    #[serde(default)]
    include_text: bool,
    /// Od segmentu podglądu `plan.preview` („rozpocznij od tego miejsca”, REQ-09-013).
    #[serde(default)]
    from_segment: Option<usize>,
}

#[derive(Serialize, JsonSchema)]
struct WarningOut {
    code: String,
    message: String,
}

#[derive(Serialize, JsonSchema)]
struct CompileResult {
    driver: String,
    extension: String,
    lines: usize,
    bytes: usize,
    /// SHA-256 pliku.
    hash: String,
    /// Czas szacowany, s.
    time_s: f64,
    /// Ostrzeżenia kompilatora i planera.
    warnings: Vec<WarningOut>,
    #[serde(skip_serializing_if = "Option::is_none")]
    text: Option<String>,
}

/// Planuje i kompiluje zadanie dokumentu (wspólne dla poleceń, CLI i MCP).
pub fn compile_session(
    s: &mut Session,
    driver: &str,
    selection_only: bool,
) -> Result<(MachineFile, Vec<bc_plan::Warning>), CmdError> {
    compile_session_from(s, driver, selection_only, None)
}

/// Jak [`compile_session`], opcjonalnie od segmentu podglądu (REQ-09-013).
pub fn compile_session_from(
    s: &mut Session,
    driver: &str,
    selection_only: bool,
    from_segment: Option<usize>,
) -> Result<(MachineFile, Vec<bc_plan::Warning>), CmdError> {
    bc_drivers::compiler(driver).map_err(compile_err)?;
    let mut job = run_plan(
        s,
        &PlanJobParams {
            selection_only,
            ..Default::default()
        },
    )?;
    if let Some(n) = from_segment {
        job = crate::machine::from_segment_of(&job, n)?;
    }
    let ctx = crate::machine::context_of(s);
    let sess: &Session = s;
    let f = bc_drivers::compile_with_progress(driver, &job, &ctx, &mut |d, t| {
        sess.tick(d, t).map_err(|e| {
            if e.code == "cancelled" {
                CompileError::Cancelled
            } else {
                CompileError::Invalid(e.message)
            }
        })
    })
    .map_err(compile_err)?;
    Ok((f, job.warnings))
}

fn warnings_out(f: &MachineFile, plan: &[bc_plan::Warning]) -> Vec<WarningOut> {
    plan.iter()
        .map(|w| WarningOut {
            code: w.code.clone(),
            message: w.message.clone(),
        })
        .chain(f.warnings.iter().map(|w| WarningOut {
            code: w.code.clone(),
            message: w.message.clone(),
        }))
        .collect()
}

fn gcode_compile(s: &mut Session, p: Value) -> Result<Value, CmdError> {
    let p: CompileParams = parse(p)?;
    let (f, pw) = compile_session_from(s, &p.driver, p.selection_only, p.from_segment)?;
    ok(CompileResult {
        driver: f.driver.clone(),
        extension: f.extension.into(),
        lines: f.lines,
        bytes: f.bytes.len(),
        hash: f.hash.clone(),
        time_s: f.time_s,
        warnings: warnings_out(&f, &pw),
        text: p.include_text.then(|| f.text().to_string()),
    })
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct ExportParams {
    /// Plik docelowy (.gcode, .nc, .gc).
    path: String,
    #[serde(default = "d_driver")]
    driver: String,
    #[serde(default)]
    selection_only: bool,
}

#[derive(Serialize, JsonSchema)]
struct ExportResult {
    path: String,
    driver: String,
    lines: usize,
    bytes: usize,
    hash: String,
    time_s: f64,
    warnings: Vec<WarningOut>,
}

/// Zapis pliku G-code (REQ-10-019, REQ-02-090); używa go też `file.export`.
pub(crate) fn export_to(
    s: &mut Session,
    path: &str,
    driver: &str,
    selection_only: bool,
) -> Result<Value, CmdError> {
    let (f, pw) = compile_session(s, driver, selection_only)?;
    std::fs::write(path, &f.bytes).map_err(|e| CmdError::new("failed", format!("{path}: {e}")))?;
    ok(ExportResult {
        path: path.into(),
        driver: f.driver.clone(),
        lines: f.lines,
        bytes: f.bytes.len(),
        hash: f.hash.clone(),
        time_s: f.time_s,
        warnings: warnings_out(&f, &pw),
    })
}

fn gcode_export(s: &mut Session, p: Value) -> Result<Value, CmdError> {
    let p: ExportParams = parse(p)?;
    export_to(s, &p.path, &p.driver, p.selection_only)
}

// ---------- gcode.import ----------

fn yes() -> bool {
    true
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct ImportParams {
    /// Plik .nc, .gc albo .gcode.
    path: String,
    /// Dodaj tor z wiązką do dokumentu jako ścieżki (podgląd na kanwie).
    #[serde(default = "yes")]
    to_document: bool,
    /// Warstwa ścieżek (domyślnie 0).
    #[serde(default)]
    layer: u8,
    /// Wartość S dla 100 % mocy (domyślnie s_max z ustawień G-code).
    #[serde(default)]
    s_max: Option<f64>,
    /// Tryb laserowy GRBL (G0 bez wiązki); false = ogólny CNC.
    #[serde(default = "yes")]
    laser_mode: bool,
}

#[derive(Serialize, JsonSchema)]
struct GcodeImportResult {
    path: String,
    lines: usize,
    moves: usize,
    cut_length: f64,
    travel_length: f64,
    bbox: Option<[f64; 4]>,
    time_s: f64,
    formatted_time: String,
    pauses: Vec<String>,
    tools: Vec<u32>,
    dynamic_power: bool,
    air_assist: bool,
    /// Obiekty dodane do dokumentu.
    ids: Vec<u64>,
    warnings: Vec<String>,
}

/// Największa liczba odcinków dodawanych do dokumentu (większe programy: tylko podgląd).
const MAX_DOC_SEGMENTS: usize = 200_000;

/// Import programu G-code (REQ-02-089); używa go też `file.import`.
pub(crate) fn import_from(
    s: &mut Session,
    path: &str,
    to_document: bool,
    layer: u8,
    s_max: Option<f64>,
    laser_mode: bool,
) -> Result<GcodeImportOut, CmdError> {
    let s_max = s_max.unwrap_or(s.doc.gcode.s_max);
    if !(s_max > 0.0 && s_max.is_finite()) {
        return Err(invalid("s_max: > 0"));
    }
    if to_document {
        s.doc.layer(layer).map_err(|e| invalid(e.to_string()))?;
    }
    let text = std::fs::read_to_string(path)
        .map_err(|e| CmdError::new("failed", format!("{path}: {e}")))?;
    let imp = bc_drivers::gcode::import(
        &text,
        &bc_drivers::gcode::ImportOptions {
            sim: bc_drivers::gcode::sim::SimOptions { laser_mode, s_max },
            kinematics: s.doc.plan.kinematics,
        },
    )
    .map_err(|e| invalid(format!("{path}: {e}")))?;
    let mut warnings = imp.report.warnings.clone();
    let mut ids = Vec::new();
    if to_document {
        // łańcuchy odcinków z wiązką → ścieżki
        let mut chains: Vec<Vec<bc_geom::Point>> = Vec::new();
        let mut at = imp.job.start;
        let mut open = false;
        let mut n = 0usize;
        for (i, m) in imp.job.ops.iter().flat_map(|o| o.moves.iter()).enumerate() {
            s.tick(i, imp.report.moves)?;
            match *m {
                Move::Cut { to, power, .. } if power > 0.0 => {
                    if !open {
                        chains.push(vec![at]);
                        open = true;
                    }
                    chains.last_mut().expect("łańcuch").push(to);
                    n += 1;
                    at = to;
                }
                Move::Cut { to, .. } | Move::Travel { to, .. } => {
                    open = false;
                    at = to;
                }
                Move::Dwell { .. } => {}
            }
        }
        if n > MAX_DOC_SEGMENTS {
            warnings.push(format!(
                "program ma {n} odcinków z wiązką — za dużo dla dokumentu (limit {MAX_DOC_SEGMENTS}); dostępny podgląd ścieżek"
            ));
        } else {
            for c in chains.iter().filter(|c| c.len() >= 2) {
                let mut d = String::new();
                for (k, p) in c.iter().enumerate() {
                    use std::fmt::Write;
                    let _ = write!(
                        d,
                        "{}{} {} ",
                        if k == 0 { "M" } else { "L" },
                        fmt(p.x),
                        fmt(p.y)
                    );
                }
                if c.len() > 2 && c[0].distance(c[c.len() - 1]) < 1e-9 {
                    d.push('Z');
                }
                let id = s
                    .doc
                    .add(
                        layer,
                        Kind::Path {
                            d: d.trim_end().to_string(),
                        },
                    )
                    .map_err(|e| CmdError::new("failed", e.to_string()))?;
                ids.push(id);
            }
            s.selection = ids.clone();
        }
    }
    let r = imp.report.clone();
    s.gcode = Some(LoadedGcode {
        path: path.into(),
        text: text.into(),
        job: imp.job,
        report: imp.report,
    });
    Ok(GcodeImportOut {
        path: path.into(),
        r,
        ids,
        warnings,
    })
}

fn fmt(v: f64) -> String {
    let r = (v * 1e6).round() / 1e6;
    let r = if r == 0.0 { 0.0 } else { r };
    format!("{r}")
}

pub(crate) struct GcodeImportOut {
    path: String,
    r: bc_drivers::gcode::ImportReport,
    pub ids: Vec<u64>,
    pub warnings: Vec<String>,
}

fn gcode_import(s: &mut Session, p: Value) -> Result<Value, CmdError> {
    let p: ImportParams = parse(p)?;
    let o = import_from(s, &p.path, p.to_document, p.layer, p.s_max, p.laser_mode)?;
    ok(GcodeImportResult {
        path: o.path,
        lines: o.r.lines,
        moves: o.r.moves,
        cut_length: o.r.cut_length,
        travel_length: o.r.travel_length,
        bbox: o.r.bbox,
        time_s: o.r.time_s,
        formatted_time: o.r.formatted_time,
        pauses: o.r.pauses,
        tools: o.r.tools,
        dynamic_power: o.r.dynamic_power,
        air_assist: o.r.air_assist,
        ids: o.ids,
        warnings: o.warnings,
    })
}

// ---------- gcode.program ----------

#[derive(Deserialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
struct ProgramParams {
    /// Zwróć treść programu.
    #[serde(default)]
    include_text: bool,
}

#[derive(Serialize, JsonSchema)]
struct ProgramResult {
    /// Program wczytany (gotowy do uruchomienia).
    loaded: bool,
    path: Option<String>,
    lines: usize,
    moves: usize,
    time_s: f64,
    /// SHA-256 treści programu.
    hash: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    text: Option<String>,
}

fn gcode_program(s: &mut Session, p: Value) -> Result<Value, CmdError> {
    let p: ProgramParams = parse(if p.is_null() {
        serde_json::json!({})
    } else {
        p
    })?;
    ok(match &s.gcode {
        None => ProgramResult {
            loaded: false,
            path: None,
            lines: 0,
            moves: 0,
            time_s: 0.0,
            hash: None,
            text: None,
        },
        Some(g) => {
            use sha2::Digest;
            ProgramResult {
                loaded: true,
                path: Some(g.path.display().to_string()),
                lines: g.report.lines,
                moves: g.report.moves,
                time_s: g.report.time_s,
                hash: Some(
                    sha2::Sha256::digest(g.text.as_bytes())
                        .iter()
                        .map(|b| format!("{b:02x}"))
                        .collect(),
                ),
                text: p.include_text.then(|| g.text.to_string()),
            }
        }
    })
}

// ---------- ustawienia ----------

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct SettingsCmd {
    /// Zmieniane ustawienia kompilatora G-code; pozostałe bez zmian.
    #[serde(default)]
    settings: Option<Value>,
}

fn gcode_settings(s: &mut Session, p: Value) -> Result<Value, CmdError> {
    let p: SettingsCmd = parse(p)?;
    let mut v =
        serde_json::to_value(&s.doc.gcode).map_err(|e| CmdError::new("failed", e.to_string()))?;
    if let Some(patch) = &p.settings {
        if !patch.is_object() {
            return Err(invalid("settings: obiekt"));
        }
        crate::plan::merge(&mut v, patch);
    }
    let ns: GcodeSettings =
        serde_json::from_value(v).map_err(|e| invalid(format!("settings: {e}")))?;
    ns.validate()
        .map_err(|e| invalid(format!("settings: {e}")))?;
    s.doc.gcode = ns;
    ok(s.doc.gcode.clone())
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct StartCmd {
    /// absolute, current albo user_origin.
    #[serde(default)]
    from: Option<StartFrom>,
    /// 9-punktowy origin zadania.
    #[serde(default)]
    origin: Option<JobOrigin>,
}

fn start_settings(s: &mut Session, p: Value) -> Result<Value, CmdError> {
    let p: StartCmd = parse(p)?;
    if let Some(f) = p.from {
        s.doc.start.from = f;
    }
    if let Some(o) = p.origin {
        s.doc.start.origin = o;
    }
    ok(s.doc.start)
}

// ---------- rejestracja ----------

#[allow(clippy::too_many_arguments)]
fn reg(
    r: &mut Registry,
    id: &'static str,
    title: &'static str,
    description: &'static str,
    undoable: bool,
    params_schema: Value,
    result_schema: Value,
    requirements: &'static [&'static str],
    handler: Handler,
) {
    r.register(
        CommandSpec {
            executor: Executor::Document,
            id,
            title,
            description,
            params_schema,
            result_schema,
            undoable,
            requires_device: false,
            safety_tier: if undoable || id == "gcode.export" {
                SafetyTier::Design
            } else {
                SafetyTier::Read
            },
            shortcut: None,
            requirements,
        },
        handler,
    );
}

pub(crate) fn register_all(r: &mut Registry) {
    reg(
        r,
        "gcode.compile",
        "Kompiluj G-code",
        "Planuje zadanie i kompiluje je do G-code bez zapisu pliku (driver: grbl — tryb laserowy, M3/M4; gcode — ogólny CNC). Zwraca liczbę wierszy, bajty, SHA-256, czas szacowany, ostrzeżenia i opcjonalnie treść.",
        false,
        schema_of::<CompileParams>(),
        schema_of::<CompileResult>(),
        &[
            "REQ-13-015", "REQ-13-016", "REQ-13-085", "REQ-13-086", "REQ-13-087", "REQ-13-088", "REQ-13-089",
            "REQ-13-090",
        ],
        gcode_compile,
    );
    reg(
        r,
        "gcode.export",
        "Zapisz plik G-code",
        "Planuje i kompiluje zadanie, zapisuje plik G-code (.gcode, .nc, .gc) dla sterownika grbl albo gcode.",
        false,
        schema_of::<ExportParams>(),
        schema_of::<ExportResult>(),
        &["REQ-10-019", "REQ-02-090", "REQ-13-015", "REQ-15-057"],
        gcode_export,
    );
    reg(
        r,
        "gcode.import",
        "Importuj G-code",
        "Wczytuje program G-code (.nc, .gc, .gcode): wykonuje go interpreterem (G0/G1/G2/G3/G4, G20/G21, G90/G91, G92, M0/M2/M3/M4/M5/M7/M8/M9/M73, S/F/T), zwraca raport (długości, czas, pauzy, narzędzia, ostrzeżenia), zapamiętuje program do uruchomienia i podglądu ścieżek; to_document dodaje tor z wiązką jako ścieżki.",
        true,
        schema_of::<ImportParams>(),
        schema_of::<GcodeImportResult>(),
        &["REQ-02-089"],
        gcode_import,
    );
    reg(
        r,
        "gcode.program",
        "Wczytany program G-code",
        "Stan programu G-code wczytanego poleceniem gcode.import (do uruchomienia na urządzeniu): ścieżka, wiersze, ruchy, czas, SHA-256, opcjonalnie treść.",
        false,
        schema_of::<ProgramParams>(),
        schema_of::<ProgramResult>(),
        &["REQ-02-089"],
        gcode_program,
    );
    reg(
        r,
        "gcode.settings",
        "Ustawienia G-code",
        "Zmienia ustawienia kompilatora G-code dokumentu (zmiana częściowa): szablony nagłówka i stopki z polami {name} {driver} {time} {time_s} {units} {s_max} {passes} {x0} {y0} {x1} {y1}, jednostki i blok G20/G21, makra zadania job_pre/job_post, s_max, miejsca po przecinku, kompresja, M73, kod air assist (M7/M8), drugi laser (wybór T i offset), oś Z, powrót do startu. Makra warstw: layer.params gcode_pre/gcode_post.",
        true,
        crate::plan::with_inline(
            schema_of::<SettingsCmd>(),
            "settings",
            schema_of::<GcodeSettings>(),
        ),
        schema_of::<GcodeSettings>(),
        &["REQ-13-105", "REQ-13-106", "REQ-13-107", "REQ-13-108", "REQ-13-088", "REQ-13-089", "REQ-13-090"],
        gcode_settings,
    );
    reg(
        r,
        "job.start_settings",
        "Ustawienia startu",
        "Ustawia „Start From” zadania (absolute — współrzędne obszaru, current — od bieżącej pozycji głowicy, user_origin — od zera użytkownika) i 9-punktowy origin zadania. Ustawienia zapisują się w pliku .bcraft.",
        true,
        schema_of::<StartCmd>(),
        schema_of::<StartSettings>(),
        &["REQ-10-006"],
        start_settings,
    );
}
