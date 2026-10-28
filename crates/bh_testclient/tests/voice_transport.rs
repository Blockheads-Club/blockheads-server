//! Voice transport on a live server (in-process): 0xBC `VOICE_FRAME` packets sent unsequenced by a modded client
//! reach the installed relay, which answers unsequenced on channel 0; the optional UDP voice socket; and that none of
//! it exists without a mod host.

use std::net::{SocketAddr, UdpSocket};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use bh_testclient::{ClientConfig, Identity, JoinOutcome, LoaderOptions, ModClient, Session};
use blockheads_server::host::{self, HostConfig, ServerHandle, Status, WorldSelect};
use blockheads_server::mods::voice::{
    UdpOut, UdpVoiceHandler, VoiceConfig, VoiceOut, VoiceSink, VoiceSinkFactory, VOICE_FRAME,
};
use blockheads_server::mods::ModsConfig;

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

fn mods_dir(tmp: &Path) -> PathBuf {
    let mods = tmp.join("mods");
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/mods/hello-lua");
    copy_dir(&fixture, &mods.join("hello-lua"));
    std::fs::write(mods.join("mods.toml"), "[hello-lua]\ngrant = [\"players:chat\"]\n").unwrap();
    mods
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
    fn stop(mut self) {
        self.handle.request_stop();
        self.handle.wait();
    }
}

fn start(tmp: &Path, port: u16, mods: Option<PathBuf>, voice: VoiceConfig) -> Running {
    let data = tmp.join("TheBlockheads");
    let w = host::create_world(Some(&data), "Voice", 1, false).expect("create");
    let lines = Arc::new(Mutex::new(Vec::<String>::new()));
    let l2 = lines.clone();
    let cfg = HostConfig {
        data_dir: Some(data),
        world: WorldSelect::Load(w.id),
        port: Some(port),
        mods: ModsConfig { disabled: mods.is_none(), dir: mods, variant_blocks: false },
        voice,
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

/// Answers every frame to its speaker with the body reversed.
struct Mirror;

impl VoiceSink for Mirror {
    fn frame(&mut self, peer: &str, body: &[u8], out: &mut dyn VoiceOut) {
        let mut p = vec![0xbc, VOICE_FRAME];
        p.extend(body.iter().rev());
        out.send(peer, &p);
    }
}

/// Answers every datagram with `pong:` + the datagram.
struct Pong;

impl UdpVoiceHandler for Pong {
    fn datagram(&self, from: SocketAddr, data: &[u8], out: &dyn UdpOut) {
        let mut a = b"pong:".to_vec();
        a.extend_from_slice(data);
        out.send_to(from, &a);
    }
}

fn free_udp_port() -> u16 {
    UdpSocket::bind("0.0.0.0:0").unwrap().local_addr().unwrap().port()
}

#[test]
fn voice_frames_are_relayed_unsequenced_and_the_udp_socket_answers() {
    let _w = watchdog("voice_relay", 180);
    let tmp = tempfile::tempdir().unwrap();
    let udp = free_udp_port();
    let sink: VoiceSinkFactory = Arc::new(|| Box::new(Mirror));
    let voice = VoiceConfig { udp_port: Some(udp), sink: Some(sink), udp_handler: Some(Arc::new(Pong)) };
    let srv = start(tmp.path(), 15480, Some(mods_dir(tmp.path())), voice);

    let mut s = srv.session("TALKER");
    let mut mc = ModClient::new(LoaderOptions::new(&tmp.path().join("cache")));
    assert_eq!(mc.join(&mut s, 10_000).unwrap(), JoinOutcome::Approved, "{}", srv.log());
    let mut v = srv.session("VANILLA");
    assert_eq!(v.join(10_000).unwrap(), JoinOutcome::Approved, "{}", srv.log());

    // A frame sent unsequenced comes back unsequenced on channel 0, through the relay (body reversed).
    s.send_unsequenced(&[0xbc, VOICE_FRAME, 1, 2, 3, 4]).unwrap();
    let rx = s.wait_for_matching(5000, &mut |rx| rx.data.starts_with(&[0xbc, VOICE_FRAME])).unwrap();
    let rx = rx.expect("the relayed frame");
    assert_eq!(rx.data, [0xbc, VOICE_FRAME, 4, 3, 2, 1]);
    assert!(rx.unsequenced && rx.channel == 0, "{rx:?}");
    // Ordinary mod packets are not unsequenced.
    assert!(s
        .received()
        .iter()
        .filter(|r| r.data.first() == Some(&0xbc) && r.data[1] != VOICE_FRAME)
        .all(|r| !r.unsequenced));

    // A vanilla player's 0xBC frame reaches no relay and nothing comes back to it.
    v.send_unsequenced(&[0xbc, VOICE_FRAME, 9]).unwrap();
    v.run_for(300).unwrap();
    assert!(!v.received().iter().any(|r| r.data.first() == Some(&0xbc)), "a vanilla client got 0xBC");

    // The dedicated UDP socket answers through its handler.
    let c = UdpSocket::bind("127.0.0.1:0").unwrap();
    c.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
    c.send_to(b"ping", ("127.0.0.1", udp)).unwrap();
    let mut b = [0u8; 64];
    let (n, _) = c.recv_from(&mut b).unwrap();
    assert_eq!(&b[..n], b"pong:ping");

    s.client.disconnect(0);
    v.client.disconnect(0);
    srv.stop();
    // Stopped with the server: the port is free again.
    UdpSocket::bind(("0.0.0.0", udp)).expect("the voice port was closed");
}

#[test]
fn a_taken_voice_port_is_reported_and_the_server_runs_on() {
    let _w = watchdog("voice_port_taken", 180);
    let tmp = tempfile::tempdir().unwrap();
    let taken = UdpSocket::bind("0.0.0.0:0").unwrap();
    let port = taken.local_addr().unwrap().port();
    let voice = VoiceConfig { udp_port: Some(port), ..VoiceConfig::default() };
    let srv = start(tmp.path(), 15481, Some(mods_dir(tmp.path())), voice);
    assert!(srv.log().contains(&format!("[mods WARN] voice: UDP port {port} could not be opened")), "{}", srv.log());
    let mut s = srv.session("SOLO");
    assert_eq!(s.join(10_000).unwrap(), JoinOutcome::Approved);
    s.client.disconnect(0);
    srv.stop();
}

#[test]
fn without_mods_no_voice_socket_is_opened() {
    let _w = watchdog("voice_no_mods", 180);
    let tmp = tempfile::tempdir().unwrap();
    let udp = free_udp_port();
    let sink: VoiceSinkFactory = Arc::new(|| Box::new(Mirror));
    let voice = VoiceConfig { udp_port: Some(udp), sink: Some(sink), udp_handler: Some(Arc::new(Pong)) };
    let srv = start(tmp.path(), 15482, None, voice);
    // The port stays free: nothing was bound.
    drop(UdpSocket::bind(("0.0.0.0", udp)).expect("no voice socket without a mod host"));
    // A 0xBC frame is just an unknown packet to the vanilla server: no answer.
    let mut s = srv.session("PLAIN");
    assert_eq!(s.join(10_000).unwrap(), JoinOutcome::Approved);
    s.send_unsequenced(&[0xbc, VOICE_FRAME, 1]).unwrap();
    s.run_for(300).unwrap();
    assert!(!s.received().iter().any(|r| r.data.first() == Some(&0xbc)));
    assert!(!srv.log().contains("voice"), "{}", srv.log());
    s.client.disconnect(0);
    srv.stop();
}
