//! Voice transport primitives (not in the original): the in-band fast path for 0xBC `VOICE_FRAME` packets and an
//! optional dedicated UDP voice socket. Only bytes move here; relay policy and crypto live in the voice relay
//! (mod-sdk `bhmod-voice`), which plugs in through [`VoiceSink`] and [`UdpVoiceHandler`].
//!
//! Zero effect: without an installed mod host nothing here runs. With one, a `VOICE_FRAME` is still dropped (as
//! every unknown op is) until a [`VoiceSink`] is installed, and the UDP socket is only bound when
//! [`VoiceConfig::udp_port`] is set.

use std::cell::RefCell;
use std::fmt;
use std::io;
use std::net::{Ipv4Addr, SocketAddr, UdpSocket};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, PoisonError, RwLock};
use std::thread::JoinHandle;
use std::time::Duration;

/// The 0xBC op of a voice frame (`VOICE_FRAME`, unreliable and unsequenced in both directions).
pub const VOICE_FRAME: u8 = 0x16;

/// Largest packet sent unreliable: ENet's fragment threshold at MTU 1392. A bigger unsequenced packet would leave
/// as reliable fragments, so it is refused instead.
pub const MAX_UNRELIABLE: usize = 1364;

/// Hard cap of the Opus payload of one voice frame (20 ms at the highest allowed bitrate fits easily).
pub const MAX_OPUS: usize = 400;

/// How long the UDP thread waits for a datagram before it looks at its stop flag.
const UDP_POLL: Duration = Duration::from_millis(50);

/// Largest datagram the UDP thread reads (bigger ones are cut and should be dropped by the handler).
pub const MAX_DATAGRAM: usize = 1500;

/// Where a [`VoiceSink`] sends what it relays: unsequenced 0xBC packets on ENet channel 0.
pub trait VoiceOut {
    /// Sends one full 0xBC packet (`bc 16 ...`) to `peer`, a modded player in the game. False when it was not
    /// sent: not a 0xBC packet, longer than [`MAX_UNRELIABLE`], the peer has no mod loader, is not in the game or
    /// is gone.
    fn send(&mut self, peer: &str, packet: &[u8]) -> bool;
}

/// The in-band voice relay. Runs on the server thread, once per `VOICE_FRAME`, before any mod is asked.
pub trait VoiceSink {
    /// A `VOICE_FRAME` from `peer` (a modded player in the game, named as the dispatcher names it). `body` is the
    /// packet after `bc 16`.
    fn frame(&mut self, peer: &str, body: &[u8], out: &mut dyn VoiceOut);
}

/// Builds the in-band relay on the server thread when the mod host starts.
pub type VoiceSinkFactory = Arc<dyn Fn() -> Box<dyn VoiceSink> + Send + Sync>;

/// Sends datagrams from the voice UDP socket.
pub trait UdpOut {
    /// Sends `data` to `to`. False when it is longer than [`MAX_UNRELIABLE`] or the socket refused it.
    fn send_to(&self, to: SocketAddr, data: &[u8]) -> bool;
}

/// The UDP voice relay. Runs on the voice thread; shared state with the server thread goes through `Arc`s it
/// holds. It maps a sender address to a session (e.g. by a token in the datagram) itself.
pub trait UdpVoiceHandler: Send + Sync {
    /// One datagram from `from`.
    fn datagram(&self, from: SocketAddr, data: &[u8], out: &dyn UdpOut);
}

/// Voice settings of a hosted server (`HostConfig.voice`).
#[derive(Clone, Default)]
pub struct VoiceConfig {
    /// `voice_udp_port`: the port of the dedicated voice socket. `None` (the default) binds nothing; `Some(0)`
    /// takes any free port.
    pub udp_port: Option<u16>,
    /// The in-band relay, built when the mod host starts.
    pub sink: Option<VoiceSinkFactory>,
    /// The UDP relay (it can also be set later with [`set_udp_handler`]).
    pub udp_handler: Option<Arc<dyn UdpVoiceHandler>>,
}

impl fmt::Debug for VoiceConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("VoiceConfig")
            .field("udp_port", &self.udp_port)
            .field("sink", &self.sink.is_some())
            .field("udp_handler", &self.udp_handler.is_some())
            .finish()
    }
}

/// A cloneable sender on the voice UDP socket, usable from any thread.
#[derive(Clone, Debug)]
pub struct UdpSender(Arc<UdpSocket>);

impl UdpOut for UdpSender {
    fn send_to(&self, to: SocketAddr, data: &[u8]) -> bool {
        data.len() <= MAX_UNRELIABLE && self.0.send_to(data, to).is_ok()
    }
}

type HandlerSlot = Arc<RwLock<Option<Arc<dyn UdpVoiceHandler>>>>;

/// The dedicated voice socket and its thread. Dropping it stops the thread (within [`UDP_POLL`]).
pub struct VoiceUdp {
    sender: UdpSender,
    handler: HandlerSlot,
    stop: Arc<AtomicBool>,
    join: Option<JoinHandle<()>>,
}

impl VoiceUdp {
    /// Binds `0.0.0.0:port` and starts the voice thread.
    pub fn bind(port: u16, handler: Option<Arc<dyn UdpVoiceHandler>>) -> io::Result<VoiceUdp> {
        let socket = Arc::new(UdpSocket::bind((Ipv4Addr::UNSPECIFIED, port))?);
        socket.set_read_timeout(Some(UDP_POLL))?;
        let sender = UdpSender(socket);
        let handler: HandlerSlot = Arc::new(RwLock::new(handler));
        let stop = Arc::new(AtomicBool::new(false));
        let (s, h, st) = (sender.clone(), handler.clone(), stop.clone());
        let join = std::thread::Builder::new().name("bh-voice-udp".into()).spawn(move || udp_loop(&s, &h, &st))?;
        Ok(VoiceUdp { sender, handler, stop, join: Some(join) })
    }

    /// The bound port.
    pub fn local_port(&self) -> u16 {
        self.sender.0.local_addr().map(|a| a.port()).unwrap_or(0)
    }

    /// A sender on the socket.
    pub fn sender(&self) -> UdpSender {
        self.sender.clone()
    }

    /// Replaces the handler (`None`: datagrams are dropped).
    pub fn set_handler(&self, h: Option<Arc<dyn UdpVoiceHandler>>) {
        *self.handler.write().unwrap_or_else(PoisonError::into_inner) = h;
    }
}

impl Drop for VoiceUdp {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(j) = self.join.take() {
            let _ = j.join();
        }
    }
}

fn udp_loop(sock: &UdpSender, handler: &HandlerSlot, stop: &AtomicBool) {
    let mut buf = [0u8; MAX_DATAGRAM];
    while !stop.load(Ordering::Relaxed) {
        // Timeouts, and on some systems ICMP errors of earlier sends, come back as errors: keep going.
        if let Ok((n, from)) = sock.0.recv_from(&mut buf) {
            let h = handler.read().unwrap_or_else(PoisonError::into_inner).clone();
            if let Some(h) = h {
                h.datagram(from, &buf[..n], sock);
            }
        }
    }
}

thread_local! {
    static CONFIG: RefCell<VoiceConfig> = RefCell::new(VoiceConfig::default());
    static SINK: RefCell<Option<Box<dyn VoiceSink>>> = const { RefCell::new(None) };
    static UDP: RefCell<Option<VoiceUdp>> = const { RefCell::new(None) };
}

/// Sets the voice configuration of this server thread (the embedding API calls it before the world starts).
pub fn set_config(c: VoiceConfig) {
    CONFIG.with(|x| *x.borrow_mut() = c);
}

/// The voice configuration of this server thread.
pub fn config() -> VoiceConfig {
    CONFIG.with(|x| x.borrow().clone())
}

/// Installs (or removes) the in-band relay of this server thread.
pub fn set_sink(sink: Option<Box<dyn VoiceSink>>) {
    SINK.with(|s| *s.borrow_mut() = sink);
}

/// True when an in-band relay is installed.
pub fn has_sink() -> bool {
    SINK.with(|s| s.try_borrow().map(|s| s.is_some()).unwrap_or(true))
}

/// Hands a `VOICE_FRAME` body to the relay. False when there is none (or it is running already: a relay that
/// re-enters is not called again).
pub fn dispatch(peer: &str, body: &[u8], out: &mut dyn VoiceOut) -> bool {
    SINK.with(|s| match s.try_borrow_mut().as_deref_mut() {
        Ok(Some(sink)) => {
            sink.frame(peer, body, out);
            true
        }
        _ => false,
    })
}

/// The port of the running voice socket, if any.
pub fn udp_port() -> Option<u16> {
    UDP.with(|u| u.borrow().as_ref().map(VoiceUdp::local_port))
}

/// A sender on the running voice socket, if any.
pub fn udp_sender() -> Option<UdpSender> {
    UDP.with(|u| u.borrow().as_ref().map(VoiceUdp::sender))
}

/// Replaces the handler of the running voice socket. False when no socket runs.
pub fn set_udp_handler(h: Option<Arc<dyn UdpVoiceHandler>>) -> bool {
    UDP.with(|u| u.borrow().as_ref().map(|u| u.set_handler(h)).is_some())
}

/// The mod host started: builds the relay and binds the voice socket as configured. Returns a warning for the log
/// when the socket could not be bound.
pub(crate) fn start() -> Option<String> {
    stop();
    let c = config();
    set_sink(c.sink.map(|f| f()));
    let port = c.udp_port?;
    match VoiceUdp::bind(port, c.udp_handler) {
        Ok(u) => {
            UDP.with(|x| *x.borrow_mut() = Some(u));
            None
        }
        Err(e) => Some(format!("voice: UDP port {port} could not be opened: {e}")),
    }
}

/// The mod host stopped: drops the relay and closes the voice socket.
pub(crate) fn stop() {
    set_sink(None);
    // Taken out first: the thread is joined outside the borrow.
    let udp = UDP.with(|u| u.borrow_mut().take());
    drop(udp);
}

#[cfg(test)]
mod tests;
