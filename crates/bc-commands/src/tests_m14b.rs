//! Testy M1.4b3 na poziomie poleceń: edytor skrótów oparty na rejestrze
//! (REQ-01-039) i porządkowanie drzewa obiektów (REQ-02-041, 043).

use super::*;
use serde_json::{json, Value};

fn bench_with_prefs(name: &str) -> Workbench {
    let d = std::env::temp_dir().join(format!("bc-m14b-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    Workbench {
        prefs: Some(prefs::PrefStore::new(d)),
        ..Default::default()
    }
}

fn run(r: &Registry, w: &mut Workbench, id: &str, p: Value) -> Value {
    w.execute(r, id, p.clone())
        .unwrap_or_else(|e| panic!("{id} {p}: {e}"))
}

fn shortcut_of(list: &Value, id: &str) -> Value {
    list["commands"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["id"] == id)
        .unwrap_or_else(|| panic!("{id} w liście"))
        .clone()
}

#[test]
fn req_01_039_shortcuts_list_defaults_from_registry() {
    let (r, mut w) = (Registry::builtin(), Workbench::default());
    let l = run(&r, &mut w, "shortcuts.list", json!({}));
    let undo = shortcut_of(&l, "edit.undo");
    assert_eq!(undo["default"], "Ctrl+Z");
    assert_eq!(undo["shortcut"], "Ctrl+Z");
    assert_eq!(undo["custom"], false);
    // każde polecenie rejestru jest na liście (także bez domyślnego skrótu)
    assert_eq!(l["commands"].as_array().unwrap().len(), r.list(None).len());
    assert_eq!(shortcut_of(&l, "path.trim")["shortcut"], Value::Null);
}

#[test]
fn req_01_039_shortcuts_set_conflicts_force_and_reset() {
    let (r, mut w) = (Registry::builtin(), bench_with_prefs("set"));
    run(
        &r,
        &mut w,
        "shortcuts.set",
        json!({"id": "path.trim", "shortcut": "ctrl + shift + t"}),
    );
    let l = run(&r, &mut w, "shortcuts.list", json!({}));
    let t = shortcut_of(&l, "path.trim");
    assert_eq!(t["shortcut"], "Ctrl+Shift+T", "postać kanoniczna");
    assert_eq!(t["custom"], true);
    // zapis w preferencjach (z kopią zapasową poprzedniej wersji)
    let p = run(
        &r,
        &mut w,
        "prefs.get",
        json!({"key": "ui.shortcuts.path.trim"}),
    );
    assert_eq!(p["value"], "Ctrl+Shift+T");
    // konflikt z domyślnym skrótem innego polecenia
    let e = w
        .execute(
            &r,
            "shortcuts.set",
            json!({"id": "path.trim", "shortcut": "Ctrl+Z"}),
        )
        .unwrap_err();
    assert_eq!(e.code, "shortcut_conflict");
    assert!(e.message.contains("edit.undo"), "{}", e.message);
    // force: skrót przechodzi, drugie polecenie traci go
    run(
        &r,
        &mut w,
        "shortcuts.set",
        json!({"id": "path.trim", "shortcut": "Ctrl+Z", "force": true}),
    );
    let l = run(&r, &mut w, "shortcuts.list", json!({}));
    assert_eq!(shortcut_of(&l, "path.trim")["shortcut"], "Ctrl+Z");
    assert_eq!(shortcut_of(&l, "edit.undo")["shortcut"], Value::Null);
    assert_eq!(shortcut_of(&l, "edit.undo")["custom"], true);
    // usunięcie skrótu i przywrócenie domyślnych
    run(
        &r,
        &mut w,
        "shortcuts.set",
        json!({"id": "object.delete", "shortcut": null}),
    );
    assert_eq!(
        shortcut_of(
            &run(&r, &mut w, "shortcuts.list", json!({})),
            "object.delete"
        )["shortcut"],
        Value::Null
    );
    run(&r, &mut w, "shortcuts.reset", json!({"id": "edit.undo"}));
    assert_eq!(
        shortcut_of(&run(&r, &mut w, "shortcuts.list", json!({})), "edit.undo")["shortcut"],
        "Ctrl+Z"
    );
    run(&r, &mut w, "shortcuts.reset", json!({}));
    let l = run(&r, &mut w, "shortcuts.list", json!({}));
    assert!(l["commands"]
        .as_array()
        .unwrap()
        .iter()
        .all(|c| c["custom"] == false));
    // błędy: nieznane polecenie, zły format
    assert_eq!(
        w.execute(
            &r,
            "shortcuts.set",
            json!({"id": "nie.ma", "shortcut": "F2"})
        )
        .unwrap_err()
        .code,
        "unknown_command"
    );
    assert_eq!(
        w.execute(
            &r,
            "shortcuts.set",
            json!({"id": "path.trim", "shortcut": "Ctrl+"})
        )
        .unwrap_err()
        .code,
        "invalid_params"
    );
}

fn rect(r: &Registry, w: &mut Workbench, x: f64) -> u64 {
    run(
        r,
        w,
        "shape.rect",
        json!({"x": x, "y": 0, "w": 10, "h": 10}),
    )["id"]
        .as_u64()
        .unwrap()
}

fn top_ids(w: &Workbench) -> Vec<u64> {
    w.active().doc.objects.iter().map(|o| o.id).collect()
}

#[test]
fn req_02_043_arrange_changes_drawing_order() {
    let (r, mut w) = (Registry::builtin(), Workbench::default());
    let a = rect(&r, &mut w, 0.);
    let b = rect(&r, &mut w, 20.);
    let c = rect(&r, &mut w, 40.);
    run(
        &r,
        &mut w,
        "object.arrange",
        json!({"ids": [c], "index": 0}),
    );
    assert_eq!(top_ids(&w), vec![c, a, b]);
    run(&r, &mut w, "object.arrange", json!({"ids": [c]})); // domyślnie na wierzch
    assert_eq!(top_ids(&w), vec![a, b, c]);
    run(&r, &mut w, "edit.undo", json!({}));
    assert_eq!(top_ids(&w), vec![c, a, b]);
}

#[test]
fn req_02_041_arrange_moves_between_groups_keeping_world_position() {
    let (r, mut w) = (Registry::builtin(), Workbench::default());
    let a = rect(&r, &mut w, 0.);
    let b = rect(&r, &mut w, 20.);
    let g = run(&r, &mut w, "object.group", json!({"ids": [a, b]}))["id"]
        .as_u64()
        .unwrap();
    run(
        &r,
        &mut w,
        "object.transform",
        json!({"ids": [g], "translate": [100, 50]}),
    );
    let c = rect(&r, &mut w, 300.);
    let bbox = |w: &Workbench, id: u64| {
        let d = &w.active().doc;
        let bb = d
            .find(id)
            .unwrap()
            .world_bbox(d, d.parent_affine(id).unwrap())
            .unwrap()
            .unwrap();
        [bb.x0, bb.y0, bb.x1, bb.y1].map(|v| (v * 1e6).round() / 1e6)
    };
    // c do grupy: położenie w świecie bez zmian
    run(
        &r,
        &mut w,
        "object.arrange",
        json!({"ids": [c], "parent": g, "index": 0}),
    );
    assert_eq!(top_ids(&w), vec![g]);
    let kids: Vec<u64> = w
        .active()
        .doc
        .find(g)
        .unwrap()
        .children()
        .iter()
        .map(|o| o.id)
        .collect();
    assert_eq!(kids, vec![c, a, b]);
    assert_eq!(bbox(&w, c), [300., 0., 310., 10.]);
    // a z grupy na poziom główny
    run(
        &r,
        &mut w,
        "object.arrange",
        json!({"ids": [a], "parent": null}),
    );
    assert_eq!(top_ids(&w), vec![g, a]);
    assert_eq!(bbox(&w, a), [100., 50., 110., 60.]);
    // grupa nie trafi do własnego potomka ani do zwykłego kształtu
    assert_eq!(
        w.execute(&r, "object.arrange", json!({"ids": [g], "parent": c}))
            .unwrap_err()
            .code,
        "invalid_params"
    );
    let g2 = run(&r, &mut w, "object.group", json!({"ids": [a]}))["id"]
        .as_u64()
        .unwrap();
    run(
        &r,
        &mut w,
        "object.arrange",
        json!({"ids": [g2], "parent": g}),
    );
    assert_eq!(
        w.execute(&r, "object.arrange", json!({"ids": [g], "parent": g2}))
            .unwrap_err()
            .code,
        "invalid_params"
    );
}
