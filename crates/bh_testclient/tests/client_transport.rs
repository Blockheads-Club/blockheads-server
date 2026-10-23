//! The ENet transport of the test client against a minimal endpoint of the server's own ENet host.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use bh_testclient::client::{Client, ClientConfig, NetEvent};
use bh_testclient::{Dir, TraceSink};
use blockheads_server::net::enet_host::{EnetHost, OwnedEvent};

/// Echoes every packet it receives and can be told to stop answering altogether.
struct Echo {
    port: u16,
    stop: Arc<AtomicBool>,
    silent: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}
impl Echo {
    fn start() -> Echo {
        let mut host = EnetHost::create(0).unwrap();
        let port = host.local_port();
        let stop = Arc::new(AtomicBool::new(false));
        let silent = Arc::new(AtomicBool::new(false));
        let (s2, q2) = (stop.clone(), silent.clone());
        let thread = std::thread::spawn(move || {
            while !s2.load(Ordering::SeqCst) {
                if q2.load(Ordering::SeqCst) {
                    std::thread::sleep(Duration::from_millis(5));
                    continue;
                }
                if let Some(OwnedEvent::Receive { peer, channel, data, .. }) = host.service_wait(5) {
                    host.send_reliable(peer, channel, &data);
                }
            }
        });
        Echo { port, stop, silent, thread: Some(thread) }
    }
    fn cfg(&self) -> ClientConfig {
        ClientConfig { addr: format!("127.0.0.1:{}", self.port).parse().unwrap(), ..Default::default() }
    }
}
impl Drop for Echo {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        self.thread.take().unwrap().join().unwrap();
    }
}

fn wait_packet(c: &mut Client) -> Option<bh_testclient::Rx> {
    for _ in 0..2000 {
        c.pump().unwrap();
        while let Some(ev) = c.poll() {
            if let NetEvent::Packet(rx) = ev {
                return Some(rx);
            }
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    None
}

#[test]
fn packets_travel_and_are_traced() {
    let echo = Echo::start();
    let bind = format!("127.0.0.1:{}", bh_testclient::harness::free_udp_port()).parse().unwrap();
    let mut c = Client::connect(ClientConfig { local_bind: Some(bind), ..echo.cfg() }).unwrap();
    assert!(c.is_connected());
    c.set_label("probe");
    assert_eq!(c.label(), "probe");
    let sink = TraceSink::memory();
    c.set_trace(Some(sink.clone()));
    assert!(matches!(c.poll(), Some(NetEvent::Connected)));
    c.send(&[7, 1, 2]).unwrap();
    let rx = wait_packet(&mut c).unwrap();
    assert_eq!((rx.channel, rx.reliable, rx.data.clone()), (0, true, vec![7, 1, 2]));
    c.send_on(3, &[9], true).unwrap();
    assert_eq!(wait_packet(&mut c).unwrap().channel, 3);
    c.send_on(2, &[8], false).unwrap();
    let recs = sink.records();
    assert_eq!(recs.iter().filter(|r| r.dir == Dir::Tx).count(), 3);
    assert!(recs.iter().any(|r| r.dir == Dir::Rx && r.client == "probe"));
    assert!(c.now_ms() < 10_000);
    // a channel the connection does not have is an error
    assert!(c.send_on(200, &[1], true).is_err());
    c.disconnect(0);
    assert!(!c.is_connected());
    c.disconnect(0);
    while let Some(ev) = c.poll() {
        let _ = ev;
    }
}

#[test]
fn a_dead_peer_is_reset_after_the_wait_and_silent_drops_work() {
    let echo = Echo::start();
    let mut c = Client::connect(echo.cfg()).unwrap();
    echo.silent.store(true, Ordering::SeqCst);
    let t = std::time::Instant::now();
    c.disconnect(1);
    assert!(!c.is_connected() && t.elapsed() >= Duration::from_secs(2));
    drop(c);
    echo.silent.store(false, Ordering::SeqCst);
    let c = Client::connect(echo.cfg()).unwrap();
    c.drop_silently();
}

#[test]
fn connect_failures_are_errors() {
    let dead = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
    let cfg = ClientConfig { addr: dead.local_addr().unwrap(), connect_timeout_ms: 200, ..Default::default() };
    assert_eq!(Client::connect(cfg.clone()).err().unwrap().kind(), std::io::ErrorKind::TimedOut);
    // zero channels are refused by the transport
    let e = Client::connect(ClientConfig { channels: 0, ..cfg.clone() }).err().unwrap();
    assert!(e.to_string().contains("Host::new"), "{e}");
    for channels in [255usize, 256, 1000] {
        let r = Client::connect(ClientConfig { channels, ..cfg.clone() });
        assert!(r.is_err(), "{channels}");
    }
    // a connect that cannot even be started
    let taken = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
    let e = Client::connect(ClientConfig { local_bind: Some(taken.local_addr().unwrap()), ..cfg }).err().unwrap();
    assert_eq!(e.kind(), std::io::ErrorKind::AddrInUse);
}
