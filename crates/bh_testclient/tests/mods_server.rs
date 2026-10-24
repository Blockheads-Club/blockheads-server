//! The server with Luau mods loaded (the SDK examples in `tests/data/mods`), driven by the protocol test client.

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

#[test]
fn lua_mods_on_a_live_server() {
    let tmp = tempfile::tempdir().unwrap();
    let data = tmp.path().join("TheBlockheads");
    let w = host::create_world(Some(&data), "Modded", 104, false).expect("create");
    let mods = mods_dir(tmp.path());
    let cfg = ModsConfig { dir: Some(mods.clone()), disabled: false, variant_blocks: false };
    let mut srv = start(&data, &w.id, 15400, cfg.clone());
    assert!(
        srv.log_has("[mods] loaded 3 mod(s): chat-filter 0.1.0, hello-lua 0.1.0, scheduler-lua 0.1.0"),
        "{}",
        srv.log()
    );
    assert!(srv.wait_log("[mods] hello-lua: hello-lua enabled", 10), "{}", srv.log());

    // player-join + storage.
    let mut alice = srv.join("ALICE");
    wait_chat(&mut alice, "Hello, ALICE! You are greeting number 1.", 5000);
    let mut bob = srv.join("BOB");
    wait_chat(&mut bob, "Hello, BOB! You are greeting number 2.", 5000);
    chats(&mut alice, 200);

    // A mod command works for a normal player; the reply is private, the broadcast reaches everyone.
    alice.chat("/hello").unwrap();
    let a = wait_chat(&mut alice, "ALICE says hello to everyone!", 5000);
    assert!(a.contains(&"Hello, ALICE!".to_string()), "{a:?}");
    let b = wait_chat(&mut bob, "ALICE says hello to everyone!", 5000);
    assert!(!b.iter().any(|m| m == "Hello, ALICE!" || m.contains("/hello")), "{b:?}");

    // chat-filter: cancel (the other player sees nothing), rewrite, permission check.
    alice.chat("free stuff at spam.example").unwrap();
    wait_chat(&mut alice, "Your message was blocked", 5000);
    alice.chat("darn it").unwrap();
    let b = wait_chat(&mut bob, "**** it", 5000);
    assert!(!b.iter().any(|m| m.contains("spam.example")), "{b:?}");
    alice.chat("/chatfilter off").unwrap();
    wait_chat(&mut alice, "You do not have permission to use /chatfilter.", 5000);
    // The vanilla path still works for normal chat.
    bob.chat("hi alice").unwrap();
    wait_chat(&mut alice, "hi alice", 5000);

    // Scheduler: repeating task and next-poll / delayed tasks.
    alice.chat("/countdown 1").unwrap();
    wait_chat(&mut bob, "Go!", 5000);
    alice.chat("/remind 0.4 stretch").unwrap();
    let a = wait_chat(&mut alice, "Reminder: stretch", 5000);
    assert!(a.contains(&"OK, I will remind you in 0.4 s.".to_string()), "{a:?}");

    // Console: mod commands and built-ins.
    srv.handle.send_command("/hello");
    assert!(srv.wait_log("Hello, console!", 5), "{}", srv.log());
    srv.handle.send_command("/chatfilter off");
    assert!(srv.wait_log("Chat filter off.", 5), "{}", srv.log());
    srv.handle.send_command("/players");
    assert!(srv.wait_log("ALICE", 5));

    // Quit (plain disconnect) and kick.
    bob.disconnect();
    srv.handle.send_command("/kick ALICE");
    let end = Instant::now() + Duration::from_secs(5);
    while Instant::now() < end {
        let _ = chats(&mut alice, 100);
    }
    assert_eq!(srv.handle.stop(), Status::Stopped(0), "{}", srv.log());
    assert!(srv.log_has("[mods] hello-lua: hello-lua disabled"), "{}", srv.log());
    let storage = data.join("saves").join(&w.id).join("mods/storage/data.mdb");
    assert!(storage.is_file(), "{}", storage.display());

    // Storage survives a restart.
    let mut srv = start(&data, &w.id, 15401, cfg);
    assert!(srv.wait_log("hello-lua: hello-lua enabled", 10), "{}", srv.log());
    let mut carol = srv.join("CAROL");
    wait_chat(&mut carol, "You are greeting number 3.", 5000);
    carol.disconnect();
    assert_eq!(srv.handle.stop(), Status::Stopped(0));
}

/// Without mods (no directory, an empty one, or only a disabled mod) the server logs nothing new and creates no
/// mod files; `HostConfig.mods.disabled` ignores a full mods directory.
#[test]
fn no_mods_means_no_trace() {
    let tmp = tempfile::tempdir().unwrap();
    let data = tmp.path().join("TheBlockheads");
    let w = host::create_world(Some(&data), "Plain", 104, false).expect("create");
    let empty = tmp.path().join("empty");
    std::fs::create_dir_all(&empty).unwrap();
    let only_disabled = tmp.path().join("disabled");
    copy_dir(&fixtures().join("hello-lua"), &only_disabled.join("hello-lua"));
    std::fs::write(only_disabled.join("mods.toml"), "[hello-lua]\nenabled = false\n").unwrap();
    let full = mods_dir(tmp.path());
    let cases = [
        ModsConfig { dir: Some(tmp.path().join("missing")), disabled: false, variant_blocks: false },
        ModsConfig { dir: Some(empty), disabled: false, variant_blocks: false },
        ModsConfig { dir: Some(only_disabled), disabled: false, variant_blocks: false },
        ModsConfig { dir: Some(full), disabled: true, variant_blocks: false },
        // The default: `<data dir>/mods`, which does not exist here.
        ModsConfig::default(),
    ];
    for (i, cfg) in cases.into_iter().enumerate() {
        let mut srv = start(&data, &w.id, 15410 + i as u16, cfg);
        let mut p = srv.join("PAT");
        p.chat("/hello").unwrap();
        let _ = chats(&mut p, 500);
        p.disconnect();
        assert_eq!(srv.handle.stop(), Status::Stopped(0));
        assert!(!srv.log_has("[mods"), "case {i}: {}", srv.log());
    }
    assert!(!data.join("saves").join(&w.id).join("mods").exists());
}

const ADMIN_TOOLS: &str = r#"
local mod = {}
function mod.on_enable()
    bh.commands.register({ name = "whois" }, function(sender, label, args)
        local p = bh.players.find_by_alias(args[1] or "")
        if not p then return "nobody" end
        return p:alias() .. " is " .. p:role()
    end)
    bh.commands.register({ name = "boot", min_role = "moderator" }, function(sender, label, args)
        local p = bh.players.find_by_alias(args[1] or "")
        p:kick("bye")
        return "booted"
    end)
    bh.commands.register({ name = "ghost" }, function(sender)
        assert(bh.players.get("no-such-id") == nil)
        local ok = pcall(function() sender.player:send_chat("x") end)
        return "ok"
    end)
    bh.commands.register({ name = "later" }, function(sender)
        local who = sender.player
        bh.scheduler.run_later(10, function()
            local ok, err = pcall(function() who:send_chat("late") end)
            local ok2, err2 = pcall(function() who:kick("late") end)
            local function short(e) return string.match(tostring(e), "stale handle") or tostring(e) end
            bh.log.info("late: " .. short(err) .. " / " .. short(err2))
        end)
        return "scheduled"
    end)
end
return mod
"#;

/// Roles, kicks and handles of players who left, through the real bridge.
#[test]
fn roles_kicks_and_stale_players() {
    let tmp = tempfile::tempdir().unwrap();
    let data = tmp.path().join("TheBlockheads");
    let w = host::create_world(Some(&data), "Roles", 104, false).expect("create");
    let mods = tmp.path().join("mods");
    let d = mods.join("admin-tools");
    std::fs::create_dir_all(d.join("server")).unwrap();
    std::fs::write(
        d.join("mod.toml"),
        "schema = 1\nid = \"admin-tools\"\nversion = \"1.0.0\"\nname = { en = \"A\" }\nauthors = [\"t\"]\nlicense = \"MIT\"\napi = \"^0.1\"\n[sides]\nserver = true\nclient = false\n[entry]\nserver = { runtime = \"lua\", path = \"server/main.luau\" }\n[permissions]\nplayers = [\"kick\"]\n",
    )
    .unwrap();
    std::fs::write(d.join("server/main.luau"), ADMIN_TOOLS).unwrap();
    std::fs::write(mods.join("mods.toml"), "[admin-tools]\ngrant = [\"players:kick\"]\n").unwrap();
    let cfg = ModsConfig { dir: Some(mods), disabled: false, variant_blocks: false };
    let mut srv = start_with(&data, &w.id, 15420, cfg, Some("OWNY"));
    assert!(srv.wait_log("[mods] admin-tools: enabled", 10), "{}", srv.log());
    srv.handle.send_command("/admin ADDY");
    srv.handle.send_command("/mod MODDY");
    std::thread::sleep(Duration::from_millis(500));
    let mut owner = srv.join("OWNY");
    let mut admin = srv.join("ADDY");
    let mut moder = srv.join("MODDY");
    let mut plain = srv.join("PLAIN");
    for (who, role) in [("OWNY", "owner"), ("ADDY", "admin"), ("MODDY", "moderator"), ("PLAIN", "player")] {
        plain.chat(&format!("/whois {who}")).unwrap();
        wait_chat(&mut plain, &format!("{who} is {role}"), 5000);
    }
    // A staff member's built-in command still reaches the vanilla handler.
    admin.chat("/help").unwrap();
    wait_chat(&mut admin, "/PLAYERS", 5000);
    plain.chat("/whois NOBODY").unwrap();
    wait_chat(&mut plain, "nobody", 5000);
    plain.chat("/ghost").unwrap();
    wait_chat(&mut plain, "ok", 5000);
    plain.chat("/later").unwrap();
    wait_chat(&mut plain, "scheduled", 5000);
    plain.disconnect();
    assert!(srv.wait_log("late: stale handle / stale handle", 10), "{}", srv.log());
    owner.chat("/boot MODDY").unwrap();
    wait_chat(&mut owner, "booted", 5000);
    let end = Instant::now() + Duration::from_secs(4);
    while Instant::now() < end {
        let _ = chats(&mut moder, 100);
    }
    assert_eq!(srv.handle.stop(), Status::Stopped(0), "{}", srv.log());
}

/// A WASM component mod (the SDK's `hello-rust`, the twin of `hello-lua`) on a live server: compiled once, cached in
/// `<mods>/.cache`, and behaving like the Luau version.
#[test]
fn wasm_mod_on_a_live_server() {
    let tmp = tempfile::tempdir().unwrap();
    let data = tmp.path().join("TheBlockheads");
    let w = host::create_world(Some(&data), "Wasm", 104, false).expect("create");
    let mods = tmp.path().join("mods");
    copy_dir(&fixtures().join("hello-rust"), &mods.join("hello-rust"));
    std::fs::write(mods.join("mods.toml"), "[hello-rust]\ngrant = [\"players:chat\"]\n").unwrap();
    let cfg = ModsConfig { dir: Some(mods.clone()), disabled: false, variant_blocks: false };
    let mut srv = start(&data, &w.id, 15430, cfg.clone());
    assert!(srv.log_has("[mods] loaded 1 mod(s): hello-rust 0.1.0"), "{}", srv.log());
    assert!(srv.wait_log("[mods] hello-rust: hello-rust enabled", 10), "{}", srv.log());
    let cached: Vec<_> = std::fs::read_dir(mods.join(".cache")).unwrap().flatten().collect();
    assert_eq!(cached.len(), 1, "{cached:?}");

    let mut alice = srv.join("ALICE");
    wait_chat(&mut alice, "Hello, ALICE! You are greeting number 1.", 5000);
    alice.chat("/hi").unwrap();
    let a = wait_chat(&mut alice, "ALICE says hello to everyone!", 5000);
    assert!(a.contains(&"Hello, ALICE!".to_string()), "{a:?}");
    srv.handle.send_command("/hello");
    assert!(srv.wait_log("Hello, console!", 5), "{}", srv.log());
    alice.disconnect();
    assert_eq!(srv.handle.stop(), Status::Stopped(0), "{}", srv.log());
    assert!(srv.log_has("[mods] hello-rust: hello-rust disabled"), "{}", srv.log());

    // The second start loads the cached code; storage survives.
    let mut srv = start(&data, &w.id, 15431, cfg);
    assert!(srv.wait_log("hello-rust enabled", 10), "{}", srv.log());
    let mut bob = srv.join("BOB");
    wait_chat(&mut bob, "You are greeting number 2.", 5000);
    bob.disconnect();
    assert_eq!(srv.handle.stop(), Status::Stopped(0));
}
