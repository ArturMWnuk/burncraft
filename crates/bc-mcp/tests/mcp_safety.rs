//! M2.4: polityka bezpieczeństwa MCP dla urządzeń (§15.4, ADR 0005): motion po potwierdzeniu
//! użytkownika, nadrzędny stop i E-stop, utrata klienta nie przerywa zadania, watchdog joga.

mod common;

use std::time::{Duration, Instant};

use bc_commands::{Registry, SafetyTier};
use bc_drivers::driver::{JobStatus, RunState};
use bc_drivers::emu::EmuConfig;
use common::*;
use serde_json::{json, Value};

fn code_of(r: &Value) -> String {
    assert_eq!(r["isError"], true, "{r}");
    let t: Value = serde_json::from_str(r["content"][0]["text"].as_str().unwrap()).unwrap();
    t["code"].as_str().unwrap().to_string()
}

fn realtime() -> EmuConfig {
    let mut c = EmuConfig::variant("grbl11");
    c.speed = 1.0;
    c
}

/// Zadanie z dokumentu uruchomione lokalnie (GUI, bez polityki MCP): prostokąt 60×60 przy 20 mm/s.
fn start_local_job(s: &mut bc_mcp::Server, device: &str) {
    let reg = Registry::builtin();
    let b = s.workbench_mut();
    for (id, p) in [
        ("machine.select", json!({"name": device})),
        ("shape.rect", json!({"x": 10, "y": 10, "w": 60, "h": 60})),
        (
            "layer.set",
            json!({"index": 0, "speed": 20, "power_max": 30, "air_assist": false}),
        ),
        ("job.start", json!({})),
    ] {
        b.execute(&reg, id, p)
            .unwrap_or_else(|e| panic!("{id}: {e}"));
    }
}

#[test]
fn req_15_094_motion_requires_user_confirmation() {
    // klient bez elicitation: motion odrzucane, maszyna stoi
    let (s, m) = server(config("confirm-none", SafetyTier::Design));
    let _emu = attach_emu(&m, "E", EmuConfig::variant("grbl11"));
    let mut c = Stdio::start(s, json!({}));
    c.call("select_device", json!({"name": "E"}));
    let r = c.call_raw("jog", json!({"axis": "x", "distance": 4}));
    assert_eq!(code_of(&r), "confirmation_required");
    let r = c.call_raw(
        "execute",
        json!({"id": "device.move_to", "params": {"x": 3}}),
    );
    assert_eq!(code_of(&r), "confirmation_required");
    let r = c.call_raw(
        "batch",
        json!({"steps": [{"id": "device.home", "params": {}}]}),
    );
    assert_eq!(code_of(&r), "confirmation_required");
    std::thread::sleep(Duration::from_millis(200));
    assert_eq!(state_of(&m, "E").wpos, [0.0, 0.0, 0.0]);
    assert!(c.take("elicitation/create").is_empty());
    c.finish().unwrap();

    // klient z elicitation: serwer pyta użytkownika o każde polecenie motion
    let (s, m) = server(config("confirm-elicit", SafetyTier::Design));
    let _emu = attach_emu(&m, "E", EmuConfig::variant("grbl11"));
    let mut c = Stdio::start(s, json!({"elicitation": {}}));
    c.call("select_device", json!({"name": "E"}));
    let id = json!(c.next_id());
    c.send(json!({"jsonrpc": "2.0", "id": id, "method": "tools/call",
        "params": {"name": "jog", "arguments": {"axis": "x", "distance": 4}}}));
    let ask = c.wait_for("elicitation/create", WAIT, |v| {
        v["method"] == "elicitation/create"
    });
    let msg = ask["params"]["message"].as_str().unwrap();
    assert!(
        msg.contains("device.jog") && msg.contains("motion"),
        "{msg}"
    );
    assert!(ask["params"]["requestedSchema"]["properties"].is_object());
    // użytkownik potwierdza
    c.send(json!({"jsonrpc": "2.0", "id": ask["id"], "result": {"action": "accept", "content": {"confirm": true}}}));
    let r = c.wait_response(&id);
    assert_eq!(r["result"]["isError"], false, "{r}");
    wait_until("ruch po potwierdzeniu", WAIT, || {
        state_of(&m, "E").wpos[0] == 4.0
    });
    // użytkownik odmawia → odmowa, brak ruchu
    let id = json!(c.next_id());
    c.send(json!({"jsonrpc": "2.0", "id": id, "method": "tools/call",
        "params": {"name": "move_to", "arguments": {"x": 30, "y": 30}}}));
    let ask = c.wait_for("elicitation/create", WAIT, |v| {
        v["method"] == "elicitation/create"
    });
    c.send(json!({"jsonrpc": "2.0", "id": ask["id"], "result": {"action": "decline"}}));
    let r = c.wait_response(&id);
    assert_eq!(code_of(&r["result"]), "confirmation_required");
    // akceptacja bez zaznaczenia potwierdzenia też jest odmową
    let id = json!(c.next_id());
    c.send(json!({"jsonrpc": "2.0", "id": id, "method": "tools/call",
        "params": {"name": "move_to", "arguments": {"x": 30, "y": 30}}}));
    let ask = c.wait_for("elicitation/create", WAIT, |v| {
        v["method"] == "elicitation/create"
    });
    c.send(json!({"jsonrpc": "2.0", "id": ask["id"], "result": {"action": "accept", "content": {"confirm": false}}}));
    assert_eq!(
        code_of(&c.wait_response(&id)["result"]),
        "confirmation_required"
    );
    std::thread::sleep(Duration::from_millis(200));
    assert_eq!(state_of(&m, "E").wpos, [4.0, 0.0, 0.0]);
    // fire i config nie są potwierdzane przez MCP w M2 (GUI / M6): odmowa bez pytania
    let r = c.call_raw(
        "execute",
        json!({"id": "device.test_fire", "params": {"power": 1, "ms": 10}}),
    );
    assert_eq!(code_of(&r), "confirmation_required");
    assert!(c.take("elicitation/create").is_empty());
    c.finish().unwrap();

    // użytkownik dopuścił motion w konfiguracji (--allow-motion): bez pytań
    let (s, m) = server(config("confirm-config", SafetyTier::Motion));
    let _emu = attach_emu(&m, "E", EmuConfig::variant("grbl11"));
    let mut c = Stdio::start(s, json!({"elicitation": {}}));
    c.call("select_device", json!({"name": "E"}));
    c.call("jog", json!({"axis": "y", "distance": 2}));
    wait_until("jog", WAIT, || state_of(&m, "E").wpos[1] == 2.0);
    assert!(c.take("elicitation/create").is_empty());
    // audyt: odmowy i wykonania
    c.finish().unwrap();
}

#[test]
fn req_15_094_motion_confirmation_is_audited() {
    let cfg = config("confirm-audit", SafetyTier::Design);
    let log = cfg.audit_log.clone().unwrap();
    let (s, m) = server(cfg);
    let _emu = attach_emu(&m, "E", EmuConfig::variant("grbl11"));
    let mut c = Stdio::start(s, json!({}));
    c.call("select_device", json!({"name": "E"}));
    c.call_raw("jog", json!({"axis": "x", "distance": 1}));
    c.finish().unwrap();
    let text = std::fs::read_to_string(log).unwrap();
    let last: Value = serde_json::from_str(text.lines().last().unwrap()).unwrap();
    assert_eq!(last["tool"], "jog");
    assert_eq!(last["outcome"], "denied");
    assert_eq!(last["client"], "test-stdio 1");
}

#[test]
fn req_15_106_stop_job_overrides_pending_confirmation_stdio() {
    let (mut s, m) = server(config("stop-busy", SafetyTier::Design));
    let _emu = attach_emu(&m, "W", realtime());
    start_local_job(&mut s, "W");
    let mut c = Stdio::start(s, json!({"elicitation": {}}));
    wait_until("zadanie w toku", WAIT, || {
        job_of(&m, "W").status == JobStatus::Running
    });
    // serwer czeka na potwierdzenie joga (zajęty), a agent wysyła stop_job
    let jog = json!(c.next_id());
    c.send(json!({"jsonrpc": "2.0", "id": jog, "method": "tools/call",
        "params": {"name": "jog", "arguments": {"axis": "x", "distance": 5}}}));
    let ask = c.wait_for("elicitation/create", WAIT, |v| {
        v["method"] == "elicitation/create"
    });
    let t0 = Instant::now();
    let stop = json!(c.next_id());
    c.send(json!({"jsonrpc": "2.0", "id": stop, "method": "tools/call",
        "params": {"name": "stop_job", "arguments": {}}}));
    let r = c.wait_response(&stop);
    assert_eq!(r["result"]["isError"], false, "{r}");
    assert_eq!(r["result"]["structuredContent"]["stopped"], json!(["W"]));
    assert!(
        t0.elapsed() < Duration::from_secs(2),
        "stop czekał na zajęty serwer"
    );
    wait_until("zatrzymane", WAIT, || {
        job_of(&m, "W").status == JobStatus::Stopped
    });
    // pytanie o jog nadal czeka; odmowa kończy je
    c.send(json!({"jsonrpc": "2.0", "id": ask["id"], "result": {"action": "decline"}}));
    assert_eq!(
        code_of(&c.wait_response(&jog)["result"]),
        "confirmation_required"
    );
    c.finish().unwrap();
}

#[test]
fn req_15_106_stop_job_overrides_busy_server_http() {
    let (mut s, m) = server(config("stop-busy-http", SafetyTier::Design));
    let _emu = attach_emu(&m, "W", realtime());
    start_local_job(&mut s, "W");
    let h = bc_mcp::http::serve(s, 0).unwrap();
    let mut a = Http::new(h.addr);
    a.initialize(json!({"elicitation": {}}));
    wait_until("zadanie w toku", WAIT, || {
        job_of(&m, "W").status == JobStatus::Running
    });
    // sesja A: jog czeka na potwierdzenie (serwer zajęty)
    let jog_id = a.next_id();
    let mut jog = a.post(
        &json!({"jsonrpc": "2.0", "id": jog_id, "method": "tools/call",
        "params": {"name": "jog", "arguments": {"axis": "x", "distance": 5}}}),
    );
    let ask = jog.next_event(WAIT).expect("elicitation na strumieniu SSE");
    assert_eq!(ask["method"], "elicitation/create");
    // sesja B: stop_job natychmiast
    let mut b = Http::new(h.addr);
    b.initialize(json!({}));
    let t0 = Instant::now();
    let r = b.call("stop_job", json!({}));
    assert_eq!(r["stopped"], json!(["W"]));
    assert!(t0.elapsed() < Duration::from_secs(2));
    wait_until("zatrzymane", WAIT, || {
        job_of(&m, "W").status == JobStatus::Stopped
    });
    // odpowiedź użytkownika (POST) kończy oczekujący jog
    let r = a.post(&json!({"jsonrpc": "2.0", "id": ask["id"], "result": {"action": "decline"}}));
    assert_eq!(r.status, 202);
    let v = read_reply(jog, jog_id);
    assert_eq!(code_of(&v["result"]), "confirmation_required");
    h.shutdown();
}

#[test]
fn req_15_106_physical_estop_overrides_mcp() {
    // UNVERIFIED: fizyczny E-stop odwzorowany jako wejście reset sterownika (jak 0x18)
    let (s, m) = server(config("estop", SafetyTier::Fire));
    let emu = attach_emu(&m, "W", realtime());
    let mut c = Stdio::start(s, json!({}));
    c.call("select_device", json!({"name": "W"}));
    c.exec("shape.rect", json!({"x": 10, "y": 10, "w": 60, "h": 60}));
    c.exec(
        "layer.set",
        json!({"index": 0, "speed": 20, "power_max": 30, "air_assist": false}),
    );
    c.exec("job.start", json!({}));
    wait_until("ruch", WAIT, || state_of(&m, "W").state == RunState::Run);
    emu.lock().unwrap().feed(&[0x18]);
    wait_until("alarm po E-stop", WAIT, || {
        state_of(&m, "W").state == RunState::Alarm
    });
    wait_until("zadanie przerwane", WAIT, || {
        matches!(
            job_of(&m, "W").status,
            JobStatus::Failed | JobStatus::Stopped
        )
    });
    // MCP nie może wznowić zadania ani ruszyć maszyną, dopóki operator nie odblokuje
    let r = c.call_raw("resume_job", json!({}));
    assert_eq!(r["isError"], true, "{r}");
    let r = c.call_raw("jog", json!({"axis": "x", "distance": 5}));
    assert_eq!(r["isError"], true, "{r}");
    std::thread::sleep(Duration::from_millis(300));
    assert!(!matches!(
        job_of(&m, "W").status,
        JobStatus::Running | JobStatus::Paused
    ));
    assert_eq!(emu.lock().unwrap().planner_len(), 0);
    let st = c.call("device_status", json!({}));
    assert_eq!(st["state"]["state"], "alarm");
    // stop_job w alarmie też działa (bez błędu)
    c.call("stop_job", json!({}));
    c.finish().unwrap();
}

#[test]
fn req_15_107_client_loss_does_not_stop_job_stdio() {
    let (s, m) = server(config("loss-stdio", SafetyTier::Fire));
    let mut c = Stdio::start(s, json!({}));
    c.exec("machine.profile_save", profile_json("W", Some(4.0)));
    c.call("connect", json!({"name": "W"}));
    c.exec("shape.rect", json!({"x": 10, "y": 10, "w": 30, "h": 30}));
    c.exec(
        "layer.set",
        json!({"index": 0, "speed": 20, "power_max": 30, "air_assist": false}),
    );
    // ~6 s ruchu przy mnożniku 4 → ~1,5 s
    let j = c.exec("job.start", json!({}));
    assert_eq!(j["status"], "running");
    let t0 = Instant::now();
    c.close_input();
    // serwer nie kończy pracy, dopóki zadanie trwa, i nie przerywa go
    std::thread::sleep(Duration::from_millis(300));
    assert_eq!(job_of(&m, "W").status, JobStatus::Running);
    c.finish().unwrap();
    let j = job_of(&m, "W");
    assert_eq!(j.status, JobStatus::Done, "{j:?}");
    assert_eq!(j.acked, j.total);
    assert!(t0.elapsed() > Duration::from_millis(800));
}

#[test]
fn req_15_107_client_loss_does_not_stop_job_http() {
    let (s, m) = server(config("loss-http", SafetyTier::Fire));
    let h = bc_mcp::http::serve(s, 0).unwrap();
    let mut c = Http::new(h.addr);
    c.initialize(json!({}));
    c.exec("machine.profile_save", profile_json("W", Some(4.0)));
    c.call("connect", json!({"name": "W"}));
    c.exec("shape.rect", json!({"x": 10, "y": 10, "w": 30, "h": 30}));
    c.exec(
        "layer.set",
        json!({"index": 0, "speed": 20, "power_max": 30, "air_assist": false}),
    );
    c.exec("job.start", json!({}));
    // klient znika: zamknięcie sesji (DELETE), potem brak żądań
    assert!(matches!(c.delete().status, 200 | 204));
    std::thread::sleep(Duration::from_millis(300));
    assert_eq!(job_of(&m, "W").status, JobStatus::Running);
    wait_until("zadanie skończone", WAIT, || {
        job_of(&m, "W").status != JobStatus::Running
    });
    let j = job_of(&m, "W");
    assert_eq!(j.status, JobStatus::Done, "{j:?}");
    h.shutdown();
}

fn watchdog_config(name: &str) -> bc_mcp::Config {
    let mut cfg = config(name, SafetyTier::Motion);
    cfg.watchdog = Some(Duration::from_millis(300));
    cfg
}

#[test]
fn req_15_108_watchdog_stops_jog_after_stdio_client_loss() {
    let (s, m) = server(watchdog_config("wd-stdio"));
    let _emu = attach_emu(&m, "J", realtime());
    let mut c = Stdio::start(s, json!({}));
    c.call("select_device", json!({"name": "J"}));
    // 100 mm przy 10 mm/s = 10 s joga
    c.call("jog", json!({"axis": "x", "distance": 100, "speed": 10}));
    wait_until("jog", WAIT, || state_of(&m, "J").state == RunState::Jog);
    let t0 = Instant::now();
    c.finish().unwrap();
    wait_until("jog zatrzymany", Duration::from_secs(3), || {
        state_of(&m, "J").state == RunState::Idle
    });
    assert!(t0.elapsed() < Duration::from_secs(3));
    assert!(state_of(&m, "J").wpos[0] < 40.0);
}

#[test]
fn req_15_108_watchdog_stops_jog_of_silent_http_client() {
    let (s, m) = server(watchdog_config("wd-http"));
    let _emu = attach_emu(&m, "J", realtime());
    let h = bc_mcp::http::serve(s, 0).unwrap();
    let mut c = Http::new(h.addr);
    c.initialize(json!({}));
    c.call("select_device", json!({"name": "J"}));
    // aktywny klient (ping co 100 ms) — jog trwa dłużej niż limit watchdoga
    c.call("jog", json!({"axis": "x", "distance": 100, "speed": 10}));
    let t0 = Instant::now();
    while t0.elapsed() < Duration::from_millis(900) {
        c.request("ping", json!({}));
        std::thread::sleep(Duration::from_millis(100));
    }
    assert_eq!(state_of(&m, "J").state, RunState::Jog, "aktywny klient");
    // klient milknie (utrata połączenia) → watchdog anuluje jog
    let t1 = Instant::now();
    wait_until("jog zatrzymany", Duration::from_secs(3), || {
        state_of(&m, "J").state == RunState::Idle
    });
    assert!(t1.elapsed() < Duration::from_secs(3));
    assert!(state_of(&m, "J").wpos[0] < 60.0);
    // zwykły ruch (move_to) i zadania nie podlegają watchdogowi joga
    h.shutdown();
}
