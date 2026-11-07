//! Polecenia edycji M1.1: kształty parametryczne, booleany, offsety, operacje na
//! ścieżkach, wyrównywanie, tablice, pomiar, paleta warstw.

use bc_doc::{Document, Kind, Layer, Object, ObjectId};
use bc_geom::ops::{self, BoolOp, GeomError, Join};
use bc_geom::{Affine, BezPath, Point, Rect, Vec2};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::builtin::{
    created, default_layer, finite, positive, top_most, world_bbox, IdResult, Ids, IdsResult,
};
use crate::{ok, parse, schema_of, CmdError, CommandSpec, Handler, Registry, SafetyTier, Session};

pub(crate) fn invalid(m: impl Into<String>) -> CmdError {
    CmdError::new("invalid_params", m)
}

pub(crate) fn geom(e: GeomError) -> CmdError {
    invalid(e.to_string())
}

// ---------- pomocnicze ----------

/// Geometria obiektu we współrzędnych świata (wszystkie liście połączone).
pub(crate) fn world_path(doc: &Document, id: ObjectId) -> Result<BezPath, CmdError> {
    let o = doc
        .find(id)
        .ok_or_else(|| CmdError::new("not_found", format!("nie znaleziono obiektu {id}")))?;
    let mut out = Vec::new();
    o.world_paths(
        doc,
        doc.parent_affine(id).unwrap_or(Affine::IDENTITY),
        &mut out,
    )?;
    let mut p = BezPath::new();
    for (_, _, q) in out {
        for el in q.elements() {
            p.push(*el);
        }
    }
    Ok(p)
}

pub(crate) fn bbox_of(doc: &Document, id: ObjectId) -> Result<Rect, CmdError> {
    world_bbox(doc, &[id])?.ok_or_else(|| invalid(format!("obiekt {id} nie ma geometrii")))
}

pub(crate) fn check_unlocked(
    doc: &Document,
    ids: &[ObjectId],
    size_change: bool,
) -> Result<(), CmdError> {
    for id in ids {
        let o = doc.find(*id).expect("sprawdzone");
        if o.locked || (size_change && o.lock_size) {
            return Err(CmdError::new(
                "locked",
                format!("obiekt {id} jest zablokowany"),
            ));
        }
    }
    Ok(())
}

/// Stosuje transformację świata do obiektu (z uwzględnieniem grup nadrzędnych).
pub(crate) fn apply_world(doc: &mut Document, id: ObjectId, t: Affine) {
    let pa = doc.parent_affine(id).unwrap_or(Affine::IDENTITY);
    let o = doc.find_mut(id).expect("sprawdzone");
    o.transform = (pa.inverse() * t * pa * o.affine()).as_coeffs();
}

/// Miejsce w drzewie: kontener (`None` = najwyższy poziom) i indeks.
#[derive(Clone, Copy)]
pub(crate) struct Place {
    parent: Option<ObjectId>,
    index: usize,
}

/// Miejsce pierwszego z obiektów w jego kontenerze (wyniki operacji trafiają tam).
pub(crate) fn place_of(doc: &Document, ids: &[ObjectId]) -> Place {
    let parent = ids.first().and_then(|id| doc.parent_of(*id)).flatten();
    let index = doc
        .container(parent)
        .and_then(|l| l.iter().position(|o| ids.contains(&o.id)))
        .unwrap_or(0);
    Place { parent, index }
}

/// Transformacja świata dzieci kontenera.
fn container_affine(doc: &Document, parent: Option<ObjectId>) -> Affine {
    match parent {
        None => Affine::IDENTITY,
        Some(p) => {
            doc.parent_affine(p).unwrap_or(Affine::IDENTITY)
                * doc.find(p).map(|o| o.affine()).unwrap_or(Affine::IDENTITY)
        }
    }
}

/// Tworzy obiekty-ścieżki z geometrii świata w miejscu `at` (w kontenerze, z przeliczeniem
/// na jego układ lokalny).
pub(crate) fn insert_paths(
    s: &mut Session,
    at: Place,
    layer: u8,
    paths: Vec<BezPath>,
) -> Vec<ObjectId> {
    let parent = if at
        .parent
        .is_some_and(|p| s.doc.container(Some(p)).is_none())
    {
        None
    } else {
        at.parent
    };
    let inv = container_affine(&s.doc, parent).inverse();
    let mut ids = Vec::new();
    let mut objs = Vec::new();
    for mut p in paths {
        if p.elements().is_empty() {
            continue;
        }
        p.apply_affine(inv);
        let id = s.doc.alloc_id();
        objs.push(Object::new(id, layer, Kind::Path { d: p.to_svg() }));
        ids.push(id);
    }
    let list = s.doc.container_mut(parent).expect("kontener istnieje");
    let at = at.index.min(list.len());
    list.splice(at..at, objs);
    ids
}

pub(crate) fn remove_all(s: &mut Session, ids: &[ObjectId]) -> Result<(), CmdError> {
    for id in ids {
        s.doc.remove(*id)?;
    }
    Ok(())
}

/// Zamienia liść na ścieżkę z „wypieczoną” własną transformacją; zwraca ścieżkę lokalną.
pub(crate) fn ensure_path(doc: &mut Document, id: ObjectId) -> Result<BezPath, CmdError> {
    let o = doc
        .find_mut(id)
        .ok_or_else(|| CmdError::new("not_found", format!("nie znaleziono obiektu {id}")))?;
    if matches!(o.kind, Kind::Group { .. }) {
        return Err(invalid(format!(
            "obiekt {id} jest grupą — rozgrupuj go albo wskaż elementy"
        )));
    }
    if o.locked {
        return Err(CmdError::new(
            "locked",
            format!("obiekt {id} jest zablokowany"),
        ));
    }
    let mut p = o.local_path()?;
    p.apply_affine(o.affine());
    o.kind = Kind::Path { d: p.to_svg() };
    o.transform = Affine::IDENTITY.as_coeffs();
    Ok(p)
}

pub(crate) fn set_path(doc: &mut Document, id: ObjectId, p: &BezPath) {
    let o = doc.find_mut(id).expect("sprawdzone");
    o.kind = Kind::Path { d: p.to_svg() };
}

pub(crate) fn layer_of(doc: &Document, id: ObjectId) -> u8 {
    doc.find(id).map(|o| o.layer).unwrap_or(0)
}

pub(crate) fn pt([x, y]: [f64; 2]) -> Result<Point, CmdError> {
    Ok(Point::new(finite("x", x)?, finite("y", y)?))
}

// ---------- kształty ----------

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct PolygonParams {
    /// Środek X (mm).
    cx: f64,
    /// Środek Y (mm).
    cy: f64,
    /// Promień okręgu opisanego (mm).
    r: f64,
    /// Liczba boków (≥ 3; 3 = trójkąt, 5 = pięciokąt, 8 = ośmiokąt).
    sides: u32,
    /// Kierunek pierwszego wierzchołka w stopniach (0 = oś +X).
    #[serde(default)]
    rotation: f64,
    #[serde(default = "default_layer")]
    layer: u8,
}

fn shape_polygon(s: &mut Session, p: Value) -> Result<Value, CmdError> {
    let p: PolygonParams = parse(p)?;
    ops::regular_polygon(p.cx, p.cy, p.r, p.sides, p.rotation).map_err(geom)?;
    let res = created(
        s,
        p.layer,
        Kind::Polygon {
            cx: p.cx,
            cy: p.cy,
            r: p.r,
            sides: p.sides,
            rotation: p.rotation,
        },
    )?;
    let id = res["id"].as_u64().expect("id");
    crate::builtin::ensure_in_range(&s.doc, &[id])?;
    Ok(res)
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct StarParams {
    cx: f64,
    cy: f64,
    /// Promień ramion (mm).
    r_outer: f64,
    /// Promień wcięć (mm).
    r_inner: f64,
    /// Podwójna gwiazda: promień krótszych ramion między głównymi.
    #[serde(default)]
    r_middle: Option<f64>,
    /// Liczba ramion (≥ 2).
    points: u32,
    /// Kierunek pierwszego ramienia w stopniach.
    #[serde(default)]
    rotation: f64,
    #[serde(default = "default_layer")]
    layer: u8,
}

fn shape_star(s: &mut Session, p: Value) -> Result<Value, CmdError> {
    let p: StarParams = parse(p)?;
    match p.r_middle {
        None => ops::star(p.cx, p.cy, p.r_outer, p.r_inner, p.points, p.rotation).map(|_| ()),
        Some(m) => {
            ops::double_star(p.cx, p.cy, p.r_outer, m, p.r_inner, p.points, p.rotation).map(|_| ())
        }
    }
    .map_err(geom)?;
    let res = created(
        s,
        p.layer,
        Kind::Star {
            cx: p.cx,
            cy: p.cy,
            r_outer: p.r_outer,
            r_inner: p.r_inner,
            r_middle: p.r_middle,
            points: p.points,
            rotation: p.rotation,
        },
    )?;
    let id = res["id"].as_u64().expect("id");
    crate::builtin::ensure_in_range(&s.doc, &[id])?;
    Ok(res)
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct ArcParams {
    /// Środek X (wariant środek + promień + kąty).
    #[serde(default)]
    cx: Option<f64>,
    #[serde(default)]
    cy: Option<f64>,
    #[serde(default)]
    r: Option<f64>,
    /// Kąt początkowy (stopnie).
    #[serde(default)]
    start: Option<f64>,
    /// Rozwartość (stopnie, > 0 = przeciwnie do ruchu wskazówek zegara).
    #[serde(default)]
    sweep: Option<f64>,
    /// Wariant przez 3 punkty: [[x1,y1],[x2,y2],[x3,y3]].
    #[serde(default)]
    through: Option<[[f64; 2]; 3]>,
    #[serde(default = "default_layer")]
    layer: u8,
}

fn shape_arc(s: &mut Session, p: Value) -> Result<Value, CmdError> {
    let p: ArcParams = parse(p)?;
    let (cx, cy, r, start, sweep) = match (p.through, p.cx, p.cy, p.r, p.start, p.sweep) {
        (Some([a, b, c]), None, None, None, None, None) => {
            let (c0, r, st, sw) = ops::arc_through(pt(a)?, pt(b)?, pt(c)?)
                .ok_or_else(|| invalid("punkty łuku są współliniowe"))?;
            (c0.x, c0.y, r, st, sw)
        }
        (None, Some(cx), Some(cy), Some(r), Some(st), Some(sw)) => (cx, cy, r, st, sw),
        _ => {
            return Err(invalid(
                "podaj albo through (3 punkty), albo cx, cy, r, start, sweep",
            ))
        }
    };
    ops::arc(cx, cy, r, start, sweep).map_err(geom)?;
    let res = created(
        s,
        p.layer,
        Kind::Arc {
            cx,
            cy,
            r,
            start,
            sweep,
        },
    )?;
    let id = res["id"].as_u64().expect("id");
    crate::builtin::ensure_in_range(&s.doc, &[id])?;
    Ok(res)
}

// ---------- booleany i offsety ----------

#[derive(Deserialize, JsonSchema, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub(crate) enum BoolKind {
    /// Suma.
    Union,
    /// Pierwszy obiekt minus pozostałe.
    Difference,
    /// Część wspólna wszystkich.
    Intersection,
    /// Różnica symetryczna.
    Xor,
    /// Podział na wszystkie rozłączne części nakładania (wycięcie kształtów).
    Cut,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct BooleanParams {
    #[serde(default)]
    ids: Option<Vec<ObjectId>>,
    op: BoolKind,
}

fn path_boolean(s: &mut Session, p: Value) -> Result<Value, CmdError> {
    let p: BooleanParams = parse(p)?;
    let ids = top_most(&s.doc, s.targets(&p.ids)?);
    check_unlocked(&s.doc, &ids, false)?;
    let paths = boolean_inputs(&s.doc, &ids)?;
    let result = boolean_result(&paths, p.op)?;
    let layer = layer_of(&s.doc, ids[0]);
    let at = place_of(&s.doc, &ids);
    remove_all(s, &ids)?;
    let new = insert_paths(s, at, layer, result);
    s.selection = new.clone();
    ok(IdsResult { ids: new })
}

/// Geometria świata obiektów do operacji logicznej (co najmniej dwa, zamknięte).
pub(crate) fn boolean_inputs(doc: &Document, ids: &[ObjectId]) -> Result<Vec<BezPath>, CmdError> {
    if ids.len() < 2 {
        return Err(invalid(
            "operacja logiczna wymaga co najmniej dwóch obiektów",
        ));
    }
    let paths: Vec<BezPath> = ids
        .iter()
        .map(|id| world_path(doc, *id))
        .collect::<Result<_, _>>()?;
    let open: Vec<String> = ids
        .iter()
        .zip(&paths)
        .filter(|(_, p)| !bc_geom::is_closed(p))
        .map(|(id, _)| id.to_string())
        .collect();
    if !open.is_empty() {
        return Err(invalid(format!(
            "kształty otwarte nie biorą udziału w operacjach logicznych: {}",
            open.join(", ")
        )));
    }
    Ok(paths)
}

/// Wynik operacji logicznej; pierwszy element jest bazą różnicy.
pub(crate) fn boolean_result(paths: &[BezPath], op: BoolKind) -> Result<Vec<BezPath>, CmdError> {
    let b = |a: &[BezPath], c: &[BezPath], op| ops::boolean(a, c, op).map_err(geom);
    Ok(match op {
        BoolKind::Union => b(paths, &[], BoolOp::Union)?,
        BoolKind::Difference => b(&paths[..1], &paths[1..], BoolOp::Difference)?,
        BoolKind::Intersection | BoolKind::Xor => {
            let op = if op == BoolKind::Xor {
                BoolOp::Xor
            } else {
                BoolOp::Intersection
            };
            let mut acc = vec![paths[0].clone()];
            for q in &paths[1..] {
                acc = b(&acc, std::slice::from_ref(q), op)?;
            }
            acc
        }
        BoolKind::Cut => {
            let mut pieces: Vec<BezPath> = b(&paths[..1], &[], BoolOp::Union)?;
            for q in &paths[1..] {
                let mut next = Vec::new();
                for piece in &pieces {
                    next.extend(b(
                        std::slice::from_ref(piece),
                        std::slice::from_ref(q),
                        BoolOp::Intersection,
                    )?);
                    next.extend(b(
                        std::slice::from_ref(piece),
                        std::slice::from_ref(q),
                        BoolOp::Difference,
                    )?);
                }
                next.extend(b(std::slice::from_ref(q), &pieces, BoolOp::Difference)?);
                pieces = next;
            }
            pieces
        }
    })
}

#[derive(Deserialize, JsonSchema, Default, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
enum Direction {
    #[default]
    Outward,
    Inward,
    Both,
}

#[derive(Deserialize, JsonSchema, Default, Clone, Copy)]
#[serde(rename_all = "lowercase")]
enum Corners {
    /// Ostre (miter, limit 4×).
    Sharp,
    /// Zaokrąglone.
    #[default]
    Round,
    /// Ścięte.
    Bevel,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct OffsetParams {
    #[serde(default)]
    ids: Option<Vec<ObjectId>>,
    /// Odległość offsetu w mm (> 0).
    distance: f64,
    #[serde(default)]
    direction: Direction,
    #[serde(default)]
    corners: Corners,
    /// Usuń oryginały po utworzeniu offsetu.
    #[serde(default)]
    delete_original: bool,
}

fn path_offset(s: &mut Session, p: Value) -> Result<Value, CmdError> {
    let p: OffsetParams = parse(p)?;
    let d = positive("distance", p.distance)?;
    let ids = top_most(&s.doc, s.targets(&p.ids)?);
    let join = match p.corners {
        Corners::Sharp => Join::Miter,
        Corners::Round => Join::Round,
        Corners::Bevel => Join::Bevel,
    };
    let mut created_ids = Vec::new();
    let mut work = Vec::new();
    for (i, id) in ids.iter().enumerate() {
        s.tick(i, ids.len())?;
        let path = world_path(&s.doc, *id)?;
        if !bc_geom::is_closed(&path) {
            return Err(invalid(format!(
                "offset wymaga kształtów zamkniętych (obiekt {id} jest otwarty)"
            )));
        }
        let mut out = Vec::new();
        if matches!(p.direction, Direction::Outward | Direction::Both) {
            out.extend(ops::offset(std::slice::from_ref(&path), d, join).map_err(geom)?);
        }
        if matches!(p.direction, Direction::Inward | Direction::Both) {
            out.extend(ops::offset(std::slice::from_ref(&path), -d, join).map_err(geom)?);
        }
        work.push((*id, out));
    }
    if p.delete_original {
        check_unlocked(&s.doc, &ids, false)?;
    }
    for (id, out) in work {
        let layer = layer_of(&s.doc, id);
        let mut at = place_of(&s.doc, &[id]);
        at.index += 1;
        created_ids.extend(insert_paths(s, at, layer, out));
    }
    if p.delete_original {
        remove_all(s, &ids)?;
    }
    s.selection = created_ids.clone();
    ok(IdsResult { ids: created_ids })
}

// ---------- operacje na ścieżkach ----------

fn path_convert(s: &mut Session, p: Value) -> Result<Value, CmdError> {
    let p: Ids = parse(p)?;
    let ids = s.targets(&p.ids)?;
    for id in &ids {
        ensure_path(&mut s.doc, *id)?;
    }
    ok(IdsResult { ids })
}

fn path_reverse(s: &mut Session, p: Value) -> Result<Value, CmdError> {
    let p: Ids = parse(p)?;
    let ids = s.targets(&p.ids)?;
    for id in &ids {
        let path = ensure_path(&mut s.doc, *id)?;
        set_path(&mut s.doc, *id, &ops::reverse(&path));
    }
    ok(IdsResult { ids })
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct NodeParams {
    /// Id obiektu.
    id: ObjectId,
    /// Indeks węzła (0 = punkt startowy).
    node: usize,
}

fn path_set_start(s: &mut Session, p: Value) -> Result<Value, CmdError> {
    let p: NodeParams = parse(p)?;
    s.targets(&Some(vec![p.id]))?;
    let path = ensure_path(&mut s.doc, p.id)?;
    set_path(
        &mut s.doc,
        p.id,
        &ops::set_start(&path, p.node).map_err(geom)?,
    );
    ok(IdResult { id: p.id })
}

fn path_break_apart(s: &mut Session, p: Value) -> Result<Value, CmdError> {
    let p: Ids = parse(p)?;
    let ids = top_most(&s.doc, s.targets(&p.ids)?);
    check_unlocked(&s.doc, &ids, false)?;
    let mut new = Vec::new();
    for id in ids {
        let path = world_path(&s.doc, id)?;
        let subs = bc_geom::subpaths(&path);
        if subs.len() < 2 {
            continue;
        }
        let layer = layer_of(&s.doc, id);
        let at = place_of(&s.doc, &[id]);
        s.doc.remove(id)?;
        new.extend(insert_paths(s, at, layer, subs));
    }
    s.selection = new.clone();
    ok(IdsResult { ids: new })
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct ToleranceParams {
    #[serde(default)]
    ids: Option<Vec<ObjectId>>,
    /// Tolerancja w mm (domyślnie 0,1).
    #[serde(default = "default_tol")]
    tolerance: f64,
}

fn default_tol() -> f64 {
    0.1
}

#[derive(Serialize, JsonSchema)]
struct CloseResult {
    ids: Vec<ObjectId>,
    /// Liczba zamkniętych podścieżek.
    closed: usize,
}

fn path_close(s: &mut Session, p: Value) -> Result<Value, CmdError> {
    let p: ToleranceParams = parse(p)?;
    let tol = positive("tolerance", p.tolerance)?;
    let ids = s.targets(&p.ids)?;
    let mut total = 0;
    for id in &ids {
        if matches!(s.doc.find(*id).map(|o| &o.kind), Some(Kind::Group { .. })) {
            continue;
        }
        let path = ensure_path(&mut s.doc, *id)?;
        let (c, n) = ops::close_with_tolerance(&path, tol);
        if n > 0 {
            set_path(&mut s.doc, *id, &c);
        }
        total += n;
    }
    ok(CloseResult { ids, closed: total })
}

fn path_join(s: &mut Session, p: Value) -> Result<Value, CmdError> {
    let p: ToleranceParams = parse(p)?;
    let tol = positive("tolerance", p.tolerance)?;
    let ids = top_most(&s.doc, s.targets(&p.ids)?);
    check_unlocked(&s.doc, &ids, false)?;
    let paths: Vec<BezPath> = ids
        .iter()
        .map(|id| world_path(&s.doc, *id))
        .collect::<Result<_, _>>()?;
    let joined = ops::join_open(&paths, tol);
    let layer = layer_of(&s.doc, ids[0]);
    let at = place_of(&s.doc, &ids);
    remove_all(s, &ids)?;
    let new = insert_paths(s, at, layer, joined);
    s.selection = new.clone();
    ok(IdsResult { ids: new })
}

fn path_remove_duplicates(s: &mut Session, p: Value) -> Result<Value, CmdError> {
    let p: Ids = parse(p)?;
    let ids = top_most(&s.doc, s.targets(&p.ids)?);
    let paths: Vec<BezPath> = ids
        .iter()
        .map(|id| world_path(&s.doc, *id))
        .collect::<Result<_, _>>()?;
    let mut removed = Vec::new();
    for j in 0..ids.len() {
        let dup = (0..j)
            .any(|i| !removed.contains(&ids[i]) && ops::same_geometry(&paths[i], &paths[j], 1e-3));
        if dup {
            removed.push(ids[j]);
        }
    }
    check_unlocked(&s.doc, &removed, false)?;
    remove_all(s, &removed)?;
    s.prune_selection();
    ok(IdsResult { ids: removed })
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct ReduceParams {
    #[serde(default)]
    ids: Option<Vec<ObjectId>>,
    /// Maksymalne odchylenie od oryginału w mm (domyślnie 0,01).
    #[serde(default = "default_reduce_tol")]
    tolerance: f64,
}

fn default_reduce_tol() -> f64 {
    0.01
}

#[derive(Serialize, JsonSchema)]
struct ReduceResult {
    ids: Vec<ObjectId>,
    nodes_before: usize,
    nodes_after: usize,
}

fn path_reduce_nodes(s: &mut Session, p: Value) -> Result<Value, CmdError> {
    let p: ReduceParams = parse(p)?;
    let tol = positive("tolerance", p.tolerance)?;
    let ids = s.targets(&p.ids)?;
    let (mut before, mut after) = (0, 0);
    for (i, id) in ids.iter().enumerate() {
        s.tick(i, ids.len())?;
        let path = ensure_path(&mut s.doc, *id)?;
        let r = ops::reduce_nodes(&path, tol);
        before += bc_geom::node_count(&path);
        after += bc_geom::node_count(&r);
        set_path(&mut s.doc, *id, &r);
    }
    ok(ReduceResult {
        ids,
        nodes_before: before,
        nodes_after: after,
    })
}

fn path_convex_hull(s: &mut Session, p: Value) -> Result<Value, CmdError> {
    let p: Ids = parse(p)?;
    let ids = top_most(&s.doc, s.targets(&p.ids)?);
    let paths: Vec<BezPath> = ids
        .iter()
        .map(|id| world_path(&s.doc, *id))
        .collect::<Result<_, _>>()?;
    let hull = ops::convex_hull(&paths).map_err(geom)?;
    let layer = layer_of(&s.doc, ids[0]);
    let mut at = place_of(&s.doc, &ids);
    at.index = usize::MAX;
    let new = insert_paths(s, at, layer, vec![hull]);
    s.selection = new.clone();
    ok(IdResult { id: new[0] })
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct ChamferParams {
    id: ObjectId,
    /// Indeks węzła narożnika.
    node: usize,
    /// Odległość fazy od wierzchołka (mm).
    distance: f64,
}

fn path_chamfer(s: &mut Session, p: Value) -> Result<Value, CmdError> {
    let p: ChamferParams = parse(p)?;
    s.targets(&Some(vec![p.id]))?;
    let path = ensure_path(&mut s.doc, p.id)?;
    set_path(
        &mut s.doc,
        p.id,
        &ops::chamfer(&path, p.node, p.distance).map_err(geom)?,
    );
    ok(IdResult { id: p.id })
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct FilletParams {
    id: ObjectId,
    /// Indeks węzła narożnika.
    node: usize,
    /// Promień zaokrąglenia (mm).
    radius: f64,
}

fn path_fillet(s: &mut Session, p: Value) -> Result<Value, CmdError> {
    let p: FilletParams = parse(p)?;
    s.targets(&Some(vec![p.id]))?;
    let path = ensure_path(&mut s.doc, p.id)?;
    set_path(
        &mut s.doc,
        p.id,
        &ops::fillet(&path, p.node, p.radius).map_err(geom)?,
    );
    ok(IdResult { id: p.id })
}

// ---------- rozmieszczanie ----------

#[derive(Deserialize, JsonSchema, Clone, Copy)]
#[serde(rename_all = "lowercase")]
enum AlignMode {
    Left,
    Right,
    /// Środki w poziomie.
    Hcenter,
    Top,
    Bottom,
    /// Środki w pionie.
    Vcenter,
}

#[derive(Deserialize, JsonSchema, Default, Clone, Copy)]
#[serde(rename_all = "lowercase")]
enum AlignTo {
    /// Obrys całego zaznaczenia.
    #[default]
    Selection,
    /// Pierwszy obiekt listy.
    First,
    /// Ostatni obiekt listy.
    Last,
    /// Obszar roboczy.
    Page,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct AlignParams {
    #[serde(default)]
    ids: Option<Vec<ObjectId>>,
    mode: AlignMode,
    #[serde(default)]
    to: AlignTo,
}

fn object_align(s: &mut Session, p: Value) -> Result<Value, CmdError> {
    let p: AlignParams = parse(p)?;
    let ids = top_most(&s.doc, s.targets(&p.ids)?);
    check_unlocked(&s.doc, &ids, false)?;
    let reference = match p.to {
        AlignTo::Selection => world_bbox(&s.doc, &ids)?.ok_or_else(|| invalid("brak geometrii"))?,
        AlignTo::First => bbox_of(&s.doc, ids[0])?,
        AlignTo::Last => bbox_of(&s.doc, *ids.last().unwrap())?,
        AlignTo::Page => Rect::new(0.0, 0.0, s.doc.workspace.width, s.doc.workspace.height),
    };
    for id in &ids {
        let b = bbox_of(&s.doc, *id)?;
        let d = match p.mode {
            AlignMode::Left => Vec2::new(reference.x0 - b.x0, 0.0),
            AlignMode::Right => Vec2::new(reference.x1 - b.x1, 0.0),
            AlignMode::Hcenter => Vec2::new(reference.center().x - b.center().x, 0.0),
            AlignMode::Bottom => Vec2::new(0.0, reference.y0 - b.y0),
            AlignMode::Top => Vec2::new(0.0, reference.y1 - b.y1),
            AlignMode::Vcenter => Vec2::new(0.0, reference.center().y - b.center().y),
        };
        apply_world(&mut s.doc, *id, Affine::translate(d));
    }
    ok(IdsResult { ids })
}

#[derive(Deserialize, JsonSchema, Clone, Copy)]
#[serde(rename_all = "lowercase")]
enum Axis {
    Horizontal,
    Vertical,
}

#[derive(Deserialize, JsonSchema, Default, Clone, Copy)]
#[serde(rename_all = "lowercase")]
enum DistributeBy {
    /// Równe odstępy między środkami.
    #[default]
    Centers,
    /// Równe przerwy między krawędziami.
    Gaps,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct DistributeParams {
    #[serde(default)]
    ids: Option<Vec<ObjectId>>,
    axis: Axis,
    #[serde(default)]
    by: DistributeBy,
}

fn object_distribute(s: &mut Session, p: Value) -> Result<Value, CmdError> {
    let p: DistributeParams = parse(p)?;
    let ids = top_most(&s.doc, s.targets(&p.ids)?);
    if ids.len() < 3 {
        return Err(invalid("rozkład wymaga co najmniej trzech obiektów"));
    }
    check_unlocked(&s.doc, &ids, false)?;
    // (id, początek, koniec) wzdłuż osi
    let mut items: Vec<(ObjectId, f64, f64)> = ids
        .iter()
        .map(|id| {
            bbox_of(&s.doc, *id).map(|b| match p.axis {
                Axis::Horizontal => (*id, b.x0, b.x1),
                Axis::Vertical => (*id, b.y0, b.y1),
            })
        })
        .collect::<Result<_, _>>()?;
    let n = items.len();
    let targets: Vec<f64> = match p.by {
        DistributeBy::Centers => {
            items.sort_by(|a, b| (a.1 + a.2).partial_cmp(&(b.1 + b.2)).unwrap());
            let c0 = (items[0].1 + items[0].2) / 2.0;
            let c1 = (items[n - 1].1 + items[n - 1].2) / 2.0;
            (0..n)
                .map(|i| {
                    c0 + (c1 - c0) * i as f64 / (n - 1) as f64 - (items[i].2 - items[i].1) / 2.0
                })
                .collect()
        }
        DistributeBy::Gaps => {
            items.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());
            let start = items[0].1;
            let end = items.iter().map(|i| i.2).fold(f64::MIN, f64::max);
            let sizes: f64 = items.iter().map(|i| i.2 - i.1).sum();
            let gap = (end - start - sizes) / (n - 1) as f64;
            let mut pos = start;
            items
                .iter()
                .map(|i| {
                    let at = pos;
                    pos += i.2 - i.1 + gap;
                    at
                })
                .collect()
        }
    };
    for (item, target) in items.iter().zip(targets) {
        let delta = target - item.1;
        let d = match p.axis {
            Axis::Horizontal => Vec2::new(delta, 0.0),
            Axis::Vertical => Vec2::new(0.0, delta),
        };
        apply_world(&mut s.doc, item.0, Affine::translate(d));
    }
    ok(IdsResult { ids })
}

#[derive(Deserialize, JsonSchema, Clone, Copy)]
#[serde(rename_all = "lowercase")]
enum OrderTo {
    /// Na wierzch (koniec kolejności rysowania).
    Front,
    /// Na spód.
    Back,
    /// O jeden wyżej.
    Forward,
    /// O jeden niżej.
    Backward,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct OrderParams {
    #[serde(default)]
    ids: Option<Vec<ObjectId>>,
    to: OrderTo,
}

fn object_order(s: &mut Session, p: Value) -> Result<Value, CmdError> {
    let p: OrderParams = parse(p)?;
    let ids = s.targets(&p.ids)?;
    let parent = crate::builtin::common_parent(&s.doc, &ids)?;
    let sel = |o: &Object| ids.contains(&o.id);
    let objs = s
        .doc
        .container_mut(parent)
        .ok_or_else(|| invalid("rodzic nie jest kontenerem"))?;
    match p.to {
        OrderTo::Front | OrderTo::Back => {
            let (mut moved, rest): (Vec<Object>, Vec<Object>) = objs.drain(..).partition(sel);
            *objs = if matches!(p.to, OrderTo::Front) {
                let mut v = rest;
                v.append(&mut moved);
                v
            } else {
                moved.extend(rest);
                moved
            };
        }
        OrderTo::Forward => {
            for i in (0..objs.len().saturating_sub(1)).rev() {
                if sel(&objs[i]) && !sel(&objs[i + 1]) {
                    objs.swap(i, i + 1);
                }
            }
        }
        OrderTo::Backward => {
            for i in 1..objs.len() {
                if sel(&objs[i]) && !sel(&objs[i - 1]) {
                    objs.swap(i, i - 1);
                }
            }
        }
    }
    ok(IdsResult { ids })
}

#[derive(Deserialize, JsonSchema, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
enum Dimension {
    Width,
    Height,
    Both,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct EqualizeParams {
    /// Pierwszy obiekt jest wzorcem rozmiaru.
    #[serde(default)]
    ids: Option<Vec<ObjectId>>,
    dimension: Dimension,
    /// Zachowaj proporcje (dla width/height).
    #[serde(default)]
    keep_aspect: bool,
}

fn object_equalize(s: &mut Session, p: Value) -> Result<Value, CmdError> {
    let p: EqualizeParams = parse(p)?;
    let ids = top_most(&s.doc, s.targets(&p.ids)?);
    if ids.len() < 2 {
        return Err(invalid(
            "wymagane co najmniej dwa obiekty (pierwszy = wzorzec)",
        ));
    }
    check_unlocked(&s.doc, &ids[1..], true)?;
    let r = bbox_of(&s.doc, ids[0])?;
    for id in &ids[1..] {
        let b = bbox_of(&s.doc, *id)?;
        if b.width() == 0.0 || b.height() == 0.0 {
            return Err(invalid(format!("obiekt {id} ma zerowy wymiar")));
        }
        let (mut sx, mut sy) = (1.0, 1.0);
        match p.dimension {
            Dimension::Width => sx = r.width() / b.width(),
            Dimension::Height => sy = r.height() / b.height(),
            Dimension::Both => {
                sx = r.width() / b.width();
                sy = r.height() / b.height();
            }
        }
        if p.keep_aspect && p.dimension != Dimension::Both {
            if p.dimension == Dimension::Width {
                sy = sx;
            } else {
                sx = sy;
            }
        }
        let c = b.center().to_vec2();
        apply_world(
            &mut s.doc,
            *id,
            Affine::translate(c) * Affine::scale_non_uniform(sx, sy) * Affine::translate(-c),
        );
    }
    crate::builtin::ensure_in_range(&s.doc, &ids)?;
    ok(IdsResult { ids })
}

// ---------- tablice ----------

/// Kopia obiektu z nowymi identyfikatorami (także dzieci grup i tablic).
pub(crate) fn clone_new_ids(doc: &mut Document, o: &Object) -> Object {
    let mut c = o.clone();
    c.id = doc.alloc_id();
    bc_doc::renumber_kind(&mut c.kind, &mut doc.next_id);
    c
}

/// Kopiuje obiekty (z dowolnego poziomu drzewa); każda kopia dostaje dodatkową
/// transformację świata i trafia do kontenera oryginału.
pub(crate) fn copies(
    s: &mut Session,
    ids: &[ObjectId],
    transforms: &[Affine],
) -> Result<Vec<ObjectId>, CmdError> {
    let originals: Vec<(Object, Option<ObjectId>, Affine)> = ids
        .iter()
        .map(|id| {
            let o = s.doc.find(*id).unwrap().clone();
            (
                o,
                s.doc.parent_of(*id).flatten(),
                s.doc.parent_affine(*id).unwrap_or(Affine::IDENTITY),
            )
        })
        .collect();
    let mut new = Vec::new();
    for (i, t) in transforms.iter().enumerate() {
        s.tick(i, 2 * transforms.len())?;
        for (o, parent, pa) in &originals {
            let mut c = clone_new_ids(&mut s.doc, o);
            c.transform = (pa.inverse() * *t * *pa * c.affine()).as_coeffs();
            new.push(c.id);
            s.doc
                .container_mut(*parent)
                .expect("kontener istnieje")
                .push(c);
        }
    }
    s.selection = new.clone();
    Ok(new)
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct GridParams {
    #[serde(default)]
    ids: Option<Vec<ObjectId>>,
    /// Liczba kolumn (≥ 1).
    columns: u32,
    /// Liczba wierszy (≥ 1).
    rows: u32,
    /// Odstęp kolumn (mm, przesunięcie środka do środka).
    dx: f64,
    /// Odstęp wierszy (mm).
    dy: f64,
    /// Przesunięcie co drugiego wiersza w X (mm).
    #[serde(default)]
    row_shift: f64,
}

fn array_grid(s: &mut Session, p: Value) -> Result<Value, CmdError> {
    let p: GridParams = parse(p)?;
    if p.columns == 0 || p.rows == 0 || p.columns as u64 * p.rows as u64 > 100_000 {
        return Err(invalid("columns, rows ≥ 1, łącznie ≤ 100000 kopii"));
    }
    let (dx, dy, shift) = (
        finite("dx", p.dx)?,
        finite("dy", p.dy)?,
        finite("row_shift", p.row_shift)?,
    );
    let ids = top_most(&s.doc, s.targets(&p.ids)?);
    let mut ts = Vec::new();
    for r in 0..p.rows {
        for c in 0..p.columns {
            if r == 0 && c == 0 {
                continue;
            }
            let x = c as f64 * dx + if r % 2 == 1 { shift } else { 0.0 };
            ts.push(Affine::translate((x, r as f64 * dy)));
        }
    }
    let new = copies(s, &ids, &ts)?;
    crate::builtin::ensure_in_range_tick(s, &new)?;
    ok(IdsResult { ids: new })
}

fn default_true() -> bool {
    true
}

fn default_full() -> f64 {
    360.0
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct CircularParams {
    #[serde(default)]
    ids: Option<Vec<ObjectId>>,
    /// Liczba elementów łącznie z oryginałem (≥ 2).
    count: u32,
    /// Środek obrotu [x, y] (mm).
    center: [f64; 2],
    /// Kąt całkowity w stopniach (360 = pełny okrąg).
    #[serde(default = "default_full")]
    angle: f64,
    /// Obracaj kopie (false = tylko przesuwaj).
    #[serde(default = "default_true")]
    rotate_copies: bool,
}

fn array_circular(s: &mut Session, p: Value) -> Result<Value, CmdError> {
    let p: CircularParams = parse(p)?;
    if !(2..=100_000).contains(&p.count) {
        return Err(invalid("count: 2–100000"));
    }
    let total = finite("angle", p.angle)?;
    let c = pt(p.center)?.to_vec2();
    let ids = top_most(&s.doc, s.targets(&p.ids)?);
    let step = if total.abs() >= 360.0 {
        total / p.count as f64
    } else {
        total / (p.count - 1) as f64
    };
    let anchor = world_bbox(&s.doc, &ids)?
        .map(|b| b.center().to_vec2())
        .unwrap_or(c);
    let ts: Vec<Affine> = (1..p.count)
        .map(|i| {
            let rot = Affine::translate(c)
                * Affine::rotate((step * i as f64).to_radians())
                * Affine::translate(-c);
            if p.rotate_copies {
                rot
            } else {
                Affine::translate((rot * anchor.to_point()).to_vec2() - anchor)
            }
        })
        .collect();
    let new = copies(s, &ids, &ts)?;
    crate::builtin::ensure_in_range_tick(s, &new)?;
    ok(IdsResult { ids: new })
}

// ---------- pomiar ----------

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct DistanceParams {
    a: [f64; 2],
    b: [f64; 2],
}

#[derive(Serialize, JsonSchema)]
struct DistanceResult {
    distance: f64,
    dx: f64,
    dy: f64,
    /// Kierunek odcinka a→b w stopniach (0 = +X, CCW).
    angle: f64,
}

fn measure_distance(_: &mut Session, p: Value) -> Result<Value, CmdError> {
    let p: DistanceParams = parse(p)?;
    let (a, b) = (pt(p.a)?, pt(p.b)?);
    let d = b - a;
    ok(DistanceResult {
        distance: d.hypot(),
        dx: d.x,
        dy: d.y,
        angle: d.atan2().to_degrees(),
    })
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct AngleParams {
    a: [f64; 2],
    vertex: [f64; 2],
    b: [f64; 2],
}

#[derive(Serialize, JsonSchema)]
struct AngleResult {
    /// Kąt a–vertex–b w stopniach (0–180).
    degrees: f64,
}

fn measure_angle(_: &mut Session, p: Value) -> Result<Value, CmdError> {
    let p: AngleParams = parse(p)?;
    let (a, v, b) = (pt(p.a)?, pt(p.vertex)?, pt(p.b)?);
    let (u, w) = (a - v, b - v);
    if u.hypot() == 0.0 || w.hypot() == 0.0 {
        return Err(invalid("punkty a i b muszą różnić się od wierzchołka"));
    }
    ok(AngleResult {
        degrees: u.cross(w).abs().atan2(u.dot(w)).to_degrees(),
    })
}

// ---------- paleta warstw ----------

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct LayerAddParams {
    /// Kolor #rrggbb.
    color: String,
    #[serde(default)]
    name: Option<String>,
}

fn layer_add(s: &mut Session, p: Value) -> Result<Value, CmdError> {
    let p: LayerAddParams = parse(p)?;
    let c = &p.color;
    if !(c.len() == 7 && c.starts_with('#') && c[1..].chars().all(|ch| ch.is_ascii_hexdigit())) {
        return Err(invalid("color: format #rrggbb"));
    }
    let next = s
        .doc
        .layers
        .iter()
        .map(|l| l.index)
        .filter(|i| *i >= bc_doc::FIRST_CUSTOM_INDEX)
        .max()
        .map_or(Some(bc_doc::FIRST_CUSTOM_INDEX), |m| m.checked_add(1))
        .ok_or_else(|| invalid("osiągnięto limit 256 warstw"))?;
    let layer = Layer {
        color: c.to_lowercase(),
        name: p.name.unwrap_or_else(|| format!("C{next:02}")),
        ..Layer::default_for(next)
    };
    s.doc.layers.push(layer.clone());
    ok(layer)
}

// ---------- rejestracja ----------

#[allow(clippy::too_many_arguments)]
pub(crate) fn add<P: JsonSchema, R: JsonSchema>(
    r: &mut Registry,
    id: &'static str,
    title: &'static str,
    description: &'static str,
    tier: SafetyTier,
    undoable: bool,
    requirements: &'static [&'static str],
    handler: Handler,
) {
    r.register(
        CommandSpec {
            executor: crate::Executor::Document,
            id,
            title,
            description,
            params_schema: schema_of::<P>(),
            result_schema: schema_of::<R>(),
            undoable,
            requires_device: false,
            safety_tier: tier,
            shortcut: None,
            requirements,
        },
        handler,
    );
}

pub(crate) fn register_all(r: &mut Registry) {
    use SafetyTier::*;
    add::<PolygonParams, IdResult>(r, "shape.polygon", "Wielokąt", "Wielokąt foremny (3 = trójkąt, 5 = pięciokąt, 8 = ośmiokąt); parametry zachowane do konwersji.", Design, true, &["REQ-02-008", "REQ-02-011", "REQ-02-033", "REQ-04-007"], shape_polygon);
    add::<StarParams, IdResult>(
        r,
        "shape.star",
        "Gwiazda",
        "Gwiazda; z r_middle — podwójna gwiazda.",
        Design,
        true,
        &["REQ-02-009", "REQ-02-010", "REQ-04-008"],
        shape_star,
    );
    add::<ArcParams, IdResult>(
        r,
        "shape.arc",
        "Łuk",
        "Łuk: środek + promień + kąty albo przez 3 punkty (through).",
        Design,
        true,
        &["REQ-02-003", "REQ-04-003", "REQ-04-004"],
        shape_arc,
    );
    add::<BooleanParams, IdsResult>(r, "path.boolean", "Operacja logiczna", "Suma, różnica (pierwszy minus reszta), iloczyn, XOR albo wycięcie kształtów; oryginały zastępowane wynikiem.", Design, true, &["REQ-04-064", "REQ-04-065", "REQ-04-066", "REQ-04-067", "REQ-04-068", "REQ-04-070"], path_boolean);
    add::<OffsetParams, IdsResult>(r, "path.offset", "Offset", "Offset kształtów zamkniętych na zewnątrz, do wewnątrz lub w obie strony; narożniki ostre, zaokrąglone lub ścięte.", Design, true, &["REQ-04-057", "REQ-04-058", "REQ-04-059", "REQ-04-060", "REQ-04-061", "REQ-04-062", "REQ-04-063"], path_offset);
    add::<Ids, IdsResult>(
        r,
        "path.convert",
        "Konwertuj do ścieżki",
        "Zamienia kształt parametryczny na ścieżkę (wypieka transformację).",
        Design,
        true,
        &["REQ-04-036"],
        path_convert,
    );
    add::<Ids, IdsResult>(
        r,
        "path.reverse",
        "Odwróć kierunek",
        "Odwraca kierunek ścieżek.",
        Design,
        true,
        &["REQ-04-031", "REQ-02-031"],
        path_reverse,
    );
    add::<NodeParams, IdResult>(
        r,
        "path.set_start",
        "Punkt startowy",
        "Ustawia punkt startowy zamkniętego kształtu na węźle.",
        Design,
        true,
        &["REQ-04-030", "REQ-02-031"],
        path_set_start,
    );
    add::<Ids, IdsResult>(
        r,
        "path.break_apart",
        "Rozbij",
        "Rozbija ścieżki wielokonturowe na osobne obiekty.",
        Design,
        true,
        &["REQ-04-041"],
        path_break_apart,
    );
    add::<ToleranceParams, CloseResult>(
        r,
        "path.close",
        "Zamknij ścieżki",
        "Zamyka otwarte ścieżki, których końce leżą w tolerancji.",
        Design,
        true,
        &["REQ-04-038"],
        path_close,
    );
    add::<ToleranceParams, IdsResult>(
        r,
        "path.join",
        "Połącz ścieżki",
        "Łączy otwarte ścieżki o końcach w tolerancji w jedną.",
        Design,
        true,
        &["REQ-04-037"],
        path_join,
    );
    add::<Ids, IdsResult>(r, "path.remove_duplicates", "Usuń duplikaty", "Usuwa obiekty o identycznej geometrii (niezależnie od kierunku i startu); zwraca usunięte.", Design, true, &["REQ-04-039"], path_remove_duplicates);
    add::<ReduceParams, ReduceResult>(
        r,
        "path.reduce_nodes",
        "Redukcja węzłów",
        "Upraszcza ścieżki z zadaną tolerancją odchylenia.",
        Design,
        true,
        &["REQ-04-042"],
        path_reduce_nodes,
    );
    add::<Ids, IdResult>(
        r,
        "path.convex_hull",
        "Otoczka wypukła",
        "Tworzy otoczkę wypukłą zaznaczenia jako nowy obiekt.",
        Design,
        true,
        &["REQ-04-052"],
        path_convex_hull,
    );
    add::<ChamferParams, IdResult>(
        r,
        "path.chamfer",
        "Fazowanie narożnika",
        "Fazuje narożnik w węźle (kształt jest konwertowany do ścieżki).",
        Design,
        true,
        &["REQ-04-053"],
        path_chamfer,
    );
    add::<FilletParams, IdResult>(
        r,
        "path.fillet",
        "Zaokrąglenie narożnika",
        "Zaokrągla narożnik w węźle łukiem o zadanym promieniu.",
        Design,
        true,
        &["REQ-04-054"],
        path_fillet,
    );
    add::<AlignParams, IdsResult>(
        r,
        "object.align",
        "Wyrównaj",
        "Wyrównuje do zaznaczenia, pierwszego, ostatniego obiektu lub obszaru roboczego.",
        Design,
        true,
        &["REQ-04-086", "REQ-04-087", "REQ-04-088"],
        object_align,
    );
    add::<DistributeParams, IdsResult>(
        r,
        "object.distribute",
        "Rozłóż",
        "Rozkłada obiekty równomiernie (środki lub przerwy).",
        Design,
        true,
        &["REQ-04-090"],
        object_distribute,
    );
    add::<OrderParams, IdsResult>(
        r,
        "object.order",
        "Kolejność rysowania",
        "Na wierzch, na spód, o jeden wyżej lub niżej.",
        Design,
        true,
        &["REQ-04-093"],
        object_order,
    );
    add::<EqualizeParams, IdsResult>(
        r,
        "object.equalize",
        "Zrównaj rozmiar",
        "Ustawia szerokość/wysokość jak pierwszego obiektu.",
        Design,
        true,
        &["REQ-04-080", "REQ-04-081"],
        object_equalize,
    );
    add::<GridParams, IdsResult>(
        r,
        "array.grid",
        "Tablica prostokątna",
        "Kopie w siatce kolumny × wiersze, z opcjonalnym przesunięciem co drugiego wiersza.",
        Design,
        true,
        &["REQ-04-071", "REQ-04-072"],
        array_grid,
    );
    add::<CircularParams, IdsResult>(
        r,
        "array.circular",
        "Tablica kołowa",
        "Kopie rozłożone na okręgu wokół środka.",
        Design,
        true,
        &["REQ-04-077"],
        array_circular,
    );
    add::<DistanceParams, DistanceResult>(
        r,
        "measure.distance",
        "Pomiar odległości",
        "Odległość, składowe i kierunek między dwoma punktami.",
        Read,
        false,
        &["REQ-04-011"],
        measure_distance,
    );
    add::<AngleParams, AngleResult>(
        r,
        "measure.angle",
        "Pomiar kąta",
        "Kąt między ramionami vertex→a i vertex→b.",
        Read,
        false,
        &["REQ-04-012"],
        measure_angle,
    );
    add::<LayerAddParams, Layer>(
        r,
        "layer.add",
        "Nowy kolor palety",
        "Dodaje warstwę z własnym kolorem (indeksy od 32).",
        Design,
        true,
        &["REQ-02-039"],
        layer_add,
    );
}
