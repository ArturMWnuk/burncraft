//! Emulator płyty Lihuiyu (REQ-19-009). Szkielet API (M3.3, testy red) — bez implementacji.

use std::collections::VecDeque;

use super::egv::Steps;
use super::Board;
use crate::ch341::Ch341;
use crate::transport::TransportError;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EgvMove {
    pub from: (i64, i64),
    pub to: (i64, i64),
    pub on: bool,
    pub rapid: bool,
    pub speed: f64,
}

/// Interpreter strumienia EGV.
#[derive(Debug, Clone)]
pub struct Interp {
    pub board: Board,
    pub pos: (i64, i64),
    pub compact: bool,
    pub laser: bool,
    pub paused: bool,
    pub finished: bool,
    pub speed: f64,
    pub raster_step: u32,
    pub horizontal_major: bool,
    pub speeds: Vec<f64>,
    pub homes: usize,
    pub rail_unlocked: bool,
    pub rapid_speed: f64,
    pub moves: VecDeque<EgvMove>,
}

impl Interp {
    pub fn new(board: Board) -> Self {
        Interp {
            board,
            pos: (0, 0),
            compact: false,
            laser: false,
            paused: false,
            finished: false,
            speed: 0.0,
            raster_step: 0,
            horizontal_major: false,
            speeds: vec![0.0],
            homes: 0,
            rail_unlocked: false,
            rapid_speed: 0.0,
            moves: VecDeque::new(),
        }
    }
    pub fn byte(&mut self, _b: u8) {}
    pub fn feed(&mut self, _bytes: &[u8]) {}
}

#[derive(Debug, Clone)]
pub struct EmuConfig {
    pub board: Board,
    pub serial: Option<String>,
    pub speed: f64,
    pub buffer: usize,
}

impl Default for EmuConfig {
    fn default() -> Self {
        EmuConfig {
            board: Board::M2,
            serial: None,
            speed: f64::INFINITY,
            buffer: 30 * 64,
        }
    }
}

/// Emulator płyty Lihuiyu za interfejsem [`Ch341`].
pub struct LihuiyuEmulator {
    pub cfg: EmuConfig,
    pub interp: Interp,
    pub trace: Vec<EgvMove>,
    pub accepted: usize,
    pub crc_errors: usize,
    pub busy: usize,
    pub received: Vec<u8>,
    pub serial_ok: bool,
    pub power: Option<f64>,
    pub corrupt_next: usize,
    pub addr_writes: usize,
}

impl LihuiyuEmulator {
    pub fn new(cfg: EmuConfig) -> Self {
        LihuiyuEmulator {
            interp: Interp::new(cfg.board),
            cfg,
            trace: Vec::new(),
            accepted: 0,
            crc_errors: 0,
            busy: 0,
            received: Vec::new(),
            serial_ok: false,
            power: None,
            corrupt_next: 0,
            addr_writes: 0,
        }
    }
    pub fn shared(cfg: EmuConfig) -> std::sync::Arc<std::sync::Mutex<Self>> {
        std::sync::Arc::new(std::sync::Mutex::new(Self::new(cfg)))
    }
    pub fn advance(&mut self, _dt: f64) {}
    pub fn running(&self) -> bool {
        true
    }
    pub fn pos(&self) -> (i64, i64) {
        (0, 0)
    }
    pub fn trace_job(&self, st: &Steps) -> bc_plan::Job {
        trace_job(&self.trace, st)
    }
}

pub fn trace_job(_trace: &[EgvMove], _st: &Steps) -> bc_plan::Job {
    bc_plan::Job::empty(bc_geom::Point::ORIGIN)
}

impl Ch341 for LihuiyuEmulator {
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
        "emulator lihuiyu".into()
    }
}
