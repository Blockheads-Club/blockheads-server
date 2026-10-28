//! `BHMatch` (abstract transport interface) and `BHNetNode` (base of `BHServer`) plus the shared enums and traits.
//!
//! `# Class BHNetNode` and its "Synthesis resolutions".

use crate::foundation::gsdict::GsDict;
use crate::foundation::nsstring;
use crate::net::server_match::BhNetServerMatch;
use crate::plist::helpers::property_list_from_data_with_options;
use crate::plist::value::{ClassTag, Origin, PlistDict, PlistValue};
use std::cell::RefCell;
use std::rc::{Rc, Weak};

/// Shared `NSMutableDictionary` player-info (same object in the match and the node).
pub type PlayerInfo = Rc<RefCell<PlistDict>>;

/// New empty `NSMutableDictionary`.
pub fn new_info_dict() -> PlistDict {
    PlistDict { map: GsDict::new(), tag: ClassTag { origin: Origin::Built, mutable: true, alt: false } }
}

/// `objectForKey:` on a player-info dict.
pub(crate) fn dget<'a>(d: &'a PlistDict, key: &str) -> Option<&'a PlistValue> {
    d.map.get(&key.to_string())
}

/// `[d[@"playerID"]...]` as a string (non-strings behave as nil for `isEqualToString:`).
pub(crate) fn dict_player_id(d: &PlayerInfo) -> Option<String> {
    dget(&d.borrow(), "playerID").and_then(|v| v.as_str()).map(|s| s.to_string())
}

/// Body of `-[BHMatch loadInfoForPlayer:]`: `[NSMutableDictionary dictionaryWithObject:playerID forKey:@"playerID"]`.
/// Free function so a subclass can make the `[super loadInfoForPlayer:]` call that bypasses its own override
pub fn bhmatch_load_info_for_player(player_id: &str) -> PlayerInfo {
    let mut d = new_info_dict();
    d.insert("playerID".to_string(), PlistValue::string(player_id));
    Rc::new(RefCell::new(d))
}

/// `BHPlayerConnectionState` (NSInteger).
#[repr(i64)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PlayerConnectionState {
    Unknown = 0,
    Connected = 1,
    Disconnected = 2,
}

/// `BHNetPrivacy`; `setPrivacy:` stores any u64 so the struct keeps a `u64`.
#[repr(u64)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum NetPrivacy {
    Undefined = 0,
    Public = 1,
    Searchable = 2,
    Private = 3,
}

/// `BHNetNodeType`.
#[repr(i64)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum NetNodeType {
    LocalServer = 0,
    LocalClient = 1,
    OnlineServer = 2,
    OnlineClient = 3,
    GameCenterServer = 4,
    GameCenterClient = 5,
}

/// `BHMatchSendDataMode`; ignored by the server match.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SendDataMode {
    Reliable = 0,
    Unreliable = 1,
}

/// Selectors the match sends to its delegate (the BHServer).
pub trait MatchDelegate {
    /// `-match:player:didChangeState:`.
    fn match_player_did_change_state(&self, m: &dyn BhMatch, player_id: &str, state: PlayerConnectionState);
    /// `-match:didReceiveData:fromPlayer:`.
    fn match_did_receive_data_from_player(&self, m: &dyn BhMatch, data: &[u8], player_id: &str);
    /// `-infoArrived:forPlayer:`.
    fn info_arrived_for_player(&self, info: &PlayerInfo, player_id: &str);
    /// `-match:didFailWithError:` (error always nil).
    fn match_did_fail_with_error(&self, m: &dyn BhMatch);
    /// `-bootAllClientsDueToNoCredit`.
    fn boot_all_clients_due_to_no_credit(&self);
    /// `-match:connectionWithPlayer:failedWithError:`; never sent by the server match.
    fn match_connection_with_player_failed_with_error(&self, m: &dyn BhMatch, player_id: &str);
    /// Not in the original: the session of `player_id` is replaced by a newer login under the same name (the
    /// "same name from another device" setting); it still has its connection, tell it why it goes.
    fn name_taken_over(&self, _player_id: &str) {}
}

/// Storage of the `BHMatch` ivars (`delegate`, `host`, `port`). A trait cannot hold fields, so implementors embed
/// this.
#[derive(Default)]
pub struct BhMatchBase {
    /// `delegate` (assign, not retained).
    pub delegate: RefCell<Option<Weak<dyn MatchDelegate>>>,
    /// `host` ivar; never written on the server.
    pub host: RefCell<Option<String>>,
    /// `port` ivar; never written on the server.
    pub port: RefCell<Option<String>>,
}

/// The abstract class `BHMatch`. Defaults are the `BHMatch` bodies of; `BhNetServerMatch` overrides the marked
/// ones.
pub trait BhMatch {
    /// `-setDelegate:` (plain assign). Required because the delegate ivar lives in the implementor
    /// (`BhMatchBase`).
    fn set_delegate(&self, d: Option<Weak<dyn MatchDelegate>>);
    /// `-localPlayerID`; default `"local_player"`.
    fn local_player_id(&self) -> String {
        "local_player".to_string()
    }
    /// `-localPlayerName`; default `"local_player"`.
    fn local_player_name(&self) -> String {
        "local_player".to_string()
    }
    /// `-playerIDs`; default nil.
    fn player_ids(&self) -> Option<Vec<String>> {
        None
    }
    /// `-disconnect`; default no-op.
    fn disconnect(&self) {}
    /// `-voiceChatWithName:`; default nil.
    fn voice_chat_with_name(&self, _n: &str) -> Option<()> {
        None
    }
    /// `-expectedPlayerCount`; default 0.
    fn expected_player_count(&self) -> u64 {
        0
    }
    /// `-sendData:toPlayers:withDataMode:error:`; default NO.
    fn send_data_to_players_with_data_mode_error(
        &self,
        _data: Option<&[u8]>,
        _players: &[String],
        _mode: SendDataMode,
    ) -> bool {
        false
    }
    /// Not in the original: one unreliable, unsequenced packet to one player (the dispatcher's name: ENet id
    /// before approval, player id after it) on ENet channel 0. True when it was queued. Default NO.
    fn send_unsequenced(&self, _player: &str, _data: &[u8]) -> bool {
        false
    }
    /// `-loadInfoForPlayer:`; default `{playerID: id}` (fresh mutable dict).
    fn load_info_for_player(&self, player_id: &str) -> Option<PlayerInfo> {
        Some(bhmatch_load_info_for_player(player_id))
    }
    /// `-clientPlayerInformationRecieved:fromPeer:`; default nil.
    fn client_player_information_recieved_from_peer(
        &self,
        _client: Option<&PlistDict>,
        _enet_or_player_id: &str,
    ) -> Option<PlayerInfo> {
        None
    }
    /// Not in the original: with the "same name from another device" setting on, the info the new connection
    /// `enet_or_player_id` would get while another connection holds the same name, without changing anything (so
    /// the join can be checked before that connection is replaced). Default none.
    fn name_holder_elsewhere(&self, _client: Option<&PlistDict>, _enet_or_player_id: &str) -> Option<PlayerInfo> {
        None
    }
    /// `-clientApprovedWithInfo:`; default no-op.
    fn client_approved_with_info(&self, _info: &PlayerInfo) {}
    /// `-sendPlayerInformationToServer:password:`; default no-op (client only).
    fn send_player_information_to_server_password(&self, _info: Option<&PlayerInfo>, _password: Option<&str>) {}
    /// `-updatedPlayerListRecievedFromServer:`: parse with option 2 (mutable containers and leaves) then
    /// `[delegate updatePlayerList:list]` (the delegate is the BHNetNode, passed in). Client path, effectively dead.
    fn updated_player_list_recieved_from_server(&self, data: &[u8], node: &BhNetNode) {
        let parsed = property_list_from_data_with_options(data, 2);
        // Note: a nil list (parse failure) reaches `[livePlayerInfos addObjectsFromArray:nil]`; treated here
        // as an empty list. Non-dictionary elements are skipped.
        let list: Vec<PlayerInfo> = match parsed {
            Some(PlistValue::Array(a)) => a
                .items
                .into_iter()
                .filter_map(|v| match v {
                    PlistValue::Dict(d) => Some(Rc::new(RefCell::new(d))),
                    _ => None,
                })
                .collect(),
            _ => Vec::new(),
        };
        node.update_player_list(&list);
    }
    /// `-persistentIDForNetID:`; default returns its argument.
    fn persistent_id_for_net_id(&self, net_id: &str) -> Option<String> {
        Some(net_id.to_string())
    }
    /// `-audioChatSupported`; default NO.
    fn audio_chat_supported(&self) -> bool {
        false
    }
    /// `-requiresPlayerAuthentication`; default NO.
    fn requires_player_authentication(&self) -> bool {
        false
    }
    /// `-usesGameCenterPlayerInformation`; default NO.
    fn uses_game_center_player_information(&self) -> bool {
        false
    }
    /// `-connectedPlayerIDs`; default nil.
    fn connected_player_ids(&self) -> Option<Vec<String>> {
        None
    }
    /// `-removeClient:`; default no-op.
    fn remove_client(&self, _id: &str) {}
    /// `-disconnectPlayer:`; default no-op.
    fn disconnect_player(&self, _id: &str) {}
    /// `-isCloudMatch`; default NO.
    fn is_cloud_match(&self) -> bool {
        false
    }
    /// `-owned`; default NO.
    fn owned(&self) -> bool {
        false
    }
    /// `-ownerName`; default nil.
    fn owner_name(&self) -> Option<String> {
        None
    }
    /// `-cloudSalt`; default nil.
    fn cloud_salt(&self) -> Option<String> {
        None
    }
    /// `-host` ivar.
    fn host(&self) -> Option<String> {
        None
    }
    /// `-port` ivar.
    fn port(&self) -> Option<String> {
        None
    }
    /// `isKindOfClass:[BHNetServerMatch class]`.
    fn as_server_match(&self) -> Option<&BhNetServerMatch> {
        None
    }
}

/// Delegate of the `BHNetNode` (the GameController). `respondsToSelector:` is always YES.
pub trait NodeDelegate {
    /// `-playersChanged`.
    fn players_changed(&self);
}

/// Calls from inside `BhNetNode` that dispatch to a BHServer override.
pub trait NodeVirtuals {
    /// `BHServer.cleanup`.
    fn cleanup(&self);
    /// `-sendChatMessage:displayNotification:sendToClients:`.
    fn send_chat_message_display_notification_send_to_clients(
        &self,
        text: &str,
        display: bool,
        clients: Option<&[String]>,
    );
}

/// Timer id handed out by a [`TimerScheduler`].
pub type TimerId = u64;

/// Run-loop timer facility, implemented by app runtime.
pub trait TimerScheduler {
    /// `+[NSTimer scheduledTimerWithTimeInterval:target:selector:userInfo:repeats:YES]`: first fire at
    /// creation_time + interval; repeating; ties go to the timer created first. Invoked on the
    /// main thread.
    fn schedule_repeating(&self, interval_secs: f64, callback: Box<dyn FnMut()>) -> TimerId;
    /// `-[NSTimer invalidate]`; idempotent.
    fn invalidate(&self, id: TimerId);
}

/// Class `BHNetNode`: base-class state of `BHServer`. After `cleanup` all accessors behave as on nil.
pub struct BhNetNode {
    delegate: RefCell<Option<Weak<dyn NodeDelegate>>>,
    match_: RefCell<Option<Rc<dyn BhMatch>>>,
    live_player_infos: RefCell<Option<Vec<PlayerInfo>>>,
    player_infos_for_all_players_by_id: RefCell<Option<GsDict<String, PlayerInfo>>>,
    net_node_type: NetNodeType,
}

impl BhNetNode {
    /// `-initWithDelegate:match:netNodeType:`. `voiceChat` is omitted (always nil); no voice-chat timer because
    /// `audioChatSupported` is NO on the server.
    pub fn init_with_delegate_match_net_node_type(
        delegate: Option<Weak<dyn NodeDelegate>>,
        m: Rc<dyn BhMatch>,
        t: NetNodeType,
    ) -> BhNetNode {
        let node = BhNetNode {
            delegate: RefCell::new(delegate),
            match_: RefCell::new(Some(m.clone())),
            live_player_infos: RefCell::new(Some(Vec::new())),
            player_infos_for_all_players_by_id: RefCell::new(Some(GsDict::new())),
            net_node_type: t,
        };
        let local_id = m.local_player_id();
        node.load_info_for_player(&local_id);
        if let Some(ids) = m.player_ids() {
            for pid in ids {
                node.load_info_for_player(&pid);
            }
        }
        node
    }

    /// `-updateMatch:`; `node_as_delegate` is the subclass pointer for `[match setDelegate:self]`.
    pub fn update_match(&self, m: Rc<dyn BhMatch>, node_as_delegate: Weak<dyn MatchDelegate>) {
        let current = self.match_.borrow().clone();
        let same = match &current {
            Some(c) => std::ptr::eq(Rc::as_ptr(c) as *const (), Rc::as_ptr(&m) as *const ()),
            None => false,
        };
        if !same {
            if let Some(old) = current {
                old.set_delegate(None);
                old.disconnect();
            }
            *self.match_.borrow_mut() = None;
            if let Some(l) = self.live_player_infos.borrow_mut().as_mut() {
                l.clear();
            }
            *self.match_.borrow_mut() = Some(m.clone());
        }
        if let Some(ids) = m.player_ids() {
            for pid in ids {
                self.load_info_for_player(&pid);
            }
        }
        m.set_delegate(Some(node_as_delegate));
    }

    fn delegate_players_changed(&self) {
        let d = self.delegate.borrow().as_ref().and_then(|w| w.upgrade());
        if let Some(d) = d {
            d.players_changed();
        }
    }

    fn add_live(&self, dict: &PlayerInfo) {
        let local = dget(&dict.borrow(), "local").is_some_and(|v| v.bool_value());
        if let Some(l) = self.live_player_infos.borrow_mut().as_mut() {
            if local {
                l.insert(0, dict.clone());
            } else {
                l.push(dict.clone());
            }
        }
    }

    fn live_contains(&self, player_id: &str) -> bool {
        let live = self.live_player_infos.borrow().clone();
        let mut found = false;
        if let Some(l) = live {
            for d in &l {
                if dict_player_id(d).as_deref() == Some(player_id) {
                    found = true; // no early exit
                }
            }
        }
        found
    }

    /// `-loadInfoForPlayer:`: returns `alreadyLoaded`.
    pub fn load_info_for_player(&self, player_id: &str) -> bool {
        let mut already_loaded = true;
        let existing = self
            .player_infos_for_all_players_by_id
            .borrow()
            .as_ref()
            .and_then(|m| m.get(&player_id.to_string()).cloned());
        let dict = match existing {
            Some(d) => Some(d),
            None => {
                already_loaded = false;
                let m = self.match_.borrow().clone();
                m.and_then(|m| m.load_info_for_player(player_id))
            }
        };
        if let Some(dict) = dict {
            if !self.live_contains(player_id) {
                self.add_live(&dict);
                self.delegate_players_changed();
            }
            if let Some(m) = self.player_infos_for_all_players_by_id.borrow_mut().as_mut() {
                m.insert(player_id.to_string(), dict);
            }
        }
        already_loaded
    }

    /// `-infoArrived:forPlayer:` (BHNetNode version; BHServer calls it as `super`). The map is written ONLY in the
    /// "added" branch.
    pub fn info_arrived_for_player(&self, info: &PlayerInfo, player_id: &str) {
        if !self.live_contains(player_id) {
            self.add_live(info);
            self.delegate_players_changed();
            if let Some(m) = self.player_infos_for_all_players_by_id.borrow_mut().as_mut() {
                m.insert(player_id.to_string(), info.clone());
            }
        }
    }

    /// `-updatePlayerList:` (client-side path).
    pub fn update_player_list(&self, incoming: &[PlayerInfo]) {
        let (old_list, previous_count) = match self.live_player_infos.borrow_mut().as_mut() {
            Some(l) => {
                let old = l.clone();
                let prev = l.len() as i32;
                while l.len() >= 2 {
                    l.pop();
                }
                l.extend(incoming.iter().cloned());
                (old, prev)
            }
            // nil livePlayerInfos (after cleanup): every message is a nil no-op, but `playersChanged` still fires
            // and `0 > 0` skips the purge.
            None => {
                self.delegate_players_changed();
                return;
            }
        };
        let live = self.live_player_infos.borrow().clone().unwrap_or_default();
        for dict in &live {
            let pid = dict_player_id(dict);
            for old in &old_list {
                let old_pid = dict_player_id(old);
                // Receiver is the OLD id; a nil receiver compares equal (nil message returns 0).
                if nsstring::safe_case_insensitive_compare(old_pid.as_deref(), pid.as_deref()) == 0 {
                    let photo = dget(&old.borrow(), "photo").cloned();
                    if let Some(p) = photo {
                        dict.borrow_mut().insert("photo".to_string(), p);
                    }
                }
            }
        }
        self.delegate_players_changed();
        let live = self.live_player_infos.borrow().clone().unwrap_or_default();
        for dict in &live {
            // Note: `setObject:forKey:nil` would raise for a dict without playerID; skipped here.
            if let Some(pid) = dict_player_id(dict) {
                if let Some(m) = self.player_infos_for_all_players_by_id.borrow_mut().as_mut() {
                    m.insert(pid, dict.clone());
                }
            }
        }
        if (live.len() as i32) > previous_count {
            self.purge_chat_pic_cache();
        }
    }

    /// `-purgeChatPicCache`: empty body.
    pub fn purge_chat_pic_cache(&self) {}

    /// `-cleanup` (BHNetNode version; BHServer.cleanup calls it as `super`). The GameCenter branch is dead
    /// (`usesGameCenterPlayerInformation` is NO and nobody implements `postGCMatchInvitePlayers:`).
    pub fn cleanup(&self) {
        let m = self.match_.borrow_mut().take();
        if let Some(m) = m {
            m.set_delegate(None);
            m.disconnect();
        }
        *self.live_player_infos.borrow_mut() = None;
        *self.player_infos_for_all_players_by_id.borrow_mut() = None;
    }

    /// `-playerInfoForPeerID:`; `"server"` falls back to the owner (`mod == "owner"`) in GsDict order.
    pub fn player_info_for_peer_id(&self, peer_id: &str) -> Option<PlayerInfo> {
        let peer_id = if peer_id == "server" { self.server_player_id() } else { peer_id };
        let guard = self.player_infos_for_all_players_by_id.borrow();
        let map = guard.as_ref()?;
        let result = map.get(&peer_id.to_string()).cloned();
        if result.is_none() && peer_id == self.server_player_id() {
            for key in map.keys_in_order() {
                for info in map.get(&key) {
                    let is_owner = dget(&info.borrow(), "mod").and_then(|v| v.as_str()) == Some("owner");
                    if is_owner {
                        return Some(info.clone());
                    }
                }
            }
        }
        result
    }

    /// `-serverPlayerID`: constant `"server"`.
    pub fn server_player_id(&self) -> &'static str {
        "server"
    }

    /// `-match:player:didChangeState:` (BHNetNode version). Acts only on state 2: the FIRST live dict with that
    /// playerID gets `connected = false`; no `playersChanged`.
    pub fn match_player_did_change_state(&self, _m: &dyn BhMatch, player_id: &str, state: PlayerConnectionState) {
        if state == PlayerConnectionState::Disconnected {
            let live = self.live_player_infos.borrow().clone().unwrap_or_default();
            for d in &live {
                if dict_player_id(d).as_deref() == Some(player_id) {
                    d.borrow_mut().insert("connected".to_string(), PlistValue::boolean(false));
                    break;
                }
            }
            // usesGameCenterPlayerInformation is NO: GameCenter branch dead.
        }
    }

    /// `-match:connectionWithPlayer:failedWithError:`.
    pub fn match_connection_with_player_failed_with_error(&self, _m: &dyn BhMatch, player_id: &str) {
        let live = self.live_player_infos.borrow().clone().unwrap_or_default();
        for d in &live {
            if dict_player_id(d).as_deref() == Some(player_id) {
                d.borrow_mut().insert("connected".to_string(), PlistValue::boolean(false));
                self.delegate_players_changed();
                return;
            }
        }
    }

    /// `-match:didReceiveData:fromPlayer:`: no-op in the base class.
    pub fn match_did_receive_data_from_player(&self, _m: &dyn BhMatch, _data: &[u8], _player_id: &str) {}

    /// `-match:didFailWithError:`: `[self cleanup]` (virtual).
    pub fn match_did_fail_with_error(&self, v: &dyn NodeVirtuals) {
        v.cleanup()
    }

    /// `-sendChatMessage:sendToClients:`: virtual call of the 3-argument form with display = YES.
    pub fn send_chat_message_send_to_clients(&self, text: &str, clients: Option<&[String]>, v: &dyn NodeVirtuals) {
        v.send_chat_message_display_notification_send_to_clients(text, true, clients)
    }

    /// `-localPlayerID` (forwards to the match; nil match gives the empty string, as nil would).
    pub fn local_player_id(&self) -> String {
        self.match_.borrow().as_ref().map(|m| m.local_player_id()).unwrap_or_default()
    }

    /// `-localPlayerName` (forwards to the match).
    pub fn local_player_name(&self) -> String {
        self.match_.borrow().as_ref().map(|m| m.local_player_name()).unwrap_or_default()
    }

    /// `-audioChatSupported` (forwards to the match).
    pub fn audio_chat_supported(&self) -> bool {
        self.match_.borrow().as_ref().is_some_and(|m| m.audio_chat_supported())
    }

    /// `-voiceChatActive`: `voiceChat != nil`, always NO.
    pub fn voice_chat_active(&self) -> bool {
        false
    }

    /// `-notifyPlayersChanged`.
    pub fn notify_players_changed(&self) {
        self.delegate_players_changed();
    }

    /// `-livePlayerInfos`: snapshot of the array (same dict objects); empty after cleanup.
    pub fn live_player_infos(&self) -> Vec<PlayerInfo> {
        self.live_player_infos.borrow().clone().unwrap_or_default()
    }

    /// Remove the live player info at `index` (integration helper for `-clientFinishedAwaySimulation:`).
    pub fn remove_live_player_info_at(&self, index: usize) {
        if let Some(l) = self.live_player_infos.borrow_mut().as_mut() {
            if index < l.len() {
                l.remove(index);
            }
        }
    }

    /// `-match`.
    pub fn match_(&self) -> Option<Rc<dyn BhMatch>> {
        self.match_.borrow().clone()
    }

    /// `-netNodeType`.
    pub fn net_node_type(&self) -> NetNodeType {
        self.net_node_type
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct M;
    impl BhMatch for M {
        fn set_delegate(&self, _d: Option<Weak<dyn MatchDelegate>>) {}
    }

    struct D(std::cell::Cell<u32>);
    impl NodeDelegate for D {
        fn players_changed(&self) {
            self.0.set(self.0.get() + 1);
        }
    }

    fn info(id: &str, local: bool) -> PlayerInfo {
        let mut d = new_info_dict();
        d.insert("playerID".into(), PlistValue::string(id));
        d.insert("local".into(), PlistValue::boolean(local));
        Rc::new(RefCell::new(d))
    }

    #[test]
    fn init_loads_local_player() {
        let d = Rc::new(D(Default::default()));
        let dd: Rc<dyn NodeDelegate> = d.clone();
        let node = BhNetNode::init_with_delegate_match_net_node_type(
            Some(Rc::downgrade(&dd)),
            Rc::new(M),
            NetNodeType::LocalServer,
        );
        assert_eq!(d.0.get(), 1);
        assert_eq!(node.live_player_infos().len(), 1);
        assert!(node.load_info_for_player("local_player"));
        assert_eq!(d.0.get(), 1);
    }

    #[test]
    fn info_arrived_and_state() {
        let node = BhNetNode::init_with_delegate_match_net_node_type(None, Rc::new(M), NetNodeType::LocalServer);
        node.info_arrived_for_player(&info("a", false), "a");
        node.info_arrived_for_player(&info("b", true), "b");
        let ids: Vec<_> = node.live_player_infos().iter().map(|d| dict_player_id(d).unwrap()).collect();
        assert_eq!(ids, ["b", "local_player", "a"]);
        // duplicate: not added, map untouched
        node.info_arrived_for_player(&info("a", false), "a");
        assert_eq!(node.live_player_infos().len(), 3);
        node.match_player_did_change_state(&M, "a", PlayerConnectionState::Connected);
        assert!(dget(&node.live_player_infos()[2].borrow(), "connected").is_none());
        node.match_player_did_change_state(&M, "a", PlayerConnectionState::Disconnected);
        assert_eq!(dget(&node.live_player_infos()[2].borrow(), "connected").map(|v| v.bool_value()), Some(false));
    }

    #[test]
    fn owner_fallback_and_cleanup() {
        let node = BhNetNode::init_with_delegate_match_net_node_type(None, Rc::new(M), NetNodeType::LocalServer);
        let i = info("o", false);
        i.borrow_mut().insert("mod".into(), PlistValue::string("owner"));
        node.info_arrived_for_player(&i, "o");
        let got = node.player_info_for_peer_id("server").unwrap();
        assert!(Rc::ptr_eq(&got, &i));
        assert!(node.player_info_for_peer_id("zzz").is_none());
        node.cleanup();
        assert!(node.match_().is_none());
        assert!(node.live_player_infos().is_empty());
        assert!(node.player_info_for_peer_id("server").is_none());
    }

    #[test]
    fn update_list_photo_copy() {
        let node = BhNetNode::init_with_delegate_match_net_node_type(None, Rc::new(M), NetNodeType::LocalClient);
        let a = info("Abc", false);
        a.borrow_mut().insert("photo".into(), PlistValue::string("P"));
        node.info_arrived_for_player(&a, "Abc");
        let incoming = info("abc", false);
        node.update_player_list(std::slice::from_ref(&incoming));
        let l = node.live_player_infos();
        assert_eq!(l.len(), 2);
        assert_eq!(dget(&incoming.borrow(), "photo").and_then(|v| v.as_str()), Some("P"));
    }

    #[test]
    fn the_abstract_match_answers_its_defaults() {
        let m = M;
        m.set_delegate(None);
        assert_eq!((m.local_player_id().as_str(), m.local_player_name().as_str()), ("local_player", "local_player"));
        assert!(m.player_ids().is_none() && m.voice_chat_with_name("x").is_none());
        m.disconnect();
        assert_eq!(m.expected_player_count(), 0);
        assert!(!m.send_data_to_players_with_data_mode_error(Some(b"x"), &["a".to_string()], SendDataMode::Reliable));
        assert!(!m.send_unsequenced("a", b"x"));
        assert!(m.load_info_for_player("p").is_some());
        assert!(m.client_player_information_recieved_from_peer(None, "e").is_none());
        assert!(m.name_holder_elsewhere(None, "e").is_none());
        let i = info("p", false);
        m.client_approved_with_info(&i);
        m.send_player_information_to_server_password(Some(&i), Some("pw"));
        assert_eq!(m.persistent_id_for_net_id("n").as_deref(), Some("n"));
        assert!(!m.audio_chat_supported() && !m.requires_player_authentication());
        assert!(!m.uses_game_center_player_information() && m.connected_player_ids().is_none());
        m.remove_client("x");
        m.disconnect_player("x");
        assert!(!m.is_cloud_match() && !m.owned());
        assert!(m.owner_name().is_none() && m.cloud_salt().is_none() && m.host().is_none() && m.port().is_none());
        assert!(m.as_server_match().is_none());
        // A list from the server replaces the live players: dictionaries are kept, anything else dropped.
        let node = BhNetNode::init_with_delegate_match_net_node_type(None, Rc::new(M), NetNodeType::LocalClient);
        let list = crate::plist::helpers::data_from_property_list(&PlistValue::array(vec![
            PlistValue::Dict(new_info_dict()),
            PlistValue::string("not a dictionary"),
        ]))
        .unwrap();
        m.updated_player_list_recieved_from_server(&list, &node);
        m.updated_player_list_recieved_from_server(b"not a plist", &node);
    }

    /// A match that records what the node does to it and lists some players.
    struct R {
        ids: Vec<String>,
        log: RefCell<Vec<String>>,
    }
    impl BhMatch for R {
        fn set_delegate(&self, d: Option<Weak<dyn MatchDelegate>>) {
            self.log.borrow_mut().push(format!("delegate {}", d.is_some()));
        }
        fn player_ids(&self) -> Option<Vec<String>> {
            Some(self.ids.clone())
        }
        fn disconnect(&self) {
            self.log.borrow_mut().push("disconnect".to_string());
        }
        fn local_player_id(&self) -> String {
            "me".to_string()
        }
    }

    /// Never called: `update_match` only hands the delegate on to the match.
    struct Nobody;
    impl MatchDelegate for Nobody {
        fn match_player_did_change_state(&self, _m: &dyn BhMatch, _p: &str, _s: PlayerConnectionState) {}
        fn match_did_receive_data_from_player(&self, _m: &dyn BhMatch, _d: &[u8], _p: &str) {}
        fn info_arrived_for_player(&self, _i: &PlayerInfo, _p: &str) {}
        fn match_did_fail_with_error(&self, _m: &dyn BhMatch) {}
        fn boot_all_clients_due_to_no_credit(&self) {}
        fn match_connection_with_player_failed_with_error(&self, _m: &dyn BhMatch, _p: &str) {}
    }

    #[test]
    fn replacing_the_match_resets_the_live_players() {
        let first = Rc::new(R { ids: vec!["a".to_string()], log: RefCell::new(Vec::new()) });
        let node = BhNetNode::init_with_delegate_match_net_node_type(None, first.clone(), NetNodeType::LocalClient);
        assert_eq!(node.live_player_infos().len(), 2); // "me" and "a"
        let nobody: Rc<dyn MatchDelegate> = Rc::new(Nobody);
        nobody.match_player_did_change_state(&M, "p", PlayerConnectionState::Connected);
        nobody.match_did_receive_data_from_player(&M, b"", "p");
        nobody.info_arrived_for_player(&info("p", false), "p");
        nobody.match_did_fail_with_error(&M);
        nobody.boot_all_clients_due_to_no_credit();
        nobody.match_connection_with_player_failed_with_error(&M, "p");
        nobody.name_taken_over("p");
        // The same match again keeps the players and only re-registers the delegate.
        let same: Rc<dyn BhMatch> = first.clone();
        node.update_match(same, Rc::downgrade(&nobody));
        assert_eq!(node.live_player_infos().len(), 2);
        assert_eq!(first.log.borrow().as_slice(), ["delegate true"]);
        // Another match disconnects the old one and starts over with its own players.
        let second = Rc::new(R { ids: vec!["b".to_string(), "c".to_string()], log: RefCell::new(Vec::new()) });
        let other: Rc<dyn BhMatch> = second.clone();
        node.update_match(other, Rc::downgrade(&nobody));
        assert_eq!(first.log.borrow().as_slice(), ["delegate true", "delegate false", "disconnect"]);
        assert_eq!(second.log.borrow().as_slice(), ["delegate true"]);
        assert_eq!(node.live_player_infos().len(), 2); // the old list is dropped; "b" and "c" are the new players
                                                       // After a cleanup there is no current match to compare with.
        node.cleanup();
        assert_eq!(second.log.borrow().as_slice(), ["delegate true", "delegate false", "disconnect"]);
        let third: Rc<dyn BhMatch> = Rc::new(R { ids: vec![], log: RefCell::new(Vec::new()) });
        node.update_match(third, Rc::downgrade(&nobody));
        assert!(node.match_().is_some());
        // A match that lists no players leaves the node with none, and the cleaned-up node ignores new arrivals.
        node.update_match(Rc::new(M), Rc::downgrade(&nobody));
        node.cleanup();
        node.info_arrived_for_player(&info("q", false), "q");
        node.remove_live_player_info_at(0);
        assert!(node.live_player_infos().is_empty());
        assert!(!node.load_info_for_player("x")); // no map, no match: nothing to load
    }

    #[test]
    fn node_notifications_and_forwards() {
        let d = Rc::new(D(Default::default()));
        let dd: Rc<dyn NodeDelegate> = d.clone();
        let node = BhNetNode::init_with_delegate_match_net_node_type(
            Some(Rc::downgrade(&dd)),
            Rc::new(M),
            NetNodeType::OnlineClient,
        );
        let gone: Rc<dyn NodeDelegate> = Rc::new(D(Default::default()));
        let orphan = BhNetNode::init_with_delegate_match_net_node_type(
            Some(Rc::downgrade(&gone)),
            Rc::new(M),
            NetNodeType::OnlineClient,
        );
        drop(gone);
        orphan.notify_players_changed(); // nobody left to tell
        let base = d.0.get();
        node.info_arrived_for_player(&info("a", false), "a");
        // A failed connection marks the matching live player disconnected and tells the delegate; unknown ones do not.
        node.match_connection_with_player_failed_with_error(&M, "a");
        node.match_connection_with_player_failed_with_error(&M, "nobody");
        assert_eq!(d.0.get(), base + 2);
        assert_eq!(dget(&node.live_player_infos()[1].borrow(), "connected").map(|v| v.bool_value()), Some(false));
        node.match_did_receive_data_from_player(&M, b"x", "a");
        node.notify_players_changed();
        assert_eq!(d.0.get(), base + 3);
        assert_eq!(
            (node.local_player_id().as_str(), node.local_player_name().as_str()),
            ("local_player", "local_player")
        );
        assert!(!node.audio_chat_supported() && !node.voice_chat_active());
        assert_eq!(node.net_node_type(), NetNodeType::OnlineClient);
        node.remove_live_player_info_at(99); // out of range: nothing
        node.remove_live_player_info_at(1);
        assert_eq!(node.live_player_infos().len(), 1);
        node.purge_chat_pic_cache();
        // The virtual hooks reach the subclass.
        struct V(RefCell<Vec<String>>);
        impl NodeVirtuals for V {
            fn cleanup(&self) {
                self.0.borrow_mut().push("cleanup".to_string());
            }
            fn send_chat_message_display_notification_send_to_clients(&self, t: &str, d: bool, c: Option<&[String]>) {
                self.0.borrow_mut().push(format!("{t} {d} {}", c.map_or(0, |c| c.len())));
            }
        }
        let v = V(RefCell::new(Vec::new()));
        node.match_did_fail_with_error(&v);
        node.send_chat_message_send_to_clients("hi", Some(&["x".to_string()]), &v);
        assert_eq!(v.0.borrow().as_slice(), ["cleanup", "hi true 1"]);
        // After cleanup everything reads as nil: the forwards give empty answers, a player list update just notifies.
        node.cleanup();
        assert_eq!((node.local_player_id().as_str(), node.local_player_name().as_str()), ("", ""));
        assert!(!node.audio_chat_supported());
        let before = d.0.get();
        node.update_player_list(&[info("z", false)]);
        assert_eq!(d.0.get(), before + 1);
        node.match_player_did_change_state(&M, "z", PlayerConnectionState::Disconnected);
        assert_eq!(node.server_player_id(), "server");
    }

    #[test]
    fn the_server_id_falls_back_to_the_owner_and_to_nothing() {
        let node = BhNetNode::init_with_delegate_match_net_node_type(None, Rc::new(M), NetNodeType::LocalServer);
        // Players that are no owner do not stand in for the server.
        node.info_arrived_for_player(&info("guest", false), "guest");
        assert!(node.player_info_for_peer_id("server").is_none());
        assert!(node.player_info_for_peer_id("guest").is_some());
    }
}
