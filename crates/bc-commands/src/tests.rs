use super::*;
use crate::inspect::Detail;
use serde_json::json;

fn setup() -> (Registry, Session) {
    (Registry::builtin(), Session::default())
}

fn rect(r: &Registry, s: &mut Session, x: f64) -> u64 {
    s.execute(r, "shape.rect", json!({"x": x, "y": 0, "w": 10, "h": 10}))
        .unwrap()["id"]
        .as_u64()
        .unwrap()
}

#[test]
fn req_01_030_to_035_every_command_has_full_spec() {
    let r = Registry::builtin();
    let all = r.list(None);
    assert!(all.len() >= 20);
    for c in all {
        // id w formacie obszar.akcja
        assert!(
            c.id.contains('.')
                && c.id
                    .chars()
                    .all(|ch| ch.is_ascii_lowercase() || ch == '.' || ch == '_'),
            "{}",
            c.id
        );
        assert!(
            !c.title.is_empty() && !c.description.is_empty(),
            "{}: opis",
            c.id
        );
        assert_eq!(
            c.params_schema["type"], "object",
            "{}: schemat parametrów",
            c.id
        );
        assert!(c.result_schema.is_object(), "{}: schemat wyniku", c.id);
        // flagi są jawnie w spec (serializacja ma wszystkie pola)
        let v = serde_json::to_value(c).unwrap();
        for k in [
            "undoable",
            "requires_device",
            "safety_tier",
            "shortcut",
            "requirements",
        ] {
            assert!(v.get(k).is_some(), "{}: brak {k}", c.id);
        }
        if c.id.starts_with("device.") {
            assert!(c.requires_device && c.safety_tier >= SafetyTier::Motion);
        }
    }
    assert_eq!(r.get("object.group").unwrap().spec.shortcut, Some("Ctrl+G"));
}

#[test]
fn req_21_006_commands_reference_requirements() {
    let r = Registry::builtin();
    for c in r.list(None) {
        for req in c.requirements {
            assert!(
                req.len() == 10 && req.starts_with("REQ-"),
                "{}: {req}",
                c.id
            );
        }
    }
    assert!(
        r.list(None)
            .iter()
            .filter(|c| !c.requirements.is_empty())
            .count()
            > 15
    );
}

#[test]
fn list_filter() {
    let r = Registry::builtin();
    let shapes = r.list(Some("shape."));
    assert!(shapes.len() >= 3 && shapes.iter().all(|c| c.id.starts_with("shape.")));
    assert!(
        !r.list(Some("ELIPS")).is_empty(),
        "filtr bez rozróżniania wielkości liter, także po tytule"
    );
}

#[test]
fn req_01_037_explicit_ids_or_selection() {
    let (r, mut s) = setup();
    let a = rect(&r, &mut s, 0.);
    let b = rect(&r, &mut s, 20.);
    assert_eq!(s.selection, vec![b], "nowy obiekt zostaje zaznaczony");
    // bez ids -> zaznaczenie
    s.execute(&r, "object.transform", json!({"translate": [5, 0]}))
        .unwrap();
    let rep = inspect::report(&s, Detail::Summary).unwrap();
    let bb = |id| {
        rep.objects
            .iter()
            .find(|o| o.id == id)
            .unwrap()
            .bbox
            .unwrap()
    };
    assert_eq!(bb(a)[0], 0.0);
    assert_eq!(bb(b)[0], 25.0);
    // jawne ids
    s.execute(
        &r,
        "object.transform",
        json!({"ids": [a], "translate": [1, 0]}),
    )
    .unwrap();
    assert_eq!(
        inspect::report(&s, Detail::Summary).unwrap().objects[0]
            .bbox
            .unwrap()[0],
        1.0
    );
    // pusta selekcja i brak ids -> błąd
    s.execute(&r, "selection.clear", json!({})).unwrap();
    let e = s.execute(&r, "object.delete", json!({})).unwrap_err();
    assert_eq!(e.code, "invalid_params");
    assert_eq!(
        s.execute(&r, "object.delete", json!({"ids": [999]}))
            .unwrap_err()
            .code,
        "not_found"
    );
}

#[test]
fn req_01_038_batch_is_one_history_step_and_atomic() {
    let (r, mut s) = setup();
    let steps: Vec<BatchStep> = serde_json::from_value(json!([
        {"id": "shape.rect", "params": {"x": 0, "y": 0, "w": 10, "h": 10}},
        {"id": "shape.ellipse", "params": {"cx": 50, "cy": 50, "rx": 5, "ry": 5}},
        {"id": "object.transform", "params": {"rotate": 90}}
    ]))
    .unwrap();
    s.batch(&r, &steps).unwrap();
    assert_eq!(s.doc.objects.len(), 2);
    assert_eq!(s.history.undo_labels(), vec!["batch"]);
    s.execute(&r, "edit.undo", json!({})).unwrap();
    assert!(s.doc.objects.is_empty(), "jedno cofnięcie usuwa cały wsad");

    // błąd w kroku 2 -> nic się nie zmienia
    let bad: Vec<BatchStep> = serde_json::from_value(json!([
        {"id": "shape.rect", "params": {"x": 0, "y": 0, "w": 10, "h": 10}},
        {"id": "shape.rect", "params": {"x": 0, "y": 0, "w": -1, "h": 10}}
    ]))
    .unwrap();
    let before = s.doc.clone();
    let (idx, e) = s.batch(&r, &bad).unwrap_err();
    assert_eq!((idx, e.code), (1, "invalid_params"));
    assert_eq!(s.doc, before);
    // polecenia zmieniające sesję nie mogą być we wsadzie
    let ctl: Vec<BatchStep> = serde_json::from_value(json!([{"id": "edit.undo"}])).unwrap();
    assert!(s.batch(&r, &ctl).is_err());
}

#[test]
fn req_15_017_018_undo_redo() {
    let (r, mut s) = setup();
    rect(&r, &mut s, 0.);
    s.execute(&r, "layer.set", json!({"index": 0, "name": "Cięcie"}))
        .unwrap();
    assert_eq!(s.doc.layers[0].name, "Cięcie");
    let u = s.execute(&r, "edit.undo", json!({})).unwrap();
    assert_eq!(u["label"], "layer.set");
    assert_eq!(s.doc.layers[0].name, "C00");
    s.execute(&r, "edit.redo", json!({})).unwrap();
    assert_eq!(s.doc.layers[0].name, "Cięcie");
    // odczyt nie tworzy kroku historii
    s.execute(&r, "document.inspect", json!({})).unwrap();
    assert_eq!(s.history.undo_labels().len(), 2);
}

#[test]
fn req_04_094_group_ungroup() {
    let (r, mut s) = setup();
    let a = rect(&r, &mut s, 0.);
    let b = rect(&r, &mut s, 20.);
    let g = s
        .execute(&r, "object.group", json!({"ids": [a, b]}))
        .unwrap()["id"]
        .as_u64()
        .unwrap();
    assert_eq!(s.doc.objects.len(), 1);
    s.execute(
        &r,
        "object.transform",
        json!({"ids": [g], "translate": [100, 0]}),
    )
    .unwrap();
    let ids = s
        .execute(&r, "object.ungroup", json!({"ids": [g]}))
        .unwrap();
    assert_eq!(ids["ids"], json!([a, b]));
    let rep = inspect::report(&s, Detail::Summary).unwrap();
    assert_eq!(
        rep.objects[0].bbox.unwrap()[0],
        100.0,
        "transformacja grupy przechodzi na dzieci"
    );
    assert_eq!(rep.objects[1].bbox.unwrap()[0], 120.0);
}

#[test]
fn req_02_024_transform_rotate_scale_mirror() {
    let (r, mut s) = setup();
    let a = s
        .execute(&r, "shape.rect", json!({"x": 0, "y": 0, "w": 20, "h": 10}))
        .unwrap()["id"]
        .as_u64()
        .unwrap();
    s.execute(
        &r,
        "object.transform",
        json!({"ids": [a], "rotate": 90, "anchor": "origin"}),
    )
    .unwrap();
    let b = inspect::report(&s, Detail::Full).unwrap().objects[0]
        .bbox
        .unwrap();
    assert!(
        (b[0] + 10.0).abs() < 1e-9 && (b[3] - 20.0).abs() < 1e-9,
        "{b:?}"
    );
    s.execute(
        &r,
        "object.transform",
        json!({"ids": [a], "scale": [2, 2], "mirror": "horizontal"}),
    )
    .unwrap();
    let rep = inspect::report(&s, Detail::Full).unwrap();
    let o = &rep.objects[0];
    assert!(
        (o.length - 120.0).abs() < 1e-6,
        "obwód po skali x2: {}",
        o.length
    );
    // geometria parametryczna zachowana (nie wypieczona)
    assert_eq!(o.geometry.as_ref().unwrap()["w"], 20.0);
}

#[test]
fn locked_objects_refuse_transform() {
    let (r, mut s) = setup();
    let a = rect(&r, &mut s, 0.);
    s.execute(&r, "object.set", json!({"ids": [a], "locked": true}))
        .unwrap();
    assert_eq!(
        s.execute(&r, "object.transform", json!({"translate": [1, 1]}))
            .unwrap_err()
            .code,
        "locked"
    );
}

#[test]
fn shape_path_from_d_and_points() {
    let (r, mut s) = setup();
    s.execute(
        &r,
        "shape.path",
        json!({"d": "M0 0 C 0 10 10 10 10 0 Q 5 -5 0 0 Z", "layer": 1}),
    )
    .unwrap();
    s.execute(
        &r,
        "shape.path",
        json!({"points": [[0,0],[10,0],[10,10]], "closed": true}),
    )
    .unwrap();
    let rep = inspect::report(&s, Detail::Summary).unwrap();
    assert!(rep.objects.iter().all(|o| o.closed));
    assert_eq!(
        s.execute(
            &r,
            "shape.path",
            json!({"d": "M0 0", "points": [[0,0],[1,1]]})
        )
        .unwrap_err()
        .code,
        "invalid_params"
    );
    assert_eq!(
        s.execute(&r, "shape.path", json!({"d": "bzdura"}))
            .unwrap_err()
            .code,
        "invalid_params"
    );
    assert_eq!(
        s.execute(
            &r,
            "shape.rect",
            json!({"x": 0, "y": 0, "w": 1, "h": 1, "kolor": 3})
        )
        .unwrap_err()
        .code,
        "invalid_params",
        "nieznane pola odrzucane"
    );
}

#[test]
fn device_commands_need_device() {
    let (r, mut s) = setup();
    let e = s
        .execute(&r, "device.jog", json!({"axis": "x", "distance": 10}))
        .unwrap_err();
    assert_eq!(e.code, "no_device");
}

#[test]
fn req_14_011_file_save_open() {
    let (r, mut s) = setup();
    rect(&r, &mut s, 0.);
    s.execute(&r, "document.set_notes", json!({"text": "notatka"}))
        .unwrap();
    let dir = std::env::temp_dir().join(format!("bc-cmd-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let p = dir.join("x.bcraft");
    s.execute(&r, "file.save", json!({"path": p.to_str().unwrap()}))
        .unwrap();
    assert!(!s.dirty);
    let mut s2 = Session::default();
    let res = s2
        .execute(&r, "file.open", json!({"path": p.to_str().unwrap()}))
        .unwrap();
    assert_eq!(res["objects"], 1);
    assert_eq!(s2.doc.notes, "notatka");
    let (_, extras) = bc_doc::bcraft::load(&p).unwrap();
    assert!(extras.thumbnail_png.is_some(), "miniatura zapisana");
    assert!(s
        .execute(
            &r,
            "file.save",
            json!({"path": dir.join("x.txt").to_str().unwrap()})
        )
        .is_err());
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn req_15_019_inspect_report_fields() {
    let (r, mut s) = setup();
    let a = rect(&r, &mut s, 0.);
    s.execute(
        &r,
        "shape.path",
        json!({"points": [[0,0],[30,40]], "layer": 4}),
    )
    .unwrap();
    let v = s.execute(&r, "document.inspect", json!({})).unwrap();
    assert_eq!(
        v["layers"].as_array().unwrap().len(),
        2,
        "summary: tylko użyte warstwy"
    );
    let o = &v["objects"][0];
    for k in [
        "id",
        "type",
        "layer",
        "bbox",
        "transform",
        "length",
        "closed",
    ] {
        assert!(o.get(k).is_some(), "brak {k}");
    }
    assert_eq!(o["id"], a);
    assert_eq!(v["objects"][1]["closed"], false);
    assert_eq!(v["objects"][1]["length"], 50.0);
    assert_eq!(v["stats"]["objects"], 2);
    let full = s
        .execute(&r, "document.inspect", json!({"detail": "full"}))
        .unwrap();
    assert_eq!(
        full["layers"].as_array().unwrap().len(),
        32,
        "30 kolorów + T1 + T2"
    );
}
