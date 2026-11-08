//! bc-drivers — sterowniki laserów (moduł z REQ-01-010).
//!
//! M2.2: kompilatory plików maszynowych niezależne od transportu (ADR 0011):
//! `compile(driver, &Job, &Context)` jest czystą funkcją zadania i ustawień, a jej
//! wynik (`MachineFile`) to zarówno plik do zapisu, jak i strumień wierszy do maszyny.
//! Trait `Driver`, transporty i emulator GRBL dochodzą w M2.3 (ADR 0010).

pub mod ch341;
pub mod driver;
pub mod emu;
pub mod gcode;
pub mod grbl;
pub mod lihuiyu;
pub mod machines;
pub(crate) mod md5;
pub mod moshi;
pub mod transport;
pub mod usb;
pub mod xmodem;

pub use driver::{Driver, DriverError};
pub use machines::{DeviceProfile, Machines, MachinesHandle};

use std::collections::BTreeMap;

use bc_doc::{Document, GcodeSettings, Kinematics, StartSettings};
use bc_plan::Job;
use serde::Serialize;
use sha2::{Digest, Sha256};

/// Nazwa modułu, używana przez `burncraft-cli version`.
pub const MODULE: &str = "bc-drivers";

/// Sterowniki z kompilatorem w tej wersji.
pub const DRIVERS: &[&str] = &["grbl", "gcode", "lihuiyu", "moshi"];

/// Sterowniki planowane w kolejnych etapach (nazwa, etap).
pub const PLANNED_DRIVERS: &[(&str, &str)] = &[
    ("ruida", "M3"),
    ("trocen", "M3"),
    ("marlin", "M3"),
    ("smoothie", "M3"),
    ("bjjcz", "M5"),
    ("ezcad3", "M5"),
];

/// Ostrzeżenie kompilatora.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CompileWarning {
    /// laser2_disabled, z_ignored, unknown_placeholder
    pub code: String,
    pub message: String,
}

impl CompileWarning {
    pub(crate) fn new(code: &str, message: impl Into<String>) -> Self {
        CompileWarning {
            code: code.into(),
            message: message.into(),
        }
    }
}

/// Wynik kompilacji: plik dla sterownika (te same wiersze idą do maszyny).
#[derive(Debug, Clone, PartialEq)]
pub struct MachineFile {
    pub driver: String,
    /// Domyślne rozszerzenie pliku.
    pub extension: &'static str,
    pub bytes: Vec<u8>,
    pub lines: usize,
    /// SHA-256 bajtów (hex).
    pub hash: String,
    /// Czas szacowany, s (model przyspieszeń planera).
    pub time_s: f64,
    pub warnings: Vec<CompileWarning>,
}

impl MachineFile {
    pub(crate) fn new(
        driver: &str,
        extension: &'static str,
        lines: Vec<String>,
        time_s: f64,
        warnings: Vec<CompileWarning>,
    ) -> Self {
        let mut text = lines.join("\n");
        text.push('\n');
        let bytes = text.into_bytes();
        let hash = Sha256::digest(&bytes)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        MachineFile {
            driver: driver.into(),
            extension,
            lines: lines.len(),
            bytes,
            hash,
            time_s,
            warnings,
        }
    }

    /// Treść tekstowa (G-code jest w ASCII).
    /// Plik binarny albo strumień bez podziału na wiersze G-code (EGV, Moshi);
    /// `lines` = liczba znaków nowej linii (EGV) albo programów (Moshi).
    pub(crate) fn from_bytes(
        driver: &str,
        extension: &'static str,
        bytes: Vec<u8>,
        time_s: f64,
        warnings: Vec<CompileWarning>,
    ) -> Self {
        let hash = Sha256::digest(&bytes)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        let lines = if driver == "moshi" {
            moshi::proto::programs(&bytes).map_or(0, |p| p.len())
        } else {
            bytes.iter().filter(|b| **b == b'\n').count()
        };
        MachineFile {
            driver: driver.into(),
            extension,
            lines,
            bytes,
            hash,
            time_s,
            warnings,
        }
    }

    pub fn text(&self) -> &str {
        std::str::from_utf8(&self.bytes).unwrap_or_default()
    }

    /// Wiersze do strumieniowania (bez znaku nowej linii).
    pub fn stream_lines(&self) -> impl Iterator<Item = &str> {
        self.text().lines()
    }
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum CompileError {
    #[error("nieobsługiwany sterownik {0}")]
    UnsupportedDriver(String),
    #[error("{0}")]
    Invalid(String),
    #[error("anulowano")]
    Cancelled,
}

/// Wszystko poza zadaniem, czego potrzebuje kompilator (ADR 0011 §1).
#[derive(Debug, Clone, PartialEq)]
pub struct Context {
    /// Nazwa zadania (pole `{name}` szablonów).
    pub name: String,
    pub settings: GcodeSettings,
    pub start: StartSettings,
    /// Kinematyka do czasu szacowanego (M73, `{time}`).
    pub kinematics: Kinematics,
    /// Makra warstw: indeks → (pre, post) (REQ-13-107).
    pub layer_macros: BTreeMap<u8, (String, String)>,
    /// Płyta K40 (Lihuiyu, Moshi): wariant, kroki/mm, korekcja skali (REQ-13-046, 047).
    pub k40: lihuiyu::K40Settings,
    /// Obszar roboczy maszyny [szerokość, wysokość], mm — K40 bazuje w lewym górnym rogu.
    pub bed: [f64; 2],
}

impl Default for Context {
    fn default() -> Self {
        Context {
            name: String::new(),
            settings: GcodeSettings::default(),
            start: StartSettings::default(),
            kinematics: Kinematics::default(),
            layer_macros: BTreeMap::new(),
            k40: lihuiyu::K40Settings::default(),
            bed: lihuiyu::BED,
        }
    }
}

impl Context {
    pub fn from_document(doc: &Document) -> Self {
        Context {
            k40: lihuiyu::K40Settings::default(),
            bed: [doc.workspace.width, doc.workspace.height],
            name: doc.name.clone(),
            settings: doc.gcode.clone(),
            start: doc.start,
            kinematics: doc.plan.kinematics,
            layer_macros: doc
                .layers
                .iter()
                .filter(|l| !l.params.gcode_pre.is_empty() || !l.params.gcode_post.is_empty())
                .map(|l| {
                    (
                        l.index,
                        (l.params.gcode_pre.clone(), l.params.gcode_post.clone()),
                    )
                })
                .collect(),
        }
    }
}

/// Postęp kompilacji: (zrobione, razem) → `Err(Cancelled)` przerywa.
pub type ProgressFn<'a> = dyn FnMut(usize, usize) -> Result<(), CompileError> + 'a;

/// Kompilator zadania dla rodziny sterowników.
pub trait JobCompiler {
    fn name(&self) -> &'static str;
    fn extension(&self) -> &'static str;
    fn compile(
        &self,
        job: &Job,
        ctx: &Context,
        progress: &mut ProgressFn,
    ) -> Result<MachineFile, CompileError>;
}

/// Kompilator dla nazwy sterownika.
pub fn compiler(driver: &str) -> Result<Box<dyn JobCompiler>, CompileError> {
    match driver {
        "grbl" => Ok(Box::new(gcode::GcodeCompiler::new(gcode::Dialect::Grbl))),
        "gcode" => Ok(Box::new(gcode::GcodeCompiler::new(gcode::Dialect::Generic))),
        "lihuiyu" => Ok(Box::new(lihuiyu::egv::EgvCompiler)),
        "moshi" => Ok(Box::new(moshi::proto::MoshiCompiler)),
        other => Err(CompileError::UnsupportedDriver(
            match PLANNED_DRIVERS.iter().find(|(n, _)| *n == other) {
                Some((n, ms)) => format!(
                    "{n} (kompilator w etapie {ms}; dostępne: {})",
                    DRIVERS.join(", ")
                ),
                None => format!("{other} (dostępne: {})", DRIVERS.join(", ")),
            },
        )),
    }
}

/// Kompiluje zadanie dla sterownika (REQ-13-015).
pub fn compile(driver: &str, job: &Job, ctx: &Context) -> Result<MachineFile, CompileError> {
    compile_with_progress(driver, job, ctx, &mut |_, _| Ok(()))
}

/// Kompiluje zadanie, zgłaszając postęp i sprawdzając anulowanie.
pub fn compile_with_progress(
    driver: &str,
    job: &Job,
    ctx: &Context,
    progress: &mut ProgressFn,
) -> Result<MachineFile, CompileError> {
    ctx.settings.validate().map_err(CompileError::Invalid)?;
    compiler(driver)?.compile(job, ctx, progress)
}
