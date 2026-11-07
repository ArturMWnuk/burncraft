//! Polecenia planera (M2.1, ADR 0009): parametry cięcia warstwy, ustawienia
//! planu, mostki ręczne i kreator mostków, plan zadania i estymacja czasu.

use bc_doc::{LayerParams, ObjectId, PlanSettings, Tab};
use bc_geom::Point;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    ok, parse, schema_of, CmdError, CommandSpec, Executor, Handler, Registry, SafetyTier, Session,
};

fn invalid(m: impl Into<String>) -> CmdError {
    CmdError::new("invalid_params", m)
}

/// Scalanie częściowej zmiany: obiekty łączone rekurencyjnie, poza wariantami
/// z polem `kind` (np. overscan) i wartościami prostymi, które są zastępowane.
pub(crate) fn merge(base: &mut Value, patch: &Value) {
    match (base, patch) {
        (Value::Object(b), Value::Object(p)) => {
            for (k, v) in p {
                match b.get_mut(k) {
                    Some(bv) if bv.is_object() && v.is_object() && v.get("kind").is_none() => {
                        merge(bv, v)
                    }
                    _ => {
                        b.insert(k.clone(), v.clone());
                    }
                }
            }
        }
        (b, p) => *b = p.clone(),
    }
}

/// Schemat polecenia z osadzonym schematem struktury pod kluczem `key`
/// (bez pól wymaganych — zmiana częściowa).
pub(crate) fn with_inline(mut root: Value, key: &str, inner: Value) -> Value {
    let mut inner = inner;
    let defs = inner.as_object_mut().and_then(|m| m.remove("$defs"));
    if let Some(m) = inner.as_object_mut() {
        m.remove("required");
        m.remove("title");
    }
    if let Some(Value::Object(d)) = defs {
        let rd = root
            .as_object_mut()
            .expect("schemat obiektu")
            .entry("$defs")
            .or_insert_with(|| Value::Object(Default::default()));
        if let Some(rd) = rd.as_object_mut() {
            rd.extend(d);
        }
    }
    root["properties"][key] = inner;
    root
}

// ---------- layer.params ----------

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct LayerParamsCmd {
    /// Indeks warstwy.
    index: u8,
    /// Zmieniane parametry cięcia (§7.2); pozostałe bez zmian.
    #[serde(default)]
    params: Option<Value>,
    /// Interwał skanowania jako linie na cal (interval = 25,4 / lpi).
    #[serde(default)]
    lpi: Option<f64>,
    /// Interwał skanowania jako DPI (interval = 25,4 / dpi).
    #[serde(default)]
    dpi: Option<f64>,
}

fn layer_params(s: &mut Session, p: Value) -> Result<Value, CmdError> {
    let p: LayerParamsCmd = parse(p)?;
    let l = s
        .doc
        .layer_mut(p.index)
        .map_err(|e| invalid(e.to_string()))?;
    let mut v =
        serde_json::to_value(&l.params).map_err(|e| CmdError::new("failed", e.to_string()))?;
    if let Some(patch) = &p.params {
        if !patch.is_object() {
            return Err(invalid("params: obiekt"));
        }
        merge(&mut v, patch);
    }
    for (name, val) in [("lpi", p.lpi), ("dpi", p.dpi)] {
        if let Some(n) = val {
            if !(n > 0.0 && n.is_finite()) {
                return Err(invalid(format!("{name}: > 0")));
            }
            v["interval"] = Value::from(bc_doc::params::interval_from_lpi(n));
        }
    }
    let np: LayerParams = serde_json::from_value(v).map_err(|e| invalid(format!("params: {e}")))?;
    np.validate().map_err(|e| invalid(format!("params: {e}")))?;
    l.params = np;
    ok(l.clone())
}

// ---------- plan.settings ----------

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct PlanSettingsCmd {
    /// Zmieniane ustawienia planu (§8); pozostałe bez zmian.
    #[serde(default)]
    settings: Option<Value>,
}

fn plan_settings(s: &mut Session, p: Value) -> Result<Value, CmdError> {
    let p: PlanSettingsCmd = parse(p)?;
    let mut v =
        serde_json::to_value(&s.doc.plan).map_err(|e| CmdError::new("failed", e.to_string()))?;
    if let Some(patch) = &p.settings {
        if !patch.is_object() {
            return Err(invalid("settings: obiekt"));
        }
        merge(&mut v, patch);
    }
    let np: PlanSettings =
        serde_json::from_value(v).map_err(|e| invalid(format!("settings: {e}")))?;
    np.validate()
        .map_err(|e| invalid(format!("settings: {e}")))?;
    s.doc.plan = np;
    ok(s.doc.plan.clone())
}

// ---------- mostki ----------

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct WorldTab {
    /// Punkt na konturze w mm (współrzędne obszaru roboczego).
    at: [f64; 2],
    /// Długość przerwy, mm (brak = rozmiar mostków warstwy).
    #[serde(default)]
    size: Option<f64>,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct ObjectTabsCmd {
    /// Id obiektu (kształt, nie grupa).
    id: ObjectId,
    /// Mostki ręczne; pusta lista usuwa mostki obiektu.
    tabs: Vec<WorldTab>,
}

#[derive(Serialize, JsonSchema)]
struct TabsResult {
    id: ObjectId,
    /// Liczba mostków obiektu.
    tabs: usize,
}

fn object_tabs(s: &mut Session, p: Value) -> Result<Value, CmdError> {
    let p: ObjectTabsCmd = parse(p)?;
    let o = s
        .doc
        .find(p.id)
        .ok_or_else(|| CmdError::new("not_found", format!("nie znaleziono obiektu {}", p.id)))?;
    if o.kind.is_container() || matches!(o.kind, bc_doc::Kind::Clone { .. }) {
        return Err(invalid(
            "mostki ustawia się na kształtach, nie na grupach, tablicach i klonach",
        ));
    }
    if o.locked {
        return Err(CmdError::new(
            "locked",
            format!("obiekt {} jest zablokowany", p.id),
        ));
    }
    let inv = (s.doc.parent_affine(p.id).unwrap_or_default() * o.affine()).inverse();
    let mut tabs = Vec::with_capacity(p.tabs.len());
    for t in &p.tabs {
        if !t.at.iter().all(|v| v.is_finite())
            || t.size.is_some_and(|v| !(v > 0.0 && v.is_finite()))
        {
            return Err(invalid("tabs: at — liczby skończone, size > 0"));
        }
        let l = inv * Point::new(t.at[0], t.at[1]);
        tabs.push(Tab {
            at: [l.x, l.y],
            size: t.size,
        });
    }
    let n = tabs.len();
    s.doc.find_mut(p.id).expect("istnieje").tabs = tabs;
    ok(TabsResult { id: p.id, tabs: n })
}

fn yes() -> bool {
    true
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct TabsWizardCmd {
    /// Obiekty; brak = zaznaczenie. Grupy są rozwijane do kształtów.
    #[serde(default)]
    ids: Option<Vec<ObjectId>>,
    /// Liczba mostków na kontur (0 = z odstępu).
    #[serde(default)]
    count: u32,
    /// Odstęp między mostkami, mm (gdy count = 0).
    #[serde(default)]
    spacing: f64,
    /// Długość mostka, mm (brak = rozmiar z warstwy).
    #[serde(default)]
    size: Option<f64>,
    /// Pomiń kształty wewnętrzne.
    #[serde(default = "yes")]
    skip_inner: bool,
    /// Zastąp istniejące mostki (false = dodaj).
    #[serde(default = "yes")]
    replace: bool,
}

#[derive(Serialize, JsonSchema)]
struct WizardResult {
    /// Liczba obiektów z mostkami.
    objects: usize,
    /// Liczba dodanych mostków.
    tabs: usize,
}

fn tabs_wizard(s: &mut Session, p: Value) -> Result<Value, CmdError> {
    let p: TabsWizardCmd = parse(p)?;
    let ids = s.targets(&p.ids)?;
    let w = bc_plan::TabsWizard {
        count: p.count,
        spacing: p.spacing,
        size: p.size,
        skip_inner: p.skip_inner,
    };
    let placed = bc_plan::tabs_wizard(&s.doc, &ids, &w).map_err(|e| invalid(e.to_string()))?;
    let mut total = 0;
    for (id, tabs) in &placed {
        if let Some(o) = s.doc.find_mut(*id) {
            if o.locked {
                continue;
            }
            total += tabs.len();
            if p.replace {
                o.tabs = tabs.clone();
            } else {
                o.tabs.extend(tabs.iter().copied());
            }
        }
    }
    ok(WizardResult {
        objects: placed.len(),
        tabs: total,
    })
}

// ---------- plan.job / plan.estimate ----------

#[derive(Deserialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct PlanJobParams {
    /// Tylko zaznaczone obiekty (REQ-08-013).
    #[serde(default)]
    pub selection_only: bool,
    /// Lewy dolny róg zaznaczenia jako origin (REQ-08-014); wymaga selection_only.
    #[serde(default)]
    pub selection_origin: bool,
    /// Limit mocy (%) — ostrzeżenie power_above_limit (REQ-15-056).
    #[serde(default)]
    pub power_limit: Option<f64>,
    /// Stawka za godzinę pracy maszyny (koszt w raporcie).
    #[serde(default)]
    pub rate_per_hour: Option<f64>,
    /// Liczba wątków planera (0 / brak = liczba rdzeni).
    #[serde(default)]
    pub threads: Option<usize>,
}

#[derive(Serialize, JsonSchema)]
struct OpReport {
    layer: u8,
    label: String,
    sublayer: u32,
    pass: u32,
    kind: bc_plan::OpKind,
    mode: bc_doc::LayerMode,
    /// Długość cięcia z wiązką, mm.
    cut_length: f64,
    time_s: f64,
    pause: Option<String>,
}

#[derive(Serialize, JsonSchema)]
struct Cost {
    rate_per_hour: f64,
    machine_hours: f64,
    machine_cost: f64,
}

#[derive(Serialize, JsonSchema)]
struct JobReport {
    /// SHA-256 kanonicznej postaci zadania (REQ-16-007).
    hash: String,
    time: bc_plan::Estimate,
    /// Czas łącznie, hh:mm:ss.
    formatted_time: String,
    /// mm
    cut_length: f64,
    /// mm
    travel_length: f64,
    /// Ruch roboczy bez wiązki (overscan, perforacja), mm.
    idle_length: f64,
    /// Liczba przebiegów (operacji z przejściami).
    passes: usize,
    layers: usize,
    ops: Vec<OpReport>,
    warnings: Vec<bc_plan::Warning>,
    bbox: Option<[f64; 4]>,
    origin: [f64; 2],
    cost: Cost,
}

#[derive(Serialize, JsonSchema)]
struct EstimateReport {
    total_s: f64,
    cut_s: f64,
    travel_s: f64,
    delay_s: f64,
    dwell_s: f64,
    /// hh:mm:ss
    formatted: String,
    /// Źródło kinematyki: `device` (ustawienia podłączonej maszyny) albo `document`.
    kinematics_source: &'static str,
}

pub(crate) fn hms(s: f64) -> String {
    let t = s.max(0.0).round() as u64;
    format!("{:02}:{:02}:{:02}", t / 3600, t / 60 % 60, t % 60)
}

pub(crate) fn run_plan(s: &mut Session, p: &PlanJobParams) -> Result<bc_plan::Job, CmdError> {
    let selection = if p.selection_only {
        if s.selection.is_empty() {
            return Err(invalid("selection_only: brak zaznaczonych obiektów"));
        }
        Some(s.selection.clone())
    } else {
        if p.selection_origin {
            return Err(invalid("selection_origin wymaga selection_only"));
        }
        None
    };
    if let Some(l) = p.power_limit {
        if !(0.0..=100.0).contains(&l) {
            return Err(invalid("power_limit: 0–100"));
        }
    }
    let opts = bc_plan::PlanOptions {
        selection,
        selection_origin: p.selection_origin,
        threads: p.threads.unwrap_or(0),
        power_limit: p.power_limit,
    };
    let mut planner = std::mem::take(&mut s.planner);
    let res = {
        let sess: &Session = s;
        planner.plan(&sess.doc, &opts, &mut |d, t| {
            sess.tick(d, t).map_err(|e| {
                if e.code == "cancelled" {
                    bc_plan::PlanError::Cancelled
                } else {
                    bc_plan::PlanError::Invalid(e.message)
                }
            })
        })
    };
    s.planner = planner;
    res.map_err(|e| match e {
        bc_plan::PlanError::Cancelled => CmdError::new("cancelled", "anulowano"),
        bc_plan::PlanError::Invalid(m) => CmdError::new("failed", m),
    })
}

/// Raport planu (wspólny dla polecenia `plan.job`, CLI i MCP `plan_job`).
fn report(s: &Session, job: &bc_plan::Job, rate: f64) -> JobReport {
    let k = &crate::machine::kinematics_of(s).0;
    let st = job.stats();
    let time = bc_plan::estimate(job, k);
    let mut at = job.start;
    let mut ops = Vec::with_capacity(job.ops.len());
    for op in &job.ops {
        let one = bc_plan::Job {
            ops: vec![op.clone()],
            warnings: Vec::new(),
            start: at,
            origin: job.origin,
        };
        let os = one.stats();
        ops.push(OpReport {
            layer: op.layer,
            label: op.label.clone(),
            sublayer: op.sublayer,
            pass: op.pass,
            kind: op.kind,
            mode: op.mode,
            cut_length: os.cut_length,
            time_s: bc_plan::estimate(&one, k).total_s,
            pause: op.pause.clone(),
        });
        at = one.end();
    }
    let hours = time.total_s / 3600.0;
    JobReport {
        hash: job.hash(),
        formatted_time: hms(time.total_s),
        time,
        cut_length: st.cut_length,
        travel_length: st.travel_length,
        idle_length: st.idle_length,
        passes: st.passes,
        layers: st.layers,
        ops,
        warnings: job.warnings.clone(),
        bbox: st.bbox,
        origin: [job.origin.x, job.origin.y],
        cost: Cost {
            rate_per_hour: rate,
            machine_hours: hours,
            machine_cost: hours * rate,
        },
    }
}

fn plan_job(s: &mut Session, p: Value) -> Result<Value, CmdError> {
    let p: PlanJobParams = parse(p)?;
    let rate = p.rate_per_hour.unwrap_or(0.0);
    if !(rate >= 0.0 && rate.is_finite()) {
        return Err(invalid("rate_per_hour: ≥ 0"));
    }
    let job = run_plan(s, &p)?;
    ok(report(s, &job, rate))
}

fn plan_estimate(s: &mut Session, p: Value) -> Result<Value, CmdError> {
    let p: PlanJobParams = parse(p)?;
    let job = run_plan(s, &p)?;
    let (k, source) = crate::machine::kinematics_of(s);
    let e = bc_plan::estimate(&job, &k);
    ok(EstimateReport {
        kinematics_source: source,
        total_s: e.total_s,
        cut_s: e.cut_s,
        travel_s: e.travel_s,
        delay_s: e.delay_s,
        dwell_s: e.dwell_s,
        formatted: hms(e.total_s),
    })
}

// ---------- plan.preview / plan.kerf (okno Podgląd, kerf w edytorze) ----------

#[derive(Deserialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
struct PreviewParams {
    /// Tylko zaznaczone obiekty (REQ-08-013).
    #[serde(default)]
    selection_only: bool,
}

#[derive(Serialize, JsonSchema)]
struct PreviewSeg {
    /// Indeks operacji w `ops`.
    op: usize,
    /// travel | cut | dwell
    kind: &'static str,
    /// Punkty w układzie dokumentu, mm.
    from: [f64; 2],
    to: [f64; 2],
    /// Moc, % (0 = wiązka wyłączona).
    power: f64,
    /// mm/s (przejazd: 0).
    speed: f64,
    /// Przedział czasu segmentu od startu zadania, s (REQ-09-001).
    t0: f64,
    t1: f64,
    /// Wektor jednostkowy kierunku ruchu (REQ-09-005); `null` dla postoju.
    dir: Option<[f64; 2]>,
}

#[derive(Serialize, JsonSchema)]
struct PreviewOp {
    layer: u8,
    label: String,
    /// Przejście od 1 i liczba przejść warstwy (pod-warstwy).
    pass: u32,
    passes: u32,
    /// Kolor warstwy #rrggbb.
    color: String,
    kind: bc_plan::OpKind,
    mode: bc_doc::LayerMode,
}

#[derive(Serialize, JsonSchema)]
struct PreviewPart {
    /// Numer w kolejności cięcia od 1 (REQ-09-003).
    index: usize,
    op: usize,
    /// Obiekt, z którego powstała część (jeśli jednoznaczny).
    object: Option<ObjectId>,
    /// Pierwszy segment cięcia części.
    segment: usize,
    /// Punkt startowy (REQ-09-006).
    start: [f64; 2],
}

#[derive(Serialize, JsonSchema)]
struct PreviewResult {
    /// Czas zadania, s (jak `plan.estimate`).
    total_s: f64,
    cut_length: f64,
    travel_length: f64,
    ops: Vec<PreviewOp>,
    segments: Vec<PreviewSeg>,
    parts: Vec<PreviewPart>,
    /// Obrysy kerfu (linia środka wiązki), REQ-07-030 / REQ-09-004.
    kerf: Vec<bc_plan::KerfOutline>,
    /// Szerokość cięcia każdej operacji (2 × kerf), mm; 0 = bez kerfu.
    kerf_width: Vec<f64>,
}

fn plan_preview(s: &mut Session, p: Value) -> Result<Value, CmdError> {
    let p: PreviewParams = parse(p)?;
    let job = run_plan(
        s,
        &PlanJobParams {
            selection_only: p.selection_only,
            ..Default::default()
        },
    )?;
    let k = &crate::machine::kinematics_of(s).0;
    let times = bc_plan::segment_times(&job, k);
    let est = bc_plan::estimate(&job, k);
    let st = job.stats();
    let (ox, oy) = (job.origin.x, job.origin.y);
    let doc_pt = |q: Point| [q.x + ox, q.y + oy];
    let ops: Vec<PreviewOp> = job
        .ops
        .iter()
        .map(|o| PreviewOp {
            layer: o.layer,
            label: o.label.clone(),
            pass: o.pass + 1,
            passes: job
                .ops
                .iter()
                .filter(|x| x.layer == o.layer && x.sublayer == o.sublayer)
                .count() as u32,
            color: s
                .doc
                .layer(o.layer)
                .map(|l| l.color.clone())
                .unwrap_or_else(|_| "#ffffff".into()),
            kind: o.kind,
            mode: o.mode,
        })
        .collect();
    let kerf_width: Vec<f64> = job
        .ops
        .iter()
        .map(|o| {
            s.doc.layer(o.layer).map_or(0.0, |l| {
                let q = &l.params;
                if o.kind == bc_plan::OpKind::Raster {
                    2.0 * q.fill_kerf.abs()
                } else {
                    2.0 * q
                        .kerf_x
                        .unwrap_or(q.kerf)
                        .abs()
                        .max(q.kerf_y.unwrap_or(q.kerf).abs())
                }
            })
        })
        .collect();
    let segs = job.segments();
    let mut out = Vec::with_capacity(segs.len());
    let mut parts: Vec<PreviewPart> = Vec::new();
    // ramki obiektów do przypisania części do obiektu
    let rep = crate::inspect::report(s, crate::inspect::Detail::Summary)?;
    let boxes: std::collections::BTreeMap<ObjectId, [f64; 4]> = rep
        .objects
        .iter()
        .filter_map(|o| o.bbox.map(|b| (o.id, b)))
        .collect();
    let mut raster_seen = std::collections::BTreeSet::new();
    for (i, g) in segs.iter().enumerate() {
        let kind = match g.kind {
            bc_plan::SegKind::Travel => "travel",
            bc_plan::SegKind::Cut => "cut",
            bc_plan::SegKind::Dwell => "dwell",
        };
        let d = g.to - g.from;
        let len = d.hypot();
        let dir = (len > 1e-9).then(|| [d.x / len, d.y / len]);
        let [t0, t1] = times.get(i).copied().unwrap_or([0.0, 0.0]);
        out.push(PreviewSeg {
            op: g.op,
            kind,
            from: doc_pt(g.from),
            to: doc_pt(g.to),
            power: g.power,
            speed: g.speed,
            t0,
            t1,
            dir,
        });
        let starts_part = g.kind == bc_plan::SegKind::Cut
            && g.power > 0.0
            && (i == 0
                || segs[i - 1].kind != bc_plan::SegKind::Cut
                || segs[i - 1].op != g.op
                || segs[i - 1].power <= 0.0);
        if !starts_part {
            continue;
        }
        let op = &job.ops[g.op];
        // raster: jedna część na operację (bez numeru każdej linii skanowania)
        if op.kind == bc_plan::OpKind::Raster && !raster_seen.insert(g.op) {
            continue;
        }
        let start = doc_pt(g.from);
        let object = if op.objects.len() == 1 {
            Some(op.objects[0])
        } else {
            let margin = 1.0 + kerf_width[g.op];
            op.objects
                .iter()
                .filter_map(|id| boxes.get(id).map(|b| (*id, *b)))
                .filter(|(_, b)| {
                    start[0] >= b[0] - margin
                        && start[0] <= b[2] + margin
                        && start[1] >= b[1] - margin
                        && start[1] <= b[3] + margin
                })
                .min_by(|a, b| {
                    let area = |r: &[f64; 4]| (r[2] - r[0]) * (r[3] - r[1]);
                    area(&a.1).total_cmp(&area(&b.1))
                })
                .map(|x| x.0)
        };
        parts.push(PreviewPart {
            index: parts.len() + 1,
            op: g.op,
            object,
            segment: i,
            start,
        });
    }
    let kerf = if p.selection_only {
        let sel: std::collections::BTreeSet<ObjectId> = s.selection.iter().copied().collect();
        kerf_of(s)?
            .into_iter()
            .filter(|o| sel.contains(&o.object))
            .collect()
    } else {
        kerf_of(s)?
    };
    ok(PreviewResult {
        total_s: est.total_s,
        cut_length: st.cut_length,
        travel_length: st.travel_length,
        ops,
        segments: out,
        parts,
        kerf,
        kerf_width,
    })
}

fn kerf_of(s: &Session) -> Result<Vec<bc_plan::KerfOutline>, CmdError> {
    bc_plan::kerf_outlines(&s.doc).map_err(|e| CmdError::new("failed", e.to_string()))
}

fn plan_kerf(s: &mut Session, p: Value) -> Result<Value, CmdError> {
    let _: Empty = parse(p)?;
    ok(serde_json::json!({ "outlines": kerf_of(s)? }))
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct Empty {}

// ---------- rejestracja ----------

#[allow(clippy::too_many_arguments)]
fn reg(
    r: &mut Registry,
    id: &'static str,
    title: &'static str,
    description: &'static str,
    tier: SafetyTier,
    undoable: bool,
    params_schema: Value,
    result_schema: Value,
    requirements: &'static [&'static str],
    handler: Handler,
) {
    r.register(
        CommandSpec {
            executor: Executor::Document,
            id,
            title,
            description,
            params_schema,
            result_schema,
            undoable,
            requires_device: false,
            safety_tier: tier,
            shortcut: None,
            requirements,
        },
        handler,
    );
}

pub(crate) fn register_all(r: &mut Registry) {
    use SafetyTier::*;
    reg(
        r,
        "layer.params",
        "Parametry cięcia warstwy",
        "Zmienia parametry cięcia warstwy z §7.2 (zmiana częściowa: podane pola, reszta bez zmian): drugi laser, opóźnienie między przejściami, pauza z komunikatem, Z offset/step, priorytet, PWM, M3/M4, kerf (osobno X/Y i dla Fill), mostki, lead-in/out, overcut, perforacja, tryb punktowy, rampy, skanowanie (dwukierunkowe, kąt, cross-hatch, interwał, overscan, scan offset), grupowanie Fill, pod-warstwy. lpi/dpi ustawiają interwał.",
        Design,
        true,
        with_inline(schema_of::<LayerParamsCmd>(), "params", schema_of::<LayerParams>()),
        schema_of::<bc_doc::Layer>(),
        &[
            "REQ-07-003", "REQ-07-004", "REQ-07-005", "REQ-07-017", "REQ-07-019", "REQ-07-020", "REQ-07-021", "REQ-07-022",
            "REQ-07-023", "REQ-07-024", "REQ-07-025", "REQ-07-026", "REQ-07-027", "REQ-07-028", "REQ-07-031",
            "REQ-07-032", "REQ-07-033", "REQ-07-034", "REQ-07-035", "REQ-07-036", "REQ-07-037", "REQ-07-038",
            "REQ-07-039", "REQ-07-040", "REQ-07-041", "REQ-07-042", "REQ-07-043", "REQ-07-044", "REQ-07-045",
            "REQ-07-046", "REQ-07-047", "REQ-07-048", "REQ-07-049", "REQ-07-050", "REQ-04-084",
        ],
        layer_params,
    );
    reg(
        r,
        "plan.settings",
        "Ustawienia planu",
        "Zmienia ustawienia planu dokumentu (§8; zmiana częściowa): cięcie na końcu, wewnętrzne przed zewnętrznymi, grupy, punkt startowy, kierunek, 2-opt, objazd wyciętych części, usuwanie nakładających się linii, redukcja zmian kierunku, strefy zakazane, pasy rastra, kinematyka maszyny (prędkości i przyspieszenia X/Y, $11), pozycja startowa głowicy.",
        Design,
        true,
        with_inline(schema_of::<PlanSettingsCmd>(), "settings", schema_of::<PlanSettings>()),
        schema_of::<PlanSettings>(),
        &[
            "REQ-07-051", "REQ-08-003", "REQ-08-004", "REQ-08-005", "REQ-08-006", "REQ-08-007", "REQ-08-008",
            "REQ-08-009", "REQ-08-010", "REQ-08-011", "REQ-08-012", "REQ-08-015", "REQ-08-018",
        ],
        plan_settings,
    );
    reg(
        r,
        "object.tabs",
        "Mostki ręczne",
        "Ustawia mostki (przerwy w cięciu) obiektu w podanych punktach konturu (mm, układ obszaru roboczego). Pusta lista usuwa mostki.",
        Design,
        true,
        schema_of::<ObjectTabsCmd>(),
        schema_of::<TabsResult>(),
        &["REQ-04-083"],
        object_tabs,
    );
    reg(
        r,
        "plan.tabs_wizard",
        "Kreator mostków",
        "Rozmieszcza mostki ręczne na konturach zamkniętych obiektów (ids albo zaznaczenie): liczba albo odstęp, rozmiar, pomijanie kształtów wewnętrznych.",
        Design,
        true,
        schema_of::<TabsWizardCmd>(),
        schema_of::<WizardResult>(),
        &["REQ-04-085"],
        tabs_wizard,
    );
    reg(
        r,
        "plan.job",
        "Plan zadania",
        "Planuje zadanie i zwraca statystyki: czas (model przyspieszeń), długość cięcia i przejazdów, przebiegi, operacje, ostrzeżenia (otwarte kształty w Fill, poza obszarem, duplikaty, cięcie przed grawerem, moc ponad limit, strefy zakazane), koszt czasu maszyny, hash zadania. Nie zmienia dokumentu.",
        Read,
        false,
        schema_of::<PlanJobParams>(),
        schema_of::<JobReport>(),
        &[
            "REQ-15-051", "REQ-15-052", "REQ-15-053", "REQ-15-054", "REQ-15-055", "REQ-15-056", "REQ-07-052",
            "REQ-08-001", "REQ-08-002", "REQ-08-013", "REQ-08-014", "REQ-08-019", "REQ-16-007",
        ],
        plan_job,
    );
    reg(
        r,
        "plan.estimate",
        "Czas zadania",
        "Szacowany czas zadania z modelu trapezowego (prędkości i przyspieszenia osi X/Y, odchylenie na złączu): cięcie, przejazdy, opóźnienia, postoje.",
        Read,
        false,
        schema_of::<PlanJobParams>(),
        schema_of::<EstimateReport>(),
        &["REQ-09-008", "REQ-08-018"],
        plan_estimate,
    );
    reg(
        r,
        "plan.preview",
        "Podgląd zadania",
        "Dane okna Podgląd: segmenty zadania w układzie dokumentu z przedziałami czasu (odtwarzanie, suwak, prędkość), rodzajem (przejazd / cięcie / postój), mocą i kierunkiem; operacje z kolorem i licznikiem przejść; części w kolejności cięcia z punktami startowymi; obrysy kerfu i szerokość cięcia. Nie zmienia dokumentu.",
        Read,
        false,
        schema_of::<PreviewParams>(),
        schema_of::<PreviewResult>(),
        &[
            "REQ-09-001", "REQ-09-002", "REQ-09-003", "REQ-09-004", "REQ-09-005", "REQ-09-006",
            "REQ-09-013", "REQ-07-030", "REQ-03-046",
        ],
        plan_preview,
    );
    reg(
        r,
        "plan.kerf",
        "Obrysy kerfu",
        "Obrysy kerfu warstw z kerfem ≠ 0 (linia środka wiązki po korekcie, z osobnym kerfem X/Y) do wizualizacji w edytorze.",
        Read,
        false,
        schema_of::<Empty>(),
        serde_json::json!({"type": "object"}),
        &["REQ-07-029"],
        plan_kerf,
    );
}
