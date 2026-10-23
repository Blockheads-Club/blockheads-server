//! The command matrix: who may issue what, on the console, as a network client and over a real cloud match.

use super::*;
use crate::foundation::exception::catch_ns;
use crate::foundation::md5::string_from_md5;
use crate::net::bhmatch::{BhMatch, PlayerInfo};
use crate::plist::helpers::property_list_from_data;
use crate::server::bhserver::test_support::*;
use std::cell::RefCell;

const DENIED: &str = "Sorry, you do not have permission to issue that command.";

/// What the server said to `id` in chat.
fn said(h: &Harness, id: &str) -> Vec<String> {
    h.m.packets_to(id)
        .iter()
        .filter(|p| p[0] == 0x24)
        .map(|p| {
            let d = property_list_from_data(&p[1..]).unwrap();
            d.object_for_key("message").and_then(|v| v.as_str()).unwrap().to_string()
        })
        .collect()
}

/// `from` issues `cmd`; the chat replies it got.
fn run(h: &Harness, from: &str, cmd: &str) -> Vec<String> {
    h.m.clear();
    h.srv.handle_command_issue_client(cmd, Some(from));
    said(h, from)
}

fn console(h: &Harness, cmd: &str) -> Vec<String> {
    let before = h.d.messages().len();
    h.srv.handle_command_issue_client(cmd, None);
    h.d.messages()[before..].to_vec()
}

fn denied(r: Vec<String>) -> bool {
    r == [DENIED]
}

/// A joined player with the given rank: 0 plain, 1 mod, 2 admin.
fn player(h: &Harness, alias: &str, rank: u8) -> String {
    let id = h.join(alias);
    for (min, list) in [(1, 3), (2, 2)] {
        if rank >= min {
            h.srv.modify_list_for_player_or_ip_is_added_list_type(&alias.to_lowercase(), true, list);
        }
    }
    id
}

#[test]
fn permission_denials_for_every_gated_command() {
    let h = Harness::new("cm_denied");
    let plain = player(&h, "Pat", 0);
    let moderator = player(&h, "Mo", 1);
    for cmd in [
        "/stop",
        "/pvp-on",
        "/pvp-off",
        "/load-lists",
        "/clear-blacklist",
        "/clear-whitelist",
        "/clear-adminlist",
        "/clear-modlist",
        "/list-adminlist",
        "/list-modlist",
        "/admin pat",
        "/unadmin pat",
        "/mod pat",
        "/unmod pat",
        "/repair",
        "/reset-owner pat",
    ] {
        assert!(denied(run(&h, &moderator, cmd)), "{cmd}");
    }
    assert!(denied(run(&h, &plain, "/clear")));
    // A mod may use the list commands and the ones aimed at plain players, but not at other mods.
    assert!(!denied(run(&h, &moderator, "/ban pat")));
    assert!(!denied(run(&h, &moderator, "/unban pat")));
    assert!(!denied(run(&h, &moderator, "/whitelist pat")));
    assert!(!denied(run(&h, &moderator, "/unwhitelist pat")));
    assert!(!denied(run(&h, &moderator, "/ban-no-device pat")));
    assert!(!denied(run(&h, &moderator, "/clear")));
    let other_mod = player(&h, "Mo2", 1);
    let _ = other_mod;
    for cmd in ["/ban mo2", "/ban-no-device mo2", "/unban mo2", "/whitelist mo2", "/unwhitelist mo2", "/kick mo2"] {
        assert!(denied(run(&h, &moderator, cmd)), "{cmd}");
    }
}

#[test]
fn admins_run_the_admin_commands() {
    let h = Harness::new("cm_admin");
    let admin = player(&h, "Ann", 2);
    let target = player(&h, "Tom", 0);
    assert_eq!(run(&h, &admin, "/pvp-off"), ["PVP is now disabled."]);
    assert_eq!(run(&h, &admin, "/pvp-on"), ["PVP is now enabled."]);
    assert_eq!(run(&h, &admin, "/load-lists"), ["Reloaded lists."]);
    // Reloading re-reads the lists from disk, which still has Ann in the adminlist.
    assert_eq!(run(&h, &admin, "/clear-whitelist"), ["Whitelist cleared."]);
    assert_eq!(run(&h, &admin, "/clear-modlist"), ["Modlist cleared."]);
    assert_eq!(run(&h, &admin, "/mod tom"), ["tom has been added to the modlist"]);
    assert_eq!(run(&h, &admin, "/list-modlist"), ["Modlist:\ntom, , "]);
    assert_eq!(run(&h, &admin, "/unmod tom"), ["tom has been removed from the modlist"]);
    assert_eq!(run(&h, &admin, "/admin tom"), ["tom has been added to the adminlist"]);
    assert_eq!(run(&h, &admin, "/unadmin tom"), ["tom has been removed from the adminlist"]);
    assert_eq!(run(&h, &admin, "/list-whitelist"), ["Whitelist:\n"]);
    assert_eq!(run(&h, &admin, "/whitelist tom"), ["tom has been added to the whitelist"]);
    assert_eq!(run(&h, &admin, "/unwhitelist tom"), ["tom has been removed from the whitelist"]);
    assert_eq!(run(&h, &admin, "/clear-blacklist"), ["Blacklist cleared."]);
    // The clear commands for a rank-less console-style call, the plain /clear, and the debug log.
    assert!(run(&h, &admin, "/clear").is_empty());
    assert!(run(&h, &admin, "/debug-log")[0].starts_with("server version:1.7.1"));
    // Kicks: an admin may kick a mod but nobody can kick an owner or a cloud-wide admin.
    h.srv.modify_list_for_player_or_ip_is_added_list_type("tom", true, 3);
    h.srv.cloud_wide_admin_list.replace(Some(vec!["boss".to_string()]));
    *h.m.owner.borrow_mut() = Some("Olly".to_string());
    assert!(denied(run(&h, &admin, "/kick olly")));
    assert!(denied(run(&h, &admin, "/kick boss")));
    assert!(!denied(run(&h, &admin, "/kick tom")));
    assert!(h.m.packets_to(&target).contains(&vec![0x26, 0x0b]));
    // Reset-owner is refused for a non-admin and answered for an admin.
    assert_eq!(run(&h, &admin, "/reset-owner sam"), ["SAM will be re-assigned to the next user who connects."]);
    // A mod listed as a target of an admin's ban is fine; an owner target is not.
    assert!(denied(run(&h, &admin, "/ban olly")));
    assert!(denied(run(&h, &admin, "/whitelist olly")));
    // Clearing the adminlist ends the admin's own rank.
    assert_eq!(run(&h, &admin, "/clear-adminlist"), ["Adminlist cleared."]);
}

#[test]
fn help_texts_by_rank_and_game_type() {
    let h = Harness::new("cm_help");
    let admin = player(&h, "Ann", 2);
    let plain = player(&h, "Pat", 0);
    let text = |from: &str| run(&h, from, "/help").remove(0);
    assert_eq!(text(&plain), MOD_COMMANDS);
    assert_eq!(
        text(&admin),
        format!("{MOD_COMMANDS}{ADMIN_EXTRA_COMMANDS}{ADMIN_EXTRA_NON_LAN_COMMANDS}{ADMIN_EXTRA_NON_CLOUD_COMMANDS}")
    );
    let local = Harness::local_server("cm_help_local");
    let la = player(&local, "Ann", 2);
    assert_eq!(
        run(&local, &la, "/help").remove(0),
        format!("{MOD_COMMANDS}{ADMIN_EXTRA_COMMANDS}{ADMIN_EXTRA_NON_CLOUD_COMMANDS}")
    );
    assert_eq!(run(&local, &la, "/repair"), ["Sorry, /repair is not supported in local network games."]);
}

#[test]
fn the_console_lists_players_with_hidden_cloud_admin_addresses() {
    let h = Harness::new("cm_players");
    let a = h.join("Ann");
    let b = h.join("Bea");
    h.srv.cloud_wide_admin_list.replace(Some(vec!["ann".to_string()]));
    let out = console(&h, "/players").remove(0);
    assert!(out.starts_with("There are 2 players connected:"), "{out}");
    assert!(
        out.contains(&format!("Ann | IP hidden | {a}")) && out.contains(&format!("Bea | 127.0.0.1 | {b}")),
        "{out}"
    );
    // A connected player nobody remembers is left out of the list.
    h.srv.recent_players.borrow_mut().clear();
    assert_eq!(console(&h, "/players"), ["There are 2 players connected:"]);
}

#[test]
fn the_mod_host_can_consume_a_command() {
    let hooks = StubHooks::install();
    struct Guard;
    impl Drop for Guard {
        fn drop(&mut self) {
            StubHooks::uninstall();
        }
    }
    let _guard = Guard;
    let h = Harness::new("cm_hooks");
    assert!(console(&h, "/pvp-off").is_empty());
    assert!(hooks.saw("command None /pvp-off"));
    hooks.consumes.set(false);
    assert_eq!(console(&h, "/pvp-off"), ["PVP is now disabled."]);
}

#[test]
fn repair_toggles_and_refuses_the_console() {
    let h = Harness::new("cm_repair2");
    let admin = player(&h, "Ann", 2);
    assert!(run(&h, &admin, "/repair")[0].starts_with("Repair mode is now enabled."));
    assert!(h.m.packets_to(&admin).contains(&vec![0x4a, 1]));
    assert_eq!(run(&h, &admin, "/repair"), ["Repair mode is now disabled."]);
    assert!(catch_ns(|| h.srv.handle_command_issue_client("/repair", None)).is_err());
}

#[test]
fn reset_owner_in_a_cloud_match_is_not_supported() {
    let h = RealMatchHarness::new("cm_cloud_reset", Some("SALT"), Some("Boss"), Some("public"), 100.0);
    let before = h.d.messages().len();
    h.srv.handle_command_issue_client("/reset-owner x", None);
    assert_eq!(h.d.messages()[before..], ["Reset-owner not supported."]);
}

#[test]
fn console_owner_commands_over_a_real_match() {
    let h = RealMatchHarness::new("cm_real", Some("SALT"), Some("Boss"), Some("public"), 100.0);
    let say = |cmd: &str| {
        let before = h.d.messages().len();
        h.srv.handle_command_issue_client(cmd, None);
        h.d.messages()[before..].to_vec()
    };
    assert!(say("/set-credit 7").is_empty());
    assert_eq!(h.srv.credit(), 7.0);
    say("/set-credit");
    assert!(say("/set-owner Newbie").is_empty());
    assert_eq!(h.m.owner_name().as_deref(), Some("newbie"));
    say("/set-owner ");
    say("/set-owner");
    assert!(say("/remove-owner").is_empty());
    assert_eq!(h.m.owner_name(), None);
    assert!(say("/set-salt abc").is_empty());
    say("/set-salt ");
    say("/set-salt");
    // Privacy: bad values are ignored, good ones are acknowledged (and logged for the cloud).
    assert_eq!(say("/set-privacy private"), ["Privacy setting changed to private."]);
    assert_eq!(h.srv.privacy_string().as_deref(), Some("private"));
    assert!(say("/set-privacy sideways").is_empty());
    assert!(say("/set-privacy ").is_empty());
    *h.srv.white_list.borrow_mut() = Some(vec!["somebody".to_string()]);
    assert!(say("/set-privacy public")[0].starts_with("Error: This world is whitelisted"));
    *h.srv.white_list.borrow_mut() = None;
    // Passwords.
    assert_eq!(say("/set-password sesame"), ["Password set."]);
    assert_eq!(h.w.password.borrow().as_deref(), Some("sesame"));
    assert_eq!(
        say("/set-password this-one-is-far-too-long"),
        ["Error setting password. Sorry, but passwords must not be longer than 16 characters."]
    );
    assert!(say("/set-password ").is_empty());
    assert_eq!(say("/remove-password"), ["Password removed."]);
    assert_eq!(say("/remove-password"), ["There was already no password set."]);
    // Cloud games log their settings and the owner help shows the cloud lines.
    assert_eq!(say("/pvp-off"), ["PVP is now disabled."]);
    assert_eq!(say("/pvp-on"), ["PVP is now enabled."]);
    assert!(say("/help")[0].ends_with(OWNER_EXTRA_CLOUD_COMMANDS));
    // The first whitelisting of a public cloud world makes it searchable and says so.
    h.m.set_privacy(1);
    *h.srv.white_list.borrow_mut() = Some(vec![]);
    let out = say("/whitelist someone");
    assert!(
        out[0].ends_with("it can no longer be public. The privacy setting has been changed to SEARCHABLE."),
        "{out:?}"
    );
    assert_eq!(h.srv.privacy_string().as_deref(), Some("searchable"));
    // An owner at the console is not subject to any of the checks.
    assert!(say("/unwhitelist someone")[0].contains("removed from the whitelist"));
}

#[test]
fn a_player_owner_of_a_real_match_uses_the_owner_commands() {
    let h = RealMatchHarness::new("cm_real_owner", Some("SALT"), Some("Boss"), Some("public"), 100.0);
    let id = string_from_md5("boss");
    let mut info = new_mutable_dict();
    info.insert("alias".to_string(), PlistValue::string("Boss"));
    info.insert("playerID".to_string(), PlistValue::string(id.as_str()));
    let info: PlayerInfo = Rc::new(RefCell::new(info));
    h.srv.node.info_arrived_for_player(&info, &id);
    h.srv.connected_clients.borrow_mut().push(id.clone());
    h.srv.handle_command_issue_client("/set-privacy searchable", Some(&id));
    assert_eq!(h.srv.privacy_string().as_deref(), Some("searchable"));
    h.srv.handle_command_issue_client("/set-password pw", Some(&id));
    h.srv.handle_command_issue_client("/remove-password", Some(&id));
    // The console-only commands do nothing for a player.
    h.srv.handle_command_issue_client("/set-credit 99", Some(&id));
    assert_ne!(h.srv.credit(), 99.0);
}

#[test]
fn statm_failures_are_reported_and_swallowed() {
    assert!(read_statm("/definitely/not/here").is_none());
    let path = std::env::temp_dir().join(format!("bh_statm_{}", std::process::id()));
    std::fs::write(&path, "1 2 3").unwrap();
    assert!(read_statm(path.to_str().unwrap()).is_none());
    std::fs::write(&path, "1 2 3 4 5 6 7").unwrap();
    let (size, resident, share) = read_statm(path.to_str().unwrap()).unwrap();
    let page = crate::foundation::sys::page_size();
    assert_eq!((size, resident, share), (page, 2 * page, 3 * page));
    let _ = std::fs::remove_file(path);
    assert!(catch_ns(|| ()).is_ok());
}

#[test]
fn odd_command_strings_are_ignored_or_answered_as_typed() {
    let h = Harness::new("cm_odd");
    assert!(console(&h, "\n").is_empty()); // a lone newline is not stripped and matches nothing
    assert_eq!(console(&h, "/players\nmore"), ["No players currently connected."]);
    let before = h.d.messages().len();
    h.srv.handle_command_issue_client(None::<&str>, None); // a chat line without text from a mod
    assert_eq!(h.d.messages().len(), before);
}

#[test]
fn console_owner_commands_find_no_server_match_on_a_mock() {
    let h = Harness::new("cm_mockowner");
    for cmd in
        ["/set-credit 5", "/set-owner x", "/set-salt s", "/remove-owner", "/set-privacy private", "/remove-password"]
    {
        assert!(console(&h, cmd).is_empty(), "{cmd}");
    }
}

#[test]
fn whitelisting_leaves_a_searchable_cloud_world_alone() {
    let h = RealMatchHarness::new("cm_white2", Some("SALT"), Some("Boss"), Some("searchable"), 100.0);
    let before = h.d.messages().len();
    h.srv.handle_command_issue_client("/whitelist someone", None);
    assert_eq!(h.d.messages()[before..], ["someone has been added to the whitelist"]);
    // Without a world the owner commands still answer but change nothing there.
    *h.srv.world.borrow_mut() = None;
    for cmd in ["/set-password pw", "/remove-password"] {
        h.srv.handle_command_issue_client(cmd, None);
    }
    assert_eq!(h.d.messages().len(), before + 3);
}
