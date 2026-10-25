//! Joining under a name that is already bound to another device: the vanilla rules (rejected) and the
//! "same name from another device" setting (the name is the identity; a session online under it is replaced).

use std::time::Duration;

use bh_testclient::harness::inproc::HostServer;
use bh_testclient::harness::refenv::{RunSpec, ServerHandle};
use bh_testclient::session::{Identity, JoinOutcome, Session};
use bh_testclient::ClientConfig;

fn start(tag: &str, shared: bool, console: bool) -> HostServer {
    let mut args: Vec<String> =
        ["--new", "NAMES", "--world_id", "5a3e0000a11ce000a11ce000a11ce000", "--world_width", "1/16"]
            .iter()
            .map(|s| s.to_string())
            .collect();
    // (the vanilla server stops when the last client has left)
    args.push("--no-exit".into());
    if shared {
        args.push("--shared-names".into());
    }
    let mut s = HostServer::start(&RunSpec { tag: tag.into(), args, console, ..Default::default() }).unwrap();
    s.wait_for_log("World load complete.", Duration::from_secs(120)).unwrap();
    s
}

fn cfg_for(port: u16) -> ClientConfig {
    ClientConfig { addr: format!("127.0.0.1:{port}").parse().unwrap(), ..Default::default() }
}

fn join(port: u16, id: Identity) -> (Session, JoinOutcome) {
    let mut s = Session::connect(cfg_for(port), id).unwrap();
    let r = s.join(10_000).unwrap();
    (s, r)
}

/// Leaves a session that was refused and gives the server time to notice (its own disconnect event must not come
/// in the middle of the next join).
fn leave(mut s: Session) {
    s.disconnect();
    std::thread::sleep(Duration::from_millis(1000));
}

/// Waits until the server logged that `name` left for the `n`th time (a client leaving is noticed a moment later).
fn wait_gone(server: &mut HostServer, name: &str, n: usize) {
    let end = std::time::Instant::now() + Duration::from_secs(15);
    let needle = format!("player disconnected {}", name.to_lowercase());
    while server.log().to_lowercase().matches(&needle).count() < n {
        assert!(std::time::Instant::now() < end, "{name} did not leave:\n{}", server.log());
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[test]
fn vanilla_refuses_a_name_that_belongs_to_another_device() {
    let mut server = start("names-off", false, false);
    let port = server.port();
    let (mut a, r) = join(port, Identity::new("Zed").udid("device-1"));
    assert_eq!(r, JoinOutcome::Approved);
    // online on device 1: "user logged in"
    let (b, r) = join(port, Identity::new("Zed").udid("device-2"));
    assert_eq!(r, JoinOutcome::Rejected { reason: 2 });
    leave(b);
    // device 1 is still the one playing
    a.run_for(300).unwrap();
    assert!(!a.state.disconnected && a.state.last_rejection.is_none());
    a.disconnect();
    wait_gone(&mut server, "Zed", 1);
    // offline, known on device 1: "username taken"
    let (b, r) = join(port, Identity::new("Zed").udid("device-2"));
    assert_eq!(r, JoinOutcome::Rejected { reason: 1 });
    leave(b);
    // the owner device is welcome
    let (mut a, r) = join(port, Identity::new("Zed").udid("device-1"));
    assert_eq!(r, JoinOutcome::Approved);
    a.disconnect();
}

#[test]
fn with_the_setting_the_name_is_the_identity() {
    let mut server = start("names-on", true, false);
    let port = server.port();
    let (mut a, r) = join(port, Identity::new("Zed").udid("device-1"));
    assert_eq!(r, JoinOutcome::Approved);
    // online on device 1, joins from device 2: the new login takes over
    let (b, r) = join(port, Identity::new("zed").udid("device-2"));
    assert_eq!(r, JoinOutcome::Approved);
    assert_eq!(Identity::new("zed").player_id(), Identity::new("Zed").player_id());
    // the old session is told why and is dropped
    a.wait_for(0x26, 5000).unwrap();
    assert_eq!(a.state.last_rejection, Some(2));
    assert!(a.count_of(0x24) >= 1, "the 'logged in from another location' chat line");
    a.run_for(2500).unwrap();
    assert!(a.state.disconnected);
    // the new session stays up
    b.run_for(500).unwrap();
    assert!(!b.state.disconnected && b.state.last_rejection.is_none());
    b.disconnect();
    // (the replaced session left once already)
    wait_gone(&mut server, "Zed", 2);
    // offline, known on other devices: a third device plays as the same player
    let (mut c, r) = join(port, Identity::new("Zed").udid("device-3"));
    assert_eq!(r, JoinOutcome::Approved);
    c.disconnect();
    wait_gone(&mut server, "Zed", 3);
    let log = server.log();
    let ids: std::collections::HashSet<&str> = log
        .lines()
        .filter(|l| l.contains("Player Connected Zed") || l.contains("Player Connected zed"))
        .filter_map(|l| l.rsplit(" | ").next())
        .collect();
    assert_eq!(ids.len(), 1, "one player id for all devices: {ids:?}");
}

#[test]
fn a_join_that_fails_does_not_log_the_player_out() {
    let mut server = start("names-pw", true, true);
    server.send_console("/set-password secret").unwrap();
    std::thread::sleep(Duration::from_millis(500));
    let port = server.port();
    let (mut a, r) = join(port, Identity::new("Zed").udid("device-1").password("secret"));
    assert_eq!(r, JoinOutcome::Approved);
    // wrong password from another device: refused, and device 1 keeps playing
    let (b, r) = join(port, Identity::new("Zed").udid("device-2").password("nope"));
    assert_eq!(r, JoinOutcome::Rejected { reason: 10 });
    leave(b);
    a.run_for(2500).unwrap();
    assert!(!a.state.disconnected && a.state.last_rejection.is_none());
    // the right password takes over
    let (mut c, r) = join(port, Identity::new("Zed").udid("device-2").password("secret"));
    assert_eq!(r, JoinOutcome::Approved);
    a.run_for(2500).unwrap();
    assert!(a.state.disconnected);
    c.disconnect();
}
