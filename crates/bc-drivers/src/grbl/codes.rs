//! Tabele protokołu GRBL (ADR 0010 §3): kody błędów i alarmów z opisami, ustawienia
//! `$N` z opisami i zakresami, odmiany firmware i producentów.
//!
//! Źródło numeracji kodów i ustawień: publiczna dokumentacja protokołu GRBL 1.1
//! (wiki projektu gnea/grbl: „Interface”, „Grbl v1.1 Configuration”). Opisy są własne.

use serde::Serialize;

/// Opis błędu `error:N`.
pub fn error_text(code: u32) -> &'static str {
    match code {
        1 => "słowo G-code bez litery polecenia",
        2 => "brak albo niepoprawna wartość liczbowa",
        3 => "nieobsługiwane polecenie systemowe $",
        4 => "wartość ujemna tam, gdzie wymagana dodatnia",
        5 => "bazowanie wyłączone ($22=0)",
        6 => "impuls kroku krótszy niż 3 µs",
        7 => "błąd odczytu EEPROM — przywrócono ustawienia domyślne",
        8 => "polecenie $ dozwolone tylko w stanie Idle",
        9 => "G-code zablokowany w stanie alarmu albo jog",
        10 => "limity programowe wymagają włączonego bazowania",
        11 => "wiersz za długi",
        12 => "przekroczona maksymalna częstotliwość kroków",
        13 => "otwarte drzwi bezpieczeństwa",
        14 => "wiersz startowy za długi",
        15 => "ruch jog poza obszarem maszyny",
        16 => "niepoprawne polecenie jog",
        17 => "tryb laserowy wymaga wyjścia PWM",
        20 => "nieobsługiwane polecenie G-code",
        21 => "dwa polecenia z tej samej grupy modalnej",
        22 => "posuw F nieustawiony",
        23 => "polecenie wymaga wartości całkowitej",
        24 => "dwa polecenia wymagające osi w jednym wierszu",
        25 => "powtórzone słowo w wierszu",
        26 => "brak osi w poleceniu, które ich wymaga",
        27 => "niepoprawny numer wiersza",
        28 => "brak wymaganego parametru",
        29 => "nieobsługiwany układ współrzędnych G59.x",
        30 => "G53 wymaga G0 albo G1",
        31 => "słowa osi bez polecenia, które ich używa",
        32 => "łuk G2/G3 bez osi w płaszczyźnie",
        33 => "niepoprawny cel ruchu",
        34 => "łuk: niepoprawny promień",
        35 => "łuk: brak I/J/K w płaszczyźnie",
        36 => "nieużyte słowa w wierszu",
        37 => "G43.1: oś korekcji narzędzia",
        38 => "numer narzędzia większy niż dozwolony",
        _ => "nieznany błąd",
    }
}

/// Opis alarmu `ALARM:N`.
pub fn alarm_text(code: u32) -> &'static str {
    match code {
        1 => "wyłącznik krańcowy (hard limit) — pozycja utracona, wymagane bazowanie",
        2 => "ruch poza obszarem maszyny (soft limit) — zadanie przerwane",
        3 => "reset w trakcie ruchu — pozycja może być utracona, zalecane bazowanie",
        4 => "sonda: niepoprawny stan początkowy",
        5 => "sonda: brak kontaktu w zakresie ruchu",
        6 => "bazowanie: reset w trakcie bazowania",
        7 => "bazowanie: otwarte drzwi w trakcie bazowania",
        8 => "bazowanie: nie zwolniono wyłącznika krańcowego",
        9 => "bazowanie: nie znaleziono wyłącznika krańcowego",
        10 => "bazowanie: nie znaleziono drugiego wyłącznika osi podwójnej",
        _ => "nieznany alarm",
    }
}

/// Rodzaj wartości ustawienia.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SettingKind {
    Bool,
    Int,
    Mask,
    Float,
}

/// Opis ustawienia `$N`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct SettingInfo {
    pub id: u32,
    pub name: &'static str,
    pub unit: &'static str,
    pub kind: SettingKind,
    pub min: f64,
    pub max: f64,
}

const fn s(
    id: u32,
    name: &'static str,
    unit: &'static str,
    kind: SettingKind,
    min: f64,
    max: f64,
) -> SettingInfo {
    SettingInfo {
        id,
        name,
        unit,
        kind,
        min,
        max,
    }
}

use SettingKind::*;

/// Ustawienia GRBL 1.1 z opisami i zakresami (REQ-13-198).
pub const SETTINGS: &[SettingInfo] = &[
    s(0, "długość impulsu kroku", "µs", Int, 3.0, 255.0),
    s(1, "opóźnienie wyłączenia silników", "ms", Int, 0.0, 255.0),
    s(
        2,
        "odwrócenie impulsu kroku (maska osi)",
        "",
        Mask,
        0.0,
        7.0,
    ),
    s(3, "odwrócenie kierunku (maska osi)", "", Mask, 0.0, 7.0),
    s(4, "odwrócenie sygnału enable", "", Bool, 0.0, 1.0),
    s(5, "odwrócenie wyłączników krańcowych", "", Bool, 0.0, 1.0),
    s(6, "odwrócenie wejścia sondy", "", Bool, 0.0, 1.0),
    s(10, "zawartość raportu stanu (maska)", "", Mask, 0.0, 3.0),
    s(
        11,
        "odchylenie na złączu (junction deviation)",
        "mm",
        Float,
        0.0,
        5.0,
    ),
    s(12, "tolerancja łuków", "mm", Float, 0.0, 5.0),
    s(13, "raport w calach", "", Bool, 0.0, 1.0),
    s(20, "limity programowe", "", Bool, 0.0, 1.0),
    s(21, "wyłączniki krańcowe (hard limits)", "", Bool, 0.0, 1.0),
    s(22, "bazowanie", "", Bool, 0.0, 1.0),
    s(23, "kierunek bazowania (maska osi)", "", Mask, 0.0, 7.0),
    s(
        24,
        "posuw bazowania (dokładny)",
        "mm/min",
        Float,
        1.0,
        100000.0,
    ),
    s(
        25,
        "posuw bazowania (szukanie)",
        "mm/min",
        Float,
        1.0,
        100000.0,
    ),
    s(26, "eliminacja drgań styków", "ms", Int, 0.0, 65535.0),
    s(
        27,
        "odjazd od wyłącznika po bazowaniu",
        "mm",
        Float,
        0.0,
        100.0,
    ),
    // rozszerzenia grblHAL / GRBL-ESP32 (np. płyty MKS DLC32)
    s(28, "odskok cyklu G73", "mm", Float, 0.0, 1000.0),
    s(
        30,
        "maksymalna wartość S (moc 100 %)",
        "",
        Float,
        1.0,
        100000.0,
    ),
    s(31, "minimalna wartość S", "", Float, 0.0, 100000.0),
    s(32, "tryb laserowy", "", Bool, 0.0, 1.0),
    s(100, "kroki na mm, oś X", "kroki/mm", Float, 0.001, 100000.0),
    s(101, "kroki na mm, oś Y", "kroki/mm", Float, 0.001, 100000.0),
    s(102, "kroki na mm, oś Z", "kroki/mm", Float, 0.001, 100000.0),
    s(
        110,
        "prędkość maksymalna X",
        "mm/min",
        Float,
        1.0,
        1000000.0,
    ),
    s(
        111,
        "prędkość maksymalna Y",
        "mm/min",
        Float,
        1.0,
        1000000.0,
    ),
    s(
        112,
        "prędkość maksymalna Z",
        "mm/min",
        Float,
        1.0,
        1000000.0,
    ),
    s(120, "przyspieszenie X", "mm/s²", Float, 1.0, 100000.0),
    s(121, "przyspieszenie Y", "mm/s²", Float, 1.0, 100000.0),
    s(122, "przyspieszenie Z", "mm/s²", Float, 1.0, 100000.0),
    s(130, "zakres ruchu X", "mm", Float, 1.0, 100000.0),
    s(131, "zakres ruchu Y", "mm", Float, 1.0, 100000.0),
    s(132, "zakres ruchu Z", "mm", Float, 0.0, 100000.0),
    // osie dodatkowe A, B, C (grblHAL, GRBL-ESP32, MKS DLC32)
    s(103, "kroki na mm, oś A", "kroki/mm", Float, 0.001, 100000.0),
    s(104, "kroki na mm, oś B", "kroki/mm", Float, 0.001, 100000.0),
    s(105, "kroki na mm, oś C", "kroki/mm", Float, 0.001, 100000.0),
    s(
        113,
        "prędkość maksymalna, oś A",
        "mm/min",
        Float,
        1.0,
        1000000.0,
    ),
    s(
        114,
        "prędkość maksymalna, oś B",
        "mm/min",
        Float,
        1.0,
        1000000.0,
    ),
    s(
        115,
        "prędkość maksymalna, oś C",
        "mm/min",
        Float,
        1.0,
        1000000.0,
    ),
    s(123, "przyspieszenie, oś A", "mm/s²", Float, 1.0, 100000.0),
    s(124, "przyspieszenie, oś B", "mm/s²", Float, 1.0, 100000.0),
    s(125, "przyspieszenie, oś C", "mm/s²", Float, 1.0, 100000.0),
    s(133, "zakres ruchu, oś A", "mm", Float, 0.0, 100000.0),
    s(134, "zakres ruchu, oś B", "mm", Float, 0.0, 100000.0),
    s(135, "zakres ruchu, oś C", "mm", Float, 0.0, 100000.0),
];

/// Nazwa ustawienia spoza tabeli: rozszerzenie producenta płyty bez opisu w GRBL 1.1.
pub const VENDOR_SETTING: &str =
    "ustawienie rozszerzone producenta płyty (opis w dokumentacji sterownika)";

pub fn setting_info(id: u32) -> Option<&'static SettingInfo> {
    SETTINGS.iter().find(|s| s.id == id)
}

/// Sprawdza wartość ustawienia względem opisu (nieznane ustawienia: tylko liczba).
pub fn validate_setting(id: u32, value: &str) -> Result<f64, String> {
    let v: f64 = value
        .trim()
        .parse()
        .map_err(|_| format!("${id}: wartość „{value}” nie jest liczbą"))?;
    if !v.is_finite() {
        return Err(format!("${id}: liczba skończona"));
    }
    if let Some(i) = setting_info(id) {
        if v < i.min || v > i.max {
            return Err(format!("${id} ({}): zakres {}…{}", i.name, i.min, i.max));
        }
        if matches!(i.kind, Bool | Int | Mask) && v.fract() != 0.0 {
            return Err(format!("${id} ({}): wartość całkowita", i.name));
        }
    }
    Ok(v)
}

/// Odmiana firmware z rodziny GRBL (REQ-13-050, 053…070).
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Variant {
    pub id: &'static str,
    pub name: &'static str,
    /// Powitanie po resecie.
    pub welcome: &'static str,
    /// Wiersz `[VER:…]` z `$I` (pusty dla 0.9).
    pub version: &'static str,
    /// Słowa identyfikujące producenta w `$I` (alternatywy rozdzielone `|`).
    pub marker: &'static str,
    /// Raport stanu w formacie 0.9 (`<Idle,MPos:…,WPos:…>`).
    pub status_v09: bool,
    /// `$J=` (1.1); bez tego jog jako `G91 G0`.
    pub jog: bool,
    /// Nadpisania czasu rzeczywistego (1.1).
    pub overrides: bool,
    /// Karta SD przez `$SD/…` (FluidNC).
    pub sd: bool,
    pub s_max: f64,
    pub baud: u32,
    /// Domyślny transport sieciowy (Telnet 23 / WebSocket).
    pub telnet: bool,
    pub websocket: bool,
    /// Uwagi (UNVERIFIED — szczegóły bez sprzętu).
    pub notes: &'static str,
}

#[allow(clippy::too_many_arguments)]
const fn v(
    id: &'static str,
    name: &'static str,
    welcome: &'static str,
    version: &'static str,
    marker: &'static str,
    flags: (bool, bool, bool, bool),
    s_max: f64,
    baud: u32,
    net: (bool, bool),
    notes: &'static str,
) -> Variant {
    Variant {
        id,
        name,
        welcome,
        version,
        marker,
        status_v09: flags.0,
        jog: flags.1,
        overrides: flags.2,
        sd: flags.3,
        s_max,
        baud,
        telnet: net.0,
        websocket: net.1,
        notes,
    }
}

const G11: (bool, bool, bool, bool) = (false, true, true, false);

// UNVERIFIED: powitania, znaczniki $I, s_max i prędkości portu odmian producentów pochodzą
// z dokumentacji producentów i opisów użytkowników; potwierdzić na sprzęcie (test M2.5,
// przechwyty od użytkowników). Rozpoznanie zawsze wraca do ogólnego GRBL 1.1.
pub const VARIANTS: &[Variant] = &[
    v(
        "grbl09",
        "GRBL 0.9",
        "Grbl 0.9j ['$' for help]",
        "",
        "",
        (true, false, false, false),
        1000.0,
        115200,
        (false, false),
        "brak $J= i nadpisań; raport stanu 0.9",
    ),
    v(
        "grbl11",
        "GRBL 1.1f/1.1h",
        "Grbl 1.1h ['$' for help]",
        "[VER:1.1h.20190825:]",
        "",
        G11,
        1000.0,
        115200,
        (false, false),
        "",
    ),
    v(
        "grblhal",
        "grblHAL",
        "GrblHAL 1.1f ['$' or '$HELP' for help]",
        "[VER:1.1f.20240801:]",
        "[FIRMWARE:grblHAL]",
        G11,
        1000.0,
        115200,
        (true, true),
        "Telnet/WebSocket zależnie od płyty",
    ),
    v(
        "fluidnc",
        "FluidNC",
        "Grbl 3.7 [FluidNC v3.7.0 (wifi) '$' for help]",
        "[VER:3.7 FluidNC v3.7.0:]",
        "FluidNC",
        (false, true, true, true),
        1000.0,
        115200,
        (true, true),
        "karta SD: $SD/List, $SD/Run, XMODEM",
    ),
    v(
        "grbl_lpc",
        "GRBL-LPC",
        "Grbl 1.1f ['$' for help]",
        "[VER:1.1f.20180715:LPC1769]",
        "LPC",
        G11,
        1000.0,
        115200,
        (false, false),
        "K40 po modernizacji (Cohesion3D, SKR)",
    ),
    v(
        "grbl_esp32",
        "GRBL-ESP32",
        "Grbl 1.3a ['$' for help]",
        "[VER:1.3a.20210424:]",
        "ESP32",
        G11,
        1000.0,
        115200,
        (true, true),
        "",
    ),
    v(
        "grbl_m3",
        "GRBL-M3 (diody)",
        "Grbl 1.1f ['$' for help]",
        "[VER:1.1f.20170801:]",
        "",
        G11,
        255.0,
        115200,
        (false, false),
        "S w zakresie 0–255",
    ),
    v(
        "ortur",
        "Ortur (GRBL)",
        "Grbl 1.1f ['$' for help]",
        "[VER:1.1f.20210909:]",
        "ORTUR",
        G11,
        1000.0,
        115200,
        (false, false),
        "",
    ),
    v(
        "atomstack",
        "Atomstack (GRBL)",
        "Grbl 1.1f ['$' for help]",
        "[VER:1.1f.20200901:]",
        "ATOMSTACK",
        G11,
        1000.0,
        115200,
        (false, false),
        "",
    ),
    v(
        "sculpfun",
        "Sculpfun (GRBL)",
        "Grbl 1.1f ['$' for help]",
        "[VER:1.1f.20211015:]",
        "SCULPFUN",
        G11,
        1000.0,
        115200,
        (false, false),
        "",
    ),
    v(
        "creality_falcon",
        "Creality Falcon (GRBL)",
        "Grbl 1.1h ['$' for help]",
        "[VER:1.1h.20220301:]",
        "FALCON",
        G11,
        1000.0,
        115200,
        (false, false),
        "",
    ),
    v(
        "two_trees",
        "Two Trees (GRBL)",
        "Grbl 1.1f ['$' for help]",
        "[VER:1.1f.20210601:]",
        "TWOTREES|TWO TREES|MACHINE:TTS-|MACHINE:TS2|MACHINE:TS3",
        G11,
        1000.0,
        115200,
        (false, false),
        "",
    ),
    v(
        "neje",
        "NEJE (G-code)",
        "Grbl 1.1f ['$' for help]",
        "[VER:1.1f.20220101:]",
        "NEJE",
        G11,
        1000.0,
        115200,
        (false, false),
        "tylko modele z firmware G-code",
    ),
    v(
        "comgrow",
        "Comgrow (GRBL)",
        "Grbl 1.1f ['$' for help]",
        "[VER:1.1f.20220601:]",
        "COMGROW",
        G11,
        1000.0,
        115200,
        (false, false),
        "",
    ),
    v(
        "longer",
        "Longer (GRBL)",
        "Grbl 1.1f ['$' for help]",
        "[VER:1.1f.20220201:]",
        "LONGER",
        G11,
        1000.0,
        115200,
        (false, false),
        "",
    ),
    v(
        "snapmaker",
        "Snapmaker (GRBL)",
        "Grbl 1.1h ['$' for help]",
        "[VER:1.1h.20220801:]",
        "SNAPMAKER",
        G11,
        1000.0,
        115200,
        (false, false),
        "moduł laserowy z firmware GRBL",
    ),
    v(
        "algolaser",
        "Algolaser (GRBL)",
        "Grbl 1.1h ['$' for help]",
        "[VER:1.1h.20230101:]",
        "ALGOLASER",
        G11,
        1000.0,
        115200,
        (true, false),
        "",
    ),
    v(
        "wainlux",
        "Wainlux (GRBL)",
        "Grbl 1.1f ['$' for help]",
        "[VER:1.1f.20220401:]",
        "WAINLUX",
        G11,
        1000.0,
        115200,
        (false, false),
        "",
    ),
    v(
        "k40_upgrade",
        "K40 po modernizacji (G-code)",
        "Grbl 1.1f ['$' for help]",
        "[VER:1.1f.20180715:LPC1769]",
        "C3D",
        G11,
        1000.0,
        115200,
        (false, false),
        "Cohesion3D Mini/LaserBoard, Awesome.tech, SKR + GRBL-LPC/grblHAL",
    ),
];

pub fn variant(id: &str) -> Option<&'static Variant> {
    VARIANTS.iter().find(|v| v.id == id)
}

/// Rozpoznanie odmiany z powitania i odpowiedzi `$I` (najpierw znaczniki producentów).
pub fn detect(welcome: &str, info: &str) -> &'static Variant {
    let all = format!("{welcome}\n{info}").to_ascii_uppercase();
    if let Some(v) = VARIANTS
        .iter()
        .filter(|v| !v.marker.is_empty() && v.id != "grbl_lpc")
        .find(|v| {
            v.marker
                .split('|')
                .any(|m| all.contains(&m.to_ascii_uppercase()))
        })
    {
        return v;
    }
    if all.contains("GRBLHAL") {
        return variant("grblhal").expect("grblhal");
    }
    if all.contains("LPC") {
        return variant("grbl_lpc").expect("lpc");
    }
    if all.contains("GRBL 0.") {
        return variant("grbl09").expect("0.9");
    }
    if all.contains("GRBL 1.3") || all.contains("ESP32") {
        return variant("grbl_esp32").expect("esp32");
    }
    variant("grbl11").expect("1.1")
}
