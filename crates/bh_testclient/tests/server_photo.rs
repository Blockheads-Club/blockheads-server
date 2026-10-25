//! The server's chat avatar: with a configured photo the player list carries a local "SERVER" player with that
//! photo (the client shows it next to the server's chat lines); without one the list is the vanilla one.

use std::time::Duration;

use bh_testclient::harness::inproc::HostServer;
use bh_testclient::harness::refenv::{RunSpec, ServerHandle};
use bh_testclient::session::{Identity, JoinOutcome, Session};
use bh_testclient::ClientConfig;
use blockheads_server::plist::value::PlistValue as PV;

const PHOTO: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];

fn start(tag: &str, photo: bool) -> HostServer {
    let mut args: Vec<String> =
        ["--new", "PHOTO", "--world_id", "5a3e0000a11ce000a11ce000a11ce001", "--world_width", "1/16", "--no-exit"]
            .iter()
            .map(|s| s.to_string())
            .collect();
    if photo {
        args.push("--server-photo-hex".into());
        args.push(PHOTO.iter().map(|b| format!("{b:02x}")).collect());
    }
    let mut s = HostServer::start(&RunSpec { tag: tag.into(), args, ..Default::default() }).unwrap();
    s.wait_for_log("World load complete.", Duration::from_secs(120)).unwrap();
    s
}

/// The player-list entries a fresh client receives right after joining.
fn list_after_join(port: u16, name: &str) -> Vec<PV> {
    let cfg = ClientConfig { addr: format!("127.0.0.1:{port}").parse().unwrap(), ..Default::default() };
    let mut s = Session::connect(cfg, Identity::new(name)).unwrap();
    assert_eq!(s.join(10_000).unwrap(), JoinOutcome::Approved);
    s.run_for(1500).unwrap();
    let list = s.state.players.clone();
    s.disconnect();
    match list {
        Some(PV::Array(a)) => a.items.clone(),
        _ => Vec::new(),
    }
}

#[test]
fn a_joining_client_gets_the_server_photo_in_the_player_list() {
    let server = start("srvphoto-on", true);
    let list = list_after_join(server.port(), "Zed");
    let me = list.iter().find(|e| e.object_for_key("alias").and_then(|v| v.as_str()) == Some("SERVER")).unwrap();
    assert!(matches!(me.object_for_key("photo"), Some(PV::Data(d)) if d.bytes == PHOTO));
    // the id the server's chat lines carry
    assert_eq!(me.object_for_key("playerID").and_then(|v| v.as_str()), Some(&*Identity::new("server").player_id()));
    // a client that joins later gets it too
    let list = list_after_join(server.port(), "Amy");
    assert!(list.iter().any(|e| matches!(e.object_for_key("photo"), Some(PV::Data(d)) if d.bytes == PHOTO)));
}

#[test]
fn without_a_server_photo_the_list_has_no_server_player() {
    let server = start("srvphoto-off", false);
    let list = list_after_join(server.port(), "Zed");
    assert!(list.iter().all(|e| e.object_for_key("photo").is_none()));
    assert!(list.iter().all(|e| e.object_for_key("alias").and_then(|v| v.as_str()) != Some("SERVER")));
}
