//! Session parity: the same client script against the reference (twice, for the volatile mask) and against ours,
//! normalised traces compared Strict.
//!
//!  Needs docker and `BH_PARITY=1`; skipped otherwise.
//! Requires `cargo build -p blockheads_server` first (or `BH_SERVER_BIN`).

use std::net::SocketAddr;
use std::time::Duration;

use bh_testclient::harness::normalize::*;
use bh_testclient::harness::refenv::*;
use bh_testclient::harness::save_diff::*;
use bh_testclient::harness::trace_diff::*;
use bh_testclient::harness::*;

const WORLD: &str = "e98c5bee38be74e00ad7748a3dd42119";

/// Solo flow (; scenario DSL of).
const SOLO: &str = "client TESTER\ninitial-data\nwait 06\nwait 1e 6000\nuid\nhb-old\nwait 17 6000\nblock 0\nwait 04\nsleep 700\nblock 100000 create=0\nwait 27\nquiet 1500\ndisconnect\n";
const REJECTIONS: &str = "client A\nclient ABCDEFGHIJKLMNOPQ\nexpect-reject 14\nclient Server\nexpect-reject 2\nclient A\nexpect-reject 1\nuse A\ndisconnect\n";
const MALFORMED: &str =
    "client TCA\nsend 0301\nsleep 1000\nsend 15aa\nsleep 1000\nsend fe\nsleep 1500\nuid\nquiet 1000\n";

fn server_args() -> Vec<String> {
    [
        "--new",
        "PROTO",
        "--world_id",
        WORLD,
        "--world_width",
        "1/4",
        "--custom-rules",
        "{\"seed\":\"custom\",\"custom_seed_value\":\"12345\"}",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect()
}

fn two_clients_script() -> String {
    let p = private_docs().join("protocol_vectors/two_clients/scriptA.txt");
    let a = std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("{}: {}", p.display(), e));
    format!("client B\nclient A\n{}\nuse B\nquiet 500\ndisconnect\nuse A\ndisconnect\n", a)
}

fn addr(port: u16) -> SocketAddr {
    format!("127.0.0.1:{}", port).parse().unwrap()
}

/// Start a server, run the script, stop it; returns the trace and the exit code and keeps the handle for save inspection.
fn run_on(h: &mut dyn ServerHandle, script: &str) -> (Vec<TraceEvent>, Option<i32>) {
    h.wait_for_log("World load complete.", Duration::from_secs(90)).expect("load");
    let dir = TempDir::new("trace").unwrap();
    let ev = run_script(script, addr(h.port()), dir.path()).expect("script");
    let code = if h.is_running() { None } else { Some(0) };
    (ev, code)
}

fn parity(tag: &str, script: &str) {
    parity_with(tag, script, server_args());
}

/// The sample world of the built-in scripts: width 1 (their block indices go up to 16383).
fn builtin_args() -> Vec<String> {
    let mut a = server_args();
    a[5] = "1".into(); // --world_width 1, same seed
    a
}

/// A built-in `bh_client` scenario (scenario.rs) under the same strict trace parity.
fn builtin(name: &str) {
    let script = bh_testclient::scenario::builtin_script(name).unwrap_or_else(|| panic!("no builtin {}", name));
    parity_with(&format!("builtin-{}", name), script, builtin_args());
}

fn parity_with(tag: &str, script: &str, args: Vec<String>) {
    if !parity_enabled() {
        eprintln!("SKIP: {} (BH_PARITY=1 + docker required)", tag);
        return;
    }
    let spec = |t: &str| RunSpec { tag: format!("{}-{}", tag, t), args: args.clone(), ..Default::default() };
    let mut r1 = RefServer::start(&spec("r1")).unwrap();
    let (t1, c1) = run_on(&mut r1, script);
    drop(r1);
    let mut r2 = RefServer::start(&spec("r2")).unwrap();
    let (t2, _) = run_on(&mut r2, script);
    drop(r2);
    let mut o = OurServer::start(&spec("ours")).unwrap();
    let (t3, c3) = run_on(&mut o, script);
    assert_eq!(c1, c3, "{}: server liveness after the script differs (ref {:?} ours {:?})", tag, c1, c3);
    let mut opts = NormalizeOptions::default();
    let (n1, n2) = (normalize_trace(&t1, &opts), normalize_trace(&t2, &opts));
    opts.volatile = Some(VolatileMask::learn(&[n1, n2]));
    let (a, b) = (normalize_trace(&t1, &opts), normalize_trace(&t3, &opts));
    let rep = diff_traces(&a, &b, DiffMode::Strict);
    assert!(rep.is_clean(), "{}:\n{}", tag, rep.render());
}

#[test]
fn session_solo() {
    parity("solo", SOLO);
}
#[test]
fn session_two_clients() {
    if !parity_enabled() {
        eprintln!("SKIP: session_two_clients");
        return;
    }
    parity("two", &two_clients_script());
}
#[test]
fn session_rejections() {
    parity("rej", REJECTIONS);
}
#[test]
fn session_malformed() {
    parity("mal", MALFORMED);
}

/// Solo session, disconnect, wait two saves, SIGINT, compare the two saves (dates masked).
#[test]
fn session_save_after() {
    if !parity_enabled() {
        eprintln!("SKIP: session_save_after");
        return;
    }
    let mut saves = Vec::new();
    let mut handles: Vec<Box<dyn ServerHandle>> = Vec::new();
    for ours in [false, true] {
        let spec = RunSpec { tag: "saveafter".into(), args: server_args(), ..Default::default() };
        let mut h: Box<dyn ServerHandle> =
            if ours { Box::new(OurServer::start(&spec).unwrap()) } else { Box::new(RefServer::start(&spec).unwrap()) };
        let _ = run_on(h.as_mut(), SOLO);
        h.wait_for_saves(WORLD, 2, Duration::from_secs(30)).unwrap();
        h.stop_sigint(Duration::from_secs(10)).unwrap();
        saves.push(h.save_dir(WORLD));
        handles.push(h);
    }
    let rep = diff_saves(&saves[0], &saves[1], &SaveDiffOptions::parity_default()).unwrap();
    assert!(rep.is_clean(), "{}", rep.render(80));
}

#[test]
fn builtin_block_requests() {
    builtin("block_requests");
}
#[test]
fn builtin_fill_remove() {
    builtin("fill_remove");
}
#[test]
fn builtin_chat_relay() {
    builtin("chat_relay");
}
#[test]
fn builtin_blockhead_create() {
    builtin("blockhead_create");
}
#[test]
fn builtin_heartbeat() {
    builtin("heartbeat");
}
