//! Testy M1.4c3 na poziomie poleceń: parametry kształtów w oknie Właściwości
//! (REQ-03-047), podgląd stanów historii (REQ-03-048), obrazy w notatkach
//! (REQ-03-049).

use super::*;
use base64::Engine;
use bc_doc::Kind;
use serde_json::{json, Value};

fn run(r: &Registry, s: &mut Session, id: &str, p: Value) -> Value {
    s.execute(r, id, p.clone())
        .unwrap_or_else(|e| panic!("{id} {p}: {e}"))
}

fn err(r: &Registry, s: &mut Session, id: &str, p: Value) -> CmdError {
    s.execute(r, id, p.clone())
        .err()
        .unwrap_or_else(|| panic!("{id} {p}: oczekiwany błąd"))
}

fn b64(data: &[u8]) -> String {
    base64::engine::general_purpose::STANDARD.encode(data)
}

fn unb64(v: &Value) -> Vec<u8> {
    base64::engine::general_purpose::STANDARD
        .decode(v.as_str().expect("base64"))
        .expect("poprawny base64")
}

/// Mały prawdziwy PNG (podgląd pustego dokumentu).
fn png() -> Vec<u8> {
    bc_sim::render_design(
        &bc_doc::Document::default(),
        &bc_sim::PreviewOptions {
            max_size: 16,
            ..Default::default()
        },
    )
    .unwrap()
    .png
}

#[test]
fn req_03_047_shape_set_params_edits_parametric_shapes() {
    let (r, mut s) = (Registry::builtin(), Session::default());
    let rect = run(
        &r,
        &mut s,
        "shape.rect",
        json!({"x": 10, "y": 20, "w": 40, "h": 20}),
    )["id"]
        .clone();
    let poly = run(
        &r,
        &mut s,
        "shape.polygon",
        json!({"cx": 100, "cy": 100, "r": 10, "sides": 6}),
    )["id"]
        .clone();
    let star = run(
        &r,
        &mut s,
        "shape.star",
        json!({"cx": 150, "cy": 100, "r_outer": 10, "r_inner": 5, "points": 5}),
    )["id"]
        .clone();
    let ell = run(
        &r,
        &mut s,
        "shape.ellipse",
        json!({"cx": 200, "cy": 100, "rx": 5, "ry": 3}),
    )["id"]
        .clone();
    let steps = s.history.undo_labels().len();

    run(
        &r,
        &mut s,
        "shape.set_params",
        json!({"id": rect, "params": {"radius": 3, "w": 50}}),
    );
    assert!(matches!(s.doc.find(rect.as_u64().unwrap()).unwrap().kind,
        Kind::Rect { x, y, w, h, radius } if (x, y, w, h, radius) == (10., 20., 50., 20., 3.)));
    run(
        &r,
        &mut s,
        "shape.set_params",
        json!({"id": poly, "params": {"sides": 8, "rotation": 22.5}}),
    );
    assert!(matches!(s.doc.find(poly.as_u64().unwrap()).unwrap().kind,
        Kind::Polygon { sides: 8, rotation, .. } if rotation == 22.5));
    run(
        &r,
        &mut s,
        "shape.set_params",
        json!({"id": star, "params": {"points": 7, "r_middle": 7.5}}),
    );
    assert!(matches!(s.doc.find(star.as_u64().unwrap()).unwrap().kind,
        Kind::Star { points: 7, r_middle: Some(m), .. } if m == 7.5));
    run(
        &r,
        &mut s,
        "shape.set_params",
        json!({"id": ell, "params": {"rx": 8}}),
    );
    assert!(matches!(s.doc.find(ell.as_u64().unwrap()).unwrap().kind,
        Kind::Ellipse { rx, ry, .. } if (rx, ry) == (8., 3.)));
    // każda zmiana to jeden krok historii
    assert_eq!(s.history.undo_labels().len(), steps + 4);
    assert_eq!(s.history.undo_labels().last().unwrap(), "shape.set_params");

    // pole, którego ten kształt nie ma
    assert_eq!(
        err(
            &r,
            &mut s,
            "shape.set_params",
            json!({"id": rect, "params": {"sides": 5}})
        )
        .code,
        "invalid_params"
    );
    // wartość poza zakresem: dokument bez zmian
    let before = s.doc.clone();
    assert_eq!(
        err(
            &r,
            &mut s,
            "shape.set_params",
            json!({"id": poly, "params": {"sides": 2}})
        )
        .code,
        "invalid_params"
    );
    assert_eq!(
        err(
            &r,
            &mut s,
            "shape.set_params",
            json!({"id": rect, "params": {"w": -1}})
        )
        .code,
        "invalid_params"
    );
    assert_eq!(s.doc, before);
    // ścieżka nie ma parametrów
    let path = run(&r, &mut s, "shape.path", json!({"d": "M10 10 L20 10"}))["id"].clone();
    assert_eq!(
        err(
            &r,
            &mut s,
            "shape.set_params",
            json!({"id": path, "params": {"w": 1}})
        )
        .code,
        "invalid_params"
    );
    assert_eq!(
        err(
            &r,
            &mut s,
            "shape.set_params",
            json!({"id": 999, "params": {"w": 1}})
        )
        .code,
        "not_found"
    );
}

#[test]
fn req_03_048_history_preview_renders_past_and_future_states() {
    let (r, mut s) = (Registry::builtin(), Session::default());
    run(
        &r,
        &mut s,
        "shape.rect",
        json!({"x": 10, "y": 10, "w": 100, "h": 100}),
    );
    run(
        &r,
        &mut s,
        "shape.ellipse",
        json!({"cx": 200, "cy": 200, "rx": 50, "ry": 50}),
    );

    let now = run(&r, &mut s, "edit.history_preview", json!({"offset": 0}));
    assert_eq!(now["objects"], 2);
    let one = run(
        &r,
        &mut s,
        "edit.history_preview",
        json!({"offset": -1, "max_size": 64}),
    );
    assert_eq!(one["objects"], 1);
    assert_eq!(one["label"], "shape.ellipse", "krok, który ten stan cofa");
    let start = run(
        &r,
        &mut s,
        "edit.history_preview",
        json!({"offset": -2, "max_size": 64}),
    );
    assert_eq!(start["objects"], 0);
    // obraz PNG o zadanym rozmiarze
    let img = unb64(&one["png_base64"]);
    assert_eq!(&img[..8], b"\x89PNG\r\n\x1a\n");
    assert_eq!(
        one["width"]
            .as_u64()
            .unwrap()
            .max(one["height"].as_u64().unwrap()),
        64
    );
    assert_ne!(
        one["png_base64"], start["png_base64"],
        "różne stany — różne obrazy"
    );
    assert_eq!(
        err(&r, &mut s, "edit.history_preview", json!({"offset": -3})).code,
        "invalid_params"
    );
    assert_eq!(
        err(&r, &mut s, "edit.history_preview", json!({"offset": 1})).code,
        "invalid_params"
    );

    // po cofnięciu podgląd kroku do ponowienia
    run(&r, &mut s, "edit.undo", json!({}));
    let fwd = run(&r, &mut s, "edit.history_preview", json!({"offset": 1}));
    assert_eq!(fwd["objects"], 2);
    assert_eq!(fwd["label"], "shape.ellipse");
    // podgląd niczego nie zmienia
    assert_eq!(s.doc.objects.len(), 1);
    assert_eq!(s.history.redo_labels().len(), 1);
}

#[test]
fn req_03_049_note_images_add_get_remove_and_roundtrip() {
    let (r, mut s) = (Registry::builtin(), Session::default());
    run(
        &r,
        &mut s,
        "document.set_notes",
        json!({"text": "Sklejka 3 mm"}),
    );
    let data = png();
    let a = run(
        &r,
        &mut s,
        "document.add_note_image",
        json!({"data_base64": b64(&data)}),
    );
    let name = a["name"].as_str().unwrap().to_string();
    assert!(name.ends_with(".png"), "{name}");
    assert!(s.doc.notes.starts_with("Sklejka 3 mm"));
    assert!(
        s.doc.notes.contains(&format!("![](notes/{name})")),
        "{}",
        s.doc.notes
    );
    let got = run(&r, &mut s, "document.note_image", json!({"name": name}));
    assert_eq!(got["mime"], "image/png");
    assert_eq!(unb64(&got["data_base64"]), data);
    // druga wklejka dostaje inną nazwę
    let b = run(
        &r,
        &mut s,
        "document.add_note_image",
        json!({"data_base64": b64(&data)}),
    );
    assert_ne!(b["name"], a["name"]);
    // nie-obraz jest odrzucany
    assert_eq!(
        err(
            &r,
            &mut s,
            "document.add_note_image",
            json!({"data_base64": b64(b"not an image")})
        )
        .code,
        "invalid_params"
    );
    assert_eq!(
        err(
            &r,
            &mut s,
            "document.note_image",
            json!({"name": "brak.png"})
        )
        .code,
        "not_found"
    );

    // zapis i odczyt .bcraft
    let dir = std::env::temp_dir().join(format!("bc-m14c-notes-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let p = dir.join("n.bcraft");
    run(
        &r,
        &mut s,
        "file.save",
        json!({"path": p.to_str().unwrap()}),
    );
    let mut s2 = Session::default();
    run(
        &r,
        &mut s2,
        "file.open",
        json!({"path": p.to_str().unwrap()}),
    );
    assert_eq!(s2.doc.notes, s.doc.notes);
    let got2 = run(&r, &mut s2, "document.note_image", json!({"name": name}));
    assert_eq!(unb64(&got2["data_base64"]), data);
    std::fs::remove_dir_all(&dir).ok();

    // usunięcie zdejmuje też odwołanie z tekstu; cofnięcie przywraca obraz
    run(
        &r,
        &mut s,
        "document.remove_note_image",
        json!({"name": name}),
    );
    assert!(!s.doc.notes.contains(&name));
    assert_eq!(
        err(&r, &mut s, "document.note_image", json!({"name": name})).code,
        "not_found"
    );
    run(&r, &mut s, "edit.undo", json!({}));
    assert_eq!(
        unb64(&run(&r, &mut s, "document.note_image", json!({"name": name}))["data_base64"]),
        data
    );
}
