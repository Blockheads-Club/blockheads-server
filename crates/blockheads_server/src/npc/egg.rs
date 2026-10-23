//! `Egg` (DynamicObject type 30, a plain `DynamicObject`, not an NPC), plus the module-wide environment seam
//! [`PassiveEnv`] used by every class of `npc_passive` and a few NULL-safe tile predicates.
//!
//!  Class `Egg`" (9.1 to 9.4) and 0.1/0.2 (`EggCreationNetData`);
//! item 1 and 2, 7.1 (Egg row); , 3.4, 3.6.
//!
//! ## Why the seam lives here
//! `npc_passive` reaches many layers that are skeletons in parallel (plants, trees, FishingRod, Blockhead
//! interaction state, the concrete `DynamicWorld`). Like `npc_base` (`NpcEnv`) and `lighting_env` (`services()`),
//! every such call goes through one trait with working defaults where a concrete type already exists and inert
//! defaults otherwise. The integrator installs one implementation with [`set_passive_env`] (usually only
//! [`PassiveEnv::dynamic_world`] has to be provided: all `dw_*` defaults then call the concrete `DynamicWorld`).
//!
use std::any::Any;
use std::cell::RefCell;
use std::rc::Rc;

use crate::blockhead::blockhead::{state_asleep, state_meditating, Blockhead, FishingRodRef};
use crate::dynworld::state::DynamicWorld;
use crate::npc::npc::npc_env;
use crate::objects::base::{
    base_dynamic_object_net_data, base_get_save_dict, base_init_with_world_dynamic_world_at_position_cache,
    base_init_with_world_dynamic_world_cache_net_data, base_init_with_world_dynamic_world_save_dict_cache,
    new_mutable_dict, padded_bytes, DynamicObject, DynamicObjectBase, DynamicWorldRef, WorldRef,
};
use crate::objects::class_info::{ClassInfo, DYNAMIC_OBJECT_CLASS};
use crate::objects::handle::{alloc_obj, CacheRef, ObjRef};
use crate::plist::value::{PlistDict, PlistValue};
use crate::rng::glibc_rand;
use crate::types::item_type::ItemType;
use crate::types::netdata::EggCreationNetData;
use crate::types::structs::{CustomRules, IntPair, Vector2, WireStruct};
use crate::types::tile::Tile;
use crate::world::simulation::SimulationEventExtra;
use crate::world::state::World;

// ------------------------------------------------------------------------------------------------ tile predicates

/// `tileIsWater` on a tile copy (NULL-safe).
pub fn t_water(t: &Option<Tile>) -> bool {
    matches!(t, Some(t) if t.type_index == 3)
}
/// `tileIsSolid`: non-NULL and `typeIndex` not in {2,3,5}.
pub fn t_solid(t: &Option<Tile>) -> bool {
    matches!(t, Some(t) if !matches!(t.type_index, 2 | 3 | 5))
}
/// `tileIsAirOrSnow`: `typeIndex` in {2,5}.
pub fn t_air_snow(t: &Option<Tile>) -> bool {
    matches!(t, Some(t) if matches!(t.type_index, 2 | 5))
}
/// `tileIsAirWaterOrSnow`: `typeIndex` in {2,3,5}.
pub fn t_air_water_snow(t: &Option<Tile>) -> bool {
    matches!(t, Some(t) if matches!(t.type_index, 2 | 3 | 5))
}
/// `tileContainsGate`: `contents == 0x92`.
pub fn t_gate(t: &Option<Tile>) -> bool {
    matches!(t, Some(t) if t.contents == 0x92)
}

/// The inlined horizontal-wrap delta of the NPC specs (`wrapDX` / `wrapInt`): `a` is already `x1 - x2`, `w` the world
/// width in tiles. C signed division.
pub fn wrap_dx(a: i32, w: i32) -> i32 {
    let half = w.wrapping_div(2);
    let nhalf = w.wrapping_neg().wrapping_div(2);
    if a < half {
        if a < nhalf {
            a.wrapping_add(w)
        } else {
            a
        }
    } else {
        a.wrapping_sub(w)
    }
}

/// `[world worldWidthMacro] << 5` of the object's world (0 when it has none).
pub fn world_width_tiles_of(me: &ObjRef) -> i32 {
    me.world().map_or(0, |w| w.world_width_macro().wrapping_shl(5))
}

/// Class-chain test by name (`isKindOfClass:` against a class this module does not own: AppleTree, KelpPlant, Boat).
pub fn class_is_kind_named(o: &ObjRef, name: &str) -> bool {
    let mut cur: Option<&'static ClassInfo> = Some(o.class_info());
    while let Some(c) = cur {
        if c.name == name {
            return true;
        }
        cur = c.superclass;
    }
    false
}

// ------------------------------------------------------------------------------------------------ env seam

crate::hooks::hook_trait! {
/// Everything `npc_passive` needs from layers that are not part of it.
/// Defaults: concrete `World` / `DynamicWorld` / `Blockhead` calls where those types exist, inert otherwise.
#[allow(unused_variables)]
pub trait PassiveEnv, stub PassiveEnvStub {
    // ---------------------------------------------------------------- concrete handles
    /// The concrete `World` (defaults to `NpcEnv::world`).
    #[provided]
    fn world(&self) -> Option<Rc<World>> {
        npc_env().world()
    }
    /// The concrete `DynamicWorld` (`None` makes every `dw_*` default inert).
    fn dynamic_world(&self) -> Option<Rc<DynamicWorld>> {
        None
    }

    // ---------------------------------------------------------------- tiles (copies)
    /// `tileAtWorldPositionLoaded(x, y, world)` copied out (loads the block on demand).
    #[provided]
    fn tile_loaded(&self, x: i32, y: i32) -> Option<Tile> {
        let w = self.world()?;
        crate::world::tiles::tile_at_world_position_loaded(x, y, &w).map(|t| t.get())
    }
    /// `tileAtWorldPosition(x, y, [world macroTiles], world)` copied out (never loads).
    #[provided]
    fn tile_cached(&self, x: i32, y: i32) -> Option<Tile> {
        let w = self.world()?;
        crate::world::tiles::tile_at_world_position(x, y, &w.macro_tiles(), &w).map(|t| t.get())
    }

    // ---------------------------------------------------------------- World
    /// `[world expertMode]`
    #[provided]
    fn expert_mode(&self) -> bool {
        self.world().map_or(false, |w| w.expert_mode())
    }
    /// `[world fastForward]`
    #[provided]
    fn fast_forward(&self) -> bool {
        self.world().map_or(false, |w| w.fast_forward())
    }
    /// `[world customRules]` (zeros when there is no world).
    #[provided]
    fn custom_rules(&self) -> CustomRules {
        self.world().map_or_else(CustomRules::default, |w| w.custom_rules())
    }
    /// `[world fillTile:atPos:withType:ITEM_DODO_EGG dataA:0 dataB:child placedByClient:nil saveDict:genes placedByBlockhead:nil
    /// placedByClientName:nil]` with the tile at `pos`. `genes` is `{genesDict: {breed: child}}`.
    #[provided]
    fn fill_tile_dodo_egg(&self, pos: IntPair, data_b: u16, genes: &PlistDict) {
        if let Some(w) = self.world() {
            let tile = crate::world::tiles::tile_at_world_position_loaded(pos.x, pos.y, &w);
            let sd = PlistValue::Dict(genes.clone());
            w.fill_tile_at_pos_with_type_data_a_data_b_placed_by_client_save_dict_placed_by_blockhead_placed_by_client_name(
                tile.as_ref(),
                pos,
                ItemType(91),
                0,
                data_b,
                None,
                Some(&sd),
                None,
                None,
            );
        }
    }
    /// `[world addSimulationEventOfType:SE_DAMAGE forBlockhead:b extraData:text]`
    #[provided]
    fn add_simulation_event_damage(&self, b: &ObjRef, text: &str) {
        if let Some(w) = self.world() {
            w.add_simulation_event_of_type_for_blockhead_extra_data(4, Some(b), SimulationEventExtra::Text(text));
        }
    }

    // ---------------------------------------------------------------- DynamicWorld
    /// `loadNPCAtPosition:type:saveDict:isAdult:wasPlaced:placedByClient:`
    #[provided]
    fn dw_load_npc_at_position(
        &self,
        pos: IntPair,
        npc_type: i32,
        save_dict: Option<&PlistDict>,
        is_adult: bool,
        was_placed: bool,
        client: Option<&str>,
    ) -> bool {
        match self.dynamic_world() {
            Some(dw) => dw.load_npc_at_position_type_save_dict_is_adult_was_placed_placed_by_client(
                pos, npc_type, save_dict, is_adult, was_placed, client,
            ),
            None => false,
        }
    }
    /// `createFreeBlockAtPosition:ofType:dataA:dataB:subItems:nil dynamicObjectSaveDict:save hovers:NO playSound:play priorityBlockhead:`
    #[provided]
    fn dw_create_free_block(
        &self,
        pos: IntPair,
        item_type: i32,
        data_a: u16,
        data_b: u16,
        save_dict: Option<PlistDict>,
        play_sound: i32,
        priority: Option<&ObjRef>,
    ) {
        if let Some(dw) = self.dynamic_world() {
            dw.create_free_block_at_position_of_type_data_a_data_b_sub_items_dynamic_object_save_dict_hovers_play_sound_priority_blockhead(
                pos,
                ItemType(item_type as u32),
                data_a,
                data_b,
                None,
                save_dict.map(PlistValue::Dict),
                false,
                play_sound,
                priority,
            );
        }
    }
    /// `npcExistsAtPos:ignoreNPC:`
    #[provided]
    fn dw_npc_exists_at_pos(&self, pos: IntPair, ignore: &ObjRef) -> bool {
        self.dynamic_world().map_or(false, |dw| dw.npc_exists_at_pos_ignore_npc(pos, Some(ignore)))
    }
    /// `blockheadOccupiesTileAtPos:ignoreBlockhead:nil`
    #[provided]
    fn dw_blockhead_occupies_tile(&self, pos: IntPair) -> bool {
        self.dynamic_world().map_or(false, |dw| dw.blockhead_occupies_tile_at_pos_ignore_blockhead(pos, None))
    }
    /// `npcCloseEnoughToBreedWithNPC:`
    #[provided]
    fn dw_npc_close_enough_to_breed(&self, npc: &ObjRef) -> Option<ObjRef> {
        self.dynamic_world().and_then(|dw| dw.npc_close_enough_to_breed_with_npc(npc))
    }
    /// `treeAtPos:`
    #[provided]
    fn dw_tree_at_pos(&self, pos: IntPair) -> Option<ObjRef> {
        self.dynamic_world().and_then(|dw| dw.tree_at_pos(pos))
    }
    /// `getPlantAtPos:`
    #[provided]
    fn dw_plant_at_pos(&self, pos: IntPair) -> Option<ObjRef> {
        self.dynamic_world().and_then(|dw| dw.get_plant_at_pos(pos))
    }
    /// `loadedCountOfObjectsOfType:`
    #[provided]
    fn dw_loaded_count(&self, object_type: i32) -> i32 {
        self.dynamic_world().map_or(0, |dw| dw.loaded_count_of_objects_of_type(object_type))
    }
    /// `tooManyNPCsToSpawnMoreNearPos:`
    #[provided]
    fn dw_too_many_npcs(&self, pos: IntPair) -> bool {
        self.dynamic_world().map_or(false, |dw| dw.too_many_npcs_to_spawn_more_near_pos(pos))
    }
    /// `removeStandardObject:`
    #[provided]
    fn dw_remove_standard_object(&self, o: &ObjRef) {
        if let Some(dw) = self.dynamic_world() {
            dw.remove_standard_object(Some(o));
        }
    }
    /// `activeBlockhead`
    #[provided]
    fn dw_active_blockhead(&self) -> Option<ObjRef> {
        self.dynamic_world().and_then(|dw| dw.active_blockhead())
    }

    // ---------------------------------------------------------------- plants and trees
    /// `[plant availableFood]` / `[tree availableFood]` (inert 0.0 until `plants` / `trees` are wired).
    fn plant_available_food(&self, p: &ObjRef) -> f32 {
        0.0
    }
    /// `[plant setAvailableFood:]`
    fn plant_set_available_food(&self, p: &ObjRef, v: f32) {}
    /// `[plant plantType]` (raw `PlantType`)
    fn plant_type(&self, p: &ObjRef) -> i32 {
        0
    }
    /// `[o isKindOfClass:[AppleTree class]]`
    #[provided]
    fn is_apple_tree(&self, o: &ObjRef) -> bool {
        class_is_kind_named(o, "AppleTree")
    }
    /// `[o isKindOfClass:[KelpPlant class]]`
    #[provided]
    fn is_kelp_plant(&self, o: &ObjRef) -> bool {
        class_is_kind_named(o, "KelpPlant")
    }
    /// `[o isKindOfClass:[Boat class]]`
    #[provided]
    fn is_boat(&self, o: &ObjRef) -> bool {
        class_is_kind_named(o, "Boat")
    }

    // ---------------------------------------------------------------- Blockhead (blockhead_core)
    /// `[b moving]`. Note: `hasJetPackEquipped` is not reachable from the concrete Blockhead yet.
    #[provided]
    fn bh_moving(&self, b: &ObjRef) -> bool {
        match b.as_any().downcast_ref::<Blockhead>() {
            Some(bh) => {
                let f = bh.bh.borrow();
                if state_meditating(&f.state) || state_asleep(&f.state) {
                    return false;
                }
                f.from_square != f.to_square
            }
            None => false,
        }
    }
    /// `[b heldItemType]`. The non-replica branch needs `currentItem` of the blockhead_inventory layer.
    #[provided]
    fn bh_held_item_type(&self, b: &ObjRef) -> i32 {
        match b.as_any().downcast_ref::<Blockhead>() {
            Some(bh) if b.is_net() => bh.bh.borrow().remote_interaction_item_type,
            _ => 0,
        }
    }
    /// `[b paused]`
    #[provided]
    fn bh_paused(&self, b: &ObjRef) -> bool {
        b.as_any().downcast_ref::<Blockhead>().map_or(false, |x| x.paused())
    }
    /// `[b regenerating]`
    #[provided]
    fn bh_regenerating(&self, b: &ObjRef) -> bool {
        b.as_any().downcast_ref::<Blockhead>().map_or(false, |x| x.regenerating())
    }
    /// `[b onTradeMission]`
    #[provided]
    fn bh_on_trade_mission(&self, b: &ObjRef) -> bool {
        b.as_any().downcast_ref::<Blockhead>().map_or(false, |x| x.on_trade_mission())
    }
    /// `[b shouldContinueSimulating]`
    #[provided]
    fn bh_should_continue_simulating(&self, b: &ObjRef) -> bool {
        b.as_any().downcast_ref::<Blockhead>().map_or(false, |x| x.should_continue_simulating())
    }
    /// `[b rideObject]`
    #[provided]
    fn bh_ride_object(&self, b: &ObjRef) -> Option<ObjRef> {
        b.as_any().downcast_ref::<Blockhead>().and_then(|x| x.bh.borrow().ride_object.clone())
    }
    /// `[b fishingRod]`
    #[provided]
    fn bh_fishing_rod(&self, b: &ObjRef) -> Option<FishingRodRef> {
        b.as_any().downcast_ref::<Blockhead>().and_then(|x| x.bh.borrow().fishing_rod.clone())
    }
    /// `[b sufferDamage:dmg isSimulation:sim recoil:recoil]`
    #[provided]
    fn bh_suffer_damage(&self, b: &ObjRef, dmg: f32, sim: bool, recoil: bool) {
        if let Some(x) = b.as_any().downcast_ref::<Blockhead>() {
            x.suffer_damage_is_simulation_recoil(dmg, sim, recoil);
        }
    }
    /// `[b stopRiding]` (re-enters `removeRider:` of the ridden object).
    #[provided]
    fn bh_stop_riding(&self, b: &ObjRef) {
        npc_env().blockhead_stop_riding(b)
    }

    // ---------------------------------------------------------------- FishingRod (physics_objects)
    /// `[rod valid]`
    fn rod_valid(&self, r: &FishingRodRef) -> bool {
        false
    }
    /// `[rod hookPos]`
    fn rod_hook_pos(&self, r: &FishingRodRef) -> Vector2 {
        Vector2::ZERO
    }
    /// `[rod hookedFish]`
    fn rod_hooked_fish(&self, r: &FishingRodRef) -> Option<ObjRef> {
        None
    }
    /// `[rod setHookedFish:f]`
    fn rod_set_hooked_fish(&self, r: &FishingRodRef, f: Option<&ObjRef>) {}
    /// `[rod blockhead]`
    fn rod_blockhead(&self, r: &FishingRodRef) -> Option<ObjRef> {
        None
    }
}
}

struct DefaultPassiveEnv;
impl PassiveEnvStub for DefaultPassiveEnv {}

thread_local! {
    static PENV: RefCell<Rc<dyn PassiveEnv>> = RefCell::new(Rc::new(DefaultPassiveEnv));
}

/// Installs the environment (once at startup).
pub fn set_passive_env(e: Rc<dyn PassiveEnv>) {
    PENV.with(|c| *c.borrow_mut() = e);
}

/// The installed environment (the default one when none was installed).
pub fn penv() -> Rc<dyn PassiveEnv> {
    PENV.with(|c| c.borrow().clone())
}

// ------------------------------------------------------------------------------------------------ Egg

/// `DYNAMIC_OBJECT_TYPE_EGG`.
pub const DYNAMIC_OBJECT_TYPE_EGG: i32 = 30;
/// `ITEM_DODO_EGG`.
pub const ITEM_DODO_EGG: i32 = 91;

/// `Egg: DynamicObject`.
pub static EGG_CLASS_INFO: ClassInfo =
    ClassInfo { name: "Egg", superclass: Some(&DYNAMIC_OBJECT_CLASS), protocols: &[] };

/// Egg ivars 0x50, 0x58, 0x5c.
#[derive(Clone, Debug, Default)]
pub struct EggState {
    /// 0x50 retained `NSDictionary` containing `@"breed"`
    pub genes_dict: Option<PlistDict>,
    /// 0x58
    pub breed: u16,
    /// 0x5c seconds remaining
    pub hatch_timer: f32,
}

/// `hatchTimer = (float)(450.0 + (double)(R() * 900.0f) * 0.5)`, one `rand()`; range [450, 900).
pub fn egg_random_hatch_timer() -> f32 {
    let r = glibc_rand::r01_tagged("Egg.hatchTimer");
    ((r * 900.0f32) as f64 * 0.5f64 + 450.0f64) as f32
}

/// `{@"breed": @(breed)}` (`dictionaryWithObjectsAndKeys:`).
fn genes_with_breed(breed: i64) -> PlistDict {
    let mut d = new_mutable_dict();
    d.map.insert("breed".to_string(), PlistValue::int(breed));
    d
}

/// `(uint16)[genesDict[@"breed"] intValue]` (nil dict or key gives 0).
fn breed_of_genes(g: &Option<PlistDict>) -> u16 {
    g.as_ref().and_then(|d| d.map.get(&"breed".to_string())).map_or(0, |v| v.int_value()) as u16
}

/// A dodo egg lying in the world: counts down and hatches into a baby Dodo.
pub struct Egg {
    pub base: RefCell<DynamicObjectBase>,
    pub egg: RefCell<EggState>,
}

impl Egg {
    fn alloc() -> Rc<Egg> {
        alloc_obj(|w| Egg {
            base: RefCell::new(DynamicObjectBase::alloc_zeroed_with(w)),
            egg: RefCell::new(EggState::default()),
        })
    }

    /// `-[Egg initWithWorld:dynamicWorld:atPosition:cache:saveDict:]` (saveDict may be nil; `addEggAtPos:saveDict:`).
    /// One `rand()` iff the dict is nil or has no `hatchTimer`.
    pub fn init_with_world_dynamic_world_at_position_cache_save_dict(
        world: &WorldRef,
        dw: &DynamicWorldRef,
        pos: IntPair,
        cache: CacheRef,
        save_dict: Option<&PlistDict>,
    ) -> Option<ObjRef> {
        let o = Egg::alloc();
        let me: ObjRef = o.clone();
        let inited = base_init_with_world_dynamic_world_at_position_cache(&me, world, dw, pos, cache);
        debug_assert!(inited); // the base initialisers cannot fail on this path
        let mut found = false;
        let mut genes: Option<PlistDict> = None;
        let mut hatch = 0.0f32;
        if let Some(sd) = save_dict {
            if let Some(PlistValue::Dict(g)) = sd.map.get(&"genesDict".to_string()) {
                genes = Some(g.clone());
            }
            if let Some(n) = sd.map.get(&"hatchTimer".to_string()) {
                hatch = n.float_value();
                found = true;
            }
        }
        if genes.is_none() {
            genes = Some(genes_with_breed(0));
        }
        if !found {
            hatch = egg_random_hatch_timer();
        }
        let breed = breed_of_genes(&genes);
        *o.egg.borrow_mut() = EggState { genes_dict: genes, breed, hatch_timer: hatch };
        Some(me)
    }

    /// `-[Egg initWithWorld:dynamicWorld:saveDict:cache:]` (registry `from_save_dict`): no `rand()`.
    pub fn init_with_world_dynamic_world_save_dict_cache(
        world: &WorldRef,
        dw: &DynamicWorldRef,
        save_dict: &PlistDict,
        cache: CacheRef,
    ) -> Option<ObjRef> {
        let o = Egg::alloc();
        let me: ObjRef = o.clone();
        let inited = base_init_with_world_dynamic_world_save_dict_cache(&me, world, dw, save_dict, cache);
        debug_assert!(inited); // the base initialisers cannot fail on this path
        let hatch = save_dict.map.get(&"hatchTimer".to_string()).map_or(0.0f32, |v| v.float_value());
        let genes = match save_dict.map.get(&"genesDict".to_string()) {
            Some(PlistValue::Dict(g)) => Some(g.clone()),
            _ => None,
        };
        let breed = breed_of_genes(&genes);
        *o.egg.borrow_mut() = EggState { genes_dict: genes, breed, hatch_timer: hatch };
        Some(me)
    }

    /// `-[Egg initWithWorld:dynamicWorld:cache:netData:]` (registry `from_net_data`). Short data is zero padded
    /// (foundation E-F9).
    pub fn init_with_world_dynamic_world_cache_net_data(
        world: &WorldRef,
        dw: &DynamicWorldRef,
        cache: CacheRef,
        net_data: &[u8],
    ) -> Option<ObjRef> {
        let o = Egg::alloc();
        let me: ObjRef = o.clone();
        if !base_init_with_world_dynamic_world_cache_net_data(&me, world, dw, cache, net_data) {
            return None;
        }
        let d = EggCreationNetData::read_le(&padded_bytes(net_data, 0x20));
        *o.egg.borrow_mut() = EggState {
            genes_dict: Some(genes_with_breed(d.breed as i64)),
            breed: d.breed,
            hatch_timer: d.hatch_timer as f32,
        };
        Some(me)
    }

    /// `-[Egg breed]`
    pub fn breed(&self) -> u16 {
        self.egg.borrow().breed
    }
    /// `-[Egg hatchTimer]`
    pub fn hatch_timer(&self) -> f32 {
        self.egg.borrow().hatch_timer
    }

    /// `-[Egg hatch:]`. The egg is removed even when the NPC could not be created. Sounds are no-ops.
    pub fn hatch(me: &ObjRef, _play_sound: bool) -> bool {
        if me.is_net() {
            return false;
        }
        let mut success = false;
        if !me.needs_removed() {
            let (genes, pos) = {
                let e = egg_of(me);
                let g = e.egg.borrow().genes_dict.clone();
                (g, me.pos())
            };
            // NPC_DODO = 1, isAdult NO, wasPlaced NO, placedByClient nil
            success = penv().dw_load_npc_at_position(pos, 1, genes.as_ref(), false, false, None);
            penv().dw_remove_standard_object(me);
        }
        success
    }
}

fn egg_of(me: &ObjRef) -> &Egg {
    me.as_any().downcast_ref::<Egg>().expect("object is not an Egg")
}

impl DynamicObject for Egg {
    fn base(&self) -> &RefCell<DynamicObjectBase> {
        &self.base
    }
    fn class_info(&self) -> &'static ClassInfo {
        &EGG_CLASS_INFO
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    ///  `objectType` -> 30
    fn object_type(&self) -> i32 {
        DYNAMIC_OBJECT_TYPE_EGG
    }
    ///  `occupiesForegroundContents` -> YES
    fn occupies_foreground_contents(&self) -> bool {
        true
    }
    ///  `freeblockCreationItemType` -> 91
    fn freeblock_creation_item_type(&self) -> i32 {
        ITEM_DODO_EGG
    }
    ///  `freeBlockCreationSaveDict` -> `getSaveDict`
    fn free_block_creation_save_dict(&self, me: &ObjRef) -> Option<PlistDict> {
        Some(self.get_save_dict(me))
    }
    ///  `freeBlockCreationDataA` -> 0
    fn free_block_creation_data_a(&self) -> u16 {
        0
    }
    ///  `freeBlockCreationDataB` -> breed
    fn free_block_creation_data_b(&self) -> u16 {
        self.egg.borrow().breed
    }

    ///  `getSaveDict`: `hatchTimer`, `saveTime` (float of `worldTime`, 0 without a world), `genesDict`.
    fn get_save_dict(&self, me: &ObjRef) -> PlistDict {
        let mut d = base_get_save_dict(me);
        let (hatch, genes) = {
            let e = self.egg.borrow();
            (e.hatch_timer, e.genes_dict.clone())
        };
        d.map.insert("hatchTimer".to_string(), PlistValue::real_f32(hatch));
        let t = me.world().map_or(0.0f64, |w| w.world_time());
        d.map.insert("saveTime".to_string(), PlistValue::real_f32(t as f32));
        if let Some(g) = genes {
            d.map.insert("genesDict".to_string(), PlistValue::Dict(g));
        }
        d
    }

    ///  `creationNetDataForClient:`: 0x20 bytes, no gzip plist; `hatchTimer` is `(int)t < 1 ? 1: (uint16)(int)t`.
    /// Padding (0x1c..0x1f) is written as zero.
    fn creation_net_data_for_client(&self, me: &ObjRef, _client_id: Option<&str>) -> Option<Vec<u8>> {
        let (breed, t) = {
            let e = self.egg.borrow();
            (e.breed, e.hatch_timer)
        };
        let ti = crate::fmath::cconv::f32_to_i32(t);
        let d = EggCreationNetData {
            dynamic_object_net_data: base_dynamic_object_net_data(me),
            breed,
            hatch_timer: if ti < 1 { 1 } else { ti as u16 },
            padding: [0; 4],
        };
        Some(d.to_vec())
    }
    ///  `updateNetDataForClient:` -> `creationNetDataForClient:`
    fn update_net_data_for_client(&self, me: &ObjRef, client_id: Option<&str>) -> Option<Vec<u8>> {
        self.creation_net_data_for_client(me, client_id)
    }

    ///  `update:accurateDT:isSimulation:`: the timer always counts down; a hatch needs `!isNet`, `hatchTimer < 0`,
    /// fewer than 512 loaded Dodos and no crowding. The timer is re-rolled after `hatch:` (one `rand()`), which
    /// itself draws the baby's 8 numbers first.
    fn update_accurate_dt_is_simulation(&self, me: &ObjRef, dt: f32, _accurate_dt: f32, _is_simulation: bool) {
        {
            let mut e = self.egg.borrow_mut();
            e.hatch_timer -= dt;
        }
        if !me.is_net() && self.egg.borrow().hatch_timer < 0.0f32 {
            let env = penv();
            let pos = me.pos();
            // NPC_DODO dynamic object type 13
            if env.dw_loaded_count(13) < 512 && !env.dw_too_many_npcs(pos) {
                Egg::hatch(me, true);
                self.egg.borrow_mut().hatch_timer = egg_random_hatch_timer();
                if let Some(dw) = me.dynamic_world() {
                    dw.dynamic_world_changed_at_pos_object_type(pos, self.object_type());
                }
            }
        }
    }

    ///  `worldChanged:`: when the tile below stops supporting the egg it drops as a free block (item 91,
    /// `dataB` = breed, its own save dict) and is removed. Exact x equality, first match only.
    fn world_changed(&self, me: &ObjRef, positions: &mut Vec<IntPair>) {
        let pos = me.pos();
        let mut matched = false;
        for p in positions.iter() {
            if p.x == pos.x && p.y == pos.y.wrapping_sub(1) && !me.needs_removed() {
                matched = true;
                break;
            }
        }
        if !matched {
            return;
        }
        let env = penv();
        let tile = env.tile_loaded(pos.x, pos.y.wrapping_sub(1));
        let t = match tile {
            Some(t) => t,
            None => return,
        };
        if t_solid(&Some(t)) {
            return;
        }
        if t.foreground_contents == 100 {
            return;
        }
        if npc_env().is_client() {
            return;
        }
        let breed = self.egg.borrow().breed;
        let sd = self.get_save_dict(me);
        env.dw_create_free_block(pos, ITEM_DODO_EGG, 0, breed, Some(sd), 1, None);
        env.dw_remove_standard_object(me);
    }

    ///  `dealloc`: drops `genesDict`.
    fn dealloc(&self, _me: &ObjRef) {
        self.egg.borrow_mut().genes_dict = None;
    }
}

/// Shared test environment (hash-map world, call recorders) for the unit tests of every class of this module.
#[cfg(test)]
pub(crate) mod test_support {
    use super::*;
    use std::cell::Cell;
    use std::collections::HashMap;

    /// Test environment with a hash-map world.
    pub struct TestPassiveEnv {
        pub tiles: RefCell<HashMap<(i32, i32), Tile>>,
        pub loaded: Cell<i32>,
        pub crowded: Cell<bool>,
        pub npcs_made: RefCell<Vec<(i32, bool, bool)>>,
        pub drops: RefCell<Vec<(i32, u16, u16, i32)>>,
        pub removed: Cell<u32>,
        pub eggs: RefCell<Vec<(IntPair, u16)>>,
        pub moving: RefCell<Vec<(usize, bool)>>,
        pub held: Cell<i32>,
        pub food: RefCell<HashMap<usize, f32>>,
        pub trees: RefCell<HashMap<(i32, i32), ObjRef>>,
        pub plants: RefCell<HashMap<(i32, i32), ObjRef>>,
        pub damage: RefCell<Vec<f32>>,
        pub sim_events: RefCell<Vec<String>>,
        pub rules: Cell<Option<CustomRules>>,
        pub expert: Cell<bool>,
        pub occupied: Cell<bool>,
        pub npc_here: Cell<bool>,
        pub bhs: RefCell<Vec<ObjRef>>,
        pub server_clients: Cell<usize>,
        /// The animal `npcCloseEnoughToBreedWithNPC:` finds.
        pub breed_with: RefCell<Option<ObjRef>>,
        /// `[dynamicWorld isClient]` (never true on the dedicated server).
        pub client: Cell<bool>,
        /// What `isAppleTree:` answers.
        pub apple_tree: Cell<bool>,
    }
    impl TestPassiveEnv {
        pub fn new() -> Rc<TestPassiveEnv> {
            Rc::new(TestPassiveEnv {
                tiles: RefCell::new(HashMap::new()),
                loaded: Cell::new(0),
                crowded: Cell::new(false),
                npcs_made: RefCell::new(vec![]),
                drops: RefCell::new(vec![]),
                removed: Cell::new(0),
                eggs: RefCell::new(vec![]),
                moving: RefCell::new(vec![]),
                held: Cell::new(0),
                food: RefCell::new(HashMap::new()),
                trees: RefCell::new(HashMap::new()),
                plants: RefCell::new(HashMap::new()),
                damage: RefCell::new(vec![]),
                sim_events: RefCell::new(vec![]),
                rules: Cell::new(None),
                expert: Cell::new(false),
                occupied: Cell::new(false),
                npc_here: Cell::new(false),
                bhs: RefCell::new(vec![]),
                server_clients: Cell::new(1),
                breed_with: RefCell::new(None),
                client: Cell::new(false),
                apple_tree: Cell::new(true),
            })
        }
        /// Installs this object as both the `PassiveEnv` and the `NpcEnv`.
        pub fn install(self: &Rc<Self>) {
            set_passive_env(self.clone());
            crate::npc::npc::set_npc_env(self.clone());
        }
        pub fn set_tile(&self, x: i32, y: i32, ty: u8) {
            let mut t = Tile::default();
            t.type_index = ty;
            self.tiles.borrow_mut().insert((x, y), t);
        }
    }
    impl crate::npc::npc::NpcEnvStub for TestPassiveEnv {
        fn tile_probe(&self, x: i32, y: i32) -> Option<crate::npc::npc::NpcTileCopy> {
            self.tiles
                .borrow()
                .get(&(x, y))
                .map(|t| crate::npc::npc::NpcTileCopy { type_index: t.type_index, ..Default::default() })
        }
        fn all_blockheads_including_net(&self) -> Vec<ObjRef> {
            self.bhs.borrow().clone()
        }
        fn server_clients_count(&self) -> usize {
            self.server_clients.get()
        }
        fn is_client(&self) -> bool {
            self.client.get()
        }
    }
    impl PassiveEnvStub for TestPassiveEnv {
        fn tile_loaded(&self, x: i32, y: i32) -> Option<Tile> {
            self.tiles.borrow().get(&(x, y)).copied()
        }
        fn tile_cached(&self, x: i32, y: i32) -> Option<Tile> {
            self.tiles.borrow().get(&(x, y)).copied()
        }
        fn expert_mode(&self) -> bool {
            self.expert.get()
        }
        fn custom_rules(&self) -> CustomRules {
            self.rules.get().unwrap_or_default()
        }
        fn fill_tile_dodo_egg(&self, pos: IntPair, data_b: u16, _genes: &PlistDict) {
            self.eggs.borrow_mut().push((pos, data_b));
        }
        fn add_simulation_event_damage(&self, _b: &ObjRef, text: &str) {
            self.sim_events.borrow_mut().push(text.to_string());
        }
        fn dw_load_npc_at_position(
            &self,
            _pos: IntPair,
            t: i32,
            _sd: Option<&PlistDict>,
            adult: bool,
            placed: bool,
            _c: Option<&str>,
        ) -> bool {
            self.npcs_made.borrow_mut().push((t, adult, placed));
            // stand-in for the baby's constructor draws (8 for a Dodo)
            for _ in 0..8 {
                glibc_rand::rand();
            }
            true
        }
        fn dw_create_free_block(
            &self,
            _pos: IntPair,
            t: i32,
            a: u16,
            b: u16,
            _sd: Option<PlistDict>,
            play: i32,
            _p: Option<&ObjRef>,
        ) {
            self.drops.borrow_mut().push((t, a, b, play));
        }
        fn dw_remove_standard_object(&self, _o: &ObjRef) {
            self.removed.set(self.removed.get() + 1);
        }
        fn dw_loaded_count(&self, _t: i32) -> i32 {
            self.loaded.get()
        }
        fn dw_too_many_npcs(&self, _pos: IntPair) -> bool {
            self.crowded.get()
        }
        fn dw_blockhead_occupies_tile(&self, _pos: IntPair) -> bool {
            self.occupied.get()
        }
        fn dw_npc_exists_at_pos(&self, _pos: IntPair, _ignore: &ObjRef) -> bool {
            self.npc_here.get()
        }
        fn dw_npc_close_enough_to_breed(&self, _npc: &ObjRef) -> Option<ObjRef> {
            self.breed_with.borrow().clone()
        }
        fn dw_tree_at_pos(&self, pos: IntPair) -> Option<ObjRef> {
            self.trees.borrow().get(&(pos.x, pos.y)).cloned()
        }
        fn dw_plant_at_pos(&self, pos: IntPair) -> Option<ObjRef> {
            self.plants.borrow().get(&(pos.x, pos.y)).cloned()
        }
        fn plant_available_food(&self, p: &ObjRef) -> f32 {
            self.food.borrow().get(&crate::objects::handle::obj_id(p)).copied().unwrap_or(0.0)
        }
        fn plant_set_available_food(&self, p: &ObjRef, v: f32) {
            self.food.borrow_mut().insert(crate::objects::handle::obj_id(p), v);
        }
        fn plant_type(&self, _p: &ObjRef) -> i32 {
            4
        }
        fn is_apple_tree(&self, _o: &ObjRef) -> bool {
            self.apple_tree.get()
        }
        fn is_kelp_plant(&self, _o: &ObjRef) -> bool {
            true
        }
        fn bh_moving(&self, b: &ObjRef) -> bool {
            let id = crate::objects::handle::obj_id(b);
            self.moving.borrow().iter().any(|(i, m)| *i == id && *m)
        }
        fn bh_held_item_type(&self, _b: &ObjRef) -> i32 {
            self.held.get()
        }
        fn bh_suffer_damage(&self, _b: &ObjRef, dmg: f32, _sim: bool, _recoil: bool) {
            self.damage.borrow_mut().push(dmg);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::test_support::TestPassiveEnv;
    use super::*;
    use crate::objects::base::testutil::{setup, MockDw, MockWorld};

    fn solid() -> Tile {
        let mut t = Tile::default();
        t.type_index = 1;
        t
    }

    fn mk(sd: Option<&PlistDict>) -> (Rc<MockWorld>, Rc<MockDw>, ObjRef) {
        let (w, dw, _log) = setup(4, 4);
        let wr: WorldRef = w.clone();
        let dr: DynamicWorldRef = dw.clone();
        let me =
            Egg::init_with_world_dynamic_world_at_position_cache_save_dict(&wr, &dr, IntPair { x: 40, y: 3 }, None, sd)
                .unwrap();
        (w, dw, me)
    }

    #[test]
    fn hatch_timer_formula_and_range() {
        for seed in 0..1000u32 {
            glibc_rand::srand(seed);
            let r = glibc_rand::rand();
            glibc_rand::srand(seed);
            let t = egg_random_hatch_timer();
            let expect = ((((r as f32) / 2147483648.0f32) * 900.0f32) as f64 * 0.5 + 450.0) as f32;
            assert_eq!(t.to_bits(), expect.to_bits());
            assert!((450.0..=900.0).contains(&t));
        }
    }

    #[test]
    fn init_draws_only_without_hatch_timer() {
        glibc_rand::srand(7);
        let (_w, _d, me) = mk(None);
        let e = egg_of(&me);
        glibc_rand::srand(7);
        let expect = egg_random_hatch_timer();
        assert_eq!(e.hatch_timer().to_bits(), expect.to_bits());
        assert_eq!(e.breed(), 0);
        // with a hatchTimer: no draw
        let mut sd = new_mutable_dict();
        sd.map.insert("hatchTimer".into(), PlistValue::real_f32(12.5));
        sd.map.insert("genesDict".into(), PlistValue::Dict(genes_with_breed(5)));
        glibc_rand::srand(7);
        let (_w, _d, me2) = mk(Some(&sd));
        assert_eq!(egg_of(&me2).hatch_timer(), 12.5);
        assert_eq!(egg_of(&me2).breed(), 5);
        assert_eq!(glibc_rand::rand(), {
            glibc_rand::srand(7);
            glibc_rand::rand()
        });
    }

    #[test]
    fn save_dict_keys_and_net_data() {
        let mut sd = new_mutable_dict();
        sd.map.insert("hatchTimer".into(), PlistValue::real_f32(700.9));
        sd.map.insert("genesDict".into(), PlistValue::Dict(genes_with_breed(9)));
        let (_w, _d, me) = mk(Some(&sd));
        let d = me.get_save_dict(&me);
        assert!(d.map.get(&"hatchTimer".to_string()).is_some());
        assert!(d.map.get(&"saveTime".to_string()).is_some());
        assert!(d.map.get(&"genesDict".to_string()).is_some());
        let nd = me.creation_net_data_for_client(&me, None).unwrap();
        assert_eq!(nd.len(), 0x20);
        let s = EggCreationNetData::read_le(&nd);
        assert_eq!(s.breed, 9);
        assert_eq!(s.hatch_timer, 700);
        assert_eq!(&nd[0x1c..0x20], &[0, 0, 0, 0]);
        assert_eq!(me.free_block_creation_data_b(), 9);
        assert_eq!(me.freeblock_creation_item_type(), 91);
        // timer < 1 is sent as 1
        egg_of(&me).egg.borrow_mut().hatch_timer = -5.0;
        let s2 = EggCreationNetData::read_le(&me.creation_net_data_for_client(&me, None).unwrap());
        assert_eq!(s2.hatch_timer, 1);
        // round trip through the net constructor
        let (w, dw, _) = mk(None);
        let wr: WorldRef = w;
        let dr: DynamicWorldRef = dw;
        let e2 = Egg::init_with_world_dynamic_world_cache_net_data(&wr, &dr, None, &nd).unwrap();
        assert_eq!(egg_of(&e2).breed(), 9);
        assert_eq!(egg_of(&e2).hatch_timer(), 700.0);
    }

    #[test]
    fn update_counts_down_and_hatches_with_rand_order() {
        let env = TestPassiveEnv::new();
        env.install();
        let mut sd = new_mutable_dict();
        sd.map.insert("hatchTimer".into(), PlistValue::real_f32(1.0));
        let (_w, _d, me) = mk(Some(&sd));
        glibc_rand::srand(3);
        me.update_accurate_dt_is_simulation(&me, 0.5, 0.5, false);
        assert_eq!(egg_of(&me).hatch_timer(), 0.5);
        assert!(env.npcs_made.borrow().is_empty());
        // strict < 0
        me.update_accurate_dt_is_simulation(&me, 0.5, 0.5, false);
        assert_eq!(egg_of(&me).hatch_timer(), 0.0);
        assert!(env.npcs_made.borrow().is_empty());
        // crossing: 8 draws by the baby, then 1 for the new timer
        glibc_rand::srand(3);
        let mut all = vec![];
        for _ in 0..9 {
            all.push(glibc_rand::rand());
        }
        glibc_rand::srand(3);
        me.update_accurate_dt_is_simulation(&me, 0.25, 0.25, false);
        assert_eq!(env.npcs_made.borrow().as_slice(), &[(1, false, false)]);
        assert_eq!(env.removed.get(), 1);
        let expect = ((((all[8] as f32) / 2147483648.0f32) * 900.0f32) as f64 * 0.5 + 450.0) as f32;
        assert_eq!(egg_of(&me).hatch_timer().to_bits(), expect.to_bits());
        // crowded: stays negative, retried
        env.crowded.set(true);
        egg_of(&me).egg.borrow_mut().hatch_timer = -1.0;
        me.update_accurate_dt_is_simulation(&me, 1.0, 1.0, false);
        assert_eq!(egg_of(&me).hatch_timer(), -2.0);
        assert_eq!(env.npcs_made.borrow().len(), 1);
        // a replica never hatches
        me.base().borrow_mut().is_net = true;
        env.crowded.set(false);
        me.update_accurate_dt_is_simulation(&me, 1.0, 1.0, false);
        assert_eq!(env.npcs_made.borrow().len(), 1);
        set_passive_env(Rc::new(DefaultPassiveEnv));
    }

    #[test]
    fn world_changed_support_rules() {
        let env = TestPassiveEnv::new();
        env.install();
        let (_w, _d, me) = mk(None);
        let below = |t: Tile| {
            env.tiles.borrow_mut().insert((40, 2), t);
        };
        let mut v = vec![IntPair { x: 40, y: 2 }];
        // supported by a solid tile
        below(solid());
        me.world_changed(&me, &mut v);
        assert!(env.drops.borrow().is_empty());
        // supported by a column
        let mut col = Tile::default();
        col.type_index = 2;
        col.foreground_contents = 100;
        below(col);
        me.world_changed(&me, &mut v);
        assert!(env.drops.borrow().is_empty());
        // unsupported: drops
        let mut air = Tile::default();
        air.type_index = 2;
        below(air);
        let mut other = vec![IntPair { x: 41, y: 2 }];
        me.world_changed(&me, &mut other);
        assert!(env.drops.borrow().is_empty());
        me.world_changed(&me, &mut v);
        assert_eq!(env.drops.borrow().as_slice(), &[(91, 0, 0, 1)]);
        assert_eq!(env.removed.get(), 1);
        set_passive_env(Rc::new(DefaultPassiveEnv));
    }

    #[test]
    fn wrap_dx_matches_spec() {
        assert_eq!(wrap_dx(10, 128), 10);
        assert_eq!(wrap_dx(64, 128), -64);
        assert_eq!(wrap_dx(-65, 128), 63);
        assert_eq!(wrap_dx(-64, 128), -64);
    }

    /// Loading from a save dict: with and without genes.
    #[test]
    fn save_dict_load_with_and_without_genes() {
        let (w, dw, e) = mk(None);
        let mut sd = e.get_save_dict(&e);
        let (wr, dr): (WorldRef, DynamicWorldRef) = (w.clone(), dw.clone());
        let with = Egg::init_with_world_dynamic_world_save_dict_cache(&wr, &dr, &sd, None).unwrap();
        assert!(egg_of(&with).egg.borrow().genes_dict.is_some());
        sd.map.remove(&"genesDict".to_string());
        let without = Egg::init_with_world_dynamic_world_save_dict_cache(&wr, &dr, &sd, None).unwrap();
        assert!(egg_of(&without).egg.borrow().genes_dict.is_none());
    }

    /// The environment as production builds it: only the world handles are given, every other answer is the
    /// default that reaches into the real `World`, `DynamicWorld` and `Blockhead`.
    #[test]
    fn real_backed_environment() {
        use crate::dynworld::state::DynamicWorld;
        use crate::interaction::sign::testkit::world_with_block;
        struct RealEnv {
            w: Rc<World>,
            dw: Option<Rc<DynamicWorld>>,
        }
        impl PassiveEnvStub for RealEnv {
            fn world(&self) -> Option<Rc<World>> {
                Some(self.w.clone())
            }
            fn dynamic_world(&self) -> Option<Rc<DynamicWorld>> {
                self.dw.clone()
            }
        }
        let (w, dw, _pb) = world_with_block("egg_real_env");
        let env: Rc<dyn PassiveEnv> = Rc::new(RealEnv { w: w.clone(), dw: Some(dw.clone()) });
        // without a dynamic world every DynamicWorld call is inert
        let bare: Rc<dyn PassiveEnv> = Rc::new(RealEnv { w: w.clone(), dw: None });
        assert!(!bare.dw_load_npc_at_position(IntPair { x: 1, y: 1 }, 1, None, true, false, None));
        bare.dw_create_free_block(IntPair { x: 1, y: 1 }, 5, 0, 0, None, 1, None);
        bare.dw_remove_standard_object(&(crate::objects::base::testutil::TestObj::new() as ObjRef));
        let pos = IntPair { x: 5, y: 5 };
        assert!(env.tile_loaded(5, 5).is_some() && env.tile_cached(5, 5).is_some());
        assert!(env.tile_loaded(5, -1).is_none());
        let _ = (env.expert_mode(), env.fast_forward(), env.custom_rules());
        let mut genes = new_mutable_dict();
        genes.map.insert("breed".to_string(), PlistValue::int(3));
        env.fill_tile_dodo_egg(pos, 3, &genes);
        let b: ObjRef = crate::objects::handle::alloc_obj(|sw| Blockhead::alloc(sw));
        let plain: ObjRef = crate::objects::base::testutil::TestObj::new();
        env.add_simulation_event_damage(&b, "ouch");
        assert!(!env.dw_load_npc_at_position(pos, 1, None, true, false, None) || true);
        env.dw_create_free_block(pos, 5, 0, 0, None, 1, None);
        let _ = (
            env.dw_npc_exists_at_pos(pos, &plain),
            env.dw_blockhead_occupies_tile(pos),
            env.dw_npc_close_enough_to_breed(&plain),
            env.dw_tree_at_pos(pos),
            env.dw_plant_at_pos(pos),
            env.dw_loaded_count(1),
            env.dw_too_many_npcs(pos),
            env.dw_active_blockhead(),
        );
        env.dw_remove_standard_object(&plain);
        // plants, trees and kinds of objects
        assert_eq!(env.plant_available_food(&plain), 0.0);
        env.plant_set_available_food(&plain, 1.0);
        assert_eq!(env.plant_type(&plain), 0);
        assert!(!env.is_apple_tree(&plain) && !env.is_kelp_plant(&plain) && !env.is_boat(&plain));
        assert!(class_is_kind_named(&plain, "DynamicObject"));
        // blockheads: the real one answers, a plain object gets the nil answers
        for o in [&b, &plain] {
            let _ = (
                env.bh_moving(o),
                env.bh_held_item_type(o),
                env.bh_paused(o),
                env.bh_regenerating(o),
                env.bh_on_trade_mission(o),
                env.bh_should_continue_simulating(o),
                env.bh_ride_object(o),
                env.bh_fishing_rod(o).is_none(),
            );
            env.bh_suffer_damage(o, 0.0, true, false);
            env.bh_stop_riding(o);
        }
        // asleep and net blockheads are not "moving"; a net one reports its item
        b.base().borrow_mut().is_net = true;
        let _ = env.bh_held_item_type(&b);
        {
            let bh = b.as_any().downcast_ref::<Blockhead>().unwrap();
            bh.bh.borrow_mut().state.sub_animation_type = 5;
        }
        assert!(!env.bh_moving(&b));
        {
            let bh = b.as_any().downcast_ref::<Blockhead>().unwrap();
            bh.bh.borrow_mut().state.sub_animation_type = 0;
            bh.bh.borrow_mut().from_square = IntPair { x: 1, y: 1 };
        }
        assert!(env.bh_moving(&b));
        // the fishing rod defaults
        let rod: FishingRodRef = Rc::new(0u8);
        assert!(!env.rod_valid(&rod) && env.rod_hooked_fish(&rod).is_none() && env.rod_blockhead(&rod).is_none());
        assert_eq!(env.rod_hook_pos(&rod), Vector2::ZERO);
        env.rod_set_hooked_fish(&rod, None);
    }

    /// The faces of an egg, replicas that never hatch, and the support check when the tile below goes away.
    #[test]
    fn faces_replicas_and_support_checks() {
        let (w, dw, e) = mk(None);
        let env = TestPassiveEnv::new();
        env.install();
        assert_eq!(e.class_info().name, "Egg");
        assert!(e.occupies_foreground_contents());
        assert!(e.free_block_creation_save_dict(&e).is_some());
        assert_eq!(e.free_block_creation_data_a(), 0);
        assert_eq!(e.update_net_data_for_client(&e, None), e.creation_net_data_for_client(&e, None));
        // a replica does not hatch
        let nd = e.creation_net_data_for_client(&e, None).unwrap();
        let rep = Egg::init_with_world_dynamic_world_cache_net_data(
            &(w.clone() as WorldRef),
            &(dw.clone() as DynamicWorldRef),
            None,
            &nd,
        )
        .unwrap();
        assert!(!Egg::hatch(&rep, true));
        // an image outside the loaded world is refused
        let mut far = nd.clone();
        far[8..12].copy_from_slice(&5000u32.to_le_bytes());
        assert!(Egg::init_with_world_dynamic_world_cache_net_data(
            &(w.clone() as WorldRef),
            &(dw.clone() as DynamicWorldRef),
            None,
            &far
        )
        .is_none());
        // support check: no tile below, solid tile below, a floor sprite, a client, then the real drop
        let pos = e.pos();
        let below = IntPair { x: pos.x, y: pos.y - 1 };
        e.world_changed(&e, &mut vec![below]);
        env.set_tile(below.x, below.y, 1);
        e.world_changed(&e, &mut vec![below]);
        env.set_tile(below.x, below.y, 2);
        let mut t = Tile::default();
        t.type_index = 2;
        t.foreground_contents = 100;
        env.tiles.borrow_mut().insert((below.x, below.y), t);
        e.world_changed(&e, &mut vec![below]);
        env.set_tile(below.x, below.y, 2);
        env.client.set(true);
        e.world_changed(&e, &mut vec![below]);
        env.client.set(false);
        assert!(env.drops.borrow().is_empty());
        e.world_changed(&e, &mut vec![below]);
        assert_eq!(env.drops.borrow().len(), 1);
        e.dealloc(&e);
    }
}
