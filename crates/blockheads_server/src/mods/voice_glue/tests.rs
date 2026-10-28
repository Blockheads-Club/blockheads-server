use std::cell::RefCell;
use std::net::{Ipv4Addr, SocketAddr};
use std::sync::Arc;

use bhmod_core::voice::{Caps, Icons, Mode, Route, Transport, TransportConfig, VoiceEvent, VoiceHost, VoiceState};
use bhmod_proto::{feature, Message};
use bhmod_voice::client::{Outgoing, VoiceClient};
use bhmod_voice::codec::TestCodec;
use bhmod_voice::crypto::KeyPair;
use bhmod_voice::server::{ServerSink, VoiceServer};
use bhmod_voice::FRAME_SAMPLES;

use super::*;

/// What a sink sent reliably: `(player, packet)`.
type Sent = Vec<(String, Vec<u8>)>;

/// Records UDP sends.
#[derive(Default)]
struct Udp(RefCell<Vec<(SocketAddr, Vec<u8>)>>);

impl UdpOut for Udp {
    fn send_to(&self, to: SocketAddr, data: &[u8]) -> bool {
        self.0.borrow_mut().push((to, data.to_vec()));
        true
    }
}

fn addr(port: u16) -> SocketAddr {
    SocketAddr::from((Ipv4Addr::LOCALHOST, port))
}

/// Runs `f` with a recording thread sink.
fn with_sink<R>(g: &VoiceGlue, udp: Option<&Udp>, f: impl FnOnce(&mut ThreadSink<'_>) -> R) -> (R, Sent) {
    let mut sent = Sent::new();
    let mut send = |p: &str, packet: Vec<u8>| sent.push((p.to_string(), packet));
    let mut sink = ThreadSink { send: &mut send, bound: g.bound(), udp: udp.map(|u| u as &dyn UdpOut) };
    let r = f(&mut sink);
    (r, sent)
}

/// A glue with deterministic keys, its socket "open" on `udp_port`, and `players` joined with a voice session each
/// (the clients answered).
fn session(players: &[&str], udp_port: u16) -> (VoiceGlue, Vec<VoiceClient>) {
    let mut n = 0u8;
    let server = VoiceServer::with_keys(Box::new(move || {
        n += 1;
        Some(KeyPair::from_secret([n; 32]))
    }));
    let g = VoiceGlue::new(server);
    assert_eq!(g.sync_udp(Some(udp_port), &mut Ok, &mut || {}), None);
    let mut clients = Vec::new();
    for (i, p) in players.iter().enumerate() {
        g.join(p, &p.to_uppercase(), &[feature::VOICE.to_string()]);
        let mut c = VoiceClient::new(Arc::new(TestCodec));
        let key = c.start_with(KeyPair::from_secret([100 + i as u8; 32]));
        let m = Message::decode(&key).unwrap();
        let (_, sent) = with_sink(&g, None, |s| g.server.lock().receive(1, p, &m, s));
        for (to, packet) in sent {
            assert_eq!(to, *p);
            c.on_packet(&packet);
        }
        assert!(c.connected());
        clients.push(c);
    }
    (g, clients)
}

/// Gives `client` what the relay sent it (`VOICE_STATE` deltas).
fn deliver(g: &VoiceGlue, player: &str, client: &mut VoiceClient, now: u64) {
    let (_, sent) = with_sink(g, None, |s| g.server.lock().tick(now, s));
    for (_, packet) in sent.iter().filter(|x| x.0 == player) {
        client.on_packet(packet);
    }
}

/// `client` talks in proximity mode for one frame and returns what it sends.
fn talk(g: &VoiceGlue, player: &str, client: &mut VoiceClient) -> Vec<Outgoing> {
    g.set_allowed(player, Mode::Proximity.bit(), 0b111).unwrap();
    deliver(g, player, client, 2);
    client.set_mic(true);
    client.talk(Some(Mode::Proximity), true);
    client.on_mic_frame(&[0.25; FRAME_SAMPLES]);
    client.poll_send(3)
}

fn udp_of(out: Vec<Outgoing>) -> Vec<Vec<u8>> {
    out.into_iter()
        .filter_map(|o| match o {
            Outgoing::Udp(d) => Some(d),
            Outgoing::Reliable(_) => None,
        })
        .collect()
}

#[test]
fn time_and_wanted_ports() {
    assert!(unix_ms() > 1_600_000_000_000);
    let g = VoiceGlue::default();
    // No request: the configured port, else the game port + 1.
    assert_eq!(g.wanted_port(Some(9), 15151), 9);
    assert_eq!(g.wanted_port(None, 15151), 15152);
    // The mod's port wins; 0 means "not set"; the mode does not matter.
    g.set_transport(TransportConfig { mode: Transport::Inband, udp_port: Some(7) }).unwrap();
    assert_eq!(g.wanted_port(Some(9), 15151), 7);
    g.set_transport(TransportConfig { mode: Transport::Udp, udp_port: Some(0) }).unwrap();
    assert_eq!(g.wanted_port(None, 15151), 15152);
    g.set_transport(TransportConfig { mode: Transport::Auto, udp_port: None }).unwrap();
    assert_eq!(g.wanted_port(Some(0), 15151), 0);
}

#[test]
fn the_socket_follows_the_wanted_port() {
    let g = VoiceGlue::new(VoiceServer::new());
    let opened = RefCell::new(Vec::new());
    let closed = RefCell::new(0);
    let mut open = |p: u16| {
        opened.borrow_mut().push(p);
        if p == 13 {
            Err("taken".to_string())
        } else {
            Ok(if p == 0 { 40_000 } else { p })
        }
    };
    let mut close = || *closed.borrow_mut() += 1;
    // Nothing wanted at first: nothing happens.
    assert_eq!(g.sync_udp(None, &mut open, &mut close), None);
    assert_eq!(*closed.borrow(), 0);
    // Any free port: the relay announces the one it got; asking again changes nothing.
    assert_eq!(g.sync_udp(Some(0), &mut open, &mut close), None);
    assert_eq!(g.udp_port(), Some(40_000));
    assert_eq!(g.sync_udp(Some(0), &mut open, &mut close), None);
    assert_eq!((opened.borrow().clone(), *closed.borrow()), (vec![0], 1));
    // A port that cannot be opened: warned once, the relay announces none.
    let w = g.sync_udp(Some(13), &mut open, &mut close).unwrap();
    assert_eq!(w, "voice: UDP port 13 could not be opened (taken); voice chat is off");
    assert_eq!(g.udp_port(), None);
    assert_eq!(g.sync_udp(Some(13), &mut open, &mut close), None);
    // Closed again when no voice mod is left.
    assert_eq!(g.sync_udp(Some(24_001), &mut open, &mut close), None);
    assert_eq!(g.udp_port(), Some(24_001));
    assert_eq!(g.sync_udp(None, &mut open, &mut close), None);
    assert_eq!(g.udp_port(), None);
    assert_eq!((opened.borrow().len(), *closed.borrow()), (3, 4));
}

#[test]
fn the_mod_policy_reaches_the_relay() {
    let (g, _) = session(&["a", "b"], 24_000);
    assert_eq!(g.state("a"), Ok(VoiceState::Ready));
    assert_eq!(g.state("x"), Ok(VoiceState::None));
    let icons = Icons { speaking: false, ..Icons::default() };
    g.set_icons(icons).unwrap();
    assert_eq!(g.icons(), Ok(icons));
    g.mute("a", None).unwrap();
    assert_eq!(g.state("a"), Ok(VoiceState::AdminMuted));
    assert_eq!(g.unmute("a"), Ok(true));
    assert_eq!(g.unmute("a"), Ok(false));
    g.set_speaker_gain("a", 50).unwrap();
    g.set_caps(Caps { max_bitrate: 16_000, max_speakers: 2, max_frames_per_sec: 50 }).unwrap();
    g.set_broadcast_overrides_deafen(true).unwrap();
    g.set_routes("a", &[]).unwrap();
    // The player leaves (no UDP address to forget, then one).
    let _ = g.take_events();
    g.leave("a");
    g.leave("nobody");
    lock(g.bound()).insert(2, addr(5));
    g.leave("b");
    assert!(lock(g.bound()).is_empty());
}

#[test]
fn udp_datagrams_bind_peers_and_are_relayed() {
    let (g, mut c) = session(&["a", "b"], 24_002);
    assert_eq!(c[0].udp_port(), Some(24_002), "VOICE_KEY announces the voice port");
    let route = Route { listener: "b".into(), modes: 0, gain: 1.0, pan: 0.0, muffle: 0.0 };
    g.set_routes("a", &[route]).unwrap();
    let h = g.udp_handler();
    let udp = Udp::default();
    // Both clients authenticate from their addresses and get AUTH_OK there.
    for (i, cl) in c.iter_mut().enumerate() {
        let auth = udp_of(cl.poll_send(1)).pop().expect("AUTH");
        h.datagram(addr(5000 + i as u16), &auth, &udp);
        let ok = udp.0.borrow().last().unwrap().clone();
        assert_eq!(ok.0, addr(5000 + i as u16));
        cl.on_udp(&ok.1);
    }
    assert_eq!(lock(g.bound()).len(), 2);
    // Garbage is dropped.
    h.datagram(addr(9), &[0], &udp);
    assert_eq!(udp.0.borrow().len(), 2);
    // A frame of `a`: from the wrong address dropped, from its own relayed to `b`'s address.
    let frames = udp_of(talk(&g, "a", &mut c[0]));
    assert!(!frames.is_empty(), "the frame leaves as a datagram");
    let sent = udp.0.borrow().len();
    for d in &frames {
        h.datagram(addr(7), d, &udp);
    }
    assert_eq!(udp.0.borrow().len(), sent);
    for d in &frames {
        h.datagram(addr(5000), d, &udp);
    }
    let to_b: Vec<Vec<u8>> = udp.0.borrow()[sent..].iter().filter(|x| x.0 == addr(5001)).map(|x| x.1.clone()).collect();
    assert!(!to_b.is_empty(), "relayed to b");
    deliver(&g, "b", &mut c[1], 4);
    for d in &to_b {
        c[1].on_udp(d);
    }
    assert!(c[1].take_events().iter().any(|e| matches!(e, VoiceEvent::TalkStart { player, .. } if player == "a")));
    assert!(g.take_events().iter().any(|e| matches!(e, VoiceEvent::TalkStart { player, .. } if player == "a")));
    // The server thread's sink sends to bound peers only, and only with a socket.
    let (_, sent) = with_sink(&g, Some(&udp), |s| {
        s.udp(1, vec![1]);
        s.udp(99, vec![2]);
        s.reliable("a", vec![3]);
    });
    assert_eq!(sent, [("a".to_string(), vec![3])]);
    assert!(udp.0.borrow().iter().any(|x| x.1 == [1]) && !udp.0.borrow().iter().any(|x| x.1 == [2]));
    with_sink(&g, None, |s| s.udp(1, vec![5]));
    assert!(!udp.0.borrow().iter().any(|x| x.1 == [5]));
    // The UDP thread's own sink queues reliable output for the server thread.
    let mut t = UdpThreadSink::default();
    t.reliable("a", vec![6]);
    assert_eq!(t.queued, [("a".to_string(), vec![6])]);
    g.queue_for_test([("b".to_string(), vec![7])]);
    assert_eq!(g.drain(), [("b".to_string(), vec![7])]);
}
