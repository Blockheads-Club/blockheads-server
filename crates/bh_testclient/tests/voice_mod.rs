//! Voice chat end to end on a live server (in-process), with the official voice mod (mod-sdk
//! `examples/voice-rust`, built by mod-sdk's `scripts/build-wasm.sh`; the fixture keeps the one component for both
//! sides once, as `voice.wasm`, and the test puts it at `server/mod.wasm` and `client/mod.wasm`) and modded test
//! clients running mod-sdk's `VoiceClient` with its `TestCodec` (no libopus). Voice frames go only over the voice
//! UDP port (game port + 1 by default); control stays on reliable 0xBC: the `VOICE_KEY` handshake, encrypted
//! frames routed by proximity, an admin mute, a taken port (voice off), and that a vanilla player gets no voice
//! byte.

use std::net::{SocketAddr, UdpSocket};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use bh_testclient::{BlockheadSpec, ClientConfig, Identity, JoinOutcome, LoaderOptions, ModClient, Session};
use bhmod_proto::{Message, PACKET_TYPE};
use bhmod_voice::client::{Outgoing, VoiceClient};
use bhmod_voice::codec::TestCodec;
use bhmod_voice::FRAME_SAMPLES;
use blockheads_server::host::{self, HostConfig, ServerHandle, Status, WorldSelect};
use blockheads_server::mods::voice::VoiceConfig;
use blockheads_server::mods::ModsConfig;
use blockheads_server::protocol::codec::t;

const OBJECT_TYPE_BLOCKHEAD: u8 = 24;
const VOICE_FRAME: u8 = 0x16;

/// Ends the test process when a test hangs.
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

/// The voice mod, granted, with `settings` as its `[voice.settings]`.
fn mods_dir(tmp: &Path, settings: &str) -> PathBuf {
    let mods = tmp.join("mods");
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/mods/voice");
    let dir = mods.join("voice");
    copy_dir(&fixture, &dir);
    for side in ["server", "client"] {
        std::fs::create_dir_all(dir.join(side)).unwrap();
        std::fs::copy(dir.join("voice.wasm"), dir.join(side).join("mod.wasm")).unwrap();
    }
    std::fs::remove_file(dir.join("voice.wasm")).unwrap();
    let toml = format!("[voice]\ngrant = [\"voice\", \"channels\", \"players:kick\"]\n\n[voice.settings]\n{settings}");
    std::fs::write(mods.join("mods.toml"), toml).unwrap();
    mods
}

struct Running {
    handle: ServerHandle,
    lines: Arc<Mutex<Vec<String>>>,
    port: u16,
    tmp: PathBuf,
}

impl Running {
    fn start(tmp: &Path, port: u16, mods: PathBuf, voice: VoiceConfig) -> Running {
        let data = tmp.join("TheBlockheads");
        let w = host::create_world(Some(&data), "Voice", 1, false).expect("create");
        let lines = Arc::new(Mutex::new(Vec::<String>::new()));
        let l2 = lines.clone();
        let cfg = HostConfig {
            data_dir: Some(data),
            world: WorldSelect::Load(w.id),
            port: Some(port),
            mods: ModsConfig { disabled: false, dir: Some(mods), variant_blocks: false },
            voice,
            ..HostConfig::default()
        };
        let handle = host::start(cfg, Arc::new(move |s| l2.lock().unwrap().push(s.to_string()))).unwrap();
        let end = Instant::now() + Duration::from_secs(90);
        while handle.status() == Status::Starting && Instant::now() < end {
            std::thread::sleep(Duration::from_millis(50));
        }
        assert_eq!(handle.status(), Status::Running, "{:#?}", lines.lock().unwrap());
        Running { handle, lines, port, tmp: tmp.to_path_buf() }
    }
    fn log(&self) -> String {
        self.lines.lock().unwrap().join("\n")
    }
    fn session(&self, alias: &str) -> Session {
        let cfg = ClientConfig { addr: SocketAddr::from(([127, 0, 0, 1], self.port)), ..Default::default() };
        Session::connect(cfg, Identity::new(alias)).unwrap()
    }
    /// A modded player with voice: joined, initial data loaded, the voice handshake done.
    fn talker(&self, alias: &str) -> Talker {
        let mut s = self.session(alias);
        let mut opts = LoaderOptions::new(&self.tmp.join(format!("cache-{alias}")));
        opts.features = vec!["voice".into(), "settings".into()];
        let mut mc = ModClient::new(opts);
        assert_eq!(mc.join(&mut s, 10_000).unwrap(), JoinOutcome::Approved, "{}", self.log());
        let hello = mc.server.clone().expect("SERVER_HELLO");
        assert!(hello.features.iter().any(|f| f == "voice"), "{hello:?}");
        s.request_initial_data().unwrap();
        s.wait_for(t::SERVER_INITIAL_DATA_REPLY, 10_000).unwrap().expect("initial data");
        let mut v = VoiceClient::new(Arc::new(TestCodec));
        s.client.send(&v.start().unwrap()).unwrap();
        let mut me = Talker { s, v, udp: None, voice_port: hello.voice_udp_port, seen: 0, datagrams: 0 };
        me.pump_until(5000, |t| t.v.connected());
        me
    }
    fn stop(mut self) {
        self.handle.request_stop();
        self.handle.wait();
    }
}

struct Talker {
    s: Session,
    v: VoiceClient,
    udp: Option<UdpSocket>,
    voice_port: Option<u16>,
    /// Packets of `s` already given to `v`.
    seen: usize,
    /// Datagrams received on the voice port (`AUTH_OK`, frames).
    datagrams: usize,
}

impl Talker {
    /// Feeds what arrived (0xBC voice packets, voice datagrams) to the voice client and sends what it wants to send.
    fn pump(&mut self, ms: u64) {
        self.s.run_for(ms).unwrap();
        let rx = self.s.received();
        for r in &rx[self.seen..] {
            if r.data.first() == Some(&PACKET_TYPE) {
                self.v.on_packet(&r.data);
            }
        }
        self.seen = rx.len();
        if let Some(u) = &self.udp {
            let mut b = [0u8; 1500];
            while let Ok(n) = u.recv(&mut b) {
                self.datagrams += 1;
                self.v.on_udp(&b[..n]);
            }
        }
        self.flush(now_ms());
    }

    fn flush(&mut self, now: u64) {
        for o in self.v.poll_send(now) {
            match o {
                Outgoing::Reliable(p) => self.s.client.send(&p).unwrap(),
                Outgoing::Udp(d) => {
                    let port = self.v.udp_port().expect("a datagram needs the port");
                    let u = self.udp.get_or_insert_with(|| {
                        let u = UdpSocket::bind("127.0.0.1:0").unwrap();
                        u.set_nonblocking(true).unwrap();
                        u
                    });
                    u.send_to(&d, ("127.0.0.1", port)).unwrap();
                }
            }
        }
    }

    fn pump_until(&mut self, ms: u64, mut done: impl FnMut(&Talker) -> bool) {
        let end = Instant::now() + Duration::from_millis(ms);
        while !done(self) {
            assert!(Instant::now() < end, "timed out");
            self.pump(20);
        }
    }

    /// A blockhead at `(x, y)` (its block loaded first).
    fn spawn(&mut self, x: i32, y: i32) {
        let i = self.s.macro_index_for_tile(x, y);
        self.s.request_block(i, true).unwrap();
        let end = Instant::now() + Duration::from_secs(10);
        while !self
            .s
            .received()
            .iter()
            .any(|r| r.data.first() == Some(&0x04) && r.data.get(1..5) == Some(&(i as u32).to_le_bytes()[..]))
        {
            assert!(Instant::now() < end, "block {i} not sent");
            self.pump(20);
        }
        self.s.request_unique_id().unwrap();
        self.s.create_blockhead(&BlockheadSpec::new("Bob", x as u32, y as u32)).unwrap();
        let end = Instant::now() + Duration::from_secs(10);
        while !self.s.state.created.iter().any(|c| c.object_type == OBJECT_TYPE_BLOCKHEAD) {
            assert!(Instant::now() < end, "blockhead was not created");
            self.pump(50);
        }
    }

    /// One 20 ms frame of push-to-talk in the proximity mode.
    fn say(&mut self) {
        self.v.set_mic(true);
        self.v.talk(Some(bhmod_core::voice::Mode::Proximity), true);
        self.v.on_mic_frame(&[0.3; FRAME_SAMPLES]);
        self.flush(now_ms());
    }

    /// Voice datagrams received (frames, after the one `AUTH_OK`).
    fn heard_frames(&self) -> usize {
        self.datagrams
    }

    /// In-band voice frames received (there must be none).
    fn in_band_frames(&self) -> usize {
        self.s.received().iter().filter(|r| r.data.starts_with(&[PACKET_TYPE, VOICE_FRAME])).count()
    }
}

fn now_ms() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as u64
}

fn summary_int(s: &Session, key: &str) -> i32 {
    s.state.summary.as_ref().and_then(|p| p.object_for_key(key)).map(|v| v.int_value()).expect(key)
}

fn any_0xbc(s: &Session) -> bool {
    s.received().iter().any(|r| r.data.first() == Some(&PACKET_TYPE))
}

/// Talks for `ms` while the others listen; returns the frames each listener got.
fn talk_for(speaker: &mut Talker, listeners: &mut [&mut Talker], ms: u64) -> Vec<usize> {
    let before: Vec<usize> = listeners.iter().map(|l| l.heard_frames()).collect();
    let end = Instant::now() + Duration::from_millis(ms);
    while Instant::now() < end {
        speaker.say();
        speaker.pump(20);
        for l in listeners.iter_mut() {
            l.pump(0);
        }
    }
    for l in listeners.iter_mut() {
        l.pump(100);
    }
    listeners.iter().zip(before).map(|(l, b)| l.heard_frames() - b).collect()
}

/// Waits until the server allowed the proximity mode (the mod's 5 Hz pass) and routes reached the relay.
fn ready(ts: &mut [&mut Talker]) {
    for t in ts.iter_mut() {
        t.pump_until(5000, |t| t.v.allowed().modes & bhmod_core::voice::Mode::Proximity.bit() != 0);
    }
    // One more routing pass after everybody's blockhead exists.
    for t in ts.iter_mut() {
        t.pump(300);
    }
}

/// The game ports of the tests; the voice port is the next one unless configured.
const PORT_ROUTED: u16 = 15490;
const PORT_CONFIGURED: u16 = 15493;
const PORT_TAKEN: u16 = 15496;
const PORT_VANILLA: u16 = 15499;
const PORT_RECONNECT: u16 = 15502;

#[test]
fn voice_is_routed_by_proximity_over_udp_and_muted_by_the_admin() {
    let _w = watchdog("voice_mod_routed", 240);
    let tmp = tempfile::tempdir().unwrap();
    let srv = Running::start(tmp.path(), PORT_ROUTED, mods_dir(tmp.path(), ""), VoiceConfig::default());
    assert!(srv.log().contains("loaded 1 mod(s): voice"), "{}", srv.log());

    let mut a = srv.talker("ANNA");
    let mut b = srv.talker("BERT");
    let mut far = srv.talker("FARA");
    let mut vanilla = srv.session("VANILLA");
    assert_eq!(vanilla.join(10_000).unwrap(), JoinOutcome::Approved);
    // The voice port is the game port + 1, announced in SERVER_HELLO and VOICE_KEY.
    assert_eq!(a.voice_port, Some(PORT_ROUTED + 1));
    assert_eq!(a.v.udp_port(), Some(PORT_ROUTED + 1));

    // A and B stand at the portal, FARA far away.
    let (px, py) = (summary_int(&a.s, "startPortalPos.x"), summary_int(&a.s, "startPortalPos.y"));
    a.spawn(px, py + 2);
    b.spawn(px, py + 2);
    far.spawn(px + 400, py + 2);
    ready(&mut [&mut a, &mut b, &mut far]);
    assert!(a.udp.is_some() && b.udp.is_some(), "the clients authenticated on the voice port");
    // Every voice client knows the others' names (VOICE_STATE.names), vanilla players included.
    assert!(a.v.players().iter().any(|(_, alias)| alias == "VANILLA"), "{:?}", a.v.players());

    // A talks: B hears A (decrypted, a talk spurt starts), FARA hears nothing.
    let got = talk_for(&mut a, &mut [&mut b, &mut far], 1000);
    assert!(got[0] > 20, "B heard {} frames\n{}", got[0], srv.log());
    assert_eq!(got[1], 0, "FARA is out of range");
    let events = b.v.take_events();
    assert!(events.iter().any(|e| matches!(e, bhmod_core::voice::VoiceEvent::TalkStart { .. })), "{events:?}");
    let mut mix = vec![0.0f32; FRAME_SAMPLES * 2];
    b.v.mix(&mut mix);
    assert_eq!(b.in_band_frames(), 0, "frames never travel in-band");

    // An in-band frame from a client is dropped (nobody hears it).
    let before = b.heard_frames();
    a.s.client.send(&[PACKET_TYPE, VOICE_FRAME, 0, 0, 0, 0, 0, 0, 0, 0, 1, 2, 3]).unwrap();
    a.pump(100);
    b.pump(200);
    assert_eq!(b.heard_frames(), before);

    // The admin mutes A: nothing reaches B any more.
    srv.handle.send_command("/voice mute ANNA");
    a.pump_until(5000, |t| t.v.state(&t.s.id.player_id()) == bhmod_core::voice::VoiceState::AdminMuted);
    b.pump(300);
    let got = talk_for(&mut a, &mut [&mut b], 600);
    assert_eq!(got[0], 0, "a muted player is not relayed");
    srv.handle.send_command("/voice unmute ANNA");
    a.pump_until(5000, |t| t.v.state(&t.s.id.player_id()) == bhmod_core::voice::VoiceState::Ready);
    let got = talk_for(&mut a, &mut [&mut b], 600);
    assert!(got[0] > 5, "unmuted again: {got:?}");

    // The vanilla player got no 0xBC byte at all.
    vanilla.run_for(200).unwrap();
    assert!(!any_0xbc(&vanilla), "a vanilla client got 0xBC");

    for c in [&mut a.s, &mut b.s, &mut far.s, &mut vanilla] {
        c.client.disconnect(0);
    }
    srv.stop();
    // Closed with the server.
    UdpSocket::bind(("0.0.0.0", PORT_ROUTED + 1)).expect("the voice port was closed");
}

#[test]
fn the_configured_voice_port_is_used() {
    let _w = watchdog("voice_mod_configured", 240);
    let tmp = tempfile::tempdir().unwrap();
    let port = UdpSocket::bind("0.0.0.0:0").unwrap().local_addr().unwrap().port();
    let voice = VoiceConfig { udp_port: Some(port), ..VoiceConfig::default() };
    let srv = Running::start(tmp.path(), PORT_CONFIGURED, mods_dir(tmp.path(), ""), voice);
    let mut a = srv.talker("ANNA");
    let mut b = srv.talker("BERT");
    assert_eq!(a.voice_port, Some(port), "SERVER_HELLO announces the configured port");
    assert_eq!(a.v.udp_port(), Some(port), "VOICE_KEY announces it too");
    let (px, py) = (summary_int(&a.s, "startPortalPos.x"), summary_int(&a.s, "startPortalPos.y"));
    a.spawn(px, py + 2);
    b.spawn(px, py + 2);
    ready(&mut [&mut a, &mut b]);
    let got = talk_for(&mut a, &mut [&mut b], 1000);
    assert!(got[0] > 20, "B heard {} frames\n{}", got[0], srv.log());
    a.s.client.disconnect(0);
    b.s.client.disconnect(0);
    srv.stop();
    UdpSocket::bind(("0.0.0.0", port)).expect("the voice port was closed");
}

#[test]
fn a_taken_voice_port_turns_voice_off() {
    let _w = watchdog("voice_mod_taken", 180);
    let tmp = tempfile::tempdir().unwrap();
    let _taken = UdpSocket::bind(("0.0.0.0", PORT_TAKEN + 1)).expect("the test port is free");
    let srv = Running::start(tmp.path(), PORT_TAKEN, mods_dir(tmp.path(), ""), VoiceConfig::default());
    let warn = format!("[mods WARN] voice: UDP port {} could not be opened", PORT_TAKEN + 1);
    assert!(srv.log().lines().any(|l| l.contains(&warn) && l.ends_with("voice chat is off")), "{}", srv.log());
    // A modded client joins: no `voice` feature, no port; its VOICE_KEY gets no answer.
    let mut s = srv.session("ANNA");
    let mut opts = LoaderOptions::new(&tmp.path().join("cache"));
    opts.features = vec!["voice".into()];
    let mut mc = ModClient::new(opts);
    assert_eq!(mc.join(&mut s, 10_000).unwrap(), JoinOutcome::Approved, "{}", srv.log());
    let hello = mc.server.clone().expect("SERVER_HELLO");
    assert!(!hello.features.iter().any(|f| f == "voice") && hello.voice_udp_port.is_none(), "{hello:?}");
    let mark = s.received().len();
    let mut v = VoiceClient::new(Arc::new(TestCodec));
    s.client.send(&v.start().unwrap()).unwrap();
    s.run_for(500).unwrap();
    assert!(!s.received()[mark..].iter().any(|r| r.data.first() == Some(&PACKET_TYPE)));
    s.client.disconnect(0);
    srv.stop();
}

#[test]
fn a_vanilla_only_game_with_the_voice_mod_gets_no_voice_bytes() {
    let _w = watchdog("voice_mod_vanilla", 180);
    let tmp = tempfile::tempdir().unwrap();
    let srv = Running::start(tmp.path(), PORT_VANILLA, mods_dir(tmp.path(), ""), VoiceConfig::default());
    let mut v = srv.session("PLAIN");
    assert_eq!(v.join(10_000).unwrap(), JoinOutcome::Approved, "{}", srv.log());
    v.request_initial_data().unwrap();
    v.run_for(1500).unwrap();
    assert!(!any_0xbc(&v));
    // A VOICE_KEY from a client that never said HELLO is just an unknown packet.
    let key = Message::VoiceKey(bhmod_proto::VoiceKey { public: [7; 32], peer: None, udp_port: None }).encode();
    v.client.send(&key).unwrap();
    v.run_for(500).unwrap();
    assert!(!any_0xbc(&v));
    v.client.disconnect(0);
    srv.stop();
}

#[test]
fn without_a_voice_mod_no_voice_socket_is_opened() {
    let _w = watchdog("voice_mod_none", 180);
    let tmp = tempfile::tempdir().unwrap();
    let mods = tmp.path().join("mods");
    std::fs::create_dir_all(&mods).unwrap();
    let srv = Running::start(tmp.path(), PORT_VANILLA, mods, VoiceConfig::default());
    UdpSocket::bind(("0.0.0.0", PORT_VANILLA + 1)).expect("no voice socket without a voice mod");
    srv.stop();
}

#[test]
fn voice_works_after_a_reconnect_before_the_old_session_timed_out() {
    let _w = watchdog("voice_mod_reconnect", 240);
    let tmp = tempfile::tempdir().unwrap();
    let srv = Running::start(tmp.path(), PORT_RECONNECT, mods_dir(tmp.path(), ""), VoiceConfig::default());
    let mut old = srv.talker("ANNA");
    let mut b = srv.talker("BERT");
    let (px, py) = (summary_int(&b.s, "startPortalPos.x"), summary_int(&b.s, "startPortalPos.y"));
    old.spawn(px, py + 2);
    b.spawn(px, py + 2);
    ready(&mut [&mut old, &mut b]);
    // ANNA's game reconnects from the same device while the old connection is still alive (not timed out): the
    // server takes the old session over. The new session must get its own voice handshake (`talker` waits for
    // VOICE_KEY + AUTH_OK) and its frames must be relayed both ways.
    let mut a = srv.talker("ANNA");
    assert!(srv.log().contains("attempt to reconnect from same device"), "{}", srv.log());
    old.pump(300);
    a.spawn(px, py + 2);
    ready(&mut [&mut a, &mut b]);
    assert!(a.v.connected() && a.udp.is_some(), "the new session authenticated on the voice port");
    let got = talk_for(&mut a, &mut [&mut b], 1000);
    assert!(got[0] > 20, "B heard {} frames from the new session\n{}", got[0], srv.log());
    let got = talk_for(&mut b, &mut [&mut a], 1000);
    assert!(got[0] > 20, "the new session heard {} frames from B\n{}", got[0], srv.log());
    // The old connection going away later does not take the new session's voice down.
    old.s.client.disconnect(0);
    b.pump(500);
    a.pump(500);
    let got = talk_for(&mut a, &mut [&mut b], 1000);
    assert!(got[0] > 20, "B heard {} frames after the old connection closed\n{}", got[0], srv.log());
    a.s.client.disconnect(0);
    b.s.client.disconnect(0);
    srv.stop();
}
