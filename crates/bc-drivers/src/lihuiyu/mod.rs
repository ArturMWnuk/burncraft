//! Sterownik K40 Lihuiyu (ADR 0014): płyty M2 Nano, M3 Nano, M1, M, B1, B2, B, A,
//! 6C6879-LASER-M2/M3 (REQ-13-030…038), protokół EGV przez CH341 (REQ-13-039…048).
//! Implementuje trait `Driver` z ADR 0010 bez zmiany interfejsu.
//! Szkielet API (M3.3, testy red): warianty płyt i ustawienia, sterownik bez implementacji.

pub mod egv;
pub mod emu;

use std::time::Duration;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::ch341::Ch341;
use crate::driver::*;
use crate::{Context, MachineFile};

/// Wariant płyty Lihuiyu (REQ-13-030…038, REQ-13-046).
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
pub enum Board {
    #[serde(rename = "m2")]
    #[default]
    M2,
    #[serde(rename = "m3")]
    M3,
    #[serde(rename = "m1")]
    M1,
    #[serde(rename = "m")]
    M,
    #[serde(rename = "b1")]
    B1,
    #[serde(rename = "b2")]
    B2,
    #[serde(rename = "b")]
    B,
    #[serde(rename = "a")]
    A,
    /// 6C6879-LASER-M2 (rewizja płyty M2).
    #[serde(rename = "6c6879_m2")]
    L6c6879M2,
    /// 6C6879-LASER-M3 (rewizja płyty M3).
    #[serde(rename = "6c6879_m3")]
    L6c6879M3,
}

/// Wszystkie warianty (kolejność listy w GUI).
pub const BOARDS: &[Board] = &[
    Board::M2,
    Board::M3,
    Board::M1,
    Board::M,
    Board::B1,
    Board::B2,
    Board::B,
    Board::A,
    Board::L6c6879M2,
    Board::L6c6879M3,
];

/// Opis wariantu płyty (lista w GUI i `machine.list`).
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
pub struct BoardInfo {
    pub id: &'static str,
    pub name: &'static str,
    /// Regulacja mocy PWM z programu.
    pub pwm: bool,
}

impl Board {
    pub fn id(self) -> &'static str {
        match self {
            Board::M2 => "m2",
            Board::M3 => "m3",
            Board::M1 => "m1",
            Board::M => "m",
            Board::B1 => "b1",
            Board::B2 => "b2",
            Board::B => "b",
            Board::A => "a",
            Board::L6c6879M2 => "6c6879_m2",
            Board::L6c6879M3 => "6c6879_m3",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Board::M2 => "M2 Nano",
            Board::M3 => "M3 Nano",
            Board::M1 => "M1",
            Board::M => "M",
            Board::B1 => "B1",
            Board::B2 => "B2",
            Board::B => "B",
            Board::A => "A",
            Board::L6c6879M2 => "6C6879-LASER-M2",
            Board::L6c6879M3 => "6C6879-LASER-M3",
        }
    }

    pub fn from_id(id: &str) -> Option<Board> {
        BOARDS.iter().copied().find(|b| b.id() == id)
    }

    pub fn info(self) -> BoardInfo {
        BoardInfo {
            id: self.id(),
            name: self.name(),
            pwm: self.pwm(),
        }
    }

    /// Grupa równania prędkości. Źródło: MeerK40t `laserspeed.py` (opis).
    pub fn group(self) -> egv::Group {
        match self {
            Board::A | Board::B | Board::B1 => egv::Group::Ab,
            Board::B2 => egv::Group::B2,
            _ => egv::Group::Default,
        }
    }

    /// Człon przekątnej w kodzie prędkości (brak dla A, B, M). Źródło: MeerK40t.
    pub fn diagonal(self) -> bool {
        !matches!(self, Board::A | Board::B | Board::M)
    }

    /// Kod z sufiksem C poniżej 7 mm/s (M2, M3, B2). Źródło: MeerK40t `laserspeed.py`.
    pub fn suffix_c(self) -> bool {
        matches!(
            self,
            Board::M2 | Board::M3 | Board::B2 | Board::L6c6879M2 | Board::L6c6879M3
        )
    }

    /// PWM mocy z programu: tylko M3 Nano (firmware 2024.01.18g+). Źródło: README MeerK40t.
    /// UNVERIFIED: wersja firmware płyty M3 nie jest sprawdzana.
    pub fn pwm(self) -> bool {
        matches!(self, Board::M3 | Board::L6c6879M3)
    }
}

/// Ustawienia płyty K40 w profilu urządzenia (REQ-13-046, 047).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct K40Settings {
    /// Wariant płyty Lihuiyu (Moshi: pomijany).
    #[serde(default)]
    pub board: Board,
    /// ID płyty (numer seryjny z naklejki); sprawdzany przy połączeniu, gdy podany.
    #[serde(default)]
    pub board_id: Option<String>,
    /// Kroki na cal (silniki 1000 dpi). Źródło: MeerK40t (1000 dpi).
    #[serde(default = "d_dpi")]
    pub dpi: f64,
    /// Korekcja skali X, Y (1 = bez korekcji).
    #[serde(default = "d_scale")]
    pub scale: [f64; 2],
}

fn d_dpi() -> f64 {
    1000.0
}
fn d_scale() -> [f64; 2] {
    [1.0, 1.0]
}

impl Default for K40Settings {
    fn default() -> Self {
        K40Settings {
            board: Board::M2,
            board_id: None,
            dpi: d_dpi(),
            scale: d_scale(),
        }
    }
}

impl K40Settings {
    pub fn validate(&self) -> Result<(), String> {
        if !(self.dpi >= 100.0 && self.dpi <= 10000.0) {
            return Err("k40.dpi: 100–10000".into());
        }
        if !self.scale.iter().all(|s| *s >= 0.5 && *s <= 2.0) {
            return Err("k40.scale: 0,5–2".into());
        }
        if let Some(id) = &self.board_id {
            if id.trim().is_empty() || id.len() > 64 || !id.is_ascii() {
                return Err("k40.board_id: 1–64 znaki ASCII".into());
            }
        }
        Ok(())
    }
}

/// Stół K40 (mm). Źródło: README MeerK40t `lihuiyu` (310 × 210 mm).
pub const BED: [f64; 2] = [310.0, 210.0];

/// Opcje połączenia.
#[derive(Debug, Clone)]
pub struct LihuiyuOptions {
    pub settings: K40Settings,
    /// Obszar roboczy (mm) — narożnik bazowania to lewy górny róg.
    pub bed: [f64; 2],
}

impl Default for LihuiyuOptions {
    fn default() -> Self {
        LihuiyuOptions {
            settings: K40Settings::default(),
            bed: BED,
        }
    }
}

fn todo() -> DriverError {
    DriverError::Unsupported("lihuiyu: niezaimplementowane (M3.3)".into())
}

/// Sterownik Lihuiyu.
pub struct LihuiyuDriver {
    name: String,
    opts: LihuiyuOptions,
}

impl LihuiyuDriver {
    pub fn connect(
        _name: &str,
        _dev: Box<dyn Ch341>,
        _opts: LihuiyuOptions,
    ) -> Result<Self, DriverError> {
        Err(todo())
    }
    pub fn settings(&self) -> &K40Settings {
        &self.opts.settings
    }
    pub fn wait_job(&self, _timeout: Duration) -> JobState {
        JobState::default()
    }
}

crate::k40_driver_stub!(LihuiyuDriver);
/// Implementacja `Driver` zwracająca „niezaimplementowane” (szkielety M3.3, testy red).
#[macro_export]
#[doc(hidden)]
macro_rules! k40_driver_stub {
    ($t:ty) => {
        impl $crate::driver::Driver for $t {
            fn name(&self) -> &str {
                &self.name
            }
            fn capabilities(&self) -> Caps {
                Caps {
                    axes: Vec::new(),
                    rotary: false,
                    z: false,
                    native_raster: false,
                    files: false,
                    air_assist: false,
                    dual_laser: false,
                    camera: false,
                    probe: false,
                    lid_sensor: false,
                    overrides: false,
                    jog: false,
                    pwm: true,
                }
            }
            fn status(&self) -> MachineState {
                MachineState::default()
            }
            fn disconnect(&mut self) {}
            fn home(&mut self) -> Result<(), DriverError> {
                Err(todo())
            }
            fn unlock(&mut self) -> Result<(), DriverError> {
                Err(todo())
            }
            fn jog(&mut self, _a: Axis, _d: f64, _s: f64) -> Result<(), DriverError> {
                Err(todo())
            }
            fn jog_cancel(&mut self) -> Result<(), DriverError> {
                Err(todo())
            }
            fn move_to(
                &mut self,
                _x: Option<f64>,
                _y: Option<f64>,
                _z: Option<f64>,
                _s: Option<f64>,
            ) -> Result<(), DriverError> {
                Err(todo())
            }
            fn set_origin(&mut self, _x: bool, _y: bool, _z: bool) -> Result<(), DriverError> {
                Err(todo())
            }
            fn frame(
                &mut self,
                _p: &[[f64; 2]],
                _s: f64,
                _q: Option<(&str, &str, [f64; 2])>,
            ) -> Result<(), DriverError> {
                Err(todo())
            }
            fn test_fire(&mut self, _p: f64, _ms: u32) -> Result<(), DriverError> {
                Err(todo())
            }
            fn send_job(
                &mut self,
                _f: &MachineFile,
                _o: JobOptions,
            ) -> Result<JobState, DriverError> {
                Err(todo())
            }
            fn queue_job(
                &mut self,
                _f: &MachineFile,
                _o: JobOptions,
            ) -> Result<usize, DriverError> {
                Err(todo())
            }
            fn job(&self) -> JobState {
                JobState::default()
            }
            fn pause(&mut self) -> Result<(), DriverError> {
                Err(todo())
            }
            fn resume(&mut self) -> Result<(), DriverError> {
                Err(todo())
            }
            fn stop(&mut self) {}
            fn set_overrides(
                &mut self,
                _f: Option<u32>,
                _p: Option<u32>,
            ) -> Result<(), DriverError> {
                Err(todo())
            }
            fn read_settings(&mut self) -> Result<Vec<Setting>, DriverError> {
                Err(todo())
            }
            fn write_settings(
                &mut self,
                _c: &[(String, String)],
                _b: Option<&std::path::Path>,
                _d: bool,
            ) -> Result<WriteReport, DriverError> {
                Err(todo())
            }
            fn probe_z(&mut self, _d: f64, _f: f64) -> Result<f64, DriverError> {
                Err(todo())
            }
            fn list_files(&mut self) -> Result<Vec<RemoteFile>, DriverError> {
                Err(todo())
            }
            fn upload_file(&mut self, _n: &str, _d: &[u8]) -> Result<(), DriverError> {
                Err(todo())
            }
            fn run_file(&mut self, _n: &str) -> Result<(), DriverError> {
                Err(todo())
            }
            fn delete_file(&mut self, _n: &str) -> Result<(), DriverError> {
                Err(todo())
            }
            fn raw(&mut self, _l: &str) -> Result<Vec<String>, DriverError> {
                Err(todo())
            }
            fn console(&self) -> Vec<String> {
                Vec::new()
            }
            fn compile(&self, _j: &bc_plan::Job, _c: &Context) -> Result<MachineFile, DriverError> {
                Err(todo())
            }
        }
    };
}
