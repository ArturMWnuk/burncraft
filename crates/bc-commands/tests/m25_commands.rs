//! M2.5 — polecenia GUI maszyny: podgląd zadania z odtwarzaniem (`plan.preview`),
//! wizualizacja kerfu (`plan.kerf`), „rozpocznij od tego miejsca” (`from_segment`),
//! licznik przejść i czas zadania (`job.status`), położenie głowicy w układzie
//! dokumentu (`machine.status`), zapamiętane pozycje, wyrównanie do pozycji lasera
//! i polecenia okien (`preview.*`, `jog.settings`).

use std::time::{Duration, Instant};

use bc_commands::{Executor, Registry, SafetyTier, Session};
use serde_json::{json, Value};

fn run(r: &Registry, s: &mut Session, id: &str, p: Value) -> Value {
    s.execute(r, id, p).unwrap_or_else(|e| panic!("{id}: {e}"))
}

fn tmp(name: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("bc-m25-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    d
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

/// Sesja z profilem „Szybki” (emulator bez opóźnień), połączona.
fn device(r: &Registry, name: &str) -> Session {
    let mut s = Session::default();
    s.machines.lock().unwrap().set_store(tmp(name)).unwrap();
    run(
        r,
        &mut s,
        "machine.profile_save",
        json!({"profile": {"name": "Szybki", "transport": {"kind": "emulator", "variant": "grbl11"}}}),
    );
    run(r, &mut s, "machine.connect", json!({"name": "Szybki"}));
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

/// Czeka, aż raport stanu pokaże głowicę w (x, y) i stan idle.
fn wait_at(r: &Registry, s: &mut Session, x: f64, y: f64) -> Value {
    let end = Instant::now() + Duration::from_secs(10);
    loop {
        let st = run(r, s, "machine.status", json!({}));
        let w = &st["state"]["wpos"];
        let there =
            (w[0].as_f64().unwrap() - x).abs() < 1e-6 && (w[1].as_f64().unwrap() - y).abs() < 1e-6;
        if there && st["state"]["state"] == "idle" || Instant::now() > end {
            return st;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn xy(v: &Value) -> [f64; 2] {
    [v[0].as_f64().unwrap(), v[1].as_f64().unwrap()]
}

fn bbox(pts: &[[f64; 2]]) -> [f64; 4] {
    let mut b = [f64::MAX, f64::MAX, f64::MIN, f64::MIN];
    for p in pts {
        b[0] = b[0].min(p[0]);
        b[1] = b[1].min(p[1]);
        b[2] = b[2].max(p[0]);
        b[3] = b[3].max(p[1]);
    }
    b
}

fn near(a: f64, b: f64, tol: f64) -> bool {
    (a - b).abs() <= tol
}

/// Pole ze znakiem łamanej zamkniętej (> 0 = przeciwnie do ruchu wskazówek zegara).
fn signed_area(pts: &[[f64; 2]]) -> f64 {
    let n = pts.len();
    (0..n)
        .map(|i| {
            let (a, b) = (pts[i], pts[(i + 1) % n]);
            a[0] * b[1] - b[0] * a[1]
        })
        .sum::<f64>()
        / 2.0
}

// ---------- podgląd zadania ----------

#[test]
fn req_09_001_preview_segments_with_times_for_playback() {
    let r = Registry::builtin();
    let mut s = Session::default();
    rect(&r, &mut s, 10.0, 10.0, 30.0, 20.0, 0);
    rect(&r, &mut s, 60.0, 10.0, 20.0, 20.0, 1);
    let spec = &r.get("plan.preview").expect("polecenie plan.preview").spec;
    assert_eq!(spec.safety_tier, SafetyTier::Read);
    assert!(!spec.undoable);
    for q in [
        "REQ-09-001",
        "REQ-09-002",
        "REQ-09-003",
        "REQ-09-004",
        "REQ-09-005",
        "REQ-09-006",
        "REQ-07-030",
    ] {
        assert!(spec.requirements.contains(&q), "{q}");
    }
    let p = run(&r, &mut s, "plan.preview", json!({}));
    let segs = p["segments"].as_array().unwrap();
    assert!(segs.len() >= 10, "segmenty: {}", segs.len());
    // czas narasta monotonicznie: suwak odtwarzania mapuje czas na segment
    let mut t = 0.0;
    for (i, g) in segs.iter().enumerate() {
        let (t0, t1) = (g["t0"].as_f64().unwrap(), g["t1"].as_f64().unwrap());
        assert!(t0 >= t - 1e-9 && t1 >= t0, "segment {i}: {t0}..{t1} po {t}");
        t = t1;
    }
    let est = run(&r, &mut s, "plan.estimate", json!({}));
    let total = p["total_s"].as_f64().unwrap();
    assert!(
        near(total, est["total_s"].as_f64().unwrap(), 1e-6),
        "{total} vs {est}"
    );
    assert!(near(t, total, 1e-6));
    // pozycja w czasie: punkt pośredni segmentu
    let mid = segs.iter().find(|g| g["kind"] == "cut").unwrap();
    assert!(mid["from"].is_array() && mid["to"].is_array());
    assert!(mid["power"].as_f64().unwrap() > 0.0);
}

#[test]
fn req_09_002_preview_shows_travel_moves() {
    let r = Registry::builtin();
    let mut s = Session::default();
    rect(&r, &mut s, 10.0, 10.0, 30.0, 20.0, 0);
    rect(&r, &mut s, 60.0, 10.0, 20.0, 20.0, 0);
    let p = run(&r, &mut s, "plan.preview", json!({}));
    let segs = p["segments"].as_array().unwrap();
    let travels: Vec<&Value> = segs.iter().filter(|g| g["kind"] == "travel").collect();
    assert!(travels.len() >= 2, "przejazdy: {}", travels.len());
    assert!(travels.iter().all(|g| g["power"].as_f64().unwrap() == 0.0));
    // przejazd między prostokątami: z 10..40 do 60..80
    assert!(travels.iter().any(|g| {
        let (a, b) = (xy(&g["from"]), xy(&g["to"]));
        (a[0] <= 40.0 + 1e-6 && b[0] >= 60.0 - 1e-6) || (b[0] <= 40.0 + 1e-6 && a[0] >= 60.0 - 1e-6)
    }));
    let len = p["travel_length"].as_f64().unwrap();
    assert!(len > 20.0, "{len}");
}

#[test]
fn req_09_003_preview_order_of_operations() {
    let r = Registry::builtin();
    let mut s = Session::default();
    let a = rect(&r, &mut s, 10.0, 10.0, 30.0, 20.0, 1);
    let b = rect(&r, &mut s, 60.0, 10.0, 20.0, 20.0, 0);
    run(&r, &mut s, "layer.set", json!({"index": 1, "passes": 2}));
    let p = run(&r, &mut s, "plan.preview", json!({}));
    let ops = p["ops"].as_array().unwrap();
    // warstwa 0 przed 1, warstwa 1 w dwóch przejściach
    let order: Vec<(u64, u64, u64)> = ops
        .iter()
        .map(|o| {
            (
                o["layer"].as_u64().unwrap(),
                o["pass"].as_u64().unwrap(),
                o["passes"].as_u64().unwrap(),
            )
        })
        .collect();
    assert_eq!(order, vec![(0, 1, 1), (1, 1, 2), (1, 2, 2)]);
    assert!(ops[0]["color"].as_str().unwrap().starts_with('#'));
    // segmenty mają numer operacji i części w kolejności cięcia
    let segs = p["segments"].as_array().unwrap();
    let mut last = 0;
    for g in segs {
        let op = g["op"].as_u64().unwrap();
        assert!(op >= last);
        last = op;
    }
    let parts = p["parts"].as_array().unwrap();
    assert_eq!(parts.len(), 3, "{parts:?}");
    assert_eq!(parts[0]["object"].as_u64().unwrap(), b);
    assert_eq!(parts[1]["object"].as_u64().unwrap(), a);
    assert_eq!(
        parts
            .iter()
            .map(|q| q["index"].as_u64().unwrap())
            .collect::<Vec<_>>(),
        vec![1, 2, 3]
    );
}

#[test]
fn req_09_004_preview_kerf_outlines_req_07_030() {
    let r = Registry::builtin();
    let mut s = Session::default();
    rect(&r, &mut s, 10.0, 10.0, 30.0, 20.0, 0);
    rect(&r, &mut s, 60.0, 10.0, 20.0, 20.0, 1);
    run(
        &r,
        &mut s,
        "layer.params",
        json!({"index": 0, "params": {"kerf": 0.5}}),
    );
    let p = run(&r, &mut s, "plan.preview", json!({}));
    let k = p["kerf"].as_array().unwrap();
    assert_eq!(k.len(), 1, "kerf tylko dla warstwy 0: {k:?}");
    assert_eq!(k[0]["layer"], 0);
    let pts: Vec<[f64; 2]> = k[0]["points"].as_array().unwrap().iter().map(xy).collect();
    let b = bbox(&pts);
    for (got, want) in b.iter().zip([9.5, 9.5, 40.5, 30.5]) {
        assert!(near(*got, want, 1e-3), "{b:?}");
    }
    assert!(k[0]["closed"].as_bool().unwrap());
    // linia środka wiązki w segmentach cięcia też jest po kerfie
    let cut: Vec<[f64; 2]> = p["segments"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|g| g["kind"] == "cut" && g["op"] == 0)
        .map(|g| xy(&g["to"]))
        .collect();
    assert!(near(bbox(&cut)[0], 9.5, 1e-3));
    assert!(
        near(p["kerf_width"][0].as_f64().unwrap(), 1.0, 1e-9),
        "szerokość cięcia warstwy 0 = 2 × kerf"
    );
}

#[test]
fn req_07_029_kerf_outlines_for_editor() {
    let r = Registry::builtin();
    let mut s = Session::default();
    let id = rect(&r, &mut s, 10.0, 10.0, 30.0, 20.0, 0);
    let empty = run(&r, &mut s, "plan.kerf", json!({}));
    assert_eq!(
        empty["outlines"].as_array().unwrap().len(),
        0,
        "bez kerfu brak obrysów"
    );
    // osobny kerf X i Y (REQ-07-027) — obrys rozszerzony osobno
    run(
        &r,
        &mut s,
        "layer.params",
        json!({"index": 0, "params": {"kerf_x": 0.4, "kerf_y": 0.2}}),
    );
    let k = run(&r, &mut s, "plan.kerf", json!({}));
    let o = &k["outlines"][0];
    assert_eq!(o["object"].as_u64().unwrap(), id);
    assert_eq!(o["layer"], 0);
    let pts: Vec<[f64; 2]> = o["points"].as_array().unwrap().iter().map(xy).collect();
    let b = bbox(&pts);
    for (got, want) in b.iter().zip([9.6, 9.8, 40.4, 30.2]) {
        assert!(near(*got, want, 1e-3), "{b:?}");
    }
    // ujemny kerf: obrys do wewnątrz
    run(
        &r,
        &mut s,
        "layer.params",
        json!({"index": 0, "params": {"kerf_x": null, "kerf_y": null, "kerf": -0.3}}),
    );
    let k = run(&r, &mut s, "plan.kerf", json!({}));
    let pts: Vec<[f64; 2]> = k["outlines"][0]["points"]
        .as_array()
        .unwrap()
        .iter()
        .map(xy)
        .collect();
    assert!(near(bbox(&pts)[0], 10.3, 1e-3));
    let spec = &r.get("plan.kerf").unwrap().spec;
    assert!(spec.requirements.contains(&"REQ-07-029"));
    assert_eq!(spec.safety_tier, SafetyTier::Read);
}

#[test]
fn req_09_005_preview_cut_direction() {
    let r = Registry::builtin();
    let mut s = Session::default();
    rect(&r, &mut s, 10.0, 10.0, 30.0, 20.0, 0);
    let area = |s: &mut Session, dir: &str| {
        run(
            &r,
            s,
            "plan.settings",
            json!({"settings": {"direction": dir}}),
        );
        let p = run(&r, s, "plan.preview", json!({}));
        let pts: Vec<[f64; 2]> = p["segments"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|g| g["kind"] == "cut")
            .map(|g| xy(&g["to"]))
            .collect();
        // kierunek strzałki = from → to każdego segmentu
        let g = &p["segments"]
            .as_array()
            .unwrap()
            .iter()
            .find(|g| g["kind"] == "cut")
            .unwrap()
            .clone();
        let d = g["dir"].as_array().unwrap();
        let (a, b) = (xy(&g["from"]), xy(&g["to"]));
        let l = ((b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2)).sqrt();
        assert!(near(d[0].as_f64().unwrap(), (b[0] - a[0]) / l, 1e-9));
        assert!(near(d[1].as_f64().unwrap(), (b[1] - a[1]) / l, 1e-9));
        signed_area(&pts)
    };
    assert!(area(&mut s, "ccw") > 0.0);
    assert!(area(&mut s, "cw") < 0.0);
}

#[test]
fn req_09_006_preview_start_points() {
    let r = Registry::builtin();
    let mut s = Session::default();
    rect(&r, &mut s, 10.0, 10.0, 30.0, 20.0, 0);
    rect(&r, &mut s, 60.0, 10.0, 20.0, 20.0, 0);
    let p = run(&r, &mut s, "plan.preview", json!({}));
    let parts = p["parts"].as_array().unwrap();
    assert_eq!(parts.len(), 2);
    let segs = p["segments"].as_array().unwrap();
    for q in parts {
        // punkt startowy = początek pierwszego segmentu cięcia części
        let i = q["segment"].as_u64().unwrap() as usize;
        assert_eq!(segs[i]["kind"], "cut");
        assert_eq!(xy(&q["start"]), xy(&segs[i]["from"]));
        assert!(i == 0 || segs[i - 1]["kind"] != "cut");
    }
}

// ---------- „rozpocznij od tego miejsca” ----------

#[test]
fn req_09_013_start_from_preview_segment() {
    let r = Registry::builtin();
    let mut s = device(&r, "from");
    rect(&r, &mut s, 10.0, 10.0, 30.0, 20.0, 0);
    rect(&r, &mut s, 60.0, 10.0, 20.0, 20.0, 0);
    run(
        &r,
        &mut s,
        "layer.set",
        json!({"index": 0, "speed": 20, "power_max": 50, "air_assist": false}),
    );
    let p = run(&r, &mut s, "plan.preview", json!({}));
    let parts = p["parts"].as_array().unwrap();
    let k = parts[1]["segment"].as_u64().unwrap();
    let from = xy(&parts[1]["start"]);
    // G-code od segmentu k: pierwszy ruch to przejazd do jego początku
    let full = run(&r, &mut s, "gcode.compile", json!({"include_text": true}));
    let part = run(
        &r,
        &mut s,
        "gcode.compile",
        json!({"include_text": true, "from_segment": k}),
    );
    assert!(part["lines"].as_u64().unwrap() < full["lines"].as_u64().unwrap());
    let text = part["text"].as_str().unwrap();
    let first_g0 = text.lines().find(|l| l.starts_with("G0")).unwrap();
    let want = format!("X{}", from[0] as i64);
    assert!(first_g0.contains(&want), "{first_g0} — oczekiwano {want}");
    // zadanie na urządzeniu od tego miejsca
    let j = run(&r, &mut s, "job.start", json!({"from_segment": k}));
    assert!(j["total"].as_u64().unwrap() > 0);
    let done = wait_done(&r, &mut s);
    assert_eq!(done["status"], "done", "{done}");
    // poza zakresem
    let e = s
        .execute(&r, "job.start", json!({"from_segment": 100000}))
        .unwrap_err();
    assert_eq!(e.code, "invalid_params");
}

// ---------- okno Laser: licznik przejść i czas ----------

#[test]
fn req_03_054_job_status_pass_counter() {
    let r = Registry::builtin();
    let mut s = device(&r, "passes");
    rect(&r, &mut s, 10.0, 10.0, 30.0, 20.0, 0);
    run(
        &r,
        &mut s,
        "layer.set",
        json!({"index": 0, "speed": 20, "power_max": 50, "air_assist": false, "passes": 5}),
    );
    run(&r, &mut s, "job.start", json!({}));
    let done = wait_done(&r, &mut s);
    assert_eq!(done["status"], "done");
    assert_eq!(done["passes"], 5, "{done}");
    assert_eq!(done["pass"], 5);
    assert_eq!(done["layer"], 0);
    assert!(done["percent"].as_f64().unwrap() >= 99.999);
    // dwie warstwy: przejścia liczone dla warstwy bieżącego wiersza
    rect(&r, &mut s, 60.0, 10.0, 10.0, 10.0, 1);
    run(
        &r,
        &mut s,
        "layer.set",
        json!({"index": 1, "speed": 20, "power_max": 50, "air_assist": false, "passes": 2}),
    );
    run(&r, &mut s, "job.start", json!({}));
    let done = wait_done(&r, &mut s);
    assert_eq!(
        (
            done["layer"].as_u64(),
            done["pass"].as_u64(),
            done["passes"].as_u64()
        ),
        (Some(1), Some(2), Some(2)),
        "{done}"
    );
    assert_eq!(done["ops"].as_u64(), Some(7), "5 + 2 przebiegi");
}

#[test]
fn req_03_053_job_time_visible_after_finish() {
    let r = Registry::builtin();
    let mut s = device(&r, "elapsed");
    rect(&r, &mut s, 10.0, 10.0, 30.0, 20.0, 0);
    run(
        &r,
        &mut s,
        "layer.set",
        json!({"index": 0, "speed": 20, "power_max": 50, "air_assist": false}),
    );
    run(&r, &mut s, "job.start", json!({}));
    let done = wait_done(&r, &mut s);
    assert_eq!(done["status"], "done");
    assert!(done["elapsed_s"].as_f64().unwrap() > 0.0);
    let f = done["elapsed"].as_str().unwrap();
    assert_eq!(f.len(), 8, "hh:mm:ss: {f}");
    // stan zostaje do następnego zadania
    std::thread::sleep(Duration::from_millis(50));
    let again = run(&r, &mut s, "job.status", json!({}));
    assert_eq!(again["elapsed"], done["elapsed"]);
    assert_eq!(again["status"], "done");
}

// ---------- położenie głowicy i wyrównanie do lasera ----------

#[test]
fn req_03_004_machine_status_head_in_document_coordinates() {
    let r = Registry::builtin();
    let mut s = device(&r, "head");
    rect(&r, &mut s, 10.0, 10.0, 30.0, 20.0, 0);
    run(&r, &mut s, "device.move_to", json!({"x": 25, "y": 15}));
    let st = wait_at(&r, &mut s, 25.0, 15.0);
    assert_eq!(xy(&st["head"]), [25.0, 15.0], "{st}");
    // start od zera użytkownika z punktem zadania w lewym dolnym rogu: głowica = róg zadania
    run(
        &r,
        &mut s,
        "job.start_settings",
        json!({"from": "user_origin", "origin": "bottom_left"}),
    );
    run(&r, &mut s, "device.move_to", json!({"x": 0, "y": 0}));
    let st = wait_at(&r, &mut s, 0.0, 0.0);
    let h = xy(&st["head"]);
    assert!(near(h[0], 10.0, 1e-6) && near(h[1], 10.0, 1e-6), "{h:?}");
    // od bieżącej pozycji — położenie w dokumencie nieokreślone
    run(&r, &mut s, "job.start_settings", json!({"from": "current"}));
    let st = run(&r, &mut s, "machine.status", json!({}));
    assert!(st["head"].is_null());
}

#[test]
fn req_04_089_align_selection_to_laser_position() {
    let r = Registry::builtin();
    let mut s = device(&r, "align");
    let id = rect(&r, &mut s, 10.0, 10.0, 30.0, 20.0, 0);
    let spec = &r
        .get("object.align_to_laser")
        .expect("object.align_to_laser")
        .spec;
    assert!(spec.undoable);
    assert!(spec.requires_device);
    assert!(spec.requirements.contains(&"REQ-04-089"));
    run(&r, &mut s, "device.move_to", json!({"x": 100, "y": 80}));
    wait_at(&r, &mut s, 100.0, 80.0);
    run(&r, &mut s, "selection.set", json!({"ids": [id]}));
    run(&r, &mut s, "object.align_to_laser", json!({}));
    let rep = run(&r, &mut s, "document.inspect", json!({}));
    let o = rep["objects"]
        .as_array()
        .unwrap()
        .iter()
        .find(|o| o["id"] == id)
        .unwrap()
        .clone();
    let b = o["bbox"].as_array().unwrap();
    let c = [
        (b[0].as_f64().unwrap() + b[2].as_f64().unwrap()) / 2.0,
        (b[1].as_f64().unwrap() + b[3].as_f64().unwrap()) / 2.0,
    ];
    assert!(near(c[0], 100.0, 1e-6) && near(c[1], 80.0, 1e-6), "{c:?}");
    // inny punkt odniesienia: lewy dolny róg
    run(
        &r,
        &mut s,
        "object.align_to_laser",
        json!({"anchor": "bottom_left"}),
    );
    let rep = run(&r, &mut s, "document.inspect", json!({}));
    let o = rep["objects"]
        .as_array()
        .unwrap()
        .iter()
        .find(|o| o["id"] == id)
        .unwrap()
        .clone();
    assert!(near(o["bbox"][0].as_f64().unwrap(), 100.0, 1e-6));
    assert!(near(o["bbox"][1].as_f64().unwrap(), 80.0, 1e-6));
    // cofnięcie
    run(&r, &mut s, "edit.undo", json!({}));
    run(&r, &mut s, "edit.undo", json!({}));
    let rep = run(&r, &mut s, "document.inspect", json!({}));
    let o = rep["objects"]
        .as_array()
        .unwrap()
        .iter()
        .find(|o| o["id"] == id)
        .unwrap()
        .clone();
    assert!(near(o["bbox"][0].as_f64().unwrap(), 10.0, 1e-6));
    // bez zaznaczenia i bez urządzenia — błąd
    run(&r, &mut s, "selection.clear", json!({}));
    assert_eq!(
        s.execute(&r, "object.align_to_laser", json!({}))
            .unwrap_err()
            .code,
        "invalid_params"
    );
    run(&r, &mut s, "selection.set", json!({"ids": [id]}));
    run(&r, &mut s, "machine.disconnect", json!({}));
    assert_eq!(
        s.execute(&r, "object.align_to_laser", json!({}))
            .unwrap_err()
            .code,
        "no_device"
    );
}

// ---------- okno Przesuń: zapamiętane pozycje ----------

#[test]
fn req_03_039_saved_named_positions() {
    let r = Registry::builtin();
    let dir = tmp("positions");
    let mut s = Session::default();
    s.machines.lock().unwrap().set_store(&dir).unwrap();
    run(&r, &mut s, "machine.connect", json!({"name": "Symulator"}));
    run(&r, &mut s, "device.move_to", json!({"x": 30, "y": 20}));
    wait_at(&r, &mut s, 30.0, 20.0);
    run(
        &r,
        &mut s,
        "machine.position_save",
        json!({"name": "Narożnik płyty"}),
    );
    run(
        &r,
        &mut s,
        "machine.position_save",
        json!({"name": "Środek", "x": 150, "y": 100}),
    );
    let l = run(&r, &mut s, "machine.positions", json!({}));
    let ps = l["positions"].as_array().unwrap();
    assert_eq!(ps.len(), 2);
    assert_eq!(ps[0]["name"], "Narożnik płyty");
    assert_eq!(
        (ps[0]["x"].as_f64().unwrap(), ps[0]["y"].as_f64().unwrap()),
        (30.0, 20.0)
    );
    // zapis w katalogu preferencji — nowa sesja widzi pozycje
    let mut s2 = Session::default();
    s2.machines.lock().unwrap().set_store(&dir).unwrap();
    assert_eq!(
        run(&r, &mut s2, "machine.positions", json!({}))["positions"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    // przejazd do pozycji z nazwą
    run(&r, &mut s, "device.position_go", json!({"name": "Środek"}));
    let st = wait_at(&r, &mut s, 150.0, 100.0);
    assert_eq!(xy(&st["state"]["wpos"]), [150.0, 100.0]);
    // nadpisanie i usunięcie
    run(
        &r,
        &mut s,
        "machine.position_save",
        json!({"name": "Środek", "x": 1, "y": 2}),
    );
    assert_eq!(
        run(&r, &mut s, "machine.positions", json!({}))["positions"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert_eq!(
        run(
            &r,
            &mut s,
            "machine.position_delete",
            json!({"name": "Środek"})
        )["removed"],
        true
    );
    assert_eq!(
        run(&r, &mut s, "machine.positions", json!({}))["positions"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        s.execute(&r, "device.position_go", json!({"name": "brak"}))
            .unwrap_err()
            .code,
        "not_found"
    );
    assert_eq!(
        s.execute(&r, "machine.position_save", json!({"name": " "}))
            .unwrap_err()
            .code,
        "invalid_params"
    );
    assert_eq!(
        r.get("device.position_go").unwrap().spec.safety_tier,
        SafetyTier::Motion
    );
    assert_eq!(
        r.get("machine.position_save").unwrap().spec.safety_tier,
        SafetyTier::Design
    );
}

// ---------- polecenia okien ----------

#[test]
fn req_03_046_037_038_window_commands_of_machine_windows() {
    let r = Registry::builtin();
    for (id, req) in [
        ("preview.open", "REQ-03-046"),
        ("preview.close", "REQ-03-046"),
        ("preview.set", "REQ-09-001"),
        ("jog.settings", "REQ-03-037"),
        ("jog.settings", "REQ-03-038"),
    ] {
        let c = r.get(id).unwrap_or_else(|| panic!("brak {id}"));
        assert_eq!(c.spec.executor, Executor::Window, "{id}");
        assert!(c.spec.requirements.contains(&req), "{id}: {req}");
        assert!(c.spec.requirements.contains(&"REQ-01-029"), "{id}");
    }
    let ps = &r.get("preview.set").unwrap().spec.params_schema["properties"];
    for k in [
        "time",
        "speed",
        "playing",
        "travel",
        "order",
        "kerf",
        "direction",
        "starts",
    ] {
        assert!(ps[k].is_object(), "preview.set: {k}");
    }
    let js = &r.get("jog.settings").unwrap().spec.params_schema["properties"];
    for k in ["step_xy", "step_z", "slow", "fast", "mode"] {
        assert!(js[k].is_object(), "jog.settings: {k}");
    }
    // nowe parametry istniejących poleceń
    assert!(
        r.get("job.start").unwrap().spec.params_schema["properties"]["from_segment"].is_object()
    );
    assert!(
        r.get("gcode.compile").unwrap().spec.params_schema["properties"]["from_segment"]
            .is_object()
    );
}

// ---------- usterki z testu sprzętowego REQ-19-017 i okno ustawień warstwy ----------

#[test]
fn req_09_008_estimate_uses_connected_machine_kinematics() {
    let r = Registry::builtin();
    let mut s = device(&r, "kin");
    rect(&r, &mut s, 10.0, 10.0, 200.0, 150.0, 0);
    run(&r, &mut s, "layer.set", json!({"index": 0, "speed": 400.0}));
    // ustawienia maszyny jak w TTS-20 PRO: $110/$111 = 30000 mm/min, $120/$121 = 1700/1300 mm/s²
    run(
        &r,
        &mut s,
        "device.settings_write",
        json!({"changes": {"$110": "30000", "$111": "30000", "$120": "1700", "$121": "1300"}}),
    );
    let dev = run(&r, &mut s, "plan.estimate", json!({}));
    assert_eq!(dev["kinematics_source"], "device");
    let gc = run(&r, &mut s, "gcode.compile", json!({}));
    // ten sam projekt bez urządzenia, kinematyka dokumentu ustawiona jak w maszynie
    let mut d = Session::default();
    rect(&r, &mut d, 10.0, 10.0, 200.0, 150.0, 0);
    run(&r, &mut d, "layer.set", json!({"index": 0, "speed": 400.0}));
    let dock = run(&r, &mut d, "plan.estimate", json!({}));
    assert_eq!(dock["kinematics_source"], "document");
    run(
        &r,
        &mut d,
        "plan.settings",
        json!({"settings": {"kinematics": {"max_speed_x": 500.0, "max_speed_y": 500.0, "accel_x": 1700.0, "accel_y": 1300.0, "junction_deviation": 0.01, "travel_speed": 500.0}}}),
    );
    let same = run(&r, &mut d, "plan.estimate", json!({}));
    let t = |v: &Value| v["total_s"].as_f64().unwrap();
    assert!(near(t(&dev), t(&same), 1e-9), "{} vs {}", t(&dev), t(&same));
    assert!(
        t(&dev) < t(&dock),
        "szybsza maszyna niż domyślna kinematyka dokumentu"
    );
    assert!(
        near(gc["time_s"].as_f64().unwrap(), t(&dev), 1e-6),
        "czas w nagłówku G-code"
    );
    let pv = run(&r, &mut s, "plan.preview", json!({}));
    assert!(near(pv["total_s"].as_f64().unwrap(), t(&dev), 1e-9));
}

#[test]
fn req_03_032_layer_settings_window_commands() {
    let r = Registry::builtin();
    for (id, req) in [
        ("layer.settings_open", "REQ-03-032"),
        ("layer.settings_close", "REQ-03-032"),
    ] {
        let c = r.get(id).unwrap_or_else(|| panic!("brak {id}"));
        assert_eq!(c.spec.executor, Executor::Window, "{id}");
        assert!(c.spec.requirements.contains(&req), "{id}: {req}");
        assert!(c.spec.requirements.contains(&"REQ-01-029"), "{id}");
    }
    let ps = &r.get("layer.settings_open").unwrap().spec.params_schema["properties"];
    assert!(ps["index"].is_object() && ps["tab"].is_object());
}
