//! `Blockhead`: the player character. Struct, ivars, class info, trivial getters, `DynamicObject` /
//! `PathUserDynamicObject` / `HarmableDynamicObject` / `BlockheadApi` impls, and the seam to the sibling modules.
//!
//!
//! ## Seams
//! Many callees of this module are owned by modules that were empty skeletons when this file was written
//! (`blockhead_inventory`, `blockhead_actions`, `world_core`, `world_ops`, `dynamic_world_ops`, NPCs, workbench,
//! plants, fishing rod, `Action`). Every such call goes through [`BlockheadHost`] (all methods have inert or
//! spec-derived defaults) which the integrator installs once with [`set_host`]. Nothing of the logic of this
//! module is hidden behind the host.

use std::any::Any;
use std::cell::RefCell;
use std::rc::{Rc, Weak};

use crate::items::inventory_item::{InventoryItem, ItemRef, SubSlot};
use crate::objects::base::{base_update_position, BlockheadApi, DynamicObject, DynamicObjectBase, DynamicWorldRef};
use crate::objects::class_info::{ClassInfo, DYNAMIC_OBJECT_CLASS};
use crate::objects::handle::{ObjRef, ObjWeak};
use crate::objects::protocols::{HarmableDynamicObject, PathUserDynamicObject};
use crate::plist::value::{PlistDict, PlistValue};
use crate::types::item_type::ItemType;
use crate::types::structs::{
    BlockheadSkinOptions, BlockheadState, FreeBlockCreationCount, IntPair, InteractionTestResult, TradeMission, Vector,
    Vector2,
};
use crate::types::tile::Tile;
use crate::world::state::{TileRef, World};

// ====================================================================================================
// Enum raw values used by the whole module (private type tables; spec "Enum values used",)
// ====================================================================================================

/// `BlockheadAnimationType` raw values.
pub mod va {
    pub const STAND: u32 = 0;
    pub const WALK: u32 = 1;
    pub const SWIM: u32 = 5;
    pub const CRAWL: u32 = 6;
    pub const HANG: u32 = 7;
    pub const MONKEY_HANG: u32 = 14;
    pub const LIE: u32 = 23;
    pub const BOB: u32 = 24;
    pub const RIDE: u32 = 27;
    pub const RIDE_HANDCAR: u32 = 28;
    pub const RIDE_LOCOMOTIVE: u32 = 29;
    pub const RIDE_PASSENGER_CAR: u32 = 30;
    pub const FLY_JETPACK: u32 = 32;
    pub const RIDE_ARMS_DOWN: u32 = 33;
}

/// `BlockheadSubAnimationType` raw values.
pub mod vsa {
    pub const NONE: u32 = 0;
    pub const DIG: u32 = 1;
    pub const CRAFT: u32 = 2;
    pub const STAB: u32 = 3;
    pub const LOOK_LEFT_RIGHT_UP: u32 = 4;
    pub const LIE_ON_BED: u32 = 5;
    pub const LIE_ON_GROUND: u32 = 6;
    pub const MEDITATING: u32 = 7;
    pub const FIRE_ARROW: u32 = 8;
}

/// `InteractionType` raw values (;).
pub mod it {
    pub const NONE: u32 = 0;
    pub const DIG: u32 = 1;
    pub const PLACE_ITEM: u32 = 2;
    pub const PLACE_SOLID_BLOCK: u32 = 3;
    pub const CRAFT: u32 = 4;
    pub const USE_INTERACTIVE_OBJECT: u32 = 5;
    pub const SOW: u32 = 6;
    pub const PICKUP: u32 = 7;
    pub const PICKUP_FREEBLOCK: u32 = 8;
    pub const BURN: u32 = 9;
    pub const STAB: u32 = 10;
    pub const RIDE: u32 = 12;
    pub const ADD_FUEL: u32 = 13;
    pub const FILL_BUCKET: u32 = 14;
    pub const PAINT: u32 = 15;
    pub const STRIP_PAINT: u32 = 16;
    pub const RIDE_BOAT: u32 = 17;
    pub const REMOVE_BOAT: u32 = 18;
    pub const CAPTURE: u32 = 19;
    pub const FIRE_RANGED_WEAPON: u32 = 20;
    pub const RIDE_TRAIN_CAR: u32 = 21;
    pub const REMOVE_TRAIN_CAR: u32 = 22;
    pub const DIG_FOREGROUND: u32 = 23;
    pub const DIG_BACK_WALL: u32 = 24;
    pub const FEED_NPC: u32 = 25;
    pub const INSPECT_NPC: u32 = 26;
    pub const HARVEST_NPC: u32 = 27;
    pub const SET_NPC_FREE: u32 = 28;
}

/// `TileTraverseType` raw values.
pub mod tt {
    pub const NONE: u32 = 0;
    pub const WALK: u32 = 1;
    pub const SWIM: u32 = 2;
    pub const CRAWL: u32 = 3;
    pub const CLIMB: u32 = 4;
    pub const FALL: u32 = 17;
    pub const INACCESSIBLE: u32 = 25;
    pub const STAIRS: u32 = 26;
    pub const ELEVATOR: u32 = 27;
    pub const FLY: u32 = 28;
}

/// `TileTraverseKeyFrameType` raw values.
pub mod kf {
    pub const STAND: u32 = 0;
    pub const BOB: u32 = 1;
    pub const CRAWL: u32 = 2;
    pub const HANG_BACK: u32 = 3;
    pub const HANG_RIGHT: u32 = 4;
    pub const HANG_LEFT: u32 = 5;
    pub const MONKEY_HANG: u32 = 6;
    pub const FALLING: u32 = 7;
    pub const INACCESSIBLE: u32 = 8;
    pub const STAIRS: u32 = 9;
    pub const STAND_ELEVATOR: u32 = 10;
    pub const FLY: u32 = 11;
}

/// `TipType` raw values.
pub mod tip {
    pub const NONE: i32 = 0;
    pub const COLLAPSE_EXHAUSTED: i32 = 1;
    pub const COLLAPSE_STARVATION: i32 = 2;
    pub const REFUSES_TO_WORK_EXHAUSTED: i32 = 3;
    pub const REFUSES_TO_WORK_STARVATION: i32 = 4;
    pub const NO_FUEL: i32 = 5;
    pub const NO_ELECTRICITY: i32 = 6;
    pub const SUFFER_DAMAGE: i32 = 7;
    pub const REGENERATING: i32 = 8;
    pub const DYING: i32 = 9;
}

/// `DYNAMIC_OBJECT_TYPE_BLOCKHEAD`.
pub const DYNAMIC_OBJECT_TYPE_BLOCKHEAD: i32 = 24;
/// `DYNAMIC_OBJECT_TYPE_NPC_DONKEY`.
pub const DYNAMIC_OBJECT_TYPE_NPC_DONKEY: i32 = 28;

// ====================================================================================================
// Class info
// ====================================================================================================

/// `Blockhead: DynamicObject <PathUserDynamicObject, HarmableDynamicObject>`.
pub static BLOCKHEAD_CLASS: ClassInfo = ClassInfo {
    name: "Blockhead",
    superclass: Some(&DYNAMIC_OBJECT_CLASS),
    protocols: &["PathUserDynamicObject", "HarmableDynamicObject"],
};

/// `[o isKindOfClass:[Blockhead class]]` downcast.
pub fn as_blockhead(o: &ObjRef) -> Option<&Blockhead> {
    o.as_any().downcast_ref::<Blockhead>()
}

// ====================================================================================================
// Placeholders for types owned by modules that did not exist when this file was written
// ====================================================================================================

/// `Action*`. Placeholder alias: the integrator replaces it by `Rc<Action>`.
pub type ActionRef = Rc<dyn Any>;
/// `FishingRod*` (physics_objects). Placeholder alias: the integrator replaces it by `Rc<FishingRod>`.
pub type FishingRodRef = Rc<dyn Any>;
/// `saveItemSlotsArray[i]` on the server: an array of `InventoryItem` save blobs.
pub type SaveSlot = Vec<Vec<u8>>;

/// The `extraData` argument of `World.addSimulationEventOfType:forBlockhead:extraData:` (world_core 5.9).
#[derive(Clone, Debug, PartialEq)]
pub enum SimulationEventExtra {
    /// `nil`
    None,
    /// an `NSString` (types 4 and 5)
    Text(String),
    /// `[NSNumber numberWithInt:itemType]` (type 3)
    ItemType(i32),
}

// ====================================================================================================
// PathExtraData
// ====================================================================================================

/// A value of the `pathExtraData` NSDictionary.
#[derive(Clone)]
pub enum ExtraValue {
    Int(i64),
    Obj(ObjRef),
    /// `CraftableItemObject` (not a `DynamicObject`)
    Any(Rc<dyn Any>),
}

/// `pathExtraData` (`NSDictionary`; keys seen: `craftCountOrExtraData`, `count`, `interactionObject`,
/// `workbench`, `craftableItemObject`, `blockhead`, `dontFollow`).
#[derive(Clone, Default)]
pub struct PathExtraData {
    entries: Vec<(String, ExtraValue)>,
}

impl PathExtraData {
    /// `[NSDictionary dictionary]`
    pub fn new() -> Self {
        PathExtraData { entries: Vec::new() }
    }
    /// `setObject:forKey:` (replaces an existing key).
    pub fn set(&mut self, key: &str, v: ExtraValue) {
        if let Some(e) = self.entries.iter_mut().find(|e| e.0 == key) {
            e.1 = v;
        } else {
            self.entries.push((key.to_string(), v));
        }
    }
    /// `[[d objectForKey:k] intValue]`: a missing key gives 0.
    pub fn int_for_key(&self, key: &str) -> i32 {
        match self.entries.iter().find(|e| e.0 == key) {
            Some((_, ExtraValue::Int(v))) => *v as i32,
            _ => 0,
        }
    }
    /// `[d objectForKey:k]` as a dynamic object.
    pub fn object_for_key(&self, key: &str) -> Option<ObjRef> {
        match self.entries.iter().find(|e| e.0 == key) {
            Some((_, ExtraValue::Obj(o))) => Some(o.clone()),
            _ => None,
        }
    }
    /// `[d objectForKey:k]` for non-`DynamicObject` values.
    pub fn any_for_key(&self, key: &str) -> Option<Rc<dyn Any>> {
        match self.entries.iter().find(|e| e.0 == key) {
            Some((_, ExtraValue::Any(o))) => Some(o.clone()),
            _ => None,
        }
    }
    /// The number entries as a plist dictionary (what a path request can carry through the path creator); object
    /// entries are left out (see [`PathExtraData::with_objects_from`]).
    pub fn to_plist(&self) -> PlistValue {
        let mut d = crate::foundation::gsdict::GsDict::new();
        for (k, v) in &self.entries {
            if let ExtraValue::Int(i) = v {
                d.insert(k.clone(), PlistValue::int(*i));
            }
        }
        PlistValue::dict(d)
    }
    /// `self` plus the object entries of `other` that `self` lacks (the original passes one dictionary through the
    /// path creator; the plist round trip of a path request keeps only its numbers).
    pub fn with_objects_from(mut self, other: &PathExtraData) -> PathExtraData {
        for (k, v) in &other.entries {
            if !matches!(v, ExtraValue::Int(_)) && !self.entries.iter().any(|e| &e.0 == k) {
                self.entries.push((k.clone(), v.clone()));
            }
        }
        self
    }
    /// Adapter from the `Option<PlistValue>` of `PathUserDynamicObject::set_path_type_goal_interaction_extra_data`
    /// (object_model signature, R-1): only number values can be carried by a `PlistValue`.
    pub fn from_plist(v: Option<&PlistValue>) -> Option<PathExtraData> {
        let v = v?;
        let PlistValue::Dict(d) = v else { return None };
        let mut out = PathExtraData::new();
        for (k, val) in d.map.iter() {
            out.set(k, ExtraValue::Int(val.long_long_value()));
        }
        Some(out)
    }
}

// ====================================================================================================
// BlockheadFields
// ====================================================================================================

/// Every server-relevant ivar of `@interface Blockhead`. Offsets are the class layout
/// offsets. Render-only ivars are omitted.
pub struct BlockheadFields {
    /// 0x050, 0x6c bytes, saved verbatim as the NSData `state`.
    pub state: BlockheadState,
    /// 0xc0
    pub client_id: Option<String>,
    /// 0xc8
    pub client_name: Option<String>,
    /// 0x1e8..0x1ee
    pub hat_item_type: u16,
    pub shirt_item_type: u16,
    pub pants_item_type: u16,
    pub shoes_item_type: u16,
    /// 0x1f0..0x1f6
    pub hat_color_index: u16,
    pub shirt_color_index: u16,
    pub pants_color_index: u16,
    pub shoes_color_index: u16,
    /// 0x1f8..0x228; `Vector()` constructs (1,1,1,1).
    pub hat_item_color: [f32; 4],
    pub shirt_item_color: [f32; 4],
    pub pants_item_color: [f32; 4],
    pub shoes_item_color: [f32; 4],
    /// 0x2b8
    pub walk_timer: f32,
    /// 0x2d8 mutable copy of the world path; element 0 = GOAL, last = NEXT step.
    pub path: Option<Vec<PlistValue>>,
    /// 0x2e0
    pub travel_speed: f32,
    /// 0x2e4..0x2f0
    pub traverse_type: i32,
    pub traverse_from_key_frame: i32,
    pub traverse_to_key_frame: i32,
    pub terrain_difficulty: i32,
    /// 0x2f4..0x314
    pub prev_from_square: IntPair,
    pub from_square: IntPair,
    pub to_square: IntPair,
    pub next_to_square: IntPair,
    pub final_goal_square: IntPair,
    /// 0x31c
    pub load_requires_recalculation: bool,
    /// 0x320, 0x328
    pub from_tile: Option<TileRef>,
    pub to_tile: Option<TileRef>,
    /// 0x330, 0x331
    pub last_path_was_falling: bool,
    pub last_path_was_move_only: bool,
    /// 0x338
    pub interacting_tile: Option<TileRef>,
    /// 0x340 retained InventoryItem (identity matters)
    pub interaction_item: Option<ItemRef>,
    /// 0x348, 0x34c (-1 = none)
    pub interaction_item_index: i32,
    pub interaction_item_sub_index: i32,
    /// 0x350, 0x358, 0x360
    pub interaction_workbench: Option<ObjRef>,
    pub interaction_object: Option<ObjRef>,
    pub tapped_npc: Option<ObjRef>,
    /// 0x368, 0x36c
    pub chasing_npc: bool,
    pub last_known_npc_position: IntPair,
    /// 0x378
    pub tapped_ride_object: Option<ObjRef>,
    /// 0x380, 0x384
    pub chasing_ride_object: bool,
    pub last_known_ride_object_position: IntPair,
    /// 0x390
    pub ride_object: Option<ObjRef>,
    /// 0x398
    pub fishing_rod: Option<FishingRodRef>,
    /// 0x3a0, 0x3a4
    pub current_temperature: f32,
    pub damage_delay_timer: f32,
    /// 0x3a8, 0x3ac
    pub travel_fraction: f32,
    pub travel_fraction_normalized: f32,
    /// 0x3b0 (written, never read on the server)
    pub sound_timer: f32,
    /// 0x3b8
    pub selected_tool_index: i32,
    /// 0x3c0: exactly 8 slots; slot 0 = clothing container.
    pub inventory_items: Vec<SubSlot>,
    /// 0x3c8, 0x3c9
    pub path_needs_recalculated: bool,
    pub path_recalculation_is_fall_path: bool,
    /// 0x3ca
    pub debug_bool: bool,
    /// 0x3e0, 0x3e4
    pub ignoring_freeblocks_due_to_drop: bool,
    pub ignoring_freeblocks_due_to_drop_location: IntPair,
    /// 0x3f0
    pub name: Option<String>,
    /// 0x3f8..0x408
    pub idle_timer: f32,
    pub random_idle_wait: f32,
    pub random_animation_value_a: f32,
    pub random_animation_value_b: f32,
    pub random_animation_value_c: f32,
    /// 0x40c, 20 bytes
    pub skin_options: BlockheadSkinOptions,
    /// 0x860, 0x864, 0x86c
    pub waiting_for_path: bool,
    pub waiting_path_goal_pos: IntPair,
    pub waiting_for_path_interaction_type: u32,
    /// 0x870..0x872
    pub use_item: bool,
    pub using_item: bool,
    pub footstep_ping_pong: bool,
    /// 0x878: 8 entries of per-slot InventoryItem save blobs
    pub save_item_slots_array: Vec<SaveSlot>,
    /// 0x880 init (1,1)
    pub render_pos: Vector2,
    /// 0x888, 0x890, 0x8b0, 0x8b8
    pub inventory_changed_this_frame_slots: [bool; 8],
    pub sub_inventory_changed_this_frame_slots: [[bool; 4]; 8],
    pub inventory_usage_changed_this_frame_slots: [bool; 8],
    pub sub_inventory_usage_changed_this_frame_slots: [[bool; 4]; 8],
    /// 0x8d8
    pub inventory_needs_saving: bool,
    /// 0x8dc
    pub gather_timer_left_over: f32,
    /// 0x8e0, 0x8e1, 0x8e2
    pub underwater: bool,
    pub drowning_in_space: bool,
    pub local_gather_progress: u8,
    /// 0x8e8 (created only if `dynamicWorld.isClient`: stays `None` on the server;)
    pub unconfirmed_pickups: Option<std::collections::BTreeMap<u64, ItemRef>>,
    /// 0x8f0 (same)
    pub this_frame_pickup_requests: Option<crate::foundation::indexset::IndexSet>,
    /// 0x8f8
    pub path_extra_data: Option<PathExtraData>,
    /// 0x900
    pub view_radius: i32,
    /// 0x908
    pub action_queue: Vec<ActionRef>,
    /// 0x910
    pub next_action_delay: f32,
    /// 0x914, 0x915
    pub needs_harm_flash: bool,
    pub paused: bool,
    /// 0x920 (never read for logic on the server)
    pub last_dig_sound_time: f64,
    /// 0x92c..0x938
    pub clothing_increment_timer: f32,
    pub clothing_ice_increment_timer: f32,
    pub fuel_usage_increment_timer: f32,
    pub passive_item_usage_increment_timer: f32,
    /// 0x93c, 0x940
    pub recoil_timer: f32,
    pub recoil_direction: f32,
    /// 0x944
    pub cancel_simulate_due_to_collapse: bool,
    /// 0x946, 0x948
    pub fishing_rod_cast_x: i16,
    pub fishing_rod_cast_y: i16,
    /// 0x950
    pub is_client_blockhead_being_controlled_by_server: bool,
    /// 0x954, 0x958
    pub controlled_by_server_inactivity_timeout: f32,
    pub controlled_by_server_sanity_check_timeout: f32,
    /// 0x95c
    pub controlled_by_server_crystal_discrepency: i32,
    /// 0x960
    pub client_save_dir: Option<String>,
    /// 0x968
    pub interaction_object_dict_to_restore_when_object_loaded: Option<PlistValue>,
    /// 0x978 `std::vector<u16>`
    pub expected_craft_items: Vec<u16>,
    /// 0x990 (read by `isVisible`; zero from alloc)
    pub visible: bool,
    /// 0x9a0, 0x9a4
    pub tip_type: i32,
    pub elevator_power_use_counter: i32,
    /// 0x9a8
    pub is_in_jet_pack_free_flight_mode: bool,
    /// 0x9ac, 0x9b4, 0x9bc: ALL init (1.0, 1.0)
    pub jet_pack_incoming_acceleration: Vector2,
    pub jet_pack_velocity: Vector2,
    pub smoothed_jet_pack_velocity: Vector2,
    /// 0x9cc, 0x9d0, 0x9d4
    pub jetpack_send_update_timer: f32,
    pub jet_fuel_usage: f32,
    pub clamped_y_acceleration_for_fuel_usage_when_in_free_flight: f32,
    /// 0x9d8
    pub z_index: i32,
    /// 0x9e0, 0x20 bytes
    pub current_trade_mission: TradeMission,
    /// 0xa00
    pub current_net_queue_size: i32,
    /// 0xa04, 0xa24, 0xa34, 0xa44
    pub net_next_to_square: [IntPair; 4],
    pub next_terrain_difficulty: [i32; 4],
    pub next_traverse_type: [i32; 4],
    pub next_traverse_to_key_frame: [i32; 4],
    /// 0xa54, 0xa58, 0xa78
    pub next_interacting: [bool; 4],
    pub next_interaction_square: [IntPair; 4],
    pub next_interaction_type: [i32; 4],
    /// 0xa88, 0xa98
    pub next_animation_type: [i32; 4],
    pub next_sub_animation_type: [i32; 4],
    /// 0xaa8
    pub remote_interaction_item_type: i32,
    /// 0xaac
    pub is_currently_active_blockhead_according_to_net_data: bool,
    /// 0xaad, 0xab0, 0xab4
    pub waiting_for_fill_response: bool,
    pub waiting_for_fill_response_index: u32,
    pub waiting_for_fill_response_timer: f32,
    /// 0xab8
    pub has_had_remote_update: bool,
    /// 0xac0
    pub remote_client_fishing_rod_fish_unique_id: u64,
    /// 0xac8
    pub client_double_time_unlocked: bool,
    /// 0xacc
    pub no_remote_action_timer: f32,
    /// 0xad0, 0xad1
    pub being_controlled_by_d_pad: bool,
    pub current_movement_was_initiated_with_d_pad: bool,
    /// 0xad4, 0xadc, 0xae4: init (1,1)
    pub float_pos_when_d_pad_movement_started: Vector2,
    pub last_d_pad_total_distance: Vector2,
    pub last_d_pad_total_distance_smoothed: Vector2,
    // ---- Rust-only
    /// Typed back pointer to the concrete `World` (the `DynamicObjectBase.world` link is a `dyn WorldApi`).
    pub world_rc: Weak<World>,
}

const ONE2: Vector2 = Vector2 { x: 1.0, y: 1.0 };
const ZERO_PAIR: IntPair = IntPair { x: 0, y: 0 };

impl BlockheadFields {
    /// ObjC `alloc` memory: everything zero/None/false EXCEPT the nine `Vector`/`Vector2` fields, which the C++
    /// default constructors set to 1.0 in every lane (`.cxx_construct`,).
    pub fn alloc_zeroed() -> BlockheadFields {
        BlockheadFields {
            state: BlockheadState::default(),
            client_id: None,
            client_name: None,
            hat_item_type: 0,
            shirt_item_type: 0,
            pants_item_type: 0,
            shoes_item_type: 0,
            hat_color_index: 0,
            shirt_color_index: 0,
            pants_color_index: 0,
            shoes_color_index: 0,
            hat_item_color: [1.0; 4],
            shirt_item_color: [1.0; 4],
            pants_item_color: [1.0; 4],
            shoes_item_color: [1.0; 4],
            walk_timer: 0.0,
            path: None,
            travel_speed: 0.0,
            traverse_type: 0,
            traverse_from_key_frame: 0,
            traverse_to_key_frame: 0,
            terrain_difficulty: 0,
            prev_from_square: ZERO_PAIR,
            from_square: ZERO_PAIR,
            to_square: ZERO_PAIR,
            next_to_square: ZERO_PAIR,
            final_goal_square: ZERO_PAIR,
            load_requires_recalculation: false,
            from_tile: None,
            to_tile: None,
            last_path_was_falling: false,
            last_path_was_move_only: false,
            interacting_tile: None,
            interaction_item: None,
            interaction_item_index: 0,
            interaction_item_sub_index: 0,
            interaction_workbench: None,
            interaction_object: None,
            tapped_npc: None,
            chasing_npc: false,
            last_known_npc_position: ZERO_PAIR,
            tapped_ride_object: None,
            chasing_ride_object: false,
            last_known_ride_object_position: ZERO_PAIR,
            ride_object: None,
            fishing_rod: None,
            current_temperature: 0.0,
            damage_delay_timer: 0.0,
            travel_fraction: 0.0,
            travel_fraction_normalized: 0.0,
            sound_timer: 0.0,
            selected_tool_index: 0,
            inventory_items: Vec::new(),
            path_needs_recalculated: false,
            path_recalculation_is_fall_path: false,
            debug_bool: false,
            ignoring_freeblocks_due_to_drop: false,
            ignoring_freeblocks_due_to_drop_location: ZERO_PAIR,
            name: None,
            idle_timer: 0.0,
            random_idle_wait: 0.0,
            random_animation_value_a: 0.0,
            random_animation_value_b: 0.0,
            random_animation_value_c: 0.0,
            skin_options: BlockheadSkinOptions::default(),
            waiting_for_path: false,
            waiting_path_goal_pos: ZERO_PAIR,
            waiting_for_path_interaction_type: 0,
            use_item: false,
            using_item: false,
            footstep_ping_pong: false,
            save_item_slots_array: Vec::new(),
            render_pos: ONE2,
            inventory_changed_this_frame_slots: [false; 8],
            sub_inventory_changed_this_frame_slots: [[false; 4]; 8],
            inventory_usage_changed_this_frame_slots: [false; 8],
            sub_inventory_usage_changed_this_frame_slots: [[false; 4]; 8],
            inventory_needs_saving: false,
            gather_timer_left_over: 0.0,
            underwater: false,
            drowning_in_space: false,
            local_gather_progress: 0,
            unconfirmed_pickups: None,
            this_frame_pickup_requests: None,
            path_extra_data: None,
            view_radius: 0,
            action_queue: Vec::new(),
            next_action_delay: 0.0,
            needs_harm_flash: false,
            paused: false,
            last_dig_sound_time: 0.0,
            clothing_increment_timer: 0.0,
            clothing_ice_increment_timer: 0.0,
            fuel_usage_increment_timer: 0.0,
            passive_item_usage_increment_timer: 0.0,
            recoil_timer: 0.0,
            recoil_direction: 0.0,
            cancel_simulate_due_to_collapse: false,
            fishing_rod_cast_x: 0,
            fishing_rod_cast_y: 0,
            is_client_blockhead_being_controlled_by_server: false,
            controlled_by_server_inactivity_timeout: 0.0,
            controlled_by_server_sanity_check_timeout: 0.0,
            controlled_by_server_crystal_discrepency: 0,
            client_save_dir: None,
            interaction_object_dict_to_restore_when_object_loaded: None,
            expected_craft_items: Vec::new(),
            visible: false,
            tip_type: 0,
            elevator_power_use_counter: 0,
            is_in_jet_pack_free_flight_mode: false,
            jet_pack_incoming_acceleration: ONE2,
            jet_pack_velocity: ONE2,
            smoothed_jet_pack_velocity: ONE2,
            jetpack_send_update_timer: 0.0,
            jet_fuel_usage: 0.0,
            clamped_y_acceleration_for_fuel_usage_when_in_free_flight: 0.0,
            z_index: 0,
            current_trade_mission: TradeMission::default(),
            current_net_queue_size: 0,
            net_next_to_square: [ZERO_PAIR; 4],
            next_terrain_difficulty: [0; 4],
            next_traverse_type: [0; 4],
            next_traverse_to_key_frame: [0; 4],
            next_interacting: [false; 4],
            next_interaction_square: [ZERO_PAIR; 4],
            next_interaction_type: [0; 4],
            next_animation_type: [0; 4],
            next_sub_animation_type: [0; 4],
            remote_interaction_item_type: 0,
            is_currently_active_blockhead_according_to_net_data: false,
            waiting_for_fill_response: false,
            waiting_for_fill_response_index: 0,
            waiting_for_fill_response_timer: 0.0,
            has_had_remote_update: false,
            remote_client_fishing_rod_fish_unique_id: 0,
            client_double_time_unlocked: false,
            no_remote_action_timer: 0.0,
            being_controlled_by_d_pad: false,
            current_movement_was_initiated_with_d_pad: false,
            float_pos_when_d_pad_movement_started: ONE2,
            last_d_pad_total_distance: ONE2,
            last_d_pad_total_distance_smoothed: ONE2,
            world_rc: Weak::new(),
        }
    }
}

impl Default for BlockheadFields {
    fn default() -> Self {
        BlockheadFields::alloc_zeroed()
    }
}

// ====================================================================================================
// Blockhead
// ====================================================================================================

/// `@interface Blockhead: DynamicObject`.
pub struct Blockhead {
    /// DynamicObject ivars 0x08..0x4c (object_model).
    pub base: RefCell<DynamicObjectBase>,
    /// Every Blockhead ivar. Other blockhead modules use this field directly.
    pub bh: RefCell<BlockheadFields>,
}

impl Blockhead {
    /// The item type of the item being interacted with (0 without one).
    pub(crate) fn interaction_item_type(&self) -> i32 {
        self.bh.borrow().interaction_item.as_ref().map_or(0, |i| i.borrow().item_type.0 as i32)
    }
    /// `[pathExtraData[@"craftCountOrExtraData"] intValue]` (0 without path extra data).
    pub(crate) fn craft_count_or_extra_data(&self) -> i32 {
        self.bh.borrow().path_extra_data.as_ref().map_or(0, |d| d.int_for_key("craftCountOrExtraData"))
    }
    /// `[Blockhead alloc]`: zeroed ivars vectors).
    pub fn alloc(self_weak: ObjWeak) -> Blockhead {
        Blockhead {
            base: RefCell::new(DynamicObjectBase::alloc_zeroed_with(self_weak)),
            bh: RefCell::new(BlockheadFields::alloc_zeroed()),
        }
    }

    /// Strong handle to self (`base.self_weak.upgrade()`).
    pub fn me(&self) -> ObjRef {
        DynamicObject::me(self)
    }

    /// The concrete `World` this blockhead was built with (`[self world]`); `None` when the world is gone.
    pub fn world_rc(&self) -> Option<Rc<World>> {
        self.bh.borrow().world_rc.upgrade()
    }

    /// `[self dynamicWorld]`
    pub fn dw(&self) -> Option<DynamicWorldRef> {
        self.base.borrow().dynamic_world.as_ref().and_then(|w| w.upgrade())
    }

    /// Copy of `state` (all fields are `Copy`).
    pub fn st(&self) -> BlockheadState {
        self.bh.borrow().state
    }

    /// `[self pos]`
    pub fn pos(&self) -> IntPair {
        self.base.borrow().pos
    }

    /// `[self floatPos]`
    pub fn float_pos(&self) -> Vector2 {
        self.base.borrow().float_pos
    }

    /// `[self isNet]`
    pub fn is_net(&self) -> bool {
        self.base.borrow().is_net
    }

    /// `[self uniqueID]`
    pub fn unique_id(&self) -> u64 {
        self.base.borrow().unique_id
    }

    /// `[self setUpdateNeedsToBeSent:]`
    pub fn set_update_needs_to_be_sent(&self, v: bool) {
        self.base.borrow_mut().update_needs_to_be_sent = v;
    }

    // ---- trivial getters ----------------------------------------------------------------

    /// `-health`
    pub fn health(&self) -> f32 {
        self.bh.borrow().state.health
    }
    /// `-death`
    pub fn death(&self) -> f32 {
        self.bh.borrow().state.death
    }
    /// `-happiness`
    pub fn happiness(&self) -> f32 {
        self.bh.borrow().state.happiness
    }
    /// `-fullness`
    pub fn fullness(&self) -> f32 {
        self.bh.borrow().state.fullness
    }
    /// `-meditationProgress`
    pub fn meditation_progress(&self) -> f32 {
        self.bh.borrow().state.meditation_progress
    }
    /// `-environment`
    pub fn environment(&self) -> f32 {
        self.bh.borrow().state.environment
    }
    /// `-environmentTemperature`
    pub fn environment_temperature(&self) -> f32 {
        self.bh.borrow().state.environment_temperature
    }
    /// `-environmentExposure`
    pub fn environment_exposure(&self) -> f32 {
        self.bh.borrow().state.environment_exposure
    }
    /// `-environmentLight`
    pub fn environment_light(&self) -> f32 {
        self.bh.borrow().state.environment_light
    }
    /// `-drownFraction`
    pub fn drown_fraction(&self) -> f32 {
        self.bh.borrow().state.drown_fraction
    }
    /// `-regenerationProgress`
    pub fn regeneration_progress(&self) -> f32 {
        self.bh.borrow().state.regeneration_progress
    }
    /// `-currentAnimationType`
    pub fn current_animation_type(&self) -> i32 {
        self.bh.borrow().state.animation_type as i32
    }

    /// `-energy`: `((double)state.energy - 0.25) / 0.75` in f64, clamped below at 0.0, narrowed to f32 (no upper
    /// clamp).
    pub fn energy(&self) -> f32 {
        let e = self.bh.borrow().state.energy;
        let x = ((e as f64) - 0.25) / 0.75;
        (if x <= 0.0 { 0.0 } else { x }) as f32
    }

    /// `-hasCoffeeEnergy`: `0.01f < state.coffeeEnergyTimer`.
    pub fn has_coffee_energy(&self) -> bool {
        0.01f32 < self.bh.borrow().state.coffee_energy_timer
    }

    // ---- accessors other modules read ---------------------------------------

    /// `-clientID`
    pub fn client_id(&self) -> Option<String> {
        self.bh.borrow().client_id.clone()
    }
    /// `-setClientID:`
    pub fn set_client_id(&self, id: &str) {
        self.bh.borrow_mut().client_id = Some(id.to_string());
    }
    /// `-clientName`
    pub fn client_name(&self) -> Option<String> {
        self.bh.borrow().client_name.clone()
    }
    /// `-setClientName:`
    pub fn set_client_name(&self, n: Option<&str>) {
        self.bh.borrow_mut().client_name = n.map(|s| s.to_string());
    }
    /// `-clientSaveDir`
    pub fn client_save_dir(&self) -> Option<String> {
        self.bh.borrow().client_save_dir.clone()
    }
    /// `-name`
    pub fn name(&self) -> Option<String> {
        self.bh.borrow().name.clone()
    }
    /// `-viewRadius`
    pub fn view_radius(&self) -> i32 {
        self.bh.borrow().view_radius
    }
    /// `-isClientBlockheadBeingControlledByServer`
    pub fn is_client_blockhead_being_controlled_by_server(&self) -> bool {
        self.bh.borrow().is_client_blockhead_being_controlled_by_server
    }
    /// `-paused`
    pub fn paused(&self) -> bool {
        self.bh.borrow().paused
    }
    /// `-regenerating`.
    pub fn regenerating(&self) -> bool {
        self.bh.borrow().state.regenerating != 0
    }

    // ---- ----------------------------------------------------------------------------------

    /// `-clothingItemAtIndex:` `i < 0` fails the unsigned compare; an empty container, an index
    /// past `subItems.count` or an empty group gives nil.
    pub fn clothing_item_at_index(&self, i: i32) -> Option<ItemRef> {
        let b = self.bh.borrow();
        let cont = b.inventory_items.first()?;
        let first = cont.borrow().first()?.clone();
        let item = first.borrow();
        let subs = item.sub_items.as_ref()?;
        if (i as i64 as u64) < subs.len() as u64 {
            let g = subs[i as usize].borrow();
            if !g.is_empty() {
                return Some(g[0].clone());
            }
        }
        None
    }

    // ---- ----------------------------------------------------------------------------------

    /// `-updateAnimation`. The switch uses the key-frame TO type (+0x2ec), not the traverse type.
    pub fn update_animation(&self) {
        let is_net = self.is_net();
        let mut b = self.bh.borrow_mut();
        if !is_net {
            b.state.animation_type = va::STAND;
            b.state.animation_type = match b.traverse_to_key_frame as u32 {
                kf::BOB => va::BOB,
                kf::HANG_BACK | kf::HANG_RIGHT | kf::HANG_LEFT => va::HANG,
                kf::CRAWL => va::LIE,
                kf::MONKEY_HANG => va::MONKEY_HANG,
                kf::FLY => va::FLY_JETPACK,
                _ => 0,
            };
            if b.is_in_jet_pack_free_flight_mode {
                b.state.animation_type = va::FLY_JETPACK;
            }
        }
        // applies for net replicas too
        let ts_y = b.to_square.y;
        let mut bp = self.base.borrow_mut();
        if b.state.animation_type == va::LIE {
            bp.float_pos.y = (-0.5f32) + (ts_y as f32);
        } else if b.state.animation_type == va::MONKEY_HANG {
            bp.float_pos.y = 0.2f32 + (ts_y as f32);
        }
    }

    // ---- ---------------------------------------------------------------------------------

    /// `-goalInteractionForNPCChaseForNPC:withItemType:`
    /// only one not guarded by `isNPC`).
    pub fn goal_interaction_for_npc_chase_for_npc_with_item_type(&self, npc: &ObjRef, item_type: ItemType) -> u32 {
        let h = host();
        let t = item_type.0;
        let is_npc = h.is_npc(npc);
        // 1
        if t != 0 && is_npc && t as i32 == h.npc_food_item_type(npc) {
            return it::FEED_NPC;
        }
        // 2
        if (t == 302 || t == 102) && is_npc && h.npc_can_be_captured_by_blockhead(npc, self, item_type) {
            return it::CAPTURE;
        }
        // 3
        if t == 102 && is_npc && h.npc_can_be_milked_by_blockhead(npc, self) {
            return it::HARVEST_NPC;
        }
        // 4
        if t == 322 && is_npc && h.npc_can_be_shaved_by_blockhead(npc, self) {
            return it::HARVEST_NPC;
        }
        // 5
        if npc.object_type() == DYNAMIC_OBJECT_TYPE_NPC_DONKEY
            && t == 74
            && h.ridable_blockhead_can_ride_using_item(npc, self, 74)
        {
            return it::RIDE;
        }
        // 6
        if is_npc && h.npc_belongs_to_player_with_blockhead(npc, self) {
            return it::INSPECT_NPC;
        }
        // 7
        if t == 42 || t == 257 {
            return it::FIRE_RANGED_WEAPON;
        }
        // 8
        it::STAB
    }

    // ---- ---------------------------------------------------------------------------------

    /// `-tileIsLitForSelf:atPos:` The per-client branch is live for offline blockheads.
    pub fn tile_is_lit_for_self_at_pos_impl(&self, tile: &Tile, pos: IntPair) -> bool {
        let (ctl, cid) = {
            let b = self.bh.borrow();
            (b.is_client_blockhead_being_controlled_by_server, b.client_id.clone())
        };
        if !ctl {
            tile.light != 0
        } else {
            host().tile_is_lit_for_client_at_pos_tile(cid.as_deref(), pos, None, Some(*tile))
        }
    }
}

// ====================================================================================================
// pure state predicates
// ====================================================================================================

/// `-asleep`: `subAnimationType == 5 || == 6`.
pub fn state_asleep(s: &BlockheadState) -> bool {
    s.sub_animation_type == vsa::LIE_ON_BED || s.sub_animation_type == vsa::LIE_ON_GROUND
}
/// `-meditating`: `subAnimationType == 7`.
pub fn state_meditating(s: &BlockheadState) -> bool {
    s.sub_animation_type == vsa::MEDITATING
}

// ====================================================================================================
// impl DynamicObject
// ====================================================================================================

impl DynamicObject for Blockhead {
    fn base(&self) -> &RefCell<DynamicObjectBase> {
        &self.base
    }
    fn class_info(&self) -> &'static ClassInfo {
        &BLOCKHEAD_CLASS
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    /// `-objectType` = `DYNAMIC_OBJECT_TYPE_BLOCKHEAD`.
    fn object_type(&self) -> i32 {
        DYNAMIC_OBJECT_TYPE_BLOCKHEAD
    }
    /// `-requiresPhysicalBlock`: `isNet ? dw.isServer: YES`.
    fn requires_physical_block(&self) -> bool {
        if self.is_net() {
            self.dw().map_or(false, |d| d.is_server())
        } else {
            true
        }
    }
    /// `-updatePosition:`.
    fn update_position(&self, me: &ObjRef, new_position: IntPair) {
        let _ = me;
        let before = self.pos();
        host().update_position(self, new_position);
        // Mods (not in the original; `None` without mods): `player-move` when a player's blockhead changed tile.
        if let Some(m) = crate::mods::hooks().filter(|m| m.wants(crate::mods::Optional::PlayerMove)) {
            if let (Some(c), after) = (self.client_id(), self.pos()) {
                if after != before {
                    m.player_move(&c, before, after);
                }
            }
        }
    }
    /// `-worldChanged:` is empty.
    fn world_changed(&self, _me: &ObjRef, _positions: &mut Vec<IntPair>) {}
    /// `-worldContentsChanged:` is empty.
    fn world_contents_changed(&self, _me: &ObjRef, _positions: &mut Vec<IntPair>) {}
    fn get_save_dict(&self, _me: &ObjRef) -> PlistDict {
        self.get_save_dict_including_workbench_or_interation_object(false)
    }
    fn creation_net_data_for_client(&self, _me: &ObjRef, client_id: Option<&str>) -> Option<Vec<u8>> {
        Some(self.creation_net_data_for_client_impl(client_id))
    }
    fn update_net_data_for_client(&self, _me: &ObjRef, client_id: Option<&str>) -> Option<Vec<u8>> {
        Some(self.update_net_data_for_client_impl(client_id))
    }
    fn remote_update(&self, me: &ObjRef, net_data: &[u8]) {
        self.remote_update_impl(me, net_data)
    }
    fn remote_creation_data_update(&self, me: &ObjRef, net_data: &[u8]) {
        self.remote_creation_data_update_impl(me, net_data)
    }
    fn update_accurate_dt_is_simulation(&self, me: &ObjRef, dt: f32, accurate_dt: f32, is_simulation: bool) {
        super::update::update_accurate_dt_is_simulation(self, me, dt, accurate_dt, is_simulation)
    }
    fn dealloc(&self, me: &ObjRef) {
        self.dealloc_impl(me)
    }
    fn as_path_user(&self) -> Option<&dyn PathUserDynamicObject> {
        Some(self)
    }
    fn as_harmable(&self) -> Option<&dyn HarmableDynamicObject> {
        Some(self)
    }
    fn as_blockhead(&self) -> Option<&dyn BlockheadApi> {
        Some(self)
    }
    /// `renderPos`.
    fn render_pos(&self) -> Vector2 {
        self.bh.borrow().render_pos
    }
}

impl BlockheadApi for Blockhead {
    fn client_id(&self) -> Option<String> {
        Blockhead::client_id(self)
    }
    /// `-localNetID`: `(!ctl && !isNet) ? [dynamicWorld localNetID]: clientID`.
    fn local_net_id(&self) -> Option<String> {
        if !self.is_client_blockhead_being_controlled_by_server() && !self.is_net() {
            Some(host().dynamic_world_local_net_id())
        } else {
            self.client_id()
        }
    }
    fn is_client_blockhead_being_controlled_by_server(&self) -> bool {
        Blockhead::is_client_blockhead_being_controlled_by_server(self)
    }
    fn is_interacting_with_any_interaction_object(&self) -> bool {
        host().is_interacting_with_any_interaction_object(self)
    }
    fn set_interaction_object(&self, o: Option<&ObjRef>) {
        host().set_interaction_object(self, o)
    }
    fn stop_interacting(&self) {
        Blockhead::stop_interacting(self)
    }
}

// ----------------------------------------------------------------------------------------------------
// PathUserDynamicObject / HarmableDynamicObject: thin forwarders to the sibling modules via the host
// ----------------------------------------------------------------------------------------------------

impl PathUserDynamicObject for Blockhead {
    fn tile_is_lit_for_self_at_pos(&self, tile: &Tile, pos: IntPair) -> bool {
        self.tile_is_lit_for_self_at_pos_impl(tile, pos)
    }
    /// : returns `traverseToKeyFrame`.
    fn current_traverse_to_key_frame(&self) -> i32 {
        host().current_traverse_to_key_frame(self)
    }
    fn next_pos(&self) -> IntPair {
        host().next_pos(self)
    }
    fn info_for_path_recalculation(&self) -> Option<PlistDict> {
        host().info_for_path_recalculation(self)
    }
    fn set_path_type_goal_interaction_extra_data(
        &self,
        _me: &ObjRef,
        path: Option<Vec<PlistValue>>,
        ty: i32,
        goal_interaction: i32,
        extra: Option<PlistValue>,
    ) {
        host().set_path_type_goal_interaction_extra_data(
            self,
            path,
            ty,
            goal_interaction,
            PathExtraData::from_plist(extra.as_ref()),
        )
    }
    fn set_waiting_for_path_to_pos(&self, p: IntPair) {
        host().set_waiting_for_path_to_pos(self, p)
    }
    fn set_no_longer_waiting_for_path(&self) {
        host().set_no_longer_waiting_for_path(self)
    }
    fn waiting_for_path(&self) -> bool {
        self.bh.borrow().waiting_for_path
    }
    fn can_crawl(&self) -> bool {
        host().can_crawl(self)
    }
    fn can_fly(&self) -> bool {
        host().can_fly(self)
    }
    fn path_needs_recalculated(&self) -> bool {
        self.bh.borrow().path_needs_recalculated
    }
    fn set_path_needs_recalculated(&self, v: bool) {
        self.bh.borrow_mut().path_needs_recalculated = v;
    }
}

impl HarmableDynamicObject for Blockhead {
    fn tap_is_within_body_radius(&self, tap: Vector2) -> bool {
        host().tap_is_within_body_radius(self, tap)
    }
    fn hit_with_force_blockhead(&self, _me: &ObjRef, force: i32, b: Option<&ObjRef>) {
        host().hit_with_force_blockhead(self, force, b)
    }
    fn will_die_if_hit_by_force(&self, force: i32) -> bool {
        host().will_die_if_hit_by_force(self, force)
    }
    fn is_visible(&self) -> bool {
        self.bh.borrow().visible
    }
    fn center(&self) -> Vector2 {
        host().center(self)
    }
}

// ====================================================================================================
// The host seam
// ====================================================================================================

crate::hooks::hook_trait! {
/// Calls into modules that were not implemented when `blockhead_core` was written. Every method has a default:
/// spec-derived where the spec is a trivial read, otherwise inert (returns nil/0/NO and does nothing). The
/// integrator implements this once (a thin adapter over the real `World`, `DynamicWorld`, registry, NPCs,
/// workbenches, plants, `FishingRod`, `Action`, and the `impl Blockhead` blocks of blockhead_inventory /
/// blockhead_actions) and installs it with [`set_host`].
#[allow(unused_variables)]
pub trait BlockheadHost, stub BlockheadHostStub {
    // ================= sibling Blockhead methods (blockhead_inventory -, blockhead_actions /) ====
    /// `-updatePosition:` override. Default = the DynamicObject version.
    fn update_position(&self, b: &Blockhead, p: IntPair) {
        base_update_position(&b.me(), p)
    }
    /// `-stopRiding`
    fn stop_riding(&self, b: &Blockhead) {}
    /// `-pickupDynamicObject:`
    fn pickup_dynamic_object(&self, b: &Blockhead, o: &ObjRef) {}
    /// `-interactionTypeForTile:atPos:item:pickupRejectedDueToInventoryFull:includeActions:faceIndex:allowProtectedActions:`
    /// Returns an `InteractionType` raw value.
    fn interaction_type_for_tile_at_pos_item_pickup_rejected_due_to_inventory_full_include_actions_face_index_allow_protected_actions(
        &self,
        b: &Blockhead,
        tile: Option<TileRef>,
        pos: IntPair,
        item: Option<&ItemRef>,
        rejected: &mut bool,
        include_actions: bool,
        face_index: i32,
        allow_protected_actions: bool,
    ) -> u32 {
        0
    }
    /// `-goodOrBadInteractionForAction:`
    fn good_or_bad_interaction_for_action(&self, b: &Blockhead, action: Option<&ActionRef>) -> InteractionTestResult {
        InteractionTestResult { good_or_bad: 0, render_item_type: 0, render_item_data_a: 0, render_item_data_b: 0 }
    }
    /// `-updateGatherSpeedAndAnimationForCurrentInterationAndItem`
    fn update_gather_speed_and_animation_for_current_interation_and_item(&self, b: &Blockhead) {}
    /// `-isAddingFuelToAnyWorkbench`
    fn is_adding_fuel_to_any_workbench(&self, b: &Blockhead) -> bool {
        false
    }
    /// `-currentTraverseToKeyFrame`: returns `traverseToKeyFrame`.
    fn current_traverse_to_key_frame(&self, b: &Blockhead) -> i32 {
        b.bh.borrow().traverse_to_key_frame
    }
    /// `-nextPos`
    fn next_pos(&self, b: &Blockhead) -> IntPair {
        b.bh.borrow().to_square
    }
    /// `-infoForPathRecalculation`
    fn info_for_path_recalculation(&self, b: &Blockhead) -> Option<PlistDict> {
        None
    }
    /// `-setPath:type:goalInteraction:extraData:`
    fn set_path_type_goal_interaction_extra_data(
        &self,
        b: &Blockhead,
        path: Option<Vec<PlistValue>>,
        ty: i32,
        goal_interaction: i32,
        extra: Option<PathExtraData>,
    ) {
        let mut f = b.bh.borrow_mut();
        f.path = path;
        f.path_extra_data = extra;
    }
    /// `-setWaitingForPathToPos:`
    fn set_waiting_for_path_to_pos(&self, b: &Blockhead, p: IntPair) {
        let mut f = b.bh.borrow_mut();
        f.waiting_for_path = true;
        f.waiting_path_goal_pos = p;
    }
    /// `-setNoLongerWaitingForPath`
    fn set_no_longer_waiting_for_path(&self, b: &Blockhead) {
        b.bh.borrow_mut().waiting_for_path = false;
    }
    /// `-canCrawl`
    fn can_crawl(&self, b: &Blockhead) -> bool {
        false
    }
    /// `-canFly`
    fn can_fly(&self, b: &Blockhead) -> bool {
        false
    }
    /// `-tapIsWithinBodyRadius:` (CLIENT-ONLY in practice)
    fn tap_is_within_body_radius(&self, b: &Blockhead, tap: Vector2) -> bool {
        false
    }
    /// `-hitWithForce:blockhead:`
    fn hit_with_force_blockhead(&self, b: &Blockhead, force: i32, hitter: Option<&ObjRef>) {}
    /// `-willDieIfHitByForce:`
    fn will_die_if_hit_by_force(&self, b: &Blockhead, force: i32) -> bool {
        false
    }
    /// `-center`
    fn center(&self, b: &Blockhead) -> Vector2 {
        let p = b.float_pos();
        Vector2 { x: p.x, y: p.y + 1.0 }
    }
    /// `-isInteractingWithAnyInteractionObject`
    fn is_interacting_with_any_interaction_object(&self, b: &Blockhead) -> bool {
        false
    }
    /// `-setInteractionObject:`
    fn set_interaction_object(&self, b: &Blockhead, o: Option<&ObjRef>) {
        b.bh.borrow_mut().interaction_object = o.cloned();
    }
    /// `-setInteractionWorkbench:`
    fn set_interaction_workbench(&self, b: &Blockhead, o: Option<&ObjRef>) {
        b.bh.borrow_mut().interaction_workbench = o.cloned();
    }
    /// `-canPickUpItemOfType:subItems:`: `== 1` means yes. Returns the raw int.
    fn can_pick_up_item_of_type_sub_items(
        &self,
        b: &Blockhead,
        item_type: ItemType,
        sub_items: Option<&[Vec<ItemRef>]>,
    ) -> i32 {
        1
    }
    /// `-canPickUpItemOfType:subItems:dataA:dataB:`
    fn can_pick_up_item_of_type_sub_items_data_a_data_b(
        &self,
        b: &Blockhead,
        item_type: ItemType,
        sub_items: Option<&[Vec<ItemRef>]>,
        data_a: u16,
        data_b: u16,
    ) -> i32 {
        1
    }
    /// `-addItemToInventory:flash:disableWarpCheck:forceSlotIndex:`. Default: a bare append to the
    /// forced slot (no stacking), enough for the starter items.
    fn add_item_to_inventory_flash_disable_warp_check_force_slot_index(
        &self,
        b: &Blockhead,
        item: &ItemRef,
        flash: bool,
        disable_warp_check: bool,
        force_slot_index: i32,
    ) {
        let f = b.bh.borrow();
        if force_slot_index >= 0 && (force_slot_index as usize) < f.inventory_items.len() {
            f.inventory_items[force_slot_index as usize].borrow_mut().push(item.clone());
        }
    }
    /// `-addItemToInventory:flash:`
    fn add_item_to_inventory_flash(&self, b: &Blockhead, item: &ItemRef, flash: bool) {
        self.add_item_to_inventory_flash_disable_warp_check_force_slot_index(b, item, flash, false, -1)
    }
    /// `-pickupFreeblockIfPossible:inTile:intentional:`
    fn pickup_freeblock_if_possible_in_tile_intentional(
        &self,
        b: &Blockhead,
        fb: &ObjRef,
        tile: Option<TileRef>,
        intentional: bool,
    ) -> bool {
        false
    }
    /// `-incrementUsageOfInteractionItem:`
    fn increment_usage_of_interaction_item(&self, b: &Blockhead, used_in_attack: bool) {}
    /// `-incrementUsageOfClothing`
    fn increment_usage_of_clothing(&self, b: &Blockhead) {}
    /// `-incrementUsageOfIceClothing`
    fn increment_usage_of_ice_clothing(&self, b: &Blockhead) {}
    /// `-incrementFuelUsage`
    fn increment_fuel_usage(&self, b: &Blockhead) {}
    /// `-incrementPassiveItemUsage`
    fn increment_passive_item_usage(&self, b: &Blockhead) {}
    /// `-incrementDamageOfArmorClothing:`
    fn increment_damage_of_armor_clothing(&self, b: &Blockhead, damage: f32) {}
    /// `-inventoryWasChanged:subIndex:wasUsage:`
    fn inventory_was_changed_sub_index_was_usage(&self, b: &Blockhead, index: i32, sub_index: i32, was_usage: bool) {}
    /// `-dropInventoryItemsAtIndex:subIndex:count:ignoreFreeblocks:`
    fn drop_inventory_items_at_index_sub_index_count_ignore_freeblocks(
        &self,
        b: &Blockhead,
        index: i32,
        sub_index: i32,
        count: i32,
        ignore_freeblocks: bool,
    ) {
    }
    /// `-removeInteractionItem:`
    fn remove_interaction_item(&self, b: &Blockhead, flag: bool) {}
    /// `-hasInteractionInventoryItemAvailable`
    fn has_interaction_inventory_item_available(&self, b: &Blockhead) -> bool {
        false
    }
    /// `-canUseDynamicObject:`
    fn can_use_dynamic_object(&self, b: &Blockhead, o: &ObjRef) -> bool {
        true
    }
    /// `-stopAllActions`
    fn stop_all_actions(&self, b: &Blockhead) {
        b.bh.borrow_mut().action_queue.clear();
    }
    /// `-stopFishing`
    fn stop_fishing(&self, b: &Blockhead) {}
    /// `-dieForGood`
    fn die_for_good(&self, b: &Blockhead) {}
    /// `-unableToWorkReason`: 0 ok, 1 exhausted, 2 starving.
    fn unable_to_work_reason(&self, b: &Blockhead) -> i32 {
        0
    }
    /// `-jetpackFuelCount`
    fn jetpack_fuel_count(&self, b: &Blockhead) -> i32 {
        if b.is_net() {
            10
        } else {
            0
        }
    }
    /// `-updateJetMotion:`
    fn update_jet_motion(&self, b: &Blockhead, dt: f32) {}
    /// `-freeBlockPickupRadius`
    fn free_block_pickup_radius(&self, b: &Blockhead) -> f32 {
        let f = b.bh.borrow();
        if f.ride_object.is_none() && !f.is_in_jet_pack_free_flight_mode {
            if f.shirt_item_type == 278 && !b.is_net() {
                // jetpackFuelCount > 0 is evaluated by the sibling; the default keeps the plain radius
            }
            return 0.5;
        }
        1.0
    }
    /// `-dpadShouldBeDisplayed` (CLIENT-ONLY: false)
    fn dpad_should_be_displayed(&self, b: &Blockhead) -> bool {
        false
    }
    /// `-currentItem`
    fn current_item(&self, b: &Blockhead) -> Option<ItemRef> {
        None
    }
    /// `-currentInteractionRequiresHumanInput`
    fn current_interaction_requires_human_input(&self, b: &Blockhead) -> bool {
        false
    }
    /// `Action -inProgress`
    fn action_in_progress(&self, a: &ActionRef) -> bool {
        false
    }
    /// `Action -setInProgress:`
    fn action_set_in_progress(&self, a: &ActionRef, v: bool) {}
    /// `Action -goalTilePos`
    fn action_goal_tile_pos(&self, a: &ActionRef) -> IntPair {
        IntPair { x: 0, y: 0 }
    }
    /// `Action -getSaveDict`
    fn action_get_save_dict(&self, a: &ActionRef) -> PlistValue {
        PlistValue::mutable_dict()
    }
    /// `[[Action alloc] initWithSaveDict:inventoryItems:]`
    fn action_init_with_save_dict_inventory_items(&self, dict: &PlistValue, inventory_items: &[SubSlot]) -> ActionRef {
        Rc::new(())
    }
    /// `Action -interactionTestResult`
    fn action_interaction_test_result(&self, a: &ActionRef) -> InteractionTestResult {
        InteractionTestResult { good_or_bad: 0, render_item_type: 0, render_item_data_a: 0, render_item_data_b: 0 }
    }
    /// `Action -interactionObjectID`
    fn action_interaction_object_id(&self, a: &ActionRef) -> i64 {
        0
    }
    /// `[[action inventoryChange] objectForKey:key]` as an int, `None` when the key is missing.
    fn action_inventory_change_int_for_key(&self, a: &ActionRef, key: &str) -> Option<i32> {
        None
    }
    /// `[action setInventoryChange:]` with `d[key] = @(value)` on a mutable copy.
    fn action_set_inventory_change_int_for_key(&self, a: &ActionRef, key: &str, value: i32) {}

    // ================= World (world_core / world_ops) ====================================================
    /// `-[World addSimulationEventOfType:forBlockhead:extraData:]` (world_core 5.9). LIVE for ctl blockheads.
    fn add_simulation_event_of_type_for_blockhead_extra_data(
        &self,
        event_type: i32,
        b: &ObjRef,
        extra: SimulationEventExtra,
    ) {
    }
    /// `-[World reportAchievementWithIdentifier:]` (empty on the server)
    fn report_achievement_with_identifier(&self, id: &str) {}
    /// `-[World abortInProgressPathIfForBlockhead:]`
    fn abort_in_progress_path_if_for_blockhead(&self, b: &ObjRef) {}
    /// `-[World sendDamage:forNetBlockhead:recoil:]`: forwarded to the blockhead's world (an inert default left PvP
    /// damage unsent in production).
    #[provided]
    fn send_damage_for_net_blockhead_recoil(&self, damage: f32, b: &ObjRef, recoil: bool) {
        if let Some(w) = crate::objects::handle::downcast::<Blockhead>(b).and_then(|x| x.world_rc()) {
            w.send_damage_for_net_blockhead_recoil(damage, b, recoil);
        }
    }
    /// `-[World fireProjectileFrom:to:at:fireItemType:firer:]`
    fn fire_projectile_from_to_at_fire_item_type_firer(
        &self,
        from: Vector2,
        to: Vector2,
        at: Option<&ObjRef>,
        fire_item_type: i32,
        firer: Option<&ObjRef>,
    ) {
    }
    /// `-[World tileIsLitForClient:atPos:tile:]`
    fn tile_is_lit_for_client_at_pos_tile(
        &self,
        client_id: Option<&str>,
        pos: IntPair,
        tile_ref: Option<TileRef>,
        tile: Option<Tile>,
    ) -> bool {
        false
    }
    /// `-[World fullyLoadIfNeededAroundPos:clientLightBlockIndex:forBlockhead:]` (`-1` = none)
    fn fully_load_if_needed_around_pos_client_light_block_index_for_blockhead(
        &self,
        pos: IntPair,
        light_block_index: i32,
        b: Option<&ObjRef>,
    ) {
    }
    /// `-[World placeWorkbenchOfType:atPos:saveDict:placedByClient:placedByBlockhead:placedByClientName:]`
    fn place_workbench_of_type_at_pos_save_dict_placed_by_client_placed_by_blockhead_placed_by_client_name(
        &self,
        workbench_type: i32,
        pos: IntPair,
        save_dict: Option<&PlistValue>,
        placed_by_client: Option<&str>,
        placed_by_blockhead: &ObjRef,
        placed_by_client_name: Option<&str>,
    ) {
    }
    /// `-[World placeInteractionObjectWithItem:atPos:saveDict:placedByClient:placedByBlockhead:placedByClientName:]`
    fn place_interaction_object_with_item_at_pos_save_dict_placed_by_client_placed_by_blockhead_placed_by_client_name(
        &self,
        item: &ItemRef,
        pos: IntPair,
        save_dict: Option<&PlistValue>,
        placed_by_client: Option<&str>,
        placed_by_blockhead: &ObjRef,
        placed_by_client_name: Option<&str>,
    ) -> Option<ObjRef> {
        None
    }
    /// `-[World fillTile:atPos:withType:]`
    fn fill_tile_at_pos_with_type(&self, tile: Option<TileRef>, pos: IntPair, item_type: i32) {}
    /// `-[World fillTile:atPos:withType:dataA:dataB:placedByClient:saveDict:placedByBlockhead:placedByClientName:]`
    fn fill_tile_at_pos_with_type_data_a_data_b_placed_by_client_save_dict_placed_by_blockhead_placed_by_client_name(
        &self,
        tile: Option<TileRef>,
        pos: IntPair,
        item_type: i32,
        data_a: u16,
        data_b: u16,
        placed_by_client: Option<&str>,
        save_dict: Option<&PlistValue>,
        placed_by_blockhead: Option<&ObjRef>,
        placed_by_client_name: Option<&str>,
    ) {
    }
    /// `-[World removeTileAtWorldX:worldY:createContentsFreeblockCount:createForegroundContentsFreeblockCount:removeBlockhead:onlyRemoveContents:onlyRemoveForegroundContents:]`
    fn remove_tile_at_world_x_world_y_create_contents_freeblock_count_create_foreground_contents_freeblock_count_remove_blockhead_only_remove_c_ontents_only_remove_foreground_contents(
        &self,
        x: i32,
        y: i32,
        contents_count: i32,
        foreground_count: i32,
        remove_blockhead: &ObjRef,
        only_remove_contents: bool,
        only_remove_foreground_contents: bool,
    ) {
    }
    /// `-[World removeBackWallAtPos:removeBlockhead:]`
    fn remove_back_wall_at_pos_remove_blockhead(&self, pos: IntPair, b: &ObjRef) {}
    /// `-[World removeWaterTileAtPos:]`
    fn remove_water_tile_at_pos(&self, pos: IntPair) {}
    /// `-[World paintTile:atPos:colorIndex:faceIndex:paintBlockhead:]`
    fn paint_tile_at_pos_color_index_face_index_paint_blockhead(
        &self,
        tile: Option<TileRef>,
        pos: IntPair,
        color_index: u16,
        face_index: i32,
        b: &ObjRef,
    ) {
    }
    /// `-[World removePaintAtTile:atPos:faceIndex:paintBlockhead:]`
    fn remove_paint_at_tile_at_pos_face_index_paint_blockhead(
        &self,
        tile: Option<TileRef>,
        pos: IntPair,
        face_index: i32,
        b: &ObjRef,
    ) {
    }
    /// `-[World sendGatherNotificationForTile:atPos:]`
    fn send_gather_notification_for_tile_at_pos(&self, tile: Option<TileRef>, pos: IntPair) {}
    /// `-[World displayOwnershipAreas]`
    fn display_ownership_areas(&self) {}
    /// `-[World blockheadReachedInteractionObjectDestination:pathExtraData:]`
    fn blockhead_reached_interaction_object_destination_path_extra_data(
        &self,
        b: &ObjRef,
        extra: Option<&PathExtraData>,
    ) {
    }

    // ================= BHServer / ServerClient ===========================================================
    /// `-[BHServer removeCurseWordsFromBlockheadName:]`
    fn remove_curse_words_from_blockhead_name(&self, name: Option<&str>) -> Option<String> {
        name.map(|s| s.to_string())
    }
    /// `-[BHServer playerIsAdminWithID:]`
    fn player_is_admin_with_id(&self, id: Option<&str>) -> bool {
        false
    }
    /// `-[BHServer updatePlayer:]`
    fn update_player(&self, id: Option<&str>) {}
    /// `-[BHServer playerNameForPlayerWithIDIncludingOldPlayers:]`
    fn player_name_for_player_with_id_including_old_players(&self, id: Option<&str>) -> Option<String> {
        None
    }
    /// `[[world serverClients] objectForKey:[blockhead localNetID]]` then `-lightBlockIndex`; `None` = no client.
    fn server_client_light_block_index_for_net_id(&self, net_id: Option<&str>) -> Option<i32> {
        None
    }

    // ================= DynamicWorld (dynamic_world_core / ops) ===========================================
    /// `-[DynamicWorld localNetID]` (md5 id of "server" on the dedicated server)
    #[provided]
    fn dynamic_world_local_net_id(&self) -> String {
        "cf1e8c14e54505f60aa10ceb8d5d8ab3".to_string()
    }
    /// `-interactionObjectWithID:`
    fn interaction_object_with_id(&self, id: i64) -> Option<ObjRef> {
        None
    }
    /// `-interactionObjectAtPos:`
    fn interaction_object_at_pos(&self, pos: IntPair) -> Option<ObjRef> {
        None
    }
    /// `-workbenchAtPos:`
    fn workbench_at_pos(&self, pos: IntPair) -> Option<ObjRef> {
        None
    }
    /// `-ridableObjectWithID:`
    fn ridable_object_with_id(&self, id: u64) -> Option<ObjRef> {
        None
    }
    /// `-npcWithID:`
    fn npc_with_id(&self, id: u64) -> Option<ObjRef> {
        None
    }
    /// `-getPlantAtPos:`
    fn get_plant_at_pos(&self, pos: IntPair) -> Option<ObjRef> {
        None
    }
    /// `-freeBlocksAtPos:` (unsorted)
    fn free_blocks_at_pos(&self, pos: IntPair) -> Vec<ObjRef> {
        Vec::new()
    }
    /// `-doorAtPos:`
    fn door_at_pos(&self, pos: IntPair) -> Option<ObjRef> {
        None
    }
    /// `-windowAtPos:`
    fn window_at_pos(&self, pos: IntPair) -> Option<ObjRef> {
        None
    }
    /// `-eggAtPos:`
    fn egg_at_pos(&self, pos: IntPair) -> Option<ObjRef> {
        None
    }
    /// `-torchAtPos:`
    fn torch_at_pos(&self, pos: IntPair) -> Option<ObjRef> {
        None
    }
    /// `-paintingAtPos:`
    fn painting_at_pos(&self, pos: IntPair) -> Option<ObjRef> {
        None
    }
    /// `-columnAtPos:`
    fn column_at_pos(&self, pos: IntPair) -> Option<ObjRef> {
        None
    }
    /// `-ladderAtPos:`
    fn ladder_at_pos(&self, pos: IntPair) -> Option<ObjRef> {
        None
    }
    /// `-wireAtPos:`
    fn wire_at_pos(&self, pos: IntPair) -> Option<ObjRef> {
        None
    }
    /// `-stairsAtPos:`
    fn stairs_at_pos(&self, pos: IntPair) -> Option<ObjRef> {
        None
    }
    /// `-elevatorShaftAtPos:`
    fn elevator_shaft_at_pos(&self, pos: IntPair) -> Option<ObjRef> {
        None
    }
    /// `-elevatorMotorAtPos:`
    fn elevator_motor_at_pos(&self, pos: IntPair) -> Option<ObjRef> {
        None
    }
    /// `-removeTorchAtPos:` / `removeEggAtPos:` /... selected by `which` (see [`Remover`]).
    fn remove_object_at_pos(&self, which: Remover, pos: IntPair) -> Option<ObjRef> {
        None
    }
    /// `-removeInteractionObjectAtPos:removeBlockhead:`
    fn remove_interaction_object_at_pos_remove_blockhead(&self, pos: IntPair, b: &ObjRef) {}
    /// `-removeWorkbenchAtPos:removeBlockhead:`
    fn remove_workbench_at_pos_remove_blockhead(&self, pos: IntPair, b: &ObjRef) {}
    /// `-loadGatherBlockAtPos:`
    fn load_gather_block_at_pos(&self, pos: IntPair) {}
    /// `-posOfDoorsOtherBlockAtPos:` (x == -1: none)
    fn pos_of_doors_other_block_at_pos(&self, pos: IntPair) -> IntPair {
        IntPair { x: -1, y: -1 }
    }
    /// `-setDoorAtPos:toOpen:direction:`
    fn set_door_at_pos_to_open_direction(&self, pos: IntPair, open: bool, direction: i32) {}
    /// `-openElevatorAtPos:`
    fn open_elevator_at_pos(&self, pos: IntPair) {}
    /// `[[dynamicWorld elevatorMotorForShaftAtPos:p] usePower]`
    fn elevator_motor_for_shaft_at_pos_use_power(&self, pos: IntPair) {}
    /// `-checkForHarmableDynamicObjectUnderTap:ignoreLocalBlockheads:ignoreAllBlockheads:`
    fn check_for_harmable_dynamic_object_under_tap_ignore_local_blockheads_ignore_all_blockheads(
        &self,
        tap: Vector2,
        ignore_local: bool,
        ignore_all: bool,
    ) -> Option<ObjRef> {
        None
    }
    /// `-createFreeBlockAtPosition:ofType:dataA:dataB:subItems:dynamicObjectSaveDict:hovers:playSound:priorityBlockhead:`
    fn create_free_block_at_position_of_type_data_a_data_b_sub_items_dynamic_object_save_dict_hovers_play_sound_priority_blockhead(
        &self,
        pos: IntPair,
        item_type: ItemType,
        data_a: u16,
        data_b: u16,
        sub_items: Option<&[Vec<ItemRef>]>,
        save_dict: Option<&PlistValue>,
        hovers: bool,
        play_sound: i32,
        priority: Option<&ObjRef>,
    ) {
    }

    // ================= other objects (NPCs, workbenches, plants, interaction objects, beds, signs, eggs...) ====
    /// `[o isKindOfClass:[NPC class]]`
    fn is_npc(&self, o: &ObjRef) -> bool {
        false
    }
    /// `[o isKindOfClass:[Donkey class]]`
    fn is_donkey(&self, o: &ObjRef) -> bool {
        false
    }
    /// `[o isKindOfClass:[Bed class]]`
    fn is_bed(&self, o: &ObjRef) -> bool {
        false
    }
    /// `-[NPC foodItemType]`
    fn npc_food_item_type(&self, npc: &ObjRef) -> i32 {
        0
    }
    /// `-[NPC canBeCapturedByBlockhead:withItemType:]`
    fn npc_can_be_captured_by_blockhead(&self, npc: &ObjRef, b: &Blockhead, item_type: ItemType) -> bool {
        false
    }
    /// `-[NPC canBeMilkedByBlockhead:]`
    fn npc_can_be_milked_by_blockhead(&self, npc: &ObjRef, b: &Blockhead) -> bool {
        false
    }
    /// `-[NPC canBeShavedByBlockhead:]`
    fn npc_can_be_shaved_by_blockhead(&self, npc: &ObjRef, b: &Blockhead) -> bool {
        false
    }
    /// `-[NPC belongsToPlayerWithBlockhead:]`
    fn npc_belongs_to_player_with_blockhead(&self, npc: &ObjRef, b: &Blockhead) -> bool {
        false
    }
    /// `-[NPC canBeFedByBlockhead:]`
    fn npc_can_be_fed_by_blockhead(&self, npc: &ObjRef, b: &Blockhead) -> bool {
        false
    }
    /// `-[NPC feedByBlockhead:]`
    fn npc_feed_by_blockhead(&self, npc: &ObjRef, b: &Blockhead) {}
    /// `-[NPC setFreeByBlockhead:]`
    fn npc_set_free_by_blockhead(&self, npc: &ObjRef, b: &Blockhead) {}
    /// `-[NPC captureByBlockhead:withItemType:]`
    fn npc_capture_by_blockhead_with_item_type(&self, npc: &ObjRef, b: &Blockhead, item_type: i32) -> bool {
        false
    }
    /// `-[NPC shaveByBlockhead:]`
    fn npc_shave_by_blockhead(&self, npc: &ObjRef, b: &Blockhead) -> bool {
        false
    }
    /// `-[NPC milkByBlockhead:]`
    fn npc_milk_by_blockhead(&self, npc: &ObjRef, b: &Blockhead) -> bool {
        false
    }
    /// `-[NPC beginBlockheadInspection:]`
    fn npc_begin_blockhead_inspection(&self, npc: &ObjRef, b: &Blockhead) {}
    /// `-[NPC inspectionStopped]`
    fn npc_inspection_stopped(&self, npc: &ObjRef) {}
    /// `-[NPC remoteBlockheadHookedYou:]`
    fn npc_remote_blockhead_hooked_you(&self, npc: &ObjRef, b: &Blockhead) {}
    /// `-[Donkey breed]` (u16)
    fn donkey_breed(&self, o: &ObjRef) -> u16 {
        0
    }
    /// `-[Egg breed]`
    fn egg_breed(&self, o: &ObjRef) -> u16 {
        0
    }
    /// `-[Torch dataA]`, `dataB`
    fn torch_data_a_b(&self, o: &ObjRef) -> (u16, u16) {
        (0, 0)
    }
    /// `-[obj itemType]` (painting, column, stairs, door, bed)
    fn object_item_type(&self, o: &ObjRef) -> i32 {
        0
    }
    /// `-[Door pos]` / `-[Door freeblockCreationItemType]` / `-[Door freeBlockCreationSaveDict]` are on the base trait.
    /// `-[Window...]` nothing extra.
    /// `-[Sign text]` length (SignTextDynamicObject): the number of UTF-16 units.
    #[provided]
    fn sign_text_length(&self, o: &ObjRef) -> usize {
        o.as_sign_text()
            .and_then(crate::objects::protocols::SignTextDynamicObject::text)
            .unwrap_or_default()
            .encode_utf16()
            .count()
    }
    /// `[rideObject blockheadCanRide:self usingItem:t]`
    #[provided]
    fn ridable_blockhead_can_ride_using_item(&self, o: &ObjRef, b: &Blockhead, item: i32) -> bool {
        o.as_ridable().map_or(false, |r| r.blockhead_can_ride_using_item(&b.me(), item))
    }
    /// `[rideObject isKindOfClass:[HandCar|SteamTrain|PassengerCar class]]`: 28 / 29 / 30 or 0 = other.
    #[provided]
    fn ride_object_animation_type(&self, o: &ObjRef) -> u32 {
        use crate::objects::handle::is_kind_of_class;
        if is_kind_of_class(o, &crate::trains::hand_car::HAND_CAR_CLASS) {
            28
        } else if is_kind_of_class(o, &crate::trains::steam_train::STEAM_TRAIN_CLASS) {
            29
        } else if is_kind_of_class(o, &crate::trains::passenger_car::PASSENGER_CAR_CLASS) {
            30
        } else {
            0
        }
    }
    /// `[rideObject respondsToSelector:@selector(riderRidesWithArmsDown)] && [rideObject riderRidesWithArmsDown]`
    /// Answered: true only for Dodo and Scorpion.
    #[provided]
    fn ride_object_rider_rides_with_arms_down(&self, o: &ObjRef) -> bool {
        use crate::objects::handle::is_kind_of_class;
        is_kind_of_class(o, &crate::npc::dodo::DODO_CLASS_INFO)
            || is_kind_of_class(o, &crate::npc::scorpion::SCORPION_CLASS)
    }
    /// `-[Workbench workbenchType]` (`type`)
    fn workbench_type(&self, wb: &ObjRef) -> i32 {
        0
    }
    /// `-[Workbench level]`
    fn workbench_level(&self, wb: &ObjRef) -> i32 {
        0
    }
    /// `-[Workbench hasRequiredFuel]`
    fn workbench_has_required_fuel(&self, wb: &ObjRef) -> bool {
        true
    }
    /// `-[Workbench usesStoresConductsOrProducesElectricity]`
    fn workbench_uses_stores_conducts_or_produces_electricity(&self, wb: &ObjRef) -> bool {
        false
    }
    /// `-[Workbench abortImmediatelyAndRestoreBlockheadItems]`
    fn workbench_abort_immediately_and_restore_blockhead_items(&self, wb: &ObjRef) {}
    /// `-[Workbench isInUse]`
    fn workbench_is_in_use(&self, wb: &ObjRef) -> bool {
        false
    }
    /// `-[Workbench currentBlockhead]`
    fn workbench_current_blockhead(&self, wb: &ObjRef) -> Option<ObjRef> {
        None
    }
    /// `-[Workbench craftItem:withBlockhead:craftProgressUI:count:]`
    fn workbench_craft_item_with_blockhead_count(
        &self,
        wb: &ObjRef,
        item: &Rc<dyn Any>,
        b: &Blockhead,
        count: i32,
    ) -> bool {
        false
    }
    /// `-[Workbench startManagingFuelWithBlockhead:]`
    fn workbench_start_managing_fuel_with_blockhead(&self, wb: &ObjRef, b: &Blockhead) {}
    /// `-[obj setWorkbenchChoiceUIOption:]`
    fn set_workbench_choice_ui_option(&self, o: &ObjRef, v: i32) {}
    /// `-[obj interactionObjectType]`
    fn interaction_object_type(&self, o: &ObjRef) -> i32 {
        0
    }
    /// `-[InteractionObject isInUse]`
    fn interaction_object_is_in_use(&self, o: &ObjRef) -> bool {
        false
    }
    /// `-[InteractionObject remove:]`
    fn interaction_object_remove(&self, o: &ObjRef, b: &Blockhead) {}
    /// `-[obj blockheadWouldLikeToTakeOwnership:withSaveDict:]`
    fn blockhead_would_like_to_take_ownership_with_save_dict(&self, o: &ObjRef, b: &Blockhead, d: &PlistValue) {}
    /// `-[Plant gatherProgressForTile:]`
    fn plant_gather_progress_for_tile(&self, plant: &ObjRef, pos: IntPair) -> i32 {
        0
    }
    /// `-[Plant setGatherProgress:forTile:]`
    fn plant_set_gather_progress_for_tile(&self, plant: &ObjRef, progress: i32, pos: IntPair) {}
    /// `-[Plant tileHarvested:removeBlockhead:correctToolMultiplier:]` returns the number of items made
    fn plant_tile_harvested_remove_blockhead_correct_tool_multiplier(
        &self,
        plant: &ObjRef,
        pos: IntPair,
        b: &Blockhead,
        multiplier: i32,
    ) -> i32 {
        0
    }
    /// `[fishingRod valid]`
    fn fishing_rod_valid(&self, r: &FishingRodRef) -> bool {
        false
    }
    /// `[[fishingRod hookedFish] uniqueID]`
    fn fishing_rod_hooked_fish_unique_id(&self, r: &FishingRodRef) -> u64 {
        0
    }
    /// `[[FishingRod alloc] initWithWorld:blockhead:cache:]` followed by `directionalSwipe:`
    fn fishing_rod_new_with_swipe(&self, b: &Blockhead, swipe: Vector2) -> FishingRodRef {
        Rc::new(())
    }
    /// `-[FishingRod update:accurateDT:isSimulation:]`
    fn fishing_rod_update(&self, r: &FishingRodRef, dt: f32, accurate_dt: f32, is_simulation: bool) {}
}
}

/// Which `-[DynamicWorld removeXAtPos:]` of the pickup table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Remover {
    Torch,
    Egg,
    Painting,
    Ladder,
    Column,
    Wire,
    Stairs,
    ElevatorShaft,
    ElevatorMotor,
    Rail,
    Window,
    Door,
}

/// The default host: every method of [`BlockheadHost`] as defaulted.
pub struct NullHost;
impl BlockheadHostStub for NullHost {}

thread_local! {
    static HOST: RefCell<Rc<dyn BlockheadHost>> = RefCell::new(Rc::new(NullHost));
}

/// Installs the host (once at startup; tests install recording hosts).
pub fn set_host(h: Rc<dyn BlockheadHost>) {
    HOST.with(|c| *c.borrow_mut() = h);
}

/// The installed host.
pub fn host() -> Rc<dyn BlockheadHost> {
    HOST.with(|c| c.borrow().clone())
}

// silence unused-import warnings for items re-exported to the sibling files of this module
#[allow(dead_code)]
type _Unused = (FreeBlockCreationCount, Vector, InventoryItem, f32);

// ====================================================================================================
// test fixtures
// ====================================================================================================

/// Fixtures shared by the unit tests of the four files of this module: a world with one block (rock below
/// y = 8, air above), a mock dynamic world, a recording host.
#[cfg(test)]
pub(crate) mod testutil {
    use super::*;
    use crate::objects::base::DynamicWorldApi;
    use crate::objects::handle::alloc_obj;
    use std::cell::Cell;

    pub struct MockDw {
        pub next_id: Cell<u64>,
        pub server: Cell<bool>,
        pub changed: RefCell<Vec<(IntPair, i32)>>,
        pub bhs: RefCell<Vec<ObjRef>>,
    }
    impl DynamicWorldApi for MockDw {
        fn get_next_dynamic_object_id(&self) -> u64 {
            let v = self.next_id.get() + 1;
            self.next_id.set(v);
            v
        }
        fn dynamic_world_changed_at_pos_object_type(&self, pos: IntPair, object_type: i32) {
            self.changed.borrow_mut().push((pos, object_type));
        }
        fn is_server(&self) -> bool {
            self.server.get()
        }
        fn blockheads(&self) -> Vec<ObjRef> {
            self.bhs.borrow().clone()
        }
        fn get_owner_name_for_object_owner_id(&self, _owner_id: &str) -> Option<String> {
            None
        }
    }

    /// World 32 macro tiles wide; macro tile (0,0) has a valid block with rock (type 1) for y < 8 and air (2)
    /// above. `is_admin` is true (the server).
    pub fn setup() -> (Rc<World>, Rc<MockDw>, DynamicWorldRef) {
        let world = World::alloc(32);
        let pb = world.take_physical_block();
        pb.valid.set(true);
        for idx in 0..1024usize {
            let t = TileRef::new(pb, idx);
            t.set_type_index(if idx / 32 < 8 { 1 } else { 2 });
        }
        let mt = world.macro_tiles.get(0).unwrap();
        *mt.physical_block.borrow_mut() = Some(pb);
        *mt.dynamic_objects.borrow_mut() = Some(Vec::new());
        world.set_is_admin(true);
        *world.distance_ordered_food_types.borrow_mut() = (0..31).collect();
        let dw = Rc::new(MockDw {
            next_id: Cell::new(100),
            server: Cell::new(true),
            changed: RefCell::new(Vec::new()),
            bhs: RefCell::new(Vec::new()),
        });
        let dwref: DynamicWorldRef = dw.clone();
        (world, dw, dwref)
    }

    pub fn tile(world: &World, x: i32, y: i32) -> TileRef {
        crate::world::tiles::tile_at_world_position_loaded(x, y, world).unwrap()
    }

    /// A new character at `(5, 8)` with number `n`.
    pub fn new_bh(world: &Rc<World>, dw: &DynamicWorldRef, n: i32) -> Rc<dyn DynamicObject> {
        Blockhead::init_with_world_dynamic_world_at_position_cache_blockhead_number_craftable_item_object_unique_id(
            world,
            dw,
            IntPair { x: 5, y: 8 },
            None,
            n,
            None,
            u64::MAX,
        )
        .expect("constructed")
    }

    pub fn bh_of(o: &ObjRef) -> &Blockhead {
        as_blockhead(o).unwrap()
    }

    /// A bare dynamic object with a settable type.
    pub struct StubObj {
        pub base: RefCell<DynamicObjectBase>,
        pub ty: i32,
        pub removable: bool,
    }
    impl DynamicObject for StubObj {
        fn base(&self) -> &RefCell<DynamicObjectBase> {
            &self.base
        }
        fn class_info(&self) -> &'static ClassInfo {
            &DYNAMIC_OBJECT_CLASS
        }
        fn as_any(&self) -> &dyn Any {
            self
        }
        fn object_type(&self) -> i32 {
            self.ty
        }
        fn can_be_removed_by_blockhead(&self, _me: &ObjRef, _b: &ObjRef) -> bool {
            self.removable
        }
    }
    pub fn stub(ty: i32, removable: bool) -> ObjRef {
        alloc_obj(|w| StubObj { base: RefCell::new(DynamicObjectBase::alloc_zeroed_with(w)), ty, removable })
    }

    /// Recording host with scripted answers.
    #[derive(Default)]
    pub struct TestHost {
        pub log: RefCell<Vec<String>>,
        pub cit: Cell<u32>,
        pub item_available: Cell<bool>,
        pub is_npc: Cell<bool>,
        pub food_item: Cell<i32>,
        pub capturable: Cell<bool>,
        pub milkable: Cell<bool>,
        pub shaveable: Cell<bool>,
        pub can_ride: Cell<bool>,
        pub belongs: Cell<bool>,
        pub torch: RefCell<Option<ObjRef>>,
        pub can_pick: Cell<bool>,
        pub io_by_id: RefCell<Option<ObjRef>>,
        pub adding_fuel: Cell<bool>,
    }
    impl TestHost {
        pub fn install() -> Rc<TestHost> {
            let h = Rc::new(TestHost { can_pick: Cell::new(true), ..Default::default() });
            set_host(h.clone());
            h
        }
        pub fn take(&self) -> Vec<String> {
            std::mem::take(&mut *self.log.borrow_mut())
        }
        fn rec(&self, s: String) {
            self.log.borrow_mut().push(s);
        }
    }
    impl BlockheadHostStub for TestHost {
        fn add_simulation_event_of_type_for_blockhead_extra_data(
            &self,
            event_type: i32,
            _b: &ObjRef,
            extra: SimulationEventExtra,
        ) {
            self.rec(format!("sim:{}:{:?}", event_type, extra));
        }
        fn report_achievement_with_identifier(&self, id: &str) {
            self.rec(format!("ach:{}", id));
        }
        fn drop_inventory_items_at_index_sub_index_count_ignore_freeblocks(
            &self,
            _b: &Blockhead,
            index: i32,
            sub_index: i32,
            count: i32,
            ignore: bool,
        ) {
            self.rec(format!("drop:{}:{}:{}:{}", index, sub_index, count, ignore));
        }
        fn interaction_type_for_tile_at_pos_item_pickup_rejected_due_to_inventory_full_include_actions_face_index_allow_protected_actions(
            &self,
            _b: &Blockhead,
            _t: Option<TileRef>,
            _p: IntPair,
            _i: Option<&ItemRef>,
            _r: &mut bool,
            _ia: bool,
            _f: i32,
            _ap: bool,
        ) -> u32 {
            self.cit.get()
        }
        fn has_interaction_inventory_item_available(&self, _b: &Blockhead) -> bool {
            self.item_available.get()
        }
        fn send_gather_notification_for_tile_at_pos(&self, _t: Option<TileRef>, pos: IntPair) {
            self.rec(format!("gather:{},{}", pos.x, pos.y));
        }
        fn remove_tile_at_world_x_world_y_create_contents_freeblock_count_create_foreground_contents_freeblock_count_remove_blockhead_only_remove_c_ontents_only_remove_foreground_contents(
            &self,
            x: i32,
            y: i32,
            c: i32,
            fgc: i32,
            _b: &ObjRef,
            only_c: bool,
            only_fg: bool,
        ) {
            self.rec(format!("removetile:{},{}:{}:{}:{}:{}", x, y, c, fgc, only_c, only_fg));
        }
        fn increment_usage_of_interaction_item(&self, _b: &Blockhead, attack: bool) {
            self.rec(format!("usage:{}", attack));
        }
        fn remove_interaction_item(&self, _b: &Blockhead, f: bool) {
            self.rec(format!("removeitem:{}", f));
        }
        fn set_door_at_pos_to_open_direction(&self, pos: IntPair, open: bool, dir: i32) {
            self.rec(format!("door:{},{}:{}:{}", pos.x, pos.y, open, dir));
        }
        fn stop_riding(&self, _b: &Blockhead) {
            self.rec("stopriding".to_string());
        }
        fn is_npc(&self, _o: &ObjRef) -> bool {
            self.is_npc.get()
        }
        fn npc_food_item_type(&self, _o: &ObjRef) -> i32 {
            self.food_item.get()
        }
        fn npc_can_be_captured_by_blockhead(&self, _o: &ObjRef, _b: &Blockhead, _t: ItemType) -> bool {
            self.capturable.get()
        }
        fn npc_can_be_milked_by_blockhead(&self, _o: &ObjRef, _b: &Blockhead) -> bool {
            self.milkable.get()
        }
        fn npc_can_be_shaved_by_blockhead(&self, _o: &ObjRef, _b: &Blockhead) -> bool {
            self.shaveable.get()
        }
        fn ridable_blockhead_can_ride_using_item(&self, _o: &ObjRef, _b: &Blockhead, _i: i32) -> bool {
            self.can_ride.get()
        }
        fn npc_belongs_to_player_with_blockhead(&self, _o: &ObjRef, _b: &Blockhead) -> bool {
            self.belongs.get()
        }
        fn torch_at_pos(&self, _p: IntPair) -> Option<ObjRef> {
            self.torch.borrow().clone()
        }
        fn can_pick_up_item_of_type_sub_items(&self, _b: &Blockhead, _t: ItemType, _s: Option<&[Vec<ItemRef>]>) -> i32 {
            self.can_pick.get() as i32
        }
        fn remove_object_at_pos(&self, which: Remover, pos: IntPair) -> Option<ObjRef> {
            self.rec(format!("remove:{:?}:{},{}", which, pos.x, pos.y));
            None
        }
        fn add_item_to_inventory_flash(&self, b: &Blockhead, item: &ItemRef, flash: bool) {
            self.rec(format!("additem:{}:{}", item.borrow().item_type.0, flash));
            let _ = b;
        }
        fn interaction_object_with_id(&self, id: i64) -> Option<ObjRef> {
            self.rec(format!("ioid:{id}"));
            self.io_by_id.borrow().clone()
        }
        fn blockhead_would_like_to_take_ownership_with_save_dict(&self, _o: &ObjRef, _b: &Blockhead, _d: &PlistValue) {
            self.rec("takeownership".to_string());
        }
        fn is_adding_fuel_to_any_workbench(&self, _b: &Blockhead) -> bool {
            self.adding_fuel.get()
        }
        fn inventory_was_changed_sub_index_was_usage(&self, _b: &Blockhead, i: i32, s: i32, u: bool) {
            self.rec(format!("invchanged:{}:{}:{}", i, s, u));
        }
        fn die_for_good(&self, _b: &Blockhead) {
            self.rec("dieforgood".to_string());
        }
        fn abort_in_progress_path_if_for_blockhead(&self, _b: &ObjRef) {
            self.rec("abortpath".to_string());
        }
        fn send_damage_for_net_blockhead_recoil(&self, d: f32, _b: &ObjRef, r: bool) {
            self.rec(format!("netdamage:{}:{}", d, r));
        }
        fn increment_damage_of_armor_clothing(&self, _b: &Blockhead, d: f32) {
            self.rec(format!("armor:{}", d));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::testutil::*;
    use super::*;
    use crate::rng::glibc_rand::srand;

    /// : all zero except the nine (1,1) vectors.
    #[test]
    fn alloc_zeroed_defaults() {
        let f = BlockheadFields::alloc_zeroed();
        assert_eq!(f.hat_item_color, [1.0; 4]);
        assert_eq!(f.shoes_item_color, [1.0; 4]);
        assert_eq!(f.render_pos, Vector2 { x: 1.0, y: 1.0 });
        assert_eq!(f.jet_pack_incoming_acceleration, Vector2 { x: 1.0, y: 1.0 });
        assert_eq!(f.jet_pack_velocity, Vector2 { x: 1.0, y: 1.0 });
        assert_eq!(f.smoothed_jet_pack_velocity, Vector2 { x: 1.0, y: 1.0 });
        assert_eq!(f.float_pos_when_d_pad_movement_started, Vector2 { x: 1.0, y: 1.0 });
        assert_eq!(f.last_d_pad_total_distance, Vector2 { x: 1.0, y: 1.0 });
        assert_eq!(f.last_d_pad_total_distance_smoothed, Vector2 { x: 1.0, y: 1.0 });
        assert_eq!(f.state.to_save_blob(), vec![0u8; 0x6c]);
        assert_eq!(f.travel_speed, 0.0);
        assert!(f.inventory_items.is_empty() && f.path.is_none() && f.fishing_rod.is_none());
        let (world, dw, dwref) = setup();
        let _ = dw;
        let o = new_bh(&world, &dwref, 0);
        assert_eq!(o.base().borrow().float_pos, Vector2 { x: 5.5, y: 8.0 });
    }

    #[test]
    fn energy_and_coffee() {
        let (world, _dw, dwref) = setup();
        let o = new_bh(&world, &dwref, 0);
        let b = bh_of(&o);
        b.bh.borrow_mut().state.energy = 1.2;
        // ((double)1.2f - 0.25) / 0.75 narrowed to f32 (the spec's decimal "1.2666667" is the rounded print)
        assert_eq!(b.energy(), 1.2666668f32);
        b.bh.borrow_mut().state.energy = 0.2;
        assert_eq!(b.energy(), 0.0);
        b.bh.borrow_mut().state.energy = 0.25;
        assert_eq!(b.energy(), 0.0);
        b.bh.borrow_mut().state.coffee_energy_timer = 0.01;
        assert!(!b.has_coffee_energy());
        b.bh.borrow_mut().state.coffee_energy_timer = 0.0100001;
        assert!(b.has_coffee_energy());
    }

    #[test]
    fn update_animation_table() {
        let (world, _dw, dwref) = setup();
        let o = new_bh(&world, &dwref, 0);
        let b = bh_of(&o);
        let cases = [(1, 24), (3, 7), (4, 7), (5, 7), (2, 23), (6, 14), (11, 32), (0, 0), (9, 0), (7, 0)];
        for (k, a) in cases {
            b.bh.borrow_mut().traverse_to_key_frame = k;
            b.bh.borrow_mut().state.animation_type = 99;
            b.update_animation();
            assert_eq!(b.bh.borrow().state.animation_type, a, "kf {k}");
        }
        // jetpack flag forces 32
        b.bh.borrow_mut().traverse_to_key_frame = 1;
        b.bh.borrow_mut().is_in_jet_pack_free_flight_mode = true;
        b.update_animation();
        assert_eq!(b.bh.borrow().state.animation_type, 32);
        // lie / monkey adjustments
        b.bh.borrow_mut().is_in_jet_pack_free_flight_mode = false;
        b.bh.borrow_mut().to_square = IntPair { x: 5, y: 10 };
        b.bh.borrow_mut().traverse_to_key_frame = 2;
        b.update_animation();
        assert_eq!(b.float_pos().y, 9.5);
        b.bh.borrow_mut().traverse_to_key_frame = 6;
        b.update_animation();
        assert_eq!(b.float_pos().y, 10.2);
        // isNet keeps the animation but still adjusts floatPos.y
        b.base.borrow_mut().is_net = true;
        b.bh.borrow_mut().state.animation_type = 23;
        b.bh.borrow_mut().traverse_to_key_frame = 0;
        b.update_animation();
        assert_eq!(b.bh.borrow().state.animation_type, 23);
        assert_eq!(b.float_pos().y, 9.5);
    }

    #[test]
    fn clothing_item_at_index_cases() {
        let (world, _dw, dwref) = setup();
        let o = new_bh(&world, &dwref, 0);
        let b = bh_of(&o);
        assert!(b.clothing_item_at_index(-1).is_none());
        assert!(b.clothing_item_at_index(0).is_none()); // group empty
        let shoes = InventoryItem::new(ItemType(0x7c), 0, 7, None, None);
        {
            let f = b.bh.borrow();
            let cont = f.inventory_items[0].borrow();
            let c0 = cont[0].borrow();
            c0.sub_items.as_ref().unwrap()[0].borrow_mut().push(shoes.clone());
        }
        let got = b.clothing_item_at_index(0).unwrap();
        assert!(Rc::ptr_eq(&got, &shoes));
        assert!(b.clothing_item_at_index(1).is_none());
        assert!(b.clothing_item_at_index(100).is_none());
        b.bh.borrow().inventory_items[0].borrow_mut().clear();
        assert!(b.clothing_item_at_index(0).is_none());
    }

    /// : the eight rules, rule 5 without the isNPC guard.
    #[test]
    fn goal_interaction_for_npc_chase() {
        let h = TestHost::install();
        let (world, _dw, dwref) = setup();
        let o = new_bh(&world, &dwref, 0);
        let b = bh_of(&o);
        let npc = stub(1, true);
        let donkey = stub(28, true);
        let g = |npc: &ObjRef, t: u32| b.goal_interaction_for_npc_chase_for_npc_with_item_type(npc, ItemType(t));
        h.is_npc.set(true);
        h.food_item.set(21);
        assert_eq!(g(&npc, 21), it::FEED_NPC);
        assert_eq!(g(&npc, 0), it::STAB);
        h.capturable.set(true);
        assert_eq!(g(&npc, 302), it::CAPTURE);
        assert_eq!(g(&npc, 102), it::CAPTURE);
        h.capturable.set(false);
        h.milkable.set(true);
        assert_eq!(g(&npc, 102), it::HARVEST_NPC);
        h.shaveable.set(true);
        assert_eq!(g(&npc, 322), it::HARVEST_NPC);
        h.belongs.set(true);
        assert_eq!(g(&npc, 5), it::INSPECT_NPC);
        assert_eq!(g(&npc, 42), it::INSPECT_NPC); // rule 6 beats the bow rule
        h.belongs.set(false);
        assert_eq!(g(&npc, 42), it::FIRE_RANGED_WEAPON);
        assert_eq!(g(&npc, 257), it::FIRE_RANGED_WEAPON);
        // rule 5: donkey + carrot stick, no isNPC guard
        h.is_npc.set(false);
        h.can_ride.set(true);
        assert_eq!(g(&donkey, 74), it::RIDE);
        h.can_ride.set(false);
        assert_eq!(g(&donkey, 74), it::STAB);
        assert_eq!(g(&npc, 42), it::FIRE_RANGED_WEAPON);
    }

    #[test]
    fn local_net_id_and_requires_physical_block() {
        let _h = TestHost::install();
        let (world, dw, dwref) = setup();
        let o = new_bh(&world, &dwref, 0);
        let b = bh_of(&o);
        assert_eq!(BlockheadApi::local_net_id(b).as_deref(), Some("cf1e8c14e54505f60aa10ceb8d5d8ab3"));
        b.set_client_id("abc");
        b.bh.borrow_mut().is_client_blockhead_being_controlled_by_server = true;
        assert_eq!(BlockheadApi::local_net_id(b).as_deref(), Some("abc"));
        assert!(o.requires_physical_block());
        b.base.borrow_mut().is_net = true;
        assert!(o.requires_physical_block());
        dw.server.set(false);
        assert!(!o.requires_physical_block());
        assert_eq!(o.object_type(), 24);
        assert!(o.as_path_user().is_some() && o.as_harmable().is_some() && o.as_blockhead().is_some());
    }

    #[test]
    fn path_extra_data() {
        let mut e = PathExtraData::new();
        assert_eq!(e.int_for_key("count"), 0);
        e.set("count", ExtraValue::Int(7));
        e.set("count", ExtraValue::Int(9));
        assert_eq!(e.int_for_key("count"), 9);
        let o = stub(1, true);
        e.set("workbench", ExtraValue::Obj(o.clone()));
        assert!(crate::objects::handle::ptr_eq(&e.object_for_key("workbench").unwrap(), &o));
        assert!(e.any_for_key("workbench").is_none());
        let _ = srand;
    }

    /// A host that keeps every default of the seam.
    struct Bare;
    impl BlockheadHostStub for Bare {}

    /// Every default body of the host seam runs once; the ones with real behaviour are checked.
    #[test]
    fn host_defaults_run_and_are_inert() {
        let (world, _dw, dwref) = setup();
        let o = new_bh(&world, &dwref, 0);
        let b = bh_of(&o);
        let ob = stub(1, true);
        let item = InventoryItem::new(ItemType(5), 0, 0, None, None);
        let act: ActionRef = Rc::new(());
        let any: Rc<dyn Any> = Rc::new(());
        let pv = PlistValue::int(1);
        let mut flag = false;
        let h = Bare;
        BlockheadHostStub::update_position(&h, b, IntPair { x: 1, y: 2 });
        BlockheadHostStub::stop_riding(&h, b);
        BlockheadHostStub::pickup_dynamic_object(&h, b, &ob);
        let _ = BlockheadHostStub::interaction_type_for_tile_at_pos_item_pickup_rejected_due_to_inventory_full_include_actions_face_index_allow_protected_actions(&h, b, None, IntPair { x: 1, y: 2 }, None, &mut flag, true, 3, true);
        let _ = BlockheadHostStub::good_or_bad_interaction_for_action(&h, b, None);
        BlockheadHostStub::update_gather_speed_and_animation_for_current_interation_and_item(&h, b);
        let _ = BlockheadHostStub::is_adding_fuel_to_any_workbench(&h, b);
        let _ = BlockheadHostStub::current_traverse_to_key_frame(&h, b);
        let _ = BlockheadHostStub::next_pos(&h, b);
        let _ = BlockheadHostStub::info_for_path_recalculation(&h, b);
        BlockheadHostStub::set_path_type_goal_interaction_extra_data(&h, b, None, 3, 3, None);
        BlockheadHostStub::set_waiting_for_path_to_pos(&h, b, IntPair { x: 1, y: 2 });
        BlockheadHostStub::set_no_longer_waiting_for_path(&h, b);
        let _ = BlockheadHostStub::can_crawl(&h, b);
        let _ = BlockheadHostStub::can_fly(&h, b);
        let _ = BlockheadHostStub::tap_is_within_body_radius(&h, b, Vector2 { x: 0.5, y: 1.5 });
        BlockheadHostStub::hit_with_force_blockhead(&h, b, 3, None);
        let _ = BlockheadHostStub::will_die_if_hit_by_force(&h, b, 3);
        let _ = BlockheadHostStub::center(&h, b);
        let _ = BlockheadHostStub::is_interacting_with_any_interaction_object(&h, b);
        BlockheadHostStub::set_interaction_object(&h, b, None);
        BlockheadHostStub::set_interaction_workbench(&h, b, None);
        let _ = BlockheadHostStub::can_pick_up_item_of_type_sub_items(&h, b, ItemType(5), None);
        let _ = BlockheadHostStub::can_pick_up_item_of_type_sub_items_data_a_data_b(&h, b, ItemType(5), None, 3, 3);
        BlockheadHostStub::add_item_to_inventory_flash_disable_warp_check_force_slot_index(&h, b, &item, true, true, 3);
        BlockheadHostStub::add_item_to_inventory_flash(&h, b, &item, true);
        let _ = BlockheadHostStub::pickup_freeblock_if_possible_in_tile_intentional(&h, b, &ob, None, true);
        BlockheadHostStub::increment_usage_of_interaction_item(&h, b, true);
        BlockheadHostStub::increment_usage_of_clothing(&h, b);
        BlockheadHostStub::increment_usage_of_ice_clothing(&h, b);
        BlockheadHostStub::increment_fuel_usage(&h, b);
        BlockheadHostStub::increment_passive_item_usage(&h, b);
        BlockheadHostStub::increment_damage_of_armor_clothing(&h, b, 0.5);
        BlockheadHostStub::inventory_was_changed_sub_index_was_usage(&h, b, 3, 3, true);
        BlockheadHostStub::drop_inventory_items_at_index_sub_index_count_ignore_freeblocks(&h, b, 3, 3, 3, true);
        BlockheadHostStub::remove_interaction_item(&h, b, true);
        let _ = BlockheadHostStub::has_interaction_inventory_item_available(&h, b);
        let _ = BlockheadHostStub::can_use_dynamic_object(&h, b, &ob);
        BlockheadHostStub::stop_all_actions(&h, b);
        BlockheadHostStub::stop_fishing(&h, b);
        BlockheadHostStub::die_for_good(&h, b);
        let _ = BlockheadHostStub::unable_to_work_reason(&h, b);
        let _ = BlockheadHostStub::jetpack_fuel_count(&h, b);
        BlockheadHostStub::update_jet_motion(&h, b, 0.5);
        let _ = BlockheadHostStub::free_block_pickup_radius(&h, b);
        let _ = BlockheadHostStub::dpad_should_be_displayed(&h, b);
        let _ = BlockheadHostStub::current_item(&h, b);
        let _ = BlockheadHostStub::current_interaction_requires_human_input(&h, b);
        let _ = BlockheadHostStub::action_in_progress(&h, &act);
        BlockheadHostStub::action_set_in_progress(&h, &act, true);
        let _ = BlockheadHostStub::action_goal_tile_pos(&h, &act);
        let _ = BlockheadHostStub::action_get_save_dict(&h, &act);
        let _ = BlockheadHostStub::action_init_with_save_dict_inventory_items(&h, &pv, &[]);
        let _ = BlockheadHostStub::action_interaction_test_result(&h, &act);
        let _ = BlockheadHostStub::action_interaction_object_id(&h, &act);
        let _ = BlockheadHostStub::action_inventory_change_int_for_key(&h, &act, "k");
        BlockheadHostStub::action_set_inventory_change_int_for_key(&h, &act, "k", 3);
        BlockheadHostStub::add_simulation_event_of_type_for_blockhead_extra_data(
            &h,
            3,
            &ob,
            SimulationEventExtra::None,
        );
        BlockheadHostStub::report_achievement_with_identifier(&h, "k");
        BlockheadHostStub::abort_in_progress_path_if_for_blockhead(&h, &ob);
        BlockheadHostStub::send_damage_for_net_blockhead_recoil(&h, 0.5, &ob, true);
        BlockheadHostStub::fire_projectile_from_to_at_fire_item_type_firer(
            &h,
            Vector2 { x: 0.5, y: 1.5 },
            Vector2 { x: 0.5, y: 1.5 },
            None,
            3,
            None,
        );
        let _ = BlockheadHostStub::tile_is_lit_for_client_at_pos_tile(&h, None, IntPair { x: 1, y: 2 }, None, None);
        BlockheadHostStub::fully_load_if_needed_around_pos_client_light_block_index_for_blockhead(
            &h,
            IntPair { x: 1, y: 2 },
            3,
            None,
        );
        BlockheadHostStub::place_workbench_of_type_at_pos_save_dict_placed_by_client_placed_by_blockhead_placed_by_client_name(&h, 3, IntPair { x: 1, y: 2 }, None, None, &ob, None);
        let _ = BlockheadHostStub::place_interaction_object_with_item_at_pos_save_dict_placed_by_client_placed_by_blockhead_placed_by_client_name(&h, &item, IntPair { x: 1, y: 2 }, None, None, &ob, None);
        BlockheadHostStub::fill_tile_at_pos_with_type(&h, None, IntPair { x: 1, y: 2 }, 3);
        BlockheadHostStub::fill_tile_at_pos_with_type_data_a_data_b_placed_by_client_save_dict_placed_by_blockhead_placed_by_client_name(&h, None, IntPair { x: 1, y: 2 }, 3, 3, 3, None, None, None, None);
        BlockheadHostStub::remove_tile_at_world_x_world_y_create_contents_freeblock_count_create_foreground_contents_freeblock_count_remove_blockhead_only_remove_c_ontents_only_remove_foreground_contents(&h, 3, 3, 3, 3, &ob, true, true);
        BlockheadHostStub::remove_back_wall_at_pos_remove_blockhead(&h, IntPair { x: 1, y: 2 }, &ob);
        BlockheadHostStub::remove_water_tile_at_pos(&h, IntPair { x: 1, y: 2 });
        BlockheadHostStub::paint_tile_at_pos_color_index_face_index_paint_blockhead(
            &h,
            None,
            IntPair { x: 1, y: 2 },
            3,
            3,
            &ob,
        );
        BlockheadHostStub::remove_paint_at_tile_at_pos_face_index_paint_blockhead(
            &h,
            None,
            IntPair { x: 1, y: 2 },
            3,
            &ob,
        );
        BlockheadHostStub::send_gather_notification_for_tile_at_pos(&h, None, IntPair { x: 1, y: 2 });
        BlockheadHostStub::display_ownership_areas(&h);
        BlockheadHostStub::blockhead_reached_interaction_object_destination_path_extra_data(&h, &ob, None);
        let _ = BlockheadHostStub::remove_curse_words_from_blockhead_name(&h, None);
        let _ = BlockheadHostStub::player_is_admin_with_id(&h, None);
        BlockheadHostStub::update_player(&h, None);
        let _ = BlockheadHostStub::player_name_for_player_with_id_including_old_players(&h, None);
        let _ = BlockheadHostStub::server_client_light_block_index_for_net_id(&h, None);
        let _ = BlockheadHostStub::dynamic_world_local_net_id(&h);
        let _ = BlockheadHostStub::interaction_object_with_id(&h, 3);
        let _ = BlockheadHostStub::interaction_object_at_pos(&h, IntPair { x: 1, y: 2 });
        let _ = BlockheadHostStub::workbench_at_pos(&h, IntPair { x: 1, y: 2 });
        let _ = BlockheadHostStub::ridable_object_with_id(&h, 3);
        let _ = BlockheadHostStub::npc_with_id(&h, 3);
        let _ = BlockheadHostStub::get_plant_at_pos(&h, IntPair { x: 1, y: 2 });
        let _ = BlockheadHostStub::free_blocks_at_pos(&h, IntPair { x: 1, y: 2 });
        let _ = BlockheadHostStub::door_at_pos(&h, IntPair { x: 1, y: 2 });
        let _ = BlockheadHostStub::window_at_pos(&h, IntPair { x: 1, y: 2 });
        let _ = BlockheadHostStub::egg_at_pos(&h, IntPair { x: 1, y: 2 });
        let _ = BlockheadHostStub::torch_at_pos(&h, IntPair { x: 1, y: 2 });
        let _ = BlockheadHostStub::painting_at_pos(&h, IntPair { x: 1, y: 2 });
        let _ = BlockheadHostStub::column_at_pos(&h, IntPair { x: 1, y: 2 });
        let _ = BlockheadHostStub::ladder_at_pos(&h, IntPair { x: 1, y: 2 });
        let _ = BlockheadHostStub::wire_at_pos(&h, IntPair { x: 1, y: 2 });
        let _ = BlockheadHostStub::stairs_at_pos(&h, IntPair { x: 1, y: 2 });
        let _ = BlockheadHostStub::elevator_shaft_at_pos(&h, IntPair { x: 1, y: 2 });
        let _ = BlockheadHostStub::elevator_motor_at_pos(&h, IntPair { x: 1, y: 2 });
        let _ = BlockheadHostStub::remove_object_at_pos(&h, Remover::Torch, IntPair { x: 1, y: 2 });
        BlockheadHostStub::remove_interaction_object_at_pos_remove_blockhead(&h, IntPair { x: 1, y: 2 }, &ob);
        BlockheadHostStub::remove_workbench_at_pos_remove_blockhead(&h, IntPair { x: 1, y: 2 }, &ob);
        BlockheadHostStub::load_gather_block_at_pos(&h, IntPair { x: 1, y: 2 });
        let _ = BlockheadHostStub::pos_of_doors_other_block_at_pos(&h, IntPair { x: 1, y: 2 });
        BlockheadHostStub::set_door_at_pos_to_open_direction(&h, IntPair { x: 1, y: 2 }, true, 3);
        BlockheadHostStub::open_elevator_at_pos(&h, IntPair { x: 1, y: 2 });
        BlockheadHostStub::elevator_motor_for_shaft_at_pos_use_power(&h, IntPair { x: 1, y: 2 });
        let _ = BlockheadHostStub::check_for_harmable_dynamic_object_under_tap_ignore_local_blockheads_ignore_all_blockheads(&h, Vector2 { x: 0.5, y: 1.5 }, true, true);
        BlockheadHostStub::create_free_block_at_position_of_type_data_a_data_b_sub_items_dynamic_object_save_dict_hovers_play_sound_priority_blockhead(&h, IntPair { x: 1, y: 2 }, ItemType(5), 3, 3, None, None, true, 3, None);
        let _ = BlockheadHostStub::is_npc(&h, &ob);
        let _ = BlockheadHostStub::is_donkey(&h, &ob);
        let _ = BlockheadHostStub::is_bed(&h, &ob);
        let _ = BlockheadHostStub::npc_food_item_type(&h, &ob);
        let _ = BlockheadHostStub::npc_can_be_captured_by_blockhead(&h, &ob, b, ItemType(5));
        let _ = BlockheadHostStub::npc_can_be_milked_by_blockhead(&h, &ob, b);
        let _ = BlockheadHostStub::npc_can_be_shaved_by_blockhead(&h, &ob, b);
        let _ = BlockheadHostStub::npc_belongs_to_player_with_blockhead(&h, &ob, b);
        let _ = BlockheadHostStub::npc_can_be_fed_by_blockhead(&h, &ob, b);
        BlockheadHostStub::npc_feed_by_blockhead(&h, &ob, b);
        BlockheadHostStub::npc_set_free_by_blockhead(&h, &ob, b);
        let _ = BlockheadHostStub::npc_capture_by_blockhead_with_item_type(&h, &ob, b, 3);
        let _ = BlockheadHostStub::npc_shave_by_blockhead(&h, &ob, b);
        let _ = BlockheadHostStub::npc_milk_by_blockhead(&h, &ob, b);
        BlockheadHostStub::npc_begin_blockhead_inspection(&h, &ob, b);
        BlockheadHostStub::npc_inspection_stopped(&h, &ob);
        BlockheadHostStub::npc_remote_blockhead_hooked_you(&h, &ob, b);
        let _ = BlockheadHostStub::donkey_breed(&h, &ob);
        let _ = BlockheadHostStub::egg_breed(&h, &ob);
        let _ = BlockheadHostStub::torch_data_a_b(&h, &ob);
        let _ = BlockheadHostStub::object_item_type(&h, &ob);
        let _ = BlockheadHostStub::sign_text_length(&h, &ob);
        let _ = BlockheadHostStub::ridable_blockhead_can_ride_using_item(&h, &ob, b, 3);
        let _ = BlockheadHostStub::ride_object_animation_type(&h, &ob);
        let _ = BlockheadHostStub::ride_object_rider_rides_with_arms_down(&h, &ob);
        let _ = BlockheadHostStub::workbench_type(&h, &ob);
        let _ = BlockheadHostStub::workbench_level(&h, &ob);
        let _ = BlockheadHostStub::workbench_has_required_fuel(&h, &ob);
        let _ = BlockheadHostStub::workbench_uses_stores_conducts_or_produces_electricity(&h, &ob);
        BlockheadHostStub::workbench_abort_immediately_and_restore_blockhead_items(&h, &ob);
        let _ = BlockheadHostStub::workbench_is_in_use(&h, &ob);
        let _ = BlockheadHostStub::workbench_current_blockhead(&h, &ob);
        let _ = BlockheadHostStub::workbench_craft_item_with_blockhead_count(&h, &ob, &any, b, 3);
        BlockheadHostStub::workbench_start_managing_fuel_with_blockhead(&h, &ob, b);
        BlockheadHostStub::set_workbench_choice_ui_option(&h, &ob, 3);
        let _ = BlockheadHostStub::interaction_object_type(&h, &ob);
        let _ = BlockheadHostStub::interaction_object_is_in_use(&h, &ob);
        BlockheadHostStub::interaction_object_remove(&h, &ob, b);
        BlockheadHostStub::blockhead_would_like_to_take_ownership_with_save_dict(&h, &ob, b, &pv);
        let _ = BlockheadHostStub::plant_gather_progress_for_tile(&h, &ob, IntPair { x: 1, y: 2 });
        BlockheadHostStub::plant_set_gather_progress_for_tile(&h, &ob, 3, IntPair { x: 1, y: 2 });
        let _ = BlockheadHostStub::plant_tile_harvested_remove_blockhead_correct_tool_multiplier(
            &h,
            &ob,
            IntPair { x: 1, y: 2 },
            b,
            3,
        );
        let _ = BlockheadHostStub::fishing_rod_valid(&h, &any);
        let _ = BlockheadHostStub::fishing_rod_hooked_fish_unique_id(&h, &any);
        let _ = BlockheadHostStub::fishing_rod_new_with_swipe(&h, b, Vector2 { x: 0.5, y: 1.5 });
        BlockheadHostStub::fishing_rod_update(&h, &any, 0.5, 0.5, true);
        // defaults with behaviour
        assert_eq!(BlockheadHostStub::next_pos(&h, b), b.bh.borrow().to_square);
        assert_eq!(BlockheadHostStub::center(&h, b), Vector2 { x: b.float_pos().x, y: b.float_pos().y + 1.0 });
        assert_eq!(BlockheadHostStub::free_block_pickup_radius(&h, b), 0.5);
        assert_eq!(BlockheadHostStub::jetpack_fuel_count(&h, b), 0);
        assert!(BlockheadHostStub::can_use_dynamic_object(&h, b, &ob));
        assert!(!b.bh.borrow().waiting_for_path, "set_no_longer_waiting_for_path ran after it");
        assert_eq!(BlockheadHostStub::dynamic_world_local_net_id(&h), "cf1e8c14e54505f60aa10ceb8d5d8ab3");
        assert_eq!(BlockheadHostStub::remove_curse_words_from_blockhead_name(&h, Some("n")).as_deref(), Some("n"));
        assert_eq!(
            BlockheadHostStub::pos_of_doors_other_block_at_pos(&h, IntPair { x: 1, y: 2 }),
            IntPair { x: -1, y: -1 }
        );
        assert_eq!(BlockheadHostStub::can_pick_up_item_of_type_sub_items(&h, b, ItemType(5), None), 1);
        // the net variant of the fuel count and pickup radius
        b.base.borrow_mut().is_net = true;
        assert_eq!(BlockheadHostStub::jetpack_fuel_count(&h, b), 10);
        b.bh.borrow_mut().shirt_item_type = 278;
        assert_eq!(BlockheadHostStub::free_block_pickup_radius(&h, b), 0.5);
        b.base.borrow_mut().is_net = false;
        assert_eq!(BlockheadHostStub::free_block_pickup_radius(&h, b), 0.5);
        b.bh.borrow_mut().ride_object = Some(ob.clone());
        assert_eq!(BlockheadHostStub::free_block_pickup_radius(&h, b), 1.0);
    }

    use crate::items::inventory_item::InventoryItem;
    use crate::mods::{ChatVerdict, LoaderKey, ModHooks, Optional};

    /// Records the moves of players' blockheads.
    #[derive(Default)]
    struct MoveRecorder {
        moves: RefCell<Vec<(String, IntPair, IntPair)>>,
    }
    impl ModHooks for MoveRecorder {
        fn player_join(&self, _p: &str) {}
        fn player_quit(&self, _p: &str, _kick: bool) {}
        fn chat(&self, _p: &str, _t: &str) -> ChatVerdict {
            ChatVerdict::Keep
        }
        fn owns_command(&self, _l: &str) -> bool {
            false
        }
        fn command(&self, _i: Option<&str>, _l: &str) -> bool {
            false
        }
        fn tick(&self, _dt: f32) {}
        fn poll(&self) {}
        fn world_loaded(&self, _n: &str, _i: &str) {}
        fn world_save(&self, _post: bool) {}
        fn server_stop(&self) {}
        fn wants(&self, what: Optional) -> bool {
            what == Optional::PlayerMove
        }
        fn mod_packet(&self, _p: &str, _d: &[u8]) {}
        fn pre_login(&self, _p: &str, _i: &str, _a: &str, _l: Option<&LoaderKey>) -> Option<u8> {
            None
        }
        fn limbo_join(&self, _p: &str, _i: &str, _l: Option<&LoaderKey>) -> bool {
            false
        }
        fn limbo_packet(&self, _p: &str) -> bool {
            false
        }
        fn limbo_gone(&self, _p: &str) {}
        fn block_place(&self, _p: &str, _pos: IntPair, _i: u32, _a: u16, _b: u16) -> bool {
            false
        }
        fn block_break(&self, _p: &str, _pos: IntPair, _bw: bool, _t: u32) -> bool {
            false
        }
        fn block_changed(&self, _pos: IntPair, _t: u32, _p: Option<&str>) {}
        fn object_place(&self, _p: &str, _pos: IntPair, _w: bool, _i: u32) -> bool {
            false
        }
        fn damage(&self, _p: Option<&str>, _b: u64, a: f32, _at: Option<&str>) -> Option<f32> {
            Some(a)
        }
        fn death(&self, _p: Option<&str>, _b: u64) {}
        fn player_move(&self, p: &str, from: IntPair, to: IntPair) {
            self.moves.borrow_mut().push((p.to_string(), from, to));
        }
    }

    #[test]
    fn trait_views_and_getters() {
        let _h = TestHost::install();
        let (world, dw, dwref) = setup();
        let o = new_bh(&world, &dwref, 0);
        let b = bh_of(&o);
        {
            let mut f = b.bh.borrow_mut();
            f.state.happiness = 0.1;
            f.state.meditation_progress = 0.2;
            f.state.environment = 0.3;
            f.state.environment_temperature = 0.4;
            f.state.environment_exposure = 0.5;
            f.state.environment_light = 0.6;
            f.state.regeneration_progress = 0.7;
            f.state.animation_type = 5;
            f.client_name = Some("Pat".into());
            f.visible = true;
        }
        assert_eq!(
            (b.happiness(), b.meditation_progress(), b.environment(), b.environment_temperature()),
            (0.1, 0.2, 0.3, 0.4)
        );
        assert_eq!((b.environment_exposure(), b.environment_light(), b.regeneration_progress()), (0.5, 0.6, 0.7));
        assert_eq!((b.current_animation_type(), b.client_name().as_deref()), (5, Some("Pat")));
        assert_eq!(BlockheadFields::default().state.to_save_blob(), vec![0u8; 0x6c]);
        // DynamicObject
        assert!(o.requires_physical_block());
        let mut v = vec![];
        o.world_changed(&o, &mut v);
        o.world_contents_changed(&o, &mut v);
        assert!(o.get_save_dict(&o).map.contains_key(&"state".to_string()));
        let created = o.creation_net_data_for_client(&o, Some("x")).unwrap();
        let updated = o.update_net_data_for_client(&o, Some("x")).unwrap();
        assert_eq!(updated.len(), 0x70);
        o.remote_update(&o, &updated);
        o.remote_creation_data_update(&o, &created);
        assert_eq!(o.render_pos(), b.bh.borrow().render_pos);
        // BlockheadApi
        let api = o.as_blockhead().unwrap();
        assert!(!api.is_interacting_with_any_interaction_object());
        api.set_interaction_object(None);
        api.stop_interacting();
        // PathUser
        let pu = o.as_path_user().unwrap();
        assert!(
            pu.tile_is_lit_for_self_at_pos(&tile(&world, 5, 8).get(), IntPair { x: 5, y: 8 })
                == (tile(&world, 5, 8).get().light != 0)
        );
        assert_eq!(pu.current_traverse_to_key_frame(), b.bh.borrow().traverse_to_key_frame);
        assert_eq!(pu.next_pos(), b.bh.borrow().to_square);
        assert!(pu.info_for_path_recalculation().is_none());
        let mut extra = crate::objects::base::new_mutable_dict();
        extra.map.insert("count".to_string(), PlistValue::int(3));
        pu.set_path_type_goal_interaction_extra_data(&o, Some(vec![]), 2, 1, Some(PlistValue::Dict(extra)));
        assert_eq!(b.bh.borrow().path_extra_data.as_ref().unwrap().int_for_key("count"), 3);
        assert!(PathExtraData::from_plist(Some(&PlistValue::int(1))).is_none());
        assert!(PathExtraData::from_plist(None).is_none());
        pu.set_waiting_for_path_to_pos(IntPair { x: 3, y: 4 });
        assert!(pu.waiting_for_path());
        pu.set_no_longer_waiting_for_path();
        assert!(!pu.waiting_for_path());
        assert!(!pu.can_crawl() && !pu.can_fly());
        pu.set_path_needs_recalculated(true);
        assert!(pu.path_needs_recalculated());
        // Harmable
        let hm = o.as_harmable().unwrap();
        assert!(!hm.tap_is_within_body_radius(Vector2 { x: 0.0, y: 0.0 }));
        hm.hit_with_force_blockhead(&o, 0, None);
        assert!(!hm.will_die_if_hit_by_force(5));
        assert!(hm.is_visible());
        assert_eq!(hm.center(), Vector2 { x: b.float_pos().x, y: b.float_pos().y + 1.0 });
        // the per-client light query of a controlled blockhead goes to the host
        b.bh.borrow_mut().is_client_blockhead_being_controlled_by_server = true;
        assert!(!b.tile_is_lit_for_self_at_pos_impl(&tile(&world, 5, 8).get(), IntPair { x: 5, y: 8 }));
        // a replica needs the dynamic world to be a server
        b.base.borrow_mut().is_net = true;
        assert!(o.requires_physical_block());
        dw.server.set(false);
        assert!(!o.requires_physical_block());
        b.base.borrow_mut().is_net = false;
        // mods hear about a player's blockhead moving one tile
        let rec = Rc::new(MoveRecorder::default());
        crate::mods::install(Some(rec.clone()));
        b.set_client_id("player-1");
        o.update_position(&o, IntPair { x: 6, y: 8 });
        o.update_position(&o, IntPair { x: 6, y: 8 });
        assert_eq!(rec.moves.borrow().len(), 1);
        assert_eq!(rec.moves.borrow()[0].0, "player-1");
        b.bh.borrow_mut().client_id = None;
        o.update_position(&o, IntPair { x: 7, y: 8 });
        assert_eq!(rec.moves.borrow().len(), 1);
        crate::mods::install(None);
    }

    #[test]
    fn the_mod_recorder_only_records_moves() {
        let r = MoveRecorder::default();
        r.player_join("p");
        r.player_quit("p", false);
        assert!(matches!(r.chat("p", "t"), ChatVerdict::Keep));
        assert!(!r.owns_command("l") && !r.command(None, "l"));
        r.tick(0.2);
        r.poll();
        r.world_loaded("n", "i");
        r.world_save(true);
        r.server_stop();
        assert!(r.wants(Optional::PlayerMove) && !r.wants(Optional::BlockChanged));
        r.mod_packet("p", &[]);
        assert!(r.pre_login("p", "i", "a", None).is_none());
        assert!(!r.limbo_join("p", "i", None) && !r.limbo_packet("p"));
        r.limbo_gone("p");
        let at = IntPair { x: 1, y: 2 };
        assert!(!r.block_place("p", at, 1, 2, 3) && !r.block_break("p", at, false, 1));
        r.block_changed(at, 1, None);
        assert!(!r.object_place("p", at, true, 1));
        assert_eq!(r.damage(None, 1, 0.5, None), Some(0.5));
        r.death(None, 1);
        r.player_move("p", at, at);
        assert_eq!(r.moves.borrow().len(), 1);
    }

    /// The ride-animation lookup knows the three train classes; the mock dynamic world answers owner names.
    #[test]
    fn ride_object_animation_types_by_class() {
        struct ClassStub {
            base: RefCell<DynamicObjectBase>,
            class: &'static ClassInfo,
        }
        impl DynamicObject for ClassStub {
            fn base(&self) -> &RefCell<DynamicObjectBase> {
                &self.base
            }
            fn class_info(&self) -> &'static ClassInfo {
                self.class
            }
            fn as_any(&self) -> &dyn Any {
                self
            }
        }
        let mk = |class: &'static ClassInfo| -> ObjRef {
            crate::objects::handle::alloc_obj(|w| ClassStub {
                base: RefCell::new(DynamicObjectBase::alloc_zeroed_with(w)),
                class,
            })
        };
        let h = NullHost;
        for (class, expect) in [
            (&crate::trains::hand_car::HAND_CAR_CLASS, 28),
            (&crate::trains::steam_train::STEAM_TRAIN_CLASS, 29),
            (&crate::trains::passenger_car::PASSENGER_CAR_CLASS, 30),
            (&DYNAMIC_OBJECT_CLASS, 0),
        ] {
            let o = mk(class);
            assert_eq!(BlockheadHostStub::ride_object_animation_type(&h, &o), expect);
            assert_eq!(o.base().borrow().pos, IntPair { x: 0, y: 0 });
            assert!(o.as_any().is::<ClassStub>());
        }
        let (_world, dw, _dwref) = setup();
        assert_eq!(crate::objects::base::DynamicWorldApi::get_owner_name_for_object_owner_id(&*dw, "x"), None);
    }
}
