//! Układ CH341 w trybie równoległym EPP (K40 Lihuiyu i Moshi, ADR 0014 §1): zapis danych
//! i adresu, odczyt stanu. Trzy dostępy: natywnie przez USB bulk z M3.1 ([`Ch341Usb`],
//! REQ-13-039), przez sterownik producenta CH341DLL na Windows ([`Ch341Dll`], REQ-13-040)
//! i emulatory w procesie (REQ-19-009).
//!
//! Źródło faktów o CH341: MeerK40t (MIT), moduł `ch341` (`libusb.py`, `windriver.py`) —
//! czytany jako opis bajtów na łączu, bez kopiowania kodu (ADR 0013).

use std::time::Duration;

use crate::transport::TransportError;
use crate::usb::{UsbBackend, UsbBulk, UsbSelector};

/// Polecenie CH341 „zapis danych EPP”: bajt przed każdymi 31 bajtami danych.
/// Źródło: MeerK40t `ch341/libusb.py` (CH341_PARA_CMD_W0 = 0xA6).
pub const CMD_WRITE_DATA: u8 = 0xA6;
/// Polecenie CH341 „zapis adresu EPP”. Źródło: MeerK40t `ch341/libusb.py` (W1 = 0xA7).
pub const CMD_WRITE_ADDR: u8 = 0xA7;
/// Polecenie CH341 „odczyt stanu” (ścieżka bulk). Źródło: MeerK40t `ch341/libusb.py` (0xA0).
pub const CMD_STATUS: u8 = 0xA0;
/// Żądanie sterujące (vendor OUT) inicjalizacji portu równoległego; wValue = (tryb << 8) | 2.
/// Źródło: MeerK40t `ch341/libusb.py` (0xB1, tryb 1 = EPP 1.9).
pub const REQ_PARA_INIT: u8 = 0xB1;
/// Żądanie sterujące (vendor IN) stanu, 8 bajtów. Źródło: MeerK40t `ch341/libusb.py` (0x52).
pub const REQ_STATUS: u8 = 0x52;
/// Żądanie sterujące (vendor IN) wersji układu, 2 bajty. Źródło: MeerK40t (0x5F).
pub const REQ_VERSION: u8 = 0x5F;
/// Tryb EPP 1.9. Źródło: MeerK40t `ch341` (CH341_PARA_MODE_EPP19 = 1).
pub const MODE_EPP19: u16 = 1;
/// Dane w jednej jednostce zapisu (32 bajty = polecenie + 31 danych). Źródło: MeerK40t.
pub const CHUNK: usize = 31;

/// Dostęp do płyty przez CH341 (port równoległy EPP).
pub trait Ch341: Send {
    /// Zapis danych (EPP data).
    fn write_data(&mut self, data: &[u8]) -> Result<(), TransportError>;
    /// Zapis adresu (EPP address) — Moshi: polecenia czasu rzeczywistego.
    fn write_addr(&mut self, data: &[u8]) -> Result<(), TransportError>;
    /// Stan płyty: 6 bajtów, kod stanu w bajcie 1 (źródło: MeerK40t `lihuiyu/controller.py`).
    fn status(&mut self) -> Result<[u8; 6], TransportError>;
    fn describe(&self) -> String;
}

/// Ramki zapisu CH341: polecenie `cmd` przed każdymi 31 bajtami.
pub fn frame(cmd: u8, data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len() + data.len() / CHUNK + 1);
    if data.is_empty() {
        out.push(cmd);
    }
    for c in data.chunks(CHUNK) {
        out.push(cmd);
        out.extend_from_slice(c);
    }
    out
}

/// CH341 przez USB bulk (`nusb` z M3.1, REQ-13-039).
pub struct Ch341Usb {
    dev: Box<dyn UsbBulk>,
    name: String,
    /// Wersja układu (żądanie 0x5F), gdy odczytana.
    pub version: Option<u16>,
}

impl Ch341Usb {
    /// Otwiera urządzenie i włącza tryb EPP 1.9.
    pub fn open(backend: &dyn UsbBackend, sel: &UsbSelector) -> Result<Self, TransportError> {
        let mut dev = backend.open(sel)?;
        // UNVERIFIED: inicjalizacja EPP żądaniem 0xB1, wValue 0x0102 — wg MeerK40t libusb.py;
        // test K40 w M3.4.
        dev.control_out(REQ_PARA_INIT, (MODE_EPP19 << 8) | 2, 0, &[])?;
        let version = dev
            .control_in(REQ_VERSION, 0, 0, 2)
            .ok()
            .filter(|v| v.len() >= 2)
            .map(|v| u16::from_le_bytes([v[0], v[1]]));
        Ok(Ch341Usb {
            dev,
            name: format!("usb {:04x}:{:04x}", sel.vid, sel.pid),
            version,
        })
    }

    fn write_all(&mut self, bytes: &[u8]) -> Result<(), TransportError> {
        let mut sent = 0;
        while sent < bytes.len() {
            let n = self.dev.write(&bytes[sent..], Duration::from_secs(60))?;
            if n == 0 {
                return Err(TransportError(format!(
                    "{}: urządzenie nie przyjmuje danych",
                    self.name
                )));
            }
            sent += n;
        }
        Ok(())
    }
}

impl Ch341 for Ch341Usb {
    fn write_data(&mut self, data: &[u8]) -> Result<(), TransportError> {
        self.write_all(&frame(CMD_WRITE_DATA, data))
    }
    fn write_addr(&mut self, data: &[u8]) -> Result<(), TransportError> {
        self.write_all(&frame(CMD_WRITE_ADDR, data))
    }
    fn status(&mut self) -> Result<[u8; 6], TransportError> {
        // UNVERIFIED: stan żądaniem sterującym 0x52 (8 bajtów, kod w bajcie 1) — wg MeerK40t
        // libusb.py (ścieżka domyślna); test K40 w M3.4.
        let v = self.dev.control_in(REQ_STATUS, 0, 0, 8)?;
        let mut s = [0u8; 6];
        for (i, b) in v.iter().take(6).enumerate() {
            s[i] = *b;
        }
        Ok(s)
    }
    fn describe(&self) -> String {
        self.name.clone()
    }
}

// ---------- CH341DLL (Windows) ----------

/// Nazwy biblioteki sterownika producenta (WCH, pakiet CH341PAR) dla procesu 64- i 32-bitowego.
/// UNVERIFIED: nazwy plików i prototypy funkcji wg nagłówka CH341DLL.H producenta (WCH);
/// test K40 w M3.4 (REQ-13-040).
pub const DLL_NAMES: &[&str] = if cfg!(target_pointer_width = "64") {
    &["CH341DLLA64.DLL", "CH341DLL.DLL"]
} else {
    &["CH341DLL.DLL"]
};

/// CH341 przez sterownik producenta CH341DLL (Windows, REQ-13-040). Biblioteka jest ładowana
/// w czasie działania (`LoadLibraryW`), więc nie jest zależnością budowania.
pub struct Ch341Dll {
    #[cfg(windows)]
    lib: dll::Library,
    index: u32,
}

impl Ch341Dll {
    /// Otwiera urządzenie `index` (0 = pierwsze) i włącza tryb EPP 1.9.
    pub fn open(index: u32) -> Result<Self, TransportError> {
        #[cfg(windows)]
        {
            let lib = dll::Library::load()?;
            lib.open(index)?;
            Ok(Ch341Dll { lib, index })
        }
        #[cfg(not(windows))]
        {
            let _ = index;
            Err(TransportError(
                "sterownik CH341DLL jest dostępny tylko na Windows".into(),
            ))
        }
    }
}

impl Ch341 for Ch341Dll {
    fn write_data(&mut self, data: &[u8]) -> Result<(), TransportError> {
        #[cfg(windows)]
        {
            self.lib.epp_write(self.index, data, false)
        }
        #[cfg(not(windows))]
        {
            let _ = data;
            Err(TransportError("CH341DLL: tylko Windows".into()))
        }
    }
    fn write_addr(&mut self, data: &[u8]) -> Result<(), TransportError> {
        #[cfg(windows)]
        {
            self.lib.epp_write(self.index, data, true)
        }
        #[cfg(not(windows))]
        {
            let _ = data;
            Err(TransportError("CH341DLL: tylko Windows".into()))
        }
    }
    fn status(&mut self) -> Result<[u8; 6], TransportError> {
        #[cfg(windows)]
        {
            self.lib.status(self.index)
        }
        #[cfg(not(windows))]
        {
            Err(TransportError("CH341DLL: tylko Windows".into()))
        }
    }
    fn describe(&self) -> String {
        format!("ch341dll {}", self.index)
    }
}

impl Drop for Ch341Dll {
    fn drop(&mut self) {
        #[cfg(windows)]
        self.lib.close(self.index);
    }
}

#[cfg(windows)]
mod dll {
    use super::*;
    use std::ffi::c_void;

    type Handle = *mut c_void;
    type OpenFn = unsafe extern "system" fn(u32) -> Handle;
    type CloseFn = unsafe extern "system" fn(u32);
    type InitFn = unsafe extern "system" fn(u32, u32) -> i32;
    type WriteFn = unsafe extern "system" fn(u32, *mut c_void, *mut u32) -> i32;
    type StatusFn = unsafe extern "system" fn(u32, *mut u32) -> i32;

    #[link(name = "kernel32")]
    extern "system" {
        fn LoadLibraryW(name: *const u16) -> Handle;
        fn GetProcAddress(module: Handle, name: *const u8) -> *mut c_void;
        fn FreeLibrary(module: Handle) -> i32;
    }

    pub(super) struct Library {
        module: Handle,
        open: OpenFn,
        close: CloseFn,
        init: InitFn,
        write_data: WriteFn,
        write_addr: WriteFn,
        status: StatusFn,
    }

    // Uchwyt biblioteki i wskaźniki funkcji są ważne w każdym wątku.
    unsafe impl Send for Library {}

    fn sym(module: Handle, name: &str) -> Result<*mut c_void, TransportError> {
        let c: Vec<u8> = name.bytes().chain([0]).collect();
        // SAFETY: `module` to uchwyt z LoadLibraryW, `c` to nazwa zakończona zerem.
        let p = unsafe { GetProcAddress(module, c.as_ptr()) };
        if p.is_null() {
            Err(TransportError(format!("CH341DLL: brak funkcji {name}")))
        } else {
            Ok(p)
        }
    }

    impl Library {
        pub(super) fn load() -> Result<Self, TransportError> {
            for n in DLL_NAMES {
                let w: Vec<u16> = n.encode_utf16().chain([0]).collect();
                // SAFETY: nazwa pliku zakończona zerem.
                let module = unsafe { LoadLibraryW(w.as_ptr()) };
                if module.is_null() {
                    continue;
                }
                let get = || -> Result<Library, TransportError> {
                    // SAFETY: prototypy wg nagłówka CH341DLL.H (UNVERIFIED, test w M3.4).
                    unsafe {
                        Ok(Library {
                            module,
                            open: std::mem::transmute::<*mut c_void, OpenFn>(sym(
                                module,
                                "CH341OpenDevice",
                            )?),
                            close: std::mem::transmute::<*mut c_void, CloseFn>(sym(
                                module,
                                "CH341CloseDevice",
                            )?),
                            init: std::mem::transmute::<*mut c_void, InitFn>(sym(
                                module,
                                "CH341InitParallel",
                            )?),
                            write_data: std::mem::transmute::<*mut c_void, WriteFn>(sym(
                                module,
                                "CH341EppWriteData",
                            )?),
                            write_addr: std::mem::transmute::<*mut c_void, WriteFn>(sym(
                                module,
                                "CH341EppWriteAddr",
                            )?),
                            status: std::mem::transmute::<*mut c_void, StatusFn>(sym(
                                module,
                                "CH341GetStatus",
                            )?),
                        })
                    }
                };
                match get() {
                    Ok(l) => return Ok(l),
                    Err(e) => {
                        // SAFETY: uchwyt z LoadLibraryW.
                        unsafe { FreeLibrary(module) };
                        return Err(e);
                    }
                }
            }
            Err(TransportError(format!(
                "nie znaleziono sterownika CH341DLL ({}) — zainstaluj sterownik producenta albo użyj WinUSB",
                DLL_NAMES.join(", ")
            )))
        }

        pub(super) fn open(&self, index: u32) -> Result<(), TransportError> {
            // SAFETY: funkcje z biblioteki producenta, argumenty wg CH341DLL.H.
            let h = unsafe { (self.open)(index) };
            if h.is_null() || h as isize == -1 {
                return Err(TransportError(format!(
                    "CH341DLL: nie można otworzyć urządzenia {index}"
                )));
            }
            // SAFETY: jw.
            if unsafe { (self.init)(index, MODE_EPP19 as u32) } == 0 {
                // SAFETY: jw.
                unsafe { (self.close)(index) };
                return Err(TransportError(format!(
                    "CH341DLL: urządzenie {index} odrzuciło tryb EPP"
                )));
            }
            Ok(())
        }

        pub(super) fn close(&self, index: u32) {
            // SAFETY: jw.
            unsafe { (self.close)(index) };
        }

        pub(super) fn epp_write(
            &self,
            index: u32,
            data: &[u8],
            addr: bool,
        ) -> Result<(), TransportError> {
            let mut buf = data.to_vec();
            let mut len = buf.len() as u32;
            let f = if addr {
                self.write_addr
            } else {
                self.write_data
            };
            // SAFETY: bufor i długość ważne przez czas wywołania.
            let ok = unsafe { f(index, buf.as_mut_ptr().cast(), &mut len) };
            if ok == 0 || len as usize != data.len() {
                return Err(TransportError("CH341DLL: zapis nieudany".into()));
            }
            Ok(())
        }

        pub(super) fn status(&self, index: u32) -> Result<[u8; 6], TransportError> {
            // UNVERIFIED: stan w drugim bajcie wyniku CH341GetStatus (jak bajt 1 stanu USB)
            // — wg opisu bitów stanu w MeerK40t windriver.py; test K40 w M3.4.
            let mut v = [0u32; 2];
            // SAFETY: bufor 8 bajtów (≥ ULONG).
            if unsafe { (self.status)(index, v.as_mut_ptr()) } == 0 {
                return Err(TransportError("CH341DLL: odczyt stanu nieudany".into()));
            }
            let b = v[0].to_le_bytes();
            Ok([b[0], b[1], b[2], b[3], 0, 0])
        }
    }

    impl Drop for Library {
        fn drop(&mut self) {
            // SAFETY: uchwyt z LoadLibraryW.
            unsafe { FreeLibrary(self.module) };
        }
    }
}

// ---------- emulator za interfejsem USB ----------

/// Urządzenie USB CH341 przed emulatorem: dekoduje ramki zapisu (0xA6/0xA7), żądania
/// sterujące (0xB1, 0x52, 0x5F) i stan przez bulk (0xA0) — test ścieżki natywnej
/// (REQ-13-039) bez sprzętu.
pub struct EmuBulk<E: Ch341> {
    pub emu: std::sync::Arc<std::sync::Mutex<E>>,
    pending: std::collections::VecDeque<u8>,
    /// Inicjalizacja EPP (żądanie 0xB1) odebrana.
    pub initialized: bool,
}

impl<E: Ch341> EmuBulk<E> {
    pub fn new(emu: std::sync::Arc<std::sync::Mutex<E>>) -> Self {
        EmuBulk {
            emu,
            pending: Default::default(),
            initialized: false,
        }
    }
}

impl<E: Ch341 + 'static> UsbBulk for EmuBulk<E> {
    fn write(&mut self, data: &[u8], _timeout: Duration) -> Result<usize, TransportError> {
        if data.first() == Some(&CMD_STATUS) && data.len() == 1 {
            let s = self.emu.lock().expect("emulator").status()?;
            self.pending.extend(s);
            return Ok(1);
        }
        let cmd = *data
            .first()
            .ok_or_else(|| TransportError("pusty zapis".into()))?;
        let mut payload = Vec::new();
        let mut i = 0;
        while i < data.len() {
            if data[i] != cmd {
                return Err(TransportError(format!(
                    "ramka CH341: oczekiwano 0x{cmd:02X} na pozycji {i}"
                )));
            }
            let end = (i + 1 + CHUNK).min(data.len());
            payload.extend_from_slice(&data[i + 1..end]);
            i = end;
        }
        let mut e = self.emu.lock().expect("emulator");
        match cmd {
            CMD_WRITE_DATA => e.write_data(&payload)?,
            CMD_WRITE_ADDR => e.write_addr(&payload)?,
            c => {
                return Err(TransportError(format!(
                    "nieznane polecenie CH341 0x{c:02X}"
                )))
            }
        }
        Ok(data.len())
    }

    fn read(&mut self, buf: &mut [u8], timeout: Duration) -> Result<usize, TransportError> {
        if self.pending.is_empty() {
            std::thread::sleep(timeout.min(Duration::from_millis(1)));
            return Ok(0);
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
        _index: u16,
        _data: &[u8],
    ) -> Result<(), TransportError> {
        match request {
            REQ_PARA_INIT if value == (MODE_EPP19 << 8) | 2 => {
                self.initialized = true;
                Ok(())
            }
            r => Err(TransportError(format!(
                "nieobsługiwane żądanie 0x{r:02X} wartość 0x{value:04X}"
            ))),
        }
    }

    fn control_in(
        &mut self,
        request: u8,
        _value: u16,
        _index: u16,
        len: u16,
    ) -> Result<Vec<u8>, TransportError> {
        match request {
            REQ_STATUS => {
                if !self.initialized {
                    return Err(TransportError("CH341: port EPP nie zainicjowany".into()));
                }
                let s = self.emu.lock().expect("emulator").status()?;
                let mut v = s.to_vec();
                v.resize(len as usize, 0);
                Ok(v)
            }
            // wersja układu 0x30 (opis w MeerK40t ch341/libusb.py)
            REQ_VERSION => Ok(vec![0x30, 0x00]),
            r => Err(TransportError(format!("nieobsługiwane żądanie 0x{r:02X}"))),
        }
    }
}

/// Backend USB z jednym urządzeniem CH341 `1a86:5512` przed emulatorem (testy REQ-13-039).
pub struct EmuUsb<E: Ch341> {
    pub emu: std::sync::Arc<std::sync::Mutex<E>>,
}

impl<E: Ch341 + 'static> UsbBackend for EmuUsb<E> {
    fn list(&self) -> Vec<crate::usb::UsbDeviceInfo> {
        vec![crate::usb::UsbDeviceInfo {
            vid: 0x1A86,
            pid: 0x5512,
            bus: "1".into(),
            address: 7,
            serial: None,
            manufacturer: None,
            product: Some("USB2.0-Printer".into()),
            driver: Some("WinUSB".into()),
            access: crate::usb::UsbAccess::Ready,
        }]
    }
    fn open(&self, sel: &UsbSelector) -> Result<Box<dyn UsbBulk>, TransportError> {
        if (sel.vid, sel.pid) != (0x1A86, 0x5512) {
            return Err(TransportError("nie znaleziono urządzenia".into()));
        }
        Ok(Box::new(EmuBulk::new(self.emu.clone())))
    }
}

/// Emulator współdzielony jako [`Ch341`] (dostęp do emulatora z testów w trakcie połączenia).
pub struct Shared<E: Ch341>(pub std::sync::Arc<std::sync::Mutex<E>>);

impl<E: Ch341> Ch341 for Shared<E> {
    fn write_data(&mut self, data: &[u8]) -> Result<(), TransportError> {
        self.0.lock().expect("emulator").write_data(data)
    }
    fn write_addr(&mut self, data: &[u8]) -> Result<(), TransportError> {
        self.0.lock().expect("emulator").write_addr(data)
    }
    fn status(&mut self) -> Result<[u8; 6], TransportError> {
        self.0.lock().expect("emulator").status()
    }
    fn describe(&self) -> String {
        self.0.lock().expect("emulator").describe()
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn frame_prefixes_every_31_bytes() {
        let data: Vec<u8> = (0..32).collect();
        let f = super::frame(0xA6, &data);
        assert_eq!(f.len(), 34);
        assert_eq!(f[0], 0xA6);
        assert_eq!(f[32], 0xA6);
        assert_eq!(f[33], 31);
        assert_eq!(super::frame(0xA7, &[]), vec![0xA7]);
    }
}
