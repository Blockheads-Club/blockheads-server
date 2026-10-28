//! Voice transport primitives (not in the original): the dedicated UDP voice socket and its thread. Voice frames
//! travel only there; control (`VOICE_KEY`, `VOICE_STATE`, `VOICE_CTRL`) stays on reliable 0xBC, and an in-band
//! `VOICE_FRAME` (op 0x16) over ENet is dropped. Only bytes move here; relay policy and crypto live in the voice
//! relay (mod-sdk `bhmod-voice`), which plugs in through [`UdpVoiceHandler`] (see `voice_glue.rs`).
//!
//! Zero effect: without an installed mod host nothing here runs, and the socket is only bound while a loaded mod
//! holds the `voice` permission.

use std::cell::RefCell;
use std::fmt;
use std::io;
use std::net::{Ipv4Addr, SocketAddr, UdpSocket};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;

/// The 0xBC op of an in-band voice frame (`VOICE_FRAME`). Frames go over the voice socket only: this op is dropped.
pub const VOICE_FRAME: u8 = 0x16;

/// Largest voice datagram sent (below a typical path MTU, so it is never fragmented).
pub const MAX_SEND: usize = 1364;

/// How long the UDP thread waits for a datagram before it looks at its stop flag.
const UDP_POLL: Duration = Duration::from_millis(50);

/// Largest datagram the UDP thread reads (bigger ones are cut and should be dropped by the handler).
pub const MAX_DATAGRAM: usize = 1500;

/// Sends datagrams from the voice UDP socket.
pub trait UdpOut {
    /// Sends `data` to `to`. False when it is longer than [`MAX_SEND`] or the socket refused it.
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
    /// `voice_udp_port`: the port of the dedicated voice socket, bound while a loaded mod holds the `voice`
    /// permission (the voice mod's own `udp_port` setting wins). `None` (the default): the game port + 1;
    /// `Some(0)` takes any free port.
    pub udp_port: Option<u16>,
    /// The UDP relay of an embedder, used instead of the built-in one.
    pub udp_handler: Option<Arc<dyn UdpVoiceHandler>>,
}

impl fmt::Debug for VoiceConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("VoiceConfig")
            .field("udp_port", &self.udp_port)
            .field("udp_handler", &self.udp_handler.is_some())
            .finish()
    }
}

/// A cloneable sender on the voice UDP socket, usable from any thread.
#[derive(Clone, Debug)]
pub struct UdpSender(Arc<UdpSocket>);

impl UdpOut for UdpSender {
    fn send_to(&self, to: SocketAddr, data: &[u8]) -> bool {
        data.len() <= MAX_SEND && self.0.send_to(data, to).is_ok()
    }
}

/// The dedicated voice socket and its thread. Dropping it stops the thread (within [`UDP_POLL`]).
pub struct VoiceUdp {
    sender: UdpSender,
    stop: Arc<AtomicBool>,
    join: Option<JoinHandle<()>>,
}

impl VoiceUdp {
    /// Binds `0.0.0.0:port` and starts the voice thread.
    pub fn bind(port: u16, handler: Option<Arc<dyn UdpVoiceHandler>>) -> io::Result<VoiceUdp> {
        let socket = Arc::new(UdpSocket::bind((Ipv4Addr::UNSPECIFIED, port))?);
        socket.set_read_timeout(Some(UDP_POLL))?;
        let sender = UdpSender(socket);
        let stop = Arc::new(AtomicBool::new(false));
        let (s, st) = (sender.clone(), stop.clone());
        let join = std::thread::Builder::new().name("bh-voice-udp".into()).spawn(move || udp_loop(&s, handler, &st))?;
        Ok(VoiceUdp { sender, stop, join: Some(join) })
    }

    /// The bound port.
    pub fn local_port(&self) -> u16 {
        self.sender.0.local_addr().map(|a| a.port()).unwrap_or(0)
    }

    /// A sender on the socket.
    pub fn sender(&self) -> UdpSender {
        self.sender.clone()
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

/// Reads datagrams until stopped; without a handler they are dropped.
fn udp_loop(sock: &UdpSender, handler: Option<Arc<dyn UdpVoiceHandler>>, stop: &AtomicBool) {
    let mut buf = [0u8; MAX_DATAGRAM];
    while !stop.load(Ordering::Relaxed) {
        // Timeouts, and on some systems ICMP errors of earlier sends, come back as errors: keep going.
        if let Ok((n, from)) = sock.0.recv_from(&mut buf) {
            if let Some(h) = &handler {
                h.datagram(from, &buf[..n], sock);
            }
        }
    }
}

thread_local! {
    static CONFIG: RefCell<VoiceConfig> = RefCell::new(VoiceConfig::default());
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

/// The port of the running voice socket, if any.
pub fn udp_port() -> Option<u16> {
    UDP.with(|u| u.borrow().as_ref().map(VoiceUdp::local_port))
}

/// A sender on the running voice socket, if any.
pub fn udp_sender() -> Option<UdpSender> {
    UDP.with(|u| u.borrow().as_ref().map(VoiceUdp::sender))
}

/// Opens the voice socket on `port` (`0`: any free port) with `handler`, closing the one that runs. Returns the bound
/// port.
pub fn open_udp(port: u16, handler: Option<Arc<dyn UdpVoiceHandler>>) -> io::Result<u16> {
    close_udp();
    let u = VoiceUdp::bind(port, handler)?;
    let got = u.local_port();
    UDP.with(|x| *x.borrow_mut() = Some(u));
    Ok(got)
}

/// Closes the voice socket, if one runs.
pub fn close_udp() {
    // Taken out first: the thread is joined outside the borrow.
    let udp = UDP.with(|u| u.borrow_mut().take());
    drop(udp);
}

/// The mod host stopped: closes the voice socket.
pub(crate) fn stop() {
    close_udp();
}

#[cfg(test)]
mod tests;
