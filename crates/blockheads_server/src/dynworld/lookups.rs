//! Lookups of `DynamicWorld`: by position / id / tap, NPC proximity queries, blockhead lists, role flags, accessors,
//! plus the seam trait [`DwOpsHooks`] through which this module (and the other `dynamic_world_ops` files) reach the
//! concrete object classes and the `World`/`BHServer` methods that do not exist as Rust code yet.
//!
//! `elevatorMotorForShaftAtPos:`; `doorAtPos:`... `posOfDoorsOtherBlockAtPos:`, 2.2 `checkForBoatUnderTap:`,
//! `boatWithID:`, `checkForTrainCarUnderTap:`, `trainCarWithID:`, 2.3, 2.4, 2.5, 2.6, 2.7, 2.8 `gatherBlockAtPos:`,
//! 2.10, 2.13, 2.14; `pathUsers`.
//!
//! ## Seam
//! The concrete classes (Tree, Plant, Torch, Workbench, Chest, Blockhead,...) and several `World`/`BHServer`
//! methods are still empty skeletons. [`DwOpsHooks`] lists every call this module makes into them; all methods have
//! inert defaults. The integrator installs the real adapter with [`set_ops_hooks`] (thread-local, one game thread).

use std::cell::RefCell;
use std::rc::Rc;

use super::state::{DynamicWorld, MAX_DYNAMIC_OBJECT_TYPES};
use crate::foundation::indexset::IndexSet;
use crate::items::inventory_item::ItemRef;
use crate::objects::base::{DynamicWorldApi, DynamicWorldRef};
use crate::objects::handle::{ptr_eq, ObjRef};
use crate::objects::type_tables::{
    DYNAMIC_OBJECT_TYPE_BOAT, DYNAMIC_OBJECT_TYPE_DOOR, DYNAMIC_OBJECT_TYPE_ELEVATOR_MOTOR,
    DYNAMIC_OBJECT_TYPE_NPC_CAVETROLL, DYNAMIC_OBJECT_TYPE_OWNERSHIP_SIGN, DYNAMIC_OBJECT_TYPE_PAINTING,
    DYNAMIC_OBJECT_TYPE_SIGN, DYNAMIC_OBJECT_TYPE_TRADE_PORTAL, DYNAMIC_OBJECT_TYPE_TRAIN_STATION,
    DYNAMIC_OBJECT_TYPE_WORKBENCH, INTERACTION_OBJECT_DYNAMIC_OBJECT_TYPES, NPC_DYNAMIC_OBJECT_TYPES,
    PLANT_DYNAMIC_OBJECT_TYPES, TREE_DYNAMIC_OBJECT_TYPES,
};
use crate::plist::value::{PlistDict, PlistValue};
use crate::types::structs::{IntPair, Vector2};
use crate::world::helpers::object_type_may_have_artifical_light;
use crate::world::state::{MacroTileRef, TileRef};
use crate::world::tiles::{macro_tile_at_world_postion, tile_at_world_position_loaded};

/// `TRAIN_CAR_DYNAMIC_OBJECT_TYPES`.
pub const TRAIN_CAR_DYNAMIC_OBJECT_TYPES: [i32; 4] = [41, 42, 43, 44];

/// Sign-extended map key of a world index (`-1` becomes `u64::MAX`).
#[inline]
pub(crate) fn wi_key(wi: i32) -> u64 {
    wi as i64 as u64
}

/// Valid table index test (`DynamicObjectType` 0..=64).
#[inline]
fn t_ok(t: i32) -> bool {
    (0..MAX_DYNAMIC_OBJECT_TYPES as i32).contains(&t)
}

// =====================================================================================================
// The seam
// =====================================================================================================

/// Every call `dynamic_world_ops` makes into classes / modules that are not part of `dynamic_world_core`.
/// All methods have inert defaults (`None` / `false` / `0`): a partial integration overrides only what it needs.
#[allow(unused_variables)]
pub trait DwOpsHooks {
    // ---- constructors (each runs `initWithWorld:...` to completion including its own RNG draws) ----
    /// `trees::tree::new_tree_for_type(...)`: class table of (`None` = unknown type).
    fn new_tree(
        &self,
        dw: &Rc<DynamicWorld>,
        tree_type: i32,
        pos: IntPair,
        max_height: i16,
        growth_rate: i16,
        adult_tree: bool,
        adult_max_age: f32,
    ) -> Option<ObjRef> {
        None
    }
    /// `plants::plant::new_plant_for_type(...)`: class table of.
    fn new_plant(
        &self,
        dw: &Rc<DynamicWorld>,
        plant_type: i32,
        pos: IntPair,
        max_age_gene: i16,
        growth_rate_gene: i16,
        adult: bool,
    ) -> Option<ObjRef> {
        None
    }
    /// `classForNPCType` + `initWithWorld:...saveDict:isAdult:wasPlaced:placedByClient:`.
    fn new_npc(
        &self,
        dw: &Rc<DynamicWorld>,
        npc_type: i32,
        pos: IntPair,
        save_dict: Option<&PlistDict>,
        is_adult: bool,
        was_placed: bool,
        client_id: Option<&str>,
    ) -> Option<ObjRef> {
        None
    }
    /// `Torch initWithWorld:...type:dataA:dataB:saveDict:placedByClient:`
    fn new_torch(
        &self,
        dw: &Rc<DynamicWorld>,
        pos: IntPair,
        ty: i32,
        data_a: u16,
        data_b: u16,
        save_dict: Option<&PlistDict>,
        client_id: Option<&str>,
    ) -> Option<ObjRef> {
        None
    }
    /// `Egg initWithWorld:...saveDict:`
    fn new_egg(&self, dw: &Rc<DynamicWorld>, pos: IntPair, save_dict: Option<&PlistDict>) -> Option<ObjRef> {
        None
    }
    /// `Painting initWithWorld:...type:saveDict:placedByClient:clientName:`
    fn new_painting(
        &self,
        dw: &Rc<DynamicWorld>,
        pos: IntPair,
        ty: i32,
        save_dict: Option<&PlistDict>,
        client_id: Option<&str>,
        client_name: Option<&str>,
    ) -> Option<ObjRef> {
        None
    }
    /// `Rail initWithWorld:...type:ownedByStation:`
    fn new_rail(&self, dw: &Rc<DynamicWorld>, pos: IntPair, ty: i32, owned_by_station: bool) -> Option<ObjRef> {
        None
    }
    /// `Boat initWithWorld:...saveDict:placedByClient:`
    fn new_boat(
        &self,
        dw: &Rc<DynamicWorld>,
        pos: IntPair,
        save_dict: Option<&PlistDict>,
        client_id: Option<&str>,
    ) -> Option<ObjRef> {
        None
    }
    /// `HandCar / SteamTrain / FreightCar / PassengerCar initWithWorld:...saveDict:placedByClient:` by item type
    /// (204 / 205 / 206 / 208; any other item type: `None`).
    fn new_train_car(
        &self,
        dw: &Rc<DynamicWorld>,
        item_type: i32,
        pos: IntPair,
        save_dict: Option<&PlistDict>,
        client_id: Option<&str>,
    ) -> Option<ObjRef> {
        None
    }
    /// `Workbench initWithWorld:...type:flipped:saveDict:placedByClient:clientName:`
    fn new_workbench(
        &self,
        dw: &Rc<DynamicWorld>,
        pos: IntPair,
        ty: i32,
        flipped: bool,
        save_dict: Option<&PlistDict>,
        client_id: Option<&str>,
        client_name: Option<&str>,
    ) -> Option<ObjRef> {
        None
    }
    /// `TradePortal initWithWorld:...item:flipped:saveDict:placedByClient:clientName:` (item type 210)
    fn new_trade_portal(
        &self,
        dw: &Rc<DynamicWorld>,
        pos: IntPair,
        item_type: i32,
        flipped: bool,
        save_dict: Option<&PlistDict>,
        client_id: Option<&str>,
        client_name: Option<&str>,
    ) -> Option<ObjRef> {
        None
    }
    /// `Chest initWithWorld:...item:flipped:saveDict:placedByClient:clientName:` (item type 1072)
    fn new_chest(
        &self,
        dw: &Rc<DynamicWorld>,
        pos: IntPair,
        item_type: i32,
        flipped: bool,
        save_dict: Option<&PlistDict>,
        client_id: Option<&str>,
        client_name: Option<&str>,
    ) -> Option<ObjRef> {
        None
    }
    /// `GlowBlock initWithWorld:...tile:`
    fn new_glow_block(&self, dw: &Rc<DynamicWorld>, pos: IntPair, tile: TileRef) -> Option<ObjRef> {
        None
    }
    /// `Blockhead initWithWorld:...blockheadNumber:craftableItemObject:uniqueID:`; `cio` is opaque here.
    fn new_blockhead(
        &self,
        dw: &Rc<DynamicWorld>,
        pos: IntPair,
        blockhead_number: i32,
        cio: &ObjRef,
        unique_id: u64,
    ) -> Option<ObjRef> {
        None
    }

    // ---- Tree ----
    /// `-treeType`
    fn tree_type(&self, tree: &ObjRef) -> i32 {
        0
    }
    /// `-height`
    fn tree_height(&self, tree: &ObjRef) -> i32 {
        0
    }
    /// `-maxHeightGeneVariation`
    fn tree_max_height_gene_variation(&self, tree: &ObjRef) -> i32 {
        0
    }
    /// `-growthRateGeneVariation`
    fn tree_growth_rate_gene_variation(&self, tree: &ObjRef) -> i32 {
        0
    }
    /// `-isRequiredSoilType:`
    fn tree_is_required_soil_type(&self, tree: &ObjRef, soil: u8) -> bool {
        false
    }

    // ---- Plant ----
    /// `-plantType`
    fn plant_type(&self, plant: &ObjRef) -> i32 {
        0
    }
    /// `-maxAgeGeneVariation`
    fn plant_max_age_gene_variation(&self, plant: &ObjRef) -> i32 {
        0
    }
    /// `-growthRateGeneVariation`
    fn plant_growth_rate_gene_variation(&self, plant: &ObjRef) -> i32 {
        0
    }
    /// `-isRequiredSoilType:`
    fn plant_is_required_soil_type(&self, plant: &ObjRef, soil: u8) -> bool {
        false
    }
    /// `-canBreed`
    fn plant_can_breed(&self, plant: &ObjRef) -> bool {
        false
    }
    /// `-numberOfOccupiedTilesAbove`
    fn plant_number_of_occupied_tiles_above(&self, plant: &ObjRef) -> i32 {
        0
    }
    /// `-numberOfOccupiedTilesBelow`
    fn plant_number_of_occupied_tiles_below(&self, plant: &ObjRef) -> i32 {
        0
    }

    // ---- NPC / taps ----
    /// `-tapIsWithinBodyRadius:` for objects that do not expose `HarmableDynamicObject` (boats, train cars).
    fn tap_is_within_body_radius(&self, o: &ObjRef, tap: Vector2) -> bool {
        false
    }
    /// `NPC -canMate`
    fn npc_can_mate(&self, npc: &ObjRef) -> bool {
        false
    }
    /// `NPC -canBeFedByBlockhead:`
    fn npc_can_be_fed_by_blockhead(&self, npc: &ObjRef, bh: &ObjRef) -> bool {
        false
    }
    /// `CaveTroll -controlIsLocal`
    fn cave_troll_control_is_local(&self, troll: &ObjRef) -> bool {
        crate::objects::handle::downcast::<crate::npc::cave_troll::CaveTroll>(troll)
            .map_or(false, |t| t.control_is_local())
    }
    /// `[[bh currentItem] itemType]` (0 when the blockhead or its item is nil)
    fn blockhead_current_item_type(&self, bh: &ObjRef) -> i32 {
        0
    }
    /// `-goalInteractionForNPCChaseForNPC:withItemType:` (25 = FEED_NPC, 26 = INSPECT_NPC)
    fn blockhead_goal_interaction_for_npc_chase(&self, bh: &ObjRef, npc: &ObjRef, item_type: i32) -> i32 {
        0
    }
    /// `Blockhead -crouching`
    fn blockhead_crouching(&self, bh: &ObjRef) -> bool {
        false
    }
    /// `Blockhead -teleportToPos:`
    fn blockhead_teleport_to_pos(&self, bh: &ObjRef, pos: IntPair) -> bool {
        false
    }
    /// `Blockhead -stopAllActions`
    fn blockhead_stop_all_actions(&self, bh: &ObjRef) {}
    /// `Blockhead -setPaused:`
    fn blockhead_set_paused(&self, bh: &ObjRef, paused: bool) {}
    /// `Blockhead -remotePickupRequestResponse:uniqueIDs:count:`
    fn blockhead_remote_pickup_request_response(&self, bh: &ObjRef, accepted: bool, ids: &[u64]) {}
    /// `Blockhead -updateNameTextView` (client UI text; harmless on the server)
    fn blockhead_update_name_text_view(&self, bh: &ObjRef) {}

    // ---- Doors ----
    /// `Door -canBeUsedByBlockhead:`
    fn door_can_be_used_by_blockhead(&self, door: &ObjRef, bh: &ObjRef) -> bool {
        false
    }
    /// `Door -isOpen`
    fn door_is_open(&self, door: &ObjRef) -> bool {
        false
    }
    /// `Door -setOpen:direction:`
    fn door_set_open_direction(&self, door: &ObjRef, open: bool, direction: i32) {}

    // ---- Elevators ----
    /// `ElevatorShaft -open`
    fn elevator_shaft_open(&self, shaft: &ObjRef) {}
    /// `ElevatorShaft -lastKnownMotorPos`
    fn elevator_shaft_last_known_motor_pos(&self, shaft: &ObjRef) -> IntPair {
        IntPair { x: 0, y: 0 }
    }
    /// `ElevatorMotor -minY`
    fn elevator_motor_min_y(&self, motor: &ObjRef) -> i32 {
        0
    }
    /// `ElevatorMotor -maxY`
    fn elevator_motor_max_y(&self, motor: &ObjRef) -> i32 {
        0
    }

    // ---- misc objects ----
    /// `TrainCar -railOrStationNameChanged`
    fn train_car_rail_or_station_name_changed(&self, car: &ObjRef) {}
    /// `TrainCar -setPaused:` (train cars are not interaction objects)
    fn train_car_set_paused(&self, car: &ObjRef, paused: bool) {}
    /// `Painting -imageData`
    fn painting_image_data(&self, painting: &ObjRef) -> Option<Vec<u8>> {
        None
    }
    /// `Painting -imageDataRecieved:` (client path)
    fn painting_image_data_recieved(&self, painting: &ObjRef, data: &[u8]) {}
    /// `Painting -userMuteChanged:`
    fn painting_user_mute_changed(&self, painting: &ObjRef, player_id: &str) {}
    /// `Painting -userBanChanged:isBanned:`
    fn painting_user_ban_changed(&self, painting: &ObjRef, player_id: &str, banned: bool) {}
    /// `FreeBlock -hovers`
    fn freeblock_hovers(&self, fb: &ObjRef) -> bool {
        false
    }
    /// `[snowSurfaceBlock updateInTimeSinceSaved]`
    fn object_update_in_time_since_saved(&self, o: &ObjRef) {}
    /// `Chest -loadInventoryItemsFromDisk IfNeeded`
    fn chest_load_inventory_items_from_disk_if_needed(&self, chest: &ObjRef) {}
    /// `Chest -inventoryDataAllowingEmpty:`
    fn chest_inventory_data(&self, chest: &ObjRef, allow_empty: bool) -> Option<Vec<u8>> {
        None
    }
    /// `Chest -remoteInventoryDataRecieved:` (client path)
    fn chest_remote_inventory_data_recieved(&self, chest: &ObjRef, data: Option<&[u8]>) {}
    /// `[obj isKindOfClass:[Chest class]]`
    fn is_chest(&self, o: &ObjRef) -> bool {
        false
    }
    /// `Chest -moveInventoryItemsFromArray:toIndex:count:` (takes from the TAIL of `coins`,)
    fn chest_move_inventory_items_from_array_to_index_count(
        &self,
        chest: &ObjRef,
        coins: &mut Vec<ItemRef>,
        index: i32,
        count: i32,
    ) {
    }
    /// `TradePortal -randomizeLocalTradeOffsets` (draws RNG)
    fn trade_portal_randomize_local_trade_offsets(&self, portal: &ObjRef) {}
    /// `OwnershipSign -landOwnerID`
    fn interaction_object_land_owner_id(&self, o: &ObjRef) -> Option<String> {
        None
    }
    /// `OwnershipSign -widthRadius`
    fn interaction_object_width_radius(&self, o: &ObjRef) -> i32 {
        0
    }
    /// `OwnershipSign -heightRadius`
    fn interaction_object_height_radius(&self, o: &ObjRef) -> i32 {
        0
    }

    // ---- World ----
    /// `-[World removeTileAtWorldX:worldY:createContentsFreeblockCount:0 createForegroundContentsFreeblockCount:0 removeBlockhead:nil]`
    fn world_remove_tile_at_world_x_world_y(&self, x: i32, y: i32) {}
    /// `-[World ownershipSignWasPlacedOrChangedAtPos:withLandOwner:widthRadius:heightRadius:wasRemoved:]`
    fn world_ownership_sign_was_placed_or_changed_at_pos(
        &self,
        pos: IntPair,
        land_owner_id: Option<&str>,
        width_radius: i32,
        height_radius: i32,
        was_removed: bool,
    ) {
    }
    /// `-[World welcomeBackEventsMessageForClientID:]`
    fn world_welcome_back_events_message_for_client_id(&self, client_id: &str) -> Option<PlistValue> {
        None
    }

    // ---- BHServer ----
    /// `-[BHServer sendNetworkData:toPeers:reliable:]`
    fn server_send_network_data_to_peers_reliable(&self, data: Vec<u8>, peers: &[String], reliable: bool) {}
    /// `-[BHServer playerNameForPlayerWithIDIncludingOldPlayers:]`
    fn server_player_name_for_player_with_id_including_old_players(&self, player_id: &str) -> Option<String> {
        None
    }
    /// `-[BHServer playerIsBannedWithID:]`
    fn server_player_is_banned_with_id(&self, player_id: &str) -> bool {
        false
    }

    // ---- ServerClient.foundItemsList (None == the record does not exist) ----
    /// `[[serverClients objectForKey:id] foundItemsList]` as a snapshot
    fn found_items_list_snapshot(&self, client_id: &str) -> Option<IndexSet> {
        None
    }
    /// `[fl removeAllIndexes]` on the record's list (no-op when there is no record)
    fn found_items_list_remove_all_indexes(&self, client_id: &str) {}
    /// `[fl addIndexes:set]` on the record's list (no-op when there is no record)
    fn found_items_list_add_indexes(&self, client_id: &str, set: &IndexSet) {}

    // ---- particles (visual only; the 1792 `rand()` draws are made by the caller) ----
    /// `-[ParticleEmitter addParticleAtPos:velocity:color:gravityType:life:scale:center:]`
    fn particle_emitter_add_particle(
        &self,
        pos: [f32; 3],
        velocity: [f32; 3],
        color: [f32; 4],
        gravity_type: i32,
        life: f32,
        scale: f32,
        center: [f32; 4],
    ) {
    }
}

/// The default hooks: every default method of [`DwOpsHooks`].
pub struct NullOpsHooks;
impl DwOpsHooks for NullOpsHooks {}

thread_local! {
    static OPS_HOOKS: RefCell<Rc<dyn DwOpsHooks>> = RefCell::new(Rc::new(NullOpsHooks));
}

/// Install the cross-module call implementation (the original has a single game thread, so a thread-local is the
/// faithful scope).
pub fn set_ops_hooks(h: Rc<dyn DwOpsHooks>) {
    OPS_HOOKS.with(|c| *c.borrow_mut() = h);
}

/// The installed hooks (cheap `Rc` clone; never hold a `RefCell` borrow across a hook call).
pub(crate) fn ops() -> Rc<dyn DwOpsHooks> {
    OPS_HOOKS.with(|c| c.borrow().clone())
}

/// `[o tapIsWithinBodyRadius:tap]`: `HarmableDynamicObject` first, else the hook.
pub(crate) fn tap_within(o: &ObjRef, tap: Vector2) -> bool {
    if let Some(h) = o.as_harmable() {
        h.tap_is_within_body_radius(tap)
    } else {
        ops().tap_is_within_body_radius(o, tap)
    }
}

/// `[o twoBlocksWide]` (nil/non-interaction object: NO).
fn two_blocks_wide(o: &ObjRef) -> bool {
    o.as_interaction_like().map_or(false, |i| i.two_blocks_wide())
}
/// `[o isDoubleHeight]`
fn is_double_height(o: &ObjRef) -> bool {
    o.as_interaction_like().map_or(false, |i| i.is_double_height())
}

/// C integer wrap rule shared by the NPC queries: `d < W/2 ? (d < -W/2 ? d + W: d): d - W`.
#[inline]
pub(crate) fn wrap_dx(d: i32, w: i32) -> i32 {
    if d < w / 2 {
        if d < (w.wrapping_neg()) / 2 {
            d.wrapping_add(w)
        } else {
            d
        }
    } else {
        d.wrapping_sub(w)
    }
}

// =====================================================================================================
// Table primitives
// =====================================================================================================

impl DynamicWorld {
    /// `dynamicObjects[t]` values, ascending uid (snapshot).
    pub(crate) fn dot_snapshot(&self, t: i32) -> Vec<ObjRef> {
        if !t_ok(t) {
            return Vec::new();
        }
        self.dynamic_objects.borrow()[t as usize].values().cloned().collect()
    }
    /// `dynamicObjectsToAdd[t]` values, ascending uid (snapshot).
    pub(crate) fn add_snapshot(&self, t: i32) -> Vec<ObjRef> {
        if !t_ok(t) {
            return Vec::new();
        }
        self.dynamic_objects_to_add.borrow()[t as usize].values().cloned().collect()
    }
    /// `dynamicObjects[t].find(uid)`.
    pub(crate) fn dot_get(&self, t: i32, uid: u64) -> Option<ObjRef> {
        if t_ok(t) {
            self.do_get(t as usize, uid)
        } else {
            None
        }
    }
    /// `dynamicObjectsToAdd[t].find(uid)`.
    pub(crate) fn add_get(&self, t: i32, uid: u64) -> Option<ObjRef> {
        if t_ok(t) {
            self.ta_get(t as usize, uid)
        } else {
            None
        }
    }
    /// REGISTER: `dynamicObjectsToAdd[t][o.uniqueID] = o` (overwrite).
    pub(crate) fn register_to_add(&self, o: &ObjRef, t: i32) {
        if t_ok(t) {
            self.ta_set(t as usize, o.unique_id(), o.clone());
        }
    }
    /// `dynamicObjectsByWorldPosIndex[t].count(wi_key(wi)) != 0`
    pub(crate) fn by_pos_contains(&self, t: i32, wi: i32) -> bool {
        t_ok(t) && self.bp_contains(t as usize, wi_key(wi))
    }
    /// `dynamicObjectsByWorldPosIndex[t][wi_key(wi)]` (read)
    pub(crate) fn by_pos_get(&self, t: i32, wi: i32) -> Option<ObjRef> {
        if t_ok(t) {
            self.bp_get(t as usize, wi_key(wi))
        } else {
            None
        }
    }
    /// `dynamicObjectsByWorldPosIndex[t][wi_key(wi)] = o`
    pub(crate) fn by_pos_set(&self, t: i32, wi: i32, o: &ObjRef) {
        if t_ok(t) {
            self.bp_set(t as usize, wi_key(wi), o.clone());
        }
    }
    /// `-loadedCountOfObjectsOfType:`: `dynamicObjects[t].size() + dynamicObjectsToAdd[t].size()`.
    pub fn loaded_count_of_objects_of_type(&self, t: i32) -> i32 {
        if !t_ok(t) {
            return 0;
        }
        (self.do_len(t as usize) as i32).wrapping_add(self.ta_len(t as usize) as i32)
    }

    /// REGISTER + by-position index (REGISTER_POS): `toAdd[t][uid] = o; byPos[t][worldIndex(pos)] = o`.
    pub(crate) fn register_with_pos(&self, o: &ObjRef, t: i32, pos: IntPair) {
        self.register_to_add(o, t);
        let wi = self.world_index(pos);
        self.by_pos_set(t, wi, o);
    }
}

// =====================================================================================================
// DynamicWorldApi (object_model's view of the DynamicWorld)
// =====================================================================================================

impl DynamicWorldApi for DynamicWorld {
    fn get_next_dynamic_object_id(&self) -> u64 {
        DynamicWorld::get_next_dynamic_object_id(self)
    }
    fn dynamic_world_changed_at_pos_object_type(&self, pos: IntPair, object_type: i32) {
        DynamicWorld::dynamic_world_changed_at_pos_object_type(self, pos, object_type)
    }
    fn is_server(&self) -> bool {
        DynamicWorld::is_server(self)
    }
    fn blockheads(&self) -> Vec<ObjRef> {
        DynamicWorld::blockheads(self)
    }
    fn get_owner_name_for_object_owner_id(&self, owner_id: &str) -> Option<String> {
        DynamicWorld::get_owner_name_for_object_owner_id(self, owner_id)
    }
}

/// Coerce a strong `Rc<DynamicWorld>` to the `DynamicWorldRef` object_model constructors take.
pub(crate) fn dw_ref(dw: &Rc<DynamicWorld>) -> DynamicWorldRef {
    dw.clone()
}

// =====================================================================================================
// Lookups
// =====================================================================================================

macro_rules! typed_lookup {
    ($( $(#[$m:meta])* $name:ident => $t:expr ),* $(,)?) => {
        impl DynamicWorld {
            $(
                $(#[$m])*
                pub fn $name(&self, pos: IntPair) -> Option<ObjRef> {
                    self.object_of_type_at_pos($t, pos)
                }
            )*
        }
    };
}

impl DynamicWorld {
    /// `-objectOfType:atPos:`: `None` for a missing entry or one with `needsRemoved`.
    pub fn object_of_type_at_pos(&self, t: i32, pos: IntPair) -> Option<ObjRef> {
        let wi = self.world_index(pos);
        let o = self.by_pos_get(t, wi)?;
        if o.needs_removed() {
            None
        } else {
            Some(o)
        }
    }
}

typed_lookup! {
    /// `-torchAtPos:` (type 17).
    torch_at_pos => 17,
    /// `-eggAtPos:` (type 30).
    egg_at_pos => 30,
    /// `-paintingAtPos:` (type 52).
    painting_at_pos => 52,
    /// `-ladderAtPos:` (type 19).
    ladder_at_pos => 19,
    /// `-columnAtPos:` (type 53).
    column_at_pos => 53,
    /// `-stairsAtPos:` (type 54).
    stairs_at_pos => 54,
    /// `-wireAtPos:` (type 38).
    wire_at_pos => 38,
    /// `-elevatorShaftAtPos:` (type 56).
    elevator_shaft_at_pos => 56,
    /// `-elevatorMotorAtPos:` (type 55).
    elevator_motor_at_pos => 55,
    /// `-getRailAtPos:` (type 40).
    get_rail_at_pos => 40,
    /// `-windowAtPos:` (type 31).
    window_at_pos => 31,
    /// `-gatherBlockAtPos:` (type 26).
    gather_block_at_pos => 26,
}

impl DynamicWorld {
    /// `-paintingWithID:`: `dynamicObjects[52]` then `dynamicObjectsToAdd[52]`.
    pub fn painting_with_id(&self, id: u64) -> Option<ObjRef> {
        self.dot_get(DYNAMIC_OBJECT_TYPE_PAINTING, id).or_else(|| self.add_get(DYNAMIC_OBJECT_TYPE_PAINTING, id))
    }

    /// `-elevatorMotorForShaftAtPos:`. `inRange` always compares against the original `pos`.
    pub fn elevator_motor_for_shaft_at_pos(&self, pos: IntPair) -> Option<ObjRef> {
        let hooks = ops();
        let in_range = |m: &ObjRef| -> bool {
            m.pos().x == pos.x && hooks.elevator_motor_min_y(m) <= pos.y && pos.y <= hooks.elevator_motor_max_y(m)
        };
        for m in self.dot_snapshot(DYNAMIC_OBJECT_TYPE_ELEVATOR_MOTOR) {
            if in_range(&m) {
                return Some(m);
            }
        }
        for m in self.add_snapshot(DYNAMIC_OBJECT_TYPE_ELEVATOR_MOTOR) {
            if in_range(&m) {
                return Some(m);
            }
        }
        let world = self.world();
        let try_shaft = |p: IntPair| -> Option<ObjRef> {
            let shaft = self.elevator_shaft_at_pos(p)?;
            let mp = hooks.elevator_shaft_last_known_motor_pos(&shaft);
            let _ = tile_at_world_position_loaded(mp.x, mp.y, &world); // result ignored; forces the block load
            let m = self.elevator_motor_at_pos(mp)?;
            if in_range(&m) {
                Some(m)
            } else {
                None
            }
        };
        let t = tile_at_world_position_loaded(pos.x, pos.y, &world);
        if let Some(t) = t {
            if t.contents() == 0x67 {
                if let Some(r) = try_shaft(pos) {
                    return Some(r);
                }
            }
        }
        let up = IntPair { x: pos.x, y: pos.y.wrapping_add(1) };
        if let Some(t) = tile_at_world_position_loaded(up.x, up.y, &world) {
            if t.contents() == 0x67 {
                if let Some(r) = try_shaft(up) {
                    return Some(r);
                }
            } else if t.contents() == 0x68 {
                return self.elevator_motor_at_pos(up); // immediate return, even if nil
            }
        }
        let down = IntPair { x: pos.x, y: pos.y.wrapping_sub(1) };
        if let Some(t) = tile_at_world_position_loaded(down.x, down.y, &world) {
            if t.contents() == 0x67 {
                if let Some(r) = try_shaft(down) {
                    return Some(r);
                }
            } else if t.contents() == 0x68 {
                return self.elevator_motor_at_pos(down);
            }
        }
        None
    }

    // ------------------------------------------------------------------------------------ doors

    /// `-doorAtPos:`: two-tile doors are registered at their lower tile and found from the upper tile.
    pub fn door_at_pos(&self, pos: IntPair) -> Option<ObjRef> {
        if let Some(d) = self.object_of_type_at_pos(DYNAMIC_OBJECT_TYPE_DOOR, pos) {
            return Some(d);
        }
        let d2 =
            self.object_of_type_at_pos(DYNAMIC_OBJECT_TYPE_DOOR, IntPair { x: pos.x, y: pos.y.wrapping_sub(1) })?;
        let it = self.hooks().item_type(&d2);
        if it == 52 || it == 164 {
            Some(d2)
        } else {
            None
        }
    }

    /// `-doorCanBeUsedByPathUser:atPos:`.
    pub fn door_can_be_used_by_path_user_at_pos(&self, pu: &ObjRef, pos: IntPair) -> bool {
        if pu.as_blockhead().is_none() {
            return false;
        }
        let world = self.world();
        let macro_tiles = world.macro_tiles();
        let tile: Option<MacroTileRef> = macro_tile_at_world_postion(pos.x, pos.y, &macro_tiles, &world);
        if let (Some(tile), Some(h)) = (tile, world.ops_hooks()) {
            h.load_dynamic_objects_if_not_already_loaded_for_macro_tile_include_surface_blocks(&world, &tile, true);
        }
        match self.door_at_pos(pos) {
            Some(d) => ops().door_can_be_used_by_blockhead(&d, pu),
            None => false,
        }
    }

    /// `-doorIsOpenAtPos:`: TRUE when there is no door.
    pub fn door_is_open_at_pos(&self, pos: IntPair) -> bool {
        match self.door_at_pos(pos) {
            Some(d) => ops().door_is_open(&d),
            None => true,
        }
    }

    /// `-setDoorAtPos:toOpen:direction:`.
    pub fn set_door_at_pos_to_open_direction(&self, pos: IntPair, open: bool, dir: i32) {
        if let Some(d) = self.door_at_pos(pos) {
            ops().door_set_open_direction(&d, open, dir);
        }
    }

    /// `-posOfDoorsOtherBlockAtPos:`: `(-1,-1)` for no door / single-tile doors.
    pub fn pos_of_doors_other_block_at_pos(&self, pos: IntPair) -> IntPair {
        if let Some(this) = self.door_at_pos(pos) {
            let up = IntPair { x: pos.x, y: pos.y.wrapping_add(1) };
            if let Some(u) = self.door_at_pos(up) {
                if ptr_eq(&u, &this) {
                    return up;
                }
            }
            let dn = IntPair { x: pos.x, y: pos.y.wrapping_sub(1) };
            if let Some(d) = self.door_at_pos(dn) {
                if ptr_eq(&d, &this) {
                    return dn;
                }
            }
        }
        IntPair { x: -1, y: -1 }
    }

    // ------------------------------------------------------------------------------------ boats, cars

    /// `-checkForBoatUnderTap:`: `dynamicObjects[32]` only.
    pub fn check_for_boat_under_tap(&self, tap: Vector2) -> Option<ObjRef> {
        self.dot_snapshot(DYNAMIC_OBJECT_TYPE_BOAT).into_iter().find(|b| tap_within(b, tap))
    }

    /// `-boatWithID:`.
    pub fn boat_with_id(&self, id: u64) -> Option<ObjRef> {
        self.dot_get(DYNAMIC_OBJECT_TYPE_BOAT, id).or_else(|| self.add_get(DYNAMIC_OBJECT_TYPE_BOAT, id))
    }

    /// `-checkForTrainCarUnderTap:`: types 41..44 in order, `dynamicObjects` only.
    pub fn check_for_train_car_under_tap(&self, tap: Vector2) -> Option<ObjRef> {
        for t in TRAIN_CAR_DYNAMIC_OBJECT_TYPES {
            for o in self.dot_snapshot(t) {
                if tap_within(&o, tap) {
                    return Some(o);
                }
            }
        }
        None
    }

    /// `-trainCarWithID:`.
    pub fn train_car_with_id(&self, id: u64) -> Option<ObjRef> {
        for t in TRAIN_CAR_DYNAMIC_OBJECT_TYPES {
            if let Some(o) = self.dot_get(t, id) {
                return Some(o);
            }
            if let Some(o) = self.add_get(t, id) {
                return Some(o);
            }
        }
        None
    }

    // ------------------------------------------------------------------------------------ trees, plants, workbenches

    /// `-treeAtPos:`: first non-nil `objectOfType:atPos:` over the 11 tree types.
    pub fn tree_at_pos(&self, pos: IntPair) -> Option<ObjRef> {
        for t in TREE_DYNAMIC_OBJECT_TYPES {
            if let Some(o) = self.object_of_type_at_pos(t, pos) {
                return Some(o);
            }
        }
        None
    }

    /// `-getPlantAtPos:`: `dynamicObjects[t]` then `ToAdd[t]` per plant type; matches on x and the
    /// occupied vertical extent.
    pub fn get_plant_at_pos(&self, pos: IntPair) -> Option<ObjRef> {
        let hooks = ops();
        for t in PLANT_DYNAMIC_OBJECT_TYPES {
            for list in [self.dot_snapshot(t), self.add_snapshot(t)] {
                for p in list {
                    let pp = p.pos();
                    if pp.x == pos.x
                        && pos.y <= pp.y.wrapping_add(hooks.plant_number_of_occupied_tiles_above(&p))
                        && pp.y.wrapping_sub(hooks.plant_number_of_occupied_tiles_below(&p)) <= pos.y
                    {
                        return Some(p);
                    }
                }
            }
        }
        None
    }

    /// `-workbenchAtPos:`.
    pub fn workbench_at_pos(&self, pos: IntPair) -> Option<ObjRef> {
        if let Some(w) = self.object_of_type_at_pos(DYNAMIC_OBJECT_TYPE_WORKBENCH, pos) {
            return Some(w);
        }
        if let Some(b) =
            self.object_of_type_at_pos(DYNAMIC_OBJECT_TYPE_WORKBENCH, IntPair { x: pos.x, y: pos.y.wrapping_sub(1) })
        {
            if is_double_height(&b) {
                return Some(b);
            }
        }
        if let Some(a) =
            self.object_of_type_at_pos(DYNAMIC_OBJECT_TYPE_WORKBENCH, IntPair { x: pos.x, y: pos.y.wrapping_add(1) })
        {
            let ty = self.hooks().workbench_type(&a);
            if ty == 1 || ty == 13 {
                return Some(a);
            }
        }
        None
    }

    /// `-portal`: the workbench at `world.startPortalPos`.
    pub fn portal(&self) -> Option<ObjRef> {
        let p = self.world().start_portal_pos();
        self.workbench_at_pos(p)
    }

    /// `-workbenchHasBeenCrafted`.
    pub fn workbench_has_been_crafted(&self) -> bool {
        self.workbench_has_been_crafted.get()
    }

    // ------------------------------------------------------------------------------------ interaction objects

    /// `-interactionObjectAtPos:` with the exact probe order of the spec.
    pub fn interaction_object_at_pos(&self, pos: IntPair) -> Option<ObjRef> {
        let (x, y) = (pos.x, pos.y);
        let at = |t: i32, px: i32, py: i32| self.object_of_type_at_pos(t, IntPair { x: px, y: py });
        let world = self.world();
        let _ = tile_at_world_position_loaded(x, y, &world); // discarded; forces the block load
        let sign = DYNAMIC_OBJECT_TYPE_SIGN;
        let osign = DYNAMIC_OBJECT_TYPE_OWNERSHIP_SIGN;
        let station = DYNAMIC_OBJECT_TYPE_TRAIN_STATION;
        if let Some(o) = at(sign, x, y) {
            return Some(o);
        }
        if let Some(o) = at(osign, x, y) {
            return Some(o);
        }
        for t in [sign, osign] {
            if let Some(o) = at(t, x.wrapping_add(1), y) {
                if two_blocks_wide(&o) && o.flipped() {
                    return Some(o);
                }
            }
        }
        for t in [sign, osign] {
            if let Some(o) = at(t, x.wrapping_sub(1), y) {
                if two_blocks_wide(&o) && !o.flipped() {
                    return Some(o);
                }
            }
        }
        if let Some(o) = self.workbench_at_pos(pos) {
            return Some(o);
        }
        for dx in -1..=1 {
            for dy in 0..=1 {
                if let Some(o) = at(station, x.wrapping_add(dx), y.wrapping_add(dy)) {
                    return Some(o);
                }
            }
        }
        for t in INTERACTION_OBJECT_DYNAMIC_OBJECT_TYPES {
            if t == sign || t == osign || t == station {
                continue;
            }
            if let Some(o) = at(t, x, y) {
                return Some(o);
            }
            if let Some(o) = at(t, x.wrapping_add(1), y) {
                if two_blocks_wide(&o) && o.flipped() {
                    return Some(o);
                }
            }
            if let Some(o) = at(t, x.wrapping_sub(1), y) {
                if two_blocks_wide(&o) && !o.flipped() {
                    return Some(o);
                }
            }
            if let Some(o) = at(t, x, y.wrapping_sub(1)) {
                if is_double_height(&o) {
                    return Some(o);
                }
            }
        }
        at(DYNAMIC_OBJECT_TYPE_TRADE_PORTAL, x, y.wrapping_add(1))
    }

    /// `-interactionObjectWithID:`.
    pub fn interaction_object_with_id(&self, id: u64) -> Option<ObjRef> {
        for t in INTERACTION_OBJECT_DYNAMIC_OBJECT_TYPES {
            if let Some(o) = self.dot_get(t, id) {
                return Some(o);
            }
            if let Some(o) = self.add_get(t, id) {
                return Some(o);
            }
        }
        None
    }

    /// `-interactionObjectTypeForObjectAtPos:`.
    pub fn interaction_object_type_for_object_at_pos(&self, pos: IntPair) -> i32 {
        match self.interaction_object_at_pos(pos) {
            Some(o) => o.as_interaction_like().map_or(0, |i| i.interaction_object_type() as i32),
            None => 0,
        }
    }

    /// `-removeWorkbenchAtPos:removeBlockhead:`.
    pub fn remove_workbench_at_pos_remove_blockhead(&self, pos: IntPair, bh: Option<&ObjRef>) -> Option<ObjRef> {
        let w = self.workbench_at_pos(pos);
        self.remove_unless_in_use(w, bh)
    }

    /// `-removeInteractionObjectAtPos:removeBlockhead:`.
    pub fn remove_interaction_object_at_pos_remove_blockhead(
        &self,
        pos: IntPair,
        bh: Option<&ObjRef>,
    ) -> Option<ObjRef> {
        let o = self.interaction_object_at_pos(pos);
        self.remove_unless_in_use(o, bh)
    }

    fn remove_unless_in_use(&self, o: Option<ObjRef>, bh: Option<&ObjRef>) -> Option<ObjRef> {
        let o = o?;
        if o.is_in_use() {
            return None;
        }
        if let Some(il) = o.as_interaction_like() {
            il.remove(&o, bh);
        }
        Some(o)
    }

    // ------------------------------------------------------------------------------------ free blocks

    /// `-freeBlocksExistAtPos:`.
    pub fn free_blocks_exist_at_pos(&self, pos: IntPair) -> bool {
        let key = self.world_index_key(pos);
        self.free_blocks_by_position.borrow().get(&key).map_or(false, |s| !s.is_empty())
    }

    /// `-freeBlocksAtPos:`: ascending uid (DECISION synthesis); `None` when the filtered list is empty.
    pub fn free_blocks_at_pos(&self, pos: IntPair) -> Option<Vec<ObjRef>> {
        let key = self.world_index_key(pos);
        let snapshot: Vec<ObjRef> = {
            let m = self.free_blocks_by_position.borrow();
            match m.get(&key) {
                Some(s) if !s.is_empty() => s.values().cloned().collect(),
                _ => return None,
            }
        };
        let arr: Vec<ObjRef> = snapshot.into_iter().filter(|fb| !fb.needs_removed()).collect();
        if arr.is_empty() {
            None
        } else {
            Some(arr)
        }
    }

    /// `-getNextDynamicObjectID`: pre-increment, wrapping `u64`.
    pub fn get_next_dynamic_object_id(&self) -> u64 {
        let v = self.dynamic_object_id_count.get().wrapping_add(1);
        self.dynamic_object_id_count.set(v);
        v
    }

    // ------------------------------------------------------------------------------------ taps, NPCs

    /// `-checkForHarmableDynamicObjectUnderTap:ignoreLocalBlockheads:ignoreAllBlockheads:`.
    pub fn check_for_harmable_dynamic_object_under_tap_ignore_local_blockheads_ignore_all_blockheads(
        &self,
        tap: Vector2,
        ignore_local: bool,
        ignore_all: bool,
    ) -> Option<ObjRef> {
        let mut found: Vec<ObjRef> = Vec::new();
        for t in NPC_DYNAMIC_OBJECT_TYPES {
            for npc in self.dot_snapshot(t) {
                if tap_within(&npc, tap) {
                    found.push(npc);
                }
            }
        }
        if found.is_empty() {
            if !ignore_all {
                let list = if ignore_local { self.net_blockheads() } else { self.all_blockheads_including_net() };
                return list.into_iter().find(|bh| tap_within(bh, tap));
            }
            return None;
        }
        if found.len() == 1 {
            return found.last().cloned();
        }
        let hooks = ops();
        let ab = self.active_blockhead();
        let item_type = ab.as_ref().map_or(0, |b| hooks.blockhead_current_item_type(b));
        let mut inspect: Option<ObjRef> = None;
        for npc in &found {
            let ia = match &ab {
                Some(b) => hooks.blockhead_goal_interaction_for_npc_chase(b, npc, item_type),
                None => 0,
            };
            if ia == 25 {
                if let Some(b) = &ab {
                    if hooks.npc_can_be_fed_by_blockhead(npc, b) {
                        return Some(npc.clone());
                    }
                }
            }
            if ia == 26 {
                inspect = Some(npc.clone());
            }
        }
        inspect.or_else(|| found.last().cloned())
    }

    /// `-npcWithID:`.
    pub fn npc_with_id(&self, id: u64) -> Option<ObjRef> {
        for t in NPC_DYNAMIC_OBJECT_TYPES {
            if let Some(o) = self.dot_get(t, id) {
                return Some(o);
            }
            if let Some(o) = self.add_get(t, id) {
                return Some(o);
            }
        }
        None
    }

    /// `-harmableDynamicObjectWithID:`.
    pub fn harmable_dynamic_object_with_id(&self, id: u64) -> Option<ObjRef> {
        if let Some(n) = self.npc_with_id(id) {
            return Some(n);
        }
        self.all_blockheads_including_net().into_iter().find(|b| b.unique_id() == id)
    }

    /// `-npcExistsAtPos:ignoreNPC:`: `dynamicObjects` only.
    pub fn npc_exists_at_pos_ignore_npc(&self, pos: IntPair, ignore: Option<&ObjRef>) -> bool {
        for t in NPC_DYNAMIC_OBJECT_TYPES {
            for npc in self.dot_snapshot(t) {
                let same = match ignore {
                    Some(i) => ptr_eq(i, &npc),
                    None => false,
                };
                if !same {
                    let p = npc.pos();
                    if p.x == pos.x && p.y == pos.y {
                        return true;
                    }
                }
            }
        }
        false
    }

    /// `-tooManyNPCsToSpawnMoreNearPos:`: true on the second NPC within |dx| < 2, |dy| <= 1.
    pub fn too_many_npcs_to_spawn_more_near_pos(&self, pos: IntPair) -> bool {
        let w = self.world().world_width_macro().wrapping_mul(32);
        let mut count = 0;
        for t in NPC_DYNAMIC_OBJECT_TYPES {
            for npc in self.dot_snapshot(t) {
                let np = npc.pos();
                let dx = wrap_dx(np.x.wrapping_sub(pos.x), w);
                if dx.wrapping_abs() < 2 && pos.y.wrapping_sub(1) <= np.y && np.y <= pos.y.wrapping_add(1) {
                    count += 1;
                    if count > 1 {
                        return true;
                    }
                }
            }
        }
        false
    }

    /// `-npcCloseEnoughToBreedWithNPC:`: same type only, `dynamicObjects` only.
    pub fn npc_close_enough_to_breed_with_npc(&self, npc: &ObjRef) -> Option<ObjRef> {
        let t = npc.object_type();
        let w = self.world().world_width_macro().wrapping_mul(32);
        let hooks = ops();
        let np = npc.pos();
        for other in self.dot_snapshot(t) {
            if ptr_eq(&other, npc) || !hooks.npc_can_mate(&other) {
                continue;
            }
            let op = other.pos();
            let xd = wrap_dx(op.x.wrapping_sub(np.x), w);
            let yd = op.y.wrapping_sub(np.y);
            if xd.wrapping_abs() < 2 && yd.wrapping_abs() < 2 {
                return Some(other);
            }
        }
        None
    }

    // ------------------------------------------------------------------------------------ blockhead queries

    /// The three lists in search order: `blockheads`, `netBlockheads`, `netBlockheadsWithDisconnectedClients`.
    fn three_blockhead_lists(&self) -> Vec<ObjRef> {
        let mut v = self.blockheads.borrow().clone();
        v.extend(self.net_blockheads.borrow().iter().cloned());
        v.extend(self.net_blockheads_with_disconnected_clients.borrow().iter().cloned());
        v
    }

    fn blockhead_at_pos_impl(&self, pos: IntPair, ignore: Option<&ObjRef>) -> Option<ObjRef> {
        let hooks = ops();
        for bh in self.three_blockhead_lists() {
            if let Some(i) = ignore {
                if ptr_eq(i, &bh) {
                    continue;
                }
            }
            let p = bh.pos();
            if p.x == pos.x {
                if p.y == pos.y {
                    return Some(bh);
                }
                if p.y == pos.y.wrapping_sub(1) && !hooks.blockhead_crouching(&bh) {
                    return Some(bh);
                }
            }
        }
        None
    }

    /// `-blockheadOccupiesTileAtPos:ignoreBlockhead:`.
    pub fn blockhead_occupies_tile_at_pos_ignore_blockhead(&self, pos: IntPair, ignore: Option<&ObjRef>) -> bool {
        self.blockhead_at_pos_impl(pos, ignore).is_some()
    }

    /// `-blockheadAtPos:`.
    pub fn blockhead_at_pos(&self, pos: IntPair) -> Option<ObjRef> {
        self.blockhead_at_pos_impl(pos, None)
    }

    // ------------------------------------------------------------------------------------ role flags, lists

    /// `-isClient`: `client != nil`, always false on the dedicated server.
    pub fn is_client(&self) -> bool {
        false
    }
    /// `-isServer`: `server != nil`.
    pub fn is_server(&self) -> bool {
        self.has_server()
    }
    /// `-localNetID`: server's local player id, else the literal `"server"`.
    pub fn local_net_id(&self) -> Option<String> {
        if self.has_server() {
            Some(self.hooks().local_net_id())
        } else {
            Some("server".to_string())
        }
    }
    /// `-allBlockheadsIncludingNet`.
    pub fn all_blockheads_including_net(&self) -> Vec<ObjRef> {
        self.three_blockhead_lists()
    }
    /// `-netBlockheads`: net + disconnected concatenated.
    pub fn net_blockheads(&self) -> Vec<ObjRef> {
        let mut v = self.net_blockheads.borrow().clone();
        v.extend(self.net_blockheads_with_disconnected_clients.borrow().iter().cloned());
        v
    }
    /// `-localAndDisconnectedClientBlockheads`.
    pub fn local_and_disconnected_client_blockheads(&self) -> Vec<ObjRef> {
        // client == nil on the server
        let dis = self.net_blockheads_with_disconnected_clients.borrow();
        let mut v = self.blockheads.borrow().clone();
        if !dis.is_empty() {
            v.extend(dis.iter().cloned());
        }
        v
    }
    /// `-blockheads`: the local list.
    pub fn blockheads(&self) -> Vec<ObjRef> {
        self.blockheads.borrow().clone()
    }
    /// `-blockheadWithUniqueID:`: LOCAL `blockheads` only.
    pub fn blockhead_with_unique_id(&self, uid: u64) -> Option<ObjRef> {
        self.blockheads.borrow().iter().find(|b| b.unique_id() == uid).cloned()
    }
    /// `-blockheadWithIDIncludingNet:`.
    pub fn blockhead_with_id_including_net(&self, uid: u64) -> Option<ObjRef> {
        self.all_blockheads_including_net().into_iter().find(|b| b.unique_id() == uid)
    }
    /// `-ridableObjectWithID:`: `dynamicObjects` of 28, 63, 13, 51, 39, 36, 32, then `trainCarWithID:`.
    pub fn ridable_object_with_id(&self, uid: u64) -> Option<ObjRef> {
        for t in [28, 63, 13, 51, 39, 36, 32] {
            if let Some(o) = self.dot_get(t, uid) {
                return Some(o);
            }
        }
        self.train_car_with_id(uid)
    }
    /// `-connectionToServerLost`: empty.
    pub fn connection_to_server_lost(&self) {}

    // ------------------------------------------------------------------------------------ tree life

    /// `-getTreeLifeFractionForPos:`, server branch (`client == nil`): `(life, treeX, treeY, 0)`.
    /// One `rand()` only when `trees` is non-empty.
    pub fn get_tree_life_fraction_for_pos(&self, pos: IntPair) -> [f32; 4] {
        let hooks = ops();
        let mut total: f32 = 0.0;
        let mut trees: Vec<ObjRef> = Vec::new();
        for t in TREE_DYNAMIC_OBJECT_TYPES {
            for tree in self.dot_snapshot(t) {
                if hooks.tree_type(&tree) == 5 {
                    continue; // cactus
                }
                let tp = tree.pos();
                let fx = 1.0f32 - (pos.x as f32 - tp.x as f32).abs() / 32.0f32;
                let fy = 1.0f32 - (pos.y as f32 - tp.y as f32).abs() / 32.0f32;
                if fx > 0.0 && fy > 0.0 {
                    total += (fx * fy) * (hooks.tree_height(&tree) as f32 / 32.0f32);
                    trees.push(tree);
                }
            }
        }
        let (mut x, mut y) = (0.0f32, 0.0f32);
        if !trees.is_empty() {
            let r = crate::rng::glibc_rand::rand();
            let idx = ((r as i64 as u64) % (trees.len() as u64)) as usize;
            let t = &trees[idx];
            let h = hooks.tree_height(t) / 2;
            let tp = t.pos();
            x = tp.x as f32;
            y = tp.y.wrapping_add(h) as f32;
        }
        [total, x, y, 0.0]
    }

    // ------------------------------------------------------------------------------------ accessors

    /// `-portalPositions` (clone of the index set).
    pub fn portal_positions(&self) -> IndexSet {
        self.portal_positions.borrow().clone()
    }

    /// `-portalIsBeingRemovedAtPos:`: `removeIndex:(NSUInteger)(int64)worldIndex`.
    pub fn portal_is_being_removed_at_pos(&self, pos: IntPair) {
        let wi = self.world_index(pos);
        self.portal_positions.borrow_mut().remove_index(wi as i64 as u64);
    }

    /// `-hasLightsToAdd`.
    pub fn has_lights_to_add(&self) -> bool {
        for i in 0..=0x40i32 {
            if object_type_may_have_artifical_light(i) && self.ta_len(i as usize) != 0 {
                return true;
            }
        }
        false
    }

    /// `-getOwnerNameForObjectOwnerID:`: nil when `server == nil`.
    pub fn get_owner_name_for_object_owner_id(&self, id: &str) -> Option<String> {
        if !self.has_server() {
            return None;
        }
        ops().server_player_name_for_player_with_id_including_old_players(id)
    }

    /// `-playerIsBannedWithID:`.
    pub fn player_is_banned_with_id(&self, id: &str) -> bool {
        if !self.has_server() {
            return false;
        }
        ops().server_player_is_banned_with_id(id)
    }

    /// `-findAndSubtractAllPowerUpTo:forUser:`.
    pub fn find_and_subtract_all_power_up_to_for_user(&self, required: u16, user: &ObjRef) -> i32 {
        self.wire_path_creator.find_and_subtract_all_power_up_to_for_user(required, Some(user))
    }

    /// `-pathUsers`: local + disconnected blockheads, plus cave trolls with `controlIsLocal`
    /// (`dynamicObjects[39]` only).
    pub fn path_users(&self) -> Vec<ObjRef> {
        let hooks = ops();
        let npc_users: Vec<ObjRef> = self
            .dot_snapshot(DYNAMIC_OBJECT_TYPE_NPC_CAVETROLL)
            .into_iter()
            .filter(|t| hooks.cave_troll_control_is_local(t))
            .collect();
        let mut base = self.local_and_disconnected_client_blockheads();
        if !npc_users.is_empty() {
            base.extend(npc_users);
        }
        base
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dynworld::state::testutil::*;
    use std::cell::RefCell;
    use std::rc::Rc;

    #[test]
    fn default_troll_control_hook_downcasts_and_rejects_non_trolls() {
        let o = mk(39, 1, p(0, 0));
        assert!(!NullOpsHooks.cave_troll_control_is_local(&o));
    }

    fn p(x: i32, y: i32) -> IntPair {
        IntPair { x, y }
    }
    fn mk(ty: i32, id: u64, pos: IntPair) -> ObjRef {
        let o: ObjRef = Obj::new(ty, id, Rc::new(RefCell::new(vec![])));
        o.base().borrow_mut().pos = pos;
        o
    }

    /// id counter.
    #[test]
    fn next_id_and_counts() {
        let d = dw("lk_id", 1);
        d.dynamic_object_id_count.set(109);
        assert_eq!(d.get_next_dynamic_object_id(), 110);
        assert_eq!(d.get_next_dynamic_object_id(), 111);
        assert_eq!(d.dynamic_object_id_count.get(), 111);
        d.dynamic_object_id_count.set(u64::MAX);
        assert_eq!(d.get_next_dynamic_object_id(), 0);
        let o = mk(17, 5, p(1, 1));
        d.do_set(17, 5, o.clone());
        let o2 = mk(17, 6, p(1, 1));
        d.register_to_add(&o2, 17);
        assert_eq!(d.loaded_count_of_objects_of_type(17), 2);
    }

    /// the by-position key of an out-of-range row is u64::MAX.
    #[test]
    fn by_pos_minus_one_key() {
        let d = dw("lk_key", 1);
        let o = mk(17, 5, p(1, 2000));
        d.register_with_pos(&o, 17, p(1, 2000));
        assert!(d.bp_contains(17, u64::MAX));
        assert!(d.object_of_type_at_pos(17, p(7, 5000)).is_some()); // any pos with wi == -1 finds it
        assert_eq!(wi_key(-1), u64::MAX);
        o.base().borrow_mut().needs_removed = true;
        assert!(d.object_of_type_at_pos(17, p(1, 2000)).is_none());
    }

    #[test]
    fn free_blocks_lookup() {
        let d = dw("lk_fb", 1);
        let a = mk(14, 9, p(3, 3));
        let b = mk(14, 4, p(3, 3));
        let key = d.world_index_key(p(3, 3));
        d.free_blocks_insert(key, &a);
        d.free_blocks_insert(key, &b);
        let l = d.free_blocks_at_pos(p(3, 3)).unwrap();
        assert_eq!(l.iter().map(|o| o.unique_id()).collect::<Vec<_>>(), vec![4, 9]);
        assert!(d.free_blocks_exist_at_pos(p(3, 3)));
        a.base().borrow_mut().needs_removed = true;
        b.base().borrow_mut().needs_removed = true;
        assert!(d.free_blocks_at_pos(p(3, 3)).is_none());
        assert!(d.free_blocks_exist_at_pos(p(3, 3)));
        assert!(d.free_blocks_at_pos(p(8, 8)).is_none());
    }

    /// crowding wraps at the world seam; true only for >= 2 NPCs.
    #[test]
    fn crowding_wrap() {
        let d = dw("lk_crowd", 1); // W = 32
        let n1 = mk(13, 1, p(31, 10));
        let n2 = mk(13, 2, p(0, 10));
        d.do_set(13, 1, n1);
        assert!(!d.too_many_npcs_to_spawn_more_near_pos(p(0, 10)));
        d.do_set(13, 2, n2);
        // x = 1 and x = W-1 = 31 are neighbours: (31 -> d = 30 >= 16 -> -2?) check explicit values
        assert_eq!(wrap_dx(31 - 0, 32), -1);
        assert!(d.too_many_npcs_to_spawn_more_near_pos(p(0, 10)));
        assert!(!d.too_many_npcs_to_spawn_more_near_pos(p(0, 20)));
        assert_eq!(wrap_dx(0 - 31, 32), 1);
        assert_eq!(wrap_dx(-16, 32), -16 + 0);
    }

    #[test]
    fn blockhead_lists() {
        let d = dw("lk_bh", 1);
        let a = mk(24, 1, p(5, 5));
        let b = mk(24, 2, p(6, 6));
        let c = mk(24, 3, p(7, 7));
        d.blockheads.borrow_mut().push(a.clone());
        d.net_blockheads.borrow_mut().push(b.clone());
        d.net_blockheads_with_disconnected_clients.borrow_mut().push(c.clone());
        assert_eq!(d.all_blockheads_including_net().len(), 3);
        assert_eq!(d.net_blockheads().len(), 2);
        assert_eq!(d.local_and_disconnected_client_blockheads().len(), 2);
        assert!(d.blockhead_with_unique_id(2).is_none());
        assert!(d.blockhead_with_id_including_net(2).is_some());
        assert!(d.blockhead_with_unique_id(1).is_some());
        // standing blockhead occupies its tile and the one above (crouching hook defaults to false)
        assert!(d.blockhead_at_pos(p(5, 6)).is_some());
        assert!(d.blockhead_occupies_tile_at_pos_ignore_blockhead(p(5, 5), None));
        assert!(!d.blockhead_occupies_tile_at_pos_ignore_blockhead(p(5, 5), Some(&a)));
        assert!(!d.blockhead_occupies_tile_at_pos_ignore_blockhead(p(5, 7), None));
    }

    struct DoorHk(u32);
    impl crate::dynworld::state::DwHooks for DoorHk {
        fn item_type(&self, _o: &ObjRef) -> i32 {
            self.0 as i32
        }
    }

    /// a two-tile door is found from its upper tile; gates are single-tile.
    #[test]
    fn door_lookups() {
        let d = dw("lk_door", 1);
        d.set_hooks(Rc::new(DoorHk(52)));
        let door = mk(20, 3, p(5, 5));
        d.register_with_pos(&door, 20, p(5, 5));
        assert!(Rc::ptr_eq(&d.door_at_pos(p(5, 6)).unwrap(), &door));
        assert!(d.door_at_pos(p(5, 7)).is_none());
        let other = d.pos_of_doors_other_block_at_pos(p(5, 5));
        assert_eq!((other.x, other.y), (5, 6));
        let other = d.pos_of_doors_other_block_at_pos(p(5, 6));
        assert_eq!((other.x, other.y), (5, 5));
        // gate (304): the y-1 path never matches
        d.set_hooks(Rc::new(DoorHk(304)));
        assert!(d.door_at_pos(p(5, 6)).is_none());
        assert_eq!(d.pos_of_doors_other_block_at_pos(p(5, 5)).x, -1);
    }

    #[test]
    fn misc_flags() {
        let d = dw("lk_flags", 1);
        assert!(!d.is_client());
        assert!(!d.is_server());
        assert_eq!(d.local_net_id().as_deref(), Some("server"));
        assert!(d.door_is_open_at_pos(p(1, 1))); // no door
        assert_eq!(d.pos_of_doors_other_block_at_pos(p(1, 1)).x, -1);
        d.portal_positions.borrow_mut().add_index(7);
        d.portal_positions.borrow_mut().add_index(u64::MAX);
        d.portal_is_being_removed_at_pos(p(0, 5000)); // wi == -1
        assert_eq!(d.portal_positions().count(), 1);
    }
}
