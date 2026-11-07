//! Prompty MCP (§15.3): gotowe przepływy pracy dla agenta. M2.4: `prepare_cut_file`
//! (otwarte kształty, duplikaty, kerf, kolejność) z bieżącymi ostrzeżeniami planera.
//! Kolejne (`engrave_photo`, `make_box`, `run_material_test`, `batch_serial_numbers`) — M3/M4.

use serde_json::{json, Value};

use crate::{RpcError, Server};

pub fn list() -> Value {
    json!({ "prompts": [
        {
            "name": "prepare_cut_file",
            "title": "Przygotuj plik do cięcia",
            "description": "Kontrola projektu przed cięciem laserem: otwarte kształty w wypełnieniu, duplikaty, kompensacja kerf, kolejność operacji, obszar roboczy — z bieżącymi ostrzeżeniami planu zadania.",
            "arguments": [
                { "name": "selection_only", "description": "true = tylko zaznaczone obiekty", "required": false },
                { "name": "material", "description": "Materiał i grubość (np. sklejka 3 mm) — do oceny kerf i kolejności", "required": false }
            ]
        }
    ]})
}

pub fn get(s: &mut Server, params: &Value) -> Result<Value, RpcError> {
    let name = params
        .get("name")
        .and_then(Value::as_str)
        .ok_or_else(|| RpcError::invalid_params("brak name"))?;
    match name {
        "prepare_cut_file" => prepare_cut_file(s, params.get("arguments").unwrap_or(&Value::Null)),
        other => Err(RpcError::invalid_params(format!("nieznany prompt {other}"))),
    }
}

fn arg<'a>(args: &'a Value, k: &str) -> Option<&'a str> {
    args.get(k).and_then(Value::as_str)
}

fn prepare_cut_file(s: &mut Server, args: &Value) -> Result<Value, RpcError> {
    let selection_only = arg(args, "selection_only").is_some_and(|v| v == "true")
        || args.get("selection_only").and_then(Value::as_bool) == Some(true);
    let material = arg(args, "material").unwrap_or("nieokreślony");
    // stan bieżącego dokumentu: plan zadania (bez zmian w dokumencie)
    let plan = s
        .bench
        .execute(
            &s.reg,
            "plan.job",
            json!({ "selection_only": selection_only }),
        )
        .map_err(|e| RpcError {
            code: -32603,
            message: format!("plan zadania: {e}"),
        })?;
    let warnings: Vec<String> = plan
        .get("warnings")
        .and_then(Value::as_array)
        .map(|ws| {
            ws.iter()
                .map(|w| {
                    let code = w.get("code").and_then(Value::as_str).unwrap_or("?");
                    let msg = w.get("message").and_then(Value::as_str).unwrap_or("");
                    let ids = w
                        .get("ids")
                        .map(|v| format!(" obiekty {v}"))
                        .unwrap_or_default();
                    format!("- {code}: {msg}{ids}")
                })
                .collect()
        })
        .unwrap_or_default();
    let warn_text = if warnings.is_empty() {
        "- brak ostrzeżeń planera".to_string()
    } else {
        warnings.join("\n")
    };
    let mut stats = serde_json::Map::new();
    for k in ["time", "cut_length", "travel_length", "passes", "cost"] {
        if let Some(v) = plan.get(k) {
            stats.insert(k.into(), v.clone());
        }
    }
    let objects = s.bench.active().doc.objects.len();
    let text = format!(
        "Przygotuj bieżący projekt BurnCraft do cięcia laserem (materiał: {material}; {zakres}). \
Nie uruchamiaj maszyny — tylko sprawdź i popraw dokument.\n\n\
Sprawdź po kolei:\n\
1. Otwarte kształty: ścieżki niezamknięte na warstwach w trybie wypełnienia (fill / offset_fill) nie dadzą się wypełnić \
(ostrzeżenie open_shape_in_fill) — zamknij je (node.join) albo przenieś na warstwę w trybie line (assign_layer).\n\
2. Duplikaty: identyczne, nakładające się obiekty (ostrzeżenie duplicate) laser przetnie dwa razy — usuń nadmiarowe \
(query_objects, execute object.delete).\n\
3. Kerf: dla elementów pasowanych (wpusty, puzzle, wkładki) ustaw kompensację szerokości cięcia w parametrach warstwy \
(set_layer z polem kerf; kontury zewnętrzne na zewnątrz, otwory do wewnątrz); typowo 0,05–0,2 mm zależnie od materiału.\n\
4. Kolejność: grawer i znakowanie przed cięciem, otwory i kontury wewnętrzne przed zewnętrznymi, żeby część nie \
wypadła przed końcem (ostrzeżenie cut_before_engrave); sprawdź kolejność warstw i przebiegów.\n\
5. Obszar roboczy i moc: obiekty poza obszarem (outside_workspace) i moc powyżej limitu (power_above_limit).\n\n\
Narzędzia: inspect_document i query_objects (stan), plan_job (czas, długości, ostrzeżenia — uruchom ponownie po każdej \
poprawce), render_preview mode=toolpath (podgląd ścieżek i kolejności), save_file na końcu.\n\n\
Stan teraz: {objects} obiektów; plan_job: {stats}\nOstrzeżenia planera:\n{warn_text}",
        zakres = if selection_only {
            "tylko zaznaczenie"
        } else {
            "cały dokument"
        },
        stats = Value::Object(stats),
    );
    Ok(json!({
        "description": "Kontrola projektu przed cięciem: otwarte kształty, duplikaty, kerf, kolejność",
        "messages": [
            { "role": "user", "content": { "type": "text", "text": text } }
        ]
    }))
}
