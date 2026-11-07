//! Zbiór geometrii do planowania: liście drzewa obiektów we współrzędnych
//! świata z grupą najwyższego poziomu, priorytetem i mostkami ręcznymi.

use std::collections::BTreeSet;

use bc_doc::{Document, Kind, Object, ObjectId, MAX_CLONE_DEPTH};
use bc_geom::{Affine, BezPath, Point};

use crate::PlanError;

/// Liść dokumentu przygotowany do planowania.
#[derive(Debug, Clone)]
pub(crate) struct Item {
    pub id: ObjectId,
    /// Kolejność w dokumencie (rozstrzyga remisy).
    pub order: usize,
    /// Indeks obiektu najwyższego poziomu (grupa).
    pub group: usize,
    pub priority: i32,
    pub layer: u8,
    pub path: BezPath,
    /// Mostki ręczne: punkt w świecie i rozmiar.
    pub tabs: Vec<(Point, Option<f64>)>,
}

struct Ctx<'a> {
    doc: &'a Document,
    selection: Option<&'a BTreeSet<ObjectId>>,
    out: Vec<Item>,
}

/// Liście do wypalenia (widoczne, bez obiektów pomocniczych), opcjonalnie tylko zaznaczone.
pub(crate) fn collect(
    doc: &Document,
    selection: Option<&BTreeSet<ObjectId>>,
) -> Result<Vec<Item>, PlanError> {
    let mut cx = Ctx {
        doc,
        selection,
        out: Vec::new(),
    };
    for (gi, top) in doc.objects.iter().enumerate() {
        let sel = selection.is_some_and(|s| s.contains(&top.id));
        walk(&mut cx, top, Affine::IDENTITY, 0, gi, top, sel)?;
    }
    Ok(cx.out)
}

fn walk(
    cx: &mut Ctx,
    o: &Object,
    parent: Affine,
    depth: u8,
    group: usize,
    top: &Object,
    selected: bool,
) -> Result<(), PlanError> {
    if !o.visible || o.helper {
        return Ok(());
    }
    let a = parent * o.affine();
    let selected = selected || cx.selection.is_some_and(|s| s.contains(&o.id));
    match &o.kind {
        Kind::Group { children } => {
            for c in children {
                walk(cx, c, a, depth, group, top, selected)?;
            }
        }
        Kind::Array { children, .. } => {
            let copies = cx.doc.array_copies(o).map_err(PlanError::doc)?;
            for t in copies {
                for c in children {
                    walk(cx, c, a * t, depth, group, top, selected)?;
                }
            }
        }
        Kind::Clone { source } => {
            if depth >= MAX_CLONE_DEPTH {
                return Err(PlanError::Invalid(format!(
                    "cykl lub zbyt głębokie odwołania klonów (obiekt {})",
                    o.id
                )));
            }
            let doc = cx.doc;
            let src = doc.find(*source).ok_or_else(|| {
                PlanError::Invalid(format!(
                    "klon {} wskazuje nieistniejący obiekt {source}",
                    o.id
                ))
            })?;
            let pa = doc.parent_affine(*source).unwrap_or(Affine::IDENTITY);
            walk(cx, src, a * pa, depth + 1, group, top, selected)?;
        }
        _ => {
            if cx.selection.is_some() && !selected {
                return Ok(());
            }
            let mut path = o.local_path().map_err(PlanError::doc)?;
            path.apply_affine(a);
            let priority = if o.id == top.id {
                top.priority
            } else {
                top.priority + o.priority
            };
            let tabs = o
                .tabs
                .iter()
                .map(|t| (a * Point::new(t.at[0], t.at[1]), t.size))
                .collect();
            let order = cx.out.len();
            cx.out.push(Item {
                id: o.id,
                order,
                group,
                priority,
                layer: o.layer,
                path,
                tabs,
            });
        }
    }
    Ok(())
}
