//! Narzędzia MCP — odbicie rejestru poleceń (sekcja 15.2).

use base64::Engine;
use bc_commands::{inspect::DocumentReport, schema_of, BatchStep, CmdError, SafetyTier};
use bc_geom::Rect;
use serde_json::{json, Value};

use crate::policy::Decision;
use crate::{Ctx, RpcError, Server};

/// Wynik wywołania narzędzia.
pub struct Outcome {
    pub structured: Option<Value>,
    pub content: Vec<Value>,
    pub is_error: bool,
    /// ok | error | denied (dla dziennika audytu)
    pub status: &'static str,
    pub error: Option<String>,
}

impl Outcome {
    pub(crate) fn ok(v: Value) -> Self {
        let text = serde_json::to_string_pretty(&v).unwrap_or_default();
        Outcome {
            structured: Some(v),
            content: vec![json!({"type": "text", "text": text})],
            is_error: false,
            status: "ok",
            error: None,
        }
    }

    pub(crate) fn fail(status: &'static str, v: Value) -> Self {
        let msg = v
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let text = serde_json::to_string_pretty(&v).unwrap_or_default();
        Outcome {
            structured: None,
            content: vec![json!({"type": "text", "text": text})],
            is_error: true,
            status,
            error: Some(msg),
        }
    }

    pub(crate) fn cmd_error(e: CmdError, step: Option<usize>) -> Self {
        let mut v = json!({ "code": e.code, "message": e.message });
        if let Some(i) = step {
            v["step"] = json!(i);
        }
        Outcome::fail("error", v)
    }

    pub fn into_result(self) -> Value {
        let mut r = json!({ "content": self.content, "isError": self.is_error });
        if let Some(s) = self.structured {
            r["structuredContent"] = s;
        }
        r
    }
}

pub(crate) fn tool(
    name: &str,
    title: &str,
    description: &str,
    input: Value,
    output: Value,
    read_only: bool,
) -> Value {
    json!({
        "name": name,
        "title": title,
        "description": description,
        "inputSchema": input,
        "outputSchema": output,
        "annotations": { "readOnlyHint": read_only, "destructiveHint": false, "openWorldHint": false }
    })
}

fn history_schema() -> Value {
    json!({"type": "object", "properties": {
        "label": {"type": ["string", "null"], "description": "Cofnięty/ponowiony krok"},
        "can_undo": {"type": "boolean"}, "can_redo": {"type": "boolean"}},
        "required": ["can_undo", "can_redo"]})
}

pub fn definitions() -> Vec<Value> {
    let empty = json!({"type": "object", "properties": {}, "additionalProperties": false});
    vec![
        tool("list_commands", "Lista poleceń",
            "Lista poleceń z rejestru BurnCraft (id, tytuł, poziom bezpieczeństwa, undoable). Filtr dopasowuje id, tytuł i opis.",
            json!({"type": "object", "properties": {"filter": {"type": "string", "description": "np. \"shape.\" albo \"warstw\""}}, "additionalProperties": false}),
            json!({"type": "object", "properties": {"commands": {"type": "array", "items": {"type": "object"}}}, "required": ["commands"]}),
            true),
        tool("describe_command", "Opis polecenia",
            "Pełny opis polecenia: schemat JSON parametrów i wyniku, flagi, skrót, wymagania.",
            json!({"type": "object", "properties": {"id": {"type": "string"}}, "required": ["id"], "additionalProperties": false}),
            json!({"type": "object", "properties": {"id": {"type": "string"}, "params_schema": {"type": "object"}, "result_schema": {"type": "object"}, "safety_tier": {"type": "string"}}, "required": ["id"]}),
            true),
        tool("execute", "Wykonaj polecenie",
            "Wykonuje polecenie z rejestru. Polecenia bez `ids` działają na bieżącym zaznaczeniu. Polecenia poziomu motion/config/fire wymagają potwierdzenia użytkownika.",
            json!({"type": "object", "properties": {"id": {"type": "string", "description": "Id polecenia, np. shape.rect"}, "params": {"type": "object", "description": "Parametry wg describe_command"}}, "required": ["id"], "additionalProperties": false}),
            json!({"type": "object", "properties": {"id": {"type": "string"}, "result": {}}, "required": ["id", "result"]}),
            false),
        tool("batch", "Wsad poleceń",
            "Wykonuje sekwencję poleceń atomowo jako jeden krok historii; błąd dowolnego kroku przywraca stan sprzed wsadu.",
            json!({"type": "object", "properties": {"steps": {"type": "array", "minItems": 1, "items": {"type": "object", "properties": {"id": {"type": "string"}, "params": {"type": "object"}}, "required": ["id"]}}}, "required": ["steps"], "additionalProperties": false}),
            json!({"type": "object", "properties": {"results": {"type": "array"}}, "required": ["results"]}),
            false),
        tool("undo", "Cofnij", "Cofa ostatni krok historii.", empty.clone(), history_schema(), false),
        tool("redo", "Ponów", "Ponawia cofnięty krok historii.", empty, history_schema(), false),
        tool("inspect_document", "Stan dokumentu",
            "Warstwy (ustawienia), obiekty (id, typ, warstwa, bbox, transformacja, długość ścieżki, zamknięcie), zaznaczenie i statystyki. detail=full dodaje wszystkie warstwy i dane geometrii.",
            json!({"type": "object", "properties": {"detail": {"type": "string", "enum": ["summary", "full"]}}, "additionalProperties": false}),
            schema_of::<DocumentReport>(),
            true),
        tool("render_preview", "Podgląd PNG",
            "Renderuje podgląd do PNG. mode=design: projekt w kolorach warstw; mode=toolpath: ścieżki zadania z planera (cięcie w kolorach warstw, przejazdy szare, ruch bez wiązki jasnoszary); burn_sim będzie dostępny w M4. source=gcode (z mode=toolpath) pokazuje program wczytany przez gcode.import. bbox = [x0, y0, x1, y1] w mm (domyślnie obszar roboczy).",
            json!({"type": "object", "properties": {
                "mode": {"type": "string", "enum": ["design", "toolpath", "burn_sim"], "default": "design"},
                "source": {"type": "string", "enum": ["document", "gcode"], "default": "document", "description": "toolpath: zadanie z dokumentu albo wczytany program G-code"},
                "bbox": {"type": "array", "items": {"type": "number"}, "minItems": 4, "maxItems": 4},
                "max_size": {"type": "integer", "minimum": 16, "maximum": 8192, "default": 1024},
                "show_workspace": {"type": "boolean", "default": true}}, "additionalProperties": false}),
            json!({"type": "object", "properties": {
                "mode": {"type": "string"}, "width": {"type": "integer"}, "height": {"type": "integer"},
                "bbox": {"type": "array", "items": {"type": "number"}}, "px_per_mm": {"type": "number"}},
                "required": ["mode", "width", "height", "bbox", "px_per_mm"]}),
            true),
        tool("screenshot", "Zrzut okna",
            "Zrzut ekranu okna BurnCraft (kanwa z interfejsem) jako PNG. Działa, gdy agent jest podłączony do żywego okna (burncraft-cli mcp --connect); bez okna zwraca błąd no_window — użyj wtedy render_preview.",
            json!({"type": "object", "properties": {}, "additionalProperties": false}),
            json!({"type": "object", "properties": {"mimeType": {"type": "string"}}, "required": ["mimeType"]}),
            true),
    ]
}

fn arg_str<'a>(args: &'a Value, k: &str) -> Option<&'a str> {
    args.get(k).and_then(Value::as_str)
}

/// Sprawdza politykę dla poleceń `ids` (wsad: wszystkie kroki); `Some(outcome)` = odmowa.
/// Poziom motion wymaga potwierdzenia użytkownika: klient z elicitation dostaje pytanie
/// (REQ-15-094); bez niego — odmowa `confirmation_required`. config i fire zawsze odmowa
/// (potwierdzenie w GUI — M6).
pub(crate) fn gate(s: &Server, ids: &[&str], args: &Value, ctx: &Ctx) -> Option<Outcome> {
    let (id, tier) = ids
        .iter()
        .filter_map(|id| s.reg.get(id).map(|c| (*id, c.spec.safety_tier)))
        .max_by_key(|(_, t)| *t)?;
    match s.policy.check(tier) {
        Decision::Allow => None,
        Decision::NeedsConfirmation(SafetyTier::Motion) if s.peer().elicitation => {
            match &ctx.channel {
                Some(ch) => match confirm_motion(s, ch.as_ref(), ids, args, ctx) {
                    Ok(true) => None,
                    Ok(false) => Some(denied(
                        SafetyTier::Motion,
                        format!(
                            "odmowa: użytkownik nie potwierdził polecenia {id} (poziom „motion”)"
                        ),
                    )),
                    Err(e) => Some(denied(
                        SafetyTier::Motion,
                        format!("odmowa: brak potwierdzenia użytkownika dla {id}: {e}"),
                    )),
                },
                None => Some(denied(tier, s.policy.denial_message(id, tier))),
            }
        }
        Decision::NeedsConfirmation(t) => Some(denied(t, s.policy.denial_message(id, t))),
    }
}

fn denied(t: SafetyTier, message: String) -> Outcome {
    Outcome::fail(
        "denied",
        json!({ "code": "confirmation_required", "safety_tier": t.as_str(), "message": message }),
    )
}

/// Pytanie użytkownika przez MCP elicitation: {confirm: boolean}.
fn confirm_motion(
    s: &Server,
    ch: &dyn crate::Channel,
    ids: &[&str],
    args: &Value,
    ctx: &Ctx,
) -> Result<bool, String> {
    let mut detail = args.to_string();
    if detail.len() > 400 {
        let mut cut = 400;
        while !detail.is_char_boundary(cut) {
            cut -= 1;
        }
        detail.truncate(cut);
        detail.push('…');
    }
    let device = s
        .bench
        .active()
        .device
        .clone()
        .unwrap_or_else(|| "(brak wybranego)".into());
    let message = format!(
        "Agent MCP „{}” chce poruszyć maszyną: {} (poziom bezpieczeństwa „motion” — ruch bez wiązki), urządzenie {device}, parametry {detail}. Czy zezwolić?",
        s.peer().client,
        ids.join(", ")
    );
    let r = ch.request(
        "elicitation/create",
        json!({"message": message, "requestedSchema": {"type": "object", "properties": {
            "confirm": {"type": "boolean", "title": "Zezwól na ruch maszyny",
                "description": "Upewnij się, że obszar roboczy jest wolny"}}, "required": ["confirm"]}}),
        &ctx.cancel,
    )?;
    Ok(r.get("action").and_then(Value::as_str) == Some("accept")
        && r.pointer("/content/confirm").and_then(Value::as_bool) == Some(true))
}

pub fn call(s: &mut Server, name: &str, args: &Value, ctx: &Ctx) -> Result<Outcome, RpcError> {
    if !args.is_object() {
        return Err(RpcError::invalid_params("arguments musi być obiektem"));
    }
    Ok(match name {
        "list_commands" => {
            let cmds: Vec<Value> = s
                .reg
                .list(arg_str(args, "filter"))
                .into_iter()
                .map(|c| {
                    json!({"id": c.id, "title": c.title, "description": c.description, "safety_tier": c.safety_tier,
                           "undoable": c.undoable, "requires_device": c.requires_device, "shortcut": c.shortcut})
                })
                .collect();
            Outcome::ok(json!({ "commands": cmds }))
        }
        "describe_command" => {
            let id = arg_str(args, "id").ok_or_else(|| RpcError::invalid_params("brak id"))?;
            match s.reg.get(id) {
                Some(c) => Outcome::ok(serde_json::to_value(&c.spec).unwrap_or_default()),
                None => Outcome::cmd_error(
                    CmdError::new("unknown_command", format!("nieznane polecenie {id}")),
                    None,
                ),
            }
        }
        "execute" => {
            let id = arg_str(args, "id").ok_or_else(|| RpcError::invalid_params("brak id"))?;
            let params = args.get("params").cloned().unwrap_or(Value::Null);
            if let Some(denied) = gate(s, &[id], &params, ctx) {
                return Ok(denied);
            }
            match s.bench.execute(&s.reg, id, params) {
                Ok(r) => Outcome::ok(json!({ "id": id, "result": r })),
                Err(e) => Outcome::cmd_error(e, None),
            }
        }
        "batch" => {
            let steps: Vec<BatchStep> =
                serde_json::from_value(args.get("steps").cloned().unwrap_or(Value::Null))
                    .map_err(|e| RpcError::invalid_params(format!("steps: {e}")))?;
            if steps.is_empty() {
                return Err(RpcError::invalid_params("steps: co najmniej jeden krok"));
            }
            let ids: Vec<&str> = steps.iter().map(|st| st.id.as_str()).collect();
            let all = args.get("steps").cloned().unwrap_or(Value::Null);
            if let Some(denied) = gate(s, &ids, &all, ctx) {
                return Ok(denied);
            }
            match s.bench.batch(&s.reg, &steps) {
                Ok(r) => Outcome::ok(json!({ "results": r })),
                Err((i, e)) => Outcome::cmd_error(e, Some(i)),
            }
        }
        "undo" | "redo" => {
            let id = if name == "undo" {
                "edit.undo"
            } else {
                "edit.redo"
            };
            match s.bench.execute(&s.reg, id, json!({})) {
                Ok(r) => Outcome::ok(r),
                Err(e) => Outcome::cmd_error(e, None),
            }
        }
        "inspect_document" => {
            let mut p = json!({});
            if let Some(d) = args.get("detail") {
                p["detail"] = d.clone();
            }
            match s.bench.execute(&s.reg, "document.inspect", p) {
                Ok(r) => Outcome::ok(r),
                Err(e) => Outcome::cmd_error(e, None),
            }
        }
        "render_preview" => render_preview(s, args),
        "screenshot" => screenshot(s),
        other => match crate::shortcuts::call(s, other, args, ctx)
            .or_else(|| crate::devices::call(s, other, args, ctx))
        {
            Some(r) => r?,
            None => {
                return Err(RpcError::invalid_params(format!(
                    "nieznane narzędzie {other}"
                )))
            }
        },
    })
}

fn screenshot(s: &Server) -> Outcome {
    let res = match &s.screenshot {
        Some(h) => (h.0)(),
        None => Err("no_window".to_string()),
    };
    match res {
        Ok(data) => Outcome {
            content: vec![json!({"type": "image", "data": data, "mimeType": "image/png"})],
            structured: Some(json!({ "mimeType": "image/png" })),
            is_error: false,
            status: "ok",
            error: None,
        },
        Err(code) if code == "no_window" => Outcome::cmd_error(
            CmdError::new(
                "no_window",
                "brak podłączonego okna BurnCraft — użyj render_preview",
            ),
            None,
        ),
        Err(e) => Outcome::cmd_error(
            CmdError::new(
                "screenshot_failed",
                format!("zrzut okna nie powiódł się: {e}"),
            ),
            None,
        ),
    }
}

fn render_preview(s: &Server, args: &Value) -> Outcome {
    let mode = arg_str(args, "mode").unwrap_or("design");
    if mode != "design" && mode != "toolpath" {
        return Outcome::cmd_error(
            CmdError::new(
                "not_available",
                format!("tryb {mode} wymaga symulacji wypalenia (etap M4)"),
            ),
            None,
        );
    }
    let bbox = match args.get("bbox") {
        None | Some(Value::Null) => None,
        Some(v) => match serde_json::from_value::<[f64; 4]>(v.clone()) {
            Ok([x0, y0, x1, y1]) => Some(Rect::new(x0, y0, x1, y1)),
            Err(_) => {
                return Outcome::cmd_error(
                    CmdError::new("invalid_params", "bbox: [x0, y0, x1, y1]"),
                    None,
                )
            }
        },
    };
    let opt = bc_sim::PreviewOptions {
        bbox,
        max_size: args.get("max_size").and_then(Value::as_u64).unwrap_or(1024) as u32,
        show_workspace: args
            .get("show_workspace")
            .and_then(Value::as_bool)
            .unwrap_or(true),
    };
    let doc = &s.bench.active().doc;
    let source = arg_str(args, "source").unwrap_or("document");
    let rendered = if mode == "toolpath" && source == "gcode" {
        match &s.bench.active().gcode {
            Some(g) => bc_sim::render_toolpath(doc, &g.job, &opt),
            None => {
                return Outcome::cmd_error(
                    CmdError::new(
                        "not_found",
                        "brak wczytanego programu G-code — użyj gcode.import",
                    ),
                    None,
                )
            }
        }
    } else if source != "document" {
        return Outcome::cmd_error(
            CmdError::new(
                "invalid_params",
                "source: document albo gcode (z mode=toolpath)",
            ),
            None,
        );
    } else if mode == "toolpath" {
        match bc_plan::plan(doc, &bc_plan::PlanOptions::default()) {
            Ok(job) => bc_sim::render_toolpath(doc, &job, &opt),
            Err(e) => {
                return Outcome::cmd_error(CmdError::new("failed", e.to_string()), None);
            }
        }
    } else {
        bc_sim::render_design(doc, &opt)
    };
    match rendered {
        Ok(p) => {
            let meta = json!({ "mode": mode, "width": p.width, "height": p.height,
                "bbox": [p.bbox.x0, p.bbox.y0, p.bbox.x1, p.bbox.y1], "px_per_mm": p.px_per_mm });
            let data = base64::engine::general_purpose::STANDARD.encode(&p.png);
            Outcome {
                content: vec![
                    json!({"type": "image", "data": data, "mimeType": "image/png"}),
                    json!({"type": "text", "text": meta.to_string()}),
                ],
                structured: Some(meta),
                is_error: false,
                status: "ok",
                error: None,
            }
        }
        Err(e) => Outcome::cmd_error(CmdError::new("invalid_params", e.to_string()), None),
    }
}
