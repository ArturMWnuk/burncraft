//! Import SVG (usvg) i eksport SVG w mm (ADR 0008).

use std::collections::HashMap;

use bc_doc::{Document, Kind};
use bc_geom::{Affine, BezPath, Point};
use usvg::tiny_skia_path::PathSegment;

use crate::{export_paths, hex, ExportOptions, ImportOptions, Imported, IoError, Item, LayerKey};

const INKSCAPE_NS: &str = "http://www.inkscape.org/namespaces/inkscape";

/// Informacje z surowego XML, których `usvg` nie przekazuje.
struct Meta {
    /// Szerokość/wysokość korzenia ma jednostkę fizyczną (mm, cm, in, pt, pc).
    physical_units: bool,
    /// Wykryte DPI dla px / wartości bez jednostek.
    dpi: f64,
    /// id grupy -> inkscape:label.
    labels: HashMap<String, String>,
    texts: usize,
    images: usize,
    /// SVG po dodaniu brakujących id grup z etykietami.
    patched: String,
}

fn meta(text: &str) -> Result<Meta, IoError> {
    let doc = roxmltree::Document::parse_with_options(
        text,
        roxmltree::ParsingOptions {
            allow_dtd: true,
            ..Default::default()
        },
    )
    .map_err(|e| IoError::Parse(format!("SVG: {e}")))?;
    let root = doc.root_element();
    if root.tag_name().name() != "svg" {
        return Err(IoError::Parse("SVG: korzeń nie jest <svg>".into()));
    }
    let physical = ["width", "height"].iter().any(|a| {
        root.attribute(*a).is_some_and(|v| {
            let v = v.trim().to_ascii_lowercase();
            ["mm", "cm", "in", "pt", "pc"]
                .iter()
                .any(|u| v.ends_with(u))
        })
    });
    let illustrator = text.contains("Adobe Illustrator");
    let affinity = root
        .namespaces()
        .any(|n| n.uri() == "http://www.serif.com/");
    let dpi = if illustrator || affinity { 72.0 } else { 96.0 };
    // usvg aproksymuje łuki z tolerancją bezwzględną — ogromne liczby kończą się brakiem pamięci
    const GEOM_ATTRS: &[&str] = &[
        "d",
        "x",
        "y",
        "x1",
        "y1",
        "x2",
        "y2",
        "cx",
        "cy",
        "r",
        "rx",
        "ry",
        "width",
        "height",
        "points",
        "viewBox",
        "transform",
    ];
    for n in doc.descendants().filter(|n| n.is_element()) {
        for a in n
            .attributes()
            .filter(|a| a.namespace().is_none() && GEOM_ATTRS.contains(&a.name()))
        {
            if numbers_out_of_range(a.value()) {
                return Err(IoError::Parse(format!(
                    "SVG: wartości liczbowe poza zakresem (|x| > {}) w atrybucie {} elementu <{}>",
                    crate::RAW_LIMIT,
                    a.name(),
                    n.tag_name().name()
                )));
            }
        }
    }
    let mut labels = HashMap::new();
    let (mut texts, mut images) = (0, 0);
    // grupy z etykietą, ale bez id — usvg by je usunął; dostaną id przed parsowaniem
    let mut inject: Vec<usize> = Vec::new();
    for n in doc.descendants().filter(|n| n.is_element()) {
        match n.tag_name().name() {
            "text" => texts += 1,
            "image" => images += 1,
            "g" => {
                if let Some(label) = n.attribute((INKSCAPE_NS, "label")) {
                    let id = match n.attribute("id") {
                        Some(id) => id.to_string(),
                        None => {
                            inject.push(n.range().start);
                            format!("bc-label-{}", n.range().start)
                        }
                    };
                    labels.insert(id, label.to_string());
                }
            }
            _ => {}
        }
    }
    let mut patched = text.to_string();
    for pos in inject.into_iter().rev() {
        // `pos` wskazuje na '<' znacznika <g ...>; id wstawiamy zaraz po nazwie elementu
        let at = pos
            + text[pos..]
                .find(|c: char| c.is_whitespace() || c == '>' || c == '/')
                .unwrap_or(2);
        patched.insert_str(at, &format!(" id=\"bc-label-{pos}\""));
    }
    Ok(Meta {
        physical_units: physical,
        dpi,
        labels,
        texts,
        images,
        patched,
    })
}

/// Czy w tekście atrybutu jest liczba o wartości bezwzględnej większej niż [`crate::RAW_LIMIT`].
fn numbers_out_of_range(s: &str) -> bool {
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i].is_ascii_digit() || (b[i] == b'.' && b.get(i + 1).is_some_and(u8::is_ascii_digit)) {
            let start = i;
            while i < b.len() && (b[i].is_ascii_digit() || b[i] == b'.') {
                i += 1;
            }
            if i < b.len() && (b[i] == b'e' || b[i] == b'E') {
                let mut j = i + 1;
                if j < b.len() && (b[j] == b'+' || b[j] == b'-') {
                    j += 1;
                }
                if j < b.len() && b[j].is_ascii_digit() {
                    i = j;
                    while i < b.len() && b[i].is_ascii_digit() {
                        i += 1;
                    }
                }
            }
            let v: f64 = s[start..i].parse().unwrap_or(0.0);
            if !v.is_finite() || v > crate::RAW_LIMIT {
                return true;
            }
        } else {
            i += 1;
        }
    }
    false
}

fn to_affine(t: usvg::Transform) -> Affine {
    Affine::new([
        t.sx as f64,
        t.ky as f64,
        t.kx as f64,
        t.sy as f64,
        t.tx as f64,
        t.ty as f64,
    ])
}

fn paint_hex(p: &usvg::Paint) -> Option<String> {
    match p {
        usvg::Paint::Color(c) => Some(hex(c.red, c.green, c.blue)),
        _ => None,
    }
}

fn convert(path: &usvg::Path, view: Affine) -> BezPath {
    let a = view * to_affine(path.abs_transform());
    let pt = |p: usvg::tiny_skia_path::Point| a * Point::new(p.x as f64, p.y as f64);
    let mut out = BezPath::new();
    for seg in path.data().segments() {
        match seg {
            PathSegment::MoveTo(p) => out.move_to(pt(p)),
            PathSegment::LineTo(p) => out.line_to(pt(p)),
            PathSegment::QuadTo(p1, p) => out.quad_to(pt(p1), pt(p)),
            PathSegment::CubicTo(p1, p2, p) => out.curve_to(pt(p1), pt(p2), pt(p)),
            PathSegment::Close => out.close_path(),
        }
    }
    out
}

fn walk(g: &usvg::Group, view: Affine, label: Option<&str>, meta: &Meta, out: &mut Imported) {
    for node in g.children() {
        match node {
            usvg::Node::Group(child) => {
                let l = meta
                    .labels
                    .get(child.id())
                    .map(String::as_str)
                    .or(if child.id().is_empty() {
                        None
                    } else {
                        Some(child.id())
                    })
                    .or(label);
                walk(child, view, l, meta, out);
            }
            usvg::Node::Path(p) => {
                if !p.is_visible() {
                    continue;
                }
                let geom = convert(p, view);
                if geom.elements().len() < 2 {
                    continue;
                }
                let color = p
                    .stroke()
                    .and_then(|s| paint_hex(s.paint()))
                    .or_else(|| p.fill().and_then(|f| paint_hex(f.paint())));
                out.items.push(Item {
                    kind: Kind::Path { d: geom.to_svg() },
                    transform: Affine::IDENTITY,
                    color_key: color.map(LayerKey::Color),
                    name_key: label.map(|l| LayerKey::Name(l.to_string())),
                });
            }
            usvg::Node::Image(_) | usvg::Node::Text(_) => {}
        }
    }
}

pub fn import(data: &[u8], opt: &ImportOptions) -> Result<Imported, IoError> {
    let text = std::str::from_utf8(data)
        .map_err(|_| IoError::Parse("SVG: plik nie jest w UTF-8".into()))?;
    let m = meta(text)?;
    let tree = usvg::Tree::from_data(m.patched.as_bytes(), &usvg::Options::default())
        .map_err(|e| IoError::Parse(format!("SVG: {e}")))?;
    // usvg przelicza jednostki fizyczne na px przy 96 DPI
    let dpi = if m.physical_units {
        96.0
    } else {
        opt.dpi
            .filter(|d| d.is_finite() && *d > 0.0)
            .unwrap_or(m.dpi)
    };
    let s = 25.4 / dpi;
    let page_h = tree.size().height() as f64 * s;
    // px (Y w dół) -> mm (Y w górę), lewy dolny róg strony = (0, 0)
    let view = Affine::new([s, 0.0, 0.0, -s, 0.0, page_h]);
    let mut out = Imported::default();
    walk(tree.root(), view, None, &m, &mut out);
    crate::retain_in_range(&mut out);
    if m.texts > 0 {
        out.warnings.push(format!("pominięto tekst SVG ({} elementów) — import tekstu wymaga fontów (M4); zamień tekst na krzywe w programie źródłowym", m.texts));
    }
    if m.images > 0 {
        out.warnings.push(format!(
            "pominięto obrazy SVG ({}) — import obrazów od M4",
            m.images
        ));
    }
    Ok(out)
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// SVG 1.1 w mm: grupa na warstwę (id i inkscape:label = nazwa), obrys w kolorze warstwy.
pub fn export(doc: &Document, opt: &ExportOptions) -> Result<String, IoError> {
    let (w, h) = (doc.workspace.width, doc.workspace.height);
    let flip = Affine::new([1.0, 0.0, 0.0, -1.0, 0.0, h]);
    let mut s = String::new();
    s.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    s.push_str(&format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" xmlns:inkscape=\"{INKSCAPE_NS}\" version=\"1.1\" width=\"{w}mm\" height=\"{h}mm\" viewBox=\"0 0 {w} {h}\">\n"
    ));
    s.push_str(&format!(
        "<!-- BurnCraft {} : {} -->\n",
        env!("CARGO_PKG_VERSION"),
        esc(&doc.name).replace("--", "- -")
    ));
    for (layer, paths) in export_paths(doc, opt)? {
        s.push_str(&format!(
            "<g id=\"layer-{}\" inkscape:groupmode=\"layer\" inkscape:label=\"{}\" fill=\"none\" stroke=\"{}\" stroke-width=\"0.1\">\n",
            layer.index,
            esc(&layer.name),
            layer.color
        ));
        for mut p in paths {
            p.apply_affine(flip);
            s.push_str(&format!("<path d=\"{}\"/>\n", p.to_svg()));
        }
        s.push_str("</g>\n");
    }
    s.push_str("</svg>\n");
    Ok(s)
}
