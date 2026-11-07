//! Testy domknięcia M1: operacje widoku, narzędzi i okien GUI jako polecenia
//! rejestru wykonywane przez okno (REQ-01-029).

use super::*;
use serde_json::json;

/// Polecenia wykonywane przez okno GUI.
const WINDOW: &[&str] = &[
    "view.zoom",
    "view.pan",
    "view.zoom_page",
    "view.zoom_selection",
    "view.actual_size",
    "view.get",
    "tool.select",
    "tool.set_option",
    "snap.set",
    "layer.set_active",
    "transform.set_anchor",
    "transform.lock_aspect",
    "dialog.open",
];

#[test]
fn req_01_029_window_commands_are_in_registry() {
    let r = Registry::builtin();
    for id in WINDOW {
        let c = r
            .get(id)
            .unwrap_or_else(|| panic!("brak polecenia okna {id}"));
        let spec = serde_json::to_value(&c.spec).unwrap();
        assert_eq!(spec["executor"], "window", "{id}: {spec}");
        assert_eq!(spec["safety_tier"], "read", "{id}");
        assert_eq!(spec["undoable"], false, "{id}");
        assert!(
            c.spec.requirements.contains(&"REQ-01-029"),
            "{id} deklaruje REQ-01-029"
        );
        assert!(spec["params_schema"].is_object() && spec["result_schema"].is_object());
    }
    // polecenia dokumentu mają wykonawcę "document", obszaru roboczego — "workbench"
    let shape = serde_json::to_value(&r.get("shape.rect").unwrap().spec).unwrap();
    assert_eq!(shape["executor"], "document");
    let tabs = serde_json::to_value(&r.get("doc.new").unwrap().spec).unwrap();
    assert_eq!(tabs["executor"], "workbench");
}

#[test]
fn req_01_029_window_command_needs_window() {
    let r = Registry::builtin();
    let mut s = Session::default();
    let e = s
        .execute(&r, "view.zoom_page", json!({}))
        .expect_err("bez okna błąd");
    assert_eq!(e.code, "no_window");
    let mut b = Workbench::new(Session::default());
    let e = b
        .execute(&r, "tool.select", json!({"tool": "rect"}))
        .expect_err("bez okna błąd");
    assert_eq!(e.code, "no_window");
    // we wsadzie (jeden krok historii dokumentu) polecenia okna są niedozwolone
    let steps = vec![BatchStep {
        id: "view.zoom".into(),
        params: json!({"factor": 2}),
    }];
    let (i, e) = b.batch(&r, &steps).expect_err("wsad odrzucony");
    assert_eq!((i, e.code), (0, "invalid_params"));
}
