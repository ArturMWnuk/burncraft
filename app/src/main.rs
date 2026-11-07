//! `burncraft` — okno desktopowe BurnCraft (ADR 0007, REQ-01-017, 021, 042).
//!
//! Uruchamia w procesie host GUI (`bc-host`: frontend i kanał sterowania
//! JSON-RPC/WebSocket na 127.0.0.1:<port>) i otwiera okno Tauri 2 z frontendem
//! z tego hosta. Agent łączy się z oknem przez `burncraft-cli mcp --connect <port>`.
//! Pierwsza linia stdout to adres hosta (dla skryptów i testów).

#![cfg_attr(all(not(debug_assertions), windows), windows_subsystem = "windows")]

use std::io::Write;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Mutex;
use std::time::Duration;

use bc_commands::{Registry, Session};
use clap::Parser;
use tauri::{RunEvent, WebviewUrl, WebviewWindowBuilder};

#[derive(Parser)]
#[command(
    name = "burncraft",
    version,
    about = "BurnCraft — okno desktopowe z kanałem sterowania"
)]
struct Args {
    /// Port kanału sterowania na 127.0.0.1 (0 = dowolny wolny).
    #[arg(long, default_value_t = bc_host::DEFAULT_PORT)]
    control: u16,
    /// Otwórz plik .bcraft / .lbrn2 na starcie.
    #[arg(long)]
    open: Option<PathBuf>,
    /// Katalog zbudowanego frontendu (domyślnie obok programu albo web/dist w drzewie roboczym).
    #[arg(long)]
    web_dir: Option<PathBuf>,
    /// Plik tokenu kanału (domyślnie control-<port>.token w katalogu danych).
    #[arg(long)]
    token_file: Option<PathBuf>,
    /// Katalog preferencji (prefs.json i kopie zapasowe).
    #[arg(long)]
    prefs_dir: Option<PathBuf>,
    /// Plik dziennika audytu MCP.
    #[arg(long)]
    audit_log: Option<PathBuf>,
    /// Bez zapisu dziennika audytu do pliku.
    #[arg(long)]
    no_audit_log: bool,
    /// Katalog autozapisu.
    #[arg(long)]
    autosave_dir: Option<PathBuf>,
    /// Interwał autozapisu w minutach (0 = wyłączony).
    #[arg(long, default_value_t = 5)]
    autosave_minutes: u64,
}

/// Katalog danych użytkownika (jak w `burncraft-cli`).
fn data_dir() -> Option<PathBuf> {
    if cfg!(windows) {
        std::env::var_os("LOCALAPPDATA").map(|p| PathBuf::from(p).join("BurnCraft"))
    } else if cfg!(target_os = "macos") {
        std::env::var_os("HOME")
            .map(|h| PathBuf::from(h).join("Library/Application Support/BurnCraft"))
    } else {
        std::env::var_os("XDG_STATE_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/state")))
            .map(|p| p.join("burncraft"))
    }
}

/// Katalog konfiguracji użytkownika (preferencje).
fn config_dir() -> Option<PathBuf> {
    if cfg!(windows) {
        std::env::var_os("APPDATA").map(|p| PathBuf::from(p).join("BurnCraft"))
    } else if cfg!(target_os = "macos") {
        std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Library/Preferences/BurnCraft"))
    } else {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
            .map(|p| p.join("burncraft"))
    }
}

/// Frontend: jawny katalog, BURNCRAFT_WEB_DIR, `web/` obok programu (instalacja)
/// albo `web/dist` w drzewie roboczym (uruchomienie z `app/target/<profil>/`).
fn find_web_dir(explicit: Option<PathBuf>) -> Option<PathBuf> {
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(PathBuf::from));
    explicit
        .into_iter()
        .chain(std::env::var_os("BURNCRAFT_WEB_DIR").map(PathBuf::from))
        .chain(exe_dir.iter().map(|d| d.join("web")))
        .chain(exe_dir.iter().map(|d| d.join("../../../web/dist")))
        .chain(std::env::current_dir().ok().map(|d| d.join("web/dist")))
        .chain(std::env::current_dir().ok().map(|d| d.join("../web/dist")))
        .find(|p| p.join("index.html").is_file())
}

fn load_session(path: &std::path::Path) -> Result<Session, String> {
    let reg = Registry::builtin();
    let mut s = Session::default();
    s.execute(
        &reg,
        "file.open",
        serde_json::json!({ "path": path.to_string_lossy() }),
    )
    .map_err(|e| e.to_string())?;
    Ok(s)
}

fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_env("BURNCRAFT_LOG")
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();
    let a = Args::parse();
    let session = match &a.open {
        Some(p) => match load_session(p) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("burncraft: {e}");
                return ExitCode::from(2);
            }
        },
        None => Session::default(),
    };
    let web_dir = find_web_dir(a.web_dir.clone());
    if web_dir.is_none() {
        tracing::warn!("brak zbudowanego frontendu (npm run build w web/) — strona zastępcza");
    }
    let audit_log = if a.no_audit_log {
        None
    } else {
        a.audit_log
            .clone()
            .or_else(|| data_dir().map(|d| d.join("mcp-audit.jsonl")))
    };
    let cfg = bc_host::HostConfig {
        port: a.control,
        web_dir,
        token_file: a
            .token_file
            .clone()
            .or_else(|| data_dir().map(|d| d.join(format!("control-{}.token", a.control)))),
        mcp: bc_mcp::Config {
            audit_log,
            autosave: (a.autosave_minutes > 0)
                .then(|| {
                    a.autosave_dir
                        .clone()
                        .or_else(|| data_dir().map(|d| d.join("autosave")))
                })
                .flatten()
                .map(|dir| bc_commands::workbench::Autosave {
                    dir,
                    interval: Duration::from_secs(a.autosave_minutes * 60),
                }),
            prefs_dir: a.prefs_dir.clone().or_else(config_dir),
            ..Default::default()
        },
    };
    let host = match bc_host::spawn(session, cfg) {
        Ok(h) => h,
        Err(e) => {
            eprintln!("burncraft: {e}");
            return ExitCode::from(2);
        }
    };
    let url: tauri::Url = match host.url().parse() {
        Ok(u) => u,
        Err(e) => {
            eprintln!("burncraft: adres hosta: {e}");
            return ExitCode::FAILURE;
        }
    };
    println!("{}", host.url());
    let _ = std::io::stdout().flush();
    let host = Mutex::new(Some(host));
    let app = tauri::Builder::default()
        .setup(move |app| {
            WebviewWindowBuilder::new(app, "main", WebviewUrl::External(url.clone()))
                .title("BurnCraft")
                .inner_size(1280.0, 820.0)
                .min_inner_size(800.0, 560.0)
                .build()?;
            Ok(())
        })
        .build(tauri::generate_context!());
    let app = match app {
        Ok(a) => a,
        Err(e) => {
            eprintln!("burncraft: okno: {e}");
            if let Some(h) = host.lock().unwrap().take() {
                h.shutdown();
            }
            return ExitCode::FAILURE;
        }
    };
    app.run(move |_app, ev| {
        if let RunEvent::Exit = ev {
            // zamknięcie okna kończy host i usuwa plik tokenu
            if let Some(h) = host.lock().unwrap().take() {
                h.shutdown();
            }
        }
    });
    ExitCode::SUCCESS
}
