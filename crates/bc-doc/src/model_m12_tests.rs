//! Testy modelu M1.2: wirtualne tablice, klony, obiekty pomocnicze, kontenery, schemat 3.

use super::*;
use crate::bcraft::{document_from_json, from_bytes, to_bytes, Extras};

fn rect(x: f64, y: f64, w: f64, h: f64) -> Kind {
    Kind::Rect {
        x,
        y,
        w,
        h,
        radius: 0.,
    }
}

fn grid(columns: u32, rows: u32, dx: f64, dy: f64) -> ArrayLayout {
    ArrayLayout::Grid {
        columns,
        rows,
        dx,
        dy,
        row_shift: 0.0,
    }
}

fn array_doc(layout: ArrayLayout) -> (Document, ObjectId, ObjectId) {
    let mut d = Document::new("t");
    let child = Object::new(d.alloc_id(), 0, rect(0., 0., 10., 10.));
    let cid = child.id;
    let aid = d.alloc_id();
    d.objects.push(Object::new(
        aid,
        0,
        Kind::Array {
            children: vec![child],
            layout,
        },
    ));
    (d, aid, cid)
}

#[test]
fn req_02_019_virtual_array_grid_copies() {
    let (d, aid, _) = array_doc(grid(3, 2, 20., 15.));
    let paths = d.paths_of(aid).unwrap();
    assert_eq!(paths.len(), 6, "3 × 2 kopie");
    let b = d.bbox_of(aid).unwrap().unwrap();
    assert_eq!((b.x0, b.y0, b.x1, b.y1), (0., 0., 50., 25.));
    assert_eq!(
        d.world_paths().unwrap().len(),
        6,
        "kopie w geometrii całego dokumentu"
    );
}

#[test]
fn req_02_019_editing_source_updates_all_copies() {
    let (mut d, aid, cid) = array_doc(grid(4, 1, 20., 0.));
    d.find_mut(cid).unwrap().kind = rect(0., 0., 5., 5.);
    let b = d.bbox_of(aid).unwrap().unwrap();
    assert_eq!(
        (b.x1, b.y1),
        (65., 5.),
        "ostatnia kopia (x=60) ma nowy rozmiar"
    );
    assert!(
        d.find(cid).is_some(),
        "źródło tablicy jest zwykłym obiektem w drzewie"
    );
    assert_eq!(d.parent_affine(cid), Some(Affine::IDENTITY));
}

#[test]
fn req_04_073_virtual_array_circular_and_row_shift() {
    let (d, aid, _) = array_doc(ArrayLayout::Circular {
        count: 4,
        center: [0., -20.],
        angle: 360.,
        rotate_copies: true,
    });
    assert_eq!(d.paths_of(aid).unwrap().len(), 4);
    let b = d.bbox_of(aid).unwrap().unwrap();
    assert!(
        (b.y0 - (-50.)).abs() < 1e-9 && (b.y1 - 10.).abs() < 1e-9,
        "{b:?}"
    );
    let (d2, a2, _) = array_doc(ArrayLayout::Grid {
        columns: 2,
        rows: 2,
        dx: 20.,
        dy: 20.,
        row_shift: 5.,
    });
    let xs: Vec<f64> = d2
        .paths_of(a2)
        .unwrap()
        .iter()
        .map(|(_, _, p)| bc_geom::bbox(p).unwrap().x0)
        .collect();
    assert_eq!(xs, vec![0., 20., 5., 25.]);
}

#[test]
fn array_layout_limits() {
    assert!(
        grid(1000, 1000, 1., 1.).transforms(Point::ORIGIN).is_err(),
        "limit kopii"
    );
    assert!(grid(0, 3, 1., 1.).transforms(Point::ORIGIN).is_err());
    assert!(ArrayLayout::Circular {
        count: 1,
        center: [0., 0.],
        angle: 360.,
        rotate_copies: true
    }
    .transforms(Point::ORIGIN)
    .is_err());
    assert_eq!(
        grid(2, 1, 5., 0.).transforms(Point::ORIGIN).unwrap()[0],
        Affine::IDENTITY,
        "pierwsza kopia = oryginał"
    );
}

#[test]
fn req_02_020_clone_follows_source() {
    let mut d = Document::new("t");
    let src = d.add(2, rect(0., 0., 10., 10.)).unwrap();
    let cl = d.alloc_id();
    let mut c = Object::new(cl, 2, Kind::Clone { source: src });
    c.transform = Affine::translate((50., 0.)).as_coeffs();
    d.objects.push(c);
    assert_eq!(d.bbox_of(cl).unwrap().unwrap().x0, 50.0);
    d.find_mut(src).unwrap().kind = rect(0., 0., 20., 10.);
    assert_eq!(
        d.bbox_of(cl).unwrap().unwrap().x1,
        70.0,
        "zmiana źródła widoczna w klonie"
    );
    // klon kopiuje także własną transformację źródła
    d.find_mut(src).unwrap().transform = Affine::translate((0., 5.)).as_coeffs();
    assert_eq!(d.bbox_of(cl).unwrap().unwrap().y0, 5.0);
    assert!(d.validate().is_ok());
}

#[test]
fn clone_of_nested_source_uses_full_world_transform() {
    let mut d = Document::new("t");
    let child = Object::new(d.alloc_id(), 0, rect(0., 0., 1., 1.));
    let src = child.id;
    let gid = d.alloc_id();
    let mut g = Object::new(
        gid,
        0,
        Kind::Group {
            children: vec![child],
        },
    );
    g.transform = Affine::translate((100., 0.)).as_coeffs();
    d.objects.push(g);
    let cl = d.alloc_id();
    d.objects
        .push(Object::new(cl, 0, Kind::Clone { source: src }));
    assert_eq!(
        d.bbox_of(cl).unwrap().unwrap().x0,
        100.0,
        "klon w tożsamości pokrywa źródło"
    );
}

#[test]
fn clone_cycles_and_dangling_are_invalid() {
    let mut d = Document::new("t");
    let a = d.alloc_id();
    let b = d.alloc_id();
    d.objects.push(Object::new(a, 0, Kind::Clone { source: b }));
    d.objects.push(Object::new(b, 0, Kind::Clone { source: a }));
    assert!(
        d.validate().is_err(),
        "cykl klonów: błąd, nie przepełnienie stosu"
    );
    let mut e = Document::new("t");
    let x = e.alloc_id();
    e.objects
        .push(Object::new(x, 0, Kind::Clone { source: 999 }));
    assert!(e.validate().is_err());
}

#[test]
fn req_02_020_deleting_source_detaches_clones() {
    let mut d = Document::new("t");
    let src = d.add(0, rect(0., 0., 10., 10.)).unwrap();
    let cl = d.alloc_id();
    let mut c = Object::new(cl, 0, Kind::Clone { source: src });
    c.transform = Affine::translate((30., 0.)).as_coeffs();
    d.objects.push(c);
    d.remove(src).unwrap();
    let o = d.find(cl).unwrap();
    assert_eq!(
        o.kind.type_name(),
        "rect",
        "klon stał się samodzielną kopią"
    );
    assert_eq!(
        d.bbox_of(cl).unwrap().unwrap().x0,
        30.0,
        "geometria bez zmian"
    );
    assert!(d.validate().is_ok());
}

#[test]
fn req_02_021_helper_object_roundtrip() {
    let mut d = Document::new("t");
    let id = d
        .add(
            bc_doc_t2(),
            Kind::Path {
                d: "M0 0 L100 0".into(),
            },
        )
        .unwrap();
    d.find_mut(id).unwrap().helper = true;
    let back = from_bytes(&to_bytes(&d, &Extras::default()).unwrap())
        .unwrap()
        .0;
    assert!(back.find(id).unwrap().helper);
    assert!(
        serde_json::to_value(Object::new(1, 0, rect(0., 0., 1., 1.)))
            .unwrap()
            .get("helper")
            .is_none()
    );
}

fn bc_doc_t2() -> u8 {
    T2_INDEX
}

#[test]
fn containers_and_parents() {
    let mut d = Document::new("t");
    let a = Object::new(d.alloc_id(), 0, rect(0., 0., 1., 1.));
    let aid = a.id;
    let gid = d.alloc_id();
    d.objects
        .push(Object::new(gid, 0, Kind::Group { children: vec![a] }));
    assert_eq!(d.parent_of(aid), Some(Some(gid)));
    assert_eq!(d.parent_of(gid), Some(None));
    assert_eq!(d.parent_of(999), None);
    assert_eq!(d.container(Some(gid)).unwrap().len(), 1);
    assert!(
        d.container(Some(aid)).is_none(),
        "prostokąt nie jest kontenerem"
    );
    d.container_mut(Some(gid)).unwrap().clear();
    assert!(d.find(aid).is_none());
}

#[test]
fn req_02_052_migration_to_schema_3_and_new_kinds_validate() {
    let v2 = r##"{"schema":2,"name":"m1","units":"mm","workspace":{"width":300,"height":200},
        "layers":[{"index":0,"role":"cut","color":"#000000","name":"C00","mode":"line","speed":10,"power_max":50,
        "power_min":10,"passes":1,"output":true,"show":true,"air_assist":false}],
        "objects":[{"id":1,"type":"rect","layer":0,"visible":true,"locked":false,
        "transform":[1,0,0,1,0,0],"x":0,"y":0,"w":5,"h":5,"radius":0}],"next_id":2}"##;
    let d = document_from_json(v2).unwrap();
    assert_eq!(d.schema, SCHEMA_VERSION);
    let (mut a, _, cid) = array_doc(grid(2, 2, 10., 10.));
    let cl = a.alloc_id();
    a.objects
        .push(Object::new(cl, 0, Kind::Clone { source: cid }));
    let v = serde_json::to_value(&a).unwrap();
    assert!(
        crate::bcraft::schema_errors(&v).is_empty(),
        "{:?}",
        crate::bcraft::schema_errors(&v)
    );
    let back = from_bytes(&to_bytes(&a, &Extras::default()).unwrap())
        .unwrap()
        .0;
    assert_eq!(back, a);
}

#[test]
fn validate_rejects_huge_coordinates() {
    let mut d = Document::new("t");
    d.add(0, rect(0., 0., 1e60, 1.)).unwrap();
    assert!(
        d.validate().is_err(),
        "dokument z geometrią poza zakresem jest odrzucany przy odczycie"
    );
}

#[test]
fn req_02_052_migration_schema_3_to_4_layer_params_and_tabs() {
    let v3 = r##"{"schema":3,"name":"m2","units":"mm","workspace":{"width":300,"height":200},
        "layers":[{"index":0,"role":"cut","color":"#000000","name":"C00","mode":"line","speed":10,"power_max":50,
        "power_min":10,"passes":1,"output":true,"show":true,"air_assist":false}],
        "objects":[{"id":1,"type":"rect","layer":0,"visible":true,"locked":false,
        "transform":[1,0,0,1,0,0],"x":0,"y":0,"w":5,"h":5,"radius":0}],"next_id":2}"##;
    let d = document_from_json(v3).unwrap();
    assert_eq!(
        d.schema,
        crate::SCHEMA_VERSION,
        "migracja 3 -> 4 -> bieżąca"
    );
    assert_eq!(d.layers[0].params, crate::LayerParams::default());
    assert_eq!(d.plan, crate::PlanSettings::default());
    // parametry i mostki przechodzą przez zapis i odczyt
    let mut a = d.clone();
    a.layers[0].params.kerf = 0.15;
    a.layers[0].params.tabs.enabled = true;
    a.plan.cut_last = true;
    a.objects[0].tabs = vec![crate::Tab {
        at: [2.5, 0.0],
        size: None,
    }];
    let v = serde_json::to_value(&a).unwrap();
    assert!(
        crate::bcraft::schema_errors(&v).is_empty(),
        "{:?}",
        crate::bcraft::schema_errors(&v)
    );
    let back = from_bytes(&to_bytes(&a, &Extras::default()).unwrap())
        .unwrap()
        .0;
    assert_eq!(back, a);
    // niepoprawny parametr jest odrzucany przy odczycie
    let mut bad = a.clone();
    bad.layers[0].params.interval = 0.0;
    assert!(bad.validate().is_err());
}
