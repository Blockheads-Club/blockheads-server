//! `recentPlayers`, `<id>_info` records, the player-list broadcast (0x1e), resetList persistence and the
//! ownership-sign name list.
//!
//! `addPlayerDictToAllPlayersEver:`, `saveAllPlayersArray`, `saveResetList`, `savedPlayerInfoDataForPlayer:`,
//! `fullPlayerInformationNowAvailableForPlayer:`, `playerListToSendIncludingPhotosForClients:sendClient:`,
//! `sendInitialPlayerListToClient:`, `sendUpdatedPlayerListToClients`, `delayedSendUpdatedPlayerListToClients`;

use crate::foundation::clock::reference_time;
use crate::foundation::nsstring::{self, safe_case_insensitive_compare as safe_ci};
use crate::foundation::paths;
use crate::plist::helpers::{data_from_property_list, property_list_from_data};
use crate::plist::value::{PlistDict, PlistValue};
use crate::protocol::messages::{self, PlayerListEntry};
use crate::server::bhserver::{dict_get, dict_str, mutable_copy, write_data_atomic, BHServer};

impl BHServer {
    /// `-recentPlayers` (snapshot).
    pub fn recent_players(&self) -> Vec<PlistDict> {
        self.recent_players.borrow().clone()
    }

    /// `-addPlayerDictToAllPlayersEver:`.
    pub fn add_player_dict_to_all_players_ever(&self, player_dict: &PlistDict) {
        let mut d = mutable_copy(player_dict);
        for k in ["connected", "micOrSpeakerOn", "voiceConnected", "photo"] {
            d.map.remove(&k.to_string());
        }
        d.map.insert("lastSeen".to_string(), PlistValue::date(reference_time()));
        self.recent_players.borrow_mut().insert(0, d.clone());
        loop {
            let last = {
                let rp = self.recent_players.borrow();
                if rp.len() <= 0x80 {
                    break;
                }
                rp.last().cloned()
            };
            if let (Some(last), Some(w)) = (last, self.world()) {
                if let Some(pid) = dict_str(&last, "playerID") {
                    // F1b: in the reference this aborts when the evicted player has an `_allIndexes` record
                    // (World -> light_archive owns that behaviour, R1).
                    w.archive_light_blocks_for_client(&pid);
                }
            }
            self.recent_players.borrow_mut().pop();
        }
        let key = format!("{}_info", dict_str(player_dict, "playerID").unwrap_or("(null)".to_string()));
        for (data, db) in data_from_property_list(&PlistValue::Dict(d)).zip(self.server_database.as_ref()) {
            db.set_data(Some(&data), Some(&key));
        }
    }

    /// `-saveAllPlayersArray`.
    pub fn save_all_players_array(&self) {
        let arr = PlistValue::array(self.recent_players.borrow().iter().cloned().map(PlistValue::Dict).collect());
        for (data, db) in data_from_property_list(&arr).zip(self.server_database.as_ref()) {
            db.set_data(Some(&data), Some("recentPlayers"));
        }
    }

    /// `-saveResetList`: `SAVE/resetList.plist` (XML), removed when the list is nil.
    pub fn save_reset_list(&self) {
        let path = format!("{}/saves/{}/resetList.plist", paths::appsup(), self.save_id.as_deref().unwrap_or("(null)"));
        let rl = self.reset_list.borrow().clone();
        match rl {
            None => {
                let _ = std::fs::remove_file(&path);
            }
            Some(list) => {
                let arr = PlistValue::array(list.into_iter().map(PlistValue::string).collect());
                for data in data_from_property_list(&arr) {
                    write_data_atomic(&path, &data);
                }
            }
        }
    }

    /// `-savedPlayerInfoDataForPlayer:`: `<id>_info`, else the legacy file (only before the 1.7 migration finished).
    pub fn saved_player_info_data_for_player(&self, player_id: &str) -> Option<Vec<u8>> {
        let data = self.server_database.as_ref().and_then(|d| d.data_for_key(Some(&format!("{}_info", player_id))));
        if data.is_some() {
            return data;
        }
        let migrated = self.world().is_some_and(|w| w.has_finished_database_migration_to17());
        if migrated {
            return None;
        }
        let p0 = nsstring::substring_with_range(player_id, 0, 1);
        let p1 = nsstring::substring_with_range(player_id, 1, 1);
        let p2 = nsstring::substring_from_index(player_id, 2);
        let path = format!(
            "{}/saves/{}/players/{}/{}/{}/playerInfo.plist",
            paths::appsup(),
            self.save_id.as_deref().unwrap_or("(null)"),
            p0,
            p1,
            p2
        );
        std::fs::read(path).ok()
    }

    /// `-playerNameForPlayerWithIDIncludingOldPlayers:`.
    pub fn player_name_for_player_with_id_including_old_players(&self, player_id: Option<&str>) -> Option<String> {
        let pid = player_id?;
        if let Some(info) = self.node.player_info_for_peer_id(pid) {
            return dict_str(&info.borrow(), "alias");
        }
        let data = self.saved_player_info_data_for_player(pid)?;
        let plist = property_list_from_data(&data)?;
        plist.object_for_key("alias").and_then(|v| v.as_str()).map(|s| s.to_string())
    }

    /// `-fullPlayerInformationNowAvailableForPlayer:`.
    pub fn full_player_information_now_available_for_player(&self, player_id: &str) {
        let info = self.node.player_info_for_peer_id(player_id);
        let (alias, ip) = match &info {
            Some(i) => (dict_str(&i.borrow(), "alias"), dict_str(&i.borrow(), "ip")),
            None => (None, None),
        };
        // Bug-compatible: the "IP hidden" substitution is computed but never logged.
        let wn = self.world().and_then(|w| w.world_name()).unwrap_or("(null)".to_string());
        crate::nslog!(
            "{} - Player Connected {} | {} | {}",
            wn,
            alias.as_deref().unwrap_or("(null)"),
            ip.as_deref().unwrap_or("(null)"),
            player_id
        );
        self.clients_connected_with_photos_unsent.borrow_mut().push(player_id.to_string());
        self.send_initial_player_list_to_client(player_id);
    }

    /// `-playerListToSendIncludingPhotosForClients:sendClient:`.
    pub fn player_list_to_send_including_photos_for_clients_send_client(
        &self,
        include_clients: Option<&[String]>,
        send_client: &str,
    ) -> Vec<PlayerListEntry> {
        let send_minor = self
            .node
            .player_info_for_peer_id(send_client)
            .and_then(|i| dict_get(&i.borrow(), "minorVersion").map(|v| v.int_value()))
            .unwrap_or(0);
        let connected = self.connected_clients.borrow().clone();
        let photo_only = crate::server::server_photo::get().is_some();
        let mut out = Vec::new();
        for info in self.node.live_player_infos() {
            let d = info.borrow().clone();
            let Some(pid) = dict_str(&d, "playerID") else { continue };
            if pid == send_client {
                continue;
            }
            let Some(alias) = dict_str(&d, "alias") else { continue };
            let mut photo = None;
            if let Some(PlistValue::Data(ph)) = dict_get(&d, "photo") {
                // The server's own photo (not in the original) is never withheld: it is not tracked as unsent, and
                // a client must keep it whichever list it got last.
                let is_local = dict_get(&d, "local").is_some_and(|v| v.bool_value());
                let withheld = !is_local && send_minor > 0 && include_clients.is_some_and(|inc| !inc.contains(&pid));
                if !withheld {
                    photo = Some(ph.bytes.clone());
                }
            }
            let local = dict_get(&d, "local").is_some_and(|v| v.bool_value());
            let is_connected = local || connected.contains(&pid);
            // The local info of the server's photo (not in the original) is listed as an invisible admin: clients
            // keep its photo for the chat but leave the server out of the players they show.
            let hidden = local && photo_only;
            let modk = if self.player_is_owner_with_alias(Some(&alias)) {
                Some("owner")
            } else if self.player_is_admin_with_alias(Some(&alias)) {
                Some("admin")
            } else if self.player_is_mod_with_alias(Some(&alias)) {
                Some("mod")
            } else {
                None
            };
            let supermod = if hidden {
                Some("mode2")
            } else if self.player_is_cloud_wide_admin_with_alias(Some(&alias)) {
                Some(if self.player_is_cloud_wide_invisible_admin_with_alias(Some(&alias)) { "mode2" } else { "mode1" })
            } else {
                None
            };
            out.push(PlayerListEntry { alias, photo, player_id: pid, connected: is_connected, modk, supermod });
        }
        out
    }

    /// `-sendInitialPlayerListToClient:`: straight to the match, only when non-empty.
    pub fn send_initial_player_list_to_client(&self, send_client: &str) {
        let list = self.player_list_to_send_including_photos_for_clients_send_client(None, send_client);
        if !list.is_empty() {
            if let Some(m) = self.node.match_() {
                m.send_data_to_players_with_data_mode_error(
                    Some(&messages::player_list(&list)),
                    &[send_client.to_string()],
                    crate::net::bhmatch::SendDataMode::Reliable,
                );
            }
        }
    }

    /// `-sendUpdatedPlayerListToClients`: coalesced by a 2.0 s timer and the flag.
    pub fn send_updated_player_list_to_clients(&self) {
        self.needs_to_send_updated_list_to_clients.set(true);
        let srv = self.rc();
        self.perform_after_delay(2.0, Box::new(move || srv.delayed_send_updated_player_list_to_clients()));
    }

    /// `-delayedSendUpdatedPlayerListToClients`.
    pub fn delayed_send_updated_player_list_to_clients(&self) {
        if !self.needs_to_send_updated_list_to_clients.get() {
            return;
        }
        if self.is_cloud_match() {
            crate::nslog!("CLOUD:STAT:PLAYERS:{}", self.node.live_player_infos().len());
        }
        let ids = self.connected_clients.borrow().clone();
        let unsent = self.clients_connected_with_photos_unsent.borrow().clone();
        for id in ids {
            let list = self.player_list_to_send_including_photos_for_clients_send_client(Some(&unsent), &id);
            if let Some(m) = self.node.match_() {
                m.send_data_to_players_with_data_mode_error(
                    Some(&messages::player_list(&list)),
                    std::slice::from_ref(&id),
                    crate::net::bhmatch::SendDataMode::Reliable,
                );
            }
        }
        self.needs_to_send_updated_list_to_clients.set(false);
        self.clients_connected_with_photos_unsent.borrow_mut().clear();
    }

    /// `-getRecentPlayerNamesForOwnershipSign`. Aborts like the reference when a name list has 2+ entries
    /// (feature `faithful-crashes`, F1a / 5.12).
    pub fn get_recent_player_names_for_ownership_sign(&self) -> PlistValue {
        // (name, id) pairs; `None` name == nil alias.
        type Entry = (Option<String>, Option<String>);
        let mut online: Vec<Entry> = Vec::new();
        let mut count = 0usize;
        for pid in self.connected_clients.borrow().clone() {
            if let Some(info) = self.node.player_info_for_peer_id(&pid) {
                let d = info.borrow();
                online.push((dict_str(&d, "alias"), dict_str(&d, "playerID")));
                count += 1;
            }
        }
        #[allow(unused_mut)]
        let sort = |mut v: Vec<Entry>| -> Vec<Entry> {
            if v.len() > 1 {
                #[cfg(feature = "faithful-crashes")]
                {
                    crate::foundation::exception::gnustep_block_abort();
                }
                #[cfg(not(feature = "faithful-crashes"))]
                {
                    v.sort_by(|a, b| safe_ci(a.0.as_deref(), b.0.as_deref()).cmp(&0));
                }
            }
            v
        };
        let sorted_online = sort(online);
        let mut offline: Vec<Entry> = Vec::new();
        for d in self.recent_players.borrow().iter() {
            if count > 49 {
                break;
            }
            let alias = dict_str(d, "alias");
            let mut found = false;
            for oe in &sorted_online {
                if safe_ci(alias.as_deref(), oe.0.as_deref()) == 0 {
                    found = true;
                }
            }
            if !found {
                offline.push((alias, dict_str(d, "playerID")));
                count += 1;
            }
        }
        let sorted_offline = sort(offline);
        let items = sorted_online
            .into_iter()
            .chain(sorted_offline)
            .map(|(name, id)| {
                // dictionaryWithObjectsAndKeys: stops at the first nil object.
                let mut m = crate::server::bhserver::new_mutable_dict();
                if let Some(n) = name {
                    m.map.insert("name".to_string(), PlistValue::string(n));
                    if let Some(i) = id {
                        m.map.insert("id".to_string(), PlistValue::string(i));
                    }
                }
                PlistValue::Dict(m)
            })
            .collect();
        PlistValue::array(items)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::foundation::md5::string_from_md5;
    use crate::server::bhserver::test_support::*;

    #[test]
    fn join_records_recent_players_and_info() {
        let h = Harness::new("players_join");
        let pid = h.join("Alice");
        assert_eq!(pid, string_from_md5("alice"));
        let rp = h.srv.recent_players();
        assert_eq!(rp.len(), 1);
        assert_eq!(dict_str(&rp[0], "alias").as_deref(), Some("Alice"));
        assert!(dict_get(&rp[0], "connected").is_none());
        assert!(matches!(dict_get(&rp[0], "lastSeen"), Some(PlistValue::Date(_))));
        let saved = h.srv.saved_player_info_data_for_player(&pid).expect("info record");
        let p = property_list_from_data(&saved).unwrap();
        assert_eq!(p.object_for_key("alias").and_then(|v| v.as_str()), Some("Alice"));
        // recentPlayers persisted
        let db = h.srv.server_database().unwrap();
        assert!(db.data_for_key(Some("recentPlayers")).is_some());
        assert_eq!(h.srv.player_name_for_player_with_id_including_old_players(Some(&pid)).as_deref(), Some("Alice"));
        assert_eq!(h.srv.player_name_for_player_with_id_including_old_players(Some("nobody")), None);
    }

    #[test]
    fn recent_players_capped_at_128_with_archive_call() {
        let h = Harness::new("players_cap");
        for i in 0..130 {
            let mut d = crate::server::bhserver::new_mutable_dict();
            d.insert("playerID".to_string(), PlistValue::string(format!("id{:03}", i)));
            d.insert("alias".to_string(), PlistValue::string(format!("p{}", i)));
            h.srv.add_player_dict_to_all_players_ever(&d);
        }
        assert_eq!(h.srv.recent_players().len(), 128);
        assert_eq!(dict_str(&h.srv.recent_players()[0], "playerID").as_deref(), Some("id129"));
        let calls = h.w.calls.borrow().clone();
        assert!(calls.contains(&"archive_light id000".to_string()));
        assert!(calls.contains(&"archive_light id001".to_string()));
    }

    #[test]
    fn player_list_shape_and_coalescing() {
        let h = Harness::new("players_list");
        let a = h.join("Alice");
        h.m.clear();
        let b = h.join("Bob");
        // newcomer gets the initial list (the other player) before anything else
        let to_b = h.m.packets_to(&b);
        let first_list = to_b.iter().find(|p| p[0] == 0x1e).expect("0x1e to Bob");
        let pl = property_list_from_data(&first_list[1..]).unwrap();
        let PlistValue::Array(arr) = pl else { unreachable!() };
        assert_eq!(arr.items.len(), 1);
        assert_eq!(arr.items[0].object_for_key("alias").and_then(|v| v.as_str()), Some("Alice"));
        assert_eq!(arr.items[0].object_for_key("playerID").and_then(|v| v.as_str()), Some(a.as_str()));
        // every join schedules its own 2 s timer; only the first does the work
        let n = h.delayed.borrow().len();
        assert!(n >= 2);
        h.m.clear();
        h.run_delayed();
        let lists_to_a: Vec<_> = h.m.packets_to(&a).into_iter().filter(|p| p[0] == 0x1e).collect();
        assert_eq!(lists_to_a.len(), 1);
        let lists_to_b: Vec<_> = h.m.packets_to(&b).into_iter().filter(|p| p[0] == 0x1e).collect();
        assert_eq!(lists_to_b.len(), 1);
    }

    #[test]
    fn reset_list_roundtrip() {
        let h = Harness::new("players_reset");
        *h.srv.reset_list.borrow_mut() = Some(vec!["".into(), "b".into()]);
        h.srv.save_reset_list();
        let path = format!("{}/saves/{}/resetList.plist", paths::appsup(), h.srv.save_id.clone().unwrap());
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("<string>b</string>"), "{}", text);
        *h.srv.reset_list.borrow_mut() = None;
        h.srv.save_reset_list();
        assert!(!std::path::Path::new(&path).exists());
    }

    #[test]
    fn ownership_sign_one_online_one_offline() {
        let h = Harness::new("players_sign");
        let _a = h.join("Alice");
        let mut d = crate::server::bhserver::new_mutable_dict();
        d.insert("playerID".to_string(), PlistValue::string("offid"));
        d.insert("alias".to_string(), PlistValue::string("Zed"));
        h.srv.recent_players.borrow_mut().push(d);
        let v = h.srv.get_recent_player_names_for_ownership_sign();
        let PlistValue::Array(a) = v else { unreachable!() };
        // Alice appears in recent players as well but is online, so it is skipped there.
        assert_eq!(a.items.len(), 2);
        assert_eq!(a.items[0].object_for_key("name").and_then(|v| v.as_str()), Some("Alice"));
        assert_eq!(a.items[1].object_for_key("name").and_then(|v| v.as_str()), Some("Zed"));
    }

    fn plist_file(path: &str, v: &PlistValue) {
        std::fs::create_dir_all(std::path::Path::new(path).parent().unwrap()).unwrap();
        std::fs::write(path, data_from_property_list(v).unwrap()).unwrap();
    }

    fn named(alias: &str) -> PlistValue {
        let mut d = crate::server::bhserver::new_mutable_dict();
        d.insert("alias".to_string(), PlistValue::string(alias));
        PlistValue::Dict(d)
    }

    #[test]
    fn without_a_database_nothing_is_stored() {
        let h = Harness::with_files("players_nodb", |save| std::fs::write(format!("{save}/server_db"), b"x").unwrap());
        let a = h.join("Alice");
        h.srv.save_all_players_array();
        assert!(h.srv.saved_player_info_data_for_player(&a).is_none());
        assert_eq!(h.srv.recent_players().len(), 1);
    }

    #[test]
    fn legacy_player_files_are_read_until_the_migration_is_done() {
        let h = Harness::new("players_legacy");
        let pid = "abcdef0123456789abcdef0123456789";
        let save = format!("{}/saves/{}", paths::appsup(), h.w.id);
        let file = |id: &str| format!("{save}/players/{}/{}/{}/playerInfo.plist", &id[..1], &id[1..2], &id[2..]);
        plist_file(&file(pid), &named("Legacy"));
        assert!(h.srv.saved_player_info_data_for_player(pid).is_none()); // migrated: only the database counts
        h.w.migrated.set(false);
        assert!(h.srv.saved_player_info_data_for_player(pid).is_some());
        assert_eq!(h.srv.player_name_for_player_with_id_including_old_players(Some(pid)).as_deref(), Some("Legacy"));
        assert_eq!(h.srv.player_name_for_player_with_id_including_old_players(None), None);
        assert_eq!(
            h.srv.player_name_for_player_with_id_including_old_players(Some("0123456789abcdef0123456789abcdef")),
            None
        );
        // Garbage and nameless records give no name; a live player is named from the live info.
        let junk = "fedcba9876543210fedcba9876543210";
        std::fs::create_dir_all(std::path::Path::new(&file(junk)).parent().unwrap()).unwrap();
        std::fs::write(file(junk), b"not a plist").unwrap();
        assert_eq!(h.srv.player_name_for_player_with_id_including_old_players(Some(junk)), None);
        let bare = "11111111111111111111111111111111";
        plist_file(&file(bare), &PlistValue::Dict(crate::server::bhserver::new_mutable_dict()));
        assert_eq!(h.srv.player_name_for_player_with_id_including_old_players(Some(bare)), None);
        let live = h.join("Alice");
        assert_eq!(h.srv.player_name_for_player_with_id_including_old_players(Some(&live)).as_deref(), Some("Alice"));
        // A player the node has never heard of is logged with nulls.
        h.srv.full_player_information_now_available_for_player("nobody");
    }

    #[test]
    fn the_reset_list_file_follows_the_list() {
        let h = Harness::new("players_reset");
        let path = format!("{}/saves/{}/resetList.plist", paths::appsup(), h.w.id);
        *h.srv.reset_list.borrow_mut() = Some(vec!["a".to_string(), "b".to_string()]);
        h.srv.save_reset_list();
        assert!(std::fs::read_to_string(&path).unwrap().contains("<string>b</string>"));
        *h.srv.reset_list.borrow_mut() = None;
        h.srv.save_reset_list();
        assert!(!std::path::Path::new(&path).exists());
    }

    #[test]
    fn the_server_photo_is_listed_for_every_client_that_has_not_got_the_list_yet() {
        let h = Harness::new("players_server_photo");
        let ids: Vec<String> = ["Ann", "Bob"].iter().map(|a| h.join(a)).collect();
        // Without a photo the server is not a listed player (vanilla).
        let list = h.srv.player_list_to_send_including_photos_for_clients_send_client(None, &ids[0]);
        assert!(list.iter().all(|e| e.alias != "SERVER"));
        // The local player info the match gives the server when a photo is configured.
        let info = h.srv.node.player_info_for_peer_id(&h.srv.local_player_id()).unwrap();
        {
            let mut d = info.borrow_mut();
            d.insert("alias".to_string(), PlistValue::string("SERVER"));
            d.insert("local".to_string(), PlistValue::boolean(true));
            d.insert("photo".to_string(), PlistValue::data(vec![9, 8, 7]));
        }
        // A client that joins now (initial list: nothing withheld) gets the photo; the server is an invisible admin
        // (no button in the clients' top bar).
        crate::server::server_photo::set(Some(vec![9, 8, 7]));
        let list = h.srv.player_list_to_send_including_photos_for_clients_send_client(None, &ids[0]);
        let srv = list.iter().find(|e| e.alias == "SERVER").unwrap();
        assert_eq!(
            (srv.photo.clone(), srv.connected, srv.supermod, srv.player_id.clone()),
            (Some(vec![9, 8, 7]), true, Some("mode2"), h.srv.local_player_id())
        );
        assert!(list.iter().filter(|e| e.alias != "SERVER").all(|e| e.supermod.is_none()), "players stay visible");
        // ... and so does a client that is only sent the photos of the players that joined since the last update.
        let list = h.srv.player_list_to_send_including_photos_for_clients_send_client(Some(&[]), &ids[0]);
        assert_eq!(list.iter().find(|e| e.alias == "SERVER").unwrap().photo, Some(vec![9, 8, 7]));
        // It travels as NSData in the 0x1e plist.
        let pkt = crate::protocol::messages::player_list(&list);
        assert!(pkt.windows(6).any(|w| w == b"<data>"));
        crate::server::server_photo::set(None);
    }

    #[test]
    fn the_player_list_marks_ranks_photos_and_the_requester() {
        let h = Harness::new("players_list");
        *h.m.owner.borrow_mut() = Some("Olly".to_string());
        let ids: Vec<String> = ["Olly", "Ann", "Mo", "Cy", "Vi", "Pam"].iter().map(|a| h.join(a)).collect();
        h.srv.modify_list_for_player_or_ip_is_added_list_type("ann", true, 2);
        h.srv.modify_list_for_player_or_ip_is_added_list_type("mo", true, 3);
        h.srv.cloud_wide_admin_list.replace(Some(vec!["cy".to_string(), "vi".to_string()]));
        h.srv.cloud_wide_invisible_admin_list.replace(Some(vec!["vi".to_string()]));
        h.srv
            .node
            .player_info_for_peer_id(&ids[5])
            .unwrap()
            .borrow_mut()
            .insert("photo".to_string(), PlistValue::data(vec![7]));
        // Two live infos that cannot be listed: one without a player id and one without an alias.
        for (key, val) in [("alias", "Idless"), ("playerID", "no-alias")] {
            let mut d = crate::server::bhserver::new_mutable_dict();
            d.insert(key.to_string(), PlistValue::string(val));
            let info: crate::net::bhmatch::PlayerInfo = std::rc::Rc::new(std::cell::RefCell::new(d));
            h.srv.node.info_arrived_for_player(&info, val);
        }
        let list = h.srv.player_list_to_send_including_photos_for_clients_send_client(None, &ids[0]);
        let by = |alias: &str| list.iter().find(|e| e.alias == alias).unwrap();
        assert_eq!(list.len(), 5); // everyone but the requester
        assert_eq!(by("Ann").modk, Some("admin"));
        assert_eq!(by("Mo").modk, Some("mod"));
        assert_eq!(by("Cy").supermod, Some("mode1"));
        assert_eq!(by("Vi").supermod, Some("mode2"));
        assert_eq!(by("Pam").photo, Some(vec![7]));
        // The owner is marked when somebody else asks; a photo is withheld from clients not yet sent it.
        let to_ann = h.srv.player_list_to_send_including_photos_for_clients_send_client(Some(&[]), &ids[1]);
        assert_eq!(to_ann.iter().find(|e| e.alias == "Olly").unwrap().modk, Some("owner"));
        assert_eq!(to_ann.iter().find(|e| e.alias == "Pam").unwrap().photo, None);
        let sent = [ids[5].clone()];
        let with_photo = h.srv.player_list_to_send_including_photos_for_clients_send_client(Some(&sent), &ids[1]);
        assert_eq!(with_photo.iter().find(|e| e.alias == "Pam").unwrap().photo, Some(vec![7]));
        // The coalesced broadcast reaches everyone connected once.
        h.m.clear();
        h.srv.send_updated_player_list_to_clients();
        h.srv.delayed_send_updated_player_list_to_clients();
        h.srv.delayed_send_updated_player_list_to_clients();
        assert_eq!(h.m.sent.borrow().len(), 6);
    }

    #[test]
    fn the_ownership_list_is_capped_and_sorted() {
        let h = Harness::new("players_sign");
        let a = h.join("Zoe");
        let b = h.join("Abe");
        // Someone without a name is listed by an empty entry; the rest are named and sorted case-insensitively.
        let nameless = "22222222222222222222222222222222";
        let mut d = crate::server::bhserver::new_mutable_dict();
        d.insert("playerID".to_string(), PlistValue::string(nameless));
        let info: crate::net::bhmatch::PlayerInfo = std::rc::Rc::new(std::cell::RefCell::new(d));
        h.srv.node.info_arrived_for_player(&info, nameless);
        h.srv.connected_clients.borrow_mut().push(nameless.to_string());
        for i in 0..70 {
            let mut p = crate::server::bhserver::new_mutable_dict();
            p.insert("alias".to_string(), PlistValue::string(format!("old{i:02}")));
            if i % 2 == 0 {
                p.insert("playerID".to_string(), PlistValue::string(format!("{i:032}")));
            }
            h.srv.recent_players.borrow_mut().push(p);
        }
        let PlistValue::Array(list) = h.srv.get_recent_player_names_for_ownership_sign() else { unreachable!() };
        let names: Vec<Option<String>> =
            list.items.iter().map(|v| v.object_for_key("name").and_then(|v| v.as_str()).map(String::from)).collect();
        assert_eq!(names.len(), 50); // three online (one nameless) plus the first 47 recent
        assert!(names.contains(&Some("Abe".to_string())) && names.contains(&Some("Zoe".to_string())));
        assert!(names.contains(&None));
        let _ = (a, b);
    }

    #[test]
    fn a_cloud_match_logs_the_player_count_with_every_broadcast() {
        let h = RealMatchHarness::new("players_cloud", Some("SALT"), Some("Boss"), Some("public"), 10.0);
        h.srv.send_updated_player_list_to_clients();
        h.srv.delayed_send_updated_player_list_to_clients();
        assert!(!h.srv.needs_to_send_updated_list_to_clients.get());
    }

    #[test]
    fn eviction_copes_with_nameless_records_and_a_missing_world() {
        let h = Harness::new("players_evict");
        for _ in 0..0x80 {
            h.srv.recent_players.borrow_mut().push(crate::server::bhserver::new_mutable_dict());
        }
        let mut d = crate::server::bhserver::new_mutable_dict();
        d.insert("playerID".to_string(), PlistValue::string("p1"));
        h.srv.add_player_dict_to_all_players_ever(&d); // the evicted record has no player id
        assert_eq!(h.srv.recent_players().len(), 0x80);
        *h.srv.world.borrow_mut() = None;
        h.srv.recent_players.borrow_mut().push(d.clone());
        h.srv.add_player_dict_to_all_players_ever(&d); // and without a world nothing is archived
        assert_eq!(h.srv.recent_players().len(), 0x80);
    }
}
