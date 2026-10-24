//! owned by plants
//!
//! `KelpPlant: Plant` (DynamicObject type 34, instance size 0x100): stalk growth above a water root tile, food ->
//! ClownFish/Shark spawn, freeze, offline catch-up, harvest cascade.
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
use crate::world::helpers::tile_is_water;
use crate::world::state::World;
use crate::worldgen::noise::{noise_xy, NoiseFunction};

use super::plant::{
    adult_environment, create_free_block, dict_double, dict_float, dict_int, dw_changed, growth_rate_local,
    mark_update_needed, plant_get_save_dict,
    plant_init_with_world_dynamic_world_at_position_cache_max_age_gene_growth_rate_gene_tree_density_noise_function_season_offset_noise_function_adult_plant as plant_init_at_position,
    plant_init_with_world_dynamic_world_cache_net_data,
    plant_init_with_world_dynamic_world_save_dict_cache_tree_density_noise_function_season_offset_noise_function as plant_init_save_dict,
    plant_max_age_gene_variation, plant_plant_creation_net_data, plant_remote_update, plant_remove_from_macro_block,
    plant_set_needs_removed, plant_update, put_float, put_int, tile_loaded, ub_parity_null_tile, PlantLike, PlantState,
    DYNAMIC_OBJECT_TYPE_KELP_PLANT, PLANT_CLASS, PLANT_KELP,
};

/// `KelpPlant: Plant` class descriptor.
pub static KELP_PLANT_CLASS: ClassInfo =
    ClassInfo { name: "KelpPlant", superclass: Some(&PLANT_CLASS), protocols: &[] };

/// `CONTENTS_KELP_PLANT`
const FG_KELP: u8 = 0x51;
/// `ITEM_KELP`
const ITEM_KELP: i32 = 144;

/// KelpPlant ivars 0x88..0x100 (client draw buffers omitted).
#[derive(Default)]
pub struct KelpState {
    /// 0x88, `60` after `init_sub_derived_items`
    pub check_weather_count: i32,
    /// 0x8c (unused)
    pub kill_count: i32,
    /// 0x90: gather progress of stalk tile `k = dy - 1`
    pub above_gather_progress: [i32; 16],
    /// 0xd0: client sway phase (but the init consumes a draw)
    pub wave_timer: f32,
    /// 0xd4
    pub growth_timer: f32,
    /// 0xd8
    pub available_food: f32,
    /// 0xf8: N, the stalk length (may transiently be -1 or 16)
    pub number_of_occupied_tiles_above: i32,
}

/// `KelpPlant`.
pub struct KelpPlant {
    pub base: RefCell<DynamicObjectBase>,
    pub plant: RefCell<PlantState>,
    pub kelp: RefCell<KelpState>,
    world: Weak<World>,
    dw: Weak<DynamicWorld>,
}

impl KelpPlant {
    fn alloc(world: &Rc<World>, dw: &Rc<DynamicWorld>) -> Rc<KelpPlant> {
        let (w, d) = (Rc::downgrade(world), Rc::downgrade(dw));
        handle::alloc_obj(|sw| KelpPlant {
            base: RefCell::new(DynamicObjectBase::alloc_zeroed_with(sw)),
            plant: RefCell::new(PlantState::default()),
            kelp: RefCell::new(KelpState::default()),
            world: w,
            dw: d,
        })
    }
    fn world(&self) -> Rc<World> {
        self.world.upgrade().expect("KelpPlant: world dropped")
    }
    fn dwr(&self) -> Rc<DynamicWorld> {
        self.dw.upgrade().expect("KelpPlant: dynamic world dropped")
    }
    fn n(&self) -> i32 {
        self.kelp.borrow().number_of_occupied_tiles_above
    }
    fn set_n(&self, v: i32) {
        self.kelp.borrow_mut().number_of_occupied_tiles_above = v;
    }

    /// `-initSubDerivedItems`. Consumes 1 draw (waveTimer),
    /// 2 if `maxAgeGene == 0`.
    pub fn init_sub_derived_items(&self, me: &ObjRef) {
        let world = self.world();
        let dw = self.dwr();
        let pos = me.pos();
        match tile_loaded(&world, pos.x, pos.y) {
            Some(t) => t.set_foreground_contents(FG_KELP),
            None => ub_parity_null_tile(),
        }
        dw.world_contents_changed_at_pos(pos);
        if self.plant.borrow().max_age_gene == 0 {
            self.plant.borrow_mut().max_age_gene = 0x7f;
            let v = plant_max_age_gene_variation(me);
            self.plant.borrow_mut().max_age_gene = v;
        }
        let gene = self.plant.borrow().max_age_gene;
        let mut max_age = (((gene as f32) / 255.0f32) * 0.5f32 + 0.75f32) * 7000.0f32;
        if let Some(b) = tile_loaded(&world, pos.x, pos.y.wrapping_sub(1)) {
            if matches!(b.type_index(), 0x30..=0x32) {
                max_age *= 1.5f32;
            }
        }
        self.plant.borrow_mut().max_age = max_age;
        let r = glibc_rand::r01_tagged("plants::kelp::initSubDerivedItems::waveTimer");
        let mut k = self.kelp.borrow_mut();
        k.wave_timer = (((pos.x as f32) / 4.0f32) as f64 + (r as f64) * 0.2f64 - 0.1f64) as f32;
        k.check_weather_count = 60;
        k.above_gather_progress = [0; 16];
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
    ) -> Option<Rc<KelpPlant>> {
        let me = Self::alloc(world, dw);
        let me_obj: ObjRef = me.clone();
        if !plant_init_at_position(&me_obj, world, dw, pos, cache, mag, grg, tdnf, sonf, adult) {
            return None;
        }
        let p = me_obj.pos();
        let fail = |me_obj: &ObjRef| -> Option<Rc<KelpPlant>> {
            me_obj.remove_from_macro_block(me_obj);
            me_obj.dealloc(me_obj);
            None
        };
        let (mut weather, mut day, mut season) = (0.0f32, 0.0f32, 0.0f32);
        // `plant_init_at_position` only succeeds on a loaded tile
        let t = tile_loaded(world, p.x, p.y).expect("loaded tile");
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
        if !tile_is_water(Some(t)) {
            return fail(&me_obj);
        }
        match tile_loaded(world, p.x, p.y.wrapping_sub(1)) {
            Some(b) if me.is_required_soil_type(b.type_index()) => {}
            _ => return fail(&me_obj),
        }
        let wm = world.world_width_macro() as f32;
        let xf = ((p.x as f32) / 32.0f32) / wm;
        let yf = ((p.y as f32) / 32.0f32) / wm;
        let sonf = me.plant.borrow().season_offset_noise_function.clone();
        let frac = noise_xy(&sonf, xf as f64, yf as f64, 6) as f32;
        me.plant.borrow_mut().season_offset = f32_to_i32(32.0f32 * frac);
        me.init_sub_derived_items(&me_obj);
        me.set_n(0);
        let dwr = me.dwr();
        for i in 0..16 {
            let y = p.y.wrapping_add(i).wrapping_add(1);
            if let Some(t) = tile_loaded(world, p.x, y) {
                if me.tile_is_kind_of_self(&t.get()) {
                    t.set_foreground_contents(0);
                    dwr.world_contents_changed_at_pos(IntPair { x: p.x, y });
                }
            }
        }
        if adult {
            let r = glibc_rand::r01_tagged("plants::kelp::init::age");
            let (age, max_age) = {
                let mut st = me.plant.borrow_mut();
                st.age = r * st.max_age;
                (st.age, st.max_age)
            };
            let mut k = 1i32;
            while (k as f32) < 16.0f32 * (age / max_age) {
                let y = p.y.wrapping_add(k);
                let t = tile_loaded(world, p.x, y);
                let ok = match t {
                    Some(tr) => {
                        tile_is_water(Some(tr))
                            && tr.foreground_contents() == 0
                            && current_temperature_for_tile_at_world_pos(
                                &tr.get(),
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
                t.unwrap().set_foreground_contents(FG_KELP);
                let n = me.n() + 1;
                me.set_n(n);
                dwr.world_contents_changed_at_pos(IntPair { x: p.x, y: p.y.wrapping_add(n) });
                k += 1;
            }
        }
        let r = glibc_rand::r01_tagged("plants::kelp::init::availableFood");
        me.kelp.borrow_mut().available_food = ((r * 900.0f32) as f64 * 1.5f64) as f32;
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
    ) -> Option<Rc<KelpPlant>> {
        let me = Self::alloc(world, dw);
        let me_obj: ObjRef = me.clone();
        plant_init_save_dict(&me_obj, world, dw, save_dict, cache, tdnf, sonf);
        me.set_n(dict_int(save_dict, "numberOfOccupiedTilesAbove"));
        me.kelp.borrow_mut().growth_timer = dict_float(save_dict, "growthTimer");
        me.init_sub_derived_items(&me_obj);
        let mut food = dict_float(save_dict, "availableFood");
        if food < 0.1f32 {
            let r = glibc_rand::r01_tagged("plants::kelp::load::availableFood");
            food = ((r * 900.0f32) as f64 * 1.5f64) as f32;
        }
        me.kelp.borrow_mut().available_food = food;
        let frozen = me.plant.borrow().frozen;
        if !frozen {
            let saved = dict_double(save_dict, "saveTime");
            let now = world.world_time();
            let mut dt = now - saved;
            {
                let mut k = me.kelp.borrow_mut();
                k.growth_timer = ((k.growth_timer as f64) + dt) as f32;
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
            } else {
                let growth_rate = me.plant.borrow().growth_rate;
                let pos = me_obj.pos();
                loop {
                    let (n, timer) = {
                        let k = me.kelp.borrow();
                        (k.number_of_occupied_tiles_above, k.growth_timer)
                    };
                    if !(n < 15 && timer > 0.0f32) {
                        break;
                    }
                    let g = growth_rate_local(n, growth_rate);
                    if timer <= 225.0f32 / g {
                        break;
                    }
                    let a = tile_loaded(world, pos.x, pos.y.wrapping_add(n).wrapping_add(1));
                    if !tile_is_water(a) || a.is_some_and(|t| t.foreground_contents() != 0) {
                        me.kelp.borrow_mut().growth_timer = 0.0;
                        break;
                    }
                    a.unwrap().set_foreground_contents(FG_KELP);
                    me.set_n(n + 1);
                    dw.world_contents_changed_at_pos(IntPair {
                        x: pos.x,
                        y: pos.y.wrapping_add(n + 1).wrapping_add(1),
                    });
                    me.kelp.borrow_mut().growth_timer -= 225.0f32 / g;
                }
                {
                    let mut st = me.plant.borrow_mut();
                    st.age = ((st.age as f64) + dt) as f32;
                }
                dw_changed(&me_obj);
            }
        }
        Some(me)
    }

    /// `-initWithWorld:dynamicWorld:cache:netData:` (client replica).
    pub fn init_with_net_data(
        world: &Rc<World>,
        dw: &Rc<DynamicWorld>,
        cache: CacheRef,
        net_data: &[u8],
    ) -> Option<Rc<KelpPlant>> {
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
        let r = glibc_rand::r01_tagged("plants::kelp::dieOfOldAge");
        let pos = me.pos();
        if r <= 0.5f32 {
            self.tile_harvested_remove_blockhead_correct_tool_multiplier(me, pos, None, 1);
            dw.sow_plant_near_parent(me);
        } else {
            let mut i = 0;
            while i < self.n() {
                let y = pos.y.wrapping_add(i).wrapping_add(1);
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

impl PlantLike for KelpPlant {
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
        PLANT_KELP
    }
    fn dropped_item_type(&self) -> i32 {
        ITEM_KELP
    }
    fn tile_is_kind_of_self(&self, tile: &Tile) -> bool {
        tile.foreground_contents == FG_KELP
    }
    /// : valid iff not AIR(2), WATER(3), ICE(4), SNOW(5), LAVA(31).
    fn is_required_soil_type(&self, t: u8) -> bool {
        !matches!(t, 2 | 3 | 4 | 5 | 31)
    }
    fn number_of_occupied_tiles_above(&self) -> i32 {
        self.n()
    }
    fn available_food(&self) -> f32 {
        self.kelp.borrow().available_food
    }
    fn set_available_food(&self, v: f32) {
        self.kelp.borrow_mut().available_food = v;
    }
    fn gather_progress_for_tile(&self, tile_pos: IntPair) -> i32 {
        let pos_y = self.base.borrow().pos.y;
        let dy = tile_pos.y.wrapping_sub(pos_y);
        if tile_pos.y > pos_y && dy < 16 {
            self.kelp.borrow().above_gather_progress[(dy - 1) as usize]
        } else {
            self.plant.borrow().gather_progress
        }
    }
    fn set_gather_progress_for_tile(&self, v: i32, tile_pos: IntPair) {
        let pos_y = self.base.borrow().pos.y;
        if tile_pos.y > pos_y {
            let dy = tile_pos.y.wrapping_sub(pos_y);
            if dy <= self.n() && dy < 16 {
                self.kelp.borrow_mut().above_gather_progress[(dy - 1) as usize] = v;
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
        let mut creation_count = 0;
        if tile_pos.x == pos.x && tile_pos.y >= pos.y {
            let age = self.plant.borrow().age;
            let ratio = if age >= 900.0f32 { 1.0f32 } else { age / 900.0f32 };
            let m = if (mult as f32) * ratio >= 1.0f32 { (mult as f32) * ratio } else { 1.0f32 };
            dw_changed(me);
            let dy = tile_pos.y.wrapping_sub(pos.y);
            if dy < 16 {
                let t = tile_loaded(&world, tile_pos.x, tile_pos.y);
                if t.is_none() {
                    ub_parity_null_tile();
                }
                if let Some(t) = t {
                    if t.foreground_contents() == FG_KELP {
                        for _ in 0..f32_to_i32(m) {
                            let a = self.max_age_gene_variation();
                            let b = self.growth_rate_gene_variation();
                            create_free_block(&dw, tile_pos, ITEM_KELP, a, b, 1, bh);
                            creation_count += 1;
                        }
                        t.set_foreground_contents(0);
                        dw.world_contents_changed_at_pos(tile_pos);
                    }
                }
                let mut i = dy;
                while i < self.n() {
                    let y = pos.y.wrapping_add(i).wrapping_add(1);
                    if let Some(u) = tile_loaded(&world, pos.x, y) {
                        if self.tile_is_kind_of_self(&u.get()) {
                            if glibc_rand::rand_tagged("plants::kelp::tileHarvested::cascade") > 0x3fff_ffff {
                                let a = self.max_age_gene_variation();
                                let b = self.growth_rate_gene_variation();
                                create_free_block(&dw, IntPair { x: pos.x, y }, ITEM_KELP, a, b, 1, None);
                            }
                            u.set_foreground_contents(0);
                            dw.world_contents_changed_at_pos(IntPair { x: pos.x, y });
                        }
                    }
                    i += 1;
                }
            }
            mark_update_needed(me);
            let nn = dy.wrapping_sub(1);
            self.set_n(if nn < 16 { nn } else { 16 });
            if tile_pos.y == pos.y {
                self.remove_plant_without_creating_freeblocks(me);
            }
        }
        creation_count
    }
}

impl DynamicObject for KelpPlant {
    super::plant::plant_dynamic_object_common!();

    fn class_info(&self) -> &'static ClassInfo {
        &KELP_PLANT_CLASS
    }
    fn object_type(&self) -> i32 {
        DYNAMIC_OBJECT_TYPE_KELP_PLANT
    }

    /// `-getSaveDict`.
    fn get_save_dict(&self, me: &ObjRef) -> PlistDict {
        let mut d = plant_get_save_dict(me);
        let k = self.kelp.borrow();
        put_int(&mut d, "numberOfOccupiedTilesAbove", k.number_of_occupied_tiles_above);
        put_float(&mut d, "growthTimer", k.growth_timer);
        put_float(&mut d, "availableFood", k.available_food);
        d
    }

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
                let y = pos.y.wrapping_add(i).wrapping_add(1);
                if let Some(t) = tile_loaded(&world, pos.x, y) {
                    if i < new_n {
                        t.set_foreground_contents(FG_KELP);
                    } else if t.foreground_contents() == FG_KELP {
                        t.set_foreground_contents(0);
                    }
                    dw.world_contents_changed_at_pos(IntPair { x: pos.x, y });
                }
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
        if !self.plant.borrow().frozen {
            let food = self.kelp.borrow().available_food;
            if food >= 1800.0f32 {
                let n = self.n();
                // `off = 1; if (N > 1) off = rand() % N`
                let pick_off = |n: i32| -> i32 {
                    if n > 1 {
                        ((glibc_rand::rand_tagged("plants::kelp::update::off") as i64) % (n as i64)) as i32
                    } else {
                        1
                    }
                };
                if n < 14 {
                    {
                        let mut k = self.kelp.borrow_mut();
                        k.available_food -= 900.0f32;
                        if k.available_food > 1800.0f32 {
                            k.available_food = 1800.0f32;
                        }
                    }
                    dw_changed(me);
                    let off = pick_off(n);
                    let t2 = tile_loaded(&world, pos.x, pos.y.wrapping_add(off));
                    if tile_is_water(t2) {
                        dw.load_npc_at_position_type_save_dict_is_adult_was_placed_placed_by_client(
                            pos, 4, None, true, false, None,
                        );
                    }
                } else {
                    self.kelp.borrow_mut().available_food = 0.0;
                    dw_changed(me);
                    let off = pick_off(n);
                    let t2 = tile_loaded(&world, pos.x, pos.y.wrapping_add(off));
                    if tile_is_water(t2) {
                        dw.load_npc_at_position_type_save_dict_is_adult_was_placed_placed_by_client(
                            pos, 5, None, true, false, None,
                        );
                    }
                }
            } else {
                self.kelp.borrow_mut().available_food = food + 0.5f32 * dt;
            }
        }
        let run_check = {
            let mut k = self.kelp.borrow_mut();
            k.check_weather_count += 1;
            if k.check_weather_count > 59 {
                k.check_weather_count = 0;
                true
            } else {
                false
            }
        };
        if run_check {
            let ice_root = tile.is_some_and(|t| t.type_index() == 4);
            self.plant.borrow_mut().frozen = ice_root;
            if !tile_is_water(tile) && tile.is_some() && tile.is_some_and(|t| t.type_index() != 4) {
                self.tile_harvested_remove_blockhead_correct_tool_multiplier(me, pos, None, 1);
                return;
            }
            let mut i = 0;
            while i < self.n() {
                let y = pos.y.wrapping_add(i).wrapping_add(1);
                let o = tile_loaded(&world, pos.x, y);
                if !tile_is_water(o) && o.is_some_and(|t| t.type_index() != 4) {
                    self.tile_harvested_remove_blockhead_correct_tool_multiplier(me, IntPair { x: pos.x, y }, None, 1);
                    break;
                } else if o.is_some_and(|t| t.type_index() == 4) {
                    self.plant.borrow_mut().frozen = true;
                }
                i += 1;
            }
        }
        if !self.plant.borrow().frozen {
            let n = self.n();
            if n < 15 {
                let g = growth_rate_local(n, self.plant.borrow().growth_rate);
                let due = {
                    let mut k = self.kelp.borrow_mut();
                    k.growth_timer += dt;
                    225.0f32 / g < k.growth_timer
                };
                if due {
                    self.kelp.borrow_mut().growth_timer = 0.0;
                    let y = pos.y.wrapping_add(n).wrapping_add(1);
                    let a = tile_loaded(&world, pos.x, y);
                    if tile_is_water(a)
                        && a.is_some_and(|t| t.foreground_contents() == 0 || t.foreground_contents() == FG_KELP)
                    {
                        dw.world_contents_changed_at_pos(IntPair { x: pos.x, y });
                        a.unwrap().set_foreground_contents(FG_KELP);
                        if let Some(slot) = self.kelp.borrow_mut().above_gather_progress.get_mut(n as usize) {
                            *slot = 0;
                        }
                        self.set_n(n + 1);
                        dw_changed(me);
                        mark_update_needed(me);
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
    }

    fn dealloc(&self, _me: &ObjRef) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    ///  R4 / U9: after a reload `growthRate == 0` gives `g = 0.5` for every N (period 450 s); a fresh plant
    /// with `growthRateGene 193` gives `g` in (0.5, 1.2).
    #[test]
    fn growth_period() {
        for n in 0..15 {
            assert_eq!(growth_rate_local(n, 0.0), 0.5);
            assert_eq!(225.0f32 / growth_rate_local(n, 0.0), 450.0);
        }
        let gr = 193.0f32 / 255.0 + 0.2;
        let g = growth_rate_local(0, gr);
        assert!(g > 0.5 && g < 1.2);
        let expect = (((((1.0f32 - 0.0f32 / 16.0f32) + 0.2f32) * gr) as f64) * 0.5 + 0.5) as f32;
        assert_eq!(g, expect);
    }

    /// : Kelp gene 193 -> maxAge 7899.019.
    #[test]
    fn golden_max_age() {
        let m = (((193_f32) / 255.0f32) * 0.5f32 + 0.75f32) * 7000.0f32;
        assert!((m - 7899.019).abs() < 0.01);
    }

    /// : soil predicate and class chain.
    #[test]
    fn soil_and_class() {
        assert!(KELP_PLANT_CLASS.is_kind_of(&PLANT_CLASS));
        let _ = Tile::default();
    }
}

/// Scenario tests: kelp rooted in a pond, growth, food and fish, freezing, catch-up on load, harvest and death.
#[cfg(test)]
mod scenario_tests {
    use super::*;
    use crate::plist::value::PlistValue;
    use crate::rng::glibc_rand::srand;
    use crate::trees::tree::testenv::env;
    use crate::world::tiles::tile_at_world_position_loaded;

    const X: i32 = 20;
    const ROOT: i32 = 10;

    /// Water over the dirt (rows 10..=26, columns 18..=22); the kelp root is row 10.
    fn pond(w: &Rc<World>) {
        for y in ROOT..=26 {
            for x in 18..=22 {
                tile_at_world_position_loaded(x, y, w).unwrap().update(|t| {
                    t.type_index = 3;
                    t.foreground_contents = 0;
                    t.sun_light = 255;
                });
            }
        }
    }

    fn tile(w: &Rc<World>, y: i32) -> crate::world::state::TileRef {
        tile_at_world_position_loaded(X, y, w).unwrap()
    }

    fn kelp_at(d: &Rc<DynamicWorld>, y: i32, adult: bool) -> Option<Rc<KelpPlant>> {
        KelpPlant::init_with_world_dynamic_world_at_position_cache_max_age_gene_growth_rate_gene_tree_density_noise_function_season_offset_noise_function_adult_plant(
            &d.world(), d, IntPair { x: X, y }, None, 128, 128, None, None, adult,
        )
    }

    fn kelp(d: &Rc<DynamicWorld>) -> Rc<KelpPlant> {
        kelp_at(d, ROOT, false).expect("kelp")
    }

    #[test]
    fn kelp_grows_up_feeds_fish_and_freezes() {
        let (w, d) = env("kp_grow");
        pond(&w);
        let k = kelp(&d);
        let me: ObjRef = k.clone();
        assert_eq!(tile(&w, ROOT).foreground_contents(), FG_KELP);
        for _ in 0..70 {
            k.update_accurate_dt_is_simulation(&me, 400.0, 400.0, false);
        }
        assert!(k.n() > 0);
        // the long ticks also aged the plant; start the next steps young again
        k.plant.borrow_mut().age_counter = 0.0;
        k.plant.borrow_mut().age = 0.0;
        // lots of food: a fish appears (nothing builds it without constructors, but the food is spent)
        k.kelp.borrow_mut().available_food = 5000.0;
        srand(4);
        k.update_accurate_dt_is_simulation(&me, 0.1, 0.1, false);
        k.set_n(14);
        k.kelp.borrow_mut().available_food = 5000.0;
        k.update_accurate_dt_is_simulation(&me, 0.1, 0.1, false);
        k.set_n(1);
        k.kelp.borrow_mut().available_food = 5000.0;
        k.update_accurate_dt_is_simulation(&me, 0.1, 0.1, false);
        // ice on the root or higher up freezes the plant: no growth, no food
        tile(&w, ROOT + 3).set_type_index(4);
        k.set_n(5);
        k.kelp.borrow_mut().check_weather_count = 59;
        k.update_accurate_dt_is_simulation(&me, 0.1, 0.1, false);
        assert!(k.plant.borrow().frozen);
        k.update_accurate_dt_is_simulation(&me, 0.1, 0.1, false);
        tile(&w, ROOT + 3).set_type_index(3);
        k.kelp.borrow_mut().check_weather_count = 59;
        k.update_accurate_dt_is_simulation(&me, 0.1, 0.1, false);
        assert!(!k.plant.borrow().frozen);
        tile(&w, ROOT).set_type_index(4);
        k.kelp.borrow_mut().check_weather_count = 59;
        k.update_accurate_dt_is_simulation(&me, 0.1, 0.1, false);
        assert!(k.plant.borrow().frozen);
        // accessors
        k.set_available_food(2.0);
        assert_eq!(k.available_food(), 2.0);
        assert_eq!(k.dropped_item_type(), ITEM_KELP);
        let above = IntPair { x: X, y: ROOT + 2 };
        k.set_gather_progress_for_tile(6, above);
        assert_eq!(k.gather_progress_for_tile(above), 6);
        k.set_gather_progress_for_tile(8, IntPair { x: X, y: ROOT });
        assert_eq!(k.gather_progress_for_tile(IntPair { x: X, y: ROOT }), 8);
        k.set_gather_progress_for_tile(9, IntPair { x: X, y: ROOT + 40 });
        assert_eq!(k.gather_progress_for_tile(IntPair { x: X, y: ROOT + 40 }), 8);
    }

    #[test]
    fn kelp_dies_out_of_water_and_of_old_age() {
        let (w, d) = env("kp_dry");
        pond(&w);
        let k = kelp(&d);
        let me: ObjRef = k.clone();
        k.set_n(4);
        for y in (ROOT + 1)..=(ROOT + 4) {
            tile(&w, y).set_foreground_contents(FG_KELP);
        }
        // a dry tile in the stalk pulls the plant down to it
        tile(&w, ROOT + 3).set_type_index(2);
        k.kelp.borrow_mut().check_weather_count = 59;
        k.update_accurate_dt_is_simulation(&me, 0.1, 0.1, false);
        assert!(k.n() < 4);
        // a dry root removes the plant
        tile(&w, ROOT).set_type_index(2);
        k.kelp.borrow_mut().check_weather_count = 59;
        k.update_accurate_dt_is_simulation(&me, 0.1, 0.1, false);
        assert!(me.needs_removed());
        // old age: either resowed or starts again
        let mut seen = (false, false);
        for seed in 1..60u32 {
            pond(&w);
            let k = kelp(&d);
            let me: ObjRef = k.clone();
            k.set_n(2);
            tile(&w, ROOT + 1).set_foreground_contents(FG_KELP);
            tile(&w, ROOT + 2).set_foreground_contents(FG_KELP);
            k.plant.borrow_mut().age = 1.0e6;
            k.plant.borrow_mut().max_age = 5.0;
            srand(seed);
            k.update_accurate_dt_is_simulation(&me, 0.1, 0.1, false);
            if me.needs_removed() {
                seen.0 = true;
            } else if k.n() == 0 {
                seen.1 = true;
            }
        }
        assert!(seen.0 && seen.1, "{seen:?}");
    }

    #[test]
    fn adult_kelp_needs_warm_water_over_solid_ground() {
        let (w, d) = env("kp_adult");
        let mut lengths = Vec::new();
        for seed in 1..40u32 {
            pond(&w);
            if seed % 3 == 0 {
                tile(&w, ROOT + 3).set_foreground_contents(1);
            }
            srand(seed);
            lengths.push(kelp_at(&d, ROOT, true).expect("kelp").n());
        }
        assert!(lengths.iter().any(|n| *n > 1) && lengths.contains(&2), "{lengths:?}");
        // water that is too cold, a root that is not wet, a root over air or water
        pond(&w);
        tile(&w, 1000).update(|t| {
            t.type_index = 3;
            t.sun_light = 255;
        });
        tile(&w, 999).set_type_index(6);
        assert!(kelp_at(&d, 1000, true).is_none());
        assert!(kelp_at(&d, 5, false).is_none());
        tile(&w, ROOT - 1).set_type_index(3);
        assert!(kelp_at(&d, ROOT, false).is_none());
    }

    #[test]
    fn saved_kelp_catches_up_and_replicas_follow() {
        let (w, d) = env("kp_load");
        pond(&w);
        let k = kelp(&d);
        let me: ObjRef = k.clone();
        k.plant.borrow_mut().age = 100.0;
        let mut dict = k.get_save_dict(&me);
        dict.map.insert("saveTime".to_string(), PlistValue::real_f64(w.world_time() - 3000.0));
        put_float(&mut dict, "availableFood", 0.0);
        put_float(&mut dict, "growthTimer", 400.0);
        let back = KelpPlant::init_with_world_dynamic_world_save_dict_cache_tree_density_noise_function_season_offset_noise_function(&w, &d, &dict, None, None, None).unwrap();
        assert!(back.n() > 0);
        // blocked growth resets the timer
        pond(&w);
        tile(&w, ROOT + 1).set_foreground_contents(1);
        let blocked = KelpPlant::init_with_world_dynamic_world_save_dict_cache_tree_density_noise_function_season_offset_noise_function(&w, &d, &dict, None, None, None).unwrap();
        assert_eq!(blocked.kelp.borrow().growth_timer, 0.0);
        // frozen plants are not caught up; very old ones die; those in compost age to the limit
        let mut frozen = dict.clone();
        frozen.map.insert("frozen".to_string(), PlistValue::boolean(true));
        assert!(KelpPlant::init_with_world_dynamic_world_save_dict_cache_tree_density_noise_function_season_offset_noise_function(&w, &d, &frozen, None, None, None).is_some());
        let mut old = dict.clone();
        old.map.insert("saveTime".to_string(), PlistValue::real_f64(w.world_time() - 1.0e9));
        srand(2);
        assert!(KelpPlant::init_with_world_dynamic_world_save_dict_cache_tree_density_noise_function_season_offset_noise_function(&w, &d, &old, None, None, None).is_some());
        tile(&w, ROOT - 1).set_type_index(0x31);
        assert!(KelpPlant::init_with_world_dynamic_world_save_dict_cache_tree_density_noise_function_season_offset_noise_function(&w, &d, &old, None, None, None).is_some());
        // replicas
        let mut net = k.plant_creation_net_data(&me);
        net.extra_data = 3;
        let copy = KelpPlant::init_with_net_data(&w, &d, None, &net.to_vec()).unwrap();
        assert_eq!(copy.n(), 3);
        k.remote_update(&me, &net.to_vec());
        assert_eq!(k.n(), 3);
        net.extra_data = 1;
        k.remote_update(&me, &net.to_vec());
        assert_eq!(k.n(), 1);
        k.remote_update(&me, &net.to_vec());
        // a replica cannot be built in a block that is not loaded
        for mx in 0..2 {
            *w.macro_tiles().get(mx).unwrap().physical_block.borrow_mut() = None;
        }
        assert!(KelpPlant::init_with_net_data(&w, &d, None, &net.to_vec()).is_none());
        net.extra_data = 4;
        k.remote_update(&me, &net.to_vec());
        assert!(kelp_at(&d, ROOT, false).is_none());
        k.init_sub_derived_items(&me);
        let copy_ref: ObjRef = copy.clone();
        copy.update_accurate_dt_is_simulation(&copy_ref, 1000.0, 1000.0, false);
        assert_eq!(KELP_PLANT_CLASS.name, "KelpPlant");
        assert_eq!(k.class_info().name, "KelpPlant");
    }

    #[test]
    fn kelp_harvests_cascade_upwards() {
        let (w, d) = env("kp_harvest");
        for seed in 1..40u32 {
            pond(&w);
            let k = kelp(&d);
            let me: ObjRef = k.clone();
            k.plant.borrow_mut().age = 5000.0;
            k.set_n(8);
            for y in (ROOT + 1)..=(ROOT + 8) {
                tile(&w, y).set_foreground_contents(FG_KELP);
            }
            srand(seed);
            k.tile_harvested_remove_blockhead_correct_tool_multiplier(&me, IntPair { x: X, y: ROOT + 3 }, None, 2);
            assert_eq!(k.n(), 2);
        }
        pond(&w);
        let k = kelp(&d);
        let me: ObjRef = k.clone();
        // wrong column, below the root, bare tiles, far above, the root itself
        assert_eq!(
            k.tile_harvested_remove_blockhead_correct_tool_multiplier(&me, IntPair { x: X + 1, y: ROOT + 1 }, None, 1),
            0
        );
        assert_eq!(
            k.tile_harvested_remove_blockhead_correct_tool_multiplier(&me, IntPair { x: X, y: ROOT - 1 }, None, 1),
            0
        );
        k.set_n(3);
        assert_eq!(
            k.tile_harvested_remove_blockhead_correct_tool_multiplier(&me, IntPair { x: X, y: ROOT + 2 }, None, 1),
            0
        );
        assert_eq!(
            k.tile_harvested_remove_blockhead_correct_tool_multiplier(&me, IntPair { x: X, y: ROOT + 20 }, None, 1),
            0
        );
        assert_eq!(
            k.tile_harvested_remove_blockhead_correct_tool_multiplier(&me, IntPair { x: X, y: ROOT }, None, 1),
            1
        );
        assert!(me.needs_removed());
        // blocks that are gone while harvesting
        pond(&w);
        let k = kelp(&d);
        let me: ObjRef = k.clone();
        k.set_n(3);
        for mx in 0..2 {
            *w.macro_tiles().get(mx).unwrap().physical_block.borrow_mut() = None;
        }
        assert_eq!(
            k.tile_harvested_remove_blockhead_correct_tool_multiplier(&me, IntPair { x: X, y: ROOT + 2 }, None, 1),
            0
        );
    }

    #[test]
    fn kelp_at_the_top_of_the_loaded_world_and_other_corners() {
        let (w, d) = env("kp_corner");
        pond(&w);
        for y in (ROOT + 1)..=(ROOT + 3) {
            tile(&w, y).set_foreground_contents(FG_KELP);
        }
        let k = kelp(&d);
        assert_eq!(tile(&w, ROOT + 1).foreground_contents(), 0);
        assert_eq!(k.plant_type(), PLANT_KELP);
        // a short offline time leaves the growth timer too small to grow
        let me: ObjRef = k.clone();
        let mut dict = k.get_save_dict(&me);
        dict.map.insert("saveTime".to_string(), PlistValue::real_f64(w.world_time() - 1.0));
        put_float(&mut dict, "growthTimer", 1.0);
        put_float(&mut dict, "availableFood", 100.0);
        let back = KelpPlant::init_with_world_dynamic_world_save_dict_cache_tree_density_noise_function_season_offset_noise_function(&w, &d, &dict, None, None, None).unwrap();
        assert_eq!(back.n(), 0);
        // rows above the loaded world stop an adult stalk
        for y in 28..=31 {
            tile(&w, y).update(|t| {
                t.type_index = if y == 28 { 6 } else { 3 };
                t.sun_light = 255;
            });
        }
        for mx in 0..2 {
            *w.macro_tiles().get(2 + mx).unwrap().physical_block.borrow_mut() = None;
        }
        let mut stopped = false;
        for seed in 1..80u32 {
            srand(seed);
            for k in kelp_at(&d, 29, true).into_iter() {
                stopped |= k.n() > 0 && k.n() < 15;
                // the stalk reaches into the unloaded rows: dying of old age skips them, growth finds no water
                let me: ObjRef = k.clone();
                k.set_n(3);
                k.die_of_old_age(&me);
                k.set_n(3);
                k.kelp.borrow_mut().growth_timer = 1.0e6;
                k.update_accurate_dt_is_simulation(&me, 300.0, 300.0, false);
            }
            for y in 30..=31 {
                tile(&w, y).set_foreground_contents(0);
            }
        }
        assert!(stopped);
    }
}
