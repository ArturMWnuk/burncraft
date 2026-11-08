//! Sterownik Moshi MS10105 V4.x przez CH341 (REQ-13-051, 052; ADR 0014 §3). Zadanie to
//! programy wysyłane blokami: czekanie na READY, prolog (adres), pakiety danych po 32 bajty,
//! epilog (adres), czekanie na koniec. Implementuje trait `Driver` z ADR 0010.

pub mod emu;
pub mod proto;

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};

use crate::ch341::Ch341;
use crate::driver::*;
use crate::lihuiyu::egv::Steps;
use crate::lihuiyu::K40Settings;
use crate::{Context, MachineFile};
use proto::Cmd as MCmd;

/// Stół Moshi (mm). Źródło: README MeerK40t `moshi` (330 × 210 mm).
pub const BED: [f64; 2] = [330.0, 210.0];

/// Opcje połączenia.
#[derive(Debug, Clone)]
pub struct MoshiOptions {
    pub settings: K40Settings,
    pub bed: [f64; 2],
}

impl Default for MoshiOptions {
    fn default() -> Self {
        MoshiOptions {
            settings: K40Settings::default(),
            bed: BED,
        }
    }
}

enum Cmd {
    Program(Vec<u8>, mpsc::Sender<Result<(), DriverError>>),
    Job(Arc<Vec<Vec<u8>>>, JobOptions, u64),
    Pause,
    Resume,
    Stop,
    Close,
}

/// Programy zadania (każdy wysyłany osobno).
type Programs = Arc<Vec<Vec<u8>>>;

struct Shared {
    state: Mutex<MachineState>,
    job: Mutex<JobState>,
    console: Mutex<VecDeque<String>>,
    alive: AtomicBool,
    queue: Mutex<VecDeque<(Programs, JobOptions)>>,
    pos: Mutex<(i64, i64)>,
}

impl Shared {
    fn log(&self, s: String) {
        let mut c = self.console.lock().expect("konsola");
        c.push_back(s);
        while c.len() > 500 {
            c.pop_front();
        }
    }
}

struct Io {
    dev: Box<dyn Ch341>,
    shared: Arc<Shared>,
    rx: mpsc::Receiver<Cmd>,
    paused: bool,
    stop: bool,
    close: bool,
}

/// Pozycja końcowa programu (kroki).
fn end_of(prog: &[u8]) -> Option<(i64, i64)> {
    let mut at: Option<(i64, i64)> = None;
    for c in proto::parse(prog).ok()? {
        let cur = at.unwrap_or((0, 0));
        at = Some(match c {
            MCmd::MoveAbs { x, y } | MCmd::CutAbs { x, y } => (x as i64, y as i64),
            MCmd::MoveH(x) | MCmd::CutH(x) => (x as i64, cur.1),
            MCmd::MoveV(y) | MCmd::CutV(y) => (cur.0, y as i64),
            _ => cur,
        });
    }
    at
}

impl Io {
    fn status(&mut self) -> Result<u8, DriverError> {
        Ok(self.dev.status().map_err(|e| DriverError::Transport(e.0))?[1])
    }

    /// Obsługa poleceń w trakcie wysyłki (pauza, stop).
    fn poll_cmds(&mut self, later: &mut Vec<Cmd>) {
        while let Ok(c) = self.rx.try_recv() {
            match c {
                Cmd::Pause => {
                    self.paused = true;
                    self.shared.state.lock().expect("stan").state = RunState::Hold;
                    self.shared.job.lock().expect("zadanie").status = JobStatus::Paused;
                }
                Cmd::Resume => {
                    self.paused = false;
                    self.shared.state.lock().expect("stan").state = RunState::Run;
                    self.shared.job.lock().expect("zadanie").status = JobStatus::Running;
                }
                Cmd::Stop => self.stop = true,
                Cmd::Close => {
                    self.stop = true;
                    self.close = true;
                }
                other => later.push(other),
            }
        }
    }

    /// Czeka na stan READY (205) przed programem. Źródło: MeerK40t `moshi/controller.py`.
    fn wait_ready(&mut self, later: &mut Vec<Cmd>) -> Result<(), DriverError> {
        let t0 = Instant::now();
        loop {
            self.poll_cmds(later);
            if self.stop {
                return Ok(());
            }
            match self.status()? {
                proto::STATUS_READY => return Ok(()),
                proto::STATUS_ERROR => {
                    return Err(DriverError::Machine(
                        proto::STATUS_ERROR as u32,
                        "płyta Moshi zgłasza błąd".into(),
                    ))
                }
                0 => return Err(DriverError::Transport("brak stanu płyty Moshi".into())),
                _ => {}
            }
            if t0.elapsed() > Duration::from_secs(600) {
                return Err(DriverError::Timeout("płyta Moshi zajęta".into()));
            }
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    /// Program: prolog (adres), dane po 32 bajty, epilog (adres), koniec = READY.
    fn program(&mut self, prog: &[u8], later: &mut Vec<Cmd>) -> Result<(), DriverError> {
        self.wait_ready(later)?;
        if self.stop {
            return Ok(());
        }
        let tr = |e: crate::transport::TransportError| DriverError::Transport(e.0);
        self.dev
            .write_addr(&[proto::opcode(proto::CMD_PROLOGUE, 0)])
            .map_err(tr)?;
        for chunk in prog.chunks(proto::PACKET) {
            self.poll_cmds(later);
            while self.paused && !self.stop {
                std::thread::sleep(Duration::from_millis(2));
                self.poll_cmds(later);
            }
            if self.stop {
                return Ok(());
            }
            self.dev.write_data(chunk).map_err(tr)?;
            let mut j = self.shared.job.lock().expect("zadanie");
            j.sent += 1;
        }
        self.dev
            .write_addr(&[proto::opcode(proto::CMD_EPILOGUE, 0)])
            .map_err(tr)?;
        self.shared.log(format!("> program Moshi {} B", prog.len()));
        self.wait_ready(later)?;
        if let Some(p) = end_of(prog) {
            *self.shared.pos.lock().expect("pozycja") = p;
        }
        Ok(())
    }

    fn run_job(&mut self, progs: Arc<Vec<Vec<u8>>>, o: JobOptions, id: u64, later: &mut Vec<Cmd>) {
        let total: usize = progs.iter().map(|p| p.len().div_ceil(proto::PACKET)).sum();
        *self.shared.job.lock().expect("zadanie") = JobState {
            id,
            name: o.name.clone(),
            status: JobStatus::Running,
            total,
            round: 1,
            repeat: o.repeat.max(1),
            ..Default::default()
        };
        self.shared.state.lock().expect("stan").state = RunState::Run;
        let t0 = Instant::now();
        let mut result = Ok(());
        'rounds: for round in 1..=o.repeat.max(1) {
            {
                let mut j = self.shared.job.lock().expect("zadanie");
                j.round = round;
                j.sent = 0;
            }
            for p in progs.iter() {
                if let Err(e) = self.program(p, later) {
                    result = Err(e);
                    break 'rounds;
                }
                if self.stop {
                    break 'rounds;
                }
                let mut j = self.shared.job.lock().expect("zadanie");
                j.acked = j.sent;
                j.elapsed_s = t0.elapsed().as_secs_f64();
            }
            if round < o.repeat && o.delay_s > 0.0 {
                std::thread::sleep(Duration::from_secs_f64(o.delay_s));
            }
        }
        let st = if self.stop {
            JobStatus::Stopped
        } else if let Err(e) = &result {
            self.shared.log(format!("! {e}"));
            self.shared
                .job
                .lock()
                .expect("zadanie")
                .errors
                .push((0, 0, e.to_string()));
            JobStatus::Failed
        } else {
            JobStatus::Done
        };
        {
            let mut j = self.shared.job.lock().expect("zadanie");
            j.status = st.clone();
            j.elapsed_s = t0.elapsed().as_secs_f64();
        }
        self.shared.state.lock().expect("stan").state = RunState::Idle;
        self.paused = false;
        if st != JobStatus::Done {
            self.shared.queue.lock().expect("kolejka").clear();
        }
    }

    fn estop(&mut self) {
        // UNVERIFIED: zatrzymanie poleceniem 1 przez zapis adresu (MeerK40t moshi: estop)
        let _ = self.dev.write_addr(&[proto::opcode(proto::CMD_ESTOP, 0)]);
        self.shared.log("> estop".into());
    }

    fn run(mut self) {
        let mut later: Vec<Cmd> = Vec::new();
        loop {
            let cmd = if !later.is_empty() {
                Some(later.remove(0))
            } else {
                match self.rx.recv_timeout(Duration::from_millis(5)) {
                    Ok(c) => Some(c),
                    Err(mpsc::RecvTimeoutError::Timeout) => None,
                    Err(_) => break,
                }
            };
            match cmd {
                Some(Cmd::Close) => break,
                Some(Cmd::Program(p, reply)) => {
                    self.stop = false;
                    let r = self.program(&p, &mut later);
                    let _ = reply.send(r);
                }
                Some(Cmd::Job(p, o, id)) => {
                    self.stop = false;
                    self.run_job(p, o, id, &mut later);
                    if self.stop {
                        self.estop();
                    }
                    while !self.stop {
                        let next = self.shared.queue.lock().expect("kolejka").pop_front();
                        let Some((p, o)) = next else { break };
                        let id = self.shared.job.lock().expect("zadanie").id + 1;
                        self.run_job(p, o, id, &mut later);
                    }
                }
                Some(Cmd::Stop) => self.estop(),
                Some(Cmd::Pause) | Some(Cmd::Resume) => {}
                None => {}
            }
            if self.close {
                break;
            }
        }
        self.shared.alive.store(false, Ordering::SeqCst);
    }
}

/// Sterownik Moshi.
pub struct MoshiDriver {
    name: String,
    opts: MoshiOptions,
    shared: Arc<Shared>,
    tx: mpsc::Sender<Cmd>,
    thread: Option<std::thread::JoinHandle<()>>,
    next_id: u64,
    wco: [f64; 2],
}

impl MoshiDriver {
    pub fn connect(
        name: &str,
        mut dev: Box<dyn Ch341>,
        opts: MoshiOptions,
    ) -> Result<Self, DriverError> {
        opts.settings.validate().map_err(DriverError::Invalid)?;
        let s = dev.status().map_err(|e| DriverError::Transport(e.0))?;
        if s[1] == 0 {
            return Err(DriverError::Transport(
                "płyta Moshi nie odpowiada (brak stanu)".into(),
            ));
        }
        let shared = Arc::new(Shared {
            state: Mutex::new(MachineState {
                state: RunState::Idle,
                variant: Some("ms10105".into()),
                version: Some("Moshi MS10105 V4.x".into()),
                ..Default::default()
            }),
            job: Mutex::new(JobState::default()),
            console: Mutex::new(VecDeque::new()),
            alive: AtomicBool::new(true),
            queue: Mutex::new(VecDeque::new()),
            pos: Mutex::new((0, 0)),
        });
        shared.log(format!("< stan {}", s[1]));
        let (tx, rx) = mpsc::channel();
        let io = Io {
            dev,
            shared: shared.clone(),
            rx,
            paused: false,
            stop: false,
            close: false,
        };
        let thread = std::thread::spawn(move || io.run());
        Ok(MoshiDriver {
            name: name.into(),
            opts,
            shared,
            tx,
            thread: Some(thread),
            next_id: 1,
            wco: [0.0; 2],
        })
    }

    fn steps(&self) -> Steps {
        Steps::new(&self.opts.settings, self.opts.bed[1])
    }

    fn alive(&self) -> Result<(), DriverError> {
        if self.shared.alive.load(Ordering::SeqCst) {
            Ok(())
        } else {
            Err(DriverError::NotConnected)
        }
    }

    fn not_running(&self) -> Result<(), DriverError> {
        if matches!(
            self.shared.job.lock().expect("zadanie").status,
            JobStatus::Running | JobStatus::Paused
        ) {
            return Err(DriverError::Busy("zadanie w toku".into()));
        }
        Ok(())
    }

    fn mpos(&self) -> bc_geom::Point {
        let p = *self.shared.pos.lock().expect("pozycja");
        self.steps().mm((p.0 as f64, p.1 as f64))
    }

    /// Program z przejazdami (bez wiązki) po punktach (kroki).
    fn moves_program(&self, pts: &[(i64, i64)], speed: Option<f64>) -> Vec<u8> {
        let c = |v: i64| v.clamp(0, i16::MAX as i64) as i16;
        // przejazdy bez wiązki z prędkością „normal” (ramka: prędkość ramki)
        let v = proto::vector_speed_byte(speed.unwrap_or(proto::RAPID_SPEED));
        let mut p = MCmd::VectorSpeed { cut: v, normal: v }.bytes();
        p.extend(MCmd::Offset { z: 0, x: 0, y: 0 }.bytes());
        for q in pts {
            p.extend(
                MCmd::MoveAbs {
                    x: c(q.0),
                    y: c(q.1),
                }
                .bytes(),
            );
        }
        for _ in 0..proto::TERMINATORS {
            p.extend(MCmd::Terminate.bytes());
        }
        p
    }

    fn send_program(&self, prog: Vec<u8>) -> Result<(), DriverError> {
        self.alive()?;
        let (rtx, rrx) = mpsc::channel();
        self.tx
            .send(Cmd::Program(prog, rtx))
            .map_err(|_| DriverError::NotConnected)?;
        rrx.recv_timeout(Duration::from_secs(600))
            .map_err(|_| DriverError::Timeout("płyta Moshi".into()))?
    }

    pub fn wait_job(&self, timeout: Duration) -> JobState {
        let t0 = Instant::now();
        loop {
            let j = self.job();
            if !matches!(j.status, JobStatus::Running | JobStatus::Paused) || t0.elapsed() > timeout
            {
                return j;
            }
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    fn file_programs(file: &MachineFile) -> Result<Arc<Vec<Vec<u8>>>, DriverError> {
        if file.driver != "moshi" {
            return Err(DriverError::Invalid(format!(
                "plik {} nie jest dla sterownika moshi",
                file.driver
            )));
        }
        proto::programs(&file.bytes)
            .map(Arc::new)
            .map_err(DriverError::Invalid)
    }
}

impl Drop for MoshiDriver {
    fn drop(&mut self) {
        self.disconnect();
    }
}

impl Driver for MoshiDriver {
    fn name(&self) -> &str {
        &self.name
    }

    fn capabilities(&self) -> Caps {
        Caps {
            axes: vec!["x".into(), "y".into()],
            rotary: false,
            z: false,
            native_raster: true,
            files: false,
            air_assist: false,
            dual_laser: false,
            camera: false,
            probe: false,
            lid_sensor: false,
            overrides: false,
            jog: false,
            pwm: false,
        }
    }

    fn status(&self) -> MachineState {
        let mut s = self.shared.state.lock().expect("stan").clone();
        let p = self.mpos();
        s.mpos = [p.x, p.y, 0.0];
        s.wco = [self.wco[0], self.wco[1], 0.0];
        s.wpos = [p.x - self.wco[0], p.y - self.wco[1], 0.0];
        if !self.shared.alive.load(Ordering::SeqCst) {
            s.state = RunState::Disconnected;
        }
        s
    }

    fn disconnect(&mut self) {
        let _ = self.tx.send(Cmd::Close);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
        self.shared.alive.store(false, Ordering::SeqCst);
    }

    /// Moshi nie ma znanego polecenia bazowania — przejazd do narożnika (0, 0).
    /// UNVERIFIED: brak publicznego opisu bazowania MS10105.
    fn home(&mut self) -> Result<(), DriverError> {
        self.not_running()?;
        let p = self.moves_program(&[(0, 0)], None);
        self.send_program(p)
    }

    /// Zwolnienie silników (polecenie 1 przez zapis adresu). Źródło: MeerK40t (freemotor).
    fn unlock(&mut self) -> Result<(), DriverError> {
        self.not_running()?;
        let _ = self.tx.send(Cmd::Stop);
        Ok(())
    }

    fn jog(&mut self, axis: Axis, distance: f64, _speed: f64) -> Result<(), DriverError> {
        let p = self.mpos();
        let to = match axis {
            Axis::X => bc_geom::Point::new(p.x + distance, p.y),
            Axis::Y => bc_geom::Point::new(p.x, p.y + distance),
            Axis::Z => return Err(DriverError::Unsupported("Moshi: brak osi Z".into())),
        };
        self.not_running()?;
        let prog = self.moves_program(&[self.steps().of(to)], None);
        self.send_program(prog)
    }

    fn jog_cancel(&mut self) -> Result<(), DriverError> {
        Ok(())
    }

    fn move_to(
        &mut self,
        x: Option<f64>,
        y: Option<f64>,
        _z: Option<f64>,
        speed: Option<f64>,
    ) -> Result<(), DriverError> {
        self.not_running()?;
        let p = self.mpos();
        let to = bc_geom::Point::new(
            x.map_or(p.x, |v| v + self.wco[0]),
            y.map_or(p.y, |v| v + self.wco[1]),
        );
        let cur = *self.shared.pos.lock().expect("pozycja");
        let prog = self.moves_program(&[cur, self.steps().of(to)], speed);
        self.send_program(prog)
    }

    fn set_origin(&mut self, x: bool, y: bool, _z: bool) -> Result<(), DriverError> {
        let p = self.mpos();
        if x {
            self.wco[0] = p.x;
        }
        if y {
            self.wco[1] = p.y;
        }
        Ok(())
    }

    fn frame(
        &mut self,
        points: &[[f64; 2]],
        speed: f64,
        _pointer: Option<(&str, &str, [f64; 2])>,
    ) -> Result<(), DriverError> {
        self.not_running()?;
        if points.is_empty() {
            return Ok(());
        }
        let st = self.steps();
        let mut pts: Vec<(i64, i64)> = points
            .iter()
            .map(|p| st.of(bc_geom::Point::new(p[0] + self.wco[0], p[1] + self.wco[1])))
            .collect();
        pts.push(pts[0]);
        let prog = self.moves_program(&pts, Some(speed));
        self.send_program(prog)
    }

    fn test_fire(&mut self, _power_pct: f64, _ms: u32) -> Result<(), DriverError> {
        Err(DriverError::Unsupported(
            "Moshi: impuls testowy z programu niedostępny — użyj przycisku Test na laserze".into(),
        ))
    }

    fn send_job(&mut self, file: &MachineFile, opts: JobOptions) -> Result<JobState, DriverError> {
        self.alive()?;
        self.not_running()?;
        let progs = Self::file_programs(file)?;
        let id = self.next_id;
        self.next_id += 1;
        self.tx
            .send(Cmd::Job(progs, opts, id))
            .map_err(|_| DriverError::NotConnected)?;
        let t0 = Instant::now();
        while self.job().id != id && t0.elapsed() < Duration::from_secs(2) {
            std::thread::sleep(Duration::from_millis(1));
        }
        Ok(self.job())
    }

    fn queue_job(&mut self, file: &MachineFile, opts: JobOptions) -> Result<usize, DriverError> {
        self.alive()?;
        let progs = Self::file_programs(file)?;
        if matches!(self.job().status, JobStatus::Running | JobStatus::Paused) {
            let mut q = self.shared.queue.lock().expect("kolejka");
            q.push_back((progs, opts));
            self.next_id += 1;
            return Ok(q.len());
        }
        self.send_job(file, opts)?;
        Ok(0)
    }

    fn job(&self) -> JobState {
        let mut j = self.shared.job.lock().expect("zadanie").clone();
        j.queued = self.shared.queue.lock().expect("kolejka").len();
        j
    }

    /// Pauza wstrzymuje wysyłkę kolejnych bloków (UNVERIFIED: płyta kończy przyjęte dane).
    fn pause(&mut self) -> Result<(), DriverError> {
        self.alive()?;
        self.tx
            .send(Cmd::Pause)
            .map_err(|_| DriverError::NotConnected)
    }

    fn resume(&mut self) -> Result<(), DriverError> {
        self.alive()?;
        self.tx
            .send(Cmd::Resume)
            .map_err(|_| DriverError::NotConnected)
    }

    fn stop(&mut self) {
        let _ = self.tx.send(Cmd::Stop);
    }

    fn set_overrides(
        &mut self,
        _feed: Option<u32>,
        _power: Option<u32>,
    ) -> Result<(), DriverError> {
        Err(DriverError::Unsupported("Moshi: brak nadpisań".into()))
    }

    fn read_settings(&mut self) -> Result<Vec<Setting>, DriverError> {
        let s = &self.opts.settings;
        let set = |id: &str, value: String, name: &str| Setting {
            id: id.into(),
            value,
            name: Some(name.into()),
            unit: None,
            min: None,
            max: None,
        };
        Ok(vec![
            set("dpi", s.dpi.to_string(), "Kroki na cal"),
            set("scale_x", s.scale[0].to_string(), "Korekcja skali X"),
            set("scale_y", s.scale[1].to_string(), "Korekcja skali Y"),
        ])
    }

    fn write_settings(
        &mut self,
        _changes: &[(String, String)],
        _backup_dir: Option<&std::path::Path>,
        _dry_run: bool,
    ) -> Result<WriteReport, DriverError> {
        Err(DriverError::Unsupported(
            "ustawienia Moshi są w profilu urządzenia (machine.profile_save)".into(),
        ))
    }

    fn probe_z(&mut self, _distance: f64, _feed: f64) -> Result<f64, DriverError> {
        Err(DriverError::Unsupported("Moshi: brak sondy Z".into()))
    }

    fn list_files(&mut self) -> Result<Vec<RemoteFile>, DriverError> {
        Err(DriverError::Unsupported(
            "Moshi: brak pamięci plików".into(),
        ))
    }

    fn upload_file(&mut self, _name: &str, _data: &[u8]) -> Result<(), DriverError> {
        Err(DriverError::Unsupported(
            "Moshi: brak pamięci plików".into(),
        ))
    }

    fn run_file(&mut self, _name: &str) -> Result<(), DriverError> {
        Err(DriverError::Unsupported(
            "Moshi: brak pamięci plików".into(),
        ))
    }

    fn delete_file(&mut self, _name: &str) -> Result<(), DriverError> {
        Err(DriverError::Unsupported(
            "Moshi: brak pamięci plików".into(),
        ))
    }

    fn raw(&mut self, _line: &str) -> Result<Vec<String>, DriverError> {
        Err(DriverError::Unsupported(
            "Moshi: protokół binarny — brak surowych poleceń tekstowych".into(),
        ))
    }

    fn console(&self) -> Vec<String> {
        self.shared
            .console
            .lock()
            .expect("konsola")
            .iter()
            .cloned()
            .collect()
    }

    fn compile(&self, job: &bc_plan::Job, ctx: &Context) -> Result<MachineFile, DriverError> {
        let mut c = ctx.clone();
        c.k40 = self.opts.settings.clone();
        c.bed = self.opts.bed;
        Ok(crate::compile("moshi", job, &c)?)
    }
}
