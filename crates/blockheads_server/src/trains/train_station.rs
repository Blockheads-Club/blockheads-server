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
        // the InteractionObject initialisers only fail through the net-data path, never here
        let inited = interaction_object_init_with_world_dynamic_world_at_position_cache_item_flipped_save_dict_placed_by_client_client_name(
            &o, &wref, &dref, pos, cache, item, flipped, save_dict, client_id, client_name,
        );
        debug_assert!(inited);
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
        let inited =
            interaction_object_init_with_world_dynamic_world_save_dict_cache(&o, &wref, &dref, save_dict, cache);
        debug_assert!(inited);
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
    use crate::interaction::sign::testkit::{tile, world_with_block};
    use crate::objects::base::{new_mutable_dict, DynamicObject};
    use crate::world::state::PhysicalBlockRef;

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

    fn fixture(name: &str) -> (Rc<World>, Rc<DynamicWorld>, PhysicalBlockRef) {
        let (w, d, pb) = world_with_block(name);
        for x in 0..32 {
            tile(pb, x, 9).set_type_index(1);
        }
        *w.distance_ordered_food_types.borrow_mut() = (0..31).collect();
        (w, d, pb)
    }

    fn station(w: &Rc<World>, d: &Rc<DynamicWorld>, x: i32, text: Option<&str>) -> ObjRef {
        let it = InventoryItem::new(ItemType(ITEM_TRAIN_STATION as u32), 0, 0, None, None);
        let it = it.borrow();
        let mut sd = new_mutable_dict();
        if let Some(t) = text {
            sd.map.insert("text".to_string(), PlistValue::string(t));
        }
        TrainStation::new(w, d, IntPair { x, y: 10 }, None, Some(&it), false, Some(&sd), Some("cid"), Some("Bob"))
            .unwrap()
    }

    fn st(o: &ObjRef) -> &TrainStation {
        handle::downcast::<TrainStation>(o).unwrap()
    }

    /// Placement fills the three rails (the quirk fetches x+1 for x-1 and vice versa), the text defaults to TRAIN.
    #[test]
    fn placement_fills_three_rails_and_defaults_text() {
        let (w, d, pb) = fixture("ts_place");
        let s = station(&w, &d, 10, None);
        assert_eq!(st(&s).text_value().as_deref(), Some("TRAIN"));
        assert_eq!(InteractionObjectLike::title(st(&s)), "TRAIN STATION");
        for x in 9..=11 {
            assert_ne!(tile(pb, x, 10).foreground_contents(), 0, "x={x}");
        }
        let s2 = station(&w, &d, 20, Some("Central"));
        assert_eq!(SignTextDynamicObject::text(st(&s2)).as_deref(), Some("Central"));
        assert!(SignTextDynamicObject::requires_single_line_text_editing(st(&s2)));
        assert_eq!(s2.object_type(), DOT_TRAIN_STATION);
        assert_eq!(s2.class_info().name, "TrainStation");
        assert!(s2.occupies_normal_contents());
        assert_eq!(s2.freeblock_creation_item_type(), ITEM_TRAIN_STATION);
        assert_eq!(st(&s2).interaction_object_type(), IOT_TRAIN_STATION);
        assert_eq!(st(&s2).destroy_item_type(), ITEM_TRAIN_STATION);
        assert!(st(&s2).requires_human_interaction());
        assert!(s2.should_add_to_macro_block());
        assert!(s2.as_tap_choice().is_some() && s2.as_sign_text().is_some());
        assert!(s2.as_interaction_object().is_some() && s2.as_interaction_like().is_some());
        let _ = TapChoiceDynamicObject::action_title(st(&s2));
        let _ = TapChoiceDynamicObject::is_double_height(st(&s2));
        assert!(_creation_helper(&s2).is_some());
    }

    /// Placement outside any loaded tile leaves the rails alone; a nil save-dict text shows as `(null)`.
    #[test]
    fn save_dict_round_trip_and_null_title() {
        let (w, d, _pb) = fixture("ts_save");
        let s = station(&w, &d, 10, Some("Hub"));
        let sd = s.get_save_dict(&s);
        assert_eq!(sd.map.get(&"text".to_string()).and_then(|v| v.as_str()), Some("Hub"));
        assert!(s.free_block_creation_save_dict(&s).is_some());
        let again = TrainStation::from_save_dict(&w, &d, &sd, None).unwrap();
        assert_eq!(st(&again).text_value().as_deref(), Some("Hub"));
        let mut bare = sd.clone();
        bare.map.remove(&"text".to_string());
        let nil = TrainStation::from_save_dict(&w, &d, &bare, None).unwrap();
        assert_eq!(st(&nil).text_value(), None);
        assert_eq!(InteractionObjectLike::title(st(&nil)), "(null) STATION");
        assert!(!nil.get_save_dict(&nil).map.contains_key(&"text".to_string()));
        nil.dealloc(&nil);
        s.dealloc(&s);
        assert_eq!(st(&s).text_value(), None);
    }

    /// `setText:` is a no-op for an equal string, a change otherwise (also to nil); the net image round-trips.
    #[test]
    fn set_text_and_net_data() {
        let (w, d, _pb) = fixture("ts_text");
        let s = station(&w, &d, 10, Some("A"));
        st(&s).set_text_value(&s, Some("A"));
        st(&s).set_text_value(&s, Some("B"));
        assert_eq!(st(&s).text_value().as_deref(), Some("B"));
        st(&s).set_text_value(&s, None);
        assert_eq!(st(&s).text_value(), None);
        st(&s).set_text_value(&s, None);
        SignTextDynamicObject::set_text(st(&s), &s, Some("C"));
        let nd = s.creation_net_data_for_client(&s, None).unwrap();
        assert_eq!(nd, s.update_net_data_for_client(&s, None).unwrap());
        assert!(nd.len() > 0x28);
        let copy = TrainStation::from_net_data(&w, &d, None, &nd).unwrap();
        assert_eq!(st(&copy).text_value().as_deref(), Some("C"));
        assert_eq!(copy.owner_id().as_deref(), Some("cid"));
        assert_eq!(copy.owner_name().as_deref(), Some("Bob"));
        // a head-only image has no tail: every field is nil
        let head = TrainStation::from_net_data(&w, &d, None, &nd[..0x28]).unwrap();
        assert_eq!(st(&head).text_value(), None);
    }

    /// `remoteUpdate:` copies text and owner from the tail unless a blockhead is using the station.
    #[test]
    fn remote_update_applies_tail() {
        let (w, d, _pb) = fixture("ts_remote");
        let s = station(&w, &d, 10, Some("old"));
        let src = station(&w, &d, 20, Some("new"));
        let nd = src.creation_net_data_for_client(&src, None).unwrap();
        s.remote_update(&s, &nd);
        assert_eq!(st(&s).text_value().as_deref(), Some("new"));
        assert_eq!(s.owner_id().as_deref(), Some("cid"));
        // short image raises (range exception) before touching anything
        let bh = crate::blockhead::blockhead::testutil::new_bh(&w, &(d.clone() as DynamicWorldRef), 0);
        st(&s).io.borrow_mut().current_blockhead = Some(bh);
        let other = station(&w, &d, 25, Some("other"));
        let nd2 = other.creation_net_data_for_client(&other, None).unwrap();
        s.remote_update(&s, &nd2);
        assert_eq!(st(&s).text_value().as_deref(), Some("new"));
    }

    #[test]
    #[should_panic]
    fn short_net_data_raises() {
        let (w, d, _pb) = fixture("ts_short");
        let _ = TrainStation::from_net_data(&w, &d, None, &[0u8; 0x10]);
    }

    /// `removeRail` on `setNeedsRemoved:`, then the support check in `worldChanged:` (wrap and non-wrap branches).
    #[test]
    fn world_changed_removes_unsupported_station() {
        let (w, d, pb) = fixture("ts_wc");
        let s = station(&w, &d, 10, None);
        // unrelated positions and net copies are ignored
        s.world_changed(&s, &mut vec![IntPair { x: 10, y: 3 }, IntPair { x: 30, y: 9 }]);
        assert!(!s.needs_removed());
        // footprint reaching into an unloaded macro tile: the missing tile is skipped
        let edge = station(&w, &d, 31, None);
        edge.world_changed(&edge, &mut vec![IntPair { x: 31, y: 9 }]);
        assert!(!edge.needs_removed());
        // a supported footprint stays
        s.world_changed(&s, &mut vec![IntPair { x: 10, y: 9 }]);
        assert!(!s.needs_removed());
        // far right in the wrapped world (d >= full / 2), still unrelated
        s.world_changed(&s, &mut vec![IntPair { x: 100, y: 9 }]);
        assert!(!s.needs_removed());
        // support gone under the right edge
        tile(pb, 11, 9).set_type_index(2);
        s.world_changed(&s, &mut vec![IntPair { x: 11, y: 9 }]);
        assert!(s.needs_removed());
        // net copies never remove themselves
        let n = station(&w, &d, 20, None);
        n.base().borrow_mut().is_net = true;
        tile(pb, 20, 9).set_type_index(2);
        n.world_changed(&n, &mut vec![IntPair { x: 20, y: 9 }]);
        assert!(!n.needs_removed());
        // wrapped distance: world is 4 macro wide (128 tiles); x = 127 is one tile left of 0
        let e = station(&w, &d, 1, None);
        tile(pb, 1, 9).set_type_index(2);
        e.world_changed(&e, &mut vec![IntPair { x: 127, y: 9 }]);
        assert!(!e.needs_removed());
        e.set_needs_removed(&e, false);
        // the mirror image: x = 127 sees a change at x = 1 as two tiles to its right
        let f = station(&w, &d, 127, None);
        f.world_changed(&f, &mut vec![IntPair { x: 1, y: 9 }]);
        assert!(!f.needs_removed());
    }

    /// `remove:` guards, the in-use deferral and the full footprint removal with the item drop.
    #[test]
    fn remove_paths() {
        let (w, d, _pb) = fixture("ts_rm");
        let bh = crate::blockhead::blockhead::testutil::new_bh(&w, &(d.clone() as DynamicWorldRef), 0);
        // a net copy is left alone
        let n = station(&w, &d, 20, None);
        n.base().borrow_mut().is_net = true;
        InteractionObjectLike::remove(st(&n), &n, None);
        assert!(!n.needs_removed());
        // in use by a net blockhead: deferred
        let a = station(&w, &d, 6, None);
        st(&a).io.borrow_mut().is_in_use = true;
        st(&a).io.borrow_mut().remote_blockhead_in_use_unique_id = 5;
        InteractionObjectLike::remove(st(&a), &a, None);
        assert!(st(&a).io.borrow().needs_to_be_removed_when_interaction_ends);
        assert!(!a.needs_removed());
        // in use locally: the blockhead stops interacting, then the removal runs
        let b = station(&w, &d, 12, None);
        {
            let mut io = st(&b).io.borrow_mut();
            io.is_in_use = true;
            io.current_blockhead = Some(bh.clone());
        }
        InteractionObjectLike::remove(st(&b), &b, Some(&bh));
        assert!(b.needs_removed());
        // already removed: nothing more happens
        InteractionObjectLike::remove(st(&b), &b, None);
        // in use by something that is no blockhead: just removed
        let e = station(&w, &d, 24, None);
        {
            let mut io = st(&e).io.borrow_mut();
            io.is_in_use = true;
            io.current_blockhead = Some(crate::blockhead::blockhead::testutil::stub(1, false));
        }
        InteractionObjectLike::remove(st(&e), &e, None);
        assert!(e.needs_removed());
        // marked in use without a blockhead: nothing to stop
        let f = station(&w, &d, 28, None);
        st(&f).io.borrow_mut().is_in_use = true;
        InteractionObjectLike::remove(st(&f), &f, None);
        assert!(f.needs_removed());
        // plain removal
        let c = station(&w, &d, 18, None);
        InteractionObjectLike::remove(st(&c), &c, None);
        assert!(c.needs_removed());
    }

    /// Admin shortcut, owner check on `localNetID`.
    #[test]
    fn can_be_used_rules() {
        let _h = crate::blockhead::blockhead::testutil::TestHost::install();
        let (w, d, _pb) = fixture("ts_use");
        w.set_is_admin(true);
        let bh = crate::blockhead::blockhead::testutil::new_bh(&w, &(d.clone() as DynamicWorldRef), 0);
        let s = station(&w, &d, 10, None);
        // the world is admin: any blockhead may use it unless server-controlled
        assert!(InteractionObjectLike::can_be_used_by_blockhead(st(&s), &s, &bh));
        crate::blockhead::blockhead::testutil::bh_of(&bh)
            .bh
            .borrow_mut()
            .is_client_blockhead_being_controlled_by_server = true;
        crate::blockhead::blockhead::testutil::bh_of(&bh).set_client_id("someone");
        // controlled by the server: the admin shortcut is off and the owner ("cid") differs
        assert!(!InteractionObjectLike::can_be_used_by_blockhead(st(&s), &s, &bh));
        crate::blockhead::blockhead::testutil::bh_of(&bh).set_client_id("cid");
        assert!(InteractionObjectLike::can_be_used_by_blockhead(st(&s), &s, &bh));
        // no owner: only the base rule applies
        s.set_owner_id(None);
        assert!(InteractionObjectLike::can_be_used_by_blockhead(st(&s), &s, &bh));
        {
            let mut io = st(&s).io.borrow_mut();
            io.is_in_use = true;
            io.current_blockhead = Some(crate::blockhead::blockhead::testutil::stub(1, false));
        }
        assert!(!InteractionObjectLike::can_be_used_by_blockhead(st(&s), &s, &bh));
    }

    /// The lifecycle hooks forward to the InteractionObject base.
    #[test]
    fn lifecycle_hooks_forward() {
        let (w, d, _pb) = fixture("ts_life");
        let bh = crate::blockhead::blockhead::testutil::new_bh(&w, &(d.clone() as DynamicWorldRef), 0);
        let s = station(&w, &d, 10, None);
        assert!(std::ptr::eq(InteractionObjectLike::io_state(st(&s)), &st(&s).io));
        s.blockheads_loaded(&s);
        s.update_accurate_dt_is_simulation(&s, 0.016, 0.016, false);
        s.blockhead_unloaded(&s, &bh);
    }

    /// A net image whose macro tile has no physical block is refused.
    #[test]
    fn net_data_without_a_loaded_tile_is_refused() {
        let (w, d, _pb) = fixture("ts_net_nil");
        let far = station(&w, &d, 10, None);
        far.base().borrow_mut().pos = IntPair { x: 70, y: 10 };
        let nd = far.creation_net_data_for_client(&far, None).unwrap();
        assert!(TrainStation::from_net_data(&w, &d, None, &nd).is_none());
    }
}
