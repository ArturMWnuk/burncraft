//! Raster z bitmapy (ADR 0011 §6): każdy piksel ma własną moc, przebiegi pikseli
//! o tej samej mocy są scalone w jeden odcinek. Obrazy w dokumencie dojdą w M4;
//! do tego czasu operację buduje się z bitmapy podanej wprost (np. w testach golden).

use bc_doc::LayerMode;
use bc_geom::Point;

use crate::job::{Move, Op, OpKind, OpSettings};

/// Bitmapa w odcieniach szarości: 0 = czarny (moc maksymalna), 255 = biały (bez wiązki).
#[derive(Debug, Clone, PartialEq)]
pub struct Bitmap {
    pub width: u32,
    pub height: u32,
    /// Wiersze od góry, `width × height` bajtów.
    pub data: Vec<u8>,
    /// Lewy dolny róg obrazu, mm.
    pub origin: Point,
    /// Rozmiar piksela, mm.
    pub pixel: f64,
}

impl Bitmap {
    /// Wartość piksela (wiersz `row` od góry).
    pub fn at(&self, col: u32, row: u32) -> u8 {
        self.data[(row * self.width + col) as usize]
    }

    /// Środek piksela w mm.
    pub fn center(&self, col: u32, row: u32) -> Point {
        Point::new(
            self.origin.x + (col as f64 + 0.5) * self.pixel,
            self.origin.y + (self.height as f64 - row as f64 - 0.5) * self.pixel,
        )
    }
}

/// Parametry skanowania bitmapy.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RasterParams {
    /// mm/s
    pub speed: f64,
    /// %
    pub power_min: f64,
    /// %
    pub power_max: f64,
    pub bidirectional: bool,
    /// Dobieg przed i za wierszem z wyłączoną wiązką, mm.
    pub overscan: f64,
}

/// Moc piksela: biały = 0, inaczej liniowo od `power_min` (jasny) do `power_max` (czarny).
pub fn pixel_power(v: u8, p: &RasterParams) -> f64 {
    if v == 255 {
        0.0
    } else {
        let k = (255.0 - v as f64) / 255.0;
        let pw = p.power_min + (p.power_max - p.power_min) * k;
        (pw * 1000.0).round() / 1000.0
    }
}

/// Ruchy skanowania bitmapy (wiersze od góry; puste wiersze pominięte).
pub fn bitmap_moves(b: &Bitmap, p: &RasterParams) -> Vec<Move> {
    let mut out = Vec::new();
    let mut forward = true;
    for row in 0..b.height {
        let powers: Vec<f64> = (0..b.width).map(|c| pixel_power(b.at(c, row), p)).collect();
        let Some(first) = powers.iter().position(|&w| w > 0.0) else {
            continue;
        };
        let last = powers.iter().rposition(|&w| w > 0.0).expect("niepusty");
        let y = b.center(0, row).y;
        let edge = |c: usize| b.origin.x + c as f64 * b.pixel;
        // przebiegi [od, do) pikseli o tej samej mocy
        let mut runs: Vec<(usize, usize, f64)> = Vec::new();
        let mut c = first;
        while c <= last {
            let w = powers[c];
            let mut e = c + 1;
            while e <= last && powers[e] == w {
                e += 1;
            }
            runs.push((c, e, w));
            c = e;
        }
        let (x0, x1) = (edge(first), edge(last + 1));
        let dir = if forward { 1.0 } else { -1.0 };
        let (start, end) = if forward { (x0, x1) } else { (x1, x0) };
        out.push(Move::Travel {
            to: Point::new(start - dir * p.overscan, y),
            lift: false,
        });
        if p.overscan > 0.0 {
            out.push(Move::Cut {
                to: Point::new(start, y),
                power: 0.0,
                speed: p.speed,
            });
        }
        let ordered: Vec<(usize, usize, f64)> = if forward {
            runs
        } else {
            runs.into_iter().rev().collect()
        };
        for (a, e, w) in ordered {
            let x = if forward { edge(e) } else { edge(a) };
            out.push(Move::Cut {
                to: Point::new(x, y),
                power: w,
                speed: p.speed,
            });
        }
        if p.overscan > 0.0 {
            out.push(Move::Cut {
                to: Point::new(end + dir * p.overscan, y),
                power: 0.0,
                speed: p.speed,
            });
        }
        if p.bidirectional {
            forward = !forward;
        }
    }
    out
}

/// Operacja rastrowa z bitmapy (tryb Image).
pub fn bitmap_op(layer: u8, label: &str, b: &Bitmap, p: &RasterParams, dynamic_power: bool) -> Op {
    Op {
        layer,
        label: label.into(),
        sublayer: 0,
        pass: 0,
        kind: OpKind::Raster,
        mode: LayerMode::Image,
        show: true,
        settings: OpSettings {
            speed: p.speed,
            power_max: p.power_max,
            power_min: p.power_min,
            power2_max: None,
            power2_min: None,
            dynamic_power,
            pwm_hz: None,
            air_assist: false,
        },
        pause: None,
        delay_s: 0.0,
        z: 0.0,
        objects: Vec::new(),
        moves: bitmap_moves(b, p),
    }
}
