//! `BHNetServerMatch`: the server's transport layer on top of one ENet host.
//!
//! (sections 3, 4.1 to 4.30 and "Synthesis resolutions").

use crate::foundation::clock::reference_time;
use crate::foundation::exception::{catch_ns, ns_raise, unrecognized_selector};
use crate::foundation::gsdict::GsDict;
use crate::foundation::md5::string_from_md5;
use crate::foundation::nslog::nslog;
use crate::foundation::nsstring::{int_value, lowercase_string, safe_case_insensitive_compare, uppercase_string};
use crate::net::bhmatch::*;
use crate::net::enet_host::{enet_id_string, EnetHost, OwnedEvent, PeerSlot};
use crate::plist::value::{PlistDict, PlistValue};
use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};
use std::thread::ThreadId;

/// Alias so other modules can use either spelling.
pub type BHNetServerMatch = BhNetServerMatch;

/// Arguments of `-initWithPort:userName:photo:hasLocalPLayer:worldName:credit:cloudSalt:ownerName:privacy:`.
pub struct ServerMatchArgs {
    /// `--port` string; `None` gives 15151.
    pub port: Option<String>,
    /// GameController passes `Some("server")`.
    pub user_name: Option<String>,
    /// false on the dedicated server.
    pub has_local_player: bool,
    /// Stored, unused.
    pub world_name: Option<String>,
    /// `(float)credit`, -1.0 for ordinary worlds.
    pub credit: f32,
    /// Cloud salt (non-nil means cloud match).
    pub cloud_salt: Option<String>,
    /// Kept even when empty (unlike `set_owner`).
    pub owner_name: Option<String>,
    /// Privacy string.
    pub privacy: Option<String>,
}

/// Bind failure of `startServer`; GameController logs "Failed to start server. Perhaps port %@ is already in use?".
#[derive(Debug)]
pub struct StartServerError;

/// 1/60 s poll interval.
const POLL_INTERVAL: f64 = 0.016666666666666666;
/// `0.01f32` widened to f64.
const CREDIT_THRESHOLD: f64 = 0.009_999_999_776_482_582;

/// Class `BHNetServerMatch` (subclass of `BHMatch`).
pub struct BhNetServerMatch {
    weak_self: Weak<BhNetServerMatch>,
    base: BhMatchBase,
    timers: Rc<dyn TimerScheduler>,
    main_thread: Cell<ThreadId>,
    host: RefCell<Option<EnetHost>>,
    /// ENet id -> peer. `None` after `disconnect` (ObjC nil).
    connections: RefCell<Option<GsDict<String, PeerSlot>>>,
    persistent_ids_to_enet_ids: RefCell<Option<GsDict<String, String>>>,
    enet_ids_to_persistent_ids: RefCell<Option<GsDict<String, String>>>,
    player_infos: RefCell<Vec<PlayerInfo>>,
    user_name: String,
    #[allow(dead_code)]
    world_name: Option<String>,
    owner_name: RefCell<Option<String>>,
    cloud_salt: RefCell<Option<String>>,
    port_to_use: u16,
    #[allow(dead_code)]
    has_local_player: bool,
    poll_timer: Cell<Option<TimerId>>,
    no_credit_boot_timer: Cell<f32>,
    credit: Cell<f64>,
    poll_timer_last_time: Cell<f64>,
    privacy: Cell<u64>,
}

impl BhNetServerMatch {
    /// `-initWithPort:...`. `Ok` is non-nil; `Err` is nil (init already ran `disconnect`; the delegate was nil so
    /// no callback happened).
    pub fn init_with_port_user_name_photo_has_local_player_world_name_credit_cloud_salt_owner_name_privacy(
        args: ServerMatchArgs,
        timers: Rc<dyn TimerScheduler>,
    ) -> Result<Rc<BhNetServerMatch>, StartServerError> {
        // gethostname() fallback is only reached when userName_ is nil (unreachable on the dedicated server).
        let user_name = match args.user_name {
            Some(u) => u,
            None => crate::foundation::paths::host_name(),
        };
        // Truncating cast of intValue to 16 bits ("0" -> ephemeral port, "70000" -> 4464).
        let port_to_use: u16 = match &args.port {
            Some(p) => int_value(p) as u16,
            None => 15151,
        };
        let m = Rc::new_cyclic(|w| BhNetServerMatch {
            weak_self: w.clone(),
            base: BhMatchBase::default(),
            timers,
            main_thread: Cell::new(std::thread::current().id()),
            host: RefCell::new(None),
            connections: RefCell::new(Some(GsDict::new())),
            persistent_ids_to_enet_ids: RefCell::new(Some(GsDict::new())),
            enet_ids_to_persistent_ids: RefCell::new(Some(GsDict::new())),
            player_infos: RefCell::new(Vec::new()),
            user_name,
            world_name: args.world_name,
            owner_name: RefCell::new(args.owner_name),
            cloud_salt: RefCell::new(args.cloud_salt),
            port_to_use,
            has_local_player: args.has_local_player,
            poll_timer: Cell::new(None),
            no_credit_boot_timer: Cell::new(0.0),
            credit: Cell::new(args.credit as f64),
            poll_timer_last_time: Cell::new(0.0),
            privacy: Cell::new(0),
        });
        m.set_privacy_by_string(args.privacy.as_deref());
        if args.has_local_player {
            // `[super loadInfoForPlayer:]` bypasses the override: fresh dict.
            let info = bhmatch_load_info_for_player(&m.local_player_id());
            {
                let mut d = info.borrow_mut();
                d.insert("alias".into(), PlistValue::string(m.user_name.clone()));
                d.insert("local".into(), PlistValue::boolean(true));
                d.insert("micOrSpeakerOn".into(), PlistValue::boolean(true));
                d.insert("voiceConnected".into(), PlistValue::boolean(false));
            }
            m.player_infos.borrow_mut().push(info);
        }
        if !m.start_server() {
            m.disconnect();
            return Err(StartServerError);
        }
        Ok(m)
    }

    fn delegate(&self) -> Option<Rc<dyn MatchDelegate>> {
        self.base.delegate.borrow().as_ref().and_then(|w| w.upgrade())
    }

    /// `-startServer`.
    pub fn start_server(&self) -> bool {
        if self.host.borrow().is_some() {
            return false;
        }
        match EnetHost::create(self.port_to_use) {
            Ok(h) => *self.host.borrow_mut() = Some(h),
            Err(_) => {
                if let Some(d) = self.delegate() {
                    d.match_did_fail_with_error(self);
                }
                return false;
            }
        }
        self.poll_timer_last_time.set(reference_time());
        let weak = self.weak_self.clone();
        let id = self.timers.schedule_repeating(
            POLL_INTERVAL,
            Box::new(move || {
                if let Some(m) = weak.upgrade() {
                    m.poll_net_events();
                }
            }),
        );
        self.poll_timer.set(Some(id));
        true
    }

    /// Local UDP port of the ENet host (testing aid; not in the original).
    pub fn local_port(&self) -> Option<u16> {
        self.host.borrow().as_ref().map(|h| h.local_port())
    }

    /// `-pollNetEvents`: 60 Hz timer callback wrapped in `@try`; an NSException logs and `exit(0)`.
    pub fn poll_net_events(&self) {
        match catch_ns(|| self.poll_body()) {
            Ok(()) => {
                // Mods (not in the original): next-poll tasks.
                if let Some(h) = crate::mods::hooks() {
                    h.poll();
                }
            }
            Err(ex) => {
                nslog(&format!(
                    "Exiting due to exception in pollNetEvents '{}' (reason '{}') ",
                    ex.name,
                    ex.reason_or_null()
                ));
                crate::foundation::sys::exit(0);
            }
        }
    }

    fn poll_body(&self) {
        let now = reference_time();
        let mut dt = now - self.poll_timer_last_time.get();
        self.poll_timer_last_time.set(now);
        if dt < 0.0 {
            dt = 0.0;
        }
        if self.is_cloud_match() {
            self.credit.set(self.credit.get() - dt);
            if self.credit.get() <= CREDIT_THRESHOLD {
                self.no_credit_boot_timer.set((self.no_credit_boot_timer.get() as f64 - dt) as f32);
                if self.no_credit_boot_timer.get() <= 0.0f32 {
                    if let Some(d) = self.delegate() {
                        d.boot_all_clients_due_to_no_credit();
                    }
                    self.no_credit_boot_timer.set(10.0f32);
                }
            }
        }
        loop {
            // The host borrow is released before any delegate call.
            let ev = {
                let mut h = self.host.borrow_mut();
                match h.as_mut() {
                    Some(h) => h.service0(),
                    None => None,
                }
            };
            let Some(ev) = ev else { break };
            match ev {
                OwnedEvent::Connect { peer, addr } => {
                    let id = enet_id_string(&addr);
                    if let Some(c) = self.connections.borrow_mut().as_mut() {
                        c.insert(id.clone(), peer);
                    }
                    if let Some(d) = self.delegate() {
                        d.match_player_did_change_state(self, &id, PlayerConnectionState::Connected);
                    }
                }
                OwnedEvent::Disconnect { addr, .. } => {
                    let id = enet_id_string(&addr);
                    let pid = self
                        .enet_ids_to_persistent_ids
                        .borrow()
                        .as_ref()
                        .and_then(|m| m.get(&id).cloned())
                        .unwrap_or_else(|| id.clone());
                    if let Some(c) = self.connections.borrow_mut().as_mut() {
                        c.remove(&id);
                    }
                    if let Some(m) = self.enet_ids_to_persistent_ids.borrow_mut().as_mut() {
                        m.remove(&id);
                    }
                    // Unconditional, even when `pid` was remapped to a newer connection (bug-compatible, E4).
                    if let Some(m) = self.persistent_ids_to_enet_ids.borrow_mut().as_mut() {
                        m.remove(&pid);
                    }
                    self.remove_client(&pid);
                    if let Some(d) = self.delegate() {
                        d.match_player_did_change_state(self, &pid, PlayerConnectionState::Disconnected);
                    }
                }
                OwnedEvent::Receive { addr, data, .. } => {
                    let id = enet_id_string(&addr);
                    let pid = self
                        .enet_ids_to_persistent_ids
                        .borrow()
                        .as_ref()
                        .and_then(|m| m.get(&id).cloned())
                        .unwrap_or(id);
                    if let Some(d) = self.delegate() {
                        d.match_did_receive_data_from_player(self, &data, &pid);
                    }
                }
            }
        }
    }

    /// `-setPrivacyByString:`: Public first, then case-insensitive match.
    pub fn set_privacy_by_string(&self, s: Option<&str>) {
        self.privacy.set(1);
        if s.is_some() {
            if safe_case_insensitive_compare(s, Some("public")) == 0 {
                self.privacy.set(1);
            } else if safe_case_insensitive_compare(s, Some("searchable")) == 0 {
                self.privacy.set(2);
            } else if safe_case_insensitive_compare(s, Some("private")) == 0 {
                self.privacy.set(3);
            }
        }
    }

    /// `-credit`: f64 narrowed to f32.
    pub fn credit(&self) -> f32 {
        self.credit.get() as f32
    }
    /// `-setCredit:`: f32 widened.
    pub fn set_credit(&self, c: f32) {
        self.credit.set(c as f64);
    }
    /// `-setOwner:`: nil or empty (UTF-16 length 0) clears.
    pub fn set_owner(&self, name: Option<&str>) {
        *self.owner_name.borrow_mut() = match name {
            Some(s) if !s.is_empty() => Some(s.to_string()),
            _ => None,
        };
    }
    /// `-setCloudSalt:`: stored as given.
    pub fn set_cloud_salt(&self, s: Option<&str>) {
        *self.cloud_salt.borrow_mut() = s.map(|s| s.to_string());
    }
    /// `-privacy`.
    pub fn privacy(&self) -> u64 {
        self.privacy.get()
    }
    /// `-setPrivacy:`: no validation.
    pub fn set_privacy(&self, p: u64) {
        self.privacy.set(p);
    }
}

impl BhNetServerMatch {
    /// The persistent id a hello stands for (the MD5 of the lower-cased alias); none without an alias or when it
    /// is the server's own.
    fn persistent_id_of(&self, client: &PlistDict) -> Option<String> {
        let persistent_id: Option<String> = match client.get(&"alias".to_string()) {
            None => None,
            Some(PlistValue::String(s)) => Some(string_from_md5(&lowercase_string(&s.s))),
            Some(other) => unrecognized_selector(other.class_name(), "lowercaseString"),
        };
        persistent_id.filter(|p| *p != self.local_player_id())
    }

    /// The info dict of a hello from `enet_or_player_id` (no map changes).
    fn info_for_hello(&self, client: &PlistDict, persistent_id: &str, enet_or_player_id: &str) -> PlayerInfo {
        let key = |k: &str| k.to_string();
        let mut d = new_info_dict();
        d.insert(key("playerID"), PlistValue::string(persistent_id.to_string()));
        if let Some(a) = client.get(&key("alias")) {
            d.insert(key("alias"), a.clone());
        }
        if let Some(p) = client.get(&key("photo")) {
            d.insert(key("photo"), p.clone());
        }
        d.insert(key("local"), PlistValue::boolean(false));
        d.insert(key("micOrSpeakerOn"), PlistValue::boolean(true));
        d.insert(key("voiceConnected"), PlistValue::boolean(false));
        d.insert(key("connected"), PlistValue::boolean(true));
        if let Some(c) = client.get(&key("cloudKey")) {
            d.insert(key("cloudKey"), c.clone());
        }
        // peer = connections[persistentIdsToEnetIds[persistentID]]; the map entry is `enet_or_player_id` by now.
        let peer = self.connections.borrow().as_ref().and_then(|c| c.get(&enet_or_player_id.to_string()).copied());
        if let Some(peer) = peer {
            let ip = self.host.borrow().as_ref().and_then(|h| h.peer_ip_string(peer));
            if let Some(ip) = ip {
                d.insert(key("ip"), PlistValue::string(ip));
            }
        }
        for k in ["udidNew", "iCloudID", "gameCenterID", "minorVersion", "clientPassword"] {
            if let Some(v) = client.get(&key(k)) {
                d.insert(key(k), v.clone());
            }
        }
        Rc::new(std::cell::RefCell::new(d))
    }

    /// The "same name from another device" setting: the connection `stored` that holds the name goes. It is told
    /// why, its player state is released as for a disconnect, and the connection is closed once that message left.
    fn replace_session(&self, persistent_id: &str, stored: &str) {
        let old_peer = self.connections.borrow().as_ref().and_then(|c| c.get(&stored.to_string()).copied());
        let Some(old_peer) = old_peer else { return };
        nslog("name taken over by a new connection");
        for d in self.delegate() {
            d.name_taken_over(persistent_id);
        }
        self.remove_client(persistent_id);
        for d in self.delegate() {
            d.match_player_did_change_state(self, persistent_id, PlayerConnectionState::Disconnected);
        }
        // Out of the maps, so the connection's own disconnect event cannot touch the player that took over.
        for c in self.connections.borrow_mut().as_mut() {
            c.remove(&stored.to_string());
        }
        for m in self.enet_ids_to_persistent_ids.borrow_mut().as_mut() {
            m.remove(&stored.to_string());
        }
        // `old_peer` came from `connections`, which only exists together with the host.
        for h in self.host.borrow_mut().as_mut() {
            h.peer_disconnect_later(old_peer, 0);
        }
    }
}

impl BhMatch for BhNetServerMatch {
    fn set_delegate(&self, d: Option<Weak<dyn MatchDelegate>>) {
        *self.base.delegate.borrow_mut() = d;
    }

    /// `[[userName lowercaseString] stringFromMD5]`.
    fn local_player_id(&self) -> String {
        string_from_md5(&lowercase_string(&self.user_name))
    }

    /// `[userName uppercaseString]`.
    fn local_player_name(&self) -> String {
        uppercase_string(&self.user_name)
    }

    /// `[persistentIdsToEnetIds allKeys]` in GsDict order.
    fn player_ids(&self) -> Option<Vec<String>> {
        self.persistent_ids_to_enet_ids.borrow().as_ref().map(|m| m.keys_in_order())
    }

    /// `-remove_client`: first matching entry only.
    fn remove_client(&self, id: &str) {
        let mut infos = self.player_infos.borrow_mut();
        if let Some(i) = infos.iter().position(|d| dict_player_id(d).as_deref() == Some(id)) {
            infos.remove(i);
        }
    }

    /// `-disconnectPlayer:`: graceful `enet_peer_disconnect(peer, 0)`; no map cleanup.
    fn disconnect_player(&self, id: &str) {
        let enet_id = self.persistent_ids_to_enet_ids.borrow().as_ref().and_then(|m| m.get(&id.to_string()).cloned());
        let Some(enet_id) = enet_id else { return };
        let peer = self.connections.borrow().as_ref().and_then(|c| c.get(&enet_id).copied());
        // A connected id always has a host.
        for (peer, h) in peer.into_iter().zip(self.host.borrow_mut().as_mut()) {
            h.peer_disconnect(peer, 0);
        }
    }

    /// `-disconnect`: graceful shutdown with up to 3 s blocking waits.
    fn disconnect(&self) {
        if let Some(t) = self.poll_timer.take() {
            self.timers.invalidate(t);
        }
        let keys = self.connections.borrow().as_ref().map(|c| c.keys_in_order()).unwrap_or_default();
        {
            let c = self.connections.borrow();
            let mut h = self.host.borrow_mut();
            if let (Some(c), Some(h)) = (c.as_ref(), h.as_mut()) {
                for k in &keys {
                    if let Some(p) = c.get(k) {
                        h.peer_disconnect_later(*p, 0);
                    }
                }
            }
        }
        loop {
            let n = self.connections.borrow().as_ref().map_or(0, |c| c.len());
            if n == 0 {
                break;
            }
            let ev = self.host.borrow_mut().as_mut().and_then(|h| h.service_wait(3000));
            // Nothing within 3 s ends the wait; only a disconnect shortens the list of connections.
            let Some(ev) = ev else { break };
            let OwnedEvent::Disconnect { addr, .. } = ev else { continue };
            for c in self.connections.borrow_mut().as_mut() {
                c.remove(&enet_id_string(&addr));
            }
        }
        {
            let c = self.connections.borrow();
            let mut h = self.host.borrow_mut();
            if let (Some(c), Some(h)) = (c.as_ref(), h.as_mut()) {
                if !c.is_empty() {
                    for k in c.keys_in_order() {
                        if let Some(p) = c.get(&k) {
                            h.peer_reset(*p);
                        }
                    }
                }
            }
        }
        *self.connections.borrow_mut() = None;
        *self.persistent_ids_to_enet_ids.borrow_mut() = None;
        *self.enet_ids_to_persistent_ids.borrow_mut() = None;
        if let Some(h) = self.host.borrow_mut().take() {
            h.destroy();
        }
        // [super disconnect]: BHMatch no-op.
    }

    /// `-sendData:toPlayers:withDataMode:error:`: always reliable, channel = enumeration index over `connections`
    /// (incremented for every key), never flushed; always YES.
    fn send_data_to_players_with_data_mode_error(
        &self,
        data: Option<&[u8]>,
        players: &[String],
        _mode: SendDataMode,
    ) -> bool {
        if std::thread::current().id() != self.main_thread.get() {
            // Note: exact GNUstep `-[NSThread description]` text of the two `%@` arguments is unknown
            // (unreachable: the port is single-threaded).
            nslog(&format!(
                "Whoops, Trying to send data off the main thread {:?} {:?}",
                self.main_thread.get(),
                std::thread::current().id()
            ));
            ns_raise("Background thread network send", "Attempting to send data from a background thread.");
        }
        let keys = match self.connections.borrow().as_ref() {
            Some(c) => c.keys_in_order(),
            None => return true,
        };
        let mut channel: i32 = 0;
        for key in keys {
            let pid = self
                .enet_ids_to_persistent_ids
                .borrow()
                .as_ref()
                .and_then(|m| m.get(&key).cloned())
                .unwrap_or_else(|| key.clone());
            if players.contains(&pid) {
                // Every key of `connections` has a peer, and the host lives as long as `connections` does.
                let peer = self.connections.borrow().as_ref().and_then(|c| c.get(&key).copied());
                for (peer, h) in peer.into_iter().zip(self.host.borrow_mut().as_mut()) {
                    h.send_reliable(peer, channel as u8, data.unwrap_or(&[]));
                }
            }
            channel += 1;
        }
        true
    }

    /// `-loadInfoForPlayer:`: first `playerInfos` dict with that playerID, no creation.
    fn load_info_for_player(&self, player_id: &str) -> Option<PlayerInfo> {
        self.player_infos.borrow().iter().find(|d| dict_player_id(d).as_deref() == Some(player_id)).cloned()
    }

    /// `-clientPlayerInformationRecieved:fromPeer:`.
    fn client_player_information_recieved_from_peer(
        &self,
        client: Option<&PlistDict>,
        enet_or_player_id: &str,
    ) -> Option<PlayerInfo> {
        let client = client?;
        let key = |k: &str| k.to_string();
        let persistent_id = self.persistent_id_of(client)?;
        let stored_enet_id =
            self.persistent_ids_to_enet_ids.borrow().as_ref().and_then(|m| m.get(&persistent_id).cloned());
        if let Some(stored) = stored_enet_id {
            if stored != enet_or_player_id && crate::server::shared_names::enabled() {
                self.replace_session(&persistent_id, &stored);
            } else if stored != enet_or_player_id {
                let mut found_same_device = false;
                let snapshot: Vec<PlayerInfo> = self.player_infos.borrow().clone();
                for dict in snapshot {
                    if dict_player_id(&dict).as_deref() != Some(persistent_id.as_str()) {
                        continue;
                    }
                    let old_udid = dget(&dict.borrow(), "udidNew").cloned();
                    let Some(old_udid) = old_udid else { continue };
                    let same = client.get(&key("udidNew")).is_some_and(|c| c.is_equal(&old_udid));
                    if !same {
                        continue;
                    }
                    let old_peer = self.connections.borrow().as_ref().and_then(|c| c.get(&stored).copied());
                    if let Some(old_peer) = old_peer {
                        nslog("attempt to reconnect from same device");
                        for d in self.delegate() {
                            d.match_player_did_change_state(self, &persistent_id, PlayerConnectionState::Disconnected);
                        }
                        // `old_peer` came from `connections`, which only exists together with the host.
                        for c in self.connections.borrow_mut().as_mut() {
                            c.remove(&stored);
                        }
                        for h in self.host.borrow_mut().as_mut() {
                            h.peer_reset(old_peer);
                        }
                        found_same_device = true;
                    }
                }
                if !found_same_device {
                    return None;
                }
            }
        }
        // Map writes come before the info dict is built. Stale entries are left behind on purpose.
        if let Some(m) = self.persistent_ids_to_enet_ids.borrow_mut().as_mut() {
            m.insert(persistent_id.clone(), enet_or_player_id.to_string());
        }
        if let Some(m) = self.enet_ids_to_persistent_ids.borrow_mut().as_mut() {
            m.insert(enet_or_player_id.to_string(), persistent_id.clone());
        }
        Some(self.info_for_hello(client, &persistent_id, enet_or_player_id))
    }

    /// With the "same name from another device" setting on: the info a hello would get while another live
    /// connection holds its name (nothing is changed); none when the name is free or held by this connection.
    fn name_holder_elsewhere(&self, client: Option<&PlistDict>, enet_or_player_id: &str) -> Option<PlayerInfo> {
        let client = client?;
        let persistent_id = self.persistent_id_of(client)?;
        let stored = self.persistent_ids_to_enet_ids.borrow().as_ref().and_then(|m| m.get(&persistent_id).cloned())?;
        let live = self.connections.borrow().as_ref().is_some_and(|c| c.get(&stored).is_some());
        (stored != enet_or_player_id && live).then(|| self.info_for_hello(client, &persistent_id, enet_or_player_id))
    }

    /// `-clientApprovedWithInfo:`: stores only if no entry with that playerID; always calls `infoArrived`.
    fn client_approved_with_info(&self, info: &PlayerInfo) {
        let pid = dict_player_id(info);
        let found = self.player_infos.borrow().iter().any(|d| dict_player_id(d) == pid && pid.is_some());
        if !found {
            self.player_infos.borrow_mut().push(info.clone());
        }
        if let (Some(d), Some(pid)) = (self.delegate(), pid) {
            d.info_arrived_for_player(info, &pid);
        }
    }

    /// `-persistentIDForNetID:`: lookup, may be nil.
    fn persistent_id_for_net_id(&self, net_id: &str) -> Option<String> {
        self.enet_ids_to_persistent_ids.borrow().as_ref().and_then(|m| m.get(&net_id.to_string()).cloned())
    }

    /// Always YES.
    fn requires_player_authentication(&self) -> bool {
        true
    }
    /// `cloudSalt != nil`.
    fn is_cloud_match(&self) -> bool {
        self.cloud_salt.borrow().is_some()
    }
    /// `ownerName != nil`.
    fn owned(&self) -> bool {
        self.owner_name.borrow().is_some()
    }
    fn owner_name(&self) -> Option<String> {
        self.owner_name.borrow().clone()
    }
    fn cloud_salt(&self) -> Option<String> {
        self.cloud_salt.borrow().clone()
    }
    fn as_server_match(&self) -> Option<&BhNetServerMatch> {
        Some(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusty_enet as enet;
    use std::net::{SocketAddr, UdpSocket};
    use std::time::{Duration, Instant};

    struct Timers {
        created: Cell<u32>,
        invalid: Cell<u32>,
    }
    impl TimerScheduler for Timers {
        fn schedule_repeating(&self, interval: f64, _cb: Box<dyn FnMut()>) -> TimerId {
            assert_eq!(interval, 0.016666666666666666);
            self.created.set(self.created.get() + 1);
            7
        }
        fn invalidate(&self, id: TimerId) {
            assert_eq!(id, 7);
            self.invalid.set(self.invalid.get() + 1);
        }
    }

    #[derive(Default)]
    struct Del {
        log: RefCell<Vec<String>>,
        boots: Cell<u32>,
        /// Raise an exception on every state change.
        raise: Cell<bool>,
    }
    impl MatchDelegate for Del {
        fn name_taken_over(&self, player_id: &str) {
            self.log.borrow_mut().push(format!("taken {player_id}"));
        }
        fn match_player_did_change_state(&self, _m: &dyn BhMatch, p: &str, s: PlayerConnectionState) {
            if self.raise.get() {
                ns_raise("NSRangeException", "boom");
            }
            self.log.borrow_mut().push(format!("state {p} {}", s as i64));
        }
        fn match_did_receive_data_from_player(&self, _m: &dyn BhMatch, d: &[u8], p: &str) {
            self.log.borrow_mut().push(format!("data {p} {}", d.len()));
        }
        fn info_arrived_for_player(&self, _i: &PlayerInfo, p: &str) {
            self.log.borrow_mut().push(format!("info {p}"));
        }
        fn match_did_fail_with_error(&self, _m: &dyn BhMatch) {}
        fn boot_all_clients_due_to_no_credit(&self) {
            self.boots.set(self.boots.get() + 1);
        }
        fn match_connection_with_player_failed_with_error(&self, _m: &dyn BhMatch, _p: &str) {}
    }

    fn args(port: &str) -> ServerMatchArgs {
        ServerMatchArgs {
            port: Some(port.into()),
            user_name: Some("server".into()),
            has_local_player: false,
            world_name: None,
            credit: -1.0,
            cloud_salt: None,
            owner_name: None,
            privacy: None,
        }
    }

    fn make(a: ServerMatchArgs) -> (Rc<BhNetServerMatch>, Rc<Del>, Rc<Timers>) {
        let t = Rc::new(Timers { created: Cell::new(0), invalid: Cell::new(0) });
        let m = BhNetServerMatch::init_with_port_user_name_photo_has_local_player_world_name_credit_cloud_salt_owner_name_privacy(
            a,
            t.clone(),
        )
        .unwrap();
        let d = Rc::new(Del::default());
        let dd: Rc<dyn MatchDelegate> = d.clone();
        m.set_delegate(Some(Rc::downgrade(&dd)));
        (m, d, t)
    }

    fn client(channels: usize) -> enet::Host<UdpSocket> {
        enet::Host::new(
            UdpSocket::bind("127.0.0.1:0").unwrap(),
            enet::HostSettings { peer_limit: 1, channel_limit: channels, ..Default::default() },
        )
        .unwrap()
    }

    /// Pump server poll and client until `f` is true or 3 s elapse; returns client events received.
    fn pump(
        m: &BhNetServerMatch,
        cs: &mut [&mut enet::Host<UdpSocket>],
        mut f: impl FnMut() -> bool,
    ) -> Vec<(usize, u8, Vec<u8>)> {
        let mut got = Vec::new();
        let end = Instant::now() + Duration::from_secs(3);
        while Instant::now() < end && !f() {
            m.poll_net_events();
            for (i, c) in cs.iter_mut().enumerate() {
                while let Ok(Some(ev)) = c.service() {
                    if let enet::Event::Receive { channel_id, packet, .. } = ev {
                        got.push((i, channel_id, packet.data().to_vec()));
                    }
                }
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        got
    }

    #[test]
    fn privacy_strings() {
        let (m, _, _) = make(args("0"));
        for (s, v) in [
            (None, 1),
            (Some(""), 1),
            (Some("x"), 1),
            (Some("PUBLIC"), 1),
            (Some("Searchable"), 2),
            (Some("pRIvate"), 3),
        ] {
            m.set_privacy(9);
            m.set_privacy_by_string(s);
            assert_eq!(m.privacy(), v, "{s:?}");
        }
        m.set_privacy(7);
        assert_eq!(m.privacy(), 7);
    }

    #[test]
    fn accessors() {
        let mut a = args("0");
        a.owner_name = Some(String::new());
        a.credit = 0.1;
        let (m, _, _) = make(a);
        assert!(m.owned()); // init keeps "" verbatim
        m.set_owner(Some(""));
        assert!(!m.owned());
        m.set_owner(Some("Foo"));
        assert_eq!(m.owner_name().as_deref(), Some("Foo"));
        assert_eq!(m.credit(), 0.1f32);
        m.set_credit(-1.0);
        assert_eq!(m.credit(), -1.0);
        assert!(!m.is_cloud_match());
        m.set_cloud_salt(Some(""));
        assert!(m.is_cloud_match());
        assert_eq!(m.local_player_id(), "cf1e8c14e54505f60aa10ceb8d5d8ab3");
        assert_eq!(m.local_player_name(), "SERVER");
        assert!(m.requires_player_authentication());
        assert!(m.as_server_match().is_some());
    }

    #[test]
    fn port_and_bind() {
        let (m, _, t) = make(args("0"));
        assert_eq!(t.created.get(), 1);
        let port = m.local_port().unwrap();
        let t2 = Rc::new(Timers { created: Cell::new(0), invalid: Cell::new(0) });
        let r = BhNetServerMatch::init_with_port_user_name_photo_has_local_player_world_name_credit_cloud_salt_owner_name_privacy(
            args(&port.to_string()),
            t2.clone(),
        );
        assert!(r.is_err());
        assert_eq!(t2.created.get(), 0);
        m.disconnect();
        assert_eq!(t.invalid.get(), 1);
    }

    #[test]
    fn connect_receive_send() {
        let (m, d, _) = make(args("0"));
        let port = m.local_port().unwrap();
        let mut c = client(8);
        c.connect(SocketAddr::from(([127, 0, 0, 1], port)), 8, 0).unwrap();
        pump(&m, &mut [&mut c], || d.log.borrow().iter().any(|l| l.starts_with("state")));
        let id = d.log.borrow()[0].clone();
        assert!(id.starts_with("state 16777343:") && id.ends_with(" 1"), "{id}");
        let enet_id = id.split(' ').nth(1).unwrap().to_string();
        // client -> server (3 packets drained in one poll)
        let peer = c.connected_peers_mut().next().unwrap();
        for n in [1usize, 0, 5] {
            peer.send(0, &enet::Packet::reliable(vec![7u8; n])).unwrap();
        }
        c.flush();
        pump(&m, &mut [&mut c], || d.log.borrow().iter().filter(|l| l.starts_with("data")).count() == 3);
        let lens: Vec<_> = d.log.borrow().iter().filter(|l| l.starts_with("data")).cloned().collect();
        assert_eq!(lens, [format!("data {enet_id} 1"), format!("data {enet_id} 0"), format!("data {enet_id} 5")]);
        // server -> client by ENet id; single connection => channel 0; empty data => empty packet
        assert!(m.send_data_to_players_with_data_mode_error(
            Some(b"hello"),
            std::slice::from_ref(&enet_id),
            SendDataMode::Reliable
        ));
        assert!(m.send_data_to_players_with_data_mode_error(
            None,
            std::slice::from_ref(&enet_id),
            SendDataMode::Unreliable
        ));
        let got = pump(&m, &mut [&mut c], || false);
        let got: Vec<_> = got.into_iter().map(|(_, ch, d)| (ch, d)).collect();
        assert_eq!(got, [(0u8, b"hello".to_vec()), (0u8, vec![])]);
        // persistent mapping: handshake
        let mut cd = new_info_dict();
        cd.insert("alias".into(), PlistValue::string("Alice"));
        cd.insert("udidNew".into(), PlistValue::string("U1"));
        let info = m.client_player_information_recieved_from_peer(Some(&cd), &enet_id).unwrap();
        {
            let i = info.borrow();
            let keys: Vec<_> = i.keys_in_order();
            assert_eq!(dget(&i, "playerID").and_then(|v| v.as_str()), Some(string_from_md5("alice").as_str()));
            assert_eq!(dget(&i, "ip").and_then(|v| v.as_str()), Some("127.0.0.1"));
            for k in ["playerID", "alias", "local", "micOrSpeakerOn", "voiceConnected", "connected", "ip", "udidNew"] {
                assert!(keys.contains(&k.to_string()), "{k}");
            }
        }
        let pid = string_from_md5("alice");
        assert_eq!(m.persistent_id_for_net_id(&enet_id).as_deref(), Some(pid.as_str()));
        assert_eq!(m.player_ids().unwrap(), vec![pid.clone()]);
        m.client_approved_with_info(&info);
        assert!(Rc::ptr_eq(&m.load_info_for_player(&pid).unwrap(), &info));
        assert_eq!(d.log.borrow().last().unwrap(), &format!("info {pid}"));
        // send by persistent id now reaches the peer
        m.send_data_to_players_with_data_mode_error(Some(b"x"), std::slice::from_ref(&pid), SendDataMode::Reliable);
        let got = pump(&m, &mut [&mut c], || false);
        assert_eq!(got.len(), 1);
        // alias "SERVER" rejected; missing alias rejected; duplicate alias from other connection rejected
        let mut sd = new_info_dict();
        sd.insert("alias".into(), PlistValue::string("SeRvEr"));
        assert!(m.client_player_information_recieved_from_peer(Some(&sd), "9:9").is_none());
        assert!(m.client_player_information_recieved_from_peer(Some(&new_info_dict()), "9:9").is_none());
        assert!(m.client_player_information_recieved_from_peer(None, "9:9").is_none());
        let mut dup = new_info_dict();
        dup.insert("alias".into(), PlistValue::string("ALICE"));
        dup.insert("udidNew".into(), PlistValue::string("U2"));
        assert!(m.client_player_information_recieved_from_peer(Some(&dup), "9:9").is_none());
        // same device, old peer alive: takeover. Old client gets reset (no disconnect event).
        let log_before = d.log.borrow().len();
        dup.insert("udidNew".into(), PlistValue::string("U1"));
        let again = m.client_player_information_recieved_from_peer(Some(&dup), "9:9");
        assert!(again.is_some());
        assert_eq!(d.log.borrow()[log_before], format!("state {pid} 2"));
        assert_eq!(m.persistent_ids_to_enet_ids.borrow().as_ref().unwrap().get(&pid).unwrap(), "9:9");
        // stale reverse entry remains
        assert_eq!(m.persistent_id_for_net_id(&enet_id).as_deref(), Some(pid.as_str()));
        assert_eq!(m.connections.borrow().as_ref().unwrap().len(), 0);
    }

    #[test]
    fn channel_numbers() {
        let (m, d, _) = make(args("0"));
        let port = m.local_port().unwrap();
        let mut cs: Vec<_> = (0..3).map(|_| client(8)).collect();
        for c in cs.iter_mut() {
            c.connect(SocketAddr::from(([127, 0, 0, 1], port)), 8, 0).unwrap();
        }
        {
            let mut refs: Vec<&mut enet::Host<UdpSocket>> = cs.iter_mut().collect();
            pump(&m, &mut refs, || d.log.borrow().len() >= 3);
        }
        let order = m.connections.borrow().as_ref().unwrap().keys_in_order();
        assert_eq!(order.len(), 3);
        m.send_data_to_players_with_data_mode_error(Some(b"b"), &order, SendDataMode::Reliable);
        let mut refs: Vec<&mut enet::Host<UdpSocket>> = cs.iter_mut().collect();
        let got = pump(&m, &mut refs, || false);
        assert_eq!(got.len(), 3);
        // each client got exactly one packet on channel == index of its id in `order`.
        let mut chans: Vec<u8> = got.iter().map(|g| g.1).collect();
        chans.sort();
        assert_eq!(chans, [0, 1, 2]);
    }

    #[test]
    fn data_goes_only_to_the_players_asked_for() {
        let (m, d, _) = make(args("0"));
        let port = m.local_port().unwrap();
        let mut cs: Vec<_> = (0..2).map(|_| client(8)).collect();
        for c in cs.iter_mut() {
            c.connect(SocketAddr::from(([127, 0, 0, 1], port)), 8, 0).unwrap();
        }
        {
            let mut refs: Vec<&mut enet::Host<UdpSocket>> = cs.iter_mut().collect();
            pump(&m, &mut refs, || d.log.borrow().len() >= 2);
        }
        let order = m.connections.borrow().as_ref().unwrap().keys_in_order();
        m.send_data_to_players_with_data_mode_error(Some(b"x"), &order[..1], SendDataMode::Reliable);
        let mut refs: Vec<&mut enet::Host<UdpSocket>> = cs.iter_mut().collect();
        assert_eq!(pump(&m, &mut refs, || false).len(), 1);
    }

    #[test]
    fn disconnect_flow() {
        let (m, d, _) = make(args("0"));
        let port = m.local_port().unwrap();
        let mut c = client(8);
        c.connect(SocketAddr::from(([127, 0, 0, 1], port)), 8, 0).unwrap();
        pump(&m, &mut [&mut c], || d.log.borrow().len() == 1);
        let enet_id = d.log.borrow()[0].split(' ').nth(1).unwrap().to_string();
        let mut cd = new_info_dict();
        cd.insert("alias".into(), PlistValue::string("Bob"));
        let info = m.client_player_information_recieved_from_peer(Some(&cd), &enet_id).unwrap();
        m.client_approved_with_info(&info);
        m.disconnect_player(&enet_id); // unauthenticated id: nothing
        let pid = string_from_md5("bob");
        m.disconnect_player(&pid); // graceful disconnect request
        pump(&m, &mut [&mut c], || d.log.borrow().iter().any(|l| l.ends_with(" 2")));
        assert!(d.log.borrow().contains(&format!("state {pid} 2")));
        assert!(m.load_info_for_player(&pid).is_none()); // removeClient on DISCONNECT
        assert!(m.player_ids().unwrap().is_empty());
        assert_eq!(m.connections.borrow().as_ref().unwrap().len(), 0);
    }

    #[test]
    fn shutdown_with_client() {
        let (m, d, t) = make(args("0"));
        let port = m.local_port().unwrap();
        let mut c = client(8);
        c.connect(SocketAddr::from(([127, 0, 0, 1], port)), 8, 0).unwrap();
        pump(&m, &mut [&mut c], || d.log.borrow().len() == 1);
        let (tx, rx) = std::sync::mpsc::channel::<()>();
        let th = std::thread::spawn(move || {
            let end = Instant::now() + Duration::from_secs(5);
            let mut disconnected = false;
            while Instant::now() < end && !disconnected {
                while let Ok(Some(ev)) = c.service() {
                    if matches!(ev, enet::Event::Disconnect { .. }) {
                        disconnected = true;
                    }
                }
                std::thread::sleep(Duration::from_millis(1));
            }
            let _ = tx.send(());
            disconnected
        });
        m.disconnect();
        assert_eq!(t.invalid.get(), 1);
        rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(th.join().unwrap());
        assert!(m.connections.borrow().is_none());
        // after disconnect, sends are nil messages
        assert!(m.send_data_to_players_with_data_mode_error(Some(b"x"), &["a".into()], SendDataMode::Reliable));
        assert!(m.player_ids().is_none());
    }

    #[test]
    fn credit_boot() {
        use crate::foundation::clock;
        let mut a = args("0");
        a.cloud_salt = Some("salt".into());
        a.credit = 1.0;
        let _cl = clock::TEST_CLOCK_LOCK.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        clock::set_virtual(Some(1_000_000.0));
        let (m, d, _) = make(a);
        m.poll_net_events();
        assert_eq!(d.boots.get(), 0);
        clock::advance_virtual(0.5);
        m.poll_net_events();
        assert_eq!(d.boots.get(), 0);
        clock::advance_virtual(0.6);
        m.poll_net_events(); // credit <= 0.01 and timer 0 -> boot
        assert_eq!(d.boots.get(), 1);
        clock::advance_virtual(5.0);
        m.poll_net_events();
        assert_eq!(d.boots.get(), 1);
        clock::advance_virtual(5.0);
        m.poll_net_events();
        assert_eq!(d.boots.get(), 2);
        let (m2, d2, _) = make(args("0"));
        clock::advance_virtual(100.0);
        m2.poll_net_events();
        assert_eq!(d2.boots.get(), 0);
        assert_eq!(m2.credit(), -1.0);
        clock::init_from_env();
    }

    #[test]
    fn non_string_alias_raises() {
        let (m, _, _) = make(args("0"));
        let mut cd = new_info_dict();
        cd.insert("alias".into(), PlistValue::int(5));
        let r = catch_ns(|| m.client_player_information_recieved_from_peer(Some(&cd), "1:1"));
        let e = r.unwrap_err();
        assert_eq!(e.name, "NSInvalidArgumentException");
        assert!(e.reason.unwrap().ends_with("does not recognize lowercaseString"));
    }

    #[test]
    fn local_player_info_at_init() {
        let mut a = args("0");
        a.has_local_player = true;
        let (m, _, _) = make(a);
        let i = m.load_info_for_player(&m.local_player_id()).unwrap();
        let mut keys = i.borrow().keys_in_order();
        keys.sort();
        assert_eq!(keys, ["alias", "local", "micOrSpeakerOn", "playerID", "voiceConnected"]);
        assert_eq!(dget(&i.borrow(), "alias").and_then(|v| v.as_str()), Some("server"));
        assert!(m.player_ids().unwrap().is_empty());
    }

    #[test]
    fn empty_alias_valid() {
        let (m, _, _) = make(args("0"));
        let mut cd = new_info_dict();
        cd.insert("alias".into(), PlistValue::string(""));
        let i = m.client_player_information_recieved_from_peer(Some(&cd), "1:1").unwrap();
        assert_eq!(dget(&i.borrow(), "playerID").and_then(|v| v.as_str()), Some("d41d8cd98f00b204e9800998ecf8427e"));
    }

    #[test]
    fn defaults_restarts_and_a_taken_port() {
        // No user name: the host name; no port: the default one (which may be taken here: either way the lines run).
        let mut a = args("0");
        a.user_name = None;
        let (m, _, _) = make(a);
        assert!(!m.local_player_name().is_empty());
        let mut b = args("0");
        b.port = None;
        let t = Rc::new(Timers { created: Cell::new(0), invalid: Cell::new(0) });
        let _ = BhNetServerMatch::init_with_port_user_name_photo_has_local_player_world_name_credit_cloud_salt_owner_name_privacy(
            b, t,
        );
        // A running server cannot be started again; a taken port tells the delegate.
        assert!(!m.start_server());
        let free = UdpSocket::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
        let (m2, d2, _) = make(args(&free.to_string()));
        m2.disconnect();
        let blocker = UdpSocket::bind(("0.0.0.0", free)).unwrap();
        assert!(!m2.start_server());
        drop(blocker);
        assert!(d2.log.borrow().is_empty());
        let _ = (Del::default().boots.get(),);
    }

    /// Timers that keep the poll callback so the test can run it.
    struct Kept(RefCell<Option<Box<dyn FnMut()>>>);
    impl TimerScheduler for Kept {
        fn schedule_repeating(&self, _interval: f64, cb: Box<dyn FnMut()>) -> TimerId {
            *self.0.borrow_mut() = Some(cb);
            1
        }
        fn invalidate(&self, _id: TimerId) {}
    }

    #[test]
    fn the_poll_timer_polls_while_the_match_lives() {
        let kept = Rc::new(Kept(RefCell::new(None)));
        let m = BhNetServerMatch::init_with_port_user_name_photo_has_local_player_world_name_credit_cloud_salt_owner_name_privacy(
            args("0"),
            kept.clone(),
        )
        .unwrap();
        let mut cb = kept.0.borrow_mut().take().unwrap();
        cb();
        m.disconnect();
        drop(m);
        cb(); // the match is gone: nothing to poll
    }

    #[test]
    fn poll_clamps_a_clock_that_went_backwards_and_survives_a_missing_host() {
        let (m, _, _) = make(args("0"));
        m.poll_timer_last_time.set(reference_time() + 1000.0);
        m.poll_net_events();
        m.disconnect();
        m.poll_net_events(); // no host any more
    }

    #[test]
    fn unauthenticated_peers_disconnect_under_their_transport_name() {
        let (m, d, _) = make(args("0"));
        let port = m.local_port().unwrap();
        let mut c = client(8);
        c.connect(SocketAddr::from(([127, 0, 0, 1], port)), 8, 0).unwrap();
        pump(&m, &mut [&mut c], || d.log.borrow().len() == 1);
        let enet_id = d.log.borrow()[0].split(' ').nth(1).unwrap().to_string();
        c.connected_peers_mut().next().unwrap().disconnect(0);
        c.flush();
        pump(&m, &mut [&mut c], || d.log.borrow().iter().any(|l| l.ends_with(" 2")));
        assert!(d.log.borrow().contains(&format!("state {enet_id} 2")));
    }

    #[test]
    fn shutdown_gives_up_on_a_client_that_never_answers() {
        let (m, d, _) = make(args("0"));
        let port = m.local_port().unwrap();
        let mut c = client(8);
        c.connect(SocketAddr::from(([127, 0, 0, 1], port)), 8, 0).unwrap();
        pump(&m, &mut [&mut c], || d.log.borrow().len() == 1);
        // The client is not serviced during the shutdown, so the wait times out and the connection is reset.
        m.disconnect();
        assert!(m.connections.borrow().is_none());
    }

    #[test]
    fn sending_from_another_thread_is_refused() {
        let (m, _, _) = make(args("0"));
        let other = std::thread::spawn(|| std::thread::current().id()).join().unwrap();
        m.main_thread.set(other);
        let e =
            catch_ns(|| m.send_data_to_players_with_data_mode_error(Some(b"x"), &["a".into()], SendDataMode::Reliable))
                .unwrap_err();
        assert_eq!(e.name, "Background thread network send");
    }

    #[test]
    fn reconnect_attempts_that_do_not_prove_the_same_device() {
        let (m, _, _) = make(args("0"));
        let pid = string_from_md5("carl");
        let join = |udid: Option<&str>, from: &str| {
            let mut cd = new_info_dict();
            cd.insert("alias".into(), PlistValue::string("Carl"));
            cd.insert("photo".into(), PlistValue::data(vec![1, 2]));
            cd.insert("cloudKey".into(), PlistValue::string("key"));
            if let Some(u) = udid {
                cd.insert("udidNew".into(), PlistValue::string(u));
            }
            m.client_player_information_recieved_from_peer(Some(&cd), from)
        };
        // Another approved player in the list is skipped while looking for the same device.
        let other = {
            let mut cd = new_info_dict();
            cd.insert("alias".into(), PlistValue::string("Dora"));
            m.client_player_information_recieved_from_peer(Some(&cd), "0:0").unwrap()
        };
        m.client_approved_with_info(&other);
        let first = join(Some("U1"), "1:1").unwrap();
        assert!(join(Some("U1"), "1:1").is_some()); // the same connection saying hello twice
        assert!(dget(&first.borrow(), "photo").is_some() && dget(&first.borrow(), "cloudKey").is_some());
        m.client_approved_with_info(&first);
        m.client_approved_with_info(&first); // already known: not added twice
        assert_eq!(m.player_ids().unwrap().len(), 2);
        // The old connection is gone (no peer), so even the same device cannot take over.
        assert!(join(Some("U1"), "2:2").is_none());
        // A stored record without a device id never matches.
        first.borrow_mut().map.remove(&"udidNew".to_string());
        assert!(join(Some("U1"), "3:3").is_none());
        let _ = pid;
    }

    #[test]
    fn the_same_name_from_another_device_replaces_the_session_that_holds_it() {
        use crate::server::shared_names::set_enabled;
        let (m, d, _) = make(args("0"));
        let port = m.local_port().unwrap();
        let hello = |alias: Option<&str>, udid: &str| {
            let mut cd = new_info_dict();
            if let Some(a) = alias {
                cd.insert("alias".into(), PlistValue::string(a));
            }
            cd.insert("udidNew".into(), PlistValue::string(udid));
            cd
        };
        let mut c1 = client(8);
        let mut c2 = client(8);
        for c in [&mut c1, &mut c2] {
            c.connect(SocketAddr::from(([127, 0, 0, 1], port)), 8, 0).unwrap();
        }
        pump(&m, &mut [&mut c1, &mut c2], || d.log.borrow().len() == 2);
        let ids: Vec<String> = d.log.borrow().iter().map(|l| l.split(' ').nth(1).unwrap().to_string()).collect();
        let (e1, e2) = (ids[0].clone(), ids[1].clone());
        // Nothing to replace: a free name, no hello, no alias, the connection that holds it itself.
        assert!(m.name_holder_elsewhere(Some(&hello(Some("Zed"), "u1")), &e1).is_none());
        assert!(m.name_holder_elsewhere(None, &e1).is_none());
        assert!(m.name_holder_elsewhere(Some(&hello(None, "u1")), &e1).is_none());
        let first = m.client_player_information_recieved_from_peer(Some(&hello(Some("Zed"), "u1")), &e1).unwrap();
        m.client_approved_with_info(&first);
        assert!(m.name_holder_elsewhere(Some(&hello(Some("zed"), "u1")), &e1).is_none());
        // Vanilla: another device cannot have the name while it is online.
        assert!(m.name_holder_elsewhere(Some(&hello(Some("zed"), "u2")), &e2).is_some());
        assert!(m.client_player_information_recieved_from_peer(Some(&hello(Some("zed"), "u2")), &e2).is_none());
        assert!(m.player_ids().unwrap().contains(&string_from_md5("zed")));
        // With the setting the holder is told, released like a disconnect, and the new connection has the name.
        set_enabled(true);
        d.log.borrow_mut().clear();
        let second = m.client_player_information_recieved_from_peer(Some(&hello(Some("zed"), "u2")), &e2).unwrap();
        set_enabled(false);
        let pid = string_from_md5("zed");
        assert_eq!(*d.log.borrow(), [format!("taken {pid}"), format!("state {pid} 2")]);
        assert_eq!(dget(&second.borrow(), "udidNew").and_then(|v| v.as_str()), Some("u2"));
        assert_eq!(m.persistent_id_for_net_id(&e2).as_deref(), Some(pid.as_str()));
        assert_eq!(m.persistent_id_for_net_id(&e1), None, "the replaced connection no longer maps to the player");
        assert!(m.load_info_for_player(&pid).is_none(), "the old record is gone, the approval adds the new one");
        m.client_approved_with_info(&second);
        assert!(Rc::ptr_eq(&m.load_info_for_player(&pid).unwrap(), &second));
        // The replaced connection is closed once its message left; its own disconnect does not touch the player.
        pump(&m, &mut [&mut c1, &mut c2], || d.log.borrow().iter().any(|l| l == &format!("state {e1} 2")));
        assert!(d.log.borrow().contains(&format!("state {e1} 2")));
        assert_eq!(m.persistent_id_for_net_id(&e2).as_deref(), Some(pid.as_str()));
        // A stale entry (no connection behind it) is just overwritten.
        set_enabled(true);
        m.disconnect_player(&pid);
        m.persistent_ids_to_enet_ids.borrow_mut().as_mut().unwrap().insert(pid.clone(), "9:9".into());
        assert!(m.name_holder_elsewhere(Some(&hello(Some("zed"), "u3")), &e1).is_none());
        assert!(m.client_player_information_recieved_from_peer(Some(&hello(Some("zed"), "u3")), &e1).is_some());
        set_enabled(false);
    }

    #[test]
    fn exceptions_in_the_poll_end_the_process() {
        if let Ok(port) = std::env::var("BH_NET_CHILD_PORT") {
            let t = Rc::new(Timers { created: Cell::new(0), invalid: Cell::new(0) });
            let m = BhNetServerMatch::init_with_port_user_name_photo_has_local_player_world_name_credit_cloud_salt_owner_name_privacy(
                args(&port),
                t,
            )
            .unwrap();
            let raising = Rc::new(Del::default());
            raising.raise.set(true);
            let d: Rc<dyn MatchDelegate> = raising;
            m.set_delegate(Some(Rc::downgrade(&d)));
            let mut c = client(8);
            c.connect(SocketAddr::from(([127, 0, 0, 1], m.local_port().unwrap())), 8, 0).unwrap();
            pump(&m, &mut [&mut c], || false);
            std::process::exit(3);
        }
        let free = UdpSocket::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
        let out = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "net::server_match::tests::exceptions_in_the_poll_end_the_process",
                "--nocapture",
                "--test-threads=1",
            ])
            .env("BH_NET_CHILD_PORT", free.to_string())
            .output()
            .unwrap();
        assert_eq!(out.status.code(), Some(0));
        assert!(String::from_utf8_lossy(&out.stderr)
            .contains("Exiting due to exception in pollNetEvents 'NSRangeException'"));
    }

    #[test]
    fn the_test_delegate_stubs_answer() {
        let d = Del::default();
        let (m, _, _) = make(args("0"));
        d.match_did_fail_with_error(&*m);
        d.match_connection_with_player_failed_with_error(&*m, "p");
        // The accessors a cloud match answers.
        m.set_cloud_salt(Some("salt"));
        assert_eq!(m.cloud_salt().as_deref(), Some("salt"));
        // The mod host is polled after every network poll.
        let hooks = crate::server::bhserver::test_support::StubHooks::install();
        m.poll_net_events();
        assert!(hooks.saw("poll"));
        crate::server::bhserver::test_support::StubHooks::uninstall();
    }
}
