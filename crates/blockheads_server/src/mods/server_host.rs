//! The server's mod host: `bhmod-core` behind the [`ModHooks`] seam, a [`GameBridge`] over `BHServer`, and mod
//! storage in a separate LMDB environment (`<world>/mods/storage`), never in the vanilla world database.

use std::cell::RefCell;
use std::collections::BTreeSet;
use std::io;
use std::path::{Path, PathBuf};
use std::rc::{Rc, Weak};
use std::time::Instant;

use bhmod_core::storage::{KvBackend, StorageProvider};
use bhmod_core::types::{
    ApiError, BreakLayer, ChannelSource, ChannelTarget, CommandSender, Event, EventKind, LogLevel, ObjectKind, Pos,
    RejectReason, Role, SavePhase, Side,
};
use bhmod_core::{GameBridge, HostOptions, ModHost};
use bhmod_wasm::{WasmConfig, WasmRuntime};

use super::net::{pre_login_text, Link, ModNet};
use super::{ChatVerdict, LoaderKey, ModHooks, Optional, LIMBO_GRACE};
use crate::server::bhserver::BHServer;
use crate::storage::lmdb::{Database, DatabaseEnvironment};
use crate::types::structs::IntPair;

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

/// The server as the 0xBC protocol sees it.
struct SrvLink<'a>(&'a BHServer);

impl Link for SrvLink<'_> {
    fn send(&self, peer: &str, packet: Vec<u8>) {
        self.0.send_network_data_to_peers_reliable(packet, &[peer.to_string()], true);
    }
    fn reject(&self, peer: &str, reason: u8) {
        self.send(peer, crate::protocol::messages::rejection(reason));
        self.0.unapproved_clients.borrow_mut().retain(|e| e != peer);
    }
    fn approved(&self, peer: &str) -> bool {
        self.0.connected_clients.borrow().iter().any(|e| e == peer)
    }
    fn alive(&self, peer: &str) -> bool {
        self.approved(peer) || self.0.unapproved_clients.borrow().iter().any(|e| e == peer)
    }
}

/// The game as seen by mods.
struct Bridge {
    srv: Weak<BHServer>,
    net: Rc<RefCell<ModNet>>,
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
    fn player_pos(&self, id: &str) -> Option<Pos> {
        // The player's (first) blockhead in the dynamic world.
        let h = crate::world::lifecycle::core_hooks();
        let all = h.dynamic_world_all_blockheads_including_net();
        all.iter().find(|b| h.blockhead_client_id(b).as_deref() == Some(id)).map(|b| pos(b.pos()))
    }
    fn channel_send(&self, _mod_id: &str, name: &str, target: &ChannelTarget, data: &[u8]) -> Result<(), ApiError> {
        let one = match target {
            ChannelTarget::Player(p) => Some(p.clone()),
            ChannelTarget::AllPlayers => None,
            _ => return Err(ApiError::InvalidArgument("a server mod sends to players".into())),
        };
        let Some(s) = self.srv.upgrade() else { return Ok(()) };
        let link = SrvLink(&s);
        let mut net = self.net.borrow_mut();
        // Players without the mod loader (or not in the game yet) are skipped.
        let peers = one.map_or_else(|| net.approved_peers(&link), |p| vec![p]);
        net.send_channel(&link, &peers, name, data).map(|_| ())
    }
}

fn pos(p: IntPair) -> Pos {
    Pos { x: p.x, y: p.y }
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
    net: Rc<RefCell<ModNet>>,
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

impl ServerMods {
    /// Ends a vanilla peer's limbo: the peer is removed like any dropped one, without a `0x26` (any reason would
    /// replace the readable welcome text with a misleading message on the client).
    fn drop_limbo(&self, player: &str) {
        let Some(s) = self.srv.upgrade() else { return };
        if let Some(m) = s.match_() {
            m.disconnect_player(player);
            m.remove_client(player);
        }
        s.client_finished_away_simulation(player);
    }

    /// Drops the limbo peers whose grace ended at `now`.
    fn drop_expired_limbo(&self, now: Instant) {
        let expired = self.net.borrow_mut().expired_limbo(now);
        for player in expired {
            self.drop_limbo(&player);
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
        self.drop_expired_limbo(Instant::now());
        if !self.net.borrow().has_peers() {
            return;
        }
        let Some(s) = self.srv.upgrade() else { return };
        let link = SrvLink(&s);
        let names = self.with(Vec::new(), |h| h.channel_names());
        let mut net = self.net.borrow_mut();
        net.prune(&link);
        net.announce(&link, &names);
    }
    fn mod_packet(&self, peer: &str, data: &[u8]) {
        let Some(s) = self.srv.upgrade() else { return };
        let names = self.with(Vec::new(), |h| h.channel_names());
        let inbound = self.net.borrow_mut().packet(&SrvLink(&s), peer, data, &names);
        if let Some((name, payload)) = inbound {
            // A full queue drops the message (the limits of bhmod-core channels apply).
            self.with(false, |h| h.channel_receive(&name, ChannelSource::Player(peer.to_string()), payload));
        }
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
    fn wants(&self, what: Optional) -> bool {
        let kind = match what {
            Optional::BlockChanged => EventKind::BlockChanged,
            Optional::PlayerMove => EventKind::PlayerMove,
        };
        self.with(false, |h| h.wants(kind))
    }
    fn pre_login(&self, peer: &str, player_id: &str, alias: &str, loader: Option<&LoaderKey>) -> Option<u8> {
        let s = self.srv.upgrade()?;
        let link = SrvLink(&s);
        self.net.borrow_mut().rekey(peer, player_id);
        if let Some(r) = self.net.borrow().check_loader(&link, player_id, loader) {
            return Some(r);
        }
        let ev = Event::PlayerPreLogin {
            player: player_id.to_string(),
            alias: alias.to_string(),
            cancelled: false,
            reason: RejectReason::Kicked,
        };
        let r = self.with(None, |h| match h.fire(ev) {
            Event::PlayerPreLogin { cancelled: true, reason, .. } => Some(reason.code()),
            _ => None,
        })?;
        self.net.borrow().reject_text(&link, player_id, r, pre_login_text(r));
        Some(r)
    }
    fn limbo_join(&self, peer: &str, player_id: &str, loader: Option<&LoaderKey>) -> bool {
        let mut net = self.net.borrow_mut();
        net.rekey(peer, player_id);
        let limbo = net.wants_limbo(player_id, loader);
        if limbo {
            net.enter_limbo(peer, player_id, Instant::now(), LIMBO_GRACE);
        }
        limbo
    }
    fn limbo_packet(&self, peer: &str) -> bool {
        let left = self.net.borrow_mut().leave_limbo(peer);
        left.map(|player| self.drop_limbo(&player)).is_some()
    }
    fn limbo_gone(&self, peer: &str) {
        self.net.borrow_mut().leave_limbo(peer);
    }
    fn block_place(&self, player_id: &str, at: IntPair, item: u32, data_a: u16, data_b: u16) -> bool {
        let ev =
            Event::BlockPlace { player: player_id.to_string(), pos: pos(at), item, data_a, data_b, cancelled: false };
        self.with(false, |h| h.fire(ev).cancelled())
    }
    fn block_break(&self, player_id: &str, at: IntPair, back_wall: bool, tile: u32) -> bool {
        let layer = if back_wall { BreakLayer::BackWall } else { BreakLayer::Block };
        let ev = Event::BlockBreak { player: player_id.to_string(), pos: pos(at), layer, tile, cancelled: false };
        self.with(false, |h| h.fire(ev).cancelled())
    }
    fn block_changed(&self, at: IntPair, tile: u32, player_id: Option<&str>) {
        let ev = Event::BlockChanged { pos: pos(at), tile, player: player_id.map(str::to_string) };
        self.with((), |h| {
            h.fire(ev);
        });
    }
    fn object_place(&self, player_id: &str, at: IntPair, workbench: bool, item: u32) -> bool {
        let object = if workbench { ObjectKind::Workbench } else { ObjectKind::InteractionObject };
        let ev = Event::ObjectPlace { player: player_id.to_string(), pos: pos(at), object, item, cancelled: false };
        self.with(false, |h| h.fire(ev).cancelled())
    }
    fn damage(&self, player_id: Option<&str>, blockhead: u64, amount: f32, attacker: Option<&str>) -> Option<f32> {
        let ev = Event::Damage {
            player: player_id.map(str::to_string),
            blockhead,
            amount,
            attacker: attacker.map(str::to_string),
            cancelled: false,
        };
        self.with(Some(amount), |h| match h.fire(ev) {
            Event::Damage { cancelled: false, amount, .. } => Some(amount),
            _ => None,
        })
    }
    fn death(&self, player_id: Option<&str>, blockhead: u64) {
        let ev = Event::Death { player: player_id.map(str::to_string), blockhead };
        self.with((), |h| {
            h.fire(ev);
        });
    }
    fn player_move(&self, player_id: &str, from: IntPair, to: IntPair) {
        let ev = Event::PlayerMove { player: player_id.to_string(), from: pos(from), to: pos(to) };
        self.with((), |h| {
            h.fire(ev);
        });
    }
}

/// Loads the mods in `dir` for world `save_id`. `None` when no mod loaded.
pub(super) fn load(dir: &Path, save_id: Option<&str>, server: &Rc<BHServer>) -> Option<Rc<dyn ModHooks>> {
    let mut opts = HostOptions::new(dir, Side::Server);
    let scale = super::budget_scale();
    opts.budgets.soft *= scale;
    opts.budgets.hard *= scale;
    opts.budgets.step *= scale;
    // WASM components; compiled code is cached in `<mods>/.cache` (hidden, so discovery skips it).
    let wasm = WasmConfig { cache_dir: Some(dir.join(".cache")), ..WasmConfig::default() };
    if let Ok(rt) = WasmRuntime::new(wasm) {
        opts.runtimes.push(Box::new(rt));
    }
    // A world always has a save id on the dedicated server; `(null)` mirrors ObjC's nil formatting otherwise.
    let world_dir = PathBuf::from(crate::foundation::paths::save_dir(save_id.unwrap_or("(null)")));
    opts.storage = Box::new(LmdbStorage::new(world_dir.join("mods/storage")));
    let net = Rc::new(RefCell::new(ModNet::default()));
    let bridge = Rc::new(Bridge { srv: Rc::downgrade(server), net: net.clone() });
    let host = ModHost::load(opts, bridge.clone());
    let loaded: Vec<String> = host.mods().into_iter().map(|m| m.id).collect();
    let catalog = super::catalog::build(dir, &loaded, &mut |m| GameBridge::log(&*bridge, None, LogLevel::Warn, &m));
    if host.is_empty() && catalog.is_empty() {
        return None;
    }
    let offered = catalog.entries.iter().filter(|e| e.bytes.is_some()).count();
    if offered > 0 {
        let req = if catalog.client_required { "required" } else { "optional" };
        GameBridge::log(
            &*bridge,
            None,
            LogLevel::Info,
            &format!("offering {offered} client mod(s) to modded clients ({req})"),
        );
    }
    net.borrow_mut().catalog = catalog;
    Some(Rc::new(ServerMods {
        host: RefCell::new(host),
        srv: Rc::downgrade(server),
        joined: RefCell::new(BTreeSet::new()),
        net,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::net::bhmatch::{MatchDelegate, PlayerConnectionState};
    use crate::plist::value::PlistValue;
    use crate::server::bhserver::test_support::Harness;

    #[test]
    fn bridge_without_a_server() {
        let b = Bridge { srv: Weak::new(), net: Rc::default() };
        assert_eq!(b.side(), Side::Server);
        assert!(b.host_name().starts_with("blockheads-server "));
        assert!(b.online_players().is_empty());
        assert_eq!(b.player_alias("p"), None);
        assert_eq!(b.player_role("p"), Role::Player);
        assert!(!b.send_chat("p", "x"));
        b.broadcast("x");
        assert!(!b.kick("p", "x"));
        assert!(b.is_builtin_command("help") && !b.is_builtin_command("hello"));
        assert_eq!(b.channel_send("m", "m:c", &ChannelTarget::AllPlayers, b"x"), Ok(()));
        assert!(b.channel_send("m", "m:c", &ChannelTarget::Server, b"x").is_err());
        GameBridge::log(&b, None, LogLevel::Debug, "hidden");
        GameBridge::log(&b, None, LogLevel::Info, "host line");
        GameBridge::log(&b, Some("m"), LogLevel::Warn, "mod warning");
    }

    #[test]
    fn busy_host_returns_defaults() {
        let tmp = tempfile::tempdir().unwrap();
        let bridge = Rc::new(Bridge { srv: Weak::new(), net: Rc::default() });
        let host = ModHost::load(HostOptions::new(tmp.path(), Side::Server), bridge);
        let sm = ServerMods {
            host: RefCell::new(host),
            srv: Weak::new(),
            joined: RefCell::new(BTreeSet::new()),
            net: Rc::default(),
        };
        let _busy = sm.host.borrow_mut();
        assert_eq!(sm.chat("p", "x"), ChatVerdict::Keep);
        assert!(!sm.owns_command("/x"));
        assert!(!sm.command(None, "/x"));
        sm.tick(0.2);
        drop(_busy);
        sm.tick(0.2);
        // Without a server the protocol hooks do nothing.
        assert_eq!(sm.pre_login("e", "p", "a", None), None);
        sm.mod_packet("e", &[0xbc, 0x02, 0xa0]);
        sm.net.borrow_mut().rekey("x", "y");
        sm.poll();
        // Nor does the limbo: a peer in it is forgotten, not dropped.
        sm.net.borrow_mut().catalog.client_required = true;
        assert!(sm.limbo_join("e", "p", None));
        assert!(sm.limbo_packet("p"));
        assert!(sm.limbo_join("e", "p", None));
        sm.drop_expired_limbo(Instant::now() + LIMBO_GRACE);
        assert!(!sm.limbo_packet("p"));
    }

    /// Keeps the host installed for the test and removes it afterwards (the hooks are thread-local).
    struct Installed(Rc<ServerMods>, tempfile::TempDir);

    impl Drop for Installed {
        fn drop(&mut self) {
            crate::mods::install(None);
        }
    }

    fn installed(h: &Harness, client_required: bool) -> Installed {
        let tmp = tempfile::tempdir().unwrap();
        let net = Rc::new(RefCell::new(ModNet::default()));
        net.borrow_mut().catalog.client_required = client_required;
        let bridge = Rc::new(Bridge { srv: Rc::downgrade(&h.srv), net: net.clone() });
        let host = ModHost::load(HostOptions::new(tmp.path(), Side::Server), bridge);
        let sm = Rc::new(ServerMods {
            host: RefCell::new(host),
            srv: Rc::downgrade(&h.srv),
            joined: RefCell::new(BTreeSet::new()),
            net,
        });
        crate::mods::install(Some(sm.clone()));
        Installed(sm, tmp)
    }

    fn limbo_join(h: &Harness, alias: &str) -> String {
        let pid = h.join(alias);
        // In the limbo: the summary with the readable welcome text, no player, no approval, no rejection yet.
        let pk = h.m.packets_to(&pid);
        let summary = pk.iter().find(|p| p[0] == 0x01).expect("summary");
        let xml = crate::foundation::gzip::gzip_inflate(&summary[1..]).unwrap();
        let PlistValue::Dict(d) = crate::plist::helpers::property_list_from_data(&xml).unwrap() else { panic!() };
        assert_eq!(d.map.get(&"welcomeMessage".to_string()).and_then(|v| v.as_str()), Some(crate::mods::LIMBO_TEXT));
        assert!(pk.iter().all(|p| p[0] != 0x26));
        assert!(h.srv.connected_client_ids().is_empty());
        assert!(h.srv.unapproved_clients.borrow().is_empty());
        assert!(!h.w.calls.borrow().join("|").contains("client_connected"));
        assert!(h.m.approved.borrow().is_empty() && h.m.removed.borrow().is_empty());
        pid
    }

    #[test]
    fn vanilla_client_waits_in_the_limbo_and_any_packet_ends_it() {
        let h = Harness::new("m_limbo_packet");
        let _mods = installed(&h, true);
        let pid = limbo_join(&h, "Vic");
        h.m.clear();
        h.send(&pid, &[0x05]);
        assert!(h.m.packets_to(&pid).is_empty());
        assert_eq!(*h.m.removed.borrow(), vec![pid.clone()]);
        assert_eq!(*h.m.disconnected.borrow(), vec![pid.clone()]);
        // Later packets are ignored.
        h.m.clear();
        h.send(&pid, &[0x05]);
        assert!(h.m.sent.borrow().is_empty());
    }

    #[test]
    fn vanilla_client_is_dropped_when_the_grace_ends() {
        let h = Harness::new("m_limbo_grace");
        let mods = installed(&h, true);
        let pid = limbo_join(&h, "Vic");
        h.m.clear();
        mods.0.poll();
        mods.0.drop_expired_limbo(Instant::now() + LIMBO_GRACE - std::time::Duration::from_secs(1));
        assert!(h.m.sent.borrow().is_empty());
        mods.0.drop_expired_limbo(Instant::now() + LIMBO_GRACE + std::time::Duration::from_secs(1));
        assert!(h.m.packets_to(&pid).is_empty());
        assert_eq!(*h.m.removed.borrow(), vec![pid.clone()]);
        assert_eq!(*h.m.disconnected.borrow(), vec![pid]);
        assert!(h.srv.connected_client_ids().is_empty());
    }

    #[test]
    fn vanilla_client_leaving_or_failing_is_forgotten() {
        let h = Harness::new("m_limbo_gone");
        let mods = installed(&h, true);
        let a = limbo_join(&h, "Ann");
        let b = limbo_join(&h, "Bob");
        h.srv.match_player_did_change_state(&*h.m, &a, PlayerConnectionState::Disconnected);
        h.srv.match_connection_with_player_failed_with_error(&*h.m, &b);
        h.m.clear();
        // Nothing left to drop.
        mods.0.drop_expired_limbo(Instant::now() + LIMBO_GRACE * 2);
        assert!(!mods.0.limbo_packet(&a) && !mods.0.limbo_packet(&b));
        assert!(h.m.sent.borrow().is_empty());
    }

    #[test]
    fn only_vanilla_clients_of_client_required_servers_get_the_limbo() {
        // No client-required mods: the vanilla join is approved as before.
        let h = Harness::new("m_limbo_optional");
        let _mods = installed(&h, false);
        let pid = h.join("Vic");
        assert_eq!(h.srv.connected_client_ids(), vec![pid]);
        // A client with a loader key is never held.
        let h = Harness::new("m_limbo_key");
        let mods = installed(&h, true);
        assert!(!mods.0.limbo_join("e", "p", Some(&LoaderKey::default())));
        assert!(!mods.0.limbo_packet("p"));
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
