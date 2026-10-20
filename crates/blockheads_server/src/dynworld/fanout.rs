//! Per-type fan-out iterations, the debug log and the free-block spatial index.
//!
//! The six `reload*ForMacroTile:` selectors are empty stubs and are omitted.

use super::lookups::{ops, TRAIN_CAR_DYNAMIC_OBJECT_TYPES};
use super::state::{DynamicWorld, MAX_DYNAMIC_OBJECT_TYPES};
use crate::objects::handle::ObjRef;
use crate::objects::type_tables::DYNAMIC_OBJECT_TYPE_PAINTING;
use crate::types::structs::IntPair;
use crate::world::helpers::object_type_may_have_artifical_light;

impl DynamicWorld {
    /// `-railOrStationNameChanged`: train cars 41..44 (`dynamicObjects` only).
    pub fn rail_or_station_name_changed(&self) {
        let hooks = ops();
        for t in TRAIN_CAR_DYNAMIC_OBJECT_TYPES {
            for o in self.dot_snapshot(t) {
                hooks.train_car_rail_or_station_name_changed(&o);
            }
        }
    }

    /// `-blockheadWillBeUnloaded:`: every object of every type, `dynamicObjects` then `ToAdd`.
    pub fn blockhead_will_be_unloaded(&self, b: &ObjRef) {
        for i in 0..MAX_DYNAMIC_OBJECT_TYPES as i32 {
            for o in self.dot_snapshot(i) {
                o.blockhead_unloaded(&o, b);
            }
            for o in self.add_snapshot(i) {
                o.blockhead_unloaded(&o, b);
            }
        }
    }

    /// `-userMuteChanged:`: paintings (`dynamicObjects[52]`), no server check.
    pub fn user_mute_changed(&self, pid: &str) {
        let hooks = ops();
        for o in self.dot_snapshot(DYNAMIC_OBJECT_TYPE_PAINTING) {
            hooks.painting_user_mute_changed(&o, pid);
        }
    }

    /// `-userBanChanged:isBanned:`: only when `server != nil`.
    pub fn user_ban_changed_is_banned(&self, pid: &str, banned: bool) {
        if !self.has_server() {
            return;
        }
        let hooks = ops();
        for o in self.dot_snapshot(DYNAMIC_OBJECT_TYPE_PAINTING) {
            hooks.painting_user_ban_changed(&o, pid, banned);
        }
    }

    /// `-playersChanged`: `updateNameTextView` on every net blockhead (client UI text).
    pub fn players_changed(&self) {
        let hooks = ops();
        let list: Vec<ObjRef> = self.net_blockheads.borrow().clone();
        for bh in list {
            hooks.blockhead_update_name_text_view(&bh);
        }
    }

    /// `-addArtificialLightContributionForPhysicalBlockLoadedAtXPos:yPos:`: light-capable types only.
    pub fn add_artificial_light_contribution_for_physical_block_loaded_at_x_pos_y_pos(&self, mx: i32, my: i32) {
        for i in 0..MAX_DYNAMIC_OBJECT_TYPES as i32 {
            if object_type_may_have_artifical_light(i) {
                for o in self.dot_snapshot(i) {
                    o.add_artificial_light_contribution_for_physical_block_loaded_at_x_pos_y_pos(&o, mx, my);
                }
                for o in self.add_snapshot(i) {
                    o.add_artificial_light_contribution_for_physical_block_loaded_at_x_pos_y_pos(&o, mx, my);
                }
            }
        }
    }

    /// `-appendDebugLog:`: per type the class name of the object with the smallest uid and the count of
    /// `dynamicObjects[i]` (not `ToAdd`).
    pub fn append_debug_log(&self, log: &mut String) {
        let mut total: i32 = 0;
        for i in 0..MAX_DYNAMIC_OBJECT_TYPES {
            let (count, first) = {
                let m = self.dynamic_objects.borrow();
                (m[i].len() as i32, m[i].values().next().cloned())
            };
            if count > 0 {
                total = total.wrapping_add(count);
                if let Some(f) = first {
                    log.push_str(&format!("{}:{}\n", f.class_info().name, count));
                }
            }
        }
        log.push_str(&format!("total objects:{}\n\n", total));
        log.push_str(&format!("local blockheads:{}\n", self.blockheads.borrow().len()));
        log.push_str(&format!("netBlockheads:{}\n", self.net_blockheads.borrow().len()));
        log.push_str(&format!(
            "netBlockheadsWithDisconnectedClients:{}\n\n",
            self.net_blockheads_with_disconnected_clients.borrow().len()
        ));
    }

    /// `-freeblockPositionChanged:oldPos:`: erase from the old bucket (dropping an emptied bucket), insert
    /// into the bucket of the new position.
    pub fn freeblock_position_changed_old_pos(&self, o: &ObjRef, old: IntPair) {
        let old_idx = self.world_index_key(old);
        self.free_blocks_erase(old_idx, o);
        let new_idx = self.world_index_key(o.pos());
        self.free_blocks_insert(new_idx, o);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dynworld::lookups::{set_ops_hooks, DwOpsHooks};
    use crate::dynworld::state::testutil::*;
    use std::cell::RefCell;
    use std::rc::Rc;

    fn obj(ty: i32, id: u64, x: i32, y: i32) -> ObjRef {
        let o: ObjRef = Obj::new(ty, id, Rc::new(RefCell::new(vec![])));
        o.base().borrow_mut().pos = IntPair { x, y };
        o
    }

    /// the free-block index moves the entry and erases an emptied bucket.
    #[test]
    fn freeblock_index_moves() {
        let d = dw("fo_fb", 2);
        let fb = obj(14, 5, 3, 3);
        let k_old = d.world_index_key(IntPair { x: 3, y: 3 });
        d.free_blocks_insert(k_old, &fb);
        fb.base().borrow_mut().pos = IntPair { x: 4, y: 3 };
        d.freeblock_position_changed_old_pos(&fb, IntPair { x: 3, y: 3 });
        assert!(!d.free_blocks_by_position.borrow().contains_key(&k_old));
        assert!(d.free_blocks_exist_at_pos(IntPair { x: 4, y: 3 }));
        assert!(!d.free_blocks_exist_at_pos(IntPair { x: 3, y: 3 }));
    }

    #[test]
    fn debug_log_format() {
        let d = dw("fo_log", 1);
        d.do_set(17, 4, obj(17, 4, 0, 0));
        d.do_set(17, 2, obj(17, 2, 0, 0));
        d.ta_set(19, 9, obj(19, 9, 0, 0)); // ToAdd not counted
        d.blockheads.borrow_mut().push(obj(24, 1, 0, 0));
        let mut log = String::new();
        d.append_debug_log(&mut log);
        assert_eq!(
            log,
            "DynamicObject:2\ntotal objects:2\n\nlocal blockheads:1\nnetBlockheads:0\nnetBlockheadsWithDisconnectedClients:0\n\n"
        );
    }

    #[test]
    fn mute_and_ban_fan_out() {
        struct O(RefCell<Vec<String>>);
        impl DwOpsHooks for O {
            fn painting_user_mute_changed(&self, p: &ObjRef, pid: &str) {
                self.0.borrow_mut().push(format!("mute{}:{}", p.unique_id(), pid));
            }
            fn painting_user_ban_changed(&self, p: &ObjRef, pid: &str, b: bool) {
                self.0.borrow_mut().push(format!("ban{}:{}:{}", p.unique_id(), pid, b));
            }
        }
        let d = dw("fo_mute", 1);
        let o = Rc::new(O(RefCell::new(vec![])));
        set_ops_hooks(o.clone());
        d.do_set(52, 6, obj(52, 6, 0, 0));
        d.user_mute_changed("p");
        d.user_ban_changed_is_banned("p", true); // no server: ignored
        assert_eq!(*o.0.borrow(), vec!["mute6:p".to_string()]);
    }
}
