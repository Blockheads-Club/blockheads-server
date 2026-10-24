//! Packet-by-packet tests of the dispatcher: every client message type reaches the world (or is refused), the chat
//! rules, the handshake edge cases and the cloud-only paths over a real server match.

use super::*;
use crate::foundation::exception::catch_ns;
use crate::foundation::md5::string_from_md5;
use crate::net::bhmatch::{MatchDelegate, PlayerConnectionState, PlayerInfo};
use crate::plist::keyed_archiver::archive_index_set;
use crate::server::bhserver::test_support::*;
use crate::types::structs::IntPair;
use rusty_enet as enet;
use std::cell::RefCell;
use std::net::{SocketAddr, UdpSocket};

fn pkt(ty: u8, body: &[u8]) -> Vec<u8> {
    let mut p = vec![ty];
    p.extend_from_slice(body);
    p
}

fn dict_of(pairs: &[(&str, PlistValue)]) -> PlistValue {
    let mut d = new_mutable_dict();
    for (k, v) in pairs {
        d.insert(k.to_string(), v.clone());
    }
    PlistValue::Dict(d)
}

fn plist_pkt(ty: u8, v: &PlistValue) -> Vec<u8> {
    pkt(ty, &data_from_property_list(v).unwrap())
}

fn gz_pkt(ty: u8, prefix: &[u8], v: &PlistValue) -> Vec<u8> {
    let mut body = prefix.to_vec();
    body.extend(gzip_deflate(&data_from_property_list(v).unwrap()));
    pkt(ty, &body)
}

fn blobs(n: u8) -> PlistValue {
    PlistValue::array((0..n).map(|i| PlistValue::data(vec![i])).chain([PlistValue::string("not data")]).collect())
}

fn calls(h: &Harness) -> Vec<String> {
    h.w.calls.borrow().clone()
}

fn dw_calls(h: &Harness) -> Vec<String> {
    h.w.dw.calls.borrow().clone()
}

/// Whether `f` ends in a raised NSException.
fn raises(f: impl FnOnce()) -> bool {
    catch_ns(f).is_err()
}

/// One packet of every world-facing client type from `a`.
fn sweep(h: &Harness, a: &str) {
    let eight = [1u8, 0, 0, 0, 0, 0, 0, 0];
    let sixteen = [2u8; 16];
    h.send(a, &pkt(0x03, &[9, 0, 0, 0, 1]));
    h.send(a, &pkt(0x0d, &[5, 0, 0, 0]));
    for ty in [0x10, 0x11, 0x3a] {
        h.send(a, &pkt(ty, &[0, 0, 0, 0]));
    }
    h.send(a, &pkt(0x12, &[eight.as_slice(), &[3, 0, 0, 0, 0, 0, 0, 0], &[4, 0, 0, 0, 0, 0, 0, 0]].concat()));
    h.send(a, &pkt(0x14, &[1, 2]));
    h.send(a, &pkt(0x15, &sixteen));
    h.send(a, &pkt(0x16, &eight));
    h.send(a, &pkt(0x17, &sixteen));
    h.send(a, &pkt(0x19, &sixteen));
    for ty in [0x1a, 0x29, 0x2d] {
        h.send(a, &pkt(ty, &sixteen));
    }
    h.send(a, &pkt(0x2a, &sixteen));
    h.send(a, &pkt(0x2c, &eight));
    h.send(a, &pkt(0x20, &sixteen));
    h.send(a, &pkt(0x21, &[eight.as_slice(), &[7]].concat()));
    h.send(a, &gz_pkt(0x2e, &[], &dict_of(&[("k", PlistValue::int(1))])));
    h.send(a, &pkt(0x30, &eight));
    h.send(a, &pkt(0x49, &[eight.as_slice(), &eight].concat()));
    h.send(a, &pkt(0x4c, &crate::protocol::codec::index_set_encode(&[(3, 2), (9, 1)])));
    h.send(a, &pkt(0x48, &archive_index_set(&crate::foundation::indexset::IndexSet::new(), "foundItems")));
    h.send(a, &pkt(0x48, b"not an archive"));
}

#[test]
fn object_batches_reach_the_world() {
    let h = Harness::new("dt_objects");
    let a = h.join("Alice");
    for (ty, name) in [(0x0a, "create"), (0x0b, "update"), (0x0c, "remove"), (0x45, "cdu")] {
        h.send(&a, &gz_pkt(ty, &[5], &blobs(2)));
        assert!(calls(&h).contains(&format!("{name} 2 5 {a}")), "{name}");
    }
    let before = calls(&h).len();
    for ty in [0x0a, 0x0b, 0x0c, 0x45] {
        h.send(&a, &pkt(ty, &[5, 1, 2, 3])); // not gzip: nothing to hand over
        h.send(&a, &gz_pkt(ty, &[5], &PlistValue::string("not an array")));
    }
    assert_eq!(calls(&h).len(), before);
    assert!(raises(|| h.send(&a, &[0x0a]))); // a type byte without the object type
}

#[test]
fn world_requests_reach_the_world() {
    let h = Harness::new("dt_world");
    let a = h.join("Alice");
    let b = h.join("Bob");
    h.m.clear();
    let sc =
        crate::server::server_client::ServerClient::init_with_client_id_server_light_block_index_light_block_database(
            &a, &h.srv, 31, None,
        );
    h.w.clients.borrow_mut().insert(a.clone(), sc);
    sweep(&h, &a);
    let log = calls(&h);
    for want in [
        format!("block_request 9 1 {a}"),
        format!("block_removed 5 {a}"),
        format!("fill {a}"),
        format!("rm {a}"),
        format!("rmbw {a}"),
        format!("workbench {a}"),
        format!("ito {a}"),
        format!("heartbeat {a}"),
        format!("paint {a}"),
        "gather".to_string(),
        "projectile".to_string(),
        "tip".to_string(),
        "damage Some(\"Alice\")".to_string(),
        format!("electricity {a}"),
        format!("transaction true {a}"),
    ] {
        assert!(log.contains(&want), "{want} in {log:?}");
    }
    let dw = dw_calls(&h);
    for want in ["freeblocks 2", "blockheads", "inventory", "painting 1", "removal 1", "found"] {
        assert!(dw.iter().any(|c| c.starts_with(want)), "{want} in {dw:?}");
    }
    assert!(dw.iter().any(|c| c.starts_with("pickup [3, 4]")));
    // The three world-wide notices are relayed to the other player only.
    let to_b: Vec<u8> = h.m.packets_to(&b).iter().map(|p| p[0]).collect();
    assert!(to_b.contains(&0x1a) && to_b.contains(&0x29) && to_b.contains(&0x2d));
    // Short payloads are refused with the original's exceptions.
    for (ty, len) in [(0x12, 4usize), (0x15, 8), (0x16, 4), (0x2c, 4), (0x21, 4)] {
        assert!(raises(|| h.send(&a, &pkt(ty, &vec![0; len]))), "{ty:#x}");
    }
}

#[test]
fn id_requests_zoom_and_initial_data() {
    let h = Harness::new("dt_misc2");
    let a = h.join("Alice");
    h.m.clear();
    h.send(&a, &pkt(0x1b, &data_from_property_list(&dict_of(&[("playerID", PlistValue::string("up"))])).unwrap()));
    assert_eq!(h.m.packets_to(&a)[0][0], 0x1c);
    assert_eq!(h.m.packets_to(&a)[0].len(), 9); // a position this time
                                                // Initial data with a chat history: a photo for one speaker, a non-photo, an unknown speaker and no speaker.
    let b = h.join("Bob");
    for entry in [
        dict_of(&[("playerID", PlistValue::string(&a)), ("message", PlistValue::string("old"))]),
        dict_of(&[("playerID", PlistValue::string(&a)), ("message", PlistValue::string("older"))]),
        dict_of(&[("playerID", PlistValue::string(&b)), ("message", PlistValue::string("no photo"))]),
        dict_of(&[("playerID", PlistValue::string("ghost")), ("message", PlistValue::string("gone"))]),
        dict_of(&[("message", PlistValue::string("no id"))]),
    ] {
        let PlistValue::Dict(d) = entry else { unreachable!() };
        h.srv.chat_history.borrow_mut().push(d);
    }
    let photo = |id: &str, v: PlistValue| {
        h.srv.node.player_info_for_peer_id(id).unwrap().borrow_mut().insert("photo".to_string(), v);
    };
    photo(&a, PlistValue::data(vec![1, 2, 3]));
    photo(&b, PlistValue::string("text"));
    h.m.clear();
    h.send(&a, &[0x05]);
    assert_eq!(h.m.packets_to(&a).iter().map(|p| p[0]).collect::<Vec<_>>(), [0x25, 0x06]);
}

#[test]
fn heartbeat_requests() {
    let h = Harness::new("dt_hb2");
    let a = h.join("Alice");
    let sc =
        crate::server::server_client::ServerClient::init_with_client_id_server_light_block_index_light_block_database(
            &a, &h.srv, 31, None,
        );
    h.w.clients.borrow_mut().insert(a.clone(), sc.clone());
    h.send(&a, &[0x18]);
    assert!(sc.requests_heart_beat());
    // With custom rules the old request is a kick, and so is a rules heartbeat that fails verification.
    h.w.rules.set(true);
    h.m.clear();
    h.send(&a, &[0x44, 1, 2, 3]);
    assert!(h.m.packets_to(&a).is_empty()); // verified: fine
    h.w.rules_ok.set(false);
    h.send(&a, &[0x44, 1, 2, 3]);
    assert!(h.m.packets_to(&a).iter().any(|p| p == &[0x26, 0x0b]));
    // An admin is never kicked for it, nor within 30 s of a rules change.
    h.m.clear();
    h.srv.modify_list_for_player_or_ip_is_added_list_type("alice", true, 2);
    h.send(&a, &[0x44]);
    h.srv.modify_list_for_player_or_ip_is_added_list_type("alice", false, 2);
    h.srv.rules_changed_time.set(reference_time());
    h.send(&a, &[0x44]);
    assert!(!h.m.packets_to(&a).contains(&vec![0x26, 0x0b]));
}

#[test]
fn permissions_decide_who_may_change_things() {
    let h = Harness::new("dt_perm");
    let a = h.join("Alice");
    let b = h.join("Bob");
    *h.m.owner.borrow_mut() = Some("Alice".to_string());
    // Welcome message: administrators only.
    h.send(&b, &gz_pkt(0x34, &[], &dict_of(&[("w", PlistValue::string("hi"))])));
    assert!(!calls(&h).iter().any(|c| c.starts_with("welcome_set")));
    h.srv.modify_list_for_player_or_ip_is_added_list_type("alice", true, 2);
    h.send(&a, &gz_pkt(0x34, &[], &dict_of(&[("w", PlistValue::string("hi"))])));
    assert!(calls(&h).contains(&format!("welcome_set true {a}")));
    h.m.clear();
    h.send(&b, &[0x35]);
    assert_eq!(h.m.packets_to(&b)[0][0], 0x36);
    h.w.rules.set(true);
    h.send(&b, &[0x35]);
    h.w.rules.set(false);
    // Owner only: password and privacy changes, the ownership sign list.
    h.send(&b, &plist_pkt(0x38, &dict_of(&[("newServerPassword", PlistValue::string("x"))])));
    assert!(h.w.password.borrow().is_none());
    h.send(&a, &plist_pkt(0x38, &dict_of(&[("newServerPassword", PlistValue::string("x"))])));
    assert_eq!(h.w.password.borrow().as_deref(), Some("x"));
    h.send(&a, &[0x38]);
    assert!(h.w.password.borrow().is_none());
    h.m.clear();
    h.send(&a, &[0x3b]);
    assert_eq!(h.m.packets_to(&a)[0][0], 0x3c);
    // Custom rules: owner or admin, relayed to the others.
    let rule = gz_pkt(0x42, &[], &dict_of(&[("pvp", PlistValue::boolean(false))]));
    h.m.clear();
    h.send(&b, &rule);
    assert!(h.m.packets_to(&a).is_empty());
    h.send(&a, &rule);
    assert!(h.m.packets_to(&b).iter().any(|p| p[0] == 0x42));
    assert!(calls(&h).contains(&"custom_rule".to_string()));
    h.send(&a, &gz_pkt(0x42, &[], &PlistValue::string("not a dictionary")));
    h.send(&a, &pkt(0x42, b"garbage"));
    // Tile repair: only for a client that was put in the repair set, and only owner or admin.
    h.send(&a, &pkt(0x4b, &[4, 0, 0, 0, 0, 0, 0, 0]));
    assert!(!calls(&h).iter().any(|c| c.starts_with("repair")));
    h.srv.repair_set.borrow_mut().insert(a.clone());
    h.send(&a, &pkt(0x4b, &[4, 0, 0, 0, 0, 0, 0, 0]));
    assert!(calls(&h).contains(&"repair 4 4".to_string()));
    h.srv.repair_set.borrow_mut().insert(b.clone());
    h.send(&b, &pkt(0x4b, &[5, 0, 0, 0, 0, 0, 0, 0]));
    assert!(!calls(&h).contains(&"repair 5 5".to_string()));
}

#[test]
fn report_user_saves_the_avatar() {
    let h = Harness::new("dt_report");
    let a = h.join("Alice");
    let b = h.join("Bob");
    h.srv.node.player_info_for_peer_id(&b).unwrap().borrow_mut().insert("photo".to_string(), PlistValue::data(vec![9]));
    h.srv.match_player_did_change_state(&*h.m, &b, PlayerConnectionState::Disconnected);
    let c = h.join("Carol");
    h.srv.match_player_did_change_state(&*h.m, &c, PlayerConnectionState::Disconnected);
    let blocked = format!("{}/saves/{}/reportedAvatars", crate::foundation::paths::appsup(), h.w.id);
    std::fs::write(&blocked, b"in the way").unwrap();
    h.send(&a, &gz_pkt(0x37, &[], &dict_of(&[("playerName", PlistValue::string("Bob"))])));
    std::fs::remove_file(&blocked).unwrap();
    let report = |name: &str| {
        gz_pkt(
            0x37,
            &[],
            &dict_of(&[("playerName", PlistValue::string(name)), ("reporterMessage", PlistValue::string("rude"))]),
        )
    };
    h.send(&a, &report("Bob"));
    h.send(&a, &report("Carol"));
    h.send(&a, &report("Nobody"));
    h.send(&a, &gz_pkt(0x37, &[], &dict_of(&[])));
    assert!(raises(|| h.send(&a, &gz_pkt(0x37, &[], &PlistValue::string("flat")))));
    let log = calls(&h);
    assert!(log.iter().any(|c| c.starts_with("report Some(\"Bob\") Some(\"Alice\") Some(\"")), "{log:?}");
    assert!(log.iter().any(|c| c == "report Some(\"Nobody\") Some(\"Alice\") None"));
}

#[test]
fn chat_variants() {
    let h = Harness::new("dt_chat2");
    let a = h.join("Alice");
    let b = h.join("Bob");
    h.srv.modify_list_for_player_or_ip_is_added_list_type("alice", true, 2);
    h.srv.modify_list_for_player_or_ip_is_added_list_type("bob", true, 3);
    h.srv.cloud_wide_admin_list.replace(Some(vec!["alice".to_string()]));
    h.srv.cloud_wide_invisible_admin_list.replace(Some(vec!["alice".to_string()]));
    *h.m.owner.borrow_mut() = Some("Alice".to_string());
    let say =
        |from: &str, text: &str| h.send(from, &plist_pkt(0x24, &dict_of(&[("message", PlistValue::string(text))])));
    h.m.clear();
    say(&a, "hello");
    let relayed = property_list_from_data(&h.m.packets_to(&b)[0][1..]).unwrap();
    assert_eq!(relayed.object_for_key("mod").and_then(|v| v.as_str()), Some("admin"));
    assert_eq!(relayed.object_for_key("supermod").and_then(|v| v.as_str()), Some("mode2"));
    h.srv.cloud_wide_invisible_admin_list.replace(None);
    h.m.clear();
    say(&a, "again");
    let relayed = property_list_from_data(&h.m.packets_to(&b)[0][1..]).unwrap();
    assert_eq!(relayed.object_for_key("supermod").and_then(|v| v.as_str()), Some("mode1"));
    h.m.clear();
    say(&b, "mod speaking");
    let relayed = property_list_from_data(&h.m.packets_to(&a)[0][1..]).unwrap();
    assert_eq!(relayed.object_for_key("mod").and_then(|v| v.as_str()), Some("mod"));
    // A mod's slash line is a command, answered to the speaker only.
    h.m.clear();
    say(&b, "/help");
    assert!(h.m.packets_to(&a).is_empty());
    // A message without text, with non-string text, and a non-dictionary chat plist all raise or do nothing.
    h.send(&a, &plist_pkt(0x24, &dict_of(&[])));
    assert!(raises(|| h.send(&a, &plist_pkt(0x24, &dict_of(&[("message", PlistValue::int(5))])))));
    assert!(raises(|| h.send(&a, &plist_pkt(0x24, &PlistValue::array(vec![])))));
    assert!(raises(|| h.send(&a, &[0x24, 1, 2, 3])));
    // The history keeps the newest 20.
    for i in 0..25 {
        say(&a, &format!("m{i}"));
    }
    assert_eq!(h.srv.chat_history.borrow().len(), 20);
}

#[test]
fn handshake_edge_cases() {
    let h = Harness::new("dt_hs2");
    // A plist that is not a dictionary raises; an alias given as data counts its bytes.
    h.connect("p1");
    assert!(raises(|| h.send("p1", &plist_pkt(0x1f, &PlistValue::array(vec![])))));
    h.connect("p2");
    h.m.clear();
    h.send("p2", &plist_pkt(0x1f, &dict_of(&[("alias", PlistValue::data(vec![b'x'; 17]))])));
    assert_eq!(h.m.packets_to("p2").iter().find(|p| p[0] == 0x26).map(|p| p[1]), Some(14));
    h.connect("p3");
    assert!(raises(|| h.send("p3", &plist_pkt(0x1f, &dict_of(&[("alias", PlistValue::int(3))])))));
    // The world refusing the client gives "server full".
    h.w.accept_clients.set(false);
    let pid = string_from_md5("DoorMan".to_lowercase().as_str());
    h.m.persistent.borrow_mut().insert(pid.clone(), pid.clone());
    h.connect(&pid);
    h.m.clear();
    h.send(&pid, &Harness::handshake_packet("DoorMan", "ud", 5));
    assert_eq!(h.m.packets_to(&pid).iter().find(|p| p[0] == 0x26).map(|p| p[1]), Some(5));
    assert!(!h.srv.connected_client_ids().contains(&pid));
    h.w.accept_clients.set(true);
    // A returning player proves themself by the iCloud id or the game center id when the device differs.
    for (key, tag) in [("iCloudID", "ic"), ("gameCenterID", "gc")] {
        let alias = format!("Ret{tag}");
        let pid = string_from_md5(&alias.to_lowercase());
        h.m.persistent.borrow_mut().insert(pid.clone(), pid.clone());
        let join = |udid: &str, id: &str| {
            h.connect(&pid);
            h.m.clear();
            let mut d = new_mutable_dict();
            d.insert("alias".to_string(), PlistValue::string(&alias));
            d.insert("udidNew".to_string(), PlistValue::string(udid));
            d.insert("minorVersion".to_string(), PlistValue::int(5));
            d.insert(key.to_string(), PlistValue::string(id));
            h.send(&pid, &plist_pkt(0x1f, &PlistValue::Dict(d)));
            h.m.packets_to(&pid).iter().find(|p| p[0] == 0x26).map(|p| p[1])
        };
        assert_eq!(join("dev-1", "same"), None);
        h.srv.match_player_did_change_state(&*h.m, &pid, PlayerConnectionState::Disconnected);
        assert_eq!(join("dev-2", "same"), None);
        h.srv.match_player_did_change_state(&*h.m, &pid, PlayerConnectionState::Disconnected);
        assert_eq!(join("dev-3", "other"), Some(1));
    }
    // Passwords: right, wrong, and the owner needs none.
    *h.w.password.borrow_mut() = Some("pw".to_string());
    *h.m.owner.borrow_mut() = Some("Boss".to_string());
    let with_pw = |alias: &str, pw: Option<&str>| {
        let pid = string_from_md5(&alias.to_lowercase());
        h.m.persistent.borrow_mut().insert(pid.clone(), pid.clone());
        h.connect(&pid);
        h.m.clear();
        let mut d = new_mutable_dict();
        d.insert("alias".to_string(), PlistValue::string(alias));
        d.insert("udidNew".to_string(), PlistValue::string(format!("u-{alias}")));
        d.insert("minorVersion".to_string(), PlistValue::int(5));
        for p in pw {
            d.insert("clientPassword".to_string(), PlistValue::string(p));
        }
        h.send(&pid, &plist_pkt(0x1f, &PlistValue::Dict(d)));
        h.m.packets_to(&pid).iter().find(|p| p[0] == 0x26).map(|p| p[1])
    };
    assert_eq!(with_pw("PwOk", Some("pw")), None);
    assert_eq!(with_pw("PwBad", Some("nope")), Some(10));
    assert_eq!(with_pw("PwNone", None), Some(10));
    assert_eq!(with_pw("Boss", None), None);
    // A spent reset entry lets the owner back in from a new device.
    *h.w.password.borrow_mut() = None;
    h.srv.match_player_did_change_state(&*h.m, &string_from_md5("boss"), PlayerConnectionState::Disconnected);
    *h.srv.reset_list.borrow_mut() = Some(vec!["boss".to_string(), "other".to_string()]);
    assert_eq!(with_pw("Boss", None), None);
    assert_eq!(h.srv.reset_list.borrow().clone(), Some(vec!["other".to_string()]));
}

#[test]
fn packets_are_gated_by_connection_state() {
    let h = Harness::new("dt_gate");
    let a = h.join("Alice");
    h.m.clear();
    h.send("stranger", &[0x0e]); // not connected, not a handshake
    h.connect("limbo");
    h.m.clear();
    h.send("limbo", &[0x0e]); // unapproved peers may only handshake
    h.send(&a, &[]); // an empty packet is type 0 and unknown
    h.send(&a, &[0xee, 1, 2]); // so is an undefined type
    assert!(h.m.sent.borrow().is_empty());
    // Clients that never finished connecting are dropped on a failed connection.
    h.srv.match_connection_with_player_failed_with_error(&*h.m, "limbo");
    h.srv.match_connection_with_player_failed_with_error(&*h.m, &a);
    assert!(!h.srv.connected_client_ids().contains(&a));
}

#[test]
fn cloud_credit_and_key() {
    let h = RealMatchHarness::new("dt_cloud2", Some("SALT"), Some("Boss"), Some("public"), 0.0);
    let send = |peer: &str, data: &[u8]| h.srv.match_did_receive_data_from_player(&*h.m, data, peer);
    h.srv.match_player_did_change_state(&*h.m, "peer", PlayerConnectionState::Connected);
    // No credit left: the client is told and removed from the unapproved list.
    send("peer", &Harness::handshake_packet("Olga", "uo", 5));
    assert!(h.srv.unapproved_clients.borrow().is_empty());
    assert!(h.srv.credit() <= 0.001);
}

#[test]
fn privacy_changes() {
    let h = RealMatchHarness::new("dt_privacy", Some("SALT"), Some("Boss"), Some("public"), 100.0);
    let send = |peer: &str, v: &PlistValue| {
        h.srv.connected_clients.borrow_mut().push(peer.to_string());
        h.srv.match_did_receive_data_from_player(&*h.m, &plist_pkt(0x39, v), peer);
    };
    let privacy = |s: &str| dict_of(&[("privacySetting", PlistValue::string(s))]);
    // Only the owner may change it; the owner's alias is looked up through the player info.
    let boss = string_from_md5("boss");
    let mut info = new_mutable_dict();
    info.insert("alias".to_string(), PlistValue::string("Boss"));
    info.insert("playerID".to_string(), PlistValue::string(boss.as_str()));
    let info: PlayerInfo = Rc::new(RefCell::new(info));
    h.srv.node.info_arrived_for_player(&info, &boss);
    send("someone", &privacy("private"));
    assert_eq!(h.srv.privacy_string().as_deref(), Some("public"));
    send(&boss, &privacy("private"));
    assert_eq!(h.srv.privacy_string().as_deref(), Some("private"));
    send(&boss, &privacy(""));
    send(&boss, &privacy("friends"));
    send(&boss, &dict_of(&[]));
    h.srv.match_did_receive_data_from_player(&*h.m, &[0x39], &boss);
    assert_eq!(h.srv.privacy_string().as_deref(), Some("private"));
    // Going public drops the password; a whitelist forbids public.
    *h.w.password.borrow_mut() = Some("pw".to_string());
    send(&boss, &privacy("Public"));
    assert!(h.w.password.borrow().is_none());
    send(&boss, &privacy("Searchable"));
    *h.srv.white_list.borrow_mut() = Some(vec!["someone".to_string()]);
    send(&boss, &privacy("public"));
    assert_eq!(h.srv.privacy_string().as_deref(), Some("searchable"));
    // Public without a password set is just a change.
    *h.srv.white_list.borrow_mut() = None;
    send(&boss, &privacy("public"));
    assert_eq!(h.srv.privacy_string().as_deref(), Some("public"));
    // The owner's password change is logged for the cloud.
    h.srv.match_did_receive_data_from_player(
        &*h.m,
        &plist_pkt(0x38, &dict_of(&[("newServerPassword", PlistValue::string("x"))])),
        &boss,
    );
    h.srv.match_did_receive_data_from_player(&*h.m, &[0x38], &boss);
    // With the world gone a switch to public has no password to clear, and a password change goes nowhere.
    *h.srv.world.borrow_mut() = None;
    send(&boss, &privacy("private"));
    send(&boss, &privacy("public"));
    h.srv.match_did_receive_data_from_player(&*h.m, &[0x38], &boss);
}

#[test]
fn portal_chest_acknowledgements_without_a_database_entry() {
    let h = Harness::new("dt_portal");
    let a = h.join("Alice");
    let before = h.m.packets_to(&a).len();
    // An acknowledgement with nothing pending is silent; one for another client's transaction is not replayed.
    send_portal_chest_acknowledgement_if_needed_for_client(&h.srv, &a);
    h.srv.trade_portal_transactions.borrow_mut().get_or_insert_with(Default::default).insert("other".to_string(), 3);
    send_portal_chest_acknowledgement_if_needed_for_client(&h.srv, &a);
    assert_eq!(h.m.packets_to(&a).len(), before);
}

#[test]
fn the_same_packets_without_a_server_client_or_a_world() {
    let h = Harness::new("dt_sweep");
    let a = h.join("Alice");
    h.join("Bob");
    // No server client registered for the sender: fill, remove and heartbeat requests find nobody.
    sweep(&h, &a);
    assert!(calls(&h).contains(&format!("fill {a}")));
    h.send(&a, &[0x18]);
    // The world is gone (dropped after load): every handler falls through quietly.
    let before = calls(&h).len();
    *h.srv.world.borrow_mut() = None;
    sweep(&h, &a);
    for ty in [0x05u8, 0x18, 0x35] {
        h.send(&a, &[ty]);
    }
    h.send(&a, &[0x44]);
    h.send(&a, &plist_pkt(0x1b, &dict_of(&[("playerID", PlistValue::string("up"))])));
    assert_eq!(calls(&h).len(), before);
}

/// Removes the stub mod host when the test ends, even by a panic.
struct HooksGuard;
impl Drop for HooksGuard {
    fn drop(&mut self) {
        StubHooks::uninstall();
    }
}

fn join_result(h: &Harness, alias: &str) -> (String, Option<u8>) {
    let pid = string_from_md5(&alias.to_lowercase());
    h.m.persistent.borrow_mut().insert(pid.clone(), pid.clone());
    h.connect(&pid);
    h.m.clear();
    h.send(&pid, &Harness::handshake_packet(alias, &format!("udid-{alias}"), 5));
    let rejection = h.m.packets_to(&pid).iter().find(|p| p[0] == 0x26).map(|p| p[1]);
    (pid, rejection)
}

#[test]
fn the_mod_host_sees_joins_packets_and_chat() {
    let hooks = StubHooks::install();
    let _guard = HooksGuard;
    let h = Harness::new("dt_hooks");
    let (a, r) = join_result(&h, "Alice");
    assert_eq!(r, None);
    assert!(hooks.saw(&format!("pre_login {a} {a} Alice")) && hooks.saw(&format!("join {a}")));
    // 0xBC goes to the host from connected and waiting peers only.
    h.connect("waiting");
    h.send(&a, &[0xbc, 1, 2]);
    h.send("waiting", &[0xbc]);
    h.send("stranger", &[0xbc, 1]);
    assert_eq!(hooks.log.borrow().iter().filter(|l| l.starts_with("packet ")).count(), 2);
    // A peer in the limbo is dropped by any packet.
    hooks.limbo_packet.set(true);
    h.m.clear();
    h.send(&a, &[0x0e]);
    assert!(h.m.sent.borrow().is_empty());
    hooks.limbo_packet.set(false);
    // Chat: kept, cancelled, rewritten; mod commands bypass the vanilla routing.
    let b = join_result(&h, "Bob").0;
    let say = |text: &str| h.send(&a, &plist_pkt(0x24, &dict_of(&[("message", PlistValue::string(text))])));
    let text_to_bob = || {
        let p = h.m.packets_to(&b);
        p.last().map(|p| {
            property_list_from_data(&p[1..]).unwrap().object_for_key("message").unwrap().as_str().unwrap().to_string()
        })
    };
    h.m.clear();
    say("plain");
    assert_eq!(text_to_bob().as_deref(), Some("plain"));
    *hooks.verdict.borrow_mut() = crate::mods::ChatVerdict::Cancel;
    h.m.clear();
    say("dropped");
    assert!(h.m.packets_to(&b).is_empty());
    *hooks.verdict.borrow_mut() = crate::mods::ChatVerdict::Replace("changed".to_string());
    say("rewritten");
    assert_eq!(text_to_bob().as_deref(), Some("changed"));
    hooks.owns.set(true);
    h.m.clear();
    say("/modcmd arg");
    assert!(hooks.saw("command Some(") && h.m.packets_to(&b).is_empty());
    hooks.owns.set(false);
    // Damage events name the attacker through the acting-player guard.
    h.send(&a, &pkt(0x2a, &[0u8; 16]));
    // Leaving reports the quit; a failed connection tells the host the peer is gone.
    h.srv.match_player_did_change_state(&*h.m, &b, PlayerConnectionState::Disconnected);
    assert!(hooks.saw(&format!("quit {b} false")));
    h.srv.match_player_did_change_state(&*h.m, "waiting", PlayerConnectionState::Disconnected);
    h.srv.match_connection_with_player_failed_with_error(&*h.m, "waiting");
    assert!(hooks.saw("limbo_gone waiting"));
}

#[test]
fn the_mod_host_can_refuse_or_park_a_joining_player() {
    let hooks = StubHooks::install();
    let _guard = HooksGuard;
    let h = Harness::new("dt_hooks2");
    hooks.pre_login.set(Some(reject::KICKED));
    assert_eq!(join_result(&h, "Cara").1, Some(reject::KICKED));
    hooks.pre_login.set(None);
    // A vanilla client on a server with client-required mods gets the explanatory summary and is not approved.
    hooks.limbo.set(true);
    let (pid, r) = join_result(&h, "Dora");
    assert_eq!(r, None);
    assert!(h.m.packets_to(&pid).iter().any(|p| p[0] == 0x01));
    assert!(!h.srv.connected_client_ids().contains(&pid));
    assert!(!hooks.saw(&format!("join {pid}")));
}

#[test]
fn stub_hooks_answer_every_call() {
    let hooks = StubHooks::install();
    let _guard = HooksGuard;
    let h = crate::mods::hooks().unwrap();
    let at = IntPair { x: 1, y: 2 };
    h.tick(0.2);
    h.poll();
    h.world_loaded("w", "id");
    h.world_save(true);
    h.server_stop();
    assert!(h.wants(crate::mods::Optional::BlockChanged));
    assert!(!h.block_place("p", at, 1, 2, 3) && !h.block_break("p", at, false, 4));
    h.block_changed(at, 5, Some("p"));
    assert!(!h.object_place("p", at, true, 6));
    assert_eq!(h.damage(Some("p"), 7, 1.5, None), Some(1.5));
    h.death(None, 8);
    h.player_move("p", at, IntPair { x: 3, y: 4 });
    h.player_quit("p", true);
    assert!(h.command(None, "/x") && hooks.saw("command None /x"));
    assert!(hooks.log.borrow().len() >= 14);
}

#[test]
fn join_checks_that_depend_on_the_transport_and_the_stored_info() {
    let h = Harness::new("dt_join3");
    // A dictionary without an alias is not a known player.
    h.connect("anon");
    h.m.clear();
    h.send("anon", &plist_pkt(0x1f, &dict_of(&[("udidNew", PlistValue::string("u"))])));
    assert_eq!(h.m.packets_to("anon").iter().find(|p| p[0] == 0x26).map(|p| p[1]), Some(2));
    // A narrow world accepts clients that are new enough.
    h.w.width.set(128);
    assert_eq!(join_result(&h, "Newish").1, None);
    h.w.width.set(512);
    // A transport that does not authenticate skips the stored-info and device checks.
    h.m.authenticates.set(false);
    assert_eq!(join_result(&h, "Trusting").1, None);
    h.m.authenticates.set(true);
    // Stored player info that is not a dictionary raises like the original.
    let pid = string_from_md5("corrupt");
    let blob = data_from_property_list(&PlistValue::array(vec![])).unwrap();
    h.srv.server_database().unwrap().set_data(Some(&blob), Some(&format!("{pid}_info")));
    assert!(raises(|| drop(join_result(&h, "Corrupt"))));
}

#[test]
fn cloud_handshake_and_chat_over_a_real_match() {
    let h = RealMatchHarness::new("dt_cloud3", Some("SALT"), Some("Boss"), Some("public"), 100.0);
    // With credit left the handshake goes on to the transport, which does not know this peer.
    h.srv.match_player_did_change_state(&*h.m, "stranger", PlayerConnectionState::Connected);
    h.srv.match_did_receive_data_from_player(&*h.m, &Harness::handshake_packet("Olga", "uo", 5), "stranger");
    assert!(h.srv.unapproved_clients.borrow().is_empty());
    // Public and searchable worlds filter public chat; private ones do not.
    h.srv.curse_list.replace(Some(vec!["darn".to_string()]));
    let id = string_from_md5("talker");
    let mut info = new_mutable_dict();
    info.insert("alias".to_string(), PlistValue::string("Talker"));
    info.insert("playerID".to_string(), PlistValue::string(id.as_str()));
    let info: PlayerInfo = Rc::new(RefCell::new(info));
    h.srv.node.info_arrived_for_player(&info, &id);
    h.srv.connected_clients.borrow_mut().push(id.clone());
    let say = |d: &PlistValue| h.srv.match_did_receive_data_from_player(&*h.m, &plist_pkt(0x24, d), &id);
    say(&dict_of(&[("message", PlistValue::string("what the DARN"))]));
    assert!(h.d.messages().iter().any(|m| m == "what the \u{1F621}"), "{:?}", h.d.messages());
    h.m.set_privacy(2);
    say(&dict_of(&[("message", PlistValue::string("darn again"))]));
    say(&dict_of(&[("alias", PlistValue::string("no text"))])); // nothing to say: the filter drops the key
    h.m.set_privacy(1);
    let garbage = [0x24u8, 1, 2, 3];
    assert!(raises(|| h.srv.match_did_receive_data_from_player(&*h.m, &garbage, &id)));
    h.m.set_privacy(3);
    say(&dict_of(&[("message", PlistValue::string("darn private"))]));
    assert!(h.d.messages().iter().any(|m| m == "darn private"));
}

#[test]
fn approval_with_missing_or_empty_stored_info() {
    let h = Harness::new("dt_info");
    // The transport knows nothing about the player after approving: nothing to remember.
    h.m.quiet_approval.set(true);
    assert_eq!(join_result(&h, "Ghosty").1, None);
    // An empty info dictionary is not remembered either.
    let pid = string_from_md5("blank");
    h.m.infos.borrow_mut().insert(pid, Rc::new(RefCell::new(new_mutable_dict())));
    assert_eq!(join_result(&h, "Blank").1, None);
}

#[test]
fn portal_chest_without_a_server_database() {
    let h = Harness::with_files("dt_nodb", |save| std::fs::write(format!("{save}/server_db"), b"in the way").unwrap());
    assert!(h.srv.server_database().is_none());
    let a = h.join("Alice");
    let raw = [5, 0, 0, 0, 0, 0, 0, 0];
    h.send(&a, &pkt(0x3e, &raw));
    h.m.clear();
    h.send(&a, &pkt(0x14, &[1]));
    assert_eq!(h.m.packets_to(&a)[0][0], 0x3f);
    h.m.clear();
    h.send(&a, &pkt(0x40, &raw)); // nothing was stored, so the resend is refused
    assert_eq!(h.m.packets_to(&a)[0][0], 0x41);
}

#[test]
fn mod_packets_and_mod_chat_without_a_host() {
    let h = Harness::new("dt_nohost");
    let a = h.join("Alice");
    h.m.clear();
    h.send(&a, &[0xbc, 1, 2]); // no host: an unknown type
    assert!(h.m.sent.borrow().is_empty());
    // With a host, a mod's slash line is a command and skips the chat hook.
    let hooks = StubHooks::install();
    let _guard = HooksGuard;
    h.srv.modify_list_for_player_or_ip_is_added_list_type("alice", true, 3);
    h.send(&a, &plist_pkt(0x24, &dict_of(&[("message", PlistValue::string("/help"))])));
    assert!(!hooks.saw("chat "));
}

fn enet_client() -> enet::Host<UdpSocket> {
    enet::Host::new(
        UdpSocket::bind("127.0.0.1:0").unwrap(),
        enet::HostSettings { peer_limit: 1, channel_limit: 8, ..Default::default() },
    )
    .unwrap()
}

/// Pump the server's poll and the client until `done` or three seconds; returns what the client received.
fn pump(h: &RealMatchHarness, c: &mut enet::Host<UdpSocket>, done: &mut dyn FnMut() -> bool) -> Vec<Vec<u8>> {
    let mut got = Vec::new();
    let end = std::time::Instant::now() + std::time::Duration::from_secs(3);
    while std::time::Instant::now() < end && !done() {
        h.m.poll_net_events();
        while let Ok(Some(ev)) = c.service() {
            if let enet::Event::Receive { packet, .. } = ev {
                got.push(packet.data().to_vec());
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    got
}

fn connect_client(h: &RealMatchHarness) -> enet::Host<UdpSocket> {
    let mut c = enet_client();
    c.connect(SocketAddr::from(([127, 0, 0, 1], h.m.local_port().unwrap())), 8, 0).unwrap();
    let waiting = h.srv.unapproved_clients.borrow().len() + 1;
    pump(h, &mut c, &mut || h.srv.unapproved_clients.borrow().len() == waiting);
    c
}

fn send_handshake(c: &mut enet::Host<UdpSocket>, alias: &str, key: &str) {
    let d = dict_of(&[
        ("alias", PlistValue::string(alias)),
        ("udidNew", PlistValue::string(format!("udid-{alias}"))),
        ("minorVersion", PlistValue::int(5)),
        ("cloudKey", PlistValue::string(key)),
    ]);
    let peer = c.connected_peers_mut().next().unwrap();
    peer.send(0, &enet::Packet::reliable(plist_pkt(0x1f, &d))).unwrap();
    c.flush();
}

#[test]
fn a_cloud_player_joins_over_a_real_connection() {
    let h = RealMatchHarness::new("dt_realjoin", Some("SALT"), Some("Remote"), Some("public"), 100.0);
    // The wrong cloud key is refused with reason 9.
    let mut bad = connect_client(&h);
    send_handshake(&mut bad, "Mallory", "wrong");
    let got = pump(&h, &mut bad, &mut || false);
    assert!(got.iter().any(|p| p == &[0x26, 9]));
    // The right one is approved, gets the summary, and shows up as connected.
    let mut c = connect_client(&h);
    send_handshake(&mut c, "Remote", &string_from_md5("SALTRemote"));
    let got = pump(&h, &mut c, &mut || false);
    assert!(got.iter().any(|p| p[0] == 0x01));
    assert_eq!(h.srv.connected_client_ids().len(), 1);
    // The owner is the only one who may change the privacy; the change reaches the transport.
    let peer = c.connected_peers_mut().next().unwrap();
    peer.send(
        0,
        &enet::Packet::reliable(plist_pkt(0x39, &dict_of(&[("privacySetting", PlistValue::string("private"))]))),
    )
    .unwrap();
    c.flush();
    pump(&h, &mut c, &mut || h.srv.privacy_string().as_deref() == Some("private"));
    assert_eq!(h.srv.privacy_string().as_deref(), Some("private"));
}
