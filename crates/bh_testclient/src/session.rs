//! Protocol-level client: handshake, state tracking, typed request methods, wait helpers.
//!

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::io;
use std::time::{Duration, Instant};

use base64::Engine;

use crate::client::{Client, ClientConfig, NetEvent, Rx};
use crate::trace::TraceSink;

/// Single place to adapt names of the lower modules.
mod deps {
    pub use blockheads_server::foundation::gsdict::GsDict;
    pub use blockheads_server::foundation::md5::string_from_md5;
    pub use blockheads_server::plist::value::PlistValue as PV;
    pub use blockheads_server::protocol::block_wire::{self, block_hash, BlockMsg, ClientMacroBlockRequest};
    pub use blockheads_server::protocol::codec::{self, frame, gz, plist_from, plist_gz, t};
    pub use blockheads_server::protocol::messages::{self, decode_server_packet, ServerMsg};
    pub use blockheads_server::protocol::object_batches::{self, blob_unique_id};
    pub use blockheads_server::types::structs::WireStruct;
}
use deps::*;

pub use blockheads_server::protocol::messages::{
    FillNetRequest, GatherNetRequest, PaintNetRequest, PlaceInteractionObjectNetRequest, PlaceWorkbenchNetRequest,
    ProjectileNetRequest, RemoveBackWallNetRequest, RemoveNetRequest, WorldHeartbeat,
};
pub use blockheads_server::protocol::object_batches::ClientBatchKind as BatchKind;

/// Which plist writer the 0x1f handshake uses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlistDialect {
    /// hand-written Apple-style XML (what the real client sends); default
    Apple,
    /// the server's own writer output (`codec::plist_xml`)
    GnuStep,
}

/// Player identity used in the 0x1f handshake.
#[derive(Clone, Debug)]
pub struct Identity {
    pub alias: String,
    /// default `"udid-<alias>"`
    pub udid: String,
    /// default 3
    pub minor_version: i32,
    pub password: Option<String>,
    pub cloud_key: Option<String>,
    pub icloud_id: Option<String>,
    pub game_center_id: Option<String>,
    pub photo: Option<Vec<u8>>,
    pub plist_dialect: PlistDialect,
    /// `bhcModLoader` `(proto, acceptedHash hex)` of a modded client (0xBC protocol), `None` for vanilla.
    pub mod_loader: Option<(i64, String)>,
}

impl Identity {
    /// Defaults of the spec: udid `udid-<alias>`, minorVersion 3, Apple dialect.
    pub fn new(alias: &str) -> Self {
        Identity {
            alias: alias.to_string(),
            udid: format!("udid-{}", alias),
            minor_version: 3,
            password: None,
            cloud_key: None,
            icloud_id: None,
            game_center_id: None,
            photo: None,
            plist_dialect: PlistDialect::Apple,
            mod_loader: None,
        }
    }
    pub fn udid(mut self, u: &str) -> Self {
        self.udid = u.to_string();
        self
    }
    pub fn password(mut self, p: &str) -> Self {
        self.password = Some(p.to_string());
        self
    }
    pub fn minor_version(mut self, v: i32) -> Self {
        self.minor_version = v;
        self
    }
    pub fn cloud_key(mut self, v: &str) -> Self {
        self.cloud_key = Some(v.to_string());
        self
    }
    pub fn icloud_id(mut self, v: &str) -> Self {
        self.icloud_id = Some(v.to_string());
        self
    }
    pub fn game_center_id(mut self, v: &str) -> Self {
        self.game_center_id = Some(v.to_string());
        self
    }
    pub fn photo(mut self, v: Vec<u8>) -> Self {
        self.photo = Some(v);
        self
    }
    pub fn mod_loader(mut self, proto: i64, accepted_hash: &str) -> Self {
        self.mod_loader = Some((proto, accepted_hash.to_string()));
        self
    }
    pub fn dialect(mut self, d: PlistDialect) -> Self {
        self.plist_dialect = d;
        self
    }

    /// Persistent id the server assigns: `md5(lowercase(alias))` hex.
    pub fn player_id(&self) -> String {
        string_from_md5(&self.alias.to_lowercase())
    }

    /// The 0x1f packet (type byte + plist). `udidNew` is always present; `None` fields are omitted.
    pub fn player_info_packet(&self) -> Vec<u8> {
        match self.plist_dialect {
            PlistDialect::Apple => {
                let mut s = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?><plist version=\"1.0\"><dict>");
                let mut kv = |k: &str, v: String| {
                    s.push_str(&format!("<key>{}</key>{}", k, v));
                };
                kv("alias", format!("<string>{}</string>", xml_escape(&self.alias)));
                kv("udidNew", format!("<string>{}</string>", xml_escape(&self.udid)));
                kv("minorVersion", format!("<integer>{}</integer>", self.minor_version));
                if let Some(p) = &self.password {
                    kv("clientPassword", format!("<string>{}</string>", xml_escape(p)));
                }
                if let Some(p) = &self.cloud_key {
                    kv("cloudKey", format!("<string>{}</string>", xml_escape(p)));
                }
                if let Some(p) = &self.icloud_id {
                    kv("iCloudID", format!("<string>{}</string>", xml_escape(p)));
                }
                if let Some(p) = &self.game_center_id {
                    kv("gameCenterID", format!("<string>{}</string>", xml_escape(p)));
                }
                if let Some(p) = &self.photo {
                    kv("photo", format!("<data>{}</data>", base64::engine::general_purpose::STANDARD.encode(p)));
                }
                if let Some((proto, hash)) = &self.mod_loader {
                    kv(
                        "bhcModLoader",
                        format!(
                            "<dict><key>acceptedHash</key><string>{}</string><key>proto</key><integer>{}</integer></dict>",
                            xml_escape(hash),
                            proto
                        ),
                    );
                }
                s.push_str("</dict></plist>");
                let mut v = vec![t::CLIENT_PLAYER_INFORMATION];
                v.extend_from_slice(s.as_bytes());
                v
            }
            PlistDialect::GnuStep => {
                let mut m: GsDict<String, PV> = GsDict::new();
                m.insert("alias".into(), PV::string(self.alias.clone()));
                m.insert("udidNew".into(), PV::string(self.udid.clone()));
                m.insert("minorVersion".into(), PV::int(self.minor_version as i64));
                if let Some(p) = &self.password {
                    m.insert("clientPassword".into(), PV::string(p.clone()));
                }
                if let Some(p) = &self.cloud_key {
                    m.insert("cloudKey".into(), PV::string(p.clone()));
                }
                if let Some(p) = &self.icloud_id {
                    m.insert("iCloudID".into(), PV::string(p.clone()));
                }
                if let Some(p) = &self.game_center_id {
                    m.insert("gameCenterID".into(), PV::string(p.clone()));
                }
                if let Some(p) = &self.photo {
                    m.insert("photo".into(), PV::data(p.clone()));
                }
                if let Some((proto, hash)) = &self.mod_loader {
                    let mut d: GsDict<String, PV> = GsDict::new();
                    d.insert("acceptedHash".into(), PV::string(hash.clone()));
                    d.insert("proto".into(), PV::int(*proto));
                    m.insert("bhcModLoader".into(), PV::dict(d));
                }
                messages::client_player_information(&PV::dict(m))
            }
        }
    }
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

/// Result of [`Session::join`].
#[derive(Debug, Clone, PartialEq)]
pub enum JoinOutcome {
    Approved,
    Rejected {
        reason: u8,
    },
    /// no 0x23 within the timeout
    NoContact,
    Disconnected,
}

/// Object announced to us that we created (clientID == our player id).
#[derive(Clone, Debug)]
pub struct CreatedObject {
    pub object_type: u8,
    pub unique_id: u64,
    pub blob: Vec<u8>,
}

/// Behaviour switches.
#[derive(Clone, Debug, Default)]
pub struct SessionConfig {
    /// reply `0x0d idx` to unsolicited 0x04 blocks (default off)
    pub auto_removal_notification: bool,
}

/// Tracked client-side state.
#[derive(Clone, Debug)]
pub struct SessionState {
    pub world_id: Option<String>,
    pub requires_password: bool,
    pub owner_name: Option<String>,
    /// the byte after 0x23 (0x26 = 38)
    pub protocol_version: Option<u8>,
    pub summary: Option<PV>,
    /// default 512 until the summary is known
    pub world_width_macro: i32,
    pub block_msgs: BTreeMap<u32, BlockMsg>,
    pub blocks_not_loaded: Vec<u64>,
    pub unique_ids: Vec<u64>,
    pub created: Vec<CreatedObject>,
    pub chat: Vec<PV>,
    pub players: Option<PV>,
    pub food_types: Option<[i32; 31]>,
    pub initial_plist: Option<PV>,
    pub last_heartbeat: Option<WorldHeartbeat>,
    pub in_flight: BTreeSet<u32>,
    pub last_rejection: Option<u8>,
    /// objects announced by 0x07/0x08 keyed by (object type, unique id)
    pub objects_seen: BTreeMap<(u8, u64), Vec<u8>>,
    /// highest update index seen per light block
    pub light_update_index: BTreeMap<u32, u32>,
    /// diagnostics (bad hash, bad block, stale light block...), not errors
    pub notes: Vec<String>,
    /// a 0x01 summary was received
    pub approved: bool,
    /// ENet reported a disconnect
    pub disconnected: bool,
}

impl Default for SessionState {
    fn default() -> Self {
        SessionState {
            world_id: None,
            requires_password: false,
            owner_name: None,
            protocol_version: None,
            summary: None,
            world_width_macro: 512,
            block_msgs: BTreeMap::new(),
            blocks_not_loaded: Vec::new(),
            unique_ids: Vec::new(),
            created: Vec::new(),
            chat: Vec::new(),
            players: None,
            food_types: None,
            initial_plist: None,
            last_heartbeat: None,
            in_flight: BTreeSet::new(),
            last_rejection: None,
            objects_seen: BTreeMap::new(),
            light_update_index: BTreeMap::new(),
            notes: Vec::new(),
            approved: false,
            disconnected: false,
        }
    }
}

/// The real client's block request throttling.
#[derive(Clone, Debug, Default)]
pub struct Throttle {
    /// `sentBlockRequests`: index -> ms of the last (first) send
    pub in_flight: BTreeMap<i32, u64>,
    /// queued requests `(index, create)` without duplicates
    pub queue: VecDeque<(i32, bool)>,
}

impl Throttle {
    /// send immediately only while fewer than 17 requests are in flight
    pub const SEND_BELOW: usize = 17;
    /// the queue drains while fewer than 16 are in flight
    pub const DRAIN_BELOW: usize = 16;
    /// an in-flight index is re-sent when the last send is more than this old
    pub const RESEND_MS: u64 = 5000;

    /// `requestBlockFromServer`: returns the packet to send now, if any.
    pub fn request(&mut self, idx: i32, create: bool, now_ms: u64) -> Option<Vec<u8>> {
        if let Some(&last) = self.in_flight.get(&idx) {
            // re-sent without touching the map
            if now_ms.saturating_sub(last) > Self::RESEND_MS {
                return Some(Self::packet(idx, create));
            }
            return None;
        }
        if self.in_flight.len() < Self::SEND_BELOW {
            self.in_flight.insert(idx, now_ms);
            return Some(Self::packet(idx, create));
        }
        if !self.queue.iter().any(|(i, _)| *i == idx) {
            self.queue.push_back((idx, create));
        }
        None
    }

    /// Record a raw (unthrottled) send.
    pub fn note_sent(&mut self, idx: i32, now_ms: u64) {
        self.in_flight.insert(idx, now_ms);
    }

    /// Reply (0x04 / 0x27) for `idx`: erase and `updateQueue`. Returns packets to send.
    pub fn answered(&mut self, idx: i32, now_ms: u64) -> Vec<Vec<u8>> {
        self.in_flight.remove(&idx);
        let mut out = Vec::new();
        while self.in_flight.len() < Self::DRAIN_BELOW {
            let Some((i, c)) = self.queue.pop_front() else { break };
            if self.in_flight.contains_key(&i) {
                continue;
            }
            self.in_flight.insert(i, now_ms);
            out.push(Self::packet(i, c));
        }
        out
    }

    fn packet(idx: i32, create: bool) -> Vec<u8> {
        block_wire::block_request(ClientMacroBlockRequest {
            macro_index: idx,
            create_if_not_created: create as u8,
            padding: [0; 3],
        })
    }
}

/// Default custom-rules image for a non-expert world, as accepted by the reference.
pub const DEFAULT_CUSTOM_RULES_IMAGE: [u8; 64] = [
    0x00, 0x00, 0x02, 0x02, 0x00, 0x00, 0x00, 0x00, 0x02, 0x02, 0x04, 0x02, 0x01, 0x01, 0x02, 0x02, 0x01, 0x02, 0x02,
    0x02, 0x01, 0x01, 0x00, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x00, 0x02, 0x00, 0x02, 0x00, 0x02, 0x00,
    0x02, 0x00, 0x02, 0x00, 0x02, 0x00, 0x02, 0x00, 0x00, 0x00, 0x02, 0x02, 0x02, 0x05, 0x01, 0x00, 0x7f, 0x7f, 0x7f,
    0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
];

/// The 0x44 custom-rules image the server expects for a world: `custom_rules_from_dict(summary.customRules,
/// summary.expertMode).to_bytes()`; without a summary the non-expert default. The server overlays the received
/// bytes onto `defaultCustomRules(expert)` and compares them with the world's current rules.
pub fn rules_image_for_summary(summary: Option<&PV>) -> [u8; 64] {
    use blockheads_server::types::structs::WireStruct;
    use blockheads_server::world::custom_rules::{custom_rules_from_dict, default_custom_rules};
    let Some(s) = summary else { return default_custom_rules(false).to_vec().try_into().unwrap() };
    let expert = s.object_for_key("expertMode").map(|v| v.bool_value()).unwrap_or(false);
    let rules = match s.object_for_key("customRules") {
        Some(PV::Dict(d)) => custom_rules_from_dict(Some(d), expert),
        _ => default_custom_rules(expert),
    };
    rules.to_vec().try_into().unwrap()
}

/// Creation parameters of a blockhead.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BlockheadSpec {
    pub name: String,
    pub pos_x: u32,
    pub pos_y: u32,
    /// `BlockheadSkinOptions` (20 bytes): isMale, faceIndex, hairTextureIndex, hairStyleIndex, 4 x RGBA
    pub skin: [u8; 20],
    pub hat: u16,
    pub pants: u16,
    pub shirt: u16,
    pub shoes: u16,
    pub hat_color: u16,
    pub pants_color: u16,
    pub shirt_color: u16,
    pub shoes_color: u16,
    pub view_radius: u16,
    pub happiness: u8,
}

impl BlockheadSpec {
    /// Defaults of 4.6: isMale 1, face/hair 0, colors `c8 aa 8c ff`, items 0, viewRadius 10, happiness 100.
    pub fn new(name: &str, pos_x: u32, pos_y: u32) -> Self {
        let mut skin = [0u8; 20];
        skin[0] = 1;
        for i in 0..4 {
            skin[4 + i * 4..8 + i * 4].copy_from_slice(&[0xc8, 0xaa, 0x8c, 0xff]);
        }
        BlockheadSpec {
            name: name.to_string(),
            pos_x,
            pos_y,
            skin,
            hat: 0,
            pants: 0,
            shirt: 0,
            shoes: 0,
            hat_color: 0,
            pants_color: 0,
            shirt_color: 0,
            shoes_color: 0,
            view_radius: 10,
            happiness: 100,
        }
    }

    /// `BlockheadCreationData` (0x48 bytes, uniqueID = u64::MAX) ++ XML plist `{name}`.
    pub fn to_blob(&self) -> Vec<u8> {
        let mut b = vec![0u8; 0x48];
        b[0..8].copy_from_slice(&u64::MAX.to_le_bytes());
        b[0x08..0x0c].copy_from_slice(&self.pos_x.to_le_bytes());
        b[0x0c..0x10].copy_from_slice(&self.pos_y.to_le_bytes());
        // 0x10 needsRemoved = 0, 0x11..0x18 pad zero
        b[0x18..0x2c].copy_from_slice(&self.skin);
        for (off, v) in [
            (0x2c, self.hat),
            (0x2e, self.pants),
            (0x30, self.shirt),
            (0x32, self.shoes),
            (0x34, self.hat_color),
            (0x36, self.pants_color),
            (0x38, self.shirt_color),
            (0x3a, self.shoes_color),
            (0x3c, self.view_radius),
        ] {
            b[off..off + 2].copy_from_slice(&v.to_le_bytes());
        }
        b[0x3e] = self.happiness;
        let mut m: GsDict<String, PV> = GsDict::new();
        m.insert("name".into(), PV::string(self.name.clone()));
        b.extend(codec::plist_xml(&PV::dict(m)).unwrap_or_default());
        b
    }
}

/// `YYYY-MM-DDTHH:MM:SSZ` for a unix time (civil-from-days). Used for the chat `date` field.
pub fn iso8601_utc(unix: u64) -> String {
    let days = (unix / 86400) as i64;
    let rem = unix % 86400;
    let z = days + 719468;
    let era = z.div_euclid(146097);
    let doe = z.rem_euclid(146097);
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z", y, m, d, rem / 3600, (rem / 60) % 60, rem % 60)
}

fn now_unix() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// Protocol-level client.
pub struct Session {
    pub client: Client,
    pub id: Identity,
    pub state: SessionState,
    pub config: SessionConfig,
    received: Vec<Rx>,
    pending: VecDeque<usize>,
    drain_cursor: usize,
    throttle: Throttle,
    last_rx: Instant,
}

type Res<T> = io::Result<T>;

impl Session {
    /// ENet connect; the handshake is separate ([`Session::join`]).
    pub fn connect(cfg: ClientConfig, id: Identity) -> Res<Session> {
        let mut client = Client::connect(cfg)?;
        client.set_label(&id.alias);
        Ok(Session {
            client,
            id,
            state: SessionState::default(),
            config: SessionConfig::default(),
            received: Vec::new(),
            pending: VecDeque::new(),
            drain_cursor: 0,
            throttle: Throttle::default(),
            last_rx: Instant::now(),
        })
    }

    pub fn set_trace(&mut self, sink: Option<TraceSink>) {
        self.client.set_trace(sink);
    }

    // ---------------------------------------------------------------- handshake (4.2)

    /// Wait for 0x23, send 0x1f, wait for 0x01 (approved) or 0x26 (rejected).
    pub fn join(&mut self, timeout_ms: u64) -> Res<JoinOutcome> {
        if self.wait_for(t::SERVER_INITIAL_CONTACT, timeout_ms)?.is_none() {
            return Ok(if self.state.disconnected { JoinOutcome::Disconnected } else { JoinOutcome::NoContact });
        }
        self.send_player_info()?;
        match self.wait_for_any(&[t::WORLD_SUMMARY, t::SERVER_CONNECTION_REJECTION], timeout_ms)? {
            Some(rx) if rx.data[0] == t::WORLD_SUMMARY => Ok(JoinOutcome::Approved),
            Some(rx) => Ok(JoinOutcome::Rejected { reason: rx.data.get(1).copied().unwrap_or(0) }),
            None => Ok(if self.state.disconnected { JoinOutcome::Disconnected } else { JoinOutcome::NoContact }),
        }
    }

    /// 0x1f only (rejection tests; a second one on an approved link drops the client).
    pub fn send_player_info(&mut self) -> Res<()> {
        let p = self.id.player_info_packet();
        self.client.send(&p)
    }

    /// 0x05.
    pub fn request_initial_data(&mut self) -> Res<()> {
        self.client.send(&[t::CLIENT_INITIAL_DATA_REQUEST])
    }

    // ---------------------------------------------------------------- receiving

    /// Pump the transport and process everything queued. Returns the number of packets processed.
    pub fn step(&mut self) -> Res<usize> {
        self.client.pump()?;
        let mut n = 0;
        while let Some(ev) = self.client.poll() {
            match ev {
                NetEvent::Packet(rx) => {
                    self.process(rx)?;
                    n += 1;
                }
                NetEvent::Disconnected { .. } => self.state.disconnected = true,
                NetEvent::Connected => {}
            }
        }
        if n == 0 {
            std::thread::sleep(Duration::from_millis(1));
        }
        Ok(n)
    }

    fn sync_in_flight(&mut self) {
        self.state.in_flight = self.throttle.in_flight.keys().map(|k| *k as u32).collect();
    }

    fn send_all(&mut self, pkts: Vec<Vec<u8>>) -> Res<()> {
        for p in pkts {
            self.client.send(&p)?;
        }
        Ok(())
    }

    /// Update `state` from one received packet.
    fn process(&mut self, rx: Rx) -> Res<()> {
        let idx = self.received.len();
        self.received.push(rx.clone());
        self.pending.push_back(idx);
        self.last_rx = Instant::now();
        let now = self.client.now_ms();
        let Ok(msg) = decode_server_packet(&rx.data) else { return Ok(()) };
        match msg {
            ServerMsg::Contact { version, plist } => {
                self.state.protocol_version = Some(version);
                if let Some(p) = &plist {
                    self.state.world_id = p.object_for_key("worldID").and_then(|v| v.as_str()).map(|s| s.to_string());
                    self.state.owner_name =
                        p.object_for_key("ownerName").and_then(|v| v.as_str()).map(|s| s.to_string());
                    self.state.requires_password =
                        p.object_for_key("requiresPassword").map(|v| v.bool_value()).unwrap_or(false);
                }
            }
            ServerMsg::Rejection { reason, .. } => self.state.last_rejection = Some(reason),
            ServerMsg::Summary(p) => {
                if let Some(p) = &p {
                    if let Some(w) = p.object_for_key("worldWidthMacro") {
                        let w = w.int_value();
                        if w > 0 {
                            self.state.world_width_macro = w;
                        }
                    }
                }
                self.state.summary = p;
                self.state.approved = true;
            }
            ServerMsg::PlayerList(p) => self.state.players = p,
            #[allow(clippy::collapsible_match)] // keeps one arm per message kind
            ServerMsg::Chat(p) => {
                if let Some(p) = p {
                    self.state.chat.push(p);
                }
            }
            ServerMsg::Heartbeat(h) => self.state.last_heartbeat = Some(h),
            ServerMsg::InitialData { food_types, plist } => {
                self.state.food_types = Some(food_types);
                self.state.initial_plist = plist;
            }
            ServerMsg::UniqueId(id) => self.state.unique_ids.push(id),
            ServerMsg::Block(b) => {
                let key = b.macro_index as i32;
                let requested = self.throttle.in_flight.contains_key(&key);
                let held = self.state.block_msgs.contains_key(&b.macro_index);
                if !requested && !held && self.config.auto_removal_notification {
                    // unsolicited block: tell the server we dropped it and ignore it
                    self.client.send(&block_wire::block_removal_notification(b.macro_index))?;
                    return Ok(());
                }
                self.check_block(&b);
                self.state.block_msgs.insert(b.macro_index, b);
                let out = self.throttle.answered(key, now);
                self.sync_in_flight();
                self.send_all(out)?;
            }
            ServerMsg::BlockNotLoaded { index, index32 } => {
                self.state.blocks_not_loaded.push(index);
                let out = self.throttle.answered(index32 as i32, now);
                self.sync_in_flight();
                self.send_all(out)?;
            }
            ServerMsg::LightBlock(l) => {
                let prev = self.state.light_update_index.get(&l.macro_index).copied();
                match prev {
                    Some(p) if l.update_index <= p => self
                        .state
                        .notes
                        .push(format!("stale light block {} upd {} <= {}", l.macro_index, l.update_index, p)),
                    _ => {
                        self.state.light_update_index.insert(l.macro_index, l.update_index);
                    }
                }
            }
            ServerMsg::Batch(b) => {
                for blob in &b.items {
                    let Some(uid) = blob_unique_id(blob) else { continue };
                    match b.kind {
                        k if k == t::SERVER_DYNAMIC_OBJECT_REMOVE => {
                            self.state.objects_seen.remove(&(b.object_type, uid));
                        }
                        k if k == t::SERVER_DYNAMIC_OBJECT_CREATE => {
                            self.state.objects_seen.insert((b.object_type, uid), blob.clone());
                            // BlockheadCreationData (0x48) ++ plist {clientID, clientName, name}
                            if b.object_type == 24 && blob.len() > 0x48 {
                                if let Some(p) = plist_from(&blob[0x48..]) {
                                    if p.object_for_key("clientID").and_then(|v| v.as_str())
                                        == Some(self.id.player_id().as_str())
                                    {
                                        self.state.created.push(CreatedObject {
                                            object_type: b.object_type,
                                            unique_id: uid,
                                            blob: blob.clone(),
                                        });
                                    }
                                }
                            }
                        }
                        _ => {
                            self.state.objects_seen.insert((b.object_type, uid), blob.clone());
                        }
                    }
                }
            }
            ServerMsg::Unknown { ty, .. } if ty == t::SERVER_BLOCK && rx.data.len() >= 5 => {
                // undecodable block: free the in-flight slot anyway and note it
                let i = u32::from_le_bytes(rx.data[1..5].try_into().unwrap());
                self.state.notes.push(format!("bad block {}", i));
                let out = self.throttle.answered(i as i32, now);
                self.sync_in_flight();
                self.send_all(out)?;
            }
            _ => {}
        }
        Ok(())
    }

    /// Hash check of a received block (reported as a note).
    fn check_block(&mut self, b: &BlockMsg) {
        let w = self.state.world_width_macro.max(1) as u32;
        let (x, y) = ((b.macro_index % w) as i32, (b.macro_index / w) as i32);
        let pid = self.id.player_id();
        let got =
            b.extra.as_ref().and_then(|e| e.object_for_key("hash")).and_then(|h| h.as_str()).unwrap_or("").to_string();
        if got != block_hash(&pid, x, y, false) && got != block_hash(&pid, x, y, true) {
            self.state.notes.push(format!("block {} hash mismatch ({})", b.macro_index, got));
        }
    }

    /// Wait for a packet of type `ty`.
    pub fn wait_for(&mut self, ty: u8, timeout_ms: u64) -> Res<Option<Rx>> {
        self.wait_for_matching(timeout_ms, &mut |rx| rx.data.first() == Some(&ty))
    }

    /// Wait for the first packet whose type is in `types`.
    pub fn wait_for_any(&mut self, types: &[u8], timeout_ms: u64) -> Res<Option<Rx>> {
        self.wait_for_matching(timeout_ms, &mut |rx| rx.data.first().is_some_and(|t| types.contains(t)))
    }

    /// Pump + process until `pred` accepts a not-yet-claimed packet (it is then claimed) or the timeout elapses.
    pub fn wait_for_matching(&mut self, timeout_ms: u64, pred: &mut dyn FnMut(&Rx) -> bool) -> Res<Option<Rx>> {
        let deadline = Instant::now() + Duration::from_millis(timeout_ms);
        loop {
            self.step()?;
            let mut found = None;
            for (pos, &i) in self.pending.iter().enumerate() {
                if pred(&self.received[i]) {
                    found = Some(pos);
                    break;
                }
            }
            if let Some(pos) = found {
                let i = self.pending.remove(pos).unwrap();
                return Ok(Some(self.received[i].clone()));
            }
            if self.state.disconnected || Instant::now() >= deadline {
                return Ok(None);
            }
        }
    }

    /// Run until no packet arrived for `quiet_ms` (at most `max_ms`).
    pub fn wait_quiet(&mut self, quiet_ms: u64, max_ms: u64) -> Res<()> {
        let deadline = Instant::now() + Duration::from_millis(max_ms);
        self.last_rx = Instant::now();
        loop {
            self.step()?;
            if self.last_rx.elapsed() >= Duration::from_millis(quiet_ms)
                || Instant::now() >= deadline
                || self.state.disconnected
            {
                return Ok(());
            }
        }
    }

    /// Pump + process for `ms` (keeps ENet alive).
    pub fn run_for(&mut self, ms: u64) -> Res<()> {
        let deadline = Instant::now() + Duration::from_millis(ms);
        while Instant::now() < deadline {
            self.step()?;
        }
        Ok(())
    }

    /// Packets processed since the last drain.
    pub fn drain(&mut self) -> Vec<Rx> {
        let v = self.received[self.drain_cursor..].to_vec();
        self.drain_cursor = self.received.len();
        v
    }

    /// All packets so far.
    pub fn received(&self) -> &[Rx] {
        &self.received
    }

    /// Number of packets of type `ty` received so far.
    pub fn count_of(&self, ty: u8) -> usize {
        self.received.iter().filter(|r| r.data.first() == Some(&ty)).count()
    }

    // ---------------------------------------------------------------- blocks (4.3)

    /// Raw 0x03, no throttling (the index is still tracked as requested).
    pub fn request_block(&mut self, macro_index: i32, create: bool) -> Res<()> {
        let now = self.client.now_ms();
        self.throttle.note_sent(macro_index, now);
        self.sync_in_flight();
        let p = block_wire::block_request(ClientMacroBlockRequest {
            macro_index,
            create_if_not_created: create as u8,
            padding: [0; 3],
        });
        self.client.send(&p)
    }

    /// `index = my * W + mx`.
    pub fn request_block_xy(&mut self, mx: i32, my: i32, create: bool) -> Res<()> {
        let i = my.wrapping_mul(self.state.world_width_macro).wrapping_add(mx);
        self.request_block(i, create)
    }

    /// Requests with the real client's rules; waits until all are answered or the timeout. Returns #answered.
    pub fn request_blocks_throttled(&mut self, idx: &[i32], create: bool, timeout_ms: u64) -> Res<usize> {
        self.config.auto_removal_notification = true;
        let before = self.count_of(t::SERVER_BLOCK) + self.count_of(t::SERVER_BLOCK_NOT_LOADED);
        for &i in idx {
            let now = self.client.now_ms();
            if let Some(p) = self.throttle.request(i, create, now) {
                self.client.send(&p)?;
            }
        }
        self.sync_in_flight();
        let deadline = Instant::now() + Duration::from_millis(timeout_ms);
        while (!self.throttle.in_flight.is_empty() || !self.throttle.queue.is_empty())
            && Instant::now() < deadline
            && !self.state.disconnected
        {
            self.step()?;
        }
        Ok(self.count_of(t::SERVER_BLOCK) + self.count_of(t::SERVER_BLOCK_NOT_LOADED) - before)
    }

    /// 0x0d.
    pub fn block_removal_notification(&mut self, macro_index: u32) -> Res<()> {
        self.client.send(&block_wire::block_removal_notification(macro_index))
    }

    /// `(tile_y >> 5) * W + (tile_x >> 5)`, x wrapped into the world width.
    pub fn macro_index_for_tile(&self, tile_x: i32, tile_y: i32) -> i32 {
        let w = self.state.world_width_macro;
        let wx = tile_x.rem_euclid(w.wrapping_mul(32).max(1));
        (tile_y >> 5).wrapping_mul(w).wrapping_add(wx >> 5)
    }

    /// Receipt value of a tile edit: `y * (W*32) + x`.
    pub fn fill_index(&self, x: i32, y: i32) -> i64 {
        y as i64 * (self.state.world_width_macro as i64 * 32) + x as i64
    }

    // ---------------------------------------------------------------- objects / blockheads (4.6, 4.7)

    /// 0x0e, waits for 0x0f.
    pub fn request_unique_id(&mut self) -> Res<Option<u64>> {
        self.client.send(&[t::CLIENT_UNIQUE_ID_REQUEST])?;
        Ok(self
            .wait_for(t::SERVER_UNIQUE_ID_REPLY, 5000)?
            .and_then(|rx| rx.data.get(1..9).map(|b| u64::from_le_bytes(b.try_into().unwrap()))))
    }

    /// 0x0a type 24.
    pub fn create_blockhead(&mut self, spec: &BlockheadSpec) -> Res<()> {
        self.send_object_batch(BatchKind::Create, 24, &[spec.to_blob()])
    }

    /// 0x0a / 0x0b / 0x0c / 0x45.
    pub fn send_object_batch(&mut self, kind: BatchKind, object_type: u8, blobs: &[Vec<u8>]) -> Res<()> {
        let p = object_batches::encode_client_batch(kind, object_type, blobs)
            .ok_or(io::Error::new(io::ErrorKind::InvalidData, "plist encode failed"))?;
        self.client.send(&p)
    }

    /// 0x20 `gzip(plist {dynamicObjects: [...]})`. `saved` may be the full dict or just the array.
    pub fn upload_blockheads(&mut self, saved: &PV) -> Res<()> {
        let d = match saved {
            PV::Array(_) => {
                let mut m: GsDict<String, PV> = GsDict::new();
                m.insert("dynamicObjects".into(), saved.clone());
                PV::dict(m)
            }
            other => other.clone(),
        };
        self.client.send(&messages::client_gz_plist(t::CLIENT_BLOCKHEADS, &d))
    }

    /// 0x21 `[u64 blockheadID] gzip(plist array of 8)`.
    pub fn upload_inventory(&mut self, blockhead_id: u64, slots: &PV) -> Res<()> {
        let mut p = vec![t::CLIENT_BLOCKHEAD_INVENTORY];
        p.extend_from_slice(&blockhead_id.to_le_bytes());
        p.extend(plist_gz(slots).unwrap_or_default());
        self.client.send(&p)
    }

    // ---------------------------------------------------------------- world edits (4.8)

    /// 0x10 (+ optional XML plist tail, not gzipped).
    pub fn fill(&mut self, r: FillNetRequest, plist_tail: Option<&PV>) -> Res<()> {
        self.client.send(&messages::fill_request(&r, plist_tail))
    }
    /// 0x11.
    pub fn remove(&mut self, r: RemoveNetRequest) -> Res<()> {
        self.client.send(&messages::client_struct(t::CLIENT_REMOVE_REQUEST, &r))
    }
    /// 0x3a.
    pub fn remove_back_wall(&mut self, r: RemoveBackWallNetRequest) -> Res<()> {
        self.client.send(&messages::client_struct(t::CLIENT_REMOVE_BACK_WALL_REQUEST, &r))
    }
    /// 0x19.
    pub fn paint(&mut self, r: PaintNetRequest) -> Res<()> {
        self.client.send(&messages::client_struct(t::CLIENT_PAINT_REQUEST, &r))
    }
    /// 0x15 struct ++ gzip plist (`None` = gzip of an empty dict).
    pub fn place_workbench(&mut self, r: PlaceWorkbenchNetRequest, tail: Option<&PV>) -> Res<()> {
        let mut p = messages::client_struct(t::CLIENT_PLACE_WORKBENCH_REQUEST, &r);
        p.extend(plist_gz(&tail.cloned().unwrap_or_else(|| PV::dict(GsDict::new()))).unwrap_or_default());
        self.client.send(&p)
    }
    /// 0x16 struct ++ gzip plist.
    pub fn place_interaction_object(&mut self, r: PlaceInteractionObjectNetRequest, tail: &PV) -> Res<()> {
        let mut p = messages::client_struct(t::CLIENT_PLACE_INTERACTION_OBJECT_REQUEST, &r);
        p.extend(plist_gz(tail).unwrap_or_default());
        self.client.send(&p)
    }
    /// 0x12.
    pub fn pickup(&mut self, requester_blockhead_id: u64, ids: &[u64]) -> Res<()> {
        self.client.send(&messages::pickup_request(requester_blockhead_id, ids))
    }
    /// 0x1a.
    pub fn gather(&mut self, r: GatherNetRequest) -> Res<()> {
        self.client.send(&messages::client_struct(t::GATHER_NOTIFICATION, &r))
    }
    /// 0x29.
    pub fn projectile(&mut self, r: ProjectileNetRequest) -> Res<()> {
        self.client.send(&messages::client_struct(t::CLIENT_PROJECTILE_REQUEST, &r))
    }
    /// 0x2d (UTF-8 text).
    pub fn tip(&mut self, text: &str) -> Res<()> {
        self.client.send(&frame(t::TIP_NOTIFICATION, text.as_bytes()))
    }
    /// 0x1b.
    pub fn zoom_request(&mut self, player_id: &str, cycle_index: i32) -> Res<()> {
        self.client.send(&messages::zoom_request(player_id, cycle_index))
    }
    /// 0x2e gzip plist.
    pub fn trade(&mut self, prices: &PV) -> Res<()> {
        self.client.send(&messages::client_gz_plist(t::CLIENT_TRADE_TRANSACTION, prices))
    }

    // ---------------------------------------------------------------- heartbeat / rules (5.4)

    /// 0x18.
    pub fn heartbeat_request_old(&mut self) -> Res<()> {
        self.client.send(&[t::CLIENT_WORLD_HEARTBEAT_REQUEST])
    }
    /// 0x44 `gzip(64 bytes)`; `None` = the default image (5.4).
    pub fn heartbeat_request_with_rules(&mut self, rules: Option<[u8; 64]>) -> Res<()> {
        let r = rules.unwrap_or_else(|| rules_image_for_summary(self.state.summary.as_ref()));
        self.client.send(&frame(t::CLIENT_WORLD_HEARTBEAT_REQUEST_WITH_CUSTOM_RULES, &gz(&r)))
    }
    /// 0x17 (16 bytes).
    pub fn send_heartbeat(&mut self, h: &WorldHeartbeat) -> Res<()> {
        self.client.send(&messages::client_heartbeat(h))
    }

    // ---------------------------------------------------------------- chat / commands (4.5)

    /// 0x24 `{message, alias, date}` in Apple dialect.
    pub fn chat(&mut self, text: &str) -> Res<()> {
        let s = format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?><plist version=\"1.0\"><dict><key>message</key><string>{}</string><key>alias</key><string>{}</string><key>date</key><date>{}</date></dict></plist>",
            xml_escape(text),
            xml_escape(&self.id.alias),
            iso8601_utc(now_unix())
        );
        self.send_xml(t::CHAT_MESSAGE, &s)
    }
    /// 0x24 with an arbitrary dict (server-writer dialect).
    pub fn chat_dict(&mut self, dict: &PV) -> Res<()> {
        self.client.send(&messages::client_chat(dict))
    }
    /// Send `line` as chat and collect all 0x24 dicts until 700 ms of quiet.
    pub fn command(&mut self, line: &str) -> Res<Vec<PV>> {
        let start = self.received.len();
        self.chat(line)?;
        self.wait_quiet(700, 10_000)?;
        let mut out = Vec::new();
        for i in start..self.received.len() {
            let d = &self.received[i].data;
            if d.first() == Some(&t::CHAT_MESSAGE) {
                if let Some(p) = plist_from(&d[1..]) {
                    out.push(p);
                }
            }
        }
        let rx = &self.received;
        self.pending.retain(|&i| i < start || rx[i].data.first() != Some(&t::CHAT_MESSAGE));
        Ok(out)
    }
    /// 0x35.
    pub fn welcome_get(&mut self) -> Res<()> {
        self.client.send(&[t::CLIENT_WELCOME_MESSAGE_GET_REQUEST])
    }
    /// 0x34 `gzip(plist {newWelcomeMessage})` (admin only).
    pub fn welcome_set(&mut self, text: &str) -> Res<()> {
        let mut m: GsDict<String, PV> = GsDict::new();
        m.insert("newWelcomeMessage".into(), PV::string(text));
        self.client.send(&messages::client_gz_plist(t::CLIENT_WELCOME_MESSAGE_SET_REQUEST, &PV::dict(m)))
    }
    /// 0x3b.
    pub fn ownership_sign_list(&mut self) -> Res<()> {
        self.client.send(&[t::CLIENT_OWNERSHIP_SIGN_PLAYER_LIST_REQUEST])
    }
    /// 0x3e / 0x40 with an 8 byte payload whose first u16 is `id`.
    pub fn portal_chest(&mut self, id: u16, resend: bool) -> Res<()> {
        let mut raw = [0u8; 8];
        raw[..2].copy_from_slice(&id.to_le_bytes());
        let ty = if resend { t::CLIENT_PORTAL_CHEST_TRANSACTION_RESEND } else { t::CLIENT_PORTAL_CHEST_TRANSACTION };
        self.client.send(&frame(ty, &raw))
    }

    // ---------------------------------------------------------------- escape hatches

    /// Any bytes (byte 0 = type), traced like any send.
    pub fn send_raw(&mut self, packet: &[u8]) -> Res<()> {
        self.client.send(packet)
    }
    /// One unreliable, unsequenced packet on channel 0 (0xBC voice frames).
    pub fn send_unsequenced(&mut self, packet: &[u8]) -> Res<()> {
        self.client.send_unsequenced(0, packet)
    }
    /// `[ty] ++ plist text` (hand written Apple-dialect plist).
    pub fn send_xml(&mut self, ty: u8, plist_text: &str) -> Res<()> {
        let mut p = vec![ty];
        p.extend_from_slice(plist_text.as_bytes());
        self.client.send(&p)
    }
    /// ENet disconnect (waits up to 3 s).
    pub fn disconnect(&mut self) {
        self.client.disconnect(0);
        let _ = self.client.pump();
        self.state.disconnected = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trace::to_hex;
    use blockheads_server::types::netdata::BlockheadCreationData;

    /// U3.
    #[test]
    fn u3_player_id() {
        assert_eq!(Identity::new("tester").player_id(), "f5d1278e8109edd94e1e4197e04873b9");
        assert_eq!(Identity::new("Server").player_id(), "cf1e8c14e54505f60aa10ceb8d5d8ab3");
        assert_eq!(Identity::new("SERVER").player_id(), "cf1e8c14e54505f60aa10ceb8d5d8ab3");
    }

    /// U4.
    #[test]
    fn u4_blockhead_blob() {
        let s = BlockheadSpec::new("Bob", 14557, 518);
        let b = s.to_blob();
        assert!(b.len() > 0x48);
        assert_eq!(&b[0..8], &[0xff; 8]);
        assert_eq!(&b[0x11..0x18], &[0; 7]);
        assert_eq!(u16::from_le_bytes([b[0x3c], b[0x3d]]), 10);
        assert_eq!(b[0x3e], 100);
        let d = BlockheadCreationData::read_le(&b);
        assert_eq!(d.dynamic_object_net_data.unique_id, u64::MAX);
        assert_eq!(d.view_radius, 10);
        assert_eq!(d.to_vec(), b[..0x48].to_vec());
        let p = plist_from(&b[0x48..]).unwrap();
        assert_eq!(p.object_for_key("name").and_then(|v| v.as_str()), Some("Bob"));
    }

    /// U5.
    #[test]
    fn u5_encoders() {
        let f = FillNetRequest { pos_x: 6837, pos_y: 545, type_: 0x0b, ..Default::default() };
        assert_eq!(to_hex(&messages::fill_request(&f, None)), "10b51a0000210200000b00000000000000");
        let r = RemoveNetRequest { pos_x: 6837, pos_y: 545, ..Default::default() };
        assert_eq!(
            to_hex(&messages::client_struct(t::CLIENT_REMOVE_REQUEST, &r)),
            "1100000000000000".to_string() + "00b51a0000210200000000000000000000"
        );
        assert_eq!(to_hex(&Throttle::packet(8646, true)), "03c621000001000000");
        assert_eq!(to_hex(&block_wire::block_removal_notification(5)), "0d05000000");
    }

    /// U6.
    #[test]
    fn u6_player_info() {
        let id = Identity::new("tester");
        let p = id.player_info_packet();
        assert_eq!(p[0], 0x1f);
        let text = std::str::from_utf8(&p[1..]).unwrap();
        assert!(text.starts_with("<?xml version=\"1.0\" encoding=\"UTF-8\"?><plist version=\"1.0\"><dict><key>alias</key><string>tester</string><key>udidNew</key><string>udid-tester</string><key>minorVersion</key><integer>3</integer></dict>"));
        let d = plist_from(&p[1..]).unwrap();
        assert_eq!(d.object_for_key("alias").and_then(|v| v.as_str()), Some("tester"));
        assert_eq!(d.object_for_key("minorVersion").map(|v| v.int_value()), Some(3));
        assert!(d.object_for_key("clientPassword").is_none() && d.object_for_key("photo").is_none());
        let g = Identity::new("tester")
            .password("pw")
            .photo(vec![1, 2, 3])
            .dialect(PlistDialect::GnuStep)
            .player_info_packet();
        let d = plist_from(&g[1..]).unwrap();
        assert_eq!(d.object_for_key("clientPassword").and_then(|v| v.as_str()), Some("pw"));
        assert_eq!(&g[1..], &codec::plist_xml(&d).unwrap()[..]);
        let a = Identity::new("x").password("p<w").photo(vec![1, 2, 3]).player_info_packet();
        let d = plist_from(&a[1..]).unwrap();
        assert_eq!(d.object_for_key("clientPassword").and_then(|v| v.as_str()), Some("p<w"));
        assert!(d.object_for_key("photo").is_some());
    }

    /// U7.
    #[test]
    fn u7_throttle() {
        let mut t = Throttle::default();
        let mut sent = 0;
        for i in 0..20 {
            if t.request(i, true, 0).is_some() {
                sent += 1;
            }
        }
        assert_eq!(sent, 17);
        assert_eq!(t.queue.len(), 3);
        // no queue duplicates, no re-send of in-flight indices before 5 s
        assert!(t.request(18, true, 0).is_none());
        assert_eq!(t.queue.len(), 3);
        assert!(t.request(3, true, 4000).is_none());
        assert!(t.request(3, true, 5000).is_none());
        assert!(t.request(3, true, 5001).is_some());
        assert_eq!(t.in_flight.get(&3), Some(&0));
        // first answer: 16 in flight, no drain; second: 15 -> one queued request goes out
        assert!(t.answered(0, 10).is_empty());
        assert_eq!(t.answered(1, 10).len(), 1);
        assert_eq!(t.queue.len(), 2);
        assert_eq!(t.answered(2, 10).len(), 1);
        assert_eq!(t.in_flight.len(), 15 + 2 - 1);
    }

    /// Expert default differs from non-expert in deathDrops/teleportation/meditation only.
    #[test]
    fn expert_rules_image() {
        let mut m = blockheads_server::foundation::gsdict::GsDict::new();
        m.insert("expertMode".to_string(), PV::boolean(true));
        let img = rules_image_for_summary(Some(&PV::dict(m)));
        assert_eq!(
            to_hex(&img),
            "0000020200000000020204020101020201020202010100010101010101010100020002000200020002000200020000000202020501007f7f7f00000000010100"
        );
        assert_eq!(rules_image_for_summary(None), DEFAULT_CUSTOM_RULES_IMAGE.map(|b| b));
    }

    /// U9: the image is the 64-byte single-line form of the spec.
    #[test]
    fn u9_rules_image() {
        assert_eq!(
            to_hex(&DEFAULT_CUSTOM_RULES_IMAGE),
            "0000020200000000020204020101020201020202010100010101010101010100020002000200020002000200020000000202020501007f7f7f01000000000000"
        );
    }

    /// Loopback: a tiny in-process ENet server drives join, block request, uid and rejection paths.
    #[test]
    fn loopback_join() {
        use rusty_enet as enet;
        let sock = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
        let addr = sock.local_addr().unwrap();
        let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let stop2 = stop.clone();
        let th = std::thread::spawn(move || {
            let mut host =
                enet::Host::new(sock, enet::HostSettings { peer_limit: 4, channel_limit: 8, ..Default::default() })
                    .unwrap();
            while !stop2.load(std::sync::atomic::Ordering::SeqCst) {
                let ev = host.service().unwrap().map(|e| e.no_ref());
                match ev {
                    Some(enet::EventNoRef::Connect { peer, .. }) => {
                        let p = messages::initial_contact(None, false, "wid");
                        host.peer_mut(peer).send(0, &enet::Packet::reliable(p)).unwrap();
                    }
                    Some(enet::EventNoRef::Receive { peer, packet, .. }) => {
                        let d = packet.data().to_vec();
                        let reply: Option<Vec<u8>> = match d[0] {
                            0x1f => {
                                let alias = plist_from(&d[1..])
                                    .and_then(|p| p.object_for_key("alias").and_then(|a| a.as_str().map(String::from)))
                                    .unwrap_or_default();
                                Some(if alias == "BAD" {
                                    messages::rejection(14)
                                } else {
                                    messages::world_summary(&messages::WorldSummary {
                                        world_width_macro: 256,
                                        ..Default::default()
                                    })
                                })
                            }
                            _ => Some(messages::unique_id_reply(0x106)),
                        };
                        if let Some(r) = reply {
                            host.peer_mut(peer).send(0, &enet::Packet::reliable(r)).unwrap();
                        }
                    }
                    Some(enet::EventNoRef::Disconnect { .. }) | None => std::thread::sleep(Duration::from_millis(1)),
                }
            }
        });
        let cfg = ClientConfig { addr, ..Default::default() };
        let mut s = Session::connect(cfg.clone(), Identity::new("tester")).unwrap();
        let mem = TraceSink::memory();
        s.set_trace(Some(mem.clone()));
        assert_eq!(s.join(3000).unwrap(), JoinOutcome::Approved);
        assert_eq!(s.state.world_id.as_deref(), Some("wid"));
        assert_eq!(s.state.protocol_version, Some(0x26));
        assert_eq!(s.state.world_width_macro, 256);
        assert_eq!(s.request_unique_id().unwrap(), Some(0x106));
        assert_eq!(s.macro_index_for_tile(-1, 40), 256 + 255);
        let recs = mem.records();
        assert_eq!(recs[0].client, "tester");
        assert!(recs.iter().any(|r| r.dir == crate::trace::Dir::Tx && r.ty == 0x1f));
        s.disconnect();
        let mut b = Session::connect(cfg, Identity::new("BAD")).unwrap();
        assert_eq!(b.join(3000).unwrap(), JoinOutcome::Rejected { reason: 14 });
        assert_eq!(b.state.last_rejection, Some(14));
        b.disconnect();
        stop.store(true, std::sync::atomic::Ordering::SeqCst);
        th.join().unwrap();
    }

    #[test]
    fn iso_date() {
        assert_eq!(iso8601_utc(1767225600), "2026-01-01T00:00:00Z");
        assert_eq!(iso8601_utc(0), "1970-01-01T00:00:00Z");
    }
}
