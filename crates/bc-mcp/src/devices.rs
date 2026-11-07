//! Narzędzia MCP urządzeń i zadań (§15.2, M2.4): cienkie nazwy nad poleceniami rejestru
//! z M2.3 (`machine.*`, `device.*`, `job.*`). Schematy wejścia i wyjścia pochodzą
//! z rejestru, poziom bezpieczeństwa — z polecenia (ruch: motion, stop: zawsze dozwolony).

use std::time::Instant;

use bc_commands::{Registry, SafetyTier};
use serde_json::{json, Value};

use crate::tools::{gate, Outcome};
use crate::{Ctx, RpcError, Server};

/// (narzędzie, polecenie, tytuł, opis, tylko odczyt)
const DEVICE_TOOLS: &[(&str, &str, &str, &str, bool)] = &[
    (
        "list_devices",
        "machine.list",
        "Urządzenia",
        "Profile urządzeń (z urządzeniem fikcyjnym „Symulator”), podłączone urządzenia, wybrane urządzenie i odmiany GRBL.",
        true,
    ),
    (
        "select_device",
        "machine.select",
        "Wybierz urządzenie",
        "Wybiera podłączone urządzenie dla narzędzi bez parametru device.",
        false,
    ),
    (
        "connect",
        "machine.connect",
        "Połącz",
        "Łączy profil z urządzeniem (rozpoznanie odmiany GRBL) i wybiera je. Kilka urządzeń może być połączonych naraz.",
        false,
    ),
    (
        "disconnect",
        "machine.disconnect",
        "Rozłącz",
        "Rozłącza urządzenie (domyślnie wybrane).",
        false,
    ),
    (
        "device_status",
        "machine.status",
        "Stan urządzenia",
        "Stan maszyny (pozycja mpos/wpos, stan idle/run/hold/jog/alarm/door, bufory, nadpisania, pokrywa, alarm, błędy), postęp zadania i możliwości. Ten sam stan daje zasób burncraft://device/{id}/status z subskrypcją.",
        true,
    ),
    (
        "home",
        "device.home",
        "Bazowanie",
        "Bazowanie osi maszyny ($H).",
        false,
    ),
    (
        "jog",
        "device.jog",
        "Jog",
        "Przesuwa oś (x, y, z) o distance mm z prędkością speed mm/s. Watchdog anuluje jog, gdy klient milczy dłużej niż limit (domyślnie 5 s) — przy długim jogu wywołuj device_status albo ping.",
        false,
    ),
    (
        "move_to",
        "device.move_to",
        "Przesuń do punktu",
        "Ruch do punktu w układzie roboczym (mm); speed mm/s albo przejazd szybki.",
        false,
    ),
    (
        "set_origin",
        "device.set_origin",
        "Ustaw zero",
        "Zero użytkownika w bieżącej pozycji głowicy (osie x, y, z).",
        false,
    ),
    (
        "frame",
        "device.frame",
        "Ramka",
        "Obrys zadania bez wiązki: kind=rect (prostokąt) albo contour (kontur); pointer — czerwonym laserem z korektą offsetu.",
        false,
    ),
    (
        "pause_job",
        "job.pause",
        "Pauza zadania",
        "Wstrzymanie zadania (feed hold).",
        false,
    ),
    (
        "resume_job",
        "job.resume",
        "Wznów zadanie",
        "Wznowienie zadania (cycle start); odmowa przy otwartej pokrywie lub alarmie.",
        false,
    ),
    (
        "stop_job",
        "job.stop",
        "Stop",
        "Natychmiastowe zatrzymanie zadania (wstrzymanie + reset, wiązka wyłączona). Zawsze dozwolone, bez potwierdzeń, także gdy serwer jest zajęty; bez wybranego urządzenia zatrzymuje wszystkie podłączone.",
        false,
    ),
    (
        "read_machine_settings",
        "machine.settings",
        "Ustawienia maszyny",
        "Odczyt ustawień sterownika ($$) z opisami, jednostkami i zakresami. Zapis — write_machine_settings (poziom config, M6).",
        true,
    ),
];

fn tier_note(t: SafetyTier) -> &'static str {
    match t {
        SafetyTier::Read => "Poziom bezpieczeństwa: read.",
        SafetyTier::Design => "Poziom bezpieczeństwa: design.",
        SafetyTier::Motion => "Poziom bezpieczeństwa: motion — wymaga potwierdzenia użytkownika.",
        SafetyTier::Config => "Poziom bezpieczeństwa: config — potwierdzenie w GUI.",
        SafetyTier::Fire => "Poziom bezpieczeństwa: fire — potwierdzenie w GUI.",
    }
}

pub fn definitions(reg: &Registry) -> Vec<Value> {
    DEVICE_TOOLS
        .iter()
        .map(|(name, cmd, title, desc, read_only)| {
            let spec = &reg.get(cmd).expect("polecenie narzędzia urządzenia").spec;
            let tier = if *name == "stop_job" {
                "Zawsze dozwolone."
            } else {
                tier_note(spec.safety_tier)
            };
            json!({
                "name": name,
                "title": title,
                "description": format!("{desc} {tier} Polecenie rejestru: {cmd}."),
                "inputSchema": spec.params_schema,
                "outputSchema": spec.result_schema,
                "annotations": {
                    "readOnlyHint": read_only,
                    "destructiveHint": false,
                    "idempotentHint": *read_only || *name == "stop_job",
                    "openWorldHint": !read_only && spec.requires_device
                }
            })
        })
        .collect()
}

/// `None` = narzędzie nie jest narzędziem urządzenia.
pub fn call(
    s: &mut Server,
    name: &str,
    args: &Value,
    ctx: &Ctx,
) -> Option<Result<Outcome, RpcError>> {
    let (_, cmd, ..) = DEVICE_TOOLS.iter().find(|(n, ..)| *n == name)?;
    // stop_job: zawsze dozwolony (job.stop ma poziom read), bez bramy potwierdzeń
    if *cmd != "job.stop" {
        if let Some(denied) = gate(s, &[cmd], args, ctx) {
            return Some(Ok(denied));
        }
    }
    let out = match s.bench.execute(&s.reg, cmd, args.clone()) {
        Ok(r) => Outcome::ok(r),
        Err(e) => Outcome::cmd_error(e, None),
    };
    if *cmd == "device.jog" && !out.is_error {
        // watchdog: jog tego klienta (REQ-15-108)
        let dev = args
            .get("device")
            .and_then(Value::as_str)
            .map(String::from)
            .or_else(|| s.bench.active().device.clone());
        if let Some(d) = dev {
            s.peer_mut().jogging.insert(d, Instant::now());
        }
    }
    Some(Ok(out))
}
