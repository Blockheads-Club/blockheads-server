//! Mod host seam (not part of the original server).
//!
//! The game code calls [`hooks`] at its hook points; it returns `None` unless mods are actually loaded, and every
//! call site then takes the exact vanilla path (no bytes, RNG draws, save keys or log lines change). The host
//! itself (feature `mods`, `server_host.rs`) builds on `bhmod-core` and is installed by [`start`] only when the mods
//! directory exists and at least one mod loaded.
//!
//! Configuration has no CLI flag (the help text is frozen): the mods directory is `HostConfig.mods.dir`, else
//! `BLOCKHEADS_MODS_DIR`, else `<data dir>/mods`; `mods.toml` in it grants permissions.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use crate::server::bhserver::BHServer;
use crate::types::structs::IntPair;

#[cfg(feature = "mods")]
mod server_host;

/// Embedding-API settings for mods (`HostConfig.mods`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ModsConfig {
    /// The mods directory. `None`: `BLOCKHEADS_MODS_DIR`, else `<data dir>/mods`.
    pub dir: Option<PathBuf>,
    /// True never loads mods, whatever the directory holds.
    pub disabled: bool,
}

/// What the mods decided about a chat message.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChatVerdict {
    /// Show and broadcast it unchanged.
    Keep,
    /// Drop it.
    Cancel,
    /// Show and broadcast this text instead.
    Replace(String),
}

/// Events that are only worth building when a mod listens to them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Optional {
    /// `block-changed` (every tile change).
    BlockChanged,
    /// `player-move` (every tile step of a player's blockhead).
    PlayerMove,
}

/// The calls the server makes into the mod host. Player ids are the vanilla `playerID` strings.
pub trait ModHooks {
    /// A player finished joining (approved, world summary sent).
    fn player_join(&self, player_id: &str);
    /// A player left (`was_kick`: kicked or banned). Fires once per join.
    fn player_quit(&self, player_id: &str, was_kick: bool);
    /// A chat message (not a command) after the profanity filter, before it is shown or broadcast.
    fn chat(&self, player_id: &str, text: &str) -> ChatVerdict;
    /// True when `line` is a mod command, so it is routed to [`ModHooks::command`] for any player.
    fn owns_command(&self, line: &str) -> bool;
    /// A command line from a player (`Some(id)`) or the console (`None`). True when the mods consumed it (replies
    /// are already sent); false runs the built-in command as usual.
    fn command(&self, issuer: Option<&str>, line: &str) -> bool;
    /// One 0.2 s world step, after the world update.
    fn tick(&self, dt: f32);
    /// One 60 Hz network poll, after the packets were handled.
    fn poll(&self);
    /// The world finished loading.
    fn world_loaded(&self, world_name: &str, world_id: &str);
    /// Before (`post = false`) and after a world save.
    fn world_save(&self, post: bool);
    /// The server is stopping (after the final save).
    fn server_stop(&self);

    /// True when a mod listens to `what`.
    fn wants(&self, what: Optional) -> bool;
    /// A player passed every vanilla join check (API 0.2). `Some(reason)`: reject with that 0x26 reason byte.
    fn pre_login(&self, player_id: &str, alias: &str) -> Option<u8>;
    /// A player places `item` into a tile (0x10). True: cancelled (the caller takes the protected-tile path).
    fn block_place(&self, player_id: &str, pos: IntPair, item: u32, data_a: u16, data_b: u16) -> bool;
    /// A player removes a tile (0x11) or a back wall (0x3a). True: cancelled (protected-tile path).
    fn block_break(&self, player_id: &str, pos: IntPair, back_wall: bool, tile: u32) -> bool;
    /// A tile changed (monitor only); `player_id` from [`actor`] when a player's request caused it.
    fn block_changed(&self, pos: IntPair, tile: u32, player_id: Option<&str>);
    /// A player places a workbench (0x15, `item` = workbench type) or an interaction object (0x16, `item` = item
    /// type). True: cancelled.
    fn object_place(&self, player_id: &str, pos: IntPair, workbench: bool, item: u32) -> bool;
    /// A blockhead is about to take `amount` damage. `None`: cancelled; otherwise the (possibly changed) amount.
    fn damage(&self, player_id: Option<&str>, blockhead: u64, amount: f32, attacker: Option<&str>) -> Option<f32>;
    /// A blockhead died.
    fn death(&self, player_id: Option<&str>, blockhead: u64);
    /// A player's blockhead moved to another tile.
    fn player_move(&self, player_id: &str, from: IntPair, to: IntPair);
}

thread_local! {
    static HOOKS: RefCell<Option<Rc<dyn ModHooks>>> = const { RefCell::new(None) };
    static CONFIG: RefCell<ModsConfig> = RefCell::new(ModsConfig::default());
    static ACTOR: RefCell<Option<String>> = const { RefCell::new(None) };
}

/// While alive, tile changes and damage are attributed to a player (the request being handled). Only set when a
/// mod host is installed.
pub struct ActorGuard {
    prev: Option<String>,
}

impl Drop for ActorGuard {
    fn drop(&mut self) {
        ACTOR.with(|a| *a.borrow_mut() = self.prev.take());
    }
}

/// Attributes what happens until the guard drops to `player_id` (see [`actor`]).
pub fn act_as(player_id: &str) -> ActorGuard {
    ActorGuard { prev: ACTOR.with(|a| a.borrow_mut().replace(player_id.to_string())) }
}

/// The player whose request is being handled, if any.
pub fn actor() -> Option<String> {
    ACTOR.with(|a| a.borrow().clone())
}

/// The installed mod host, `None` on the vanilla path.
pub fn hooks() -> Option<Rc<dyn ModHooks>> {
    HOOKS.with(|h| h.borrow().clone())
}

/// Installs (or removes) the mod host of this server thread.
pub fn install(h: Option<Rc<dyn ModHooks>>) {
    HOOKS.with(|c| *c.borrow_mut() = h);
}

/// Sets the configuration of this server thread (the embedding API calls it before the world starts).
pub fn set_config(c: ModsConfig) {
    CONFIG.with(|x| *x.borrow_mut() = c);
}

/// The configuration of this server thread.
pub fn config() -> ModsConfig {
    CONFIG.with(|x| x.borrow().clone())
}

/// The mods directory for this server thread.
pub fn mods_dir() -> PathBuf {
    if let Some(d) = config().dir {
        return d;
    }
    match std::env::var_os("BLOCKHEADS_MODS_DIR") {
        Some(d) if !d.is_empty() => PathBuf::from(d),
        _ => PathBuf::from(crate::foundation::paths::appsup()).join("mods"),
    }
}

/// Loads the mods for the world `save_id` and installs the host if any mod loaded. Without the `mods` feature,
/// with `ModsConfig.disabled`, or without a mods directory this does nothing at all.
pub fn start(save_id: Option<&str>, server: &Rc<BHServer>) {
    install(None);
    let dir = mods_dir();
    if config().disabled || !dir.is_dir() {
        return;
    }
    #[cfg(feature = "mods")]
    install(server_host::load(&dir, save_id, server));
    #[cfg(not(feature = "mods"))]
    let _ = (save_id, server);
}

/// Server stop: disables every mod and removes the host.
pub fn stop() {
    if let Some(h) = hooks() {
        h.server_stop();
    }
    install(None);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_and_dir_resolution() {
        set_config(ModsConfig { dir: Some(PathBuf::from("/x/mods")), disabled: false });
        assert_eq!(mods_dir(), PathBuf::from("/x/mods"));
        set_config(ModsConfig::default());
        assert_eq!(config(), ModsConfig::default());
        // Tests run single-threaded (.cargo/config.toml).
        std::env::set_var("BLOCKHEADS_MODS_DIR", "/y/mods");
        assert_eq!(mods_dir(), PathBuf::from("/y/mods"));
        std::env::set_var("BLOCKHEADS_MODS_DIR", "");
        assert!(mods_dir().ends_with("TheBlockheads/mods"), "{:?}", mods_dir());
        std::env::remove_var("BLOCKHEADS_MODS_DIR");
        assert!(mods_dir().ends_with("mods"));
    }

    #[test]
    fn actor_guards_nest() {
        assert_eq!(actor(), None);
        {
            let _a = act_as("a");
            assert_eq!(actor().as_deref(), Some("a"));
            {
                let _b = act_as("b");
                assert_eq!(actor().as_deref(), Some("b"));
            }
            assert_eq!(actor().as_deref(), Some("a"));
        }
        assert_eq!(actor(), None);
    }

    #[test]
    fn stop_without_hooks_is_harmless() {
        install(None);
        stop();
        assert!(hooks().is_none());
    }
}
