//! Kompilator G-code (ADR 0011): dialekty `grbl` (tryb laserowy) i `gcode` (ogólny CNC),
//! interpreter (`sim`) i import programów G-code.

mod import;
pub mod sim;

pub use import::{import, ImportOptions, ImportReport, Imported};

use std::collections::BTreeSet;

use bc_doc::{AirCode, GcodeSettings, GcodeUnits, StartFrom};
use bc_geom::Point;
use bc_plan::{Job, Move, Op};

use crate::{CompileError, CompileWarning, Context, JobCompiler, MachineFile, ProgressFn};

/// Dialekt G-code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dialect {
    /// GRBL i pochodne w trybie laserowym (`$32=1`): G0 wyłącza wiązkę, M4 = moc dynamiczna.
    Grbl,
    /// Ogólny CNC (Mach3/Mach4, UCCNC, Masso, LinuxCNC): wiązka jak wrzeciono, zawsze M3.
    Generic,
}

pub struct GcodeCompiler {
    pub dialect: Dialect,
}

impl GcodeCompiler {
    pub fn new(dialect: Dialect) -> Self {
        GcodeCompiler { dialect }
    }
}

impl JobCompiler for GcodeCompiler {
    fn name(&self) -> &'static str {
        match self.dialect {
            Dialect::Grbl => "grbl",
            Dialect::Generic => "gcode",
        }
    }

    fn extension(&self) -> &'static str {
        match self.dialect {
            Dialect::Grbl => "gcode",
            Dialect::Generic => "nc",
        }
    }

    fn compile(
        &self,
        job: &Job,
        ctx: &Context,
        progress: &mut ProgressFn,
    ) -> Result<MachineFile, CompileError> {
        let mut e = Emitter::new(self.dialect, self.name(), job, ctx);
        e.run(progress)?;
        Ok(MachineFile::new(
            self.name(),
            self.extension(),
            e.out,
            e.total_s,
            e.warnings,
        ))
    }
}

// ---------- liczby ----------

/// Liczba z `dec` miejscami po przecinku; `compress` usuwa zbędne zera (`0.500` → `.5`).
pub fn fmt_num(v: f64, dec: u8, compress: bool) -> String {
    let m = 10f64.powi(dec as i32);
    fmt_q((v * m).round() as i64, dec, compress)
}

/// Liczba zapisana jako całkowita wielokrotność 10^-dec.
fn fmt_q(q: i64, dec: u8, compress: bool) -> String {
    let neg = q < 0;
    let a = q.unsigned_abs();
    let m = 10u64.pow(dec as u32);
    let (ip, fp) = (a / m, a % m);
    let mut s = if dec == 0 {
        ip.to_string()
    } else {
        format!("{ip}.{fp:0w$}", w = dec as usize)
    };
    if compress {
        if s.contains('.') {
            while s.ends_with('0') {
                s.pop();
            }
            if s.ends_with('.') {
                s.pop();
            }
        }
        if let Some(rest) = s.strip_prefix("0.") {
            s = format!(".{rest}");
        }
    }
    if neg && a != 0 {
        s.insert(0, '-');
    }
    s
}

/// Słowa S i F: zawsze skrócone, 2 miejsca po przecinku.
fn fmt_word(v: f64) -> String {
    fmt_num(v, 2, true)
}

fn hms(s: f64) -> String {
    let t = s.max(0.0).round() as u64;
    format!("{:02}:{:02}:{:02}", t / 3600, t / 60 % 60, t % 60)
}

/// Transliteracja do ASCII (komentarze i szablony): polskie znaki bez ogonków, reszta `?`.
pub fn ascii(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            'ą' => 'a',
            'ć' => 'c',
            'ę' => 'e',
            'ł' => 'l',
            'ń' => 'n',
            'ó' => 'o',
            'ś' => 's',
            'ź' | 'ż' => 'z',
            'Ą' => 'A',
            'Ć' => 'C',
            'Ę' => 'E',
            'Ł' => 'L',
            'Ń' => 'N',
            'Ó' => 'O',
            'Ś' => 'S',
            'Ź' | 'Ż' => 'Z',
            '\t' => ' ',
            c if c.is_ascii() && !c.is_ascii_control() => c,
            _ => '?',
        })
        .collect()
}

/// Rozwija pola `{nazwa}`; nieznane pola zostają bez zmian i trafiają do `unknown`.
pub fn expand(template: &str, vars: &[(&str, String)], unknown: &mut BTreeSet<String>) -> String {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(i) = rest.find('{') {
        out.push_str(&rest[..i]);
        let after = &rest[i + 1..];
        match after.find('}') {
            Some(j)
                if j > 0
                    && after[..j]
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b == b'_') =>
            {
                let key = &after[..j];
                match vars.iter().find(|(k, _)| *k == key) {
                    Some((_, v)) => out.push_str(v),
                    None => {
                        unknown.insert(key.to_string());
                        out.push_str(&rest[i..i + j + 2]);
                    }
                }
                rest = &after[j + 1..];
            }
            _ => {
                out.push('{');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// Punkt origin zadania (ADR 0011 §4): (0, 0) dla `absolute`, inaczej punkt ramki cięcia.
pub fn job_anchor(job: &Job, start: &bc_doc::StartSettings) -> Point {
    match start.from {
        StartFrom::Absolute => Point::ORIGIN,
        _ => match job.stats().bbox {
            Some([x0, y0, x1, y1]) => {
                let (fx, fy) = start.origin.fractions();
                Point::new(x0 + (x1 - x0) * fx, y0 + (y1 - y0) * fy)
            }
            None => Point::ORIGIN,
        },
    }
}

/// Ramka zadania (REQ-10-007, 008) w układzie pliku (względem origin zadania), mm:
/// prostokąt obejmujący cięcie albo kontur — otoczka wypukła odcinków z wiązką.
pub fn frame_points(job: &Job, start: &bc_doc::StartSettings, contour: bool) -> Vec<[f64; 2]> {
    let a = job_anchor(job, start);
    let mut pts: Vec<Point> = Vec::new();
    for s in job.segments() {
        if s.kind == bc_plan::SegKind::Cut && s.power > 0.0
            || s.kind == bc_plan::SegKind::Dwell && s.power > 0.0
        {
            pts.push(s.from);
            pts.push(s.to);
        }
    }
    if pts.is_empty() {
        return Vec::new();
    }
    let out: Vec<Point> = if contour {
        hull(pts)
    } else {
        let [x0, y0, x1, y1] = job.stats().bbox.unwrap_or([0.0; 4]);
        vec![
            Point::new(x0, y0),
            Point::new(x1, y0),
            Point::new(x1, y1),
            Point::new(x0, y1),
        ]
    };
    out.into_iter()
        .map(|p| {
            let r = |v: f64| (v * 1000.0).round() / 1000.0;
            [r(p.x - a.x), r(p.y - a.y)]
        })
        .collect()
}

/// Otoczka wypukła (monotone chain), przeciwnie do ruchu wskazówek zegara.
fn hull(mut p: Vec<Point>) -> Vec<Point> {
    p.sort_by(|a, b| a.x.total_cmp(&b.x).then(a.y.total_cmp(&b.y)));
    p.dedup_by(|a, b| a.distance(*b) < 1e-9);
    if p.len() < 3 {
        return p;
    }
    let cross =
        |o: Point, a: Point, b: Point| (a.x - o.x) * (b.y - o.y) - (a.y - o.y) * (b.x - o.x);
    let mut lower: Vec<Point> = Vec::new();
    for &q in &p {
        while lower.len() >= 2 && cross(lower[lower.len() - 2], lower[lower.len() - 1], q) <= 1e-12
        {
            lower.pop();
        }
        lower.push(q);
    }
    let mut upper: Vec<Point> = Vec::new();
    for &q in p.iter().rev() {
        while upper.len() >= 2 && cross(upper[upper.len() - 2], upper[upper.len() - 1], q) <= 1e-12
        {
            upper.pop();
        }
        upper.push(q);
    }
    lower.pop();
    upper.pop();
    lower.extend(upper);
    lower
}

// ---------- emiter ----------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Beam {
    Off,
    M3,
    M4,
}

/// Jedno wykonanie operacji: (indeks operacji, laser 0/1).
#[derive(Debug, Clone, Copy)]
struct Pass {
    op: usize,
    laser: usize,
}

struct Emitter<'a> {
    d: Dialect,
    driver: &'static str,
    job: &'a Job,
    ctx: &'a Context,
    set: &'a GcodeSettings,
    out: Vec<String>,
    warnings: Vec<CompileWarning>,
    unknown: BTreeSet<String>,
    rel: bool,
    scale: f64,
    anchor: Point,
    /// Pozycja w jednostkach 10^-dec (po zaokrągleniu).
    pos: Option<(i64, i64)>,
    z: Option<i64>,
    motion: Option<u8>,
    feed: Option<String>,
    s: Option<String>,
    beam: Beam,
    air: bool,
    tool: Option<usize>,
    passes: Vec<Pass>,
    /// Czas na koniec każdego ruchu każdego przebiegu.
    times: Vec<Vec<f64>>,
    total_s: f64,
    last_pct: i64,
}

impl<'a> Emitter<'a> {
    fn new(d: Dialect, driver: &'static str, job: &'a Job, ctx: &'a Context) -> Self {
        let set = &ctx.settings;
        let rel = ctx.start.from == StartFrom::Current;
        let anchor = match ctx.start.from {
            StartFrom::Absolute => Point::ORIGIN,
            _ => match job.stats().bbox {
                Some([x0, y0, x1, y1]) => {
                    let (fx, fy) = ctx.start.origin.fractions();
                    Point::new(x0 + (x1 - x0) * fx, y0 + (y1 - y0) * fy)
                }
                None => Point::ORIGIN,
            },
        };
        let mut passes = Vec::new();
        let mut warned_l2 = BTreeSet::new();
        let mut warnings = Vec::new();
        for (i, op) in job.ops.iter().enumerate() {
            let uses2 = op.settings.power2_max.is_some_and(|p| p > 0.0);
            if !uses2 || op.settings.power_max > 0.0 || !set.laser2.enabled {
                passes.push(Pass { op: i, laser: 0 });
            }
            if uses2 {
                if set.laser2.enabled {
                    passes.push(Pass { op: i, laser: 1 });
                } else if warned_l2.insert(op.layer) {
                    warnings.push(CompileWarning::new(
                        "laser2_disabled",
                        format!(
                            "warstwa {}: moc lasera 2 jest ustawiona, ale drugi laser jest wyłączony w ustawieniach G-code",
                            op.label
                        ),
                    ));
                }
            }
        }
        if !set.use_z && job.ops.iter().any(|o| o.z.abs() > 1e-9) {
            warnings.push(CompileWarning::new(
                "z_ignored",
                "warstwy mają Z offset/krok, ale oś Z jest wyłączona (use_z) — Z pominięte",
            ));
        }
        let mut e = Emitter {
            d,
            driver,
            job,
            ctx,
            set,
            out: Vec::new(),
            warnings,
            unknown: BTreeSet::new(),
            rel,
            scale: if set.units == GcodeUnits::Inch {
                1.0 / 25.4
            } else {
                1.0
            },
            anchor,
            pos: if rel { Some((0, 0)) } else { None },
            z: if rel { Some(0) } else { None },
            motion: None,
            feed: None,
            s: None,
            beam: Beam::Off,
            air: false,
            tool: None,
            passes,
            times: Vec::new(),
            total_s: 0.0,
            last_pct: 0,
        };
        e.timeline();
        e
    }

    /// Czas szacowany każdego ruchu (do M73 i `{time}`).
    fn timeline(&mut self) {
        let k = &self.ctx.kinematics;
        let mut at = self.job.start;
        let mut t = 0.0;
        for (pi, p) in self.passes.iter().enumerate() {
            let op = &self.job.ops[p.op];
            let first_of_op = pi == 0 || self.passes[pi - 1].op != p.op;
            let one = Job {
                ops: vec![op.clone()],
                warnings: Vec::new(),
                start: at,
                origin: self.job.origin,
            };
            let est = bc_plan::estimate(&one, k);
            let motion = (est.total_s - est.delay_s).max(0.0);
            let mut cur = at;
            let weights: Vec<f64> = op
                .moves
                .iter()
                .map(|m| match *m {
                    Move::Travel { to, .. } | Move::Cut { to, .. } => {
                        let w = cur.distance(to);
                        cur = to;
                        w
                    }
                    Move::Dwell { s, .. } => s,
                })
                .collect();
            let sum: f64 = weights.iter().sum();
            if first_of_op {
                t += op.delay_s;
            }
            let mut v = Vec::with_capacity(weights.len());
            for w in weights {
                if sum > 0.0 {
                    t += motion * w / sum;
                }
                v.push(t);
            }
            self.times.push(v);
            at = one.end();
        }
        self.total_s = t;
    }

    fn dec(&self) -> u8 {
        self.set.decimals
    }

    fn compress(&self) -> bool {
        self.set.compress
    }

    fn q(&self, v: f64) -> i64 {
        (v * self.scale * 10f64.powi(self.dec() as i32)).round() as i64
    }

    fn qp(&self, p: Point) -> (i64, i64) {
        (self.q(p.x - self.anchor.x), self.q(p.y - self.anchor.y))
    }

    fn line(&mut self, s: impl Into<String>) {
        self.out.push(s.into());
    }

    fn comment(&mut self, text: &str) {
        let t = ascii(text);
        let l = match self.d {
            Dialect::Grbl => format!("; {t}"),
            Dialect::Generic => format!("({})", t.replace(['(', ')'], "")),
        };
        self.line(l);
    }

    /// Wiersze szablonu lub makra (rozwinięte, ASCII, bez pustych).
    fn template(&mut self, text: &str, vars: &[(&str, String)]) {
        let mut unknown = std::mem::take(&mut self.unknown);
        let exp = expand(text, vars, &mut unknown);
        self.unknown = unknown;
        for l in exp.lines() {
            let l = ascii(l.trim_end());
            if l.trim().is_empty() {
                continue;
            }
            let l = match (self.d, l.trim_start().strip_prefix(';')) {
                (Dialect::Generic, Some(c)) => format!("({})", c.trim().replace(['(', ')'], "")),
                _ => l,
            };
            self.line(l);
        }
    }

    fn job_vars(&self) -> Vec<(&'static str, String)> {
        let b = self.job.stats().bbox.unwrap_or([0.0; 4]);
        let c = |v: f64, a: f64| fmt_num((v - a) * self.scale, self.dec(), true);
        vec![
            ("name", self.ctx.name.clone()),
            ("driver", self.driver.to_string()),
            ("time", hms(self.total_s)),
            ("time_s", format!("{}", self.total_s.round() as i64)),
            (
                "units",
                match self.set.units {
                    GcodeUnits::Mm => "mm".into(),
                    GcodeUnits::Inch => "inch".into(),
                },
            ),
            ("s_max", fmt_word(self.set.s_max)),
            ("passes", self.passes.len().to_string()),
            ("x0", c(b[0], self.anchor.x)),
            ("y0", c(b[1], self.anchor.y)),
            ("x1", c(b[2], self.anchor.x)),
            ("y1", c(b[3], self.anchor.y)),
        ]
    }

    fn s_of(&self, power: f64) -> f64 {
        power.clamp(0.0, 100.0) * self.set.s_max / 100.0
    }

    fn feed_of(&self, speed: f64) -> f64 {
        speed * 60.0 * self.scale
    }

    fn layer_vars(&self, op: &Op) -> Vec<(&'static str, String)> {
        vec![
            ("layer", op.layer.to_string()),
            ("label", op.label.clone()),
            ("pass", (op.pass + 1).to_string()),
            ("speed", fmt_word(self.feed_of(op.settings.speed))),
            ("power", fmt_word(self.s_of(op.settings.power_max))),
            ("power_pct", fmt_word(op.settings.power_max)),
        ]
    }

    /// Wiązka wyłączona (przed pauzą, opóźnieniem, zmianą narzędzia, na końcu).
    fn beam_off(&mut self) {
        if self.beam != Beam::Off {
            self.line("M5");
            self.beam = Beam::Off;
        }
    }

    fn set_air(&mut self, on: bool) {
        if on && !self.air {
            self.line(match self.set.air_code {
                AirCode::M7 => "M7",
                AirCode::M8 => "M8",
            });
        } else if !on && self.air {
            self.line("M9");
        }
        self.air = on;
    }

    /// Ruch G0/G1 (z kompresją); `false` = ruch zerowy (pominięty).
    fn motion(&mut self, g: u8, to: Point, s: Option<f64>, f: Option<f64>) -> bool {
        let (qx, qy) = self.qp(to);
        let c = self.compress();
        let dec = self.dec();
        let mut axes = Vec::with_capacity(2);
        let (px, py) = match self.pos {
            Some(p) => (Some(p.0), Some(p.1)),
            None => (None, None),
        };
        for (letter, qv, prev) in [('X', qx, px), ('Y', qy, py)] {
            // kompresja: oś bez zmiany pominięta; bez kompresji zawsze obie osie
            if c && prev == Some(qv) {
                continue;
            }
            let val = if self.rel { qv - prev.unwrap_or(0) } else { qv };
            axes.push(format!("{letter}{}", fmt_q(val, dec, c)));
        }
        if self.pos == Some((qx, qy)) {
            return false;
        }
        let mut words = Vec::with_capacity(5);
        if !c || self.motion != Some(g) {
            words.push(format!("G{g}"));
        }
        words.extend(axes);
        if let Some(s) = s {
            let w = fmt_word(s);
            if !c || self.s.as_deref() != Some(w.as_str()) {
                words.push(format!("S{w}"));
            }
            self.s = Some(w);
        }
        if let Some(f) = f {
            let w = fmt_word(f);
            if !c || self.feed.as_deref() != Some(w.as_str()) {
                words.push(format!("F{w}"));
            }
            self.feed = Some(w);
        }
        self.motion = Some(g);
        self.pos = Some((qx, qy));
        self.line(words.join(" "));
        true
    }

    fn z_to(&mut self, z: f64) {
        let qz = (z * self.scale * 10f64.powi(self.dec() as i32)).round() as i64;
        if self.z == Some(qz) {
            return;
        }
        let v = if self.rel {
            qz - self.z.unwrap_or(0)
        } else {
            qz
        };
        let w = fmt_q(v, self.dec(), self.compress());
        self.line(format!("G0 Z{w}"));
        self.motion = Some(0);
        self.z = Some(qz);
    }

    /// Tryb wiązki przed ruchem roboczym.
    fn ensure_cut_beam(&mut self, dynamic: bool) {
        match self.d {
            Dialect::Grbl => {
                let want = if dynamic { Beam::M4 } else { Beam::M3 };
                if self.beam != want {
                    let code = if want == Beam::M4 { "M4" } else { "M3" };
                    if self.beam == Beam::Off {
                        self.line(format!("{code} S0"));
                        self.s = Some("0".into());
                    } else {
                        self.line(code);
                    }
                    self.beam = want;
                }
            }
            Dialect::Generic => {
                if self.beam == Beam::Off {
                    self.line("M3 S0");
                    self.s = Some("0".into());
                    self.beam = Beam::M3;
                }
            }
        }
    }

    fn dwell(&mut self, seconds: f64, power_s: f64) {
        let p = fmt_num(seconds, 3, true);
        if power_s > 0.0 {
            let w = fmt_word(power_s);
            if self.beam == Beam::M3 && self.s.as_deref() == Some(w.as_str()) && self.compress() {
                self.line(format!("G4 P{p}"));
                return;
            }
            // UNVERIFIED: GRBL 1.1 (wiki „Laser Mode”) — w M3 wiązka świeci także bez ruchu,
            // w M4 gaśnie; postój z wiązką (tryb punktowy) wymaga więc M3. Sprawdzić na sprzęcie.
            self.line(format!("M3 S{w}"));
            self.s = Some(w);
            self.beam = Beam::M3;
            self.line(format!("G4 P{p}"));
        } else {
            self.beam_off();
            self.line(format!("G4 P{p}"));
        }
    }

    fn progress_m73(&mut self, t: f64, force: bool) {
        if !self.set.progress_m73 {
            return;
        }
        let total = self.total_s;
        let pct = if total > 0.0 {
            ((t / total) * 100.0).floor() as i64
        } else {
            0
        }
        .clamp(0, 100);
        if force || (pct > self.last_pct && pct < 100) {
            let r = ((total - t).max(0.0) / 60.0).ceil() as i64;
            // UNVERIFIED: M73 P/R (konwencja Marlin/Prusa) — standardowy GRBL 1.1 odrzuca M73
            // (error:20); grblHAL i część odmian producentów ją akceptują. Domyślnie wyłączone.
            self.line(format!("M73 P{pct} R{r}"));
            self.last_pct = pct;
        }
    }

    fn run(&mut self, progress: &mut ProgressFn) -> Result<(), CompileError> {
        let set = self.set;
        let jv = self.job_vars();
        self.template(&set.header, &jv);
        if set.units_block.trim().is_empty() {
            self.line("G90");
            self.line(match set.units {
                GcodeUnits::Mm => "G21",
                GcodeUnits::Inch => "G20",
            });
        } else {
            self.template(&set.units_block, &jv);
        }
        if self.rel {
            self.line("G91");
        }
        self.template(&set.job_pre, &jv);
        self.progress_m73(0.0, true);
        let total_moves: usize = self
            .passes
            .iter()
            .map(|p| self.job.ops[p.op].moves.len())
            .sum();
        let mut done = 0usize;
        progress(0, total_moves)?;
        let mut cur_layer: Option<u8> = None;
        let passes = self.passes.clone();
        for (pi, p) in passes.iter().enumerate() {
            let op = &self.job.ops[p.op];
            if cur_layer != Some(op.layer) {
                if let Some(prev) = cur_layer {
                    self.layer_post(prev, pi);
                }
                if let Some((pre, _)) = self.ctx.layer_macros.get(&op.layer) {
                    if !pre.is_empty() {
                        let lv = self.layer_vars(op);
                        let pre = pre.clone();
                        self.template(&pre, &lv);
                    }
                }
                cur_layer = Some(op.layer);
            }
            let mut title = format!("{} - pass {}", op.label, op.pass + 1);
            if p.laser == 1 {
                title.push_str(" (laser 2)");
            }
            self.comment(&title);
            // pauza i opóźnienie raz na operację (nie przed przebiegiem lasera 2)
            let first_of_op = pi == 0 || passes[pi - 1].op != p.op;
            if let Some(msg) = op.pause.as_ref().filter(|_| first_of_op) {
                self.beam_off();
                self.comment(&format!("PAUSE: {msg}"));
                self.line("M0");
            }
            if op.delay_s > 0.0 && first_of_op {
                self.beam_off();
                self.line(format!("G4 P{}", fmt_num(op.delay_s, 3, true)));
            }
            if set.laser2.enabled && self.tool != Some(p.laser) {
                self.beam_off();
                // UNVERIFIED: wybór lasera słowem T (grblHAL, sterowniki dwulaserowe) — numeracja
                // narzędzi zależy od firmware; słowa są konfigurowalne (laser2.select1/2).
                let w = if p.laser == 0 {
                    set.laser2.select1.clone()
                } else {
                    set.laser2.select2.clone()
                };
                if !w.trim().is_empty() {
                    self.line(ascii(w.trim()));
                }
                self.tool = Some(p.laser);
            }
            self.set_air(op.settings.air_assist);
            if set.use_z {
                self.z_to(op.z);
            }
            let (off, k2) = if p.laser == 1 {
                let o = set.laser2.offset;
                let pm = op.settings.power_max;
                let p2 = op.settings.power2_max.unwrap_or(0.0);
                (
                    bc_geom::Vec2::new(o[0], o[1]),
                    if pm > 0.0 { p2 / pm } else { 0.0 },
                )
            } else {
                (bc_geom::Vec2::ZERO, 1.0)
            };
            let dynamic = op.settings.dynamic_power && self.d == Dialect::Grbl;
            for (mi, m) in op.moves.iter().enumerate() {
                match *m {
                    Move::Travel { to, lift } => {
                        if self.d == Dialect::Generic {
                            self.beam_off();
                        }
                        let lifted = lift && set.use_z;
                        if lifted {
                            self.z_to(op.z + set.z_lift);
                        }
                        self.motion(0, to - off, None, None);
                        if lifted {
                            self.z_to(op.z);
                        }
                    }
                    Move::Cut { to, power, speed } => {
                        self.ensure_cut_beam(dynamic);
                        let s = self.s_of(power * k2);
                        let f = self.feed_of(speed);
                        self.motion(1, to - off, Some(s), Some(f));
                    }
                    Move::Dwell { s, power } => {
                        let ps = self.s_of(power * k2);
                        self.dwell(s, ps);
                    }
                }
                let t = self.times[pi][mi];
                self.progress_m73(t, false);
                done += 1;
                if done.is_multiple_of(2048) {
                    progress(done, total_moves)?;
                }
            }
        }
        if let Some(prev) = cur_layer {
            self.layer_post(prev, passes.len());
        }
        self.template(&set.job_post, &jv);
        // bezpieczeństwo: wiązka i air zawsze wyłączone na końcu
        self.beam = Beam::M3;
        self.beam_off();
        self.set_air(false);
        if set.return_to_start && self.pos.is_some() {
            let target = if self.ctx.start.from == StartFrom::Absolute {
                self.job.start
            } else {
                self.anchor
            };
            self.motion(0, target, None, None);
        }
        self.progress_m73(self.total_s, true);
        self.template(&set.footer, &jv);
        if !self.unknown.is_empty() {
            let list: Vec<String> = self.unknown.iter().map(|k| format!("{{{k}}}")).collect();
            self.warnings.push(CompileWarning::new(
                "unknown_placeholder",
                format!("nieznane pola szablonu: {}", list.join(", ")),
            ));
        }
        progress(total_moves, total_moves)?;
        Ok(())
    }

    /// Makro post warstwy przed przebiegiem `next` (albo na końcu).
    fn layer_post(&mut self, layer: u8, next: usize) {
        let Some((_, post)) = self.ctx.layer_macros.get(&layer) else {
            return;
        };
        if post.is_empty() {
            return;
        }
        // zmienne z ostatniej operacji warstwy
        let last = self.passes[..next]
            .iter()
            .rev()
            .map(|p| &self.job.ops[p.op])
            .find(|o| o.layer == layer)
            .expect("operacja warstwy");
        let lv = self.layer_vars(last);
        let post = post.clone();
        self.template(&post, &lv);
    }
}
