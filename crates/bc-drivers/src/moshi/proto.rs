//! Protokół Moshi MS10105 V4.x (REQ-13-051, 052; ADR 0014 §3): kody poleceń przez
//! permutację bitów, program = nagłówek prędkości + przesunięcie + ruchy + zakończenie,
//! wysyłka w pakietach po 32 bajty między prologiem a epilogiem (zapis adresu EPP).
//!
//! Źródło faktów: MeerK40t (MIT), moduł `moshi` (README, opis `builder.py` i
//! `controller.py`) — czytany jako opis bajtów, bez kopiowania kodu ani tablic (tablica
//! kodów jest liczona z reguły permutacji).

use bc_plan::{Job, Move, OpKind, SegKind};

use crate::lihuiyu::egv::Steps;
use crate::{CompileError, CompileWarning, Context, JobCompiler, MachineFile, ProgressFn};

/// Gotowe do przyjęcia programu. Źródło: README MeerK40t `moshi` (OK = 205).
pub const STATUS_READY: u8 = 205;
/// Wykonuje program. Źródło: README MeerK40t (PROCESSING = 207).
pub const STATUS_PROCESSING: u8 = 207;
/// Błąd. Źródło: README MeerK40t (ERROR = 237).
pub const STATUS_ERROR: u8 = 237;
/// Reset. Źródło: README MeerK40t (RESET = 239).
pub const STATUS_RESET: u8 = 239;

/// Rozmiar pakietu danych programu. Źródło: MeerK40t `moshi/controller.py` (32 bajty).
pub const PACKET: usize = 32;

/// Permutacja bitów bajtu kodu: bajty nieparzyste (b7…b0) ← (7, 6, 2, 4, 3, 5, 1, 0),
/// parzyste ← (5, 1, 7, 2, 4, 3, 6, 0). Źródło: MeerK40t `moshi/builder.py` (opis).
pub fn swizzle(v: u8) -> u8 {
    let perm: [u8; 8] = if v & 1 == 1 {
        [7, 6, 2, 4, 3, 5, 1, 0]
    } else {
        [5, 1, 7, 2, 4, 3, 6, 0]
    };
    let mut o = 0u8;
    for (k, src) in perm.iter().enumerate() {
        if (v >> src) & 1 == 1 {
            o |= 1 << (7 - k);
        }
    }
    o
}

/// Odwrotność [`swizzle`].
pub fn unswizzle(b: u8) -> u8 {
    (0..=255u8).find(|v| swizzle(*v) == b).unwrap_or(0)
}

/// Bajt polecenia: numer polecenia (wiersz) i kolumna.
pub fn opcode(cmd: u8, col: u8) -> u8 {
    swizzle((cmd << 4) | (col & 0x0F))
}

// Numery poleceń (wiersze tablicy kodów). Źródło: MeerK40t `moshi/builder.py` (opis).
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
/// Cięcie do punktu (kolumna 1).
pub const CMD_CUT_ABS: u8 = 15;
/// Prolog przed programem (zapis adresu): polecenie 6. Źródło: MeerK40t.
pub const CMD_PROLOGUE: u8 = 6;
/// Epilog po programie (zapis adresu): polecenie 2. Źródło: MeerK40t.
pub const CMD_EPILOGUE: u8 = 2;

/// Polecenie programu Moshi.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cmd {
    /// Prędkość cięcia i przejazdów (bajt = mm/s − 1).
    VectorSpeed {
        cut: u8,
        normal: u8,
    },
    /// Prędkość rastra (bajt = cm/s − 1).
    RasterSpeed(u8),
    Offset {
        z: i16,
        x: i16,
        y: i16,
    },
    MoveAbs {
        x: i16,
        y: i16,
    },
    MoveH(i16),
    MoveV(i16),
    CutAbs {
        x: i16,
        y: i16,
    },
    CutH(i16),
    CutV(i16),
    Terminate,
}

impl Cmd {
    /// Bajty polecenia (liczby: int16 little-endian, prędkości: u8). Źródło: MeerK40t.
    pub fn bytes(&self) -> Vec<u8> {
        let mut v = Vec::new();
        let i16le = |v: &mut Vec<u8>, x: i16| v.extend_from_slice(&x.to_le_bytes());
        match *self {
            Cmd::VectorSpeed { cut, normal } => {
                v.push(opcode(CMD_VECTOR_SPEED, 0));
                v.push(cut);
                v.push(normal);
            }
            Cmd::RasterSpeed(s) => {
                v.push(opcode(CMD_RASTER_SPEED, 0));
                v.push(s);
            }
            Cmd::Offset { z, x, y } => {
                v.push(opcode(CMD_SET_OFFSET, 0));
                i16le(&mut v, z);
                i16le(&mut v, x);
                i16le(&mut v, y);
            }
            Cmd::MoveAbs { x, y } => {
                v.push(opcode(CMD_MOVE_ABS, 0));
                i16le(&mut v, x);
                i16le(&mut v, y);
            }
            Cmd::MoveH(x) => {
                v.push(opcode(CMD_MOVE_H_ABS, 0));
                i16le(&mut v, x);
            }
            Cmd::MoveV(y) => {
                v.push(opcode(CMD_MOVE_V_ABS, 0));
                i16le(&mut v, y);
            }
            Cmd::CutAbs { x, y } => {
                v.push(opcode(CMD_CUT_ABS, 1));
                i16le(&mut v, x);
                i16le(&mut v, y);
            }
            Cmd::CutH(x) => {
                v.push(opcode(CMD_CUT_H_ABS, 0));
                i16le(&mut v, x);
            }
            Cmd::CutV(y) => {
                v.push(opcode(CMD_CUT_V_ABS, 0));
                i16le(&mut v, y);
            }
            Cmd::Terminate => v.push(opcode(CMD_TERMINATION, 0)),
        }
        v
    }
}

/// Zakończenie programu: 7 × polecenie 2. Źródło: MeerK40t `moshi/builder.py`.
pub const TERMINATORS: usize = 7;

/// Rozbiór bajtów na polecenia.
pub fn parse(bytes: &[u8]) -> Result<Vec<Cmd>, String> {
    let mut out = Vec::new();
    let mut i = 0;
    let rd = |i: usize| -> Result<i16, String> {
        bytes
            .get(i..i + 2)
            .map(|b| i16::from_le_bytes([b[0], b[1]]))
            .ok_or_else(|| format!("bajt {i}: urwane polecenie"))
    };
    while i < bytes.len() {
        let v = unswizzle(bytes[i]);
        let (cmd, col) = (v >> 4, v & 0x0F);
        let c = match (cmd, col) {
            (CMD_VECTOR_SPEED, 0) => {
                let b = bytes.get(i + 1..i + 3).ok_or("urwana prędkość")?;
                i += 3;
                Cmd::VectorSpeed {
                    cut: b[0],
                    normal: b[1],
                }
            }
            (CMD_RASTER_SPEED, 0) => {
                let b = *bytes.get(i + 1).ok_or("urwana prędkość")?;
                i += 2;
                Cmd::RasterSpeed(b)
            }
            (CMD_SET_OFFSET, 0) => {
                let c = Cmd::Offset {
                    z: rd(i + 1)?,
                    x: rd(i + 3)?,
                    y: rd(i + 5)?,
                };
                i += 7;
                c
            }
            (CMD_MOVE_ABS, 0) => {
                let c = Cmd::MoveAbs {
                    x: rd(i + 1)?,
                    y: rd(i + 3)?,
                };
                i += 5;
                c
            }
            (CMD_CUT_ABS, 1) => {
                let c = Cmd::CutAbs {
                    x: rd(i + 1)?,
                    y: rd(i + 3)?,
                };
                i += 5;
                c
            }
            (CMD_MOVE_H_ABS, 0) => {
                let c = Cmd::MoveH(rd(i + 1)?);
                i += 3;
                c
            }
            (CMD_MOVE_V_ABS, 0) => {
                let c = Cmd::MoveV(rd(i + 1)?);
                i += 3;
                c
            }
            (CMD_CUT_H_ABS, 0) => {
                let c = Cmd::CutH(rd(i + 1)?);
                i += 3;
                c
            }
            (CMD_CUT_V_ABS, 0) => {
                let c = Cmd::CutV(rd(i + 1)?);
                i += 3;
                c
            }
            (CMD_TERMINATION, 0) => {
                i += 1;
                Cmd::Terminate
            }
            _ => return Err(format!("bajt {i}: nieznane polecenie 0x{:02X}", bytes[i])),
        };
        out.push(c);
    }
    Ok(out)
}

/// Podział pliku na programy (każdy kończy się 7 poleceniami zakończenia).
pub fn programs(bytes: &[u8]) -> Result<Vec<Vec<u8>>, String> {
    let mut out = Vec::new();
    let mut cur = Vec::new();
    let mut terms = 0;
    for c in parse(bytes)? {
        cur.extend(c.bytes());
        if c == Cmd::Terminate {
            terms += 1;
            if terms == TERMINATORS {
                out.push(std::mem::take(&mut cur));
                terms = 0;
            }
        } else {
            terms = 0;
        }
    }
    if !cur.is_empty() {
        return Err("program bez zakończenia".into());
    }
    Ok(out)
}

/// Bajt prędkości cięcia (mm/s − 1, 1…256). Źródło: MeerK40t `moshi/builder.py`.
pub fn vector_speed_byte(mm_s: f64) -> u8 {
    (mm_s.round().clamp(1.0, 256.0) - 1.0) as u8
}

/// Bajt prędkości rastra (cm/s − 1, co najmniej 1 cm/s). Źródło: MeerK40t.
pub fn raster_speed_byte(mm_s: f64) -> u8 {
    ((mm_s / 10.0).round().clamp(1.0, 256.0) - 1.0) as u8
}

/// Prędkość przejazdów (bajt „normal”) — domyślnie 40 mm/s (MeerK40t `rapid_speed`).
pub const RAPID_SPEED: f64 = 40.0;

/// Kompilator programu Moshi (sterownik `moshi`): jeden program na operację.
pub struct MoshiCompiler;

fn c16(v: i64) -> i16 {
    v.clamp(0, i16::MAX as i64) as i16
}

impl JobCompiler for MoshiCompiler {
    fn name(&self) -> &'static str {
        "moshi"
    }
    fn extension(&self) -> &'static str {
        "mos"
    }
    fn compile(
        &self,
        job: &Job,
        ctx: &Context,
        progress: &mut ProgressFn,
    ) -> Result<MachineFile, CompileError> {
        let cfg = &ctx.k40;
        cfg.validate().map_err(CompileError::Invalid)?;
        let st = Steps::new(cfg, ctx.bed[1]);
        let mut out = Vec::new();
        let mut warnings: Vec<CompileWarning> = Vec::new();
        let mut warned = std::collections::BTreeSet::new();
        let mut warn = |code: &str, m: String| {
            if warned.insert(code.to_string()) {
                warnings.push(CompileWarning::new(code, m));
            }
        };
        let segs = job.segments();
        let mut time = 0.0;
        let total = job.ops.len();
        for (oi, op) in job.ops.iter().enumerate() {
            progress(oi, total)?;
            let mine: Vec<_> = segs.iter().filter(|s| s.op == oi).collect();
            if mine.is_empty() {
                continue;
            }
            if op.z != 0.0 {
                warn("z_ignored", "Moshi: brak osi Z — Z pominięte".into());
            }
            let max_p = op
                .moves
                .iter()
                .filter_map(|m| match m {
                    Move::Cut { power, .. } if *power > 0.0 => Some(*power),
                    _ => None,
                })
                .fold(0.0f64, f64::max);
            if max_p > 0.0 && max_p < 100.0 {
                warn(
                    "power_ignored",
                    format!(
                        "Moshi nie ma PWM: moc ustawia pokrętło lasera; {max_p:.0} % pominięte"
                    ),
                );
            }
            let speed = op.settings.speed.max(0.1);
            let raster = op.kind == OpKind::Raster || op.mode == bc_doc::LayerMode::Fill;
            let mut prog = Vec::new();
            if raster {
                prog.extend(Cmd::RasterSpeed(raster_speed_byte(speed)).bytes());
            } else {
                if speed > 256.0 {
                    warn("speed_clamped", "Moshi: prędkość do 256 mm/s".into());
                }
                prog.extend(
                    Cmd::VectorSpeed {
                        cut: vector_speed_byte(speed),
                        normal: vector_speed_byte(RAPID_SPEED),
                    }
                    .bytes(),
                );
            }
            prog.extend(Cmd::Offset { z: 0, x: 0, y: 0 }.bytes());
            let s0 = st.of(mine[0].from);
            prog.extend(
                Cmd::MoveAbs {
                    x: c16(s0.0),
                    y: c16(s0.1),
                }
                .bytes(),
            );
            let mut at = s0;
            for s in &mine {
                let to = st.of(s.to);
                if to.0 < 0 || to.1 < 0 || to.0 > i16::MAX as i64 || to.1 > i16::MAX as i64 {
                    warn(
                        "outside_bed",
                        "Moshi: ruch poza stołem — współrzędne ograniczone".into(),
                    );
                }
                let len = s.from.distance(s.to);
                let c = match s.kind {
                    SegKind::Dwell => {
                        warn("dwell_ignored", "Moshi: postój pominięty".into());
                        continue;
                    }
                    SegKind::Travel => {
                        time += len / RAPID_SPEED;
                        Cmd::MoveAbs {
                            x: c16(to.0),
                            y: c16(to.1),
                        }
                    }
                    SegKind::Cut => {
                        time += len / speed;
                        let on = s.power > 0.0;
                        match (to.0 == at.0, to.1 == at.1, on) {
                            (_, true, true) => Cmd::CutH(c16(to.0)),
                            (true, _, true) => Cmd::CutV(c16(to.1)),
                            (_, _, true) => Cmd::CutAbs {
                                x: c16(to.0),
                                y: c16(to.1),
                            },
                            (_, true, false) => Cmd::MoveH(c16(to.0)),
                            (true, _, false) => Cmd::MoveV(c16(to.1)),
                            _ => Cmd::MoveAbs {
                                x: c16(to.0),
                                y: c16(to.1),
                            },
                        }
                    }
                };
                if to == at {
                    continue;
                }
                prog.extend(c.bytes());
                at = to;
            }
            for _ in 0..TERMINATORS {
                prog.extend(Cmd::Terminate.bytes());
            }
            out.extend(prog);
        }
        progress(total, total)?;
        Ok(MachineFile::from_bytes("moshi", "mos", out, time, warnings))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn swizzle_matches_source_rows() {
        // pierwsze bajty wierszy 0–2 tablicy kodów z README MeerK40t `moshi`
        assert_eq!(
            [swizzle(0x00), swizzle(0x01), swizzle(0x02)],
            [0x00, 0x01, 0x40]
        );
        assert_eq!(
            [swizzle(0x10), swizzle(0x11), swizzle(0x12)],
            [0x08, 0x11, 0x48]
        );
        assert_eq!(
            [swizzle(0x20), swizzle(0x21), swizzle(0x22)],
            [0x80, 0x05, 0xC0]
        );
        // kody poleceń z opisu builder.py
        assert_eq!(opcode(CMD_VECTOR_SPEED, 0), 0x0A);
        assert_eq!(opcode(CMD_MOVE_ABS, 0), 0x8A);
        assert_eq!(opcode(CMD_CUT_ABS, 1), 0xD5);
        assert_eq!(opcode(CMD_TERMINATION, 0), 0x80);
        assert_eq!(opcode(CMD_ESTOP, 0), 0x08);
        for v in 0..=255u8 {
            assert_eq!(unswizzle(swizzle(v)), v);
        }
    }
}
