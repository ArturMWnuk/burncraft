//! Transport USB bulk (REQ-13-019, ADR 0013): lista urządzeń z VID/PID, rozpoznanie rodzin
//! laserów (kreator „Znajdź mój laser”, REQ-13-193), stan dostępu (sterownik WinUSB na
//! Windows, uprawnienia udev na Linuksie) z przewodnikiem instalacji (REQ-13-020, 021).
//! Biblioteka: `nusb` (czysty Rust). Urządzenie fikcyjne [`FakeUsb`] zastępuje sprzęt
//! w testach i przy `BURNCRAFT_FAKE_USB=1`.

use std::time::Duration;

use serde::Serialize;

use crate::transport::{Transport, TransportError};

/// Rodzina laserów rozpoznawana po VID/PID.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, schemars::JsonSchema)]
pub struct UsbFamily {
    pub id: &'static str,
    pub name: &'static str,
    pub vid: u16,
    pub pid: u16,
    /// Sterownik BurnCraft dla tej rodziny i etap, w którym powstaje.
    pub driver: &'static str,
    pub milestone: &'static str,
    /// Interfejs i punkty końcowe bulk (OUT, IN).
    pub interface: u8,
    pub ep_out: u8,
    pub ep_in: u8,
}

/// Znane rodziny laserów USB.
pub const USB_FAMILIES: &[UsbFamily] = &[];

pub fn usb_family(_vid: u16, _pid: u16) -> Option<&'static UsbFamily> {
    None
}

/// Czy program może otworzyć urządzenie.
#[derive(Debug, Clone, PartialEq, Serialize, schemars::JsonSchema)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum UsbAccess {
    /// Gotowe do użycia (WinUSB na Windows, uprawnienia na Linuksie).
    Ready,
    /// Windows: urządzenie bez sterownika.
    DriverMissing,
    /// Windows: sterownik producenta (np. CH341) zamiast WinUSB.
    VendorDriver { driver: String },
    /// Linux: brak uprawnień (reguły udev).
    PermissionDenied,
    /// Używane przez inny program.
    Busy,
    /// Nie sprawdzono.
    Unknown,
}

impl UsbAccess {
    /// Stan z nazwy sterownika urządzenia na Windows.
    pub fn from_windows_driver(_driver: Option<&str>) -> UsbAccess {
        UsbAccess::Unknown
    }
}

/// Urządzenie USB w systemie.
#[derive(Debug, Clone, PartialEq, Serialize, schemars::JsonSchema)]
pub struct UsbDeviceInfo {
    pub vid: u16,
    pub pid: u16,
    pub bus: String,
    pub address: u8,
    pub serial: Option<String>,
    pub manufacturer: Option<String>,
    pub product: Option<String>,
    /// Windows: nazwa sterownika urządzenia.
    pub driver: Option<String>,
    pub access: UsbAccess,
}

/// Wybór urządzenia i punktów końcowych.
#[derive(Debug, Clone, PartialEq)]
pub struct UsbSelector {
    pub vid: u16,
    pub pid: u16,
    pub serial: Option<String>,
    pub interface: u8,
    pub ep_out: u8,
    pub ep_in: u8,
}

/// Otwarte urządzenie: transfery bulk z limitem czasu.
pub trait UsbBulk: Send {
    fn write(&mut self, data: &[u8], timeout: Duration) -> Result<usize, TransportError>;
    /// 0 bajtów = brak danych w czasie `timeout`.
    fn read(&mut self, buf: &mut [u8], timeout: Duration) -> Result<usize, TransportError>;
}

/// Dostęp do USB (system albo urządzenie fikcyjne).
pub trait UsbBackend: Send + Sync {
    fn list(&self) -> Vec<UsbDeviceInfo>;
    fn open(&self, sel: &UsbSelector) -> Result<Box<dyn UsbBulk>, TransportError>;
}

/// Brak USB.
pub struct NoUsb;

impl UsbBackend for NoUsb {
    fn list(&self) -> Vec<UsbDeviceInfo> {
        Vec::new()
    }
    fn open(&self, sel: &UsbSelector) -> Result<Box<dyn UsbBulk>, TransportError> {
        Err(TransportError(format!(
            "USB {:04x}:{:04x}: brak dostępu do USB",
            sel.vid, sel.pid
        )))
    }
}

/// Transport USB bulk.
pub struct UsbTransport {
    dev: Box<dyn UsbBulk>,
    name: String,
}

impl UsbTransport {
    pub fn open(backend: &dyn UsbBackend, sel: &UsbSelector) -> Result<Self, TransportError> {
        let dev = backend.open(sel)?;
        Ok(UsbTransport {
            dev,
            name: format!("{:04x}:{:04x}", sel.vid, sel.pid),
        })
    }
}

impl Transport for UsbTransport {
    fn write(&mut self, data: &[u8]) -> Result<(), TransportError> {
        self.dev.write(data, Duration::from_secs(1)).map(|_| ())
    }
    fn read(&mut self, buf: &mut [u8], timeout: Duration) -> Result<usize, TransportError> {
        self.dev.read(buf, timeout)
    }
    fn describe(&self) -> String {
        format!("usb {}", self.name)
    }
}

/// USB systemu przez `nusb`.
pub struct NusbBackend;

impl UsbBackend for NusbBackend {
    fn list(&self) -> Vec<UsbDeviceInfo> {
        Vec::new()
    }
    fn open(&self, sel: &UsbSelector) -> Result<Box<dyn UsbBulk>, TransportError> {
        NoUsb.open(sel)
    }
}

/// Urządzenie fikcyjne USB: lista urządzeń z zadanym stanem dostępu; otwarte urządzenie
/// odsyła zapisane bajty (pętla zwrotna).
pub struct FakeUsb {
    pub devices: Vec<UsbDeviceInfo>,
}

impl FakeUsb {
    /// Zestaw do testów i zrzutów: K40 bez sterownika, galvo BJJCZ gotowe, nieznane urządzenie.
    pub fn demo() -> Self {
        FakeUsb {
            devices: Vec::new(),
        }
    }
}

impl UsbBackend for FakeUsb {
    fn list(&self) -> Vec<UsbDeviceInfo> {
        self.devices.clone()
    }
    fn open(&self, sel: &UsbSelector) -> Result<Box<dyn UsbBulk>, TransportError> {
        NoUsb.open(sel)
    }
}

/// Reguły udev dla znanych rodzin (plik `packaging/linux/99-burncraft.rules`).
pub fn udev_rules() -> String {
    String::new()
}

/// Ścieżka reguł udev w systemie.
pub const UDEV_RULES_PATH: &str = "/etc/udev/rules.d/99-burncraft.rules";

/// Przewodnik: co zrobić, żeby urządzenie było dostępne.
#[derive(Debug, Clone, PartialEq, Serialize, schemars::JsonSchema)]
pub struct UsbGuide {
    pub ok: bool,
    pub title: String,
    pub steps: Vec<String>,
}

/// Przewodnik dla stanu urządzenia i systemu (`windows`, `linux`, `macos`).
pub fn driver_guide(_dev: &UsbDeviceInfo, _os: &str) -> UsbGuide {
    UsbGuide {
        ok: false,
        title: String::new(),
        steps: Vec::new(),
    }
}
