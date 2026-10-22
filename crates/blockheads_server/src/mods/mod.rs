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
}

thread_local! {
    static HOOKS: RefCell<Option<Rc<dyn ModHooks>>> = const { RefCell::new(None) };
    static CONFIG: RefCell<ModsConfig> = RefCell::new(ModsConfig::default());
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
    fn stop_without_hooks_is_harmless() {
        install(None);
        stop();
        assert!(hooks().is_none());
    }
}
