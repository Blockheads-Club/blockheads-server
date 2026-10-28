//! The server's mod host: `bhmod-core` behind the [`ModHooks`] seam, a [`GameBridge`] over `BHServer`, and mod
//! storage in a separate LMDB environment (`<world>/mods/storage`), never in the vanilla world database.

use std::cell::RefCell;
use std::collections::BTreeSet;
use std::io;
use std::path::{Path, PathBuf};
use std::rc::{Rc, Weak};
use std::time::Instant;

use bhmod_content::Kind;
use bhmod_core::host::split_command;
use bhmod_core::host::{ModState, ReloadOutcome};
use bhmod_core::remote::{RemoteCall, RemoteError, RemoteTarget};
use bhmod_core::storage::{KvBackend, StorageProvider};
use bhmod_core::types::{
    ApiError, BreakLayer, ChannelSource, ChannelTarget, CommandSender, ContentKind, Event, EventKind, LogLevel,
    ObjectKind, Pos, RejectReason, Role, SavePhase, Side,
};
use bhmod_core::watch::Watcher;
use bhmod_core::{GameBridge, HostOptions, ModHost};
use bhmod_manifest::content::SLOTS;
use bhmod_wasm::{WasmConfig, WasmRuntime};

use super::admin::{self, Reply};
use super::catalog;
use super::net::{pre_login_text, Inbound, Link, ModNet};
use super::voice::{self, VoiceOut};
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
    "mods",
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
    fn send_unreliable(&self, peer: &str, packet: &[u8]) -> bool {
        self.0.send_unsequenced(peer, packet)
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
    fn players(&self) -> Option<(u32, u32)> {
        // The same limit `full()` applies: the configured maximum, never more than 32.
        let capacity = u32::try_from(self.0.max_players.get()).ok().filter(|c| *c > 0)?.min(32);
        Some((u32::try_from(self.0.connected_clients.borrow().len()).unwrap_or(u32::MAX), capacity))
    }
}

/// Where the voice relay sends: unsequenced 0xBC packets to modded players in the game.
struct VoiceLink<'a> {
    link: SrvLink<'a>,
    net: &'a ModNet,
}

impl VoiceOut for VoiceLink<'_> {
    fn send(&mut self, peer: &str, packet: &[u8]) -> bool {
        self.net.send_unreliable(&self.link, peer, packet)
    }
}

/// The game as seen by mods.
pub(super) struct Bridge {
    pub(super) srv: Weak<BHServer>,
    pub(super) net: Rc<RefCell<ModNet>>,
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
    fn content_id(&self, kind: ContentKind, key: &str) -> Option<u64> {
        self.net.borrow().content.id(registry_kind(kind), key)
    }
    fn content_key(&self, kind: ContentKind, id: u64) -> Option<String> {
        self.net.borrow().content.key(registry_kind(kind), id).map(str::to_string)
    }
    fn set_cosmetic(&self, player: &str, slot: &str, id: Option<u64>) -> Result<(), ApiError> {
        if !SLOTS.contains(&slot) {
            return Err(ApiError::InvalidArgument(format!("unknown slot `{slot}`")));
        }
        let s = self.srv.upgrade().filter(|s| self.online(s, player)).ok_or(ApiError::StaleHandle)?;
        self.net.borrow_mut().set_cosmetic(&SrvLink(&s), player, slot, id)
    }
    fn cosmetics_of(&self, player: &str) -> Vec<(String, u64)> {
        self.net.borrow().worn(player)
    }
    fn give_item(&self, player: &str, item: u64, count: u32) -> Result<(), ApiError> {
        self.srv.upgrade().filter(|s| self.online(s, player)).ok_or(ApiError::StaleHandle)?;
        let item = u32::try_from(item).map_err(|_| ApiError::InvalidArgument("item id out of range".into()))?;
        if crate::world::lifecycle::core_hooks().mods_give_item(player, item, count) {
            Ok(())
        } else {
            Err(ApiError::Internal("the player has no blockhead in the world".into()))
        }
    }
    fn place_block(&self, x: i32, y: i32, block: u64) -> Result<(), ApiError> {
        if !super::variant_blocks_enabled() {
            return Err(ApiError::Unsupported);
        }
        let info = self.net.borrow().content.variants.get(&block).copied();
        let (Some(info), Ok(id)) = (info, u16::try_from(block)) else {
            return Err(ApiError::InvalidArgument("that block has no base block".into()));
        };
        let record = |wx: i32, wy: i32| {
            bhmod_content::variant::encode(
                &bhmod_content::variant::Variant { id, base: info.base, layer: 0, data: 0 },
                wx,
                wy,
            )
        };
        if crate::world::lifecycle::core_hooks().mods_set_tile_record(x, y, info.base, &record) {
            Ok(())
        } else {
            Err(ApiError::InvalidArgument(format!("the tile ({x}, {y}) is not loaded")))
        }
    }
    fn block_at(&self, x: i32, y: i32) -> Option<u64> {
        let (wx, type_index, padding) = crate::world::lifecycle::core_hooks().mods_tile_record(x, y)?;
        super::variant_of(&padding, type_index, wx, y).map(|(id, _)| u64::from(id))
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
    fn service_send(&self, call: &RemoteCall) -> Result<(), RemoteError> {
        let (RemoteTarget::Player(peer), Some(s)) = (&call.target, self.srv.upgrade()) else {
            return Err(RemoteError::Unavailable);
        };
        self.net.borrow().send_service_call(&SrvLink(&s), peer, call)
    }
}

fn registry_kind(kind: ContentKind) -> Kind {
    match kind {
        ContentKind::Block => Kind::Blocks,
        ContentKind::Item => Kind::Items,
        ContentKind::Cosmetic => Kind::Cosmetics,
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

/// The arguments of a `/mods` line (`None` for any other line).
fn mods_args(line: &str) -> Option<Vec<String>> {
    split_command(line).filter(|(name, _)| name == "mods").map(|(_, args)| args)
}

/// True for a `/mod` line that is not the host's `/mod settings <mod>` or `/mod set <mod> <key> <value>`: those stay the
/// built-in `/MOD player_name` (adds a player to the modlist), so mods never change what vanilla `/mod` does.
fn vanilla_mod(line: &str) -> bool {
    split_command(line).is_some_and(|(name, args)| {
        let host = args.len() >= 2 && (args[0] == "settings" || args[0] == "set");
        name == bhmod_core::host::MOD_COMMAND && !host
    })
}

/// The installed host.
struct ServerMods {
    host: RefCell<ModHost>,
    srv: Weak<BHServer>,
    joined: RefCell<BTreeSet<String>>,
    net: Rc<RefCell<ModNet>>,
    /// The mods directory (what a reload reads again).
    dir: PathBuf,
    /// The developer file watcher (`BLOCKHEADS_MODS_WATCH`).
    watch: RefCell<Option<Watcher>>,
}

impl ServerMods {
    fn bridge(&self) -> Bridge {
        Bridge { srv: self.srv.clone(), net: self.net.clone() }
    }

    /// `/mods ...` from the console or a player: admins only.
    fn mods_command(&self, issuer: Option<&str>, args: &[String]) {
        let allowed = issuer.is_none_or(|p| self.bridge().player_role(p) >= Role::Admin);
        let reply = if allowed {
            let reply = self.with(Reply::busy(), |h| admin::run(h, args));
            if reply.changed {
                self.refresh_catalog();
            }
            reply
        } else {
            Reply { lines: vec!["You do not have permission to use /mods.".to_string()], changed: false }
        };
        if let Some(s) = self.srv.upgrade() {
            for line in &reply.lines {
                s.reply_to(line, issuer);
            }
        }
    }

    /// Builds the offered packages again after mods were reloaded, so players who join from now on are offered the
    /// new client halves. Players already in the game keep what they run. Content registered in the world (ids of
    /// cosmetics, items, blocks) is not rebuilt: changes there need a restart.
    fn refresh_catalog(&self) {
        let running: Vec<String> = self.with(Vec::new(), |h| {
            h.mods().into_iter().filter(|m| !matches!(m.state, ModState::Disabled(_))).map(|m| m.id).collect()
        });
        let bridge = self.bridge();
        let catalog = catalog::build(&self.dir, &running, &mut |m| GameBridge::log(&bridge, None, LogLevel::Warn, &m));
        self.net.borrow_mut().catalog = catalog;
    }

    /// The developer watcher found a change: reload what changed and log it.
    fn watched_reload(&self) {
        let results = self.with(Vec::new(), |h| h.reload(None));
        let bridge = self.bridge();
        let mut changed = false;
        for r in results.iter().filter(|r| r.outcome != ReloadOutcome::Unchanged) {
            let failed = matches!(r.outcome, ReloadOutcome::Failed(_));
            changed |= !failed;
            let level = if failed { LogLevel::Warn } else { LogLevel::Info };
            GameBridge::log(&bridge, None, level, &format!("watch: {r}"));
        }
        if changed {
            self.refresh_catalog();
        }
    }
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
        if let Some(s) = self.srv.upgrade() {
            self.net.borrow().cosmetics_full(&SrvLink(&s), player_id);
        }
        self.with((), |h| {
            h.fire(Event::PlayerJoin { player: player_id.to_string() });
        });
    }
    fn player_quit(&self, player_id: &str, was_kick: bool) {
        if !self.joined.borrow_mut().remove(player_id) {
            return;
        }
        if let Some(s) = self.srv.upgrade() {
            self.net.borrow_mut().player_left(&SrvLink(&s), player_id);
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
        !vanilla_mod(line) && (mods_args(line).is_some() || self.with(false, |h| h.owns_command(line)))
    }
    fn command(&self, issuer: Option<&str>, line: &str) -> bool {
        if vanilla_mod(line) {
            return false;
        }
        if let Some(args) = mods_args(line) {
            self.mods_command(issuer, &args);
            return true;
        }
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
        if self.watch.borrow_mut().as_mut().is_some_and(|w| w.changed(Instant::now())) {
            self.watched_reload();
        }
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
        // Voice frames take the fast path: straight to the relay, no mod is asked; without one they are dropped
        // like any unknown op.
        if data.get(1) == Some(&voice::VOICE_FRAME) {
            let net = self.net.borrow();
            if net.is_modded(peer) && SrvLink(&s).approved(peer) {
                voice::dispatch(peer, &data[2..], &mut VoiceLink { link: SrvLink(&s), net: &net });
            }
            return;
        }
        let names = self.with(Vec::new(), |h| h.channel_names());
        let inbound = self.net.borrow_mut().packet(&SrvLink(&s), peer, data, &names);
        let from = RemoteTarget::Player(peer.to_string());
        match inbound {
            // A full queue drops the message (the limits of bhmod-core channels apply).
            Some(Inbound::Channel(name, payload)) => {
                self.with(false, |h| h.channel_receive(&name, ChannelSource::Player(peer.to_string()), payload));
            }
            Some(Inbound::Call(c)) => {
                // The caller must be a client mod the player runs (READY.loaded); what it may reach is decided by
                // the provider's `remote = true` and the host's rate limit. The net state is not borrowed while
                // the provider runs (it may send channel messages).
                let runs = self.net.borrow().runs_mod(peer, &c.mod_id);
                let outcome = if runs {
                    self.with(Err(RemoteError::Busy), |h| {
                        h.remote_call_in(&from, &c.mod_id, &c.service, &c.version_req, &c.method, &c.args)
                    })
                } else {
                    Err(RemoteError::NotPermitted)
                };
                self.net.borrow().send_service_result(&SrvLink(&s), peer, c.id, outcome);
            }
            Some(Inbound::Result(r)) => {
                let outcome = r.outcome.map_err(RemoteError::from);
                self.with(false, |h| h.remote_result(&from, r.id, outcome));
            }
            None => {}
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
    opts.remote = super::remote_limits();
    // WASM components; compiled code is cached in `<mods>/.cache` (hidden, so discovery skips it).
    let wasm = WasmConfig { cache_dir: Some(dir.join(".cache")), ..WasmConfig::default() };
    opts.runtimes.extend(WasmRuntime::new(wasm).ok().map(|rt| Box::new(rt) as _));
    // JVM mods: only when the owner turned them on (feature `jvm`, `BLOCKHEADS_JVM=1`). Logged once the bridge exists.
    #[cfg(feature = "jvm")]
    let mut notes: Vec<(LogLevel, String)> = Vec::new();
    #[cfg(feature = "jvm")]
    if let Some(mut cfg) = super::jvm_settings(&|k| std::env::var(k).ok()) {
        cfg.work_dir = Some(dir.join(".cache"));
        match bhmod_core::jvm::JvmRuntime::new(cfg) {
            Ok(rt) => {
                opts.runtimes.push(Box::new(rt));
                notes.push((
                    LogLevel::Warn,
                    "JVM mods are enabled: they run unsandboxed (permission `native`)".to_string(),
                ));
            }
            Err(e) => notes.push((LogLevel::Error, format!("JVM mods are on but the JVM could not be started: {e}"))),
        }
    }
    // A world always has a save id on the dedicated server; `(null)` mirrors ObjC's nil formatting otherwise.
    let world_dir = PathBuf::from(crate::foundation::paths::save_dir(save_id.unwrap_or("(null)")));
    opts.storage = Box::new(LmdbStorage::new(world_dir.join("mods/storage")));
    let net = Rc::new(RefCell::new(ModNet::default()));
    let bridge = Rc::new(Bridge { srv: Rc::downgrade(server), net: net.clone() });
    #[cfg(feature = "jvm")]
    for (level, note) in &notes {
        GameBridge::log(&*bridge, None, *level, note);
    }
    let host = ModHost::load(opts, bridge.clone());
    let loaded: Vec<String> = host.mods().into_iter().map(|m| m.id).collect();
    let catalog = super::catalog::build(dir, &loaded, &mut |m| GameBridge::log(&*bridge, None, LogLevel::Warn, &m));
    if host.is_empty() && catalog.is_empty() {
        return None;
    }
    let offered = catalog.entries.iter().filter(|e| e.is_client()).count();
    if offered > 0 {
        let req = if catalog.client_required { "required" } else { "optional" };
        GameBridge::log(
            &*bridge,
            None,
            LogLevel::Info,
            &format!("offering {offered} client mod(s) to modded clients ({req})"),
        );
    }
    let offered: Vec<String> = catalog.entries.iter().filter(|e| e.is_client()).map(|e| e.info.id.clone()).collect();
    let content = super::content::build(dir, &offered, &world_dir.join("mods/registry.toml"), &mut |m| {
        GameBridge::log(&*bridge, None, LogLevel::Error, &m)
    });
    super::set_tables(content.tables(super::variant_blocks_enabled()));
    net.borrow_mut().catalog = catalog;
    net.borrow_mut().content = content;
    let watch = super::watch_enabled().then(|| Watcher::new(dir));
    if watch.is_some() {
        GameBridge::log(&*bridge, None, LogLevel::Info, "watching the mods directory: changed mods are reloaded");
    }
    Some(Rc::new(ServerMods {
        host: RefCell::new(host),
        srv: Rc::downgrade(server),
        joined: RefCell::new(BTreeSet::new()),
        net,
        dir: dir.to_path_buf(),
        watch: RefCell::new(watch),
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
        // Service calls need a server and a player.
        let mut call = RemoteCall {
            id: 1,
            target: RemoteTarget::Player("p".into()),
            mod_id: "m".into(),
            service: "s".into(),
            version_req: "*".into(),
            method: "x".into(),
            args: vec![0x80],
        };
        assert_eq!(b.service_send(&call), Err(RemoteError::Unavailable));
        call.target = RemoteTarget::Server;
        assert_eq!(b.service_send(&call), Err(RemoteError::Unavailable));
        GameBridge::log(&b, None, LogLevel::Debug, "hidden");
        GameBridge::log(&b, None, LogLevel::Info, "host line");
        GameBridge::log(&b, Some("m"), LogLevel::Warn, "mod warning");
    }

    #[test]
    fn content_bridge_without_a_server() {
        let b = Bridge { srv: Weak::new(), net: Rc::default() };
        b.net.borrow_mut().content = crate::mods::content::Content {
            wire: [("cosmetics".to_string(), [("hats:crown".to_string(), 1u64)].into())].into(),
            cosmetics: [(1, "hat".to_string())].into(),
            ..Default::default()
        };
        assert_eq!(b.content_id(ContentKind::Cosmetic, "hats:crown"), Some(1));
        assert_eq!(b.content_id(ContentKind::Cosmetic, "hats:none"), None);
        assert_eq!(b.content_key(ContentKind::Cosmetic, 1).as_deref(), Some("hats:crown"));
        assert_eq!(b.content_key(ContentKind::Cosmetic, 7), None);
        assert_eq!(b.content_key(ContentKind::Item, 1), None);
        assert_eq!(b.content_id(ContentKind::Block, "hats:crown"), None);
        assert_eq!(b.content_id(ContentKind::Item, "hats:crown"), None);
        assert!(matches!(b.set_cosmetic("p", "cape", Some(1)), Err(ApiError::InvalidArgument(_))));
        assert_eq!(b.set_cosmetic("p", "hat", Some(1)), Err(ApiError::StaleHandle));
        assert!(b.cosmetics_of("p").is_empty());
    }

    /// A link that says nobody is in the game yet (so a `HELLO` is accepted) and swallows what is sent.
    struct Waiting;

    impl Link for Waiting {
        fn send(&self, _: &str, _: Vec<u8>) {}
        fn send_unreliable(&self, _: &str, _: &[u8]) -> bool {
            false
        }
        fn reject(&self, _: &str, _: u8) {}
        fn approved(&self, _: &str) -> bool {
            false
        }
        fn alive(&self, _: &str) -> bool {
            true
        }
        fn players(&self) -> Option<(u32, u32)> {
            None
        }
    }

    #[test]
    fn service_calls_with_a_player_in_the_game() {
        use bhmod_proto::{Hello, Message, Ready, ServiceCall, ServiceResult};
        // The stand-in link: nobody is approved, everybody is alive, rejections are swallowed.
        assert!(Waiting.alive("x") && !Waiting.approved("x"));
        Waiting.reject("x", 3);
        assert!(!Waiting.send_unreliable("x", b""));
        let h = Harness::new("m_svc_player");
        let mods = installed(&h, false);
        mods.0.world_loaded("World", "w");
        let pid = h.join("Sam");
        let bridge = Bridge { srv: Rc::downgrade(&h.srv), net: mods.0.net.clone() };
        let call = RemoteCall {
            id: 1,
            target: RemoteTarget::Player(pid.clone()),
            mod_id: "srv".into(),
            service: "hud".into(),
            version_req: "*".into(),
            method: "title".into(),
            args: vec![0x80],
        };
        // A player without the mod loader cannot be asked.
        h.m.clear();
        assert_eq!(bridge.service_send(&call), Err(RemoteError::Unavailable));
        assert!(h.m.packets_to(&pid).is_empty());
        // After the protocol started (before approval, as on the wire) the call goes out as SERVICE_CALL.
        let hello = Message::Hello(Hello {
            proto: 1,
            loader: "0.1.0".into(),
            platform: "pc-linux-x86_64".into(),
            api: "0.3.1".into(),
            runtimes: vec!["lua".into()],
            cached: vec![],
            features: vec![],
        });
        mods.0.net.borrow_mut().packet(&Waiting, &pid, &hello.encode(), &[]);
        assert_eq!(bridge.service_send(&call), Ok(()));
        let sent = h.m.packets_to(&pid);
        assert!(matches!(Message::decode(&sent[0]), Ok(Message::ServiceCall(c)) if c.id == 1 && c.service == "hud"));
        // The player calls a service: nothing is provided here (NOT_FOUND); a caller mod the player did not load
        // is refused first (NOT_PERMITTED); an answer to a call nobody made is dropped.
        let answered = |h: &Harness, code: u8| {
            matches!(
                Message::decode(h.m.packets_to(&pid).last().unwrap()),
                Ok(Message::ServiceResult(ServiceResult { outcome: Err(f), .. })) if f.code == code
            )
        };
        let wire = |mod_id: &str| {
            Message::ServiceCall(ServiceCall {
                id: 9,
                mod_id: mod_id.into(),
                service: "prices".into(),
                version_req: "*".into(),
                method: "price".into(),
                args: vec![0x80],
            })
            .encode()
        };
        h.m.clear();
        mods.0.mod_packet(&pid, &wire("demo"));
        assert!(answered(&h, bhmod_proto::service_error::NOT_FOUND));
        // The cosmetics seam of the same bridge, for a player in the game, without content mods.
        assert_eq!(bridge.set_cosmetic(&pid, "hat", None), Err(ApiError::Unsupported));
        // While a mod call is running (the host is borrowed) the answer is BUSY.
        {
            let _running = mods.0.host.borrow_mut();
            h.m.clear();
            mods.0.mod_packet(&pid, &wire("demo"));
            assert!(answered(&h, bhmod_proto::service_error::BUSY));
        }
        let ready = Message::Ready(Ready { accepted_hash: [0; 32], loaded: vec![] });
        mods.0.net.borrow_mut().packet(&Waiting, &pid, &ready.encode(), &[]);
        h.m.clear();
        mods.0.mod_packet(&pid, &wire("demo"));
        assert!(answered(&h, bhmod_proto::service_error::NOT_PERMITTED));
        h.m.clear();
        let stray = Message::ServiceResult(ServiceResult { id: 5, outcome: Ok(vec![0xf6]) });
        mods.0.mod_packet(&pid, &stray.encode());
        assert!(h.m.packets_to(&pid).is_empty());
    }

    #[test]
    fn voice_frames_take_the_fast_path_to_the_relay() {
        use crate::mods::voice::{self, VoiceOut, VoiceSink, VOICE_FRAME};
        use bhmod_proto::{Hello, Message};
        /// Relays every frame back to its speaker and to a vanilla player, and records the outcomes.
        struct Back(Rc<RefCell<Vec<(String, Vec<u8>, bool, bool)>>>, String);
        impl VoiceSink for Back {
            fn frame(&mut self, peer: &str, body: &[u8], out: &mut dyn VoiceOut) {
                let mut p = vec![0xbc, VOICE_FRAME];
                p.extend_from_slice(body);
                let to_speaker = out.send(peer, &p);
                let to_vanilla = out.send(&self.1, &p);
                self.0.borrow_mut().push((peer.to_string(), body.to_vec(), to_speaker, to_vanilla));
            }
        }
        let h = Harness::new("m_voice_fast");
        let mods = installed(&h, false);
        mods.0.world_loaded("World", "w");
        let pid = h.join("Sam");
        let vanilla = h.join("Van");
        let frame = [0xbc, VOICE_FRAME, 1, 2, 3];
        // No relay: dropped, nothing sent either way.
        voice::set_sink(None);
        h.m.clear();
        mods.0.mod_packet(&pid, &frame);
        assert!(h.m.sent.borrow().is_empty() && h.m.unsequenced.borrow().is_empty());
        let seen = Rc::new(RefCell::new(Vec::new()));
        voice::set_sink(Some(Box::new(Back(seen.clone(), vanilla.clone()))));
        // A player without the mod loader reaches no relay.
        mods.0.mod_packet(&pid, &frame);
        assert!(seen.borrow().is_empty());
        let hello = Message::Hello(Hello {
            proto: 1,
            loader: "0.1.0".into(),
            platform: "pc-linux-x86_64".into(),
            api: "0.3.1".into(),
            runtimes: vec![],
            cached: vec![],
        });
        mods.0.net.borrow_mut().packet(&Waiting, &pid, &hello.encode(), &[]);
        mods.0.net.borrow_mut().packet(&Waiting, "waiting", &hello.encode(), &[]);
        // Modded but not in the game: no relay either.
        mods.0.mod_packet("waiting", &frame);
        assert!(seen.borrow().is_empty());
        // Modded and in the game: the relay gets the body; the echo leaves unsequenced, the vanilla copy does not.
        mods.0.mod_packet(&pid, &frame);
        assert_eq!(*seen.borrow(), [(pid.clone(), vec![1, 2, 3], true, false)]);
        assert_eq!(*h.m.unsequenced.borrow(), [(pid.clone(), frame.to_vec())]);
        assert!(h.m.sent.borrow().is_empty(), "nothing reliable");
        voice::set_sink(None);
    }

    #[test]
    fn cosmetics_are_set_for_an_online_player() {
        let h = Harness::new("m_bridge_cosmetics");
        let pid = h.join("Ann");
        let b = Bridge { srv: Rc::downgrade(&h.srv), net: Rc::default() };
        b.net.borrow_mut().content = crate::mods::content::Content {
            wire: [("cosmetics".to_string(), [("hats:crown".to_string(), 1u64)].into())].into(),
            cosmetics: [(1, "hat".to_string())].into(),
            ..Default::default()
        };
        assert_eq!(b.set_cosmetic(&pid, "hat", Some(1)), Ok(()));
        assert_eq!(b.cosmetics_of(&pid), vec![("hat".to_string(), 1)]);
        assert_eq!(b.set_cosmetic(&pid, "hat", None), Ok(()));
        assert!(b.cosmetics_of(&pid).is_empty());
        assert_eq!(b.set_cosmetic("nobody", "hat", Some(1)), Err(ApiError::StaleHandle));
    }

    /// The mod seams of the world: a recorder in place of the production glue.
    #[derive(Default)]
    struct World {
        given: RefCell<Vec<(String, u32, u32)>>,
        tile: RefCell<Option<(u8, [u8; 16])>>,
        has_blockhead: bool,
    }
    impl crate::world::lifecycle::WorldCoreHooksStub for World {
        fn mods_give_item(&self, client_id: &str, item: u32, count: u32) -> bool {
            self.given.borrow_mut().push((client_id.to_string(), item, count));
            self.has_blockhead
        }
        fn mods_tile_record(&self, x: i32, _y: i32) -> Option<(i32, u8, [u8; 16])> {
            let (ty, pad) = (*self.tile.borrow())?;
            Some((x.rem_euclid(1024), ty, pad))
        }
        fn mods_set_tile_record(&self, x: i32, y: i32, base: u8, record: &dyn Fn(i32, i32) -> [u8; 16]) -> bool {
            if y < 0 {
                return false;
            }
            *self.tile.borrow_mut() = Some((base, record(x.rem_euclid(1024), y)));
            true
        }
    }

    #[test]
    fn items_and_variant_blocks_through_the_bridge() {
        use crate::mods::{set_config, set_tables, ModsConfig};
        let h = Harness::new("m_bridge_items");
        let pid = h.join("Ann");
        let b = Bridge { srv: Rc::downgrade(&h.srv), net: Rc::default() };
        let world = Rc::new(World { has_blockhead: true, ..World::default() });
        crate::world::lifecycle::set_core_hooks(world.clone());

        // Items: the player must be online, the world must have a blockhead for them.
        assert_eq!(b.give_item("nobody", 1107, 2), Err(ApiError::StaleHandle));
        assert_eq!(
            b.give_item(&pid, u64::from(u32::MAX) + 1, 2),
            Err(ApiError::InvalidArgument("item id out of range".into()))
        );
        assert_eq!(b.give_item(&pid, 1107, 2), Ok(()));
        assert_eq!(*world.given.borrow(), vec![(pid.clone(), 1107, 2)]);
        let empty = Rc::new(World::default());
        crate::world::lifecycle::set_core_hooks(empty);
        assert!(matches!(b.give_item(&pid, 1107, 1), Err(ApiError::Internal(_))));
        crate::world::lifecycle::set_core_hooks(world.clone());

        // Variant blocks are off unless the server owner turned them on.
        let variant = crate::mods::VariantInfo { base: 11, drops: None };
        b.net.borrow_mut().content.variants.insert(3, variant);
        assert_eq!(b.place_block(5, 9, 3), Err(ApiError::Unsupported));
        set_config(ModsConfig { variant_blocks: true, ..ModsConfig::default() });
        set_tables(b.net.borrow().content.tables(true));
        assert_eq!(b.place_block(5, 9, 3), Ok(()));
        let (ty, pad) = world.tile.borrow().unwrap();
        let rec = bhmod_content::variant::decode(&pad, ty, 5, 9).unwrap();
        assert_eq!((ty, rec.id, rec.base, rec.layer), (11, 3, 11, 0));
        assert_eq!(b.block_at(5, 9), Some(3));
        // The tile's x wraps: the record belongs to the wrapped position.
        assert_eq!(b.place_block(5 + 1024, 9, 3), Ok(()));
        assert_eq!(b.block_at(5 + 1024, 9), Some(3));
        assert_eq!(b.block_at(6, 9), None);
        // Unknown blocks, blocks without a base, ids too wide for a tile, tiles that are not loaded.
        assert!(matches!(b.place_block(5, 9, 4), Err(ApiError::InvalidArgument(_))));
        b.net.borrow_mut().content.variants.insert(70_000, variant);
        assert!(matches!(b.place_block(5, 9, 70_000), Err(ApiError::InvalidArgument(_))));
        assert!(matches!(b.place_block(5, -1, 3), Err(ApiError::InvalidArgument(_))));
        // A plain tile, and no tile at all.
        *world.tile.borrow_mut() = Some((11, [0; 16]));
        assert_eq!(b.block_at(5, 9), None);
        *world.tile.borrow_mut() = None;
        assert_eq!(b.block_at(5, 9), None);
        set_config(ModsConfig::default());
        set_tables(Default::default());
        crate::world::lifecycle::set_core_hooks(Rc::new(crate::world::lifecycle::NullCoreHooks));
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
            dir: tmp.path().to_path_buf(),
            watch: RefCell::new(None),
        };
        let _busy = sm.host.borrow_mut();
        assert_eq!(sm.chat("p", "x"), ChatVerdict::Keep);
        assert!(!sm.owns_command("/x"));
        assert!(!sm.command(None, "/x"));
        sm.tick(0.2);
        // /mods is the admin's even then, and says so; without a server there is nobody to tell.
        assert!(sm.owns_command("/mods") && sm.command(None, "/mods reload"));
        drop(_busy);
        assert!(sm.command(None, "/mods list"));
        sm.tick(0.2);
        // Without a server the protocol hooks do nothing.
        assert_eq!(sm.pre_login("e", "p", "a", None), None);
        sm.mod_packet("e", &[0xbc, 0x02, 0xa0]);
        // Joining and leaving without a server tell nobody anything.
        sm.player_join("p");
        sm.player_quit("p", false);
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
            dir: tmp.path().to_path_buf(),
            watch: RefCell::new(None),
        });
        crate::mods::install(Some(sm.clone()));
        Installed(sm, tmp)
    }

    #[test]
    fn mods_command_is_for_admins_and_the_console() {
        use crate::mods::admin::tests::write_lua_mod;
        let h = Harness::new("m_admin_cmd");
        let mods = installed(&h, false);
        let sm = &mods.0;
        sm.world_loaded("World", "w");
        let main = "return { on_enable = function() bh.log.info('alpha up') end }";
        write_lua_mod(mods.1.path(), "alpha", "1.0.0", main);
        let pid = h.join("Sam");
        assert!(sm.owns_command("/MODS reload") && sm.owns_command("/mods"));
        // `/mod settings <mod>` and `/mod set ...` are the host's; any other `/mod` is the built-in modlist command.
        assert!(sm.owns_command("/mod settings alpha") && sm.owns_command("/mod set alpha k v"));
        assert!(!sm.owns_command("/mod") && !sm.owns_command("/mod tom") && !sm.owns_command("/MOD settings"));
        // A player who is not an admin is refused and nothing happens.
        h.m.clear();
        assert!(sm.command(Some(&pid), "/mods reload"));
        assert!(sm.host.borrow().is_empty());
        assert!(!h.m.packets_to(&pid).is_empty(), "the refusal reached the player");
        // An admin reloads: the new mod starts in the running world and is offered in the catalog.
        *h.srv.admin_list.borrow_mut() = Some(vec!["Sam".to_string()]);
        h.m.clear();
        assert!(sm.command(Some(&pid), "/mods reload"));
        assert_eq!(sm.host.borrow().mods().len(), 1);
        assert!(!h.m.packets_to(&pid).is_empty());
        assert_eq!(sm.net.borrow().catalog.entries.len(), 1);
        // The console sees the same text; a command that changes nothing leaves the catalog alone.
        let before = h.d.messages().len();
        assert!(sm.command(None, "/mods reload"));
        assert!(sm.command(None, "/mods timings"));
        let said = h.d.messages()[before..].to_vec();
        assert_eq!(said[0], "No mod changed on disk (1 checked).");
        assert!(said[1].starts_with("Mod timings over the last 60 s"), "{said:?}");
        // While a mod call runs the host cannot be entered.
        let busy = sm.host.borrow_mut();
        let before = h.d.messages().len();
        assert!(sm.command(None, "/mods"));
        drop(busy);
        assert_eq!(h.d.messages()[before..], ["The mods are busy, try again."]);
    }

    #[test]
    fn mod_settings_through_the_server() {
        let h = Harness::new("m_settings");
        let mods = installed(&h, false);
        let sm = &mods.0;
        sm.world_loaded("World", "w");
        let d = mods.1.path().join("knobs");
        std::fs::create_dir_all(d.join("server")).unwrap();
        let toml = "schema = 1\nid = \"knobs\"\nversion = \"1.0.0\"\nname = { en = \"N\" }\nauthors = [\"a\"]\n\
                    license = \"MIT\"\napi = \"^0.3.3\"\n[sides]\nserver = true\nclient = false\n[entry]\n\
                    server = { runtime = \"lua\", path = \"server/main.luau\" }\n\
                    [settings.radius]\ntype = \"int\"\ndefault = 32\nmin = 4\nmax = 128\nlabel = \"Radius\"\n";
        std::fs::write(d.join("mod.toml"), toml).unwrap();
        // The mod counts the settings-changed events it gets and shows them with the current radius.
        let main = "local changes = 0\n\
            return { on_enable = function()\n\
              bh.events.on_custom('knobs:settings-changed', function() changes += 1 end)\n\
              bh.commands.register({ name = 'radius' }, function() return bh.settings.get('radius') .. ' ' .. changes end)\n\
            end }";
        std::fs::write(d.join("server/main.luau"), main).unwrap();
        assert!(sm.command(None, "/mods reload"));
        let last = |n: usize| h.d.messages()[h.d.messages().len() - n..].to_vec();
        assert!(sm.command(None, "/radius"));
        assert_eq!(last(1), ["32 0"]);
        // A player who is not an admin cannot change it; the built-in `/mod tom` is not taken over.
        let pid = h.join("Sam");
        h.m.clear();
        assert!(sm.command(Some(&pid), "/mod set knobs radius 64"));
        assert!(!h.m.packets_to(&pid).is_empty(), "the refusal reached the player");
        assert!(!sm.command(Some(&pid), "/mod tom") && !sm.command(None, "/mod tom"));
        assert!(sm.command(None, "/radius"));
        assert_eq!(last(1), ["32 0"]);
        // An admin from the server's admin list can; the mod hears about it and mods.toml keeps the value.
        *h.srv.admin_list.borrow_mut() = Some(vec!["Sam".to_string()]);
        assert!(sm.command(Some(&pid), "/mod set knobs radius 64"));
        assert!(sm.command(None, "/mod settings knobs"));
        assert_eq!(last(2), ["knobs settings:", "radius = 64"]);
        assert!(sm.command(None, "/radius"));
        assert_eq!(last(1), ["64 1"]);
        let file = mods.1.path().join("mods.toml");
        assert!(std::fs::read_to_string(&file).unwrap().contains("radius = 64"));
        // An edit of mods.toml is picked up by the poll and reaches the mod too.
        std::fs::write(&file, "[knobs.settings]\nradius = 100\n").unwrap();
        sm.poll();
        assert!(sm.command(None, "/radius"));
        assert_eq!(last(1), ["100 2"]);
    }

    #[test]
    fn the_watcher_reloads_changed_mods() {
        use crate::mods::admin::tests::write_lua_mod;
        let h = Harness::new("m_watch");
        let mods = installed(&h, false);
        let sm = &mods.0;
        sm.world_loaded("World", "w");
        let dir = mods.1.path();
        let main = |tag: &str| {
            format!("return {{ on_enable = function() bh.commands.register({{ name = 'hi' }}, function() return '{tag}' end) end }}")
        };
        write_lua_mod(dir, "alpha", "1.0.0", &main("one"));
        sm.watched_reload();
        assert!(sm.command(None, "/hi"));
        // Poll with the watcher on: the change is seen, and acted on once it is quiet.
        *sm.watch.borrow_mut() = Some(Watcher::new(dir));
        sm.poll();
        write_lua_mod(dir, "alpha", "1.0.1", &main("two"));
        write_lua_mod(dir, "bad", "1.0.0", "this is not luau");
        // It has a client half too: the catalog says why it is not offered when its server half is down.
        let toml = std::fs::read_to_string(dir.join("bad/mod.toml")).unwrap();
        let toml = toml
            .replace("client = false", "client = true")
            .replace("[entry]\n", "[entry]\nclient = { runtime = \"lua\", path = \"client/main.luau\" }\n");
        std::fs::write(dir.join("bad/mod.toml"), toml).unwrap();
        std::fs::create_dir_all(dir.join("bad/client")).unwrap();
        std::fs::write(dir.join("bad/client/main.luau"), "return {}").unwrap();
        let settle = || std::thread::sleep(bhmod_core::watch::INTERVAL + std::time::Duration::from_millis(100));
        settle();
        sm.poll();
        settle();
        sm.poll();
        let mods_now = sm.host.borrow().mods();
        assert_eq!(mods_now.iter().find(|m| m.id == "alpha").unwrap().version, "1.0.1");
        assert!(matches!(mods_now.iter().find(|m| m.id == "bad").unwrap().state, ModState::Disabled(_)));
        let before = h.d.messages().len();
        assert!(sm.command(None, "/hi"));
        assert_eq!(h.d.messages()[before..], ["two"]);
        // Nothing changed: the watcher stays quiet. A host that is busy ignores a change.
        sm.watched_reload();
        let busy = sm.host.borrow_mut();
        sm.watched_reload();
        drop(busy);
        // Mods that are disabled are not offered.
        assert!(sm.net.borrow().catalog.entries.iter().all(|e| e.info.id != "bad"));
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
    fn limbo_peer_is_dropped_even_without_a_match() {
        let h = Harness::new("m_limbo_no_match");
        let mods = installed(&h, true);
        let pid = limbo_join(&h, "Vic");
        // the server's match is gone (cleanup): nothing to disconnect the peer on, but the drop still completes
        h.srv.node.cleanup();
        assert!(h.srv.match_().is_none());
        h.m.clear();
        mods.0.drop_expired_limbo(Instant::now() + LIMBO_GRACE + std::time::Duration::from_secs(1));
        assert!(h.m.removed.borrow().is_empty() && h.m.disconnected.borrow().is_empty());
        assert!(!mods.0.limbo_packet(&pid));
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
