//! Model zadania (ADR 0009 §2): operacje, ruchy, ostrzeżenia, statystyki,
//! kanoniczna serializacja i hash.

use bc_doc::{LayerMode, ObjectId};
use bc_geom::Point;
use schemars::JsonSchema;
use serde::Serialize;
use sha2::{Digest, Sha256};

/// Rodzaj operacji: wektorowa (kontury) albo rastrowa (linie skanowania).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum OpKind {
    Vector,
    Raster,
}

/// Ustawienia źródła i ruchu wspólne dla całej operacji.
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
pub struct OpSettings {
    /// mm/s
    pub speed: f64,
    /// %
    pub power_max: f64,
    /// %
    pub power_min: f64,
    /// Moc lasera 2 (skaluje się proporcjonalnie do mocy lasera 1).
    pub power2_max: Option<f64>,
    pub power2_min: Option<f64>,
    /// M4 (moc dynamiczna) zamiast M3.
    pub dynamic_power: bool,
    pub pwm_hz: Option<u32>,
    pub air_assist: bool,
}

/// Ruch głowicy. Punkt początkowy = koniec poprzedniego ruchu.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Move {
    /// Przejazd z wyłączoną wiązką; `lift` = podniesienie Z (brak objazdu).
    Travel { to: Point, lift: bool },
    /// Ruch roboczy z mocą `power` % (0 = wiązka wyłączona, np. overscan, perforacja).
    Cut { to: Point, power: f64, speed: f64 },
    /// Postój z wiązką (tryb punktowy) albo bez (`power` = 0).
    Dwell { s: f64, power: f64 },
}

/// Jeden przebieg jednej warstwy (pod-warstwy) z jednym zestawem ustawień.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Op {
    pub layer: u8,
    /// Nazwa warstwy (z nazwą pod-warstwy).
    pub label: String,
    /// 0 = warstwa, 1.. = pod-warstwy.
    pub sublayer: u32,
    /// Numer przejścia od 0.
    pub pass: u32,
    pub kind: OpKind,
    pub mode: LayerMode,
    /// Flaga Show warstwy (tylko wyświetlanie).
    pub show: bool,
    pub settings: OpSettings,
    /// Pauza z komunikatem przed operacją.
    pub pause: Option<String>,
    /// Opóźnienie przed operacją, s.
    pub delay_s: f64,
    /// Z, mm.
    pub z: f64,
    /// Obiekty, z których powstała operacja.
    pub objects: Vec<ObjectId>,
    pub moves: Vec<Move>,
}

/// Ostrzeżenie planera.
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
pub struct Warning {
    /// Kod: open_shape_in_fill, outside_workspace, duplicate, cut_before_engrave,
    /// power_above_limit, beam_no_go, travel_lift, image_mode, empty_fill.
    pub code: String,
    pub message: String,
    pub layer: Option<u8>,
    pub objects: Vec<ObjectId>,
}

impl Warning {
    pub fn new(
        code: &str,
        message: impl Into<String>,
        layer: Option<u8>,
        objects: Vec<ObjectId>,
    ) -> Self {
        Warning {
            code: code.into(),
            message: message.into(),
            layer,
            objects,
        }
    }
}

/// Zadanie: wynik planera.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Job {
    pub ops: Vec<Op>,
    pub warnings: Vec<Warning>,
    /// Pozycja głowicy przed zadaniem.
    pub start: Point,
    /// Przesunięcie dokumentu → zadanie (origin zaznaczenia, REQ-08-014).
    pub origin: Point,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SegKind {
    Travel,
    Cut,
    Dwell,
}

/// Ruch z punktem początkowym (do testów, podglądu i estymacji).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Seg {
    pub op: usize,
    pub from: Point,
    pub to: Point,
    pub kind: SegKind,
    pub power: f64,
    pub speed: f64,
    pub lift: bool,
    pub dwell_s: f64,
}

/// Statystyki zadania.
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
pub struct Stats {
    /// Długość cięcia z wiązką, mm.
    pub cut_length: f64,
    /// Długość przejazdów, mm.
    pub travel_length: f64,
    /// Ruch roboczy bez wiązki (overscan, przerwy perforacji), mm.
    pub idle_length: f64,
    /// Liczba operacji (każde przejście osobno).
    pub ops: usize,
    /// Liczba przejść łącznie (suma po warstwach).
    pub passes: usize,
    /// Liczba warstw z wyjściem.
    pub layers: usize,
    /// Obszar cięcia [x0, y0, x1, y1] (mm) albo brak.
    pub bbox: Option<[f64; 4]>,
}

impl Job {
    pub fn empty(start: Point) -> Self {
        Job {
            ops: Vec::new(),
            warnings: Vec::new(),
            start,
            origin: Point::ORIGIN,
        }
    }

    /// Wszystkie ruchy z punktami początkowymi.
    pub fn segments(&self) -> Vec<Seg> {
        let mut out = Vec::new();
        let mut at = self.start;
        for (i, op) in self.ops.iter().enumerate() {
            for m in &op.moves {
                let s = match *m {
                    Move::Travel { to, lift } => Seg {
                        op: i,
                        from: at,
                        to,
                        kind: SegKind::Travel,
                        power: 0.0,
                        speed: 0.0,
                        lift,
                        dwell_s: 0.0,
                    },
                    Move::Cut { to, power, speed } => Seg {
                        op: i,
                        from: at,
                        to,
                        kind: SegKind::Cut,
                        power,
                        speed,
                        lift: false,
                        dwell_s: 0.0,
                    },
                    Move::Dwell { s, power } => Seg {
                        op: i,
                        from: at,
                        to: at,
                        kind: SegKind::Dwell,
                        power,
                        speed: 0.0,
                        lift: false,
                        dwell_s: s,
                    },
                };
                at = s.to;
                out.push(s);
            }
        }
        out
    }

    /// Zadanie od segmentu `n` ([`Job::segments`]) do końca — „rozpocznij od tego
    /// miejsca” (REQ-09-013): przejazd do początku segmentu, potem pozostałe ruchy.
    /// Operacja, od której środka startujemy, nie powtarza pauzy ani opóźnienia.
    /// `None`, gdy `n` jest poza zakresem.
    pub fn from_segment(&self, n: usize) -> Option<Job> {
        let segs = self.segments();
        let seg = segs.get(n)?;
        let (oi, mut skip) = (seg.op, n);
        for s in &segs[..n] {
            if s.op < oi {
                skip -= 1;
            }
        }
        let mut ops: Vec<Op> = self.ops[oi..].to_vec();
        let first = &mut ops[0];
        if skip > 0 {
            first.pause = None;
            first.delay_s = 0.0;
        }
        let mut moves = Vec::with_capacity(first.moves.len() - skip + 1);
        moves.push(Move::Travel {
            to: seg.from,
            lift: false,
        });
        moves.extend_from_slice(&first.moves[skip..]);
        first.moves = moves;
        Some(Job {
            ops,
            warnings: self.warnings.clone(),
            start: self.start,
            origin: self.origin,
        })
    }

    /// Pozycja głowicy po zadaniu.
    pub fn end(&self) -> Point {
        self.segments().last().map_or(self.start, |s| s.to)
    }

    pub fn stats(&self) -> Stats {
        let mut st = Stats {
            cut_length: 0.0,
            travel_length: 0.0,
            idle_length: 0.0,
            ops: self.ops.len(),
            passes: self.ops.len(),
            layers: 0,
            bbox: None,
        };
        let mut layers: Vec<u8> = self.ops.iter().map(|o| o.layer).collect();
        layers.sort_unstable();
        layers.dedup();
        st.layers = layers.len();
        let mut b: Option<[f64; 4]> = None;
        let mut grow = |p: Point| {
            let r = b.get_or_insert([p.x, p.y, p.x, p.y]);
            r[0] = r[0].min(p.x);
            r[1] = r[1].min(p.y);
            r[2] = r[2].max(p.x);
            r[3] = r[3].max(p.y);
        };
        for s in self.segments() {
            let len = s.from.distance(s.to);
            match s.kind {
                SegKind::Travel => st.travel_length += len,
                SegKind::Cut if s.power > 0.0 => {
                    st.cut_length += len;
                    grow(s.from);
                    grow(s.to);
                }
                SegKind::Cut => st.idle_length += len,
                SegKind::Dwell => {
                    if s.power > 0.0 {
                        grow(s.from);
                    }
                }
            }
        }
        st.bbox = b;
        st
    }

    /// Kanoniczna postać bajtowa (ADR 0009 §5): współrzędne do 1 µm, moc do 0,001 %.
    pub fn to_bytes(&self) -> Vec<u8> {
        use std::fmt::Write;
        let mut s = String::new();
        let _ = writeln!(s, "BCJOB 1");
        let _ = writeln!(s, "start {} {}", num(self.start.x), num(self.start.y));
        let _ = writeln!(s, "origin {} {}", num(self.origin.x), num(self.origin.y));
        for op in &self.ops {
            let st = &op.settings;
            let _ = writeln!(
                s,
                "op {} {} {} {:?} {:?} show={} speed={} p={}..{} p2={}..{} m4={} pwm={} air={} z={} delay={} pause={:?} label={:?}",
                op.layer,
                op.sublayer,
                op.pass,
                op.kind,
                op.mode,
                op.show,
                num(st.speed),
                num(st.power_max),
                num(st.power_min),
                st.power2_max.map(num).unwrap_or_default(),
                st.power2_min.map(num).unwrap_or_default(),
                st.dynamic_power,
                st.pwm_hz.unwrap_or(0),
                st.air_assist,
                num(op.z),
                num(op.delay_s),
                op.pause,
                op.label
            );
            for m in &op.moves {
                let _ = match *m {
                    Move::Travel { to, lift } => {
                        writeln!(
                            s,
                            "T {} {}{}",
                            num(to.x),
                            num(to.y),
                            if lift { " lift" } else { "" }
                        )
                    }
                    Move::Cut { to, power, speed } => writeln!(
                        s,
                        "C {} {} {} {}",
                        num(to.x),
                        num(to.y),
                        num(power),
                        num(speed)
                    ),
                    Move::Dwell { s: t, power } => writeln!(s, "D {} {}", num(t), num(power)),
                };
            }
        }
        for w in &self.warnings {
            let _ = writeln!(
                s,
                "W {} {:?} {:?} {:?}",
                w.code, w.layer, w.objects, w.message
            );
        }
        s.into_bytes()
    }

    /// SHA-256 kanonicznej postaci (hex).
    pub fn hash(&self) -> String {
        let d = Sha256::digest(self.to_bytes());
        d.iter().map(|b| format!("{b:02x}")).collect()
    }
}

/// Liczba z dokładnością 0,001 (bez „-0.000”).
fn num(v: f64) -> String {
    let r = (v * 1000.0).round() / 1000.0;
    let r = if r == 0.0 { 0.0 } else { r };
    format!("{r:.3}")
}
