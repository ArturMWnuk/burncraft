//! M3.1 — polecenia kreatora USB: `machine.find` z urządzeniami USB znanych rodzin
//! (REQ-13-193) i `machine.usb_guide` — przewodnik sterownika WinUSB na Windows
//! (REQ-13-020) i reguł udev na Linuksie (REQ-13-021).

use std::sync::Arc;

use bc_commands::{Registry, SafetyTier, Session};
use bc_drivers::machines::{open_spec, Machines, Opener};
use bc_drivers::transport::{PortInfo, Transport, TransportError, TransportSpec};
use bc_drivers::usb::{FakeUsb, UsbBackend};
use serde_json::{json, Value};

struct FakeOpener(Arc<FakeUsb>);

impl Opener for FakeOpener {
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

fn session() -> Session {
    Session {
        machines: Machines::new(Arc::new(FakeOpener(Arc::new(FakeUsb::demo())))).handle(),
        ..Default::default()
    }
}

fn run(r: &Registry, s: &mut Session, id: &str, p: Value) -> Value {
    s.execute(r, id, p).unwrap_or_else(|e| panic!("{id}: {e}"))
}

#[test]
fn req_13_193_machine_find_lists_usb_lasers() {
    let r = Registry::builtin();
    let mut s = session();
    let f = run(&r, &mut s, "machine.find", json!({"probe": false}));
    let usb = f["usb"].as_array().expect("usb w wyniku kreatora");
    assert_eq!(usb.len(), 2, "{usb:?}");
    let k40 = usb.iter().find(|u| u["device"]["vid"] == 0x1A86).unwrap();
    assert_eq!(k40["family"]["id"], "ch341_k40");
    assert_eq!(k40["device"]["access"]["state"], "driver_missing");
    assert_eq!(k40["guide"]["ok"], false);
    let spec = &r.get("machine.find").unwrap().spec;
    assert!(spec.requirements.contains(&"REQ-13-193"));
}

#[test]
fn req_13_020_021_usb_guide_command() {
    let r = Registry::builtin();
    let mut s = session();
    let g = run(
        &r,
        &mut s,
        "machine.usb_guide",
        json!({"vid": 0x1A86, "pid": 0x5512, "os": "windows"}),
    );
    let steps = g["steps"].as_array().unwrap();
    assert!(
        steps.iter().any(|x| x.as_str().unwrap().contains("Zadig")),
        "{g}"
    );
    let g = run(
        &r,
        &mut s,
        "machine.usb_guide",
        json!({"vid": 0x9588, "pid": 0x9899}),
    );
    assert_eq!(g["ok"], true);
    // nieznane urządzenie
    let e = s
        .execute(&r, "machine.usb_guide", json!({"vid": 1, "pid": 2}))
        .unwrap_err();
    assert_eq!(e.code, "not_found");
    let spec = &r.get("machine.usb_guide").unwrap().spec;
    assert_eq!(spec.safety_tier, SafetyTier::Read);
    for req in ["REQ-13-020", "REQ-13-021"] {
        assert!(spec.requirements.contains(&req), "{req}");
    }
}
