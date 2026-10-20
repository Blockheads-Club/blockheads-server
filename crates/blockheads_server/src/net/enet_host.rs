//! Thin, game-agnostic wrapper over `rusty_enet::Host<UdpSocket>`; no ObjC semantics.
//!
//! (`enet_host_create(&{ANY, port}, 32, 0, 0, 0)`, `enet_host_service`, `enet_peer_*`).

use rusty_enet as enet;
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4, UdpSocket};
use std::time::{Duration, Instant};

/// Slot of an ENet peer (`ENetPeer*` in the original).
pub type PeerSlot = enet::PeerID;

/// One ENet event with all data copied out, so no borrow of the host outlives the call.
#[derive(Debug, Clone)]
pub enum OwnedEvent {
    /// `ENET_EVENT_TYPE_CONNECT`.
    Connect { peer: PeerSlot, addr: SocketAddrV4 },
    /// `ENET_EVENT_TYPE_DISCONNECT` (`addr` captured at event time).
    Disconnect { peer: PeerSlot, addr: SocketAddrV4 },
    /// `ENET_EVENT_TYPE_RECEIVE`; the packet is already copied (and destroyed).
    Receive { peer: PeerSlot, addr: SocketAddrV4, channel: u8, data: Vec<u8> },
}

/// The ENet host.
pub struct EnetHost {
    host: enet::Host<UdpSocket>,
}

fn to_v4(a: Option<SocketAddr>) -> SocketAddrV4 {
    match a {
        Some(SocketAddr::V4(v4)) => v4,
        _ => SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, 0),
    }
}

impl EnetHost {
    /// `enet_host_create(&{ANY, port}, 32, 0, 0, 0)`: 32 peers, channel limit 0 (= 255), no bandwidth limit,
    /// no compressor, no checksum. `SO_RCVBUF`/`SO_SNDBUF` = 0x40000, no `SO_REUSEADDR`.
    /// Err on bind/socket failure (NULL host in C).
    pub fn create(port: u16) -> Result<EnetHost, std::io::Error> {
        let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, port))?;
        // Result ignored like the C code.
        let sock = socket2::SockRef::from(&socket);
        let _ = sock.set_recv_buffer_size(0x40000);
        let _ = sock.set_send_buffer_size(0x40000);
        // NOTE: the default `time` (real wall/monotonic time) is kept on purpose. This is the one allowed exception
        // to "no Instant/SystemTime outside foundation::clock": ENet timing is real network timing and must not
        // follow the Virtual clock.
        let settings = enet::HostSettings {
            peer_limit: 32,
            channel_limit: 255, // C's 0 becomes ENET_PROTOCOL_MAXIMUM_CHANNEL_COUNT; rusty_enet rejects 0
            incoming_bandwidth_limit: None,
            outgoing_bandwidth_limit: None,
            compressor: None,
            checksum: None,
            seed: None,
            ..Default::default()
        };
        let host = enet::Host::new(socket, settings)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, format!("{e:?}")))?;
        Ok(EnetHost { host })
    }

    /// Local UDP port the host is bound to (useful with port 0). Not in the original.
    pub fn local_port(&self) -> u16 {
        self.host.socket().local_addr().map(|a| a.port()).unwrap_or(0)
    }

    /// `enet_host_service(host, &ev, 0) > 0 ? Some(ev): None`. Socket errors map to `None`
    /// (C returns -1 and every loop tests `> 0`).
    pub fn service0(&mut self) -> Option<OwnedEvent> {
        match self.host.service() {
            Ok(Some(ev)) => Some(match ev {
                enet::Event::Connect { peer, .. } => {
                    OwnedEvent::Connect { peer: peer.id(), addr: to_v4(peer.address()) }
                }
                enet::Event::Disconnect { peer, .. } => {
                    OwnedEvent::Disconnect { peer: peer.id(), addr: to_v4(peer.address()) }
                }
                enet::Event::Receive { peer, channel_id, packet } => OwnedEvent::Receive {
                    peer: peer.id(),
                    addr: to_v4(peer.address()),
                    channel: channel_id,
                    data: packet.data().to_vec(),
                },
            }),
            Ok(None) | Err(_) => None,
        }
    }

    /// `enet_host_service(host, &ev, timeout_ms)`: deadline loop of `service()` plus a sleep of at most 1 ms.
    pub fn service_wait(&mut self, timeout_ms: u32) -> Option<OwnedEvent> {
        let deadline = Instant::now() + Duration::from_millis(timeout_ms as u64);
        loop {
            if let Some(ev) = self.service0() {
                return Some(ev);
            }
            if Instant::now() >= deadline {
                return None;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
    }

    /// `enet_packet_create(RELIABLE)` + `enet_peer_send`; an Err (bad channel, wrong state) is ignored like the C
    /// return value. NOT flushed: the packet leaves at the next `service`.
    pub fn send_reliable(&mut self, peer: PeerSlot, channel: u8, data: &[u8]) {
        if let Some(p) = self.host.get_peer_mut(peer) {
            let _ = p.send(channel, &enet::Packet::reliable(data));
        }
    }

    /// `enet_peer_disconnect`.
    pub fn peer_disconnect(&mut self, peer: PeerSlot, data: u32) {
        if let Some(p) = self.host.get_peer_mut(peer) {
            p.disconnect(data);
        }
    }

    /// `enet_peer_disconnect_later`.
    pub fn peer_disconnect_later(&mut self, peer: PeerSlot, data: u32) {
        if let Some(p) = self.host.get_peer_mut(peer) {
            p.disconnect_later(data);
        }
    }

    /// `enet_peer_reset` (immediate drop, no DISCONNECT event).
    pub fn peer_reset(&mut self, peer: PeerSlot) {
        if let Some(p) = self.host.get_peer_mut(peer) {
            p.reset();
        }
    }

    /// `enet_address_get_host_ip`: dotted quad.
    pub fn peer_ip_string(&self, peer: PeerSlot) -> Option<String> {
        match self.host.get_peer(peer)?.address()? {
            SocketAddr::V4(a) => Some(a.ip().to_string()),
            SocketAddr::V6(_) => None,
        }
    }

    /// `enet_host_destroy`.
    pub fn destroy(self) {
        drop(self);
    }
}

/// `[NSString stringWithFormat:@"%d:%d", (int)address.host, (int)address.port]`. The C `host` is the network-order
/// u32 read as a little-endian int, so 127.0.0.1 gives 16777343.
pub fn enet_id_string(addr: &SocketAddrV4) -> String {
    format!("{}:{}", i32::from_le_bytes(addr.ip().octets()), addr.port() as i32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn id_strings() {
        assert_eq!(enet_id_string(&"127.0.0.1:54321".parse().unwrap()), "16777343:54321");
        assert_eq!(enet_id_string(&"192.168.1.2:1".parse().unwrap()), "33663168:1");
        assert!(enet_id_string(&"1.2.3.255:9".parse().unwrap()).starts_with('-'));
    }

    #[test]
    fn second_bind_fails() {
        let a = EnetHost::create(0).unwrap();
        let port = a.local_port();
        assert_ne!(port, 0);
        assert!(EnetHost::create(port).is_err());
    }
}
