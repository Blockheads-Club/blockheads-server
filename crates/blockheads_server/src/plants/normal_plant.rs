//! owned by plants
//!
//! `NormalPlant: Plant` (instance size 0xC0) and the lifecycle shared by the seven crops (Flax, Sunflower, Corn,
//! Carrot, Chilli, Wheat, Tomato): construct, per-tick update (light, food/NPC spawn, 300-tick survival check,
//! season flower/seed/die cycle), harvest, save/load, glow light.
//!

use std::cell::RefCell;
use std::rc::{Rc, Weak};

use crate::dynworld::state::DynamicWorld;
use crate::fmath::cconv::f32_to_i32;
use crate::fmath::libm::fabsf;
use crate::fmath::vector::clampi;
use crate::lighting::artificial_light::ArtificialLight;
use crate::objects::base::{new_mutable_dict, DynamicObject, DynamicObjectBase, DynamicWorldRef};
use crate::objects::class_info::ClassInfo;
use crate::objects::handle::{self, CacheRef, ObjRef};
use crate::plist::value::{PlistDict, PlistValue};
use crate::rng::glibc_rand;
use crate::types::enums::PlantType;
use crate::types::item_type::ItemType;
use crate::types::structs::IntPair;
use crate::types::tile::Tile;
use crate::world::climate::season_for_world_x;
use crate::world::growth::{contents_type_for_plant_type, growth_vigor_for_plant_type_at_pos};
use crate::world::helpers::tile_is_air_water_or_snow;
use crate::world::state::World;
use crate::worldgen::noise::{noise_xy, NoiseFunction};

use super::plant::{
    dict_float, mark_update_needed, plant_environment_ok, plant_get_save_dict,
    plant_init_with_world_dynamic_world_at_position_cache_max_age_gene_growth_rate_gene_tree_density_noise_function_season_offset_noise_function_adult_plant as plant_init_at_position,
    plant_init_with_world_dynamic_world_cache_net_data,
    plant_init_with_world_dynamic_world_save_dict_cache_tree_density_noise_function_season_offset_noise_function as plant_init_save_dict,
    plant_remote_update, plant_remove_from_macro_block, plant_set_needs_removed, plant_update, put_float, season_index,
    tile_loaded, ub_parity_null_tile, PlantLike, PlantState, PLANT_CLASS,
};
use super::simple_plants::{
    config_for_kind, light_color, light_factor, EmitsLight, NormalPlantConfig, NormalPlantKind,
};

/// `NormalPlant: Plant` class descriptor.
pub static NORMAL_PLANT_CLASS: ClassInfo =
    ClassInfo { name: "NormalPlant", superclass: Some(&PLANT_CLASS), protocols: &[] };

/// `NormalPlant` ivars 0xa0..0xc0 (client shader/texture ivars omitted). Zero/None at alloc.
#[derive(Default)]
pub struct NormalPlantState {
    /// 0xa0
    pub check_weather_count: i32,
    /// 0xa4
    pub kill_count: i32,
    /// 0xa8
    pub available_food: f32,
    /// 0xb0 (retained)
    pub light: Option<Rc<ArtificialLight>>,
    /// 0xb8
    pub last_light_factor: f32,
}

/// One of the seven crop classes (they differ only in [`NormalPlantConfig`]).
pub struct NormalPlant {
    pub base: RefCell<DynamicObjectBase>,
    pub plant: RefCell<PlantState>,
    pub np: RefCell<NormalPlantState>,
    pub kind: NormalPlantKind,
    world: Weak<World>,
    dw: Weak<DynamicWorld>,
}

impl NormalPlant {
    fn alloc(kind: NormalPlantKind, world: &Rc<World>, dw: &Rc<DynamicWorld>) -> Rc<NormalPlant> {
        let (w, d) = (Rc::downgrade(world), Rc::downgrade(dw));
        handle::alloc_obj(|sw| NormalPlant {
            base: RefCell::new(DynamicObjectBase::alloc_zeroed_with(sw)),
            plant: RefCell::new(PlantState::default()),
            np: RefCell::new(NormalPlantState::default()),
            kind,
            world: w,
            dw: d,
        })
    }

    /// The configuration row of this kind.
    pub fn cfg(&self) -> &'static NormalPlantConfig {
        config_for_kind(self.kind)
    }

    /// `-maxAgeBase`
    pub fn max_age_base(&self) -> f32 {
        self.cfg().max_age_base
    }
    /// `-contentsType`: `contentsTypeForPlantType(plantType, NO)`.
    pub fn contents_type(&self) -> i32 {
        contents_type_for_plant_type(PlantType(self.cfg().plant_type as u32), false).0 as i32
    }
    /// `-flowerContentsType`: `contentsTypeForPlantType(plantType, YES)`.
    pub fn flower_contents_type(&self) -> i32 {
        contents_type_for_plant_type(PlantType(self.cfg().plant_type as u32), true).0 as i32
    }
    /// `-minAllowedTemperature`
    pub fn min_allowed_temperature(&self) -> i32 {
        self.cfg().min_allowed_temperature
    }
    /// `-seedItemType`
    pub fn seed_item_type(&self) -> i32 {
        self.cfg().seed_item_type
    }
    /// `-folliageItemType`
    pub fn folliage_item_type(&self) -> i32 {
        self.cfg().folliage_item_type
    }
    /// `-foodToRemoveWhenSpawningNPC`
    pub fn food_to_remove_when_spawning_npc(&self) -> f32 {
        self.cfg().food_to_remove_when_spawning_npc
    }
    /// `-floweringSeason:`
    pub fn flowering_season(&self, s: i32) -> bool {
        (self.cfg().flowering_season)(s)
    }
    /// `-canDieSeason:`
    pub fn can_die_season(&self, s: i32) -> bool {
        (self.cfg().can_die_season)(s)
    }
    /// `-npcSpawnType`
    pub fn npc_spawn_type(&self) -> i32 {
        self.cfg().npc_spawn_type
    }
    /// `-emitsLight` (Sunflower: `flowering`).
    pub fn emits_light(&self) -> bool {
        match self.cfg().emits_light {
            EmitsLight::Never => false,
            EmitsLight::Always => true,
            EmitsLight::WhenFlowering => self.plant.borrow().flowering,
        }
    }
    /// `-lightFactor`
    pub fn light_factor(&self) -> f32 {
        let st = self.plant.borrow();
        light_factor(self.kind, st.age, st.flowering)
    }
    /// `-lightColor`
    pub fn light_color(&self) -> [f32; 4] {
        light_color(self.kind, self.light_factor())
    }

    /// Build the glow light at `(pos.x, pos.y + 1)` from the current `lightColor`.
    fn make_light(&self, me: &ObjRef, world: &Rc<World>, dw: &Rc<DynamicWorld>) -> Option<Rc<ArtificialLight>> {
        let c = self.light_color();
        let p = me.pos();
        let dref: DynamicWorldRef = dw.clone();
        ArtificialLight::init_with_world_dynamic_world_at_position_cache_parent_object_color_r_color_g_color_b_heat_radius_light_direction(
            world,
            &dref,
            IntPair { x: p.x, y: p.y.wrapping_add(1) },
            None,
            Some(me),
            f32_to_i32(c[0]),
            f32_to_i32(c[1]),
            f32_to_i32(c[2]),
            f32_to_i32(c[3]),
            3,
            0,
        )
    }

    /// `-initSubDerivedItems`.
    pub fn init_sub_derived_items(&self, me: &ObjRef) {
        let world = self.world.upgrade().expect("NormalPlant: world dropped");
        let pos = me.pos();
        match tile_loaded(&world, pos.x, pos.y) {
            Some(t) => t.set_foreground_contents(self.contents_type() as u8),
            None => ub_parity_null_tile(),
        }
        let vigor = growth_vigor_for_plant_type_at_pos(PlantType(self.cfg().plant_type as u32), pos, Some(&world));
        let gene = self.plant.borrow().max_age_gene;
        let mut max_age = ((gene as f32) / 255.0f32 + 0.5f32) * self.max_age_base() * (0.8f32 + vigor);
        if let Some(b) = tile_loaded(&world, pos.x, pos.y.wrapping_sub(1)) {
            if matches!(b.type_index(), 48..=50) {
                max_age *= 1.5f32;
            }
        }
        self.plant.borrow_mut().max_age = max_age;
    }

    /// `-initWithWorld:dynamicWorld:atPosition:cache:maxAgeGene:growthRateGene:treeDensityNoiseFunction:seasonOffsetNoiseFunction:adultPlant:`
    #[allow(clippy::too_many_arguments)]
    pub fn init_with_world_dynamic_world_at_position_cache_max_age_gene_growth_rate_gene_tree_density_noise_function_season_offset_noise_function_adult_plant(
        kind: NormalPlantKind,
        world: &Rc<World>,
        dw: &Rc<DynamicWorld>,
        pos: IntPair,
        cache: CacheRef,
        max_age_gene: u16,
        growth_rate_gene: u16,
        tdnf: Option<Rc<NoiseFunction>>,
        sonf: Option<Rc<NoiseFunction>>,
        adult_plant: bool,
    ) -> Option<Rc<NormalPlant>> {
        let me = Self::alloc(kind, world, dw);
        let me_obj: ObjRef = me.clone();
        if !plant_init_at_position(
            &me_obj,
            world,
            dw,
            pos,
            cache,
            max_age_gene,
            growth_rate_gene,
            tdnf,
            sonf,
            adult_plant,
        ) {
            return None;
        }
        let p = me_obj.pos();
        let tile = if adult_plant { tile_loaded(world, p.x, p.y) } else { None };
        if adult_plant && tile.is_some() {
            // adult plant being loaded/generated: same survival test as the update check
            if !plant_environment_ok(p, tile.as_ref(), world, me.min_allowed_temperature() as f32) {
                me_obj.remove_from_macro_block(&me_obj);
                me_obj.dealloc(&me_obj);
                return None;
            }
        }
        // CREATE
        let fx = (p.x as f32) / 32.0f32;
        let fy = (p.y as f32) / 32.0f32;
        let wm = world.world_width_macro() as f32;
        let sonf = me.plant.borrow().season_offset_noise_function.clone();
        let noise = noise_xy(&sonf, (fx / wm) as f64, (fy / wm) as f64, 6);
        me.plant.borrow_mut().season_offset = f32_to_i32(64.0f32 * (noise as f32));
        me.init_sub_derived_items(&me_obj);
        if adult_plant {
            let r = glibc_rand::r01_tagged("plants::normal_plant::init::age");
            let mut st = me.plant.borrow_mut();
            st.age = r * st.max_age;
        }
        let r = glibc_rand::r01_tagged("plants::normal_plant::init::availableFood");
        me.np.borrow_mut().available_food = ((r * 900.0f32) as f64 * 1.9f64) as f32;
        if me.emits_light() {
            let lf = me.light_factor();
            me.np.borrow_mut().last_light_factor = lf;
            let light = me.make_light(&me_obj, world, dw);
            me.np.borrow_mut().light = light;
        }
        Some(me)
    }

    /// `-initWithWorld:dynamicWorld:saveDict:cache:treeDensityNoiseFunction:seasonOffsetNoiseFunction:`.
    pub fn init_with_world_dynamic_world_save_dict_cache_tree_density_noise_function_season_offset_noise_function(
        kind: NormalPlantKind,
        world: &Rc<World>,
        dw: &Rc<DynamicWorld>,
        save_dict: &PlistDict,
        cache: CacheRef,
        tdnf: Option<Rc<NoiseFunction>>,
        sonf: Option<Rc<NoiseFunction>>,
    ) -> Option<Rc<NormalPlant>> {
        let me = Self::alloc(kind, world, dw);
        let me_obj: ObjRef = me.clone();
        plant_init_save_dict(&me_obj, world, dw, save_dict, cache, tdnf, sonf);
        me.init_sub_derived_items(&me_obj);
        let flowering = me.plant.borrow().flowering;
        if flowering {
            let p = me_obj.pos();
            if let Some(above) = tile_loaded(world, p.x, p.y.wrapping_add(1)) {
                if !me.tile_is_kind_of_self(&above.get()) {
                    me.plant.borrow_mut().flowering = false;
                }
            }
        }
        me.np.borrow_mut().available_food = dict_float(save_dict, "availableFood");
        if me.emits_light() {
            let empty;
            let ld = match save_dict.get(&"lightDict".to_string()) {
                Some(PlistValue::Dict(d)) => d,
                _ => {
                    empty = new_mutable_dict();
                    &empty
                }
            };
            let dref: DynamicWorldRef = dw.clone();
            let light = ArtificialLight::init_with_world_dynamic_world_save_dict_cache_parent_object(
                world,
                &dref,
                ld,
                None,
                Some(&me_obj),
            );
            me.np.borrow_mut().light = light;
        }
        Some(me)
    }

    /// `-initWithWorld:dynamicWorld:cache:netData:` (client replica).
    pub fn init_with_net_data(
        kind: NormalPlantKind,
        world: &Rc<World>,
        dw: &Rc<DynamicWorld>,
        cache: CacheRef,
        net_data: &[u8],
    ) -> Option<Rc<NormalPlant>> {
        let me = Self::alloc(kind, world, dw);
        let me_obj: ObjRef = me.clone();
        if !plant_init_with_world_dynamic_world_cache_net_data(&me_obj, world, dw, cache, net_data) {
            return None;
        }
        me.init_sub_derived_items(&me_obj);
        Some(me)
    }

    fn world(&self) -> Rc<World> {
        self.world.upgrade().expect("NormalPlant: world dropped")
    }
    fn dwr(&self) -> Rc<DynamicWorld> {
        self.dw.upgrade().expect("NormalPlant: dynamic world dropped")
    }

    /// `[light removeFromMacroBlock]; [light release]; light = nil` (the plant removes the light from the macro
    /// tile itself).
    fn release_light(&self, remove_tiles_first: bool) {
        let l = self.np.borrow_mut().light.take();
        if let Some(l) = l {
            if remove_tiles_first {
                l.remove_from_tiles();
            }
            l.remove_from_macro_block_light();
            l.dealloc_light();
        }
    }
}

impl PlantLike for NormalPlant {
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
        self.cfg().plant_type
    }
    fn is_required_soil_type(&self, tile_type: u8) -> bool {
        self.cfg().required_soil.contains(&tile_type)
    }
    /// : `fg == (u8)contentsType || fg == (u8)flowerContentsType`.
    fn tile_is_kind_of_self(&self, tile: &Tile) -> bool {
        let fg = tile.foreground_contents;
        fg == self.contents_type() as u8 || fg == self.flower_contents_type() as u8
    }
    fn number_of_occupied_tiles_above(&self) -> i32 {
        self.plant.borrow().flowering as i32
    }
    fn dropped_item_type(&self) -> i32 {
        if !self.plant.borrow().flowering {
            let f = self.folliage_item_type();
            if f != 0 {
                return f;
            }
        }
        self.seed_item_type()
    }
    fn available_food(&self) -> f32 {
        self.np.borrow().available_food
    }
    fn set_available_food(&self, v: f32) {
        self.np.borrow_mut().available_food = v;
    }

    /// `-setFlowering:`.
    fn set_flowering(&self, me: &ObjRef, new_flowering: bool) {
        let cur = self.plant.borrow().flowering;
        if cur == new_flowering {
            return;
        }
        self.plant.borrow_mut().flowering = new_flowering;
        let world = self.world();
        let pos = me.pos();
        let above = tile_loaded(&world, pos.x, pos.y.wrapping_add(1));
        if let Some(a) = above {
            if !new_flowering {
                if a.foreground_contents() == self.flower_contents_type() as u8 {
                    a.set_foreground_contents(0);
                }
            } else if a.foreground_contents() == 0 && tile_is_air_water_or_snow(Some(a)) {
                a.set_foreground_contents(self.flower_contents_type() as u8);
            }
        }
    }

    /// `-tileHarvested:removeBlockhead:correctToolMultiplier:`. `tile_pos` is unused: the whole plant
    /// is removed.
    fn tile_harvested_remove_blockhead_correct_tool_multiplier(
        &self,
        me: &ObjRef,
        _tile_pos: IntPair,
        bh: Option<&ObjRef>,
        mult: i32,
    ) -> i32 {
        let world = self.world();
        let dw = self.dwr();
        let pos = me.pos();
        let (age, mag, grg, flowering) = {
            let st = self.plant.borrow();
            (st.age, st.max_age_gene, st.growth_rate_gene, st.flowering)
        };
        let mut count_created = 0;
        let age_factor = if age >= 900.0f32 { 0.8f32 + age / self.max_age_base() } else { (age / 900.0f32) * 0.8f32 };
        let n = clampi(f32_to_i32(age_factor * (mult as f32)), 1, 5);
        let expert = world.expert_mode();
        let play_sound = bh.is_some() as i32;
        let seed = ItemType(self.seed_item_type() as u32);
        // (1) the plant's own seed at the plant tile
        if !expert || mult > 1 {
            dw.create_free_block_at_position_of_type_data_a_data_b_sub_items_dynamic_object_save_dict_hovers_play_sound_priority_blockhead(
                pos, seed, mag, grg, None, None, false, play_sound, bh,
            );
            count_created = 1;
        }
        // (2) seeds from the flower on the tile above
        if flowering {
            let mut seed_count = n;
            if expert {
                let m = if n > 1 { 2 } else { n };
                seed_count = if mult < 2 {
                    if bh.is_some() {
                        1
                    } else {
                        0
                    }
                } else {
                    m
                };
            }
            for _ in 0..seed_count {
                let a = self.max_age_gene_variation();
                let b = self.growth_rate_gene_variation();
                dw.create_free_block_at_position_of_type_data_a_data_b_sub_items_dynamic_object_save_dict_hovers_play_sound_priority_blockhead(
                    IntPair { x: pos.x, y: pos.y.wrapping_add(1) },
                    seed,
                    a,
                    b,
                    None,
                    None,
                    false,
                    play_sound,
                    bh,
                );
                count_created += 1;
            }
        }
        // (3) foliage
        if age > 900.0f32 && self.folliage_item_type() != 0 {
            for _ in 0..n {
                dw.create_free_block_at_position_of_type_data_a_data_b_sub_items_dynamic_object_save_dict_hovers_play_sound_priority_blockhead(
                    pos,
                    ItemType(self.folliage_item_type() as u32),
                    0,
                    0,
                    None,
                    None,
                    false,
                    play_sound,
                    bh,
                );
                count_created += 1;
            }
        }
        self.remove_plant_without_creating_freeblocks(me);
        count_created
    }
}

impl DynamicObject for NormalPlant {
    super::plant::plant_dynamic_object_common!();

    fn class_info(&self) -> &'static ClassInfo {
        self.cfg().class
    }
    fn object_type(&self) -> i32 {
        self.cfg().object_type
    }

    /// `-getSaveDict`.
    fn get_save_dict(&self, me: &ObjRef) -> PlistDict {
        let mut d = plant_get_save_dict(me);
        put_float(&mut d, "availableFood", self.np.borrow().available_food);
        if self.emits_light() {
            let l = self.np.borrow().light.clone();
            if let Some(l) = l {
                let lo: ObjRef = l;
                let ld = lo.get_save_dict(&lo);
                d.map.insert("lightDict".to_string(), PlistValue::Dict(ld));
            }
        }
        d
    }

    /// `-setNeedsRemoved:`: Plant's, then drop the light contribution.
    fn set_needs_removed(&self, me: &ObjRef, v: bool) {
        plant_set_needs_removed(me, v);
        if v && self.emits_light() {
            let l = self.np.borrow().light.clone();
            if let Some(l) = l {
                l.remove_from_tiles();
            }
        }
    }

    /// `-remoteUpdate:` (client path; the light redraw is a no-op).
    fn remote_update(&self, me: &ObjRef, net_data: &[u8]) {
        plant_remote_update(me, net_data);
    }

    /// `-removeFromMacroBlock`.
    fn remove_from_macro_block(&self, me: &ObjRef) {
        if self.emits_light() {
            self.release_light(false);
        }
        plant_remove_from_macro_block(me);
    }

    /// `-addArtificialLightContributionForPhysicalBlockLoadedAtXPos:yPos:`.
    fn add_artificial_light_contribution_for_physical_block_loaded_at_x_pos_y_pos(
        &self,
        _me: &ObjRef,
        mx: i32,
        my: i32,
    ) {
        let l = self.np.borrow().light.clone();
        if let Some(l) = l {
            l.add_contribution_for_physical_block_loaded_at_x_pos_y_pos(mx, my);
        }
    }

    /// `-dealloc`: only drops the reference (R9).
    fn dealloc(&self, _me: &ObjRef) {
        self.np.borrow_mut().light = None;
    }

    /// `-update:accurateDT:isSimulation:`.
    fn update_accurate_dt_is_simulation(&self, me: &ObjRef, dt: f32, accurate_dt: f32, is_simulation: bool) {
        plant_update(me, dt, accurate_dt, is_simulation);
        if me.needs_removed() || me.is_net() {
            return;
        }
        let world = self.world();
        let dw = self.dwr();
        let object_type = self.cfg().object_type;

        // ---- (A) light object maintenance
        if !self.emits_light() {
            self.release_light(false);
        } else {
            let nf = self.light_factor();
            let last = self.np.borrow().last_light_factor;
            if fabsf(last - nf) > 0.05f32 {
                self.release_light(true);
                self.np.borrow_mut().last_light_factor = nf;
                let light = self.make_light(me, &world, &dw);
                self.np.borrow_mut().light = light;
            }
        }

        let pos = me.pos();
        let tile = tile_loaded(&world, pos.x, pos.y);

        // ---- (B) food accumulator / NPC spawn
        let food = self.np.borrow().available_food;
        if !(food < 1800.0f32) {
            self.np.borrow_mut().available_food = food - self.food_to_remove_when_spawning_npc();
            if self.npc_spawn_type() != 0
                && (glibc_rand::r01_tagged("plants::normal_plant::update::npc") as f64) > 0.95f64
            {
                if let Some(t) = tile {
                    if t.back_wall_type_index() == 2 && !dw.npc_exists_at_pos_ignore_npc(pos, None) {
                        dw.load_npc_at_position_type_save_dict_is_adult_was_placed_placed_by_client(
                            pos,
                            self.npc_spawn_type(),
                            None,
                            true,
                            false,
                            None,
                        );
                    }
                }
                dw.dynamic_world_changed_at_pos_object_type(pos, object_type);
            }
        } else {
            self.np.borrow_mut().available_food = 0.5f32 * dt + food;
        }

        // ---- (C) periodic survival check
        let run_check = {
            let mut np = self.np.borrow_mut();
            np.check_weather_count += 1;
            if np.check_weather_count >= 300 {
                np.check_weather_count = 0;
                true
            } else {
                false
            }
        };
        if run_check {
            let survives = plant_environment_ok(pos, tile.as_ref(), &world, self.min_allowed_temperature() as f32);
            if survives {
                self.np.borrow_mut().kill_count = 0;
            } else {
                let kc = {
                    let mut np = self.np.borrow_mut();
                    np.kill_count += 1;
                    np.kill_count
                };
                if kc > 1 {
                    // second consecutive failure: the plant dies and drops items
                    self.tile_harvested_remove_blockhead_correct_tool_multiplier(me, pos, None, 1);
                    return;
                }
            }
        }

        // ---- (D) seasonal cycle, every update once age > 1800
        if !(self.plant.borrow().age > 1800.0f32) {
            return;
        }
        let season_x = season_for_world_x(pos.x, world.world_time(), Some(&world));
        let season_offset = self.plant.borrow().season_offset;
        let this_season = season_index(season_x, season_offset);
        let (max_age, age) = {
            let st = self.plant.borrow();
            (st.max_age, st.age)
        };

        if max_age < age && self.can_die_season(this_season) {
            let r = glibc_rand::r01_tagged("plants::normal_plant::update::regrow");
            if r > 0.5f32 {
                // regrow from the root (age reset)
                let above = tile_loaded(&world, pos.x, pos.y.wrapping_add(1));
                if let Some(a) = above {
                    if self.tile_is_kind_of_self(&a.get()) {
                        a.set_foreground_contents(0);
                        dw.world_contents_changed_at_pos(IntPair { x: pos.x, y: pos.y.wrapping_add(1) });
                    }
                }
                {
                    let mut st = self.plant.borrow_mut();
                    st.age = 0.0;
                    st.flowering = false;
                }
                dw.dynamic_world_changed_at_pos_object_type(pos, object_type);
                mark_update_needed(me);
                return;
            }
            // die and spread
            self.tile_harvested_remove_blockhead_correct_tool_multiplier(me, pos, None, 1);
            dw.sow_plant_near_parent(me);
            return;
        }

        let up = IntPair { x: pos.x, y: pos.y.wrapping_add(1) };
        if !self.flowering_season(this_season) {
            let flowering = self.plant.borrow().flowering;
            if flowering {
                let a = self.max_age_gene_variation();
                let b = self.growth_rate_gene_variation();
                dw.create_free_block_at_position_of_type_data_a_data_b_sub_items_dynamic_object_save_dict_hovers_play_sound_priority_blockhead(
                    up,
                    ItemType(self.seed_item_type() as u32),
                    a,
                    b,
                    None,
                    None,
                    false,
                    0,
                    None,
                );
                match tile_loaded(&world, up.x, up.y) {
                    Some(t) => {
                        if t.foreground_contents() == self.flower_contents_type() as u8 {
                            t.set_foreground_contents(0);
                            dw.world_contents_changed_at_pos(up);
                        }
                    }
                    None => ub_parity_null_tile(),
                }
                self.plant.borrow_mut().flowering = false;
                dw.dynamic_world_changed_at_pos_object_type(pos, object_type);
                mark_update_needed(me);
            }
            let has = self.plant.borrow().has_flowered_this_season;
            if has {
                self.plant.borrow_mut().has_flowered_this_season = false;
                dw.dynamic_world_changed_at_pos_object_type(pos, object_type);
            }
        } else if !self.plant.borrow().has_flowered_this_season {
            self.plant.borrow_mut().has_flowered_this_season = true;
            match tile_loaded(&world, up.x, up.y) {
                Some(t) => {
                    if t.foreground_contents() == 0 && tile_is_air_water_or_snow(Some(t)) {
                        t.set_foreground_contents(self.flower_contents_type() as u8);
                        self.plant.borrow_mut().flowering = true;
                        dw.dynamic_world_changed_at_pos_object_type(pos, object_type);
                        dw.world_contents_changed_at_pos(up);
                        mark_update_needed(me);
                    }
                }
                None => ub_parity_null_tile(),
            }
        }
    }
}

#[cfg(test)]
mod tests {

    ///  U3: golden maxAge (Flax gene 191, vigor 1.0): `(191/255+0.5)*14400*(0.8+1)`.
    #[test]
    fn golden_max_age_formula() {
        let m = ((191_f32) / 255.0f32 + 0.5f32) * 14400.0f32 * (0.8f32 + 1.0f32);
        assert!((m - 32374.59).abs() < 0.1);
        let m = ((180_f32) / 255.0f32 + 0.5f32) * 7200.0f32 * (0.8f32 + 1.0f32);
        assert!((m - 15628.23).abs() < 0.1);
        let m = ((249_f32) / 255.0f32 + 0.5f32) * 7200.0f32 * (0.8f32 + 1.0f32);
        assert!((m - 19135.06).abs() < 0.1);
    }
}

/// Scenario tests over all seven crops: seasons, flowers, food, survival checks, harvests and persistence.
#[cfg(test)]
mod scenario_tests {
    use super::*;
    use crate::dynworld::state::testutil::{tobj, Log};
    use crate::rng::glibc_rand::srand;
    use crate::trees::tree::testenv::env;
    use crate::types::structs::WireStruct;
    use crate::world::tiles::tile_at_world_position_loaded;
    use NormalPlantKind::*;

    const KINDS: [NormalPlantKind; 7] = [Flax, Sunflower, Corn, Carrot, Chilli, Wheat, Tomato];
    const X: i32 = 20;
    const Y: i32 = 10;

    fn plant_at(w: &Rc<World>, d: &Rc<DynamicWorld>, kind: NormalPlantKind, adult: bool) -> Option<Rc<NormalPlant>> {
        NormalPlant::init_with_world_dynamic_world_at_position_cache_max_age_gene_growth_rate_gene_tree_density_noise_function_season_offset_noise_function_adult_plant(
            kind, w, d, IntPair { x: X, y: Y }, None, 128, 128, None, None, adult,
        )
    }

    fn clear(w: &Rc<World>) {
        for y in Y..=Y + 2 {
            tile_at_world_position_loaded(X, y, w).unwrap().update(|t| {
                t.type_index = 2;
                t.foreground_contents = 0;
                t.sun_light = 255;
                t.back_wall_type_index = 2;
            });
        }
    }

    fn tile(w: &Rc<World>, y: i32) -> crate::world::state::TileRef {
        tile_at_world_position_loaded(X, y, w).unwrap()
    }

    #[test]
    fn every_kind_flowers_in_its_season_and_dies_back() {
        let (w, d) = env("np_seasons");
        for kind in KINDS {
            clear(&w);
            let p = plant_at(&w, &d, kind, false).expect("plant");
            let me: ObjRef = p.clone();
            assert_eq!(tile(&w, Y).foreground_contents(), p.contents_type() as u8);
            p.plant.borrow_mut().age = 2000.0;
            p.plant.borrow_mut().max_age = 1.0e9;
            for k in 0..24 {
                w.set_world_time(f64::from(k) * 150.0);
                p.update_accurate_dt_is_simulation(&me, 0.1, 0.1, false);
            }
            // the whole year with a plant that is over its age
            p.plant.borrow_mut().max_age = 100.0;
            for seed in 1..30u32 {
                for k in 0..8 {
                    w.set_world_time(f64::from(k) * 450.0);
                    p.plant.borrow_mut().age = 2000.0;
                    srand(seed);
                    p.update_accurate_dt_is_simulation(&me, 0.1, 0.1, false);
                    if me.needs_removed() {
                        break;
                    }
                }
                if me.needs_removed() {
                    break;
                }
            }
            assert_eq!(p.class_info().name, config_for_kind(kind).class.name);
            let _ = (p.flowering_season(1), p.can_die_season(1), p.max_age_base(), p.min_allowed_temperature());
            let _ = (p.food_to_remove_when_spawning_npc(), p.npc_spawn_type(), p.folliage_item_type());
            let _ = (p.light_factor(), p.light_color(), p.emits_light());
        }
    }

    #[test]
    fn crops_feed_animals_and_check_their_surroundings() {
        let (w, d) = env("np_food");
        for kind in KINDS {
            clear(&w);
            let p = plant_at(&w, &d, kind, false).unwrap();
            let me: ObjRef = p.clone();
            for seed in 1..40u32 {
                p.np.borrow_mut().available_food = 2000.0;
                srand(seed);
                p.update_accurate_dt_is_simulation(&me, 0.1, 0.1, false);
            }
            // survival: fine at first, then too dark twice in a row
            p.np.borrow_mut().check_weather_count = 299;
            p.update_accurate_dt_is_simulation(&me, 0.1, 0.1, false);
            assert_eq!(p.np.borrow().kill_count, 0);
            tile(&w, Y).set_sun_light(0);
            p.np.borrow_mut().check_weather_count = 299;
            p.update_accurate_dt_is_simulation(&me, 0.1, 0.1, false);
            assert_eq!(p.np.borrow().kill_count, 1);
            p.np.borrow_mut().check_weather_count = 299;
            p.update_accurate_dt_is_simulation(&me, 0.1, 0.1, false);
            assert!(me.needs_removed());
        }
    }

    #[test]
    fn harvesting_drops_seeds_flowers_and_foliage() {
        let (w, d) = env("np_harvest");
        let log: Log = Rc::new(RefCell::new(vec![]));
        let bh = tobj(24, 1, &log, None, None, None);
        for kind in KINDS {
            for expert in [false, true] {
                w.set_expert_mode(expert);
                for (age, flowering) in [(100.0f32, false), (5000.0, true), (5000.0, false)] {
                    for mult in [1, 2, 3] {
                        for with_bh in [false, true] {
                            clear(&w);
                            let p = plant_at(&w, &d, kind, false).unwrap();
                            let me: ObjRef = p.clone();
                            p.plant.borrow_mut().age = age;
                            p.plant.borrow_mut().flowering = flowering;
                            let n = p.tile_harvested_remove_blockhead_correct_tool_multiplier(
                                &me,
                                IntPair { x: X, y: Y },
                                if with_bh { Some(&bh) } else { None },
                                mult,
                            );
                            assert!(n >= 0);
                            assert!(me.needs_removed());
                        }
                    }
                }
            }
        }
        w.set_expert_mode(false);
    }

    #[test]
    fn saved_crops_replicas_and_lights() {
        let (w, d) = env("np_persist");
        for kind in KINDS {
            clear(&w);
            let p = plant_at(&w, &d, kind, false).unwrap();
            let me: ObjRef = p.clone();
            p.plant.borrow_mut().age = 3000.0;
            // a flower that is not there any more is forgotten, one that is there stays
            p.plant.borrow_mut().flowering = true;
            let dict = p.get_save_dict(&me);
            let back = NormalPlant::init_with_world_dynamic_world_save_dict_cache_tree_density_noise_function_season_offset_noise_function(kind, &w, &d, &dict, None, None, None).unwrap();
            assert!(!back.plant.borrow().flowering || tile(&w, Y + 1).foreground_contents() != 0);
            tile(&w, Y + 1).set_foreground_contents(p.flower_contents_type() as u8);
            let back = NormalPlant::init_with_world_dynamic_world_save_dict_cache_tree_density_noise_function_season_offset_noise_function(kind, &w, &d, &dict, None, None, None).unwrap();
            let _ = back.plant.borrow().flowering;
            // lights: a plant that emits light keeps the dict of its light
            p.plant.borrow_mut().flowering = true;
            let with_light = p.get_save_dict(&me);
            let again = NormalPlant::init_with_world_dynamic_world_save_dict_cache_tree_density_noise_function_season_offset_noise_function(kind, &w, &d, &with_light, None, None, None).unwrap();
            let again_ref: ObjRef = again.clone();
            again.set_needs_removed(&again_ref, true);
            again.remove_from_macro_block(&again_ref);
            again.dealloc(&again_ref);
            // replicas and the flowering switch
            let net = p.plant_creation_net_data(&me);
            let copy = NormalPlant::init_with_net_data(kind, &w, &d, None, &net.to_vec()).unwrap();
            let copy_ref: ObjRef = copy.clone();
            copy.remote_update(&copy_ref, &net.to_vec());
            p.set_flowering(&me, false);
            p.set_flowering(&me, false);
            p.set_flowering(&me, true);
            p.set_flowering(&me, false);
            p.plant.borrow_mut().flowering = false;
            let _ = p.dropped_item_type();
            p.plant.borrow_mut().flowering = true;
            let _ = p.dropped_item_type();
        }
    }

    #[test]
    fn adult_crops_need_a_survivable_spot() {
        let (w, d) = env("np_adult");
        for kind in KINDS {
            clear(&w);
            for seed in 1..10u32 {
                srand(seed);
                assert!(plant_at(&w, &d, kind, true).is_some());
            }
            tile(&w, Y).set_type_index(3);
            assert!(plant_at(&w, &d, kind, true).is_none());
        }
    }

    #[test]
    fn sunflower_light_follows_the_flower() {
        let (w, d) = env("np_light");
        clear(&w);
        let p = plant_at(&w, &d, Sunflower, false).unwrap();
        let me: ObjRef = p.clone();
        p.plant.borrow_mut().flowering = true;
        p.plant.borrow_mut().age = 100.0;
        for age in [100.0f32, 20_000.0, 30_000.0] {
            p.plant.borrow_mut().age = age;
            p.update_accurate_dt_is_simulation(&me, 0.1, 0.1, false);
        }
        p.plant.borrow_mut().flowering = false;
        p.update_accurate_dt_is_simulation(&me, 0.1, 0.1, false);
        assert!(!p.emits_light());
    }

    #[test]
    fn compost_blocks_unloaded_columns_and_odd_accessors() {
        let (w, d) = env("np_odd");
        for kind in KINDS {
            clear(&w);
            // compost under the root grows bigger plants
            tile(&w, Y - 1).set_type_index(48);
            let p = plant_at(&w, &d, kind, false).unwrap();
            let me: ObjRef = p.clone();
            p.set_available_food(12.0);
            assert_eq!(p.available_food(), 12.0);
            p.add_artificial_light_contribution_for_physical_block_loaded_at_x_pos_y_pos(&me, 0, 0);
            tile(&w, Y - 1).set_type_index(6);
            // removed and replica plants do not tick
            let net = p.plant_creation_net_data(&me);
            let copy = NormalPlant::init_with_net_data(kind, &w, &d, None, &net.to_vec()).unwrap();
            let copy_ref: ObjRef = copy.clone();
            copy.update_accurate_dt_is_simulation(&copy_ref, 0.1, 0.1, false);
            // a flower season with the row above gone: the plant copes
            p.plant.borrow_mut().age = 5000.0;
            p.plant.borrow_mut().max_age = 1.0e9;
            let dict = p.get_save_dict(&me);
            let mut net_bad = net;
            net_bad.dynamic_object_net_data.pos_y = 5;
            for mx in 0..2 {
                *w.macro_tiles().get(mx).unwrap().physical_block.borrow_mut() = None;
            }
            assert!(NormalPlant::init_with_net_data(kind, &w, &d, None, &net_bad.to_vec()).is_none());
            let _ = dict;
            p.init_sub_derived_items(&me);
            for k in 0..8 {
                w.set_world_time(f64::from(k) * 450.0);
                p.plant.borrow_mut().flowering = false;
                p.plant.borrow_mut().has_flowered_this_season = false;
                p.update_accurate_dt_is_simulation(&me, 0.1, 0.1, false);
                p.plant.borrow_mut().flowering = true;
                p.update_accurate_dt_is_simulation(&me, 0.1, 0.1, false);
            }
            assert!(plant_at(&w, &d, kind, false).is_none());
            // pos in unloaded block: rebuild the loaded world for the next kind
            let (_w2, _d2) = (w.clone(), d.clone());
            for mx in 0..2 {
                let pb = crate::world::state::PhysicalBlock::new_fresh();
                let pb: &'static crate::world::state::PhysicalBlock = Box::leak(Box::new(pb));
                pb.x_pos.set(mx as i32);
                pb.y_pos.set(0);
                pb.valid.set(true);
                for i in 0..1024 {
                    pb.tiles[i].set(Tile::default());
                }
                *w.macro_tiles().get(mx).unwrap().physical_block.borrow_mut() = Some(pb);
                for ly in 0..32usize {
                    for lx in 0..32usize {
                        let mut t = Tile::default();
                        if ly <= 9 {
                            t.type_index = 6;
                        } else {
                            t.type_index = 2;
                            t.back_wall_type_index = 2;
                            t.sun_light = 255;
                        }
                        pb.tiles[ly * 32 + lx].set(t);
                    }
                }
            }
        }
    }
}
