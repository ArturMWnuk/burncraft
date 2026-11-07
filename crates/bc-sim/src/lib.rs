//! bc-sim — podgląd i symulacja.
//!
//! M0: rasteryzacja projektu (`render_preview` w trybie `design`) do PNG.
//! M2: ścieżki zadania z planera (`toolpath`). Tryb `burn_sim` dochodzi w M4.

use bc_doc::Document;
use bc_geom::{BezPath, PathEl, Rect};
use tiny_skia::{Color, FillRule, Paint, PathBuilder, Pixmap, Stroke, Transform};

#[derive(Debug, thiserror::Error)]
pub enum RenderError {
    #[error("{0}")]
    Doc(#[from] bc_doc::DocError),
    #[error("nieprawidłowe parametry podglądu: {0}")]
    Params(String),
    #[error("kodowanie PNG: {0}")]
    Png(String),
}

#[derive(Debug, Clone)]
pub struct PreviewOptions {
    /// Obszar w mm (współrzędne świata, Y w górę). `None` = obszar roboczy maszyny.
    pub bbox: Option<Rect>,
    /// Dłuższy bok obrazu w pikselach.
    pub max_size: u32,
    /// Rysuj obrys obszaru roboczego i siatkę 10 mm.
    pub show_workspace: bool,
}

impl Default for PreviewOptions {
    fn default() -> Self {
        PreviewOptions {
            bbox: None,
            max_size: 1024,
            show_workspace: true,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Preview {
    pub png: Vec<u8>,
    pub width: u32,
    pub height: u32,
    /// Obszar świata (mm) odpowiadający obrazowi.
    pub bbox: Rect,
    /// Skala: piksele na mm.
    pub px_per_mm: f64,
}

fn parse_hex(c: &str) -> Color {
    let v = u32::from_str_radix(c.trim_start_matches('#'), 16).unwrap_or(0);
    Color::from_rgba8((v >> 16) as u8, (v >> 8) as u8, v as u8, 255)
}

fn to_skia(p: &BezPath) -> Option<tiny_skia::Path> {
    let mut b = PathBuilder::new();
    for el in p.elements() {
        match *el {
            PathEl::MoveTo(a) => b.move_to(a.x as f32, a.y as f32),
            PathEl::LineTo(a) => b.line_to(a.x as f32, a.y as f32),
            PathEl::QuadTo(a, c) => b.quad_to(a.x as f32, a.y as f32, c.x as f32, c.y as f32),
            PathEl::CurveTo(a, c, d) => b.cubic_to(
                a.x as f32, a.y as f32, c.x as f32, c.y as f32, d.x as f32, d.y as f32,
            ),
            PathEl::ClosePath => b.close(),
        }
    }
    b.finish()
}

/// Płótno podglądu: piksmapa, transformacja świat → piksele, obszar i skala.
fn canvas(
    doc: &Document,
    opt: &PreviewOptions,
) -> Result<(Pixmap, Transform, Rect, f64), RenderError> {
    if opt.max_size < 16 || opt.max_size > 8192 {
        return Err(RenderError::Params(
            "max_size musi być w zakresie 16..8192".into(),
        ));
    }
    let area = match opt.bbox {
        Some(b) if b.width() > 0.0 && b.height() > 0.0 => b,
        Some(_) => {
            return Err(RenderError::Params(
                "bbox musi mieć dodatnią szerokość i wysokość".into(),
            ))
        }
        None => Rect::new(0.0, 0.0, doc.workspace.width, doc.workspace.height),
    };
    let s = opt.max_size as f64 / area.width().max(area.height());
    let w = ((area.width() * s).round() as u32).max(1);
    let h = ((area.height() * s).round() as u32).max(1);
    let mut pm = Pixmap::new(w, h).ok_or_else(|| RenderError::Params("pusty obraz".into()))?;
    pm.fill(Color::WHITE);
    // świat (mm, Y w górę) -> piksele (Y w dół)
    let tf = Transform::from_row(
        s as f32,
        0.0,
        0.0,
        -s as f32,
        (-area.x0 * s) as f32,
        (area.y1 * s) as f32,
    );

    if opt.show_workspace {
        let mut grid = Paint::default();
        grid.set_color_rgba8(225, 228, 235, 255);
        let mut b = PathBuilder::new();
        let ws = doc.workspace.width;
        let hs = doc.workspace.height;
        let mut x = 0.0;
        while x <= ws {
            b.move_to(x as f32, 0.0);
            b.line_to(x as f32, hs as f32);
            x += 10.0;
        }
        let mut y = 0.0;
        while y <= hs {
            b.move_to(0.0, y as f32);
            b.line_to(ws as f32, y as f32);
            y += 10.0;
        }
        if let Some(p) = b.finish() {
            pm.stroke_path(
                &p,
                &grid,
                &Stroke {
                    width: 1.0 / s as f32,
                    ..Stroke::default()
                },
                tf,
                None,
            );
        }
        if let Some(r) = tiny_skia::Rect::from_xywh(0.0, 0.0, ws as f32, hs as f32) {
            let mut border = Paint::default();
            border.set_color_rgba8(120, 128, 140, 255);
            let p = PathBuilder::from_rect(r);
            pm.stroke_path(
                &p,
                &border,
                &Stroke {
                    width: 2.0 / s as f32,
                    ..Stroke::default()
                },
                tf,
                None,
            );
        }
    }

    Ok((pm, tf, area, s))
}

fn encode(pm: Pixmap, area: Rect, s: f64) -> Result<Preview, RenderError> {
    let (width, height) = (pm.width(), pm.height());
    let png = pm
        .encode_png()
        .map_err(|e| RenderError::Png(e.to_string()))?;
    Ok(Preview {
        png,
        width,
        height,
        bbox: area,
        px_per_mm: s,
    })
}

/// Renderuje projekt (geometria w kolorach warstw) do PNG.
pub fn render_design(doc: &Document, opt: &PreviewOptions) -> Result<Preview, RenderError> {
    let (mut pm, tf, area, s) = canvas(doc, opt)?;
    let stroke = Stroke {
        width: (1.5 / s) as f32,
        ..Stroke::default()
    };
    for (layer, _id, path) in doc.world_paths()? {
        let l = doc.layer(layer)?;
        if !l.show {
            continue;
        }
        let Some(sp) = to_skia(&path) else { continue };
        let mut paint = Paint::default();
        paint.set_color(parse_hex(&l.color));
        paint.anti_alias = true;
        if matches!(
            l.mode,
            bc_doc::LayerMode::Fill
                | bc_doc::LayerMode::FillLine
                | bc_doc::LayerMode::OffsetFill
                | bc_doc::LayerMode::FloodFill
        ) && bc_geom::is_closed(&path)
        {
            let mut fill = paint.clone();
            let c = parse_hex(&l.color);
            fill.set_color(Color::from_rgba(c.red(), c.green(), c.blue(), 0.35).unwrap_or(c));
            pm.fill_path(&sp, &fill, FillRule::EvenOdd, tf, None);
        }
        pm.stroke_path(&sp, &paint, &stroke, tf, None);
    }
    encode(pm, area, s)
}

/// Renderuje ścieżki zadania (`render_preview mode=toolpath`, REQ-15-023):
/// cięcie w kolorach warstw, ruch roboczy bez wiązki jasnoszary, przejazdy szare,
/// impulsy trybu punktowego jako kropki. Operacje warstw z wyłączonym Show są pomijane.
pub fn render_toolpath(
    doc: &Document,
    job: &bc_plan::Job,
    opt: &PreviewOptions,
) -> Result<Preview, RenderError> {
    let (mut pm, tf, area, s) = canvas(doc, opt)?;
    let thin = Stroke {
        width: (1.0 / s) as f32,
        ..Stroke::default()
    };
    let thick = Stroke {
        width: (1.5 / s) as f32,
        ..Stroke::default()
    };
    let line = |a: bc_geom::Point, b: bc_geom::Point| {
        let mut pb = PathBuilder::new();
        pb.move_to(a.x as f32, a.y as f32);
        pb.line_to(b.x as f32, b.y as f32);
        pb.finish()
    };
    let segs = job.segments();
    // kolejność warstw rysowania: przejazdy, ruch bez wiązki, cięcie
    for pass in 0..3 {
        for sg in &segs {
            let op = &job.ops[sg.op];
            if !op.show {
                continue;
            }
            let mut paint = Paint {
                anti_alias: true,
                ..Paint::default()
            };
            let (draw, stroke) = match (pass, sg.kind) {
                (0, bc_plan::SegKind::Travel) => {
                    paint.set_color_rgba8(150, 150, 150, 255);
                    (true, &thin)
                }
                (1, bc_plan::SegKind::Cut) if sg.power <= 0.0 => {
                    paint.set_color_rgba8(205, 205, 205, 255);
                    (true, &thin)
                }
                (2, bc_plan::SegKind::Cut) if sg.power > 0.0 => {
                    paint.set_color(parse_hex(&doc.layer(op.layer)?.color));
                    (true, &thick)
                }
                (2, bc_plan::SegKind::Dwell) => {
                    paint.set_color(parse_hex(&doc.layer(op.layer)?.color));
                    if let Some(c) = PathBuilder::from_circle(
                        sg.from.x as f32,
                        sg.from.y as f32,
                        (1.5 / s) as f32,
                    ) {
                        pm.fill_path(&c, &paint, FillRule::Winding, tf, None);
                    }
                    (false, &thin)
                }
                _ => (false, &thin),
            };
            if draw {
                if let Some(p) = line(sg.from, sg.to) {
                    pm.stroke_path(&p, &paint, stroke, tf, None);
                }
            }
        }
    }
    encode(pm, area, s)
}

#[cfg(test)]
mod tests {
    use super::*;
    use bc_doc::Kind;

    #[test]
    fn req_15_022_render_design_png() {
        let mut d = Document::new("t");
        d.add(
            2,
            Kind::Rect {
                x: 10.,
                y: 10.,
                w: 100.,
                h: 50.,
                radius: 0.,
            },
        )
        .unwrap();
        let p = render_design(
            &d,
            &PreviewOptions {
                max_size: 400,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(&p.png[1..4], b"PNG");
        assert_eq!((p.width, p.height), (400, 400));
        // piksel na krawędzi prostokąta ma kolor warstwy 2 (czerwony), Y odwrócone
        let pm = Pixmap::decode_png(&p.png).unwrap();
        let x = (60.0 * p.px_per_mm) as u32;
        let y = ((400.0 - 10.0) * p.px_per_mm).round() as u32;
        let px = pm.pixel(x, y.min(399)).unwrap();
        assert!(px.red() > 150 && px.green() < 120, "{px:?}");
    }

    #[test]
    fn req_15_025_bbox_and_max_size() {
        let mut d = Document::new("t");
        d.add(
            0,
            Kind::Ellipse {
                cx: 50.,
                cy: 50.,
                rx: 20.,
                ry: 10.,
            },
        )
        .unwrap();
        let p = render_design(
            &d,
            &PreviewOptions {
                bbox: Some(Rect::new(20., 30., 80., 70.)),
                max_size: 300,
                show_workspace: false,
            },
        )
        .unwrap();
        assert_eq!((p.width, p.height), (300, 200));
        assert!(render_design(
            &d,
            &PreviewOptions {
                max_size: 1,
                ..Default::default()
            }
        )
        .is_err());
    }

    #[test]
    fn req_15_023_render_toolpath_png() {
        let mut d = Document::new("t");
        d.add(
            2,
            Kind::Rect {
                x: 100.,
                y: 100.,
                w: 100.,
                h: 50.,
                radius: 0.,
            },
        )
        .unwrap();
        let job = bc_plan::plan(&d, &bc_plan::PlanOptions::default()).unwrap();
        let p = render_toolpath(
            &d,
            &job,
            &PreviewOptions {
                max_size: 400,
                show_workspace: false,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(&p.png[1..4], b"PNG");
        let pm = Pixmap::decode_png(&p.png).unwrap();
        let px = |x: f64, y: f64| {
            pm.pixel(
                (x * p.px_per_mm).floor() as u32,
                ((400.0 - y) * p.px_per_mm).floor().min(399.0) as u32,
            )
            .unwrap()
        };
        // cięcie w kolorze warstwy 2 (czerwony)
        let c = px(150.0, 100.0);
        assert!(c.red() > 150 && c.green() < 120, "{c:?}");
        // przejazd z (0, 0) do (100, 100) — szary, nie biały
        let t = (40..=60)
            .map(|i| px(i as f64 + 0.5, i as f64 + 0.5))
            .find(|q| q.red() < 230 && q.red() == q.green());
        assert!(t.is_some(), "brak linii przejazdu");
        // projekt bez ścieżek ukrytych przez Show
        let mut d2 = d.clone();
        d2.layer_mut(2).unwrap().show = false;
        let job2 = bc_plan::plan(&d2, &bc_plan::PlanOptions::default()).unwrap();
        let p2 = render_toolpath(
            &d2,
            &job2,
            &PreviewOptions {
                max_size: 400,
                show_workspace: false,
                ..Default::default()
            },
        )
        .unwrap();
        let pm2 = Pixmap::decode_png(&p2.png).unwrap();
        let c2 = pm2
            .pixel(
                (150.0 * p2.px_per_mm) as u32,
                ((400.0 - 100.0) * p2.px_per_mm).round() as u32,
            )
            .unwrap();
        assert!(c2.red() > 240 && c2.green() > 240, "warstwa ukryta: {c2:?}");
    }
}
