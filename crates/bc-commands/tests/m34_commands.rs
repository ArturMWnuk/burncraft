//! M3.4 — pliki na sterowniku w poleceniach rejestru (REQ-13-014, REQ-03-042): lista
//! (`machine.files`), wysyłka projektu albo pliku z dysku (`device.sd_upload`),
//! uruchomienie (`device.sd_run`) i usunięcie (`device.sd_delete`) na emulatorze FluidNC;
//! urządzenia bez pamięci plików (K40, Moshi) — lista „nieobsługiwana” bez błędu.
//! Okno „Lista plików na sterowniku” (`files.open`/`files.close`) wykonuje GUI.

use std::time::{Duration, Instant};

use bc_commands::{Executor, Registry, SafetyTier, Session};
use serde_json::{json, Value};

fn run(r: &Registry, s: &mut Session, id: &str, p: Value) -> Value {
    s.execute(r, id, p).unwrap_or_else(|e| panic!("{id}: {e}"))
}

fn setup(r: &Registry, name: &str) -> Session {
    let mut s = Session::default();
    let d = std::env::temp_dir().join(format!("bc-m34-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    s.machines.lock().unwrap().set_store(d).unwrap();
    run(
        r,
        &mut s,
        "machine.profile_save",
        json!({"profile": {"name": "FluidNC", "driver": "grbl",
               "transport": {"kind": "emulator", "variant": "fluidnc"},
               "workspace": [400, 300]}}),
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
        json!({"index": 0, "speed": 50, "power_max": 40, "air_assist": false}),
    );
    s
}

fn files(r: &Registry, s: &mut Session) -> Vec<(String, u64)> {
    run(r, s, "machine.files", json!({}))["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| {
            (
                f["name"].as_str().unwrap().to_string(),
                f["size"].as_u64().unwrap(),
            )
        })
        .collect()
}

/// Czeka na stan Idle w punkcie `x` (plik z karty wykonuje się w tle).
fn wait_at(r: &Registry, s: &mut Session, x: f64) -> Value {
    let end = Instant::now() + Duration::from_secs(20);
    loop {
        let st = run(r, s, "machine.status", json!({}));
        let at = (st["state"]["wpos"][0].as_f64().unwrap_or(f64::NAN) - x).abs() < 1e-6;
        if st["state"]["state"] == "idle" && at || Instant::now() > end {
            return st;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn req_13_014_files_commands_on_fluidnc_emulator() {
    let r = Registry::builtin();
    let mut s = setup(&r, "fluidnc");
    let c = run(&r, &mut s, "machine.connect", json!({"name": "FluidNC"}));
    assert_eq!(c["capabilities"]["files"], true);
    let l = run(&r, &mut s, "machine.files", json!({}));
    assert_eq!(l["supported"], true);
    assert_eq!(l["files"], json!([]));
    assert_eq!(l["total_bytes"], 0);
    // projekt skompilowany przez sterownik urządzenia
    let up = run(
        &r,
        &mut s,
        "device.sd_upload",
        json!({"name": "/sd/projekt.gcode"}),
    );
    let n = up["bytes"].as_u64().unwrap();
    assert!(n > 50, "{up}");
    assert_eq!(up["source"], "document");
    // plik z dysku (treść G-code przekazana przez okno)
    let text = "G21 G90\nG0 X5 Y5\nG1 X25 F600 S300 M3\nM5\n";
    let up = run(
        &r,
        &mut s,
        "device.sd_upload",
        json!({"name": "/sd/reczny.gcode", "text": text}),
    );
    assert_eq!(up["bytes"], text.len());
    assert_eq!(up["source"], "text");
    assert_eq!(
        files(&r, &mut s),
        vec![
            ("/sd/projekt.gcode".to_string(), n),
            ("/sd/reczny.gcode".to_string(), text.len() as u64)
        ]
    );
    let l = run(&r, &mut s, "machine.files", json!({}));
    assert_eq!(l["total_bytes"], n + text.len() as u64);
    // uruchomienie pliku z karty: maszyna pracuje, potem wraca do Idle
    run(
        &r,
        &mut s,
        "device.sd_run",
        json!({"name": "/sd/reczny.gcode"}),
    );
    let st = wait_at(&r, &mut s, 25.0);
    assert_eq!(st["state"]["state"], "idle");
    let x = st["state"]["wpos"][0].as_f64().unwrap();
    assert!((x - 25.0).abs() < 1e-6, "{st}");
    // usunięcie
    run(
        &r,
        &mut s,
        "device.sd_delete",
        json!({"name": "/sd/reczny.gcode"}),
    );
    assert_eq!(
        files(&r, &mut s),
        vec![("/sd/projekt.gcode".to_string(), n)]
    );
    // błędy: brak pliku, zła nazwa, pusta treść, nieznane pole
    let e = s
        .execute(&r, "device.sd_run", json!({"name": "/sd/brak.gcode"}))
        .unwrap_err();
    assert_eq!(e.code, "machine_error");
    let e = s
        .execute(&r, "device.sd_delete", json!({"name": "/sd/a\n$H"}))
        .unwrap_err();
    assert_eq!(e.code, "invalid_params");
    let e = s
        .execute(
            &r,
            "device.sd_upload",
            json!({"name": "/sd/pusty.gcode", "text": ""}),
        )
        .unwrap_err();
    assert_eq!(e.code, "invalid_params");
    let e = s
        .execute(
            &r,
            "device.sd_run",
            json!({"name": "/sd/projekt.gcode", "text": "G0"}),
        )
        .unwrap_err();
    assert_eq!(e.code, "invalid_params");
}

#[test]
fn req_13_014_files_unsupported_on_k40_and_moshi() {
    let r = Registry::builtin();
    let mut s = setup(&r, "k40");
    for name in ["Symulator K40 (Lihuiyu)", "Symulator Moshi"] {
        run(&r, &mut s, "machine.connect", json!({"name": name}));
        // okno pokazuje komunikat zamiast błędu
        let l = run(&r, &mut s, "machine.files", json!({}));
        assert_eq!(l["supported"], false, "{name}");
        assert_eq!(l["files"], json!([]), "{name}");
        assert!(!l["reason"].as_str().unwrap().is_empty(), "{name}");
        for (id, p) in [
            (
                "device.sd_upload",
                json!({"name": "/a.gcode", "text": "G0"}),
            ),
            ("device.sd_run", json!({"name": "/a.gcode"})),
            ("device.sd_delete", json!({"name": "/a.gcode"})),
        ] {
            let e = s.execute(&r, id, p).unwrap_err();
            assert_eq!(e.code, "unsupported", "{name} {id}");
        }
        run(&r, &mut s, "machine.disconnect", json!({}));
    }
}

#[test]
fn req_03_042_files_window_commands() {
    let r = Registry::builtin();
    for id in ["files.open", "files.close"] {
        let c = r.get(id).unwrap_or_else(|| panic!("brak {id}"));
        assert_eq!(c.spec.executor, Executor::Window, "{id}");
        assert!(c.spec.requirements.contains(&"REQ-03-042"), "{id}");
        assert!(c.spec.requirements.contains(&"REQ-01-029"), "{id}");
    }
    // kontrolki okna: polecenia urządzenia z wymaganiami i poziomem bezpieczeństwa
    for (id, tier) in [
        ("machine.files", SafetyTier::Read),
        ("device.sd_upload", SafetyTier::Config),
        ("device.sd_run", SafetyTier::Fire),
        ("device.sd_delete", SafetyTier::Config),
    ] {
        let c = r.get(id).unwrap_or_else(|| panic!("brak {id}"));
        assert!(c.spec.requirements.contains(&"REQ-13-014"), "{id}");
        assert!(c.spec.requirements.contains(&"REQ-03-042"), "{id}");
        assert_eq!(c.spec.safety_tier, tier, "{id}");
        assert!(c.spec.requires_device, "{id}");
    }
    let ps = &r.get("device.sd_upload").unwrap().spec.params_schema["properties"];
    assert!(ps["name"].is_object() && ps["text"].is_object());
}
