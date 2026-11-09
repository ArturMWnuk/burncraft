//! M3.3: polecenia rejestru dla K40 Lihuiyu i Moshi (profil, płyta, kompilacja, zadanie).

use std::time::{Duration, Instant};

use bc_commands::{Registry, Session};
use serde_json::{json, Value};

fn run(r: &Registry, s: &mut Session, id: &str, p: Value) -> Value {
    s.execute(r, id, p).unwrap_or_else(|e| panic!("{id}: {e}"))
}

fn setup(r: &Registry, name: &str, profile: Value) -> Session {
    let mut s = Session::default();
    let d = std::env::temp_dir().join(format!("bc-k40-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    s.machines.lock().unwrap().set_store(d).unwrap();
    run(
        r,
        &mut s,
        "machine.profile_save",
        json!({ "profile": profile }),
    );
    run(
        r,
        &mut s,
        "shape.rect",
        json!({"x": 10, "y": 10, "w": 30, "h": 20, "layer": 0}),
    );
    run(
        r,
        &mut s,
        "layer.set",
        json!({"index": 0, "speed": 20, "power_max": 50, "air_assist": false}),
    );
    s
}

fn wait_done(r: &Registry, s: &mut Session) -> Value {
    let end = Instant::now() + Duration::from_secs(30);
    loop {
        let j = run(r, s, "job.status", json!({}));
        if j["status"] != "running" && j["status"] != "paused" || Instant::now() > end {
            return j;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn req_13_046_047_k40_profile_board_compile_and_job() {
    let r = Registry::builtin();
    let mut s = setup(
        &r,
        "lhy",
        json!({"name": "K40", "driver": "lihuiyu",
               "transport": {"kind": "emulator", "variant": "lihuiyu"},
               "workspace": [310, 210],
               "k40": {"board": "b1", "scale": [1.0, 1.0]}}),
    );
    let l = run(&r, &mut s, "machine.list", json!({}));
    let boards: Vec<&str> = l["boards"]
        .as_array()
        .unwrap()
        .iter()
        .map(|b| b["id"].as_str().unwrap())
        .collect();
    assert_eq!(boards.len(), 10);
    assert!(boards.contains(&"6c6879_m2"));
    let c = run(&r, &mut s, "machine.connect", json!({"name": "K40"}));
    assert_eq!(c["variant"], "b1");
    assert_eq!(c["capabilities"]["pwm"], false);
    assert_eq!(c["capabilities"]["native_raster"], true);
    // kompilacja EGV z płytą z profilu (B1: kod prędkości z członem przekątnej)
    let g = run(
        &r,
        &mut s,
        "gcode.compile",
        json!({"driver": "lihuiyu", "include_text": true}),
    );
    assert_eq!(g["extension"], "egv");
    let text = g["text"].as_str().unwrap();
    assert!(text.contains("CV") && text.ends_with("FNSE-\n"), "{text}");
    assert!(g["warnings"]
        .as_array()
        .unwrap()
        .iter()
        .any(|w| w["code"] == "power_ignored"));
    run(&r, &mut s, "job.start", json!({}));
    assert_eq!(wait_done(&r, &mut s)["status"], "done");
    run(&r, &mut s, "device.home", json!({}));
    let st = run(&r, &mut s, "machine.status", json!({}));
    assert_eq!(st["state"]["variant"], "b1");
}

#[test]
fn req_03_003_connect_sets_document_workspace_from_profile() {
    // test sprzętowy K40 (B4/A5): po bazowaniu w lewy górny róg krzyżyk głowicy stał w
    // połowie wysokości kanwy — dokument 400×400 mm, stół K40 310×210 mm
    let r = Registry::builtin();
    let mut s = setup(
        &r,
        "ws",
        json!({"name": "K40", "driver": "lihuiyu",
               "transport": {"kind": "emulator", "variant": "lihuiyu"},
               "workspace": [310, 210]}),
    );
    let d = run(&r, &mut s, "document.inspect", json!({}));
    assert_eq!(d["workspace"], json!([400.0, 400.0]));
    let rev = s.revision;
    run(&r, &mut s, "machine.connect", json!({"name": "K40"}));
    let d = run(&r, &mut s, "document.inspect", json!({}));
    assert_eq!(d["workspace"], json!([310.0, 210.0]));
    // zmiana dokumentu widoczna dla okna (doc.changed)
    assert!(s.revision > rev);
    // narożnik bazowania K40 (lewy górny) = lewy górny róg obszaru dokumentu
    run(&r, &mut s, "device.home", json!({}));
    let st = run(&r, &mut s, "machine.status", json!({}));
    let h = st["head"].as_array().unwrap();
    assert!(
        (h[0].as_f64().unwrap() - 0.0).abs() < 0.01
            && (h[1].as_f64().unwrap() - 210.0).abs() < 0.01,
        "{st}"
    );
}

#[test]
fn req_13_052_moshi_job_via_registry() {
    let r = Registry::builtin();
    let mut s = setup(
        &r,
        "moshi",
        json!({"name": "Moshi", "driver": "moshi",
               "transport": {"kind": "emulator", "variant": "moshi"},
               "workspace": [330, 210]}),
    );
    run(&r, &mut s, "machine.connect", json!({"name": "Moshi"}));
    let g = run(&r, &mut s, "gcode.compile", json!({"driver": "moshi"}));
    assert_eq!(g["extension"], "mos");
    assert_eq!(g["lines"], 1);
    run(&r, &mut s, "job.start", json!({}));
    assert_eq!(wait_done(&r, &mut s)["status"], "done");
}

#[test]
fn req_13_045_profile_without_pwm_validation() {
    let r = Registry::builtin();
    let mut s = Session::default();
    // sterownik K40 wymaga dostępu CH341 albo emulatora
    let e = s
        .execute(
            &r,
            "machine.profile_save",
            json!({"profile": {"name": "Zły", "driver": "lihuiyu",
                   "transport": {"kind": "serial", "port": "COM3"}}}),
        )
        .unwrap_err();
    assert_eq!(e.code, "invalid_params");
    let e = s
        .execute(
            &r,
            "machine.profile_save",
            json!({"profile": {"name": "Symulator K40 (Lihuiyu)",
                   "transport": {"kind": "emulator", "variant": "grbl11"}}}),
        )
        .unwrap_err();
    assert_eq!(e.code, "invalid_params");
}
