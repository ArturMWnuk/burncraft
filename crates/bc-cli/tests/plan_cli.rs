//! Testy `burncraft-cli plan` i `burncraft-cli estimate` (REQ-14-013, REQ-14-014).

use std::process::Command;

fn cli() -> Command {
    Command::new(env!("CARGO_BIN_EXE_burncraft-cli"))
}

fn doc(n: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("bc-cli-plan-{n}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
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
fn req_14_013_cli_plan() {
    let bc = doc("plan");
    let o = cli().args(["plan", "--json"]).arg(&bc).output().unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let v: serde_json::Value = serde_json::from_slice(&o.stdout).unwrap();
    assert!(
        (v["cut_length"].as_f64().unwrap() - 100.0).abs() < 1e-3,
        "{v}"
    );
    assert!(v["warnings"].is_array());
    assert_eq!(v["hash"].as_str().unwrap().len(), 64);
    // wynik deterministyczny
    let o2 = cli().args(["plan", "--json"]).arg(&bc).output().unwrap();
    assert_eq!(o.stdout, o2.stdout);
    // tekst dla człowieka
    let t = cli().args(["plan"]).arg(&bc).output().unwrap();
    let text = String::from_utf8_lossy(&t.stdout);
    assert!(text.contains("Czas") && text.contains("Cięcie"), "{text}");
    // limit mocy z wiersza poleceń
    let o = cli()
        .args(["plan", "--json", "--power-limit", "5"])
        .arg(&bc)
        .output()
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&o.stdout).unwrap();
    assert_eq!(v["warnings"][0]["code"], "power_above_limit");
    // brak pliku = kod błędu
    assert!(!cli()
        .args(["plan", "brak.bcraft"])
        .status()
        .unwrap()
        .success());
}

#[test]
fn req_14_014_cli_estimate() {
    let bc = doc("estimate");
    let o = cli()
        .args(["estimate", "--json"])
        .arg(&bc)
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let v: serde_json::Value = serde_json::from_slice(&o.stdout).unwrap();
    let total = v["total_s"].as_f64().unwrap();
    assert!(total > 1.0, "{v}");
    assert!(
        (total
            - v["cut_s"].as_f64().unwrap()
            - v["travel_s"].as_f64().unwrap()
            - v["delay_s"].as_f64().unwrap()
            - v["dwell_s"].as_f64().unwrap())
        .abs()
            < 1e-9
    );
    let t = cli().args(["estimate"]).arg(&bc).output().unwrap();
    let text = String::from_utf8_lossy(&t.stdout);
    assert!(text.contains("Szacowany czas"), "{text}");
}
