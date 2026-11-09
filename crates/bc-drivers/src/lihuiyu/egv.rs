//! Protokół EGV płyt Lihuiyu (ADR 0014 §2): CRC-8, kodowanie odległości i prędkości,
//! pakiety po 30 bajtów, kompilator zadania (REQ-13-041, 043, 044, 047).
//!
//! Źródła faktów (ADR 0013, `THIRD_PARTY.md`): MeerK40t (MIT) — README modułu `lihuiyu`,
//! opis zachowania `laserspeed.py`, `parser.py`, `controller.py`, `driver.py`. Kod powstał od
//! zera na podstawie opisu bajtów; tablice są liczone, nie kopiowane.

use bc_plan::{Job, Move, OpKind};

use super::{Board, K40Settings};
use crate::{CompileError, CompileWarning, Context, JobCompiler, MachineFile, ProgressFn};

// ---------- stany płyty ----------

/// Pakiet przyjęty. Źródło: README MeerK40t `lihuiyu` (OK = 206).
pub const STATUS_OK: u8 = 206;
/// Płyta zajęta — czekaj. Źródło: README MeerK40t (BUSY = 238).
pub const STATUS_BUSY: u8 = 238;
/// Pakiet odrzucony (zła suma CRC). Źródło: MeerK40t `controller.py` (207).
pub const STATUS_CRC_ERROR: u8 = 207;
/// Zadanie zakończone. Źródło: README MeerK40t (FINISH = 236).
pub const STATUS_FINISH: u8 = 236;
/// Numer seryjny potwierdzony / koniec (M3). Źródło: MeerK40t `controller.py` (204).
pub const STATUS_SERIAL_OK: u8 = 204;
/// Niskie zasilanie. Źródło: MeerK40t `controller.py` (239).
pub const STATUS_POWER: u8 = 239;

/// Nazwa stanu (konsola).
pub fn status_name(s: u8) -> &'static str {
    match s {
        STATUS_OK => "OK",
        STATUS_BUSY => "BUSY",
        STATUS_CRC_ERROR => "CRC ERROR",
        STATUS_FINISH => "TASK COMPLETE",
        STATUS_SERIAL_OK => "SERIAL OK",
        STATUS_POWER => "LOW POWER",
        0 => "NO STATUS",
        _ => "?",
    }
}

/// Dane pakietu (bez nagłówka 0x00 i CRC). Źródło: README MeerK40t (pakiety po 30 bajtów).
pub const PAYLOAD: usize = 30;
/// Dopełnienie pakietu. Źródło: MeerK40t `controller.py` (znak 'F').
pub const PAD: u8 = b'F';

/// CRC-8 Dallas/Maxim 1-Wire (wielomian 0x31 odbity = 0x8C, start 0, bez XOR końcowego)
/// po 30 bajtach danych. Źródło: MeerK40t `controller.py` (opis algorytmu).
pub fn crc8(data: &[u8]) -> u8 {
    let mut crc = 0u8;
    for &b in data {
        let mut x = b;
        for _ in 0..8 {
            let mix = (crc ^ x) & 1;
            crc >>= 1;
            if mix != 0 {
                crc ^= 0x8C;
            }
            x >>= 1;
        }
    }
    crc
}

/// Ramka pakietu: 0x00, 30 bajtów danych, CRC-8 (32 bajty). Źródło: MeerK40t `controller.py`.
pub fn frame(payload: &[u8; PAYLOAD]) -> [u8; 32] {
    let mut f = [0u8; 32];
    f[1..31].copy_from_slice(payload);
    f[31] = crc8(payload);
    f
}

/// Pakiet do wysłania.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Packet {
    pub data: [u8; PAYLOAD],
    /// Po pakiecie czekaj na koniec zadania (znak `-` na końcu wiersza strumienia).
    pub wait_finish: bool,
}

/// Dzieli strumień EGV na pakiety: `\n` kończy pakiet (dopełnienie `F`, w wierszu `AT…`
/// zerami), `-` przed `\n` oznacza „czekaj na koniec”, para `|`+litera nie jest dzielona.
/// Źródło: MeerK40t `controller.py` (znaki specjalne bufora).
pub fn packets(stream: &[u8]) -> Vec<Packet> {
    let mut out = Vec::new();
    for line in stream.split(|b| *b == b'\n') {
        if line.is_empty() {
            continue;
        }
        let (line, wait) = match line.last() {
            Some(b'-') => (&line[..line.len() - 1], true),
            _ => (line, false),
        };
        let pad = if line.starts_with(b"AT") { 0 } else { PAD };
        let mut i = 0;
        while i < line.len() {
            let mut end = (i + PAYLOAD).min(line.len());
            // nie dzielimy pary `|`+litera ani liczby (pakiet czasu rzeczywistego między
            // pakietami zadania nie rozetnie odległości ani kodu prędkości)
            while end < line.len()
                && end > i + 12
                && (line[end - 1] == b'|'
                    || (line[end - 1].is_ascii_digit() && line[end].is_ascii_digit()))
            {
                end -= 1;
            }
            let mut data = [pad; PAYLOAD];
            data[..end - i].copy_from_slice(&line[i..end]);
            out.push(Packet {
                data,
                wait_finish: false,
            });
            i = end;
        }
        if wait {
            match out.last_mut() {
                Some(p) if !line.is_empty() => p.wait_finish = true,
                _ => out.push(Packet {
                    data: [pad; PAYLOAD],
                    wait_finish: true,
                }),
            }
        }
    }
    out
}

// ---------- odległości ----------

/// Kodowanie odległości w krokach: `z` = 255, `a`…`y` = 1…25, `|a`…`|z` = 26…51,
/// 52…254 = trzy cyfry. Źródło: MeerK40t `driver.py` i `parser.py` (opis kodowania).
pub fn distance(steps: u64) -> Vec<u8> {
    let mut v = vec![b'z'; (steps / 255) as usize];
    let r = (steps % 255) as u8;
    match r {
        0 => {}
        1..=25 => v.push(b'a' + r - 1),
        26..=51 => {
            v.push(b'|');
            v.push(b'a' + r - 26);
        }
        _ => v.extend(format!("{r:03}").bytes()),
    }
    v
}

// ---------- prędkość ----------

/// Wynik kodowania prędkości.
#[derive(Debug, Clone, PartialEq)]
pub struct SpeedCode {
    pub text: String,
    /// Prędkość po ograniczeniu do zakresu płyty, mm/s.
    pub speed: f64,
    pub accel: u8,
    pub suffix_c: bool,
    /// Prędkość poza zakresem płyty (ograniczona).
    pub clamped: bool,
}

/// Stała `d_ratio` członu przekątnej. Źródło: MeerK40t `laserspeed.py` (opis, 0.261199033289).
const D_RATIO: f64 = 0.261199033289;
/// Najwyższa prędkość wektorowa. Źródło: MeerK40t `laserspeed.py` (powyżej 240 mm/s kod
/// zastępczy) — BurnCraft ogranicza prędkość z ostrzeżeniem.
pub const MAX_VECTOR_SPEED: f64 = 240.0;
/// Poniżej tej prędkości płyty M2/M3/B2 używają kodu z sufiksem C. Źródło: MeerK40t.
pub const SUFFIX_C_BELOW: f64 = 7.0;

/// Poziom przyspieszenia z prędkości. Źródło: README i `laserspeed.py` MeerK40t (wektor:
/// progi 25,4 / 60 / 127 mm/s; raster poziomy: 25,4 / 60 / 127 / 320 mm/s).
pub fn accel(speed: f64, raster: bool) -> u8 {
    if !raster {
        match speed {
            s if s <= 25.4 => 1,
            s if s <= 60.0 => 2,
            s if s < 127.0 => 3,
            _ => 4,
        }
    } else {
        match speed {
            s if s <= 25.4 => 1,
            s if s < 127.0 => 2,
            s if s < 320.0 => 3,
            _ => 4,
        }
    }
}

/// Stałe równania `wartość = 65536 − (B + M·T)` (T = okres kroku w ms przy 1000 dpi).
/// Źródło: README i `laserspeed.py` MeerK40t (opis stałych dla grup płyt).
pub fn equation(board: Board, accel: u8, suffix_c: bool) -> (f64, f64) {
    let i = (accel.clamp(1, 4) - 1) as usize;
    let ab = [784.0, 784.0, 896.0, 1024.0][i];
    let def = [5120.0, 5120.0, 5632.0, 6144.0][i];
    match (board.group(), suffix_c) {
        (Group::Ab, _) => (ab, 2000.0),
        (Group::Default, false) => (def, 12120.0),
        (Group::B2, false) => (def, 24240.0),
        (Group::Default, true) => (8.0, 1010.0),
        (Group::B2, true) => (ab, 2020.0),
    }
}

/// Grupa płyt o wspólnym równaniu prędkości.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Group {
    /// A, B, B1.
    Ab,
    /// M, M1, M2, M3, 6C6879-LASER-M2/M3.
    Default,
    B2,
}

fn digits3(v: u16) -> String {
    format!("{:03}{:03}", v >> 8, v & 0xFF)
}

/// Najmniejsza prędkość, dla której wartość mieści się w 16 bitach.
fn min_speed(b: f64, m: f64) -> f64 {
    25.4 * m / (65535.0 - b)
}

/// Kod prędkości: wektor `CV` + 6 cyfr + przyspieszenie (+ krok i przekątna, + `C`), raster
/// `V` + 6 cyfr + przyspieszenie + `G` + krok. Źródło: MeerK40t `laserspeed.py` (opis).
pub fn speed_code(board: Board, speed: f64, raster_step: Option<u32>) -> SpeedCode {
    let raster = raster_step.is_some();
    let mut clamped = false;
    let mut s = speed;
    if !raster && s > MAX_VECTOR_SPEED {
        s = MAX_VECTOR_SPEED;
        clamped = true;
    }
    let suffix_c = !raster && board.suffix_c() && s < SUFFIX_C_BELOW;
    let mut a = if suffix_c { 1 } else { accel(s, raster) };
    let (mut b, mut m) = equation(board, a, suffix_c);
    let lo = min_speed(b, m);
    if s.is_nan() || s < lo {
        s = lo;
        clamped = true;
        if !suffix_c {
            a = accel(s, raster);
            (b, m) = equation(board, a, suffix_c);
        }
    }
    let t = 25.4 / s;
    let value = (65536.0 - (b + m * t)).round().clamp(0.0, 65535.0) as u16;
    let mut text = String::new();
    if raster {
        text.push('V');
    } else {
        text.push_str("CV");
    }
    text.push_str(&digits3(value));
    text.push(char::from(b'0' + a));
    if let Some(g) = raster_step {
        text.push_str(&format!("G{:03}", g.min(999)));
    } else if board.diagonal() {
        let step = ((s.floor() as u32) + 1).min(128);
        let diag = (D_RATIO * m * t / step as f64).round().clamp(0.0, 65535.0) as u16;
        text.push_str(&format!("{step:03}{}", digits3(diag)));
    }
    if suffix_c {
        text.push('C');
    }
    SpeedCode {
        text,
        speed: s,
        accel: a,
        suffix_c,
        clamped,
    }
}

/// Prędkość z wartości kodu (odwrotność [`speed_code`]), mm/s.
pub fn decode_speed(board: Board, value: u16, accel: u8, suffix_c: bool) -> f64 {
    let (b, m) = equation(board, accel, suffix_c);
    let x = 65536.0 - value as f64;
    let t = (x - b) / m;
    if t > 0.0 {
        25.4 / t
    } else {
        f64::INFINITY
    }
}

// ---------- kompilator ----------

/// Litera kierunku. Źródło: MeerK40t `parser.py`/`driver.py`: `B` = +X (prawo), `T` = −X,
/// `R` = +Y (do przodu, „bottom”), `L` = −Y („top”), `M` = przekątna.
pub fn dir_letter(axis_x: bool, positive: bool) -> u8 {
    match (axis_x, positive) {
        (true, true) => b'B',
        (true, false) => b'T',
        (false, true) => b'R',
        (false, false) => b'L',
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Ax {
    None,
    X,
    Y,
    Diag,
}

/// Generator strumienia EGV (współrzędne w krokach płyty, Y w dół od narożnika bazowania).
pub struct Egv {
    pub out: Vec<u8>,
    pub pos: (i64, i64),
    program: bool,
    started: bool,
    xpos: bool,
    ypos: bool,
    axis: Ax,
    laser: bool,
}

impl Default for Egv {
    fn default() -> Self {
        Egv::new((0, 0))
    }
}

impl Egv {
    pub fn new(pos: (i64, i64)) -> Self {
        Egv {
            out: Vec::new(),
            pos,
            program: false,
            started: false,
            xpos: true,
            ypos: true,
            axis: Ax::None,
            laser: false,
        }
    }

    fn put(&mut self, s: &[u8]) {
        self.out.extend_from_slice(s);
    }

    /// Przejazd szybki w trybie domyślnym (bez `I` i `S1P`): litery i odległości.
    fn default_moves(&mut self, to: (i64, i64)) {
        let (dx, dy) = (to.0 - self.pos.0, to.1 - self.pos.1);
        if dx != 0 {
            self.xpos = dx > 0;
            self.out.push(dir_letter(true, self.xpos));
            let d = distance(dx.unsigned_abs());
            self.put(&d);
        }
        if dy != 0 {
            self.ypos = dy > 0;
            self.out.push(dir_letter(false, self.ypos));
            let d = distance(dy.unsigned_abs());
            self.put(&d);
        }
        self.pos = to;
        self.axis = Ax::None;
    }

    /// Przejazd szybki poza programem: `I` + ruchy + `S1P`. Źródło: MeerK40t `driver.py`.
    pub fn rapid(&mut self, to: (i64, i64)) {
        self.leave(false);
        self.put(b"I");
        self.default_moves(to);
        self.put(b"S1P\n");
    }

    /// Początek programu: koniec poprzedniego (`FNSE-`, czekanie) i `I`. Płyta M2 Nano po
    /// `@NSE` nie wykonuje kolejnego programu (test sprzętowy K40, 2026-11-09), dlatego
    /// zmiana trybu zawsze kończy program jak przejście do przejazdu w MeerK40t (rapid_mode).
    fn begin(&mut self) {
        self.leave(true);
        self.put(b"I");
    }

    /// Wejście w program: `I` + kod prędkości + przejazd do `start` + `N` + deklaracja
    /// kierunków + `S1E`. Źródło: MeerK40t `driver.py` (program_mode).
    pub fn enter(&mut self, speed: &str, start: (i64, i64), horizontal_major: bool) {
        self.begin();
        self.put(speed.as_bytes());
        self.default_moves(start);
        self.put(b"N");
        let (x, y) = (dir_letter(true, self.xpos), dir_letter(false, self.ypos));
        if horizontal_major {
            self.put(&[y, x]);
            self.axis = Ax::X;
        } else {
            self.put(&[x, y]);
            self.axis = Ax::Y;
        }
        self.put(b"S1E");
        self.program = true;
        self.started = true;
        self.laser = false;
    }

    /// Wyjście z programu: `FNSE-` (koniec z czekaniem). `@NSE` nie jest używane — płyta
    /// M2 Nano po nim przestaje wykonywać program (test sprzętowy K40, 2026-11-09).
    pub fn leave(&mut self, _finish: bool) {
        if self.program {
            self.put(b"FNSE-\n");
            self.program = false;
            self.laser = false;
        }
    }

    pub fn in_program(&self) -> bool {
        self.program
    }

    fn set_laser(&mut self, on: bool) {
        if self.laser != on {
            self.out.push(if on { b'D' } else { b'U' });
            self.laser = on;
        }
    }

    fn axial(&mut self, x: bool, positive: bool, n: u64, on: bool) {
        let want = if x { Ax::X } else { Ax::Y };
        let cur = if x { self.xpos } else { self.ypos };
        if self.axis != want || cur != positive {
            if x {
                self.xpos = positive;
            } else {
                self.ypos = positive;
            }
            self.out.push(dir_letter(x, positive));
            self.axis = want;
        }
        self.set_laser(on);
        let d = distance(n);
        self.put(&d);
    }

    fn diagonal(&mut self, xp: bool, yp: bool, n: u64, on: bool) {
        if self.axis != Ax::Diag || self.xpos != xp || self.ypos != yp {
            if self.xpos != xp {
                self.xpos = xp;
                self.out.push(dir_letter(true, xp));
            }
            if self.ypos != yp {
                self.ypos = yp;
                self.out.push(dir_letter(false, yp));
            }
            self.out.push(b'M');
            self.axis = Ax::Diag;
        }
        self.set_laser(on);
        let d = distance(n);
        self.put(&d);
    }

    /// Ruch w programie do `to` (linia: odcinki osiowe i przekątne, Bresenham).
    pub fn line(&mut self, to: (i64, i64), on: bool) {
        let (dx, dy) = (to.0 - self.pos.0, to.1 - self.pos.1);
        if dx == 0 && dy == 0 {
            return;
        }
        let (ax, ay) = (dx.unsigned_abs(), dy.unsigned_abs());
        let (xp, yp) = (dx > 0, dy > 0);
        if ay == 0 {
            self.axial(true, xp, ax, on);
        } else if ax == 0 {
            self.axial(false, yp, ay, on);
        } else if ax == ay {
            self.diagonal(xp, yp, ax, on);
        } else {
            let (major, minor, x_major) = if ax > ay {
                (ax, ay, true)
            } else {
                (ay, ax, false)
            };
            let mut runs: Vec<(bool, u64)> = Vec::new();
            let mut prev = 0u64;
            for i in 1..=major {
                let m = (i as u128 * minor as u128 * 2 + major as u128) / (2 * major as u128);
                let m = m as u64;
                let diag = m > prev;
                prev = m;
                match runs.last_mut() {
                    Some((d, n)) if *d == diag => *n += 1,
                    _ => runs.push((diag, 1)),
                }
            }
            for (diag, n) in runs {
                if diag {
                    self.diagonal(xp, yp, n, on);
                } else if x_major {
                    self.axial(true, xp, n, on);
                } else {
                    self.axial(false, yp, n, on);
                }
            }
        }
        self.pos = to;
    }

    /// Zmiana kierunku osi X w rastrze (płyta sama przesuwa Y o krok `G`).
    pub fn raster_reverse(&mut self, raster_step: i64) {
        self.set_laser(false);
        self.xpos = !self.xpos;
        self.out.push(dir_letter(true, self.xpos));
        self.axis = Ax::X;
        self.pos.1 += if self.ypos { raster_step } else { -raster_step };
    }

    pub fn x_positive(&self) -> bool {
        self.xpos
    }
}

/// Przeliczenie mm (dokument, Y w górę) na kroki płyty (Y w dół od narożnika bazowania).
#[derive(Debug, Clone, Copy)]
pub struct Steps {
    pub kx: f64,
    pub ky: f64,
    pub bed_h: f64,
}

impl Steps {
    pub fn new(s: &K40Settings, bed_h: f64) -> Self {
        Steps {
            kx: s.dpi / 25.4 * s.scale[0],
            ky: s.dpi / 25.4 * s.scale[1],
            bed_h,
        }
    }
    pub fn of(&self, p: bc_geom::Point) -> (i64, i64) {
        (
            (p.x * self.kx).round() as i64,
            ((self.bed_h - p.y) * self.ky).round() as i64,
        )
    }
    /// Odwrotność (kroki → mm dokumentu).
    pub fn mm(&self, s: (f64, f64)) -> bc_geom::Point {
        bc_geom::Point::new(s.0 / self.kx, self.bed_h - s.1 / self.ky)
    }
}

/// Próg przejazdu w programie: dłuższe przejazdy idą jako szybkie (nowy program).
/// Źródło: MeerK40t (opt_jog_minimum = 256 mil).
pub const JOG_MINIMUM_STEPS: i64 = 256;

/// Linia skanowania rastra: (y, x początku, x końca, odcinki (x końca, moc %)).
struct ScanLine {
    y: i64,
    x0: i64,
    segs: Vec<(i64, f64)>,
}

/// Linie rastra natywnego (poziome, dwukierunkowe, stały krok Y); `None` = nie pasuje.
fn scanlines(moves: &[Move], start: bc_geom::Point, st: &Steps) -> Option<(Vec<ScanLine>, i64)> {
    let mut lines: Vec<ScanLine> = Vec::new();
    let mut at = st.of(start);
    for m in moves {
        match *m {
            Move::Travel { to, .. } => {
                let p = st.of(to);
                lines.push(ScanLine {
                    y: p.1,
                    x0: p.0,
                    segs: Vec::new(),
                });
                at = p;
            }
            Move::Cut { to, power, .. } => {
                let p = st.of(to);
                if p.1 != at.1 {
                    return None;
                }
                let l = lines.last_mut()?;
                l.segs.push((p.0, power));
                at = p;
            }
            Move::Dwell { .. } => return None,
        }
    }
    lines.retain(|l| !l.segs.is_empty());
    if lines.len() < 2 {
        return None;
    }
    let step = lines[1].y - lines[0].y;
    if step == 0 || step.abs() > 999 {
        return None;
    }
    let mut dir = 0i64;
    for (i, l) in lines.iter().enumerate() {
        if i > 0 && l.y - lines[i - 1].y != step {
            return None;
        }
        let mut x = l.x0;
        let mut ldir = 0;
        for &(sx, _) in &l.segs {
            let d = (sx - x).signum();
            if d != 0 {
                if ldir != 0 && d != ldir {
                    return None;
                }
                ldir = d;
            }
            x = sx;
        }
        if ldir == 0 || ldir == dir {
            return None;
        }
        dir = ldir;
    }
    Some((lines, step))
}

/// Włączenie wiązki na kolejnych krokach odcinka (dither błędu wzdłuż linii, gdy moc
/// zmienia się w obrębie operacji, a płyta nie ma PWM — REQ-13-045).
struct Dither {
    err: f64,
}

impl Dither {
    /// Odcinek `n` kroków z wypełnieniem `duty` (0–1) jako przebiegi (włączona, kroki).
    fn runs(&mut self, n: u64, duty: f64) -> Vec<(bool, u64)> {
        let mut out: Vec<(bool, u64)> = Vec::new();
        if duty >= 0.999 || duty <= 0.001 {
            out.push((duty > 0.5, n));
            return out;
        }
        for _ in 0..n {
            self.err += duty;
            let on = self.err >= 0.5;
            if on {
                self.err -= 1.0;
            }
            match out.last_mut() {
                Some((o, k)) if *o == on => *k += 1,
                _ => out.push((on, 1)),
            }
        }
        out
    }
}

/// Kompilator EGV (sterownik `lihuiyu`).
pub struct EgvCompiler;

/// Szybkość przejazdów szybkich do szacowania czasu, mm/s (UNVERIFIED: zależy od płyty).
const RAPID_MM_S: f64 = 100.0;

/// Polecenie mocy PWM płyty M3: `AT` `1` + dwa bajty (moc/254, moc%254), moc 0–1000 ‰.
/// Źródło: MeerK40t `driver.py` (opis polecenia). UNVERIFIED: wymaga firmware 2024.01.18g+.
pub fn pwm_command(power_pct: f64) -> Vec<u8> {
    let mut p = (power_pct.clamp(0.0, 100.0) * 10.0).round() as u32;
    // bajt 10 (`\n`) kończyłby wiersz strumienia — moc o 0,1 ‰ wyżej
    if p % 254 == 10 {
        p += 1;
    }
    let mut v = b"AT1".to_vec();
    v.push((p / 254) as u8);
    v.push((p % 254) as u8);
    v.push(b'\n');
    v
}

impl JobCompiler for EgvCompiler {
    fn name(&self) -> &'static str {
        "lihuiyu"
    }
    fn extension(&self) -> &'static str {
        "egv"
    }
    fn compile(
        &self,
        job: &Job,
        ctx: &Context,
        progress: &mut ProgressFn,
    ) -> Result<MachineFile, CompileError> {
        let cfg = &ctx.k40;
        cfg.validate().map_err(CompileError::Invalid)?;
        let board = cfg.board;
        let st = Steps::new(cfg, ctx.bed[1]);
        // protokół względny: zadanie zaczyna się w narożniku bazowania (0, 0); sterownik
        // dojeżdża tam przed zadaniem (ADR 0014 §2)
        let mut e = Egv::new((0, 0));
        let mut warn: Vec<CompileWarning> = Vec::new();
        let mut warned = std::collections::BTreeSet::new();
        let mut w = |code: &str, msg: String| {
            if warned.insert(code.to_string()) {
                warn.push(CompileWarning::new(code, msg));
            }
        };
        let mut time = 0.0;
        let total = job.ops.len();
        let all = job.segments();
        for (oi, op) in job.ops.iter().enumerate() {
            progress(oi, total)?;
            if op.z != 0.0 {
                w("z_ignored", "K40 nie ma osi Z — Z pominięte".into());
            }
            if op.settings.air_assist {
                w(
                    "air_ignored",
                    "Lihuiyu nie steruje nadmuchem z programu".into(),
                );
            }
            let max_p = op
                .moves
                .iter()
                .filter_map(|m| match m {
                    Move::Cut { power, .. } if *power > 0.0 => Some(*power),
                    _ => None,
                })
                .fold(0.0f64, f64::max);
            if !board.pwm() && max_p > 0.0 && max_p < 100.0 {
                w(
                    "power_ignored",
                    format!(
                        "płyta {} nie ma PWM: moc ustawia pokrętło lasera; ustawienie {max_p:.0} % pominięte (jasność: dither, prędkość)",
                        board.name()
                    ),
                );
            }
            if board.pwm() && max_p > 0.0 {
                e.leave(false);
                let at = pwm_command(max_p);
                if !e.out.is_empty() && e.out.last() != Some(&b'\n') {
                    e.out.push(b'\n');
                }
                e.out.extend_from_slice(&at);
            }
            let segs: Vec<&bc_plan::Seg> = all.iter().filter(|s| s.op == oi).collect();
            let start_pt = segs.first().map_or(job.start, |s| s.from);
            let speed = op.settings.speed.max(0.01);
            let native = if op.kind == OpKind::Raster || op.mode == bc_doc::LayerMode::Fill {
                scanlines(&op.moves, start_pt, &st)
            } else {
                None
            };
            let mut dither = Dither { err: 0.0 };
            let duty = |p: f64| if max_p > 0.0 { p / max_p } else { 0.0 };
            if let Some((lines, step)) = native {
                let code = speed_code(board, speed, Some(step.unsigned_abs() as u32));
                if code.clamped {
                    w(
                        "speed_clamped",
                        format!("prędkość poza zakresem płyty — {:.2} mm/s", code.speed),
                    );
                }
                let l0 = &lines[0];
                // kierunek Y kroku rastra: deklaracja przed S1E
                let ypos = step > 0;
                let first_dir = (l0.segs[0].0 - l0.x0) > 0;
                // przejazd do startu z ustawieniem kierunków (deklaracja po N)
                let tgt = (l0.x0, l0.y);
                e.enter_with_dirs(&code.text, tgt, first_dir, ypos);
                for (li, l) in lines.iter().enumerate() {
                    if li > 0 {
                        // przejazd X do początku linii, potem zmiana kierunku (krok Y)
                        let dx = l.x0 - e.pos.0;
                        let cont = (dx > 0) == e.x_positive() && dx != 0;
                        if cont {
                            e.line((l.x0, e.pos.1), false);
                            e.raster_reverse(step.abs());
                        } else {
                            e.raster_reverse(step.abs());
                            e.line((l.x0, e.pos.1), false);
                        }
                        e.pos.1 = l.y;
                    }
                    for &(x, p) in &l.segs {
                        let n = (x - e.pos.0).unsigned_abs();
                        let positive = x > e.pos.0;
                        for (on, k) in dither.runs(n, duty(p)) {
                            let nx = e.pos.0 + if positive { k as i64 } else { -(k as i64) };
                            e.line((nx, e.pos.1), on);
                        }
                    }
                }
                for s in &segs {
                    time += s.from.distance(s.to) / code.speed;
                }
            } else {
                let code = speed_code(board, speed, None);
                if code.clamped {
                    w(
                        "speed_clamped",
                        format!("prędkość poza zakresem płyty — {:.2} mm/s", code.speed),
                    );
                }
                let s0 = st.of(start_pt);
                e.enter(&code.text, s0, false);
                for s in &segs {
                    let to = st.of(s.to);
                    match s.kind {
                        bc_plan::SegKind::Travel => {
                            let d = (to.0 - e.pos.0).abs().max((to.1 - e.pos.1).abs());
                            if d > JOG_MINIMUM_STEPS {
                                e.enter(&code.text, to, false);
                                time += s.from.distance(s.to) / RAPID_MM_S;
                            } else {
                                e.line(to, false);
                                time += s.from.distance(s.to) / code.speed;
                            }
                        }
                        bc_plan::SegKind::Cut => {
                            let from = e.pos;
                            let n = (to.0 - from.0).abs().max((to.1 - from.1).abs()) as u64;
                            let runs = dither.runs(n.max(1), duty(s.power));
                            if runs.len() == 1 || n == 0 {
                                e.line(to, runs[0].0);
                            } else {
                                // dither wzdłuż odcinka: punkty pośrednie po krokach osi głównej
                                let mut done = 0u64;
                                for (on, k) in runs {
                                    done += k;
                                    let f = done as f64 / n as f64;
                                    let p = (
                                        from.0 + ((to.0 - from.0) as f64 * f).round() as i64,
                                        from.1 + ((to.1 - from.1) as f64 * f).round() as i64,
                                    );
                                    e.line(p, on);
                                }
                            }
                            time += s.from.distance(s.to) / code.speed;
                        }
                        bc_plan::SegKind::Dwell => {
                            w(
                                "dwell_ignored",
                                "Lihuiyu: postój (tryb punktowy) niedostępny — pominięty".into(),
                            );
                        }
                    }
                }
            }
        }
        e.leave(true);
        progress(total, total)?;
        Ok(MachineFile::from_bytes("lihuiyu", "egv", e.out, time, warn))
    }
}

impl Egv {
    /// Wejście w program rastra z zadanymi kierunkami (deklaracja Y, potem X).
    pub fn enter_with_dirs(&mut self, speed: &str, start: (i64, i64), xpos: bool, ypos: bool) {
        self.begin();
        self.put(speed.as_bytes());
        self.default_moves(start);
        self.xpos = xpos;
        self.ypos = ypos;
        self.put(b"N");
        self.put(&[dir_letter(false, ypos), dir_letter(true, xpos)]);
        self.axis = Ax::X;
        self.put(b"S1E");
        self.program = true;
        self.started = true;
        self.laser = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn distance_codes() {
        assert_eq!(distance(1), b"a");
        assert_eq!(distance(25), b"y");
        assert_eq!(distance(26), b"|a");
        assert_eq!(distance(51), b"|z");
        assert_eq!(distance(52), b"052");
        assert_eq!(distance(254), b"254");
        assert_eq!(distance(255), b"z");
        assert_eq!(distance(256), b"za");
        assert_eq!(distance(0), b"");
    }

    #[test]
    fn crc_known_values() {
        // CRC-8/MAXIM: "123456789" → 0xA1 (wektor kontrolny algorytmu)
        assert_eq!(crc8(b"123456789"), 0xA1);
        assert_eq!(crc8(&[0; 30]), 0);
    }
}
