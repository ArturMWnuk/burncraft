//! Sterownik GRBL na emulatorze (ADR 0010): trait Driver, strumieniowanie, stan,
//! polecenia, ustawienia, kody błędów, odmiany.
#![allow(clippy::field_reassign_with_default)]

mod common_emu;

use std::time::{Duration, Instant};

use bc_drivers::driver::*;
use bc_drivers::emu::EmuConfig;
use bc_drivers::grbl::{codes, parse_status, GrblOptions};
use common_emu::*;

#[test]
fn req_13_001_002_003_077_connect_capabilities_status() {
    let e = emu(EmuConfig::default());
    let mut d = connect(&e);
    let c = d.capabilities();
    assert!(c.jog && c.overrides && c.probe && c.air_assist && !c.files && !c.native_raster);
    let s = d.status();
    assert_eq!(s.state, RunState::Idle);
    assert_eq!(s.variant.as_deref(), Some("grbl11"));
    assert!(s.version.as_deref().unwrap_or("").contains("1.1h"));
    assert_eq!(s.buffer, Some([15, 128]));
    assert_eq!(s.lid_open, Some(false));
    assert!(
        s.cooling.is_none() && s.temperature.is_none(),
        "GRBL nie raportuje chłodzenia"
    );
    // pokrywa otwarta → Door, lid_open
    e.lock().unwrap().set_door(true);
    std::thread::sleep(Duration::from_millis(80));
    let s = d.status();
    assert_eq!(s.state, RunState::Door);
    assert_eq!(s.lid_open, Some(true));
    e.lock().unwrap().set_door(false);
    d.disconnect();
    assert_eq!(d.status().state, RunState::Disconnected);
    assert!(matches!(d.home(), Err(DriverError::NotConnected)));
}

#[test]
fn req_13_010_075_stream_character_counting() {
    let mut cfg = EmuConfig::default();
    cfg.speed = 50.0;
    let e = emu(cfg);
    let mut d = connect(&e);
    let mut doc = rect_doc();
    for i in 0..40 {
        doc.add(
            0,
            bc_doc::Kind::Ellipse {
                cx: 60.0 + (i % 8) as f64 * 12.0,
                cy: 20.0 + (i / 8) as f64 * 12.0,
                rx: 5.0,
                ry: 5.0,
            },
        )
        .unwrap();
    }
    let f = file(&doc);
    let lines = bc_drivers::grbl::job_lines(&f).len();
    let t0 = Instant::now();
    let st = d.send_job(&f, JobOptions::default()).unwrap();
    assert_eq!(st.status, JobStatus::Running);
    assert_eq!(st.total, lines);
    // bufor wstępny: od razu wypełniony prawie cały bufor RX
    std::thread::sleep(Duration::from_millis(30));
    assert!(
        e.lock().unwrap().max_rx > 100,
        "{}",
        e.lock().unwrap().max_rx
    );
    let j = d.wait_job(Duration::from_secs(60));
    assert_eq!(j.status, JobStatus::Done, "{j:?}");
    assert_eq!(j.acked, lines);
    let em = e.lock().unwrap();
    assert!(!em.overflow, "nigdy ponad 127 B niepotwierdzonych");
    assert!(em.max_rx <= 127);
    assert!(em.errors.is_empty(), "{:?}", em.errors);
    drop(em);
    let want = bc_plan::plan(&doc, &Default::default())
        .unwrap()
        .stats()
        .cut_length;
    assert!(
        (burn_len(&e) - want).abs() < 0.05,
        "{} vs {want}",
        burn_len(&e)
    );
    let _ = t0;
}

#[test]
fn req_13_076_stream_send_response() {
    let e = emu(EmuConfig::default());
    let mut d = connect_with(
        &e,
        GrblOptions {
            stream: StreamMode::SendResponse,
            ..Default::default()
        },
    );
    let f = file(&rect_doc());
    let j = run_job(&mut d, &f);
    assert_eq!(j.status, JobStatus::Done);
    let em = e.lock().unwrap();
    // send-response: w buforze najwyżej jeden wiersz naraz
    assert!(em.max_rx <= 40, "{}", em.max_rx);
    drop(em);
    assert!((burn_len(&e) - 100.0).abs() < 1e-3);
}

#[test]
fn req_16_004_005_throughput_and_underrun_monitor() {
    // ≥ 1000 wierszy/s na emulatorze natychmiastowym
    let e = emu(EmuConfig::default());
    let mut d = connect(&e);
    let mut text = String::from("G90 G21\nM4 S0\n");
    for i in 0..4000 {
        text.push_str(&format!(
            "G1 X{} Y{} S100 F6000\n",
            i % 100,
            (i / 100) % 100
        ));
    }
    text.push_str("M5\n");
    let f = bc_drivers::MachineFile {
        driver: "grbl".into(),
        extension: "gcode",
        lines: 4003,
        bytes: text.clone().into_bytes(),
        hash: String::new(),
        time_s: 0.0,
        warnings: Vec::new(),
    };
    let t = Instant::now();
    let j = run_job(&mut d, &f);
    let secs = t.elapsed().as_secs_f64();
    assert_eq!(j.status, JobStatus::Done);
    let rate = j.acked as f64 / secs;
    assert!(rate >= 1000.0, "{rate:.0} wierszy/s");
    // monitor niedopełnienia: łącze wolniejsze od maszyny → maszyna staje w trakcie zadania
    // wiersze docierają z opóźnieniem 120 ms (zapytania `?` od razu)
    struct Slow(bc_drivers::transport::EmuTransport, Vec<(Instant, Vec<u8>)>);
    impl bc_drivers::transport::Transport for Slow {
        fn write(&mut self, d: &[u8]) -> Result<(), bc_drivers::transport::TransportError> {
            if d.contains(&b'\n') {
                self.1
                    .push((Instant::now() + Duration::from_millis(120), d.to_vec()));
                Ok(())
            } else {
                self.0.write(d)
            }
        }
        fn read(
            &mut self,
            b: &mut [u8],
            t: Duration,
        ) -> Result<usize, bc_drivers::transport::TransportError> {
            let now = Instant::now();
            while self.1.first().is_some_and(|(at, _)| *at <= now) {
                let (_, d) = self.1.remove(0);
                self.0.write(&d)?;
            }
            self.0.read(b, t)
        }
        fn describe(&self) -> String {
            "wolne łącze".into()
        }
    }
    let mut cfg = EmuConfig::default();
    cfg.speed = 1.0;
    let e2 = emu(cfg);
    let mut d2 = bc_drivers::grbl::GrblDriver::connect(
        "wolny",
        Box::new(Slow(
            bc_drivers::transport::EmuTransport::new(e2.clone()),
            Vec::new(),
        )),
        GrblOptions {
            poll_ms: 15,
            stream: StreamMode::SendResponse,
            ..Default::default()
        },
    )
    .unwrap();
    let mut slow = String::from("G90 G21\nM4 S0\n");
    for i in 0..8 {
        slow.push_str(&format!("G1 X{} S100 F600\n", (i % 2) as f64 * 0.4 + 0.4));
    }
    let f2 = bc_drivers::MachineFile {
        bytes: slow.into_bytes(),
        ..f.clone()
    };
    let j2 = run_job(&mut d2, &f2);
    assert_eq!(j2.status, JobStatus::Done);
    assert!(j2.underruns >= 1, "{j2:?}");
    assert!(j2.warnings.iter().any(|w| w.starts_with("buffer_underrun")));
    // szybkie łącze z buforem wstępnym: bez niedopełnień
    let mut cfg = EmuConfig::default();
    cfg.speed = 1.0;
    let e3 = emu(cfg);
    let mut d3 = connect(&e3);
    let mut long = String::from("G90 G21\nM4 S0\n");
    for i in 0..30 {
        long.push_str(&format!("G1 X{} S100 F3000\n", (i % 2) as f64 * 2.0 + 1.0));
    }
    let j3 = run_job(
        &mut d3,
        &bc_drivers::MachineFile {
            bytes: long.into_bytes(),
            ..f.clone()
        },
    );
    assert_eq!(j3.status, JobStatus::Done);
    assert_eq!(j3.underruns, 0, "{j3:?}");
}
#[test]
fn req_13_004_080_081_home_and_unlock() {
    let mut cfg = EmuConfig::default();
    cfg.homing = true;
    let e = emu(cfg);
    let mut d = connect(&e);
    assert_eq!(d.status().state, RunState::Alarm);
    // ruch zablokowany w alarmie
    assert!(matches!(
        d.jog(Axis::X, 5.0, 10.0),
        Err(DriverError::Alarm(..))
    ));
    d.unlock().unwrap();
    std::thread::sleep(Duration::from_millis(60));
    assert_eq!(d.status().state, RunState::Idle);
    d.move_to(Some(50.0), Some(40.0), None, None).unwrap();
    assert!(d.wait_idle(Duration::from_secs(5)));
    assert_eq!(d.status().mpos[0], 50.0);
    d.home().unwrap();
    std::thread::sleep(Duration::from_millis(60));
    assert_eq!(d.status().mpos, [0.0, 0.0, 0.0]);
    assert!(d.console().iter().any(|l| l == "> $H"));
    // bez bazowania w ustawieniach: error:5 z opisem
    let e2 = emu(EmuConfig::default());
    let mut d2 = connect(&e2);
    match d2.home() {
        Err(DriverError::Machine(5, t)) => assert!(t.contains("bazowanie")),
        other => panic!("{other:?}"),
    }
}

#[test]
fn req_13_005_006_007_079_jog_move_to_and_origin() {
    let mut cfg = EmuConfig::default();
    cfg.speed = 5.0;
    let e = emu(cfg);
    let mut d = connect(&e);
    d.jog(Axis::X, 10.0, 20.0).unwrap();
    d.jog(Axis::Y, -0.0, 20.0).unwrap();
    assert!(d.wait_idle(Duration::from_secs(10)));
    assert!((d.status().mpos[0] - 10.0).abs() < 1e-6);
    assert!(e.lock().unwrap().received.windows(4).any(|w| w == b"$J=G"));
    // jog nie zmienia trybu modalnego (G90 dalej obowiązuje)
    d.move_to(Some(5.0), None, None, Some(30.0)).unwrap();
    assert!(d.wait_idle(Duration::from_secs(10)));
    assert!((d.status().mpos[0] - 5.0).abs() < 1e-6);
    // anulowanie jog w trakcie
    d.jog(Axis::X, 200.0, 5.0).unwrap();
    std::thread::sleep(Duration::from_millis(100));
    d.jog_cancel().unwrap();
    assert!(d.wait_idle(Duration::from_secs(10)));
    let x = d.status().mpos[0];
    assert!(x > 5.0 && x < 200.0, "{x}");
    assert!(e.lock().unwrap().burns().is_empty(), "jog bez wiązki");
    // zero użytkownika w bieżącej pozycji (G10 L20 P1)
    d.set_origin(true, true, false).unwrap();
    std::thread::sleep(Duration::from_millis(80));
    let s = d.status();
    assert_eq!(s.wpos[0], 0.0);
    assert_eq!(s.wco[0], x);
    // zapytania `?` w tle: stan aktualny bez wywołań
    let polls = e
        .lock()
        .unwrap()
        .received
        .iter()
        .filter(|b| **b == b'?')
        .count();
    std::thread::sleep(Duration::from_millis(120));
    assert!(
        e.lock()
            .unwrap()
            .received
            .iter()
            .filter(|b| **b == b'?')
            .count()
            > polls
    );
    // GRBL 0.9: jog przez G91 G1
    let e9 = emu(EmuConfig::variant("grbl09"));
    let mut d9 = connect(&e9);
    assert!(!d9.capabilities().jog);
    d9.jog(Axis::Y, 7.0, 10.0).unwrap();
    assert!(d9.wait_idle(Duration::from_secs(5)));
    assert!((d9.status().mpos[1] - 7.0).abs() < 1e-6);
}

#[test]
fn req_13_011_012_pause_resume_stop() {
    let mut cfg = EmuConfig::default();
    cfg.speed = 2.0;
    let e = emu(cfg);
    let mut d = connect(&e);
    let f = file(&rect_doc());
    d.send_job(&f, JobOptions::default()).unwrap();
    std::thread::sleep(Duration::from_millis(150));
    d.pause().unwrap();
    std::thread::sleep(Duration::from_millis(100));
    assert_eq!(d.status().state, RunState::Hold);
    assert_eq!(d.job().status, JobStatus::Paused);
    let p1 = d.status().mpos;
    std::thread::sleep(Duration::from_millis(150));
    assert_eq!(d.status().mpos, p1, "w pauzie bez ruchu");
    d.resume().unwrap();
    std::thread::sleep(Duration::from_millis(100));
    assert_eq!(d.status().state, RunState::Run);
    // stop: zawsze, natychmiast — reset, wiązka wyłączona, zadanie przerwane
    d.stop();
    std::thread::sleep(Duration::from_millis(100));
    assert_eq!(d.job().status, JobStatus::Stopped);
    let em = e.lock().unwrap();
    assert!(em.resets >= 1);
    assert_eq!(em.planner_len(), 0);
    assert!(em.received.contains(&0x18) && em.received.contains(&b'!'));
    drop(em);
    let s = d.status();
    assert_eq!(s.state, RunState::Alarm, "reset w ruchu = ALARM:3");
    assert_eq!(s.alarm.as_ref().map(|a| a.0), Some(3));
    // stop działa także bez zadania i nie blokuje
    d.stop();
    d.unlock().unwrap();
}

#[test]
fn req_13_078_feed_and_power_overrides() {
    let mut cfg = EmuConfig::default();
    cfg.speed = 3.0;
    let e = emu(cfg);
    let mut d = connect(&e);
    d.set_overrides(Some(150), Some(73)).unwrap();
    std::thread::sleep(Duration::from_millis(80));
    assert_eq!(d.status().overrides, [150, 100, 73]);
    let f = file(&rect_doc());
    let j = run_job(&mut d, &f);
    assert_eq!(j.status, JobStatus::Done);
    let b = e.lock().unwrap().burns();
    assert!(
        b.iter().all(|x| (x.s - 500.0 * 0.73).abs() < 1e-6),
        "{:?}",
        b[0].s
    );
    assert!(d.set_overrides(Some(250), None).is_err());
    let e9 = emu(EmuConfig::variant("grbl09"));
    let mut d9 = connect(&e9);
    assert!(matches!(
        d9.set_overrides(Some(110), None),
        Err(DriverError::Unsupported(_))
    ));
}

#[test]
fn req_13_013_082_198_settings_backup_diff_validation() {
    let e = emu(EmuConfig::default());
    let mut d = connect(&e);
    let s = d.read_settings().unwrap();
    let s32 = s.iter().find(|x| x.id == "$32").unwrap();
    assert_eq!(s32.value, "1");
    assert_eq!(s32.name.as_deref(), Some("tryb laserowy"));
    let s110 = s.iter().find(|x| x.id == "$110").unwrap();
    assert_eq!(s110.unit.as_deref(), Some("mm/min"));
    // walidacja zakresów — nic nie zapisano
    assert!(d
        .write_settings(&[("$32".into(), "2".into())], None, false)
        .is_err());
    assert!(d
        .write_settings(&[("$110".into(), "abc".into())], None, false)
        .is_err());
    // dry run: diff bez zapisu
    let dir = std::env::temp_dir().join(format!("bc-drv-bk-{}", std::process::id()));
    let r = d
        .write_settings(
            &[("$110".into(), "8000".into()), ("$32".into(), "1".into())],
            Some(&dir),
            true,
        )
        .unwrap();
    assert!(r.dry_run && r.backup.is_none());
    assert_eq!(r.changes.len(), 1, "$32 bez zmiany");
    assert_eq!(r.changes[0].before.as_deref(), Some("6000.000"));
    assert_eq!(e.lock().unwrap().settings[&110], "6000.000");
    // zapis z kopią przed zapisem
    let r = d
        .write_settings(&[("$110".into(), "8000".into())], Some(&dir), false)
        .unwrap();
    let backup = r.backup.expect("kopia");
    let v: serde_json::Value = serde_json::from_slice(&std::fs::read(&backup).unwrap()).unwrap();
    assert_eq!(v["$110"], "6000.000", "kopia sprzed zmiany");
    assert_eq!(e.lock().unwrap().settings[&110], "8000.000");
}

#[test]
fn req_13_083_error_and_alarm_codes_with_descriptions() {
    for c in 1..=38 {
        if !(18..=19).contains(&c) {
            assert_ne!(codes::error_text(c), "nieznany błąd", "error:{c}");
        }
    }
    for c in 1..=10 {
        assert_ne!(codes::alarm_text(c), "nieznany alarm");
    }
    let e = emu(EmuConfig::default());
    let mut d = connect(&e);
    match d.raw("G99") {
        Err(DriverError::Machine(20, t)) => assert!(t.contains("nieobsługiwane")),
        other => panic!("{other:?}"),
    }
    match d.raw("G1 X10") {
        Err(DriverError::Machine(22, t)) => assert!(t.contains("posuw")),
        other => panic!("{other:?}"),
    }
    let s = d.status();
    assert!(
        s.errors.is_empty(),
        "błędy poleceń interaktywnych nie są błędami zadania"
    );
    // błąd w zadaniu przerywa zadanie
    let f = bc_drivers::MachineFile {
        driver: "grbl".into(),
        extension: "gcode",
        lines: 3,
        bytes: b"G90\nG99\nG0 X1\n".to_vec(),
        hash: String::new(),
        time_s: 0.0,
        warnings: Vec::new(),
    };
    let j = run_job(&mut d, &f);
    assert_eq!(j.status, JobStatus::Failed);
    assert_eq!(j.errors[0].1, 20);
}

#[test]
fn req_13_084_laser_mode_and_smax_check() {
    let e = emu(EmuConfig::default());
    let mut d = connect(&e);
    let chk = bc_drivers::grbl::check_laser_config(&mut d, 1000.0).unwrap();
    assert!(chk.ok, "{chk:?}");
    d.write_settings(
        &[("$32".into(), "0".into()), ("$30".into(), "255".into())],
        None,
        false,
    )
    .unwrap();
    let chk = bc_drivers::grbl::check_laser_config(&mut d, 1000.0).unwrap();
    assert!(!chk.ok);
    assert_eq!(
        chk.fixes,
        vec![
            ("$32".to_string(), "1".to_string()),
            ("$30".to_string(), "1000".to_string())
        ]
    );
    assert!(chk.problems.iter().any(|p| p.contains("$32")));
    d.write_settings(&chk.fixes, None, false).unwrap();
    assert!(
        bc_drivers::grbl::check_laser_config(&mut d, 1000.0)
            .unwrap()
            .ok
    );
}

#[test]
fn req_13_003_parse_status_formats() {
    let mut s = MachineState::default();
    assert!(parse_status("<Run|MPos:1.000,2.000,3.000|Bf:12,100|FS:600,500|Ov:110,100,90|WCO:1.000,1.000,0.000|Pn:D>", &mut s));
    assert_eq!(s.state, RunState::Run);
    assert_eq!(s.wpos, [0.0, 1.0, 3.0]);
    assert_eq!(s.buffer, Some([12, 100]));
    assert_eq!((s.feed, s.spindle), (600.0, 500.0));
    assert_eq!(s.overrides, [110, 100, 90]);
    assert_eq!(s.lid_open, Some(true));
    let mut s = MachineState::default();
    assert!(parse_status(
        "<Hold:0|WPos:5.000,6.000,0.000|FS:0,0>",
        &mut s
    ));
    assert_eq!(s.state, RunState::Hold);
    assert_eq!(s.mpos, [5.0, 6.0, 0.0]);
    let mut s = MachineState::default();
    assert!(parse_status(
        "<Idle,MPos:1.000,2.000,0.000,WPos:0.500,1.000,0.000>",
        &mut s
    ));
    assert_eq!(s.state, RunState::Idle);
    assert_eq!(s.wco, [0.5, 1.0, 0.0]);
}

#[test]
fn req_10_014_home_sets_machine_zero() {
    let mut cfg = EmuConfig::default();
    cfg.homing = true;
    let e = emu(cfg);
    let mut d = connect(&e);
    d.unlock().unwrap();
    d.move_to(Some(20.0), Some(30.0), None, None).unwrap();
    assert!(d.wait_idle(Duration::from_secs(5)));
    d.home().unwrap();
    std::thread::sleep(Duration::from_millis(60));
    let s = d.status();
    assert_eq!(s.state, RunState::Idle);
    assert_eq!(s.mpos, [0.0, 0.0, 0.0]);
}
