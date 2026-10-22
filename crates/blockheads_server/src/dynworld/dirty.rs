//! The "dirty position" trackers of `DynamicWorld` and the active-blockhead accessors.
//!
//! `worldChangedAtPos:sendReliably:`, `worldContentsChangedAtPos:`, `waterChangedAtPos:fullBlock:`,
//! `dynamicWorldChangedAtPos:objectType:`, `exploreLightChangedAtMacroPos:clientLightBlockIndex:`,
//! `lightChangedAtMacroPos:sendReliably:` (both), `snowChangedAtMacroPos:`, `activeBlockhead`,
//! `activeBlockheadIndex`, `selectedBlockheadChanged:`.
//!
//! All lists are insertion-ordered `Vec<IntPair>` (never sets): the consumers drain them in
//! order. `macroOf(pos)` uses C truncating division (`/`, never `div_euclid`).

use std::cell::RefCell;

use super::state::{DynamicWorld, MAX_DYNAMIC_OBJECT_TYPES};
use crate::objects::handle::ObjRef;
use crate::objects::type_tables::object_type_is_interaction_object;
use crate::types::structs::IntPair;

/// `macroOf(pos) = makeIntpair(pos.x / 32, pos.y / 32)` (C truncating division).
#[inline]
pub(crate) fn macro_of(pos: IntPair) -> IntPair {
    IntPair { x: pos.x / 32, y: pos.y / 32 }
}

/// Linear membership test over both coordinates.
#[inline]
fn vec_contains(v: &[IntPair], p: IntPair) -> bool {
    v.iter().any(|e| e.x == p.x && e.y == p.y)
}

/// `ADD_UNIQUE(vec, v)`: push when absent.
#[inline]
pub(crate) fn add_unique(v: &RefCell<Vec<IntPair>>, p: IntPair) {
    let mut v = v.borrow_mut();
    if !vec_contains(&v, p) {
        v.push(p);
    }
}

/// Erase the FIRST element equal to `p` (`std::find` + `erase`).
#[inline]
fn erase_first(v: &RefCell<Vec<IntPair>>, p: IntPair) {
    let mut v = v.borrow_mut();
    if let Some(i) = v.iter().position(|e| e.x == p.x && e.y == p.y) {
        v.remove(i);
    }
}

impl DynamicWorld {
    /// `-worldChangedAtPos:sendReliably:`. Reliable wins; an already-reliable macro stays reliable.
    pub fn world_changed_at_pos_send_reliably(&self, pos: IntPair, send_reliably: bool) {
        add_unique(&self.world_changed_positions, pos);
        let m = macro_of(pos);
        let already_reliable = vec_contains(&self.world_changed_macro_positions.borrow(), m);
        if !already_reliable {
            if !send_reliably {
                add_unique(&self.world_changed_send_unreliably_macro_positions, m);
            } else {
                self.world_changed_macro_positions.borrow_mut().push(m);
                erase_first(&self.world_changed_send_unreliably_macro_positions, m);
            }
        }
        // Mods (not in the original; `None` without mods): `block-changed`, attributed to the requesting player.
        if let Some(h) = crate::mods::hooks().filter(|h| h.wants(crate::mods::Optional::BlockChanged)) {
            let tile =
                self.world.upgrade().and_then(|w| crate::world::tiles::tile_at_world_position_loaded(pos.x, pos.y, &w));
            let tile = tile.map_or(0, |t| t.type_index() as u32);
            h.block_changed(pos, tile, crate::mods::actor().as_deref());
        }
    }

    /// `-worldContentsChangedAtPos:`: pushes to the RELIABLE macro list, never touches the unreliable one.
    pub fn world_contents_changed_at_pos(&self, pos: IntPair) {
        add_unique(&self.world_contents_changed_positions, pos);
        add_unique(&self.world_changed_macro_positions, macro_of(pos));
    }

    /// `-waterChangedAtPos:fullBlock:`. `full_block` gates only the position list; the
    /// `reloadDrawBlockWaterForTile` call is a no-op on the server and omitted.
    pub fn water_changed_at_pos_full_block(&self, pos: IntPair, full_block: bool) {
        if full_block {
            add_unique(&self.water_changed_positions, pos);
        }
        let m = macro_of(pos);
        let reliable = vec_contains(&self.world_changed_macro_positions.borrow(), m);
        if !reliable {
            add_unique(&self.world_changed_send_unreliably_macro_positions, m);
        }
    }

    /// `-dynamicWorldChangedAtPos:objectType:`. Types 22 and 29 return at once; for interaction-object
    /// types the BLOCKHEAD list (24) is also updated, only when the macro was newly added to the object's own list.
    pub fn dynamic_world_changed_at_pos_object_type(&self, pos_world: IntPair, object_type: i32) {
        if object_type == 22 || object_type == 29 {
            return;
        }
        // UB-PARITY: out-of-range indices index foreign memory in the original; ignored here.
        if !(0..MAX_DYNAMIC_OBJECT_TYPES as i32).contains(&object_type) {
            return;
        }
        let m = macro_of(pos_world);
        let newly = {
            let mut lists = self.dynamic_world_changed_macro_positions.borrow_mut();
            let list = &mut lists[object_type as usize];
            if vec_contains(list, m) {
                false
            } else {
                list.push(m);
                true
            }
        };
        if newly && object_type_is_interaction_object(object_type) {
            self.dynamic_world_changed_at_pos_object_type(pos_world, 24);
        }
    }

    /// `-exploreLightChangedAtMacroPos:clientLightBlockIndex:`. `idx` must be -1 or 0..31; out of range
    /// is UB in the original (`debug_assert!`, then ignored).
    pub fn explore_light_changed_at_macro_pos_client_light_block_index(&self, macro_pos: IntPair, idx: i32) {
        if idx == -1 {
            self.light_changed_at_macro_pos_send_reliably(macro_pos, false);
        } else {
            debug_assert!((0..32).contains(&idx), "clientLightBlockIndex out of range");
            if (0..32).contains(&idx) {
                let mut lists = self.light_changed_send_unreliably_macro_positions_single_client.borrow_mut();
                let list = &mut lists[idx as usize];
                if !vec_contains(list, macro_pos) {
                    list.push(macro_pos);
                }
            }
        }
    }

    /// `-lightChangedAtMacroPos:sendReliably:`: `sendAtAll = YES`.
    pub fn light_changed_at_macro_pos_send_reliably(&self, macro_pos: IntPair, r: bool) {
        self.light_changed_at_macro_pos_send_reliably_send_at_all(macro_pos, r, true);
    }

    /// `-lightChangedAtMacroPos:sendReliably:sendAtAll:`. `send_at_all == false` routes to
    /// `worldChangedDontSendMacroPositions` only when the macro is not already in the unreliable list.
    pub fn light_changed_at_macro_pos_send_reliably_send_at_all(&self, macro_pos: IntPair, r: bool, send_at_all: bool) {
        let reliable = vec_contains(&self.world_changed_macro_positions.borrow(), macro_pos);
        if reliable {
            return;
        }
        if !r {
            let in_unrel = vec_contains(&self.world_changed_send_unreliably_macro_positions.borrow(), macro_pos);
            if !in_unrel {
                if !send_at_all {
                    add_unique(&self.world_changed_dont_send_macro_positions, macro_pos);
                } else {
                    self.world_changed_send_unreliably_macro_positions.borrow_mut().push(macro_pos);
                }
            }
        } else {
            self.world_changed_macro_positions.borrow_mut().push(macro_pos);
            erase_first(&self.world_changed_send_unreliably_macro_positions, macro_pos);
        }
    }

    /// `-snowChangedAtMacroPos:`.
    pub fn snow_changed_at_macro_pos(&self, macro_pos: IntPair) {
        add_unique(&self.snow_changed_macro_positions, macro_pos);
    }

    /// `-activeBlockhead`: `(uint64)(int64)i < count` (a negative index is out of range).
    pub fn active_blockhead(&self) -> Option<ObjRef> {
        let i = self.active_blockhead_index.get();
        let bh = self.blockheads.borrow();
        if (i as i64 as u64) < bh.len() as u64 {
            Some(bh[i as usize].clone())
        } else {
            None
        }
    }

    /// `-activeBlockheadIndex`.
    pub fn active_blockhead_index(&self) -> i32 {
        self.active_blockhead_index.get()
    }

    /// `-selectedBlockheadChanged:`: out of range (unsigned compare) becomes 0.
    pub fn selected_blockhead_changed(&self, new_index: i32) {
        let n = self.blockheads.borrow().len() as u64;
        self.active_blockhead_index.set(if (new_index as i64 as u64) < n { new_index } else { 0 });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dynworld::state::testutil::*;
    use crate::objects::handle::ObjRef;
    use std::cell::RefCell;
    use std::rc::Rc;

    fn p(x: i32, y: i32) -> IntPair {
        IntPair { x, y }
    }
    fn pv(v: &RefCell<Vec<IntPair>>) -> Vec<(i32, i32)> {
        v.borrow().iter().map(|e| (e.x, e.y)).collect()
    }

    /// world_changed sequence, truncating macro division.
    #[test]
    fn world_changed_reliability() {
        let d = dw("dirty_wc", 2);
        let pos = p(40, 5);
        d.world_changed_at_pos_send_reliably(pos, false);
        assert_eq!(pv(&d.world_changed_send_unreliably_macro_positions), vec![(1, 0)]);
        d.world_changed_at_pos_send_reliably(pos, true);
        assert!(d.world_changed_send_unreliably_macro_positions.borrow().is_empty());
        assert_eq!(pv(&d.world_changed_macro_positions), vec![(1, 0)]);
        d.world_changed_at_pos_send_reliably(pos, false);
        assert_eq!(pv(&d.world_changed_positions), vec![(40, 5)]);
        assert_eq!(pv(&d.world_changed_macro_positions), vec![(1, 0)]);
        assert!(d.world_changed_send_unreliably_macro_positions.borrow().is_empty());
        // truncating division: (-1, 5) -> macro (0, 0)
        let d = dw("dirty_wc2", 2);
        d.world_changed_at_pos_send_reliably(p(-1, 5), true);
        assert_eq!(pv(&d.world_changed_macro_positions), vec![(0, 0)]);
    }

    /// reliable add erases only the FIRST equal element of the unreliable list.
    #[test]
    fn reliable_erases_first_only() {
        let d = dw("dirty_first", 2);
        d.world_changed_send_unreliably_macro_positions.borrow_mut().extend([p(1, 1), p(2, 2), p(1, 1)]);
        d.world_changed_at_pos_send_reliably(p(32, 32), true);
        assert_eq!(pv(&d.world_changed_send_unreliably_macro_positions), vec![(2, 2), (1, 1)]);
    }

    #[test]
    fn contents_and_water() {
        let d = dw("dirty_cw", 2);
        d.world_contents_changed_at_pos(p(33, 1));
        d.world_contents_changed_at_pos(p(33, 1));
        assert_eq!(pv(&d.world_contents_changed_positions), vec![(33, 1)]);
        assert_eq!(pv(&d.world_changed_macro_positions), vec![(1, 0)]);
        assert!(d.world_changed_send_unreliably_macro_positions.borrow().is_empty());
        d.water_changed_at_pos_full_block(p(33, 1), true);
        // macro already reliable: unreliable list untouched
        assert!(d.world_changed_send_unreliably_macro_positions.borrow().is_empty());
        assert_eq!(pv(&d.water_changed_positions), vec![(33, 1)]);
        d.water_changed_at_pos_full_block(p(70, 1), false);
        assert_eq!(pv(&d.water_changed_positions), vec![(33, 1)]);
        assert_eq!(pv(&d.world_changed_send_unreliably_macro_positions), vec![(2, 0)]);
    }

    /// dynamic_world_changed type 46 also fills list 24 once; 22 and 29 add nothing.
    #[test]
    fn dynamic_world_changed_lists() {
        let d = dw("dirty_dwc", 2);
        d.dynamic_world_changed_at_pos_object_type(p(70, 3), 46);
        d.dynamic_world_changed_at_pos_object_type(p(71, 3), 46);
        {
            let l = d.dynamic_world_changed_macro_positions.borrow();
            assert_eq!(l[46].len(), 1);
            assert_eq!(l[24].len(), 1);
        }
        d.dynamic_world_changed_at_pos_object_type(p(70, 3), 22);
        d.dynamic_world_changed_at_pos_object_type(p(70, 3), 29);
        let l = d.dynamic_world_changed_macro_positions.borrow();
        assert!(l[22].is_empty() && l[29].is_empty());
    }

    /// light trackers.
    #[test]
    fn light_trackers() {
        let d = dw("dirty_light", 2);
        let m = p(1, 1);
        d.light_changed_at_macro_pos_send_reliably_send_at_all(m, false, false);
        assert_eq!(pv(&d.world_changed_dont_send_macro_positions), vec![(1, 1)]);
        d.light_changed_at_macro_pos_send_reliably_send_at_all(m, false, true);
        assert_eq!(pv(&d.world_changed_send_unreliably_macro_positions), vec![(1, 1)]);
        // already unreliable and !sendAtAll: no new dont-send entry
        d.light_changed_at_macro_pos_send_reliably_send_at_all(p(9, 9), false, false);
        d.light_changed_at_macro_pos_send_reliably_send_at_all(m, true, true);
        assert_eq!(pv(&d.world_changed_macro_positions), vec![(1, 1)]);
        assert!(d.world_changed_send_unreliably_macro_positions.borrow().is_empty());
        d.explore_light_changed_at_macro_pos_client_light_block_index(p(5, 5), 3);
        d.explore_light_changed_at_macro_pos_client_light_block_index(p(5, 5), 3);
        assert_eq!(d.light_changed_send_unreliably_macro_positions_single_client.borrow()[3].len(), 1);
        d.explore_light_changed_at_macro_pos_client_light_block_index(p(6, 6), -1);
        assert_eq!(pv(&d.world_changed_send_unreliably_macro_positions), vec![(6, 6)]);
        d.snow_changed_at_macro_pos(p(2, 2));
        d.snow_changed_at_macro_pos(p(2, 2));
        assert_eq!(pv(&d.snow_changed_macro_positions), vec![(2, 2)]);
    }

    /// active blockhead accessors use an unsigned compare.
    #[test]
    fn active_blockhead_bounds() {
        let d = dw("dirty_active", 1);
        let log = Rc::new(RefCell::new(vec![]));
        let a: ObjRef = Obj::new(24, 1, log.clone());
        let b: ObjRef = Obj::new(24, 2, log);
        d.blockheads.borrow_mut().push(a);
        d.blockheads.borrow_mut().push(b);
        d.selected_blockhead_changed(1);
        assert_eq!(d.active_blockhead_index(), 1);
        d.selected_blockhead_changed(-1);
        assert_eq!(d.active_blockhead_index(), 0);
        d.selected_blockhead_changed(2);
        assert_eq!(d.active_blockhead_index(), 0);
        d.active_blockhead_index.set(-1);
        assert!(d.active_blockhead().is_none());
        d.active_blockhead_index.set(1);
        assert_eq!(d.active_blockhead().unwrap().unique_id(), 2);
    }
}
