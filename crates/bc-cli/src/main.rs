//! burncraft-cli — wiersz poleceń BurnCraft.
//!
//! `burncraft-cli mcp` wystawia serwer MCP po stdio. Z `--connect <port>`
//! łączy się z oknem `burncraft --control <port>` (albo `burncraft-cli serve`)
//! i działa na jego żywym dokumencie (REQ-01-043); gdy okna nie ma, uruchamia
//! sesję bezgłową (REQ-01-044). Z `--http [PORT]` serwer MCP działa przez Streamable
//! HTTP na 127.0.0.1 (REQ-15-003, ADR 0012). Logi trafiają wyłącznie na stderr — stdout
//! należy do protokołu MCP.

mod connect;

use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Duration;

use bc_commands::{inspect, Registry, Session};
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "burncraft-cli",
    version,
    about = "BurnCraft — wiersz poleceń i serwer MCP"
)]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

/// Opcje sesji wspólne dla `mcp` i `serve`.
#[derive(clap::Args)]
struct SessionOpts {
    /// Otwórz plik .bcraft na starcie.
    #[arg(long)]
    open: Option<PathBuf>,
    /// Plik dziennika audytu (JSON Lines). Domyślnie w katalogu danych użytkownika.
    #[arg(long)]
    audit_log: Option<PathBuf>,
    /// Wyłącz zapis dziennika audytu do pliku.
    #[arg(long)]
    no_audit_log: bool,
    /// Katalog autozapisu (domyślnie w katalogu danych użytkownika).
    #[arg(long)]
    autosave_dir: Option<PathBuf>,
    /// Interwał autozapisu w minutach (0 = wyłączony).
    #[arg(long, default_value_t = 5)]
    autosave_minutes: u64,
    /// Katalog preferencji (prefs.json i kopie zapasowe).
    #[arg(long)]
    prefs_dir: Option<PathBuf>,
    /// Zgoda użytkownika na polecenia ruchu (poziom motion: jog, bazowanie, ramka) bez
    /// pytania o każde z nich. Bez tej opcji motion wymaga potwierdzenia (elicitation).
    #[arg(long)]
    allow_motion: bool,
    /// Limit ciszy klienta MCP w sekundach, po którym watchdog anuluje jog.
    #[arg(long, default_value_t = 5.0)]
    jog_watchdog: f64,
}

impl SessionOpts {
    fn session(&self) -> Result<Session, String> {
        match &self.open {
            Some(p) => load_session(p),
            None => Ok(Session::default()),
        }
    }

    fn mcp_config(self) -> bc_mcp::Config {
        let audit = if self.no_audit_log {
            None
        } else {
            self.audit_log.or_else(default_audit_log)
        };
        if let Some(p) = &audit {
            tracing::info!(path = %p.display(), "dziennik audytu MCP");
        }
        let autosave_minutes = self.autosave_minutes;
        let mut policy = bc_mcp::policy::Policy::default();
        if self.allow_motion {
            tracing::warn!("MCP: polecenia ruchu (motion) dopuszczone bez potwierdzeń");
            policy.auto_allow = bc_commands::SafetyTier::Motion;
        }
        bc_mcp::Config {
            policy,
            watchdog: Some(Duration::from_secs_f64(self.jog_watchdog.clamp(0.2, 600.0))),
            audit_log: audit,
            autosave: (autosave_minutes > 0)
                .then(|| {
                    self.autosave_dir
                        .or_else(|| data_dir().map(|d| d.join("autosave")))
                })
                .flatten()
                .map(|dir| bc_commands::workbench::Autosave {
                    dir,
                    interval: Duration::from_secs(autosave_minutes * 60),
                }),
            prefs_dir: self.prefs_dir.or_else(config_dir),
            ..Default::default()
        }
    }
}

#[derive(Subcommand)]
enum Cmd {
    /// Serwer MCP po stdio albo przez Streamable HTTP (--http).
    Mcp {
        /// Port kanału sterowania okna BurnCraft (domyślnie 7984 przy samym --connect).
        #[arg(long, num_args = 0..=1, default_missing_value = "7984", conflicts_with = "http")]
        connect: Option<u16>,
        /// MCP przez Streamable HTTP na 127.0.0.1:PORT (domyślnie 7985, 0 = dowolny wolny);
        /// pierwszy wiersz stdout to adres punktu końcowego.
        #[arg(long, num_args = 0..=1, default_missing_value = "7985")]
        http: Option<u16>,
        /// Plik z tokenem kanału sterowania (domyślnie control-<port>.token w katalogu danych).
        #[arg(long)]
        token_file: Option<PathBuf>,
        #[command(flatten)]
        opts: SessionOpts,
    },
    /// Host GUI bez okna: frontend w przeglądarce i kanał sterowania na 127.0.0.1.
    Serve {
        /// Port (0 = dowolny wolny).
        #[arg(long, default_value_t = bc_host::DEFAULT_PORT)]
        port: u16,
        /// Katalog zbudowanego frontendu (domyślnie BURNCRAFT_WEB_DIR, web/ obok programu albo web/dist).
        #[arg(long)]
        web_dir: Option<PathBuf>,
        /// Plik, do którego zapisać token sesji (domyślnie control-<port>.token w katalogu danych).
        #[arg(long)]
        token_file: Option<PathBuf>,
        #[command(flatten)]
        opts: SessionOpts,
    },
    /// Wypisz podsumowanie pliku .bcraft.
    Open {
        path: PathBuf,
        /// Pełny raport (wszystkie warstwy, geometria).
        #[arg(long)]
        full: bool,
    },
    /// Wypisz rejestr poleceń.
    Commands {
        /// Filtr (id, tytuł, opis).
        #[arg(long)]
        filter: Option<String>,
        /// Format JSON (pełne specyfikacje ze schematami).
        #[arg(long)]
        json: bool,
    },
    /// Konwersja: wejście .bcraft/.lbrn2/.svg/.dxf/.plt -> wyjście .bcraft/.svg/.dxf/.plt.
    Convert {
        input: PathBuf,
        output: PathBuf,
        /// Format wyjścia: svg, dxf, dxf-r12, hpgl (domyślnie z rozszerzenia).
        #[arg(long)]
        format: Option<String>,
        /// Przypisanie warstw przy imporcie: color albo name.
        #[arg(long, default_value = "color")]
        layer_by: String,
    },
    /// Eksport dokumentu .bcraft do SVG/DXF/HPGL albo G-code (.gcode, .nc, .gc).
    Export {
        input: PathBuf,
        output: PathBuf,
        /// Format: svg, dxf, dxf-r12, hpgl, gcode (domyślnie z rozszerzenia).
        #[arg(long)]
        format: Option<String>,
        /// Sterownik G-code: grbl (domyślnie) albo gcode (ogólny CNC).
        #[arg(long)]
        driver: Option<String>,
    },
    /// Plan zadania z pliku .bcraft: czas, długości, przebiegi, operacje, ostrzeżenia, koszt.
    Plan {
        path: PathBuf,
        /// Wynik w JSON (jak narzędzie MCP plan_job).
        #[arg(long)]
        json: bool,
        /// Limit mocy (%) — ostrzeżenie power_above_limit.
        #[arg(long)]
        power_limit: Option<f64>,
        /// Stawka za godzinę pracy maszyny (koszt w raporcie).
        #[arg(long)]
        rate: Option<f64>,
    },
    /// Szacowany czas zadania z pliku .bcraft (model przyspieszeń X/Y).
    Estimate {
        path: PathBuf,
        /// Wynik w JSON.
        #[arg(long)]
        json: bool,
    },
    /// Wersje modułów.
    /// Wysyła zadanie (.bcraft albo G-code .nc/.gc/.gcode) do urządzenia i czeka na koniec.
    Send {
        path: PathBuf,
        /// Nazwa urządzenia (profil, np. "Symulator").
        #[arg(long)]
        device: String,
        /// Katalog preferencji z devices.json (domyślnie katalog konfiguracji).
        #[arg(long)]
        prefs_dir: Option<PathBuf>,
        /// Powtórzenia zadania.
        #[arg(long, default_value_t = 1)]
        repeat: u32,
        /// Opóźnienie między powtórzeniami, s.
        #[arg(long, default_value_t = 0.0)]
        delay: f64,
        /// Wynik w JSON.
        #[arg(long)]
        json: bool,
    },
    /// Stan urządzenia (pozycja, stan, bufory, alarm).
    Status {
        /// Nazwa urządzenia (profil).
        #[arg(long)]
        device: String,
        #[arg(long)]
        prefs_dir: Option<PathBuf>,
        /// Wynik w JSON.
        #[arg(long)]
        json: bool,
    },
    /// Emulator GRBL jako serwer TCP (urządzenie fikcyjne dla innych programów i testów).
    Emulator {
        /// Adres nasłuchu.
        #[arg(long, default_value = "127.0.0.1:2323")]
        tcp: String,
        /// Odmiana GRBL (grbl11, grblhal, fluidnc, …).
        #[arg(long, default_value = "grbl11")]
        variant: String,
        /// Mnożnik czasu (brak = czas rzeczywisty).
        #[arg(long)]
        speed: Option<f64>,
    },
    /// Reguły udev dla laserów USB (Linux): burncraft-cli udev | sudo tee /etc/udev/rules.d/99-burncraft.rules
    Udev,
    Version,
}

/// `plan` i `estimate` przez polecenia rejestru (te same co w GUI i MCP).
fn plan_cmd(
    path: &std::path::Path,
    id: &str,
    params: serde_json::Value,
) -> Result<serde_json::Value, String> {
    let mut s = load_session(path)?;
    let reg = Registry::builtin();
    s.execute(&reg, id, params).map_err(|e| e.to_string())
}

fn mm(v: &serde_json::Value) -> String {
    format!("{:.1} mm", v.as_f64().unwrap_or(0.0))
}

fn secs(v: &serde_json::Value) -> String {
    format!("{:.1} s", v.as_f64().unwrap_or(0.0))
}

fn print_plan(path: &std::path::Path, r: &serde_json::Value) {
    let t = &r["time"];
    println!("Plan: {}", path.display());
    println!(
        "Czas: {} (cięcie {}, przejazdy {}, opóźnienia {}, postoje {})",
        r["formatted_time"].as_str().unwrap_or("?"),
        secs(&t["cut_s"]),
        secs(&t["travel_s"]),
        secs(&t["delay_s"]),
        secs(&t["dwell_s"])
    );
    println!(
        "Cięcie: {}, przejazdy: {}, przebiegi: {}",
        mm(&r["cut_length"]),
        mm(&r["travel_length"]),
        r["passes"]
    );
    for op in r["ops"].as_array().into_iter().flatten() {
        println!(
            "  {} (warstwa {}, przejście {}): {}, {}",
            op["label"].as_str().unwrap_or(""),
            op["layer"],
            op["pass"].as_u64().unwrap_or(0) + 1,
            mm(&op["cut_length"]),
            secs(&op["time_s"])
        );
    }
    let ws = r["warnings"].as_array().cloned().unwrap_or_default();
    if ws.is_empty() {
        println!("Ostrzeżenia: brak");
    } else {
        println!("Ostrzeżenia:");
        for w in ws {
            println!(
                "  - {}: {}",
                w["code"].as_str().unwrap_or(""),
                w["message"].as_str().unwrap_or("")
            );
        }
    }
    if r["cost"]["rate_per_hour"].as_f64().unwrap_or(0.0) > 0.0 {
        println!(
            "Koszt czasu maszyny: {:.2}",
            r["cost"]["machine_cost"].as_f64().unwrap_or(0.0)
        );
    }
    println!("Hash: {}", r["hash"].as_str().unwrap_or(""));
}

fn is_ext(p: &std::path::Path, exts: &[&str]) -> bool {
    p.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| exts.iter().any(|x| e.eq_ignore_ascii_case(x)))
}

/// Konwersja przez polecenia rejestru (te same co w GUI i MCP).
fn convert(
    input: &std::path::Path,
    output: &std::path::Path,
    format: Option<&str>,
    layer_by: &str,
    bcraft_only: bool,
) -> Result<String, String> {
    let reg = Registry::builtin();
    let mut s = Session::default();
    let path = |p: &std::path::Path| serde_json::Value::from(p.to_string_lossy().into_owned());
    if is_ext(input, &["bcraft", "bcraft-template", "lbrn2", "lbrn"]) {
        s.execute(
            &reg,
            "file.open",
            serde_json::json!({ "path": path(input) }),
        )
        .map_err(|e| e.to_string())?;
    } else if bcraft_only {
        return Err(format!(
            "{}: export przyjmuje plik .bcraft (do innych formatów użyj convert)",
            input.display()
        ));
    } else {
        let r = s
            .execute(
                &reg,
                "file.import",
                serde_json::json!({ "path": path(input), "layer_by": layer_by }),
            )
            .map_err(|e| e.to_string())?;
        for w in r["warnings"].as_array().into_iter().flatten() {
            eprintln!("ostrzeżenie: {}", w.as_str().unwrap_or_default());
        }
    }
    if format.is_none() && is_ext(output, &["bcraft"]) {
        s.execute(
            &reg,
            "file.save",
            serde_json::json!({ "path": path(output) }),
        )
        .map_err(|e| e.to_string())?;
        return Ok(format!(
            "{} -> {} (bcraft)",
            input.display(),
            output.display()
        ));
    }
    let mut params = serde_json::json!({ "path": path(output) });
    if let Some(f) = format {
        params["format"] = f.into();
    }
    let r = s
        .execute(&reg, "file.export", params)
        .map_err(|e| e.to_string())?;
    Ok(format!(
        "{} -> {} ({}, {} B)",
        input.display(),
        output.display(),
        r["format"].as_str().unwrap_or("?"),
        r["bytes"]
    ))
}

/// Katalog danych użytkownika (dziennik, autozapis).
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

fn default_audit_log() -> Option<PathBuf> {
    let base = if cfg!(windows) {
        std::env::var_os("LOCALAPPDATA").map(|p| PathBuf::from(p).join("BurnCraft"))
    } else if cfg!(target_os = "macos") {
        std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Library/Logs/BurnCraft"))
    } else {
        std::env::var_os("XDG_STATE_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/state")))
            .map(|p| p.join("burncraft"))
    };
    base.map(|b| b.join("mcp-audit.jsonl"))
}

/// Katalog frontendu: jawny, z BURNCRAFT_WEB_DIR, `web/` obok programu albo `web/dist` w drzewie roboczym.
fn find_web_dir(explicit: Option<PathBuf>) -> Option<PathBuf> {
    let has_index = |p: &PathBuf| p.join("index.html").is_file();
    explicit
        .or_else(|| std::env::var_os("BURNCRAFT_WEB_DIR").map(PathBuf::from))
        .into_iter()
        .chain(
            std::env::current_exe()
                .ok()
                .and_then(|e| e.parent().map(|d| d.join("web"))),
        )
        .chain(
            std::env::current_dir()
                .ok()
                .map(|d| d.join("web").join("dist")),
        )
        .find(has_index)
}

/// Sesja z urządzeniami z katalogu preferencji, połączona z `device` (CLI send/status).
fn device_session(
    path: Option<&std::path::Path>,
    device: &str,
    prefs_dir: Option<PathBuf>,
) -> Result<(Registry, Session), String> {
    let reg = Registry::builtin();
    let mut s = match path {
        Some(p) if !bc_commands::gcode::is_gcode_path(p) => load_session(p)?,
        _ => Session::default(),
    };
    if let Some(dir) = prefs_dir.or_else(config_dir) {
        s.machines
            .lock()
            .map_err(|e| e.to_string())?
            .set_store(dir)?;
    }
    if let Some(p) = path.filter(|p| bc_commands::gcode::is_gcode_path(p)) {
        s.execute(
            &reg,
            "gcode.import",
            serde_json::json!({"path": p.to_string_lossy(), "to_document": false}),
        )
        .map_err(|e| e.to_string())?;
    }
    s.execute(
        &reg,
        "machine.connect",
        serde_json::json!({ "name": device }),
    )
    .map_err(|e| e.to_string())?;
    Ok((reg, s))
}

fn send_cmd(
    path: &std::path::Path,
    device: &str,
    prefs_dir: Option<PathBuf>,
    repeat: u32,
    delay: f64,
) -> Result<serde_json::Value, String> {
    let (reg, mut s) = device_session(Some(path), device, prefs_dir)?;
    let source = if bc_commands::gcode::is_gcode_path(path) {
        "gcode"
    } else {
        "document"
    };
    s.execute(
        &reg,
        "job.start",
        serde_json::json!({"source": source, "repeat": repeat, "delay_s": delay}),
    )
    .map_err(|e| e.to_string())?;
    let mut last = std::time::Instant::now();
    loop {
        let j = s
            .execute(&reg, "job.status", serde_json::json!({}))
            .map_err(|e| e.to_string())?;
        let st = j["status"].as_str().unwrap_or("");
        if st != "running" && st != "paused" {
            let _ = s.execute(&reg, "machine.disconnect", serde_json::json!({}));
            return Ok(j);
        }
        if last.elapsed() >= std::time::Duration::from_secs(1) {
            last = std::time::Instant::now();
            eprintln!(
                "{}: {}/{} wierszy, runda {}/{}",
                device, j["acked"], j["total"], j["round"], j["repeat"]
            );
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
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
    let cli = Cli::parse();
    match cli.cmd {
        Cmd::Mcp {
            connect,
            http,
            token_file,
            opts,
        } => {
            if let Some(port) = connect {
                let tf = token_file.or_else(|| connect::default_token_file(data_dir(), port));
                match connect::connect(port, tf.as_deref()) {
                    Ok(Some(ws)) => {
                        tracing::info!(port, "MCP na żywym oknie BurnCraft");
                        return match connect::run(ws) {
                            Ok(()) => ExitCode::SUCCESS,
                            Err(e) => {
                                eprintln!("burncraft-cli: {e}");
                                ExitCode::FAILURE
                            }
                        };
                    }
                    Ok(None) => tracing::info!(port, "brak okna BurnCraft — sesja bezgłowa"),
                    Err(e) => {
                        eprintln!("burncraft-cli: {e}");
                        return ExitCode::from(2);
                    }
                }
            }
            let session = match opts.session() {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("burncraft-cli: {e}");
                    return ExitCode::from(2);
                }
            };
            let mut server = bc_mcp::Server::new(session, opts.mcp_config());
            if let Some(port) = http {
                return match bc_mcp::http::serve(server, port) {
                    Ok(h) => {
                        // pierwsza linia stdout: adres punktu końcowego (dla skryptów i testów)
                        println!("{}", h.url());
                        use std::io::Write;
                        let _ = std::io::stdout().flush();
                        eprintln!("BurnCraft MCP (Streamable HTTP): {}", h.url());
                        h.wait();
                        ExitCode::SUCCESS
                    }
                    Err(e) => {
                        eprintln!("burncraft-cli: MCP HTTP: {e}");
                        ExitCode::FAILURE
                    }
                };
            }
            tracing::info!("serwer MCP BurnCraft {} na stdio", bc_mcp::SERVER_VERSION);
            match server.run_stdio() {
                Ok(()) => ExitCode::SUCCESS,
                Err(e) => {
                    eprintln!("burncraft-cli: stdio: {e}");
                    ExitCode::FAILURE
                }
            }
        }
        Cmd::Serve {
            port,
            web_dir,
            token_file,
            opts,
        } => {
            let session = match opts.session() {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("burncraft-cli: {e}");
                    return ExitCode::from(2);
                }
            };
            let web_dir = find_web_dir(web_dir);
            if web_dir.is_none() {
                tracing::warn!(
                    "brak zbudowanego frontendu (npm run build w web/) — strona zastępcza"
                );
            }
            let cfg = bc_host::HostConfig {
                port,
                web_dir,
                token_file: token_file.or_else(|| connect::default_token_file(data_dir(), port)),
                mcp: opts.mcp_config(),
            };
            match bc_host::spawn(session, cfg) {
                Ok(h) => {
                    // pierwsza linia stdout: adres (dla skryptów i testów)
                    println!("{}", h.url());
                    use std::io::Write;
                    let _ = std::io::stdout().flush();
                    eprintln!("BurnCraft: otwórz {} w przeglądarce", h.url());
                    h.wait();
                    ExitCode::SUCCESS
                }
                Err(e) => {
                    eprintln!("burncraft-cli: {e}");
                    ExitCode::FAILURE
                }
            }
        }
        Cmd::Open { path, full } => match load_session(&path) {
            Ok(s) => {
                let detail = if full {
                    inspect::Detail::Full
                } else {
                    inspect::Detail::Summary
                };
                match inspect::report(&s, detail) {
                    Ok(r) => {
                        println!("{}", serde_json::to_string_pretty(&r).unwrap_or_default());
                        ExitCode::SUCCESS
                    }
                    Err(e) => {
                        eprintln!("burncraft-cli: {e}");
                        ExitCode::FAILURE
                    }
                }
            }
            Err(e) => {
                eprintln!("burncraft-cli: {e}");
                ExitCode::from(2)
            }
        },
        Cmd::Commands { filter, json } => {
            let reg = Registry::builtin();
            let list = reg.list(filter.as_deref());
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&list).unwrap_or_default()
                );
            } else {
                for c in list {
                    println!(
                        "{:<24} {:<7} {}{}",
                        c.id,
                        c.safety_tier.as_str(),
                        c.title,
                        c.shortcut.map(|s| format!("  [{s}]")).unwrap_or_default()
                    );
                }
            }
            ExitCode::SUCCESS
        }
        Cmd::Convert {
            input,
            output,
            format,
            layer_by,
        } => match convert(&input, &output, format.as_deref(), &layer_by, false) {
            Ok(msg) => {
                eprintln!("{msg}");
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("burncraft-cli: {e}");
                ExitCode::from(2)
            }
        },
        Cmd::Export {
            input,
            output,
            format,
            driver,
        } if driver.is_some()
            || format.as_deref() == Some("gcode")
            || (format.is_none() && bc_commands::gcode::is_gcode_path(&output)) =>
        {
            // G-code przez polecenie rejestru gcode.export (to samo co GUI i MCP)
            let driver = driver.unwrap_or_else(|| "grbl".into());
            let res = load_session(&input).and_then(|mut s| {
                s.execute(
                    &Registry::builtin(),
                    "gcode.export",
                    serde_json::json!({"path": output.to_string_lossy(), "driver": driver}),
                )
                .map_err(|e| e.to_string())
            });
            match res {
                Ok(r) => {
                    eprintln!(
                        "{} → {} (G-code {}, {} wierszy, czas szacowany {:.1} s)",
                        input.display(),
                        output.display(),
                        r["driver"].as_str().unwrap_or("?"),
                        r["lines"],
                        r["time_s"].as_f64().unwrap_or(0.0)
                    );
                    for w in r["warnings"].as_array().into_iter().flatten() {
                        eprintln!("  ostrzeżenie: {}", w["message"].as_str().unwrap_or(""));
                    }
                    ExitCode::SUCCESS
                }
                Err(e) => {
                    eprintln!("burncraft-cli: {e}");
                    ExitCode::from(2)
                }
            }
        }
        Cmd::Export {
            input,
            output,
            format,
            ..
        } => match convert(&input, &output, format.as_deref(), "color", true) {
            Ok(msg) => {
                eprintln!("{msg}");
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("burncraft-cli: {e}");
                ExitCode::from(2)
            }
        },
        Cmd::Plan {
            path,
            json,
            power_limit,
            rate,
        } => {
            let mut p = serde_json::json!({});
            if let Some(v) = power_limit {
                p["power_limit"] = v.into();
            }
            if let Some(v) = rate {
                p["rate_per_hour"] = v.into();
            }
            match plan_cmd(&path, "plan.job", p) {
                Ok(r) => {
                    if json {
                        println!("{}", serde_json::to_string_pretty(&r).unwrap_or_default());
                    } else {
                        print_plan(&path, &r);
                    }
                    ExitCode::SUCCESS
                }
                Err(e) => {
                    eprintln!("burncraft-cli: {e}");
                    ExitCode::from(2)
                }
            }
        }
        Cmd::Estimate { path, json } => {
            match plan_cmd(&path, "plan.estimate", serde_json::json!({})) {
                Ok(r) => {
                    if json {
                        println!("{}", serde_json::to_string_pretty(&r).unwrap_or_default());
                    } else {
                        println!(
                        "Szacowany czas: {} (cięcie {}, przejazdy {}, opóźnienia {}, postoje {})",
                        r["formatted"].as_str().unwrap_or("?"),
                        secs(&r["cut_s"]),
                        secs(&r["travel_s"]),
                        secs(&r["delay_s"]),
                        secs(&r["dwell_s"])
                    );
                    }
                    ExitCode::SUCCESS
                }
                Err(e) => {
                    eprintln!("burncraft-cli: {e}");
                    ExitCode::from(2)
                }
            }
        }
        Cmd::Send {
            path,
            device,
            prefs_dir,
            repeat,
            delay,
            json,
        } => match send_cmd(&path, &device, prefs_dir, repeat, delay) {
            Ok(j) => {
                if json {
                    println!("{}", serde_json::to_string_pretty(&j).unwrap_or_default());
                } else {
                    println!(
                        "{}: zadanie {} — {} ({} wierszy)",
                        device,
                        j["name"].as_str().unwrap_or(""),
                        j["status"].as_str().unwrap_or("?"),
                        j["acked"]
                    );
                    for w in j["warnings"].as_array().into_iter().flatten() {
                        println!("  ostrzeżenie: {}", w.as_str().unwrap_or(""));
                    }
                    for e in j["errors"].as_array().into_iter().flatten() {
                        println!(
                            "  błąd: wiersz {}: error:{} {}",
                            e[0],
                            e[1],
                            e[2].as_str().unwrap_or("")
                        );
                    }
                }
                if j["status"] == "done" {
                    ExitCode::SUCCESS
                } else {
                    ExitCode::from(3)
                }
            }
            Err(e) => {
                eprintln!("burncraft-cli: {e}");
                ExitCode::from(2)
            }
        },
        Cmd::Status {
            device,
            prefs_dir,
            json,
        } => {
            let r = device_session(None, &device, prefs_dir).and_then(|(reg, mut s)| {
                let v = s
                    .execute(&reg, "machine.status", serde_json::json!({}))
                    .map_err(|e| e.to_string());
                let _ = s.execute(&reg, "machine.disconnect", serde_json::json!({}));
                v
            });
            match r {
                Ok(v) => {
                    if json {
                        println!("{}", serde_json::to_string_pretty(&v).unwrap_or_default());
                    } else {
                        let st = &v["state"];
                        println!(
                            "{}: {} ({}), MPos {} WPos {}",
                            device,
                            st["state"].as_str().unwrap_or("?"),
                            st["version"].as_str().unwrap_or(""),
                            st["mpos"],
                            st["wpos"]
                        );
                        if let Some(a) = st["alarm"].as_array() {
                            println!("  alarm {}: {}", a[0], a[1].as_str().unwrap_or(""));
                        }
                    }
                    ExitCode::SUCCESS
                }
                Err(e) => {
                    eprintln!("burncraft-cli: {e}");
                    ExitCode::from(2)
                }
            }
        }
        Cmd::Emulator {
            tcp,
            variant,
            speed,
        } => {
            if bc_drivers::grbl::codes::variant(&variant).is_none() {
                eprintln!("burncraft-cli: nieznana odmiana {variant}");
                return ExitCode::from(2);
            }
            let listener = match std::net::TcpListener::bind(&tcp) {
                Ok(l) => l,
                Err(e) => {
                    eprintln!("burncraft-cli: {tcp}: {e}");
                    return ExitCode::from(2);
                }
            };
            let addr = listener.local_addr().map(|a| a.to_string()).unwrap_or(tcp);
            let mut cfg = bc_drivers::emu::EmuConfig::variant(&variant);
            cfg.speed = speed.unwrap_or(1.0);
            let emu = std::sync::Arc::new(std::sync::Mutex::new(
                bc_drivers::emu::GrblEmulator::new(cfg),
            ));
            println!("BurnCraft: emulator GRBL ({variant}) nasłuchuje na {addr}");
            use std::io::Write;
            let _ = std::io::stdout().flush();
            let _ = bc_drivers::transport::serve_tcp(listener, emu).join();
            ExitCode::SUCCESS
        }
        Cmd::Udev => {
            print!("{}", bc_drivers::usb::udev_rules());
            ExitCode::SUCCESS
        }
        Cmd::Version => {
            println!("burncraft-cli {}", env!("CARGO_PKG_VERSION"));
            for m in [
                bc_text::MODULE,
                bc_image::MODULE,
                bc_io::MODULE,
                bc_camera::MODULE,
                bc_materials::MODULE,
                bc_script::MODULE,
            ] {
                println!("  {m} (szkielet)");
            }
            println!("  {}", bc_plan::MODULE);
            println!(
                "  {} (sterowniki: {})",
                bc_drivers::MODULE,
                bc_drivers::DRIVERS.join(", ")
            );
            println!("  schemat dokumentu: {}", bc_doc::SCHEMA_VERSION);
            println!("  protokół MCP: {}", bc_mcp::PROTOCOL_VERSIONS.join(", "));
            ExitCode::SUCCESS
        }
    }
}
