//! Emulator płyty Lihuiyu (REQ-19-009, ADR 0014 §4): pakiety 30 bajtów z CRC-8, stany
//! OK/BUSY/CRC/zadanie zakończone, bufor wejściowy z czasem wirtualnym, interpreter EGV
//! (kierunki, odległości, kody prędkości, program, raster z krokiem G, bazowanie, pauza)
//! i tor ruchu do PNG. Model zachowania płyty opisuje ADR 0014; szczegóły niepotwierdzone
//! na sprzęcie są `UNVERIFIED` (test K40 w M3.4).

use std::collections::VecDeque;
use std::time::Instant;

use super::egv::{self, Steps, PAYLOAD};
use super::Board;
use crate::ch341::Ch341;
use crate::transport::TransportError;

/// Ruch wykonany przez płytę (kroki, Y w dół od narożnika bazowania).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EgvMove {
    pub from: (i64, i64),
    pub to: (i64, i64),
    /// Wiązka włączona.
    pub on: bool,
    /// Przejazd szybki (tryb domyślny).
    pub rapid: bool,
    /// mm/s (z kodu prędkości; przejazd szybki: prędkość płyty).
    pub speed: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Ax {
    X,
    Y,
    Diag,
}

#[derive(Debug, Clone, Default)]
struct SpeedParse {
    digits: String,
    raster: Option<String>,
    diag: bool,
    done_main: bool,
}

/// Interpreter strumienia EGV (emulator i śledzenie pozycji w sterowniku).
#[derive(Debug, Clone)]
pub struct Interp {
    pub board: Board,
    pub pos: (i64, i64),
    pub compact: bool,
    pub laser: bool,
    pub paused: bool,
    pub finished: bool,
    /// Prędkość z ostatniego kodu, mm/s.
    pub speed: f64,
    /// Krok rastra `G` (0 = brak).
    pub raster_step: u32,
    pub horizontal_major: bool,
    /// Kody prędkości (mm/s) w kolejności.
    pub speeds: Vec<f64>,
    pub homes: usize,
    pub rail_unlocked: bool,
    /// Prędkość przejazdów szybkich płyty, mm/s.
    pub rapid_speed: f64,
    xpos: bool,
    ypos: bool,
    axis: Ax,
    dist: u64,
    pending_xy: (i64, i64),
    number: String,
    pipe: bool,
    s_wait: bool,
    mode: Option<u8>,
    finish_flag: bool,
    last: [u8; 2],
    speed_parse: Option<SpeedParse>,
    skip_digits: usize,
    g_digits: Option<String>,
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
            speed: 10.0,
            raster_step: 0,
            horizontal_major: false,
            speeds: Vec::new(),
            homes: 0,
            rail_unlocked: false,
            // UNVERIFIED: prędkość przejazdów szybkich płyty (zależna od firmware) — tylko czas
            rapid_speed: 100.0,
            xpos: true,
            ypos: true,
            axis: Ax::X,
            dist: 0,
            pending_xy: (0, 0),
            number: String::new(),
            pipe: false,
            s_wait: false,
            mode: None,
            finish_flag: false,
            last: [0; 2],
            speed_parse: None,
            skip_digits: 0,
            g_digits: None,
            moves: VecDeque::new(),
        }
    }

    fn push(&mut self, to: (i64, i64), rapid: bool) {
        if to == self.pos {
            return;
        }
        let m = EgvMove {
            from: self.pos,
            to,
            on: self.laser && !rapid,
            rapid,
            speed: if rapid { self.rapid_speed } else { self.speed },
        };
        self.pos = to;
        self.moves.push_back(m);
    }

    fn sign(p: bool) -> i64 {
        if p {
            1
        } else {
            -1
        }
    }

    /// Wykonuje zgromadzoną odległość.
    fn exec(&mut self) {
        self.number.clear();
        if self.compact {
            if self.dist == 0 {
                return;
            }
            let d = self.dist as i64;
            self.dist = 0;
            let (dx, dy) = match self.axis {
                Ax::X => (Self::sign(self.xpos) * d, 0),
                Ax::Y => (0, Self::sign(self.ypos) * d),
                Ax::Diag => (Self::sign(self.xpos) * d, Self::sign(self.ypos) * d),
            };
            let to = (self.pos.0 + dx, self.pos.1 + dy);
            self.push(to, false);
        } else {
            self.flush_default();
            if self.pending_xy != (0, 0) {
                let to = (
                    self.pos.0 + self.pending_xy.0,
                    self.pos.1 + self.pending_xy.1,
                );
                self.pending_xy = (0, 0);
                self.push(to, true);
            }
        }
    }

    /// Tryb domyślny: odległość dopisana do osi bieżącej litery.
    fn flush_default(&mut self) {
        if self.dist == 0 {
            return;
        }
        let d = self.dist as i64;
        self.dist = 0;
        match self.axis {
            Ax::X => self.pending_xy.0 += Self::sign(self.xpos) * d,
            Ax::Y => self.pending_xy.1 += Self::sign(self.ypos) * d,
            Ax::Diag => {
                self.pending_xy.0 += Self::sign(self.xpos) * d;
                self.pending_xy.1 += Self::sign(self.ypos) * d;
            }
        }
    }

    fn speed_byte(&mut self, b: u8) -> bool {
        let Some(sp) = self.speed_parse.as_mut() else {
            return false;
        };
        if !sp.done_main {
            if b.is_ascii_digit() {
                sp.digits.push(b as char);
                if sp.digits.len() == 7 {
                    sp.done_main = true;
                }
                return true;
            }
            // niepełny kod — odrzucony
            self.speed_parse = None;
            return false;
        }
        if let Some(r) = sp.raster.as_mut() {
            if r.len() < 3 && b.is_ascii_digit() {
                r.push(b as char);
                if r.len() == 3 {
                    self.finish_speed(false);
                }
                return true;
            }
            self.finish_speed(false);
            return false;
        }
        if b == b'G' && sp.digits.len() == 7 {
            sp.raster = Some(String::new());
            return true;
        }
        if b.is_ascii_digit() && sp.digits.len() < 16 {
            sp.diag = true;
            sp.digits.push(b as char);
            return true;
        }
        if b == b'C' {
            self.finish_speed(true);
            return true;
        }
        self.finish_speed(false);
        false
    }

    fn finish_speed(&mut self, suffix_c: bool) {
        let Some(sp) = self.speed_parse.take() else {
            return;
        };
        let d = sp.digits.as_bytes();
        let n = |a: usize, k: usize| -> u32 {
            std::str::from_utf8(&d[a..a + k])
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(0)
        };
        let value = (n(0, 3) * 256 + n(3, 3)).min(65535) as u16;
        let accel = n(6, 1) as u8;
        let s = egv::decode_speed(self.board, value, accel, suffix_c);
        self.speed = s;
        self.speeds.push(s);
        // kod V zeruje krok rastra; G w kodzie ustawia go (MeerK40t parser.py)
        self.raster_step = sp
            .raster
            .as_deref()
            .and_then(|r| r.parse().ok())
            .unwrap_or(0);
    }

    /// Jeden bajt strumienia.
    pub fn byte(&mut self, b: u8) {
        if self.finished && b != b'I' {
            return;
        }
        if self.speed_byte(b) {
            return;
        }
        if let Some(g) = self.g_digits.as_mut() {
            if b.is_ascii_digit() && g.len() < 3 {
                g.push(b as char);
                if g.len() == 3 {
                    self.raster_step = g.parse().unwrap_or(0);
                    self.g_digits = None;
                }
                return;
            }
            self.g_digits = None;
        }
        if self.skip_digits > 0 && b.is_ascii_digit() {
            self.skip_digits -= 1;
            return;
        }
        self.skip_digits = 0;
        let was_pipe = self.pipe;
        self.pipe = false;
        match b {
            b'0'..=b'9' => {
                if self.s_wait {
                    self.s_wait = false;
                    self.mode = Some(b - b'0');
                } else {
                    self.number.push(b as char);
                    if self.number.len() == 3 {
                        self.dist += self.number.parse::<u64>().unwrap_or(0);
                        self.number.clear();
                    }
                }
            }
            b'a'..=b'y' => {
                self.number.clear();
                self.dist += (b - b'a' + 1) as u64;
            }
            b'z' => {
                self.number.clear();
                self.dist += if was_pipe { 26 } else { 255 };
            }
            b'|' => {
                self.number.clear();
                self.dist += 25;
                self.pipe = true;
            }
            _ => self.command(b),
        }
        if !b.is_ascii_digit() || self.mode.is_some() {
            self.last = [self.last[1], b];
        }
    }

    fn command(&mut self, b: u8) {
        if self.compact {
            self.exec();
        } else {
            self.flush_default();
        }
        if b != b'S' {
            self.s_wait = false;
        }
        match b {
            b'B' | b'T' => {
                let p = b == b'B';
                if self.compact
                    && self.raster_step > 0
                    && self.horizontal_major
                    && p != self.xpos
                    && self.axis == Ax::X
                {
                    // raster „step mode”: zmiana kierunku X przesuwa Y o krok G
                    let on = self.laser;
                    self.laser = false;
                    let to = (
                        self.pos.0,
                        self.pos.1 + Self::sign(self.ypos) * self.raster_step as i64,
                    );
                    self.push(to, false);
                    self.laser = on;
                }
                self.xpos = p;
                self.axis = Ax::X;
            }
            b'R' | b'L' => {
                let p = b == b'R';
                if self.compact
                    && self.raster_step > 0
                    && !self.horizontal_major
                    && p != self.ypos
                    && self.axis == Ax::Y
                {
                    let on = self.laser;
                    self.laser = false;
                    let to = (
                        self.pos.0 + Self::sign(self.xpos) * self.raster_step as i64,
                        self.pos.1,
                    );
                    self.push(to, false);
                    self.laser = on;
                }
                self.ypos = p;
                self.axis = Ax::Y;
            }
            b'M' => self.axis = Ax::Diag,
            b'D' => self.laser = true,
            b'U' => self.laser = false,
            b'I' => {
                self.compact = false;
                self.finished = false;
                self.paused = false;
                self.finish_flag = false;
                self.mode = None;
                self.dist = 0;
                self.pending_xy = (0, 0);
                self.laser = false;
            }
            b'S' => {
                self.exec();
                self.laser = false;
                self.s_wait = true;
                self.mode = None;
            }
            b'E' => {
                match self.mode {
                    Some(1) => {
                        self.compact = true;
                        self.horizontal_major = self.axis == Ax::X;
                    }
                    Some(_) => {}
                    None => {
                        self.exec();
                        self.compact = false;
                        if self.finish_flag {
                            self.finished = true;
                        }
                    }
                }
                self.finish_flag = false;
                self.mode = None;
            }
            b'N' => {
                self.exec();
                self.laser = false;
                self.compact = false;
            }
            b'P' => {
                self.exec();
                self.laser = false;
                if self.compact {
                    // UNVERIFIED: „PN” w programie wstrzymuje / wznawia wykonanie
                    self.paused = !self.paused;
                } else if self.last == *b"IP" {
                    // „IPP” — bazowanie do narożnika (0, 0). Źródło: MeerK40t driver.py
                    self.push((0, 0), true);
                    self.homes += 1;
                } else {
                    match self.mode {
                        Some(2) => self.rail_unlocked = true,
                        Some(1) => self.rail_unlocked = false,
                        _ => {}
                    }
                }
                self.mode = None;
            }
            b'F' => self.finish_flag = true,
            b'@' => self.finish_flag = false,
            b'V' => self.speed_parse = Some(SpeedParse::default()),
            b'G' => self.g_digits = Some(String::new()),
            b'W' => self.skip_digits = 3,
            _ => {}
        }
    }

    pub fn feed(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.byte(b);
        }
    }
}

/// Konfiguracja emulatora.
#[derive(Debug, Clone)]
pub struct EmuConfig {
    pub board: Board,
    /// Numer seryjny płyty (potwierdzenie `A` + MD5).
    pub serial: Option<String>,
    /// Mnożnik czasu (`f64::INFINITY` = natychmiast).
    pub speed: f64,
    /// Bufor wejściowy płyty, bajty. UNVERIFIED: rozmiar bufora płyty nieznany.
    pub buffer: usize,
    /// Zachowanie płyty M2 Nano zmierzone w teście sprzętowym K40 (2026-11-09, dziennik
    /// `k40_probe`): (1) dwa odczyty stanu tuż po zapisie zwracają stan sprzed zapisu,
    /// potem BUSY, potem wynik; stan po wyniku trwa (206 / 207) do następnego zapisu;
    /// (2) przyjęty pakiet płyta wykonuje dopiero po przyjęciu następnego.
    /// UNVERIFIED: pakiet zapisany, zanim płyta odpowiedziała na poprzedni, jest odrzucany
    /// (207) — tak modelujemy wyścig; zachowanie płyty w tym przypadku nie było mierzone.
    pub m2_nano_timing: bool,
}

impl Default for EmuConfig {
    fn default() -> Self {
        EmuConfig {
            board: Board::M2,
            serial: None,
            speed: f64::INFINITY,
            buffer: 30 * 64,
            m2_nano_timing: false,
        }
    }
}

/// Emulator płyty Lihuiyu za interfejsem [`Ch341`].
pub struct LihuiyuEmulator {
    pub cfg: EmuConfig,
    pub interp: Interp,
    input: VecDeque<u8>,
    pending: Option<[u8; PAYLOAD]>,
    ack: Option<u8>,
    cur: Option<(EgvMove, f64)>,
    last: Instant,
    /// Wykonane ruchy.
    pub trace: Vec<EgvMove>,
    /// Pakiety przyjęte / odrzucone przez CRC / odpowiedzi BUSY.
    pub accepted: usize,
    pub crc_errors: usize,
    pub busy: usize,
    /// Wszystkie przyjęte dane (bez nagłówków i CRC).
    pub received: Vec<u8>,
    pub serial_ok: bool,
    /// Moc PWM (M3), %.
    pub power: Option<f64>,
    /// Psuje sumę CRC kolejnych `n` pakietów (test ponowień).
    pub corrupt_next: usize,
    /// Zapisy adresu EPP (Lihuiyu ich nie używa).
    pub addr_writes: usize,
    /// Pakiety pominięte jako powtórzenie poprzedniego.
    pub duplicates: usize,
    last_payload: Option<[u8; PAYLOAD]>,
    /// `m2_nano_timing`: kolejne odczyty stanu po zapisie, ostatni zwrócony stan,
    /// pakiet przyjęty, czekający na następny, zapisy przed odpowiedzią płyty.
    script: VecDeque<u8>,
    last_status: u8,
    held: Option<[u8; PAYLOAD]>,
    pub collisions: usize,
}

impl LihuiyuEmulator {
    pub fn new(cfg: EmuConfig) -> Self {
        LihuiyuEmulator {
            interp: Interp::new(cfg.board),
            cfg,
            input: VecDeque::new(),
            pending: None,
            ack: None,
            cur: None,
            last: Instant::now(),
            trace: Vec::new(),
            accepted: 0,
            crc_errors: 0,
            busy: 0,
            received: Vec::new(),
            serial_ok: false,
            power: None,
            corrupt_next: 0,
            addr_writes: 0,
            duplicates: 0,
            last_payload: None,
            script: VecDeque::new(),
            last_status: egv::STATUS_OK,
            held: None,
            collisions: 0,
        }
    }

    pub fn shared(cfg: EmuConfig) -> std::sync::Arc<std::sync::Mutex<Self>> {
        std::sync::Arc::new(std::sync::Mutex::new(Self::new(cfg)))
    }

    fn room(&self) -> bool {
        self.input.len() + PAYLOAD <= self.cfg.buffer
    }

    fn accept(&mut self, data: [u8; PAYLOAD]) {
        self.accepted += 1;
        // Pakiet identyczny z poprzednim płyta potwierdza (OK), ale go nie wykonuje.
        // Zaobserwowane w teście sprzętowym K40 (M2 Nano, 2026-11-09): co drugi z kolejnych
        // jednakowych przejazdów `IRz139S1P` nie ruszał głowicy. UNVERIFIED: przyczyna
        // (prawdopodobnie płyta traktuje go jako ponowienie już wykonanego pakietu).
        if self.last_payload == Some(data) {
            // kolejny taki sam pakiet płyta znów wykonuje (co drugi)
            self.duplicates += 1;
            self.last_payload = None;
            return;
        }
        self.last_payload = Some(data);
        if self.cfg.m2_nano_timing {
            // wykonanie poprzedniego przyjętego pakietu, ten czeka na następny
            match self.held.replace(data) {
                Some(h) => self.execute(h),
                None => return,
            }
        } else {
            self.execute(data);
        }
    }

    fn execute(&mut self, data: [u8; PAYLOAD]) {
        self.received.extend_from_slice(&data);
        if data.starts_with(b"AT") {
            // UNVERIFIED: moc PWM płyty M3 — „AT1” + dwa bajty (MeerK40t driver.py)
            if data[2] == b'1' {
                self.power = Some((data[3] as f64 * 254.0 + data[4] as f64) / 10.0);
            }
            return;
        }
        if data[0] == b'A' {
            let want = self
                .cfg
                .serial
                .as_deref()
                .map(|s| crate::md5::md5(s.to_uppercase().as_bytes()));
            if want.is_some_and(|m| data[1..17] == m) {
                self.serial_ok = true;
                self.ack = Some(egv::STATUS_SERIAL_OK);
            }
            return;
        }
        self.input.extend(data);
    }

    /// Wykonanie w czasie wirtualnym.
    pub fn advance(&mut self, dt: f64) {
        let mut budget = dt * self.cfg.speed;
        loop {
            if self.cur.is_none() {
                // wstrzymana płyta czyta dalej tylko do polecenia wznowienia
                while self.interp.moves.is_empty() || self.interp.paused {
                    match self.input.pop_front() {
                        Some(b) => self.interp.byte(b),
                        None => break,
                    }
                }
                if self.interp.paused {
                    return;
                }
                match self.interp.moves.pop_front() {
                    Some(m) => {
                        let len = (((m.to.0 - m.from.0) as f64).powi(2)
                            + ((m.to.1 - m.from.1) as f64).powi(2))
                        .sqrt()
                            * 25.4
                            / 1000.0;
                        let t = len / m.speed.max(0.01);
                        self.cur = Some((m, t));
                    }
                    None => return,
                }
            }
            let (m, t) = self.cur.expect("ruch");
            if budget.is_infinite() || budget >= t {
                if budget.is_finite() {
                    budget -= t;
                }
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
        if let Some(p) = self.pending {
            if self.room() {
                self.pending = None;
                self.accept(p);
                self.ack.get_or_insert(egv::STATUS_OK);
            }
        }
    }

    /// Płyta wykonuje albo ma dane do wykonania.
    pub fn running(&self) -> bool {
        self.cur.is_some()
            || !self.interp.moves.is_empty()
            || !self.input.is_empty()
            || self.interp.paused
            || self.pending.is_some()
    }

    /// Pozycja płyty (kroki).
    pub fn pos(&self) -> (i64, i64) {
        self.trace.last().map_or(self.interp_start(), |m| m.to)
    }

    fn interp_start(&self) -> (i64, i64) {
        (0, 0)
    }

    /// Tor jako zadanie w mm dokumentu (PNG porównywany z podglądem planera, REQ-19-012).
    pub fn trace_job(&self, st: &Steps) -> bc_plan::Job {
        trace_job(&self.trace, st)
    }
}

/// Tor płyty K40 (kroki) jako zadanie w mm dokumentu.
pub fn trace_job(trace: &[EgvMove], st: &Steps) -> bc_plan::Job {
    use bc_plan::{Move, Op, OpKind, OpSettings};
    let mm = |p: (i64, i64)| st.mm((p.0 as f64, p.1 as f64));
    let moves = trace
        .iter()
        .map(|m| {
            if m.rapid {
                Move::Travel {
                    to: mm(m.to),
                    lift: false,
                }
            } else {
                Move::Cut {
                    to: mm(m.to),
                    power: if m.on { 100.0 } else { 0.0 },
                    speed: m.speed,
                }
            }
        })
        .collect();
    bc_plan::Job {
        ops: vec![Op {
            layer: 0,
            label: "emulator".into(),
            sublayer: 0,
            pass: 0,
            kind: OpKind::Vector,
            mode: bc_doc::LayerMode::Line,
            show: true,
            settings: OpSettings {
                speed: 0.0,
                power_max: 100.0,
                power_min: 0.0,
                power2_max: None,
                power2_min: None,
                dynamic_power: false,
                pwm_hz: None,
                air_assist: false,
            },
            pause: None,
            delay_s: 0.0,
            z: 0.0,
            objects: Vec::new(),
            moves,
        }],
        warnings: Vec::new(),
        start: trace.first().map_or(st.mm((0.0, 0.0)), |m| mm(m.from)),
        origin: bc_geom::Point::ORIGIN,
    }
}

impl Ch341 for LihuiyuEmulator {
    fn write_data(&mut self, data: &[u8]) -> Result<(), TransportError> {
        self.tick();
        if self.cfg.m2_nano_timing {
            let last = self.last_status;
            if !self.script.is_empty() {
                // zapis, zanim płyta odpowiedziała na poprzedni pakiet
                self.collisions += 1;
                self.crc_errors += 1;
                self.script = VecDeque::from([last, egv::STATUS_CRC_ERROR]);
                return Ok(());
            }
            let ok = data.len() == 32
                && data[0] == 0
                && self.corrupt_next == 0
                && egv::crc8(&data[1..31]) == data[31];
            if !ok {
                self.corrupt_next = self.corrupt_next.saturating_sub(1);
                self.crc_errors += 1;
                self.script = VecDeque::from([last, egv::STATUS_CRC_ERROR]);
                return Ok(());
            }
            let mut p = [0u8; PAYLOAD];
            p.copy_from_slice(&data[1..31]);
            if self.room() && self.pending.is_none() {
                self.accept(p);
                let ack = self.ack.take().unwrap_or(egv::STATUS_OK);
                self.script = VecDeque::from([last, last, egv::STATUS_BUSY, ack]);
            } else {
                self.pending = Some(p);
                self.ack = None;
                self.script = VecDeque::from([last, last]);
            }
            return Ok(());
        }
        if data.len() != 32 || data[0] != 0 {
            self.crc_errors += 1;
            self.ack = Some(egv::STATUS_CRC_ERROR);
            return Ok(());
        }
        let mut p = [0u8; PAYLOAD];
        p.copy_from_slice(&data[1..31]);
        let mut crc = data[31];
        if self.corrupt_next > 0 {
            self.corrupt_next -= 1;
            crc ^= 0x5A;
        }
        if egv::crc8(&p) != crc {
            self.crc_errors += 1;
            self.ack = Some(egv::STATUS_CRC_ERROR);
            return Ok(());
        }
        if self.room() && self.pending.is_none() {
            self.ack = Some(egv::STATUS_OK);
            self.accept(p);
        } else {
            self.pending = Some(p);
            self.ack = None;
        }
        Ok(())
    }

    fn write_addr(&mut self, _data: &[u8]) -> Result<(), TransportError> {
        self.addr_writes += 1;
        Ok(())
    }

    fn status(&mut self) -> Result<[u8; 6], TransportError> {
        self.tick();
        if self.cfg.m2_nano_timing {
            let code = if let Some(s) = self.script.pop_front() {
                s
            } else if let Some(a) = self.ack.take() {
                a
            } else if self.pending.is_some() {
                self.busy += 1;
                egv::STATUS_BUSY
            } else if !self.running() && self.interp.finished {
                egv::STATUS_FINISH
            } else {
                // stan po odpowiedzi trwa do następnego zapisu (dziennik k40_probe)
                self.last_status
            };
            self.last_status = code;
            return Ok([0xFF, code, 0, 0, 0, 0]);
        }
        let code = if let Some(a) = self.ack.take() {
            a
        } else if self.pending.is_some() {
            self.busy += 1;
            egv::STATUS_BUSY
        } else if self.running() {
            egv::STATUS_OK
        } else {
            egv::STATUS_FINISH
        };
        Ok([0xFF, code, 0, 0, 0, 0])
    }

    fn describe(&self) -> String {
        format!("emulator lihuiyu {}", self.cfg.board.id())
    }
}
