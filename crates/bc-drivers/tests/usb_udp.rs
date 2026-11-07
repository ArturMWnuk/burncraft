//! M3.1 — transporty USB bulk i UDP, kreator USB po VID/PID, reguły udev i przewodnik
//! instalacji sterownika WinUSB (ADR 0013). USB sprawdzane na urządzeniu fikcyjnym
//! [`FakeUsb`]; prawdziwe urządzenia — w teście sprzętowym K40 (M3.4).

use std::net::UdpSocket;
use std::sync::Arc;
use std::time::{Duration, Instant};

use bc_drivers::machines::*;
use bc_drivers::transport::*;
use bc_drivers::usb::*;

fn read_all(t: &mut dyn Transport, want: usize) -> Vec<u8> {
    let mut out = Vec::new();
    let end = Instant::now() + Duration::from_secs(3);
    let mut buf = [0u8; 256];
    while out.len() < want && Instant::now() < end {
        let n = t.read(&mut buf, Duration::from_millis(50)).unwrap();
        out.extend_from_slice(&buf[..n]);
    }
    out
}

// ---------- UDP ----------

#[test]
fn req_13_023_udp_transport_datagrams() {
    let server = UdpSocket::bind("127.0.0.1:0").unwrap();
    server
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    let port = server.local_addr().unwrap().port();
    let mut t = UdpTransport::connect("127.0.0.1", port, None).unwrap();
    assert!(t.describe().contains("udp") && t.describe().contains(&port.to_string()));
    // zapis = jeden datagram
    t.write(&[0xD7, 0x01, 0x02]).unwrap();
    let mut buf = [0u8; 64];
    let (n, from) = server.recv_from(&mut buf).unwrap();
    assert_eq!(&buf[..n], &[0xD7, 0x01, 0x02]);
    // odpowiedź wraca do transportu (także w dwóch datagramach)
    server.send_to(b"ab", from).unwrap();
    server.send_to(b"cd", from).unwrap();
    assert_eq!(read_all(&mut t, 4), b"abcd");
    // brak danych w limicie czasu = 0 bajtów
    let t0 = Instant::now();
    assert_eq!(t.read(&mut buf, Duration::from_millis(30)).unwrap(), 0);
    assert!(t0.elapsed() < Duration::from_secs(1));
    // datagramy od innych nadawców są pomijane
    let other = UdpSocket::bind("127.0.0.1:0").unwrap();
    other.send_to(b"obcy", from).unwrap();
    server.send_to(b"ok", from).unwrap();
    assert_eq!(read_all(&mut t, 2), b"ok");
}

#[test]
fn req_13_023_udp_local_port_and_spec() {
    // Ruida: datagramy z portu 40200 na 50200 — port lokalny ustawiany w profilu
    let server = UdpSocket::bind("127.0.0.1:0").unwrap();
    server
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    let local = UdpSocket::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let spec: TransportSpec = serde_json::from_value(serde_json::json!({
        "kind": "udp", "host": "127.0.0.1", "port": server.local_addr().unwrap().port(), "local_port": local
    }))
    .unwrap();
    assert!(spec.describe().starts_with("udp 127.0.0.1:"));
    let mut t = SystemOpener.open(&spec).unwrap();
    t.write(b"x").unwrap();
    let mut buf = [0u8; 8];
    let (_, from) = server.recv_from(&mut buf).unwrap();
    assert_eq!(from.port(), local);
    assert!(UdpTransport::connect("nie.ma.takiego.hosta.invalid", 1, None).is_err());
}

// ---------- USB ----------

fn dev(vid: u16, pid: u16, access: UsbAccess) -> UsbDeviceInfo {
    UsbDeviceInfo {
        vid,
        pid,
        bus: "1".into(),
        address: 4,
        serial: None,
        manufacturer: None,
        product: None,
        driver: None,
        access,
    }
}

fn sel(vid: u16, pid: u16) -> UsbSelector {
    UsbSelector {
        vid,
        pid,
        serial: None,
        interface: 0,
        ep_out: 0x02,
        ep_in: 0x82,
    }
}

#[test]
fn req_13_019_usb_bulk_transport_on_fake_device() {
    let usb = FakeUsb {
        devices: vec![
            dev(0x1A86, 0x5512, UsbAccess::Ready),
            dev(0x9588, 0x9899, UsbAccess::DriverMissing),
        ],
    };
    let mut t = UsbTransport::open(&usb, &sel(0x1A86, 0x5512)).unwrap();
    assert_eq!(t.describe(), "usb 1a86:5512");
    t.write(&[1, 2, 3, 0xA0]).unwrap();
    assert_eq!(read_all(&mut t, 4), vec![1, 2, 3, 0xA0], "pętla zwrotna");
    let mut buf = [0u8; 8];
    assert_eq!(t.read(&mut buf, Duration::from_millis(20)).unwrap(), 0);
    // brak urządzenia albo sterownika: błąd z opisem
    let e = UsbTransport::open(&usb, &sel(0x0403, 0x6001))
        .err()
        .unwrap();
    assert!(e.0.contains("0403:6001"), "{e}");
    let e = UsbTransport::open(&usb, &sel(0x9588, 0x9899))
        .err()
        .unwrap();
    assert!(e.0.contains("sterownik"), "{e}");
}

#[test]
fn req_13_019_usb_transport_spec_and_opener() {
    let spec: TransportSpec =
        serde_json::from_value(serde_json::json!({"kind": "usb", "vid": 0x1A86, "pid": 0x5512}))
            .unwrap();
    assert_eq!(spec.describe(), "usb 1a86:5512");
    assert_eq!(
        serde_json::to_value(&spec).unwrap()["kind"],
        "usb",
        "zapis profilu"
    );
    // otwarcie przez Opener z USB urządzenia fikcyjnego (interfejs i punkty końcowe z rodziny)
    struct O(Arc<FakeUsb>);
    impl Opener for O {
        fn open(&self, spec: &TransportSpec) -> Result<Box<dyn Transport>, TransportError> {
            open_spec(self, spec)
        }
        fn ports(&self) -> Vec<PortInfo> {
            Vec::new()
        }
        fn usb(&self) -> Arc<dyn UsbBackend> {
            self.0.clone()
        }
    }
    let o = O(Arc::new(FakeUsb {
        devices: vec![dev(0x1A86, 0x5512, UsbAccess::Ready)],
    }));
    let mut t = o.open(&spec).unwrap();
    t.write(b"k40").unwrap();
    assert_eq!(read_all(t.as_mut(), 3), b"k40");
}

#[test]
fn req_13_019_known_families_and_windows_driver_state() {
    let k40 = usb_family(0x1A86, 0x5512).expect("CH341 K40");
    assert_eq!(k40.id, "ch341_k40");
    assert!(k40.name.contains("K40") && k40.name.contains("Moshi"));
    assert_eq!(k40.driver, "lihuiyu");
    assert_eq!((k40.ep_out, k40.ep_in), (0x02, 0x82));
    let galvo = usb_family(0x9588, 0x9899).expect("BJJCZ");
    assert_eq!(galvo.milestone, "M5");
    assert!(
        usb_family(0x0403, 0x6001).is_none(),
        "FTDI to port szeregowy"
    );
    assert_eq!(
        UsbAccess::from_windows_driver(Some("WinUSB")),
        UsbAccess::Ready
    );
    assert_eq!(
        UsbAccess::from_windows_driver(Some("winusb")),
        UsbAccess::Ready
    );
    assert_eq!(
        UsbAccess::from_windows_driver(None),
        UsbAccess::DriverMissing
    );
    assert_eq!(
        UsbAccess::from_windows_driver(Some("CH341")),
        UsbAccess::VendorDriver {
            driver: "CH341".into()
        }
    );
}

#[test]
fn req_13_193_find_my_laser_lists_usb_lasers_by_vid_pid() {
    struct O(Arc<FakeUsb>);
    impl Opener for O {
        fn open(&self, spec: &TransportSpec) -> Result<Box<dyn Transport>, TransportError> {
            open_spec(self, spec)
        }
        fn ports(&self) -> Vec<PortInfo> {
            Vec::new()
        }
        fn usb(&self) -> Arc<dyn UsbBackend> {
            self.0.clone()
        }
    }
    let m = Machines::new(Arc::new(O(Arc::new(FakeUsb::demo()))));
    let found = m.find_usb();
    // demo: K40 (CH341) bez sterownika i galvo BJJCZ; nieznane urządzenie (mysz) pominięte
    assert_eq!(found.len(), 2, "{found:?}");
    let k40 = found.iter().find(|f| f.device.vid == 0x1A86).expect("K40");
    assert_eq!(k40.family.unwrap().id, "ch341_k40");
    assert_eq!(k40.device.access, UsbAccess::DriverMissing);
    assert!(!k40.guide.ok && !k40.guide.steps.is_empty());
    let galvo = found
        .iter()
        .find(|f| f.device.vid == 0x9588)
        .expect("galvo");
    assert_eq!(galvo.device.access, UsbAccess::Ready);
    assert!(galvo.guide.ok);
}

// ---------- udev i przewodnik sterownika ----------

#[test]
fn req_13_021_udev_rules_file_matches_known_families() {
    let rules = udev_rules();
    for f in USB_FAMILIES {
        let line = format!(
            "ATTRS{{idVendor}}==\"{:04x}\", ATTRS{{idProduct}}==\"{:04x}\"",
            f.vid, f.pid
        );
        assert!(rules.contains(&line), "{}: {line}", f.id);
    }
    assert!(rules.contains("TAG+=\"uaccess\"") && rules.contains("MODE=\"0660\""));
    assert!(rules
        .lines()
        .all(|l| l.is_empty() || l.starts_with('#') || l.starts_with("SUBSYSTEM==\"usb\"")));
    // plik w repozytorium jest aktualny (pakiety i instrukcja instalacji)
    let file = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../packaging/linux/99-burncraft.rules"),
    )
    .expect("packaging/linux/99-burncraft.rules");
    assert_eq!(file.replace("\r\n", "\n"), rules);
}

#[test]
fn req_13_020_windows_driver_guide() {
    let k40 = dev(0x1A86, 0x5512, UsbAccess::DriverMissing);
    let g = driver_guide(&k40, "windows");
    assert!(!g.ok);
    let all = g.steps.join("\n");
    assert!(all.contains("Zadig") && all.contains("WinUSB"), "{all}");
    assert!(
        all.contains("1A86") && all.contains("5512"),
        "VID/PID do wyboru w narzędziu: {all}"
    );
    assert!(
        all.contains("CH341"),
        "alternatywa: sterownik producenta (REQ-13-040): {all}"
    );
    // sterownik producenta zamiast WinUSB
    let vendor = dev(
        0x1A86,
        0x5512,
        UsbAccess::VendorDriver {
            driver: "CH341".into(),
        },
    );
    let g = driver_guide(&vendor, "windows");
    assert!(!g.ok && g.steps.join("\n").contains("WinUSB"));
    // gotowe urządzenie: bez kroków
    let g = driver_guide(&dev(0x1A86, 0x5512, UsbAccess::Ready), "windows");
    assert!(g.ok && g.steps.is_empty());
}

#[test]
fn req_13_021_linux_permission_guide() {
    let k40 = dev(0x1A86, 0x5512, UsbAccess::PermissionDenied);
    let g = driver_guide(&k40, "linux");
    assert!(!g.ok);
    let all = g.steps.join("\n");
    assert!(all.contains(UDEV_RULES_PATH), "{all}");
    assert!(all.contains("udevadm control --reload-rules"), "{all}");
    assert!(all.contains("burncraft-cli udev"), "{all}");
    // macOS: dostęp bez sterowników i reguł
    let g = driver_guide(&dev(0x1A86, 0x5512, UsbAccess::Ready), "macos");
    assert!(g.ok);
}
