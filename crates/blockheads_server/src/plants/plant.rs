//! owned by plants
//!
//! `Plant: DynamicObject` (instance size 0x88): the shared base of every crop, flower, kelp and vine, plus the
//! type tables, the shared environment/light helpers and the plant factories.
//!
//! Plant: DynamicObject (Plant.mm)` (6.1 to 6.4).
//!
//! ## Shape
//! * [`PlantLike`] is the virtual surface of `Plant`. The base bodies are free functions
//!   `plant_*(me,...)` so that subclasses can `[super...]`-chain explicitly.
//! * Borrow discipline: every call out of a plant (`dw.*`, tile edits, virtuals on `me`) happens
//!   with no `RefCell` borrow held.

use std::cell::RefCell;
use std::rc::Rc;

use crate::dynworld::state::DynamicWorld;
use crate::fmath::cconv::f32_to_i32;
use crate::fmath::vector::clampi;
use crate::objects::base::{
    base_dynamic_object_net_data, base_get_save_dict, base_init_with_world_dynamic_world_at_position_cache,
    base_init_with_world_dynamic_world_cache_net_data, base_init_with_world_dynamic_world_save_dict_cache,
    base_remote_update, base_remove_from_macro_block, padded_bytes, ub_null_tile_deref, DynamicObject, DynamicWorldRef,
    WorldRef,
};
use crate::objects::class_info::{ClassInfo, DYNAMIC_OBJECT_CLASS};
use crate::objects::handle::{CacheRef, ObjRef};
use crate::plist::value::{PlistDict, PlistValue};
use crate::rng::glibc_rand;
use crate::types::netdata::PlantCreationNetData;
use crate::types::structs::{IntPair, WireStruct};
use crate::types::tile::Tile;
use crate::world::climate::{current_temperature_for_tile_at_world_pos, season_for_world_x};
use crate::world::helpers::{tile_is_air_water_or_snow, tile_is_plant};
use crate::world::state::{TileRef, World};
use crate::world::tiles::{tile_at_world_position, tile_at_world_position_loaded};
use crate::worldgen::noise::NoiseFunction;

use super::kelp::KelpPlant;
use super::normal_plant::NormalPlant;
use super::simple_plants::{kind_for_object_type, kind_for_plant_type};
use super::tulip::TulipPlant;
use super::vine::VinePlant;

/// `Plant` class descriptor.
pub static PLANT_CLASS: ClassInfo =
    ClassInfo { name: "Plant", superclass: Some(&DYNAMIC_OBJECT_CLASS), protocols: &[] };

/// `PlantType` maximum (TOMATO).
pub const MAX_PLANT_TYPE: i32 = 10;

/// `PlantType` values.
pub const PLANT_NOTHING: i32 = 0;
pub const PLANT_FLAX: i32 = 1;
pub const PLANT_SUNFLOWER: i32 = 2;
pub const PLANT_CORN: i32 = 3;
pub const PLANT_CARROT: i32 = 4;
pub const PLANT_CHILLI: i32 = 5;
pub const PLANT_KELP: i32 = 6;
pub const PLANT_VINE: i32 = 7;
pub const PLANT_TULIP: i32 = 8;
pub const PLANT_WHEAT: i32 = 9;
pub const PLANT_TOMATO: i32 = 10;

/// `DynamicObjectType` of the plant classes.
pub const DYNAMIC_OBJECT_TYPE_KELP_PLANT: i32 = 34;
pub const DYNAMIC_OBJECT_TYPE_VINE_PLANT: i32 = 58;
pub const DYNAMIC_OBJECT_TYPE_TULIP_PLANT: i32 = 59;

/// `Plant` ivars 0x4e..0x88.
/// Alloc state: everything 0 / None / false. `growth_rate` is ONLY set by the at-position init (R4).
#[derive(Clone, Default)]
pub struct PlantState {
    /// 0x4e, clamped 1..255
    pub max_age_gene: u16,
    /// 0x50, clamped 1..255
    pub growth_rate_gene: u16,
    /// 0x58 (assign; stored, never read by any plant)
    pub tree_density_noise_function: Option<Rc<NoiseFunction>>,
    /// 0x60 (assign)
    pub season_offset_noise_function: Option<Rc<NoiseFunction>>,
    /// 0x68
    pub season_offset: i32,
    /// 0x6c
    pub age: f32,
    /// 0x70 (only Kelp ever writes it, via its update)
    pub frozen: bool,
    /// 0x74
    pub gather_progress: i32,
    /// 0x78
    pub has_flowered_this_season: bool,
    /// 0x79
    pub flowering: bool,
    /// 0x7c (set by the subclass `initSubDerivedItems`)
    pub max_age: f32,
    /// 0x80
    pub growth_rate: f32,
    /// 0x84
    pub age_counter: f32,
}

/// Every `Plant` selector another class may send. Defaults are `Plant`'s own bodies.
pub trait PlantLike: DynamicObject {
    /// The `Plant` ivars.
    fn plant_state(&self) -> &RefCell<PlantState>;
    /// The concrete `World` this plant was constructed with.
    fn plant_world(&self) -> Rc<World>;
    /// The concrete `DynamicWorld` this plant was constructed with.
    fn plant_dw(&self) -> Rc<DynamicWorld>;

    /// `-plantType` (Plant: `PLANT_NOTHING`).
    fn plant_type(&self) -> i32 {
        PLANT_NOTHING
    }
    /// `-isRequiredSoilType:` (Plant: dirt/compost family).
    fn is_required_soil_type(&self, tile_type: u8) -> bool {
        matches!(tile_type, 6 | 27 | 28 | 48 | 49 | 50)
    }
    /// `-tileIsKindOfSelf:` (Plant: NO).
    fn tile_is_kind_of_self(&self, _tile: &Tile) -> bool {
        false
    }
    /// `-tileHarvested:removeBlockhead:correctToolMultiplier:` (Plant: remove the plant, 0 items).
    fn tile_harvested_remove_blockhead_correct_tool_multiplier(
        &self,
        me: &ObjRef,
        _tile_pos: IntPair,
        _remove_blockhead: Option<&ObjRef>,
        _correct_tool_multiplier: i32,
    ) -> i32 {
        plant_remove_plant_without_creating_freeblocks(me);
        0
    }
    /// `-gatherProgressForTile:` (Plant: the plain ivar).
    fn gather_progress_for_tile(&self, _tile_pos: IntPair) -> i32 {
        self.plant_state().borrow().gather_progress
    }
    /// `-setGatherProgress:forTile:` (Plant: the plain ivar).
    fn set_gather_progress_for_tile(&self, v: i32, _tile_pos: IntPair) {
        self.plant_state().borrow_mut().gather_progress = v;
    }
    /// `-droppedItemType` (Plant: `ITEM_NONE`).
    fn dropped_item_type(&self) -> i32 {
        0
    }
    /// `-canBreed` (Plant: NO).
    fn can_breed(&self) -> bool {
        false
    }
    /// `-numberOfOccupiedTilesAbove` (Plant: 0).
    fn number_of_occupied_tiles_above(&self) -> i32 {
        0
    }
    /// `-numberOfOccupiedTilesBelow` (Plant: 0).
    fn number_of_occupied_tiles_below(&self) -> i32 {
        0
    }
    /// `-isGrowingInCompost`: tile BELOW the plant in {48,49,50} (Vine overrides with the head tile).
    fn is_growing_in_compost(&self, me: &ObjRef) -> bool {
        let p = me.pos();
        let w = self.plant_world();
        match tile_at_world_position_loaded(p.x, p.y.wrapping_sub(1), &w) {
            Some(t) => matches!(t.type_index(), 0x30..=0x32),
            None => false,
        }
    }
    /// `-setFlowering:` (Plant: the plain ivar; NormalPlant/Tulip override).
    fn set_flowering(&self, _me: &ObjRef, v: bool) {
        self.plant_state().borrow_mut().flowering = v;
    }
    /// `-isFlowering`
    fn is_flowering(&self) -> bool {
        self.plant_state().borrow().flowering
    }
    /// `-availableFood` (Normal, Kelp, Tulip, Vine implement).
    fn available_food(&self) -> f32 {
        0.0
    }
    /// `-setAvailableFood:`
    fn set_available_food(&self, _v: f32) {}
    /// `-plantCreationNetData` (Plant default; Kelp/Vine/Tulip extend it).
    fn plant_creation_net_data(&self, me: &ObjRef) -> PlantCreationNetData {
        plant_plant_creation_net_data(me)
    }
    /// `-maxAgeGeneVariation`: 1 draw.
    fn max_age_gene_variation(&self) -> u16 {
        let g = self.plant_state().borrow().max_age_gene;
        gene_variation(g, "plants::plant::maxAgeGeneVariation")
    }
    /// `-growthRateGeneVariation`: 1 draw.
    fn growth_rate_gene_variation(&self) -> u16 {
        let g = self.plant_state().borrow().growth_rate_gene;
        gene_variation(g, "plants::plant::growthRateGeneVariation")
    }
    /// `-removePlantWithoutCreatingFreeblocks` (non-virtual in ObjC).
    fn remove_plant_without_creating_freeblocks(&self, me: &ObjRef) {
        plant_remove_plant_without_creating_freeblocks(me);
    }
}

/// `clampi((int)((float)gene + (R01 - 0.5f) * 8.0f), 1, 255)`; one draw.
fn gene_variation(gene: u16, tag: &'static str) -> u16 {
    let r = glibc_rand::r01_tagged(tag);
    clampi(f32_to_i32((gene as f32) + (r - 0.5f32) * 8.0f32), 1, 255) as u16
}

/// `[o isKindOfClass:Plant]` as a trait object.
pub fn as_plant(o: &ObjRef) -> Option<&dyn PlantLike> {
    let a = o.as_any();
    if let Some(p) = a.downcast_ref::<NormalPlant>() {
        return Some(p);
    }
    if let Some(p) = a.downcast_ref::<KelpPlant>() {
        return Some(p);
    }
    if let Some(p) = a.downcast_ref::<TulipPlant>() {
        return Some(p);
    }
    if let Some(p) = a.downcast_ref::<VinePlant>() {
        return Some(p);
    }
    None
}

/// Internal: `as_plant` that must succeed (the base bodies are only ever reached from plant classes).
fn pl(me: &ObjRef) -> &dyn PlantLike {
    as_plant(me).expect("plant base body called on a non-plant object")
}

// ---------------------------------------------------------------- tile / dict helpers shared by the plant files

/// `tileAtWorldPositionLoaded(x, y, world)` (loads the block on demand).
pub fn tile_loaded(w: &World, x: i32, y: i32) -> Option<TileRef> {
    tile_at_world_position_loaded(x, y, w)
}

/// `tileAtWorldPosition(x, y, [world macroTiles], world)` (`tileAtRaw`, never loads).
pub fn tile_raw(w: &World, x: i32, y: i32) -> Option<TileRef> {
    tile_at_world_position(x, y, &w.macro_tiles(), w)
}

/// UB-PARITY: the original dereferences a NULL tile here. Under feature `faithful-crashes` this
/// raises SIGSEGV, otherwise it returns and the caller skips the statement.
#[inline]
pub fn ub_parity_null_tile() {
    ub_null_tile_deref();
}

/// `[[d objectForKey:k] intValue]` (missing -> 0).
pub fn dict_int(d: &PlistDict, k: &str) -> i32 {
    d.get(&k.to_string()).map_or(0, |v| v.int_value())
}
/// `[[d objectForKey:k] floatValue]` (missing -> 0).
pub fn dict_float(d: &PlistDict, k: &str) -> f32 {
    d.get(&k.to_string()).map_or(0.0, |v| v.float_value())
}
/// `[[d objectForKey:k] doubleValue]` (missing -> 0).
pub fn dict_double(d: &PlistDict, k: &str) -> f64 {
    d.get(&k.to_string()).map_or(0.0, |v| v.double_value())
}
/// `[[d objectForKey:k] boolValue]` (missing -> NO).
pub fn dict_bool(d: &PlistDict, k: &str) -> bool {
    d.get(&k.to_string()).map_or(false, |v| v.bool_value())
}
/// `d[k] = [NSNumber numberWithInt:v]`
pub fn put_int(d: &mut PlistDict, k: &str, v: i32) {
    d.map.insert(k.to_string(), PlistValue::int(v as i64));
}
/// `d[k] = [NSNumber numberWithFloat:v]`
pub fn put_float(d: &mut PlistDict, k: &str, v: f32) {
    d.map.insert(k.to_string(), PlistValue::real_f32(v));
}
/// `d[k] = [NSNumber numberWithBool:v]`
pub fn put_bool(d: &mut PlistDict, k: &str, v: bool) {
    d.map.insert(k.to_string(), PlistValue::boolean(v));
}

/// `[dynamicWorld dynamicWorldChangedAtPos:pos objectType:[self objectType]]`.
pub fn dw_changed(me: &ObjRef) {
    if let Some(dw) = as_plant(me).map(|p| p.plant_dw()) {
        dw.dynamic_world_changed_at_pos_object_type(me.pos(), me.object_type());
    }
}

/// `self.updateNeedsToBeSent = YES`.
pub fn mark_update_needed(me: &ObjRef) {
    me.base().borrow_mut().update_needs_to_be_sent = true;
}

// ---------------------------------------------------------------- shared environment / light helpers

/// Season index of the per-plant offset (eighth of the year).
///  (D). `idx = (int)(8.0f*s + 4.0f + 256.0f) % 8` with `s = seasonForX + 0.003f * (float)(offset - 127)`.
pub fn season_index(season_for_x: f32, season_offset: i32) -> i32 {
    let season = season_for_x + 0.003f32 * (season_offset.wrapping_sub(127) as f32);
    let t = 8.0f32 * season;
    let t = t + 4.0f32;
    let t = t + 256.0f32;
    f32_to_i32(t) % 8
}

/// `((float)R / 4.0f + (float)(G >> 2) + (float)(B >> 1)) / 1024.0f`.
pub fn artificial_light_value(tile: &Tile) -> f32 {
    ((tile.artificial_light_r as f32) / 4.0f32
        + ((tile.artificial_light_g >> 2) as f32)
        + ((tile.artificial_light_b >> 1) as f32))
        / 1024.0f32
}

/// The environment check of NormalPlant (C)/(init) and Tulip: not water, combined light >= 0.05f and
/// `min_temperature <= currentTemperature`.
/// A NULL tile is a UB-PARITY crash in the original (R6); here it counts as ok.
pub fn plant_environment_ok(pos: IntPair, tile: Option<&TileRef>, world: &Rc<World>, min_temperature: f32) -> bool {
    let weather = world.get_weather_fraction_for_pos(pos);
    let t1 = world.world_time();
    let day = world.get_day_night_fraction_for_x_at_world_time(pos.x as f32, t1);
    let t2 = world.world_time();
    let season = season_for_world_x(pos.x, t2, Some(world));
    let Some(tr) = tile else {
        ub_parity_null_tile();
        return true;
    };
    let t = tr.get();
    let art = artificial_light_value(&t);
    let d: f64 = if (day as f64) <= 0.2f64 { 0.2f64 } else { day as f64 };
    let combined = ((t.sun_light as f32) as f64 * d * 2.0f64 / 255.0f64 + (art as f64)) as f32;
    if t.type_index == 3 || !(combined >= 0.05f32) {
        return false;
    }
    let temp = current_temperature_for_tile_at_world_pos(&t, pos, season, day, weather, Some(world));
    min_temperature <= temp
}

/// `(weather, dayNight, season)` as the adult-plant inits of Kelp/Vine compute them: `getWeatherFractionForPos:`,
/// `getDayNightFractionForX:atWorldTime:` (first `worldTime`), `seasonForWorldX(x, worldTime, world)` (second
/// `worldTime`).
pub fn adult_environment(world: &Rc<World>, pos: IntPair) -> (f32, f32, f32) {
    let weather = world.get_weather_fraction_for_pos(pos);
    let t = world.world_time();
    let day = world.get_day_night_fraction_for_x_at_world_time(pos.x as f32, t);
    let season = season_for_world_x(pos.x, world.world_time(), Some(world));
    (weather, day, season)
}

/// `g = (float)((double)(((1.0f - (float)N/16.0f) + 0.2f) * growthRate) * 0.5 + 0.5)` shared by Kelp and Vine
/// (`growthRateLocal`).
pub fn growth_rate_local(n: i32, growth_rate: f32) -> f32 {
    let inner = ((1.0f32 - (n as f32) / 16.0f32) + 0.2f32) * growth_rate;
    ((inner as f64) * 0.5f64 + 0.5f64) as f32
}

/// `[dynamicWorld createFreeBlockAtPosition:pos ofType:item dataA:a dataB:b subItems:nil dynamicObjectSaveDict:nil
/// hovers:NO playSound:play priorityBlockhead:prio]`.
pub fn create_free_block(
    dw: &DynamicWorld,
    pos: IntPair,
    item: i32,
    a: u16,
    b: u16,
    play_sound: i32,
    prio: Option<&ObjRef>,
) {
    dw.create_free_block_at_position_of_type_data_a_data_b_sub_items_dynamic_object_save_dict_hovers_play_sound_priority_blockhead(
        pos,
        crate::types::item_type::ItemType(item as u32),
        a,
        b,
        None,
        None,
        false,
        play_sound,
        prio,
    );
}

// ---------------------------------------------------------------- Plant base bodies

/// Shared allocation/registration prologue of the at-position init failure path:
/// `[self removeFromMacroBlock]; [self release]`.
fn fail_release(me: &ObjRef) {
    me.remove_from_macro_block(me);
    me.dealloc(me);
}

/// `-[Plant initWithWorld:dynamicWorld:atPosition:cache:maxAgeGene:growthRateGene:treeDensityNoiseFunction:seasonOffsetNoiseFunction:adultPlant:]`
/// `false` is the original's `return nil`. `adultPlant` is unused here.
#[allow(clippy::too_many_arguments)]
pub fn plant_init_with_world_dynamic_world_at_position_cache_max_age_gene_growth_rate_gene_tree_density_noise_function_season_offset_noise_function_adult_plant(
    me: &ObjRef,
    world: &Rc<World>,
    dw: &Rc<DynamicWorld>,
    pos: IntPair,
    cache: CacheRef,
    max_age_gene: u16,
    growth_rate_gene: u16,
    tdnf: Option<Rc<NoiseFunction>>,
    sonf: Option<Rc<NoiseFunction>>,
    _adult_plant: bool,
) -> bool {
    let wref: WorldRef = world.clone();
    let dref: DynamicWorldRef = dw.clone();
    if !base_init_with_world_dynamic_world_at_position_cache(me, &wref, &dref, pos, cache) {
        return false;
    }
    let p = me.pos();
    let tile = tile_loaded(world, p.x, p.y);
    let blocked = match tile {
        None => true,
        Some(t) => t.foreground_contents() != 0,
    };
    if blocked {
        let mut found = false;
        if tile_is_plant(tile) && dw.get_plant_at_pos(p).is_none() {
            // stale plant tile with no plant object: clear it and allow creation
            if let Some(t) = tile {
                t.set_foreground_contents(0);
            }
            dw.world_contents_changed_at_pos(p);
            found = true;
        }
        if !found {
            fail_release(me);
            return false;
        }
    }
    {
        let mut st = pl(me).plant_state().borrow_mut();
        st.tree_density_noise_function = tdnf;
        st.season_offset_noise_function = sonf;
        st.max_age_gene = clampi(max_age_gene as i32, 1, 255) as u16;
        st.growth_rate_gene = clampi(growth_rate_gene as i32, 1, 255) as u16;
        st.growth_rate = (st.growth_rate_gene as f32) / 255.0f32 + 0.2f32;
    }
    dw_changed(me);
    true
}

/// `-[Plant loadSaveDictValues:]`.
pub fn plant_load_save_dict_values(me: &ObjRef, d: &PlistDict) {
    let p = pl(me);
    let world = p.plant_world();
    let mut st = p.plant_state().borrow_mut();
    st.season_offset = dict_int(d, "seasonOffset");
    st.age = dict_float(d, "age");
    st.gather_progress = dict_int(d, "gatherProgress");
    st.has_flowered_this_season = dict_bool(d, "hasFloweredThisSeason");
    st.flowering = dict_bool(d, "flowering");
    st.frozen = dict_bool(d, "frozen");
    st.max_age_gene = dict_int(d, "maxAgeGene") as u16;
    st.growth_rate_gene = dict_int(d, "growthRateGene") as u16;
    st.max_age_gene = clampi(st.max_age_gene as i32, 1, 255) as u16;
    st.growth_rate_gene = clampi(st.growth_rate_gene as i32, 1, 255) as u16;
    let save_time = dict_double(d, "saveTime");
    let now = world.world_time();
    if 1800.0f64 < (now - save_time) {
        st.has_flowered_this_season = false;
    }
}

/// `-[Plant initWithWorld:dynamicWorld:saveDict:cache:treeDensityNoiseFunction:seasonOffsetNoiseFunction:]`.
/// `growthRate` is NOT restored (R4).
pub fn plant_init_with_world_dynamic_world_save_dict_cache_tree_density_noise_function_season_offset_noise_function(
    me: &ObjRef,
    world: &Rc<World>,
    dw: &Rc<DynamicWorld>,
    save_dict: &PlistDict,
    cache: CacheRef,
    tdnf: Option<Rc<NoiseFunction>>,
    sonf: Option<Rc<NoiseFunction>>,
) -> bool {
    let wref: WorldRef = world.clone();
    let dref: DynamicWorldRef = dw.clone();
    if !base_init_with_world_dynamic_world_save_dict_cache(me, &wref, &dref, save_dict, cache) {
        return false;
    }
    {
        let mut st = pl(me).plant_state().borrow_mut();
        st.tree_density_noise_function = tdnf;
        st.season_offset_noise_function = sonf;
    }
    plant_load_save_dict_values(me, save_dict);
    dw_changed(me);
    true
}

/// `-[Plant initWithWorld:dynamicWorld:cache:netData:]` (client replica path).
pub fn plant_init_with_world_dynamic_world_cache_net_data(
    me: &ObjRef,
    world: &Rc<World>,
    dw: &Rc<DynamicWorld>,
    cache: CacheRef,
    net_data: &[u8],
) -> bool {
    let wref: WorldRef = world.clone();
    let dref: DynamicWorldRef = dw.clone();
    if !base_init_with_world_dynamic_world_cache_net_data(me, &wref, &dref, cache, net_data) {
        return false;
    }
    let s = PlantCreationNetData::read_le(&padded_bytes(net_data, 0x28));
    let mut st = pl(me).plant_state().borrow_mut();
    st.age = s.age as f32;
    st.flowering = s.flowering != 0;
    true
}

/// `-[Plant getSaveDict]`.
pub fn plant_get_save_dict(me: &ObjRef) -> PlistDict {
    let p = pl(me);
    let mut d = base_get_save_dict(me);
    let now = p.plant_world().world_time();
    let st = p.plant_state().borrow();
    d.map.insert("saveTime".to_string(), PlistValue::real_f64(now));
    put_int(&mut d, "seasonOffset", st.season_offset);
    put_float(&mut d, "age", st.age);
    put_int(&mut d, "gatherProgress", st.gather_progress);
    put_bool(&mut d, "hasFloweredThisSeason", st.has_flowered_this_season);
    put_bool(&mut d, "flowering", st.flowering);
    put_bool(&mut d, "frozen", st.frozen);
    put_int(&mut d, "maxAgeGene", st.max_age_gene as i32);
    put_int(&mut d, "growthRateGene", st.growth_rate_gene as i32);
    put_float(&mut d, "maxAge", st.max_age);
    put_float(&mut d, "growthRate", st.growth_rate);
    d
}

/// `-[Plant plantCreationNetData]`: only `dynamicObjectNetData`, `age` and `flowering` are written;
/// the rest is 0 (R3).
pub fn plant_plant_creation_net_data(me: &ObjRef) -> PlantCreationNetData {
    let st = pl(me).plant_state().borrow();
    PlantCreationNetData {
        dynamic_object_net_data: base_dynamic_object_net_data(me),
        age: f32_to_i32(st.age) as u16,
        flowering: st.flowering as u8,
        ..PlantCreationNetData::default()
    }
}

/// `-[Plant creationNetDataForClient:]` / `-[Plant updateNetDataForClient:]`: the same 0x28-byte blob via the
/// (virtual) `plantCreationNetData`.
pub fn plant_creation_net_data_for_client(me: &ObjRef) -> Option<Vec<u8>> {
    let s = pl(me).plant_creation_net_data(me);
    Some(s.to_vec())
}

/// `-[Plant remoteUpdate:]` (client path, kept for the net protocol).
pub fn plant_remote_update(me: &ObjRef, net_data: &[u8]) {
    base_remote_update(me, net_data);
    let u = PlantCreationNetData::read_le(&padded_bytes(net_data, 0x28));
    let p = pl(me);
    p.plant_state().borrow_mut().age = u.age as f32;
    p.set_flowering(me, u.flowering != 0);
}

/// `-[Plant worldChanged:]`.
pub fn plant_world_changed(me: &ObjRef, positions: &mut Vec<IntPair>) {
    let p = pl(me);
    if me.needs_removed() || p.plant_type() == PLANT_VINE {
        return;
    }
    let world = p.plant_world();
    let list: Vec<IntPair> = positions.clone();
    for q in list {
        let above = p.number_of_occupied_tiles_above();
        let pos = me.pos();
        if q.x == pos.x && q.y <= pos.y.wrapping_add(above) && pos.y.wrapping_sub(1) <= q.y {
            let soil = tile_raw(&world, pos.x, pos.y.wrapping_sub(1));
            if let Some(s) = soil {
                if !p.is_required_soil_type(s.type_index()) {
                    p.tile_harvested_remove_blockhead_correct_tool_multiplier(me, pos, None, 1);
                    return;
                }
            }
            let mut bad = false;
            let mut i = 0;
            while i < above {
                let ty = pos.y.wrapping_add(i).wrapping_add(1);
                let t = tile_raw(&world, pos.x, ty);
                if t.is_some() && (!tile_is_air_water_or_snow(t) || bad) {
                    p.tile_harvested_remove_blockhead_correct_tool_multiplier(me, IntPair { x: pos.x, y: ty }, None, 1);
                    bad = true;
                }
                i += 1;
            }
        }
    }
}

/// `-[Plant update:accurateDT:isSimulation:]` (ageing flush).
pub fn plant_update(me: &ObjRef, dt: f32, _accurate_dt: f32, _is_simulation: bool) {
    let p = pl(me);
    let (is_net, frozen) = (me.is_net(), p.plant_state().borrow().frozen);
    if is_net || frozen {
        return;
    }
    let flush = {
        let mut st = p.plant_state().borrow_mut();
        st.age_counter += dt;
        if 1000.0f32 * dt < st.age_counter {
            st.age += st.age_counter;
            true
        } else {
            false
        }
    };
    if !flush {
        return;
    }
    let world = p.plant_world();
    let pos = me.pos();
    let below = tile_raw(&world, pos.x, pos.y.wrapping_sub(1));
    if let Some(b) = below {
        if matches!(b.type_index(), 0x30..=0x32) {
            let mut st = p.plant_state().borrow_mut();
            st.age += st.age_counter;
        }
    }
    let (max_age, age) = {
        let st = p.plant_state().borrow();
        (st.max_age, st.age)
    };
    if max_age <= age && p.is_growing_in_compost(me) {
        p.plant_state().borrow_mut().age = ((max_age as f64) - 0.1f64) as f32;
    }
    p.plant_state().borrow_mut().age_counter = 0.0;
    dw_changed(me);
    mark_update_needed(me);
}

/// `-[Plant clearAllTileContents]`.
pub fn plant_clear_all_tile_contents(me: &ObjRef) {
    let p = pl(me);
    let world = p.plant_world();
    let dw = p.plant_dw();
    let pos = me.pos();
    if let Some(t) = tile_loaded(&world, pos.x, pos.y) {
        if p.tile_is_kind_of_self(&t.get()) {
            t.set_foreground_contents(0);
            dw.world_contents_changed_at_pos(pos);
        }
    }
    let below = p.number_of_occupied_tiles_below();
    let mut i = -1i32 - below;
    while i < p.number_of_occupied_tiles_above() {
        let y = pos.y.wrapping_add(i).wrapping_add(1);
        if let Some(t) = tile_loaded(&world, pos.x, y) {
            if p.tile_is_kind_of_self(&t.get()) {
                t.set_foreground_contents(0);
                dw.world_contents_changed_at_pos(IntPair { x: pos.x, y });
            }
        }
        i += 1;
    }
}

/// `-[Plant setNeedsRemoved:]`.
pub fn plant_set_needs_removed(me: &ObjRef, v: bool) {
    if v && !me.needs_removed() {
        plant_clear_all_tile_contents(me);
    }
    me.base().borrow_mut().needs_removed = v;
}

/// `-[Plant removeFromMacroBlock]`.
pub fn plant_remove_from_macro_block(me: &ObjRef) {
    plant_clear_all_tile_contents(me);
    base_remove_from_macro_block(me);
}

/// `-[Plant removePlantWithoutCreatingFreeblocks]`: `setNeedsRemoved:YES` (virtual), notify, dirty.
pub fn plant_remove_plant_without_creating_freeblocks(me: &ObjRef) {
    me.set_needs_removed(me, true);
    dw_changed(me);
    mark_update_needed(me);
}

/// `-[Plant maxAgeGeneVariation]`.
pub fn plant_max_age_gene_variation(me: &ObjRef) -> u16 {
    pl(me).max_age_gene_variation()
}
/// `-[Plant growthRateGeneVariation]`.
pub fn plant_growth_rate_gene_variation(me: &ObjRef) -> u16 {
    pl(me).growth_rate_gene_variation()
}

/// Plant base `-isGrowingInCompost`.
pub fn plant_is_growing_in_compost(me: &ObjRef) -> bool {
    pl(me).is_growing_in_compost(me)
}

// ---------------------------------------------------------------- type tables and factories

/// `plant_object_type_for_plant_type`: PlantType -> DynamicObjectType (0 for others).
pub fn plant_object_type_for_plant_type(plant_type: i32) -> i32 {
    match plant_type {
        1 => 10,
        2 => 11,
        3 => 12,
        4 => 27,
        5 => 33,
        6 => 34,
        7 => 58,
        8 => 59,
        9 => 61,
        10 => 62,
        _ => 0,
    }
}

/// ` loadPlantAtPosition` class table: construct the plant of `plant_type`. Unknown types give `None`.
/// The `i16` genes are passed on as `u16` (C implicit conversion).
#[allow(clippy::too_many_arguments)]
pub fn new_plant_for_type(
    plant_type: i32,
    world: &Rc<World>,
    dw: &Rc<DynamicWorld>,
    pos: IntPair,
    cache: CacheRef,
    max_age_gene: i16,
    growth_rate_gene: i16,
    tdnf: Option<Rc<NoiseFunction>>,
    sonf: Option<Rc<NoiseFunction>>,
    adult: bool,
) -> Option<ObjRef> {
    let mag = max_age_gene as u16;
    let grg = growth_rate_gene as u16;
    match plant_type {
        6 => KelpPlant::init_with_world_dynamic_world_at_position_cache_max_age_gene_growth_rate_gene_tree_density_noise_function_season_offset_noise_function_adult_plant(
            world, dw, pos, cache, mag, grg, tdnf, sonf, adult,
        )
        .map(|k| k as ObjRef),
        7 => VinePlant::init_with_world_dynamic_world_at_position_cache_max_age_gene_growth_rate_gene_tree_density_noise_function_season_offset_noise_function_adult_plant(
            world, dw, pos, cache, mag, grg, tdnf, sonf, adult,
        )
        .map(|k| k as ObjRef),
        8 => TulipPlant::init_with_world_dynamic_world_at_position_cache_max_age_gene_growth_rate_gene_tree_density_noise_function_season_offset_noise_function_adult_plant(
            world, dw, pos, cache, mag, grg, tdnf, sonf, adult,
        )
        .map(|k| k as ObjRef),
        t => {
            let kind = kind_for_plant_type(t)?;
            NormalPlant::init_with_world_dynamic_world_at_position_cache_max_age_gene_growth_rate_gene_tree_density_noise_function_season_offset_noise_function_adult_plant(
                kind, world, dw, pos, cache, mag, grg, tdnf, sonf, adult,
            )
            .map(|k| k as ObjRef)
        }
    }
}

/// DynamicWorld load from DB: construct the plant of `object_type` from its save dict. Other types give
/// `None`.
pub fn new_plant_from_save_dict(
    object_type: i32,
    world: &Rc<World>,
    dw: &Rc<DynamicWorld>,
    save_dict: &PlistDict,
    cache: CacheRef,
    tdnf: Option<Rc<NoiseFunction>>,
    sonf: Option<Rc<NoiseFunction>>,
) -> Option<ObjRef> {
    match object_type {
        34 => KelpPlant::init_with_world_dynamic_world_save_dict_cache_tree_density_noise_function_season_offset_noise_function(
            world, dw, save_dict, cache, tdnf, sonf,
        )
        .map(|k| k as ObjRef),
        58 => VinePlant::init_with_world_dynamic_world_save_dict_cache_tree_density_noise_function_season_offset_noise_function(
            world, dw, save_dict, cache, tdnf, sonf,
        )
        .map(|k| k as ObjRef),
        59 => TulipPlant::init_with_world_dynamic_world_save_dict_cache_tree_density_noise_function_season_offset_noise_function(
            world, dw, save_dict, cache, tdnf, sonf,
        )
        .map(|k| k as ObjRef),
        t => {
            let kind = kind_for_object_type(t)?;
            NormalPlant::init_with_world_dynamic_world_save_dict_cache_tree_density_noise_function_season_offset_noise_function(
                kind, world, dw, save_dict, cache, tdnf, sonf,
            )
            .map(|k| k as ObjRef)
        }
    }
}

/// Client-replica path (`initWithWorld:dynamicWorld:cache:netData:`); the server never constructs plants this way
/// but `object_registry` keeps the slot.
pub fn new_plant_from_net_data(
    object_type: i32,
    world: &Rc<World>,
    dw: &Rc<DynamicWorld>,
    cache: CacheRef,
    net_data: &[u8],
) -> Option<ObjRef> {
    match object_type {
        34 => KelpPlant::init_with_net_data(world, dw, cache, net_data).map(|k| k as ObjRef),
        58 => VinePlant::init_with_net_data(world, dw, cache, net_data).map(|k| k as ObjRef),
        59 => TulipPlant::init_with_net_data(world, dw, cache, net_data).map(|k| k as ObjRef),
        t => {
            let kind = kind_for_object_type(t)?;
            NormalPlant::init_with_net_data(kind, world, dw, cache, net_data).map(|k| k as ObjRef)
        }
    }
}

/// Shared `DynamicObject` pieces of every plant class (identical bodies).
macro_rules! plant_dynamic_object_common {
    () => {
        fn base(&self) -> &std::cell::RefCell<crate::objects::base::DynamicObjectBase> {
            &self.base
        }
        fn as_any(&self) -> &dyn std::any::Any {
            self
        }
        fn occupies_foreground_contents(&self) -> bool {
            true
        }
        fn creation_net_data_for_client(
            &self,
            me: &crate::objects::handle::ObjRef,
            _client_id: Option<&str>,
        ) -> Option<Vec<u8>> {
            crate::plants::plant::plant_creation_net_data_for_client(me)
        }
        fn update_net_data_for_client(
            &self,
            me: &crate::objects::handle::ObjRef,
            _client_id: Option<&str>,
        ) -> Option<Vec<u8>> {
            crate::plants::plant::plant_creation_net_data_for_client(me)
        }
        fn world_changed(
            &self,
            me: &crate::objects::handle::ObjRef,
            positions: &mut Vec<crate::types::structs::IntPair>,
        ) {
            crate::plants::plant::plant_world_changed(me, positions)
        }
    };
}
pub(crate) use plant_dynamic_object_common;

#[cfg(test)]
mod tests {
    use super::*;

    ///  U2: `season_index` equals the three-step f32 formula; table at offset 127.
    #[test]
    fn season_index_table() {
        // year fraction f at offset 127 -> (floor(8f)+4) mod 8
        for k in 0..8 {
            let f = (k as f32 + 0.5) / 8.0;
            assert_eq!(season_index(f, 127), (k + 4) % 8);
        }
        let mut f = 0.0f32;
        while f < 1.0 {
            for off in -64..64 {
                let s = f + 0.003f32 * ((off - 127) as f32);
                let mut t = 8.0f32 * s;
                t += 4.0;
                t += 256.0;
                assert_eq!(season_index(f, off), (t as i32) % 8);
            }
            f += 0.0137;
        }
    }

    /// : the artificial light combination (R/4 float, G>>2 and B>>1 integer shifts).
    #[test]
    fn artificial_light() {
        let mut t = Tile::default();
        t.artificial_light_r = 1024;
        t.artificial_light_g = 1024;
        t.artificial_light_b = 1024;
        assert_eq!(artificial_light_value(&t), (256.0 + 256.0 + 512.0) / 1024.0);
        t.artificial_light_g = 3;
        t.artificial_light_b = 1;
        assert_eq!(artificial_light_value(&t), (256.0 + 0.0 + 0.0) / 1024.0);
    }

    /// : PlantType -> DynamicObjectType.
    #[test]
    fn type_map() {
        let v: Vec<i32> = (0..12).map(plant_object_type_for_plant_type).collect();
        assert_eq!(v, vec![0, 10, 11, 12, 27, 33, 34, 58, 59, 61, 62, 0]);
    }

    ///  U5: gene variation clamps and the single draw.
    #[test]
    fn gene_variation_clamps() {
        crate::rng::glibc_rand::srand(1);
        for g in [1u16, 2, 128, 254, 255] {
            for _ in 0..200 {
                let v = gene_variation(g, "test");
                assert!((1..=255).contains(&v));
                assert!((v as i32 - g as i32).abs() <= 4);
            }
        }
        crate::rng::glibc_rand::srand(7);
        let r = crate::rng::glibc_rand::r01_from(crate::rng::glibc_rand::rand());
        crate::rng::glibc_rand::srand(7);
        assert_eq!(gene_variation(100, "t"), clampi(f32_to_i32(100.0 + (r - 0.5) * 8.0), 1, 255) as u16);
    }
}

/// Integration tests on a real `World` + `DynamicWorld` (one macro column, blocks (0,0) and (0,1) loaded).
#[cfg(test)]
mod integration_tests {
    use super::*;
    use crate::dynworld::state::{testutil, DwHooks, DynamicWorldInit, FreeBlockInit};
    use crate::lighting::artificial_light::testkit::world_with_blocks;
    use crate::plants::kelp::KelpPlant;
    use crate::plants::normal_plant::NormalPlant;
    use crate::plants::simple_plants::NormalPlantKind;
    use crate::plants::tulip::TulipPlant;
    use crate::plants::vine::VinePlant;
    use std::cell::RefCell as Cell_;

    /// Records every free block creation `(pos, item, a, b, play_sound-less)`.
    #[derive(Default)]
    struct Rec {
        blocks: Cell_<Vec<(IntPair, u32, u16, u16)>>,
    }
    impl DwHooks for Rec {
        fn freeblock_from_position(&self, init: FreeBlockInit<'_>) -> Option<ObjRef> {
            self.blocks.borrow_mut().push((init.position, init.item_type, init.data_a, init.data_b));
            None
        }
    }

    struct Env {
        world: Rc<World>,
        dw: Rc<DynamicWorld>,
        rec: Rc<Rec>,
    }

    /// Ground (dirt, type 6) for y < 10, sunlit air above; a water column at x = 20 for y in 10..30.
    fn env(name: &str) -> Env {
        let world = world_with_blocks(1, &[(0, 0), (0, 1)]);
        let (main, dwdb) = testutil::dbs(name);
        let dir = testutil::scratch(&format!("{name}_save"));
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
        let rec = Rc::new(Rec::default());
        dw.set_hooks(rec.clone());
        for x in 0..32 {
            for y in 0..64 {
                let t = tile_raw(&world, x, y).unwrap();
                t.update(|t| {
                    if y < 10 {
                        t.type_index = 6;
                    } else {
                        t.type_index = 2;
                        t.sun_light = 255;
                    }
                });
            }
        }
        for y in 10..30 {
            tile_raw(&world, 20, y).unwrap().update(|t| t.type_index = 3);
        }
        Env { world, dw, rec }
    }

    fn ip(x: i32, y: i32) -> IntPair {
        IntPair { x, y }
    }

    fn flax(e: &Env, x: i32, adult: bool) -> Rc<NormalPlant> {
        NormalPlant::init_with_world_dynamic_world_at_position_cache_max_age_gene_growth_rate_gene_tree_density_noise_function_season_offset_noise_function_adult_plant(
            NormalPlantKind::Flax,
            &e.world,
            &e.dw,
            ip(x, 10),
            None,
            191,
            188,
            None,
            None,
            adult,
        )
        .expect("flax")
    }

    fn next_rand_after(seed: u32, skip: usize) -> i32 {
        glibc_rand::srand(seed);
        for _ in 0..skip {
            glibc_rand::rand();
        }
        glibc_rand::rand()
    }

    /// U6 + U3: non-adult NormalPlant init = 1 draw; tile written; maxAge and genes set; growthRate set.
    #[test]
    fn normal_plant_init_non_adult() {
        let e = env("pl_init");
        glibc_rand::srand(11);
        let p = flax(&e, 5, false);
        assert_eq!(glibc_rand::rand(), next_rand_after(11, 1));
        let t = tile_raw(&e.world, 5, 10).unwrap();
        assert_eq!(t.foreground_contents(), 0x43);
        let st = p.plant.borrow();
        assert_eq!(st.max_age_gene, 191);
        assert_eq!(st.growth_rate_gene, 188);
        assert_eq!(st.growth_rate, 188.0 / 255.0 + 0.2);
        assert!(st.max_age > 0.0 && st.max_age < 40000.0);
        let food = p.np.borrow().available_food;
        assert!((0.0..1710.0).contains(&food));
        assert_eq!(p.object_type(), 10);
        assert!(p.class_info().is_kind_of(&PLANT_CLASS));
        assert!(p.is_required_soil_type(7) && !p.is_required_soil_type(3));
    }

    /// Plant init fails (and draws nothing) when the tile is already occupied.
    #[test]
    fn plant_init_blocked_tile() {
        let e = env("pl_blocked");
        tile_raw(&e.world, 6, 10).unwrap().set_foreground_contents(0x30);
        glibc_rand::srand(3);
        let p = NormalPlant::init_with_world_dynamic_world_at_position_cache_max_age_gene_growth_rate_gene_tree_density_noise_function_season_offset_noise_function_adult_plant(
            NormalPlantKind::Corn,
            &e.world,
            &e.dw,
            ip(6, 10),
            None,
            100,
            100,
            None,
            None,
            false,
        );
        assert!(p.is_none());
        assert_eq!(glibc_rand::rand(), next_rand_after(3, 0));
    }

    /// U4: `Plant.update` flushes `age` on tick 1001 with dt 0.2 (f32 sum), not before.
    #[test]
    fn plant_update_flush_tick_1001() {
        let e = env("pl_flush");
        let p = flax(&e, 5, false);
        let me: ObjRef = p.clone();
        p.plant.borrow_mut().max_age = 1.0e9;
        for i in 0..1001 {
            assert_eq!(p.plant.borrow().age, 0.0, "tick {i}");
            plant_update(&me, 0.2, 0.2, false);
        }
        let age = p.plant.borrow().age;
        let mut sum = 0.0f32;
        for _ in 0..1001 {
            sum += 0.2f32;
        }
        assert_eq!(age, sum);
        assert!((age - 200.19809).abs() < 1e-3);
        assert_eq!(p.plant.borrow().age_counter, 0.0);
        assert!(me.update_needs_to_be_sent());
    }

    /// U4: compost below doubles the flush and clamps to `maxAge - 0.1`.
    #[test]
    fn plant_update_compost() {
        let e = env("pl_compost");
        tile_raw(&e.world, 5, 9).unwrap().set_type_index(48);
        let p = flax(&e, 5, false);
        let me: ObjRef = p.clone();
        for _ in 0..1001 {
            plant_update(&me, 0.2, 0.2, false);
        }
        let mut sum = 0.0f32;
        for _ in 0..1001 {
            sum += 0.2f32;
        }
        assert_eq!(p.plant.borrow().age, sum + sum);
        // clamp
        p.plant.borrow_mut().max_age = 100.0;
        for _ in 0..1001 {
            plant_update(&me, 0.2, 0.2, false);
        }
        assert_eq!(p.plant.borrow().age, (100.0f32 as f64 - 0.1) as f32);
    }

    /// U8: harvest of a flowering flax at full age: own seed (non-expert) + n flower seeds (2 draws each) +
    /// foliage; plant removed, tiles cleared.
    #[test]
    fn normal_plant_harvest() {
        let e = env("pl_harvest");
        let p = flax(&e, 5, false);
        let me: ObjRef = p.clone();
        // flowering flax with the flower in the tile above
        {
            let mut st = p.plant.borrow_mut();
            st.age = 20000.0;
            st.flowering = true;
        }
        tile_raw(&e.world, 5, 11).unwrap().set_foreground_contents(0x44);
        assert_eq!(p.number_of_occupied_tiles_above(), 1);
        assert_eq!(p.dropped_item_type(), 54);
        e.rec.blocks.borrow_mut().clear();
        glibc_rand::srand(21);
        let n = p.tile_harvested_remove_blockhead_correct_tool_multiplier(&me, ip(5, 10), None, 3);
        // ageFactor = 0.8 + 20000/14400 = 2.18; n = clampi((int)(2.18*3 = 6.5), 1, 5) = 5
        assert_eq!(n, 1 + 5 + 5);
        let b = e.rec.blocks.borrow();
        assert_eq!(b.len(), 11);
        assert_eq!(b[0], (ip(5, 10), 54, 191, 188));
        assert_eq!(b[1].0, ip(5, 11));
        assert_eq!(b[6], (ip(5, 10), 55, 0, 0));
        // 2 draws per flower seed
        assert_eq!(glibc_rand::rand(), next_rand_after(21, 10));
        assert!(me.needs_removed());
        assert_eq!(tile_raw(&e.world, 5, 10).unwrap().foreground_contents(), 0);
        assert_eq!(tile_raw(&e.world, 5, 11).unwrap().foreground_contents(), 0);
    }

    /// U12 + section 6: save dict key set and round trip; `growthRate` is 0 after a reload (R4).
    #[test]
    fn save_dict_round_trip() {
        let e = env("pl_save");
        let p = flax(&e, 5, false);
        let me: ObjRef = p.clone();
        p.plant.borrow_mut().age = 1234.5;
        let d = me.get_save_dict(&me);
        for k in [
            "uniqueID",
            "pos_x",
            "pos_y",
            "floatPos",
            "saveTime",
            "seasonOffset",
            "age",
            "gatherProgress",
            "hasFloweredThisSeason",
            "flowering",
            "frozen",
            "maxAgeGene",
            "growthRateGene",
            "maxAge",
            "growthRate",
            "availableFood",
        ] {
            assert!(d.get(&k.to_string()).is_some(), "missing {k}");
        }
        assert!(d.get(&"lightDict".to_string()).is_none());
        assert_eq!(dict_float(&d, "growthRate"), 188.0 / 255.0 + 0.2);
        // reload (the old plant leaves its tile in place; clear it so the new init accepts the spot)
        me.remove_from_macro_block(&me);
        let q = new_plant_from_save_dict(10, &e.world, &e.dw, &d, None, None, None).expect("reload");
        let d2 = q.get_save_dict(&q);
        assert_eq!(dict_float(&d2, "growthRate"), 0.0);
        assert_eq!(dict_float(&d2, "age"), 1234.5);
        assert_eq!(dict_float(&d2, "availableFood"), dict_float(&d, "availableFood"));
        assert_eq!(dict_int(&d2, "maxAgeGene"), 191);
        assert_eq!(dict_int(&d2, "uniqueID"), dict_int(&d, "uniqueID"));
        assert!((dict_float(&d2, "maxAge") - dict_float(&d, "maxAge")).abs() < 1.0);
    }

    /// U13: `plant_creation_net_data` layout: 0x28 bytes, age low 16 bits, flowering byte, creation == update.
    #[test]
    fn net_data() {
        let e = env("pl_net");
        let p = flax(&e, 5, false);
        let me: ObjRef = p.clone();
        {
            let mut st = p.plant.borrow_mut();
            st.age = 70000.7;
            st.flowering = true;
        }
        let c = me.creation_net_data_for_client(&me, None).unwrap();
        assert_eq!(c.len(), 0x28);
        assert_eq!(u16::from_le_bytes([c[0x18], c[0x19]]), 70000u32 as u16);
        assert_eq!(c[0x22], 1);
        assert_eq!(c, me.update_net_data_for_client(&me, None).unwrap());
    }

    /// U14: `worldChanged:` removes the plant when its soil disappears.
    #[test]
    fn world_changed_soil_gone() {
        let e = env("pl_wc");
        let p = flax(&e, 5, false);
        let me: ObjRef = p.clone();
        let mut pos = vec![ip(9, 9)];
        me.world_changed(&me, &mut pos);
        assert!(!me.needs_removed());
        tile_raw(&e.world, 5, 9).unwrap().set_type_index(2);
        let mut pos = vec![ip(5, 9)];
        me.world_changed(&me, &mut pos);
        assert!(me.needs_removed());
        assert_eq!(tile_raw(&e.world, 5, 10).unwrap().foreground_contents(), 0);
    }

    fn kelp(e: &Env, grg: u16) -> Rc<KelpPlant> {
        KelpPlant::init_with_world_dynamic_world_at_position_cache_max_age_gene_growth_rate_gene_tree_density_noise_function_season_offset_noise_function_adult_plant(
            &e.world,
            &e.dw,
            ip(20, 10),
            None,
            193,
            grg,
            None,
            None,
            false,
        )
        .expect("kelp")
    }

    /// U6/U9: kelp non-adult init = 2 draws (wave + food); root tile 0x51; N = 0.
    #[test]
    fn kelp_init_and_growth() {
        let e = env("kelp_g");
        // root needs soil below: tile (20,9) is dirt (6)
        glibc_rand::srand(5);
        let k = kelp(&e, 100);
        assert_eq!(glibc_rand::rand(), next_rand_after(5, 2));
        let me: ObjRef = k.clone();
        assert_eq!(tile_raw(&e.world, 20, 10).unwrap().foreground_contents(), 0x51);
        assert_eq!(k.number_of_occupied_tiles_above(), 0);
        assert!((k.plant.borrow().max_age - 7899.019).abs() < 0.01);
        k.plant.borrow_mut().frozen = false;
        // grow: period 225/g seconds
        let g = growth_rate_local(0, k.plant.borrow().growth_rate);
        let ticks = (225.0 / g) as usize + 2;
        for _ in 0..ticks {
            me.update_accurate_dt_is_simulation(&me, 1.0, 1.0, false);
        }
        assert_eq!(k.number_of_occupied_tiles_above(), 1);
        assert_eq!(tile_raw(&e.world, 20, 11).unwrap().foreground_contents(), 0x51);
        assert!(me.update_needs_to_be_sent());
    }

    /// U9: interior harvest keeps `N = dy - 1`, drops the stalk above (cascade draws) and the harvested block.
    #[test]
    fn kelp_harvest_interior() {
        let e = env("kelp_h");
        let k = kelp(&e, 100);
        let me: ObjRef = k.clone();
        for y in 11..14 {
            tile_raw(&e.world, 20, y).unwrap().set_foreground_contents(0x51);
        }
        k.kelp.borrow_mut().number_of_occupied_tiles_above = 3;
        k.plant.borrow_mut().age = 1000.0;
        assert_eq!(me.pos(), ip(20, 10));
        e.rec.blocks.borrow_mut().clear();
        glibc_rand::srand(8);
        let n = k.tile_harvested_remove_blockhead_correct_tool_multiplier(&me, ip(20, 11), None, 1);
        assert_eq!(n, 1);
        assert_eq!(k.number_of_occupied_tiles_above(), 0);
        assert_eq!(tile_raw(&e.world, 20, 11).unwrap().foreground_contents(), 0);
        assert_eq!(tile_raw(&e.world, 20, 12).unwrap().foreground_contents(), 0);
        assert_eq!(tile_raw(&e.world, 20, 13).unwrap().foreground_contents(), 0);
        assert_eq!(tile_raw(&e.world, 20, 10).unwrap().foreground_contents(), 0x51);
        assert!(!me.needs_removed());
        // 2 draws for the harvested block; then 2 stalk tiles: 1 coin each (+2 when it drops)
        assert!(e.rec.blocks.borrow().len() >= 1);
        // getPlantAtPos-style occupancy
        assert_eq!(k.number_of_occupied_tiles_above(), 0);
    }

    /// U9 + R4: a reloaded kelp has growthRate 0 -> period 450 s.
    #[test]
    fn kelp_reload_growth_rate_zero() {
        let e = env("kelp_r");
        let k = kelp(&e, 100);
        let me: ObjRef = k.clone();
        let d = me.get_save_dict(&me);
        assert_eq!(dict_int(&d, "numberOfOccupiedTilesAbove"), 0);
        assert!(d.get(&"growthTimer".to_string()).is_some());
        me.remove_from_macro_block(&me);
        tile_raw(&e.world, 20, 10).unwrap().set_foreground_contents(0);
        let q = new_plant_from_save_dict(34, &e.world, &e.dw, &d, None, None, None).expect("reload");
        let d2 = q.get_save_dict(&q);
        assert_eq!(dict_float(&d2, "growthRate"), 0.0);
        assert_eq!(dict_int(&d2, "numberOfOccupiedTilesAbove"), 0);
    }

    /// U10: vine init (non-adult: 0 init draws besides the food draw), growth, and harvest from the head.
    #[test]
    fn vine_growth_and_harvest() {
        let e = env("vine_g");
        // head at (5,10) must be soil: make it dirt; below (5,9..) is air
        tile_raw(&e.world, 5, 10).unwrap().set_type_index(6);
        for y in 5..10 {
            tile_raw(&e.world, 5, y).unwrap().update(|t| {
                t.type_index = 2;
                t.sun_light = 255;
            });
        }
        glibc_rand::srand(2);
        let v = VinePlant::init_with_world_dynamic_world_at_position_cache_max_age_gene_growth_rate_gene_tree_density_noise_function_season_offset_noise_function_adult_plant(
            &e.world,
            &e.dw,
            ip(5, 10),
            None,
            200,
            200,
            None,
            None,
            false,
        )
        .expect("vine");
        assert_eq!(glibc_rand::rand(), next_rand_after(2, 1));
        let me: ObjRef = v.clone();
        assert_eq!(tile_raw(&e.world, 5, 10).unwrap().foreground_contents(), 0x7C);
        let g = growth_rate_local(0, v.plant.borrow().growth_rate);
        let ticks = (900.0 / g) as usize + 2;
        for _ in 0..ticks {
            me.update_accurate_dt_is_simulation(&me, 1.0, 1.0, false);
        }
        assert_eq!(v.number_of_occupied_tiles_below(), 1);
        assert_eq!(tile_raw(&e.world, 5, 9).unwrap().foreground_contents(), 0x7C);
        // harvesting the head removes the whole vine (N becomes -1)
        v.plant.borrow_mut().age = 10.0;
        let n = v.tile_harvested_remove_blockhead_correct_tool_multiplier(&me, ip(5, 10), None, 1);
        assert_eq!(n, 1);
        assert!(me.needs_removed());
        assert_eq!(v.number_of_occupied_tiles_below(), -1);
    }

    /// U6 + U11: tulip init = 2 draws non-adult; tile 125; maxAge 99999; raw gene words; harvest tables.
    #[test]
    fn tulip_init_and_harvest() {
        let e = env("tulip_h");
        glibc_rand::srand(4);
        let t = TulipPlant::init_with_world_dynamic_world_at_position_cache_max_age_gene_growth_rate_gene_tree_density_noise_function_season_offset_noise_function_adult_plant(
            &e.world,
            &e.dw,
            ip(7, 10),
            None,
            13107,
            0,
            None,
            None,
            false,
        )
        .expect("tulip");
        assert_eq!(glibc_rand::rand(), next_rand_after(4, 2));
        let me: ObjRef = t.clone();
        assert_eq!(tile_raw(&e.world, 7, 10).unwrap().foreground_contents(), 125);
        assert_eq!(t.plant.borrow().max_age, 99999.0);
        assert_eq!(t.plant.borrow().max_age_gene, 255);
        assert_eq!(t.plant.borrow().growth_rate_gene, 1);
        assert_eq!(t.color_genes(), 13107);
        let d = me.get_save_dict(&me);
        assert_eq!(dict_int(&d, "colorGenes"), 13107);
        assert_eq!(dict_int(&d, "mixGenes"), 0);
        // non-flowering, young: 1 bulb, no seed, no draw
        e.rec.blocks.borrow_mut().clear();
        glibc_rand::srand(1);
        let n = t.tile_harvested_remove_blockhead_correct_tool_multiplier(&me, ip(7, 10), None, 1);
        assert_eq!(n, 1);
        assert_eq!(e.rec.blocks.borrow()[0], (ip(7, 10), 296, 13107, 0));
        assert_eq!(glibc_rand::rand(), next_rand_after(1, 0));
        assert!(me.needs_removed());
    }

    /// Factory table: unknown types give None; object types map to classes.
    #[test]
    fn factories() {
        let e = env("pl_fac");
        assert!(new_plant_for_type(0, &e.world, &e.dw, ip(3, 10), None, 10, 10, None, None, false).is_none());
        assert!(new_plant_for_type(11, &e.world, &e.dw, ip(3, 10), None, 10, 10, None, None, false).is_none());
        let p = new_plant_for_type(4, &e.world, &e.dw, ip(3, 10), None, 10, 10, None, None, false).unwrap();
        assert_eq!(p.object_type(), 27);
        assert_eq!(p.class_info().name, "CarrotPlant");
        assert!(as_plant(&p).is_some());
        assert_eq!(as_plant(&p).unwrap().plant_type(), 4);
        assert!(new_plant_from_save_dict(5, &e.world, &e.dw, &new_mutable_dict_for_test(), None, None, None).is_none());
    }

    fn new_mutable_dict_for_test() -> PlistDict {
        crate::objects::base::new_mutable_dict()
    }
}
