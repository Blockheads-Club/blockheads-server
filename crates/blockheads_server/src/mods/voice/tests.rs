use std::net::SocketAddr;
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use super::*;

/// Records what it is handed and echoes it back to the sender with a prefix.
struct Echo {
    seen: Arc<Mutex<Vec<(String, Vec<u8>)>>>,
}

impl VoiceSink for Echo {
    fn frame(&mut self, peer: &str, body: &[u8], out: &mut dyn VoiceOut) {
        self.seen.lock().unwrap().push((peer.to_string(), body.to_vec()));
        let mut p = vec![0xbc, VOICE_FRAME];
        p.extend_from_slice(body);
        out.send(peer, &p);
    }
}

#[derive(Default)]
struct Out(Vec<(String, Vec<u8>)>);

impl VoiceOut for Out {
    fn send(&mut self, peer: &str, packet: &[u8]) -> bool {
        self.0.push((peer.to_string(), packet.to_vec()));
        true
    }
}

/// Answers every datagram with `ack:` + the datagram, and reports it on a channel.
struct UdpEcho(Mutex<mpsc::Sender<(SocketAddr, Vec<u8>)>>);

impl UdpVoiceHandler for UdpEcho {
    fn datagram(&self, from: SocketAddr, data: &[u8], out: &dyn UdpOut) {
        let _ = self.0.lock().unwrap().send((from, data.to_vec()));
        let mut a = b"ack:".to_vec();
        a.extend_from_slice(data);
        out.send_to(from, &a);
    }
}

fn echo() -> (Arc<dyn UdpVoiceHandler>, mpsc::Receiver<(SocketAddr, Vec<u8>)>) {
    let (tx, rx) = mpsc::channel();
    (Arc::new(UdpEcho(Mutex::new(tx))), rx)
}

fn client() -> UdpSocket {
    let c = UdpSocket::bind("127.0.0.1:0").unwrap();
    c.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
    c
}

fn recv(c: &UdpSocket) -> Option<Vec<u8>> {
    let mut b = [0u8; 2048];
    c.recv_from(&mut b).ok().map(|(n, _)| b[..n].to_vec())
}

#[test]
fn the_sink_gets_frames_once_installed() {
    let mut out = Out::default();
    // Nothing installed: the frame is not taken.
    set_sink(None);
    assert!(!has_sink());
    assert!(!dispatch("p", b"x", &mut out));
    let seen = Arc::new(Mutex::new(Vec::new()));
    set_sink(Some(Box::new(Echo { seen: seen.clone() })));
    assert!(has_sink());
    assert!(dispatch("p", b"abc", &mut out));
    assert_eq!(*seen.lock().unwrap(), [("p".to_string(), b"abc".to_vec())]);
    assert_eq!(out.0, [("p".to_string(), b"\xbc\x16abc".to_vec())]);
    // A relay that re-enters (a frame while a frame runs) is not called again.
    struct Reenter;
    impl VoiceSink for Reenter {
        fn frame(&mut self, _: &str, _: &[u8], out: &mut dyn VoiceOut) {
            assert!(has_sink());
            assert!(!dispatch("q", b"inner", out));
        }
    }
    set_sink(Some(Box::new(Reenter)));
    assert!(dispatch("p", b"outer", &mut out));
    set_sink(None);
}

#[test]
fn config_is_per_thread_and_printable() {
    let (h, _rx) = echo();
    let factory: VoiceSinkFactory = Arc::new(|| Box::new(Echo { seen: Arc::default() }));
    let c = VoiceConfig { udp_port: Some(7), sink: Some(factory), udp_handler: Some(h) };
    assert_eq!(format!("{c:?}"), "VoiceConfig { udp_port: Some(7), sink: true, udp_handler: true }");
    assert_eq!(
        format!("{:?}", VoiceConfig::default()),
        "VoiceConfig { udp_port: None, sink: false, udp_handler: false }"
    );
    set_config(c);
    assert_eq!(config().udp_port, Some(7));
    set_config(VoiceConfig::default());
    assert_eq!(config().udp_port, None);
}

#[test]
fn start_builds_the_sink_and_binds_the_socket() {
    // Default config: no sink, no socket.
    set_config(VoiceConfig::default());
    assert_eq!(start(), None);
    assert!(!has_sink());
    assert_eq!(udp_port(), None);
    assert!(udp_sender().is_none());
    assert!(!set_udp_handler(None));
    // Sink and socket on any free port.
    let factory: VoiceSinkFactory = Arc::new(|| Box::new(Echo { seen: Arc::default() }));
    set_config(VoiceConfig { udp_port: Some(0), sink: Some(factory), udp_handler: None });
    assert_eq!(start(), None);
    assert!(has_sink());
    let port = udp_port().unwrap();
    assert_ne!(port, 0);
    // No handler yet: datagrams are dropped. Then one is set and answers.
    let c = client();
    let to = SocketAddr::from(([127, 0, 0, 1], port));
    c.send_to(b"lost", to).unwrap();
    std::thread::sleep(Duration::from_millis(100));
    let (h, rx) = echo();
    assert!(set_udp_handler(Some(h)));
    c.send_to(b"hi", to).unwrap();
    assert_eq!(recv(&c).as_deref(), Some(&b"ack:hi"[..]));
    let (from, data) = rx.recv_timeout(Duration::from_secs(3)).unwrap();
    assert_eq!((from, data), (c.local_addr().unwrap(), b"hi".to_vec()));
    assert!(rx.try_recv().is_err(), "the datagram before the handler was dropped");
    // The server thread can send on the socket too.
    let s = udp_sender().unwrap();
    assert!(s.send_to(c.local_addr().unwrap(), b"push"));
    assert_eq!(recv(&c).as_deref(), Some(&b"push"[..]));
    // A port that is taken is reported, and nothing runs.
    let taken = UdpSocket::bind("0.0.0.0:0").unwrap();
    let busy = taken.local_addr().unwrap().port();
    set_config(VoiceConfig { udp_port: Some(busy), ..VoiceConfig::default() });
    let w = start().unwrap();
    assert!(w.starts_with(&format!("voice: UDP port {busy} could not be opened: ")), "{w}");
    assert_eq!(udp_port(), None);
    // Stop closes everything.
    set_config(VoiceConfig { udp_port: Some(0), ..VoiceConfig::default() });
    assert_eq!(start(), None);
    let t = Instant::now();
    stop();
    assert!(t.elapsed() < Duration::from_secs(2));
    assert_eq!(udp_port(), None);
    assert!(!has_sink());
    set_config(VoiceConfig::default());
}

#[test]
fn oversized_datagrams_are_not_sent() {
    let (h, _rx) = echo();
    let u = VoiceUdp::bind(0, Some(h)).unwrap();
    let c = client();
    let s = u.sender();
    assert!(!s.send_to(c.local_addr().unwrap(), &[0u8; MAX_UNRELIABLE + 1]));
    assert!(s.send_to(c.local_addr().unwrap(), &[1u8; MAX_UNRELIABLE]));
    assert_eq!(recv(&c).map(|d| d.len()), Some(MAX_UNRELIABLE));
    // A handler can be removed again; the socket keeps running.
    u.set_handler(None);
    assert_ne!(u.local_port(), 0);
}

#[test]
fn a_poisoned_handler_slot_still_works() {
    let (h, rx) = echo();
    let u = VoiceUdp::bind(0, Some(h)).unwrap();
    let slot = u.handler.clone();
    let _ = std::thread::spawn(move || {
        let _g = slot.write().unwrap();
        panic!("poison the slot");
    })
    .join();
    assert!(u.handler.is_poisoned());
    let c = client();
    c.send_to(b"p", SocketAddr::from(([127, 0, 0, 1], u.local_port()))).unwrap();
    assert_eq!(recv(&c).as_deref(), Some(&b"ack:p"[..]));
    assert!(rx.recv_timeout(Duration::from_secs(3)).is_ok());
    u.set_handler(None);
}
