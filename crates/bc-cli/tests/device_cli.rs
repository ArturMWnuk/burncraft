//! CLI urządzeń (REQ-14-016, REQ-14-017, REQ-13-029): `send`, `status`, `--device`,
//! `emulator --tcp` (urządzenie fikcyjne w sieci).

use std::io::BufRead;
use std::process::{Command, Stdio};

fn cli() -> Command {
    Command::new(env!("CARGO_BIN_EXE_burncraft-cli"))
}

fn dir(n: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("bc-cli-dev-{n}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn doc(d: &std::path::Path) -> std::path::PathBuf {
    let svg = d.join("in.svg");
    std::fs::write(
        &svg,
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="80mm" height="40mm" viewBox="0 0 80 40"><rect x="5" y="5" width="30" height="20" fill="none" stroke="#000"/></svg>"##,
    )
    .unwrap();
    let bc = d.join("doc.bcraft");
    let o = cli().arg("convert").arg(&svg).arg(&bc).output().unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    bc
}

fn profiles(d: &std::path::Path, v: serde_json::Value) {
    std::fs::write(d.join("devices.json"), serde_json::to_vec(&v).unwrap()).unwrap();
}

#[test]
fn req_14_016_cli_send() {
    let d = dir("send");
    let bc = doc(&d);
    profiles(
        &d,
        serde_json::json!([{"name": "Emu", "transport": {"kind": "emulator", "variant": "grbl11"}}]),
    );
    let o = cli()
        .args(["send", "--json", "--device", "Emu", "--prefs-dir"])
        .arg(&d)
        .arg(&bc)
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let j: serde_json::Value = serde_json::from_slice(&o.stdout).unwrap();
    assert_eq!(j["status"], "done");
    assert!(j["acked"].as_u64().unwrap() > 5);
    // program G-code z powtórzeniem
    let g = d.join("p.gcode");
    std::fs::write(&g, "G21 G90\nM4 S0\nG0 X1 Y1\nG1 X5 S300 F900\nM5\n").unwrap();
    let o = cli()
        .args([
            "send",
            "--json",
            "--repeat",
            "2",
            "--device",
            "Emu",
            "--prefs-dir",
        ])
        .arg(&d)
        .arg(&g)
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let j: serde_json::Value = serde_json::from_slice(&o.stdout).unwrap();
    assert_eq!(
        (j["status"].as_str(), j["round"].as_u64()),
        (Some("done"), Some(2))
    );
    // błąd w programie: kod wyjścia 3 i opis
    std::fs::write(&g, "G21\nG99\n").unwrap();
    let o = cli()
        .args(["send", "--device", "Emu", "--prefs-dir"])
        .arg(&d)
        .arg(&g)
        .output()
        .unwrap();
    assert_eq!(o.status.code(), Some(3));
    assert!(String::from_utf8_lossy(&o.stdout).contains("error:20"));
}

#[test]
fn req_14_017_cli_status() {
    let d = dir("status");
    let o = cli()
        .args(["status", "--json", "--device", "Symulator", "--prefs-dir"])
        .arg(&d)
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let v: serde_json::Value = serde_json::from_slice(&o.stdout).unwrap();
    assert_eq!(v["state"]["state"], "idle");
    assert_eq!(v["state"]["variant"], "grbl11");
    let o = cli()
        .args(["status", "--device", "Symulator", "--prefs-dir"])
        .arg(&d)
        .output()
        .unwrap();
    let t = String::from_utf8_lossy(&o.stdout);
    assert!(t.contains("Symulator: idle") && t.contains("1.1h"), "{t}");
}

#[test]
fn req_13_029_cli_device_option_and_network_emulator() {
    let d = dir("device");
    // nieznane urządzenie
    let o = cli()
        .args(["status", "--device", "Nie Ma", "--prefs-dir"])
        .arg(&d)
        .output()
        .unwrap();
    assert_eq!(o.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&o.stderr).contains("Nie Ma"));
    // emulator w sieci + profil TCP z nazwą ze spacją
    let mut emu = cli()
        .args([
            "emulator",
            "--tcp",
            "127.0.0.1:0",
            "--variant",
            "grblhal",
            "--speed",
            "100",
        ])
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut line = String::new();
    std::io::BufReader::new(emu.stdout.take().unwrap())
        .read_line(&mut line)
        .unwrap();
    let addr = line.trim().rsplit(' ').next().unwrap().to_string();
    let (host, port) = addr.rsplit_once(':').unwrap();
    profiles(
        &d,
        serde_json::json!([{"name": "Laser w sieci", "transport": {"kind": "tcp", "host": host, "port": port.parse::<u16>().unwrap()}}]),
    );
    let bc = doc(&d);
    let o = cli()
        .args(["send", "--json", "--device", "Laser w sieci", "--prefs-dir"])
        .arg(&d)
        .arg(&bc)
        .output()
        .unwrap();
    let _ = emu.kill();
    let _ = emu.wait();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let j: serde_json::Value = serde_json::from_slice(&o.stdout).unwrap();
    assert_eq!(j["status"], "done");
}
