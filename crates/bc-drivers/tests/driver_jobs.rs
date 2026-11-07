//! Zadania na emulatorze (ADR 0010): start zadania, ramki, test wiązki, sonda, karta SD,
//! kolejka i powtórzenia, odmiany GRBL, transporty, wiele urządzeń, kreator, profile,
//! urządzenie fikcyjne, podgląd emulatora porównany z planerem.
#![allow(clippy::field_reassign_with_default)]

mod common_emu;

use std::net::TcpListener;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use bc_doc::{JobOrigin, StartFrom};
use bc_drivers::driver::*;
use bc_drivers::emu::EmuConfig;
use bc_drivers::grbl::{codes, GrblDriver, GrblOptions};
use bc_drivers::machines::*;
use bc_drivers::transport::*;
use bc_drivers::{compile, Context};
use bc_geom::Point;
use common_emu::*;

fn burns_bbox(e: &SharedEmulator) -> [f64; 4] {
    let mut b = [f64::MAX, f64::MAX, f64::MIN, f64::MIN];
    for x in e.lock().unwrap().burns() {
        for p in [x.from_pt(), x.to_pt()] {
            b = [b[0].min(p.x), b[1].min(p.y), b[2].max(p.x), b[3].max(p.y)];
        }
    }
    b
}

fn close4(a: [f64; 4], b: [f64; 4]) -> bool {
    a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-6)
}

fn job_with_start(from: StartFrom, origin: JobOrigin) -> bc_drivers::MachineFile {
    let mut d = rect_doc();
    d.start.from = from;
    d.start.origin = origin;
    file(&d)
}

#[test]
fn req_10_001_start_from_absolute() {
    let e = emu(EmuConfig::default());
    let mut d = connect(&e);
    d.move_to(Some(100.0), Some(100.0), None, None).unwrap();
    let j = run_job(
        &mut d,
        &job_with_start(StartFrom::Absolute, JobOrigin::Center),
    );
    assert_eq!(j.status, JobStatus::Done);
    assert!(
        close4(burns_bbox(&e), [10.0, 10.0, 40.0, 30.0]),
        "{:?}",
        burns_bbox(&e)
    );
}

#[test]
fn req_10_002_start_from_current_position() {
    let e = emu(EmuConfig::default());
    let mut d = connect(&e);
    d.move_to(Some(100.0), Some(80.0), None, None).unwrap();
    d.wait_idle(Duration::from_secs(5));
    let j = run_job(
        &mut d,
        &job_with_start(StartFrom::Current, JobOrigin::BottomLeft),
    );
    assert_eq!(j.status, JobStatus::Done);
    assert!(
        close4(burns_bbox(&e), [100.0, 80.0, 130.0, 100.0]),
        "{:?}",
        burns_bbox(&e)
    );
    // powrót do punktu startu, tryb G90 przywrócony (M2)
    assert_eq!(d.status().mpos, [100.0, 80.0, 0.0]);
}

#[test]
fn req_10_003_012_start_from_user_origin() {
    let e = emu(EmuConfig::default());
    let mut d = connect(&e);
    d.move_to(Some(60.0), Some(50.0), None, None).unwrap();
    d.set_origin(true, true, false).unwrap();
    std::thread::sleep(Duration::from_millis(60));
    let s = d.status();
    assert_eq!(s.wco, [60.0, 50.0, 0.0], "zero użytkownika (G10 L20)");
    assert_eq!(s.wpos, [0.0, 0.0, 0.0]);
    d.move_to(Some(30.0), Some(30.0), None, None).unwrap();
    let j = run_job(
        &mut d,
        &job_with_start(StartFrom::UserOrigin, JobOrigin::TopRight),
    );
    assert_eq!(j.status, JobStatus::Done);
    // prawy górny róg ramki w zerze użytkownika (60, 50)
    assert!(
        close4(burns_bbox(&e), [30.0, 30.0, 60.0, 50.0]),
        "{:?}",
        burns_bbox(&e)
    );
}

#[test]
fn req_10_004_nine_point_job_origin() {
    let want = [
        (JobOrigin::TopLeft, [0.0, -20.0]),
        (JobOrigin::Top, [-15.0, -20.0]),
        (JobOrigin::TopRight, [-30.0, -20.0]),
        (JobOrigin::Left, [0.0, -10.0]),
        (JobOrigin::Center, [-15.0, -10.0]),
        (JobOrigin::Right, [-30.0, -10.0]),
        (JobOrigin::BottomLeft, [0.0, 0.0]),
        (JobOrigin::Bottom, [-15.0, 0.0]),
        (JobOrigin::BottomRight, [-30.0, 0.0]),
    ];
    for (o, off) in want {
        let e = emu(EmuConfig::default());
        let mut d = connect(&e);
        d.move_to(Some(100.0), Some(100.0), None, None).unwrap();
        d.wait_idle(Duration::from_secs(5));
        run_job(&mut d, &job_with_start(StartFrom::Current, o));
        let b = burns_bbox(&e);
        let x0 = 100.0 + off[0];
        let y0 = 100.0 + off[1];
        assert!(close4(b, [x0, y0, x0 + 30.0, y0 + 20.0]), "{o:?}: {b:?}");
    }
}

#[test]
fn req_10_007_008_010_frames() {
    let mut doc = rect_doc();
    doc.add(
        0,
        bc_doc::Kind::Polygon {
            cx: 80.0,
            cy: 50.0,
            r: 10.0,
            sides: 3,
            rotation: 90.0,
        },
    )
    .unwrap();
    let job = bc_plan::plan(&doc, &Default::default()).unwrap();
    let rect = bc_drivers::gcode::frame_points(&job, &doc.start, false);
    assert_eq!(rect.len(), 4);
    let contour = bc_drivers::gcode::frame_points(&job, &doc.start, true);
    assert!(contour.len() >= 5 && contour.len() != 4, "{contour:?}");
    let e = emu(EmuConfig::default());
    let mut d = connect(&e);
    d.frame(&rect, 50.0, None).unwrap();
    d.wait_idle(Duration::from_secs(5));
    {
        let em = e.lock().unwrap();
        assert!(em.burns().is_empty(), "ramka bez wiązki");
        let visited: Vec<Point> = em.trace.iter().map(|b| b.to_pt()).collect();
        for p in &rect {
            assert!(
                visited
                    .iter()
                    .any(|v| v.distance(Point::new(p[0], p[1])) < 1e-6),
                "{p:?}"
            );
        }
    }
    // kontur (rubber band): wierzchołki otoczki, w tym wierzchołek trójkąta
    let n0 = e.lock().unwrap().trace.len();
    d.frame(&contour, 50.0, None).unwrap();
    d.wait_idle(Duration::from_secs(5));
    let tip = e.lock().unwrap().trace[n0..]
        .iter()
        .map(|b| b.to[1])
        .fold(f64::MIN, f64::max);
    assert!((tip - 60.0).abs() < 1e-3, "{tip}");
    // czerwony laser z korektą offsetu i poleceniami włączenia
    let e2 = emu(EmuConfig::default());
    let mut d2 = connect(&e2);
    d2.frame(&rect, 50.0, Some(("M8", "M9", [5.0, -3.0])))
        .unwrap();
    d2.wait_idle(Duration::from_secs(5));
    let em = e2.lock().unwrap();
    let txt = String::from_utf8_lossy(&em.received).to_string();
    let on = txt.find("M8").unwrap();
    let off = txt.rfind("M9").unwrap();
    assert!(on < txt.find("G0 X").unwrap() && off > on);
    // plamka czerwonego lasera trafia w ramkę: głowica przesunięta o −offset
    assert!(em
        .trace
        .iter()
        .any(|b| (b.to[0] - (rect[2][0] - 5.0)).abs() < 1e-6
            && (b.to[1] - (rect[2][1] + 3.0)).abs() < 1e-6));
}

#[test]
fn req_13_008_009_test_fire_and_frame_via_trait() {
    let e = emu(EmuConfig::default());
    let mut d = connect(&e);
    let dr: &mut dyn Driver = &mut d;
    dr.test_fire(20.0, 150).unwrap();
    dr.frame(&[[0.0, 0.0], [10.0, 0.0], [10.0, 10.0]], 30.0, None)
        .unwrap();
    assert!(dr.test_fire(150.0, 10).is_err());
    d.wait_idle(Duration::from_secs(5));
    let em = e.lock().unwrap();
    let fire: Vec<_> = em.burns();
    assert_eq!(fire.len(), 1);
    assert_eq!(fire[0].dwell, 0.15);
    assert_eq!(fire[0].s, 200.0);
    assert!(String::from_utf8_lossy(&em.received).contains("M5"));
}

#[test]
fn req_11_028_probe_z_g38() {
    let mut cfg = EmuConfig::default();
    cfg.probe_z = Some(-7.25);
    let e = emu(cfg);
    let mut d = connect(&e);
    let z = d.probe_z(20.0, 2.0).unwrap();
    assert!((z + 7.25).abs() < 1e-6, "{z}");
    assert!(String::from_utf8_lossy(&e.lock().unwrap().received).contains("G38.2 Z-20 F120"));
    // brak kontaktu: ALARM:5
    let e2 = emu(EmuConfig::default());
    let mut d2 = connect(&e2);
    match d2.probe_z(5.0, 2.0) {
        Err(DriverError::Alarm(5, _)) | Err(DriverError::Invalid(_)) => {}
        other => panic!("{other:?}"),
    }
}

#[test]
fn req_10_018_save_to_sd_card() {
    let e = emu(EmuConfig::variant("fluidnc"));
    let mut d = connect(&e);
    assert_eq!(d.status().variant.as_deref(), Some("fluidnc"));
    assert!(d.capabilities().files);
    let f = file(&rect_doc());
    d.upload_file("/sd/prostokat.gcode", &f.bytes).unwrap();
    assert_eq!(e.lock().unwrap().sd["/sd/prostokat.gcode"], f.bytes);
    let list = d.list_files().unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].size as usize, f.bytes.len());
    d.run_file("/sd/prostokat.gcode").unwrap();
    std::thread::sleep(Duration::from_millis(100));
    assert!((burn_len(&e) - 100.0).abs() < 1e-3);
    d.delete_file("/sd/prostokat.gcode").unwrap();
    assert!(d.list_files().unwrap().is_empty());
    // GRBL bez karty SD
    let e2 = emu(EmuConfig::default());
    let mut d2 = connect(&e2);
    assert!(matches!(
        d2.upload_file("/x", b"G0"),
        Err(DriverError::Unsupported(_))
    ));
}

#[test]
fn req_10_020_021_queue_and_repeat() {
    let mut cfg = EmuConfig::default();
    cfg.speed = 20.0;
    let e = emu(cfg);
    let mut d = connect(&e);
    let f = file(&rect_doc());
    let t = Instant::now();
    d.send_job(
        &f,
        JobOptions {
            name: "pierwsze".into(),
            repeat: 3,
            delay_s: 0.2,
        },
    )
    .unwrap();
    let mut doc2 = rect_doc();
    doc2.objects.clear();
    doc2.add(
        0,
        bc_doc::Kind::Rect {
            x: 100.,
            y: 100.,
            w: 10.,
            h: 10.,
            radius: 0.,
        },
    )
    .unwrap();
    let f2 = file(&doc2);
    let q = d
        .queue_job(
            &f2,
            JobOptions {
                name: "drugie".into(),
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(q, 1, "w kolejce");
    // pierwsze zadanie: 3 rundy z opóźnieniem
    let end = Instant::now() + Duration::from_secs(30);
    while d.job().name != "drugie" && Instant::now() < end {
        std::thread::sleep(Duration::from_millis(5));
    }
    let j = d.wait_job(Duration::from_secs(30));
    assert_eq!(j.name, "drugie");
    assert_eq!(j.status, JobStatus::Done);
    assert!(
        t.elapsed() >= Duration::from_millis(400),
        "opóźnienia między rundami"
    );
    assert!(
        (burn_len(&e) - (3.0 * 100.0 + 40.0)).abs() < 1e-3,
        "{}",
        burn_len(&e)
    );
}

#[test]
fn req_13_050_053_to_070_grbl_variants() {
    for v in codes::VARIANTS {
        let e = emu(EmuConfig::variant(v.id));
        let opts = GrblOptions {
            variant: None,
            ..Default::default()
        };
        let mut d = connect_with(&e, opts);
        let detected = d.status().variant.unwrap();
        // odmiany bez znacznika w $I rozpoznają się jako GRBL 1.1 — wtedy profil wymusza odmianę
        if v.marker.is_empty() && !matches!(v.id, "grbl09" | "grbl_esp32") {
            assert!(
                detected == v.id || detected == "grbl11",
                "{}: {detected}",
                v.id
            );
            // poprzednie połączenie zamknięte, zanim nowe zacznie czytać ten sam emulator
            d.disconnect();
            d = connect_with(
                &e,
                GrblOptions {
                    variant: Some(v.id.into()),
                    ..Default::default()
                },
            );
        } else {
            assert_eq!(detected, v.id, "rozpoznanie {}", v.id);
        }
        assert_eq!(d.variant().id, v.id);
        // zadanie: moc wg s_max odmiany
        let doc = rect_doc();
        let job = bc_plan::plan(&doc, &Default::default()).unwrap();
        let f = d.compile(&job, &Context::from_document(&doc)).unwrap();
        let j = run_job(&mut d, &f);
        assert_eq!(j.status, JobStatus::Done, "{}: {j:?}", v.id);
        let b = e.lock().unwrap().burns();
        assert!((burn_len(&e) - 100.0).abs() < 1e-3, "{}", v.id);
        assert!(
            b.iter().all(|x| (x.s - v.s_max * 0.5).abs() < 1e-6),
            "{}: S {}",
            v.id,
            b[0].s
        );
        // 0.9 bez trybu laserowego: brak wiązki w przejazdach
        assert!(
            e.lock()
                .unwrap()
                .trace
                .iter()
                .filter(|x| x.rapid)
                .all(|x| !x.on),
            "{}",
            v.id
        );
        d.disconnect();
    }
}

fn tcp_emu(cfg: EmuConfig) -> (u16, SharedEmulator) {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = l.local_addr().unwrap().port();
    let e = emu(cfg);
    serve_tcp(l, e.clone());
    (port, e)
}

#[test]
fn req_13_022_073_tcp_transport() {
    let (port, e) = tcp_emu(EmuConfig::variant("grblhal"));
    let t = TcpTransport::connect("127.0.0.1", port).unwrap();
    assert!(t.describe().contains("tcp"));
    let mut d = GrblDriver::connect("tcp", Box::new(t), GrblOptions::default()).unwrap();
    assert_eq!(d.status().variant.as_deref(), Some("grblhal"));
    let j = run_job(&mut d, &file(&rect_doc()));
    assert_eq!(j.status, JobStatus::Done);
    assert!((burn_len(&e) - 100.0).abs() < 1e-3);
    // domyślny port Telnet 23 w profilu
    let spec: TransportSpec =
        serde_json::from_value(serde_json::json!({"kind": "tcp", "host": "laser.local"})).unwrap();
    assert_eq!(
        spec,
        TransportSpec::Tcp {
            host: "laser.local".into(),
            port: 23
        }
    );
}

#[test]
fn req_13_024_074_websocket_transport() {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = l.local_addr().unwrap().port();
    let e = emu(EmuConfig::variant("fluidnc"));
    serve_ws(l, e.clone());
    let t = WsTransport::connect(&format!("ws://127.0.0.1:{port}/")).unwrap();
    let mut d = GrblDriver::connect("ws", Box::new(t), GrblOptions::default()).unwrap();
    assert_eq!(d.status().variant.as_deref(), Some("fluidnc"));
    let j = run_job(&mut d, &file(&rect_doc()));
    assert_eq!(j.status, JobStatus::Done);
    assert!((burn_len(&e) - 100.0).abs() < 1e-3);
}

/// Port szeregowy-atrapa (testuje ścieżkę serialport bez sprzętu).
struct MockPort {
    emu: SharedEmulator,
    last: Instant,
    timeout: Duration,
}

impl std::io::Read for MockPort {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let end = Instant::now() + self.timeout;
        loop {
            let out = {
                let mut e = self.emu.lock().unwrap();
                let now = Instant::now();
                e.advance((now - self.last).as_secs_f64());
                self.last = now;
                e.take_output_max(buf.len())
            };
            if !out.is_empty() {
                buf[..out.len()].copy_from_slice(&out);
                return Ok(out.len());
            }
            if Instant::now() >= end {
                return Err(std::io::ErrorKind::TimedOut.into());
            }
            std::thread::sleep(Duration::from_millis(1));
        }
    }
}

impl std::io::Write for MockPort {
    fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
        self.emu.lock().unwrap().feed(b);
        Ok(b.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl serialport::SerialPort for MockPort {
    fn name(&self) -> Option<String> {
        Some("MOCK0".into())
    }
    fn baud_rate(&self) -> serialport::Result<u32> {
        Ok(115200)
    }
    fn data_bits(&self) -> serialport::Result<serialport::DataBits> {
        Ok(serialport::DataBits::Eight)
    }
    fn flow_control(&self) -> serialport::Result<serialport::FlowControl> {
        Ok(serialport::FlowControl::None)
    }
    fn parity(&self) -> serialport::Result<serialport::Parity> {
        Ok(serialport::Parity::None)
    }
    fn stop_bits(&self) -> serialport::Result<serialport::StopBits> {
        Ok(serialport::StopBits::One)
    }
    fn timeout(&self) -> Duration {
        self.timeout
    }
    fn set_baud_rate(&mut self, _: u32) -> serialport::Result<()> {
        Ok(())
    }
    fn set_data_bits(&mut self, _: serialport::DataBits) -> serialport::Result<()> {
        Ok(())
    }
    fn set_flow_control(&mut self, _: serialport::FlowControl) -> serialport::Result<()> {
        Ok(())
    }
    fn set_parity(&mut self, _: serialport::Parity) -> serialport::Result<()> {
        Ok(())
    }
    fn set_stop_bits(&mut self, _: serialport::StopBits) -> serialport::Result<()> {
        Ok(())
    }
    fn set_timeout(&mut self, t: Duration) -> serialport::Result<()> {
        self.timeout = t;
        Ok(())
    }
    fn write_request_to_send(&mut self, _: bool) -> serialport::Result<()> {
        Ok(())
    }
    fn write_data_terminal_ready(&mut self, _: bool) -> serialport::Result<()> {
        Ok(())
    }
    fn read_clear_to_send(&mut self) -> serialport::Result<bool> {
        Ok(true)
    }
    fn read_data_set_ready(&mut self) -> serialport::Result<bool> {
        Ok(true)
    }
    fn read_ring_indicator(&mut self) -> serialport::Result<bool> {
        Ok(false)
    }
    fn read_carrier_detect(&mut self) -> serialport::Result<bool> {
        Ok(false)
    }
    fn bytes_to_read(&self) -> serialport::Result<u32> {
        Ok(0)
    }
    fn bytes_to_write(&self) -> serialport::Result<u32> {
        Ok(0)
    }
    fn clear(&self, _: serialport::ClearBuffer) -> serialport::Result<()> {
        Ok(())
    }
    fn try_clone(&self) -> serialport::Result<Box<dyn serialport::SerialPort>> {
        Err(serialport::Error::new(
            serialport::ErrorKind::Unknown,
            "mock",
        ))
    }
    fn set_break(&self) -> serialport::Result<()> {
        Ok(())
    }
    fn clear_break(&self) -> serialport::Result<()> {
        Ok(())
    }
}

fn mock_serial(cfg: EmuConfig) -> (SerialTransport, SharedEmulator) {
    let e = emu(cfg);
    let p = MockPort {
        emu: e.clone(),
        last: Instant::now(),
        timeout: Duration::from_millis(10),
    };
    (SerialTransport::from_port(Box::new(p), "MOCK0"), e)
}

#[test]
fn req_13_018_072_serial_transport_and_usb_chips() {
    let (t, e) = mock_serial(EmuConfig::variant("ortur"));
    assert_eq!(t.describe(), "serial MOCK0");
    let mut d = GrblDriver::connect("serial", Box::new(t), GrblOptions::default()).unwrap();
    assert_eq!(d.status().variant.as_deref(), Some("ortur"));
    let j = run_job(&mut d, &file(&rect_doc()));
    assert_eq!(j.status, JobStatus::Done);
    assert!((burn_len(&e) - 100.0).abs() < 1e-3);
    assert_eq!(usb_chip(0x0403, 0x6001), Some("FTDI"));
    assert_eq!(usb_chip(0x1A86, 0x7523), Some("CH340"));
    assert_eq!(usb_chip(0x10C4, 0xEA60), Some("CP210x"));
    assert_eq!(usb_chip(0x1234, 0x5678), None);
    // nieistniejący port: czytelny błąd
    assert!(SerialTransport::open("BRAK-PORTU-XYZ", 115200).is_err());
    let _ = list_serial_ports();
}

/// Opener testowy: porty-atrapy z emulatorami (kreator, wiele urządzeń).
struct TestOpener {
    ports: Vec<(PortInfo, Option<&'static str>)>,
    emus: Mutex<Vec<SharedEmulator>>,
}

impl Opener for TestOpener {
    fn open(&self, spec: &TransportSpec) -> Result<Box<dyn Transport>, TransportError> {
        match spec {
            TransportSpec::Serial { port, baud } => {
                let (_, v) = self
                    .ports
                    .iter()
                    .find(|(p, _)| &p.port == port)
                    .ok_or_else(|| TransportError("brak portu".into()))?;
                match v {
                    Some(v) if *baud == 115200 => {
                        let (t, e) = mock_serial(EmuConfig::variant(v));
                        self.emus.lock().unwrap().push(e);
                        Ok(Box::new(t))
                    }
                    // inne urządzenie / zła prędkość: cisza
                    _ => Ok(Box::new(Silent)),
                }
            }
            other => SystemOpener.open(other),
        }
    }
    fn ports(&self) -> Vec<PortInfo> {
        self.ports.iter().map(|p| p.0.clone()).collect()
    }
}

struct Silent;
impl Transport for Silent {
    fn write(&mut self, _: &[u8]) -> Result<(), TransportError> {
        Ok(())
    }
    fn read(&mut self, _: &mut [u8], t: Duration) -> Result<usize, TransportError> {
        std::thread::sleep(t.min(Duration::from_millis(5)));
        Ok(0)
    }
    fn describe(&self) -> String {
        "cisza".into()
    }
}

fn port(name: &str, vid: u16, pid: u16) -> PortInfo {
    PortInfo {
        port: name.into(),
        vid: Some(vid),
        pid: Some(pid),
        chip: usb_chip(vid, pid).map(str::to_string),
        product: None,
    }
}

#[test]
fn req_13_192_find_my_laser() {
    let op = Arc::new(TestOpener {
        ports: vec![
            (port("COM3", 0x1A86, 0x7523), Some("sculpfun")),
            (port("COM4", 0x0403, 0x6001), None),
        ],
        emus: Mutex::new(Vec::new()),
    });
    let m = Machines::new(op);
    let found = m.find(true, &[115200, 250000]);
    assert_eq!(found.len(), 2);
    assert_eq!(found[0].port.chip.as_deref(), Some("CH340"));
    assert_eq!(found[0].variant.as_deref(), Some("sculpfun"));
    assert_eq!(found[0].baud, Some(115200));
    let p = found[0].profile.clone().unwrap();
    assert!(p.name.contains("Sculpfun") && p.name.contains("COM3"));
    assert!(found[1].variant.is_none() && found[1].error.is_some());
    // bez próby połączenia: tylko lista portów
    let quick = m.find(false, &[115200]);
    assert!(quick
        .iter()
        .all(|f| f.variant.is_none() && f.error.is_none()));
}

#[test]
fn req_13_196_028_profiles_and_multiple_devices() {
    let dir = std::env::temp_dir().join(format!("bc-devices-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let op = Arc::new(TestOpener {
        ports: vec![(port("COM7", 0x10C4, 0xEA60), Some("atomstack"))],
        emus: Mutex::new(Vec::new()),
    });
    let mut m = Machines::new(op.clone());
    m.set_store(&dir).unwrap();
    // ręczny profil
    let mut p = DeviceProfile::simulator();
    p.name = "Atomstack".into();
    p.variant = Some("atomstack".into());
    p.transport = TransportSpec::Serial {
        port: "COM7".into(),
        baud: 115200,
    };
    p.workspace = [410.0, 400.0];
    m.save_profile(p.clone()).unwrap();
    let mut bad = p.clone();
    bad.name = SIMULATOR.into();
    assert!(m.save_profile(bad).is_err());
    let mut bad = p.clone();
    bad.variant = Some("nieznana".into());
    assert!(m.save_profile(bad).is_err());
    // zapis w devices.json i odczyt
    let mut m2 = Machines::new(op.clone());
    m2.set_store(&dir).unwrap();
    assert_eq!(m2.profile("Atomstack"), Some(p.clone()));
    assert_eq!(
        m2.profiles()[0].name,
        SIMULATOR,
        "urządzenie fikcyjne zawsze dostępne"
    );
    // dwa urządzenia naraz, niezależne zadania
    m2.connect("Atomstack").unwrap();
    m2.connect(SIMULATOR).unwrap();
    assert_eq!(m2.connected().len(), 2);
    let f = file(&rect_doc());
    for n in ["Atomstack", SIMULATOR] {
        m2.driver(n)
            .unwrap()
            .send_job(&f, JobOptions::default())
            .unwrap();
    }
    for n in ["Atomstack", SIMULATOR] {
        let end = Instant::now() + Duration::from_secs(30);
        while m2.driver(n).unwrap().job().status == JobStatus::Running && Instant::now() < end {
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(m2.driver(n).unwrap().job().status, JobStatus::Done, "{n}");
    }
    let e = op.emus.lock().unwrap()[0].clone();
    assert!((burn_len(&e) - 100.0).abs() < 1e-3);
    assert!(m2.disconnect("Atomstack"));
    assert_eq!(m2.connected(), vec![SIMULATOR.to_string()]);
    assert!(m2.delete_profile("Atomstack").unwrap());
}

#[test]
fn req_13_191_017_simulator_device() {
    let mut m = Machines::default();
    let mut p = DeviceProfile::simulator();
    p.transport = TransportSpec::Emulator {
        variant: "grbl11".into(),
        speed: None,
    };
    let t = SystemOpener.open(&p.transport).unwrap();
    let d = GrblDriver::connect(SIMULATOR, t, GrblOptions::default()).unwrap();
    m.attach(p, Box::new(d));
    let dr = m.driver(SIMULATOR).unwrap();
    assert_eq!(dr.status().state, RunState::Idle);
    let doc = rect_doc();
    let job = bc_plan::plan(&doc, &Default::default()).unwrap();
    let f = dr.compile(&job, &Context::from_document(&doc)).unwrap();
    dr.send_job(&f, JobOptions::default()).unwrap();
    let end = Instant::now() + Duration::from_secs(10);
    while dr.job().status == JobStatus::Running && Instant::now() < end {
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(dr.job().status, JobStatus::Done);
    // każdy sterownik ma emulator: konsola pokazuje wymianę z emulatorem
    assert!(dr.console().iter().any(|l| l.starts_with("< Grbl 1.1h")));
}

fn pixels(png: &[u8]) -> Vec<u8> {
    tiny_skia::Pixmap::decode_png(png).unwrap().data().to_vec()
}

#[test]
fn req_19_012_007_emulator_png_matches_planner_preview() {
    let mut doc = rect_doc();
    doc.add(
        0,
        bc_doc::Kind::Ellipse {
            cx: 70.0,
            cy: 30.0,
            rx: 12.0,
            ry: 8.0,
        },
    )
    .unwrap();
    let job = bc_plan::plan(&doc, &Default::default()).unwrap();
    let f = compile("grbl", &job, &Context::from_document(&doc)).unwrap();
    let e = emu(EmuConfig::default());
    let mut d = connect(&e);
    assert_eq!(run_job(&mut d, &f).status, JobStatus::Done);
    let trace = e.lock().unwrap().trace.clone();
    let ej = bc_drivers::emu::trace_job(&trace, 1000.0);
    let opt = bc_sim::PreviewOptions {
        bbox: Some(bc_geom::Rect::new(0.0, 0.0, 100.0, 50.0)),
        max_size: 400,
        show_workspace: false,
    };
    let a = bc_sim::render_toolpath(&doc, &job, &opt).unwrap();
    let b = bc_sim::render_toolpath(&doc, &ej, &opt).unwrap();
    assert_eq!((a.width, a.height), (b.width, b.height));
    let (pa, pb) = (pixels(&a.png), pixels(&b.png));
    let diff = pa
        .chunks(4)
        .zip(pb.chunks(4))
        .filter(|(x, y)| x.iter().zip(y.iter()).any(|(p, q)| p.abs_diff(*q) > 40))
        .count();
    let total = (a.width * a.height) as usize;
    // tolerancja: 0,5 % pikseli (wygładzanie, kolejność przejazdów)
    assert!(diff * 200 <= total, "{diff} z {total} pikseli różnych");
}
