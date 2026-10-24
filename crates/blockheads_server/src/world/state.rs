//! `World` data model: every ivar of `@interface World`, the physical block / macro tile structs, `TileRef`
//! (the live `Tile*` handle) and the physical block pools.
//!
//!
//! ## Placeholders for later layers
//! `World` names types owned by layers above world_state (`DynamicWorld`, `BHServer`,...). They do not exist
//! yet when this layer is built, so this file declares minimal placeholder types in the section
//! "PLACEHOLDERS"
//! swapping the `pub struct`/`pub type` definitions below for `pub use` of their real types.
//! `WorldOpsHooks` is the single seam through which world_state calls world_ops code
//! (block loading, protection, dynamic object loading) without depending on it.

use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::rc::{Rc, Weak};

use crate::foundation::gsdict::GsDict;
use crate::foundation::indexset::IndexSet;
use crate::foundation::nsnumber::NsNumberKey;
use crate::foundation::opqueue::OpQueue;
use crate::objects::base::{MacroTileApi, ServerApi, WorldApi};
use crate::objects::handle::ObjRef;
use crate::plist::value::PlistDict;
use crate::storage::lmdb::{Database, DatabaseEnvironment};
use crate::types::net_types::BhNetPrivacy;
use crate::types::structs::{CustomRules, IntPair, Vector, Vector2, WindowInfo};
use crate::types::tile::{Tile, BLOCK_TILES};

// =====================================================================================================
// PLACEHOLDERS (types owned by later layers;, R-A)
// =====================================================================================================

/// Queries `DynamicWorld` answers for the tile predicates and loot code ("DynamicWorld
/// calls used by predicates"). The real `DynamicWorld` (dynamic_world_core) provides the same names.
pub trait DynamicWorldQueries {
    /// `-[DynamicWorld doorCanBeUsedByPathUser:atPos:]`
    fn door_can_be_used_by_path_user(&self, path_user: Option<&ObjRef>, pos: IntPair) -> bool;
    /// `-[DynamicWorld elevatorMotorForShaftAtPos:]`, reduced to `hasRequiredPower` and `pos`.
    fn elevator_motor_for_shaft_at_pos(&self, pos: IntPair) -> Option<ElevatorMotorInfo>;
    /// `-[DynamicWorld interactionObjectAtPos:]`
    fn interaction_object_at_pos(&self, pos: IntPair) -> Option<DwObject>;
    /// `-[DynamicWorld ladderAtPos:]`
    fn ladder_at_pos(&self, pos: IntPair) -> Option<DwObject>;
    /// `-[DynamicWorld columnAtPos:]`
    fn column_at_pos(&self, pos: IntPair) -> Option<DwObject>;
    /// `-[DynamicWorld stairsAtPos:]`
    fn stairs_at_pos(&self, pos: IntPair) -> Option<DwObject>;
    /// `-[DynamicWorld elevatorShaftAtPos:]`
    fn elevator_shaft_at_pos(&self, pos: IntPair) -> Option<DwObject>;
    /// `-[DynamicWorld paintingAtPos:]`
    fn painting_at_pos(&self, pos: IntPair) -> Option<DwObject>;
}

/// What the predicates read from an `ElevatorMotor`.
#[derive(Clone, Copy, Debug)]
pub struct ElevatorMotorInfo {
    pub has_required_power: bool,
    pub pos: IntPair,
}

/// A dynamic object returned by a `DynamicWorld` lookup, with the getters world_state reads from it
/// (`itemType`, `interactionObjectType`, `chestType`, `isPaintable`, sign placement info).
#[derive(Clone)]
pub struct DwObject {
    pub obj: ObjRef,
    /// `-itemType`
    pub item_type: i32,
    /// `-interactionObjectType`
    pub interaction_object_type: u16,
    /// `-chestType`
    pub chest_type: i32,
    /// `-isPaintable`
    pub is_paintable: bool,
    /// Sign placement info `(connectionType, offsetType)`; `None` when the object is not a Sign.
    pub sign_info: Option<(i32, i32)>,
}

/// PLACEHOLDER for `crate::dynworld::state::DynamicWorld` (owner dynamic_world_core).
pub struct DynamicWorld {
    pub queries: Box<dyn DynamicWorldQueries>,
}
impl DynamicWorld {
    pub fn new(queries: Box<dyn DynamicWorldQueries>) -> DynamicWorld {
        DynamicWorld { queries }
    }
    pub fn door_can_be_used_by_path_user(&self, path_user: Option<&ObjRef>, pos: IntPair) -> bool {
        self.queries.door_can_be_used_by_path_user(path_user, pos)
    }
    pub fn elevator_motor_for_shaft_at_pos(&self, pos: IntPair) -> Option<ElevatorMotorInfo> {
        self.queries.elevator_motor_for_shaft_at_pos(pos)
    }
    pub fn interaction_object_at_pos(&self, pos: IntPair) -> Option<DwObject> {
        self.queries.interaction_object_at_pos(pos)
    }
    pub fn ladder_at_pos(&self, pos: IntPair) -> Option<DwObject> {
        self.queries.ladder_at_pos(pos)
    }
    pub fn column_at_pos(&self, pos: IntPair) -> Option<DwObject> {
        self.queries.column_at_pos(pos)
    }
    pub fn stairs_at_pos(&self, pos: IntPair) -> Option<DwObject> {
        self.queries.stairs_at_pos(pos)
    }
    pub fn elevator_shaft_at_pos(&self, pos: IntPair) -> Option<DwObject> {
        self.queries.elevator_shaft_at_pos(pos)
    }
    pub fn painting_at_pos(&self, pos: IntPair) -> Option<DwObject> {
        self.queries.painting_at_pos(pos)
    }
    /// `-[DynamicWorld interactionObjectTypeForObjectAtPos:]`: 0 if none.
    pub fn interaction_object_type_for_object_at_pos(&self, pos: IntPair) -> u16 {
        self.queries.interaction_object_at_pos(pos).map_or(0, |o| o.interaction_object_type)
    }
}

/// PLACEHOLDER for `crate::worldgen::tile_loader::WorldTileLoader`.
pub struct WorldTileLoader {}
/// PLACEHOLDER for `crate::worldgen::noise::NoiseFunction`.
pub struct NoiseFunction {}
/// PLACEHOLDER for `crate::path::path_creator::PathCreator`.
pub struct PathCreator {}
/// PLACEHOLDER for `crate::path::path_creator::PathRequest`.
pub struct PathRequest {}
/// PLACEHOLDER for `crate::physics_objects::projectile_manager::ProjectileManager`.
pub struct ProjectileManager {}
/// PLACEHOLDER for `crate::server::server_client::ServerClient`.
pub struct ServerClient {}
/// PLACEHOLDER for `crate::world::db_convertor::DatabaseConvertor`.
pub struct DatabaseConvertor {}
/// PLACEHOLDER for `crate::env::weather::Weather`.
pub struct Weather {}
/// PLACEHOLDER for `crate::interaction::trade_portal::PortalChestManager`.
pub struct PortalChestManager {}
/// PLACEHOLDER for `crate::server::bhserver::BHServer`. `api` is the `ServerApi` face object_model needs.
pub struct BHServer {
    pub api: Option<Rc<dyn ServerApi>>,
}
/// PLACEHOLDER for `crate::dynworld::state::ServerClientMap`.
pub type ServerClientMap = Rc<RefCell<GsDict<String, Rc<ServerClient>>>>;

/// PLACEHOLDER for `crate::world::lifecycle::WorldDelegate` (the GameController). is the
/// only call world_state makes.
pub trait WorldDelegate {
    /// `-[delegate appDatabase]`
    fn app_database(&self) -> Option<Rc<Database>>;
}

/// The seam to world_ops (`world/blocks.rs`, `world/protection.rs`): world_state calls these, world_ops installs
/// an implementation with [`World::set_ops_hooks`]. Without hooks, loads are no-ops returning `false`.
pub trait WorldOpsHooks {
    /// `-[World loadPhysicalBlockForMacroTile:atX:y:loadSurroundingBlocks:createIfNotCreated:]`
    fn load_physical_block_for_macro_tile_at_x_y_load_surrounding_blocks_create_if_not_created(
        &self,
        world: &World,
        mt: &MacroTileRef,
        mx: i32,
        my: i32,
        load_surrounding_blocks: bool,
        create_if_not_created: bool,
    ) -> bool;
    /// `-[World loadDynamicObjectsIfNotAlreadyLoadedForMacroTile:includeSurfaceBlocks:]`
    fn load_dynamic_objects_if_not_already_loaded_for_macro_tile_include_surface_blocks(
        &self,
        world: &World,
        mt: &MacroTileRef,
        include_surface_blocks: bool,
    );
    /// `-[World tileIsProtectedAtPos:againstBlockhead:]`
    fn tile_is_protected_at_pos_against_blockhead(&self, world: &World, pos: IntPair, b: &ObjRef) -> bool;
}

// =====================================================================================================
// PhysicalBlock / MacroTile / TileRef
// =====================================================================================================

/// Handle to a physical block. Blocks are `Box::leak`ed and recycled, never deallocated (3.3).
pub type PhysicalBlockRef = &'static PhysicalBlock;
/// Handle to a macro tile (`MacroTile*`).
pub type MacroTileRef = Rc<MacroTile>;
/// What `World::macro_tiles()` returns (cheap clone).
pub type MacroTiles = Rc<MacroTileArray>;

/// `PhysicalBlock` (C: 0x148 bytes, calloc'ed).
pub struct PhysicalBlock {
    /// `xPos`, MACRO coordinates (tile x = x_pos * 32).
    pub x_pos: Cell<i32>,
    /// `yPos`, MACRO coordinates. A fresh block has -1000.
    pub y_pos: Cell<i32>,
    /// `Tile *tiles`: 1024 entries, index = local_y * 32 + local_x, local_y 0 is the bottom row.
    pub tiles: Box<[Cell<Tile>]>,
    pub valid: Cell<bool>,
    pub version: Cell<u8>,
    /// `lastRequiredTime` (NSTimeInterval)
    pub last_required_time: Cell<f64>,
    pub update_index: Cell<u32>,
    pub light_block_update_index: Cell<u32>,
    /// Per client light index; one byte per tile.
    pub client_light_blocks: RefCell<[Option<Box<[u8; 1024]>>; 32]>,
    pub client_explored: RefCell<[u8; 32]>,
}

impl PhysicalBlock {
    /// `calloc(1, 0x148)` followed by the `yPos = -1000` of a fresh block (3.3 `take_physical_block`).
    pub fn new_fresh() -> PhysicalBlock {
        PhysicalBlock {
            x_pos: Cell::new(0),
            y_pos: Cell::new(-1000),
            tiles: vec![Cell::new(Tile::default()); BLOCK_TILES].into_boxed_slice(),
            valid: Cell::new(false),
            version: Cell::new(0),
            last_required_time: Cell::new(0.0),
            update_index: Cell::new(0),
            light_block_update_index: Cell::new(0),
            client_light_blocks: RefCell::new(std::array::from_fn(|_| None)),
            client_explored: RefCell::new([0; 32]),
        }
    }

    /// Re-zero a recycled graveyard block to the `calloc` state (all fields zero, `yPos = -1000`).
    fn rezero(&self) {
        self.x_pos.set(0);
        self.y_pos.set(-1000);
        for t in self.tiles.iter() {
            t.set(Tile::default());
        }
        self.valid.set(false);
        self.version.set(0);
        self.last_required_time.set(0.0);
        self.update_index.set(0);
        self.light_block_update_index.set(0);
        for c in self.client_light_blocks.borrow_mut().iter_mut() {
            *c = None;
        }
        *self.client_explored.borrow_mut() = [0; 32];
    }

    /// `&block->tiles[local_index]` as a live handle.
    pub fn tile_ref(&'static self, local_index: usize) -> TileRef {
        TileRef::new(self, local_index)
    }
}

/// `MacroTile` (C: 0x28 bytes, calloc'ed => everything starts zero / None).
pub struct MacroTile {
    pub unexplored: Cell<u8>,
    /// `dirty != 0` means "needs reload".
    pub dirty: Cell<u8>,
    pub dynamic_objects_loaded: Cell<bool>,
    pub physical_block: RefCell<Option<PhysicalBlockRef>>,
    /// The `NSMutableArray` may be nil (D).
    pub dynamic_objects: RefCell<Option<Vec<ObjRef>>>,
    pub time_physical_block_created: Cell<f32>,
}

impl MacroTile {
    /// A calloc'ed `MacroTile`.
    pub fn new_zeroed() -> MacroTile {
        MacroTile {
            unexplored: Cell::new(0),
            dirty: Cell::new(0),
            dynamic_objects_loaded: Cell::new(false),
            physical_block: RefCell::new(None),
            dynamic_objects: RefCell::new(None),
            time_physical_block_created: Cell::new(0.0),
        }
    }
    /// `macroTile->physicalBlock` by value.
    pub fn physical_block(&self) -> Option<PhysicalBlockRef> {
        *self.physical_block.borrow()
    }
}

impl MacroTileApi for MacroTile {
    fn has_physical_block(&self) -> bool {
        self.physical_block.borrow().is_some()
    }
    fn dynamic_objects(&self) -> &RefCell<Option<Vec<ObjRef>>> {
        &self.dynamic_objects
    }
}

/// `MacroTile *macroTiles` array: `world_width_macro * 32` zeroed macro tiles, index = `my * W + mx`.
pub struct MacroTileArray {
    tiles: Vec<MacroTileRef>,
}

impl MacroTileArray {
    /// `calloc(count, 0x28)`.
    pub fn new_zeroed(count: usize) -> MacroTileArray {
        MacroTileArray { tiles: (0..count).map(|_| Rc::new(MacroTile::new_zeroed())).collect() }
    }
    pub fn len(&self) -> usize {
        self.tiles.len()
    }
    pub fn is_empty(&self) -> bool {
        self.tiles.is_empty()
    }
    /// `&macroTiles[index]`; `None` when out of range.
    pub fn get(&self, index: usize) -> Option<MacroTileRef> {
        self.tiles.get(index).cloned()
    }
}

/// The C `Tile*`: a LIVE handle into a physical block's tile array. Reads always see the latest write and the
/// handle stays valid after the block is recycled.
#[derive(Clone, Copy)]
pub struct TileRef {
    pb: PhysicalBlockRef,
    idx: u16,
}

impl TileRef {
    pub fn new(pb: PhysicalBlockRef, local_index: usize) -> TileRef {
        assert!(local_index < BLOCK_TILES);
        TileRef { pb, idx: local_index as u16 }
    }
    /// Copy the tile out (`Tile` is Copy, 0x40 bytes).
    #[inline]
    pub fn get(&self) -> Tile {
        self.pb.tiles[self.idx as usize].get()
    }
    #[inline]
    pub fn set(&self, t: Tile) {
        self.pb.tiles[self.idx as usize].set(t)
    }
    /// get, mutate, set. `f` must not re-enter tile code for this tile.
    pub fn update<R>(&self, f: impl FnOnce(&mut Tile) -> R) -> R {
        let mut t = self.get();
        let r = f(&mut t);
        self.set(t);
        r
    }
    pub fn physical_block(&self) -> PhysicalBlockRef {
        self.pb
    }
    pub fn local_index(&self) -> usize {
        self.idx as usize
    }

    pub fn type_index(&self) -> u8 {
        self.get().type_index
    }
    pub fn back_wall_type_index(&self) -> u8 {
        self.get().back_wall_type_index
    }
    pub fn zone_type_index(&self) -> u8 {
        self.get().zone_type_index
    }
    pub fn contents(&self) -> u8 {
        self.get().contents
    }
    pub fn partial_content_left(&self) -> u8 {
        self.get().partial_content_left
    }
    pub fn gather_progress(&self) -> u8 {
        self.get().gather_progress
    }
    pub fn light(&self) -> u8 {
        self.get().light
    }
    pub fn sun_light(&self) -> u8 {
        self.get().sun_light
    }
    pub fn season_offset(&self) -> u8 {
        self.get().season_offset
    }
    pub fn explored_fraction(&self) -> u8 {
        self.get().explored_fraction
    }
    pub fn terrain_slow_factor(&self) -> u8 {
        self.get().terrain_slow_factor
    }
    pub fn foreground_contents(&self) -> u8 {
        self.get().foreground_contents
    }
    pub fn background_contents(&self) -> u8 {
        self.get().background_contents
    }
    pub fn artificial_light_r(&self) -> u16 {
        self.get().artificial_light_r
    }
    pub fn artificial_light_g(&self) -> u16 {
        self.get().artificial_light_g
    }
    pub fn artificial_light_b(&self) -> u16 {
        self.get().artificial_light_b
    }
    pub fn artificial_heat(&self) -> i16 {
        self.get().artificial_heat
    }
    pub fn on_fire(&self) -> u8 {
        self.get().on_fire
    }

    pub fn set_type_index(&self, v: u8) {
        self.update(|t| t.retype(v))
    }
    pub fn set_back_wall_type_index(&self, v: u8) {
        self.update(|t| t.back_wall_type_index = v)
    }
    pub fn set_contents(&self, v: u8) {
        self.update(|t| t.contents = v)
    }
    pub fn set_partial_content_left(&self, v: u8) {
        self.update(|t| t.partial_content_left = v)
    }
    pub fn set_gather_progress(&self, v: u8) {
        self.update(|t| t.gather_progress = v)
    }
    pub fn set_light(&self, v: u8) {
        self.update(|t| t.light = v)
    }
    pub fn set_sun_light(&self, v: u8) {
        self.update(|t| t.sun_light = v)
    }
    pub fn set_foreground_contents(&self, v: u8) {
        self.update(|t| t.foreground_contents = v)
    }
    pub fn set_background_contents(&self, v: u8) {
        self.update(|t| t.background_contents = v)
    }
    pub fn set_terrain_slow_factor(&self, v: u8) {
        self.update(|t| t.terrain_slow_factor = v)
    }
    pub fn set_on_fire(&self, v: u8) {
        self.update(|t| t.on_fire = v)
    }
}

/// Pointer identity of the `Tile` slot.
impl PartialEq for TileRef {
    fn eq(&self, other: &TileRef) -> bool {
        std::ptr::eq(self.pb, other.pb) && self.idx == other.idx
    }
}
impl Eq for TileRef {}
impl std::fmt::Debug for TileRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "TileRef({:p}[{}])", self.pb, self.idx)
    }
}

// =====================================================================================================
// World
// =====================================================================================================

/// The four `initial*` dictionaries (all nil on the server, l.469).
#[derive(Default)]
pub struct InitialObjectQueues {
    pub create: Option<GsDict<String, GsDict<NsNumberKey, Vec<Vec<u8>>>>>,
    pub update: Option<GsDict<NsNumberKey, Vec<Vec<u8>>>>,
    pub creation_data_update: Option<GsDict<NsNumberKey, Vec<Vec<u8>>>>,
    pub remove: Option<GsDict<NsNumberKey, Vec<Vec<u8>>>>,
}

fn zero_vector() -> Vector {
    Vector { v: [0.0; 4] }
}
fn zero_vector2() -> Vector2 {
    Vector2 { x: 0.0, y: 0.0 }
}
fn zero_pair() -> IntPair {
    IntPair { x: 0, y: 0 }
}

/// Generates the `World` struct (every ivar its own `Cell` / `RefCell`), the calloc'ed constructor and the
/// trivial getters/setters of the `Cell` fields.
macro_rules! world_struct {
    (
        cells: [ $( ($f:ident, $set:ident, $t:ty, $init:expr) ),* $(,)? ],
        refs: [ $( ($rf:ident, $rt:ty, $rinit:expr) ),* $(,)? ]
    ) => {
        /// `World` (the class layout, size 0xF60). All ivars are zero/None/empty as calloc'ed memory.
        pub struct World {
            pub self_weak: Weak<World>,
            /// 0x010 `worldWidthMacro` (init only); width in tiles = *32.
            pub world_width_macro: i32,
            /// 0x1c0 `macroTiles`: `world_width_macro * 32` macro tiles.
            pub macro_tiles: MacroTiles,
            /// 0x218 `usedPhysicalBlocks`, allocation order (3.3).
            pub used_physical_blocks: RefCell<Vec<PhysicalBlockRef>>,
            /// 0x248 `freePhysicalBlocks`, insertion order (3.3).
            pub free_physical_blocks: RefCell<VecDeque<PhysicalBlockRef>>,
            /// Blocks the original `free()`s (3.3).
            pub graveyard: RefCell<Vec<PhysicalBlockRef>>,
            /// Seam to world_ops (see [`WorldOpsHooks`]).
            pub ops_hooks: RefCell<Option<Rc<dyn WorldOpsHooks>>>,
            $( pub $f: Cell<$t>, )*
            $( pub $rf: RefCell<$rt>, )*
        }

        impl World {
            fn new_calloced(self_weak: Weak<World>, world_width_macro: i32, macro_tiles: MacroTiles) -> World {
                World {
                    self_weak,
                    world_width_macro,
                    macro_tiles,
                    used_physical_blocks: RefCell::new(Vec::new()),
                    free_physical_blocks: RefCell::new(VecDeque::new()),
                    graveyard: RefCell::new(Vec::new()),
                    ops_hooks: RefCell::new(None),
                    $( $f: Cell::new($init), )*
                    $( $rf: RefCell::new($rinit), )*
                }
            }
            $(
                #[doc = concat!("Getter of ivar `", stringify!($f), "`.")]
                #[inline] pub fn $f(&self) -> $t { self.$f.get() }
                #[doc = concat!("Setter of ivar `", stringify!($f), "`.")]
                #[inline] pub fn $set(&self, v: $t) { self.$f.set(v) }
            )*
        }
    };
}

world_struct! {
    cells: [
        // ---- kept server ivars (3.4)
        (did_load_game, set_did_load_game, bool, false),
        (incremental_load_count, set_incremental_load_count, i32, 0),
        (dynamic_world_failed_to_load_wait_count_timer, set_dynamic_world_failed_to_load_wait_count_timer, i32, 0),
        (load_complete, set_load_complete, bool, false),
        (start_portal_pos, set_start_portal_pos, IntPair, zero_pair()),
        (custom_rules, set_custom_rules, CustomRules, CustomRules::default()),
        (expert_mode, set_expert_mode, bool, false),
        (loaded_version, set_loaded_version, i32, 0),
        (random_seed, set_random_seed, u32, 0),
        (creation_date, set_creation_date, Option<f64>, None),
        (world_time, set_world_time, f64, 0.0),
        (sun_direction, set_sun_direction, Vector, zero_vector()),
        (time_of_day_fraction, set_time_of_day_fraction, f32, 0.0),
        (day_color, set_day_color, Vector, zero_vector()),
        (weather_fraction, set_weather_fraction, f32, 0.0),
        (rain_fraction, set_rain_fraction, f32, 0.0),
        (rain_fraction_not_including_snow, set_rain_fraction_not_including_snow, f32, 0.0),
        (cloud_fraction, set_cloud_fraction, f32, 0.0),
        (fast_forward, set_fast_forward, bool, false),
        (save_count, set_save_count, f32, 0.0),
        (no_rain_timer, set_no_rain_timer, f32, 0.0),
        (send_heartbeat_request_timer, set_send_heartbeat_request_timer, f32, 0.0),
        (particle_random_number_index, set_particle_random_number_index, i32, 0),
        (decommision_counter, set_decommision_counter, i32, 0),
        (server_reports_all_paused, set_server_reports_all_paused, bool, false),
        (last_update_time, set_last_update_time, f64, 0.0),
        (time_left_to_simulate, set_time_left_to_simulate, f64, 0.0),
        (total_number_of_actions_to_simulate, set_total_number_of_actions_to_simulate, i32, 0),
        (total_time_to_simulate, set_total_time_to_simulate, f32, 0.0),
        (simulation_progress, set_simulation_progress, f32, 0.0),
        (is_simulating, set_is_simulating, bool, false),
        (save_next_update, set_save_next_update, bool, false),
        (final_save_completion_count, set_final_save_completion_count, i32, 0),
        (highest_point, set_highest_point, IntPair, zero_pair()),
        (dynamic_world_load_fail_count, set_dynamic_world_load_fail_count, i32, 0),
        (is_owner, set_is_owner, bool, false),
        (is_admin, set_is_admin, bool, false),
        (is_mod, set_is_mod, bool, false),
        (cloud_mode, set_cloud_mode, i32, 0),
        (save_delay, set_save_delay, i32, 0),
        (send_trade_transaction_timer, set_send_trade_transaction_timer, f32, 0.0),
        (multiplayer_server_send_trade_transaction_timer, set_multiplayer_server_send_trade_transaction_timer, f32, 0.0),
        (pause_idle_timer, set_pause_idle_timer, f32, 0.0),
        (pvp_enabled, set_pvp_enabled, bool, false),
        (portal_level, set_portal_level, i32, 0),
        (server_minor_version, set_server_minor_version, i32, 0),
        (server_privacy_setting, set_server_privacy_setting, BhNetPrivacy, BhNetPrivacy(0)),
        (clientside_credit, set_clientside_credit, f32, 0.0),
        (save_disabled, set_save_disabled, bool, false),
        (ownership_sign_positions_need_saving, set_ownership_sign_positions_need_saving, bool, false),
        (save_counter, set_save_counter, i32, 0),
        (lightblock_save_counter, set_lightblock_save_counter, i32, 0),
        (has_finished_database_migration_to17, set_has_finished_database_migration_to17, bool, false),
        (repair_mode, set_repair_mode, bool, false),
        // ---- client-state scalars kept as Cell (3.5)
        (supports_gyro, set_supports_gyro, bool, false),
        (is_observing_motion_events, set_is_observing_motion_events, bool, false),
        (calibrating, set_calibrating, bool, false),
        (has_calibrated, set_has_calibrated, bool, false),
        (long_term_averaged_acceleration, set_long_term_averaged_acceleration, Vector, zero_vector()),
        (drag_in_progress, set_drag_in_progress, bool, false),
        (forced_calibration_timer, set_forced_calibration_timer, f32, 0.0),
        (move_left_right_fraction, set_move_left_right_fraction, f32, 0.0),
        (move_up_down_fraction, set_move_up_down_fraction, f32, 0.0),
        (x_smooth, set_x_smooth, f32, 0.0),
        (pinch_scale, set_pinch_scale, f64, 0.0),
        (num_draw_blocks, set_num_draw_blocks, i32, 0),
        (update_geometry_draw_block_index, set_update_geometry_draw_block_index, i32, 0),
        (update_geometry_draw_block_timer, set_update_geometry_draw_block_timer, f32, 0.0),
        (rounded_translation, set_rounded_translation, Vector2, zero_vector2()),
        (accurate_translation, set_accurate_translation, Vector2, zero_vector2()),
        (translation_goal, set_translation_goal, Vector2, zero_vector2()),
        (translating_to_goal, set_translating_to_goal, bool, false),
        (background_y_offset, set_background_y_offset, f32, 0.0),
        (debug_has_created_ref_image, set_debug_has_created_ref_image, bool, false),
        (idle_timer_disabled, set_idle_timer_disabled, bool, false),
        (map_update_timer, set_map_update_timer, f32, 0.0),
        (connection_to_server_lost, set_connection_to_server_lost, bool, false),
        (initial_net_data_request_sent, set_initial_net_data_request_sent, bool, false),
        (client_last_got_heartbeat_counter, set_client_last_got_heartbeat_counter, f32, 0.0),
        (water_animation_index, set_water_animation_index, i32, 0),
        (water_animation_timer, set_water_animation_timer, f32, 0.0),
        (slow_animation_index, set_slow_animation_index, i32, 0),
        (slow_animation_timer, set_slow_animation_timer, f32, 0.0),
        (camera_min_x_macro, set_camera_min_x_macro, i32, 0),
        (camera_max_x_macro, set_camera_max_x_macro, i32, 0),
        (camera_min_y_macro, set_camera_min_y_macro, i32, 0),
        (camera_max_y_macro, set_camera_max_y_macro, i32, 0),
        (camera_min_x_world, set_camera_min_x_world, i32, 0),
        (camera_max_x_world, set_camera_max_x_world, i32, 0),
        (camera_min_y_world, set_camera_min_y_world, i32, 0),
        (camera_max_y_world, set_camera_max_y_world, i32, 0),
        (unable_to_craft_blockhead_due_to_waiting_for_server, set_unable_to_craft_blockhead_due_to_waiting_for_server, bool, false),
        (hide_ui_type, set_hide_ui_type, i32, 0),
        (double_time_unlocked, set_double_time_unlocked, bool, false),
        (needs_to_do_portal_screenshot, set_needs_to_do_portal_screenshot, bool, false),
        (has_just_taken_photo, set_has_just_taken_photo, bool, false),
        (last_music_play_time, set_last_music_play_time, f64, 0.0),
        (frame_counter, set_frame_counter, i32, 0),
        (show_blockhead_available_prompt_blockhead_with_corrrect_food_index, set_show_blockhead_available_prompt_blockhead_with_corrrect_food_index, i32, 0),
        (is_heading_towards_m_idday, set_is_heading_towards_m_idday, bool, false),
        (has_played_dying_song, set_has_played_dying_song, bool, false),
        (client_added_credit_timer, set_client_added_credit_timer, f32, 0.0),
        (has_shown1_minute_credit_warning, set_has_shown1_minute_credit_warning, bool, false),
        (has_shown10_minute_credit_warning, set_has_shown10_minute_credit_warning, bool, false),
        (has_shown1_hour_credit_warning, set_has_shown1_hour_credit_warning, bool, false),
        (client_side_credit_warning_delay, set_client_side_credit_warning_delay, f32, 0.0),
        (smoothed_camera_z_offset, set_smoothed_camera_z_offset, f32, 0.0),
        (client_zoom_request_sent, set_client_zoom_request_sent, bool, false),
        (following_blockhead, set_following_blockhead, bool, false),
        (touch_start_translation, set_touch_start_translation, Vector2, zero_vector2()),
        (last_pinch_pan_translation, set_last_pinch_pan_translation, Vector2, zero_vector2()),
        (last_distance_travelled_this_d_pad_movement, set_last_distance_travelled_this_d_pad_movement, Vector2, zero_vector2()),
        (dpad_control, set_dpad_control, bool, false),
        (dpad_direct_control_disabled, set_dpad_direct_control_disabled, bool, false),
        (found_items_list_needs_to_be_sent_to_server, set_found_items_list_needs_to_be_sent_to_server, bool, false),
        (render_repair_mode_confirm, set_render_repair_mode_confirm, bool, false),
        (repair_mode_confirm_pos, set_repair_mode_confirm_pos, IntPair, zero_pair()),
    ],
    refs: [
        (delegate, Option<Weak<dyn WorldDelegate>>, None),
        (window_info, Option<WindowInfo>, None),
        (weather, Option<Rc<Weather>>, None),
        (custom_rules_dict, Option<PlistDict>, None),
        (dynamic_world, Option<Rc<DynamicWorld>>, None),
        (world_tile_loader, Option<Rc<WorldTileLoader>>, None),
        (path_creator, Option<Rc<PathCreator>>, None),
        (path_queue, Vec<PathRequest>, Vec::new()),
        (save_id, Option<String>, None),
        (world_name, Option<String>, None),
        (host_port, Option<String>, None),
        (max_players, Option<String>, None),
        (free_client_light_block_indices, Option<IndexSet>, None),
        (weather_noise_function, Option<Rc<NoiseFunction>>, None),
        (distance_ordered_food_types, Vec<i32>, Vec::new()),
        (server_clients, Option<ServerClientMap>, None),
        (server, Option<Rc<BHServer>>, None),
        (initial_create_objects, InitialObjectQueues, InitialObjectQueues::default()),
        (particle_random_numbers, Vec<f32>, vec![0.0f32; 500]),
        (circum_navigate_booleans, Option<IndexSet>, None),
        (simulation_events, Option<PlistDict>, None),
        (away_client_simulation_events, Option<PlistDict>, None),
        (net_events_message_to_display_once_loaded, Option<String>, None),
        (save_queue, Option<OpQueue>, None),
        (projectile_manager, Option<Rc<ProjectileManager>>, None),
        (global_prices, Option<PlistDict>, None),
        (world_price_multipliers, Option<PlistDict>, None),
        (unsent_multiplayer_trade_transactions, Option<PlistDict>, None),
        (unsent_global_trade_transactions, Option<PlistDict>, None),
        (welcome_message, Option<String>, None),
        (muted_players, Vec<String>, Vec::new()),
        (server_password, Option<String>, None),
        (client_password, Option<String>, None),
        (ownership_sign_positions, Option<PlistDict>, None),
        (portal_chest_manager, Option<Rc<PortalChestManager>>, None),
        (zoom_player_cycle_counts, Option<PlistDict>, None),
        (found_items_list, Option<IndexSet>, None),
        (database_environment, Option<Rc<DatabaseEnvironment>>, None),
        (main_database, Option<Rc<Database>>, None),
        (dynamic_object_database, Option<Rc<Database>>, None),
        (block_database, Option<Rc<Database>>, None),
        (database_convertor, Option<Rc<DatabaseConvertor>>, None),
    ]
}

/// Maximum number of free physical blocks kept before `trim_free_physical_blocks` releases one.
const FREE_BLOCK_KEEP: usize = 32;

impl World {
    /// ObjC `+alloc` plus the width: builds the `Rc<World>` with every ivar zero/None/empty as calloc'ed memory;
    /// `macro_tiles` = `world_width_macro * 32` zeroed macro tiles (init: `calloc(W<<5, 0x28)`).
    /// `isAdmin/isMod/isOwner` are set by world_core's init, not here.
    pub fn alloc(world_width_macro: i32) -> Rc<World> {
        let count = (world_width_macro.max(0) as usize) * 32;
        let macro_tiles = Rc::new(MacroTileArray::new_zeroed(count));
        Rc::new_cyclic(|w| World::new_calloced(w.clone(), world_width_macro, macro_tiles))
    }

    /// Strong handle to self (`self_weak.upgrade()`).
    pub fn me(&self) -> Rc<World> {
        self.self_weak.upgrade().expect("World already deallocated")
    }

    /// Install the world_ops seam (see [`WorldOpsHooks`]).
    pub fn set_ops_hooks(&self, hooks: Option<Rc<dyn WorldOpsHooks>>) {
        *self.ops_hooks.borrow_mut() = hooks;
    }
    /// The installed hooks, cloned out so no borrow is held across the call.
    pub fn ops_hooks(&self) -> Option<Rc<dyn WorldOpsHooks>> {
        self.ops_hooks.borrow().clone()
    }

    // ---- for the non-Cell ivars (by value) ----

    /// `-worldWidthMacro`
    pub fn world_width_macro(&self) -> i32 {
        self.world_width_macro
    }
    /// `-macroTiles`
    pub fn macro_tiles(&self) -> MacroTiles {
        self.macro_tiles.clone()
    }
    /// `-dynamicWorld`
    pub fn dynamic_world(&self) -> Option<Rc<DynamicWorld>> {
        self.dynamic_world.borrow().clone()
    }
    /// `-saveID`
    pub fn save_id(&self) -> Option<String> {
        self.save_id.borrow().clone()
    }
    /// `-worldName`
    pub fn world_name(&self) -> Option<String> {
        self.world_name.borrow().clone()
    }
    /// `-windowInfo`
    pub fn window_info(&self) -> Option<WindowInfo> {
        *self.window_info.borrow()
    }
    /// `-serverClients`
    pub fn server_clients(&self) -> &RefCell<Option<ServerClientMap>> {
        &self.server_clients
    }
    /// `-server` (non-nil on the dedicated server)
    pub fn server(&self) -> Option<Rc<BHServer>> {
        self.server.borrow().clone()
    }
    /// `-client`: always nil on the server.
    pub fn client(&self) -> Option<std::convert::Infallible> {
        None
    }
    /// `-clientTileLoader`: always nil on the server.
    pub fn client_tile_loader(&self) -> Option<std::convert::Infallible> {
        None
    }
    /// `-uiManager`: always nil on the server.
    pub fn ui_manager(&self) -> Option<std::convert::Infallible> {
        None
    }
    /// `-tutorial`: always nil on the server.
    pub fn tutorial(&self) -> Option<std::convert::Infallible> {
        None
    }
    /// `-cloudInterface`: always nil on the server.
    pub fn cloud_interface(&self) -> Option<std::convert::Infallible> {
        None
    }
    /// `-cache`: always nil on the server.
    pub fn cache(&self) -> Option<std::convert::Infallible> {
        None
    }
    /// `-multiplayerLoadDict`: always nil on the server.
    pub fn multiplayer_load_dict(&self) -> Option<std::convert::Infallible> {
        None
    }
    /// `-dynamicObjectIDRequestInfoDicts`: always nil on the server.
    pub fn dynamic_object_id_request_info_dicts(&self) -> Option<std::convert::Infallible> {
        None
    }
    /// `-globalPrices`
    pub fn global_prices(&self) -> Option<PlistDict> {
        self.global_prices.borrow().clone()
    }
    /// `-worldPriceMultipliers`
    pub fn world_price_multipliers(&self) -> Option<PlistDict> {
        self.world_price_multipliers.borrow().clone()
    }
    /// `-portalChestManager`
    pub fn portal_chest_manager(&self) -> Option<Rc<PortalChestManager>> {
        self.portal_chest_manager.borrow().clone()
    }
    /// `-welcomeMessage`
    pub fn welcome_message(&self) -> Option<String> {
        self.welcome_message.borrow().clone()
    }
    /// `-serverPassword`
    pub fn server_password(&self) -> Option<String> {
        self.server_password.borrow().clone()
    }
    /// `-clientPassword`
    pub fn client_password(&self) -> Option<String> {
        self.client_password.borrow().clone()
    }
    /// `-foundItemsList`
    pub fn found_items_list(&self) -> &RefCell<Option<IndexSet>> {
        &self.found_items_list
    }
    /// `-appDatabase`: `[delegate appDatabase]`.
    pub fn app_database(&self) -> Option<Rc<Database>> {
        let d = self.delegate.borrow().as_ref().and_then(|w| w.upgrade());
        d.and_then(|d| d.app_database())
    }

    // ---- physical block pools (3.3) ----

    /// `-usedPhysicalBlocks`: snapshot copy of the used set, allocation order.
    pub fn used_physical_blocks(&self) -> Vec<PhysicalBlockRef> {
        self.used_physical_blocks.borrow().clone()
    }
    pub fn used_physical_blocks_len(&self) -> usize {
        self.used_physical_blocks.borrow().len()
    }
    pub fn free_physical_blocks_len(&self) -> usize {
        self.free_physical_blocks.borrow().len()
    }

    ///  pool part: when the free set is empty a new block is allocated
    /// (from the graveyard if available, re-zeroed; `y_pos = -1000`, tiles zeroed), else the front of the free
    /// set is taken (recycled blocks keep ALL old values); then it is inserted into the used set.
    /// `update_index`/`light_block_update_index` are NOT touched here.
    pub fn take_physical_block(&self) -> PhysicalBlockRef {
        let recycled = self.free_physical_blocks.borrow_mut().pop_front();
        let pb: PhysicalBlockRef = match recycled {
            Some(pb) => pb,
            None => {
                let from_grave = self.graveyard.borrow_mut().pop();
                match from_grave {
                    Some(pb) => {
                        pb.rezero();
                        pb
                    }
                    None => Box::leak(Box::new(PhysicalBlock::new_fresh())),
                }
            }
        };
        let mut used = self.used_physical_blocks.borrow_mut();
        if !used.iter().any(|u| std::ptr::eq(*u, pb)) {
            used.push(pb);
        }
        pb
    }

    /// : `for pb in eraseSet { free.insert(pb); used.erase(pb) }` (set semantics).
    pub fn move_to_free_physical_blocks(&self, blocks: &[PhysicalBlockRef]) {
        for &pb in blocks {
            {
                let mut free = self.free_physical_blocks.borrow_mut();
                if !free.iter().any(|f| std::ptr::eq(*f, pb)) {
                    free.push_back(pb);
                }
            }
            self.used_physical_blocks.borrow_mut().retain(|u| !std::ptr::eq(*u, pb));
        }
    }

    ///  tail: `if free.len() > 32 { pb = free.pop_front(); clear every client light
    /// block; graveyard.push(pb) }` (at most ONE block per call).
    pub fn trim_free_physical_blocks(&self) {
        if self.free_physical_blocks.borrow().len() > FREE_BLOCK_KEEP {
            let pb = self.free_physical_blocks.borrow_mut().pop_front().expect("more than FREE_BLOCK_KEEP free blocks");
            for c in pb.client_light_blocks.borrow_mut().iter_mut() {
                *c = None;
            }
            self.graveyard.borrow_mut().push(pb);
        }
    }

    /// World dealloc: nothing observable; blocks are leaked by design (3.3).
    pub fn release_all_physical_blocks(&self) {}
}

// =====================================================================================================
// WorldApi (object_model's view of the world)
// =====================================================================================================

impl WorldApi for World {
    fn world_width_macro(&self) -> i32 {
        self.world_width_macro
    }
    fn macro_pos_and_tile_for_world_pos(&self, pos: IntPair) -> (IntPair, Option<crate::objects::base::MacroTileRef>) {
        let mp = super::tiles::macro_pos_for_world_pos(pos, self);
        let mt = super::tiles::macro_tile_at_macro_postion(mp.x, mp.y, &self.macro_tiles, self);
        (mp, mt.map(|m| m as crate::objects::base::MacroTileRef))
    }
    fn is_admin(&self) -> bool {
        self.is_admin.get()
    }
    fn server(&self) -> Option<Rc<dyn ServerApi>> {
        self.server.borrow().as_ref().and_then(|s| s.api.clone())
    }
    fn world_time(&self) -> f64 {
        self.world_time.get()
    }
    fn load_physical_block_for_macro_tile_at_x_y_load_surrounding_blocks_create_if_not_created(
        &self,
        _tile: &crate::objects::base::MacroTileRef,
        mx: i32,
        my: i32,
        load_surrounding_blocks: bool,
        create_if_not_created: bool,
    ) -> bool {
        // The trait object cannot be downcast: the concrete tile is recovered from its (mx, my) position.
        match super::tiles::macro_tile_at_macro_postion(mx, my, &self.macro_tiles, self) {
            Some(mt) => self.run_block_loader(&mt, mx, my, load_surrounding_blocks, create_if_not_created),
            None => false,
        }
    }
    fn load_dynamic_objects_if_not_already_loaded_for_macro_tile_include_surface_blocks(
        &self,
        tile: &crate::objects::base::MacroTileRef,
        include_surface_blocks: bool,
    ) {
        let hooks = self.ops_hooks();
        if let Some(h) = hooks {
            // Find the concrete tile by identity.
            let id = crate::objects::base::tile_id(tile);
            let found = (0..self.macro_tiles.len())
                .filter_map(|i| self.macro_tiles.get(i))
                .find(|m| Rc::as_ptr(m) as *const () as usize == id);
            if let Some(mt) = found {
                h.load_dynamic_objects_if_not_already_loaded_for_macro_tile_include_surface_blocks(
                    self,
                    &mt,
                    include_surface_blocks,
                );
            }
        }
    }
    fn tile_is_protected_at_pos_against_blockhead(&self, pos: IntPair, b: &ObjRef) -> bool {
        match self.ops_hooks() {
            Some(h) => h.tile_is_protected_at_pos_against_blockhead(self, pos, b),
            None => false,
        }
    }
}

impl World {
    /// Calls the installed world_ops block loader; `false` when no hooks are installed.
    pub(crate) fn run_block_loader(
        &self,
        mt: &MacroTileRef,
        mx: i32,
        my: i32,
        load_surrounding: bool,
        create_if_not_created: bool,
    ) -> bool {
        match self.ops_hooks() {
            Some(h) => h.load_physical_block_for_macro_tile_at_x_y_load_surrounding_blocks_create_if_not_created(
                self,
                mt,
                mx,
                my,
                load_surrounding,
                create_if_not_created,
            ),
            None => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alloc_has_zeroed_macro_tiles() {
        let w = World::alloc(32);
        assert_eq!(w.macro_tiles().len(), 1024);
        assert!(w.macro_tiles().get(1023).unwrap().physical_block().is_none());
        assert_eq!(w.macro_tiles().get(5).unwrap().dirty.get(), 0);
        assert!(w.macro_tiles().get(1024).is_none());
        assert!(!w.is_admin() && !w.is_mod() && !w.is_owner());
        assert!(w.client().is_none() && w.ui_manager().is_none() && w.tutorial().is_none());
        assert_eq!(w.particle_random_numbers.borrow().len(), 500);
        assert_eq!(w.custom_rules().custom_rules_enabled, 0);
    }

    #[test]
    fn app_database_asks_the_delegate() {
        struct NoDatabase;
        impl WorldDelegate for NoDatabase {
            fn app_database(&self) -> Option<Rc<Database>> {
                None
            }
        }
        let w = World::alloc(32);
        assert!(w.app_database().is_none(), "no delegate");
        let delegate: Rc<dyn WorldDelegate> = Rc::new(NoDatabase);
        *w.delegate.borrow_mut() = Some(Rc::downgrade(&delegate));
        assert!(w.app_database().is_none(), "a delegate without a database");
        drop(delegate);
        assert!(w.app_database().is_none(), "a delegate that is gone");
    }

    #[test]
    fn tile_ref_shares_writes() {
        let w = World::alloc(32);
        let pb = w.take_physical_block();
        let a = TileRef::new(pb, 37);
        let b = TileRef::new(pb, 37);
        a.set_type_index(9);
        a.set_contents(0x30);
        assert_eq!(b.type_index(), 9);
        assert_eq!(b.contents(), 0x30);
        assert!(a == b);
        assert!(a != TileRef::new(pb, 38));
        b.update(|t| t.light = 200);
        assert_eq!(a.light(), 200);
    }

    #[test]
    fn fresh_block_and_pools() {
        let w = World::alloc(32);
        let pb = w.take_physical_block();
        assert_eq!(pb.y_pos.get(), -1000);
        assert!(pb.tiles.iter().all(|t| t.get() == Tile::default()));
        assert_eq!(w.used_physical_blocks_len(), 1);
        pb.x_pos.set(7);
        TileRef::new(pb, 3).set_type_index(55);
        w.move_to_free_physical_blocks(&[pb]);
        assert_eq!((w.used_physical_blocks_len(), w.free_physical_blocks_len()), (0, 1));
        // set semantics: moving again does not duplicate
        w.move_to_free_physical_blocks(&[pb]);
        assert_eq!(w.free_physical_blocks_len(), 1);
        // recycled block keeps stale values
        let again = w.take_physical_block();
        assert!(std::ptr::eq(pb, again));
        assert_eq!(again.x_pos.get(), 7);
        assert_eq!(TileRef::new(again, 3).type_index(), 55);
    }

    #[test]
    fn free_list_is_fifo_and_trim_releases_one() {
        let w = World::alloc(32);
        let blocks: Vec<_> = (0..34)
            .map(|i| {
                let b = w.take_physical_block();
                b.x_pos.set(i);
                b
            })
            .collect();
        w.move_to_free_physical_blocks(&blocks);
        assert_eq!(w.free_physical_blocks_len(), 34);
        w.trim_free_physical_blocks();
        assert_eq!(w.free_physical_blocks_len(), 33);
        assert_eq!(w.graveyard.borrow().len(), 1);
        assert_eq!(w.graveyard.borrow()[0].x_pos.get(), 0);
        w.trim_free_physical_blocks();
        assert_eq!(w.free_physical_blocks_len(), 32);
        w.trim_free_physical_blocks();
        assert_eq!(w.free_physical_blocks_len(), 32); // not > 32
                                                      // FIFO: next take returns block 2 (0 and 1 are in the graveyard)
        assert_eq!(w.take_physical_block().x_pos.get(), 2);
    }

    #[test]
    fn graveyard_tile_ref_stays_valid_and_block_reused_zeroed() {
        let w = World::alloc(32);
        let blocks: Vec<_> = (0..33).map(|_| w.take_physical_block()).collect();
        let t = TileRef::new(blocks[0], 1);
        t.set_type_index(9);
        w.move_to_free_physical_blocks(&blocks);
        w.trim_free_physical_blocks();
        assert_eq!(t.type_index(), 9); // dangling-but-mapped
                                       // drain the free list, then the graveyard block is re-zeroed and reused
        for _ in 0..32 {
            w.take_physical_block();
        }
        let re = w.take_physical_block();
        assert!(std::ptr::eq(re, blocks[0]));
        assert_eq!(re.y_pos.get(), -1000);
        assert_eq!(t.type_index(), 0);
    }

    #[test]
    fn world_api_impl_basic() {
        let w = World::alloc(32);
        w.set_is_admin(true);
        let api: &dyn WorldApi = &*w;
        assert!(api.is_admin());
        assert_eq!(api.world_width_macro(), 32);
        let (mp, mt) = api.macro_pos_and_tile_for_world_pos(IntPair { x: 33, y: 5000 });
        assert_eq!((mp.x, mp.y), (1, 31));
        assert!(mt.is_some());
        assert!(!api.tile_is_protected_at_pos_against_blockhead(
            IntPair { x: 0, y: 0 },
            &crate::world::helpers::testutil::new_obj()
        ));
    }
}

/// Accessors and the `WorldApi` view that no other test touches.
#[cfg(test)]
mod accessor_tests {
    use super::*;
    use crate::objects::base::WorldApi;
    use crate::world::helpers::testutil::new_obj;

    #[test]
    fn tile_accessors_debug_and_macro_tiles() {
        let pb: PhysicalBlockRef = Box::leak(Box::new(PhysicalBlock::new_fresh()));
        let t = TileRef::new(pb, 3);
        t.update(|x| {
            x.zone_type_index = 1;
            x.season_offset = 2;
            x.terrain_slow_factor = 3;
            x.artificial_light_r = 4;
            x.artificial_light_g = 5;
            x.artificial_light_b = 6;
            x.artificial_heat = -7;
        });
        assert_eq!((t.zone_type_index(), t.season_offset(), t.terrain_slow_factor()), (1, 2, 3));
        assert_eq!((t.artificial_light_r(), t.artificial_light_g(), t.artificial_light_b()), (4, 5, 6));
        assert_eq!(t.artificial_heat(), -7);
        assert!(format!("{:?}", t).starts_with("TileRef("));
        let mt = MacroTileArray::new_zeroed(2);
        assert!(!mt.is_empty() && mt.len() == 2);
        assert!(MacroTileArray::new_zeroed(0).is_empty());
    }

    #[test]
    fn world_getters_and_the_api_view() {
        let w = World::alloc(32);
        assert!(w.world_name().is_none() && w.welcome_message().is_none());
        assert!(w.server_password().is_none() && w.client_password().is_none());
        assert!(w.global_prices().is_none() && w.world_price_multipliers().is_none());
        assert!(w.portal_chest_manager().is_none());
        assert!(w.server_clients().borrow().is_none());
        assert!(w.found_items_list().borrow().is_none());
        assert!(w.app_database().is_none());
        assert!(w.client_tile_loader().is_none() && w.cloud_interface().is_none() && w.cache().is_none());
        assert!(w.multiplayer_load_dict().is_none() && w.dynamic_object_id_request_info_dicts().is_none());
        // the world api without hooks
        let obj = new_obj();
        assert!(!WorldApi::tile_is_protected_at_pos_against_blockhead(&*w, IntPair { x: 1, y: 1 }, &obj));
        let mt: crate::objects::base::MacroTileRef = w.macro_tiles().get(0).unwrap();
        assert!(!WorldApi::load_physical_block_for_macro_tile_at_x_y_load_surrounding_blocks_create_if_not_created(
            &*w, &mt, 0, 5000, false, false
        ));
        // and with the block hooks installed
        crate::world::blocks::install_ops_hooks(&w);
        assert!(!WorldApi::tile_is_protected_at_pos_against_blockhead(&*w, IntPair { x: 1, y: 1 }, &obj));
        // trimming drops one free block beyond the 32 kept
        let taken: Vec<_> = (0..FREE_BLOCK_KEEP + 2).map(|_| w.take_physical_block()).collect();
        for pb in taken {
            w.free_physical_blocks.borrow_mut().push_back(pb);
        }
        let before = w.free_physical_blocks_len();
        w.trim_free_physical_blocks();
        assert_eq!(w.free_physical_blocks_len(), before - 1);
    }
}
