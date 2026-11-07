//! Preferencje aplikacji z kopiami zapasowymi (REQ-02-056).
//!
//! `prefs.json` w katalogu konfiguracji; każdy zapis najpierw przenosi poprzednią
//! wersję do `backups/prefs-<znacznik czasu>.json` (ostatnie [`PrefStore::keep`]).

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use serde::Serialize;
use serde_json::Value;

use crate::CmdError;

#[derive(Debug, Clone)]
pub struct PrefStore {
    pub dir: PathBuf,
    /// Liczba przechowywanych kopii zapasowych.
    pub keep: usize,
}

#[derive(Debug, Clone, Serialize, schemars::JsonSchema)]
pub struct BackupInfo {
    /// Nazwa pliku kopii (do `prefs.restore`).
    pub name: String,
    /// Czas utworzenia kopii (ms od epoki).
    pub created_ms: u64,
    pub bytes: u64,
}

fn io(e: std::io::Error) -> CmdError {
    CmdError::new("failed", format!("preferencje: {e}"))
}

impl PrefStore {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        PrefStore {
            dir: dir.into(),
            keep: 20,
        }
    }

    fn file(&self) -> PathBuf {
        self.dir.join("prefs.json")
    }

    fn backup_dir(&self) -> PathBuf {
        self.dir.join("backups")
    }

    /// Bieżące preferencje (pusty obiekt, gdy brak pliku).
    pub fn load(&self) -> Result<Value, CmdError> {
        match std::fs::read(self.file()) {
            Ok(b) => serde_json::from_slice(&b).map_err(|e| {
                CmdError::new(
                    "failed",
                    format!("prefs.json uszkodzony: {e} — przywróć kopię (prefs.restore)"),
                )
            }),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                Ok(Value::Object(Default::default()))
            }
            Err(e) => Err(io(e)),
        }
    }

    /// Zapisuje preferencje; poprzednia wersja trafia do kopii zapasowych.
    pub fn save(&self, prefs: &Value) -> Result<(), CmdError> {
        if !prefs.is_object() {
            return Err(CmdError::new(
                "invalid_params",
                "preferencje muszą być obiektem JSON",
            ));
        }
        std::fs::create_dir_all(&self.dir).map_err(io)?;
        let cur = self.file();
        if cur.exists() {
            static N: AtomicU64 = AtomicU64::new(0);
            let ms = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0);
            std::fs::create_dir_all(self.backup_dir()).map_err(io)?;
            let name = format!(
                "prefs-{ms:013}-{:06}.json",
                N.fetch_add(1, Ordering::Relaxed) % 1_000_000
            );
            std::fs::copy(&cur, self.backup_dir().join(name)).map_err(io)?;
        }
        let tmp = self.dir.join("prefs.json.tmp");
        std::fs::write(&tmp, serde_json::to_vec_pretty(prefs).unwrap_or_default()).map_err(io)?;
        std::fs::rename(&tmp, &cur).map_err(io)?;
        let all = self.backups()?;
        for old in all.iter().skip(self.keep) {
            let _ = std::fs::remove_file(self.backup_dir().join(&old.name));
        }
        Ok(())
    }

    /// Kopie zapasowe, od najnowszej.
    pub fn backups(&self) -> Result<Vec<BackupInfo>, CmdError> {
        let rd = match std::fs::read_dir(self.backup_dir()) {
            Ok(rd) => rd,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(io(e)),
        };
        let mut v: Vec<BackupInfo> = rd
            .filter_map(|e| e.ok())
            .filter_map(|e| {
                let name = e.file_name().to_string_lossy().into_owned();
                let ts = name
                    .strip_prefix("prefs-")?
                    .split('-')
                    .next()?
                    .parse::<u64>()
                    .ok()?;
                Some(BackupInfo {
                    created_ms: ts,
                    bytes: e.metadata().map(|m| m.len()).unwrap_or(0),
                    name,
                })
            })
            .collect();
        v.sort_by(|a, b| b.name.cmp(&a.name));
        Ok(v)
    }

    /// Treść kopii o podanej nazwie (tylko z katalogu kopii).
    pub fn read_backup(&self, name: &str) -> Result<(BackupInfo, Value), CmdError> {
        let safe = name.starts_with("prefs-")
            && name.ends_with(".json")
            && !name.contains(['/', '\\'])
            && !name.contains("..");
        let info = safe
            .then(|| self.backups())
            .transpose()?
            .and_then(|all| all.into_iter().find(|b| b.name == name))
            .ok_or_else(|| CmdError::new("invalid_params", format!("nie ma kopii {name}")))?;
        let v: Value =
            serde_json::from_slice(&std::fs::read(self.backup_dir().join(name)).map_err(io)?)
                .map_err(|e| CmdError::new("failed", format!("kopia {name} uszkodzona: {e}")))?;
        Ok((info, v))
    }

    /// Przywraca kopię (bieżąca wersja też trafia do kopii, więc przywrócenie można cofnąć).
    pub fn restore(&self, name: &str) -> Result<Value, CmdError> {
        let (_, v) = self.read_backup(name)?;
        self.save(&v)?;
        Ok(v)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, schemars::JsonSchema)]
pub struct PrefChange {
    /// Klucz z kropkami (np. ui.theme).
    pub key: String,
    /// Wartość w kopii (null = brak klucza).
    pub backup: Value,
    /// Wartość bieżąca (null = brak klucza).
    pub current: Value,
}

/// Różnice liść po liściu (obiekty rozwijane, tablice porównywane w całości),
/// posortowane po kluczu.
pub fn diff(backup: &Value, current: &Value) -> Vec<PrefChange> {
    fn walk(prefix: &str, a: Option<&Value>, b: Option<&Value>, out: &mut Vec<PrefChange>) {
        match (a, b) {
            (Some(Value::Object(x)), Some(Value::Object(y))) => {
                let keys: std::collections::BTreeSet<&String> = x.keys().chain(y.keys()).collect();
                for k in keys {
                    let key = if prefix.is_empty() {
                        k.clone()
                    } else {
                        format!("{prefix}.{k}")
                    };
                    walk(&key, x.get(k), y.get(k), out);
                }
            }
            (Some(Value::Object(x)), other) | (other, Some(Value::Object(x)))
                if other.is_none_or(Value::is_null) && !x.is_empty() =>
            {
                // obiekt po jednej stronie, brak po drugiej — każdy liść osobno
                let empty = Value::Object(Default::default());
                if a.is_some_and(Value::is_object) {
                    walk(prefix, a, Some(&empty), out);
                } else {
                    walk(prefix, Some(&empty), b, out);
                }
            }
            _ => {
                let (x, y) = (
                    a.cloned().unwrap_or(Value::Null),
                    b.cloned().unwrap_or(Value::Null),
                );
                if x != y {
                    out.push(PrefChange {
                        key: prefix.to_string(),
                        backup: x,
                        current: y,
                    });
                }
            }
        }
    }
    let mut out = Vec::new();
    walk("", Some(backup), Some(current), &mut out);
    out
}
