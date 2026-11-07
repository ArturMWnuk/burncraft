//! Sterownik GRBL (ADR 0010 §1, §3): wątek połączenia jest właścicielem transportu i
//! silnikiem protokołu — strumieniowanie character counting / send-response, zapytania
//! `?` w tle, kolejka zadań z powtórzeniami, bajty czasu rzeczywistego poza kolejką.

pub mod codes;

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};

use crate::driver::*;
use crate::transport::Transport;
use crate::{Context, MachineFile};
use codes::Variant;

/// Opcje połączenia GRBL.
#[derive(Debug, Clone)]
pub struct GrblOptions {
    pub stream: StreamMode,
    /// Odstęp zapytań `?`, ms (REQ-13-077).
    pub poll_ms: u64,
    /// `None` = `s_max` odmiany.
    pub s_max: Option<f64>,
    /// Wymuszona odmiana (bez rozpoznania).
    pub variant: Option<String>,
    /// Limit czasu odpowiedzi na polecenie.
    pub reply_timeout: Duration,
}

impl Default for GrblOptions {
    fn default() -> Self {
        GrblOptions {
            stream: StreamMode::CharacterCounting,
            poll_ms: 200,
            s_max: None,
            variant: None,
            reply_timeout: Duration::from_secs(15),
        }
    }
}

type Reply = mpsc::Sender<Result<Vec<String>, DriverError>>;

enum Cmd {
    Line(String, Reply, bool),
    Job(Arc<Vec<String>>, JobOptions, u64, bool),
    Upload(String, Vec<u8>, Reply),
    Close,
}

struct Shared {
    state: Mutex<MachineState>,
    job: Mutex<JobState>,
    console: Mutex<VecDeque<String>>,
    rt: Mutex<Vec<u8>>,
    alive: AtomicBool,
    next_id: AtomicU64,
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

/// Sterownik GRBL i odmian.
pub struct GrblDriver {
    name: String,
    variant: &'static Variant,
    opts: GrblOptions,
    tx: mpsc::Sender<Cmd>,
    shared: Arc<Shared>,
    thread: Option<std::thread::JoinHandle<()>>,
}

const RX_LIMIT: usize = 127;

fn parse3(s: &str) -> Option<[f64; 3]> {
    let v: Vec<f64> = s.split(',').filter_map(|x| x.trim().parse().ok()).collect();
    match v.len() {
        2 => Some([v[0], v[1], 0.0]),
        n if n >= 3 => Some([v[0], v[1], v[2]]),
        _ => None,
    }
}

/// Raport stanu GRBL 1.1 (`<Idle|MPos:…|…>`) albo 0.9 (`<Idle,MPos:…,WPos:…>`).
pub fn parse_status(line: &str, st: &mut MachineState) -> bool {
    let Some(body) = line.strip_prefix('<').and_then(|l| l.strip_suffix('>')) else {
        return false;
    };
    let (state, rest): (&str, Vec<String>) = if body.contains('|') {
        let mut it = body.split('|');
        let s = it.next().unwrap_or("");
        (s, it.map(str::to_string).collect())
    } else {
        // 0.9: <Idle,MPos:x,y,z,WPos:x,y,z>
        let (s, r) = body.split_once(',').unwrap_or((body, ""));
        let mut fields = Vec::new();
        for part in ["MPos:", "WPos:"] {
            if let Some(i) = r.find(part) {
                let tail = &r[i..];
                let end = tail[part.len()..]
                    .find(|c: char| c.is_ascii_alphabetic())
                    .map(|e| e + part.len())
                    .unwrap_or(tail.len());
                fields.push(tail[..end].trim_end_matches(',').to_string());
            }
        }
        (s, fields)
    };
    let name = state.split(':').next().unwrap_or("");
    st.state = match name {
        "Idle" => RunState::Idle,
        "Run" => RunState::Run,
        "Hold" => RunState::Hold,
        "Jog" => RunState::Jog,
        "Alarm" => RunState::Alarm,
        "Door" => RunState::Door,
        "Home" => RunState::Home,
        "Check" => RunState::Check,
        "Sleep" => RunState::Sleep,
        _ => RunState::Unknown,
    };
    let mut mpos = None;
    let mut wpos = None;
    st.lid_open = Some(name == "Door");
    for f in &rest {
        let (k, v) = f.split_once(':').unwrap_or((f.as_str(), ""));
        match k {
            "MPos" => mpos = parse3(v),
            "WPos" => wpos = parse3(v),
            "WCO" => {
                if let Some(w) = parse3(v) {
                    st.wco = w
                }
            }
            "Bf" => {
                let b: Vec<u32> = v.split(',').filter_map(|x| x.parse().ok()).collect();
                if b.len() == 2 {
                    st.buffer = Some([b[0], b[1]]);
                }
            }
            "FS" | "F" => {
                let b: Vec<f64> = v.split(',').filter_map(|x| x.parse().ok()).collect();
                if let Some(f) = b.first() {
                    st.feed = *f;
                }
                if let Some(s) = b.get(1) {
                    st.spindle = *s;
                }
            }
            "Ov" => {
                let b: Vec<u32> = v.split(',').filter_map(|x| x.parse().ok()).collect();
                if b.len() == 3 {
                    st.overrides = [b[0], b[1], b[2]];
                }
            }
            "Pn" => st.lid_open = Some(v.contains('D') || name == "Door"),
            _ => {}
        }
    }
    match (mpos, wpos) {
        (Some(m), Some(w)) => {
            st.mpos = m;
            st.wpos = w;
            st.wco = [m[0] - w[0], m[1] - w[1], m[2] - w[2]];
        }
        (Some(m), None) => {
            st.mpos = m;
            st.wpos = [m[0] - st.wco[0], m[1] - st.wco[1], m[2] - st.wco[2]];
        }
        (None, Some(w)) => {
            st.wpos = w;
            st.mpos = [w[0] + st.wco[0], w[1] + st.wco[1], w[2] + st.wco[2]];
        }
        _ => {}
    }
    true
}

/// Wiersze zadania do wysłania (bez pustych i samych komentarzy).
pub fn job_lines(file: &MachineFile) -> Vec<String> {
    file.stream_lines()
        .map(str::trim)
        .filter(|l| {
            !l.is_empty() && !l.starts_with(';') && !(l.starts_with('(') && l.ends_with(')'))
        })
        .map(str::to_string)
        .collect()
}

/// Wynik kreatora konfiguracji lasera (REQ-13-084).
#[derive(Debug, Clone, PartialEq, serde::Serialize, schemars::JsonSchema)]
pub struct LaserCheck {
    pub ok: bool,
    /// Opisy problemów.
    pub problems: Vec<String>,
    /// Proponowane zmiany ustawień (do `write_settings` po potwierdzeniu).
    pub fixes: Vec<(String, String)>,
}

/// Kreator: tryb laserowy `$32=1` i `$30` równe `s_max` profilu.
pub fn check_laser_config(d: &mut dyn Driver, s_max: f64) -> Result<LaserCheck, DriverError> {
    let s = d.read_settings()?;
    let get = |id: &str| {
        s.iter()
            .find(|x| x.id == id)
            .and_then(|x| x.value.parse::<f64>().ok())
    };
    let mut problems = Vec::new();
    let mut fixes = Vec::new();
    match get("$32") {
        Some(1.0) => {}
        Some(_) => {
            problems.push(
                "$32=0: tryb laserowy wyłączony — wiązka świeci w przejazdach G0 i postojach"
                    .into(),
            );
            fixes.push(("$32".into(), "1".into()));
        }
        None => problems.push("brak ustawienia $32 (firmware bez trybu laserowego?)".into()),
    }
    match get("$30") {
        Some(v) if (v - s_max).abs() < 1e-6 => {}
        Some(v) => {
            problems.push(format!(
                "$30={v}: maksymalna wartość S różni się od profilu ({s_max}) — moc będzie przeskalowana"
            ));
            fixes.push(("$30".into(), crate::gcode::fmt_num(s_max, 3, true)));
        }
        None => problems.push("brak ustawienia $30".into()),
    }
    Ok(LaserCheck {
        ok: problems.is_empty(),
        problems,
        fixes,
    })
}

enum Pending {
    Job,
    Cmd(Reply, Vec<String>),
}

struct Running {
    lines: Arc<Vec<String>>,
    next: usize,
    opts: JobOptions,
    round: u32,
    started: Instant,
    delay_until: Option<Instant>,
    saw_run: bool,
    acked_all_at: Option<Instant>,
}

struct Io {
    t: Box<dyn Transport>,
    shared: Arc<Shared>,
    opts: GrblOptions,
    variant: &'static Variant,
    rx: Vec<u8>,
    pending: VecDeque<(usize, Pending)>,
    unacked: usize,
    interactive: VecDeque<(String, Reply)>,
    job: Option<Running>,
    queue: VecDeque<(Arc<Vec<String>>, JobOptions, u64)>,
    last_poll: Instant,
    /// Wysłany reset (0x18): następne powitanie jest oczekiwane.
    reset_sent: bool,
}

impl Io {
    fn write(&mut self, b: &[u8]) -> bool {
        match self.t.write(b) {
            Ok(()) => true,
            Err(e) => {
                self.shared.log(format!("! {e}"));
                self.shared.alive.store(false, Ordering::SeqCst);
                false
            }
        }
    }

    fn fail_pending(&mut self, e: DriverError) {
        for (_, p) in self.pending.drain(..) {
            if let Pending::Cmd(r, _) = p {
                let _ = r.send(Err(e.clone()));
            }
        }
        for (_, r) in self.interactive.drain(..) {
            let _ = r.send(Err(e.clone()));
        }
        self.unacked = 0;
    }

    fn end_job(&mut self, status: JobStatus) {
        if self.job.take().is_some() {
            let mut j = self.shared.job.lock().expect("zadanie");
            j.status = status.clone();
            if status != JobStatus::Done {
                self.queue.clear();
                j.queued = 0;
            }
        }
    }

    fn start_job(&mut self, lines: Arc<Vec<String>>, opts: JobOptions, id: u64) {
        let mut j = self.shared.job.lock().expect("zadanie");
        *j = JobState {
            id,
            name: opts.name.clone(),
            status: JobStatus::Running,
            total: lines.len(),
            round: 1,
            repeat: opts.repeat.max(1),
            queued: self.queue.len(),
            ..Default::default()
        };
        drop(j);
        self.job = Some(Running {
            lines,
            next: 0,
            opts,
            round: 1,
            started: Instant::now(),
            delay_until: None,
            saw_run: false,
            acked_all_at: None,
        });
    }

    fn realtime(&mut self) {
        let bytes: Vec<u8> = std::mem::take(&mut *self.shared.rt.lock().expect("rt"));
        if bytes.is_empty() {
            return;
        }
        if !self.write(&bytes) {
            return;
        }
        if bytes.contains(&0x18) {
            self.reset_sent = true;
            // reset: bufor sterownika wyczyszczony, wiersze nie zostaną potwierdzone
            self.pending.clear();
            self.unacked = 0;
            self.end_job(JobStatus::Stopped);
            self.queue.clear();
            for (_, r) in self.interactive.drain(..) {
                let _ = r.send(Err(DriverError::Busy("zatrzymano".into())));
            }
        }
    }

    fn send(&mut self) {
        let cc = self.opts.stream == StreamMode::CharacterCounting;
        let mut out: Vec<u8> = Vec::new();
        if let Some(job) = &mut self.job {
            if let Some(t) = job.delay_until {
                if Instant::now() < t {
                    return;
                }
                job.delay_until = None;
            }
            while job.next < job.lines.len() {
                let l = &job.lines[job.next];
                let need = l.len() + 1;
                let fits = if cc {
                    self.unacked + need <= RX_LIMIT || self.unacked == 0
                } else {
                    self.pending.is_empty()
                };
                if !fits {
                    break;
                }
                out.extend_from_slice(l.as_bytes());
                out.push(b'\n');
                self.unacked += need;
                self.pending.push_back((need, Pending::Job));
                job.next += 1;
            }
            let sent = job.next;
            let (round, lines) = (job.round, job.lines.clone());
            let mut j = self.shared.job.lock().expect("zadanie");
            j.sent = sent + (round as usize - 1) * lines.len();
        } else if self.pending.is_empty() {
            if let Some((l, r)) = self.interactive.pop_front() {
                let need = l.len() + 1;
                self.shared.log(format!("> {l}"));
                out.extend_from_slice(l.as_bytes());
                out.push(b'\n');
                self.unacked += need;
                self.pending.push_back((need, Pending::Cmd(r, Vec::new())));
            }
        }
        if !out.is_empty() {
            self.write(&out);
        }
    }

    fn on_line(&mut self, line: &str) {
        let line = line.trim();
        if line.is_empty() {
            return;
        }
        if line.starts_with('<') {
            let mut st = self.shared.state.lock().expect("stan");
            parse_status(line, &mut st);
            // alarm opisuje bieżący stan: znika, gdy maszyna wyszła z alarmu ($X z Konsoli,
            // bazowanie $H, reset) — test sprzętowy REQ-19-017
            if !matches!(st.state, RunState::Alarm | RunState::Unknown) {
                st.alarm = None;
            }
            let s = st.state.clone();
            drop(st);
            self.on_status(s);
            return;
        }
        self.shared.log(format!("< {line}"));
        if line == "ok" || line.starts_with("error") {
            let err = line
                .strip_prefix("error:")
                .and_then(|n| n.trim().parse::<u32>().ok());
            if let Some((len, p)) = self.pending.pop_front() {
                self.unacked = self.unacked.saturating_sub(len);
                match p {
                    Pending::Cmd(r, lines) => {
                        let _ = r.send(match err {
                            None => Ok(lines),
                            Some(c) => Err(DriverError::Machine(c, codes::error_text(c).into())),
                        });
                    }
                    Pending::Job => {
                        let mut j = self.shared.job.lock().expect("zadanie");
                        j.acked += 1;
                        if let Some(c) = err {
                            let at = j.acked;
                            j.errors.push((at, c, codes::error_text(c).into()));
                            drop(j);
                            self.shared
                                .state
                                .lock()
                                .expect("stan")
                                .errors
                                .push((c, codes::error_text(c).into()));
                            // błąd w zadaniu: zatrzymanie (bezpieczeństwo)
                            self.shared
                                .rt
                                .lock()
                                .expect("rt")
                                .extend_from_slice(&[b'!', 0x18]);
                            self.end_job(JobStatus::Failed);
                        }
                    }
                }
            }
            return;
        }
        if let Some(n) = line.strip_prefix("ALARM:") {
            let c = n.trim().parse().unwrap_or(0);
            {
                let mut st = self.shared.state.lock().expect("stan");
                st.alarm = Some((c, codes::alarm_text(c).into()));
                st.state = RunState::Alarm;
            }
            self.fail_pending(DriverError::Alarm(c, codes::alarm_text(c).into()));
            if self.job.is_some() {
                self.end_job(JobStatus::Failed);
            }
            return;
        }
        if line.starts_with("Grbl") || line.starts_with("GrblHAL") {
            if std::mem::take(&mut self.reset_sent) {
                return;
            }
            // nieoczekiwany reset sterownika
            self.fail_pending(DriverError::Busy("sterownik zresetował się".into()));
            if self.job.is_some() {
                self.end_job(JobStatus::Stopped);
            }
            return;
        }
        if let Some(m) = line.strip_prefix("[MSG:").and_then(|m| m.strip_suffix(']')) {
            self.shared.state.lock().expect("stan").message = Some(m.to_string());
        }
        if let Some((_, Pending::Cmd(_, lines))) = self.pending.front_mut() {
            lines.push(line.to_string());
        }
    }

    fn on_status(&mut self, s: RunState) {
        let Some(job) = &mut self.job else { return };
        match s {
            RunState::Run | RunState::Hold => job.saw_run = true,
            RunState::Idle => {
                let remaining = job.next < job.lines.len() || !self.pending.is_empty();
                if remaining && job.saw_run && job.delay_until.is_none() {
                    // niedopełnienie bufora: maszyna stanęła, a zadanie trwa (REQ-16-005)
                    job.saw_run = false;
                    let mut j = self.shared.job.lock().expect("zadanie");
                    j.underruns += 1;
                    if j.underruns == 1 {
                        j.warnings.push(
                            "buffer_underrun: maszyna czekała na dane — połączenie nie nadąża za zadaniem".into(),
                        );
                    }
                }
                if let Some(t) = job.acked_all_at {
                    if Instant::now() >= t {
                        job.acked_all_at = None;
                        self.round_done();
                    }
                }
            }
            _ => {}
        }
    }

    fn round_done(&mut self) {
        let Some(job) = &mut self.job else { return };
        if job.round < job.opts.repeat.max(1) {
            job.round += 1;
            job.next = 0;
            job.saw_run = false;
            job.delay_until =
                Some(Instant::now() + Duration::from_secs_f64(job.opts.delay_s.max(0.0)));
            self.shared.job.lock().expect("zadanie").round = job.round;
            return;
        }
        self.end_job(JobStatus::Done);
        if let Some((l, o, id)) = self.queue.pop_front() {
            self.start_job(l, o, id);
        }
    }

    fn check_job_acked(&mut self) {
        let Some(job) = &mut self.job else { return };
        let all = job.next >= job.lines.len()
            && !self.pending.iter().any(|(_, p)| matches!(p, Pending::Job));
        if all && job.acked_all_at.is_none() && job.delay_until.is_none() {
            // koniec rundy po potwierdzeniu i raporcie Idle wysłanym po nim
            job.acked_all_at = Some(Instant::now());
            self.last_poll = Instant::now() - Duration::from_millis(self.opts.poll_ms);
        }
        let el = job.started.elapsed().as_secs_f64();
        self.shared.job.lock().expect("zadanie").elapsed_s = el;
    }

    fn read(&mut self, timeout: Duration) {
        let mut buf = [0u8; 2048];
        match self.t.read(&mut buf, timeout) {
            Ok(0) => {}
            Ok(n) => {
                self.rx.extend_from_slice(&buf[..n]);
                while let Some(i) = self.rx.iter().position(|&b| b == b'\n') {
                    let l: Vec<u8> = self.rx.drain(..=i).collect();
                    let s = String::from_utf8_lossy(&l).to_string();
                    self.on_line(&s);
                }
            }
            Err(e) => {
                self.shared.log(format!("! {e}"));
                self.shared.alive.store(false, Ordering::SeqCst);
            }
        }
    }

    /// Bajty surowe (XMODEM): czeka na jeden z `want`.
    fn wait_byte(&mut self, want: &[u8], timeout: Duration) -> Option<u8> {
        let end = Instant::now() + timeout;
        loop {
            if let Some(i) = self.rx.iter().position(|b| want.contains(b)) {
                let b = self.rx[i];
                self.rx.drain(..=i);
                return Some(b);
            }
            if Instant::now() >= end {
                return None;
            }
            let mut buf = [0u8; 256];
            if let Ok(n) = self.t.read(&mut buf, Duration::from_millis(5)) {
                self.rx.extend_from_slice(&buf[..n]);
            }
        }
    }

    fn upload(&mut self, name: &str, data: &[u8]) -> Result<Vec<String>, DriverError> {
        let cmd = format!("$Xmodem/Receive={name}\n");
        self.shared.log(format!("> {}", cmd.trim()));
        self.write(cmd.as_bytes());
        if self.wait_byte(b"C", Duration::from_secs(3)).is_none() {
            return Err(DriverError::Timeout(
                "XMODEM: brak gotowości odbiorcy".into(),
            ));
        }
        for p in crate::xmodem::packets(data) {
            let mut tries = 0;
            loop {
                self.write(&p);
                match self.wait_byte(&[0x06, 0x15], Duration::from_secs(3)) {
                    Some(0x06) => break,
                    _ if tries < 10 => tries += 1,
                    _ => return Err(DriverError::Transport("XMODEM: pakiet odrzucony".into())),
                }
            }
        }
        self.write(&[0x04]);
        self.wait_byte(&[0x06], Duration::from_secs(3))
            .ok_or_else(|| DriverError::Timeout("XMODEM: brak potwierdzenia EOT".into()))?;
        // „ok” polecenia $Xmodem
        let end = Instant::now() + Duration::from_secs(3);
        while Instant::now() < end {
            if let Some(i) = self.rx.iter().position(|&b| b == b'\n') {
                let l: Vec<u8> = self.rx.drain(..=i).collect();
                let s = String::from_utf8_lossy(&l).trim().to_string();
                if s == "ok" {
                    return Ok(Vec::new());
                }
                continue;
            }
            let mut buf = [0u8; 256];
            if let Ok(n) = self.t.read(&mut buf, Duration::from_millis(5)) {
                self.rx.extend_from_slice(&buf[..n]);
            }
        }
        Err(DriverError::Timeout("XMODEM: brak ok".into()))
    }

    fn run(mut self, rx: mpsc::Receiver<Cmd>) {
        loop {
            if !self.shared.alive.load(Ordering::SeqCst) {
                self.fail_pending(DriverError::NotConnected);
                self.shared.state.lock().expect("stan").state = RunState::Disconnected;
                return;
            }
            self.realtime();
            loop {
                match rx.try_recv() {
                    Ok(Cmd::Line(l, r, force)) => {
                        if self.job.is_some() && !force {
                            let _ = r.send(Err(DriverError::Busy(
                                "zadanie w toku — polecenie niedostępne".into(),
                            )));
                        } else {
                            self.interactive.push_back((l, r));
                        }
                    }
                    Ok(Cmd::Job(l, o, id, queue)) => {
                        if self.job.is_some() {
                            if queue {
                                self.queue.push_back((l, o, id));
                                self.shared.job.lock().expect("zadanie").queued = self.queue.len();
                            }
                        } else {
                            self.start_job(l, o, id);
                        }
                    }
                    Ok(Cmd::Upload(name, data, r)) => {
                        let res = if self.job.is_some() || !self.pending.is_empty() {
                            Err(DriverError::Busy("urządzenie zajęte".into()))
                        } else {
                            self.upload(&name, &data)
                        };
                        let _ = r.send(res);
                    }
                    Ok(Cmd::Close) | Err(mpsc::TryRecvError::Disconnected) => {
                        self.fail_pending(DriverError::NotConnected);
                        self.shared.alive.store(false, Ordering::SeqCst);
                        self.shared.state.lock().expect("stan").state = RunState::Disconnected;
                        return;
                    }
                    Err(mpsc::TryRecvError::Empty) => break,
                }
            }
            self.send();
            if self.last_poll.elapsed() >= Duration::from_millis(self.opts.poll_ms) {
                self.last_poll = Instant::now();
                self.write(b"?");
            }
            self.read(Duration::from_millis(2));
            self.check_job_acked();
        }
    }
}

impl GrblDriver {
    /// Łączy przez transport: powitanie, `$I`, rozpoznanie odmiany, start wątku (REQ-13-002).
    pub fn connect(
        name: &str,
        mut t: Box<dyn Transport>,
        opts: GrblOptions,
    ) -> Result<GrblDriver, DriverError> {
        // powitanie po resecie płytki (serial) albo połączeniu (emulator, FluidNC)
        let mut text = String::new();
        let end = Instant::now() + Duration::from_millis(1500);
        let mut buf = [0u8; 1024];
        while Instant::now() < end && !text.contains("Grbl") {
            let n = t
                .read(&mut buf, Duration::from_millis(20))
                .map_err(|e| DriverError::Transport(e.0))?;
            text.push_str(&String::from_utf8_lossy(&buf[..n]));
        }
        t.write(b"\n$I\n")
            .map_err(|e| DriverError::Transport(e.0))?;
        let mut info = String::new();
        let end = Instant::now() + Duration::from_millis(2500);
        while Instant::now() < end && info.matches("ok").count() < 2 && !info.contains("error") {
            let n = t
                .read(&mut buf, Duration::from_millis(20))
                .map_err(|e| DriverError::Transport(e.0))?;
            info.push_str(&String::from_utf8_lossy(&buf[..n]));
        }
        if !info.contains("ok") && !text.contains("Grbl") {
            return Err(DriverError::Timeout(format!(
                "{}: brak odpowiedzi GRBL",
                t.describe()
            )));
        }
        let variant = match &opts.variant {
            Some(id) => codes::variant(id).unwrap_or_else(|| codes::detect(&text, &info)),
            None => codes::detect(&text, &info),
        };
        let version = info
            .lines()
            .find(|l| l.starts_with("[VER:") || l.starts_with("[0.") || l.starts_with("[1."))
            .map(|l| l.trim().to_string())
            .or_else(|| {
                text.lines()
                    .find(|l| l.contains("Grbl"))
                    .map(|l| l.trim().to_string())
            });
        let state = MachineState {
            state: RunState::Unknown,
            variant: Some(variant.id.into()),
            version,
            ..Default::default()
        };
        let shared = Arc::new(Shared {
            state: Mutex::new(state),
            job: Mutex::new(JobState::default()),
            console: Mutex::new(VecDeque::new()),
            rt: Mutex::new(Vec::new()),
            alive: AtomicBool::new(true),
            next_id: AtomicU64::new(1),
        });
        for l in text.lines().chain(info.lines()) {
            if !l.trim().is_empty() {
                shared.log(format!("< {}", l.trim()));
            }
        }
        let (tx, rx) = mpsc::channel();
        let io = Io {
            t,
            shared: shared.clone(),
            opts: opts.clone(),
            variant,
            rx: Vec::new(),
            pending: VecDeque::new(),
            unacked: 0,
            interactive: VecDeque::new(),
            job: None,
            queue: VecDeque::new(),
            last_poll: Instant::now() - Duration::from_secs(1),
            reset_sent: false,
        };
        let _ = io.variant;
        let thread = std::thread::Builder::new()
            .name(format!("grbl-{name}"))
            .spawn(move || io.run(rx))
            .map_err(|e| DriverError::Transport(e.to_string()))?;
        let d = GrblDriver {
            name: name.into(),
            variant,
            opts,
            tx,
            shared,
            thread: Some(thread),
        };
        // pierwszy raport stanu
        let end = Instant::now() + Duration::from_secs(2);
        while d.status().state == RunState::Unknown && Instant::now() < end {
            std::thread::sleep(Duration::from_millis(5));
        }
        Ok(d)
    }

    pub fn variant(&self) -> &'static Variant {
        self.variant
    }

    pub fn s_max(&self) -> f64 {
        self.opts.s_max.unwrap_or(self.variant.s_max)
    }

    fn rt(&self, bytes: &[u8]) {
        self.shared.rt.lock().expect("rt").extend_from_slice(bytes);
    }

    fn alive(&self) -> Result<(), DriverError> {
        if self.shared.alive.load(Ordering::SeqCst) {
            Ok(())
        } else {
            Err(DriverError::NotConnected)
        }
    }

    fn cmd_t(
        &self,
        line: &str,
        timeout: Duration,
        force: bool,
    ) -> Result<Vec<String>, DriverError> {
        self.alive()?;
        let (r, w) = mpsc::channel();
        self.tx
            .send(Cmd::Line(line.into(), r, force))
            .map_err(|_| DriverError::NotConnected)?;
        w.recv_timeout(timeout)
            .map_err(|_| DriverError::Timeout(line.into()))?
    }

    fn cmd(&self, line: &str) -> Result<Vec<String>, DriverError> {
        self.cmd_t(line, self.opts.reply_timeout, false)
    }

    fn not_alarm(&self) -> Result<(), DriverError> {
        let st = self.status();
        if st.state == RunState::Alarm {
            let (c, t) = st.alarm.unwrap_or((0, "alarm".into()));
            return Err(DriverError::Alarm(
                c,
                format!("{t} — odblokuj ($X) albo wykonaj bazowanie"),
            ));
        }
        Ok(())
    }

    fn f(v: f64) -> String {
        crate::gcode::fmt_num(v, 3, true)
    }

    /// Czeka, aż maszyna wróci do stanu Idle (testy, CLI).
    pub fn wait_idle(&self, timeout: Duration) -> bool {
        let end = Instant::now() + timeout;
        let mut seen_busy = false;
        while Instant::now() < end {
            let s = self.status().state;
            if matches!(s, RunState::Run | RunState::Jog | RunState::Home) {
                seen_busy = true;
            }
            if s == RunState::Idle && (seen_busy || self.shared.rt.lock().expect("rt").is_empty()) {
                std::thread::sleep(Duration::from_millis(self.opts.poll_ms + 20));
                if self.status().state == RunState::Idle {
                    return true;
                }
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        false
    }

    /// Czeka na koniec zadania (Done/Stopped/Failed).
    pub fn wait_job(&self, timeout: Duration) -> JobState {
        let end = Instant::now() + timeout;
        loop {
            let j = self.job();
            if !matches!(j.status, JobStatus::Running | JobStatus::Paused) || Instant::now() >= end
            {
                return j;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    }
}

impl Drop for GrblDriver {
    fn drop(&mut self) {
        self.disconnect();
    }
}

impl Driver for GrblDriver {
    fn name(&self) -> &str {
        &self.name
    }

    fn capabilities(&self) -> Caps {
        let v = self.variant;
        Caps {
            axes: vec!["x".into(), "y".into(), "z".into()],
            rotary: false,
            z: true,
            native_raster: false,
            files: v.sd,
            air_assist: true,
            dual_laser: matches!(v.id, "grblhal" | "fluidnc"),
            camera: false,
            probe: true,
            lid_sensor: true,
            overrides: v.overrides,
            jog: v.jog,
        }
    }

    fn status(&self) -> MachineState {
        let mut s = self.shared.state.lock().expect("stan").clone();
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

    fn home(&mut self) -> Result<(), DriverError> {
        self.cmd_t("$H", Duration::from_secs(120), false)
            .map(|_| ())
    }

    fn unlock(&mut self) -> Result<(), DriverError> {
        self.cmd("$X")?;
        let mut st = self.shared.state.lock().expect("stan");
        st.alarm = None;
        if st.state == RunState::Alarm {
            st.state = RunState::Idle;
        }
        Ok(())
    }

    fn jog(&mut self, axis: Axis, distance: f64, speed: f64) -> Result<(), DriverError> {
        if !(distance.is_finite() && speed > 0.0 && speed.is_finite()) {
            return Err(DriverError::Invalid(
                "jog: odległość skończona, prędkość > 0".into(),
            ));
        }
        self.not_alarm()?;
        let a = axis.letter();
        if self.variant.jog {
            self.cmd(&format!(
                "$J=G91 G21 {a}{} F{}",
                Self::f(distance),
                Self::f(speed * 60.0)
            ))
            .map(|_| ())
        } else {
            self.cmd(&format!(
                "G91 G21 G1 {a}{} F{}",
                Self::f(distance),
                Self::f(speed * 60.0)
            ))?;
            self.cmd("G90").map(|_| ())
        }
    }

    fn jog_cancel(&mut self) -> Result<(), DriverError> {
        self.alive()?;
        if self.variant.jog {
            self.rt(&[0x85]);
        } else {
            self.rt(b"!");
        }
        Ok(())
    }

    fn move_to(
        &mut self,
        x: Option<f64>,
        y: Option<f64>,
        z: Option<f64>,
        speed: Option<f64>,
    ) -> Result<(), DriverError> {
        self.not_alarm()?;
        let mut l = String::from(match speed {
            Some(_) => "G90 G21 G1",
            None => "G90 G21 G0",
        });
        for (c, v) in [('X', x), ('Y', y), ('Z', z)] {
            if let Some(v) = v {
                if !v.is_finite() {
                    return Err(DriverError::Invalid("move_to: liczby skończone".into()));
                }
                l.push_str(&format!(" {c}{}", Self::f(v)));
            }
        }
        if let Some(s) = speed {
            l.push_str(&format!(" F{}", Self::f(s * 60.0)));
        }
        self.cmd(&l).map(|_| ())
    }

    fn set_origin(&mut self, x: bool, y: bool, z: bool) -> Result<(), DriverError> {
        let mut l = String::from("G10 L20 P1");
        for (c, on) in [('X', x), ('Y', y), ('Z', z)] {
            if on {
                l.push_str(&format!(" {c}0"));
            }
        }
        self.cmd(&l).map(|_| ())
    }

    fn frame(
        &mut self,
        points: &[[f64; 2]],
        speed: f64,
        pointer: Option<(&str, &str, [f64; 2])>,
    ) -> Result<(), DriverError> {
        if points.is_empty() {
            return Err(DriverError::Invalid("ramka: brak punktów".into()));
        }
        self.not_alarm()?;
        let off = pointer.map(|p| p.2).unwrap_or([0.0, 0.0]);
        let mut lines = vec!["M5".to_string(), "G90 G21".to_string()];
        if let Some((on, _, _)) = pointer {
            lines.extend(
                on.lines()
                    .filter(|l| !l.trim().is_empty())
                    .map(str::to_string),
            );
        }
        let pt = |p: [f64; 2]| format!("X{} Y{}", Self::f(p[0] - off[0]), Self::f(p[1] - off[1]));
        lines.push(format!("G0 {}", pt(points[0])));
        for p in points.iter().skip(1).chain(std::iter::once(&points[0])) {
            lines.push(format!("G1 {} F{} S0", pt(*p), Self::f(speed * 60.0)));
        }
        if let Some((_, offc, _)) = pointer {
            lines.extend(
                offc.lines()
                    .filter(|l| !l.trim().is_empty())
                    .map(str::to_string),
            );
        }
        for l in lines {
            self.cmd(&l)?;
        }
        Ok(())
    }

    fn test_fire(&mut self, power_pct: f64, ms: u32) -> Result<(), DriverError> {
        if !(0.0..=100.0).contains(&power_pct) || ms == 0 || ms > 5000 {
            return Err(DriverError::Invalid(
                "test_fire: moc 0–100 %, czas 1–5000 ms".into(),
            ));
        }
        self.not_alarm()?;
        let s = power_pct * self.s_max() / 100.0;
        // UNVERIFIED: wiązka w postoju przy M3 (GRBL 1.1 wiki „Laser Mode”) — jak tryb punktowy.
        self.cmd(&format!("M3 S{}", crate::gcode::fmt_num(s, 2, true)))?;
        self.cmd(&format!(
            "G4 P{}",
            crate::gcode::fmt_num(ms as f64 / 1000.0, 3, true)
        ))?;
        self.cmd("M5").map(|_| ())
    }

    fn send_job(&mut self, file: &MachineFile, opts: JobOptions) -> Result<JobState, DriverError> {
        self.alive()?;
        self.not_alarm()?;
        if matches!(self.job().status, JobStatus::Running | JobStatus::Paused) {
            return Err(DriverError::Busy("zadanie w toku".into()));
        }
        let lines = Arc::new(job_lines(file));
        let id = self.shared.next_id.fetch_add(1, Ordering::SeqCst);
        {
            // stan „w toku” od razu (bufor wstępny: wątek wypełnia RX od razu)
            let mut j = self.shared.job.lock().expect("zadanie");
            *j = JobState {
                id,
                name: opts.name.clone(),
                status: JobStatus::Running,
                total: lines.len(),
                round: 1,
                repeat: opts.repeat.max(1),
                ..Default::default()
            };
        }
        self.tx
            .send(Cmd::Job(lines, opts, id, false))
            .map_err(|_| DriverError::NotConnected)?;
        Ok(self.job())
    }

    fn queue_job(&mut self, file: &MachineFile, opts: JobOptions) -> Result<usize, DriverError> {
        self.alive()?;
        if !matches!(self.job().status, JobStatus::Running | JobStatus::Paused) {
            self.send_job(file, opts)?;
            return Ok(0);
        }
        let lines = Arc::new(job_lines(file));
        let id = self.shared.next_id.fetch_add(1, Ordering::SeqCst);
        self.tx
            .send(Cmd::Job(lines, opts, id, true))
            .map_err(|_| DriverError::NotConnected)?;
        let end = Instant::now() + Duration::from_millis(500);
        let before = self.job().queued;
        while self.job().queued == before && Instant::now() < end {
            std::thread::sleep(Duration::from_millis(2));
        }
        Ok(self.job().queued)
    }

    fn job(&self) -> JobState {
        self.shared.job.lock().expect("zadanie").clone()
    }

    fn pause(&mut self) -> Result<(), DriverError> {
        self.alive()?;
        self.rt(b"!");
        let mut j = self.shared.job.lock().expect("zadanie");
        if j.status == JobStatus::Running {
            j.status = JobStatus::Paused;
        }
        Ok(())
    }

    fn resume(&mut self) -> Result<(), DriverError> {
        self.alive()?;
        let st = self.status();
        if st.lid_open == Some(true) {
            return Err(DriverError::Busy(
                "pokrywa otwarta — zamknij przed wznowieniem".into(),
            ));
        }
        // alarm (np. E-stop / reset w ruchu): wznowienie niemożliwe, operator odblokowuje ($X / $H)
        if st.state == RunState::Alarm {
            let (c, t) = st.alarm.unwrap_or((
                0,
                "alarm — odblokuj maszynę ($X) albo wykonaj bazowanie".into(),
            ));
            return Err(DriverError::Alarm(c, t));
        }
        self.rt(b"~");
        let mut j = self.shared.job.lock().expect("zadanie");
        if j.status == JobStatus::Paused {
            j.status = JobStatus::Running;
        }
        Ok(())
    }

    fn stop(&mut self) {
        // wstrzymanie i soft reset (wiązka wyłączona, bufory wyczyszczone) — poza kolejką
        self.rt(&[b'!', 0x18]);
        let mut j = self.shared.job.lock().expect("zadanie");
        if matches!(j.status, JobStatus::Running | JobStatus::Paused) {
            j.status = JobStatus::Stopped;
        }
    }

    fn set_overrides(&mut self, feed: Option<u32>, power: Option<u32>) -> Result<(), DriverError> {
        self.alive()?;
        if !self.variant.overrides {
            return Err(DriverError::Unsupported(format!(
                "{}: brak nadpisań czasu rzeczywistego",
                self.variant.name
            )));
        }
        let steps = |target: u32,
                     reset: u8,
                     p10: u8,
                     m10: u8,
                     p1: u8,
                     m1: u8|
         -> Result<Vec<u8>, DriverError> {
            if !(10..=200).contains(&target) {
                return Err(DriverError::Invalid("nadpisanie: 10–200 %".into()));
            }
            let mut v = vec![reset];
            let mut cur: i64 = 100;
            let t = target as i64;
            while (t - cur).abs() >= 10 {
                v.push(if t > cur { p10 } else { m10 });
                cur += if t > cur { 10 } else { -10 };
            }
            while cur != t {
                v.push(if t > cur { p1 } else { m1 });
                cur += if t > cur { 1 } else { -1 };
            }
            Ok(v)
        };
        let mut bytes = Vec::new();
        if let Some(f) = feed {
            bytes.extend(steps(f, 0x90, 0x91, 0x92, 0x93, 0x94)?);
        }
        if let Some(p) = power {
            bytes.extend(steps(p, 0x99, 0x9A, 0x9B, 0x9C, 0x9D)?);
        }
        self.rt(&bytes);
        Ok(())
    }

    fn read_settings(&mut self) -> Result<Vec<Setting>, DriverError> {
        let lines = self.cmd("$$")?;
        Ok(lines
            .iter()
            .filter_map(|l| l.strip_prefix('$'))
            .filter_map(|l| l.split_once('='))
            .map(|(k, v)| {
                let v = v.split_whitespace().next().unwrap_or("").to_string();
                let info = k.parse().ok().and_then(codes::setting_info);
                Setting {
                    id: format!("${k}"),
                    value: v,
                    // spoza tabeli: rozszerzenie płyty producenta (np. MKS DLC32 $40–$54)
                    name: Some(info.map_or(codes::VENDOR_SETTING, |i| i.name).to_string()),
                    unit: info.map(|i| i.unit.to_string()),
                    min: info.map(|i| i.min),
                    max: info.map(|i| i.max),
                }
            })
            .collect())
    }

    fn write_settings(
        &mut self,
        changes: &[(String, String)],
        backup_dir: Option<&std::path::Path>,
        dry_run: bool,
    ) -> Result<WriteReport, DriverError> {
        let current = self.read_settings()?;
        let mut diff = Vec::new();
        for (k, v) in changes {
            let id: u32 = k
                .trim_start_matches('$')
                .parse()
                .map_err(|_| DriverError::Invalid(format!("{k}: numer ustawienia")))?;
            codes::validate_setting(id, v).map_err(DriverError::Invalid)?;
            let key = format!("${id}");
            let before = current
                .iter()
                .find(|s| s.id == key)
                .map(|s| s.value.clone());
            if before.is_none() && codes::setting_info(id).is_none() {
                return Err(DriverError::Invalid(format!("{key}: nieznane ustawienie")));
            }
            let same = before
                .as_deref()
                .and_then(|b| b.parse::<f64>().ok())
                .zip(v.trim().parse::<f64>().ok())
                .is_some_and(|(a, b)| (a - b).abs() < 1e-9);
            if !same {
                diff.push(SettingChange {
                    id: key,
                    before,
                    after: v.trim().to_string(),
                });
            }
        }
        if dry_run || diff.is_empty() {
            return Ok(WriteReport {
                changes: diff,
                backup: None,
                dry_run,
            });
        }
        // kopia przed zapisem (REQ-13-013)
        let backup = match backup_dir {
            Some(dir) => {
                std::fs::create_dir_all(dir).map_err(|e| DriverError::Invalid(e.to_string()))?;
                let stamp = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_millis())
                    .unwrap_or(0);
                let safe: String = self
                    .name
                    .chars()
                    .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
                    .collect();
                let p = dir.join(format!("{safe}-{stamp}.json"));
                let v: serde_json::Map<String, serde_json::Value> = current
                    .iter()
                    .map(|s| (s.id.clone(), serde_json::Value::from(s.value.clone())))
                    .collect();
                std::fs::write(&p, serde_json::to_vec_pretty(&v).unwrap_or_default())
                    .map_err(|e| DriverError::Invalid(e.to_string()))?;
                Some(p.display().to_string())
            }
            None => None,
        };
        for c in &diff {
            self.cmd(&format!("{}={}", c.id, c.after))?;
        }
        Ok(WriteReport {
            changes: diff,
            backup,
            dry_run: false,
        })
    }

    fn probe_z(&mut self, distance: f64, feed: f64) -> Result<f64, DriverError> {
        if !(distance > 0.0 && feed > 0.0) {
            return Err(DriverError::Invalid("sonda: odległość i posuw > 0".into()));
        }
        self.not_alarm()?;
        let lines = self.cmd(&format!(
            "G38.2 Z-{} F{}",
            Self::f(distance),
            Self::f(feed * 60.0)
        ));
        let lines = match lines {
            Ok(l) => l,
            Err(DriverError::Alarm(c, t)) => return Err(DriverError::Alarm(c, t)),
            Err(e) => return Err(e),
        };
        let prb = lines
            .iter()
            .find_map(|l| l.strip_prefix("[PRB:").and_then(|r| r.strip_suffix(']')))
            .ok_or_else(|| DriverError::Invalid("sonda: brak wyniku [PRB]".into()))?;
        let (pos, ok) = prb.rsplit_once(':').unwrap_or((prb, "0"));
        if ok != "1" {
            return Err(DriverError::Invalid("sonda: brak kontaktu".into()));
        }
        let p = parse3(pos).ok_or_else(|| DriverError::Invalid("sonda: zły wynik".into()))?;
        Ok(p[2] - self.status().wco[2])
    }

    fn list_files(&mut self) -> Result<Vec<RemoteFile>, DriverError> {
        if !self.variant.sd {
            return Err(DriverError::Unsupported(format!(
                "{}: brak karty SD",
                self.variant.name
            )));
        }
        let lines = self.cmd("$SD/List")?;
        Ok(lines
            .iter()
            .filter_map(|l| l.strip_prefix("[FILE:").and_then(|r| r.strip_suffix(']')))
            .map(|r| {
                let (n, s) = r.split_once("|SIZE:").unwrap_or((r, "0"));
                RemoteFile {
                    name: n.to_string(),
                    size: s.parse().unwrap_or(0),
                }
            })
            .collect())
    }

    fn upload_file(&mut self, name: &str, data: &[u8]) -> Result<(), DriverError> {
        if !self.variant.sd {
            return Err(DriverError::Unsupported(format!(
                "{}: brak karty SD",
                self.variant.name
            )));
        }
        self.alive()?;
        let (r, w) = mpsc::channel();
        self.tx
            .send(Cmd::Upload(name.into(), data.to_vec(), r))
            .map_err(|_| DriverError::NotConnected)?;
        w.recv_timeout(Duration::from_secs(120))
            .map_err(|_| DriverError::Timeout("XMODEM".into()))?
            .map(|_| ())
    }

    fn run_file(&mut self, name: &str) -> Result<(), DriverError> {
        if !self.variant.sd {
            return Err(DriverError::Unsupported("brak karty SD".into()));
        }
        self.not_alarm()?;
        self.cmd(&format!("$SD/Run={name}")).map(|_| ())
    }

    fn delete_file(&mut self, name: &str) -> Result<(), DriverError> {
        if !self.variant.sd {
            return Err(DriverError::Unsupported("brak karty SD".into()));
        }
        self.cmd(&format!("$SD/Delete={name}")).map(|_| ())
    }

    fn raw(&mut self, line: &str) -> Result<Vec<String>, DriverError> {
        let l = line.trim();
        if l.contains('\n') || l.contains('\r') {
            return Err(DriverError::Invalid("jedno polecenie w wierszu".into()));
        }
        match l {
            "?" | "!" | "~" => {
                self.rt(l.as_bytes());
                Ok(Vec::new())
            }
            _ => self.cmd(l),
        }
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
        c.settings.s_max = self.s_max();
        // GRBL 0.9 nie ma trybu laserowego: wiązka jak wrzeciono (M3, M5 przed przejazdem)
        let dialect = if self.variant.status_v09 {
            "gcode"
        } else {
            "grbl"
        };
        Ok(crate::compile(dialect, job, &c)?)
    }
}
