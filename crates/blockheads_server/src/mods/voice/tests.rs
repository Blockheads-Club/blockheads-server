use std::net::SocketAddr;
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use super::*;

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
fn config_is_per_thread_and_printable() {
    let (h, _rx) = echo();
    let c = VoiceConfig { udp_port: Some(7), udp_handler: Some(h) };
    assert_eq!(format!("{c:?}"), "VoiceConfig { udp_port: Some(7), udp_handler: true }");
    assert_eq!(format!("{:?}", VoiceConfig::default()), "VoiceConfig { udp_port: None, udp_handler: false }");
    set_config(c);
    assert_eq!(config().udp_port, Some(7));
    set_config(VoiceConfig::default());
    assert_eq!(config().udp_port, None);
}

#[test]
fn the_socket_answers_and_the_server_thread_can_send() {
    assert_eq!(udp_port(), None);
    assert!(udp_sender().is_none());
    // Without a handler datagrams are dropped.
    let port = open_udp(0, None).unwrap();
    assert_ne!(port, 0);
    let c = client();
    let to = SocketAddr::from(([127, 0, 0, 1], port));
    c.send_to(b"lost", to).unwrap();
    std::thread::sleep(Duration::from_millis(100));
    let (h, rx) = echo();
    let port = open_udp(0, Some(h)).unwrap();
    let to = SocketAddr::from(([127, 0, 0, 1], port));
    c.send_to(b"hi", to).unwrap();
    assert_eq!(recv(&c).as_deref(), Some(&b"ack:hi"[..]));
    let (from, data) = rx.recv_timeout(Duration::from_secs(3)).unwrap();
    assert_eq!((from, data), (c.local_addr().unwrap(), b"hi".to_vec()));
    let s = udp_sender().unwrap();
    assert!(s.send_to(c.local_addr().unwrap(), b"push"));
    assert_eq!(recv(&c).as_deref(), Some(&b"push"[..]));
    // Stop closes it quickly.
    let t = Instant::now();
    stop();
    assert!(t.elapsed() < Duration::from_secs(2));
    assert_eq!(udp_port(), None);
}

#[test]
fn oversized_datagrams_are_not_sent() {
    let (h, _rx) = echo();
    let u = VoiceUdp::bind(0, Some(h)).unwrap();
    let c = client();
    let s = u.sender();
    assert!(!s.send_to(c.local_addr().unwrap(), &[0u8; MAX_SEND + 1]));
    assert!(s.send_to(c.local_addr().unwrap(), &[1u8; MAX_SEND]));
    assert_eq!(recv(&c).map(|d| d.len()), Some(MAX_SEND));
    assert_ne!(u.local_port(), 0);
}

#[test]
fn the_socket_is_opened_and_closed_on_demand() {
    let (h, rx) = echo();
    let port = open_udp(0, Some(h)).expect("open");
    assert_eq!(udp_port(), Some(port));
    let c = client();
    c.send_to(b"hi", ("127.0.0.1", port)).unwrap();
    assert_eq!(rx.recv_timeout(Duration::from_secs(3)).unwrap().1, b"hi");
    // Reopening closes the old socket first; a taken port fails and leaves none.
    let taken = UdpSocket::bind("0.0.0.0:0").unwrap();
    assert!(open_udp(taken.local_addr().unwrap().port(), None).is_err());
    assert_eq!(udp_port(), None);
    close_udp();
    assert_eq!(udp_port(), None);
}
