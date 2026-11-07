//! Trait `Driver` z §13.1 i typy wspólne sterowników (ADR 0010).

use serde::Serialize;

use crate::{CompileError, Context, MachineFile};

/// Możliwości urządzenia (REQ-13-001).
#[derive(Debug, Clone, PartialEq, Serialize, schemars::JsonSchema)]
pub struct Caps {
    /// Osie, np. ["x", "y"] albo ["x", "y", "z"].
    pub axes: Vec<String>,
    pub rotary: bool,
    pub z: bool,
    /// Raster natywny w sterowniku (GRBL: nie — raster jako G-code).
    pub native_raster: bool,
    /// Pamięć plików / karta SD.
    pub files: bool,
    pub air_assist: bool,
    pub dual_laser: bool,
    pub camera: bool,
    pub probe: bool,
    /// Czujnik pokrywy / drzwi.
    pub lid_sensor: bool,
    /// Nadpisania czasu rzeczywistego (feed / moc).
    pub overrides: bool,
    /// Jog z anulowaniem (`$J=`).
    pub jog: bool,
}

/// Stan maszyny (REQ-13-003).
#[derive(Debug, Clone, PartialEq, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RunState {
    Disconnected,
    Idle,
    Run,
    Hold,
    Jog,
    Alarm,
    Door,
    Home,
    Check,
    Sleep,
    Unknown,
}

/// Migawka stanu maszyny.
#[derive(Debug, Clone, PartialEq, Serialize, schemars::JsonSchema)]
pub struct MachineState {
    pub state: RunState,
    /// Pozycja maszyny, mm.
    pub mpos: [f64; 3],
    /// Pozycja robocza (względem zera użytkownika), mm.
    pub wpos: [f64; 3],
    pub wco: [f64; 3],
    /// mm/min
    pub feed: f64,
    pub spindle: f64,
    /// Nadpisania: posuw, przejazdy, moc (%).
    pub overrides: [u32; 3],
    /// Wolne bloki planera i bajty bufora RX (raport `Bf:`).
    pub buffer: Option<[u32; 2]>,
    /// Pokrywa / drzwi otwarte (pin D).
    pub lid_open: Option<bool>,
    /// Chłodzenie (brak danych w GRBL).
    pub cooling: Option<bool>,
    /// Temperatura, °C (brak danych w GRBL).
    pub temperature: Option<f64>,
    /// Ostatni alarm (kod i opis).
    pub alarm: Option<(u32, String)>,
    /// Ostatnie błędy (kod i opis).
    pub errors: Vec<(u32, String)>,
    /// Ostatnia wiadomość `[MSG:…]`.
    pub message: Option<String>,
    /// Rozpoznana odmiana firmware.
    pub variant: Option<String>,
    pub version: Option<String>,
}

impl Default for MachineState {
    fn default() -> Self {
        MachineState {
            state: RunState::Disconnected,
            mpos: [0.0; 3],
            wpos: [0.0; 3],
            wco: [0.0; 3],
            feed: 0.0,
            spindle: 0.0,
            overrides: [100, 100, 100],
            buffer: None,
            lid_open: None,
            cooling: None,
            temperature: None,
            alarm: None,
            errors: Vec::new(),
            message: None,
            variant: None,
            version: None,
        }
    }
}

/// Stan zadania na urządzeniu.
#[derive(Debug, Clone, PartialEq, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum JobStatus {
    None,
    Running,
    Paused,
    Done,
    Stopped,
    Failed,
}

/// Postęp zadania.
#[derive(Debug, Clone, PartialEq, Serialize, schemars::JsonSchema)]
pub struct JobState {
    pub id: u64,
    pub name: String,
    pub status: JobStatus,
    /// Wiersze do wysłania.
    pub total: usize,
    pub sent: usize,
    /// Wiersze potwierdzone (ok/error).
    pub acked: usize,
    /// Powtórzenie (1..=repeat).
    pub round: u32,
    pub repeat: u32,
    /// Błędy: (wiersz, kod, opis).
    pub errors: Vec<(usize, u32, String)>,
    /// Ostrzeżenia, np. buffer_underrun (REQ-16-005).
    pub warnings: Vec<String>,
    /// Liczba wykrytych niedopełnień bufora.
    pub underruns: u32,
    /// Czas od startu, s.
    pub elapsed_s: f64,
    /// Zadania w kolejce po tym.
    pub queued: usize,
}

impl Default for JobState {
    fn default() -> Self {
        JobState {
            id: 0,
            name: String::new(),
            status: JobStatus::None,
            total: 0,
            sent: 0,
            acked: 0,
            round: 0,
            repeat: 0,
            errors: Vec::new(),
            warnings: Vec::new(),
            underruns: 0,
            elapsed_s: 0.0,
            queued: 0,
        }
    }
}

/// Tryb strumieniowania (REQ-13-075, 076).
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, serde::Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum StreamMode {
    /// Character counting (bufor RX 127 B).
    #[default]
    CharacterCounting,
    /// Jeden wiersz na `ok`.
    SendResponse,
}

/// Opcje wysyłki zadania.
#[derive(Debug, Clone, PartialEq)]
pub struct JobOptions {
    pub name: String,
    /// Powtórzenia (≥ 1, REQ-10-021).
    pub repeat: u32,
    /// Opóźnienie między powtórzeniami, s.
    pub delay_s: f64,
}

impl Default for JobOptions {
    fn default() -> Self {
        JobOptions {
            name: "zadanie".into(),
            repeat: 1,
            delay_s: 0.0,
        }
    }
}

/// Rodzaj ramki (REQ-10-007, 008).
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, serde::Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum FrameKind {
    /// Prostokąt obejmujący zadanie.
    Rect,
    /// Kontur (rubber band): otoczka wypukła zadania.
    Contour,
}

/// Oś jog.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, serde::Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "lowercase")]
pub enum Axis {
    X,
    Y,
    Z,
}

impl Axis {
    pub fn letter(self) -> char {
        match self {
            Axis::X => 'X',
            Axis::Y => 'Y',
            Axis::Z => 'Z',
        }
    }
}

/// Ustawienie maszyny z opisem (REQ-13-198).
#[derive(Debug, Clone, PartialEq, Serialize, schemars::JsonSchema)]
pub struct Setting {
    pub id: String,
    pub value: String,
    pub name: Option<String>,
    pub unit: Option<String>,
    pub min: Option<f64>,
    pub max: Option<f64>,
}

/// Zmiana ustawienia (diff).
#[derive(Debug, Clone, PartialEq, Serialize, schemars::JsonSchema)]
pub struct SettingChange {
    pub id: String,
    pub before: Option<String>,
    pub after: String,
}

/// Wynik zapisu ustawień.
#[derive(Debug, Clone, PartialEq, Serialize, schemars::JsonSchema)]
pub struct WriteReport {
    pub changes: Vec<SettingChange>,
    /// Plik kopii zapasowej (przed zapisem).
    pub backup: Option<String>,
    pub dry_run: bool,
}

/// Plik w pamięci sterownika.
#[derive(Debug, Clone, PartialEq, Serialize, schemars::JsonSchema)]
pub struct RemoteFile {
    pub name: String,
    pub size: u64,
}

/// Błąd sterownika.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum DriverError {
    #[error("brak połączenia z urządzeniem")]
    NotConnected,
    #[error("transport: {0}")]
    Transport(String),
    #[error("error:{0} — {1}")]
    Machine(u32, String),
    #[error("ALARM:{0} — {1}")]
    Alarm(u32, String),
    #[error("{0}")]
    Busy(String),
    #[error("{0}")]
    Unsupported(String),
    #[error("{0}")]
    Invalid(String),
    #[error("przekroczony czas oczekiwania: {0}")]
    Timeout(String),
}

impl From<CompileError> for DriverError {
    fn from(e: CompileError) -> Self {
        DriverError::Invalid(e.to_string())
    }
}

/// Sterownik lasera (§13.1). Operacje ruchu kończą się po przyjęciu polecenia
/// przez sterownik (bez czekania na koniec ruchu); `stop` jest zawsze dozwolony.
pub trait Driver: Send {
    fn name(&self) -> &str;
    fn capabilities(&self) -> Caps;
    fn status(&self) -> MachineState;
    fn disconnect(&mut self);
    fn home(&mut self) -> Result<(), DriverError>;
    fn unlock(&mut self) -> Result<(), DriverError>;
    fn jog(&mut self, axis: Axis, distance: f64, speed: f64) -> Result<(), DriverError>;
    fn jog_cancel(&mut self) -> Result<(), DriverError>;
    /// Ruch do punktu (układ roboczy), mm, mm/s (`None` = przejazd szybki).
    fn move_to(
        &mut self,
        x: Option<f64>,
        y: Option<f64>,
        z: Option<f64>,
        speed: Option<f64>,
    ) -> Result<(), DriverError>;
    /// Zero użytkownika w bieżącej pozycji (REQ-10-012).
    fn set_origin(&mut self, x: bool, y: bool, z: bool) -> Result<(), DriverError>;
    /// Ramka po punktach (układ roboczy), mm/s; `pointer` = czerwony laser (polecenia z profilu).
    fn frame(
        &mut self,
        points: &[[f64; 2]],
        speed: f64,
        pointer: Option<(&str, &str, [f64; 2])>,
    ) -> Result<(), DriverError>;
    /// Krótki impuls lasera (poziom `fire`).
    fn test_fire(&mut self, power_pct: f64, ms: u32) -> Result<(), DriverError>;
    fn send_job(&mut self, file: &MachineFile, opts: JobOptions) -> Result<JobState, DriverError>;
    /// Dodaje zadanie do kolejki (REQ-10-020).
    fn queue_job(&mut self, file: &MachineFile, opts: JobOptions) -> Result<usize, DriverError>;
    fn job(&self) -> JobState;
    fn pause(&mut self) -> Result<(), DriverError>;
    fn resume(&mut self) -> Result<(), DriverError>;
    /// Natychmiastowe zatrzymanie (wstrzymanie + reset); zawsze dozwolone.
    fn stop(&mut self);
    fn set_overrides(&mut self, feed: Option<u32>, power: Option<u32>) -> Result<(), DriverError>;
    fn read_settings(&mut self) -> Result<Vec<Setting>, DriverError>;
    fn write_settings(
        &mut self,
        changes: &[(String, String)],
        backup_dir: Option<&std::path::Path>,
        dry_run: bool,
    ) -> Result<WriteReport, DriverError>;
    /// Sonda Z (G38.2); wynik: Z kontaktu (układ roboczy).
    fn probe_z(&mut self, distance: f64, feed: f64) -> Result<f64, DriverError>;
    fn list_files(&mut self) -> Result<Vec<RemoteFile>, DriverError>;
    fn upload_file(&mut self, name: &str, data: &[u8]) -> Result<(), DriverError>;
    fn run_file(&mut self, name: &str) -> Result<(), DriverError>;
    fn delete_file(&mut self, name: &str) -> Result<(), DriverError>;
    /// Surowe polecenie; zwraca odpowiedź (wiersze przed `ok`).
    fn raw(&mut self, line: &str) -> Result<Vec<String>, DriverError>;
    /// Ostatnie wiersze konsoli (wysłane `>` i odebrane `<`).
    fn console(&self) -> Vec<String>;
    /// Kompilacja offline (REQ-13-015).
    fn compile(&self, job: &bc_plan::Job, ctx: &Context) -> Result<MachineFile, DriverError>;
}
