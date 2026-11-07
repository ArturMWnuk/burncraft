//! Próbki `.lbrn`/`.lbrn2` od użytkownika (AppVersion 2.0.05): .lbrn2 (FormatVersion 1)
//! i .lbrn (FormatVersion 0) — ADR 0008, clean-room.

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

macro_rules! sample {
    ($n:literal) => {
        (
            &include_bytes!(concat!("fixtures/", $n, ".lbrn2"))[..],
            &include_bytes!(concat!("fixtures/", $n, ".lbrn"))[..],
        )
    };
}

fn doc2(data: &[u8]) -> Document {
    lbrn::lbrn2_document(data, "t").unwrap()
}

fn doc_lbrn(data: &[u8]) -> Document {
    lbrn::lbrn_document(data, "t").unwrap()
}

fn only(d: &Document) -> (&bc_doc::Object, Rect) {
    assert_eq!(
        d.objects.len(),
        1,
        "{:?}",
        d.objects
            .iter()
            .map(|o| o.kind.type_name())
            .collect::<Vec<_>>()
    );
    let o = &d.objects[0];
    (o, d.bbox_of(o.id).unwrap().unwrap())
}

#[test]
fn req_02_092_ellipse() {
    let (l2, _) = sample!("elipsa");
    let d = doc2(l2);
    let (o, b) = only(&d);
    assert!(
        matches!(o.kind, Kind::Ellipse { rx, ry, .. } if rx == 87.5 && ry == 29.5),
        "elipsa zostaje parametryczna"
    );
    assert!(rect_eq(b, [40., 105., 215., 164.], 1e-3), "{b:?}");
}

#[test]
fn req_02_092_polygon_ellipse_inscribed_and_mirrored() {
    let (l2, _) = sample!("hex");
    let d = doc2(l2);
    let (_, b) = only(&d);
    // N=6, Rx=50,5, Ry=41: wierzchołki na kątach 0°, 60°… (lewy i prawy wierzchołek, płaska góra)
    let h = 41.0 * (60f64).to_radians().sin();
    assert!(rect_eq(b, [107., 86. - h, 208., 86. + h], 1e-3), "{b:?}");
    let p = &d.paths_of(d.objects[0].id).unwrap()[0].2;
    assert!(bc_geom::is_closed(p));
    assert_eq!(bc_geom::node_count(p), 6);
}

#[test]
fn req_02_092_regular_polygon_stays_parametric() {
    let (l2, _) = sample!("grupa");
    let d = doc2(l2);
    let g = &d.objects[0];
    let poly = g
        .children()
        .iter()
        .find(|c| c.kind.type_name() == "polygon")
        .expect("Rx = Ry -> wielokąt foremny");
    assert!(matches!(poly.kind, Kind::Polygon { sides: 6, r, .. } if r == 12.0));
}

#[test]
fn req_02_092_path_with_beziers_and_y_up() {
    let (l2, _) = sample!("krzywe");
    let d = doc2(l2);
    let (o, b) = only(&d);
    let p = &d.paths_of(o.id).unwrap()[0].2;
    assert!(bc_geom::is_closed(p));
    let curves = p
        .elements()
        .iter()
        .filter(|e| matches!(e, bc_geom::PathEl::CurveTo(..)))
        .count();
    assert_eq!(curves, 2, "B0 1 i B6 0");
    // oś Y w górę: wierzchołek (85, 67) jest najniżej, (41, 169) najwyżej
    assert!(close(b.y0, 67.0, 1e-9) && close(b.y1, 169.0, 1e-6), "{b:?}");
    assert!(close(b.x1, 143.0, 1e-9));
    assert!(
        b.x0 < 41.0 && b.x0 > 20.0,
        "łuk lewej krzywej wychodzi poza wierzchołki: {}",
        b.x0
    );
}

#[test]
fn req_02_092_text_imported_as_curves_with_warning() {
    let (l2, _) = sample!("text");
    let imp = lbrn::import_lbrn2(l2, &ImportOptions::default()).unwrap();
    assert!(
        imp.warnings.iter().any(|w| w.contains("Lorem ipsum")),
        "{:?}",
        imp.warnings
    );
    let d = doc2(l2);
    let (o, b) = only(&d);
    assert_eq!(o.kind.type_name(), "path");
    assert!(
        bc_geom::subpath_count(&d.paths_of(o.id).unwrap()[0].2) > 10,
        "kontury liter"
    );
    // napis wysokości 25 mm (H="25") — kontur ma rozsądną wysokość i leży wokół XForm
    assert!(
        b.height() > 15.0 && b.height() < 30.0 && b.center().x > 60.0 && b.center().x < 130.0,
        "{b:?}"
    );
}

#[test]
fn req_02_092_group_with_children_and_world_backup_path() {
    let (l2, _) = sample!("grupa");
    let d = doc2(l2);
    let (g, _) = only(&d);
    assert_eq!(g.kind.type_name(), "group");
    let kids = g.children();
    assert_eq!(kids.len(), 5);
    let kinds: Vec<&str> = kids.iter().map(|c| c.kind.type_name()).collect();
    assert_eq!(kinds, vec!["path", "polygon", "path", "ellipse", "ellipse"]);
    // dzieci grupy: XForm względny wobec grupy (39,812897; -16,509392)
    let e = d.bbox_of(kids[3].id).unwrap().unwrap();
    assert!(
        close(e.center().x, 150.0 + 39.812897, 1e-6) && close(e.center().y, 75.5 - 16.509392, 1e-6),
        "{e:?}"
    );
    // tekst w grupie: BackupPath ma XForm bezwzględny — w świecie to ten sam napis co w text.lbrn2, przesunięty o grupę
    let (t2, _) = sample!("text");
    let td = doc2(t2);
    let tb = td.bbox().unwrap().unwrap();
    let gb = d.bbox_of(kids[0].id).unwrap().unwrap();
    assert!(
        close(gb.x0, tb.x0 + 39.812897, 1e-3) && close(gb.y0, tb.y0 - 16.509392, 1e-3),
        "{gb:?} vs {tb:?}"
    );
    assert!(d.validate().is_ok());
}

#[test]
fn req_02_091_lbrn_format0_matches_lbrn2() {
    for (name, (l2, l)) in [
        ("elipsa", sample!("elipsa")),
        ("hex", sample!("hex")),
        ("krzywe", sample!("krzywe")),
        ("text", sample!("text")),
        ("grupa", sample!("grupa")),
    ] {
        let a = doc2(l2);
        let b = doc_lbrn(l);
        assert_eq!(a.objects.len(), b.objects.len(), "{name}");
        let (ba, bb) = (a.bbox().unwrap().unwrap(), b.bbox().unwrap().unwrap());
        assert!(
            rect_eq(ba, [bb.x0, bb.y0, bb.x1, bb.y1], 1e-6),
            "{name}: {ba:?} vs {bb:?}"
        );
        let na: usize = a
            .world_paths()
            .unwrap()
            .iter()
            .map(|(_, _, p)| bc_geom::node_count(p))
            .sum();
        let nb: usize = b
            .world_paths()
            .unwrap()
            .iter()
            .map(|(_, _, p)| bc_geom::node_count(p))
            .sum();
        assert_eq!(na, nb, "{name}: liczba węzłów");
    }
}

#[test]
fn lbrn_import_file_by_extension() {
    let dir = std::env::temp_dir().join(format!("bc-lbrn-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let p = dir.join("hex.lbrn");
    std::fs::write(&p, sample!("hex").1).unwrap();
    assert_eq!(
        import_file(&p, &ImportOptions::default())
            .unwrap()
            .items
            .len(),
        1
    );
    let _ = std::fs::remove_dir_all(&dir);
}
