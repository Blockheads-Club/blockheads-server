//! `TrainStation` (InteractionObject, DynamicObject type 49, interaction type 6, item 200): the 3-wide station
//! platform with an editable name; its placement fills three station-owned rails.
//!
//! `updateBitmapString`, `draw:`, `interactionRenderItemType`) are not ported.

use std::cell::RefCell;
use std::rc::{Rc, Weak};

use crate::dynworld::state::DynamicWorld;
use crate::foundation::exception::range_exception_beyond_size;
use crate::foundation::gzip::{gzip_deflate, gzip_inflate};
use crate::items::inventory_item::InventoryItem;
use crate::objects::base::{new_mutable_dict, DynamicObject, DynamicObjectBase, DynamicWorldRef, WorldRef};
use crate::objects::class_info::{ClassInfo, INTERACTION_OBJECT_CLASS};
use crate::objects::handle::{self, CacheRef, ObjRef};
use crate::objects::interaction_object::{
    interaction_object_blockhead_unloaded, interaction_object_blockheads_loaded,
    interaction_object_can_be_used_by_blockhead, interaction_object_creation_net_data_for_client,
    interaction_object_dealloc, interaction_object_get_save_dict,
    interaction_object_init_with_world_dynamic_world_at_position_cache_item_flipped_save_dict_placed_by_client_client_name,
    interaction_object_init_with_world_dynamic_world_cache_net_data,
    interaction_object_init_with_world_dynamic_world_save_dict_cache,
    interaction_object_interaction_object_creation_net_data, interaction_object_remote_update,
    interaction_object_should_add_to_macro_block, interaction_object_update_accurate_dt_is_simulation,
    InteractionObjectLike, InteractionObjectState,
};
use crate::objects::protocols::{SignTextDynamicObject, TapChoiceDynamicObject};
use crate::plist::helpers::{data_from_property_list, property_list_from_data};
use crate::plist::value::{PlistDict, PlistValue};
use crate::types::item_type::ItemType;
use crate::types::structs::{IntPair, WireStruct};
use crate::world::helpers::tile_is_solid;
use crate::world::state::World;
use crate::world::tiles::tile_at_world_position_loaded;

use super::train_car::{ITEM_RAIL, ITEM_TRAIN_STATION};

/// `DYNAMIC_OBJECT_TYPE_TRAIN_STATION`.
pub const DOT_TRAIN_STATION: i32 = 49;
/// `INTERACTION_OBJECT_TYPE_TRAIN_STATION`.
pub const IOT_TRAIN_STATION: u16 = 6;

/// `TrainStation: InteractionObject <SignTextDynamicObject>` class descriptor.
pub static TRAIN_STATION_CLASS: ClassInfo = ClassInfo {
    name: "TrainStation",
    superclass: Some(&INTERACTION_OBJECT_CLASS),
    protocols: &["SignTextDynamicObject"],
};

/// `TrainStation` (instance size 0xe0; only `text` is server state).
pub struct TrainStation {
    base: RefCell<DynamicObjectBase>,
    io: RefCell<InteractionObjectState>,
    /// 0xd8 `text`
    text: RefCell<Option<String>>,
    world: Weak<World>,
    dw: Weak<DynamicWorld>,
}

impl TrainStation {
    fn alloc(world: &Rc<World>, dw: &Rc<DynamicWorld>) -> Rc<TrainStation> {
        let (w, d) = (Rc::downgrade(world), Rc::downgrade(dw));
        handle::alloc_obj(|sw| TrainStation {
            base: RefCell::new(DynamicObjectBase::alloc_zeroed_with(sw)),
            io: RefCell::new(InteractionObjectState::default()),
            text: RefCell::new(None),
            world: w,
            dw: d,
        })
    }

    fn world(&self) -> Rc<World> {
        self.world.upgrade().expect("TrainStation: world dropped")
    }
    fn dw(&self) -> Rc<DynamicWorld> {
        self.dw.upgrade().expect("TrainStation: dynamic world dropped")
    }

    /// `-[TrainStation initWithWorld:dynamicWorld:atPosition:cache:item:flipped:saveDict:placedByClient:clientName:]`
    /// : `text` from the save dict or `"TRAIN"`, then the three rail fills (tile fetched at x+1 is filled at
    /// x-1 and vice versa: original quirk, reproduced).
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        world: &Rc<World>,
        dw: &Rc<DynamicWorld>,
        pos: IntPair,
        cache: CacheRef,
        item: Option<&InventoryItem>,
        flipped: bool,
        save_dict: Option<&PlistDict>,
        client_id: Option<&str>,
        client_name: Option<&str>,
    ) -> Option<ObjRef> {
        let me = Self::alloc(world, dw);
        let o: ObjRef = me.clone();
        let (wref, dref): (WorldRef, DynamicWorldRef) = (world.clone(), dw.clone());
        if !interaction_object_init_with_world_dynamic_world_at_position_cache_item_flipped_save_dict_placed_by_client_client_name(
            &o, &wref, &dref, pos, cache, item, flipped, save_dict, client_id, client_name,
        ) {
            return None;
        }
        let text =
            save_dict.and_then(|d| d.map.get(&"text".to_string())).and_then(|v| v.as_str()).map(|s| s.to_string());
        *me.text.borrow_mut() = Some(text.unwrap_or_else(|| "TRAIN".to_string()));
        let p = o.pos();
        let fill = |tx: i32, at: IntPair| {
            if let Some(t) = tile_at_world_position_loaded(tx, p.y, world) {
                world.fill_tile_at_pos_with_type_data_a_data_b_placed_by_client_save_dict_placed_by_client_name(
                    Some(&t),
                    at,
                    ItemType(ITEM_RAIL as u32),
                    1,
                    0,
                    None,
                    None,
                    None,
                );
            }
        };
        fill(p.x, IntPair { x: p.x, y: p.y });
        fill(p.x.wrapping_add(1), IntPair { x: p.x.wrapping_sub(1), y: p.y });
        fill(p.x.wrapping_sub(1), IntPair { x: p.x.wrapping_add(1), y: p.y });
        Some(o)
    }

    /// `-[TrainStation initWithWorld:dynamicWorld:saveDict:cache:]`: `text` may stay nil, no rail fill.
    pub fn from_save_dict(
        world: &Rc<World>,
        dw: &Rc<DynamicWorld>,
        save_dict: &PlistDict,
        cache: CacheRef,
    ) -> Option<ObjRef> {
        let me = Self::alloc(world, dw);
        let o: ObjRef = me.clone();
        let (wref, dref): (WorldRef, DynamicWorldRef) = (world.clone(), dw.clone());
        if !interaction_object_init_with_world_dynamic_world_save_dict_cache(&o, &wref, &dref, save_dict, cache) {
            return None;
        }
        *me.text.borrow_mut() = save_dict.map.get(&"text".to_string()).and_then(|v| v.as_str()).map(|s| s.to_string());
        Some(o)
    }

    /// `-[TrainStation initWithWorld:dynamicWorld:cache:netData:]`: tail `[0x28, length)` is
    /// `gzip(plist {text, ownerID, ownerName})`; `length < 0x28` raises `NSRangeException`.
    pub fn from_net_data(world: &Rc<World>, dw: &Rc<DynamicWorld>, cache: CacheRef, net_data: &[u8]) -> Option<ObjRef> {
        let me = Self::alloc(world, dw);
        let o: ObjRef = me.clone();
        let (wref, dref): (WorldRef, DynamicWorldRef) = (world.clone(), dw.clone());
        if !interaction_object_init_with_world_dynamic_world_cache_net_data(&o, &wref, &dref, cache, net_data) {
            return None;
        }
        let plist = read_tail(net_data);
        let get =
            |k: &str| plist.as_ref().and_then(|p| p.object_for_key(k)).and_then(|v| v.as_str()).map(|s| s.to_string());
        *me.text.borrow_mut() = get("text");
        o.set_owner_id(get("ownerID"));
        me.io.borrow_mut().owner_name = get("ownerName");
        Some(o)
    }

    /// `-[TrainStation text]`.
    pub fn text_value(&self) -> Option<String> {
        self.text.borrow().clone()
    }

    /// `-[TrainStation setText:]`: no-op when `[newText isEqualToString:text]` (a nil `newText` never
    /// compares equal), else store, `dynamicWorldChangedAtPos:objectType:` and `railOrStationNameChanged`.
    pub fn set_text_value(&self, me: &ObjRef, new_text: Option<&str>) {
        let same = match (new_text, self.text.borrow().as_deref()) {
            (Some(a), Some(b)) => a == b,
            _ => false,
        };
        if same {
            return;
        }
        *self.text.borrow_mut() = new_text.map(|s| s.to_string());
        // updateBitmapString: client-only
        let dw = self.dw();
        dw.dynamic_world_changed_at_pos_object_type(me.pos(), me.object_type());
        dw.rail_or_station_name_changed();
    }
}

/// `subdataWithRange:{0x28, length-0x28}` + `gzipInflate` + `propertyListFromData`; `length < 0x28` raises.
fn read_tail(net_data: &[u8]) -> Option<PlistValue> {
    if net_data.len() < 0x28 {
        range_exception_beyond_size(
            "subdataWithRange:",
            0x28,
            (net_data.len() as u64).wrapping_sub(0x28),
            net_data.len() as u64,
        );
    }
    gzip_inflate(&net_data[0x28..]).as_deref().and_then(property_list_from_data)
}

impl DynamicObject for TrainStation {
    fn base(&self) -> &RefCell<DynamicObjectBase> {
        &self.base
    }
    fn class_info(&self) -> &'static ClassInfo {
        &TRAIN_STATION_CLASS
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn object_type(&self) -> i32 {
        DOT_TRAIN_STATION
    }
    fn occupies_normal_contents(&self) -> bool {
        true
    }
    ///  = `proxyObjectOwner == nil`
    fn should_add_to_macro_block(&self) -> bool {
        interaction_object_should_add_to_macro_block(self)
    }
    fn freeblock_creation_item_type(&self) -> i32 {
        ITEM_TRAIN_STATION
    }
    fn free_block_creation_save_dict(&self, me: &ObjRef) -> Option<PlistDict> {
        Some(self.get_save_dict(me))
    }
    fn get_save_dict(&self, me: &ObjRef) -> PlistDict {
        let mut d = interaction_object_get_save_dict(me);
        if let Some(t) = self.text.borrow().clone() {
            d.map.insert("text".to_string(), PlistValue::string(t));
        }
        d
    }
    /// : `[0x28 InteractionObjectCreationNetData][gzip(XML plist {text?, ownerID?, ownerName?})]`.
    fn creation_net_data_for_client(&self, me: &ObjRef, _client_id: Option<&str>) -> Option<Vec<u8>> {
        let mut out = interaction_object_interaction_object_creation_net_data(me).to_vec();
        let mut dict = new_mutable_dict();
        if let Some(t) = self.text.borrow().clone() {
            dict.map.insert("text".to_string(), PlistValue::string(t));
        }
        if let Some(o) = me.owner_id() {
            dict.map.insert("ownerID".to_string(), PlistValue::string(o));
        }
        if let Some(n) = me.owner_name() {
            dict.map.insert("ownerName".to_string(), PlistValue::string(n));
        }
        if let Some(p) = data_from_property_list(&PlistValue::Dict(dict)) {
            out.extend_from_slice(&gzip_deflate(&p));
        }
        Some(out)
    }
    fn update_net_data_for_client(&self, me: &ObjRef, client_id: Option<&str>) -> Option<Vec<u8>> {
        self.creation_net_data_for_client(me, client_id)
    }
    /// : `[super remoteUpdate:]`; unless a blockhead is using the station, the tail sets text and owner.
    fn remote_update(&self, me: &ObjRef, net_data: &[u8]) {
        interaction_object_remote_update(me, net_data);
        if me.current_blockhead().is_none() {
            let plist = read_tail(net_data);
            let get = |k: &str| {
                plist.as_ref().and_then(|p| p.object_for_key(k)).and_then(|v| v.as_str()).map(|s| s.to_string())
            };
            self.set_text_value(me, get("text").as_deref());
            me.set_owner_id(get("ownerID"));
            self.io.borrow_mut().owner_name = get("ownerName");
        }
    }
    fn blockheads_loaded(&self, me: &ObjRef) {
        interaction_object_blockheads_loaded(me)
    }
    fn update_accurate_dt_is_simulation(&self, me: &ObjRef, dt: f32, accurate_dt: f32, is_simulation: bool) {
        interaction_object_update_accurate_dt_is_simulation(me, dt, accurate_dt, is_simulation)
    }
    fn blockhead_unloaded(&self, me: &ObjRef, b: &ObjRef) {
        interaction_object_blockhead_unloaded(me, b)
    }
    fn dealloc(&self, me: &ObjRef) {
        interaction_object_dealloc(me);
        *self.text.borrow_mut() = None;
    }
    /// : removes the three rails unconditionally (also for `NO`), then the DynamicObject setter.
    fn set_needs_removed(&self, me: &ObjRef, v: bool) {
        let dw = self.dw();
        let p = me.pos();
        dw.remove_rail_at_pos(p);
        dw.remove_rail_at_pos(IntPair { x: p.x.wrapping_sub(1), y: p.y });
        dw.remove_rail_at_pos(IntPair { x: p.x.wrapping_add(1), y: p.y });
        self.base.borrow_mut().needs_removed = v;
    }
    /// : support check below the 3-wide footprint.
    fn world_changed(&self, me: &ObjRef, positions: &mut Vec<IntPair>) {
        if self.base.borrow().is_net {
            return;
        }
        let world = self.world();
        let pos = self.base.borrow().pos;
        for p in positions.iter() {
            let w = world.world_width_macro();
            let full = w.wrapping_shl(5);
            let mut d = pos.x.wrapping_sub(p.x);
            if d < full / 2 {
                if d < (w.wrapping_mul(-32)) / 2 {
                    d = d.wrapping_add(full);
                }
            } else {
                d = d.wrapping_sub(full);
            }
            if d.wrapping_abs() < 2 && p.y == pos.y.wrapping_sub(1) {
                for x in [pos.x, pos.x.wrapping_sub(1), pos.x.wrapping_add(1)] {
                    let t = tile_at_world_position_loaded(x, pos.y.wrapping_sub(1), &world);
                    if let Some(t) = t {
                        if !tile_is_solid(Some(t)) {
                            InteractionObjectLike::remove(self, me, None);
                            return;
                        }
                    }
                }
            }
        }
    }
    fn as_tap_choice(&self) -> Option<&dyn TapChoiceDynamicObject> {
        Some(self)
    }
    fn as_sign_text(&self) -> Option<&dyn SignTextDynamicObject> {
        Some(self)
    }
    fn as_interaction_object(&self) -> Option<&RefCell<InteractionObjectState>> {
        Some(&self.io)
    }
    fn as_interaction_like(&self) -> Option<&dyn InteractionObjectLike> {
        Some(self)
    }
}

impl InteractionObjectLike for TrainStation {
    fn io_state(&self) -> &RefCell<InteractionObjectState> {
        &self.io
    }
    fn interaction_object_type(&self) -> u16 {
        IOT_TRAIN_STATION
    }
    fn destroy_item_type(&self) -> i32 {
        ITEM_TRAIN_STATION
    }
    /// : `"%@ STATION"` (a nil `text` formats as `(null)`).
    fn title(&self) -> String {
        match &*self.text.borrow() {
            Some(t) => format!("{} STATION", t),
            None => "(null) STATION".to_string(),
        }
    }
    fn requires_human_interaction(&self) -> bool {
        true
    }
    ///  `remove:`: server-authoritative removal of the footprint (3x2), dropping item 200.
    fn remove(&self, me: &ObjRef, removing: Option<&ObjRef>) {
        if self.base.borrow().needs_removed || self.base.borrow().is_net {
            return;
        }
        let in_use = self.io.borrow().is_in_use;
        if in_use {
            let cur = self.io.borrow().current_blockhead.clone();
            let remote = self.io.borrow().remote_blockhead_in_use_unique_id;
            if cur.as_ref().map_or(false, |c| c.is_net()) || remote != 0 {
                self.io.borrow_mut().needs_to_be_removed_when_interaction_ends = true;
                return;
            }
            if let Some(c) = &cur {
                if let Some(bh) = c.as_blockhead() {
                    bh.stop_interacting();
                }
            }
        }
        let world = self.world();
        let dw = self.dw();
        let pos = self.base.borrow().pos;
        let rm = |x: i32, y: i32| {
            world.remove_tile_at_world_x_world_y_create_contents_freeblock_count_create_foreground_contents_freeblock_count_remove_blockhead(
                x, y, 0, 0, removing,
            );
        };
        rm(pos.x, pos.y);
        me.set_needs_removed(me, true);
        let sd = me.get_save_dict(me);
        dw.create_free_block_at_position_of_type_data_a_data_b_sub_items_dynamic_object_save_dict_hovers_play_sound_priority_blockhead(
            pos,
            ItemType(ITEM_TRAIN_STATION as u32),
            0,
            0,
            None,
            Some(PlistValue::Dict(sd)),
            false,
            1,
            removing,
        );
        dw.rail_or_station_name_changed();
        rm(pos.x.wrapping_sub(1), pos.y);
        rm(pos.x.wrapping_sub(1), pos.y.wrapping_add(1));
        rm(pos.x, pos.y.wrapping_add(1));
        rm(pos.x.wrapping_add(1), pos.y);
        rm(pos.x.wrapping_add(1), pos.y.wrapping_add(1));
    }
    /// : admin shortcut, else the base rule plus the owner check on `localNetID`.
    fn can_be_used_by_blockhead(&self, me: &ObjRef, b: &ObjRef) -> bool {
        let controlled = b.as_blockhead().map_or(false, |x| x.is_client_blockhead_being_controlled_by_server());
        if self.world().is_admin() && !controlled {
            return true;
        }
        let mut ok = interaction_object_can_be_used_by_blockhead(me, b);
        if ok {
            if let Some(owner) = me.owner_id() {
                ok = b.as_blockhead().and_then(|x| x.local_net_id()).map_or(false, |l| l == owner);
            }
        }
        ok
    }
}

impl TapChoiceDynamicObject for TrainStation {
    fn action_title(&self) -> String {
        InteractionObjectLike::action_title(self)
    }
    fn is_double_height(&self) -> bool {
        InteractionObjectLike::is_double_height(self)
    }
}

impl SignTextDynamicObject for TrainStation {
    fn text(&self) -> Option<String> {
        self.text_value()
    }
    fn set_text(&self, me: &ObjRef, t: Option<&str>) {
        self.set_text_value(me, t)
    }
    fn requires_single_line_text_editing(&self) -> bool {
        true
    }
}

// keep the interaction net-data helper referenced for readers of the spec cross-reference
#[allow(dead_code)]
fn _creation_helper(me: &ObjRef) -> Option<Vec<u8>> {
    interaction_object_creation_net_data_for_client(me, None)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// : the 3x2 footprint order and the swapped-tile placement quirk are encoded in `new`/`remove`; here the
    /// pure parts: title formatting and constants.
    #[test]
    fn constants_and_title_format() {
        assert_eq!(DOT_TRAIN_STATION, 49);
        assert_eq!(IOT_TRAIN_STATION, 6);
        assert_eq!(ITEM_TRAIN_STATION, 200);
        assert_eq!(ITEM_RAIL, 199);
        assert_eq!(format!("{} STATION", "TRAIN"), "TRAIN STATION");
    }
}
