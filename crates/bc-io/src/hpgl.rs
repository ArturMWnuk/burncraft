//! Import i eksport HPGL/PLT (40 jednostek plotera na mm, ADR 0008).

use bc_doc::{Document, Kind};
use bc_geom::{Affine, BezPath, Point};

use crate::{export_paths, ExportOptions, ImportOptions, Imported, IoError, Item, LayerKey};

/// Jednostki plotera na milimetr.
pub const UNITS_PER_MM: f64 = 40.0;

struct State {
    pen: u32,
    down: bool,
    relative: bool,
    pos: Point,
    path: BezPath,
    open_sub: bool,
}

impl State {
    fn flush(&mut self, out: &mut Imported) {
        if self.path.elements().len() > 1 {
            out.items.push(Item {
                kind: Kind::Path {
                    d: std::mem::take(&mut self.path).to_svg(),
                },
                transform: Affine::IDENTITY,
                color_key: Some(LayerKey::Pen(self.pen.max(1))),
                name_key: None,
            });
        }
        self.path = BezPath::new();
        self.open_sub = false;
    }

    fn go(&mut self, p: Point) {
        let target = if self.relative {
            self.pos + p.to_vec2()
        } else {
            p
        };
        if self.down {
            if !self.open_sub {
                self.path.move_to(self.pos);
                self.open_sub = true;
            }
            self.path.line_to(target);
        } else {
            self.open_sub = false;
        }
        self.pos = target;
    }
}

pub fn import(data: &[u8], opt: &ImportOptions) -> Result<Imported, IoError> {
    let _ = opt;
    let text = String::from_utf8_lossy(data);
    let mut out = Imported::default();
    let mut st = State {
        pen: 1,
        down: false,
        relative: false,
        pos: Point::ORIGIN,
        path: BezPath::new(),
        open_sub: false,
    };
    let mut unknown: Vec<String> = Vec::new();
    for raw in text.split([';', '\n', '\r']) {
        let cmd = raw.trim();
        if cmd.len() < 2 || !cmd.is_char_boundary(2) {
            continue;
        }
        let (name, args) = cmd.split_at(2);
        let nums: Vec<f64> = args
            .split([',', ' '])
            .filter(|s| !s.trim().is_empty())
            .filter_map(|s| s.trim().parse::<f64>().ok())
            .filter(|v| v.is_finite())
            .map(|v| v / UNITS_PER_MM)
            .collect();
        match name.to_ascii_uppercase().as_str() {
            "IN" => {
                st.flush(&mut out);
                st.relative = false;
                st.down = false;
                st.pos = Point::ORIGIN;
            }
            "SP" => {
                st.flush(&mut out);
                st.pen = args.trim().parse::<u32>().unwrap_or(1).clamp(1, 256);
            }
            "PU" | "PD" | "PA" | "PR" => {
                match name.to_ascii_uppercase().as_str() {
                    "PU" => {
                        st.down = false;
                        st.open_sub = false;
                    }
                    "PD" => st.down = true,
                    "PA" => st.relative = false,
                    _ => st.relative = true,
                }
                for c in nums.as_chunks::<2>().0 {
                    st.go(Point::new(c[0], c[1]));
                }
            }
            other => {
                if !unknown.iter().any(|u| u == other) && unknown.len() < 20 {
                    unknown.push(other.to_string());
                }
            }
        }
    }
    st.flush(&mut out);
    crate::retain_in_range(&mut out);
    if !unknown.is_empty() {
        out.warnings.push(format!(
            "HPGL: pominięto nieobsługiwane polecenia: {}",
            unknown.join(", ")
        ));
    }
    Ok(out)
}

/// `IN;`, pióro na warstwę (indeks + 1), PU/PD ze spłaszczonych ścieżek (0,01 mm).
pub fn export(doc: &Document, opt: &ExportOptions) -> Result<String, IoError> {
    let mut s = String::from("IN;");
    let u = |v: f64| (v * UNITS_PER_MM).round() as i64;
    for (layer, paths) in export_paths(doc, opt)? {
        s.push_str(&format!("SP{};", layer.index as u32 + 1));
        for p in paths {
            for poly in bc_geom::flatten(&p, 0.01) {
                let Some(first) = poly.first() else { continue };
                s.push_str(&format!("PU{},{};", u(first.x), u(first.y)));
                if poly.len() > 1 {
                    let coords: Vec<String> = poly[1..]
                        .iter()
                        .map(|q| format!("{},{}", u(q.x), u(q.y)))
                        .collect();
                    s.push_str(&format!("PD{};", coords.join(",")));
                }
            }
        }
    }
    s.push_str("PU;SP0;");
    Ok(s)
}
