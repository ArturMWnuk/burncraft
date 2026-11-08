//! Polecenia urządzeń w rejestrze (M2.3, ADR 0010) na emulatorze: profile, połączenia,
//! ruch, ramki, start zadania, kolejka, ustawienia, makra, poziomy bezpieczeństwa.

use std::time::{Duration, Instant};

use bc_commands::{Registry, SafetyTier, Session, Workbench};
use serde_json::{json, Value};

fn run(r: &Registry, s: &mut Session, id: &str, p: Value) -> Value {
    s.execute(r, id, p).unwrap_or_else(|e| panic!("{id}: {e}"))
}

fn tmp(name: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("bc-mach-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    d
}

/// Sesja z profilem „Szybki” (emulator bez opóźnień) i prostokątem 10,10–40,30.
fn setup(r: &Registry, name: &str) -> Session {
    let mut s = Session::default();
    s.machines.lock().unwrap().set_store(tmp(name)).unwrap();
    run(
        r,
        &mut s,
        "machine.profile_save",
        json!({"profile": {"name": "Szybki", "transport": {"kind": "emulator", "variant": "grbl11"},
               "pointer_offset": [4, -2], "pointer_on": "M8", "pointer_off": "M9"}}),
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

fn wait_idle(r: &Registry, s: &mut Session) {
    let end = Instant::now() + Duration::from_secs(10);
    std::thread::sleep(Duration::from_millis(250));
    while run(r, s, "machine.status", json!({}))["state"]["state"] != "idle" && Instant::now() < end
    {
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn req_13_191_196_028_profiles_connect_select() {
    let r = Registry::builtin();
    let mut s = setup(&r, "profiles");
    let l = run(&r, &mut s, "machine.list", json!({}));
    let names: Vec<&str> = l["profiles"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["name"].as_str().unwrap())
        .collect();
    assert_eq!(
        names,
        vec![
            "Symulator",
            "Symulator K40 (Lihuiyu)",
            "Symulator Moshi",
            "Szybki"
        ]
    );
    assert!(l["variants"].as_array().unwrap().len() >= 19);
    // bez urządzenia: no_device
    assert_eq!(
        s.execute(&r, "device.jog", json!({"axis": "x", "distance": 1}))
            .unwrap_err()
            .code,
        "no_device"
    );
    let c = run(&r, &mut s, "machine.connect", json!({"name": "Szybki"}));
    assert_eq!(c["variant"], "grbl11");
    assert_eq!(c["capabilities"]["jog"], true);
    run(&r, &mut s, "machine.connect", json!({"name": "Symulator"}));
    assert_eq!(s.device.as_deref(), Some("Symulator"));
    let l = run(&r, &mut s, "machine.list", json!({}));
    assert_eq!(l["connected"].as_array().unwrap().len(), 2);
    run(&r, &mut s, "machine.select", json!({"name": "Szybki"}));
    assert_eq!(s.device.as_deref(), Some("Szybki"));
    let st = run(&r, &mut s, "machine.status", json!({"device": "Symulator"}));
    assert_eq!(st["device"], "Symulator");
    assert_eq!(st["state"]["state"], "idle");
    // urządzenia wspólne dla zakładek (nowy dokument zachowuje połączenia)
    let mut w = Workbench::new(s);
    w.execute(&r, "doc.new", json!({})).unwrap();
    assert_eq!(w.active().device.as_deref(), Some("Szybki"));
    w.execute(&r, "document.new", json!({})).unwrap();
    assert_eq!(w.active().machines.lock().unwrap().connected().len(), 2);
    let s = w.active_mut();
    run(&r, s, "machine.disconnect", json!({"device": "Symulator"}));
    assert_eq!(
        run(&r, s, "machine.list", json!({}))["connected"],
        json!(["Szybki"])
    );
    // błędne profile
    assert!(s
        .execute(
            &r,
            "machine.profile_save",
            json!({"profile": {"name": "Symulator", "transport": {"kind": "emulator"}}})
        )
        .is_err());
    assert!(s
        .execute(&r, "machine.profile_save", json!({"profile": {"name": "X", "driver": "ruida", "transport": {"kind": "tcp", "host": "a"}}}))
        .is_err());
}

#[test]
fn req_10_013_015_016_job_start_moves_and_positions() {
    let r = Registry::builtin();
    let mut s = setup(&r, "jobs");
    run(&r, &mut s, "machine.connect", json!({"name": "Szybki"}));
    let j = run(&r, &mut s, "job.start", json!({}));
    assert_eq!(j["status"], "running");
    let j = wait_done(&r, &mut s);
    assert_eq!(j["status"], "done", "{j}");
    // przesuń do zaznaczenia (środek) i kliknięcie w obszarze
    let ids: Vec<u64> = s.doc.objects.iter().map(|o| o.id).collect();
    s.selection = ids;
    let m = run(&r, &mut s, "device.move_to_selection", json!({}));
    assert_eq!((m["x"].as_f64(), m["y"].as_f64()), (Some(25.0), Some(20.0)));
    wait_idle(&r, &mut s);
    assert_eq!(
        run(&r, &mut s, "machine.status", json!({}))["state"]["mpos"],
        json!([25.0, 20.0, 0.0])
    );
    run(&r, &mut s, "device.set_position", json!({"x": 77, "y": 66}));
    wait_idle(&r, &mut s);
    assert_eq!(
        run(&r, &mut s, "machine.status", json!({}))["state"]["mpos"],
        json!([77.0, 66.0, 0.0])
    );
    // zero użytkownika i powrót do zera
    run(&r, &mut s, "device.set_origin", json!({}));
    run(&r, &mut s, "device.move_to", json!({"x": 5, "y": 5}));
    run(&r, &mut s, "device.go_origin", json!({}));
    wait_idle(&r, &mut s);
    let st = run(&r, &mut s, "machine.status", json!({}));
    assert_eq!(st["state"]["wpos"], json!([0.0, 0.0, 0.0]));
    assert_eq!(st["state"]["mpos"], json!([77.0, 66.0, 0.0]));
    // jog, home bez $22 (error:5 z opisem), odblokowanie
    run(
        &r,
        &mut s,
        "device.jog",
        json!({"axis": "y", "distance": 3, "speed": 10}),
    );
    let e = s.execute(&r, "device.home", json!({})).unwrap_err();
    assert_eq!(e.code, "machine_error");
    assert!(e.message.contains("error:5"));
    run(&r, &mut s, "device.unlock", json!({}));
    // konsola
    let c = run(&r, &mut s, "machine.console", json!({}));
    assert!(c["lines"].as_array().unwrap().iter().any(|l| l == "> $H"));
}

#[test]
fn req_10_007_010_frame_command_with_pointer() {
    let r = Registry::builtin();
    let mut s = setup(&r, "frame");
    run(&r, &mut s, "machine.connect", json!({"name": "Szybki"}));
    let f = run(&r, &mut s, "device.frame", json!({"kind": "rect"}));
    assert_eq!(f["points"], 4);
    let f = run(
        &r,
        &mut s,
        "device.frame",
        json!({"kind": "contour", "pointer": true}),
    );
    assert!(f["points"].as_u64().unwrap() >= 4);
    let c = run(&r, &mut s, "machine.console", json!({}));
    let lines: Vec<String> = c["lines"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| l.as_str().unwrap().to_string())
        .collect();
    assert!(lines.contains(&"> M8".to_string()) && lines.contains(&"> M9".to_string()));
    // offset czerwonego lasera (4, −2): narożnik 40,30 → głowica 36,32
    assert!(lines.iter().any(|l| l.contains("X36 Y32")), "{lines:?}");
    // bieżąca pozycja: ramka względem głowicy
    run(
        &r,
        &mut s,
        "job.start_settings",
        json!({"from": "current", "origin": "bottom_left"}),
    );
    run(&r, &mut s, "device.move_to", json!({"x": 100, "y": 100}));
    wait_idle(&r, &mut s);
    run(&r, &mut s, "device.frame", json!({}));
    let c = run(&r, &mut s, "machine.console", json!({}));
    assert!(c["lines"]
        .as_array()
        .unwrap()
        .iter()
        .any(|l| l.as_str().unwrap().contains("X130 Y120")));
}

#[test]
fn req_10_020_021_queue_repeat_and_gcode_source() {
    let r = Registry::builtin();
    let mut s = setup(&r, "queue");
    run(&r, &mut s, "machine.connect", json!({"name": "Szybki"}));
    // program G-code jako źródło (REQ-02-089: uruchamianie zaimportowanego programu)
    let path = tmp("queue-prog").with_extension("gcode");
    std::fs::write(&path, "G21 G90\nM4 S0\nG0 X0 Y0\nG1 X10 S500 F600\nM5\n").unwrap();
    run(
        &r,
        &mut s,
        "gcode.import",
        json!({"path": path.to_str().unwrap(), "to_document": false}),
    );
    let j = run(
        &r,
        &mut s,
        "job.start",
        json!({"source": "gcode", "repeat": 2, "delay_s": 0.05}),
    );
    assert_eq!(j["repeat"], 2);
    let j = wait_done(&r, &mut s);
    assert_eq!(j["status"], "done");
    assert_eq!(j["round"], 2);
    let st = run(&r, &mut s, "job.start", json!({"queue": true, "name": "a"}));
    assert_eq!(st["name"], "a");
    assert!(s.execute(&r, "job.start", json!({"repeat": 0})).is_err());
    let j = wait_done(&r, &mut s);
    assert_eq!(j["status"], "done");
}

#[test]
fn req_13_011_012_pause_resume_stop_commands() {
    let r = Registry::builtin();
    let mut s = setup(&r, "stop");
    run(
        &r,
        &mut s,
        "machine.profile_save",
        json!({"profile": {"name": "Wolny", "transport": {"kind": "emulator", "speed": 1.0}}}),
    );
    run(&r, &mut s, "machine.connect", json!({"name": "Wolny"}));
    run(&r, &mut s, "job.start", json!({}));
    std::thread::sleep(Duration::from_millis(200));
    run(&r, &mut s, "job.pause", json!({}));
    assert_eq!(run(&r, &mut s, "job.status", json!({}))["status"], "paused");
    run(&r, &mut s, "job.resume", json!({}));
    let st = run(&r, &mut s, "job.stop", json!({}));
    assert_eq!(st["stopped"], json!(["Wolny"]));
    assert_eq!(
        run(&r, &mut s, "job.status", json!({}))["status"],
        "stopped"
    );
    // stop bez urządzenia nie jest błędem
    let mut s2 = Session::default();
    assert_eq!(
        run(&r, &mut s2, "job.stop", json!({}))["stopped"],
        json!([])
    );
}

#[test]
fn req_13_198_013_084_settings_commands_with_backup() {
    let r = Registry::builtin();
    let dir = tmp("settings");
    let mut s = setup(&r, "settings");
    s.machines.lock().unwrap().set_store(&dir).unwrap();
    run(
        &r,
        &mut s,
        "machine.profile_save",
        json!({"profile": {"name": "Szybki", "transport": {"kind": "emulator"}}}),
    );
    run(&r, &mut s, "machine.connect", json!({"name": "Szybki"}));
    let st = run(&r, &mut s, "machine.settings", json!({}));
    let s110 = st["settings"]
        .as_array()
        .unwrap()
        .iter()
        .find(|x| x["id"] == "$110")
        .unwrap()
        .clone();
    assert_eq!(s110["name"], "prędkość maksymalna X");
    assert_eq!(s110["max"], 1000000.0);
    let d = run(
        &r,
        &mut s,
        "device.settings_write",
        json!({"changes": {"$110": "7000"}, "dry_run": true}),
    );
    assert_eq!(d["changes"][0]["before"], "6000.000");
    assert_eq!(d["backup"], Value::Null);
    let w = run(
        &r,
        &mut s,
        "device.settings_write",
        json!({"changes": {"$110": "7000", "$30": "255"}}),
    );
    let backup = w["backup"].as_str().unwrap();
    assert!(backup.contains("machine-backups"));
    assert!(std::path::Path::new(backup).exists());
    let chk = run(&r, &mut s, "machine.laser_check", json!({}));
    assert_eq!(chk["ok"], false);
    assert_eq!(chk["fixes"], json!([["$30", "1000"]]));
    assert_eq!(
        s.execute(
            &r,
            "device.settings_write",
            json!({"changes": {"$32": "7"}})
        )
        .unwrap_err()
        .code,
        "invalid_params"
    );
}

#[test]
fn req_01_040_041_macros_commands_and_raw() {
    let r = Registry::builtin();
    let mut s = setup(&r, "macros");
    run(&r, &mut s, "machine.connect", json!({"name": "Szybki"}));
    run(
        &r,
        &mut s,
        "macro.save",
        json!({"macro": {"name": "Narożnik", "steps": [
            {"kind": "command", "id": "device.move_to", "params": {"x": 12, "y": 34}},
            {"kind": "raw", "line": "G91 G0 X1"},
            {"kind": "raw", "line": "G90"},
            {"kind": "command", "id": "shape.rect", "params": {"x": 0, "y": 0, "w": 1, "h": 1, "layer": 1}}
        ]}}),
    );
    let l = run(&r, &mut s, "macro.list", json!({}));
    assert_eq!(l["macros"][0]["name"], "Narożnik");
    let n = s.doc.objects.len();
    let res = run(&r, &mut s, "macro.run", json!({"name": "Narożnik"}));
    assert_eq!(res["steps"], 4);
    assert_eq!(s.doc.objects.len(), n + 1);
    wait_idle(&r, &mut s);
    assert_eq!(
        run(&r, &mut s, "machine.status", json!({}))["state"]["mpos"],
        json!([13.0, 34.0, 0.0])
    );
    // walidacja: nieznane polecenie, rekurencja, wiele wierszy
    for bad in [
        json!({"macro": {"name": "a", "steps": [{"kind": "command", "id": "nie.ma"}]}}),
        json!({"macro": {"name": "a", "steps": [{"kind": "command", "id": "macro.run", "params": {"name": "a"}}]}}),
        json!({"macro": {"name": "a", "steps": [{"kind": "raw", "line": "G0\nM3"}]}}),
        json!({"macro": {"name": "a", "steps": []}}),
    ] {
        assert!(s.execute(&r, "macro.save", bad.clone()).is_err(), "{bad}");
    }
    // błąd kroku przerywa makro z numerem kroku
    run(
        &r,
        &mut s,
        "macro.save",
        json!({"macro": {"name": "Zły", "steps": [{"kind": "raw", "line": "G99"}]}}),
    );
    let e = s
        .execute(&r, "macro.run", json!({"name": "Zły"}))
        .unwrap_err();
    assert!(e.message.contains("krok 1"), "{}", e.message);
    // surowe polecenie pojedyncze
    let raw = run(&r, &mut s, "device.raw", json!({"line": "$I"}));
    assert!(raw["response"][0].as_str().unwrap().starts_with("[VER:"));
    // makra zapisane w macros.json
    let s2 = Session::default();
    s2.machines
        .lock()
        .unwrap()
        .set_store(tmp("macros-reload"))
        .unwrap();
    assert!(s2.machines.lock().unwrap().macros().is_empty());
    assert_eq!(
        run(&r, &mut s, "macro.delete", json!({"name": "Zły"}))["removed"],
        true
    );
}

#[test]
fn req_13_009_safety_tiers_of_device_commands() {
    let r = Registry::builtin();
    let tier = |id: &str| r.get(id).unwrap_or_else(|| panic!("{id}")).spec.safety_tier;
    for id in [
        "device.jog",
        "device.home",
        "device.move_to",
        "device.frame",
        "device.set_origin",
        "job.pause",
    ] {
        assert_eq!(tier(id), SafetyTier::Motion, "{id}");
    }
    for id in [
        "device.test_fire",
        "job.start",
        "device.raw",
        "macro.run",
        "device.sd_run",
    ] {
        assert_eq!(tier(id), SafetyTier::Fire, "{id}");
    }
    assert_eq!(tier("device.settings_write"), SafetyTier::Config);
    assert_eq!(tier("job.stop"), SafetyTier::Read, "stop zawsze dozwolony");
    assert_eq!(tier("machine.status"), SafetyTier::Read);
    // test wiązki przez polecenie
    let mut s = setup(&r, "fire");
    run(&r, &mut s, "machine.connect", json!({"name": "Szybki"}));
    run(
        &r,
        &mut s,
        "device.test_fire",
        json!({"power": 10, "ms": 50}),
    );
    assert!(s
        .execute(&r, "device.test_fire", json!({"power": 10, "ms": 0}))
        .is_err());
    run(&r, &mut s, "device.override", json!({"feed": 120}));
    let e = s
        .execute(&r, "device.probe_z", json!({"distance": 5}))
        .unwrap_err();
    assert!(matches!(e.code, "alarm" | "invalid_params"), "{e:?}");
}
