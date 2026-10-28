//! ENet transport only: connect, pump, send, disconnect, per-packet timestamps. No protocol knowledge.
//!

use std::collections::VecDeque;
use std::io;
use std::net::{SocketAddr, UdpSocket};
use std::time::{Duration, Instant};

use rusty_enet as enet;

use crate::trace::{Dir, TraceSink};

/// Connection parameters.
#[derive(Clone, Debug)]
pub struct ClientConfig {
    pub addr: SocketAddr,
    /// ENet channel count requested in CONNECT (the real client requests 8).
    pub channels: usize,
    pub connect_data: u32,
    /// Channel used by [`Client::send`]; the server ignores the receive channel.
    pub send_channel: u8,
    pub connect_timeout_ms: u64,
    /// Local bind address, default `0.0.0.0:0`.
    pub local_bind: Option<SocketAddr>,
}

impl Default for ClientConfig {
    fn default() -> Self {
        ClientConfig {
            addr: "127.0.0.1:16200".parse().unwrap(),
            channels: 8,
            connect_data: 0,
            send_channel: 0,
            connect_timeout_ms: 5000,
            local_bind: None,
        }
    }
}

/// A received application packet.
#[derive(Clone, Debug)]
pub struct Rx {
    /// ms since the `connect()` call (real monotonic clock)
    pub t_ms: u64,
    pub channel: u8,
    pub reliable: bool,
    pub data: Vec<u8>,
}

/// Transport event.
#[derive(Clone, Debug)]
pub enum NetEvent {
    Connected,
    Disconnected { data: u32 },
    Packet(Rx),
}

/// ENet client with one peer.
pub struct Client {
    host: enet::Host<UdpSocket>,
    peer: enet::PeerID,
    cfg: ClientConfig,
    t0: Instant,
    queue: VecDeque<NetEvent>,
    trace: Option<TraceSink>,
    label: String,
    connected: bool,
}

fn enet_err<E: std::fmt::Debug>(what: &str, e: E) -> io::Error {
    io::Error::other(format!("{what}: {e:?}"))
}

/// Queue the CONNECT to `cfg.addr` on a free peer slot.
fn start_connect(host: &mut enet::Host<UdpSocket>, cfg: &ClientConfig) -> io::Result<enet::PeerID> {
    Ok(host.connect(cfg.addr, cfg.channels, cfg.connect_data).map_err(|e| enet_err("connect", e))?.id())
}

impl Client {
    /// Bind, send CONNECT and wait for VERIFY_CONNECT (`NetEvent::Connected` is queued).
    pub fn connect(cfg: ClientConfig) -> io::Result<Client> {
        let bind = cfg.local_bind.unwrap_or_else(|| "0.0.0.0:0".parse().unwrap());
        let socket = UdpSocket::bind(bind)?;
        let settings = enet::HostSettings { peer_limit: 1, channel_limit: cfg.channels, ..Default::default() };
        let mut host = enet::Host::new(socket, settings).map_err(|e| enet_err("Host::new", e))?;
        let t0 = Instant::now();
        let peer = start_connect(&mut host, &cfg)?;
        let mut c =
            Client { host, peer, cfg, t0, queue: VecDeque::new(), trace: None, label: String::new(), connected: false };
        let deadline = Instant::now() + Duration::from_millis(c.cfg.connect_timeout_ms);
        while !c.connected {
            c.pump()?;
            if c.connected {
                break;
            }
            // before the connection is up the only event that can be queued is a refusal
            if !c.queue.is_empty() || Instant::now() >= deadline {
                return Err(io::Error::new(io::ErrorKind::TimedOut, format!("ENet connect to {} failed", c.cfg.addr)));
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        Ok(c)
    }

    /// Attach (or remove) the trace sink.
    pub fn set_trace(&mut self, sink: Option<TraceSink>) {
        self.trace = sink;
    }

    /// Alias written into the `client` field of trace records.
    pub fn set_label(&mut self, label: &str) {
        self.label = label.to_string();
    }

    /// Current trace label.
    pub fn label(&self) -> &str {
        &self.label
    }

    /// ms since `connect()` was called.
    pub fn now_ms(&self) -> u64 {
        self.t0.elapsed().as_millis() as u64
    }

    /// Service the host until no event is pending (non-blocking).
    pub fn pump(&mut self) -> io::Result<()> {
        loop {
            let Some(ev) = self.host.service()? else { break };
            let ev = ev.no_ref();
            let t = self.now_ms();
            match ev {
                enet::EventNoRef::Connect { .. } => {
                    self.connected = true;
                    self.queue.push_back(NetEvent::Connected);
                }
                enet::EventNoRef::Disconnect { data, .. } => {
                    self.connected = false;
                    self.queue.push_back(NetEvent::Disconnected { data });
                }
                enet::EventNoRef::Receive { channel_id, packet, .. } => {
                    let reliable = matches!(packet.kind(), enet::PacketKind::Reliable);
                    let data = packet.data().to_vec();
                    if let Some(tr) = &self.trace {
                        tr.record(Dir::Rx, &self.label, t, channel_id, reliable, &data);
                    }
                    self.queue.push_back(NetEvent::Packet(Rx { t_ms: t, channel: channel_id, reliable, data }));
                }
            }
        }
        Ok(())
    }

    /// Pop one queued event (after `pump`).
    pub fn poll(&mut self) -> Option<NetEvent> {
        self.queue.pop_front()
    }

    /// Reliable send on `cfg.send_channel`.
    pub fn send(&mut self, data: &[u8]) -> io::Result<()> {
        let ch = self.cfg.send_channel;
        self.send_on(ch, data, true)
    }

    /// Send on an explicit channel, reliable or unreliable; flushes the host.
    pub fn send_on(&mut self, channel: u8, data: &[u8], reliable: bool) -> io::Result<()> {
        let pkt = if reliable { enet::Packet::reliable(data) } else { enet::Packet::unreliable(data) };
        self.host.peer_mut(self.peer).send(channel, &pkt).map_err(|e| enet_err("send", e))?;
        if let Some(tr) = &self.trace {
            tr.record(Dir::Tx, &self.label, self.now_ms(), channel, reliable, data);
        }
        self.host.flush();
        Ok(())
    }

    /// True while the ENet link is up.
    pub fn is_connected(&self) -> bool {
        self.connected
    }

    /// ENet disconnect + flush + wait up to 3 s for the event (mirrors the server's own 3000 ms wait).
    pub fn disconnect(&mut self, data: u32) {
        if !self.connected {
            return;
        }
        self.host.peer_mut(self.peer).disconnect(data);
        self.host.flush();
        let deadline = Instant::now() + Duration::from_millis(3000);
        while Instant::now() < deadline {
            let _ = self.pump();
            if !self.connected {
                break;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        if self.connected {
            self.host.peer_mut(self.peer).reset();
            self.connected = false;
        }
    }

    /// `enet_peer_reset`-like: no DISCONNECT is sent (timeout tests).
    pub fn drop_silently(mut self) {
        self.host.peer_mut(self.peer).reset();
        self.host.flush();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_host_without_free_peer_slots_cannot_connect() {
        let cfg = ClientConfig::default();
        let settings = enet::HostSettings { peer_limit: 1, channel_limit: cfg.channels, ..Default::default() };
        let mut host = enet::Host::new(UdpSocket::bind("127.0.0.1:0").unwrap(), settings).unwrap();
        assert!(start_connect(&mut host, &cfg).is_ok());
        let e = start_connect(&mut host, &cfg).unwrap_err();
        assert!(e.to_string().starts_with("connect: "), "{e}");
    }
}
