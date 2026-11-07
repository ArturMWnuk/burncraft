//! Wbudowane polecenia etapu M0 (dokument, kształty, obiekty, zaznaczenie,
//! warstwy, historia) oraz zaślepki poleceń urządzenia dla polityki bezpieczeństwa.

use bc_doc::{Document, Kind, LayerMode, Object, ObjectId};
use bc_geom::{Affine, Point, Vec2};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::inspect::{self, Detail, DocumentReport};
use crate::{ok, parse, schema_of, CmdError, CommandSpec, Handler, Registry, SafetyTier, Session};

#[allow(clippy::too_many_arguments)]
fn add<P: JsonSchema, R: JsonSchema>(
    r: &mut Registry,
    id: &'static str,
    title: &'static str,
    description: &'static str,
    tier: SafetyTier,
    undoable: bool,
    shortcut: Option<&'static str>,
    requirements: &'static [&'static str],
    handler: Handler,
) {
    let requires_device = id.starts_with("device.");
    r.register(
        CommandSpec {
            executor: crate::Executor::Document,
            id,
            title,
            description,
            params_schema: schema_of::<P>(),
            result_schema: schema_of::<R>(),
            undoable,
            requires_device,
            safety_tier: tier,
            shortcut,
            requirements,
        },
        handler,
    );
}

// ---------- typy wspólne ----------

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NoParams {}

#[derive(Serialize, JsonSchema)]
pub struct Empty {}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Ids {
    /// Id obiektów; brak = bieżące zaznaczenie.
    #[serde(default)]
    pub ids: Option<Vec<ObjectId>>,
}

#[derive(Serialize, JsonSchema)]
pub struct IdResult {
    /// Id utworzonego obiektu.
    pub id: ObjectId,
}

#[derive(Serialize, JsonSchema)]
pub struct IdsResult {
    pub ids: Vec<ObjectId>,
}

pub(crate) fn default_layer() -> u8 {
    0
}

// ---------- dokument i pliki ----------

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct NewDoc {
    /// Nazwa dokumentu.
    #[serde(default)]
    name: Option<String>,
    /// Szerokość obszaru roboczego w mm (domyślnie 400).
    #[serde(default)]
    width: Option<f64>,
    /// Wysokość obszaru roboczego w mm (domyślnie 400).
    #[serde(default)]
    height: Option<f64>,
}

fn document_new(s: &mut Session, p: Value) -> Result<Value, CmdError> {
    let p: NewDoc = parse(p)?;
    let mut d = Document::new(p.name.as_deref().unwrap_or("Bez tytułu"));
    if let Some(w) = p.width {
        d.workspace.width = positive("width", w)?;
    }
    if let Some(h) = p.height {
        d.workspace.height = positive("height", h)?;
    }
    crate::workbench::discard_autosave(s);
    *s = Session::new(d).with_machines_of(s);
    ok(Empty {})
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct InspectParams {
    /// summary (domyślnie) albo full.
    #[serde(default)]
    detail: Detail,
}

fn document_inspect(s: &mut Session, p: Value) -> Result<Value, CmdError> {
    let p: InspectParams = parse(p)?;
    ok(inspect::report(s, p.detail)?)
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct NotesParams {
    /// Treść notatek (Markdown).
    text: String,
}

fn document_set_notes(s: &mut Session, p: Value) -> Result<Value, CmdError> {
    let p: NotesParams = parse(p)?;
    s.doc.notes = p.text;
    ok(Empty {})
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct WorkspaceParams {
    /// mm
    width: f64,
    /// mm
    height: f64,
}

fn document_set_workspace(s: &mut Session, p: Value) -> Result<Value, CmdError> {
    let p: WorkspaceParams = parse(p)?;
    s.doc.workspace.width = positive("width", p.width)?;
    s.doc.workspace.height = positive("height", p.height)?;
    ok(Empty {})
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct OpenParams {
    /// Ścieżka pliku .bcraft, .bcraft-template albo .lbrn2.
    path: String,
}

#[derive(Serialize, JsonSchema)]
struct FileResult {
    path: String,
    objects: usize,
}

fn file_open(s: &mut Session, p: Value) -> Result<Value, CmdError> {
    let p: OpenParams = parse(p)?;
    let path = std::path::PathBuf::from(&p.path);
    if matches!(
        bc_io::Format::from_path(&path),
        Some(bc_io::Format::Lbrn2 | bc_io::Format::Lbrn)
    ) {
        return open_lbrn(s, &path, &p.path);
    }
    let (doc, extras) = bc_doc::bcraft::load(&path)?;
    let n = doc.all_objects().len();
    crate::workbench::discard_autosave(s);
    *s = Session::new(doc).with_machines_of(s);
    // szablon otwiera się jako nowy, niezapisany dokument (REQ-02-053)
    if !extras.template {
        s.path = Some(path);
    }
    ok(FileResult {
        path: p.path,
        objects: n,
    })
}

/// Projekt `.lbrn2`/`.lbrn` jako nowy dokument — bez ścieżki, więc zapis nie nadpisze oryginału.
fn open_lbrn(s: &mut Session, path: &std::path::Path, shown: &str) -> Result<Value, CmdError> {
    let data = std::fs::read(path)
        .map_err(|e| CmdError::new("failed", format!("{}: {e}", path.display())))?;
    let name = path
        .file_stem()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "projekt".into());
    let (doc, warnings) = if bc_io::Format::from_path(path) == Some(bc_io::Format::Lbrn) {
        bc_io::lbrn::lbrn_document_with_warnings(&data, &name)
    } else {
        bc_io::lbrn::lbrn2_document_with_warnings(&data, &name)
    }
    .map_err(crate::files::io_err)?;
    let n = doc.all_objects().len();
    crate::workbench::discard_autosave(s);
    *s = Session::new(doc).with_machines_of(s);
    ok(serde_json::json!({ "path": shown, "objects": n, "warnings": warnings }))
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct SaveParams {
    /// Ścieżka docelowa (.bcraft); brak = bieżący plik.
    #[serde(default)]
    path: Option<String>,
}

fn file_save(s: &mut Session, p: Value) -> Result<Value, CmdError> {
    let p: SaveParams = parse(p)?;
    let path = match (p.path, &s.path) {
        (Some(x), _) => std::path::PathBuf::from(x),
        (None, Some(cur)) => cur.clone(),
        (None, None) => {
            return Err(CmdError::new(
                "invalid_params",
                "dokument nie ma jeszcze pliku: podaj path",
            ))
        }
    };
    if path.extension().and_then(|e| e.to_str()) != Some("bcraft") {
        return Err(CmdError::new(
            "invalid_params",
            "plik musi mieć rozszerzenie .bcraft",
        ));
    }
    write_bcraft(s, &path, false)?;
    s.path = Some(path.clone());
    s.dirty = false;
    crate::workbench::discard_autosave(s);
    ok(FileResult {
        path: path.display().to_string(),
        objects: s.doc.all_objects().len(),
    })
}

/// Zapisuje dokument sesji jako `.bcraft` (lub szablon) z miniaturą.
pub(crate) fn write_bcraft(
    s: &Session,
    path: &std::path::Path,
    template: bool,
) -> Result<(), CmdError> {
    let thumb = bc_sim::render_design(
        &s.doc,
        &bc_sim::PreviewOptions {
            max_size: 256,
            ..Default::default()
        },
    )
    .map(|p| p.png)
    .ok();
    bc_doc::bcraft::save(
        &s.doc,
        &bc_doc::bcraft::Extras {
            thumbnail_png: thumb,
            template,
        },
        path,
    )?;
    Ok(())
}

// ---------- kształty ----------

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct RectParams {
    /// Lewy dolny narożnik X (mm).
    x: f64,
    /// Lewy dolny narożnik Y (mm).
    y: f64,
    /// Szerokość (mm, > 0).
    w: f64,
    /// Wysokość (mm, > 0).
    h: f64,
    /// Promień narożników (mm).
    #[serde(default)]
    radius: f64,
    /// Indeks warstwy (0-29).
    #[serde(default = "default_layer")]
    layer: u8,
}

fn shape_rect(s: &mut Session, p: Value) -> Result<Value, CmdError> {
    let p: RectParams = parse(p)?;
    let kind = Kind::Rect {
        x: finite("x", p.x)?,
        y: finite("y", p.y)?,
        w: positive("w", p.w)?,
        h: positive("h", p.h)?,
        radius: p.radius.max(0.0),
    };
    created(s, p.layer, kind)
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct EllipseParams {
    /// Środek X (mm).
    cx: f64,
    /// Środek Y (mm).
    cy: f64,
    /// Półoś X (mm, > 0).
    rx: f64,
    /// Półoś Y (mm, > 0).
    ry: f64,
    #[serde(default = "default_layer")]
    layer: u8,
}

fn shape_ellipse(s: &mut Session, p: Value) -> Result<Value, CmdError> {
    let p: EllipseParams = parse(p)?;
    let kind = Kind::Ellipse {
        cx: finite("cx", p.cx)?,
        cy: finite("cy", p.cy)?,
        rx: positive("rx", p.rx)?,
        ry: positive("ry", p.ry)?,
    };
    created(s, p.layer, kind)
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct PathParams {
    /// Dane ścieżki w składni SVG `d` (M/L/H/V/C/S/Q/T/A/Z), współrzędne w mm, Y w górę.
    #[serde(default)]
    d: Option<String>,
    /// Alternatywnie: lista punktów [[x, y], ...] polilinii.
    #[serde(default)]
    points: Option<Vec<[f64; 2]>>,
    /// Zamknij polilinię (dotyczy `points`).
    #[serde(default)]
    closed: bool,
    #[serde(default = "default_layer")]
    layer: u8,
}

fn shape_path(s: &mut Session, p: Value) -> Result<Value, CmdError> {
    let p: PathParams = parse(p)?;
    let d = match (p.d, p.points) {
        (Some(d), None) => {
            let path =
                bc_geom::parse_svg_path(&d).map_err(|e| CmdError::new("invalid_params", e))?;
            if path.elements().is_empty() {
                return Err(CmdError::new("invalid_params", "pusta ścieżka"));
            }
            if !bc_geom::within_limits(&path) {
                return Err(CmdError::new(
                    "invalid_params",
                    format!(
                        "współrzędne ścieżki poza zakresem ±{} mm",
                        bc_geom::MAX_COORD
                    ),
                ));
            }
            path.to_svg()
        }
        (None, Some(pts)) if pts.len() >= 2 => {
            let pts: Vec<Point> = pts.iter().map(|[x, y]| Point::new(*x, *y)).collect();
            if pts.iter().any(|p| {
                !p.x.is_finite()
                    || !p.y.is_finite()
                    || p.x.abs() > bc_geom::MAX_COORD
                    || p.y.abs() > bc_geom::MAX_COORD
            }) {
                return Err(CmdError::new(
                    "invalid_params",
                    "współrzędne muszą być skończone",
                ));
            }
            bc_geom::polyline_path(&pts, p.closed).to_svg()
        }
        (None, Some(_)) => {
            return Err(CmdError::new(
                "invalid_params",
                "points: co najmniej 2 punkty",
            ))
        }
        _ => {
            return Err(CmdError::new(
                "invalid_params",
                "podaj dokładnie jedno z: d, points",
            ))
        }
    };
    created(s, p.layer, Kind::Path { d })
}

pub(crate) fn created(s: &mut Session, layer: u8, kind: Kind) -> Result<Value, CmdError> {
    let id = s
        .doc
        .add(layer, kind)
        .map_err(|e| CmdError::new("invalid_params", e.to_string()))?;
    s.selection = vec![id];
    ok(IdResult { id })
}

// ---------- obiekty ----------

fn object_delete(s: &mut Session, p: Value) -> Result<Value, CmdError> {
    let p: Ids = parse(p)?;
    let ids = top_most(&s.doc, s.targets(&p.ids)?);
    for id in &ids {
        s.doc.remove(*id)?;
    }
    s.prune_selection();
    ok(IdsResult { ids })
}

#[derive(Deserialize, JsonSchema)]
#[serde(untagged)]
enum Anchor {
    /// "center" (środek zaznaczenia, domyślnie) albo "origin" (0,0).
    Named(String),
    /// Punkt [x, y] w mm.
    Point([f64; 2]),
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
enum Mirror {
    /// Odbicie w poziomie (względem osi pionowej).
    Horizontal,
    /// Odbicie w pionie (względem osi poziomej).
    Vertical,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct TransformParams {
    #[serde(default)]
    ids: Option<Vec<ObjectId>>,
    /// Przesunięcie [dx, dy] w mm (wykonywane na końcu).
    #[serde(default)]
    translate: Option<[f64; 2]>,
    /// Obrót w stopniach, przeciwnie do ruchu wskazówek zegara.
    #[serde(default)]
    rotate: Option<f64>,
    /// Skala [sx, sy].
    #[serde(default)]
    scale: Option<[f64; 2]>,
    #[serde(default)]
    mirror: Option<Mirror>,
    /// Punkt odniesienia dla skali, obrotu i odbicia.
    #[serde(default)]
    anchor: Option<Anchor>,
}

fn object_transform(s: &mut Session, p: Value) -> Result<Value, CmdError> {
    let p: TransformParams = parse(p)?;
    let ids = top_most(&s.doc, s.targets(&p.ids)?);
    if let Some(id) = ids
        .iter()
        .find(|id| s.doc.find(**id).is_some_and(|o| o.locked))
    {
        return Err(CmdError::new(
            "locked",
            format!("obiekt {id} jest zablokowany"),
        ));
    }
    if p.scale.is_some() {
        if let Some(id) = ids
            .iter()
            .find(|id| s.doc.find(**id).is_some_and(|o| o.lock_size))
        {
            return Err(CmdError::new(
                "locked",
                format!("obiekt {id} ma zablokowany rozmiar"),
            ));
        }
    }
    let anchor = match p.anchor {
        None => world_bbox(&s.doc, &ids)?
            .map(|r| r.center())
            .unwrap_or(Point::ORIGIN),
        Some(Anchor::Named(n)) if n == "center" => world_bbox(&s.doc, &ids)?
            .map(|r| r.center())
            .unwrap_or(Point::ORIGIN),
        Some(Anchor::Named(n)) if n == "origin" => Point::ORIGIN,
        Some(Anchor::Named(n)) => {
            return Err(CmdError::new(
                "invalid_params",
                format!("anchor: nieznana wartość {n}"),
            ))
        }
        Some(Anchor::Point([x, y])) => Point::new(x, y),
    };
    let mut local = Affine::IDENTITY;
    if let Some([sx, sy]) = p.scale {
        if sx == 0.0 || sy == 0.0 || !sx.is_finite() || !sy.is_finite() {
            return Err(CmdError::new(
                "invalid_params",
                "scale: wartości muszą być skończone i niezerowe",
            ));
        }
        local = Affine::scale_non_uniform(sx, sy) * local;
    }
    if let Some(deg) = p.rotate {
        local = Affine::rotate(finite("rotate", deg)?.to_radians()) * local;
    }
    match p.mirror {
        Some(Mirror::Horizontal) => local = Affine::scale_non_uniform(-1.0, 1.0) * local,
        Some(Mirror::Vertical) => local = Affine::scale_non_uniform(1.0, -1.0) * local,
        None => {}
    }
    let a = anchor.to_vec2();
    let mut t = Affine::translate(a) * local * Affine::translate(-a);
    if let Some([dx, dy]) = p.translate {
        t = Affine::translate(Vec2::new(
            finite("translate", dx)?,
            finite("translate", dy)?,
        )) * t;
    }
    for id in &ids {
        let pa = s.doc.parent_affine(*id).unwrap_or(Affine::IDENTITY);
        let o = s.doc.find_mut(*id).expect("sprawdzone w targets");
        o.transform = (pa.inverse() * t * pa * o.affine()).as_coeffs();
    }
    ensure_in_range(&s.doc, &ids)?;
    ok(IdsResult { ids })
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct SetParams {
    #[serde(default)]
    ids: Option<Vec<ObjectId>>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    visible: Option<bool>,
    /// Blokada pozycji.
    #[serde(default)]
    locked: Option<bool>,
    /// Blokada tylko rozmiaru (przesuwanie i obrót dozwolone).
    #[serde(default)]
    lock_size: Option<bool>,
    /// Notatka (pusty tekst usuwa).
    #[serde(default)]
    note: Option<String>,
    /// Priorytet w kolejności cięcia (niższy = wcześniej).
    #[serde(default)]
    priority: Option<i32>,
    /// Tagi (zastępują dotychczasowe).
    #[serde(default)]
    tags: Option<Vec<String>>,
    /// Obiekt pomocniczy — widoczny, nigdy nie wypalany.
    #[serde(default)]
    helper: Option<bool>,
}

fn object_set(s: &mut Session, p: Value) -> Result<Value, CmdError> {
    let p: SetParams = parse(p)?;
    let ids = s.targets(&p.ids)?;
    for id in &ids {
        let o = s.doc.find_mut(*id).expect("sprawdzone");
        if let Some(n) = &p.name {
            o.name = if n.is_empty() { None } else { Some(n.clone()) };
        }
        if let Some(v) = p.visible {
            o.visible = v;
        }
        if let Some(l) = p.locked {
            o.locked = l;
        }
        if let Some(l) = p.lock_size {
            o.lock_size = l;
        }
        if let Some(n) = &p.note {
            o.note = if n.is_empty() { None } else { Some(n.clone()) };
        }
        if let Some(v) = p.priority {
            o.priority = v;
        }
        if let Some(h) = p.helper {
            o.helper = h;
        }
        if let Some(t) = &p.tags {
            o.tags = t
                .iter()
                .map(|x| x.trim().to_string())
                .filter(|x| !x.is_empty())
                .collect();
        }
    }
    ok(IdsResult { ids })
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct AssignLayerParams {
    #[serde(default)]
    ids: Option<Vec<ObjectId>>,
    /// Indeks warstwy (0-29).
    layer: u8,
}

fn object_assign_layer(s: &mut Session, p: Value) -> Result<Value, CmdError> {
    let p: AssignLayerParams = parse(p)?;
    s.doc
        .layer(p.layer)
        .map_err(|e| CmdError::new("invalid_params", e.to_string()))?;
    let ids = s.targets(&p.ids)?;
    fn set(o: &mut Object, l: u8) {
        o.layer = l;
        if let Kind::Group { children } = &mut o.kind {
            for c in children {
                set(c, l);
            }
        }
    }
    for id in &ids {
        set(s.doc.find_mut(*id).expect("sprawdzone"), p.layer);
    }
    ok(IdsResult { ids })
}

/// Wspólny rodzic obiektów (`None` = najwyższy poziom); błąd, gdy leżą w różnych miejscach drzewa.
pub(crate) fn common_parent(
    doc: &Document,
    ids: &[ObjectId],
) -> Result<Option<ObjectId>, CmdError> {
    let mut parent = None;
    for (i, id) in ids.iter().enumerate() {
        let p = doc
            .parent_of(*id)
            .ok_or_else(|| CmdError::new("not_found", format!("nie znaleziono obiektu {id}")))?;
        if i == 0 {
            parent = p;
        } else if p != parent {
            return Err(CmdError::new(
                "invalid_params",
                "obiekty muszą leżeć w tej samej grupie (albo wszystkie na najwyższym poziomie)",
            ));
        }
    }
    Ok(parent)
}

fn object_group(s: &mut Session, p: Value) -> Result<Value, CmdError> {
    let p: Ids = parse(p)?;
    let ids = s.targets(&p.ids)?;
    let parent = common_parent(&s.doc, &ids)?;
    let gid = s.doc.alloc_id();
    let list = s
        .doc
        .container_mut(parent)
        .ok_or_else(|| CmdError::new("invalid_params", "rodzic nie jest kontenerem"))?;
    // kolejność rysowania członków zachowana; grupa trafia w miejsce najniższego
    let pos = list
        .iter()
        .position(|o| ids.contains(&o.id))
        .expect("co najmniej jeden");
    let mut members = Vec::new();
    list.retain(|o| {
        if ids.contains(&o.id) {
            members.push(o.clone());
            false
        } else {
            true
        }
    });
    let layer = members[0].layer;
    list.insert(
        pos,
        Object::new(gid, layer, Kind::Group { children: members }),
    );
    s.selection = vec![gid];
    ok(IdResult { id: gid })
}

fn object_ungroup(s: &mut Session, p: Value) -> Result<Value, CmdError> {
    let p: Ids = parse(p)?;
    let ids = s.targets(&p.ids)?;
    let mut released = Vec::new();
    for gid in ids {
        if !matches!(s.doc.find(gid).map(|o| &o.kind), Some(Kind::Group { .. })) {
            return Err(CmdError::new(
                "invalid_params",
                format!("obiekt {gid} nie jest grupą"),
            ));
        }
        let parent = s.doc.parent_of(gid).expect("istnieje");
        let list = s
            .doc
            .container_mut(parent)
            .expect("rodzic grupy jest kontenerem");
        let pos = list
            .iter()
            .position(|o| o.id == gid)
            .expect("grupa w kontenerze");
        let g = list.remove(pos);
        let ga = g.affine();
        let Kind::Group { children } = g.kind else {
            unreachable!()
        };
        for (i, mut c) in children.into_iter().enumerate() {
            c.transform = (ga * c.affine()).as_coeffs();
            released.push(c.id);
            list.insert(pos + i, c);
        }
    }
    s.selection = released.clone();
    ok(IdsResult { ids: released })
}

// ---------- zaznaczenie ----------

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct SelectParams {
    /// Nowe zaznaczenie.
    ids: Vec<ObjectId>,
}

fn selection_set(s: &mut Session, p: Value) -> Result<Value, CmdError> {
    let p: SelectParams = parse(p)?;
    if let Some(id) = p.ids.iter().find(|id| s.doc.find(**id).is_none()) {
        return Err(CmdError::new(
            "not_found",
            format!("nie znaleziono obiektu {id}"),
        ));
    }
    s.selection = p.ids.clone();
    ok(IdsResult { ids: p.ids })
}

fn selection_clear(s: &mut Session, p: Value) -> Result<Value, CmdError> {
    let _: NoParams = parse(p)?;
    s.selection.clear();
    ok(IdsResult { ids: vec![] })
}

fn selection_all(s: &mut Session, p: Value) -> Result<Value, CmdError> {
    let _: NoParams = parse(p)?;
    s.selection = s.doc.objects.iter().map(|o| o.id).collect();
    ok(IdsResult {
        ids: s.selection.clone(),
    })
}

fn selection_get(s: &mut Session, p: Value) -> Result<Value, CmdError> {
    let _: NoParams = parse(p)?;
    ok(IdsResult {
        ids: s.selection.clone(),
    })
}

// ---------- warstwy ----------

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct LayerSetParams {
    /// Indeks warstwy (0-29).
    index: u8,
    #[serde(default)]
    name: Option<String>,
    /// Kolor #rrggbb.
    #[serde(default)]
    color: Option<String>,
    #[serde(default)]
    mode: Option<LayerMode>,
    /// mm/s
    #[serde(default)]
    speed: Option<f64>,
    /// 0-100 %
    #[serde(default)]
    power_max: Option<f64>,
    /// 0-100 %
    #[serde(default)]
    power_min: Option<f64>,
    #[serde(default)]
    passes: Option<u32>,
    #[serde(default)]
    output: Option<bool>,
    #[serde(default)]
    show: Option<bool>,
    #[serde(default)]
    air_assist: Option<bool>,
}

fn layer_set(s: &mut Session, p: Value) -> Result<Value, CmdError> {
    let p: LayerSetParams = parse(p)?;
    let l = s
        .doc
        .layer_mut(p.index)
        .map_err(|e| CmdError::new("invalid_params", e.to_string()))?;
    let pct = |n: &str, v: f64| {
        if (0.0..=100.0).contains(&v) {
            Ok(v)
        } else {
            Err(CmdError::new("invalid_params", format!("{n}: 0-100")))
        }
    };
    if let Some(v) = p.name {
        l.name = v;
    }
    if let Some(c) = p.color {
        let valid =
            c.len() == 7 && c.starts_with('#') && c[1..].chars().all(|ch| ch.is_ascii_hexdigit());
        if !valid {
            return Err(CmdError::new("invalid_params", "color: format #rrggbb"));
        }
        l.color = c.to_lowercase();
    }
    if let Some(v) = p.mode {
        l.mode = v;
    }
    if let Some(v) = p.speed {
        l.speed = positive("speed", v)?;
    }
    if let Some(v) = p.power_max {
        l.power_max = pct("power_max", v)?;
    }
    if let Some(v) = p.power_min {
        l.power_min = pct("power_min", v)?;
    }
    if l.power_min > l.power_max {
        return Err(CmdError::new(
            "invalid_params",
            "power_min nie może przekraczać power_max",
        ));
    }
    if let Some(v) = p.passes {
        if v == 0 {
            return Err(CmdError::new("invalid_params", "passes >= 1"));
        }
        l.passes = v;
    }
    if let Some(v) = p.output {
        l.output = v;
    }
    if let Some(v) = p.show {
        l.show = v;
    }
    if let Some(v) = p.air_assist {
        l.air_assist = v;
    }
    ok(l.clone())
}

// ---------- historia ----------

#[derive(Serialize, JsonSchema)]
struct HistoryResult {
    /// Etykieta cofniętego/ponowionego kroku (brak = nic do zrobienia).
    label: Option<String>,
    can_undo: bool,
    can_redo: bool,
}

fn edit_undo(s: &mut Session, p: Value) -> Result<Value, CmdError> {
    let _: NoParams = parse(p)?;
    let label = s.history.undo(&mut s.doc);
    if label.is_some() {
        s.touch();
    }
    s.prune_selection();
    ok(HistoryResult {
        label,
        can_undo: s.history.can_undo(),
        can_redo: s.history.can_redo(),
    })
}

fn edit_redo(s: &mut Session, p: Value) -> Result<Value, CmdError> {
    let _: NoParams = parse(p)?;
    let label = s.history.redo(&mut s.doc);
    if label.is_some() {
        s.touch();
    }
    s.prune_selection();
    ok(HistoryResult {
        label,
        can_undo: s.history.can_undo(),
        can_redo: s.history.can_redo(),
    })
}

#[derive(Serialize, JsonSchema)]
struct HistoryList {
    /// Kroki do cofnięcia, od najstarszego.
    undo: Vec<String>,
    can_redo: bool,
}

fn history_list(s: &mut Session, p: Value) -> Result<Value, CmdError> {
    let _: NoParams = parse(p)?;
    ok(HistoryList {
        undo: s.history.undo_labels(),
        can_redo: s.history.can_redo(),
    })
}

// ---------- pomocnicze ----------

pub(crate) fn positive(name: &str, v: f64) -> Result<f64, CmdError> {
    if v.is_finite() && v > 0.0 && v <= bc_geom::MAX_COORD {
        Ok(v)
    } else {
        Err(CmdError::new(
            "invalid_params",
            format!("{name} musi być > 0 i ≤ {} mm", bc_geom::MAX_COORD),
        ))
    }
}

pub(crate) fn finite(name: &str, v: f64) -> Result<f64, CmdError> {
    if v.is_finite() && v.abs() <= bc_geom::MAX_COORD {
        Ok(v)
    } else {
        Err(CmdError::new(
            "invalid_params",
            format!(
                "{name} musi być liczbą skończoną w zakresie ±{} mm",
                bc_geom::MAX_COORD
            ),
        ))
    }
}

/// Sprawdza, czy geometria obiektów mieści się w ±MAX_COORD (po transformacjach,
/// kopiach itd.). Błąd w poleceniu cofa jego zmiany (polecenia są atomowe).
pub(crate) fn ensure_in_range(doc: &Document, ids: &[ObjectId]) -> Result<(), CmdError> {
    ensure_in_range_with(doc, ids, &mut |_| Ok(()))
}

/// [`ensure_in_range`] dla wielu nowych obiektów (tablice): druga połowa postępu
/// polecenia i możliwość anulowania (REQ-16-006).
pub(crate) fn ensure_in_range_tick(s: &Session, ids: &[ObjectId]) -> Result<(), CmdError> {
    let n = ids.len();
    ensure_in_range_with(&s.doc, ids, &mut |k| s.tick(n + k, 2 * n))
}

fn ensure_in_range_with(
    doc: &Document,
    ids: &[ObjectId],
    tick: &mut dyn FnMut(usize) -> Result<(), CmdError>,
) -> Result<(), CmdError> {
    // jedno przejście drzewa: wyszukiwanie każdego id osobno jest kwadratowe przy
    // dziesiątkach tysięcy kopii (REQ-16-006)
    fn rec(
        doc: &Document,
        list: &[Object],
        pa: Affine,
        want: &std::collections::HashSet<ObjectId>,
        seen: &mut usize,
        tick: &mut dyn FnMut(usize) -> Result<(), CmdError>,
    ) -> Result<(), CmdError> {
        for o in list {
            if want.contains(&o.id) {
                tick(*seen)?;
                *seen += 1;
                let mut out = Vec::new();
                o.world_paths(doc, pa, &mut out)?;
                if out.iter().any(|(_, _, p)| !bc_geom::within_limits(p)) {
                    return Err(CmdError::new(
                        "invalid_params",
                        format!(
                            "obiekt {} wyszedłby poza zakres ±{} mm",
                            o.id,
                            bc_geom::MAX_COORD
                        ),
                    ));
                }
            } else if !o.children().is_empty() {
                rec(doc, o.children(), pa * o.affine(), want, seen, tick)?;
            }
        }
        Ok(())
    }
    let want: std::collections::HashSet<ObjectId> = ids.iter().copied().collect();
    rec(doc, &doc.objects, Affine::IDENTITY, &want, &mut 0, tick)
}

/// Usuwa z listy obiekty, których przodek też jest na liście (żeby nie transformować dwa razy).
pub(crate) fn top_most(doc: &Document, ids: Vec<ObjectId>) -> Vec<ObjectId> {
    fn ancestors(list: &[Object], id: ObjectId, path: &mut Vec<ObjectId>) -> bool {
        for o in list {
            if o.id == id {
                return true;
            }
            path.push(o.id);
            if ancestors(o.children(), id, path) {
                return true;
            }
            path.pop();
        }
        false
    }
    let set: std::collections::HashSet<_> = ids.iter().copied().collect();
    let mut out = Vec::new();
    for id in ids {
        let mut path = Vec::new();
        ancestors(&doc.objects, id, &mut path);
        if !path.iter().any(|a| set.contains(a)) && !out.contains(&id) {
            out.push(id);
        }
    }
    out
}

pub(crate) fn world_bbox(
    doc: &Document,
    ids: &[ObjectId],
) -> Result<Option<bc_geom::Rect>, CmdError> {
    let mut acc: Option<bc_geom::Rect> = None;
    for id in ids {
        let pa = doc.parent_affine(*id).unwrap_or(Affine::IDENTITY);
        if let Some(b) = doc.find(*id).expect("sprawdzone").world_bbox(doc, pa)? {
            acc = Some(acc.map_or(b, |a| a.union(b)));
        }
    }
    Ok(acc)
}

pub(crate) fn register_all(r: &mut Registry) {
    use SafetyTier::*;
    add::<NewDoc, Empty>(
        r,
        "document.new",
        "Nowy dokument",
        "Tworzy pusty dokument (30 warstw, obszar 400x400 mm); czyści historię.",
        Design,
        false,
        Some("Ctrl+N"),
        &["REQ-02-034"],
        document_new,
    );
    add::<InspectParams, DocumentReport>(r, "document.inspect", "Raport dokumentu", "Warstwy, obiekty (id, typ, warstwa, bbox, transformacja, długość, zamknięcie), zaznaczenie i statystyki.", Read, false, None, &["REQ-15-019"], document_inspect);
    add::<NotesParams, Empty>(
        r,
        "document.set_notes",
        "Notatki dokumentu",
        "Ustawia notatki dokumentu (zapisywane w .bcraft).",
        Design,
        true,
        None,
        &["REQ-02-051"],
        document_set_notes,
    );
    add::<WorkspaceParams, Empty>(
        r,
        "document.set_workspace",
        "Obszar roboczy",
        "Ustawia rozmiar obszaru roboczego w mm.",
        Design,
        true,
        None,
        &[],
        document_set_workspace,
    );
    add::<OpenParams, FileResult>(
        r,
        "file.open",
        "Otwórz",
        "Otwiera plik .bcraft (z migracją i walidacją schematu); czyści historię.",
        Design,
        false,
        Some("Ctrl+O"),
        &["REQ-02-046", "REQ-02-052", "REQ-14-011"],
        file_open,
    );
    add::<SaveParams, FileResult>(
        r,
        "file.save",
        "Zapisz",
        "Zapisuje dokument jako .bcraft (z miniaturą i notatkami).",
        Design,
        false,
        Some("Ctrl+S"),
        &["REQ-02-046", "REQ-02-050"],
        file_save,
    );
    add::<RectParams, IdResult>(
        r,
        "shape.rect",
        "Prostokąt",
        "Dodaje prostokąt (x, y = lewy dolny narożnik) z opcjonalnym promieniem narożników.",
        Design,
        true,
        Some("R"),
        &["REQ-02-006"],
        shape_rect,
    );
    add::<EllipseParams, IdResult>(
        r,
        "shape.ellipse",
        "Elipsa",
        "Dodaje elipsę o środku (cx, cy) i półosiach rx, ry.",
        Design,
        true,
        Some("E"),
        &["REQ-02-007"],
        shape_ellipse,
    );
    add::<PathParams, IdResult>(
        r,
        "shape.path",
        "Ścieżka",
        "Dodaje ścieżkę z danych SVG `d` lub z listy punktów polilinii.",
        Design,
        true,
        Some("L"),
        &["REQ-02-001", "REQ-02-002", "REQ-02-004", "REQ-02-005"],
        shape_path,
    );
    add::<Ids, IdsResult>(
        r,
        "object.delete",
        "Usuń",
        "Usuwa obiekty.",
        Design,
        true,
        Some("Delete"),
        &["REQ-01-037"],
        object_delete,
    );
    add::<TransformParams, IdsResult>(r, "object.transform", "Transformuj", "Skala, obrót, odbicie (względem anchor) i przesunięcie; zapisywane jako transformacja obiektu, nie wypiekane w geometrię.", Design, true, None, &["REQ-02-024", "REQ-04-056", "REQ-04-092"], object_transform);
    add::<SetParams, IdsResult>(
        r,
        "object.set",
        "Właściwości obiektu",
        "Ustawia nazwę, widoczność, blokady, notatkę, priorytet i tagi.",
        Design,
        true,
        None,
        &[
            "REQ-02-025",
            "REQ-02-026",
            "REQ-02-027",
            "REQ-02-028",
            "REQ-02-029",
            "REQ-02-030",
            "REQ-02-032",
        ],
        object_set,
    );
    add::<AssignLayerParams, IdsResult>(
        r,
        "object.assign_layer",
        "Przypisz warstwę",
        "Przypisuje obiekty (z dziećmi grup) do warstwy.",
        Design,
        true,
        None,
        &["REQ-02-023"],
        object_assign_layer,
    );
    add::<Ids, IdResult>(
        r,
        "object.group",
        "Grupuj",
        "Grupuje obiekty najwyższego poziomu.",
        Design,
        true,
        Some("Ctrl+G"),
        &["REQ-02-018", "REQ-04-094"],
        object_group,
    );
    add::<Ids, IdsResult>(
        r,
        "object.ungroup",
        "Rozgrupuj",
        "Rozgrupowuje grupy; transformacja grupy przechodzi na dzieci.",
        Design,
        true,
        Some("Ctrl+U"),
        &["REQ-04-094"],
        object_ungroup,
    );
    add::<SelectParams, IdsResult>(
        r,
        "selection.set",
        "Zaznacz",
        "Ustawia zaznaczenie.",
        Read,
        false,
        None,
        &["REQ-01-037"],
        selection_set,
    );
    add::<NoParams, IdsResult>(
        r,
        "selection.clear",
        "Odznacz",
        "Czyści zaznaczenie.",
        Read,
        false,
        Some("Escape"),
        &["REQ-01-037"],
        selection_clear,
    );
    add::<NoParams, IdsResult>(
        r,
        "selection.all",
        "Zaznacz wszystko",
        "Zaznacza wszystkie obiekty najwyższego poziomu.",
        Read,
        false,
        Some("Ctrl+A"),
        &["REQ-01-037"],
        selection_all,
    );
    add::<NoParams, IdsResult>(
        r,
        "selection.get",
        "Bieżące zaznaczenie",
        "Zwraca bieżące zaznaczenie.",
        Read,
        false,
        None,
        &["REQ-01-037"],
        selection_get,
    );
    add::<LayerSetParams, bc_doc::Layer>(
        r,
        "layer.set",
        "Ustawienia warstwy",
        "Zmienia nazwę, kolor, tryb i podstawowe parametry warstwy.",
        Design,
        true,
        None,
        &["REQ-02-034"],
        layer_set,
    );
    add::<NoParams, HistoryResult>(
        r,
        "edit.undo",
        "Cofnij",
        "Cofa ostatni krok historii.",
        Design,
        false,
        Some("Ctrl+Z"),
        &["REQ-15-017"],
        edit_undo,
    );
    add::<NoParams, HistoryResult>(
        r,
        "edit.redo",
        "Ponów",
        "Ponawia cofnięty krok.",
        Design,
        false,
        Some("Ctrl+Y"),
        &["REQ-15-018"],
        edit_redo,
    );
    add::<NoParams, HistoryList>(
        r,
        "history.list",
        "Historia cofania",
        "Lista kroków historii.",
        Read,
        false,
        None,
        &[],
        history_list,
    );
}
