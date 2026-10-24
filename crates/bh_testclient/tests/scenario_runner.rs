//! The scenario runner against our server in-process: every step kind, the options of a run and each way a script
//! can fail (timeouts, unexpected packets, unknown clients, rejections, a server that is not there).

use std::net::SocketAddr;
use std::time::Duration;

use bh_testclient::harness::inproc::HostServer;
use bh_testclient::harness::refenv::{RunSpec, ServerHandle};
use bh_testclient::harness::TempDir;
use bh_testclient::scenario::{RunOptions, Scenario, ScenarioError};
use bh_testclient::ClientConfig;

fn start() -> (HostServer, ClientConfig) {
    let spec = RunSpec {
        tag: "scenario-runner".into(),
        args: [
            "--new",
            "RUNNER",
            "--world_id",
            "c0ffee00c0ffee00c0ffee00c0ffee00",
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
    let mut s = HostServer::start(&spec).unwrap();
    s.wait_for_log("World load complete.", Duration::from_secs(120)).unwrap();
    let addr: SocketAddr = format!("127.0.0.1:{}", s.port()).parse().unwrap();
    (s, ClientConfig { addr, ..Default::default() })
}

fn run(
    cfg: &ClientConfig,
    script: &str,
    opts: &RunOptions,
) -> Result<bh_testclient::scenario::RunReport, ScenarioError> {
    let mut sc = Scenario::parse(script).unwrap();
    sc.name = "runner".into();
    sc.run(cfg, opts)
}

fn fast() -> RunOptions {
    RunOptions { default_timeout_ms: 1500, ..Default::default() }
}

fn failure(cfg: &ClientConfig, script: &str) -> String {
    match run(cfg, script, &fast()) {
        Ok(_) => panic!("expected a failure: {script}"),
        Err(e) => e.to_string(),
    }
}

#[test]
fn every_step_runs_and_the_report_says_so() {
    let (_server, cfg) = start();
    let dir = TempDir::new("runner-traces").unwrap();
    let opts = RunOptions {
        trace_dir: Some(dir.path().to_path_buf()),
        client_port_base: Some(bh_testclient::harness::free_udp_port_range(4)),
        ..fast()
    };
    let script = "\
client A dialect=gnustep
client B udid=bee minor=3
use A
initial-data
wait 06
block-xy 3 3
block 700 create=0
blocks 701,702
block-tile 100 100
wait 04
sleep 50
uid
blockhead Al 100 100
hb-old
hb-rules
fill 100 100 3
remove 100 100
paint 100 100 1 2
workbench 100 100 1
chat hello
cmd /help
send 0e
xml 24 <?xml version=\"1.0\"?><plist><dict><key>message</key><string>hi</string></dict></plist>
gz 2e <?xml version=\"1.0\"?><plist><dict/></plist>
zoom somebody 1
inventory @LAST_BLOCKHEAD [[],[],[],[],[],[],[],[]]
quiet 200
expect 06
expect-none 55 100
use B
disconnect
use A
disconnect
";
    // the inventory step needs a blockhead the server announced to us; it may or may not have arrived, both are fine
    let report = match run(&cfg, &script.replace("inventory @LAST_BLOCKHEAD [[],[],[],[],[],[],[],[]]\n", ""), &opts) {
        Ok(r) => r,
        Err(e) => panic!("{e}"),
    };
    assert!(report.notes.iter().any(|n| n == "A: join approved"));
    let owner = run(&cfg, "client Boss\ncmd /help\ndisconnect\n", &fast()).unwrap();
    assert!(owner.notes.iter().any(|n| n.starts_with("cmd /help -> ")), "{:?}", owner.notes);
    assert_eq!(report.traces.len(), 2);
    assert!(report.traces.iter().all(|(_, p)| p.exists()));
    assert!(report.received_counts.iter().any(|(a, t, c)| a == "A" && *t == 0x06 && *c >= 1));
    // the inventory step with no blockhead known fails with a hint
    let e = failure(&cfg, "client C\ninventory @LAST_BLOCKHEAD []\n");
    assert!(e.contains("no created blockhead known"), "{e}");
    // a blockhead announced to us can be addressed as @LAST_BLOCKHEAD (the notes of the session are reported too)
    let mut script =
        String::from("client Boss\nblock-tile 100 600\nwait 04\nuid\nblockhead Al 100 600\nwait 07\nsleep 300\n");
    script.push_str("inventory @LAST_BLOCKHEAD [[],[],[],[],[],[],[],[]]\nsleep 100\n");
    let created = run(&cfg, &script, &fast());
    assert!(created.is_ok(), "{:?}", created.err());
    // an explicit blockhead id is sent as is
    run(&cfg, "client D\ninventory 12345 [[],[]]\nsleep 50\n", &fast()).unwrap();
}

#[test]
fn failing_scripts_report_the_line_and_the_reason() {
    let (_server, cfg) = start();
    assert!(failure(&cfg, "client A\nclient A\n").contains("FAIL 2 client A already exists"));
    assert!(failure(&cfg, "client A\nuse Z\n").contains("FAIL 2 unknown client Z"));
    assert!(failure(&cfg, "chat nobody\n").contains("FAIL 1 no active client"));
    assert!(failure(&cfg, "sleep 10\nuid\n").contains("no active client"));
    for step in [
        "wait 55 100",
        "expect 55",
        "initial-data\nexpect-none 06 1200",
        "expect-reject 14",
        "quiet 10\nexpect-reject 3",
    ] {
        let e = failure(&cfg, &format!("client A\n{step}\n"));
        assert!(e.starts_with("FAIL "), "{step}: {e}");
    }
    assert!(failure(&cfg, "client A\nwait 55 100\n").contains("timeout: wait for 0x55 (100 ms)"));
    assert!(failure(&cfg, "client A\nexpect 55\n").contains("expected a 0x55 packet, none received"));
    assert!(failure(&cfg, "client A\ninitial-data\nexpect-none 06 1200\n")
        .contains("unexpected 0x06 packet within 1200 ms"));
    // rejected joins are recorded, not fatal
    let ok = (0..10)
        .find_map(|_| {
            run(&cfg, "client ABCDEFGHIJKLMNOPQ\nexpect-reject 14\nclient Server\nexpect-reject 2\n", &fast()).ok()
        })
        .expect("both rejections seen in ten tries");
    assert!(
        ok.notes.iter().any(|n| n.contains("join rejected 14"))
            && ok.notes.iter().any(|n| n.contains("join rejected 2"))
    );
    // the server may hang up before the rejection arrives (a disconnect drops what is still queued): try again
    let wrong = (0..10)
        .map(|_| failure(&cfg, "client ABCDEFGHIJKLMNOPQ\nexpect-reject 3\n"))
        .find(|e| e.contains("expected rejection 3"))
        .expect("rejection 14 seen in ten tries");
    assert!(wrong.contains("expected rejection 3, got Some(14)"), "{wrong}");
    // join-raw stops after the contact packet; a second 0x1f is sent by hand
    run(&cfg, "join-raw R\nsleep 100\nexpect 23\ndisconnect\n", &fast()).unwrap();
}

#[test]
fn unreachable_servers_fail_the_connect() {
    let dead = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
    let cfg = ClientConfig { addr: dead.local_addr().unwrap(), connect_timeout_ms: 300, ..Default::default() };
    for script in ["client A\n", "join-raw A\n"] {
        let e = run(&cfg, script, &fast()).unwrap_err();
        assert!(e.is_connect_failure(), "{e}");
    }
}

#[test]
fn a_silent_endpoint_times_out_the_join() {
    // an ENet host that never sends 0x23: the join reports no contact
    let mut quiet = blockheads_server::net::enet_host::EnetHost::create(0).unwrap();
    let port = quiet.local_port();
    let cfg = ClientConfig { addr: format!("127.0.0.1:{port}").parse().unwrap(), ..Default::default() };
    let handle = std::thread::spawn(move || {
        let end = std::time::Instant::now() + Duration::from_secs(4);
        while std::time::Instant::now() < end {
            let _ = quiet.service_wait(5);
        }
    });
    let opts = RunOptions { default_timeout_ms: 400, ..Default::default() };
    let e = run(&cfg, "client A\n", &opts).unwrap_err();
    assert!(e.to_string().contains("timeout: A: no 0x23 / 0x01"), "{e}");
    let e = run(&cfg, "join-raw B\n", &opts).unwrap_err();
    assert!(e.to_string().contains("timeout: B: no 0x23"), "{e}");
    handle.join().unwrap();
}

/// A fake ENet endpoint: runs `on_event` for every event until the returned guard is dropped.
struct Fake {
    port: u16,
    stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}
impl Fake {
    fn start(
        mut on_event: impl FnMut(&mut blockheads_server::net::enet_host::EnetHost, blockheads_server::net::enet_host::OwnedEvent)
            + Send
            + 'static,
    ) -> Fake {
        let mut host = blockheads_server::net::enet_host::EnetHost::create(0).unwrap();
        let port = host.local_port();
        let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let s2 = stop.clone();
        let thread = std::thread::spawn(move || {
            while !s2.load(std::sync::atomic::Ordering::SeqCst) {
                if let Some(ev) = host.service_wait(5) {
                    on_event(&mut host, ev);
                }
            }
        });
        Fake { port, stop, thread: Some(thread) }
    }
    fn cfg(&self) -> ClientConfig {
        ClientConfig { addr: format!("127.0.0.1:{}", self.port).parse().unwrap(), ..Default::default() }
    }
}
impl Drop for Fake {
    fn drop(&mut self) {
        self.stop.store(true, std::sync::atomic::Ordering::SeqCst);
        if let Some(t) = self.thread.take() {
            t.join().unwrap();
        }
    }
}

#[test]
fn a_server_that_hangs_up_or_stays_silent_after_the_handshake() {
    use blockheads_server::net::enet_host::OwnedEvent;
    use blockheads_server::protocol::messages as m;
    // drops the peer right after it connected: the join ends as "disconnected"
    let hangup = Fake::start(|host, ev| {
        if let OwnedEvent::Connect { peer, .. } = ev {
            host.peer_disconnect(peer, 0);
        }
    });
    let e = run(&hangup.cfg(), "client A\n", &fast()).unwrap_err();
    assert!(e.to_string().contains("disconnected during join"), "{e}");
    // a block with a wrong receipt hash ends up in the notes of the report
    let tiles = [0u8; blockheads_server::protocol::block_wire::TILES_LEN];
    let light = [0u8; blockheads_server::protocol::block_wire::LIGHT_LEN];
    let bad_block = blockheads_server::protocol::block_wire::server_block_from_parts(
        5,
        &tiles,
        0,
        1,
        Some((&light, &blockheads_server::protocol::block_wire::ExtraData { hash: "wrong".into(), receipts: None })),
    )
    .unwrap();
    let noisy = Fake::start(move |host, ev| match ev {
        OwnedEvent::Connect { peer, .. } => host.send_reliable(peer, 0, &m::initial_contact(None, false, "w")),
        OwnedEvent::Receive { peer, data, .. } if data.first() == Some(&0x1f) => {
            host.send_reliable(peer, 0, &m::world_summary(&m::WorldSummary::default()));
            host.send_reliable(peer, 0, &bad_block);
        }
        _ => {}
    });
    let report = run(&noisy.cfg(), "client A\nsleep 300\n", &fast()).unwrap();
    assert!(report.notes.iter().any(|n| n.starts_with("A: block 5 hash mismatch (wrong)")), "{:?}", report.notes);
    // approves the join and then ignores every request
    let mute = Fake::start(|host, ev| {
        if let OwnedEvent::Connect { peer, .. } = ev {
            host.send_reliable(peer, 0, &m::initial_contact(None, false, "w"));
        }
        if let OwnedEvent::Receive { peer, data, .. } = ev {
            if data.first() == Some(&0x1f) {
                host.send_reliable(peer, 0, &m::world_summary(&m::WorldSummary::default()));
            }
        }
    });
    let e =
        run(&mute.cfg(), "client A\nuid\n", &RunOptions { default_timeout_ms: 300, ..Default::default() }).unwrap_err();
    assert!(e.to_string().contains("unique id reply (0x0f)"), "{e}");
}

/// Steps that are not a single send do nothing in `send_step` (the runner handles them itself).
#[test]
fn non_send_steps_do_nothing_in_send_step() {
    let (_s, cfg) = start();
    let mut c = bh_testclient::Session::connect(cfg, bh_testclient::Identity::new("Nop")).unwrap();
    bh_testclient::scenario::send_step(&mut c, 1, &bh_testclient::Step::Sleep(1)).unwrap();
    bh_testclient::scenario::send_step(&mut c, 1, &bh_testclient::Step::InitialData).unwrap();
}
