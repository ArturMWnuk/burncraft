//! Eksport G-code z wiersza poleceń (REQ-02-090, REQ-10-019): `burncraft-cli export
//! doc.bcraft out.gcode [--driver grbl|gcode]` przez polecenie rejestru gcode.export.

use std::process::Command;

fn cli() -> Command {
    Command::new(env!("CARGO_BIN_EXE_burncraft-cli"))
}

fn dir(n: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("bc-cli-gcode-{n}-{}", std::process::id()));
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

#[test]
fn req_02_090_cli_export_gcode() {
    let d = dir("export");
    let bc = doc(&d);
    let out = d.join("out.gcode");
    let o = cli().arg("export").arg(&bc).arg(&out).output().unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let text = std::fs::read_to_string(&out).unwrap();
    assert!(
        text.contains("G21") && text.contains("M4 S0") && text.contains("G1 "),
        "{text}"
    );
    // deterministycznie
    let out2 = d.join("out2.gcode");
    cli().arg("export").arg(&bc).arg(&out2).output().unwrap();
    assert_eq!(std::fs::read(&out).unwrap(), std::fs::read(&out2).unwrap());
    // ogólny CNC
    let nc = d.join("out.nc");
    let o = cli()
        .arg("export")
        .arg(&bc)
        .arg(&nc)
        .args(["--driver", "gcode"])
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let t = std::fs::read_to_string(&nc).unwrap();
    assert!(t.contains("M3") && !t.contains("M4"), "{t}");
    // nieobsługiwany sterownik
    let o = cli()
        .arg("export")
        .arg(&bc)
        .arg(d.join("x.rd"))
        .args(["--driver", "ruida"])
        .output()
        .unwrap();
    assert!(!o.status.success());
    assert!(String::from_utf8_lossy(&o.stderr).contains("ruida"));
}
