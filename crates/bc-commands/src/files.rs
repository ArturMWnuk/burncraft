//! Import i eksport plików jako polecenia (M1.3, ADR 0008).

use bc_io::{ExportFormat, ExportOptions, ImportOptions, LayerBy, Unit};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{ok, parse, schema_of, CmdError, CommandSpec, Handler, Registry, SafetyTier, Session};

fn invalid(m: impl Into<String>) -> CmdError {
    CmdError::new("invalid_params", m)
}

pub(crate) fn io_err(e: bc_io::IoError) -> CmdError {
    match e {
        bc_io::IoError::Unsupported(m) => invalid(format!("nieobsługiwany format: {m}")),
        other => CmdError::new("failed", other.to_string()),
    }
}

#[derive(Deserialize, JsonSchema, Default, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
enum LayerByParam {
    /// Wg koloru obrysu/wypełnienia, pióra HPGL lub koloru encji DXF.
    #[default]
    Color,
    /// Wg nazwy warstwy/grupy w pliku.
    Name,
    /// Wszystko na warstwę `layer`.
    Single,
}

#[derive(Deserialize, JsonSchema, Clone, Copy)]
#[serde(rename_all = "lowercase")]
enum UnitParam {
    Mm,
    Cm,
    In,
    Pt,
}

fn yes() -> bool {
    true
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct ImportParams {
    /// Plik .svg, .dxf, .plt/.hpgl, .lbrn2 albo G-code (.nc, .gc, .gcode).
    path: String,
    #[serde(default)]
    layer_by: LayerByParam,
    /// Warstwa dla layer_by=single.
    #[serde(default)]
    layer: Option<u8>,
    /// SVG: DPI dla px/wartości bez jednostek (domyślnie wykrywane z generatora).
    #[serde(default)]
    dpi: Option<f64>,
    /// DXF: pomiń tekst bez ostrzeżeń (domyślnie tak).
    #[serde(default = "yes")]
    ignore_text: bool,
    /// DXF bez $INSUNITS: jednostka pliku (domyślnie mm).
    #[serde(default)]
    units: Option<UnitParam>,
    /// Zgrupuj zaimportowane obiekty (domyślnie tak).
    #[serde(default = "yes")]
    group: bool,
}

#[derive(Serialize, JsonSchema)]
struct ImportResult {
    ids: Vec<u64>,
    layers_added: Vec<u8>,
    warnings: Vec<String>,
}

fn file_import(s: &mut Session, p: Value) -> Result<Value, CmdError> {
    let p: ImportParams = parse(p)?;
    // G-code: program do podglądu i uruchamiania (REQ-02-089)
    if crate::gcode::is_gcode_path(std::path::Path::new(&p.path)) {
        let layer = match (p.layer_by, p.layer) {
            (LayerByParam::Single, Some(l)) => l,
            _ => 0,
        };
        let o = crate::gcode::import_from(s, &p.path, true, layer, None, true)?;
        return ok(ImportResult {
            ids: o.ids,
            layers_added: Vec::new(),
            warnings: o.warnings,
        });
    }
    let layer_by = match (p.layer_by, p.layer) {
        (LayerByParam::Color, _) => LayerBy::Color,
        (LayerByParam::Name, _) => LayerBy::Name,
        (LayerByParam::Single, Some(l)) => {
            s.doc.layer(l).map_err(|e| invalid(e.to_string()))?;
            LayerBy::Single(l)
        }
        (LayerByParam::Single, None) => {
            return Err(invalid("layer_by=single wymaga parametru layer"))
        }
    };
    let opt = ImportOptions {
        layer_by,
        dpi: p.dpi,
        ignore_text: p.ignore_text,
        units: p.units.map(|u| match u {
            UnitParam::Mm => Unit::Mm,
            UnitParam::Cm => Unit::Cm,
            UnitParam::In => Unit::Inch,
            UnitParam::Pt => Unit::Point,
        }),
        group: p.group,
    };
    let imp = bc_io::import_file(std::path::Path::new(&p.path), &opt).map_err(io_err)?;
    let applied = bc_io::apply(&mut s.doc, &imp, &opt).map_err(io_err)?;
    s.selection = applied.ids.clone();
    ok(ImportResult {
        ids: applied.ids,
        layers_added: applied.layers_added,
        warnings: imp.warnings,
    })
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct ExportParams {
    /// Plik docelowy.
    path: String,
    /// svg, dxf (AC1021), dxf-r12 (AC1009), hpgl, gcode (GRBL); domyślnie z rozszerzenia (.gcode/.nc/.gc = G-code GRBL).
    #[serde(default)]
    format: Option<String>,
    /// Tylko te obiekty (domyślnie cały dokument).
    #[serde(default)]
    ids: Option<Vec<u64>>,
    /// Także obiekty pomocnicze i warstwy T1/T2.
    #[serde(default)]
    include_helpers: bool,
}

#[derive(Serialize, JsonSchema)]
struct ExportResult {
    path: String,
    format: String,
    bytes: usize,
}

fn file_export(s: &mut Session, p: Value) -> Result<Value, CmdError> {
    let p: ExportParams = parse(p)?;
    let path = std::path::PathBuf::from(&p.path);
    // G-code (REQ-02-090): format gcode/grbl albo rozszerzenie .gcode, .nc, .gc
    let gfmt = p.format.as_deref().map(|f| f.to_ascii_lowercase());
    if matches!(gfmt.as_deref(), Some("gcode" | "grbl" | "nc"))
        || (gfmt.is_none() && crate::gcode::is_gcode_path(&path))
    {
        if p.ids.is_some() || p.include_helpers {
            return Err(invalid(
                "eksport G-code planuje cały dokument — użyj gcode.export {selection_only}",
            ));
        }
        let driver = "grbl";
        let r = crate::gcode::export_to(s, &p.path, driver, false)?;
        return ok(ExportResult {
            path: p.path,
            format: format!("gcode-{driver}"),
            bytes: r["bytes"].as_u64().unwrap_or(0) as usize,
        });
    }
    let fmt = match &p.format {
        Some(f) => ExportFormat::parse(f)
            .ok_or_else(|| invalid(format!("nieznany format {f} (svg, dxf, dxf-r12, hpgl)")))?,
        None => ExportFormat::from_path(&path).ok_or_else(|| {
            invalid(
                "nie rozpoznano formatu z rozszerzenia — podaj format (svg, dxf, dxf-r12, hpgl)",
            )
        })?,
    };
    if let Some(ids) = &p.ids {
        s.targets(&Some(ids.clone()))?;
    }
    let bytes = bc_io::export(
        &s.doc,
        fmt,
        &ExportOptions {
            ids: p.ids,
            include_helpers: p.include_helpers,
        },
    )
    .map_err(io_err)?;
    std::fs::write(&path, &bytes)
        .map_err(|e| CmdError::new("failed", format!("{}: {e}", path.display())))?;
    ok(ExportResult {
        path: p.path,
        format: fmt.name().into(),
        bytes: bytes.len(),
    })
}

pub(crate) fn register_all(r: &mut Registry) {
    let add = |r: &mut Registry, spec: CommandSpec, h: Handler| r.register(spec, h);
    add(
        r,
        CommandSpec {
            executor: crate::Executor::Document,
            id: "file.import",
            title: "Importuj",
            description: "Importuje SVG, DXF, HPGL/PLT lub .lbrn2 do bieżącego dokumentu (warstwy wg koloru, nazwy albo jednej warstwy). Pliki G-code (.nc, .gc, .gcode) wczytuje jak gcode.import: tor z wiązką jako ścieżki i program do uruchomienia.",
            params_schema: schema_of::<ImportParams>(),
            result_schema: schema_of::<ImportResult>(),
            undoable: true,
            requires_device: false,
            safety_tier: SafetyTier::Design,
            shortcut: Some("Ctrl+I"),
            requirements: &["REQ-02-062", "REQ-02-064", "REQ-02-065", "REQ-02-066", "REQ-02-068", "REQ-02-069", "REQ-02-070", "REQ-02-071", "REQ-02-072", "REQ-02-073", "REQ-02-080", "REQ-02-092", "REQ-15-029", "REQ-02-089"],
        },
        file_import,
    );
    add(
        r,
        CommandSpec {
            executor: crate::Executor::Document,
            id: "file.export",
            title: "Eksportuj",
            description: "Eksportuje dokument lub zaznaczone obiekty do SVG (mm), DXF AC1021/AC1009 albo HPGL. Pliki .gcode/.nc/.gc (lub format gcode) zapisuje jak gcode.export (sterownik grbl).",
            params_schema: schema_of::<ExportParams>(),
            result_schema: schema_of::<ExportResult>(),
            undoable: false,
            requires_device: false,
            safety_tier: SafetyTier::Design,
            shortcut: Some("Ctrl+E"),
            requirements: &["REQ-02-063", "REQ-02-074", "REQ-02-075", "REQ-02-081", "REQ-15-030", "REQ-02-090"],
        },
        file_export,
    );
}
