//! Polecenia wykonywane przez okno GUI (REQ-01-029): widok, narzędzia,
//! przyciąganie, aktywna warstwa i okna dialogowe. Rejestr opisuje je tak jak
//! pozostałe polecenia (id, schematy, poziom, skrót); wykonuje je okno — lokalnie
//! przy akcji użytkownika albo na żądanie hosta (`ui.execute`) dla MCP i skryptów.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{schema_of, CommandSpec, Executor, Registry, SafetyTier};

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[allow(dead_code)]
struct ZoomParams {
    /// Mnożnik powiększenia (> 1 = przybliż).
    factor: f64,
    /// Punkt świata [x, y] (mm), który zostaje w miejscu; domyślnie środek widoku.
    #[serde(default)]
    at: Option<[f64; 2]>,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[allow(dead_code)]
struct PanParams {
    /// Przesunięcie rysunku w prawo (mm).
    dx: f64,
    /// Przesunięcie rysunku w górę (mm).
    dy: f64,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct NoParams {}

#[derive(Serialize, JsonSchema)]
#[allow(dead_code)]
struct ViewResult {
    /// Środek widoku (mm).
    cx: f64,
    cy: f64,
    /// Powiększenie w % (100 = rzeczywisty rozmiar po kalibracji ekranu).
    zoom: f64,
    /// Widoczny obszar świata [x0, y0, x1, y1] (mm).
    visible: [f64; 4],
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
#[allow(dead_code)]
enum ToolId {
    Select,
    Line,
    Node,
    Lasso,
    Trim,
    Eraser,
    Rotate2,
    Scale2,
    Warp,
    Rect,
    Ellipse,
    Polygon,
    Star,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[allow(dead_code)]
struct ToolParams {
    /// Narzędzie (narzędzia zaawansowane są niedostępne w trybie początkującego).
    tool: ToolId,
}

#[derive(Serialize, JsonSchema)]
#[allow(dead_code)]
struct ToolResult {
    tool: String,
}

#[derive(Deserialize, JsonSchema)]
#[allow(dead_code)]
enum ToolOption {
    /// Liczba boków wielokąta (3–1000).
    #[serde(rename = "sides")]
    Sides,
    /// Liczba ramion gwiazdy (2–1000).
    #[serde(rename = "points")]
    Points,
    /// Stosunek promieni gwiazdy (0–1).
    #[serde(rename = "ratio")]
    Ratio,
    /// Szerokość gumki (mm).
    #[serde(rename = "eraserWidth")]
    EraserWidth,
    /// Tryb wypaczania: "4" (4 punkty) | "mesh" | "envelope".
    #[serde(rename = "warpMode")]
    WarpMode,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[allow(dead_code)]
struct ToolOptionParams {
    key: ToolOption,
    value: serde_json::Value,
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
#[allow(dead_code)]
enum SnapKind {
    Grid,
    Nodes,
    Mids,
    Centers,
    Edges,
    Align,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[allow(dead_code)]
struct SnapParams {
    key: SnapKind,
    enabled: bool,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[allow(dead_code)]
struct LayerActiveParams {
    /// Warstwa nowych kształtów, gdy nic nie jest zaznaczone.
    layer: u8,
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
#[allow(dead_code)]
enum DialogId {
    Settings,
    Shortcuts,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[allow(dead_code)]
struct DialogParams {
    dialog: DialogId,
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
#[allow(dead_code)]
enum AnchorId {
    Tl,
    Tc,
    Tr,
    Cl,
    Cc,
    Cr,
    Bl,
    Bc,
    Br,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[allow(dead_code)]
struct AnchorParams {
    /// Punkt odniesienia pól X/Y/W/H/obrót (t/c/b = góra/środek/dół, l/c/r = lewo/środek/prawo).
    anchor: AnchorId,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[allow(dead_code)]
struct LockParams {
    /// Zmiana W albo H zachowuje proporcje.
    locked: bool,
}

#[derive(Serialize, JsonSchema)]
struct Empty {}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[allow(dead_code)]
struct PreviewOpenParams {
    /// Tylko zaznaczone obiekty.
    #[serde(default)]
    selection_only: bool,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[allow(dead_code)]
struct PreviewSetParams {
    /// Położenie suwaka: czas od startu zadania, s (REQ-09-001).
    #[serde(default)]
    time: Option<f64>,
    /// Prędkość odtwarzania (×1…×1000).
    #[serde(default)]
    speed: Option<f64>,
    /// Odtwarzanie / pauza.
    #[serde(default)]
    playing: Option<bool>,
    /// Pokaż przejazdy (REQ-09-002).
    #[serde(default)]
    travel: Option<bool>,
    /// Numery kolejności części (REQ-09-003).
    #[serde(default)]
    order: Option<bool>,
    /// Szerokość cięcia z kerfem (REQ-09-004).
    #[serde(default)]
    kerf: Option<bool>,
    /// Strzałki kierunku (REQ-09-005).
    #[serde(default)]
    direction: Option<bool>,
    /// Punkty startowe (REQ-09-006).
    #[serde(default)]
    starts: Option<bool>,
}

#[derive(Serialize, JsonSchema)]
#[allow(dead_code)]
struct PreviewState {
    open: bool,
    time: f64,
    total: f64,
    speed: f64,
    playing: bool,
    /// Segment w bieżącym czasie (do „rozpocznij od tego miejsca”).
    segment: usize,
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
#[allow(dead_code)]
enum JogMode {
    Slow,
    Fast,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[allow(dead_code)]
struct JogSettingsParams {
    /// Krok jogu XY, mm (REQ-03-037).
    #[serde(default)]
    step_xy: Option<f64>,
    /// Krok jogu Z, mm.
    #[serde(default)]
    step_z: Option<f64>,
    /// Prędkość wolna, mm/s (REQ-03-038).
    #[serde(default)]
    slow: Option<f64>,
    /// Prędkość szybka, mm/s.
    #[serde(default)]
    fast: Option<f64>,
    /// Bieżąca prędkość: slow | fast.
    #[serde(default)]
    mode: Option<JogMode>,
}

/// Zakładka okna ustawień warstwy.
#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
#[allow(dead_code)]
enum LayerTab {
    General,
    Cut,
    Scan,
    Advanced,
    Sublayers,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[allow(dead_code)]
struct LayerSettingsOpenParams {
    /// Indeks warstwy (0–31).
    index: u8,
    /// Zakładka: general | cut | scan | advanced | sublayers (domyślnie general).
    #[serde(default)]
    tab: Option<LayerTab>,
}

#[allow(clippy::too_many_arguments)]
fn add_req<P: JsonSchema, R: JsonSchema>(
    r: &mut Registry,
    id: &'static str,
    title: &'static str,
    description: &'static str,
    requirements: &'static [&'static str],
) {
    r.register_window(CommandSpec {
        executor: Executor::Window,
        id,
        title,
        description,
        params_schema: schema_of::<P>(),
        result_schema: schema_of::<R>(),
        undoable: false,
        requires_device: false,
        safety_tier: SafetyTier::Read,
        shortcut: None,
        requirements,
    });
}

#[allow(clippy::too_many_arguments)]
fn add<P: JsonSchema, R: JsonSchema>(
    r: &mut Registry,
    id: &'static str,
    title: &'static str,
    description: &'static str,
    shortcut: Option<&'static str>,
) {
    r.register_window(CommandSpec {
        executor: Executor::Window,
        id,
        title,
        description,
        params_schema: schema_of::<P>(),
        result_schema: schema_of::<R>(),
        undoable: false,
        requires_device: false,
        safety_tier: SafetyTier::Read,
        shortcut,
        requirements: &["REQ-01-029"],
    });
}

pub(crate) fn register_all(r: &mut Registry) {
    add::<ZoomParams, ViewResult>(
        r,
        "view.zoom",
        "Powiększ",
        "Zmienia powiększenie widoku wokół punktu (kółko myszy, gest, przyciski +/−).",
        None,
    );
    add::<PanParams, ViewResult>(
        r,
        "view.pan",
        "Przesuń widok",
        "Przesuwa widok o wektor w mm (przeciągnięcie środkowym przyciskiem, gest dwoma palcami).",
        None,
    );
    add::<NoParams, ViewResult>(
        r,
        "view.zoom_page",
        "Dopasuj stronę",
        "Pokazuje cały obszar roboczy.",
        None,
    );
    add::<NoParams, ViewResult>(
        r,
        "view.zoom_selection",
        "Dopasuj zaznaczenie",
        "Pokazuje zaznaczone obiekty.",
        None,
    );
    add::<NoParams, ViewResult>(
        r,
        "view.actual_size",
        "Rzeczywisty rozmiar",
        "Powiększenie 100% (po kalibracji ekranu).",
        None,
    );
    add::<NoParams, ViewResult>(
        r,
        "view.get",
        "Stan widoku",
        "Środek, powiększenie i widoczny obszar.",
        None,
    );
    add::<ToolParams, ToolResult>(
        r,
        "tool.select",
        "Wybierz narzędzie",
        "Aktywuje narzędzie (Esc wraca do zaznaczania).",
        None,
    );
    add::<ToolOptionParams, Empty>(r, "tool.set_option", "Opcja narzędzia", "Ustawia opcję narzędzia (boki wielokąta, ramiona gwiazdy, szerokość gumki, tryb wypaczania).", None);
    add::<SnapParams, Empty>(
        r,
        "snap.set",
        "Przyciąganie",
        "Włącza lub wyłącza rodzaj przyciągania.",
        None,
    );
    add::<LayerActiveParams, Empty>(
        r,
        "layer.set_active",
        "Aktywna warstwa",
        "Warstwa dla nowych kształtów, gdy nic nie jest zaznaczone.",
        None,
    );
    add::<AnchorParams, Empty>(
        r,
        "transform.set_anchor",
        "Punkt odniesienia",
        "Punkt odniesienia (9-punktowy) pól edycji numerycznej.",
        None,
    );
    add::<LockParams, Empty>(
        r,
        "transform.lock_aspect",
        "Blokada proporcji",
        "Zmiana szerokości albo wysokości zachowuje proporcje.",
        None,
    );
    r.register_window(CommandSpec {
        executor: Executor::Window,
        id: "job.cancel",
        title: "Anuluj operację",
        description: "Przerywa trwającą długą operację (przycisk przy pasku postępu); dokument zostaje bez zmian.",
        params_schema: schema_of::<NoParams>(),
        result_schema: schema_of::<Empty>(),
        undoable: false,
        requires_device: false,
        safety_tier: SafetyTier::Read,
        shortcut: None,
        requirements: &["REQ-01-029", "REQ-16-006"],
    });
    add_req::<PreviewOpenParams, PreviewState>(
        r,
        "preview.open",
        "Podgląd",
        "Otwiera okno Podgląd zadania: tor głowicy z odtwarzaniem, przejazdy, kolejność, kerf, kierunek i punkty startowe.",
        &["REQ-01-029", "REQ-03-046", "REQ-09-001"],
    );
    add_req::<NoParams, Empty>(
        r,
        "preview.close",
        "Zamknij podgląd",
        "Zamyka okno Podgląd.",
        &["REQ-01-029", "REQ-03-046"],
    );
    add_req::<PreviewSetParams, PreviewState>(
        r,
        "preview.set",
        "Ustaw podgląd",
        "Suwak czasu, prędkość i odtwarzanie, widoczność przejazdów, numerów kolejności, kerfu, kierunku i punktów startowych w oknie Podgląd.",
        &[
            "REQ-01-029", "REQ-03-046", "REQ-09-001", "REQ-09-002", "REQ-09-003", "REQ-09-004",
            "REQ-09-005", "REQ-09-006", "REQ-07-030",
        ],
    );
    add_req::<JogSettingsParams, Empty>(
        r,
        "jog.settings",
        "Ustawienia jogu",
        "Okno Przesuń: krok XY i Z, prędkość wolna i szybka oraz bieżąca prędkość (zapis w preferencjach ui.jog).",
        &["REQ-01-029", "REQ-03-037", "REQ-03-038"],
    );
    add_req::<LayerSettingsOpenParams, Empty>(
        r,
        "layer.settings_open",
        "Ustawienia warstwy",
        "Otwiera okno ustawień warstwy (Warstwy/Cięcia, §3): wszystkie parametry layer.set i layer.params z §7.2 w zakładkach Ogólne, Cięcie, Grawer/Skanowanie, Zaawansowane, Pod-warstwy.",
        &["REQ-01-029", "REQ-03-032"],
    );
    add_req::<NoParams, Empty>(
        r,
        "layer.settings_close",
        "Zamknij ustawienia warstwy",
        "Zamyka okno ustawień warstwy.",
        &["REQ-01-029", "REQ-03-032"],
    );
    add::<DialogParams, Empty>(
        r,
        "dialog.open",
        "Otwórz okno",
        "Otwiera okno Ustawienia albo Skróty klawiszowe.",
        None,
    );
}
