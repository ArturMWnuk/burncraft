//! Testy poleceń rdzenia M1 domykających etap: okręgi styczne, nakładające się
//! odcinki, wygładzanie, łuki z odcinków, asystent booleanów, kopiowanie wzdłuż
//! ścieżki, zmiana rozmiaru wpustów i czopów, dokowanie, Quick Nest.
//! Wszystko przez rejestr, tak jak wywołuje to GUI i MCP.

use super::*;
use crate::inspect::{Detail, ObjectReport};
use bc_geom::{ops, BezPath, ParamCurve, Point};
use serde_json::{json, Value};

fn setup() -> (Registry, Session) {
    (Registry::builtin(), Session::default())
}

fn run(r: &Registry, s: &mut Session, id: &str, p: Value) -> Value {
    s.execute(r, id, p.clone())
        .unwrap_or_else(|e| panic!("{id} {p}: {e}"))
}

fn err(r: &Registry, s: &mut Session, id: &str, p: Value) -> CmdError {
    match s.execute(r, id, p.clone()) {
        Ok(v) => panic!("{id} {p}: oczekiwano błędu, wynik {v}"),
        Err(e) => e,
    }
}

fn id_of(v: &Value) -> u64 {
    v["id"].as_u64().unwrap()
}

fn ids_of(v: &Value) -> Vec<u64> {
    v["ids"]
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x.as_u64().unwrap())
        .collect()
}

fn rect(r: &Registry, s: &mut Session, x: f64, y: f64, w: f64, h: f64) -> u64 {
    id_of(&run(
        r,
        s,
        "shape.rect",
        json!({"x": x, "y": y, "w": w, "h": h}),
    ))
}

fn circle(r: &Registry, s: &mut Session, cx: f64, cy: f64, rad: f64) -> u64 {
    id_of(&run(
        r,
        s,
        "shape.ellipse",
        json!({"cx": cx, "cy": cy, "rx": rad, "ry": rad}),
    ))
}

fn path(r: &Registry, s: &mut Session, pts: &[[f64; 2]], closed: bool) -> u64 {
    id_of(&run(
        r,
        s,
        "shape.path",
        json!({"points": pts, "closed": closed}),
    ))
}

fn d_path(r: &Registry, s: &mut Session, d: &str) -> u64 {
    id_of(&run(r, s, "shape.path", json!({ "d": d })))
}

fn report(s: &Session, id: u64) -> ObjectReport {
    inspect::report(s, Detail::Summary)
        .unwrap()
        .objects
        .into_iter()
        .find(|o| o.id == id)
        .unwrap_or_else(|| panic!("brak obiektu {id}"))
}

fn bbox(s: &Session, id: u64) -> [f64; 4] {
    report(s, id).bbox.unwrap()
}

fn center(s: &Session, id: u64) -> Point {
    let b = bbox(s, id);
    Point::new((b[0] + b[2]) / 2.0, (b[1] + b[3]) / 2.0)
}

fn world(s: &Session, id: u64) -> BezPath {
    let mut out = Vec::new();
    let pa = s.doc.parent_affine(id).unwrap();
    s.doc
        .find(id)
        .unwrap()
        .world_paths(&s.doc, pa, &mut out)
        .unwrap();
    let mut p = BezPath::new();
    for (_, _, q) in out {
        for el in q.elements() {
            p.push(*el);
        }
    }
    p
}

fn area(s: &Session, id: u64) -> f64 {
    ops::area(&[world(s, id)])
}

fn close(a: f64, b: f64, eps: f64) -> bool {
    (a - b).abs() <= eps
}

fn has_curves(p: &BezPath) -> bool {
    p.elements()
        .iter()
        .any(|e| matches!(e, bc_geom::PathEl::CurveTo(..)))
}

/// Największe odchylenie punktów ścieżki (gęsto próbkowanej) od okręgu.
fn max_radial_error(p: &BezPath, c: Point, rad: f64) -> f64 {
    let mut worst: f64 = 0.0;
    for seg in p.segments() {
        for i in 0..=40 {
            let q = seg.eval(i as f64 / 40.0);
            worst = worst.max((q.distance(c) - rad).abs());
        }
    }
    worst
}

fn regular(n: usize, c: [f64; 2], rad: f64) -> Vec<[f64; 2]> {
    (0..n)
        .map(|i| {
            let a = std::f64::consts::TAU * i as f64 / n as f64;
            [c[0] + rad * a.cos(), c[1] + rad * a.sin()]
        })
        .collect()
}

fn requirement_tagged(r: &Registry, cmd: &str, req: &str) {
    let spec = &r
        .get(cmd)
        .unwrap_or_else(|| panic!("brak polecenia {cmd}"))
        .spec;
    assert!(
        spec.requirements.contains(&req),
        "{cmd} powinno deklarować {req}"
    );
}

// ---------- REQ-04-019: okręgi styczne do 3 okręgów ----------

fn tangent_ok(c: (f64, f64, f64), given: (f64, f64, f64)) -> bool {
    let d = ((c.0 - given.0).powi(2) + (c.1 - given.1).powi(2)).sqrt();
    close(d, c.2 + given.2, 1e-6) || close(d, (c.2 - given.2).abs(), 1e-6)
}

fn circles_of(v: &Value) -> Vec<(f64, f64, f64)> {
    v["circles"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| {
            (
                c["cx"].as_f64().unwrap(),
                c["cy"].as_f64().unwrap(),
                c["r"].as_f64().unwrap(),
            )
        })
        .collect()
}

#[test]
fn req_04_019_tangent_circles_all_solutions() {
    let (r, mut s) = setup();
    requirement_tagged(&r, "shape.tangent_circles", "REQ-04-019");
    let given = [
        (100.0, 100.0, 10.0),
        (160.0, 100.0, 10.0),
        (130.0, 150.0, 10.0),
    ];
    let ids: Vec<u64> = given
        .iter()
        .map(|c| circle(&r, &mut s, c.0, c.1, c.2))
        .collect();
    let res = run(&r, &mut s, "shape.tangent_circles", json!({ "ids": ids }));
    let new = ids_of(&res);
    let circles = circles_of(&res);
    // trzy rozłączne okręgi niewspółliniowe: 8 rozwiązań problemu Apoloniusza
    assert_eq!(new.len(), 8, "{res}");
    assert_eq!(circles.len(), 8);
    for (id, c) in new.iter().zip(&circles) {
        for g in &given {
            assert!(tangent_ok(*c, *g), "okrąg {c:?} nie jest styczny do {g:?}");
        }
        // obiekt w dokumencie to okrąg o podanych parametrach
        let b = bbox(&s, *id);
        assert!(close(b[2] - b[0], 2.0 * c.2, 1e-3) && close(b[3] - b[1], 2.0 * c.2, 1e-3));
        assert!(close((b[0] + b[2]) / 2.0, c.0, 1e-6));
        assert_eq!(report(&s, *id).kind, "ellipse");
    }
    assert_eq!(s.selection, new);
    // jedno wywołanie = jeden krok historii
    run(&r, &mut s, "edit.undo", json!({}));
    assert!(new.iter().all(|id| s.doc.find(*id).is_none()));
}

#[test]
fn req_04_019_tangent_circles_filter_and_explicit_circles() {
    let (r, mut s) = setup();
    let given = [
        [100.0, 100.0, 10.0],
        [160.0, 100.0, 10.0],
        [130.0, 150.0, 10.0],
    ];
    let outer = run(
        &r,
        &mut s,
        "shape.tangent_circles",
        json!({"circles": given, "filter": "outer"}),
    );
    let c = circles_of(&outer);
    assert_eq!(c.len(), 1, "{outer}");
    for g in &given {
        let d = ((c[0].0 - g[0]).powi(2) + (c[0].1 - g[1]).powi(2)).sqrt();
        assert!(close(d, c[0].2 + g[2], 1e-6), "styczność zewnętrzna");
    }
    assert!(outer["circles"][0]["tangency"]
        .as_array()
        .unwrap()
        .iter()
        .all(|t| t == "external"));
    let inner = run(
        &r,
        &mut s,
        "shape.tangent_circles",
        json!({"circles": given, "filter": "inner"}),
    );
    let c = circles_of(&inner);
    assert_eq!(c.len(), 1);
    for g in &given {
        let d = ((c[0].0 - g[0]).powi(2) + (c[0].1 - g[1]).powi(2)).sqrt();
        assert!(close(d, c[0].2 - g[2], 1e-6), "okrąg obejmujący");
    }
    // środki współliniowe: rozwiązania skończone nadal są znajdowane
    let col = [
        [100.0, 200.0, 1.0],
        [110.0, 200.0, 2.0],
        [120.0, 200.0, 1.0],
    ];
    let res = run(
        &r,
        &mut s,
        "shape.tangent_circles",
        json!({ "circles": col }),
    );
    let c = circles_of(&res);
    assert!(!c.is_empty());
    for x in &c {
        for g in &col {
            assert!(tangent_ok(*x, (g[0], g[1], g[2])));
        }
    }
    assert!(
        c.iter().any(|x| close(x.2, 51.5, 1e-6)),
        "okrąg obejmujący R = 51,5"
    );
}

#[test]
fn req_04_019_tangent_circles_rejects_non_circles() {
    let (r, mut s) = setup();
    let a = circle(&r, &mut s, 10., 10., 5.);
    let b = circle(&r, &mut s, 50., 10., 5.);
    let c = rect(&r, &mut s, 20., 40., 10., 10.);
    assert_eq!(
        err(
            &r,
            &mut s,
            "shape.tangent_circles",
            json!({"ids": [a, b, c]})
        )
        .code,
        "invalid_params"
    );
    let e = circle(&r, &mut s, 80., 80., 5.);
    assert_eq!(
        err(&r, &mut s, "shape.tangent_circles", json!({"ids": [a, b]})).code,
        "invalid_params"
    );
    // okrąg przeskalowany równomiernie nadal jest okręgiem
    run(
        &r,
        &mut s,
        "object.transform",
        json!({"ids": [e], "scale": [2, 2]}),
    );
    run(
        &r,
        &mut s,
        "shape.tangent_circles",
        json!({"ids": [a, b, e]}),
    );
    // elipsa nie
    run(
        &r,
        &mut s,
        "object.transform",
        json!({"ids": [e], "scale": [2, 1]}),
    );
    assert_eq!(
        err(
            &r,
            &mut s,
            "shape.tangent_circles",
            json!({"ids": [a, b, e]})
        )
        .code,
        "invalid_params"
    );
}

// ---------- REQ-04-040: usuwanie nakładających się odcinków ----------

#[test]
fn req_04_040_remove_overlaps_shared_edge() {
    let (r, mut s) = setup();
    requirement_tagged(&r, "path.remove_overlaps", "REQ-04-040");
    let a = rect(&r, &mut s, 0., 0., 10., 10.);
    let b = rect(&r, &mut s, 10., 0., 10., 10.);
    let res = run(&r, &mut s, "path.remove_overlaps", json!({"ids": [a, b]}));
    assert!(
        close(res["removed_length"].as_f64().unwrap(), 10.0, 1e-6),
        "{res}"
    );
    assert!(close(report(&s, a).length, 40.0, 1e-6));
    assert!(report(&s, a).closed, "pierwszy kształt nienaruszony");
    let rb = report(&s, b);
    assert!(close(rb.length, 30.0, 1e-6), "{}", rb.length);
    assert!(!rb.closed, "wspólna krawędź usunięta z drugiego");
    // drugi zachowuje pozostałe trzy boki jako jedną ciągłą ścieżkę
    assert_eq!(bc_geom::subpath_count(&world(&s, b)), 1);
    run(&r, &mut s, "edit.undo", json!({}));
    assert!(close(report(&s, b).length, 40.0, 1e-6));
}

#[test]
fn req_04_040_remove_overlaps_partial_and_full() {
    let (r, mut s) = setup();
    let a = path(&r, &mut s, &[[0., 0.], [10., 0.]], false);
    let b = path(&r, &mut s, &[[5., 0.], [20., 0.]], false);
    let c = path(&r, &mut s, &[[2., 0.], [8., 0.]], false);
    let d = path(&r, &mut s, &[[0., 5.], [10., 5.], [0., 5.]], false);
    let res = run(
        &r,
        &mut s,
        "path.remove_overlaps",
        json!({"ids": [a, b, c, d]}),
    );
    assert!(close(report(&s, a).length, 10.0, 1e-9));
    let bb = bbox(&s, b);
    assert!(close(report(&s, b).length, 10.0, 1e-6) && close(bb[0], 10.0, 1e-6));
    assert!(
        s.doc.find(c).is_none(),
        "odcinek całkowicie pokryty usunięty"
    );
    assert_eq!(res["removed"], json!([c]));
    assert!(
        close(report(&s, d).length, 10.0, 1e-6),
        "nakładanie w obrębie jednej ścieżki"
    );
    assert!(close(
        res["removed_length"].as_f64().unwrap(),
        5.0 + 6.0 + 10.0,
        1e-6
    ));
    // odcinki równoległe, ale nie współliniowe, zostają
    let e = path(&r, &mut s, &[[0., 0.5], [10., 0.5]], false);
    let res = run(&r, &mut s, "path.remove_overlaps", json!({"ids": [a, e]}));
    assert!(close(res["removed_length"].as_f64().unwrap(), 0.0, 1e-12));
    assert!(close(report(&s, e).length, 10.0, 1e-9));
}

// ---------- REQ-04-043: wygładzanie ----------

#[test]
fn req_04_043_smooth_polygon_into_curves() {
    let (r, mut s) = setup();
    requirement_tagged(&r, "path.smooth", "REQ-04-043");
    let pts = regular(12, [100., 100.], 20.);
    let a = path(&r, &mut s, &pts, true);
    let before = max_radial_error(&world(&s, a), Point::new(100., 100.), 20.);
    assert!(before > 0.6, "wielokąt odstaje od okręgu o {before}");
    run(&r, &mut s, "path.smooth", json!({"ids": [a]}));
    let p = world(&s, a);
    assert!(has_curves(&p), "odcinki zamienione na krzywe");
    assert_eq!(
        bc_geom::nodes::count(&bc_geom::nodes::contours(&p)),
        12,
        "węzły zachowane"
    );
    assert!(report(&s, a).closed);
    let after = max_radial_error(&p, Point::new(100., 100.), 20.);
    assert!(after < 0.1, "wygładzony kształt bliski okręgowi: {after}");
    // krzywe przechodzą przez pierwotne węzły
    for q in &pts {
        let hit = p
            .segments()
            .any(|seg| seg.start().distance(Point::new(q[0], q[1])) < 1e-9);
        assert!(hit, "węzeł {q:?} zachowany");
    }
}

#[test]
fn req_04_043_smooth_keeps_sharp_corners() {
    let (r, mut s) = setup();
    let sq = path(
        &r,
        &mut s,
        &[[0., 0.], [10., 0.], [10., 10.], [0., 10.]],
        true,
    );
    run(&r, &mut s, "path.smooth", json!({"ids": [sq]}));
    let p = world(&s, sq);
    assert!(
        close(ops::area(std::slice::from_ref(&p)), 100.0, 1e-9),
        "narożniki 90° > progu 60° zostają"
    );
    // z progiem 100° kwadrat też jest wygładzany
    run(
        &r,
        &mut s,
        "path.smooth",
        json!({"ids": [sq], "corner_angle": 100}),
    );
    assert!(has_curves(&world(&s, sq)));
    assert_eq!(
        err(
            &r,
            &mut s,
            "path.smooth",
            json!({"ids": [sq], "corner_angle": 200})
        )
        .code,
        "invalid_params"
    );
    assert_eq!(
        err(
            &r,
            &mut s,
            "path.smooth",
            json!({"ids": [sq], "smoothness": 1.5})
        )
        .code,
        "invalid_params"
    );
}

// ---------- REQ-04-044: łuki z odcinków ----------

#[test]
fn req_04_044_lines_to_arcs_circle() {
    let (r, mut s) = setup();
    requirement_tagged(&r, "path.lines_to_arcs", "REQ-04-044");
    let a = path(&r, &mut s, &regular(72, [100., 100.], 20.), true);
    assert_eq!(report(&s, a).nodes, 72);
    let res = run(
        &r,
        &mut s,
        "path.lines_to_arcs",
        json!({"ids": [a], "tolerance": 0.05}),
    );
    assert!(res["arcs"].as_u64().unwrap() >= 1, "{res}");
    let p = world(&s, a);
    assert!(has_curves(&p));
    assert!(report(&s, a).closed);
    assert!(
        report(&s, a).nodes <= 12,
        "mniej węzłów: {}",
        report(&s, a).nodes
    );
    let e = max_radial_error(&p, Point::new(100., 100.), 20.);
    assert!(e < 0.01, "łuki leżą na okręgu (odchylenie {e})");
}

#[test]
fn req_04_044_lines_to_arcs_mixed_and_straight() {
    let (r, mut s) = setup();
    // półkole: średnica (odcinek) + 36 odcinków na łuku
    let mut pts = vec![[80.0, 100.0]];
    for i in 0..=36 {
        let t = std::f64::consts::PI * i as f64 / 36.0;
        pts.push([100.0 + 20.0 * t.cos(), 100.0 + 20.0 * t.sin()]);
    }
    let half = path(&r, &mut s, &pts, true);
    let res = run(&r, &mut s, "path.lines_to_arcs", json!({"ids": [half]}));
    assert!(res["arcs"].as_u64().unwrap() >= 1);
    let p = world(&s, half);
    assert!(
        p.elements()
            .iter()
            .any(|e| matches!(e, bc_geom::PathEl::LineTo(_))),
        "średnica zostaje odcinkiem"
    );
    assert!(close(ops::area(&[p]), std::f64::consts::PI * 200.0, 2.0));
    // prostokąt nie ma łuków
    let sq = path(
        &r,
        &mut s,
        &[[0., 0.], [10., 0.], [10., 10.], [0., 10.]],
        true,
    );
    let res = run(&r, &mut s, "path.lines_to_arcs", json!({"ids": [sq]}));
    assert_eq!(res["arcs"], json!(0));
    assert!(!has_curves(&world(&s, sq)));
    assert_eq!(
        err(
            &r,
            &mut s,
            "path.lines_to_arcs",
            json!({"ids": [sq], "tolerance": 0})
        )
        .code,
        "invalid_params"
    );
}

// ---------- REQ-04-069: asystent booleanów ----------

#[test]
fn req_04_069_boolean_assistant_previews_all_results() {
    let (r, mut s) = setup();
    requirement_tagged(&r, "path.boolean_preview", "REQ-04-069");
    let a = rect(&r, &mut s, 0., 0., 20., 20.);
    let b = rect(&r, &mut s, 10., 10., 20., 20.);
    let rev = s.revision;
    let res = run(&r, &mut s, "path.boolean_preview", json!({"ids": [a, b]}));
    assert_eq!(s.revision, rev, "podgląd nie zmienia dokumentu");
    assert_eq!(report(&s, a).kind, "rect");
    let opts = res["options"].as_array().unwrap();
    let find = |op: &str, first: u64| {
        opts.iter()
            .find(|o| o["op"] == op && o["ids"][0] == first)
            .unwrap_or_else(|| panic!("brak opcji {op} ({first}): {res}"))
            .clone()
    };
    let expect = [
        ("union", a, 700.0, 1),
        ("difference", a, 300.0, 1),
        ("difference", b, 300.0, 1),
        ("intersection", a, 100.0, 1),
        ("xor", a, 600.0, 2),
        ("cut", a, 700.0, 3),
    ];
    for (op, first, ar, pieces) in expect {
        let o = find(op, first);
        assert!(close(o["area"].as_f64().unwrap(), ar, 1e-6), "{op}: {o}");
        assert_eq!(o["pieces"], json!(pieces), "{op}: {o}");
        assert!(o["d"].as_str().unwrap().starts_with('M'));
        assert_eq!(o["apply"]["id"], "path.boolean");
    }
    // wybór opcji = wykonanie polecenia z pola apply
    let o = find("difference", b);
    let applied = run(&r, &mut s, "path.boolean", o["apply"]["params"].clone());
    let new = ids_of(&applied);
    assert_eq!(new.len(), 1);
    assert!(close(area(&s, new[0]), 300.0, 1e-6));
    assert_eq!(bbox(&s, new[0]), [10.0, 10.0, 30.0, 30.0]);
}

#[test]
fn req_04_069_boolean_assistant_errors() {
    let (r, mut s) = setup();
    let a = rect(&r, &mut s, 0., 0., 20., 20.);
    assert_eq!(
        err(&r, &mut s, "path.boolean_preview", json!({"ids": [a]})).code,
        "invalid_params"
    );
    let open = path(&r, &mut s, &[[0., 0.], [30., 30.]], false);
    assert_eq!(
        err(
            &r,
            &mut s,
            "path.boolean_preview",
            json!({"ids": [a, open]})
        )
        .code,
        "invalid_params"
    );
    // trzy obiekty: różnica dla każdego obiektu jako bazy
    let b = rect(&r, &mut s, 10., 0., 20., 20.);
    let c = rect(&r, &mut s, 20., 0., 20., 20.);
    let res = run(
        &r,
        &mut s,
        "path.boolean_preview",
        json!({"ids": [a, b, c]}),
    );
    let diffs = res["options"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|o| o["op"] == "difference")
        .count();
    assert_eq!(diffs, 3);
}

// ---------- REQ-04-078: kopiowanie wzdłuż ścieżki ----------

#[test]
fn req_04_078_copy_along_open_path() {
    let (r, mut s) = setup();
    requirement_tagged(&r, "array.along_path", "REQ-04-078");
    let guide = path(&r, &mut s, &[[0., 100.], [100., 100.]], false);
    let obj = rect(&r, &mut s, 200., 200., 10., 4.);
    let res = run(
        &r,
        &mut s,
        "array.along_path",
        json!({"ids": [obj], "path": guide, "count": 5}),
    );
    let new = ids_of(&res);
    assert_eq!(new.len(), 5);
    for (i, id) in new.iter().enumerate() {
        let c = center(&s, *id);
        assert!(
            close(c.x, 25.0 * i as f64, 1e-6) && close(c.y, 100.0, 1e-6),
            "{c:?}"
        );
        let b = bbox(&s, *id);
        assert!(close(b[2] - b[0], 10.0, 1e-6) && close(b[3] - b[1], 4.0, 1e-6));
    }
    assert!(s.doc.find(obj).is_some(), "oryginał zostaje");
    // odstęp zamiast liczby
    let res = run(
        &r,
        &mut s,
        "array.along_path",
        json!({"ids": [obj], "path": guide, "spacing": 30}),
    );
    let xs: Vec<f64> = ids_of(&res).iter().map(|id| center(&s, *id).x).collect();
    assert_eq!(xs.len(), 4);
    for (x, e) in xs.iter().zip([0.0, 30.0, 60.0, 90.0]) {
        assert!(close(*x, e, 1e-6), "{xs:?}");
    }
}

#[test]
fn req_04_078_copy_along_rotates_to_tangent() {
    let (r, mut s) = setup();
    let guide = path(&r, &mut s, &[[50., 0.], [50., 100.]], false);
    let obj = rect(&r, &mut s, 200., 200., 10., 4.);
    let res = run(
        &r,
        &mut s,
        "array.along_path",
        json!({"ids": [obj], "path": guide, "count": 3}),
    );
    for id in ids_of(&res) {
        let b = bbox(&s, id);
        assert!(
            close(b[2] - b[0], 4.0, 1e-6) && close(b[3] - b[1], 10.0, 1e-6),
            "obrót o 90°"
        );
    }
    let res = run(
        &r,
        &mut s,
        "array.along_path",
        json!({"ids": [obj], "path": guide, "count": 3, "rotate": false}),
    );
    for id in ids_of(&res) {
        let b = bbox(&s, id);
        assert!(close(b[2] - b[0], 10.0, 1e-6), "bez obrotu");
    }
    // ścieżka zamknięta: kopie równomiernie na okręgu, bez dublowania punktu startowego
    let ring = circle(&r, &mut s, 200., 200., 50.);
    let res = run(
        &r,
        &mut s,
        "array.along_path",
        json!({"ids": [obj], "path": ring, "count": 4}),
    );
    let cs: Vec<Point> = ids_of(&res).iter().map(|id| center(&s, *id)).collect();
    assert_eq!(cs.len(), 4);
    for (i, c) in cs.iter().enumerate() {
        assert!(close(c.distance(Point::new(200., 200.)), 50.0, 1e-3));
        let next = cs[(i + 1) % 4];
        assert!(close(c.distance(next), 50.0 * 2f64.sqrt(), 1e-2), "{cs:?}");
    }
    assert_eq!(
        err(
            &r,
            &mut s,
            "array.along_path",
            json!({"ids": [obj], "path": guide})
        )
        .code,
        "invalid_params"
    );
    assert_eq!(
        err(
            &r,
            &mut s,
            "array.along_path",
            json!({"ids": [obj], "path": obj, "count": 2})
        )
        .code,
        "invalid_params"
    );
}

// ---------- REQ-04-082: wpusty i czopy ----------

#[test]
fn req_04_082_resize_edge_slot_with_tolerance() {
    let (r, mut s) = setup();
    requirement_tagged(&r, "path.resize_slots", "REQ-04-082");
    let plate = path(
        &r,
        &mut s,
        &[
            [0., 0.],
            [100., 0.],
            [100., 50.],
            [51.5, 50.],
            [51.5, 30.],
            [48.5, 30.],
            [48.5, 50.],
            [0., 50.],
        ],
        true,
    );
    let res = run(
        &r,
        &mut s,
        "path.resize_slots",
        json!({"ids": [plate], "from": 3, "to": 4, "tolerance": 0.1}),
    );
    assert_eq!(
        (res["slots"].clone(), res["tabs"].clone()),
        (json!(1), json!(0)),
        "{res}"
    );
    assert!(close(area(&s, plate), 5000.0 - 4.1 * 20.0, 1e-6));
    assert_eq!(bbox(&s, plate), [0.0, 0.0, 100.0, 50.0]);
    let p = world(&s, plate);
    let xs: Vec<f64> = p
        .segments()
        .map(|seg| seg.start())
        .filter(|q| close(q.y, 30.0, 1e-9))
        .map(|q| q.x)
        .collect();
    assert_eq!(xs.len(), 2);
    assert!(
        xs.iter().any(|x| close(*x, 47.95, 1e-9)) && xs.iter().any(|x| close(*x, 52.05, 1e-9)),
        "wpust wyśrodkowany: {xs:?}"
    );
}

#[test]
fn req_04_082_resize_hole_and_tab() {
    let (r, mut s) = setup();
    let holed = d_path(
        &r,
        &mut s,
        "M0 0 L100 0 L100 50 L0 50 Z M48.5 15 L51.5 15 L51.5 35 L48.5 35 Z",
    );
    let res = run(
        &r,
        &mut s,
        "path.resize_slots",
        json!({"ids": [holed], "from": 3, "to": 4, "tolerance": 0.1}),
    );
    assert_eq!(res["slots"], json!(1), "{res}");
    assert!(close(area(&s, holed), 5000.0 - 4.1 * 20.0, 1e-6));
    let tab = path(
        &r,
        &mut s,
        &[
            [0., 0.],
            [100., 0.],
            [100., 50.],
            [60., 50.],
            [60., 53.],
            [40., 53.],
            [40., 50.],
            [0., 50.],
        ],
        true,
    );
    let res = run(
        &r,
        &mut s,
        "path.resize_slots",
        json!({"ids": [tab], "from": 3, "to": 4, "tolerance": 0.1}),
    );
    assert_eq!(
        (res["slots"].clone(), res["tabs"].clone()),
        (json!(0), json!(1)),
        "{res}"
    );
    assert!(
        close(area(&s, tab), 5000.0 + 20.0 * 4.0, 1e-6),
        "czop o długości nowej grubości"
    );
    assert!(close(bbox(&s, tab)[3], 54.0, 1e-9));
    // niepasujący wymiar: brak zmian
    let before = world(&s, tab);
    let res = run(
        &r,
        &mut s,
        "path.resize_slots",
        json!({"ids": [tab], "from": 5, "to": 6}),
    );
    assert_eq!(
        (res["slots"].clone(), res["tabs"].clone()),
        (json!(0), json!(0))
    );
    assert!(ops::same_geometry(&before, &world(&s, tab), 1e-9));
    assert_eq!(
        err(
            &r,
            &mut s,
            "path.resize_slots",
            json!({"ids": [tab], "from": 3, "to": -1})
        )
        .code,
        "invalid_params"
    );
}

// ---------- REQ-04-091: dokowanie ----------

#[test]
fn req_04_091_dock_against_objects_and_page() {
    let (r, mut s) = setup();
    requirement_tagged(&r, "object.dock", "REQ-04-091");
    let wall = rect(&r, &mut s, 0., 0., 20., 20.);
    let b = rect(&r, &mut s, 100., 5., 20., 10.);
    run(
        &r,
        &mut s,
        "object.dock",
        json!({"ids": [b], "direction": "left"}),
    );
    assert_eq!(bbox(&s, b), [20.0, 5.0, 40.0, 15.0]);
    assert_eq!(bbox(&s, wall), [0.0, 0.0, 20.0, 20.0], "przeszkoda stoi");
    // odstęp
    run(
        &r,
        &mut s,
        "object.transform",
        json!({"ids": [b], "translate": [50, 0]}),
    );
    run(
        &r,
        &mut s,
        "object.dock",
        json!({"ids": [b], "direction": "left", "gap": 2}),
    );
    assert!(close(bbox(&s, b)[0], 22.0, 1e-9));
    // brak przeszkody w pasie ruchu: do krawędzi obszaru roboczego
    let c = rect(&r, &mut s, 100., 50., 10., 10.);
    run(
        &r,
        &mut s,
        "object.dock",
        json!({"ids": [c], "direction": "left"}),
    );
    assert!(close(bbox(&s, c)[0], 0.0, 1e-9));
    run(
        &r,
        &mut s,
        "object.dock",
        json!({"ids": [c], "direction": "up"}),
    );
    assert!(
        close(bbox(&s, c)[3], 400.0, 1e-9),
        "górna krawędź obszaru (Y w górę)"
    );
    run(
        &r,
        &mut s,
        "object.dock",
        json!({"ids": [c], "direction": "down"}),
    );
    assert!(
        close(bbox(&s, c)[1], 20.0, 1e-9),
        "opiera się o przeszkodę pod spodem"
    );
}

#[test]
fn req_04_091_dock_stacks_selection() {
    let (r, mut s) = setup();
    rect(&r, &mut s, 0., 0., 20., 20.);
    let c = rect(&r, &mut s, 200., 5., 10., 10.);
    let b = rect(&r, &mut s, 100., 5., 20., 10.);
    run(
        &r,
        &mut s,
        "object.dock",
        json!({"ids": [c, b], "direction": "left"}),
    );
    assert!(close(bbox(&s, b)[0], 20.0, 1e-9));
    assert!(
        close(bbox(&s, c)[0], 40.0, 1e-9),
        "zaznaczone układają się jeden za drugim"
    );
    run(
        &r,
        &mut s,
        "object.set",
        json!({"ids": [c], "locked": true}),
    );
    assert_eq!(
        err(
            &r,
            &mut s,
            "object.dock",
            json!({"ids": [c], "direction": "right"})
        )
        .code,
        "locked"
    );
}

// ---------- REQ-04-095: Quick Nest ----------

fn no_overlap(s: &Session, ids: &[u64], gap: f64) {
    for (i, a) in ids.iter().enumerate() {
        for b in &ids[i + 1..] {
            let (p, q) = (bbox(s, *a), bbox(s, *b));
            let sep_x = (q[0] - p[2]).max(p[0] - q[2]);
            let sep_y = (q[1] - p[3]).max(p[1] - q[3]);
            assert!(
                sep_x.max(sep_y) >= gap - 1e-9,
                "{a} i {b} nachodzą: {p:?} {q:?}"
            );
        }
    }
}

#[test]
fn req_04_095_quick_nest_packs_into_sheet() {
    let (r, mut s) = setup();
    requirement_tagged(&r, "object.quick_nest", "REQ-04-095");
    let ids: Vec<u64> = [(300., 10.), (150., 300.), (20., 200.), (250., 250.)]
        .iter()
        .map(|(x, y)| rect(&r, &mut s, *x, *y, 50., 50.))
        .collect();
    let res = run(
        &r,
        &mut s,
        "object.quick_nest",
        json!({"ids": ids, "sheet": {"x": 0, "y": 0, "w": 100, "h": 100}}),
    );
    assert_eq!(ids_of(&res).len(), 4, "{res}");
    assert_eq!(res["unplaced"], json!([]));
    assert!(close(res["utilization"].as_f64().unwrap(), 1.0, 1e-9));
    no_overlap(&s, &ids, 0.0);
    for id in &ids {
        let b = bbox(&s, *id);
        assert!(b[0] >= -1e-9 && b[1] >= -1e-9 && b[2] <= 100.0 + 1e-9 && b[3] <= 100.0 + 1e-9);
    }
    // deterministycznie: drugie wywołanie nic nie zmienia
    let snap: Vec<[f64; 4]> = ids.iter().map(|id| bbox(&s, *id)).collect();
    run(
        &r,
        &mut s,
        "object.quick_nest",
        json!({"ids": ids, "sheet": {"x": 0, "y": 0, "w": 100, "h": 100}}),
    );
    assert_eq!(snap, ids.iter().map(|id| bbox(&s, *id)).collect::<Vec<_>>());
}

#[test]
fn req_04_095_quick_nest_spacing_rotation_and_default_sheet() {
    let (r, mut s) = setup();
    let ids: Vec<u64> = (0..4)
        .map(|i| rect(&r, &mut s, 300. + i as f64, 300., 50., 50.))
        .collect();
    let res = run(
        &r,
        &mut s,
        "object.quick_nest",
        json!({"ids": ids, "sheet": {"x": 0, "y": 0, "w": 100, "h": 100}, "spacing": 2}),
    );
    assert_eq!(ids_of(&res).len(), 1, "50 + 2 + 50 > 100: {res}");
    assert_eq!(res["unplaced"].as_array().unwrap().len(), 3);
    // obrót o 90°, gdy kształt mieści się tylko obrócony
    let tall = rect(&r, &mut s, 300., 0., 20., 90.);
    let sheet = json!({"x": 0, "y": 0, "w": 100, "h": 30});
    let res = run(
        &r,
        &mut s,
        "object.quick_nest",
        json!({"ids": [tall], "sheet": sheet, "rotate": false}),
    );
    assert_eq!(res["unplaced"], json!([tall]));
    run(
        &r,
        &mut s,
        "object.quick_nest",
        json!({"ids": [tall], "sheet": sheet}),
    );
    let b = bbox(&s, tall);
    assert!(
        close(b[2] - b[0], 90.0, 1e-6) && close(b[3] - b[1], 20.0, 1e-6),
        "{b:?}"
    );
    // domyślny arkusz = obszar roboczy, odstępy zachowane
    let many: Vec<u64> = (0..12)
        .map(|i| rect(&r, &mut s, 500. + 7.0 * i as f64, 500., 30. + i as f64, 25.))
        .collect();
    let res = run(
        &r,
        &mut s,
        "object.quick_nest",
        json!({"ids": many, "spacing": 3}),
    );
    assert_eq!(ids_of(&res).len(), 12);
    no_overlap(&s, &many, 3.0);
    for id in &many {
        let b = bbox(&s, *id);
        assert!(b[0] >= -1e-9 && b[1] >= -1e-9 && b[2] <= 400.0 + 1e-9 && b[3] <= 400.0 + 1e-9);
    }
}
