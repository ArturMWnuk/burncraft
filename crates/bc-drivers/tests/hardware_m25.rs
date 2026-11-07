//! Usterki z testu sprzętowego REQ-19-017 (Two Trees TTS-20 PRO, MKS DLC32, GRBL 1.1h,
//! 2026-11-05): pole `alarm` po wyjściu ze stanu alarmu, rozpoznanie odmiany Two Trees
//! z `$I`, obszar profilu z kreatora z `$130/$131`, kinematyka podłączonej maszyny
//! z `$110/$111/$120/$121`, nazwy rozszerzonych ustawień płyt producentów.

mod common_emu;

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use bc_drivers::driver::*;
use bc_drivers::emu::{EmuConfig, GrblEmulator};
use bc_drivers::grbl::codes;
use bc_drivers::machines::*;
use bc_drivers::transport::*;
use common_emu::*;

fn wait_state(d: &bc_drivers::grbl::GrblDriver, s: RunState) -> MachineState {
    let end = Instant::now() + Duration::from_secs(5);
    loop {
        let st = d.status();
        if st.state == s || Instant::now() > end {
            return st;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

/// Alarm (sonda bez kontaktu → ALARM:5) w emulatorze z bazowaniem.
fn alarmed() -> (SharedEmulator, bc_drivers::grbl::GrblDriver) {
    let e = emu(EmuConfig {
        homing: true,
        ..Default::default()
    });
    let mut d = connect(&e);
    d.home().unwrap();
    wait_state(&d, RunState::Idle);
    assert!(d.probe_z(2.0, 2.0).is_err());
    let st = wait_state(&d, RunState::Alarm);
    assert_eq!(st.alarm.as_ref().map(|a| a.0), Some(5));
    (e, d)
}

#[test]
fn req_13_003_alarm_cleared_after_unlock_from_console() {
    // „Odblokuj” wpisane w Konsoli ($X przez device.raw), jak na MKS DLC32
    let (_e, mut d) = alarmed();
    d.raw("$X").unwrap();
    let st = wait_state(&d, RunState::Idle);
    std::thread::sleep(Duration::from_millis(100));
    let st2 = d.status();
    assert_eq!(st.state, RunState::Idle);
    assert_eq!(
        st2.alarm, None,
        "alarm po $X i raporcie Idle: {:?}",
        st2.alarm
    );
}

#[test]
fn req_13_003_alarm_cleared_after_homing() {
    let (_e, mut d) = alarmed();
    d.home().unwrap();
    wait_state(&d, RunState::Idle);
    std::thread::sleep(Duration::from_millis(100));
    let st = d.status();
    assert_eq!(st.state, RunState::Idle);
    assert_eq!(st.alarm, None, "alarm po $H: {:?}", st.alarm);
}

#[test]
fn req_13_002_detect_two_trees_from_machine_line() {
    // odpowiedź $I lasera Two Trees TTS-20 PRO (płyta MKS DLC32)
    let welcome = "Grbl 1.1h ['$' for help]";
    let info =
        "[VER:1.1h.2023112101:]\n[OPT:VL,35,254]\n[MSG:Using machine:TTS-20 PRO_20230512]\nok";
    assert_eq!(codes::detect(welcome, info).id, "two_trees");
    for model in ["TTS-55", "TS2", "TS3"] {
        let i = format!("[VER:1.1h.20230101:]\n[MSG:Using machine:{model}_20230101]\nok");
        assert_eq!(codes::detect(welcome, &i).id, "two_trees", "{model}");
    }
    // zwykły GRBL bez znacznika producenta
    assert_eq!(
        codes::detect(welcome, "[VER:1.1h.20190825:]\n[OPT:V,15,128]\nok").id,
        "grbl11"
    );
}

/// Porty-atrapy: każdy port szeregowy to emulator z obszarem 419 × 419 mm.
struct Opener419;

impl Opener for Opener419 {
    fn open(&self, spec: &TransportSpec) -> Result<Box<dyn Transport>, TransportError> {
        match spec {
            TransportSpec::Serial { .. } => {
                let mut em = GrblEmulator::new(EmuConfig::default());
                for (k, v) in [(130, "419.000"), (131, "419.000"), (120, "1700.000")] {
                    em.settings.insert(k, v.into());
                }
                Ok(Box::new(EmuTransport::new(Arc::new(Mutex::new(em)))))
            }
            other => SystemOpener.open(other),
        }
    }
    fn ports(&self) -> Vec<PortInfo> {
        vec![PortInfo {
            port: "COM3".into(),
            vid: Some(0x1A86),
            pid: Some(0x7523),
            chip: usb_chip(0x1A86, 0x7523).map(str::to_string),
            product: None,
        }]
    }
}

#[test]
fn req_13_192_find_profile_workspace_from_max_travel() {
    let m = Machines::new(Arc::new(Opener419));
    let found = m.find(true, &[115200]);
    let p = found[0].profile.clone().expect("profil z kreatora");
    assert_eq!(p.workspace, [419.0, 419.0], "obszar z $130/$131");
}

#[test]
fn req_13_198_vendor_extended_settings_have_names() {
    let e = emu(EmuConfig::default());
    {
        let mut em = e.lock().unwrap();
        for (k, v) in [
            (28, "0.100"),
            (40, "1"),
            (45, "0"),
            (103, "80.000"),
            (113, "1000.000"),
            (123, "100.000"),
            (133, "300.000"),
        ] {
            em.settings.insert(k, v.into());
        }
    }
    let mut d = connect(&e);
    let s = d.read_settings().unwrap();
    let get = |id: &str| {
        s.iter()
            .find(|x| x.id == id)
            .unwrap_or_else(|| panic!("{id}"))
    };
    for id in ["$28", "$40", "$45", "$103", "$113", "$123", "$133"] {
        assert!(get(id).name.is_some(), "{id} bez nazwy");
    }
    assert!(get("$103").name.as_deref().unwrap().contains("oś A"));
    assert_eq!(get("$113").unit.as_deref(), Some("mm/min"));
    assert!(get("$45").name.as_deref().unwrap().contains("producenta"));
    // nieznane ustawienie producenta: bez zakresu (zapis sprawdza tylko liczbę)
    assert_eq!(get("$45").min, None);
}
