//! Protokół EGV płyt Lihuiyu (ADR 0014). Szkielet API (M3.3, testy red) — bez implementacji.

use bc_plan::Job;

use super::{Board, K40Settings};
use crate::{CompileError, Context, JobCompiler, MachineFile, ProgressFn};

pub const STATUS_OK: u8 = 206;
pub const STATUS_BUSY: u8 = 238;
pub const STATUS_CRC_ERROR: u8 = 207;
pub const STATUS_FINISH: u8 = 236;
pub const STATUS_SERIAL_OK: u8 = 204;
pub const STATUS_POWER: u8 = 239;

pub fn status_name(_s: u8) -> &'static str {
    "?"
}

pub const PAYLOAD: usize = 30;
pub const PAD: u8 = b'F';

pub fn crc8(_data: &[u8]) -> u8 {
    0
}

pub fn frame(_payload: &[u8; PAYLOAD]) -> [u8; 32] {
    [0; 32]
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Packet {
    pub data: [u8; PAYLOAD],
    pub wait_finish: bool,
}

pub fn packets(_stream: &[u8]) -> Vec<Packet> {
    Vec::new()
}

pub fn distance(_steps: u64) -> Vec<u8> {
    Vec::new()
}

#[derive(Debug, Clone, PartialEq)]
pub struct SpeedCode {
    pub text: String,
    pub speed: f64,
    pub accel: u8,
    pub suffix_c: bool,
    pub clamped: bool,
}

pub const MAX_VECTOR_SPEED: f64 = 240.0;
pub const SUFFIX_C_BELOW: f64 = 7.0;

pub fn accel(_speed: f64, _raster: bool) -> u8 {
    0
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Group {
    Ab,
    Default,
    B2,
}

pub fn equation(_board: Board, _accel: u8, _suffix_c: bool) -> (f64, f64) {
    (0.0, 0.0)
}

pub fn speed_code(_board: Board, speed: f64, _raster_step: Option<u32>) -> SpeedCode {
    SpeedCode {
        text: "?".repeat(16),
        speed,
        accel: 0,
        suffix_c: false,
        clamped: false,
    }
}

pub fn decode_speed(_board: Board, _value: u16, _accel: u8, _suffix_c: bool) -> f64 {
    0.0
}

pub fn dir_letter(_axis_x: bool, _positive: bool) -> u8 {
    b'?'
}

/// Generator strumienia EGV.
#[derive(Default)]
pub struct Egv {
    pub out: Vec<u8>,
    pub pos: (i64, i64),
}

impl Egv {
    pub fn new(pos: (i64, i64)) -> Self {
        Egv {
            out: Vec::new(),
            pos,
        }
    }
    pub fn rapid(&mut self, _to: (i64, i64)) {}
    pub fn enter(&mut self, _speed: &str, _start: (i64, i64), _horizontal_major: bool) {}
    pub fn enter_with_dirs(&mut self, _speed: &str, _start: (i64, i64), _x: bool, _y: bool) {}
    pub fn leave(&mut self, _finish: bool) {}
    pub fn in_program(&self) -> bool {
        false
    }
    pub fn line(&mut self, _to: (i64, i64), _on: bool) {}
    pub fn raster_reverse(&mut self, _raster_step: i64) {}
    pub fn x_positive(&self) -> bool {
        true
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Steps {
    pub kx: f64,
    pub ky: f64,
    pub bed_h: f64,
}

impl Steps {
    pub fn new(_s: &K40Settings, bed_h: f64) -> Self {
        Steps {
            kx: 1.0,
            ky: 1.0,
            bed_h,
        }
    }
    pub fn of(&self, _p: bc_geom::Point) -> (i64, i64) {
        (0, 0)
    }
    pub fn mm(&self, _s: (f64, f64)) -> bc_geom::Point {
        bc_geom::Point::ORIGIN
    }
}

pub const JOG_MINIMUM_STEPS: i64 = 256;

pub fn pwm_command(_power_pct: f64) -> Vec<u8> {
    Vec::new()
}

/// Kompilator EGV (sterownik `lihuiyu`).
pub struct EgvCompiler;

impl JobCompiler for EgvCompiler {
    fn name(&self) -> &'static str {
        "lihuiyu"
    }
    fn extension(&self) -> &'static str {
        "egv"
    }
    fn compile(
        &self,
        _job: &Job,
        _ctx: &Context,
        _progress: &mut ProgressFn,
    ) -> Result<MachineFile, CompileError> {
        Err(CompileError::Invalid(
            "lihuiyu: niezaimplementowane (M3.3)".into(),
        ))
    }
}
