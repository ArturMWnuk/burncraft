//! Format natywny `.bcraft` (ADR 0004).
//!
//! Archiwum ZIP:
//! - `mimetype` — `application/x-burncraft` (pierwszy wpis, bez kompresji),
//! - `document.json` — dokument wg `schemas/document.schema.json`,
//! - `notes.md` — notatki dokumentu,
//! - `notes/<nazwa>.png|jpg` — obrazy notatek (REQ-03-049),
//! - `thumbnail.png` — miniatura (opcjonalnie).
//!
//! Odczyt: JSON → migracja do bieżącego schematu → walidacja schematem JSON →
//! deserializacja → kontrola niezmienników ([`Document::validate`]).

use std::io::{Cursor, Read, Write};
use std::sync::OnceLock;

use serde_json::Value;
use zip::write::SimpleFileOptions;

use crate::{DocError, Document, SCHEMA_VERSION};

pub const MIMETYPE: &str = "application/x-burncraft";
/// Szablon `.bcraft-template`: ten sam kontener, otwiera się jako nowy dokument (REQ-02-053).
pub const TEMPLATE_MIMETYPE: &str = "application/x-burncraft-template";
pub const SCHEMA_JSON: &str = include_str!("../../../schemas/document.schema.json");

/// Dodatkowe zasoby archiwum poza samym dokumentem.
#[derive(Debug, Default, Clone)]
pub struct Extras {
    pub thumbnail_png: Option<Vec<u8>>,
    /// Plik jest szablonem (`.bcraft-template`).
    pub template: bool,
}

fn validator() -> &'static jsonschema::Validator {
    static V: OnceLock<jsonschema::Validator> = OnceLock::new();
    V.get_or_init(|| {
        let schema: Value =
            serde_json::from_str(SCHEMA_JSON).expect("schemat dokumentu jest poprawnym JSON");
        jsonschema::validator_for(&schema).expect("schemat dokumentu jest poprawny")
    })
}

/// Waliduje `document.json` względem schematu; zwraca listę błędów.
pub fn schema_errors(v: &Value) -> Vec<String> {
    validator()
        .iter_errors(v)
        .map(|e| format!("{}: {}", e.instance_path(), e))
        .collect()
}

/// Migruje surowy JSON dokumentu do [`SCHEMA_VERSION`].
///
/// Schemat 0 (prototyp przedwydaniowy): pole `format_version` zamiast `schema`,
/// obiekty odwołują się do warstwy polem `color` zamiast `layer`.
pub fn migrate(mut v: Value) -> Result<Value, DocError> {
    let version = v
        .get("schema")
        .or_else(|| v.get("format_version"))
        .and_then(Value::as_u64)
        .ok_or_else(|| DocError::Invalid("brak wersji schematu".into()))? as u32;
    if version > SCHEMA_VERSION {
        return Err(DocError::Invalid(format!(
            "plik zapisany nowszą wersją programu (schemat {version}, obsługiwany {SCHEMA_VERSION})"
        )));
    }
    if version == 0 {
        let obj = v
            .as_object_mut()
            .ok_or_else(|| DocError::Invalid("dokument nie jest obiektem".into()))?;
        obj.remove("format_version");
        obj.insert("schema".into(), Value::from(1));
        fn fix(list: &mut Value) {
            if let Some(arr) = list.as_array_mut() {
                for o in arr {
                    if let Some(m) = o.as_object_mut() {
                        if let Some(c) = m.remove("color") {
                            m.insert("layer".into(), c);
                        }
                        if let Some(ch) = m.get_mut("children") {
                            fix(ch);
                        }
                    }
                }
            }
        }
        if let Some(objs) = obj.get_mut("objects") {
            fix(objs);
        }
    }
    let version = v.get("schema").and_then(Value::as_u64).unwrap_or(1) as u32;
    if version == 1 {
        // 1 -> 2: nowe pola (rola warstwy, atrybuty obiektów) mają wartości domyślne
        v["schema"] = Value::from(2);
    }
    let version = v.get("schema").and_then(Value::as_u64).unwrap_or(2) as u32;
    if version == 2 {
        // 2 -> 3: nowe typy (array, clone) i flaga helper — stare dokumenty ich nie używają
        v["schema"] = Value::from(3);
    }
    let version = v.get("schema").and_then(Value::as_u64).unwrap_or(3) as u32;
    if version == 3 {
        // 3 -> 4: parametry cięcia warstw, ustawienia planu i mostki (ADR 0009) mają wartości domyślne
        v["schema"] = Value::from(4);
    }
    let version = v.get("schema").and_then(Value::as_u64).unwrap_or(4) as u32;
    if version == 4 {
        // 4 -> 5: ustawienia startu, kompilatora G-code i makra warstw (ADR 0011) mają wartości domyślne
        v["schema"] = Value::from(5);
    }
    Ok(v)
}

/// Parsuje `document.json` (z migracją i walidacją).
pub fn document_from_json(text: &str) -> Result<Document, DocError> {
    let raw: Value =
        serde_json::from_str(text).map_err(|e| DocError::Invalid(format!("JSON: {e}")))?;
    let v = migrate(raw)?;
    let errs = schema_errors(&v);
    if !errs.is_empty() {
        return Err(DocError::Invalid(format!(
            "niezgodność ze schematem: {}",
            errs.join("; ")
        )));
    }
    let doc: Document = serde_json::from_value(v).map_err(|e| DocError::Invalid(e.to_string()))?;
    doc.validate()?;
    Ok(doc)
}

pub fn to_bytes(doc: &Document, extras: &Extras) -> Result<Vec<u8>, DocError> {
    let io = |e: zip::result::ZipError| DocError::Io(e.to_string());
    let mut zw = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let stored = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    let deflated =
        SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    zw.start_file("mimetype", stored).map_err(io)?;
    zw.write_all(
        if extras.template {
            TEMPLATE_MIMETYPE
        } else {
            MIMETYPE
        }
        .as_bytes(),
    )
    .map_err(|e| DocError::Io(e.to_string()))?;
    zw.start_file("document.json", deflated).map_err(io)?;
    let json = serde_json::to_vec_pretty(doc).map_err(|e| DocError::Io(e.to_string()))?;
    zw.write_all(&json)
        .map_err(|e| DocError::Io(e.to_string()))?;
    zw.start_file("notes.md", deflated).map_err(io)?;
    zw.write_all(doc.notes.as_bytes())
        .map_err(|e| DocError::Io(e.to_string()))?;
    for (name, data) in &doc.note_images {
        zw.start_file(format!("notes/{name}"), stored).map_err(io)?;
        zw.write_all(data)
            .map_err(|e| DocError::Io(e.to_string()))?;
    }
    if let Some(png) = &extras.thumbnail_png {
        zw.start_file("thumbnail.png", stored).map_err(io)?;
        zw.write_all(png).map_err(|e| DocError::Io(e.to_string()))?;
    }
    Ok(zw.finish().map_err(io)?.into_inner())
}

pub fn from_bytes(bytes: &[u8]) -> Result<(Document, Extras), DocError> {
    let io = |e: zip::result::ZipError| DocError::Io(e.to_string());
    let mut za = zip::ZipArchive::new(Cursor::new(bytes)).map_err(io)?;
    let mut read = |name: &str| -> Result<Option<Vec<u8>>, DocError> {
        match za.by_name(name) {
            Ok(mut f) => {
                let mut buf = Vec::new();
                f.read_to_end(&mut buf)
                    .map_err(|e| DocError::Io(e.to_string()))?;
                Ok(Some(buf))
            }
            Err(zip::result::ZipError::FileNotFound) => Ok(None),
            Err(e) => Err(io(e)),
        }
    };
    let template = match read("mimetype")? {
        Some(m) if m == MIMETYPE.as_bytes() => false,
        Some(m) if m == TEMPLATE_MIMETYPE.as_bytes() => true,
        _ => {
            return Err(DocError::Invalid(
                "to nie jest plik .bcraft (brak mimetype)".into(),
            ))
        }
    };
    let json =
        read("document.json")?.ok_or_else(|| DocError::Invalid("brak document.json".into()))?;
    let mut doc = document_from_json(&String::from_utf8_lossy(&json))?;
    if let Some(n) = read("notes.md")? {
        doc.notes = String::from_utf8_lossy(&n).into_owned();
    }
    let thumbnail_png = read("thumbnail.png")?;
    for i in 0..za.len() {
        let mut f = za.by_index(i).map_err(io)?;
        let full = f.name().map_err(io)?.into_owned();
        let Some(name) = full
            .strip_prefix("notes/")
            .filter(|n| crate::valid_note_image_name(n))
            .map(str::to_string)
        else {
            continue;
        };
        let mut buf = Vec::new();
        f.read_to_end(&mut buf)
            .map_err(|e| DocError::Io(e.to_string()))?;
        if crate::note_image_mime(&buf).is_some() {
            doc.note_images.insert(name, buf.into());
        }
    }
    Ok((
        doc,
        Extras {
            thumbnail_png,
            template,
        },
    ))
}

pub fn save(doc: &Document, extras: &Extras, path: &std::path::Path) -> Result<(), DocError> {
    let bytes = to_bytes(doc, extras)?;
    // zapis atomowy: plik tymczasowy obok i zamiana
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = std::path::PathBuf::from(tmp);
    std::fs::write(&tmp, bytes).map_err(|e| DocError::Io(e.to_string()))?;
    std::fs::rename(&tmp, path).map_err(|e| DocError::Io(e.to_string()))
}

pub fn load(path: &std::path::Path) -> Result<(Document, Extras), DocError> {
    let bytes =
        std::fs::read(path).map_err(|e| DocError::Io(format!("{}: {e}", path.display())))?;
    from_bytes(&bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Kind;

    fn sample() -> Document {
        let mut d = Document::new("Próbka");
        d.add(
            0,
            Kind::Rect {
                x: 0.,
                y: 0.,
                w: 50.,
                h: 30.,
                radius: 3.,
            },
        )
        .unwrap();
        d.add(
            2,
            Kind::Path {
                d: "M0 0 C 0 10 10 10 10 0 Z".into(),
            },
        )
        .unwrap();
        d.notes = "Sklejka 3 mm, ostrość po wymianie stołu".into();
        d
    }

    #[test]
    fn req_02_046_bcraft_is_zip_with_document_json() {
        let bytes = to_bytes(&sample(), &Extras::default()).unwrap();
        assert_eq!(&bytes[0..2], b"PK");
        let mut za = zip::ZipArchive::new(Cursor::new(&bytes[..])).unwrap();
        assert!(za.by_name("document.json").is_ok());
        let (d, _) = from_bytes(&bytes).unwrap();
        assert_eq!(d, sample());
    }

    #[test]
    fn req_00_016_own_native_format() {
        let bytes = to_bytes(&sample(), &Extras::default()).unwrap();
        let mut za = zip::ZipArchive::new(Cursor::new(&bytes[..])).unwrap();
        let mut m = String::new();
        za.by_name("mimetype")
            .unwrap()
            .read_to_string(&mut m)
            .unwrap();
        assert_eq!(m, MIMETYPE);
    }

    #[test]
    fn req_02_047_schema_versioned_and_validated() {
        let mut v = serde_json::to_value(sample()).unwrap();
        assert_eq!(v["schema"], SCHEMA_VERSION);
        assert!(schema_errors(&v).is_empty(), "{:?}", schema_errors(&v));
        v["layers"][0]["power_max"] = Value::from(150);
        assert!(!schema_errors(&v).is_empty());
        assert!(document_from_json(&v.to_string()).is_err());
        let mut v2 = serde_json::to_value(sample()).unwrap();
        v2["objects"][0]["w"] = Value::from(-1);
        assert!(document_from_json(&v2.to_string()).is_err());
    }

    #[test]
    fn req_02_052_migration_from_schema_0() {
        let v0 = r##"{"format_version":0,"name":"stary","units":"mm","workspace":{"width":300,"height":200},
            "layers":[{"index":0,"color":"#000000","name":"C00","mode":"line","speed":10,"power_max":50,
            "power_min":10,"passes":1,"output":true,"show":true,"air_assist":false}],
            "objects":[{"id":1,"type":"rect","color":0,"visible":true,"locked":false,
            "transform":[1,0,0,1,0,0],"x":0,"y":0,"w":10,"h":10,"radius":0}],"next_id":2}"##;
        let d = document_from_json(v0).unwrap();
        assert_eq!(d.schema, SCHEMA_VERSION, "migracja 0 -> 1 -> bieżąca");
        assert_eq!(d.objects[0].layer, 0);
        let future = r#"{"schema":99}"#;
        assert!(
            matches!(document_from_json(future), Err(DocError::Invalid(m)) if m.contains("nowszą"))
        );
    }

    #[test]
    fn req_02_050_thumbnail_roundtrip() {
        let png = vec![0x89, b'P', b'N', b'G', 1, 2, 3];
        let bytes = to_bytes(
            &sample(),
            &Extras {
                thumbnail_png: Some(png.clone()),
                template: false,
            },
        )
        .unwrap();
        assert_eq!(from_bytes(&bytes).unwrap().1.thumbnail_png, Some(png));
    }

    #[test]
    fn req_02_051_notes_roundtrip() {
        let bytes = to_bytes(&sample(), &Extras::default()).unwrap();
        assert_eq!(
            from_bytes(&bytes).unwrap().0.notes,
            "Sklejka 3 mm, ostrość po wymianie stołu"
        );
    }

    #[test]
    fn rejects_non_bcraft_zip() {
        let mut zw = zip::ZipWriter::new(Cursor::new(Vec::new()));
        zw.start_file("x.txt", SimpleFileOptions::default())
            .unwrap();
        zw.write_all(b"x").unwrap();
        let bytes = zw.finish().unwrap().into_inner();
        assert!(from_bytes(&bytes).is_err());
    }

    #[test]
    fn save_and_load_file() {
        let dir = std::env::temp_dir().join(format!("bc-doc-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("a.bcraft");
        save(&sample(), &Extras::default(), &p).unwrap();
        assert_eq!(load(&p).unwrap().0, sample());
        std::fs::remove_dir_all(&dir).ok();
    }
}
