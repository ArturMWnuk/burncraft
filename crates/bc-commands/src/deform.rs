//! Przycinanie, gumka i wypaczanie (M1.4b2b, REQ-04-021, 034, 045…047).
//! Współrzędne światowe; wynik zastępuje geometrię obiektu (jeden krok historii).

use bc_doc::ObjectId;
use bc_geom::cut;
use bc_geom::{Point, Rect};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::Value;

use crate::builtin::{finite, positive, world_bbox, IdResult, IdsResult};
use crate::edit::{add, geom, invalid};
use crate::nodes::{load_path, store_path};
use crate::{ok, parse, CmdError, Registry, SafetyTier, Session};

fn pt([x, y]: [f64; 2]) -> Result<Point, CmdError> {
    Ok(Point::new(finite("x", x)?, finite("y", y)?))
}

/// Widoczne liście dokumentu (bez obiektów pomocniczych) we współrzędnych świata.
fn visible_leaves(s: &Session) -> Result<Vec<(ObjectId, bc_geom::BezPath)>, CmdError> {
    Ok(s.doc
        .world_paths()?
        .into_iter()
        .filter(|(_, id, _)| s.doc.find(*id).is_some_and(|o| !o.helper))
        .map(|(_, id, p)| (id, p))
        .collect())
}

// ---------- path.trim ----------

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct TrimParams {
    /// Przycinany obiekt.
    id: ObjectId,
    /// Punkt [x, y] na usuwanym fragmencie (mm).
    at: [f64; 2],
}

fn path_trim(s: &mut Session, p: Value) -> Result<Value, CmdError> {
    let p: TrimParams = parse(p)?;
    let at = pt(p.at)?;
    let cutters: Vec<_> = visible_leaves(s)?
        .into_iter()
        .filter(|(id, _)| *id != p.id)
        .map(|(_, path)| path)
        .collect();
    let target = load_path(s, p.id)?;
    let out = cut::trim(&target, &cutters, at).map_err(geom)?;
    store_path(s, p.id, out)?;
    ok(IdResult { id: p.id })
}

// ---------- path.erase ----------

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct EraseParams {
    /// Obiekty do wymazywania; domyślnie wszystkie widoczne, odblokowane pod pociągnięciem.
    #[serde(default)]
    ids: Option<Vec<ObjectId>>,
    /// Pociągnięcie gumki: punkty [[x, y], …] w mm.
    points: Vec<[f64; 2]>,
    /// Szerokość gumki (mm).
    width: f64,
}

fn path_erase(s: &mut Session, p: Value) -> Result<Value, CmdError> {
    let p: EraseParams = parse(p)?;
    let width = positive("width", p.width)?;
    let stroke: Vec<Point> = p.points.iter().map(|q| pt(*q)).collect::<Result<_, _>>()?;
    if stroke.is_empty() {
        return Err(invalid("points: co najmniej jeden punkt"));
    }
    let r = width / 2.0;
    let area = stroke
        .iter()
        .fold(Rect::from_points(stroke[0], stroke[0]), |b, q| {
            b.union_pt(*q)
        })
        .inflate(r, r);
    let overlaps =
        |b: Rect| b.x0 <= area.x1 && b.x1 >= area.x0 && b.y0 <= area.y1 && b.y1 >= area.y0;
    let targets: Vec<ObjectId> = match &p.ids {
        Some(ids) => {
            s.targets(&Some(ids.clone()))?;
            ids.clone()
        }
        None => visible_leaves(s)?
            .into_iter()
            .filter(|(id, path)| {
                !s.doc.find(*id).is_some_and(|o| o.locked)
                    && bc_geom::bbox(path).is_some_and(overlaps)
            })
            .map(|(id, _)| id)
            .collect(),
    };
    let mut changed = Vec::new();
    for id in targets {
        // ocena bez zmiany dokumentu: kształt zamieniany na ścieżkę tylko, gdy coś wymazano
        let o = s
            .doc
            .find(id)
            .ok_or_else(|| CmdError::new("not_found", format!("nie znaleziono obiektu {id}")))?;
        let mut world = o.local_path()?;
        world.apply_affine(s.doc.parent_affine(id).unwrap_or_default() * o.affine());
        let out = cut::erase(&world, &stroke, width).map_err(geom)?;
        let same = (bc_geom::path_length(&out) - bc_geom::path_length(&world)).abs() < 1e-6
            && bc_geom::subpaths(&out).len() == bc_geom::subpaths(&world).len()
            && bc_geom::is_closed(&world) == bc_geom::is_closed(&out);
        if same {
            continue;
        }
        load_path(s, id)?; // blokada i zamiana na ścieżkę
        store_path(s, id, out)?;
        changed.push(id);
    }
    ok(IdsResult { ids: changed })
}

// ---------- path.warp ----------

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct WarpParams {
    /// Obiekty (domyślnie zaznaczenie).
    #[serde(default)]
    ids: Option<Vec<ObjectId>>,
    /// Obszar siatki źródłowej [x0, y0, x1, y1] (domyślnie ramka obiektów).
    #[serde(default)]
    bounds: Option<[f64; 4]>,
    /// Kolumny komórek (1 = wypaczanie 4-punktowe).
    cols: usize,
    /// Wiersze komórek (1 przy kilku kolumnach = obwiednia).
    rows: usize,
    /// Docelowe punkty siatki wierszami od dołu: (cols+1)·(rows+1) punktów [x, y].
    points: Vec<[f64; 2]>,
}

fn path_warp(s: &mut Session, p: Value) -> Result<Value, CmdError> {
    let p: WarpParams = parse(p)?;
    let ids = s.targets(&p.ids)?;
    let grid: Vec<Point> = p.points.iter().map(|q| pt(*q)).collect::<Result<_, _>>()?;
    let bounds = match p.bounds {
        Some([x0, y0, x1, y1]) => Rect::new(
            finite("x0", x0)?,
            finite("y0", y0)?,
            finite("x1", x1)?,
            finite("y1", y1)?,
        ),
        None => world_bbox(&s.doc, &ids)?.ok_or_else(|| invalid("obiekty nie mają geometrii"))?,
    };
    for id in &ids {
        let path = load_path(s, *id)?;
        let out = cut::mesh_warp(&path, bounds, p.cols, p.rows, &grid).map_err(geom)?;
        store_path(s, *id, out)?;
    }
    ok(IdsResult { ids })
}

pub(crate) fn register_all(r: &mut Registry) {
    use SafetyTier::*;
    add::<TrimParams, IdResult>(r, "path.trim", "Przytnij", "Usuwa fragment ścieżki między najbliższymi przecięciami z innymi obiektami (bez pomocniczych) wokół punktu `at`.", Design, true, &["REQ-04-034"], path_trim);
    add::<EraseParams, IdsResult>(r, "path.erase", "Gumka", "Wymazuje części ścieżek pod pociągnięciem o danej szerokości; wynik to polilinie otwarte, wymazany w całości obiekt znika.", Design, true, &["REQ-04-021"], path_erase);
    add::<WarpParams, IdsResult>(r, "path.warp", "Wypaczanie siatką", "Odwzorowanie biliniowe siatki cols×rows: 1×1 — wypaczanie 4-punktowe, większa siatka — deformacja siatką, 1 wiersz — obwiednia.", Design, true, &["REQ-04-045", "REQ-04-046", "REQ-04-047"], path_warp);
}
