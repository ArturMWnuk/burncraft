//! M2.1 (3/3): ostrzeżenia planera dla plan_job (REQ-15-052…056).

mod common;

use bc_doc::{Document, LayerMode};
use bc_plan::PlanOptions;
use common::*;

fn codes(job: &bc_plan::Job) -> Vec<&str> {
    job.warnings.iter().map(|w| w.code.as_str()).collect()
}

#[test]
fn req_15_052_warning_open_shapes_in_fill() {
    let mut d = Document::new("t");
    let open = d.add(0, path("M0 0 L10 0 L10 10")).unwrap();
    d.add(0, rect(20., 0., 10., 10.)).unwrap();
    set_mode(&mut d, 0, LayerMode::Fill);
    params(&mut d, 0).interval = 1.0;
    let job = plan(&d);
    let w = job
        .warnings
        .iter()
        .find(|w| w.code == "open_shape_in_fill")
        .expect("ostrzeżenie");
    assert_eq!((w.layer, w.objects.clone()), (Some(0), vec![open]));
    // zamknięty kształt nadal wypełniony
    assert!(close(powered_len(&job), 100.0, 1e-6));
    // w trybie Line otwarte kształty są w porządku
    set_mode(&mut d, 0, LayerMode::Line);
    assert!(!codes(&plan(&d)).contains(&"open_shape_in_fill"));
}

#[test]
fn req_15_053_warning_objects_outside_work_area() {
    let mut d = Document::new("t");
    d.workspace.width = 100.0;
    d.workspace.height = 100.0;
    let out = d.add(0, rect(95., 10., 10., 10.)).unwrap();
    d.add(0, rect(10., 10., 10., 10.)).unwrap();
    let job = plan(&d);
    let w = job
        .warnings
        .iter()
        .find(|w| w.code == "outside_workspace")
        .expect("ostrzeżenie");
    assert_eq!(w.objects, vec![out]);
    // kerf wypychający kontur poza obszar też się liczy
    let mut d2 = Document::new("t");
    d2.workspace.width = 100.0;
    d2.add(0, rect(0., 10., 10., 10.)).unwrap();
    assert!(!codes(&plan(&d2)).contains(&"outside_workspace"));
    params(&mut d2, 0).kerf = 0.2;
    assert!(codes(&plan(&d2)).contains(&"outside_workspace"));
}

#[test]
fn req_15_054_warning_duplicates() {
    let mut d = Document::new("t");
    let a = d.add(0, rect(0., 0., 10., 10.)).unwrap();
    let b = d.add(0, rect(0., 0., 10., 10.)).unwrap();
    d.add(1, rect(0., 0., 10., 10.)).unwrap(); // inna warstwa: nie duplikat
    d.add(0, rect(20., 0., 10., 10.)).unwrap();
    let job = plan(&d);
    let dups: Vec<_> = job
        .warnings
        .iter()
        .filter(|w| w.code == "duplicate")
        .collect();
    assert_eq!(dups.len(), 1);
    assert_eq!(dups[0].objects, vec![a, b]);
    assert_eq!(dups[0].layer, Some(0));
    // ta sama geometria zapisana inaczej (ścieżka w przeciwnym kierunku) też jest duplikatem
    let mut d = Document::new("t");
    d.add(0, rect(0., 0., 10., 10.)).unwrap();
    d.add(0, path("M0 0 L0 10 L10 10 L10 0 Z")).unwrap();
    assert!(codes(&plan(&d)).contains(&"duplicate"));
}

#[test]
fn req_15_055_warning_cut_before_engrave() {
    let mut d = Document::new("t");
    d.add(0, rect(0., 0., 40., 40.)).unwrap();
    d.add(1, rect(5., 5., 10., 10.)).unwrap();
    set_mode(&mut d, 1, LayerMode::Fill);
    assert!(codes(&plan(&d)).contains(&"cut_before_engrave"));
    // grawer przed cięciem: bez ostrzeżenia
    set_mode(&mut d, 0, LayerMode::Fill);
    set_mode(&mut d, 1, LayerMode::Line);
    assert!(!codes(&plan(&d)).contains(&"cut_before_engrave"));
}

#[test]
fn req_15_056_warning_power_above_limit() {
    let mut d = Document::new("t");
    d.add(0, rect(0., 0., 10., 10.)).unwrap();
    d.add(1, rect(20., 0., 10., 10.)).unwrap();
    d.layer_mut(0).unwrap().power_max = 80.0;
    d.layer_mut(1).unwrap().power_max = 20.0;
    params(&mut d, 1).power2_max = Some(60.0);
    assert!(
        !codes(&plan(&d)).contains(&"power_above_limit"),
        "bez limitu bez ostrzeżeń"
    );
    let job = plan_with(
        &d,
        PlanOptions {
            power_limit: Some(50.0),
            ..Default::default()
        },
    );
    let ws: Vec<_> = job
        .warnings
        .iter()
        .filter(|w| w.code == "power_above_limit")
        .collect();
    assert_eq!(
        ws.iter().map(|w| w.layer).collect::<Vec<_>>(),
        vec![Some(0), Some(1)]
    );
    assert!(ws[0].message.contains("80"));
}
