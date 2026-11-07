//! Wspólne pomocnicze funkcje testów kompilatora G-code.
#![allow(dead_code)]

use bc_doc::{Document, Kind};
use bc_drivers::gcode::sim::{self, SimMove, SimOptions};
use bc_drivers::{compile, Context, MachineFile};
use bc_geom::Point;
use bc_plan::{Job, PlanOptions};

pub fn rect(x: f64, y: f64, w: f64, h: f64) -> Kind {
    Kind::Rect {
        x,
        y,
        w,
        h,
        radius: 0.,
    }
}

/// Prostokąt 10,10–40,30 na warstwie 0: 10 mm/s, 50 %, M4.
pub fn rect_doc() -> Document {
    let mut d = Document::new("Próba");
    d.add(0, rect(10., 10., 30., 20.)).unwrap();
    let l = d.layer_mut(0).unwrap();
    l.speed = 10.0;
    l.power_max = 50.0;
    l.power_min = 0.0;
    d
}

pub fn job(d: &Document) -> Job {
    bc_plan::plan(d, &PlanOptions::default()).unwrap()
}

pub fn gc(d: &Document, driver: &str) -> MachineFile {
    compile(driver, &job(d), &Context::from_document(d)).unwrap()
}

pub fn gc_ctx(d: &Document, driver: &str, ctx: &Context) -> MachineFile {
    compile(driver, &job(d), ctx).unwrap()
}

pub fn run(text: &str) -> sim::Run {
    sim::run(text, &SimOptions::default()).unwrap()
}

pub fn run_with(text: &str, laser_mode: bool) -> sim::Run {
    sim::run(
        text,
        &SimOptions {
            laser_mode,
            s_max: 1000.0,
        },
    )
    .unwrap()
}

/// Ruchy z wiązką.
pub fn burns(r: &sim::Run) -> Vec<SimMove> {
    r.moves().filter(|m| m.on).copied().collect()
}

pub fn burn_len(r: &sim::Run) -> f64 {
    burns(r).iter().map(|m| m.from.distance(m.to)).sum()
}

pub fn close(a: f64, b: f64, eps: f64) -> bool {
    (a - b).abs() <= eps
}

pub fn pclose(a: Point, b: Point, eps: f64) -> bool {
    a.distance(b) <= eps
}

/// Indeks pierwszego wiersza zaczynającego się od `prefix`.
pub fn line_idx(text: &str, prefix: &str) -> Option<usize> {
    text.lines().position(|l| l.starts_with(prefix))
}

pub fn lines_starting(text: &str, prefix: &str) -> usize {
    text.lines().filter(|l| l.starts_with(prefix)).count()
}
