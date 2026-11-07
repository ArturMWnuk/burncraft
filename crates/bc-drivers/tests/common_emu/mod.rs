//! Pomocnicze funkcje testów sterownika na emulatorze GRBL.
#![allow(dead_code)]

use std::sync::{Arc, Mutex};
use std::time::Duration;

use bc_doc::{Document, Kind};
use bc_drivers::driver::{Driver, JobOptions};
use bc_drivers::emu::{EmuConfig, GrblEmulator};
use bc_drivers::grbl::{GrblDriver, GrblOptions};
use bc_drivers::transport::{EmuTransport, SharedEmulator};
use bc_drivers::{compile, Context, MachineFile};
use bc_plan::PlanOptions;

pub fn emu(cfg: EmuConfig) -> SharedEmulator {
    Arc::new(Mutex::new(GrblEmulator::new(cfg)))
}

pub fn connect(e: &SharedEmulator) -> GrblDriver {
    connect_with(e, GrblOptions::default())
}

pub fn connect_with(e: &SharedEmulator, o: GrblOptions) -> GrblDriver {
    GrblDriver::connect(
        "test",
        Box::new(EmuTransport::new(e.clone())),
        GrblOptions { poll_ms: 20, ..o },
    )
    .expect("połączenie z emulatorem")
}

pub fn rect_doc() -> Document {
    let mut d = Document::new("t");
    d.add(
        0,
        Kind::Rect {
            x: 10.,
            y: 10.,
            w: 30.,
            h: 20.,
            radius: 0.,
        },
    )
    .unwrap();
    let l = d.layer_mut(0).unwrap();
    l.speed = 20.0;
    l.power_max = 50.0;
    l.air_assist = false;
    d
}

pub fn file(d: &Document) -> MachineFile {
    let j = bc_plan::plan(d, &PlanOptions::default()).unwrap();
    compile("grbl", &j, &Context::from_document(d)).unwrap()
}

pub fn run_job(dr: &mut GrblDriver, f: &MachineFile) -> bc_drivers::driver::JobState {
    dr.send_job(f, JobOptions::default()).unwrap();
    dr.wait_job(Duration::from_secs(30))
}

pub fn burn_len(e: &SharedEmulator) -> f64 {
    e.lock()
        .unwrap()
        .burns()
        .iter()
        .map(|b| b.from_pt().distance(b.to_pt()))
        .sum()
}
