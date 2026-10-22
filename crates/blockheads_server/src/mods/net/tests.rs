use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use bhmod_proto::{mods_hash, FileRequest, Hello, ModInfo, ModPermissions, Ready, CHUNK_SIZE, PACKET_TYPE};

use super::*;
use crate::mods::catalog::{Bytes, Entry};

#[derive(Default)]
struct Fake {
    sent: RefCell<Vec<(String, Vec<u8>)>>,
    rejected: RefCell<Vec<(String, u8)>>,
    approved: RefCell<Vec<String>>,
    waiting: RefCell<Vec<String>>,
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
        self.approved(peer) || self.waiting.borrow().iter().any(|p| p == peer)
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
    let l = Fake::default();
    let mut n = net();
    // Nothing before HELLO; garbage and unknown ops are dropped.
    for p in [Message::ListRequest.encode(), vec![PACKET_TYPE, 0x7f], vec![PACKET_TYPE], vec![1, 2]] {
        assert_eq!(n.packet(&l, "e", &p, &[]), None);
    }
    assert!(l.take().is_empty());
    n.packet(&l, "e", &hello(1), &[]);
    assert!(n.is_modded("e") && n.has_peers());
    match &l.take()[..] {
        [Message::ServerHello(h)] => {
            assert!(h.client_required);
            assert_eq!(h.count, 1);
            assert_eq!(h.mods_hash, n.catalog.mods_hash);
        }
        o => panic!("{o:?}"),
    }
    n.packet(&l, "e", &Message::ListRequest.encode(), &["big:sync".to_string()]);
    match &l.take()[..] {
        [Message::ModList(m)] => {
            assert_eq!(m.mods[0].id, "big");
            assert_eq!(m.channels, vec![ChannelInfo { name: "big:sync".into(), id: 1 }]);
        }
        o => panic!("{o:?}"),
    }
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
    let want = "bc08a36375726c783468747470733a2f2f626c6f636b68656164732d636c75622e6769746875622e696f2f776562736974652f646f63732f6d6f64732f6474657874784d5468697320736572766572206e65656473206d6f64206c6f616465722070726f746f636f6c20312e2055706461746520796f7572206d6f64206c6f61646572207769746820506174636865722e6a726561736f6e436f646503";
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
    assert_eq!(n.packet(&l, "p", &ch(1, b"x"), &[]), Some(("m:a".to_string(), b"x".to_vec())));
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
    match l.take().last() {
        Some(Message::ModList(m)) => assert!(m.channels.is_empty()),
        o => panic!("{o:?}"),
    }
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
        .map(|m| match m {
            Message::RejectText(r) => r.text,
            o => panic!("{o:?}"),
        })
        .collect();
    assert_eq!(texts, vec![STALE_TEXT.to_string(), proto_text(), STALE_TEXT.to_string()]);
    // Without client-required mods there is nothing to check.
    n.catalog.client_required = false;
    assert_eq!(n.check_loader(&l, "v", None), None);
    assert_eq!(pre_login_text(11), "A server mod refused the join (reason 11).");
}
