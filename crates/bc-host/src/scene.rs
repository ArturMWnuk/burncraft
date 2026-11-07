//! Scena dla kanwy GUI: polilinie obiektów we współrzędnych świata (mm),
//! warstwy, zaznaczenie, karty dokumentów i historia (`gui.scene`).

use bc_commands::Workbench;
use bc_geom::{Affine, BezPath, PathEl, Point};
use serde_json::{json, Value};

/// Punkty przyciągania ścieżki (płaskie tablice x,y): węzły w numeracji
/// poleceń `node.*` (bc_geom::nodes) i środki segmentów (t = 0,5).
fn snap_points(p: &BezPath, nodes: &mut Vec<f64>, mids: &mut Vec<f64>) {
    let push = |v: &mut Vec<f64>, q: Point| {
        v.push(q.x);
        v.push(q.y);
    };
    for c in bc_geom::nodes::contours(p) {
        let n = c.nodes.len();
        for nd in &c.nodes {
            push(nodes, nd.p);
        }
        let segs = if c.closed { n } else { n - 1 };
        for i in 0..segs {
            let (a, b) = (&c.nodes[i], &c.nodes[(i + 1) % n]);
            let (p0, p3) = (a.p.to_vec2(), b.p.to_vec2());
            let p1 = a.hout.unwrap_or(a.p).to_vec2();
            let p2 = b.hin.unwrap_or(b.p).to_vec2();
            push(mids, ((p0 + p1 * 3.0 + p2 * 3.0 + p3) / 8.0).to_point());
        }
    }
}

/// Węzeł drzewa obiektów (panel Obiekty, REQ-02-040).
fn tree_node(o: &bc_doc::Object) -> Value {
    let kind = serde_json::to_value(&o.kind)
        .ok()
        .and_then(|v| v.get("type").cloned())
        .unwrap_or(Value::Null);
    json!({
        "id": o.id, "name": o.name, "type": kind, "layer": o.layer,
        "visible": o.visible, "locked": o.locked, "helper": o.helper,
        "tags": o.tags, "note": o.note,
        "children": o.children().iter().map(tree_node).collect::<Vec<_>>(),
    })
}

/// Parametry kształtu parametrycznego (okno Właściwości kształtu, REQ-03-047);
/// `null` dla ścieżek, grup, tablic i klonów.
fn shape_params(k: &bc_doc::Kind) -> Value {
    use bc_doc::Kind::*;
    if !matches!(
        k,
        Rect { .. } | Ellipse { .. } | Polygon { .. } | Star { .. } | Arc { .. }
    ) {
        return Value::Null;
    }
    let mut v = serde_json::to_value(k).unwrap_or(Value::Null);
    if let Some(m) = v.as_object_mut() {
        m.remove("type");
    }
    v
}

/// Współrzędna zaokrąglona do 0,1 µm — krótszy JSON sceny (dokładne wartości
/// węzłów do edycji daje `node.list`).
fn r4(v: f64) -> f64 {
    (v * 1e4).round() / 1e4
}

/// Skrót wszystkiego, co klient rysuje dla obiektu: geometria w świecie
/// (także klonów i tablic), warstwy, flagi, nazwa, tolerancja spłaszczania.
fn object_hash(
    o: &bc_doc::Object,
    paths: &[(u8, bc_doc::ObjectId, BezPath)],
    tolerance: f64,
) -> String {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    tolerance.to_bits().hash(&mut h);
    (o.layer, o.visible, o.locked, o.helper, &o.name, &o.note).hash(&mut h);
    shape_params(&o.kind).to_string().hash(&mut h);
    for v in o.transform {
        v.to_bits().hash(&mut h);
    }
    for (layer, id, p) in paths {
        (layer, id).hash(&mut h);
        for el in p.elements() {
            let pts: &[Point] = match el {
                PathEl::MoveTo(a) | PathEl::LineTo(a) => std::slice::from_ref(a),
                PathEl::QuadTo(a, b) => &[*a, *b],
                PathEl::CurveTo(a, b, c) => &[*a, *b, *c],
                PathEl::ClosePath => &[],
            };
            pts.len().hash(&mut h);
            for q in pts {
                (q.x.to_bits(), q.y.to_bits()).hash(&mut h);
            }
        }
    }
    format!("{:016x}", h.finish())
}

/// Scena. `have` (id → skrót) z poprzedniego pobrania: obiekty o tym samym
/// skrócie przychodzą jako `{id, hash, same: true}` bez geometrii (REQ-16-006 —
/// zmiana jednego obiektu w dużym dokumencie nie przesyła całej sceny).
pub fn build(
    bench: &Workbench,
    revision: u64,
    tolerance: f64,
    have: Option<&serde_json::Map<String, Value>>,
    have_tree: Option<&str>,
) -> Value {
    let s = bench.active();
    let tree: Value = s
        .doc
        .objects
        .iter()
        .map(tree_node)
        .collect::<Vec<_>>()
        .into();
    let tree_hash = {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        tree.to_string().hash(&mut h);
        format!("{:016x}", h.finish())
    };
    let doc = &s.doc;
    let objects: Vec<Value> = doc
        .objects
        .iter()
        .map(|o| {
            let mut paths = Vec::new();
            let mut error = None;
            if let Err(e) = o.world_paths(doc, Affine::IDENTITY, &mut paths) {
                error = Some(e.to_string());
            }
            let hash = object_hash(o, &paths, tolerance);
            if have
                .and_then(|m| m.get(&o.id.to_string()))
                .and_then(Value::as_str)
                == Some(hash.as_str())
            {
                // klient zna skrót — wystarczy id
                return json!({"id": o.id, "same": true});
            }
            let mut polylines = Vec::new();
            let mut layers = Vec::new();
            let mut closed = Vec::new();
            let mut bbox: Option<[f64; 4]> = None;
            let mut nodes = Vec::new();
            let mut mids = Vec::new();
            for (layer, _, p) in &paths {
                snap_points(p, &mut nodes, &mut mids);
                for poly in bc_geom::flatten(p, tolerance) {
                    let mut flat = Vec::with_capacity(poly.len() * 2);
                    for pt in &poly {
                        flat.push(r4(pt.x));
                        flat.push(r4(pt.y));
                        let b = bbox.get_or_insert([pt.x, pt.y, pt.x, pt.y]);
                        b[0] = b[0].min(pt.x);
                        b[1] = b[1].min(pt.y);
                        b[2] = b[2].max(pt.x);
                        b[3] = b[3].max(pt.y);
                    }
                    let is_closed = poly.len() > 2
                        && poly
                            .first()
                            .zip(poly.last())
                            .is_some_and(|(a, b)| a.distance(*b) < 1e-9);
                    closed.push(is_closed);
                    polylines.push(flat);
                    layers.push(*layer);
                }
            }
            let kind = serde_json::to_value(&o.kind)
                .ok()
                .and_then(|v| v.get("type").cloned())
                .unwrap_or(Value::Null);
            let nodes: Vec<f64> = nodes.into_iter().map(r4).collect();
            let mids: Vec<f64> = mids.into_iter().map(r4).collect();
            json!({
                "id": o.id, "hash": hash, "name": o.name, "type": kind, "layer": o.layer,
                "visible": o.visible, "locked": o.locked, "helper": o.helper,
                "note": o.note, "transform": o.transform, "bbox": bbox,
                "polylines": polylines, "polyline_layers": layers, "closed": closed,
                "nodes": nodes, "mids": mids, "error": error,
                "params": shape_params(&o.kind),
            })
        })
        .collect();
    let docs: Vec<Value> = bench
        .docs
        .iter()
        .map(|d| json!({"name": d.doc.name, "path": d.path, "dirty": d.dirty}))
        .collect();
    json!({
        "revision": revision,
        "name": doc.name,
        "workspace": [doc.workspace.width, doc.workspace.height],
        "layers": doc.layers,
        "objects": objects,
        // drzewo tylko po zmianie (klient podaje skrót poprzedniego w `have_tree`)
        "tree": if have_tree == Some(tree_hash.as_str()) { Value::Null } else { tree },
        "tree_hash": tree_hash,
        "selection": s.selection,
        "notes": doc.notes,
        "note_images": doc.note_images.iter()
            .map(|(name, data)| json!({"name": name, "size": data.len()}))
            .collect::<Vec<_>>(),
        "docs": docs,
        "active": bench.active,
        "history": {
            "undo": s.history.undo_labels(),
            "redo": s.history.redo_labels(),
        },
    })
}
