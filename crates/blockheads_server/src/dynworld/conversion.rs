//! Version 1 -> 2 save layout conversion, emulated synchronously instead of by a background thread.
//!
//! `### mainThreadRemoveDirFromConversionList:`; Part A (the trigger);
//! `loadDynamicObjectsForMacroTile` (B) first block (on-demand conversion, in `load.rs`).

use std::collections::VecDeque;

use super::state::DynamicWorld;
use crate::foundation::clock::reference_time;
use crate::foundation::gsdict::GsDict;
use crate::foundation::paths::append_path_component;

/// Seconds the original thread sleeps before each directory (`[NSThread sleepForTimeInterval:0.5]`).
pub const CONVERSION_STEP_SECONDS: f64 = 0.5;

impl DynamicWorld {
    /// `-conversionThread:` entry: copy the list (`[arg copy]`, GsDict order preserved) and arm the first step at
    /// `now + 0.5`. The thread's `setThreadPriority:` and autorelease pools have no equivalent.
    pub fn conversion_thread(&self, list: GsDict<String, GsDict<String, String>>) {
        let entries: VecDeque<(String, GsDict<String, String>)> =
            list.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
        *self.conversion_snapshot.borrow_mut() = Some(entries);
        self.conversion_next_due.set(reference_time() + CONVERSION_STEP_SECONDS);
    }

    /// `-mainThreadRemoveDirFromConversionList:`: remove the directory; when the list becomes empty it is released
    /// (set to nil), which is what makes `saveGame` write `saveVersion` 8 again.
    pub fn main_thread_remove_dir_from_conversion_list(&self, dir_name: &str) {
        let mut l = self.version_one_to_two_conversion_list.borrow_mut();
        if let Some(list) = l.as_mut() {
            list.remove(&dir_name.to_string());
            if list.is_empty() {
                *l = None;
            }
        }
    }

    /// Advance the emulated thread by at most one directory when `reference_time() >= conversion_next_due`.
    /// A cheap no-op when no conversion is running. Must be called at the start of
    /// `update_accurate_dt_is_simulation` and from `World::pre_update` (request recorded in the.
    pub fn conversion_pump(&self) {
        if self.conversion_snapshot.borrow().is_none() {
            return;
        }
        if reference_time() < self.conversion_next_due.get() {
            return;
        }
        // `if (self->versionOneToTwoConversionList == nil) return;` (aborted)
        if self.version_one_to_two_conversion_list.borrow().is_none() {
            *self.conversion_snapshot.borrow_mut() = None;
            return;
        }
        let next = self.conversion_snapshot.borrow_mut().as_mut().and_then(|q| q.pop_front());
        let Some((dir_name, macro_objects)) = next else {
            *self.conversion_snapshot.borrow_mut() = None;
            return;
        };
        let dir_path = append_path_component(&self.world_save_directory, &dir_name);
        let _ = std::fs::create_dir_all(&dir_path);
        // rename() errors are ignored
        for (old, new) in macro_objects.iter() {
            let _ = std::fs::rename(old, new);
        }
        self.main_thread_remove_dir_from_conversion_list(&dir_name);
        self.conversion_next_due.set(self.conversion_next_due.get() + CONVERSION_STEP_SECONDS);
        if self.conversion_snapshot.borrow().as_ref().is_some_and(|q| q.is_empty()) {
            *self.conversion_snapshot.borrow_mut() = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dynworld::state::testutil::dw;

    /// contract 8 item 15: the list stays non-nil until the last directory is converted; files are moved.
    #[test]
    fn converts_one_dir_per_step() {
        let _cl = crate::foundation::clock::TEST_CLOCK_LOCK.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        crate::foundation::clock::set_virtual(Some(1000.0));
        let d = dw("conv", 1);
        let base = d.world_save_directory.clone();
        let old1 = format!("{base}3_4_dynamicWorld_1");
        let old2 = format!("{base}5_6_dynamicWorld_1");
        std::fs::write(&old1, b"a").unwrap();
        std::fs::write(&old2, b"b").unwrap();
        let mut m1 = GsDict::new();
        m1.insert(old1.clone(), format!("{base}3_4/dynamicWorld"));
        let mut m2 = GsDict::new();
        m2.insert(old2.clone(), format!("{base}5_6/dynamicWorld"));
        let mut list = GsDict::new();
        list.insert("3_4".to_string(), m1);
        list.insert("5_6".to_string(), m2);
        *d.version_one_to_two_conversion_list.borrow_mut() = Some(list.clone());
        d.conversion_thread(list);
        d.conversion_pump();
        assert!(d.version_one_to_two_conversion_list.borrow().is_some());
        crate::foundation::clock::advance_virtual(0.6);
        d.conversion_pump();
        assert_eq!(d.version_one_to_two_conversion_list.borrow().as_ref().unwrap().len(), 1);
        crate::foundation::clock::advance_virtual(0.5);
        d.conversion_pump();
        assert!(d.version_one_to_two_conversion_list.borrow().is_none());
        assert!(std::path::Path::new(&format!("{base}3_4/dynamicWorld")).exists());
        assert!(std::path::Path::new(&format!("{base}5_6/dynamicWorld")).exists());
        assert!(d.conversion_snapshot.borrow().is_none());
    }

    /// The pump gives up when the list or the queue is gone; removing from a missing list is harmless.
    #[test]
    fn pump_gives_up_without_work() {
        let _cl = crate::foundation::clock::TEST_CLOCK_LOCK.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        crate::foundation::clock::set_virtual(Some(2000.0));
        let d = dw("conv_idle", 1);
        d.main_thread_remove_dir_from_conversion_list("0_0");
        assert!(d.version_one_to_two_conversion_list.borrow().is_none());
        // a snapshot but no list: aborted
        *d.conversion_snapshot.borrow_mut() = Some(VecDeque::new());
        d.conversion_next_due.set(f64::MIN);
        d.conversion_pump();
        assert!(d.conversion_snapshot.borrow().is_none());
        // a list but an exhausted snapshot: finished
        let mut list = GsDict::new();
        list.insert("1_1".to_string(), GsDict::new());
        *d.version_one_to_two_conversion_list.borrow_mut() = Some(list);
        *d.conversion_snapshot.borrow_mut() = Some(VecDeque::new());
        d.conversion_pump();
        assert!(d.conversion_snapshot.borrow().is_none());
        crate::foundation::clock::set_virtual(None);
    }
}
