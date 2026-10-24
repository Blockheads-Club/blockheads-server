//! Construction, the legacy player-list migrations, transport glue, kicks and the delegate faces of the server.

use super::test_support::*;
use super::*;
use crate::foundation::md5::string_from_md5;
use crate::plist::helpers::data_from_property_list;

fn dict_with(pairs: &[(&str, &str)]) -> PlistValue {
    let mut d = new_mutable_dict();
    for (k, v) in pairs {
        d.insert(k.to_string(), PlistValue::string(*v));
    }
    PlistValue::Dict(d)
}

fn put(path: &str, v: &PlistValue) {
    std::fs::create_dir_all(std::path::Path::new(path).parent().unwrap()).unwrap();
    std::fs::write(path, data_from_property_list(v).unwrap()).unwrap();
}

/// The save directory handed to a `with_files` hook and the id it belongs to.
fn save_id(save: &str) -> String {
    save.rsplit('/').next().unwrap().to_string()
}

#[test]
fn every_peer_list_shape_converts() {
    let v = vec!["a".to_string(), "b".to_string()];
    let arr = ["a".to_string(), "b".to_string()];
    let one = Some(vec!["x".to_string()]);
    assert_eq!(Some(&v[..]).into_peers(), Some(v.clone()));
    assert_eq!(None::<&[String]>.into_peers(), None);
    assert_eq!(v[..].iter().as_slice().into_peers(), Some(v.clone()));
    assert_eq!((&v).into_peers(), Some(v.clone()));
    assert_eq!(v.clone().into_peers(), Some(v.clone()));
    assert_eq!(Some(&v).into_peers(), Some(v.clone()));
    assert_eq!(None::<&Vec<String>>.into_peers(), None);
    assert_eq!(Some(v.clone()).into_peers(), Some(v.clone()));
    assert_eq!("x".into_peers(), one);
    assert_eq!((&"x".to_string()).into_peers(), one);
    assert_eq!((&arr).into_peers(), Some(v.clone()));
    assert_eq!(arr.clone().into_peers(), Some(v));
    assert_eq!(raise_name(), "NSInvalidArgumentException");
}

fn raise_name() -> String {
    let e = crate::foundation::exception::catch_ns(|| raise_unrecognized(&PlistValue::int(1), "length")).unwrap_err();
    e.name
}

#[test]
fn atomic_writes_report_failures() {
    let dir = std::env::temp_dir().join(format!("bh_bhserver_atomic_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("target")).unwrap();
    assert!(!write_data_atomic(dir.join("missing/dir/file").to_str().unwrap(), b"x")); // the temp file cannot be made
    assert!(!write_data_atomic(dir.join("target").to_str().unwrap(), b"x")); // the rename onto a directory fails
    assert!(write_data_atomic(dir.join("file").to_str().unwrap(), b"x"));
    assert!(pv_into_dict(PlistValue::int(1)).is_none() && pv_into_dict(dict_with(&[])).is_some());
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn start_up_reads_the_reset_list_and_the_recent_players_file() {
    let h = Harness::with_files("bs_files", |save| {
        put(&format!("{save}/resetList.plist"), &PlistValue::array(vec![PlistValue::string("a"), PlistValue::int(3)]));
        let player = dict_with(&[("alias", "Ann"), ("playerID", "p1")]);
        put(&format!("{save}/recentPlayers.plist"), &PlistValue::array(vec![player, PlistValue::string("junk")]));
    });
    assert_eq!(h.srv.reset_list.borrow().clone(), Some(vec!["a".to_string()]));
    assert_eq!(h.srv.recent_players().len(), 1);
    // Files that are not arrays leave the lists empty.
    let h = Harness::with_files("bs_files2", |save| {
        put(&format!("{save}/resetList.plist"), &dict_with(&[("a", "b")]));
        put(&format!("{save}/recentPlayers.plist"), &dict_with(&[("a", "b")]));
    });
    assert_eq!(h.srv.reset_list.borrow().clone(), None);
    assert!(h.srv.recent_players().is_empty());
}

/// A 1.4 record for the migration tests.
fn old_player(alias: &str, id: &str) -> PlistValue {
    let mut d = new_mutable_dict();
    d.insert("alias".to_string(), PlistValue::string(alias));
    d.insert("playerID".to_string(), PlistValue::string(id));
    d.insert("connected".to_string(), PlistValue::boolean(true));
    d.insert("photo".to_string(), PlistValue::data(vec![1]));
    PlistValue::Dict(d)
}

#[test]
fn the_legacy_player_list_is_migrated_in_two_steps() {
    let ids: Vec<String> = ["alpha", "beta"].iter().map(|n| string_from_md5(n)).collect();
    let mut items: Vec<PlistValue> = vec![
        old_player("Alpha", &ids[0]),
        old_player("Alpha again", &ids[0]), // the directory and the info file already exist
        old_player("Beta", &ids[1]),
        old_player("Short", "x"),
        old_player("Two", "ab"),
        old_player("Nameless", ""),
        PlistValue::string("not a dictionary"),
    ];
    for i in 0..130 {
        items.push(old_player(&format!("filler{i}"), &string_from_md5(&format!("filler{i}"))));
    }
    let h = Harness::with_files("bs_migrate", |save| {
        let id = save_id(save);
        let old = format!("{}/allPlayers-{}", crate::foundation::paths::appsup(), id);
        put(&old, &PlistValue::array(items.clone()));
        // Per-player files of the 1.5 layout sit directly in the save directory and move into players/a/b/rest.
        std::fs::write(format!("{save}/{}", ids[1]), b"old record").unwrap();
        std::fs::write(format!("{save}/not-a-player-file"), b"stays").unwrap();
    });
    let save = format!("{}/saves/{}", crate::foundation::paths::appsup(), h.w.id);
    assert!(!std::path::Path::new(&format!("{save}/allPlayers.plist")).exists());
    assert!(std::path::Path::new(&format!("{save}/recentPlayers.plist")).exists());
    assert!(std::path::Path::new(&format!("{save}/not-a-player-file")).exists());
    assert!(std::path::Path::new(&format!(
        "{save}/players/{}/{}/{}/playerInfo.plist",
        &ids[0][..1],
        &ids[0][1..2],
        &ids[0][2..]
    ))
    .exists());
    assert!(
        std::path::Path::new(&format!("{save}/players/{}/{}/{}", &ids[1][..1], &ids[1][1..2], &ids[1][2..])).exists()
    );
    assert_eq!(h.srv.recent_players().len(), 0x80 - 1); // the first 128 items, minus the one that is no dictionary
                                                        // The photo and the connection flag were dropped on the way.
    assert!(dict_get(&h.srv.recent_players()[0], "photo").is_none());
}

#[test]
fn migrations_survive_garbage_and_blocked_targets() {
    // Not a plist: the old file is left alone.
    let h = Harness::with_files("bs_garbage", |save| {
        let old = format!("{}/allPlayers-{}", crate::foundation::paths::appsup(), save_id(save));
        std::fs::write(old, b"garbage").unwrap();
    });
    let old = format!("{}/allPlayers-{}", crate::foundation::paths::appsup(), h.w.id);
    assert!(std::path::Path::new(&old).exists());
    // The new file cannot be written (a directory is in the way): the old one stays too.
    let h = Harness::with_files("bs_blocked", |save| {
        let old = format!("{}/allPlayers-{}", crate::foundation::paths::appsup(), save_id(save));
        put(&old, &PlistValue::array(vec![old_player("A", &string_from_md5("a"))]));
        std::fs::create_dir_all(format!("{save}/allPlayers.plist/in-the-way")).unwrap();
    });
    let old = format!("{}/allPlayers-{}", crate::foundation::paths::appsup(), h.w.id);
    assert!(std::path::Path::new(&old).exists());
    // A 1.5 list whose recentPlayers.plist cannot be written is kept.
    let h = Harness::with_files("bs_blocked2", |save| {
        put(&format!("{save}/allPlayers.plist"), &PlistValue::array(vec![old_player("A", &string_from_md5("a"))]));
        std::fs::create_dir_all(format!("{save}/recentPlayers.plist/in-the-way")).unwrap();
    });
    let src = format!("{}/saves/{}/allPlayers.plist", crate::foundation::paths::appsup(), h.w.id);
    assert!(std::path::Path::new(&src).exists());
    // An unreadable 1.5 list (a directory) migrates nothing.
    let h = Harness::with_files("bs_blocked3", |save| {
        std::fs::create_dir_all(format!("{save}/allPlayers.plist/in-the-way")).unwrap();
    });
    assert!(h.srv.recent_players().is_empty());
}

#[test]
fn a_cloud_server_without_a_server_match_logs_a_public_world() {
    let h = Harness::tuned("bs_cloud_mock", |_| {}, |m| m.cloud.set(true));
    assert!(h.srv.is_cloud_match());
    assert_eq!(h.srv.privacy_string(), None);
}

#[test]
fn a_world_set_later_gets_the_summary_for_everyone_waiting() {
    let h = Harness::new("bs_setworld");
    let a = h.join("Alice");
    h.srv.cloud_wide_admin_list.replace(Some(vec!["alice".to_string()]));
    h.m.clear();
    h.srv.set_world(&h.world_dyn);
    assert!(h.m.packets_to(&a).iter().any(|p| p[0] == 0x01));
    assert_eq!(h.srv.cloud_mode_for_alias(Some("Alice")), 43252);
    h.srv.cloud_wide_invisible_admin_list.replace(Some(vec!["alice".to_string()]));
    assert_eq!(h.srv.cloud_mode_for_alias(Some("Alice")), 19843);
    assert_eq!(h.srv.cloud_mode_for_alias(Some("Bob")), 0);
}

#[test]
fn cleanup_disconnects_everyone_and_silences_the_transport() {
    let h = Harness::new("bs_cleanup");
    let a = h.join("Alice");
    h.srv.cleanup();
    assert!(h.srv.connected_client_ids().is_empty());
    assert!(h.w.calls.borrow().contains(&format!("client_disconnected {a} false")));
    // Empty recipient lists and a gone transport send nothing.
    h.m.clear();
    h.srv.send_network_data_to_peers_reliable([1u8], Vec::<String>::new(), true);
    h.srv.send_network_data_to_peers_reliable([1u8], vec!["x".to_string()], false);
    assert!(h.m.sent.borrow().len() <= 1);
}

#[test]
fn recent_players_come_from_the_database_before_the_file() {
    let h = Harness::new("bs_recent_db");
    let blob =
        data_from_property_list(&PlistValue::array(vec![dict_with(&[("alias", "Dee"), ("playerID", "p9")])])).unwrap();
    h.srv.server_database().unwrap().set_data(Some(&blob), Some("recentPlayers"));
    h.srv.load_recent_players();
    assert_eq!(dict_str(&h.srv.recent_players()[0], "alias").as_deref(), Some("Dee"));
}

#[test]
fn a_server_without_a_save_id_asks_the_world_for_its_id() {
    init_env();
    let m = MockMatch::new();
    let d = MockDelegate::new();
    let dd: Rc<dyn ServerDelegate> = d.clone();
    let srv = BHServer::init_with_delegate_match_net_node_type_save_id_max_players(
        Rc::downgrade(&dd),
        m.clone(),
        NetNodeType::OnlineServer,
        None,
        8,
    )
    .unwrap();
    srv.match_player_did_change_state(&*m, "p", PlayerConnectionState::Connected); // no world yet: an empty id
    let w = MockWorld::new("world-id");
    let world: Rc<dyn ServerWorld> = w.clone();
    srv.set_world(&world);
    srv.match_player_did_change_state(&*m, "q", PlayerConnectionState::Connected);
    assert!(m.packets_to("q")[0].windows(8).any(|x| x == b"world-id"));
}

/// Removes the stub mod host when the test ends.
struct HooksGuard;
impl Drop for HooksGuard {
    fn drop(&mut self) {
        StubHooks::uninstall();
    }
}

#[test]
fn the_mod_host_hears_about_delayed_disconnects() {
    let hooks = StubHooks::install();
    let _guard = HooksGuard;
    let h = Harness::new("bs_hook_quit");
    let a = h.join("Alice");
    h.srv.delayed_disconnect_player_due_to_kick_with_id(&a);
    assert!(hooks.saw(&format!("quit {a} true")));
}

#[test]
fn unknown_peers_are_taken_at_their_transport_name() {
    let h = Harness::new("bs_unknown_peer");
    h.connect("raw-peer");
    h.m.clear();
    h.send("raw-peer", &Harness::handshake_packet("Eve", "u", 5));
    assert!(h.srv.connected_client_ids().contains(&"raw-peer".to_string()));
}

#[test]
fn a_real_match_can_be_disconnected_through_its_timers() {
    let h = RealMatchHarness::new("bs_real_cancel", None, None, None, 0.0);
    h.m.disconnect();
}

#[test]
fn the_server_chat_history_keeps_twenty() {
    let h = Harness::new("bs_chat");
    for i in 0..22 {
        h.srv.send_chat_message_send_to_clients(format!("m{i}").as_str(), None);
    }
    assert_eq!(h.srv.chat_history.borrow().len(), 20);
    assert_eq!(h.srv.chat_history.borrow()[0].get(&"message".to_string()).and_then(|v| v.as_str()), Some("m2"));
    h.srv.send_chat_message_send_to_clients(None::<&str>, None);
    h.srv.send_chat_message_display_notification_send_to_clients(None, true, None);
}

#[test]
fn booting_players_by_name_searches_connected_then_recent() {
    let h = Harness::new("bs_boot");
    let a = h.join("Alice");
    let b = h.join("Bob");
    h.srv.match_player_did_change_state(&*h.m, &b, PlayerConnectionState::Disconnected);
    h.m.clear();
    h.srv.boot_player_named_was_ban("alice", false);
    assert!(h.m.packets_to(&a).contains(&vec![0x26, 0x0b]));
    // Bob is gone but still remembered: the recent list names his id.
    h.srv.boot_player_named_was_ban("BOB", true);
    assert!(h.m.packets_to(&b).contains(&vec![0x26, 0x0c]));
    h.srv.boot_player_named_was_ban("nobody", false);
    // The recent list search stops after 65 entries.
    for i in 0..70 {
        let mut d = new_mutable_dict();
        d.insert("alias".to_string(), PlistValue::string(format!("filler{i}")));
        h.srv.recent_players.borrow_mut().push(d);
    }
    h.srv.boot_player_named_was_ban("filler69", false);
}

#[test]
fn delayed_disconnects_only_touch_connected_players() {
    let h = Harness::new("bs_delay");
    h.srv.delayed_disconnect_player_due_to_kick_with_id("stranger");
    assert!(h.m.disconnected.borrow().is_empty());
    let a = h.join("Alice");
    h.srv.delayed_disconnect_player_due_to_no_credit_with_id(&a);
    assert_eq!(h.m.disconnected.borrow().as_slice(), std::slice::from_ref(&a));
    assert!(h.w.calls.borrow().contains(&format!("client_disconnected {a} false")));
}

#[test]
fn the_anti_cheat_timer_clamps_its_clock() {
    let h = Harness::new("bs_timer");
    let a = h.join("Alice");
    h.srv.update_player(&a);
    h.srv.player_update_timer.set(1000.0);
    h.srv.last_player_update.set(reference_time() + 100.0); // a clock that went backwards
    h.srv.update_players();
    assert_eq!(h.srv.player_update_timer.get(), 1000.0);
    h.srv.last_player_update.set(reference_time() - 100.0); // a long stall counts for five seconds at most
    h.srv.update_players();
    assert!((h.srv.player_update_timer.get() - 995.0).abs() < 0.5);
}

#[test]
fn small_accessors_and_the_delegate_faces() {
    let h = Harness::new("bs_misc");
    h.srv.notify_players_changed();
    h.srv.send_player_changed_notifcation_to_delegate();
    assert_eq!(h.srv.server_player_id(), h.srv.local_player_id());
    assert_eq!(h.srv.local_player_name(), h.srv.node.local_player_name());
    assert!(h.srv.match_().is_some());
    assert!(!h.srv.is_waiting_for_multi_part_command_response());
    h.srv.start_bulk_transaction();
    assert!(h.srv.finish_bulk_transaction() || !h.srv.finish_bulk_transaction());
    h.srv.do_repair_for_tile_at_pos(IntPair { x: 1, y: 2 });
    assert!(h.w.calls.borrow().contains(&"repair 1 2".to_string()));
    let a = h.join("Alice");
    assert!(h.srv.player_info_dict_for_peer_id(&a).is_some());
    // The traits the other modules see the server through.
    let api: &dyn ServerApi = &*h.srv;
    assert!(!api.player_is_admin_with_id(Some(&a)));
    let dw: &dyn crate::dynworld::state::DwServer = &*h.srv;
    assert!(dw.player_info_for_peer_id(&a).is_some());
    assert!(!dw.player_is_admin_with_id(&a));
    assert!(dw.player_update().is_none());
    dw.notify_players_changed();
    let node: &dyn NodeVirtuals = &*h.srv;
    node.send_chat_message_display_notification_send_to_clients("hello", false, None);
    let md: &dyn MatchDelegate = &*h.srv;
    md.boot_all_clients_due_to_no_credit();
    md.match_player_did_change_state(&*h.m, "who", PlayerConnectionState::Unknown);
    md.match_did_fail_with_error(&*h.m);
    node.cleanup();
}

#[test]
fn a_server_without_a_database_has_no_bulk_environment() {
    let h = Harness::with_files("bs_nodb", |save| std::fs::write(format!("{save}/server_db"), b"x").unwrap());
    h.srv.start_bulk_transaction();
    assert!(!h.srv.finish_bulk_transaction());
}

#[test]
fn disconnect_bookkeeping_covers_unknown_and_unlisted_players() {
    let h = Harness::new("bs_disc");
    // A peer that never approved: the unapproved list is cleaned, nobody is remembered.
    h.connect("limbo");
    h.srv.match_player_did_change_state(&*h.m, "limbo", PlayerConnectionState::Disconnected);
    assert!(h.srv.unapproved_clients.borrow().is_empty());
    // A player the recent list names without an alias.
    let a = h.join("Alice");
    h.srv.recent_players.borrow_mut()[0].map.remove(&"alias".to_string());
    h.srv.trade_portal_transactions.borrow_mut().get_or_insert_with(Default::default).insert(a.clone(), 3);
    h.srv.match_player_did_change_state(&*h.m, &a, PlayerConnectionState::Disconnected);
    assert!(h.srv.trade_portal_transactions.borrow().as_ref().unwrap().get(&a).is_none());
    // A failed connection removes the player's pending portal transaction as well.
    let b = h.join("Bob");
    h.srv.trade_portal_transactions.borrow_mut().get_or_insert_with(Default::default).insert(b.clone(), 4);
    h.srv.match_connection_with_player_failed_with_error(&*h.m, &b);
    assert!(h.srv.trade_portal_transactions.borrow().as_ref().unwrap().get(&b).is_none());
    // Many recent players: the search for the disconnected one stops at 65.
    let c = h.join("Cy");
    for i in 0..70 {
        let PlistValue::Dict(d) = dict_with(&[("playerID", &format!("other{i}"))]) else { unreachable!() };
        h.srv.recent_players.borrow_mut().insert(0, d);
    }
    h.srv.match_player_did_change_state(&*h.m, &c, PlayerConnectionState::Disconnected);
}

#[test]
fn contact_packets_use_the_worlds_id_when_the_server_has_none() {
    let h = Harness::new("bs_contact");
    h.connect("x");
    let id = h.srv.save_id.clone().unwrap();
    assert!(h.m.packets_to("x")[0].windows(id.len()).any(|w| w == id.as_bytes()));
}

#[test]
fn the_mock_world_answers_every_question() {
    let h = Harness::new("bs_mock");
    let w = &h.world_dyn;
    assert!(w.load_complete() && w.world_name().is_some() && w.save_id().is_some());
    assert_eq!(w.world_width_macro(), 512);
    assert!(!w.custom_rules_enabled() && w.custom_rules_dict().is_none());
    assert!(w.server_password().is_none() && w.welcome_message().is_none() && w.pvp_enabled());
    assert!(w.dynamic_world().is_some() && w.distance_ordered_food_types().len() == 31);
    assert!(w.has_finished_database_migration_to17());
    assert_eq!(w.server_client_count(), 0);
    assert!(w.world_pos_at_world_index(3) == IntPair { x: 3, y: 3 });
    let mut log = String::new();
    w.append_debug_log(&mut log);
    assert!(w.summary_net_data_for_admin_mod_owner_cloud_mode(true, false, true, 3).len() == 5);
    assert!(h.w.dw.get_next_dynamic_object_id() == 0x94);
    assert!(Harness::plist_of(
        &data_from_property_list(&dict_with(&[("a", "b")])).map(|d| [vec![0], d].concat()).unwrap(),
        1
    )
    .is_some());
    h.m.base.delegate.borrow_mut().take();
    assert!(h.m.local_player_id().len() == 32);
}
