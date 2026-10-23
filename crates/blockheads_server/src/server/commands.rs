//! `handleCommand:issueClient:` (console and chat commands) and `getDebugLog`.
//!

use std::rc::Rc;

use crate::foundation::nsstring::{self, components_separated_by_string, safe_case_insensitive_compare as safe_ci};
use crate::net::bhmatch::NetNodeType;
use crate::plist::value::PlistValue;
use crate::protocol::messages;
use crate::server::bhserver::{new_mutable_dict, BHServer};

// ----, byte exact (generated from the spec text; lengths 918/1155/89/225/290 are asserted in tests) ----
/// `MOD_COMMANDS`.
pub const MOD_COMMANDS: &str = "\n/HELP - display this message.\n/PLAYERS - list currently active players.\n/KICK player_name - kicks player_name, but they will still be allowed to reconnect.\n/BAN player_name_or_ip - adds player_name_or_ip to the blacklist, ban's that user's device from reconnecting with a different name, and kicks anyone who is no longer authorized.\n/BAN-NO-DEVICE player_name_or_ip - works the same as the ban command, but doesn't ban the player's device.\n/UNBAN player_name_or_ip - removes player_name_or_ip from the blacklist.\n/WHITELIST player_name_or_ip - adds player_name_or_ip to the whitelist, and kicks anyone who is no longer authorized.\n/UNWHITELIST player_name_or_ip - removes player_name_or_ip from the whitelist, and kicks anyone who is no longer authorized.\n/LIST-BLACKLIST lists the 50 most recent players on the blacklist.\n/LIST-WHITELIST lists the 50 most recent players on the whitelist.\n/CLEAR - clears the chat.\n";
/// `ADMIN_EXTRA_COMMANDS`.
pub const ADMIN_EXTRA_COMMANDS: &str = "/LIST-MODLIST - (admin only) lists the 50 most recent players on the modlist.\n/LIST-ADMINLIST - (admin only) lists the 50 most recent players on the adminlist.\n/PVP-ON - (admin only) enables PVP (Player vs. Player) so players can directly hurt each other.\n/PVP-OFF - (admin only) disables PVP (Player vs. Player) so players cannot directly hurt each other \n/LOAD-LISTS - (admin only) reloads the blacklist.txt, modlist.txt and adminlist.txt files, and kicks anyone who is no longer authorized.\n/MOD player_name - (admin only) adds player_name to the modlist, allowing them to issue some server commands via chat.\n/UNMOD player_name - (admin only) removes player_name from the modlist.\n/ADMIN player_name - (admin only) adds player_name to the adminlist, allowing them to issue server commands via chat.\n/UNADMIN player_name - (admin only) removes player_name from the adminlist.\n/CLEAR-BLACKLIST - (admin only) removes all names from the blacklist.\n/CLEAR-WHITELIST - (admin only) removes all names from the whitelist.\n/CLEAR-MODLIST - (admin only) removes all names from the modlist.\n/CLEAR-ADMINLIST - (admin only) removes all names from the adminlist.\n";
/// `ADMIN_EXTRA_NON_LAN_COMMANDS`.
pub const ADMIN_EXTRA_NON_LAN_COMMANDS: &str =
    "/REPAIR - (admin only) toggles repair mode, which clears the contents of glitched tiles \n";
/// `ADMIN_EXTRA_NON_CLOUD_COMMANDS`.
pub const ADMIN_EXTRA_NON_CLOUD_COMMANDS: &str = "/RESET-OWNER player_name - (admin only) allows anyone to connect as player_name, and whoever does will take ownership and control of that username including that user's blockheads, and be able to log in with it from then on.\n";
/// `OWNER_EXTRA_CLOUD_COMMANDS`.
pub const OWNER_EXTRA_CLOUD_COMMANDS: &str = "/SET-PASSWORD password - (owner only) sets a new password, which all players except the owner must use in order to connect.\n/REMOVE-PASSWORD - (owner only) removes the password, so all players may connect.\n/SET-PRIVACY public/searchable/private - (owner only) changes the privacy setting.\n\n";

/// `[command rangeOfString:X].location == 1`.
fn at1(command: &Option<String>, literal: &str) -> bool {
    match command {
        Some(c) => nsstring::range_of_string(c, literal).map_or(false, |(l, _)| l == 1),
        None => false,
    }
}

/// `[command substringWithRange:NSMakeRange(n, length - n)]`; only asked for after `at1` matched, so the command
/// exists.
fn arg(command: &Option<String>, n: usize) -> String {
    command.as_ref().map(|c| nsstring::substring_from_index(c, n)).unwrap_or_default()
}

/// `list-*` output: newest (last) first, at most 50 entries, first `" \\"` component of each.
fn list_output(header: &str, list: Option<Vec<String>>) -> String {
    let mut out = String::from(header);
    let list = list.unwrap_or_default();
    let count = list.len() as i32;
    let mut index = count - 1;
    while index >= 0 && index >= count - 50 {
        let comps = components_separated_by_string(&list[index as usize], " \\");
        if !comps.is_empty() {
            out.push_str(&format!("{}, ", comps[0]));
        }
        index -= 1;
    }
    out
}

impl BHServer {
    /// The reply of a command: the console goes through the delegate, a client through the network.
    pub(crate) fn reply_to(&self, text: &str, issue_client: Option<&str>) {
        match issue_client {
            None => {
                if let Some(d) = self.delegate() {
                    let mut dict = new_mutable_dict();
                    dict.map.insert("message".to_string(), PlistValue::string(text));
                    dict.map.insert("alias".to_string(), PlistValue::string("SERVER"));
                    d.chat_message_recieved_display_notification(dict, true);
                }
            }
            Some(c) => self.send_chat_message_send_to_clients(text, Some(&[c.to_string()])),
        }
    }

    /// `-handleCommand:issueClient:`; `issue_client` None == console. May `ns_raise` (console `/repair`) and
    /// `stop` may drop the last reference of the server, so a clone is kept alive.
    pub fn handle_command_issue_client<'a>(&self, command: impl Into<Option<&'a str>>, issue_client: Option<&str>) {
        self.run_command(command.into(), issue_client);
    }

    /// The body of [`BHServer::handle_command_issue_client`] (not generic, so all its lines count once).
    fn run_command(&self, command: Option<&str>, issue_client: Option<&str>) {
        let _keep_alive: Rc<BHServer> = self.rc();
        // 1. usesGameCenterPlayerInformation is NO on the dedicated server.
        // 2. strip ONE trailing newline
        let mut command: Option<String> = command.map(|c| c.to_string());
        if let Some(c) = &command {
            let len = nsstring::length(c);
            if let Some((loc, _)) = nsstring::range_of_string(c, "\n") {
                if loc + 1 == len && len > 1 {
                    command = Some(nsstring::substring_to_index(c, len - 1));
                }
            }
        }
        // Mods (not in the original): mod commands and `command` handlers run before the built-ins.
        if let (Some(h), Some(c)) = (crate::mods::hooks(), command.as_deref()) {
            if h.command(issue_client, c) {
                return;
            }
        }
        // 3.-4. issuer flags
        let alias = self.player_alias_for_peer_id(issue_client);
        let a = alias.as_deref();
        let mut issuer_is_owner = issue_client.is_none();
        if issue_client.is_some() {
            issuer_is_owner = self.player_is_owner_with_alias(a);
        }
        let mut issuer_is_admin = issuer_is_owner;
        if !issuer_is_admin {
            issuer_is_admin = self.player_is_admin_with_alias(a);
        }
        let mut issuer_is_mod = issuer_is_admin;
        if !issuer_is_mod {
            issuer_is_mod = self.player_is_mod_with_alias(a);
        }
        // 5. lowercase (so every argument is lower-cased)
        command = command.map(|c| nsstring::lowercase_string(&c));
        let mut incorrect_permissions = false;
        let mut print_result: Option<String> = None;
        let is_cloud = self.is_cloud_match();
        let world = self.world();
        let match_ = self.node.match_();
        let server_match = match_.as_ref().and_then(|m| m.as_server_match());

        // ---- Block A: console-only owner commands ----
        if issue_client.is_none() {
            if at1(&command, "set-credit ") {
                let amount = arg(&command, 12);
                if let Some(s) = server_match {
                    s.set_credit(nsstring::int_value(&amount) as f32);
                }
            } else if at1(&command, "set-owner ") {
                let owner = arg(&command, 11);
                if let (true, Some(s)) = (nsstring::length(&owner) != 0, server_match) {
                    s.set_owner(Some(&owner));
                }
            } else if at1(&command, "remove-owner") {
                if let Some(s) = server_match {
                    s.set_owner(None);
                }
            } else if at1(&command, "set-salt ") {
                let salt = arg(&command, 10);
                if let (true, Some(s)) = (nsstring::length(&salt) != 0, server_match) {
                    s.set_cloud_salt(Some(&salt));
                }
            }
        }

        // ---- Block B: owner commands ----
        let owner_entry = issue_client.is_none() || self.player_is_owner_with_alias(a);
        if owner_entry {
            if at1(&command, "set-privacy ") {
                let privacy = arg(&command, 13);
                if let (true, Some(s)) = (nsstring::length(&privacy) != 0, server_match) {
                    let white_nonempty = self.white_list.borrow().as_ref().map_or(false, |l| !l.is_empty());
                    if is_cloud && white_nonempty && safe_ci(Some(&privacy), Some("public")) == 0 {
                        print_result = Some(
                            "Error: This world is whitelisted, and whitelisted worlds can only be serachable or private. Privacy setting has been left unchanged."
                                .to_string(),
                        );
                    } else if safe_ci(Some(&privacy), Some("public")) != 0
                        && safe_ci(Some(&privacy), Some("searchable")) != 0
                        && safe_ci(Some(&privacy), Some("private")) != 0
                    {
                        // no reply
                    } else {
                        s.set_privacy_by_string(Some(&privacy));
                        if is_cloud {
                            crate::nslog!("CLOUD:SETTING:PRIVACY:{}", nsstring::uppercase_string(&privacy));
                        }
                        print_result = Some(format!("Privacy setting changed to {}.", privacy));
                    }
                }
            } else if at1(&command, "set-password ") {
                let password = arg(&command, 14);
                if nsstring::length(&password) != 0 && server_match.is_some() {
                    if nsstring::length(&password) < 16 {
                        if let Some(w) = &world {
                            w.new_server_password_set(Some(&password));
                        }
                        print_result = Some("Password set.".to_string());
                        if is_cloud {
                            crate::nslog!("CLOUD:SETTING:PASSWORD:ON");
                        }
                    } else {
                        print_result = Some(
                            "Error setting password. Sorry, but passwords must not be longer than 16 characters."
                                .to_string(),
                        );
                    }
                }
            } else if at1(&command, "remove-password") && server_match.is_some() {
                let had = world.as_ref().map_or(false, |w| w.server_password().is_some());
                print_result =
                    Some(if !had { "There was already no password set." } else { "Password removed." }.to_string());
                if let Some(w) = &world {
                    w.new_server_password_set(None);
                }
                if is_cloud {
                    crate::nslog!("CLOUD:SETTING:PASSWORD:OFF");
                }
            }
        }

        // ---- Block C ----
        let mut handled = true;
        if at1(&command, "help") {
            let mut s = MOD_COMMANDS.to_string();
            if issuer_is_admin {
                s.push_str(ADMIN_EXTRA_COMMANDS);
                if self.node.net_node_type() != NetNodeType::LocalServer {
                    s.push_str(ADMIN_EXTRA_NON_LAN_COMMANDS);
                }
                if !is_cloud {
                    s.push_str(ADMIN_EXTRA_NON_CLOUD_COMMANDS);
                }
            }
            if issuer_is_owner && is_cloud {
                s.push_str(OWNER_EXTRA_CLOUD_COMMANDS);
            }
            print_result = Some(s);
        } else if at1(&command, "players") {
            let players = self.connected_clients.borrow().clone();
            if players.is_empty() {
                print_result = Some("No players currently connected.".to_string());
            } else {
                let mut ps = String::new();
                let recent = self.recent_players.borrow().clone();
                for cid in &players {
                    if let Some(d) = recent
                        .iter()
                        .find(|d| crate::server::bhserver::dict_str(d, "playerID").as_deref() == Some(cid.as_str()))
                    {
                        let alias = crate::server::bhserver::dict_str(d, "alias");
                        let mut ip = crate::server::bhserver::dict_str(d, "ip");
                        if self.player_is_cloud_wide_admin_with_alias(alias.as_deref()) {
                            ip = Some("IP hidden".to_string());
                        }
                        ps.push_str(&format!(
                            "\n{} | {} | {}",
                            alias.as_deref().unwrap_or("(null)"),
                            ip.as_deref().unwrap_or("(null)"),
                            cid
                        ));
                    }
                }
                print_result = Some(format!("There are {} players connected:{}", players.len(), ps));
            }
        } else if at1(&command, "stop") {
            if !issuer_is_admin {
                incorrect_permissions = true;
            } else if let Some(d) = self.delegate() {
                d.stop();
            }
        } else if at1(&command, "pvp-on") {
            if !issuer_is_admin {
                incorrect_permissions = true;
            } else {
                let enabled = world.as_ref().map_or(false, |w| w.pvp_enabled());
                if !enabled {
                    if let Some(w) = &world {
                        w.set_pvp_enabled_display_notifciation(true, true);
                    }
                    print_result = Some("PVP is now enabled.".to_string());
                } else {
                    print_result = Some("PVP was already enabled.".to_string());
                }
                if is_cloud {
                    crate::nslog!("CLOUD:SETTING:PVP:ON");
                }
            }
        } else if at1(&command, "pvp-off") {
            if !issuer_is_admin {
                incorrect_permissions = true;
            } else {
                let enabled = world.as_ref().map_or(false, |w| w.pvp_enabled());
                if !enabled {
                    print_result = Some("PVP was already disabled.".to_string());
                } else {
                    if let Some(w) = &world {
                        w.set_pvp_enabled_display_notifciation(false, true);
                    }
                    print_result = Some("PVP is now disabled.".to_string());
                }
                if is_cloud {
                    crate::nslog!("CLOUD:SETTING:PVP:OFF");
                }
            }
        } else if at1(&command, "load-lists") {
            if !issuer_is_admin {
                incorrect_permissions = true;
            } else {
                self.reload_lists();
                print_result = Some("Reloaded lists.".to_string());
            }
        } else if at1(&command, "clear-blacklist") {
            if !issuer_is_admin {
                incorrect_permissions = true;
            } else {
                self.clear_list(0);
                print_result = Some("Blacklist cleared.".to_string());
            }
        } else if at1(&command, "clear-whitelist") {
            if !issuer_is_admin {
                incorrect_permissions = true;
            } else {
                self.clear_list(1);
                print_result = Some("Whitelist cleared.".to_string());
            }
        } else if at1(&command, "clear-adminlist") {
            if !issuer_is_admin {
                incorrect_permissions = true;
            } else {
                self.clear_list(2);
                print_result = Some("Adminlist cleared.".to_string());
            }
        } else if at1(&command, "clear-modlist") {
            if !issuer_is_admin {
                incorrect_permissions = true;
            } else {
                self.clear_list(3);
                print_result = Some("Modlist cleared.".to_string());
            }
        } else if at1(&command, "clear") {
            if !issuer_is_mod {
                incorrect_permissions = true;
            } else {
                self.chat_history.borrow_mut().clear();
                self.send_network_data_to_peers_reliable(messages::clear_chat(), None::<Vec<String>>, true);
                if let Some(d) = self.delegate() {
                    d.clear_chat();
                }
            }
        } else if at1(&command, "debug-log") {
            print_result = Some(self.get_debug_log());
        } else if at1(&command, "list-blacklist") {
            print_result = Some(list_output("Blacklist:\n", self.black_list()));
        } else if at1(&command, "list-whitelist") {
            print_result = Some(list_output("Whitelist:\n", self.white_list()));
        } else if at1(&command, "list-adminlist") {
            if !issuer_is_admin {
                incorrect_permissions = true;
            } else {
                print_result = Some(list_output("Adminlist:\n", self.admin_list()));
            }
        } else if at1(&command, "list-modlist") {
            if !issuer_is_admin {
                incorrect_permissions = true;
            } else {
                print_result = Some(list_output("Modlist:\n", self.mod_list()));
            }
        } else {
            handled = false;
        }

        // ---- Block D ----
        if !handled {
            let deny_target = |name: &str| -> bool {
                self.player_is_owner_with_alias(Some(name))
                    || (!issuer_is_admin && self.player_is_mod_with_alias(Some(name)))
            };
            if at1(&command, "kick ") {
                {
                    let name = arg(&command, 6);
                    if self.player_is_owner_with_alias(Some(&name))
                        || self.player_is_cloud_wide_admin_with_alias(Some(&name))
                    {
                        incorrect_permissions = true;
                    } else if !issuer_is_admin && self.player_is_mod_with_alias(Some(&name)) {
                        incorrect_permissions = true;
                    } else {
                        self.boot_player_named_was_ban(&name, false);
                    }
                }
            } else if at1(&command, "ban ") {
                {
                    let name = arg(&command, 5);
                    if deny_target(&name) {
                        incorrect_permissions = true;
                    } else {
                        self.modify_list_for_player_or_ip_is_added_list_type(&name, false, 1);
                        self.modify_list_for_player_or_ip_is_added_list_type(&name, false, 2);
                        self.modify_list_for_player_or_ip_is_added_list_type(&name, false, 3);
                        print_result =
                            self.modify_list_for_player_or_ip_is_added_list_type_ban_udid(&name, true, 0, true);
                    }
                }
            } else if at1(&command, "ban-no-device ") {
                {
                    let name = arg(&command, 15);
                    if deny_target(&name) {
                        incorrect_permissions = true;
                    } else {
                        self.modify_list_for_player_or_ip_is_added_list_type(&name, false, 1);
                        self.modify_list_for_player_or_ip_is_added_list_type(&name, false, 2);
                        self.modify_list_for_player_or_ip_is_added_list_type(&name, false, 3);
                        print_result =
                            self.modify_list_for_player_or_ip_is_added_list_type_ban_udid(&name, true, 0, false);
                    }
                }
            } else if at1(&command, "unban ") {
                {
                    let name = arg(&command, 7);
                    if deny_target(&name) {
                        incorrect_permissions = true;
                    } else {
                        print_result = self.modify_list_for_player_or_ip_is_added_list_type(&name, false, 0);
                    }
                }
            } else if at1(&command, "whitelist ") {
                {
                    let name = arg(&command, 11);
                    if deny_target(&name) {
                        incorrect_permissions = true;
                    } else {
                        let mut append_warning: Option<&str> = None;
                        let white_empty = self.white_list.borrow().as_ref().map_or(true, |l| l.is_empty());
                        if let (true, true, Some(s)) = (is_cloud, white_empty, server_match) {
                            if s.privacy() == 1 {
                                s.set_privacy(2);
                                crate::nslog!("CLOUD:SETTING:PRIVACY:SEARCHABLE");
                                append_warning = Some(
                                    "\nAs this server is now whitelisted, it can no longer be public. The privacy setting has been changed to SEARCHABLE.",
                                );
                            }
                        }
                        self.modify_list_for_player_or_ip_is_added_list_type(&name, false, 0);
                        let mut r = self.modify_list_for_player_or_ip_is_added_list_type(&name, true, 1);
                        if let (Some(w), Some(res)) = (append_warning, r.as_mut()) {
                            res.push_str(w);
                        }
                        print_result = r;
                    }
                }
            } else if at1(&command, "unwhitelist ") {
                {
                    let name = arg(&command, 13);
                    if deny_target(&name) {
                        incorrect_permissions = true;
                    } else {
                        print_result = self.modify_list_for_player_or_ip_is_added_list_type(&name, false, 1);
                    }
                }
            } else if at1(&command, "admin ") {
                {
                    let name = arg(&command, 7);
                    if !issuer_is_admin {
                        incorrect_permissions = true;
                    } else {
                        self.modify_list_for_player_or_ip_is_added_list_type(&name, false, 0);
                        print_result = self.modify_list_for_player_or_ip_is_added_list_type(&name, true, 2);
                    }
                }
            } else if at1(&command, "unadmin ") {
                {
                    let name = arg(&command, 9);
                    if !issuer_is_admin {
                        incorrect_permissions = true;
                    } else {
                        print_result = self.modify_list_for_player_or_ip_is_added_list_type(&name, false, 2);
                    }
                }
            } else if at1(&command, "mod ") {
                {
                    let name = arg(&command, 5);
                    if !issuer_is_admin {
                        incorrect_permissions = true;
                    } else {
                        self.modify_list_for_player_or_ip_is_added_list_type(&name, false, 0);
                        print_result = self.modify_list_for_player_or_ip_is_added_list_type(&name, true, 3);
                    }
                }
            } else if at1(&command, "unmod ") {
                {
                    let name = arg(&command, 7);
                    if !issuer_is_admin {
                        incorrect_permissions = true;
                    } else {
                        print_result = self.modify_list_for_player_or_ip_is_added_list_type(&name, false, 3);
                    }
                }
            } else if at1(&command, "repair") {
                if self.node.net_node_type() == NetNodeType::LocalServer {
                    print_result = Some("Sorry, /repair is not supported in local network games.".to_string());
                } else if !issuer_is_admin {
                    incorrect_permissions = true;
                } else {
                    let Some(client) = issue_client else {
                        // `[repairSet addObject:nil]` (observed on the console): the stdin handler is the catch site.
                        crate::foundation::exception::ns_raise("NSInvalidArgumentException", "Tried to add nil to set");
                    };
                    let mut new_enabled = false;
                    let contains = self.repair_set.borrow().contains(client);
                    if !contains {
                        self.repair_set.borrow_mut().insert(client.to_string());
                        new_enabled = true;
                        print_result = Some("Repair mode is now enabled. Tapping any non-solid tile will now remove any contents within. Tapping a solid block will remove it, unless it is an un-mined block. Disable repair mode by typing /repair again. Use with care!".to_string());
                    } else {
                        self.repair_set.borrow_mut().remove(client);
                        print_result = Some("Repair mode is now disabled.".to_string());
                    }
                    self.send_network_data_to_peers_reliable(
                        messages::toggle_repair_mode(new_enabled),
                        &[client.to_string()],
                        true,
                    );
                }
            } else if at1(&command, "reset-owner ") {
                if !is_cloud {
                    if !issuer_is_admin {
                        incorrect_permissions = true;
                    } else {
                        let name = arg(&command, 13);
                        let found = self
                            .reset_list
                            .borrow_mut()
                            .get_or_insert_with(Vec::new)
                            .iter()
                            .any(|e| safe_ci(Some(e), Some(&name)) == 0);
                        if !found {
                            self.reset_list.borrow_mut().get_or_insert_with(Vec::new).push(name.clone());
                            self.save_reset_list();
                            print_result = Some(format!(
                                "{} will be re-assigned to the next user who connects.",
                                nsstring::uppercase_string(&name)
                            ));
                        } else {
                            print_result =
                                Some(format!("{} has already been reset.", nsstring::uppercase_string(&name)));
                        }
                    }
                } else {
                    print_result = Some("Reset-owner not supported.".to_string());
                }
            }
        }

        // ---- reply step ----
        if incorrect_permissions {
            print_result = Some("Sorry, you do not have permission to issue that command.".to_string());
        }
        if let Some(text) = print_result {
            self.reply_to(&text, issue_client);
        }
    }

    /// `-getDebugLog`: version, connected clients, `/proc/self/statm` memory line, then the world's lines.
    pub fn get_debug_log(&self) -> String {
        let mut result = String::new();
        result.push_str("server version:1.7.1\n");
        result.push_str(&format!("connectedClients:{}\n", self.connected_clients.borrow().len()));
        for (size, resident, share) in read_off_memory_status() {
            let mb = |bytes: u64| -> f64 { (bytes as f32 / 1048576.0f32) as f64 };
            result.push_str(&format!(
                "memory usage: total:{:.2} MB resident:{:.2} MB share:{:.2} MB\n",
                mb(size),
                mb(resident),
                mb(share)
            ));
        }
        if let Some(w) = self.world() {
            w.append_debug_log(&mut result);
        }
        result
    }
}

/// `read_off_memory_status`: the first three numbers of `/proc/self/statm` (pages), as bytes; `None` on failure
/// (then `perror("/proc/self/statm")` goes to stderr). Hosts without procfs (macOS, Windows) report the same
/// three figures from the native process-memory query instead.
#[cfg(not(any(target_vendor = "apple", windows)))]
fn read_off_memory_status() -> Option<(u64, u64, u64)> {
    read_statm("/proc/self/statm")
}

/// The statm reader behind [`read_off_memory_status`] (the path is a parameter for the tests).
#[cfg(not(any(target_vendor = "apple", windows)))]
fn read_statm(path: &str) -> Option<(u64, u64, u64)> {
    match std::fs::read_to_string(path) {
        Err(e) => {
            // perror(3): "<prefix>: <strerror(errno)>" (no Rust "(os error N)" suffix).
            let msg = crate::foundation::sys::strerror(&e);
            eprintln!("/proc/self/statm: {}", msg);
            None
        }
        Ok(s) => {
            let nums: Vec<u64> = s.split_whitespace().take(7).filter_map(|t| t.parse::<u64>().ok()).collect();
            if nums.len() != 7 {
                eprintln!("/proc/self/statm: Success");
                return None;
            }
            let page = crate::foundation::sys::page_size();
            Some((nums[0].wrapping_mul(page), nums[1].wrapping_mul(page), nums[2].wrapping_mul(page)))
        }
    }
}

#[cfg(any(target_vendor = "apple", windows))]
fn read_off_memory_status() -> Option<(u64, u64, u64)> {
    crate::foundation::sys::process_memory()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::server::bhserver::test_support::*;

    #[test]
    fn help_constants_have_the_binary_lengths() {
        assert_eq!(MOD_COMMANDS.len(), 918);
        assert_eq!(ADMIN_EXTRA_COMMANDS.len(), 1155);
        assert_eq!(ADMIN_EXTRA_NON_LAN_COMMANDS.len(), 89);
        assert_eq!(ADMIN_EXTRA_NON_CLOUD_COMMANDS.len(), 225);
        assert_eq!(OWNER_EXTRA_CLOUD_COMMANDS.len(), 290);
        assert!(MOD_COMMANDS.starts_with("\n/HELP - display this message."));
    }

    fn console(h: &Harness, cmd: &str) -> Vec<String> {
        let before = h.d.messages().len();
        h.srv.handle_command_issue_client(cmd, None);
        h.d.messages()[before..].to_vec()
    }

    #[test]
    fn console_basic_commands() {
        let h = Harness::new("cmd_basic");
        assert_eq!(console(&h, "/players\n"), vec!["No players currently connected."]);
        assert_eq!(console(&h, "/pvp-on"), vec!["PVP was already enabled."]);
        assert_eq!(console(&h, "/pvp-off"), vec!["PVP is now disabled."]);
        assert_eq!(console(&h, "/pvp-off"), vec!["PVP was already disabled."]);
        assert_eq!(console(&h, "/pvp-on"), vec!["PVP is now enabled."]);
        assert_eq!(console(&h, "/unknowncommand"), Vec::<String>::new());
        assert_eq!(console(&h, "/load-lists"), vec!["Reloaded lists."]);
        let help = console(&h, "/HELPme");
        assert_eq!(help.len(), 1);
        let exp = format!(
            "{}{}{}{}",
            MOD_COMMANDS, ADMIN_EXTRA_COMMANDS, ADMIN_EXTRA_NON_LAN_COMMANDS, ADMIN_EXTRA_NON_CLOUD_COMMANDS
        );
        assert_eq!(help[0], exp);
        // trailing newline strip happens only for a single trailing \n
        assert_eq!(console(&h, "/players\n"), vec!["No players currently connected."]);
    }

    #[test]
    fn console_list_commands_and_ban() {
        let h = Harness::new("cmd_lists");
        assert_eq!(console(&h, "/list-blacklist"), vec!["Blacklist:\n"]);
        assert_eq!(console(&h, "/ban bob"), vec!["bob has been added to the blacklist"]);
        assert_eq!(console(&h, "/ban Carl"), vec!["carl has been added to the blacklist"]);
        assert_eq!(console(&h, "/list-blacklist"), vec!["Blacklist:\ncarl, bob, "]);
        assert_eq!(console(&h, "/unban bob"), vec!["bob has been removed from the blacklist"]);
        assert_eq!(console(&h, "/ban-no-device bob"), vec!["bob has been added to the blacklist"]);
        assert_eq!(console(&h, "/admin alice"), vec!["alice has been added to the adminlist"]);
        assert_eq!(console(&h, "/list-adminlist"), vec!["Adminlist:\nalice, , "]);
        assert_eq!(console(&h, "/mod bob"), vec!["bob has been added to the modlist"]);
        assert_eq!(console(&h, "/unmod bob"), vec!["bob has been removed from the modlist"]);
        assert_eq!(console(&h, "/clear-blacklist"), vec!["Blacklist cleared."]);
        assert_eq!(console(&h, "/list-blacklist"), vec!["Blacklist:\n"]);
        assert_eq!(console(&h, "/kick nobody"), Vec::<String>::new());
        assert_eq!(console(&h, "/whitelist zed"), vec!["zed has been added to the whitelist"]);
    }

    #[test]
    fn reset_owner_texts() {
        let h = Harness::new("cmd_reset");
        assert_eq!(console(&h, "/reset-owner ALICE"), vec!["ALICE will be re-assigned to the next user who connects."]);
        assert_eq!(console(&h, "/reset-owner ALICE"), vec!["ALICE has already been reset."]);
        let path =
            format!("{}/saves/{}/resetList.plist", crate::foundation::paths::appsup(), h.srv.save_id.clone().unwrap());
        assert!(std::fs::read_to_string(path).unwrap().contains("<string>alice</string>"));
        // Latin-1 only strings keep their accents; 8-bit uppercase touches ASCII only
        assert_eq!(
            console(&h, "/reset-owner \u{f1}ab"),
            vec!["\u{f1}AB will be re-assigned to the next user who connects."]
        );
    }

    #[test]
    fn permissions_for_network_issuers() {
        let h = Harness::new("cmd_perm");
        let a = h.join("Alice");
        h.srv.modify_list_for_player_or_ip_is_added_list_type("alice", true, 3); // mod only
        h.m.clear();
        h.srv.handle_command_issue_client("/pvp-off", Some(&a));
        let p = h.m.packets_to(&a);
        let d = crate::plist::helpers::property_list_from_data(&p[0][1..]).unwrap();
        assert_eq!(
            d.object_for_key("message").and_then(|v| v.as_str()),
            Some("Sorry, you do not have permission to issue that command.")
        );
        // the mod help has no admin lines
        h.m.clear();
        h.srv.handle_command_issue_client("/help", Some(&a));
        let d = crate::plist::helpers::property_list_from_data(&h.m.packets_to(&a)[0][1..]).unwrap();
        assert_eq!(d.object_for_key("message").and_then(|v| v.as_str()), Some(MOD_COMMANDS));
        // mod may /clear: broadcast 0x47
        h.m.clear();
        h.srv.handle_command_issue_client("/clear", Some(&a));
        assert!(h.m.packets_to(&a).iter().any(|p| p == &[0x47]));
        assert_eq!(h.d.cleared.get(), 1);
        // mod cannot kick an admin: owner/mod protections
        h.srv.modify_list_for_player_or_ip_is_added_list_type("bob", true, 3);
        h.m.clear();
        h.srv.handle_command_issue_client("/ban bob", Some(&a));
        let d = crate::plist::helpers::property_list_from_data(&h.m.packets_to(&a)[0][1..]).unwrap();
        assert_eq!(
            d.object_for_key("message").and_then(|v| v.as_str()),
            Some("Sorry, you do not have permission to issue that command.")
        );
    }

    #[test]
    fn kick_and_players() {
        let h = Harness::new("cmd_kick");
        let p = h.join("Pam");
        let out = console(&h, "/players");
        assert_eq!(out, vec![format!("There are 1 players connected:\nPam | 127.0.0.1 | {}", p)]);
        h.m.clear();
        h.srv.handle_command_issue_client("/kick pam", None);
        assert!(h.m.packets_to(&p).iter().any(|x| x == &[0x26, 0x0b]));
        h.run_delayed();
        assert_eq!(h.m.disconnected.borrow().as_slice(), &[p.clone()]);
        assert!(h.w.calls.borrow().contains(&format!("client_disconnected {} true", p)));
    }

    #[test]
    fn password_commands() {
        let h = Harness::new("cmd_pw");
        // the mock match is not a BHNetServerMatch: no reply at all
        assert_eq!(console(&h, "/set-password abc"), Vec::<String>::new());
        assert_eq!(h.w.password.borrow().clone(), None);
    }

    #[test]
    fn repair_toggle_and_console_raise() {
        let h = Harness::new("cmd_repair");
        let a = h.join("Alice");
        h.srv.modify_list_for_player_or_ip_is_added_list_type("alice", true, 2);
        h.m.clear();
        h.srv.handle_command_issue_client("/repair", Some(&a));
        let pk = h.m.packets_to(&a);
        assert_eq!(pk[0], vec![0x4a, 1]);
        assert!(h.srv.repair_set.borrow().contains(&a));
        // the dispatcher's 0x4b now reaches the world
        let mut pkt = vec![0x4b];
        pkt.extend(5u64.to_le_bytes());
        h.send(&a, &pkt);
        assert!(h.w.calls.borrow().contains(&"repair 5 5".to_string()));
        h.m.clear();
        h.srv.handle_command_issue_client("/repair", Some(&a));
        assert_eq!(h.m.packets_to(&a)[0], vec![0x4a, 0]);
        // console: NSInvalidArgumentException "Tried to add nil to set"
        let r = crate::foundation::exception::catch_ns(|| h.srv.handle_command_issue_client("/repair", None));
        let e = r.unwrap_err();
        assert_eq!(e.name, "NSInvalidArgumentException");
        assert_eq!(e.reason.as_deref(), Some("Tried to add nil to set"));
    }

    #[test]
    fn stop_reaches_delegate_and_debug_log() {
        let h = Harness::new("cmd_stop");
        assert_eq!(console(&h, "/stop"), Vec::<String>::new());
        assert_eq!(h.d.stopped.get(), 1);
        let log = console(&h, "/debug-log");
        assert!(log[0].starts_with("server version:1.7.1\nconnectedClients:0\nmemory usage: total:"), "{}", log[0]);
        assert!(log[0].ends_with("loaded macro blocks:7\n"));
    }
}

#[cfg(test)]
#[path = "commands_tests.rs"]
mod command_matrix;
