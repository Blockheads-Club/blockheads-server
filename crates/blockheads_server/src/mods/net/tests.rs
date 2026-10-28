use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;
use std::time::{Duration, Instant};

use bhmod_proto::{mods_hash, FileRequest, Hello, ModInfo, ModPermissions, Ready, CHUNK_SIZE, PACKET_TYPE};

use super::*;
use crate::mods::catalog::{Bytes, Entry};
use crate::mods::content::Content;

#[derive(Default)]
struct Fake {
    sent: RefCell<Vec<(String, Vec<u8>)>>,
    rejected: RefCell<Vec<(String, u8)>>,
    approved: RefCell<Vec<String>>,
    waiting: RefCell<Vec<String>>,
    players: Cell<Option<(u32, u32)>>,
}

impl Link for Fake {
    fn send(&self, peer: &str, packet: Vec<u8>) {
        self.sent.borrow_mut().push((peer.to_string(), packet));
    }
    fn reject(&self, peer: &str, reason: u8) {
        self.rejected.borrow_mut().push((peer.to_string(), reason));
    }
    fn approved(&self, peer: &str) -> bool {
        self.approved.borrow().iter().any(|p| p == peer)
    }
    fn alive(&self, peer: &str) -> bool {
        self.approved(peer) || self.waiting.borrow().contains(&peer.to_string())
    }
    fn players(&self) -> Option<(u32, u32)> {
        self.players.get()
    }
}

impl Fake {
    fn take(&self) -> Vec<Message> {
        self.sent.borrow_mut().drain(..).map(|(_, p)| Message::decode(&p).unwrap()).collect()
    }
}

fn entry(id: &str, required: bool, bytes: Option<Bytes>, data: &[u8]) -> Entry {
    let sha: Hash = crate::mods::catalog::parse_hash(&bhmod_manifest::pack::sha256_hex(data)).unwrap();
    Entry {
        info: ModInfo {
            id: id.into(),
            version: "1.0.0".into(),
            name: [("en".to_string(), id.to_string())].into(),
            description: Default::default(),
            sides: vec!["client".into()],
            client_required: required,
            sha256: sha,
            size: data.len() as u64,
            permissions: ModPermissions::default(),
            source_url: None,
            index_listed: false,
        },
        bytes,
    }
}

const BIG: usize = CHUNK_SIZE * 10 + 5;

fn big() -> Vec<u8> {
    (0..BIG).map(|i| (i % 251) as u8).collect()
}

fn net() -> ModNet {
    let data = big();
    let e = entry("big", true, Some(Bytes::Memory(Rc::new(data.clone()))), &data);
    let h = mods_hash([(e.info.id.as_str(), e.info.version.as_str(), &e.info.sha256)]);
    ModNet::new(Catalog { entries: vec![e], mods_hash: h, client_required: true })
}

fn hello(proto: u64) -> Vec<u8> {
    Message::Hello(Hello {
        proto,
        loader: "0.1.0".into(),
        platform: "pc-linux-x86_64".into(),
        api: "0.2.0".into(),
        runtimes: vec!["lua".into()],
        cached: vec![],
        features: vec![],
    })
    .encode()
}

fn req(sha: Hash, offset: u64) -> Vec<u8> {
    Message::FileRequest(FileRequest { sha256: sha, offset }).encode()
}

fn chunks(ms: &[Message]) -> Vec<(u64, usize)> {
    ms.iter()
        .filter_map(|m| match m {
            Message::FileChunk(c) => Some((c.offset, c.data.len())),
            _ => None,
        })
        .collect()
}

fn err(ms: &[Message]) -> Option<u64> {
    ms.iter().find_map(|m| match m {
        Message::FileError(e) => Some(e.code),
        _ => None,
    })
}

#[test]
fn hello_and_list() {
    assert!(chunks(&[Message::ListRequest]).is_empty()); // only file chunks count
    let l = Fake::default();
    let mut n = net();
    // Nothing before HELLO; garbage and unknown ops are dropped.
    for p in [Message::ListRequest.encode(), vec![PACKET_TYPE, 0x7f], vec![PACKET_TYPE], vec![1, 2]] {
        assert_eq!(n.packet(&l, "e", &p, &[]), None);
    }
    assert!(l.take().is_empty());
    n.packet(&l, "e", &hello(1), &[]);
    assert!(n.is_modded("e") && n.has_peers());
    let sent = l.take();
    let [Message::ServerHello(h)] = &sent[..] else { unreachable!() };
    assert!(h.client_required);
    assert_eq!(h.count, 1);
    assert_eq!(h.mods_hash, n.catalog.mods_hash);
    assert_eq!(h.players, None);
    // A link that knows its player count puts it into the answer.
    l.players.set(Some((3, 16)));
    n.packet(&l, "f", &hello(1), &[]);
    let sent = l.take();
    let [Message::ServerHello(h)] = &sent[..] else { unreachable!() };
    assert_eq!(h.players, Some(PlayerCount { online: 3, capacity: 16 }));
    n.packet(&l, "e", &Message::ListRequest.encode(), &["big:sync".to_string()]);
    let sent = l.take();
    let [Message::ModList(m)] = &sent[..] else { unreachable!() };
    assert_eq!(m.mods[0].id, "big");
    assert_eq!(m.channels, vec![ChannelInfo { name: "big:sync".into(), id: 1 }]);
    // HELLO from a peer in the game is ignored.
    l.approved.borrow_mut().push("p".into());
    n.packet(&l, "p", &hello(1), &[]);
    assert!(l.take().is_empty() && !n.is_modded("p"));
}

#[test]
fn wrong_proto_is_rejected_with_the_vector_text() {
    let l = Fake::default();
    let mut n = net();
    n.packet(&l, "e", &hello(2), &[]);
    let sent = l.sent.borrow().clone();
    // vectors/08-reject-text.hex of mod-sdk.
    let want = "bc08a36375726c782968747470733a2f2f626c6f636b68656164732e636c75622f646f63732f6d6f64732f706c61796572736474657874784d5468697320736572766572206e65656473206d6f64206c6f616465722070726f746f636f6c20312e2055706461746520796f7572206d6f64206c6f61646572207769746820506174636865722e6a726561736f6e436f646503";
    assert_eq!(hex_of(&sent[0].1), want);
    assert_eq!(l.rejected.borrow().clone(), vec![("e".to_string(), 3)]);
    assert!(!n.is_modded("e"));
}

fn hex_of(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

#[test]
fn download_window_resume_and_errors() {
    let l = Fake::default();
    let mut n = net();
    let sha = n.catalog.entries[0].info.sha256;
    n.packet(&l, "e", &hello(1), &[]);
    l.take();
    let c = CHUNK_SIZE as u64;
    // First request: 8 chunks in flight.
    n.packet(&l, "e", &req(sha, 0), &[]);
    let got = chunks(&l.take());
    assert_eq!(got.len(), 8);
    assert_eq!(got[7], (7 * c, CHUNK_SIZE));
    // Ack 4 chunks: the window slides by 4.
    n.packet(&l, "e", &req(sha, 4 * c), &[]);
    assert_eq!(chunks(&l.take()), vec![(8 * c, CHUNK_SIZE), (9 * c, CHUNK_SIZE), (10 * c, 5)]);
    // Ack beyond: nothing left to send.
    n.packet(&l, "e", &req(sha, 8 * c), &[]);
    assert!(l.take().is_empty());
    // Resume from an earlier offset (a new connection would do this): restarts there.
    n.packet(&l, "e", &req(sha, 2 * c), &[]);
    assert_eq!(chunks(&l.take()).first(), Some(&(2 * c, CHUNK_SIZE)));
    // Final ack completes the transfer.
    n.packet(&l, "e", &req(sha, BIG as u64), &[]);
    assert!(l.take().is_empty());
    // Errors.
    n.packet(&l, "e", &req([7; 32], 0), &[]);
    assert_eq!(err(&l.take()), Some(file_error::NOT_FOUND));
    n.packet(&l, "e", &req(sha, 5), &[]);
    assert_eq!(err(&l.take()), Some(file_error::BAD_OFFSET));
    n.packet(&l, "e", &req(sha, BIG as u64 + c), &[]);
    assert_eq!(err(&l.take()), Some(file_error::BAD_OFFSET));
    // READY ends any transfer and is remembered.
    n.packet(&l, "e", &req(sha, 0), &[]);
    l.take();
    n.packet(&l, "e", &Message::Ready(Ready { accepted_hash: [1; 32], loaded: vec![] }).encode(), &[]);
    assert_eq!(n.ready_hash("e"), Some([1; 32]));
    assert_eq!(n.ready_hash("x"), None);
}

#[test]
fn ready_tells_which_optional_mods_run() {
    let l = Fake::default();
    let mut n = net();
    n.packet(&l, "e", &hello(1), &[]);
    // Before READY nothing is known: as before, the peer gets every channel.
    assert!(n.runs_mod("e", "m") && n.runs_mod("e", "other"));
    let ready = |ids: &[&str]| {
        let loaded = ids.iter().map(|i| (i.to_string(), "1.0.0".to_string())).collect();
        Message::Ready(Ready { accepted_hash: [1; 32], loaded }).encode()
    };
    n.packet(&l, "e", &ready(&["m"]), &[]);
    assert!(n.runs_mod("e", "m") && !n.runs_mod("e", "opt"));
    n.rekey("e", "p");
    l.approved.borrow_mut().push("p".into());
    l.take();
    // A channel of a mod the player declined is not sent to them.
    let peers = vec!["p".to_string()];
    assert_eq!(n.send_channel(&l, &peers, "opt:x", b"1"), Ok(0));
    assert_eq!(n.send_channel(&l, &peers, "m:x", b"1"), Ok(1));
    let got = l.take();
    assert_eq!(got.len(), 2, "one CHANNEL_REGISTER and one CHANNEL, nothing for opt:x");
}

#[test]
fn busy_and_read_failures() {
    let l = Fake::default();
    let mut n = net();
    let sha = n.catalog.entries[0].info.sha256;
    for i in 0..=MAX_TRANSFERS {
        let p = format!("e{i}");
        n.packet(&l, &p, &hello(1), &[]);
        n.packet(&l, &p, &req(sha, 0), &[]);
    }
    let last = l.sent.borrow().last().cloned().unwrap();
    assert_eq!(last.0, format!("e{MAX_TRANSFERS}"));
    assert!(matches!(Message::decode(&last.1).unwrap(), Message::FileError(FileError { code: file_error::BUSY, .. })));
    // A package that vanished from disk.
    let l = Fake::default();
    let data = vec![1u8; 10];
    let e = entry("gone", false, Some(Bytes::File(PathBuf::from("/nonexistent/x.bhmod"))), &data);
    let sha = e.info.sha256;
    let mut n = ModNet::new(Catalog { entries: vec![e], ..Catalog::default() });
    n.packet(&l, "e", &hello(1), &[]);
    n.packet(&l, "e", &req(sha, 0), &[]);
    assert_eq!(err(&l.take()), Some(file_error::INTERNAL));
}

#[test]
fn listed_only_entries_are_not_served() {
    let l = Fake::default();
    let e = entry("srv", false, None, b"x");
    let sha = e.info.sha256;
    assert!(e.read(0, 1).is_err());
    let mut n = ModNet::new(Catalog { entries: vec![e], ..Catalog::default() });
    n.packet(&l, "e", &hello(1), &[]);
    n.packet(&l, "e", &req(sha, 0), &[]);
    assert_eq!(err(&l.take()), Some(file_error::NOT_FOUND));
}

#[test]
fn channels_both_ways() {
    let l = Fake::default();
    let mut n = net();
    n.packet(&l, "e", &hello(1), &[]);
    n.packet(&l, "e", &Message::ListRequest.encode(), &["m:a".to_string()]);
    l.take();
    let ch = |id: u16, d: &[u8]| Message::Channel(Channel { channel_id: id, payload: d.to_vec() }).encode();
    // Not in the game yet: dropped.
    assert_eq!(n.packet(&l, "e", &ch(1, b"x"), &[]), None);
    n.rekey("e", "p");
    n.rekey("p", "p");
    l.approved.borrow_mut().push("p".into());
    assert_eq!(n.packet(&l, "p", &ch(1, b"x"), &[]), Some(Inbound::Channel("m:a".to_string(), b"x".to_vec())));
    assert_eq!(n.packet(&l, "p", &ch(9, b"x"), &[]), None);
    // A client CHANNEL_REGISTER is ignored.
    let reg = Message::ChannelRegister(ChannelRegister { name: "m:z".into(), id: 5 }).encode();
    assert_eq!(n.packet(&l, "p", &reg, &[]), None);
    assert!(l.take().is_empty());
    // Outbound: "m:a" was in the list; "m:b" is announced first.
    let peers = vec!["p".to_string(), "vanilla".to_string()];
    assert_eq!(n.send_channel(&l, &peers, "m:a", b"1"), Ok(1));
    assert_eq!(n.send_channel(&l, &peers, "m:b", b"2"), Ok(1));
    let got = l.take();
    assert_eq!(got.len(), 3);
    assert_eq!(got[1], Message::ChannelRegister(ChannelRegister { name: "m:b".into(), id: 2 }));
    assert_eq!(got[2], Message::Channel(Channel { channel_id: 2, payload: b"2".to_vec() }));
    // Late channels are announced once, to peers in the game.
    let names = vec!["m:a".to_string(), "m:b".to_string(), "m:c".to_string()];
    n.announce(&l, &names);
    n.announce(&l, &names);
    assert_eq!(l.take(), vec![Message::ChannelRegister(ChannelRegister { name: "m:c".into(), id: 3 })]);
    assert_eq!(n.approved_peers(&l), vec!["p".to_string()]);
    // Peers that left are forgotten.
    l.approved.borrow_mut().clear();
    n.prune(&l);
    assert!(!n.has_peers());
    n.announce_to(&l, "gone", &names);
}

fn call(id: u32) -> ServiceCall {
    ServiceCall {
        id,
        mod_id: "shop-ui".into(),
        service: "economy".into(),
        version_req: "^1".into(),
        method: "balance".into(),
        args: vec![0x80],
    }
}

#[test]
fn service_calls_both_ways() {
    let l = Fake::default();
    let mut n = net();
    n.packet(&l, "e", &hello(1), &[]);
    l.take();
    let wire_call = Message::ServiceCall(call(7)).encode();
    let wire_result = Message::ServiceResult(ServiceResult { id: 7, outcome: Ok(vec![0xf6]) }).encode();
    // Not in the game yet: dropped. A peer that never sent HELLO is dropped too.
    assert_eq!(n.packet(&l, "e", &wire_call, &[]), None);
    assert_eq!(n.packet(&l, "e", &wire_result, &[]), None);
    assert_eq!(n.packet(&l, "vanilla", &wire_call, &[]), None);
    n.rekey("e", "p");
    l.approved.borrow_mut().push("p".into());
    assert_eq!(n.packet(&l, "p", &wire_call, &[]), Some(Inbound::Call(call(7))));
    assert!(matches!(n.packet(&l, "p", &wire_result, &[]), Some(Inbound::Result(r)) if r.id == 7));
    assert!(l.take().is_empty());
    // A server mod's call goes to modded peers in the game only.
    let rc = RemoteCall {
        id: 3,
        target: bhmod_core::remote::RemoteTarget::Player("p".into()),
        mod_id: "srv".into(),
        service: "hud".into(),
        version_req: "*".into(),
        method: "title".into(),
        args: vec![0x80],
    };
    assert_eq!(n.send_service_call(&l, "p", &rc), Ok(()));
    assert_eq!(n.send_service_call(&l, "vanilla", &rc), Err(RemoteError::Unavailable));
    l.approved.borrow_mut().push("vanilla".into());
    assert_eq!(n.send_service_call(&l, "vanilla", &rc), Err(RemoteError::Unavailable), "approved but no HELLO");
    l.approved.borrow_mut().retain(|p| p != "p");
    assert_eq!(n.send_service_call(&l, "p", &rc), Err(RemoteError::Unavailable), "modded but gone");
    let sent = l.take();
    assert_eq!(sent.len(), 1);
    assert!(matches!(&sent[0], Message::ServiceCall(c) if c.id == 3 && c.mod_id == "srv" && c.service == "hud"));
    // Answers: a value and a failure.
    n.send_service_result(&l, "p", 7, Ok(vec![0x18, 0xfa]));
    n.send_service_result(&l, "p", 8, Err(RemoteError::NotPermitted));
    assert_eq!(
        l.take(),
        vec![
            Message::ServiceResult(ServiceResult { id: 7, outcome: Ok(vec![0x18, 0xfa]) }),
            Message::ServiceResult(ServiceResult {
                id: 8,
                outcome: Err(ServiceFailure {
                    code: bhmod_proto::service_error::NOT_PERMITTED,
                    message: String::new()
                })
            }),
        ]
    );
}

#[test]
fn wire_ids_run_out() {
    let l = Fake::default();
    let mut n = ModNet::default();
    for i in 0..u16::MAX {
        assert_eq!(n.wire_id(&format!("m:c{i}")), Some(i + 1));
    }
    assert_eq!(n.wire_id("m:c0"), Some(1));
    assert_eq!(n.wire_id("m:full"), None);
    assert!(matches!(n.send_channel(&l, &[], "m:full", b""), Err(ApiError::LimitExceeded(_))));
    n.announce_to(&l, "p", &["m:full".to_string()]);
    let mut list = ModNet { wire: n.wire.clone(), ..ModNet::default() };
    list.packet(&l, "e", &hello(1), &[]);
    list.packet(&l, "e", &Message::ListRequest.encode(), &["m:full".to_string()]);
    let sent = l.take();
    let Some(Message::ModList(m)) = sent.last() else { unreachable!() };
    assert!(m.channels.is_empty());
}

#[test]
fn loader_check() {
    let l = Fake::default();
    let mut n = net();
    let good = LoaderKey { proto: Some(1), accepted_hash: Some(hex(&n.catalog.mods_hash)) };
    assert_eq!(n.check_loader(&l, "v", Some(&good)), None);
    // Vanilla: no key, no text.
    assert_eq!(n.check_loader(&l, "v", None), Some(3));
    assert!(l.take().is_empty());
    // Modded: stale hash or other proto get a text first.
    n.packet(&l, "m", &hello(1), &[]);
    l.take();
    let stale = LoaderKey { proto: Some(1), accepted_hash: Some("00".into()) };
    assert_eq!(n.check_loader(&l, "m", Some(&stale)), Some(3));
    let old = LoaderKey { proto: Some(0), accepted_hash: None };
    assert_eq!(n.check_loader(&l, "m", Some(&old)), Some(3));
    assert_eq!(n.check_loader(&l, "m", None), Some(3));
    let texts: Vec<String> = l
        .take()
        .into_iter()
        .map(|m| {
            let Message::RejectText(r) = m else { unreachable!() };
            r.text
        })
        .collect();
    assert_eq!(texts, vec![STALE_TEXT.to_string(), proto_text(), STALE_TEXT.to_string()]);
    // Without client-required mods there is nothing to check.
    n.catalog.client_required = false;
    assert_eq!(n.check_loader(&l, "v", None), None);
    assert_eq!(pre_login_text(11), "A server mod refused the join (reason 11).");
}

fn with_cosmetics(n: &mut ModNet) {
    n.content = Content {
        wire: [("cosmetics".to_string(), [("hats:crown".to_string(), 1u64), ("hats:ember".to_string(), 2)].into())]
            .into(),
        cosmetics: [(1, "hat".to_string()), (2, "hair".to_string())].into(),
        ..Content::default()
    };
}

fn cosmetics(ms: &[Message]) -> Vec<(bool, Vec<(String, Vec<(String, u64)>)>)> {
    ms.iter()
        .filter_map(|m| match m {
            Message::Cosmetics(c) => Some((
                c.full,
                c.players.iter().map(|(p, s)| (p.clone(), s.iter().map(|(a, b)| (a.clone(), *b)).collect())).collect(),
            )),
            _ => None,
        })
        .collect()
}

#[test]
fn mod_list_carries_the_registry() {
    let l = Fake::default();
    let mut n = net();
    n.packet(&l, "e", &hello(1), &[]);
    with_cosmetics(&mut n);
    l.take();
    n.packet(&l, "e", &Message::ListRequest.encode(), &[]);
    let sent = l.take();
    let [Message::ModList(m)] = &sent[..] else { unreachable!() };
    assert_eq!(m.registry["cosmetics"]["hats:ember"], 2);
}

#[test]
fn cosmetics_full_delta_and_leave() {
    let l = Fake::default();
    let mut n = net();
    // Without declared cosmetics nothing is sent and equipping is unsupported.
    n.packet(&l, "a", &hello(1), &[]);
    l.take();
    n.cosmetics_full(&l, "a");
    assert!(l.take().is_empty());
    assert_eq!(n.set_cosmetic(&l, "a", "hat", None), Err(ApiError::Unsupported));
    with_cosmetics(&mut n);
    l.approved.borrow_mut().extend(["a".to_string(), "v".to_string()]);
    n.packet(&l, "b", &hello(1), &[]);
    n.rekey("b", "b2");
    l.approved.borrow_mut().push("b2".into());
    // Full: only to a modded peer, empty table at first.
    n.cosmetics_full(&l, "a");
    n.cosmetics_full(&l, "v");
    assert_eq!(cosmetics(&l.take()), vec![(true, vec![])]);
    // Validation.
    assert!(matches!(n.set_cosmetic(&l, "a", "hat", Some(9)), Err(ApiError::InvalidArgument(_))));
    assert!(matches!(n.set_cosmetic(&l, "a", "hat", Some(2)), Err(ApiError::InvalidArgument(_))));
    assert!(l.take().is_empty());
    // A delta goes to every modded peer in the game (not to the vanilla one), including the wearer.
    n.set_cosmetic(&l, "a", "hat", Some(1)).unwrap();
    n.set_cosmetic(&l, "a", "hair", Some(2)).unwrap();
    let sent: Vec<String> = l.sent.borrow().iter().map(|(p, _)| p.clone()).collect();
    assert_eq!(sent, ["a", "b2", "a", "b2"]);
    assert_eq!(
        cosmetics(&l.take()).last().unwrap().1,
        vec![("a".to_string(), vec![("hair".to_string(), 2), ("hat".to_string(), 1)])]
    );
    assert_eq!(n.worn("a"), vec![("hair".to_string(), 2), ("hat".to_string(), 1)]);
    // The next joiner gets the table.
    n.cosmetics_full(&l, "b2");
    assert!(cosmetics(&l.take())[0].0);
    // Taking off, then leaving.
    n.set_cosmetic(&l, "a", "hat", None).unwrap();
    l.take();
    n.player_left(&l, "a");
    assert_eq!(cosmetics(&l.take())[0].1, vec![("a".to_string(), vec![])]);
    assert!(n.worn("a").is_empty());
    n.player_left(&l, "a");
    n.set_cosmetic(&l, "a", "hair", None).unwrap();
    assert!(n.worn("a").is_empty());
    l.take();
    n.set_cosmetic(&l, "a", "hat", Some(1)).unwrap();
    n.set_cosmetic(&l, "a", "hat", None).unwrap();
    assert_eq!(cosmetics(&l.take()).last().unwrap().1, vec![("a".to_string(), vec![])]);
    assert!(n.worn("a").is_empty());
}

#[test]
fn limbo_holds_vanilla_peers_until_the_grace_ends() {
    let mut n = net();
    let now = Instant::now();
    let grace = Duration::from_secs(20);
    // Only a vanilla peer (no HELLO, no key) on a server with client-required mods.
    assert!(n.wants_limbo("v", None));
    assert!(!n.wants_limbo("v", Some(&LoaderKey::default())));
    n.packet(&Fake::default(), "m", &hello(1), &[]);
    assert!(!n.wants_limbo("m", None));
    n.catalog.client_required = false;
    assert!(!n.wants_limbo("v", None));
    // The grace ends exactly at its deadline; either name ends the limbo once.
    n.enter_limbo("e1", "p1", now, grace);
    n.enter_limbo("e2", "p2", now + grace, grace);
    n.enter_limbo("e3", "p3", now, grace);
    assert!(n.expired_limbo(now + grace - Duration::from_millis(1)).is_empty());
    assert_eq!(n.expired_limbo(now + grace), vec!["p1".to_string(), "p3".to_string()]);
    assert_eq!(n.expired_limbo(now + grace * 2), vec!["p2".to_string()]);
    assert!(n.expired_limbo(now + grace * 3).is_empty());
    n.enter_limbo("e4", "p4", now, grace);
    n.enter_limbo("e5", "p5", now, grace);
    assert_eq!(n.leave_limbo("e4"), Some("p4".to_string()));
    assert_eq!(n.leave_limbo("p5"), Some("p5".to_string()));
    assert_eq!(n.leave_limbo("p5"), None);
}

#[test]
fn voice_and_settings_features() {
    use bhmod_proto::{feature, SettingsSet, VoiceCtrl, VoiceKey};
    let link = Fake::default();
    let mut n = net();
    let with_features = |f: &[&str]| {
        Message::Hello(Hello {
            proto: PROTO,
            loader: "0.1.0".into(),
            platform: "pc-linux-x86_64".into(),
            api: "0.3.4".into(),
            runtimes: vec![],
            cached: vec![],
            features: f.iter().map(|s| s.to_string()).collect(),
        })
        .encode()
    };
    // Without a voice or settings mod: no feature, no port, even when one is set.
    n.voice_udp_port = Some(24_000);
    n.packet(&link, "p", &with_features(&["voice", "settings"]), &[]);
    let Message::ServerHello(h) = &link.take()[0] else { panic!("no SERVER_HELLO") };
    assert!(h.features.is_empty() && h.voice_udp_port.is_none());
    assert_eq!(n.features_of("p"), ["voice", "settings"]);
    assert!(n.features_of("nobody").is_empty());
    // The ops are ignored while not served.
    link.approved.borrow_mut().push("p".into());
    let key = Message::VoiceKey(VoiceKey { public: [1; 32], peer: None, udp_port: None }).encode();
    let set = SettingsSet { mod_id: "m".into(), key: "k".into(), value: vec![0xf5] };
    let set_packet = Message::SettingsSet(set.clone()).encode();
    assert_eq!(n.packet(&link, "p", &key, &[]), None);
    assert_eq!(n.packet(&link, "p", &set_packet, &[]), None);
    // Served: handed to the host, only for players in the game.
    n.features = vec![feature::VOICE.into(), feature::SETTINGS.into()];
    assert!(n.serves(feature::VOICE) && !n.serves("other"));
    assert!(matches!(n.packet(&link, "p", &key, &[]), Some(Inbound::Voice(Message::VoiceKey(_)))));
    let ctrl = Message::VoiceCtrl(VoiceCtrl { self_mute: true, deafen: false }).encode();
    assert!(matches!(n.packet(&link, "p", &ctrl, &[]), Some(Inbound::Voice(Message::VoiceCtrl(_)))));
    assert_eq!(n.packet(&link, "p", &set_packet, &[]), Some(Inbound::SettingsSet(set)));
    n.packet(&link, "q", &with_features(&[]), &[]);
    let Message::ServerHello(h) = &link.take()[0] else { panic!("no SERVER_HELLO") };
    assert_eq!(
        (h.features.clone(), h.voice_udp_port),
        (vec!["voice".to_string(), "settings".to_string()], Some(24_000))
    );
    assert_eq!(n.packet(&link, "q", &key, &[]), None);
    assert_eq!(n.packet(&link, "q", &set_packet, &[]), None);
    // Settings alone: the port is not announced.
    n.features = vec![feature::SETTINGS.into()];
    n.packet(&link, "r", &with_features(&[]), &[]);
    let Message::ServerHello(h) = &link.take()[0] else { panic!("no SERVER_HELLO") };
    assert_eq!(h.voice_udp_port, None);
    // has-mod: only what READY listed.
    assert!(!n.has_mod("p", "voice") && !n.has_mod("vanilla", "voice"));
    let ready = Message::Ready(Ready { accepted_hash: [0; 32], loaded: vec![("voice".into(), "0.1.0".into())] });
    n.packet(&link, "p", &ready.encode(), &[]);
    assert!(n.has_mod("p", "voice") && !n.has_mod("p", "other"));
}
