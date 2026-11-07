//! Efektywne ustawienia przebiegu: warstwa albo pod-warstwa (REQ-07-050).

use bc_doc::{Layer, LayerMode, LayerParams};

use crate::job::OpSettings;

/// Ustawienia jednego przebiegu warstwy (warstwa = 0, pod-warstwy 1..).
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Spec {
    pub layer: u8,
    pub sublayer: u32,
    pub label: String,
    pub mode: LayerMode,
    pub speed: f64,
    pub power_max: f64,
    pub power_min: f64,
    pub passes: u32,
    pub show: bool,
    pub air_assist: bool,
    pub params: LayerParams,
}

impl Spec {
    pub fn settings(&self) -> OpSettings {
        let p = &self.params;
        OpSettings {
            speed: self.speed,
            power_max: self.power_max,
            power_min: self.power_min,
            power2_max: p.power2_max,
            power2_min: p.power2_min,
            dynamic_power: p.dynamic_power,
            pwm_hz: p.pwm_hz,
            air_assist: self.air_assist,
        }
    }
}

/// Przebiegi warstwy: główny i pod-warstwy z wyjściem.
pub(crate) fn specs(l: &Layer) -> Vec<Spec> {
    let main = Spec {
        layer: l.index,
        sublayer: 0,
        label: l.name.clone(),
        mode: l.mode,
        speed: l.speed,
        power_max: l.power_max,
        power_min: l.power_min,
        passes: l.passes.max(1),
        show: l.show,
        air_assist: l.air_assist,
        params: l.params.clone(),
    };
    let mut out = vec![main.clone()];
    for (i, s) in l.params.sublayers.iter().enumerate() {
        if !s.output {
            continue;
        }
        let mut params = l.params.clone();
        params.sublayers.clear();
        params.pause_message = None;
        if let Some(v) = s.interval {
            params.interval = v;
        }
        if let Some(v) = s.scan_angle {
            params.scan_angle = v;
        }
        let power_max = s.power_max.unwrap_or(l.power_max);
        out.push(Spec {
            sublayer: i as u32 + 1,
            label: if s.name.is_empty() {
                format!("{} / {}", l.name, i + 1)
            } else {
                format!("{} / {}", l.name, s.name)
            },
            mode: s.mode,
            speed: s.speed.unwrap_or(l.speed),
            power_max,
            power_min: s.power_min.unwrap_or(l.power_min).min(power_max),
            passes: s.passes.unwrap_or(1).max(1),
            params,
            ..main.clone()
        });
    }
    out
}
