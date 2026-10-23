//! `CaveTroll: NPC <PathUserDynamicObject>`: the hostile cave dweller (NPC type 6, dynamic object type 39).
//!
//!  (`preDrawUpdate:`) is NOT ported (resolution R1: client only, no `rand()` draws on the server).
//!
//! ## Shared support (also used by `drop_bear.rs` and `scorpion.rs`)
//! This file also holds [`HostileHost`], the seam to the layers that were not available when this module was
//! written (concrete `DynamicWorld` queries, free block creation, cactus food, `Blockhead` calls, `World` calls).
//! Every method has a default: the `World` / `Blockhead` calls are implemented on the concrete types reached
//! through [`npc_env`]`().world()`; the `DynamicWorld` calls are inert. The integrator installs one
//! implementation with [`set_hostile_host`]. //!
//! ## Numerics / RNG
//! `f32` stays `f32`; `(double)` expressions are computed in `f64` and narrowed where the spec says so.
//! Every `rand()` goes through `rng::glibc_rand` and the call order of each function is part of the contract

use std::any::Any;
use std::cell::RefCell;
use std::rc::Rc;

use crate::blockhead::blockhead::as_blockhead;
use crate::fmath::cconv::{f32_to_i32, f64_to_i32};
use crate::fmath::libm::{powf, sinf};
use crate::fmath::vector::linear_interpolate;
use crate::impl_npc_protocols;
use crate::npc::npc::{
    alloc_npc, npc_add_rider, npc_blockhead_unloaded, npc_die, npc_env, npc_get_save_dict,
    npc_hit_with_force_blockhead,
    npc_init_with_world_dynamic_world_at_position_cache_save_dict_is_adult_was_placed_placed_by_client,
    npc_init_with_world_dynamic_world_cache_net_data, npc_init_with_world_dynamic_world_save_dict_cache,
    npc_remote_creation_data_update, npc_remote_update, npc_remove_rider_opt, npc_update_accurate_dt_is_simulation,
    register_npc_ctor, Npc, NpcState, NPC_CAVETROLL, NPC_CLASS,
};
use crate::npc_forward;
use crate::objects::base::{padded_bytes, DynamicObject, DynamicObjectBase, DynamicWorldRef, WorldRef};
use crate::objects::class_info::ClassInfo;
use crate::objects::handle::{ptr_eq, CacheRef, ObjRef};
use crate::objects::protocols::PathUserDynamicObject;
use crate::path::can_enter::{check_can_enter_tile, FromProps};
use crate::path::dpad::dpad_find_path;
use crate::path::path_creator::PathStep;
use crate::plist::value::{PlistDict, PlistValue};
use crate::rng::glibc_rand;
use crate::types::netdata::{CaveTrollCreationData, CaveTrollUpdateData, NPCUpdateNetData};
use crate::types::structs::{make_intpair, CaveTrollState, CustomRules, IntPair, Vector, Vector2, WireStruct};
use crate::types::tile::Tile;
use crate::world::helpers::{
    tile_contains_door, tile_contains_gate, tile_contains_trap_door, tile_is_climable, tile_is_solid, tile_is_water,
};
use crate::world::state::{TileRef, World};
use crate::world::tiles::{
    get_world_pos_for_world_index, tile_at_world_index_loaded, tile_at_world_position, tile_at_world_position_loaded,
};

// =====================================================================================================
// Shared support for the three hostile NPC classes
// =====================================================================================================

/// `CUSTOM_RULE_NPC_HARD` (GameConstants.h:1133).
pub const CUSTOM_RULE_NPC_HARD: i8 = 0;
/// `CUSTOM_RULE_NPC_NORMAL`
pub const CUSTOM_RULE_NPC_NORMAL: i8 = 1;
/// `CUSTOM_RULE_NPC_EASY`
pub const CUSTOM_RULE_NPC_EASY: i8 = 2;
/// `CUSTOM_RULE_NPC_PASSIVE`
pub const CUSTOM_RULE_NPC_PASSIVE: i8 = 3;
/// `SE_DAMAGE` (SimulationEvent 4).
pub const SE_DAMAGE: i32 = 4;

/// `ITEM_CAGE`
pub const ITEM_CAGE: i32 = 302;

crate::hooks::hook_trait! {
/// Seam to layers that did not exist when `npc_hostile` was written. All methods have defaults; the `World` and
/// `Blockhead` ones are implemented on the concrete types, the `DynamicWorld` ones are inert (they return the
/// "nothing there" answer).
#[allow(unused_variables)]
pub trait HostileHost, stub HostileHostStub {
    // ---- World ----
    /// The concrete `World` (`[self world]`); `None` when there is none (tests).
    #[provided]
    fn world(&self) -> Option<Rc<World>> {
        npc_env().world()
    }
    /// `[world customRules]` by value (zeroed if the world is gone).
    #[provided]
    fn custom_rules(&self) -> CustomRules {
        self.world().map(|w| w.custom_rules()).unwrap_or_default()
    }
    /// `[world expertMode]`
    #[provided]
    fn expert_mode(&self) -> bool {
        self.world().map_or(false, |w| w.expert_mode())
    }
    /// `[world tutorialActive]`
    #[provided]
    fn tutorial_active(&self) -> bool {
        self.world().map_or(false, |w| w.tutorial_active())
    }
    /// `[world getDayNightFractionForX:atWorldTime:]`; 0 when there is no world.
    #[provided]
    fn day_night_fraction(&self, x: f32, t: f64) -> f32 {
        self.world().map_or(0.0, |w| w.get_day_night_fraction_for_x_at_world_time(x, t))
    }
    /// `[world fireProjectileFrom:to:at:fireItemType:firer:]`.
    #[provided]
    fn fire_projectile(&self, from: Vector2, to: Vector2, at: Option<&ObjRef>, item: i32, firer: Option<&ObjRef>) {
        if let Some(w) = self.world() {
            w.fire_projectile_from_to_at_fire_item_type_firer(from, to, at, item, firer);
        }
    }
    /// `[world addSimulationEventOfType:SE_DAMAGE forBlockhead:b extraData:text]` (only when `isSimulation`).
    #[provided]
    fn add_damage_simulation_event(&self, b: &ObjRef, text: &str) {
        if let Some(w) = self.world() {
            w.add_simulation_event_of_type_for_blockhead_extra_data(
                SE_DAMAGE,
                Some(b),
                crate::world::simulation::SimulationEventExtra::Text(text),
            );
        }
    }
    // ---- DynamicWorld (inert defaults) ----
    /// `[dynamicWorld npcExistsAtPos:ignoreNPC:]`.
    fn npc_exists_at_pos_ignore_npc(&self, pos: IntPair, ignore: Option<&ObjRef>) -> bool {
        false
    }
    /// `[dynamicWorld blockheadOccupiesTileAtPos:ignoreBlockhead:nil]`
    fn blockhead_occupies_tile_at_pos(&self, pos: IntPair) -> bool {
        false
    }
    /// `[dynamicWorld createFreeBlockAtPosition:pos ofType:item dataA:0 dataB:0 subItems:nil dynamicObjectSaveDict:nil
    /// hovers:NO playSound:YES priorityBlockhead:b]`.
    fn create_free_block(&self, pos: IntPair, item_type: i32, priority_blockhead: Option<&ObjRef>) {}
    /// `[dynamicWorld loadNPCAtPosition:pos type:t saveDict:nil isAdult:NO wasPlaced:NO placedByClient:nil]`.
    fn load_baby_npc_at_position(&self, pos: IntPair, npc_type: i32) {}
    /// `[tree availableFood]` when `[dynamicWorld treeAtPos:pos]` is a `CactusTree`, else `None`.
    fn cactus_available_food_at(&self, pos: IntPair) -> Option<f32> {
        None
    }
    /// `[tree setAvailableFood:v]` for the cactus at `pos`.
    fn set_cactus_available_food_at(&self, pos: IntPair, v: f32) {}
    // ---- Blockhead ----
    /// `[b paused]`
    #[provided]
    fn bh_paused(&self, b: &ObjRef) -> bool {
        as_blockhead(b).map_or(false, |x| x.paused())
    }
    /// `[b regenerating]`
    #[provided]
    fn bh_regenerating(&self, b: &ObjRef) -> bool {
        as_blockhead(b).map_or(false, |x| x.regenerating())
    }
    /// `[b onTradeMission]`
    #[provided]
    fn bh_on_trade_mission(&self, b: &ObjRef) -> bool {
        as_blockhead(b).map_or(false, |x| x.on_trade_mission())
    }
    /// `[b shouldContinueSimulating]`
    #[provided]
    fn bh_should_continue_simulating(&self, b: &ObjRef) -> bool {
        as_blockhead(b).map_or(false, |x| x.should_continue_simulating())
    }
    /// `[b sufferDamage:dmg isSimulation:sim recoil:recoil]`.
    #[provided]
    fn bh_suffer_damage(&self, b: &ObjRef, dmg: f32, sim: bool, recoil: bool) {
        if let Some(x) = as_blockhead(b) {
            x.suffer_damage_is_simulation_recoil(dmg, sim, recoil);
        }
    }
    /// `[b center]`
    #[provided]
    fn bh_center(&self, b: &ObjRef) -> Vector2 {
        match b.as_harmable() {
            Some(h) => h.center(),
            None => b.float_pos(),
        }
    }
}
}

struct DefaultHost;
impl HostileHostStub for DefaultHost {}

/// [`HostileHost`] backed by the real `DynamicWorld` (dynamic_world_ops): the integrator installs
/// `set_hostile_host(Rc::new(DynamicWorldHost(dw)))`.
pub struct DynamicWorldHost(pub Rc<crate::dynworld::state::DynamicWorld>);

impl HostileHost for DynamicWorldHost {
    fn npc_exists_at_pos_ignore_npc(&self, pos: IntPair, ignore: Option<&ObjRef>) -> bool {
        self.0.npc_exists_at_pos_ignore_npc(pos, ignore)
    }
    fn blockhead_occupies_tile_at_pos(&self, pos: IntPair) -> bool {
        self.0.blockhead_occupies_tile_at_pos_ignore_blockhead(pos, None)
    }
    fn create_free_block(&self, pos: IntPair, item_type: i32, priority_blockhead: Option<&ObjRef>) {
        self.0.create_free_block_at_position_of_type_data_a_data_b_sub_items_dynamic_object_save_dict_hovers_play_sound_priority_blockhead(
            pos,
            crate::types::item_type::ItemType(item_type as u32),
            0,
            0,
            None,
            None,
            false,
            1,
            priority_blockhead,
        );
    }
    fn load_baby_npc_at_position(&self, pos: IntPair, npc_type: i32) {
        let _ = self.0.load_npc_at_position_type_save_dict_is_adult_was_placed_placed_by_client(
            pos, npc_type, None, false, false, None,
        );
    }
    /// `tree = [dynamicWorld treeAtPos:pos]; if [tree isKindOfClass:[CactusTree class]] -> [tree availableFood]`.
    fn cactus_available_food_at(&self, pos: IntPair) -> Option<f32> {
        let tree = self.0.tree_at_pos(pos)?;
        crate::objects::handle::downcast::<crate::trees::cactus::CactusTree>(&tree).map(|c| c.available_food())
    }
    /// `[tree setAvailableFood:v]` on the cactus at `pos`.
    fn set_cactus_available_food_at(&self, pos: IntPair, v: f32) {
        if let Some(tree) = self.0.tree_at_pos(pos) {
            if let Some(c) = crate::objects::handle::downcast::<crate::trees::cactus::CactusTree>(&tree) {
                c.set_available_food(v);
            }
        }
    }
}

thread_local! {
    static HOST: RefCell<Rc<dyn HostileHost>> = RefCell::new(Rc::new(DefaultHost));
}

/// Installs the host seam (once at startup).
pub fn set_hostile_host(h: Rc<dyn HostileHost>) {
    HOST.with(|c| *c.borrow_mut() = h);
}

/// The installed host (an inert default when none was installed).
pub fn hostile_host() -> Rc<dyn HostileHost> {
    HOST.with(|c| c.borrow().clone())
}

/// `rand()` converted `(float)rand() / 2147483648.0f`.
#[inline]
pub(crate) fn r01(tag: &'static str) -> f32 {
    glibc_rand::r01_tagged(tag)
}

/// `tileAtWorldPositionLoaded(x, y, world)` (may be NULL).
pub(crate) fn tile_loaded(x: i32, y: i32) -> Option<TileRef> {
    let w = hostile_host().world()?;
    tile_at_world_position_loaded(x, y, &w)
}

/// `tileAtWorldPosition(x, y, [world macroTiles], world)`: never loads.
pub(crate) fn tile_unloaded(x: i32, y: i32) -> Option<TileRef> {
    let w = hostile_host().world()?;
    let mt = w.macro_tiles();
    tile_at_world_position(x, y, &mt, &w)
}

/// `tileIsClimable(tile, pos, world, nil)`; NULL tile / no world gives NO.
pub(crate) fn climable(t: Option<TileRef>, pos: IntPair) -> bool {
    match hostile_host().world() {
        Some(w) => tile_is_climable(t, pos, &w, None),
        None => false,
    }
}

/// `wrapDX(a, b)`: wrapped horizontal difference `a - b`.
pub(crate) fn wrap_dx(a: i32, b: i32, world_width_macro: i32) -> i32 {
    let d = a.wrapping_sub(b);
    let w = world_width_macro.wrapping_mul(32);
    if d < w / 2 {
        if d < world_width_macro.wrapping_mul(-32) / 2 {
            d.wrapping_add(w)
        } else {
            d
        }
    } else {
        d.wrapping_sub(w)
    }
}

/// `[world worldWidthMacro]` of `me` (0 when the world is gone).
pub(crate) fn world_width_macro(me: &ObjRef) -> i32 {
    me.world().map_or(0, |w| w.world_width_macro())
}

/// `[world worldTime]` (0 when the world is gone).
pub(crate) fn world_time(me: &ObjRef) -> f64 {
    me.world().map_or(0.0, |w| w.world_time())
}

/// C pointer identity of two `Tile*` (both NULL compare equal).
pub(crate) fn tile_eq(a: &Option<TileRef>, b: &Option<TileRef>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(a), Some(b)) => {
            std::ptr::eq(a.physical_block(), b.physical_block()) && a.local_index() == b.local_index()
        }
        _ => false,
    }
}

/// `Tile` copy of an optional handle.
pub(crate) fn tile_copy(t: &Option<TileRef>) -> Option<Tile> {
    t.map(|t| t.get())
}

/// `floatPos = (x, y)` on the base ivars.
pub(crate) fn set_float_pos(me: &ObjRef, x: f32, y: f32) {
    me.base().borrow_mut().float_pos = Vector2 { x, y };
}

/// `floatPos = ((float)sq.x + 0.5f, (float)sq.y)` (the arrival / snap formula).
pub(crate) fn snap_float_pos(me: &ObjRef, sq: IntPair) {
    set_float_pos(me, (sq.x as f32) + 0.5f32, sq.y as f32);
}

/// `[a == b]` handle identity of two optional handles (`nil == nil` is true).
#[allow(dead_code)]
pub(crate) fn opt_same(a: &Option<ObjRef>, b: &Option<ObjRef>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(a), Some(b)) => ptr_eq(a, b),
        _ => false,
    }
}

/// `x == obj` for an optional handle (`nil == obj` is false).
pub(crate) fn is_same(a: &Option<ObjRef>, b: &ObjRef) -> bool {
    a.as_ref().map_or(false, |a| ptr_eq(a, b))
}

/// Non-empty `[bh pos]` of an optional handle (nil gives (0,0)).
pub(crate) fn pos_of(b: &Option<ObjRef>) -> IntPair {
    b.as_ref().map_or(IntPair { x: 0, y: 0 }, |b| b.pos())
}

/// NSNumber-style `intValue` of a save dict key (0 if missing).
pub(crate) fn dict_int(d: &PlistDict, k: &str) -> i32 {
    d.map.get(&k.to_string()).map_or(0, |v| v.int_value())
}

/// NSNumber-style `floatValue` of a save dict key (0 if missing).
pub(crate) fn dict_float(d: &PlistDict, k: &str) -> f32 {
    d.map.get(&k.to_string()).map_or(0.0, |v| v.float_value())
}

/// NSNumber-style `boolValue` of a save dict key (NO if missing).
pub(crate) fn dict_bool(d: &PlistDict, k: &str) -> bool {
    d.map.get(&k.to_string()).map_or(false, |v| v.bool_value())
}

/// Applies the custom-rules damage scaling shared by the troll punch and the scorpion bite:
/// `if enabled { if difficulty == EASY { (float)((double)dmg * 0.25) } else if difficulty == HARD { 4.0f * dmg } }`.
pub(crate) fn scale_damage_for_difficulty(dmg: f32, cr: &CustomRules) -> f32 {
    let mut d = dmg;
    if cr.custom_rules_enabled != 0 {
        if cr.npc_difficulty == CUSTOM_RULE_NPC_EASY {
            d = ((d as f64) * 0.25f64) as f32;
        } else if cr.npc_difficulty == CUSTOM_RULE_NPC_HARD {
            d = 4.0f32 * d;
        }
    }
    d
}

// =====================================================================================================
// Constants and enums
// =====================================================================================================

/// `CaveTrollAnimationType` values used.
pub mod cta {
    pub const STAND: u32 = 0;
    pub const WALK: u32 = 1;
    pub const STAND_TO_HANG_JUMP: u32 = 2;
    pub const HANG_TO_STAND_JUMP: u32 = 3;
    pub const CLIMB: u32 = 4;
    pub const SWIM: u32 = 5;
    pub const CRAWL: u32 = 6;
    pub const HANG: u32 = 7;
    pub const HANG_FALL_BALANCE: u32 = 8;
    pub const CROUCH_UP: u32 = 9;
    pub const CROUCH_DOWN: u32 = 10;
    pub const CROUCH_CRAWL_UP: u32 = 11;
    pub const CROUCH_CRAWL_DOWN: u32 = 12;
    pub const MONKEY_SWING: u32 = 13;
    pub const MONKEY_HANG: u32 = 14;
    pub const STAND_TO_HANG_DOUBLE_JUMP: u32 = 15;
    pub const STAND_TO_MONKEY_HANG_JUMP: u32 = 16;
    pub const MONKEY_HANG_TO_STAND_JUMP: u32 = 17;
    pub const CLIMB_DOWN_TO_MONKEY_SWING: u32 = 18;
    pub const MONKEY_SWING_UP_TO_CLIMB: u32 = 19;
    pub const HANG_TO_MONKEY_SWING_DIAGONAL_JUMP_UP: u32 = 20;
    pub const MONKEY_SWING_TO_HANG_DIAGONAL_JUMP_DOWN: u32 = 21;
    pub const HANG_TO_HANG_JUMP: u32 = 22;
    pub const LIE: u32 = 23;
    pub const BOB: u32 = 24;
    pub const FALL: u32 = 25;
    pub const JUMP_OVER_GAP: u32 = 26;
    pub const RIDE: u32 = 27;
}

/// `CaveTrollSubAnimationType` values used.
pub mod ctsa {
    pub const NONE: u32 = 0;
    pub const STAB: u32 = 3;
    pub const LOOK_LEFT_RIGHT_UP: u32 = 4;
}

/// `TileTraverseType` values used.
pub mod tt {
    pub const NONE: i32 = 0;
    pub const WALK: i32 = 1;
    pub const SWIM: i32 = 2;
    pub const CRAWL: i32 = 3;
    pub const CLIMB: i32 = 4;
    pub const STAND_TO_HANG_JUMP: i32 = 5;
    pub const HANG_TO_STAND_JUMP: i32 = 6;
    pub const HANG_TO_HANG_JUMP: i32 = 7;
    pub const STAND_TO_HANG_DOUBLE_JUMP: i32 = 8;
    pub const STAND_TO_MONKEY_HANG_JUMP: i32 = 11;
    pub const MONKEY_HANG_TO_STAND_JUMP: i32 = 12;
    pub const CROUCH_UP: i32 = 13;
    pub const CROUCH_DOWN: i32 = 14;
    pub const CROUCH_CRAWL_UP: i32 = 15;
    pub const CROUCH_CRAWL_DOWN: i32 = 16;
    pub const FALL: i32 = 17;
    pub const MONKEY_SWING: i32 = 19;
    pub const CLIMB_DOWN_TO_MONKEY_SWING: i32 = 20;
    pub const MONKEY_SWING_UP_TO_CLIMB: i32 = 21;
    pub const HANG_TO_MONKEY_SWING_DIAGONAL_JUMP_UP: i32 = 22;
    pub const MONKEY_SWING_TO_HANG_DIAGONAL_JUMP_DOWN: i32 = 23;
    pub const JUMP_OVER_GAP: i32 = 24;
    pub const INACCESSIBLE: i32 = 25;
}

/// `TileTraverseKeyFrameType` values used.
pub mod kf {
    pub const STAND: i32 = 0;
    pub const BOB: i32 = 1;
    pub const CRAWL: i32 = 2;
    pub const HANG_BACK: i32 = 3;
    pub const HANG_RIGHT: i32 = 4;
    pub const HANG_LEFT: i32 = 5;
    pub const MONKEY_HANG: i32 = 6;
    pub const FALLING: i32 = 7;
    pub const INACCESSIBLE: i32 = 8;
}

/// `InteractionType` values used.
pub const INTERACTION_NONE: u32 = 0;
/// `INTERACTION_STAB`
pub const INTERACTION_STAB: u32 = 10;
/// `INTERACTION_RIDE`
pub const INTERACTION_RIDE: u32 = 12;

/// `PathType` values used by `setPath:type:...`.
pub mod path_type {
    pub const WORLD_TAPPED: i32 = 0;
    pub const WORLD_TAPPED_MOVE_ONLY: i32 = 1;
    pub const RECALCULATED_DUE_TO_CHANGE: i32 = 2;
    pub const NPC_CHASE: i32 = 3;
    pub const RIDE_OBJECT_CHASE: i32 = 4;
    pub const FALL: i32 = 5;
}

/// `ITEM_GOLD_COIN`
pub const ITEM_GOLD_COIN: i32 = 167;
/// `ITEM_ICE_PANTS`, `ITEM_ICE_CHEST_PLATE`, `ITEM_ICE_HELMET`, `ITEM_ICE_BOOTS`.
pub const ITEM_ICE_PANTS: i32 = 195;
pub const ITEM_ICE_CHEST_PLATE: i32 = 196;
pub const ITEM_ICE_HELMET: i32 = 197;
pub const ITEM_ICE_BOOTS: i32 = 198;

/// `CaveTroll: NPC <PathUserDynamicObject>`.
pub static CAVE_TROLL_CLASS: ClassInfo =
    ClassInfo { name: "CaveTroll", superclass: Some(&NPC_CLASS), protocols: &["PathUserDynamicObject"] };

/// `CAVE_TROLL_NAMES` (35 entries, "Herculies" spelling kept).
pub const CAVE_TROLL_NAMES: [&str; 35] = [
    "Trollface",
    "Blue",
    "Cyclops",
    "Monocle",
    "Grumpy",
    "Wolf",
    "Bert",
    "Fred",
    "Rocky",
    "Fin",
    "Ernest",
    "Hugo",
    "Scrag",
    "Gollum",
    "Shrek",
    "Doc",
    "Happy",
    "Sleepy",
    "Dopey",
    "Bashful",
    "Sneezy",
    "Grendel",
    "Dunker",
    "Ymer",
    "Dovregubben",
    "Hrungnir",
    "Trym",
    "Shaggy",
    "Goldy",
    "Bear",
    "Ted",
    "Womble",
    "Monsta",
    "Kitty",
    "Herculies",
];

/// `PI` as used by the troll `floatPos` formulas (f64).
const PI_D: f64 = 3.141592653589793;

// =====================================================================================================
// State
// =====================================================================================================

/// The own ivars of `CaveTroll` that are saved, sent, or read by `update:` (render-only ones are omitted).
#[derive(Clone)]
pub struct CaveTrollData {
    /// 0x118 `state` (raw saved, 0x24 bytes)
    pub state: CaveTrollState,
    /// 0x190
    pub walk_timer: f32,
    /// 0x1a8: the LAST element is the NEXT step, `[0]` is the goal
    pub path: Option<Vec<PathStep>>,
    pub travel_speed: f32,
    pub traverse_type: i32,
    pub traverse_from_key_frame: i32,
    pub traverse_to_key_frame: i32,
    pub terrain_difficulty: i32,
    pub from_square: IntPair,
    pub to_square: IntPair,
    pub final_goal_square: IntPair,
    pub defend_square: IntPair,
    pub return_to_defense: bool,
    pub cannot_return: bool,
    pub from_tile: Option<TileRef>,
    pub to_tile: Option<TileRef>,
    pub interacting_tile: Option<TileRef>,
    pub last_path_was_falling: bool,
    pub last_path_was_move_only: bool,
    pub tapped_npc: Option<ObjRef>,
    pub chasing_npc: bool,
    pub last_known_npc_position: IntPair,
    pub travel_fraction: f32,
    pub travel_fraction_normalized: f32,
    pub path_needs_recalculated: bool,
    /// never written
    pub path_recalculation_is_fall_path: bool,
    pub idle_timer: f32,
    pub random_idle_wait: f32,
    pub random_animation_value_a: f32,
    pub random_animation_value_b: f32,
    pub random_animation_value_c: f32,
    /// 0x280: ALWAYS ZERO on the server (R1)
    pub mhead_matrix: [f32; 16],
    pub waiting_for_path: bool,
    pub waiting_path_goal_pos: IntPair,
    pub footstep_ping_pong: bool,
    pub path_extra_data: Option<PlistValue>,
    pub recoil_timer: f32,
    pub recoil_direction: f32,
    pub random_ai_wait: f32,
    pub waiting_before_retaliation_blockhead: Option<ObjRef>,
    pub was_lit: bool,
    /// INITIAL (1.0, 1.0) (R2)
    pub rider_target_velocity: Vector2,
    /// INITIAL (1,1,1,1); render-only
    pub rider_position: Vector,
    pub current_movement_was_initiated_by_rider: bool,
    pub current_net_queue_size: i32,
    pub next_to_square: [IntPair; 4],
    pub next_terrain_difficulty: [i32; 4],
    pub next_traverse_type: [i32; 4],
    pub next_traverse_to_key_frame: [i32; 4],
    pub next_interacting: [bool; 4],
    pub next_interaction_square: [IntPair; 4],
    pub next_interaction_type: [i32; 4],
    pub next_animation_type: [i32; 4],
    pub next_sub_animation_type: [i32; 4],
    pub has_had_remote_update: bool,
    pub no_remote_action_timer: f32,
}

impl CaveTrollData {
    /// The zeroed-ivar state after `alloc`, plus the `.cxx_construct` defaults (Vector2 / Vector are (1,1[,1,1])).
    pub fn new() -> CaveTrollData {
        let z = IntPair { x: 0, y: 0 };
        CaveTrollData {
            state: CaveTrollState::default(),
            walk_timer: 0.0,
            path: None,
            travel_speed: 0.0,
            traverse_type: 0,
            traverse_from_key_frame: 0,
            traverse_to_key_frame: 0,
            terrain_difficulty: 0,
            from_square: z,
            to_square: z,
            final_goal_square: z,
            defend_square: z,
            return_to_defense: false,
            cannot_return: false,
            from_tile: None,
            to_tile: None,
            interacting_tile: None,
            last_path_was_falling: false,
            last_path_was_move_only: false,
            tapped_npc: None,
            chasing_npc: false,
            last_known_npc_position: z,
            travel_fraction: 0.0,
            travel_fraction_normalized: 0.0,
            path_needs_recalculated: false,
            path_recalculation_is_fall_path: false,
            idle_timer: 0.0,
            random_idle_wait: 0.0,
            random_animation_value_a: 0.0,
            random_animation_value_b: 0.0,
            random_animation_value_c: 0.0,
            mhead_matrix: [0.0; 16],
            waiting_for_path: false,
            waiting_path_goal_pos: z,
            footstep_ping_pong: false,
            path_extra_data: None,
            recoil_timer: 0.0,
            recoil_direction: 0.0,
            random_ai_wait: 0.0,
            waiting_before_retaliation_blockhead: None,
            was_lit: false,
            rider_target_velocity: Vector2::cxx_new(),
            rider_position: Vector::cxx_new(),
            current_movement_was_initiated_by_rider: false,
            current_net_queue_size: 0,
            next_to_square: [z; 4],
            next_terrain_difficulty: [0; 4],
            next_traverse_type: [0; 4],
            next_traverse_to_key_frame: [0; 4],
            next_interacting: [false; 4],
            next_interaction_square: [z; 4],
            next_interaction_type: [0; 4],
            next_animation_type: [0; 4],
            next_sub_animation_type: [0; 4],
            has_had_remote_update: false,
            no_remote_action_timer: 0.0,
        }
    }
}

/// `CaveTroll`.
pub struct CaveTroll {
    base: RefCell<DynamicObjectBase>,
    npc: RefCell<NpcState>,
    troll: RefCell<CaveTrollData>,
}

/// `TileTraverseKeyFrameType` to `CaveTrollAnimationType` for a resting troll: STAND, BOB(1)->BOB, HANG_*(3,4,5)->HANG,
/// CRAWL(2)->LIE, MONKEY_HANG(6)->MONKEY_HANG, anything else STAND.
pub fn animation_for_key_frame(kf_: i32) -> u32 {
    match kf_ {
        kf::BOB => cta::BOB,
        kf::HANG_BACK | kf::HANG_RIGHT | kf::HANG_LEFT => cta::HANG,
        kf::CRAWL => cta::LIE,
        kf::MONKEY_HANG => cta::MONKEY_HANG,
        _ => cta::STAND,
    }
}

/// The `traverseType` to `animationType` table of the ARRIVAL block; `None` = "animationType unchanged".
pub fn animation_for_traverse(traverse_type: i32, travel_speed: f32) -> Option<u32> {
    Some(match traverse_type {
        tt::WALK => cta::WALK,
        tt::CLIMB => cta::CLIMB,
        tt::JUMP_OVER_GAP => cta::JUMP_OVER_GAP,
        tt::CRAWL => cta::CRAWL,
        tt::STAND_TO_HANG_JUMP => cta::STAND_TO_HANG_JUMP,
        tt::HANG_TO_STAND_JUMP => cta::HANG_TO_STAND_JUMP,
        tt::CROUCH_UP => cta::CROUCH_UP,
        tt::CROUCH_DOWN => cta::CROUCH_DOWN,
        tt::CROUCH_CRAWL_UP => cta::CROUCH_CRAWL_UP,
        tt::CROUCH_CRAWL_DOWN => cta::CROUCH_CRAWL_DOWN,
        tt::MONKEY_SWING => cta::MONKEY_SWING,
        tt::STAND_TO_HANG_DOUBLE_JUMP => cta::STAND_TO_HANG_DOUBLE_JUMP,
        tt::STAND_TO_MONKEY_HANG_JUMP => cta::STAND_TO_MONKEY_HANG_JUMP,
        tt::MONKEY_HANG_TO_STAND_JUMP => cta::MONKEY_HANG_TO_STAND_JUMP,
        tt::CLIMB_DOWN_TO_MONKEY_SWING => cta::CLIMB_DOWN_TO_MONKEY_SWING,
        tt::MONKEY_SWING_UP_TO_CLIMB => cta::MONKEY_SWING_UP_TO_CLIMB,
        tt::HANG_TO_MONKEY_SWING_DIAGONAL_JUMP_UP => cta::HANG_TO_MONKEY_SWING_DIAGONAL_JUMP_UP,
        tt::MONKEY_SWING_TO_HANG_DIAGONAL_JUMP_DOWN => cta::MONKEY_SWING_TO_HANG_DIAGONAL_JUMP_DOWN,
        tt::FALL => {
            if travel_speed <= 4.0f32 {
                cta::STAND
            } else {
                cta::FALL
            }
        }
        tt::HANG_TO_HANG_JUMP => cta::HANG_TO_HANG_JUMP,
        tt::SWIM => cta::SWIM,
        _ => return None,
    })
}

/// Converts the pathfinder's plist step array into [`PathStep`]s (only `index`, `traverseType`,
/// `traverseToKeyFrame`, `terrainDifficulty` are read).
pub fn path_steps_from_plist(path: &[PlistValue]) -> Vec<PathStep> {
    path.iter()
        .map(|v| {
            let g = |k: &str| v.object_for_key(k).map_or(0, |x| x.int_value());
            PathStep {
                index: g("index"),
                terrain_difficulty: g("terrainDifficulty"),
                traverse_type: g("traverseType"),
                traverse_to_key_frame: g("traverseToKeyFrame"),
                parent_index: None,
                properties_g: None,
            }
        })
        .collect()
}

impl CaveTroll {
    /// `[CaveTroll alloc]` with the zeroed ivars and `.cxx_construct` defaults.
    fn alloc() -> Rc<CaveTroll> {
        alloc_npc(|w| CaveTroll {
            base: RefCell::new(DynamicObjectBase::alloc_zeroed_with(w)),
            npc: RefCell::new(NpcState::default()),
            troll: RefCell::new(CaveTrollData::new()),
        })
    }

    /// Read access to the troll ivars.
    pub fn data(&self) -> std::cell::Ref<'_, CaveTrollData> {
        self.troll.borrow()
    }

    /// Write access to the troll ivars (callers must not hold it across calls into other objects).
    pub fn data_mut(&self) -> std::cell::RefMut<'_, CaveTrollData> {
        self.troll.borrow_mut()
    }

    fn t<R>(&self, f: impl FnOnce(&CaveTrollData) -> R) -> R {
        f(&self.troll.borrow())
    }

    fn tm(&self, f: impl FnOnce(&mut CaveTrollData)) {
        f(&mut self.troll.borrow_mut())
    }

    // ------------------------------------------------------------------------------------ initialisers

    /// `-initWithWorld:dynamicWorld:atPosition:cache:saveDict:isAdult:wasPlaced:placedByClient:`.
    #[allow(clippy::too_many_arguments)]
    pub fn init_with_world_dynamic_world_at_position_cache_save_dict_is_adult_was_placed_placed_by_client(
        world: &WorldRef,
        dw: &DynamicWorldRef,
        pos: IntPair,
        cache: CacheRef,
        save_dict: Option<&PlistDict>,
        is_adult: bool,
        was_placed: bool,
        placed_by_client: Option<&str>,
    ) -> Option<Rc<CaveTroll>> {
        let o = CaveTroll::alloc();
        let me: ObjRef = o.clone();
        let inited = npc_init_with_world_dynamic_world_at_position_cache_save_dict_is_adult_was_placed_placed_by_client(
            &me,
            world,
            dw,
            pos,
            cache,
            save_dict,
            is_adult,
            was_placed,
            placed_by_client,
        );
        debug_assert!(inited); // the base initialisers cannot fail on this path
        let tile = tile_loaded(pos.x, pos.y);
        o.tm(|t| {
            t.to_square = pos;
            t.from_square = t.to_square;
            t.travel_fraction = 1.0f32;
            t.travel_speed = 4.0f32;
            t.from_tile = tile;
            t.to_tile = tile;
            t.defend_square = t.from_square;
        });
        o.init_sub_derived_stuff_stuff();
        Some(o)
    }

    /// `-initWithWorld:dynamicWorld:saveDict:cache:`.
    pub fn init_with_world_dynamic_world_save_dict_cache(
        world: &WorldRef,
        dw: &DynamicWorldRef,
        save_dict: &PlistDict,
        cache: CacheRef,
    ) -> Option<Rc<CaveTroll>> {
        let o = CaveTroll::alloc();
        let me: ObjRef = o.clone();
        let inited = npc_init_with_world_dynamic_world_save_dict_cache(&me, world, dw, save_dict, cache);
        debug_assert!(inited); // the base initialisers cannot fail on this path
        let pos = me.pos();
        let tile = tile_loaded(pos.x, pos.y);
        o.tm(|t| {
            t.to_square = pos;
            t.from_square = t.to_square;
            t.travel_fraction = 1.0f32;
            t.travel_speed = 4.0f32;
            t.from_tile = tile;
            t.to_tile = tile;
            t.defend_square.x = dict_int(save_dict, "defendSquare.x");
            t.defend_square.y = dict_int(save_dict, "defendSquare.y");
        });
        if let Some(PlistValue::Data(d)) = save_dict.map.get(&"state".to_string()) {
            // memcpy(&state, bytes, length): the original does not clamp; Rust clamps to 0x24. UB-PARITY.
            let mut raw = o.t(|t| t.state.to_vec());
            let n = d.bytes.len().min(raw.len());
            raw[..n].copy_from_slice(&d.bytes[..n]);
            o.tm(|t| t.state = CaveTrollState::read_le(&raw));
        }
        let fp = me.float_pos();
        if f32_to_i32(fp.x) != pos.x || f32_to_i32(fp.y) != pos.y {
            set_float_pos(&me, 0.5f32 + (pos.x as f32), pos.y as f32);
        }
        o.npc.borrow_mut().dead = dict_bool(save_dict, "dead");
        let st = o.t(|t| t.state);
        if st.interacting != 0 {
            let it = tile_loaded(st.interacting_square.x, st.interacting_square.y);
            o.tm(|t| t.interacting_tile = it);
        }
        o.init_sub_derived_stuff_stuff();
        Some(o)
    }

    /// `-initWithWorld:dynamicWorld:cache:netData:`: the client replica path; the server never calls it
    /// except through a remote `remoteCreate`. Short data is zero padded.
    pub fn init_with_world_dynamic_world_cache_net_data(
        world: &WorldRef,
        dw: &DynamicWorldRef,
        cache: CacheRef,
        net_data: &[u8],
    ) -> Option<Rc<CaveTroll>> {
        let o = CaveTroll::alloc();
        let me: ObjRef = o.clone();
        if !npc_init_with_world_dynamic_world_cache_net_data(&me, world, dw, cache, net_data) {
            return None;
        }
        let pos = me.pos();
        let tile = tile_loaded(pos.x, pos.y);
        o.tm(|t| {
            t.to_square = pos;
            t.from_square = t.to_square;
            t.travel_fraction = 1.0f32;
            t.travel_speed = 4.0f32;
            for i in 0..4 {
                t.next_to_square[i] = t.from_square;
                t.next_terrain_difficulty[i] = 10;
                t.next_animation_type[i] = t.state.animation_type as i32;
                t.next_sub_animation_type[i] = t.state.sub_animation_type as i32;
            }
            t.from_tile = tile;
            t.to_tile = tile;
            if let Some(ft) = tile {
                if ft.get().light > 0x7f {
                    t.was_lit = true;
                }
            }
        });
        o.init_sub_derived_stuff_stuff();
        o.remote_creation_data_update(&me, net_data);
        Some(o)
    }

    /// `-initSubDerivedStuffStuff`: the only server-visible effect is one `rand()`:
    /// `randomIdleWait = R * 10.0f + 5.0f`.
    pub fn init_sub_derived_stuff_stuff(&self) {
        let v = r01("CaveTroll.initSubDerivedStuffStuff") * 10.0f32 + 5.0f32;
        self.tm(|t| t.random_idle_wait = v);
    }

    // ------------------------------------------------------------------------------------ net / save

    /// `-caveTrollUpdateDataForClient:`. `health` and the padding are written as zero.
    pub fn cave_troll_update_data_for_client(&self, me: &ObjRef, client_id: Option<&str>) -> CaveTrollUpdateData {
        let npc_upd: NPCUpdateNetData = Npc::npc_update_net_data_for_client(self, me, client_id);
        let dead = self.npc.borrow().dead;
        let t = self.troll.borrow();
        let mut u = CaveTrollUpdateData::default();
        u.dynamic_object_net_data = npc_upd;
        u.from_square_x = t.from_square.x as u32;
        u.from_square_y = t.from_square.y as u32;
        u.to_square_x = t.to_square.x as u32;
        u.to_square_y = t.to_square.y as u32;
        u.interacting_square_x = t.state.interacting_square.x as u32;
        u.interacting_square_y = t.state.interacting_square.y as u32;
        u.traverse_type = t.traverse_type as u8;
        u.traverse_from_key_frame = t.traverse_from_key_frame as u8;
        u.traverse_to_key_frame = t.traverse_to_key_frame as u8;
        u.dead = dead as u8;
        u.terrain_difficulty = t.terrain_difficulty as u16;
        u.interacting = t.state.interacting;
        u.interaction_type = t.state.interaction_type as u8;
        u.animation_type = t.state.animation_type as u8;
        u.sub_animation_type = t.state.sub_animation_type as u8;
        u
    }

    /// `-doRemoteUpdate:`: applies the data of a remote controller (queue of at most 4).
    pub fn do_remote_update(&self, _me: &ObjRef, u: &CaveTrollUpdateData) {
        let mut t = self.troll.borrow_mut();
        let mut idx = t.current_net_queue_size;
        if idx > 3 {
            t.current_net_queue_size = 3;
            idx = 3;
        }
        let to = make_intpair(u.to_square_x as i32, u.to_square_y as i32);
        let isq = make_intpair(u.interacting_square_x as i32, u.interacting_square_y as i32);
        if !t.has_had_remote_update {
            t.to_square = to;
            t.terrain_difficulty = u.terrain_difficulty as i32;
            t.traverse_type = u.traverse_type as i32;
            t.traverse_to_key_frame = u.traverse_to_key_frame as i32;
            t.state.interacting = u.interacting;
            t.state.interaction_type = u.interaction_type as u32;
            t.state.animation_type = u.animation_type as u32;
            t.state.sub_animation_type = u.sub_animation_type as u32;
            t.state.interacting_square = isq;
            t.has_had_remote_update = true;
        } else {
            t.current_net_queue_size += 1;
            let i = idx as usize;
            t.next_to_square[i] = to;
            t.next_terrain_difficulty[i] = u.terrain_difficulty as i32;
            t.next_traverse_type[i] = u.traverse_type as i32;
            t.next_traverse_to_key_frame[i] = u.traverse_to_key_frame as i32;
            t.next_interacting[i] = u.interacting != 0;
            t.next_interaction_type[i] = u.interaction_type as i32;
            t.next_animation_type[i] = u.animation_type as i32;
            t.next_sub_animation_type[i] = u.sub_animation_type as i32;
            t.next_interaction_square[i] = isq;
        }
    }

    /// `-controlIsLocal`: `rider == nil ? !isNet: !rider.isNet`.
    pub fn control_is_local(&self) -> bool {
        let rider = self.npc.borrow().rider.clone();
        match rider {
            None => !self.base.borrow().is_net,
            Some(r) => !r.is_net(),
        }
    }

    /// `-falling`: `[path count] != 0 && traverseType == TRAVERSE_FALL`.
    pub fn falling(&self) -> bool {
        self.t(|t| t.path.as_ref().map_or(0, |p| p.len()) != 0 && t.traverse_type == tt::FALL)
    }

    /// `-crouching`: LIE or CRAWL animation.
    pub fn crouching(&self) -> bool {
        self.t(|t| t.state.animation_type == cta::LIE || t.state.animation_type == cta::CRAWL)
    }

    /// `-currentAnimationType`
    pub fn current_animation_type(&self) -> i32 {
        self.t(|t| t.state.animation_type as i32)
    }

    /// `-currentInteractionType`
    pub fn current_interaction_type(&self) -> i32 {
        self.t(|t| t.state.interaction_type as i32)
    }

    /// `-isHeadingForSquare:`
    pub fn is_heading_for_square(&self, sq: IntPair) -> bool {
        let (fin, first) = self.t(|t| (t.final_goal_square, t.path.as_ref().and_then(|p| p.first().copied())));
        if fin == sq {
            return true;
        }
        if let Some(step) = first {
            if let Some(w) = hostile_host().world() {
                return get_world_pos_for_world_index(step.index, &w) == sq;
            }
        }
        false
    }

    /// `-startInteractingWithTileAtIndex:tile:interactionType:`. The sound is not played (server).
    pub fn start_interacting_with_tile_at_index_tile_interaction_type(
        &self,
        _me: &ObjRef,
        tile_index: i32,
        tile: Option<TileRef>,
        ty: i32,
    ) {
        let sq =
            hostile_host().world().map_or(IntPair { x: 0, y: 0 }, |w| get_world_pos_for_world_index(tile_index, &w));
        self.tm(|t| {
            t.state.interaction_timer = 0.0f32;
            t.state.interacting = 1;
            t.state.interacting_square = sq;
            t.interacting_tile = tile;
            t.state.interaction_type = ty as u32;
            t.state.goal_interaction_type_while_walking = t.state.interaction_type;
        });
        if !self.base.borrow().is_net {
            self.base.borrow_mut().update_needs_to_be_sent = true;
        }
        self.update_gather_speed_and_animation_for_current_interation_and_item();
    }

    /// `-stopInteracting`
    pub fn stop_interacting(&self, me: &ObjRef) {
        self.tm(|t| {
            t.state.interacting = 0;
            t.state.interaction_type = 0;
            t.state.goal_interaction_type_while_walking = 0;
            t.state.sub_animation_type = 0;
            t.tapped_npc = None;
            t.chasing_npc = false;
        });
        if !me.is_net() {
            if let Some(dw) = me.dynamic_world() {
                dw.dynamic_world_changed_at_pos_object_type(me.pos(), me.object_type());
            }
            me.set_update_needs_to_be_sent(true);
        }
    }

    /// `-updateGatherSpeedAndAnimationForCurrentInterationAndItem`
    pub fn update_gather_speed_and_animation_for_current_interation_and_item(&self) {
        self.tm(|t| {
            if t.state.interaction_type == INTERACTION_STAB {
                t.state.sub_animation_type = ctsa::STAB;
            }
        });
    }

    /// `-setPath:type:goalInteraction:extraData:`.
    pub fn set_path(
        &self,
        me: &ObjRef,
        path: Option<Vec<PathStep>>,
        ty: i32,
        goal_interaction: i32,
        extra: Option<PlistValue>,
    ) {
        self.tm(|t| {
            t.path_extra_data = extra;
            t.path_needs_recalculated = false;
            t.path = path;
        });
        let (has_path, chasing, ret_def) = self.t(|t| (t.path.is_some(), t.chasing_npc, t.return_to_defense));
        if !has_path && chasing {
            self.stop_interacting(me);
            self.tm(|t| t.path_needs_recalculated = true);
            return;
        }
        if !has_path && ret_def {
            self.tm(|t| t.cannot_return = true);
        }
        self.tm(|t| t.last_path_was_falling = ty == path_type::FALL);
        let count = self.t(|t| t.path.as_ref().map_or(0, |p| p.len()));
        if count == 0 {
            if ty == path_type::WORLD_TAPPED {
                self.tm(|t| t.idle_timer = 0.0f32);
            } else if ty == path_type::NPC_CHASE {
                self.tm(|t| t.idle_timer = 0.0f32);
                self.stop_interacting(me);
            }
        } else {
            self.tm(|t| t.idle_timer = 0.0f32);
            let idx = self.t(|t| t.path.as_ref().unwrap()[0].index);
            let host = hostile_host();
            let xy = match host.world() {
                Some(w) => {
                    let p = get_world_pos_for_world_index(idx, &w);
                    // result unused but forces the goal's macro block to be loaded/created
                    let _ = tile_at_world_position_loaded(p.x, p.y, &w);
                    p
                }
                None => IntPair { x: 0, y: 0 },
            };
            self.tm(|t| t.final_goal_square = xy);
            if matches!(
                ty,
                path_type::WORLD_TAPPED
                    | path_type::WORLD_TAPPED_MOVE_ONLY
                    | path_type::NPC_CHASE
                    | path_type::RIDE_OBJECT_CHASE
            ) {
                if self.t(|t| t.state.interacting) != 0 {
                    self.update_gather_speed_and_animation_for_current_interation_and_item();
                }
                self.tm(|t| {
                    t.state.goal_interaction_type_while_walking = if ty == path_type::WORLD_TAPPED_MOVE_ONLY {
                        INTERACTION_NONE
                    } else {
                        goal_interaction as u32
                    };
                });
            }
        }
    }

    /// `-tapIsWithinBodyRadius:`: client UI hit test, ported for completeness.
    pub fn tap_is_within_body_radius_impl(&self, tap: Vector2) -> bool {
        if self.npc.borrow().dead {
            return false;
        }
        let fp = self.base.borrow().float_pos;
        let wwm = self.base.borrow().world.as_ref().and_then(|w| w.upgrade()).map_or(0, |w| w.world_width_macro());
        let half = ((wwm.wrapping_shl(5)) / 2) as f32;
        let dx = tap.x - fp.x;
        let d = if dx < half {
            if dx >= ((wwm.wrapping_mul(-32)) / 2) as f32 {
                dx
            } else {
                dx + (wwm.wrapping_shl(5)) as f32
            }
        } else {
            dx - (wwm.wrapping_shl(5)) as f32
        };
        if (d as f64) < 0.5f64 && (d as f64) > -0.5f64 {
            return (fp.y as f64) < (tap.y as f64) && (tap.y as f64) < (fp.y as f64) + 1.5f64 + 0.5f64;
        }
        false
    }
}

// =====================================================================================================
// update:accurateDT:isSimulation:
// =====================================================================================================

impl CaveTroll {
    /// `-update:accurateDT:isSimulation:`. Sections A to H are marked in the body.
    fn update_impl(&self, me: &ObjRef, dt: f32, accurate_dt: f32, is_sim: bool) {
        // ---- A. prologue and death handling
        npc_update_accurate_dt_is_simulation(me, dt, accurate_dt, is_sim);
        if me.needs_removed() {
            return;
        }
        let host = hostile_host();
        let dead = self.npc.borrow().dead;
        if dead {
            let tf = self.t(|t| t.travel_fraction) + dt;
            self.tm(|t| t.travel_fraction = tf);
            if me.is_net() {
                return;
            }
            if tf < 1.0f32 {
                return;
            }
            let r = glibc_rand::rand_tagged("CaveTroll.update.drop") % 4;
            let item = match r {
                0 => ITEM_ICE_HELMET,
                1 => ITEM_ICE_PANTS,
                2 => ITEM_ICE_CHEST_PLATE,
                _ => ITEM_ICE_BOOTS,
            };
            let kb = self.npc.borrow().kill_blockhead.clone();
            host.create_free_block(me.pos(), item, kb.as_ref());
            me.set_needs_removed(me, true);
            return;
        }

        // ---- B. local control flag
        let local = self.control_is_local();
        if !local {
            self.tm(|t| t.path_needs_recalculated = false);
        } else {
            // ---- C. local-control prelude
            self.validate_targets();
            if self.t(|t| t.path_needs_recalculated) {
                return;
            }
            if self.t(|t| t.waiting_for_path) {
                return;
            }
            // 9. fire damage
            let from_tile = self.t(|t| t.from_tile);
            if let Some(ft) = from_tile {
                if ft.get().on_fire != 0 {
                    let d = {
                        let mut n = self.npc.borrow_mut();
                        n.damage_delay_timer -= dt;
                        n.damage_delay_timer
                    };
                    if d <= 0.0f32 {
                        Npc::hit_with_force_blockhead(self, me, 4, None);
                        self.npc.borrow_mut().damage_delay_timer = 1.0f32;
                    }
                }
            }
            // 10. drowning (never reachable with a zero mheadMatrix on the server, R1; ported anyway)
            self.update_drowning(me, dt, is_sim);
        }

        // ---- D. idle look animation (both control modes)
        {
            let (sub, interacting) = self.t(|t| (t.state.sub_animation_type, t.state.interacting));
            if sub == 0 && interacting == 0 {
                let go = self.tm_ret(|t| {
                    t.idle_timer += dt;
                    t.random_idle_wait < t.idle_timer
                });
                if go {
                    let w = r01("CaveTroll.update.idle_wait") * 10.0f32 + 5.0f32;
                    let a = r01("CaveTroll.update.idle_a");
                    let b = r01("CaveTroll.update.idle_b");
                    let c = r01("CaveTroll.update.idle_c");
                    self.tm(|t| {
                        t.random_idle_wait = w;
                        t.idle_timer = 0.0f32;
                        t.walk_timer = 0.0f32;
                        t.state.sub_animation_type = ctsa::LOOK_LEFT_RIGHT_UP;
                        t.random_animation_value_a = a;
                        t.random_animation_value_b = b;
                        t.random_animation_value_c = c;
                    });
                }
            }
        }

        // ---- E. rider dpad
        let (mut dpad_target, mut dpad_moving) = (self.t(|t| t.to_square), false);
        {
            let rider = self.npc.borrow().rider.clone();
            if let Some(r) = rider {
                if !r.is_net() && !self.falling() {
                    let v = self.t(|t| t.rider_target_velocity);
                    if v.x > 0.2f32 {
                        dpad_target.x += 1;
                    } else if v.x < -0.2f32 {
                        dpad_target.x -= 1;
                    }
                    if v.y > 0.2f32 {
                        dpad_target.y += 1;
                    } else if v.y < -0.2f32 {
                        dpad_target.y -= 1;
                    }
                    dpad_moving = dpad_target != self.t(|t| t.to_square);
                }
            }
        }

        // ---- F. validation of the current target tile
        if local
            && !self.t(|t| t.path_needs_recalculated)
            && !self.t(|t| t.current_movement_was_initiated_by_rider)
            && !self.t(|t| t.waiting_for_path)
        {
            self.validate_target_tile(me, &host);
        }

        // ---- G. movement vs rest
        let (from_sq, to_sq) = self.t(|t| (t.from_square, t.to_square));
        let rest_test = from_sq == to_sq && !dpad_moving;
        if rest_test {
            let path_count = self.t(|t| t.path.as_ref().map_or(0, |p| p.len()));
            let (qsize, interacting, itimer, chasing) = self
                .t(|t| (t.current_net_queue_size, t.state.interacting != 0, t.state.interaction_timer, t.chasing_npc));
            if path_count == 0 && (local || qsize < 1 || (interacting && itimer < 0.3f32)) && (!chasing || interacting)
            {
                self.update_rest(me, dt, is_sim, local, &host);
                self.epilogue(local);
                return;
            }
        }
        self.update_move(me, dt, local, dpad_target, dpad_moving, &host);
        self.epilogue(local);
    }

    fn tm_ret<R>(&self, f: impl FnOnce(&mut CaveTrollData) -> R) -> R {
        f(&mut self.troll.borrow_mut())
    }

    /// H. epilogue: `if !local { pathNeedsRecalculated = NO }`.
    fn epilogue(&self, local: bool) {
        if !local {
            self.tm(|t| t.path_needs_recalculated = false);
        }
    }

    /// Steps 6 and 7: target validity of `tappedNPC` and `waitingBeforeRetaliationBlockhead`.
    fn validate_targets(&self) {
        let host = hostile_host();
        let tapped = self.t(|t| t.tapped_npc.clone());
        if let Some(b) = tapped {
            if host.bh_regenerating(&b) || b.needs_removed() || host.bh_on_trade_mission(&b) {
                self.tm(|t| {
                    t.tapped_npc = None;
                    t.chasing_npc = false;
                });
            }
        }
        let wb = self.t(|t| t.waiting_before_retaliation_blockhead.clone());
        if let Some(b) = wb {
            if host.bh_regenerating(&b) || b.needs_removed() || host.bh_on_trade_mission(&b) {
                self.tm(|t| t.waiting_before_retaliation_blockhead = None);
            }
        }
    }

    /// Step 10: drowning / thin air. `mouth = mheadMatrix * (0, 0.25, 0.25, 1)` (translation applied), or
    /// `(floatPos.x, 1.5 + floatPos.y)` when `isSimulation`.
    fn update_drowning(&self, me: &ObjRef, dt: f32, is_sim: bool) {
        let (mx, my) = if !is_sim {
            let m = self.t(|t| t.mhead_matrix);
            let (vx, vy, vz) = (0.0f32, 0.25f32, 0.25f32);
            (m[0] * vx + m[4] * vy + m[8] * vz + m[12], m[1] * vx + m[5] * vy + m[9] * vz + m[13])
        } else {
            let fp = me.float_pos();
            (fp.x, 1.5f32 + fp.y)
        };
        if !((my as f64) > 0.5f64) {
            return;
        }
        let mp = IntPair { x: f32_to_i32(mx), y: f32_to_i32(my) };
        let mouth_tile = tile_loaded(mp.x, mp.y);
        let mut underwater = false;
        if tile_is_water(mouth_tile) {
            let part = mouth_tile.map_or(0, |t| t.get().partial_content_left) as f32;
            if (my - (mp.y as f32)) * 255.0f32 <= part {
                underwater = true;
            }
            if self.t(|t| t.state.animation_type) == cta::SWIM {
                let ay = f64_to_i32((my as f64) + 0.5f64);
                let above = tile_loaded(mp.x, ay);
                let apart = above.map_or(0, |t| t.get().partial_content_left) as f32;
                if !tile_is_water(above) || apart < ((0.5f32 + my) - (ay as f32)) * 255.0f32 {
                    underwater = tile_is_solid(above);
                }
            }
        } else if tile_is_solid(mouth_tile) {
            let pos = me.pos();
            let body = tile_loaded(pos.x, pos.y);
            if tile_is_water(body) {
                underwater = true;
            }
        }
        let mut space = (my - 960.0f32) / 32.0f32;
        if space < 0.0f32 {
            space = 0.0f32;
        }
        if underwater || space > 0.001f32 {
            let tick = self.tm_ret(|t| {
                if underwater {
                    t.state.drown_fraction += dt / 30.0f32;
                    if t.state.drown_fraction >= 1.0333333f32 {
                        t.state.drown_fraction -= 0.033333335f32;
                        return true;
                    }
                } else {
                    t.state.drown_fraction += (dt / 30.0f32) * space;
                    if t.state.drown_fraction >= 1.1f32 {
                        t.state.drown_fraction -= 0.1f32;
                        return true;
                    }
                }
                false
            });
            if tick {
                let (dmg, max) = {
                    let mut n = self.npc.borrow_mut();
                    n.damage = n.damage.wrapping_add(1);
                    (n.damage, Npc::max_health(self))
                };
                if dmg >= max {
                    Npc::die(self, me, None);
                }
                if let Some(dw) = me.dynamic_world() {
                    dw.dynamic_world_changed_at_pos_object_type(me.pos(), me.object_type());
                }
            }
        } else {
            self.tm(|t| {
                if t.state.drown_fraction > 0.0f32 {
                    if t.state.drown_fraction <= 1.0f32 {
                        t.state.drown_fraction -= 0.5f32 * dt;
                        if t.state.drown_fraction <= 0.0f32 && t.state.drown_fraction != 0.0f32 {
                            t.state.drown_fraction = 0.0f32;
                        }
                    } else {
                        t.state.drown_fraction = 1.0f32;
                    }
                }
            });
        }
    }

    /// Steps 13 to 16: validation of the target tile with `checkCanEnterTile`.
    fn validate_target_tile(&self, me: &ObjRef, host: &Rc<dyn HostileHost>) {
        let Some(world) = host.world() else { return };
        let (fs, ts) = self.t(|t| (t.from_square, t.to_square));
        // UB-PARITY: fromProps.terrainDifficulty / traverseType are uninitialised in the original and not read.
        let from_props =
            FromProps { terrain_difficulty: 0, traverse_to_key_frame: self.t(|t| t.traverse_to_key_frame) };
        if self.t(|t| t.to_tile).is_none() {
            let nt = tile_at_world_position_loaded(ts.x, ts.y, &world);
            self.tm(|t| t.to_tile = nt);
        }
        if self.t(|t| t.from_tile).is_none() {
            let nt = tile_at_world_position_loaded(fs.x, fs.y, &world);
            self.tm(|t| t.from_tile = nt);
        }
        let (to_tile, from_tile) = self.t(|t| (t.to_tile, t.from_tile));
        let res = check_can_enter_tile(
            to_tile,
            from_tile,
            from_props,
            fs.x,
            fs.y,
            ts.x,
            ts.y,
            Some(me),
            &world,
            true,
            true,
            &[],
        );
        let no_path = self.t(|t| t.path.as_ref().map_or(0, |p| p.len())) == 0;
        if res.traverse_type as i32 == tt::INACCESSIBLE || (res.traverse_to_key_frame as i32 == kf::FALLING && no_path)
        {
            if tile_eq(&to_tile, &from_tile) {
                self.stop_interacting(me);
                self.tm(|t| t.path_needs_recalculated = true);
            } else {
                self.tm(|t| {
                    t.to_tile = t.from_tile;
                    t.to_square = t.from_square;
                });
                let to = self.t(|t| t.to_square);
                me.update_position(me, to);
                snap_float_pos(me, to);
                self.tm(|t| {
                    t.path_needs_recalculated = true;
                    t.path = None;
                });
            }
        } else if res.traverse_to_key_frame as i32 != self.t(|t| t.traverse_to_key_frame) {
            let new_type = animation_for_key_frame(res.traverse_to_key_frame as i32);
            self.tm(|t| {
                t.traverse_to_key_frame = res.traverse_to_key_frame as i32;
                t.state.animation_type = new_type;
            });
            me.set_update_needs_to_be_sent(true);
            let (anim, tsq) = self.t(|t| (t.state.animation_type, t.to_square));
            let fp = me.float_pos();
            if anim == cta::LIE {
                set_float_pos(me, fp.x, (tsq.y as f32) + -0.5f32);
            }
            if anim == cta::MONKEY_HANG {
                set_float_pos(me, fp.x, (tsq.y as f32) + 0.2f32);
            }
        }
    }

    /// REST block (step 17 onwards). Always followed by the epilogue.
    fn update_rest(&self, me: &ObjRef, dt: f32, is_sim: bool, local: bool, host: &Rc<dyn HostileHost>) {
        self.tm(|t| t.travel_fraction = 1.0f32);
        let interacting = self.t(|t| t.state.interacting) != 0;
        if interacting {
            if self.t(|t| t.state.interaction_type) == 0 {
                self.stop_interacting(me);
            }
            let (prev, now) = self.tm_ret(|t| {
                let prev = t.state.interaction_timer;
                t.state.interaction_timer += 1.2f32 * dt;
                (prev, t.state.interaction_timer)
            });
            if local && self.t(|t| t.state.interaction_type) == INTERACTION_STAB {
                if now >= 0.15f32 && prev < 0.15f32 {
                    self.strike(me, is_sim, host);
                } else if now >= 0.3f32 {
                    self.stop_interacting(me);
                }
            }
            return;
        }
        // not interacting
        let (chasing, rider) = (self.t(|t| t.chasing_npc), self.npc.borrow().rider.clone());
        if me.is_net() || chasing {
            return;
        }
        if rider.as_ref().map_or(false, |r| r.is_net()) {
            return;
        }
        let cr = host.custom_rules();
        if cr.custom_rules_enabled != 0 && cr.npc_difficulty == CUSTOM_RULE_NPC_PASSIVE {
            return;
        }
        let wait = self.tm_ret(|t| {
            if rider.is_some() && t.random_ai_wait > 1.0f32 {
                t.random_ai_wait = 1.0f32;
            }
            t.random_ai_wait -= dt;
            t.random_ai_wait
        });
        if wait > 0.0f32 {
            return;
        }
        let draw = if rider.is_none() {
            r01("CaveTroll.update.ai_wait") * 6.0f32 + 0.5f32
        } else {
            r01("CaveTroll.update.ai_wait") * 1.0f32 + 0.5f32
        };
        self.tm(|t| {
            t.return_to_defense = false;
            t.random_ai_wait = draw;
        });
        if rider.is_none() {
            let wb = self.t(|t| t.waiting_before_retaliation_blockhead.clone());
            if let Some(wb) = wb {
                if !host.bh_paused(&wb) && !Npc::belongs_to_player_with_blockhead(self, &wb) {
                    let p = wb.pos();
                    self.tm(|t| {
                        t.tapped_npc = Some(wb.clone());
                        t.chasing_npc = true;
                        t.last_known_npc_position = p;
                        t.path_needs_recalculated = true;
                        t.waiting_before_retaliation_blockhead = None;
                    });
                    return;
                }
            }
        }
        let blockheads = npc_env().all_blockheads_including_net();
        let pos = me.pos();
        let wwm = world_width_macro(me);
        let (cannot_return, defend) = self.t(|t| (t.cannot_return, t.defend_square));
        let scan = if rider.is_some() {
            true
        } else if !cannot_return {
            let dx = wrap_dx(defend.x, pos.x, wwm);
            if dx.wrapping_abs() < 32 && pos.y - 32 < defend.y && defend.y < pos.y + 32 {
                let r = r01("CaveTroll.update.scan");
                r > 0.2f32
            } else {
                self.tm(|t| {
                    t.return_to_defense = true;
                    t.path_needs_recalculated = true;
                });
                false
            }
        } else {
            let r = r01("CaveTroll.update.scan");
            r > 0.2f32
        };
        if scan {
            let min_distance = if rider.is_some() { 3 } else { 16 };
            for bh in blockheads.iter() {
                if host.bh_paused(bh) || Npc::belongs_to_player_with_blockhead(self, bh) {
                    continue;
                }
                let p = bh.pos();
                if !(wrap_dx(p.x, pos.x, wwm).wrapping_abs() < min_distance
                    && pos.y - min_distance < p.y
                    && p.y < pos.y + min_distance)
                {
                    continue;
                }
                let t = tile_loaded(p.x, p.y);
                let Some(tc) = tile_copy(&t) else { continue };
                if !(cannot_return || rider.is_some() || tc.back_wall_type_index == 1) {
                    continue;
                }
                self.tm(|s| {
                    s.tapped_npc = Some(bh.clone());
                    s.chasing_npc = true;
                    s.last_known_npc_position = p;
                    s.path_needs_recalculated = true;
                });
            }
        }
    }

    /// STRIKE (REST b): the punch lands.
    fn strike(&self, _me: &ObjRef, is_sim: bool, host: &Rc<dyn HostileHost>) {
        let damage = self.npc.borrow().damage;
        let max = Npc::max_health(self);
        let ratio = (damage as f32) / (max as f32);
        let mut dmg = 0.4f32 * (1.0f32 - ratio);
        if dmg <= 0.02f32 {
            dmg = 0.02f32;
        }
        let cr = host.custom_rules();
        dmg = scale_damage_for_difficulty(dmg, &cr);
        let tapped = self.t(|t| t.tapped_npc.clone());
        if let Some(tb) = &tapped {
            host.bh_suffer_damage(tb, dmg, is_sim, true);
            if is_sim {
                let s = if host.bh_regenerating(tb) { "killed" } else { "attacked" };
                host.add_damage_simulation_event(tb, &format!("{} by a cave troll", s));
            }
        }
        self.tm(|t| t.chasing_npc = false);
    }

    /// MOVE block (steps 20 to 23).
    fn update_move(
        &self,
        me: &ObjRef,
        dt: f32,
        local: bool,
        dpad_target: IntPair,
        dpad_moving: bool,
        host: &Rc<dyn HostileHost>,
    ) {
        let mut needs_interp = true;
        let qsize = self.t(|t| t.current_net_queue_size);
        let net_delay_factor = if local { 1.0f32 } else { 0.7f32 + 0.2f32 * (qsize as f32) };
        let rider = self.npc.borrow().rider.clone();
        let rider_influence = if rider.is_some() { 1.5f32 } else { 1.0f32 };
        // 21
        self.tm(|t| {
            let step = dt * t.travel_speed * rider_influence * net_delay_factor * 0.5f32;
            t.travel_fraction += if step < 1.0f32 { step } else { 1.0f32 };
            t.travel_fraction_normalized = t.travel_fraction;
            if t.travel_fraction_normalized >= 1.0f32 {
                t.travel_fraction_normalized -= 1.0f32;
            }
        });
        // 22 ARRIVAL
        if self.t(|t| t.travel_fraction) >= 1.0f32 {
            self.tm(|t| t.travel_fraction -= 1.0f32);
            let to_sq = self.t(|t| t.to_square);
            me.update_position(me, to_sq);
            Npc::check_current_position_for_food(self, me);
            self.tm(|t| t.current_movement_was_initiated_by_rider = false);
            snap_float_pos(me, to_sq);
            let (from_tile, to_tile, from_sq) = self.t(|t| (t.from_tile, t.to_tile, t.from_square));
            if !tile_eq(&from_tile, &to_tile) {
                let mut door_like = tile_contains_door(from_tile)
                    || tile_contains_trap_door(from_tile)
                    || tile_contains_gate(from_tile);
                if !door_like {
                    door_like = tile_contains_door(tile_loaded(from_sq.x, from_sq.y + 1));
                }
                if door_like && me.dynamic_world().is_some() {
                    npc_env().set_door_at_pos_to_open_direction(from_sq, false, 0);
                }
                // footstep: only the ping-pong toggle is state (the sound is cosmetic)
                self.tm(|t| {
                    if t.to_square.y == t.from_square.y {
                        t.footstep_ping_pong = !t.footstep_ping_pong;
                    } else {
                        t.footstep_ping_pong = true;
                    }
                });
            }
            self.tm(|t| {
                t.from_square = t.to_square;
                t.from_tile = t.to_tile;
                t.traverse_from_key_frame = t.traverse_to_key_frame;
            });
            let mut idle_after = true;
            let mut next_point_index: i32 = 0;
            let mut interact_tile: Option<TileRef> = None;
            let mut tile_interaction_type: u32 = INTERACTION_NONE;
            // chase position refresh
            if self.t(|t| t.chasing_npc) {
                let tapped = self.t(|t| t.tapped_npc.clone());
                let npc_pos = pos_of(&tapped);
                let (inter, itype, last) =
                    self.t(|t| (t.state.interacting != 0, t.state.interaction_type, t.last_known_npc_position));
                let skip = (inter && (itype == INTERACTION_STAB || itype == INTERACTION_RIDE)) || npc_pos == last;
                if !skip {
                    let wwm = world_width_macro(me);
                    let pos = me.pos();
                    let d1 = wrap_dx(last.x, pos.x, wwm);
                    let d2 = wrap_dx(npc_pos.x, pos.x, wwm);
                    if (-8 < d1 && d1 < 8) || (-8 < d2 && d2 < 8) {
                        self.tm(|t| {
                            t.last_known_npc_position = npc_pos;
                            t.path_needs_recalculated = true;
                        });
                    }
                }
            }
            if !self.t(|t| t.path_needs_recalculated) {
                let path_count = self.t(|t| t.path.as_ref().map_or(0, |p| p.len()));
                let qs = self.t(|t| t.current_net_queue_size);
                if path_count != 0 || dpad_moving || (!local && qs > 0) {
                    if (!me.is_net() && rider.is_none()) || dpad_moving {
                        me.set_update_needs_to_be_sent(true);
                    }
                    let mut next_tile: Option<TileRef> = None;
                    self.tm(|t| t.terrain_difficulty = 10);
                    let mut should_travel = true;
                    let (mut next_x, mut next_y) = (0i32, 0i32);
                    if !local {
                        self.arrival_replay_queue(me, &mut next_tile, &mut should_travel, &mut next_x, &mut next_y);
                    } else if dpad_moving {
                        // branch 2
                        let (fs, td) = self.t(|t| (t.from_square, t.terrain_difficulty));
                        // UB-PARITY: fromProps.traverseType is uninitialised in the original and not read.
                        let from_props = FromProps {
                            terrain_difficulty: td,
                            traverse_to_key_frame: self.t(|t| t.traverse_to_key_frame),
                        };
                        if let Some(world) = host.world() {
                            let pr = dpad_find_path(from_props, fs, dpad_target, Some(me), &world);
                            should_travel = pr.path_test_result.traverse_type as i32 != tt::INACCESSIBLE;
                            if should_travel {
                                next_tile = tile_at_world_position_loaded(pr.movement_pos.x, pr.movement_pos.y, &world);
                                self.tm(|t| {
                                    t.traverse_type = pr.path_test_result.traverse_type as i32;
                                    t.traverse_to_key_frame = pr.path_test_result.traverse_to_key_frame as i32;
                                    t.terrain_difficulty = pr.path_test_result.terrain_difficulty;
                                    t.current_movement_was_initiated_by_rider = true;
                                });
                                next_x = pr.movement_pos.x;
                                next_y = pr.movement_pos.y;
                            }
                        } else {
                            should_travel = false;
                        }
                    } else {
                        // branch 3: follow the path
                        let step = self.tm_ret(|t| {
                            let p = t.path.as_mut().unwrap();
                            let step = *p.last().unwrap();
                            t.traverse_type = step.traverse_type;
                            t.traverse_to_key_frame = step.traverse_to_key_frame;
                            t.terrain_difficulty = step.terrain_difficulty;
                            p.pop();
                            step
                        });
                        next_point_index = step.index;
                        if let Some(world) = host.world() {
                            next_tile = tile_at_world_index_loaded(next_point_index, &world);
                            let p = get_world_pos_for_world_index(next_point_index, &world);
                            next_x = p.x;
                            next_y = p.y;
                        }
                        let remaining = self.t(|t| t.path.as_ref().map_or(0, |p| p.len()));
                        if remaining == 0 {
                            let pos = me.pos();
                            if (next_x, next_y) == (pos.x, pos.y) {
                                should_travel = false;
                                self.tm(|t| t.traverse_from_key_frame = t.traverse_to_key_frame);
                            }
                            let (falling, move_only, goal) = self.t(|t| {
                                (
                                    t.last_path_was_falling,
                                    t.last_path_was_move_only,
                                    t.state.goal_interaction_type_while_walking,
                                )
                            });
                            if !falling && !move_only {
                                tile_interaction_type = goal;
                                if goal != INTERACTION_NONE {
                                    should_travel = false;
                                    let (inter, itile) = self.t(|t| (t.state.interacting != 0, t.interacting_tile));
                                    if !inter || !tile_eq(&itile, &next_tile) {
                                        let tapped = self.t(|t| t.tapped_npc.clone());
                                        let paused = tapped.as_ref().map_or(false, |b| host.bh_paused(b));
                                        if !paused {
                                            interact_tile = next_tile;
                                        }
                                    }
                                }
                            }
                        }
                    }
                    if should_travel && self.t(|t| t.traverse_type) != tt::INACCESSIBLE {
                        idle_after = false;
                        self.tm(|t| {
                            t.state.interacting = 0;
                            t.state.interaction_type = 0;
                        });
                        if local {
                            self.tm(|t| t.state.sub_animation_type = 0);
                            me.set_update_needs_to_be_sent(true);
                        }
                        let (td, ttype) = self.tm_ret(|t| {
                            if t.terrain_difficulty < 10 {
                                t.terrain_difficulty = 10;
                            }
                            t.travel_speed = 40.0f32 / (t.terrain_difficulty as f32);
                            (t.terrain_difficulty, t.traverse_type)
                        });
                        if ttype == tt::FALL {
                            self.tm(|t| {
                                t.travel_speed = ((td as f32) + 20.0f32) / 8.0f32;
                                if t.travel_speed < 1.0f32 {
                                    t.travel_speed = 1.0f32;
                                }
                            });
                            let tokf = self.t(|t| t.traverse_to_key_frame);
                            if local && tokf == kf::STAND && td < 99999 {
                                let x = ((td as f32) - 70.0f32) / 100.0f32;
                                if x > 0.0f32 {
                                    Npc::hit_with_force_blockhead(self, me, f32_to_i32(10.0f32 * x), None);
                                }
                            }
                        }
                        self.tm(|t| {
                            t.to_square = IntPair { x: next_x, y: next_y };
                            t.to_tile = next_tile;
                        });
                        if local {
                            let (ttype, ts) = self.t(|t| (t.traverse_type, t.travel_speed));
                            if let Some(a) = animation_for_traverse(ttype, ts) {
                                self.tm(|t| t.state.animation_type = a);
                            }
                        }
                    }
                }
            }
            // evaluated on every arrival, even when pathNeedsRecalculated skipped the selection above
            if idle_after || interact_tile.is_some() {
                self.tm(|t| {
                    t.travel_fraction = 1.0f32;
                    t.travel_speed = 4.0f32;
                });
                needs_interp = false;
                if (!me.is_net() && rider.is_none()) || dpad_moving {
                    me.set_update_needs_to_be_sent(true);
                }
                self.tm(|t| {
                    if t.traverse_to_key_frame == kf::INACCESSIBLE {
                        t.traverse_to_key_frame = t.traverse_from_key_frame;
                    }
                    t.state.animation_type = animation_for_key_frame(t.traverse_to_key_frame);
                });
                let (anim, tsq) = self.t(|t| (t.state.animation_type, t.to_square));
                let fp = me.float_pos();
                if anim == cta::LIE {
                    set_float_pos(me, fp.x, (tsq.y as f32) + -0.5f32);
                }
                if anim == cta::MONKEY_HANG {
                    set_float_pos(me, fp.x, 0.2f32 + (tsq.y as f32));
                }
            }
            if let Some(it) = interact_tile {
                self.start_interacting_with_tile_at_index_tile_interaction_type(
                    me,
                    next_point_index,
                    Some(it),
                    tile_interaction_type as i32,
                );
            }
        }
        // 23. floatPos interpolation
        if needs_interp {
            self.interpolate_float_pos(me);
        }
    }

    /// Branch 1 of the ARRIVAL selection: replays one queued net state (index 0).
    fn arrival_replay_queue(
        &self,
        _me: &ObjRef,
        next_tile: &mut Option<TileRef>,
        should_travel: &mut bool,
        next_x: &mut i32,
        next_y: &mut i32,
    ) {
        let n0 = self.t(|t| t.next_to_square[0]);
        let (a, sa) = self.t(|t| (t.next_animation_type[0], t.next_sub_animation_type[0]));
        self.tm(|t| {
            t.state.animation_type = a as u32;
            t.state.sub_animation_type = sa as u32;
        });
        *next_x = n0.x;
        *next_y = n0.y;
        *next_tile = tile_loaded(n0.x, n0.y);
        let (to_sq, interacting, ni0) = self.t(|t| (t.to_square, t.state.interacting != 0, t.next_interacting[0]));
        if n0 == to_sq && ni0 == interacting {
            self.tm(|t| {
                t.traverse_type = 0;
                t.traverse_from_key_frame = t.traverse_to_key_frame;
            });
            *should_travel = false;
        } else {
            self.tm(|t| {
                t.traverse_type = t.next_traverse_type[0];
                t.traverse_to_key_frame = t.next_traverse_to_key_frame[0];
                t.terrain_difficulty = t.next_terrain_difficulty[0];
            });
            if n0 == to_sq {
                *should_travel = false;
            }
        }
        if ni0 && !interacting {
            self.tm(|t| {
                t.state.interaction_timer = 0.0f32;
                t.state.interacting = 1;
                t.state.interacting_square = t.next_interaction_square[0];
                t.state.interaction_type = t.next_interaction_type[0] as u32;
                t.state.goal_interaction_type_while_walking = t.state.interaction_type;
            });
            self.update_gather_speed_and_animation_for_current_interation_and_item();
        } else if !ni0 && interacting {
            self.tm(|t| {
                t.state.interacting = 0;
                t.state.interaction_type = 0;
                t.state.goal_interaction_type_while_walking = 0;
                t.tapped_npc = None;
                t.chasing_npc = false;
            });
        }
        // shift the queue
        self.tm(|t| {
            let n = (t.current_net_queue_size - 1).max(0) as usize;
            for i in 0..n {
                t.next_to_square[i] = t.next_to_square[i + 1];
                t.next_terrain_difficulty[i] = t.next_terrain_difficulty[i + 1];
                t.next_traverse_type[i] = t.next_traverse_type[i + 1];
                t.next_traverse_to_key_frame[i] = t.next_traverse_to_key_frame[i + 1];
                t.next_interacting[i] = t.next_interacting[i + 1];
                t.next_interaction_square[i] = t.next_interaction_square[i + 1];
                t.next_interaction_type[i] = t.next_interaction_type[i + 1];
                t.next_animation_type[i] = t.next_animation_type[i + 1];
                t.next_sub_animation_type[i] = t.next_sub_animation_type[i + 1];
            }
            t.current_net_queue_size -= 1;
        });
    }

    /// Step 23: `floatPos` interpolation along the current move (animation-specific curves).
    fn interpolate_float_pos(&self, me: &ObjRef) {
        let (fs, ts, tf, tn, anim) = self.t(|t| {
            (t.from_square, t.to_square, t.travel_fraction, t.travel_fraction_normalized, t.state.animation_type)
        });
        let mut from = Vector2 { x: ((fs.x as f64) + 0.5f64) as f32, y: fs.y as f32 };
        let to = Vector2 { x: ((ts.x as f64) + 0.5f64) as f32, y: ts.y as f32 };
        let f = tf;
        let t = tn;
        let (mut x_lerp, mut y_lerp, mut y_offset) = (f, f, 0.0f32);
        let k = 0.75f64;
        let h = 0.5f64;
        // sinf((float)(double expression))
        let sf = |v: f64| -> f32 { sinf(v as f32) };
        let fmin = |a: f32, b: f32| if a < b { a } else { b };
        let fmax = |a: f32, b: f32| if a > b { a } else { b };
        match anim {
            cta::HANG_FALL_BALANCE => {
                let s = fmin(1.0, 2.0 * t);
                x_lerp = s;
                y_lerp = s;
            }
            cta::STAND_TO_HANG_JUMP | cta::HANG_TO_STAND_JUMP => {
                let s =
                    if anim == cta::STAND_TO_HANG_JUMP { fmin(1.0, 2.0 * t) } else { fmax(0.0, (t - 0.5f32) * 2.0f32) };
                x_lerp = s;
                y_lerp = s;
                if fs.y < ts.y {
                    y_lerp = sf((s as f64) * PI_D * k) / 0.7f32;
                } else if ts.y < fs.y {
                    y_lerp = 1.0f32 - sf(((1.0f32 - s) as f64) * PI_D * k) / 0.7f32;
                } else {
                    y_offset = sf((s as f64) * PI_D) * 0.4f32;
                }
            }
            cta::HANG_TO_HANG_JUMP => {
                let c = 3.0f32 * t - 1.0f32;
                x_lerp = if c < 0.0 {
                    0.0
                } else if c > 1.0 {
                    1.0
                } else {
                    c
                };
                if fs.y < ts.y {
                    y_lerp = sf((x_lerp as f64) * PI_D * k) / 0.7f32;
                } else if ts.y < fs.y {
                    y_lerp = 1.0f32 - sf(((1.0f32 - x_lerp) as f64) * PI_D * k) / 0.7f32;
                } else {
                    y_offset = sf((x_lerp as f64) * PI_D) * 0.4f32;
                    y_lerp = x_lerp;
                }
            }
            cta::WALK => {
                if fs.y < ts.y {
                    y_lerp = sf((t as f64) * PI_D * k) / 0.7f32;
                } else if ts.y < fs.y {
                    y_lerp = 1.0f32 - sf(((1.0f32 - t) as f64) * PI_D * k) / 0.7f32;
                }
            }
            cta::JUMP_OVER_GAP => {
                y_offset = ((sf((t as f64) * PI_D) as f64) * 0.35f64) as f32;
                x_lerp = t;
            }
            cta::CROUCH_UP | cta::CROUCH_DOWN => {
                let mut q = f * f;
                if anim == cta::CROUCH_DOWN {
                    q = 1.0f32 - q;
                }
                y_lerp = sf((q as f64) * PI_D * h);
                x_lerp = sf((q as f64) * PI_D * h);
                y_offset = ((-0.3f64) * (sf((q as f64) * PI_D) as f64)) as f32;
            }
            cta::CROUCH_CRAWL_UP | cta::CROUCH_CRAWL_DOWN => {
                let mut q = f * f;
                if anim == cta::CROUCH_CRAWL_DOWN {
                    q = 1.0f32 - q;
                }
                y_lerp = sf((q as f64) * PI_D * h);
                x_lerp = sf((q as f64) * PI_D * h);
                y_offset = ((-0.5f64) * (q as f64)) as f32;
            }
            cta::CLIMB => {
                if ts.x != fs.x {
                    x_lerp = if (x_lerp as f64) < 0.25f64 {
                        0.0
                    } else if (x_lerp as f64) <= 0.75f64 {
                        sf(((x_lerp as f64) - 0.25f64) * 2.0f64 * PI_D * h)
                    } else {
                        1.0
                    };
                }
            }
            cta::CRAWL => y_offset = -0.5f32,
            cta::MONKEY_SWING | cta::MONKEY_HANG => y_offset = 0.2f32,
            cta::STAND_TO_HANG_DOUBLE_JUMP => {
                y_lerp = t;
                if t < 0.25f32 {
                    y_lerp = (sf(((4.0f32 * t) as f64) * PI_D * k) / 0.7f32) * 0.25f32;
                }
            }
            cta::STAND_TO_MONKEY_HANG_JUMP | cta::MONKEY_HANG_TO_STAND_JUMP => {
                let u = t;
                let w;
                if anim == cta::MONKEY_HANG_TO_STAND_JUMP {
                    let v = 1.0f32 - u;
                    y_lerp = fmax(0.0, (v - 0.2f32) / 0.8f32);
                    x_lerp = fmin(1.0, v / 0.5f32);
                    w = fmin(1.0, (1.0f32 - v) / 0.75f32);
                } else {
                    x_lerp = fmin(1.0, 2.0f32 * u);
                    y_lerp = 0.5f32 * x_lerp + (sf((x_lerp as f64) * PI_D * k) / 0.7f32) * 0.5f32;
                    w = u;
                }
                y_offset = 0.2f32 * w;
            }
            cta::MONKEY_SWING_UP_TO_CLIMB | cta::CLIMB_DOWN_TO_MONKEY_SWING => {
                let mut u = f;
                if anim == cta::CLIMB_DOWN_TO_MONKEY_SWING {
                    u = 1.0f32 - u;
                }
                y_lerp = fmax(0.0, (u - 0.2f32) / 0.8f32);
                x_lerp = fmin(1.0, u / 0.3f32);
                let w = fmin(1.0, (1.0f32 - u) / 0.75f32);
                y_offset = 0.2f32 * w;
            }
            cta::HANG_TO_MONKEY_SWING_DIAGONAL_JUMP_UP | cta::MONKEY_SWING_TO_HANG_DIAGONAL_JUMP_DOWN => {
                let mut u = t;
                if anim == cta::MONKEY_SWING_TO_HANG_DIAGONAL_JUMP_DOWN {
                    u = 1.0f32 - u;
                }
                x_lerp = if u >= 0.5f32 { powf((u - 0.5f32) * 2.0f32, 0.5f32) } else { 0.0 };
                y_lerp = if u >= 0.5f32 {
                    if u <= 0.75f32 {
                        sf(((u - 0.5f32) * 4.0f32) as f64 * PI_D * h)
                    } else {
                        1.0
                    }
                } else {
                    0.0
                };
                y_offset = 0.2f32 * u;
            }
            _ => {
                x_lerp = f;
                y_lerp = f;
                y_offset = 0.0;
            }
        }
        // wrap fix of from.x
        let wwm = world_width_macro(me);
        let wt = wwm.wrapping_shl(5);
        if from.x - to.x <= ((wt / 2) as f32) {
            if from.x - to.x < ((wt.wrapping_neg() / 2) as f32) {
                from.x += wt as f32;
            }
        } else {
            from.x -= wt as f32;
        }
        let reversed = matches!(
            anim,
            cta::CROUCH_DOWN
                | cta::CROUCH_CRAWL_DOWN
                | cta::CLIMB_DOWN_TO_MONKEY_SWING
                | cta::MONKEY_SWING_TO_HANG_DIAGONAL_JUMP_DOWN
                | cta::MONKEY_HANG_TO_STAND_JUMP
        );
        let (nx, ny) = if reversed {
            (linear_interpolate(to.x, from.x, x_lerp), linear_interpolate(to.y, from.y, y_lerp) + y_offset)
        } else {
            (linear_interpolate(from.x, to.x, x_lerp), linear_interpolate(from.y, to.y, y_lerp) + y_offset)
        };
        set_float_pos(me, nx, ny);
    }
}

// =====================================================================================================
// DynamicObject / PathUser / Npc / protocol impls
// =====================================================================================================

impl DynamicObject for CaveTroll {
    fn base(&self) -> &RefCell<DynamicObjectBase> {
        &self.base
    }
    fn class_info(&self) -> &'static ClassInfo {
        &CAVE_TROLL_CLASS
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    npc_forward!(
        object_type,
        blockheads_loaded,
        can_be_removed_by_blockhead,
        update_position,
        should_save_every_change_in_position,
        client_id_for_saving_seperately_and_only_loading_while_player_online,
        requires_physical_block,
        dealloc,
        as_harmable,
        as_ridable,
        as_tap_choice
    );

    /// `-update:accurateDT:isSimulation:`.
    fn update_accurate_dt_is_simulation(&self, me: &ObjRef, dt: f32, accurate_dt: f32, is_simulation: bool) {
        self.update_impl(me, dt, accurate_dt, is_simulation)
    }

    /// `-renderPos` = `floatPos + (0, 0.3)`.
    fn render_pos(&self) -> Vector2 {
        let fp = self.base.borrow().float_pos;
        Vector2 { x: fp.x + 0.0f32, y: fp.y + 0.3f32 }
    }

    /// `-getSaveDict`.
    fn get_save_dict(&self, me: &ObjRef) -> PlistDict {
        let mut d = npc_get_save_dict(me);
        let dead = self.npc.borrow().dead;
        let (st, def, chasing, last) = self.t(|t| (t.state, t.defend_square, t.chasing_npc, t.last_known_npc_position));
        let m = &mut d.map;
        m.insert("state".to_string(), PlistValue::data(st.to_vec()));
        m.insert("dead".to_string(), PlistValue::boolean(dead));
        m.insert("defendSquare.x".to_string(), PlistValue::int(def.x as i64));
        m.insert("defendSquare.y".to_string(), PlistValue::int(def.y as i64));
        if chasing {
            m.insert("attackingNPC".to_string(), PlistValue::boolean(chasing));
            m.insert("lastKnownNPCPosition.x".to_string(), PlistValue::int(last.x as i64));
            m.insert("lastKnownNPCPosition.y".to_string(), PlistValue::int(last.y as i64));
        }
        d
    }

    /// `-creationNetDataForClient:`: 0x88 bytes plus the NPC tail.
    fn creation_net_data_for_client(&self, me: &ObjRef, client_id: Option<&str>) -> Option<Vec<u8>> {
        let cd = CaveTrollCreationData {
            dynamic_object_net_data: Npc::npc_creation_net_data_for_client(self, me, client_id),
            cave_troll_update_data: self.cave_troll_update_data_for_client(me, client_id),
        };
        Some(Npc::append_npc_creation_data_to_data(self, me, cd.to_vec()))
    }

    /// `-updateNetDataForClient:`: 0x40 bytes.
    fn update_net_data_for_client(&self, me: &ObjRef, client_id: Option<&str>) -> Option<Vec<u8>> {
        Some(self.cave_troll_update_data_for_client(me, client_id).to_vec())
    }

    /// `-remoteUpdate:`.
    fn remote_update(&self, me: &ObjRef, net_data: &[u8]) {
        npc_remote_update(me, net_data);
        let dead = self.npc.borrow().dead;
        if !dead {
            if !self.control_is_local() {
                let u = <CaveTrollUpdateData as WireStruct>::read_le(&padded_bytes(net_data, 0x40));
                self.do_remote_update(me, &u);
            }
        } else if self.t(|t| t.travel_fraction) < 1.0f32 && !npc_env().is_client() {
            me.set_needs_removed(me, false);
        }
    }

    /// `-remoteCreationDataUpdate:`.
    fn remote_creation_data_update(&self, me: &ObjRef, net_data: &[u8]) {
        npc_remote_creation_data_update(me, net_data);
        let cd = <CaveTrollCreationData as WireStruct>::read_le(&padded_bytes(net_data, 0x88));
        self.do_remote_update(me, &cd.cave_troll_update_data);
    }

    /// `-blockheadUnloaded:`.
    fn blockhead_unloaded(&self, me: &ObjRef, b: &ObjRef) {
        if is_same(&self.t(|t| t.tapped_npc.clone()), b) {
            self.tm(|t| {
                t.tapped_npc = None;
                t.chasing_npc = false;
            });
        }
        if is_same(&self.npc.borrow().rider.clone(), b) {
            self.tm(|t| t.rider_target_velocity = Vector2 { x: 0.0, y: 0.0 });
        }
        npc_blockhead_unloaded(me, b);
    }

    /// `-worldChanged:` / `-worldContentsChanged:` are empty: the base defaults are empty too.
    fn world_changed(&self, _me: &ObjRef, _positions: &mut Vec<IntPair>) {}
    fn world_contents_changed(&self, _me: &ObjRef, _positions: &mut Vec<IntPair>) {}

    fn as_path_user(&self) -> Option<&dyn PathUserDynamicObject> {
        Some(self)
    }
}

impl PathUserDynamicObject for CaveTroll {
    /// `-tileIsLitForSelf:atPos:` is YES always.
    fn tile_is_lit_for_self_at_pos(&self, _tile: &Tile, _pos: IntPair) -> bool {
        true
    }
    fn current_traverse_to_key_frame(&self) -> i32 {
        self.t(|t| t.traverse_to_key_frame)
    }
    fn next_pos(&self) -> IntPair {
        self.t(|t| t.to_square)
    }
    /// `-infoForPathRecalculation`. The `"pathUser"` entry cannot be stored in a plist; the pump supplies
    /// the object itself (`PathRequest::from_plist_dict_with_user`,).
    fn info_for_path_recalculation(&self) -> Option<PlistDict> {
        let (pnr, chasing, ret_def, def, last) = self.t(|t| {
            (t.path_needs_recalculated, t.chasing_npc, t.return_to_defense, t.defend_square, t.last_known_npc_position)
        });
        if !pnr {
            return None;
        }
        let mut d = crate::objects::base::new_mutable_dict();
        let m = &mut d.map;
        let mut put = |k: &str, v: i32| {
            m.insert(k.to_string(), PlistValue::int(v as i64));
        };
        if !chasing {
            if !ret_def {
                put("pathType", path_type::FALL);
                put("goalX", def.x);
                put("goalY", def.y);
            } else {
                put("goalX", def.x);
                put("goalY", def.y);
                put("goalInteraction", 0);
                put("pathType", path_type::WORLD_TAPPED_MOVE_ONLY);
            }
        } else {
            put("goalX", last.x);
            put("goalY", last.y);
            put("goalInteraction", INTERACTION_STAB as i32);
            put("pathType", path_type::NPC_CHASE);
        }
        Some(d)
    }
    fn set_path_type_goal_interaction_extra_data(
        &self,
        me: &ObjRef,
        path: Option<Vec<PlistValue>>,
        ty: i32,
        goal_interaction: i32,
        extra: Option<PlistValue>,
    ) {
        let steps = path.map(|p| path_steps_from_plist(&p));
        self.set_path(me, steps, ty, goal_interaction, extra);
    }
    fn set_waiting_for_path_to_pos(&self, p: IntPair) {
        self.tm(|t| {
            t.waiting_for_path = true;
            t.waiting_path_goal_pos = p;
        });
    }
    fn set_no_longer_waiting_for_path(&self) {
        self.tm(|t| t.waiting_for_path = false);
    }
    fn waiting_for_path(&self) -> bool {
        self.t(|t| t.waiting_for_path)
    }
    /// `-canCrawl` NO
    fn can_crawl(&self) -> bool {
        false
    }
    /// `-canFly` NO
    fn can_fly(&self) -> bool {
        false
    }
    fn path_needs_recalculated(&self) -> bool {
        self.t(|t| t.path_needs_recalculated)
    }
    fn set_path_needs_recalculated(&self, v: bool) {
        self.tm(|t| t.path_needs_recalculated = v);
    }
}

impl Npc for CaveTroll {
    fn npc(&self) -> &RefCell<NpcState> {
        &self.npc
    }
    fn npc_type(&self) -> i32 {
        NPC_CAVETROLL
    }
    fn max_age(&self) -> f32 {
        86400.0f32
    }
    fn dies_of_old_age(&self) -> bool {
        false
    }
    fn dies_of_low_fullness(&self) -> bool {
        false
    }
    fn max_health(&self) -> u16 {
        256
    }
    fn creation_data_struct_size(&self) -> usize {
        0x88
    }
    fn species_name(&self) -> String {
        "CAVE TROLL".to_string()
    }
    fn food_item_type(&self) -> i32 {
        ITEM_GOLD_COIN
    }
    fn capture_required_item_type(&self) -> i32 {
        ITEM_CAGE
    }
    fn get_names_array(&self) -> Option<&'static [&'static str]> {
        Some(&CAVE_TROLL_NAMES)
    }
    fn get_names_array_count(&self) -> i32 {
        35
    }
    fn suffers_damage_at_high_temperatures(&self) -> bool {
        false
    }
    /// `-canBeCapturedByBlockhead:withItemType:` NO always.
    fn can_be_captured_by_blockhead_with_item_type(&self, _b: &ObjRef, _item: i32) -> bool {
        false
    }
    /// `-cantBeCapturedTipStringForBlockhead:withItemType:`.
    fn cant_be_captured_tip_string_for_blockhead_with_item_type(&self, _b: &ObjRef, item: i32) -> Option<String> {
        if item == ITEM_CAGE {
            Some(format!("YOU DIDN'T SERIOUSLY THINK\nYOU COULD CAGE A {}?!", self.species_name()))
        } else {
            None
        }
    }
    /// `-die:`: the death timer restarts at 0, then the NPC `die:`.
    fn die(&self, me: &ObjRef, killer: Option<&ObjRef>) {
        if !self.npc.borrow().dead {
            self.tm(|t| t.travel_fraction = 0.0f32);
            npc_die(me, killer);
        }
    }
    /// `-reactToBeingHit`: one `rand()` when not dead.
    fn react_to_being_hit(&self, _me: &ObjRef) {
        if !self.npc.borrow().dead {
            let r = r01("CaveTroll.reactToBeingHit");
            self.tm(|t| {
                t.current_net_queue_size = 0;
                t.recoil_timer = 0.5f32;
                t.recoil_direction = r;
            });
        }
    }
    /// `-reactToBeingFed`: `[super reactToBeingFed]` is empty in the NPC base.
    fn react_to_being_fed(&self, me: &ObjRef) {
        self.tm(|t| t.current_net_queue_size = 0);
        me.set_update_needs_to_be_sent(true);
    }
    /// `-hitWithForce:blockhead:`: super, then the retaliation trigger.
    fn hit_with_force_blockhead(&self, me: &ObjRef, force: i32, b: Option<&ObjRef>) {
        npc_hit_with_force_blockhead(me, force, b);
        let dead = self.npc.borrow().dead;
        let Some(bh) = b else { return };
        let tapped_chasing = self.t(|t| is_same(&t.tapped_npc, bh) && t.chasing_npc);
        if !dead && force > 0 && !me.is_net() && !tapped_chasing {
            let cr = hostile_host().custom_rules();
            if cr.custom_rules_enabled != 0 && cr.npc_difficulty == CUSTOM_RULE_NPC_PASSIVE {
                return;
            }
            let wait = r01("CaveTroll.hitWithForce.retaliation") * 0.6f32 + 0.2f32;
            self.tm(|t| {
                t.waiting_before_retaliation_blockhead = Some(bh.clone());
                t.random_ai_wait = wait;
            });
            self.stop_interacting(me);
            self.set_path(me, None, path_type::RECALCULATED_DUE_TO_CHANGE, 0, None);
            self.tm(|t| t.waiting_for_path = false);
        }
    }
    fn tap_is_within_body_radius(&self, tap: Vector2) -> bool {
        self.tap_is_within_body_radius_impl(tap)
    }
    /// `-namePos` = `renderPos + (0, 1.5)`.
    fn name_pos(&self) -> Vector2 {
        let r = DynamicObject::render_pos(self);
        Vector2 { x: r.x + 0.0f32, y: r.y + 1.5f32 }
    }
    /// `-riderPosForBlockhead:`.
    fn rider_pos_for_blockhead(&self, _b: &ObjRef) -> [f32; 4] {
        let r = DynamicObject::render_pos(self);
        [r.x + 0.0f32, r.y + 0.8f32, -0.75f32, 1.0f32]
    }
    fn camera_pos_for_blockhead(&self, _b: &ObjRef) -> Vector2 {
        DynamicObject::render_pos(self)
    }
    fn set_target_velocity(&self, v: Vector2) {
        self.tm(|t| t.rider_target_velocity = v);
    }
    /// `-rideDirection`
    fn ride_direction(&self) -> i32 {
        if self.npc.borrow().rider.is_none() {
            return 0;
        }
        let vx = self.t(|t| t.rider_target_velocity.x);
        if vx > 0.2f32 {
            1
        } else if vx < -0.2f32 {
            -1
        } else {
            0
        }
    }
    /// `-addRider:`
    fn add_rider(&self, me: &ObjRef, b: &ObjRef) {
        npc_add_rider(me, b);
        if !self.npc.borrow().being_controlled {
            self.tm(|t| t.rider_target_velocity = Vector2 { x: 0.0, y: 0.0 });
        }
    }
    /// `-removeRider:`
    fn remove_rider(&self, me: &ObjRef, b: &ObjRef) {
        self.remove_rider_opt(me, Some(b))
    }
    fn remove_rider_opt(&self, me: &ObjRef, b: Option<&ObjRef>) {
        let cur = self.npc.borrow().rider.clone();
        let same = match (&cur, b) {
            (None, None) => true,
            (Some(a), Some(b)) => ptr_eq(a, b),
            _ => false,
        };
        if same {
            npc_remove_rider_opt(me, b);
            self.tm(|t| t.rider_target_velocity = Vector2 { x: 0.0, y: 0.0 });
        }
    }
    /// `-blockheadCanRide:usingItem:`
    fn blockhead_can_ride_using_item(&self, b: &ObjRef, _item: i32) -> bool {
        Npc::belongs_to_player_with_blockhead(self, b) && !self.npc.borrow().dead && !self.base.borrow().needs_removed
    }
    fn rider_d_pad_should_allow_up_down(&self) -> bool {
        true
    }
    fn rider_d_pad_should_give_discrete_values(&self) -> bool {
        true
    }
}

impl_npc_protocols!(CaveTroll);

/// The registry constructor (`class_for_npc_type(6)`): the 8-argument initialiser.
fn ctor(
    world: &WorldRef,
    dw: &DynamicWorldRef,
    pos: IntPair,
    cache: CacheRef,
    save_dict: Option<&PlistDict>,
    is_adult: bool,
    was_placed: bool,
    placed_by_client: Option<&str>,
) -> Option<ObjRef> {
    CaveTroll::init_with_world_dynamic_world_at_position_cache_save_dict_is_adult_was_placed_placed_by_client(
        world,
        dw,
        pos,
        cache,
        save_dict,
        is_adult,
        was_placed,
        placed_by_client,
    )
    .map(|o| o as ObjRef)
}

/// Registers the troll constructor for NPC type 6 with the npc_base factory (`object_registry::install` calls this).
pub fn register() {
    register_npc_ctor(NPC_CAVETROLL, ctor);
}

// =====================================================================================================
// Test support (shared with drop_bear.rs and scorpion.rs tests) and the troll unit tests
// =====================================================================================================

#[cfg(test)]
pub(crate) mod test_support {
    use super::*;
    use crate::npc::npc::{set_npc_env, NpcEnv};
    use crate::objects::base::testutil::MockDw;
    use crate::path::path_creator::testutil::{new_world, GridLoader};
    use std::cell::Cell;

    /// A stand-in Blockhead: a plain DynamicObject at a position with an id.
    pub struct TestBh {
        base: RefCell<DynamicObjectBase>,
    }
    impl DynamicObject for TestBh {
        fn base(&self) -> &RefCell<DynamicObjectBase> {
            &self.base
        }
        fn class_info(&self) -> &'static ClassInfo {
            &crate::objects::class_info::DYNAMIC_OBJECT_CLASS
        }
        fn as_any(&self) -> &dyn Any {
            self
        }
    }
    impl TestBh {
        pub fn alloc(id: u64, pos: IntPair) -> ObjRef {
            let o = crate::objects::handle::alloc_obj(|w| TestBh {
                base: RefCell::new(DynamicObjectBase::alloc_zeroed_with(w)),
            });
            {
                let mut b = o.base.borrow_mut();
                b.unique_id = id;
                b.pos = pos;
                b.float_pos = Vector2 { x: pos.x as f32 + 0.5, y: pos.y as f32 };
            }
            o
        }
    }

    pub struct TestEnv {
        pub world: Rc<World>,
        pub bhs: RefCell<Vec<ObjRef>>,
        pub doors: RefCell<Vec<(IntPair, bool, i32)>>,
        /// `[dynamicWorld isClient]`
        pub client: Cell<bool>,
    }
    impl crate::npc::npc::NpcEnvStub for TestEnv {
        fn world(&self) -> Option<Rc<World>> {
            Some(self.world.clone())
        }
        fn is_client(&self) -> bool {
            self.client.get()
        }
        fn all_blockheads_including_net(&self) -> Vec<ObjRef> {
            self.bhs.borrow().clone()
        }
        fn set_door_at_pos_to_open_direction(&self, pos: IntPair, open: bool, dir: i32) {
            self.doors.borrow_mut().push((pos, open, dir));
        }
    }

    pub struct TestHost {
        pub world: Rc<World>,
        pub rules: Cell<CustomRules>,
        pub expert: Cell<bool>,
        pub day: Cell<f32>,
        pub damage: RefCell<Vec<(u64, f32, bool, bool)>>,
        pub free_blocks: RefCell<Vec<(IntPair, i32)>>,
        pub projectiles: RefCell<Vec<(Vector2, Vector2, i32)>>,
        pub npc_at: Cell<bool>,
        pub babies: RefCell<Vec<(IntPair, i32)>>,
        pub sim_events: RefCell<Vec<String>>,
        /// Blockhead ids the stand-in answers `paused` / `regenerating` / `onTradeMission` / not `shouldContinueSimulating` for.
        pub paused: Cell<Option<u64>>,
        pub regen: Cell<Option<u64>>,
        pub trade: Cell<Option<u64>>,
        pub nosim: Cell<Option<u64>>,
        /// The food of the cactus the stand-in finds (None: no cactus).
        pub cactus: Cell<Option<f32>>,
        /// `world()` answers `None` while set.
        pub no_world: Cell<bool>,
    }
    impl HostileHostStub for TestHost {
        fn cactus_available_food_at(&self, _pos: IntPair) -> Option<f32> {
            self.cactus.get()
        }
        fn set_cactus_available_food_at(&self, _pos: IntPair, v: f32) {
            self.cactus.set(Some(v));
        }
        fn bh_paused(&self, b: &ObjRef) -> bool {
            Some(b.unique_id()) == self.paused.get()
        }
        fn bh_regenerating(&self, b: &ObjRef) -> bool {
            Some(b.unique_id()) == self.regen.get()
        }
        fn bh_on_trade_mission(&self, b: &ObjRef) -> bool {
            Some(b.unique_id()) == self.trade.get()
        }
        fn bh_should_continue_simulating(&self, b: &ObjRef) -> bool {
            match self.nosim.get() {
                Some(id) => b.unique_id() != id,
                None => HostileHost::bh_should_continue_simulating(&DefaultHost, b),
            }
        }
        fn world(&self) -> Option<Rc<World>> {
            if self.no_world.get() {
                None
            } else {
                Some(self.world.clone())
            }
        }
        fn custom_rules(&self) -> CustomRules {
            self.rules.get()
        }
        fn expert_mode(&self) -> bool {
            self.expert.get()
        }
        fn day_night_fraction(&self, _x: f32, _t: f64) -> f32 {
            self.day.get()
        }
        fn npc_exists_at_pos_ignore_npc(&self, _pos: IntPair, _ignore: Option<&ObjRef>) -> bool {
            self.npc_at.get()
        }
        fn create_free_block(&self, pos: IntPair, item_type: i32, _p: Option<&ObjRef>) {
            self.free_blocks.borrow_mut().push((pos, item_type));
        }
        fn load_baby_npc_at_position(&self, pos: IntPair, npc_type: i32) {
            self.babies.borrow_mut().push((pos, npc_type));
        }
        fn bh_suffer_damage(&self, b: &ObjRef, dmg: f32, sim: bool, recoil: bool) {
            self.damage.borrow_mut().push((b.unique_id(), dmg, sim, recoil));
        }
        fn fire_projectile(
            &self,
            from: Vector2,
            to: Vector2,
            _at: Option<&ObjRef>,
            item: i32,
            _firer: Option<&ObjRef>,
        ) {
            self.projectiles.borrow_mut().push((from, to, item));
        }
        fn add_damage_simulation_event(&self, _b: &ObjRef, text: &str) {
            self.sim_events.borrow_mut().push(text.to_string());
        }
    }

    pub struct Fx {
        pub world: Rc<World>,
        pub wref: WorldRef,
        pub dw: Rc<MockDw>,
        pub dwr: DynamicWorldRef,
        pub env: Rc<TestEnv>,
        pub host: Rc<TestHost>,
        pub _loader: Rc<GridLoader>,
    }

    /// A real `World` (4 macro tiles wide) with solid stone for `y <= ground` and air above, a mock dynamic world,
    /// and the env / host seams installed. The libc RNG is seeded with 1.
    pub fn fixture(ground: i32) -> Fx {
        let (world, loader, _fdw) = new_world(4, ground);
        let wref: WorldRef = world.clone();
        let dw = Rc::new(MockDw {
            next_id: Cell::new(100),
            log: Rc::new(RefCell::new(Vec::new())),
            server: true,
            blockheads: RefCell::new(vec![]),
        });
        let dwr: DynamicWorldRef = dw.clone();
        let env = Rc::new(TestEnv {
            world: world.clone(),
            bhs: RefCell::new(vec![]),
            doors: RefCell::new(vec![]),
            client: Cell::new(false),
        });
        set_npc_env(env.clone());
        let host = Rc::new(TestHost {
            world: world.clone(),
            rules: Cell::new(CustomRules::default()),
            expert: Cell::new(false),
            day: Cell::new(0.5),
            damage: RefCell::new(vec![]),
            free_blocks: RefCell::new(vec![]),
            projectiles: RefCell::new(vec![]),
            npc_at: Cell::new(false),
            babies: RefCell::new(vec![]),
            sim_events: RefCell::new(vec![]),
            paused: Cell::new(None),
            regen: Cell::new(None),
            trade: Cell::new(None),
            nosim: Cell::new(None),
            cactus: Cell::new(None),
            no_world: Cell::new(false),
        });
        set_hostile_host(host.clone());
        glibc_rand::srand(1);
        Fx { world, wref, dw, dwr, env, host, _loader: loader }
    }

    /// Sets the custom rules: enabled with the given NPC difficulty.
    pub fn set_difficulty(fx: &Fx, d: i8) {
        let mut r = CustomRules::default();
        r.custom_rules_enabled = 1;
        r.npc_difficulty = d;
        fx.host.rules.set(r);
    }

    /// The first `n + 1` outputs of `rand()` after `srand(seed)`; leaves the stream re-seeded and untouched.
    pub fn rand_stream(seed: u32, n: usize) -> Vec<i32> {
        glibc_rand::srand(seed);
        let v: Vec<i32> = (0..=n).map(|_| glibc_rand::rand()).collect();
        glibc_rand::srand(seed);
        v
    }

    /// Number of `rand()` calls made since `srand(seed)`: the next output is compared against the stream.
    pub fn draws_since(seed: u32) -> usize {
        let next = glibc_rand::rand();
        let s = rand_stream(seed, 64);
        s.iter().position(|v| *v == next).expect("rand stream position")
    }
}

#[cfg(test)]
mod tests {
    use super::test_support::*;
    use super::*;
    use crate::npc::npc::Npc;

    fn mk_troll(fx: &Fx, pos: IntPair) -> (Rc<CaveTroll>, ObjRef) {
        let t =
            CaveTroll::init_with_world_dynamic_world_at_position_cache_save_dict_is_adult_was_placed_placed_by_client(
                &fx.wref, &fx.dwr, pos, None, None, true, false, None,
            )
            .expect("troll");
        let me: ObjRef = t.clone();
        (t, me)
    }

    fn ip(x: i32, y: i32) -> IntPair {
        IntPair { x, y }
    }

    /// Test 1: struct layouts and sizes.
    #[test]
    fn layouts() {
        assert_eq!(std::mem::size_of::<CaveTrollUpdateData>(), 0x40);
        assert_eq!(std::mem::size_of::<CaveTrollCreationData>(), 0x88);
        assert_eq!(std::mem::size_of::<CaveTrollState>(), 0x24);
        let f = fixture(10);
        let (t, me) = mk_troll(&f, ip(10, 11));
        let u = t.cave_troll_update_data_for_client(&me, None);
        let b = u.to_vec();
        assert_eq!(b.len(), 0x40);
        assert_eq!(b[0x3a], 0, "health byte is zero");
        assert!(b[0x3b..].iter().all(|x| *x == 0), "padding is zero");
        let c = me.creation_net_data_for_client(&me, None).unwrap();
        assert_eq!(c.len(), 0x88);
        let up = me.update_net_data_for_client(&me, None).unwrap();
        assert_eq!(up.len(), 0x40);
    }

    /// Test 2: constants.
    #[test]
    fn constants() {
        let f = fixture(10);
        let (t, me) = mk_troll(&f, ip(10, 11));
        assert_eq!(CAVE_TROLL_NAMES.len(), 35);
        assert_eq!(CAVE_TROLL_NAMES[34], "Herculies");
        assert_eq!(me.object_type(), 39);
        assert_eq!(Npc::npc_type(&*t), 6);
        assert_eq!(Npc::max_age(&*t), 86400.0);
        assert!(!Npc::dies_of_old_age(&*t));
        assert!(!Npc::dies_of_low_fullness(&*t));
        assert_eq!(Npc::species_name(&*t), "CAVE TROLL");
        assert_eq!(Npc::food_item_type(&*t), 167);
        assert_eq!(Npc::capture_required_item_type(&*t), 302);
        assert_eq!(Npc::max_health(&*t), 256);
        assert_eq!(Npc::creation_data_struct_size(&*t), 0x88);
        assert_eq!(Npc::get_names_array_count(&*t), 35);
        assert!(!Npc::suffers_damage_at_high_temperatures(&*t));
        assert!(!Npc::can_be_captured_by_blockhead_with_item_type(&*t, &me, 302));
        assert_eq!(
            Npc::cant_be_captured_tip_string_for_blockhead_with_item_type(&*t, &me, 302).unwrap(),
            "YOU DIDN'T SERIOUSLY THINK\nYOU COULD CAGE A CAVE TROLL?!"
        );
        assert!(Npc::cant_be_captured_tip_string_for_blockhead_with_item_type(&*t, &me, 5).is_none());
        assert!(t.tile_is_lit_for_self_at_pos(&Tile::default(), ip(0, 0)));
        assert!(Npc::rider_d_pad_should_allow_up_down(&*t));
        assert!(Npc::rider_d_pad_should_give_discrete_values(&*t));
        assert!(!t.can_crawl() && !t.can_fly());
        assert_eq!(me.class_info().name, "CaveTroll");
        assert!(me.class_info().conforms_to("PathUserDynamicObject"));
        assert!(me.class_info().is_kind_of(&NPC_CLASS));
    }

    /// Test 3: initial values; test 4: `initSubDerivedStuffStuff` consumes exactly one draw.
    #[test]
    fn initial_values_and_rng() {
        let f = fixture(10);
        glibc_rand::srand(1);
        let (t, me) = mk_troll(&f, ip(10, 11));
        let d = t.data();
        assert_eq!(d.travel_fraction, 1.0);
        assert_eq!(d.travel_speed, 4.0);
        assert_eq!(d.defend_square, ip(10, 11));
        assert_eq!(d.from_square, ip(10, 11));
        assert_eq!(d.to_square, ip(10, 11));
        assert!(!d.cannot_return);
        assert_eq!(d.rider_target_velocity, Vector2 { x: 1.0, y: 1.0 });
        assert_eq!(d.rider_position.v, [1.0; 4]);
        assert!(d.mhead_matrix.iter().all(|x| *x == 0.0));
        assert!(d.random_idle_wait >= 5.0 && d.random_idle_wait <= 15.0);
        assert!(d.from_tile.is_some() && d.to_tile.is_some());
        drop(d);
        // NPC adult init: 3 start values + 1 hunger timer, then the troll's idle wait: 5 draws in total
        assert_eq!(draws_since(1), 5);
        let _ = me;
    }

    /// Test 5: save dict key set and plist types; test 6: load round trip.
    #[test]
    fn save_dict_and_load() {
        let f = fixture(10);
        let (t, me) = mk_troll(&f, ip(10, 11));
        t.data_mut().state.sub_animation_type = 4;
        let d = me.get_save_dict(&me);
        match d.map.get(&"state".to_string()) {
            Some(PlistValue::Data(b)) => assert_eq!(b.bytes.len(), 36),
            other => panic!("state: {:?}", other),
        }
        assert!(matches!(d.map.get(&"dead".to_string()), Some(PlistValue::Bool(false))));
        assert_eq!(d.map.get(&"defendSquare.x".to_string()).unwrap().int_value(), 10);
        assert_eq!(d.map.get(&"defendSquare.y".to_string()).unwrap().int_value(), 11);
        assert!(d.map.get(&"attackingNPC".to_string()).is_none());
        // chasing adds three keys
        {
            let mut dd = t.data_mut();
            dd.chasing_npc = true;
            dd.last_known_npc_position = ip(7, 8);
        }
        let d2 = me.get_save_dict(&me);
        assert!(matches!(d2.map.get(&"attackingNPC".to_string()), Some(PlistValue::Bool(true))));
        assert_eq!(d2.map.get(&"lastKnownNPCPosition.x".to_string()).unwrap().int_value(), 7);
        assert_eq!(d2.map.get(&"lastKnownNPCPosition.y".to_string()).unwrap().int_value(), 8);
        // load it back: chasing is NOT restored, state and defend square are
        let t2 = CaveTroll::init_with_world_dynamic_world_save_dict_cache(&f.wref, &f.dwr, &d2, None).expect("load");
        assert!(!t2.data().chasing_npc);
        assert_eq!(t2.data().defend_square, ip(10, 11));
        assert_eq!(t2.data().state.sub_animation_type, 4);
        assert_eq!(t2.data().travel_fraction, 1.0);
    }

    /// Test 8a: the dead path: `travelFraction += dt`, no drop before 1.0, one `rand() % 4`.
    #[test]
    fn dead_path_drops_ice_armour() {
        let f = fixture(10);
        let (t, me) = mk_troll(&f, ip(10, 11));
        let kb = TestBh::alloc(900, ip(1, 11));
        t.npc.borrow_mut().dead = true;
        t.npc.borrow_mut().kill_blockhead = Some(kb);
        t.data_mut().travel_fraction = 0.0;
        glibc_rand::srand(7);
        let expect = glibc_rand::rand() % 4;
        glibc_rand::srand(7);
        me.update_accurate_dt_is_simulation(&me, 0.4, 0.4, false);
        assert!(f.host.free_blocks.borrow().is_empty());
        assert!(!me.needs_removed());
        assert_eq!(draws_since(7), 0, "no draw before the timer elapses");
        me.update_accurate_dt_is_simulation(&me, 0.7, 0.7, false);
        let fb = f.host.free_blocks.borrow().clone();
        assert_eq!(fb.len(), 1);
        let want = [197, 195, 196, 198][expect as usize];
        assert_eq!(fb[0], (ip(10, 11), want));
        assert!(me.needs_removed());
        assert_eq!(draws_since(7), 1);
    }

    /// Test 8b: the strike fires on the update where `interactionTimer` crosses 0.15; damage formula and clamp.
    #[test]
    fn strike_timing_and_damage() {
        let run = |diff: Option<i8>, damage: u16| -> Vec<(u64, f32, bool, bool)> {
            let f = fixture(10);
            if let Some(d) = diff {
                set_difficulty(&f, d);
            }
            let (t, me) = mk_troll(&f, ip(10, 11));
            let victim = TestBh::alloc(500, ip(11, 11));
            {
                t.npc.borrow_mut().damage = damage;
                let mut d = t.data_mut();
                d.tapped_npc = Some(victim);
                d.chasing_npc = true;
                d.state.interacting = 1;
                d.state.interaction_type = INTERACTION_STAB;
                d.state.goal_interaction_type_while_walking = INTERACTION_STAB;
                d.state.interaction_timer = 0.0;
            }
            // 0.1 s * 1.2 = 0.12 < 0.15: no strike yet
            me.update_accurate_dt_is_simulation(&me, 0.1, 0.1, false);
            assert!(f.host.damage.borrow().is_empty());
            assert!(t.data().chasing_npc);
            // another 0.1 s: 0.24 >= 0.15 and prev < 0.15: strike
            me.update_accurate_dt_is_simulation(&me, 0.1, 0.1, false);
            assert!(!t.data().chasing_npc);
            // the same update also stops interacting only at >= 0.3; run once more and check no second strike
            me.update_accurate_dt_is_simulation(&me, 0.1, 0.1, false);
            let v = f.host.damage.borrow().clone();
            v
        };
        let normal = run(None, 0);
        assert_eq!(normal.len(), 1);
        assert_eq!(normal[0], (500, 0.4f32 * (1.0 - 0.0), false, true));
        let easy = run(Some(CUSTOM_RULE_NPC_EASY), 0);
        assert_eq!(easy[0].1, ((0.4f32 as f64) * 0.25) as f32);
        let hard = run(Some(CUSTOM_RULE_NPC_HARD), 0);
        assert_eq!(hard[0].1, 4.0f32 * 0.4f32);
        let weak = run(None, 256);
        assert_eq!(weak[0].1, 0.02f32);
    }

    /// Test 8c: `randomAIWait` is decremented every rest update; the scan happens only at <= 0; passive never scans.
    #[test]
    fn rest_ai_wait_and_scan() {
        let f = fixture(10);
        let (t, me) = mk_troll(&f, ip(10, 11));
        // stand in a spot with a natural rock back wall
        crate::path::path_creator::testutil::edit(&f.world, 11, 11, |tl| tl.back_wall_type_index = 1);
        let victim = TestBh::alloc(500, ip(11, 11));
        f.env.bhs.borrow_mut().push(victim);
        t.data_mut().random_ai_wait = 1.0;
        me.update_accurate_dt_is_simulation(&me, 0.25, 0.25, false);
        assert!((t.data().random_ai_wait - 0.75).abs() < 1e-6);
        assert!(t.data().tapped_npc.is_none());
        glibc_rand::srand(3);
        me.update_accurate_dt_is_simulation(&me, 1.0, 1.0, false);
        // the wait elapsed: one draw to re-arm (+ at most one for the scan decision)
        assert!(t.data().random_ai_wait > 0.0);
        let n = draws_since(3);
        assert!(n == 1 || n == 2, "draws: {n}");
        // passive rules: no scan, no draws
        let f2 = fixture(10);
        set_difficulty(&f2, CUSTOM_RULE_NPC_PASSIVE);
        let (t2, me2) = mk_troll(&f2, ip(10, 11));
        t2.data_mut().random_ai_wait = 0.0;
        glibc_rand::srand(3);
        me2.update_accurate_dt_is_simulation(&me2, 1.0, 1.0, false);
        assert_eq!(draws_since(3), 0);
        assert!(t2.data().tapped_npc.is_none());
    }

    /// Test 8e: idle look: after `randomIdleWait` seconds the animation becomes 4 and four draws are consumed, once.
    #[test]
    fn idle_look_once() {
        let f = fixture(10);
        set_difficulty(&f, CUSTOM_RULE_NPC_PASSIVE); // keep the rest block from drawing
        let (t, me) = mk_troll(&f, ip(10, 11));
        t.data_mut().random_idle_wait = 2.0;
        me.update_accurate_dt_is_simulation(&me, 1.0, 1.0, false);
        assert_eq!(t.data().state.sub_animation_type, 0);
        glibc_rand::srand(9);
        me.update_accurate_dt_is_simulation(&me, 1.5, 1.5, false);
        assert_eq!(t.data().state.sub_animation_type, ctsa::LOOK_LEFT_RIGHT_UP);
        assert_eq!(draws_since(9), 4);
        glibc_rand::srand(9);
        me.update_accurate_dt_is_simulation(&me, 20.0, 20.0, false);
        assert_eq!(draws_since(9), 0, "no more idle draws until the sub animation is cleared");
        let s = rand_stream(9, 4);
        glibc_rand::srand(9);
        let _ = s;
    }

    /// Test 8d (R1): with the zero `mheadMatrix` a troll standing in water never accrues `drownFraction`.
    #[test]
    fn zero_matrix_never_drowns() {
        let f = fixture(10);
        crate::path::path_creator::testutil::edit(&f.world, 10, 11, |tl| {
            tl.type_index = 3;
            tl.partial_content_left = 255;
        });
        set_difficulty(&f, CUSTOM_RULE_NPC_PASSIVE);
        let (t, me) = mk_troll(&f, ip(10, 11));
        for _ in 0..10 {
            me.update_accurate_dt_is_simulation(&me, 1.0, 1.0, false);
        }
        assert_eq!(t.data().state.drown_fraction, 0.0);
    }

    /// `hitWithForce`: two draws (reactToBeingHit then the retaliation), passive never retaliates.
    #[test]
    fn hit_with_force_retaliation_draws() {
        let f = fixture(10);
        let (t, me) = mk_troll(&f, ip(10, 11));
        let hitter = TestBh::alloc(77, ip(12, 11));
        glibc_rand::srand(11);
        let s = rand_stream(11, 3);
        glibc_rand::srand(11);
        Npc::hit_with_force_blockhead(&*t, &me, 5, Some(&hitter));
        assert_eq!(draws_since(11), 2);
        let want = (s[1] as f32 / 2147483648.0f32) * 0.6f32 + 0.2f32;
        assert_eq!(t.data().random_ai_wait, want);
        assert!(is_same(&t.data().waiting_before_retaliation_blockhead, &hitter));
        assert!(!t.data().waiting_for_path);
        assert_eq!(t.npc.borrow().damage, 5);
        assert_eq!(t.data().recoil_timer, 0.5);
        // passive: one draw (reactToBeingHit), no retaliation
        let f2 = fixture(10);
        set_difficulty(&f2, CUSTOM_RULE_NPC_PASSIVE);
        let (t2, me2) = mk_troll(&f2, ip(10, 11));
        let hitter2 = TestBh::alloc(78, ip(12, 11));
        glibc_rand::srand(11);
        Npc::hit_with_force_blockhead(&*t2, &me2, 5, Some(&hitter2));
        assert_eq!(draws_since(11), 1);
        assert!(t2.data().waiting_before_retaliation_blockhead.is_none());
    }

    /// Rider rules: `controlIsLocal` for a remote rider is false (R8); velocity resets; `rideDirection` thresholds.
    #[test]
    fn rider_rules() {
        let f = fixture(10);
        let (t, me) = mk_troll(&f, ip(10, 11));
        assert!(t.control_is_local());
        assert_eq!(Npc::ride_direction(&*t), 0);
        let rider = TestBh::alloc(5, ip(10, 11));
        rider.base().borrow_mut().is_net = true;
        t.npc.borrow_mut().rider = Some(rider.clone());
        assert!(!t.control_is_local(), "a remote (isNet) rider means the server runs no AI");
        Npc::set_target_velocity(&*t, Vector2 { x: 0.5, y: 0.0 });
        assert_eq!(Npc::ride_direction(&*t), 1);
        Npc::set_target_velocity(&*t, Vector2 { x: -0.5, y: 0.0 });
        assert_eq!(Npc::ride_direction(&*t), -1);
        Npc::set_target_velocity(&*t, Vector2 { x: 0.2, y: 0.0 });
        assert_eq!(Npc::ride_direction(&*t), 0);
        // unloading the rider zeroes the velocity
        Npc::set_target_velocity(&*t, Vector2 { x: 0.9, y: 0.9 });
        t.blockhead_unloaded(&me, &rider);
        assert_eq!(t.data().rider_target_velocity, Vector2 { x: 0.0, y: 0.0 });
        // remote update replays through the queue (<= 4, slot 3 overwritten when full)
        let mut u = CaveTrollUpdateData::default();
        u.to_square_x = 11;
        u.to_square_y = 11;
        assert!(!t.data().has_had_remote_update);
        t.do_remote_update(&me, &u);
        assert!(t.data().has_had_remote_update);
        assert_eq!(t.data().to_square, ip(11, 11));
        for i in 0..6u32 {
            u.to_square_x = 20 + i;
            t.do_remote_update(&me, &u);
        }
        assert_eq!(t.data().current_net_queue_size, 4);
        assert_eq!(t.data().next_to_square[3], ip(25, 11));
    }

    /// `setPath` / `infoForPathRecalculation` / waiting-for-path.
    #[test]
    fn path_user_protocol() {
        let f = fixture(10);
        let (t, me) = mk_troll(&f, ip(10, 11));
        assert!(t.info_for_path_recalculation().is_none());
        t.set_path_needs_recalculated(true);
        let d = t.info_for_path_recalculation().unwrap();
        assert_eq!(d.map.get(&"pathType".to_string()).unwrap().int_value(), 5);
        assert_eq!(d.map.get(&"goalX".to_string()).unwrap().int_value(), 10);
        assert!(d.map.get(&"goalInteraction".to_string()).is_none());
        t.data_mut().return_to_defense = true;
        let d = t.info_for_path_recalculation().unwrap();
        assert_eq!(d.map.get(&"pathType".to_string()).unwrap().int_value(), 1);
        assert_eq!(d.map.get(&"goalInteraction".to_string()).unwrap().int_value(), 0);
        {
            let mut dd = t.data_mut();
            dd.chasing_npc = true;
            dd.last_known_npc_position = ip(14, 12);
        }
        let d = t.info_for_path_recalculation().unwrap();
        assert_eq!(d.map.get(&"pathType".to_string()).unwrap().int_value(), 3);
        assert_eq!(d.map.get(&"goalX".to_string()).unwrap().int_value(), 14);
        assert_eq!(d.map.get(&"goalInteraction".to_string()).unwrap().int_value(), 10);
        t.set_waiting_for_path_to_pos(ip(3, 4));
        assert!(t.waiting_for_path());
        t.set_no_longer_waiting_for_path();
        assert!(!t.waiting_for_path());
        // setPath: nil path while chasing stops interacting and requests a recalculation
        t.set_path_type_goal_interaction_extra_data(&me, None, 3, 10, None);
        assert!(t.path_needs_recalculated());
        assert!(!t.data().chasing_npc);
        // a path: [0] is the goal, the last element the next step
        let wt = 4 * 32;
        let mk = |idx: i32| {
            let mut dct = crate::objects::base::new_mutable_dict();
            dct.map.insert("index".to_string(), PlistValue::int(idx as i64));
            dct.map.insert("traverseType".to_string(), PlistValue::int(1));
            dct.map.insert("traverseToKeyFrame".to_string(), PlistValue::int(0));
            dct.map.insert("terrainDifficulty".to_string(), PlistValue::int(10));
            PlistValue::Dict(dct)
        };
        let goal = 11 * wt + 15;
        t.set_path_type_goal_interaction_extra_data(&me, Some(vec![mk(goal), mk(11 * wt + 11)]), 0, 10, None);
        assert_eq!(t.data().final_goal_square, ip(15, 11));
        assert_eq!(t.data().state.goal_interaction_type_while_walking, 10);
        assert!(!t.path_needs_recalculated());
        assert_eq!(t.data().path.as_ref().unwrap().len(), 2);
        assert!(t.is_heading_for_square(ip(15, 11)));
        assert!(!t.is_heading_for_square(ip(16, 11)));
    }

    /// Following a path: the troll walks to the next tile and arrives with the right animation.
    #[test]
    fn follows_path_and_arrives() {
        let f = fixture(10);
        let (t, me) = mk_troll(&f, ip(10, 11));
        t.set_path(
            &me,
            Some(vec![
                PathStep {
                    index: 11 * 128 + 12,
                    terrain_difficulty: 10,
                    traverse_type: 1,
                    traverse_to_key_frame: 0,
                    parent_index: None,
                    properties_g: None,
                },
                PathStep {
                    index: 11 * 128 + 11,
                    terrain_difficulty: 10,
                    traverse_type: 1,
                    traverse_to_key_frame: 0,
                    parent_index: None,
                    properties_g: None,
                },
            ]),
            0,
            0,
            None,
        );
        // first update: from == to, path non-empty => MOVE; travel_fraction starts at 1.0 so it arrives at once
        me.update_accurate_dt_is_simulation(&me, 0.1, 0.1, false);
        assert_eq!(t.data().to_square, ip(11, 11));
        assert_eq!(t.data().state.animation_type, cta::WALK);
        assert_eq!(t.data().travel_speed, 4.0);
        // walk until arrival
        for _ in 0..20 {
            me.update_accurate_dt_is_simulation(&me, 0.1, 0.1, false);
        }
        assert_eq!(me.pos(), ip(12, 11));
        assert_eq!(t.data().path.as_ref().unwrap().len(), 0);
    }

    /// Remote update on a dead troll with `travelFraction < 1.0` clears `needsRemoved` (R8).
    #[test]
    fn remote_update_dead_clears_removal() {
        let f = fixture(10);
        let (t, me) = mk_troll(&f, ip(10, 11));
        t.npc.borrow_mut().dead = true;
        t.data_mut().travel_fraction = 0.3;
        me.base().borrow_mut().needs_removed = true;
        let u = CaveTrollUpdateData::default();
        me.remote_update(&me, &u.to_vec());
        assert!(!me.needs_removed());
    }

    /// `registry` constructor produces a type-39 troll.
    #[test]
    fn registered_ctor() {
        let f = fixture(10);
        register();
        let o = crate::npc::npc::new_npc_for_type(6, &f.wref, &f.dwr, ip(10, 11), None, None, true, false, None)
            .expect("troll");
        assert_eq!(o.object_type(), 39);
    }
}
