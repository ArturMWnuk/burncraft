//! Test porównawczy z grbl-sim (REQ-19-007, ADR 0010 §4). grbl-sim jest budowany ze
//! źródeł GRBL tylko w CI na Linuksie (poza repozytorium, bez kodu GPL w produkcie);
//! ścieżkę do programu podaje zmienna `GRBL_SIM`. Lokalnie test jest pomijany (`--ignored`).
//!
//! Ten sam G-code przechodzi przez grbl-sim i przez własny emulator; porównywana jest
//! sekwencja odpowiedzi (`ok` / `error:N`) wiersz po wierszu.

use std::io::Write;
use std::process::{Command, Stdio};

use bc_drivers::emu::{EmuConfig, GrblEmulator};

const PROGRAM: &str = "\
G21 G90
G1 X5
G0 X10 Y10
G1 X20 Y10 F600
M4 S500
G1 X20 Y20
G2 X30 Y30 I10 J0
G3 X20 Y20 I-10 J0
G4 P0.1
G99
M5
M8
G1 X0 Y0 F1200
M9
G91
G1 X1 Y1
G90
G0 X0 Y0
G99
";

fn responses(text: &str) -> Vec<String> {
    text.lines()
        .map(|l| {
            l.trim()
                .trim_start_matches(|c: char| !c.is_ascii_alphanumeric())
        })
        .filter(|l| *l == "ok" || l.starts_with("error:"))
        .map(str::to_string)
        .collect()
}

fn emulator_responses(program: &str) -> Vec<String> {
    let mut e = GrblEmulator::new(EmuConfig::default());
    let _ = e.take_output();
    let mut out = Vec::new();
    for l in program.lines() {
        e.feed(format!("{l}\n").as_bytes());
        e.advance(1000.0);
        out.extend(e.take_output());
    }
    responses(&String::from_utf8_lossy(&out))
}

#[test]
fn req_19_007_emulator_responses_follow_grbl_protocol() {
    let r = emulator_responses(PROGRAM);
    assert_eq!(r.len(), PROGRAM.lines().count());
    assert_eq!(r[1], "error:22", "G1 bez posuwu");
    assert_eq!(r[9], "error:20", "nieobsługiwane G99");
    assert_eq!(
        r.last().map(String::as_str),
        Some("error:20"),
        "ostatni wiersz z błędem (dopełnienie grbl-sim nie zastąpi odpowiedzi)"
    );
    assert_eq!(r.iter().filter(|x| *x == "ok").count(), r.len() - 3);
}

#[test]
#[ignore = "wymaga grbl-sim (GRBL_SIM) — uruchamiane w CI na Linuksie"]
fn req_19_007_differential_with_grbl_sim() {
    let sim = std::env::var("GRBL_SIM").expect("GRBL_SIM: ścieżka do grbl_sim.exe");
    let tmp = std::env::temp_dir();
    // -n: odpowiedzi bez prefiksu; kroki i bloki do plików (stdout = tylko odpowiedzi GRBL)
    // stdout grbl-sim jest buforowany blokami przy potoku — stdbuf -oL (coreutils) wymusza wiersze
    let stdbuf = std::path::Path::new("/usr/bin/stdbuf");
    let mut cmd = if stdbuf.exists() {
        let mut c = Command::new(stdbuf);
        c.arg("-oL").arg(&sim);
        c
    } else {
        Command::new(&sim)
    };
    let mut child = cmd
        .args(["-n", "-s"])
        .arg(tmp.join("grbl-sim-steps.out"))
        .arg("-b")
        .arg(tmp.join("grbl-sim-blocks.out"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("uruchomienie grbl-sim");
    let mut stdin = child.stdin.take().unwrap();
    let stdout = child.stdout.take().unwrap();
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        use std::io::BufRead;
        for l in std::io::BufReader::new(stdout).lines() {
            let Ok(l) = l else { break };
            if tx.send(l).is_err() {
                break;
            }
        }
    });
    // start grbl-sim: error:7 (EEPROM), lista ustawień i powitanie — pomijane do powitania
    let mut text = String::new();
    let boot_end = std::time::Instant::now() + std::time::Duration::from_secs(20);
    while std::time::Instant::now() < boot_end {
        if let Ok(s) = rx.recv_timeout(std::time::Duration::from_millis(100)) {
            text.push_str(&s);
            text.push('\n');
            if s.contains("Grbl") {
                break;
            }
        }
    }
    let want = PROGRAM.lines().count();
    let mut theirs = Vec::new();
    // wiersz po wierszu, jak nadawca send-response
    for l in PROGRAM.lines() {
        stdin.write_all(format!("{l}\n").as_bytes()).unwrap();
        stdin.flush().unwrap();
        let end = std::time::Instant::now() + std::time::Duration::from_secs(60);
        loop {
            match rx.recv_timeout(std::time::Duration::from_millis(100)) {
                Ok(s) => {
                    text.push_str(&s);
                    text.push('\n');
                    let r = responses(&s);
                    if !r.is_empty() {
                        theirs.extend(r);
                        break;
                    }
                }
                Err(_) if std::time::Instant::now() > end => break,
                Err(_) => {}
            }
        }
    }
    // grbl-sim przetwarza ostatni wiersz dopiero po kolejnych bajtach wejścia:
    // puste wiersze dopełniające (ich odpowiedzi nie są porównywane)
    for _ in 0..3 {
        if theirs.len() >= want {
            break;
        }
        let _ = stdin.write_all(b"\n");
        let _ = stdin.flush();
        let end = std::time::Instant::now() + std::time::Duration::from_secs(30);
        while theirs.len() < want && std::time::Instant::now() < end {
            if let Ok(s) = rx.recv_timeout(std::time::Duration::from_millis(100)) {
                text.push_str(&s);
                text.push('\n');
                theirs.extend(responses(&s));
            }
        }
    }
    theirs.truncate(want);
    let _ = child.kill();
    let _ = child.wait();
    let blocks = std::fs::read_to_string(tmp.join("grbl-sim-blocks.out")).unwrap_or_default();
    assert_eq!(
        theirs.len(),
        want,
        "grbl-sim: brak odpowiedzi\n--- stdout ---\n{text}\n--- bloki ---\n{}",
        blocks.chars().take(2000).collect::<String>()
    );
    let ours = emulator_responses(PROGRAM);
    assert_eq!(
        theirs, ours,
        "grbl-sim i emulator odpowiadają inaczej\n--- grbl-sim ---\n{text}"
    );
}
