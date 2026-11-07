//! M1.4b3: edytor skrótów oparty na rejestrze (REQ-01-039) i porządkowanie
//! drzewa obiektów — zmiana rodzica i kolejności (REQ-02-041, 043).
//!
//! Skróty użytkownika leżą w preferencjach pod `ui.shortcuts.<id polecenia>`
//! (`null` = polecenie bez skrótu); brak wpisu = skrót domyślny z rejestru.

use std::sync::OnceLock;

use bc_doc::{Kind, ObjectId};
use bc_geom::Affine;
use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::{json, Value};

use crate::builtin::IdsResult;
use crate::edit::{add, invalid};
use crate::{
    ok, parse, schema_of, CmdError, CommandSpec, Registry, SafetyTier, Session, Workbench,
};

/// Rejestr wbudowany do odczytu domyślnych skrótów (polecenia na poziomie
/// Workbench nie dostają rejestru wywołującego).
fn builtin() -> &'static Registry {
    static R: OnceLock<Registry> = OnceLock::new();
    R.get_or_init(Registry::builtin)
}

// ---------- postać kanoniczna skrótu ----------

/// „ctrl + shift + t” → „Ctrl+Shift+T”; kolejność Ctrl, Shift, Alt, klawisz.
pub(crate) fn canonical(s: &str) -> Result<String, CmdError> {
    let parts: Vec<&str> = s.split('+').map(str::trim).collect();
    let (key, mods) = parts.split_last().ok_or_else(|| invalid("pusty skrót"))?;
    if key.is_empty() {
        return Err(invalid(format!(
            "skrót „{s}”: brak klawisza po modyfikatorach"
        )));
    }
    let (mut ctrl, mut shift, mut alt) = (false, false, false);
    for m in mods {
        match m.to_ascii_lowercase().as_str() {
            "ctrl" | "control" | "cmd" | "meta" | "command" => ctrl = true,
            "shift" => shift = true,
            "alt" | "option" => alt = true,
            other => {
                return Err(invalid(format!(
                    "skrót „{s}”: nieznany modyfikator „{other}”"
                )))
            }
        }
    }
    let lower = key.to_ascii_lowercase();
    let k = match lower.as_str() {
        "del" | "delete" => "Delete".to_string(),
        "esc" | "escape" => "Escape".to_string(),
        "enter" | "return" => "Enter".to_string(),
        "space" => "Space".to_string(),
        "tab" => "Tab".to_string(),
        "backspace" => "Backspace".to_string(),
        "home" => "Home".to_string(),
        "end" => "End".to_string(),
        "pageup" => "PageUp".to_string(),
        "pagedown" => "PageDown".to_string(),
        "insert" => "Insert".to_string(),
        "up" | "arrowup" => "ArrowUp".to_string(),
        "down" | "arrowdown" => "ArrowDown".to_string(),
        "left" | "arrowleft" => "ArrowLeft".to_string(),
        "right" | "arrowright" => "ArrowRight".to_string(),
        _ if key.chars().count() == 1 => key.to_uppercase(),
        _ if lower.starts_with('f')
            && lower[1..]
                .parse::<u8>()
                .is_ok_and(|n| (1..=24).contains(&n)) =>
        {
            lower.to_uppercase()
        }
        _ => return Err(invalid(format!("skrót „{s}”: nieznany klawisz „{key}”"))),
    };
    let mut out = Vec::new();
    if ctrl {
        out.push("Ctrl".to_string());
    }
    if shift {
        out.push("Shift".to_string());
    }
    if alt {
        out.push("Alt".to_string());
    }
    out.push(k);
    Ok(out.join("+"))
}

// ---------- skróty użytkownika ----------

fn overrides(w: &Workbench) -> Value {
    w.prefs
        .as_ref()
        .and_then(|st| st.load().ok())
        .and_then(|p| p.pointer("/ui/shortcuts").cloned())
        .unwrap_or_else(|| json!({}))
}

/// Wpis użytkownika dla polecenia: `None` = brak, `Some(None)` = bez skrótu.
fn custom(ov: &Value, id: &str) -> Option<Option<String>> {
    let mut cur = ov;
    for part in id.split('.') {
        cur = cur.get(part)?;
    }
    Some(cur.as_str().map(str::to_string))
}

fn effective(ov: &Value, spec: &CommandSpec) -> (Option<String>, bool) {
    match custom(ov, spec.id) {
        Some(c) => (c, true),
        None => (spec.shortcut.and_then(|s| canonical(s).ok()), false),
    }
}

#[derive(Serialize, JsonSchema)]
struct ShortcutInfo {
    id: &'static str,
    title: &'static str,
    /// Skrót domyślny z rejestru.
    default: Option<String>,
    /// Skrót obowiązujący (null = brak).
    shortcut: Option<String>,
    /// Czy ustawiony przez użytkownika.
    custom: bool,
}

#[derive(Serialize, JsonSchema)]
struct ShortcutList {
    commands: Vec<ShortcutInfo>,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct NoParams {}

fn shortcuts_list(w: &mut Workbench, p: Value) -> Result<Value, CmdError> {
    let _: NoParams = parse(p)?;
    let ov = overrides(w);
    let commands = builtin()
        .list(None)
        .into_iter()
        .map(|c| {
            let (shortcut, custom) = effective(&ov, c);
            ShortcutInfo {
                id: c.id,
                title: c.title,
                default: c.shortcut.and_then(|s| canonical(s).ok()),
                shortcut,
                custom,
            }
        })
        .collect();
    ok(ShortcutList { commands })
}

fn set_override(all: &mut Value, id: &str, v: Option<Value>) {
    if !all.get("ui").is_some_and(Value::is_object) {
        all["ui"] = json!({});
    }
    if !all["ui"].get("shortcuts").is_some_and(Value::is_object) {
        all["ui"]["shortcuts"] = json!({});
    }
    let mut cur = &mut all["ui"]["shortcuts"];
    let parts: Vec<&str> = id.split('.').collect();
    for part in &parts[..parts.len() - 1] {
        if !cur.get(*part).is_some_and(Value::is_object) {
            cur[*part] = json!({});
        }
        cur = cur.get_mut(*part).expect("utworzone");
    }
    let last = parts[parts.len() - 1];
    match v {
        Some(v) => cur[last] = v,
        None => {
            if let Some(o) = cur.as_object_mut() {
                o.remove(last);
            }
        }
    }
}

fn store(w: &Workbench) -> Result<&crate::prefs::PrefStore, CmdError> {
    w.prefs.as_ref().ok_or_else(|| {
        CmdError::new(
            "invalid_state",
            "magazyn preferencji nie jest skonfigurowany",
        )
    })
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct SetParams {
    /// Id polecenia.
    id: String,
    /// Skrót, np. „Ctrl+Shift+T”, „F2”, „Delete”; null = bez skrótu.
    shortcut: Option<String>,
    /// Zabierz skrót poleceniu, które go używa.
    #[serde(default)]
    force: bool,
}

#[derive(Serialize, JsonSchema)]
struct SetResult {
    id: String,
    shortcut: Option<String>,
    /// Polecenie, które straciło skrót (force).
    taken_from: Option<String>,
}

fn shortcuts_set(w: &mut Workbench, p: Value) -> Result<Value, CmdError> {
    let p: SetParams = parse(p)?;
    let reg = builtin();
    if reg.get(&p.id).is_none() {
        return Err(CmdError::new(
            "unknown_command",
            format!("nieznane polecenie {}", p.id),
        ));
    }
    let key = p.shortcut.as_deref().map(canonical).transpose()?;
    let st = store(w)?;
    let mut all = st.load()?;
    let ov = all
        .pointer("/ui/shortcuts")
        .cloned()
        .unwrap_or_else(|| json!({}));
    let mut taken_from = None;
    if let Some(k) = &key {
        let other = reg
            .list(None)
            .into_iter()
            .find(|c| c.id != p.id && effective(&ov, c).0.as_deref() == Some(k.as_str()));
        if let Some(o) = other {
            if !p.force {
                return Err(CmdError::new(
                    "shortcut_conflict",
                    format!(
                        "skrót {k} ma już polecenie {} („{}”) — użyj force, aby go przenieść",
                        o.id, o.title
                    ),
                ));
            }
            set_override(&mut all, o.id, Some(Value::Null));
            taken_from = Some(o.id.to_string());
        }
    }
    set_override(
        &mut all,
        &p.id,
        Some(key.clone().map(Value::String).unwrap_or(Value::Null)),
    );
    st.save(&all)?;
    ok(SetResult {
        id: p.id,
        shortcut: key,
        taken_from,
    })
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct ResetParams {
    /// Polecenie; brak = wszystkie skróty domyślne.
    #[serde(default)]
    id: Option<String>,
}

fn shortcuts_reset(w: &mut Workbench, p: Value) -> Result<Value, CmdError> {
    let p: ResetParams = parse(p)?;
    let st = store(w)?;
    let mut all = st.load()?;
    match &p.id {
        Some(id) => set_override(&mut all, id, None),
        None => {
            if let Some(ui) = all.get_mut("ui").and_then(Value::as_object_mut) {
                ui.remove("shortcuts");
            }
        }
    }
    st.save(&all)?;
    shortcuts_list(w, json!({}))
}

// ---------- object.arrange ----------

/// Rozróżnia brak pola (`None`) od `null` (`Some(None)` = poziom główny).
fn double<'de, D: Deserializer<'de>>(d: D) -> Result<Option<Option<ObjectId>>, D::Error> {
    Option::<ObjectId>::deserialize(d).map(Some)
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct ArrangeParams {
    /// Przenoszone obiekty (domyślnie zaznaczenie), w podanej kolejności.
    #[serde(default)]
    ids: Option<Vec<ObjectId>>,
    /// Nowy rodzic: id grupy albo null (poziom główny); brak = bez zmiany rodzica.
    #[serde(default, deserialize_with = "double")]
    parent: Option<Option<ObjectId>>,
    /// Pozycja w kolejności rysowania rodzica (0 = na spód); brak = na wierzch.
    #[serde(default)]
    index: Option<usize>,
}

fn world_affine(s: &Session, id: ObjectId) -> Affine {
    let o = s.doc.find(id).expect("sprawdzone");
    s.doc.parent_affine(id).unwrap_or(Affine::IDENTITY) * o.affine()
}

fn in_subtree(s: &Session, root: ObjectId, id: ObjectId) -> bool {
    fn rec(o: &bc_doc::Object, id: ObjectId) -> bool {
        o.id == id || o.children().iter().any(|c| rec(c, id))
    }
    s.doc.find(root).is_some_and(|o| rec(o, id))
}

fn object_arrange(s: &mut Session, p: Value) -> Result<Value, CmdError> {
    let p: ArrangeParams = parse(p)?;
    let ids = s.targets(&p.ids)?;
    for id in &ids {
        if s.doc.find(*id).is_some_and(|o| o.locked) {
            return Err(CmdError::new(
                "locked",
                format!("obiekt {id} jest zablokowany"),
            ));
        }
    }
    let first = ids[0];
    let parent = match p.parent {
        Some(np) => np,
        None => s.doc.parent_of(first).flatten(),
    };
    if let Some(pid) = parent {
        let po = s
            .doc
            .find(pid)
            .ok_or_else(|| CmdError::new("not_found", format!("nie znaleziono obiektu {pid}")))?;
        if !matches!(po.kind, Kind::Group { .. }) {
            return Err(invalid(format!("obiekt {pid} nie jest grupą")));
        }
        if let Some(bad) = ids.iter().find(|id| in_subtree(s, **id, pid)) {
            return Err(invalid(format!(
                "obiektu {bad} nie można przenieść do jego własnego wnętrza"
            )));
        }
    }
    let parent_world = |s: &Session| match parent {
        Some(pid) => world_affine(s, pid),
        None => Affine::IDENTITY,
    };
    // wyjmij obiekty, zachowując ich przekształcenie w świecie
    let mut moved = Vec::new();
    for id in &ids {
        let world = world_affine(s, *id);
        let mut o = s.doc.remove(*id)?;
        o.transform = world.as_coeffs();
        moved.push(o);
    }
    let pw = parent_world(s).inverse();
    let list = s
        .doc
        .container_mut(parent)
        .ok_or_else(|| invalid("rodzic nie jest kontenerem"))?;
    let at = p.index.unwrap_or(list.len()).min(list.len());
    for (k, mut o) in moved.into_iter().enumerate() {
        o.transform = (pw * Affine::new(o.transform)).as_coeffs();
        list.insert(at + k, o);
    }
    ok(IdsResult { ids })
}

pub(crate) fn register_all(r: &mut Registry) {
    use SafetyTier::*;
    let bench =
        |r: &mut Registry, id, title, description, tier, params: Value, result: Value, h| {
            r.register_bench(
                CommandSpec {
                    executor: crate::Executor::Document,
                    id,
                    title,
                    description,
                    params_schema: params,
                    result_schema: result,
                    undoable: false,
                    requires_device: false,
                    safety_tier: tier,
                    shortcut: None,
                    requirements: &["REQ-01-039"],
                },
                h,
            )
        };
    bench(
        r,
        "shortcuts.list",
        "Skróty klawiszowe",
        "Wszystkie polecenia rejestru ze skrótem domyślnym i obowiązującym.",
        Read,
        schema_of::<NoParams>(),
        schema_of::<ShortcutList>(),
        shortcuts_list,
    );
    bench(r, "shortcuts.set", "Ustaw skrót", "Przypisuje skrót poleceniu (null = bez skrótu); konflikt z innym poleceniem = błąd, chyba że force.", Config, schema_of::<SetParams>(), schema_of::<SetResult>(), shortcuts_set);
    bench(
        r,
        "shortcuts.reset",
        "Skróty domyślne",
        "Przywraca skrót domyślny polecenia albo wszystkich poleceń.",
        Config,
        schema_of::<ResetParams>(),
        schema_of::<ShortcutList>(),
        shortcuts_reset,
    );
    add::<ArrangeParams, IdsResult>(r, "object.arrange", "Przenieś w drzewie", "Zmienia rodzica (grupa albo poziom główny) i pozycję w kolejności rysowania; położenie w świecie bez zmian.", Design, true, &["REQ-02-041", "REQ-02-043"], object_arrange);
}
