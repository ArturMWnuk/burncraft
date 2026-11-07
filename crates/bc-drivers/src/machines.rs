//! Urządzenia (ADR 0010 §5): profile (`devices.json`), połączenia kilku laserów naraz,
//! urządzenie fikcyjne, kreator „Znajdź mój laser”.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use bc_doc::params::Kinematics;

use crate::driver::{Driver, DriverError, Setting, StreamMode};
use crate::emu::{EmuConfig, GrblEmulator};
use crate::grbl::{GrblDriver, GrblOptions};
use crate::transport::*;

/// Nazwa urządzenia fikcyjnego (zawsze dostępne, REQ-13-191).
pub const SIMULATOR: &str = "Symulator";

/// Profil urządzenia (REQ-13-196).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DeviceProfile {
    /// Nazwa (unikalna, `--device "Nazwa"`).
    pub name: String,
    /// Sterownik: grbl (rodzina GRBL i odmiany).
    #[serde(default = "d_driver")]
    pub driver: String,
    /// Odmiana z tabeli (pusta = rozpoznanie przy połączeniu).
    #[serde(default)]
    pub variant: Option<String>,
    pub transport: TransportSpec,
    /// Obszar roboczy [szerokość, wysokość], mm.
    #[serde(default = "d_area")]
    pub workspace: [f64; 2],
    #[serde(default)]
    pub stream: StreamMode,
    /// S dla 100 % mocy (brak = z odmiany).
    #[serde(default)]
    pub s_max: Option<f64>,
    /// Czerwony laser pozycjonujący: offset względem plamki lasera, mm (REQ-10-010).
    #[serde(default)]
    pub pointer_offset: [f64; 2],
    /// Polecenia włączenia / wyłączenia czerwonego lasera.
    #[serde(default)]
    pub pointer_on: String,
    #[serde(default)]
    pub pointer_off: String,
    /// Prędkość ramki, mm/s.
    #[serde(default = "d_frame_speed")]
    pub frame_speed: f64,
}

fn d_driver() -> String {
    "grbl".into()
}
fn d_area() -> [f64; 2] {
    [400.0, 400.0]
}
fn d_frame_speed() -> f64 {
    50.0
}

impl DeviceProfile {
    pub fn simulator() -> Self {
        DeviceProfile {
            name: SIMULATOR.into(),
            driver: "grbl".into(),
            variant: Some("grbl11".into()),
            transport: TransportSpec::Emulator {
                variant: "grbl11".into(),
                speed: Some(1.0),
            },
            workspace: d_area(),
            stream: StreamMode::CharacterCounting,
            s_max: None,
            pointer_offset: [0.0; 2],
            pointer_on: String::new(),
            pointer_off: String::new(),
            frame_speed: d_frame_speed(),
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.name.trim().is_empty() || self.name.len() > 80 {
            return Err("name: 1–80 znaków".into());
        }
        if self.driver != "grbl" {
            return Err(format!(
                "driver {}: w M2 dostępny grbl (Ruida, Lihuiyu — M3)",
                self.driver
            ));
        }
        if let Some(v) = &self.variant {
            if crate::grbl::codes::variant(v).is_none() {
                return Err(format!("variant {v}: nieznana odmiana"));
            }
        }
        if !self.workspace.iter().all(|v| *v > 0.0 && v.is_finite()) {
            return Err("workspace: > 0".into());
        }
        if let Some(s) = self.s_max {
            if !(s > 0.0 && s.is_finite()) {
                return Err("s_max: > 0".into());
            }
        }
        if !(self.frame_speed > 0.0 && self.frame_speed.is_finite()) {
            return Err("frame_speed: > 0".into());
        }
        if let TransportSpec::Emulator { variant, speed } = &self.transport {
            if crate::grbl::codes::variant(variant).is_none() {
                return Err(format!("transport.variant {variant}: nieznana odmiana"));
            }
            if speed.is_some_and(|s| s.is_nan() || s <= 0.0) {
                return Err("transport.speed: > 0".into());
            }
        }
        bc_doc::params::check_macro("pointer_on", &self.pointer_on)?;
        bc_doc::params::check_macro("pointer_off", &self.pointer_off)?;
        Ok(())
    }
}

/// Otwieranie transportów (wymienialne w testach).
pub trait Opener: Send + Sync {
    fn open(&self, spec: &TransportSpec) -> Result<Box<dyn Transport>, TransportError>;
    fn ports(&self) -> Vec<PortInfo>;
}

/// Otwieranie prawdziwych portów, sieci i emulatorów w procesie.
pub struct SystemOpener;

impl Opener for SystemOpener {
    fn open(&self, spec: &TransportSpec) -> Result<Box<dyn Transport>, TransportError> {
        Ok(match spec {
            TransportSpec::Serial { port, baud } => Box::new(SerialTransport::open(port, *baud)?),
            TransportSpec::Tcp { host, port } => Box::new(TcpTransport::connect(host, *port)?),
            TransportSpec::Websocket { url } => Box::new(WsTransport::connect(url)?),
            TransportSpec::Emulator { variant, speed } => {
                let mut cfg = EmuConfig::variant(variant);
                cfg.speed = speed.unwrap_or(f64::INFINITY);
                Box::new(EmuTransport::new(Arc::new(Mutex::new(GrblEmulator::new(
                    cfg,
                )))))
            }
        })
    }

    fn ports(&self) -> Vec<PortInfo> {
        list_serial_ports()
    }
}

/// Krok makra (REQ-01-040, 041): polecenie rejestru albo surowe polecenie sterownika.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum MacroStep {
    Command {
        id: String,
        #[serde(default)]
        params: serde_json::Value,
    },
    Raw {
        line: String,
    },
}

/// Makro: nazwana sekwencja kroków.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Macro {
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub steps: Vec<MacroStep>,
}

impl Macro {
    pub fn validate(&self) -> Result<(), String> {
        if self.name.trim().is_empty() || self.name.len() > 80 {
            return Err("name: 1–80 znaków".into());
        }
        if self.steps.is_empty() || self.steps.len() > 1000 {
            return Err("steps: 1–1000 kroków".into());
        }
        for (i, s) in self.steps.iter().enumerate() {
            match s {
                MacroStep::Command { id, .. } if id == "macro.run" => {
                    return Err(format!("steps[{i}]: makro nie może uruchamiać makr"))
                }
                MacroStep::Command { id, .. } if id.trim().is_empty() => {
                    return Err(format!("steps[{i}]: id polecenia"))
                }
                MacroStep::Raw { line }
                    if line.contains('\n') || line.trim().is_empty() || line.len() > 256 =>
                {
                    return Err(format!("steps[{i}]: jedno polecenie sterownika w wierszu"))
                }
                _ => {}
            }
        }
        Ok(())
    }
}

/// Zapamiętana pozycja XY z nazwą (okno Przesuń, REQ-03-039).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SavedPosition {
    pub name: String,
    /// mm, układ roboczy.
    pub x: f64,
    pub y: f64,
}

impl SavedPosition {
    pub fn validate(&self) -> Result<(), String> {
        if self.name.trim().is_empty() || self.name.chars().count() > 80 {
            return Err("name: 1–80 znaków".into());
        }
        if !(self.x.is_finite() && self.y.is_finite()) {
            return Err("x, y: liczby skończone".into());
        }
        Ok(())
    }
}

/// Wynik kreatora „Znajdź mój laser” (REQ-13-192).
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
pub struct Found {
    pub port: PortInfo,
    /// Znaleziony GRBL: prędkość portu i odmiana.
    pub baud: Option<u32>,
    pub variant: Option<String>,
    pub version: Option<String>,
    pub error: Option<String>,
    /// Gotowy profil do zapisu.
    pub profile: Option<DeviceProfile>,
}

/// Urządzenia: profile i połączenia (współdzielone przez zakładki).
pub struct Machines {
    profiles: Vec<DeviceProfile>,
    store: Option<PathBuf>,
    connected: BTreeMap<String, Box<dyn Driver>>,
    connected_profiles: BTreeMap<String, DeviceProfile>,
    opener: Arc<dyn Opener>,
    macros: Vec<Macro>,
    positions: Vec<SavedPosition>,
    /// Kinematyka podłączonych maszyn z `$110/$111/$120/$121/$11` (REQ-09-008).
    kinematics: BTreeMap<String, Kinematics>,
}

/// Ustawienia `$N` jako liczby.
pub fn settings_map(s: &[Setting]) -> BTreeMap<u32, f64> {
    s.iter()
        .filter_map(|x| {
            Some((
                x.id.trim_start_matches('$').parse().ok()?,
                x.value.parse().ok()?,
            ))
        })
        .collect()
}

/// Obszar roboczy z zakresów ruchu `$130/$131` (brak = `None`).
pub fn workspace_from_settings(m: &BTreeMap<u32, f64>) -> Option<[f64; 2]> {
    let w = *m.get(&130)?;
    let h = *m.get(&131)?;
    (w > 0.0 && h > 0.0 && w.is_finite() && h.is_finite()).then_some([w, h])
}

pub type MachinesHandle = Arc<Mutex<Machines>>;

impl Default for Machines {
    fn default() -> Self {
        Machines::new(Arc::new(SystemOpener))
    }
}

impl std::fmt::Debug for Machines {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Machines")
            .field("profiles", &self.profiles.len())
            .field("connected", &self.connected.keys().collect::<Vec<_>>())
            .finish()
    }
}

impl Machines {
    pub fn new(opener: Arc<dyn Opener>) -> Self {
        Machines {
            profiles: Vec::new(),
            store: None,
            connected: BTreeMap::new(),
            connected_profiles: BTreeMap::new(),
            opener,
            macros: Vec::new(),
            positions: Vec::new(),
            kinematics: BTreeMap::new(),
        }
    }

    pub fn handle(self) -> MachinesHandle {
        Arc::new(Mutex::new(self))
    }

    /// Katalog preferencji: profile w `devices.json`, kopie ustawień w `machine-backups/`.
    pub fn set_store(&mut self, dir: impl Into<PathBuf>) -> Result<(), String> {
        let dir = dir.into();
        let p = dir.join("devices.json");
        self.profiles = if p.exists() {
            let text = std::fs::read_to_string(&p).map_err(|e| format!("{}: {e}", p.display()))?;
            serde_json::from_str(&text).map_err(|e| format!("{}: {e}", p.display()))?
        } else {
            Vec::new()
        };
        let mp = dir.join("macros.json");
        self.macros = if mp.exists() {
            let text =
                std::fs::read_to_string(&mp).map_err(|e| format!("{}: {e}", mp.display()))?;
            serde_json::from_str(&text).map_err(|e| format!("{}: {e}", mp.display()))?
        } else {
            Vec::new()
        };
        let pp = dir.join("positions.json");
        self.positions = if pp.exists() {
            let text =
                std::fs::read_to_string(&pp).map_err(|e| format!("{}: {e}", pp.display()))?;
            serde_json::from_str(&text).map_err(|e| format!("{}: {e}", pp.display()))?
        } else {
            Vec::new()
        };
        self.store = Some(dir);
        Ok(())
    }

    /// Zapamiętane pozycje (`positions.json` w katalogu preferencji).
    pub fn positions(&self) -> &[SavedPosition] {
        &self.positions
    }

    /// Dodaje albo zastępuje pozycję o tej samej nazwie.
    pub fn save_position(&mut self, p: SavedPosition) -> Result<(), String> {
        p.validate()?;
        match self.positions.iter_mut().find(|x| x.name == p.name) {
            Some(x) => *x = p,
            None => self.positions.push(p),
        }
        self.save()
    }

    pub fn delete_position(&mut self, name: &str) -> Result<bool, String> {
        let n = self.positions.len();
        self.positions.retain(|p| p.name != name);
        let r = self.positions.len() != n;
        self.save()?;
        Ok(r)
    }

    pub fn macros(&self) -> &[Macro] {
        &self.macros
    }

    pub fn save_macro(&mut self, m: Macro) -> Result<(), String> {
        m.validate()?;
        match self.macros.iter_mut().find(|x| x.name == m.name) {
            Some(x) => *x = m,
            None => self.macros.push(m),
        }
        self.save()
    }

    pub fn delete_macro(&mut self, name: &str) -> Result<bool, String> {
        let n = self.macros.len();
        self.macros.retain(|m| m.name != name);
        let r = self.macros.len() != n;
        self.save()?;
        Ok(r)
    }

    pub fn backup_dir(&self) -> Option<PathBuf> {
        self.store.as_ref().map(|d| d.join("machine-backups"))
    }

    fn save(&self) -> Result<(), String> {
        if let Some(dir) = &self.store {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
            let p = dir.join("devices.json");
            std::fs::write(
                &p,
                serde_json::to_vec_pretty(&self.profiles).unwrap_or_default(),
            )
            .map_err(|e| format!("{}: {e}", p.display()))?;
            let mp = dir.join("macros.json");
            std::fs::write(
                &mp,
                serde_json::to_vec_pretty(&self.macros).unwrap_or_default(),
            )
            .map_err(|e| format!("{}: {e}", mp.display()))?;
            let pp = dir.join("positions.json");
            std::fs::write(
                &pp,
                serde_json::to_vec_pretty(&self.positions).unwrap_or_default(),
            )
            .map_err(|e| format!("{}: {e}", pp.display()))?;
        }
        Ok(())
    }

    /// Profile (z urządzeniem fikcyjnym na początku).
    pub fn profiles(&self) -> Vec<DeviceProfile> {
        let mut v = vec![DeviceProfile::simulator()];
        v.extend(
            self.profiles
                .iter()
                .filter(|p| p.name != SIMULATOR)
                .cloned(),
        );
        v
    }

    pub fn profile(&self, name: &str) -> Option<DeviceProfile> {
        self.profiles().into_iter().find(|p| p.name == name)
    }

    /// Dodaje albo zastępuje profil (REQ-13-196).
    pub fn save_profile(&mut self, p: DeviceProfile) -> Result<(), String> {
        p.validate()?;
        if p.name == SIMULATOR {
            return Err(format!(
                "{SIMULATOR}: nazwa zarezerwowana dla urządzenia fikcyjnego"
            ));
        }
        match self.profiles.iter_mut().find(|q| q.name == p.name) {
            Some(q) => *q = p,
            None => self.profiles.push(p),
        }
        self.save()
    }

    pub fn delete_profile(&mut self, name: &str) -> Result<bool, String> {
        let n = self.profiles.len();
        self.profiles.retain(|p| p.name != name);
        let removed = self.profiles.len() != n;
        self.save()?;
        Ok(removed)
    }

    /// Łączy profil z urządzeniem (kilka naraz, REQ-13-028).
    pub fn connect(&mut self, name: &str) -> Result<(), DriverError> {
        if self
            .connected
            .get(name)
            .is_some_and(|d| d.status().state != crate::driver::RunState::Disconnected)
        {
            return Ok(());
        }
        let p = self
            .profile(name)
            .ok_or_else(|| DriverError::Invalid(format!("nieznane urządzenie {name}")))?;
        let t = self
            .opener
            .open(&p.transport)
            .map_err(|e| DriverError::Transport(e.0))?;
        let d = GrblDriver::connect(
            &p.name,
            t,
            GrblOptions {
                stream: p.stream,
                s_max: p.s_max,
                variant: p.variant.clone(),
                ..Default::default()
            },
        )?;
        self.connected.insert(name.into(), Box::new(d));
        self.connected_profiles.insert(name.into(), p);
        self.refresh_kinematics(name);
        Ok(())
    }

    /// Odczyt kinematyki z ustawień podłączonej maszyny (po połączeniu i zapisie `$$`).
    pub fn refresh_kinematics(&mut self, name: &str) {
        let k = self
            .connected
            .get_mut(name)
            .and_then(|d| d.read_settings().ok())
            .map(|s| settings_map(&s))
            .filter(|m| m.contains_key(&110) || m.contains_key(&120))
            .map(|m| Kinematics::from_grbl(&m));
        match k {
            Some(k) => {
                self.kinematics.insert(name.into(), k);
            }
            None => {
                self.kinematics.remove(name);
            }
        }
    }

    /// Kinematyka podłączonej maszyny (`None` = niepodłączona albo bez ustawień GRBL).
    pub fn kinematics(&self, name: &str) -> Option<Kinematics> {
        self.connected
            .contains_key(name)
            .then(|| self.kinematics.get(name).copied())
            .flatten()
    }

    /// Dołącza gotowy sterownik (testy, emulator współdzielony).
    pub fn attach(&mut self, profile: DeviceProfile, d: Box<dyn Driver>) {
        self.connected_profiles
            .insert(profile.name.clone(), profile.clone());
        let name = profile.name.clone();
        self.connected.insert(profile.name, d);
        self.refresh_kinematics(&name);
    }

    pub fn disconnect(&mut self, name: &str) -> bool {
        self.connected_profiles.remove(name);
        self.kinematics.remove(name);
        match self.connected.remove(name) {
            Some(mut d) => {
                d.disconnect();
                true
            }
            None => false,
        }
    }

    pub fn connected(&self) -> Vec<String> {
        self.connected.keys().cloned().collect()
    }

    pub fn driver(&mut self, name: &str) -> Option<&mut Box<dyn Driver>> {
        self.connected.get_mut(name)
    }

    pub fn connected_profile(&self, name: &str) -> Option<&DeviceProfile> {
        self.connected_profiles.get(name)
    }

    /// Kreator „Znajdź mój laser”: porty z VID/PID i próba GRBL na znanych prędkościach.
    pub fn find(&self, probe: bool, bauds: &[u32]) -> Vec<Found> {
        self.opener
            .ports()
            .into_iter()
            .map(|port| {
                let mut f = Found {
                    port: port.clone(),
                    baud: None,
                    variant: None,
                    version: None,
                    error: None,
                    profile: None,
                };
                if !probe {
                    return f;
                }
                for &baud in bauds {
                    let spec = TransportSpec::Serial {
                        port: port.port.clone(),
                        baud,
                    };
                    match self.opener.open(&spec) {
                        Ok(t) => match GrblDriver::connect(&port.port, t, GrblOptions::default()) {
                            Ok(mut d) => {
                                let st = d.status();
                                // obszar roboczy z $130/$131 (zakresy ruchu maszyny)
                                let area = d
                                    .read_settings()
                                    .ok()
                                    .and_then(|s| workspace_from_settings(&settings_map(&s)));
                                d.disconnect();
                                f.baud = Some(baud);
                                f.variant = st.variant.clone();
                                f.version = st.version;
                                f.error = None;
                                let name = format!(
                                    "{} ({})",
                                    st.variant
                                        .as_deref()
                                        .and_then(crate::grbl::codes::variant)
                                        .map(|v| v.name)
                                        .unwrap_or("GRBL"),
                                    port.port
                                );
                                f.profile = Some(DeviceProfile {
                                    name,
                                    variant: st.variant,
                                    transport: spec,
                                    workspace: area.unwrap_or_else(d_area),
                                    ..DeviceProfile::simulator()
                                });
                                break;
                            }
                            Err(e) => f.error = Some(e.to_string()),
                        },
                        Err(e) => f.error = Some(e.0),
                    }
                }
                f
            })
            .collect()
    }
}
