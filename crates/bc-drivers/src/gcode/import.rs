//! Import programu G-code (REQ-02-089): program → zadanie (podgląd ścieżek, czas,
//! długości) i raport. Treść programu zostaje bez zmian do uruchomienia (M2.3).

use bc_doc::{Kinematics, LayerMode};
use bc_geom::Point;
use bc_plan::{Job, Move, Op, OpKind, OpSettings};
use serde::Serialize;

use super::sim::{self, Event, SimError, SimOptions, Spindle};

/// Opcje importu.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ImportOptions {
    pub sim: SimOptions,
    /// Kinematyka do czasu szacowanego.
    pub kinematics: Kinematics,
}

/// Raport importu.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ImportReport {
    /// Wiersze pliku.
    pub lines: usize,
    /// Ruchy (po spłaszczeniu łuków).
    pub moves: usize,
    /// Długość z wiązką, mm.
    pub cut_length: f64,
    /// Długość przejazdów i ruchów bez wiązki, mm.
    pub travel_length: f64,
    pub bbox: Option<[f64; 4]>,
    pub time_s: f64,
    /// hh:mm:ss
    pub formatted_time: String,
    /// Pauzy M0/M1 (z komentarzem).
    pub pauses: Vec<String>,
    /// Użyte narzędzia (lasery).
    pub tools: Vec<u32>,
    /// Program zawiera M4 (moc dynamiczna).
    pub dynamic_power: bool,
    pub air_assist: bool,
    pub warnings: Vec<String>,
}

/// Zaimportowany program.
#[derive(Debug, Clone, PartialEq)]
pub struct Imported {
    pub job: Job,
    pub run: sim::Run,
    pub report: ImportReport,
}

fn hms(s: f64) -> String {
    let t = s.max(0.0).round() as u64;
    format!("{:02}:{:02}:{:02}", t / 3600, t / 60 % 60, t % 60)
}

/// Wykonuje program interpreterem i buduje zadanie (jedna operacja na narzędzie).
pub fn import(text: &str, o: &ImportOptions) -> Result<Imported, SimError> {
    let run = sim::run(text, &o.sim)?;
    let pct = |s: f64| ((s / o.sim.s_max) * 100.0).clamp(0.0, 100.0);
    let mut ops: Vec<Op> = Vec::new();
    let mut cur_tool: Option<u32> = None;
    let mut pauses = Vec::new();
    let mut dynamic = false;
    let mut air = false;
    let mut tools: Vec<u32> = Vec::new();
    let mut pending_pause: Option<String> = None;
    let new_op = |tool: u32, pause: Option<String>| Op {
        layer: 0,
        label: if tool == 0 {
            "G-code".into()
        } else {
            format!("G-code T{tool}")
        },
        sublayer: 0,
        pass: 0,
        kind: OpKind::Vector,
        mode: LayerMode::Line,
        show: true,
        settings: OpSettings {
            speed: 0.0,
            power_max: 0.0,
            power_min: 0.0,
            power2_max: None,
            power2_min: None,
            dynamic_power: false,
            pwm_hz: None,
            air_assist: false,
        },
        pause,
        delay_s: 0.0,
        z: 0.0,
        objects: Vec::new(),
        moves: Vec::new(),
    };
    let mut first = true;
    for ev in &run.events {
        match ev {
            Event::Pause { comment, .. } => {
                let c = comment.clone().unwrap_or_else(|| "M0".into());
                pauses.push(c.clone());
                pending_pause = Some(c);
            }
            Event::Move(m) => {
                if cur_tool != Some(m.tool) || pending_pause.is_some() || ops.is_empty() {
                    let mut op = new_op(m.tool, pending_pause.take());
                    if first {
                        op.z = m.z;
                        first = false;
                    }
                    ops.push(op);
                    cur_tool = Some(m.tool);
                    if !tools.contains(&m.tool) {
                        tools.push(m.tool);
                    }
                }
                let op = ops.last_mut().expect("operacja");
                dynamic |= m.spindle == Spindle::M4 && m.on;
                air |= m.air.any() && m.on;
                if m.rapid {
                    op.moves.push(Move::Travel {
                        to: m.to,
                        lift: false,
                    });
                } else {
                    let speed = m.feed / 60.0;
                    let power = if m.on { pct(m.s) } else { 0.0 };
                    op.settings.speed = op.settings.speed.max(speed);
                    op.settings.power_max = op.settings.power_max.max(power);
                    op.settings.dynamic_power |= m.spindle == Spindle::M4;
                    op.settings.air_assist |= m.air.any();
                    op.moves.push(Move::Cut {
                        to: m.to,
                        power,
                        speed,
                    });
                }
            }
            Event::Dwell { seconds, s, on, .. } => {
                if ops.is_empty() || pending_pause.is_some() {
                    ops.push(new_op(cur_tool.unwrap_or(0), pending_pause.take()));
                }
                let op = ops.last_mut().expect("operacja");
                op.moves.push(Move::Dwell {
                    s: *seconds,
                    power: if *on { pct(*s) } else { 0.0 },
                });
            }
            Event::Progress { .. } | Event::Tool { .. } | Event::End { .. } => {}
        }
    }
    ops.retain(|o| !o.moves.is_empty());
    let job = Job {
        ops,
        warnings: Vec::new(),
        start: Point::ORIGIN,
        origin: Point::ORIGIN,
    };
    let st = job.stats();
    let time_s = bc_plan::estimate(&job, &o.kinematics).total_s;
    let report = ImportReport {
        lines: run.lines,
        moves: run.moves().count(),
        cut_length: st.cut_length,
        travel_length: st.travel_length + st.idle_length,
        bbox: st.bbox,
        time_s,
        formatted_time: hms(time_s),
        pauses,
        tools,
        dynamic_power: dynamic,
        air_assist: air,
        warnings: run.warnings.clone(),
    };
    Ok(Imported { job, run, report })
}
