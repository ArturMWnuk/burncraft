//! M3.4 — API plików sterownika (REQ-13-014): lista, wysyłka, uruchomienie i usunięcie
//! pliku na karcie SD FluidNC (emulator GRBL odmiany `fluidnc`, XMODEM); sterowniki bez
//! pamięci plików (GRBL bez karty, K40 Lihuiyu, Moshi) zwracają `Unsupported`.

mod common_emu;

use std::time::Duration;

use bc_drivers::driver::*;
use bc_drivers::emu::EmuConfig;
use bc_drivers::machines::*;
use common_emu::*;

fn names(d: &mut dyn Driver) -> Vec<String> {
    d.list_files()
        .unwrap()
        .into_iter()
        .map(|f| f.name)
        .collect()
}

#[test]
fn req_13_014_fluidnc_list_upload_run_delete() {
    let e = emu(EmuConfig::variant("fluidnc"));
    let mut d = connect(&e);
    assert!(d.capabilities().files);
    assert!(d.list_files().unwrap().is_empty());
    let f = file(&rect_doc());
    d.upload_file("/sd/b.gcode", &f.bytes).unwrap();
    d.upload_file("/sd/a.gcode", b"G0 X1\n").unwrap();
    // lista posortowana po nazwie, z rozmiarem
    let list = d.list_files().unwrap();
    assert_eq!(
        list,
        vec![
            RemoteFile {
                name: "/sd/a.gcode".into(),
                size: 6
            },
            RemoteFile {
                name: "/sd/b.gcode".into(),
                size: f.bytes.len() as u64
            },
        ]
    );
    // uruchomienie: emulator wykonuje plik z karty (prostokąt 30 × 20 = 100 mm cięcia)
    d.run_file("/sd/b.gcode").unwrap();
    std::thread::sleep(Duration::from_millis(100));
    d.wait_idle(Duration::from_secs(5));
    assert!((burn_len(&e) - 100.0).abs() < 1e-3, "{}", burn_len(&e));
    // brak pliku: błąd sterownika (FluidNC error:60)
    assert!(matches!(
        d.run_file("/sd/brak.gcode"),
        Err(DriverError::Machine(60, _))
    ));
    assert!(matches!(
        d.delete_file("/sd/brak.gcode"),
        Err(DriverError::Machine(60, _))
    ));
    // usunięcie
    d.delete_file("/sd/a.gcode").unwrap();
    assert_eq!(names(&mut d), vec!["/sd/b.gcode".to_string()]);
}

#[test]
fn req_13_014_invalid_file_names_never_sent() {
    let e = emu(EmuConfig::variant("fluidnc"));
    let mut d = connect(&e);
    let before = e.lock().unwrap().received.len();
    // nazwa z końcem wiersza wstrzyknęłaby drugie polecenie ($H); pusta nazwa i znaki
    // sterujące też są odrzucane przed wysłaniem
    for bad in [
        "",
        "   ",
        "/sd/a.gcode\n$H",
        "/sd/a\r$X",
        "/sd/\u{0}x",
        "/sd/a\u{1b}b",
    ] {
        assert!(
            matches!(d.run_file(bad), Err(DriverError::Invalid(_))),
            "run {bad:?}"
        );
        assert!(
            matches!(d.delete_file(bad), Err(DriverError::Invalid(_))),
            "delete {bad:?}"
        );
        assert!(
            matches!(d.upload_file(bad, b"G0"), Err(DriverError::Invalid(_))),
            "upload {bad:?}"
        );
    }
    let long = format!("/sd/{}.gcode", "x".repeat(300));
    assert!(matches!(d.run_file(&long), Err(DriverError::Invalid(_))));
    let sent = String::from_utf8_lossy(&e.lock().unwrap().received[before..]).to_string();
    assert!(!sent.contains("$SD"), "{sent}");
    assert!(!sent.contains("$H") && !sent.contains("$X"), "{sent}");
    assert!(!sent.contains("Xmodem"), "{sent}");
}

fn assert_unsupported(d: &mut dyn Driver, who: &str) {
    assert!(!d.capabilities().files, "{who}");
    assert!(
        matches!(d.list_files(), Err(DriverError::Unsupported(_))),
        "{who}"
    );
    assert!(
        matches!(
            d.upload_file("/a.gcode", b"G0"),
            Err(DriverError::Unsupported(_))
        ),
        "{who}"
    );
    assert!(
        matches!(d.run_file("/a.gcode"), Err(DriverError::Unsupported(_))),
        "{who}"
    );
    assert!(
        matches!(d.delete_file("/a.gcode"), Err(DriverError::Unsupported(_))),
        "{who}"
    );
}

#[test]
fn req_13_014_unsupported_without_file_storage() {
    // GRBL 1.1 bez karty SD
    let e = emu(EmuConfig::default());
    let mut d = connect(&e);
    assert_unsupported(&mut d, "grbl11");
    // K40 Lihuiyu i Moshi: brak pamięci plików
    let mut m = Machines::default();
    for n in [SIMULATOR_K40, SIMULATOR_MOSHI] {
        m.connect(n).unwrap();
        let d = m.driver(n).unwrap();
        assert_unsupported(d.as_mut(), n);
    }
}
