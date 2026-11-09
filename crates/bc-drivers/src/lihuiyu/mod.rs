//! Sterownik K40 Lihuiyu (ADR 0014): płyty M2 Nano, M3 Nano, M1, M, B1, B2, B, A,
//! 6C6879-LASER-M2/M3 (REQ-13-030…038), protokół EGV przez CH341 (REQ-13-039…048).
//! Implementuje trait `Driver` z ADR 0010 bez zmiany interfejsu.

pub mod egv;
pub mod emu;

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::ch341::Ch341;
use crate::driver::*;
use crate::{Context, MachineFile};
use egv::{Egv, Packet, Steps};

/// Wariant płyty Lihuiyu (REQ-13-030…038, REQ-13-046).
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
pub enum Board {
    #[serde(rename = "m2")]
    #[default]
    M2,
    #[serde(rename = "m3")]
    M3,
    #[serde(rename = "m1")]
    M1,
    #[serde(rename = "m")]
    M,
    #[serde(rename = "b1")]
    B1,
    #[serde(rename = "b2")]
    B2,
    #[serde(rename = "b")]
    B,
    #[serde(rename = "a")]
    A,
    /// 6C6879-LASER-M2 (rewizja płyty M2).
    #[serde(rename = "6c6879_m2")]
    L6c6879M2,
    /// 6C6879-LASER-M3 (rewizja płyty M3).
    #[serde(rename = "6c6879_m3")]
    L6c6879M3,
}

/// Wszystkie warianty (kolejność listy w GUI).
pub const BOARDS: &[Board] = &[
    Board::M2,
    Board::M3,
    Board::M1,
    Board::M,
    Board::B1,
    Board::B2,
    Board::B,
    Board::A,
    Board::L6c6879M2,
    Board::L6c6879M3,
];

/// Opis wariantu płyty (lista w GUI i `machine.list`).
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
pub struct BoardInfo {
    pub id: &'static str,
    pub name: &'static str,
    /// Regulacja mocy PWM z programu.
    pub pwm: bool,
}

impl Board {
    pub fn id(self) -> &'static str {
        match self {
            Board::M2 => "m2",
            Board::M3 => "m3",
            Board::M1 => "m1",
            Board::M => "m",
            Board::B1 => "b1",
            Board::B2 => "b2",
            Board::B => "b",
            Board::A => "a",
            Board::L6c6879M2 => "6c6879_m2",
            Board::L6c6879M3 => "6c6879_m3",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Board::M2 => "M2 Nano",
            Board::M3 => "M3 Nano",
            Board::M1 => "M1",
            Board::M => "M",
            Board::B1 => "B1",
            Board::B2 => "B2",
            Board::B => "B",
            Board::A => "A",
            Board::L6c6879M2 => "6C6879-LASER-M2",
            Board::L6c6879M3 => "6C6879-LASER-M3",
        }
    }

    pub fn from_id(id: &str) -> Option<Board> {
        BOARDS.iter().copied().find(|b| b.id() == id)
    }

    pub fn info(self) -> BoardInfo {
        BoardInfo {
            id: self.id(),
            name: self.name(),
            pwm: self.pwm(),
        }
    }

    /// Grupa równania prędkości. Źródło: MeerK40t `laserspeed.py` (opis).
    pub fn group(self) -> egv::Group {
        match self {
            Board::A | Board::B | Board::B1 => egv::Group::Ab,
            Board::B2 => egv::Group::B2,
            _ => egv::Group::Default,
        }
    }

    /// Człon przekątnej w kodzie prędkości (brak dla A, B, M). Źródło: MeerK40t.
    pub fn diagonal(self) -> bool {
        !matches!(self, Board::A | Board::B | Board::M)
    }

    /// Kod z sufiksem C poniżej 7 mm/s (M2, M3, B2). Źródło: MeerK40t `laserspeed.py`.
    pub fn suffix_c(self) -> bool {
        matches!(
            self,
            Board::M2 | Board::M3 | Board::B2 | Board::L6c6879M2 | Board::L6c6879M3
        )
    }

    /// PWM mocy z programu: tylko M3 Nano (firmware 2024.01.18g+). Źródło: README MeerK40t.
    /// UNVERIFIED: wersja firmware płyty M3 nie jest sprawdzana.
    pub fn pwm(self) -> bool {
        matches!(self, Board::M3 | Board::L6c6879M3)
    }
}

/// Ustawienia płyty K40 w profilu urządzenia (REQ-13-046, 047).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct K40Settings {
    /// Wariant płyty Lihuiyu (Moshi: pomijany).
    #[serde(default)]
    pub board: Board,
    /// ID płyty (numer seryjny z naklejki); sprawdzany przy połączeniu, gdy podany.
    #[serde(default)]
    pub board_id: Option<String>,
    /// Kroki na cal (silniki 1000 dpi). Źródło: MeerK40t (1000 dpi).
    #[serde(default = "d_dpi")]
    pub dpi: f64,
    /// Korekcja skali X, Y (1 = bez korekcji).
    #[serde(default = "d_scale")]
    pub scale: [f64; 2],
}

fn d_dpi() -> f64 {
    1000.0
}
fn d_scale() -> [f64; 2] {
    [1.0, 1.0]
}

impl Default for K40Settings {
    fn default() -> Self {
        K40Settings {
            board: Board::M2,
            board_id: None,
            dpi: d_dpi(),
            scale: d_scale(),
        }
    }
}

impl K40Settings {
    pub fn validate(&self) -> Result<(), String> {
        if !(self.dpi >= 100.0 && self.dpi <= 10000.0) {
            return Err("k40.dpi: 100–10000".into());
        }
        if !self.scale.iter().all(|s| *s >= 0.5 && *s <= 2.0) {
            return Err("k40.scale: 0,5–2".into());
        }
        if let Some(id) = &self.board_id {
            if id.trim().is_empty() || id.len() > 64 || !id.is_ascii() {
                return Err("k40.board_id: 1–64 znaki ASCII".into());
            }
        }
        Ok(())
    }
}

/// Stół K40 (mm). Źródło: README MeerK40t `lihuiyu` (310 × 210 mm).
pub const BED: [f64; 2] = [310.0, 210.0];

/// Opcje połączenia.
#[derive(Debug, Clone)]
pub struct LihuiyuOptions {
    pub settings: K40Settings,
    /// Obszar roboczy (mm) — narożnik bazowania to lewy górny róg.
    pub bed: [f64; 2],
}

impl Default for LihuiyuOptions {
    fn default() -> Self {
        LihuiyuOptions {
            settings: K40Settings::default(),
            bed: BED,
        }
    }
}

type Reply = mpsc::Sender<Result<(), DriverError>>;

enum Cmd {
    /// Pakiety poza zadaniem (bazowanie, przejazd, ramka, surowe).
    Send(Vec<Packet>, Reply),
    Job(Arc<Vec<Packet>>, JobOptions, u64),
    Pause,
    Resume,
    Stop,
    Close,
}

struct Shared {
    state: Mutex<MachineState>,
    job: Mutex<JobState>,
    console: Mutex<VecDeque<String>>,
    alive: AtomicBool,
    /// Żądanie zatrzymania widoczne od razu w wątku płyty, także gdy czeka on na koniec
    /// zadania albo na potwierdzenie pakietu (test sprzętowy K40, A15).
    stop: AtomicBool,
    /// Żądana pauza zadania (obsługa także w trakcie czekania na koniec programu).
    want_pause: AtomicBool,
    queue: Mutex<VecDeque<(Arc<Vec<Packet>>, JobOptions)>>,
    /// Pozycja wg wysłanych pakietów (kroki).
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

/// Sterownik Lihuiyu.
pub struct LihuiyuDriver {
    name: String,
    opts: LihuiyuOptions,
    shared: Arc<Shared>,
    tx: mpsc::Sender<Cmd>,
    thread: Option<std::thread::JoinHandle<()>>,
    next_id: u64,
    wco: [f64; 2],
}

const POLLS: usize = 500;
/// Czas, po którym niezmieniony stan po zapisie uznajemy za odpowiedź płyty (pakiet
/// identyczny z poprzednim: bez BUSY). Test K40: wynik po ok. 6 ms.
const SETTLE: Duration = Duration::from_millis(30);

struct Io {
    dev: Box<dyn Ch341>,
    shared: Arc<Shared>,
    track: emu::Interp,
    job: Option<(Arc<Vec<Packet>>, usize, JobOptions, Instant)>,
    /// Płyta wstrzymana pakietem `PN`.
    board_paused: bool,
    /// Ruchy wysłane, jeszcze niewykonane przez płytę (szacowanie pozycji głowicy w czasie),
    /// czas w bieżącym ruchu i ostatnia aktualizacja.
    plan: VecDeque<emu::EgvMove>,
    plan_t: f64,
    plan_at: Instant,
    /// Dane ostatniego potwierdzonego pakietu.
    last_sent: Option<[u8; egv::PAYLOAD]>,
}

impl Io {
    fn status(&mut self) -> Result<u8, DriverError> {
        let s = self.dev.status().map_err(|e| DriverError::Transport(e.0))?;
        Ok(s[1])
    }

    /// Wysyła pakiet; pakiet identyczny z poprzednim poprzedza pakiet samego dopełnienia.
    /// Płyta potwierdza, ale pomija pakiet taki sam jak poprzedni (test sprzętowy K40,
    /// M2 Nano, 2026-11-09: co drugi jednakowy jog). Dopełnienie `F` między pakietami jest
    /// bezpieczne jak pakiet czasu rzeczywistego, o ile nie rozcina liczby.
    fn send(&mut self, p: &Packet) -> Result<(), DriverError> {
        let filler = [egv::PAD; egv::PAYLOAD];
        if self.last_sent == Some(p.data) && p.data != filler {
            let last = p
                .data
                .iter()
                .rposition(|b| *b != egv::PAD)
                .map(|k| p.data[k]);
            let splits_number =
                last.is_some_and(|b| b.is_ascii_digit() || b == b'|') && p.data[0].is_ascii_digit();
            if !splits_number {
                self.send_raw(&Packet {
                    data: filler,
                    wait_finish: false,
                })?;
            }
        }
        self.send_raw(p)
    }

    /// Wysyła pakiet i czeka na potwierdzenie (ponowienie po błędzie CRC).
    ///
    /// Odczyt stanu tuż po zapisie zwraca stan sprzed zapisu (test sprzętowy K40, M2 Nano,
    /// dziennik `k40_probe` 2026-11-09: wynik po ok. 6 ms, wcześniej BUSY). Odpowiedź liczy
    /// się więc dopiero po BUSY, po zmianie stanu albo po [`SETTLE`] bez zmiany.
    fn send_raw(&mut self, p: &Packet) -> Result<(), DriverError> {
        self.send_raw_opts(p, false)
    }

    /// Pakiet czasu rzeczywistego (`I`, `PN`) i dopełnienie po nim, bez czekania na koniec
    /// stanu BUSY: płyta wstrzymana z pełnym buforem zgłasza BUSY do wznowienia.
    /// Źródło: MeerK40t `controller.py` (pakiety czasu rzeczywistego omijają BUSY).
    fn send_realtime(&mut self, text: &[u8]) -> Result<(), DriverError> {
        self.send_raw_opts(&packet(text), true)?;
        // dopełnienie bez czekania na odpowiedź: M2 Nano wykonuje pakiet po następnym,
        // a wstrzymana płyta z pełnym buforem nie przyjmie dopełnienia do wznowienia
        let fill = egv::frame(&[egv::PAD; egv::PAYLOAD]);
        self.dev
            .write_data(&fill)
            .map_err(|e| DriverError::Transport(e.0))?;
        self.last_sent = None;
        std::thread::sleep(SETTLE);
        Ok(())
    }

    fn send_raw_opts(&mut self, p: &Packet, realtime: bool) -> Result<(), DriverError> {
        let frame = egv::frame(&p.data);
        let text = String::from_utf8_lossy(&p.data)
            .trim_end_matches('F')
            .to_string();
        let mut resends = 0;
        'send: loop {
            // przed zapisem: płyta nie może być zajęta poprzednim pakietem
            let mut before = self.status()?;
            let mut n = 0;
            while before == egv::STATUS_BUSY && !realtime {
                n += 1;
                if n > POLLS {
                    return Err(DriverError::Timeout("płyta stale zajęta (BUSY)".into()));
                }
                std::thread::sleep(Duration::from_millis(2));
                before = self.status()?;
            }
            self.dev
                .write_data(&frame)
                .map_err(|e| DriverError::Transport(e.0))?;
            let written = Instant::now();
            let mut fresh = false;
            for i in 0..POLLS {
                let s = self.status()?;
                if !fresh {
                    fresh = s == egv::STATUS_BUSY || s != before || written.elapsed() >= SETTLE;
                    if !fresh {
                        std::thread::sleep(Duration::from_millis(1));
                        continue;
                    }
                }
                match s {
                    egv::STATUS_OK => {
                        self.shared.log(format!("> {text}"));
                        break 'send;
                    }
                    egv::STATUS_CRC_ERROR => {
                        resends += 1;
                        self.shared.log(format!("< CRC ERROR ({resends})"));
                        if resends > 5 {
                            return Err(DriverError::Transport(
                                "płyta odrzuca pakiety (CRC) — sprawdź kabel USB".into(),
                            ));
                        }
                        continue 'send;
                    }
                    _ => {
                        // BUSY, FINISH, 0 — czekaj (MeerK40t controller.py)
                        if i > 10 {
                            std::thread::sleep(Duration::from_secs_f64(
                                (0.001 * i as f64).min(0.1),
                            ));
                        }
                    }
                }
            }
            return Err(DriverError::Timeout("brak potwierdzenia pakietu".into()));
        }
        self.last_sent = Some(p.data);
        self.advance_estimate();
        self.track.feed(&p.data);
        let moves: Vec<_> = self.track.moves.drain(..).collect();
        self.plan.extend(moves);
        if p.wait_finish {
            self.push_through()?;
            if self.wait_finish()? {
                // płyta skończyła program: głowica na końcu wysłanych ruchów
                self.plan.clear();
                self.plan_t = 0.0;
                self.set_pos(self.track.pos);
            }
        }
        Ok(())
    }

    fn set_pos(&self, p: (i64, i64)) {
        *self.shared.pos.lock().expect("pozycja") = p;
    }

    /// Szacowana pozycja głowicy: ruchy wysłane do płyty odtwarzane w czasie wg prędkości
    /// z programu (płyta wykonuje program po swojemu, sterownik nie zna jej pozycji). Test
    /// sprzętowy K40 (2026-11-09): krzyżyk na kanwie skakał na koniec wysłanego programu.
    /// UNVERIFIED: prędkość przejazdów szybkich płyty (`Interp::rapid_speed`).
    fn advance_estimate(&mut self) {
        let now = Instant::now();
        let mut dt = (now - self.plan_at).as_secs_f64();
        self.plan_at = now;
        if self.board_paused {
            return;
        }
        let mut pos = None;
        while let Some(m) = self.plan.front().copied() {
            let len = (((m.to.0 - m.from.0) as f64).powi(2) + ((m.to.1 - m.from.1) as f64).powi(2))
                .sqrt()
                * 25.4
                / 1000.0;
            let dur = len / m.speed.max(0.01);
            if self.plan_t + dt >= dur {
                dt -= dur - self.plan_t;
                self.plan_t = 0.0;
                self.plan.pop_front();
                pos = Some(m.to);
            } else {
                self.plan_t += dt;
                let f = if dur > 0.0 { self.plan_t / dur } else { 1.0 };
                pos = Some((
                    m.from.0 + ((m.to.0 - m.from.0) as f64 * f).round() as i64,
                    m.from.1 + ((m.to.1 - m.from.1) as f64 * f).round() as i64,
                ));
                break;
            }
        }
        if let Some(p) = pos {
            self.set_pos(p);
        }
    }

    /// Szacowana pozycja (kroki).
    fn estimate(&self) -> (i64, i64) {
        *self.shared.pos.lock().expect("pozycja")
    }

    /// Pakiet dopełnienia po ostatnim pakiecie polecenia: płyta M2 Nano wykonuje przyjęty
    /// pakiet dopiero po przyjęciu następnego (test sprzętowy K40, dziennik `k40_probe`
    /// 2026-11-09). Dopełnienie `F` nie zmienia stanu płyty.
    fn push_through(&mut self) -> Result<(), DriverError> {
        self.send_raw(&Packet {
            data: [egv::PAD; egv::PAYLOAD],
            wait_finish: false,
        })
    }

    /// Czeka, aż płyta skończy (bit 0x02 stanu zgaszony: FINISH 236 / 204).
    /// Źródło: MeerK40t `controller.py` (wait-finished). `false` — przerwane przez Stop.
    fn wait_finish(&mut self) -> Result<bool, DriverError> {
        let t0 = Instant::now();
        loop {
            let s = self.status()?;
            if s != 0 && s & 0x02 == 0 {
                return Ok(true);
            }
            if s == 0 && t0.elapsed() > Duration::from_secs(5) {
                return Err(DriverError::Transport("brak stanu płyty".into()));
            }
            // zatrzymanie w trakcie czekania
            if self.shared.stop.load(Ordering::SeqCst) {
                return Ok(false);
            }
            self.advance_estimate();
            // pauza i wznowienie w trakcie programu (koniec pakietu z „-” — granica bezpieczna)
            self.sync_pause();
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    /// Wysyła `PN`, gdy żądany stan pauzy różni się od stanu płyty. Test sprzętowy K40
    /// (2026-11-09): pauza obsługiwana dopiero po końcu programu nie działała.
    /// UNVERIFIED: `PN` wstrzymuje i wznawia płytę w trakcie programu (MeerK40t driver.py).
    fn sync_pause(&mut self) {
        let want = self.shared.want_pause.load(Ordering::SeqCst);
        if want == self.board_paused {
            return;
        }
        if let Err(e) = self.send_realtime(b"PN") {
            self.shared.log(format!("! pauza: {e}"));
            return;
        }
        self.board_paused = want;
        let mut j = self.shared.job.lock().expect("zadanie");
        if matches!(j.status, JobStatus::Running | JobStatus::Paused) {
            j.status = if want {
                JobStatus::Paused
            } else {
                JobStatus::Running
            };
        }
        drop(j);
        self.set_state(if want { RunState::Hold } else { RunState::Run });
    }

    fn set_state(&self, s: RunState) {
        self.shared.state.lock().expect("stan").state = s;
    }

    fn start_job(&mut self, p: Arc<Vec<Packet>>, o: JobOptions, id: u64) {
        {
            let mut j = self.shared.job.lock().expect("zadanie");
            *j = JobState {
                id,
                name: o.name.clone(),
                status: JobStatus::Running,
                total: p.len(),
                round: 1,
                repeat: o.repeat.max(1),
                queued: self.shared.queue.lock().expect("kolejka").len(),
                ..Default::default()
            };
        }
        self.job = Some((p, 0, o, Instant::now()));
        self.shared.want_pause.store(false, Ordering::SeqCst);
        self.set_state(RunState::Run);
    }

    fn end_job(&mut self, st: JobStatus) {
        self.job = None;
        self.shared.want_pause.store(false, Ordering::SeqCst);
        self.shared.job.lock().expect("zadanie").status = st.clone();
        self.set_state(RunState::Idle);
        let next = self.shared.queue.lock().expect("kolejka").pop_front();
        if st == JobStatus::Done {
            if let Some((p, o)) = next {
                let id = self.shared.job.lock().expect("zadanie").id + 1;
                self.start_job(p, o, id);
            }
        } else {
            self.shared.queue.lock().expect("kolejka").clear();
        }
    }

    fn step_job(&mut self) {
        let Some((p, i, o, t0)) = self.job.take() else {
            return;
        };
        // pakiet „PN” tylko na granicy, która nie rozcina liczby
        if self.safe_boundary_of(&p, i) {
            self.sync_pause();
        }
        if self.board_paused {
            self.job = Some((p, i, o, t0));
            std::thread::sleep(Duration::from_millis(2));
            return;
        }
        if i >= p.len() {
            let mut j = self.shared.job.lock().expect("zadanie");
            j.elapsed_s = t0.elapsed().as_secs_f64();
            if j.round < j.repeat {
                j.round += 1;
                drop(j);
                if o.delay_s > 0.0 {
                    std::thread::sleep(Duration::from_secs_f64(o.delay_s));
                }
                self.job = Some((p, 0, o, t0));
                return;
            }
            drop(j);
            self.end_job(JobStatus::Done);
            return;
        }
        if i == 0 && self.track.pos != (0, 0) {
            // zadanie skompilowane od narożnika bazowania: przejazd szybki do (0, 0)
            let mut e = Egv::new(self.track.pos);
            e.rapid((0, 0));
            for q in egv::packets(&e.out) {
                if let Err(err) = self.send(&q) {
                    self.shared.log(format!("! {err}"));
                    self.end_job(JobStatus::Failed);
                    return;
                }
            }
        }
        match self.send(&p[i]) {
            Ok(()) => {
                let mut j = self.shared.job.lock().expect("zadanie");
                j.sent = i + 1;
                j.acked = i + 1;
                j.elapsed_s = t0.elapsed().as_secs_f64();
                drop(j);
                self.job = Some((p, i + 1, o, t0));
            }
            Err(e) => {
                let msg = e.to_string();
                self.shared.log(format!("! {msg}"));
                self.shared
                    .job
                    .lock()
                    .expect("zadanie")
                    .errors
                    .push((i, 0, msg));
                self.end_job(JobStatus::Failed);
            }
        }
    }

    /// Zatrzymanie: koniec zadania i kolejki, pakiet „I” przerywający program płyty
    /// (MeerK40t driver.py: abort `I\n`, reset `I*`) i dopełnienie, bo M2 Nano wykonuje
    /// pakiet dopiero po następnym. UNVERIFIED: „I” przerywa program już buforowany w płycie.
    fn do_stop(&mut self) {
        if self.job.is_some() {
            self.shared.job.lock().expect("zadanie").status = JobStatus::Stopped;
        }
        self.job = None;
        // „I” zdejmuje też pauzę płyty
        self.shared.want_pause.store(false, Ordering::SeqCst);
        self.advance_estimate();
        self.board_paused = false;
        // głowica stoi w szacowanym miejscu; kolejne ruchy liczone od niego (bazowanie
        // przywraca dokładną pozycję)
        self.plan.clear();
        self.plan_t = 0.0;
        self.track.pos = self.estimate();
        self.shared.queue.lock().expect("kolejka").clear();
        if let Err(e) = self.send_realtime(b"I") {
            self.shared.log(format!("! stop: {e}"));
        }
        self.set_state(RunState::Idle);
    }

    /// Granica pakietów bezpieczna dla pakietu czasu rzeczywistego (nie rozcina liczby).
    fn safe_boundary_of(&self, p: &Arc<Vec<Packet>>, i: usize) -> bool {
        if i == 0 || i >= p.len() {
            return true;
        }
        let prev = p[i - 1].data;
        let last = prev.iter().rposition(|b| *b != egv::PAD).map(|k| prev[k]);
        !(last.is_some_and(|b| b.is_ascii_digit() || b == b'|') && p[i].data[0].is_ascii_digit())
    }

    fn run(mut self, rx: mpsc::Receiver<Cmd>) {
        loop {
            let busy = self.job.is_some() && !self.board_paused;
            let cmd = if busy {
                rx.try_recv().ok()
            } else {
                match rx.recv_timeout(Duration::from_millis(5)) {
                    Ok(c) => Some(c),
                    Err(mpsc::RecvTimeoutError::Timeout) => None,
                    Err(_) => break,
                }
            };
            match cmd {
                Some(Cmd::Close) => break,
                Some(Cmd::Send(ps, reply)) => {
                    let mut r = Ok(());
                    for p in &ps {
                        if let Err(e) = self.send(p) {
                            r = Err(e);
                            break;
                        }
                    }
                    if r.is_ok() && ps.last().is_some_and(|p| !p.wait_finish) {
                        r = self.push_through();
                    }
                    // polecenia poza zadaniem (jog, bazowanie, przejazd): pozycja docelowa od
                    // razu, szacowanie w czasie tylko w zadaniu
                    self.plan.clear();
                    self.plan_t = 0.0;
                    self.set_pos(self.track.pos);
                    let _ = reply.send(r);
                }
                Some(Cmd::Job(p, o, id)) => self.start_job(p, o, id),
                // pauzę obsługuje `want_pause` (sync_pause), zatrzymanie flaga `stop` poniżej
                Some(Cmd::Pause) | Some(Cmd::Resume) | Some(Cmd::Stop) | None => {}
            }
            if self.shared.stop.swap(false, Ordering::SeqCst) {
                self.do_stop();
            }
            self.advance_estimate();
            if self.job.is_some() {
                self.step_job();
                // Stop w trakcie pakietu albo czekania na koniec zadania
                if self.shared.stop.swap(false, Ordering::SeqCst) {
                    self.do_stop();
                }
            }
        }
        self.shared.alive.store(false, Ordering::SeqCst);
    }
}

/// Pakiet z tekstem (dopełnienie `F`).
fn packet(text: &[u8]) -> Packet {
    let mut data = [egv::PAD; egv::PAYLOAD];
    data[..text.len().min(egv::PAYLOAD)].copy_from_slice(&text[..text.len().min(egv::PAYLOAD)]);
    Packet {
        data,
        wait_finish: false,
    }
}

impl LihuiyuDriver {
    /// Łączy z płytą: odczyt stanu, potwierdzenie ID płyty (gdy podane).
    pub fn connect(
        name: &str,
        mut dev: Box<dyn Ch341>,
        opts: LihuiyuOptions,
    ) -> Result<Self, DriverError> {
        opts.settings.validate().map_err(DriverError::Invalid)?;
        let s = dev.status().map_err(|e| DriverError::Transport(e.0))?;
        if s[1] == 0 {
            return Err(DriverError::Transport(
                "płyta Lihuiyu nie odpowiada (brak stanu)".into(),
            ));
        }
        let shared = Arc::new(Shared {
            state: Mutex::new(MachineState {
                state: RunState::Idle,
                variant: Some(opts.settings.board.id().into()),
                version: Some(opts.settings.board.name().into()),
                ..Default::default()
            }),
            job: Mutex::new(JobState::default()),
            console: Mutex::new(VecDeque::new()),
            alive: AtomicBool::new(true),
            stop: AtomicBool::new(false),
            want_pause: AtomicBool::new(false),
            queue: Mutex::new(VecDeque::new()),
            pos: Mutex::new((0, 0)),
        });
        shared.log(format!(
            "< {} ({})",
            egv::status_name(s[1]),
            opts.settings.board.name()
        ));
        if let Some(id) = &opts.settings.board_id {
            // potwierdzenie numeru seryjnego: `A` + MD5(ID wielkimi literami), czekanie
            // 0,5 s na stan 204. Źródło: MeerK40t controller.py.
            let mut data = [egv::PAD; egv::PAYLOAD];
            data[0] = b'A';
            data[1..17].copy_from_slice(&crate::md5::md5(id.to_uppercase().as_bytes()));
            dev.write_data(&egv::frame(&data))
                .map_err(|e| DriverError::Transport(e.0))?;
            let t0 = Instant::now();
            let mut ok = false;
            while t0.elapsed() < Duration::from_millis(500) {
                let s = dev.status().map_err(|e| DriverError::Transport(e.0))?;
                if s[1] == egv::STATUS_SERIAL_OK {
                    ok = true;
                    break;
                }
                std::thread::sleep(Duration::from_millis(2));
            }
            if !ok {
                return Err(DriverError::Invalid(format!(
                    "ID płyty {id} nie zostało potwierdzone — sprawdź numer z naklejki płyty"
                )));
            }
            shared.log(format!("< SERIAL OK ({id})"));
        }
        let (tx, rx) = mpsc::channel();
        let io = Io {
            dev,
            shared: shared.clone(),
            track: emu::Interp::new(opts.settings.board),
            job: None,
            board_paused: false,
            plan: VecDeque::new(),
            plan_t: 0.0,
            plan_at: Instant::now(),
            last_sent: None,
        };
        let thread = std::thread::spawn(move || io.run(rx));
        Ok(LihuiyuDriver {
            name: name.into(),
            opts,
            shared,
            tx,
            thread: Some(thread),
            next_id: 1,
            wco: [0.0; 2],
        })
    }

    pub fn settings(&self) -> &K40Settings {
        &self.opts.settings
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
        if self.shared.job.lock().expect("zadanie").status == JobStatus::Running {
            return Err(DriverError::Busy("zadanie w toku".into()));
        }
        Ok(())
    }

    fn send_stream(&self, stream: &[u8]) -> Result<(), DriverError> {
        self.alive()?;
        let (rtx, rrx) = mpsc::channel();
        self.tx
            .send(Cmd::Send(egv::packets(stream), rtx))
            .map_err(|_| DriverError::NotConnected)?;
        rrx.recv_timeout(Duration::from_secs(120))
            .map_err(|_| DriverError::Timeout("płyta Lihuiyu".into()))?
    }

    /// Pozycja głowicy (mm dokumentu, Y w górę).
    fn mpos(&self) -> bc_geom::Point {
        let p = *self.shared.pos.lock().expect("pozycja");
        self.steps().mm((p.0 as f64, p.1 as f64))
    }

    fn cur_steps(&self) -> (i64, i64) {
        *self.shared.pos.lock().expect("pozycja")
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

    fn file_packets(file: &MachineFile) -> Result<Arc<Vec<Packet>>, DriverError> {
        if file.driver != "lihuiyu" {
            return Err(DriverError::Invalid(format!(
                "plik {} nie jest dla sterownika lihuiyu",
                file.driver
            )));
        }
        Ok(Arc::new(egv::packets(&file.bytes)))
    }

    /// Program z ruchami bez wiązki (ramka, przejazd z prędkością).
    fn program(&self, points: &[(i64, i64)], speed: f64) -> Vec<u8> {
        let code = egv::speed_code(self.opts.settings.board, speed, None);
        let mut e = Egv::new(self.cur_steps());
        e.enter(&code.text, points[0], false);
        for p in &points[1..] {
            e.line(*p, false);
        }
        e.leave(true);
        e.out
    }
}

impl Drop for LihuiyuDriver {
    fn drop(&mut self) {
        self.disconnect();
    }
}

impl Driver for LihuiyuDriver {
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
            pwm: self.opts.settings.board.pwm(),
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

    /// Bazowanie `IPP`. Źródło: MeerK40t `driver.py`.
    fn home(&mut self) -> Result<(), DriverError> {
        self.not_running()?;
        self.send_stream(b"IPP\n")
    }

    /// Odblokowanie wózka `IS2P`. Źródło: MeerK40t `driver.py`.
    fn unlock(&mut self) -> Result<(), DriverError> {
        self.not_running()?;
        self.send_stream(b"IS2P\n")
    }

    fn jog(&mut self, axis: Axis, distance: f64, _speed: f64) -> Result<(), DriverError> {
        self.not_running()?;
        let p = self.mpos();
        let to = match axis {
            Axis::X => bc_geom::Point::new(p.x + distance, p.y),
            Axis::Y => bc_geom::Point::new(p.x, p.y + distance),
            Axis::Z => return Err(DriverError::Unsupported("K40: brak osi Z".into())),
        };
        let mut e = Egv::new(self.cur_steps());
        e.rapid(self.steps().of(to));
        self.send_stream(&e.out)
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
        let t = self.steps().of(to);
        let out = match speed {
            None => {
                let mut e = Egv::new(self.cur_steps());
                e.rapid(t);
                e.out
            }
            Some(s) => self.program(&[self.cur_steps(), t], s),
        };
        self.send_stream(&out)
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
        let out = self.program(&pts, speed);
        self.send_stream(&out)
    }

    /// Krótki impuls: `IDS1P`, czekanie, `IUS1P` (moc z pokrętła; M2 nie ma PWM).
    /// Źródło: MeerK40t `driver.py` (laser on/off w trybie szybkim).
    fn test_fire(&mut self, _power_pct: f64, ms: u32) -> Result<(), DriverError> {
        self.not_running()?;
        self.send_stream(b"IDS1P\n")?;
        std::thread::sleep(Duration::from_millis(ms.min(2000) as u64));
        self.send_stream(b"IUS1P\n")
    }

    fn send_job(&mut self, file: &MachineFile, opts: JobOptions) -> Result<JobState, DriverError> {
        self.alive()?;
        self.not_running()?;
        let p = Self::file_packets(file)?;
        let id = self.next_id;
        self.next_id += 1;
        self.tx
            .send(Cmd::Job(p, opts, id))
            .map_err(|_| DriverError::NotConnected)?;
        let t0 = Instant::now();
        while self.job().id != id && t0.elapsed() < Duration::from_secs(2) {
            std::thread::sleep(Duration::from_millis(1));
        }
        Ok(self.job())
    }

    fn queue_job(&mut self, file: &MachineFile, opts: JobOptions) -> Result<usize, DriverError> {
        self.alive()?;
        let p = Self::file_packets(file)?;
        if matches!(self.job().status, JobStatus::Running | JobStatus::Paused) {
            let mut q = self.shared.queue.lock().expect("kolejka");
            q.push_back((p, opts));
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

    fn pause(&mut self) -> Result<(), DriverError> {
        self.alive()?;
        if self.job().status == JobStatus::Running {
            self.shared.want_pause.store(true, Ordering::SeqCst);
        }
        self.tx
            .send(Cmd::Pause)
            .map_err(|_| DriverError::NotConnected)
    }

    fn resume(&mut self) -> Result<(), DriverError> {
        self.alive()?;
        self.shared.want_pause.store(false, Ordering::SeqCst);
        self.tx
            .send(Cmd::Resume)
            .map_err(|_| DriverError::NotConnected)
    }

    fn stop(&mut self) {
        self.shared.stop.store(true, Ordering::SeqCst);
        let _ = self.tx.send(Cmd::Stop);
    }

    fn set_overrides(
        &mut self,
        _feed: Option<u32>,
        _power: Option<u32>,
    ) -> Result<(), DriverError> {
        Err(DriverError::Unsupported(
            "Lihuiyu: brak nadpisań w czasie rzeczywistym".into(),
        ))
    }

    fn read_settings(&mut self) -> Result<Vec<Setting>, DriverError> {
        let s = &self.opts.settings;
        let set = |id: &str, value: String, name: &str, unit: Option<&str>| Setting {
            id: id.into(),
            value,
            name: Some(name.into()),
            unit: unit.map(str::to_string),
            min: None,
            max: None,
        };
        Ok(vec![
            set("board", s.board.id().into(), "Płyta", None),
            set(
                "board_id",
                s.board_id.clone().unwrap_or_default(),
                "ID płyty",
                None,
            ),
            set("dpi", s.dpi.to_string(), "Kroki na cal", Some("dpi")),
            set("scale_x", s.scale[0].to_string(), "Korekcja skali X", None),
            set("scale_y", s.scale[1].to_string(), "Korekcja skali Y", None),
        ])
    }

    fn write_settings(
        &mut self,
        _changes: &[(String, String)],
        _backup_dir: Option<&std::path::Path>,
        _dry_run: bool,
    ) -> Result<WriteReport, DriverError> {
        Err(DriverError::Unsupported(
            "ustawienia płyty K40 są w profilu urządzenia (machine.profile_save)".into(),
        ))
    }

    fn probe_z(&mut self, _distance: f64, _feed: f64) -> Result<f64, DriverError> {
        Err(DriverError::Unsupported("K40: brak sondy Z".into()))
    }

    fn list_files(&mut self) -> Result<Vec<RemoteFile>, DriverError> {
        Err(DriverError::Unsupported(
            "Lihuiyu: brak pamięci plików".into(),
        ))
    }

    fn upload_file(&mut self, _name: &str, _data: &[u8]) -> Result<(), DriverError> {
        Err(DriverError::Unsupported(
            "Lihuiyu: brak pamięci plików".into(),
        ))
    }

    fn run_file(&mut self, _name: &str) -> Result<(), DriverError> {
        Err(DriverError::Unsupported(
            "Lihuiyu: brak pamięci plików".into(),
        ))
    }

    fn delete_file(&mut self, _name: &str) -> Result<(), DriverError> {
        Err(DriverError::Unsupported(
            "Lihuiyu: brak pamięci plików".into(),
        ))
    }

    /// Surowe polecenie EGV (jeden pakiet albo kilka).
    fn raw(&mut self, line: &str) -> Result<Vec<String>, DriverError> {
        if !line.is_ascii() || line.contains('\n') {
            return Err(DriverError::Invalid(
                "polecenie EGV: jeden wiersz ASCII".into(),
            ));
        }
        let mut s = line.as_bytes().to_vec();
        s.push(b'\n');
        self.send_stream(&s)?;
        Ok(vec!["OK".into()])
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
        Ok(crate::compile("lihuiyu", job, &c)?)
    }
}
