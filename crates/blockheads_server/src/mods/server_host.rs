//! The server's mod host: `bhmod-core` behind the [`ModHooks`] seam, a [`GameBridge`] over `BHServer`, and mod
//! storage in a separate LMDB environment (`<world>/mods/storage`), never in the vanilla world database.

use std::cell::RefCell;
use std::collections::BTreeSet;
use std::io;
use std::path::{Path, PathBuf};
use std::rc::{Rc, Weak};

use bhmod_core::storage::{KvBackend, StorageProvider};
use bhmod_core::types::{CommandSender, Event, LogLevel, Role, SavePhase, Side};
use bhmod_core::{GameBridge, HostOptions, ModHost};
use bhmod_wasm::{WasmConfig, WasmRuntime};

use super::{ChatVerdict, ModHooks};
use crate::server::bhserver::BHServer;
use crate::storage::lmdb::{Database, DatabaseEnvironment};

/// Built-in command names (`handleCommand:issueClient:`); mods may not register them.
pub const BUILTIN_COMMANDS: &[&str] = &[
    "admin",
    "ban",
    "ban-no-device",
    "clear",
    "clear-adminlist",
    "clear-blacklist",
    "clear-modlist",
    "clear-whitelist",
    "debug-log",
    "help",
    "kick",
    "list-adminlist",
    "list-blacklist",
    "list-modlist",
    "list-whitelist",
    "load-lists",
    "mod",
    "players",
    "pvp-off",
    "pvp-on",
    "remove-owner",
    "remove-password",
    "repair",
    "reset-owner",
    "set-credit",
    "set-owner",
    "set-password",
    "set-privacy",
    "set-salt",
    "stop",
    "unadmin",
    "unban",
    "unmod",
    "unwhitelist",
    "whitelist",
];

/// The game as seen by mods.
struct Bridge {
    srv: Weak<BHServer>,
}

impl Bridge {
    fn online(&self, s: &BHServer, id: &str) -> bool {
        s.connected_client_ids().iter().any(|c| c == id)
    }
}

impl GameBridge for Bridge {
    fn side(&self) -> Side {
        Side::Server
    }
    fn host_name(&self) -> String {
        format!("blockheads-server {}", env!("CARGO_PKG_VERSION"))
    }
    fn log(&self, mod_id: Option<&str>, level: LogLevel, message: &str) {
        if level < LogLevel::Info {
            return;
        }
        let tag = if level >= LogLevel::Warn { format!(" {}", level.name().to_uppercase()) } else { String::new() };
        match mod_id {
            Some(m) => crate::nslog!("[mods{}] {}: {}", tag, m, message),
            None => crate::nslog!("[mods{}] {}", tag, message),
        }
    }
    fn online_players(&self) -> Vec<String> {
        self.srv.upgrade().map(|s| s.connected_client_ids()).unwrap_or_default()
    }
    fn player_alias(&self, id: &str) -> Option<String> {
        let s = self.srv.upgrade()?;
        if !self.online(&s, id) {
            return None;
        }
        s.player_alias_for_peer_id(Some(id))
    }
    fn player_role(&self, id: &str) -> Role {
        let Some(s) = self.srv.upgrade() else { return Role::Player };
        let alias = s.player_alias_for_peer_id(Some(id));
        let a = alias.as_deref();
        if s.player_is_owner_with_alias(a) {
            Role::Owner
        } else if s.player_is_admin_with_alias(a) {
            Role::Admin
        } else if s.player_is_mod_with_alias(a) {
            Role::Moderator
        } else {
            Role::Player
        }
    }
    fn send_chat(&self, id: &str, text: &str) -> bool {
        let Some(s) = self.srv.upgrade() else { return false };
        if !self.online(&s, id) {
            return false;
        }
        s.send_chat_message_send_to_clients(text, Some(&[id.to_string()]));
        true
    }
    fn broadcast(&self, text: &str) {
        if let Some(s) = self.srv.upgrade() {
            s.send_chat_message_send_to_clients(text, None);
        }
    }
    fn kick(&self, id: &str, _reason: &str) -> bool {
        let Some(s) = self.srv.upgrade() else { return false };
        if !self.online(&s, id) {
            return false;
        }
        s.boot_player_was_ban(id, false);
        true
    }
    fn is_builtin_command(&self, name: &str) -> bool {
        BUILTIN_COMMANDS.contains(&name)
    }
}

/// Mod storage: one LMDB environment per world, one named database per mod, created on first use.
pub struct LmdbStorage {
    path: PathBuf,
    env: RefCell<Option<Rc<DatabaseEnvironment>>>,
}

impl LmdbStorage {
    /// Storage in `path` (a directory).
    pub fn new(path: PathBuf) -> LmdbStorage {
        LmdbStorage { path, env: RefCell::new(None) }
    }
}

struct LmdbKv {
    db: Database,
}

fn failed(what: &str) -> io::Error {
    io::Error::other(format!("mod storage: {what} failed"))
}

impl KvBackend for LmdbKv {
    fn load(&mut self) -> io::Result<Vec<(String, Vec<u8>)>> {
        // Keys are written by `put` from UTF-8 strings only.
        Ok(self.db.entries().into_iter().map(|(k, v)| (String::from_utf8_lossy(&k).into_owned(), v)).collect())
    }
    fn put(&mut self, key: &str, value: &[u8]) -> io::Result<()> {
        self.db.put(key, value).then_some(()).ok_or_else(|| failed("write"))
    }
    fn delete(&mut self, key: &str) -> io::Result<()> {
        self.db.remove(key);
        Ok(())
    }
    fn flush(&mut self) -> io::Result<()> {
        // Every write is its own committed transaction.
        Ok(())
    }
}

impl StorageProvider for LmdbStorage {
    fn open(&self, mod_id: &str) -> io::Result<Box<dyn KvBackend>> {
        let mut env = self.env.borrow_mut();
        if env.is_none() {
            *env = DatabaseEnvironment::new(&self.path.to_string_lossy(), 256, 1024);
        }
        let db = env.as_ref().and_then(|e| Database::new(e, mod_id)).ok_or_else(|| failed("open"))?;
        Ok(Box::new(LmdbKv { db }))
    }
}

/// The installed host.
struct ServerMods {
    host: RefCell<ModHost>,
    srv: Weak<BHServer>,
    joined: RefCell<BTreeSet<String>>,
}

impl ServerMods {
    /// Runs `f` on the host unless a mod call is already running (a host function that re-enters the game must
    /// not re-enter the mods).
    fn with<R>(&self, default: R, f: impl FnOnce(&mut ModHost) -> R) -> R {
        match self.host.try_borrow_mut() {
            Ok(mut h) => f(&mut h),
            Err(_) => default,
        }
    }
}

impl ModHooks for ServerMods {
    fn player_join(&self, player_id: &str) {
        self.joined.borrow_mut().insert(player_id.to_string());
        self.with((), |h| {
            h.fire(Event::PlayerJoin { player: player_id.to_string() });
        });
    }
    fn player_quit(&self, player_id: &str, was_kick: bool) {
        if !self.joined.borrow_mut().remove(player_id) {
            return;
        }
        self.with((), |h| {
            h.fire(Event::PlayerQuit { player: player_id.to_string(), was_kick });
        });
    }
    fn chat(&self, player_id: &str, text: &str) -> ChatVerdict {
        self.with(ChatVerdict::Keep, |h| {
            match h.fire(Event::Chat { player: player_id.to_string(), text: text.to_string(), cancelled: false }) {
                Event::Chat { cancelled: true, .. } => ChatVerdict::Cancel,
                Event::Chat { text: t, .. } if t != text => ChatVerdict::Replace(t),
                _ => ChatVerdict::Keep,
            }
        })
    }
    fn owns_command(&self, line: &str) -> bool {
        self.with(false, |h| h.owns_command(line))
    }
    fn command(&self, issuer: Option<&str>, line: &str) -> bool {
        let sender = issuer.map_or(CommandSender::Console, |p| CommandSender::Player(p.to_string()));
        let Some(replies) = self.with(None, |h| h.command(sender, line)) else { return false };
        let Some(s) = self.srv.upgrade() else { return true };
        for r in replies {
            s.reply_to(&r, issuer);
        }
        true
    }
    fn tick(&self, dt: f32) {
        self.with((), |h| h.tick(dt));
    }
    fn poll(&self) {
        self.with((), |h| h.poll());
    }
    fn world_loaded(&self, world_name: &str, world_id: &str) {
        self.with((), |h| h.enable(world_name, world_id));
    }
    fn world_save(&self, post: bool) {
        self.with((), |h| h.world_save(if post { SavePhase::Post } else { SavePhase::Pre }));
    }
    fn server_stop(&self) {
        self.with((), |h| h.disable_all());
    }
}

/// Loads the mods in `dir` for world `save_id`. `None` when no mod loaded.
pub(super) fn load(dir: &Path, save_id: Option<&str>, server: &Rc<BHServer>) -> Option<Rc<dyn ModHooks>> {
    let mut opts = HostOptions::new(dir, Side::Server);
    // WASM components; compiled code is cached in `<mods>/.cache` (hidden, so discovery skips it).
    let wasm = WasmConfig { cache_dir: Some(dir.join(".cache")), ..WasmConfig::default() };
    if let Ok(rt) = WasmRuntime::new(wasm) {
        opts.runtimes.push(Box::new(rt));
    }
    // A world always has a save id on the dedicated server; `(null)` mirrors ObjC's nil formatting otherwise.
    let world_dir = PathBuf::from(crate::foundation::paths::save_dir(save_id.unwrap_or("(null)")));
    opts.storage = Box::new(LmdbStorage::new(world_dir.join("mods/storage")));
    let bridge = Rc::new(Bridge { srv: Rc::downgrade(server) });
    let host = ModHost::load(opts, bridge);
    if host.is_empty() {
        return None;
    }
    Some(Rc::new(ServerMods {
        host: RefCell::new(host),
        srv: Rc::downgrade(server),
        joined: RefCell::new(BTreeSet::new()),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bridge_without_a_server() {
        let b = Bridge { srv: Weak::new() };
        assert_eq!(b.side(), Side::Server);
        assert!(b.host_name().starts_with("blockheads-server "));
        assert!(b.online_players().is_empty());
        assert_eq!(b.player_alias("p"), None);
        assert_eq!(b.player_role("p"), Role::Player);
        assert!(!b.send_chat("p", "x"));
        b.broadcast("x");
        assert!(!b.kick("p", "x"));
        assert!(b.is_builtin_command("help") && !b.is_builtin_command("hello"));
        GameBridge::log(&b, None, LogLevel::Debug, "hidden");
        GameBridge::log(&b, None, LogLevel::Info, "host line");
        GameBridge::log(&b, Some("m"), LogLevel::Warn, "mod warning");
    }

    #[test]
    fn busy_host_returns_defaults() {
        let tmp = tempfile::tempdir().unwrap();
        let bridge = Rc::new(Bridge { srv: Weak::new() });
        let host = ModHost::load(HostOptions::new(tmp.path(), Side::Server), bridge);
        let sm = ServerMods { host: RefCell::new(host), srv: Weak::new(), joined: RefCell::new(BTreeSet::new()) };
        let _busy = sm.host.borrow_mut();
        assert_eq!(sm.chat("p", "x"), ChatVerdict::Keep);
        assert!(!sm.owns_command("/x"));
        assert!(!sm.command(None, "/x"));
        sm.tick(0.2);
        drop(_busy);
        sm.tick(0.2);
    }

    #[test]
    fn lmdb_storage_round_trip_and_failure() {
        let tmp = tempfile::tempdir().unwrap();
        let st = LmdbStorage::new(tmp.path().join("storage"));
        let mut kv = st.open("m").unwrap();
        assert!(kv.load().unwrap().is_empty());
        kv.put("k", b"v").unwrap();
        kv.put("gone", b"x").unwrap();
        kv.delete("gone").unwrap();
        kv.flush().unwrap();
        let mut again = st.open("m").unwrap();
        assert_eq!(again.load().unwrap(), vec![("k".to_string(), b"v".to_vec())]);
        // A key LMDB refuses (longer than its 511 byte key limit).
        assert!(kv.put(&"k".repeat(600), b"v").is_err());
        // An environment that cannot be created.
        std::fs::write(tmp.path().join("file"), b"").unwrap();
        let bad = LmdbStorage::new(tmp.path().join("file/storage"));
        assert!(bad.open("m").is_err());
    }
}
