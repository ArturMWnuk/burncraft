//! Polecenia edycji węzłów (M1.4b2, REQ-04-023…035). Współrzędne w parametrach
//! i wynikach są światowe (mm), także dla obiektów w grupach. Edycja zamienia
//! kształt parametryczny na ścieżkę (jak `path.convert`).

use bc_doc::ObjectId;
use bc_geom::nodes::{self, Axis, Contour, Side};
use bc_geom::{Affine, BezPath, Point};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::builtin::{finite, IdResult};
use crate::edit::{add, ensure_path, geom, invalid, set_path};
use crate::{ok, parse, CmdError, Registry, SafetyTier, Session};

fn pt([x, y]: [f64; 2]) -> Result<Point, CmdError> {
    Ok(Point::new(finite("x", x)?, finite("y", y)?))
}

fn xy(p: Point) -> [f64; 2] {
    [p.x, p.y]
}

fn parent(s: &Session, id: ObjectId) -> Affine {
    s.doc.parent_affine(id).unwrap_or(Affine::IDENTITY)
}

/// Ścieżka obiektu we współrzędnych świata; zamienia kształt na ścieżkę.
pub(crate) fn load_path(s: &mut Session, id: ObjectId) -> Result<BezPath, CmdError> {
    s.targets(&Some(vec![id]))?;
    let mut p = ensure_path(&mut s.doc, id)?;
    p.apply_affine(parent(s, id));
    Ok(p)
}

/// Zapisuje ścieżkę w świecie (→ układ rodzica); pusta ścieżka usuwa obiekt.
pub(crate) fn store_path(s: &mut Session, id: ObjectId, mut p: BezPath) -> Result<(), CmdError> {
    if p.elements().is_empty() {
        s.doc.remove(id)?;
        s.prune_selection();
        return Ok(());
    }
    p.apply_affine(parent(s, id).inverse());
    set_path(&mut s.doc, id, &p);
    Ok(())
}

/// Kontury obiektu we współrzędnych świata; zamienia kształt na ścieżkę.
fn load(s: &mut Session, id: ObjectId) -> Result<Vec<Contour>, CmdError> {
    Ok(nodes::contours(&load_path(s, id)?))
}

/// Zapisuje kontury; brak węzłów usuwa obiekt.
fn store(s: &mut Session, id: ObjectId, cs: &[Contour]) -> Result<(), CmdError> {
    let p = if nodes::count(cs) == 0 {
        BezPath::new()
    } else {
        nodes::to_path(cs)
    };
    store_path(s, id, p)
}

fn edit(
    s: &mut Session,
    id: ObjectId,
    f: impl FnOnce(&mut Vec<Contour>) -> Result<(), bc_geom::ops::GeomError>,
) -> Result<Value, CmdError> {
    let mut cs = load(s, id)?;
    f(&mut cs).map_err(geom)?;
    store(s, id, &cs)?;
    ok(IdResult { id })
}

// ---------- node.list ----------

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct IdParams {
    /// Id obiektu.
    id: ObjectId,
}

#[derive(Serialize, JsonSchema)]
struct NodeOut {
    /// Położenie [x, y] w mm.
    p: [f64; 2],
    /// Uchwyt wejściowy (null = brak).
    hin: Option<[f64; 2]>,
    /// Uchwyt wyjściowy (null = brak).
    hout: Option<[f64; 2]>,
    /// Uchwyty współliniowe po przeciwnych stronach.
    smooth: bool,
}

#[derive(Serialize, JsonSchema)]
struct ContourOut {
    closed: bool,
    nodes: Vec<NodeOut>,
}

#[derive(Serialize, JsonSchema)]
struct NodeListResult {
    id: ObjectId,
    /// Kontury; węzły numerowane kolejno we wszystkich konturach.
    contours: Vec<ContourOut>,
}

fn to_out(cs: &[Contour]) -> Vec<ContourOut> {
    cs.iter()
        .map(|c| ContourOut {
            closed: c.closed,
            nodes: c
                .nodes
                .iter()
                .map(|n| NodeOut {
                    p: xy(n.p),
                    hin: n.hin.map(xy),
                    hout: n.hout.map(xy),
                    smooth: n.smooth(),
                })
                .collect(),
        })
        .collect()
}

fn node_list(s: &mut Session, p: Value) -> Result<Value, CmdError> {
    let p: IdParams = parse(p)?;
    let o = s
        .doc
        .find(p.id)
        .ok_or_else(|| CmdError::new("not_found", format!("nie znaleziono obiektu {}", p.id)))?;
    if !o.children().is_empty() {
        return Err(invalid(format!(
            "obiekt {} jest grupą — wskaż jej element",
            p.id
        )));
    }
    let mut path = o.local_path()?;
    path.apply_affine(parent(s, p.id) * o.affine());
    ok(NodeListResult {
        id: p.id,
        contours: to_out(&nodes::contours(&path)),
    })
}

// ---------- edycja ----------

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct MoveParams {
    id: ObjectId,
    /// Indeks węzła.
    node: usize,
    /// Nowe położenie [x, y] w mm.
    #[serde(default)]
    to: Option<[f64; 2]>,
    /// Albo przesunięcie [dx, dy] w mm.
    #[serde(default)]
    by: Option<[f64; 2]>,
}

fn node_move(s: &mut Session, p: Value) -> Result<Value, CmdError> {
    let p: MoveParams = parse(p)?;
    let by = p.by.map(pt).transpose()?;
    let to = p.to.map(pt).transpose()?;
    edit(s, p.id, |cs| {
        let cur = nodes::locate(cs, p.node)
            .map(|(c, i)| cs[c].nodes[i].p)
            .ok_or_else(|| {
                bc_geom::ops::GeomError::Invalid(format!("węzeł {} poza zakresem", p.node))
            })?;
        let target = match (to, by) {
            (Some(t), None) => t,
            (None, Some(d)) => cur + d.to_vec2(),
            _ => {
                return Err(bc_geom::ops::GeomError::Invalid(
                    "podaj dokładnie jedno z: to, by".into(),
                ))
            }
        };
        nodes::move_node(cs, p.node, target)
    })
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
enum SideParam {
    In,
    Out,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct HandleParams {
    id: ObjectId,
    node: usize,
    /// Uchwyt wejściowy (in) albo wyjściowy (out).
    side: SideParam,
    /// Położenie uchwytu [x, y] w mm.
    to: [f64; 2],
}

fn node_set_handle(s: &mut Session, p: Value) -> Result<Value, CmdError> {
    let p: HandleParams = parse(p)?;
    let to = pt(p.to)?;
    let side = match p.side {
        SideParam::In => Side::In,
        SideParam::Out => Side::Out,
    };
    edit(s, p.id, |cs| nodes::set_handle(cs, p.node, side, to))
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct AtParams {
    id: ObjectId,
    /// Punkt [x, y] w mm; użyty zostanie najbliższy punkt ścieżki.
    at: [f64; 2],
}

#[derive(Serialize, JsonSchema)]
struct NodeResult {
    id: ObjectId,
    /// Indeks nowego węzła.
    node: usize,
}

fn node_add(s: &mut Session, p: Value) -> Result<Value, CmdError> {
    let p: AtParams = parse(p)?;
    let at = pt(p.at)?;
    let mut cs = load(s, p.id)?;
    let node = nodes::insert_at(&mut cs, at).map_err(geom)?;
    store(s, p.id, &cs)?;
    ok(NodeResult { id: p.id, node })
}

fn path_cut(s: &mut Session, p: Value) -> Result<Value, CmdError> {
    let p: AtParams = parse(p)?;
    let at = pt(p.at)?;
    edit(s, p.id, |cs| nodes::cut_at(cs, at))
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct NodesParams {
    id: ObjectId,
    /// Indeksy węzłów.
    nodes: Vec<usize>,
}

fn node_delete(s: &mut Session, p: Value) -> Result<Value, CmdError> {
    let p: NodesParams = parse(p)?;
    edit(s, p.id, |cs| nodes::delete_nodes(cs, &p.nodes))
}

fn node_smooth(s: &mut Session, p: Value) -> Result<Value, CmdError> {
    let p: NodesParams = parse(p)?;
    edit(s, p.id, |cs| nodes::make_smooth(cs, &p.nodes))
}

fn node_corner(s: &mut Session, p: Value) -> Result<Value, CmdError> {
    let p: NodesParams = parse(p)?;
    edit(s, p.id, |cs| nodes::make_corner(cs, &p.nodes))
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct OneNodeParams {
    id: ObjectId,
    node: usize,
}

fn node_break(s: &mut Session, p: Value) -> Result<Value, CmdError> {
    let p: OneNodeParams = parse(p)?;
    edit(s, p.id, |cs| nodes::break_at(cs, p.node))
}

fn default_join_tol() -> f64 {
    0.01
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct JoinParams {
    id: ObjectId,
    /// Węzeł końcowy pierwszej ścieżki.
    a: usize,
    /// Węzeł końcowy drugiej (albo tej samej) ścieżki.
    b: usize,
    /// Końce bliżej niż tolerancja (mm) są spawane w jeden węzeł; dalsze łączy odcinek.
    #[serde(default = "default_join_tol")]
    tolerance: f64,
}

fn node_join(s: &mut Session, p: Value) -> Result<Value, CmdError> {
    let p: JoinParams = parse(p)?;
    let tol = finite("tolerance", p.tolerance)?.max(0.0);
    edit(s, p.id, |cs| nodes::join_ends(cs, p.a, p.b, tol))
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
enum SegmentKind {
    Line,
    Curve,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct SegmentParams {
    id: ObjectId,
    /// Segment = indeks węzła, w którym się zaczyna.
    segment: usize,
    kind: SegmentKind,
}

fn node_segment(s: &mut Session, p: Value) -> Result<Value, CmdError> {
    let p: SegmentParams = parse(p)?;
    let curve = matches!(p.kind, SegmentKind::Curve);
    edit(s, p.id, |cs| nodes::set_segment_kind(cs, p.segment, curve))
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
enum AxisParam {
    X,
    Y,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct AlignParams {
    id: ObjectId,
    nodes: Vec<usize>,
    /// x — wspólna współrzędna X (w pionie), y — wspólna Y (w poziomie).
    axis: AxisParam,
    /// Wartość w mm (domyślnie średnia).
    #[serde(default)]
    value: Option<f64>,
}

fn node_align(s: &mut Session, p: Value) -> Result<Value, CmdError> {
    let p: AlignParams = parse(p)?;
    let value = p.value.map(|v| finite("value", v)).transpose()?;
    let axis = match p.axis {
        AxisParam::X => Axis::X,
        AxisParam::Y => Axis::Y,
    };
    edit(s, p.id, |cs| nodes::align(cs, &p.nodes, axis, value))
}

pub(crate) fn register_all(r: &mut Registry) {
    use SafetyTier::*;
    add::<IdParams, NodeListResult>(
        r,
        "node.list",
        "Węzły",
        "Kontury i węzły obiektu (położenie, uchwyty, gładkość) we współrzędnych świata.",
        Read,
        false,
        &["REQ-04-032"],
        node_list,
    );
    add::<MoveParams, IdResult>(
        r,
        "node.move",
        "Przesuń węzeł",
        "Ustawia węzeł w punkcie `to` albo przesuwa o `by`; uchwyty jadą z węzłem.",
        Design,
        true,
        &["REQ-04-032"],
        node_move,
    );
    add::<HandleParams, IdResult>(
        r,
        "node.set_handle",
        "Uchwyt węzła",
        "Ustawia uchwyt Béziera; w węźle gładkim przeciwny uchwyt zachowuje kierunek.",
        Design,
        true,
        &["REQ-04-001"],
        node_set_handle,
    );
    add::<AtParams, NodeResult>(
        r,
        "node.add",
        "Dodaj węzeł",
        "Dodaje węzeł w najbliższym punkcie ścieżki; kształt się nie zmienia.",
        Design,
        true,
        &["REQ-04-023"],
        node_add,
    );
    add::<NodesParams, IdResult>(
        r,
        "node.delete",
        "Usuń węzły",
        "Usuwa węzły; kontur z jednym węzłem znika, obiekt bez węzłów jest usuwany.",
        Design,
        true,
        &["REQ-04-024"],
        node_delete,
    );
    add::<OneNodeParams, IdResult>(
        r,
        "node.break",
        "Przełam w węźle",
        "Kontur zamknięty staje się otwarty w węźle, otwarty dzieli się na dwa.",
        Design,
        true,
        &["REQ-04-025"],
        node_break,
    );
    add::<JoinParams, IdResult>(
        r,
        "node.join",
        "Połącz węzły",
        "Łączy dwa końce otwartych konturów obiektu: ten sam kontur zamyka, różne scala.",
        Design,
        true,
        &["REQ-04-026"],
        node_join,
    );
    add::<NodesParams, IdResult>(
        r,
        "node.smooth",
        "Wygładź węzły",
        "Uchwyty współliniowe wzdłuż stycznej wyznaczonej przez sąsiednie węzły.",
        Design,
        true,
        &["REQ-04-027"],
        node_smooth,
    );
    add::<NodesParams, IdResult>(
        r,
        "node.corner",
        "Narożnik",
        "Usuwa uchwyty węzłów — ostre załamanie.",
        Design,
        true,
        &["REQ-04-028"],
        node_corner,
    );
    add::<SegmentParams, IdResult>(
        r,
        "node.segment",
        "Linia ↔ krzywa",
        "Zamienia segment na odcinek prosty albo krzywą Béziera.",
        Design,
        true,
        &["REQ-04-029"],
        node_segment,
    );
    add::<AlignParams, IdResult>(
        r,
        "node.align",
        "Wyrównaj węzły",
        "Ustawia wspólną współrzędną X albo Y węzłów (domyślnie średnią).",
        Design,
        true,
        &["REQ-04-033"],
        node_align,
    );
    add::<AtParams, IdResult>(
        r,
        "path.cut",
        "Nożyczki",
        "Przecina ścieżkę w najbliższym punkcie (nowy węzeł + przełamanie).",
        Design,
        true,
        &["REQ-04-035"],
        path_cut,
    );
}
