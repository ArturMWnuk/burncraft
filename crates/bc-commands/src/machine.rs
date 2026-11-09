//! Polecenia urządzeń (M2.3, ADR 0010): profile, połączenia, stan, ruch, ramki, start
//! zadania, kolejka, ustawienia maszyny, sonda, karta SD, makra. GUI, CLI i MCP używają
//! tych samych poleceń; poziomy bezpieczeństwa: ruch `motion`, wiązka `fire`, zapis
//! ustawień `config`, stop zawsze dozwolony.

use std::collections::BTreeMap;
use std::sync::OnceLock;

use bc_doc::{JobOrigin, StartFrom};
use bc_drivers::driver::{Axis, DriverError, FrameKind, JobOptions};
use bc_drivers::machines::{DeviceProfile, Macro, MacroStep};
use bc_drivers::{Context, MachineFile};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::plan::{run_plan, PlanJobParams};
use crate::{
    ok, parse, schema_of, CmdError, CommandSpec, Executor, Handler, Registry, SafetyTier, Session,
};

fn invalid(m: impl Into<String>) -> CmdError {
    CmdError::new("invalid_params", m)
}

pub(crate) fn drv_err(e: DriverError) -> CmdError {
    let code = match &e {
        DriverError::NotConnected => "no_device",
        DriverError::Alarm(..) => "alarm",
        DriverError::Machine(..) => "machine_error",
        DriverError::Busy(_) => "invalid_state",
        DriverError::Unsupported(_) => "unsupported",
        DriverError::Invalid(_) => "invalid_params",
        DriverError::Timeout(_) => "timeout",
        DriverError::Transport(_) => "failed",
    };
    CmdError::new(code, e.to_string())
}

fn device_name(s: &Session, d: &Option<String>) -> Result<String, CmdError> {
    d.clone()
        .or_else(|| s.device.clone())
        .ok_or_else(|| CmdError::new("no_device", "brak wybranego urządzenia — machine.connect"))
}

/// Wykonuje operację na podłączonym urządzeniu.
fn with<T>(
    s: &Session,
    d: &Option<String>,
    f: impl FnOnce(&mut Box<dyn bc_drivers::Driver>) -> Result<T, DriverError>,
) -> Result<T, CmdError> {
    let name = device_name(s, d)?;
    let mut m = s.machines.lock().expect("urządzenia");
    let dr = m.driver(&name).ok_or_else(|| {
        CmdError::new(
            "no_device",
            format!("urządzenie {name} nie jest podłączone"),
        )
    })?;
    f(dr).map_err(drv_err)
}

/// Kinematyka do szacowania czasu: podłączonej (wybranej) maszyny z `$110/$111/$120/$121`,
/// inaczej z dokumentu (REQ-09-008); drugi element: `device` | `document`.
pub(crate) fn kinematics_of(s: &Session) -> (bc_doc::params::Kinematics, &'static str) {
    if let Some(name) = &s.device {
        if let Some(k) = s.machines.lock().expect("urządzenia").kinematics(name) {
            return (k, "device");
        }
    }
    (s.doc.plan.kinematics, "document")
}

/// Kontekst kompilatora z kinematyką jak w [`kinematics_of`]; dla wybranego urządzenia K40
/// (lihuiyu, moshi) — płyta, kroki/mm, skala i stół z profilu (REQ-13-046, 047).
pub(crate) fn context_of(s: &Session) -> Context {
    let mut c = Context::from_document(&s.doc);
    c.kinematics = kinematics_of(s).0;
    if let Some(name) = &s.device {
        let m = s.machines.lock().expect("urządzenia");
        if let Some(p) = m
            .connected_profile(name)
            .cloned()
            .or_else(|| m.profile(name))
            .filter(|p| p.is_k40())
        {
            c.k40 = p.k40_settings();
            c.bed = p.workspace;
        }
    }
    c
}

fn profile_of(s: &Session, d: &Option<String>) -> Result<DeviceProfile, CmdError> {
    let name = device_name(s, d)?;
    let m = s.machines.lock().expect("urządzenia");
    m.connected_profile(&name)
        .cloned()
        .or_else(|| m.profile(&name))
        .ok_or_else(|| CmdError::new("no_device", format!("nieznane urządzenie {name}")))
}

#[derive(Deserialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
struct DevParam {
    /// Urządzenie (brak = wybrane w sesji).
    #[serde(default)]
    device: Option<String>,
}

#[derive(Serialize, JsonSchema)]
struct Done {
    ok: bool,
}

fn done() -> Result<Value, CmdError> {
    ok(Done { ok: true })
}

// ---------- profile i połączenia ----------

#[derive(Serialize, JsonSchema)]
struct ListResult {
    profiles: Vec<DeviceProfile>,
    connected: Vec<String>,
    selected: Option<String>,
    /// Odmiany GRBL z tabeli (id, nazwa).
    variants: Vec<(String, String)>,
    /// Warianty płyt K40 Lihuiyu (REQ-13-030…038): id, nazwa, PWM.
    boards: Vec<bc_drivers::lihuiyu::BoardInfo>,
}

fn machine_list(s: &mut Session, p: Value) -> Result<Value, CmdError> {
    let _: DevParam = parse(if p.is_null() { json!({}) } else { p })?;
    let m = s.machines.lock().expect("urządzenia");
    ok(ListResult {
        profiles: m.profiles(),
        connected: m.connected(),
        selected: s.device.clone(),
        variants: bc_drivers::grbl::codes::VARIANTS
            .iter()
            .map(|v| (v.id.to_string(), v.name.to_string()))
            .collect(),
        boards: bc_drivers::lihuiyu::BOARDS
            .iter()
            .map(|b| b.info())
            .collect(),
    })
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct ProfileParams {
    profile: DeviceProfile,
}

fn profile_save(s: &mut Session, p: Value) -> Result<Value, CmdError> {
    let p: ProfileParams = parse(p)?;
    s.machines
        .lock()
        .expect("urządzenia")
        .save_profile(p.profile.clone())
        .map_err(invalid)?;
    ok(p.profile)
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct NameParams {
    name: String,
}

#[derive(Serialize, JsonSchema)]
struct Removed {
    removed: bool,
}

fn profile_delete(s: &mut Session, p: Value) -> Result<Value, CmdError> {
    let p: NameParams = parse(p)?;
    let removed = s
        .machines
        .lock()
        .expect("urządzenia")
        .delete_profile(&p.name)
        .map_err(invalid)?;
    ok(Removed { removed })
}

fn d_bauds() -> Vec<u32> {
    vec![115200, 250000, 57600, 9600]
}

fn yes() -> bool {
    true
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct FindParams {
    /// Próba połączenia z każdym portem (rozpoznanie GRBL).
    #[serde(default = "yes")]
    probe: bool,
    /// Prędkości portu do sprawdzenia.
    #[serde(default = "d_bauds")]
    bauds: Vec<u32>,
}

fn machine_find(s: &mut Session, p: Value) -> Result<Value, CmdError> {
    let p: FindParams = parse(p)?;
    s.tick(0, 1)?;
    let (found, usb) = {
        let m = s.machines.lock().expect("urządzenia");
        (m.find(p.probe, &p.bauds), m.find_usb())
    };
    s.tick(1, 1)?;
    ok(json!({ "found": found, "usb": usb }))
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct UsbGuideParams {
    /// VID urządzenia USB.
    vid: u16,
    /// PID urządzenia USB.
    pid: u16,
    /// System: windows | linux | macos (domyślnie system programu).
    #[serde(default)]
    os: Option<String>,
}

fn machine_usb_guide(s: &mut Session, p: Value) -> Result<Value, CmdError> {
    let p: UsbGuideParams = parse(p)?;
    let dev = s
        .machines
        .lock()
        .expect("urządzenia")
        .usb_devices()
        .into_iter()
        .find(|d| d.vid == p.vid && d.pid == p.pid)
        .ok_or_else(|| {
            CmdError::new(
                "not_found",
                format!("brak urządzenia USB {:04x}:{:04x}", p.vid, p.pid),
            )
        })?;
    let os = p.os.unwrap_or_else(|| std::env::consts::OS.to_string());
    ok(bc_drivers::usb::driver_guide(&dev, &os))
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct ConnectParams {
    /// Nazwa profilu (np. „Symulator”).
    name: String,
}

#[derive(Serialize, JsonSchema)]
struct ConnectResult {
    name: String,
    variant: Option<String>,
    version: Option<String>,
    capabilities: bc_drivers::driver::Caps,
    state: bc_drivers::driver::RunState,
}

fn machine_connect(s: &mut Session, p: Value) -> Result<Value, CmdError> {
    let p: ConnectParams = parse(p)?;
    let mut m = s.machines.lock().expect("urządzenia");
    m.connect(&p.name).map_err(drv_err)?;
    let d = m.driver(&p.name).expect("podłączone");
    let st = d.status();
    let r = ConnectResult {
        name: p.name.clone(),
        variant: st.variant,
        version: st.version,
        capabilities: d.capabilities(),
        state: st.state,
    };
    let ws = m
        .connected_profile(&p.name)
        .cloned()
        .or_else(|| m.profile(&p.name))
        .map(|p| p.workspace);
    drop(m);
    // obszar dokumentu = stół podłączonej maszyny (REQ-03-003; test sprzętowy K40 B4/A5:
    // dokument 400×400 przy stole 310×210 przesuwał krzyżyk głowicy na kanwie)
    if let Some([w, h]) = ws.filter(|w| w[0] > 0.0 && w[1] > 0.0) {
        if s.doc.workspace.width != w || s.doc.workspace.height != h {
            s.doc.workspace.width = w;
            s.doc.workspace.height = h;
            s.touch();
        }
    }
    s.device = Some(p.name);
    ok(r)
}

fn machine_disconnect(s: &mut Session, p: Value) -> Result<Value, CmdError> {
    let p: DevParam = parse(p)?;
    let name = device_name(s, &p.device)?;
    let r = s.machines.lock().expect("urządzenia").disconnect(&name);
    if s.device.as_deref() == Some(&name) {
        s.device = None;
    }
    ok(Removed { removed: r })
}

fn machine_select(s: &mut Session, p: Value) -> Result<Value, CmdError> {
    let p: NameParams = parse(p)?;
    if !s
        .machines
        .lock()
        .expect("urządzenia")
        .connected()
        .contains(&p.name)
    {
        return Err(CmdError::new(
            "no_device",
            format!("urządzenie {} nie jest podłączone", p.name),
        ));
    }
    s.device = Some(p.name);
    done()
}

#[derive(Serialize, JsonSchema)]
struct StatusResult {
    device: String,
    state: bc_drivers::driver::MachineState,
    job: bc_drivers::driver::JobState,
    capabilities: bc_drivers::driver::Caps,
    /// Położenie głowicy w układzie dokumentu (obszar roboczy, mm) wg ustawień startu;
    /// `null` przy starcie od bieżącej pozycji (REQ-03-004).
    head: Option<[f64; 2]>,
}

/// Pozycja robocza maszyny → punkt dokumentu (odwrotność [`to_job_coords`]).
fn head_in_document(s: &mut Session, wpos: [f64; 3]) -> Option<[f64; 2]> {
    match s.doc.start.from {
        StartFrom::Absolute => Some([wpos[0], wpos[1]]),
        StartFrom::UserOrigin => {
            let job = run_plan(s, &PlanJobParams::default()).ok()?;
            let a = bc_drivers::gcode::job_anchor(&job, &s.doc.start);
            Some([wpos[0] + a.x, wpos[1] + a.y])
        }
        StartFrom::Current => None,
    }
}

fn machine_status(s: &mut Session, p: Value) -> Result<Value, CmdError> {
    let p: DevParam = parse(p)?;
    let name = device_name(s, &p.device)?;
    let (state, job, capabilities) = with(s, &p.device, |d| {
        Ok((d.status(), d.job(), d.capabilities()))
    })?;
    let head = head_in_document(s, state.wpos);
    ok(StatusResult {
        device: name,
        state,
        job,
        capabilities,
        head,
    })
}

fn machine_settings(s: &mut Session, p: Value) -> Result<Value, CmdError> {
    let p: DevParam = parse(p)?;
    let v = with(s, &p.device, |d| d.read_settings())?;
    ok(json!({ "settings": v }))
}

fn machine_laser_check(s: &mut Session, p: Value) -> Result<Value, CmdError> {
    let p: DevParam = parse(p)?;
    let prof = profile_of(s, &p.device)?;
    let s_max = with(s, &p.device, |d| {
        Ok(prof.s_max.unwrap_or_else(|| {
            d.status()
                .variant
                .as_deref()
                .and_then(bc_drivers::grbl::codes::variant)
                .map(|v| v.s_max)
                .unwrap_or(1000.0)
        }))
    })?;
    let r = with(s, &p.device, |d| {
        bc_drivers::grbl::check_laser_config(d.as_mut(), s_max)
    })?;
    ok(r)
}

fn machine_console(s: &mut Session, p: Value) -> Result<Value, CmdError> {
    let p: DevParam = parse(p)?;
    let v = with(s, &p.device, |d| Ok(d.console()))?;
    ok(json!({ "lines": v }))
}

/// Lista plików w pamięci sterownika (REQ-13-014, okno REQ-03-042). Urządzenie bez
/// pamięci plików (K40, Moshi, GRBL bez karty SD) daje `supported: false` z powodem,
/// a nie błąd — okno pokazuje komunikat.
fn machine_files(s: &mut Session, p: Value) -> Result<Value, CmdError> {
    let p: DevParam = parse(p)?;
    let r = with(s, &p.device, |d| match d.list_files() {
        Ok(v) => Ok(Ok(v)),
        Err(DriverError::Unsupported(m)) => Ok(Err(m)),
        Err(e) => Err(e),
    })?;
    match r {
        Ok(v) => {
            let total: u64 = v.iter().map(|f| f.size).sum();
            ok(json!({ "supported": true, "files": v, "total_bytes": total }))
        }
        Err(reason) => ok(json!({
            "supported": false, "files": [], "total_bytes": 0, "reason": reason
        })),
    }
}

// ---------- ruch ----------

fn d_jog_speed() -> f64 {
    20.0
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct JogParams {
    axis: Axis,
    /// mm
    distance: f64,
    /// mm/s
    #[serde(default = "d_jog_speed")]
    speed: f64,
    #[serde(default)]
    device: Option<String>,
}

fn device_jog(s: &mut Session, p: Value) -> Result<Value, CmdError> {
    let p: JogParams = parse(p)?;
    with(s, &p.device, |d| d.jog(p.axis, p.distance, p.speed))?;
    done()
}

fn device_jog_cancel(s: &mut Session, p: Value) -> Result<Value, CmdError> {
    let p: DevParam = parse(p)?;
    with(s, &p.device, |d| d.jog_cancel())?;
    done()
}

fn device_home(s: &mut Session, p: Value) -> Result<Value, CmdError> {
    let p: DevParam = parse(p)?;
    with(s, &p.device, |d| d.home())?;
    done()
}

fn device_unlock(s: &mut Session, p: Value) -> Result<Value, CmdError> {
    let p: DevParam = parse(p)?;
    with(s, &p.device, |d| d.unlock())?;
    done()
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct MoveParams {
    #[serde(default)]
    x: Option<f64>,
    #[serde(default)]
    y: Option<f64>,
    #[serde(default)]
    z: Option<f64>,
    /// mm/s (brak = przejazd szybki).
    #[serde(default)]
    speed: Option<f64>,
    #[serde(default)]
    device: Option<String>,
}

fn device_move_to(s: &mut Session, p: Value) -> Result<Value, CmdError> {
    let p: MoveParams = parse(p)?;
    if p.x.is_none() && p.y.is_none() && p.z.is_none() {
        return Err(invalid("podaj x, y albo z"));
    }
    with(s, &p.device, |d| d.move_to(p.x, p.y, p.z, p.speed))?;
    done()
}

fn device_go_origin(s: &mut Session, p: Value) -> Result<Value, CmdError> {
    let p: DevParam = parse(p)?;
    with(s, &p.device, |d| {
        d.move_to(Some(0.0), Some(0.0), None, None)
    })?;
    done()
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct OriginParams {
    #[serde(default = "yes")]
    x: bool,
    #[serde(default = "yes")]
    y: bool,
    #[serde(default)]
    z: bool,
    #[serde(default)]
    device: Option<String>,
}

fn device_set_origin(s: &mut Session, p: Value) -> Result<Value, CmdError> {
    let p: OriginParams = parse(p)?;
    with(s, &p.device, |d| d.set_origin(p.x, p.y, p.z))?;
    done()
}

/// Punkt dokumentu → współrzędne zadania wg ustawień startu.
fn to_job_coords(s: &mut Session, x: f64, y: f64) -> Result<[f64; 2], CmdError> {
    match s.doc.start.from {
        StartFrom::Absolute => Ok([x, y]),
        StartFrom::UserOrigin => {
            let job = run_plan(s, &PlanJobParams::default())?;
            let a = bc_drivers::gcode::job_anchor(&job, &s.doc.start);
            Ok([x - a.x, y - a.y])
        }
        StartFrom::Current => Err(CmdError::new(
            "invalid_state",
            "start od bieżącej pozycji: położenie w obszarze nie ma odpowiednika na maszynie",
        )),
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct PositionParams {
    /// Punkt obszaru roboczego, mm.
    x: f64,
    y: f64,
    #[serde(default)]
    device: Option<String>,
}

fn device_set_position(s: &mut Session, p: Value) -> Result<Value, CmdError> {
    let p: PositionParams = parse(p)?;
    let [x, y] = to_job_coords(s, p.x, p.y)?;
    with(s, &p.device, |d| d.move_to(Some(x), Some(y), None, None))?;
    ok(json!({ "x": x, "y": y }))
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct ToSelectionParams {
    /// Punkt ramki zaznaczenia (domyślnie środek).
    #[serde(default)]
    anchor: Option<JobOrigin>,
    #[serde(default)]
    device: Option<String>,
}

fn device_move_to_selection(s: &mut Session, p: Value) -> Result<Value, CmdError> {
    let p: ToSelectionParams = parse(p)?;
    if s.selection.is_empty() {
        return Err(invalid("brak zaznaczonych obiektów"));
    }
    let rep = crate::inspect::report(s, crate::inspect::Detail::Summary)?;
    let mut b: Option<[f64; 4]> = None;
    for o in rep.objects.iter().filter(|o| s.selection.contains(&o.id)) {
        if let Some(ob) = o.bbox {
            b = Some(match b {
                None => ob,
                Some(c) => [
                    c[0].min(ob[0]),
                    c[1].min(ob[1]),
                    c[2].max(ob[2]),
                    c[3].max(ob[3]),
                ],
            });
        }
    }
    let [x0, y0, x1, y1] = b.ok_or_else(|| invalid("zaznaczenie bez geometrii"))?;
    let (fx, fy) = p.anchor.unwrap_or(JobOrigin::Center).fractions();
    let [x, y] = to_job_coords(s, x0 + (x1 - x0) * fx, y0 + (y1 - y0) * fy)?;
    with(s, &p.device, |d| d.move_to(Some(x), Some(y), None, None))?;
    ok(json!({ "x": x, "y": y }))
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct FrameParams {
    /// rect — prostokąt, contour — kontur (rubber band).
    #[serde(default = "d_frame")]
    kind: FrameKind,
    /// Czerwony laser pozycjonujący z korektą offsetu (polecenia z profilu).
    #[serde(default)]
    pointer: bool,
    #[serde(default)]
    selection_only: bool,
    /// mm/s (brak = z profilu).
    #[serde(default)]
    speed: Option<f64>,
    #[serde(default)]
    device: Option<String>,
}

fn d_frame() -> FrameKind {
    FrameKind::Rect
}

fn device_frame(s: &mut Session, p: Value) -> Result<Value, CmdError> {
    let p: FrameParams = parse(p)?;
    let prof = profile_of(s, &p.device)?;
    let job = run_plan(
        s,
        &PlanJobParams {
            selection_only: p.selection_only,
            ..Default::default()
        },
    )?;
    let mut pts = bc_drivers::gcode::frame_points(&job, &s.doc.start, p.kind == FrameKind::Contour);
    if pts.is_empty() {
        return Err(invalid("zadanie bez cięcia — brak ramki"));
    }
    let start = s.doc.start.from;
    let speed = p.speed.unwrap_or(prof.frame_speed);
    let pointer = p.pointer;
    let n = pts.len();
    with(s, &p.device, |d| {
        if start == StartFrom::Current {
            // ramka względem bieżącej pozycji głowicy
            let w = d.status().wpos;
            for q in &mut pts {
                q[0] += w[0];
                q[1] += w[1];
            }
        }
        let ptr = pointer.then_some((
            prof.pointer_on.as_str(),
            prof.pointer_off.as_str(),
            prof.pointer_offset,
        ));
        d.frame(&pts, speed, ptr)
    })?;
    ok(json!({ "points": n }))
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct TestFireParams {
    /// 0–100 %
    power: f64,
    /// ms (1–5000)
    ms: u32,
    #[serde(default)]
    device: Option<String>,
}

fn device_test_fire(s: &mut Session, p: Value) -> Result<Value, CmdError> {
    let p: TestFireParams = parse(p)?;
    with(s, &p.device, |d| d.test_fire(p.power, p.ms))?;
    done()
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct OverrideParams {
    /// Posuw, % (10–200).
    #[serde(default)]
    feed: Option<u32>,
    /// Moc, % (10–200).
    #[serde(default)]
    power: Option<u32>,
    #[serde(default)]
    device: Option<String>,
}

fn device_override(s: &mut Session, p: Value) -> Result<Value, CmdError> {
    let p: OverrideParams = parse(p)?;
    with(s, &p.device, |d| d.set_overrides(p.feed, p.power))?;
    done()
}

fn d_probe_feed() -> f64 {
    2.0
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct ProbeParams {
    /// Największy ruch w dół, mm.
    distance: f64,
    /// mm/s
    #[serde(default = "d_probe_feed")]
    feed: f64,
    /// Ustaw zero Z na powierzchni + grubość płytki sondy (mm).
    #[serde(default)]
    set_zero: Option<f64>,
    #[serde(default)]
    device: Option<String>,
}

fn device_probe_z(s: &mut Session, p: Value) -> Result<Value, CmdError> {
    let p: ProbeParams = parse(p)?;
    let z = with(s, &p.device, |d| {
        let z = d.probe_z(p.distance, p.feed)?;
        if let Some(t) = p.set_zero {
            d.raw(&format!(
                "G10 L20 P1 Z{}",
                bc_drivers::gcode::fmt_num(t, 3, true)
            ))?;
        }
        Ok(z)
    })?;
    ok(json!({ "z": z }))
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct SettingsWriteParams {
    /// Zmiany, np. {"$110": "8000", "$32": "1"}.
    changes: BTreeMap<String, String>,
    /// Tylko diff i walidacja, bez zapisu.
    #[serde(default)]
    dry_run: bool,
    #[serde(default)]
    device: Option<String>,
}

fn device_settings_write(s: &mut Session, p: Value) -> Result<Value, CmdError> {
    let p: SettingsWriteParams = parse(p)?;
    let backup = s.machines.lock().expect("urządzenia").backup_dir();
    let changes: Vec<(String, String)> = p.changes.into_iter().collect();
    let r = with(s, &p.device, |d| {
        d.write_settings(&changes, backup.as_deref(), p.dry_run)
    })?;
    if !p.dry_run {
        // nowe $110/$111/$120/$121 → czas zadania z nowej kinematyki
        let name = device_name(s, &p.device)?;
        s.machines
            .lock()
            .expect("urządzenia")
            .refresh_kinematics(&name);
    }
    ok(r)
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct RawParams {
    /// Jedno polecenie sterownika (np. $I, G0 X10).
    line: String,
    #[serde(default)]
    device: Option<String>,
}

fn device_raw(s: &mut Session, p: Value) -> Result<Value, CmdError> {
    let p: RawParams = parse(p)?;
    let r = with(s, &p.device, |d| d.raw(&p.line))?;
    ok(json!({ "response": r }))
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct SdUploadParams {
    /// Nazwa pliku na karcie (np. /sd/zadanie.gcode).
    name: String,
    /// Tylko zaznaczone obiekty (wysyłka projektu).
    #[serde(default)]
    selection_only: bool,
    /// Treść pliku z dysku (G-code) — wysyłana bez kompilacji; bez pola wysyłany jest
    /// projekt skompilowany przez sterownik urządzenia.
    #[serde(default)]
    text: Option<String>,
    #[serde(default)]
    device: Option<String>,
}

/// Największy plik wysyłany z dysku przez `device.sd_upload {text}`.
const MAX_UPLOAD_TEXT: usize = 16 * 1024 * 1024;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct SdFileParams {
    /// Nazwa pliku na karcie (np. /sd/zadanie.gcode).
    name: String,
    #[serde(default)]
    device: Option<String>,
}

fn device_sd_upload(s: &mut Session, p: Value) -> Result<Value, CmdError> {
    let p: SdUploadParams = parse(p)?;
    bc_drivers::driver::check_file_name(&p.name)
        .map_err(|e| CmdError::new("invalid_params", e.to_string()))?;
    let (bytes, source) = match p.text {
        Some(t) => {
            if t.is_empty() {
                return Err(CmdError::new("invalid_params", "text: pusty plik"));
            }
            if t.len() > MAX_UPLOAD_TEXT {
                return Err(CmdError::new(
                    "invalid_params",
                    format!(
                        "text: plik większy niż {} MB",
                        MAX_UPLOAD_TEXT / (1024 * 1024)
                    ),
                ));
            }
            (t.into_bytes(), "text")
        }
        None => (
            compile_for(s, &p.device, p.selection_only, Source::Document)?.bytes,
            "document",
        ),
    };
    let n = bytes.len();
    with(s, &p.device, |d| d.upload_file(&p.name, &bytes))?;
    ok(json!({ "name": p.name, "bytes": n, "source": source }))
}

fn device_sd_run(s: &mut Session, p: Value) -> Result<Value, CmdError> {
    let p: SdFileParams = parse(p)?;
    with(s, &p.device, |d| d.run_file(&p.name))?;
    done()
}

fn device_sd_delete(s: &mut Session, p: Value) -> Result<Value, CmdError> {
    let p: SdFileParams = parse(p)?;
    with(s, &p.device, |d| d.delete_file(&p.name))?;
    done()
}

// ---------- zadania ----------

#[derive(Deserialize, JsonSchema, Default, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum Source {
    /// Zadanie z dokumentu (planer + kompilator sterownika).
    #[default]
    Document,
    /// Program wczytany przez gcode.import.
    Gcode,
}

fn compile_for(
    s: &mut Session,
    d: &Option<String>,
    selection_only: bool,
    src: Source,
) -> Result<MachineFile, CmdError> {
    compile_job(s, d, selection_only, src, None).map(|(f, _)| f)
}

/// Kompilacja zadania (z planem dla dokumentu); `from_segment` — od segmentu
/// podglądu (REQ-09-013).
fn compile_job(
    s: &mut Session,
    d: &Option<String>,
    selection_only: bool,
    src: Source,
    from_segment: Option<usize>,
) -> Result<(MachineFile, Option<bc_plan::Job>), CmdError> {
    if from_segment.is_some() && src == Source::Gcode {
        return Err(invalid("from_segment: tylko dla zadania z dokumentu"));
    }
    match src {
        Source::Gcode => {
            let g = s.gcode.as_ref().ok_or_else(|| {
                CmdError::new("not_found", "brak wczytanego programu — gcode.import")
            })?;
            let text = g.text.to_string();
            let lines = text.lines().count();
            Ok((
                MachineFile {
                    driver: "gcode".into(),
                    extension: "gcode",
                    lines,
                    bytes: text.into_bytes(),
                    hash: String::new(),
                    time_s: g.report.time_s,
                    warnings: Vec::new(),
                },
                None,
            ))
        }
        Source::Document => {
            let mut job = run_plan(
                s,
                &PlanJobParams {
                    selection_only,
                    ..Default::default()
                },
            )?;
            if let Some(n) = from_segment {
                job = from_segment_of(&job, n)?;
            }
            let ctx = context_of(s);
            let f = with(s, d, |dr| dr.compile(&job, &ctx))?;
            Ok((f, Some(job)))
        }
    }
}

/// Zadanie od segmentu podglądu (REQ-09-013).
pub(crate) fn from_segment_of(job: &bc_plan::Job, n: usize) -> Result<bc_plan::Job, CmdError> {
    job.from_segment(n).ok_or_else(|| {
        invalid(format!(
            "from_segment: {n} poza zakresem (segmentów: {})",
            job.segments().len()
        ))
    })
}

/// Przypisanie wierszy strumienia do przebiegów zadania: licznik „przejście 3/5”
/// (REQ-03-054). Wiersze liczone jak w sterowniku GRBL (bez pustych i komentarzy);
/// przebieg zaczyna komentarz z jego tytułem wstawiany przez kompilator.
#[derive(Debug, Clone)]
pub struct JobMap {
    device: String,
    job_id: u64,
    ops: usize,
    /// (pierwszy wiersz, warstwa, przejście od 1, przejść warstwy)
    rows: Vec<(usize, u8, u32, u32)>,
}

fn job_map(device: &str, job_id: u64, f: &MachineFile, job: &bc_plan::Job) -> JobMap {
    let passes_of = |layer: u8, sub: u32| {
        job.ops
            .iter()
            .filter(|o| o.layer == layer && o.sublayer == sub)
            .count() as u32
    };
    let mut rows = Vec::new();
    let mut k = 0;
    let mut kept = 0usize;
    for l in f.stream_lines().map(str::trim) {
        let comment = l.starts_with(';') || (l.starts_with('(') && l.ends_with(')'));
        if comment {
            if let Some(op) = job.ops.get(k) {
                if l.contains(&format!("{} - pass {}", op.label, op.pass + 1)) {
                    rows.push((
                        kept,
                        op.layer,
                        op.pass + 1,
                        passes_of(op.layer, op.sublayer),
                    ));
                    k += 1;
                }
            }
        } else if !l.is_empty() {
            kept += 1;
        }
    }
    JobMap {
        device: device.to_string(),
        job_id,
        ops: job.ops.len(),
        rows,
    }
}

fn one() -> u32 {
    1
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct StartParams {
    #[serde(default)]
    source: Source,
    #[serde(default)]
    selection_only: bool,
    /// Powtórzenia (REQ-10-021).
    #[serde(default = "one")]
    repeat: u32,
    /// Opóźnienie między powtórzeniami, s.
    #[serde(default)]
    delay_s: f64,
    /// Dodaj do kolejki, gdy zadanie trwa (REQ-10-020).
    #[serde(default)]
    queue: bool,
    #[serde(default)]
    name: Option<String>,
    /// „Rozpocznij od tego miejsca”: indeks segmentu z `plan.preview` (REQ-09-013).
    #[serde(default)]
    from_segment: Option<usize>,
    #[serde(default)]
    device: Option<String>,
}

fn job_start(s: &mut Session, p: Value) -> Result<Value, CmdError> {
    let p: StartParams = parse(p)?;
    if p.repeat == 0 || p.repeat > 1000 || !(p.delay_s >= 0.0 && p.delay_s <= 3600.0) {
        return Err(invalid("repeat: 1–1000, delay_s: 0–3600"));
    }
    let (f, job) = compile_job(s, &p.device, p.selection_only, p.source, p.from_segment)?;
    let dev = device_name(s, &p.device)?;
    let opts = JobOptions {
        name: p.name.clone().unwrap_or_else(|| s.doc.name.clone()),
        repeat: p.repeat,
        delay_s: p.delay_s,
    };
    let queue = p.queue;
    let r = with(s, &p.device, |d| {
        if queue {
            d.queue_job(&f, opts)?;
            Ok(d.job())
        } else {
            d.send_job(&f, opts)
        }
    })?;
    if !queue {
        s.job_map = job.map(|j| job_map(&dev, r.id, &f, &j));
    }
    ok(r)
}

fn job_pause(s: &mut Session, p: Value) -> Result<Value, CmdError> {
    let p: DevParam = parse(p)?;
    with(s, &p.device, |d| d.pause())?;
    done()
}

fn job_resume(s: &mut Session, p: Value) -> Result<Value, CmdError> {
    let p: DevParam = parse(p)?;
    with(s, &p.device, |d| d.resume())?;
    done()
}

fn job_stop(s: &mut Session, p: Value) -> Result<Value, CmdError> {
    let p: DevParam = parse(if p.is_null() { json!({}) } else { p })?;
    // stop zawsze dozwolony: bez wybranego urządzenia zatrzymuje wszystkie podłączone
    let names = match device_name(s, &p.device) {
        Ok(n) => vec![n],
        Err(_) => s.machines.lock().expect("urządzenia").connected(),
    };
    let mut m = s.machines.lock().expect("urządzenia");
    for n in &names {
        if let Some(d) = m.driver(n) {
            d.stop();
        }
    }
    ok(json!({ "stopped": names }))
}

fn job_status(s: &mut Session, p: Value) -> Result<Value, CmdError> {
    let p: DevParam = parse(p)?;
    let dev = device_name(s, &p.device)?;
    let j = with(s, &p.device, |d| Ok(d.job()))?;
    let mut v = ok(&j)?;
    let rounds = j.repeat.max(1) as f64;
    let done = j.status == bc_drivers::driver::JobStatus::Done;
    let percent = if done {
        100.0
    } else if j.total == 0 {
        0.0
    } else {
        (j.acked as f64 / (j.total as f64 * rounds) * 100.0).clamp(0.0, 100.0)
    };
    let o = v.as_object_mut().expect("obiekt");
    o.insert("percent".into(), json!(percent));
    // czas zadania, także po zakończeniu (REQ-03-053)
    o.insert("elapsed".into(), json!(crate::plan::hms(j.elapsed_s)));
    // licznik przejść (REQ-03-054)
    if let Some(m) = s
        .job_map
        .as_ref()
        .filter(|m| m.device == dev && m.job_id == j.id && !m.rows.is_empty())
    {
        let line = if done || j.total == 0 {
            usize::MAX
        } else {
            j.acked.saturating_sub(1) % j.total
        };
        let row = m
            .rows
            .iter()
            .rev()
            .find(|r| r.0 <= line)
            .unwrap_or(&m.rows[0]);
        o.insert("layer".into(), json!(row.1));
        o.insert("pass".into(), json!(row.2));
        o.insert("passes".into(), json!(row.3));
        o.insert("ops".into(), json!(m.ops));
    }
    Ok(v)
}

// ---------- zapamiętane pozycje (okno Przesuń) ----------

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct PositionSaveParams {
    name: String,
    /// mm; brak = bieżąca pozycja robocza głowicy.
    #[serde(default)]
    x: Option<f64>,
    #[serde(default)]
    y: Option<f64>,
    #[serde(default)]
    device: Option<String>,
}

fn device_positions(s: &mut Session, _: Value) -> Result<Value, CmdError> {
    let m = s.machines.lock().expect("urządzenia");
    ok(json!({ "positions": m.positions() }))
}

fn device_position_save(s: &mut Session, p: Value) -> Result<Value, CmdError> {
    let p: PositionSaveParams = parse(p)?;
    if p.name.trim().is_empty() {
        return Err(invalid("name: nazwa pozycji"));
    }
    let (x, y) = match (p.x, p.y) {
        (Some(x), Some(y)) => (x, y),
        (None, None) => {
            let w = with(s, &p.device, |d| Ok(d.status().wpos))?;
            (w[0], w[1])
        }
        _ => return Err(invalid("podaj x i y albo żadne (bieżąca pozycja)")),
    };
    let pos = bc_drivers::machines::SavedPosition {
        name: p.name.trim().to_string(),
        x,
        y,
    };
    s.machines
        .lock()
        .expect("urządzenia")
        .save_position(pos.clone())
        .map_err(invalid)?;
    ok(pos)
}

fn device_position_delete(s: &mut Session, p: Value) -> Result<Value, CmdError> {
    let p: NameParams = parse(p)?;
    let removed = s
        .machines
        .lock()
        .expect("urządzenia")
        .delete_position(&p.name)
        .map_err(invalid)?;
    ok(Removed { removed })
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct PositionGoParams {
    name: String,
    /// mm/s (brak = przejazd szybki).
    #[serde(default)]
    speed: Option<f64>,
    #[serde(default)]
    device: Option<String>,
}

fn device_position_go(s: &mut Session, p: Value) -> Result<Value, CmdError> {
    let p: PositionGoParams = parse(p)?;
    let pos = s
        .machines
        .lock()
        .expect("urządzenia")
        .positions()
        .iter()
        .find(|q| q.name == p.name)
        .cloned()
        .ok_or_else(|| CmdError::new("not_found", format!("nieznana pozycja {}", p.name)))?;
    with(s, &p.device, |d| {
        d.move_to(Some(pos.x), Some(pos.y), None, p.speed)
    })?;
    ok(pos)
}

// ---------- wyrównanie do pozycji lasera ----------

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct AlignLaserParams {
    /// Obiekty (brak = zaznaczenie).
    #[serde(default)]
    ids: Option<Vec<bc_doc::ObjectId>>,
    /// Punkt ramki obiektów przenoszony na pozycję lasera (domyślnie środek).
    #[serde(default)]
    anchor: Option<JobOrigin>,
    #[serde(default)]
    device: Option<String>,
}

fn object_align_to_laser(s: &mut Session, p: Value) -> Result<Value, CmdError> {
    let p: AlignLaserParams = parse(p)?;
    let ids = s.targets(&p.ids)?;
    let wpos = with(s, &p.device, |d| Ok(d.status().wpos))?;
    let [hx, hy] = head_in_document(s, wpos).ok_or_else(|| {
        CmdError::new(
            "invalid_state",
            "start od bieżącej pozycji: położenie lasera w obszarze nieokreślone",
        )
    })?;
    let rep = crate::inspect::report(s, crate::inspect::Detail::Summary)?;
    let mut b: Option<[f64; 4]> = None;
    for o in rep.objects.iter().filter(|o| ids.contains(&o.id)) {
        if let Some(ob) = o.bbox {
            b = Some(match b {
                None => ob,
                Some(c) => [
                    c[0].min(ob[0]),
                    c[1].min(ob[1]),
                    c[2].max(ob[2]),
                    c[3].max(ob[3]),
                ],
            });
        }
    }
    let [x0, y0, x1, y1] = b.ok_or_else(|| invalid("obiekty bez geometrii"))?;
    let (fx, fy) = p.anchor.unwrap_or(JobOrigin::Center).fractions();
    let (dx, dy) = (hx - (x0 + (x1 - x0) * fx), hy - (y0 + (y1 - y0) * fy));
    let Some(c) = registry().get("object.transform") else {
        return Err(CmdError::new("failed", "brak object.transform"));
    };
    let crate::Exec::Session(tr) = c.exec else {
        return Err(CmdError::new("failed", "object.transform"));
    };
    tr(s, json!({ "ids": ids, "translate": [dx, dy] }))?;
    ok(json!({ "translate": [dx, dy], "laser": [hx, hy] }))
}

// ---------- makra ----------

fn registry() -> &'static Registry {
    static R: OnceLock<Registry> = OnceLock::new();
    R.get_or_init(Registry::builtin)
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct MacroParams {
    #[serde(rename = "macro")]
    m: Macro,
}

fn macro_save(s: &mut Session, p: Value) -> Result<Value, CmdError> {
    let p: MacroParams = parse(p)?;
    for st in &p.m.steps {
        if let MacroStep::Command { id, .. } = st {
            if registry().get(id).is_none() {
                return Err(invalid(format!("nieznane polecenie {id}")));
            }
        }
    }
    s.machines
        .lock()
        .expect("urządzenia")
        .save_macro(p.m.clone())
        .map_err(invalid)?;
    ok(p.m)
}

fn macro_list(s: &mut Session, _: Value) -> Result<Value, CmdError> {
    let m = s.machines.lock().expect("urządzenia");
    ok(json!({ "macros": m.macros() }))
}

fn macro_delete(s: &mut Session, p: Value) -> Result<Value, CmdError> {
    let p: NameParams = parse(p)?;
    let removed = s
        .machines
        .lock()
        .expect("urządzenia")
        .delete_macro(&p.name)
        .map_err(invalid)?;
    ok(Removed { removed })
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct MacroRunParams {
    name: String,
    #[serde(default)]
    device: Option<String>,
}

fn macro_run(s: &mut Session, p: Value) -> Result<Value, CmdError> {
    let p: MacroRunParams = parse(p)?;
    let m = s
        .machines
        .lock()
        .expect("urządzenia")
        .macros()
        .iter()
        .find(|m| m.name == p.name)
        .cloned()
        .ok_or_else(|| CmdError::new("not_found", format!("nieznane makro {}", p.name)))?;
    let mut results = Vec::new();
    let n = m.steps.len();
    for (i, st) in m.steps.iter().enumerate() {
        s.tick(i, n)?;
        let r = match st {
            MacroStep::Command { id, params } => {
                let c = registry()
                    .get(id)
                    .ok_or_else(|| invalid(format!("nieznane polecenie {id}")))?;
                if c.spec.executor != Executor::Document {
                    return Err(invalid(format!(
                        "{id}: w makrze tylko polecenia dokumentu i urządzeń"
                    )));
                }
                s.execute(registry(), id, params.clone())
            }
            MacroStep::Raw { line } => {
                let l = line.clone();
                with(s, &p.device, |d| d.raw(&l)).map(|r| json!({ "response": r }))
            }
        };
        match r {
            Ok(v) => results.push(v),
            Err(e) => {
                return Err(CmdError::new(
                    e.code,
                    format!("makro {}, krok {}: {}", m.name, i + 1, e.message),
                ))
            }
        }
    }
    ok(json!({ "steps": results.len(), "results": results }))
}

// ---------- rejestracja ----------

#[allow(clippy::too_many_arguments)]
fn reg(
    r: &mut Registry,
    id: &'static str,
    title: &'static str,
    description: &'static str,
    tier: SafetyTier,
    requires_device: bool,
    params_schema: Value,
    requirements: &'static [&'static str],
    handler: Handler,
) {
    r.register(
        CommandSpec {
            executor: Executor::Document,
            id,
            title,
            description,
            params_schema,
            result_schema: json!({"type": "object"}),
            undoable: false,
            requires_device,
            safety_tier: tier,
            shortcut: None,
            requirements,
        },
        handler,
    );
}

pub(crate) fn register_all(r: &mut Registry) {
    use SafetyTier::*;
    let dev = schema_of::<DevParam>;
    reg(r, "machine.list", "Urządzenia", "Profile urządzeń (z urządzeniami fikcyjnymi „Symulator”, „Symulator K40 (Lihuiyu)”, „Symulator Moshi”), podłączone urządzenia, wybrane urządzenie, odmiany GRBL i warianty płyt K40 (boards).", Read, false, dev(), &["REQ-13-191", "REQ-13-196", "REQ-13-046", "REQ-19-009"], machine_list);
    reg(r, "machine.profile_save", "Zapisz profil urządzenia", "Dodaje albo zastępuje profil urządzenia (nazwa, sterownik grbl / lihuiyu (K40) / moshi, odmiana, transport serial/tcp/websocket/usb/ch341_dll/emulator, obszar, tryb strumieniowania, s_max, czerwony laser, prędkość ramki, k40: płyta, ID płyty, dpi, korekcja skali); zapis w devices.json katalogu preferencji.", Design, false, schema_of::<ProfileParams>(), &["REQ-13-196"], profile_save);
    reg(
        r,
        "machine.profile_delete",
        "Usuń profil urządzenia",
        "Usuwa profil urządzenia.",
        Design,
        false,
        schema_of::<NameParams>(),
        &["REQ-13-196"],
        profile_delete,
    );
    reg(r, "machine.find", "Znajdź mój laser", "Kreator: lista portów szeregowych z VID/PID (FTDI, CH340, CP210x) i próba połączenia GRBL na kolejnych prędkościach; wynik z gotowymi profilami. Pole usb: urządzenia USB znanych rodzin laserów (K40 CH341, galvo BJJCZ) ze stanem sterownika i przewodnikiem.", Read, false, schema_of::<FindParams>(), &["REQ-13-192", "REQ-13-018", "REQ-13-193"], machine_find);
    reg(r, "machine.usb_guide", "Sterownik USB", "Przewodnik dostępu do urządzenia USB: instalacja sterownika WinUSB narzędziem Zadig na Windows (albo sterownik producenta CH341), reguły udev na Linuksie; ok = urządzenie gotowe.", Read, false, schema_of::<UsbGuideParams>(), &["REQ-13-020", "REQ-13-021", "REQ-13-019"], machine_usb_guide);
    reg(r, "machine.connect", "Połącz", "Łączy profil z urządzeniem (rozpoznanie odmiany GRBL; K40: USB CH341 natywnie albo sterownik CH341DLL, potwierdzenie ID płyty) i wybiera je w sesji. Kilka urządzeń może być połączonych naraz.", Design, false, schema_of::<ConnectParams>(), &["REQ-13-002", "REQ-13-028", "REQ-13-072", "REQ-13-073", "REQ-13-074", "REQ-13-039", "REQ-13-040", "REQ-13-046"], machine_connect);
    reg(
        r,
        "machine.disconnect",
        "Rozłącz",
        "Rozłącza urządzenie (domyślnie wybrane).",
        Design,
        false,
        dev(),
        &["REQ-13-002"],
        machine_disconnect,
    );
    reg(
        r,
        "machine.select",
        "Wybierz urządzenie",
        "Wybiera podłączone urządzenie dla poleceń bez parametru device.",
        Design,
        false,
        schema_of::<NameParams>(),
        &["REQ-13-028"],
        machine_select,
    );
    reg(r, "machine.status", "Stan urządzenia", "Stan maszyny (pozycja, stan, bufory, nadpisania, pokrywa, alarm, błędy), postęp zadania i możliwości urządzenia.", Read, true, dev(), &["REQ-13-001", "REQ-13-003", "REQ-13-077"], machine_status);
    reg(
        r,
        "machine.settings",
        "Ustawienia maszyny",
        "Odczyt ustawień sterownika ($$) z opisami, jednostkami i zakresami.",
        Read,
        true,
        dev(),
        &["REQ-13-013", "REQ-13-082", "REQ-13-198"],
        machine_settings,
    );
    reg(r, "machine.laser_check", "Kreator konfiguracji lasera", "Sprawdza tryb laserowy ($32=1) i $30 (S-max) względem profilu; zwraca problemy i proponowane zmiany do device.settings_write.", Read, true, dev(), &["REQ-13-084"], machine_laser_check);
    reg(
        r,
        "machine.console",
        "Konsola",
        "Ostatnie wiersze wymiany ze sterownikiem (> wysłane, < odebrane).",
        Read,
        true,
        dev(),
        &["REQ-13-003"],
        machine_console,
    );
    reg(
        r,
        "machine.files",
        "Pliki na sterowniku",
        "Lista plików w pamięci sterownika (karta SD FluidNC) z rozmiarem i sumą; urządzenie bez pamięci plików (K40, Moshi, GRBL bez karty) daje supported: false z powodem.",
        Read,
        true,
        dev(),
        &["REQ-10-018", "REQ-13-014", "REQ-03-042"],
        machine_files,
    );
    reg(
        r,
        "device.jog",
        "Przesuń głowicę",
        "Jog: przesuwa oś o zadaną odległość (GRBL 1.1: $J=, 0.9: G91 G1).",
        Motion,
        true,
        schema_of::<JogParams>(),
        &["REQ-13-005", "REQ-13-079", "REQ-15-092"],
        device_jog,
    );
    reg(
        r,
        "device.jog_cancel",
        "Zatrzymaj jog",
        "Anuluje trwający jog (0x85).",
        Motion,
        true,
        dev(),
        &["REQ-13-079"],
        device_jog_cancel,
    );
    reg(
        r,
        "device.home",
        "Bazowanie",
        "Bazowanie osi maszyny ($H).",
        Motion,
        true,
        dev(),
        &["REQ-13-004", "REQ-13-080", "REQ-10-014", "REQ-15-092"],
        device_home,
    );
    reg(
        r,
        "device.unlock",
        "Odblokuj",
        "Odblokowanie po alarmie ($X).",
        Motion,
        true,
        dev(),
        &["REQ-13-081"],
        device_unlock,
    );
    reg(
        r,
        "device.move_to",
        "Przesuń do punktu",
        "Ruch do punktu w układzie roboczym (mm; speed mm/s albo przejazd szybki).",
        Motion,
        true,
        schema_of::<MoveParams>(),
        &["REQ-13-006"],
        device_move_to,
    );
    reg(
        r,
        "device.go_origin",
        "Idź do zera",
        "Przejazd do zera układu roboczego (X0 Y0).",
        Motion,
        true,
        dev(),
        &["REQ-10-013"],
        device_go_origin,
    );
    reg(
        r,
        "device.set_origin",
        "Ustaw zero",
        "Zero użytkownika w bieżącej pozycji głowicy (G10 L20 P1).",
        Motion,
        true,
        schema_of::<OriginParams>(),
        &["REQ-13-007", "REQ-10-012"],
        device_set_origin,
    );
    reg(r, "device.set_position", "Ustaw pozycję lasera", "Przesuwa laser do punktu klikniętego w obszarze roboczym (współrzędne dokumentu przeliczone wg ustawień startu).", Motion, true, schema_of::<PositionParams>(), &["REQ-10-016"], device_set_position);
    reg(
        r,
        "device.move_to_selection",
        "Przesuń laser do zaznaczenia",
        "Przesuwa laser do punktu ramki zaznaczenia (domyślnie środek).",
        Motion,
        true,
        schema_of::<ToSelectionParams>(),
        &["REQ-10-015"],
        device_move_to_selection,
    );
    reg(r, "device.frame", "Ramka", "Obrys zadania bez wiązki: prostokąt albo kontur (rubber band); z pointer — czerwonym laserem z korektą offsetu z profilu.", Motion, true, schema_of::<FrameParams>(), &["REQ-13-008", "REQ-10-007", "REQ-10-008", "REQ-10-010"], device_frame);
    reg(
        r,
        "device.test_fire",
        "Test wiązki",
        "Krótki impuls lasera (moc %, czas ms).",
        Fire,
        true,
        schema_of::<TestFireParams>(),
        &["REQ-13-009", "REQ-15-092"],
        device_test_fire,
    );
    reg(
        r,
        "device.override",
        "Nadpisania",
        "Nadpisania czasu rzeczywistego: posuw i moc (10–200 %).",
        Motion,
        true,
        schema_of::<OverrideParams>(),
        &["REQ-13-078"],
        device_override,
    );
    reg(
        r,
        "device.probe_z",
        "Sonda Z",
        "Sonda Z (G38.2): ruch w dół do kontaktu; set_zero ustawia zero Z z grubością płytki.",
        Motion,
        true,
        schema_of::<ProbeParams>(),
        &["REQ-11-028"],
        device_probe_z,
    );
    reg(r, "device.settings_write", "Zapisz ustawienia maszyny", "Zapis ustawień sterownika ($N=v) z walidacją zakresów, diffem i kopią przed zapisem (machine-backups w katalogu preferencji); dry_run — tylko diff.", Config, true, schema_of::<SettingsWriteParams>(), &["REQ-13-013", "REQ-13-082", "REQ-13-198"], device_settings_write);
    reg(
        r,
        "device.raw",
        "Surowe polecenie",
        "Wysyła jedno surowe polecenie do sterownika i zwraca odpowiedź.",
        Fire,
        true,
        schema_of::<RawParams>(),
        &["REQ-01-041"],
        device_raw,
    );
    reg(
        r,
        "device.sd_upload",
        "Zapisz na kartę SD",
        "Zapisuje plik na karcie SD sterownika (FluidNC, XMODEM): projekt skompilowany przez sterownik urządzenia albo treść pliku z dysku (text).",
        Config,
        true,
        schema_of::<SdUploadParams>(),
        &["REQ-10-018", "REQ-13-014", "REQ-03-042"],
        device_sd_upload,
    );
    reg(
        r,
        "device.sd_run",
        "Uruchom plik z karty SD",
        "Uruchamia plik z karty SD sterownika.",
        Fire,
        true,
        schema_of::<SdFileParams>(),
        &["REQ-10-018", "REQ-13-014", "REQ-03-042"],
        device_sd_run,
    );
    reg(
        r,
        "device.sd_delete",
        "Usuń plik z karty SD",
        "Usuwa plik z karty SD sterownika.",
        Config,
        true,
        schema_of::<SdFileParams>(),
        &["REQ-10-018", "REQ-13-014", "REQ-03-042"],
        device_sd_delete,
    );
    reg(r, "job.start", "Start zadania", "Planuje, kompiluje (sterownik urządzenia) i wysyła zadanie z dokumentu albo wczytany program G-code; repeat i delay_s — powtórzenia, queue — kolejka zadań.", Fire, true, schema_of::<StartParams>(), &["REQ-13-010", "REQ-10-001", "REQ-10-002", "REQ-10-003", "REQ-10-004", "REQ-10-020", "REQ-10-021", "REQ-02-089"], job_start);
    reg(
        r,
        "job.pause",
        "Pauza",
        "Wstrzymanie zadania (feed hold).",
        Motion,
        true,
        dev(),
        &["REQ-13-011"],
        job_pause,
    );
    reg(
        r,
        "job.resume",
        "Wznów",
        "Wznowienie zadania (cycle start); odmowa przy otwartej pokrywie.",
        Motion,
        true,
        dev(),
        &["REQ-13-011"],
        job_resume,
    );
    reg(r, "job.stop", "Stop", "Natychmiastowe zatrzymanie (wstrzymanie + reset, wiązka wyłączona). Zawsze dozwolone; bez urządzenia zatrzymuje wszystkie podłączone.", Read, false, dev(), &["REQ-13-012"], job_stop);
    reg(r, "job.status", "Postęp zadania", "Stan zadania na urządzeniu: wiersze wysłane i potwierdzone, rundy, kolejka, błędy, ostrzeżenia (buffer_underrun).", Read, true, dev(), &["REQ-13-010", "REQ-16-005"], job_status);
    reg(r, "machine.positions", "Zapamiętane pozycje", "Lista zapamiętanych pozycji XY z nazwami (okno Przesuń; positions.json w katalogu preferencji).", Read, false, json!({"type": "object", "properties": {}, "additionalProperties": false}), &["REQ-03-039"], device_positions);
    reg(r, "machine.position_save", "Zapamiętaj pozycję", "Zapisuje pozycję XY z nazwą: podane x, y albo bieżąca pozycja głowicy; ta sama nazwa zastępuje pozycję.", Design, false, schema_of::<PositionSaveParams>(), &["REQ-03-039"], device_position_save);
    reg(
        r,
        "machine.position_delete",
        "Usuń zapamiętaną pozycję",
        "Usuwa zapamiętaną pozycję o podanej nazwie.",
        Design,
        false,
        schema_of::<NameParams>(),
        &["REQ-03-039"],
        device_position_delete,
    );
    reg(
        r,
        "device.position_go",
        "Idź do zapamiętanej pozycji",
        "Przesuwa głowicę do zapamiętanej pozycji XY o podanej nazwie.",
        Motion,
        true,
        schema_of::<PositionGoParams>(),
        &["REQ-03-039"],
        device_position_go,
    );
    r.register(
        CommandSpec {
            executor: Executor::Document,
            id: "object.align_to_laser",
            title: "Wyrównaj do pozycji lasera",
            description: "Przesuwa obiekty (ids albo zaznaczenie) tak, by punkt ich ramki (anchor, domyślnie środek) znalazł się w bieżącym położeniu lasera w obszarze roboczym (wg ustawień startu).",
            params_schema: schema_of::<AlignLaserParams>(),
            result_schema: json!({"type": "object"}),
            undoable: true,
            requires_device: true,
            safety_tier: Design,
            shortcut: None,
            requirements: &["REQ-04-089"],
        },
        object_align_to_laser,
    );
    reg(r, "macro.save", "Zapisz makro", "Makro: sekwencja kroków {kind: command, id, params} albo {kind: raw, line}; zapis w macros.json.", Design, false, schema_of::<MacroParams>(), &["REQ-01-040", "REQ-01-041"], macro_save);
    reg(
        r,
        "macro.list",
        "Makra",
        "Lista makr.",
        Read,
        false,
        json!({"type": "object", "properties": {}, "additionalProperties": false}),
        &["REQ-01-040"],
        macro_list,
    );
    reg(
        r,
        "macro.delete",
        "Usuń makro",
        "Usuwa makro.",
        Design,
        false,
        schema_of::<NameParams>(),
        &["REQ-01-040"],
        macro_delete,
    );
    reg(r, "macro.run", "Uruchom makro", "Wykonuje kroki makra po kolei (polecenia rejestru i surowe polecenia do urządzenia); błąd kroku przerywa makro.", Fire, false, schema_of::<MacroRunParams>(), &["REQ-01-040", "REQ-01-041"], macro_run);
}
