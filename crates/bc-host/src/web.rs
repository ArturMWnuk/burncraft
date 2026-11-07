//! Statyczny frontend z `web_dir`; `index.html` dostaje token sesji.

use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

use axum::extract::State;
use axum::http::{header, StatusCode, Uri};
use axum::response::{IntoResponse, Response};

use crate::{Shared, TOKEN_PLACEHOLDER};

/// Strona zastępcza, gdy frontend nie jest zbudowany (`npm run build` w `web/`).
const FALLBACK: &str = "<!doctype html><meta charset=utf-8><title>BurnCraft</title>\
<p>Host BurnCraft działa. Frontend nie jest zbudowany: uruchom <code>npm run build</code> w katalogu <code>web/</code>.</p>";

fn content_type(p: &Path) -> &'static str {
    match p.extension().and_then(|e| e.to_str()).unwrap_or("") {
        "html" => "text/html; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "json" | "map" => "application/json",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "ico" => "image/x-icon",
        "wasm" => "application/wasm",
        "woff2" => "font/woff2",
        "ftl" | "txt" => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}

/// Ścieżka względna wyłącznie z normalnych składników (bez `..`, dysków, korzenia).
fn safe_rel(path: &str) -> Option<PathBuf> {
    let mut out = PathBuf::new();
    for part in path.split('/').filter(|p| !p.is_empty()) {
        if part.contains('\\') || part.contains(':') {
            return None;
        }
        let mut c = Path::new(part).components();
        match (c.next(), c.next()) {
            (Some(Component::Normal(n)), None) => out.push(n),
            _ => return None,
        }
    }
    Some(out)
}

pub async fn serve(State(sh): State<Arc<Shared>>, uri: Uri) -> Response {
    let Some(rel) = safe_rel(uri.path()) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let is_index = rel.as_os_str().is_empty() || rel == Path::new("index.html");
    let Some(dir) = &sh.web_dir else {
        if is_index {
            return (
                [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
                FALLBACK,
            )
                .into_response();
        }
        return StatusCode::NOT_FOUND.into_response();
    };
    let file = if is_index {
        dir.join("index.html")
    } else {
        dir.join(&rel)
    };
    match tokio::fs::read(&file).await {
        Ok(bytes) if is_index => {
            let html = String::from_utf8_lossy(&bytes).replace(TOKEN_PLACEHOLDER, &sh.token);
            (
                [
                    (header::CONTENT_TYPE, "text/html; charset=utf-8"),
                    (header::CACHE_CONTROL, "no-store"),
                ],
                html,
            )
                .into_response()
        }
        Ok(bytes) => ([(header::CONTENT_TYPE, content_type(&file))], bytes).into_response(),
        Err(_) => StatusCode::NOT_FOUND.into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::safe_rel;

    #[test]
    fn rejects_escapes() {
        assert!(safe_rel("/../Cargo.toml").is_none());
        assert!(safe_rel("/a/..").is_none());
        assert!(safe_rel("/C:/x").is_none());
        assert!(safe_rel("/a\\..\\b").is_none());
        assert_eq!(
            safe_rel("/assets/app.js").unwrap(),
            std::path::Path::new("assets/app.js")
        );
    }
}
