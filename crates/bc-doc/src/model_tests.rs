//! Testy modelu M1.1: kształty parametryczne, atrybuty obiektów, warstwy T1/T2, schemat 2.

use super::*;
use crate::bcraft::{document_from_json, from_bytes, to_bytes, Extras};

fn bb(d: &Document) -> Rect {
    d.bbox().unwrap().unwrap()
}

fn close(a: f64, b: f64, eps: f64) -> bool {
    (a - b).abs() <= eps
}

#[test]
fn req_02_034_thirty_color_layers_plus_tools() {
    let d = Document::new("t");
    let cut: Vec<_> = d
        .layers
        .iter()
        .filter(|l| l.role == LayerRole::Cut)
        .collect();
    assert_eq!(cut.len(), 30, "30 warstw kolorów");
    let colors: std::collections::HashSet<_> = cut.iter().map(|l| l.color.clone()).collect();
    assert_eq!(colors.len(), 30);
}

#[test]
fn req_02_035_t1_frame_layer() {
    let d = Document::new("t");
    let t1 = d.layer(T1_INDEX).unwrap();
    assert_eq!(
        (t1.role, t1.name.as_str(), t1.output),
        (LayerRole::Frame, "T1", false)
    );
}

#[test]
fn req_02_036_t2_helper_layer() {
    let d = Document::new("t");
    let t2 = d.layer(T2_INDEX).unwrap();
    assert_eq!(
        (t2.role, t2.name.as_str(), t2.output),
        (LayerRole::Helper, "T2", false)
    );
}

#[test]
fn req_02_008_polygon_object() {
    let mut d = Document::new("t");
    d.add(
        0,
        Kind::Polygon {
            cx: 50.,
            cy: 50.,
            r: 10.,
            sides: 4,
            rotation: 45.,
        },
    )
    .unwrap();
    let b = bb(&d);
    assert!(close(b.width(), 10.0 * 2f64.sqrt(), 1e-9) && close(b.center().x, 50.0, 1e-9));
}

#[test]
fn req_02_009_star_object() {
    let mut d = Document::new("t");
    d.add(
        0,
        Kind::Star {
            cx: 0.,
            cy: 0.,
            r_outer: 10.,
            r_inner: 5.,
            r_middle: None,
            points: 5,
            rotation: 90.,
        },
    )
    .unwrap();
    assert!(close(bb(&d).y1, 10.0, 1e-9));
    assert!(bc_geom::is_closed(&d.objects[0].local_path().unwrap()));
}

#[test]
fn req_02_010_double_star_object() {
    let mut d = Document::new("t");
    d.add(
        0,
        Kind::Star {
            cx: 0.,
            cy: 0.,
            r_outer: 10.,
            r_inner: 3.,
            r_middle: Some(6.),
            points: 4,
            rotation: 0.,
        },
    )
    .unwrap();
    assert_eq!(bc_geom::node_count(&d.objects[0].local_path().unwrap()), 16);
}

#[test]
fn req_02_003_arc_object_is_open() {
    let mut d = Document::new("t");
    d.add(
        0,
        Kind::Arc {
            cx: 0.,
            cy: 0.,
            r: 10.,
            start: 0.,
            sweep: 90.,
        },
    )
    .unwrap();
    let p = d.objects[0].local_path().unwrap();
    assert!(!bc_geom::is_closed(&p));
    assert!(close(
        bc_geom::path_length(&p),
        std::f64::consts::PI * 5.0,
        1e-4
    ));
}

#[test]
fn req_02_033_parametric_shapes_keep_params() {
    let mut d = Document::new("t");
    d.add(
        0,
        Kind::Polygon {
            cx: 1.,
            cy: 2.,
            r: 3.,
            sides: 7,
            rotation: 15.,
        },
    )
    .unwrap();
    d.add(
        0,
        Kind::Star {
            cx: 0.,
            cy: 0.,
            r_outer: 10.,
            r_inner: 3.,
            r_middle: Some(6.),
            points: 4,
            rotation: 0.,
        },
    )
    .unwrap();
    let back = from_bytes(&to_bytes(&d, &Extras::default()).unwrap())
        .unwrap()
        .0;
    assert!(
        matches!(back.objects[0].kind, Kind::Polygon { sides: 7, rotation, .. } if rotation == 15.)
    );
    assert!(matches!(back.objects[1].kind, Kind::Star { r_middle: Some(m), .. } if m == 6.));
}

#[test]
fn invalid_parametric_shape_fails_validation() {
    let mut d = Document::new("t");
    let id = d.alloc_id();
    d.objects.push(Object::new(
        id,
        0,
        Kind::Polygon {
            cx: 0.,
            cy: 0.,
            r: 1.,
            sides: 2,
            rotation: 0.,
        },
    ));
    assert!(d.validate().is_err());
    assert!(d.objects[0].local_path().is_err(), "błąd, nie panika");
}

#[test]
fn req_02_026_029_030_032_attributes_roundtrip() {
    let mut d = Document::new("t");
    let id = d
        .add(
            0,
            Kind::Rect {
                x: 0.,
                y: 0.,
                w: 1.,
                h: 1.,
                radius: 0.,
            },
        )
        .unwrap();
    {
        let o = d.find_mut(id).unwrap();
        o.lock_size = true;
        o.note = Some("otwór na śrubę M3".into());
        o.priority = -2;
        o.tags = vec!["logo".into(), "klient-A".into()];
    }
    let back = from_bytes(&to_bytes(&d, &Extras::default()).unwrap())
        .unwrap()
        .0;
    let o = back.find(id).unwrap();
    assert!(o.lock_size);
    assert_eq!(o.note.as_deref(), Some("otwór na śrubę M3"));
    assert_eq!(o.priority, -2);
    assert_eq!(o.tags, vec!["logo", "klient-A"]);
    // domyślne wartości nie zaśmiecają pliku
    let plain = serde_json::to_value(Object::new(
        1,
        0,
        Kind::Path {
            d: "M0 0 L1 1".into(),
        },
    ))
    .unwrap();
    assert!(
        plain.get("tags").is_none()
            && plain.get("note").is_none()
            && plain.get("lock_size").is_none()
    );
}

#[test]
fn req_02_045_layer_names_saved_per_file() {
    let mut d = Document::new("t");
    d.layer_mut(3).unwrap().name = "Grawer logo".into();
    d.layer_mut(3).unwrap().color = "#123456".into();
    let back = from_bytes(&to_bytes(&d, &Extras::default()).unwrap())
        .unwrap()
        .0;
    assert_eq!(back.layer(3).unwrap().name, "Grawer logo");
    assert_eq!(back.layer(3).unwrap().color, "#123456");
    assert_eq!(
        Document::new("inny").layer(3).unwrap().name,
        "C03",
        "nazwy nie są globalne"
    );
}

#[test]
fn req_02_052_migration_from_schema_1() {
    let v1 = r##"{"schema":1,"name":"m0","units":"mm","workspace":{"width":300,"height":200},
        "layers":[{"index":0,"color":"#000000","name":"C00","mode":"line","speed":10,"power_max":50,
        "power_min":10,"passes":1,"output":true,"show":true,"air_assist":false}],
        "objects":[{"id":1,"type":"ellipse","layer":0,"visible":true,"locked":false,
        "transform":[1,0,0,1,0,0],"cx":0,"cy":0,"rx":5,"ry":5}],"next_id":2}"##;
    let d = document_from_json(v1).unwrap();
    assert_eq!(d.schema, SCHEMA_VERSION);
    assert_eq!(d.layers[0].role, LayerRole::Cut);
    assert_eq!(d.objects[0].priority, 0);
    let out = serde_json::to_value(&d).unwrap();
    assert!(
        crate::bcraft::schema_errors(&out).is_empty(),
        "{:?}",
        crate::bcraft::schema_errors(&out)
    );
}
