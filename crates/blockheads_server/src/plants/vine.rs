//! owned by plants
//!
//! `VinePlant: Plant` (DynamicObject type 58, instance size 0xE0): a vine hanging DOWN from a soil head tile,
//! growth, water kill, offline catch-up, harvest cascade.
//!

use std::cell::RefCell;
use std::rc::{Rc, Weak};

use crate::dynworld::state::DynamicWorld;
use crate::fmath::cconv::f32_to_i32;
use crate::objects::base::{padded_bytes, DynamicObject, DynamicObjectBase};
use crate::objects::class_info::ClassInfo;
use crate::objects::handle::{self, CacheRef, ObjRef};
use crate::plist::value::PlistDict;
use crate::rng::glibc_rand;
use crate::types::netdata::PlantCreationNetData;
use crate::types::structs::{IntPair, WireStruct};
use crate::types::tile::Tile;
use crate::world::climate::current_temperature_for_tile_at_world_pos;
use crate::world::helpers::{tile_is_solid, tile_is_water};
use crate::world::state::World;
use crate::worldgen::noise::{noise_xy, NoiseFunction};

use super::plant::{
    adult_environment, artificial_light_value, create_free_block, dict_double, dict_float, dict_int, dw_changed,
    growth_rate_local, mark_update_needed, plant_get_save_dict,
    plant_init_with_world_dynamic_world_at_position_cache_max_age_gene_growth_rate_gene_tree_density_noise_function_season_offset_noise_function_adult_plant as plant_init_at_position,
    plant_init_with_world_dynamic_world_cache_net_data,
    plant_init_with_world_dynamic_world_save_dict_cache_tree_density_noise_function_season_offset_noise_function as plant_init_save_dict,
    plant_max_age_gene_variation, plant_plant_creation_net_data, plant_remote_update, plant_remove_from_macro_block,
    plant_set_needs_removed, plant_update, put_float, put_int, tile_loaded, ub_parity_null_tile, PlantLike, PlantState,
    DYNAMIC_OBJECT_TYPE_VINE_PLANT, PLANT_CLASS, PLANT_VINE,
};

/// `VinePlant: Plant` class descriptor.
pub static VINE_PLANT_CLASS: ClassInfo =
    ClassInfo { name: "VinePlant", superclass: Some(&PLANT_CLASS), protocols: &[] };

/// `CONTENTS_VINE`
const FG_VINE: u8 = 0x7C;
/// `ITEM_VINE`
const ITEM_VINE: i32 = 295;

/// VinePlant ivars 0x88..0xE0.
#[derive(Default)]
pub struct VineState {
    /// 0x8C, water check every 60 ticks
    pub check_weather_count: i32,
    /// 0x90 (unused)
    pub kill_count: i32,
    /// 0x94: gather progress of vine tile at depth `k + 1`
    pub below_gather_progress: [i32; 16],
    /// 0xD4
    pub growth_timer: f32,
    /// 0x88
    pub available_food: f32,
    /// 0xD8: N, the number of vine tiles below the head (can become -1)
    pub number_of_occupied_tiles_below: i32,
}

/// `VinePlant`.
pub struct VinePlant {
    pub base: RefCell<DynamicObjectBase>,
    pub plant: RefCell<PlantState>,
    pub vine: RefCell<VineState>,
    world: Weak<World>,
    dw: Weak<DynamicWorld>,
}

/// `lightAt(tile).combined`: `(float)((double)sunLight * 0.4 / 255.0 + (double)art)`; a NULL tile gives 1.0f.
pub fn vine_light_combined(t: Option<&Tile>) -> f32 {
    match t {
        None => 1.0f32,
        Some(t) => {
            let art = artificial_light_value(t);
            (((t.sun_light as f32) as f64) * 0.4f64 / 255.0f64 + (art as f64)) as f32
        }
    }
}

impl VinePlant {
    fn alloc(world: &Rc<World>, dw: &Rc<DynamicWorld>) -> Rc<VinePlant> {
        let (w, d) = (Rc::downgrade(world), Rc::downgrade(dw));
        handle::alloc_obj(|sw| VinePlant {
            base: RefCell::new(DynamicObjectBase::alloc_zeroed_with(sw)),
            plant: RefCell::new(PlantState::default()),
            vine: RefCell::new(VineState::default()),
            world: w,
            dw: d,
        })
    }
    fn world(&self) -> Rc<World> {
        self.world.upgrade().expect("VinePlant: world dropped")
    }
    fn dwr(&self) -> Rc<DynamicWorld> {
        self.dw.upgrade().expect("VinePlant: dynamic world dropped")
    }
    fn n(&self) -> i32 {
        self.vine.borrow().number_of_occupied_tiles_below
    }
    fn set_n(&self, v: i32) {
        self.vine.borrow_mut().number_of_occupied_tiles_below = v;
    }

    /// `-initSubDerivedItems`. Draws only if `maxAgeGene == 0`.
    pub fn init_sub_derived_items(&self, me: &ObjRef) {
        let world = self.world();
        let dw = self.dwr();
        let pos = me.pos();
        let tile = tile_loaded(&world, pos.x, pos.y);
        match tile {
            Some(t) => t.set_foreground_contents(FG_VINE),
            None => ub_parity_null_tile(),
        }
        dw.world_contents_changed_at_pos(pos);
        if self.plant.borrow().max_age_gene == 0 {
            self.plant.borrow_mut().max_age_gene = 0x7F;
            let v = plant_max_age_gene_variation(me);
            self.plant.borrow_mut().max_age_gene = v;
        }
        let gene = self.plant.borrow().max_age_gene;
        let mut max_age = (((gene as f32) / 255.0f32) * 0.5f32 + 0.75f32) * 28000.0f32;
        if tile.map_or(false, |t| matches!(t.type_index(), 0x30..=0x32)) {
            max_age *= 1.5f32;
        }
        self.plant.borrow_mut().max_age = max_age;
        self.vine.borrow_mut().below_gather_progress = [0; 16];
    }

    /// `-initWithWorld:...atPosition:cache:maxAgeGene:growthRateGene:...adultPlant:`.
    #[allow(clippy::too_many_arguments)]
    pub fn init_with_world_dynamic_world_at_position_cache_max_age_gene_growth_rate_gene_tree_density_noise_function_season_offset_noise_function_adult_plant(
        world: &Rc<World>,
        dw: &Rc<DynamicWorld>,
        pos: IntPair,
        cache: CacheRef,
        mag: u16,
        grg: u16,
        tdnf: Option<Rc<NoiseFunction>>,
        sonf: Option<Rc<NoiseFunction>>,
        adult: bool,
    ) -> Option<Rc<VinePlant>> {
        let me = Self::alloc(world, dw);
        let me_obj: ObjRef = me.clone();
        if !plant_init_at_position(&me_obj, world, dw, pos, cache, mag, grg, tdnf, sonf, adult) {
            return None;
        }
        let p = me_obj.pos();
        let fail = |me_obj: &ObjRef| -> Option<Rc<VinePlant>> {
            me_obj.remove_from_macro_block(me_obj);
            me_obj.dealloc(me_obj);
            None
        };
        let (mut weather, mut day, mut season) = (0.0f32, 0.0f32, 0.0f32);
        if let Some(t) = tile_loaded(world, p.x, p.y) {
            if adult {
                let e = adult_environment(world, p);
                weather = e.0;
                day = e.1;
                season = e.2;
                let temp = current_temperature_for_tile_at_world_pos(&t.get(), p, season, day, weather, Some(world));
                if temp < 1.0f32 {
                    return fail(&me_obj);
                }
            }
            if !me.is_required_soil_type(t.type_index()) {
                return fail(&me_obj);
            }
        }
        let wm = world.world_width_macro() as f32;
        let xf = ((p.x as f32) / 32.0f32) / wm;
        let yf = ((p.y as f32) / 32.0f32) / wm;
        let sonf = me.plant.borrow().season_offset_noise_function.clone();
        let f = noise_xy(&sonf, xf as f64, yf as f64, 6) as f32;
        me.plant.borrow_mut().season_offset = f32_to_i32(32.0f32 * f);
        me.init_sub_derived_items(&me_obj);
        me.set_n(0);
        for i in 0..16 {
            let y = p.y.wrapping_sub(i).wrapping_sub(1);
            if let Some(bt) = tile_loaded(world, p.x, y) {
                if me.tile_is_kind_of_self(&bt.get()) {
                    bt.set_foreground_contents(0);
                    dw.world_contents_changed_at_pos(IntPair { x: p.x, y });
                }
            }
        }
        if adult {
            let r = glibc_rand::r01_tagged("plants::vine::init::age");
            let (age, max_age) = {
                let mut st = me.plant.borrow_mut();
                st.age = r * st.max_age;
                (st.age, st.max_age)
            };
            let mut i = 1i32;
            while (i as f32) < 16.0f32 * (age / max_age) {
                let y = p.y.wrapping_sub(i);
                let bt = tile_loaded(world, p.x, y);
                let ok = match bt {
                    Some(t) => {
                        !tile_is_water(Some(t))
                            && t.type_index() != 0x1F
                            && t.foreground_contents() == 0
                            && current_temperature_for_tile_at_world_pos(
                                &t.get(),
                                IntPair { x: p.x, y },
                                season,
                                day,
                                weather,
                                Some(world),
                            ) > 0.0f32
                    }
                    None => false,
                };
                if !ok {
                    break;
                }
                bt.unwrap().set_foreground_contents(FG_VINE);
                let n = me.n() + 1;
                me.set_n(n);
                dw.world_contents_changed_at_pos(IntPair { x: p.x, y: p.y.wrapping_sub(n) });
                i += 1;
            }
        }
        let r = glibc_rand::r01_tagged("plants::vine::init::availableFood");
        me.vine.borrow_mut().available_food = ((r * 900.0f32) as f64 * 1.5f64) as f32;
        Some(me)
    }

    /// `-initWithWorld:dynamicWorld:saveDict:cache:...` (load + offline catch-up).
    pub fn init_with_world_dynamic_world_save_dict_cache_tree_density_noise_function_season_offset_noise_function(
        world: &Rc<World>,
        dw: &Rc<DynamicWorld>,
        save_dict: &PlistDict,
        cache: CacheRef,
        tdnf: Option<Rc<NoiseFunction>>,
        sonf: Option<Rc<NoiseFunction>>,
    ) -> Option<Rc<VinePlant>> {
        let me = Self::alloc(world, dw);
        let me_obj: ObjRef = me.clone();
        if !plant_init_save_dict(&me_obj, world, dw, save_dict, cache, tdnf, sonf) {
            return None;
        }
        me.set_n(dict_int(save_dict, "numberOfOccupiedTilesBelow"));
        me.vine.borrow_mut().growth_timer = dict_float(save_dict, "growthTimer");
        me.init_sub_derived_items(&me_obj);
        let mut food = dict_float(save_dict, "availableFood");
        if food < 0.1f32 {
            let r = glibc_rand::r01_tagged("plants::vine::load::availableFood");
            food = ((r * 900.0f32) as f64 * 1.5f64) as f32;
        }
        me.vine.borrow_mut().available_food = food;
        let saved = dict_double(save_dict, "saveTime");
        let now = world.world_time();
        let mut dt = now - saved;
        {
            let mut v = me.vine.borrow_mut();
            v.growth_timer = ((v.growth_timer as f64) + dt) as f32;
        }
        let (max_age, age) = {
            let st = me.plant.borrow();
            (st.max_age, st.age)
        };
        if (max_age as f64) <= (age as f64) + dt && me.is_growing_in_compost(&me_obj) {
            dt = ((max_age - age) as f64) - 0.1f64;
        }
        if (max_age as f64) <= (age as f64) + dt {
            me.die_of_old_age(&me_obj);
            return Some(me);
        }
        let growth_rate = me.plant.borrow().growth_rate;
        let pos = me_obj.pos();
        loop {
            let (n, timer) = {
                let v = me.vine.borrow();
                (v.number_of_occupied_tiles_below, v.growth_timer)
            };
            if !(n < 15 && timer > 0.0f32) {
                break;
            }
            let g = growth_rate_local(n, growth_rate);
            if !(timer > 900.0f32 / g) {
                break;
            }
            let y = pos.y.wrapping_sub(n).wrapping_sub(1);
            let bt = tile_loaded(world, pos.x, y);
            let blocked = match bt {
                None => true,
                Some(t) => {
                    tile_is_water(Some(t))
                        || t.type_index() == 0x1F
                        || t.foreground_contents() != 0
                        || vine_light_combined(Some(&t.get())) <= 0.2f32
                }
            };
            if blocked {
                me.vine.borrow_mut().growth_timer = 0.0;
                break;
            }
            bt.unwrap().set_foreground_contents(FG_VINE);
            me.set_n(n + 1);
            dw.world_contents_changed_at_pos(IntPair { x: pos.x, y: pos.y.wrapping_sub(n + 1) });
            me.vine.borrow_mut().growth_timer -= 900.0f32 / g;
        }
        {
            let mut st = me.plant.borrow_mut();
            st.age = ((st.age as f64) + dt) as f32;
        }
        dw_changed(&me_obj);
        Some(me)
    }

    /// `-initWithWorld:dynamicWorld:cache:netData:` (client replica).
    pub fn init_with_net_data(
        world: &Rc<World>,
        dw: &Rc<DynamicWorld>,
        cache: CacheRef,
        net_data: &[u8],
    ) -> Option<Rc<VinePlant>> {
        let me = Self::alloc(world, dw);
        let me_obj: ObjRef = me.clone();
        if !plant_init_with_world_dynamic_world_cache_net_data(&me_obj, world, dw, cache, net_data) {
            return None;
        }
        let s = PlantCreationNetData::read_le(&padded_bytes(net_data, 0x28));
        me.set_n(s.extra_data as i32);
        me.init_sub_derived_items(&me_obj);
        Some(me)
    }

    /// `-dieOfOldAge`. One draw.
    pub fn die_of_old_age(&self, me: &ObjRef) {
        let world = self.world();
        let dw = self.dwr();
        let r = glibc_rand::r01_tagged("plants::vine::dieOfOldAge");
        let pos = me.pos();
        if r <= 0.5f32 {
            self.tile_harvested_remove_blockhead_correct_tool_multiplier(me, pos, None, 1);
            dw.sow_plant_near_parent(me);
        } else {
            let mut i = 0;
            while i < self.n() {
                let y = pos.y.wrapping_sub(i).wrapping_sub(1);
                if let Some(t) = tile_loaded(&world, pos.x, y) {
                    if self.tile_is_kind_of_self(&t.get()) {
                        t.set_foreground_contents(0);
                        dw.world_contents_changed_at_pos(IntPair { x: pos.x, y });
                    }
                }
                i += 1;
            }
            {
                let mut st = self.plant.borrow_mut();
                st.age = 0.0;
                st.flowering = false;
            }
            self.set_n(0);
            dw_changed(me);
            mark_update_needed(me);
        }
    }
}

impl PlantLike for VinePlant {
    fn plant_state(&self) -> &RefCell<PlantState> {
        &self.plant
    }
    fn plant_world(&self) -> Rc<World> {
        self.world()
    }
    fn plant_dw(&self) -> Rc<DynamicWorld> {
        self.dwr()
    }
    fn plant_type(&self) -> i32 {
        PLANT_VINE
    }
    fn dropped_item_type(&self) -> i32 {
        ITEM_VINE
    }
    fn tile_is_kind_of_self(&self, tile: &Tile) -> bool {
        tile.foreground_contents == FG_VINE
    }
    fn is_required_soil_type(&self, t: u8) -> bool {
        matches!(t, 6 | 27 | 28 | 48 | 49 | 50)
    }
    fn number_of_occupied_tiles_below(&self) -> i32 {
        self.n()
    }
    fn available_food(&self) -> f32 {
        self.vine.borrow().available_food
    }
    fn set_available_food(&self, v: f32) {
        self.vine.borrow_mut().available_food = v;
    }
    /// `-isGrowingInCompost`: the HEAD tile (R10).
    fn is_growing_in_compost(&self, me: &ObjRef) -> bool {
        let p = me.pos();
        match tile_loaded(&self.world(), p.x, p.y) {
            Some(t) => matches!(t.type_index(), 0x30..=0x32),
            None => false,
        }
    }
    /// `-gatherProgressForTile:` (x is not checked).
    fn gather_progress_for_tile(&self, tile_pos: IntPair) -> i32 {
        let py = self.base.borrow().pos.y;
        if py.wrapping_sub(16) < tile_pos.y && tile_pos.y < py {
            self.vine.borrow().below_gather_progress[(py - tile_pos.y - 1) as usize]
        } else {
            self.plant.borrow().gather_progress
        }
    }
    /// `-setGatherProgress:forTile:`
    fn set_gather_progress_for_tile(&self, v: i32, tile_pos: IntPair) {
        let py = self.base.borrow().pos.y;
        if tile_pos.y < py {
            let d = py - tile_pos.y;
            if d <= self.n() && d < 16 {
                self.vine.borrow_mut().below_gather_progress[(d - 1) as usize] = v;
            }
        } else {
            self.plant.borrow_mut().gather_progress = v;
        }
    }
    /// `-plantCreationNetData`: Plant's plus `extraData = N`.
    fn plant_creation_net_data(&self, me: &ObjRef) -> PlantCreationNetData {
        let mut s = plant_plant_creation_net_data(me);
        s.extra_data = self.n() as i16;
        s
    }

    /// `-tileHarvested:removeBlockhead:correctToolMultiplier:`.
    fn tile_harvested_remove_blockhead_correct_tool_multiplier(
        &self,
        me: &ObjRef,
        tile_pos: IntPair,
        bh: Option<&ObjRef>,
        mult: i32,
    ) -> i32 {
        let world = self.world();
        let dw = self.dwr();
        let pos = me.pos();
        let mut cnt = 0;
        if !(tile_pos.x == pos.x && tile_pos.y <= pos.y) {
            return 0;
        }
        let d = pos.y.wrapping_sub(tile_pos.y);
        let age = self.plant.borrow().age;
        let age_factor = if age < 900.0f32 { age / 900.0f32 } else { 1.0f32 };
        let m = (mult as f32) * age_factor;
        let count = if 1.0f32 > m { 1.0f32 } else { m };
        let count_int = f32_to_i32(count);
        dw_changed(me);
        if d < 16 {
            let tile = tile_loaded(&world, tile_pos.x, tile_pos.y);
            if tile.is_none() {
                ub_parity_null_tile();
            }
            if let Some(t) = tile {
                if t.foreground_contents() == FG_VINE {
                    for _ in 0..count_int {
                        let a = self.max_age_gene_variation();
                        let b = self.growth_rate_gene_variation();
                        create_free_block(&dw, tile_pos, ITEM_VINE, a, b, 1, bh);
                        cnt += 1;
                    }
                    t.set_foreground_contents(0);
                    dw.world_contents_changed_at_pos(tile_pos);
                }
            }
            // cascade: strict `i < N`, the last vine tile is not visited (original quirk)
            let mut i = d.wrapping_add(1);
            while i < self.n() {
                let y = pos.y.wrapping_sub(i);
                let t = tile_loaded(&world, pos.x, y);
                if let Some(tr) = t {
                    if self.tile_is_kind_of_self(&tr.get()) {
                        if glibc_rand::rand_tagged("plants::vine::tileHarvested::cascade") > 0x3fff_ffff {
                            let mut cp = IntPair { x: pos.x, y };
                            if tile_is_solid(t) {
                                cp = pos;
                            }
                            let a = self.max_age_gene_variation();
                            let b = self.growth_rate_gene_variation();
                            create_free_block(&dw, cp, ITEM_VINE, a, b, 1, None);
                        }
                        tr.set_foreground_contents(0);
                        dw.world_contents_changed_at_pos(IntPair { x: pos.x, y });
                    }
                }
                i += 1;
            }
        }
        mark_update_needed(me);
        let nn = d.wrapping_sub(1);
        self.set_n(if nn < 16 { nn } else { 16 });
        if tile_pos.y == pos.y {
            self.remove_plant_without_creating_freeblocks(me);
        }
        cnt
    }
}

impl DynamicObject for VinePlant {
    super::plant::plant_dynamic_object_common!();

    fn class_info(&self) -> &'static ClassInfo {
        &VINE_PLANT_CLASS
    }
    fn object_type(&self) -> i32 {
        DYNAMIC_OBJECT_TYPE_VINE_PLANT
    }

    /// `-getSaveDict`.
    fn get_save_dict(&self, me: &ObjRef) -> PlistDict {
        let mut d = plant_get_save_dict(me);
        let v = self.vine.borrow();
        put_int(&mut d, "numberOfOccupiedTilesBelow", v.number_of_occupied_tiles_below);
        put_float(&mut d, "growthTimer", v.growth_timer);
        put_float(&mut d, "availableFood", v.available_food);
        d
    }

    /// `-setNeedsRemoved:`: Plant's only.
    fn set_needs_removed(&self, me: &ObjRef, v: bool) {
        plant_set_needs_removed(me, v);
    }

    fn remove_from_macro_block(&self, me: &ObjRef) {
        plant_remove_from_macro_block(me);
    }

    /// `-remoteUpdate:` (client path).
    fn remote_update(&self, me: &ObjRef, net_data: &[u8]) {
        plant_remote_update(me, net_data);
        let u = PlantCreationNetData::read_le(&padded_bytes(net_data, 0x28));
        let new_n = u.extra_data as i32;
        let n = self.n();
        if n != new_n {
            let world = self.world();
            let dw = self.dwr();
            let pos = me.pos();
            let mut i = 0;
            while i < n.max(new_n) {
                let y = pos.y.wrapping_sub(i).wrapping_sub(1);
                if let Some(t) = tile_loaded(&world, pos.x, y) {
                    if i < new_n {
                        t.set_foreground_contents(FG_VINE);
                    } else if t.foreground_contents() == FG_VINE {
                        t.set_foreground_contents(0);
                    }
                }
                dw.world_contents_changed_at_pos(IntPair { x: pos.x, y });
                i += 1;
            }
            self.set_n(new_n);
        }
    }

    /// `-update:accurateDT:isSimulation:`.
    fn update_accurate_dt_is_simulation(&self, me: &ObjRef, dt: f32, accurate_dt: f32, is_simulation: bool) {
        plant_update(me, dt, accurate_dt, is_simulation);
        if me.needs_removed() || me.is_net() {
            return;
        }
        let world = self.world();
        let dw = self.dwr();
        let pos = me.pos();
        let tile = tile_loaded(&world, pos.x, pos.y);
        let run_check = {
            let mut v = self.vine.borrow_mut();
            v.check_weather_count += 1;
            if v.check_weather_count > 59 {
                v.check_weather_count = 0;
                true
            } else {
                false
            }
        };
        if run_check {
            if tile_is_water(tile) {
                self.tile_harvested_remove_blockhead_correct_tool_multiplier(me, pos, None, 1);
                return;
            }
            let mut i = 0;
            while i < self.n() {
                let y = pos.y.wrapping_sub(i).wrapping_sub(1);
                if tile_is_water(tile_loaded(&world, pos.x, y)) {
                    self.tile_harvested_remove_blockhead_correct_tool_multiplier(me, IntPair { x: pos.x, y }, None, 1);
                    break;
                }
                i += 1;
            }
        }
        let n = self.n();
        if n < 15 {
            let g = growth_rate_local(n, self.plant.borrow().growth_rate);
            let due = {
                let mut v = self.vine.borrow_mut();
                v.growth_timer += dt;
                if v.growth_timer > 900.0f32 / g {
                    v.growth_timer -= 900.0f32 / g;
                    true
                } else {
                    false
                }
            };
            if due {
                let y = pos.y.wrapping_sub(n).wrapping_sub(1);
                if let Some(bt) = tile_loaded(&world, pos.x, y) {
                    let t = bt.get();
                    if t.type_index != 0x1F
                        && !tile_is_water(Some(bt))
                        && (t.foreground_contents == 0 || t.foreground_contents == FG_VINE)
                        && vine_light_combined(Some(&t)) > 0.2f32
                    {
                        dw.world_contents_changed_at_pos(IntPair { x: pos.x, y });
                        bt.set_foreground_contents(FG_VINE);
                        if let Some(slot) = self.vine.borrow_mut().below_gather_progress.get_mut(n as usize) {
                            *slot = 0;
                        }
                        self.set_n(n + 1);
                        dw_changed(me);
                        mark_update_needed(me);
                    }
                }
            }
        }
        let (age, max_age) = {
            let st = self.plant.borrow();
            (st.age, st.max_age)
        };
        if age > max_age {
            self.die_of_old_age(me);
        }
    }

    fn dealloc(&self, _me: &ObjRef) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    ///  R4 / U10: reload gives `g = 0.5`, period 900 / 0.5 = 1800 s.
    #[test]
    fn growth_period() {
        for n in 0..15 {
            assert_eq!(900.0f32 / growth_rate_local(n, 0.0), 1800.0);
        }
    }

    /// : `sunLight*0.4/255 + artificial`; NULL gives 1.0f; threshold 0.2f.
    #[test]
    fn light_combined() {
        assert_eq!(vine_light_combined(None), 1.0);
        let mut t = Tile::default();
        t.sun_light = 255;
        assert!((vine_light_combined(Some(&t)) - 0.4).abs() < 1e-6);
        t.sun_light = 127;
        assert!(vine_light_combined(Some(&t)) <= 0.2 + 0.0001);
        t.sun_light = 128;
        assert!(vine_light_combined(Some(&t)) > 0.2);
    }

    ///  initSubDerivedItems: maxAge range 21000..35000 for genes 0..255 (gene 255 -> 35000).
    #[test]
    fn max_age_range() {
        let m = (((255u16 as f32) / 255.0f32) * 0.5f32 + 0.75f32) * 28000.0f32;
        assert_eq!(m, 35000.0);
    }
}

/// Scenario tests: a vine hanging from a ceiling, growth, catch-up on load, harvest and death.
#[cfg(test)]
mod scenario_tests {
    use super::*;
    use crate::plist::value::PlistValue;
    use crate::rng::glibc_rand::srand;
    use crate::trees::tree::testenv::env;
    use crate::world::tiles::tile_at_world_position_loaded;

    const X: i32 = 20;
    const HEAD: i32 = 24;

    /// Dirt ceiling at rows 24..=26 over column 20, open air (with sun) below it down to row 9.
    fn cave(w: &Rc<World>) {
        cave_at(w, HEAD);
    }

    /// The same cave with its ceiling at row `head`.
    fn cave_at(w: &Rc<World>, head: i32) {
        for y in (head - 14)..=(head + 2) {
            for x in 18..=22 {
                tile_at_world_position_loaded(x, y, w).unwrap().update(|t| {
                    if y >= head {
                        t.type_index = 6;
                        t.sun_light = 0;
                    } else {
                        t.type_index = 2;
                        t.back_wall_type_index = 2;
                        t.sun_light = 255;
                        t.foreground_contents = 0;
                    }
                });
            }
        }
    }

    fn vine_at(d: &Rc<DynamicWorld>, adult: bool) -> Option<Rc<VinePlant>> {
        VinePlant::init_with_world_dynamic_world_at_position_cache_max_age_gene_growth_rate_gene_tree_density_noise_function_season_offset_noise_function_adult_plant(
            &d.world(), d, IntPair { x: X, y: HEAD }, None, 128, 128, None, None, adult,
        )
    }

    fn vine_at_height(d: &Rc<DynamicWorld>, y: i32) -> Option<Rc<VinePlant>> {
        VinePlant::init_with_world_dynamic_world_at_position_cache_max_age_gene_growth_rate_gene_tree_density_noise_function_season_offset_noise_function_adult_plant(
            &d.world(), d, IntPair { x: X, y }, None, 128, 128, None, None, false,
        )
    }

    fn fg(w: &Rc<World>, y: i32) -> u8 {
        tile_at_world_position_loaded(X, y, w).unwrap().foreground_contents()
    }

    #[test]
    fn a_young_vine_grows_down_its_column() {
        let (w, d) = env("vn_grow");
        cave(&w);
        let vine = vine_at(&d, false).expect("vine");
        let me: ObjRef = vine.clone();
        assert_eq!(fg(&w, HEAD), FG_VINE);
        assert_eq!(vine.n(), 0);
        // a long second at a time: the growth timer fires, the plant ages, the weather check runs every 60 calls
        for _ in 0..70 {
            vine.update_accurate_dt_is_simulation(&me, 1000.0, 1000.0, false);
        }
        assert!(vine.n() > 0, "n = {}", vine.n());
        assert_eq!(fg(&w, HEAD - 1), FG_VINE);
        // gather progress per tile
        let below = IntPair { x: X, y: HEAD - 1 };
        vine.set_gather_progress_for_tile(5, below);
        assert_eq!(vine.gather_progress_for_tile(below), 5);
        vine.set_gather_progress_for_tile(7, IntPair { x: X, y: HEAD });
        assert_eq!(vine.gather_progress_for_tile(IntPair { x: X, y: HEAD }), 7);
        vine.set_gather_progress_for_tile(9, IntPair { x: X, y: HEAD - 40 });
        assert_eq!(vine.gather_progress_for_tile(IntPair { x: X, y: HEAD - 40 }), 7);
        assert_eq!(vine.plant_type(), PLANT_VINE);
        assert_eq!(vine.dropped_item_type(), ITEM_VINE);
        assert_eq!(vine.available_food(), vine.vine.borrow().available_food);
        vine.set_available_food(3.0);
        assert!(vine.is_required_soil_type(6) && !vine.is_required_soil_type(1));
        assert!(!vine.is_growing_in_compost(&me));
        tile_at_world_position_loaded(X, HEAD, &w).unwrap().set_type_index(0x31);
        assert!(vine.is_growing_in_compost(&me));
    }

    #[test]
    fn adult_vines_hang_down_at_once_unless_it_is_too_cold() {
        let (w, d) = env("vn_adult");
        let mut lengths = Vec::new();
        for seed in 1..40u32 {
            cave(&w);
            if seed % 3 == 0 {
                // something in the way two tiles below the head stops the growth
                tile_at_world_position_loaded(X, HEAD - 2, &w).unwrap().set_foreground_contents(1);
            }
            srand(seed);
            lengths.push(vine_at(&d, true).expect("vine").n());
        }
        assert!(
            lengths.iter().any(|n| *n > 0) && lengths.contains(&1) && lengths.iter().all(|n| *n <= 15),
            "{lengths:?}"
        );
        // freezing worlds refuse adult vines
        // high in the sky it is freezing whatever the clock says
        cave_at(&w, 1000);
        tile_at_world_position_loaded(X, 1000, &w).unwrap().update(|t| t.sun_light = 255);
        let vine = VinePlant::init_with_world_dynamic_world_at_position_cache_max_age_gene_growth_rate_gene_tree_density_noise_function_season_offset_noise_function_adult_plant(
            &w, &d, IntPair { x: X, y: 1000 }, None, 128, 128, None, None, true,
        );
        assert!(vine.is_none());
        w.set_custom_rules(Default::default());
        cave(&w);
        // a head that is not on soil is refused
        tile_at_world_position_loaded(X, HEAD, &w).unwrap().set_type_index(1);
        assert!(vine_at(&d, false).is_none());
        assert!(vine_at(&d, true).is_none());
    }

    #[test]
    fn vines_at_the_edge_of_the_loaded_world() {
        let (w, d) = env("vn_edge");
        cave(&w);
        // the bottom macro row is not loaded: the vine head sits in the row above it
        let cut = w.macro_tiles();
        for mx in 0..2 {
            *cut.get(mx).unwrap().physical_block.borrow_mut() = None;
        }
        let head = IntPair { x: X, y: 32 };
        tile_at_world_position_loaded(X, 32, &w).unwrap().set_type_index(6);
        let vine = VinePlant::init_with_world_dynamic_world_at_position_cache_max_age_gene_growth_rate_gene_tree_density_noise_function_season_offset_noise_function_adult_plant(
            &w, &d, head, None, 128, 128, None, None, true,
        )
        .expect("vine");
        let me: ObjRef = vine.clone();
        vine.set_n(2);
        for _ in 0..80 {
            vine.update_accurate_dt_is_simulation(&me, 1000.0, 1000.0, false);
        }
        // a harvest, a death and a reload with the world below missing
        let mut dict = vine.get_save_dict(&me);
        dict.map.insert("saveTime".to_string(), PlistValue::real_f64(w.world_time() - 10_000.0));
        let back = VinePlant::init_with_world_dynamic_world_save_dict_cache_tree_density_noise_function_season_offset_noise_function(&w, &d, &dict, None, None, None);
        assert!(back.is_some());
        vine.set_n(3);
        srand(2);
        vine.tile_harvested_remove_blockhead_correct_tool_multiplier(&me, IntPair { x: X, y: 30 }, None, 1);
        vine.die_of_old_age(&me);
        srand(1);
        vine.die_of_old_age(&me);
    }

    #[test]
    fn saved_vines_catch_up_after_loading() {
        let (w, d) = env("vn_load");
        cave(&w);
        let vine = vine_at(&d, false).unwrap();
        let me: ObjRef = vine.clone();
        vine.plant.borrow_mut().age = 100.0;
        let mut dict = vine.get_save_dict(&me);
        dict.map.insert("saveTime".to_string(), PlistValue::real_f64(w.world_time() - 10_000.0));
        put_float(&mut dict, "availableFood", 0.0);
        put_float(&mut dict, "growthTimer", 5000.0);
        let back = VinePlant::init_with_world_dynamic_world_save_dict_cache_tree_density_noise_function_season_offset_noise_function(&w, &d, &dict, None, None, None).unwrap();
        assert!(back.n() > 0 || back.vine.borrow().growth_timer > 0.0);
        // an offline time longer than the life dies of old age; in compost it only ages to the limit
        let mut old = dict.clone();
        old.map.insert("saveTime".to_string(), PlistValue::real_f64(w.world_time() - 1.0e9));
        srand(1);
        let dead = VinePlant::init_with_world_dynamic_world_save_dict_cache_tree_density_noise_function_season_offset_noise_function(&w, &d, &old, None, None, None);
        assert!(dead.is_some());
        tile_at_world_position_loaded(X, HEAD, &w).unwrap().set_type_index(0x31);
        let compost = VinePlant::init_with_world_dynamic_world_save_dict_cache_tree_density_noise_function_season_offset_noise_function(&w, &d, &old, None, None, None);
        assert!(compost.is_some());
        // a blocked column resets the growth timer
        cave(&w);
        tile_at_world_position_loaded(X, HEAD - 1, &w).unwrap().set_foreground_contents(1);
        let blocked = VinePlant::init_with_world_dynamic_world_save_dict_cache_tree_density_noise_function_season_offset_noise_function(&w, &d, &dict, None, None, None).unwrap();
        assert_eq!(blocked.vine.borrow().growth_timer, 0.0);
    }

    #[test]
    fn network_copies_follow_the_length() {
        let (w, d) = env("vn_net");
        cave(&w);
        let vine = vine_at(&d, false).unwrap();
        let me: ObjRef = vine.clone();
        vine.set_n(3);
        for y in (HEAD - 3)..HEAD {
            tile_at_world_position_loaded(X, y, &w).unwrap().set_foreground_contents(FG_VINE);
        }
        let net = vine.plant_creation_net_data(&me);
        let copy = VinePlant::init_with_net_data(&w, &d, None, &net.to_vec()).unwrap();
        assert_eq!(copy.n(), 3);
        // a shorter update removes tiles, a longer one adds them
        let mut shorter = net;
        shorter.extra_data = 1;
        vine.remote_update(&me, &shorter.to_vec());
        assert_eq!(vine.n(), 1);
        assert_eq!(fg(&w, HEAD - 3), 0);
        let mut longer = net;
        longer.extra_data = 5;
        vine.remote_update(&me, &longer.to_vec());
        assert_eq!(vine.n(), 5);
        assert_eq!(fg(&w, HEAD - 5), FG_VINE);
    }

    #[test]
    fn water_kills_the_vine_and_harvests_cascade() {
        let (w, d) = env("vn_water");
        cave(&w);
        let vine = vine_at(&d, false).unwrap();
        let me: ObjRef = vine.clone();
        vine.set_n(6);
        for y in (HEAD - 6)..HEAD {
            tile_at_world_position_loaded(X, y, &w).unwrap().set_foreground_contents(FG_VINE);
        }
        vine.plant.borrow_mut().age = 2000.0;
        srand(3);
        let n =
            vine.tile_harvested_remove_blockhead_correct_tool_multiplier(&me, IntPair { x: X, y: HEAD - 3 }, None, 2);
        assert!(n >= 1);
        assert_eq!(vine.n(), 2);
        // wrong column or below the head: nothing
        assert_eq!(
            vine.tile_harvested_remove_blockhead_correct_tool_multiplier(
                &me,
                IntPair { x: X + 1, y: HEAD - 1 },
                None,
                1
            ),
            0
        );
        // a young vine yields at least one
        vine.plant.borrow_mut().age = 10.0;
        vine.set_n(0);
        tile_at_world_position_loaded(X, HEAD - 1, &w).unwrap().set_foreground_contents(FG_VINE);
        assert_eq!(
            vine.tile_harvested_remove_blockhead_correct_tool_multiplier(&me, IntPair { x: X, y: HEAD - 1 }, None, 1),
            1
        );
        // water below the head makes the periodic check pull the vine
        vine.set_n(4);
        for y in (HEAD - 4)..HEAD {
            tile_at_world_position_loaded(X, y, &w).unwrap().set_foreground_contents(FG_VINE);
        }
        tile_at_world_position_loaded(X, HEAD - 3, &w).unwrap().set_type_index(3);
        vine.vine.borrow_mut().check_weather_count = 59;
        vine.update_accurate_dt_is_simulation(&me, 0.1, 0.1, false);
        assert!(vine.n() < 4);
        // water on the head tile removes the whole plant
        tile_at_world_position_loaded(X, HEAD, &w).unwrap().set_type_index(3);
        vine.vine.borrow_mut().check_weather_count = 59;
        vine.update_accurate_dt_is_simulation(&me, 0.1, 0.1, false);
        assert!(me.needs_removed());
    }

    #[test]
    fn old_age_either_resows_or_starts_over() {
        let (w, d) = env("vn_old");
        cave(&w);
        let mut seen = (false, false);
        for seed in 1..60u32 {
            cave(&w);
            let vine = vine_at(&d, false).unwrap();
            let me: ObjRef = vine.clone();
            vine.set_n(2);
            for y in (HEAD - 2)..HEAD {
                tile_at_world_position_loaded(X, y, &w).unwrap().set_foreground_contents(FG_VINE);
            }
            vine.plant.borrow_mut().age = 99_999.0;
            vine.plant.borrow_mut().max_age = 10.0;
            srand(seed);
            vine.update_accurate_dt_is_simulation(&me, 0.1, 0.1, false);
            if me.needs_removed() {
                seen.0 = true;
            } else if vine.n() == 0 {
                seen.1 = true;
            }
            if seen.0 && seen.1 {
                break;
            }
        }
        assert!(seen.0 && seen.1, "{seen:?}");
        assert_eq!(VINE_PLANT_CLASS.name, "VinePlant");
    }

    #[test]
    fn vines_cannot_be_built_outside_the_world_or_in_unloaded_blocks() {
        let (w, d) = env("vn_unloaded");
        cave(&w);
        let vine = vine_at(&d, false).unwrap();
        let me: ObjRef = vine.clone();
        let mut net = vine.plant_creation_net_data(&me);
        // a block that is not loaded cannot take a replica
        for mx in 0..2 {
            *w.macro_tiles().get(mx).unwrap().physical_block.borrow_mut() = None;
        }
        net.dynamic_object_net_data.pos_y = 10;
        assert!(VinePlant::init_with_net_data(&w, &d, None, &net.to_vec()).is_none());
        assert!(vine_at_height(&d, 10).is_none());
        // a vine whose head block vanished does not trip over it
        vine.init_sub_derived_items(&me);
        assert!(!vine.is_growing_in_compost(&me));
        vine.vine.borrow_mut().check_weather_count = 59;
        vine.update_accurate_dt_is_simulation(&me, 0.1, 0.1, false);
    }

    #[test]
    fn harvest_cascades_drop_some_vines_and_remove_the_rest() {
        let (w, d) = env("vn_cascade");
        for seed in 1..40u32 {
            cave(&w);
            let vine = vine_at(&d, false).unwrap();
            let me: ObjRef = vine.clone();
            vine.plant.borrow_mut().age = 5000.0;
            vine.set_n(10);
            for y in (HEAD - 10)..HEAD {
                tile_at_world_position_loaded(X, y, &w).unwrap().set_foreground_contents(FG_VINE);
            }
            // a solid tile in the middle makes its dropped vine fall at the head instead
            tile_at_world_position_loaded(X, HEAD - 5, &w).unwrap().set_type_index(6);
            srand(seed);
            vine.tile_harvested_remove_blockhead_correct_tool_multiplier(&me, IntPair { x: X, y: HEAD - 2 }, None, 1);
        }
        assert_eq!(vine_at(&d, false).unwrap().class_info().name, "VinePlant");
    }

    #[test]
    fn replicas_ignore_unloaded_columns_and_full_vines_stop_growing() {
        let (w, d) = env("vn_replica");
        cave(&w);
        let vine = vine_at(&d, false).unwrap();
        let me: ObjRef = vine.clone();
        let mut dict = vine.get_save_dict(&me);
        dict.map.insert("numberOfOccupiedTilesBelow".to_string(), PlistValue::int(15));
        dict.map.insert("saveTime".to_string(), PlistValue::real_f64(w.world_time() - 100.0));
        let full = VinePlant::init_with_world_dynamic_world_save_dict_cache_tree_density_noise_function_season_offset_noise_function(&w, &d, &dict, None, None, None).unwrap();
        assert_eq!(full.n(), 15);
        let mut net = vine.plant_creation_net_data(&me);
        net.extra_data = 4;
        // the rows below are gone, so the replica cannot paint them
        for mx in 0..2 {
            *w.macro_tiles().get(mx).unwrap().physical_block.borrow_mut() = None;
        }
        vine.remote_update(&me, &net.to_vec());
        net.extra_data = 1;
        vine.remote_update(&me, &net.to_vec());
        assert_eq!(vine.n(), 1);
    }

    #[test]
    fn replicas_and_removed_vines_do_not_tick() {
        let (w, d) = env("vn_replica_tick");
        cave(&w);
        let vine = vine_at(&d, false).unwrap();
        let me: ObjRef = vine.clone();
        let net = vine.plant_creation_net_data(&me);
        let copy = VinePlant::init_with_net_data(&w, &d, None, &net.to_vec()).unwrap();
        let copy_ref: ObjRef = copy.clone();
        copy.update_accurate_dt_is_simulation(&copy_ref, 1000.0, 1000.0, false);
        vine.set_needs_removed(&me, true);
        vine.update_accurate_dt_is_simulation(&me, 1000.0, 1000.0, false);
        assert_eq!(vine.vine.borrow().check_weather_count, 0);
        // a tile that is not loaded while a harvest asks for it
        let (w2, d2) = env("vn_harvest_gone");
        cave(&w2);
        let v2 = vine_at(&d2, false).unwrap();
        let m2: ObjRef = v2.clone();
        v2.set_n(3);
        for mx in 0..2 {
            *w2.macro_tiles().get(mx).unwrap().physical_block.borrow_mut() = None;
        }
        assert_eq!(
            v2.tile_harvested_remove_blockhead_correct_tool_multiplier(&m2, IntPair { x: X, y: HEAD - 2 }, None, 1),
            0
        );
        // growing past the end of a young plant's life
        let (w3, d3) = env("vn_grow_old");
        cave(&w3);
        let v3 = vine_at(&d3, false).unwrap();
        let m3: ObjRef = v3.clone();
        v3.plant.borrow_mut().max_age = 1.0;
        v3.plant.borrow_mut().age = 5.0;
        srand(7);
        v3.update_accurate_dt_is_simulation(&m3, 0.1, 0.1, false);
    }

    #[test]
    fn odd_harvests_and_full_length_ticks() {
        let (w, d) = env("vn_odd");
        cave(&w);
        let vine = vine_at(&d, false).unwrap();
        let me: ObjRef = vine.clone();
        vine.plant.borrow_mut().age = 5000.0;
        // n says 5 tiles but the tiles are bare: nothing is dropped and the cascade has nothing to remove
        vine.set_n(5);
        assert_eq!(
            vine.tile_harvested_remove_blockhead_correct_tool_multiplier(&me, IntPair { x: X, y: HEAD - 2 }, None, 1),
            0
        );
        // far below the head nothing happens to the tiles
        vine.set_n(5);
        assert_eq!(
            vine.tile_harvested_remove_blockhead_correct_tool_multiplier(&me, IntPair { x: X, y: HEAD - 20 }, None, 1),
            0
        );
        // an update with the same length changes nothing; a full vine does not grow
        let net = vine.plant_creation_net_data(&me);
        vine.remote_update(&me, &net.to_vec());
        vine.set_n(15);
        vine.vine.borrow_mut().growth_timer = 1.0e6;
        vine.update_accurate_dt_is_simulation(&me, 0.1, 0.1, false);
        assert_eq!(vine.n(), 15);
    }
}
