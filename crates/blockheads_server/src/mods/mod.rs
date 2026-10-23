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

use crate::plist::value::{PlistDict, PlistValue};
use crate::server::bhserver::BHServer;
use crate::types::structs::IntPair;

#[cfg(feature = "mods")]
pub mod catalog;
#[cfg(feature = "mods")]
pub mod net;
#[cfg(feature = "mods")]
mod server_host;

/// The 0xBC mod protocol packet type (mod-sdk `docs/protocol-0xbc.md`). Only routed when a mod host is installed.
pub const MOD_PACKET: u8 = 0xBC;

/// How long a vanilla client waits in the readable-rejection limbo (see [`ModHooks::limbo_join`]) before the server
/// drops it (no `0x26`).
pub const LIMBO_GRACE: std::time::Duration = std::time::Duration::from_secs(20);

/// The `welcomeMessage` the vanilla client shows (as "WELCOME TO <WORLD>!") while it is in the limbo.
pub const LIMBO_TEXT: &str = "This server uses Blockheads Club mods. Install the mod loader with Patcher \
(blockheads.club/patcher), then join again. Your worlds stay.";

/// The `bhcModLoader` dict a modded client adds to its 0x1f player information.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LoaderKey {
    /// `proto` (an integer).
    pub proto: Option<i64>,
    /// `acceptedHash` (64 lowercase hex characters).
    pub accepted_hash: Option<String>,
}

impl LoaderKey {
    /// Reads `bhcModLoader` from a 0x1f plist. `None` when the key is absent; a key of another type reads as an
    /// empty (invalid) key. The key is never copied into the stored player info (only listed keys are).
    pub fn from_player_info(info: Option<&PlistDict>) -> Option<LoaderKey> {
        let v = info?.map.get(&"bhcModLoader".to_string())?;
        let PlistValue::Dict(d) = v else { return Some(LoaderKey::default()) };
        let get = |k: &str| d.map.get(&k.to_string());
        Some(LoaderKey {
            proto: match get("proto") {
                Some(PlistValue::Int(n)) => Some(*n),
                _ => None,
            },
            accepted_hash: get("acceptedHash").and_then(|v| v.as_str()).map(str::to_string),
        })
    }
}

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
    /// A 0xBC packet from a peer that is in the game or waiting for approval (`peer` as the dispatcher names it).
    fn mod_packet(&self, peer: &str, data: &[u8]);
    /// A player passed every vanilla join check: the `bhcModLoader` check, then `player-pre-login` (API 0.2).
    /// `peer` is the dispatcher's name of the connection, `loader` the 0x1f `bhcModLoader` key. `Some(reason)`:
    /// reject with that 0x26 reason byte (a modded client already got `REJECT_TEXT`).
    fn pre_login(&self, peer: &str, player_id: &str, alias: &str, loader: Option<&LoaderKey>) -> Option<u8>;
    /// Called at the same place as [`ModHooks::pre_login`], before it. True: the peer is a vanilla client (no
    /// `HELLO`, no `bhcModLoader`) on a server with client-required mods. The host then holds it in a limbo for
    /// [`LIMBO_GRACE`]: the caller sends a world summary carrying [`LIMBO_TEXT`] (the vanilla game shows it as the
    /// welcome popup), does not approve the player and returns. `pre_login` is not called for it.
    fn limbo_join(&self, peer: &str, player_id: &str, loader: Option<&LoaderKey>) -> bool;
    /// A packet arrived from `peer`. True when the peer was in the limbo: it is dropped now (no `0x26`) and the
    /// packet must be ignored.
    fn limbo_packet(&self, peer: &str) -> bool;
    /// `peer` disconnected or failed: forgets it if it was in the limbo.
    fn limbo_gone(&self, peer: &str);
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

/// Factor applied to the mod host's time budgets (soft, hard, per-step), from `BLOCKHEADS_MODS_BUDGET_SCALE`.
/// The budgets are wall-clock limits, so a starved or instrumented process (CI, coverage builds, a shared test
/// machine) can trip them with perfectly healthy mods; test setups raise the scale. Unset, empty, unparsable or
/// zero: 1, the production budgets.
pub fn budget_scale() -> u32 {
    parse_scale(std::env::var("BLOCKHEADS_MODS_BUDGET_SCALE").ok().as_deref())
}

fn parse_scale(v: Option<&str>) -> u32 {
    v.and_then(|s| s.trim().parse::<u32>().ok()).filter(|n| *n > 0).unwrap_or(1)
}

/// JVM mods are unsandboxed code, so they are off unless the owner asks: `BLOCKHEADS_JVM` set to `1`, `true`, `yes` or
/// `on` (feature `jvm` only). `BLOCKHEADS_JAVA_HOME` picks the JRE (default: `JAVA_HOME`, then `java` on the `PATH`)
/// and `BLOCKHEADS_JVM_HEAP_MB` the heap limit of the JVM (default 256).
#[cfg(feature = "jvm")]
pub fn jvm_settings(var: &dyn Fn(&str) -> Option<String>) -> Option<bhmod_core::jvm::JvmConfig> {
    let on = var("BLOCKHEADS_JVM")
        .is_some_and(|v| matches!(v.trim().to_ascii_lowercase().as_str(), "1" | "true" | "yes" | "on"));
    on.then(|| {
        let mut cfg = bhmod_core::jvm::JvmConfig::default();
        cfg.java_home = var("BLOCKHEADS_JAVA_HOME").filter(|v| !v.is_empty()).map(PathBuf::from);
        cfg.max_heap_mb = var("BLOCKHEADS_JVM_HEAP_MB")
            .and_then(|v| v.trim().parse().ok())
            .filter(|n| *n > 0)
            .unwrap_or(cfg.max_heap_mb);
        // The grace periods scale with the budgets (slow test machines).
        let scale = budget_scale();
        cfg.grace *= scale;
        cfg.enable_grace *= scale;
        cfg
    })
}

/// The SDK jar of the JVM runtime (`blockheads-mod-sdk.jar`): what mods compile against.
#[cfg(feature = "jvm")]
pub use bhmod_core::jvm::SDK_JAR as JVM_SDK_JAR;

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

    #[cfg(feature = "jvm")]
    #[test]
    fn jvm_is_opt_in() {
        let env =
            |pairs: &'static [(&str, &str)]| move |k: &str| pairs.iter().find(|p| p.0 == k).map(|p| p.1.to_string());
        assert!(jvm_settings(&env(&[])).is_none());
        assert!(jvm_settings(&env(&[("BLOCKHEADS_JVM", "0")])).is_none());
        assert!(jvm_settings(&env(&[("BLOCKHEADS_JVM", "maybe")])).is_none());
        for on in ["1", "true", " Yes ", "ON"] {
            let cfg = jvm_settings(&|k| (k == "BLOCKHEADS_JVM").then(|| on.to_string())).expect(on);
            assert_eq!(cfg.java_home, None);
            assert_eq!(cfg.max_heap_mb, 256);
        }
        let cfg = jvm_settings(&env(&[
            ("BLOCKHEADS_JVM", "1"),
            ("BLOCKHEADS_JAVA_HOME", "/opt/jre"),
            ("BLOCKHEADS_JVM_HEAP_MB", "512"),
        ]))
        .unwrap();
        assert_eq!(cfg.java_home, Some(PathBuf::from("/opt/jre")));
        assert_eq!(cfg.max_heap_mb, 512);
        // Empty or unusable values fall back to the defaults.
        let cfg = jvm_settings(&env(&[
            ("BLOCKHEADS_JVM", "1"),
            ("BLOCKHEADS_JAVA_HOME", ""),
            ("BLOCKHEADS_JVM_HEAP_MB", "0"),
        ]))
        .unwrap();
        assert_eq!((cfg.java_home, cfg.max_heap_mb), (None, 256));
        assert_eq!(
            jvm_settings(&env(&[("BLOCKHEADS_JVM", "1"), ("BLOCKHEADS_JVM_HEAP_MB", "x")])).unwrap().max_heap_mb,
            256
        );
        assert!(!JVM_SDK_JAR.is_empty());
    }

    #[test]
    fn budget_scale_parsing() {
        assert_eq!(parse_scale(None), 1);
        assert_eq!(parse_scale(Some("")), 1);
        assert_eq!(parse_scale(Some("x")), 1);
        assert_eq!(parse_scale(Some("0")), 1);
        assert_eq!(parse_scale(Some(" 20 ")), 20);
        // Tests run single-threaded (.cargo/config.toml); put the configured value back afterwards.
        let saved = std::env::var_os("BLOCKHEADS_MODS_BUDGET_SCALE");
        std::env::set_var("BLOCKHEADS_MODS_BUDGET_SCALE", "7");
        assert_eq!(budget_scale(), 7);
        std::env::remove_var("BLOCKHEADS_MODS_BUDGET_SCALE");
        assert_eq!(budget_scale(), 1);
        if let Some(v) = saved {
            std::env::set_var("BLOCKHEADS_MODS_BUDGET_SCALE", v);
        }
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
    fn loader_key_from_player_info() {
        use crate::foundation::gsdict::GsDict;
        let dict = |v: Option<PlistValue>| {
            let mut m: GsDict<String, PlistValue> = GsDict::new();
            m.insert("alias".into(), PlistValue::string("A".to_string()));
            if let Some(v) = v {
                m.insert("bhcModLoader".into(), v);
            }
            PlistDict { map: m, tag: Default::default() }
        };
        assert_eq!(LoaderKey::from_player_info(None), None);
        assert_eq!(LoaderKey::from_player_info(Some(&dict(None))), None);
        let not_a_dict = dict(Some(PlistValue::string("x".to_string())));
        assert_eq!(LoaderKey::from_player_info(Some(&not_a_dict)), Some(LoaderKey::default()));
        let mut k: GsDict<String, PlistValue> = GsDict::new();
        k.insert("proto".into(), PlistValue::string("1".to_string()));
        k.insert("acceptedHash".into(), PlistValue::string("ab".to_string()));
        let key = LoaderKey::from_player_info(Some(&dict(Some(PlistValue::dict(k))))).unwrap();
        assert_eq!(key, LoaderKey { proto: None, accepted_hash: Some("ab".into()) });
        let mut k: GsDict<String, PlistValue> = GsDict::new();
        k.insert("proto".into(), PlistValue::Int(1));
        let key = LoaderKey::from_player_info(Some(&dict(Some(PlistValue::dict(k))))).unwrap();
        assert_eq!(key, LoaderKey { proto: Some(1), accepted_hash: None });
    }

    #[test]
    fn stop_without_hooks_is_harmless() {
        install(None);
        stop();
        assert!(hooks().is_none());
    }
}
