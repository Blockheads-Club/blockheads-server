//! The server side of the 0xBC mod protocol (mod-sdk `docs/protocol-0xbc.md`): detection, mod list, package
//! downloads, the `bhcModLoader` join check, readable rejections and mod channels over the network. Every packet is
//! encoded and decoded by `bhmod-proto`.
//!
//! Peers are named as the packet dispatcher names them: the ENet id until the 0x1f handshake, the player id after
//! it ([`ModNet::rekey`] moves the state at approval time). Only peers that sent `HELLO` ever receive a 0xBC byte.

use std::collections::{BTreeMap, BTreeSet};

use bhmod_core::types::ApiError;
use bhmod_proto::{
    file_error, hex, Channel, ChannelInfo, ChannelRegister, FileChunk, FileError, Hash, Message, ModList, RejectText,
    ServerHello, CHUNK_SIZE, MAX_IN_FLIGHT, PROTO, REJECT_CLIENT_TOO_OLD,
};

use super::catalog::Catalog;
use super::LoaderKey;

/// Most peers downloading at the same time; more get `FILE_ERROR` BUSY.
pub const MAX_TRANSFERS: usize = 8;

/// Where mod loader help lives (shown with every rejection text).
pub const HELP_URL: &str = "https://blockheads.club/docs/mods/players";

/// Rejection text for a `HELLO` with another protocol version.
pub fn proto_text() -> String {
    format!("This server needs mod loader protocol {PROTO}. Update your mod loader with Patcher.")
}

/// Rejection text for a join without the accepted mods.
pub const STALE_TEXT: &str =
    "This server's mods changed or were not accepted. Reconnect to review and download the server's mods.";

/// Rejection text for a join refused by a server mod (`player-pre-login`).
pub fn pre_login_text(reason: u8) -> String {
    format!("A server mod refused the join (reason {reason}).")
}

/// What the protocol needs from the server.
pub trait Link {
    /// Sends one reliable packet to a peer.
    fn send(&self, peer: &str, packet: Vec<u8>);
    /// Rejects a peer that is waiting for approval (`0x26 reason`, then it is no longer waiting).
    fn reject(&self, peer: &str, reason: u8);
    /// True when the peer is approved (in the game).
    fn approved(&self, peer: &str) -> bool;
    /// True while the peer is connected (approved or waiting for approval).
    fn alive(&self, peer: &str) -> bool;
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Transfer {
    sha256: Hash,
    total: u64,
    acked: u64,
    next: u64,
}

#[derive(Debug, Default)]
struct Peer {
    announced: BTreeSet<u16>,
    transfer: Option<Transfer>,
    ready: Option<Hash>,
}

/// Protocol state of the server.
#[derive(Debug, Default)]
pub struct ModNet {
    /// What the server offers.
    pub catalog: Catalog,
    peers: BTreeMap<String, Peer>,
    /// Channel name -> wire id (ids start at 1 and are never reused).
    wire: BTreeMap<String, u16>,
}

impl ModNet {
    /// Protocol state over a catalog.
    pub fn new(catalog: Catalog) -> ModNet {
        ModNet { catalog, ..ModNet::default() }
    }

    /// True when `peer` sent a valid `HELLO`.
    pub fn is_modded(&self, peer: &str) -> bool {
        self.peers.contains_key(peer)
    }

    /// True when some peer sent `HELLO`.
    pub fn has_peers(&self) -> bool {
        !self.peers.is_empty()
    }

    /// The `acceptedHash` of the peer's `READY`.
    pub fn ready_hash(&self, peer: &str) -> Option<Hash> {
        self.peers.get(peer).and_then(|p| p.ready)
    }

    /// The wire id of a channel, assigned on first use. `None` when all 65535 ids are taken.
    pub fn wire_id(&mut self, name: &str) -> Option<u16> {
        if let Some(id) = self.wire.get(name) {
            return Some(*id);
        }
        let id = u16::try_from(self.wire.len() + 1).ok()?;
        self.wire.insert(name.to_string(), id);
        Some(id)
    }

    fn name_of(&self, id: u16) -> Option<&str> {
        self.wire.iter().find(|(_, i)| **i == id).map(|(n, _)| n.as_str())
    }

    /// Moves a peer's state to its player id (the handshake approves under that name).
    pub fn rekey(&mut self, from: &str, to: &str) {
        if from != to {
            if let Some(p) = self.peers.remove(from) {
                self.peers.insert(to.to_string(), p);
            }
        }
    }

    /// Forgets peers that are gone.
    pub fn prune(&mut self, link: &dyn Link) {
        self.peers.retain(|k, _| link.alive(k));
    }

    /// Modded peers that are in the game.
    pub fn approved_peers(&self, link: &dyn Link) -> Vec<String> {
        self.peers.keys().filter(|k| link.approved(k)).cloned().collect()
    }

    /// Sends `REJECT_TEXT` to a modded peer (nothing to a vanilla one).
    pub fn reject_text(&self, link: &dyn Link, peer: &str, reason: u8, text: String) {
        if self.is_modded(peer) {
            let m = Message::RejectText(RejectText { reason_code: reason, text, url: Some(HELP_URL.to_string()) });
            link.send(peer, m.encode());
        }
    }

    /// The 0x1f `bhcModLoader` check. `Some(reason)`: reject (a modded peer got `REJECT_TEXT` first).
    pub fn check_loader(&self, link: &dyn Link, peer: &str, key: Option<&LoaderKey>) -> Option<u8> {
        if !self.catalog.client_required {
            return None;
        }
        let ok = key.is_some_and(|k| {
            k.proto == Some(PROTO as i64) && k.accepted_hash.as_deref() == Some(hex(&self.catalog.mods_hash).as_str())
        });
        if ok {
            return None;
        }
        let text = match key {
            Some(k) if k.proto != Some(PROTO as i64) => proto_text(),
            _ => STALE_TEXT.to_string(),
        };
        self.reject_text(link, peer, REJECT_CLIENT_TOO_OLD, text);
        Some(REJECT_CLIENT_TOO_OLD)
    }

    /// Announces channels (`CHANNEL_REGISTER`) a peer has not seen yet.
    fn announce_to(&mut self, link: &dyn Link, peer: &str, names: &[String]) {
        for n in names {
            let Some(id) = self.wire_id(n) else { continue };
            let Some(p) = self.peers.get_mut(peer) else { return };
            if p.announced.insert(id) {
                link.send(peer, Message::ChannelRegister(ChannelRegister { name: n.clone(), id }).encode());
            }
        }
    }

    /// Announces every channel in `names` to every modded peer in the game.
    pub fn announce(&mut self, link: &dyn Link, names: &[String]) {
        for peer in self.approved_peers(link) {
            self.announce_to(link, &peer, names);
        }
    }

    /// Sends a channel message to modded peers in the game (`peers`; others are skipped). Returns how many got it.
    pub fn send_channel(
        &mut self,
        link: &dyn Link,
        peers: &[String],
        name: &str,
        data: &[u8],
    ) -> Result<usize, ApiError> {
        let id = self.wire_id(name).ok_or_else(|| ApiError::LimitExceeded("network channels".into()))?;
        let packet = Message::Channel(Channel { channel_id: id, payload: data.to_vec() }).encode();
        let mut n = 0;
        for peer in peers {
            if !self.is_modded(peer) || !link.approved(peer) {
                continue;
            }
            self.announce_to(link, peer, &[name.to_string()]);
            link.send(peer, packet.clone());
            n += 1;
        }
        Ok(n)
    }

    fn file_error(link: &dyn Link, peer: &str, sha256: Hash, code: u64) {
        link.send(peer, Message::FileError(FileError { sha256, code }).encode());
    }

    fn file_request(&mut self, link: &dyn Link, peer: &str, sha256: Hash, offset: u64) {
        let Some(entry) = self.catalog.served(&sha256).cloned() else {
            return Self::file_error(link, peer, sha256, file_error::NOT_FOUND);
        };
        let total = entry.info.size;
        let chunk = CHUNK_SIZE as u64;
        if offset > total || (offset % chunk != 0 && offset != total) {
            return Self::file_error(link, peer, sha256, file_error::BAD_OFFSET);
        }
        let busy = self.peers.iter().filter(|(k, p)| k.as_str() != peer && p.transfer.is_some()).count();
        let p = self.peers.get_mut(peer).expect("caller checked HELLO");
        if offset == total {
            // Final acknowledgement: the transfer is complete.
            p.transfer = None;
            return;
        }
        match &mut p.transfer {
            Some(t) if t.sha256 == sha256 && offset >= t.acked => {
                t.acked = offset;
                t.next = t.next.max(offset);
            }
            _ if busy >= MAX_TRANSFERS => return Self::file_error(link, peer, sha256, file_error::BUSY),
            t => *t = Some(Transfer { sha256, total, acked: offset, next: offset }),
        }
        let t = p.transfer.as_mut().expect("set above");
        while t.next < t.total && t.next < t.acked + MAX_IN_FLIGHT * chunk {
            let len = chunk.min(t.total - t.next) as usize;
            match entry.read(t.next, len) {
                Ok(data) => {
                    let m = Message::FileChunk(FileChunk { sha256, offset: t.next, total, data });
                    link.send(peer, m.encode());
                    t.next += len as u64;
                }
                Err(_) => {
                    p.transfer = None;
                    return Self::file_error(link, peer, sha256, file_error::INTERNAL);
                }
            }
        }
    }

    /// Handles one 0xBC packet. `names`: the channels open now. Returns an inbound channel message `(name, data)`
    /// for the mods.
    pub fn packet(&mut self, link: &dyn Link, peer: &str, data: &[u8], names: &[String]) -> Option<(String, Vec<u8>)> {
        let msg = match Message::decode(data) {
            Ok(m) => m,
            // Unknown ops are ignored; malformed packets are dropped.
            Err(_) => return None,
        };
        if let Message::Hello(h) = &msg {
            if link.approved(peer) {
                return None;
            }
            if h.proto != PROTO {
                let m = Message::RejectText(RejectText {
                    reason_code: REJECT_CLIENT_TOO_OLD,
                    text: proto_text(),
                    url: Some(HELP_URL.to_string()),
                });
                link.send(peer, m.encode());
                self.peers.remove(peer);
                link.reject(peer, REJECT_CLIENT_TOO_OLD);
                return None;
            }
            self.peers.insert(peer.to_string(), Peer::default());
            let reply = ServerHello {
                proto: PROTO,
                mods_hash: self.catalog.mods_hash,
                client_required: self.catalog.client_required,
                count: self.catalog.entries.len() as u64,
            };
            link.send(peer, Message::ServerHello(reply).encode());
            return None;
        }
        if !self.is_modded(peer) {
            return None;
        }
        match msg {
            Message::ListRequest => {
                let mut channels = Vec::new();
                for n in names {
                    if let Some(id) = self.wire_id(n) {
                        channels.push(ChannelInfo { name: n.clone(), id });
                    }
                }
                let p = self.peers.get_mut(peer).expect("modded");
                p.announced.extend(channels.iter().map(|c| c.id));
                let list = ModList {
                    mods: self.catalog.entries.iter().map(|e| e.info.clone()).collect(),
                    channels,
                    registry: BTreeMap::new(),
                };
                link.send(peer, Message::ModList(list).encode());
            }
            Message::FileRequest(r) => self.file_request(link, peer, r.sha256, r.offset),
            Message::Ready(r) => {
                let p = self.peers.get_mut(peer).expect("modded");
                p.ready = Some(r.accepted_hash);
                p.transfer = None;
            }
            Message::Channel(c) if link.approved(peer) => {
                return self.name_of(c.channel_id).map(|n| (n.to_string(), c.payload));
            }
            // Client CHANNEL_REGISTER (the server owns the id space), server-to-client ops, channel traffic before
            // approval: ignored.
            _ => {}
        }
        None
    }
}

#[cfg(test)]
mod tests;
