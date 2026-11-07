//! Transporty (ADR 0010 §2): serial / USB-CDC, TCP (Telnet 23), WebSocket (FluidNC,
//! ESP3D) i emulator w procesie. Każdy transport to strumień bajtów z odczytem z limitem
//! czasu; protokół (GRBL) jest w wątku połączenia sterownika.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::emu::GrblEmulator;

/// Błąd transportu.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
#[error("{0}")]
pub struct TransportError(pub String);

impl From<std::io::Error> for TransportError {
    fn from(e: std::io::Error) -> Self {
        TransportError(e.to_string())
    }
}

/// Strumień bajtów do maszyny.
pub trait Transport: Send {
    fn write(&mut self, data: &[u8]) -> Result<(), TransportError>;
    /// Odczyt dostępnych bajtów (czeka najwyżej `timeout`; 0 bajtów = brak danych).
    fn read(&mut self, buf: &mut [u8], timeout: Duration) -> Result<usize, TransportError>;
    fn describe(&self) -> String;
}

/// Opis transportu w profilu urządzenia.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum TransportSpec {
    /// Port szeregowy / USB-CDC (np. COM3, /dev/ttyUSB0).
    Serial {
        port: String,
        #[serde(default = "d_baud")]
        baud: u32,
    },
    /// TCP (Telnet, domyślnie port 23).
    Tcp {
        host: String,
        #[serde(default = "d_telnet")]
        port: u16,
    },
    /// WebSocket (FluidNC, ESP3D), np. ws://192.168.4.1:81.
    Websocket { url: String },
    /// USB bulk (REQ-13-019), np. CH341 K40 `1a86:5512`; interfejs i punkty końcowe z rodziny
    /// urządzenia, chyba że podane.
    Usb {
        vid: u16,
        pid: u16,
        /// Numer seryjny, gdy podłączonych jest kilka takich samych urządzeń.
        #[serde(default)]
        serial: Option<String>,
    },
    /// UDP (REQ-13-023), np. Ruida: port docelowy 50200, lokalny 40200.
    Udp {
        host: String,
        port: u16,
        /// Port lokalny (brak = dowolny wolny).
        #[serde(default)]
        local_port: Option<u16>,
    },
    /// Emulator GRBL w procesie (urządzenie fikcyjne).
    Emulator {
        /// Odmiana z tabeli (grbl11, grblhal, fluidnc, …).
        #[serde(default = "d_variant")]
        variant: String,
        /// Mnożnik czasu (brak = natychmiast).
        #[serde(default)]
        speed: Option<f64>,
    },
}

fn d_baud() -> u32 {
    115200
}
fn d_telnet() -> u16 {
    23
}
fn d_variant() -> String {
    "grbl11".into()
}

impl TransportSpec {
    pub fn describe(&self) -> String {
        match self {
            TransportSpec::Serial { port, baud } => format!("serial {port} @ {baud}"),
            TransportSpec::Tcp { host, port } => format!("tcp {host}:{port}"),
            TransportSpec::Websocket { url } => format!("websocket {url}"),
            TransportSpec::Usb { vid, pid, .. } => format!("usb {vid:04x}:{pid:04x}"),
            TransportSpec::Udp { host, port, .. } => format!("udp {host}:{port}"),
            TransportSpec::Emulator { variant, .. } => format!("emulator {variant}"),
        }
    }
}

// ---------- serial ----------

/// Port szeregowy (crate `serialport`).
pub struct SerialTransport {
    port: Box<dyn serialport::SerialPort>,
    name: String,
}

impl SerialTransport {
    pub fn open(port: &str, baud: u32) -> Result<Self, TransportError> {
        let p = serialport::new(port, baud)
            .timeout(Duration::from_millis(20))
            .open()
            .map_err(|e| TransportError(format!("{port}: {e}")))?;
        Ok(SerialTransport::from_port(p, port))
    }

    /// Transport z otwartego portu (także atrapy w testach).
    pub fn from_port(port: Box<dyn serialport::SerialPort>, name: &str) -> Self {
        SerialTransport {
            port,
            name: name.into(),
        }
    }
}

impl Transport for SerialTransport {
    fn write(&mut self, data: &[u8]) -> Result<(), TransportError> {
        self.port.write_all(data)?;
        self.port.flush()?;
        Ok(())
    }

    fn read(&mut self, buf: &mut [u8], timeout: Duration) -> Result<usize, TransportError> {
        let _ = self.port.set_timeout(timeout.max(Duration::from_millis(1)));
        match self.port.read(buf) {
            Ok(n) => Ok(n),
            Err(e) if e.kind() == std::io::ErrorKind::TimedOut => Ok(0),
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => Ok(0),
            Err(e) => Err(e.into()),
        }
    }

    fn describe(&self) -> String {
        format!("serial {}", self.name)
    }
}

/// Port szeregowy znaleziony w systemie.
#[derive(Debug, Clone, PartialEq, Serialize, schemars::JsonSchema)]
pub struct PortInfo {
    pub port: String,
    pub vid: Option<u16>,
    pub pid: Option<u16>,
    /// FTDI, CH340, CP210x, … (z VID/PID).
    pub chip: Option<String>,
    pub product: Option<String>,
}

/// Układ USB-serial z VID/PID (REQ-13-018).
pub fn usb_chip(vid: u16, pid: u16) -> Option<&'static str> {
    match (vid, pid) {
        (0x0403, _) => Some("FTDI"),
        (0x1A86, 0x7523) | (0x1A86, 0x7522) | (0x1A86, 0x5523) | (0x1A86, 0x55D4) => Some("CH340"),
        (0x1A86, _) => Some("WCH"),
        (0x10C4, 0xEA60) | (0x10C4, 0xEA70) | (0x10C4, _) => Some("CP210x"),
        (0x2341, _) | (0x2A03, _) => Some("Arduino"),
        (0x0483, _) => Some("STM32 CDC"),
        (0x303A, _) => Some("ESP32 USB"),
        (0x1D50, _) => Some("USB-CDC (OpenMoko)"),
        _ => None,
    }
}

/// Lista portów szeregowych systemu.
pub fn list_serial_ports() -> Vec<PortInfo> {
    serialport::available_ports()
        .unwrap_or_default()
        .into_iter()
        .map(|p| match p.port_type {
            serialport::SerialPortType::UsbPort(u) => PortInfo {
                chip: usb_chip(u.vid, u.pid).map(str::to_string),
                vid: Some(u.vid),
                pid: Some(u.pid),
                product: u.product,
                port: p.port_name,
            },
            _ => PortInfo {
                port: p.port_name,
                vid: None,
                pid: None,
                chip: None,
                product: None,
            },
        })
        .collect()
}

// ---------- TCP ----------

pub struct TcpTransport {
    s: TcpStream,
    name: String,
}

impl TcpTransport {
    pub fn connect(host: &str, port: u16) -> Result<Self, TransportError> {
        use std::net::ToSocketAddrs;
        let addr = (host, port)
            .to_socket_addrs()
            .map_err(|e| TransportError(format!("{host}:{port}: {e}")))?
            .next()
            .ok_or_else(|| TransportError(format!("{host}:{port}: brak adresu")))?;
        let s = TcpStream::connect_timeout(&addr, Duration::from_secs(5))
            .map_err(|e| TransportError(format!("{host}:{port}: {e}")))?;
        s.set_nodelay(true).ok();
        Ok(TcpTransport {
            s,
            name: format!("{host}:{port}"),
        })
    }
}

impl Transport for TcpTransport {
    fn write(&mut self, data: &[u8]) -> Result<(), TransportError> {
        self.s.write_all(data)?;
        Ok(())
    }

    fn read(&mut self, buf: &mut [u8], timeout: Duration) -> Result<usize, TransportError> {
        self.s
            .set_read_timeout(Some(timeout.max(Duration::from_millis(1))))?;
        match self.s.read(buf) {
            Ok(0) => Err(TransportError("połączenie zamknięte".into())),
            Ok(n) => Ok(n),
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) =>
            {
                Ok(0)
            }
            Err(e) => Err(e.into()),
        }
    }

    fn describe(&self) -> String {
        format!("tcp {}", self.name)
    }
}

// ---------- UDP ----------

/// UDP (REQ-13-023): każdy zapis to jeden datagram do `host:port`; odczyt zwraca kolejne
/// bajty datagramów od tego nadawcy (inne są pomijane).
pub struct UdpTransport {
    sock: std::net::UdpSocket,
    peer: std::net::SocketAddr,
    pending: std::collections::VecDeque<u8>,
    name: String,
}

impl UdpTransport {
    pub fn connect(host: &str, port: u16, local_port: Option<u16>) -> Result<Self, TransportError> {
        use std::net::ToSocketAddrs;
        let peer = (host, port)
            .to_socket_addrs()
            .map_err(|e| TransportError(format!("{host}:{port}: {e}")))?
            .next()
            .ok_or_else(|| TransportError(format!("{host}:{port}: brak adresu")))?;
        let any: std::net::IpAddr = if peer.is_ipv4() {
            std::net::Ipv4Addr::UNSPECIFIED.into()
        } else {
            std::net::Ipv6Addr::UNSPECIFIED.into()
        };
        let sock = std::net::UdpSocket::bind((any, local_port.unwrap_or(0))).map_err(|e| {
            TransportError(format!("UDP port lokalny {}: {e}", local_port.unwrap_or(0)))
        })?;
        Ok(UdpTransport {
            sock,
            peer,
            pending: std::collections::VecDeque::new(),
            name: format!("{host}:{port}"),
        })
    }
}

impl Transport for UdpTransport {
    fn write(&mut self, data: &[u8]) -> Result<(), TransportError> {
        self.sock.send_to(data, self.peer)?;
        Ok(())
    }

    fn read(&mut self, buf: &mut [u8], timeout: Duration) -> Result<usize, TransportError> {
        let deadline = Instant::now() + timeout;
        let mut dgram = [0u8; 65536];
        while self.pending.is_empty() {
            let left = deadline.saturating_duration_since(Instant::now());
            self.sock
                .set_read_timeout(Some(left.max(Duration::from_millis(1))))?;
            match self.sock.recv_from(&mut dgram) {
                Ok((n, from)) if from == self.peer => self.pending.extend(&dgram[..n]),
                Ok(_) => {}
                // Windows: ICMP „port nieosiągalny” po wysłaniu do zamkniętego portu
                Err(e)
                    if matches!(
                        e.kind(),
                        std::io::ErrorKind::WouldBlock
                            | std::io::ErrorKind::TimedOut
                            | std::io::ErrorKind::ConnectionReset
                    ) => {}
                Err(e) => return Err(e.into()),
            }
            if Instant::now() >= deadline {
                break;
            }
        }
        let n = self.pending.len().min(buf.len());
        for (i, b) in self.pending.drain(..n).enumerate() {
            buf[i] = b;
        }
        Ok(n)
    }

    fn describe(&self) -> String {
        format!("udp {}", self.name)
    }
}
// ---------- WebSocket ----------

type Ws = tungstenite::WebSocket<tungstenite::stream::MaybeTlsStream<TcpStream>>;

pub struct WsTransport {
    ws: Ws,
    url: String,
    pending: Vec<u8>,
}

impl WsTransport {
    pub fn connect(url: &str) -> Result<Self, TransportError> {
        let (ws, _) =
            tungstenite::connect(url).map_err(|e| TransportError(format!("{url}: {e}")))?;
        Ok(WsTransport {
            ws,
            url: url.into(),
            pending: Vec::new(),
        })
    }

    fn stream(&mut self) -> &TcpStream {
        match self.ws.get_ref() {
            tungstenite::stream::MaybeTlsStream::Plain(s) => s,
            _ => unreachable!("wss nieobsługiwane"),
        }
    }
}

impl Transport for WsTransport {
    fn write(&mut self, data: &[u8]) -> Result<(), TransportError> {
        // FluidNC/ESP3D: polecenia jako ramki tekstowe
        let text = String::from_utf8_lossy(data).to_string();
        self.ws
            .send(tungstenite::Message::text(text))
            .map_err(|e| TransportError(e.to_string()))
    }

    fn read(&mut self, buf: &mut [u8], timeout: Duration) -> Result<usize, TransportError> {
        if self.pending.is_empty() {
            let t = timeout.max(Duration::from_millis(1));
            self.stream().set_read_timeout(Some(t))?;
            match self.ws.read() {
                Ok(tungstenite::Message::Text(t)) => self.pending.extend_from_slice(t.as_bytes()),
                Ok(tungstenite::Message::Binary(b)) => self.pending.extend_from_slice(&b),
                Ok(tungstenite::Message::Close(_)) => {
                    return Err(TransportError("połączenie zamknięte".into()))
                }
                Ok(_) => {}
                Err(tungstenite::Error::Io(e))
                    if matches!(
                        e.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                    ) => {}
                Err(e) => return Err(TransportError(e.to_string())),
            }
        }
        let n = self.pending.len().min(buf.len());
        buf[..n].copy_from_slice(&self.pending[..n]);
        self.pending.drain(..n);
        Ok(n)
    }

    fn describe(&self) -> String {
        format!("websocket {}", self.url)
    }
}

// ---------- emulator ----------

/// Emulator współdzielony (testy mogą go oglądać w trakcie połączenia).
pub type SharedEmulator = Arc<Mutex<GrblEmulator>>;

/// Transport do emulatora w procesie.
pub struct EmuTransport {
    emu: SharedEmulator,
    last: Instant,
}

impl EmuTransport {
    pub fn new(emu: SharedEmulator) -> Self {
        EmuTransport {
            emu,
            last: Instant::now(),
        }
    }
}

impl Transport for EmuTransport {
    fn write(&mut self, data: &[u8]) -> Result<(), TransportError> {
        self.emu.lock().expect("emulator").feed(data);
        Ok(())
    }

    fn read(&mut self, buf: &mut [u8], timeout: Duration) -> Result<usize, TransportError> {
        let deadline = Instant::now() + timeout;
        loop {
            let out = {
                let mut e = self.emu.lock().expect("emulator");
                let now = Instant::now();
                e.advance((now - self.last).as_secs_f64());
                self.last = now;
                e.take_output_max(buf.len())
            };
            if !out.is_empty() {
                buf[..out.len()].copy_from_slice(&out);
                return Ok(out.len());
            }
            if Instant::now() >= deadline {
                return Ok(0);
            }
            std::thread::sleep(Duration::from_millis(1));
        }
    }

    fn describe(&self) -> String {
        format!(
            "emulator {}",
            self.emu.lock().expect("emulator").cfg.variant.id
        )
    }
}

/// Serwer TCP emulatora (`burncraft-cli emulator --tcp`, testy transportu TCP).
/// Obsługuje kolejne połączenia do zamknięcia nasłuchu.
pub fn serve_tcp(listener: TcpListener, emu: SharedEmulator) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        for conn in listener.incoming() {
            let Ok(mut s) = conn else { break };
            s.set_nodelay(true).ok();
            s.set_read_timeout(Some(Duration::from_millis(2))).ok();
            emu.lock().expect("emulator").greet();
            let mut last = Instant::now();
            let mut buf = [0u8; 4096];
            loop {
                match s.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => emu.lock().expect("emulator").feed(&buf[..n]),
                    Err(e)
                        if matches!(
                            e.kind(),
                            std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                        ) => {}
                    Err(_) => break,
                }
                let out = {
                    let mut e = emu.lock().expect("emulator");
                    let now = Instant::now();
                    e.advance((now - last).as_secs_f64());
                    last = now;
                    e.take_output()
                };
                if !out.is_empty() && s.write_all(&out).is_err() {
                    break;
                }
            }
        }
    })
}

/// Serwer WebSocket emulatora (testy transportu WebSocket; jak FluidNC: ramki tekstowe).
pub fn serve_ws(listener: TcpListener, emu: SharedEmulator) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        for conn in listener.incoming() {
            let Ok(s) = conn else { break };
            let Ok(mut ws) = tungstenite::accept(s) else {
                continue;
            };
            ws.get_ref()
                .set_read_timeout(Some(Duration::from_millis(2)))
                .ok();
            emu.lock().expect("emulator").greet();
            let mut last = Instant::now();
            loop {
                match ws.read() {
                    Ok(tungstenite::Message::Text(t)) => {
                        emu.lock().expect("emulator").feed(t.as_bytes())
                    }
                    Ok(tungstenite::Message::Binary(b)) => emu.lock().expect("emulator").feed(&b),
                    Ok(tungstenite::Message::Close(_)) => break,
                    Ok(_) => {}
                    Err(tungstenite::Error::Io(e))
                        if matches!(
                            e.kind(),
                            std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                        ) => {}
                    Err(_) => break,
                }
                let out = {
                    let mut e = emu.lock().expect("emulator");
                    let now = Instant::now();
                    e.advance((now - last).as_secs_f64());
                    last = now;
                    e.take_output()
                };
                if !out.is_empty()
                    && ws
                        .send(tungstenite::Message::text(
                            String::from_utf8_lossy(&out).to_string(),
                        ))
                        .is_err()
                {
                    break;
                }
            }
        }
    })
}
