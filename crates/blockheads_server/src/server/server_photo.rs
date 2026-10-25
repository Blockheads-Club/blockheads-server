//! The server's own chat avatar (not in the original).
//!
//! The client shows a chat line's icon from the sender's player info (`info[@"photo"]`, falling back to the
//! generic face). Lines from the server carry the player id of the local player, so when a photo is configured the
//! server gets a local player info that carries it and the clients receive it with the player list, like any other
//! player's photo. `None` (the default) is exactly the vanilla behaviour: no local player info at all.
//!
//! Like the mods configuration it lives in a thread-local: every server runs on its own thread.

use std::cell::RefCell;

thread_local! {
    static PHOTO: RefCell<Option<Vec<u8>>> = const { RefCell::new(None) };
}

/// Set (or clear) the PNG/JPEG bytes of the server's avatar for the server of this thread.
pub fn set(photo: Option<Vec<u8>>) {
    PHOTO.with(|p| *p.borrow_mut() = photo);
}

/// The configured avatar bytes (default none, which is vanilla).
pub fn get() -> Option<Vec<u8>> {
    PHOTO.with(|p| p.borrow().clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn none_by_default_and_per_thread() {
        assert_eq!(get(), None);
        set(Some(vec![1, 2]));
        assert_eq!(get(), Some(vec![1, 2]));
        assert_eq!(std::thread::spawn(get).join().unwrap(), None);
        set(None);
        assert_eq!(get(), None);
    }
}
