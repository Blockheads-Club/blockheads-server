//! The protocol session of the test client: every request method against our server in-process, the handshake
//! variants of the identity, and the receive-side bookkeeping against an ENet endpoint that sends crafted packets.

use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use bh_testclient::harness::inproc::HostServer;
use bh_testclient::harness::refenv::{RunSpec, ServerHandle};
use bh_testclient::session::{
    BatchKind, BlockheadSpec, FillNetRequest, GatherNetRequest, Identity, JoinOutcome, PaintNetRequest,
    PlaceInteractionObjectNetRequest, PlaceWorkbenchNetRequest, PlistDialect, ProjectileNetRequest,
    RemoveBackWallNetRequest, RemoveNetRequest, Session, Throttle, WorldHeartbeat,
};
use bh_testclient::{ClientConfig, TraceSink};
use blockheads_server::foundation::gsdict::GsDict;
use blockheads_server::net::enet_host::{EnetHost, OwnedEvent};
use blockheads_server::plist::value::PlistValue as PV;
use blockheads_server::protocol::block_wire::{self, ExtraData, LIGHT_LEN, TILES_LEN};
use blockheads_server::protocol::messages as m;
use blockheads_server::protocol::object_batches::{encode_batch, BatchKind as ServerBatch};

fn dict(pairs: Vec<(&str, PV)>) -> PV {
    let mut g = GsDict::new();
    for (k, v) in pairs {
        g.insert(k.to_string(), v);
    }
    PV::dict(g)
}

fn cfg_for(port: u16) -> ClientConfig {
    ClientConfig { addr: format!("127.0.0.1:{port}").parse().unwrap(), ..Default::default() }
}

/// An ENet endpoint that answers the handshake and then sends `after_join` (one packet per entry).
struct Fake {
    port: u16,
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}
impl Fake {
    fn start(contact: Vec<u8>, after_join: Vec<Vec<u8>>) -> Fake {
        Self::start_replying(contact, after_join, None)
    }
    /// Like `start`, and answers every packet of the given type with the given packet.
    fn start_replying(contact: Vec<u8>, after_join: Vec<Vec<u8>>, reply: Option<(u8, Vec<Vec<u8>>)>) -> Fake {
        let mut host = EnetHost::create(0).unwrap();
        let port = host.local_port();
        let stop = Arc::new(AtomicBool::new(false));
        let s2 = stop.clone();
        let thread = std::thread::spawn(move || {
            while !s2.load(Ordering::SeqCst) {
                match host.service_wait(5) {
                    Some(OwnedEvent::Connect { peer, .. }) => host.send_reliable(peer, 0, &contact),
                    Some(OwnedEvent::Receive { peer, data, .. }) if data.first() == Some(&0x1f) => {
                        for p in &after_join {
                            host.send_reliable(peer, 0, p);
                        }
                    }
                    Some(OwnedEvent::Receive { peer, data, .. })
                        if reply.as_ref().is_some_and(|(ty, _)| data.first() == Some(ty)) =>
                    {
                        for p in &reply.as_ref().unwrap().1 {
                            host.send_reliable(peer, 0, p);
                        }
                    }
                    _ => {}
                }
            }
        });
        Fake { port, stop, thread: Some(thread) }
    }
}
impl Fake {
    /// Approves the join and then drops the peer.
    fn hangup_after_summary() -> Fake {
        let mut host = EnetHost::create(0).unwrap();
        let port = host.local_port();
        let stop = Arc::new(AtomicBool::new(false));
        let s2 = stop.clone();
        let thread = std::thread::spawn(move || {
            while !s2.load(Ordering::SeqCst) {
                match host.service_wait(5) {
                    Some(OwnedEvent::Connect { peer, .. }) => {
                        host.send_reliable(peer, 0, &m::initial_contact(None, false, "w"))
                    }
                    Some(OwnedEvent::Receive { peer, data, .. }) if data.first() == Some(&0x1f) => {
                        host.send_reliable(peer, 0, &m::world_summary(&m::WorldSummary::default()));
                        host.peer_disconnect_later(peer, 0);
                    }
                    _ => {}
                }
            }
        });
        Fake { port, stop, thread: Some(thread) }
    }
}
impl Drop for Fake {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(t) = self.thread.take() {
            t.join().unwrap();
        }
    }
}

#[test]
fn identity_packets_in_both_dialects() {
    let full = |d: PlistDialect| {
        Identity::new("Al")
            .udid("udid-x")
            .password("pw")
            .minor_version(5)
            .cloud_key("ck")
            .icloud_id("ic")
            .game_center_id("gc")
            .photo(vec![1, 2, 3])
            .mod_loader(1, "abcd")
            .dialect(d)
    };
    let apple = full(PlistDialect::Apple).player_info_packet();
    let text = String::from_utf8_lossy(&apple[1..]).into_owned();
    for needle in [
        "clientPassword",
        "cloudKey",
        "iCloudID",
        "gameCenterID",
        "<key>photo</key><data>AQID</data>",
        "bhcModLoader",
        "<integer>5</integer>",
    ] {
        assert!(text.contains(needle), "{needle}");
    }
    let gnustep = full(PlistDialect::GnuStep).player_info_packet();
    let ClientMsgLike(parsed) = ClientMsgLike::of(&gnustep);
    for k in [
        "alias",
        "udidNew",
        "minorVersion",
        "clientPassword",
        "cloudKey",
        "iCloudID",
        "gameCenterID",
        "photo",
        "bhcModLoader",
    ] {
        assert!(parsed.object_for_key(k).is_some(), "{k}");
    }
    assert_eq!(Identity::new("Al").player_id(), Identity::new("AL").player_id());
    let plain = Identity::new("P");
    assert!(
        plain.player_info_packet().len() < apple.len()
            && plain.dialect(PlistDialect::GnuStep).player_info_packet().len() > 10
    );
    assert_eq!(bh_testclient::session::iso8601_utc(0), "1970-01-01T00:00:00Z");
    assert_eq!(bh_testclient::session::iso8601_utc(1_700_000_000), "2023-11-14T22:13:20Z");
}

struct ClientMsgLike(PV);
impl ClientMsgLike {
    fn of(packet: &[u8]) -> ClientMsgLike {
        match blockheads_server::protocol::messages::decode_client_packet(packet).unwrap() {
            Some(blockheads_server::protocol::messages::ClientMsg::PlayerInformation { payload }) => {
                ClientMsgLike(blockheads_server::protocol::codec::plist_from(&payload).unwrap())
            }
            other => panic!("{other:?}"),
        }
    }
}

#[test]
fn the_throttle_follows_the_real_clients_rules() {
    let mut t = Throttle::default();
    for i in 0..Throttle::SEND_BELOW as i32 {
        assert!(t.request(i, true, 0).is_some());
    }
    assert!(t.request(100, true, 0).is_none());
    assert!(t.request(100, true, 0).is_none());
    assert_eq!(t.queue.len(), 1);
    assert!(t.request(3, true, 10).is_none());
    assert!(t.request(3, false, Throttle::RESEND_MS + 1).is_some());
    let out = t.answered(0, 20);
    assert!(out.is_empty(), "still {} in flight", t.in_flight.len());
    let out = t.answered(1, 20);
    assert_eq!(out.len(), 1);
    t.note_sent(7, 0);
    t.queue.push_back((7, true));
    t.queue.push_back((200, false));
    let out = t.answered(2, 30);
    assert!(out.len() <= 1 && t.in_flight.contains_key(&7));
}

#[test]
fn every_request_method_against_our_server() {
    let spec = RunSpec {
        tag: "session-api".into(),
        args: [
            "--new",
            "API",
            "--world_id",
            "a11ce000a11ce000a11ce000a11ce000",
            "--world_width",
            "1/16",
            "--owner",
            "Boss",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect(),
        ..Default::default()
    };
    let mut server = HostServer::start(&spec).unwrap();
    server.wait_for_log("World load complete.", Duration::from_secs(120)).unwrap();
    let cfg = cfg_for(server.port());
    let mut s = Session::connect(cfg.clone(), Identity::new("Boss")).unwrap();
    s.set_trace(Some(TraceSink::memory()));
    assert_eq!(s.join(10_000).unwrap(), JoinOutcome::Approved);
    s.set_trace(None);
    s.request_initial_data().unwrap();
    assert!(s.wait_for(0x06, 5000).unwrap().is_some());
    s.request_block_xy(0, 0, true).unwrap();
    assert_eq!(s.macro_index_for_tile(40, 70), 2 * s.state.world_width_macro + 1);
    assert_eq!(s.fill_index(3, 2), 2 * s.state.world_width_macro as i64 * 32 + 3);
    let answered = s.request_blocks_throttled(&[0, 1, 2, 3], true, 10_000).unwrap();
    assert!(answered >= 1);
    // more than 17 outstanding requests are queued and sent as the answers come in
    let many: Vec<i32> = (4..44).collect();
    assert!(s.request_blocks_throttled(&many, true, 60_000).unwrap() >= 20);
    s.block_removal_notification(3).unwrap();
    assert!(s.request_unique_id().unwrap().is_some());
    s.create_blockhead(&BlockheadSpec::new("Al", 100, 600)).unwrap();
    s.send_object_batch(BatchKind::Update, 24, &[vec![0; 9]]).unwrap();
    s.upload_blockheads(&PV::array(vec![])).unwrap();
    s.upload_blockheads(&dict(vec![("dynamicObjects", PV::array(vec![]))])).unwrap();
    s.upload_inventory(5, &PV::array(vec![])).unwrap();
    s.fill(FillNetRequest::default(), None).unwrap();
    s.fill(FillNetRequest::default(), Some(&dict(vec![]))).unwrap();
    s.remove(RemoveNetRequest::default()).unwrap();
    s.remove_back_wall(RemoveBackWallNetRequest::default()).unwrap();
    s.paint(PaintNetRequest::default()).unwrap();
    s.place_workbench(PlaceWorkbenchNetRequest::default(), None).unwrap();
    s.place_workbench(PlaceWorkbenchNetRequest::default(), Some(&dict(vec![]))).unwrap();
    s.place_interaction_object(PlaceInteractionObjectNetRequest::default(), &dict(vec![])).unwrap();
    s.pickup(5, &[1, 2]).unwrap();
    s.gather(GatherNetRequest::default()).unwrap();
    s.projectile(ProjectileNetRequest::default()).unwrap();
    s.tip("hint").unwrap();
    s.zoom_request("pid", 1).unwrap();
    s.trade(&dict(vec![])).unwrap();
    s.heartbeat_request_old().unwrap();
    s.heartbeat_request_with_rules(None).unwrap();
    s.heartbeat_request_with_rules(Some([0; 64])).unwrap();
    s.send_heartbeat(&WorldHeartbeat::default()).unwrap();
    s.chat("hello").unwrap();
    s.chat_dict(&dict(vec![("message", PV::string("hi")), ("alias", PV::string("Boss"))])).unwrap();
    let replies = s.command("/help").unwrap();
    let _ = replies;
    s.welcome_get().unwrap();
    s.welcome_set("welcome").unwrap();
    s.ownership_sign_list().unwrap();
    s.portal_chest(3, false).unwrap();
    s.portal_chest(3, true).unwrap();
    s.send_raw(&[0x0e]).unwrap();
    s.send_xml(0x24, "<?xml version=\"1.0\"?><plist><dict/></plist>").unwrap();
    s.run_for(300).unwrap();
    s.wait_quiet(200, 2000).unwrap();
    assert!(s.wait_for_any(&[0x55, 0x56], 50).unwrap().is_none());
    assert!(s.wait_for_matching(50, &mut |_| false).unwrap().is_none());
    let drained = s.drain();
    assert!(!drained.is_empty() && s.drain().is_empty());
    assert!(s.count_of(0x01) == 1 && s.received().len() >= drained.len());
    s.disconnect();
    assert!(s.state.disconnected);
    assert!(Session::connect(
        ClientConfig {
            connect_timeout_ms: 200,
            addr: "127.0.0.1:9".parse::<SocketAddr>().unwrap(),
            ..Default::default()
        },
        Identity::new("X")
    )
    .is_err());
}

#[test]
fn the_receive_side_tracks_what_the_server_sends() {
    let pid = Identity::new("Zed").player_id();
    let tiles = [0u8; TILES_LEN];
    let light = [0u8; LIGHT_LEN];
    let good_hash = block_wire::block_hash(&pid, 1, 0, false);
    let good = block_wire::server_block_from_parts(
        1,
        &tiles,
        0,
        1,
        Some((&light, &ExtraData { hash: good_hash, receipts: None })),
    )
    .unwrap();
    let bad_hash = block_wire::server_block_from_parts(
        2,
        &tiles,
        0,
        1,
        Some((&light, &ExtraData { hash: "nope".into(), receipts: None })),
    )
    .unwrap();
    let mut undecodable = vec![0x04];
    undecodable.extend_from_slice(&3u32.to_le_bytes());
    undecodable.extend_from_slice(b"garbage");
    let mut blockhead_blob = vec![0u8; 0x48];
    blockhead_blob[..8].copy_from_slice(&77u64.to_le_bytes());
    blockhead_blob
        .extend(blockheads_server::protocol::codec::plist_xml(&dict(vec![("clientID", PV::string(&pid))])).unwrap());
    let foreign = {
        let mut b = vec![0u8; 0x48];
        b[..8].copy_from_slice(&78u64.to_le_bytes());
        b.extend(
            blockheads_server::protocol::codec::plist_xml(&dict(vec![("clientID", PV::string("someone else"))]))
                .unwrap(),
        );
        b
    };
    let mut junk_blob = vec![6u8; 0x48];
    junk_blob.extend_from_slice(b"<?xml junk");
    let mut food = [0i32; 31];
    food[0] = 4;
    let script = vec![
        m::world_summary(&m::WorldSummary { world_width_macro: 64, ..Default::default() }),
        m::player_list(&[]),
        m::chat_message(&m::server_chat_dict("hi", "A", "p", 1.0)),
        m::heartbeat(&WorldHeartbeat::default()),
        m::initial_data_reply(&food, None),
        m::unique_id_reply(99),
        good,
        bad_hash,
        undecodable,
        block_wire::block_not_loaded(5),
        block_wire::light_block(7, &light, 2),
        block_wire::light_block(7, &light, 2),
        block_wire::light_block(7, &light, 3),
        encode_batch(
            ServerBatch::Create,
            24,
            &[blockhead_blob, foreign, vec![1, 2], vec![9; 8], vec![5; 0x48], junk_blob],
        )
        .unwrap(),
        encode_batch(
            ServerBatch::Update,
            24,
            &[{
                let mut b = vec![0u8; 9];
                b[..8].copy_from_slice(&77u64.to_le_bytes());
                b
            }],
        )
        .unwrap(),
        encode_batch(ServerBatch::Remove, 24, &[77u64.to_le_bytes().to_vec()]).unwrap(),
        m::zoom_response(None),
        vec![0xee],
        vec![],
        vec![0x01],
        blockheads_server::protocol::codec::frame(
            0x01,
            &blockheads_server::protocol::codec::plist_gz(&dict(vec![])).unwrap(),
        ),
    ];
    let fake = Fake::start(m::initial_contact(Some("Owner"), true, "world-1"), script);
    let mut s = Session::connect(cfg_for(fake.port), Identity::new("Zed")).unwrap();
    assert_eq!(s.join(5000).unwrap(), JoinOutcome::Approved);
    s.run_for(600).unwrap();
    let st = &s.state;
    assert_eq!(
        (st.world_id.as_deref(), st.owner_name.as_deref(), st.requires_password, st.protocol_version),
        (Some("world-1"), Some("Owner"), true, Some(0x26))
    );
    assert_eq!(st.world_width_macro, 64);
    assert!(
        st.players.is_some() && st.chat.len() == 1 && st.last_heartbeat.is_some() && st.food_types.unwrap()[0] == 4
    );
    assert_eq!(st.unique_ids, vec![99]);
    assert!(st.block_msgs.contains_key(&1) && st.block_msgs.contains_key(&2));
    assert!(st.notes.iter().any(|n| n.contains("block 2 hash mismatch (nope)")), "{:?}", st.notes);
    assert!(!st.notes.iter().any(|n| n.contains("block 1 hash mismatch")));
    assert!(st.notes.iter().any(|n| n == "bad block 3"));
    assert_eq!(st.blocks_not_loaded, vec![5]);
    assert!(st.notes.iter().any(|n| n.contains("stale light block 7 upd 2 <= 2")));
    assert_eq!(st.light_update_index[&7], 3);
    assert_eq!(st.created.len(), 1);
    assert!(!st.objects_seen.contains_key(&(24, 77)) && st.objects_seen.contains_key(&(24, 78)));
    // unsolicited blocks are dropped with a removal notification when asked to
    drop(s);
    let fake2 = Fake::start(
        m::initial_contact(None, false, "w"),
        vec![
            m::world_summary(&m::WorldSummary::default()),
            block_wire::server_block_from_parts(9, &tiles, 0, 1, None).unwrap(),
        ],
    );
    let mut s = Session::connect(cfg_for(fake2.port), Identity::new("Zed")).unwrap();
    s.config.auto_removal_notification = true;
    assert_eq!(s.join(5000).unwrap(), JoinOutcome::Approved);
    s.run_for(400).unwrap();
    assert!(s.state.block_msgs.is_empty());
    // custom rules of the summary decide the image of the 0x44 heartbeat request; a hang-up is noticed
    let rules = dict(vec![("seed", PV::string("custom"))]);
    let fake3 = Fake::start(
        m::initial_contact(None, false, "w"),
        vec![m::world_summary(&m::WorldSummary { custom_rules: Some(rules), expert_mode: true, ..Default::default() })],
    );
    let mut s = Session::connect(cfg_for(fake3.port), Identity::new("Zed")).unwrap();
    assert_eq!(s.join(5000).unwrap(), JoinOutcome::Approved);
    s.heartbeat_request_with_rules(None).unwrap();
    drop(fake3);
    s.run_for(200).unwrap();
    let hang = Fake::hangup_after_summary();
    let mut s = Session::connect(cfg_for(hang.port), Identity::new("Zed")).unwrap();
    assert_eq!(s.join(5000).unwrap(), JoinOutcome::Approved);
    s.run_for(500).unwrap();
    assert!(s.state.disconnected);
    // a contact without plist, and a command that gets chat replies
    let bare = Fake::start_replying(
        vec![0x23, 0x26],
        vec![m::world_summary(&m::WorldSummary::default()), vec![0x01]],
        Some((
            0x24,
            vec![m::chat_message(&m::server_chat_dict("reply", "S", "p", 1.0)), b"\x24<?xml junk".to_vec(), vec![0xee]],
        )),
    );
    let mut s = Session::connect(cfg_for(bare.port), Identity::new("Zed")).unwrap();
    assert_eq!(s.join(5000).unwrap(), JoinOutcome::Approved);
    assert_eq!(s.state.world_id, None);
    let replies = s.command("/help").unwrap();
    assert!(
        replies.iter().any(|r| r.object_for_key("message").and_then(|v| v.as_str()) == Some("reply")),
        "{replies:?}"
    );
    // rejections and an unreachable contact
    let rej = Fake::start(m::initial_contact(None, false, "w"), vec![m::rejection(14)]);
    let mut s = Session::connect(cfg_for(rej.port), Identity::new("Zed")).unwrap();
    assert_eq!(s.join(3000).unwrap(), JoinOutcome::Rejected { reason: 14 });
    assert_eq!(s.state.last_rejection, Some(14));
    let silent = Fake::start(vec![], vec![]);
    let mut s = Session::connect(cfg_for(silent.port), Identity::new("Zed")).unwrap();
    assert_eq!(s.join(300).unwrap(), JoinOutcome::NoContact);
    let no_summary = Fake::start(m::initial_contact(None, false, "w"), vec![]);
    let mut s = Session::connect(cfg_for(no_summary.port), Identity::new("Zed")).unwrap();
    assert_eq!(s.join(300).unwrap(), JoinOutcome::NoContact);
}
