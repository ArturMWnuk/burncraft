//! Zasoby MCP (sekcja 15.3): dokument, obiekty, warstwy, polecenia, stan urządzenia
//! i zadania. Wszystkie można subskrybować (`resources/subscribe`).

use bc_commands::inspect::{self, Detail};
use serde_json::{json, Value};

use crate::{RpcError, Server};

/// Kodowanie segmentu URI (nazwy urządzeń mogą mieć spacje i znaki narodowe).
pub(crate) fn encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b"-._~".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

pub(crate) fn decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Some(v) = s
                .get(i + 1..i + 3)
                .and_then(|h| u8::from_str_radix(h, 16).ok())
            {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

pub fn list(s: &mut Server) -> Value {
    let mut v = vec![
        json!({ "uri": "burncraft://document", "name": "document", "title": "Bieżący dokument",
          "description": "Raport dokumentu (detail=full)", "mimeType": "application/json" }),
        json!({ "uri": "burncraft://commands", "name": "commands", "title": "Rejestr poleceń",
          "description": "Wszystkie polecenia z flagami", "mimeType": "application/json" }),
    ];
    // podłączone urządzenia i ich zadania
    let devices: Vec<(String, u64)> = {
        let mut m = s.bench.active().machines.lock().expect("urządzenia");
        let names = m.connected();
        names
            .into_iter()
            .map(|n| {
                let id = m.driver(&n).map(|d| d.job().id).unwrap_or(0);
                (n, id)
            })
            .collect()
    };
    for (name, job) in devices {
        v.push(json!({ "uri": format!("burncraft://device/{}/status", encode(&name)),
            "name": format!("device-{name}"), "title": format!("Stan urządzenia {name}"),
            "description": "Pozycja, stan, alarm, postęp zadania (subskrypcja)", "mimeType": "application/json" }));
        if job != 0 {
            v.push(json!({ "uri": format!("burncraft://job/{job}"), "name": format!("job-{job}"),
                "title": format!("Zadanie {job} ({name})"), "description": "Postęp i dziennik zadania",
                "mimeType": "application/json" }));
        }
    }
    json!({ "resources": v })
}

pub fn templates() -> Value {
    json!({ "resourceTemplates": [
        { "uriTemplate": "burncraft://object/{id}", "name": "object", "title": "Obiekt", "mimeType": "application/json" },
        { "uriTemplate": "burncraft://layer/{color}", "name": "layer", "title": "Warstwa (indeks 0-29 lub kolor #rrggbb)", "mimeType": "application/json" },
        { "uriTemplate": "burncraft://command/{id}", "name": "command", "title": "Polecenie (pełny opis)", "mimeType": "application/json" },
        { "uriTemplate": "burncraft://device/{id}/status", "name": "device-status", "title": "Stan urządzenia (subskrypcja)",
          "description": "id = nazwa profilu urządzenia (kodowanie procentowe)", "mimeType": "application/json" },
        { "uriTemplate": "burncraft://job/{id}", "name": "job", "title": "Zadanie: postęp i dziennik",
          "description": "id = identyfikator zadania z device_status / job.start", "mimeType": "application/json" }
    ]})
}

/// Polecenie rejestru dla wskazanego urządzenia (bez zmiany wyboru w sesji).
fn exec(s: &Server, device: &str, id: &str, p: Value) -> Result<Value, bc_commands::CmdError> {
    s.emergency.exec(device, id, p)
}

pub fn read(s: &mut Server, params: &Value) -> Result<Value, RpcError> {
    let uri = params
        .get("uri")
        .and_then(Value::as_str)
        .ok_or_else(|| RpcError::invalid_params("brak uri"))?
        .to_string();
    let not_found = || RpcError {
        code: -32002,
        message: format!("nie znaleziono zasobu {uri}"),
    };
    let internal = |e: String| RpcError {
        code: -32603,
        message: e,
    };
    let rest = uri.strip_prefix("burncraft://").ok_or_else(not_found)?;
    let body: Value = match rest.split_once('/') {
        None if rest == "document" => serde_json::to_value(
            inspect::report(s.bench.active(), Detail::Full).map_err(|e| internal(e.to_string()))?,
        )
        .unwrap_or_default(),
        None if rest == "commands" => serde_json::to_value(s.reg.list(None)).unwrap_or_default(),
        Some(("object", id)) => {
            let id: u64 = id.parse().map_err(|_| not_found())?;
            serde_json::to_value(s.bench.active().doc.find(id).ok_or_else(not_found)?)
                .unwrap_or_default()
        }
        Some(("layer", key)) => {
            let key = key.replace("%23", "#").to_lowercase();
            let layer = s
                .bench
                .active()
                .doc
                .layers
                .iter()
                .find(|l| key.parse::<u8>().ok() == Some(l.index) || l.color == key)
                .ok_or_else(not_found)?;
            serde_json::to_value(layer).unwrap_or_default()
        }
        Some(("command", id)) => {
            serde_json::to_value(&s.reg.get(id).ok_or_else(not_found)?.spec).unwrap_or_default()
        }
        Some(("device", tail)) => {
            let name = tail.strip_suffix("/status").ok_or_else(not_found)?;
            let name = decode(name);
            if !s
                .bench
                .active()
                .machines
                .lock()
                .expect("urządzenia")
                .connected()
                .contains(&name)
            {
                return Err(not_found());
            }
            exec(s, &name, "machine.status", json!({ "device": name }))
                .map_err(|e| internal(e.to_string()))?
        }
        Some(("job", id)) => {
            let id: u64 = id.parse().map_err(|_| not_found())?;
            job(s, id)?.ok_or_else(not_found)?
        }
        _ => return Err(not_found()),
    };
    Ok(
        json!({ "contents": [ { "uri": uri, "mimeType": "application/json", "text": serde_json::to_string_pretty(&body).unwrap_or_default() } ] }),
    )
}

/// Zadanie o identyfikatorze `id` na którymś z podłączonych urządzeń: stan, postęp 0–1, dziennik.
fn job(s: &mut Server, id: u64) -> Result<Option<Value>, RpcError> {
    if id == 0 {
        return Ok(None);
    }
    let names = s
        .bench
        .active()
        .machines
        .lock()
        .expect("urządzenia")
        .connected();
    for name in names {
        let Ok(j) = exec(s, &name, "job.status", json!({ "device": name })) else {
            continue;
        };
        if j.get("id").and_then(Value::as_u64) != Some(id) {
            continue;
        }
        let total = j.get("total").and_then(Value::as_u64).unwrap_or(0);
        let acked = j.get("acked").and_then(Value::as_u64).unwrap_or(0);
        let progress = if total > 0 {
            (acked as f64 / total as f64).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let log = exec(s, &name, "machine.console", json!({ "device": name }))
            .ok()
            .and_then(|c| c.get("lines").cloned())
            .unwrap_or_else(|| json!([]));
        return Ok(Some(json!({
            "device": name,
            "job": j,
            "progress": progress,
            "log": log,
        })));
    }
    Ok(None)
}
