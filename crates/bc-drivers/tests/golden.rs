//! Testy golden kompilatorów (REQ-19-006, ADR 0011 §9): projekt referencyjny →
//! G-code porównany bajt w bajt z zatwierdzonymi plikami w `tests/golden/`.
//! Świadoma aktualizacja: `BC_UPDATE_GOLDEN=1 cargo test -p bc-drivers --test golden`.

mod common;

use std::path::PathBuf;

use bc_doc::{Document, Kind, LayerMode};
use bc_drivers::{compile, Context};
use bc_geom::Point;
use bc_plan::raster::{bitmap_op, Bitmap, RasterParams};
use bc_plan::{Job, PlanOptions};
use common::rect;

fn dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/golden")
}

fn update() -> bool {
    std::env::var_os("BC_UPDATE_GOLDEN").is_some()
}

/// Projekt referencyjny: cięcie (M4, 2 przejścia, air, mostki), grawer Fill (M3),
/// warstwa z pauzą, makrami i drugim laserem, okrąg.
fn reference_document() -> Document {
    let mut d = Document::new("Projekt referencyjny M2.2");
    d.workspace.width = 200.0;
    d.workspace.height = 150.0;
    // warstwa 0: grawer Fill
    d.add(0, rect(10.0, 90.0, 40.0, 20.0)).unwrap();
    let l = d.layer_mut(0).unwrap();
    l.mode = LayerMode::Fill;
    l.speed = 120.0;
    l.power_max = 30.0;
    l.power_min = 5.0;
    l.air_assist = false;
    l.params.dynamic_power = false;
    l.params.interval = 1.0;
    // warstwa 1: cięcie z mostkami i dwoma przejściami
    d.add(1, rect(10.0, 10.0, 60.0, 40.0)).unwrap();
    d.add(
        1,
        Kind::Ellipse {
            cx: 40.0,
            cy: 30.0,
            rx: 10.0,
            ry: 10.0,
        },
    )
    .unwrap();
    let l = d.layer_mut(1).unwrap();
    l.mode = LayerMode::Line;
    l.speed = 8.0;
    l.power_max = 90.0;
    l.power_min = 20.0;
    l.passes = 2;
    l.air_assist = true;
    l.params.pass_delay = 0.5;
    l.params.z_step = 0.5;
    l.params.tabs.enabled = true;
    l.params.tabs.count = 2;
    l.params.tabs.size = 1.0;
    // warstwa 2: znakowanie z pauzą, makrami i drugim laserem
    d.add(
        2,
        Kind::Path {
            d: "M 100 20 L 140 20 L 120 50 Z".into(),
        },
    )
    .unwrap();
    let l = d.layer_mut(2).unwrap();
    l.mode = LayerMode::Line;
    l.speed = 25.0;
    l.power_max = 40.0;
    l.power_min = 10.0;
    l.air_assist = false;
    l.params.pause_message = Some("Zmień materiał".into());
    l.params.power2_max = Some(20.0);
    l.params.gcode_pre = "; warstwa {label}: S{power} F{speed}".into();
    l.params.gcode_post = "; koniec warstwy {layer}".into();
    d.plan.cut_last = true;
    d.gcode.progress_m73 = true;
    d.gcode.laser2.enabled = true;
    d.gcode.laser2.offset = [12.5, 0.0];
    d.gcode.job_pre = "; start zadania {name}".into();
    d.gcode.job_post = "; koniec zadania".into();
    d
}

/// Syntetyczna bitmapa (obrazy w dokumencie dojdą w M4).
fn raster_job() -> Job {
    let (w, h) = (12u32, 6u32);
    let data = (0..h)
        .flat_map(|r| {
            (0..w).map(move |c| {
                if (c + r) % 5 == 0 {
                    255
                } else {
                    ((c * 21 + r * 13) % 256) as u8
                }
            })
        })
        .collect();
    let b = Bitmap {
        width: w,
        height: h,
        data,
        origin: Point::new(20.0, 20.0),
        pixel: 0.25,
    };
    let p = RasterParams {
        speed: 80.0,
        power_min: 5.0,
        power_max: 60.0,
        bidirectional: true,
        overscan: 2.0,
    };
    Job {
        ops: vec![bitmap_op(0, "Obraz testowy", &b, &p, true)],
        ..Job::empty(Point::ORIGIN)
    }
}

fn check(name: &str, bytes: &[u8]) {
    let path = dir().join(name);
    if update() {
        std::fs::create_dir_all(dir()).unwrap();
        std::fs::write(&path, bytes).unwrap();
        return;
    }
    let want = std::fs::read(&path).unwrap_or_else(|e| {
        panic!(
            "{}: {e} — utwórz plik golden: BC_UPDATE_GOLDEN=1 cargo test -p bc-drivers --test golden",
            path.display()
        )
    });
    // .gitattributes: pliki golden bez konwersji końców linii
    if want != bytes {
        let (a, b) = (
            String::from_utf8_lossy(&want),
            String::from_utf8_lossy(bytes),
        );
        let line = a
            .lines()
            .zip(b.lines())
            .position(|(x, y)| x != y)
            .unwrap_or(a.lines().count().min(b.lines().count()));
        panic!(
            "{name}: G-code różni się od pliku golden od wiersza {}:\n  golden: {:?}\n  wynik:  {:?}\n(świadoma zmiana: BC_UPDATE_GOLDEN=1)",
            line + 1,
            a.lines().nth(line),
            b.lines().nth(line)
        );
    }
}

fn load_reference() -> Document {
    let path = dir().join("reference.bcraft");
    let built = reference_document();
    if update() {
        std::fs::create_dir_all(dir()).unwrap();
        bc_doc::bcraft::save(&built, &Default::default(), &path).unwrap();
    }
    let (doc, _) = bc_doc::bcraft::load(&path).unwrap();
    assert_eq!(
        doc.layers, built.layers,
        "projekt referencyjny zgodny z definicją"
    );
    assert_eq!(doc.objects, built.objects);
    assert_eq!(doc.gcode, built.gcode);
    doc
}

#[test]
fn req_19_006_golden_reference_project() {
    let doc = load_reference();
    let job = bc_plan::plan(&doc, &PlanOptions::default()).unwrap();
    let ctx = Context::from_document(&doc);
    let grbl = compile("grbl", &job, &ctx).unwrap();
    check("reference.grbl.gcode", &grbl.bytes);
    let generic = compile("gcode", &job, &ctx).unwrap();
    check("reference.generic.nc", &generic.bytes);
    // zawartość: pauza, M73, dwa lasery, air, makra, M3 i M4
    let t = grbl.text();
    for w in [
        "M0",
        "M73 P0",
        "M73 P100 R0",
        "T0",
        "T1",
        "M8",
        "M9",
        "M3 S0",
        "M4 S0",
    ] {
        assert!(t.lines().any(|l| l.starts_with(w)), "{w} w pliku golden");
    }
    assert!(t.contains("; warstwa C02: S400 F1500"));
    assert!(t.contains("; PAUSE: Zmien material"));
    assert_eq!(
        t.lines().filter(|l| *l == "M0").count(),
        1,
        "pauza raz, nie przed laserem 2"
    );
}

#[test]
fn req_19_006_golden_raster_bitmap() {
    let job = raster_job();
    let f = compile("grbl", &job, &Context::default()).unwrap();
    check("raster.grbl.gcode", &f.bytes);
}

#[test]
fn req_19_006_016_007_golden_deterministic() {
    let doc = reference_document();
    let ctx = Context::from_document(&doc);
    let mut hashes = Vec::new();
    for threads in [1, 2, 4, 8] {
        let job = bc_plan::plan(
            &doc,
            &PlanOptions {
                threads,
                ..Default::default()
            },
        )
        .unwrap();
        for _ in 0..2 {
            hashes.push(compile("grbl", &job, &ctx).unwrap().hash);
        }
    }
    assert!(hashes.windows(2).all(|w| w[0] == w[1]), "{hashes:?}");
}
