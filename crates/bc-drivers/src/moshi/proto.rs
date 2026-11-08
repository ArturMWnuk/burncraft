//! Protokół Moshi MS10105 V4.x (ADR 0014). Szkielet API (M3.3, testy red) — bez implementacji.

use bc_plan::Job;

use crate::{CompileError, Context, JobCompiler, MachineFile, ProgressFn};

pub const STATUS_READY: u8 = 205;
pub const STATUS_PROCESSING: u8 = 207;
pub const STATUS_ERROR: u8 = 237;
pub const STATUS_RESET: u8 = 239;
pub const PACKET: usize = 32;

pub fn swizzle(v: u8) -> u8 {
    v
}

pub fn unswizzle(b: u8) -> u8 {
    b
}

pub fn opcode(cmd: u8, col: u8) -> u8 {
    (cmd << 4) | col
}

pub const CMD_SET_OFFSET: u8 = 0;
pub const CMD_ESTOP: u8 = 1;
pub const CMD_TERMINATION: u8 = 2;
pub const CMD_MOVE_V_ABS: u8 = 3;
pub const CMD_RASTER_SPEED: u8 = 4;
pub const CMD_VECTOR_SPEED: u8 = 5;
pub const CMD_MOVE_H_ABS: u8 = 6;
pub const CMD_MOVE_ABS: u8 = 7;
pub const CMD_CUT_V_ABS: u8 = 11;
pub const CMD_CUT_H_ABS: u8 = 14;
pub const CMD_CUT_ABS: u8 = 15;
pub const CMD_PROLOGUE: u8 = 6;
pub const CMD_EPILOGUE: u8 = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cmd {
    VectorSpeed { cut: u8, normal: u8 },
    RasterSpeed(u8),
    Offset { z: i16, x: i16, y: i16 },
    MoveAbs { x: i16, y: i16 },
    MoveH(i16),
    MoveV(i16),
    CutAbs { x: i16, y: i16 },
    CutH(i16),
    CutV(i16),
    Terminate,
}

impl Cmd {
    pub fn bytes(&self) -> Vec<u8> {
        Vec::new()
    }
}

pub const TERMINATORS: usize = 7;

pub fn parse(_bytes: &[u8]) -> Result<Vec<Cmd>, String> {
    Err("moshi: niezaimplementowane (M3.3)".into())
}

pub fn programs(_bytes: &[u8]) -> Result<Vec<Vec<u8>>, String> {
    Err("moshi: niezaimplementowane (M3.3)".into())
}

pub fn vector_speed_byte(_mm_s: f64) -> u8 {
    0
}

pub fn raster_speed_byte(_mm_s: f64) -> u8 {
    0
}

pub const RAPID_SPEED: f64 = 40.0;

/// Kompilator programu Moshi (sterownik `moshi`).
pub struct MoshiCompiler;

impl JobCompiler for MoshiCompiler {
    fn name(&self) -> &'static str {
        "moshi"
    }
    fn extension(&self) -> &'static str {
        "mos"
    }
    fn compile(
        &self,
        _job: &Job,
        _ctx: &Context,
        _progress: &mut ProgressFn,
    ) -> Result<MachineFile, CompileError> {
        Err(CompileError::Invalid(
            "moshi: niezaimplementowane (M3.3)".into(),
        ))
    }
}
