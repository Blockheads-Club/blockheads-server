//! `Window: DynamicObject` (DynamicObjectType 31).
//!
//! (`Class WirePathCreator` of that file belongs to pathfinding). Client-only members are not ported.

use std::cell::RefCell;
use std::rc::Rc;

use super::column::{append_plist_tail, owner_from_client_or_save, read_plist_tail, sd_string, tail_string, Links};
use crate::dynworld::state::DynamicWorld;
use crate::lighting::sunlight::WorldHelper;
use crate::objects::base::{
    base_dynamic_object_net_data, base_get_save_dict, base_init_with_world_dynamic_world_at_position_cache,
    base_init_with_world_dynamic_world_cache_net_data, base_init_with_world_dynamic_world_save_dict_cache,
    padded_bytes, DynamicObject, DynamicObjectBase, DynamicWorldRef, WorldRef,
};
use crate::objects::class_info::{ClassInfo, DYNAMIC_OBJECT_CLASS};
use crate::objects::handle::{self, CacheRef, ObjRef};
use crate::plist::value::{PlistDict, PlistValue};
use crate::types::netdata::WindowCreationNetData;
use crate::types::structs::{IntPair, WireStruct};
use crate::world::state::World;
use crate::world::tiles::tile_at_world_position_loaded;

/// `DYNAMIC_OBJECT_TYPE_WINDOW`.
pub const DYNAMIC_OBJECT_TYPE_WINDOW: i32 = 31;

/// `Window` class descriptor.
pub static WINDOW_CLASS: ClassInfo =
    ClassInfo { name: "Window", superclass: Some(&DYNAMIC_OBJECT_CLASS), protocols: &[] };

/// Window ivars.
pub struct WindowState {
    pub item_type: i32,
}

/// `Window: DynamicObject` (type 31).
pub struct Window {
    base: RefCell<DynamicObjectBase>,
    /// The Window ivars.
    pub st: RefCell<WindowState>,
    links: Links,
}

impl Window {
    fn alloc(world: &Rc<World>, dw: &Rc<DynamicWorld>) -> Rc<Window> {
        let links = Links::new(world, dw);
        handle::alloc_obj(|sw| Window {
            base: RefCell::new(DynamicObjectBase::alloc_zeroed_with(sw)),
            st: RefCell::new(WindowState { item_type: 0 }),
            links,
        })
    }

    /// Placement constructor. Order: base, itemType, owner, `initSubDerivedItems` (empty).
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
        me.st.borrow_mut().item_type = item_type;
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
        me.st.borrow_mut().item_type = save_dict.map.get(&"itemType".to_string()).map_or(0, |v| v.int_value());
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
        let s = WindowCreationNetData::read_le(&padded_bytes(net_data, 0x20));
        me.st.borrow_mut().item_type = s.item_type as i32;
        if net_data.len() > 0x20 {
            let owner = tail_string(&read_plist_tail(net_data, 0x20), "ownerID");
            me.base.borrow_mut().owner_id = owner;
        }
        Some(me_obj)
    }

    /// `-[Window itemType]`
    pub fn item_type(&self) -> i32 {
        self.st.borrow().item_type
    }
}

impl DynamicObject for Window {
    fn base(&self) -> &RefCell<DynamicObjectBase> {
        &self.base
    }
    fn class_info(&self) -> &'static ClassInfo {
        &WINDOW_CLASS
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn object_type(&self) -> i32 {
        DYNAMIC_OBJECT_TYPE_WINDOW
    }
    fn freeblock_creation_item_type(&self) -> i32 {
        self.item_type()
    }
    fn occupies_background_contents(&self) -> bool {
        true
    }

    /// `-[Window setNeedsRemoved:]`: super store, then on `YES` the sun light is re-propagated (even when the tile is nil,
    /// and every time it is called).
    fn set_needs_removed(&self, _me: &ObjRef, v: bool) {
        self.base.borrow_mut().needs_removed = v;
        if v {
            let world = self.links.world();
            let pos = self.base.borrow().pos;
            let tile = tile_at_world_position_loaded(pos.x, pos.y, &world);
            WorldHelper::update_sun_light_removed_for_tile_at_pos_world(tile, pos, &world);
        }
    }

    fn get_save_dict(&self, me: &ObjRef) -> PlistDict {
        let mut d = base_get_save_dict(me);
        d.map.insert("itemType".to_string(), PlistValue::int(self.item_type() as i64));
        if let Some(o) = self.base.borrow().owner_id.clone() {
            d.map.insert("ownerID".to_string(), PlistValue::string(o));
        }
        d
    }

    fn creation_net_data_for_client(&self, me: &ObjRef, _client_id: Option<&str>) -> Option<Vec<u8>> {
        let s = WindowCreationNetData {
            dynamic_object_net_data: base_dynamic_object_net_data(me),
            item_type: self.item_type() as u16,
            padding: [0; 6],
        };
        let mut out = s.to_vec();
        let owner = self.base.borrow().owner_id.clone();
        append_plist_tail(&mut out, &[("ownerID", owner.as_deref())]);
        Some(out)
    }
    fn update_net_data_for_client(&self, me: &ObjRef, client_id: Option<&str>) -> Option<Vec<u8>> {
        self.creation_net_data_for_client(me, client_id)
    }
}

#[cfg(test)]
mod tests {
    use super::super::column::testkit::*;
    use super::*;

    #[test]
    fn round_trips_and_flags() {
        let (w, d) = env();
        let o = Window::init_with_world_dynamic_world_at_position_cache_type_save_dict_placed_by_client(
            &w,
            &d,
            pos(40, 20),
            None,
            58,
            None,
            Some("own"),
        )
        .unwrap();
        assert_eq!(handle::downcast::<Window>(&o).unwrap().item_type(), 58);
        assert!(o.occupies_background_contents() && !o.occupies_normal_contents() && !o.occupies_foreground_contents());
        assert!(o.free_block_creation_save_dict(&o).is_none());
        assert_eq!(o.freeblock_creation_item_type(), 58);
        let sd = o.get_save_dict(&o);
        assert_eq!(sd.map.len(), 6);
        let o2 = Window::init_with_world_dynamic_world_save_dict_cache(&w, &d, &sd, None).unwrap();
        assert_eq!(o2.owner_id().as_deref(), Some("own"));
        let b = o.creation_net_data_for_client(&o, None).unwrap();
        assert_eq!(&b[0x18..0x1a], &58u16.to_le_bytes());
        assert!(b[0x1a..0x20].iter().all(|x| *x == 0));
        let o3 = Window::init_with_world_dynamic_world_cache_net_data(&w, &d, None, &b).unwrap();
        assert_eq!(o3.owner_id().as_deref(), Some("own"));
        // no owner, no tail contents but tail present
        let o4 = Window::init_with_world_dynamic_world_at_position_cache_type_save_dict_placed_by_client(
            &w,
            &d,
            pos(41, 20),
            None,
            174,
            None,
            None,
        )
        .unwrap();
        assert!(o4.creation_net_data_for_client(&o4, None).unwrap().len() > 0x20);
    }

    /// `setNeedsRemoved:` re-propagates the sun light on every `YES` (also when already removed) and not on `NO`.
    #[test]
    fn set_needs_removed_updates_sun_light() {
        let (w, d) = env();
        let _ = d;
        let o = Window::init_with_world_dynamic_world_at_position_cache_type_save_dict_placed_by_client(
            &w,
            &d,
            pos(40, 20),
            None,
            58,
            None,
            None,
        )
        .unwrap();
        tile(&w, 40, 20).set_sun_light(0);
        o.set_needs_removed(&o, true);
        assert!(o.needs_removed());
        o.set_needs_removed(&o, true);
        o.set_needs_removed(&o, false);
        assert!(!o.needs_removed());
    }
}
