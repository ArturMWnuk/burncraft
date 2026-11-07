//! M2.1 (1/3): model zadania, tryb Line i parametry warstwy §7.2 (ADR 0009).

mod common;

use bc_doc::{DotMode, LayerMode, Lead, LeadKind, Perforation, Ramp, SubLayer, Tab, TabParams};
use bc_geom::Point;
use bc_plan::{OpKind, SegKind};
use common::*;

#[test]
fn req_07_001_line_mode_traces_contour() {
    let (d, id) = doc1(rect(10., 10., 20., 10.));
    let job = plan(&d);
    assert_eq!(job.ops.len(), 1);
    let op = &job.ops[0];
    assert_eq!(op.kind, OpKind::Vector);
    assert_eq!(op.mode, LayerMode::Line);
    assert!(
        close(powered_len(&job), 60.0, 1e-6),
        "{}",
        powered_len(&job)
    );
    let b = bbox_of(&powered(&job));
    assert!(close(b[0], 10., 1e-6) && close(b[3], 20., 1e-6), "{b:?}");
    assert!(op.objects.contains(&id));
    // stała moc = moc maksymalna warstwy (domyślnie 20 %), prędkość warstwy
    assert!(powered(&job)
        .iter()
        .all(|s| close(s.power, 20.0, 1e-9) && close(s.speed, 100.0, 1e-9)));
}

#[test]
fn req_07_010_layer_name_in_op() {
    let (mut d, _) = doc1(rect(0., 0., 5., 5.));
    d.layer_mut(0).unwrap().name = "Cięcie sklejki".into();
    assert_eq!(plan(&d).ops[0].label, "Cięcie sklejki");
}

#[test]
fn req_07_011_output_off_excludes_layer() {
    let (mut d, _) = doc1(rect(0., 0., 5., 5.));
    d.add(1, rect(10., 0., 5., 5.)).unwrap();
    d.layer_mut(0).unwrap().output = false;
    let job = plan(&d);
    assert_eq!(job.ops.len(), 1);
    assert_eq!(job.ops[0].layer, 1);
}

#[test]
fn req_07_012_show_flag_kept_in_job() {
    let (mut d, _) = doc1(rect(0., 0., 5., 5.));
    d.layer_mut(0).unwrap().show = false;
    let job = plan(&d);
    // Show dotyczy wyświetlania; warstwa nadal jest wypalana
    assert_eq!(job.ops.len(), 1);
    assert!(!job.ops[0].show);
}

#[test]
fn req_07_013_air_assist() {
    let (mut d, _) = doc1(rect(0., 0., 5., 5.));
    assert!(plan(&d).ops[0].settings.air_assist);
    d.layer_mut(0).unwrap().air_assist = false;
    assert!(!plan(&d).ops[0].settings.air_assist);
}

#[test]
fn req_07_014_speed() {
    let (mut d, _) = doc1(rect(0., 0., 5., 5.));
    d.layer_mut(0).unwrap().speed = 12.5;
    let job = plan(&d);
    assert_eq!(job.ops[0].settings.speed, 12.5);
    assert!(powered(&job).iter().all(|s| s.speed == 12.5));
}

#[test]
fn req_07_015_016_power_max_min() {
    let (mut d, _) = doc1(rect(0., 0., 5., 5.));
    {
        let l = d.layer_mut(0).unwrap();
        l.power_max = 65.0;
        l.power_min = 15.0;
    }
    let job = plan(&d);
    let s = &job.ops[0].settings;
    assert_eq!((s.power_max, s.power_min), (65.0, 15.0));
    assert!(powered(&job).iter().all(|s| s.power == 65.0));
}

#[test]
fn req_07_017_second_laser_powers() {
    let (mut d, _) = doc1(rect(0., 0., 5., 5.));
    params(&mut d, 0).power2_max = Some(40.0);
    params(&mut d, 0).power2_min = Some(5.0);
    let s = plan(&d).ops[0].settings.clone();
    assert_eq!((s.power2_max, s.power2_min), (Some(40.0), Some(5.0)));
    params(&mut d, 0).power2_min = Some(50.0);
    assert!(d.validate().is_err(), "power2_min > power2_max");
}

#[test]
fn req_07_018_passes() {
    let (mut d, _) = doc1(rect(0., 0., 5., 5.));
    d.layer_mut(0).unwrap().passes = 3;
    let job = plan(&d);
    assert_eq!(job.ops.len(), 3);
    assert_eq!(
        job.ops.iter().map(|o| o.pass).collect::<Vec<_>>(),
        vec![0, 1, 2]
    );
    assert!(close(powered_len(&job), 60.0, 1e-6));
    assert_eq!(job.stats().passes, 3);
}

#[test]
fn req_07_019_delay_between_passes() {
    let (mut d, _) = doc1(rect(0., 0., 5., 5.));
    d.layer_mut(0).unwrap().passes = 2;
    params(&mut d, 0).pass_delay = 2.5;
    let job = plan(&d);
    assert_eq!(job.ops[0].delay_s, 0.0);
    assert_eq!(job.ops[1].delay_s, 2.5);
    let e = bc_plan::estimate(&job, &d.plan.kinematics);
    assert!(close(e.delay_s, 2.5, 1e-9));
}

#[test]
fn req_07_020_pause_with_message_before_layer() {
    let (mut d, _) = doc1(rect(0., 0., 5., 5.));
    d.layer_mut(0).unwrap().passes = 2;
    params(&mut d, 0).pause_message = Some("Zmień materiał".into());
    let job = plan(&d);
    assert_eq!(job.ops[0].pause.as_deref(), Some("Zmień materiał"));
    assert_eq!(
        job.ops[1].pause, None,
        "pauza tylko przed warstwą, nie przed każdym przejściem"
    );
}

#[test]
fn req_07_021_022_z_offset_and_step() {
    let (mut d, _) = doc1(rect(0., 0., 5., 5.));
    d.layer_mut(0).unwrap().passes = 3;
    params(&mut d, 0).z_offset = 1.0;
    params(&mut d, 0).z_step = 0.5;
    let z: Vec<f64> = plan(&d).ops.iter().map(|o| o.z).collect();
    assert_eq!(z, vec![1.0, 0.5, 0.0]);
}

#[test]
fn req_07_024_pwm_frequency() {
    let (mut d, _) = doc1(rect(0., 0., 5., 5.));
    assert_eq!(plan(&d).ops[0].settings.pwm_hz, None);
    params(&mut d, 0).pwm_hz = Some(5000);
    assert_eq!(plan(&d).ops[0].settings.pwm_hz, Some(5000));
}

#[test]
fn req_07_025_constant_vs_dynamic_power() {
    let (mut d, _) = doc1(rect(0., 0., 5., 5.));
    assert!(plan(&d).ops[0].settings.dynamic_power, "domyślnie M4");
    params(&mut d, 0).dynamic_power = false;
    assert!(!plan(&d).ops[0].settings.dynamic_power);
}

#[test]
fn req_07_026_kerf_outer_out_holes_in() {
    // kontur zewnętrzny 20×20 z otworem 10×10
    let (mut d, _) = doc1(path("M0 0 L20 0 L20 20 L0 20 Z M5 5 L5 15 L15 15 L15 5 Z"));
    params(&mut d, 0).kerf = 0.1;
    let job = plan(&d);
    let p = powered(&job);
    let b = bbox_of(&p);
    assert!(close(b[0], -0.1, 1e-3) && close(b[2], 20.1, 1e-3), "{b:?}");
    // otwór zmniejszony o kerf: żaden punkt cięcia nie leży w pasie 5..5.1 od wewnątrz
    let inner: Vec<_> = p
        .iter()
        .filter(|s| s.from.x > 2.0 && s.from.x < 18.0 && s.from.y > 2.0 && s.from.y < 18.0)
        .collect();
    let ib = bbox_of(&inner.into_iter().cloned().collect::<Vec<_>>());
    assert!(
        close(ib[0], 5.1, 1e-3) && close(ib[2], 14.9, 1e-3),
        "{ib:?}"
    );
}

#[test]
fn req_07_027_separate_kerf_x_y() {
    let (mut d, _) = doc1(rect(0., 0., 10., 10.));
    params(&mut d, 0).kerf_x = Some(0.2);
    params(&mut d, 0).kerf_y = Some(0.05);
    let b = bbox_of(&powered(&plan(&d)));
    assert!(close(b[2] - b[0], 10.4, 1e-3), "{b:?}");
    assert!(close(b[3] - b[1], 10.1, 1e-3), "{b:?}");
}

#[test]
fn req_07_028_fill_kerf_shrinks_scan_area() {
    let (mut d, _) = doc1(rect(0., 0., 10., 10.));
    set_mode(&mut d, 0, LayerMode::Fill);
    params(&mut d, 0).interval = 1.0;
    params(&mut d, 0).fill_kerf = 0.5;
    let b = bbox_of(&powered(&plan(&d)));
    assert!(close(b[0], 0.5, 1e-3) && close(b[2], 9.5, 1e-3), "{b:?}");
    assert!(b[1] >= 0.5 - 1e-9 && b[3] <= 9.5 + 1e-9, "{b:?}");
}

fn tabs(d: &mut bc_doc::Document, t: TabParams) {
    params(d, 0).tabs = t;
}

#[test]
fn req_07_031_tab_size() {
    let (mut d, _) = doc1(rect(0., 0., 10., 10.));
    tabs(
        &mut d,
        TabParams {
            enabled: true,
            size: 1.0,
            count: 4,
            ..Default::default()
        },
    );
    assert!(close(powered_len(&plan(&d)), 36.0, 1e-6));
}

#[test]
fn req_07_032_tab_count() {
    let (mut d, _) = doc1(rect(0., 0., 10., 10.));
    tabs(
        &mut d,
        TabParams {
            enabled: true,
            size: 0.5,
            count: 3,
            ..Default::default()
        },
    );
    let job = plan(&d);
    assert_eq!(chains(&job), 3);
    assert!(close(powered_len(&job), 38.5, 1e-6));
}

#[test]
fn req_07_033_tab_spacing() {
    let (mut d, _) = doc1(rect(0., 0., 10., 10.));
    tabs(
        &mut d,
        TabParams {
            enabled: true,
            size: 0.5,
            count: 0,
            spacing: 10.0,
            ..Default::default()
        },
    );
    let job = plan(&d);
    assert_eq!(chains(&job), 4);
    assert!(close(powered_len(&job), 38.0, 1e-6));
}

#[test]
fn req_07_034_tabs_skip_inner_shapes() {
    let mut d = bc_doc::Document::new("t");
    d.add(0, rect(0., 0., 40., 40.)).unwrap();
    d.add(0, rect(15., 15., 10., 10.)).unwrap();
    let t = TabParams {
        enabled: true,
        size: 1.0,
        count: 2,
        ..Default::default()
    };
    tabs(
        &mut d,
        TabParams {
            skip_inner: true,
            ..t
        },
    );
    assert!(close(powered_len(&plan(&d)), 160.0 - 2.0 + 40.0, 1e-6));
    tabs(
        &mut d,
        TabParams {
            skip_inner: false,
            ..t
        },
    );
    assert!(close(
        powered_len(&plan(&d)),
        160.0 - 2.0 + 40.0 - 2.0,
        1e-6
    ));
}

#[test]
fn req_04_083_manual_tabs() {
    let (mut d, id) = doc1(rect(0., 0., 10., 10.));
    d.find_mut(id).unwrap().tabs = vec![Tab {
        at: [5.0, 0.0],
        size: Some(2.0),
    }];
    let job = plan(&d);
    assert!(close(powered_len(&job), 38.0, 1e-6));
    // przerwa na dolnej krawędzi w x ∈ (4, 6)
    for s in powered(&job) {
        if s.from.y.abs() < 1e-9 && s.to.y.abs() < 1e-9 {
            let (a, b) = (s.from.x.min(s.to.x), s.from.x.max(s.to.x));
            assert!(
                b <= 4.0 + 1e-6 || a >= 6.0 - 1e-6,
                "cięcie przez mostek: {a}..{b}"
            );
        }
    }
}

#[test]
fn req_04_084_automatic_tabs() {
    let (mut d, _) = doc1(rect(0., 0., 20., 20.));
    assert_eq!(chains(&plan(&d)), 1, "bez mostków jeden łańcuch");
    tabs(
        &mut d,
        TabParams {
            enabled: true,
            size: 1.0,
            count: 0,
            spacing: 20.0,
            ..Default::default()
        },
    );
    let job = plan(&d);
    assert_eq!(chains(&job), 4);
    assert!(close(powered_len(&job), 76.0, 1e-6));
}

#[test]
fn req_04_085_tabs_wizard() {
    let (mut d, id) = doc1(rect(0., 0., 10., 10.));
    let w = bc_plan::TabsWizard {
        count: 4,
        spacing: 0.0,
        size: Some(1.0),
        skip_inner: true,
    };
    let placed = bc_plan::tabs_wizard(&d, &[id], &w).unwrap();
    assert_eq!(placed.len(), 1);
    let (oid, t) = &placed[0];
    assert_eq!(*oid, id);
    assert_eq!(t.len(), 4);
    for tab in t {
        let p = Point::new(tab.at[0], tab.at[1]);
        assert!(
            dist_to_rect_edge(p, [0., 0., 10., 10.]) < 1e-6,
            "{p:?} nie leży na konturze"
        );
        assert_eq!(tab.size, Some(1.0));
    }
    d.find_mut(id).unwrap().tabs = t.clone();
    assert!(close(powered_len(&plan(&d)), 36.0, 1e-6));
}

#[test]
fn req_07_035_lead_in_out_line() {
    let (mut d, _) = doc1(rect(0., 0., 10., 10.));
    params(&mut d, 0).lead_in = Lead {
        kind: LeadKind::Line,
        length: 2.0,
        angle: 90.0,
    };
    params(&mut d, 0).lead_out = Lead {
        kind: LeadKind::Line,
        length: 1.0,
        angle: 90.0,
    };
    let job = plan(&d);
    let p = powered(&job);
    let first = p.first().unwrap();
    // wejście z zewnątrz (strona odpadu konturu zewnętrznego), prostopadle
    assert!(close(
        dist_to_rect_edge(first.from, [0., 0., 10., 10.]),
        2.0,
        1e-6
    ));
    assert!(dist_to_rect_edge(first.to, [0., 0., 10., 10.]) < 1e-6);
    let last = p.last().unwrap();
    assert!(close(
        dist_to_rect_edge(last.to, [0., 0., 10., 10.]),
        1.0,
        1e-6
    ));
    assert!(close(powered_len(&job), 43.0, 1e-6));
}

#[test]
fn req_07_036_lead_arc() {
    let (mut d, _) = doc1(rect(0., 0., 10., 10.));
    params(&mut d, 0).lead_in = Lead {
        kind: LeadKind::Arc,
        length: 2.0,
        angle: 90.0,
    };
    let job = plan(&d);
    let p = powered(&job);
    // łuk 90° o promieniu 2 = 2π/4 · 2 ≈ 3,1416 mm, cięciwa 2·2·sin 45° ≈ 2,828 mm
    assert!(
        close(powered_len(&job), 40.0 + std::f64::consts::PI, 0.01),
        "{}",
        powered_len(&job)
    );
    let arc_pts = p
        .iter()
        .take_while(|s| dist_to_rect_edge(s.to, [0., 0., 10., 10.]) > 1e-6)
        .count();
    assert!(arc_pts >= 2, "łuk spłaszczony do kilku odcinków");
    let on_contour = p[arc_pts].to;
    assert!(close(
        p[0].from.distance(on_contour),
        2.0 * 2.0 * 45f64.to_radians().sin(),
        1e-3
    ));
    assert!(
        dist_to_rect_edge(p[0].from, [0., 0., 10., 10.]) > 1.0,
        "łuk zaczyna się od strony odpadu"
    );
}

#[test]
fn req_07_037_lead_length_and_angle() {
    let (mut d, _) = doc1(rect(0., 0., 10., 10.));
    params(&mut d, 0).lead_in = Lead {
        kind: LeadKind::Line,
        length: 3.0,
        angle: 45.0,
    };
    let job = plan(&d);
    let p = powered(&job);
    let lead = &p[0];
    let next = &p[1];
    assert!(close(lead.from.distance(lead.to), 3.0, 1e-6));
    let u = (lead.to - lead.from).normalize();
    let t = (next.to - next.from).normalize();
    let ang = u.dot(t).clamp(-1.0, 1.0).acos().to_degrees();
    assert!(close(ang, 45.0, 1e-6), "{ang}");
}

#[test]
fn req_07_038_overcut_closed_shapes() {
    let (mut d, _) = doc1(rect(0., 0., 10., 10.));
    params(&mut d, 0).overcut = 2.0;
    assert!(close(powered_len(&plan(&d)), 42.0, 1e-6));
    // otwarte kształty bez overcutu
    let (mut d2, _) = doc1(path("M0 0 L10 0"));
    params(&mut d2, 0).overcut = 2.0;
    assert!(close(powered_len(&plan(&d2)), 10.0, 1e-6));
}

#[test]
fn req_07_039_perforation() {
    let (mut d, _) = doc1(rect(0., 0., 10., 10.));
    params(&mut d, 0).perforation = Some(Perforation { cut: 1.0, gap: 1.0 });
    let job = plan(&d);
    assert!(close(powered_len(&job), 20.0, 1e-6));
    let idle: f64 = segs(&job)
        .iter()
        .filter(|s| s.kind == SegKind::Cut && s.power == 0.0)
        .map(|s| s.from.distance(s.to))
        .sum();
    assert!(close(idle, 20.0, 1e-6));
    assert_eq!(chains(&job), 20);
}

#[test]
fn req_07_040_dot_mode() {
    let (mut d, _) = doc1(rect(0., 0., 10., 10.));
    params(&mut d, 0).dot_mode = Some(DotMode {
        dwell_ms: 5.0,
        spacing: 1.0,
    });
    let job = plan(&d);
    assert_eq!(powered_len(&job), 0.0);
    let dots: Vec<_> = segs(&job)
        .into_iter()
        .filter(|s| s.kind == SegKind::Dwell)
        .collect();
    assert_eq!(dots.len(), 40);
    assert!(dots
        .iter()
        .all(|s| close(s.dwell_s, 0.005, 1e-12) && s.power == 20.0));
}

#[test]
fn req_07_041_power_ramp_and_outer_only() {
    let mut d = bc_doc::Document::new("t");
    d.add(0, rect(0., 0., 40., 40.)).unwrap();
    d.add(0, rect(15., 15., 10., 10.)).unwrap();
    {
        let l = d.layer_mut(0).unwrap();
        l.power_max = 80.0;
        l.power_min = 10.0;
    }
    params(&mut d, 0).ramp = Some(Ramp {
        length: 2.0,
        outer_only: false,
    });
    let job = plan(&d);
    let p = powered(&job);
    assert!((10.0..80.0).contains(&p[0].power), "{}", p[0].power);
    assert!(p.iter().any(|s| s.power == 80.0));
    // rampa rośnie
    let ramp: Vec<f64> = p
        .iter()
        .take_while(|s| s.power < 80.0)
        .map(|s| s.power)
        .collect();
    assert!(ramp.windows(2).all(|w| w[1] >= w[0]), "{ramp:?}");
    assert!(close(powered_len(&job), 200.0, 1e-6));
    // tylko na zewnątrz: kontur wewnętrzny bez rampy
    params(&mut d, 0).ramp = Some(Ramp {
        length: 2.0,
        outer_only: true,
    });
    let job = plan(&d);
    let inner_low = powered(&job)
        .iter()
        .filter(|s| s.from.x > 14.0 && s.from.x < 26.0 && s.from.y > 14.0 && s.from.y < 26.0)
        .any(|s| s.power < 80.0);
    assert!(!inner_low);
    assert!(
        powered(&job).iter().any(|s| s.power < 80.0),
        "zewnętrzny z rampą"
    );
}

#[test]
fn req_07_042_corner_power_ramp() {
    let (mut d, _) = doc1(rect(0., 0., 100., 2.));
    {
        let l = d.layer_mut(0).unwrap();
        l.power_max = 60.0;
        l.power_min = 10.0;
        l.speed = 100.0;
    }
    params(&mut d, 0).corner_ramp = true;
    let job = plan(&d);
    let p = powered(&job);
    let min = p.iter().map(|s| s.power).fold(f64::MAX, f64::min);
    let max = p.iter().map(|s| s.power).fold(f64::MIN, f64::max);
    assert!(
        (10.0..60.0).contains(&min),
        "zwolnienia na narożnikach: {min}"
    );
    assert!(
        close(max, 60.0, 1e-9),
        "jazda ze stałą prędkością = pełna moc: {max}"
    );
    // długość bez zmian
    assert!(close(powered_len(&job), 204.0, 1e-6));
}

#[test]
fn req_07_050_sublayers() {
    let (mut d, _) = doc1(rect(0., 0., 10., 10.));
    params(&mut d, 0).sublayers = vec![SubLayer {
        name: "grawer".into(),
        output: true,
        mode: LayerMode::Fill,
        speed: Some(200.0),
        power_max: Some(30.0),
        power_min: None,
        passes: Some(1),
        interval: Some(1.0),
        scan_angle: None,
    }];
    let job = plan(&d);
    assert_eq!(job.ops.len(), 2);
    assert_eq!((job.ops[0].sublayer, job.ops[0].mode), (0, LayerMode::Line));
    let s = &job.ops[1];
    assert_eq!(
        (s.sublayer, s.mode, s.kind),
        (1, LayerMode::Fill, OpKind::Raster)
    );
    assert_eq!((s.settings.speed, s.settings.power_max), (200.0, 30.0));
    assert_eq!(s.layer, 0);
}
