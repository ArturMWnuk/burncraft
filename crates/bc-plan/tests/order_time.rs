//! M2.1 (2/3): kolejność i optymalizacja (§8), przejazdy, strefy zakazane,
//! model czasu (§9), determinizm i przyrostowość (ADR 0009 §4–§7).

mod common;

use std::collections::BTreeMap;

use bc_doc::{CutDirection, Document, Kind, Kinematics, LayerMode, NoGoZone, Object, ObjectId};
use bc_geom::Point;
use bc_plan::{PlanOptions, Planner, SegKind};
use common::*;

/// Pierwszy punkt cięcia z mocą każdego obiektu (kolejność obiektów w zadaniu).
fn object_order(job: &bc_plan::Job, d: &Document, ids: &[ObjectId]) -> Vec<ObjectId> {
    let boxes: Vec<(ObjectId, bc_geom::Rect)> = ids
        .iter()
        .map(|id| (*id, d.bbox_of(*id).unwrap().unwrap().inflate(0.01, 0.01)))
        .collect();
    let mut out: Vec<ObjectId> = Vec::new();
    for s in powered(job) {
        // najmniejszy bbox zawierający odcinek
        let hit = boxes
            .iter()
            .filter(|(_, b)| b.contains(s.from) && b.contains(s.to))
            .min_by(|a, b| a.1.area().total_cmp(&b.1.area()));
        if let Some((id, _)) = hit {
            if out.last() != Some(id) {
                out.push(*id);
            }
        }
    }
    out
}

fn seg_crosses_rect(a: Point, b: Point, r: [f64; 4]) -> bool {
    // próbkowanie odcinka (testy: przeszkody większe niż krok)
    let n = 200;
    (1..n).any(|i| {
        let t = i as f64 / n as f64;
        let p = a + (b - a) * t;
        p.x > r[0] + 1e-6 && p.x < r[2] - 1e-6 && p.y > r[1] + 1e-6 && p.y < r[3] - 1e-6
    })
}

#[test]
fn req_08_001_order_by_layer() {
    let mut d = Document::new("t");
    let a = d.add(3, rect(0., 0., 5., 5.)).unwrap();
    let b = d.add(1, rect(50., 0., 5., 5.)).unwrap();
    let job = plan(&d);
    assert_eq!(
        job.ops.iter().map(|o| o.layer).collect::<Vec<_>>(),
        vec![1, 3]
    );
    assert_eq!(object_order(&job, &d, &[a, b]), vec![b, a]);
}

#[test]
fn req_07_023_layer_priority() {
    let mut d = Document::new("t");
    d.add(0, rect(0., 0., 5., 5.)).unwrap();
    d.add(5, rect(10., 0., 5., 5.)).unwrap();
    params(&mut d, 5).priority = -1;
    assert_eq!(
        plan(&d).ops.iter().map(|o| o.layer).collect::<Vec<_>>(),
        vec![5, 0]
    );
}

#[test]
fn req_08_002_order_by_object_priority() {
    let mut d = Document::new("t");
    let near = d.add(0, rect(0., 0., 5., 5.)).unwrap();
    let far = d.add(0, rect(100., 100., 5., 5.)).unwrap();
    assert_eq!(object_order(&plan(&d), &d, &[near, far]), vec![near, far]);
    d.find_mut(far).unwrap().priority = -1;
    assert_eq!(object_order(&plan(&d), &d, &[near, far]), vec![far, near]);
}

#[test]
fn req_08_003_order_by_group() {
    let mut d = Document::new("t");
    // A i C w grupie, B leży między nimi
    let a = Object::new(d.alloc_id(), 0, rect(0., 0., 5., 5.));
    let c = Object::new(d.alloc_id(), 0, rect(40., 0., 5., 5.));
    let (aid, cid) = (a.id, c.id);
    let gid = d.alloc_id();
    d.objects.push(Object::new(
        gid,
        0,
        Kind::Group {
            children: vec![a, c],
        },
    ));
    let b = d.add(0, rect(20., 0., 5., 5.)).unwrap();
    let order = object_order(&plan(&d), &d, &[aid, b, cid]);
    assert_eq!(order, vec![aid, cid, b], "grupa razem");
    d.plan.order_by_group = false;
    let order = object_order(&plan(&d), &d, &[aid, b, cid]);
    assert_eq!(order, vec![aid, b, cid], "bez grup: najbliższy sąsiad");
}

#[test]
fn req_08_004_inner_before_outer() {
    let mut d = Document::new("t");
    let outer = d.add(0, rect(0., 0., 40., 40.)).unwrap();
    let inner = d.add(0, rect(15., 15., 10., 10.)).unwrap();
    // głowica startuje przy konturze zewnętrznym
    let order = object_order(&plan(&d), &d, &[outer, inner]);
    assert_eq!(order, vec![inner, outer]);
    d.plan.inner_first = false;
    assert_eq!(
        object_order(&plan(&d), &d, &[outer, inner]),
        vec![outer, inner]
    );
}

#[test]
fn req_08_005_best_start_point() {
    let (mut d, _) = doc1(rect(0., 0., 10., 10.));
    d.plan.start = [12.0, 11.0];
    let p = powered(&plan(&d));
    assert!(
        p[0].from.distance(Point::new(10., 10.)) < 1e-9,
        "start w najbliższym wierzchołku: {:?}",
        p[0].from
    );
    d.plan.optimize_start = false;
    let p = powered(&plan(&d));
    assert!(p[0].from.distance(Point::new(0., 0.)) < 1e-9);
}

fn signed_area_of_cuts(job: &bc_plan::Job) -> f64 {
    powered(job)
        .iter()
        .map(|s| s.from.to_vec2().cross(s.to.to_vec2()))
        .sum::<f64>()
        / 2.0
}

#[test]
fn req_08_006_cut_direction() {
    let (mut d, _) = doc1(rect(0., 0., 10., 10.));
    d.plan.direction = CutDirection::Cw;
    assert!(close(signed_area_of_cuts(&plan(&d)), -100.0, 1e-6));
    d.plan.direction = CutDirection::Ccw;
    assert!(close(signed_area_of_cuts(&plan(&d)), 100.0, 1e-6));
    // otwarta ścieżka: od bliższego końca
    let (mut d, _) = doc1(path("M0 0 L50 0"));
    d.plan.start = [60.0, 0.0];
    let p = powered(&plan(&d));
    assert!(p[0].from.distance(Point::new(50., 0.)) < 1e-9);
}

fn grid_doc(n: usize) -> Document {
    // okręgi w siatce w „złej” kolejności dokumentu (wężykiem po przekątnych)
    let mut d = Document::new("t");
    let mut cells: Vec<(usize, usize)> = (0..n).flat_map(|i| (0..n).map(move |j| (i, j))).collect();
    cells.sort_by_key(|&(i, j)| ((i * 7 + j * 13) % (n * n), i, j));
    for (i, j) in cells {
        d.add(
            0,
            Kind::Ellipse {
                cx: 10.0 + i as f64 * 20.0,
                cy: 10.0 + j as f64 * 20.0,
                rx: 3.0,
                ry: 3.0,
            },
        )
        .unwrap();
    }
    d
}

#[test]
fn req_08_007_minimize_travel_nn_and_2opt() {
    let mut d = grid_doc(6);
    let opt = plan(&d).stats().travel_length;
    d.plan.two_opt = false;
    let nn = plan(&d).stats().travel_length;
    d.plan.optimize_start = false;
    let naive = bc_plan::plan(
        &d,
        &PlanOptions {
            ..Default::default()
        },
    )
    .unwrap();
    // kolejność dokumentu (bez optymalizacji): suma odległości kolejnych obiektów
    let mut doc_order = 0.0;
    let mut at = Point::ORIGIN;
    for o in &d.objects {
        let c = d.bbox_of(o.id).unwrap().unwrap().center();
        let p = Point::new(c.x + 3.0, c.y);
        doc_order += at.distance(p);
        at = p;
    }
    assert!(nn < doc_order * 0.5, "NN {nn} vs dokument {doc_order}");
    assert!(opt <= nn + 1e-9, "2-opt {opt} nie gorszy niż NN {nn}");
    let _ = naive;
}

#[test]
fn req_08_007_two_opt_improves_crossing_tour() {
    // punkty na osi po obu stronach startu: najbliższy sąsiad skacze zygzakiem,
    // 2-opt najpierw wycina jedną stronę, potem drugą
    let mut d = Document::new("t");
    d.plan.start = [100.0, 0.0];
    for x in [101.0, 97.9, 104.3, 91.4, 116.9] {
        d.add(0, path(&format!("M{x} 0 L{} 0", x + 0.2))).unwrap();
    }
    let opt = plan(&d).stats().travel_length;
    d.plan.two_opt = false;
    let nn = plan(&d).stats().travel_length;
    assert!(opt < nn - 1.0, "2-opt {opt} < NN {nn}");
}

#[test]
fn req_08_008_avoid_travel_over_cut_parts() {
    let mut d = Document::new("t");
    let big = d.add(0, rect(20., -20., 20., 40.)).unwrap();
    let a = d.add(0, rect(0., 0., 5., 5.)).unwrap();
    let c = d.add(0, rect(60., 0., 5., 5.)).unwrap();
    d.find_mut(big).unwrap().priority = -2;
    d.find_mut(a).unwrap().priority = -1;
    d.plan.optimize_start = false;
    let crosses = |job: &bc_plan::Job| {
        job.segments()
            .iter()
            .filter(|s| s.kind == SegKind::Travel)
            .skip(2) // po wycięciu dużego prostokąta
            .any(|s| seg_crosses_rect(s.from, s.to, [20., -20., 40., 20.]))
    };
    assert!(
        crosses(&plan(&d)),
        "bez opcji przejazd idzie prosto nad wyciętą częścią"
    );
    d.plan.avoid_cut_parts = true;
    let job = plan(&d);
    assert!(!crosses(&job), "objazd wyciętej części");
    assert_eq!(object_order(&job, &d, &[big, a, c]), vec![big, a, c]);
}

#[test]
fn req_08_009_no_go_zone_for_travel() {
    let mut d = Document::new("t");
    d.add(0, rect(0., 0., 5., 5.)).unwrap();
    d.add(0, rect(60., 0., 5., 5.)).unwrap();
    let zone = [20., -10., 40., 15.];
    d.plan.no_go_zones = vec![NoGoZone {
        name: "zacisk".into(),
        rect: zone,
        travel: true,
        beam: false,
    }];
    let job = plan(&d);
    for s in job.segments() {
        if s.kind == SegKind::Travel {
            assert!(
                !seg_crosses_rect(s.from, s.to, zone),
                "przejazd przez strefę: {s:?}"
            );
        }
    }
    assert!(job.warnings.iter().all(|w| w.code != "travel_lift"));
}

#[test]
fn req_08_010_no_go_zone_for_beam() {
    let (mut d, id) = doc1(rect(0., 0., 40., 10.));
    let zone = [15., -5., 25., 15.];
    d.plan.no_go_zones = vec![NoGoZone {
        name: "kamera".into(),
        rect: zone,
        travel: false,
        beam: true,
    }];
    let job = plan(&d);
    for s in powered(&job) {
        assert!(
            !seg_crosses_rect(s.from, s.to, zone),
            "wiązka w strefie: {s:?}"
        );
    }
    assert!(close(powered_len(&job), 100.0 - 20.0, 1e-6));
    let w = job
        .warnings
        .iter()
        .find(|w| w.code == "beam_no_go")
        .expect("ostrzeżenie");
    assert!(w.objects.contains(&id));
}

#[test]
fn req_08_009_010_property_no_cut_or_travel_in_zones() {
    let mut rng = Lcg(42);
    for case in 0..20 {
        let mut d = Document::new("t");
        for _ in 0..8 {
            let (x, y) = (rng.range(0., 150.), rng.range(0., 150.));
            d.add(0, rect(x, y, rng.range(2., 20.), rng.range(2., 20.)))
                .unwrap();
        }
        let zx = rng.range(30., 100.);
        let zy = rng.range(30., 100.);
        let zone = [zx, zy, zx + rng.range(5., 30.), zy + rng.range(5., 30.)];
        d.plan.no_go_zones = vec![NoGoZone {
            name: String::new(),
            rect: zone,
            travel: true,
            beam: true,
        }];
        let job = plan(&d);
        for s in job.segments() {
            match s.kind {
                SegKind::Cut if s.power > 0.0 => {
                    assert!(
                        !seg_crosses_rect(s.from, s.to, zone),
                        "przypadek {case}: cięcie w strefie {s:?}"
                    )
                }
                SegKind::Travel if !s.lift => {
                    assert!(
                        !seg_crosses_rect(s.from, s.to, zone),
                        "przypadek {case}: przejazd w strefie {s:?}"
                    )
                }
                _ => {}
            }
        }
    }
}

#[test]
fn req_08_011_remove_overlapping_lines() {
    let mut d = Document::new("t");
    d.add(0, rect(0., 0., 10., 10.)).unwrap();
    d.add(0, rect(10., 0., 10., 10.)).unwrap(); // wspólna krawędź x = 10
    assert!(close(powered_len(&plan(&d)), 80.0, 1e-6));
    d.plan.remove_overlaps = true;
    assert!(close(powered_len(&plan(&d)), 70.0, 1e-6));
}

#[test]
fn req_08_012_reduce_direction_changes() {
    let (mut d, _) = doc1(path("M0 0 L1 0 L2 0 L3 0 L10 0 L10 5"));
    let p = powered(&plan(&d));
    assert_eq!(p.len(), 2, "współliniowe odcinki scalone: {p:?}");
    d.plan.reduce_direction_changes = false;
    assert_eq!(powered(&plan(&d)).len(), 5);
}

#[test]
fn req_08_013_cut_selected_only() {
    let mut d = Document::new("t");
    let a = d.add(0, rect(0., 0., 10., 10.)).unwrap();
    d.add(0, rect(20., 0., 10., 10.)).unwrap();
    let job = plan_with(
        &d,
        PlanOptions {
            selection: Some(vec![a]),
            ..Default::default()
        },
    );
    assert!(close(powered_len(&job), 40.0, 1e-6));
    assert_eq!(job.ops[0].objects, vec![a]);
}

#[test]
fn req_08_014_selection_origin() {
    let mut d = Document::new("t");
    let a = d.add(0, rect(50., 60., 10., 10.)).unwrap();
    let job = plan_with(
        &d,
        PlanOptions {
            selection: Some(vec![a]),
            selection_origin: true,
            ..Default::default()
        },
    );
    let b = bbox_of(&powered(&job));
    assert!(
        close(b[0], 0., 1e-9) && close(b[1], 0., 1e-9) && close(b[2], 10., 1e-9),
        "{b:?}"
    );
    assert_eq!(job.origin, Point::new(50., 60.));
}

#[test]
fn req_07_051_cut_last_option() {
    let mut d = Document::new("t");
    d.add(0, rect(0., 0., 40., 40.)).unwrap(); // cięcie zewnętrzne, warstwa 0
    d.add(1, rect(5., 5., 10., 10.)).unwrap(); // grawer, warstwa 1
    d.add(2, rect(20., 20., 10., 10.)).unwrap(); // cięcie wewnętrzne, warstwa 2
    set_mode(&mut d, 1, LayerMode::Fill);
    let layers = |d: &Document| plan(d).ops.iter().map(|o| o.layer).collect::<Vec<_>>();
    assert_eq!(layers(&d), vec![0, 1, 2]);
    d.plan.cut_last = true;
    assert_eq!(
        layers(&d),
        vec![1, 2, 0],
        "grawer, cięcie wewnętrzne, cięcie zewnętrzne"
    );
}

#[test]
fn req_07_052_warning_line_before_fill() {
    let mut d = Document::new("t");
    d.add(0, rect(0., 0., 40., 40.)).unwrap();
    d.add(1, rect(5., 5., 10., 10.)).unwrap();
    set_mode(&mut d, 1, LayerMode::Fill);
    let job = plan(&d);
    let w = job
        .warnings
        .iter()
        .find(|w| w.code == "cut_before_engrave")
        .expect("ostrzeżenie");
    assert_eq!(w.layer, Some(0));
    d.plan.cut_last = true;
    assert!(plan(&d)
        .warnings
        .iter()
        .all(|w| w.code != "cut_before_engrave"));
}

#[test]
fn req_09_008_trapezoid_time_from_grbl_settings() {
    // GRBL: $110 = 6000 mm/min, $120 = 1000 mm/s², $11 = 0,01 mm
    let g: BTreeMap<u32, f64> = [
        (110, 6000.0),
        (111, 6000.0),
        (120, 1000.0),
        (121, 1000.0),
        (11, 0.01),
    ]
    .into();
    let k = Kinematics::from_grbl(&g);
    assert_eq!(
        (k.max_speed_x, k.accel_x, k.junction_deviation),
        (100.0, 1000.0, 0.01)
    );
    // odcinek 100 mm przy 100 mm/s: 2 × 0,1 s przyspieszania + 90 mm / 100 mm/s = 1,1 s
    assert!(close(
        bc_plan::segment_time(100.0, 0.0, 0.0, 100.0, 1000.0),
        1.1,
        1e-9
    ));
    // profil trójkątny: 2 mm przy 1000 mm/s² → 2·√(L/a) = 2·√(2/1000) s
    assert!(close(
        bc_plan::segment_time(2.0, 0.0, 0.0, 100.0, 1000.0),
        2.0 * (0.002f64).sqrt(),
        1e-9
    ));
    let (mut d, _) = doc1(path("M0 0 L100 0"));
    d.plan.kinematics = k;
    let e = bc_plan::estimate(&plan(&d), &k);
    assert!(close(e.cut_s, 1.1, 1e-9), "{e:?}");
    assert!(close(
        e.total_s,
        e.cut_s + e.travel_s + e.delay_s + e.dwell_s,
        1e-12
    ));
    // Ruida: jerk wydłuża fazy przyspieszania
    let r = Kinematics::from_ruida(100.0, 1000.0, 1000.0, 1.5);
    assert!(bc_plan::estimate(&plan(&d), &r).cut_s > 1.1);
}

#[test]
fn req_09_008_corners_slow_down() {
    // kwadrat: postój na narożnikach przy $11 = 0 wydłuża czas względem $11 = 0,1
    let (mut d, _) = doc1(rect(0., 0., 50., 50.));
    let job = plan(&d);
    let mut k = d.plan.kinematics;
    k.junction_deviation = 0.0;
    let slow = bc_plan::estimate(&job, &k).cut_s;
    k.junction_deviation = 0.1;
    let fast = bc_plan::estimate(&job, &k).cut_s;
    assert!(slow > fast, "{slow} > {fast}");
    d.plan.kinematics.junction_deviation = 0.0;
}

#[test]
fn req_08_018_separate_xy_kinematics() {
    let k = Kinematics {
        accel_y: 100.0,
        max_speed_y: 50.0,
        ..Default::default()
    };
    let (mut dx, _) = doc1(path("M0 0 L100 0"));
    dx.plan.kinematics = k;
    let (mut dy, _) = doc1(path("M0 0 L0 100"));
    dy.plan.kinematics = k;
    let tx = bc_plan::estimate(&plan(&dx), &k).cut_s;
    let ty = bc_plan::estimate(&plan(&dy), &k).cut_s;
    assert!(close(tx, 1.1, 1e-9));
    // Y: 50 mm/s, 100 mm/s²: 2 × 0,5 s rozpędzania (2 × 12,5 mm) + 75 mm / 50 mm/s = 2,5 s
    assert!(close(ty, 2.5, 1e-9), "{ty}");
    // overscan automatyczny w osi Y liczony z przyspieszenia Y
    let (mut d, _) = doc1(rect(0., 0., 10., 10.));
    set_mode(&mut d, 0, LayerMode::Fill);
    params(&mut d, 0).interval = 1.0;
    params(&mut d, 0).scan_angle = 90.0;
    d.layer_mut(0).unwrap().speed = 40.0;
    d.plan.kinematics = k;
    // v²/(2a) = 40² / 200 = 8 mm
    let b = bbox_of(
        &segs(&plan(&d))
            .into_iter()
            .filter(|s| s.kind == SegKind::Cut)
            .collect::<Vec<_>>(),
    );
    assert!(close(b[1], -8.0, 1e-6) && close(b[3], 18.0, 1e-6), "{b:?}");
}

fn complex_doc() -> Document {
    let mut d = grid_doc(5);
    let mut rng = Lcg(7);
    for _ in 0..30 {
        let (x, y) = (rng.range(0., 200.), rng.range(0., 200.));
        d.add(1, rect(x, y, rng.range(1., 15.), rng.range(1., 15.)))
            .unwrap();
        d.add(
            2,
            path(&format!(
                "M{x} {y} C {} {} {} {} {} {}",
                x + 5.,
                y + 9.,
                x + 12.,
                y - 4.,
                x + 20.,
                y + 2.
            )),
        )
        .unwrap();
    }
    set_mode(&mut d, 1, LayerMode::Fill);
    params(&mut d, 1).cross_hatch = true;
    params(&mut d, 0).tabs.enabled = true;
    params(&mut d, 0).kerf = 0.1;
    d.plan.no_go_zones = vec![NoGoZone {
        name: String::new(),
        rect: [90., 90., 110., 110.],
        travel: true,
        beam: true,
    }];
    d.plan.avoid_cut_parts = true;
    d
}

#[test]
fn req_16_007_deterministic_byte_stream() {
    let d = complex_doc();
    let base = plan_with(
        &d,
        PlanOptions {
            threads: 1,
            ..Default::default()
        },
    );
    let bytes = base.to_bytes();
    assert!(bytes.len() > 1000);
    for threads in [1, 2, 4, 8] {
        for _ in 0..2 {
            let j = plan_with(
                &d,
                PlanOptions {
                    threads,
                    ..Default::default()
                },
            );
            assert_eq!(j.to_bytes(), bytes, "wątki {threads}");
            assert_eq!(j.hash(), base.hash());
        }
    }
    // planer z pamięcią podręczną daje ten sam wynik
    let mut p = Planner::new();
    assert_eq!(
        p.plan(&d, &PlanOptions::default(), &mut |_, _| Ok(()))
            .unwrap()
            .hash(),
        base.hash()
    );
    assert_eq!(
        p.plan(&d, &PlanOptions::default(), &mut |_, _| Ok(()))
            .unwrap()
            .hash(),
        base.hash()
    );
}

/// „Typowy projekt”: 400 kształtów Line na 3 warstwach, 40 wypełnień, krzywe.
fn typical_project() -> Document {
    let mut d = Document::new("typowy");
    d.workspace.width = 600.0;
    d.workspace.height = 400.0;
    let mut rng = Lcg(2026);
    for i in 0..400 {
        let (x, y) = (rng.range(0., 580.), rng.range(0., 380.));
        let k = if i % 3 == 0 {
            Kind::Ellipse {
                cx: x + 5.,
                cy: y + 5.,
                rx: rng.range(1., 8.),
                ry: rng.range(1., 8.),
            }
        } else if i % 3 == 1 {
            rect(x, y, rng.range(1., 15.), rng.range(1., 15.))
        } else {
            path(&format!(
                "M{x} {y} C {} {} {} {} {} {} L {} {} Z",
                x + 4.,
                y + 9.,
                x + 12.,
                y - 3.,
                x + 15.,
                y + 5.,
                x + 3.,
                y + 12.
            ))
        };
        d.add((i % 3) as u8, k).unwrap();
    }
    for _ in 0..40 {
        let (x, y) = (rng.range(0., 560.), rng.range(0., 360.));
        d.add(3, rect(x, y, rng.range(5., 40.), rng.range(5., 40.)))
            .unwrap();
    }
    set_mode(&mut d, 3, LayerMode::Fill);
    params(&mut d, 0).kerf = 0.1;
    params(&mut d, 1).tabs.enabled = true;
    d
}

#[test]
fn req_08_019_fast_multithreaded_incremental_planner() {
    let mut d = typical_project();
    let mut p = Planner::new();
    let t = std::time::Instant::now();
    let first = p
        .plan(&d, &PlanOptions::default(), &mut |_, _| Ok(()))
        .unwrap();
    let full = t.elapsed();
    assert!(full.as_secs_f64() < 1.0, "pełne planowanie {full:?}");
    assert_eq!(p.last_recomputed(), 4, "4 warstwy policzone");
    // bez zmian: nic do przeliczenia
    let t = std::time::Instant::now();
    let again = p
        .plan(&d, &PlanOptions::default(), &mut |_, _| Ok(()))
        .unwrap();
    assert_eq!(p.last_recomputed(), 0);
    assert_eq!(again.hash(), first.hash());
    assert!(
        t.elapsed().as_secs_f64() < 1.0,
        "z pamięci podręcznej {:?}",
        t.elapsed()
    );
    // zmiana jednej warstwy przelicza tylko ją
    d.layer_mut(2).unwrap().speed = 50.0;
    let changed = p
        .plan(&d, &PlanOptions::default(), &mut |_, _| Ok(()))
        .unwrap();
    assert_eq!(p.last_recomputed(), 1);
    assert_eq!(
        changed.hash(),
        bc_plan::plan(&d, &PlanOptions::default()).unwrap().hash()
    );
    // anulowanie przerywa planowanie
    let r = p.plan(
        &typical_project(),
        &PlanOptions {
            threads: 2,
            ..Default::default()
        },
        &mut |done, _| {
            if done >= 1 {
                Err(bc_plan::PlanError::Cancelled)
            } else {
                Ok(())
            }
        },
    );
    assert_eq!(r.unwrap_err(), bc_plan::PlanError::Cancelled);
}
