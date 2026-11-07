//! Wspólne pomocnicze funkcje testów planera.
#![allow(dead_code)]

use bc_doc::{Document, Kind, LayerMode, LayerParams, ObjectId};
use bc_geom::Point;
use bc_plan::{Job, Move, Op, PlanOptions, Seg, SegKind};

pub fn rect(x: f64, y: f64, w: f64, h: f64) -> Kind {
    Kind::Rect {
        x,
        y,
        w,
        h,
        radius: 0.,
    }
}

pub fn path(d: &str) -> Kind {
    Kind::Path { d: d.into() }
}

/// Dokument z jednym obiektem na warstwie 0.
pub fn doc1(kind: Kind) -> (Document, ObjectId) {
    let mut d = Document::new("t");
    let id = d.add(0, kind).unwrap();
    (d, id)
}

pub fn set_mode(d: &mut Document, layer: u8, mode: LayerMode) {
    d.layer_mut(layer).unwrap().mode = mode;
}

pub fn params(d: &mut Document, layer: u8) -> &mut LayerParams {
    &mut d.layer_mut(layer).unwrap().params
}

pub fn plan(d: &Document) -> Job {
    bc_plan::plan(d, &PlanOptions::default()).unwrap()
}

pub fn plan_with(d: &Document, o: PlanOptions) -> Job {
    bc_plan::plan(d, &o).unwrap()
}

/// Odcinki zadania (z punktem początkowym).
pub fn segs(job: &Job) -> Vec<Seg> {
    job.segments()
}

pub fn powered(job: &Job) -> Vec<Seg> {
    job.segments()
        .into_iter()
        .filter(|s| s.kind == SegKind::Cut && s.power > 0.0)
        .collect()
}

pub fn op_segs(job: &Job, op: usize) -> Vec<Seg> {
    job.segments().into_iter().filter(|s| s.op == op).collect()
}

pub fn powered_len(job: &Job) -> f64 {
    powered(job).iter().map(|s| s.from.distance(s.to)).sum()
}

pub fn op_powered_len(op: &Op, start: Point) -> f64 {
    let mut at = start;
    let mut len = 0.0;
    for m in &op.moves {
        match *m {
            Move::Travel { to, .. } => at = to,
            Move::Cut { to, power, .. } => {
                if power > 0.0 {
                    len += at.distance(to);
                }
                at = to;
            }
            Move::Dwell { .. } => {}
        }
    }
    len
}

/// Liczba ciągłych łańcuchów cięcia z mocą (przerwy = przejazd albo odcinek bez mocy).
pub fn chains(job: &Job) -> usize {
    let mut n = 0;
    let mut in_chain = false;
    for s in job.segments() {
        let on = s.kind == SegKind::Cut && s.power > 0.0;
        if on && !in_chain {
            n += 1;
        }
        in_chain = on;
    }
    n
}

pub fn bbox_of(segs: &[Seg]) -> [f64; 4] {
    let mut b = [f64::MAX, f64::MAX, f64::MIN, f64::MIN];
    for s in segs {
        for p in [s.from, s.to] {
            b[0] = b[0].min(p.x);
            b[1] = b[1].min(p.y);
            b[2] = b[2].max(p.x);
            b[3] = b[3].max(p.y);
        }
    }
    b
}

pub fn close(a: f64, b: f64, eps: f64) -> bool {
    (a - b).abs() <= eps
}

/// Odległość punktu od brzegu prostokąta osiowego.
pub fn dist_to_rect_edge(p: Point, r: [f64; 4]) -> f64 {
    let [x0, y0, x1, y1] = r;
    let inside = p.x >= x0 && p.x <= x1 && p.y >= y0 && p.y <= y1;
    if inside {
        (p.x - x0).min(x1 - p.x).min(p.y - y0).min(y1 - p.y)
    } else {
        let dx = (x0 - p.x).max(0.0).max(p.x - x1);
        let dy = (y0 - p.y).max(0.0).max(p.y - y1);
        (dx * dx + dy * dy).sqrt()
    }
}

/// Wiersze skanowania z mocą: (y, x_od, x_do) w kolejności wykonania.
pub fn scan_rows(job: &Job) -> Vec<(f64, f64, f64)> {
    powered(job)
        .iter()
        .filter(|s| (s.from.y - s.to.y).abs() < 1e-9)
        .map(|s| (s.from.y, s.from.x, s.to.x))
        .collect()
}

/// Deterministyczny generator liczb (LCG) do testów własności.
pub struct Lcg(pub u64);

impl Lcg {
    pub fn next_f64(&mut self) -> f64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (self.0 >> 11) as f64 / (1u64 << 53) as f64
    }
    pub fn range(&mut self, a: f64, b: f64) -> f64 {
        a + (b - a) * self.next_f64()
    }
}
