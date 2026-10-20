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
                if let Some(sm) = self.node.match_().as_ref().and_then(|m| m.as_server_match().map(|s| s.privacy())) {
                    if sm == 1 {
                        if let Some(m) = self.node.match_() {
                            if let Some(s) = m.as_server_match() {
                                s.set_privacy(2);
                            }
                        }
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
        if let Some(bl) = self.black_list.borrow().as_ref() {
            for entry in bl {
                let comps = components_separated_by_string(entry, " \\");
                if !comps.is_empty() {
                    let first = &comps[0];
                    if safe_ci(Some(first), alias.as_deref()) == 0 || safe_ci(Some(first), ip.as_deref()) == 0 {
                        return true;
                    }
                    if comps.len() > 1 && safe_ci(Some(&comps[1]), udid.as_deref()) == 0 {
                        return true;
                    }
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
}
