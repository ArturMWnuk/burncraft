//! Dziennik audytu wywołań MCP (REQ-15-105): kto, kiedy, co, parametry, wynik.
//! Format: JSON Lines, dopisywany do pliku.

use std::io::Write;
use std::path::PathBuf;

use serde_json::{json, Value};

use crate::tools::Outcome;

/// Dane obrazów nie trafiają do dziennika — tylko ich rozmiar.
const MAX_ARG_LEN: usize = 16 * 1024;

pub struct Audit {
    path: Option<PathBuf>,
    /// Ostatnie wpisy w pamięci (diagnostyka, testy).
    pub recent: Vec<Value>,
}

impl Audit {
    pub fn new(path: Option<PathBuf>) -> Self {
        if let Some(p) = &path {
            if let Some(dir) = p.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
        }
        Audit {
            path,
            recent: Vec::new(),
        }
    }

    pub fn record(&mut self, client: &str, tool: &str, args: &Value, outcome: &Outcome) {
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        let mut args = args.clone();
        let text = args.to_string();
        if text.len() > MAX_ARG_LEN {
            args = json!({ "truncated": true, "bytes": text.len() });
        }
        let entry = json!({
            "ts_ms": ts,
            "client": client,
            "tool": tool,
            "args": args,
            "outcome": outcome.status,
            "error": outcome.error,
        });
        if let Some(p) = &self.path {
            let line = format!("{entry}\n");
            let res = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(p)
                .and_then(|mut f| f.write_all(line.as_bytes()));
            if let Err(e) = res {
                tracing::error!(path = %p.display(), "nie można zapisać dziennika audytu: {e}");
            }
        }
        self.recent.push(entry);
        if self.recent.len() > 1000 {
            self.recent.remove(0);
        }
    }
}
