//! ENet wire-compatibility probe: connects to a (reference) Blockheads server with
//! rusty_enet, logs every raw UDP datagram in/out (hex) and every ENet event.
//! Usage: enet_probe [host:port] [channel_count] [seconds] [connect_data]
use rusty_enet as enet;
use std::net::{SocketAddr, UdpSocket};
use std::time::{Duration, Instant};

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect::<Vec<_>>().join(" ")
}

/// UdpSocket wrapper that dumps raw ENet datagrams.
struct LogSock(UdpSocket, Instant);
impl enet::Socket for LogSock {
    type Address = SocketAddr;
    type Error = std::io::Error;
    fn init(&mut self, o: enet::SocketOptions) -> Result<(), Self::Error> {
        enet::Socket::init(&mut self.0, o)
    }
    fn send(&mut self, a: SocketAddr, buf: &[u8]) -> Result<usize, Self::Error> {
        println!("[{:>6}ms] UDP OUT {:>4}B -> {a}: {}", self.1.elapsed().as_millis(), buf.len(), hex(buf));
        enet::Socket::send(&mut self.0, a, buf)
    }
    fn receive(
        &mut self,
        buf: &mut [u8; enet::MTU_MAX],
    ) -> Result<Option<(SocketAddr, enet::PacketReceived)>, Self::Error> {
        let r = enet::Socket::receive(&mut self.0, buf)?;
        if let Some((a, enet::PacketReceived::Complete(n))) = &r {
            println!("[{:>6}ms] UDP IN  {:>4}B <- {a}: {}", self.1.elapsed().as_millis(), n, hex(&buf[..*n]));
        }
        Ok(r)
    }
}

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let addr: SocketAddr = args.get(1).map(String::as_str).unwrap_or("127.0.0.1:16200").parse()?;
    let channels: usize = args.get(2).map(|s| s.parse()).transpose()?.unwrap_or(8);
    let secs: u64 = args.get(3).map(|s| s.parse()).transpose()?.unwrap_or(5);
    let data: u32 = args.get(4).map(|s| s.parse()).transpose()?.unwrap_or(0);
    let t0 = Instant::now();
    let sock = LogSock(UdpSocket::bind("0.0.0.0:0")?, t0);
    let mut host =
        enet::Host::new(sock, enet::HostSettings { peer_limit: 1, channel_limit: channels, ..Default::default() })?;
    host.connect(addr, channels, data)?;
    let deadline = t0 + Duration::from_secs(secs);
    while Instant::now() < deadline {
        while let Some(ev) = host.service()? {
            let ms = t0.elapsed().as_millis();
            match ev {
                enet::Event::Connect { peer, data } => println!(
                    "[{ms:>6}ms] EVENT CONNECT peer={:?} data={data} (handshake CONNECT/VERIFY_CONNECT ok)",
                    peer.id()
                ),
                enet::Event::Disconnect { data, .. } => {
                    println!("[{ms:>6}ms] EVENT DISCONNECT data={data}");
                    return Ok(());
                }
                enet::Event::Receive { channel_id, packet, .. } => println!(
                    "[{ms:>6}ms] EVENT RECEIVE ch={channel_id} kind={:?} len={} data={}",
                    packet.kind(),
                    packet.data().len(),
                    hex(packet.data())
                ),
            }
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    println!("[{:>6}ms] probe timeout reached, disconnecting", t0.elapsed().as_millis());
    if let Some(p) = host.connected_peers_mut().next() {
        p.disconnect(0);
    }
    host.flush();
    Ok(())
}
