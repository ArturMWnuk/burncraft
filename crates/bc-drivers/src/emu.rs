//! Emulator GRBL (ADR 0010 §4): bufor RX 128 B, bufor planera 16 bloków, wykonanie ruchów
//! w czasie wirtualnym, stany Idle/Run/Hold/Jog/Alarm/Door, `$$`, `$I`, `$H`, `$X`, `$J=`,
//! nadpisania, sonda G38, karta SD (odmiana FluidNC), zapis toru z wiązką. Emulator jest
//! urządzeniem fikcyjnym (REQ-13-191) i podstawą testów sterownika (REQ-13-017, REQ-19-007).

use std::collections::{BTreeMap, VecDeque};

use bc_geom::Point;

use crate::gcode::sim::{Event, Interp, SimOptions, Spindle};
use crate::grbl::codes::{self, Variant};

/// Rozmiar bufora RX GRBL.
pub const RX_SIZE: usize = 128;
/// Bloki planera.
pub const PLANNER_BLOCKS: usize = 16;

/// Konfiguracja emulatora.
#[derive(Debug, Clone)]
pub struct EmuConfig {
    pub variant: &'static Variant,
    /// Mnożnik czasu (1 = czas rzeczywisty); `f64::INFINITY` = wykonanie natychmiastowe.
    pub speed: f64,
    /// `$22=1` (bazowanie) — start w stanie alarmu do `$H`/`$X`.
    pub homing: bool,
    /// Powierzchnia sondy (Z maszyny), `None` = brak kontaktu.
    pub probe_z: Option<f64>,
}

impl Default for EmuConfig {
    fn default() -> Self {
        EmuConfig {
            variant: codes::variant("grbl11").expect("grbl11"),
            speed: f64::INFINITY,
            homing: false,
            probe_z: None,
        }
    }
}

impl EmuConfig {
    pub fn variant(id: &str) -> Self {
        EmuConfig {
            variant: codes::variant(id)
                .unwrap_or_else(|| codes::variant("grbl11").expect("grbl11")),
            ..Default::default()
        }
    }
}

/// Wykonany blok ruchu (współrzędne maszyny, mm).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Block {
    pub line: usize,
    pub from: [f64; 3],
    pub to: [f64; 3],
    /// mm/min (0 = szybki).
    pub feed: f64,
    pub rapid: bool,
    pub jog: bool,
    /// Wiązka świeci (tryb laserowy: G1/G2/G3 z M3/M4 i S > 0).
    pub on: bool,
    /// S po nadpisaniu mocy.
    pub s: f64,
    pub spindle: Spindle,
    pub air: bool,
    pub tool: u32,
    /// Postój, s (G4).
    pub dwell: f64,
    /// Pauza programu (M0).
    pub pause: bool,
}

impl Block {
    pub fn from_pt(&self) -> Point {
        Point::new(self.from[0], self.from[1])
    }
    pub fn to_pt(&self) -> Point {
        Point::new(self.to[0], self.to[1])
    }
    fn len(&self) -> f64 {
        let d: f64 = (0..3).map(|i| (self.to[i] - self.from[i]).powi(2)).sum();
        d.sqrt()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EState {
    Idle,
    Alarm(u32),
    Door,
}

struct XRecv {
    path: String,
    data: Vec<u8>,
    buf: Vec<u8>,
    expect: u8,
}

/// Emulator GRBL.
pub struct GrblEmulator {
    pub cfg: EmuConfig,
    pub settings: BTreeMap<u32, String>,
    interp: Interp,
    rx: Vec<u8>,
    out: Vec<u8>,
    planner: VecDeque<Block>,
    /// Postęp bieżącego bloku, s.
    cur_t: f64,
    state: EState,
    hold: bool,
    ov_feed: u32,
    ov_power: u32,
    /// Wykonane bloki (tor).
    pub trace: Vec<Block>,
    /// Przepełnienie bufora RX (błąd nadawcy).
    pub overflow: bool,
    /// Największe zajęcie bufora RX.
    pub max_rx: usize,
    /// Wiersze przyjęte (ok/error).
    pub lines: usize,
    pub errors: Vec<(usize, u32)>,
    pub sd: BTreeMap<String, Vec<u8>>,
    sd_run: VecDeque<String>,
    xmodem: Option<XRecv>,
    probe: Option<([f64; 3], bool)>,
    /// Czas wirtualny, s.
    pub clock: f64,
    /// Liczba resetów (0x18).
    pub resets: usize,
    /// Wszystkie bajty od nadawcy (do testów).
    pub received: Vec<u8>,
}

fn default_settings(v: &Variant, homing: bool) -> BTreeMap<u32, String> {
    let mut m = BTreeMap::new();
    for (k, val) in [
        (0, "10"),
        (1, "25"),
        (2, "0"),
        (3, "0"),
        (4, "0"),
        (5, "0"),
        (6, "0"),
        (10, "1"),
        (11, "0.010"),
        (12, "0.002"),
        (13, "0"),
        (20, "0"),
        (21, "0"),
        (23, "0"),
        (24, "25.000"),
        (25, "500.000"),
        (26, "250"),
        (27, "1.000"),
        (31, "0"),
        (32, "1"),
        (100, "80.000"),
        (101, "80.000"),
        (102, "250.000"),
        (110, "6000.000"),
        (111, "6000.000"),
        (112, "500.000"),
        (120, "1000.000"),
        (121, "1000.000"),
        (122, "10.000"),
        (130, "400.000"),
        (131, "400.000"),
        (132, "50.000"),
    ] {
        m.insert(k, val.to_string());
    }
    m.insert(22, if homing { "1" } else { "0" }.into());
    m.insert(30, format!("{}", v.s_max));
    if v.status_v09 {
        // GRBL 0.9: bez trybu laserowego i $30/$31/$32
        for k in [30, 31, 32] {
            m.remove(&k);
        }
    }
    m
}

fn f3(v: f64) -> String {
    let r = if v.abs() < 5e-4 { 0.0 } else { v };
    format!("{r:.3}")
}

impl GrblEmulator {
    pub fn new(cfg: EmuConfig) -> Self {
        let mut e = GrblEmulator {
            settings: default_settings(cfg.variant, cfg.homing),
            interp: Interp::new(SimOptions {
                laser_mode: true,
                s_max: cfg.variant.s_max,
            }),
            rx: Vec::new(),
            out: Vec::new(),
            planner: VecDeque::new(),
            cur_t: 0.0,
            state: EState::Idle,
            hold: false,
            ov_feed: 100,
            ov_power: 100,
            trace: Vec::new(),
            overflow: false,
            max_rx: 0,
            lines: 0,
            errors: Vec::new(),
            sd: BTreeMap::new(),
            sd_run: VecDeque::new(),
            xmodem: None,
            probe: None,
            clock: 0.0,
            resets: 0,
            received: Vec::new(),
            cfg,
        };
        e.boot(false);
        e
    }

    fn say(&mut self, s: &str) {
        self.out.extend_from_slice(s.as_bytes());
        self.out.extend_from_slice(b"\r\n");
    }

    fn boot(&mut self, alarm3: bool) {
        if alarm3 {
            self.say("ALARM:3");
            self.state = EState::Alarm(3);
        }
        self.say("");
        let w = self.cfg.variant.welcome;
        self.say(w);
        if self.setting(22) == 1.0 && !alarm3 {
            self.state = EState::Alarm(0);
        }
        if matches!(self.state, EState::Alarm(_)) {
            self.say("[MSG:'$H'|'$X' to unlock]");
        }
    }

    fn setting(&self, id: u32) -> f64 {
        self.settings
            .get(&id)
            .and_then(|v| v.parse().ok())
            .unwrap_or(0.0)
    }

    /// Pozycja maszyny (z postępem bieżącego bloku).
    pub fn mpos(&self) -> [f64; 3] {
        match self.planner.front() {
            Some(b) if !b.pause && b.dwell == 0.0 => {
                let d = self.duration(b);
                let k = if d > 0.0 {
                    (self.cur_t / d).min(1.0)
                } else {
                    1.0
                };
                [0, 1, 2].map(|i| b.from[i] + (b.to[i] - b.from[i]) * k)
            }
            _ => self.interp_pos_after_planner(),
        }
    }

    fn interp_pos_after_planner(&self) -> [f64; 3] {
        // pozycja po ostatnim wykonanym bloku (planer pusty) albo przed pierwszym
        match self.planner.front() {
            Some(b) => b.from,
            None => self.interp.st.pos,
        }
    }

    pub fn wco(&self) -> [f64; 3] {
        self.interp.st.offset()
    }

    /// Stan jako w raporcie (`Idle`, `Run`, `Hold:0`, `Jog`, `Alarm`, `Door:0`).
    pub fn state_name(&self) -> String {
        match self.state {
            EState::Alarm(_) => "Alarm".into(),
            EState::Door => "Door:0".into(),
            EState::Idle => {
                if self.hold && (!self.planner.is_empty() || !self.sd_run.is_empty()) {
                    "Hold:0".into()
                } else if self.planner.front().is_some_and(|b| b.jog) {
                    "Jog".into()
                } else if !self.planner.is_empty() || !self.sd_run.is_empty() {
                    "Run".into()
                } else {
                    "Idle".into()
                }
            }
        }
    }

    pub fn is_alarm(&self) -> bool {
        matches!(self.state, EState::Alarm(_))
    }

    pub fn burns(&self) -> Vec<Block> {
        self.trace.iter().filter(|b| b.on).copied().collect()
    }

    /// Otwarcie / zamknięcie drzwi (pokrywy).
    pub fn set_door(&mut self, open: bool) {
        if open {
            self.state = EState::Door;
            self.hold = true;
        } else if self.state == EState::Door {
            self.state = EState::Idle;
        }
    }

    fn status(&self) -> String {
        let m = self.mpos();
        let st = self.state_name();
        if self.cfg.variant.status_v09 {
            let o = self.wco();
            let st09 = st.split(':').next().unwrap_or("Idle").to_string();
            return format!(
                "<{st09},MPos:{},{},{},WPos:{},{},{}>",
                f3(m[0]),
                f3(m[1]),
                f3(m[2]),
                f3(m[0] - o[0]),
                f3(m[1] - o[1]),
                f3(m[2] - o[2])
            );
        }
        let o = self.wco();
        let (feed, s) = self
            .planner
            .front()
            .map(|b| (b.feed, b.s))
            .unwrap_or((0.0, 0.0));
        let mut r = format!(
            "<{st}|MPos:{},{},{}|Bf:{},{}|FS:{},{}|Ov:{},100,{}|WCO:{},{},{}",
            f3(m[0]),
            f3(m[1]),
            f3(m[2]),
            PLANNER_BLOCKS - 1 - self.planner.len().min(PLANNER_BLOCKS - 1),
            RX_SIZE - self.rx.len().min(RX_SIZE),
            feed.round(),
            s.round(),
            self.ov_feed,
            self.ov_power,
            f3(o[0]),
            f3(o[1]),
            f3(o[2])
        );
        if self.state == EState::Door {
            r.push_str("|Pn:D");
        }
        r.push('>');
        r
    }

    /// Bajty od nadawcy.
    pub fn feed(&mut self, bytes: &[u8]) {
        self.received.extend_from_slice(bytes);
        for &b in bytes {
            if self.xmodem.is_some() {
                self.xmodem_byte(b);
                continue;
            }
            match b {
                b'?' => {
                    let s = self.status();
                    self.say(&s);
                }
                b'!' => {
                    if self.planner.front().is_some_and(|b| b.jog) {
                        self.planner.retain(|b| !b.jog);
                        self.cur_t = 0.0;
                    } else {
                        self.hold = true;
                    }
                }
                b'~' => {
                    if self.state != EState::Door {
                        self.hold = false;
                    }
                }
                0x18 => self.reset(),
                0x85 => {
                    if self.planner.front().is_some_and(|b| b.jog) {
                        let at = self.mpos();
                        self.planner.retain(|b| !b.jog);
                        self.interp.st.pos = at;
                        self.cur_t = 0.0;
                    }
                }
                0x90..=0x9D if self.cfg.variant.overrides => self.override_byte(b),
                0x80..=0xFF => {}
                _ => {
                    self.rx.push(b);
                    if self.rx.len() > RX_SIZE {
                        self.overflow = true;
                    }
                    self.max_rx = self.max_rx.max(self.rx.len());
                }
            }
        }
        self.process_lines();
    }

    fn override_byte(&mut self, b: u8) {
        let clamp = |v: i64| v.clamp(10, 200) as u32;
        match b {
            0x90 => self.ov_feed = 100,
            0x91 => self.ov_feed = clamp(self.ov_feed as i64 + 10),
            0x92 => self.ov_feed = clamp(self.ov_feed as i64 - 10),
            0x93 => self.ov_feed = clamp(self.ov_feed as i64 + 1),
            0x94 => self.ov_feed = clamp(self.ov_feed as i64 - 1),
            0x99 => self.ov_power = 100,
            0x9A => self.ov_power = clamp(self.ov_power as i64 + 10),
            0x9B => self.ov_power = clamp(self.ov_power as i64 - 10),
            0x9C => self.ov_power = clamp(self.ov_power as i64 + 1),
            0x9D => self.ov_power = clamp(self.ov_power as i64 - 1),
            _ => {}
        }
    }

    fn reset(&mut self) {
        let moving = !self.planner.is_empty() || !self.sd_run.is_empty();
        if let Some(b) = self.planner.front() {
            let at = self.mpos();
            let _ = b;
            self.interp.st.pos = at;
        }
        self.planner.clear();
        self.sd_run.clear();
        self.rx.clear();
        self.hold = false;
        self.cur_t = 0.0;
        self.interp.st.spindle = Spindle::Off;
        self.interp.st.air = Default::default();
        self.interp.st.g92 = [0.0; 3];
        self.resets += 1;
        let keep_alarm = matches!(self.state, EState::Alarm(n) if n > 0);
        if !keep_alarm {
            self.state = EState::Idle;
        }
        self.boot(moving);
    }

    fn reply(&mut self, r: Result<(), u32>) {
        self.lines += 1;
        match r {
            Ok(()) => self.say("ok"),
            Err(c) => {
                self.errors.push((self.lines, c));
                self.say(&format!("error:{c}"));
            }
        }
    }

    fn process_lines(&mut self) {
        loop {
            if self.xmodem.is_some() {
                return;
            }
            if self.planner.len() >= PLANNER_BLOCKS - 1 {
                return;
            }
            let Some(nl) = self.rx.iter().position(|&b| b == b'\n') else {
                return;
            };
            let raw: Vec<u8> = self.rx.drain(..=nl).collect();
            let line = String::from_utf8_lossy(&raw).trim().to_string();
            let r = self.exec(&line);
            if self.xmodem.is_some() {
                // odpowiedź po zakończeniu transferu
                return;
            }
            self.reply(r);
        }
    }

    fn exec(&mut self, line: &str) -> Result<(), u32> {
        if line.is_empty() {
            return Ok(());
        }
        let alarm = matches!(self.state, EState::Alarm(_));
        if let Some(cmd) = line.strip_prefix('$') {
            return self.system(cmd, alarm);
        }
        if alarm {
            return Err(9);
        }
        if line.len() > 80 && !self.cfg.variant.sd {
            return Err(11);
        }
        let up = line.to_ascii_uppercase();
        if up.contains("G38.2") || up.contains("G38.3") {
            return self.probe_line(&up);
        }
        if up.contains("M73") && !matches!(self.cfg.variant.id, "grblhal" | "fluidnc") {
            return Err(20);
        }
        self.gcode(line, false)
    }

    fn gcode(&mut self, line: &str, jog: bool) -> Result<(), u32> {
        let ln = self.lines + 1;
        self.interp.opts.laser_mode = self.setting(32) == 1.0;
        let before = self.interp.run.events.len();
        let st_before = self.interp.st.clone();
        let info = self.interp.line(ln, line).map_err(|_| 2u32)?;
        if info.unsupported {
            self.interp.st = st_before;
            self.interp.run.events.truncate(before);
            return Err(20);
        }
        let evs: Vec<Event> = self.interp.run.events.drain(before..).collect();
        // posuw wymagany dla G1/G2/G3 (error:22)
        if evs
            .iter()
            .any(|e| matches!(e, Event::Move(m) if !m.rapid && m.feed <= 0.0))
        {
            self.interp.st = st_before;
            return Err(22);
        }
        let off = self.interp.st.offset();
        let mut prev = st_before.pos;
        for e in evs {
            let b = match e {
                Event::Move(m) => Block {
                    line: ln,
                    from: [m.from.x + off[0], m.from.y + off[1], 0.0],
                    to: [m.to.x + off[0], m.to.y + off[1], m.z + off[2]],
                    feed: m.feed,
                    rapid: m.rapid,
                    jog,
                    on: m.on && !jog,
                    s: m.s,
                    spindle: m.spindle,
                    air: m.air.any(),
                    tool: m.tool,
                    dwell: 0.0,
                    pause: false,
                },
                Event::Dwell {
                    seconds,
                    s,
                    on,
                    tool,
                    ..
                } => {
                    let p = self.interp.st.pos;
                    Block {
                        line: ln,
                        from: p,
                        to: p,
                        feed: 0.0,
                        rapid: false,
                        jog: false,
                        on,
                        s,
                        spindle: self.interp.st.spindle,
                        air: self.interp.st.air.any(),
                        tool,
                        dwell: seconds,
                        pause: false,
                    }
                }
                Event::Pause { .. } => {
                    let p = self.interp.st.pos;
                    Block {
                        line: ln,
                        from: p,
                        to: p,
                        feed: 0.0,
                        rapid: false,
                        jog: false,
                        on: false,
                        s: 0.0,
                        spindle: Spindle::Off,
                        air: false,
                        tool: 0,
                        dwell: 0.0,
                        pause: true,
                    }
                }
                _ => continue,
            };
            // Z z poprzedniej pozycji; postój i pauza w miejscu
            let mut b = b;
            if b.dwell > 0.0 || b.pause {
                b.from = prev;
                b.to = prev;
            } else {
                b.from[2] = prev[2];
            }
            prev = b.to;
            self.planner.push_back(b);
        }
        Ok(())
    }

    fn last_pos(&self) -> [f64; 3] {
        self.interp.st.pos
    }

    fn system(&mut self, cmd: &str, alarm: bool) -> Result<(), u32> {
        let busy = !self.planner.is_empty() || !self.sd_run.is_empty();
        let up = cmd.to_ascii_uppercase();
        if up.is_empty() {
            self.say("[HLP:$$ $# $G $I $N $x=val $J=line $X $H ~ ! ? ctrl-x]");
            return Ok(());
        }
        self.system2(cmd, &up, alarm, busy)
    }

    fn system2(&mut self, cmd: &str, up: &str, alarm: bool, busy: bool) -> Result<(), u32> {
        if up == "$" {
            // „$$” → cmd == "$"
            if busy {
                return Err(8);
            }
            let lines: Vec<String> = self
                .settings
                .iter()
                .map(|(k, v)| format!("${k}={v}"))
                .collect();
            for l in lines {
                self.say(&l);
            }
            return Ok(());
        }
        match up {
            "I" => {
                let v = self.cfg.variant;
                if !v.version.is_empty() {
                    self.say(v.version);
                    self.say(&format!("[OPT:V,{},{}]", PLANNER_BLOCKS - 1, RX_SIZE));
                }
                if !v.marker.is_empty() {
                    let first = v.marker.split('|').next().unwrap_or("");
                    self.say(&format!("[OEM:{first}]"));
                }
                Ok(())
            }
            "G" => {
                let st = &self.interp.st;
                let l = format!(
                    "[GC:G{} G54 G17 {} {} G94 {} M9 T{} F{} S{}]",
                    st.motion,
                    if st.inch { "G20" } else { "G21" },
                    if st.abs { "G90" } else { "G91" },
                    match st.spindle {
                        Spindle::Off => "M5",
                        Spindle::M3 => "M3",
                        Spindle::M4 => "M4",
                    },
                    st.tool,
                    st.feed.round(),
                    st.s.round()
                );
                self.say(&l);
                Ok(())
            }
            "#" => {
                let o = self.interp.st.wco;
                let g = self.interp.st.g92;
                self.say(&format!("[G54:{},{},{}]", f3(o[0]), f3(o[1]), f3(o[2])));
                self.say(&format!("[G92:{},{},{}]", f3(g[0]), f3(g[1]), f3(g[2])));
                let (p, ok) = self.probe.unwrap_or(([0.0; 3], false));
                self.say(&format!(
                    "[PRB:{},{},{}:{}]",
                    f3(p[0]),
                    f3(p[1]),
                    f3(p[2]),
                    u8::from(ok)
                ));
                Ok(())
            }
            "H" => {
                if self.setting(22) != 1.0 {
                    return Err(5);
                }
                if busy {
                    return Err(8);
                }
                self.interp.st.pos = [0.0; 3];
                self.state = EState::Idle;
                Ok(())
            }
            "X" => {
                if alarm {
                    self.state = EState::Idle;
                    self.say("[MSG:Caution: Unlocked]");
                }
                Ok(())
            }
            _ if up.starts_with("J=") => {
                if !self.cfg.variant.jog {
                    return Err(3);
                }
                if alarm {
                    return Err(9);
                }
                let body = &cmd[2..];
                // jog: tryb modalny tylko w tym wierszu
                let saved = (
                    self.interp.st.abs,
                    self.interp.st.inch,
                    self.interp.st.motion,
                    self.interp.st.feed,
                );
                let r = self.gcode(&format!("G1 {body}"), true);
                self.interp.st.abs = saved.0;
                self.interp.st.inch = saved.1;
                self.interp.st.motion = saved.2;
                self.interp.st.feed = saved.3;
                r.map_err(|_| 16)
            }
            _ if up.starts_with("SD/LIST") && self.cfg.variant.sd => {
                let files: Vec<String> = self
                    .sd
                    .iter()
                    .map(|(k, v)| format!("[FILE:{k}|SIZE:{}]", v.len()))
                    .collect();
                for f in files {
                    self.say(&f);
                }
                Ok(())
            }
            _ if up.starts_with("SD/RUN=") && self.cfg.variant.sd => {
                if alarm {
                    return Err(9);
                }
                let path = cmd[7..].trim().to_string();
                match self.sd.get(&path) {
                    Some(d) => {
                        let text = String::from_utf8_lossy(d).to_string();
                        self.sd_run = text.lines().map(str::to_string).collect();
                        Ok(())
                    }
                    None => Err(60),
                }
            }
            _ if up.starts_with("SD/DELETE=") && self.cfg.variant.sd => {
                let path = cmd[10..].trim().to_string();
                self.sd.remove(&path).map(|_| ()).ok_or(60)
            }
            _ if up.starts_with("XMODEM/RECEIVE=") && self.cfg.variant.sd => {
                // UNVERIFIED: FluidNC — $Xmodem/Receive=<plik> (wiki FluidNC „Xmodem”).
                let path = cmd[15..].trim().to_string();
                self.xmodem = Some(XRecv {
                    path,
                    data: Vec::new(),
                    buf: Vec::new(),
                    expect: 1,
                });
                self.out.push(b'C');
                Ok(())
            }
            _ => {
                // $N=v
                if let Some((k, v)) = cmd.split_once('=') {
                    if let Ok(id) = k.trim().parse::<u32>() {
                        if busy {
                            return Err(8);
                        }
                        if codes::setting_info(id).is_none() && !self.settings.contains_key(&id) {
                            return Err(3);
                        }
                        let val = codes::validate_setting(id, v).map_err(|_| 2u32)?;
                        let txt = if codes::setting_info(id)
                            .is_some_and(|i| i.kind == codes::SettingKind::Float)
                        {
                            format!("{val:.3}")
                        } else {
                            format!("{}", val as i64)
                        };
                        self.settings.insert(id, txt);
                        return Ok(());
                    }
                }
                Err(3)
            }
        }
    }

    fn probe_line(&mut self, up: &str) -> Result<(), u32> {
        let strict = up.contains("G38.2");
        let num = |l: char| -> Option<f64> {
            let i = up.find(l)?;
            let rest = &up[i + 1..];
            let end = rest
                .find(|c: char| !(c.is_ascii_digit() || c == '.' || c == '-'))
                .unwrap_or(rest.len());
            rest[..end].parse().ok()
        };
        let Some(dz) = num('Z') else { return Err(26) };
        if num('F').is_none() && self.interp.st.feed <= 0.0 {
            return Err(22);
        }
        let start = self.last_pos();
        let off = self.interp.st.offset();
        let target_z = if self.interp.st.abs {
            dz + off[2]
        } else {
            start[2] + dz
        };
        let hit = self
            .cfg
            .probe_z
            .filter(|&s| (start[2] >= s && s >= target_z) || (start[2] <= s && s <= target_z));
        match hit {
            Some(z) => {
                let p = [start[0], start[1], z];
                self.interp.st.pos = p;
                self.probe = Some((p, true));
                self.say(&format!("[PRB:{},{},{}:1]", f3(p[0]), f3(p[1]), f3(p[2])));
                Ok(())
            }
            None => {
                let p = [start[0], start[1], target_z];
                self.interp.st.pos = p;
                self.probe = Some((p, false));
                self.say(&format!("[PRB:{},{},{}:0]", f3(p[0]), f3(p[1]), f3(p[2])));
                if strict {
                    self.state = EState::Alarm(5);
                    self.say("ALARM:5");
                }
                Ok(())
            }
        }
    }

    fn xmodem_byte(&mut self, b: u8) {
        let x = self.xmodem.as_mut().expect("xmodem");
        if x.buf.is_empty() && b == 0x04 {
            // EOT
            let mut data = std::mem::take(&mut x.data);
            while data.last() == Some(&0x1A) {
                data.pop();
            }
            let path = x.path.clone();
            self.xmodem = None;
            self.sd.insert(path, data);
            self.out.push(0x06);
            self.reply(Ok(()));
            self.process_lines();
            return;
        }
        x.buf.push(b);
        if x.buf.len() == 133 {
            let pkt = std::mem::take(&mut x.buf);
            let ok = pkt[0] == 0x01
                && pkt[1] == x.expect
                && pkt[2] == !x.expect
                && crate::xmodem::crc16(&pkt[3..131]) == u16::from_be_bytes([pkt[131], pkt[132]]);
            if ok {
                x.data.extend_from_slice(&pkt[3..131]);
                x.expect = x.expect.wrapping_add(1);
                self.out.push(0x06);
            } else {
                self.out.push(0x15);
            }
        }
    }

    fn duration(&self, b: &Block) -> f64 {
        if b.dwell > 0.0 {
            return b.dwell;
        }
        if b.pause {
            return 0.0;
        }
        let rate = if b.rapid || b.feed <= 0.0 {
            self.setting(110).max(1.0)
        } else {
            b.feed * self.ov_feed as f64 / 100.0
        };
        b.len() / (rate / 60.0)
    }

    /// Upływ czasu rzeczywistego `dt` s (z mnożnikiem z konfiguracji).
    pub fn advance(&mut self, dt: f64) {
        let mut budget = if self.cfg.speed.is_infinite() {
            f64::INFINITY
        } else {
            dt * self.cfg.speed
        };
        loop {
            // wiersze z karty SD jak strumień
            while self.planner.len() < PLANNER_BLOCKS - 1 && !self.hold && self.xmodem.is_none() {
                let Some(l) = self.sd_run.pop_front() else {
                    break;
                };
                let r = self.exec(l.trim());
                if r.is_err() {
                    self.sd_run.clear();
                }
            }
            if self.hold || matches!(self.state, EState::Alarm(_) | EState::Door) {
                return;
            }
            let Some(b) = self.planner.front().copied() else {
                self.process_lines();
                if self.planner.is_empty() {
                    return;
                }
                continue;
            };
            if b.pause {
                self.planner.pop_front();
                self.hold = true;
                self.trace.push(b);
                return;
            }
            let d = self.duration(&b);
            let left = d - self.cur_t;
            if budget < left {
                self.cur_t += budget;
                self.clock += budget;
                return;
            }
            budget -= left;
            self.clock += left;
            self.cur_t = 0.0;
            self.planner.pop_front();
            let mut done = b;
            done.s = b.s * self.ov_power as f64 / 100.0;
            self.trace.push(done);
            self.process_lines();
        }
    }

    /// Najwyżej `max` bajtów do nadawcy.
    pub fn take_output_max(&mut self, max: usize) -> Vec<u8> {
        if self.out.len() <= max {
            return std::mem::take(&mut self.out);
        }
        let rest = self.out.split_off(max);
        std::mem::replace(&mut self.out, rest)
    }

    /// Nowe połączenie (TCP/WebSocket/otwarcie portu): powitanie jak po resecie płytki.
    pub fn greet(&mut self) {
        let w = self.cfg.variant.welcome;
        self.say("");
        self.say(w);
    }

    /// Bajty do nadawcy (opróżnia bufor wyjścia).
    pub fn take_output(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.out)
    }

    /// Bloki w planerze.
    pub fn planner_len(&self) -> usize {
        self.planner.len()
    }
}

/// Tor emulatora jako zadanie (podgląd PNG porównywany z planerem, REQ-19-012).
pub fn trace_job(trace: &[Block], s_max: f64) -> bc_plan::Job {
    use bc_plan::{Move, Op, OpKind, OpSettings};
    let mut moves = Vec::new();
    for b in trace {
        if b.pause {
            continue;
        }
        let to = b.to_pt();
        if b.dwell > 0.0 {
            moves.push(Move::Dwell {
                s: b.dwell,
                power: if b.on {
                    (b.s / s_max * 100.0).min(100.0)
                } else {
                    0.0
                },
            });
        } else if b.rapid || b.jog {
            moves.push(Move::Travel { to, lift: false });
        } else {
            moves.push(Move::Cut {
                to,
                power: if b.on {
                    (b.s / s_max * 100.0).min(100.0)
                } else {
                    0.0
                },
                speed: b.feed / 60.0,
            });
        }
    }
    let op = Op {
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
            dynamic_power: true,
            pwm_hz: None,
            air_assist: false,
        },
        pause: None,
        delay_s: 0.0,
        z: 0.0,
        objects: Vec::new(),
        moves,
    };
    bc_plan::Job {
        ops: vec![op],
        warnings: Vec::new(),
        start: trace.first().map(|b| b.from_pt()).unwrap_or(Point::ORIGIN),
        origin: Point::ORIGIN,
    }
}
