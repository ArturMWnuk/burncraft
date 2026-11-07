//! Polecenia rdzenia M1 domykające etap: okręgi styczne do trzech okręgów,
//! usuwanie nakładających się odcinków, wygładzanie, łuki z odcinków, asystent
//! booleanów, kopiowanie wzdłuż ścieżki, wpusty i czopy, dokowanie, Quick Nest.

use bc_doc::{Kind, ObjectId};
use bc_geom::shaping::{self, Tangency};
use bc_geom::{ops, Affine, BezPath, Point, Rect, Vec2};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::builtin::{
    default_layer, ensure_in_range, finite, positive, top_most, world_bbox, IdsResult,
};
use crate::edit::{
    add, apply_world, bbox_of, boolean_inputs, boolean_result, check_unlocked, copies, ensure_path,
    geom, invalid, set_path, world_path, BoolKind,
};
use crate::{ok, parse, CmdError, Registry, SafetyTier, Session};

// ---------- REQ-04-019: okręgi styczne ----------

#[derive(Deserialize, JsonSchema, Default, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
enum TangentFilter {
    /// Wszystkie rozwiązania (do 8).
    #[default]
    All,
    /// Tylko okrąg styczny zewnętrznie do wszystkich trzech.
    Outer,
    /// Tylko okrąg obejmujący wszystkie trzy.
    Inner,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct TangentParams {
    /// Trzy obiekty-okręgi (elipsy o równych półosiach, skalowane równomiernie).
    #[serde(default)]
    ids: Option<Vec<ObjectId>>,
    /// Zamiast ids: trzy okręgi [cx, cy, r] w mm.
    #[serde(default)]
    circles: Option<Vec<[f64; 3]>>,
    #[serde(default)]
    filter: TangentFilter,
    /// Warstwa wyników (domyślnie warstwa pierwszego okręgu).
    #[serde(default)]
    layer: Option<u8>,
}

#[derive(Serialize, JsonSchema)]
struct TangentInfo {
    cx: f64,
    cy: f64,
    r: f64,
    /// Styczność z kolejnymi okręgami: external | encloses (wynik obejmuje okrąg) | inside.
    tangency: Vec<&'static str>,
}

#[derive(Serialize, JsonSchema)]
struct TangentResult {
    ids: Vec<ObjectId>,
    circles: Vec<TangentInfo>,
}

/// Okrąg świata z obiektu-elipsy o równych półosiach i transformacji podobieństwa.
fn circle_of(s: &Session, id: ObjectId) -> Result<(Point, f64), CmdError> {
    let o = s.doc.find(id).expect("sprawdzone");
    let not_circle = || invalid(format!("obiekt {id} nie jest okręgiem"));
    let Kind::Ellipse { cx, cy, rx, ry } = o.kind else {
        return Err(not_circle());
    };
    if (rx - ry).abs() > 1e-9 * rx.abs().max(1.0) {
        return Err(not_circle());
    }
    let t = s.doc.parent_affine(id).unwrap_or(Affine::IDENTITY) * o.affine();
    let [a, b, c, d, _, _] = t.as_coeffs();
    let eps = 1e-9 * (a.abs() + b.abs() + c.abs() + d.abs()).max(1.0);
    let similar = ((a - d).abs() < eps && (b + c).abs() < eps)
        || ((a + d).abs() < eps && (b - c).abs() < eps);
    if !similar {
        return Err(not_circle());
    }
    Ok((
        t * Point::new(cx, cy),
        rx.abs() * (a * d - b * c).abs().sqrt(),
    ))
}

fn shape_tangent_circles(s: &mut Session, p: Value) -> Result<Value, CmdError> {
    let p: TangentParams = parse(p)?;
    let (given, layer) = match (&p.circles, &p.ids) {
        (Some(cs), None) => {
            let v = cs
                .iter()
                .map(|c| {
                    Ok((
                        Point::new(finite("cx", c[0])?, finite("cy", c[1])?),
                        positive("r", c[2])?,
                    ))
                })
                .collect::<Result<Vec<_>, CmdError>>()?;
            (v, default_layer())
        }
        (None, ids) => {
            let ids = s.targets(ids)?;
            let v = ids
                .iter()
                .map(|id| circle_of(s, *id))
                .collect::<Result<Vec<_>, _>>()?;
            (v, s.doc.find(ids[0]).map(|o| o.layer).unwrap_or(0))
        }
        _ => return Err(invalid("podaj albo ids, albo circles")),
    };
    let given: [(Point, f64); 3] = given
        .try_into()
        .map_err(|_| invalid("wymagane dokładnie trzy okręgi"))?;
    let layer = p.layer.unwrap_or(layer);
    let sol: Vec<_> = shaping::tangent_circles(given)
        .into_iter()
        .filter(|c| match p.filter {
            TangentFilter::All => true,
            TangentFilter::Outer => c.tangency.iter().all(|t| *t == Tangency::External),
            TangentFilter::Inner => c.tangency.iter().all(|t| *t == Tangency::Encloses),
        })
        .collect();
    if sol.is_empty() {
        return Err(invalid("brak okręgów stycznych spełniających warunki"));
    }
    let mut ids = Vec::new();
    let mut circles = Vec::new();
    for c in sol {
        let id = s
            .doc
            .add(
                layer,
                Kind::Ellipse {
                    cx: c.center.x,
                    cy: c.center.y,
                    rx: c.r,
                    ry: c.r,
                },
            )
            .map_err(|e| invalid(e.to_string()))?;
        ids.push(id);
        circles.push(TangentInfo {
            cx: c.center.x,
            cy: c.center.y,
            r: c.r,
            tangency: c
                .tangency
                .iter()
                .map(|t| match t {
                    Tangency::External => "external",
                    Tangency::Encloses => "encloses",
                    Tangency::Inside => "inside",
                })
                .collect(),
        });
    }
    ensure_in_range(&s.doc, &ids)?;
    s.selection = ids.clone();
    ok(TangentResult { ids, circles })
}

// ---------- REQ-04-040: nakładające się odcinki ----------

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct OverlapParams {
    #[serde(default)]
    ids: Option<Vec<ObjectId>>,
    /// Tolerancja współliniowości w mm (domyślnie 0,01).
    #[serde(default = "default_fine_tol")]
    tolerance: f64,
}

fn default_fine_tol() -> f64 {
    0.01
}

#[derive(Serialize, JsonSchema)]
struct OverlapResult {
    /// Obiekty zmienione (przycięte).
    ids: Vec<ObjectId>,
    /// Obiekty usunięte, bo w całości pokrywały wcześniejsze.
    removed: Vec<ObjectId>,
    /// Łączna usunięta długość (mm).
    removed_length: f64,
}

/// Zastępuje geometrię obiektu ścieżką podaną we współrzędnych świata.
fn set_world_path(s: &mut Session, id: ObjectId, world: &BezPath) -> Result<(), CmdError> {
    ensure_path(&mut s.doc, id)?;
    let mut local = world.clone();
    local.apply_affine(
        s.doc
            .parent_affine(id)
            .unwrap_or(Affine::IDENTITY)
            .inverse(),
    );
    set_path(&mut s.doc, id, &local);
    Ok(())
}

fn path_remove_overlaps(s: &mut Session, p: Value) -> Result<Value, CmdError> {
    let p: OverlapParams = parse(p)?;
    let tol = positive("tolerance", p.tolerance)?;
    let ids = top_most(&s.doc, s.targets(&p.ids)?);
    let paths: Vec<BezPath> = ids
        .iter()
        .map(|id| world_path(&s.doc, *id))
        .collect::<Result<_, _>>()?;
    let (out, removed_length) = shaping::remove_overlaps(&paths, tol);
    let (mut changed, mut removed) = (Vec::new(), Vec::new());
    for (i, ((id, before), after)) in ids.iter().zip(&paths).zip(out).enumerate() {
        s.tick(i, ids.len())?;
        if after.elements().is_empty() && !before.elements().is_empty() {
            check_unlocked(&s.doc, &[*id], false)?;
            s.doc.remove(*id)?;
            removed.push(*id);
        } else if after != *before {
            set_world_path(s, *id, &after)?;
            changed.push(*id);
        }
    }
    s.prune_selection();
    ok(OverlapResult {
        ids: changed,
        removed,
        removed_length,
    })
}

// ---------- REQ-04-043: wygładzanie ----------

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct SmoothParams {
    #[serde(default)]
    ids: Option<Vec<ObjectId>>,
    /// Długość uchwytów (0–1], 1 = pełne wygładzenie.
    #[serde(default = "default_one")]
    smoothness: f64,
    /// Węzły, w których kierunek zmienia się o więcej stopni, zostają narożnikami (0–180].
    #[serde(default = "default_corner")]
    corner_angle: f64,
}

fn default_one() -> f64 {
    1.0
}

fn default_arcs_tol() -> f64 {
    0.05
}

fn default_corner() -> f64 {
    60.0
}

fn path_smooth(s: &mut Session, p: Value) -> Result<Value, CmdError> {
    let p: SmoothParams = parse(p)?;
    if !(p.smoothness > 0.0 && p.smoothness <= 1.0) {
        return Err(invalid("smoothness: (0, 1]"));
    }
    if !(p.corner_angle > 0.0 && p.corner_angle <= 180.0) {
        return Err(invalid("corner_angle: (0, 180]"));
    }
    let ids = s.targets(&p.ids)?;
    for (i, id) in ids.iter().enumerate() {
        s.tick(i, ids.len())?;
        let path = ensure_path(&mut s.doc, *id)?;
        set_path(
            &mut s.doc,
            *id,
            &shaping::smooth(&path, p.smoothness, p.corner_angle),
        );
    }
    ok(IdsResult { ids })
}

// ---------- REQ-04-044: łuki z odcinków ----------

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct ArcsParams {
    #[serde(default)]
    ids: Option<Vec<ObjectId>>,
    /// Maksymalne odchylenie łuku od odcinków w mm (domyślnie 0,05).
    #[serde(default = "default_arcs_tol")]
    tolerance: f64,
}

#[derive(Serialize, JsonSchema)]
struct ArcsResult {
    ids: Vec<ObjectId>,
    /// Liczba utworzonych łuków.
    arcs: usize,
}

fn path_lines_to_arcs(s: &mut Session, p: Value) -> Result<Value, CmdError> {
    let p: ArcsParams = parse(p)?;
    let tol = positive("tolerance", p.tolerance)?;
    let ids = s.targets(&p.ids)?;
    let mut arcs = 0;
    for (i, id) in ids.iter().enumerate() {
        s.tick(i, ids.len())?;
        let path = ensure_path(&mut s.doc, *id)?;
        let (q, n) = shaping::lines_to_arcs(&path, tol);
        if n > 0 {
            set_path(&mut s.doc, *id, &q);
        }
        arcs += n;
    }
    ok(ArcsResult { ids, arcs })
}

// ---------- REQ-04-069: asystent booleanów ----------

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct PreviewParams {
    /// Obiekty (≥ 2, zamknięte); domyślnie zaznaczenie.
    #[serde(default)]
    ids: Option<Vec<ObjectId>>,
}

#[derive(Serialize, JsonSchema)]
struct BoolOption {
    /// union | difference | intersection | xor | cut
    op: &'static str,
    /// Kolejność obiektów (dla difference pierwszy = baza).
    ids: Vec<ObjectId>,
    /// Pole wyniku (mm²).
    area: f64,
    /// Liczba kształtów wynikowych.
    pieces: usize,
    /// Wynik jako ścieżka SVG (mm, Y w górę) do podglądu.
    d: String,
    /// Polecenie wykonujące tę opcję: {id, params}.
    apply: Value,
}

#[derive(Serialize, JsonSchema)]
struct PreviewResult {
    options: Vec<BoolOption>,
}

fn path_boolean_preview(s: &mut Session, p: Value) -> Result<Value, CmdError> {
    let p: PreviewParams = parse(p)?;
    let ids = top_most(&s.doc, s.targets(&p.ids)?);
    let paths = boolean_inputs(&s.doc, &ids)?;
    let mut variants: Vec<(&'static str, BoolKind, Vec<usize>)> =
        vec![("union", BoolKind::Union, (0..ids.len()).collect())];
    for base in 0..ids.len() {
        let mut order = vec![base];
        order.extend((0..ids.len()).filter(|i| *i != base));
        variants.push(("difference", BoolKind::Difference, order));
    }
    for (name, op) in [
        ("intersection", BoolKind::Intersection),
        ("xor", BoolKind::Xor),
        ("cut", BoolKind::Cut),
    ] {
        variants.push((name, op, (0..ids.len()).collect()));
    }
    let mut options = Vec::new();
    for (name, op, order) in variants {
        let input: Vec<BezPath> = order.iter().map(|i| paths[*i].clone()).collect();
        let res = boolean_result(&input, op)?;
        let ordered: Vec<ObjectId> = order.iter().map(|i| ids[*i]).collect();
        let mut d = BezPath::new();
        for r in &res {
            for el in r.elements() {
                d.push(*el);
            }
        }
        options.push(BoolOption {
            op: name,
            ids: ordered.clone(),
            area: ops::area(&res),
            pieces: res.len(),
            d: d.to_svg(),
            apply: json!({"id": "path.boolean", "params": {"ids": ordered, "op": name}}),
        });
    }
    ok(PreviewResult { options })
}

// ---------- REQ-04-078: kopiowanie wzdłuż ścieżki ----------

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct AlongParams {
    /// Kopiowane obiekty (domyślnie zaznaczenie).
    #[serde(default)]
    ids: Option<Vec<ObjectId>>,
    /// Id obiektu-ścieżki prowadzącej.
    path: ObjectId,
    /// Liczba kopii (ścieżka otwarta: od początku do końca; zamknięta: równo na obwodzie).
    #[serde(default)]
    count: Option<u32>,
    /// Zamiast count: odstęp kopii wzdłuż ścieżki (mm).
    #[serde(default)]
    spacing: Option<f64>,
    /// Obracaj kopie zgodnie ze styczną (0° = kierunek +X oryginału).
    #[serde(default = "default_true")]
    rotate: bool,
}

fn default_true() -> bool {
    true
}

fn array_along_path(s: &mut Session, p: Value) -> Result<Value, CmdError> {
    let p: AlongParams = parse(p)?;
    s.targets(&Some(vec![p.path]))?;
    let ids = top_most(&s.doc, s.targets(&p.ids)?);
    if ids.contains(&p.path) {
        return Err(invalid("ścieżka prowadząca nie może być kopiowana"));
    }
    let guide = world_path(&s.doc, p.path)?;
    let len = shaping::length(&guide);
    if len <= 0.0 {
        return Err(invalid("ścieżka prowadząca ma zerową długość"));
    }
    let closed = bc_geom::is_closed(&guide);
    let positions: Vec<f64> = match (p.count, p.spacing) {
        (Some(n), None) => {
            if !(1..=100_000).contains(&n) {
                return Err(invalid("count: 1–100000"));
            }
            let n = n as usize;
            (0..n)
                .map(|i| {
                    if closed {
                        len * i as f64 / n as f64
                    } else if n == 1 {
                        0.0
                    } else {
                        len * i as f64 / (n - 1) as f64
                    }
                })
                .collect()
        }
        (None, Some(d)) => {
            let d = positive("spacing", d)?;
            if len / d > 100_000.0 {
                return Err(invalid("spacing: ponad 100000 kopii"));
            }
            let mut v = Vec::new();
            let mut x = 0.0;
            while x <= len + 1e-9 && !(closed && x >= len - 1e-9 && !v.is_empty()) {
                v.push(x);
                x += d;
            }
            v
        }
        _ => return Err(invalid("podaj albo count, albo spacing")),
    };
    let anchor = world_bbox(&s.doc, &ids)?
        .ok_or_else(|| invalid("kopiowane obiekty nie mają geometrii"))?
        .center()
        .to_vec2();
    let ts: Vec<Affine> = positions
        .iter()
        .filter_map(|x| shaping::point_at_length(&guide, *x))
        .map(|(pt, ang)| {
            let rot = if p.rotate { ang } else { 0.0 };
            Affine::translate(pt.to_vec2()) * Affine::rotate(rot) * Affine::translate(-anchor)
        })
        .collect();
    let new = copies(s, &ids, &ts)?;
    crate::builtin::ensure_in_range_tick(s, &new)?;
    ok(IdsResult { ids: new })
}

// ---------- REQ-04-082: wpusty i czopy ----------

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct SlotParams {
    #[serde(default)]
    ids: Option<Vec<ObjectId>>,
    /// Dotychczasowa grubość materiału (szerokość wpustów / długość czopów), mm.
    from: f64,
    /// Nowa grubość materiału, mm.
    to: f64,
    /// Tolerancja materiału dodawana do szerokości wpustów (może być ujemna, np. na kerf).
    #[serde(default)]
    tolerance: f64,
    /// Dopuszczalna różnica wymiaru od `from` przy rozpoznawaniu (mm, domyślnie 0,05).
    #[serde(default = "default_match")]
    match_tolerance: f64,
}

fn default_match() -> f64 {
    0.05
}

#[derive(Serialize, JsonSchema)]
struct SlotResult {
    ids: Vec<ObjectId>,
    /// Zmienione wpusty (nacięcia krawędzi i otwory).
    slots: usize,
    /// Zmienione czopy.
    tabs: usize,
}

fn path_resize_slots(s: &mut Session, p: Value) -> Result<Value, CmdError> {
    let p: SlotParams = parse(p)?;
    let from = positive("from", p.from)?;
    let to = positive("to", p.to)?;
    let tol = finite("tolerance", p.tolerance)?;
    let mt = finite("match_tolerance", p.match_tolerance)?;
    let ids = s.targets(&p.ids)?;
    let (mut slots, mut tabs) = (0, 0);
    for (i, id) in ids.iter().enumerate() {
        s.tick(i, ids.len())?;
        let path = ensure_path(&mut s.doc, *id)?;
        let r = shaping::resize_slots(&path, from, to, tol, mt).map_err(geom)?;
        if r.slots + r.tabs > 0 {
            set_path(&mut s.doc, *id, &r.path);
        }
        slots += r.slots;
        tabs += r.tabs;
    }
    ensure_in_range(&s.doc, &ids)?;
    ok(SlotResult { ids, slots, tabs })
}

// ---------- REQ-04-091: dokowanie ----------

#[derive(Deserialize, JsonSchema, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
enum DockDir {
    Left,
    Right,
    /// W stronę +Y.
    Up,
    /// W stronę −Y.
    Down,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct DockParams {
    #[serde(default)]
    ids: Option<Vec<ObjectId>>,
    direction: DockDir,
    /// Odstęp od obiektu, o który opiera się dokowany (mm).
    #[serde(default)]
    gap: f64,
}

fn object_dock(s: &mut Session, p: Value) -> Result<Value, CmdError> {
    let p: DockParams = parse(p)?;
    let gap = finite("gap", p.gap)?;
    let ids = top_most(&s.doc, s.targets(&p.ids)?);
    check_unlocked(&s.doc, &ids, false)?;
    // przeszkody: widoczne obiekty najwyższego poziomu poza przesuwanymi i ich przodkami
    let mut skip = ids.clone();
    for id in &ids {
        let mut cur = s.doc.parent_of(*id).flatten();
        while let Some(a) = cur {
            skip.push(a);
            cur = s.doc.parent_of(a).flatten();
        }
    }
    let mut obstacles: Vec<Rect> = Vec::new();
    for o in s
        .doc
        .objects
        .iter()
        .filter(|o| o.visible && !o.helper && !skip.contains(&o.id))
    {
        if let Some(b) = world_bbox(&s.doc, &[o.id])? {
            obstacles.push(b);
        }
    }
    let mut items: Vec<(ObjectId, Rect)> = ids
        .iter()
        .map(|id| bbox_of(&s.doc, *id).map(|b| (*id, b)))
        .collect::<Result<_, _>>()?;
    let key = |b: &Rect| match p.direction {
        DockDir::Left => b.x0,
        DockDir::Right => -b.x1,
        DockDir::Down => b.y0,
        DockDir::Up => -b.y1,
    };
    items.sort_by(|a, b| key(&a.1).partial_cmp(&key(&b.1)).unwrap());
    let (ww, wh) = (s.doc.workspace.width, s.doc.workspace.height);
    let eps = 1e-9;
    for (id, b) in items {
        let horizontal = matches!(p.direction, DockDir::Left | DockDir::Right);
        let in_band = |o: &Rect| {
            if horizontal {
                o.y0 < b.y1 - eps && o.y1 > b.y0 + eps
            } else {
                o.x0 < b.x1 - eps && o.x1 > b.x0 + eps
            }
        };
        let band = obstacles.iter().filter(|o| in_band(o));
        let d = match p.direction {
            DockDir::Left => {
                let stop = band
                    .filter(|o| o.x1 <= b.x0 + eps)
                    .map(|o| o.x1 + gap)
                    .fold(f64::NAN, f64::max);
                Vec2::new((if stop.is_nan() { 0.0 } else { stop }) - b.x0, 0.0)
            }
            DockDir::Right => {
                let stop = band
                    .filter(|o| o.x0 >= b.x1 - eps)
                    .map(|o| o.x0 - gap)
                    .fold(f64::NAN, f64::min);
                Vec2::new((if stop.is_nan() { ww } else { stop }) - b.x1, 0.0)
            }
            DockDir::Down => {
                let stop = band
                    .filter(|o| o.y1 <= b.y0 + eps)
                    .map(|o| o.y1 + gap)
                    .fold(f64::NAN, f64::max);
                Vec2::new(0.0, (if stop.is_nan() { 0.0 } else { stop }) - b.y0)
            }
            DockDir::Up => {
                let stop = band
                    .filter(|o| o.y0 >= b.y1 - eps)
                    .map(|o| o.y0 - gap)
                    .fold(f64::NAN, f64::min);
                Vec2::new(0.0, (if stop.is_nan() { wh } else { stop }) - b.y1)
            }
        };
        apply_world(&mut s.doc, id, Affine::translate(d));
        obstacles.push(b + d);
    }
    ok(IdsResult { ids })
}

// ---------- REQ-04-095: Quick Nest ----------

#[derive(Deserialize, JsonSchema, Clone, Copy)]
#[serde(deny_unknown_fields)]
struct Sheet {
    /// Lewy dolny narożnik arkusza (mm).
    x: f64,
    y: f64,
    w: f64,
    h: f64,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct NestParams {
    #[serde(default)]
    ids: Option<Vec<ObjectId>>,
    /// Arkusz; domyślnie cały obszar roboczy.
    #[serde(default)]
    sheet: Option<Sheet>,
    /// Odstęp między częściami (mm).
    #[serde(default)]
    spacing: f64,
    /// Pozwól obracać części o 90°.
    #[serde(default = "default_true")]
    rotate: bool,
}

#[derive(Serialize, JsonSchema)]
struct NestResult {
    /// Rozmieszczone obiekty.
    ids: Vec<ObjectId>,
    /// Obiekty, które się nie zmieściły (pozostają na miejscu).
    unplaced: Vec<ObjectId>,
    /// Część pola arkusza zajęta przez obrysy rozmieszczonych obiektów (0–1).
    utilization: f64,
}

fn object_quick_nest(s: &mut Session, p: Value) -> Result<Value, CmdError> {
    let p: NestParams = parse(p)?;
    let sheet = p.sheet.unwrap_or(Sheet {
        x: 0.0,
        y: 0.0,
        w: s.doc.workspace.width,
        h: s.doc.workspace.height,
    });
    let (sx, sy) = (finite("sheet.x", sheet.x)?, finite("sheet.y", sheet.y)?);
    let (sw, sh) = (positive("sheet.w", sheet.w)?, positive("sheet.h", sheet.h)?);
    let spacing = finite("spacing", p.spacing)?;
    if spacing < 0.0 {
        return Err(invalid("spacing musi być ≥ 0"));
    }
    let ids = top_most(&s.doc, s.targets(&p.ids)?);
    check_unlocked(&s.doc, &ids, false)?;
    let boxes: Vec<Rect> = ids
        .iter()
        .map(|id| bbox_of(&s.doc, *id))
        .collect::<Result<_, _>>()?;
    let sizes: Vec<(f64, f64)> = boxes.iter().map(|b| (b.width(), b.height())).collect();
    let places = shaping::pack_rects(&sizes, sw, sh, spacing, p.rotate);
    let (mut placed, mut unplaced, mut used) = (Vec::new(), Vec::new(), 0.0);
    for (i, ((id, b), pl)) in ids.iter().zip(&boxes).zip(places).enumerate() {
        s.tick(i, ids.len())?;
        let Some(pl) = pl else {
            unplaced.push(*id);
            continue;
        };
        let c = b.center().to_vec2();
        let (w, h, rot) = if pl.rotated {
            (
                b.height(),
                b.width(),
                Affine::translate(c)
                    * Affine::rotate(std::f64::consts::FRAC_PI_2)
                    * Affine::translate(-c),
            )
        } else {
            (b.width(), b.height(), Affine::IDENTITY)
        };
        let min = Vec2::new(c.x - w / 2.0, c.y - h / 2.0);
        let target = Vec2::new(sx + pl.x, sy + pl.y);
        apply_world(&mut s.doc, *id, Affine::translate(target - min) * rot);
        used += w * h;
        placed.push(*id);
    }
    ok(NestResult {
        ids: placed,
        unplaced,
        utilization: used / (sw * sh),
    })
}

// ---------- rejestracja ----------

pub(crate) fn register_all(r: &mut Registry) {
    use SafetyTier::*;
    add::<TangentParams, TangentResult>(r, "shape.tangent_circles", "Okręgi styczne", "Tworzy okręgi styczne do trzech okręgów (problem Apoloniusza, do 8 rozwiązań); filtr outer/inner.", Design, true, &["REQ-04-019"], shape_tangent_circles);
    add::<OverlapParams, OverlapResult>(r, "path.remove_overlaps", "Usuń nakładające się odcinki", "Przycina odcinki pokrywające się z wcześniejszymi (w kolejności obiektów), żeby każda linia była cięta raz.", Design, true, &["REQ-04-040"], path_remove_overlaps);
    add::<SmoothParams, IdsResult>(
        r,
        "path.smooth",
        "Wygładź",
        "Zamienia odcinki na gładkie krzywe przez te same węzły; ostre narożniki zostają.",
        Design,
        true,
        &["REQ-04-043"],
        path_smooth,
    );
    add::<ArcsParams, ArcsResult>(
        r,
        "path.lines_to_arcs",
        "Łuki z odcinków",
        "Zastępuje ciągi odcinków leżących na okręgu łukami (w tolerancji).",
        Design,
        true,
        &["REQ-04-044"],
        path_lines_to_arcs,
    );
    add::<PreviewParams, PreviewResult>(r, "path.boolean_preview", "Asystent booleanów", "Podgląd wyników wszystkich operacji logicznych bez zmiany dokumentu; pole apply wykonuje wybraną.", Read, false, &["REQ-04-069"], path_boolean_preview);
    add::<AlongParams, IdsResult>(r, "array.along_path", "Kopiuj wzdłuż ścieżki", "Kopie rozłożone równomiernie wzdłuż ścieżki (liczba albo odstęp), opcjonalnie obrócone wg stycznej.", Design, true, &["REQ-04-078"], array_along_path);
    add::<SlotParams, SlotResult>(r, "path.resize_slots", "Wpusty i czopy", "Zmienia szerokość wpustów i długość czopów z grubości from na to (+ tolerancja materiału dla wpustów).", Design, true, &["REQ-04-082"], path_resize_slots);
    add::<DockParams, IdsResult>(r, "object.dock", "Dokuj", "Przesuwa obiekty w kierunku, aż oprą się o inny obiekt (obrys) albo krawędź obszaru roboczego.", Design, true, &["REQ-04-091"], object_dock);
    add::<NestParams, NestResult>(
        r,
        "object.quick_nest",
        "Quick Nest",
        "Rozmieszcza obrysy prostokątne obiektów na arkuszu (MaxRects, obrót 90°, odstęp).",
        Design,
        true,
        &["REQ-04-095"],
        object_quick_nest,
    );
}
