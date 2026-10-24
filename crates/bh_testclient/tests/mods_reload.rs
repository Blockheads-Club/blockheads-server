//! Hot reload and time accounting on a live server: `/mods reload`, `/mods timings`, the developer watcher
//! (`BLOCKHEADS_MODS_WATCH`), driven by the protocol test client. The owner is an admin, everybody else is refused.

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use bh_testclient::{ClientConfig, Identity, JoinOutcome, Session};
use blockheads_server::host::{self, HostConfig, ServerHandle, Status, WorldSelect};
use blockheads_server::mods::ModsConfig;
use blockheads_server::plist::value::PlistValue;
use blockheads_server::protocol::codec::{plist_from, t};

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for e in std::fs::read_dir(from).unwrap().flatten() {
        let p = e.path();
        if p.is_dir() {
            copy_dir(&p, &to.join(e.file_name()));
        } else {
            std::fs::copy(&p, to.join(e.file_name())).unwrap();
        }
    }
}

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/mods")
}

struct Running {
    handle: ServerHandle,
    lines: Arc<Mutex<Vec<String>>>,
    port: u16,
}

impl Running {
    fn log_has(&self, needle: &str) -> bool {
        self.lines.lock().unwrap().iter().any(|l| l.contains(needle))
    }
    fn wait_log(&self, needle: &str, secs: u64) -> bool {
        let end = Instant::now() + Duration::from_secs(secs);
        while Instant::now() < end {
            if self.log_has(needle) {
                return true;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        false
    }
    fn log_count(&self, needle: &str) -> usize {
        self.lines.lock().unwrap().iter().filter(|l| l.contains(needle)).count()
    }
    /// Waits until the log has at least `n` lines containing `needle`.
    fn wait_count(&self, needle: &str, n: usize, secs: u64) -> bool {
        let end = Instant::now() + Duration::from_secs(secs);
        while Instant::now() < end {
            if self.log_count(needle) >= n {
                return true;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        false
    }
    fn log(&self) -> String {
        self.lines.lock().unwrap().join("\n")
    }
    fn join(&self, alias: &str) -> Session {
        let cfg = ClientConfig { addr: SocketAddr::from(([127, 0, 0, 1], self.port)), ..Default::default() };
        let mut s = Session::connect(cfg, Identity::new(alias)).unwrap();
        assert_eq!(s.join(10_000).unwrap(), JoinOutcome::Approved, "{}", self.log());
        s
    }
}

fn start(data: &Path, world: &str, port: u16, mods: ModsConfig) -> Running {
    start_with(data, world, port, mods, None)
}

fn start_with(data: &Path, world: &str, port: u16, mods: ModsConfig, owner: Option<&str>) -> Running {
    let lines = Arc::new(Mutex::new(Vec::<String>::new()));
    let l2 = lines.clone();
    let cfg = HostConfig {
        data_dir: Some(data.to_path_buf()),
        world: WorldSelect::Load(world.to_string()),
        port: Some(port),
        mods,
        owner_name: owner.map(str::to_string),
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

fn text_of(data: &[u8]) -> Option<String> {
    if data.first() != Some(&t::CHAT_MESSAGE) {
        return None;
    }
    let p = plist_from(&data[1..])?;
    match p.object_for_key("message") {
        Some(PlistValue::String(s)) => Some(s.s.clone()),
        _ => None,
    }
}

/// Chat lines received within `ms`.
fn chats(s: &mut Session, ms: u64) -> Vec<String> {
    s.run_for(ms).unwrap();
    s.drain().iter().filter_map(|rx| text_of(&rx.data)).collect()
}

/// Waits until a chat line containing `needle` arrives; returns everything seen until then.
fn wait_chat(s: &mut Session, needle: &str, ms: u64) -> Vec<String> {
    let mut seen = Vec::new();
    let end = Instant::now() + Duration::from_millis(ms);
    while Instant::now() < end {
        seen.extend(chats(s, 50));
        if seen.iter().any(|m| m.contains(needle)) {
            return seen;
        }
    }
    panic!("no chat line containing {needle:?}; saw {seen:?}");
}

fn mods_dir(root: &Path) -> PathBuf {
    let mods = root.join("mods");
    for m in ["hello-lua", "chat-filter", "scheduler-lua"] {
        copy_dir(&fixtures().join(m), &mods.join(m));
    }
    std::fs::write(
        mods.join("mods.toml"),
        "[hello-lua]\ngrant = [\"players:chat\"]\n[scheduler-lua]\ngrant = [\"players:chat\"]\n",
    )
    .unwrap();
    mods
}

fn hello_main(mods: &Path) -> PathBuf {
    mods.join("hello-lua/server/main.luau")
}

#[test]
fn admins_reload_and_inspect_mods_on_a_live_server() {
    let tmp = tempfile::tempdir().unwrap();
    let data = tmp.path().join("TheBlockheads");
    let w = host::create_world(Some(&data), "Reload", 104, false).expect("create");
    let mods = mods_dir(tmp.path());
    let cfg = ModsConfig { dir: Some(mods.clone()), disabled: false, variant_blocks: false };
    let mut srv = start_with(&data, &w.id, 15230, cfg, Some("BOSS"));
    assert!(srv.wait_log("hello-lua: hello-lua enabled", 10), "{}", srv.log());
    let mut boss = srv.join("BOSS");
    let mut pal = srv.join("PAL");
    chats(&mut boss, 300);
    chats(&mut pal, 300);

    // Everybody can still use the mods' commands; /mods is for admins.
    pal.chat("/mods reload").unwrap();
    wait_chat(&mut pal, "You do not have permission to use /mods.", 5000);
    pal.chat("/mods timings").unwrap();
    wait_chat(&mut pal, "You do not have permission to use /mods.", 5000);
    boss.chat("/mods").unwrap();
    let list = wait_chat(&mut boss, "hello-lua 0.1.0 [enabled]", 5000);
    assert!(list.iter().any(|l| l.contains("3 mod(s):")), "{list:?}");

    // A change on disk, reloaded for one mod: the command now answers differently, storage (the greeting count) stays.
    boss.chat("/hello").unwrap();
    wait_chat(&mut boss, "Hello, BOSS!", 5000);
    chats(&mut pal, 300);
    let main = hello_main(&mods);
    let text = std::fs::read_to_string(&main).unwrap().replace("Hello, {who}!", "Howdy, {who}!");
    std::fs::write(&main, text).unwrap();
    boss.chat("/mods reload hello-lua").unwrap();
    wait_chat(&mut boss, "hello-lua: reloaded (0.1.0)", 5000);
    assert!(srv.wait_log("hello-lua: disabled: reloading", 5), "{}", srv.log());
    boss.chat("/hello").unwrap();
    wait_chat(&mut boss, "Howdy, BOSS!", 5000);
    // The old command registration is gone, not doubled: one broadcast per use.
    let said = chats(&mut pal, 500);
    assert_eq!(said.iter().filter(|m| m.contains("BOSS says hello to everyone!")).count(), 1, "{said:?}");
    // Players joining after the reload are greeted by the new instance with the old storage.
    let mut cat = srv.join("CAT");
    wait_chat(&mut cat, "You are greeting number 3.", 5000);

    // A reload of everything finds nothing changed now; a new mod on disk is started, a vanished one stopped.
    boss.chat("/mods reload").unwrap();
    wait_chat(&mut boss, "No mod changed on disk (3 checked).", 5000);
    copy_dir(&fixtures().join("hello-lua"), &mods.join("hello-two"));
    let toml = std::fs::read_to_string(mods.join("hello-two/mod.toml")).unwrap().replace("hello-lua", "hello-two");
    std::fs::write(mods.join("hello-two/mod.toml"), toml).unwrap();
    std::fs::remove_dir_all(mods.join("chat-filter")).unwrap();
    boss.chat("/mods reload").unwrap();
    let r = wait_chat(&mut boss, "chat-filter: stopped", 5000);
    assert!(r.iter().any(|l| l.contains("hello-two: permissions not granted in mods.toml: players:chat")), "{r:?}");
    assert!(srv.log_has("[mods] reload:"), "{}", srv.log());

    // Time accounting.
    boss.chat("/mods timings").unwrap();
    let t = wait_chat(&mut boss, "hello-lua [enabled]", 5000);
    assert!(t.iter().any(|l| l.starts_with("Mod timings over the last 60 s")), "{t:?}");
    boss.chat("/mods timings scheduler-lua").unwrap();
    wait_chat(&mut boss, "scheduler-lua [enabled]", 5000);
    boss.chat("/mods timings ghost").unwrap();
    wait_chat(&mut boss, "No mod ghost is loaded.", 5000);
    boss.chat("/mods timings reset").unwrap();
    wait_chat(&mut boss, "Timings cleared.", 5000);
    boss.chat("/mods bogus").unwrap();
    wait_chat(&mut boss, "Usage: /mods", 5000);
    // The console may use it too.
    srv.handle.send_command("/mods");
    assert!(srv.wait_log("hello-lua 0.1.0 [enabled]", 5), "{}", srv.log());
    assert_eq!(srv.handle.stop(), Status::Stopped(0), "{}", srv.log());
}

#[test]
fn the_watcher_reloads_changed_mods_by_itself() {
    let tmp = tempfile::tempdir().unwrap();
    let data = tmp.path().join("TheBlockheads");
    let w = host::create_world(Some(&data), "Watch", 104, false).expect("create");
    let mods = mods_dir(tmp.path());
    let cfg = ModsConfig { dir: Some(mods.clone()), disabled: false, variant_blocks: false };
    std::env::set_var("BLOCKHEADS_MODS_WATCH", "1");
    let mut srv = start(&data, &w.id, 15231, cfg);
    std::env::remove_var("BLOCKHEADS_MODS_WATCH");
    assert!(srv.wait_log("watching the mods directory", 10), "{}", srv.log());
    let mut alice = srv.join("ALICE");
    alice.chat("/hello").unwrap();
    wait_chat(&mut alice, "Hello, ALICE!", 5000);
    let main = hello_main(&mods);
    let text = std::fs::read_to_string(&main).unwrap().replace("Hello, {who}!", "Hej, {who}!");
    std::fs::write(&main, text).unwrap();
    assert!(srv.wait_log("watch: hello-lua: reloaded (0.1.0)", 15), "{}", srv.log());
    alice.chat("/hello").unwrap();
    wait_chat(&mut alice, "Hej, ALICE!", 5000);
    // A broken edit is reported and the mod stays down until it is fixed (then the watcher brings it back).
    std::fs::write(&main, "this is not luau").unwrap();
    assert!(srv.wait_log("watch: hello-lua: not reloaded", 15), "{}", srv.log());
    let fixed =
        "return { on_enable = function() bh.commands.register({ name = 'hello' }, function() return 'Back!' end) end }";
    std::fs::write(&main, fixed).unwrap();
    assert!(srv.wait_count("watch: hello-lua: reloaded", 2, 15), "{}", srv.log());
    alice.chat("/hello").unwrap();
    wait_chat(&mut alice, "Back!", 5000);
    alice.disconnect();
    assert_eq!(srv.handle.stop(), Status::Stopped(0), "{}", srv.log());
}
