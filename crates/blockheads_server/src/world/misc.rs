//! `world_ops` misc: the dependency seam (`WorldOpsDeps`), `ub_null_deref`, welcome-back text, pause helpers,
//! `cancelAllActionsAtPos`, `queueBlockheadAIActionToTile`, `updateIdleTimerDisabled`.
//!
//! ;, `queueBlockheadAIActionToTile`, `pauseUpdates`,
//! `resetPauseIdleTimer`, `updateIdleTimerDisabled`; Protection (`displayOwnershipAreas`,
//! `renderingTeaserFrames`).
//!
//! ## The dependency seam
//! The collaborators of `World` that live in later layers (DynamicWorld, WorldTileLoader, ServerClient, BHServer,
//! Blockhead, Workbench, CrystalManager, ParticleEmitter,...) are still empty skeletons or placeholder structs in
//! `world::state` when this module is built. Every call into them goes through the object-safe trait
//! [`WorldOpsDeps`], whose methods are named with the  rule from the ObjC selectors listed in
//! All methods default to an inert value; the integrator implements the trait once
//! (a thin adapter) and installs it with [`install_deps`]. The unit tests install a recording stub.

use std::cell::RefCell;
use std::rc::Rc;

use super::state::{MacroTileRef, PhysicalBlockRef, TileRef, World};
use crate::foundation::gsdict::GsDict;
use crate::foundation::nsstring::{capitalized_string, lowercase_string};
use crate::items::inventory_item::{InventoryItem, SubSlot};
use crate::items::names::title_for_item_type_and_count_including_one;
use crate::items::predicates::interaction_type_disallowed_on_protected_tiles;
use crate::objects::handle::ObjRef;
use crate::plist::value::{PlistDict, PlistValue};
use crate::types::enums::{InteractionType, NPCType, WorkbenchType};
use crate::types::item_type::ItemType;
use crate::types::structs::{CraftableItem, IntPair, WireStruct};

// =====================================================================================================
// UB parity
// =====================================================================================================

/// UB-PARITY: the original dereferences a NULL `Tile*` here and dies with
/// SIGSEGV (exit 139). Under feature `faithful-crashes` (default) the process raises SIGSEGV without flushing
/// anything; otherwise this panics. `site` names the ObjC method for the panic text.
// UB-PARITY
pub(crate) fn ub_null_deref(site: &'static str) -> ! {
    #[cfg(feature = "faithful-crashes")]
    crate::foundation::sys::faithful_segv();
    panic!("UB-PARITY: NULL tile dereference in -[World {site}] (the original dies with SIGSEGV)");
}

// =====================================================================================================
// The dependency seam
// =====================================================================================================

crate::hooks::hook_trait! {
/// Calls `world_ops` makes into other modules. Every method has an inert default so that
/// a partially wired server still links; the integrator overrides all of them.
///
/// Peers (`ServerClient`) are addressed by their peer id string; the adapter looks the object up in
/// `World.server_clients`. Blockhead / Workbench / interaction object calls take the `ObjRef`.
#[allow(unused_variables, clippy::too_many_arguments)]
pub trait WorldOpsDeps, stub WorldOpsDepsStub {
    // ------------------------------------------------------------------ DynamicWorld
    /// `-[DynamicWorld pathUsers]`.
    fn path_users(&self) -> Vec<ObjRef> {
        Vec::new()
    }
    /// `-[DynamicWorld workbenchAtPos:]`.
    fn workbench_at_pos(&self, pos: IntPair) -> Option<ObjRef> {
        None
    }
    /// `-[DynamicWorld activeBlockhead]`.
    fn active_blockhead(&self) -> Option<ObjRef> {
        None
    }
    /// `-[DynamicWorld allBlockheadsIncludingNet]`.
    fn all_blockheads_including_net(&self) -> Vec<ObjRef> {
        Vec::new()
    }
    /// `-[DynamicWorld blockheadWithIDIncludingNet:]`.
    fn blockhead_with_id_including_net(&self, id: u64) -> Option<ObjRef> {
        None
    }
    /// `-[DynamicWorld blockheads]`.
    fn blockheads(&self) -> Vec<ObjRef> {
        Vec::new()
    }
    /// `-[DynamicWorld removeDynamicObjectsBelongingToClient:]`.
    fn remove_dynamic_objects_belonging_to_client(&self, client_id: &str) {}
    /// `-[DynamicWorld update:accurateDT:]`.
    fn update_accurate_dt(&self, dt: f32, accurate_dt: f32) {}
    /// `-[DynamicWorld setPaused:]`.
    fn dynamic_world_set_paused(&self, paused: bool) {}
    /// `-[DynamicWorld worldChangedAtPos:sendReliably:]`.
    fn world_changed_at_pos_send_reliably(&self, pos: IntPair, reliably: bool) {}
    /// `-[DynamicWorld worldContentsChangedAtPos:]`.
    fn world_contents_changed_at_pos(&self, pos: IntPair) {}
    /// `-[DynamicWorld waterChangedAtPos:fullBlock:]`.
    fn water_changed_at_pos_full_block(&self, pos: IntPair, full_block: bool) {}
    /// `-[DynamicWorld loadSurfaceBlockAtPos:]`.
    fn load_surface_block_at_pos(&self, pos: IntPair) {}
    /// `-[DynamicWorld loadSnowSurfaceBlockAtPos:loadSnow:]`.
    fn load_snow_surface_block_at_pos_load_snow(&self, pos: IntPair, load_snow: bool) {}
    /// `-[DynamicWorld loadGlowBlockIfNeededAtPos:tile:]`.
    fn load_glow_block_if_needed_at_pos_tile(&self, pos: IntPair, tile: &TileRef) {}
    /// `-[DynamicWorld loadGatherBlockAtPos:]`.
    fn load_gather_block_at_pos(&self, pos: IntPair) {}
    /// `-[DynamicWorld playTimeCrystalReceivedSoundAtPos:]`.
    fn play_time_crystal_received_sound_at_pos(&self, pos: IntPair) {}
    /// `-[DynamicWorld createFreeBlockAtPosition:ofType:dataA:dataB:subItems:dynamicObjectSaveDict:hovers:playSound:priorityBlockhead:]`.
    fn create_free_block_at_position_of_type_data_a_data_b_sub_items_dynamic_object_save_dict_hovers_play_sound_priority_blockhead(
        &self,
        pos: IntPair,
        item_type: ItemType,
        data_a: u16,
        data_b: u16,
        sub_items: Option<&[SubSlot]>,
        dynamic_object_save_dict: Option<&PlistValue>,
        hovers: bool,
        play_sound: i32,
        priority_blockhead: Option<&ObjRef>,
    ) {
    }
    /// `-[DynamicWorld createFreeBlockAtPosition:forForegroundContents:forTile:priorityBlockhead:]`.
    fn create_free_block_at_position_for_foreground_contents_for_tile_priority_blockhead(
        &self,
        pos: IntPair,
        for_foreground_contents: bool,
        tile: &TileRef,
        priority_blockhead: Option<&ObjRef>,
    ) {
    }
    /// `-[DynamicWorld createBackgroundContentFreeBlockAtPosition:forTile:removeBlockhead:]`.
    fn create_background_content_free_block_at_position_for_tile_remove_blockhead(
        &self,
        pos: IntPair,
        tile: &TileRef,
        remove_blockhead: Option<&ObjRef>,
    ) {
    }
    /// `-[DynamicWorld workbenchPlacedAtPosition:ofType:saveDict:placedByClient:clientName:]`.
    fn workbench_placed_at_position_of_type_save_dict_placed_by_client_client_name(
        &self,
        pos: IntPair,
        t: WorkbenchType,
        save_dict: Option<&PlistValue>,
        client_id: Option<&str>,
        client_name: Option<&str>,
    ) {
    }
    /// `-[DynamicWorld interactionObjectPlacedAtPosition:withItem:flipped:saveDict:placedByClient:clientName:]`.
    fn interaction_object_placed_at_position_with_item_flipped_save_dict_placed_by_client_client_name(
        &self,
        pos: IntPair,
        item: &InventoryItem,
        flipped: bool,
        save_dict: Option<&PlistValue>,
        client_id: Option<&str>,
        client_name: Option<&str>,
    ) -> Option<ObjRef> {
        None
    }
    /// `-[DynamicWorld sowTreeOrPlantAtPosition:itemType:maxHeight:growthRate:]`.
    fn sow_tree_or_plant_at_position_item_type_max_height_growth_rate(
        &self,
        pos: IntPair,
        item_type: ItemType,
        max_height: i32,
        growth_rate: i32,
    ) {
    }
    /// `-[DynamicWorld addTorchAtPos:ofType:dataA:dataB:saveDict:placedByClient:]`.
    fn add_torch_at_pos_of_type_data_a_data_b_save_dict_placed_by_client(
        &self,
        pos: IntPair,
        item_type: ItemType,
        data_a: u16,
        data_b: u16,
        save_dict: Option<&PlistValue>,
        client_id: Option<&str>,
    ) {
    }
    /// `-[DynamicWorld addEggAtPos:saveDict:]`.
    fn add_egg_at_pos_save_dict(&self, pos: IntPair, save_dict: Option<&PlistValue>) {}
    /// `-[DynamicWorld addPaintingAtPos:ofType:saveDict:placedByClient:clientName:]`.
    fn add_painting_at_pos_of_type_save_dict_placed_by_client_client_name(
        &self,
        pos: IntPair,
        item_type: ItemType,
        save_dict: Option<&PlistValue>,
        client_id: Option<&str>,
        client_name: Option<&str>,
    ) {
    }
    /// `-[DynamicWorld addWindowAtPos:ofType:saveDict:placedByClient:]`.
    fn add_window_at_pos_of_type_save_dict_placed_by_client(
        &self,
        pos: IntPair,
        item_type: ItemType,
        save_dict: Option<&PlistValue>,
        client_id: Option<&str>,
    ) {
    }
    /// `-[DynamicWorld addLadderAtPos:ofType:saveDict:placedByClient:]`.
    fn add_ladder_at_pos_of_type_save_dict_placed_by_client(
        &self,
        pos: IntPair,
        item_type: ItemType,
        save_dict: Option<&PlistValue>,
        client_id: Option<&str>,
    ) {
    }
    /// `-[DynamicWorld addWireAtPos:ofType:saveDict:placedByClient:]`.
    fn add_wire_at_pos_of_type_save_dict_placed_by_client(
        &self,
        pos: IntPair,
        item_type: ItemType,
        save_dict: Option<&PlistValue>,
        client_id: Option<&str>,
    ) {
    }
    /// `-[DynamicWorld addElevatorShaftAtPos:ofType:saveDict:placedByClient:]`.
    fn add_elevator_shaft_at_pos_of_type_save_dict_placed_by_client(
        &self,
        pos: IntPair,
        item_type: ItemType,
        save_dict: Option<&PlistValue>,
        client_id: Option<&str>,
    ) {
    }
    /// `-[DynamicWorld addElevatorMotorAtPos:ofType:saveDict:placedByClient:]`.
    fn add_elevator_motor_at_pos_of_type_save_dict_placed_by_client(
        &self,
        pos: IntPair,
        item_type: ItemType,
        save_dict: Option<&PlistValue>,
        client_id: Option<&str>,
    ) {
    }
    /// `-[DynamicWorld addColumnAtPos:ofType:saveDict:placedByClient:]`.
    fn add_column_at_pos_of_type_save_dict_placed_by_client(
        &self,
        pos: IntPair,
        item_type: ItemType,
        save_dict: Option<&PlistValue>,
        client_id: Option<&str>,
    ) {
    }
    /// `-[DynamicWorld addStairsAtPos:ofType:saveDict:placedByClient:]`.
    fn add_stairs_at_pos_of_type_save_dict_placed_by_client(
        &self,
        pos: IntPair,
        item_type: ItemType,
        save_dict: Option<&PlistValue>,
        client_id: Option<&str>,
    ) {
    }
    /// `-[DynamicWorld addDoorAtPos:ofType:saveDict:placedByClient:]`.
    fn add_door_at_pos_of_type_save_dict_placed_by_client(
        &self,
        pos: IntPair,
        item_type: ItemType,
        save_dict: Option<&PlistValue>,
        client_id: Option<&str>,
    ) {
    }
    /// `-[DynamicWorld addRailAtPos:ofType:ownedByStation:]`.
    fn add_rail_at_pos_of_type_owned_by_station(&self, pos: IntPair, item_type: ItemType, owned_by_station: bool) {}
    /// `-[DynamicWorld placeBoatInWaterAtPos:saveDict:placedByClient:]`.
    fn place_boat_in_water_at_pos_save_dict_placed_by_client(
        &self,
        pos: IntPair,
        save_dict: Option<&PlistValue>,
        client_id: Option<&str>,
    ) {
    }
    /// `-[DynamicWorld placeTrainCarAtPos:ofType:saveDict:placedByClient:]`.
    fn place_train_car_at_pos_of_type_save_dict_placed_by_client(
        &self,
        pos: IntPair,
        item_type: ItemType,
        save_dict: Option<&PlistValue>,
        client_id: Option<&str>,
    ) {
    }
    /// `-[DynamicWorld placeFireAtPosition:]`.
    fn place_fire_at_position(&self, pos: IntPair) {}
    /// `-[DynamicWorld loadNPCAtPosition:type:saveDict:isAdult:wasPlaced:placedByClient:]`.
    fn load_npc_at_position_type_save_dict_is_adult_was_placed_placed_by_client(
        &self,
        pos: IntPair,
        npc_type: NPCType,
        save_dict: Option<&PlistValue>,
        is_adult: bool,
        was_placed: bool,
        client_id: Option<&str>,
    ) {
    }
    /// `-[DynamicWorld clientBlockheadInventoryRecievedForPlayerID:blockheadID:data:]`.
    fn client_blockhead_inventory_recieved_for_player_id_blockhead_id_data(
        &self,
        player_id: Option<&str>,
        blockhead_id: u64,
        data: &[u8],
    ) {
    }
    /// `-[DynamicWorld loadNewBlockheadAtPos:craftableItemObject:uniqueID:]`.
    fn load_new_blockhead_at_pos_craftable_item_object_unique_id(
        &self,
        pos: IntPair,
        craftable_item_object: &ObjRef,
        unique_id: u64,
    ) {
    }
    /// `-[DynamicWorld loadDynamicObjectsForMacroTile:includeSurfaceBlocks:]`.
    fn load_dynamic_objects_for_macro_tile_include_surface_blocks(
        &self,
        mt: &MacroTileRef,
        include_surface_blocks: bool,
    ) {
    }
    /// `-[DynamicWorld removeDynamicObjectsForMacroTile:]`.
    fn remove_dynamic_objects_for_macro_tile(&self, mt: &MacroTileRef) {}
    /// `-[DynamicWorld addArtificialLightContributionForPhysicalBlockLoadedAtXPos:yPos:]`.
    fn add_artificial_light_contribution_for_physical_block_loaded_at_x_pos_y_pos(&self, x: i32, y: i32) {}
    /// `-[DynamicWorld doRepairForTileAtPos:]`.
    fn dynamic_world_do_repair_for_tile_at_pos(&self, pos: IntPair) {}

    // ------------------------------------------------------------------ WorldTileLoader
    /// `-[WorldTileLoader loadPhysicalBlock:atXPos:yPos:createIfNotCreated:]`. The loader assigns
    /// `physical_block`, `unexplored`, `dynamic_objects_loaded` and `valid` of the macro tile at `(x, y)` itself.
    fn load_physical_block_at_x_pos_y_pos_create_if_not_created(
        &self,
        pb: PhysicalBlockRef,
        x: i32,
        y: i32,
        create_if_not_created: bool,
    ) {
    }
    /// `-[WorldTileLoader savePhysicalBlock:macroTile:sendToClients:server:sendReliably:]`.
    fn save_physical_block_macro_tile_send_to_clients_server_send_reliably(
        &self,
        pb: PhysicalBlockRef,
        mt: &MacroTileRef,
        send_to_clients: &[String],
        send_reliably: bool,
    ) {
    }
    /// `-[WorldTileLoader loadLightBlockForClientLightBlockIndex:clientID:intoPhysicalBlock:]`.
    fn load_light_block_for_client_light_block_index_client_id_into_physical_block(
        &self,
        idx: i32,
        client_id: &str,
        pb: PhysicalBlockRef,
    ) {
    }
    /// `-[WorldTileLoader saveLightBlockForClientLightBlockIndex:clientID:physicalBlock:sendNow:]`.
    fn save_light_block_for_client_light_block_index_client_id_physical_block_send_now(
        &self,
        idx: i32,
        client_id: &str,
        pb: PhysicalBlockRef,
        send_now: bool,
    ) {
    }
    /// `-[WorldTileLoader sendBlockToClientWithoutSavingForBlock:pos:sendToClient:server:sendDynamicObjects:reliable:]`
    /// The return value is ignored by `World.update`.
    fn send_block_to_client_without_saving_for_block_pos_send_to_client_server_send_dynamic_objects_reliable(
        &self,
        pb: PhysicalBlockRef,
        macro_pos: IntPair,
        client_id: &str,
        send_dynamic_objects: bool,
        reliable: bool,
    ) -> bool {
        false
    }
    /// `-[WorldTileLoader unmodifiedGroundLevelForX:]`.
    fn unmodified_ground_level_for_x(&self, x: i32) -> i32 {
        0
    }
    /// `-[WorldTileLoader sandFractionForPos:highRes:]`.
    fn sand_fraction_for_pos_high_res(&self, pos: IntPair, high_res: bool) -> f32 {
        0.0
    }
    /// `-[WorldTileLoader archiveLightBlocksForClient:]`.
    fn archive_light_blocks_for_client(&self, client_id: &str) {}

    // ------------------------------------------------------------------ ServerClient, by peer id
    /// `-[ServerClient connected]`.
    fn sc_connected(&self, peer: &str) -> bool {
        false
    }
    /// `-[[ServerClient requestedBlockIndices] getIndexes:maxCount:4 inIndexRange:NULL]`: the (at most) `n`
    /// smallest requested indices, ascending (NSUInteger values).
    fn sc_requested_block_indices_first_n(&self, peer: &str, n: usize) -> Vec<u64> {
        Vec::new()
    }
    /// `-[ServerClient createIfNotCreatedForBlockRequest:]`.
    fn sc_create_if_not_created_for_block_request(&self, peer: &str, index: i32) -> bool {
        false
    }
    /// `-[ServerClient removeBlockRequest:]`.
    fn sc_remove_block_request(&self, peer: &str, index: i32) {}
    /// `-[ServerClient blockIsWired:]`.
    fn sc_block_is_wired(&self, peer: &str, macro_index: i32) -> bool {
        false
    }
    /// `-[ServerClient requestsHeartBeat]`.
    fn sc_requests_heart_beat(&self, peer: &str) -> bool {
        false
    }
    /// `-[ServerClient setRequestsHeartBeat:]`.
    fn sc_set_requests_heart_beat(&self, peer: &str, v: bool) {}
    /// `-[ServerClient setHasEverRequestedHeartbeat:]`.
    fn sc_set_has_ever_requested_heartbeat(&self, peer: &str, v: bool) {}
    /// `-[ServerClient setTimeSinceLastHeartbeatRequest:]`.
    fn sc_set_time_since_last_heartbeat_request(&self, peer: &str, v: f32) {}
    /// `-[ServerClient lightBlockIndex]`.
    fn sc_light_block_index(&self, peer: &str) -> i32 {
        -1
    }
    /// `-[ServerClient paused]`.
    fn sc_paused(&self, peer: &str) -> bool {
        false
    }

    // ------------------------------------------------------------------ BHServer
    /// `-[BHServer sendNetworkData:toPeers:reliable:]` (`None` = broadcast to the connected clients).
    fn send_network_data_to_peers_reliable(&self, data: &[u8], peers: Option<&[String]>, reliable: bool) {}
    /// `-[BHServer playerIsAdminWithID:]`.
    fn player_is_admin_with_id(&self, client_id: Option<&str>) -> bool {
        false
    }
    /// `-[BHServer updatePlayer:]` (arms the delayed anti-cheat kick).
    fn update_player(&self, client_id: &str) {}
    /// `[[BHServer playerInfoForPeerID:] objectForKey:@"alias"]`.
    fn player_alias_for_peer_id(&self, client_id: Option<&str>) -> Option<String> {
        None
    }
    /// `-[BHServer clientFinishedAwaySimulation:]`.
    fn client_finished_away_simulation(&self, client_id: &str) {}
    /// `-[BHServer sendPlayerChangedNotifcationToDelegate]`.
    fn send_player_changed_notifcation_to_delegate(&self) {}

    // ------------------------------------------------------------------ World siblings (world_core)
    /// `-[World heartbeatData]` (0x17 packet including the type byte).
    fn heartbeat_data(&self) -> Vec<u8> {
        Vec::new()
    }
    /// `-[World sendHeartbeatData]`.
    fn send_heartbeat_data(&self) {}
    /// `-[World isControllingBlockheadsForClientPlayer:]`.
    fn is_controlling_blockheads_for_client_player(&self, client_id: &str) -> bool {
        false
    }
    /// `-[World continueSimulate]` (only while `isSimulating`, never on the server).
    fn continue_simulate(&self) {}
    /// `-[delegate setPaused:]` (no-op stub in GameController).
    fn delegate_set_paused(&self, paused: bool) {}

    // ------------------------------------------------------------------ PathCreator (real type, pathfinding)
    /// The shared `PathCreator` (`World.pathCreator`). `None` skips the path pump.
    fn path_creator(&self) -> Option<Rc<crate::path::path_creator::PathCreator>> {
        None
    }

    // ------------------------------------------------------------------ other singletons
    /// `-[ProjectileManager update:]`.
    fn projectile_manager_update(&self, dt: f32) {}
    /// `-[NoiseFunction getX:Y:Z:octaves:]` of `World.weatherNoiseFunction`.
    fn weather_noise_get_x_y_z_octaves(&self, x: f64, y: f64, z: f64, octaves: i32) -> f64 {
        0.0
    }
    /// `-[Weather windStrength]` of `World.weather`.
    fn weather_wind_strength(&self) -> f32 {
        0.0
    }
    /// `-[CrystalManager setCountWatcher:nil]`.
    fn crystal_manager_set_count_watcher_none(&self) {}
    /// `-[CrystalManager amount]`.
    fn crystal_manager_amount(&self) -> i32 {
        0
    }
    /// `-[CrystalManager modify:modifyString:]`.
    fn crystal_manager_modify_modify_string(&self, amount: i32, modify_string: &str) {}
    /// `-[ParticleEmitter addParticleAtPos:velocity:color:gravityType:life:scale:]` on `ParticleEmitter.instance()`.
    fn particle_emitter_add_particle_at_pos_velocity_color_gravity_type_life_scale(
        &self,
        pos: [f32; 3],
        velocity: [f32; 3],
        color: [f32; 4],
        gravity_type: i32,
        life: f32,
        scale: f32,
    ) {
    }

    // ------------------------------------------------------------------ WorldHelper (lighting_env)
    /// `+[WorldHelper updateSunLightForTile:atPos:world:]`.
    fn update_sun_light_for_tile_at_pos_world(&self, tile: &TileRef, pos: IntPair) {}
    /// `+[WorldHelper updateSunLightRemovedForTile:atPos:world:]`.
    fn update_sun_light_removed_for_tile_at_pos_world(&self, tile: &TileRef, pos: IntPair) {}
    /// `+[WorldHelper recalculateLightingForPhysicalBlockIfNeeded:world:clientLightBlockIndex:forBlockhead:]`.
    fn recalculate_lighting_for_physical_block_if_needed_world_client_light_block_index_for_blockhead(
        &self,
        pb: Option<PhysicalBlockRef>,
        client_light_block_index: i32,
        blockhead: Option<&ObjRef>,
    ) {
    }
    /// `+[WorldHelper checkIfMacroTileCanBeDecommissioned:world:minAge:blockToSavePhyscialBlock:]`.
    /// The implementation may call back into `World::decommision_block_block_to_save_physcial_block`.
    fn check_if_macro_tile_can_be_decommissioned_world_min_age_block_to_save_physcial_block(
        &self,
        mt: Option<&MacroTileRef>,
        min_age: f64,
        block_to_save: bool,
    ) {
    }

    // ------------------------------------------------------------------ Blockhead (blockhead_*)
    /// `-[Blockhead asleep]`.
    fn bh_asleep(&self, bh: &ObjRef) -> bool {
        false
    }
    /// `-[Blockhead meditating]`.
    fn bh_meditating(&self, bh: &ObjRef) -> bool {
        false
    }
    /// `-[Blockhead regenerating]`.
    fn bh_regenerating(&self, bh: &ObjRef) -> bool {
        false
    }
    /// `-[Blockhead onTradeMission]`.
    fn bh_on_trade_mission(&self, bh: &ObjRef) -> bool {
        false
    }
    /// `-[Blockhead idle]`.
    fn bh_is_idle(&self, bh: &ObjRef) -> bool {
        true
    }
    /// `-[Blockhead isNet]`.
    fn bh_is_net(&self, bh: &ObjRef) -> bool {
        false
    }
    /// `-[Blockhead isClientBlockheadBeingControlledByServer]`.
    fn bh_is_client_blockhead_being_controlled_by_server(&self, bh: &ObjRef) -> bool {
        false
    }
    /// `-[Blockhead addToCrystalDiscrepency:]`.
    fn bh_add_to_crystal_discrepency(&self, bh: &ObjRef, n: i32) {}
    /// `-[Blockhead checkIfCanWarpInSecondBlockheadAfterItemAdded:dataB:]`.
    fn bh_check_if_can_warp_in_second_blockhead_after_item_added_data_b(
        &self,
        bh: &ObjRef,
        item: ItemType,
        data_b: u16,
    ) {
    }
    /// `-[Blockhead addExpectedCraftItem:]`.
    fn bh_add_expected_craft_item(&self, bh: &ObjRef, item: ItemType) {}
    /// `-[Blockhead localNetID]`.
    fn bh_local_net_id(&self, bh: &ObjRef) -> Option<String> {
        None
    }
    /// `-[Blockhead canUseDynamicObject:]`.
    fn bh_can_use_dynamic_object(&self, bh: &ObjRef, o: &ObjRef) -> bool {
        false
    }
    /// `-[Blockhead stopInteracting]`.
    fn bh_stop_interacting(&self, bh: &ObjRef) {}
    /// `-[Blockhead currentInteractionTypeForTile:atPos:pickupRejectedDueToInventoryFull:includeActions:faceIndex:allowProtectedActions:]`.
    fn bh_current_interaction_type_for_tile_at_pos_pickup_rejected_due_to_inventory_full_include_actions_face_index_allow_protected_actions(
        &self,
        bh: &ObjRef,
        tile: &TileRef,
        pos: IntPair,
        pickup_rejected: &mut bool,
        include_actions: bool,
        face_index: u8,
        allow_protected_actions: bool,
    ) -> InteractionType {
        InteractionType::INTERACTION_NONE
    }
    /// `-[Blockhead queueActionWithGoalPos:goalInteraction:pathType:interactionObjectID:craftableItemObject:craftCountOrExtraData:isAI:]`.
    fn bh_queue_action_with_goal_pos_goal_interaction_path_type_interaction_object_id_craftable_item_object_craft_count_or_extra_data_is_ai(
        &self,
        bh: &ObjRef,
        goal_pos: IntPair,
        goal_interaction: InteractionType,
        path_type: i32,
        interaction_object_id: u64,
        craftable_item_object: Option<&ObjRef>,
        craft_count_or_extra_data: i32,
        is_ai: bool,
    ) {
    }
    /// `-[Blockhead cancelAnyActionAtGoalPos:orWithInteractionObjectID:goalInteraction:craftCountOrExtraData:]`.
    fn bh_cancel_any_action_at_goal_pos_or_with_interaction_object_id_goal_interaction_craft_count_or_extra_data(
        &self,
        bh: &ObjRef,
        goal_pos: IntPair,
        interaction_object_id: u64,
        goal_interaction: InteractionType,
        craft_count_or_extra_data: i32,
    ) {
    }

    // ------------------------------------------------------------------ interaction objects, workbench, craftables
    /// `-[InteractionObject canBeUsedByBlockhead:]`.
    fn io_can_be_used_by_blockhead(&self, io: &ObjRef, bh: &ObjRef) -> bool {
        false
    }
    /// `-[InteractionObject isInUse]`.
    fn io_is_in_use(&self, io: &ObjRef) -> bool {
        false
    }
    /// `-[InteractionObject startInteractionWithBlockhead:]`.
    fn io_start_interaction_with_blockhead(&self, io: &ObjRef, bh: &ObjRef) {}
    /// `-[InteractionObject isPaintable]`.
    fn io_is_paintable(&self, io: &ObjRef) -> bool {
        false
    }
    /// `-[DynamicObject needsRemoved]`.
    fn obj_needs_removed(&self, o: &ObjRef) -> bool {
        false
    }
    /// `-[DynamicObject canBeRemovedByBlockhead:]` (Column / Ladder / Stairs / ElevatorShaft / InteractionObject).
    fn obj_can_be_removed_by_blockhead(&self, o: &ObjRef, bh: &ObjRef) -> bool {
        true
    }
    /// `-[... paint:]` on Column / Ladder / Stairs / ElevatorShaft / InteractionObject.
    fn obj_paint(&self, o: &ObjRef, color_index: u16) {}
    /// `-[Workbench pos]`.
    fn workbench_pos(&self, wb: &ObjRef) -> IntPair {
        IntPair { x: 0, y: 0 }
    }
    /// `-[Workbench type]`.
    fn workbench_type(&self, wb: &ObjRef) -> WorkbenchType {
        WorkbenchType::WORKBENCH_UNDEFINED
    }
    /// `-[Workbench level]`.
    fn workbench_level(&self, wb: &ObjRef) -> i32 {
        0
    }
    /// `-[Workbench upgradeToNextLevel]`.
    fn workbench_upgrade_to_next_level(&self, wb: &ObjRef) {}
    /// `-[CraftableItemObject craftableItem]` (struct 0x7c; all zero for a nil object).
    fn craftable_item(&self, obj: &ObjRef) -> CraftableItem {
        zero_craftable_item()
    }
    /// `-[CraftableItemObject freeBlockCreationItemSaveDict]`.
    fn free_block_creation_item_save_dict(&self, obj: &ObjRef) -> Option<PlistValue> {
        None
    }
    /// `-[Blockhead subtractItemsFromInventoryOfType:count:dataB:]`: number of items removed.
    fn bh_subtract_items_from_inventory_of_type_count_data_b(
        &self,
        bh: &ObjRef,
        item: ItemType,
        count: i32,
        data_b: i32,
    ) -> i32 {
        0
    }
}
}

/// A zero-initialised `CraftableItem` (what a nil `craftedItemObject` leaves in the local).
pub fn zero_craftable_item() -> CraftableItem {
    <CraftableItem as WireStruct>::read_le(&[0u8; 0x7c])
}

/// All-defaults implementation (nothing wired).
pub struct NullDeps;
impl WorldOpsDepsStub for NullDeps {}

thread_local! {
    static DEPS: RefCell<Rc<dyn WorldOpsDeps>> = RefCell::new(Rc::new(NullDeps));
}

/// Install the adapter that implements [`WorldOpsDeps`] (main thread; one world per process).
pub fn install_deps(d: Rc<dyn WorldOpsDeps>) {
    DEPS.with(|c| *c.borrow_mut() = d);
}

/// The installed adapter (cheap `Rc` clone; never hold a `RefCell` borrow across the call).
pub fn deps() -> Rc<dyn WorldOpsDeps> {
    DEPS.with(|c| c.borrow().clone())
}

// =====================================================================================================
// impl World: misc methods
// =====================================================================================================

/// Counted entries of a plist dictionary, in the dictionary's enumeration order.
fn counted(d: &PlistDict) -> Vec<(String, i32)> {
    d.keys_in_order()
        .into_iter()
        .map(|k| {
            let n = d.get(&k).map_or(0, |v| v.int_value());
            (k, n)
        })
        .collect()
}

/// Lower-cased display title (count included) of a numeric-string item type.
fn count_title(item: &str, n: i32) -> String {
    let ty = ItemType(item.parse::<i64>().unwrap_or(0) as u32);
    lowercase_string(&title_for_item_type_and_count_including_one(ty, n))
}

/// Which optional parts of a blockhead's record exist, plus the running "something reported" state.
struct Narrator<'a> {
    rec: &'a PlistDict,
    out: &'a mut String,
    reported: bool,
    has_place: bool,
    has_collapse: bool,
}

impl Narrator<'_> {
    fn slot(&self, n: u8) -> Option<&PlistValue> {
        self.rec.get(&n.to_string())
    }

    fn slot_counts(&self, n: u8) -> Option<Vec<(String, i32)>> {
        match self.slot(n) {
            Some(PlistValue::Dict(d)) => Some(counted(d)),
            _ => None,
        }
    }

    /// Opening of a clause: a space for the first one, a comma (optionally with "and") for later ones.
    fn open(&mut self, and_when_later: bool) {
        if self.reported {
            self.out.push_str(if and_when_later { ", and " } else { ", " });
        } else {
            self.out.push(' ');
        }
        self.reported = true;
    }

    /// Joins `parts` with ", ", using ", and " before the last part when `and_last` and there are several.
    fn join(parts: &[String], and_last: bool) -> String {
        let mut s = String::new();
        for (i, p) in parts.iter().enumerate() {
            if i > 0 {
                s.push_str(if and_last && i + 1 == parts.len() { ", and " } else { ", " });
            }
            s.push_str(p);
        }
        s
    }

    fn name(&mut self) {
        self.out.push(' ');
        match self.rec.get(&"blockheadName".to_string()).and_then(|v| v.as_str()) {
            Some(s) => self.out.push_str(&capitalized_string(s)),
            None => self.out.push_str("(null)"),
        }
    }

    fn damage(&mut self) {
        if let Some(PlistValue::Array(a)) = self.slot(4) {
            let mut clause = String::from("was");
            for (i, v) in a.items.iter().enumerate() {
                clause.push_str(if i == 0 { " " } else { ", " });
                clause.push_str(v.as_str().unwrap_or(""));
            }
            self.open(false);
            self.out.push_str(&clause);
        }
    }

    fn sleep(&mut self) {
        if self.slot(0).is_some() {
            self.open(false);
            self.out.push_str("slept");
        }
    }

    /// Fresh working copy of the obtained dictionary, made with the dictionary-copy semantics (`GsDict::copy_from`):
    /// capacity from the entry count, entries inserted in the source's enumeration order. Made unconditionally.
    fn obtained_copy(&self) -> GsDict<String, PlistValue> {
        match self.slot(2) {
            Some(PlistValue::Dict(d)) => GsDict::copy_from(&d.map),
            _ => GsDict::new(),
        }
    }

    /// Crafting; returns the working copy of the obtained items after the deductions.
    fn craft(
        &mut self,
        crafted: Vec<(String, i32)>,
        mut left: GsDict<String, PlistValue>,
    ) -> GsDict<String, PlistValue> {
        let last = crafted.len().saturating_sub(1);
        let mut clause = String::from("crafted");
        for (i, (item, n)) in crafted.iter().enumerate() {
            if let Some(have) = left.get(item).map(|v| v.int_value()) {
                let rest = have - n;
                if rest < 1 {
                    left.remove(item);
                } else {
                    left.insert(item.clone(), PlistValue::int(rest as i64));
                }
            }
            let and = i > 0 && left.is_empty() && !self.has_place && i == last;
            clause.push_str(if i == 0 {
                " "
            } else if and {
                ", and "
            } else {
                ", "
            });
            clause.push_str(&count_title(item, *n));
        }
        let nothing_after = left.is_empty() && !self.has_place;
        self.open(nothing_after);
        self.out.push_str(&clause);
        if nothing_after && !self.has_collapse {
            self.out.push('.');
        }
        left
    }

    fn obtained(&mut self, items: Vec<(String, i32)>) {
        if items.is_empty() {
            return;
        }
        let parts: Vec<String> = items.iter().map(|(k, n)| count_title(k, *n)).collect();
        let body = Self::join(&parts, !self.has_place);
        self.open(!self.has_place);
        self.out.push_str("got ");
        self.out.push_str(&body);
        if !self.has_place && !self.has_collapse {
            self.out.push('.');
        }
    }

    fn placed(&mut self, placed: Vec<(String, i32)>) {
        let parts: Vec<String> = placed
            .iter()
            .map(|(kind, n)| {
                let (verb, one, many) = match kind.as_str() {
                    "item" => ("placed", "item", "items"),
                    "sow" => ("planted", "seed", "seeds"),
                    _ => ("placed", "block", "blocks"),
                };
                format!("{verb} {n} {}", if *n > 1 { many } else { one })
            })
            .collect();
        let several = parts.len() > 1;
        let body = Self::join(&parts, true);
        // With several phrases the join already carries an "and"; a lone phrase takes it from the opening.
        self.open(!several);
        self.out.push_str(&body);
        if !self.has_collapse {
            self.out.push('.');
        }
    }

    fn collapse(&mut self) {
        if self.has_collapse {
            self.out.push_str(if self.reported {
                ", before collapsing from exhaustion."
            } else {
                " collapsed from exhaustion."
            });
        }
    }
}

/// Appends one blockhead's sentence fragment to `out`.
fn narrate_blockhead(out: &mut String, rec: &PlistDict) {
    let mut n = Narrator { rec, out, reported: false, has_place: false, has_collapse: false };
    n.has_place = n.slot(3).is_some();
    n.has_collapse = n.slot(5).is_some();
    n.name();
    n.damage();
    n.sleep();
    let crafted = n.slot_counts(1);
    let copy = n.obtained_copy();
    let left = match crafted {
        Some(c) => n.craft(c, copy),
        None => copy,
    };
    n.obtained(
        left.keys_in_order()
            .into_iter()
            .map(|k| {
                let n = left.get(&k).map_or(0, |v| v.int_value());
                (k, n)
            })
            .collect(),
    );
    if let Some(p) = n.slot_counts(3) {
        n.placed(p);
    }
    n.collapse();
}

/// One blockhead's typed away/offline-simulation record (world::simulation) in the dictionary shape the narrator
/// reads: `"blockheadName"`, then each event under the decimal string of its type: sleep / collapse as a
/// `{"count": n}` dictionary, craft / got as item type -> count, place as kind -> count, damage as the array of its
/// texts (the original's text -> text dictionary, enumerated).
fn events_record(ev: &crate::world::simulation::BlockheadEvents) -> PlistDict {
    use crate::world::simulation::SimEvent;
    let mut rec = GsDict::new();
    if let Some(n) = &ev.name {
        rec.insert("blockheadName".to_string(), PlistValue::string(n.clone()));
    }
    for ty in ev.events.keys_in_order() {
        let v = match ev.events.get(&ty).expect("listed key") {
            SimEvent::Count(c) => {
                PlistValue::dict(GsDict::from_pairs([("count".to_string(), PlistValue::int(*c as i64))]))
            }
            SimEvent::Items(items) => PlistValue::dict(GsDict::from_pairs(
                items.iter().map(|(t, n)| (t.to_string(), PlistValue::int(*n as i64))),
            )),
            SimEvent::Places(p) => {
                PlistValue::dict(GsDict::from_pairs(p.iter().map(|(k, n)| (k.clone(), PlistValue::int(*n as i64)))))
            }
            SimEvent::Damage(d) => PlistValue::array(d.iter().map(|(_, t)| PlistValue::string(t.clone())).collect()),
        };
        rec.insert(ty.to_string(), v);
    }
    let mut d = crate::objects::base::new_mutable_dict();
    d.map = rec;
    d
}

impl World {
    /// `-[World welcomeBackMessageForInfoDict:]` of a typed event map (world::simulation): the blockheads in the
    /// map's (uniqueID `NSNumber`) enumeration order.
    pub fn welcome_back_message_for_events(
        &self,
        events: Option<&crate::world::simulation::BlockheadEventsMap>,
    ) -> Option<String> {
        let events = events.filter(|e| !e.is_empty())?;
        let mut out = String::from("While you were away,");
        for (_, ev) in events.iter() {
            narrate_blockhead(&mut out, &events_record(ev));
        }
        Some(out)
    }

    /// `[self welcomeBackMessageForInfoDict:simulationEvents]` (offline catch-up simulation, `finishSimulating`).
    pub fn welcome_back_message_for_simulation_events(&self) -> Option<String> {
        crate::world::simulation::with_simulation_events(|s| self.welcome_back_message_for_events(s.as_ref()))
    }

    /// `server_clients` keys in GSIMap enumeration order; empty when the dictionary is nil.
    pub fn server_clients_keys_in_order(&self) -> Vec<String> {
        match self.server_clients.borrow().as_ref() {
            Some(m) => m.borrow().keys_in_order(),
            None => Vec::new(),
        }
    }

    /// `[serverClients count]` (0 when nil).
    pub fn server_clients_count(&self) -> usize {
        match self.server_clients.borrow().as_ref() {
            Some(m) => m.borrow().len(),
            None => 0,
        }
    }

    /// `-[World welcomeBackMessageForInfoDict:]`: the "While you were away, ..." text for a set of away-simulation
    /// events (blockhead key -> record), or `None` when there are no events. Event slots and item types are the
    /// decimal strings of their numeric keys.
    pub fn welcome_back_message_for_info_dict(&self, simulation_events: Option<&PlistValue>) -> Option<String> {
        let events = match simulation_events {
            Some(PlistValue::Dict(d)) if d.len() > 0 => d,
            _ => return None,
        };
        let mut out = String::from("While you were away,");
        for key in events.keys_in_order() {
            if let Some(PlistValue::Dict(record)) = events.get(&key) {
                narrate_blockhead(&mut out, record);
            }
        }
        Some(out)
    }

    /// `-[World welcomeBackEventsMessageForClientID:]`: `welcomeBackMessageForInfoDict:` of
    /// `awayClientSimulationEvents[clientID]`.
    /// The events are the typed `awayClientSimulationEvents` that `addSimulationEventOfType:forBlockhead:extraData:`
    /// fills (world::simulation).
    pub fn welcome_back_events_message_for_client_id(&self, client_id: &str) -> Option<String> {
        crate::world::simulation::with_away_client_simulation_events(|a| {
            let entry = a.as_ref().and_then(|m| m.get(&client_id.to_string()));
            self.welcome_back_message_for_events(entry)
        })
    }

    /// `-[World setNetEventsMessageToDisplayOnceLoaded:]` (retain/release, no copy).
    pub fn set_net_events_message_to_display_once_loaded(&self, m: Option<String>) {
        *self.net_events_message_to_display_once_loaded.borrow_mut() = m;
    }

    /// `-[World cancelAllActionsAtPos:orWithInteractionObjectID:]`: acts on the active blockhead only (nil on the
    /// server).
    pub fn cancel_all_actions_at_pos_or_with_interaction_object_id(&self, pos: IntPair, unique_id: u64) {
        let d = deps();
        if let Some(bh) = d.active_blockhead() {
            d.bh_cancel_any_action_at_goal_pos_or_with_interaction_object_id_goal_interaction_craft_count_or_extra_data(
                &bh,
                pos,
                unique_id,
                InteractionType::INTERACTION_NONE,
                0,
            );
        }
    }

    /// `-[World queueBlockheadAIActionToTile:atPos:forBlockhead:]`.
    pub fn queue_blockhead_ai_action_to_tile_at_pos_for_blockhead(
        &self,
        tile: &TileRef,
        tile_pos: IntPair,
        bh: &ObjRef,
    ) {
        let d = deps();
        let mut pickup_rejected = false;
        let mut goal = d.bh_current_interaction_type_for_tile_at_pos_pickup_rejected_due_to_inventory_full_include_actions_face_index_allow_protected_actions(
            bh,
            tile,
            tile_pos,
            &mut pickup_rejected,
            false,
            0,
            true,
        );
        if interaction_type_disallowed_on_protected_tiles(goal)
            && self.tile_is_protected_at_pos_against_blockhead(tile_pos, bh)
        {
            goal = d.bh_current_interaction_type_for_tile_at_pos_pickup_rejected_due_to_inventory_full_include_actions_face_index_allow_protected_actions(
                bh,
                tile,
                tile_pos,
                &mut pickup_rejected,
                false,
                0,
                false,
            );
        }
        let mut obj_id = 0u64;
        if goal == InteractionType::INTERACTION_USE_INTERACTIVE_OBJECT {
            // `[[dynamicWorld interactionObjectAtPos:] uniqueID]`; nil => 0
            if let Some(dw) = self.dynamic_world() {
                if let Some(io) = dw.interaction_object_at_pos(tile_pos) {
                    obj_id = io.obj.base().borrow().unique_id;
                }
            }
        }
        d.bh_queue_action_with_goal_pos_goal_interaction_path_type_interaction_object_id_craftable_item_object_craft_count_or_extra_data_is_ai(
            bh, tile_pos, goal, 0, obj_id, None, 0, true,
        );
    }

    /// `-[World pauseUpdates]`: with remote clients connected only the dynamic world is paused; always followed by
    /// a heartbeat broadcast.
    pub fn pause_updates(&self) {
        let d = deps();
        let has_server = self.server().is_some();
        // `if (server != nil) { if (count != 0) skip; } if (client == nil) [delegate setPaused:YES];`
        if !(has_server && self.server_clients_count() != 0) {
            d.delegate_set_paused(true);
        }
        d.dynamic_world_set_paused(true);
        d.send_heartbeat_data();
    }

    /// `-[World resetPauseIdleTimer]`: `pauseIdleTimer = 0`.
    pub fn reset_pause_idle_timer(&self) {
        self.pause_idle_timer.set(0.0);
    }

    /// `-[World updateIdleTimerDisabled]`: sets the ivar only.
    pub fn update_idle_timer_disabled(&self) {
        let mut new_disabled = false;
        if self.server().is_some() && self.server_clients_count() != 0 {
            new_disabled = true;
        } else {
            // `client != nil` is never true; `uiManager` is nil so `gameBlockingUIDisplayed` is NO.
            for bh in deps().blockheads() {
                // `-[Blockhead idle]` is "isIdle" of step 8/9: not available separately.
                if !deps().bh_is_idle(&bh) {
                    new_disabled = true;
                    break;
                }
            }
        }
        if new_disabled != self.idle_timer_disabled.get() {
            self.idle_timer_disabled.set(new_disabled);
        }
    }

    /// `-[World displayOwnershipAreas]`: empty body.
    pub fn display_ownership_areas(&self) {}

    /// `-[World renderingTeaserFrames]`: `hideUIType == 4`.
    pub fn rendering_teaser_frames(&self) -> bool {
        self.hide_ui_type.get() == 4
    }
}

#[cfg(test)]
pub(crate) mod testkit {
    //! A recording [`WorldOpsDeps`] used by the world_ops unit tests.
    use super::*;
    use crate::foundation::gsdict::GsDict;
    use std::cell::{Cell, RefCell};
    use std::collections::HashMap;

    /// Records every call as a `name(args)` line and answers from scripted tables.
    #[derive(Default)]
    pub struct Rec {
        pub log: RefCell<Vec<String>>,
        pub admin: RefCell<Vec<String>>,
        pub alias: RefCell<HashMap<String, String>>,
        pub connected: RefCell<HashMap<String, bool>>,
        pub requested: RefCell<HashMap<String, Vec<u64>>>,
        pub create_req: Cell<bool>,
        pub requests_hb: RefCell<HashMap<String, bool>>,
        pub light_idx: RefCell<HashMap<String, i32>>,
        pub controlling: RefCell<Vec<String>>,
        pub blockheads: RefCell<Vec<ObjRef>>,
        pub asleep: RefCell<Vec<usize>>,
        pub heartbeat: RefCell<Vec<u8>>,
        pub wired: RefCell<Vec<(String, i32)>>,
        pub noise: Cell<f64>,
        pub sand: Cell<f32>,
        pub load_makes_valid: Cell<bool>,
        pub send_block_calls: RefCell<Vec<(i32, i32, String)>>,
        pub world: RefCell<Option<std::rc::Weak<World>>>,
        pub path_creator: RefCell<Option<Rc<crate::path::path_creator::PathCreator>>>,
        /// `bh_is_net` / `bh_is_client_blockhead_being_controlled_by_server` / `io_is_paintable` answers.
        pub net_bh: Cell<bool>,
        pub controlled_bh: Cell<bool>,
        pub paintable: Cell<bool>,
        /// What `blockhead_with_id_including_net` returns.
        pub bh_by_id: RefCell<Option<ObjRef>>,
        /// What `workbench_at_pos` returns, with its level and type.
        pub wb: RefCell<Option<ObjRef>>,
        pub wb_level: Cell<i32>,
        pub wb_type: Cell<u32>,
        /// `obj_can_be_removed_by_blockhead` answers the opposite of the default.
        pub cannot_remove: Cell<bool>,
    }
    impl Rec {
        pub fn push(&self, s: String) {
            self.log.borrow_mut().push(s);
        }
        pub fn take(&self) -> Vec<String> {
            std::mem::take(&mut *self.log.borrow_mut())
        }
    }
    fn p(pos: IntPair) -> String {
        format!("({},{})", pos.x, pos.y)
    }
    impl WorldOpsDepsStub for Rec {
        fn bh_is_net(&self, _bh: &ObjRef) -> bool {
            self.net_bh.get()
        }
        fn bh_is_client_blockhead_being_controlled_by_server(&self, _bh: &ObjRef) -> bool {
            self.controlled_bh.get()
        }
        fn bh_add_to_crystal_discrepency(&self, _bh: &ObjRef, n: i32) {
            self.push(format!("crystalDiscrepency({n})"));
        }
        fn io_is_paintable(&self, _io: &ObjRef) -> bool {
            self.paintable.get()
        }
        fn blockhead_with_id_including_net(&self, _id: u64) -> Option<ObjRef> {
            self.bh_by_id.borrow().clone()
        }
        fn workbench_at_pos(&self, _pos: IntPair) -> Option<ObjRef> {
            self.wb.borrow().clone()
        }
        fn workbench_level(&self, _wb: &ObjRef) -> i32 {
            self.wb_level.get()
        }
        fn workbench_type(&self, _wb: &ObjRef) -> WorkbenchType {
            WorkbenchType(self.wb_type.get())
        }
        fn obj_can_be_removed_by_blockhead(&self, _o: &ObjRef, _bh: &ObjRef) -> bool {
            !self.cannot_remove.get()
        }
        fn all_blockheads_including_net(&self) -> Vec<ObjRef> {
            self.blockheads.borrow().clone()
        }
        fn blockheads(&self) -> Vec<ObjRef> {
            self.blockheads.borrow().clone()
        }
        fn bh_asleep(&self, bh: &ObjRef) -> bool {
            let id = crate::objects::handle::obj_id(bh);
            self.asleep.borrow().contains(&id)
        }
        fn update_accurate_dt(&self, dt: f32, accurate_dt: f32) {
            self.push(format!("dw.update({},{})", dt, accurate_dt));
        }
        fn projectile_manager_update(&self, dt: f32) {
            self.push(format!("pm.update({})", dt));
        }
        fn send_heartbeat_data(&self) {
            self.push("sendHeartbeatData".into());
        }
        fn heartbeat_data(&self) -> Vec<u8> {
            self.heartbeat.borrow().clone()
        }
        fn is_controlling_blockheads_for_client_player(&self, c: &str) -> bool {
            self.controlling.borrow().iter().any(|x| x == c)
        }
        fn sc_connected(&self, peer: &str) -> bool {
            *self.connected.borrow().get(peer).unwrap_or(&false)
        }
        fn sc_requested_block_indices_first_n(&self, peer: &str, n: usize) -> Vec<u64> {
            let mut v = self.requested.borrow().get(peer).cloned().unwrap_or_default();
            v.sort();
            v.truncate(n);
            v
        }
        fn sc_create_if_not_created_for_block_request(&self, peer: &str, index: i32) -> bool {
            self.push(format!("sc.create({},{})", peer, index));
            self.create_req.get()
        }
        fn sc_remove_block_request(&self, peer: &str, index: i32) {
            self.push(format!("sc.remove({},{})", peer, index));
            if let Some(v) = self.requested.borrow_mut().get_mut(peer) {
                v.retain(|x| *x as u32 as i32 != index);
            }
        }
        fn sc_block_is_wired(&self, peer: &str, macro_index: i32) -> bool {
            self.wired.borrow().iter().any(|(p, i)| p == peer && *i == macro_index)
        }
        fn sc_requests_heart_beat(&self, peer: &str) -> bool {
            *self.requests_hb.borrow().get(peer).unwrap_or(&false)
        }
        fn sc_set_requests_heart_beat(&self, peer: &str, v: bool) {
            self.push(format!("sc.setRequestsHeartBeat({},{})", peer, v));
            self.requests_hb.borrow_mut().insert(peer.to_string(), v);
        }
        fn sc_set_has_ever_requested_heartbeat(&self, peer: &str, v: bool) {
            self.push(format!("sc.setHasEverRequestedHeartbeat({},{})", peer, v));
        }
        fn sc_set_time_since_last_heartbeat_request(&self, peer: &str, v: f32) {
            self.push(format!("sc.setTimeSinceLastHeartbeatRequest({},{})", peer, v));
        }
        fn sc_light_block_index(&self, peer: &str) -> i32 {
            *self.light_idx.borrow().get(peer).unwrap_or(&-1)
        }
        fn send_network_data_to_peers_reliable(&self, data: &[u8], peers: Option<&[String]>, reliable: bool) {
            self.push(format!("send({:?},{:?},{})", data, peers, reliable));
        }
        fn player_is_admin_with_id(&self, id: Option<&str>) -> bool {
            id.map_or(false, |i| self.admin.borrow().iter().any(|a| a == i))
        }
        fn update_player(&self, id: &str) {
            self.push(format!("updatePlayer({})", id));
        }
        fn player_alias_for_peer_id(&self, id: Option<&str>) -> Option<String> {
            id.and_then(|i| self.alias.borrow().get(i).cloned())
        }
        fn client_finished_away_simulation(&self, id: &str) {
            self.push(format!("clientFinishedAwaySimulation({})", id));
        }
        fn send_player_changed_notifcation_to_delegate(&self) {
            self.push("playerChanged".into());
        }
        fn remove_dynamic_objects_belonging_to_client(&self, id: &str) {
            self.push(format!("removeDynamicObjectsBelongingToClient({})", id));
        }
        fn world_changed_at_pos_send_reliably(&self, pos: IntPair, r: bool) {
            self.push(format!("worldChanged{}{}", p(pos), r));
        }
        fn world_contents_changed_at_pos(&self, pos: IntPair) {
            self.push(format!("contentsChanged{}", p(pos)));
        }
        fn water_changed_at_pos_full_block(&self, pos: IntPair, f: bool) {
            self.push(format!("waterChanged{}{}", p(pos), f));
        }
        fn load_surface_block_at_pos(&self, pos: IntPair) {
            self.push(format!("loadSurface{}", p(pos)));
        }
        fn load_snow_surface_block_at_pos_load_snow(&self, pos: IntPair, s: bool) {
            self.push(format!("loadSnowSurface{}{}", p(pos), s));
        }
        fn load_glow_block_if_needed_at_pos_tile(&self, pos: IntPair, _t: &TileRef) {
            self.push(format!("loadGlow{}", p(pos)));
        }
        fn load_gather_block_at_pos(&self, pos: IntPair) {
            self.push(format!("loadGather{}", p(pos)));
        }
        fn play_time_crystal_received_sound_at_pos(&self, pos: IntPair) {
            self.push(format!("crystalSound{}", p(pos)));
        }
        fn create_free_block_at_position_of_type_data_a_data_b_sub_items_dynamic_object_save_dict_hovers_play_sound_priority_blockhead(
            &self,
            pos: IntPair,
            t: ItemType,
            a: u16,
            b: u16,
            _s: Option<&[SubSlot]>,
            _d: Option<&PlistValue>,
            hovers: bool,
            snd: i32,
            bh: Option<&ObjRef>,
        ) {
            self.push(format!(
                "freeBlock{} type={} a={} b={} hovers={} snd={} bh={}",
                p(pos),
                t.0,
                a,
                b,
                hovers,
                snd,
                bh.is_some()
            ));
        }
        fn create_free_block_at_position_for_foreground_contents_for_tile_priority_blockhead(
            &self,
            pos: IntPair,
            fg: bool,
            _t: &TileRef,
            bh: Option<&ObjRef>,
        ) {
            self.push(format!("freeBlockForTile{} fg={} bh={}", p(pos), fg, bh.is_some()));
        }
        fn create_background_content_free_block_at_position_for_tile_remove_blockhead(
            &self,
            pos: IntPair,
            _t: &TileRef,
            _bh: Option<&ObjRef>,
        ) {
            self.push(format!("bgFreeBlock{}", p(pos)));
        }
        fn update_sun_light_for_tile_at_pos_world(&self, _t: &TileRef, pos: IntPair) {
            self.push(format!("sunLight{}", p(pos)));
        }
        fn update_sun_light_removed_for_tile_at_pos_world(&self, _t: &TileRef, pos: IntPair) {
            self.push(format!("sunLightRemoved{}", p(pos)));
        }
        fn add_door_at_pos_of_type_save_dict_placed_by_client(
            &self,
            pos: IntPair,
            t: ItemType,
            _s: Option<&PlistValue>,
            c: Option<&str>,
        ) {
            self.push(format!("addDoor{} type={} client={:?}", p(pos), t.0, c));
        }
        fn add_torch_at_pos_of_type_data_a_data_b_save_dict_placed_by_client(
            &self,
            pos: IntPair,
            t: ItemType,
            a: u16,
            b: u16,
            _s: Option<&PlistValue>,
            c: Option<&str>,
        ) {
            self.push(format!("addTorch{} type={} a={} b={} client={:?}", p(pos), t.0, a, b, c));
        }
        fn add_ladder_at_pos_of_type_save_dict_placed_by_client(
            &self,
            pos: IntPair,
            t: ItemType,
            _s: Option<&PlistValue>,
            c: Option<&str>,
        ) {
            self.push(format!("addLadder{} type={} client={:?}", p(pos), t.0, c));
        }
        fn add_column_at_pos_of_type_save_dict_placed_by_client(
            &self,
            pos: IntPair,
            t: ItemType,
            _s: Option<&PlistValue>,
            c: Option<&str>,
        ) {
            self.push(format!("addColumn{} type={} client={:?}", p(pos), t.0, c));
        }
        fn add_stairs_at_pos_of_type_save_dict_placed_by_client(
            &self,
            pos: IntPair,
            t: ItemType,
            _s: Option<&PlistValue>,
            c: Option<&str>,
        ) {
            self.push(format!("addStairs{} type={} client={:?}", p(pos), t.0, c));
        }
        fn add_window_at_pos_of_type_save_dict_placed_by_client(
            &self,
            pos: IntPair,
            t: ItemType,
            _s: Option<&PlistValue>,
            c: Option<&str>,
        ) {
            self.push(format!("addWindow{} type={} client={:?}", p(pos), t.0, c));
        }
        fn add_wire_at_pos_of_type_save_dict_placed_by_client(
            &self,
            pos: IntPair,
            t: ItemType,
            _s: Option<&PlistValue>,
            c: Option<&str>,
        ) {
            self.push(format!("addWire{} type={} client={:?}", p(pos), t.0, c));
        }
        fn add_rail_at_pos_of_type_owned_by_station(&self, pos: IntPair, t: ItemType, o: bool) {
            self.push(format!("addRail{} type={} station={}", p(pos), t.0, o));
        }
        fn sow_tree_or_plant_at_position_item_type_max_height_growth_rate(
            &self,
            pos: IntPair,
            t: ItemType,
            h: i32,
            g: i32,
        ) {
            self.push(format!("sow{} type={} h={} g={}", p(pos), t.0, h, g));
        }
        fn place_fire_at_position(&self, pos: IntPair) {
            self.push(format!("placeFire{}", p(pos)));
        }
        fn place_boat_in_water_at_pos_save_dict_placed_by_client(
            &self,
            pos: IntPair,
            _s: Option<&PlistValue>,
            c: Option<&str>,
        ) {
            self.push(format!("placeBoat{} client={:?}", p(pos), c));
        }
        fn workbench_placed_at_position_of_type_save_dict_placed_by_client_client_name(
            &self,
            pos: IntPair,
            t: WorkbenchType,
            _s: Option<&PlistValue>,
            c: Option<&str>,
            n: Option<&str>,
        ) {
            self.push(format!("workbenchPlaced{} type={} client={:?} name={:?}", p(pos), t.0, c, n));
        }
        fn interaction_object_placed_at_position_with_item_flipped_save_dict_placed_by_client_client_name(
            &self,
            pos: IntPair,
            item: &InventoryItem,
            flipped: bool,
            _s: Option<&PlistValue>,
            c: Option<&str>,
            _n: Option<&str>,
        ) -> Option<ObjRef> {
            self.push(format!("ioPlaced{} type={} flipped={} client={:?}", p(pos), item.item_type.0, flipped, c));
            None
        }
        fn crystal_manager_set_count_watcher_none(&self) {
            self.push("crystal.setCountWatcher(nil)".into());
        }
        fn crystal_manager_amount(&self) -> i32 {
            1000
        }
        fn crystal_manager_modify_modify_string(&self, amount: i32, s: &str) {
            self.push(format!("crystal.modify({},{})", amount, s));
        }
        fn bh_check_if_can_warp_in_second_blockhead_after_item_added_data_b(&self, _bh: &ObjRef, t: ItemType, b: u16) {
            self.push(format!("bh.checkWarp({},{})", t.0, b));
        }
        fn weather_noise_get_x_y_z_octaves(&self, x: f64, y: f64, z: f64, o: i32) -> f64 {
            self.push(format!("noise({},{},{},{})", x, y, z, o));
            self.noise.get()
        }
        fn sand_fraction_for_pos_high_res(&self, pos: IntPair, hi: bool) -> f32 {
            self.push(format!("sand{}{}", p(pos), hi));
            self.sand.get()
        }
        fn load_physical_block_at_x_pos_y_pos_create_if_not_created(
            &self,
            pb: PhysicalBlockRef,
            x: i32,
            y: i32,
            create: bool,
        ) {
            self.push(format!("loader.load({},{},{})", x, y, create));
            if self.load_makes_valid.get() {
                if let Some(w) = self.world.borrow().as_ref().and_then(|w| w.upgrade()) {
                    let mt = crate::world::tiles::macro_tile_at_macro_postion(x, y, &w.macro_tiles(), &w).unwrap();
                    pb.x_pos.set(x);
                    pb.y_pos.set(y);
                    pb.valid.set(true);
                    *mt.physical_block.borrow_mut() = Some(pb);
                    mt.unexplored.set(0);
                }
            }
        }
        fn send_block_to_client_without_saving_for_block_pos_send_to_client_server_send_dynamic_objects_reliable(
            &self,
            _pb: PhysicalBlockRef,
            macro_pos: IntPair,
            client_id: &str,
            _d: bool,
            _r: bool,
        ) -> bool {
            self.send_block_calls.borrow_mut().push((macro_pos.x, macro_pos.y, client_id.to_string()));
            self.push(format!("sendBlock({},{},{})", macro_pos.x, macro_pos.y, client_id));
            true
        }
        fn save_physical_block_macro_tile_send_to_clients_server_send_reliably(
            &self,
            _pb: PhysicalBlockRef,
            _mt: &MacroTileRef,
            clients: &[String],
            r: bool,
        ) {
            self.push(format!("savePhysicalBlock({:?},{})", clients, r));
        }
        fn remove_dynamic_objects_for_macro_tile(&self, _mt: &MacroTileRef) {
            self.push("removeDynamicObjectsForMacroTile".into());
        }
        fn load_dynamic_objects_for_macro_tile_include_surface_blocks(&self, _mt: &MacroTileRef, s: bool) {
            self.push(format!("loadDynamicObjectsForMacroTile({})", s));
        }
        fn add_artificial_light_contribution_for_physical_block_loaded_at_x_pos_y_pos(&self, x: i32, y: i32) {
            self.push(format!("addArtificialLight({},{})", x, y));
        }
        fn recalculate_lighting_for_physical_block_if_needed_world_client_light_block_index_for_blockhead(
            &self,
            pb: Option<PhysicalBlockRef>,
            idx: i32,
            _bh: Option<&ObjRef>,
        ) {
            self.push(format!("recalcLighting(pb={},{})", pb.is_some(), idx));
        }
        fn check_if_macro_tile_can_be_decommissioned_world_min_age_block_to_save_physcial_block(
            &self,
            mt: Option<&MacroTileRef>,
            min_age: f64,
            save: bool,
        ) {
            self.push(format!("checkDecommission({},{},{})", mt.is_some(), min_age, save));
        }
        fn load_light_block_for_client_light_block_index_client_id_into_physical_block(
            &self,
            idx: i32,
            client: &str,
            _pb: PhysicalBlockRef,
        ) {
            self.push(format!("loadLightBlock({},{})", idx, client));
        }
        fn save_light_block_for_client_light_block_index_client_id_physical_block_send_now(
            &self,
            idx: i32,
            client: &str,
            _pb: PhysicalBlockRef,
            now: bool,
        ) {
            self.push(format!("saveLightBlock({},{},{})", idx, client, now));
        }
        fn delegate_set_paused(&self, v: bool) {
            self.push(format!("delegate.setPaused({})", v));
        }
        fn dynamic_world_set_paused(&self, v: bool) {
            self.push(format!("dw.setPaused({})", v));
        }
        fn path_creator(&self) -> Option<Rc<crate::path::path_creator::PathCreator>> {
            self.path_creator.borrow().clone()
        }
    }

    /// Install a fresh recorder and return it.
    pub fn install() -> Rc<Rec> {
        let r = Rc::new(Rec::default());
        install_deps(r.clone());
        r
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::foundation::gsdict::GsDict;

    fn num(v: i64) -> PlistValue {
        PlistValue::int(v)
    }
    fn dict(pairs: Vec<(&str, PlistValue)>) -> PlistValue {
        PlistValue::dict(GsDict::from_pairs(pairs.into_iter().map(|(k, v)| (k.to_string(), v))))
    }

    /// T6: synthetic cases.
    #[test]
    fn welcome_back_message() {
        let w = World::alloc(32);
        assert_eq!(w.welcome_back_message_for_info_dict(None), None);
        assert_eq!(w.welcome_back_message_for_info_dict(Some(&dict(vec![]))), None);
        let ev = dict(vec![("1", dict(vec![("blockheadName", PlistValue::string("bob")), ("0", num(1))]))]);
        assert_eq!(w.welcome_back_message_for_info_dict(Some(&ev)).as_deref(), Some("While you were away, Bob slept"));
        let ev = dict(vec![("1", dict(vec![("blockheadName", PlistValue::string("bob")), ("5", num(1))]))]);
        assert_eq!(
            w.welcome_back_message_for_info_dict(Some(&ev)).as_deref(),
            Some("While you were away, Bob collapsed from exhaustion.")
        );
        let ev =
            dict(vec![("1", dict(vec![("blockheadName", PlistValue::string("al")), ("0", num(1)), ("5", num(1))]))]);
        assert_eq!(
            w.welcome_back_message_for_info_dict(Some(&ev)).as_deref(),
            Some("While you were away, Al slept, before collapsing from exhaustion.")
        );
        // place events
        let ev = dict(vec![(
            "1",
            dict(vec![("blockheadName", PlistValue::string("al")), ("3", dict(vec![("sow", num(2))]))]),
        )]);
        assert_eq!(
            w.welcome_back_message_for_info_dict(Some(&ev)).as_deref(),
            Some("While you were away, Al planted 2 seeds.")
        );
    }

    fn msg(recs: Vec<PlistValue>) -> Option<String> {
        let w = World::alloc(32);
        let pairs: Vec<(String, PlistValue)> = recs.into_iter().enumerate().map(|(i, r)| (i.to_string(), r)).collect();
        let ev = PlistValue::dict(GsDict::from_pairs(pairs.into_iter()));
        w.welcome_back_message_for_info_dict(Some(&ev))
    }
    fn rec(extra: Vec<(&str, PlistValue)>) -> PlistValue {
        let mut v = vec![("blockheadName", PlistValue::string("bob"))];
        v.extend(extra);
        dict(v)
    }
    fn m1(extra: Vec<(&str, PlistValue)>) -> String {
        msg(vec![rec(extra)]).unwrap()
    }
    fn ct(item: &str, n: i32) -> String {
        count_title(item, n)
    }
    fn dmg() -> PlistValue {
        PlistValue::array(vec![PlistValue::string("fell")])
    }
    const P: &str = "While you were away, Bob";

    /// The away-simulation events recorded by `addSimulationEventOfType:forBlockhead:extraData:` (typed, world::
    /// simulation) are what the welcome-back text is made of.
    #[test]
    fn welcome_back_from_recorded_away_events() {
        use crate::world::simulation::{with_away_client_simulation_events, BlockheadEvents, SimEvent};
        let w = World::alloc(32);
        let mut events = GsDict::new();
        events.insert(0, SimEvent::Count(1)); // slept
        events.insert(1, SimEvent::Items(GsDict::from_pairs([(1048i64, 2i32)]))); // crafted
        events.insert(2, SimEvent::Items(GsDict::from_pairs([(1048i64, 3i32)]))); // got
        events.insert(3, SimEvent::Places(GsDict::from_pairs([("sow".to_string(), 2i32)])));
        events.insert(4, SimEvent::Damage(GsDict::from_pairs([("hurt".to_string(), "hurt".to_string())])));
        events.insert(5, SimEvent::Count(1)); // collapsed
        let mut per_bh = GsDict::new();
        per_bh.insert(7u64, BlockheadEvents { name: Some("bob".into()), events });
        per_bh.insert(8u64, BlockheadEvents { name: None, events: GsDict::new() });
        with_away_client_simulation_events(|a| {
            let mut m = GsDict::new();
            m.insert("cid".to_string(), per_bh);
            *a = Some(m);
        });
        let msg = w.welcome_back_events_message_for_client_id("cid").expect("message");
        assert!(msg.starts_with("While you were away,"), "{msg}");
        for part in ["Bob", "was hurt", "slept", "crafted", "planted 2 seeds", "collapsing from exhaustion", "(null)"] {
            assert!(msg.contains(part), "{part} missing in {msg}");
        }
        assert_eq!(w.welcome_back_events_message_for_client_id("other"), None);
        // the offline catch-up simulation reads `simulationEvents`
        assert_eq!(w.welcome_back_message_for_simulation_events(), None);
        with_away_client_simulation_events(|a| *a = None);
    }

    #[test]
    fn welcome_back_degenerate_inputs() {
        assert_eq!(msg(vec![dict(vec![("0", num(1))])]).unwrap(), "While you were away, (null) slept");
        assert_eq!(msg(vec![num(3)]).unwrap(), "While you were away,");
        let w = World::alloc(32);
        assert_eq!(w.welcome_back_message_for_info_dict(Some(&num(1))), None);
        assert_eq!(m1(vec![]), P);
        // client lookup
        assert_eq!(w.welcome_back_events_message_for_client_id("nobody"), None);
    }

    #[test]
    fn welcome_back_damage() {
        let d = PlistValue::array(vec![PlistValue::string("fell"), PlistValue::string("burned"), num(1)]);
        assert_eq!(m1(vec![("4", d)]), format!("{P} was fell, burned, "));
        assert_eq!(m1(vec![("4", PlistValue::array(vec![]))]), format!("{P} was"));
        assert_eq!(m1(vec![("4", num(1))]), P);
        assert_eq!(
            m1(vec![("4", dmg()), ("0", num(1)), ("5", num(1))]),
            format!("{P} was fell, slept, before collapsing from exhaustion.")
        );
    }

    #[test]
    fn welcome_back_craft() {
        let c1 = dict(vec![("1", num(2))]);
        assert_eq!(m1(vec![("1", c1)]), format!("{P} crafted {}.", ct("1", 2)));
        let c2 = || dict(vec![("1", num(1)), ("2", num(3))]);
        assert_eq!(m1(vec![("1", c2())]), format!("{P} crafted {}, and {}.", ct("1", 1), ct("2", 3)));
        // collapse suppresses the full stop (first clause)
        assert_eq!(
            m1(vec![("1", c2()), ("5", num(1))]),
            format!("{P} crafted {}, and {}, before collapsing from exhaustion.", ct("1", 1), ct("2", 3))
        );
        // later clause variants
        assert_eq!(
            m1(vec![("0", num(1)), ("1", c2())]),
            format!("{P} slept, and crafted {}, and {}.", ct("1", 1), ct("2", 3))
        );
        assert_eq!(
            m1(vec![("0", num(1)), ("1", c2()), ("5", num(1))]),
            format!("{P} slept, and crafted {}, and {}, before collapsing from exhaustion.", ct("1", 1), ct("2", 3))
        );
        // remaining obtained or a place record: plain comma joins, no "and" before the last element
        let place = dict(vec![("item", num(1))]);
        assert_eq!(
            m1(vec![("0", num(1)), ("1", c2()), ("3", place)]),
            format!("{P} slept, crafted {}, {}, and placed 1 item.", ct("1", 1), ct("2", 3))
        );
        let got = dict(vec![("9", num(4))]);
        assert_eq!(
            m1(vec![("0", num(1)), ("1", c2()), ("2", got)]),
            format!("{P} slept, crafted {}, {}, and got {}.", ct("1", 1), ct("2", 3), ct("9", 4))
        );
    }

    #[test]
    fn welcome_back_obtained_deductions() {
        // fully consumed by crafting -> removed; partially -> remainder reported
        let craft = dict(vec![("1", num(2)), ("2", num(1))]);
        // event dictionaries come from the plist parser (9 buckets: order 1, 2, 3); the working copy has 3 buckets
        // and enumerates 3, 1, 2, so after crafting item 1 away the clause lists 3 then 2
        let got = {
            let mut d = GsDict::plist_parsed();
            for (k, n) in [("1", 2), ("2", 5), ("3", 1)] {
                d.insert(k.to_string(), num(n));
            }
            PlistValue::dict(d)
        };
        assert_eq!(
            m1(vec![("1", craft), ("2", got)]),
            format!("{P} crafted {}, {}, and got {}, and {}.", ct("1", 2), ct("2", 1), ct("3", 1), ct("2", 4))
        );
        // no craft record: the full copy is enumerated in copy order 3, 1, 2
        let full = {
            let mut d = GsDict::plist_parsed();
            for (k, n) in [("1", 2), ("2", 5), ("3", 1)] {
                d.insert(k.to_string(), num(n));
            }
            PlistValue::dict(d)
        };
        assert_eq!(m1(vec![("2", full)]), format!("{P} got {}, {}, and {}.", ct("3", 1), ct("1", 2), ct("2", 5)));
        // obtained without crafting; single, several, first/later clause, collapse, place
        let g1 = || dict(vec![("7", num(1))]);
        assert_eq!(m1(vec![("2", g1())]), format!("{P} got {}.", ct("7", 1)));
        let g2 = || dict(vec![("7", num(1)), ("8", num(2))]);
        assert_eq!(m1(vec![("2", g2())]), format!("{P} got {}, and {}.", ct("7", 1), ct("8", 2)));
        assert_eq!(
            m1(vec![("2", g2()), ("5", num(1))]),
            format!("{P} got {}, and {}, before collapsing from exhaustion.", ct("7", 1), ct("8", 2))
        );
        assert_eq!(m1(vec![("0", num(1)), ("2", g1())]), format!("{P} slept, and got {}.", ct("7", 1)));
        assert_eq!(
            m1(vec![("0", num(1)), ("2", g1()), ("5", num(1))]),
            format!("{P} slept, and got {}, before collapsing from exhaustion.", ct("7", 1))
        );
        assert_eq!(
            m1(vec![("0", num(1)), ("2", g2()), ("3", dict(vec![("sow", num(1))]))]),
            format!("{P} slept, got {}, {}, and planted 1 seed.", ct("7", 1), ct("8", 2))
        );
        assert_eq!(
            m1(vec![("2", g1()), ("3", dict(vec![("sow", num(1))]))]),
            format!("{P} got {}, and planted 1 seed.", ct("7", 1))
        );
    }

    #[test]
    fn welcome_back_placed() {
        let pl = |v: Vec<(&str, i64)>| dict(v.into_iter().map(|(k, n)| (k, num(n))).collect());
        assert_eq!(m1(vec![("3", pl(vec![("sow", 2)]))]), format!("{P} planted 2 seeds."));
        assert_eq!(m1(vec![("3", pl(vec![("item", 1)]))]), format!("{P} placed 1 item."));
        assert_eq!(m1(vec![("3", pl(vec![("item", 3)]))]), format!("{P} placed 3 items."));
        assert_eq!(m1(vec![("3", pl(vec![("other", 1)]))]), format!("{P} placed 1 block."));
        assert_eq!(
            m1(vec![("3", pl(vec![("other", 5), ("sow", 1)])), ("5", num(1))]),
            format!("{P} placed 5 blocks, and planted 1 seed, before collapsing from exhaustion.")
        );
        assert_eq!(
            m1(vec![("3", pl(vec![("other", 5), ("sow", 1)]))]),
            format!("{P} placed 5 blocks, and planted 1 seed.")
        );
        assert_eq!(
            m1(vec![("3", pl(vec![("other", 5), ("item", 2), ("sow", 1)]))]),
            format!("{P} placed 5 blocks, placed 2 items, and planted 1 seed.")
        );
        // later clause: single phrase gets "and"; multiple do not (the join already has it)
        assert_eq!(m1(vec![("0", num(1)), ("3", pl(vec![("sow", 1)]))]), format!("{P} slept, and planted 1 seed."));
        assert_eq!(
            m1(vec![("0", num(1)), ("3", pl(vec![("other", 2), ("sow", 1)]))]),
            format!("{P} slept, placed 2 blocks, and planted 1 seed.")
        );
        assert_eq!(
            m1(vec![("0", num(1)), ("3", pl(vec![("sow", 1)])), ("5", num(1))]),
            format!("{P} slept, and planted 1 seed, before collapsing from exhaustion.")
        );
        assert_eq!(
            m1(vec![("0", num(1)), ("3", pl(vec![("other", 2), ("sow", 1)])), ("5", num(1))]),
            format!("{P} slept, placed 2 blocks, and planted 1 seed, before collapsing from exhaustion.")
        );
        assert_eq!(
            m1(vec![("3", pl(vec![("sow", 1)])), ("5", num(1))]),
            format!("{P} planted 1 seed, before collapsing from exhaustion.")
        );
        // empty place record
        assert_eq!(m1(vec![("3", pl(vec![]))]), format!("{P} ."));
        // two blockheads
        assert_eq!(
            msg(vec![rec(vec![("0", num(1))]), rec(vec![("5", num(1))])]).unwrap(),
            "While you were away, Bob slept Bob collapsed from exhaustion."
        );
    }

    #[test]
    fn idle_timer_and_pause() {
        let _rec = testkit::install();
        let w = World::alloc(32);
        w.pause_idle_timer.set(5.0);
        w.reset_pause_idle_timer();
        assert_eq!(w.pause_idle_timer.get(), 0.0);
        assert!(!w.rendering_teaser_frames());
        w.hide_ui_type.set(4);
        assert!(w.rendering_teaser_frames());
        w.pause_updates();
        let log = _rec.take();
        assert_eq!(log, vec!["delegate.setPaused(true)", "dw.setPaused(true)", "sendHeartbeatData"]);
        install_deps(Rc::new(NullDeps));
    }
}
