//! The voice relay of the server (API 0.3.4, not in the original): mod-sdk's host-agnostic
//! [`VoiceServer`](bhmod_voice::server::VoiceServer) glued to the transport primitives of [`super::voice`].
//!
//! Voice frames travel only over the dedicated UDP voice socket; control (`VOICE_KEY`, `VOICE_STATE`,
//! `VOICE_CTRL`) stays on reliable 0xBC. The relay never sends an in-band frame.
//!
//! - [`VoiceGlue`] is the `voice` side of the mod bridge ([`VoiceHost`]): it forwards the voice mod's policy to the
//!   shared voice server. Of `set-transport` only the port counts ([`VoiceGlue::wanted_port`]: the mod's non-zero
//!   `udp_port`, else `HostConfig.voice.udp_port`, else the game port + 1); [`VoiceGlue::sync_udp`] opens or moves
//!   the socket to match once per poll and the relay announces the port it really got.
//! - [`UdpGlue`] is the voice socket's handler ([`UdpVoiceHandler`]): it runs on the socket's thread, binds a peer
//!   to the address it authenticated from, sends datagrams itself and queues the reliable packets it produces for
//!   the server thread ([`VoiceGlue::drain`]).
//!
//! Nothing here runs unless a loaded mod holds the `voice` permission and the socket is bound (the server then
//! lists `voice` in `SERVER_HELLO`); vanilla players never get a voice byte.

use std::cell::Cell;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{SystemTime, UNIX_EPOCH};

use bhmod_core::types::ApiError;
use bhmod_core::voice::{Caps, Icons, Route, Transport, TransportConfig, VoiceEvent, VoiceHost, VoiceState as State};
use bhmod_voice::server::{ServerSink, SharedVoiceServer, VoiceServer};
use bhmod_voice::PeerId;

use super::voice::{UdpOut, UdpVoiceHandler};

/// Unix time in milliseconds (admin mutes end at a Unix time).
pub fn unix_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64)
}

/// A reliable 0xBC packet the UDP thread produced for the server thread: `(player, packet)`.
pub type Queued = (String, Vec<u8>);

type Bound = Arc<Mutex<HashMap<PeerId, SocketAddr>>>;

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Where packets of the server thread go: reliable 0xBC packets through `send` (the server's link), datagrams to
/// the bound address of a peer on the voice socket.
pub struct ThreadSink<'a> {
    /// Sends a reliable packet to a player.
    pub send: &'a mut dyn FnMut(&str, Vec<u8>),
    /// Peer -> address on the voice socket.
    pub bound: &'a Bound,
    /// The voice socket's sender, if one is open.
    pub udp: Option<&'a dyn UdpOut>,
}

impl ServerSink for ThreadSink<'_> {
    fn reliable(&mut self, player: &str, packet: Vec<u8>) {
        (self.send)(player, packet);
    }
    fn udp(&mut self, peer: PeerId, datagram: Vec<u8>) {
        let to = lock(self.bound).get(&peer).copied();
        if let (Some(u), Some(to)) = (self.udp, to) {
            u.send_to(to, &datagram);
        }
    }
}

/// The voice state of the server's mod host.
pub struct VoiceGlue {
    /// The relay, shared with the UDP thread.
    pub server: SharedVoiceServer,
    /// The `udp_port` of the mod's last `set-transport` (`None`, `Some(0)`: not set).
    requested: Cell<Option<u16>>,
    /// The port the voice socket was opened for (the request, not the bound port: `0` = any).
    udp_for: Cell<Option<u16>>,
    bound: Bound,
    queue: Arc<Mutex<Vec<Queued>>>,
}

impl Default for VoiceGlue {
    fn default() -> VoiceGlue {
        VoiceGlue::new(VoiceServer::new())
    }
}

impl VoiceGlue {
    /// A relay over `server`, its socket closed.
    pub fn new(server: VoiceServer) -> VoiceGlue {
        VoiceGlue {
            server: SharedVoiceServer::new(server),
            requested: Cell::new(None),
            udp_for: Cell::new(None),
            bound: Bound::default(),
            queue: Arc::default(),
        }
    }

    /// The peer -> address map of the voice socket.
    pub fn bound(&self) -> &Bound {
        &self.bound
    }

    /// The handler of the voice socket.
    pub fn udp_handler(&self) -> UdpGlue {
        UdpGlue { server: self.server.clone(), bound: self.bound.clone(), queue: self.queue.clone() }
    }

    /// Queues packets as the UDP thread does (tests).
    #[cfg(test)]
    pub fn queue_for_test(&self, items: impl IntoIterator<Item = Queued>) {
        lock(&self.queue).extend(items);
    }

    /// The packets the UDP thread queued for the server thread.
    pub fn drain(&self) -> Vec<Queued> {
        std::mem::take(&mut *lock(&self.queue))
    }

    /// A player is in the game.
    pub fn join(&self, player: &str, alias: &str, features: &[String]) {
        self.server.lock().join(player, alias, features);
    }

    /// A player left: the relay forgets them, the voice socket forgets their address.
    pub fn leave(&self, player: &str) {
        let peer = {
            let mut s = self.server.lock();
            let peer = s.peer_of(player);
            s.leave(player);
            peer
        };
        if let Some(peer) = peer {
            lock(&self.bound).remove(&peer);
        }
    }

    /// The events since the last call.
    pub fn take_events(&self) -> Vec<VoiceEvent> {
        self.server.lock().take_events()
    }

    /// The UDP port the voice socket should be open on: the mod's non-zero `udp_port`, else `config_port`
    /// (`HostConfig.voice.udp_port`, `0` = any free port), else the game port + 1.
    pub fn wanted_port(&self, config_port: Option<u16>, game_port: u16) -> u16 {
        match self.requested.get() {
            Some(p) if p != 0 => p,
            _ => config_port.unwrap_or(game_port.wrapping_add(1)),
        }
    }

    /// Once per poll: opens, moves or closes (`want` = `None`) the voice socket (`open` binds a port and says which
    /// one it got, `close` closes it) and gives the relay the port it really has. Returns a warning when a port
    /// could not be opened (once; voice chat is then off).
    pub fn sync_udp(
        &self,
        want: Option<u16>,
        open: &mut dyn FnMut(u16) -> Result<u16, String>,
        close: &mut dyn FnMut(),
    ) -> Option<String> {
        if want == self.udp_for.replace(want) {
            return None;
        }
        close();
        let mut port = None;
        let mut warning = None;
        if let Some(p) = want {
            match open(p) {
                Ok(got) => port = Some(got),
                Err(e) => {
                    warning = Some(format!("voice: UDP port {p} could not be opened ({e}); voice chat is off"));
                }
            }
        }
        self.server.lock().set_transport(TransportConfig { mode: Transport::Udp, udp_port: port });
        warning
    }

    /// The port the relay announces (`SERVER_HELLO.voiceUdpPort`, `VOICE_KEY.udpPort`).
    pub fn udp_port(&self) -> Option<u16> {
        self.server.lock().udp_port()
    }
}

impl VoiceHost for VoiceGlue {
    fn state(&self, player: &str) -> Result<State, ApiError> {
        self.server.state(player)
    }
    fn icons(&self) -> Result<Icons, ApiError> {
        self.server.icons()
    }
    fn set_routes(&self, speaker: &str, routes: &[Route]) -> Result<(), ApiError> {
        self.server.set_routes(speaker, routes)
    }
    fn mute(&self, player: &str, until_ms: Option<u64>) -> Result<(), ApiError> {
        self.server.mute(player, until_ms)
    }
    fn unmute(&self, player: &str) -> Result<bool, ApiError> {
        self.server.unmute(player)
    }
    fn set_speaker_gain(&self, player: &str, percent: u16) -> Result<(), ApiError> {
        self.server.set_speaker_gain(player, percent)
    }
    fn set_caps(&self, caps: Caps) -> Result<(), ApiError> {
        self.server.set_caps(caps)
    }
    fn set_transport(&self, config: TransportConfig) -> Result<(), ApiError> {
        // Only the port counts (frames go over UDP whatever the mode); the next poll opens the socket there.
        self.requested.set(config.udp_port);
        Ok(())
    }
    fn set_allowed(&self, player: &str, modes: u8, activations: u8) -> Result<(), ApiError> {
        self.server.set_allowed(player, modes, activations)
    }
    fn set_icons(&self, icons: Icons) -> Result<(), ApiError> {
        self.server.set_icons(icons)
    }
    fn set_broadcast_overrides_deafen(&self, on: bool) -> Result<(), ApiError> {
        self.server.set_broadcast_overrides_deafen(on)
    }
}

/// The handler of the voice socket (runs on its thread).
pub struct UdpGlue {
    server: SharedVoiceServer,
    bound: Bound,
    queue: Arc<Mutex<Vec<Queued>>>,
}

/// What a datagram produced: datagrams by peer and packets for the server thread.
#[derive(Default)]
struct UdpThreadSink {
    udp: Vec<(PeerId, Vec<u8>)>,
    queued: Vec<Queued>,
}

impl ServerSink for UdpThreadSink {
    fn reliable(&mut self, player: &str, packet: Vec<u8>) {
        self.queued.push((player.to_string(), packet));
    }
    fn udp(&mut self, peer: PeerId, datagram: Vec<u8>) {
        self.udp.push((peer, datagram));
    }
}

impl UdpVoiceHandler for UdpGlue {
    fn datagram(&self, from: SocketAddr, data: &[u8], out: &dyn UdpOut) {
        let mut sink = UdpThreadSink::default();
        let is_bound = |peer: PeerId| lock(&self.bound).get(&peer) == Some(&from);
        let auth = self.server.lock().receive_udp(unix_ms(), data, &is_bound, &mut sink);
        let addrs = {
            let mut b = lock(&self.bound);
            if let Some(peer) = auth {
                b.insert(peer, from);
            }
            b.clone()
        };
        for (peer, d) in sink.udp {
            if let Some(to) = addrs.get(&peer) {
                out.send_to(*to, &d);
            }
        }
        lock(&self.queue).extend(sink.queued);
    }
}

#[cfg(test)]
mod tests;
