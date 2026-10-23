//! `World` lifecycle: designated initializer, the LMDB open, `loadGame` / `loadDefaultGame`, the staged
//! `incrementalLoad` (stages 0-14), `saveAll`, `dealloc`, and the seam trait [`WorldCoreHooks`] through which
//! world_core reaches the collaborators that do not have a published API yet.
//!
//!  Methods",
//!
//! ## Seams
//! `World` (world_state) still names placeholder types for `DynamicWorld`, `WorldTileLoader`, `ServerClient`,
//! `BHServer`, `PathCreator`, `ProjectileManager`, `Weather`, `NoiseFunction` and `DatabaseConvertor`. They have no
//! methods, so every call into those classes goes through [`WorldCoreHooks`] (all methods have inert defaults).
//! The integrator installs the real implementation with [`set_core_hooks`] (one per thread, the server is
//! single-threaded) and, when the placeholders in `world/state.rs` are replaced by the real types, shrinks the hooks
//! to direct calls. The `World` ivars that hold those objects are still set to `Some(placeholder)` /
//! `None` exactly like the original sets the ObjC pointers, so every nil test of the spec keeps working.

use std::cell::RefCell;
use std::rc::{Rc, Weak};

use crate::foundation::clock;
use crate::foundation::gsdict::GsDict;
use crate::foundation::indexset::IndexSet;
use crate::foundation::nsnumber::{NsNumber, NsNumberKey};
use crate::foundation::paths;
use crate::nslog;
use crate::objects::handle::ObjRef;
use crate::plist::helpers::{data_from_property_list, property_list_from_data};
use crate::plist::keyed_archiver::{archive_index_set, unarchive_index_set};
use crate::plist::value::{ClassTag, Origin, PlistDict, PlistValue};
use crate::rng::glibc_rand;
use crate::storage::lmdb::{Database, DatabaseEnvironment};
use crate::types::netdata::ClientMacroBlockRequest;
use crate::types::structs::{ElectrictyParticlePathIndex, IntPair, Vector2, WindowInfo};
use crate::world::custom_rules::custom_rules_from_dict;
use crate::world::db_convertor::DatabaseConvertor as RealDatabaseConvertor;
use crate::world::state::{
    BHServer, DatabaseConvertor, DwObject, DynamicWorld, DynamicWorldQueries, ElevatorMotorInfo, MacroTileRef,
    NoiseFunction, PathCreator, PhysicalBlockRef, ProjectileManager, ServerClientMap, Weather, World, WorldTileLoader,
};
use crate::world::tiles::tile_at_world_position_loaded;

// =====================================================================================================
// WorldDelegate
// =====================================================================================================

/// The object stored in `World.delegate`. On the dedicated server it is the `GameController` (init:
/// `delegate:self`), NOT the CommandLineDelegate as line 27 says (R1).
///
/// `state::WorldDelegate` (the single-method placeholder in world_state, `app_database`) is the supertrait so that
/// the `World.delegate` field, which still has the placeholder type, can hold the same object by trait upcasting.
pub trait WorldDelegate: crate::world::state::WorldDelegate {
    /// `-getDefaultGameSaveForWorldWithID:`: an EMPTY dictionary.
    fn get_default_game_save_for_world_with_id(&self, save_id: &str) -> PlistDict;
    /// `-fileWriteFailed:`: `NSLog(@"failed to write to file:%@")`.
    fn file_write_failed(&self, save_path: &str);
    /// `-appDatabaseEnvironment`: `None` on the dedicated server.
    fn app_database_environment(&self) -> Option<Rc<DatabaseEnvironment>>;
    /// `-showWelcomeBackPopupWithMessage:`: empty body.
    fn show_welcome_back_popup_with_message(&self, message: Option<&str>);
    /// `-setPaused:`: empty body (used by world_ops `pause_updates`).
    fn set_paused(&self, paused: bool);
}

/// Arguments of the 13-keyword designated initializer `initWithWindowInfo:cache:delegate:saveID:name:client:server:
/// multiplayerWorldData:serverHostData:saveDelay:worldWidthMacro:customRules:expertMode:` (`cache`, `client` and
/// `multiplayerWorldData` are always nil on the server and omitted).
pub struct WorldInitArgs {
    /// GameController's `{1024, 768, 512, 384, 0, 0, 0, 0}` (R11).
    pub window_info: WindowInfo,
    /// `assign`, not retained.
    pub delegate: Weak<dyn WorldDelegate>,
    pub save_id: Option<String>,
    /// `worldName`
    pub name: Option<String>,
    /// `Some` on the dedicated server.
    pub server: Option<Rc<BHServer>>,
    /// `{"hostPort": String, "maxPlayers": String}`
    pub server_host_data: Option<PlistDict>,
    pub save_delay: i32,
    pub world_width_macro: i32,
    /// `Some(empty dict)` for `--convert-to-custum-rules`, else `None`.
    pub custom_rules: Option<PlistDict>,
    pub expert_mode: bool,
}

// =====================================================================================================
// The seam
// =====================================================================================================

crate::hooks::hook_trait! {
/// Every call world_core makes into classes that have no published API yet. All methods are inert by default
/// (`false` / `None` / empty / no-op) so a `World` can be driven end to end without them (unit tests, bring-up).
/// The method names are the selectors of the callee in snake_case, prefixed by the receiver. Receivers that the
/// original messages through an ivar (`worldTileLoader`, `dynamicWorld`,...) are NOT passed: the hook owner holds
/// them. World_core only calls a hook when the matching `World` ivar is `Some` (nil messaging otherwise).
#[allow(unused_variables)]
pub trait WorldCoreHooks, stub WorldCoreHooksStub {
    // ---------------------------------------------------------------- BHServer
    /// `-[BHServer sendNetworkData:toPeers:reliable:]`; `peers == None` is nil = all `connectedClients`.
    fn server_send_network_data_to_peers_reliable(&self, data: &[u8], peers: Option<&[String]>, reliable: bool) {}
    /// `-[BHServer credit]` (0.0 unless cloud).
    fn server_credit(&self) -> f32 {
        0.0
    }
    /// `-[BHServer isCloudMatch]`
    fn server_is_cloud_match(&self) -> bool {
        false
    }
    /// `-[BHServer privacyString]`
    fn server_privacy_string(&self) -> Option<String> {
        None
    }
    /// `[[[BHServer match] asServerMatch] privacy]` (the `isKindOfClass:[BHNetServerMatch class]` test is always true).
    fn server_match_privacy(&self) -> u64 {
        0
    }
    /// `-[BHServer serverDatabase]`
    fn server_database(&self) -> Option<Rc<Database>> {
        None
    }
    /// `-[BHServer startBulkTransaction]`
    fn server_start_bulk_transaction(&self) {}
    /// `-[BHServer finishBulkTransaction]`
    fn server_finish_bulk_transaction(&self) -> bool {
        true
    }
    /// `-[BHServer localPlayerName]`
    fn server_local_player_name(&self) -> String {
        String::new()
    }
    /// `-[BHServer localPlayerID]`
    fn server_local_player_id(&self) -> Option<String> {
        None
    }
    /// `-[BHServer playerNameForPlayerWithIDIncludingOldPlayers:]`
    fn server_player_name_for_player_with_id_including_old_players(&self, id: Option<&str>) -> Option<String> {
        None
    }

    // ---------------------------------------------------------------- ServerClient, keyed by client id
    /// `-[[ServerClient alloc] initWithClientID:server:lightBlockIndex:lightBlockDatabase:]`; the hook owner keeps the object.
    fn server_client_create(
        &self,
        client_id: &str,
        light_block_index: i32,
        light_block_database: Option<Rc<Database>>,
    ) {
    }
    /// `-connected`
    fn server_client_connected(&self, client_id: &str) -> bool {
        false
    }
    /// `-setConnected:` (also clears `paused`)
    fn server_client_set_connected(&self, client_id: &str, connected: bool) {}
    /// `-clientReconnected`
    fn server_client_reconnected(&self, client_id: &str) {}
    /// `-lightBlockIndex`
    fn server_client_light_block_index(&self, client_id: &str) -> i32 {
        0
    }
    /// `-requestForBlock:`
    fn server_client_request_for_block(&self, client_id: &str, request: ClientMacroBlockRequest) {}
    /// `-blockRemoved:`
    fn server_client_block_removed(&self, client_id: &str, macro_index: i32) {}
    /// `-blockIsWired:`
    fn server_client_block_is_wired(&self, client_id: &str, macro_index: i32) -> bool {
        false
    }
    /// `-paused`
    fn server_client_paused(&self, client_id: &str) -> bool {
        false
    }
    /// `-setPaused:`
    fn server_client_set_paused(&self, client_id: &str, paused: bool) {}

    // ---------------------------------------------------------------- WorldTileLoader
    /// `-[[WorldTileLoader alloc] initWithWorld:randomSeed:isNewWorld:saveID:loadedVersion:blockDatabase:]`; returns the
    /// loader's own `randomSeed` (it may differ), `None` keeps the seed.
    fn loader_init(
        &self,
        world: &Rc<World>,
        random_seed: u32,
        is_new_world: bool,
        save_id: &str,
        loaded_version: i32,
        block_database: Option<Rc<Database>>,
    ) -> Option<u32> {
        None
    }
    /// `-refineTerrain`
    fn loader_refine_terrain(&self) {}
    /// `-highestPoint`
    fn loader_highest_point(&self) -> IntPair {
        IntPair { x: 0, y: 0 }
    }
    /// `-bestStartPosition`
    fn loader_best_start_position(&self) -> IntPair {
        IntPair { x: 0, y: 0 }
    }
    /// `-distanceOrderedFoodTypes` (31 x i32)
    fn loader_distance_ordered_food_types(&self) -> Vec<i32> {
        vec![0; 31]
    }
    /// `-xFrequencyMultiplier`
    fn loader_x_frequency_multiplier(&self) -> i32 {
        0
    }
    /// `-lightBlockDatabase`
    fn loader_light_block_database(&self) -> Option<Rc<Database>> {
        None
    }
    /// `-startBulkLightBlockTransaction`
    fn loader_start_bulk_light_block_transaction(&self) {}
    /// `-finishBulkLightBlockTransaction` (YES when no transaction is open)
    fn loader_finish_bulk_light_block_transaction(&self) -> bool {
        true
    }
    /// `-unarchiveLightBlocksForClient:`
    fn loader_unarchive_light_blocks_for_client(&self, client_id: &str) {}
    /// `-setNeedsToExit:`
    fn loader_set_needs_to_exit(&self, needs_to_exit: bool) {}
    /// `-updatePhysicalBlockToLatestVersion:`
    fn loader_update_physical_block_to_latest_version(&self, pb: PhysicalBlockRef) {}

    // ---------------------------------------------------------------- misc constructors (stages 10 and 11)
    /// `-[[NoiseFunction alloc] initWithFrequencyX:frequencyY:frequencyZ:amplitude:seed:tileable:loop:persistance:]`
    fn weather_noise_function_init(
        &self,
        fx: i32,
        fy: i32,
        fz: i32,
        amplitude: f64,
        seed: i32,
        tileable: bool,
        loop_: bool,
        persistance: f64,
    ) {
    }
    /// `-[[Weather alloc] initWithCache:nil world:self worldTime:(float)worldTime]`
    fn weather_init(&self, world: &Rc<World>, world_time: f32) {}
    /// `-[[PathCreator alloc] initWithWorld:self]`
    fn path_creator_init(&self, world: &Rc<World>) {}
    /// `-[pathCreator inProgress]`
    fn path_creator_in_progress(&self) -> bool {
        false
    }
    /// `-[pathCreator pathUser]`
    fn path_creator_path_user(&self) -> Option<ObjRef> {
        None
    }
    /// `-[pathCreator abortPath]`
    fn path_creator_abort_path(&self) {}
    /// `-[[ProjectileManager alloc] initWithWorld:self cache:nil]`
    fn projectile_manager_init(&self, world: &Rc<World>) {}
    /// `-[projectileManager fireProjectileFrom:to:at:fireItemType:firer:]`
    fn projectile_manager_fire(
        &self,
        from: Vector2,
        to: Vector2,
        at: Option<&ObjRef>,
        item_type: i32,
        firer: Option<&ObjRef>,
    ) {
    }

    // ---------------------------------------------------------------- DynamicWorld (dynamic_world_core / ops)
    /// `-[[DynamicWorld alloc] initWithWorld:...]` (512 `rand()` draws inside). `None` = the placeholder with inert queries.
    fn dynamic_world_create(&self, world: &Rc<World>) -> Option<Rc<DynamicWorld>> {
        None
    }
    /// `-loadDynamicObjects:repositionBlockheadLoadFailures:`
    fn dynamic_world_load_dynamic_objects_reposition_blockhead_load_failures(
        &self,
        did_load_game: bool,
        reposition: bool,
    ) -> bool {
        true
    }
    /// `-setServer:serverClients:`
    fn dynamic_world_set_server_server_clients(&self, has_server: bool) {}
    /// `-conversionPump`
    fn dynamic_world_conversion_pump(&self) {}
    /// `-sendLightblocksToClients`
    fn dynamic_world_send_lightblocks_to_clients(&self) {}
    /// `-saveAndSendOnlyBlocksThatNeedToBeSent`
    fn dynamic_world_save_and_send_only_blocks_that_need_to_be_sent(&self) {}
    /// `-saveGameWithWorldData:signOwnershipData:`
    fn dynamic_world_save_game_with_world_data_sign_ownership_data(
        &self,
        world_data: Option<&[u8]>,
        sign_data: Option<&[u8]>,
    ) {
    }
    /// `[[dynamicWorld portal] level]`
    fn dynamic_world_portal_level(&self) -> Option<i32> {
        None
    }
    /// `-blockheads` each `previewData` (an `NSData`/dictionary, stored verbatim in `blockheadDatasv2`)
    fn dynamic_world_blockhead_previews(&self) -> Vec<PlistValue> {
        Vec::new()
    }
    /// `-blockheads` (index order)
    fn dynamic_world_blockheads(&self) -> Vec<ObjRef> {
        Vec::new()
    }
    /// `-allBlockheadsIncludingNet`
    fn dynamic_world_all_blockheads_including_net(&self) -> Vec<ObjRef> {
        Vec::new()
    }
    /// `-clientConnected:`
    fn dynamic_world_client_connected(&self, client_id: &str) {}
    /// `-clientDisconnected:simulate:`
    fn dynamic_world_client_disconnected_simulate(&self, client_id: &str, simulate: bool) {}
    /// `-stopAllBlockheadActionsForClientDueToKick:`
    fn dynamic_world_stop_all_blockhead_actions_for_client_due_to_kick(&self, client_id: &str) {}
    /// `-isControllingBlockheadsForClientPlayer:`
    fn dynamic_world_is_controlling_blockheads_for_client_player(&self, player_id: &str) -> bool {
        false
    }
    /// `-lightChangedAtMacroPos:sendReliably:`
    fn dynamic_world_light_changed_at_macro_pos_send_reliably(&self, macro_pos: IntPair, send_reliably: bool) {}
    /// `-remoteCreate:forObjectsOfType:clientID:`
    fn dynamic_world_remote_create(&self, objects: Vec<Vec<u8>>, object_type: i32, client_id: Option<&str>) {}
    /// `-remoteUpdate:forObjectsOfType:fromClient:`
    fn dynamic_world_remote_update(&self, objects: Vec<Vec<u8>>, object_type: i32, client_id: Option<&str>) {}
    /// `-remoteCreationDataUpdate:forObjectsOfType:fromClient:`
    fn dynamic_world_remote_creation_data_update(
        &self,
        objects: Vec<Vec<u8>>,
        object_type: i32,
        client_id: Option<&str>,
    ) {
    }
    /// `-remoteRemove:forObjectsOfType:fromClient:`
    fn dynamic_world_remote_remove(&self, objects: Vec<Vec<u8>>, object_type: i32, client_id: Option<&str>) {}
    /// `-userMuteChanged:`
    fn dynamic_world_user_mute_changed(&self, player_id: &str) {}
    /// `-playersChanged`
    fn dynamic_world_players_changed(&self) {}
    /// `-appendDebugLog:`
    fn dynamic_world_append_debug_log(&self, log: &mut String) {}
    /// `-finishSimulating`
    fn dynamic_world_finish_simulating(&self) {}
    /// `-simulate:`
    fn dynamic_world_simulate(&self, dt: f32) {}
    /// `-setPaused:`
    fn dynamic_world_set_paused(&self, paused: bool) {}

    // ---------------------------------------------------------------- Blockhead (blockhead_core / actions)
    /// `-[Blockhead name]`
    fn blockhead_name(&self, b: &ObjRef) -> Option<String> {
        None
    }
    /// `-[Blockhead clientID]`
    fn blockhead_client_id(&self, b: &ObjRef) -> Option<String> {
        None
    }
    /// `-[Blockhead isClientBlockheadBeingControlledByServer]`
    fn blockhead_is_client_blockhead_being_controlled_by_server(&self, b: &ObjRef) -> bool {
        false
    }
    /// `-[Blockhead shouldContinueSimulating]`
    fn blockhead_should_continue_simulating(&self, b: &ObjRef) -> bool {
        false
    }
    /// `-[Blockhead sufferDamage:isSimulation:recoil:]`
    fn blockhead_suffer_damage_is_simulation_recoil(&self, b: &ObjRef, damage: f32, is_simulation: bool, recoil: bool) {
    }
    /// `-[Blockhead dieForGood]`
    fn blockhead_die_for_good(&self, b: &ObjRef) {}
    /// `-asleep` / `-regenerating` / `-meditating` / `-onTradeMission` / `-hasActions` / `-actionCount` /
    /// `-cancelSimulateDueToCollapse` / `-currentInteractionRequiresHumanInput` (offline simulation, dead on the server)
    fn blockhead_asleep(&self, b: &ObjRef) -> bool {
        false
    }
    fn blockhead_regenerating(&self, b: &ObjRef) -> bool {
        false
    }
    fn blockhead_meditating(&self, b: &ObjRef) -> bool {
        false
    }
    fn blockhead_on_trade_mission(&self, b: &ObjRef) -> bool {
        false
    }
    fn blockhead_has_actions(&self, b: &ObjRef) -> bool {
        false
    }
    fn blockhead_action_count(&self, b: &ObjRef) -> i32 {
        0
    }
    fn blockhead_cancel_simulate_due_to_collapse(&self, b: &ObjRef) -> bool {
        false
    }
    fn blockhead_current_interaction_requires_human_input(&self, b: &ObjRef) -> bool {
        false
    }

    // ---------------------------------------------------------------- World helpers owned by other modules
    /// `[WorldHelper checkIfMacroTileCanBeDecommissioned:world:minAge:blockToSavePhyscialBlock:]` (lighting_env)
    fn world_helper_check_if_macro_tile_can_be_decommissioned(
        &self,
        world: &World,
        mt: Option<&MacroTileRef>,
        min_age: f64,
        block_to_save: Option<PhysicalBlockRef>,
    ) {
    }
    /// `-[World fullyLoadIfNeededAroundPos:clientLightBlockIndex:forBlockhead:nil]` (world_ops; `-1` = none)
    fn world_fully_load_if_needed_around_pos_client_light_block_index(
        &self,
        world: &World,
        pos: IntPair,
        client_light_block_index: i32,
    ) {
    }
    /// `-[World placeWorkbenchOfType:1 atPos:saveDict:nil placedByClient:nil placedByBlockhead:nil placedByClientName:nil]` (world_ops)
    fn world_place_workbench_of_type_at_pos(&self, world: &World, workbench_type: i32, pos: IntPair) {}
    /// `-[World loadLightBlockForClientLightBlockIndex:intoPhysicalBlock:]` (world_ops)
    fn world_load_light_block_for_client_light_block_index_into_physical_block(
        &self,
        world: &World,
        index: i32,
        pb: PhysicalBlockRef,
    ) {
    }
    /// `-[World welcomeBackMessageForInfoDict:simulationEvents]` (world_ops)
    fn world_welcome_back_message_for_simulation_events(&self, world: &World) -> Option<String> {
        None
    }
    /// `-[World pauseButtonTapped]` (world_ops)
    fn world_pause_button_tapped(&self, world: &World) {}
    /// the path-request pump that follows `[dynamicWorld simulate:]` in `continueSimulate`
    fn world_path_request_pump(&self, world: &World) {}

    // ---------------------------------------------------------------- singletons (env module)
    /// `[[ParticleEmitter instance] setWorldWidthMacro:]`
    fn particle_emitter_set_world_width_macro(&self, width_macro: i32) {}
    /// `[[ParticleEmitter instance] doAddElectricityParticleWithPath:size:]`
    fn particle_emitter_do_add_electricity_particle_with_path_size(
        &self,
        path: &[ElectrictyParticlePathIndex],
        size: f32,
    ) {
    }
    /// `[[TipManager instance] setWorld:world]; [[TipManager instance] reset]`
    fn tip_manager_set_world_and_reset(&self, world: &Rc<World>) {}
    /// `[[TipManager instance] update:dt]`
    fn tip_manager_update(&self, dt: f32) {}
    /// `[[TipManager instance] displayTip:text withTimeOut:10.0f displayEvenIfDisabled:NO tipColor:white]`
    fn tip_manager_display_tip(&self, text: &str) {}
    /// `[[TipManager instance] currentTip]`
    fn tip_manager_current_tip(&self) -> Option<String> {
        None
    }
}
}

/// The hooks used until [`set_core_hooks`] is called: every method inert.
pub struct NullCoreHooks;
impl WorldCoreHooksStub for NullCoreHooks {}

thread_local! {
    static CORE_HOOKS: RefCell<Rc<dyn WorldCoreHooks>> = RefCell::new(Rc::new(NullCoreHooks));
    static DELEGATE: RefCell<Option<Weak<dyn WorldDelegate>>> = const { RefCell::new(None) };
}

/// Install the seam implementation (per thread; the server is single-threaded).
pub fn set_core_hooks(hooks: Rc<dyn WorldCoreHooks>) {
    CORE_HOOKS.with(|h| *h.borrow_mut() = hooks);
}

/// The installed hooks, cloned out so no borrow is held across a call.
pub fn core_hooks() -> Rc<dyn WorldCoreHooks> {
    CORE_HOOKS.with(|h| h.borrow().clone())
}

/// `placeholder` queries for a `DynamicWorld` created without a hook implementation.
struct NullQueries;
impl DynamicWorldQueries for NullQueries {
    fn door_can_be_used_by_path_user(&self, _p: Option<&ObjRef>, _pos: IntPair) -> bool {
        false
    }
    fn elevator_motor_for_shaft_at_pos(&self, _pos: IntPair) -> Option<ElevatorMotorInfo> {
        None
    }
    fn interaction_object_at_pos(&self, _pos: IntPair) -> Option<DwObject> {
        None
    }
    fn ladder_at_pos(&self, _pos: IntPair) -> Option<DwObject> {
        None
    }
    fn column_at_pos(&self, _pos: IntPair) -> Option<DwObject> {
        None
    }
    fn stairs_at_pos(&self, _pos: IntPair) -> Option<DwObject> {
        None
    }
    fn elevator_shaft_at_pos(&self, _pos: IntPair) -> Option<DwObject> {
        None
    }
    fn painting_at_pos(&self, _pos: IntPair) -> Option<DwObject> {
        None
    }
}

// =====================================================================================================
// Small shared helpers (also used by the sibling files of this module)
// =====================================================================================================

/// A fresh `NSMutableDictionary` as a [`PlistDict`] (1 bucket, mutable tag).
pub(crate) fn new_mutable_dict() -> PlistDict {
    PlistDict { map: GsDict::new(), tag: ClassTag { origin: Origin::Built, mutable: true, alt: false } }
}

/// `[[NSMutableDictionary alloc] initWithDictionary:d]` (capacity = count, enumeration order of the source).
pub(crate) fn mutable_copy_of_dict(d: &PlistDict) -> PlistDict {
    PlistDict { map: GsDict::copy_from(&d.map), tag: ClassTag { origin: Origin::Built, mutable: true, alt: false } }
}

/// `-[NSObject mutableCopy]` of a container (arrays and dictionaries become mutable, other values are returned as is).
pub(crate) fn mutable_copy_of_value(v: &PlistValue) -> PlistValue {
    match v {
        PlistValue::Array(a) => {
            let mut a = a.clone();
            a.tag = ClassTag { origin: Origin::Built, mutable: true, alt: false };
            PlistValue::Array(a)
        }
        PlistValue::Dict(d) => PlistValue::Dict(mutable_copy_of_dict(d)),
        other => other.clone(),
    }
}

/// `[NSMutableArray array]` holding `items`.
pub(crate) fn new_mutable_array(items: Vec<PlistValue>) -> PlistValue {
    let mut a = PlistValue::mutable_array();
    if let PlistValue::Array(arr) = &mut a {
        arr.items = items;
    }
    a
}

/// `NSNumber -hash` key for the integer type argument of the `initial*` queues (`numberWithInt:`).
pub(crate) fn int_key(v: i32) -> NsNumberKey {
    NsNumberKey(NsNumber::Int(v as i64))
}

/// Atomic file write (`writeToFile:atomically:YES`): temp file in the same directory, then rename.
pub(crate) fn write_file_atomically(path: &str, bytes: &[u8]) -> bool {
    let tmp = format!("{}.tmp{}", path, std::process::id());
    if std::fs::write(&tmp, bytes).is_err() {
        let _ = std::fs::remove_file(&tmp);
        return false;
    }
    if std::fs::rename(&tmp, path).is_err() {
        let _ = std::fs::remove_file(&tmp);
        return false;
    }
    true
}

/// `"{appsup}/saves/{id}"`, the directory of a save (no trailing slash).
pub(crate) fn save_dir_for(id: &str) -> String {
    format!("{}/saves/{}", paths::appsup(), id)
}

/// Pointer identity of two objects (`==` on ObjC pointers).
pub(crate) fn same_object(a: &ObjRef, b: &ObjRef) -> bool {
    Rc::as_ptr(a) as *const () == Rc::as_ptr(b) as *const ()
}

/// Little-endian bytes of 31 food type ids (`NSData dataWithBytes:distanceOrderedFoodTypes length:0x7c`).
fn food_types_to_bytes(v: &[i32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(124);
    for i in 0..31 {
        out.extend_from_slice(&v.get(i).copied().unwrap_or(0).to_le_bytes());
    }
    out
}

fn str_of(d: &PlistValue, key: &str) -> Option<String> {
    d.object_for_key(key).and_then(|v| v.as_str()).map(|s| s.to_string())
}

fn date_secs(v: Option<&PlistValue>) -> Option<f64> {
    match v {
        Some(PlistValue::Date(s)) => Some(*s),
        _ => None,
    }
}

// =====================================================================================================
// impl World: init, set_server, delete_timers, dealloc
// =====================================================================================================

impl World {
    /// The `GameController` stored in `delegate` (full trait).
    pub(crate) fn core_delegate(&self) -> Option<Rc<dyn WorldDelegate>> {
        DELEGATE.with(|d| d.borrow().as_ref().and_then(|w| w.upgrade()))
    }

    /// Test helper: the delegate `World::init` would store.
    #[cfg(test)]
    pub(crate) fn set_core_delegate_for_tests(d: Option<Weak<dyn WorldDelegate>>) {
        DELEGATE.with(|x| *x.borrow_mut() = d);
    }

    /// `[server sendNetworkData:data toPeers:peers reliable:reliable]` (no-op without a server).
    pub(crate) fn send_to_peers(&self, data: &[u8], peers: Option<&[String]>, reliable: bool) {
        if self.server.borrow().is_some() {
            core_hooks().server_send_network_data_to_peers_reliable(data, peers, reliable);
        }
    }

    /// Whether `serverClients[client_id]` is non-nil.
    pub(crate) fn has_server_client(&self, client_id: &str) -> bool {
        let map: Option<ServerClientMap> = self.server_clients.borrow().clone();
        map.map_or(false, |m| m.borrow().contains_key(&client_id.to_string()))
    }

    /// `[serverClients[id] connected]` (nil receiver gives NO).
    pub(crate) fn server_client_is_connected(&self, client_id: &str) -> bool {
        self.has_server_client(client_id) && core_hooks().server_client_connected(client_id)
    }

    /// Keys of `serverClients` in GNUstep enumeration order.
    pub(crate) fn server_client_keys(&self) -> Vec<String> {
        let map: Option<ServerClientMap> = self.server_clients.borrow().clone();
        map.map_or_else(Vec::new, |m| m.borrow().keys_in_order())
    }

    /// `-[World initWithWindowInfo:cache:delegate:saveID:name:client:server:multiplayerWorldData:serverHostData:
    /// saveDelay:worldWidthMacro:customRules:expertMode:]`, server branch (client always nil). Always `Some` on the
    /// server.
    pub fn init(args: WorldInitArgs) -> Option<Rc<World>> {
        let world = World::alloc(args.world_width_macro);
        nslog!("loading world with size:{} expertMode:{}", args.world_width_macro, args.expert_mode as i32);
        world.set_expert_mode(args.expert_mode);
        world.set_save_delay(if args.save_delay < 2 { 1 } else { args.save_delay });
        *world.window_info.borrow_mut() = Some(args.window_info);
        // `delegate` is assign (not retained): keep the weak handle, both as the full trait and as the placeholder trait.
        let up: Weak<dyn crate::world::state::WorldDelegate> = args.delegate.clone();
        *world.delegate.borrow_mut() = Some(up);
        DELEGATE.with(|d| *d.borrow_mut() = Some(args.delegate.clone()));
        *world.save_id.borrow_mut() = args.save_id.clone();
        *world.world_name.borrow_mut() = args.name.clone();
        if let Some(d) = &args.custom_rules {
            *world.custom_rules_dict.borrow_mut() = Some(mutable_copy_of_dict(d));
        }
        let rules = {
            let d = world.custom_rules_dict.borrow();
            custom_rules_from_dict(d.as_ref(), args.expert_mode)
        };
        world.set_custom_rules(rules);
        let hooks = core_hooks();
        hooks.particle_emitter_set_world_width_macro(args.world_width_macro);
        hooks.tip_manager_set_world_and_reset(&world);
        *world.found_items_list.borrow_mut() = Some(IndexSet::new());
        // dpadControl defaults read from NSUserDefaults are client UI state (false).
        if (args.save_id.is_none() || args.name.is_none()) && args.server.is_none() {
            nslog!("Attempting to load invalid world");
            return None;
        }
        world.set_cloud_mode(0);
        let has_server = args.server.is_some();
        *world.server.borrow_mut() = args.server;
        if has_server {
            *world.server_clients.borrow_mut() = Some(Rc::new(RefCell::new(GsDict::new())));
            let mut set = IndexSet::new();
            for i in 0..32u64 {
                set.add_index(i);
            }
            *world.free_client_light_block_indices.borrow_mut() = Some(set);
        }
        // client == nil
        world.set_is_mod(true);
        world.set_is_admin(true);
        world.set_is_owner(true);
        if let Some(h) = &args.server_host_data {
            *world.host_port.borrow_mut() =
                h.get(&"hostPort".to_string()).and_then(|v| v.as_str()).map(|s| s.to_string());
            *world.max_players.borrow_mut() =
                h.get(&"maxPlayers".to_string()).and_then(|v| v.as_str()).map(|s| s.to_string());
        }
        // 500 draws from the process-global rand(); overwritten by the srand of stage 1 (R2: divisor 2147483648.0f).
        {
            let mut t = world.particle_random_numbers.borrow_mut();
            t.clear();
            for _ in 0..500 {
                t.push(glibc_rand::rand_tagged("World.init.particleRandomNumbers") as f32 / 2147483648.0f32);
            }
        }
        Some(world)
    }

    /// `-[World setServer:]`. BHServer never calls it on the dedicated server.
    pub fn set_server(&self, s: Option<Rc<BHServer>>) {
        let same = match (&*self.server.borrow(), &s) {
            (Some(a), Some(b)) => Rc::ptr_eq(a, b),
            (None, None) => true,
            _ => false,
        };
        if same {
            return;
        }
        *self.server_clients.borrow_mut() = None;
        let has = s.is_some();
        *self.server.borrow_mut() = s;
        if has {
            *self.server_clients.borrow_mut() = Some(Rc::new(RefCell::new(GsDict::new())));
        }
        if self.dynamic_world.borrow().is_some() {
            core_hooks().dynamic_world_set_server_server_clients(has);
        }
    }

    /// `-[World deleteTimers]`: `clientTileLoader` is nil, a no-op.
    pub fn delete_timers(&self) {}

    /// `-[World dealloc]` (explicit call, never `Drop`): `finishBulkDatabaseUpdate`, `Exiting World.`, the loader is
    /// told to exit, pools are freed (nothing observable).
    pub fn dealloc(&self) {
        let _ = crate::world::pre_update::core_finish_bulk_database_update(self);
        nslog!("Exiting World.");
        if self.world_tile_loader.borrow().is_some() {
            core_hooks().loader_set_needs_to_exit(true);
        }
        self.release_all_physical_blocks();
    }

    /// `-[World fileWriteFailed:]`: forwards to the delegate.
    pub fn file_write_failed(&self, save_path: &str) {
        if let Some(d) = self.core_delegate() {
            d.file_write_failed(save_path);
        }
    }

    /// `-[World setTranslation:]` (camera; the sound listener call is dropped).
    pub fn set_translation(&self, t: Vector2) {
        let mut accurate = t;
        let mut goal = self.translation_goal();
        let w = (self.world_width_macro << 5) as f32;
        if accurate.x < w {
            if accurate.x < 0.0 {
                accurate.x += w;
                if goal.x < 0.0 {
                    goal.x += w;
                }
            }
        } else {
            accurate.x -= w;
            if goal.x >= w {
                goal.x -= w;
            }
        }
        self.set_translation_goal(goal);
        self.set_accurate_translation(accurate);
        let s = self.pinch_scale().max(1.0);
        let step = (1.0f64 / (40.0f64 / s)) as f32;
        // fmodf on the ORIGINAL argument
        let rx = t.x - crate::fmath::libm::fmodf(t.x, step);
        let ry = t.y - crate::fmath::libm::fmodf(t.y, step);
        self.set_rounded_translation(Vector2 { x: rx, y: ry });
    }

    /// `-[World translation]` -> `accurateTranslation`.
    pub fn translation(&self) -> Vector2 {
        self.accurate_translation()
    }

    /// `-[World tutorialActive]`: `tutorial != nil`, always NO on the server.
    pub fn tutorial_active(&self) -> bool {
        false
    }

    /// `-[World dieConfirmationConfirmed:]` -> `[b dieForGood]`.
    pub fn die_confirmation_confirmed(&self, b: &ObjRef) {
        core_hooks().blockhead_die_for_good(b);
    }

    /// `-[World appendDebugLog:]`.
    pub fn append_debug_log(&self, log: &mut String) {
        log.push_str(&format!("loaded macro blocks:{}\n", self.used_physical_blocks_len()));
        log.push_str(&format!("free macro blocks:{}\n\n", self.free_physical_blocks_len()));
        if self.dynamic_world.borrow().is_some() {
            core_hooks().dynamic_world_append_debug_log(log);
        }
    }

    // =================================================================================================
    // databases and game load
    // =================================================================================================

    /// `-[World initializeDatabases]`. Failures log and `exit(0)`.
    pub fn initialize_databases(&self) {
        let id = self.save_id.borrow().clone().unwrap_or_default();
        let path = format!("{}/saves/{}/world_db/", paths::appsup(), id);
        let env = DatabaseEnvironment::new(&path, 4, 0x400);
        *self.database_environment.borrow_mut() = env.clone();
        let (main, blocks, dw) = match &env {
            Some(e) => (
                Database::new(e, "main").map(Rc::new),
                Database::new(e, "blocks").map(Rc::new),
                Database::new(e, "dw").map(Rc::new),
            ),
            None => (None, None, None),
        };
        *self.main_database.borrow_mut() = main.clone();
        *self.block_database.borrow_mut() = blocks.clone();
        *self.dynamic_object_database.borrow_mut() = dw;
        if main.is_none() {
            nslog!("Failed to load world mainDatabase.");
            crate::foundation::sys::exit(0);
        }
        if blocks.is_none() {
            nslog!("Failed to load blockDatabase.");
            crate::foundation::sys::exit(0);
        }
    }

    /// `-[World loadGame]` -> BOOL.
    pub fn load_game(&self) -> bool {
        let id = self.save_id.borrow().clone().unwrap_or_default();
        let base = paths::appsup();
        let main = self.main_database.borrow().clone();
        let mut save_dict: Option<PlistValue> = None;
        if let Some(d) = main.as_ref().and_then(|m| m.data_for_key(Some("worldv2"))) {
            save_dict = property_list_from_data(&d);
        }
        if save_dict.is_none() {
            let p = format!("{}/saves/{}/worldv2", base, id);
            if std::path::Path::new(&p).exists() {
                if let Ok(bytes) = std::fs::read(&p) {
                    save_dict = property_list_from_data(&bytes);
                }
            }
        }
        if save_dict.is_none() {
            // legacy `world` file: `+[NSDictionary dictionaryWithContentsOfFile:]`: missing file ->
            // nil silently; XML / OpenStep accepted; binary plists, garbage and non-dict roots -> nil + one NSLog warning.
            let p = format!("{}/saves/{}/world", base, id);
            if let Ok(bytes) = std::fs::read(&p) {
                let parsed = if bytes.starts_with(b"bplist") {
                    None
                } else {
                    crate::foundation::exception::catch_ns(|| property_list_from_data(&bytes)).ok().flatten()
                };
                match parsed {
                    Some(v @ PlistValue::Dict(_)) => save_dict = Some(v),
                    _ => nslog!(
                        "File NSDictionary.m: 635. In -[NSDictionary initWithContentsOfFile:] Contents of file '{}' does not contain a dictionary",
                        p
                    ),
                }
            }
        }
        if let Some(sd) = &save_dict {
            if self.custom_rules_dict.borrow().is_none() {
                // `[[NSMutableDictionary alloc] initWithDictionary:incoming]` with no class check: `count` is sent first,
                // then `keyEnumerator` when count > 0.
                match sd.object_for_key("customRulesDict") {
                    None => {}
                    Some(PlistValue::Dict(incoming)) => {
                        *self.custom_rules_dict.borrow_mut() = Some(mutable_copy_of_dict(incoming));
                    }
                    Some(v @ PlistValue::Array(a)) => {
                        if !a.items.is_empty() {
                            crate::foundation::exception::unrecognized_selector(v.class_name(), "keyEnumerator");
                        }
                        *self.custom_rules_dict.borrow_mut() = Some(PlistDict {
                            map: GsDict::new(),
                            tag: ClassTag { origin: Origin::Built, mutable: true, alt: false },
                        });
                    }
                    Some(v) => crate::foundation::exception::unrecognized_selector(v.class_name(), "count"),
                }
                let rules = {
                    let d = self.custom_rules_dict.borrow();
                    custom_rules_from_dict(d.as_ref(), self.expert_mode())
                };
                self.set_custom_rules(rules);
                self.custom_rules_changed();
            }
        }
        let sd = match save_dict {
            Some(sd) => sd,
            None => return false,
        };
        if sd.object_for_key("randomSeed").is_none() {
            return false;
        }
        let num = |k: &str| sd.object_for_key(k);
        self.set_has_finished_database_migration_to17(num("migrationComplete_1.7").map_or(false, |v| v.bool_value()));
        *self.host_port.borrow_mut() = str_of(&sd, "hostPort");
        *self.max_players.borrow_mut() = str_of(&sd, "maxPlayers");
        self.set_random_seed(num("randomSeed").map_or(0, |v| v.unsigned_int_value()));
        let (mut x, mut y) = (0.0f32, 0.0f32);
        if let Some(PlistValue::Array(tr)) = num("translation") {
            x = tr.items.first().map_or(0.0, |v| v.float_value());
            y = tr.items.get(1).map_or(0.0, |v| v.float_value());
        }
        self.set_rounded_translation(Vector2 { x, y });
        self.set_accurate_translation(Vector2 { x, y });
        self.set_world_time(num("worldTime").map_or(0.0, |v| v.double_value()));
        self.set_no_rain_timer(num("noRainTimer").map_or(0.0, |v| v.float_value()));
        self.set_last_update_time(date_secs(num("saveDate")).unwrap_or(0.0));
        self.set_creation_date(date_secs(num("creationDate")));
        self.set_start_portal_pos(IntPair {
            x: num("startPortalPos.x").map_or(0, |v| v.int_value()),
            y: num("startPortalPos.y").map_or(0, |v| v.int_value()),
        });
        self.set_portal_level(num("portalLevel").map_or(0, |v| v.int_value()));
        self.set_loaded_version(num("saveVersion").map_or(0, |v| v.int_value()));
        *self.welcome_message.borrow_mut() = str_of(&sd, "welcomeMessage");
        self.set_pvp_enabled(!num("pvpDisabled").map_or(false, |v| v.bool_value()));
        *self.server_password.borrow_mut() = str_of(&sd, "serverPassword");

        // found items (client == nil)
        let mut saved: Option<IndexSet> = None;
        if let Some(PlistValue::Data(d)) = num("foundItems") {
            saved = unarchive_index_set(&d.bytes, "foundItems");
        }
        match saved {
            Some(set) => {
                if set.count() != 0 {
                    if let Some(f) = self.found_items_list.borrow_mut().as_mut() {
                        f.add_indexes(&set);
                    }
                }
            }
            None => {
                let player_id = str_of(&sd, "playerID");
                let mut arr: Option<&PlistValue> = None;
                if let Some(p) = &player_id {
                    arr = num(&format!("{}_blockheadDatasv2", p));
                }
                if arr.is_none() {
                    arr = num("blockheadDatasv2");
                }
                let mut legacy: Option<&PlistValue> = None;
                if arr.is_none() {
                    if let Some(p) = &player_id {
                        legacy = num(&format!("{}_blockheadDatas", p));
                    }
                    if legacy.is_none() {
                        legacy = num("blockheadDatas");
                    }
                }
                let count = |v: Option<&PlistValue>| match v {
                    Some(PlistValue::Array(a)) => a.items.len(),
                    _ => 0,
                };
                if count(arr) != 0 || count(legacy) != 0 {
                    if let Some(f) = self.found_items_list.borrow_mut().as_mut() {
                        f.add_indexes_in_range(0, 0x12e);
                        f.add_indexes_in_range(0x400, 0x48);
                    }
                }
            }
        }

        if core_hooks().server_is_cloud_match() {
            let pw = self.server_password.borrow().is_some();
            nslog!("{}", if pw { "CLOUD:SETTING:PASSWORD:ON" } else { "CLOUD:SETTING:PASSWORD:OFF" });
            nslog!("{}", if self.pvp_enabled() { "CLOUD:SETTING:PVP:ON" } else { "CLOUD:SETTING:PVP:OFF" });
        }
        if let Some(PlistValue::Data(fd)) = num("distanceOrderedFoodTypes") {
            if fd.bytes.len() >= 0x7c {
                // UB-PARITY (R4): the original memcpy's the full length into a 124-byte buffer; copy min(len, 124).
                let mut v = vec![0i32; 31];
                for (i, c) in fd.bytes[..124].chunks_exact(4).enumerate() {
                    v[i] = i32::from_le_bytes([c[0], c[1], c[2], c[3]]);
                }
                *self.distance_ordered_food_types.borrow_mut() = v;
            }
        }
        let mut circ: Option<IndexSet> = None;
        match num("circumNavigateBooleansData") {
            None => {
                let p = format!("{}/saves/{}/circumNavigateBooleans", base, id);
                if let Ok(bytes) = std::fs::read(&p) {
                    circ = unarchive_index_set(&bytes, "root");
                }
            }
            Some(PlistValue::Data(d)) => circ = unarchive_index_set(&d.bytes, "root"),
            Some(_) => {}
        }
        *self.circum_navigate_booleans.borrow_mut() = Some(circ.unwrap_or_default());

        // ownership signs (client == nil)
        let legacy_path = format!("{}/saves/{}/signOwnershipData", base, id);
        let legacy_exists = std::path::Path::new(&legacy_path).exists();
        let mut sd_bytes = main.as_ref().and_then(|m| m.data_for_key(Some("signOwnershipData")));
        if sd_bytes.is_none() {
            if legacy_exists {
                sd_bytes = std::fs::read(&legacy_path).ok();
            }
        } else if legacy_exists {
            let _ = std::fs::remove_dir_all(&legacy_path).or_else(|_| std::fs::remove_file(&legacy_path));
        }
        if let Some(b) = sd_bytes {
            if let Some(PlistValue::Dict(dict)) = property_list_from_data(&b) {
                if dict.map.len() != 0 {
                    let mut out = new_mutable_dict();
                    for (k, v) in dict.map.iter() {
                        out.map.insert(k.clone(), mutable_copy_of_value(v));
                    }
                    *self.ownership_sign_positions.borrow_mut() = Some(out);
                }
            }
        }
        true
    }

    /// `-[World loadDefaultGame]` (new world).
    pub fn load_default_game(&self) {
        let rules = self.custom_rules();
        if rules.seed_type == 1 {
            self.set_random_seed(rules.custom_seed_value);
        } else {
            self.set_random_seed(clock::unix_time() as u32);
        }
        self.set_rounded_translation(Vector2 { x: 0.0, y: 640.0 });
        self.set_accurate_translation(Vector2 { x: 0.0, y: 640.0 });
        *self.circum_navigate_booleans.borrow_mut() = Some(IndexSet::new());
        self.set_has_finished_database_migration_to17(true);
        self.set_pvp_enabled(true);
        if core_hooks().server_is_cloud_match() {
            nslog!("CLOUD:SETTING:PASSWORD:OFF");
            nslog!("CLOUD:SETTING:PVP:ON");
        }
    }

    // =================================================================================================
    // incremental load
    // =================================================================================================

    /// `-[World incrementalLoad]`: one stage per call, stage = `incremental_load_count`; after stage 14 nothing.
    pub fn incremental_load(&self) {
        let count = self.incremental_load_count();
        let hooks = core_hooks();
        let me = self.me();
        match count {
            0 => {
                if self.save_id.borrow().is_none() {
                    crate::foundation::sys::exit(0);
                }
                self.initialize_databases();
                self.set_did_load_game(true);
                if !self.load_game() {
                    self.set_did_load_game(false);
                    self.load_default_game();
                }
                // R6: the analytics event is an empty function in the original.
                self.set_incremental_load_count(count + 1);
            }
            1 => {
                glibc_rand::srand_tagged("World.incrementalLoad.stage1", self.random_seed());
                let id = self.save_id.borrow().clone().unwrap_or_default();
                let block_db = self.block_database.borrow().clone();
                let new_seed = hooks.loader_init(
                    &me,
                    self.random_seed(),
                    !self.did_load_game(),
                    &id,
                    self.loaded_version(),
                    block_db,
                );
                *self.world_tile_loader.borrow_mut() = Some(Rc::new(WorldTileLoader {}));
                if let Some(s) = new_seed {
                    if s != self.random_seed() {
                        self.set_random_seed(s);
                    }
                }
                nslog!("using seed:{}", self.random_seed() as i32);
                nslog!("save delay:{}", self.save_delay());
                glibc_rand::srand_tagged("World.incrementalLoad.stage1b", self.random_seed());
                self.set_incremental_load_count(count + 1);
            }
            2..=9 => {
                if !self.did_load_game() {
                    if self.world_tile_loader.borrow().is_some() {
                        hooks.loader_refine_terrain();
                    }
                    self.set_incremental_load_count(count + 1);
                } else {
                    self.set_incremental_load_count(10);
                }
            }
            10 => {
                let has_loader = self.world_tile_loader.borrow().is_some();
                self.set_highest_point(if has_loader { hooks.loader_highest_point() } else { IntPair { x: 0, y: 0 } });
                if !self.did_load_game() {
                    let p = if has_loader { hooks.loader_best_start_position() } else { IntPair { x: 0, y: 0 } };
                    self.set_start_portal_pos(p);
                    let t = Vector2 { x: p.x as f32, y: p.y as f32 };
                    self.set_rounded_translation(t);
                    self.set_accurate_translation(t);
                    nslog!("best start pos:{:.1} {:.1}", t.x as f64, t.y as f64);
                    self.set_world_time(0.0);
                    self.set_no_rain_timer(0.0);
                    let w = (self.world_width_macro << 5) as f32;
                    let sx = p.x as f32;
                    if w * 0.5f32 < sx {
                        self.set_world_time(450.0);
                    }
                    let mut season = 0.25f32;
                    if w * 0.25f32 < sx && sx < w * 0.75f32 {
                        season = 0.75f32;
                    }
                    let wt = (4.0f32 * season * 900.0f32) as f64 + self.world_time();
                    self.set_world_time(wt);
                }
                if self.distance_ordered_food_types.borrow().is_empty() {
                    let mut v = hooks.loader_distance_ordered_food_types();
                    v.resize(31, 0);
                    *self.distance_ordered_food_types.borrow_mut() = v;
                }
                let xf = if has_loader { hooks.loader_x_frequency_multiplier() } else { 0 };
                hooks.weather_noise_function_init(
                    xf + 2,
                    xf + 2,
                    2,
                    0.9,
                    self.random_seed().wrapping_add(0xe) as i32,
                    true,
                    false,
                    1.8,
                );
                *self.weather_noise_function.borrow_mut() = Some(Rc::new(NoiseFunction {}));
                hooks.weather_init(&me, self.world_time() as f32);
                *self.weather.borrow_mut() = Some(Rc::new(Weather {}));
                self.set_incremental_load_count(count + 1);
            }
            11 => {
                hooks.path_creator_init(&me);
                *self.path_creator.borrow_mut() = Some(Rc::new(PathCreator {}));
                self.path_queue.borrow_mut().clear();
                hooks.projectile_manager_init(&me);
                *self.projectile_manager.borrow_mut() = Some(Rc::new(ProjectileManager {}));
                self.set_incremental_load_count(count + 1);
            }
            12 => self.incremental_load_stage_12(&hooks, &me, count),
            13 => {
                if !self.did_load_game() {
                    let p = self.start_portal_pos();
                    hooks.world_fully_load_if_needed_around_pos_client_light_block_index(self, p, -1);
                    for yo in 0..=1 {
                        if let Some(t) = tile_at_world_position_loaded(p.x, p.y + yo, self) {
                            t.update(|t| {
                                t.background_contents = 0;
                                t.foreground_contents = 0;
                                t.contents = 0;
                                t.type_index = 2;
                                t.back_wall_type_index = 2;
                            });
                        }
                    }
                    hooks.world_place_workbench_of_type_at_pos(self, 1, p);
                    if let Some(t) = tile_at_world_position_loaded(p.x, p.y - 1, self) {
                        t.update(|t| {
                            t.type_index = 0x19;
                            t.contents = 0;
                            t.zone_type_index = 1;
                        });
                    }
                }
                self.set_incremental_load_count(count + 1);
            }
            14 => {
                let p = self.start_portal_pos();
                let _ = tile_at_world_position_loaded(p.x, p.y, self);
                if !self.has_finished_database_migration_to17() {
                    let loader_lb = if self.world_tile_loader.borrow().is_some() {
                        hooks.loader_light_block_database()
                    } else {
                        None
                    };
                    let server_db = if self.server.borrow().is_some() { hooks.server_database() } else { None };
                    if let (Some(w), Some(d), Some(b)) = (
                        self.main_database.borrow().clone(),
                        self.dynamic_object_database.borrow().clone(),
                        self.block_database.borrow().clone(),
                    ) {
                        let conv = RealDatabaseConvertor::new(&me, w, d, b, loader_lb, server_db);
                        crate::world::db_convertor::set_convertor(Some(conv));
                        *self.database_convertor.borrow_mut() = Some(Rc::new(DatabaseConvertor {}));
                    }
                }
                self.set_load_complete(true);
                self.set_incremental_load_count(count + 1);
                let msg = self.net_events_message_to_display_once_loaded.borrow().clone();
                if let Some(m) = msg {
                    if let Some(d) = self.core_delegate() {
                        d.show_welcome_back_popup_with_message(Some(&m));
                    }
                }
            }
            _ => {}
        }
    }

    /// Stage 12: build the `DynamicWorld`, load the dynamic objects, replay the `initial*` queues.
    fn incremental_load_stage_12(&self, hooks: &Rc<dyn WorldCoreHooks>, me: &Rc<World>, count: i32) {
        if self.dynamic_world_failed_to_load_wait_count_timer() < 1 {
            let dw =
                hooks.dynamic_world_create(me).unwrap_or_else(|| Rc::new(DynamicWorld::new(Box::new(NullQueries))));
            *self.dynamic_world.borrow_mut() = Some(dw);
            let ok =
                hooks.dynamic_world_load_dynamic_objects_reposition_blockhead_load_failures(self.did_load_game(), true);
            if !ok {
                self.set_dynamic_world_failed_to_load_wait_count_timer(
                    self.dynamic_world_failed_to_load_wait_count_timer() + 1,
                );
                self.set_dynamic_world_load_fail_count(self.dynamic_world_load_fail_count() + 1);
                *self.dynamic_world.borrow_mut() = None;
            } else {
                let q = std::mem::take(&mut *self.initial_create_objects.borrow_mut());
                for i in 0..=0x40i32 {
                    let key = int_key(i);
                    if let Some(create) = &q.create {
                        for client_id in create.keys_in_order() {
                            let objs =
                                create.get(&client_id).and_then(|per| per.get(&key)).cloned().unwrap_or_default();
                            if !objs.is_empty() {
                                hooks.dynamic_world_remote_create(objs, i, Some(&client_id));
                            }
                        }
                    }
                    let upd = q.update.as_ref().and_then(|d| d.get(&key)).cloned().unwrap_or_default();
                    if !upd.is_empty() {
                        hooks.dynamic_world_remote_update(upd.clone(), i, None);
                    }
                    let cd = q.creation_data_update.as_ref().and_then(|d| d.get(&key)).cloned().unwrap_or_default();
                    if !cd.is_empty() {
                        // quirk confirmed in asm: the ORIGINAL passes `upd`, not `cd`.
                        hooks.dynamic_world_remote_creation_data_update(upd.clone(), i, None);
                    }
                    let rem = q.remove.as_ref().and_then(|d| d.get(&key)).cloned().unwrap_or_default();
                    if !rem.is_empty() {
                        hooks.dynamic_world_remote_remove(rem, i, None);
                    }
                }
                self.set_incremental_load_count(count + 1);
            }
        } else {
            let t = self.dynamic_world_failed_to_load_wait_count_timer() + 1;
            self.set_dynamic_world_failed_to_load_wait_count_timer(t);
            if t == 0x3c {
                self.set_dynamic_world_failed_to_load_wait_count_timer(0);
            }
        }
    }

    // =================================================================================================
    // save
    // =================================================================================================

    /// `-[World worldInfoForiCloudSave]`.
    pub fn world_info_for_i_cloud_save(&self) -> PlistDict {
        let mut d = new_mutable_dict();
        d.map.insert("portalLevel".to_string(), PlistValue::int(self.portal_level() as i64));
        let arr = if self.dynamic_world.borrow().is_some() {
            core_hooks().dynamic_world_blockhead_previews()
        } else {
            Vec::new()
        };
        d.map.insert("blockheadDatasv2".to_string(), new_mutable_array(arr));
        d
    }

    /// `-[World saveAll]`: builds the world dictionary (`main/worldv2`) and hands it to the `DynamicWorld`, which writes
    /// it. Runs before and during loading (nothing is written while `dynamic_world` is `None`, R7).
    pub fn save_all(&self) {
        let (save_id, world_name) = match (self.save_id.borrow().clone(), self.world_name.borrow().clone()) {
            (Some(a), Some(b)) => (a, b),
            _ => return,
        };
        // Mods (not in the original; `None` without mods).
        let mods = crate::mods::hooks();
        if let Some(h) = &mods {
            h.world_save(false);
        }
        let hooks = core_hooks();
        let mut d = match self.core_delegate() {
            Some(del) => mutable_copy_of_dict(&del.get_default_game_save_for_world_with_id(&save_id)),
            None => new_mutable_dict(),
        };
        let mut put = |k: &str, v: PlistValue| {
            d.map.insert(k.to_string(), v);
        };
        // migrationComplete_1.7
        let mut flag = true;
        if !self.has_finished_database_migration_to17() {
            flag = self.core_convertor().map_or(false, |c| c.migration_complete());
        }
        self.set_has_finished_database_migration_to17(flag);
        put("migrationComplete_1.7", PlistValue::boolean(flag));
        put("randomSeed", PlistValue::int(self.random_seed() as i64));
        let rt = self.rounded_translation();
        put("translation", PlistValue::array(vec![PlistValue::real_f32(rt.x), PlistValue::real_f32(rt.y)]));
        put("worldTime", PlistValue::real_f64(self.world_time()));
        put("noRainTimer", PlistValue::real_f32(self.no_rain_timer()));
        put("expertMode", PlistValue::boolean(self.expert_mode()));
        let sp = self.start_portal_pos();
        put("startPortalPos.x", PlistValue::int(sp.x as i64));
        put("startPortalPos.y", PlistValue::int(sp.y as i64));
        put("saveID", PlistValue::string(save_id.clone()));
        put("worldWidthMacro", PlistValue::int(self.world_width_macro as i64));
        put("saveDate", PlistValue::date(clock::reference_time()));
        put("worldName", PlistValue::string(world_name));
        if let Some(h) = self.host_port.borrow().clone() {
            put("hostPort", PlistValue::string(h));
        }
        if !self.pvp_enabled() {
            put("pvpDisabled", PlistValue::boolean(true));
        }
        if let Some(m) = self.max_players.borrow().clone() {
            put("maxPlayers", PlistValue::string(m));
        }
        put("runAtLaunch", PlistValue::boolean(true));
        if self.creation_date().is_none() {
            self.set_creation_date(Some(clock::reference_time()));
        }
        if let Some(c) = self.creation_date() {
            put("creationDate", PlistValue::date(c));
        }
        put("saveVersion", PlistValue::int(1100));
        put("remoteGame", PlistValue::boolean(false));
        match self.server_password.borrow().clone() {
            Some(p) => put("serverPassword", PlistValue::string(p)),
            None => {
                d.map.remove(&"serverPassword".to_string());
            }
        }
        match self.client_password.borrow().clone() {
            Some(p) => {
                d.map.insert("clientPassword".to_string(), PlistValue::string(p));
            }
            None => {
                d.map.remove(&"clientPassword".to_string());
            }
        }
        if self.server.borrow().is_some() && hooks.server_is_cloud_match() {
            match hooks.server_privacy_string() {
                Some(p) => {
                    d.map.insert("privacy".to_string(), PlistValue::string(p));
                }
                None => {
                    d.map.remove(&"privacy".to_string());
                }
            }
        }
        if let Some(f) = self.found_items_list.borrow().as_ref() {
            d.map.insert("foundItems".to_string(), PlistValue::data(archive_index_set(f, "foundItems")));
        }
        let has_dw = self.dynamic_world.borrow().is_some();
        if has_dw {
            if let Some(l) = hooks.dynamic_world_portal_level() {
                self.set_portal_level(l);
            }
        }
        d.map.insert("portalLevel".to_string(), PlistValue::int(self.portal_level() as i64));
        let previews = if has_dw { hooks.dynamic_world_blockhead_previews() } else { Vec::new() };
        d.map.insert("blockheadDatasv2".to_string(), new_mutable_array(previews));
        if let Some(c) = self.custom_rules_dict.borrow().as_ref() {
            d.map.insert("customRulesDict".to_string(), PlistValue::Dict(c.clone()));
        }
        {
            let f = self.distance_ordered_food_types.borrow();
            if !f.is_empty() {
                d.map.insert("distanceOrderedFoodTypes".to_string(), PlistValue::data(food_types_to_bytes(&f)));
            }
        }
        {
            let c = self.circum_navigate_booleans.borrow();
            let empty = IndexSet::new();
            d.map.insert(
                "circumNavigateBooleansData".to_string(),
                PlistValue::data(archive_index_set(c.as_ref().unwrap_or(&empty), "root")),
            );
        }
        if let Some(w) = self.welcome_message.borrow().clone() {
            d.map.insert("welcomeMessage".to_string(), PlistValue::string(w));
        }

        let world_data = data_from_property_list(&PlistValue::Dict(d));
        let mut sign_data: Option<Vec<u8>> = None;
        if self.ownership_sign_positions_need_saving() {
            if let Some(s) = self.ownership_sign_positions.borrow().clone() {
                sign_data = data_from_property_list(&PlistValue::Dict(s));
            }
            self.set_ownership_sign_positions_need_saving(false);
        }
        if has_dw {
            hooks.dynamic_world_save_game_with_world_data_sign_ownership_data(
                world_data.as_deref(),
                sign_data.as_deref(),
            );
        }
        if let Some(h) = mods {
            h.world_save(true);
        }
    }
}

// =====================================================================================================
// Test kit
// =====================================================================================================

/// Recording hooks and fixtures shared by the unit tests of the world_core files.
#[cfg(test)]
pub(crate) mod testkit {
    use super::*;
    use std::cell::Cell;
    use std::collections::{BTreeMap, HashSet};
    use std::sync::{Arc, Mutex};

    /// Serialises tests that change `HOME` (the save root is derived from it).
    pub static HOME_LOCK: Mutex<()> = Mutex::new(());

    #[derive(Default, Clone)]
    pub struct Cl {
        pub connected: bool,
        pub paused: bool,
        pub lbi: i32,
        pub wired: HashSet<i32>,
        pub requested: Vec<i32>,
        pub removed: Vec<i32>,
    }

    /// Per-blockhead data keyed by `unique_id`.
    #[derive(Default, Clone)]
    pub struct Bh {
        pub client_id: Option<String>,
        pub name: Option<String>,
        pub controlled_by_server: bool,
        pub should_continue: bool,
        pub damage_log: Vec<(f32, bool, bool)>,
        pub asleep: bool,
        pub regenerating: bool,
        pub meditating: bool,
        pub on_trade_mission: bool,
        pub actions: i32,
        pub collapse: bool,
        pub needs_human: bool,
    }

    #[derive(Default)]
    pub struct Rec {
        pub sends: RefCell<Vec<(Vec<u8>, Option<Vec<String>>, bool)>>,
        pub clients: RefCell<BTreeMap<String, Cl>>,
        pub cloud: Cell<bool>,
        pub privacy: Cell<u64>,
        pub credit: Cell<f32>,
        pub tips: RefCell<Vec<String>>,
        pub saved: RefCell<Vec<(Option<Vec<u8>>, Option<Vec<u8>>)>>,
        pub calls: RefCell<Vec<String>>,
        pub fired: RefCell<Vec<(Vector2, Vector2, i32)>>,
        pub blockheads: RefCell<Vec<ObjRef>>,
        pub bh: RefCell<BTreeMap<u64, Bh>>,
        pub best_start: Cell<(i32, i32)>,
        pub loader_seed: Cell<Option<u32>>,
        pub local_player_id: RefCell<Option<String>>,
        pub dw_load_results: RefCell<Vec<bool>>,
        pub sc_created: RefCell<Vec<(String, i32)>>,
        pub finish_light_block: Cell<bool>,
        pub privacy_none: Cell<bool>,
        pub pc_in_progress: Cell<bool>,
        pub pc_user: RefCell<Option<ObjRef>>,
    }

    impl Rec {
        pub fn note(&self, s: &str) {
            self.calls.borrow_mut().push(s.to_string());
        }
        pub fn count(&self, prefix: &str) -> usize {
            self.calls.borrow().iter().filter(|c| c.starts_with(prefix)).count()
        }
        fn bh_of(&self, b: &ObjRef) -> Bh {
            self.bh.borrow().get(&b.base().borrow().unique_id).cloned().unwrap_or_default()
        }
    }

    impl WorldCoreHooksStub for Rec {
        fn server_send_network_data_to_peers_reliable(&self, data: &[u8], peers: Option<&[String]>, reliable: bool) {
            self.sends.borrow_mut().push((data.to_vec(), peers.map(|p| p.to_vec()), reliable));
        }
        fn server_credit(&self) -> f32 {
            self.credit.get()
        }
        fn path_creator_in_progress(&self) -> bool {
            self.pc_in_progress.get()
        }
        fn path_creator_path_user(&self) -> Option<ObjRef> {
            self.pc_user.borrow().clone()
        }
        fn path_creator_abort_path(&self) {
            self.note("abort_path");
        }
        fn tip_manager_current_tip(&self) -> Option<String> {
            Some("a tip".to_string())
        }
        fn dynamic_world_is_controlling_blockheads_for_client_player(&self, player_id: &str) -> bool {
            player_id == "boss"
        }
        fn dynamic_world_players_changed(&self) {
            self.note("players_changed");
        }
        fn dynamic_world_user_mute_changed(&self, player_id: &str) {
            self.note(&format!("mute_changed {player_id}"));
        }
        fn loader_update_physical_block_to_latest_version(&self, _pb: PhysicalBlockRef) {
            self.note("update_pb");
        }
        fn server_is_cloud_match(&self) -> bool {
            self.cloud.get()
        }
        fn server_privacy_string(&self) -> Option<String> {
            if self.privacy_none.get() {
                None
            } else {
                Some("public".to_string())
            }
        }
        fn server_match_privacy(&self) -> u64 {
            self.privacy.get()
        }
        fn server_local_player_name(&self) -> String {
            "HOST".to_string()
        }
        fn server_local_player_id(&self) -> Option<String> {
            self.local_player_id.borrow().clone()
        }
        fn server_player_name_for_player_with_id_including_old_players(&self, id: Option<&str>) -> Option<String> {
            id.map(|i| format!("N-{}", i))
        }
        fn server_start_bulk_transaction(&self) {
            self.note("server_start_bulk");
        }
        fn server_finish_bulk_transaction(&self) -> bool {
            self.note("server_finish_bulk");
            true
        }
        fn server_client_create(&self, client_id: &str, light_block_index: i32, _db: Option<Rc<Database>>) {
            self.sc_created.borrow_mut().push((client_id.to_string(), light_block_index));
            self.clients
                .borrow_mut()
                .insert(client_id.to_string(), Cl { connected: true, lbi: light_block_index, ..Default::default() });
            self.note("sc_create");
        }
        fn server_client_connected(&self, id: &str) -> bool {
            self.clients.borrow().get(id).map_or(false, |c| c.connected)
        }
        fn server_client_set_connected(&self, id: &str, v: bool) {
            if let Some(c) = self.clients.borrow_mut().get_mut(id) {
                c.connected = v;
                if !v {
                    c.paused = false;
                }
            }
        }
        fn server_client_reconnected(&self, id: &str) {
            if let Some(c) = self.clients.borrow_mut().get_mut(id) {
                c.connected = true;
            }
            self.note("sc_reconnected");
        }
        fn server_client_light_block_index(&self, id: &str) -> i32 {
            self.clients.borrow().get(id).map_or(0, |c| c.lbi)
        }
        fn server_client_request_for_block(&self, id: &str, r: ClientMacroBlockRequest) {
            if let Some(c) = self.clients.borrow_mut().get_mut(id) {
                c.requested.push(r.macro_index);
            }
        }
        fn server_client_block_removed(&self, id: &str, i: i32) {
            if let Some(c) = self.clients.borrow_mut().get_mut(id) {
                c.removed.push(i);
            }
        }
        fn server_client_block_is_wired(&self, id: &str, i: i32) -> bool {
            self.clients.borrow().get(id).map_or(false, |c| c.wired.contains(&i))
        }
        fn server_client_paused(&self, id: &str) -> bool {
            self.clients.borrow().get(id).map_or(false, |c| c.paused)
        }
        fn server_client_set_paused(&self, id: &str, p: bool) {
            if let Some(c) = self.clients.borrow_mut().get_mut(id) {
                c.paused = p;
            }
        }
        fn loader_init(
            &self,
            _w: &Rc<World>,
            seed: u32,
            is_new: bool,
            _id: &str,
            _lv: i32,
            _db: Option<Rc<Database>>,
        ) -> Option<u32> {
            self.note(&format!("loader_init seed={} new={}", seed, is_new));
            self.loader_seed.get()
        }
        fn loader_refine_terrain(&self) {
            self.note("refine");
        }
        fn loader_best_start_position(&self) -> IntPair {
            let (x, y) = self.best_start.get();
            IntPair { x, y }
        }
        fn loader_highest_point(&self) -> IntPair {
            IntPair { x: 5, y: 6 }
        }
        fn loader_distance_ordered_food_types(&self) -> Vec<i32> {
            (0..31).collect()
        }
        fn loader_unarchive_light_blocks_for_client(&self, id: &str) {
            self.note(&format!("unarchive {}", id));
        }
        fn loader_finish_bulk_light_block_transaction(&self) -> bool {
            self.note("lb_finish");
            self.finish_light_block.get()
        }
        fn loader_start_bulk_light_block_transaction(&self) {
            self.note("lb_start");
        }
        fn weather_init(&self, _w: &Rc<World>, t: f32) {
            self.note(&format!("weather {}", t));
        }
        fn projectile_manager_fire(
            &self,
            from: Vector2,
            to: Vector2,
            _at: Option<&ObjRef>,
            item_type: i32,
            _firer: Option<&ObjRef>,
        ) {
            self.fired.borrow_mut().push((from, to, item_type));
        }
        fn dynamic_world_load_dynamic_objects_reposition_blockhead_load_failures(
            &self,
            did_load: bool,
            _r: bool,
        ) -> bool {
            self.note(&format!("dw_load did_load={}", did_load));
            let mut q = self.dw_load_results.borrow_mut();
            if q.is_empty() {
                true
            } else {
                q.remove(0)
            }
        }
        fn dynamic_world_save_game_with_world_data_sign_ownership_data(&self, w: Option<&[u8]>, s: Option<&[u8]>) {
            self.saved.borrow_mut().push((w.map(|x| x.to_vec()), s.map(|x| x.to_vec())));
        }
        fn dynamic_world_conversion_pump(&self) {
            self.note("pump");
        }
        fn dynamic_world_client_connected(&self, id: &str) {
            self.note(&format!("dw_client_connected {}", id));
        }
        fn dynamic_world_client_disconnected_simulate(&self, id: &str, sim: bool) {
            self.note(&format!("dw_client_disconnected {} {}", id, sim));
        }
        fn dynamic_world_stop_all_blockhead_actions_for_client_due_to_kick(&self, id: &str) {
            self.note(&format!("dw_stop_actions {}", id));
        }
        fn dynamic_world_all_blockheads_including_net(&self) -> Vec<ObjRef> {
            self.blockheads.borrow().clone()
        }
        fn dynamic_world_blockheads(&self) -> Vec<ObjRef> {
            self.blockheads.borrow().clone()
        }
        fn dynamic_world_remote_create(&self, o: Vec<Vec<u8>>, t: i32, c: Option<&str>) {
            self.note(&format!("dw_remote_create {} {} {:?}", o.len(), t, c));
        }
        fn dynamic_world_remote_update(&self, o: Vec<Vec<u8>>, t: i32, c: Option<&str>) {
            self.note(&format!("dw_remote_update {} {} {:?}", o.len(), t, c));
        }
        fn dynamic_world_remote_creation_data_update(&self, o: Vec<Vec<u8>>, t: i32, c: Option<&str>) {
            self.note(&format!("dw_remote_cdu {} {} {:?}", o.len(), t, c));
        }
        fn dynamic_world_remote_remove(&self, o: Vec<Vec<u8>>, t: i32, c: Option<&str>) {
            self.note(&format!("dw_remote_remove {} {} {:?}", o.len(), t, c));
        }
        fn dynamic_world_light_changed_at_macro_pos_send_reliably(&self, p: IntPair, r: bool) {
            self.note(&format!("light_changed {} {} {}", p.x, p.y, r));
        }
        fn dynamic_world_portal_level(&self) -> Option<i32> {
            Some(3)
        }
        fn dynamic_world_blockhead_previews(&self) -> Vec<PlistValue> {
            vec![PlistValue::data(vec![1u8, 2, 3])]
        }
        fn blockhead_name(&self, b: &ObjRef) -> Option<String> {
            self.bh_of(b).name
        }
        fn blockhead_client_id(&self, b: &ObjRef) -> Option<String> {
            self.bh_of(b).client_id
        }
        fn blockhead_is_client_blockhead_being_controlled_by_server(&self, b: &ObjRef) -> bool {
            self.bh_of(b).controlled_by_server
        }
        fn blockhead_should_continue_simulating(&self, b: &ObjRef) -> bool {
            self.bh_of(b).should_continue
        }
        fn blockhead_asleep(&self, b: &ObjRef) -> bool {
            self.bh_of(b).asleep
        }
        fn blockhead_regenerating(&self, b: &ObjRef) -> bool {
            self.bh_of(b).regenerating
        }
        fn blockhead_meditating(&self, b: &ObjRef) -> bool {
            self.bh_of(b).meditating
        }
        fn blockhead_on_trade_mission(&self, b: &ObjRef) -> bool {
            self.bh_of(b).on_trade_mission
        }
        fn blockhead_has_actions(&self, b: &ObjRef) -> bool {
            self.bh_of(b).actions > 0
        }
        fn blockhead_action_count(&self, b: &ObjRef) -> i32 {
            self.bh_of(b).actions
        }
        fn blockhead_cancel_simulate_due_to_collapse(&self, b: &ObjRef) -> bool {
            self.bh_of(b).collapse
        }
        fn blockhead_current_interaction_requires_human_input(&self, b: &ObjRef) -> bool {
            self.bh_of(b).needs_human
        }
        fn blockhead_suffer_damage_is_simulation_recoil(&self, b: &ObjRef, d: f32, sim: bool, recoil: bool) {
            if let Some(x) = self.bh.borrow_mut().get_mut(&b.base().borrow().unique_id) {
                x.damage_log.push((d, sim, recoil));
            }
        }
        fn tip_manager_display_tip(&self, t: &str) {
            self.tips.borrow_mut().push(t.to_string());
        }
        fn world_fully_load_if_needed_around_pos_client_light_block_index(&self, _w: &World, p: IntPair, idx: i32) {
            self.note(&format!("fully_load {} {} {}", p.x, p.y, idx));
        }
        fn world_place_workbench_of_type_at_pos(&self, _w: &World, t: i32, p: IntPair) {
            self.note(&format!("workbench {} {} {}", t, p.x, p.y));
        }
        fn world_helper_check_if_macro_tile_can_be_decommissioned(
            &self,
            _w: &World,
            mt: Option<&MacroTileRef>,
            age: f64,
            _b: Option<PhysicalBlockRef>,
        ) {
            self.note(&format!("decommission_check {} {}", mt.is_some(), age));
        }
    }

    pub struct TestDelegate {
        pub default_save: PlistDict,
    }
    impl crate::world::state::WorldDelegate for TestDelegate {
        fn app_database(&self) -> Option<Rc<Database>> {
            None
        }
    }
    impl WorldDelegate for TestDelegate {
        fn get_default_game_save_for_world_with_id(&self, _id: &str) -> PlistDict {
            self.default_save.clone()
        }
        fn file_write_failed(&self, p: &str) {
            nslog!("failed to write to file:{}", p);
        }
        fn app_database_environment(&self) -> Option<Rc<DatabaseEnvironment>> {
            None
        }
        fn show_welcome_back_popup_with_message(&self, _m: Option<&str>) {}
        fn set_paused(&self, _p: bool) {}
    }

    /// Run `f` while capturing NSLog output (global sink, serialised).
    pub fn capture_log(f: impl FnOnce()) -> String {
        let _g = crate::foundation::nslog::TEST_SINK_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let sink = Arc::new(Mutex::new(Vec::new()));
        crate::foundation::nslog::set_test_sink(Some(sink.clone()));
        f();
        crate::foundation::nslog::set_test_sink(None);
        let v = sink.lock().unwrap().clone();
        String::from_utf8_lossy(&v).into_owned()
    }

    /// Strip the `YYYY-MM-DD HH:MM:SS.mmm name[pid] ` prefix of every line.
    pub fn strip_log(s: &str) -> Vec<String> {
        s.lines()
            .map(|l| {
                l.splitn(3, ' ').nth(2).map(|r| r.splitn(2, "] ").nth(1).unwrap_or(r).to_string()).unwrap_or_default()
            })
            .collect()
    }

    pub struct Fixture {
        pub world: Rc<World>,
        pub rec: Rc<Rec>,
        pub delegate: Rc<TestDelegate>,
    }

    /// A server-mode world (id `sv`, name `TESTWORLD`, width 32) with recording hooks installed.
    pub fn fixture(width_macro: i32) -> Fixture {
        let rec = Rc::new(Rec::default());
        set_core_hooks(rec.clone());
        let delegate = Rc::new(TestDelegate { default_save: new_mutable_dict() });
        let del: Rc<dyn WorldDelegate> = delegate.clone();
        let mut host = new_mutable_dict();
        host.map.insert("hostPort".to_string(), PlistValue::string("15151"));
        host.map.insert("maxPlayers".to_string(), PlistValue::string("32"));
        let world = World::init(WorldInitArgs {
            window_info: WindowInfo {
                screen_width: 1024.0,
                screen_height: 768.0,
                half_screen_width: 512.0,
                half_screen_height: 384.0,
                padding_left: 0.0,
                padding_right: 0.0,
                padding_top: 0.0,
                padding_bottom: 0.0,
            },
            delegate: Rc::downgrade(&del),
            save_id: Some("sv".to_string()),
            name: Some("TESTWORLD".to_string()),
            server: Some(Rc::new(BHServer { api: None })),
            server_host_data: Some(host),
            save_delay: 10,
            world_width_macro: width_macro,
            custom_rules: None,
            expert_mode: false,
        })
        .expect("server world");
        Fixture { world, rec, delegate }
    }

    /// Install a `DynamicWorld` placeholder so the nil-receiver guards pass.
    pub fn with_dw(w: &World) {
        *w.dynamic_world.borrow_mut() = Some(Rc::new(DynamicWorld::new(Box::new(NullQueries))));
    }

    /// Mark `n` clients connected through `client_connected`.
    pub fn connect(f: &Fixture, id: &str) -> bool {
        f.world.client_connected(id)
    }
}

#[cfg(test)]
mod tests {
    use super::testkit::*;
    use super::*;
    use crate::foundation::gzip::gzip_inflate;
    use crate::plist::value::PlistValue;
    use crate::types::structs::WireStruct;

    fn sandbox_home(tag: &str) -> (std::sync::MutexGuard<'static, ()>, String) {
        let g = HOME_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let home = format!("{}/bhwc_{}_{}", std::env::temp_dir().display(), tag, std::process::id());
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(&home).unwrap();
        crate::foundation::paths::set_home_for_test(&home);
        (g, home)
    }

    #[test]
    fn init_logs_flags_and_draws_500_randoms() {
        glibc_rand::srand(1);
        let mut f = None;
        let log = capture_log(|| f = Some(fixture(32)));
        let f = f.unwrap();
        assert!(strip_log(&log).contains(&"loading world with size:32 expertMode:0".to_string()), "{}", log);
        let w = &f.world;
        assert!(w.is_owner() && w.is_admin() && w.is_mod());
        assert_eq!(w.save_delay(), 10);
        assert!(!w.pvp_enabled());
        assert_eq!(w.host_port.borrow().as_deref(), Some("15151"));
        assert_eq!(w.max_players.borrow().as_deref(), Some("32"));
        // first glibc rand() after srand(1) is 1804289383 (R2: divisor 2147483648.0f)
        assert_eq!(w.particle_random_numbers.borrow()[0], 1804289383i32 as f32 / 2147483648.0f32);
        assert_eq!(w.particle_random_numbers.borrow().len(), 500);
        let free = w.free_client_light_block_indices.borrow();
        assert_eq!(free.as_ref().unwrap().count(), 32);
        assert_eq!(free.as_ref().unwrap().last_index(), Some(31));
        assert!(w.server_clients.borrow().is_some());
        assert!(w.found_items_list.borrow().is_some());
    }

    #[test]
    fn save_delay_clamps_below_two() {
        let f = fixture(32);
        let w = World::init(WorldInitArgs {
            window_info: f.world.window_info().unwrap(),
            delegate: Rc::downgrade(&(f.delegate.clone() as Rc<dyn WorldDelegate>)),
            save_id: Some("x".into()),
            name: Some("y".into()),
            server: Some(Rc::new(BHServer { api: None })),
            server_host_data: None,
            save_delay: 1,
            world_width_macro: 32,
            custom_rules: None,
            expert_mode: true,
        })
        .unwrap();
        assert_eq!(w.save_delay(), 1);
        assert!(w.expert_mode());
    }

    #[test]
    fn new_world_loads_in_15_stages_with_exact_logs_and_state() {
        let (_g, _home) = sandbox_home("newworld");
        let f = fixture(32);
        f.rec.best_start.set((700, 100)); // 32*32 = 1024 wide; 700 > 512 (W/2) and in (256, 768): season.75
        f.rec.loader_seed.set(Some(0x9000_0000));
        let w = f.world.clone();
        let log = capture_log(|| {
            let mut n = 0;
            while !w.load_complete() && n < 40 {
                w.incremental_load();
                n += 1;
            }
            assert_eq!(n, 15, "stages: 0,1,2..9 (8 refines),10..14");
        });
        let lines = strip_log(&log);
        assert!(lines.contains(&"using seed:-1879048192".to_string()), "{:?}", lines);
        let pos = |s: &str| lines.iter().position(|l| l == s).unwrap_or_else(|| panic!("missing {}: {:?}", s, lines));
        assert!(pos("using seed:-1879048192") < pos("save delay:10"));
        assert!(pos("save delay:10") < pos("best start pos:700.0 100.0"));
        assert!(!w.did_load_game());
        assert_eq!(w.incremental_load_count(), 15);
        assert_eq!(w.random_seed(), 0x9000_0000);
        assert_eq!(w.start_portal_pos(), IntPair { x: 700, y: 100 });
        assert_eq!(w.highest_point(), IntPair { x: 5, y: 6 });
        // worldTime = 450 (x > W/2) + 2700 (season.75)
        assert_eq!(w.world_time(), 3150.0);
        assert!(w.pvp_enabled());
        assert!(w.has_finished_database_migration_to17());
        assert_eq!(w.distance_ordered_food_types.borrow().len(), 31);
        assert_eq!(f.rec.count("refine"), 8);
        assert_eq!(f.rec.count("loader_init"), 1);
        assert_eq!(f.rec.count("workbench 1 700 100"), 1);
        assert_eq!(f.rec.count("fully_load 700 100 -1"), 1);
        assert!(
            w.dynamic_world.borrow().is_some()
                && w.path_creator.borrow().is_some()
                && w.projectile_manager.borrow().is_some()
        );
        // further calls do nothing
        w.incremental_load();
        assert_eq!(w.incremental_load_count(), 15);
    }

    #[test]
    fn stage_12_retries_and_waits_sixty_calls() {
        let (_g, _home) = sandbox_home("stage12");
        let f = fixture(32);
        let w = f.world.clone();
        *f.rec.dw_load_results.borrow_mut() = vec![false, true];
        let _ = capture_log(|| {
            while w.incremental_load_count() < 12 {
                w.incremental_load();
            }
            w.incremental_load(); // fails: timer = 1, dw dropped
        });
        assert_eq!(w.incremental_load_count(), 12);
        assert_eq!(w.dynamic_world_failed_to_load_wait_count_timer(), 1);
        assert_eq!(w.dynamic_world_load_fail_count(), 1);
        assert!(w.dynamic_world.borrow().is_none());
        for _ in 0..58 {
            w.incremental_load();
        }
        assert_eq!(w.dynamic_world_failed_to_load_wait_count_timer(), 59);
        w.incremental_load(); // 59 -> 60 -> reset to 0
        assert_eq!(w.dynamic_world_failed_to_load_wait_count_timer(), 0);
        w.incremental_load(); // retry succeeds
        assert_eq!(w.incremental_load_count(), 13);
        assert!(w.dynamic_world.borrow().is_some());
    }

    #[test]
    fn stage_12_replays_queued_initial_objects_with_the_creation_data_quirk() {
        let (_g, _home) = sandbox_home("queue");
        let f = fixture(32);
        let w = f.world.clone();
        w.remote_create_for_objects_of_type_client_id(vec![vec![1], vec![2]], 7, "cA");
        w.remote_update_for_objects_of_type_from_client(vec![vec![3]], 8, None);
        w.remote_creation_data_update_for_objects_of_type_from_client(vec![vec![4]], 9, None);
        w.remote_remove_for_objects_of_type_from_client(vec![vec![5]], 10, None);
        let _ = capture_log(|| {
            while w.incremental_load_count() < 13 {
                w.incremental_load();
            }
        });
        let calls = f.rec.calls.borrow().clone();
        assert!(calls.contains(&"dw_remote_create 2 7 Some(\"cA\")".to_string()), "{:?}", calls);
        assert!(calls.contains(&"dw_remote_update 1 8 None".to_string()));
        assert!(calls.contains(&"dw_remote_remove 1 10 None".to_string()));
        // creation data update of type 9 has no `upd` entries for type 9, so the quirk passes the (empty) update list
        assert!(calls.contains(&"dw_remote_cdu 0 9 None".to_string()), "{:?}", calls);
        assert!(w.initial_create_objects.borrow().create.is_none());
        // after the dynamic world exists the relays forward directly
        w.remote_remove_for_objects_of_type_from_client(vec![vec![9]], 3, Some("c"));
        assert!(f.rec.calls.borrow().contains(&"dw_remote_remove 1 3 Some(\"c\")".to_string()));
    }

    #[test]
    fn save_all_writes_the_exact_key_set_and_types() {
        let (_g, _home) = sandbox_home("saveall");
        let f = fixture(32);
        let w = f.world.clone();
        f.rec.best_start.set((100, 50));
        with_dw(&w);
        w.set_random_seed(0xdeadbeef);
        w.set_has_finished_database_migration_to17(true);
        w.set_world_time(123.5);
        w.set_no_rain_timer(4.5);
        w.set_start_portal_pos(IntPair { x: 100, y: 50 });
        w.set_rounded_translation(Vector2 { x: 100.0, y: 50.0 });
        w.set_pvp_enabled(false);
        *w.welcome_message.borrow_mut() = Some("hi".into());
        *w.server_password.borrow_mut() = Some("pw".into());
        *w.distance_ordered_food_types.borrow_mut() = (0..31).collect();
        *w.circum_navigate_booleans.borrow_mut() = Some(IndexSet::new());
        let _cl = clock::TEST_CLOCK_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        clock::set_virtual(Some(1_000_000_000.0));
        w.save_all();
        clock::set_virtual(None);
        let saved = f.rec.saved.borrow();
        assert_eq!(saved.len(), 1);
        let (wd, sd) = &saved[0];
        assert!(sd.is_none());
        let d = property_list_from_data(wd.as_ref().unwrap()).unwrap();
        let PlistValue::Dict(m) = &d else { panic!() };
        let mut keys: Vec<String> = m.map.keys_in_order();
        keys.sort();
        let mut want: Vec<&str> = vec![
            "migrationComplete_1.7",
            "randomSeed",
            "translation",
            "worldTime",
            "noRainTimer",
            "expertMode",
            "startPortalPos.x",
            "startPortalPos.y",
            "saveID",
            "worldWidthMacro",
            "saveDate",
            "worldName",
            "hostPort",
            "pvpDisabled",
            "maxPlayers",
            "runAtLaunch",
            "creationDate",
            "saveVersion",
            "remoteGame",
            "serverPassword",
            "foundItems",
            "portalLevel",
            "blockheadDatasv2",
            "distanceOrderedFoodTypes",
            "circumNavigateBooleansData",
            "welcomeMessage",
        ];
        want.sort();
        assert_eq!(keys, want);
        let g = |k: &str| d.object_for_key(k).unwrap().clone();
        assert_eq!(g("randomSeed").unsigned_int_value(), 0xdeadbeef);
        assert!(matches!(g("worldTime"), PlistValue::Real { .. }));
        assert!(matches!(g("noRainTimer"), PlistValue::Real { .. }));
        assert!(matches!(g("saveDate"), PlistValue::Date(_)));
        assert_eq!(g("saveVersion").int_value(), 1100);
        assert_eq!(g("portalLevel").int_value(), 3);
        assert!(g("migrationComplete_1.7").bool_value());
        assert!(g("pvpDisabled").bool_value());
        assert!(g("runAtLaunch").bool_value());
        assert!(!g("remoteGame").bool_value());
        let PlistValue::Data(b) = g("distanceOrderedFoodTypes") else { panic!() };
        assert_eq!(b.bytes.len(), 124);
        let PlistValue::Array(a) = g("blockheadDatasv2") else { panic!() };
        assert_eq!(a.items.len(), 1);
        // creationDate is set at the first save and kept
        assert!(w.creation_date().is_some());
    }

    #[test]
    fn save_all_before_dynamic_world_writes_nothing_but_sets_creation_date_and_clears_sign_flag() {
        let f = fixture(32);
        let w = f.world.clone();
        w.set_ownership_sign_positions_need_saving(true);
        *w.ownership_sign_positions.borrow_mut() = Some(new_mutable_dict());
        w.save_all();
        assert!(f.rec.saved.borrow().is_empty());
        assert!(w.creation_date().is_some());
        assert!(!w.ownership_sign_positions_need_saving());
    }

    #[test]
    fn save_then_load_round_trip_restores_the_fields() {
        let (_g, home) = sandbox_home("roundtrip");
        let f = fixture(32);
        let w = f.world.clone();
        with_dw(&w);
        let _ = capture_log(|| {
            w.initialize_databases();
        });
        w.set_random_seed(12345);
        w.set_has_finished_database_migration_to17(true);
        w.set_world_time(77.25);
        w.set_no_rain_timer(1.5);
        w.set_start_portal_pos(IntPair { x: 9, y: 8 });
        w.set_rounded_translation(Vector2 { x: 9.0, y: 8.5 });
        w.set_portal_level(2);
        w.set_pvp_enabled(false);
        *w.welcome_message.borrow_mut() = Some("hello".into());
        *w.server_password.borrow_mut() = Some("pw".into());
        *w.distance_ordered_food_types.borrow_mut() = (100..131).collect();
        let mut circ = IndexSet::new();
        circ.add_index(4);
        circ.add_index(5);
        *w.circum_navigate_booleans.borrow_mut() = Some(circ);
        w.found_items_list.borrow_mut().as_mut().unwrap().add_indexes_in_range(3, 4);
        w.save_all();
        let saved = f.rec.saved.borrow()[0].0.clone().unwrap();
        // the DynamicWorld would store it in main/worldv2
        w.main_database.borrow().as_ref().unwrap().set_data(Some(&saved), Some("worldv2"));
        // a second world loads it
        let f2 = fixture(32);
        let w2 = f2.world.clone();
        let _ = capture_log(|| w2.initialize_databases());
        assert!(w2.load_game());
        assert_eq!(w2.random_seed(), 12345);
        assert_eq!(w2.world_time(), 77.25);
        assert_eq!(w2.no_rain_timer(), 1.5);
        assert_eq!(w2.start_portal_pos(), IntPair { x: 9, y: 8 });
        assert_eq!(w2.rounded_translation(), Vector2 { x: 9.0, y: 8.5 });
        assert_eq!(w2.accurate_translation(), Vector2 { x: 9.0, y: 8.5 });
        assert_eq!(w2.portal_level(), 3); // save_all takes the portal level from the DynamicWorld
        assert!(!w2.pvp_enabled());
        assert_eq!(w2.welcome_message.borrow().as_deref(), Some("hello"));
        assert_eq!(w2.server_password.borrow().as_deref(), Some("pw"));
        assert_eq!(w2.loaded_version(), 1100);
        assert!(w2.has_finished_database_migration_to17());
        assert_eq!(w2.distance_ordered_food_types.borrow().clone(), (100..131).collect::<Vec<i32>>());
        assert_eq!(w2.circum_navigate_booleans.borrow().as_ref().unwrap().iter().collect::<Vec<_>>(), vec![4, 5]);
        assert_eq!(w2.found_items_list.borrow().as_ref().unwrap().iter().collect::<Vec<_>>(), vec![3, 4, 5, 6]);
        assert!(w2.creation_date().is_some());
        assert!(w2.last_update_time() > 0.0);
        assert!(w2.did_load_game() == false); // set by incremental_load stage 0, not by load_game
        let _ = home;
    }

    #[test]
    fn load_game_without_random_seed_returns_false_and_takes_custom_rules_from_the_file() {
        let (_g, home) = sandbox_home("noseed");
        let f = fixture(32);
        let w = f.world.clone();
        // CommandLineDelegate's small worldv2 FILE: customRulesDict but no randomSeed
        let dir = format!("{}/GNUstep/Library/ApplicationSupport/TheBlockheads/saves/sv", home);
        std::fs::create_dir_all(&dir).unwrap();
        let mut cr = new_mutable_dict();
        cr.map.insert("seedType".to_string(), PlistValue::int(1));
        let mut top = new_mutable_dict();
        top.map.insert("customRulesDict".to_string(), PlistValue::Dict(cr));
        std::fs::write(format!("{}/worldv2", dir), data_from_property_list(&PlistValue::Dict(top)).unwrap()).unwrap();
        let _ = capture_log(|| w.initialize_databases());
        let ok = std::cell::Cell::new(true);
        let _ = capture_log(|| ok.set(w.load_game()));
        assert!(!ok.get());
        assert!(w.custom_rules_dict.borrow().is_some());
        w.load_default_game();
        assert!(w.pvp_enabled());
        assert!(w.has_finished_database_migration_to17());
        assert_eq!(w.rounded_translation(), Vector2 { x: 0.0, y: 640.0 });
    }

    #[test]
    fn non_dict_custom_rules_dict_follows_init_with_dictionary() {
        use crate::foundation::exception::catch_ns;
        let (_g, home) = sandbox_home("crarr");
        let dir = format!("{}/GNUstep/Library/ApplicationSupport/TheBlockheads/saves/sv", home);
        std::fs::create_dir_all(&dir).unwrap();
        let write = |v: PlistValue| {
            let mut top = new_mutable_dict();
            top.map.insert("customRulesDict".to_string(), v);
            std::fs::write(format!("{}/worldv2", dir), data_from_property_list(&PlistValue::Dict(top)).unwrap())
                .unwrap();
        };
        // non-empty array -> keyEnumerator unrecognised
        write(PlistValue::array(vec![PlistValue::int(1)]));
        let f = fixture(32);
        let w = f.world.clone();
        let _ = capture_log(|| w.initialize_databases());
        let e = catch_ns(|| w.load_game()).unwrap_err();
        assert_eq!(e.name, "NSInvalidArgumentException");
        assert_eq!(e.reason.as_deref(), Some("GSArray(instance) does not recognize keyEnumerator"));
        // empty array -> empty non-nil dict, custom rules enabled
        write(PlistValue::array(vec![]));
        let f = fixture(32);
        let w = f.world.clone();
        let _ = capture_log(|| w.initialize_databases());
        let _ = capture_log(|| {
            w.load_game();
        });
        assert_eq!(w.custom_rules_dict.borrow().as_ref().map(|d| d.map.len()), Some(0));
        assert_eq!(w.custom_rules().custom_rules_enabled, 1);
        // number -> `count` unrecognised
        write(PlistValue::int(5));
        let f = fixture(32);
        let w = f.world.clone();
        let _ = capture_log(|| w.initialize_databases());
        let e = catch_ns(|| w.load_game()).unwrap_err();
        assert!(e.reason.unwrap().starts_with("Can not determine type information for -["));
    }

    #[test]
    fn found_items_legacy_inference_and_food_type_clamp() {
        let (_g, _home) = sandbox_home("legacy");
        let f = fixture(32);
        let w = f.world.clone();
        let _ = capture_log(|| w.initialize_databases());
        let mut top = new_mutable_dict();
        top.map.insert("randomSeed".to_string(), PlistValue::int(5));
        top.map.insert("blockheadDatasv2".to_string(), PlistValue::array(vec![PlistValue::data(vec![1u8])]));
        top.map.insert("distanceOrderedFoodTypes".to_string(), PlistValue::data(vec![1u8; 130]));
        let bytes = data_from_property_list(&PlistValue::Dict(top)).unwrap();
        w.main_database.borrow().as_ref().unwrap().set_data(Some(&bytes), Some("worldv2"));
        assert!(w.load_game());
        let fi = w.found_items_list.borrow();
        let fi = fi.as_ref().unwrap();
        assert!(fi.contains_index(0) && fi.contains_index(0x12d) && !fi.contains_index(0x12e));
        assert!(fi.contains_index(0x400) && fi.contains_index(0x447) && !fi.contains_index(0x448));
        // 130 bytes: only 124 are used (R4)
        assert_eq!(w.distance_ordered_food_types.borrow().len(), 31);
        assert_eq!(w.distance_ordered_food_types.borrow()[0], 0x01010101);
        // missing keys read as 0 / NO
        assert_eq!(w.start_portal_pos(), IntPair { x: 0, y: 0 });
        assert!(w.pvp_enabled());
    }

    #[test]
    fn set_translation_wraps_and_rounds() {
        let f = fixture(32);
        let w = &f.world;
        w.set_translation(Vector2 { x: -5.0, y: 10.3 });
        assert_eq!(w.translation().x, 1019.0);
        let w2 = &f.world;
        w2.set_translation(Vector2 { x: 1030.0, y: 0.0 });
        assert_eq!(w2.translation().x, 6.0);
        let _ = gzip_inflate;
        let _ = WireStruct::to_vec as fn(&WindowInfo) -> Vec<u8>;
    }

    #[test]
    fn dealloc_logs_exiting_world() {
        let f = fixture(32);
        let log = capture_log(|| f.world.dealloc());
        assert!(strip_log(&log).contains(&"Exiting World.".to_string()));
    }
}

/// The inert defaults of the hook seam answer without side effects.
#[cfg(test)]
mod hook_default_tests {
    use super::*;
    use crate::world::helpers::testutil::new_obj;
    use crate::world::state::PhysicalBlock;

    #[test]
    fn every_inert_default_answers() {
        let h = NullCoreHooks;
        let w = World::alloc(32);
        let obj = new_obj();
        let pb: PhysicalBlockRef = Box::leak(Box::new(PhysicalBlock::new_fresh()));
        let mut log = String::new();
        let _ = WorldCoreHooksStub::server_send_network_data_to_peers_reliable(&h, &[], None, false);
        let _ = WorldCoreHooksStub::server_credit(&h);
        let _ = WorldCoreHooksStub::server_is_cloud_match(&h);
        let _ = WorldCoreHooksStub::server_privacy_string(&h);
        let _ = WorldCoreHooksStub::server_match_privacy(&h);
        let _ = WorldCoreHooksStub::server_database(&h);
        let _ = WorldCoreHooksStub::server_start_bulk_transaction(&h);
        let _ = WorldCoreHooksStub::server_finish_bulk_transaction(&h);
        let _ = WorldCoreHooksStub::server_local_player_name(&h);
        let _ = WorldCoreHooksStub::server_local_player_id(&h);
        let _ = WorldCoreHooksStub::server_player_name_for_player_with_id_including_old_players(&h, None);
        let _ = WorldCoreHooksStub::server_client_create(&h, "x", 0, None);
        let _ = WorldCoreHooksStub::server_client_connected(&h, "x");
        let _ = WorldCoreHooksStub::server_client_set_connected(&h, "x", false);
        let _ = WorldCoreHooksStub::server_client_reconnected(&h, "x");
        let _ = WorldCoreHooksStub::server_client_light_block_index(&h, "x");
        let _ = WorldCoreHooksStub::server_client_request_for_block(
            &h,
            "x",
            ClientMacroBlockRequest { macro_index: 0, create_if_not_created: 0, padding: [0; 3] },
        );
        let _ = WorldCoreHooksStub::server_client_block_removed(&h, "x", 0);
        let _ = WorldCoreHooksStub::server_client_block_is_wired(&h, "x", 0);
        let _ = WorldCoreHooksStub::server_client_paused(&h, "x");
        let _ = WorldCoreHooksStub::server_client_set_paused(&h, "x", false);
        let _ = WorldCoreHooksStub::loader_init(&h, &w, 0, false, "x", 0, None);
        let _ = WorldCoreHooksStub::loader_refine_terrain(&h);
        let _ = WorldCoreHooksStub::loader_highest_point(&h);
        let _ = WorldCoreHooksStub::loader_best_start_position(&h);
        let _ = WorldCoreHooksStub::loader_distance_ordered_food_types(&h);
        let _ = WorldCoreHooksStub::loader_x_frequency_multiplier(&h);
        let _ = WorldCoreHooksStub::loader_light_block_database(&h);
        let _ = WorldCoreHooksStub::loader_start_bulk_light_block_transaction(&h);
        let _ = WorldCoreHooksStub::loader_finish_bulk_light_block_transaction(&h);
        let _ = WorldCoreHooksStub::loader_unarchive_light_blocks_for_client(&h, "x");
        let _ = WorldCoreHooksStub::loader_set_needs_to_exit(&h, false);
        let _ = WorldCoreHooksStub::loader_update_physical_block_to_latest_version(&h, pb);
        let _ = WorldCoreHooksStub::weather_noise_function_init(&h, 0, 0, 0, 0.0, 0, false, false, 0.0);
        let _ = WorldCoreHooksStub::weather_init(&h, &w, 0.0);
        let _ = WorldCoreHooksStub::path_creator_init(&h, &w);
        let _ = WorldCoreHooksStub::path_creator_in_progress(&h);
        let _ = WorldCoreHooksStub::path_creator_path_user(&h);
        let _ = WorldCoreHooksStub::path_creator_abort_path(&h);
        let _ = WorldCoreHooksStub::projectile_manager_init(&h, &w);
        let _ = WorldCoreHooksStub::projectile_manager_fire(
            &h,
            Vector2 { x: 0.0, y: 0.0 },
            Vector2 { x: 0.0, y: 0.0 },
            None,
            0,
            None,
        );
        let _ = WorldCoreHooksStub::dynamic_world_create(&h, &w);
        let _ =
            WorldCoreHooksStub::dynamic_world_load_dynamic_objects_reposition_blockhead_load_failures(&h, false, false);
        let _ = WorldCoreHooksStub::dynamic_world_set_server_server_clients(&h, false);
        let _ = WorldCoreHooksStub::dynamic_world_conversion_pump(&h);
        let _ = WorldCoreHooksStub::dynamic_world_send_lightblocks_to_clients(&h);
        let _ = WorldCoreHooksStub::dynamic_world_save_and_send_only_blocks_that_need_to_be_sent(&h);
        let _ = WorldCoreHooksStub::dynamic_world_save_game_with_world_data_sign_ownership_data(&h, None, None);
        let _ = WorldCoreHooksStub::dynamic_world_portal_level(&h);
        let _ = WorldCoreHooksStub::dynamic_world_blockhead_previews(&h);
        let _ = WorldCoreHooksStub::dynamic_world_blockheads(&h);
        let _ = WorldCoreHooksStub::dynamic_world_all_blockheads_including_net(&h);
        let _ = WorldCoreHooksStub::dynamic_world_client_connected(&h, "x");
        let _ = WorldCoreHooksStub::dynamic_world_client_disconnected_simulate(&h, "x", false);
        let _ = WorldCoreHooksStub::dynamic_world_stop_all_blockhead_actions_for_client_due_to_kick(&h, "x");
        let _ = WorldCoreHooksStub::dynamic_world_is_controlling_blockheads_for_client_player(&h, "x");
        let _ = WorldCoreHooksStub::dynamic_world_light_changed_at_macro_pos_send_reliably(
            &h,
            IntPair { x: 0, y: 0 },
            false,
        );
        let _ = WorldCoreHooksStub::dynamic_world_remote_create(&h, Vec::new(), 0, None);
        let _ = WorldCoreHooksStub::dynamic_world_remote_update(&h, Vec::new(), 0, None);
        let _ = WorldCoreHooksStub::dynamic_world_remote_creation_data_update(&h, Vec::new(), 0, None);
        let _ = WorldCoreHooksStub::dynamic_world_remote_remove(&h, Vec::new(), 0, None);
        let _ = WorldCoreHooksStub::dynamic_world_user_mute_changed(&h, "x");
        let _ = WorldCoreHooksStub::dynamic_world_players_changed(&h);
        let _ = WorldCoreHooksStub::dynamic_world_append_debug_log(&h, &mut log);
        let _ = WorldCoreHooksStub::dynamic_world_finish_simulating(&h);
        let _ = WorldCoreHooksStub::dynamic_world_simulate(&h, 0.0);
        let _ = WorldCoreHooksStub::dynamic_world_set_paused(&h, false);
        let _ = WorldCoreHooksStub::blockhead_name(&h, &obj);
        let _ = WorldCoreHooksStub::blockhead_client_id(&h, &obj);
        let _ = WorldCoreHooksStub::blockhead_is_client_blockhead_being_controlled_by_server(&h, &obj);
        let _ = WorldCoreHooksStub::blockhead_should_continue_simulating(&h, &obj);
        let _ = WorldCoreHooksStub::blockhead_suffer_damage_is_simulation_recoil(&h, &obj, 0.0, false, false);
        let _ = WorldCoreHooksStub::blockhead_die_for_good(&h, &obj);
        let _ = WorldCoreHooksStub::blockhead_asleep(&h, &obj);
        let _ = WorldCoreHooksStub::blockhead_regenerating(&h, &obj);
        let _ = WorldCoreHooksStub::blockhead_meditating(&h, &obj);
        let _ = WorldCoreHooksStub::blockhead_on_trade_mission(&h, &obj);
        let _ = WorldCoreHooksStub::blockhead_has_actions(&h, &obj);
        let _ = WorldCoreHooksStub::blockhead_action_count(&h, &obj);
        let _ = WorldCoreHooksStub::blockhead_cancel_simulate_due_to_collapse(&h, &obj);
        let _ = WorldCoreHooksStub::blockhead_current_interaction_requires_human_input(&h, &obj);
        let _ = WorldCoreHooksStub::world_helper_check_if_macro_tile_can_be_decommissioned(&h, &w, None, 0.0, None);
        let _ = WorldCoreHooksStub::world_fully_load_if_needed_around_pos_client_light_block_index(
            &h,
            &w,
            IntPair { x: 0, y: 0 },
            0,
        );
        let _ = WorldCoreHooksStub::world_place_workbench_of_type_at_pos(&h, &w, 0, IntPair { x: 0, y: 0 });
        let _ =
            WorldCoreHooksStub::world_load_light_block_for_client_light_block_index_into_physical_block(&h, &w, 0, pb);
        let _ = WorldCoreHooksStub::world_welcome_back_message_for_simulation_events(&h, &w);
        let _ = WorldCoreHooksStub::world_pause_button_tapped(&h, &w);
        let _ = WorldCoreHooksStub::world_path_request_pump(&h, &w);
        let _ = WorldCoreHooksStub::particle_emitter_set_world_width_macro(&h, 0);
        let _ = WorldCoreHooksStub::particle_emitter_do_add_electricity_particle_with_path_size(&h, &[], 0.0);
        let _ = WorldCoreHooksStub::tip_manager_set_world_and_reset(&h, &w);
        let _ = WorldCoreHooksStub::tip_manager_update(&h, 0.0);
        let _ = WorldCoreHooksStub::tip_manager_display_tip(&h, "x");
        let _ = WorldCoreHooksStub::tip_manager_current_tip(&h);
        assert!(log.is_empty());
        let q = NullQueries;
        let _ = (
            q.door_can_be_used_by_path_user(None, IntPair { x: 0, y: 0 }),
            q.elevator_motor_for_shaft_at_pos(IntPair { x: 0, y: 0 }),
        );
        let _ = (q.interaction_object_at_pos(IntPair { x: 0, y: 0 }), q.ladder_at_pos(IntPair { x: 0, y: 0 }));
        let _ = (q.column_at_pos(IntPair { x: 0, y: 0 }), q.stairs_at_pos(IntPair { x: 0, y: 0 }));
        let _ = (q.elevator_shaft_at_pos(IntPair { x: 0, y: 0 }), q.painting_at_pos(IntPair { x: 0, y: 0 }));
    }
}

/// Odd inputs of the lifecycle: construction variants, server switching, camera wrap, helper functions, save
/// files in all their shapes and the less travelled stages.
#[cfg(test)]
mod lifecycle_scenarios {
    use super::testkit::*;
    use super::*;

    fn sandbox(tag: &str) -> (std::sync::MutexGuard<'static, ()>, String) {
        let g = HOME_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let home = format!("{}/bhls_{}_{}", std::env::temp_dir().display(), tag, std::process::id());
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(&home).unwrap();
        crate::foundation::paths::set_home_for_test(&home);
        (g, format!("{}/GNUstep/Library/ApplicationSupport/TheBlockheads/saves/sv", home))
    }

    fn encode(entries: Vec<(&str, PlistValue)>) -> Vec<u8> {
        let mut d = new_mutable_dict();
        for (k, v) in entries {
            d.map.insert(k.to_string(), v);
        }
        data_from_property_list(&PlistValue::Dict(d)).unwrap()
    }

    #[test]
    fn construction_and_server_switching() {
        let f = fixture(32);
        let del: Rc<dyn WorldDelegate> = f.delegate.clone();
        let mk = |save_id: Option<&str>, server: bool, rules: Option<PlistDict>| {
            World::init(WorldInitArgs {
                window_info: f.world.window_info().unwrap(),
                delegate: Rc::downgrade(&del),
                save_id: save_id.map(|s| s.to_string()),
                name: Some("n".to_string()),
                server: if server { Some(Rc::new(BHServer { api: None })) } else { None },
                server_host_data: None,
                save_delay: 5,
                world_width_macro: 32,
                custom_rules: rules,
                expert_mode: false,
            })
        };
        let mut rules = new_mutable_dict();
        rules.map.insert("seedType".to_string(), PlistValue::int(1));
        let plain = mk(Some("b"), false, None).unwrap();
        assert!(plain.server_clients.borrow().is_none());
        let w = mk(Some("a"), true, Some(rules)).unwrap();
        assert!(w.custom_rules_dict.borrow().is_some());
        let log = capture_log(|| assert!(mk(None, false, None).is_none()));
        assert!(log.contains("Attempting to load invalid world"));
        // switching servers
        let w = f.world.clone();
        let same = w.server.borrow().clone();
        w.set_server(same);
        assert!(w.server.borrow().is_some());
        with_dw(&w);
        w.set_server(Some(Rc::new(BHServer { api: None })));
        assert!(w.server_clients.borrow().is_some());
        w.set_server(None);
        assert!(w.server_clients.borrow().is_none());
        w.set_server(None);
        *w.dynamic_world.borrow_mut() = None;
        w.set_server(Some(Rc::new(BHServer { api: None })));
        // trivial entry points
        w.delete_timers();
        assert!(!w.tutorial_active());
        let obj = crate::world::helpers::testutil::new_obj();
        w.die_confirmation_confirmed(&obj);
        let mut log = String::new();
        w.append_debug_log(&mut log);
        with_dw(&w);
        w.append_debug_log(&mut log);
        assert!(log.contains("loaded macro blocks:"));
        let l = capture_log(|| w.file_write_failed("/x/y"));
        assert!(l.contains("failed to write to file:/x/y"));
        World::set_core_delegate_for_tests(None);
        w.file_write_failed("/x/y");
        // the loader is told to exit
        *w.world_tile_loader.borrow_mut() = Some(Rc::new(WorldTileLoader {}));
        let _ = capture_log(|| w.dealloc());
        // server clients
        w.send_to_peers(&[1, 2], None, true);
        assert!(!w.has_server_client("nobody"));
        assert!(!w.server_client_is_connected("nobody"));
        assert!(w.server_client_keys().is_empty());
        w.set_server(None);
        w.send_to_peers(&[1], None, true);
        assert!(w.server_client_keys().is_empty());
        assert!(!w.has_server_client("x"));
        // world info for iCloud with and without a dynamic world
        *w.dynamic_world.borrow_mut() = None;
        let info = w.world_info_for_i_cloud_save();
        assert!(info.map.contains_key(&"portalLevel".to_string()));
        with_dw(&w);
        let _ = w.world_info_for_i_cloud_save();
    }

    #[test]
    fn camera_wraps_in_both_directions_with_goals() {
        let f = fixture(32);
        let w = &f.world;
        w.set_translation(Vector2 { x: 5.0, y: 1.0 });
        assert_eq!(w.translation().x, 5.0);
        w.set_translation_goal(Vector2 { x: -10.0, y: 0.0 });
        w.set_translation(Vector2 { x: -5.0, y: 1.0 });
        assert_eq!(w.translation_goal().x, 1014.0);
        w.set_translation_goal(Vector2 { x: 1100.0, y: 0.0 });
        w.set_translation(Vector2 { x: 1030.0, y: 1.0 });
        assert_eq!(w.translation_goal().x, 76.0);
        w.set_translation_goal(Vector2 { x: 5.0, y: 0.0 });
        w.set_translation(Vector2 { x: 1030.0, y: 1.0 });
        assert_eq!(w.translation_goal().x, 5.0);
    }

    #[test]
    fn helper_functions() {
        let arr = PlistValue::array(vec![PlistValue::int(1)]);
        let PlistValue::Array(a) = mutable_copy_of_value(&arr) else { panic!() };
        assert!(a.tag.mutable);
        let mut d = new_mutable_dict();
        d.map.insert("k".to_string(), PlistValue::int(1));
        let PlistValue::Dict(c) = mutable_copy_of_value(&PlistValue::Dict(d)) else { panic!() };
        assert_eq!(c.map.len(), 1);
        assert_eq!(mutable_copy_of_value(&PlistValue::int(3)).int_value(), 3);
        let dir = format!("{}/bhls_atomic_{}", std::env::temp_dir().display(), std::process::id());
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        assert!(write_file_atomically(&format!("{dir}/f"), b"abc"));
        assert_eq!(std::fs::read(format!("{dir}/f")).unwrap(), b"abc");
        // no directory: the temp file cannot be written
        assert!(!write_file_atomically(&format!("{dir}/missing/f"), b"abc"));
        // a directory in the way: the rename fails
        std::fs::create_dir_all(format!("{dir}/d")).unwrap();
        assert!(!write_file_atomically(&format!("{dir}/d"), b"abc"));
        assert!(save_dir_for("abc").ends_with("/saves/abc"));
        let o = crate::world::helpers::testutil::new_obj();
        let p = crate::world::helpers::testutil::new_obj();
        assert!(same_object(&o, &o.clone()) && !same_object(&o, &p));
    }

    #[test]
    fn databases_that_cannot_be_opened_end_the_process() {
        let (_g, _dir) = sandbox("nodb");
        // a file where the saves directory should be
        let appsup = crate::foundation::paths::appsup();
        std::fs::create_dir_all(&appsup).unwrap();
        std::fs::write(format!("{appsup}/saves"), b"x").unwrap();
        let f = fixture(32);
        crate::foundation::sys::set_hosted(true);
        let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _ = capture_log(|| f.world.initialize_databases());
        }));
        crate::foundation::sys::set_hosted(false);
        assert!(r.is_err());
        // a world without an id cannot start loading
        *f.world.save_id.borrow_mut() = None;
        crate::foundation::sys::set_hosted(true);
        let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| f.world.incremental_load()));
        crate::foundation::sys::set_hosted(false);
        assert!(r.is_err());
    }

    #[test]
    fn legacy_world_files_and_odd_save_contents() {
        let (_g, dir) = sandbox("legacyfile");
        std::fs::create_dir_all(&dir).unwrap();
        let f = fixture(32);
        let w = f.world.clone();
        let _ = capture_log(|| w.initialize_databases());
        let xml = |seed: i64| {
            let mut d = new_mutable_dict();
            d.map.insert("randomSeed".to_string(), PlistValue::int(seed));
            String::from_utf8(crate::plist::xml_write::write_xml(&PlistValue::Dict(d))).unwrap()
        };
        // binary plist, garbage and a non-dictionary root are refused with the NSLog warning
        for bad in
            [b"bplist00junk".to_vec(), b"garbage {{{".to_vec(), crate::plist::xml_write::write_xml(&PlistValue::int(1))]
        {
            std::fs::write(format!("{dir}/world"), &bad).unwrap();
            let log = capture_log(|| assert!(!w.load_game()));
            assert!(log.contains("does not contain a dictionary"), "{log}");
        }
        // a good XML legacy file
        std::fs::write(format!("{dir}/world"), xml(77)).unwrap();
        assert!(w.load_game());
        assert_eq!(w.random_seed(), 77);
        // new style file wins
        std::fs::write(format!("{dir}/worldv2"), encode(vec![("randomSeed", PlistValue::int(88))])).unwrap();
        assert!(w.load_game());
        assert_eq!(w.random_seed(), 88);
    }

    #[test]
    fn save_file_variants() {
        let (_g, dir) = sandbox("variants");
        std::fs::create_dir_all(&dir).unwrap();
        let f = fixture(32);
        let w = f.world.clone();
        let _ = capture_log(|| w.initialize_databases());
        let main = w.main_database.borrow().clone().unwrap();
        let blockheads = || PlistValue::array(vec![PlistValue::data(vec![1u8])]);
        // blockheads of a named player, new format
        main.set_data(
            Some(&encode(vec![
                ("randomSeed", PlistValue::int(1)),
                ("playerID", PlistValue::string("p1")),
                ("p1_blockheadDatasv2", blockheads()),
                ("circumNavigateBooleansData", PlistValue::int(3)),
                ("distanceOrderedFoodTypes", PlistValue::data(vec![0u8; 124])),
            ])),
            Some("worldv2"),
        );
        f.rec.cloud.set(true);
        let log = capture_log(|| assert!(w.load_game()));
        assert!(log.contains("CLOUD:SETTING:PASSWORD:OFF") && log.contains("CLOUD:SETTING:PVP:ON"));
        assert!(w.found_items_list.borrow().as_ref().unwrap().contains_index(0));
        // food types too short to use; found items archived (empty and not), no found items list
        let mut set = IndexSet::new();
        main.set_data(
            Some(&encode(vec![
                ("randomSeed", PlistValue::int(1)),
                ("foundItems", PlistValue::data(archive_index_set(&set, "foundItems"))),
                ("distanceOrderedFoodTypes", PlistValue::data(vec![0u8; 8])),
                ("signOwnershipData", PlistValue::int(1)),
            ])),
            Some("worldv2"),
        );
        assert!(w.load_game());
        set.add_index(5);
        main.set_data(
            Some(&encode(vec![
                ("randomSeed", PlistValue::int(1)),
                ("foundItems", PlistValue::data(archive_index_set(&set, "foundItems"))),
            ])),
            Some("worldv2"),
        );
        assert!(w.load_game());
        assert!(w.found_items_list.borrow().as_ref().unwrap().contains_index(5));
        let list = w.found_items_list.borrow_mut().take();
        assert!(w.load_game());
        *w.found_items_list.borrow_mut() = list;
        // a world that already has its rules keeps them
        main.set_data(
            Some(&encode(vec![
                ("randomSeed", PlistValue::int(1)),
                ("customRulesDict", PlistValue::Dict(new_mutable_dict())),
            ])),
            Some("worldv2"),
        );
        *w.custom_rules_dict.borrow_mut() = Some(new_mutable_dict());
        assert!(w.load_game());
        *w.custom_rules_dict.borrow_mut() = None;
        // a password and pvp off in the cloud
        main.set_data(
            Some(&encode(vec![
                ("randomSeed", PlistValue::int(1)),
                ("serverPassword", PlistValue::string("pw")),
                ("pvpDisabled", PlistValue::boolean(true)),
                ("playerID", PlistValue::string("p2")),
                ("p2_blockheadDatas", blockheads()),
            ])),
            Some("worldv2"),
        );
        let log = capture_log(|| assert!(w.load_game()));
        assert!(log.contains("CLOUD:SETTING:PASSWORD:ON") && log.contains("CLOUD:SETTING:PVP:OFF"), "{log}");
        assert!(w.found_items_list.borrow().as_ref().unwrap().contains_index(0x12d));
        f.rec.cloud.set(false);
        // the oldest blockhead list, nothing for the player
        main.set_data(
            Some(&encode(vec![
                ("randomSeed", PlistValue::int(1)),
                ("playerID", PlistValue::string("p3")),
                ("blockheadDatas", blockheads()),
            ])),
            Some("worldv2"),
        );
        assert!(w.load_game());
        // circumnavigation from the legacy file
        main.set_data(Some(&encode(vec![("randomSeed", PlistValue::int(1))])), Some("worldv2"));
        std::fs::write(format!("{dir}/circumNavigateBooleans"), b"not an archive").unwrap();
        assert!(w.load_game());
        // sign ownership: legacy file only, then the database with the legacy file removed
        let mut signs = new_mutable_dict();
        signs.map.insert("1,2".to_string(), PlistValue::array(vec![PlistValue::int(1)]));
        let mut inner = new_mutable_dict();
        inner.map.insert("a".to_string(), PlistValue::int(1));
        signs.map.insert("3,4".to_string(), PlistValue::Dict(inner));
        let sign_bytes = data_from_property_list(&PlistValue::Dict(signs)).unwrap();
        std::fs::write(format!("{dir}/signOwnershipData"), &sign_bytes).unwrap();
        assert!(w.load_game());
        assert_eq!(w.ownership_sign_positions.borrow().as_ref().unwrap().map.len(), 2);
        main.set_data(Some(&sign_bytes), Some("signOwnershipData"));
        assert!(w.load_game());
        assert!(!std::path::Path::new(&format!("{dir}/signOwnershipData")).exists());
        main.set_data(Some(&data_from_property_list(&PlistValue::int(1)).unwrap()), Some("signOwnershipData"));
        assert!(w.load_game());
        // an empty dictionary changes nothing
        main.set_data(
            Some(&data_from_property_list(&PlistValue::Dict(new_mutable_dict())).unwrap()),
            Some("signOwnershipData"),
        );
        assert!(w.load_game());
        // a directory at the legacy place is removed as well
        std::fs::create_dir_all(format!("{dir}/signOwnershipData/sub")).unwrap();
        assert!(w.load_game());
    }

    #[test]
    fn a_new_world_clears_and_marks_the_start_portal() {
        let (_g, _dir) = sandbox("portal");
        let f = fixture(32);
        f.rec.best_start.set((700, 100));
        let w = f.world.clone();
        let _ = capture_log(|| {
            while w.incremental_load_count() < 13 {
                w.incremental_load();
            }
        });
        for i in [3 * 32 + 21, 3 * 32 + 22] {
            let pb: PhysicalBlockRef = Box::leak(Box::new(crate::world::state::PhysicalBlock::new_fresh()));
            pb.valid.set(true);
            *w.macro_tiles().get(i).unwrap().physical_block.borrow_mut() = Some(pb);
        }
        w.incremental_load();
        let t = tile_at_world_position_loaded(700, 99, &w).unwrap();
        assert_eq!(t.type_index(), 0x19);
        assert_eq!(tile_at_world_position_loaded(700, 100, &w).unwrap().type_index(), 2);
        // the old database format without a tile loader
        *w.world_tile_loader.borrow_mut() = None;
        w.set_has_finished_database_migration_to17(false);
        w.incremental_load();
        assert!(w.load_complete());
        // saving in the cloud without a privacy string
        f.rec.cloud.set(true);
        f.rec.privacy_none.set(true);
        w.save_all();
        // the helpers of the test kit
        core_hooks().server_start_bulk_transaction();
        assert!(core_hooks().server_finish_bulk_transaction());
        core_hooks().server_client_set_connected("nobody", true);
        let del: &dyn crate::world::state::WorldDelegate = &*f.delegate;
        assert!(del.app_database().is_none());
        f.delegate.set_paused(true);
    }

    #[test]
    fn default_game_with_a_custom_seed_in_the_cloud() {
        let f = fixture(32);
        let mut cr = new_mutable_dict();
        cr.map.insert("seed".to_string(), PlistValue::string("custom"));
        cr.map.insert("custom_seed_value".to_string(), PlistValue::string("1234"));
        *f.world.custom_rules_dict.borrow_mut() = Some(cr);
        let rules = custom_rules_from_dict(f.world.custom_rules_dict.borrow().as_ref(), false);
        f.world.set_custom_rules(rules);
        f.rec.cloud.set(true);
        let log = capture_log(|| f.world.load_default_game());
        assert_eq!(f.world.random_seed(), f.world.custom_rules().custom_seed_value);
        assert_eq!(f.world.custom_rules().seed_type, 1);
        assert!(log.contains("CLOUD:SETTING:PVP:ON"));
    }

    #[test]
    fn loading_an_existing_game_skips_the_refine_stages_and_converts_old_databases() {
        let (_g, dir) = sandbox("existing");
        std::fs::create_dir_all(&dir).unwrap();
        let f = fixture(32);
        let w = f.world.clone();
        let _ = capture_log(|| w.initialize_databases());
        w.main_database
            .borrow()
            .as_ref()
            .unwrap()
            .set_data(Some(&encode(vec![("randomSeed", PlistValue::int(9))])), Some("worldv2"));
        *w.net_events_message_to_display_once_loaded.borrow_mut() = Some("welcome back".to_string());
        let _ = capture_log(|| {
            let mut n = 0;
            while !w.load_complete() && n < 60 {
                w.incremental_load();
                n += 1;
            }
        });
        assert!(w.did_load_game() && w.load_complete());
        // the old database format asks for the convertor
        assert!(w.database_convertor.borrow().is_some() || w.has_finished_database_migration_to17());
        // saving: no delegate, a client password, the cloud privacy string, mods
        World::set_core_delegate_for_tests(None);
        *w.client_password.borrow_mut() = Some("cp".to_string());
        f.rec.cloud.set(true);
        w.save_all();
        *w.custom_rules_dict.borrow_mut() = Some(new_mutable_dict());
        let spy = Rc::new(crate::dynworld::state::testutil::ModSpy::default());
        crate::mods::install(Some(spy));
        w.save_all();
        crate::mods::install(None);
    }
}
