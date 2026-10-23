//! The 0xBC mod protocol on a live server (in-process), driven by the test client in vanilla and modded mode:
//! detection, mod list, downloads with resume, the `bhcModLoader` join check, readable rejections and channels
//! between a server mod and a modded client. Packets the spec pins byte for byte are compared with the mod-sdk
//! vectors (copied below; `BHMOD_SDK_DIR` = a mod-sdk checkout also checks the copies against its `vectors/`).

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use bh_testclient::{ClientConfig, Identity, JoinOutcome, LoaderOptions, ModClient, Session};
use bhmod_proto::{hex, FileRequest, Message, ModInfo, CHUNK_SIZE, PACKET_TYPE};
use blockheads_server::host::{self, HostConfig, ServerHandle, Status, WorldSelect};
use blockheads_server::mods::{ModsConfig, LIMBO_TEXT};
use blockheads_server::plist::value::PlistValue;
use blockheads_server::protocol::messages::{decode_server_packet, ServerMsg};

/// mod-sdk `vectors/*.hex` this test compares with (mod-sdk ca82eaa).
const VECTORS: &[(&str, &str)] = &[
    ("01-server-hello-server-only", "bc01a465636f756e74016570726f746f01686d6f6473486173685820e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b8556e636c69656e745265717569726564f4"),
    ("06-file-error", "bc06a264636f646501667368613235365820ca3704aa0b06f5954c79ee837faa152d84d6b2d42838f0637a15eda8337dbdce"),
    ("08-reject-text", "bc08a36375726c782968747470733a2f2f626c6f636b68656164732e636c75622f646f63732f6d6f64732f706c61796572736474657874784d5468697320736572766572206e65656473206d6f64206c6f616465722070726f746f636f6c20312e2055706461746520796f7572206d6f64206c6f61646572207769746820506174636865722e6a726561736f6e436f646503"),
];
/// `vectors.json` `vanillaReject.packet`.
const VANILLA_REJECT: &str = "2603";

fn vector(name: &str) -> Vec<u8> {
    let h = VECTORS.iter().find(|(n, _)| *n == name).unwrap().1;
    (0..h.len()).step_by(2).map(|i| u8::from_str_radix(&h[i..i + 2], 16).unwrap()).collect()
}

fn hex_bytes(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// Ends the test process when a test hangs (each test runs well under its budget).
struct Watchdog(#[allow(dead_code)] mpsc::Sender<()>);

fn watchdog(name: &'static str, secs: u64) -> Watchdog {
    let (tx, rx) = mpsc::channel::<()>();
    std::thread::spawn(move || {
        if let Err(mpsc::RecvTimeoutError::Timeout) = rx.recv_timeout(Duration::from_secs(secs)) {
            eprintln!("watchdog: {name} ran longer than {secs} s");
            std::process::exit(101);
        }
    });
    Watchdog(tx)
}

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for e in std::fs::read_dir(from).unwrap().flatten() {
        let p = e.path();
        if p.is_dir() {
            copy_dir(&p, &to.join(e.file_name()));
        } else {
            std::fs::copy(&p, to.join(e.file_name())).unwrap();
        }
    }
}

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/mods").join(name)
}

struct Running {
    handle: ServerHandle,
    lines: Arc<Mutex<Vec<String>>>,
    port: u16,
}

impl Running {
    fn log(&self) -> String {
        self.lines.lock().unwrap().join("\n")
    }
    fn session(&self, alias: &str) -> Session {
        let cfg = ClientConfig { addr: SocketAddr::from(([127, 0, 0, 1], self.port)), ..Default::default() };
        Session::connect(cfg, Identity::new(alias)).unwrap()
    }
}

fn start(data: &Path, world: &str, port: u16, mods: &Path) -> Running {
    let lines = Arc::new(Mutex::new(Vec::<String>::new()));
    let l2 = lines.clone();
    let cfg = HostConfig {
        data_dir: Some(data.to_path_buf()),
        world: WorldSelect::Load(world.to_string()),
        port: Some(port),
        mods: ModsConfig { dir: Some(mods.to_path_buf()), disabled: false },
        ..HostConfig::default()
    };
    let handle = host::start(cfg, Arc::new(move |s| l2.lock().unwrap().push(s.to_string()))).unwrap();
    let end = Instant::now() + Duration::from_secs(90);
    while handle.status() == Status::Starting && Instant::now() < end {
        std::thread::sleep(Duration::from_millis(50));
    }
    assert_eq!(handle.status(), Status::Running, "{:#?}", lines.lock().unwrap());
    Running { handle, lines, port }
}

fn world(tmp: &Path) -> (PathBuf, String) {
    let data = tmp.join("TheBlockheads");
    let w = host::create_world(Some(&data), "Proto", 104, false).expect("create");
    (data, w.id)
}

fn no_mod_bytes(s: &Session) {
    assert!(!s.received().iter().any(|rx| rx.data.first() == Some(&PACKET_TYPE)), "a vanilla client got 0xBC");
}

fn mod_packets(s: &Session, op: u8) -> Vec<Vec<u8>> {
    s.received()
        .iter()
        .filter(|rx| rx.data.len() > 1 && rx.data[0] == PACKET_TYPE && rx.data[1] == op)
        .map(|rx| rx.data.clone())
        .collect()
}

#[test]
fn vectors_match_the_sdk_checkout() {
    let Some(dir) = std::env::var_os("BHMOD_SDK_DIR") else {
        eprintln!("SKIP: BHMOD_SDK_DIR not set");
        return;
    };
    for (name, hex) in VECTORS {
        let f = Path::new(&dir).join("vectors").join(format!("{name}.hex"));
        assert_eq!(std::fs::read_to_string(&f).unwrap().trim(), *hex, "{}", f.display());
    }
    let json = std::fs::read_to_string(Path::new(&dir).join("vectors/vectors.json")).unwrap();
    assert!(json.contains(&format!("\"packet\": \"{VANILLA_REJECT}\"")));
}

#[test]
fn server_only_mods_admit_vanilla_and_modded_clients() {
    let _w = watchdog("server_only_mods", 180);
    let tmp = tempfile::tempdir().unwrap();
    let (data, wid) = world(tmp.path());
    let mods = tmp.path().join("mods");
    copy_dir(&fixture("hello-lua"), &mods.join("hello-lua"));
    std::fs::write(mods.join("mods.toml"), "[hello-lua]\ngrant = [\"players:chat\"]\n").unwrap();
    let mut srv = start(&data, &wid, 15460, &mods);

    let mut v = srv.session("VANILLA");
    assert_eq!(v.join(10_000).unwrap(), JoinOutcome::Approved, "{}", srv.log());
    v.run_for(300).unwrap();
    no_mod_bytes(&v);

    let cache = tmp.path().join("cache");
    let mut s = srv.session("MODDER");
    let mut mc = ModClient::new(LoaderOptions::new(&cache));
    assert_eq!(mc.join(&mut s, 10_000).unwrap(), JoinOutcome::Approved, "{}", srv.log());
    // SERVER_HELLO: one server-only mod listed, empty modsHash, nothing required (byte-exact vector).
    assert_eq!(mod_packets(&s, 0x01), vec![vector("01-server-hello-server-only")]);
    assert!(mc.list.is_none() && mc.downloads.is_empty());
    // The list is still available; hello-lua is information only and cannot be downloaded.
    let list = mc.request_list(&mut s, 5000).unwrap();
    assert_eq!(list.mods.len(), 1);
    assert_eq!(list.mods[0].sides, ["server"]);
    let m: ModInfo = list.mods[0].clone();
    assert!(!mc.download(&mut s, &m, 5000).unwrap());
    assert_eq!(mc.file_errors[0].code, bhmod_proto::file_error::NOT_FOUND);
    assert_eq!(srv.handle.stop(), Status::Stopped(0));
}

/// A mods directory with chan-echo (client + server, channels) and a client-only mod with an incompressible data
/// file of `big` bytes (several chunks).
fn client_mods(root: &Path, big: usize) -> PathBuf {
    let mods = root.join("mods");
    copy_dir(&fixture("chan-echo"), &mods.join("chan-echo"));
    let skin = mods.join("big-skin");
    std::fs::create_dir_all(skin.join("client")).unwrap();
    std::fs::create_dir_all(skin.join("data")).unwrap();
    let toml = std::fs::read_to_string(fixture("chan-echo").join("mod.toml"))
        .unwrap()
        .replace("chan-echo", "big-skin")
        .replace("Channel echo", "Big skin")
        .replace("server = true", "server = false")
        .replace("server = { runtime = \"lua\", path = \"server/main.luau\" }\n", "")
        .replace("channels = true", "channels = false");
    std::fs::write(skin.join("mod.toml"), toml).unwrap();
    std::fs::write(skin.join("client/main.luau"), "return {}").unwrap();
    let mut x: u32 = 12345;
    let blob: Vec<u8> = (0..big)
        .map(|_| {
            x ^= x << 13;
            x ^= x >> 17;
            x ^= x << 5;
            x as u8
        })
        .collect();
    std::fs::write(skin.join("data/blob.bin"), blob).unwrap();
    // A native client mod is never offered.
    let nat = mods.join("nat");
    std::fs::create_dir_all(nat.join("client")).unwrap();
    let toml = std::fs::read_to_string(skin.join("mod.toml"))
        .unwrap()
        .replace("big-skin", "nat")
        .replace("channels = false", "native = true");
    std::fs::write(nat.join("mod.toml"), toml).unwrap();
    std::fs::write(nat.join("client/main.luau"), "return {}").unwrap();
    std::fs::write(mods.join("mods.toml"), "[chan-echo]\ngrant = [\"channels\"]\n").unwrap();
    mods
}

#[test]
fn client_mods_reject_vanilla_and_serve_modded_clients() {
    let _w = watchdog("client_mods", 240);
    let tmp = tempfile::tempdir().unwrap();
    let (data, wid) = world(tmp.path());
    let mods = client_mods(tmp.path(), 200_000);
    let mut srv = start(&data, &wid, 15461, &mods);
    assert!(srv.log().contains("offering 2 client mod(s) to modded clients (required)"), "{}", srv.log());
    assert!(srv.log().contains("not offering nat to clients: native client mods are never pushed"), "{}", srv.log());

    // Vanilla client: held in the limbo. It gets the world summary with the readable welcomeMessage (the vanilla
    // game shows it as its welcome popup), is not a player and never sees a 0xBC byte; its next packet ends it
    // with a plain disconnect (no 0x26).
    let mut v = srv.session("VANILLA");
    assert_eq!(v.join(10_000).unwrap(), JoinOutcome::Approved);
    let summary = v.received().iter().find(|rx| rx.data[0] == 0x01).unwrap().data.clone();
    let ServerMsg::Summary(Some(PlistValue::Dict(d))) = decode_server_packet(&summary).unwrap() else { panic!() };
    assert_eq!(d.map.get(&"welcomeMessage".to_string()).and_then(|v| v.as_str()), Some(LIMBO_TEXT));
    assert_eq!(d.map.get(&"saveID".to_string()).and_then(|v| v.as_str()), Some(wid.as_str()));
    assert!(d.map.get(&"owner".to_string()).is_none() && d.map.get(&"admin".to_string()).is_none());
    v.run_for(500).unwrap();
    assert_eq!(v.count_of(0x26), 0, "dropped before the grace ended");
    assert_eq!(v.count_of(0x17) + v.count_of(0x05), 0, "a limbo peer is not a player");
    v.request_initial_data().unwrap();
    v.run_for(1500).unwrap();
    assert!(v.state.disconnected, "dropped by its next packet");
    assert_eq!(v.count_of(0x26), 0, "no rejection reason");
    no_mod_bytes(&v);

    // A vanilla client that leaves while in the limbo leaves nothing behind: it can join (limbo) again.
    let mut v = srv.session("LEAVER");
    assert_eq!(v.join(10_000).unwrap(), JoinOutcome::Approved);
    v.disconnect();
    std::thread::sleep(Duration::from_millis(1500));
    let mut v = srv.session("LEAVER");
    assert_eq!(v.join(10_000).unwrap(), JoinOutcome::Approved);
    v.disconnect();

    // Protocol mismatch: REJECT_TEXT then 0x26 03, both byte-exact.
    let mut s = srv.session("OLDLOADER");
    let mut opts = LoaderOptions::new(&tmp.path().join("c0"));
    opts.proto = 2;
    let mut mc = ModClient::new(opts);
    assert_eq!(mc.join(&mut s, 10_000).unwrap(), JoinOutcome::Rejected { reason: 3 });
    let tail: Vec<String> = s.received().iter().rev().take(2).rev().map(|rx| hex_bytes(&rx.data)).collect();
    assert_eq!(tail, [hex_bytes(&vector("08-reject-text")), VANILLA_REJECT.to_string()]);

    // Unknown file: FILE_ERROR NOT_FOUND (byte-exact).
    let mut s = srv.session("PROBE");
    let mut mc = ModClient::new(LoaderOptions::new(&tmp.path().join("c1")));
    assert!(s.wait_for(0x23, 10_000).unwrap().is_some());
    s.client.send(&mc.hello().encode()).unwrap();
    s.wait_for_matching(5000, &mut |rx| rx.data.starts_with(&[0xbc, 0x01])).unwrap().unwrap();
    let unknown = vector("06-file-error")[18..50].try_into().unwrap();
    s.client.send(&Message::FileRequest(FileRequest { sha256: unknown, offset: 0 }).encode()).unwrap();
    let e = s.wait_for_matching(5000, &mut |rx| rx.data.starts_with(&[0xbc, 0x06])).unwrap().unwrap();
    assert_eq!(e.data, vector("06-file-error"));
    let list = mc.request_list(&mut s, 5000).unwrap();
    assert_eq!(list.mods.iter().map(|m| m.id.as_str()).collect::<Vec<_>>(), ["big-skin", "chan-echo"]);
    assert_eq!(list.channels.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(), ["chan-echo:echo"]);
    s.disconnect();

    // Declined mods: REJECT_TEXT (stale) then reason 3.
    let mut s = srv.session("DECLINER");
    let mut opts = LoaderOptions::new(&tmp.path().join("c2"));
    opts.accept = false;
    let mut mc = ModClient::new(opts);
    assert_eq!(mc.join(&mut s, 10_000).unwrap(), JoinOutcome::Rejected { reason: 3 });
    assert!(mc.reject.as_ref().unwrap().text.contains("not accepted"), "{:?}", mc.reject);

    // A stale acceptedHash.
    let mut s = srv.session("STALE");
    s.id = s.id.clone().mod_loader(1, &"00".repeat(32));
    let mc = ModClient::new(LoaderOptions::new(&tmp.path().join("c3")));
    assert!(s.wait_for(0x23, 10_000).unwrap().is_some());
    s.client.send(&mc.hello().encode()).unwrap();
    s.wait_for_matching(5000, &mut |rx| rx.data.starts_with(&[0xbc, 0x01])).unwrap().unwrap();
    s.send_player_info().unwrap();
    let r = s.wait_for(0x26, 5000).unwrap().unwrap();
    assert_eq!(r.data, [0x26, 3]);
    assert_eq!(mod_packets(&s, 0x08).len(), 1);

    // The modded client: list, download, verify, READY, join.
    let cache = tmp.path().join("cache");
    let mut s = srv.session("MODDER");
    let mut mc = ModClient::new(LoaderOptions::new(&cache));
    assert_eq!(mc.join(&mut s, 10_000).unwrap(), JoinOutcome::Approved, "{}", srv.log());
    assert_eq!(mc.downloads.len(), 2);
    assert!(mc.downloads.iter().all(|d| d.complete && d.resumed_from == 0));
    let big = mc.downloads.iter().map(|d| d.chunk_offsets.len()).max().unwrap();
    assert_eq!(big, 7, "{:?}", mc.downloads);
    assert_eq!(mc.cached().len(), 2);
    for d in &mc.downloads {
        let bytes = std::fs::read(cache.join(format!("{}.bhmod", hex(&d.sha256)))).unwrap();
        use sha2::Digest;
        assert_eq!(<[u8; 32]>::from(sha2::Sha256::digest(&bytes)), d.sha256);
    }

    // Channels: echo to the sender, broadcast to every modded player, a channel opened later.
    mc.send_channel(&mut s, "chan-echo:echo", b"hi").unwrap();
    assert_eq!(mc.wait_channel(&mut s, 5000).unwrap(), Some(("chan-echo:echo".into(), b"echo:hi".to_vec())));
    s.chat("/chanall").unwrap();
    // The mod's reply: sending to "server" from a server mod failed (as it must).
    let reply = s.wait_for_matching(5000, &mut |rx| rx.data.windows(4).any(|w| w == b"sent")).unwrap().unwrap();
    assert!(!reply.data.windows(6).any(|w| w == b"server"));
    assert_eq!(mc.wait_channel(&mut s, 5000).unwrap(), Some(("chan-echo:echo".into(), b"to all".to_vec())));
    s.chat("/chanlate").unwrap();
    s.wait_for_matching(5000, &mut |rx| rx.data.starts_with(&[0xbc, 0x11])).unwrap().expect("CHANNEL_REGISTER");
    mc.send_channel(&mut s, "chan-echo:late", b"x").unwrap();
    assert_eq!(mc.wait_channel(&mut s, 5000).unwrap(), Some(("chan-echo:late".into(), b"late:x".to_vec())));
    // Unknown channel ids are dropped; the link stays up.
    s.client.send(&[0xbc, 0x10, 0x63, 0x00, 1]).unwrap();
    mc.send_channel(&mut s, "chan-echo:echo", b"again").unwrap();
    assert_eq!(mc.wait_channel(&mut s, 5000).unwrap().unwrap().1, b"echo:again");

    // A second join with everything cached downloads nothing.
    let mut s2 = srv.session("MODDER2");
    let mut mc2 = ModClient::new(LoaderOptions::new(&cache));
    assert_eq!(mc2.join(&mut s2, 10_000).unwrap(), JoinOutcome::Approved);
    assert!(mc2.downloads.is_empty());

    // A server mod refusing the join (player-pre-login): REJECT_TEXT, then its reason.
    let mut s3 = srv.session("NOPE");
    let mut mc3 = ModClient::new(LoaderOptions::new(&cache));
    assert_eq!(mc3.join(&mut s3, 10_000).unwrap(), JoinOutcome::Rejected { reason: 12 });
    assert_eq!(mc3.reject.as_ref().unwrap().reason_code, 12);

    s.disconnect();
    s2.disconnect();
    assert_eq!(srv.handle.stop(), Status::Stopped(0), "{}", srv.log());
    // bhcModLoader never reaches the saved player info.
    let save = data.join("saves").join(&wid);
    let mut found_player = false;
    for f in walk(&save) {
        let b = std::fs::read(&f).unwrap();
        assert!(!contains(&b, b"bhcModLoader"), "{}", f.display());
        found_player |= contains(&b, b"udid-MODDER");
    }
    assert!(found_player, "player info not found in the save");
}

fn walk(p: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for e in std::fs::read_dir(p).unwrap().flatten() {
        let q = e.path();
        if q.is_dir() {
            out.extend(walk(&q));
        } else {
            out.push(q);
        }
    }
    out
}

fn contains(hay: &[u8], needle: &[u8]) -> bool {
    hay.windows(needle.len()).any(|w| w == needle)
}

#[test]
fn interrupted_download_resumes() {
    let _w = watchdog("resume", 180);
    let tmp = tempfile::tempdir().unwrap();
    let (data, wid) = world(tmp.path());
    let mods = client_mods(tmp.path(), 6 * CHUNK_SIZE);
    let mut srv = start(&data, &wid, 15462, &mods);
    let cache = tmp.path().join("cache");

    // Interrupted once 100000 bytes are held (chunks arrive whole: 4 chunks).
    let mut s = srv.session("RESUMER");
    let mut opts = LoaderOptions::new(&cache);
    opts.interrupt_after = Some(100_000);
    let mut mc = ModClient::new(opts.clone());
    assert_eq!(mc.join(&mut s, 10_000).unwrap(), JoinOutcome::Disconnected);
    let partial = mc.downloads.last().unwrap().clone();
    assert!(!partial.complete);
    let held = partial.chunk_offsets.len() as u64 * CHUNK_SIZE as u64;
    s.disconnect();

    // A new connection resumes from the bytes held (a multiple of the chunk size).
    let mut s = srv.session("RESUMER");
    opts.interrupt_after = None;
    let mut mc = ModClient::new(opts);
    assert_eq!(mc.join(&mut s, 10_000).unwrap(), JoinOutcome::Approved, "{}", srv.log());
    let d = mc.downloads.iter().find(|d| d.sha256 == partial.sha256).unwrap();
    assert!(d.complete);
    assert_eq!(d.resumed_from, held);
    assert_eq!(d.chunk_offsets[0], held);
    s.disconnect();
    assert_eq!(srv.handle.stop(), Status::Stopped(0));
}

#[test]
fn modded_client_on_a_vanilla_server() {
    let _w = watchdog("vanilla_server", 180);
    let tmp = tempfile::tempdir().unwrap();
    let (data, wid) = world(tmp.path());
    // No mods directory: the server has no mod host and drops 0xBC before approval like the original.
    let mut srv = start(&data, &wid, 15463, &tmp.path().join("no-mods"));
    let mut s = srv.session("MODDER");
    let mut mc = ModClient::new(LoaderOptions::new(&tmp.path().join("cache")));
    assert_eq!(mc.join(&mut s, 10_000).unwrap(), JoinOutcome::Approved);
    assert!(mc.server.is_none());
    s.run_for(300).unwrap();
    no_mod_bytes(&s);
    assert_eq!(srv.handle.stop(), Status::Stopped(0));
}

enum Act {
    Send(Vec<u8>),
    Disconnect,
}

/// A scripted ENet server: `script(None)` on connect, `script(Some(packet))` on every packet.
fn fake_server(port: u16, mut script: impl FnMut(Option<&[u8]>) -> Vec<Act> + Send + 'static) {
    use rusty_enet as enet;
    let socket = std::net::UdpSocket::bind(("127.0.0.1", port)).unwrap();
    let settings = enet::HostSettings { peer_limit: 4, channel_limit: 8, ..Default::default() };
    let mut host = enet::Host::new(socket, settings).unwrap();
    std::thread::spawn(move || {
        let end = Instant::now() + Duration::from_secs(20);
        while Instant::now() < end {
            while let Ok(Some(ev)) = host.service() {
                let (peer, acts) = match ev {
                    enet::Event::Connect { peer, .. } => (peer.id(), script(None)),
                    enet::Event::Receive { peer, packet, .. } => {
                        let id = peer.id();
                        let data = packet.data().to_vec();
                        (id, script(Some(&data)))
                    }
                    enet::Event::Disconnect { .. } => return,
                };
                for a in acts {
                    let Some(p) = host.get_peer_mut(peer) else { continue };
                    match a {
                        Act::Send(d) => {
                            let _ = p.send(0, &enet::Packet::reliable(d.as_slice()));
                        }
                        Act::Disconnect => p.disconnect(0),
                    }
                }
            }
            std::thread::sleep(Duration::from_millis(1));
        }
    });
}

fn fake_session(port: u16) -> Session {
    let cfg = ClientConfig { addr: SocketAddr::from(([127, 0, 0, 1], port)), ..Default::default() };
    Session::connect(cfg, Identity::new("FAKE")).unwrap()
}

fn contact() -> Vec<u8> {
    blockheads_server::protocol::messages::initial_contact(None, false, "world")
}

/// A fake modded server that lists one client mod of `size` bytes with hash `sha` and answers the first
/// `FILE_REQUEST` with `chunks`.
fn fake_modded(port: u16, sha: [u8; 32], size: u64, chunks: Vec<Vec<u8>>) {
    use bhmod_proto::{ModList, ModPermissions, ServerHello};
    let mut chunks = Some(chunks);
    fake_server(port, move |p| match p.and_then(|p| Message::decode(p).ok()) {
        None if p.is_none() => vec![Act::Send(contact())],
        Some(Message::Hello(_)) => {
            vec![Act::Send(
                Message::ServerHello(ServerHello { proto: 1, mods_hash: [0; 32], client_required: true, count: 1 })
                    .encode(),
            )]
        }
        Some(Message::ListRequest) => {
            let m = ModInfo {
                id: "fake".into(),
                version: "1.0.0".into(),
                name: Default::default(),
                description: Default::default(),
                sides: vec!["client".into()],
                client_required: true,
                sha256: sha,
                size,
                permissions: ModPermissions::default(),
                source_url: None,
                index_listed: false,
            };
            vec![Act::Send(Message::ModList(ModList { mods: vec![m], ..ModList::default() }).encode())]
        }
        Some(Message::FileRequest(_)) => chunks.take().unwrap_or_default().into_iter().map(Act::Send).collect(),
        _ => vec![],
    });
}

fn chunk(sha: [u8; 32], offset: u64, total: u64, data: &[u8]) -> Vec<u8> {
    Message::FileChunk(bhmod_proto::FileChunk { sha256: sha, offset, total, data: data.to_vec() }).encode()
}

#[test]
fn modded_client_edge_cases() {
    let _w = watchdog("edge_cases", 120);
    let tmp = tempfile::tempdir().unwrap();
    // No initial contact.
    fake_server(15470, |_| vec![]);
    let mut s = fake_session(15470);
    let mut mc = ModClient::new(LoaderOptions::new(&tmp.path().join("a")));
    assert_eq!(mc.join(&mut s, 300).unwrap(), JoinOutcome::NoContact);

    // A vanilla server that drops the link after 0x1f.
    fake_server(15471, |p| match p {
        None => vec![Act::Send(contact())],
        Some(d) if d[0] == 0x1f => vec![Act::Disconnect],
        _ => vec![],
    });
    let mut s = fake_session(15471);
    let mut opts = LoaderOptions::new(&tmp.path().join("b"));
    opts.hello_timeout_ms = 300;
    let mut mc = ModClient::new(opts);
    assert_eq!(mc.join(&mut s, 3000).unwrap(), JoinOutcome::Disconnected);

    // A modded server that never answers the 0x1f.
    fake_server(15474, |p| if p.is_none() { vec![Act::Send(contact())] } else { vec![] });
    let mut s = fake_session(15474);
    let mut opts = LoaderOptions::new(&tmp.path().join("b2"));
    opts.hello_timeout_ms = 100;
    let mut mc = ModClient::new(opts);
    assert_eq!(mc.join(&mut s, 300).unwrap(), JoinOutcome::NoContact);

    // A chunk at the wrong offset (after one for another file, which is skipped).
    let sha = [9u8; 32];
    let total = 40_000u64;
    fake_modded(15472, sha, total, vec![chunk([8; 32], 0, 5, b"other"), chunk(sha, 32768, total, &[0; 100])]);
    let mut s = fake_session(15472);
    let mut mc = ModClient::new(LoaderOptions::new(&tmp.path().join("c")));
    let e = mc.join(&mut s, 3000).unwrap_err();
    assert!(e.to_string().contains("chunk at 32768"), "{e}");

    // A package whose bytes do not match its hash; it is not kept.
    fake_modded(15473, sha, 4, vec![chunk(sha, 0, 4, b"oops")]);
    let mut s = fake_session(15473);
    let cache = tmp.path().join("d");
    let mut mc = ModClient::new(LoaderOptions::new(&cache));
    let e = mc.join(&mut s, 3000).unwrap_err();
    assert!(e.to_string().contains("SHA-256 mismatch"), "{e}");
    assert!(std::fs::read_dir(&cache).unwrap().next().is_none());

    // A malformed SERVER_HELLO, a missing MOD_LIST, a stalled download.
    fake_server(15475, |p| match p {
        None => vec![Act::Send(contact())],
        Some(d) if d[0] == PACKET_TYPE => vec![Act::Send(vec![PACKET_TYPE, 0x01, 0xff])],
        _ => vec![],
    });
    let mut s = fake_session(15475);
    let e = ModClient::new(LoaderOptions::new(&tmp.path().join("e"))).join(&mut s, 3000).unwrap_err();
    assert!(e.to_string().contains("0xBC"), "{e}");
    fake_server(15476, |p| match p.and_then(|p| Message::decode(p).ok()) {
        None if p.is_none() => vec![Act::Send(contact())],
        Some(Message::Hello(_)) => vec![Act::Send(
            Message::ServerHello(bhmod_proto::ServerHello {
                proto: 1,
                mods_hash: [0; 32],
                client_required: true,
                count: 0,
            })
            .encode(),
        )],
        _ => vec![],
    });
    let mut s = fake_session(15476);
    let e = ModClient::new(LoaderOptions::new(&tmp.path().join("f"))).join(&mut s, 300).unwrap_err();
    assert!(e.to_string().contains("no MOD_LIST"), "{e}");
    fake_modded(15477, sha, 4, vec![]);
    let mut s = fake_session(15477);
    let e = ModClient::new(LoaderOptions::new(&tmp.path().join("g"))).join(&mut s, 300).unwrap_err();
    assert!(e.to_string().contains("stalled"), "{e}");

    // Channels: an unknown name cannot be sent, an unknown id cannot be named.
    fake_server(15478, |p| match p {
        None => vec![Act::Send(Message::Channel(bhmod_proto::Channel { channel_id: 99, payload: vec![] }).encode())],
        _ => vec![],
    });
    let mut s = fake_session(15478);
    let mut mc = ModClient::new(LoaderOptions::new(&tmp.path().join("h")));
    assert!(mc.send_channel(&mut s, "x:y", b"").unwrap_err().to_string().contains("not known"));
    assert!(mc.wait_channel(&mut s, 3000).unwrap_err().to_string().contains("unknown channel id 99"));
    assert_eq!(mc.wait_channel(&mut s, 50).unwrap(), None);

    // Cache entries that are not package hashes are ignored.
    std::fs::write(cache.join("short.bhmod"), b"").unwrap();
    std::fs::write(cache.join(format!("{}.bhmod", "zz".repeat(32))), b"").unwrap();
    assert!(ModClient::new(LoaderOptions::new(&cache)).cached().is_empty());
}

/// An original 1x1 PNG for the content fixture (not game art).
const TINY_PNG: &[u8] = &[
    0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0, 0, 0, 0x0d, 0x49, 0x48, 0x44, 0x52, 0, 0, 0, 1, 0, 0, 0, 1, 8,
    6, 0, 0, 0, 0x1f, 0x15, 0xc4, 0x89, 0, 0, 0, 0x0d, 0x49, 0x44, 0x41, 0x54, 0x78, 0xda, 0x63, 0xf8, 0xcf, 0xc0,
    0xf0, 0x1f, 0, 5, 0, 2, 1, 0xa5, 0xf6, 0x45, 0x40, 0, 0, 0, 0, 0x49, 0x45, 0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
];

/// A content mod with a client half (cosmetics) and a server half that equips them with commands.
fn hats_mod(mods: &Path) {
    let hats = mods.join("hats");
    std::fs::create_dir_all(hats.join("client")).unwrap();
    std::fs::create_dir_all(hats.join("server")).unwrap();
    std::fs::create_dir_all(hats.join("assets")).unwrap();
    std::fs::write(
        hats.join("mod.toml"),
        "schema = 1\nid = \"hats\"\nversion = \"1.0.0\"\nname = { en = \"Hats\" }\nauthors = [\"me\"]\nlicense = \"MIT\"\n\
         api = \"^0.3\"\n[sides]\nserver = true\nclient = true\n[entry]\n\
         server = { runtime = \"lua\", path = \"server/main.luau\" }\n\
         client = { runtime = \"lua\", path = \"client/main.luau\" }\n\
         [content]\ncosmetics = [\n\
         { key = \"hats:crown\", slot = \"hat\", base = \"linen-cap\", texture = \"assets/crown.png\" },\n\
         { key = \"hats:ember\", slot = \"hair\", texture = \"assets/crown.png\" },\n]\n",
    )
    .unwrap();
    std::fs::write(hats.join("client/main.luau"), "return {}").unwrap();
    std::fs::write(
        hats.join("server/main.luau"),
        r#"--!strict
local mod = {}
function mod.on_enable()
	bh.commands.register({ name = "wear", description = "Wear.", usage = "/wear <key>" }, function(sender, label, args)
		local ok, err = pcall(bh.content.equip, sender.player, args[1])
		return { handled = true, reply = { if ok then "worn" else "refused" } }
	end)
	bh.commands.register({ name = "strip", description = "Strip.", usage = "/strip <slot>" }, function(sender, label, args)
		bh.content.unequip(sender.player, args[1])
		return { handled = true, reply = { "stripped" } }
	end)
end
return mod
"#,
    )
    .unwrap();
    std::fs::write(hats.join("assets/crown.png"), TINY_PNG).unwrap();
}

fn cosmetic_msgs(s: &Session) -> Vec<bhmod_proto::Cosmetics> {
    mod_packets(s, 0x12)
        .iter()
        .map(|p| match Message::decode(p).unwrap() {
            Message::Cosmetics(c) => c,
            m => panic!("{m:?}"),
        })
        .collect()
}

fn wait_cosmetics(s: &mut Session, count: usize) {
    let end = Instant::now() + Duration::from_secs(5);
    while cosmetic_msgs(s).len() < count && Instant::now() < end {
        s.run_for(50).unwrap();
    }
}

#[test]
fn content_mods_get_ids_and_the_cosmetics_table() {
    let _w = watchdog("content_mods", 240);
    let tmp = tempfile::tempdir().unwrap();
    let (data, wid) = world(tmp.path());
    let mods = tmp.path().join("mods");
    hats_mod(&mods);
    let mut srv = start(&data, &wid, 15479, &mods);

    let mut s = srv.session("MODDER");
    let mut mc = ModClient::new(LoaderOptions::new(&tmp.path().join("cache")));
    assert_eq!(mc.join(&mut s, 10_000).unwrap(), JoinOutcome::Approved, "{}", srv.log());
    let list = mc.list.clone().expect("a content mod is client-required, so the list was fetched");
    let cos = &list.registry["cosmetics"];
    assert_eq!((cos["hats:crown"], cos["hats:ember"]), (1, 2));
    assert_eq!(list.registry.len(), 1);
    // The table arrives right after approval: full, nobody wears anything yet.
    wait_cosmetics(&mut s, 1);
    assert_eq!(
        cosmetic_msgs(&s),
        vec![bhmod_proto::Cosmetics { full: true, players: Default::default() }],
        "{}",
        srv.log()
    );
    // The ids are in the world's sidecar.
    let side = walk(tmp.path()).into_iter().find(|p| p.ends_with("mods/registry.toml")).expect("registry.toml");
    let text = std::fs::read_to_string(&side).unwrap();
    assert!(text.contains("\"hats:crown\" = 1") && text.contains("\"hats:ember\" = 2"), "{text}");

    // A second modded player watches. The mod equips through bh.content: deltas reach both, vanilla gets nothing.
    let mut watcher = srv.session("WATCHER");
    let mut wc = ModClient::new(LoaderOptions::new(&tmp.path().join("cache2")));
    assert_eq!(wc.join(&mut watcher, 10_000).unwrap(), JoinOutcome::Approved, "{}", srv.log());
    wait_cosmetics(&mut watcher, 1);
    let me = Identity::new("MODDER").player_id();
    s.chat("/wear hats:crown").unwrap();
    wait_cosmetics(&mut s, 2);
    wait_cosmetics(&mut watcher, 2);
    for sess in [&s, &watcher] {
        let last = cosmetic_msgs(sess).pop().unwrap();
        assert!(!last.full);
        assert_eq!(last.players, [(me.clone(), [("hat".to_string(), 1)].into())].into(), "{}", srv.log());
    }
    // Another slot adds to the set; a cosmetic of a wrong key is refused (nothing is sent).
    s.chat("/wear hats:ember").unwrap();
    s.chat("/wear hats:nope").unwrap();
    s.chat("/wear other:crown").unwrap();
    wait_cosmetics(&mut watcher, 3);
    s.run_for(300).unwrap();
    assert_eq!(cosmetic_msgs(&watcher).len(), 3, "{}", srv.log());
    let last = cosmetic_msgs(&watcher).pop().unwrap();
    assert_eq!(last.players[&me], [("hair".to_string(), 2), ("hat".to_string(), 1)].into());
    s.chat("/strip hat").unwrap();
    wait_cosmetics(&mut watcher, 4);
    assert_eq!(cosmetic_msgs(&watcher).pop().unwrap().players[&me], [("hair".to_string(), 2)].into());
    // A late joiner gets the table in its first message; the wearer leaving clears it for the others.
    let mut late = srv.session("LATE");
    let mut lc = ModClient::new(LoaderOptions::new(&tmp.path().join("cache3")));
    assert_eq!(lc.join(&mut late, 10_000).unwrap(), JoinOutcome::Approved, "{}", srv.log());
    wait_cosmetics(&mut late, 1);
    let first = cosmetic_msgs(&late).remove(0);
    assert!(first.full);
    assert_eq!(first.players[&me], [("hair".to_string(), 2)].into());
    s.disconnect();
    wait_cosmetics(&mut watcher, 5);
    let gone = cosmetic_msgs(&watcher).pop().unwrap();
    assert_eq!(gone.players, [(me.clone(), Default::default())].into(), "{}", srv.log());
    assert_eq!(srv.handle.stop(), Status::Stopped(0));
}

#[test]
fn a_broken_registry_turns_content_off_without_touching_the_file() {
    let _w = watchdog("broken_registry", 240);
    let tmp = tempfile::tempdir().unwrap();
    let (data, wid) = world(tmp.path());
    let mods = tmp.path().join("mods");
    hats_mod(&mods);
    let mut srv = start(&data, &wid, 15481, &mods);
    assert_eq!(srv.handle.stop(), Status::Stopped(0));
    let side = walk(tmp.path()).into_iter().find(|p| p.ends_with("mods/registry.toml")).expect("registry.toml");
    std::fs::write(&side, "not = valid [").unwrap();
    let mut srv = start(&data, &wid, 15481, &mods);
    assert!(srv.log().contains("content is off"), "{}", srv.log());
    let mut s = srv.session("MODDER");
    let mut mc = ModClient::new(LoaderOptions::new(&tmp.path().join("cache")));
    assert_eq!(mc.join(&mut s, 10_000).unwrap(), JoinOutcome::Approved, "{}", srv.log());
    s.run_for(500).unwrap();
    assert!(mc.list.unwrap().registry.is_empty());
    assert!(mod_packets(&s, 0x12).is_empty());
    s.chat("/wear hats:crown").unwrap();
    s.run_for(500).unwrap();
    assert!(mod_packets(&s, 0x12).is_empty());
    assert_eq!(std::fs::read_to_string(&side).unwrap(), "not = valid [");
    assert_eq!(srv.handle.stop(), Status::Stopped(0));
}

#[test]
fn servers_without_content_write_no_registry_and_send_no_cosmetics() {
    let _w = watchdog("no_content", 180);
    let tmp = tempfile::tempdir().unwrap();
    let (data, wid) = world(tmp.path());
    let mods = client_mods(tmp.path(), 1000);
    let mut srv = start(&data, &wid, 15480, &mods);
    let mut s = srv.session("MODDER");
    let mut mc = ModClient::new(LoaderOptions::new(&tmp.path().join("cache")));
    assert_eq!(mc.join(&mut s, 10_000).unwrap(), JoinOutcome::Approved, "{}", srv.log());
    s.run_for(500).unwrap();
    assert!(mc.list.unwrap().registry.is_empty());
    assert!(mod_packets(&s, 0x12).is_empty());
    assert!(!walk(tmp.path()).iter().any(|p| p.ends_with("registry.toml")));
    assert_eq!(srv.handle.stop(), Status::Stopped(0));
}
