//! `GatherBlock` (DynamicObject type 26, instance size 0x58): resets a half-gathered tile after 20 s.
//!
//! sections 3.5 and 7 item 10.

use std::cell::RefCell;
use std::rc::{Rc, Weak};

use crate::lighting::artificial_light::services;
use crate::objects::base::{
    base_get_save_dict, base_init_with_world_dynamic_world_at_position_cache,
    base_init_with_world_dynamic_world_save_dict_cache, DynamicObject, DynamicObjectBase, DynamicWorldRef, WorldRef,
};
use crate::objects::class_info::{ClassInfo, DYNAMIC_OBJECT_CLASS};
use crate::objects::handle::{self, CacheRef, ObjRef};
use crate::plist::value::{PlistDict, PlistValue};
use crate::types::structs::IntPair;
use crate::world::state::World;
use crate::world::tiles::{macro_pos_for_world_pos, tile_at_world_position_loaded};

/// `GatherBlock` class descriptor.
pub static GATHER_BLOCK_CLASS: ClassInfo =
    ClassInfo { name: "GatherBlock", superclass: Some(&DYNAMIC_OBJECT_CLASS), protocols: &[] };

/// `DYNAMIC_OBJECT_TYPE_GATHERBLOCK`
pub const DYNAMIC_OBJECT_TYPE_GATHERBLOCK: i32 = 26;

/// GatherBlock ivars 0x50..0x58.
#[derive(Default)]
pub struct GatherBlockState {
    /// `timer` (0x50): seconds since the last `gatherProgress` change.
    pub timer: f32,
    /// `lastKnownGatherValue` (0x54)
    pub last_known_gather_value: i32,
}

/// `GatherBlock: DynamicObject` (type 26).
pub struct GatherBlock {
    base: RefCell<DynamicObjectBase>,
    /// The GatherBlock ivars.
    pub st: RefCell<GatherBlockState>,
    world: Weak<World>,
}

impl GatherBlock {
    fn alloc(world: &Rc<World>) -> Rc<GatherBlock> {
        let w = Rc::downgrade(world);
        handle::alloc_obj(|sw| GatherBlock {
            base: RefCell::new(DynamicObjectBase::alloc_zeroed_with(sw)),
            st: RefCell::new(GatherBlockState::default()),
            world: w,
        })
    }

    /// Position-spawn factory (`loadStandardDynamicObjectOfType:26 atPos:`): base init only; timer and
    /// `lastKnownGatherValue` stay 0.
    pub fn init_with_world_dynamic_world_at_position_cache(
        world: &Rc<World>,
        dw: &DynamicWorldRef,
        pos: IntPair,
        cache: CacheRef,
    ) -> Option<Rc<GatherBlock>> {
        let me = Self::alloc(world);
        let me_obj: ObjRef = me.clone();
        let wref: WorldRef = world.clone();
        // The base init always succeeds for a new object (it ignores a failed registration).
        let _ = base_init_with_world_dynamic_world_at_position_cache(&me_obj, &wref, dw, pos, cache);
        Some(me)
    }

    /// `-[GatherBlock initWithWorld:dynamicWorld:saveDict:cache:]`.
    pub fn init_with_world_dynamic_world_save_dict_cache(
        world: &Rc<World>,
        dw: &DynamicWorldRef,
        save_dict: &PlistDict,
        cache: CacheRef,
    ) -> Option<Rc<GatherBlock>> {
        let me = Self::alloc(world);
        let me_obj: ObjRef = me.clone();
        let wref: WorldRef = world.clone();
        // The base init always succeeds for a saved object.
        let _ = base_init_with_world_dynamic_world_save_dict_cache(&me_obj, &wref, dw, save_dict, cache);
        {
            let mut st = me.st.borrow_mut();
            st.timer = save_dict.get(&"timer".to_string()).map_or(0.0, |v| v.float_value());
            st.last_known_gather_value =
                save_dict.get(&"lastKnownGatherValue".to_string()).map_or(0, |v| v.int_value());
        }
        Some(me)
    }
}

impl GatherBlock {
    /// `-[DynamicObject initWithWorld:dynamicWorld:cache:netData:]` inherited unchanged (GatherBlock defines no net init; object_registry A3).
    /// Zero-state species ivars + the DynamicObject net init. `None` when the base init yields nil.
    pub fn init_with_world_dynamic_world_cache_net_data(
        world: &Rc<World>,
        dw: &DynamicWorldRef,
        cache: CacheRef,
        net_data: &[u8],
    ) -> Option<Rc<GatherBlock>> {
        let me = Self::alloc(world);
        let me_obj: ObjRef = me.clone();
        let wref: WorldRef = world.clone();
        if !crate::objects::base::base_init_with_world_dynamic_world_cache_net_data(&me_obj, &wref, dw, cache, net_data)
        {
            return None;
        }
        Some(me)
    }
}

impl DynamicObject for GatherBlock {
    fn base(&self) -> &RefCell<DynamicObjectBase> {
        &self.base
    }
    fn class_info(&self) -> &'static ClassInfo {
        &GATHER_BLOCK_CLASS
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn object_type(&self) -> i32 {
        DYNAMIC_OBJECT_TYPE_GATHERBLOCK
    }
    fn get_save_dict(&self, me: &ObjRef) -> PlistDict {
        let mut d = base_get_save_dict(me);
        let st = self.st.borrow();
        d.map.insert("timer".to_string(), PlistValue::real_f32(st.timer));
        d.map.insert("lastKnownGatherValue".to_string(), PlistValue::int(st.last_known_gather_value as i64));
        d
    }
    /// `accurateDT` / `isSimulation` unused.
    fn update_accurate_dt_is_simulation(&self, me: &ObjRef, dt: f32, _accurate_dt: f32, _is_simulation: bool) {
        let Some(world) = self.world.upgrade() else { return };
        let pos = self.base.borrow().pos;
        let Some(tile) = tile_at_world_position_loaded(pos.x, pos.y, &world) else { return };
        let progress = tile.gather_progress() as i32;
        if progress != self.st.borrow().last_known_gather_value {
            let mut st = self.st.borrow_mut();
            st.timer = 0.0;
            st.last_known_gather_value = progress;
        }
        let last = self.st.borrow().last_known_gather_value;
        if last == 255 || last == 0 {
            self.set_needs_removed(me, true);
        } else {
            let timer = {
                let mut st = self.st.borrow_mut();
                st.timer += dt;
                st.timer
            };
            if 20.0f32 < timer {
                tile.set_gather_progress(0);
                // reloadDrawBlockGeometryForTile: empty on the server
                services().light_changed_at_macro_pos_send_reliably(macro_pos_for_world_pos(pos, &world), true);
                self.set_needs_removed(me, true);
            }
        }
    }
    fn world_changed(&self, me: &ObjRef, positions: &mut Vec<IntPair>) {
        let pos = self.base.borrow().pos;
        if !positions.iter().any(|p| *p == pos) {
            return;
        }
        let Some(world) = self.world.upgrade() else { return };
        // Note: NULL is not checked in the original (crash); an unloaded tile is treated as "nothing to do".
        let Some(tile) = tile_at_world_position_loaded(pos.x, pos.y, &world) else { return };
        let gp = tile.gather_progress();
        if gp != 255 && gp != 0 {
            return;
        }
        self.set_needs_removed(me, true);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lighting::artificial_light::testkit::*;

    #[test]
    fn timer_and_reset() {
        let rec = install_rec();
        let w = world_with_blocks(4, &[(0, 0), (1, 0), (0, 1), (1, 1)]);
        let (_d, dwr) = test_dw();
        let g = GatherBlock::init_with_world_dynamic_world_at_position_cache(&w, &dwr, IntPair { x: 40, y: 40 }, None)
            .unwrap();
        let me: ObjRef = g.clone();
        assert_eq!(me.object_type(), 26);
        // progress 0 with last 0 -> removal
        me.update_accurate_dt_is_simulation(&me, 0.1, 0.1, false);
        assert!(me.needs_removed());
        // a fresh block with an in-progress tile
        let g2 = GatherBlock::init_with_world_dynamic_world_at_position_cache(&w, &dwr, IntPair { x: 41, y: 40 }, None)
            .unwrap();
        let m2: ObjRef = g2.clone();
        tile(&w, 41, 40).update(|t| t.gather_progress = 100);
        m2.update_accurate_dt_is_simulation(&m2, 10.0, 10.0, false);
        assert_eq!(g2.st.borrow().timer, 10.0);
        assert_eq!(g2.st.borrow().last_known_gather_value, 100);
        m2.update_accurate_dt_is_simulation(&m2, 10.0, 10.0, false);
        // 20.0 < 20.0 is false (strict)
        assert!(!m2.needs_removed());
        rec.take();
        m2.update_accurate_dt_is_simulation(&m2, 0.5, 0.5, false);
        assert!(m2.needs_removed());
        assert_eq!(tile(&w, 41, 40).get().gather_progress, 0);
        assert!(rec.take().contains(&"lightChanged(1,1,true)".to_string()));
        // progress change resets the timer
        let g3 = GatherBlock::init_with_world_dynamic_world_at_position_cache(&w, &dwr, IntPair { x: 42, y: 40 }, None)
            .unwrap();
        let m3: ObjRef = g3.clone();
        tile(&w, 42, 40).update(|t| t.gather_progress = 5);
        m3.update_accurate_dt_is_simulation(&m3, 15.0, 15.0, false);
        tile(&w, 42, 40).update(|t| t.gather_progress = 6);
        m3.update_accurate_dt_is_simulation(&m3, 1.0, 1.0, false);
        assert_eq!(g3.st.borrow().timer, 1.0);
        // 255 -> removal; worldChanged only reacts to its own position
        tile(&w, 42, 40).update(|t| t.gather_progress = 255);
        m3.world_changed(&m3, &mut vec![IntPair { x: 43, y: 40 }]);
        assert!(!m3.needs_removed());
        m3.world_changed(&m3, &mut vec![IntPair { x: 42, y: 40 }]);
        assert!(m3.needs_removed());
    }

    #[test]
    fn save_dict() {
        install_rec();
        let w = world_with_blocks(4, &[(0, 0), (1, 0), (0, 1), (1, 1)]);
        let (_d, dwr) = test_dw();
        let g = GatherBlock::init_with_world_dynamic_world_at_position_cache(&w, &dwr, IntPair { x: 40, y: 40 }, None)
            .unwrap();
        g.st.borrow_mut().timer = 2.5;
        g.st.borrow_mut().last_known_gather_value = 77;
        let me: ObjRef = g.clone();
        let d = me.get_save_dict(&me);
        let g2 = GatherBlock::init_with_world_dynamic_world_save_dict_cache(&w, &dwr, &d, None).unwrap();
        assert_eq!(g2.st.borrow().timer, 2.5);
        assert_eq!(g2.st.borrow().last_known_gather_value, 77);
    }

    #[test]
    fn inherited_net_init() {
        install_rec();
        let w = world_with_blocks(4, &[(0, 0), (1, 0), (0, 1), (1, 1)]);
        let (_d, dwr) = test_dw();
        let mut nd = vec![0u8; 0x18];
        nd[0..8].copy_from_slice(&1234u64.to_le_bytes());
        nd[8..12].copy_from_slice(&40i32.to_le_bytes());
        nd[12..16].copy_from_slice(&40i32.to_le_bytes());
        let g = GatherBlock::init_with_world_dynamic_world_cache_net_data(&w, &dwr, None, &nd).unwrap();
        let me: ObjRef = g.clone();
        assert_eq!(me.object_type(), 26);
        assert_eq!(me.pos(), IntPair { x: 40, y: 40 });
        assert_eq!(me.base().borrow().unique_id, 1234);
        assert!(me.base().borrow().is_net);
        // zero-state species ivars
        assert_eq!(g.st.borrow().timer, 0.0);
        assert_eq!(g.st.borrow().last_known_gather_value, 0);
    }

    #[test]
    fn network_init_class_and_the_unloaded_tile() {
        install_rec();
        let w = world_with_blocks(4, &[(0, 0), (1, 0), (0, 1), (1, 1)]);
        let (_d, dwr) = test_dw();
        let at = |x: u32, y: u32| {
            let nd =
                crate::types::netdata::DynamicObjectNetData { unique_id: 5, pos_x: x, pos_y: y, ..Default::default() };
            crate::types::structs::WireStruct::to_vec(&nd)
        };
        let g = GatherBlock::init_with_world_dynamic_world_cache_net_data(&w, &dwr, None, &at(40, 40)).unwrap();
        let me: ObjRef = g.clone();
        assert_eq!(me.class_info().name, "GatherBlock");
        assert!(me.as_any().is::<GatherBlock>());
        assert!(GatherBlock::init_with_world_dynamic_world_cache_net_data(&w, &dwr, None, &at(100, 100)).is_none());
        // A change at a position nobody loaded is ignored.
        let far =
            GatherBlock::init_with_world_dynamic_world_at_position_cache(&w, &dwr, IntPair { x: 100, y: 100 }, None)
                .unwrap();
        let far_obj: ObjRef = far.clone();
        far_obj.world_changed(&far_obj, &mut vec![IntPair { x: 100, y: 100 }]);
        assert!(!far_obj.needs_removed());
        // A gather in progress survives a change at its position.
        let busy =
            GatherBlock::init_with_world_dynamic_world_at_position_cache(&w, &dwr, IntPair { x: 41, y: 41 }, None)
                .unwrap();
        let busy_obj: ObjRef = busy.clone();
        tile(&w, 41, 41).update(|t| t.gather_progress = 100);
        busy_obj.world_changed(&busy_obj, &mut vec![IntPair { x: 41, y: 41 }]);
        assert!(!busy_obj.needs_removed());
        // A world that is gone ignores changes too.
        let orphan = GatherBlock::alloc(&w);
        drop(w);
        let orphan: ObjRef = orphan;
        orphan.world_changed(&orphan, &mut vec![IntPair { x: 0, y: 0 }]);
    }
}
