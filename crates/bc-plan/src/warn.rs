//! Ostrzeżenia planera (REQ-07-052, REQ-15-052…056).

use bc_doc::{Document, Layer, LayerMode, ObjectId};

use crate::collect::Item;
use crate::job::{Job, OpKind, Warning};
use crate::spec::Spec;
use crate::PlanOptions;

/// Otwarte kształty w wypełnieniu są pomijane (REQ-15-052).
pub(crate) fn open_in_fill(sp: &Spec, open: &[ObjectId], out: &mut Vec<Warning>) {
    if open.is_empty() {
        return;
    }
    out.push(Warning::new(
        "open_shape_in_fill",
        format!(
            "warstwa {}: {} obiekt(y) z otwartymi kształtami w wypełnieniu — otwarte fragmenty nie zostaną wypełnione",
            sp.label,
            open.len()
        ),
        Some(sp.layer),
        open.to_vec(),
    ));
}

/// Cięcie (Line) przed grawerem (Fill) — REQ-07-052, REQ-15-055.
pub(crate) fn cut_before_engrave(job: &mut Job) {
    let mut seen_cut: Option<usize> = None;
    for (i, op) in job.ops.iter().enumerate() {
        if op.kind == OpKind::Vector && op.mode == LayerMode::Line {
            seen_cut.get_or_insert(i);
        }
        if op.kind == OpKind::Raster {
            if let Some(c) = seen_cut {
                let cut = &job.ops[c];
                let w = Warning::new(
                    "cut_before_engrave",
                    format!(
                        "warstwa cięcia „{}” wypada przed grawerem „{}” — wycięte części mogą się przesunąć przed grawerowaniem (włącz „cięcie na końcu”)",
                        cut.label, op.label
                    ),
                    Some(cut.layer),
                    cut.objects.clone(),
                );
                job.warnings.push(w);
                return;
            }
        }
    }
}

/// Ostrzeżenia dotyczące dokumentu: poza obszarem, duplikaty, moc ponad limit (REQ-15-053, 054, 056).
pub(crate) fn document_warnings(
    doc: &Document,
    items: &[Item],
    layers: &[&Layer],
    opts: &PlanOptions,
    job: &mut Job,
) {
    // poza obszarem roboczym (z kerfem warstwy)
    let (w, h) = (doc.workspace.width, doc.workspace.height);
    let mut outside: Vec<ObjectId> = Vec::new();
    for it in items {
        let Some(l) = layers.iter().find(|l| l.index == it.layer) else {
            continue;
        };
        let Some(b) = bc_geom::bbox(&it.path) else {
            continue;
        };
        let p = &l.params;
        let grow = if l.mode == LayerMode::Line {
            p.kerf_x
                .unwrap_or(p.kerf)
                .max(p.kerf_y.unwrap_or(p.kerf))
                .max(0.0)
        } else {
            0.0
        };
        let eps = 1e-6;
        if b.x0 - grow < -eps
            || b.y0 - grow < -eps
            || b.x1 + grow > w + eps
            || b.y1 + grow > h + eps
        {
            outside.push(it.id);
        }
    }
    outside.dedup();
    if !outside.is_empty() {
        job.warnings.push(Warning::new(
            "outside_workspace",
            format!(
                "{} obiekt(y) wychodzi poza obszar roboczy {w} × {h} mm",
                outside.len()
            ),
            None,
            outside,
        ));
    }
    // duplikaty na tej samej warstwie
    for l in layers {
        let mut its: Vec<(&Item, bc_geom::Rect)> = items
            .iter()
            .filter(|i| i.layer == l.index)
            .filter_map(|i| bc_geom::bbox(&i.path).map(|b| (i, b)))
            .collect();
        its.sort_by(|a, b| a.1.x0.total_cmp(&b.1.x0).then(a.0.order.cmp(&b.0.order)));
        let tol = 1e-3;
        let mut used = vec![false; its.len()];
        let mut groups: Vec<Vec<ObjectId>> = Vec::new();
        for i in 0..its.len() {
            if used[i] {
                continue;
            }
            let mut g = vec![its[i].0.id];
            for j in i + 1..its.len() {
                if its[j].1.x0 - its[i].1.x0 > tol {
                    break;
                }
                let (a, b) = (its[i].1, its[j].1);
                if used[j]
                    || (a.y0 - b.y0).abs() > tol
                    || (a.x1 - b.x1).abs() > tol
                    || (a.y1 - b.y1).abs() > tol
                {
                    continue;
                }
                if bc_geom::ops::same_geometry(&its[i].0.path, &its[j].0.path, tol) {
                    used[j] = true;
                    g.push(its[j].0.id);
                }
            }
            if g.len() > 1 {
                g.sort_unstable();
                groups.push(g);
            }
        }
        groups.sort();
        for g in groups {
            job.warnings.push(Warning::new(
                "duplicate",
                format!(
                    "warstwa {}: {} obiekty o tej samej geometrii — zostaną wycięte kilka razy",
                    l.name,
                    g.len()
                ),
                Some(l.index),
                g,
            ));
        }
    }
    // moc powyżej limitu
    if let Some(limit) = opts.power_limit {
        for l in layers {
            let mut powers = vec![l.power_max];
            powers.extend(l.params.power2_max);
            powers.extend(
                l.params
                    .sublayers
                    .iter()
                    .filter(|s| s.output)
                    .filter_map(|s| s.power_max),
            );
            let max = powers.into_iter().fold(0.0f64, f64::max);
            if max > limit + 1e-9 {
                job.warnings.push(Warning::new(
                    "power_above_limit",
                    format!("warstwa {}: moc {max} % powyżej limitu {limit} %", l.name),
                    Some(l.index),
                    Vec::new(),
                ));
            }
        }
    }
}
