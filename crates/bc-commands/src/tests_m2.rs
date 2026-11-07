//! Testy M2.1: polecenia planera — parametry warstwy, ustawienia planu,
//! mostki, kreator mostków, plan.job i plan.estimate (ADR 0009).

use super::*;
use serde_json::json;

fn run(r: &Registry, s: &mut Session, id: &str, p: Value) -> Value {
    s.execute(r, id, p).unwrap_or_else(|e| panic!("{id}: {e}"))
}

fn rect(r: &Registry, s: &mut Session, x: f64, y: f64, w: f64, h: f64, layer: u8) -> u64 {
    run(
        r,
        s,
        "shape.rect",
        json!({"x": x, "y": y, "w": w, "h": h, "layer": layer}),
    )["id"]
        .as_u64()
        .unwrap()
}

#[test]
fn req_07_046_layer_params_command_merge_lpi_and_validation() {
    let r = Registry::builtin();
    let mut s = Session::default();
    let spec = &r.get("layer.params").expect("polecenie layer.params").spec;
    assert_eq!(spec.safety_tier, SafetyTier::Design);
    assert!(spec.undoable);
    assert!(spec.requirements.contains(&"REQ-07-026"));
    assert!(spec.params_schema["properties"]["params"]["properties"]["kerf"].is_object());
    let res = run(
        &r,
        &mut s,
        "layer.params",
        json!({"index": 0, "params": {"kerf": 0.1, "tabs": {"enabled": true},
               "overscan": {"kind": "percent", "percent": 2.0}, "pause_message": "Zmień materiał"}, "lpi": 254}),
    );
    assert_eq!(res["params"]["kerf"], 0.1);
    let p = &s.doc.layer(0).unwrap().params;
    assert!(p.tabs.enabled);
    assert_eq!(p.tabs.size, 0.5, "pozostałe pola mostków bez zmian");
    assert!((p.interval - 0.1).abs() < 1e-12, "LPI 254 = 0,1 mm");
    assert_eq!(p.overscan, bc_doc::Overscan::Percent { percent: 2.0 });
    // zmiana z wariantu na wariant (overscan auto)
    run(
        &r,
        &mut s,
        "layer.params",
        json!({"index": 0, "params": {"overscan": {"kind": "auto"}}, "dpi": 508}),
    );
    let p = &s.doc.layer(0).unwrap().params;
    assert_eq!(p.overscan, bc_doc::Overscan::Auto);
    assert!((p.interval - 0.05).abs() < 1e-12);
    // błędy: nieznane pole, zły zakres — bez zmian w dokumencie
    let before = s.doc.clone();
    for bad in [
        json!({"index": 0, "params": {"kerff": 1}}),
        json!({"index": 0, "params": {"interval": -1}}),
        json!({"index": 0, "params": {"kerf_x": 0.1, "kerf_y": -0.1}}),
        json!({"index": 99, "params": {}}),
    ] {
        assert_eq!(
            s.execute(&r, "layer.params", bad.clone()).unwrap_err().code,
            "invalid_params",
            "{bad}"
        );
    }
    assert_eq!(s.doc, before);
    // cofnięcie
    run(&r, &mut s, "edit.undo", json!({}));
    run(&r, &mut s, "edit.undo", json!({}));
    assert_eq!(
        s.doc.layer(0).unwrap().params,
        bc_doc::LayerParams::default()
    );
}

#[test]
fn req_08_009_plan_settings_command() {
    let r = Registry::builtin();
    let mut s = Session::default();
    let res = run(
        &r,
        &mut s,
        "plan.settings",
        json!({"settings": {"no_go_zones": [{"name": "zacisk", "rect": [0, 0, 20, 10]}],
               "kinematics": {"accel_y": 500}, "cut_last": true}}),
    );
    assert_eq!(res["cut_last"], true);
    let p = &s.doc.plan;
    assert_eq!(p.no_go_zones.len(), 1);
    assert!(
        p.no_go_zones[0].travel && p.no_go_zones[0].beam,
        "domyślnie obie blokady"
    );
    assert_eq!(
        (p.kinematics.accel_x, p.kinematics.accel_y),
        (1000.0, 500.0)
    );
    let e = s
        .execute(
            &r,
            "plan.settings",
            json!({"settings": {"no_go_zones": [{"rect": [10, 0, 0, 10]}]}}),
        )
        .unwrap_err();
    assert_eq!(e.code, "invalid_params");
    assert!(r.get("plan.settings").unwrap().spec.undoable);
}

#[test]
fn req_04_083_object_tabs_command() {
    let r = Registry::builtin();
    let mut s = Session::default();
    let id = rect(&r, &mut s, 100., 0., 10., 10., 0);
    let res = run(
        &r,
        &mut s,
        "object.tabs",
        json!({"id": id, "tabs": [{"at": [105, 0], "size": 2}]}),
    );
    assert_eq!(res["tabs"], 1);
    // zapis we współrzędnych lokalnych obiektu
    run(
        &r,
        &mut s,
        "object.transform",
        json!({"ids": [id], "translate": [-100, 0]}),
    );
    let job = run(&r, &mut s, "plan.job", json!({}));
    assert!(
        (job["cut_length"].as_f64().unwrap() - 38.0).abs() < 1e-6,
        "{job}"
    );
    run(&r, &mut s, "object.tabs", json!({"id": id, "tabs": []}));
    assert!(s.doc.find(id).unwrap().tabs.is_empty());
}

#[test]
fn req_04_085_tabs_wizard_command() {
    let r = Registry::builtin();
    let mut s = Session::default();
    let a = rect(&r, &mut s, 0., 0., 10., 10., 0);
    rect(&r, &mut s, 50., 0., 10., 10., 0);
    run(&r, &mut s, "selection.set", json!({"ids": [a]}));
    let res = run(
        &r,
        &mut s,
        "plan.tabs_wizard",
        json!({"count": 4, "size": 1.0}),
    );
    assert_eq!(
        (res["objects"].as_u64(), res["tabs"].as_u64()),
        (Some(1), Some(4))
    );
    let job = run(&r, &mut s, "plan.job", json!({}));
    assert!(
        (job["cut_length"].as_f64().unwrap() - 76.0).abs() < 1e-6,
        "{job}"
    );
    run(&r, &mut s, "edit.undo", json!({}));
    assert!(s.doc.find(a).unwrap().tabs.is_empty());
    let e = s
        .execute(
            &r,
            "plan.tabs_wizard",
            json!({"ids": [a], "count": 0, "spacing": 0}),
        )
        .unwrap_err();
    assert_eq!(e.code, "invalid_params");
}

#[test]
fn req_15_051_plan_job_command_stats() {
    let r = Registry::builtin();
    let mut s = Session::default();
    rect(&r, &mut s, 10., 10., 10., 10., 0);
    rect(&r, &mut s, 50., 10., 10., 10., 0);
    let spec = &r.get("plan.job").unwrap().spec;
    assert_eq!(spec.safety_tier, SafetyTier::Read);
    assert!(!spec.undoable);
    let rev = s.revision;
    let res = run(&r, &mut s, "plan.job", json!({"rate_per_hour": 36.0}));
    assert!((res["cut_length"].as_f64().unwrap() - 80.0).abs() < 1e-6);
    assert!(res["travel_length"].as_f64().unwrap() > 40.0);
    assert_eq!(res["passes"], 1);
    assert_eq!(res["hash"].as_str().unwrap().len(), 64);
    let total = res["time"]["total_s"].as_f64().unwrap();
    assert!(total > 0.8, "{res}");
    assert!((res["cost"]["machine_cost"].as_f64().unwrap() - total / 3600.0 * 36.0).abs() < 1e-9);
    assert!(res["warnings"].as_array().unwrap().is_empty());
    assert_eq!(res["ops"][0]["layer"], 0);
    assert!(res["ops"][0]["time_s"].as_f64().unwrap() > 0.0);
    // dokument się nie zmienia, historia bez kroku
    assert_eq!(s.revision, rev);
}

#[test]
fn req_09_008_plan_estimate_command() {
    let r = Registry::builtin();
    let mut s = Session::default();
    run(&r, &mut s, "shape.path", json!({"d": "M0 0 L100 0"}));
    let e = run(&r, &mut s, "plan.estimate", json!({}));
    assert!((e["cut_s"].as_f64().unwrap() - 1.1).abs() < 1e-9, "{e}");
    assert_eq!(e["formatted"], "00:00:01");
    let job = run(&r, &mut s, "plan.job", json!({}));
    assert_eq!(job["time"]["total_s"], e["total_s"]);
}

#[test]
fn req_08_013_plan_job_selection_only() {
    let r = Registry::builtin();
    let mut s = Session::default();
    let a = rect(&r, &mut s, 0., 0., 10., 10., 0);
    rect(&r, &mut s, 50., 0., 10., 10., 0);
    run(&r, &mut s, "selection.clear", json!({}));
    let e = s
        .execute(&r, "plan.job", json!({"selection_only": true}))
        .unwrap_err();
    assert_eq!(e.code, "invalid_params");
    run(&r, &mut s, "selection.set", json!({"ids": [a]}));
    let res = run(
        &r,
        &mut s,
        "plan.job",
        json!({"selection_only": true, "selection_origin": true}),
    );
    assert!((res["cut_length"].as_f64().unwrap() - 40.0).abs() < 1e-6);
    assert_eq!(res["origin"], json!([0.0, 0.0]));
}

#[test]
fn req_08_019_plan_job_progress_and_cancel() {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};
    let r = Registry::builtin();
    let mut s = Session::default();
    for l in 0..4u8 {
        rect(&r, &mut s, 10. * l as f64, 0., 5., 5., l);
    }
    let seen = Arc::new(Mutex::new(Vec::new()));
    let cancel = Arc::new(AtomicBool::new(false));
    let seen2 = seen.clone();
    s.progress = Some(Progress {
        report: Arc::new(move |f| seen2.lock().unwrap().push(f)),
        cancel: cancel.clone(),
    });
    run(&r, &mut s, "plan.job", json!({}));
    assert!(
        seen.lock().unwrap().iter().any(|f| *f >= 1.0),
        "postęp do 100 %"
    );
    cancel.store(true, Ordering::Relaxed);
    assert_eq!(
        s.execute(&r, "plan.job", json!({})).unwrap_err().code,
        "cancelled"
    );
}
