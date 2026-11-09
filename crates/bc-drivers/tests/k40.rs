//! M3.3: sterowniki K40 Lihuiyu i Moshi na emulatorach (ADR 0014).

use std::sync::{Arc, Mutex};
use std::time::Duration;

use bc_doc::{Document, Kind, LayerMode};
use bc_drivers::ch341::{Ch341, EmuUsb, Shared};
use bc_drivers::driver::{Driver, JobOptions, JobStatus};
use bc_drivers::lihuiyu::egv::{self, Steps};
use bc_drivers::lihuiyu::emu::{EmuConfig, LihuiyuEmulator};
use bc_drivers::lihuiyu::{Board, K40Settings, LihuiyuDriver, LihuiyuOptions, BED, BOARDS};
use bc_drivers::machines::{DeviceProfile, Machines, Opener};
use bc_drivers::moshi::emu::{EmuConfig as MoshiConfig, MoshiEmulator};
use bc_drivers::moshi::{proto, MoshiDriver, MoshiOptions};
use bc_drivers::transport::{PortInfo, Transport, TransportError, TransportSpec};
use bc_drivers::usb::UsbBackend;
use bc_drivers::{compile, Context, MachineFile};
use bc_plan::PlanOptions;

type Emu = Arc<Mutex<LihuiyuEmulator>>;

fn rect_doc(speed: f64, power: f64) -> Document {
    let mut d = Document::new("k40");
    d.workspace.width = BED[0];
    d.workspace.height = BED[1];
    d.add(
        0,
        Kind::Rect {
            x: 10.,
            y: 10.,
            w: 30.,
            h: 20.,
            radius: 0.,
        },
    )
    .unwrap();
    let l = d.layer_mut(0).unwrap();
    l.speed = speed;
    l.power_max = power;
    l.air_assist = false;
    // czerwony: piksele cięcia odróżnialne od szarych przejazdów w PNG
    l.color = "#ff0000".into();
    d
}

fn ctx(doc: &Document, board: Board) -> Context {
    let mut c = Context::from_document(doc);
    c.k40 = K40Settings {
        board,
        ..Default::default()
    };
    c.bed = BED;
    c
}

fn egv_file(doc: &Document, board: Board) -> MachineFile {
    let j = bc_plan::plan(doc, &PlanOptions::default()).unwrap();
    compile("lihuiyu", &j, &ctx(doc, board)).unwrap()
}

fn emu(board: Board) -> Emu {
    LihuiyuEmulator::shared(EmuConfig {
        board,
        ..Default::default()
    })
}

fn connect(e: &Emu, board: Board) -> LihuiyuDriver {
    LihuiyuDriver::connect(
        "k40",
        Box::new(Shared(e.clone())),
        LihuiyuOptions {
            settings: K40Settings {
                board,
                ..Default::default()
            },
            bed: BED,
        },
    )
    .expect("połączenie z emulatorem Lihuiyu")
}

fn run(d: &mut LihuiyuDriver, f: &MachineFile) -> JobStatus {
    d.send_job(f, JobOptions::default()).unwrap();
    d.wait_job(Duration::from_secs(30)).status
}

fn burn_mm(trace: &[bc_drivers::lihuiyu::emu::EgvMove]) -> f64 {
    trace
        .iter()
        .filter(|m| m.on)
        .map(|m| {
            (((m.to.0 - m.from.0) as f64).powi(2) + ((m.to.1 - m.from.1) as f64).powi(2)).sqrt()
                * 25.4
                / 1000.0
        })
        .sum()
}

// ---------- REQ-13-041: pakiety 30 B + CRC-8 ----------

#[test]
fn req_13_041_egv_packets_30_bytes_crc8() {
    // CRC-8/MAXIM (1-Wire): wektor kontrolny „123456789” → 0xA1
    assert_eq!(egv::crc8(b"123456789"), 0xA1);
    let f = egv_file(&rect_doc(20.0, 100.0), Board::M2);
    let ps = egv::packets(&f.bytes);
    assert!(ps.len() > 1);
    for p in &ps {
        let fr = egv::frame(&p.data);
        assert_eq!(fr.len(), 32);
        assert_eq!(fr[0], 0);
        assert_eq!(fr[31], egv::crc8(&p.data));
    }
    // ostatni pakiet: koniec zadania z czekaniem (FNSE-), dopełnienie 'F'
    let last = ps.last().unwrap();
    assert!(last.wait_finish);
    assert!(String::from_utf8_lossy(&last.data).contains("FNSE"));
    assert_eq!(*last.data.last().unwrap(), b'F');
    // emulator: pakiety ze złą sumą odrzucone (207), sterownik powtarza
    let e = emu(Board::M2);
    e.lock().unwrap().corrupt_next = 2;
    let mut d = connect(&e, Board::M2);
    assert_eq!(run(&mut d, &f), JobStatus::Done);
    let g = e.lock().unwrap();
    assert_eq!(g.crc_errors, 2);
    // + pakiet dopełnienia po ostatnim: płyta M2 Nano wykonuje pakiet dopiero po następnym
    assert_eq!(g.accepted, ps.len() + 1);
    assert!(d.console().iter().any(|l| l.contains("CRC ERROR")));
}

#[test]
fn req_13_041_identical_consecutive_packets_all_executed() {
    // test sprzętowy K40 (B4/A6): co drugi jednakowy jog („IRz139S1P”) nie ruszał głowicy —
    // płyta pomija pakiet identyczny z poprzednim; sterownik musi je rozdzielić
    use bc_drivers::driver::Axis;
    let e = emu(Board::M2);
    let mut d = connect(&e, Board::M2);
    let start = e.lock().unwrap().pos();
    for _ in 0..4 {
        d.jog(Axis::X, 10.0, 50.0).unwrap();
    }
    for _ in 0..2 {
        d.jog(Axis::Y, -10.0, 50.0).unwrap();
    }
    std::thread::sleep(Duration::from_millis(50));
    let g = e.lock().unwrap();
    let steps = |mm: f64| (mm * 1000.0 / 25.4).round() as i64;
    // płyta przejechała wszystkie 4 × 10 mm w X i 2 × 10 mm w Y (Y w dół = kroki w górę)
    assert_eq!(g.pos().0 - start.0, 4 * steps(10.0), "{:?}", g.trace);
    assert_eq!(g.pos().1 - start.1, 2 * steps(10.0), "{:?}", g.trace);
    // pozycja sterownika zgodna z płytą
    let st = d.status();
    let p = Steps::new(&K40Settings::default(), BED[1]).mm((g.pos().0 as f64, g.pos().1 as f64));
    assert!(
        (st.mpos[0] - p.x).abs() < 0.05 && (st.mpos[1] - p.y).abs() < 0.05,
        "{st:?}"
    );
    // podwójne bazowanie: oba pakiety wykonane
    drop(g);
    d.home().unwrap();
    d.home().unwrap();
    std::thread::sleep(Duration::from_millis(50));
    let g = e.lock().unwrap();
    assert_eq!(g.interp.homes, 2);
    assert_eq!(g.duplicates, 0);
}

#[test]
fn req_13_041_m2_nano_stale_status_and_late_execution() {
    // test sprzętowy K40 (dziennik k40_probe, 2026-11-09): stan tuż po zapisie jest stanem
    // sprzed zapisu, a przyjęty pakiet płyta wykonuje dopiero po następnym — każdy jog,
    // bazowanie i zadanie musi mimo to ruszyć głowicę od razu
    use bc_drivers::driver::Axis;
    let e = LihuiyuEmulator::shared(EmuConfig {
        board: Board::M2,
        m2_nano_timing: true,
        ..Default::default()
    });
    let mut d = connect(&e, Board::M2);
    let steps = |mm: f64| (mm * 1000.0 / 25.4).round() as i64;
    let start = e.lock().unwrap().pos();
    for i in 1..=4 {
        d.jog(Axis::X, 10.0, 50.0).unwrap();
        let g = e.lock().unwrap();
        assert_eq!(
            g.pos().0 - start.0,
            i * steps(10.0),
            "jog {i}: {:?}",
            g.trace
        );
    }
    for i in 1..=2 {
        d.jog(Axis::Y, -10.0, 50.0).unwrap();
        let g = e.lock().unwrap();
        assert_eq!(
            g.pos().1 - start.1,
            i * steps(10.0),
            "jog Y {i}: {:?}",
            g.trace
        );
    }
    d.home().unwrap();
    assert_eq!(e.lock().unwrap().interp.homes, 1);
    d.home().unwrap();
    assert_eq!(e.lock().unwrap().interp.homes, 2);
    // zadanie: całe wykonane, sterownik nie pisał, zanim płyta odpowiedziała
    let f = egv_file(&rect_doc(20.0, 100.0), Board::M2);
    assert_eq!(run(&mut d, &f), JobStatus::Done);
    let g = e.lock().unwrap();
    assert!(
        (burn_mm(&g.trace) - 100.0).abs() < 0.5,
        "{}",
        burn_mm(&g.trace)
    );
    assert_eq!(g.collisions, 0);
    assert_eq!(g.duplicates, 0);
}

// ---------- REQ-13-042: stany OK / BUSY / CRC / zadanie zakończone ----------

#[test]
fn req_13_042_status_ok_busy_crc_task_complete() {
    let e = LihuiyuEmulator::shared(EmuConfig {
        board: Board::M2,
        speed: 50.0,
        buffer: 60,
        ..Default::default()
    });
    let mut d = connect(&e, Board::M2);
    let f = egv_file(&rect_doc(20.0, 100.0), Board::M2);
    assert_eq!(run(&mut d, &f), JobStatus::Done);
    let mut g = e.lock().unwrap();
    // mały bufor: płyta odpowiadała BUSY, potem OK
    assert!(g.busy > 0, "brak BUSY");
    // koniec zadania: czekanie na TASK COMPLETE (236) — płyta skończyła ruchy
    assert!(!g.running());
    assert_eq!(g.status().unwrap()[1], egv::STATUS_FINISH);
    assert!((burn_mm(&g.trace) - 100.0).abs() < 0.2);
    // zła ramka → CRC ERROR
    g.write_data(&[0u8; 32][..31]).unwrap();
    assert_eq!(g.status().unwrap()[1], egv::STATUS_CRC_ERROR);
    assert_eq!(egv::status_name(egv::STATUS_BUSY), "BUSY");
}

// ---------- REQ-13-043 i warianty płyt: prędkość kodowana wg płyty ----------

#[test]
fn req_13_043_speed_code_known_value_m2() {
    // README MeerK40t: CV1410801 ≈ 12,7 mm/s (M2, przyspieszenie 1)
    let c = egv::speed_code(Board::M2, 12.7, None);
    assert!(c.text.starts_with("CV1410801"), "{}", c.text);
    // odwrotność
    let v = 141 * 256 + 80;
    assert!((egv::decode_speed(Board::M2, v, 1, false) - 12.7).abs() < 0.01);
    // poziomy przyspieszenia: wektor 4 progi, raster 3 progi
    assert_eq!(egv::accel(20.0, false), 1);
    assert_eq!(egv::accel(50.0, false), 2);
    assert_eq!(egv::accel(100.0, false), 3);
    assert_eq!(egv::accel(150.0, false), 4);
    assert_eq!(egv::accel(100.0, true), 2);
    assert_eq!(egv::accel(200.0, true), 3);
    assert_eq!(egv::accel(400.0, true), 4);
    // sufiks C poniżej 7 mm/s na M2, nie na A
    assert!(egv::speed_code(Board::M2, 5.0, None).text.ends_with('C'));
    assert!(!egv::speed_code(Board::A, 5.0, None).text.ends_with('C'));
}

/// Zadanie na emulatorze danej płyty: emulator odczytuje z kodu tę samą prędkość.
fn board_roundtrip(board: Board) {
    for speed in [3.0, 15.0, 45.0, 90.0, 180.0] {
        let code = egv::speed_code(board, speed, None);
        let doc = rect_doc(speed, 100.0);
        let f = egv_file(&doc, board);
        let e = emu(board);
        let mut d = connect(&e, board);
        assert_eq!(run(&mut d, &f), JobStatus::Done, "{board:?} {speed}");
        let g = e.lock().unwrap();
        let got = g.interp.speeds[0];
        let want = code.speed;
        assert!(
            ((got - want) / want).abs() < 0.01,
            "{board:?}: {speed} mm/s → emulator {got}"
        );
        if !code.clamped {
            assert!(((want - speed) / speed).abs() < 1e-9);
        }
        assert!((burn_mm(&g.trace) - 100.0).abs() < 0.2, "{board:?}");
        // człon przekątnej: brak dla A, B, M
        let diag = !matches!(board, Board::A | Board::B | Board::M);
        assert_eq!(board.diagonal(), diag);
    }
}

#[test]
fn req_13_031_board_m3_nano() {
    board_roundtrip(Board::M3);
    assert!(Board::M3.pwm());
    // M3: moc PWM poleceniem AT przed operacją
    let f = egv_file(&rect_doc(20.0, 40.0), Board::M3);
    let e = emu(Board::M3);
    let mut d = connect(&e, Board::M3);
    assert!(d.capabilities().pwm);
    assert_eq!(run(&mut d, &f), JobStatus::Done);
    assert!((e.lock().unwrap().power.unwrap() - 40.0).abs() < 0.2);
}

#[test]
fn req_13_032_board_m1() {
    board_roundtrip(Board::M1);
}

#[test]
fn req_13_033_board_m() {
    board_roundtrip(Board::M);
    assert!(
        !egv::speed_code(Board::M, 20.0, None).text[9..].starts_with(|c: char| c.is_ascii_digit())
    );
}

#[test]
fn req_13_034_board_b1() {
    board_roundtrip(Board::B1);
}

#[test]
fn req_13_035_board_b2() {
    board_roundtrip(Board::B2);
    // B2: podwojone M równania (ta sama prędkość → inna wartość niż M2)
    assert_ne!(
        egv::speed_code(Board::B2, 20.0, None).text[2..8],
        egv::speed_code(Board::M2, 20.0, None).text[2..8]
    );
}

#[test]
fn req_13_036_board_b() {
    board_roundtrip(Board::B);
}

#[test]
fn req_13_037_board_a() {
    board_roundtrip(Board::A);
}

#[test]
fn req_13_038_board_6c6879_laser_m2_m3() {
    board_roundtrip(Board::L6c6879M2);
    board_roundtrip(Board::L6c6879M3);
    assert_eq!(Board::L6c6879M2.name(), "6C6879-LASER-M2");
    assert_eq!(Board::L6c6879M3.name(), "6C6879-LASER-M3");
    assert_eq!(BOARDS.len(), 10);
}

#[test]
fn req_13_043_direction_step_commands() {
    // kierunki B/T/R/L, przekątna M, odległości a–y, |a–|z, 3 cyfry, z = 255
    assert_eq!(egv::distance(26), b"|a");
    assert_eq!(egv::distance(300), b"z|t");
    let mut e = egv::Egv::new((0, 0));
    e.enter("CV1410801", (0, 0), false);
    e.line((100, 0), true);
    e.line((100, 50), true);
    e.line((50, 100), true);
    e.leave(true);
    let s = String::from_utf8(e.out.clone()).unwrap();
    assert!(s.starts_with("ICV1410801NBRS1E"), "{s}");
    assert!(s.contains("BD100"), "{s}");
    assert!(s.contains("R|y"), "{s}");
    assert!(s.contains("TM|y"), "{s}");
    assert!(s.ends_with("FNSE-\n"));
    // emulator wykonuje te same ruchy
    let mut i = bc_drivers::lihuiyu::emu::Interp::new(Board::M2);
    i.feed(&e.out);
    let ends: Vec<_> = i.moves.iter().map(|m| m.to).collect();
    assert_eq!(ends, vec![(100, 0), (100, 50), (50, 100)]);
    // linia ukośna nie pod 45°: odcinki osiowe i przekątne, koniec dokładny
    let mut e = egv::Egv::new((0, 0));
    e.enter("CV1410801", (0, 0), false);
    e.line((97, -31), true);
    e.leave(true);
    let mut i = bc_drivers::lihuiyu::emu::Interp::new(Board::M2);
    i.feed(&e.out);
    assert_eq!(i.pos, (97, -31));
    assert!(i.moves.iter().all(|m| m.on));
}

// ---------- REQ-13-044: raster natywny „step mode” ----------

fn fill_doc() -> Document {
    let mut d = rect_doc(100.0, 100.0);
    let l = d.layer_mut(0).unwrap();
    l.mode = LayerMode::Fill;
    l.params.interval = 0.254;
    l.params.bidirectional = true;
    d
}

#[test]
fn req_13_044_native_raster_step_mode() {
    let doc = fill_doc();
    let f = egv_file(&doc, Board::M2);
    let s = String::from_utf8_lossy(&f.bytes).to_string();
    // kod rastra: V + 6 cyfr + przyspieszenie + G010 (krok 10 kroków = 0,254 mm)
    assert!(s.contains("G010"), "{}", &s[..80.min(s.len())]);
    // jedno wejście w program: bez przejazdów między liniami (płyta sama robi krok)
    assert_eq!(s.matches("S1E").count(), 1);
    let e = emu(Board::M2);
    let mut d = connect(&e, Board::M2);
    assert_eq!(run(&mut d, &f), JobStatus::Done);
    let g = e.lock().unwrap();
    assert_eq!(g.interp.raster_step, 10);
    // ~79 linii po 30 mm
    let lines = (20.0f64 / 0.254).round();
    let burn = burn_mm(&g.trace);
    assert!((burn - lines * 30.0).abs() < 31.0, "{burn}");
    // PNG toru emulatora ≈ podgląd planera (REQ-19-012)
    let job = bc_plan::plan(&doc, &PlanOptions::default()).unwrap();
    let ej = g.trace_job(&Steps::new(&K40Settings::default(), BED[1]));
    assert!(png_diff(&doc, &job, &ej) < 0.01);
}

fn png_diff(doc: &Document, a: &bc_plan::Job, b: &bc_plan::Job) -> f64 {
    let opt = bc_sim::PreviewOptions {
        bbox: Some(bc_geom::Rect::new(0.0, 0.0, 60.0, 40.0)),
        max_size: 300,
        show_workspace: false,
    };
    let pa = bc_sim::render_toolpath(doc, a, &opt).unwrap();
    let pb = bc_sim::render_toolpath(doc, b, &opt).unwrap();
    assert_eq!((pa.width, pa.height), (pb.width, pb.height));
    if let Ok(dir) = std::env::var("BC_K40_PNG") {
        static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let n = N.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        std::fs::write(format!("{dir}/plan{n}.png"), &pa.png).unwrap();
        std::fs::write(format!("{dir}/emu{n}.png"), &pb.png).unwrap();
    }
    let x = tiny_skia::Pixmap::decode_png(&pa.png).unwrap();
    let y = tiny_skia::Pixmap::decode_png(&pb.png).unwrap();
    // porównanie pikseli cięcia (czerwone); przejazdy różnią się startem z narożnika
    // bazowania K40 i podziałem na ruchy szybkie / bez wiązki
    let burn = |p: &[u8]| p[0] > 150 && p[1] < 110 && p[2] < 110;
    let diff = x
        .data()
        .chunks(4)
        .zip(y.data().chunks(4))
        .filter(|(p, q)| burn(p) != burn(q))
        .count();
    diff as f64 / (pa.width * pa.height) as f64
}

// ---------- REQ-13-045: brak PWM ----------

#[test]
fn req_13_045_no_pwm_power_ignored_warning() {
    let f = egv_file(&rect_doc(20.0, 40.0), Board::M2);
    assert!(f.warnings.iter().any(|w| w.code == "power_ignored"));
    assert!(!Board::M2.pwm());
    let e = emu(Board::M2);
    let d = connect(&e, Board::M2);
    assert!(!d.capabilities().pwm);
    // profil bez połączenia też mówi o braku PWM (GUI wyłącza pole mocy)
    assert!(!DeviceProfile::simulator_k40().pwm());
    assert!(DeviceProfile::simulator().pwm());
    // dither: zmienna moc w rastrze → wiązka włączana na części kroków
    let mut doc = fill_doc();
    doc.layer_mut(0).unwrap().power_max = 100.0;
    let job = bc_plan::plan(&doc, &PlanOptions::default()).unwrap();
    let mut job_half = job.clone();
    for op in &mut job_half.ops {
        for m in &mut op.moves {
            if let bc_plan::Move::Cut { power, .. } = m {
                if *power > 0.0 {
                    *power = if *power > 99.0 { 100.0 } else { *power };
                }
            }
        }
        // pierwsza połowa linii z mocą 50 %
        let n = op.moves.len();
        for m in op.moves.iter_mut().take(n / 2) {
            if let bc_plan::Move::Cut { power, .. } = m {
                if *power > 0.0 {
                    *power = 50.0;
                }
            }
        }
    }
    let f = compile("lihuiyu", &job_half, &ctx(&doc, Board::M2)).unwrap();
    let e = emu(Board::M2);
    let mut d = connect(&e, Board::M2);
    assert_eq!(run(&mut d, &f), JobStatus::Done);
    let full = 79.0 * 30.0;
    let burn = burn_mm(&e.lock().unwrap().trace);
    assert!(burn > full * 0.6 && burn < full * 0.9, "{burn}");
}

// ---------- REQ-13-046: wariant i ID płyty ----------

#[test]
fn req_13_046_board_variant_and_id() {
    let e = LihuiyuEmulator::shared(EmuConfig {
        board: Board::B2,
        serial: Some("ab12cd34".into()),
        ..Default::default()
    });
    let opts = |id: &str| LihuiyuOptions {
        settings: K40Settings {
            board: Board::B2,
            board_id: Some(id.into()),
            ..Default::default()
        },
        bed: BED,
    };
    let d = LihuiyuDriver::connect("k40", Box::new(Shared(e.clone())), opts("AB12CD34")).unwrap();
    assert!(e.lock().unwrap().serial_ok);
    let st = d.status();
    assert_eq!(st.variant.as_deref(), Some("b2"));
    assert_eq!(st.version.as_deref(), Some("B2"));
    drop(d);
    let e2 = LihuiyuEmulator::shared(EmuConfig {
        board: Board::B2,
        serial: Some("ab12cd34".into()),
        ..Default::default()
    });
    let err = LihuiyuDriver::connect("k40", Box::new(Shared(e2)), opts("ZZZ"))
        .err()
        .unwrap();
    assert!(err.to_string().contains("ID płyty"), "{err}");
    // profil: wariant i ID w devices.json
    let p: DeviceProfile = serde_json::from_value(serde_json::json!({
        "name": "Mój K40", "driver": "lihuiyu",
        "transport": {"kind": "usb", "vid": 0x1a86, "pid": 0x5512},
        "workspace": [310, 210],
        "k40": {"board": "6c6879_m3", "board_id": "A1B2C3"}
    }))
    .unwrap();
    p.validate().unwrap();
    assert_eq!(p.k40_settings().board, Board::L6c6879M3);
    assert!(p.pwm());
    let mut bad = p.clone();
    bad.transport = TransportSpec::Serial {
        port: "COM3".into(),
        baud: 115200,
    };
    assert!(bad.validate().is_err());
}

// ---------- REQ-13-047: kroki/mm (1000 dpi) i korekcja skali ----------

#[test]
fn req_13_047_steps_per_mm_and_scale() {
    let st = Steps::new(&K40Settings::default(), BED[1]);
    // 1000 dpi: 25,4 mm = 1000 kroków; Y w dół od lewego górnego rogu
    assert_eq!(st.of(bc_geom::Point::new(25.4, BED[1])), (1000, 0));
    assert_eq!(st.of(bc_geom::Point::new(0.0, BED[1] - 50.8)), (0, 2000));
    let doc = rect_doc(30.0, 100.0);
    let j = bc_plan::plan(&doc, &PlanOptions::default()).unwrap();
    let mut c = ctx(&doc, Board::M2);
    c.k40.scale = [1.02, 0.98];
    let f = compile("lihuiyu", &j, &c).unwrap();
    let e = emu(Board::M2);
    let mut d = LihuiyuDriver::connect(
        "k40",
        Box::new(Shared(e.clone())),
        LihuiyuOptions {
            settings: c.k40.clone(),
            bed: BED,
        },
    )
    .unwrap();
    assert_eq!(run(&mut d, &f), JobStatus::Done);
    // prostokąt 30 × 20 mm: w krokach 30·1,02 i 20·0,98 cala/1000
    let g = e.lock().unwrap();
    let xs: Vec<i64> = g.trace.iter().filter(|m| m.on).map(|m| m.to.0).collect();
    let ys: Vec<i64> = g.trace.iter().filter(|m| m.on).map(|m| m.to.1).collect();
    let w = xs.iter().max().unwrap() - xs.iter().min().unwrap();
    let h = ys.iter().max().unwrap() - ys.iter().min().unwrap();
    assert!((w as f64 - 30.0 / 25.4 * 1000.0 * 1.02).abs() <= 1.0, "{w}");
    assert!((h as f64 - 20.0 / 25.4 * 1000.0 * 0.98).abs() <= 1.0, "{h}");
    // zakresy w profilu
    let k = K40Settings {
        scale: [3.0, 1.0],
        ..Default::default()
    };
    assert!(k.validate().is_err());
}

// ---------- REQ-13-048: bazowanie, przejazd szybki vs powolny, pauza ----------

#[test]
fn req_13_048_home_rapid_slow_pause_resume() {
    let e = emu(Board::M2);
    let mut d = connect(&e, Board::M2);
    d.move_to(Some(50.0), Some(100.0), None, None).unwrap();
    d.move_to(Some(60.0), Some(100.0), None, Some(10.0))
        .unwrap();
    {
        let g = e.lock().unwrap();
        let t = &g.trace;
        assert!(t[0].rapid, "przejazd szybki");
        assert!(
            !t.last().unwrap().rapid && !t.last().unwrap().on,
            "ruch powolny bez wiązki"
        );
        assert!((t.last().unwrap().speed - 10.0).abs() < 0.1);
    }
    let p = d.status().wpos;
    assert!(
        (p[0] - 60.0).abs() < 0.05 && (p[1] - 100.0).abs() < 0.05,
        "{p:?}"
    );
    d.home().unwrap();
    assert_eq!(e.lock().unwrap().interp.homes, 1);
    let p = d.status().mpos;
    assert!(p[0].abs() < 1e-9 && (p[1] - BED[1]).abs() < 1e-9);
    // pauza i wznowienie w trakcie zadania (emulator w czasie rzeczywistym ×5)
    let slow = LihuiyuEmulator::shared(EmuConfig {
        board: Board::M2,
        speed: 5.0,
        buffer: 90,
        ..Default::default()
    });
    let mut d = connect(&slow, Board::M2);
    let f = egv_file(&rect_doc(20.0, 100.0), Board::M2);
    d.send_job(&f, JobOptions::default()).unwrap();
    std::thread::sleep(Duration::from_millis(100));
    d.pause().unwrap();
    let t0 = std::time::Instant::now();
    while d.job().status != JobStatus::Paused && t0.elapsed() < Duration::from_secs(2) {
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(d.job().status, JobStatus::Paused);
    // wstrzymana płyta po przyjętych danych przestaje się ruszać
    let t0 = std::time::Instant::now();
    while !slow.lock().unwrap().interp.paused && t0.elapsed() < Duration::from_secs(10) {
        std::thread::sleep(Duration::from_millis(5));
    }
    let n = slow.lock().unwrap().trace.len();
    std::thread::sleep(Duration::from_millis(200));
    assert_eq!(slow.lock().unwrap().trace.len(), n);
    d.resume().unwrap();
    assert_eq!(d.wait_job(Duration::from_secs(30)).status, JobStatus::Done);
    let t0 = std::time::Instant::now();
    while slow.lock().unwrap().running() && t0.elapsed() < Duration::from_secs(30) {
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!((burn_mm(&slow.lock().unwrap().trace) - 100.0).abs() < 0.2);
    // stop: kolejne zadanie przerwane, wiązka wyłączona (I)
    let mut d2 = connect(&emu(Board::M2), Board::M2);
    d2.send_job(&f, JobOptions::default()).unwrap();
    d2.stop();
    let s = d2.wait_job(Duration::from_secs(5)).status;
    assert!(matches!(s, JobStatus::Stopped | JobStatus::Done), "{s:?}");
}

// ---------- REQ-13-039: CH341 natywnie przez USB ----------

struct EmuOpener(Arc<dyn UsbBackend>);

impl Opener for EmuOpener {
    fn open(&self, _spec: &TransportSpec) -> Result<Box<dyn Transport>, TransportError> {
        Err(TransportError("brak".into()))
    }
    fn ports(&self) -> Vec<PortInfo> {
        Vec::new()
    }
    fn usb(&self) -> Arc<dyn UsbBackend> {
        self.0.clone()
    }
}

fn usb_profile(driver: &str) -> DeviceProfile {
    serde_json::from_value(serde_json::json!({
        "name": "K40 USB", "driver": driver,
        "transport": {"kind": "usb", "vid": 0x1a86, "pid": 0x5512},
        "workspace": [310, 210],
        "k40": {"board": "m2"}
    }))
    .unwrap()
}

#[test]
fn req_13_039_ch341_native_usb() {
    let e = emu(Board::M2);
    let mut m = Machines::new(Arc::new(EmuOpener(Arc::new(EmuUsb { emu: e.clone() }))));
    m.save_profile(usb_profile("lihuiyu")).unwrap();
    m.connect("K40 USB").unwrap();
    let doc = rect_doc(20.0, 100.0);
    let j = bc_plan::plan(&doc, &PlanOptions::default()).unwrap();
    let d = m.driver("K40 USB").unwrap();
    let f = d.compile(&j, &Context::from_document(&doc)).unwrap();
    assert_eq!(f.driver, "lihuiyu");
    assert_eq!(f.extension, "egv");
    d.send_job(&f, JobOptions::default()).unwrap();
    let t0 = std::time::Instant::now();
    while d.job().status == JobStatus::Running && t0.elapsed() < Duration::from_secs(30) {
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(d.job().status, JobStatus::Done);
    // ramki 0xA6 dekodowane przez urządzenie USB, inicjalizacja EPP żądaniem 0xB1
    let g = e.lock().unwrap();
    // + pakiet dopełnienia po ostatnim (płyta M2 Nano wykonuje pakiet po następnym)
    assert_eq!(g.accepted, egv::packets(&f.bytes).len() + 1);
    assert!((burn_mm(&g.trace) - 100.0).abs() < 0.2);
}

// ---------- REQ-13-040: CH341DLL (Windows) ----------

#[test]
fn req_13_040_ch341_dll_runtime_loading() {
    // biblioteka ładowana w czasie działania: bez sterownika — jasny błąd, nie awaria
    match bc_drivers::ch341::Ch341Dll::open(15) {
        Ok(_) => {}
        Err(e) => assert!(e.0.contains("CH341DLL") || e.0.contains("CH341"), "{e}"),
    }
    let p: DeviceProfile = serde_json::from_value(serde_json::json!({
        "name": "K40 DLL", "driver": "lihuiyu",
        "transport": {"kind": "ch341_dll", "index": 0}
    }))
    .unwrap();
    p.validate().unwrap();
    assert_eq!(p.transport.describe(), "ch341dll 0");
}

// ---------- REQ-13-051, 052: Moshi ----------

fn moshi_file(doc: &Document) -> MachineFile {
    let j = bc_plan::plan(doc, &PlanOptions::default()).unwrap();
    let mut c = Context::from_document(doc);
    c.bed = bc_drivers::moshi::BED;
    compile("moshi", &j, &c).unwrap()
}

#[test]
fn req_13_051_moshi_ms10105_over_ch341() {
    let e = MoshiEmulator::shared(MoshiConfig::default());
    let mut m = Machines::new(Arc::new(EmuOpener(Arc::new(EmuUsb { emu: e.clone() }))));
    let mut p = usb_profile("moshi");
    p.workspace = bc_drivers::moshi::BED;
    m.save_profile(p).unwrap();
    m.connect("K40 USB").unwrap();
    let d = m.driver("K40 USB").unwrap();
    assert_eq!(d.status().variant.as_deref(), Some("ms10105"));
    let mut doc = rect_doc(25.0, 100.0);
    doc.workspace.width = 330.0;
    let j = bc_plan::plan(&doc, &PlanOptions::default()).unwrap();
    let f = d.compile(&j, &Context::from_document(&doc)).unwrap();
    assert_eq!(f.driver, "moshi");
    d.send_job(&f, JobOptions::default()).unwrap();
    let t0 = std::time::Instant::now();
    while d.job().status == JobStatus::Running && t0.elapsed() < Duration::from_secs(30) {
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(d.job().status, JobStatus::Done);
    let g = e.lock().unwrap();
    assert_eq!(g.programs.len(), 1);
    assert!((burn_mm(&g.trace) - 100.0).abs() < 0.2);
    assert!(g
        .trace
        .iter()
        .filter(|m| m.on)
        .all(|m| (m.speed - 25.0).abs() < 1e-9));
}

#[test]
fn req_13_052_moshi_protocol_job_in_blocks() {
    let mut doc = rect_doc(25.0, 100.0);
    doc.add(
        1,
        Kind::Ellipse {
            cx: 70.0,
            cy: 25.0,
            rx: 10.0,
            ry: 8.0,
        },
    )
    .unwrap();
    {
        let l = doc.layer_mut(1).unwrap();
        l.speed = 50.0;
        l.power_max = 100.0;
        l.air_assist = false;
        l.color = "#ff0000".into();
    }
    doc.workspace.width = 330.0;
    let f = moshi_file(&doc);
    // program na operację: prędkość, przesunięcie, ruchy, 7 × zakończenie
    let progs = proto::programs(&f.bytes).unwrap();
    assert_eq!(progs.len(), 2);
    let cmds = proto::parse(&progs[1]).unwrap();
    assert_eq!(
        cmds[0],
        proto::Cmd::VectorSpeed {
            cut: 49,
            normal: 39
        }
    );
    assert!(matches!(cmds[1], proto::Cmd::Offset { .. }));
    assert_eq!(
        cmds.iter()
            .rev()
            .take(7)
            .filter(|c| **c == proto::Cmd::Terminate)
            .count(),
        7
    );
    // sterownik: bloki po 32 bajty między prologiem a epilogiem
    let e = MoshiEmulator::shared(MoshiConfig { speed: 200.0 });
    let mut d = MoshiDriver::connect(
        "moshi",
        Box::new(Shared(e.clone())),
        MoshiOptions::default(),
    )
    .unwrap();
    d.send_job(&f, JobOptions::default()).unwrap();
    assert_eq!(d.wait_job(Duration::from_secs(60)).status, JobStatus::Done);
    let g = e.lock().unwrap();
    assert_eq!(g.programs, progs);
    assert!(g.packets.iter().all(|n| *n <= proto::PACKET));
    let total: usize = progs.iter().map(|p| p.len().div_ceil(32)).sum();
    assert_eq!(g.packets.len(), total);
    // PNG toru ≈ podgląd planera
    let job = bc_plan::plan(&doc, &PlanOptions::default()).unwrap();
    let ej = bc_drivers::lihuiyu::emu::trace_job(
        &g.trace,
        &Steps::new(&K40Settings::default(), bc_drivers::moshi::BED[1]),
    );
    assert!(png_diff(&doc, &job, &ej) < 0.01);
}

// ---------- REQ-19-009: emulator Lihuiyu ----------

#[test]
fn req_19_009_012_lihuiyu_emulator_png_matches_planner() {
    let mut doc = rect_doc(30.0, 100.0);
    doc.add(
        0,
        Kind::Ellipse {
            cx: 50.0,
            cy: 25.0,
            rx: 6.0,
            ry: 9.0,
        },
    )
    .unwrap();
    let job = bc_plan::plan(&doc, &PlanOptions::default()).unwrap();
    let f = compile("lihuiyu", &job, &ctx(&doc, Board::M2)).unwrap();
    let e = emu(Board::M2);
    let mut d = connect(&e, Board::M2);
    assert_eq!(run(&mut d, &f), JobStatus::Done);
    let g = e.lock().unwrap();
    // pakiety z CRC, stany BUSY/OK, tor → PNG
    assert!(g.accepted > 0 && g.crc_errors == 0);
    let ej = g.trace_job(&Steps::new(&K40Settings::default(), BED[1]));
    let diff = png_diff(&doc, &job, &ej);
    assert!(diff < 0.005, "{diff}");
}

#[test]
fn req_19_009_simulators_in_machines() {
    let mut m = Machines::default();
    let names: Vec<String> = m.profiles().into_iter().map(|p| p.name).collect();
    assert!(names.contains(&"Symulator K40 (Lihuiyu)".to_string()));
    assert!(names.contains(&"Symulator Moshi".to_string()));
    m.connect("Symulator K40 (Lihuiyu)").unwrap();
    m.connect("Symulator Moshi").unwrap();
    let d = m.driver("Symulator K40 (Lihuiyu)").unwrap();
    assert!(d.capabilities().native_raster);
    d.home().unwrap();
    assert!(m
        .save_profile(DeviceProfile {
            name: "Symulator Moshi".into(),
            ..DeviceProfile::simulator_moshi()
        })
        .is_err());
}
