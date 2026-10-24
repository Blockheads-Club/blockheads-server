//! `Ladder: DynamicObject` (DynamicObjectType 19).
//!
//! Client-only members (`draw:`, `reloadDrawBlock*`, render ivars) are not ported.

use std::cell::RefCell;
use std::rc::Rc;

use super::column::{
    append_plist_tail, drop_as_free_block, owner_from_client_or_save, read_plist_tail, sd_get, sd_string, tail_string,
    Links,
};
use crate::dynworld::state::DynamicWorld;
use crate::objects::base::{
    base_dynamic_object_net_data, base_get_save_dict, base_init_with_world_dynamic_world_at_position_cache,
    base_init_with_world_dynamic_world_cache_net_data, base_init_with_world_dynamic_world_save_dict_cache,
    padded_bytes, DynamicObject, DynamicObjectBase, DynamicWorldRef, WorldRef,
};
use crate::objects::class_info::{ClassInfo, DYNAMIC_OBJECT_CLASS};
use crate::objects::handle::{self, CacheRef, ObjRef};
use crate::plist::value::{PlistDict, PlistValue};
use crate::types::netdata::LadderCreationNetData;
use crate::types::structs::{IntPair, WireStruct};
use crate::world::helpers::tile_is_water;
use crate::world::state::World;
use crate::world::tiles::tile_at_world_position_loaded;

/// `DYNAMIC_OBJECT_TYPE_LADDER`.
pub const DYNAMIC_OBJECT_TYPE_LADDER: i32 = 19;
/// The item type (53) for which `worldChanged:` reacts to water.
pub const LADDER_WATER_SENSITIVE_ITEM: i32 = 53;

/// `Ladder` class descriptor.
pub static LADDER_CLASS: ClassInfo =
    ClassInfo { name: "Ladder", superclass: Some(&DYNAMIC_OBJECT_CLASS), protocols: &[] };

/// Ladder ivars.
pub struct LadderState {
    pub item_type: i32,
    pub paint_color: u16,
}

/// `Ladder: DynamicObject` (type 19).
pub struct Ladder {
    base: RefCell<DynamicObjectBase>,
    /// The Ladder ivars.
    pub st: RefCell<LadderState>,
    links: Links,
}

impl Ladder {
    fn alloc(world: &Rc<World>, dw: &Rc<DynamicWorld>) -> Rc<Ladder> {
        let links = Links::new(world, dw);
        handle::alloc_obj(|sw| Ladder {
            base: RefCell::new(DynamicObjectBase::alloc_zeroed_with(sw)),
            st: RefCell::new(LadderState { item_type: 0, paint_color: 0 }),
            links,
        })
    }

    /// Placement constructor. Order: base, itemType, paintColor, owner, `initSubDerivedItems` (empty on the server).
    pub fn init_with_world_dynamic_world_at_position_cache_type_save_dict_placed_by_client(
        world: &Rc<World>,
        dw: &Rc<DynamicWorld>,
        pos: IntPair,
        cache: CacheRef,
        item_type: i32,
        save_dict: Option<&PlistDict>,
        client_id: Option<&str>,
    ) -> Option<ObjRef> {
        let me = Self::alloc(world, dw);
        let me_obj: ObjRef = me.clone();
        let (wref, dref): (WorldRef, DynamicWorldRef) = (world.clone(), dw.clone());
        // `base_init_with_world_dynamic_world_at_position_cache` answers true on every path
        let _ = base_init_with_world_dynamic_world_at_position_cache(&me_obj, &wref, &dref, pos, cache);
        {
            let mut st = me.st.borrow_mut();
            st.item_type = item_type;
            st.paint_color = sd_get(save_dict, "paintColor").map_or(0, PlistValue::unsigned_int_value) as u16;
        }
        if let Some(o) = owner_from_client_or_save(client_id, save_dict) {
            me.base.borrow_mut().owner_id = Some(o);
        }
        Some(me_obj)
    }

    /// Save constructor.
    pub fn init_with_world_dynamic_world_save_dict_cache(
        world: &Rc<World>,
        dw: &Rc<DynamicWorld>,
        save_dict: &PlistDict,
        cache: CacheRef,
    ) -> Option<ObjRef> {
        let me = Self::alloc(world, dw);
        let me_obj: ObjRef = me.clone();
        let (wref, dref): (WorldRef, DynamicWorldRef) = (world.clone(), dw.clone());
        // `base_init_with_world_dynamic_world_save_dict_cache` answers true on every path
        let _ = base_init_with_world_dynamic_world_save_dict_cache(&me_obj, &wref, &dref, save_dict, cache);
        {
            let mut st = me.st.borrow_mut();
            st.item_type = save_dict.map.get(&"itemType".to_string()).map_or(0, |v| v.int_value());
            st.paint_color = save_dict.map.get(&"paintColor".to_string()).map_or(0, |v| v.unsigned_int_value()) as u16;
        }
        me.base.borrow_mut().owner_id = sd_string(save_dict, "ownerID");
        Some(me_obj)
    }

    /// Net constructor.
    pub fn init_with_world_dynamic_world_cache_net_data(
        world: &Rc<World>,
        dw: &Rc<DynamicWorld>,
        cache: CacheRef,
        net_data: &[u8],
    ) -> Option<ObjRef> {
        let me = Self::alloc(world, dw);
        let me_obj: ObjRef = me.clone();
        let (wref, dref): (WorldRef, DynamicWorldRef) = (world.clone(), dw.clone());
        if !base_init_with_world_dynamic_world_cache_net_data(&me_obj, &wref, &dref, cache, net_data) {
            return None;
        }
        let s = LadderCreationNetData::read_le(&padded_bytes(net_data, 0x20));
        {
            let mut st = me.st.borrow_mut();
            st.item_type = s.item_type as i32;
            st.paint_color = s.paint_color;
        }
        if net_data.len() > 0x20 {
            let owner = tail_string(&read_plist_tail(net_data, 0x20), "ownerID");
            me.base.borrow_mut().owner_id = owner;
        }
        Some(me_obj)
    }

    /// `-[Ladder itemType]`
    pub fn item_type(&self) -> i32 {
        self.st.borrow().item_type
    }

    /// `-[Ladder paint:]`.
    pub fn paint(&self, _me: &ObjRef, color_index: u16) {
        self.st.borrow_mut().paint_color = color_index;
        if !self.base.borrow().is_net {
            self.base.borrow_mut().update_needs_to_be_sent = true;
            let pos = self.base.borrow().pos;
            self.links.dw().dynamic_world_changed_at_pos_object_type(pos, DYNAMIC_OBJECT_TYPE_LADDER);
        }
    }
}

impl DynamicObject for Ladder {
    fn base(&self) -> &RefCell<DynamicObjectBase> {
        &self.base
    }
    fn class_info(&self) -> &'static ClassInfo {
        &LADDER_CLASS
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn object_type(&self) -> i32 {
        DYNAMIC_OBJECT_TYPE_LADDER
    }
    fn occupies_foreground_contents(&self) -> bool {
        true
    }
    fn freeblock_creation_item_type(&self) -> i32 {
        self.item_type()
    }
    fn free_block_creation_save_dict(&self, me: &ObjRef) -> Option<PlistDict> {
        Some(self.get_save_dict(me))
    }

    fn get_save_dict(&self, me: &ObjRef) -> PlistDict {
        let mut d = base_get_save_dict(me);
        let (it, pc) = {
            let st = self.st.borrow();
            (st.item_type, st.paint_color)
        };
        d.map.insert("itemType".to_string(), PlistValue::int(it as i64));
        d.map.insert("paintColor".to_string(), PlistValue::int(pc as i64));
        if let Some(o) = self.base.borrow().owner_id.clone() {
            d.map.insert("ownerID".to_string(), PlistValue::string(o));
        }
        d
    }

    fn creation_net_data_for_client(&self, me: &ObjRef, _client_id: Option<&str>) -> Option<Vec<u8>> {
        let st = self.st.borrow();
        let s = LadderCreationNetData {
            dynamic_object_net_data: base_dynamic_object_net_data(me),
            item_type: st.item_type as u16,
            paint_color: st.paint_color,
            padding: [0; 4],
        };
        let mut out = s.to_vec();
        let owner = self.base.borrow().owner_id.clone();
        append_plist_tail(&mut out, &[("ownerID", owner.as_deref())]);
        Some(out)
    }
    fn update_net_data_for_client(&self, me: &ObjRef, client_id: Option<&str>) -> Option<Vec<u8>> {
        self.creation_net_data_for_client(me, client_id)
    }

    /// `-[Ladder worldChanged:]`: only item 53; exact position match, no wrap.
    fn world_changed(&self, me: &ObjRef, positions: &mut Vec<IntPair>) {
        if self.item_type() != LADDER_WATER_SENSITIVE_ITEM {
            return;
        }
        let world = self.links.world();
        let pos = self.base.borrow().pos;
        for p in positions.iter() {
            if p.x == pos.x && p.y == pos.y && tile_is_water(tile_at_world_position_loaded(pos.x, pos.y, &world)) {
                if self.base.borrow().needs_removed {
                    return;
                }
                let sd = self.get_save_dict(me);
                drop_as_free_block(me, &self.links, pos, self.item_type(), Some(sd));
                return;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::column::testkit::*;
    use super::*;

    fn mk(w: &Rc<World>, d: &Rc<DynamicWorld>, x: i32, y: i32, item: i32, owner: Option<&str>) -> ObjRef {
        Ladder::init_with_world_dynamic_world_at_position_cache_type_save_dict_placed_by_client(
            w,
            d,
            pos(x, y),
            None,
            item,
            None,
            owner,
        )
        .unwrap()
    }

    #[test]
    fn water_drops_only_item_53_at_exact_pos() {
        let (w, d) = env();
        let l = mk(&w, &d, 40, 20, 53, Some("o"));
        tile(&w, 40, 20).set_foreground_contents(0x42);
        // water elsewhere: nothing
        l.world_changed(&l, &mut vec![pos(41, 20)]);
        assert!(!l.needs_removed());
        // water at the ladder position but the tile is not water: continue
        l.world_changed(&l, &mut vec![pos(40, 20)]);
        assert!(!l.needs_removed());
        tile(&w, 40, 20).set_type_index(3);
        l.world_changed(&l, &mut vec![pos(40, 20)]);
        assert!(l.needs_removed());
        assert_eq!(tile(&w, 40, 20).foreground_contents(), 0);
        // other item types ignore worldChanged
        let l2 = mk(&w, &d, 50, 20, 54, None);
        tile(&w, 50, 20).set_type_index(3);
        l2.world_changed(&l2, &mut vec![pos(50, 20)]);
        assert!(!l2.needs_removed());
    }

    #[test]
    fn save_net_round_trip_and_paint() {
        let (w, d) = env();
        let l = mk(&w, &d, 40, 20, 53, Some("own"));
        handle::downcast::<Ladder>(&l).unwrap().paint(&l, 7);
        assert!(l.update_needs_to_be_sent());
        let sd = l.get_save_dict(&l);
        assert_eq!(sd.map.len(), 7);
        assert_eq!(sd.map.get(&"paintColor".to_string()).unwrap().unsigned_int_value(), 7);
        let l2 = Ladder::init_with_world_dynamic_world_save_dict_cache(&w, &d, &sd, None).unwrap();
        assert_eq!(handle::downcast::<Ladder>(&l2).unwrap().st.borrow().paint_color, 7);
        assert_eq!(l2.owner_id().as_deref(), Some("own"));
        let b = l.creation_net_data_for_client(&l, None).unwrap();
        assert_eq!(&b[0x1a..0x1c], &7u16.to_le_bytes());
        assert!(b[0x1c..0x20].iter().all(|x| *x == 0));
        let l3 = Ladder::init_with_world_dynamic_world_cache_net_data(&w, &d, None, &b).unwrap();
        assert!(l3.is_net());
        assert_eq!(l3.owner_id().as_deref(), Some("own"));
        // isNet paint does not flag an update
        handle::downcast::<Ladder>(&l3).unwrap().paint(&l3, 3);
        assert!(!l3.update_needs_to_be_sent());
        assert!(l.occupies_foreground_contents());
        assert!(l.free_block_creation_save_dict(&l).is_some());
    }
}

#[cfg(test)]
mod more_tests {
    use super::super::column::testkit::*;
    use super::*;

    fn mk(w: &Rc<World>, d: &Rc<DynamicWorld>, x: i32, y: i32, item: i32) -> ObjRef {
        Ladder::init_with_world_dynamic_world_at_position_cache_type_save_dict_placed_by_client(
            w,
            d,
            pos(x, y),
            None,
            item,
            None,
            None,
        )
        .unwrap()
    }

    #[test]
    fn trait_answers_and_water_sensitive_ladders() {
        let (w, d) = env();
        let l = mk(&w, &d, 40, 20, LADDER_WATER_SENSITIVE_ITEM);
        let _ = l.class_info();
        assert_eq!(l.freeblock_creation_item_type(), LADDER_WATER_SENSITIVE_ITEM);
        let net = l.update_net_data_for_client(&l, None).unwrap();
        let mut lost = net.clone();
        lost[12..16].copy_from_slice(&300u32.to_le_bytes());
        assert!(Ladder::init_with_world_dynamic_world_cache_net_data(&w, &d, None, &lost).is_none());
        // other ladders ignore the water
        let other = mk(&w, &d, 50, 20, LADDER_WATER_SENSITIVE_ITEM + 1);
        tile(&w, 50, 20).set_type_index(3);
        other.world_changed(&other, &mut vec![pos(50, 20)]);
        assert!(!other.needs_removed());
        // being removed already, then in the water
        tile(&w, 40, 20).set_type_index(3);
        l.set_needs_removed(&l, true);
        l.world_changed(&l, &mut vec![pos(40, 20)]);
    }
}
