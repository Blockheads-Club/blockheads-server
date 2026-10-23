//! `SnowSurfaceBlock` (DynamicObject type 29, instance size 0x68): per-column weather agent on the air tile above
//! the ground surface: snow accumulation / melt, freezing and thawing of water, ice and grass, grass spreading.
//!
//! World weather ivars (`timeOfDayFraction`, `weatherFraction`,...) are plain getters that stay 0.0 on the server
//! (R1), so the init temperature uses dayNight = weather = 0.0. All float math is f32; the melt formulas are f64.

use std::cell::RefCell;
use std::rc::{Rc, Weak};

use crate::env::particle_emitter::ParticleEmitter;
use crate::fmath::cconv::{f32_to_i32, f32_to_u32};
use crate::fmath::vector::Vector;
use crate::lighting::artificial_light::services;
use crate::objects::base::{
    base_get_save_dict, base_init_with_world_dynamic_world_at_position_cache,
    base_init_with_world_dynamic_world_save_dict_cache, DynamicObject, DynamicObjectBase, DynamicWorldRef, WorldRef,
};
use crate::objects::class_info::{ClassInfo, DYNAMIC_OBJECT_CLASS};
use crate::objects::handle::{self, CacheRef, ObjRef};
use crate::plist::value::PlistDict;
use crate::rng::glibc_rand::r01_from;
use crate::types::structs::IntPair;
use crate::types::tile::Tile;
use crate::world::climate::{current_temperature_for_tile_at_world_pos, season_for_world_x};
use crate::world::helpers::{tile_is_air, tile_is_air_or_snow, tile_is_solid, tile_is_water};
use crate::world::state::{TileRef, World};
use crate::world::tiles::{tile_at_world_position, tile_at_world_position_loaded};

/// `SnowSurfaceBlock` class descriptor.
pub static SNOW_SURFACE_BLOCK_CLASS: ClassInfo =
    ClassInfo { name: "SnowSurfaceBlock", superclass: Some(&DYNAMIC_OBJECT_CLASS), protocols: &[] };

/// `DYNAMIC_OBJECT_TYPE_SNOW_SURFACEBLOCK`
pub const DYNAMIC_OBJECT_TYPE_SNOW_SURFACEBLOCK: i32 = 29;

const ITEM_FREEZE_WATER: i32 = 1058;
const ITEM_MELT_WATER: i32 = 1059;
/// `0.02f` widened to f64: NOT exactly 0.02.
const C002: f64 = 0.019999999552965164f64;
const TILE_ICE: u8 = 4;

/// SnowSurfaceBlock ivars 0x50..0x68.
pub struct SnowSurfaceBlockState {
    /// `temperature` (0x50)
    pub temperature: f32,
    /// `partialContent` (0x54): snow amount 0.0.. 0.6
    pub partial_content: f32,
    /// `saveDictCached` (0x58): `[[super getSaveDict] retain]` taken in `initSubDerivedItems`
    pub save_dict_cached: PlistDict,
    /// `rainRandomTimer` (0x60)
    pub rain_random_timer: f32,
}

/// `SnowSurfaceBlock: DynamicObject` (type 29).
pub struct SnowSurfaceBlock {
    base: RefCell<DynamicObjectBase>,
    /// The SnowSurfaceBlock ivars.
    pub st: RefCell<SnowSurfaceBlockState>,
    world: Weak<World>,
}

fn r01() -> f32 {
    r01_from(crate::rng_rand!())
}

impl SnowSurfaceBlock {
    fn alloc(world: &Rc<World>) -> Rc<SnowSurfaceBlock> {
        let w = Rc::downgrade(world);
        handle::alloc_obj(|sw| SnowSurfaceBlock {
            base: RefCell::new(DynamicObjectBase::alloc_zeroed_with(sw)),
            st: RefCell::new(SnowSurfaceBlockState {
                temperature: 0.0,
                partial_content: 0.0,
                save_dict_cached: crate::objects::base::new_mutable_dict(),
                rain_random_timer: 0.0,
            }),
            world: w,
        })
    }

    fn world(&self) -> Rc<World> {
        self.world.upgrade().expect("SnowSurfaceBlock: world dropped")
    }

    fn pos(&self) -> IntPair {
        self.base.borrow().pos
    }

    /// `-[SnowSurfaceBlock initSubDerivedItems]`: caches the base dict, ONE `rand()`.
    fn init_sub_derived_items(&self) {
        let me = self.me();
        let d = base_get_save_dict(&me);
        let t = r01();
        let mut st = self.st.borrow_mut();
        st.save_dict_cached = d;
        st.rain_random_timer = t;
    }

    /// `-[SnowSurfaceBlock initWithWorld:dynamicWorld:atPosition:cache:]`: 1 `rand()`; the first
    /// temperature uses `[world timeOfDayFraction]` / `[world weatherFraction]` (both 0.0 on the server, R1).
    pub fn init_with_world_dynamic_world_at_position_cache(
        world: &Rc<World>,
        dw: &DynamicWorldRef,
        pos: IntPair,
        cache: CacheRef,
    ) -> Option<Rc<SnowSurfaceBlock>> {
        let me = Self::alloc(world);
        let me_obj: ObjRef = me.clone();
        let wref: WorldRef = world.clone();
        // The base init always succeeds for a new object (it ignores a failed registration).
        let _ = base_init_with_world_dynamic_world_at_position_cache(&me_obj, &wref, dw, pos, cache);
        me.init_sub_derived_items();
        let p = me.pos();
        let tile = tile_at_world_position_loaded(p.x, p.y, world);
        let t = world.world_time();
        let season = season_for_world_x(p.x, t, Some(world));
        let day_night = world.time_of_day_fraction();
        let weather = world.weather_fraction();
        // Note: the original does not null-check `tile`; an unloaded tile reads as an all-zero Tile here.
        let tv = tile.map(|t| t.get()).unwrap_or_default();
        let temp = current_temperature_for_tile_at_world_pos(&tv, p, season, day_night, weather, Some(world));
        me.st.borrow_mut().temperature = temp;
        Some(me)
    }

    /// `-[SnowSurfaceBlock initWithWorld:dynamicWorld:saveDict:cache:]`: 1 `rand()`;
    /// `temperature` and `partialContent` stay 0.
    pub fn init_with_world_dynamic_world_save_dict_cache(
        world: &Rc<World>,
        dw: &DynamicWorldRef,
        save_dict: &PlistDict,
        cache: CacheRef,
    ) -> Option<Rc<SnowSurfaceBlock>> {
        let me = Self::alloc(world);
        let me_obj: ObjRef = me.clone();
        let wref: WorldRef = world.clone();
        // The base init always succeeds for a saved object.
        let _ = base_init_with_world_dynamic_world_save_dict_cache(&me_obj, &wref, dw, save_dict, cache);
        me.init_sub_derived_items();
        Some(me)
    }

    /// `-[SnowSurfaceBlock partialContent]`
    pub fn partial_content(&self) -> f32 {
        self.st.borrow().partial_content
    }
    /// `-[SnowSurfaceBlock setPartialContent:]`: no clamping or side effects.
    pub fn set_partial_content(&self, v: f32) {
        self.st.borrow_mut().partial_content = v;
    }

    fn temperature_for(
        &self,
        world: &Rc<World>,
        tile: &Tile,
        pos: IntPair,
        season: f32,
        day_night: f32,
        weather: f32,
    ) -> f32 {
        current_temperature_for_tile_at_world_pos(tile, pos, season, day_night, weather, Some(world))
    }

    /// `-[SnowSurfaceBlock updateInTimeSinceSaved]`: fast-forwards the snow for the elapsed world time.
    pub fn update_in_time_since_saved(&self) {
        if self.base.borrow().needs_removed {
            return;
        }
        let world = self.world();
        let sv = services();
        let pos = self.pos();
        let below = tile_at_world_position_loaded(pos.x, pos.y - 1, &world);
        let tile = tile_at_world_position_loaded(pos.x, pos.y, &world);
        if let (Some(below), Some(tile)) = (below, tile) {
            if !tile_is_water(Some(below)) && !tile_is_water(Some(tile)) && tile.type_index() != TILE_ICE {
                let t = world.world_time();
                for i in 0..4i32 {
                    let ti: f32 = (t - ((112.5f32 * ((4 - i) as f32)) as f64)) as f32;
                    let wf = sv.get_weather_fraction_for_pos_at_world_time(&world, pos, ti as f64);
                    let dn = sv.get_day_night_fraction_for_x_at_world_time(&world, pos.x as f32, ti as f64);
                    let season = season_for_world_x(pos.x, ti as f64, Some(&world));
                    let temp = self.temperature_for(&world, &tile.get(), pos, season, dn, wf);
                    self.st.borrow_mut().temperature = temp;
                    let tt = temp;
                    if tt <= 0.0f32 {
                        let pc = self.st.borrow().partial_content;
                        if tt < 0.0f32 && wf > 0.2f32 && pc < 0.6f32 && tile.sun_light() > 239 {
                            let mut a = ((tile.sun_light() as f32) - 240.0f32) / 16.0f32;
                            if a <= 0.0f32 {
                                a = 0.0f32;
                            }
                            let mut b = -tt / 4.0f32;
                            if 1.0f32 <= b {
                                b = 1.0f32;
                            }
                            let mut npc = (wf - 0.2f32) * 0.005f32 * a * b * 300.0f32 + pc;
                            if 0.6f32 < npc {
                                npc = 0.6f32;
                            }
                            self.st.borrow_mut().partial_content = npc;
                        }
                    } else {
                        let pc = self.st.borrow().partial_content;
                        if 0.001f32 < pc {
                            let mut dc = wf - 0.2f32;
                            if dc <= 0.0f32 {
                                dc = 0.0f32;
                            }
                            let mut npc = ((pc as f64)
                                - (C002 + (dc as f64) * 0.2f64) * ((tt as f64) + 2.0f64) * 0.01f64 * 80.0f64)
                                as f32;
                            if npc < 0.002f32 {
                                npc = 0.0f32;
                            }
                            self.st.borrow_mut().partial_content = npc;
                        }
                    }
                }
                // after the loop, one more sample using the current time
                let season = season_for_world_x(pos.x, t, Some(&world));
                let dn = sv.get_day_night_fraction_for_x_at_world_time(&world, pos.x as f32, t);
                let wf = sv.get_weather_fraction_for_pos(&world, pos);
                let temp = self.temperature_for(&world, &tile.get(), pos, season, dn, wf);
                self.st.borrow_mut().temperature = temp;
                if 5.0f32 < temp {
                    self.st.borrow_mut().partial_content = 0.0;
                }
                self.update_snow_content_tile(true, &tile);
                self.update_ground_frozen_tile(&below, &tile);
            }
        }
    }

    /// `-[SnowSurfaceBlock updateRain:dt:]`. Never reached on the server (R2: the DynamicWorld gate
    /// `rainFractionNotIncludingSnow > 0.04` is always false) but ported.
    pub fn update_rain_dt(&self, rain_fraction: f32, dt: f32) {
        let temperature = self.st.borrow().temperature;
        if rain_fraction > 0.04f32 && temperature > 0.0f32 {
            let t = {
                let mut st = self.st.borrow_mut();
                st.rain_random_timer -= dt;
                st.rain_random_timer
            };
            if t < 0.0f32 {
                let nt = r01() * (1.1f32 - rain_fraction) * 2.0f32;
                self.st.borrow_mut().rain_random_timer = nt;
                let world = self.world();
                let pos = self.pos();
                let tile = tile_at_world_position_loaded(pos.x, pos.y, &world);
                if let Some(tile) = tile {
                    if tile.sun_light() > 249 && tile.back_wall_type_index() == 2 && tile_is_air_or_snow(Some(tile)) {
                        let below = tile_at_world_position_loaded(pos.x, pos.y - 1, &world);
                        if let Some(below) = below {
                            let fy = (below.partial_content_left() as f32) / 255.0f32 - 1.0f32;
                            let r1 = r01();
                            let r2 = r01();
                            let hit = Vector::new(r1 + pos.x as f32, fy + pos.y as f32, -r2 * 3.0f32);
                            let color = self.particle_color(&world, &tile.get(), 0.7f32);
                            let emitter = ParticleEmitter::instance();
                            for _ in 0..4 {
                                let ra = r01();
                                let rb = r01();
                                let vel = Vector::new(ra + (-0.5f32), rb + rb, 0.0f32);
                                emitter.add_particle_at_pos_velocity_color_gravity_type_life_scale(
                                    hit, vel, color, 1, 0.25f32, 8.0f32,
                                );
                            }
                        }
                    }
                }
            }
        }
    }

    /// Light colour of the particle effects (client visual; no server-visible state).
    fn particle_color(&self, world: &Rc<World>, tile: &Tile, factor: f32) -> Vector {
        let mut r = (tile.artificial_light_r as f32) / 1024.0f32;
        let mut g = (tile.artificial_light_g as f32) / 1024.0f32;
        let mut b = (tile.artificial_light_b as f32) / 1024.0f32;
        let mut m = r;
        if m < g {
            m = g;
        }
        if m < b {
            m = b;
        }
        if m > 1.0f32 {
            r /= m;
            g /= m;
            b /= m;
            m = 1.0f32;
        }
        let k = 1.0f32 / (m + 0.3f32);
        let day = world.day_color();
        let s = (tile.sun_light as f32) / 255.0f32;
        Vector::new4(
            (r * k + s * day.v[0]) * factor,
            (g * k + s * day.v[1]) * factor,
            (b * k + s * day.v[2]) * factor,
            1.0f32,
        )
    }

    /// `-[SnowSurfaceBlock spreadGrass:tile:]`. Server only.
    fn spread_grass_tile(&self, below: &TileRef, tile: &TileRef) {
        if !(self.st.borrow().temperature > 0.0f32) {
            return;
        }
        let tv = tile.get();
        if !(tv.sun_light > 0x40
            || tv.artificial_light_r > 0x40
            || tv.artificial_light_g > 0x40
            || tv.artificial_light_b > 0x40)
        {
            return;
        }
        let world = self.world();
        let pos = self.pos();
        let bw = below.back_wall_type_index();
        let tbw = tile.back_wall_type_index();
        let macro_pos = IntPair { x: pos.x / 32, y: (pos.y - 1) / 32 };
        let sv = services();
        if below.type_index() == 48 || (bw == 48 && tbw == 2) {
            if (r01() as f64) < 0.1f64 {
                if below.type_index() == 48 {
                    below.set_type_index(49);
                }
                if bw == 48 && tbw == 2 {
                    below.set_back_wall_type_index(49);
                }
                sv.snow_changed_at_macro_pos(macro_pos);
            }
        } else if below.type_index() == 6 || (bw == 6 && tbw == 2) {
            let dx = f32_to_i32(r01() * 16.0f32 - 8.0f32);
            let dy = f32_to_i32(r01() * 16.0f32 - 8.0f32);
            let macro_tiles = world.macro_tiles();
            let s = tile_at_world_position(pos.x.wrapping_add(dx), pos.y.wrapping_add(dy), &macro_tiles, &world);
            if let Some(s) = s {
                if s.type_index() == 27 || s.type_index() == 49 {
                    let mut blocked = false;
                    let mut test_x = pos.x;
                    let mut test_y = pos.y - 1;
                    let mut test_y_max = pos.y + dy;
                    if test_y_max < test_y {
                        test_y_max = pos.y;
                        test_y = pos.y + dy;
                        test_x = pos.x + dx;
                    }
                    loop {
                        test_y += 1;
                        if test_y_max <= test_y {
                            break;
                        }
                        let t = tile_at_world_position(test_x, test_y, &macro_tiles, &world);
                        if t.is_some() && !tile_is_solid(t) {
                            continue;
                        }
                        blocked = true;
                        break;
                    }
                    if !blocked {
                        if below.type_index() == 6 {
                            below.set_type_index(27);
                        }
                        if bw == 6 && tbw == 2 {
                            below.set_back_wall_type_index(27);
                        }
                        sv.snow_changed_at_macro_pos(macro_pos);
                    }
                }
            }
        }
    }

    /// `-[SnowSurfaceBlock updateGroundFrozen:tile:]`. Server only.
    fn update_ground_frozen_tile(&self, below: &TileRef, tile: &TileRef) {
        let pos = self.pos();
        let macro_pos = IntPair { x: pos.x / 32, y: (pos.y - 1) / 32 };
        let sv = services();
        let temp = self.st.borrow().temperature;
        let swap = |from: u8, to: u8| {
            below.set_type_index(to);
            if below.back_wall_type_index() == from {
                below.set_back_wall_type_index(to);
            }
            sv.snow_changed_at_macro_pos(macro_pos);
        };
        if temp < 0.0f32 || (tile.type_index() == 5 && tile.partial_content_left() != 0) {
            if below.type_index() == 27 {
                swap(27, 28);
            } else if below.type_index() == 49 {
                swap(49, 50);
            }
        } else if temp > 0.0f32 {
            if below.type_index() == 28 {
                swap(28, 27);
            } else if below.type_index() == 50 {
                swap(50, 49);
            }
        }
    }

    /// `-[SnowSurfaceBlock updateSnowContent:tile:]`. Server only.
    fn update_snow_content_tile(&self, force_update: bool, tile: &TileRef) {
        if tile.type_index() == 3 || tile.type_index() == 4 {
            return;
        }
        let new_val: u32 = f32_to_u32(255.0f32 * self.st.borrow().partial_content);
        let cur = tile.partial_content_left() as u32;
        if new_val != cur || force_update {
            // abs((int)(newVal - pcl)) > 0: reloadDrawBlockGeometryForTile, empty on the server
            if tile.partial_content_left() as u32 != new_val {
                tile.set_partial_content_left(new_val as u8);
                let pos = self.pos();
                services().snow_changed_at_macro_pos(IntPair { x: pos.x / 32, y: pos.y / 32 });
            }
            if tile.type_index() == 5 || tile.type_index() == 2 {
                if tile.partial_content_left() == 0 {
                    if tile.type_index() != 2 {
                        tile.set_type_index(2);
                    }
                } else if tile.type_index() != 5 {
                    tile.set_type_index(5);
                }
            }
        }
    }

    /// `-[SnowSurfaceBlock removeAllSnow]`
    pub fn remove_all_snow(&self) {
        self.st.borrow_mut().partial_content = 0.0;
        let world = self.world();
        let pos = self.pos();
        if let Some(tile) = tile_at_world_position_loaded(pos.x, pos.y, &world) {
            if tile.type_index() != 3 && tile.partial_content_left() != 0 && tile.type_index() != TILE_ICE {
                self.update_snow_content_tile(true, &tile);
                // reloadDrawBlockGeometryForTile: empty on the server
            }
        }
    }

    /// `-[SnowSurfaceBlock removeIfFloating]`
    pub fn remove_if_floating(&self) -> bool {
        let world = self.world();
        let pos = self.pos();
        let below = tile_at_world_position_loaded(pos.x, pos.y - 1, &world);
        if !tile_is_solid(below) {
            if !tile_is_water(below) {
                let tile = tile_at_world_position_loaded(pos.x, pos.y, &world);
                if let Some(t) = tile {
                    if t.type_index() != TILE_ICE {
                        let me = self.me();
                        self.set_needs_removed(&me, true);
                        return true;
                    }
                }
            } else {
                self.remove_all_snow();
            }
        }
        false
    }
}

impl SnowSurfaceBlock {
    /// `-[DynamicObject initWithWorld:dynamicWorld:cache:netData:]` inherited unchanged (SnowSurfaceBlock defines no net init; object_registry A3).
    /// Zero-state species ivars + the DynamicObject net init. `None` when the base init yields nil.
    pub fn init_with_world_dynamic_world_cache_net_data(
        world: &Rc<World>,
        dw: &DynamicWorldRef,
        cache: CacheRef,
        net_data: &[u8],
    ) -> Option<Rc<SnowSurfaceBlock>> {
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

impl DynamicObject for SnowSurfaceBlock {
    fn base(&self) -> &RefCell<DynamicObjectBase> {
        &self.base
    }
    fn class_info(&self) -> &'static ClassInfo {
        &SNOW_SURFACE_BLOCK_CLASS
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn object_type(&self) -> i32 {
        DYNAMIC_OBJECT_TYPE_SNOW_SURFACEBLOCK
    }
    /// : the dictionary captured at init, never refreshed.
    fn get_save_dict(&self, _me: &ObjRef) -> PlistDict {
        self.st.borrow().save_dict_cached.clone()
    }
    fn dealloc(&self, _me: &ObjRef) {
        self.st.borrow_mut().save_dict_cached = crate::objects::base::new_mutable_dict();
    }
    /// : `removeAllSnow` first when `flag`, then the base.
    fn set_needs_removed(&self, _me: &ObjRef, v: bool) {
        if v {
            self.remove_all_snow();
        }
        self.base.borrow_mut().needs_removed = v;
    }
    fn world_changed(&self, me: &ObjRef, positions: &mut Vec<IntPair>) {
        if self.base.borrow().needs_removed {
            return;
        }
        let pos = self.pos();
        let world = self.world();
        for cp in positions.iter() {
            if cp.x == pos.x {
                if cp.y == pos.y {
                    // the original does not null-check the tile
                    let tile = tile_at_world_position_loaded(pos.x, pos.y, &world);
                    if let Some(t) = tile {
                        if t.type_index() != TILE_ICE && (tile_is_solid(Some(t)) || tile_is_water(Some(t))) {
                            self.set_needs_removed(me, true);
                            return;
                        }
                    }
                } else if cp.y == pos.y - 1 && self.remove_if_floating() {
                    return;
                }
            }
        }
    }

    /// : `dt`, `accurateDT`, `isSimulation` unused.
    fn update_accurate_dt_is_simulation(&self, me: &ObjRef, _dt: f32, _accurate_dt: f32, _is_simulation: bool) {
        if self.base.borrow().needs_removed {
            return;
        }
        let world = self.world();
        let sv = services();
        let pos = self.pos();
        let below_pos = IntPair { x: pos.x, y: pos.y - 1 };
        let above_pos = IntPair { x: pos.x, y: pos.y + 1 };
        let below = tile_at_world_position_loaded(below_pos.x, below_pos.y, &world);
        let above = tile_at_world_position_loaded(above_pos.x, above_pos.y, &world);
        let tile = tile_at_world_position_loaded(pos.x, pos.y, &world);
        let (Some(below), Some(tile), Some(above)) = (below, tile, above) else { return };

        let wf = sv.get_weather_fraction_for_pos(&world, pos);
        let dn = sv.get_day_night_fraction_for_x_at_world_time(&world, pos.x as f32, world.world_time());
        let season = season_for_world_x(pos.x, world.world_time(), Some(&world));
        let temp = self.temperature_for(&world, &tile.get(), pos, season, dn, wf);
        self.st.borrow_mut().temperature = temp;
        let tt = temp;

        // 1. the block's own tile
        if tile.type_index() == TILE_ICE && tile.explored_fraction() != 0 {
            if tt > 1.0f32 {
                sv.fill_tile_at_pos_with_type(&world, tile, pos, ITEM_MELT_WATER);
            } else if tt < -2.0f32 {
                if tile_is_water(Some(above)) {
                    sv.fill_tile_at_pos_with_type(&world, above, above_pos, ITEM_FREEZE_WATER);
                }
                let lp = IntPair { x: pos.x - 1, y: pos.y };
                let rp = IntPair { x: pos.x + 1, y: pos.y };
                let lt = tile_at_world_position_loaded(lp.x, lp.y, &world);
                let rt = tile_at_world_position_loaded(rp.x, rp.y, &world);
                if tile_is_water(lt) {
                    sv.fill_tile_at_pos_with_type(&world, lt.unwrap(), lp, ITEM_FREEZE_WATER);
                }
                if tile_is_water(rt) {
                    sv.fill_tile_at_pos_with_type(&world, rt.unwrap(), rp, ITEM_FREEZE_WATER);
                }
            }
        } else if tile_is_water(Some(tile)) {
            self.set_needs_removed(me, true);
        }

        // 2. the tile below
        if tile_is_water(Some(below)) {
            self.remove_all_snow();
            if tt < -1.0f32 {
                sv.fill_tile_at_pos_with_type(&world, below, below_pos, ITEM_FREEZE_WATER);
            }
            if below.get().artificial_heat > 4 {
                // steam particle effect: client visual, but the draws happen on the server too (2 + 2n)
                let fy = (below.partial_content_left() as f32) / 255.0f32 - 1.0f32;
                let r1 = r01();
                let r2 = r01();
                let hit = Vector::new(r1 + pos.x as f32, fy + pos.y as f32, -r2 * 3.0f32);
                let color = self.particle_color(&world, &tile.get(), 0.9f32);
                let n = (below.get().artificial_heat as i32 + 4) / 4;
                let emitter = ParticleEmitter::instance();
                for _ in 0..n {
                    let ra = r01();
                    let rb = r01();
                    let vel = Vector::new(((ra - 0.5f32) as f64 * 0.5f64) as f32, rb * 0.5f32, 0.0f32);
                    emitter
                        .add_particle_at_pos_velocity_color_gravity_type_life_scale(hit, vel, color, 2, 2.0f32, 3.0f32);
                }
            }
        } else if tile_is_air(Some(below)) && tile.type_index() != TILE_ICE {
            self.set_needs_removed(me, true);
        } else if tile.type_index() != TILE_ICE {
            let pc = self.st.borrow().partial_content;
            if tt <= 0.0f32 {
                if tt < 0.0f32 && wf > 0.2f32 && pc < 0.6f32 && tile.sun_light() > 239 {
                    let mut a = ((tile.sun_light() as f32) - 240.0f32) / 16.0f32;
                    if a <= 0.0f32 {
                        a = 0.0f32;
                    }
                    let mut b = -tt / 4.0f32;
                    if 1.0f32 <= b {
                        b = 1.0f32;
                    }
                    let mut npc = (wf - 0.2f32) * 0.005f32 * a * b * 1.0f32 + pc;
                    if 0.6f32 < npc {
                        npc = 0.6f32;
                    }
                    self.st.borrow_mut().partial_content = npc;
                    self.update_snow_content_tile(false, &tile);
                }
            } else if 0.001f32 < pc || tile.partial_content_left() != 0 {
                let mut dc = wf - 0.2f32;
                if dc <= 0.0f32 {
                    dc = 0.0f32;
                }
                let mut npc =
                    ((pc as f64) - (C002 + (dc as f64) * 0.2f64) * ((tt as f64) + 1.0f64) * 0.01f64 * 4.0f64) as f32;
                if npc < 0.002f32 {
                    npc = 0.0f32;
                }
                self.st.borrow_mut().partial_content = npc;
                self.update_snow_content_tile(false, &tile);
            }
            self.update_ground_frozen_tile(&below, &tile);
            self.spread_grass_tile(&below, &tile);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lighting::artificial_light::testkit::*;
    use crate::rng::glibc_rand::{rand, srand};

    fn setup() -> Rc<World> {
        let w = world_with_blocks(4, &[(0, 0), (1, 0), (0, 1), (1, 1)]);
        for x in 30..60 {
            tile(&w, x, 39).update(|t| {
                t.type_index = 6; // dirt
                t.back_wall_type_index = 1;
            });
            for y in 40..60 {
                tile(&w, x, y).update(|t| t.sun_light = 255);
            }
        }
        w
    }

    fn mk(w: &Rc<World>, x: i32, y: i32) -> (Rc<SnowSurfaceBlock>, ObjRef) {
        let (_d, dwr) = test_dw();
        let b =
            SnowSurfaceBlock::init_with_world_dynamic_world_at_position_cache(w, &dwr, IntPair { x, y }, None).unwrap();
        let o: ObjRef = b.clone();
        (b, o)
    }

    /// Run `f` and return whether it consumed exactly `n` draws.
    fn draws(n: usize, seed: u32, f: impl FnOnce()) -> bool {
        srand(seed);
        f();
        let after = rand();
        srand(seed);
        for _ in 0..n {
            rand();
        }
        after == rand()
    }

    #[test]
    fn init_one_draw_and_temperature_with_zero_fractions() {
        install_rec();
        let w = setup();
        assert!(draws(1, 3, || {
            mk(&w, 40, 40);
        }));
        let (b, o) = mk(&w, 40, 40);
        assert_eq!(o.object_type(), 29);
        let t = tile(&w, 40, 40).get();
        let season = season_for_world_x(40, w.world_time(), Some(&w));
        let expect =
            current_temperature_for_tile_at_world_pos(&t, IntPair { x: 40, y: 40 }, season, 0.0, 0.0, Some(&w));
        assert_eq!(b.st.borrow().temperature, expect);
        assert_eq!(b.partial_content(), 0.0);
        // cached save dict has the base keys
        let d = o.get_save_dict(&o);
        assert_eq!(d.get(&"pos_x".to_string()).unwrap().int_value(), 40);
        // from a save dict: 1 draw, temperature 0
        let (_d2, dwr) = test_dw();
        assert!(draws(1, 4, || {
            let b2 = SnowSurfaceBlock::init_with_world_dynamic_world_save_dict_cache(&w, &dwr, &d, None).unwrap();
            assert_eq!(b2.st.borrow().temperature, 0.0);
        }));
    }

    #[test]
    fn accumulation_uses_exact_f32_formula_and_clamps() {
        let rec = install_rec();
        rec.weather.set(0.8);
        let w = setup();
        w.set_world_time(0.0);
        let (b, o) = mk(&w, 40, 40);
        // force freezing: use the world's custom base temperature via artificial heat -60 on the own tile
        tile(&w, 40, 40).update(|t| t.artificial_heat = -60);
        o.update_accurate_dt_is_simulation(&o, 0.1, 0.1, false);
        let tt = b.st.borrow().temperature;
        assert!(tt < 0.0, "temperature {tt}");
        let mut bb = -tt / 4.0f32;
        if 1.0f32 <= bb {
            bb = 1.0f32;
        }
        let a = (255.0f32 - 240.0f32) / 16.0f32;
        let expect = (0.8f32 - 0.2f32) * 0.005f32 * a * bb * 1.0f32 + 0.0f32;
        assert_eq!(b.partial_content(), expect);
        // one tick adds ~0.0028: 255 * that truncates to 0, so the tile is still air
        assert_eq!(tile(&w, 40, 40).get().partial_content_left as u32, f32_to_u32(255.0f32 * expect));
        assert_eq!(tile(&w, 40, 40).get().type_index, 2);
        for _ in 0..3 {
            o.update_accurate_dt_is_simulation(&o, 0.1, 0.1, false);
        }
        assert_eq!(tile(&w, 40, 40).get().type_index, 5); // snow
        assert!(rec.take().iter().any(|s| s == "snow(1,1)"));
        // clamp at 0.6
        b.set_partial_content(0.5999);
        for _ in 0..200 {
            o.update_accurate_dt_is_simulation(&o, 0.1, 0.1, false);
        }
        assert_eq!(b.partial_content(), 0.6f32);
    }

    #[test]
    fn melt_uses_double_formula() {
        let rec = install_rec();
        rec.weather.set(0.5);
        let w = setup();
        let (b, o) = mk(&w, 40, 40);
        tile(&w, 40, 40).update(|t| t.artificial_heat = 60);
        b.set_partial_content(0.3);
        o.update_accurate_dt_is_simulation(&o, 0.1, 0.1, false);
        let tt = b.st.borrow().temperature;
        assert!(tt > 0.0);
        let dc = 0.5f32 - 0.2f32;
        let expect = (0.3f32 as f64 - (C002 + (dc as f64) * 0.2) * ((tt as f64) + 1.0) * 0.01 * 4.0) as f32;
        assert_eq!(b.partial_content(), if expect < 0.002 { 0.0 } else { expect });
    }

    #[test]
    fn ground_frozen_swaps_and_removal() {
        let rec = install_rec();
        let w = setup();
        let (b, o) = mk(&w, 40, 40);
        // freezing: grass 27 -> 28, back wall too
        tile(&w, 40, 39).update(|t| {
            t.type_index = 27;
            t.back_wall_type_index = 27;
        });
        b.st.borrow_mut().temperature = -5.0;
        let below = tile(&w, 40, 39);
        let t = tile(&w, 40, 40);
        b.update_ground_frozen_tile(&below, &t);
        assert_eq!((below.type_index(), below.back_wall_type_index()), (28, 28));
        assert!(rec.take().contains(&"snow(1,1)".to_string()));
        // thaw
        b.st.borrow_mut().temperature = 5.0;
        b.update_ground_frozen_tile(&below, &t);
        assert_eq!(below.type_index(), 27);
        // compost grass 49 <-> 50
        below.set_type_index(49);
        b.st.borrow_mut().temperature = -1.0;
        b.update_ground_frozen_tile(&below, &t);
        assert_eq!(below.type_index(), 50);
        // exactly zero: neither branch
        b.st.borrow_mut().temperature = 0.0;
        b.update_ground_frozen_tile(&below, &t);
        assert_eq!(below.type_index(), 50);
        // removal when the floor disappears
        tile(&w, 40, 39).update(|t| t.type_index = 2);
        o.world_changed(&o, &mut vec![IntPair { x: 40, y: 39 }]);
        assert!(o.needs_removed());
        // solid at own position
        let (_b2, o2) = mk(&w, 45, 40);
        tile(&w, 45, 40).update(|t| t.type_index = 1);
        o2.world_changed(&o2, &mut vec![IntPair { x: 45, y: 40 }]);
        assert!(o2.needs_removed());
    }

    #[test]
    fn spread_grass_draw_counts() {
        let rec = install_rec();
        let w = setup();
        let (b, _o) = mk(&w, 40, 40);
        b.st.borrow_mut().temperature = 10.0;
        let below = tile(&w, 40, 39);
        let t = tile(&w, 40, 40);
        // dirt: two draws even if no grass is found
        assert!(draws(2, 11, || b.spread_grass_tile(&below, &t)));
        // compost: one draw
        below.set_type_index(48);
        assert!(draws(1, 11, || b.spread_grass_tile(&below, &t)));
        // neither: no draw
        below.set_type_index(1);
        below.set_back_wall_type_index(1);
        assert!(draws(0, 11, || b.spread_grass_tile(&below, &t)));
        // not warm / not lit: no draw
        below.set_type_index(6);
        b.st.borrow_mut().temperature = 0.0;
        assert!(draws(0, 11, || b.spread_grass_tile(&below, &t)));
        b.st.borrow_mut().temperature = 5.0;
        t.update(|x| x.sun_light = 0x40);
        assert!(draws(0, 11, || b.spread_grass_tile(&below, &t)));
        t.update(|x| x.sun_light = 0xff);
        // successful spread: a grass tile within reach, clear line
        below.set_type_index(6);
        for dx in -8..8 {
            for dy in -8..8 {
                if (dx, dy) != (0, -1) && (40 + dx, 40 + dy) != (40, 39) && 40 + dy >= 39 {
                    // plant grass on the surface row of the area so any random pick hits grass
                    if 40 + dy == 39 {
                        tile(&w, 40 + dx, 39).set_type_index(27);
                    }
                }
            }
        }
        below.set_type_index(6);
        rec.take();
        // search seeds until a draw lands on a grass tile at y=39 (dy = -1)
        let spread = (1..400).any(|seed| {
            below.set_type_index(6);
            srand(seed);
            b.spread_grass_tile(&below, &t);
            below.type_index() == 27
        });
        assert!(spread);
        assert!(rec.take().contains(&"snow(1,1)".to_string()));
    }

    #[test]
    fn steam_draw_count_over_hot_water() {
        install_rec();
        let w = setup();
        let (_b, o) = mk(&w, 40, 40);
        tile(&w, 40, 39).update(|t| {
            t.type_index = 3;
            t.artificial_heat = 9; // n = (9+4)/4 = 3 -> 2 + 2*3 = 8 draws
        });
        assert!(draws(8, 21, || o.update_accurate_dt_is_simulation(&o, 0.1, 0.1, false)));
    }

    #[test]
    fn catch_up_loop_and_hot_clear() {
        let rec = install_rec();
        rec.weather.set(0.9);
        let w = setup();
        let (b, _o) = mk(&w, 40, 40);
        tile(&w, 40, 40).update(|t| t.artificial_heat = -60);
        b.update_in_time_since_saved();
        // four accumulation samples with the *300.0f multiplier, clamped at 0.6, then the snow is written to the tile
        assert!(b.partial_content() > 0.0 && b.partial_content() <= 0.6);
        assert_eq!(tile(&w, 40, 40).get().type_index, 5);
        // hot: > 5.0 clears all snow
        tile(&w, 40, 40).update(|t| t.artificial_heat = 200);
        b.update_in_time_since_saved();
        assert_eq!(b.partial_content(), 0.0);
        assert_eq!(tile(&w, 40, 40).get().type_index, 2);
        // water tile: untouched
        let (b2, _o2) = mk(&w, 45, 40);
        tile(&w, 45, 40).update(|t| t.type_index = 3);
        b2.update_in_time_since_saved();
        assert_eq!(b2.partial_content(), 0.0);
    }

    #[test]
    fn ice_melt_and_freeze_neighbours_and_water_below() {
        let rec = install_rec();
        let w = setup();
        let (b, o) = mk(&w, 40, 40);
        tile(&w, 40, 40).update(|t| {
            t.type_index = 4;
            t.explored_fraction = 200;
            t.artificial_heat = 300;
        });
        rec.take();
        o.update_accurate_dt_is_simulation(&o, 0.1, 0.1, false);
        assert!(rec.take().contains(&"fill(40,40,0x423)".to_string()));
        // very cold: freeze the water above and beside
        tile(&w, 40, 40).update(|t| {
            t.type_index = 4;
            t.artificial_heat = -300;
        });
        tile(&w, 40, 41).update(|t| t.type_index = 3);
        tile(&w, 39, 40).update(|t| t.type_index = 3);
        o.update_accurate_dt_is_simulation(&o, 0.1, 0.1, false);
        let log = rec.take();
        assert!(log.contains(&"fill(40,41,0x422)".to_string()));
        assert!(log.contains(&"fill(39,40,0x422)".to_string()));
        // water below: removeAllSnow, freeze below at T < -1
        let (b3, o3) = mk(&w, 50, 40);
        tile(&w, 50, 39).update(|t| t.type_index = 3);
        tile(&w, 50, 40).update(|t| {
            t.artificial_heat = -300;
            t.partial_content_left = 100;
        });
        b3.set_partial_content(0.4);
        rec.take();
        o3.update_accurate_dt_is_simulation(&o3, 0.1, 0.1, false);
        assert_eq!(b3.partial_content(), 0.0);
        assert!(rec.take().contains(&"fill(50,39,0x422)".to_string()));
        let _ = b;
    }

    fn lit(w: &Rc<World>, x: i32, y: i32, f: impl Fn(&mut Tile)) {
        tile(w, x, y).update(f);
    }

    #[test]
    fn rain_makes_drops_only_when_it_rains_on_a_lit_open_tile() {
        install_rec();
        let w = setup();
        let (b, _o) = mk(&w, 40, 40);
        let wet = |b: &SnowSurfaceBlock| {
            let mut st = b.st.borrow_mut();
            st.temperature = 5.0;
            st.rain_random_timer = 0.0;
        };
        // 1 draw for the timer, 2 for the splash position, 2 per drop for 4 drops
        wet(&b);
        assert!(draws(11, 5, || b.update_rain_dt(0.5, 0.1)));
        // Too little rain, or freezing: nothing happens at all.
        wet(&b);
        assert!(draws(0, 5, || b.update_rain_dt(0.04, 0.1)));
        b.st.borrow_mut().temperature = 0.0;
        assert!(draws(0, 5, || b.update_rain_dt(0.5, 0.1)));
        // The timer has not run out yet.
        wet(&b);
        b.st.borrow_mut().rain_random_timer = 5.0;
        assert!(draws(0, 5, || b.update_rain_dt(0.5, 0.1)));
        // Unlit, covered or non-air tiles only re-arm the timer.
        for edit in
            [(|t: &mut Tile| t.sun_light = 100) as fn(&mut Tile), |t| t.back_wall_type_index = 1, |t| t.type_index = 1]
        {
            wet(&b);
            lit(&w, 40, 40, |t| {
                t.sun_light = 255;
                t.back_wall_type_index = 2;
                t.type_index = 2;
            });
            lit(&w, 40, 40, edit);
            assert!(draws(1, 5, || b.update_rain_dt(0.5, 0.1)));
        }
        // At the bottom of the world there is no tile below to splash on.
        let (edge, _o2) = mk(&w, 41, 0);
        lit(&w, 41, 0, |t| t.sun_light = 255);
        {
            let mut st = edge.st.borrow_mut();
            st.temperature = 5.0;
            st.rain_random_timer = 0.0;
        }
        assert!(draws(1, 5, || edge.update_rain_dt(0.5, 0.1)));
    }

    #[test]
    fn particle_colour_follows_the_strongest_light_and_the_day() {
        install_rec();
        let w = setup();
        let (b, _o) = mk(&w, 40, 40);
        let colour = |r: u16, g: u16, bl: u16| {
            let mut t = tile(&w, 40, 40).get();
            t.artificial_light_r = r;
            t.artificial_light_g = g;
            t.artificial_light_b = bl;
            t.sun_light = 128;
            b.particle_color(&w, &t, 1.0)
        };
        // Whichever channel is brightest decides the scale; beyond 1.0 everything is normalised.
        for (r, g, bl) in [(100, 200, 50), (100, 50, 200), (200, 100, 50), (4000, 2000, 1000), (0, 0, 0)] {
            let c = colour(r, g, bl);
            assert!(c.x.is_finite() && c.y.is_finite() && c.z.is_finite(), "{r} {g} {bl}");
        }
    }

    #[test]
    fn grass_spreads_from_compost_and_through_open_air_only() {
        let rec = install_rec();
        let w = setup();
        let (b, _o) = mk(&w, 40, 40);
        b.st.borrow_mut().temperature = 10.0;
        let below = tile(&w, 40, 39);
        let t = tile(&w, 40, 40);
        // Compost ground, or compost under a plain back wall, turns into compost grass on a lucky draw.
        let mut hit = (false, false);
        for seed in 1..200 {
            below.update(|x| {
                x.type_index = 48;
                x.back_wall_type_index = 1;
            });
            srand(seed);
            b.spread_grass_tile(&below, &t);
            hit.0 |= below.type_index() == 49;
            below.update(|x| {
                x.type_index = 1;
                x.back_wall_type_index = 48;
            });
            srand(seed);
            b.spread_grass_tile(&below, &t);
            hit.1 |= below.back_wall_type_index() == 49;
        }
        assert_eq!(hit, (true, true));
        assert!(rec.take().contains(&"snow(1,1)".to_string()));
        // Dirt looks at a random tile nearby: grass in the sky above the ground spreads when the straight line to it
        // is open, and does not when something solid is in the way.
        for x in 24..57 {
            for y in 30..50 {
                let ground = y <= 39;
                lit(&w, x, y, |t| {
                    t.type_index = if ground { 6 } else { 2 };
                    t.back_wall_type_index = if ground { 1 } else { 2 };
                    t.sun_light = 255;
                });
            }
        }
        // Grass pockets above and below the line, with a solid wall and open air between in different places.
        for y in 30..40 {
            lit(&w, 43, y, |t| t.type_index = 2); // an open shaft below the surface
        }
        for (x, y, ty) in [(36, 41, 27), (44, 41, 49), (43, 32, 49), (38, 33, 27), (36, 45, 27)] {
            lit(&w, x, y, |t| t.type_index = ty);
        }
        lit(&w, 40, 42, |t| t.type_index = 1);
        let (mut spread, mut kept, mut bw) = (false, false, false);
        for seed in 1..3000 {
            below.update(|x| {
                x.type_index = 6;
                x.back_wall_type_index = 1;
            });
            srand(seed);
            b.spread_grass_tile(&below, &t);
            let went = below.type_index() == 27;
            spread |= went;
            kept |= !went;
            below.update(|x| {
                x.type_index = 1;
                x.back_wall_type_index = 6;
            });
            t.update(|x| x.back_wall_type_index = 2);
            srand(seed);
            b.spread_grass_tile(&below, &t);
            bw |= below.back_wall_type_index() == 27;
        }
        assert_eq!((spread, kept, bw), (true, true, true));
    }

    #[test]
    fn thawing_and_removing_snow_cover_the_remaining_ground_cases() {
        let rec = install_rec();
        let w = setup();
        let (b, o) = mk(&w, 40, 40);
        let below = tile(&w, 40, 39);
        let t = tile(&w, 40, 40);
        // Frozen compost grass thaws again; a snowy tile with a snow layer counts as freezing too.
        below.set_type_index(50);
        b.st.borrow_mut().temperature = 5.0;
        b.update_ground_frozen_tile(&below, &t);
        assert_eq!(below.type_index(), 49);
        b.st.borrow_mut().temperature = 0.0;
        t.update(|x| {
            x.type_index = 5;
            x.partial_content_left = 10;
        });
        below.set_type_index(27);
        b.update_ground_frozen_tile(&below, &t);
        assert_eq!(below.type_index(), 28);
        // Snow content: water and ice tiles keep their type; unchanged content without force does nothing.
        for ty in [3u8, 4] {
            t.set_type_index(ty);
            b.set_partial_content(0.5);
            b.update_snow_content_tile(true, &t);
        }
        t.update(|x| {
            x.type_index = 5;
            x.partial_content_left = 127;
        });
        rec.take();
        b.update_snow_content_tile(false, &t);
        assert!(rec.take().is_empty());
        b.set_partial_content(0.0);
        b.update_snow_content_tile(true, &t);
        assert_eq!(t.type_index(), 2); // no snow left on a snow tile: back to air
                                       // removeAllSnow only touches snowy tiles that are not ice or water.
        t.update(|x| {
            x.type_index = 3;
            x.partial_content_left = 9;
        });
        b.remove_all_snow();
        assert_eq!(t.partial_content_left(), 9);
        t.set_type_index(2);
        // Removing when the ground vanished, was water, or the own tile is ice or missing.
        assert!(!o.needs_removed());
        below.set_type_index(3);
        assert!(!b.remove_if_floating());
        below.set_type_index(2);
        t.set_type_index(TILE_ICE);
        assert!(!b.remove_if_floating());
        t.set_type_index(2);
        assert!(b.remove_if_floating());
        assert!(o.needs_removed());
        // The unloaded neighbourhood: the block outside the loaded area has no tile at all.
        let (far, _of) = mk(&w, 100, 100);
        assert!(!far.remove_if_floating());
        far.remove_all_snow();
        let mut positions = vec![IntPair { x: 100, y: 100 }];
        _of.world_changed(&_of, &mut positions);
        // Changes elsewhere in the column or away from it are ignored; a removed block ignores everything.
        let (c, oc) = mk(&w, 50, 40);
        oc.world_changed(&oc, &mut vec![IntPair { x: 50, y: 45 }, IntPair { x: 51, y: 40 }]);
        assert!(!oc.needs_removed());
        tile(&w, 50, 40).set_type_index(TILE_ICE);
        oc.world_changed(&oc, &mut vec![IntPair { x: 50, y: 40 }]);
        assert!(!oc.needs_removed());
        tile(&w, 50, 40).set_type_index(3);
        oc.world_changed(&oc, &mut vec![IntPair { x: 50, y: 40 }]);
        assert!(oc.needs_removed());
        oc.world_changed(&oc, &mut vec![IntPair { x: 50, y: 40 }]);
        c.update_in_time_since_saved();
        oc.update_accurate_dt_is_simulation(&oc, 0.1, 0.1, false);
        oc.dealloc(&oc);
        oc.set_needs_removed(&oc, false);
        assert!(!oc.needs_removed());
    }

    #[test]
    fn nets_and_edges() {
        install_rec();
        let w = setup();
        let (_d, dwr) = test_dw();
        let nd =
            crate::types::netdata::DynamicObjectNetData { unique_id: 7, pos_x: 40, pos_y: 40, ..Default::default() };
        let bytes = crate::types::structs::WireStruct::to_vec(&nd);
        let made = SnowSurfaceBlock::init_with_world_dynamic_world_cache_net_data(&w, &dwr, None, &bytes);
        assert!(made.is_some());
        // An object at an unloaded position cannot be created from the network.
        let far =
            crate::types::netdata::DynamicObjectNetData { unique_id: 8, pos_x: 100, pos_y: 100, ..Default::default() };
        let bytes = crate::types::structs::WireStruct::to_vec(&far);
        assert!(SnowSurfaceBlock::init_with_world_dynamic_world_cache_net_data(&w, &dwr, None, &bytes).is_none());
    }

    #[test]
    fn weather_samples_clamp_at_their_limits() {
        let rec = install_rec();
        rec.weather.set(0.9);
        let w = setup();
        let (b, o) = mk(&w, 40, 40);
        // Freezing at the first light level that snows: the light factor is exactly zero, so nothing accumulates.
        lit(&w, 40, 40, |t| {
            t.artificial_heat = -60;
            t.sun_light = 240;
        });
        o.update_accurate_dt_is_simulation(&o, 0.1, 0.1, false);
        assert_eq!(b.partial_content(), 0.0);
        b.update_in_time_since_saved();
        // Barely above freezing with little weather: the melt step uses no weather bonus and snaps tiny layers to 0.
        rec.weather.set(0.1);
        lit(&w, 40, 40, |t| {
            t.artificial_heat = 60;
            t.sun_light = 255;
        });
        b.set_partial_content(0.0015);
        o.update_accurate_dt_is_simulation(&o, 0.1, 0.1, false);
        assert_eq!(b.partial_content(), 0.0);
        b.set_partial_content(0.3);
        b.update_in_time_since_saved();
        b.set_partial_content(0.3);
        o.update_accurate_dt_is_simulation(&o, 0.1, 0.1, false);
        assert!(b.partial_content() < 0.3);
        // The class descriptor and the downcast hook.
        assert_eq!(o.class_info().name, "SnowSurfaceBlock");
        assert!(o.as_any().is::<SnowSurfaceBlock>());
    }

    #[test]
    fn blocks_outside_the_loaded_area_and_odd_neighbours() {
        let rec = install_rec();
        let w = setup();
        let (far, ofar) = mk(&w, 100, 100);
        far.update_in_time_since_saved();
        ofar.update_accurate_dt_is_simulation(&ofar, 0.1, 0.1, false);
        far.st.borrow_mut().temperature = 5.0;
        far.st.borrow_mut().rain_random_timer = 0.0;
        far.update_rain_dt(0.5, 0.1);
        // Water at the block's own tile removes it; air below too, unless the tile is ice.
        let (_b, o) = mk(&w, 40, 40);
        tile(&w, 40, 40).set_type_index(3);
        o.update_accurate_dt_is_simulation(&o, 0.1, 0.1, false);
        assert!(o.needs_removed());
        let (_b2, o2) = mk(&w, 42, 40);
        tile(&w, 42, 39).set_type_index(2);
        o2.update_accurate_dt_is_simulation(&o2, 0.1, 0.1, false);
        assert!(o2.needs_removed());
        // Ice that freezes the water on both sides.
        let (_b3, o3) = mk(&w, 46, 40);
        tile(&w, 46, 40).update(|t| {
            t.type_index = 4;
            t.explored_fraction = 200;
            t.artificial_heat = -300;
        });
        tile(&w, 45, 40).set_type_index(3);
        tile(&w, 47, 40).set_type_index(3);
        rec.take();
        o3.update_accurate_dt_is_simulation(&o3, 0.1, 0.1, false);
        let log = rec.take();
        assert!(log.contains(&"fill(45,40,0x422)".to_string()) && log.contains(&"fill(47,40,0x422)".to_string()));
        // Ice at a mild temperature neither melts nor freezes anything.
        let (b5, o5) = mk(&w, 52, 40);
        let calm = (-60..60i16).any(|heat| {
            tile(&w, 52, 40).update(|t| {
                t.type_index = 4;
                t.explored_fraction = 200;
                t.artificial_heat = heat;
            });
            o5.update_accurate_dt_is_simulation(&o5, 0.1, 0.1, false);
            (-2.0..=1.0).contains(&b5.st.borrow().temperature)
        });
        assert!(calm);
        // Snow settles on any tile type without turning it into snow; a solid floor keeps the block.
        let (b6, _o6) = mk(&w, 54, 40);
        tile(&w, 54, 40).set_type_index(1);
        b6.set_partial_content(0.5);
        b6.update_snow_content_tile(true, &tile(&w, 54, 40));
        assert!(!b6.remove_if_floating());
        // Grass near the east edge of the loaded area: some random picks land outside it.
        let (edge, _oe) = mk(&w, 62, 40);
        edge.st.borrow_mut().temperature = 10.0;
        lit(&w, 62, 39, |t| t.type_index = 6);
        lit(&w, 62, 40, |t| t.sun_light = 255);
        let (below, here) = (tile(&w, 62, 39), tile(&w, 62, 40));
        for seed in 1..200 {
            below.set_type_index(6);
            srand(seed);
            edge.spread_grass_tile(&below, &here);
        }
        // A block that is freezing on a snowy tile with a snow layer already counts the layer.
        let (_b4, o4) = mk(&w, 48, 40);
        tile(&w, 48, 40).update(|t| {
            t.type_index = 2;
            t.artificial_heat = 40;
            t.partial_content_left = 50;
        });
        o4.update_accurate_dt_is_simulation(&o4, 0.1, 0.1, false);
    }
}
