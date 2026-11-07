//! Polecenia struktury dokumentu M1.2: wirtualne tablice, klony, obiekty pomocnicze, szablony.

use bc_doc::{ArrayLayout, Kind, Object, ObjectId};
use bc_geom::{Affine, Point};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::Value;

use crate::builtin::{common_parent, created, top_most, Empty, IdResult, Ids, IdsResult};
use crate::{ok, parse, schema_of, CmdError, CommandSpec, Handler, Registry, SafetyTier, Session};

fn invalid(m: impl Into<String>) -> CmdError {
    CmdError::new("invalid_params", m)
}

fn check_layout(l: &ArrayLayout) -> Result<(), CmdError> {
    l.transforms(Point::ORIGIN)
        .map(|_| ())
        .map_err(|e| invalid(e.to_string()))
}

// ---------- wirtualne tablice ----------

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct VirtualParams {
    #[serde(default)]
    ids: Option<Vec<ObjectId>>,
    /// {"kind":"grid", columns, rows, dx, dy, row_shift?} albo {"kind":"circular", count, center, angle?, rotate_copies?}
    layout: ArrayLayout,
}

fn array_virtual(s: &mut Session, p: Value) -> Result<Value, CmdError> {
    let p: VirtualParams = parse(p)?;
    check_layout(&p.layout)?;
    let ids = top_most(&s.doc, s.targets(&p.ids)?);
    let parent = common_parent(&s.doc, &ids)?;
    let aid = s.doc.alloc_id();
    let list = s
        .doc
        .container_mut(parent)
        .ok_or_else(|| invalid("rodzic nie jest kontenerem"))?;
    let pos = list
        .iter()
        .position(|o| ids.contains(&o.id))
        .expect("co najmniej jeden");
    let mut members = Vec::new();
    list.retain(|o| {
        if ids.contains(&o.id) {
            members.push(o.clone());
            false
        } else {
            true
        }
    });
    let layer = members[0].layer;
    list.insert(
        pos,
        Object::new(
            aid,
            layer,
            Kind::Array {
                children: members,
                layout: p.layout,
            },
        ),
    );
    s.selection = vec![aid];
    crate::builtin::ensure_in_range(&s.doc, &[aid])?;
    ok(IdResult { id: aid })
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct SetLayoutParams {
    /// Id wirtualnej tablicy.
    id: ObjectId,
    layout: ArrayLayout,
}

fn array_set_layout(s: &mut Session, p: Value) -> Result<Value, CmdError> {
    let p: SetLayoutParams = parse(p)?;
    check_layout(&p.layout)?;
    s.targets(&Some(vec![p.id]))?;
    match &mut s.doc.find_mut(p.id).expect("sprawdzone").kind {
        Kind::Array { layout, .. } => *layout = p.layout,
        _ => {
            return Err(invalid(format!(
                "obiekt {} nie jest wirtualną tablicą",
                p.id
            )))
        }
    }
    crate::builtin::ensure_in_range(&s.doc, &[p.id])?;
    ok(IdResult { id: p.id })
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct BakeParams {
    /// Id wirtualnej tablicy.
    id: ObjectId,
}

fn array_bake(s: &mut Session, p: Value) -> Result<Value, CmdError> {
    let p: BakeParams = parse(p)?;
    s.targets(&Some(vec![p.id]))?;
    let arr = s.doc.find(p.id).expect("sprawdzone").clone();
    let Kind::Array { children, .. } = &arr.kind else {
        return Err(invalid(format!(
            "obiekt {} nie jest wirtualną tablicą",
            p.id
        )));
    };
    let transforms = s.doc.array_copies(&arr)?;
    let parent = s.doc.parent_of(p.id).flatten();
    let mut objs = Vec::new();
    for (i, t) in transforms.iter().enumerate() {
        for c in children {
            // pierwsza kopia to oryginał — zachowuje identyfikatory źródła
            let mut o = if i == 0 {
                c.clone()
            } else {
                crate::edit::clone_new_ids(&mut s.doc, c)
            };
            o.transform = (arr.affine() * *t * o.affine()).as_coeffs();
            objs.push(o);
        }
    }
    let list = s.doc.container_mut(parent).expect("kontener istnieje");
    let pos = list
        .iter()
        .position(|o| o.id == p.id)
        .expect("tablica w kontenerze");
    list.remove(pos);
    let ids: Vec<ObjectId> = objs.iter().map(|o| o.id).collect();
    for (k, o) in objs.into_iter().enumerate() {
        list.insert(pos + k, o);
    }
    s.selection = ids.clone();
    ok(IdsResult { ids })
}

// ---------- klony ----------

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct CloneParams {
    #[serde(default)]
    ids: Option<Vec<ObjectId>>,
    /// Przesunięcie klonów względem źródła [dx, dy] (mm).
    #[serde(default)]
    offset: Option<[f64; 2]>,
}

fn object_clone(s: &mut Session, p: Value) -> Result<Value, CmdError> {
    let p: CloneParams = parse(p)?;
    let [dx, dy] = p.offset.unwrap_or([0.0, 0.0]);
    if !(dx.is_finite() && dy.is_finite()) {
        return Err(invalid("offset musi być skończony"));
    }
    let ids = top_most(&s.doc, s.targets(&p.ids)?);
    let mut new = Vec::new();
    for id in ids {
        let layer = s.doc.find(id).expect("sprawdzone").layer;
        let cid = s.doc.alloc_id();
        let mut c = Object::new(cid, layer, Kind::Clone { source: id });
        c.transform = Affine::translate((dx, dy)).as_coeffs();
        s.doc.objects.push(c);
        new.push(cid);
    }
    s.selection = new.clone();
    ok(IdsResult { ids: new })
}

fn object_unlink(s: &mut Session, p: Value) -> Result<Value, CmdError> {
    let p: Ids = parse(p)?;
    let ids = s.targets(&p.ids)?;
    for id in &ids {
        let o = s.doc.find(*id).expect("sprawdzone");
        if !matches!(o.kind, Kind::Clone { .. }) {
            return Err(invalid(format!("obiekt {id} nie jest klonem")));
        }
        let mut m = s.doc.materialize(o)?;
        bc_doc::renumber_kind(&mut m.kind, &mut s.doc.next_id);
        let target = s.doc.find_mut(*id).expect("sprawdzone");
        target.kind = m.kind;
        target.transform = m.transform;
        target.layer = m.layer;
    }
    ok(IdsResult { ids })
}

// ---------- obiekty pomocnicze ----------

fn t2() -> u8 {
    bc_doc::T2_INDEX
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct GuideParams {
    /// Początek linii [x, y] (mm).
    p0: [f64; 2],
    /// Koniec linii [x, y] (mm).
    p1: [f64; 2],
    /// Warstwa (domyślnie T2 — pomocnicza).
    #[serde(default = "t2")]
    layer: u8,
}

fn shape_guide(s: &mut Session, p: Value) -> Result<Value, CmdError> {
    let p: GuideParams = parse(p)?;
    if !p.p0.iter().chain(&p.p1).all(|v| v.is_finite()) || p.p0 == p.p1 {
        return Err(invalid("p0 i p1 muszą być różnymi, skończonymi punktami"));
    }
    let d = format!("M{} {} L{} {}", p.p0[0], p.p0[1], p.p1[0], p.p1[1]);
    let res = created(s, p.layer, Kind::Path { d })?;
    let id = res["id"].as_u64().expect("id");
    s.doc.find_mut(id).expect("utworzony").helper = true;
    Ok(res)
}

// ---------- szablony ----------

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct TemplateParams {
    /// Ścieżka pliku .bcraft-template.
    path: String,
}

fn file_save_template(s: &mut Session, p: Value) -> Result<Value, CmdError> {
    let p: TemplateParams = parse(p)?;
    if !p.path.ends_with(".bcraft-template") {
        return Err(invalid(
            "plik szablonu musi mieć rozszerzenie .bcraft-template",
        ));
    }
    crate::builtin::write_bcraft(s, std::path::Path::new(&p.path), true)?;
    ok(Empty {})
}

// ---------- rejestracja ----------

#[allow(clippy::too_many_arguments)]
fn add<P: JsonSchema, R: JsonSchema>(
    r: &mut Registry,
    id: &'static str,
    title: &'static str,
    description: &'static str,
    undoable: bool,
    requirements: &'static [&'static str],
    handler: Handler,
) {
    r.register(
        CommandSpec {
            executor: crate::Executor::Document,
            id,
            title,
            description,
            params_schema: schema_of::<P>(),
            result_schema: schema_of::<R>(),
            undoable,
            requires_device: false,
            safety_tier: SafetyTier::Design,
            shortcut: None,
            requirements,
        },
        handler,
    );
}

pub(crate) fn register_all(r: &mut Registry) {
    add::<VirtualParams, IdResult>(
        r,
        "array.virtual",
        "Tablica wirtualna",
        "Zamienia obiekty w źródło dynamicznej tablicy: kopie podążają za zmianami źródła.",
        true,
        &["REQ-02-019", "REQ-04-073"],
        array_virtual,
    );
    add::<SetLayoutParams, IdResult>(
        r,
        "array.set_layout",
        "Układ tablicy",
        "Zmienia układ kopii wirtualnej tablicy.",
        true,
        &["REQ-02-019", "REQ-04-073"],
        array_set_layout,
    );
    add::<BakeParams, IdsResult>(
        r,
        "array.bake",
        "Utrwal tablicę",
        "Zamienia wirtualną tablicę na zwykłe, niezależne kopie.",
        true,
        &["REQ-02-019"],
        array_bake,
    );
    add::<CloneParams, IdsResult>(
        r,
        "object.clone",
        "Klonuj",
        "Tworzy klony (obiekty-odniesienia), które powtarzają geometrię źródła.",
        true,
        &["REQ-02-020"],
        object_clone,
    );
    add::<Ids, IdsResult>(
        r,
        "object.unlink",
        "Odłącz klon",
        "Zamienia klony na samodzielne kopie.",
        true,
        &["REQ-02-020"],
        object_unlink,
    );
    add::<GuideParams, IdResult>(
        r,
        "shape.guide",
        "Linia pomocnicza",
        "Linia prowadząca (obiekt pomocniczy, nie wypalany), domyślnie na warstwie T2.",
        true,
        &["REQ-02-021"],
        shape_guide,
    );
    add::<TemplateParams, Empty>(
        r,
        "file.save_template",
        "Zapisz jako szablon",
        "Zapisuje dokument jako .bcraft-template — otwarcie szablonu tworzy nowy dokument.",
        false,
        &["REQ-02-053"],
        file_save_template,
    );
}
