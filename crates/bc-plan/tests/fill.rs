//! M2.1 (1/3): tryby wypełnienia i parametry skanowania (§7.1, §7.2, §8).

mod common;

use bc_doc::{Document, FillGrouping, Kind, LayerMode, Object, Overscan, ScanOffset};
use bc_plan::{OpKind, SegKind};
use common::*;

fn fill_doc(kinds: &[Kind], interval: f64) -> Document {
    let mut d = Document::new("t");
    for k in kinds {
        d.add(0, k.clone()).unwrap();
    }
    set_mode(&mut d, 0, LayerMode::Fill);
    params(&mut d, 0).interval = interval;
    params(&mut d, 0).overscan = Overscan::Off;
    d
}

/// Kolejne wiersze (y) w kolejności wykonania, bez powtórzeń sąsiednich.
fn row_sequence(rows: &[(f64, f64, f64)]) -> Vec<f64> {
    let mut v: Vec<f64> = Vec::new();
    for r in rows {
        if v.last().is_none_or(|y| (y - r.0).abs() > 1e-9) {
            v.push(r.0);
        }
    }
    v
}

#[test]
fn req_07_002_fill_scanlines() {
    let d = fill_doc(&[rect(0., 0., 10., 10.)], 1.0);
    let job = plan(&d);
    assert_eq!(job.ops.len(), 1);
    assert_eq!(job.ops[0].kind, OpKind::Raster);
    let rows = scan_rows(&job);
    assert_eq!(rows.len(), 10);
    for (i, (y, a, b)) in rows.iter().enumerate() {
        assert!(close(*y, 0.5 + i as f64, 1e-9), "{y}");
        assert!(close((b - a).abs(), 10.0, 1e-6));
    }
    assert!(close(powered_len(&job), 100.0, 1e-6));
}

#[test]
fn req_07_003_fill_all_shapes_together() {
    let d = fill_doc(&[rect(0., 0., 10., 10.), rect(20., 0., 10., 10.)], 1.0);
    let rows = scan_rows(&plan(&d));
    assert_eq!(rows.len(), 20);
    // każdy wiersz przechodzi przez oba kształty, zanim zacznie się następny
    assert_eq!(row_sequence(&rows).len(), 10);
}

#[test]
fn req_07_004_fill_groups_together() {
    let mut d = fill_doc(&[], 1.0);
    params(&mut d, 0).fill_grouping = FillGrouping::Groups;
    for x in [0.0, 20.0] {
        let a = Object::new(d.alloc_id(), 0, rect(x, 0., 4., 10.));
        let b = Object::new(d.alloc_id(), 0, rect(x + 5., 0., 4., 10.));
        let g = Object::new(
            d.alloc_id(),
            0,
            Kind::Group {
                children: vec![a, b],
            },
        );
        d.objects.push(g);
    }
    let rows = scan_rows(&plan(&d));
    // grupa 1 (x < 15) w całości przed grupą 2, a w grupie oba kształty w jednym wierszu
    let split = rows.iter().position(|r| r.1.min(r.2) > 15.0).unwrap();
    assert!(rows[split..].iter().all(|r| r.1.min(r.2) > 15.0));
    assert_eq!(row_sequence(&rows[..split]).len(), 10);
    assert_eq!(rows[..split].len(), 20);
}

#[test]
fn req_07_005_fill_each_shape_separately() {
    let mut d = fill_doc(&[rect(0., 0., 10., 10.), rect(20., 0., 10., 10.)], 1.0);
    params(&mut d, 0).fill_grouping = FillGrouping::Each;
    let rows = scan_rows(&plan(&d));
    assert_eq!(rows.len(), 20);
    // kształty skanowane po kolei: jedna zmiana kształtu w całej sekwencji
    let shape = |r: &(f64, f64, f64)| r.1.min(r.2) > 15.0;
    let switches = rows
        .windows(2)
        .filter(|w| shape(&w[0]) != shape(&w[1]))
        .count();
    assert_eq!(switches, 1);
}

#[test]
fn req_07_006_offset_fill_concentric() {
    let mut d = fill_doc(&[rect(0., 0., 10., 10.)], 1.0);
    set_mode(&mut d, 0, LayerMode::OffsetFill);
    let job = plan(&d);
    assert_eq!(job.ops[0].kind, OpKind::Vector);
    // pierścienie co 1 mm od 0,5 mm do środka: 9×9, 7×7, 5×5, 3×3, 1×1
    assert!(
        close(powered_len(&job), 4.0 * (9. + 7. + 5. + 3. + 1.), 1e-3),
        "{}",
        powered_len(&job)
    );
    assert_eq!(chains(&job), 5);
}

#[test]
fn req_07_008_fill_plus_line() {
    let mut d = fill_doc(&[rect(0., 0., 10., 10.)], 1.0);
    set_mode(&mut d, 0, LayerMode::FillLine);
    let job = plan(&d);
    assert_eq!(job.ops.len(), 2);
    assert_eq!(job.ops[0].kind, OpKind::Raster, "najpierw wypełnienie");
    assert_eq!(job.ops[1].kind, OpKind::Vector);
    let start = job.segments().iter().find(|s| s.op == 1).unwrap().from;
    assert!(close(op_powered_len(&job.ops[1], start), 40.0, 1e-6));
}

#[test]
fn req_07_009_flood_fill_by_regions() {
    // jeden obiekt z dwoma rozłącznymi obszarami
    let shape = path("M0 0 L10 0 L10 10 L0 10 Z M50 0 L60 0 L60 10 L50 10 Z");
    let mut d = fill_doc(&[shape], 1.0);
    let together = scan_rows(&plan(&d));
    assert_eq!(
        row_sequence(&together).len(),
        10,
        "Fill: wiersz przez oba obszary"
    );
    set_mode(&mut d, 0, LayerMode::FloodFill);
    let rows = scan_rows(&plan(&d));
    assert_eq!(rows.len(), 20);
    let split = rows.iter().position(|r| r.1.min(r.2) > 30.0).unwrap();
    assert_eq!(split, 10, "Flood Fill: najpierw cały pierwszy obszar");
    assert!(rows[split..].iter().all(|r| r.1.min(r.2) > 30.0));
}

#[test]
fn req_07_043_bidirectional_scanning() {
    let mut d = fill_doc(&[rect(0., 0., 10., 10.)], 1.0);
    let rows = scan_rows(&plan(&d));
    for w in rows.windows(2) {
        assert!(
            (w[0].2 - w[0].1).signum() != (w[1].2 - w[1].1).signum(),
            "kierunki naprzemienne"
        );
    }
    params(&mut d, 0).bidirectional = false;
    let rows = scan_rows(&plan(&d));
    assert!(
        rows.iter().all(|r| r.2 > r.1),
        "jednokierunkowo: zawsze w prawo"
    );
}

#[test]
fn req_07_044_scan_angle() {
    let mut d = fill_doc(&[rect(0., 0., 10., 10.)], 1.0);
    params(&mut d, 0).scan_angle = 90.0;
    let p = powered(&plan(&d));
    assert_eq!(p.len(), 10);
    assert!(p
        .iter()
        .all(|s| close(s.from.x, s.to.x, 1e-9) && close((s.to.y - s.from.y).abs(), 10.0, 1e-6)));
    params(&mut d, 0).scan_angle = 45.0;
    let p = powered(&plan(&d));
    assert!(p
        .iter()
        .all(|s| close((s.to.y - s.from.y).abs(), (s.to.x - s.from.x).abs(), 1e-6)));
    assert!(close(
        p.iter().map(|s| s.from.distance(s.to)).sum::<f64>(),
        100.0,
        2.0
    ));
}

#[test]
fn req_07_045_cross_hatch() {
    let mut d = fill_doc(&[rect(0., 0., 10., 10.)], 1.0);
    params(&mut d, 0).cross_hatch = true;
    let p = powered(&plan(&d));
    let horiz = p.iter().filter(|s| close(s.from.y, s.to.y, 1e-9)).count();
    let vert = p.iter().filter(|s| close(s.from.x, s.to.x, 1e-9)).count();
    assert_eq!((horiz, vert), (10, 10));
}

#[test]
fn req_07_046_interval_lpi_dpi() {
    let d = fill_doc(&[rect(0., 0., 10., 10.)], 0.5);
    assert_eq!(scan_rows(&plan(&d)).len(), 20);
    assert!(close(bc_doc::params::interval_from_lpi(254.0), 0.1, 1e-12));
    assert!(close(bc_doc::params::lpi_from_interval(0.1), 254.0, 1e-9));
}

fn idle_extent(job: &bc_plan::Job) -> [f64; 4] {
    bbox_of(
        &job.segments()
            .into_iter()
            .filter(|s| s.kind == SegKind::Cut)
            .collect::<Vec<_>>(),
    )
}

#[test]
fn req_07_047_overscan_auto() {
    let mut d = fill_doc(&[rect(0., 0., 10., 10.)], 1.0);
    params(&mut d, 0).overscan = Overscan::Auto;
    d.layer_mut(0).unwrap().speed = 100.0;
    d.plan.kinematics.accel_x = 1000.0;
    // v²/(2a) = 100² / 2000 = 5 mm po obu stronach
    let b = idle_extent(&plan(&d));
    assert!(close(b[0], -5.0, 1e-6) && close(b[2], 15.0, 1e-6), "{b:?}");
    assert!(close(bbox_of(&powered(&plan(&d)))[0], 0.0, 1e-6));
}

#[test]
fn req_07_048_overscan_manual_percent() {
    let mut d = fill_doc(&[rect(0., 0., 10., 10.)], 1.0);
    params(&mut d, 0).overscan = Overscan::Percent { percent: 2.5 };
    d.layer_mut(0).unwrap().speed = 100.0;
    let b = idle_extent(&plan(&d));
    assert!(close(b[0], -2.5, 1e-6) && close(b[2], 12.5, 1e-6), "{b:?}");
}

#[test]
fn req_07_049_scan_offset_by_speed() {
    let mut d = fill_doc(&[rect(0., 0., 10., 10.)], 1.0);
    d.layer_mut(0).unwrap().speed = 150.0;
    params(&mut d, 0).scan_offsets = vec![
        ScanOffset {
            speed: 100.0,
            offset: 0.1,
        },
        ScanOffset {
            speed: 200.0,
            offset: 0.3,
        },
    ];
    assert!(close(
        d.layer(0).unwrap().params.scan_offset_at(150.0),
        0.2,
        1e-12
    ));
    let rows = scan_rows(&plan(&d));
    let fwd = rows.iter().find(|r| r.2 > r.1).unwrap();
    let rev = rows.iter().find(|r| r.2 < r.1).unwrap();
    assert!(close(fwd.1, 0.0, 1e-6) && close(fwd.2, 10.0, 1e-6));
    // linie powrotne przesunięte o 0,2 mm (interpolacja 100 → 0,1 i 200 → 0,3)
    assert!(
        close(rev.1, 10.2, 1e-6) && close(rev.2, 0.2, 1e-6),
        "{rev:?}"
    );
}

#[test]
fn req_08_015_raster_bands() {
    let mut d = fill_doc(&[rect(0., 0., 10., 10.), rect(100., 0., 10., 10.)], 1.0);
    let rows = scan_rows(&plan(&d));
    assert_eq!(
        row_sequence(&rows).len(),
        10,
        "bez pasów wiersz przez oba kształty"
    );
    d.plan.raster_band = 50.0;
    let rows = scan_rows(&plan(&d));
    let split = rows.iter().position(|r| r.1.min(r.2) > 50.0).unwrap();
    assert_eq!(split, 10, "pas 0–50 mm w całości przed pasem 100–150 mm");
    assert!(rows[split..].iter().all(|r| r.1.min(r.2) > 50.0));
}
