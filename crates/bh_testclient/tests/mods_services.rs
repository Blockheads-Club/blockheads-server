//! Client <-> server service calls (0xBC `SERVICE_CALL` / `SERVICE_RESULT`, mod-sdk API 0.3.1) on a live server
//! (in-process): the modded test client calls a service of a Luau server mod and answers the server mod's calls.
//! What the server decides is checked on the wire: the opt-in (`remote = true`, `remote-methods`), the caller mod,
//! the per-player rate limit, the provider's budget and the vanilla rules.

use std::net::SocketAddr;
use std::path::Path;
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use bh_testclient::{ClientConfig, Identity, JoinOutcome, LoaderOptions, ModClient, Session};
use bhmod_proto::cbor::{self, Value};
use bhmod_proto::{service_error as code, ServiceCall, ServiceResult, PACKET_TYPE};
use blockheads_server::host::{self, HostConfig, ServerHandle, Status, WorldSelect};
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

struct Running {
    handle: ServerHandle,
    lines: Arc<Mutex<Vec<String>>>,
    port: u16,
}

impl Running {
    fn log(&self) -> String {
        self.lines.lock().unwrap().join("\n")
    }
    fn count(&self, needle: &str) -> usize {
        self.lines.lock().unwrap().iter().filter(|l| l.contains(needle)).count()
    }
    fn wait_log(&self, needle: &str, secs: u64) -> bool {
        let end = Instant::now() + Duration::from_secs(secs);
        while Instant::now() < end {
            if self.count(needle) > 0 {
                return true;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        false
    }
    fn session(&self, alias: &str) -> Session {
        let cfg = ClientConfig { addr: SocketAddr::from(([127, 0, 0, 1], self.port)), ..Default::default() };
        Session::connect(cfg, Identity::new(alias)).unwrap()
    }
}

fn start(data: &Path, world: &str, port: u16, mods: &Path) -> Running {
    let lines = Arc::new(Mutex::new(Vec::<String>::new()));
    let l2 = lines.clone();
    let cfg = HostConfig {
        data_dir: Some(data.to_path_buf()),
        world: WorldSelect::Load(world.to_string()),
        port: Some(port),
        mods: ModsConfig { dir: Some(mods.to_path_buf()), disabled: false },
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

const MOD_TOML: &str = r#"schema = 1
id = "svc"
version = "1.0.0"
name = { en = "Service test" }
authors = ["t"]
license = "MIT"
api = "^0.3.1"
[sides]
server = true
client = true
[entry]
server = { runtime = "lua", path = "server/main.luau" }
client = { runtime = "lua", path = "client/main.luau" }
[permissions]
channels = true
[provides]
services = [
  { name = "prices", version = "1.2.0", wit = "wit/prices.wit", remote = true, remote-methods = ["price", "who", "boom", "spin", "huge"] },
  { name = "vault", version = "1.0.0", wit = "wit/prices.wit" },
]
"#;

const SERVER: &str = r#"
local mod = {}
function mod.on_enable()
    local prices = { apple = 3, bread = 5 }
    bh.services.provide("prices", "1.2.0", {
        price = function(item) return prices[item] end,
        who = function()
            local c = bh.services.caller()
            return { mod = c.mod, alias = c.player:alias(), id = c.player:id() }
        end,
        boom = function() error("kaboom", 0) end,
        spin = function() while true do end end,
        huge = function() return string.rep("x", 40000) end,
        secret = function() return "not for clients" end,
    })
    bh.services.provide("vault", "1.0.0", { peek = function() return 1 end })
    -- server -> client
    bh.commands.register({ name = "askclient" }, function(sender)
        bh.services.call_remote(sender.player, "hud", "^1", "title", { 7 }, function(ok, v)
            bh.log.info("hud " .. tostring(ok) .. " " .. (ok and tostring(v) or v.kind))
        end, 1000)
        return "asked"
    end)
end
return mod
"#;

fn write_mod(mods: &Path) {
    let d = mods.join("svc");
    for sub in ["server", "client", "wit"] {
        std::fs::create_dir_all(d.join(sub)).unwrap();
    }
    std::fs::write(d.join("mod.toml"), MOD_TOML).unwrap();
    std::fs::write(d.join("server/main.luau"), SERVER).unwrap();
    std::fs::write(d.join("client/main.luau"), "return {}").unwrap();
    std::fs::write(d.join("wit/prices.wit"), "package svc:prices@1.0.0;\n").unwrap();
    std::fs::write(mods.join("mods.toml"), "[svc]\ngrant = [\"channels\"]\n").unwrap();
}

fn call(id: u32, mod_id: &str, service: &str, method: &str, args: Vec<Value>) -> ServiceCall {
    ServiceCall {
        id,
        mod_id: mod_id.into(),
        service: service.into(),
        version_req: "^1".into(),
        method: method.into(),
        args: cbor::encode(&Value::Array(args)),
    }
}

/// Calls and waits for the answer.
fn ask(mc: &mut ModClient, s: &mut Session, c: ServiceCall) -> Result<Value, (u8, String)> {
    let id = c.id;
    mc.call_service(s, c).unwrap();
    let r: ServiceResult = mc.wait_service_result(s, 10_000).unwrap().expect("an answer");
    assert_eq!(r.id, id);
    match r.outcome {
        Ok(v) => Ok(cbor::decode(&v).unwrap()),
        Err(f) => Err((f.code, f.message)),
    }
}

fn text(s: &str) -> Value {
    Value::text(s)
}

fn get<'a>(v: &'a Value, key: &str) -> &'a Value {
    let Value::Map(m) = v else { panic!("{v:?}") };
    &m.iter().find(|(k, _)| *k == text(key)).unwrap().1
}

fn join(srv: &Running, alias: &str, cache: &Path) -> (Session, ModClient) {
    let mut s = srv.session(alias);
    let mut mc = ModClient::new(LoaderOptions::new(cache));
    assert_eq!(mc.join(&mut s, 15_000).unwrap(), JoinOutcome::Approved, "{}", srv.log());
    (s, mc)
}

#[test]
fn clients_call_server_services_and_the_server_calls_clients() {
    let _w = watchdog("services", 240);
    // Rate limit of the server: bursts of 5, one more per second (set before the server reads it).
    std::env::set_var("BLOCKHEADS_MODS_REMOTE_BURST", "5");
    std::env::set_var("BLOCKHEADS_MODS_REMOTE_RATE", "1");
    let tmp = tempfile::tempdir().unwrap();
    let data = tmp.path().join("TheBlockheads");
    let w = host::create_world(Some(&data), "Services", 104, false).expect("create");
    let mods = tmp.path().join("mods");
    write_mod(&mods);
    let mut srv = start(&data, &w.id, 15490, &mods);
    assert!(srv.wait_log("[mods] svc: enabled", 10), "{}", srv.log());
    let cache = tmp.path().join("cache");

    // The opt-in, the answers and the caller (ALICE: 5 calls use her burst).
    let (mut alice, mut ma) = join(&srv, "ALICE", &cache);
    assert_eq!(ask(&mut ma, &mut alice, call(1, "svc", "prices", "price", vec![text("apple")])), Ok(Value::Uint(3)));
    let who = ask(&mut ma, &mut alice, call(2, "svc", "prices", "who", vec![])).unwrap();
    assert_eq!(get(&who, "mod"), &text("svc"));
    assert_eq!(get(&who, "alias"), &text("ALICE"));
    assert_eq!(get(&who, "id"), &text(&Identity::new("ALICE").player_id()));
    // A service or a method the provider did not expose is not callable, whatever it is.
    let (c, _) = ask(&mut ma, &mut alice, call(3, "svc", "prices", "secret", vec![])).unwrap_err();
    assert_eq!(c, code::NOT_PERMITTED);
    let (c, _) = ask(&mut ma, &mut alice, call(4, "svc", "vault", "peek", vec![])).unwrap_err();
    assert_eq!(c, code::NOT_PERMITTED);
    let (c, _) = ask(&mut ma, &mut alice, call(5, "svc", "nothing", "x", vec![])).unwrap_err();
    assert_eq!(c, code::NOT_FOUND);
    // Her burst is used up: BUSY (the provider does not run).
    let (c, _) = ask(&mut ma, &mut alice, call(6, "svc", "prices", "price", vec![text("bread")])).unwrap_err();
    assert_eq!(c, code::BUSY);
    // A caller that is not one of her loaded mods is refused before anything else (and costs no token).
    let (c, _) = ask(&mut ma, &mut alice, call(7, "ghost-mod", "prices", "price", vec![])).unwrap_err();
    assert_eq!(c, code::NOT_PERMITTED);
    // A token comes back after a second.
    std::thread::sleep(Duration::from_millis(1200));
    assert_eq!(ask(&mut ma, &mut alice, call(8, "svc", "prices", "price", vec![text("bread")])), Ok(Value::Uint(5)));

    // Another player has a bucket of their own, and the provider's failures are typed.
    let (mut bob, mut mb) = join(&srv, "BOB", &cache);
    let (c, m) = ask(&mut mb, &mut bob, call(1, "svc", "prices", "boom", vec![])).unwrap_err();
    assert_eq!((c, m.as_str()), (code::PROVIDER_ERROR, "kaboom"));
    let (c, _) = ask(&mut mb, &mut bob, call(2, "svc", "prices", "huge", vec![])).unwrap_err();
    assert_eq!(c, code::TOO_LARGE);
    // A provider that never returns runs into its time budget: TRAP, and the server is fine.
    let (c, _) = ask(&mut mb, &mut bob, call(4, "svc", "prices", "spin", vec![])).unwrap_err();
    assert_eq!(c, code::TRAP);
    assert!(srv.log().contains("remote service prices.spin: hard time budget exceeded"), "{}", srv.log());
    // Bad arguments are BAD_VALUE; too many bytes cannot even be sent (the codec refuses), so send them raw.
    let mut bad = call(5, "svc", "prices", "price", vec![]);
    bad.args = vec![0xff];
    let (c, _) = ask(&mut mb, &mut bob, bad).unwrap_err();
    assert_eq!(c, code::BAD_VALUE);
    assert_eq!(ask(&mut mb, &mut bob, call(6, "svc", "prices", "price", vec![text("apple")])), Ok(Value::Uint(3)));

    // Server -> client: the server mod calls ALICE's client, which answers; then a call nobody answers; then one
    // whose player leaves.
    alice.chat("/askclient").unwrap();
    let c = ma.wait_service_call(&mut alice, 10_000).unwrap().expect("SERVICE_CALL");
    assert_eq!((c.mod_id.as_str(), c.service.as_str(), c.method.as_str()), ("svc", "hud", "title"));
    assert_eq!(cbor::decode(&c.args).unwrap(), Value::Array(vec![Value::Uint(7)]));
    ma.answer_service(&mut alice, c.id, Ok(cbor::encode(&text("Hi")))).unwrap();
    assert!(srv.wait_log("hud true Hi", 10), "{}", srv.log());
    alice.chat("/askclient").unwrap();
    let c2 = ma.wait_service_call(&mut alice, 10_000).unwrap().expect("second SERVICE_CALL");
    assert_ne!(c2.id, c.id);
    assert!(srv.wait_log("hud false timeout", 10), "{}", srv.log());
    // A late answer after the timeout changes nothing; neither does an answer to a call that was never made.
    ma.answer_service(&mut alice, c2.id, Ok(cbor::encode(&text("late")))).unwrap();
    ma.answer_service(&mut alice, 4_000_000_000, Ok(cbor::encode(&text("stray")))).unwrap();
    alice.run_for(300).unwrap();
    assert_eq!(srv.count("hud true"), 1, "{}", srv.log());
    // BOB's client cannot answer ALICE's call.
    alice.chat("/askclient").unwrap();
    let c3 = ma.wait_service_call(&mut alice, 10_000).unwrap().expect("third SERVICE_CALL");
    mb.answer_service(&mut bob, c3.id, Ok(cbor::encode(&text("bob")))).unwrap();
    let end = Instant::now() + Duration::from_secs(10);
    while srv.count("hud false timeout") < 2 && Instant::now() < end {
        std::thread::sleep(Duration::from_millis(50));
    }
    assert_eq!(srv.count("hud false timeout"), 2, "{}", srv.log());
    assert_eq!(srv.count("hud true"), 1, "{}", srv.log());
    // A client that fails the call: the server mod hears the kind.
    alice.chat("/askclient").unwrap();
    let c4 = ma.wait_service_call(&mut alice, 10_000).unwrap().expect("fourth SERVICE_CALL");
    ma.answer_service(&mut alice, c4.id, Err((code::PROVIDER_ERROR, "no hud"))).unwrap();
    assert!(srv.wait_log("hud false provider-error", 10), "{}", srv.log());
    // BOB leaves with a call open.
    bob.chat("/askclient").unwrap();
    mb.wait_service_call(&mut bob, 10_000).unwrap().expect("BOB's SERVICE_CALL");
    bob.disconnect();
    assert!(srv.wait_log("hud false disconnected", 10), "{}", srv.log());

    // A modded peer that is not in the game yet is ignored.
    let mut early = srv.session("EARLY");
    assert!(early.wait_for(0x23, 10_000).unwrap().is_some());
    let mc = ModClient::new(LoaderOptions::new(&tmp.path().join("c-early")));
    early.client.send(&mc.hello().encode()).unwrap();
    early.wait_for_matching(5000, &mut |rx| rx.data.starts_with(&[PACKET_TYPE, 0x01])).unwrap().unwrap();
    let pkt = bhmod_proto::Message::ServiceCall(call(1, "svc", "prices", "price", vec![text("apple")])).encode();
    early.client.send(&pkt).unwrap();
    assert!(early.wait_for_matching(700, &mut |rx| rx.data.starts_with(&[PACKET_TYPE, 0x14])).unwrap().is_none());
    early.disconnect();

    alice.disconnect();
    assert_eq!(srv.handle.stop(), Status::Stopped(0), "{}", srv.log());
}

#[test]
fn servers_without_service_mods_ignore_service_packets() {
    let _w = watchdog("services-vanilla", 120);
    let tmp = tempfile::tempdir().unwrap();
    let data = tmp.path().join("TheBlockheads");
    let w = host::create_world(Some(&data), "Plain", 104, false).expect("create");
    // A server-only mod without services: vanilla clients join as usual and never see a 0xBC byte; a service call
    // from a modded client is answered NOT_FOUND (nothing is provided).
    let mods = tmp.path().join("mods");
    let d = mods.join("quiet");
    std::fs::create_dir_all(d.join("server")).unwrap();
    std::fs::write(
        d.join("mod.toml"),
        "schema = 1\nid = \"quiet\"\nversion = \"1.0.0\"\nname = { en = \"Q\" }\nauthors = [\"t\"]\nlicense = \"MIT\"\napi = \"^0.1\"\n[sides]\nserver = true\nclient = false\n[entry]\nserver = { runtime = \"lua\", path = \"server/main.luau\" }\n",
    )
    .unwrap();
    std::fs::write(d.join("server/main.luau"), "return {}").unwrap();
    let mut srv = start(&data, &w.id, 15491, &mods);
    let mut v = srv.session("VANILLA");
    assert_eq!(v.join(10_000).unwrap(), JoinOutcome::Approved);
    // A vanilla client sending the new ops anyway gets nothing back.
    let pkt = bhmod_proto::Message::ServiceCall(call(1, "quiet", "prices", "price", vec![])).encode();
    v.client.send(&pkt).unwrap();
    v.run_for(500).unwrap();
    assert!(!v.received().iter().any(|rx| rx.data.first() == Some(&PACKET_TYPE)), "a vanilla client got 0xBC");
    v.disconnect();
    // A modded client with no client mods joins, calls and learns that nothing is provided.
    let mut s = srv.session("MODDED");
    let mut mc = ModClient::new(LoaderOptions::new(&tmp.path().join("cache")));
    assert_eq!(mc.join(&mut s, 10_000).unwrap(), JoinOutcome::Approved, "{}", srv.log());
    let (c, _) = ask(&mut mc, &mut s, call(1, "quiet", "prices", "price", vec![])).unwrap_err();
    assert_eq!(c, code::NOT_FOUND);
    s.disconnect();
    assert_eq!(srv.handle.stop(), Status::Stopped(0), "{}", srv.log());
}
