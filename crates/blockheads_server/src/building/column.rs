//! `Column: DynamicObject` (DynamicObjectType 53) plus the helpers shared by every building object.
//!
//! Shared definitions" and "# 1. Class `Column`".
//! Client-only members (`draw:`, `imageIndex`, `addDrawCubeData*`, `reloadDrawBlock*`) are not ported.

use std::cell::RefCell;
use std::rc::{Rc, Weak};

use crate::dynworld::state::DynamicWorld;
use crate::foundation::exception::{ns_raise, unrecognized_selector};
use crate::foundation::gzip::{gzip_deflate, gzip_inflate};
use crate::objects::base::{
    base_dynamic_object_net_data, base_get_save_dict, base_init_with_world_dynamic_world_at_position_cache,
    base_init_with_world_dynamic_world_cache_net_data, base_init_with_world_dynamic_world_save_dict_cache,
    base_remote_update, new_mutable_dict, padded_bytes, DynamicObject, DynamicObjectBase, DynamicWorldRef, WorldApi,
    WorldRef,
};
use crate::objects::class_info::{ClassInfo, DYNAMIC_OBJECT_CLASS};
use crate::objects::handle::{self, CacheRef, ObjRef};
use crate::plist::helpers::{data_from_property_list, property_list_from_data};
use crate::plist::value::{PlistDict, PlistValue};
use crate::rng::glibc_rand::r01;
use crate::types::item_type::ItemType;
use crate::types::netdata::ColumnCreationNetData;
use crate::types::structs::{IntPair, WireStruct};
use crate::world::climate::{current_temperature_for_tile_at_world_pos, season_for_world_x};
use crate::world::helpers::tile_is_solid;
use crate::world::state::World;
use crate::world::tiles::tile_at_world_position_loaded;

/// `DYNAMIC_OBJECT_TYPE_COLUMN`.
pub const DYNAMIC_OBJECT_TYPE_COLUMN: i32 = 53;
/// `CONTENTS_COLUMN` (`foregroundContents` byte).
pub const CONTENTS_COLUMN: u8 = 100;
/// `ITEM_ICE_COLUMN`.
pub const ITEM_ICE_COLUMN: i32 = 244;
/// `ITEM_MELT_WATER`.
pub const ITEM_MELT_WATER: u32 = 1059;

/// `Column` class descriptor.
pub static COLUMN_CLASS: ClassInfo =
    ClassInfo { name: "Column", superclass: Some(&DYNAMIC_OBJECT_CLASS), protocols: &[] };

/// `ColumnConfiguration`.
pub const COLUMN_UNDEFINED: i32 = 0;
pub const COLUMN_NO_PLINTH: i32 = 1;
pub const COLUMN_PLINTH_BELOW: i32 = 2;
pub const COLUMN_PLINTH_ABOVE: i32 = 3;
pub const COLUMN_PLINTH_ABOVE_AND_BELOW: i32 = 4;

// =====================================================================================================
// shared helpers
// =====================================================================================================

/// Strong links to the concrete `World` and `DynamicWorld` kept by every building object (the object base only
/// stores the abstract trait handles).
#[derive(Clone)]
pub(crate) struct Links {
    world: Weak<World>,
    dw: Weak<DynamicWorld>,
}

impl Links {
    pub(crate) fn new(world: &Rc<World>, dw: &Rc<DynamicWorld>) -> Links {
        Links { world: Rc::downgrade(world), dw: Rc::downgrade(dw) }
    }
    pub(crate) fn world(&self) -> Rc<World> {
        self.world.upgrade().expect("building object: world dropped")
    }
    pub(crate) fn dw(&self) -> Rc<DynamicWorld> {
        self.dw.upgrade().expect("building object: dynamic world dropped")
    }
}

/// Horizontal wrap used by Column/Stairs/Wire `worldChanged:` (identical arithmetic, C `int` division).
pub(crate) fn wrap_dx(d: i32, w_macro: i32) -> i32 {
    let w = w_macro.wrapping_mul(32);
    if d < w / 2 {
        if d < w_macro.wrapping_mul(-32) / 2 {
            d.wrapping_add(w)
        } else {
            d
        }
    } else {
        d.wrapping_sub(w)
    }
}

/// Fixed struct bytes are already in `out`; appends `gzip_deflate(data_from_property_list(dict))` where the dict holds
/// every `Some` entry. The tail is ALWAYS appended; a `None` plist (cannot happen for string values) appends nothing.
pub(crate) fn append_plist_tail(out: &mut Vec<u8>, entries: &[(&str, Option<&str>)]) {
    let mut d = new_mutable_dict();
    for (k, v) in entries {
        if let Some(v) = v {
            d.map.insert((*k).to_string(), PlistValue::string(*v));
        }
    }
    if let Some(p) = data_from_property_list(&PlistValue::Dict(d)) {
        out.extend_from_slice(&gzip_deflate(&p));
    }
}

/// `[netData subdataWithRange:{off, len-off}]` + `gzipInflate` + `propertyListFromData`. `len < off` raises
/// `NSRangeException` ("Range location + length too great"); `len == off` gives `None`.
pub(crate) fn read_plist_tail(net: &[u8], off: usize) -> Option<PlistValue> {
    if net.len() < off {
        ns_raise("NSRangeException", "Range location + length too great");
    }
    let inflated = gzip_inflate(&net[off..])?;
    property_list_from_data(&inflated)
}

/// `[dict objectForKey:k]` as a string. A non-dictionary plist root raises `NSInvalidArgumentException`
/// ("<class>(instance) does not recognize objectForKey:"); a non-string value gives `None` (documented deviation).
pub(crate) fn tail_string(d: &Option<PlistValue>, key: &str) -> Option<String> {
    match d {
        None => None,
        Some(PlistValue::Dict(dict)) => dict.map.get(&key.to_string()).and_then(|v| v.as_str()).map(|s| s.to_string()),
        Some(other) => unrecognized_selector(other.class_name(), "objectForKey:"),
    }
}

/// `ownerID` of the placement constructors: `clientID` if given, else `saveDict["ownerID"]` if present.
pub(crate) fn owner_from_client_or_save(client_id: Option<&str>, save_dict: Option<&PlistDict>) -> Option<String> {
    match client_id {
        Some(c) => Some(c.to_string()),
        None => sd_get(save_dict, "ownerID").and_then(PlistValue::as_str).map(str::to_string),
    }
}

/// `[saveDict objectForKey:k]` (nil dict gives nil).
pub(crate) fn sd_get<'a>(d: Option<&'a PlistDict>, k: &str) -> Option<&'a PlistValue> {
    d.and_then(|d| d.map.get(&k.to_string()))
}

/// `[[saveDict objectForKey:@"ownerID"] retain]` (nil when absent or not a string).
pub(crate) fn sd_string(d: &PlistDict, k: &str) -> Option<String> {
    d.map.get(&k.to_string()).and_then(|v| v.as_str()).map(|s| s.to_string())
}

/// Shared drop sequence of Column/Stairs/Ladder/Wire/Door: create a free block with `(dataA, dataB) = (0, 0)` then
/// `removeStandardObject:`. The caller already tested `!isClient && !needsRemoved`.
pub(crate) fn drop_as_free_block(
    me: &ObjRef,
    links: &Links,
    pos: IntPair,
    item_type: i32,
    save_dict: Option<PlistDict>,
) {
    let dw = links.dw();
    dw.create_free_block_at_position_of_type_data_a_data_b_sub_items_dynamic_object_save_dict_hovers_play_sound_priority_blockhead(
        pos,
        ItemType(item_type as u32),
        0,
        0,
        None,
        save_dict.map(PlistValue::Dict),
        false,
        1,
        None,
    );
    dw.remove_standard_object(Some(me));
}

/// `crash_or_return()` of: SIGSEGV under `faithful-crashes`, otherwise the caller returns.
/// UB-PARITY (E4).
pub(crate) fn ub_crash_or_return() {
    crate::objects::base::ub_null_tile_deref();
}

/// Shared by Column and Stairs `update` (ice melting). Caller already checked `!is_client && !needs_removed &&
/// item_type == ice`.
pub(crate) fn ice_melt_step(me: &ObjRef, links: &Links, pos: IntPair, dt: f32, timer: &mut f32) {
    *timer += dt;
    if !(*timer > 5.0f32) {
        return;
    }
    *timer = 0.0;
    let world = links.world();
    let weather = world.get_weather_fraction_for_pos(pos);
    let wt1 = WorldApi::world_time(&*world);
    let daynight = world.get_day_night_fraction_for_x_at_world_time(pos.x as f32, wt1);
    let tile = tile_at_world_position_loaded(pos.x, pos.y, &world);
    let season = season_for_world_x(pos.x, WorldApi::world_time(&*world), Some(&world));
    let Some(t) = tile else {
        // UB-PARITY: the original reads tile->sunLight of NULL inside currentTemperatureForTileAtWorldPos -> SIGSEGV
        ub_crash_or_return();
        return;
    };
    let temp = current_temperature_for_tile_at_world_pos(&t.get(), pos, season, daynight, weather, Some(&world));
    if temp > 1.0f32 {
        t.set_partial_content_left(0x7f);
        world.fill_tile_at_pos_with_type(Some(&t), pos, ItemType(ITEM_MELT_WATER));
        links.dw().remove_standard_object(Some(me));
    }
}

// =====================================================================================================
// Column
// =====================================================================================================

/// Column ivars 0x50.. (render ivars omitted).
pub struct ColumnState {
    pub item_type: i32,
    pub paint_color: u16,
    pub current_configuration: i32,
    pub ice_melt_timer: f32,
}

/// `Column: DynamicObject` (type 53). Class `Column`".
pub struct Column {
    base: RefCell<DynamicObjectBase>,
    /// The Column ivars.
    pub st: RefCell<ColumnState>,
    links: Links,
}

impl Column {
    fn alloc(world: &Rc<World>, dw: &Rc<DynamicWorld>) -> Rc<Column> {
        let links = Links::new(world, dw);
        handle::alloc_obj(|sw| Column {
            base: RefCell::new(DynamicObjectBase::alloc_zeroed_with(sw)),
            st: RefCell::new(ColumnState {
                item_type: 0,
                paint_color: 0,
                current_configuration: 0,
                ice_melt_timer: 0.0,
            }),
            links,
        })
    }

    /// `-[Column initWithWorld:dynamicWorld:atPosition:cache:type:saveDict:placedByClient:]`.
    /// Order: base, itemType, `initSubDerivedItems` (rand), `updateConfiguration`, owner, paint.
    #[allow(clippy::too_many_arguments)]
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
        me.init_sub_derived_items();
        me.update_configuration(&me_obj);
        if let Some(o) = owner_from_client_or_save(client_id, save_dict) {
            me.base.borrow_mut().owner_id = Some(o);
        }
        me.st.borrow_mut().paint_color =
            sd_get(save_dict, "paintColor").map_or(0, PlistValue::unsigned_int_value) as u16;
        Some(me_obj)
    }

    /// `-[Column initWithWorld:dynamicWorld:saveDict:cache:]`.
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
        let geti = |k: &str| save_dict.map.get(&k.to_string()).map_or(0, |v| v.int_value());
        {
            let mut st = me.st.borrow_mut();
            st.item_type = geti("itemType");
            st.current_configuration = geti("configuration");
            st.paint_color = save_dict.map.get(&"paintColor".to_string()).map_or(0, |v| v.unsigned_int_value()) as u16;
        }
        me.base.borrow_mut().owner_id = sd_string(save_dict, "ownerID");
        me.init_sub_derived_items();
        Some(me_obj)
    }

    /// `-[Column initWithWorld:dynamicWorld:cache:netData:]`.
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
        let s = ColumnCreationNetData::read_le(&padded_bytes(net_data, 0x20));
        {
            let mut st = me.st.borrow_mut();
            st.item_type = s.item_type as i32;
            st.current_configuration = s.configuration as i32;
            st.paint_color = s.paint_color;
        }
        if net_data.len() > 0x20 {
            let owner = tail_string(&read_plist_tail(net_data, 0x20), "ownerID");
            me.base.borrow_mut().owner_id = owner;
        }
        me.init_sub_derived_items();
        Some(me_obj)
    }

    /// `-[Column initSubDerivedItems]`: exactly one `rand()`.
    pub fn init_sub_derived_items(&self) {
        self.st.borrow_mut().ice_melt_timer = r01() * 5.0f32;
    }

    /// `-[Column itemType]`
    pub fn item_type(&self) -> i32 {
        self.st.borrow().item_type
    }

    /// `-[Column isTransparent]` (tables only, no server caller).
    /// Answered: {244, 266, 268, 334..=338}.
    pub fn is_transparent(&self) -> bool {
        matches!(self.st.borrow().item_type, 244 | 266 | 268 | 334..=338)
    }

    /// `-[Column paint:]`.
    pub fn paint(&self, _me: &ObjRef, color_index: u16) {
        self.st.borrow_mut().paint_color = color_index;
        if !self.base.borrow().is_net {
            self.base.borrow_mut().update_needs_to_be_sent = true;
            let pos = self.base.borrow().pos;
            self.links.dw().dynamic_world_changed_at_pos_object_type(pos, DYNAMIC_OBJECT_TYPE_COLUMN);
        }
    }

    /// `-[Column updateConfiguration]`.
    pub fn update_configuration(&self, _me: &ObjRef) {
        let pos = self.base.borrow().pos;
        let world = self.links.world();
        let has_up =
            tile_at_world_position_loaded(pos.x, pos.y + 1, &world).map(|t| t.foreground_contents() == CONTENTS_COLUMN);
        let has_down =
            tile_at_world_position_loaded(pos.x, pos.y - 1, &world).map(|t| t.foreground_contents() == CONTENTS_COLUMN);
        let up = has_up.unwrap_or(false);
        let down = has_down.unwrap_or(false);
        let new = if up {
            if !down {
                COLUMN_PLINTH_BELOW
            } else {
                COLUMN_NO_PLINTH
            }
        } else if down {
            COLUMN_PLINTH_ABOVE
        } else {
            COLUMN_PLINTH_ABOVE_AND_BELOW
        };
        let changed = {
            let mut st = self.st.borrow_mut();
            if new != st.current_configuration {
                st.current_configuration = new;
                true
            } else {
                false
            }
        };
        if changed && !self.base.borrow().is_net {
            self.base.borrow_mut().update_needs_to_be_sent = true;
            self.links.dw().dynamic_world_changed_at_pos_object_type(pos, DYNAMIC_OBJECT_TYPE_COLUMN);
        }
    }
}

impl DynamicObject for Column {
    fn base(&self) -> &RefCell<DynamicObjectBase> {
        &self.base
    }
    fn class_info(&self) -> &'static ClassInfo {
        &COLUMN_CLASS
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn object_type(&self) -> i32 {
        DYNAMIC_OBJECT_TYPE_COLUMN
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
        let (it, cfg, pc) = {
            let st = self.st.borrow();
            (st.item_type, st.current_configuration, st.paint_color)
        };
        d.map.insert("itemType".to_string(), PlistValue::int(it as i64));
        d.map.insert("configuration".to_string(), PlistValue::int(cfg as i64));
        d.map.insert("paintColor".to_string(), PlistValue::int(pc as i64));
        if let Some(o) = self.base.borrow().owner_id.clone() {
            d.map.insert("ownerID".to_string(), PlistValue::string(o));
        }
        d
    }

    fn creation_net_data_for_client(&self, me: &ObjRef, _client_id: Option<&str>) -> Option<Vec<u8>> {
        let st = self.st.borrow();
        let s = ColumnCreationNetData {
            dynamic_object_net_data: base_dynamic_object_net_data(me),
            item_type: st.item_type as u16,
            configuration: st.current_configuration as u8,
            padding_a: 0,
            paint_color: st.paint_color,
            padding: [0; 2],
        };
        let mut out = s.to_vec();
        let owner = self.base.borrow().owner_id.clone();
        append_plist_tail(&mut out, &[("ownerID", owner.as_deref())]);
        Some(out)
    }
    fn update_net_data_for_client(&self, me: &ObjRef, client_id: Option<&str>) -> Option<Vec<u8>> {
        self.creation_net_data_for_client(me, client_id)
    }

    fn remote_update(&self, me: &ObjRef, net_data: &[u8]) {
        base_remote_update(me, net_data);
        let u = ColumnCreationNetData::read_le(&padded_bytes(net_data, 0x20));
        let mut st = self.st.borrow_mut();
        if u.configuration as i32 != st.current_configuration {
            st.current_configuration = u.configuration as i32;
        }
        if u.paint_color != st.paint_color {
            st.paint_color = u.paint_color;
        }
    }

    fn update_accurate_dt_is_simulation(&self, me: &ObjRef, dt: f32, _accurate_dt: f32, _is_simulation: bool) {
        let dw = self.links.dw();
        let (nr, pos) = {
            let b = self.base.borrow();
            (b.needs_removed, b.pos)
        };
        if !dw.is_client() && !nr && self.item_type() == ITEM_ICE_COLUMN {
            let mut timer = self.st.borrow().ice_melt_timer;
            ice_melt_step(me, &self.links, pos, dt, &mut timer);
            self.st.borrow_mut().ice_melt_timer = timer;
        }
    }

    /// `-[Column worldChanged:]`.
    fn world_changed(&self, me: &ObjRef, positions: &mut Vec<IntPair>) {
        let world = self.links.world();
        let pos = self.base.borrow().pos;
        for p in positions.iter() {
            let dx = wrap_dx(p.x.wrapping_sub(pos.x), world.world_width_macro());
            if dx.abs() < 1 && pos.y - 1 <= p.y && p.y <= pos.y + 1 {
                self.update_configuration(me);
                if self.st.borrow().current_configuration == COLUMN_PLINTH_ABOVE_AND_BELOW {
                    let below = tile_at_world_position_loaded(pos.x, pos.y - 1, &world);
                    if let Some(b) = below {
                        if !tile_is_solid(Some(b)) && b.foreground_contents() != CONTENTS_COLUMN {
                            let above = tile_at_world_position_loaded(pos.x, pos.y + 1, &world);
                            if let Some(a) = above {
                                if !tile_is_solid(Some(a)) && a.foreground_contents() != CONTENTS_COLUMN {
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
                }
            }
        }
    }
}

/// Shared offline test environment: a real `World` (4 macro tiles wide, macro blocks x 0..4, y 0..2 loaded, all air with
/// open back walls) and a real `DynamicWorld` (NullHooks, so free blocks are not materialised).
#[cfg(test)]
pub(crate) mod testkit {
    use super::*;
    use crate::dynworld::state::testutil::{dbs, scratch};
    use crate::dynworld::state::DynamicWorldInit;
    use crate::lighting::artificial_light::testkit::world_with_blocks;

    pub fn env() -> (Rc<World>, Rc<DynamicWorld>) {
        let mut blocks = vec![];
        for mx in 0..4 {
            for my in 0..2 {
                blocks.push((mx, my));
            }
        }
        let world = world_with_blocks(4, &blocks);
        let (main, dwdb) = dbs("bo");
        let dir = scratch("bo_save");
        let dw = DynamicWorld::new(DynamicWorldInit {
            world: world.clone(),
            world_tile_loader: None,
            server: None,
            server_clients: None,
            cache: None,
            tree_density_noise_function: None,
            season_offset_noise_function: None,
            app_database: None,
            world_database: main,
            dynamic_object_database: dwdb,
            world_save_directory_override: Some(dir),
        });
        (world, dw)
    }

    pub fn tile(w: &World, x: i32, y: i32) -> crate::world::state::TileRef {
        crate::lighting::artificial_light::testkit::tile(w, x, y)
    }

    /// Make the tile solid (type 1) or air (type 2).
    pub fn solid(w: &World, x: i32, y: i32, s: bool) {
        tile(w, x, y).set_type_index(if s { 1 } else { 2 });
    }

    pub fn pos(x: i32, y: i32) -> IntPair {
        IntPair { x, y }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrap_dx_table() {
        assert_eq!(wrap_dx(70, 4), -58);
        assert_eq!(wrap_dx(-70, 4), 58);
        assert_eq!(wrap_dx(63, 4), 63);
        assert_eq!(wrap_dx(-64, 4), -64);
        assert_eq!(wrap_dx(64, 4), -64);
        assert_eq!(wrap_dx(0, 4), 0);
    }

    use super::testkit::*;
    use crate::rng::glibc_rand::{rand, srand};

    fn mk_column(w: &Rc<World>, d: &Rc<DynamicWorld>, x: i32, y: i32, item: i32, owner: Option<&str>) -> ObjRef {
        Column::init_with_world_dynamic_world_at_position_cache_type_save_dict_placed_by_client(
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

    /// transparent item set.
    #[test]
    fn is_transparent_set() {
        let (w, d) = env();
        for (item, exp) in
            [(244, true), (266, true), (268, true), (334, true), (338, true), (339, false), (222, false), (333, false)]
        {
            let c = mk_column(&w, &d, 40, 20, item, None);
            assert_eq!(handle::downcast::<Column>(&c).unwrap().is_transparent(), exp, "item {item}");
        }
    }

    /// : the four plinth cases.
    #[test]
    fn update_configuration_cases() {
        let (w, d) = env();
        let set_col =
            |x: i32, y: i32, on: bool| tile(&w, x, y).set_foreground_contents(if on { CONTENTS_COLUMN } else { 0 });
        let c = mk_column(&w, &d, 40, 20, 222, None);
        let col = handle::downcast::<Column>(&c).unwrap();
        assert_eq!(col.st.borrow().current_configuration, COLUMN_PLINTH_ABOVE_AND_BELOW);
        assert!(c.update_needs_to_be_sent());
        set_col(40, 21, true);
        col.update_configuration(&c);
        assert_eq!(col.st.borrow().current_configuration, COLUMN_PLINTH_BELOW);
        set_col(40, 19, true);
        col.update_configuration(&c);
        assert_eq!(col.st.borrow().current_configuration, COLUMN_NO_PLINTH);
        set_col(40, 21, false);
        col.update_configuration(&c);
        assert_eq!(col.st.borrow().current_configuration, COLUMN_PLINTH_ABOVE);
    }

    /// Exactly one rand() in each of the three init paths.
    #[test]
    fn one_rand_per_init() {
        let (w, d) = env();
        srand(7);
        let first: Vec<i32> = (0..4).map(|_| rand()).collect();
        srand(7);
        let c = mk_column(&w, &d, 40, 20, 222, Some("o"));
        assert_eq!(rand(), first[1]);
        let sd = c.get_save_dict(&c);
        srand(7);
        let _c2 = Column::init_with_world_dynamic_world_save_dict_cache(&w, &d, &sd, None).unwrap();
        assert_eq!(rand(), first[1]);
        let net = c.creation_net_data_for_client(&c, None).unwrap();
        srand(7);
        let _c3 = Column::init_with_world_dynamic_world_cache_net_data(&w, &d, None, &net).unwrap();
        assert_eq!(rand(), first[1]);
    }

    /// Net layout (0x20 + tail), zero padding, owner round trip, isNet semantics.
    #[test]
    fn net_round_trip() {
        let (w, d) = env();
        let c = mk_column(&w, &d, 40, 20, 222, Some("owner-1"));
        let col = handle::downcast::<Column>(&c).unwrap();
        col.paint(&c, 9);
        let bytes = c.creation_net_data_for_client(&c, None).unwrap();
        assert!(bytes.len() > 0x20);
        assert!(bytes[0x11..0x18].iter().all(|b| *b == 0));
        assert!(bytes[0x1b] == 0 && bytes[0x1e..0x20].iter().all(|b| *b == 0));
        assert_eq!(&bytes[0x18..0x1a], &222u16.to_le_bytes());
        assert_eq!(&bytes[0x1c..0x1e], &9u16.to_le_bytes());
        assert_eq!(c.update_net_data_for_client(&c, None).unwrap(), bytes);
        let c2 = Column::init_with_world_dynamic_world_cache_net_data(&w, &d, None, &bytes).unwrap();
        assert!(c2.is_net());
        assert_eq!(c2.owner_id().as_deref(), Some("owner-1"));
        let col2 = handle::downcast::<Column>(&c2).unwrap();
        assert_eq!(col2.item_type(), 222);
        assert_eq!(col2.st.borrow().paint_color, 9);
        // no owner: the tail is still present
        let c3 = mk_column(&w, &d, 41, 20, 222, None);
        let b3 = c3.creation_net_data_for_client(&c3, None).unwrap();
        assert!(b3.len() > 0x20);
        let c4 = Column::init_with_world_dynamic_world_cache_net_data(&w, &d, None, &b3[..0x20]).unwrap();
        assert!(c4.owner_id().is_none());
    }

    #[test]
    fn save_dict_keys_and_reload() {
        let (w, d) = env();
        let c = mk_column(&w, &d, 40, 20, 244, Some("o1"));
        let sd = c.get_save_dict(&c);
        for k in ["itemType", "configuration", "paintColor", "ownerID", "uniqueID", "pos_x", "pos_y", "floatPos"] {
            assert!(sd.map.get(&k.to_string()).is_some(), "{k}");
        }
        assert_eq!(sd.map.len(), 8);
        let c2 = Column::init_with_world_dynamic_world_save_dict_cache(&w, &d, &sd, None).unwrap();
        let col2 = handle::downcast::<Column>(&c2).unwrap();
        assert_eq!(col2.item_type(), 244);
        assert_eq!(col2.st.borrow().current_configuration, COLUMN_PLINTH_ABOVE_AND_BELOW);
        assert_eq!(c2.owner_id().as_deref(), Some("o1"));
    }

    /// A column with no neighbouring columns above/below and non-solid above and below drops as a free block.
    #[test]
    fn floating_column_drops() {
        let (w, d) = env();
        let c = mk_column(&w, &d, 40, 20, 222, None);
        tile(&w, 40, 20).set_foreground_contents(CONTENTS_COLUMN);
        assert!(!c.needs_removed());
        c.world_changed(&c, &mut vec![pos(40, 21)]);
        assert!(c.needs_removed());
        assert_eq!(tile(&w, 40, 20).foreground_contents(), 0);
        // a supported column stays
        let c2 = mk_column(&w, &d, 50, 20, 222, None);
        solid(&w, 50, 19, true);
        c2.world_changed(&c2, &mut vec![pos(50, 19)]);
        assert!(!c2.needs_removed());
    }

    #[test]
    fn tail_round_trip() {
        let mut out = vec![0u8; 0x20];
        append_plist_tail(&mut out, &[("ownerID", Some("abc"))]);
        assert!(out.len() > 0x20);
        let d = read_plist_tail(&out, 0x20);
        assert_eq!(tail_string(&d, "ownerID").as_deref(), Some("abc"));
        assert_eq!(tail_string(&d, "other"), None);
        // empty dict still has a gzip tail
        let mut e = vec![0u8; 0x20];
        append_plist_tail(&mut e, &[("ownerID", None)]);
        assert!(e.len() > 0x20 + 10);
        let d = read_plist_tail(&e, 0x20);
        assert!(matches!(d, Some(PlistValue::Dict(_))));
        assert_eq!(tail_string(&d, "ownerID"), None);
        // exactly the header: nil plist
        assert!(read_plist_tail(&[0u8; 0x20], 0x20).is_none());
        // short data raises
        let r = crate::foundation::exception::catch_ns(|| read_plist_tail(&[0u8; 0x10], 0x20));
        let e = r.expect_err("raises");
        assert_eq!(e.name, "NSRangeException");
        assert_eq!(e.reason_or_null(), "Range location + length too great");
    }
}

#[cfg(test)]
mod more_tests {
    use super::testkit::*;
    use super::*;

    fn mk(w: &Rc<World>, d: &Rc<DynamicWorld>, x: i32, y: i32, item: i32) -> ObjRef {
        Column::init_with_world_dynamic_world_at_position_cache_type_save_dict_placed_by_client(
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
    fn helpers_and_trait_answers() {
        assert_eq!(tail_string(&None, "k"), None);
        let e = crate::foundation::exception::catch_ns(|| tail_string(&Some(PlistValue::int(1)), "k")).unwrap_err();
        assert_eq!(e.name, "NSInvalidArgumentException");
        ub_crash_or_return();
        let (w, d) = env();
        let c = mk(&w, &d, 40, 20, 222);
        let _ = c.class_info();
        assert_eq!(c.freeblock_creation_item_type(), 222);
        assert!(c.free_block_creation_save_dict(&c).is_some());
        // an update from the net overrides configuration and colour
        let mut net = c.creation_net_data_for_client(&c, None).unwrap();
        net[0x1a] = COLUMN_NO_PLINTH as u8;
        net[0x1c] = 7;
        c.remote_update(&c, &net);
        let col = handle::downcast::<Column>(&c).unwrap();
        assert_eq!((col.st.borrow().current_configuration, col.st.borrow().paint_color), (COLUMN_NO_PLINTH, 7));
        // the same values again change nothing
        c.remote_update(&c, &net);
        // a net object on a tile that is not loaded cannot be created
        let mut lost = c.creation_net_data_for_client(&c, None).unwrap();
        lost[12..16].copy_from_slice(&300u32.to_le_bytes());
        assert!(Column::init_with_world_dynamic_world_cache_net_data(&w, &d, None, &lost).is_none());
    }

    #[test]
    fn ice_columns_melt_when_it_is_warm() {
        let (w, d) = env();
        let c = mk(&w, &d, 40, 20, ITEM_ICE_COLUMN);
        let col = handle::downcast::<Column>(&c).unwrap();
        // not yet time
        col.st.borrow_mut().ice_melt_timer = 0.0;
        c.update_accurate_dt_is_simulation(&c, 0.1, 0.1, false);
        assert!(!c.needs_removed());
        // time, but cold
        tile(&w, 40, 20).update(|t| t.artificial_heat = -3000);
        c.update_accurate_dt_is_simulation(&c, 6.0, 6.0, false);
        assert!(!c.needs_removed());
        // warm
        tile(&w, 40, 20).update(|t| t.artificial_heat = 0);
        c.update_accurate_dt_is_simulation(&c, 6.0, 6.0, false);
        assert!(c.needs_removed());
        // a removed column does nothing, nor does a non-ice one
        c.update_accurate_dt_is_simulation(&c, 6.0, 6.0, false);
        let plain = mk(&w, &d, 50, 20, 222);
        plain.update_accurate_dt_is_simulation(&plain, 6.0, 6.0, false);
        assert!(!plain.needs_removed());
        // an ice column on a tile that is not there
        let lost = mk(&w, &d, 41, 20, ITEM_ICE_COLUMN);
        lost.base().borrow_mut().pos = pos(41, 300);
        handle::downcast::<Column>(&lost).unwrap().st.borrow_mut().ice_melt_timer = 9.0;
        lost.update_accurate_dt_is_simulation(&lost, 1.0, 1.0, false);
    }

    #[test]
    fn floating_columns_in_odd_surroundings() {
        let (w, d) = env();
        let set = |x: i32, y: i32| tile(&w, x, y).set_foreground_contents(CONTENTS_COLUMN);
        // already being removed: stays put
        let a = mk(&w, &d, 40, 20, 222);
        set(40, 20);
        a.set_needs_removed(&a, true);
        a.world_changed(&a, &mut vec![pos(40, 20)]);
        assert_eq!(tile(&w, 40, 20).foreground_contents(), CONTENTS_COLUMN);
        // solid below, solid above, nothing loaded below or above
        let b = mk(&w, &d, 50, 20, 222);
        solid(&w, 50, 19, true);
        b.world_changed(&b, &mut vec![pos(50, 20)]);
        solid(&w, 50, 19, false);
        solid(&w, 50, 21, true);
        b.world_changed(&b, &mut vec![pos(50, 20)]);
        assert!(!b.needs_removed());
        let low = mk(&w, &d, 60, 0, 222);
        low.world_changed(&low, &mut vec![pos(60, 0)]);
        let high = mk(&w, &d, 60, 63, 222);
        high.world_changed(&high, &mut vec![pos(60, 63)]);
        // a neighbour column keeps it
        let near = mk(&w, &d, 80, 20, 222);
        set(80, 21);
        near.world_changed(&near, &mut vec![pos(80, 21)]);
        assert!(!near.needs_removed());
        // a change that is not next to the column
        let far = mk(&w, &d, 70, 20, 222);
        far.world_changed(&far, &mut vec![pos(75, 20), pos(70, 30)]);
        assert!(!far.needs_removed());
    }
}

/// Random neighbourhoods for the building tests.
#[cfg(test)]
pub(crate) mod fuzz {
    use super::testkit::*;
    use super::*;

    pub struct Lcg(pub u64);
    impl Lcg {
        pub fn next(&mut self) -> u64 {
            self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            self.0 >> 33
        }
        pub fn pick<T: Copy>(&mut self, v: &[T]) -> T {
            v[(self.next() % v.len() as u64) as usize]
        }
    }

    /// Sets the 5x5 tiles around `(x, y)` that exist to random types, contents, foreground contents and back walls.
    pub fn scramble(w: &World, r: &mut Lcg, x: i32, y: i32, types: &[u8], contents: &[u8], fg: &[u8]) {
        for dy in -2..=2 {
            for dx in -2..=2 {
                if y + dy < 0 || y + dy >= 64 {
                    continue;
                }
                let (ty, c, f, bw) = (r.pick(types), r.pick(contents), r.pick(fg), r.pick(&[2u8, 6, 6]));
                tile(w, x + dx, y + dy).update(|t| {
                    t.type_index = ty;
                    t.contents = c;
                    t.foreground_contents = f;
                    t.back_wall_type_index = bw;
                });
            }
        }
    }
}
