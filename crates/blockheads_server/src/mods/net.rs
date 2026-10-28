//! The server side of the 0xBC mod protocol (mod-sdk `docs/protocol-0xbc.md`): detection, mod list, package
//! downloads, the `bhcModLoader` join check, readable rejections and mod channels over the network. Every packet is
//! encoded and decoded by `bhmod-proto`.
//!
//! Peers are named as the packet dispatcher names them: the ENet id until the 0x1f handshake, the player id after
//! it ([`ModNet::rekey`] moves the state at approval time). Only peers that sent `HELLO` ever receive a 0xBC byte.

use std::collections::{BTreeMap, BTreeSet};
use std::time::{Duration, Instant};

use bhmod_core::remote::{RemoteCall, RemoteError};
use bhmod_core::types::ApiError;
use bhmod_proto::{
    file_error, hex, Channel, ChannelInfo, ChannelRegister, Cosmetics, FileChunk, FileError, Hash, Message, ModList,
    PlayerCount, RejectText, ServerHello, ServiceCall, ServiceFailure, ServiceResult, SettingsSet, CHUNK_SIZE,
    MAX_IN_FLIGHT, PROTO, REJECT_CLIENT_TOO_OLD,
};

use super::catalog::Catalog;
use super::content::Content;
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
    /// Players in the game and the server's capacity, if known (the optional player count of `SERVER_HELLO`).
    fn players(&self) -> Option<(u32, u32)>;
}

/// What a 0xBC packet from a player asks the mods to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Inbound {
    /// A channel message `(name, data)`.
    Channel(String, Vec<u8>),
    /// The player's client calls a service of a server mod.
    Call(ServiceCall),
    /// The player's client answers a call of a server mod.
    Result(ServiceResult),
    /// `VOICE_KEY` or `VOICE_CTRL` for the voice relay (only while the server lists `voice` in `SERVER_HELLO`).
    Voice(Message),
    /// `SETTINGS_SET` (only while the server lists `settings` in `SERVER_HELLO`).
    SettingsSet(SettingsSet),
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
    /// The mods of `READY.loaded`: the required client mods plus the optional ones the player switched on.
    loaded: Option<BTreeSet<String>>,
    /// `HELLO.features`: the optional protocol features the client speaks.
    features: Vec<String>,
}

/// A vanilla peer held in the limbo until `until`.
#[derive(Debug)]
struct Limbo {
    peer: String,
    player: String,
    until: Instant,
}

/// Protocol state of the server.
#[derive(Debug, Default)]
pub struct ModNet {
    /// What the server offers.
    pub catalog: Catalog,
    /// The world's content ids and cosmetics (empty without content mods).
    pub content: Content,
    /// Player id -> slot -> cosmetic id. Not saved: mods re-equip on join.
    worn: BTreeMap<String, BTreeMap<String, u64>>,
    peers: BTreeMap<String, Peer>,
    /// Channel name -> wire id (ids start at 1 and are never reused).
    wire: BTreeMap<String, u16>,
    /// Vanilla peers shown the readable rejection (not approved, not in the game).
    limbo: Vec<Limbo>,
    /// The optional features the server serves (`SERVER_HELLO.features`): `voice` while a loaded mod holds the
    /// voice permission and the voice socket is bound, `settings` while a mod has server settings. Empty: nothing
    /// new on the wire.
    pub features: Vec<String>,
    /// The voice UDP port announced in `SERVER_HELLO` (only with `voice`).
    pub voice_udp_port: Option<u16>,
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

    /// True when the server serves the optional feature `f` (see [`ModNet::features`]).
    pub fn serves(&self, f: &str) -> bool {
        self.features.iter().any(|x| x == f)
    }

    /// The `HELLO.features` of a modded peer (empty for a vanilla one).
    pub fn features_of(&self, peer: &str) -> Vec<String> {
        self.peers.get(peer).map(|p| p.features.clone()).unwrap_or_default()
    }

    /// True when the client of `peer` listed mod `id` in its `READY` (API 0.3.4 `players.has-mod`): false for a
    /// vanilla peer and for one that sent no `READY`.
    pub fn has_mod(&self, peer: &str, id: &str) -> bool {
        self.peers.get(peer).and_then(|p| p.loaded.as_ref()).is_some_and(|l| l.contains(id))
    }

    /// The `acceptedHash` of the peer's `READY`.
    pub fn ready_hash(&self, peer: &str) -> Option<Hash> {
        self.peers.get(peer).and_then(|p| p.ready)
    }

    /// True when the client of `peer` runs mod `id`. A peer that never sent `READY` (all mods server-only) runs no
    /// client mods; the server still sends it channel traffic, as before, so only a `READY` can say no.
    pub fn runs_mod(&self, peer: &str, id: &str) -> bool {
        self.peers.get(peer).and_then(|p| p.loaded.as_ref()).is_none_or(|l| l.contains(id))
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

    /// What a player wears as `(slot, cosmetic id)`.
    pub fn worn(&self, player: &str) -> Vec<(String, u64)> {
        self.worn.get(player).map(|m| m.iter().map(|(s, i)| (s.clone(), *i)).collect()).unwrap_or_default()
    }

    /// Sends the whole `COSMETICS` table to one modded peer (right after it joined). Nothing when no cosmetic is
    /// declared or the peer has no mod loader.
    pub fn cosmetics_full(&self, link: &dyn Link, peer: &str) {
        if self.content.has_cosmetics() && self.is_modded(peer) {
            let m = Message::Cosmetics(Cosmetics { full: true, players: self.worn.clone() });
            link.send(peer, m.encode());
        }
    }

    /// Puts cosmetic `id` (or nothing) on a player in `slot` and tells every modded peer in the game. The id must
    /// be a cosmetic of that slot.
    pub fn set_cosmetic(&mut self, link: &dyn Link, player: &str, slot: &str, id: Option<u64>) -> Result<(), ApiError> {
        if let Some(id) = id {
            match self.content.cosmetics.get(&id) {
                Some(s) if s == slot => {}
                Some(s) => return Err(ApiError::InvalidArgument(format!("cosmetic {id} is for the {s} slot"))),
                None => return Err(ApiError::InvalidArgument(format!("unknown cosmetic {id}"))),
            }
        } else if !self.content.has_cosmetics() {
            return Err(ApiError::Unsupported);
        }
        let set = self.worn.entry(player.to_string()).or_default();
        match id {
            Some(id) => set.insert(slot.to_string(), id),
            None => set.remove(slot),
        };
        let now = set.clone();
        if now.is_empty() {
            self.worn.remove(player);
        }
        self.cosmetics_delta(link, player, now);
        Ok(())
    }

    /// A player left: they wear nothing any more.
    pub fn player_left(&mut self, link: &dyn Link, player: &str) {
        if self.worn.remove(player).is_some() {
            self.cosmetics_delta(link, player, BTreeMap::new());
        }
    }

    fn cosmetics_delta(&self, link: &dyn Link, player: &str, set: BTreeMap<String, u64>) {
        let m = Message::Cosmetics(Cosmetics { full: false, players: BTreeMap::from([(player.to_string(), set)]) });
        let packet = m.encode();
        for peer in self.approved_peers(link) {
            link.send(&peer, packet.clone());
        }
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

    /// True when a join that failed [`ModNet::check_loader`] gets the limbo instead of the bare rejection: the
    /// server requires client mods and the peer is vanilla (no `HELLO`, no `bhcModLoader`).
    pub fn wants_limbo(&self, peer: &str, key: Option<&LoaderKey>) -> bool {
        self.catalog.client_required && key.is_none() && !self.is_modded(peer)
    }

    /// Holds a vanilla peer (`player` is its player id) in the limbo for `grace` from `now`.
    pub fn enter_limbo(&mut self, peer: &str, player: &str, now: Instant, grace: Duration) {
        self.limbo.push(Limbo { peer: peer.to_string(), player: player.to_string(), until: now + grace });
    }

    /// Takes a peer (by either name) out of the limbo. `Some(player id)` when it was in it.
    pub fn leave_limbo(&mut self, peer: &str) -> Option<String> {
        let i = self.limbo.iter().position(|l| l.peer == peer || l.player == peer)?;
        Some(self.limbo.remove(i).player)
    }

    /// Takes the peers whose grace ended at `now` out of the limbo. Returns their player ids.
    pub fn expired_limbo(&mut self, now: Instant) -> Vec<String> {
        let (gone, kept): (Vec<Limbo>, Vec<Limbo>) =
            std::mem::take(&mut self.limbo).into_iter().partition(|l| l.until <= now);
        self.limbo = kept;
        gone.into_iter().map(|l| l.player).collect()
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
        // Channel names are `<mod-id>:<name>`; a player who declined that optional mod gets nothing.
        let mod_id = name.split(':').next().unwrap_or(name);
        for peer in peers {
            if !self.is_modded(peer) || !link.approved(peer) || !self.runs_mod(peer, mod_id) {
                continue;
            }
            self.announce_to(link, peer, &[name.to_string()]);
            link.send(peer, packet.clone());
            n += 1;
        }
        Ok(n)
    }

    /// Sends a service call of a server mod to a modded peer in the game (`SERVICE_CALL`). `unavailable` for a peer
    /// without the mod loader, one that is not in the game, or one that is gone.
    pub fn send_service_call(&self, link: &dyn Link, peer: &str, call: &RemoteCall) -> Result<(), RemoteError> {
        if !self.is_modded(peer) || !link.approved(peer) {
            return Err(RemoteError::Unavailable);
        }
        link.send(peer, Message::ServiceCall(ServiceCall::from(call)).encode());
        Ok(())
    }

    /// Answers a service call of a peer (`SERVICE_RESULT`).
    pub fn send_service_result(&self, link: &dyn Link, peer: &str, id: u32, outcome: Result<Vec<u8>, RemoteError>) {
        let outcome = outcome.map_err(|e| ServiceFailure::from(&e));
        link.send(peer, Message::ServiceResult(ServiceResult { id, outcome }).encode());
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

    /// Handles one 0xBC packet. `names`: the channels open now. Returns what the mods have to handle (a channel
    /// message, a service call or a service answer), if anything.
    pub fn packet(&mut self, link: &dyn Link, peer: &str, data: &[u8], names: &[String]) -> Option<Inbound> {
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
            self.peers.insert(peer.to_string(), Peer { features: h.features.clone(), ..Peer::default() });
            let voice = self.serves(bhmod_proto::feature::VOICE);
            let reply = ServerHello {
                proto: PROTO,
                mods_hash: self.catalog.mods_hash,
                client_required: self.catalog.client_required,
                count: self.catalog.entries.len() as u64,
                players: link.players().map(|(online, capacity)| PlayerCount { online, capacity }),
                // Empty without a voice or settings mod: nothing new on the wire.
                features: self.features.clone(),
                voice_udp_port: self.voice_udp_port.filter(|_| voice),
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
                    registry: self.content.wire.clone(),
                };
                link.send(peer, Message::ModList(list).encode());
            }
            Message::FileRequest(r) => self.file_request(link, peer, r.sha256, r.offset),
            Message::Ready(r) => {
                let p = self.peers.get_mut(peer).expect("modded");
                p.ready = Some(r.accepted_hash);
                p.loaded = Some(r.loaded.into_iter().map(|(id, _)| id).collect());
                p.transfer = None;
            }
            Message::Channel(c) if link.approved(peer) => {
                return self.name_of(c.channel_id).map(|n| Inbound::Channel(n.to_string(), c.payload));
            }
            Message::ServiceCall(c) if link.approved(peer) => return Some(Inbound::Call(c)),
            Message::ServiceResult(r) if link.approved(peer) => return Some(Inbound::Result(r)),
            Message::VoiceKey(_) | Message::VoiceCtrl(_)
                if link.approved(peer) && self.serves(bhmod_proto::feature::VOICE) =>
            {
                return Some(Inbound::Voice(msg));
            }
            Message::SettingsSet(s) if link.approved(peer) && self.serves(bhmod_proto::feature::SETTINGS) => {
                return Some(Inbound::SettingsSet(s));
            }
            // Client CHANNEL_REGISTER (the server owns the id space), server-to-client ops, channel, service, voice
            // and settings traffic before approval (or of a feature the server does not serve): ignored.
            _ => {}
        }
        None
    }
}

#[cfg(test)]
mod tests;
