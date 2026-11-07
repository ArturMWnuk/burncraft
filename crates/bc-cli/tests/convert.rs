//! Testy `burncraft-cli convert` i `burncraft-cli export` (REQ-14-012, REQ-14-015).

use std::process::Command;

fn cli() -> Command {
    Command::new(env!("CARGO_BIN_EXE_burncraft-cli"))
}

fn dir(n: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("bc-cli-{n}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

const SVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" width="80mm" height="40mm" viewBox="0 0 80 40"><rect x="5" y="5" width="30" height="20" fill="none" stroke="#000"/></svg>"##;

#[test]
fn req_14_012_convert_between_formats() {
    let d = dir("convert");
    let svg = d.join("in.svg");
    std::fs::write(&svg, SVG).unwrap();
    // SVG -> DXF (format z rozszerzenia)
    let dxf = d.join("out.dxf");
    let o = cli()
        .args(["convert"])
        .arg(&svg)
        .arg(&dxf)
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    assert!(std::fs::read_to_string(&dxf).unwrap().contains("AC1021"));
    // DXF -> .bcraft -> HPGL z jawnym formatem
    let bc = d.join("mid.bcraft");
    assert!(cli()
        .args(["convert"])
        .arg(&dxf)
        .arg(&bc)
        .status()
        .unwrap()
        .success());
    let out = cli().args(["open"]).arg(&bc).output().unwrap();
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["stats"]["objects"], 2, "grupa + prostokąt");
    let plt = d.join("out.txt");
    let o = cli()
        .args(["convert", "--format", "hpgl"])
        .arg(&bc)
        .arg(&plt)
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    assert!(std::fs::read_to_string(&plt).unwrap().starts_with("IN;"));
    // nieznany format -> kod błędu
    let bad = cli()
        .args(["convert"])
        .arg(&svg)
        .arg(d.join("x.docx"))
        .output()
        .unwrap();
    assert!(!bad.status.success());
}

#[test]
fn req_14_015_export_bcraft() {
    let d = dir("export");
    let svg = d.join("in.svg");
    std::fs::write(&svg, SVG).unwrap();
    let bc = d.join("doc.bcraft");
    assert!(cli()
        .args(["convert"])
        .arg(&svg)
        .arg(&bc)
        .status()
        .unwrap()
        .success());
    let out = d.join("doc.svg");
    let o = cli().args(["export"]).arg(&bc).arg(&out).output().unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let text = std::fs::read_to_string(&out).unwrap();
    assert!(text.contains("<svg") && text.contains("mm\""), "{text}");
}
