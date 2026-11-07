//! Import G-code (REQ-02-089) i eksport w obie strony (REQ-02-090): interpreter,
//! zadanie do podglądu, raport, łuki, cale, G91/G92, ostrzeżenia.

mod common;

use bc_drivers::gcode::sim::{self, Event, SimOptions};
use bc_drivers::gcode::{import, ImportOptions};
use bc_geom::Point;
use bc_plan::{Move, SegKind};
use common::*;

const SAMPLE: &str = "\
; program z innego programu
%
G21 G90 (mm, absolutnie)
$H
M4 S0
G0 X10 Y10
G1 X30 Y10 S750 F1200 ; krawędź
Y20
G2 X40 Y30 I10 J0
M5
G0 X0 Y0
(Zmień kartkę)
M0
M8
M3 S200
G4 P0.5
T1
G1 X5 F600
M9
G38.2 Z-5
M2
";

#[test]
fn req_02_089_import_gcode_preview_and_report() {
    let imp = import(SAMPLE, &ImportOptions::default()).unwrap();
    let r = &imp.report;
    assert_eq!(r.lines, SAMPLE.lines().count());
    // odcinek 20 + 10 + ćwiartka okręgu r=10, potem X5 z laserem 2 (M3 S200)
    let quarter = std::f64::consts::FRAC_PI_2 * 10.0;
    assert!(
        close(r.cut_length, 30.0 + quarter + 5.0, 0.01),
        "{}",
        r.cut_length
    );
    assert_eq!(r.pauses, vec!["Zmień kartkę".to_string()]);
    assert!(r.dynamic_power && r.air_assist);
    assert_eq!(r.tools, vec![0, 1]);
    assert!(r.time_s > 0.0 && r.formatted_time.starts_with("00:00:"));
    assert!(
        r.warnings.iter().any(|w| w.contains("G38.2")),
        "{:?}",
        r.warnings
    );
    assert!(
        r.warnings.iter().any(|w| w.contains("$H")),
        "{:?}",
        r.warnings
    );
    let b = r.bbox.unwrap();
    assert!(close(b[0], 0.0, 1e-9) && close(b[2], 40.0, 1e-6) && close(b[3], 30.0, 1e-6));
    // zadanie do podglądu: moc w %, prędkość w mm/s, przejazdy, postój, pauza
    let j = &imp.job;
    assert_eq!(j.ops.len(), 3, "T0, po pauzie, T1");
    assert_eq!(j.ops[1].pause.as_deref(), Some("Zmień kartkę"));
    let segs = j.segments();
    let first_cut = segs.iter().find(|s| s.kind == SegKind::Cut).unwrap();
    assert_eq!(first_cut.power, 75.0);
    assert_eq!(first_cut.speed, 20.0);
    assert!(segs.iter().any(|s| s.kind == SegKind::Travel));
    assert!(j
        .ops
        .iter()
        .flat_map(|o| &o.moves)
        .any(|m| matches!(m, Move::Dwell { s, power } if *s == 0.5 && *power == 20.0)));
    // łuk G2 spłaszczony w tolerancji 0,01 mm
    let arc: Vec<_> = imp.run.moves().filter(|m| m.line == 9).collect();
    assert!(arc.len() > 8);
    for m in &arc {
        let c = Point::new(40.0, 20.0);
        assert!(close(m.to.distance(c), 10.0, 1e-9));
    }
    // podgląd renderuje się jak zadanie planera (bc-sim)
    let doc = bc_doc::Document::new("t");
    let png = bc_sim::render_toolpath(&doc, j, &bc_sim::PreviewOptions::default()).unwrap();
    assert!(png.png.starts_with(b"\x89PNG"));
}

#[test]
fn req_02_089_import_units_relative_and_offsets() {
    let prog = "G20 G91\nM3 S500\nG1 X1 F10\nY1\nG90 G21\nG92 X0 Y0\nG1 X10\nM5\n";
    let r = sim::run(prog, &SimOptions::default()).unwrap();
    let m: Vec<_> = r.moves().collect();
    assert!(pclose(m[0].to, Point::new(25.4, 0.0), 1e-9));
    assert!(pclose(m[1].to, Point::new(25.4, 25.4), 1e-9));
    assert!(close(m[0].feed, 254.0, 1e-9), "F w calach/min");
    // po G92 współrzędne programu liczone od bieżącej pozycji
    assert!(pclose(m[2].from, Point::ORIGIN, 1e-9));
    assert!(pclose(m[2].to, Point::new(10.0, 0.0), 1e-9));
    // błąd składni z numerem wiersza
    let e = sim::run("G1 X1\nG1 X1..2\n", &SimOptions::default()).unwrap_err();
    assert_eq!(e.line, 2);
    // uruchomienie programu na interpreterze: pauza i koniec programu w kolejności
    let r = sim::run(SAMPLE, &SimOptions::default()).unwrap();
    let kinds: Vec<&str> = r
        .events
        .iter()
        .filter_map(|e| match e {
            Event::Pause { .. } => Some("pause"),
            Event::End { .. } => Some("end"),
            _ => None,
        })
        .collect();
    assert_eq!(kinds, vec!["pause", "end"]);
}

#[test]
fn req_02_090_export_import_round_trip() {
    let mut d = rect_doc();
    d.add(
        1,
        bc_doc::Kind::Ellipse {
            cx: 80.0,
            cy: 30.0,
            rx: 15.0,
            ry: 8.0,
        },
    )
    .unwrap();
    d.layer_mut(1).unwrap().params.dynamic_power = false;
    let j = job(&d);
    for driver in ["grbl", "gcode"] {
        let f = gc(&d, driver);
        let imp = import(
            f.text(),
            &ImportOptions {
                sim: SimOptions {
                    laser_mode: driver == "grbl",
                    s_max: d.gcode.s_max,
                },
                kinematics: d.plan.kinematics,
            },
        )
        .unwrap();
        // te same odcinki z wiązką (położenie i moc) co w zadaniu planera
        let want: Vec<_> = j
            .segments()
            .into_iter()
            .filter(|s| s.kind == SegKind::Cut && s.power > 0.0)
            .collect();
        let got: Vec<_> = imp
            .job
            .segments()
            .into_iter()
            .filter(|s| s.kind == SegKind::Cut && s.power > 0.0)
            .collect();
        assert_eq!(want.len(), got.len(), "{driver}");
        for (a, b) in want.iter().zip(&got) {
            assert!(
                pclose(a.from, b.from, 1e-3) && pclose(a.to, b.to, 1e-3),
                "{driver}"
            );
            assert!(close(a.power, b.power, 0.01) && close(a.speed, b.speed, 0.01));
        }
        assert!(close(imp.report.cut_length, j.stats().cut_length, 1e-2));
        // eksport zaimportowanego zadania daje ten sam tor
        let again =
            bc_drivers::compile(driver, &imp.job, &bc_drivers::Context::from_document(&d)).unwrap();
        let r2 = run_with(again.text(), driver == "grbl");
        assert!(close(burn_len(&r2), j.stats().cut_length, 1e-2), "{driver}");
    }
}
