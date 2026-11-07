//! bc-plan — planer cięcia (ADR 0009): dokument → zadanie (operacje z ruchami,
//! mocą i prędkością), ostrzeżenia i estymacja czasu.
//!
//! Potok warstwy: geometria → nakładki → kerf → wypełnienia → mostki →
//! lead-in/out i overcut → kolejność → optymalizacja → rozwinięcie mocy →
//! przejazdy → origin. Etap „przygotowania” warstwy (do mostków włącznie) liczy
//! się równolegle i trafia do pamięci podręcznej [`Planner`]; kolejność i
//! przejazdy liczone są zawsze, w kolejności warstw.

mod collect;
mod fill;
mod job;
mod order;
mod poly;
pub mod raster;
mod route;
mod spec;
mod time;
mod vector;
mod warn;

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use bc_doc::{Document, Layer, LayerMode, LayerRole, ObjectId, PlanSettings, Tab};
use bc_geom::Point;
use sha2::{Digest, Sha256};

pub use job::{Job, Move, Op, OpKind, OpSettings, Seg, SegKind, Stats, Warning};
pub use time::{estimate, segment_time, segment_times, Estimate};

use collect::Item;
use poly::{merge_collinear, Poly};
use route::{Obstacle, Route};
use spec::Spec;
use vector::Part;

/// Nazwa modułu, używana przez `burncraft-cli version`.
pub const MODULE: &str = "bc-plan";

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum PlanError {
    #[error("{0}")]
    Invalid(String),
    #[error("anulowano")]
    Cancelled,
}

impl PlanError {
    pub(crate) fn doc(e: bc_doc::DocError) -> Self {
        PlanError::Invalid(e.to_string())
    }
}

/// Opcje jednego planowania.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PlanOptions {
    /// Tylko te obiekty (i ich potomkowie) — „cięcie zaznaczonych” (REQ-08-013).
    pub selection: Option<Vec<ObjectId>>,
    /// Lewy dolny róg zaznaczenia jako origin zadania (REQ-08-014).
    pub selection_origin: bool,
    /// Liczba wątków (0 = liczba rdzeni). Wynik nie zależy od tej liczby.
    pub threads: usize,
    /// Limit mocy (%) do ostrzeżeń (REQ-15-056).
    pub power_limit: Option<f64>,
}

/// Postęp planowania: (zrobione, razem) → `Err(Cancelled)` przerywa.
pub type ProgressFn<'a> = dyn FnMut(usize, usize) -> Result<(), PlanError> + 'a;

/// Planuje zadanie z dokumentu.
pub fn plan(doc: &Document, opts: &PlanOptions) -> Result<Job, PlanError> {
    Planner::new().plan(doc, opts, &mut |_, _| Ok(()))
}

/// Planuje zadanie, zgłaszając postęp i sprawdzając anulowanie.
pub fn plan_with_progress(
    doc: &Document,
    opts: &PlanOptions,
    progress: &mut ProgressFn,
) -> Result<Job, PlanError> {
    Planner::new().plan(doc, opts, progress)
}

/// Obrys kerfu jednego konturu: linia środka wiązki po korekcie (REQ-07-029/030).
#[derive(Debug, Clone, PartialEq, serde::Serialize, schemars::JsonSchema)]
pub struct KerfOutline {
    pub layer: u8,
    pub object: ObjectId,
    pub closed: bool,
    /// Punkty łamanej w układzie dokumentu, mm.
    pub points: Vec<[f64; 2]>,
}

/// Obrysy kerfu warstw cięcia z kerfem ≠ 0 (tryby z konturem: Line, Offset Fill,
/// Fill+Line) — wizualizacja w edytorze i podglądzie. Te same przekształcenia co w
/// planerze (kerf X/Y, kontury zewnętrzne na zewnątrz, otwory do wewnątrz).
pub fn kerf_outlines(doc: &Document) -> Result<Vec<KerfOutline>, PlanError> {
    let kerf = |l: &Layer| {
        let p = &l.params;
        (p.kerf_x.unwrap_or(p.kerf), p.kerf_y.unwrap_or(p.kerf))
    };
    let layers: Vec<&Layer> = doc
        .layers
        .iter()
        .filter(|l| {
            let (kx, ky) = kerf(l);
            l.role == LayerRole::Cut
                && l.output
                && (kx != 0.0 || ky != 0.0)
                && matches!(
                    l.mode,
                    LayerMode::Line | LayerMode::OffsetFill | LayerMode::FillLine
                )
        })
        .collect();
    let mut out = Vec::new();
    if layers.is_empty() {
        return Ok(out);
    }
    let items = collect::collect(doc, None)?;
    for l in layers {
        let (kx, ky) = kerf(l);
        for it in items.iter().filter(|i| i.layer == l.index) {
            let (closed, _) = vector::split_closed(&it.path);
            if closed.elements().is_empty() {
                continue;
            }
            for path in vector::kerf_offset(&[closed], kx, ky)? {
                for poly in Poly::from_path(&path, vector::TOL) {
                    out.push(KerfOutline {
                        layer: l.index,
                        object: it.id,
                        closed: poly.closed,
                        points: poly.pts.iter().map(|q| [q.x, q.y]).collect(),
                    });
                }
            }
        }
    }
    Ok(out)
}

/// Treść jednego przejścia przebiegu.
#[derive(Debug, Clone)]
enum Content {
    /// Części i ograniczenia kolejności (a przed b).
    Vector(Vec<Part>, Vec<(usize, usize)>),
    Raster(Vec<fill::FillGroup>),
}

#[derive(Debug, Clone)]
struct PreparedSpec {
    spec: Spec,
    contents: Vec<(OpKind, Content)>,
}

/// Przygotowana warstwa (niezależna od pozycji głowicy) — jednostka pamięci podręcznej.
#[derive(Debug, Clone)]
struct Prepared {
    layer: u8,
    specs: Vec<PreparedSpec>,
    warnings: Vec<Warning>,
    objects: Vec<ObjectId>,
    /// Kontury zamknięte trybu Line (do „cięcie na końcu”, REQ-07-051).
    line_contours: Vec<Poly>,
}

/// Planer z pamięcią podręczną przygotowanych warstw (REQ-08-019).
#[derive(Default)]
pub struct Planner {
    cache: BTreeMap<String, Arc<Prepared>>,
    recomputed: usize,
}

impl Planner {
    pub fn new() -> Self {
        Planner::default()
    }

    /// Liczba warstw przeliczonych w ostatnim planowaniu (reszta z pamięci podręcznej).
    pub fn last_recomputed(&self) -> usize {
        self.recomputed
    }

    pub fn plan(
        &mut self,
        doc: &Document,
        opts: &PlanOptions,
        progress: &mut ProgressFn,
    ) -> Result<Job, PlanError> {
        doc.plan.validate().map_err(PlanError::Invalid)?;
        for l in &doc.layers {
            l.params
                .validate()
                .map_err(|e| PlanError::Invalid(format!("warstwa {}: {e}", l.index)))?;
        }
        let selection: Option<BTreeSet<ObjectId>> =
            opts.selection.as_ref().map(|v| v.iter().copied().collect());
        let items = collect::collect(doc, selection.as_ref())?;
        let layers: Vec<&Layer> = doc
            .layers
            .iter()
            .filter(|l| l.role == LayerRole::Cut && l.output)
            .filter(|l| items.iter().any(|i| i.layer == l.index))
            .collect();
        // klucze pamięci podręcznej
        let per_layer: Vec<Vec<Item>> = layers
            .iter()
            .map(|l| {
                items
                    .iter()
                    .filter(|i| i.layer == l.index)
                    .cloned()
                    .collect()
            })
            .collect();
        let keys: Vec<String> = layers
            .iter()
            .zip(&per_layer)
            .map(|(l, its)| cache_key(l, its, &doc.plan))
            .collect();
        let todo: Vec<usize> = (0..layers.len())
            .filter(|&i| !self.cache.contains_key(&keys[i]))
            .collect();
        let threads = if opts.threads == 0 {
            std::thread::available_parallelism().map_or(1, |n| n.get())
        } else {
            opts.threads
        }
        .max(1);
        let total = todo.len();
        progress(0, total)?;
        let mut fresh: BTreeMap<usize, Arc<Prepared>> = BTreeMap::new();
        for (bi, batch) in todo.chunks(threads).enumerate() {
            let results: Vec<Result<Prepared, PlanError>> = if batch.len() == 1 {
                vec![prepare(layers[batch[0]], &per_layer[batch[0]], &doc.plan)]
            } else {
                std::thread::scope(|sc| {
                    let hs: Vec<_> = batch
                        .iter()
                        .map(|&i| {
                            let (l, its, ps) = (layers[i], &per_layer[i], &doc.plan);
                            sc.spawn(move || prepare(l, its, ps))
                        })
                        .collect();
                    hs.into_iter()
                        .map(|h| {
                            h.join().unwrap_or_else(|_| {
                                Err(PlanError::Invalid("błąd wątku planera".into()))
                            })
                        })
                        .collect()
                })
            };
            for (&i, r) in batch.iter().zip(results) {
                fresh.insert(i, Arc::new(r?));
            }
            progress((bi * threads + batch.len()).min(total), total)?;
        }
        self.recomputed = fresh.len();
        let mut prepared: Vec<Arc<Prepared>> = Vec::with_capacity(layers.len());
        let mut next_cache = BTreeMap::new();
        for (i, k) in keys.iter().enumerate() {
            let p = match fresh.remove(&i) {
                Some(p) => p,
                None => self.cache.get(k).expect("w pamięci").clone(),
            };
            next_cache.insert(k.clone(), p.clone());
            prepared.push(p);
        }
        self.cache = next_cache;
        let mut job = assemble(doc, &layers, &prepared)?;
        warn::document_warnings(doc, &items, &layers, opts, &mut job);
        if opts.selection_origin && opts.selection.is_some() {
            if let Some(b) = items
                .iter()
                .filter_map(|i| bc_geom::bbox(&i.path))
                .reduce(|a, b| a.union(b))
            {
                shift(&mut job, Point::new(b.x0, b.y0));
            }
        }
        Ok(job)
    }
}

fn cache_key(l: &Layer, items: &[Item], ps: &PlanSettings) -> String {
    let mut h = Sha256::new();
    h.update(serde_json::to_vec(l).unwrap_or_default());
    // ustawienia planu używane w przygotowaniu warstwy
    h.update(
        serde_json::to_vec(&(
            ps.remove_overlaps,
            ps.inner_first,
            &ps.no_go_zones,
            ps.raster_band,
        ))
        .unwrap_or_default(),
    );
    for it in items {
        h.update(
            format!(
                "{}|{}|{}|{}|{}|{:?}\n",
                it.id,
                it.order,
                it.group,
                it.priority,
                it.path.to_svg(),
                it.tabs
            )
            .as_bytes(),
        );
    }
    h.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

/// Przygotowanie warstwy: geometria, kerf, wypełnienia, mostki, strefy dla wiązki.
fn prepare(l: &Layer, items: &[Item], ps: &PlanSettings) -> Result<Prepared, PlanError> {
    let mut warnings = Vec::new();
    let beam: Vec<[f64; 4]> = ps
        .no_go_zones
        .iter()
        .filter(|z| z.beam)
        .map(|z| z.rect)
        .collect();
    let mut beam_hit: Vec<ObjectId> = Vec::new();
    let mut line_contours: Vec<Poly> = Vec::new();
    // usuwanie nakładających się linii (REQ-08-011) dla konturów
    let line_items: Vec<Item> = if ps.remove_overlaps {
        let paths: Vec<bc_geom::BezPath> = items.iter().map(|i| i.path.clone()).collect();
        let (clean, _) = bc_geom::shaping::remove_overlaps(&paths, 1e-3);
        items
            .iter()
            .zip(clean)
            .map(|(i, p)| Item {
                path: p,
                ..i.clone()
            })
            .collect()
    } else {
        items.to_vec()
    };
    let line = |sp: &Spec,
                beam_hit: &mut Vec<ObjectId>,
                keep: &mut Vec<Poly>|
     -> Result<Content, PlanError> {
        let cs = vector::contours(&line_items, &sp.params)?;
        let mut parts = vector::parts(&line_items, &cs, &sp.params);
        keep.extend(cs.iter().filter(|c| c.poly.closed).map(|c| c.poly.clone()));
        // ograniczenia: kontur zawarty przed zawierającym
        let mut cons = Vec::new();
        let boxes: Vec<[f64; 4]> = cs.iter().map(|c| c.poly.bbox()).collect();
        let mut by_contour: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
        for (pi, p) in parts.iter().enumerate() {
            by_contour.entry(p.contour).or_default().push(pi);
        }
        for (i, ci) in cs.iter().enumerate() {
            for (j, cj) in cs.iter().enumerate() {
                if i != j && vector::contains(cj, &boxes[j], ci, &boxes[i]) {
                    for &a in by_contour.get(&i).into_iter().flatten() {
                        for &b in by_contour.get(&j).into_iter().flatten() {
                            cons.push((a, b));
                        }
                    }
                }
            }
        }
        if !beam.is_empty() {
            for p in &mut parts {
                let mut changed = false;
                let mut out = Vec::new();
                for poly in &p.polys {
                    match route::clip_out(&poly.walk(), &beam) {
                        None => out.push(poly.clone()),
                        Some(pieces) => {
                            changed = true;
                            out.extend(
                                pieces
                                    .into_iter()
                                    .map(|v| Poly::new(v, false))
                                    .filter(|q| q.pts.len() >= 2),
                            );
                        }
                    }
                }
                if changed {
                    beam_hit.push(p.id);
                    p.polys = out;
                    p.free = false;
                }
            }
            // usunięcie pustych części i przenumerowanie ograniczeń
            let mut map = vec![usize::MAX; parts.len()];
            let mut n = 0;
            for (i, p) in parts.iter().enumerate() {
                if !p.polys.is_empty() {
                    map[i] = n;
                    n += 1;
                }
            }
            parts.retain(|p| !p.polys.is_empty());
            cons = cons
                .into_iter()
                .filter(|(a, b)| map[*a] != usize::MAX && map[*b] != usize::MAX)
                .map(|(a, b)| (map[a], map[b]))
                .collect();
        }
        Ok(Content::Vector(parts, cons))
    };
    let mut specs = Vec::new();
    for sp in spec::specs(l) {
        let mut contents = Vec::new();
        match sp.mode {
            LayerMode::Line => {
                contents.push((
                    OpKind::Vector,
                    line(&sp, &mut beam_hit, &mut line_contours)?,
                ));
            }
            LayerMode::OffsetFill => {
                contents.push((
                    OpKind::Vector,
                    Content::Vector(vector::offset_rings(items, sp.params.interval)?, Vec::new()),
                ));
            }
            LayerMode::Fill | LayerMode::FloodFill | LayerMode::FillLine => {
                let flood = sp.mode == LayerMode::FloodFill;
                let (groups, open) = fill::groups(items, &sp, flood, ps.raster_band)?;
                warn::open_in_fill(&sp, &open, &mut warnings);
                contents.push((OpKind::Raster, Content::Raster(groups)));
                if sp.mode == LayerMode::FillLine {
                    let mut ignore = Vec::new();
                    contents.push((OpKind::Vector, line(&sp, &mut beam_hit, &mut ignore)?));
                }
            }
            LayerMode::Image => {
                warnings.push(Warning::new(
                    "image_mode",
                    format!(
                        "warstwa {}: tryb Image będzie dostępny razem z obrazami (M4) — pominięta",
                        sp.label
                    ),
                    Some(sp.layer),
                    items.iter().map(|i| i.id).collect(),
                ));
            }
        }
        specs.push(PreparedSpec { spec: sp, contents });
    }
    if !beam_hit.is_empty() {
        beam_hit.sort_unstable();
        beam_hit.dedup();
        warnings.push(Warning::new(
            "beam_no_go",
            format!(
                "warstwa {}: cięcie w strefie zakazanej dla wiązki zostało pominięte",
                l.name
            ),
            Some(l.index),
            beam_hit,
        ));
    }
    let mut objects: Vec<ObjectId> = items.iter().map(|i| i.id).collect();
    objects.dedup();
    Ok(Prepared {
        layer: l.index,
        specs,
        warnings,
        objects,
        line_contours,
    })
}

/// Kolejność warstw (REQ-08-001, REQ-07-023, REQ-07-051).
fn layer_order(layers: &[&Layer], prepared: &[Arc<Prepared>], ps: &PlanSettings) -> Vec<usize> {
    let mut idx: Vec<usize> = (0..layers.len()).collect();
    let is_cut = |i: usize| layers[i].mode == LayerMode::Line;
    // zagnieżdżenie konturów cięcia między warstwami: warstwa z konturami zawartymi w innej warstwie idzie wcześniej
    let depth: Vec<usize> = (0..layers.len())
        .map(|i| {
            if !(ps.cut_last && is_cut(i)) {
                return 0;
            }
            prepared[i]
                .line_contours
                .iter()
                .map(|c| {
                    let cb = c.bbox();
                    (0..layers.len())
                        .filter(|&j| j != i && is_cut(j))
                        .flat_map(|j| prepared[j].line_contours.iter())
                        .filter(|o| {
                            let ob = o.bbox();
                            ob[0] <= cb[0]
                                && ob[1] <= cb[1]
                                && ob[2] >= cb[2]
                                && ob[3] >= cb[3]
                                && o.signed_area().abs() > c.signed_area().abs()
                                && o.contains(c.pts[0])
                        })
                        .count()
                })
                .max()
                .unwrap_or(0)
        })
        .collect();
    idx.sort_by(|&a, &b| {
        let ka = (
            ps.cut_last && is_cut(a),
            layers[a].params.priority,
            std::cmp::Reverse(depth[a]),
        );
        let kb = (
            ps.cut_last && is_cut(b),
            layers[b].params.priority,
            std::cmp::Reverse(depth[b]),
        );
        ka.cmp(&kb).then(a.cmp(&b))
    });
    idx
}

/// Kontekst przejazdów: strefy i wycięte części.
struct Travel<'a> {
    ps: &'a PlanSettings,
    obstacles: Vec<Obstacle>,
    lifted: bool,
}

impl Travel<'_> {
    fn go(&mut self, from: Point, to: Point, out: &mut Vec<Move>) {
        if from.distance(to) < poly::EPS {
            return;
        }
        match route::route(from, to, &self.obstacles) {
            Route::Path(pts) => {
                for p in pts {
                    out.push(Move::Travel { to: p, lift: false });
                }
            }
            Route::Lift(zone) => {
                self.lifted |= zone;
                out.push(Move::Travel { to, lift: true });
            }
        }
    }

    fn cut_part(&mut self, polys: &[Poly], free: bool) {
        if self.ps.avoid_cut_parts && free {
            for p in polys.iter().filter(|p| p.closed) {
                self.obstacles.push(Obstacle::Part(p.clone(), p.bbox()));
            }
        }
    }
}

/// Zastępuje przejazdy w ruchach trasami omijającymi przeszkody.
fn route_moves(moves: Vec<Move>, start: Point, tr: &mut Travel) -> Vec<Move> {
    if tr.obstacles.is_empty() {
        return moves;
    }
    let mut out = Vec::with_capacity(moves.len());
    let mut at = start;
    for m in moves {
        match m {
            Move::Travel { to, .. } => {
                tr.go(at, to, &mut out);
                at = to;
            }
            Move::Cut { to, .. } => {
                out.push(m);
                at = to;
            }
            Move::Dwell { .. } => out.push(m),
        }
    }
    out
}

/// Złożenie zadania z przygotowanych warstw: kolejność, optymalizacja, ruchy, przejazdy.
fn assemble(
    doc: &Document,
    layers: &[&Layer],
    prepared: &[Arc<Prepared>],
) -> Result<Job, PlanError> {
    let ps = &doc.plan;
    let k = &ps.kinematics;
    let start = Point::new(ps.start[0], ps.start[1]);
    let mut job = Job::empty(start);
    let mut tr = Travel {
        ps,
        obstacles: ps
            .no_go_zones
            .iter()
            .filter(|z| z.travel)
            .map(|z| Obstacle::Zone(z.rect))
            .collect(),
        lifted: false,
    };
    let beam: Vec<[f64; 4]> = ps
        .no_go_zones
        .iter()
        .filter(|z| z.beam)
        .map(|z| z.rect)
        .collect();
    let mut cur = start;
    for li in layer_order(layers, prepared, ps) {
        let pr = &prepared[li];
        job.warnings.extend(pr.warnings.iter().cloned());
        for (si, ps_) in pr.specs.iter().enumerate() {
            let sp = &ps_.spec;
            for pass in 0..sp.passes {
                for (ci, (kind, content)) in ps_.contents.iter().enumerate() {
                    let op_start = cur;
                    let mut moves = Vec::new();
                    match content {
                        Content::Vector(parts, cons) => {
                            for pl in order::order(parts, cons, ps, cur) {
                                let part = &parts[pl.part];
                                let ramp = sp
                                    .params
                                    .ramp
                                    .is_some_and(|r| !r.outer_only || part.depth == 0);
                                for poly in &pl.polys {
                                    let mut pts = vector::part_polyline(
                                        poly, part.free, part.depth, &sp.params,
                                    );
                                    if ps.reduce_direction_changes {
                                        pts = merge_collinear(&pts);
                                    }
                                    let mut tmp = Vec::new();
                                    let end = vector::expand(&pts, sp, ramp, cur, &mut tmp);
                                    moves.extend(route_moves(tmp, cur, &mut tr));
                                    cur = end;
                                }
                                tr.cut_part(&pl.polys, part.free);
                            }
                        }
                        Content::Raster(groups) => {
                            let mut tmp = Vec::new();
                            for g in order_fill_groups(groups, cur) {
                                cur = fill::emit(g, sp, k, cur, &mut tmp);
                            }
                            if !beam.is_empty() {
                                tmp = beam_off(tmp, op_start, &beam);
                            }
                            moves = route_moves(tmp, op_start, &mut tr);
                        }
                    }
                    if sp.params.corner_ramp {
                        moves = time::corner_ramp(&moves, op_start, sp.power_min, k);
                    }
                    if moves.is_empty() {
                        continue;
                    }
                    job.ops.push(Op {
                        layer: sp.layer,
                        label: sp.label.clone(),
                        sublayer: sp.sublayer,
                        pass,
                        kind: *kind,
                        mode: sp.mode,
                        show: sp.show,
                        settings: sp.settings(),
                        pause: (si == 0 && pass == 0 && ci == 0)
                            .then(|| sp.params.pause_message.clone())
                            .flatten(),
                        delay_s: if pass > 0 && ci == 0 {
                            sp.params.pass_delay
                        } else {
                            0.0
                        },
                        z: sp.params.z_offset - sp.params.z_step * pass as f64,
                        objects: pr.objects.clone(),
                        moves,
                    });
                }
            }
        }
        let _ = pr.layer;
    }
    if tr.lifted {
        job.warnings.push(Warning::new(
            "travel_lift",
            "przejazd przez strefę zakazaną bez możliwego objazdu — głowica zostanie podniesiona",
            None,
            Vec::new(),
        ));
    }
    if !ps.cut_last {
        warn::cut_before_engrave(&mut job);
    }
    Ok(job)
}

/// Wyłącza wiązkę na odcinkach rastra wewnątrz stref zakazanych.
fn beam_off(moves: Vec<Move>, start: Point, zones: &[[f64; 4]]) -> Vec<Move> {
    let mut out = Vec::with_capacity(moves.len());
    let mut at = start;
    for m in moves {
        match m {
            Move::Cut { to, power, speed } if power > 0.0 => {
                let mut ts: Vec<(f64, f64)> = zones
                    .iter()
                    .filter_map(|z| route::clip_rect(at, to, *z))
                    .collect();
                ts.sort_by(|a, b| a.0.total_cmp(&b.0));
                let mut t = 0.0;
                for (t0, t1) in ts {
                    if t0 > t {
                        out.push(Move::Cut {
                            to: at + (to - at) * t0,
                            power,
                            speed,
                        });
                    }
                    if t1 > t {
                        out.push(Move::Cut {
                            to: at + (to - at) * t1,
                            power: 0.0,
                            speed,
                        });
                        t = t1;
                    }
                }
                if t < 1.0 {
                    out.push(Move::Cut { to, power, speed });
                }
                at = to;
            }
            Move::Cut { to, .. } | Move::Travel { to, .. } => {
                out.push(m);
                at = to;
            }
            Move::Dwell { .. } => out.push(m),
        }
    }
    out
}

/// Przesuwa zadanie o `-origin` (REQ-08-014).
fn shift(job: &mut Job, origin: Point) {
    let d = origin.to_vec2();
    for op in &mut job.ops {
        for m in &mut op.moves {
            match m {
                Move::Travel { to, .. } | Move::Cut { to, .. } => *to -= d,
                Move::Dwell { .. } => {}
            }
        }
    }
    job.origin = origin;
}

/// Grupy wypełnienia od najbliższej (Flood Fill: obszarami, bez skoków po pustkach).
fn order_fill_groups(groups: &[fill::FillGroup], from: Point) -> Vec<&fill::FillGroup> {
    let mut left: Vec<&fill::FillGroup> = groups.iter().collect();
    let mut out = Vec::with_capacity(left.len());
    let mut at = from;
    while !left.is_empty() {
        let (i, _) = left
            .iter()
            .enumerate()
            .map(|(i, g)| (i, fill::entry(g).distance(at)))
            .min_by(|a, b| {
                a.1.total_cmp(&b.1)
                    .then(left[a.0].order.cmp(&left[b.0].order))
            })
            .expect("niepusta");
        let g = left.remove(i);
        at = fill::entry(g);
        out.push(g);
    }
    out
}

/// Parametry kreatora mostków (REQ-04-085).
#[derive(Debug, Clone, PartialEq)]
pub struct TabsWizard {
    /// Liczba mostków na kontur (0 = z odstępu).
    pub count: u32,
    /// Odstęp, mm (gdy `count` = 0).
    pub spacing: f64,
    /// Rozmiar mostka, mm (`None` = rozmiar z warstwy).
    pub size: Option<f64>,
    /// Pomiń kształty wewnętrzne (zawarte w innych konturach warstwy).
    pub skip_inner: bool,
}

/// Kreator mostków: rozmieszcza mostki ręczne na konturach zamkniętych obiektów
/// (współrzędne lokalne obiektu). Grupy są rozwijane do liści.
pub fn tabs_wizard(
    doc: &Document,
    ids: &[ObjectId],
    w: &TabsWizard,
) -> Result<Vec<(ObjectId, Vec<Tab>)>, PlanError> {
    if w.count == 0 && !(w.spacing > 0.0 && w.spacing.is_finite()) {
        return Err(PlanError::Invalid(
            "podaj count > 0 albo spacing > 0".into(),
        ));
    }
    if let Some(s) = w.size {
        if !(s > 0.0 && s.is_finite()) {
            return Err(PlanError::Invalid("size: > 0".into()));
        }
    }
    let sel: BTreeSet<ObjectId> = ids.iter().copied().collect();
    let items = collect::collect(doc, Some(&sel))?;
    let all = collect::collect(doc, None)?;
    let mut out: Vec<(ObjectId, Vec<Tab>)> = Vec::new();
    for it in &items {
        // zagnieżdżenie liczone w obrębie warstwy (wszystkie obiekty warstwy)
        let layer_items: Vec<Item> = all
            .iter()
            .filter(|a| a.layer == it.layer)
            .cloned()
            .collect();
        let cs = vector::contours(&layer_items, &bc_doc::LayerParams::default())?;
        let Some(o) = doc.find(it.id) else { continue };
        let world = doc.parent_affine(it.id).unwrap_or_default() * o.affine();
        let inv = world.inverse();
        let mut tabs = Vec::new();
        for c in cs
            .iter()
            .filter(|c| layer_items[c.item].id == it.id && c.poly.closed)
        {
            if w.skip_inner && c.depth > 0 {
                continue;
            }
            let len = c.poly.length();
            let n = if w.count > 0 {
                w.count as usize
            } else {
                ((len / w.spacing).floor() as usize).max(1)
            };
            for k in 0..n {
                let (p, _, _) = c.poly.point_at((k as f64 + 0.5) * len / n as f64);
                let l = inv * p;
                tabs.push(Tab {
                    at: [l.x, l.y],
                    size: w.size,
                });
            }
        }
        if !tabs.is_empty() {
            match out.iter_mut().find(|(id, _)| *id == it.id) {
                Some((_, v)) => v.extend(tabs),
                None => out.push((it.id, tabs)),
            }
        }
    }
    Ok(out)
}
