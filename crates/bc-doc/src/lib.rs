//! bc-doc — model dokumentu BurnCraft.
//!
//! Dokument = warstwy cięcia (kolory) + drzewo obiektów. Transformacje obiektów
//! są przechowywane osobno i nie są „wypiekane” w geometrię (REQ-02-024).
//! Historia cofania jest w [`history`], format pliku `.bcraft` w [`bcraft`].

pub mod bcraft;
pub mod history;
pub mod params;

pub use params::{
    AirCode, CutDirection, DotMode, FillGrouping, GcodeSettings, GcodeUnits, JobOrigin, Kinematics,
    Laser2Settings, LayerParams, Lead, LeadKind, NoGoZone, Overscan, Perforation, PlanSettings,
    Ramp, ScanOffset, StartFrom, StartSettings, SubLayer, Tab, TabParams,
};

use bc_geom::{Affine, BezPath, Point, Rect, Vec2};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub type ObjectId = u64;

/// Aktualna wersja schematu `document.json`.
pub const SCHEMA_VERSION: u32 = 5;

/// Liczba domyślnych warstw kolorów (REQ-02-034).
pub const DEFAULT_LAYER_COUNT: usize = 30;

/// Własna paleta BurnCraft (nie kopiuje palet innych programów).
pub const DEFAULT_PALETTE: [&str; DEFAULT_LAYER_COUNT] = [
    "#000000", "#1f5fd1", "#d12c1f", "#1f9d3a", "#c7a400", "#8a2bc2", "#d1691f", "#178a8a",
    "#c2185b", "#5d4037", "#607d8b", "#0d47a1", "#b71c1c", "#1b5e20", "#f57f17", "#4a148c",
    "#e65100", "#006064", "#880e4f", "#3e2723", "#9e9d24", "#283593", "#ad1457", "#2e7d32",
    "#ff6f00", "#6a1b9a", "#00838f", "#4e342e", "#455a64", "#827717",
];

#[derive(Debug, thiserror::Error)]
pub enum DocError {
    #[error("nie znaleziono obiektu {0}")]
    NotFound(ObjectId),
    #[error("niepoprawny dokument: {0}")]
    Invalid(String),
    #[error("błąd pliku: {0}")]
    Io(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum LayerMode {
    Line,
    Fill,
    OffsetFill,
    Image,
    FillLine,
    FloodFill,
}

/// Rola warstwy: zwykła warstwa cięcia albo warstwa narzędziowa T1/T2 (REQ-02-035/036).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum LayerRole {
    /// Warstwa cięcia/grawerowania (kolor).
    #[default]
    Cut,
    /// T1: obszar „wytnij zaznaczone” / ramka — nie jest wypalany.
    Frame,
    /// T2: geometria pomocnicza — nie jest wypalana.
    Helper,
}

/// Indeks warstwy T1 i T2.
pub const T1_INDEX: u8 = 30;
pub const T2_INDEX: u8 = 31;
/// Pierwszy indeks warstw z własnej palety użytkownika (REQ-02-039).
pub const FIRST_CUSTOM_INDEX: u8 = 32;

/// Warstwa cięcia. Pełny zestaw parametrów z sekcji 7.2 jest w [LayerParams] (ADR 0009).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Layer {
    pub index: u8,
    #[serde(default)]
    pub role: LayerRole,
    pub color: String,
    pub name: String,
    pub mode: LayerMode,
    /// mm/s
    pub speed: f64,
    /// %
    pub power_max: f64,
    /// %
    pub power_min: f64,
    pub passes: u32,
    pub output: bool,
    pub show: bool,
    pub air_assist: bool,
    /// Parametry cięcia §7.2 (schemat 4).
    #[serde(default)]
    pub params: LayerParams,
}

/// 30 warstw kolorów + T1 i T2.
pub fn default_layers() -> Vec<Layer> {
    let mut v: Vec<Layer> = (0..DEFAULT_LAYER_COUNT as u8)
        .map(Layer::default_for)
        .collect();
    for (index, role, name, color) in [
        (T1_INDEX, LayerRole::Frame, "T1", "#7a7a7a"),
        (T2_INDEX, LayerRole::Helper, "T2", "#b0b0b0"),
    ] {
        v.push(Layer {
            role,
            name: name.into(),
            color: color.into(),
            output: false,
            air_assist: false,
            ..Layer::default_for(index)
        });
    }
    v
}

impl Layer {
    pub fn default_for(index: u8) -> Self {
        Layer {
            index,
            role: LayerRole::Cut,
            color: DEFAULT_PALETTE[index as usize % DEFAULT_LAYER_COUNT].to_string(),
            name: format!("C{index:02}"),
            mode: LayerMode::Line,
            speed: 100.0,
            power_max: 20.0,
            power_min: 10.0,
            passes: 1,
            output: true,
            show: true,
            air_assist: true,
            params: LayerParams::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Workspace {
    pub width: f64,
    pub height: f64,
}

/// Geometria obiektu we współrzędnych lokalnych (przed `transform`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Kind {
    /// Dowolna ścieżka, dane w składni SVG `d`.
    Path {
        d: String,
    },
    /// (x, y) = lewy dolny narożnik.
    Rect {
        x: f64,
        y: f64,
        w: f64,
        h: f64,
        radius: f64,
    },
    Ellipse {
        cx: f64,
        cy: f64,
        rx: f64,
        ry: f64,
    },
    /// Wielokąt foremny; `rotation` w stopniach (kierunek pierwszego wierzchołka).
    Polygon {
        cx: f64,
        cy: f64,
        r: f64,
        sides: u32,
        rotation: f64,
    },
    /// Gwiazda; z `r_middle` — podwójna gwiazda.
    Star {
        cx: f64,
        cy: f64,
        r_outer: f64,
        r_inner: f64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        r_middle: Option<f64>,
        points: u32,
        rotation: f64,
    },
    /// Łuk okręgu (otwarty); kąty w stopniach, `sweep` > 0 = CCW.
    Arc {
        cx: f64,
        cy: f64,
        r: f64,
        start: f64,
        sweep: f64,
    },
    Group {
        children: Vec<Object>,
    },
    /// Wirtualna (dynamiczna) tablica: `children` to źródło, kopie powstają z `layout`
    /// przy każdym odczycie geometrii — edycja źródła zmienia wszystkie kopie (REQ-02-019).
    Array {
        children: Vec<Object>,
        layout: ArrayLayout,
    },
    /// Klon (obiekt-odniesienie): geometria obiektu `source` (z jego własną
    /// transformacją) w miejscu wyznaczonym transformacją klonu (REQ-02-020).
    Clone {
        source: ObjectId,
    },
}

/// Układ kopii wirtualnej tablicy.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ArrayLayout {
    /// Siatka: kolumny × wiersze, odstępy środek–środek, przesunięcie co drugiego wiersza.
    Grid {
        columns: u32,
        rows: u32,
        dx: f64,
        dy: f64,
        #[serde(default)]
        row_shift: f64,
    },
    /// Okrąg: `count` elementów (z oryginałem) wokół `center`, kąt całkowity `angle` (°).
    Circular {
        count: u32,
        center: [f64; 2],
        #[serde(default = "full_turn")]
        angle: f64,
        #[serde(default = "yes")]
        rotate_copies: bool,
    },
}

fn full_turn() -> f64 {
    360.0
}

fn yes() -> bool {
    true
}

/// Maksymalna głębokość odwołań klon → źródło (ochrona przed cyklami).
pub const MAX_CLONE_DEPTH: u8 = 16;

/// Maksymalna liczba kopii w jednej tablicy (ochrona przed przypadkowym „wybuchem” dokumentu).
pub const MAX_ARRAY_COPIES: u64 = 100_000;

impl ArrayLayout {
    /// Transformacje kopii (pierwsza = tożsamość, czyli oryginał). `anchor` to środek
    /// źródła — używany dla kopii kołowych bez obrotu.
    pub fn transforms(&self, anchor: Point) -> Result<Vec<Affine>, DocError> {
        let bad = |m: &str| Err(DocError::Invalid(format!("tablica: {m}")));
        match *self {
            ArrayLayout::Grid {
                columns,
                rows,
                dx,
                dy,
                row_shift,
            } => {
                if columns == 0 || rows == 0 {
                    return bad("columns i rows muszą być ≥ 1");
                }
                if u64::from(columns) * u64::from(rows) > MAX_ARRAY_COPIES {
                    return bad(&format!("najwyżej {MAX_ARRAY_COPIES} kopii"));
                }
                if ![dx, dy, row_shift].iter().all(|v| v.is_finite()) {
                    return bad("odstępy muszą być skończone");
                }
                let mut out = Vec::with_capacity((columns * rows) as usize);
                for r in 0..rows {
                    for c in 0..columns {
                        let shift = if r % 2 == 1 { row_shift } else { 0.0 };
                        out.push(Affine::translate((c as f64 * dx + shift, r as f64 * dy)));
                    }
                }
                Ok(out)
            }
            ArrayLayout::Circular {
                count,
                center,
                angle,
                rotate_copies,
            } => {
                if count < 2 || u64::from(count) > MAX_ARRAY_COPIES {
                    return bad(&format!("count: 2–{MAX_ARRAY_COPIES}"));
                }
                if !(angle.is_finite() && center.iter().all(|v| v.is_finite())) {
                    return bad("kąt i środek muszą być skończone");
                }
                let c = Vec2::new(center[0], center[1]);
                let step = if angle.abs() >= 360.0 {
                    angle / count as f64
                } else {
                    angle / (count - 1) as f64
                };
                Ok((0..count)
                    .map(|i| {
                        let rot = Affine::translate(c)
                            * Affine::rotate((step * i as f64).to_radians())
                            * Affine::translate(-c);
                        if rotate_copies || i == 0 {
                            rot
                        } else {
                            Affine::translate((rot * anchor).to_vec2() - anchor.to_vec2())
                        }
                    })
                    .collect())
            }
        }
    }
}

impl Kind {
    pub fn type_name(&self) -> &'static str {
        match self {
            Kind::Path { .. } => "path",
            Kind::Rect { .. } => "rect",
            Kind::Ellipse { .. } => "ellipse",
            Kind::Polygon { .. } => "polygon",
            Kind::Star { .. } => "star",
            Kind::Arc { .. } => "arc",
            Kind::Group { .. } => "group",
            Kind::Array { .. } => "array",
            Kind::Clone { .. } => "clone",
        }
    }

    /// Czy obiekt zawiera inne obiekty (grupa, wirtualna tablica).
    pub fn is_container(&self) -> bool {
        matches!(self, Kind::Group { .. } | Kind::Array { .. })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Object {
    pub id: ObjectId,
    #[serde(default)]
    pub name: Option<String>,
    pub layer: u8,
    pub visible: bool,
    pub locked: bool,
    /// Blokada tylko rozmiaru: przesuwanie i obrót dozwolone, skalowanie nie (REQ-02-026).
    #[serde(default, skip_serializing_if = "is_false")]
    pub lock_size: bool,
    /// Notatka obiektu (REQ-02-029).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    /// Priorytet w kolejności cięcia; niższy = wcześniej (REQ-02-030).
    #[serde(default, skip_serializing_if = "is_zero")]
    pub priority: i32,
    /// Tagi (REQ-02-032).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    /// Obiekt pomocniczy (linia prowadząca): widoczny, ale nigdy nie wypalany (REQ-02-021).
    #[serde(default, skip_serializing_if = "is_false")]
    pub helper: bool,
    /// Macierz afiniczna [a, b, c, d, e, f] jak w kurbo/SVG.
    pub transform: [f64; 6],
    /// Mostki ręczne (REQ-04-083), schemat 4.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tabs: Vec<Tab>,
    #[serde(flatten)]
    pub kind: Kind,
}

/// Nadaje nowe identyfikatory obiektom zagnieżdżonym w `kind` (kopie dzieci grup i tablic).
pub fn renumber_kind(kind: &mut Kind, next_id: &mut ObjectId) {
    if let Kind::Group { children } | Kind::Array { children, .. } = kind {
        for c in children {
            c.id = *next_id;
            *next_id += 1;
            renumber_kind(&mut c.kind, next_id);
        }
    }
}

fn geom_err(e: bc_geom::ops::GeomError) -> DocError {
    DocError::Invalid(e.to_string())
}

fn is_false(b: &bool) -> bool {
    !*b
}

fn is_zero(v: &i32) -> bool {
    *v == 0
}

impl Object {
    pub fn new(id: ObjectId, layer: u8, kind: Kind) -> Self {
        Object {
            id,
            name: None,
            layer,
            visible: true,
            locked: false,
            lock_size: false,
            note: None,
            priority: 0,
            tags: Vec::new(),
            helper: false,
            transform: Affine::IDENTITY.as_coeffs(),
            tabs: Vec::new(),
            kind,
        }
    }

    pub fn affine(&self) -> Affine {
        Affine::new(self.transform)
    }

    /// Ścieżka lokalna (bez transformacji). Grupy nie mają własnej geometrii.
    pub fn local_path(&self) -> Result<BezPath, DocError> {
        Ok(match &self.kind {
            Kind::Path { d } => bc_geom::parse_svg_path(d).map_err(DocError::Invalid)?,
            Kind::Rect { x, y, w, h, radius } => bc_geom::rect_path(*x, *y, *w, *h, *radius),
            Kind::Ellipse { cx, cy, rx, ry } => bc_geom::ellipse_path(*cx, *cy, *rx, *ry),
            Kind::Polygon {
                cx,
                cy,
                r,
                sides,
                rotation,
            } => {
                bc_geom::ops::regular_polygon(*cx, *cy, *r, *sides, *rotation).map_err(geom_err)?
            }
            Kind::Star {
                cx,
                cy,
                r_outer,
                r_inner,
                r_middle: None,
                points,
                rotation,
            } => bc_geom::ops::star(*cx, *cy, *r_outer, *r_inner, *points, *rotation)
                .map_err(geom_err)?,
            Kind::Star {
                cx,
                cy,
                r_outer,
                r_inner,
                r_middle: Some(m),
                points,
                rotation,
            } => bc_geom::ops::double_star(*cx, *cy, *r_outer, *m, *r_inner, *points, *rotation)
                .map_err(geom_err)?,
            Kind::Arc {
                cx,
                cy,
                r,
                start,
                sweep,
            } => bc_geom::ops::arc(*cx, *cy, *r, *start, *sweep).map_err(geom_err)?,
            Kind::Group { .. } | Kind::Array { .. } | Kind::Clone { .. } => BezPath::new(),
        })
    }

    /// Wszystkie ścieżki liściowe we współrzędnych świata, z warstwą każdego liścia.
    /// `doc` rozwiązuje odwołania klonów; kopie wirtualnych tablic są rozwijane.
    pub fn world_paths(
        &self,
        doc: &Document,
        parent: Affine,
        out: &mut Vec<(u8, ObjectId, BezPath)>,
    ) -> Result<(), DocError> {
        doc.collect(self, parent, 0, out)
    }

    pub fn world_bbox(&self, doc: &Document, parent: Affine) -> Result<Option<Rect>, DocError> {
        let mut paths = Vec::new();
        self.world_paths(doc, parent, &mut paths)?;
        Ok(paths
            .iter()
            .filter_map(|(_, _, p)| bc_geom::bbox(p))
            .reduce(|a, b| a.union(b)))
    }

    pub fn children(&self) -> &[Object] {
        match &self.kind {
            Kind::Group { children } | Kind::Array { children, .. } => children,
            _ => &[],
        }
    }

    pub fn children_mut(&mut self) -> Option<&mut Vec<Object>> {
        match &mut self.kind {
            Kind::Group { children } | Kind::Array { children, .. } => Some(children),
            _ => None,
        }
    }

    fn visit<'a>(&'a self, f: &mut dyn FnMut(&'a Object)) {
        f(self);
        for c in self.children() {
            c.visit(f);
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Document {
    pub schema: u32,
    pub name: String,
    pub units: String,
    pub workspace: Workspace,
    pub layers: Vec<Layer>,
    pub objects: Vec<Object>,
    pub next_id: ObjectId,
    /// Ustawienia planu cięcia (§8, schemat 4).
    #[serde(default)]
    pub plan: PlanSettings,
    /// Ustawienia startu zadania (REQ-10-006, schemat 5).
    #[serde(default)]
    pub start: StartSettings,
    /// Ustawienia kompilatora G-code (ADR 0011, schemat 5).
    #[serde(default)]
    pub gcode: GcodeSettings,
    /// Notatki przechowywane w `.bcraft` jako osobny `notes.md`.
    #[serde(skip)]
    pub notes: String,
    /// Obrazy notatek (nazwa → PNG/JPEG); w `.bcraft` jako `notes/<nazwa>`,
    /// w tekście jako `![](notes/<nazwa>)`. `Arc` — tanie kopie w historii cofania.
    #[serde(skip)]
    pub note_images: std::collections::BTreeMap<String, std::sync::Arc<[u8]>>,
}

/// Typ MIME obrazu notatki po sygnaturze pliku (PNG, JPEG).
pub fn note_image_mime(data: &[u8]) -> Option<&'static str> {
    if data.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some("image/png")
    } else if data.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some("image/jpeg")
    } else {
        None
    }
}

/// Dopuszczalna nazwa obrazu notatki: `[a-z0-9_-]+.(png|jpg)`.
pub fn valid_note_image_name(name: &str) -> bool {
    let Some((stem, ext)) = name.rsplit_once('.') else {
        return false;
    };
    !stem.is_empty()
        && stem.len() <= 64
        && stem
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
        && matches!(ext, "png" | "jpg")
}

impl Default for Document {
    fn default() -> Self {
        Document::new("Bez tytułu")
    }
}

impl Document {
    pub fn new(name: &str) -> Self {
        Document {
            schema: SCHEMA_VERSION,
            name: name.to_string(),
            units: "mm".into(),
            workspace: Workspace {
                width: 400.0,
                height: 400.0,
            },
            layers: default_layers(),
            objects: Vec::new(),
            next_id: 1,
            plan: PlanSettings::default(),
            start: StartSettings::default(),
            gcode: GcodeSettings::default(),
            notes: String::new(),
            note_images: Default::default(),
        }
    }

    pub fn alloc_id(&mut self) -> ObjectId {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    /// Dodaje obiekt na górze kolejności rysowania i zwraca jego id.
    pub fn add(&mut self, layer: u8, kind: Kind) -> Result<ObjectId, DocError> {
        self.layer(layer)?;
        let id = self.alloc_id();
        self.objects.push(Object::new(id, layer, kind));
        Ok(id)
    }

    pub fn layer(&self, index: u8) -> Result<&Layer, DocError> {
        self.layers
            .iter()
            .find(|l| l.index == index)
            .ok_or_else(|| DocError::Invalid(format!("brak warstwy {index}")))
    }

    pub fn layer_mut(&mut self, index: u8) -> Result<&mut Layer, DocError> {
        self.layers
            .iter_mut()
            .find(|l| l.index == index)
            .ok_or_else(|| DocError::Invalid(format!("brak warstwy {index}")))
    }

    /// Wszystkie obiekty (także zagnieżdżone w grupach), w kolejności rysowania.
    pub fn all_objects(&self) -> Vec<&Object> {
        let mut out = Vec::new();
        for o in &self.objects {
            o.visit(&mut |x| out.push(x));
        }
        out
    }

    pub fn find(&self, id: ObjectId) -> Option<&Object> {
        self.all_objects().into_iter().find(|o| o.id == id)
    }

    pub fn find_mut(&mut self, id: ObjectId) -> Option<&mut Object> {
        fn rec(list: &mut [Object], id: ObjectId) -> Option<&mut Object> {
            for o in list.iter_mut() {
                if o.id == id {
                    return Some(o);
                }
                if let Some(children) = o.children_mut() {
                    if let Some(f) = rec(children, id) {
                        return Some(f);
                    }
                }
            }
            None
        }
        rec(&mut self.objects, id)
    }

    /// Usuwa obiekt z dowolnego poziomu drzewa. Klony usuwanego obiektu (i jego
    /// potomków) stają się samodzielnymi kopiami, żeby nie zostały puste odwołania.
    pub fn remove(&mut self, id: ObjectId) -> Result<Object, DocError> {
        fn rec(list: &mut Vec<Object>, id: ObjectId) -> Option<Object> {
            if let Some(i) = list.iter().position(|o| o.id == id) {
                return Some(list.remove(i));
            }
            for o in list.iter_mut() {
                if let Some(children) = o.children_mut() {
                    if let Some(f) = rec(children, id) {
                        return Some(f);
                    }
                }
            }
            None
        }
        self.find(id).ok_or(DocError::NotFound(id))?;
        self.detach_clones_of(id)?;
        rec(&mut self.objects, id).ok_or(DocError::NotFound(id))
    }

    /// Zamienia klony obiektu `id` (i jego potomków) na samodzielne kopie źródła.
    pub fn detach_clones_of(&mut self, id: ObjectId) -> Result<usize, DocError> {
        let mut subtree = std::collections::HashSet::new();
        if let Some(o) = self.find(id) {
            o.visit(&mut |x| {
                subtree.insert(x.id);
            });
        }
        // klony spoza usuwanego poddrzewa, które na nie wskazują
        let targets: Vec<(ObjectId, ObjectId)> = self
            .all_objects()
            .into_iter()
            .filter_map(|o| match o.kind {
                Kind::Clone { source } if subtree.contains(&source) && !subtree.contains(&o.id) => {
                    Some((o.id, source))
                }
                _ => None,
            })
            .collect();
        for (clone_id, source) in &targets {
            let src = self.find(*source).expect("źródło istnieje").clone();
            let pa = self.parent_affine(*source).unwrap_or(Affine::IDENTITY);
            let mut kind = src.kind.clone();
            renumber_kind(&mut kind, &mut self.next_id);
            let o = self.find_mut(*clone_id).expect("klon istnieje");
            o.transform = (o.affine() * pa * src.affine()).as_coeffs();
            o.kind = kind;
            o.layer = src.layer;
        }
        Ok(targets.len())
    }

    /// Kopia obiektu, w której wszystkie klony (także zagnieżdżone) są zastąpione
    /// samodzielną geometrią źródła — do schowka i eksportu.
    pub fn materialize(&self, o: &Object) -> Result<Object, DocError> {
        self.materialize_at(o, 0)
    }

    fn materialize_at(&self, o: &Object, depth: u8) -> Result<Object, DocError> {
        if depth > MAX_CLONE_DEPTH {
            return Err(DocError::Invalid(format!(
                "zbyt głębokie odwołania klonów przy obiekcie {}",
                o.id
            )));
        }
        let mut out = o.clone();
        match &o.kind {
            Kind::Clone { source } => {
                let src = self.find(*source).ok_or_else(|| {
                    DocError::Invalid(format!("klon {} bez źródła {source}", o.id))
                })?;
                let m = self.materialize_at(src, depth + 1)?;
                let pa = self.parent_affine(*source).unwrap_or(Affine::IDENTITY);
                out.transform = (o.affine() * pa * m.affine()).as_coeffs();
                out.layer = m.layer;
                out.kind = m.kind;
            }
            Kind::Group { children } | Kind::Array { children, .. } => {
                let kids = children
                    .iter()
                    .map(|c| self.materialize_at(c, depth))
                    .collect::<Result<Vec<_>, _>>()?;
                if let Some(ch) = out.children_mut() {
                    *ch = kids;
                }
            }
            _ => {}
        }
        Ok(out)
    }

    /// Lista, w której leży obiekt: `None` = najwyższy poziom, `Some(rodzic)` = dzieci kontenera.
    pub fn parent_of(&self, id: ObjectId) -> Option<Option<ObjectId>> {
        fn rec(
            list: &[Object],
            id: ObjectId,
            parent: Option<ObjectId>,
        ) -> Option<Option<ObjectId>> {
            for o in list {
                if o.id == id {
                    return Some(parent);
                }
                if let Some(p) = rec(o.children(), id, Some(o.id)) {
                    return Some(p);
                }
            }
            None
        }
        rec(&self.objects, id, None)
    }

    /// Lista obiektów rodzica (`None` = najwyższy poziom). `None`, gdy rodzic nie jest kontenerem.
    pub fn container_mut(&mut self, parent: Option<ObjectId>) -> Option<&mut Vec<Object>> {
        match parent {
            None => Some(&mut self.objects),
            Some(p) => self.find_mut(p)?.children_mut(),
        }
    }

    pub fn container(&self, parent: Option<ObjectId>) -> Option<&[Object]> {
        match parent {
            None => Some(&self.objects),
            Some(p) => {
                let o = self.find(p)?;
                o.kind.is_container().then(|| o.children())
            }
        }
    }

    /// Rozwija geometrię obiektu do ścieżek świata (grupy, kopie tablic, klony).
    fn collect(
        &self,
        o: &Object,
        parent: Affine,
        depth: u8,
        out: &mut Vec<(u8, ObjectId, BezPath)>,
    ) -> Result<(), DocError> {
        let a = parent * o.affine();
        match &o.kind {
            Kind::Group { children } => {
                for c in children {
                    self.collect(c, a, depth, out)?;
                }
            }
            Kind::Array { children, .. } => {
                for t in self.array_copies_at(o, depth)? {
                    for c in children {
                        self.collect(c, a * t, depth, out)?;
                    }
                }
            }
            Kind::Clone { source } => {
                if depth >= MAX_CLONE_DEPTH {
                    return Err(DocError::Invalid(format!(
                        "cykl lub zbyt głębokie odwołania klonów (obiekt {})",
                        o.id
                    )));
                }
                let src = self.find(*source).ok_or_else(|| {
                    DocError::Invalid(format!(
                        "klon {} wskazuje nieistniejący obiekt {source}",
                        o.id
                    ))
                })?;
                let pa = self.parent_affine(*source).unwrap_or(Affine::IDENTITY);
                self.collect(src, a * pa, depth + 1, out)?;
            }
            _ => {
                let mut p = o.local_path()?;
                p.apply_affine(a);
                out.push((o.layer, o.id, p));
            }
        }
        Ok(())
    }

    /// Transformacje kopii wirtualnej tablicy (w jej układzie lokalnym; pierwsza = oryginał).
    pub fn array_copies(&self, array: &Object) -> Result<Vec<Affine>, DocError> {
        self.array_copies_at(array, 0)
    }

    fn array_copies_at(&self, array: &Object, depth: u8) -> Result<Vec<Affine>, DocError> {
        let Kind::Array { children, layout } = &array.kind else {
            return Err(DocError::Invalid(format!(
                "obiekt {} nie jest tablicą",
                array.id
            )));
        };
        let mut src = Vec::new();
        for c in children {
            self.collect(c, Affine::IDENTITY, depth, &mut src)?;
        }
        let anchor = src
            .iter()
            .filter_map(|(_, _, p)| bc_geom::bbox(p))
            .reduce(|x, y| x.union(y))
            .map(|r| r.center())
            .unwrap_or(Point::ORIGIN);
        layout.transforms(anchor)
    }

    /// Ścieżki świata jednego obiektu (z dowolnego poziomu drzewa).
    pub fn paths_of(&self, id: ObjectId) -> Result<Vec<(u8, ObjectId, BezPath)>, DocError> {
        let o = self.find(id).ok_or(DocError::NotFound(id))?;
        let mut out = Vec::new();
        self.collect(
            o,
            self.parent_affine(id).unwrap_or(Affine::IDENTITY),
            0,
            &mut out,
        )?;
        Ok(out)
    }

    /// Bbox obiektu we współrzędnych świata.
    pub fn bbox_of(&self, id: ObjectId) -> Result<Option<Rect>, DocError> {
        Ok(self
            .paths_of(id)?
            .iter()
            .filter_map(|(_, _, p)| bc_geom::bbox(p))
            .reduce(|a, b| a.union(b)))
    }

    /// Transformacja świata rodzica obiektu (iloczyn transformacji grup nad nim).
    pub fn parent_affine(&self, id: ObjectId) -> Option<Affine> {
        fn rec(list: &[Object], id: ObjectId, acc: Affine) -> Option<Affine> {
            for o in list {
                if o.id == id {
                    return Some(acc);
                }
                if let Some(a) = rec(o.children(), id, acc * o.affine()) {
                    return Some(a);
                }
            }
            None
        }
        rec(&self.objects, id, Affine::IDENTITY)
    }

    /// Czy obiekt jest na najwyższym poziomie (nie w grupie).
    pub fn is_top_level(&self, id: ObjectId) -> bool {
        self.objects.iter().any(|o| o.id == id)
    }

    /// Wszystkie widoczne ścieżki liściowe we współrzędnych świata.
    pub fn world_paths(&self) -> Result<Vec<(u8, ObjectId, BezPath)>, DocError> {
        let mut out = Vec::new();
        for o in self.objects.iter().filter(|o| o.visible) {
            self.collect(o, Affine::IDENTITY, 0, &mut out)?;
        }
        Ok(out)
    }

    pub fn bbox(&self) -> Result<Option<Rect>, DocError> {
        Ok(self
            .world_paths()?
            .iter()
            .filter_map(|(_, _, p)| bc_geom::bbox(p))
            .reduce(|a, b| a.union(b)))
    }

    /// Sprawdza niezmienniki, których nie wyraża schemat JSON.
    pub fn validate(&self) -> Result<(), DocError> {
        let mut seen = std::collections::HashSet::new();
        let mut layer_idx = std::collections::HashSet::new();
        for l in &self.layers {
            if !layer_idx.insert(l.index) {
                return Err(DocError::Invalid(format!("zdublowana warstwa {}", l.index)));
            }
            l.params
                .validate()
                .map_err(|e| DocError::Invalid(format!("warstwa {}: {e}", l.index)))?;
        }
        self.plan
            .validate()
            .map_err(|e| DocError::Invalid(format!("ustawienia planu: {e}")))?;
        self.gcode
            .validate()
            .map_err(|e| DocError::Invalid(format!("ustawienia G-code: {e}")))?;
        for o in self.all_objects() {
            if !seen.insert(o.id) {
                return Err(DocError::Invalid(format!("zdublowane id obiektu {}", o.id)));
            }
            if o.id >= self.next_id {
                return Err(DocError::Invalid(format!(
                    "id {} >= next_id {}",
                    o.id, self.next_id
                )));
            }
            if !layer_idx.contains(&o.layer) {
                return Err(DocError::Invalid(format!(
                    "obiekt {} wskazuje nieistniejącą warstwę {}",
                    o.id, o.layer
                )));
            }
            if o.transform.iter().any(|v| !v.is_finite()) {
                return Err(DocError::Invalid(format!(
                    "obiekt {}: nieskończona transformacja",
                    o.id
                )));
            }
            o.local_path()?;
            if let Kind::Clone { source } = o.kind {
                self.find(source).ok_or_else(|| {
                    DocError::Invalid(format!(
                        "klon {} wskazuje nieistniejący obiekt {source}",
                        o.id
                    ))
                })?;
            }
        }
        // cykle klonów, przekroczenie limitu kopii i błędy układu tablic
        for (_, id, p) in self.world_paths()? {
            if !bc_geom::within_limits(&p) {
                return Err(DocError::Invalid(format!(
                    "obiekt {id}: geometria poza zakresem ±{} mm",
                    bc_geom::MAX_COORD
                )));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn req_02_023_object_layer_assignment() {
        let mut d = Document::new("t");
        let id = d
            .add(
                3,
                Kind::Rect {
                    x: 0.,
                    y: 0.,
                    w: 10.,
                    h: 10.,
                    radius: 0.,
                },
            )
            .unwrap();
        assert_eq!(d.find(id).unwrap().layer, 3);
        assert!(d
            .add(
                200,
                Kind::Rect {
                    x: 0.,
                    y: 0.,
                    w: 1.,
                    h: 1.,
                    radius: 0.
                }
            )
            .is_err());
    }

    #[test]
    fn req_02_024_transform_not_baked() {
        let mut d = Document::new("t");
        let id = d
            .add(
                0,
                Kind::Rect {
                    x: 0.,
                    y: 0.,
                    w: 10.,
                    h: 10.,
                    radius: 0.,
                },
            )
            .unwrap();
        d.find_mut(id).unwrap().transform = Affine::translate((5., 7.)).as_coeffs();
        // geometria lokalna bez zmian, świat przesunięty
        assert!(matches!(d.find(id).unwrap().kind, Kind::Rect { x, y, .. } if x == 0. && y == 0.));
        let b = d.bbox().unwrap().unwrap();
        assert_eq!((b.x0, b.y0, b.x1, b.y1), (5., 7., 15., 17.));
    }

    #[test]
    fn req_02_018_group_nested_transform() {
        let mut d = Document::new("t");
        let child = Object::new(
            d.alloc_id(),
            0,
            Kind::Ellipse {
                cx: 0.,
                cy: 0.,
                rx: 1.,
                ry: 1.,
            },
        );
        let gid = d.alloc_id();
        let mut g = Object::new(
            gid,
            0,
            Kind::Group {
                children: vec![child.clone()],
            },
        );
        g.transform = Affine::translate((100., 0.)).as_coeffs();
        d.objects.push(g);
        assert!(d.find(child.id).is_some());
        assert_eq!(
            d.parent_affine(child.id).unwrap(),
            Affine::translate((100., 0.))
        );
        let b = d.bbox().unwrap().unwrap();
        assert!((b.center().x - 100.).abs() < 1e-9);
        d.remove(child.id).unwrap();
        assert!(d.find(child.id).is_none());
    }

    #[test]
    fn req_02_027_visibility_excludes_from_world() {
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
        d.find_mut(id).unwrap().visible = false;
        assert!(d.world_paths().unwrap().is_empty());
    }

    #[test]
    fn req_02_028_object_name() {
        let mut d = Document::new("t");
        let id = d
            .add(
                0,
                Kind::Path {
                    d: "M0 0 L1 1".into(),
                },
            )
            .unwrap();
        d.find_mut(id).unwrap().name = Some("Logo".into());
        let json = serde_json::to_value(&d).unwrap();
        assert_eq!(json["objects"][0]["name"], "Logo");
    }

    #[test]
    fn validate_rejects_duplicate_ids() {
        let mut d = Document::new("t");
        d.add(
            0,
            Kind::Path {
                d: "M0 0 L1 1".into(),
            },
        )
        .unwrap();
        let dup = d.objects[0].clone();
        d.objects.push(dup);
        assert!(d.validate().is_err());
    }
}

#[cfg(test)]
mod model_m12_tests;
#[cfg(test)]
mod model_tests;
