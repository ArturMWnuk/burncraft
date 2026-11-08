//! Emulator płyty Moshi MS10105 (ADR 0014). Szkielet API (M3.3, testy red) — bez implementacji.

use crate::ch341::Ch341;
use crate::lihuiyu::emu::EgvMove;
use crate::transport::TransportError;

#[derive(Debug, Clone)]
pub struct EmuConfig {
    pub speed: f64,
}

impl Default for EmuConfig {
    fn default() -> Self {
        EmuConfig {
            speed: f64::INFINITY,
        }
    }
}

/// Emulator Moshi za interfejsem [`Ch341`].
pub struct MoshiEmulator {
    pub cfg: EmuConfig,
    pub trace: Vec<EgvMove>,
    pub programs: Vec<Vec<u8>>,
    pub packets: Vec<usize>,
    pub estops: usize,
}

impl MoshiEmulator {
    pub fn new(cfg: EmuConfig) -> Self {
        MoshiEmulator {
            cfg,
            trace: Vec::new(),
            programs: Vec::new(),
            packets: Vec::new(),
            estops: 0,
        }
    }
    pub fn shared(cfg: EmuConfig) -> std::sync::Arc<std::sync::Mutex<Self>> {
        std::sync::Arc::new(std::sync::Mutex::new(Self::new(cfg)))
    }
    pub fn advance(&mut self, _dt: f64) {}
    pub fn running(&self) -> bool {
        false
    }
}

impl Ch341 for MoshiEmulator {
    fn write_data(&mut self, _data: &[u8]) -> Result<(), TransportError> {
        Ok(())
    }
    fn write_addr(&mut self, _data: &[u8]) -> Result<(), TransportError> {
        Ok(())
    }
    fn status(&mut self) -> Result<[u8; 6], TransportError> {
        Ok([0; 6])
    }
    fn describe(&self) -> String {
        "emulator moshi".into()
    }
}
