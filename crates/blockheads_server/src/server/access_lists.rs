//! Access lists (black / white / admin / mod / cloud-wide / curse / reset), the permission predicates and the
//! 0x2b admin-or-mod notice.
//!
//! `modifyListForPlayerOrIP:isAdded:listType:(banUDID:)`, `playerIsBlackListedWithInfo:`, `playerIsWhiteListedWithInfo:`,
//! `playerIsOwnerWithAlias:`, `playerIsCloudWide*`, `playerIsAdminWithID:/Alias:`, `playerIsModWithAlias:`,
//! `playerIsBannedWithID:`, `playerIsConnectedWithInfo:`.
use std::cell::RefCell;
use std::collections::HashMap;

use crate::foundation::nsstring::{components_separated_by_string, safe_case_insensitive_compare as safe_ci};
use crate::foundation::paths;
use crate::plist::helpers::property_list_from_data;
use crate::plist::value::PlistDict;
use crate::protocol::messages;
use crate::server::bhserver::{dict_get, dict_str, new_mutable_dict, pv_into_dict, BHServer};
use crate::storage::lists::{self, LIST_TYPE_ADMINLIST, LIST_TYPE_BLACKLIST, LIST_TYPE_MODLIST};

/// `any entry safeCI-equals alias` (a nil alias never matches: `safeCI(entry, nil) == 1`).
fn list_contains_ci(list: &RefCell<Option<Vec<String>>>, alias: Option<&str>) -> bool {
    list.borrow().as_ref().map_or(false, |l| l.iter().any(|e| safe_ci(Some(e), alias) == 0))
}

impl BHServer {
    fn docs_path(&self) -> std::path::PathBuf {
        std::path::PathBuf::from(paths::appsup())
    }

    fn save_id_str(&self) -> &str {
        self.save_id.as_deref().unwrap_or("(null)")
    }

    /// `-reloadLists`: clear the cache and the 7 lists, reload from the files, log the cloud lines and boot
    /// every connected player who is no longer authorised (reason byte 12).
    pub fn reload_lists(&self) {
        *self.cached_player_banned.borrow_mut() = None;
        for l in [
            &self.black_list,
            &self.white_list,
            &self.admin_list,
            &self.cloud_wide_admin_list,
            &self.cloud_wide_invisible_admin_list,
            &self.curse_list,
            &self.mod_list,
        ] {
            *l.borrow_mut() = None;
        }
        let loaded = lists::load_all(&self.docs_path(), self.save_id_str());
        *self.black_list.borrow_mut() = loaded.black;
        *self.white_list.borrow_mut() = loaded.white;
        if self.is_cloud_match() {
            let white_empty = self.white_list.borrow().as_ref().map_or(true, |l| l.is_empty());
            if white_empty {
                crate::nslog!("CLOUD:SETTING:WHITELISTED:OFF");
            } else {
                crate::nslog!("CLOUD:SETTING:WHITELISTED:ON");
                if let Some(s) = self.node.match_().as_ref().and_then(|m| m.as_server_match()) {
                    if s.privacy() == 1 {
                        s.set_privacy(2);
                        crate::nslog!("CLOUD:SETTING:PRIVACY:SEARCHABLE");
                    }
                }
            }
        }
        *self.admin_list.borrow_mut() = loaded.admin;
        *self.cloud_wide_admin_list.borrow_mut() = loaded.cloud_admin;
        *self.cloud_wide_invisible_admin_list.borrow_mut() = loaded.cloud_invisible_admin;
        *self.curse_list.borrow_mut() = loaded.curse;
        *self.mod_list.borrow_mut() = loaded.moderator;

        // Enforcement: whitelist failures first, then blacklist hits (a player may appear twice).
        let mut boot: Vec<String> = Vec::new();
        let connected = self.connected_clients.borrow().clone();
        let white_nonempty = self.white_list.borrow().as_ref().map_or(false, |l| !l.is_empty());
        if white_nonempty {
            for pid in &connected {
                let info = self.player_info_dict_for_peer_id(pid).unwrap_or_else(new_mutable_dict);
                if !self.player_is_white_listed_with_info(&info) {
                    boot.push(pid.clone());
                }
            }
        }
        let black_nonempty = self.black_list.borrow().as_ref().map_or(false, |l| !l.is_empty());
        if black_nonempty {
            for pid in &connected {
                let info = self.player_info_dict_for_peer_id(pid).unwrap_or_else(new_mutable_dict);
                if self.player_is_black_listed_with_info(&info) {
                    boot.push(pid.clone());
                }
            }
        }
        for pid in boot {
            self.boot_player_was_ban(&pid, true);
        }
        self.players_changed();
    }

    /// `-clearList:`: header-only file, then `reloadLists`.
    pub fn clear_list(&self, list_type: i32) {
        lists::write_header_only(&self.docs_path(), self.save_id_str(), list_type);
        self.reload_lists();
    }

    /// The in-memory list of a list type.
    fn list_for_type(&self, list_type: i32) -> Option<Vec<String>> {
        match list_type {
            lists::LIST_TYPE_BLACKLIST => self.black_list.borrow().clone(),
            lists::LIST_TYPE_WHITELIST => self.white_list.borrow().clone(),
            lists::LIST_TYPE_ADMINLIST => self.admin_list.borrow().clone(),
            lists::LIST_TYPE_MODLIST => self.mod_list.borrow().clone(),
            _ => None,
        }
    }

    /// `-modifyListForPlayerOrIP:isAdded:listType:` (banUDID = NO).
    pub fn modify_list_for_player_or_ip_is_added_list_type(
        &self,
        name_or_ip: &str,
        is_added: bool,
        list_type: i32,
    ) -> Option<String> {
        self.modify_list_for_player_or_ip_is_added_list_type_ban_udid(name_or_ip, is_added, list_type, false)
    }

    /// `-modifyListForPlayerOrIP:isAdded:listType:banUDID:` steps 1-10.
    pub fn modify_list_for_player_or_ip_is_added_list_type_ban_udid(
        &self,
        name_or_ip: &str,
        is_added: bool,
        list_type: i32,
        ban_udid: bool,
    ) -> Option<String> {
        // 1.
        if self.player_is_owner_with_alias(Some(name_or_ip)) {
            return None;
        }
        // 2.
        let mut udid: Option<String> = None;
        if list_type == LIST_TYPE_BLACKLIST && is_added && ban_udid {
            let recent = self.recent_players.borrow().clone();
            for d in recent.iter() {
                let alias = dict_str(d, "alias");
                if safe_ci(Some(name_or_ip), alias.as_deref()) == 0
                    || safe_ci(Some(name_or_ip), dict_str(d, "ip").as_deref()) == 0
                {
                    let on_reset = self
                        .reset_list
                        .borrow()
                        .as_ref()
                        .map_or(false, |rl| rl.iter().any(|n| safe_ci(Some(n), alias.as_deref()) == 0));
                    if !on_reset {
                        udid = dict_str(d, "udidNew");
                    }
                    break;
                }
            }
        }
        // 3.-6.
        let existing = self.list_for_type(list_type);
        let result = lists::build_modified_list(list_type, existing.as_deref(), name_or_ip, is_added, udid.as_deref());
        // 7.
        lists::write_text_atomic(
            &lists::list_file_path(&self.docs_path(), self.save_id_str(), list_type),
            &result.new_text,
        );
        self.reload_lists();
        // 8.
        if result.changed && (list_type == LIST_TYPE_ADMINLIST || list_type == LIST_TYPE_MODLIST) {
            let connected = self.connected_clients.borrow().clone();
            for pid in connected {
                let info = self.player_info_dict_for_peer_id(&pid);
                let Some(info) = info else { continue };
                let alias = dict_str(&info, "alias");
                if safe_ci(Some(name_or_ip), alias.as_deref()) == 0
                    || safe_ci(Some(name_or_ip), dict_str(&info, "ip").as_deref()) == 0
                {
                    let pkt = if self.player_is_admin_with_alias(alias.as_deref()) {
                        messages::admin_or_mod_changed(1, 1)
                    } else if self.player_is_mod_with_alias(alias.as_deref()) {
                        messages::admin_or_mod_changed(0, 1)
                    } else {
                        messages::admin_or_mod_changed(0, 0)
                    };
                    self.send_network_data_to_peers_reliable(pkt, &[pid.clone()], true);
                    self.send_updated_player_list_to_clients();
                    if let Some(w) = self.world() {
                        if let Some(sc) = w.server_client(&pid) {
                            sc.set_is_admin(self.player_is_admin_with_alias(alias.as_deref()));
                        }
                    }
                    break;
                }
            }
        }
        // 9.
        if list_type == LIST_TYPE_BLACKLIST {
            let recent = self.recent_players.borrow().clone();
            for d in recent.iter() {
                if safe_ci(Some(name_or_ip), dict_str(d, "alias").as_deref()) == 0
                    || safe_ci(Some(name_or_ip), dict_str(d, "ip").as_deref()) == 0
                {
                    if let (Some(w), Some(pid)) = (self.world(), dict_str(d, "playerID")) {
                        if let Some(dw) = w.dynamic_world() {
                            dw.user_ban_changed_is_banned(&pid, self.player_is_black_listed_with_info(d));
                        }
                    }
                }
            }
        }
        // 10.
        Some(result.message)
    }

    /// `-playerIsBlackListedWithInfo:`.
    pub fn player_is_black_listed_with_info(&self, info: &PlistDict) -> bool {
        let alias = dict_str(info, "alias");
        if self.player_is_mod_with_alias(alias.as_deref()) {
            return false;
        }
        let ip = dict_str(info, "ip");
        let udid = dict_str(info, "udidNew");
        for entry in self.black_list.borrow().iter().flatten() {
            let comps = components_separated_by_string(entry, " \\");
            // A split always has a first component.
            for first in comps.first() {
                if safe_ci(Some(first), alias.as_deref()) == 0 || safe_ci(Some(first), ip.as_deref()) == 0 {
                    return true;
                }
                if comps.len() > 1 && safe_ci(Some(&comps[1]), udid.as_deref()) == 0 {
                    return true;
                }
            }
        }
        false
    }

    /// `-playerIsWhiteListedWithInfo:`.
    pub fn player_is_white_listed_with_info(&self, info: &PlistDict) -> bool {
        let wl = self.white_list.borrow().clone();
        let Some(wl) = wl else { return true };
        if wl.is_empty() {
            return true;
        }
        let alias = dict_str(info, "alias");
        let ip = dict_str(info, "ip");
        for entry in &wl {
            if safe_ci(Some(entry), alias.as_deref()) == 0 || safe_ci(Some(entry), ip.as_deref()) == 0 {
                return true;
            }
        }
        self.player_is_mod_with_alias(alias.as_deref())
    }

    /// `-playerIsOwnerWithAlias:`.
    pub fn player_is_owner_with_alias(&self, alias: Option<&str>) -> bool {
        let owner = self.node.match_().and_then(|m| m.owner_name());
        match owner {
            Some(o) => safe_ci(Some(&o), alias) == 0,
            None => false,
        }
    }

    /// `-playerIsCloudWideAdminWithAlias:` (true for invisible admins too).
    pub fn player_is_cloud_wide_admin_with_alias(&self, alias: Option<&str>) -> bool {
        list_contains_ci(&self.cloud_wide_admin_list, alias)
            || self.player_is_cloud_wide_invisible_admin_with_alias(alias)
    }

    /// `-playerIsCloudWideInvisibleAdminWithAlias:`.
    pub fn player_is_cloud_wide_invisible_admin_with_alias(&self, alias: Option<&str>) -> bool {
        list_contains_ci(&self.cloud_wide_invisible_admin_list, alias)
    }

    /// `-playerIsAdminWithID:` (alias lookup through `playerInfoForPeerID:`; nil gives NO).
    pub fn player_is_admin_with_id(&self, player_id: Option<&str>) -> bool {
        let alias = self.player_alias_for_peer_id(player_id);
        self.player_is_admin_with_alias(alias.as_deref())
    }

    /// `-playerIsAdminWithAlias:`.
    pub fn player_is_admin_with_alias(&self, alias: Option<&str>) -> bool {
        self.player_is_owner_with_alias(alias)
            || list_contains_ci(&self.admin_list, alias)
            || self.player_is_cloud_wide_admin_with_alias(alias)
    }

    /// `-playerIsModWithAlias:`.
    pub fn player_is_mod_with_alias(&self, alias: Option<&str>) -> bool {
        list_contains_ci(&self.mod_list, alias) || self.player_is_admin_with_alias(alias)
    }

    /// `-playerIsBannedWithID:` (caches saved-record lookups only).
    pub fn player_is_banned_with_id(&self, player_id: Option<&str>) -> bool {
        let Some(pid) = player_id else { return false };
        if let Some(c) = self.cached_player_banned.borrow().as_ref() {
            if let Some(v) = c.get(pid) {
                return *v;
            }
        }
        if let Some(info) = self.player_info_dict_for_peer_id(pid) {
            return self.player_is_black_listed_with_info(&info);
        }
        let Some(data) = self.saved_player_info_data_for_player(pid) else { return false };
        let Some(plist) = property_list_from_data(&data) else { return false };
        let Some(d) = pv_into_dict(plist) else { return false };
        let r = self.player_is_black_listed_with_info(&d);
        self.cached_player_banned.borrow_mut().get_or_insert_with(HashMap::new).insert(pid.to_string(), r);
        r
    }

    /// `-playerIsConnectedWithInfo:`.
    pub fn player_is_connected_with_info(&self, info: &PlistDict) -> bool {
        if dict_get(info, "local").map_or(false, |v| v.bool_value()) {
            return true;
        }
        match dict_str(info, "playerID") {
            Some(pid) => self.connected_clients.borrow().iter().any(|e| *e == pid),
            None => false,
        }
    }

    /// `-whiteList`.
    pub fn white_list(&self) -> Option<Vec<String>> {
        self.white_list.borrow().clone()
    }
    /// `-blackList`.
    pub fn black_list(&self) -> Option<Vec<String>> {
        self.black_list.borrow().clone()
    }
    /// `-adminList`.
    pub fn admin_list(&self) -> Option<Vec<String>> {
        self.admin_list.borrow().clone()
    }
    /// `-modList`.
    pub fn mod_list(&self) -> Option<Vec<String>> {
        self.mod_list.borrow().clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::server::bhserver::test_support::*;
    use crate::storage::lists::{ADMIN_HEADER, BLACK_HEADER, MOD_HEADER, WHITE_HEADER};

    fn write(save: &str, name: &str, text: &str) {
        std::fs::write(format!("{}/{}", save, name), text).unwrap();
    }

    #[test]
    fn fresh_world_lists_f3() {
        let h = Harness::with_files("lists_f3", |s| {
            write(s, "blacklist.txt", BLACK_HEADER);
            write(s, "whitelist.txt", WHITE_HEADER);
            write(s, "adminlist.txt", ADMIN_HEADER);
            write(s, "modlist.txt", MOD_HEADER);
        });
        assert_eq!(h.srv.admin_list(), Some(vec!["".to_string()]));
        assert_eq!(h.srv.black_list(), None);
        // F3: the empty alias is admin
        assert!(h.srv.player_is_admin_with_alias(Some("")));
        assert!(!h.srv.player_is_admin_with_alias(None));
        assert!(!h.srv.player_is_admin_with_alias(Some("alice")));
    }

    #[test]
    fn nil_lists_when_files_missing() {
        let h = Harness::new("lists_missing");
        assert_eq!(h.srv.admin_list(), None);
        assert!(!h.srv.player_is_admin_with_alias(Some("")));
    }

    #[test]
    fn modify_admin_sends_notice_and_message() {
        let h = Harness::new("lists_modify");
        let pid = h.join("Alice");
        h.m.clear();
        let msg = h.srv.modify_list_for_player_or_ip_is_added_list_type("alice", true, 2);
        assert_eq!(msg.as_deref(), Some("alice has been added to the adminlist"));
        assert!(h.srv.player_is_admin_with_alias(Some("Alice")));
        let pk = h.m.packets_to(&pid);
        assert!(pk.iter().any(|p| p == &[0x2b, 1, 0, 0, 0, 1, 0, 0, 0]), "{:?}", pk);
        let msg = h.srv.modify_list_for_player_or_ip_is_added_list_type("alice", false, 2);
        assert_eq!(msg.as_deref(), Some("alice has been removed from the adminlist"));
        let msg = h.srv.modify_list_for_player_or_ip_is_added_list_type("alice", true, 3);
        assert_eq!(msg.as_deref(), Some("alice has been added to the modlist"));
        let pk = h.m.packets_to(&pid);
        assert!(pk.iter().any(|p| p == &[0x2b, 0, 0, 0, 0, 1, 0, 0, 0]));
        h.srv.modify_list_for_player_or_ip_is_added_list_type("alice", false, 3);
        assert!(h.m.packets_to(&pid).iter().any(|p| p == &[0x2b, 0, 0, 0, 0, 0, 0, 0, 0]));
    }

    #[test]
    fn ban_boots_connected_player_with_reason_12() {
        let h = Harness::new("lists_ban");
        let pid = h.join("Bob");
        h.m.clear();
        let msg = h.srv.modify_list_for_player_or_ip_is_added_list_type_ban_udid("bob", true, 0, true);
        assert_eq!(msg.as_deref(), Some("bob has been added to the blacklist"));
        assert!(h.m.packets_to(&pid).iter().any(|p| p == &[0x26, 0x0c]));
        // delayed disconnect after 2 s
        h.run_delayed();
        assert_eq!(h.m.disconnected.borrow().as_slice(), &[pid.clone()]);
        // blacklisted now
        let info = h.srv.player_info_dict_for_peer_id(&pid).unwrap();
        assert!(h.srv.player_is_black_listed_with_info(&info));
        // the udid was stored: "bob \udid-Bob"
        let bl = h.srv.black_list().unwrap();
        assert!(bl.iter().any(|e| e.starts_with("bob \\")), "{:?}", bl);
    }

    #[test]
    fn owner_cannot_be_listed() {
        let h = Harness::new("lists_owner");
        *h.m.owner.borrow_mut() = Some("Olga".into());
        assert_eq!(h.srv.modify_list_for_player_or_ip_is_added_list_type("OLGA", true, 0), None);
        assert!(h.srv.player_is_owner_with_alias(Some("olga")));
        assert!(h.srv.player_is_mod_with_alias(Some("olga")));
    }

    #[test]
    fn whitelist_rules() {
        let h = Harness::with_files("lists_white", |s| write(s, "whitelist.txt", &format!("{}\nzed", WHITE_HEADER)));
        let mut d = new_mutable_dict();
        d.insert("alias".to_string(), crate::plist::value::PlistValue::string("ZED"));
        assert!(h.srv.player_is_white_listed_with_info(&d));
        let mut d2 = new_mutable_dict();
        d2.insert("alias".to_string(), crate::plist::value::PlistValue::string("other"));
        assert!(!h.srv.player_is_white_listed_with_info(&d2));
    }

    fn info_of(alias: &str, ip: &str, udid: &str) -> PlistDict {
        let mut d = new_mutable_dict();
        for (k, v) in [("alias", alias), ("ip", ip), ("udidNew", udid)] {
            d.insert(k.to_string(), crate::plist::value::PlistValue::string(v));
        }
        d
    }

    #[test]
    fn reloading_boots_players_that_lost_their_access() {
        let h = Harness::new("lists_reload");
        let bob = h.join("Bob");
        let amy = h.join("Amy");
        // A connected id without info is checked as an empty dictionary.
        h.srv.connected_clients.borrow_mut().push("ghost".to_string());
        let save = format!("{}/saves/{}", paths::appsup(), h.w.id);
        write(&save, "whitelist.txt", &format!("{}\nsomeoneelse", WHITE_HEADER));
        write(&save, "blacklist.txt", &format!("{}\nbob", BLACK_HEADER));
        h.m.clear();
        h.srv.reload_lists();
        for id in [&bob, &amy, &"ghost".to_string()] {
            assert!(h.m.packets_to(id).contains(&vec![0x26, 0x0c]), "{id}");
        }
    }

    #[test]
    fn a_cloud_world_with_a_whitelist_turns_searchable() {
        let h = RealMatchHarness::new("lists_cloud", Some("SALT"), Some("Boss"), Some("public"), 10.0);
        let path = format!("{}/saves/{}/whitelist.txt", paths::appsup(), h.w.id);
        std::fs::write(&path, format!("{}\nsomeone", WHITE_HEADER)).unwrap();
        h.srv.reload_lists();
        assert_eq!(h.srv.privacy_string().as_deref(), Some("searchable"));
        h.srv.reload_lists(); // already searchable: left alone
        assert_eq!(h.srv.privacy_string().as_deref(), Some("searchable"));
        std::fs::write(&path, WHITE_HEADER).unwrap();
        h.srv.reload_lists();
    }

    #[test]
    fn rank_changes_reach_the_server_client_and_bans_reach_the_dynamic_world() {
        let h = Harness::new("lists_ranks");
        let bob = h.join("Bob");
        let sc = crate::server::server_client::ServerClient::init_with_client_id_server_light_block_index_light_block_database(
            &bob, &h.srv, 31, None,
        );
        h.w.clients.borrow_mut().insert(bob.clone(), sc.clone());
        h.srv.modify_list_for_player_or_ip_is_added_list_type("127.0.0.1", true, 2); // by address: not by name
        assert!(!sc.is_admin());
        h.srv.modify_list_for_player_or_ip_is_added_list_type("bob", true, 2);
        assert!(sc.is_admin());
        // Nobody connected matches: no notice.
        h.m.clear();
        h.srv.modify_list_for_player_or_ip_is_added_list_type("nobody", true, 3);
        assert!(h.m.sent.borrow().is_empty());
        // A ban also tells the dynamic world; a player without an id in the recent list is skipped.
        h.srv.modify_list_for_player_or_ip_is_added_list_type("bob", true, 0);
        assert!(h.w.dw.calls.borrow().contains(&format!("userban {bob} false")));
        h.srv.recent_players.borrow_mut().insert(0, info_of("Bob", "9.9.9.9", "u"));
        h.srv.modify_list_for_player_or_ip_is_added_list_type("bob", false, 0);
        // An unknown list type changes nothing in memory.
        assert!(h.srv.modify_list_for_player_or_ip_is_added_list_type("x", true, 9).is_some());
        h.srv.clear_list(9);
    }

    #[test]
    fn ban_with_device_uses_the_remembered_udid_unless_the_name_is_being_reset() {
        let h = Harness::new("lists_udid");
        h.srv.recent_players.borrow_mut().push(info_of("Cara", "1.2.3.4", "udid-cara"));
        h.srv.recent_players.borrow_mut().push(info_of("Dan", "1.2.3.5", "udid-dan"));
        *h.srv.reset_list.borrow_mut() = Some(vec!["dan".to_string()]);
        h.srv.modify_list_for_player_or_ip_is_added_list_type_ban_udid("1.2.3.4", true, 0, true);
        h.srv.modify_list_for_player_or_ip_is_added_list_type_ban_udid("dan", true, 0, true);
        h.srv.modify_list_for_player_or_ip_is_added_list_type_ban_udid("unknown", true, 0, true);
        let bl = h.srv.black_list().unwrap();
        assert!(bl.contains(&"1.2.3.4 \\udid-cara".to_string()), "{bl:?}");
        assert!(bl.contains(&"dan".to_string()) && bl.contains(&"unknown".to_string()), "{bl:?}");
        // The blacklist matches by name, address or the stored device; mods are exempt.
        assert!(h.srv.player_is_black_listed_with_info(&info_of("Other", "1.2.3.4", "x")));
        assert!(h.srv.player_is_black_listed_with_info(&info_of("Other", "8.8.8.8", "udid-cara")));
        assert!(h.srv.player_is_black_listed_with_info(&info_of("Dan", "8.8.8.8", "y")));
        assert!(!h.srv.player_is_black_listed_with_info(&info_of("Eve", "8.8.8.8", "z")));
        h.srv.modify_list_for_player_or_ip_is_added_list_type("dan", true, 3);
        assert!(!h.srv.player_is_black_listed_with_info(&info_of("Dan", "8.8.8.8", "y")));
    }

    #[test]
    fn bans_are_looked_up_live_then_in_the_saved_record() {
        let h = Harness::new("lists_banned");
        assert!(!h.srv.player_is_banned_with_id(None));
        let pid = h.join("Fay");
        assert!(!h.srv.player_is_banned_with_id(Some(&pid)));
        h.srv.modify_list_for_player_or_ip_is_added_list_type("fay", true, 0);
        assert!(h.srv.player_is_banned_with_id(Some(&pid)));
        // An offline player is judged by the saved record, and the answer is remembered.
        let saved = "33333333333333333333333333333333";
        let blob = crate::plist::helpers::data_from_property_list(&crate::plist::value::PlistValue::Dict(info_of(
            "Fay", "5.5.5.5", "u",
        )))
        .unwrap();
        h.srv.server_database().unwrap().set_data(Some(&blob), Some(&format!("{saved}_info")));
        assert!(h.srv.player_is_banned_with_id(Some(saved)));
        h.srv.black_list.replace(None);
        assert!(h.srv.player_is_banned_with_id(Some(saved)));
        // Unknown, unparsable and non-dictionary records are not banned.
        assert!(!h.srv.player_is_banned_with_id(Some("44444444444444444444444444444444")));
        let junk = "55555555555555555555555555555555";
        h.srv.server_database().unwrap().set_data(Some(b"not a plist"), Some(&format!("{junk}_info")));
        assert!(!h.srv.player_is_banned_with_id(Some(junk)));
        let flat = "66666666666666666666666666666666";
        let array =
            crate::plist::helpers::data_from_property_list(&crate::plist::value::PlistValue::array(vec![])).unwrap();
        h.srv.server_database().unwrap().set_data(Some(&array), Some(&format!("{flat}_info")));
        assert!(!h.srv.player_is_banned_with_id(Some(flat)));
    }

    #[test]
    fn connection_predicate_and_whitelist_edges() {
        let h = Harness::new("lists_conn");
        let mut local = new_mutable_dict();
        local.insert("local".to_string(), crate::plist::value::PlistValue::boolean(true));
        assert!(h.srv.player_is_connected_with_info(&local));
        assert!(!h.srv.player_is_connected_with_info(&new_mutable_dict()));
        let pid = h.join("Gus");
        assert!(h.srv.player_is_connected_with_info(&h.srv.player_info_dict_for_peer_id(&pid).unwrap()));
        // An empty whitelist admits everybody; a mod passes a whitelist they are not on.
        h.srv.white_list.replace(Some(vec![]));
        assert!(h.srv.player_is_white_listed_with_info(&info_of("Anyone", "1.1.1.1", "u")));
        h.srv.white_list.replace(Some(vec!["gus".to_string()]));
        h.srv.mod_list.replace(Some(vec!["moe".to_string()]));
        assert!(h.srv.player_is_white_listed_with_info(&info_of("Moe", "1.1.1.1", "u")));
        assert!(h.srv.player_is_white_listed_with_info(&info_of("Other", "gus", "u")));
        assert!(!h.srv.player_is_white_listed_with_info(&info_of("Other", "1.1.1.1", "u")));
        // The owner and cloud-wide ranks.
        assert!(h.srv.player_is_admin_with_id(None) == h.srv.player_is_admin_with_alias(None));
    }

    #[test]
    fn edge_cases_without_a_world_a_dynamic_world_or_a_blacklist() {
        let h = Harness::new("lists_edges");
        let bob = h.join("Bob");
        h.w.dw_present.set(false);
        h.srv.modify_list_for_player_or_ip_is_added_list_type("bob", true, 0);
        assert!(h.w.dw.calls.borrow().is_empty());
        *h.srv.world.borrow_mut() = None;
        h.srv.modify_list_for_player_or_ip_is_added_list_type("bob", false, 0);
        h.srv.modify_list_for_player_or_ip_is_added_list_type("bob", true, 3); // nobody to tell without a world
        h.srv.black_list.replace(None);
        assert!(!h.srv.player_is_black_listed_with_info(&info_of("Bob", "1.1.1.1", "u")));
        assert!(!h.srv.player_is_banned_with_id(Some(&bob)));
        // A cloud match that is not a server match has nothing to make searchable.
        h.m.cloud.set(true);
        h.srv.white_list.replace(None);
        let save = format!("{}/saves/{}", paths::appsup(), h.w.id);
        write(&save, "whitelist.txt", &format!("{}\nsomeone", WHITE_HEADER));
        h.srv.reload_lists();
        assert!(h.srv.white_list().is_some());
    }
}
