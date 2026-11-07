//! Kolejność i optymalizacja części (REQ-08-002…007): priorytet, grupy,
//! wewnętrzne przed zewnętrznymi (ograniczenia), najbliższy sąsiad z wyborem
//! punktu startowego i kierunku, potem 2-opt w oknie.

use bc_doc::{CutDirection, PlanSettings};
use bc_geom::Point;

use crate::poly::Poly;
use crate::vector::Part;

/// Część na swoim miejscu: wielokąty gotowe do wycięcia (kierunek i start ustalone).
#[derive(Debug, Clone)]
pub(crate) struct Placed {
    pub part: usize,
    pub polys: Vec<Poly>,
}

/// Wariant części: wejście, wyjście i wielokąty dla danej pozycji głowicy.
fn orient(part: &Part, from: Point, s: &PlanSettings) -> Vec<Poly> {
    if part.polys.len() != 1 {
        return part.polys.clone();
    }
    let p = &part.polys[0];
    if p.closed && part.free {
        let area = p.signed_area();
        let q = match s.direction {
            CutDirection::Cw if area > 0.0 => p.reversed(),
            CutDirection::Ccw if area < 0.0 => p.reversed(),
            _ => p.clone(),
        };
        if s.optimize_start {
            let i = q.nearest_vertex(from);
            vec![q.rotated(i)]
        } else {
            vec![q]
        }
    } else if !p.closed {
        // otwarta: od bliższego końca
        let (a, b) = (p.pts[0], p.pts[p.pts.len() - 1]);
        if b.distance(from) < a.distance(from) - 1e-12 {
            vec![p.reversed()]
        } else {
            vec![p.clone()]
        }
    } else {
        vec![p.clone()]
    }
}

fn entry(polys: &[Poly]) -> Point {
    polys[0].pts[0]
}

fn exit(polys: &[Poly]) -> Point {
    let last = &polys[polys.len() - 1];
    if last.closed {
        last.pts[0]
    } else {
        last.pts[last.pts.len() - 1]
    }
}

/// Koszt dojazdu do części z `from` (z najlepszym startem i kierunkiem).
fn reach(part: &Part, from: Point, s: &PlanSettings) -> f64 {
    let p = &part.polys[0];
    if part.polys.len() == 1 && p.closed && part.free && s.optimize_start {
        p.pts[p.nearest_vertex(from)].distance(from)
    } else if part.polys.len() == 1 && !p.closed {
        p.pts[0]
            .distance(from)
            .min(p.pts[p.pts.len() - 1].distance(from))
    } else {
        p.pts[0].distance(from)
    }
}

/// Kolejność części. `cons` = pary (a, b): część a przed częścią b.
pub(crate) fn order(
    parts: &[Part],
    cons: &[(usize, usize)],
    s: &PlanSettings,
    from: Point,
) -> Vec<Placed> {
    let n = parts.len();
    let mut preds = vec![0usize; n];
    let mut succ: Vec<Vec<usize>> = vec![Vec::new(); n];
    if s.inner_first {
        for &(a, b) in cons {
            preds[b] += 1;
            succ[a].push(b);
        }
    }
    let unit = |i: usize| {
        if s.order_by_group {
            parts[i].group
        } else {
            usize::MAX - i
        }
    };
    let mut done = vec![false; n];
    let mut seq: Vec<usize> = Vec::with_capacity(n);
    let mut at = from;
    let mut cur_unit: Option<usize> = None;
    // priorytety rosnąco
    let mut prios: Vec<i32> = parts.iter().map(|p| p.priority).collect();
    prios.sort_unstable();
    prios.dedup();
    for prio in prios {
        loop {
            let ready = |i: usize| !done[i] && parts[i].priority == prio && preds[i] == 0;
            // najpierw dokończ bieżącą grupę
            let pick = |only_unit: Option<usize>| {
                (0..n)
                    .filter(|&i| ready(i) && only_unit.is_none_or(|u| unit(i) == u))
                    .map(|i| (i, reach(&parts[i], at, s)))
                    .min_by(|a, b| {
                        a.1.total_cmp(&b.1)
                            .then(parts[a.0].order.cmp(&parts[b.0].order))
                            .then(a.0.cmp(&b.0))
                    })
                    .map(|x| x.0)
            };
            let next = cur_unit.and_then(|u| pick(Some(u))).or_else(|| pick(None));
            let Some(i) = next else {
                // pozostałe części tego priorytetu zablokowane ograniczeniami z innych priorytetów
                let rest: Vec<usize> = (0..n)
                    .filter(|&i| !done[i] && parts[i].priority == prio)
                    .collect();
                if rest.is_empty() {
                    break;
                }
                // cykl/ograniczenia między priorytetami: priorytet ważniejszy
                for &i in &rest {
                    preds[i] = 0;
                }
                continue;
            };
            done[i] = true;
            for &b in &succ[i] {
                preds[b] = preds[b].saturating_sub(1);
            }
            let polys = orient(&parts[i], at, s);
            at = exit(&polys);
            cur_unit = Some(unit(i));
            seq.push(i);
        }
    }
    if s.two_opt {
        two_opt(&mut seq, parts, cons, s, from, &unit);
    }
    // ostateczne ustawienie startów i kierunków wzdłuż kolejności
    let mut at = from;
    seq.into_iter()
        .map(|i| {
            let polys = orient(&parts[i], at, s);
            at = exit(&polys);
            Placed { part: i, polys }
        })
        .collect()
}

/// Długość przejazdów dla kolejności (z najlepszymi startami).
fn cost(seq: &[usize], parts: &[Part], s: &PlanSettings, from: Point) -> f64 {
    let mut at = from;
    let mut c = 0.0;
    for &i in seq {
        let polys = orient(&parts[i], at, s);
        c += at.distance(entry(&polys));
        at = exit(&polys);
    }
    c
}

const WINDOW: usize = 60;

/// 2-opt w oknie: odwrócenie fragmentu kolejności części odwracalnych
/// (otwarta ścieżka albo kontur zamknięty bez mostków), gdy skraca przejazdy.
/// Koszt zmiany liczony na krawędziach fragmentu; na końcu wynik jest
/// porównywany dokładnie z kolejnością wejściową i wybierany lepszy.
fn two_opt(
    seq: &mut Vec<usize>,
    parts: &[Part],
    cons: &[(usize, usize)],
    s: &PlanSettings,
    from: Point,
    unit: &dyn Fn(usize) -> usize,
) {
    let n = seq.len();
    if n < 3 {
        return;
    }
    let before = seq.clone();
    let pairs: std::collections::BTreeSet<(usize, usize)> = if s.inner_first {
        cons.iter().flat_map(|&(a, b)| [(a, b), (b, a)]).collect()
    } else {
        Default::default()
    };
    let reversible =
        |i: usize| parts[i].polys.len() == 1 && (!parts[i].polys[0].closed || parts[i].free);
    // wejścia i wyjścia wg bieżącej kolejności
    let mut ent = vec![Point::ORIGIN; n];
    let mut ext = vec![Point::ORIGIN; n];
    let fill = |seq: &[usize], ent: &mut Vec<Point>, ext: &mut Vec<Point>| {
        let mut at = from;
        for (k, &i) in seq.iter().enumerate() {
            let polys = orient(&parts[i], at, s);
            ent[k] = entry(&polys);
            ext[k] = exit(&polys);
            at = ext[k];
        }
    };
    fill(seq, &mut ent, &mut ext);
    for _round in 0..20 {
        let mut improved = false;
        for i in 0..n - 1 {
            if !reversible(seq[i]) {
                continue;
            }
            for j in i + 1..n.min(i + WINDOW) {
                let pj = seq[j];
                if !reversible(pj) || parts[pj].priority != parts[seq[i]].priority {
                    break;
                }
                if !pairs.is_empty() && seq[i..j].iter().any(|&a| pairs.contains(&(a, pj))) {
                    break;
                }
                if s.order_by_group && unit(seq[i]) != unit(pj) {
                    let split_left = i > 0 && unit(seq[i - 1]) == unit(seq[i]);
                    let split_right = j + 1 < n && unit(seq[j + 1]) == unit(pj);
                    if split_left || split_right {
                        continue;
                    }
                }
                let prev = if i == 0 { from } else { ext[i - 1] };
                let mut old = prev.distance(ent[i]);
                let mut new = prev.distance(ext[j]);
                if j + 1 < n {
                    old += ext[j].distance(ent[j + 1]);
                    new += ent[i].distance(ent[j + 1]);
                }
                if new < old - 1e-9 {
                    seq[i..=j].reverse();
                    ent[i..=j].reverse();
                    ext[i..=j].reverse();
                    for k in i..=j {
                        std::mem::swap(&mut ent[k], &mut ext[k]);
                    }
                    improved = true;
                }
            }
        }
        if !improved {
            break;
        }
        fill(seq, &mut ent, &mut ext);
    }
    if cost(seq, parts, s, from) > cost(&before, parts, s, from) {
        *seq = before;
    }
}
