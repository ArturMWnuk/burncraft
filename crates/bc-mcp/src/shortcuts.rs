//! Narzędzia-skróty MCP (§15.2): wygodne nazwy nad poleceniami rejestru.
//! Schematy wejścia i wyjścia są brane z rejestru, więc zgadzają się 1:1
//! z `describe_command`. Każde wywołanie przechodzi przez politykę bezpieczeństwa.

use bc_commands::inspect::{self, Detail, ObjectReport};
use bc_commands::{schema_of, CmdError, Registry};
use schemars::JsonSchema;
use serde::Serialize;
use serde_json::{json, Map, Value};

use crate::tools::{gate, tool, Outcome};
use crate::{Ctx, RpcError, Server};

/// (narzędzie, polecenie, tytuł, opis) — narzędzia przekazujące argumenty bez zmian.
const PASS_THROUGH: &[(&str, &str, &str, &str)] = &[
    (
        "transform",
        "object.transform",
        "Transformacja",
        "Przesunięcie, obrót, skala, odbicie obiektów (ids albo zaznaczenie) względem anchor.",
    ),
    (
        "boolean",
        "path.boolean",
        "Operacja logiczna",
        "Suma, różnica (pierwszy minus reszta), iloczyn, XOR albo wycięcie kształtów.",
    ),
    (
        "offset",
        "path.offset",
        "Offset",
        "Offset kształtów zamkniętych: outward/inward/both, narożniki sharp/round/bevel.",
    ),
    (
        "array_grid",
        "array.grid",
        "Tablica prostokątna",
        "Kopie w siatce kolumny × wiersze.",
    ),
    (
        "array_circular",
        "array.circular",
        "Tablica kołowa",
        "Kopie rozłożone na okręgu.",
    ),
    (
        "align",
        "object.align",
        "Wyrównaj",
        "Wyrównanie do zaznaczenia, pierwszego/ostatniego obiektu lub obszaru roboczego.",
    ),
    (
        "distribute",
        "object.distribute",
        "Rozłóż",
        "Równomierny rozkład obiektów (środki lub przerwy).",
    ),
    ("open_file", "file.open", "Otwórz plik", "Otwiera .bcraft, szablon .bcraft-template albo projekt .lbrn2 w aktywnej zakładce."),
    ("save_file", "file.save", "Zapisz plik", "Zapisuje aktywny dokument jako .bcraft (path albo bieżący plik)."),
    ("export_file", "file.export", "Eksportuj plik", "Eksport do SVG (mm), DXF (AC1021 / dxf-r12 = AC1009) albo HPGL; format z rozszerzenia lub parametru."),
];

const SHAPES: &[&str] = &["rect", "ellipse", "polygon", "star", "arc"];

#[derive(Serialize, JsonSchema)]
struct QueryResult {
    objects: Vec<ObjectReport>,
}

fn cmd_schema(reg: &Registry, id: &str, params: bool) -> Value {
    let spec = &reg.get(id).expect("polecenie skrótu istnieje").spec;
    if params {
        spec.params_schema.clone()
    } else {
        spec.result_schema.clone()
    }
}

fn layer_key_schema() -> Value {
    json!({"description": "Warstwa: indeks (0–255) albo kolor \"#rrggbb\"", "oneOf": [{"type": "integer", "minimum": 0, "maximum": 255}, {"type": "string", "pattern": "^#[0-9a-fA-F]{6}$"}]})
}

pub fn definitions(reg: &Registry) -> Vec<Value> {
    let mut v = vec![
        tool("query_objects", "Wyszukaj obiekty",
            "Obiekty spełniające wszystkie podane warunki: warstwa, typ, przecięcie z bbox [x0,y0,x1,y1] (mm), tekst w nazwie/notatce/tagach (bez rozróżniania wielkości liter).",
            json!({"type": "object", "properties": {
                "layer": {"type": "integer", "minimum": 0, "maximum": 255},
                "type": {"type": "string", "enum": ["path", "rect", "ellipse", "polygon", "star", "arc", "group", "array", "clone"]},
                "bbox": {"type": "array", "items": {"type": "number"}, "minItems": 4, "maxItems": 4},
                "text": {"type": "string"}}, "additionalProperties": false}),
            schema_of::<QueryResult>(), true),
        tool("get_object", "Obiekt",
            "Pełne dane obiektu (parametry geometrii, atrybuty, dzieci grupy) i jego raport (bbox w świecie, długość, zamknięcie).",
            json!({"type": "object", "properties": {"id": {"type": "integer", "minimum": 1}}, "required": ["id"], "additionalProperties": false}),
            json!({"type": "object", "properties": {"object": {"type": "object"}, "report": {"type": "object"}}, "required": ["object", "report"]}),
            true),
        tool("draw_shape", "Narysuj kształt",
            "Tworzy kształt parametryczny. params jak w describe_command shape.<type> (bez layer). Nowy obiekt zostaje zaznaczony.",
            json!({"type": "object", "properties": {
                "type": {"type": "string", "enum": SHAPES},
                "params": {"type": "object"},
                "layer": {"type": "integer", "minimum": 0, "maximum": 255}}, "required": ["type", "params"], "additionalProperties": false}),
            cmd_schema(reg, "shape.rect", false), false),
        tool("draw_path", "Narysuj ścieżkę",
            "Ścieżka z danych SVG d (mm, Y w górę) albo z listy punktów.",
            cmd_schema(reg, "shape.path", true), cmd_schema(reg, "shape.path", false), false),
        tool("import_file", "Importuj plik",
            "Importuje SVG, DXF, HPGL/PLT lub .lbrn2 do aktywnego dokumentu. options jak w describe_command file.import (layer_by, layer, dpi, ignore_text, units, group).",
            json!({"type": "object", "properties": {"path": {"type": "string"}, "options": {"type": "object"}}, "required": ["path"], "additionalProperties": false}),
            cmd_schema(reg, "file.import", false), false),
        tool(
            "set_layer", "Ustawienia warstwy",
            "Zmienia nazwę, kolor, tryb i parametry warstwy. settings jak w describe_command layer.set (bez index).",
            json!({"type": "object", "properties": {"color": layer_key_schema(), "settings": {"type": "object"}}, "required": ["color", "settings"], "additionalProperties": false}),
            cmd_schema(reg, "layer.set", false), false),
        tool("assign_layer", "Przypisz warstwę",
            "Przenosi obiekty (ids albo zaznaczenie) na warstwę.",
            json!({"type": "object", "properties": {"ids": {"type": "array", "items": {"type": "integer"}}, "color": layer_key_schema()}, "required": ["color"], "additionalProperties": false}),
            cmd_schema(reg, "object.assign_layer", false), false),
        tool("plan_job", "Plan zadania",
            "Planuje zadanie aktywnego dokumentu (bez zmian w dokumencie): czas szacowany z modelu przyspieszeń, długość cięcia i przejazdów, liczba przebiegów, operacje, ostrzeżenia (open_shape_in_fill, outside_workspace, duplicate, cut_before_engrave, power_above_limit, beam_no_go, travel_lift), koszt czasu maszyny i hash planu. power_limit i rate_per_hour domyślnie z preferencji limits.max_power i costs.machine_rate_per_hour.",
            cmd_schema(reg, "plan.job", true), cmd_schema(reg, "plan.job", false), true),
        tool("export_machine_file", "Eksportuj plik maszynowy",
            "Planuje zadanie aktywnego dokumentu, kompiluje je dla sterownika i zapisuje plik (REQ-15-057): driver grbl — G-code GRBL w trybie laserowym (M3/M4, S na piksel, M7/M8, M73, dwa lasery), gcode — ogólny CNC (Mach3/Mach4, UCCNC, Masso, LinuxCNC). Ustawienia kompilatora: polecenie gcode.settings; start zadania: job.start_settings. Inne sterowniki (Ruida .rd, Lihuiyu — M3, galvo — M5) zwracają błąd unsupported_driver.",
            json!({"type": "object", "properties": {
                "driver": {"type": "string", "description": "grbl albo gcode", "examples": ["grbl", "gcode"]},
                "path": {"type": "string", "description": "Plik docelowy (.gcode, .nc, .gc)"},
                "selection_only": {"type": "boolean", "default": false, "description": "Tylko zaznaczone obiekty"}},
                "required": ["driver", "path"], "additionalProperties": false}),
            cmd_schema(reg, "gcode.export", false), false),
    ];
    for (name, cmd, title, desc) in PASS_THROUGH {
        v.push(tool(
            name,
            title,
            desc,
            cmd_schema(reg, cmd, true),
            cmd_schema(reg, cmd, false),
            false,
        ));
    }
    v
}

fn run(s: &mut Server, cmd: &str, params: Value, ctx: &Ctx) -> Outcome {
    if let Some(denied) = gate(s, &[cmd], &params, ctx) {
        return denied;
    }
    match s.bench.execute(&s.reg, cmd, params) {
        Ok(r) => Outcome::ok(r),
        Err(e) => Outcome::cmd_error(e, None),
    }
}

fn resolve_layer(s: &Server, key: &Value) -> Result<u8, CmdError> {
    let found = match key {
        Value::Number(n) => n.as_u64().and_then(|i| {
            s.bench
                .active()
                .doc
                .layers
                .iter()
                .find(|l| u64::from(l.index) == i)
        }),
        Value::String(c) => s
            .bench
            .active()
            .doc
            .layers
            .iter()
            .find(|l| l.color.eq_ignore_ascii_case(c)),
        _ => None,
    };
    found
        .map(|l| l.index)
        .ok_or_else(|| CmdError::new("invalid_params", format!("nie ma warstwy {key}")))
}

fn query(s: &Server, args: &Value) -> Result<Outcome, RpcError> {
    let rep = inspect::report(s.bench.active(), Detail::Summary)
        .map_err(|e| RpcError::invalid_params(e.to_string()))?;
    let layer = args.get("layer").and_then(Value::as_u64);
    let ty = args.get("type").and_then(Value::as_str);
    let bbox: Option<[f64; 4]> = match args.get("bbox") {
        Some(v) => Some(
            serde_json::from_value(v.clone())
                .map_err(|_| RpcError::invalid_params("bbox: [x0, y0, x1, y1]"))?,
        ),
        None => None,
    };
    let text = args
        .get("text")
        .and_then(Value::as_str)
        .map(str::to_lowercase);
    let objects = rep
        .objects
        .into_iter()
        .filter(|o| layer.is_none_or(|l| u64::from(o.layer) == l))
        .filter(|o| ty.is_none_or(|t| o.kind == t))
        .filter(|o| match (bbox, o.bbox) {
            (None, _) => true,
            (Some(q), Some(b)) => b[0] <= q[2] && b[2] >= q[0] && b[1] <= q[3] && b[3] >= q[1],
            (Some(_), None) => false,
        })
        .filter(|o| match &text {
            None => true,
            Some(t) => s.bench.active().doc.find(o.id).is_some_and(|obj| {
                obj.name
                    .iter()
                    .chain(obj.note.iter())
                    .chain(obj.tags.iter())
                    .any(|x| x.to_lowercase().contains(t.as_str()))
            }),
        })
        .collect();
    Ok(Outcome::ok(
        serde_json::to_value(QueryResult { objects }).unwrap_or_default(),
    ))
}

/// `None` = narzędzie nie jest skrótem.
pub fn call(
    s: &mut Server,
    name: &str,
    args: &Value,
    ctx: &Ctx,
) -> Option<Result<Outcome, RpcError>> {
    let obj = |v: Option<&Value>| v.and_then(Value::as_object).cloned().unwrap_or_default();
    Some(match name {
        "query_objects" => query(s, args),
        "get_object" => {
            let Some(id) = args.get("id").and_then(Value::as_u64) else {
                return Some(Err(RpcError::invalid_params("brak id")));
            };
            let rep = inspect::report(s.bench.active(), Detail::Full)
                .map_err(|e| RpcError::invalid_params(e.to_string()));
            Ok(match (s.bench.active().doc.find(id), rep) {
                (Some(o), Ok(rep)) => {
                    let report = rep.objects.into_iter().find(|r| r.id == id);
                    Outcome::ok(json!({ "object": o, "report": report }))
                }
                (None, _) => Outcome::cmd_error(
                    CmdError::new("not_found", format!("nie znaleziono obiektu {id}")),
                    None,
                ),
                (_, Err(e)) => return Some(Err(e)),
            })
        }
        "draw_shape" => {
            let ty = args.get("type").and_then(Value::as_str).unwrap_or_default();
            if !SHAPES.contains(&ty) {
                return Some(Ok(Outcome::cmd_error(
                    CmdError::new(
                        "invalid_params",
                        format!("type: jeden z {}", SHAPES.join(", ")),
                    ),
                    None,
                )));
            }
            let mut p: Map<String, Value> = obj(args.get("params"));
            if let Some(l) = args.get("layer") {
                p.insert("layer".into(), l.clone());
            }
            Ok(run(s, &format!("shape.{ty}"), Value::Object(p), ctx))
        }
        "draw_path" => Ok(run(s, "shape.path", args.clone(), ctx)),
        "export_machine_file" => {
            let mut p = Map::new();
            for k in ["driver", "path", "selection_only"] {
                if let Some(v) = args.get(k) {
                    p.insert(k.into(), v.clone());
                }
            }
            if !p.contains_key("driver") || !p.contains_key("path") {
                return Some(Err(RpcError::invalid_params("wymagane: driver, path")));
            }
            Ok(run(s, "gcode.export", Value::Object(p), ctx))
        }
        "plan_job" => {
            let mut p: Map<String, Value> = obj(Some(args));
            // globalne limity i stawki z preferencji (§15.4, §9)
            let prefs = s.bench.prefs.as_ref().and_then(|st| st.load().ok());
            for (key, path) in [
                ("power_limit", ["limits", "max_power"]),
                ("rate_per_hour", ["costs", "machine_rate_per_hour"]),
            ] {
                if !p.contains_key(key) {
                    if let Some(v) = prefs
                        .as_ref()
                        .and_then(|v| v.get(path[0]))
                        .and_then(|v| v.get(path[1]))
                        .filter(|v| v.is_number())
                    {
                        p.insert(key.into(), v.clone());
                    }
                }
            }
            Ok(run(s, "plan.job", Value::Object(p), ctx))
        }
        "import_file" => {
            let mut p = obj(args.get("options"));
            if let Some(path) = args.get("path") {
                p.insert("path".into(), path.clone());
            }
            Ok(run(s, "file.import", Value::Object(p), ctx))
        }
        "set_layer" => Ok(
            match resolve_layer(s, args.get("color").unwrap_or(&Value::Null)) {
                Ok(index) => {
                    let mut p = obj(args.get("settings"));
                    p.insert("index".into(), json!(index));
                    run(s, "layer.set", Value::Object(p), ctx)
                }
                Err(e) => Outcome::cmd_error(e, None),
            },
        ),
        "assign_layer" => Ok(
            match resolve_layer(s, args.get("color").unwrap_or(&Value::Null)) {
                Ok(layer) => {
                    let mut p = Map::new();
                    if let Some(ids) = args.get("ids") {
                        p.insert("ids".into(), ids.clone());
                    }
                    p.insert("layer".into(), json!(layer));
                    run(s, "object.assign_layer", Value::Object(p), ctx)
                }
                Err(e) => Outcome::cmd_error(e, None),
            },
        ),
        _ => {
            let (_, cmd, _, _) = PASS_THROUGH.iter().find(|(n, ..)| *n == name)?;
            Ok(run(s, cmd, args.clone(), ctx))
        }
    })
}
