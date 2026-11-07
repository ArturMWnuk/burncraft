//! Polecenia okien Właściwości kształtu (REQ-03-047), Historia cofania z
//! podglądem stanów (REQ-03-048) i Notatki z obrazami (REQ-03-049).

use base64::Engine;
use bc_doc::{Kind, ObjectId};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::builtin::{finite, positive, IdResult};
use crate::edit::{add, invalid};
use crate::{ok, parse, CmdError, Registry, SafetyTier, Session};

const B64: base64::engine::GeneralPurpose = base64::engine::general_purpose::STANDARD;

// ---------- parametry kształtów ----------

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct SetParams {
    /// Id kształtu parametrycznego (rect, ellipse, polygon, star, arc).
    id: ObjectId,
    /// Zmieniane pola, np. {"radius": 3} dla prostokąta, {"sides": 8} dla wielokąta.
    params: Map<String, Value>,
}

/// Pola długości (> 0) i liczby ramion/boków z minimum.
const LENGTHS: &[&str] = &["w", "h", "rx", "ry", "r", "r_outer", "r_inner", "r_middle"];

fn check_field(key: &str, v: &Value) -> Result<(), CmdError> {
    match key {
        "sides" | "points" => {
            let min = if key == "sides" { 3 } else { 2 };
            match v.as_u64() {
                Some(n) if n >= min && n <= 1000 => Ok(()),
                _ => Err(invalid(format!("{key}: liczba całkowita od {min} do 1000"))),
            }
        }
        "r_middle" if v.is_null() => Ok(()),
        "radius" => match v.as_f64() {
            Some(x) if x.is_finite() && x >= 0.0 => Ok(()),
            _ => Err(invalid("radius: liczba ≥ 0")),
        },
        k if LENGTHS.contains(&k) => positive(k, v.as_f64().unwrap_or(f64::NAN)).map(|_| ()),
        k => finite(k, v.as_f64().unwrap_or(f64::NAN)).map(|_| ()),
    }
}

fn shape_set_params(s: &mut Session, p: Value) -> Result<Value, CmdError> {
    let p: SetParams = parse(p)?;
    let o = s
        .doc
        .find(p.id)
        .ok_or_else(|| CmdError::new("not_found", format!("nie znaleziono obiektu {}", p.id)))?;
    if o.locked {
        return Err(CmdError::new(
            "locked",
            format!("obiekt {} jest zablokowany", p.id),
        ));
    }
    if !matches!(
        o.kind,
        Kind::Rect { .. }
            | Kind::Ellipse { .. }
            | Kind::Polygon { .. }
            | Kind::Star { .. }
            | Kind::Arc { .. }
    ) {
        return Err(invalid(format!(
            "obiekt {} ({}) nie jest kształtem parametrycznym",
            p.id,
            o.kind.type_name()
        )));
    }
    // zmiana przez JSON rodzaju: te same nazwy pól co w poleceniach shape.*
    let mut v =
        serde_json::to_value(&o.kind).map_err(|e| CmdError::new("failed", e.to_string()))?;
    let fields = v.as_object_mut().expect("rodzaj jest obiektem");
    for (k, val) in p.params {
        let known =
            fields.contains_key(&k) || (k == "r_middle" && matches!(o.kind, Kind::Star { .. }));
        if k == "type" || !known {
            return Err(invalid(format!(
                "{} nie ma parametru {k}",
                o.kind.type_name()
            )));
        }
        check_field(&k, &val)?;
        fields.insert(k, val);
    }
    let kind: Kind = serde_json::from_value(v).map_err(|e| invalid(e.to_string()))?;
    let obj = s.doc.find_mut(p.id).expect("sprawdzone");
    obj.kind = kind;
    // geometria (np. promień wcięć gwiazdy większy od ramion) i zakres współrzędnych
    obj.local_path().map_err(|e| invalid(e.to_string()))?;
    crate::builtin::ensure_in_range(&s.doc, &[p.id])?;
    ok(IdResult { id: p.id })
}

// ---------- podgląd stanów historii ----------

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct PreviewParams {
    /// Kroki od stanu bieżącego: −k = stan po k cofnięciach, +k = po k ponowieniach, 0 = bieżący.
    offset: i64,
    /// Dłuższy bok miniatury w pikselach (16…1024, domyślnie 192).
    #[serde(default = "default_size")]
    max_size: u32,
}

fn default_size() -> u32 {
    192
}

#[derive(Serialize, JsonSchema)]
struct PreviewResult {
    /// Etykieta kroku prowadzącego do tego stanu (pusta dla bieżącego).
    label: String,
    /// Liczba obiektów w tym stanie.
    objects: usize,
    width: u32,
    height: u32,
    png_base64: String,
}

fn history_preview(s: &mut Session, p: Value) -> Result<Value, CmdError> {
    let p: PreviewParams = parse(p)?;
    if !(16..=1024).contains(&p.max_size) {
        return Err(invalid("max_size: od 16 do 1024"));
    }
    let (doc, label) = if p.offset == 0 {
        (&s.doc, "")
    } else {
        s.history
            .state(p.offset)
            .ok_or_else(|| invalid(format!("offset {}: brak takiego kroku historii", p.offset)))?
    };
    let img = bc_sim::render_design(
        doc,
        &bc_sim::PreviewOptions {
            max_size: p.max_size,
            ..Default::default()
        },
    )
    .map_err(|e| CmdError::new("failed", e.to_string()))?;
    ok(PreviewResult {
        label: label.to_string(),
        objects: doc.all_objects().len(),
        width: img.width,
        height: img.height,
        png_base64: B64.encode(&img.png),
    })
}

// ---------- obrazy notatek ----------

/// Limit rozmiaru jednego obrazu notatki.
const MAX_IMAGE: usize = 10 << 20;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct AddImageParams {
    /// Obraz PNG albo JPEG (base64, do 10 MB).
    data_base64: String,
    /// Nazwa w archiwum ([a-z0-9_-]+.png|jpg); brak = image-N.
    #[serde(default)]
    name: Option<String>,
}

#[derive(Serialize, JsonSchema)]
struct AddImageResult {
    name: String,
    /// Odwołanie dopisane do notatek.
    markdown: String,
}

fn reference(name: &str) -> String {
    format!("![](notes/{name})")
}

fn add_note_image(s: &mut Session, p: Value) -> Result<Value, CmdError> {
    let p: AddImageParams = parse(p)?;
    let data = B64
        .decode(p.data_base64.trim())
        .map_err(|e| invalid(format!("data_base64: {e}")))?;
    if data.len() > MAX_IMAGE {
        return Err(invalid("obraz większy niż 10 MB"));
    }
    let mime =
        bc_doc::note_image_mime(&data).ok_or_else(|| invalid("obsługiwane obrazy: PNG, JPEG"))?;
    let ext = if mime == "image/png" { "png" } else { "jpg" };
    let images = &s.doc.note_images;
    let name = match p.name {
        Some(n) if !bc_doc::valid_note_image_name(&n) => {
            return Err(invalid(format!("name {n}: dozwolone [a-z0-9_-]+.png|jpg")))
        }
        Some(n) if images.contains_key(&n) => {
            return Err(invalid(format!("obraz {n} już istnieje")))
        }
        Some(n) => n,
        None => (1..)
            .map(|i| format!("image-{i}.{ext}"))
            .find(|n| !images.contains_key(n))
            .expect("wolna nazwa"),
    };
    let markdown = reference(&name);
    let notes = &mut s.doc.notes;
    if !notes.is_empty() {
        notes.push_str(if notes.ends_with('\n') { "\n" } else { "\n\n" });
    }
    notes.push_str(&markdown);
    s.doc.note_images.insert(name.clone(), data.into());
    ok(AddImageResult { name, markdown })
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct ImageName {
    /// Nazwa obrazu notatki.
    name: String,
}

#[derive(Serialize, JsonSchema)]
struct ImageData {
    name: String,
    mime: String,
    size: usize,
    data_base64: String,
}

fn missing(name: &str) -> CmdError {
    CmdError::new("not_found", format!("brak obrazu notatki {name}"))
}

fn note_image(s: &mut Session, p: Value) -> Result<Value, CmdError> {
    let p: ImageName = parse(p)?;
    let data = s
        .doc
        .note_images
        .get(&p.name)
        .ok_or_else(|| missing(&p.name))?;
    ok(ImageData {
        mime: bc_doc::note_image_mime(data)
            .unwrap_or("application/octet-stream")
            .into(),
        size: data.len(),
        data_base64: B64.encode(data),
        name: p.name,
    })
}

fn remove_note_image(s: &mut Session, p: Value) -> Result<Value, CmdError> {
    let p: ImageName = parse(p)?;
    s.doc
        .note_images
        .remove(&p.name)
        .ok_or_else(|| missing(&p.name))?;
    let r = reference(&p.name);
    let kept: Vec<&str> = s.doc.notes.lines().filter(|l| l.trim() != r).collect();
    let mut text = kept.join("\n").replace(&r, "");
    while text.ends_with('\n') {
        text.pop();
    }
    s.doc.notes = text;
    ok(serde_json::json!({}))
}

pub(crate) fn register_all(r: &mut Registry) {
    use SafetyTier::*;
    add::<SetParams, IdResult>(r, "shape.set_params", "Parametry kształtu", "Zmienia parametry kształtu parametrycznego (okno Właściwości kształtu): prostokąt w/h/radius, elipsa rx/ry, wielokąt r/sides/rotation, gwiazda r_outer/r_inner/r_middle/points/rotation, łuk r/start/sweep.", Design, true, &["REQ-03-047", "REQ-02-033"], shape_set_params);
    add::<PreviewParams, PreviewResult>(r, "edit.history_preview", "Podgląd stanu historii", "Miniatura PNG dokumentu w stanie odległym o offset kroków historii (−k = cofnięcia, +k = ponowienia); niczego nie zmienia.", Read, false, &["REQ-03-048"], history_preview);
    add::<AddImageParams, AddImageResult>(r, "document.add_note_image", "Dodaj obraz do notatek", "Dodaje obraz PNG/JPEG do notatek dokumentu (zapisywany w .bcraft) i dopisuje odwołanie ![](notes/nazwa).", Design, true, &["REQ-03-049"], add_note_image);
    add::<ImageName, ImageData>(
        r,
        "document.note_image",
        "Obraz notatki",
        "Zwraca obraz notatki (base64).",
        Read,
        false,
        &["REQ-03-049"],
        note_image,
    );
    add::<ImageName, crate::builtin::Empty>(
        r,
        "document.remove_note_image",
        "Usuń obraz z notatek",
        "Usuwa obraz notatki razem z odwołaniem w tekście.",
        Design,
        true,
        &["REQ-03-049"],
        remove_note_image,
    );
}
