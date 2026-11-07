//! Parametry cięcia warstwy (§7.2) i ustawienia planu dokumentu (§8) — ADR 0009.
//!
//! Wszystkie pola mają wartości domyślne, więc dokumenty schematu 3 wczytują się
//! bez zmian (migracja 3 → 4). Jednostki: mm, mm/s, mm/s², s, % mocy.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::LayerMode;

/// Grupowanie kształtów przy wypełnieniu Fill (REQ-07-003…005).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum FillGrouping {
    /// Wszystkie kształty warstwy razem: jedna linia skanowania przechodzi przez wszystkie.
    #[default]
    All,
    /// Każda grupa najwyższego poziomu (i każdy obiekt poza grupą) osobno.
    Groups,
    /// Każdy kształt osobno.
    Each,
}

/// Rodzaj wejścia/wyjścia (REQ-07-035/036).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum LeadKind {
    #[default]
    None,
    Line,
    Arc,
}

/// Lead-in albo lead-out (REQ-07-035…037). Linia: `length` mm pod kątem `angle`
/// do stycznej konturu. Łuk: promień `length` mm, kąt środkowy `angle`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Lead {
    #[serde(default)]
    pub kind: LeadKind,
    /// mm
    #[serde(default = "d_lead_len")]
    pub length: f64,
    /// stopnie
    #[serde(default = "d_lead_angle")]
    pub angle: f64,
}

impl Default for Lead {
    fn default() -> Self {
        Lead {
            kind: LeadKind::None,
            length: d_lead_len(),
            angle: d_lead_angle(),
        }
    }
}

/// Mostki automatyczne (REQ-07-031…034, REQ-04-084).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TabParams {
    /// Mostki automatyczne włączone.
    #[serde(default)]
    pub enabled: bool,
    /// Długość mostka (przerwy w cięciu), mm. Dotyczy też mostków ręcznych bez własnego rozmiaru.
    #[serde(default = "d_tab_size")]
    pub size: f64,
    /// Liczba mostków na kontur (0 = z odstępu).
    #[serde(default)]
    pub count: u32,
    /// Odstęp między mostkami, mm (gdy `count` = 0).
    #[serde(default = "d_tab_spacing")]
    pub spacing: f64,
    /// Nie stawiaj mostków automatycznych na kształtach wewnętrznych.
    #[serde(default = "yes")]
    pub skip_inner: bool,
}

impl Default for TabParams {
    fn default() -> Self {
        TabParams {
            enabled: false,
            size: d_tab_size(),
            count: 0,
            spacing: d_tab_spacing(),
            skip_inner: true,
        }
    }
}

/// Perforacja: naprzemiennie `cut` mm cięcia i `gap` mm przerwy (REQ-07-039).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Perforation {
    pub cut: f64,
    pub gap: f64,
}

/// Tryb punktowy: impuls `dwell_ms` co `spacing` mm (REQ-07-040).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DotMode {
    pub dwell_ms: f64,
    pub spacing: f64,
}

/// Rampa mocy na początku i końcu cięcia (REQ-07-041).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Ramp {
    /// mm
    pub length: f64,
    /// Tylko kontury zewnętrzne.
    #[serde(default)]
    pub outer_only: bool,
}

/// Overscan: przedłużenie linii skanowania z wyłączoną wiązką (REQ-07-047/048).
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Overscan {
    Off,
    /// Droga rozpędzania v²/(2a) w osi skanowania.
    #[default]
    Auto,
    /// `percent` · prędkość (mm na każde 1 mm/s), np. 2,5 % przy 100 mm/s = 2,5 mm.
    Percent {
        percent: f64,
    },
}

/// Wiersz tabeli kompensacji scan offset (REQ-07-049).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ScanOffset {
    /// mm/s
    pub speed: f64,
    /// mm (przesunięcie linii powrotnych)
    pub offset: f64,
}

/// Pod-warstwa: dodatkowy przebieg o własnych ustawieniach (REQ-07-050).
/// Pola `None` dziedziczą wartość z warstwy.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SubLayer {
    #[serde(default)]
    pub name: String,
    #[serde(default = "yes")]
    pub output: bool,
    pub mode: LayerMode,
    #[serde(default)]
    pub speed: Option<f64>,
    #[serde(default)]
    pub power_max: Option<f64>,
    #[serde(default)]
    pub power_min: Option<f64>,
    #[serde(default)]
    pub passes: Option<u32>,
    #[serde(default)]
    pub interval: Option<f64>,
    #[serde(default)]
    pub scan_angle: Option<f64>,
}

/// Parametry cięcia warstwy z §7.2 (poza podstawowymi polami [`crate::Layer`]).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LayerParams {
    /// Moc maksymalna lasera 2, % (REQ-07-017). `None` = drugi laser nieużywany.
    #[serde(default)]
    pub power2_max: Option<f64>,
    /// Moc minimalna lasera 2, %.
    #[serde(default)]
    pub power2_min: Option<f64>,
    /// Opóźnienie przed każdym kolejnym przejściem, s (REQ-07-019).
    #[serde(default)]
    pub pass_delay: f64,
    /// Pauza przed warstwą z komunikatem, np. „zmień materiał” (REQ-07-020).
    #[serde(default)]
    pub pause_message: Option<String>,
    /// Z offset, mm (REQ-07-021).
    #[serde(default)]
    pub z_offset: f64,
    /// Obniżenie Z na każde przejście, mm (REQ-07-022).
    #[serde(default)]
    pub z_step: f64,
    /// Priorytet warstwy; niższy = wcześniej (REQ-07-023).
    #[serde(default)]
    pub priority: i32,
    /// Częstotliwość PWM, Hz (gdzie sterownik wspiera; REQ-07-024).
    #[serde(default)]
    pub pwm_hz: Option<u32>,
    /// Moc dynamiczna (GRBL M4) zamiast stałej (M3) (REQ-07-025).
    #[serde(default = "yes")]
    pub dynamic_power: bool,
    /// Kerf offset, mm: kontury zewnętrzne na zewnątrz, otwory do wewnątrz (REQ-07-026).
    #[serde(default)]
    pub kerf: f64,
    /// Osobny kerf w osi X, mm (zastępuje `kerf` w X; REQ-07-027).
    #[serde(default)]
    pub kerf_x: Option<f64>,
    /// Osobny kerf w osi Y, mm.
    #[serde(default)]
    pub kerf_y: Option<f64>,
    /// Kerf dla Fill: offset obszaru przed skanowaniem, mm (REQ-07-028).
    #[serde(default)]
    pub fill_kerf: f64,
    #[serde(default)]
    pub tabs: TabParams,
    #[serde(default)]
    pub lead_in: Lead,
    #[serde(default)]
    pub lead_out: Lead,
    /// Overcut zamkniętych kształtów, mm (REQ-07-038).
    #[serde(default)]
    pub overcut: f64,
    #[serde(default)]
    pub perforation: Option<Perforation>,
    #[serde(default)]
    pub dot_mode: Option<DotMode>,
    #[serde(default)]
    pub ramp: Option<Ramp>,
    /// Rampa mocy na narożnikach: moc proporcjonalna do prędkości (min moc w zwolnieniach; REQ-07-042).
    #[serde(default)]
    pub corner_ramp: bool,
    /// Skanowanie dwukierunkowe (REQ-07-043).
    #[serde(default = "yes")]
    pub bidirectional: bool,
    /// Kąt skanowania, stopnie (REQ-07-044).
    #[serde(default)]
    pub scan_angle: f64,
    /// Cross-hatch: drugi przebieg pod kątem +90° (REQ-07-045).
    #[serde(default)]
    pub cross_hatch: bool,
    /// Interwał linii skanowania, mm (REQ-07-046; LPI = 25,4 / interwał).
    #[serde(default = "d_interval")]
    pub interval: f64,
    #[serde(default)]
    pub overscan: Overscan,
    /// Tabela scan offset wg prędkości (REQ-07-049), interpolacja liniowa.
    #[serde(default)]
    pub scan_offsets: Vec<ScanOffset>,
    /// Grupowanie kształtów Fill (REQ-07-003…005).
    #[serde(default)]
    pub fill_grouping: FillGrouping,
    /// Pod-warstwy (REQ-07-050).
    #[serde(default)]
    pub sublayers: Vec<SubLayer>,
    /// Makro G-code przed warstwą (REQ-13-107, schemat 5); pola `{layer}`, `{label}`,
    /// `{pass}`, `{speed}`, `{power}`, `{power_pct}`.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub gcode_pre: String,
    /// Makro G-code po warstwie (REQ-13-107).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub gcode_post: String,
}

impl Default for LayerParams {
    fn default() -> Self {
        serde_json::from_value(serde_json::json!({})).expect("domyślne parametry warstwy")
    }
}

/// Interwał (mm) z linii na cal.
pub fn interval_from_lpi(lpi: f64) -> f64 {
    25.4 / lpi
}

/// Linie na cal z interwału (mm).
pub fn lpi_from_interval(interval: f64) -> f64 {
    25.4 / interval
}

impl LayerParams {
    /// Sprawdza zakresy; błąd = opis pola.
    pub fn validate(&self) -> Result<(), String> {
        let nonneg = |n: &str, v: f64| {
            if v.is_finite() && v >= 0.0 {
                Ok(())
            } else {
                Err(format!("{n}: liczba ≥ 0"))
            }
        };
        let pct = |n: &str, v: f64| {
            if (0.0..=100.0).contains(&v) {
                Ok(())
            } else {
                Err(format!("{n}: 0–100 %"))
            }
        };
        for (n, v) in [
            ("power2_max", self.power2_max),
            ("power2_min", self.power2_min),
        ] {
            if let Some(v) = v {
                pct(n, v)?;
            }
        }
        if let (Some(a), Some(b)) = (self.power2_min, self.power2_max) {
            if a > b {
                return Err("power2_min nie może przekraczać power2_max".into());
            }
        }
        nonneg("pass_delay", self.pass_delay)?;
        if !(self.z_offset.is_finite() && self.z_step.is_finite()) {
            return Err("z_offset, z_step: liczby skończone".into());
        }
        if !self.kerf.is_finite() || self.kerf.abs() > 10.0 {
            return Err("kerf: −10…10 mm".into());
        }
        for (n, v) in [("kerf_x", self.kerf_x), ("kerf_y", self.kerf_y)] {
            if let Some(v) = v {
                if !v.is_finite() || v.abs() > 10.0 {
                    return Err(format!("{n}: −10…10 mm"));
                }
            }
        }
        let (kx, ky) = (
            self.kerf_x.unwrap_or(self.kerf),
            self.kerf_y.unwrap_or(self.kerf),
        );
        if kx * ky < 0.0 {
            return Err("kerf_x i kerf_y muszą mieć ten sam znak".into());
        }
        if !self.fill_kerf.is_finite() || self.fill_kerf.abs() > 10.0 {
            return Err("fill_kerf: −10…10 mm".into());
        }
        if !(self.tabs.size > 0.0 && self.tabs.size.is_finite()) {
            return Err("tabs.size: > 0".into());
        }
        if !(self.tabs.spacing > 0.0 && self.tabs.spacing.is_finite()) {
            return Err("tabs.spacing: > 0".into());
        }
        for (n, l) in [("lead_in", &self.lead_in), ("lead_out", &self.lead_out)] {
            if !(l.length > 0.0 && l.length.is_finite() && l.angle.is_finite()) {
                return Err(format!("{n}: length > 0, angle skończony"));
            }
        }
        nonneg("overcut", self.overcut)?;
        if let Some(p) = self.perforation {
            if !(p.cut > 0.0 && p.gap > 0.0 && p.cut.is_finite() && p.gap.is_finite()) {
                return Err("perforation: cut i gap > 0".into());
            }
        }
        if let Some(d) = self.dot_mode {
            if !(d.dwell_ms > 0.0
                && d.spacing > 0.0
                && d.dwell_ms.is_finite()
                && d.spacing.is_finite())
            {
                return Err("dot_mode: dwell_ms i spacing > 0".into());
            }
        }
        if let Some(r) = self.ramp {
            if !(r.length > 0.0 && r.length.is_finite()) {
                return Err("ramp.length: > 0".into());
            }
        }
        if !self.scan_angle.is_finite() {
            return Err("scan_angle: liczba skończona".into());
        }
        if !(self.interval >= 0.001 && self.interval <= 50.0) {
            return Err("interval: 0,001–50 mm".into());
        }
        if let Overscan::Percent { percent } = self.overscan {
            if !(0.0..=100.0).contains(&percent) {
                return Err("overscan.percent: 0–100".into());
            }
        }
        for s in &self.scan_offsets {
            if !(s.speed > 0.0 && s.speed.is_finite() && s.offset.is_finite()) {
                return Err("scan_offsets: speed > 0, offset skończony".into());
            }
        }
        for (i, s) in self.sublayers.iter().enumerate() {
            if let Some(v) = s.speed {
                if !(v > 0.0 && v.is_finite()) {
                    return Err(format!("sublayers[{i}].speed: > 0"));
                }
            }
            for v in [s.power_max, s.power_min].into_iter().flatten() {
                pct(&format!("sublayers[{i}].power"), v)?;
            }
            if s.passes == Some(0) {
                return Err(format!("sublayers[{i}].passes: ≥ 1"));
            }
            if let Some(v) = s.interval {
                if !(0.001..=50.0).contains(&v) {
                    return Err(format!("sublayers[{i}].interval: 0,001–50 mm"));
                }
            }
        }
        for (n, m) in [
            ("gcode_pre", &self.gcode_pre),
            ("gcode_post", &self.gcode_post),
        ] {
            check_macro(n, m)?;
        }
        Ok(())
    }

    /// Scan offset dla prędkości (interpolacja liniowa, poza zakresem — wartość skrajna).
    pub fn scan_offset_at(&self, speed: f64) -> f64 {
        let mut t: Vec<ScanOffset> = self.scan_offsets.clone();
        t.sort_by(|a, b| a.speed.total_cmp(&b.speed));
        match t.as_slice() {
            [] => 0.0,
            [one] => one.offset,
            _ => {
                if speed <= t[0].speed {
                    return t[0].offset;
                }
                for w in t.windows(2) {
                    if speed <= w[1].speed {
                        let f = (speed - w[0].speed) / (w[1].speed - w[0].speed);
                        return w[0].offset + f * (w[1].offset - w[0].offset);
                    }
                }
                t[t.len() - 1].offset
            }
        }
    }
}

/// Kierunek cięcia konturów zamkniętych (REQ-08-006).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CutDirection {
    /// Kontury zamknięte w kierunku z rysunku; otwarte od bliższego końca.
    #[default]
    Auto,
    /// Zgodnie z ruchem wskazówek zegara.
    Cw,
    /// Przeciwnie do ruchu wskazówek zegara.
    Ccw,
}

/// Strefa zakazana (REQ-08-009/010): prostokąt w mm (układ dokumentu).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NoGoZone {
    #[serde(default)]
    pub name: String,
    /// [x0, y0, x1, y1]
    pub rect: [f64; 4],
    /// Zakaz przejazdów nad strefą (objazd).
    #[serde(default = "yes")]
    pub travel: bool,
    /// Zakaz wiązki w strefie (cięcie w strefie jest usuwane z ostrzeżeniem).
    #[serde(default = "yes")]
    pub beam: bool,
}

/// Kinematyka maszyny do modelu czasu i overscanu (REQ-08-018, REQ-09-008).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Kinematics {
    /// mm/s (GRBL `$110` / 60)
    #[serde(default = "d_vmax")]
    pub max_speed_x: f64,
    /// mm/s (GRBL `$111` / 60)
    #[serde(default = "d_vmax")]
    pub max_speed_y: f64,
    /// mm/s² (GRBL `$120`)
    #[serde(default = "d_accel")]
    pub accel_x: f64,
    /// mm/s² (GRBL `$121`)
    #[serde(default = "d_accel")]
    pub accel_y: f64,
    /// Odchylenie na złączu, mm (GRBL `$11`).
    #[serde(default = "d_junction")]
    pub junction_deviation: f64,
    /// Prędkość przejazdów, mm/s (ograniczona prędkościami osi).
    #[serde(default = "d_vmax")]
    pub travel_speed: f64,
    /// Wydłużenie faz przyspieszania przez ograniczenie jerku (1 = brak; Ruida > 1).
    #[serde(default = "one")]
    pub jerk_factor: f64,
}

impl Default for Kinematics {
    fn default() -> Self {
        serde_json::from_value(serde_json::json!({})).expect("domyślna kinematyka")
    }
}

impl Kinematics {
    /// Z ustawień GRBL: `$110/$111` (mm/min), `$120/$121` (mm/s²), `$11` (mm).
    /// Brakujące klucze zostają domyślne.
    pub fn from_grbl(settings: &std::collections::BTreeMap<u32, f64>) -> Self {
        let mut k = Kinematics::default();
        let get = |n: u32| {
            settings
                .get(&n)
                .copied()
                .filter(|v| v.is_finite() && *v > 0.0)
        };
        if let Some(v) = get(110) {
            k.max_speed_x = v / 60.0;
        }
        if let Some(v) = get(111) {
            k.max_speed_y = v / 60.0;
        }
        if let Some(v) = get(120) {
            k.accel_x = v;
        }
        if let Some(v) = get(121) {
            k.accel_y = v;
        }
        if let Some(v) = get(11) {
            k.junction_deviation = v;
        }
        k.travel_speed = k.max_speed_x.max(k.max_speed_y);
        k
    }

    /// Z ustawień Ruida: prędkość maks. (mm/s), przyspieszenie osi (mm/s²) i jerk
    /// jako współczynnik wydłużenia faz przyspieszania.
    pub fn from_ruida(max_speed: f64, accel_x: f64, accel_y: f64, jerk_factor: f64) -> Self {
        Kinematics {
            max_speed_x: max_speed,
            max_speed_y: max_speed,
            accel_x,
            accel_y,
            junction_deviation: d_junction(),
            travel_speed: max_speed,
            jerk_factor: jerk_factor.max(1.0),
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        for (n, v) in [
            ("max_speed_x", self.max_speed_x),
            ("max_speed_y", self.max_speed_y),
            ("accel_x", self.accel_x),
            ("accel_y", self.accel_y),
            ("travel_speed", self.travel_speed),
        ] {
            if !(v > 0.0 && v.is_finite()) {
                return Err(format!("{n}: > 0"));
            }
        }
        if !(self.junction_deviation >= 0.0 && self.junction_deviation.is_finite()) {
            return Err("junction_deviation: ≥ 0".into());
        }
        if !(self.jerk_factor >= 1.0 && self.jerk_factor.is_finite()) {
            return Err("jerk_factor: ≥ 1".into());
        }
        Ok(())
    }
}

/// Ustawienia planu dokumentu (§8).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PlanSettings {
    /// Opcja globalna: grawer najpierw, wewnętrzne przed zewnętrznymi, cięcie zawsze na końcu (REQ-07-051).
    #[serde(default)]
    pub cut_last: bool,
    /// Wewnętrzne przed zewnętrznymi (REQ-08-004).
    #[serde(default = "yes")]
    pub inner_first: bool,
    /// Obiekty jednej grupy razem (REQ-08-003).
    #[serde(default = "yes")]
    pub order_by_group: bool,
    /// Najlepszy punkt startowy konturów zamkniętych (REQ-08-005).
    #[serde(default = "yes")]
    pub optimize_start: bool,
    /// Kierunek cięcia (REQ-08-006).
    #[serde(default)]
    pub direction: CutDirection,
    /// Optymalizacja kolejności 2-opt po heurystyce najbliższego sąsiada (REQ-08-007).
    #[serde(default = "yes")]
    pub two_opt: bool,
    /// Objazd wyciętych już części (REQ-08-008).
    #[serde(default)]
    pub avoid_cut_parts: bool,
    /// Usuwanie nakładających się linii (REQ-08-011).
    #[serde(default)]
    pub remove_overlaps: bool,
    /// Redukcja zmian kierunku: scalanie współliniowych odcinków (REQ-08-012).
    #[serde(default = "yes")]
    pub reduce_direction_changes: bool,
    /// Strefy zakazane (REQ-08-009/010).
    #[serde(default)]
    pub no_go_zones: Vec<NoGoZone>,
    /// Szerokość pasa rastra, mm; 0 = bez pasów (REQ-08-015).
    #[serde(default)]
    pub raster_band: f64,
    /// Kinematyka maszyny (do czasu profili urządzeń z M2.3).
    #[serde(default)]
    pub kinematics: Kinematics,
    /// Pozycja głowicy przed zadaniem, mm.
    #[serde(default)]
    pub start: [f64; 2],
}

impl Default for PlanSettings {
    fn default() -> Self {
        serde_json::from_value(serde_json::json!({})).expect("domyślne ustawienia planu")
    }
}

impl PlanSettings {
    pub fn validate(&self) -> Result<(), String> {
        self.kinematics.validate()?;
        for (i, z) in self.no_go_zones.iter().enumerate() {
            let [x0, y0, x1, y1] = z.rect;
            if !(z.rect.iter().all(|v| v.is_finite()) && x1 > x0 && y1 > y0) {
                return Err(format!(
                    "no_go_zones[{i}].rect: [x0, y0, x1, y1], x1 > x0, y1 > y0"
                ));
            }
        }
        if !(self.raster_band >= 0.0 && self.raster_band.is_finite()) {
            return Err("raster_band: ≥ 0".into());
        }
        if !self.start.iter().all(|v| v.is_finite()) {
            return Err("start: liczby skończone".into());
        }
        Ok(())
    }
}

/// Mostek ręczny (REQ-04-083): punkt na konturze we współrzędnych lokalnych obiektu.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Tab {
    pub at: [f64; 2],
    /// Długość przerwy, mm; `None` = `tabs.size` warstwy.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<f64>,
}

fn yes() -> bool {
    true
}
fn one() -> f64 {
    1.0
}
fn d_lead_len() -> f64 {
    2.0
}
fn d_lead_angle() -> f64 {
    90.0
}
fn d_tab_size() -> f64 {
    0.5
}
fn d_tab_spacing() -> f64 {
    50.0
}
fn d_interval() -> f64 {
    0.1
}
fn d_vmax() -> f64 {
    100.0
}
fn d_accel() -> f64 {
    1000.0
}
fn d_junction() -> f64 {
    0.01
}

/// Makro / szablon G-code: tekst wielowierszowy bez znaków sterujących (poza nową linią i tabulatorem).
pub fn check_macro(name: &str, text: &str) -> Result<(), String> {
    if text.len() > 65_536 {
        return Err(format!("{name}: najwyżej 64 KiB"));
    }
    if text
        .chars()
        .any(|c| c.is_control() && c != '\n' && c != '\r' && c != '\t')
    {
        return Err(format!("{name}: znaki sterujące niedozwolone"));
    }
    Ok(())
}

/// Skąd liczone są współrzędne zadania (§10 „Start From”, REQ-10-006).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum StartFrom {
    /// Współrzędne obszaru roboczego (maszyny).
    #[default]
    Absolute,
    /// Względem bieżącej pozycji głowicy (origin zadania w miejscu głowicy).
    Current,
    /// Względem zera użytkownika (układ roboczy ustawiony „ustaw zero”).
    UserOrigin,
}

/// Punkt ramki zadania, który trafia w punkt startu (9-punktowy origin, §10).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum JobOrigin {
    TopLeft,
    Top,
    TopRight,
    Left,
    Center,
    Right,
    #[default]
    BottomLeft,
    Bottom,
    BottomRight,
}

impl JobOrigin {
    /// Udział szerokości i wysokości ramki (0, 0,5, 1) od lewego dolnego rogu.
    pub fn fractions(self) -> (f64, f64) {
        use JobOrigin::*;
        match self {
            TopLeft => (0.0, 1.0),
            Top => (0.5, 1.0),
            TopRight => (1.0, 1.0),
            Left => (0.0, 0.5),
            Center => (0.5, 0.5),
            Right => (1.0, 0.5),
            BottomLeft => (0.0, 0.0),
            Bottom => (0.5, 0.0),
            BottomRight => (1.0, 0.0),
        }
    }
}

/// Ustawienia startu zadania zapisywane w dokumencie (REQ-10-006, schemat 5).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct StartSettings {
    #[serde(default)]
    pub from: StartFrom,
    /// Origin zadania (dla `current` i `user_origin`).
    #[serde(default)]
    pub origin: JobOrigin,
}

/// Jednostki pliku G-code (blok G20/G21, REQ-13-106).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum GcodeUnits {
    #[default]
    Mm,
    Inch,
}

/// Kod włączenia air assist (REQ-13-090).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum AirCode {
    /// M7 (mist).
    M7,
    /// M8 (flood).
    #[default]
    M8,
}

/// Drugi laser (REQ-13-089): wybór narzędzia i offset względem lasera 1.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Laser2Settings {
    /// Warstwy z `power2_max` są wypalane także laserem 2.
    #[serde(default)]
    pub enabled: bool,
    /// Słowo wyboru lasera 1 (np. `T0`).
    #[serde(default = "d_t0")]
    pub select1: String,
    /// Słowo wyboru lasera 2 (np. `T1`).
    #[serde(default = "d_t1")]
    pub select2: String,
    /// Położenie plamki lasera 2 względem lasera 1, mm.
    #[serde(default)]
    pub offset: [f64; 2],
}

impl Default for Laser2Settings {
    fn default() -> Self {
        serde_json::from_value(serde_json::json!({})).expect("domyślny laser 2")
    }
}

/// Ustawienia kompilatora G-code (ADR 0011, schemat 5). Do czasu profili urządzeń (M2.3)
/// są zapisane w dokumencie.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GcodeSettings {
    /// Szablon nagłówka (REQ-13-105): `{name}`, `{driver}`, `{time}`, `{time_s}`, `{units}`,
    /// `{s_max}`, `{passes}`, `{x0}`, `{y0}`, `{x1}`, `{y1}`.
    #[serde(default = "d_header")]
    pub header: String,
    /// Szablon stopki (REQ-13-105), te same pola.
    #[serde(default = "d_footer")]
    pub footer: String,
    #[serde(default)]
    pub units: GcodeUnits,
    /// Blok jednostek (REQ-13-106); pusty = `G90` i `G21`/`G20` wg `units`.
    #[serde(default)]
    pub units_block: String,
    /// Makro przed zadaniem (REQ-13-108).
    #[serde(default)]
    pub job_pre: String,
    /// Makro po zadaniu (REQ-13-108).
    #[serde(default)]
    pub job_post: String,
    /// Wartość S dla 100 % mocy (GRBL `$30`).
    #[serde(default = "d_s_max")]
    pub s_max: f64,
    /// Miejsca po przecinku współrzędnych.
    #[serde(default = "d_decimals")]
    pub decimals: u8,
    /// Kompresja: pomijanie powtórzeń i skrócone liczby (REQ-13-086).
    #[serde(default = "yes")]
    pub compress: bool,
    /// Postęp M73 (REQ-13-088).
    #[serde(default)]
    pub progress_m73: bool,
    #[serde(default)]
    pub air_code: AirCode,
    #[serde(default)]
    pub laser2: Laser2Settings,
    /// Oś Z: ruchy Z warstw (offset, krok) i podniesienie przy przejazdach.
    #[serde(default)]
    pub use_z: bool,
    /// Podniesienie Z przy przejeździe z `lift`, mm.
    #[serde(default = "d_z_lift")]
    pub z_lift: f64,
    /// Powrót do punktu startu na końcu zadania.
    #[serde(default = "yes")]
    pub return_to_start: bool,
}

impl Default for GcodeSettings {
    fn default() -> Self {
        serde_json::from_value(serde_json::json!({})).expect("domyślne ustawienia G-code")
    }
}

impl GcodeSettings {
    pub fn validate(&self) -> Result<(), String> {
        if !(self.s_max > 0.0 && self.s_max.is_finite() && self.s_max <= 1e6) {
            return Err("s_max: > 0".into());
        }
        if self.decimals > 6 {
            return Err("decimals: 0–6".into());
        }
        if !(self.z_lift >= 0.0 && self.z_lift.is_finite()) {
            return Err("z_lift: ≥ 0".into());
        }
        if !self.laser2.offset.iter().all(|v| v.is_finite()) {
            return Err("laser2.offset: liczby skończone".into());
        }
        for (n, w) in [
            ("laser2.select1", &self.laser2.select1),
            ("laser2.select2", &self.laser2.select2),
        ] {
            if w.contains('\n') || w.contains('\r') {
                return Err(format!("{n}: jeden wiersz"));
            }
        }
        for (n, t) in [
            ("header", &self.header),
            ("footer", &self.footer),
            ("units_block", &self.units_block),
            ("job_pre", &self.job_pre),
            ("job_post", &self.job_post),
            ("laser2.select1", &self.laser2.select1),
            ("laser2.select2", &self.laser2.select2),
        ] {
            check_macro(n, t)?;
        }
        Ok(())
    }
}

fn d_header() -> String {
    "; BurnCraft G-code ({driver}): {name}\n; Estimated time: {time}".into()
}
fn d_footer() -> String {
    "M2".into()
}
fn d_s_max() -> f64 {
    1000.0
}
fn d_decimals() -> u8 {
    3
}
fn d_z_lift() -> f64 {
    3.0
}
fn d_t0() -> String {
    "T0".into()
}
fn d_t1() -> String {
    "T1".into()
}
