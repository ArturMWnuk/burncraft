//! Emulator płyty Moshi MS10105 (ADR 0014 §4): prolog/epilog przez zapis adresu EPP,
//! program w pakietach danych, stany READY/PROCESSING/ERROR, wykonanie w czasie
//! wirtualnym i tor ruchu (te same ruchy co emulator Lihuiyu → PNG).

use std::collections::VecDeque;
use std::time::Instant;

use super::proto::{self, Cmd};
use crate::ch341::Ch341;
use crate::lihuiyu::emu::EgvMove;
use crate::transport::TransportError;

/// Konfiguracja emulatora Moshi.
#[derive(Debug, Clone)]
pub struct EmuConfig {
    /// Mnożnik czasu (`f64::INFINITY` = natychmiast).
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
    receiving: bool,
    buf: Vec<u8>,
    run: VecDeque<EgvMove>,
    cur: Option<(EgvMove, f64)>,
    pos: (i64, i64),
    last: Instant,
    error: bool,
    /// Wykonane ruchy.
    pub trace: Vec<EgvMove>,
    /// Programy przyjęte (każdy między prologiem a epilogiem).
    pub programs: Vec<Vec<u8>>,
    /// Długości zapisów danych (pakiety).
    pub packets: Vec<usize>,
    pub estops: usize,
}

impl MoshiEmulator {
    pub fn new(cfg: EmuConfig) -> Self {
        MoshiEmulator {
            cfg,
            receiving: false,
            buf: Vec::new(),
            run: VecDeque::new(),
            cur: None,
            pos: (0, 0),
            last: Instant::now(),
            error: false,
            trace: Vec::new(),
            programs: Vec::new(),
            packets: Vec::new(),
            estops: 0,
        }
    }

    pub fn shared(cfg: EmuConfig) -> std::sync::Arc<std::sync::Mutex<Self>> {
        std::sync::Arc::new(std::sync::Mutex::new(Self::new(cfg)))
    }

    /// Wykonuje odebrany program: ruchy do kolejki.
    fn load(&mut self, prog: &[u8]) {
        let Ok(cmds) = proto::parse(prog) else {
            self.error = true;
            return;
        };
        let (mut cut, mut normal) = (10.0, proto::RAPID_SPEED);
        let mut off = (0i64, 0i64);
        let mut at = self.run.back().map_or(self.pos, |m| m.to);
        for c in cmds {
            let mut go = |to: (i64, i64), on: bool, rapid: bool, at: &mut (i64, i64)| {
                if to != *at {
                    self.run.push_back(EgvMove {
                        from: *at,
                        to,
                        on,
                        rapid,
                        speed: if rapid { normal } else { cut },
                    });
                    *at = to;
                }
            };
            match c {
                Cmd::VectorSpeed { cut: c, normal: n } => {
                    cut = c as f64 + 1.0;
                    normal = n as f64 + 1.0;
                }
                Cmd::RasterSpeed(s) => cut = (s as f64 + 1.0) * 10.0,
                Cmd::Offset { x, y, .. } => off = (x as i64, y as i64),
                Cmd::MoveAbs { x, y } => {
                    go((x as i64 + off.0, y as i64 + off.1), false, true, &mut at)
                }
                Cmd::MoveH(x) => go((x as i64 + off.0, at.1), false, false, &mut at),
                Cmd::MoveV(y) => go((at.0, y as i64 + off.1), false, false, &mut at),
                Cmd::CutAbs { x, y } => {
                    go((x as i64 + off.0, y as i64 + off.1), true, false, &mut at)
                }
                Cmd::CutH(x) => go((x as i64 + off.0, at.1), true, false, &mut at),
                Cmd::CutV(y) => go((at.0, y as i64 + off.1), true, false, &mut at),
                Cmd::Terminate => {}
            }
        }
    }

    pub fn advance(&mut self, dt: f64) {
        let mut budget = dt * self.cfg.speed;
        loop {
            if self.cur.is_none() {
                let Some(m) = self.run.pop_front() else {
                    return;
                };
                let len = (((m.to.0 - m.from.0) as f64).powi(2)
                    + ((m.to.1 - m.from.1) as f64).powi(2))
                .sqrt()
                    * 25.4
                    / 1000.0;
                self.cur = Some((m, len / m.speed.max(0.01)));
            }
            let (m, t) = self.cur.expect("ruch");
            if budget.is_infinite() || budget >= t {
                if budget.is_finite() {
                    budget -= t;
                }
                self.pos = m.to;
                self.trace.push(m);
                self.cur = None;
            } else {
                self.cur = Some((m, t - budget));
                return;
            }
        }
    }

    fn tick(&mut self) {
        let now = Instant::now();
        let dt = (now - self.last).as_secs_f64();
        self.last = now;
        self.advance(dt);
    }

    pub fn running(&self) -> bool {
        self.cur.is_some() || !self.run.is_empty()
    }
}

impl Ch341 for MoshiEmulator {
    fn write_data(&mut self, data: &[u8]) -> Result<(), TransportError> {
        self.tick();
        self.packets.push(data.len());
        if self.receiving {
            self.buf.extend_from_slice(data);
        } else {
            self.error = true;
        }
        Ok(())
    }

    fn write_addr(&mut self, data: &[u8]) -> Result<(), TransportError> {
        self.tick();
        for &b in data {
            match proto::unswizzle(b) >> 4 {
                proto::CMD_PROLOGUE => {
                    self.receiving = true;
                    self.buf.clear();
                    self.error = false;
                }
                proto::CMD_EPILOGUE => {
                    self.receiving = false;
                    let prog = std::mem::take(&mut self.buf);
                    self.load(&prog);
                    self.programs.push(prog);
                }
                proto::CMD_ESTOP => {
                    // zatrzymanie: kolejka ruchów czyszczona
                    self.estops += 1;
                    self.run.clear();
                    if let Some((m, _)) = self.cur.take() {
                        self.pos = m.from;
                    }
                    self.receiving = false;
                    self.buf.clear();
                }
                _ => self.error = true,
            }
        }
        Ok(())
    }

    fn status(&mut self) -> Result<[u8; 6], TransportError> {
        self.tick();
        let code = if self.error {
            proto::STATUS_ERROR
        } else if self.running() || self.receiving {
            proto::STATUS_PROCESSING
        } else {
            proto::STATUS_READY
        };
        Ok([0xFF, code, 0, 0, 0, 0])
    }

    fn describe(&self) -> String {
        "emulator moshi".into()
    }
}
