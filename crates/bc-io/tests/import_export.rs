//! Testy importu i eksportu (M1.3, ADR 0008).

use bc_doc::{Document, Kind};
use bc_geom::Rect;
use bc_io::*;

fn close(a: f64, b: f64, eps: f64) -> bool {
    (a - b).abs() <= eps
}

fn rect_eq(r: Rect, e: [f64; 4], eps: f64) -> bool {
    close(r.x0, e[0], eps)
        && close(r.y0, e[1], eps)
        && close(r.x1, e[2], eps)
        && close(r.y1, e[3], eps)
}

/// Importuje do świeżego dokumentu (bez grupowania) i zwraca dokument + ids.
fn into_doc(imp: &Imported, layer_by: LayerBy) -> (Document, Vec<u64>) {
    let mut d = Document::new("t");
    let opt = ImportOptions {
        layer_by,
        group: false,
        ..Default::default()
    };
    let a = apply(&mut d, imp, &opt).unwrap();
    (d, a.ids)
}

fn bbox(d: &Document, id: u64) -> Rect {
    d.bbox_of(id).unwrap().unwrap()
}

// ---------- SVG ----------

const INKSCAPE: &str = r##"<?xml version="1.0"?>
<svg xmlns="http://www.w3.org/2000/svg" xmlns:inkscape="http://www.inkscape.org/namespaces/inkscape"
     width="100mm" height="50mm" viewBox="0 0 100 50">
  <g inkscape:groupmode="layer" id="layer1" inkscape:label="Cięcie">
    <rect x="10" y="10" width="20" height="10" fill="none" stroke="#ff0000"/>
  </g>
  <g inkscape:groupmode="layer" id="layer2" inkscape:label="Grawer">
    <circle cx="70" cy="25" r="10" fill="#000000"/>
  </g>
</svg>"##;

#[test]
fn req_02_062_svg_import_mm_and_y_flip() {
    let imp = svg::import(INKSCAPE.as_bytes(), &ImportOptions::default()).unwrap();
    assert_eq!(imp.items.len(), 2);
    let (d, ids) = into_doc(&imp, LayerBy::Color);
    // y w dół (10..20) -> y w górę: 50-20 .. 50-10
    assert!(
        rect_eq(bbox(&d, ids[0]), [10., 30., 30., 40.], 1e-6),
        "{:?}",
        bbox(&d, ids[0])
    );
    assert!(rect_eq(bbox(&d, ids[1]), [60., 15., 80., 35.], 1e-3));
}

#[test]
fn req_02_062_svg_auto_dpi() {
    // Illustrator: px przy 72 DPI -> 72 px = 25,4 mm
    let ai = r##"<?xml version="1.0"?><!-- Generator: Adobe Illustrator 28.0, SVG Export Plug-In -->
<svg xmlns="http://www.w3.org/2000/svg" width="144" height="144" viewBox="0 0 144 144">
<rect x="0" y="0" width="72" height="72" fill="none" stroke="#000"/></svg>"##;
    let (d, ids) = into_doc(
        &svg::import(ai.as_bytes(), &ImportOptions::default()).unwrap(),
        LayerBy::Color,
    );
    assert!(
        close(bbox(&d, ids[0]).width(), 25.4, 1e-6),
        "{:?}",
        bbox(&d, ids[0])
    );
    // Inkscape/przeglądarka bez jednostek: 96 DPI
    let plain = r##"<svg xmlns="http://www.w3.org/2000/svg" width="192" height="192"><rect width="96" height="96" fill="none" stroke="#000"/></svg>"##;
    let (d, ids) = into_doc(
        &svg::import(plain.as_bytes(), &ImportOptions::default()).unwrap(),
        LayerBy::Color,
    );
    assert!(close(bbox(&d, ids[0]).width(), 25.4, 1e-6));
    // Affinity (xmlns:serif) -> 72 DPI
    let affinity = r##"<svg xmlns="http://www.w3.org/2000/svg" xmlns:serif="http://www.serif.com/" width="144" height="144"><rect width="72" height="72" fill="none" stroke="#000"/></svg>"##;
    let (d, ids) = into_doc(
        &svg::import(affinity.as_bytes(), &ImportOptions::default()).unwrap(),
        LayerBy::Color,
    );
    assert!(close(bbox(&d, ids[0]).width(), 25.4, 1e-6));
    // jawne DPI nadpisuje wykrycie
    let opt = ImportOptions {
        dpi: Some(25.4),
        ..Default::default()
    };
    let (d, ids) = into_doc(
        &svg::import(plain.as_bytes(), &opt).unwrap(),
        LayerBy::Color,
    );
    assert!(close(bbox(&d, ids[0]).width(), 96.0, 1e-6));
}

#[test]
fn req_02_064_svg_layers_by_color() {
    let imp = svg::import(INKSCAPE.as_bytes(), &ImportOptions::default()).unwrap();
    let (d, ids) = into_doc(&imp, LayerBy::Color);
    let red = d.find(ids[0]).unwrap().layer;
    assert_eq!(
        d.layer(red).unwrap().color,
        "#ff0000",
        "nowa warstwa z dokładnym kolorem"
    );
    assert!(red >= bc_doc::FIRST_CUSTOM_INDEX);
    assert_eq!(
        d.find(ids[1]).unwrap().layer,
        0,
        "czarne wypełnienie -> istniejąca warstwa #000000"
    );
}

#[test]
fn req_02_065_svg_layers_by_name() {
    let imp = svg::import(INKSCAPE.as_bytes(), &ImportOptions::default()).unwrap();
    let mut d = Document::new("t");
    d.layer_mut(5).unwrap().name = "grawer".into(); // istniejąca, inna wielkość liter
    let a = apply(
        &mut d,
        &imp,
        &ImportOptions {
            layer_by: LayerBy::Name,
            group: false,
            ..Default::default()
        },
    )
    .unwrap();
    let l0 = d.find(a.ids[0]).unwrap().layer;
    assert_eq!(d.layer(l0).unwrap().name, "Cięcie");
    assert_eq!(a.layers_added, vec![l0]);
    assert_eq!(d.find(a.ids[1]).unwrap().layer, 5);
}

#[test]
fn req_02_065_svg_label_without_id() {
    // regresja: usvg usuwa grupy bez id — etykieta warstwy nie może zginąć
    let s = r##"<svg xmlns="http://www.w3.org/2000/svg" xmlns:inkscape="http://www.inkscape.org/namespaces/inkscape" width="10mm" height="10mm" viewBox="0 0 10 10"><g inkscape:label="Bez id"><rect width="5" height="5" fill="none" stroke="#000"/></g></svg>"##;
    let imp = svg::import(s.as_bytes(), &ImportOptions::default()).unwrap();
    assert_eq!(imp.items[0].name_key, Some(LayerKey::Name("Bez id".into())));
}

#[test]
fn req_02_066_svg_nested() {
    let nested = r##"<svg xmlns="http://www.w3.org/2000/svg" width="100mm" height="100mm" viewBox="0 0 100 100">
<g transform="translate(5 0)"><svg x="45" y="0" width="20" height="20" viewBox="0 0 10 10">
<rect width="10" height="10" fill="none" stroke="#000"/></svg></g></svg>"##;
    let (d, ids) = into_doc(
        &svg::import(nested.as_bytes(), &ImportOptions::default()).unwrap(),
        LayerBy::Color,
    );
    // usvg liczy na f32: dokładność ~1e-5 mm (ADR 0008)
    assert!(
        rect_eq(bbox(&d, ids[0]), [50., 80., 70., 100.], 1e-4),
        "{:?}",
        bbox(&d, ids[0])
    );
}

#[test]
fn svg_text_and_images_warn() {
    let s = r##"<svg xmlns="http://www.w3.org/2000/svg" width="10mm" height="10mm" viewBox="0 0 10 10"><text x="1" y="5">A</text>
<image x="0" y="0" width="1" height="1" href="data:image/png;base64,iVBORw0KGgo="/><rect width="1" height="1" stroke="#000" fill="none"/></svg>"##;
    let imp = svg::import(s.as_bytes(), &ImportOptions::default()).unwrap();
    assert_eq!(imp.items.len(), 1);
    assert!(
        imp.warnings.iter().any(|w| w.contains("tekst")),
        "{:?}",
        imp.warnings
    );
}

#[test]
fn req_02_063_svg_export_roundtrip() {
    let mut d = Document::new("t");
    let a = d
        .add(
            2,
            Kind::Rect {
                x: 10.,
                y: 20.,
                w: 30.,
                h: 15.,
                radius: 0.,
            },
        )
        .unwrap();
    let b = d
        .add(
            5,
            Kind::Ellipse {
                cx: 100.,
                cy: 100.,
                rx: 10.,
                ry: 5.,
            },
        )
        .unwrap();
    let h = d
        .add(
            0,
            Kind::Path {
                d: "M0 0 L1 1".into(),
            },
        )
        .unwrap();
    d.find_mut(h).unwrap().helper = true;
    let out = String::from_utf8(export(&d, ExportFormat::Svg, &ExportOptions::default()).unwrap())
        .unwrap();
    assert!(
        out.contains("width=\"400mm\"") && out.contains("viewBox=\"0 0 400 400\""),
        "{out}"
    );
    assert!(out.contains(&d.layer(2).unwrap().color) && out.contains("inkscape:label=\"C02\""));
    let imp = svg::import(out.as_bytes(), &ImportOptions::default()).unwrap();
    assert_eq!(imp.items.len(), 2, "obiekt pomocniczy pominięty");
    let (d2, ids) = into_doc(&imp, LayerBy::Color);
    assert!(rect_eq(bbox(&d2, ids[0]), [10., 20., 40., 35.], 1e-3));
    assert!(rect_eq(bbox(&d2, ids[1]), [90., 95., 110., 105.], 1e-3));
    assert_eq!(
        d2.find(ids[0]).unwrap().layer,
        2,
        "kolor warstwy wraca na tę samą warstwę"
    );
    let _ = (a, b);
    let sel = String::from_utf8(
        export(
            &d,
            ExportFormat::Svg,
            &ExportOptions {
                ids: Some(vec![b]),
                include_helpers: false,
            },
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(
        svg::import(sel.as_bytes(), &ImportOptions::default())
            .unwrap()
            .items
            .len(),
        1,
        "eksport zaznaczenia"
    );
}

// ---------- DXF ----------

const DXF_R12: &str =
    "0\nSECTION\n2\nHEADER\n9\n$ACADVER\n1\nAC1009\n0\nENDSEC\n0\nSECTION\n2\nENTITIES\n\
0\nLINE\n8\n0\n10\n0.0\n20\n0.0\n30\n0.0\n11\n100.0\n21\n0.0\n31\n0.0\n\
0\nCIRCLE\n8\nCut\n10\n50.0\n20\n50.0\n30\n0.0\n40\n10.0\n\
0\nARC\n8\n0\n10\n0.0\n20\n0.0\n30\n0.0\n40\n20.0\n50\n0.0\n51\n90.0\n\
0\nENDSEC\n0\nEOF\n";

fn dxf_bytes(d: &dxf::Drawing) -> Vec<u8> {
    let mut buf = Vec::new();
    d.save(&mut buf).unwrap();
    buf
}

fn entity(layer: &str, t: dxf::entities::EntityType) -> dxf::entities::Entity {
    let mut e = dxf::entities::Entity::new(t);
    e.common.layer = layer.into();
    e
}

#[test]
fn req_02_068_dxf_r12_import() {
    let imp = dxf_io::import(DXF_R12.as_bytes(), &ImportOptions::default()).unwrap();
    assert_eq!(imp.items.len(), 3);
    let (d, ids) = into_doc(&imp, LayerBy::Name);
    assert!(rect_eq(bbox(&d, ids[0]), [0., 0., 100., 0.], 1e-9));
    assert!(rect_eq(bbox(&d, ids[1]), [40., 40., 60., 60.], 1e-3));
    assert_eq!(d.layer(d.find(ids[1]).unwrap().layer).unwrap().name, "Cut");
}

#[test]
fn req_02_071_dxf_arcs_and_bulges() {
    let (d, ids) = into_doc(
        &dxf_io::import(DXF_R12.as_bytes(), &ImportOptions::default()).unwrap(),
        LayerBy::Single(0),
    );
    assert!(
        rect_eq(bbox(&d, ids[2]), [0., 0., 20., 20.], 1e-3),
        "{:?}",
        bbox(&d, ids[2])
    );
    // LWPOLYLINE z wybrzuszeniem 1 = półokrąg
    let mut dr = dxf::Drawing::new();
    dr.header.version = dxf::enums::AcadVersion::R2018;
    let mut lw = dxf::entities::LwPolyline::default();
    lw.vertices.push(dxf::LwPolylineVertex {
        x: 0.,
        y: 0.,
        bulge: 1.0,
        ..Default::default()
    });
    lw.vertices.push(dxf::LwPolylineVertex {
        x: 20.,
        y: 0.,
        ..Default::default()
    });
    lw.flags = 1; // zamknięta
    dr.add_entity(entity("0", dxf::entities::EntityType::LwPolyline(lw)));
    let (d, ids) = into_doc(
        &dxf_io::import(&dxf_bytes(&dr), &ImportOptions::default()).unwrap(),
        LayerBy::Name,
    );
    let b = bbox(&d, ids[0]);
    assert!(
        close(b.width(), 20., 1e-3) && close(b.height(), 10., 1e-3),
        "półokrąg: {b:?}"
    );
    assert!(bc_geom::is_closed(&d.paths_of(ids[0]).unwrap()[0].2));
}

#[test]
fn req_02_069_dxf_splines() {
    let mut dr = dxf::Drawing::new();
    dr.header.version = dxf::enums::AcadVersion::R2018;
    let sp = dxf::entities::Spline {
        degree_of_curve: 3,
        knot_values: vec![0., 0., 0., 0., 1., 1., 1., 1.],
        control_points: vec![
            dxf::Point::new(0., 0., 0.),
            dxf::Point::new(0., 10., 0.),
            dxf::Point::new(10., 10., 0.),
            dxf::Point::new(10., 0., 0.),
        ],
        ..Default::default()
    };
    dr.add_entity(entity("0", dxf::entities::EntityType::Spline(sp)));
    let (d, ids) = into_doc(
        &dxf_io::import(&dxf_bytes(&dr), &ImportOptions::default()).unwrap(),
        LayerBy::Name,
    );
    let b = bbox(&d, ids[0]);
    // Bézier sześcienny (0,0)(0,10)(10,10)(10,0): maksimum y = 7,5
    assert!(rect_eq(b, [0., 0., 10., 7.5], 0.01), "{b:?}");
}

#[test]
fn req_02_070_dxf_blocks() {
    let mut dr = dxf::Drawing::new();
    dr.header.version = dxf::enums::AcadVersion::R2018;
    let mut blk = dxf::Block {
        name: "OTWOR".into(),
        base_point: dxf::Point::new(1., 0., 0.),
        ..Default::default()
    };
    blk.entities.push(entity(
        "0",
        dxf::entities::EntityType::Circle(dxf::entities::Circle::new(
            dxf::Point::new(1., 0., 0.),
            2.,
        )),
    ));
    dr.add_block(blk);
    for (x, s) in [(10., 1.), (50., 2.)] {
        let ins = dxf::entities::Insert {
            name: "OTWOR".into(),
            location: dxf::Point::new(x, 0., 0.),
            x_scale_factor: s,
            y_scale_factor: s,
            z_scale_factor: 1.,
            ..Default::default()
        };
        dr.add_entity(entity("0", dxf::entities::EntityType::Insert(ins)));
    }
    let (d, ids) = into_doc(
        &dxf_io::import(&dxf_bytes(&dr), &ImportOptions::default()).unwrap(),
        LayerBy::Name,
    );
    assert_eq!(ids.len(), 2);
    assert!(
        rect_eq(bbox(&d, ids[0]), [8., -2., 12., 2.], 1e-3),
        "{:?}",
        bbox(&d, ids[0])
    );
    assert!(
        rect_eq(bbox(&d, ids[1]), [46., -4., 54., 4.], 1e-3),
        "{:?}",
        bbox(&d, ids[1])
    );
}

#[test]
fn req_02_072_dxf_text_ignore_option() {
    let mut dr = dxf::Drawing::new();
    dr.header.version = dxf::enums::AcadVersion::R2018;
    let t = dxf::entities::Text {
        value: "Opis".into(),
        text_height: 5.,
        ..Default::default()
    };
    dr.add_entity(entity("0", dxf::entities::EntityType::Text(t)));
    dr.add_entity(entity(
        "0",
        dxf::entities::EntityType::Line(dxf::entities::Line::new(
            dxf::Point::new(0., 0., 0.),
            dxf::Point::new(1., 0., 0.),
        )),
    ));
    let ignored = dxf_io::import(
        &dxf_bytes(&dr),
        &ImportOptions {
            ignore_text: true,
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(ignored.items.len(), 1);
    assert!(ignored.warnings.is_empty(), "{:?}", ignored.warnings);
    let not = dxf_io::import(
        &dxf_bytes(&dr),
        &ImportOptions {
            ignore_text: false,
            ..Default::default()
        },
    )
    .unwrap();
    assert!(
        not.warnings.iter().any(|w| w.contains("tekst")),
        "{:?}",
        not.warnings
    );
}

/// DEV-004: do M4 tekst DXF (MTEXT, tekst w blokach) jest pomijany z jednym
/// ostrzeżeniem, a geometria obok niego importuje się bez zmian.
#[test]
fn req_02_072_dxf_mtext_and_block_text_skipped() {
    let mut dr = dxf::Drawing::new();
    dr.header.version = dxf::enums::AcadVersion::R2018;
    let mut blk = dxf::Block {
        name: "TABLICZKA".into(),
        ..Default::default()
    };
    blk.entities.push(entity(
        "0",
        dxf::entities::EntityType::Text(dxf::entities::Text {
            value: "Nr 1".into(),
            text_height: 3.,
            ..Default::default()
        }),
    ));
    blk.entities.push(entity(
        "0",
        dxf::entities::EntityType::Circle(dxf::entities::Circle::new(
            dxf::Point::new(0., 0., 0.),
            2.,
        )),
    ));
    dr.add_block(blk);
    dr.add_entity(entity(
        "0",
        dxf::entities::EntityType::Insert(dxf::entities::Insert {
            name: "TABLICZKA".into(),
            location: dxf::Point::new(20., 0., 0.),
            x_scale_factor: 1.,
            y_scale_factor: 1.,
            z_scale_factor: 1.,
            ..Default::default()
        }),
    ));
    dr.add_entity(entity(
        "0",
        dxf::entities::EntityType::MText(dxf::entities::MText {
            text: "Opis\\Pdrugi wiersz".into(),
            initial_text_height: 4.,
            ..Default::default()
        }),
    ));
    let bytes = dxf_bytes(&dr);
    for ignore_text in [false, true] {
        let imp = dxf_io::import(
            &bytes,
            &ImportOptions {
                ignore_text,
                ..Default::default()
            },
        )
        .unwrap();
        // tylko okrąg z bloku; tekst nie daje geometrii
        assert_eq!(imp.items.len(), 1, "ignore_text={ignore_text}");
        let text_warnings = imp.warnings.iter().filter(|w| w.contains("tekst")).count();
        assert_eq!(
            text_warnings,
            usize::from(!ignore_text),
            "ignore_text={ignore_text}: {:?}",
            imp.warnings
        );
    }
}

#[test]
fn req_02_073_dxf_units_from_header() {
    let mut dr = dxf::Drawing::new();
    dr.header.version = dxf::enums::AcadVersion::R2018;
    dr.header.default_drawing_units = dxf::enums::Units::Inches;
    dr.add_entity(entity(
        "0",
        dxf::entities::EntityType::Line(dxf::entities::Line::new(
            dxf::Point::new(0., 0., 0.),
            dxf::Point::new(1., 0., 0.),
        )),
    ));
    let (d, ids) = into_doc(
        &dxf_io::import(&dxf_bytes(&dr), &ImportOptions::default()).unwrap(),
        LayerBy::Name,
    );
    assert!(close(bbox(&d, ids[0]).width(), 25.4, 1e-9));
    dr.header.default_drawing_units = dxf::enums::Units::Centimeters;
    let (d, ids) = into_doc(
        &dxf_io::import(&dxf_bytes(&dr), &ImportOptions::default()).unwrap(),
        LayerBy::Name,
    );
    assert!(close(bbox(&d, ids[0]).width(), 10.0, 1e-9));
}

fn sample_doc() -> Document {
    let mut d = Document::new("t");
    d.layer_mut(1).unwrap().name = "Cięcie".into();
    d.add(
        1,
        Kind::Rect {
            x: 0.,
            y: 0.,
            w: 30.,
            h: 20.,
            radius: 0.,
        },
    )
    .unwrap();
    d.add(
        2,
        Kind::Ellipse {
            cx: 50.,
            cy: 50.,
            rx: 10.,
            ry: 10.,
        },
    )
    .unwrap();
    d
}

#[test]
fn req_02_074_dxf_export_ac1009() {
    let bytes = export(
        &sample_doc(),
        ExportFormat::DxfR12,
        &ExportOptions::default(),
    )
    .unwrap();
    let dr = dxf::Drawing::load(&mut bytes.as_slice()).unwrap();
    assert_eq!(dr.header.version, dxf::enums::AcadVersion::R12);
    let imp = dxf_io::import(&bytes, &ImportOptions::default()).unwrap();
    let (d, ids) = into_doc(&imp, LayerBy::Name);
    assert!(rect_eq(bbox(&d, ids[0]), [0., 0., 30., 20.], 1e-6));
    assert!(
        rect_eq(bbox(&d, ids[1]), [40., 40., 60., 60.], 0.02),
        "krzywe spłaszczone: {:?}",
        bbox(&d, ids[1])
    );
    assert_eq!(
        d.layer(d.find(ids[0]).unwrap().layer).unwrap().name,
        "Cięcie"
    );
}

#[test]
fn req_02_075_dxf_export_ac1021() {
    let bytes = export(
        &sample_doc(),
        ExportFormat::Dxf2007,
        &ExportOptions::default(),
    )
    .unwrap();
    let dr = dxf::Drawing::load(&mut bytes.as_slice()).unwrap();
    assert_eq!(dr.header.version, dxf::enums::AcadVersion::R2007);
    assert_eq!(
        dr.header.default_drawing_units,
        dxf::enums::Units::Millimeters
    );
    assert!(
        dr.entities()
            .any(|e| matches!(e.specific, dxf::entities::EntityType::Spline(_))),
        "krzywe jako SPLINE"
    );
    let (d, ids) = into_doc(
        &dxf_io::import(&bytes, &ImportOptions::default()).unwrap(),
        LayerBy::Name,
    );
    assert!(
        rect_eq(bbox(&d, ids[1]), [40., 40., 60., 60.], 1e-3),
        "{:?}",
        bbox(&d, ids[1])
    );
}

// ---------- HPGL ----------

#[test]
fn req_02_080_hpgl_import() {
    let plt = b"IN;SP1;PU0,0;PD400,0,400,400;SP2;PU800,0;PD1200,0;PR0,400;";
    let imp = hpgl::import(plt, &ImportOptions::default()).unwrap();
    let (d, ids) = into_doc(&imp, LayerBy::Color);
    assert_eq!(ids.len(), 2);
    assert!(rect_eq(bbox(&d, ids[0]), [0., 0., 10., 10.], 1e-9));
    assert!(
        rect_eq(bbox(&d, ids[1]), [20., 0., 30., 10.], 1e-9),
        "PR = ruch względny"
    );
    assert_eq!(d.find(ids[0]).unwrap().layer, 0);
    assert_eq!(d.find(ids[1]).unwrap().layer, 1, "pióro 2 -> warstwa 1");
}

#[test]
fn req_02_081_hpgl_export_roundtrip() {
    let out = export(&sample_doc(), ExportFormat::Hpgl, &ExportOptions::default()).unwrap();
    let text = String::from_utf8(out.clone()).unwrap();
    assert!(
        text.starts_with("IN;") && text.contains("SP2;") && text.contains("SP3;"),
        "{text}"
    );
    let (d, ids) = into_doc(
        &hpgl::import(&out, &ImportOptions::default()).unwrap(),
        LayerBy::Color,
    );
    assert!(rect_eq(bbox(&d, ids[0]), [0., 0., 30., 20.], 0.025));
    assert_eq!(d.find(ids[1]).unwrap().layer, 2);
}

// ---------- .lbrn2 ----------

const SAMPLE: &[u8] = include_bytes!("fixtures/plik_przykladowy.lbrn2");

#[test]
fn req_02_092_lbrn2_user_sample() {
    let doc = lbrn::lbrn2_document(SAMPLE, "plik_przykladowy").unwrap();
    assert!(doc.validate().is_ok());
    assert_eq!(doc.objects.len(), 3);
    let sizes: Vec<(f64, f64, u8)> = doc
        .objects
        .iter()
        .map(|o| {
            let b = doc.bbox_of(o.id).unwrap().unwrap();
            assert!(
                close(b.center().x, 169.5, 1e-9) && close(b.center().y, 122.0, 1e-9),
                "środek z XForm"
            );
            (b.width(), b.height(), o.layer)
        })
        .collect();
    assert_eq!(sizes, vec![(55., 39., 2), (62., 44., 0), (231., 102., 1)]);
    assert!(
        doc.objects.iter().all(|o| o.kind.type_name() == "rect"),
        "prostokąty zostają parametryczne"
    );
    for i in 0..3u8 {
        let l = doc.layer(i).unwrap();
        assert_eq!(
            (l.name.as_str(), l.speed, l.power_max),
            (["C00", "C01", "C02"][i as usize], 100.0, 20.0)
        );
    }
}

#[test]
fn lbrn2_unknown_shapes_are_reported_not_guessed() {
    let s = br#"<?xml version="1.0"?><LightBurnProject AppVersion="2.0.05" FormatVersion="1">
<CutSetting type="Cut"><index Value="0"/><name Value="C00"/><maxPower Value="20"/><speed Value="100"/></CutSetting>
<Shape Type="Spiral" CutIndex="0"><XForm>1 0 0 1 0 0</XForm></Shape>
<Shape Type="Rect" CutIndex="0" W="10" H="10" Cr="2"><XForm>1 0 0 1 5 5</XForm></Shape></LightBurnProject>"#;
    let imp = lbrn::import_lbrn2(s, &ImportOptions::default()).unwrap();
    assert_eq!(imp.items.len(), 1);
    assert!(
        imp.warnings.iter().any(|w| w.contains("Spiral")),
        "{:?}",
        imp.warnings
    );
    assert!(matches!(imp.items[0].kind, Kind::Rect { radius, .. } if radius == 2.0));
    assert!(lbrn::import_lbrn2(b"<NotAProject/>", &ImportOptions::default()).is_err());
}

// ---------- formaty, pliki ----------

#[test]
fn formats_by_extension() {
    use std::path::Path;
    assert_eq!(Format::from_path(Path::new("a.SVG")), Some(Format::Svg));
    assert_eq!(Format::from_path(Path::new("a.dxf")), Some(Format::Dxf));
    assert_eq!(Format::from_path(Path::new("a.plt")), Some(Format::Hpgl));
    assert_eq!(Format::from_path(Path::new("a.hpgl")), Some(Format::Hpgl));
    assert_eq!(Format::from_path(Path::new("a.lbrn2")), Some(Format::Lbrn2));
    assert_eq!(Format::from_path(Path::new("a.doc")), None);
    assert_eq!(ExportFormat::parse("dxf"), Some(ExportFormat::Dxf2007));
    assert_eq!(ExportFormat::parse("dxf-r12"), Some(ExportFormat::DxfR12));
    assert_eq!(
        ExportFormat::from_path(Path::new("x.plt")),
        Some(ExportFormat::Hpgl)
    );
    let dir = std::env::temp_dir().join(format!("bc-io-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let p = dir.join("t.svg");
    std::fs::write(&p, INKSCAPE).unwrap();
    assert_eq!(
        import_file(&p, &ImportOptions::default())
            .unwrap()
            .items
            .len(),
        2
    );
    assert!(matches!(
        import_file(&dir.join("x.xyz"), &ImportOptions::default()),
        Err(IoError::Unsupported(_))
    ));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn apply_groups_by_default() {
    let imp = svg::import(INKSCAPE.as_bytes(), &ImportOptions::default()).unwrap();
    let mut d = Document::new("t");
    let a = apply(&mut d, &imp, &ImportOptions::default()).unwrap();
    assert_eq!(a.ids.len(), 1);
    assert_eq!(d.find(a.ids[0]).unwrap().kind.type_name(), "group");
    assert!(d.validate().is_ok());
}

// ---------- odporność (uzupełnienie cargo-fuzz, REQ-19-003…005) ----------

/// Deterministyczny generator (LCG) — powtarzalne mutacje bez zależności.
struct Lcg(u64);
impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.0 >> 33
    }
}

fn mutate(seed: &[u8], rng: &mut Lcg) -> Vec<u8> {
    let mut v = seed.to_vec();
    for _ in 0..(1 + rng.next() % 8) {
        if v.is_empty() {
            break;
        }
        let i = (rng.next() as usize) % v.len();
        match rng.next() % 4 {
            0 => v[i] = rng.next() as u8,
            1 => {
                v.remove(i);
            }
            2 => v.insert(i, b"0123456789-.<>\"/\n"[(rng.next() % 17) as usize]),
            _ => v.truncate(i),
        }
    }
    v
}

#[test]
fn req_19_003_004_005_importers_never_panic_on_mutated_input() {
    let mut rng = Lcg(0x5eed);
    let seeds: [(&str, &[u8]); 4] = [
        ("svg", INKSCAPE.as_bytes()),
        ("dxf", DXF_R12.as_bytes()),
        ("lbrn2", SAMPLE),
        ("hpgl", b"IN;SP1;PU0,0;PD400,0,400,400;"),
    ];
    for (kind, seed) in seeds {
        for _ in 0..400 {
            let m = mutate(seed, &mut rng);
            let opt = ImportOptions::default();
            let r = std::panic::catch_unwind(|| match kind {
                "svg" => svg::import(&m, &opt).map(|_| ()),
                "dxf" => dxf_io::import(&m, &opt).map(|_| ()),
                "lbrn2" => lbrn::import_lbrn2(&m, &opt).map(|_| ()),
                _ => hpgl::import(&m, &opt).map(|_| ()),
            });
            assert!(
                r.is_ok(),
                "{kind}: panika na danych {:?}",
                String::from_utf8_lossy(&m)
            );
        }
    }
}

// ---------- regresje z fuzzingu (run 40824017757): brak pamięci na ogromnych współrzędnych ----------

const SVG_OOM: &[u8] = include_bytes!("fixtures/fuzz/svg-oom-huge-arc.svg");
const DXF_OOM: &[u8] = include_bytes!("fixtures/fuzz/dxf-oom-huge-radius.dxf");

#[test]
fn req_19_003_regression_svg_huge_arc_coordinates() {
    let t = std::time::Instant::now();
    match svg::import(SVG_OOM, &ImportOptions::default()) {
        Ok(imp) => assert!(
            imp.warnings.iter().any(|w| w.contains("zakres")),
            "{:?}",
            imp.warnings
        ),
        Err(e) => assert!(e.to_string().contains("zakres"), "{e}"),
    }
    assert!(t.elapsed().as_secs() < 5, "import trwał {:?}", t.elapsed());
}

#[test]
fn req_19_004_regression_dxf_huge_radius() {
    let t = std::time::Instant::now();
    let imp = dxf_io::import(DXF_OOM, &ImportOptions::default()).unwrap();
    assert!(
        imp.warnings.iter().any(|w| w.contains("zakres")),
        "{:?}",
        imp.warnings
    );
    assert!(t.elapsed().as_secs() < 5);
}

#[test]
fn dxf_insert_arrays_are_bounded() {
    let mut dr = dxf::Drawing::new();
    dr.header.version = dxf::enums::AcadVersion::R2018;
    let mut blk = dxf::Block {
        name: "B".into(),
        ..Default::default()
    };
    blk.entities.push(entity(
        "0",
        dxf::entities::EntityType::Line(dxf::entities::Line::new(
            dxf::Point::new(0., 0., 0.),
            dxf::Point::new(1., 0., 0.),
        )),
    ));
    dr.add_block(blk);
    let ins = dxf::entities::Insert {
        name: "B".into(),
        x_scale_factor: 1.,
        y_scale_factor: 1.,
        z_scale_factor: 1.,
        row_count: 30000,
        column_count: 30000,
        row_spacing: 1.,
        column_spacing: 1.,
        ..Default::default()
    };
    dr.add_entity(entity("0", dxf::entities::EntityType::Insert(ins)));
    let t = std::time::Instant::now();
    let imp = dxf_io::import(&dxf_bytes(&dr), &ImportOptions::default()).unwrap();
    assert!(
        imp.warnings.iter().any(|w| w.contains("limit")),
        "{:?}",
        imp.warnings
    );
    assert!(t.elapsed().as_secs() < 5);
}
