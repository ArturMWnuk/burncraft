//! Testy M1.2: tablice wirtualne, klony, obiekty pomocnicze, szablony, wiele dokumentów,
//! schowek, autozapis i odzyskiwanie, preferencje z kopiami, edycja w grupach.

use std::time::{Duration, SystemTime};

use super::*;
use crate::prefs::PrefStore;
use crate::workbench::Autosave;
use serde_json::{json, Value};

fn reg() -> Registry {
    Registry::builtin()
}

fn run(r: &Registry, w: &mut Workbench, id: &str, p: Value) -> Value {
    w.execute(r, id, p.clone())
        .unwrap_or_else(|e| panic!("{id} {p}: {e}"))
}

fn err(r: &Registry, w: &mut Workbench, id: &str, p: Value) -> CmdError {
    w.execute(r, id, p).unwrap_err()
}

fn rect(r: &Registry, w: &mut Workbench, x: f64, y: f64) -> u64 {
    run(
        r,
        w,
        "shape.rect",
        json!({"x": x, "y": y, "w": 10, "h": 10}),
    )["id"]
        .as_u64()
        .unwrap()
}

fn bbox(w: &Workbench, id: u64) -> [f64; 4] {
    let b = w.active().doc.bbox_of(id).unwrap().unwrap();
    [b.x0, b.y0, b.x1, b.y1]
}

fn tmp(name: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("bc-m12-{}-{}", name, std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

// ---------- tablice wirtualne ----------

#[test]
fn req_04_073_virtual_array_command() {
    let (r, mut w) = (reg(), Workbench::default());
    let a = rect(&r, &mut w, 0., 0.);
    let arr = run(
        &r,
        &mut w,
        "array.virtual",
        json!({"ids": [a], "layout": {"kind": "grid", "columns": 3, "rows": 1, "dx": 20, "dy": 0}}),
    )["id"]
        .as_u64()
        .unwrap();
    assert_eq!(w.active().doc.objects.len(), 1, "źródło wewnątrz tablicy");
    assert_eq!(bbox(&w, arr), [0., 0., 50., 10.]);
    // edycja źródła zmienia kopie
    run(
        &r,
        &mut w,
        "object.transform",
        json!({"ids": [a], "scale": [2, 1], "anchor": "origin"}),
    );
    assert_eq!(bbox(&w, arr)[2], 60.0);
    run(
        &r,
        &mut w,
        "array.set_layout",
        json!({"id": arr, "layout": {"kind": "circular", "count": 6, "center": [5, -30]}}),
    );
    assert_eq!(w.active().doc.paths_of(arr).unwrap().len(), 6);
    run(&r, &mut w, "edit.undo", json!({}));
    assert_eq!(w.active().doc.paths_of(arr).unwrap().len(), 3);
}

#[test]
fn req_02_019_bake_virtual_array_into_real_copies() {
    let (r, mut w) = (reg(), Workbench::default());
    let a = rect(&r, &mut w, 0., 0.);
    let arr = run(&r, &mut w, "array.virtual", json!({"ids": [a], "layout": {"kind": "grid", "columns": 2, "rows": 2, "dx": 15, "dy": 15}}))["id"].as_u64().unwrap();
    let ids = run(&r, &mut w, "array.bake", json!({"id": arr}))["ids"].clone();
    assert_eq!(ids.as_array().unwrap().len(), 4);
    assert!(w.active().doc.find(arr).is_none());
    assert_eq!(w.active().doc.objects.len(), 4);
    assert_eq!(w.active().doc.bbox().unwrap().unwrap().x1, 25.0);
}

// ---------- klony ----------

#[test]
fn req_02_020_clone_command_and_unlink() {
    let (r, mut w) = (reg(), Workbench::default());
    let a = rect(&r, &mut w, 0., 0.);
    let c = run(
        &r,
        &mut w,
        "object.clone",
        json!({"ids": [a], "offset": [40, 0]}),
    )["ids"][0]
        .as_u64()
        .unwrap();
    assert_eq!(w.active().doc.find(c).unwrap().kind.type_name(), "clone");
    assert_eq!(bbox(&w, c), [40., 0., 50., 10.]);
    run(
        &r,
        &mut w,
        "object.transform",
        json!({"ids": [a], "scale": [1, 3], "anchor": "origin"}),
    );
    assert_eq!(bbox(&w, c)[3], 30.0, "klon śledzi źródło");
    let rep = inspect::report(w.active(), inspect::Detail::Summary).unwrap();
    assert_eq!(
        rep.objects.iter().find(|o| o.id == c).unwrap().source,
        Some(a)
    );
    run(&r, &mut w, "object.unlink", json!({"ids": [c]}));
    run(
        &r,
        &mut w,
        "object.transform",
        json!({"ids": [a], "scale": [1, 0.5], "anchor": "origin"}),
    );
    assert_eq!(bbox(&w, c)[3], 30.0, "po odłączeniu klon jest niezależny");
    assert_ne!(w.active().doc.find(c).unwrap().kind.type_name(), "clone");
}

#[test]
fn deleting_clone_source_keeps_clone_geometry() {
    let (r, mut w) = (reg(), Workbench::default());
    let a = rect(&r, &mut w, 0., 0.);
    let c = run(
        &r,
        &mut w,
        "object.clone",
        json!({"ids": [a], "offset": [20, 0]}),
    )["ids"][0]
        .as_u64()
        .unwrap();
    run(&r, &mut w, "object.delete", json!({"ids": [a]}));
    assert_eq!(bbox(&w, c), [20., 0., 30., 10.]);
    run(&r, &mut w, "edit.undo", json!({}));
    assert_eq!(
        w.active().doc.find(c).unwrap().kind.type_name(),
        "clone",
        "cofnięcie przywraca klon"
    );
}

// ---------- obiekty pomocnicze ----------

#[test]
fn req_02_021_helper_objects() {
    let (r, mut w) = (reg(), Workbench::default());
    let g = run(
        &r,
        &mut w,
        "shape.guide",
        json!({"p0": [0, 50], "p1": [400, 50]}),
    )["id"]
        .as_u64()
        .unwrap();
    let o = w.active().doc.find(g).unwrap();
    assert!(o.helper);
    assert_eq!(o.layer, bc_doc::T2_INDEX);
    let a = rect(&r, &mut w, 0., 0.);
    run(
        &r,
        &mut w,
        "object.set",
        json!({"ids": [a], "helper": true}),
    );
    assert!(w.active().doc.find(a).unwrap().helper);
    let rep = inspect::report(w.active(), inspect::Detail::Summary).unwrap();
    assert!(rep.objects.iter().all(|o| o.helper));
}

// ---------- szablony ----------

#[test]
fn req_02_053_template_opens_as_new_document() {
    let (r, mut w) = (reg(), Workbench::default());
    rect(&r, &mut w, 0., 0.);
    run(
        &r,
        &mut w,
        "layer.set",
        json!({"index": 1, "name": "Cięcie"}),
    );
    let dir = tmp("tpl");
    let p = dir.join("wizytowka.bcraft-template");
    run(
        &r,
        &mut w,
        "file.save_template",
        json!({"path": p.to_str().unwrap()}),
    );
    assert!(
        err(
            &r,
            &mut w,
            "file.save_template",
            json!({"path": dir.join("x.bcraft").to_str().unwrap()})
        )
        .code
            == "invalid_params"
    );
    let mut w2 = Workbench::default();
    run(
        &r,
        &mut w2,
        "file.open",
        json!({"path": p.to_str().unwrap()}),
    );
    let s = w2.active();
    assert!(
        s.path.is_none(),
        "szablon otwiera się jako nowy, niezapisany dokument"
    );
    assert_eq!(s.doc.objects.len(), 1);
    assert_eq!(s.doc.layer(1).unwrap().name, "Cięcie");
    assert!(
        err(&r, &mut w2, "file.save", json!({})).code == "invalid_params",
        "zapis wymaga nowej ścieżki — szablon nie zostanie nadpisany"
    );
}

// ---------- wiele dokumentów i schowek ----------

#[test]
fn req_02_060_multiple_documents_in_tabs() {
    let (r, mut w) = (reg(), Workbench::default());
    rect(&r, &mut w, 0., 0.);
    let res = run(&r, &mut w, "doc.new", json!({"name": "Drugi"}));
    assert_eq!(
        (res["index"].as_u64(), res["count"].as_u64()),
        (Some(1), Some(2))
    );
    assert_eq!(w.active().doc.objects.len(), 0);
    let list = run(&r, &mut w, "doc.list", json!({}));
    assert_eq!(list["active"], 1);
    assert_eq!(list["docs"][0]["objects"], 1);
    assert_eq!(list["docs"][1]["name"], "Drugi");
    run(&r, &mut w, "doc.switch", json!({"index": 0}));
    assert_eq!(w.active().doc.objects.len(), 1);
    assert_eq!(
        err(&r, &mut w, "doc.switch", json!({"index": 5})).code,
        "invalid_params"
    );
    // zamknięcie dokumentu ze zmianami wymaga force
    assert_eq!(
        err(&r, &mut w, "doc.close", json!({"index": 0})).code,
        "unsaved_changes"
    );
    run(&r, &mut w, "doc.close", json!({"index": 0, "force": true}));
    assert_eq!(w.docs.len(), 1);
    run(&r, &mut w, "doc.close", json!({}));
    assert_eq!(
        w.docs.len(),
        1,
        "ostatnia zakładka zostaje zastąpiona pustym dokumentem"
    );
}

#[test]
fn req_02_061_copy_between_documents() {
    let (r, mut w) = (reg(), Workbench::default());
    let a = rect(&r, &mut w, 5., 5.);
    run(
        &r,
        &mut w,
        "layer.add",
        json!({"color": "#ff00ff", "name": "Róż"}),
    );
    run(
        &r,
        &mut w,
        "object.assign_layer",
        json!({"ids": [a], "layer": 32}),
    );
    let c = run(
        &r,
        &mut w,
        "object.clone",
        json!({"ids": [a], "offset": [20, 0]}),
    )["ids"][0]
        .as_u64()
        .unwrap();
    assert_eq!(
        run(&r, &mut w, "edit.copy", json!({"ids": [a, c]}))["count"],
        2
    );
    run(&r, &mut w, "doc.new", json!({}));
    let ids: Vec<u64> =
        serde_json::from_value(run(&r, &mut w, "edit.paste", json!({}))["ids"].clone()).unwrap();
    assert_eq!(ids.len(), 2);
    let d = &w.active().doc;
    assert!(
        d.validate().is_ok(),
        "klon bez źródła w nowym dokumencie został odłączony"
    );
    assert_eq!(d.bbox().unwrap().unwrap().x0, 5.0, "te same współrzędne");
    assert_eq!(
        d.layer(32).unwrap().color,
        "#ff00ff",
        "brakująca warstwa skopiowana razem z obiektami"
    );
    let again: Vec<u64> = serde_json::from_value(
        run(&r, &mut w, "edit.paste", json!({"offset": [100, 0]}))["ids"].clone(),
    )
    .unwrap();
    assert!(
        again.iter().all(|i| !ids.contains(i)),
        "każde wklejenie ma nowe id"
    );
    run(&r, &mut w, "edit.undo", json!({}));
    assert_eq!(
        w.active().doc.objects.len(),
        2,
        "wklejenie jest krokiem historii"
    );
}

#[test]
fn cut_removes_and_paste_restores() {
    let (r, mut w) = (reg(), Workbench::default());
    let a = rect(&r, &mut w, 0., 0.);
    run(&r, &mut w, "edit.cut", json!({"ids": [a]}));
    assert!(w.active().doc.objects.is_empty());
    run(&r, &mut w, "edit.paste", json!({}));
    assert_eq!(w.active().doc.objects.len(), 1);
}

// ---------- autozapis i odzyskiwanie ----------

#[test]
fn req_02_054_autosave_every_n_minutes() {
    let (r, mut w) = (reg(), Workbench::default());
    let dir = tmp("autosave");
    w.autosave = Some(Autosave {
        dir: dir.clone(),
        interval: Duration::from_secs(300),
    });
    let t0 = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000);
    assert!(
        w.autosave_tick(t0).unwrap().is_empty(),
        "czysty dokument nie jest zapisywany"
    );
    rect(&r, &mut w, 0., 0.);
    assert!(
        w.autosave_tick(t0).unwrap().is_empty(),
        "odliczanie startuje od pierwszej zmiany"
    );
    assert!(w
        .autosave_tick(t0 + Duration::from_secs(299))
        .unwrap()
        .is_empty());
    let saved = w.autosave_tick(t0 + Duration::from_secs(300)).unwrap();
    assert_eq!(saved.len(), 1);
    assert!(saved[0].exists());
    assert!(
        w.autosave_tick(t0 + Duration::from_secs(900))
            .unwrap()
            .is_empty(),
        "bez nowych zmian nie ma zapisu"
    );
    // zwykły zapis usuwa plik autozapisu
    let p = dir.join("projekt.bcraft");
    run(
        &r,
        &mut w,
        "file.save",
        json!({"path": p.to_str().unwrap()}),
    );
    assert!(!saved[0].exists());
}

#[test]
fn req_02_055_crash_recovery() {
    let r = reg();
    let dir = tmp("recover");
    {
        // „poprzednia sesja”, która uległa awarii: autozapis został na dysku
        let mut w = Workbench {
            autosave: Some(Autosave {
                dir: dir.clone(),
                interval: Duration::from_secs(60),
            }),
            ..Workbench::default()
        };
        rect(&r, &mut w, 1., 2.);
        let t = SystemTime::now();
        w.autosave_tick(t).unwrap();
        w.autosave_tick(t + Duration::from_secs(60)).unwrap();
    }
    let mut w = Workbench {
        autosave: Some(Autosave {
            dir: dir.clone(),
            interval: Duration::from_secs(60),
        }),
        ..Workbench::default()
    };
    let list = run(&r, &mut w, "file.recover_list", json!({}));
    let files = list["files"].as_array().unwrap();
    assert_eq!(files.len(), 1, "{list}");
    let path = files[0]["path"].as_str().unwrap().to_string();
    let res = run(&r, &mut w, "file.recover", json!({"path": path}));
    assert_eq!(res["index"], 1, "odzyskany dokument w nowej zakładce");
    let s = w.active();
    assert_eq!(s.doc.objects.len(), 1);
    assert!(
        s.dirty && s.path.is_none(),
        "odzyskany dokument wymaga zapisania"
    );
}

// ---------- preferencje i kopie zapasowe ----------

#[test]
fn req_02_056_preferences_with_backups() {
    let dir = tmp("prefs");
    let store = PrefStore::new(&dir);
    assert_eq!(store.load().unwrap(), json!({}));
    store.save(&json!({"ui": {"theme": "dark"}})).unwrap();
    assert!(
        store.backups().unwrap().is_empty(),
        "pierwszy zapis nie ma czego kopiować"
    );
    std::thread::sleep(Duration::from_millis(5));
    store.save(&json!({"ui": {"theme": "light"}})).unwrap();
    let b = store.backups().unwrap();
    assert_eq!(b.len(), 1);
    assert_eq!(store.load().unwrap()["ui"]["theme"], "light");
    let restored = store.restore(&b[0].name).unwrap();
    assert_eq!(restored["ui"]["theme"], "dark");
    assert_eq!(store.load().unwrap()["ui"]["theme"], "dark");
    assert_eq!(
        store.backups().unwrap().len(),
        2,
        "przywrócenie też tworzy kopię — można je cofnąć"
    );
    assert!(
        store.restore("../../etc/passwd").is_err(),
        "nazwy spoza katalogu kopii odrzucane"
    );
    let small = PrefStore {
        dir: dir.clone(),
        keep: 2,
    };
    for i in 0..5 {
        std::thread::sleep(Duration::from_millis(3));
        small.save(&json!({"n": i})).unwrap();
    }
    assert_eq!(small.backups().unwrap().len(), 2, "limit kopii");
}

#[test]
fn req_02_056_preferences_commands() {
    let (r, mut w) = (reg(), Workbench::default());
    assert_eq!(
        err(&r, &mut w, "prefs.get", json!({})).code,
        "invalid_state",
        "bez magazynu preferencji"
    );
    w.prefs = Some(PrefStore::new(tmp("prefs-cmd")));
    run(
        &r,
        &mut w,
        "prefs.set",
        json!({"key": "ui.theme", "value": "dark"}),
    );
    run(
        &r,
        &mut w,
        "prefs.set",
        json!({"key": "ui.units", "value": "mm"}),
    );
    assert_eq!(
        run(&r, &mut w, "prefs.get", json!({"key": "ui.theme"}))["value"],
        "dark"
    );
    assert_eq!(
        run(&r, &mut w, "prefs.get", json!({}))["value"]["ui"]["units"],
        "mm"
    );
    let backups = run(&r, &mut w, "prefs.backups", json!({}))["backups"].clone();
    assert_eq!(backups.as_array().unwrap().len(), 1);
    run(
        &r,
        &mut w,
        "prefs.restore",
        json!({"name": backups[0]["name"]}),
    );
    assert!(run(&r, &mut w, "prefs.get", json!({"key": "ui.units"}))["value"].is_null());
    assert_eq!(
        r.get("prefs.set").unwrap().spec.safety_tier,
        SafetyTier::Config,
        "zmiana preferencji wymaga potwierdzenia"
    );
}

#[test]
fn req_02_056_backup_diff_against_current() {
    let (r, mut w) = (reg(), Workbench::default());
    w.prefs = Some(PrefStore::new(tmp("prefs-diff")));
    let set = |w: &mut Workbench, key: &str, value: Value| {
        run(&r, w, "prefs.set", json!({"key": key, "value": value}));
    };
    set(&mut w, "ui.theme", json!("dark"));
    set(&mut w, "ui.units", json!("mm"));
    set(&mut w, "ui.toolbars", json!({"main": "top"}));
    // bieżące: theme=light, units usunięte, toolbars.main zmienione, nowy klucz mode
    set(&mut w, "ui.theme", json!("light"));
    set(&mut w, "ui.units", Value::Null);
    set(&mut w, "ui.toolbars", json!({"main": "left"}));
    set(&mut w, "ui.mode", json!("beginner"));
    let backups = run(&r, &mut w, "prefs.backups", json!({}))["backups"].clone();
    // kopia sprzed pierwszej zmiany: {theme: dark, units: mm, toolbars.main: top}
    let name = backups
        .as_array()
        .unwrap()
        .iter()
        .map(|b| b["name"].as_str().unwrap().to_string())
        .find(|n| {
            let d = run(&r, &mut w, "prefs.backup_diff", json!({"name": n}));
            d["changes"]
                .as_array()
                .unwrap()
                .iter()
                .any(|c| c["key"] == "ui.toolbars.main" && c["backup"] == "top")
                && d["changes"].as_array().unwrap().len() == 4
        })
        .expect("kopia sprzed zmian");
    let d = run(&r, &mut w, "prefs.backup_diff", json!({"name": name}));
    assert_eq!(d["name"], name.as_str());
    assert!(d["created_ms"].as_u64().unwrap() > 0);
    let changes: Vec<(String, Value, Value)> = d["changes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| {
            (
                c["key"].as_str().unwrap().to_string(),
                c["backup"].clone(),
                c["current"].clone(),
            )
        })
        .collect();
    assert_eq!(
        changes,
        vec![
            ("ui.mode".into(), Value::Null, json!("beginner")),
            ("ui.theme".into(), json!("dark"), json!("light")),
            ("ui.toolbars.main".into(), json!("top"), json!("left")),
            ("ui.units".into(), json!("mm"), Value::Null),
        ],
        "różnice liść po liściu, posortowane po kluczu; brak klucza = null"
    );
    assert_eq!(
        err(
            &r,
            &mut w,
            "prefs.backup_diff",
            json!({"name": "../prefs.json"})
        )
        .code,
        "invalid_params"
    );
    assert_eq!(
        r.get("prefs.backup_diff").unwrap().spec.safety_tier,
        SafetyTier::Read,
        "podgląd różnic niczego nie zmienia"
    );
}

// ---------- edycja wewnątrz grup ----------

#[test]
fn req_04_055_edit_inside_groups_without_ungrouping() {
    let (r, mut w) = (reg(), Workbench::default());
    let a = rect(&r, &mut w, 0., 0.);
    let b = rect(&r, &mut w, 5., 0.);
    let c = rect(&r, &mut w, 30., 0.);
    let g = run(&r, &mut w, "object.group", json!({"ids": [a, b, c]}))["id"]
        .as_u64()
        .unwrap();
    run(
        &r,
        &mut w,
        "object.transform",
        json!({"ids": [g], "translate": [100, 0]}),
    );
    // grupa w grupie
    let inner = run(&r, &mut w, "object.group", json!({"ids": [a, b]}))["id"]
        .as_u64()
        .unwrap();
    assert_eq!(w.active().doc.parent_of(inner), Some(Some(g)));
    // kolejność wewnątrz grupy
    run(
        &r,
        &mut w,
        "object.order",
        json!({"ids": [inner], "to": "front"}),
    );
    let kids: Vec<u64> = w
        .active()
        .doc
        .find(g)
        .unwrap()
        .children()
        .iter()
        .map(|o| o.id)
        .collect();
    assert_eq!(kids, vec![c, inner]);
    // operacja logiczna na dzieciach — wynik zostaje w tej samej grupie, w tym samym miejscu świata
    run(&r, &mut w, "object.ungroup", json!({"ids": [inner]}));
    let res = run(
        &r,
        &mut w,
        "path.boolean",
        json!({"ids": [a, b], "op": "union"}),
    )["ids"][0]
        .as_u64()
        .unwrap();
    assert_eq!(w.active().doc.parent_of(res), Some(Some(g)));
    assert_eq!(bbox(&w, res), [100., 0., 115., 10.]);
    // tablica z elementu grupy
    let copies = run(
        &r,
        &mut w,
        "array.grid",
        json!({"ids": [c], "columns": 2, "rows": 1, "dx": 20, "dy": 0}),
    )["ids"][0]
        .as_u64()
        .unwrap();
    assert_eq!(w.active().doc.parent_of(copies), Some(Some(g)));
    assert_eq!(bbox(&w, copies)[0], 150.0);
    assert_eq!(w.active().doc.objects.len(), 1, "nic nie wypadło z grupy");
}
