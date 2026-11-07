//! Raport stanu dokumentu (`document.inspect`, narzędzie MCP `inspect_document`).

use bc_doc::{Kind, Layer, Object};
use bc_geom::Affine;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{CmdError, Session};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Detail {
    /// Warstwy używane przez obiekty, obiekty bez danych geometrii.
    #[default]
    Summary,
    /// Wszystkie warstwy z ustawieniami i obiekty z danymi geometrii.
    Full,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct ObjectReport {
    pub id: u64,
    #[serde(rename = "type")]
    pub kind: String,
    pub name: Option<String>,
    pub layer: u8,
    /// Id grupy nadrzędnej (brak = najwyższy poziom).
    pub parent: Option<u64>,
    pub visible: bool,
    pub locked: bool,
    /// [x0, y0, x1, y1] w mm, współrzędne świata.
    pub bbox: Option<[f64; 4]>,
    /// Macierz afiniczna [a, b, c, d, e, f].
    pub transform: [f64; 6],
    /// Długość ścieżki w mm (dla grup: suma dzieci).
    pub length: f64,
    pub closed: bool,
    pub nodes: usize,
    /// Obiekt pomocniczy (niewypalany).
    pub helper: bool,
    /// Źródło klonu.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<u64>,
    /// Dzieci grupy.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub children: Option<Vec<u64>>,
    /// Parametry geometrii (tylko detail=full).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub geometry: Option<serde_json::Value>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct LayerReport {
    pub index: u8,
    pub color: String,
    pub name: String,
    pub objects: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub settings: Option<serde_json::Value>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct Stats {
    pub objects: usize,
    pub top_level: usize,
    pub nodes: usize,
    /// Suma długości ścieżek w mm.
    pub total_length: f64,
    /// [x0, y0, x1, y1] wszystkich widocznych obiektów.
    pub bbox: Option<[f64; 4]>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct DocumentReport {
    pub name: String,
    pub units: String,
    /// Układ: mm, Y w górę, (0,0) = lewy dolny róg obszaru roboczego.
    pub coordinate_system: String,
    pub workspace: [f64; 2],
    pub file: Option<String>,
    pub modified: bool,
    pub layers: Vec<LayerReport>,
    pub objects: Vec<ObjectReport>,
    pub selection: Vec<u64>,
    pub can_undo: bool,
    pub can_redo: bool,
    pub stats: Stats,
}

fn rect4(r: bc_geom::Rect) -> [f64; 4] {
    [r.x0, r.y0, r.x1, r.y1]
}

fn report_object(
    doc: &bc_doc::Document,
    o: &Object,
    parent: Option<u64>,
    pa: Affine,
    detail: Detail,
    out: &mut Vec<ObjectReport>,
) -> Result<(), CmdError> {
    let mut paths = Vec::new();
    o.world_paths(doc, pa, &mut paths)?;
    let length = paths.iter().map(|(_, _, p)| bc_geom::path_length(p)).sum();
    let nodes = paths.iter().map(|(_, _, p)| bc_geom::node_count(p)).sum();
    let closed = !paths.is_empty() && paths.iter().all(|(_, _, p)| bc_geom::is_closed(p));
    let geometry = (detail == Detail::Full && !matches!(o.kind, Kind::Group { .. })).then(|| {
        let mut v = serde_json::to_value(&o.kind).unwrap_or_default();
        if let Some(m) = v.as_object_mut() {
            m.remove("type");
        }
        v
    });
    out.push(ObjectReport {
        id: o.id,
        kind: o.kind.type_name().into(),
        name: o.name.clone(),
        layer: o.layer,
        parent,
        visible: o.visible,
        locked: o.locked,
        bbox: o.world_bbox(doc, pa)?.map(rect4),
        transform: o.transform,
        length,
        closed,
        helper: o.helper,
        source: match o.kind {
            Kind::Clone { source } => Some(source),
            _ => None,
        },
        nodes,
        children: matches!(o.kind, Kind::Group { .. })
            .then(|| o.children().iter().map(|c| c.id).collect()),
        geometry,
    });
    let a = pa * o.affine();
    for c in o.children() {
        report_object(doc, c, Some(o.id), a, detail, out)?;
    }
    Ok(())
}

pub fn report(s: &Session, detail: Detail) -> Result<DocumentReport, CmdError> {
    let d = &s.doc;
    let mut objects = Vec::new();
    for o in &d.objects {
        report_object(d, o, None, Affine::IDENTITY, detail, &mut objects)?;
    }
    let count = |l: &Layer| {
        objects
            .iter()
            .filter(|o| o.layer == l.index && o.kind != "group")
            .count()
    };
    let layers = d
        .layers
        .iter()
        .filter(|l| detail == Detail::Full || count(l) > 0)
        .map(|l| LayerReport {
            index: l.index,
            color: l.color.clone(),
            name: l.name.clone(),
            objects: count(l),
            settings: (detail == Detail::Full).then(|| serde_json::to_value(l).unwrap_or_default()),
        })
        .collect();
    let leaves: Vec<_> = objects.iter().filter(|o| o.kind != "group").collect();
    let stats = Stats {
        objects: objects.len(),
        top_level: d.objects.len(),
        nodes: leaves.iter().map(|o| o.nodes).sum(),
        total_length: leaves.iter().map(|o| o.length).sum(),
        bbox: d.bbox()?.map(rect4),
    };
    Ok(DocumentReport {
        name: d.name.clone(),
        units: d.units.clone(),
        coordinate_system: "mm; Y w górę; (0,0) = lewy dolny róg obszaru roboczego".into(),
        workspace: [d.workspace.width, d.workspace.height],
        file: s.path.as_ref().map(|p| p.display().to_string()),
        modified: s.dirty,
        layers,
        objects,
        selection: s.selection.clone(),
        can_undo: s.history.can_undo(),
        can_redo: s.history.can_redo(),
        stats,
    })
}
