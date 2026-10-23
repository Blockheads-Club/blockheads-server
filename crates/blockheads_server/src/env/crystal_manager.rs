//! `CrystalManager`: the premium crystal count singleton (inert on the dedicated server, but gates TradePortal
//! and crystal mining through its MD5 token scheme).
//!
//!  Class `CrystalManager`";
//! sections 3.6, 5 and 6 (R8) and 7 item 12.

use std::cell::RefCell;
use std::rc::{Rc, Weak};

use crate::foundation::md5::string_from_md5;

/// `crystalCountChanged:` receiver (`countWatcher`, assign, always nil on the server).
pub trait CrystalCountWatcher {
    /// `-[countWatcher crystalCountChanged:]`
    fn crystal_count_changed(&self, changed: bool);
}

/// CrystalManager ivars (saveQueue @0x08 is never used: nothing is ever enqueued, R8).
pub struct CrystalManagerState {
    /// `crystalCount` (0x10)
    pub crystal_count: i32,
    /// `amountString` (0x18): MD5 hex of the count, nil until the first successful `modify:`.
    pub amount_string: Option<String>,
    /// `countWatcher` (0x20)
    pub count_watcher: Option<Weak<dyn CrystalCountWatcher>>,
    /// `needsSave` (0x28)
    pub needs_save: bool,
}

/// `CrystalManager: NSObject` singleton (instance size 0x30).
pub struct CrystalManager {
    st: RefCell<CrystalManagerState>,
}

thread_local! {
    static INSTANCE: RefCell<Option<Rc<CrystalManager>>> = const { RefCell::new(None) };
}

impl CrystalManager {
    /// `+[CrystalManager instance]`: lazy, not thread-safe in the original.
    pub fn instance() -> Rc<CrystalManager> {
        INSTANCE.with(|c| {
            let mut c = c.borrow_mut();
            if c.is_none() {
                *c = Some(Rc::new(CrystalManager::new_uninstanced()));
            }
            c.as_ref().unwrap().clone()
        })
    }

    /// A fresh manager (`init`: `crystalCount` 0, `amountString` nil). Not the singleton (tests).
    pub fn new_uninstanced() -> CrystalManager {
        CrystalManager {
            st: RefCell::new(CrystalManagerState {
                crystal_count: 0,
                amount_string: None,
                count_watcher: None,
                needs_save: false,
            }),
        }
    }

    /// `-loadFromSave`: empty.
    pub fn load_from_save(&self) {}
    /// `-amount` = `crystalCount`
    pub fn amount(&self) -> i32 {
        self.st.borrow().crystal_count
    }
    /// `-amountString`
    pub fn amount_string(&self) -> Option<String> {
        self.st.borrow().amount_string.clone()
    }
    /// `-commitSaveIfNeeded`: empty.
    pub fn commit_save_if_needed(&self) {}
    /// `-save`: `needsSave = YES`; nothing is written.
    pub fn save(&self) {
        self.st.borrow_mut().needs_save = true;
    }

    /// `-modify:modifyString:`: spend `count` crystals (negative credits) when `modify_string` equals
    /// `md5hex("7acfe93afc08c%d65ae2c54ecaf07f" % ((crystalCount - count) + 0x49))`; otherwise nothing happens.
    pub fn modify_modify_string(&self, count: i32, modify_string: &str) {
        let cur = self.st.borrow().crystal_count;
        let expected =
            string_from_md5(&format!("7acfe93afc08c{}65ae2c54ecaf07f", cur.wrapping_sub(count).wrapping_add(0x49)));
        if expected == modify_string {
            let new_count = cur.wrapping_sub(count);
            {
                let mut st = self.st.borrow_mut();
                st.crystal_count = new_count;
                st.amount_string = Some(string_from_md5(&format!("7acfe93afc08{}c65ae2c54ecaf07f", new_count)));
            }
            let watcher = self.count_watcher();
            if let Some(w) = watcher {
                w.crystal_count_changed(true);
            }
            self.save();
        }
    }

    /// `-iCloudID`: the literal `"unspported"` (sic).
    pub fn i_cloud_id(&self) -> &'static str {
        "unspported"
    }
    /// `-iCloudServerRejoinID` = `md5hex(iCloudID + "rejoin")`.
    pub fn i_cloud_server_rejoin_id(&self) -> String {
        string_from_md5(&format!("{}rejoin", self.i_cloud_id()))
    }
    /// `-countWatcher`
    pub fn count_watcher(&self) -> Option<Rc<dyn CrystalCountWatcher>> {
        self.st.borrow().count_watcher.as_ref().and_then(|w| w.upgrade())
    }
    /// `-setCountWatcher:` (assign, not retained)
    pub fn set_count_watcher(&self, w: Option<&Rc<dyn CrystalCountWatcher>>) {
        self.st.borrow_mut().count_watcher = w.map(Rc::downgrade);
    }
    /// `-needsSave`
    pub fn needs_save(&self) -> bool {
        self.st.borrow().needs_save
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    fn token(count_after: i32) -> String {
        string_from_md5(&format!("7acfe93afc08c{}65ae2c54ecaf07f", count_after.wrapping_add(73)))
    }

    /// item 12 md5 vectors and the credit / spend sequence.
    #[test]
    fn md5_and_modify() {
        let m = CrystalManager::new_uninstanced();
        m.load_from_save();
        m.commit_save_if_needed();
        assert_eq!(m.i_cloud_id(), "unspported");
        assert_eq!(m.i_cloud_server_rejoin_id(), "e0512522237df73051c70deabaccbe34");
        assert_eq!(m.amount(), 0);
        assert_eq!(m.amount_string(), None);
        assert!(!m.needs_save());
        // wrong token: nothing at all
        m.modify_modify_string(-5, "deadbeef");
        assert_eq!((m.amount(), m.amount_string(), m.needs_save()), (0, None, false));
        // credit 5: (0 - (-5)) + 73 = 78
        assert_eq!(token(5), string_from_md5("7acfe93afc08c7865ae2c54ecaf07f"));
        m.modify_modify_string(-5, &token(5));
        assert_eq!(m.amount(), 5);
        assert_eq!(m.amount_string(), Some(string_from_md5("7acfe93afc085c65ae2c54ecaf07f")));
        assert!(m.needs_save());
        // spend 3 with the token for 5-3+73
        m.modify_modify_string(3, &token(2));
        assert_eq!(m.amount(), 2);
        assert_eq!(m.amount_string(), Some(string_from_md5("7acfe93afc082c65ae2c54ecaf07f")));
        // replaying the old token does nothing
        m.modify_modify_string(3, &token(2));
        assert_eq!(m.amount(), 2);
    }

    struct W(Cell<u32>, Cell<bool>);
    impl CrystalCountWatcher for W {
        fn crystal_count_changed(&self, changed: bool) {
            self.0.set(self.0.get() + 1);
            self.1.set(changed);
        }
    }

    #[test]
    fn watcher_and_singleton() {
        let m = CrystalManager::new_uninstanced();
        let w = Rc::new(W(Cell::new(0), Cell::new(false)));
        let wd: Rc<dyn CrystalCountWatcher> = w.clone();
        m.set_count_watcher(Some(&wd));
        m.modify_modify_string(-1, &token(1));
        assert_eq!((w.0.get(), w.1.get()), (1, true));
        m.set_count_watcher(None);
        assert!(m.count_watcher().is_none());
        assert!(Rc::ptr_eq(&CrystalManager::instance(), &CrystalManager::instance()));
        // i32 wrap
        let m2 = CrystalManager::new_uninstanced();
        m2.st.borrow_mut().crystal_count = i32::MIN;
        m2.modify_modify_string(1, &token(i32::MIN.wrapping_sub(1)));
        assert_eq!(m2.amount(), i32::MAX);
    }
}
