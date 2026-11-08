//! Transport USB bulk (REQ-13-019, ADR 0013): lista urządzeń z VID/PID, rozpoznanie rodzin
//! laserów (kreator „Znajdź mój laser”, REQ-13-193), stan dostępu (sterownik WinUSB na
//! Windows, uprawnienia udev na Linuksie) z przewodnikiem instalacji (REQ-13-020, 021).
//! Biblioteka: `nusb` (czysty Rust). Urządzenie fikcyjne [`FakeUsb`] zastępuje sprzęt
//! w testach i przy `BURNCRAFT_FAKE_USB=1`.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
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

/// Znane rodziny laserów USB (źródła: ADR 0013, `THIRD_PARTY.md`).
pub const USB_FAMILIES: &[UsbFamily] = &[
    // VID/PID CH341 w trybie równoległym: specyfikacja §13.3. Punkty końcowe 0x02/0x82:
    // UNVERIFIED: wg opisu połączenia libusb w MeerK40t (moduł ch341) — test K40 w M3.4.
    UsbFamily {
        id: "ch341_k40",
        name: "K40 Lihuiyu / Moshi (CH341)",
        vid: 0x1A86,
        pid: 0x5512,
        driver: "lihuiyu",
        milestone: "M3.3",
        interface: 0,
        ep_out: 0x02,
        ep_in: 0x82,
    },
    // VID/PID galvo BJJCZ: specyfikacja §13.3. Punkty końcowe 0x02/0x88:
    // UNVERIFIED: wg opisu protokołu Balor — sterownik galvo w M5.
    UsbFamily {
        id: "bjjcz_galvo",
        name: "Galvo BJJCZ (EZCAD2, LMC)",
        vid: 0x9588,
        pid: 0x9899,
        driver: "bjjcz",
        milestone: "M5",
        interface: 0,
        ep_out: 0x02,
        ep_in: 0x88,
    },
];

pub fn usb_family(vid: u16, pid: u16) -> Option<&'static UsbFamily> {
    USB_FAMILIES.iter().find(|f| f.vid == vid && f.pid == pid)
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
    pub fn from_windows_driver(driver: Option<&str>) -> UsbAccess {
        match driver.map(str::trim).filter(|d| !d.is_empty()) {
            None => UsbAccess::DriverMissing,
            Some(d) if d.eq_ignore_ascii_case("winusb") => UsbAccess::Ready,
            Some(d) => UsbAccess::VendorDriver { driver: d.into() },
        }
    }

    fn problem(&self) -> Option<&'static str> {
        match self {
            UsbAccess::Ready | UsbAccess::Unknown => None,
            UsbAccess::DriverMissing => Some("brak sterownika WinUSB"),
            UsbAccess::VendorDriver { .. } => Some("sterownik producenta zamiast WinUSB"),
            UsbAccess::PermissionDenied => Some("brak uprawnień (reguły udev)"),
            UsbAccess::Busy => Some("urządzenie zajęte przez inny program"),
        }
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

impl UsbSelector {
    /// Interfejs i punkty końcowe z rodziny urządzenia (domyślnie 0, 0x02, 0x82).
    pub fn for_device(vid: u16, pid: u16, serial: Option<String>) -> Self {
        let f = usb_family(vid, pid);
        UsbSelector {
            vid,
            pid,
            serial,
            interface: f.map_or(0, |f| f.interface),
            ep_out: f.map_or(0x02, |f| f.ep_out),
            ep_in: f.map_or(0x82, |f| f.ep_in),
        }
    }

    fn name(&self) -> String {
        format!("{:04x}:{:04x}", self.vid, self.pid)
    }

    fn matches(&self, d: &UsbDeviceInfo) -> bool {
        d.vid == self.vid
            && d.pid == self.pid
            && self
                .serial
                .as_deref()
                .is_none_or(|s| d.serial.as_deref() == Some(s))
    }
}

/// Otwarte urządzenie: transfery bulk z limitem czasu.
pub trait UsbBulk: Send {
    fn write(&mut self, data: &[u8], timeout: Duration) -> Result<usize, TransportError>;
    /// 0 bajtów = brak danych w czasie `timeout`.
    fn read(&mut self, buf: &mut [u8], timeout: Duration) -> Result<usize, TransportError>;
    /// Żądanie sterujące vendor OUT do urządzenia (CH341: tryb portu równoległego).
    fn control_out(
        &mut self,
        request: u8,
        value: u16,
        index: u16,
        data: &[u8],
    ) -> Result<(), TransportError> {
        let _ = (value, index, data);
        Err(TransportError(format!(
            "żądanie sterujące 0x{request:02X}: nieobsługiwane"
        )))
    }
    /// Żądanie sterujące vendor IN z urządzenia (CH341: stan, wersja).
    fn control_in(
        &mut self,
        request: u8,
        value: u16,
        index: u16,
        len: u16,
    ) -> Result<Vec<u8>, TransportError> {
        let _ = (value, index, len);
        Err(TransportError(format!(
            "żądanie sterujące 0x{request:02X}: nieobsługiwane"
        )))
    }
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
            "USB {}: brak dostępu do USB",
            sel.name()
        )))
    }
}

/// Błąd otwarcia urządzenia w danym stanie dostępu.
fn access_error(sel: &UsbSelector, access: &UsbAccess) -> Option<TransportError> {
    access.problem().map(|p| {
        TransportError(format!(
            "USB {}: {p} — sterownik i uprawnienia: przewodnik machine.usb_guide",
            sel.name()
        ))
    })
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
            name: sel.name(),
        })
    }
}

impl Transport for UsbTransport {
    fn write(&mut self, data: &[u8]) -> Result<(), TransportError> {
        let mut sent = 0;
        while sent < data.len() {
            let n = self.dev.write(&data[sent..], Duration::from_secs(2))?;
            if n == 0 {
                return Err(TransportError(format!(
                    "usb {}: urządzenie nie przyjmuje danych",
                    self.name
                )));
            }
            sent += n;
        }
        Ok(())
    }
    fn read(&mut self, buf: &mut [u8], timeout: Duration) -> Result<usize, TransportError> {
        self.dev.read(buf, timeout)
    }
    fn describe(&self) -> String {
        format!("usb {}", self.name)
    }
}

// ---------- nusb ----------

/// USB systemu przez `nusb`.
pub struct NusbBackend;

fn nusb_err(sel: &UsbSelector, e: impl std::fmt::Display) -> TransportError {
    TransportError(format!("USB {}: {e}", sel.name()))
}

impl NusbBackend {
    fn info(d: &nusb::DeviceInfo) -> UsbDeviceInfo {
        #[cfg(target_os = "windows")]
        let (driver, access) = (
            d.driver().map(str::to_string),
            UsbAccess::from_windows_driver(d.driver()),
        );
        #[cfg(not(target_os = "windows"))]
        let (driver, access) = (None, UsbAccess::Unknown);
        UsbDeviceInfo {
            vid: d.vendor_id(),
            pid: d.product_id(),
            bus: d.bus_id().to_string(),
            address: d.device_address(),
            serial: d.serial_number().map(str::to_string),
            manufacturer: d.manufacturer_string().map(str::to_string),
            product: d.product_string().map(str::to_string),
            driver,
            access,
        }
    }
}

impl UsbBackend for NusbBackend {
    fn list(&self) -> Vec<UsbDeviceInfo> {
        use nusb::MaybeFuture;
        let Ok(devs) = nusb::list_devices().wait() else {
            return Vec::new();
        };
        devs.map(|d| {
            let mut info = Self::info(&d);
            // Linux/macOS: dostęp sprawdzany próbą otwarcia, tylko dla znanych rodzin laserów
            if info.access == UsbAccess::Unknown && usb_family(info.vid, info.pid).is_some() {
                info.access = match d.open().wait() {
                    Ok(_) => UsbAccess::Ready,
                    Err(e) => match e.kind() {
                        nusb::ErrorKind::PermissionDenied => UsbAccess::PermissionDenied,
                        nusb::ErrorKind::Busy => UsbAccess::Busy,
                        _ => UsbAccess::Unknown,
                    },
                };
            }
            info
        })
        .collect()
    }

    fn open(&self, sel: &UsbSelector) -> Result<Box<dyn UsbBulk>, TransportError> {
        use nusb::transfer::{Bulk, In, Out};
        use nusb::MaybeFuture;
        let devs = nusb::list_devices().wait().map_err(|e| nusb_err(sel, e))?;
        let d = devs
            .into_iter()
            .find(|d| sel.matches(&Self::info(d)))
            .ok_or_else(|| nusb_err(sel, "nie znaleziono urządzenia"))?;
        let info = Self::info(&d);
        if let Some(e) = access_error(sel, &info.access) {
            return Err(e);
        }
        let dev = d.open().wait().map_err(|e| {
            let access = match e.kind() {
                nusb::ErrorKind::PermissionDenied => UsbAccess::PermissionDenied,
                nusb::ErrorKind::Busy => UsbAccess::Busy,
                _ => UsbAccess::Unknown,
            };
            access_error(sel, &access).unwrap_or_else(|| nusb_err(sel, e))
        })?;
        let iface = dev
            .detach_and_claim_interface(sel.interface)
            .wait()
            .map_err(|e| nusb_err(sel, e))?;
        let out = iface
            .endpoint::<Bulk, Out>(sel.ep_out)
            .map_err(|e| nusb_err(sel, e))?;
        let inp = iface
            .endpoint::<Bulk, In>(sel.ep_in)
            .map_err(|e| nusb_err(sel, e))?;
        Ok(Box::new(NusbBulk {
            iface,
            out,
            inp,
            pending: VecDeque::new(),
        }))
    }
}

struct NusbBulk {
    iface: nusb::Interface,
    out: nusb::Endpoint<nusb::transfer::Bulk, nusb::transfer::Out>,
    inp: nusb::Endpoint<nusb::transfer::Bulk, nusb::transfer::In>,
    pending: VecDeque<u8>,
}

fn transfer_err(e: nusb::transfer::TransferError) -> TransportError {
    TransportError(format!("USB: {e}"))
}

impl UsbBulk for NusbBulk {
    fn write(&mut self, data: &[u8], timeout: Duration) -> Result<usize, TransportError> {
        let c = self
            .out
            .transfer_blocking(nusb::transfer::Buffer::from(data.to_vec()), timeout);
        match c.status {
            Ok(()) => Ok(c.actual_len),
            Err(nusb::transfer::TransferError::Cancelled) => Ok(c.actual_len),
            Err(e) => Err(transfer_err(e)),
        }
    }

    fn read(&mut self, buf: &mut [u8], timeout: Duration) -> Result<usize, TransportError> {
        if self.pending.is_empty() {
            let mps = self.inp.max_packet_size().max(1);
            let b = self.inp.allocate(mps * 4);
            let c = self.inp.transfer_blocking(b, timeout);
            match c.status {
                Ok(()) | Err(nusb::transfer::TransferError::Cancelled) => {
                    let v = c.buffer.into_vec();
                    self.pending.extend(&v[..c.actual_len.min(v.len())]);
                }
                Err(e) => return Err(transfer_err(e)),
            }
        }
        let n = self.pending.len().min(buf.len());
        for (i, b) in self.pending.drain(..n).enumerate() {
            buf[i] = b;
        }
        Ok(n)
    }

    fn control_out(
        &mut self,
        request: u8,
        value: u16,
        index: u16,
        data: &[u8],
    ) -> Result<(), TransportError> {
        use nusb::transfer::{ControlOut, ControlType, Recipient};
        use nusb::MaybeFuture;
        self.iface
            .control_out(
                ControlOut {
                    control_type: ControlType::Vendor,
                    recipient: Recipient::Device,
                    request,
                    value,
                    index,
                    data,
                },
                Duration::from_millis(1500),
            )
            .wait()
            .map_err(transfer_err)
    }

    fn control_in(
        &mut self,
        request: u8,
        value: u16,
        index: u16,
        len: u16,
    ) -> Result<Vec<u8>, TransportError> {
        use nusb::transfer::{ControlIn, ControlType, Recipient};
        use nusb::MaybeFuture;
        self.iface
            .control_in(
                ControlIn {
                    control_type: ControlType::Vendor,
                    recipient: Recipient::Device,
                    request,
                    value,
                    index,
                    length: len,
                },
                Duration::from_millis(1500),
            )
            .wait()
            .map_err(transfer_err)
    }
}

// ---------- urządzenie fikcyjne ----------

/// Urządzenie fikcyjne USB: lista urządzeń z zadanym stanem dostępu; otwarte urządzenie
/// odsyła zapisane bajty (pętla zwrotna).
pub struct FakeUsb {
    pub devices: Vec<UsbDeviceInfo>,
}

impl FakeUsb {
    /// Zestaw do testów i zrzutów: K40 bez sterownika, galvo BJJCZ gotowe, mysz (nieznana).
    pub fn demo() -> Self {
        let dev = |vid, pid, address, product: &str, driver: Option<&str>, access| UsbDeviceInfo {
            vid,
            pid,
            bus: "1".into(),
            address,
            serial: None,
            manufacturer: None,
            product: Some(product.into()),
            driver: driver.map(str::to_string),
            access,
        };
        FakeUsb {
            devices: vec![
                dev(
                    0x1A86,
                    0x5512,
                    3,
                    "USB2.0-Printer",
                    None,
                    UsbAccess::DriverMissing,
                ),
                dev(
                    0x9588,
                    0x9899,
                    4,
                    "USBLMCV2",
                    Some("WinUSB"),
                    UsbAccess::Ready,
                ),
                dev(
                    0x046D,
                    0xC077,
                    5,
                    "USB Optical Mouse",
                    Some("HidUsb"),
                    UsbAccess::Ready,
                ),
            ],
        }
    }
}

struct Loopback(Arc<Mutex<VecDeque<u8>>>);

impl UsbBulk for Loopback {
    fn write(&mut self, data: &[u8], _timeout: Duration) -> Result<usize, TransportError> {
        self.0.lock().expect("pętla").extend(data);
        Ok(data.len())
    }
    fn read(&mut self, buf: &mut [u8], timeout: Duration) -> Result<usize, TransportError> {
        let mut q = self.0.lock().expect("pętla");
        if q.is_empty() {
            drop(q);
            std::thread::sleep(timeout.min(Duration::from_millis(5)));
            return Ok(0);
        }
        let n = q.len().min(buf.len());
        for (i, b) in q.drain(..n).enumerate() {
            buf[i] = b;
        }
        Ok(n)
    }
}

impl UsbBackend for FakeUsb {
    fn list(&self) -> Vec<UsbDeviceInfo> {
        self.devices.clone()
    }
    fn open(&self, sel: &UsbSelector) -> Result<Box<dyn UsbBulk>, TransportError> {
        let d = self
            .devices
            .iter()
            .find(|d| sel.matches(d))
            .ok_or_else(|| nusb_err(sel, "nie znaleziono urządzenia"))?;
        if let Some(e) = access_error(sel, &d.access) {
            return Err(e);
        }
        Ok(Box::new(Loopback(Arc::new(Mutex::new(VecDeque::new())))))
    }
}

// ---------- udev i przewodnik ----------

/// Ścieżka reguł udev w systemie.
pub const UDEV_RULES_PATH: &str = "/etc/udev/rules.d/99-burncraft.rules";

/// Reguły udev dla znanych rodzin (plik `packaging/linux/99-burncraft.rules`,
/// `burncraft-cli udev`): dostęp zalogowanego użytkownika bez uprawnień administratora.
pub fn udev_rules() -> String {
    let mut s = String::from(
        "# BurnCraft: dostęp do laserów USB bez uprawnień administratora (REQ-13-021).\n\
         # Instalacja: burncraft-cli udev | sudo tee /etc/udev/rules.d/99-burncraft.rules\n\
         #             sudo udevadm control --reload-rules && sudo udevadm trigger\n",
    );
    for f in USB_FAMILIES {
        s.push_str(&format!(
            "\n# {}\nSUBSYSTEM==\"usb\", ATTRS{{idVendor}}==\"{:04x}\", ATTRS{{idProduct}}==\"{:04x}\", MODE=\"0660\", TAG+=\"uaccess\"\n",
            f.name, f.vid, f.pid
        ));
    }
    s
}

/// Przewodnik: co zrobić, żeby urządzenie było dostępne.
#[derive(Debug, Clone, PartialEq, Serialize, schemars::JsonSchema)]
pub struct UsbGuide {
    pub ok: bool,
    /// Rodzaj przewodnika (tłumaczenia w GUI): ok | winusb | udev | busy.
    pub kind: String,
    pub title: String,
    pub steps: Vec<String>,
}

/// Przewodnik dla stanu urządzenia i systemu (`windows`, `linux`, `macos`).
pub fn driver_guide(dev: &UsbDeviceInfo, os: &str) -> UsbGuide {
    let ids = format!("{:04X} {:04X}", dev.vid, dev.pid);
    let guide = |kind: &str, title: &str, steps: Vec<String>| UsbGuide {
        ok: steps.is_empty(),
        kind: kind.into(),
        title: title.into(),
        steps,
    };
    let winusb = |vendor: Option<&str>| {
        let mut steps = Vec::new();
        if let Some(d) = vendor {
            steps.push(format!(
                "Urządzenie używa teraz sterownika producenta {d}. Zamiana na WinUSB wyłączy dla niego program producenta."
            ));
        }
        steps.extend([
            "Pobierz bezpłatne narzędzie Zadig (zadig.akeo.ie). BurnCraft go nie dołącza.".to_string(),
            format!("Uruchom Zadig, w menu Options zaznacz „List All Devices” i wybierz urządzenie o USB ID {ids}."),
            "Jako sterownik docelowy wybierz WinUSB i kliknij „Install Driver” (albo „Replace Driver”).".to_string(),
            "Odłącz i podłącz laser, potem w BurnCraft kliknij „Znajdź mój laser”.".to_string(),
        ]);
        if dev.vid == 0x1A86 && dev.pid == 0x5512 {
            steps.push(
                "Zamiast WinUSB możesz zostawić oryginalny sterownik CH341 producenta (CH341DLL): BurnCraft połączy się z K40 także przez niego."
                    .to_string(),
            );
        }
        guide("winusb", "Sterownik WinUSB (Windows)", steps)
    };
    let udev = || {
        guide(
            "udev",
            "Uprawnienia USB (Linux, udev)",
            vec![
                format!("Zapisz reguły udev: burncraft-cli udev | sudo tee {UDEV_RULES_PATH}"),
                "Wczytaj reguły: sudo udevadm control --reload-rules && sudo udevadm trigger"
                    .to_string(),
                "Odłącz i podłącz laser, potem w BurnCraft kliknij „Znajdź mój laser”.".to_string(),
            ],
        )
    };
    match &dev.access {
        UsbAccess::Ready => guide("ok", "Urządzenie gotowe", Vec::new()),
        UsbAccess::DriverMissing => winusb(None),
        UsbAccess::VendorDriver { driver } => winusb(Some(driver)),
        UsbAccess::PermissionDenied => udev(),
        UsbAccess::Busy => guide(
            "busy",
            "Urządzenie zajęte",
            vec![
                "Zamknij inne programy, które używają lasera (np. program producenta), i kliknij „Znajdź mój laser”."
                    .to_string(),
            ],
        ),
        UsbAccess::Unknown => match os {
            "linux" => udev(),
            _ => guide("ok", "Urządzenie gotowe", Vec::new()),
        },
    }
}
