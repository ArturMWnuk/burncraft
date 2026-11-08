//! Układ CH341 w trybie równoległym EPP (K40 Lihuiyu i Moshi, ADR 0014): zapis danych
//! i adresu, odczyt stanu. Szkielet API (M3.3, testy red) — bez implementacji.

use crate::transport::TransportError;
use crate::usb::{UsbBackend, UsbBulk, UsbSelector};

pub const CMD_WRITE_DATA: u8 = 0xA6;
pub const CMD_WRITE_ADDR: u8 = 0xA7;
pub const CMD_STATUS: u8 = 0xA0;
pub const REQ_PARA_INIT: u8 = 0xB1;
pub const REQ_STATUS: u8 = 0x52;
pub const REQ_VERSION: u8 = 0x5F;
pub const MODE_EPP19: u16 = 1;
pub const CHUNK: usize = 31;

fn todo() -> TransportError {
    TransportError("CH341: niezaimplementowane (M3.3)".into())
}

/// Dostęp do płyty przez CH341 (port równoległy EPP).
pub trait Ch341: Send {
    fn write_data(&mut self, data: &[u8]) -> Result<(), TransportError>;
    fn write_addr(&mut self, data: &[u8]) -> Result<(), TransportError>;
    fn status(&mut self) -> Result<[u8; 6], TransportError>;
    fn describe(&self) -> String;
}

pub fn frame(_cmd: u8, _data: &[u8]) -> Vec<u8> {
    Vec::new()
}

/// CH341 przez USB bulk (REQ-13-039).
pub struct Ch341Usb {
    pub version: Option<u16>,
}

impl Ch341Usb {
    pub fn open(_backend: &dyn UsbBackend, _sel: &UsbSelector) -> Result<Self, TransportError> {
        Err(todo())
    }
}

impl Ch341 for Ch341Usb {
    fn write_data(&mut self, _data: &[u8]) -> Result<(), TransportError> {
        Err(todo())
    }
    fn write_addr(&mut self, _data: &[u8]) -> Result<(), TransportError> {
        Err(todo())
    }
    fn status(&mut self) -> Result<[u8; 6], TransportError> {
        Err(todo())
    }
    fn describe(&self) -> String {
        "ch341".into()
    }
}

pub const DLL_NAMES: &[&str] = &[];

/// CH341 przez sterownik producenta CH341DLL (REQ-13-040).
pub struct Ch341Dll;

impl Ch341Dll {
    pub fn open(_index: u32) -> Result<Self, TransportError> {
        Err(TransportError("niezaimplementowane".into()))
    }
}

impl Ch341 for Ch341Dll {
    fn write_data(&mut self, _data: &[u8]) -> Result<(), TransportError> {
        Err(todo())
    }
    fn write_addr(&mut self, _data: &[u8]) -> Result<(), TransportError> {
        Err(todo())
    }
    fn status(&mut self) -> Result<[u8; 6], TransportError> {
        Err(todo())
    }
    fn describe(&self) -> String {
        "ch341dll".into()
    }
}

/// Urządzenie USB CH341 przed emulatorem.
pub struct EmuBulk<E: Ch341> {
    pub emu: std::sync::Arc<std::sync::Mutex<E>>,
    pub initialized: bool,
}

impl<E: Ch341> EmuBulk<E> {
    pub fn new(emu: std::sync::Arc<std::sync::Mutex<E>>) -> Self {
        EmuBulk {
            emu,
            initialized: false,
        }
    }
}

impl<E: Ch341 + 'static> UsbBulk for EmuBulk<E> {
    fn write(
        &mut self,
        _data: &[u8],
        _timeout: std::time::Duration,
    ) -> Result<usize, TransportError> {
        Err(todo())
    }
    fn read(
        &mut self,
        _buf: &mut [u8],
        _timeout: std::time::Duration,
    ) -> Result<usize, TransportError> {
        Err(todo())
    }
}

/// Backend USB z urządzeniem CH341 przed emulatorem.
pub struct EmuUsb<E: Ch341> {
    pub emu: std::sync::Arc<std::sync::Mutex<E>>,
}

impl<E: Ch341 + 'static> UsbBackend for EmuUsb<E> {
    fn list(&self) -> Vec<crate::usb::UsbDeviceInfo> {
        Vec::new()
    }
    fn open(&self, _sel: &UsbSelector) -> Result<Box<dyn UsbBulk>, TransportError> {
        Err(todo())
    }
}

/// Emulator współdzielony jako [`Ch341`].
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
