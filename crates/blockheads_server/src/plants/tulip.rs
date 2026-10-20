//! owned by plants
//!
//! `TulipPlant: Plant` (DynamicObject type 59, instance size 0xD8): the breedable flower with its colour/mix gene
//! words, bulbs and seeds.
//!
//! (item 2), 10 (U11).

use std::cell::RefCell;
use std::rc::{Rc, Weak};

use crate::dynworld::state::DynamicWorld;
use crate::fmath::cconv::f32_to_i32;
use crate::objects::base::{padded_bytes, DynamicObject, DynamicObjectBase};
use crate::objects::class_info::ClassInfo;
use crate::objects::handle::{self, downcast, CacheRef, ObjRef};
use crate::plist::value::PlistDict;
use crate::rng::glibc_rand;
use crate::types::netdata::PlantCreationNetData;
use crate::types::structs::{IntPair, WireStruct};
use crate::types::tile::Tile;
use crate::world::climate::season_for_world_x;
use crate::world::state::World;
use crate::worldgen::noise::{noise_xy, NoiseFunction};

use super::plant::{
    create_free_block, dict_float, dict_int, dw_changed, mark_update_needed, plant_environment_ok, plant_get_save_dict,
    plant_init_with_world_dynamic_world_at_position_cache_max_age_gene_growth_rate_gene_tree_density_noise_function_season_offset_noise_function_adult_plant as plant_init_at_position,
    plant_init_with_world_dynamic_world_cache_net_data,
    plant_init_with_world_dynamic_world_save_dict_cache_tree_density_noise_function_season_offset_noise_function as plant_init_save_dict,
    plant_plant_creation_net_data, plant_remote_update, plant_remove_from_macro_block, plant_set_needs_removed,
    plant_update, put_float, put_int, season_index, tile_loaded, ub_parity_null_tile, PlantLike, PlantState,
    DYNAMIC_OBJECT_TYPE_TULIP_PLANT, PLANT_CLASS, PLANT_TULIP,
};

/// `TulipPlant: Plant` class descriptor.
pub static TULIP_PLANT_CLASS: ClassInfo =
    ClassInfo { name: "TulipPlant", superclass: Some(&PLANT_CLASS), protocols: &[] };

/// `CONTENTS_TULIP_PLANT`
const FG_TULIP: u8 = 125;
/// `ITEM_TULIP_BULB`
const ITEM_TULIP_BULB: i32 = 296;
/// `ITEM_TULIP_SEED`
const ITEM_TULIP_SEED: i32 = 297;

/// TulipPlant ivars 0x88..0x9a (client colours/shader omitted).
#[derive(Default)]
pub struct TulipState {
    /// 0x88
    pub available_food: f32,
    /// 0x8c
    pub check_weather_count: i32,
    /// 0x90
    pub kill_count: i32,
    /// 0x94: nibbles dominantTop(12-15) recessiveTop(8-11) dominantBot(4-7) recessiveBot(0-3)
    pub color_genes: u16,
    /// 0x96: topMix (12-15), botMix (8-11)
    pub mix_genes: u16,
    /// 0x98
    pub mate_color_genes: u16,
}

/// `TulipPlant`.
pub struct TulipPlant {
    pub base: RefCell<DynamicObjectBase>,
    pub plant: RefCell<PlantState>,
    pub tulip: RefCell<TulipState>,
    world: Weak<World>,
    dw: Weak<DynamicWorld>,
}

/// `tulipGeneOrderForPigment` (lower order wins).
pub fn tulip_gene_order_for_pigment(p: u16) -> i32 {
    match p {
        0 => 99,
        1 => 3,
        2 => 5,
        3 => 0,
        4 => 1,
        5 => 2,
        6 => 6,
        7 => 7,
        8 => 4,
        9 => 99,
        _ => 99,
    }
}

impl TulipPlant {
    fn alloc(world: &Rc<World>, dw: &Rc<DynamicWorld>) -> Rc<TulipPlant> {
        let (w, d) = (Rc::downgrade(world), Rc::downgrade(dw));
        handle::alloc_obj(|sw| TulipPlant {
            base: RefCell::new(DynamicObjectBase::alloc_zeroed_with(sw)),
            plant: RefCell::new(PlantState::default()),
            tulip: RefCell::new(TulipState::default()),
            world: w,
            dw: d,
        })
    }
    fn world(&self) -> Rc<World> {
        self.world.upgrade().expect("TulipPlant: world dropped")
    }
    fn dwr(&self) -> Rc<DynamicWorld> {
        self.dw.upgrade().expect("TulipPlant: dynamic world dropped")
    }

    /// `colorGenes` getter.
    pub fn color_genes(&self) -> u16 {
        self.tulip.borrow().color_genes
    }
    /// `setColorGenes:`
    pub fn set_color_genes(&self, v: u16) {
        self.tulip.borrow_mut().color_genes = v;
    }

    /// `-initSubDerivedItems`: `maxAge = 99999`, tile fg = 125 (no NULL check), ONE draw
    /// (`randomRotation`, value discarded). The colour block is client-only (R7).
    pub fn init_sub_derived_items(&self, me: &ObjRef) {
        self.plant.borrow_mut().max_age = 99999.0f32;
        let world = self.world();
        let pos = me.pos();
        match tile_loaded(&world, pos.x, pos.y) {
            Some(t) => t.set_foreground_contents(FG_TULIP),
            None => ub_parity_null_tile(),
        }
        let _random_rotation =
            (glibc_rand::r01_tagged("plants::tulip::initSubDerivedItems::randomRotation") - 0.5f32) * 0.5f32;
    }

    /// `-initWithWorld:...atPosition:cache:maxAgeGene:growthRateGene:...adultPlant:`. The gene words take
    /// the RAW unclamped arguments.
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
    ) -> Option<Rc<TulipPlant>> {
        let me = Self::alloc(world, dw);
        let me_obj: ObjRef = me.clone();
        if !plant_init_at_position(&me_obj, world, dw, pos, cache, mag, grg, tdnf, sonf, adult) {
            return None;
        }
        {
            let mut t = me.tulip.borrow_mut();
            t.color_genes = mag;
            t.mate_color_genes = mag;
            t.mix_genes = grg;
        }
        let p = me_obj.pos();
        if adult {
            if let Some(tile) = tile_loaded(world, p.x, p.y) {
                if !plant_environment_ok(p, Some(&tile), world, -20.0f32) {
                    me_obj.remove_from_macro_block(&me_obj);
                    me_obj.dealloc(&me_obj);
                    return None;
                }
            }
        }
        let wm = world.world_width_macro() as f32;
        let xf = ((p.x as f32) / 32.0f32) / wm;
        let yf = ((p.y as f32) / 32.0f32) / wm;
        let sonf = me.plant.borrow().season_offset_noise_function.clone();
        let fraction = noise_xy(&sonf, xf as f64, yf as f64, 6) as f32;
        me.plant.borrow_mut().season_offset = f32_to_i32(32.0f32 * fraction);
        me.init_sub_derived_items(&me_obj);
        if adult {
            let r = glibc_rand::r01_tagged("plants::tulip::init::age");
            me.plant.borrow_mut().age = 900.0f32 + r * 900.0f32;
        }
        let r = glibc_rand::r01_tagged("plants::tulip::init::availableFood");
        me.tulip.borrow_mut().available_food = ((r * 900.0f32) as f64 * 0.5f64) as f32;
        Some(me)
    }

    /// `-initWithWorld:dynamicWorld:saveDict:cache:...`. No offline catch-up (R15).
    pub fn init_with_world_dynamic_world_save_dict_cache_tree_density_noise_function_season_offset_noise_function(
        world: &Rc<World>,
        dw: &Rc<DynamicWorld>,
        save_dict: &PlistDict,
        cache: CacheRef,
        tdnf: Option<Rc<NoiseFunction>>,
        sonf: Option<Rc<NoiseFunction>>,
    ) -> Option<Rc<TulipPlant>> {
        let me = Self::alloc(world, dw);
        let me_obj: ObjRef = me.clone();
        if !plant_init_save_dict(&me_obj, world, dw, save_dict, cache, tdnf, sonf) {
            return None;
        }
        {
            let mut t = me.tulip.borrow_mut();
            t.available_food = dict_float(save_dict, "availableFood");
            t.color_genes = dict_int(save_dict, "colorGenes") as u16;
            t.mate_color_genes = dict_int(save_dict, "mateColorGenes") as u16;
            t.mix_genes = dict_int(save_dict, "mixGenes") as u16;
        }
        me.init_sub_derived_items(&me_obj);
        Some(me)
    }

    /// `-initWithWorld:dynamicWorld:cache:netData:` (client replica).
    pub fn init_with_net_data(
        world: &Rc<World>,
        dw: &Rc<DynamicWorld>,
        cache: CacheRef,
        net_data: &[u8],
    ) -> Option<Rc<TulipPlant>> {
        let me = Self::alloc(world, dw);
        let me_obj: ObjRef = me.clone();
        if !plant_init_with_world_dynamic_world_cache_net_data(&me_obj, world, dw, cache, net_data) {
            return None;
        }
        let d = PlantCreationNetData::read_le(&padded_bytes(net_data, 0x28));
        {
            let mut t = me.tulip.borrow_mut();
            t.color_genes = d.display_gene_data_a;
            t.mix_genes = d.display_gene_data_b;
            t.mate_color_genes = d.display_gene_data_c;
        }
        me.init_sub_derived_items(&me_obj);
        Some(me)
    }

    /// `-mixGenesVariation`: two draws (top then bottom).
    pub fn mix_genes_variation(&self) -> u16 {
        let mix = self.tulip.borrow().mix_genes;
        let mut top = (mix & 0xf000) >> 12;
        let mut bot = (mix & 0x0f00) >> 8;
        let r = glibc_rand::r01_tagged("plants::tulip::mixGenesVariation::top");
        if r < 0.25f32 {
            if top != 0 {
                top -= 1;
            }
        } else if r > 0.85f32 && top < 8 {
            top += 1;
        }
        let r = glibc_rand::r01_tagged("plants::tulip::mixGenesVariation::bot");
        if r < 0.25f32 {
            if bot != 0 {
                bot -= 1;
            }
        } else if r > 0.85f32 && bot < 8 {
            bot += 1;
        }
        (top << 12) | (bot << 8)
    }

    /// `-colorGenesVariation`. Draw order: [A], [B] (only when the genes differ), mutation roll,
    /// [float roll, only when mutating], slot roll.
    pub fn color_genes_variation(&self) -> u16 {
        let (cg, mate) = {
            let t = self.tulip.borrow();
            (t.color_genes, t.mate_color_genes)
        };
        let (mut d_t, mut r_t, mut d_b, mut r_b) = ((cg >> 12) & 0xf, (cg >> 8) & 0xf, (cg >> 4) & 0xf, cg & 0xf);
        let (m_dt, m_rt, m_db, m_rb) = ((mate >> 12) & 0xf, (mate >> 8) & 0xf, (mate >> 4) & 0xf, mate & 0xf);
        let order = tulip_gene_order_for_pigment;
        if cg != mate {
            let r = glibc_rand::r01_tagged("plants::tulip::colorGenesVariation::A");
            if r > 0.5f32 {
                if r <= 0.75f32 {
                    if order(m_dt) < order(d_t) {
                        d_t = m_dt;
                    }
                } else if order(m_rt) < order(r_t) {
                    r_t = m_rt;
                }
            }
            let r = glibc_rand::r01_tagged("plants::tulip::colorGenesVariation::B");
            if r > 0.5f32 {
                if r <= 0.75f32 {
                    if order(m_db) < order(d_b) {
                        d_b = m_db;
                    }
                } else if order(m_rb) < order(r_b) {
                    r_b = m_rb;
                }
            }
        }
        // mutation (always)
        if glibc_rand::rand_tagged("plants::tulip::colorGenesVariation::mutate") % 100 == 0 {
            let mut new_color: u16 = 3;
            let r = glibc_rand::r01_tagged("plants::tulip::colorGenesVariation::color");
            if r < 0.5f32 {
                new_color = if r >= 0.25f32 {
                    4
                } else if r >= 0.125f32 {
                    5
                } else if r >= 0.0625f32 {
                    8
                } else if r >= 0.03125f32 {
                    2
                } else if r >= 0.3f32 * 0.03125f32 {
                    6
                } else {
                    7
                };
            }
            match glibc_rand::rand_tagged("plants::tulip::colorGenesVariation::slot") % 4 {
                0 => d_t = new_color,
                1 => r_t = new_color,
                2 => d_b = new_color,
                _ => r_b = new_color,
            }
        }
        r_b | (d_t << 12) | (r_t << 8) | (d_b << 4)
    }
}

impl PlantLike for TulipPlant {
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
        PLANT_TULIP
    }
    fn tile_is_kind_of_self(&self, tile: &Tile) -> bool {
        tile.foreground_contents == FG_TULIP
    }
    fn dropped_item_type(&self) -> i32 {
        ITEM_TULIP_BULB
    }
    /// `-canBreed`: `flowering ? true: (double)age > 450.0`.
    fn can_breed(&self) -> bool {
        let st = self.plant.borrow();
        if st.flowering {
            true
        } else {
            (st.age as f64) > 450.0f64
        }
    }
    fn number_of_occupied_tiles_above(&self) -> i32 {
        self.plant.borrow().flowering as i32
    }
    fn available_food(&self) -> f32 {
        self.tulip.borrow().available_food
    }
    fn set_available_food(&self, v: f32) {
        self.tulip.borrow_mut().available_food = v;
    }
    /// `-setFlowering:`: flag only.
    fn set_flowering(&self, _me: &ObjRef, v: bool) {
        let mut st = self.plant.borrow_mut();
        if st.flowering != v {
            st.flowering = v;
        }
    }
    /// `-plantCreationNetData`.
    fn plant_creation_net_data(&self, me: &ObjRef) -> PlantCreationNetData {
        let mut s = plant_plant_creation_net_data(me);
        let t = self.tulip.borrow();
        s.display_gene_data_a = t.color_genes;
        s.display_gene_data_b = t.mix_genes;
        s.display_gene_data_c = t.mate_color_genes;
        s
    }

    /// `-tileHarvested:removeBlockhead:correctToolMultiplier:`; `tile_pos` is unused.
    fn tile_harvested_remove_blockhead_correct_tool_multiplier(
        &self,
        me: &ObjRef,
        _tile_pos: IntPair,
        bh: Option<&ObjRef>,
        mult: i32,
    ) -> i32 {
        let dw = self.dwr();
        let pos = me.pos();
        let (flowering, age) = {
            let st = self.plant.borrow();
            (st.flowering, st.age)
        };
        let age_factor = if !flowering {
            if age >= 900.0f32 {
                1.0f32
            } else {
                (age / 900.0f32) * 0.8f32
            }
        } else {
            1.0f32
        };
        let mut v = (mult as f32) * age_factor;
        if v < 1.0f32 {
            v = 1.0f32;
        }
        let n = f32_to_i32(v);
        let mut bulb_count = 1;
        let mut seed_count = 0;
        if !flowering {
            if n > 1 {
                bulb_count = 2;
            }
        } else if n < 2 {
            seed_count =
                if glibc_rand::rand_tagged("plants::tulip::tileHarvested::seedRoll") > 0x3fff_ffff { 1 } else { 0 };
        } else {
            bulb_count = 2;
            seed_count = 1;
        }
        let (cg, mg) = {
            let t = self.tulip.borrow();
            (t.color_genes, t.mix_genes)
        };
        for _ in 0..bulb_count {
            create_free_block(&dw, pos, ITEM_TULIP_BULB, cg, mg, 1, bh);
        }
        for _ in 0..seed_count {
            let d_a = self.color_genes_variation();
            let d_b = self.mix_genes_variation();
            create_free_block(&dw, IntPair { x: pos.x, y: pos.y.wrapping_add(1) }, ITEM_TULIP_SEED, d_a, d_b, 1, bh);
        }
        self.remove_plant_without_creating_freeblocks(me);
        bulb_count + seed_count
    }
}

impl DynamicObject for TulipPlant {
    super::plant::plant_dynamic_object_common!();

    fn class_info(&self) -> &'static ClassInfo {
        &TULIP_PLANT_CLASS
    }
    fn object_type(&self) -> i32 {
        DYNAMIC_OBJECT_TYPE_TULIP_PLANT
    }

    /// `-getSaveDict`.
    fn get_save_dict(&self, me: &ObjRef) -> PlistDict {
        let mut d = plant_get_save_dict(me);
        let t = self.tulip.borrow();
        put_float(&mut d, "availableFood", t.available_food);
        put_int(&mut d, "colorGenes", t.color_genes as i32);
        put_int(&mut d, "mixGenes", t.mix_genes as i32);
        put_int(&mut d, "mateColorGenes", t.mate_color_genes as i32);
        d
    }

    fn set_needs_removed(&self, me: &ObjRef, v: bool) {
        plant_set_needs_removed(me, v);
    }

    fn remove_from_macro_block(&self, me: &ObjRef) {
        plant_remove_from_macro_block(me);
    }

    fn remote_update(&self, me: &ObjRef, net_data: &[u8]) {
        plant_remote_update(me, net_data);
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
        {
            let mut t = self.tulip.borrow_mut();
            if t.available_food >= 1800.0f32 {
                t.available_food -= 900.0f32;
            } else {
                t.available_food = 0.5f32 * dt + t.available_food;
            }
        }
        let run_check = {
            let mut t = self.tulip.borrow_mut();
            t.check_weather_count += 1;
            if t.check_weather_count > 299 {
                t.check_weather_count = 0;
                true
            } else {
                false
            }
        };
        if run_check {
            if plant_environment_ok(pos, tile.as_ref(), &world, -20.0f32) {
                self.tulip.borrow_mut().kill_count = 0;
            } else {
                let kc = {
                    let mut t = self.tulip.borrow_mut();
                    t.kill_count += 1;
                    t.kill_count
                };
                if kc > 1 {
                    self.tile_harvested_remove_blockhead_correct_tool_multiplier(me, pos, None, 1);
                    return;
                }
            }
        }
        // flowering logic (every frame)
        if (self.plant.borrow().age as f64) > 450.0f64 {
            let season_x = season_for_world_x(pos.x, world.world_time(), Some(&world));
            let offset = self.plant.borrow().season_offset;
            let this_season = season_index(season_x, offset);
            if matches!(this_season, 1 | 2 | 3) {
                let has = self.plant.borrow().has_flowered_this_season;
                if !has {
                    {
                        let mut st = self.plant.borrow_mut();
                        st.has_flowered_this_season = true;
                        st.flowering = true;
                    }
                    dw_changed(me);
                    dw.world_contents_changed_at_pos(IntPair { x: pos.x, y: pos.y.wrapping_add(1) });
                    mark_update_needed(me);
                    let mate = dw.find_breeding_plant_near_plant(me);
                    let mate_genes = match mate.as_ref().and_then(|m| downcast::<TulipPlant>(m)) {
                        Some(m) => m.color_genes(),
                        None => self.color_genes(),
                    };
                    self.tulip.borrow_mut().mate_color_genes = mate_genes;
                }
            } else {
                let flowering = self.plant.borrow().flowering;
                if flowering {
                    self.plant.borrow_mut().flowering = false;
                    dw_changed(me);
                    mark_update_needed(me);
                }
                let has = self.plant.borrow().has_flowered_this_season;
                if has {
                    self.plant.borrow_mut().has_flowered_this_season = false;
                    dw_changed(me);
                }
            }
        }
    }

    fn dealloc(&self, _me: &ObjRef) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mk(color: u16, mate: u16, mix: u16) -> Rc<TulipPlant> {
        // A free-standing instance without worlds is enough for the pure gene functions.
        handle::alloc_obj(|sw| TulipPlant {
            base: RefCell::new(DynamicObjectBase::alloc_zeroed_with(sw)),
            plant: RefCell::new(PlantState::default()),
            tulip: RefCell::new(TulipState {
                color_genes: color,
                mate_color_genes: mate,
                mix_genes: mix,
                ..Default::default()
            }),
            world: Weak::new(),
            dw: Weak::new(),
        })
    }

    ///  U11: pigment order table.
    #[test]
    fn order_table() {
        let v: Vec<i32> = (0..11).map(tulip_gene_order_for_pigment).collect();
        assert_eq!(v, vec![99, 3, 5, 0, 1, 2, 6, 7, 4, 99, 99]);
    }

    ///  U11 / section 8: identical genes consume only the mutation roll (1 draw, plus 2 when mutating).
    #[test]
    fn color_variation_draw_count() {
        let t = mk(0x3333, 0x3333, 0);
        glibc_rand::srand(5);
        let a = glibc_rand::rand();
        glibc_rand::srand(5);
        let out = t.color_genes_variation();
        let after = glibc_rand::rand();
        if a % 100 != 0 {
            // exactly one draw consumed: the next value is the stream's second value
            glibc_rand::srand(5);
            let _ = glibc_rand::rand();
            assert_eq!(after, glibc_rand::rand());
            assert_eq!(out, 0x3333);
        }
    }

    ///  U11: differing genes consume the two crossing draws first.
    #[test]
    fn color_variation_crossing() {
        let t = mk(0x1111, 0x4444, 0);
        for seed in 1..200u32 {
            glibc_rand::srand(seed);
            let r1 = glibc_rand::r01();
            let r2 = glibc_rand::r01();
            let m = glibc_rand::rand();
            glibc_rand::srand(seed);
            let out = t.color_genes_variation();
            if m % 100 != 0 {
                let (mut d_t, mut r_t, mut d_b, mut r_b) = (1u16, 1u16, 1u16, 1u16);
                if r1 > 0.5 {
                    if r1 <= 0.75 {
                        d_t = if tulip_gene_order_for_pigment(4) < tulip_gene_order_for_pigment(1) { 4 } else { d_t };
                    } else {
                        r_t = if tulip_gene_order_for_pigment(4) < tulip_gene_order_for_pigment(1) { 4 } else { r_t };
                    }
                }
                if r2 > 0.5 {
                    if r2 <= 0.75 {
                        d_b = if tulip_gene_order_for_pigment(4) < tulip_gene_order_for_pigment(1) { 4 } else { d_b };
                    } else {
                        r_b = if tulip_gene_order_for_pigment(4) < tulip_gene_order_for_pigment(1) { 4 } else { r_b };
                    }
                }
                assert_eq!(out, r_b | (d_t << 12) | (r_t << 8) | (d_b << 4), "seed {seed}");
            }
        }
    }

    ///  U11: mix genes stay within 0..8 nibbles and the low byte is zeroed.
    #[test]
    fn mix_variation_bounds() {
        let t = mk(0, 0, 0x0800);
        glibc_rand::srand(3);
        for _ in 0..500 {
            let v = t.mix_genes_variation();
            assert_eq!(v & 0x00ff, 0);
            assert!((v >> 12) <= 8 && ((v >> 8) & 0xf) <= 8);
        }
        // exactly two draws
        glibc_rand::srand(9);
        let _ = t.mix_genes_variation();
        let after = glibc_rand::rand();
        glibc_rand::srand(9);
        let _ = glibc_rand::rand();
        let _ = glibc_rand::rand();
        assert_eq!(after, glibc_rand::rand());
    }

    ///  U11: `can_breed` = flowering or age > 450.0.
    #[test]
    fn can_breed_rule() {
        let t = mk(0, 0, 0);
        assert!(!t.can_breed());
        t.plant.borrow_mut().age = 450.0;
        assert!(!t.can_breed());
        t.plant.borrow_mut().age = 450.5;
        assert!(t.can_breed());
        t.plant.borrow_mut().age = 0.0;
        t.plant.borrow_mut().flowering = true;
        assert!(t.can_breed());
    }
}
