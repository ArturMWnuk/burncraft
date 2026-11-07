//! Testy M1.3 na poziomie poleceń: file.import, file.export, otwieranie .lbrn2.

use super::*;
use serde_json::{json, Value};

const SAMPLE: &[u8] = include_bytes!("../../bc-io/tests/fixtures/plik_przykladowy.lbrn2");

fn dir(name: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("bc-m13-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn run(r: &Registry, w: &mut Workbench, id: &str, p: Value) -> Value {
    w.execute(r, id, p.clone())
        .unwrap_or_else(|e| panic!("{id} {p}: {e}"))
}

const SVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" xmlns:inkscape="http://www.inkscape.org/namespaces/inkscape" width="100mm" height="100mm" viewBox="0 0 100 100">
<g inkscape:label="Cięcie"><rect x="10" y="10" width="20" height="20" fill="none" stroke="#0000ff"/></g></svg>"##;

#[test]
fn req_15_029_file_import_command() {
    let (r, mut w) = (Registry::builtin(), Workbench::default());
    let d = dir("import");
    let p = d.join("a.svg");
    std::fs::write(&p, SVG).unwrap();
    let res = run(
        &r,
        &mut w,
        "file.import",
        json!({"path": p.to_str().unwrap(), "layer_by": "name"}),
    );
    assert_eq!(
        res["ids"].as_array().unwrap().len(),
        1,
        "domyślnie jedna grupa"
    );
    assert_eq!(res["layers_added"].as_array().unwrap().len(), 1);
    let doc = &w.active().doc;
    assert!(doc.layers.iter().any(|l| l.name == "Cięcie"));
    run(&r, &mut w, "edit.undo", json!({}));
    assert!(
        w.active().doc.objects.is_empty(),
        "import to jeden krok historii"
    );
    assert!(
        !w.active().doc.layers.iter().any(|l| l.name == "Cięcie"),
        "cofnięcie usuwa też dodane warstwy"
    );
    let e = w
        .execute(
            &r,
            "file.import",
            json!({"path": d.join("brak.svg").to_str().unwrap()}),
        )
        .unwrap_err();
    assert_eq!(e.code, "failed");
    let e = w
        .execute(
            &r,
            "file.import",
            json!({"path": p.to_str().unwrap(), "layer_by": "single", "layer": 77}),
        )
        .unwrap_err();
    assert_eq!(e.code, "invalid_params");
}

#[test]
fn req_15_030_file_export_command() {
    let (r, mut w) = (Registry::builtin(), Workbench::default());
    run(
        &r,
        &mut w,
        "shape.rect",
        json!({"x": 0, "y": 0, "w": 10, "h": 10, "layer": 3}),
    );
    let d = dir("export");
    for (name, fmt) in [("a.svg", "svg"), ("a.dxf", "dxf"), ("a.plt", "hpgl")] {
        let p = d.join(name);
        let res = run(
            &r,
            &mut w,
            "file.export",
            json!({"path": p.to_str().unwrap()}),
        );
        assert_eq!(res["format"], fmt);
        assert!(std::fs::metadata(&p).unwrap().len() > 20);
    }
    let r12 = d.join("r12.dxf");
    run(
        &r,
        &mut w,
        "file.export",
        json!({"path": r12.to_str().unwrap(), "format": "dxf-r12"}),
    );
    assert!(std::fs::read_to_string(&r12).unwrap().contains("AC1009"));
    assert_eq!(
        w.execute(
            &r,
            "file.export",
            json!({"path": d.join("a.docx").to_str().unwrap()})
        )
        .unwrap_err()
        .code,
        "invalid_params"
    );
    assert!(
        w.active().history.undo_labels().len() == 1,
        "eksport nie zmienia dokumentu"
    );
}

#[test]
fn req_15_027_open_lbrn2_as_new_document() {
    let (r, mut w) = (Registry::builtin(), Workbench::default());
    let d = dir("lbrn");
    let p = d.join("projekt.lbrn2");
    std::fs::write(&p, SAMPLE).unwrap();
    let res = run(
        &r,
        &mut w,
        "file.open",
        json!({"path": p.to_str().unwrap()}),
    );
    assert_eq!(res["objects"], 3);
    let s = w.active();
    assert!(
        s.path.is_none(),
        ".lbrn2 nie zostanie nadpisany zapisem .bcraft"
    );
    assert_eq!(s.doc.name, "projekt");
    assert_eq!(s.doc.layer(1).unwrap().name, "C01");
}

#[test]
fn req_14_012_015_bcraft_roundtrip_via_import_export() {
    let (r, mut w) = (Registry::builtin(), Workbench::default());
    let d = dir("rt");
    let svg = d.join("in.svg");
    std::fs::write(&svg, SVG).unwrap();
    run(
        &r,
        &mut w,
        "file.import",
        json!({"path": svg.to_str().unwrap(), "group": false}),
    );
    let out = d.join("out.svg");
    run(
        &r,
        &mut w,
        "file.export",
        json!({"path": out.to_str().unwrap()}),
    );
    let mut w2 = Workbench::default();
    run(
        &r,
        &mut w2,
        "file.import",
        json!({"path": out.to_str().unwrap(), "group": false}),
    );
    let b1 = w.active().doc.bbox().unwrap().unwrap();
    let b2 = w2.active().doc.bbox().unwrap().unwrap();
    assert!(
        (b1.x0 - b2.x0).abs() < 1e-3 && (b1.y1 - b2.y1).abs() < 1e-3,
        "{b1:?} {b2:?}"
    );
}
