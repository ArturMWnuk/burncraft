//! Interpreter G-code (ADR 0011 §8): wykonuje program i zwraca ruchy ze stanem wiązki,
//! air assist, narzędzia i postępu. Służy do importu i podglądu programów G-code oraz
//! jako emulator semantyki G-code w testach kompilatora. Pełny emulator GRBL (bufor,
//! protokół, raporty `?`, alarmy) powstaje w M2.3 (ADR 0010).

use bc_geom::Point;

/// Opcje interpretera.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SimOptions {
    /// Tryb laserowy GRBL (`$32=1`): G0 i postój w M4 bez wiązki.
    pub laser_mode: bool,
    /// Wartość S dla 100 % mocy.
    pub s_max: f64,
}

impl Default for SimOptions {
    fn default() -> Self {
        SimOptions {
            laser_mode: true,
            s_max: 1000.0,
        }
    }
}

/// Stan wrzeciona / lasera.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Spindle {
    #[default]
    Off,
    M3,
    M4,
}

/// Air assist: M7 (mist) i M8 (flood).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Air {
    pub mist: bool,
    pub flood: bool,
}

impl Air {
    pub fn any(self) -> bool {
        self.mist || self.flood
    }
}

/// Ruch wykonany przez interpreter (mm, układ programu).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SimMove {
    pub line: usize,
    pub from: Point,
    pub to: Point,
    pub z: f64,
    pub rapid: bool,
    pub s: f64,
    pub spindle: Spindle,
    /// Wiązka świeci w trakcie ruchu.
    pub on: bool,
    /// mm/min
    pub feed: f64,
    pub tool: u32,
    pub air: Air,
}

/// Zdarzenie programu w kolejności wykonania.
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    Move(SimMove),
    Dwell {
        line: usize,
        seconds: f64,
        s: f64,
        on: bool,
        tool: u32,
    },
    /// M0 / M1 z ostatnim komentarzem przed pauzą.
    Pause {
        line: usize,
        comment: Option<String>,
    },
    /// M73 P… R….
    Progress {
        line: usize,
        percent: f64,
        minutes: Option<f64>,
    },
    /// Wybór narzędzia (słowo T).
    Tool {
        line: usize,
        tool: u32,
    },
    /// M2 / M30.
    End {
        line: usize,
    },
}

/// Wynik wykonania programu.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Run {
    pub events: Vec<Event>,
    /// Nieobsługiwane słowa i polecenia (bez powtórzeń).
    pub warnings: Vec<String>,
    pub lines: usize,
    /// Wiersze z ruchem albo poleceniem (bez pustych i komentarzy).
    pub code_lines: usize,
}

impl Run {
    pub fn moves(&self) -> impl Iterator<Item = &SimMove> {
        self.events.iter().filter_map(|e| match e {
            Event::Move(m) => Some(m),
            _ => None,
        })
    }

    /// Wartość S wiązki w punkcie `p` (pierwszy ruch z wiązką przechodzący przez punkt).
    pub fn burn_at(&self, p: Point, tol: f64) -> Option<f64> {
        self.moves()
            .filter(|m| m.on)
            .find(|m| dist_to_segment(p, m.from, m.to) <= tol)
            .map(|m| m.s)
    }
}

/// Błąd programu (niepoprawne słowo).
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
#[error("wiersz {line}: {message}")]
pub struct SimError {
    pub line: usize,
    pub message: String,
}

pub fn dist_to_segment(p: Point, a: Point, b: Point) -> f64 {
    let ab = b - a;
    let l2 = ab.hypot2();
    if l2 == 0.0 {
        return p.distance(a);
    }
    let t = ((p - a).dot(ab) / l2).clamp(0.0, 1.0);
    p.distance(a + ab * t)
}

/// Stan interpretera (współrzędne maszyny w mm).
#[derive(Debug, Clone, PartialEq)]
pub struct State {
    pub pos: [f64; 3],
    /// Przesunięcie G92 (pozycja programu = maszyna − WCO − G92).
    pub g92: [f64; 3],
    /// Przesunięcie układu roboczego G54 (G10 L20 P1).
    pub wco: [f64; 3],
    pub motion: u8,
    pub abs: bool,
    pub inch: bool,
    /// mm/min
    pub feed: f64,
    pub s: f64,
    pub spindle: Spindle,
    pub air: Air,
    pub tool: u32,
    pub comment: Option<String>,
}

impl Default for State {
    fn default() -> Self {
        State {
            pos: [0.0; 3],
            g92: [0.0; 3],
            wco: [0.0; 3],
            motion: 0,
            abs: true,
            inch: false,
            feed: 0.0,
            s: 0.0,
            spindle: Spindle::Off,
            air: Air::default(),
            tool: 0,
            comment: None,
        }
    }
}

impl State {
    /// Łączne przesunięcie programu względem maszyny.
    pub fn offset(&self) -> [f64; 3] {
        [
            self.wco[0] + self.g92[0],
            self.wco[1] + self.g92[1],
            self.wco[2] + self.g92[2],
        ]
    }
}

fn warn(w: &mut Vec<String>, msg: String) {
    if !w.contains(&msg) {
        w.push(msg);
    }
}

/// Wynik jednego wiersza.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct LineInfo {
    /// Wiersz z kodem (nie pusty, nie sam komentarz).
    pub code: bool,
    /// Wiersz zawierał nieobsługiwane polecenie (GRBL: `error:20`).
    pub unsupported: bool,
}

/// Interpreter przyrostowy: wiersz po wierszu (używa go emulator GRBL).
#[derive(Debug, Clone)]
pub struct Interp {
    pub st: State,
    pub opts: SimOptions,
    pub run: Run,
}

impl Interp {
    pub fn new(opts: SimOptions) -> Self {
        Interp {
            st: State::default(),
            opts,
            run: Run::default(),
        }
    }

    /// Wykonuje jeden wiersz (numer `ln` od 1); zdarzenia trafiają do `run.events`.
    pub fn line(&mut self, ln: usize, raw: &str) -> Result<LineInfo, SimError> {
        let o = self.opts;
        let st = &mut self.st;
        let r = &mut self.run;
        let mut info = LineInfo::default();
        r.lines = r.lines.max(ln);
        let mut code = String::new();
        let mut comment: Option<String> = None;
        let mut in_paren = false;
        let mut paren = String::new();
        let mut chars = raw.chars();
        while let Some(c) = chars.next() {
            if in_paren {
                if c == ')' {
                    in_paren = false;
                    comment = Some(paren.trim().to_string());
                    paren.clear();
                } else {
                    paren.push(c);
                }
            } else if c == '(' {
                in_paren = true;
            } else if c == ';' {
                comment = Some(chars.as_str().trim().to_string());
                break;
            } else {
                code.push(c);
            }
        }
        if let Some(c) = comment {
            if !c.is_empty() {
                st.comment = Some(c);
            }
        }
        let code = code.trim();
        if code.is_empty() || code == "%" {
            return Ok(info);
        }
        info.code = true;
        if code.starts_with('$') {
            warn(
                &mut r.warnings,
                format!("polecenia systemowe GRBL ({code}) są pomijane"),
            );
            return Ok(info);
        }
        r.code_lines += 1;
        let up: Vec<char> = code
            .chars()
            .filter(|c| !c.is_whitespace())
            .map(|c| c.to_ascii_uppercase())
            .collect();
        let mut words: Vec<(char, f64, String)> = Vec::new();
        let mut k = 0;
        while k < up.len() {
            let letter = up[k];
            if !letter.is_ascii_alphabetic() {
                return Err(SimError {
                    line: ln,
                    message: format!("niepoprawny znak '{letter}'"),
                });
            }
            k += 1;
            let start = k;
            while k < up.len() && (up[k].is_ascii_digit() || matches!(up[k], '.' | '-' | '+')) {
                k += 1;
            }
            let num: String = up[start..k].iter().collect();
            let v: f64 = num.parse().map_err(|_| SimError {
                line: ln,
                message: format!("niepoprawna liczba w słowie {letter}{num}"),
            })?;
            words.push((letter, v, num));
        }
        let get = |l: char| words.iter().find(|w| w.0 == l).map(|w| w.1);
        let unit = |st: &State, v: f64| if st.inch { v * 25.4 } else { v };
        let mut motion: Option<u8> = None;
        let mut non_modal: Option<u32> = None;
        let mut events_after: Vec<Event> = Vec::new();
        for (l, v, raw) in &words {
            match l {
                'G' => {
                    let g = (v * 10.0).round() as i64;
                    match g {
                        0 => motion = Some(0),
                        10 => motion = Some(1),
                        20 => motion = Some(2),
                        30 => motion = Some(3),
                        40 => non_modal = Some(4),
                        100 => non_modal = Some(10),
                        200 => st.inch = true,
                        210 => st.inch = false,
                        900 => st.abs = true,
                        910 => st.abs = false,
                        920 => non_modal = Some(92),
                        921 => st.g92 = [0.0; 3],
                        170 | 940 | 540 | 800 => {}
                        _ => {
                            info.unsupported = true;
                            warn(
                                &mut r.warnings,
                                format!("nieobsługiwane G{raw} (pominięte)"),
                            )
                        }
                    }
                }
                'M' => match *v as i64 {
                    0 | 1 => events_after.push(Event::Pause {
                        line: ln,
                        comment: st.comment.clone(),
                    }),
                    2 | 30 => {
                        st.spindle = Spindle::Off;
                        st.air = Air::default();
                        st.abs = true;
                        st.motion = 1;
                        events_after.push(Event::End { line: ln });
                    }
                    3 => st.spindle = Spindle::M3,
                    4 => st.spindle = Spindle::M4,
                    5 => st.spindle = Spindle::Off,
                    7 => st.air.mist = true,
                    8 => st.air.flood = true,
                    9 => st.air = Air::default(),
                    73 => events_after.push(Event::Progress {
                        line: ln,
                        percent: get('P').unwrap_or(0.0),
                        minutes: get('R'),
                    }),
                    _ => {
                        info.unsupported = true;
                        warn(
                            &mut r.warnings,
                            format!("nieobsługiwane M{raw} (pominięte)"),
                        )
                    }
                },
                'S' => st.s = *v,
                'F' => st.feed = unit(st, *v),
                'T' => {
                    st.tool = *v as u32;
                    events_after.push(Event::Tool {
                        line: ln,
                        tool: st.tool,
                    });
                }
                'X' | 'Y' | 'Z' | 'I' | 'J' | 'P' | 'R' | 'N' | 'L' => {}
                other => {
                    info.unsupported = true;
                    warn(
                        &mut r.warnings,
                        format!("nieobsługiwane słowo {other} (pominięte)"),
                    )
                }
            }
        }
        if let Some(m) = motion {
            st.motion = m;
        }
        let has_axes = ['X', 'Y', 'Z'].iter().any(|l| get(*l).is_some());
        let target = |st: &State| -> [f64; 3] {
            let mut t = st.pos;
            let off = st.offset();
            for (ax, l) in ['X', 'Y', 'Z'].iter().enumerate() {
                if let Some(v) = get(*l) {
                    let v = unit(st, v);
                    t[ax] = if st.abs { v + off[ax] } else { st.pos[ax] + v };
                }
            }
            t
        };
        match non_modal {
            Some(4) => {
                let secs = get('P').unwrap_or(0.0);
                let on = st.s > 0.0
                    && match st.spindle {
                        Spindle::Off => false,
                        Spindle::M3 => true,
                        Spindle::M4 => !o.laser_mode,
                    };
                r.events.push(Event::Dwell {
                    line: ln,
                    seconds: secs,
                    s: st.s,
                    on,
                    tool: st.tool,
                });
            }
            Some(92) => {
                for (ax, l) in ['X', 'Y', 'Z'].iter().enumerate() {
                    if let Some(v) = get(*l) {
                        st.g92[ax] = st.pos[ax] - st.wco[ax] - unit(st, v);
                    }
                }
            }
            Some(10) => {
                // G10 L20 P1: bieżąca pozycja = podane współrzędne w G54
                if get('L') == Some(20.0) {
                    for (ax, l) in ['X', 'Y', 'Z'].iter().enumerate() {
                        if let Some(v) = get(*l) {
                            st.wco[ax] = st.pos[ax] - st.g92[ax] - unit(st, v);
                        }
                    }
                } else {
                    info.unsupported = true;
                }
            }
            _ if has_axes || (st.motion >= 2 && (get('I').is_some() || get('J').is_some())) => {
                let to = target(st);
                let from = st.pos;
                let rapid = st.motion == 0;
                let on = st.s > 0.0 && st.spindle != Spindle::Off && (!rapid || !o.laser_mode);
                let off = st.offset();
                let mk = |a: [f64; 3], b: [f64; 3], st: &State| SimMove {
                    line: ln,
                    from: Point::new(a[0] - off[0], a[1] - off[1]),
                    to: Point::new(b[0] - off[0], b[1] - off[1]),
                    z: b[2] - off[2],
                    rapid,
                    s: st.s,
                    spindle: st.spindle,
                    on,
                    feed: st.feed,
                    tool: st.tool,
                    air: st.air,
                };
                if st.motion >= 2 {
                    let (i, j) = (
                        unit(st, get('I').unwrap_or(0.0)),
                        unit(st, get('J').unwrap_or(0.0)),
                    );
                    if get('R').is_some() {
                        warn(
                            &mut r.warnings,
                            "łuki G2/G3 z R są zamieniane na odcinki proste".into(),
                        );
                        r.events.push(Event::Move(mk(from, to, st)));
                    } else {
                        let pts = arc(from, to, [from[0] + i, from[1] + j], st.motion == 2);
                        let mut a = from;
                        for p in pts {
                            r.events.push(Event::Move(mk(a, p, st)));
                            a = p;
                        }
                    }
                } else {
                    r.events.push(Event::Move(mk(from, to, st)));
                }
                st.pos = to;
            }
            _ => {}
        }
        r.events.extend(events_after);
        Ok(info)
    }
}

/// Wykonuje program G-code.
pub fn run(text: &str, o: &SimOptions) -> Result<Run, SimError> {
    let mut it = Interp::new(*o);
    for (i, raw) in text.lines().enumerate() {
        it.line(i + 1, raw)?;
    }
    Ok(it.run)
}
/// Spłaszczenie łuku (tolerancja 0,01 mm). `cw` = G2.
fn arc(from: [f64; 3], to: [f64; 3], c: [f64; 2], cw: bool) -> Vec<[f64; 3]> {
    let r = ((from[0] - c[0]).powi(2) + (from[1] - c[1]).powi(2)).sqrt();
    let a0 = (from[1] - c[1]).atan2(from[0] - c[0]);
    let a1 = (to[1] - c[1]).atan2(to[0] - c[0]);
    let tau = std::f64::consts::TAU;
    let mut sweep = a1 - a0;
    if cw {
        while sweep >= 0.0 {
            sweep -= tau;
        }
    } else {
        while sweep <= 0.0 {
            sweep += tau;
        }
    }
    if r < 1e-9 {
        return vec![to];
    }
    let step = 2.0 * (1.0 - 0.01 / r).clamp(-1.0, 1.0).acos();
    let n = ((sweep.abs() / step.max(1e-3)).ceil() as usize).clamp(1, 10_000);
    (1..=n)
        .map(|k| {
            if k == n {
                to
            } else {
                let t = k as f64 / n as f64;
                let a = a0 + sweep * t;
                [
                    c[0] + r * a.cos(),
                    c[1] + r * a.sin(),
                    from[2] + (to[2] - from[2]) * t,
                ]
            }
        })
        .collect()
}
