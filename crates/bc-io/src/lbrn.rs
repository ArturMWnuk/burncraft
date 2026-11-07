//! Import projektów `.lbrn2` (FormatVersion 1) i `.lbrn` (FormatVersion 0)
//! — clean-room, ADR 0008.
//!
//! Format ustalony wyłącznie z plików dostarczonych przez użytkownika
//! (`tests/fixtures/*.lbrn*`, AppVersion 2.0.05):
//! - `CutSetting`: `index`, `name`, `maxPower`, `minPower`, `speed` (atrybut `Value`),
//! - każdy `Shape` ma `CutIndex` i `XForm` = macierz `a b c d e f` (oś Y w górę — potwierdzone
//!   konturem litery „L” w `text` i miniaturami `krzywe`/`hex`),
//! - `Rect`: `W`, `H`, `Cr`, wyśrodkowany; `Ellipse`: `Rx`, `Ry`, wyśrodkowana,
//! - `Polygon`: `Rx`, `Ry`, `N`, wierzchołki `(Rx·cos θk, Ry·sin θk)`, θk = 2πk/N od 0°,
//! - `Path`: wierzchołki z uchwytami `c0` (wychodzący) i `c1` (wchodzący) oraz prymitywy
//!   `L i j` (odcinek) / `B i j` (Bézier: vi, c0(vi), c1(vj), vj). W `.lbrn2` zapis zwięzły
//!   (`VertList`: `Vx yc0x…c0y…c1x…c1y…`, `c0x1` = brak uchwytu, `S` = węzeł gładki;
//!   `PrimList`: `L0 1B1 2…`), w `.lbrn` — elementy `<V vx vy c0x c0y c1x c1y>` i `<P T p0 p1>`,
//! - `Group`: dzieci w `Children`, ich `XForm` względny wobec grupy,
//! - `Text`: `Str`, `Font` i `BackupPath` (kontury liter jako Path) z `XForm` **bezwzględnym**
//!   — importowany jako krzywe.
//!
//! Inne typy kształtów są zgłaszane jako ostrzeżenia, bez zgadywania formatu.

use bc_doc::{Document, Kind, Object};
use bc_geom::{Affine, BezPath, Point};

use crate::{apply, ImportOptions, Imported, IoError, Item, LayerBy, LayerDef, LayerKey};

type Node<'a> = roxmltree::Node<'a, 'a>;

fn value<'a>(node: Node<'a>, tag: &str) -> Option<&'a str> {
    node.children()
        .find(|c| c.has_tag_name(tag))
        .and_then(|c| c.attribute("Value"))
}

fn num(s: Option<&str>) -> Option<f64> {
    s.and_then(|v| v.trim().parse::<f64>().ok())
        .filter(|v| v.is_finite())
}

fn xform(node: Node) -> Result<Affine, String> {
    let Some(x) = node.children().find(|c| c.has_tag_name("XForm")) else {
        return Ok(Affine::IDENTITY);
    };
    let v: Vec<f64> = x
        .text()
        .unwrap_or("")
        .split_whitespace()
        .map(|s| s.parse::<f64>())
        .collect::<Result<_, _>>()
        .map_err(|_| "XForm: niepoprawna liczba".to_string())?;
    if v.len() != 6 || v.iter().any(|f| !f.is_finite()) {
        return Err(format!("XForm: oczekiwano 6 liczb, jest {}", v.len()));
    }
    Ok(Affine::new([v[0], v[1], v[2], v[3], v[4], v[5]]))
}

/// Wierzchołek ścieżki z opcjonalnymi uchwytami.
#[derive(Clone, Copy, Debug)]
struct Vert {
    p: Point,
    c0: Option<Point>,
    c1: Option<Point>,
}

/// Zwięzły zapis `.lbrn2`: `V47 138c0x78.6c0y139.8c1x15.3c1y136.1SV52 116c0x1…`.
fn parse_vert_list(s: &str) -> Result<Vec<Vert>, String> {
    let mut out = Vec::new();
    for chunk in s.split('V').map(str::trim).filter(|c| !c.is_empty()) {
        let head_end = chunk.find(['c', 'S']).unwrap_or(chunk.len());
        let xy: Vec<f64> = chunk[..head_end]
            .split_whitespace()
            .map(|v| v.parse::<f64>())
            .collect::<Result<_, _>>()
            .map_err(|_| "VertList: niepoprawny wierzchołek".to_string())?;
        if xy.len() != 2 || xy.iter().any(|v| !v.is_finite()) {
            return Err("VertList: wierzchołek wymaga dwóch współrzędnych".into());
        }
        let field = |key: &str| -> Option<f64> {
            let i = chunk.find(key)? + key.len();
            let rest = &chunk[i..];
            let end = rest
                .find(|c: char| !(c.is_ascii_digit() || matches!(c, '.' | '-' | '+' | 'e' | 'E')))
                .unwrap_or(rest.len());
            rest[..end].parse::<f64>().ok().filter(|v| v.is_finite())
        };
        // `c0x1` bez `c0y` oznacza brak uchwytu
        let handle = |x: &str, y: &str| match (field(x), field(y)) {
            (Some(hx), Some(hy)) => Some(Point::new(hx, hy)),
            _ => None,
        };
        out.push(Vert {
            p: Point::new(xy[0], xy[1]),
            c0: handle("c0x", "c0y"),
            c1: handle("c1x", "c1y"),
        });
    }
    Ok(out)
}

/// Zwięzły zapis `.lbrn2`: `L0 1B1 2…`.
fn parse_prim_list(s: &str) -> Result<Vec<(char, usize, usize)>, String> {
    let mut out = Vec::new();
    let mut rest = s.trim();
    while let Some(t) = rest.chars().next() {
        if !matches!(t, 'L' | 'B') {
            return Err(format!("PrimList: nieznany prymityw „{t}”"));
        }
        let body_end = rest[1..]
            .find(['L', 'B'])
            .map(|i| i + 1)
            .unwrap_or(rest.len());
        let ij: Vec<usize> = rest[1..body_end]
            .split_whitespace()
            .map(|v| v.parse::<usize>())
            .collect::<Result<_, _>>()
            .map_err(|_| "PrimList: niepoprawny indeks".to_string())?;
        if ij.len() != 2 {
            return Err("PrimList: prymityw wymaga dwóch indeksów".into());
        }
        out.push((t, ij[0], ij[1]));
        rest = rest[body_end..].trim_start();
    }
    Ok(out)
}

fn attr_point(v: Node, x: &str, y: &str) -> Option<Point> {
    match (num(v.attribute(x)), num(v.attribute(y))) {
        (Some(px), Some(py)) => Some(Point::new(px, py)),
        _ => None,
    }
}

/// Geometria ścieżki z `VertList`/`PrimList` (.lbrn2) albo `<V>`/`<P>` (.lbrn).
fn path_geometry(node: Node) -> Result<BezPath, String> {
    let vl = node.children().find(|c| c.has_tag_name("VertList"));
    let pl = node.children().find(|c| c.has_tag_name("PrimList"));
    let (verts, prims) = match (vl, pl) {
        (Some(v), Some(p)) => (
            parse_vert_list(v.text().unwrap_or(""))?,
            parse_prim_list(p.text().unwrap_or(""))?,
        ),
        _ => {
            let mut verts = Vec::new();
            for v in node.children().filter(|c| c.has_tag_name("V")) {
                let p = attr_point(v, "vx", "vy").ok_or("V bez poprawnych vx/vy")?;
                verts.push(Vert {
                    p,
                    c0: attr_point(v, "c0x", "c0y"),
                    c1: attr_point(v, "c1x", "c1y"),
                });
            }
            let mut prims = Vec::new();
            for p in node.children().filter(|c| c.has_tag_name("P")) {
                let t = match p.attribute("T") {
                    Some("L") => 'L',
                    Some("B") => 'B',
                    other => return Err(format!("P: nieznany typ {other:?}")),
                };
                let idx = |k: &str| {
                    p.attribute(k)
                        .and_then(|v| v.parse::<usize>().ok())
                        .ok_or(format!("P bez poprawnego {k}"))
                };
                prims.push((t, idx("p0")?, idx("p1")?));
            }
            (verts, prims)
        }
    };
    let mut path = BezPath::new();
    let mut start: Option<usize> = None;
    let mut last: Option<usize> = None;
    for (t, i, j) in prims {
        let (Some(vi), Some(vj)) = (verts.get(i), verts.get(j)) else {
            return Err(format!(
                "prymityw {t}{i} {j} wskazuje nieistniejący wierzchołek"
            ));
        };
        if last != Some(i) {
            path.move_to(vi.p);
            start = Some(i);
        }
        match t {
            'L' => path.line_to(vj.p),
            _ => path.curve_to(vi.c0.unwrap_or(vi.p), vj.c1.unwrap_or(vj.p), vj.p),
        }
        if start == Some(j) {
            path.close_path();
            last = None;
        } else {
            last = Some(j);
        }
    }
    Ok(path)
}

struct Ctx {
    warnings: Vec<String>,
    unknown: Vec<String>,
}

/// Kształt jako (geometria, transformacja lokalna, warstwa). `parent` = transformacja świata rodzica.
fn shape(n: Node, parent: Affine, ctx: &mut Ctx) -> Option<(Kind, Affine, u8)> {
    let ty = n.attribute("Type").unwrap_or("?");
    let layer = num(n.attribute("CutIndex"))
        .filter(|i| (0.0..=255.0).contains(i))
        .unwrap_or(0.0) as u8;
    let local = match xform(n) {
        Ok(t) => t,
        Err(e) => {
            ctx.warnings.push(format!("{ty}: {e} — pominięto"));
            return None;
        }
    };
    let pos = |a: &str| num(n.attribute(a)).filter(|v| *v > 0.0);
    let kind = match ty {
        "Rect" => {
            let (Some(w), Some(h)) = (pos("W"), pos("H")) else {
                ctx.warnings
                    .push("Rect bez dodatnich W/H — pominięto".into());
                return None;
            };
            Kind::Rect {
                x: -w / 2.0,
                y: -h / 2.0,
                w,
                h,
                radius: num(n.attribute("Cr")).unwrap_or(0.0).max(0.0),
            }
        }
        "Ellipse" => {
            let (Some(rx), Some(ry)) = (pos("Rx"), pos("Ry")) else {
                ctx.warnings
                    .push("Ellipse bez dodatnich Rx/Ry — pominięto".into());
                return None;
            };
            Kind::Ellipse {
                cx: 0.0,
                cy: 0.0,
                rx,
                ry,
            }
        }
        "Polygon" => {
            let (Some(rx), Some(ry)) = (pos("Rx"), pos("Ry")) else {
                ctx.warnings
                    .push("Polygon bez dodatnich Rx/Ry — pominięto".into());
                return None;
            };
            let Some(sides) = num(n.attribute("N"))
                .filter(|v| (3.0..=10_000.0).contains(v))
                .map(|v| v as u32)
            else {
                ctx.warnings
                    .push("Polygon z niepoprawnym N — pominięto".into());
                return None;
            };
            if (rx - ry).abs() < 1e-12 {
                Kind::Polygon {
                    cx: 0.0,
                    cy: 0.0,
                    r: rx,
                    sides,
                    rotation: 0.0,
                }
            } else {
                let pts: Vec<Point> = (0..sides)
                    .map(|k| {
                        let a = std::f64::consts::TAU * k as f64 / sides as f64;
                        Point::new(rx * a.cos(), ry * a.sin())
                    })
                    .collect();
                Kind::Path {
                    d: bc_geom::polyline_path(&pts, true).to_svg(),
                }
            }
        }
        "Path" => match path_geometry(n) {
            Ok(p) if p.elements().len() > 1 => Kind::Path { d: p.to_svg() },
            Ok(_) => return None,
            Err(e) => {
                ctx.warnings.push(format!("Path: {e} — pominięto"));
                return None;
            }
        },
        "Text" => {
            let text = n.attribute("Str").unwrap_or("");
            let font = n
                .attribute("Font")
                .and_then(|f| f.split(',').next())
                .unwrap_or("?");
            let Some(bp) = n.children().find(|c| c.has_tag_name("BackupPath")) else {
                ctx.warnings.push(format!(
                    "tekst „{text}” bez konturów (BackupPath) — pominięto; edycja tekstu od M4"
                ));
                return None;
            };
            let geom = match path_geometry(bp) {
                Ok(p) if p.elements().len() > 1 => p,
                Ok(_) => return None,
                Err(e) => {
                    ctx.warnings
                        .push(format!("tekst „{text}”: {e} — pominięto"));
                    return None;
                }
            };
            // XForm konturów jest bezwzględny — przeliczamy go na układ rodzica
            let world = xform(bp).unwrap_or(Affine::IDENTITY);
            ctx.warnings.push(format!(
                "tekst „{text}” (font {font}) zaimportowany jako krzywe — edycja tekstu od M4"
            ));
            return Some((
                Kind::Path { d: geom.to_svg() },
                parent.inverse() * world,
                layer,
            ));
        }
        "Group" => {
            let world = parent * local;
            let mut children = Vec::new();
            for c in n
                .children()
                .filter(|c| c.has_tag_name("Children"))
                .flat_map(|c| c.children())
                .filter(|c| c.has_tag_name("Shape"))
            {
                if let Some((k, t, l)) = shape(c, world, ctx) {
                    let mut o = Object::new(0, l, k);
                    o.transform = t.as_coeffs();
                    children.push(o);
                }
            }
            if children.is_empty() {
                return None;
            }
            let layer = children[0].layer;
            return Some((Kind::Group { children }, local, layer));
        }
        other => {
            if !ctx.unknown.iter().any(|u| u == other) {
                ctx.unknown.push(other.to_string());
            }
            return None;
        }
    };
    Some((kind, local, layer))
}

/// Element główny XML wymagany przez format `.lbrn`/`.lbrn2`.
const ROOT_TAG: &str = "LightBurnProject";

fn import_any(data: &[u8], what: &str) -> Result<Imported, IoError> {
    let text = std::str::from_utf8(data)
        .map_err(|_| IoError::Parse(format!("{what}: plik nie jest w UTF-8")))?;
    let xml =
        roxmltree::Document::parse(text).map_err(|e| IoError::Parse(format!("{what}: {e}")))?;
    let root = xml.root_element();
    if !root.has_tag_name(ROOT_TAG) {
        return Err(IoError::Parse(format!("{what}: brak elementu {ROOT_TAG}")));
    }
    let mut out = Imported::default();
    let mut ctx = Ctx {
        warnings: Vec::new(),
        unknown: Vec::new(),
    };
    for n in root.children().filter(|n| n.is_element()) {
        match n.tag_name().name() {
            "CutSetting" => {
                let Some(index) = num(value(n, "index")).filter(|i| (0.0..=255.0).contains(i))
                else {
                    ctx.warnings
                        .push("CutSetting bez poprawnego index — pominięto".into());
                    continue;
                };
                out.layers.push(LayerDef {
                    index: index as u8,
                    name: value(n, "name").map(str::to_string),
                    color: None,
                    speed: num(value(n, "speed")),
                    power_max: num(value(n, "maxPower")),
                    power_min: num(value(n, "minPower")),
                });
            }
            "Shape" => {
                if let Some((kind, t, layer)) = shape(n, Affine::IDENTITY, &mut ctx) {
                    out.items.push(Item {
                        kind,
                        transform: t,
                        color_key: Some(LayerKey::Index(layer)),
                        name_key: Some(LayerKey::Index(layer)),
                    });
                }
            }
            _ => {}
        }
    }
    out.warnings = ctx.warnings;
    crate::retain_in_range(&mut out);
    if !ctx.unknown.is_empty() {
        out.warnings.push(format!(
            "pominięto kształty typu: {} — format nie jest jeszcze zweryfikowany na plikach przykładowych (prześlij plik z takim kształtem)",
            ctx.unknown.join(", ")
        ));
    }
    Ok(out)
}

pub fn import_lbrn2(data: &[u8], opt: &ImportOptions) -> Result<Imported, IoError> {
    let _ = opt;
    import_any(data, ".lbrn2")
}

/// Import starszego formatu `.lbrn` (FormatVersion 0).
pub fn import_lbrn(data: &[u8], opt: &ImportOptions) -> Result<Imported, IoError> {
    let _ = opt;
    import_any(data, ".lbrn")
}

fn to_document(imp: Imported, data: &[u8], name: &str) -> Result<(Document, Vec<String>), IoError> {
    let mut doc = Document::new(name);
    if let Ok(text) = std::str::from_utf8(data) {
        if let Ok(xml) = roxmltree::Document::parse(text) {
            if let Some(notes) = xml
                .root_element()
                .children()
                .find(|c| c.has_tag_name("Notes"))
                .and_then(|c| c.attribute("Notes"))
            {
                doc.notes = notes.to_string();
            }
        }
    }
    apply(
        &mut doc,
        &imp,
        &ImportOptions {
            layer_by: LayerBy::Color,
            group: false,
            ..Default::default()
        },
    )?;
    doc.validate()?;
    Ok((doc, imp.warnings))
}

/// Projekt .lbrn2 jako nowy dokument (warstwy z CutSetting, obiekty bez dodatkowego grupowania).
pub fn lbrn2_document(data: &[u8], name: &str) -> Result<Document, IoError> {
    lbrn2_document_with_warnings(data, name).map(|(d, _)| d)
}

/// Jak [`lbrn2_document`], z ostrzeżeniami importu.
pub fn lbrn2_document_with_warnings(
    data: &[u8],
    name: &str,
) -> Result<(Document, Vec<String>), IoError> {
    to_document(import_lbrn2(data, &ImportOptions::default())?, data, name)
}

/// Projekt `.lbrn` jako nowy dokument.
pub fn lbrn_document(data: &[u8], name: &str) -> Result<Document, IoError> {
    lbrn_document_with_warnings(data, name).map(|(d, _)| d)
}

/// Jak [`lbrn_document`], z ostrzeżeniami importu.
pub fn lbrn_document_with_warnings(
    data: &[u8],
    name: &str,
) -> Result<(Document, Vec<String>), IoError> {
    to_document(import_lbrn(data, &ImportOptions::default())?, data, name)
}
