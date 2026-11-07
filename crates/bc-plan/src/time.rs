//! Model czasu (ADR 0009 §7): trapezowy profil prędkości z planowaniem
//! wyprzedzającym, osobne prędkości i przyspieszenia osi X/Y, odchylenie na
//! złączu jak GRBL `$11`. Rampa mocy na narożnikach korzysta z tego samego profilu.

use bc_doc::Kinematics;
use bc_geom::{Point, Vec2};
use schemars::JsonSchema;
use serde::Serialize;

use crate::fill::axis_limits;
use crate::job::{Job, Move};
use crate::poly::EPS;

/// Szacowany czas zadania (s).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, JsonSchema)]
pub struct Estimate {
    pub total_s: f64,
    /// Ruchy robocze (z wiązką i bez: overscan, perforacja).
    pub cut_s: f64,
    pub travel_s: f64,
    /// Opóźnienia między przejściami.
    pub delay_s: f64,
    /// Postoje trybu punktowego.
    pub dwell_s: f64,
}

#[derive(Debug, Clone, Copy)]
struct Block {
    len: f64,
    u: Vec2,
    vmax: f64,
    a: f64,
}

fn block(from: Point, to: Point, feed: f64, k: &Kinematics) -> Option<Block> {
    let d = to - from;
    let len = d.hypot();
    if len < EPS {
        return None;
    }
    let u = d / len;
    let (vmax, a) = axis_limits(u, feed, k);
    Some(Block { len, u, vmax, a })
}

/// Prędkości na złączach łańcucha bloków (n + 1 wartości, końce = 0).
fn junctions(bs: &[Block], k: &Kinematics) -> Vec<f64> {
    let n = bs.len();
    let mut v = vec![0.0; n + 1];
    for i in 1..n {
        let (a, b) = (&bs[i - 1], &bs[i]);
        let cos_theta = -a.u.dot(b.u);
        let vj = if cos_theta > 0.999_999 {
            0.0
        } else if cos_theta < -0.999_999 {
            f64::MAX
        } else {
            let s = (0.5 * (1.0 - cos_theta)).sqrt();
            (a.a.min(b.a) * k.junction_deviation * s / (1.0 - s)).sqrt()
        };
        v[i] = vj.min(a.vmax).min(b.vmax);
    }
    // wstecz i w przód: prędkości osiągalne z przyspieszenia
    for i in (0..n).rev() {
        v[i] = v[i].min((v[i + 1] * v[i + 1] + 2.0 * bs[i].a * bs[i].len).sqrt());
    }
    for i in 0..n {
        v[i + 1] = v[i + 1].min((v[i] * v[i] + 2.0 * bs[i].a * bs[i].len).sqrt());
    }
    v
}

/// Fazy odcinka: (długość, średnia prędkość, czas) przyspieszania, jazdy i hamowania.
fn phases(len: f64, v0: f64, v1: f64, vmax: f64, a: f64, jerk: f64) -> [(f64, f64, f64); 3] {
    let d1 = ((vmax * vmax - v0 * v0) / (2.0 * a)).max(0.0);
    let d2 = ((vmax * vmax - v1 * v1) / (2.0 * a)).max(0.0);
    let (vp, d1, d2) = if d1 + d2 <= len {
        (vmax, d1, d2)
    } else {
        let vp = ((2.0 * a * len + v0 * v0 + v1 * v1) / 2.0).sqrt().min(vmax);
        let d1 = ((vp * vp - v0 * v0) / (2.0 * a)).clamp(0.0, len);
        (vp, d1, (len - d1).max(0.0))
    };
    let cruise = (len - d1 - d2).max(0.0);
    [
        (d1, (v0 + vp) / 2.0, (vp - v0).max(0.0) / a * jerk),
        (cruise, vp, if vp > EPS { cruise / vp } else { 0.0 }),
        (d2, (vp + v1) / 2.0, (vp - v1).max(0.0) / a * jerk),
    ]
}

/// Czas odcinka długości `len` z prędkością wejścia `v0`, wyjścia `v1`, maksymalną
/// `vmax` i przyspieszeniem `a` (profil trapezowy albo trójkątny).
pub fn segment_time(len: f64, v0: f64, v1: f64, vmax: f64, a: f64) -> f64 {
    phases(len, v0, v1, vmax, a, 1.0).iter().map(|p| p.2).sum()
}

fn chain_time(bs: &[Block], k: &Kinematics) -> f64 {
    if bs.is_empty() {
        return 0.0;
    }
    let v = junctions(bs, k);
    bs.iter()
        .enumerate()
        .map(|(i, b)| {
            phases(b.len, v[i], v[i + 1], b.vmax, b.a, k.jerk_factor)
                .iter()
                .map(|p| p.2)
                .sum::<f64>()
        })
        .sum()
}

/// Czasy bloków łańcucha (s), w kolejności bloków.
fn chain_times(bs: &[Block], k: &Kinematics) -> Vec<f64> {
    if bs.is_empty() {
        return Vec::new();
    }
    let v = junctions(bs, k);
    bs.iter()
        .enumerate()
        .map(|(i, b)| {
            phases(b.len, v[i], v[i + 1], b.vmax, b.a, k.jerk_factor)
                .iter()
                .map(|p| p.2)
                .sum::<f64>()
        })
        .collect()
}

/// Przedziały czasu `[t0, t1]` każdego segmentu zadania ([`Job::segments`]) w tym
/// samym modelu co [`estimate`]: suwak podglądu mapuje czas na segment (REQ-09-001).
/// Opóźnienie przed operacją przesuwa jej pierwszy segment.
pub fn segment_times(job: &Job, k: &Kinematics) -> Vec<[f64; 2]> {
    let mut out: Vec<[f64; 2]> = Vec::new();
    let mut dur: Vec<f64> = Vec::new();
    let mut delay: Vec<f64> = Vec::new();
    let mut at = job.start;
    for op in &job.ops {
        let first = dur.len();
        // łańcuch ruchów roboczych: (indeks segmentu, blok)
        let mut chain: Vec<(usize, Block)> = Vec::new();
        let flush = |chain: &mut Vec<(usize, Block)>, dur: &mut Vec<f64>| {
            let bs: Vec<Block> = chain.iter().map(|c| c.1).collect();
            for ((i, _), t) in chain.iter().zip(chain_times(&bs, k)) {
                dur[*i] = t;
            }
            chain.clear();
        };
        for m in &op.moves {
            let i = dur.len();
            dur.push(0.0);
            delay.push(0.0);
            match *m {
                Move::Cut { to, speed, .. } => {
                    if let Some(b) = block(at, to, speed, k) {
                        chain.push((i, b));
                    }
                    at = to;
                }
                Move::Travel { to, .. } => {
                    flush(&mut chain, &mut dur);
                    if let Some(b) = block(at, to, k.travel_speed, k) {
                        dur[i] = chain_time(&[b], k);
                    }
                    at = to;
                }
                Move::Dwell { s, .. } => {
                    flush(&mut chain, &mut dur);
                    dur[i] = s;
                }
            }
        }
        flush(&mut chain, &mut dur);
        if first < delay.len() {
            delay[first] += op.delay_s;
        }
    }
    let mut t = 0.0;
    for (d, w) in dur.iter().zip(&delay) {
        t += w;
        out.push([t, t + d]);
        t += d;
    }
    out
}

/// Estymacja czasu zadania (REQ-09-008, REQ-08-018).
pub fn estimate(job: &Job, k: &Kinematics) -> Estimate {
    let mut e = Estimate {
        total_s: 0.0,
        cut_s: 0.0,
        travel_s: 0.0,
        delay_s: 0.0,
        dwell_s: 0.0,
    };
    let mut at = job.start;
    for op in &job.ops {
        e.delay_s += op.delay_s;
        let mut chain: Vec<Block> = Vec::new();
        for m in &op.moves {
            match *m {
                Move::Cut { to, speed, .. } => {
                    if let Some(b) = block(at, to, speed, k) {
                        chain.push(b);
                    }
                    at = to;
                }
                Move::Travel { to, .. } => {
                    e.cut_s += chain_time(&chain, k);
                    chain.clear();
                    if let Some(b) = block(at, to, k.travel_speed, k) {
                        e.travel_s += chain_time(&[b], k);
                    }
                    at = to;
                }
                Move::Dwell { s, .. } => {
                    e.cut_s += chain_time(&chain, k);
                    chain.clear();
                    e.dwell_s += s;
                }
            }
        }
        e.cut_s += chain_time(&chain, k);
    }
    e.total_s = e.cut_s + e.travel_s + e.delay_s + e.dwell_s;
    e
}

/// Rampa mocy na narożnikach (REQ-07-042): ruchy z mocą dzielone na fazy
/// przyspieszania, jazdy i hamowania; moc proporcjonalna do średniej prędkości fazy.
pub(crate) fn corner_ramp(moves: &[Move], start: Point, pmin: f64, k: &Kinematics) -> Vec<Move> {
    let mut out = Vec::with_capacity(moves.len() * 2);
    let mut i = 0;
    let mut at = start;
    while i < moves.len() {
        match moves[i] {
            Move::Cut { .. } => {
                // łańcuch kolejnych ruchów roboczych
                let mut j = i;
                let mut pts = vec![at];
                while j < moves.len() {
                    if let Move::Cut { to, .. } = moves[j] {
                        pts.push(to);
                        j += 1;
                    } else {
                        break;
                    }
                }
                let mut bs = Vec::new();
                let mut idx = Vec::new(); // blok -> ruch
                for (n, w) in pts.windows(2).enumerate() {
                    if let Move::Cut { speed, .. } = moves[i + n] {
                        if let Some(b) = block(w[0], w[1], speed, k) {
                            bs.push(b);
                            idx.push(i + n);
                        }
                    }
                }
                let v = junctions(&bs, k);
                let mut bi = 0;
                for (n, w) in pts.windows(2).enumerate() {
                    let Move::Cut { to, power, speed } = moves[i + n] else {
                        unreachable!()
                    };
                    if bi < idx.len() && idx[bi] == i + n && power > 0.0 {
                        let b = bs[bi];
                        let lo = pmin.min(power);
                        let mut s = 0.0;
                        for (len, vavg, _) in phases(b.len, v[bi], v[bi + 1], b.vmax, b.a, 1.0) {
                            if len < EPS {
                                continue;
                            }
                            s += len;
                            let ratio = (vavg / speed).clamp(0.0, 1.0);
                            let q = if s >= b.len - EPS { to } else { w[0] + b.u * s };
                            out.push(Move::Cut {
                                to: q,
                                power: lo + (power - lo) * ratio,
                                speed,
                            });
                        }
                    } else {
                        out.push(moves[i + n]);
                    }
                    if bi < idx.len() && idx[bi] == i + n {
                        bi += 1;
                    }
                }
                at = pts[pts.len() - 1];
                i = j;
            }
            Move::Travel { to, .. } => {
                out.push(moves[i]);
                at = to;
                i += 1;
            }
            Move::Dwell { .. } => {
                out.push(moves[i]);
                i += 1;
            }
        }
    }
    out
}
