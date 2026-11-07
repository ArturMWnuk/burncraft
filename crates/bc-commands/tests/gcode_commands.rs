//! Polecenia G-code w rejestrze (M2.2, ADR 0011): gcode.compile, gcode.export,
//! gcode.import, gcode.program, gcode.settings, job.start_settings, makra warstw
//! przez layer.params i przekierowanie file.import / file.export dla plików G-code.

use bc_commands::{Registry, SafetyTier, Session};
use serde_json::{json, Value};

fn run(r: &Registry, s: &mut Session, id: &str, p: Value) -> Value {
    s.execute(r, id, p).unwrap_or_else(|e| panic!("{id}: {e}"))
}

fn tmp(name: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("bc-gcode-cmd-{}", std::process::id()));
    std::fs::create_dir_all(&d).unwrap();
    d.join(name)
}

fn with_rect(r: &Registry) -> Session {
    let mut s = Session::default();
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
        json!({"index": 0, "speed": 10, "power_max": 50}),
    );
    s
}

#[test]
fn req_10_019_save_gcode_file() {
    let r = Registry::builtin();
    let spec = &r.get("gcode.export").expect("gcode.export").spec;
    assert_eq!(spec.safety_tier, SafetyTier::Design);
    assert!(spec.requirements.contains(&"REQ-10-019"));
    let mut s = with_rect(&r);
    let path = tmp("zapis.gcode");
    let res = run(
        &r,
        &mut s,
        "gcode.export",
        json!({"path": path.to_str().unwrap()}),
    );
    let bytes = std::fs::read(&path).unwrap();
    assert_eq!(res["bytes"].as_u64().unwrap() as usize, bytes.len());
    assert_eq!(res["driver"], "grbl");
    let text = String::from_utf8(bytes).unwrap();
    assert!(text.contains("G1 X40 S500 F600"), "{text}");
    // ten sam wynik co gcode.compile
    let c = run(&r, &mut s, "gcode.compile", json!({"include_text": true}));
    assert_eq!(c["text"].as_str().unwrap(), text);
    assert_eq!(c["hash"], res["hash"]);
    // zapis nie zmienia dokumentu ani historii
    assert!(!s
        .history
        .undo_labels()
        .iter()
        .any(|l| l.starts_with("gcode.")));
    // nieobsługiwany sterownik i zła ścieżka
    let e = s
        .execute(
            &r,
            "gcode.export",
            json!({"path": path.to_str().unwrap(), "driver": "ruida"}),
        )
        .unwrap_err();
    assert_eq!(e.code, "unsupported_driver");
    let bad = tmp("brak/katalogu/x.gcode");
    let e = s
        .execute(&r, "gcode.export", json!({"path": bad.to_str().unwrap()}))
        .unwrap_err();
    assert_eq!(e.code, "failed");
}

#[test]
fn req_02_090_export_gcode_via_file_export() {
    let r = Registry::builtin();
    let mut s = with_rect(&r);
    for ext in ["gcode", "nc", "gc"] {
        let path = tmp(&format!("eksport.{ext}"));
        let res = run(
            &r,
            &mut s,
            "file.export",
            json!({"path": path.to_str().unwrap()}),
        );
        assert_eq!(res["format"], "gcode-grbl");
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("M4 S0") && text.contains("M5"));
    }
    // ogólny CNC przez gcode.export
    let path = tmp("cnc.nc");
    let res = run(
        &r,
        &mut s,
        "gcode.export",
        json!({"path": path.to_str().unwrap(), "driver": "gcode"}),
    );
    assert_eq!(res["driver"], "gcode");
    assert!(!std::fs::read_to_string(&path).unwrap().contains("M4"));
    // tylko zaznaczone
    run(
        &r,
        &mut s,
        "shape.rect",
        json!({"x": 100, "y": 100, "w": 5, "h": 5, "layer": 0}),
    );
    let all = run(&r, &mut s, "gcode.compile", json!({}));
    let sel = run(&r, &mut s, "gcode.compile", json!({"selection_only": true}));
    assert!(sel["bytes"].as_u64() < all["bytes"].as_u64());
    // ids w file.export dla G-code — błąd z podpowiedzią
    let e = s
        .execute(
            &r,
            "file.export",
            json!({"path": tmp("x.gcode").to_str().unwrap(), "ids": [1]}),
        )
        .unwrap_err();
    assert!(e.message.contains("gcode.export"));
}

#[test]
fn req_02_089_import_gcode_command_preview_and_run() {
    let r = Registry::builtin();
    // program z kompilatora (eksport → import)
    let mut src = with_rect(&r);
    let path = tmp("program.gcode");
    run(
        &r,
        &mut src,
        "gcode.export",
        json!({"path": path.to_str().unwrap()}),
    );
    let mut s = Session::default();
    let res = run(
        &r,
        &mut s,
        "gcode.import",
        json!({"path": path.to_str().unwrap(), "layer": 3}),
    );
    assert!(
        (res["cut_length"].as_f64().unwrap() - 100.0).abs() < 1e-3,
        "{res}"
    );
    assert_eq!(res["dynamic_power"], true);
    let ids = res["ids"].as_array().unwrap();
    assert_eq!(ids.len(), 1, "jeden zamknięty łańcuch");
    assert_eq!(s.selection.len(), 1);
    let o = s.doc.find(ids[0].as_u64().unwrap()).unwrap();
    assert_eq!(o.layer, 3);
    // podgląd planu z dokumentu = tor programu
    let plan = run(&r, &mut s, "plan.job", json!({}));
    assert!((plan["cut_length"].as_f64().unwrap() - 100.0).abs() < 1e-3);
    // program gotowy do uruchomienia: treść bez zmian
    let p = run(&r, &mut s, "gcode.program", json!({"include_text": true}));
    assert_eq!(p["loaded"], true);
    assert_eq!(
        p["text"].as_str().unwrap(),
        std::fs::read_to_string(&path).unwrap()
    );
    let g = s.gcode.as_ref().unwrap();
    assert!((g.job.stats().cut_length - 100.0).abs() < 1e-3);
    // import cofalny
    run(&r, &mut s, "edit.undo", json!({}));
    assert!(s.doc.objects.is_empty());
    // file.import przekazuje pliki G-code; bez dokumentu: tylko program
    let res = run(
        &r,
        &mut s,
        "file.import",
        json!({"path": path.to_str().unwrap()}),
    );
    assert_eq!(res["ids"].as_array().unwrap().len(), 1);
    let mut s2 = Session::default();
    let res = run(
        &r,
        &mut s2,
        "gcode.import",
        json!({"path": path.to_str().unwrap(), "to_document": false}),
    );
    assert!(res["ids"].as_array().unwrap().is_empty() && s2.doc.objects.is_empty());
    assert!(s2.gcode.is_some());
    // błędy
    let bad = tmp("zly.nc");
    std::fs::write(&bad, "G1 X1..2\n").unwrap();
    let e = s2
        .execute(&r, "gcode.import", json!({"path": bad.to_str().unwrap()}))
        .unwrap_err();
    assert!(e.message.contains("wiersz 1"), "{}", e.message);
    let e = s2
        .execute(
            &r,
            "gcode.import",
            json!({"path": path.to_str().unwrap(), "layer": 250}),
        )
        .unwrap_err();
    assert_eq!(e.code, "invalid_params");
}

#[test]
fn req_10_006_start_settings_saved_in_bcraft() {
    let r = Registry::builtin();
    let spec = &r
        .get("job.start_settings")
        .expect("job.start_settings")
        .spec;
    assert!(spec.undoable && spec.requirements.contains(&"REQ-10-006"));
    let mut s = with_rect(&r);
    let res = run(
        &r,
        &mut s,
        "job.start_settings",
        json!({"from": "user_origin", "origin": "center"}),
    );
    assert_eq!(res, json!({"from": "user_origin", "origin": "center"}));
    run(
        &r,
        &mut s,
        "gcode.settings",
        json!({"settings": {"progress_m73": true, "header": "; {name}"}}),
    );
    let path = tmp("start.bcraft");
    run(
        &r,
        &mut s,
        "file.save",
        json!({"path": path.to_str().unwrap()}),
    );
    let mut s2 = Session::default();
    run(
        &r,
        &mut s2,
        "file.open",
        json!({"path": path.to_str().unwrap()}),
    );
    assert_eq!(s2.doc.start, s.doc.start);
    assert_eq!(s2.doc.start.from, bc_doc::StartFrom::UserOrigin);
    assert_eq!(s2.doc.start.origin, bc_doc::JobOrigin::Center);
    assert!(s2.doc.gcode.progress_m73);
    // wynik kompilacji z zapisanymi ustawieniami startu (względem środka ramki)
    let c = run(&r, &mut s2, "gcode.compile", json!({"include_text": true}));
    assert!(c["text"].as_str().unwrap().contains("G0 X-15 Y-10"), "{c}");
    // cofanie
    run(&r, &mut s, "edit.undo", json!({}));
    run(&r, &mut s, "edit.undo", json!({}));
    assert_eq!(s.doc.start, bc_doc::StartSettings::default());
    // zły parametr
    assert!(s
        .execute(&r, "job.start_settings", json!({"from": "middle"}))
        .is_err());
}

#[test]
fn req_13_105_to_108_settings_and_layer_macros_commands() {
    let r = Registry::builtin();
    let mut s = with_rect(&r);
    let spec = &r.get("gcode.settings").unwrap().spec;
    assert!(spec.params_schema["properties"]["settings"]["properties"]["header"].is_object());
    run(
        &r,
        &mut s,
        "gcode.settings",
        json!({"settings": {"header": "; H {name}", "footer": "M30", "units_block": "G21 G90",
               "job_pre": "; JP", "job_post": "; JQ", "laser2": {"enabled": true}}}),
    );
    assert_eq!(
        s.doc.gcode.laser2.select2, "T1",
        "pozostałe pola lasera 2 bez zmian"
    );
    run(
        &r,
        &mut s,
        "layer.params",
        json!({"index": 0, "params": {"gcode_pre": "; LP {label}", "gcode_post": "; LQ"}}),
    );
    let t = run(&r, &mut s, "gcode.compile", json!({"include_text": true}))["text"]
        .as_str()
        .unwrap()
        .to_string();
    let l: Vec<&str> = t.lines().collect();
    let pos = |x: &str| {
        l.iter()
            .position(|v| *v == x)
            .unwrap_or_else(|| panic!("{x}: {t}"))
    };
    assert_eq!(l[0], "; H Bez tytulu");
    assert!(pos("G21 G90") < pos("; JP") && pos("; JP") < pos("; LP C00"));
    assert!(pos("; LP C00") < pos("; LQ") && pos("; LQ") < pos("; JQ") && pos("; JQ") < pos("M30"));
    // walidacja: znaki sterujące i nieznane pola — bez zmian w dokumencie
    let before = s.doc.gcode.clone();
    for bad in [
        json!({"settings": {"header": "a\u{0007}b"}}),
        json!({"settings": {"s_max": -1}}),
        json!({"settings": {"nope": 1}}),
        json!({"settings": {"decimals": 9}}),
    ] {
        assert!(
            s.execute(&r, "gcode.settings", bad.clone()).is_err(),
            "{bad}"
        );
    }
    assert_eq!(s.doc.gcode, before);
    assert!(s
        .execute(
            &r,
            "layer.params",
            json!({"index": 0, "params": {"gcode_pre": "\u{0001}"}})
        )
        .is_err());
}

#[test]
fn req_13_015_compile_command_reports_and_ticks() {
    let r = Registry::builtin();
    let mut s = with_rect(&r);
    run(
        &r,
        &mut s,
        "layer.params",
        json!({"index": 0, "params": {"power2_max": 20}}),
    );
    let c = run(&r, &mut s, "gcode.compile", json!({"driver": "grbl"}));
    assert_eq!(c["extension"], "gcode");
    assert!(c["lines"].as_u64().unwrap() > 5);
    assert!(c["time_s"].as_f64().unwrap() > 0.0);
    assert!(c.get("text").is_none());
    let codes: Vec<&str> = c["warnings"]
        .as_array()
        .unwrap()
        .iter()
        .map(|w| w["code"].as_str().unwrap())
        .collect();
    assert!(codes.contains(&"laser2_disabled"), "{codes:?}");
    // anulowanie długiej operacji (REQ-16-006)
    let cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
    s.progress = Some(bc_commands::Progress {
        report: std::sync::Arc::new(|_| {}),
        cancel: cancel.clone(),
    });
    let e = s.execute(&r, "gcode.compile", json!({})).unwrap_err();
    assert_eq!(e.code, "cancelled");
}
