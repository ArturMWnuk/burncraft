//! M2.4: narzędzia MCP urządzeń i zadań (§15.2) oraz zasoby `device/{id}/status`
//! i `job/{id}` (§15.3) — klient MCP po stdio, urządzenia na emulatorze GRBL.
//! Narzędzia wywołują polecenia rejestru z M2.3 (`machine.*`, `device.*`, `job.*`).

mod common;

use std::time::Duration;

use bc_commands::SafetyTier;
use bc_drivers::emu::EmuConfig;
use common::*;
use serde_json::{json, Value};

fn tool_def(c: &mut Stdio, name: &str) -> Value {
    let r = c.request("tools/list", json!({}));
    r["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["name"] == name)
        .unwrap_or_else(|| panic!("brak narzędzia {name}"))
        .clone()
}

/// Klient stdio z profilem „Szybki” (emulator bez opóźnień).
fn client(name: &str, tier: SafetyTier) -> (Stdio, bc_drivers::MachinesHandle) {
    let (s, m) = server(config(name, tier));
    let mut c = Stdio::start(s, json!({}));
    c.exec("machine.profile_save", profile_json("Szybki", None));
    (c, m)
}

fn wpos(c: &mut Stdio) -> Vec<f64> {
    let st = c.call("device_status", json!({}));
    st["state"]["wpos"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_f64().unwrap())
        .collect()
}

/// Czeka, aż pozycja robocza osiągnie `want` (stan odświeżany co okres odpytywania).
fn wait_pos(c: &mut Stdio, want: [f64; 3]) {
    for _ in 0..1000 {
        if wpos(c) == want.to_vec() {
            return;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(wpos(c), want.to_vec());
}

fn wait_idle(c: &mut Stdio) {
    std::thread::sleep(Duration::from_millis(300));
    for _ in 0..500 {
        if c.call("device_status", json!({}))["state"]["state"] == "idle" {
            return;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("urządzenie nie wróciło do idle");
}

fn denied_tier(r: &Value) -> String {
    assert_eq!(r["isError"], true, "{r}");
    let t: Value = serde_json::from_str(r["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(t["code"], "confirmation_required", "{t}");
    t["safety_tier"].as_str().unwrap().to_string()
}

#[test]
fn req_15_058_list_devices() {
    let (mut c, _) = client("list", SafetyTier::Design);
    let def = tool_def(&mut c, "list_devices");
    assert_eq!(def["annotations"]["readOnlyHint"], true);
    assert!(def["outputSchema"].is_object());
    let l = c.call("list_devices", json!({}));
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
    assert_eq!(l["connected"], json!([]));
    assert_eq!(l["selected"], Value::Null);
    c.finish().unwrap();
}

#[test]
fn req_15_059_060_061_connect_select_disconnect() {
    let (mut c, _) = client("conn", SafetyTier::Design);
    c.exec("machine.profile_save", profile_json("Drugi", None));
    // connect: poziom design (bez potwierdzeń), wybiera urządzenie
    let r = c.call("connect", json!({"name": "Szybki"}));
    assert_eq!(r["name"], "Szybki");
    assert_eq!(r["capabilities"]["jog"], true);
    assert_eq!(r["state"], "idle");
    c.call("connect", json!({"name": "Drugi"}));
    let l = c.call("list_devices", json!({}));
    assert_eq!(l["connected"], json!(["Drugi", "Szybki"]));
    assert_eq!(l["selected"], "Drugi");
    // select_device
    c.call("select_device", json!({"name": "Szybki"}));
    assert_eq!(c.call("list_devices", json!({}))["selected"], "Szybki");
    let e = c.call_raw("select_device", json!({"name": "Nieznane"}));
    assert_eq!(e["isError"], true);
    assert!(e["content"][0]["text"]
        .as_str()
        .unwrap()
        .contains("no_device"));
    // disconnect: domyślnie wybrane, albo wskazane
    assert_eq!(c.call("disconnect", json!({}))["removed"], true);
    let l = c.call("list_devices", json!({}));
    assert_eq!(l["connected"], json!(["Drugi"]));
    assert_eq!(l["selected"], Value::Null);
    assert_eq!(
        c.call("disconnect", json!({"device": "Drugi"}))["removed"],
        true
    );
    assert_eq!(c.call("list_devices", json!({}))["connected"], json!([]));
    c.finish().unwrap();
}

#[test]
fn req_15_062_device_status_tool() {
    let (mut c, _) = client("status", SafetyTier::Design);
    assert_eq!(
        tool_def(&mut c, "device_status")["annotations"]["readOnlyHint"],
        true
    );
    let e = c.call_raw("device_status", json!({}));
    assert_eq!(e["isError"], true);
    assert!(e["content"][0]["text"]
        .as_str()
        .unwrap()
        .contains("no_device"));
    c.call("connect", json!({"name": "Szybki"}));
    let st = c.call("device_status", json!({}));
    assert_eq!(st["device"], "Szybki");
    assert_eq!(st["state"]["state"], "idle");
    assert_eq!(st["job"]["status"], "none");
    assert_eq!(st["capabilities"]["jog"], true);
    let st = c.call("device_status", json!({"device": "Szybki"}));
    assert_eq!(st["device"], "Szybki");
    c.finish().unwrap();
}

#[test]
fn req_15_063_home_tier_motion() {
    let (s, m) = server(config("home", SafetyTier::Design));
    let mut cfg = EmuConfig::variant("grbl11");
    cfg.homing = true;
    let _emu = attach_emu(&m, "Bazowany", cfg);
    let mut c = Stdio::start(s, json!({}));
    c.call("select_device", json!({"name": "Bazowany"}));
    // bez potwierdzenia: odmowa na poziomie motion
    assert_eq!(denied_tier(&c.call_raw("home", json!({}))), "motion");
    assert_eq!(
        state_of(&m, "Bazowany").state,
        bc_drivers::driver::RunState::Alarm
    );
    c.finish().unwrap();
    // użytkownik dopuścił motion: bazowanie zdejmuje alarm
    let (s, m2) = server(config("home2", SafetyTier::Motion));
    let mut cfg = EmuConfig::variant("grbl11");
    cfg.homing = true;
    let _emu = attach_emu(&m2, "Bazowany", cfg);
    let mut c = Stdio::start(s, json!({}));
    c.call("select_device", json!({"name": "Bazowany"}));
    assert_eq!(c.call("home", json!({}))["ok"], true);
    wait_until("idle po bazowaniu", WAIT, || {
        state_of(&m2, "Bazowany").state == bc_drivers::driver::RunState::Idle
    });
    let console = c.exec("machine.console", json!({}));
    assert!(console["lines"]
        .as_array()
        .unwrap()
        .iter()
        .any(|l| l == "> $H"));
    c.finish().unwrap();
}

#[test]
fn req_15_064_jog_tier_motion() {
    let (mut c, _) = client("jog-denied", SafetyTier::Design);
    c.call("connect", json!({"name": "Szybki"}));
    assert_eq!(
        denied_tier(&c.call_raw("jog", json!({"axis": "x", "distance": 5}))),
        "motion"
    );
    assert_eq!(wpos(&mut c), vec![0.0, 0.0, 0.0]);
    c.finish().unwrap();
    let (mut c, _) = client("jog", SafetyTier::Motion);
    c.call("connect", json!({"name": "Szybki"}));
    let def = tool_def(&mut c, "jog");
    for k in ["axis", "distance", "speed"] {
        assert!(def["inputSchema"]["properties"][k].is_object(), "{k}");
    }
    assert_eq!(
        c.call("jog", json!({"axis": "x", "distance": 5, "speed": 20}))["ok"],
        true
    );
    c.call("jog", json!({"axis": "y", "distance": 3}));
    wait_pos(&mut c, [5.0, 3.0, 0.0]);
    c.finish().unwrap();
}

#[test]
fn req_15_065_move_to_tier_motion() {
    let (mut c, _) = client("move-denied", SafetyTier::Design);
    c.call("connect", json!({"name": "Szybki"}));
    assert_eq!(
        denied_tier(&c.call_raw("move_to", json!({"x": 20, "y": 15}))),
        "motion"
    );
    c.finish().unwrap();
    let (mut c, _) = client("move", SafetyTier::Motion);
    c.call("connect", json!({"name": "Szybki"}));
    c.call("move_to", json!({"x": 20, "y": 15}));
    wait_pos(&mut c, [20.0, 15.0, 0.0]);
    let e = c.call_raw("move_to", json!({}));
    assert_eq!(e["isError"], true);
    c.finish().unwrap();
}

#[test]
fn req_15_066_set_origin_tier_motion() {
    let (mut c, _) = client("origin-denied", SafetyTier::Design);
    c.call("connect", json!({"name": "Szybki"}));
    assert_eq!(denied_tier(&c.call_raw("set_origin", json!({}))), "motion");
    c.finish().unwrap();
    let (mut c, _) = client("origin", SafetyTier::Motion);
    c.call("connect", json!({"name": "Szybki"}));
    c.call("move_to", json!({"x": 20, "y": 15}));
    wait_pos(&mut c, [20.0, 15.0, 0.0]);
    c.call("set_origin", json!({}));
    wait_pos(&mut c, [0.0, 0.0, 0.0]);
    let st = c.call("device_status", json!({}));
    assert_eq!(st["state"]["wpos"], json!([0.0, 0.0, 0.0]));
    assert_eq!(st["state"]["mpos"], json!([20.0, 15.0, 0.0]));
    c.finish().unwrap();
}

#[test]
fn req_15_067_frame_tier_motion() {
    let (mut c, _) = client("frame-denied", SafetyTier::Design);
    c.call("connect", json!({"name": "Szybki"}));
    c.exec("shape.rect", json!({"x": 10, "y": 10, "w": 30, "h": 20}));
    assert_eq!(
        denied_tier(&c.call_raw("frame", json!({"kind": "rect"}))),
        "motion"
    );
    c.finish().unwrap();
    let (mut c, _) = client("frame", SafetyTier::Motion);
    c.call("connect", json!({"name": "Szybki"}));
    c.exec("shape.rect", json!({"x": 10, "y": 10, "w": 30, "h": 20}));
    let def = tool_def(&mut c, "frame");
    assert!(def["inputSchema"]["properties"]["kind"].is_object());
    assert_eq!(c.call("frame", json!({"kind": "rect"}))["points"], 4);
    assert!(
        c.call("frame", json!({"kind": "contour"}))["points"]
            .as_u64()
            .unwrap()
            >= 4
    );
    wait_idle(&mut c);
    let lines = c.exec("machine.console", json!({}))["lines"].clone();
    assert!(
        lines
            .as_array()
            .unwrap()
            .iter()
            .any(|l| l.as_str().unwrap().contains("X40 Y30")),
        "{lines}"
    );
    c.finish().unwrap();
}

/// Sesja z długim zadaniem w toku: emulator w czasie rzeczywistym, prostokąt 60×60 przy 20 mm/s.
fn running_job(name: &str, tier: SafetyTier) -> (Stdio, bc_drivers::MachinesHandle) {
    let (s, m) = server(config(name, tier));
    let mut c = Stdio::start(s, json!({}));
    c.exec("machine.profile_save", profile_json("Wolny", Some(1.0)));
    c.exec("shape.rect", json!({"x": 10, "y": 10, "w": 60, "h": 60}));
    c.exec(
        "layer.set",
        json!({"index": 0, "speed": 20, "power_max": 30, "air_assist": false}),
    );
    c.call("connect", json!({"name": "Wolny"}));
    (c, m)
}

#[test]
fn req_15_070_071_pause_resume_job() {
    // start zadania (fire) dopuszczony lokalnie przez użytkownika testu
    let (mut c, m) = running_job("pause", SafetyTier::Fire);
    assert_eq!(c.exec("job.start", json!({}))["status"], "running");
    wait_until("ruch zadania", WAIT, || {
        state_of(&m, "Wolny").state == bc_drivers::driver::RunState::Run
    });
    assert_eq!(c.call("pause_job", json!({}))["ok"], true);
    wait_until("pauza", WAIT, || {
        job_of(&m, "Wolny").status == bc_drivers::driver::JobStatus::Paused
    });
    let st = c.call("device_status", json!({}));
    assert_eq!(st["job"]["status"], "paused");
    assert_eq!(c.call("resume_job", json!({}))["ok"], true);
    wait_until("wznowienie", WAIT, || {
        job_of(&m, "Wolny").status == bc_drivers::driver::JobStatus::Running
    });
    c.call("stop_job", json!({}));
    wait_until("stop", WAIT, || {
        job_of(&m, "Wolny").status == bc_drivers::driver::JobStatus::Stopped
    });
    c.finish().unwrap();
}

#[test]
fn req_15_070_071_pause_resume_need_motion_confirmation() {
    let (s, m) = server(config("pause-denied", SafetyTier::Design));
    let mut cfg = EmuConfig::variant("grbl11");
    cfg.speed = 1.0;
    let _emu = attach_emu(&m, "Wolny", cfg);
    let mut c = Stdio::start(s, json!({}));
    c.call("select_device", json!({"name": "Wolny"}));
    assert_eq!(denied_tier(&c.call_raw("pause_job", json!({}))), "motion");
    assert_eq!(denied_tier(&c.call_raw("resume_job", json!({}))), "motion");
    c.finish().unwrap();
}

#[test]
fn req_15_072_stop_job_always_allowed() {
    // domyślna polityka: motion i fire odrzucane, stop_job — zawsze
    let (mut s, m) = server(config("stop", SafetyTier::Design));
    {
        // bez urządzeń: nic do zatrzymania, ale bez błędu
        let mut c = Stdio::start(s, json!({}));
        let def = tool_def(&mut c, "stop_job");
        assert_eq!(def["annotations"]["destructiveHint"], false);
        assert_eq!(c.call("stop_job", json!({}))["stopped"], json!([]));
        c.finish().unwrap();
        s = server(config("stop2", SafetyTier::Design)).0;
    }
    let m = {
        let _ = m;
        s.session().machines.clone()
    };
    // zadanie uruchomione w GUI (lokalny użytkownik, bez polityki MCP)
    let reg = bc_commands::Registry::builtin();
    let b = s.workbench_mut();
    for (id, p) in [
        ("machine.profile_save", profile_json("Wolny", Some(1.0))),
        ("shape.rect", json!({"x": 10, "y": 10, "w": 60, "h": 60})),
        (
            "layer.set",
            json!({"index": 0, "speed": 20, "power_max": 30}),
        ),
        ("machine.connect", json!({"name": "Wolny"})),
        ("job.start", json!({})),
    ] {
        b.execute(&reg, id, p)
            .unwrap_or_else(|e| panic!("{id}: {e}"));
    }
    let mut c = Stdio::start(s, json!({}));
    let e = c.call_raw("execute", json!({"id": "job.start", "params": {}}));
    assert_eq!(e["isError"], true, "start zadania (fire) bez potwierdzenia");
    assert_eq!(denied_tier(&c.call_raw("pause_job", json!({}))), "motion");
    assert_eq!(
        job_of(&m, "Wolny").status,
        bc_drivers::driver::JobStatus::Running
    );
    let r = c.call("stop_job", json!({}));
    assert_eq!(r["stopped"], json!(["Wolny"]));
    wait_until("zatrzymane", WAIT, || {
        job_of(&m, "Wolny").status == bc_drivers::driver::JobStatus::Stopped
    });
    // wskazane urządzenie
    assert_eq!(
        c.call("stop_job", json!({"device": "Wolny"}))["stopped"],
        json!(["Wolny"])
    );
    c.finish().unwrap();
}

#[test]
fn req_15_073_read_machine_settings() {
    let (mut c, _) = client("settings", SafetyTier::Design);
    assert_eq!(
        tool_def(&mut c, "read_machine_settings")["annotations"]["readOnlyHint"],
        true
    );
    c.call("connect", json!({"name": "Szybki"}));
    let s = c.call("read_machine_settings", json!({}));
    let list = s["settings"].as_array().unwrap();
    let laser = list.iter().find(|x| x["id"] == "$32").expect("$32");
    assert_eq!(laser["value"], "1");
    assert!(laser["name"].is_string());
    let rate = list.iter().find(|x| x["id"] == "$110").expect("$110");
    assert!(rate["unit"].is_string());
    c.finish().unwrap();
}

#[test]
fn req_15_083_device_status_resource_with_subscription() {
    let (mut c, _) = client("res-dev", SafetyTier::Motion);
    let t = c.request("resources/templates/list", json!({}));
    let uris: Vec<&str> = t["result"]["resourceTemplates"]
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x["uriTemplate"].as_str().unwrap())
        .collect();
    assert!(uris.contains(&"burncraft://device/{id}/status"), "{uris:?}");
    c.call("connect", json!({"name": "Szybki"}));
    let l = c.request("resources/list", json!({}));
    let uri = "burncraft://device/Szybki/status";
    assert!(l["result"]["resources"]
        .as_array()
        .unwrap()
        .iter()
        .any(|r| r["uri"] == uri));
    let r = c.request("resources/read", json!({"uri": uri}));
    let body: Value =
        serde_json::from_str(r["result"]["contents"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(body["device"], "Szybki");
    assert_eq!(body["state"]["state"], "idle");
    // subskrypcja: ruch zmienia stan → notifications/resources/updated
    let s = c.request("resources/subscribe", json!({"uri": uri}));
    assert!(s.get("result").is_some(), "{s}");
    c.call("move_to", json!({"x": 12, "y": 7}));
    let n = c.wait_for("resources/updated", WAIT, |v| {
        v["method"] == "notifications/resources/updated" && v["params"]["uri"] == uri
    });
    assert_eq!(n["params"]["uri"], uri);
    wait_idle(&mut c);
    // po odsubskrybowaniu — cisza
    c.request("resources/unsubscribe", json!({"uri": uri}));
    c.drain_for(Duration::from_millis(300));
    c.take("notifications/resources/updated");
    c.call("move_to", json!({"x": 1, "y": 1}));
    wait_idle(&mut c);
    c.drain_for(Duration::from_millis(400));
    assert!(c.take("notifications/resources/updated").is_empty());
    // nieznane urządzenie
    let e = c.request(
        "resources/read",
        json!({"uri": "burncraft://device/Nieznane/status"}),
    );
    assert_eq!(e["error"]["code"], -32002);
    c.finish().unwrap();
}

#[test]
fn req_15_084_job_resource_progress_and_log() {
    let (mut c, m) = running_job("res-job", SafetyTier::Fire);
    let t = c.request("resources/templates/list", json!({}));
    assert!(t["result"]["resourceTemplates"]
        .as_array()
        .unwrap()
        .iter()
        .any(|x| x["uriTemplate"] == "burncraft://job/{id}"));
    let j = c.exec("job.start", json!({"name": "ramka testowa"}));
    let id = j["id"].as_u64().unwrap();
    let uri = format!("burncraft://job/{id}");
    let l = c.request("resources/list", json!({}));
    assert!(l["result"]["resources"]
        .as_array()
        .unwrap()
        .iter()
        .any(|r| r["uri"] == uri.as_str()));
    let r = c.request("resources/read", json!({"uri": uri}));
    let body: Value =
        serde_json::from_str(r["result"]["contents"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(body["device"], "Wolny");
    assert_eq!(body["job"]["id"], id);
    assert_eq!(body["job"]["name"], "ramka testowa");
    assert!(body["job"]["total"].as_u64().unwrap() > 0);
    assert!(body["progress"].as_f64().unwrap() >= 0.0);
    // dziennik: wymiana ze sterownikiem (> wysłane, < odebrane)
    let log = body["log"].as_array().unwrap();
    assert!(
        log.iter().any(|l| {
            let l = l.as_str().unwrap();
            l.starts_with("> ") || l.starts_with("< ")
        }),
        "{log:?}"
    );
    c.call("stop_job", json!({}));
    wait_until("stop", WAIT, || {
        job_of(&m, "Wolny").status == bc_drivers::driver::JobStatus::Stopped
    });
    let r = c.request("resources/read", json!({"uri": uri}));
    let body: Value =
        serde_json::from_str(r["result"]["contents"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(body["job"]["status"], "stopped");
    let e = c.request("resources/read", json!({"uri": "burncraft://job/999999"}));
    assert_eq!(e["error"]["code"], -32002);
    c.finish().unwrap();
}
