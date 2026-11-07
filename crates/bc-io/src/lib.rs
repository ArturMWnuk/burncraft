//! bc-io — import i eksport plików (ADR 0008).
//!
//! Importery zwracają [`Imported`] (geometria w mm, oś Y w górę, klucze warstw,
//! ostrzeżenia); [`apply`] wstawia wynik do dokumentu i dobiera warstwy.

pub mod dxf_io;
pub mod hpgl;
pub mod lbrn;
pub mod svg;

use std::path::Path;

use bc_doc::{Document, Kind, Layer, Object, ObjectId, DEFAULT_PALETTE};
use bc_geom::{Affine, BezPath};

/// Nazwa modułu, używana przez `burncraft-cli version`.
pub const MODULE: &str = "bc-io";

#[derive(Debug, thiserror::Error)]
pub enum IoError {
    #[error("nieobsługiwany format: {0}")]
    Unsupported(String),
    #[error("niepoprawny plik: {0}")]
    Parse(String),
    #[error("błąd pliku: {0}")]
    Io(String),
    #[error("{0}")]
    Doc(#[from] bc_doc::DocError),
}

/// Jak przypisywać importowane obiekty do warstw.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum LayerBy {
    /// Wg koloru obrysu/wypełnienia (SVG), pióra (HPGL) lub koloru encji (DXF).
    #[default]
    Color,
    /// Wg nazwy warstwy/grupy w pliku (SVG: inkscape:label/id, DXF: warstwa).
    Name,
    /// Wszystko na jedną warstwę.
    Single(u8),
}

/// Jednostka długości plików bez jednostek.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Unit {
    Mm,
    Cm,
    Inch,
    Point,
}

impl Unit {
    pub fn mm(self) -> f64 {
        match self {
            Unit::Mm => 1.0,
            Unit::Cm => 10.0,
            Unit::Inch => 25.4,
            Unit::Point => 25.4 / 72.0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ImportOptions {
    pub layer_by: LayerBy,
    /// SVG: wymuszone DPI dla px / wartości bez jednostek (inaczej auto-DPI).
    pub dpi: Option<f64>,
    /// DXF: pomiń tekst (TEXT/MTEXT) bez ostrzeżeń.
    pub ignore_text: bool,
    /// DXF bez $INSUNITS: przyjmowana jednostka (domyślnie mm).
    pub units: Option<Unit>,
    /// Zgrupuj zaimportowane obiekty.
    pub group: bool,
}

impl Default for ImportOptions {
    fn default() -> Self {
        ImportOptions {
            layer_by: LayerBy::Color,
            dpi: None,
            ignore_text: true,
            units: None,
            group: true,
        }
    }
}

/// Klucz warstwy importowanego elementu.
#[derive(Debug, Clone, PartialEq)]
pub enum LayerKey {
    Index(u8),
    /// Kolor `#rrggbb` (małe litery).
    Color(String),
    Name(String),
    /// Pióro HPGL (1 = pierwsze).
    Pen(u32),
}

/// Element do wstawienia: geometria (mm, Y w górę) i transformacja.
#[derive(Debug, Clone)]
pub struct Item {
    pub kind: Kind,
    pub transform: Affine,
    /// Klucz wg koloru.
    pub color_key: Option<LayerKey>,
    /// Klucz wg nazwy.
    pub name_key: Option<LayerKey>,
}

/// Definicja warstwy niesiona przez plik (np. CutSetting w .lbrn2).
#[derive(Debug, Clone, PartialEq)]
pub struct LayerDef {
    pub index: u8,
    pub name: Option<String>,
    pub color: Option<String>,
    pub speed: Option<f64>,
    pub power_max: Option<f64>,
    pub power_min: Option<f64>,
}

#[derive(Debug, Clone, Default)]
pub struct Imported {
    pub items: Vec<Item>,
    pub layers: Vec<LayerDef>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Applied {
    /// Nowe obiekty najwyższego poziomu (grupa albo elementy).
    pub ids: Vec<ObjectId>,
    /// Utworzone warstwy (indeksy).
    pub layers_added: Vec<u8>,
}

/// Wstawia wynik importu do dokumentu.
pub fn apply(doc: &mut Document, imp: &Imported, opt: &ImportOptions) -> Result<Applied, IoError> {
    if let LayerBy::Single(i) = opt.layer_by {
        doc.layer(i)?;
    }
    let mut added = Vec::new();
    for def in &imp.layers {
        if doc.layer(def.index).is_err() {
            doc.layers.push(Layer::default_for(def.index));
            added.push(def.index);
        }
        let l = doc.layer_mut(def.index)?;
        if let Some(n) = &def.name {
            l.name = n.clone();
        }
        if let Some(c) = &def.color {
            l.color = c.clone();
        }
        if let Some(v) = def.speed.filter(|v| v.is_finite() && *v > 0.0) {
            l.speed = v;
        }
        if let Some(v) = def.power_max.filter(|v| (0.0..=100.0).contains(v)) {
            l.power_max = v;
        }
        if let Some(v) = def.power_min.filter(|v| (0.0..=100.0).contains(v)) {
            l.power_min = v.min(l.power_max);
        }
    }
    let mut objs = Vec::new();
    for it in &imp.items {
        let key = match &opt.layer_by {
            LayerBy::Color => it.color_key.clone().or_else(|| it.name_key.clone()),
            LayerBy::Name => it.name_key.clone().or_else(|| it.color_key.clone()),
            LayerBy::Single(i) => Some(LayerKey::Index(*i)),
        };
        let layer = match key {
            Some(k) => resolve_layer(doc, &k, &mut added)?,
            None => 0,
        };
        let mut o = Object::new(doc.alloc_id(), layer, it.kind.clone());
        o.transform = it.transform.as_coeffs();
        // dzieci grup z importu (np. .lbrn2) dostają identyfikatory dokumentu i istniejące warstwy
        bc_doc::renumber_kind(&mut o.kind, &mut doc.next_id);
        ensure_child_layers(doc, &o, &mut added);
        objs.push(o);
    }
    let ids = if opt.group && !objs.is_empty() {
        let layer = objs[0].layer;
        let gid = doc.alloc_id();
        doc.objects
            .push(Object::new(gid, layer, Kind::Group { children: objs }));
        vec![gid]
    } else {
        let ids = objs.iter().map(|o| o.id).collect();
        doc.objects.extend(objs);
        ids
    };
    Ok(Applied {
        ids,
        layers_added: added,
    })
}

fn ensure_child_layers(doc: &mut Document, o: &Object, added: &mut Vec<u8>) {
    for c in o.children() {
        if doc.layer(c.layer).is_err() {
            doc.layers.push(Layer::default_for(c.layer));
            added.push(c.layer);
        }
        ensure_child_layers(doc, c, added);
    }
}

fn next_custom_index(doc: &Document) -> Result<u8, IoError> {
    doc.layers
        .iter()
        .map(|l| l.index)
        .filter(|i| *i >= bc_doc::FIRST_CUSTOM_INDEX)
        .max()
        .map_or(Some(bc_doc::FIRST_CUSTOM_INDEX), |m| m.checked_add(1))
        .ok_or_else(|| IoError::Parse("przekroczono limit 256 warstw".into()))
}

fn resolve_layer(doc: &mut Document, key: &LayerKey, added: &mut Vec<u8>) -> Result<u8, IoError> {
    let idx = match key {
        LayerKey::Index(i) => {
            if doc.layer(*i).is_err() {
                doc.layers.push(Layer::default_for(*i));
                added.push(*i);
            }
            *i
        }
        LayerKey::Pen(n) => {
            return resolve_layer(
                doc,
                &LayerKey::Index((n.saturating_sub(1)).min(255) as u8),
                added,
            )
        }
        LayerKey::Color(c) => match doc.layers.iter().find(|l| l.color.eq_ignore_ascii_case(c)) {
            Some(l) => l.index,
            None => {
                let i = next_custom_index(doc)?;
                doc.layers.push(Layer {
                    color: c.to_lowercase(),
                    name: c.to_lowercase(),
                    ..Layer::default_for(i)
                });
                added.push(i);
                i
            }
        },
        LayerKey::Name(n) => match doc
            .layers
            .iter()
            .find(|l| l.name.to_lowercase() == n.to_lowercase())
        {
            Some(l) => l.index,
            None => {
                let i = next_custom_index(doc)?;
                let color = DEFAULT_PALETTE[i as usize % DEFAULT_PALETTE.len()].to_string();
                doc.layers.push(Layer {
                    name: n.clone(),
                    color,
                    ..Layer::default_for(i)
                });
                added.push(i);
                i
            }
        },
    };
    Ok(idx)
}

/// Limit surowych wartości w plikach wejściowych (przed przeliczeniem jednostek).
/// Większe liczby to dane uszkodzone lub złośliwe (fuzzing: brak pamięci).
pub(crate) const RAW_LIMIT: f64 = 1.0e8;

/// Usuwa elementy, których geometria wychodzi poza ±[`bc_geom::MAX_COORD`] mm.
pub(crate) fn retain_in_range(out: &mut Imported) {
    let before = out.items.len();
    out.items.retain(|it| {
        let mut d = Document::new("zakres");
        let mut o = Object::new(1, 0, it.kind.clone());
        o.transform = it.transform.as_coeffs();
        bc_doc::renumber_kind(&mut o.kind, &mut 2);
        for c in o.children() {
            if d.layer(c.layer).is_err() {
                d.layers.push(Layer::default_for(c.layer));
            }
        }
        d.objects.push(o);
        d.next_id = u64::MAX / 2;
        match d.paths_of(1) {
            Ok(paths) => paths.iter().all(|(_, _, p)| bc_geom::within_limits(p)),
            Err(_) => false,
        }
    });
    let dropped = before - out.items.len();
    if dropped > 0 {
        out.warnings.push(format!(
            "pominięto {dropped} elementów z geometrią poza zakresem ±{} mm",
            bc_geom::MAX_COORD
        ));
    }
}

/// Kolor `#rrggbb` z kanałów RGB.
pub(crate) fn hex(r: u8, g: u8, b: u8) -> String {
    format!("#{r:02x}{g:02x}{b:02x}")
}

/// Geometria do eksportu: (warstwa, ścieżki świata) w kolejności warstw.
pub(crate) fn export_paths(
    doc: &Document,
    opt: &ExportOptions,
) -> Result<Vec<(Layer, Vec<BezPath>)>, IoError> {
    let mut leaves: Vec<(u8, ObjectId, BezPath)> = match &opt.ids {
        None => doc.world_paths()?,
        Some(ids) => {
            let mut v = Vec::new();
            for id in ids {
                v.extend(doc.paths_of(*id)?);
            }
            v
        }
    };
    if !opt.include_helpers {
        leaves.retain(|(layer, id, _)| {
            let helper = doc.find(*id).is_some_and(|o| o.helper);
            let tool = doc
                .layer(*layer)
                .map(|l| l.role != bc_doc::LayerRole::Cut)
                .unwrap_or(false);
            !helper && !tool
        });
    }
    let mut out: Vec<(Layer, Vec<BezPath>)> = Vec::new();
    for l in &doc.layers {
        let paths: Vec<BezPath> = leaves
            .iter()
            .filter(|(li, _, _)| *li == l.index)
            .map(|(_, _, p)| p.clone())
            .collect();
        if !paths.is_empty() {
            out.push((l.clone(), paths));
        }
    }
    Ok(out)
}

/// Format pliku wg rozszerzenia.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Svg,
    Dxf,
    Hpgl,
    Lbrn,
    Lbrn2,
}

impl Format {
    pub fn from_path(p: &Path) -> Option<Format> {
        match p.extension()?.to_str()?.to_ascii_lowercase().as_str() {
            "svg" => Some(Format::Svg),
            "dxf" => Some(Format::Dxf),
            "plt" | "hpgl" | "hpg" => Some(Format::Hpgl),
            "lbrn" => Some(Format::Lbrn),
            "lbrn2" => Some(Format::Lbrn2),
            _ => None,
        }
    }
}

/// Importuje plik dowolnego obsługiwanego formatu.
pub fn import_file(path: &Path, opt: &ImportOptions) -> Result<Imported, IoError> {
    let fmt = Format::from_path(path).ok_or_else(|| {
        IoError::Unsupported(format!(
            "{} (obsługiwane: svg, dxf, plt/hpgl, lbrn2)",
            path.display()
        ))
    })?;
    let data = std::fs::read(path).map_err(|e| IoError::Io(format!("{}: {e}", path.display())))?;
    match fmt {
        Format::Svg => svg::import(&data, opt),
        Format::Dxf => dxf_io::import(&data, opt),
        Format::Hpgl => hpgl::import(&data, opt),
        Format::Lbrn2 => lbrn::import_lbrn2(&data, opt),
        Format::Lbrn => lbrn::import_lbrn(&data, opt),
    }
}

/// Format eksportu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportFormat {
    Svg,
    /// DXF AC1009 (R12).
    DxfR12,
    /// DXF AC1021 (R2007).
    Dxf2007,
    Hpgl,
}

impl ExportFormat {
    /// `svg`, `dxf` (= R2007), `dxf-r12`, `dxf-2007`, `hpgl`/`plt`.
    pub fn parse(name: &str) -> Option<ExportFormat> {
        match name.to_ascii_lowercase().as_str() {
            "svg" => Some(ExportFormat::Svg),
            "dxf" | "dxf-2007" | "dxf2007" | "ac1021" => Some(ExportFormat::Dxf2007),
            "dxf-r12" | "dxfr12" | "r12" | "ac1009" => Some(ExportFormat::DxfR12),
            "hpgl" | "plt" => Some(ExportFormat::Hpgl),
            _ => None,
        }
    }

    pub fn from_path(p: &Path) -> Option<ExportFormat> {
        match p.extension()?.to_str()?.to_ascii_lowercase().as_str() {
            "svg" => Some(ExportFormat::Svg),
            "dxf" => Some(ExportFormat::Dxf2007),
            "plt" | "hpgl" | "hpg" => Some(ExportFormat::Hpgl),
            _ => None,
        }
    }

    /// Nazwa formatu (do raportów).
    pub fn name(self) -> &'static str {
        match self {
            ExportFormat::Svg => "svg",
            ExportFormat::DxfR12 => "dxf-r12",
            ExportFormat::Dxf2007 => "dxf",
            ExportFormat::Hpgl => "hpgl",
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct ExportOptions {
    /// Tylko te obiekty (z potomkami); `None` = cały dokument.
    pub ids: Option<Vec<ObjectId>>,
    /// Eksportuj także obiekty pomocnicze.
    pub include_helpers: bool,
}

/// Eksportuje dokument do bajtów w danym formacie.
pub fn export(doc: &Document, fmt: ExportFormat, opt: &ExportOptions) -> Result<Vec<u8>, IoError> {
    Ok(match fmt {
        ExportFormat::Svg => svg::export(doc, opt)?.into_bytes(),
        ExportFormat::DxfR12 => dxf_io::export(doc, true, opt)?,
        ExportFormat::Dxf2007 => dxf_io::export(doc, false, opt)?,
        ExportFormat::Hpgl => hpgl::export(doc, opt)?.into_bytes(),
    })
}
