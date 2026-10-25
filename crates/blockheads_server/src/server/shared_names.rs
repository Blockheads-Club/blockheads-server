//! The "same name from another device" setting (not in the original).
//!
//! Vanilla refuses a join whose name is already bound to another device: while that name is online the join is
//! rejected with "user logged in" (2), and when the name belongs to a known player on another device with
//! "username taken" (1). With this setting on the name alone is the identity, like a Minecraft offline-mode
//! server: the join is accepted and plays as that player (the player id is the MD5 of the lower-cased name, so
//! blockheads, inventory, ownership and the admin/mod/whitelist/ban state all follow the name), and a session
//! that is online under that name is replaced by the new one. Off is exactly the vanilla behaviour.
//!
//! Like the mods configuration it lives in a thread-local: every server runs on its own thread.

use std::cell::Cell;

thread_local! {
    static ENABLED: Cell<bool> = const { Cell::new(false) };
}

/// What a session that was replaced by a newer login under its name is told.
pub const TAKEOVER_TEXT: &str = "You logged in from another location.";

/// Turn the setting on or off for the server of this thread.
pub fn set_enabled(on: bool) {
    ENABLED.with(|e| e.set(on));
}

/// Whether the setting is on (default off, which is vanilla).
pub fn enabled() -> bool {
    ENABLED.with(Cell::get)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn off_by_default_and_per_thread() {
        assert!(!enabled());
        set_enabled(true);
        assert!(enabled());
        assert!(!std::thread::spawn(enabled).join().unwrap());
        set_enabled(false);
        assert!(!enabled());
    }
}
