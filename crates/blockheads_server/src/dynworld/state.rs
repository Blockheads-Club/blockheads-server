//! `DynamicWorld` data model: every ivar of `@interface DynamicWorld`, the designated initializer, the
//! `setServer:serverClients:` setter, `dealloc`, the 65-way table helpers and the seams (traits) through which
//! this module reaches layers that do not exist yet (ServerClient, BHServer, Blockhead, FreeBlock, registry,
//! worldgen, the `dynamic_world_ops` half of the class).
//!
//!
//! ## Seams
//! At the time this module was written `server`, `blockhead`, `physics_objects`, `worldgen`, the registry and the
//! `dynamic_world_ops` files are empty skeletons. Every call into them goes through [`DwHooks`] (all methods have
//! inert defaults), [`DwServerClient`], [`DwServer`], [`DwWorldGen`] and [`DwNoise`]. The integrator installs the
//! real implementation with [`DynamicWorld::set_hooks`] and friends. No behaviour of this module is hidden behind a
//! hook except the calls the contract (section 4) lists as belonging to other modules.

use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, HashSet, VecDeque};
use std::rc::{Rc, Weak};

use crate::foundation::gsdict::GsDict;
use crate::foundation::indexset::IndexSet;
use crate::items::inventory_item::InventoryItem;
use crate::objects::handle::{CacheRef, ObjRef};
use crate::plist::value::{PlistDict, PlistValue};
use crate::storage::lmdb::Database;
use crate::types::structs::IntPair;
use crate::world::state::{MacroTileRef, PhysicalBlockRef, TileRef, World};
use crate::world::tiles::world_index_at_world_pos;

/// `MAX_DYNAMIC_OBJECT_TYPES`: types `0..=64`.
pub const MAX_DYNAMIC_OBJECT_TYPES: usize = 65;
/// `std::map<uint64, DynamicObject*>`: ascending key order is observable.
pub type ObjMap = BTreeMap<u64, ObjRef>;
/// `std::vector<intpair>`; set semantics are enforced by `dynamic_world_ops` (dirty.rs).
pub type PosVec = Vec<IntPair>;
/// One `NSData` net blob.
pub type NetBlob = Vec<u8>;
/// The `World` handle.
pub type WorldRef = Rc<World>;
/// `BHServer` seen through [`DwServer`].
pub type ServerRef = Rc<dyn DwServer>;
/// `ServerClient` seen through [`DwServerClient`].
pub type ServerClientRef = Rc<dyn DwServerClient>;
/// NSDictionary clientID -> ServerClient (shared with `World.serverClients`). DEFINED HERE (contract section 2).
pub type ServerClientMap = Rc<RefCell<GsDict<String, ServerClientRef>>>;

/// The `ServerClient` calls made by this module.
pub trait DwServerClient {
    /// `-connected`
    fn connected(&self) -> bool;
    /// `-clientID`
    fn client_id(&self) -> String;
    /// `-lightBlockIndex`
    fn light_block_index(&self) -> i32;
    /// `-blockIsWired:`
    fn block_is_wired(&self, macro_index: i32) -> bool;
    /// `-dynamicObjectIsWired:`
    fn dynamic_object_is_wired(&self, unique_id: u64) -> bool;
    /// `-wireDynamicObject:`
    fn wire_dynamic_object(&self, unique_id: u64);
    /// `-unwireDynamicObject:`
    fn unwire_dynamic_object(&self, unique_id: u64);
    /// `-addCreationObjectDataToSend:ofType:objectID:`
    fn add_creation_object_data_to_send_of_type_object_id(&self, data: &NetBlob, object_type: u8, object_id: u64);
    /// `-addCreationDataUpdateObjectDataToSend:ofType:`
    fn add_creation_data_update_object_data_to_send_of_type(&self, data: &NetBlob, object_type: u8);
    /// `-addUpdateObjectDataToSend:ofType:reliable:`
    fn add_update_object_data_to_send_of_type_reliable(&self, data: &NetBlob, object_type: u8, reliable: bool);
    /// `-addRemovalObjectDataToSend:ofType:objectID:`
    fn add_removal_object_data_to_send_of_type_object_id(&self, data: &NetBlob, object_type: u8, object_id: u64);
    /// `-sendAllObjectData`
    fn send_all_object_data(&self);
    /// `-worldIndicesContainingTamedAnimals`: every type 0..64 has a set, `len == 65`.
    fn world_indices_containing_tamed_animals(&self) -> &RefCell<Vec<HashSet<u32>>>;
}

/// The `BHServer` calls made by this module.
pub trait DwServer {
    /// `-playerInfoForPeerID:` (key `"alias"` is read)
    fn player_info_for_peer_id(&self, peer_id: &str) -> Option<PlistValue>;
    /// `-playerIsAdminWithID:`
    fn player_is_admin_with_id(&self, client_id: &str) -> bool;
    /// `-playerUpdate`: the clientID pending the delayed anti-cheat kick.
    fn player_update(&self) -> Option<String>;
    /// `-notifyPlayersChanged`
    fn notify_players_changed(&self);
}

/// The `WorldTileLoader` calls made by this module.
pub trait DwWorldGen {
    /// `treePositions`: per-world-x-column array, SHARED and mutated (`arr[x] = 0` after a spawn).
    fn tree_positions(&self) -> Rc<RefCell<Vec<i32>>>;
    /// `npcPositions` (shared, mutated)
    fn npc_positions(&self) -> Rc<RefCell<Vec<i32>>>;
    /// `plantPositions` (shared, mutated)
    fn plant_positions(&self) -> Rc<RefCell<Vec<i32>>>;
    /// `maxOfRockAndDirtHeightForX:`
    fn max_of_rock_and_dirt_height_for_x(&self, x: i32) -> i32;
    /// `lakeHeightForX:`
    fn lake_height_for_x(&self, x: i32) -> i32;
}

/// `NoiseFunction -getX:Y:octaves:`.
pub trait DwNoise {
    fn get_x_y_octaves(&self, x: f64, y: f64, octaves: i32) -> f64;
}

/// Which constructor shape `loadDynamicObjectsOfType` uses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SaveLoadKind {
    /// `initWithWorld:dynamicWorld:saveDict:cache:treeDensityNoiseFunction:seasonOffsetNoiseFunction:` (trees and plants)
    TreeOrPlant,
    /// `Shark initWithWorld:dynamicWorld:saveDict:cache:`
    Shark,
    /// `FreightCar initWithWorld:dynamicWorld:saveDict:chestSaveDict:cache:`
    FreightCar,
    /// `Workbench initWithWorld:dynamicWorld:saveDict:cache:`
    Workbench,
    /// `classForInteractionObjectType(ito)` then `initWithWorld:dynamicWorld:saveDict:cache:`
    InteractionObject(i32),
    /// `classForDynamicObjectType(t)` then `initWithWorld:dynamicWorld:saveDict:cache:`
    Generic,
}

/// Arguments of `createFreeBlockAtPosition:ofType:dataA:dataB:subItems:dynamicObjectSaveDict:hovers:playSound:priorityBlockhead:`.
pub struct FreeBlockInit<'a> {
    pub position: IntPair,
    pub item_type: u32,
    pub data_a: u16,
    pub data_b: u16,
    pub sub_items: Option<Vec<Vec<InventoryItem>>>,
    pub dynamic_object_save_dict: Option<PlistValue>,
    pub hovers: bool,
    pub priority_blockhead: Option<&'a ObjRef>,
}

crate::hooks::hook_trait! {
/// Every call this module makes into modules that are not part of `dynamic_world_core`. All methods have inert
/// defaults so that a partial integration (or a unit-test mock) overrides only what it needs.
///  Note:.
#[allow(unused_variables)]
pub trait DwHooks, stub DwHooksStub {
    // ---- World (world_core / world_ops) ----
    /// `-[World savePhysicalBlockForMacroTile:sendReliably:dontSend:onlySaveIfClientsNeedIt:]`
    fn save_physical_block_for_macro_tile(
        &self,
        mt: &MacroTileRef,
        send_reliably: bool,
        dont_send: bool,
        only_save_if_clients_need_it: bool,
    ) -> bool {
        false
    }
    /// `-[World saveLightBlockForClientLightBlockIndex:physicalBlock:sendNow:]`
    fn save_light_block_for_client_light_block_index(&self, index: i32, pb: Option<PhysicalBlockRef>, send_now: bool) {}
    /// `-[World fullyLoadIfNeededAroundPos:clientLightBlockIndex:forBlockhead:]` (`0xFFFFFFFF` = none)
    fn fully_load_if_needed_around_pos(&self, pos: IntPair, client_light_block_index: u32, blockhead: &ObjRef) {}
    /// `-[World updatePhysicalBlockToLatestVersion:]`
    fn update_physical_block_to_latest_version(&self, pb: PhysicalBlockRef) {}
    /// `-[World placeWorkbenchOfType:1 atPos:saveDict:nil placedByClient:nil placedByBlockhead:nil placedByClientName:nil]`
    fn place_basic_portal_workbench(&self, pos: IntPair) {}
    /// `-[World tileIsLitForClient:atPos:tile:]`
    fn tile_is_lit_for_client(&self, client_id: Option<&str>, pos: IntPair, tile: TileRef) -> bool {
        false
    }

    // ---- dynamic_world_ops (same struct, other impl blocks) ----
    /// `-dynamicWorldChangedAtPos:objectType:`
    fn dynamic_world_changed_at_pos_object_type(&self, pos: IntPair, object_type: i32) {}
    /// `-lightChangedAtMacroPos:sendReliably:`
    fn light_changed_at_macro_pos_send_reliably(&self, macro_pos: IntPair, send_reliably: bool) {}
    /// `-blockheadWithIDIncludingNet:`
    fn blockhead_with_id_including_net(&self, unique_id: u64) -> Option<ObjRef> {
        None
    }
    /// `-blockheadWillBeUnloaded:`
    fn blockhead_will_be_unloaded(&self, blockhead: &ObjRef) {}
    /// `-localNetID`: on the dedicated server the md5 persistent id of `"server"`.
    #[provided]
    fn local_net_id(&self) -> String {
        "cf1e8c14e54505f60aa10ceb8d5d8ab3".to_string()
    }
    /// `-checkAndRestorePoleItems:`
    fn check_and_restore_pole_items(&self, dt: f32) {}
    /// `-loadTreeAtPosition:type:maxHeight:growthRate:adultTree:adultMaxAge:`
    fn load_tree_at_position(
        &self,
        pos: IntPair,
        tree_type: i32,
        max_height: i32,
        growth_rate: i32,
        adult_tree: bool,
        adult_max_age: f32,
    ) {
    }
    /// `-loadPlantAtPosition:type:maxAgeGene:growthRateGene:adult:`
    fn load_plant_at_position(
        &self,
        pos: IntPair,
        plant_type: i32,
        max_age_gene: i32,
        growth_rate_gene: i32,
        adult: bool,
    ) {
    }
    /// `-loadNPCAtPosition:type:saveDict:nil isAdult:wasPlaced:placedByClient:nil`
    fn load_npc_at_position(&self, pos: IntPair, npc_type: i32, is_adult: bool, was_placed: bool) {}
    /// `-addTorchAtPos:ofType:dataA:0 dataB:0 saveDict:nil placedByClient:nil`
    fn add_torch_at_pos_of_type(&self, pos: IntPair, torch_type: i32) {}
    /// `-createTreasureChestOrTrollAtTile:atPos:loadTroll:loadTreasure:`
    fn create_treasure_chest_or_troll_at_tile(
        &self,
        tile: TileRef,
        pos: IntPair,
        load_troll: bool,
        load_treasure: bool,
    ) {
    }
    /// `-loadGlowBlockIfNeededAtPos:tile:`
    fn load_glow_block_if_needed_at_pos_tile(&self, pos: IntPair, tile: TileRef) {}
    /// `-loadSurfaceBlockAtPos:`
    fn load_surface_block_at_pos(&self, pos: IntPair) {}
    /// `-loadSnowSurfaceBlockAtPos:loadSnow:`
    fn load_snow_surface_block_at_pos_load_snow(&self, pos: IntPair, load_snow: bool) {}
    /// `-workbenchAtPos:`
    fn workbench_at_pos(&self, pos: IntPair) -> Option<ObjRef> {
        None
    }
    /// `-interactionObjectAtPos:`
    fn interaction_object_at_pos(&self, pos: IntPair) -> Option<ObjRef> {
        None
    }
    /// `-paintingAtPos:` / `columnAtPos:` / `stairsAtPos:` / `torchAtPos:` / `wireAtPos:` selected by `which`
    /// (`'p'`, `'c'`, `'s'`, `'t'`, `'w'`).
    fn object_at_pos_for_item(&self, which: char, pos: IntPair) -> Option<ObjRef> {
        None
    }
    /// `-removeDoorAtPos:`
    fn remove_door_at_pos(&self, pos: IntPair) -> Option<ObjRef> {
        None
    }
    /// `-removeWindowAtPos:`
    fn remove_window_at_pos(&self, pos: IntPair) -> Option<ObjRef> {
        None
    }

    // ---- registry / concrete object classes ----
    /// Allocate + init one object from a save dict (`SaveLoadKind` selects the constructor). `None` = init returned nil.
    fn construct_from_save_dict(
        &self,
        kind: SaveLoadKind,
        object_type: i32,
        save_dict: &PlistDict,
        chest_save_dict: Option<&PlistDict>,
    ) -> Option<ObjRef> {
        None
    }
    /// Allocate + init one object from a net blob (`initWithWorld:dynamicWorld:cache:netData:`).
    /// `interaction_type` is `Some` for interaction objects (read from the blob by the caller).
    fn construct_from_net_data(
        &self,
        object_type: i32,
        interaction_type: Option<i32>,
        net_data: &[u8],
    ) -> Option<ObjRef> {
        None
    }
    /// `Workbench -type`
    fn workbench_type(&self, wb: &ObjRef) -> i32 {
        0
    }
    /// `Workbench -level`
    fn workbench_level(&self, wb: &ObjRef) -> i32 {
        0
    }
    /// `Workbench -setLevelSilently:`
    fn workbench_set_level_silently(&self, wb: &ObjRef, level: i32) {}
    /// `trees::tree::tree_update_all_owned_tiles_to_new_id_size(&me)`
    fn tree_update_all_owned_tiles_to_new_id_size(&self, tree: &ObjRef) {}
    /// `-updateRain:dt:` (type 29)
    fn update_rain_dt(&self, obj: &ObjRef, rain: f32, dt: f32) {}
    /// `-itemType` (door gate test, doors/windows/free blocks)
    fn item_type(&self, obj: &ObjRef) -> i32 {
        0
    }
    /// `InteractionObject -destroyItemType`
    fn destroy_item_type(&self, obj: &ObjRef) -> i32 {
        0
    }
    /// `Chest -chestType`
    fn chest_type(&self, chest: &ObjRef) -> i32 {
        0
    }
    /// `[chest inventoryDataAllowingEmpty:YES]`
    fn chest_inventory_data_allowing_empty(&self, chest: &ObjRef) -> Option<Vec<u8>> {
        None
    }
    /// `[chest getSaveDictIncludingInventory:NO]`
    fn chest_save_dict_excluding_inventory(&self, chest: &ObjRef) -> Option<PlistValue> {
        None
    }
    /// `FreightCar -needsChestSave`
    fn freight_needs_chest_save(&self, car: &ObjRef) -> bool {
        false
    }
    /// `FreightCar -setNeedsChestSave:`
    fn freight_set_needs_chest_save(&self, car: &ObjRef, v: bool) {}
    /// `FreightCar -getChestSaveDict`
    fn freight_chest_save_dict(&self, car: &ObjRef) -> Option<PlistValue> {
        None
    }
    /// `[obj cleanup]` (train cars, `dealloc` only)
    fn cleanup(&self, obj: &ObjRef) {}

    // ---- FreeBlock ----
    /// `FreeBlock initWithWorld:dynamicWorld:cache:netData:avoidFreeblockDupeObjectIds:` (`avoid` is `Some` for packet 0x14).
    fn freeblock_from_net_data(&self, net_data: &[u8], avoid: Option<&mut VecDeque<u64>>) -> Option<ObjRef> {
        None
    }
    /// `FreeBlock initWithWorld:dynamicWorld:atPosition:ofType:dataA:dataB:subItems:dynamicObjectSaveDict:cache:hovers:priorityBlockhead:`
    fn freeblock_from_position(&self, init: FreeBlockInit<'_>) -> Option<ObjRef> {
        None
    }
    /// `FreeBlock -setCreationSoundPlayTime:` and `-setSoundType:`
    fn freeblock_set_creation_sound(&self, fb: &ObjRef, creation_sound_play_time: f32, sound_type: i32) {}

    // ---- Blockhead (blockhead_core) ----
    /// `Blockhead initWithWorld:dynamicWorld:saveDict:savedInventorySlots:cache:repositionOnLoadFailure:clientSaveDir:clientLocallySavedDict:`
    fn blockhead_from_save_dict(
        &self,
        save_dict: &PlistDict,
        saved_inventory_slots: Option<PlistValue>,
        reposition: bool,
        client_save_dir: Option<&str>,
        client_locally_saved_dict: Option<PlistValue>,
    ) -> Option<ObjRef> {
        None
    }
    /// `Blockhead initWithWorld:dynamicWorld:cache:netData:`
    fn blockhead_from_net_data(&self, net_data: &[u8]) -> Option<ObjRef> {
        None
    }
    /// `Blockhead -clientID`
    fn blockhead_client_id(&self, b: &ObjRef) -> Option<String> {
        None
    }
    /// `Blockhead -setClientID:`
    fn blockhead_set_client_id(&self, b: &ObjRef, id: &str) {}
    /// `Blockhead -setClientName:`
    fn blockhead_set_client_name(&self, b: &ObjRef, name: Option<&str>) {}
    /// `Blockhead -clientSaveDir`
    fn blockhead_client_save_dir(&self, b: &ObjRef) -> Option<String> {
        None
    }
    /// `[bh getSaveDictIncludingWorkbenchOrInterationObject:]`
    fn blockhead_save_dict_including_workbench_or_interaction_object(&self, b: &ObjRef, including: bool) -> PlistValue {
        PlistValue::mutable_dict()
    }
    /// `[bh saveItemSlotsArray]`
    fn blockhead_save_item_slots_array(&self, b: &ObjRef) -> PlistValue {
        PlistValue::mutable_array()
    }
    /// `[bh prepareInventoryForSaving]`
    fn blockhead_prepare_inventory_for_saving(&self, b: &ObjRef) {}
    /// `[bh inventoryNeedsSaving]`
    fn blockhead_inventory_needs_saving(&self, b: &ObjRef) -> bool {
        false
    }
    /// `[bh setInventoryNeedsSaving:]`
    fn blockhead_set_inventory_needs_saving(&self, b: &ObjRef, v: bool) {}
    /// `[bh inventoryWasChanged:subIndex:wasUsage:]`
    fn blockhead_inventory_was_changed(&self, b: &ObjRef, index: i32, sub_index: i32, was_usage: bool) {}
    /// `[bh stopInteractingWithInteractionObjectsIfNoInteractionObject]`
    fn blockhead_stop_interacting_if_no_interaction_object(&self, b: &ObjRef) {}
    /// `[bh stopRiding]`
    fn blockhead_stop_riding(&self, b: &ObjRef) {}
    /// `[bh die]`
    fn blockhead_die(&self, b: &ObjRef) {}
}
}

/// The default hooks: every default method of [`DwHooks`].
pub struct NullHooks;
impl DwHooksStub for NullHooks {}

/// Arguments of [`DynamicWorld::new`].
pub struct DynamicWorldInit {
    pub world: WorldRef,
    pub world_tile_loader: Option<Rc<dyn DwWorldGen>>,
    pub server: Option<ServerRef>,
    pub server_clients: Option<ServerClientMap>,
    pub cache: CacheRef,
    pub tree_density_noise_function: Option<Rc<dyn DwNoise>>,
    pub season_offset_noise_function: Option<Rc<dyn DwNoise>>,
    pub app_database: Option<Rc<Database>>,
    pub world_database: Rc<Database>,
    pub dynamic_object_database: Rc<Database>,
    /// Test seam: when `Some`, used instead of `<APPSUP>/saves/<saveID>/` (the directory is still created).
    pub world_save_directory_override: Option<String>,
}

fn vec65<T>(f: impl FnMut() -> T) -> Vec<T> {
    let mut v = Vec::with_capacity(MAX_DYNAMIC_OBJECT_TYPES);
    let mut f = f;
    for _ in 0..MAX_DYNAMIC_OBJECT_TYPES {
        v.push(f());
    }
    v
}

/// `@interface DynamicWorld: NSObject` (instance size 0x5b98). Ivar table`; contract section 2.
pub struct DynamicWorld {
    // ---- links (ObjC: plain pointer stores, NO retain) ----
    /// 0x08
    pub world: Weak<World>,
    /// 0x10 (worldgen; `Some` on the server)
    pub world_tile_loader: RefCell<Option<Rc<dyn DwWorldGen>>>,
    /// 0x20, set again by `set_server_server_clients`
    pub server: RefCell<Option<ServerRef>>,
    /// 0x30
    pub server_clients: RefCell<Option<ServerClientMap>>,
    /// 0x38 (`None` on the dedicated server, decision 4)
    pub app_database: Option<Rc<Database>>,
    /// 0x40 sub-db `main`
    pub world_database: Rc<Database>,
    /// 0x48 sub-db `dw`
    pub dynamic_object_database: Rc<Database>,
    /// 0x50 `"<APPSUP>/TheBlockheads/saves/<saveID>/"` (trailing slash)
    pub world_save_directory: String,
    /// 0x4ec8 (client only: `None` on the server)
    pub cache: CacheRef,
    /// 0x4ed0 (`None` => noise value 0.0)
    pub tree_density_noise_function: Option<Rc<dyn DwNoise>>,
    /// 0x4ed8
    pub season_offset_noise_function: Option<Rc<dyn DwNoise>>,
    /// 0x5b68 `wirePathCreator`, created at the end of init (`[[WirePathCreator alloc] initWithWorld:world]`).
    pub wire_path_creator: Rc<crate::path::wire_path::WirePathCreator>,
    /// Calls into other modules (see module docs).
    pub hooks: RefCell<Rc<dyn DwHooks>>,

    // ---- blockhead lists ----
    /// 0x58 (always empty on the dedicated server, decision 3)
    pub blockheads: RefCell<Vec<ObjRef>>,
    /// 0x60
    pub net_blockheads: RefCell<Vec<ObjRef>>,
    /// 0x68
    pub net_blockheads_with_disconnected_clients: RefCell<Vec<ObjRef>>,
    /// 0x70 key -> data, written by saveGame
    pub client_blockhead_inventories_to_save: RefCell<GsDict<String, Vec<u8>>>,
    /// 0x5b70 (written by ops clients.rs)
    pub live_server_client_blockhead_inventories: RefCell<GsDict<String, PlistValue>>,
    /// 0x5b40 (nil until the first simulated disconnect)
    pub disconnected_clients_save_dir_names: RefCell<Option<Vec<String>>>,
    /// 0x5b48
    pub disconnected_clients_cached_save_dicts_not_done: RefCell<Option<GsDict<String, Vec<PlistValue>>>>,

    // ---- the 65-way object tables ----
    /// 0x78, `len == 65`
    pub dynamic_objects: RefCell<Vec<ObjMap>>,
    /// 0xca8
    pub dynamic_objects_to_add: RefCell<Vec<ObjMap>>,
    /// 0x18d8, key = `worldIndexAtWorldPos as i32` sign-extended to u64
    pub dynamic_objects_by_world_pos_index: RefCell<Vec<ObjMap>>,
    /// 0x2508 `std::map<u64, std::set<DynamicObject*>>`; inner key = unique id (CRITIC: DECISION 'ascending uniqueID').
    pub free_blocks_by_position: RefCell<BTreeMap<u64, BTreeMap<u64, ObjRef>>>,
    /// 0x2538
    pub currently_adding_glow_blocks: RefCell<HashSet<u64>>,
    /// 0x2568, `len == 65` (never iterated)
    pub currently_adding_object_ids: RefCell<Vec<HashSet<u64>>>,
    /// 0x3198, `len == 65` (never iterated)
    pub currently_loading_macro_blocks: RefCell<Vec<HashSet<u32>>>,
    /// 0x3dc8, `len == 65` (only types 22 and 29 are ever non-empty)
    pub partial_update_ordered_objects: RefCell<Vec<Vec<ObjRef>>>,
    /// 0x43e0, `len == 65`
    pub partial_update_current_index: RefCell<Vec<i32>>,

    // ---- dirty trackers (written by ops dirty.rs; consumed here) ----
    /// 0x44e8
    pub world_changed_positions: RefCell<PosVec>,
    /// 0x4500
    pub world_changed_send_unreliably_macro_positions: RefCell<PosVec>,
    /// 0x4518
    pub snow_changed_macro_positions: RefCell<PosVec>,
    /// 0x4530
    pub world_changed_dont_send_macro_positions: RefCell<PosVec>,
    /// 0x4548, `len == 32`
    pub light_changed_send_unreliably_macro_positions_single_client: RefCell<Vec<PosVec>>,
    /// 0x4848
    pub water_changed_positions: RefCell<PosVec>,
    /// 0x4860
    pub world_contents_changed_positions: RefCell<PosVec>,
    /// 0x4878
    pub world_partial_content_changed_positions: RefCell<PosVec>,
    /// 0x4890, `len == 65`
    pub dynamic_world_changed_macro_positions: RefCell<Vec<PosVec>>,
    /// 0x4ea8
    pub world_changed_macro_positions: RefCell<PosVec>,
    /// 0x4ec0 NSMutableIndexSet of world tile indices holding trade portals
    pub portal_positions: RefCell<IndexSet>,

    // ---- scalars ----
    /// 0x4ee0
    pub dynamic_object_id_count: Cell<u64>,
    /// 0x4ee8
    pub active_blockhead_index: Cell<i32>,
    /// 0x4eec
    pub free_block_sound_delay: Cell<f32>,
    /// 0x4ef0
    pub has_loaded_blockheads: Cell<bool>,
    /// 0x4ef1 (client only; never set)
    pub has_recieved_initial_dynamic_objects_data_from_server: Cell<bool>,
    /// 0x4ef2
    pub workbench_has_been_crafted: Cell<bool>,

    // ---- net queues (ObjC arrays created lazily; nil == empty) ----
    /// 0x4ef8, `len == 65`, clientID -> blobs (GsDict: enumeration order is observable)
    pub net_create_dynamic_objects: RefCell<Vec<GsDict<String, Vec<NetBlob>>>>,
    /// 0x5100
    pub net_update_dynamic_objects: RefCell<Vec<Vec<NetBlob>>>,
    /// 0x5308
    pub net_update_creation_data_dynamic_objects: RefCell<Vec<Vec<NetBlob>>>,
    /// 0x5510
    pub net_remove_dynamic_objects: RefCell<Vec<Vec<NetBlob>>>,
    /// 0x5720, filled in init with `rand() as u16`, i = 0..511 in order
    pub random_numbers: [u16; 512],
    /// 0x5b20
    pub random_index: Cell<i32>,
    /// 0x5b24
    pub animal_save_counter: Cell<i32>,
    /// 0x5b28
    pub save_unreliable_counter: Cell<i32>,
    /// 0x5b2c
    pub send_light_counter: Cell<i32>,
    /// 0x5b30
    pub save_to_disk_counter: Cell<i32>,
    /// 0x5b34 (wraps)
    pub world_changed_simulate_counter: Cell<i32>,
    /// 0x5b38 dirName -> {oldPath -> newPath}
    pub version_one_to_two_conversion_list: RefCell<Option<GsDict<String, GsDict<String, String>>>>,
    /// 0x5b50
    pub send_unreliable_counter: Cell<i32>,
    /// 0x5b54 (vec4, ops)
    pub client_tree_life_fraction: Cell<[f32; 4]>,
    /// 0x5b78 `std::list<uint64>`
    pub avoid_freeblock_dupe_object_ids: RefCell<VecDeque<u64>>,
    /// 0x5b88 `"%d"` itemType -> real worldTime
    pub pole_item_taken_times: RefCell<Option<GsDict<String, PlistValue>>>,
    /// 0x5b90
    pub pole_item_restore_recheck_timer: Cell<f32>,
    /// 0x5b94
    pub pole_item_restore_add_timer: Cell<f32>,
    // conversion.rs private state
    /// reference-time of the next directory step (contract 5.8)
    pub conversion_next_due: Cell<f64>,
    /// the `localversionOneToTwoConversionList` copy the thread iterates
    pub conversion_snapshot: RefCell<Option<VecDeque<(String, GsDict<String, String>)>>>,
}

impl DynamicWorld {
    /// `-initWithWorld:worldTileLoader:clientTileLoader:server:client:serverClients:cache:treeDensityNoiseFunction:
    /// seasonOffsetNoiseFunction:appDatabase:worldDatabase:dynamicObjectDatabase:`. Init order is observable: lists,
    /// directory creation, 512 `rand()` draws (i ascending), then the WirePathCreator.
    pub fn new(init: DynamicWorldInit) -> Rc<DynamicWorld> {
        // worldSaveDirectory = "%@/saves/%@/" % (appSupport/TheBlockheads, saveID)
        let world_save_directory = match &init.world_save_directory_override {
            Some(p) => p.clone(),
            None => {
                let app_support = crate::foundation::paths::search_path_application_support();
                let save_id = init.world.save_id().unwrap_or_default();
                format!(
                    "{}/saves/{}/",
                    crate::foundation::paths::append_path_component(&app_support, "TheBlockheads"),
                    save_id
                )
            }
        };
        if !std::path::Path::new(&world_save_directory).exists() {
            let _ = std::fs::create_dir_all(&world_save_directory);
        }
        let mut random_numbers = [0u16; 512];
        for r in random_numbers.iter_mut() {
            *r = crate::rng::glibc_rand::rand_tagged("DynamicWorld.init.randomNumbers") as u16;
        }
        // wirePathCreator = [[WirePathCreator alloc] initWithWorld:world] (after the rand() draws; consumes no rand())
        let wire_path_creator = crate::path::wire_path::WirePathCreator::init_with_world(&init.world);
        Rc::new(DynamicWorld {
            world: Rc::downgrade(&init.world),
            wire_path_creator,
            world_tile_loader: RefCell::new(init.world_tile_loader),
            server: RefCell::new(init.server),
            server_clients: RefCell::new(init.server_clients),
            app_database: init.app_database,
            world_database: init.world_database,
            dynamic_object_database: init.dynamic_object_database,
            world_save_directory,
            cache: init.cache,
            tree_density_noise_function: init.tree_density_noise_function,
            season_offset_noise_function: init.season_offset_noise_function,
            hooks: RefCell::new(Rc::new(NullHooks)),
            blockheads: RefCell::new(Vec::new()),
            net_blockheads: RefCell::new(Vec::new()),
            net_blockheads_with_disconnected_clients: RefCell::new(Vec::new()),
            client_blockhead_inventories_to_save: RefCell::new(GsDict::new()),
            live_server_client_blockhead_inventories: RefCell::new(GsDict::new()),
            disconnected_clients_save_dir_names: RefCell::new(None),
            disconnected_clients_cached_save_dicts_not_done: RefCell::new(None),
            dynamic_objects: RefCell::new(vec65(BTreeMap::new)),
            dynamic_objects_to_add: RefCell::new(vec65(BTreeMap::new)),
            dynamic_objects_by_world_pos_index: RefCell::new(vec65(BTreeMap::new)),
            free_blocks_by_position: RefCell::new(BTreeMap::new()),
            currently_adding_glow_blocks: RefCell::new(HashSet::new()),
            currently_adding_object_ids: RefCell::new(vec65(HashSet::new)),
            currently_loading_macro_blocks: RefCell::new(vec65(HashSet::new)),
            partial_update_ordered_objects: RefCell::new(vec65(Vec::new)),
            partial_update_current_index: RefCell::new(vec![0; MAX_DYNAMIC_OBJECT_TYPES]),
            world_changed_positions: RefCell::new(Vec::new()),
            world_changed_send_unreliably_macro_positions: RefCell::new(Vec::new()),
            snow_changed_macro_positions: RefCell::new(Vec::new()),
            world_changed_dont_send_macro_positions: RefCell::new(Vec::new()),
            light_changed_send_unreliably_macro_positions_single_client: RefCell::new(
                (0..32).map(|_| Vec::new()).collect(),
            ),
            water_changed_positions: RefCell::new(Vec::new()),
            world_contents_changed_positions: RefCell::new(Vec::new()),
            world_partial_content_changed_positions: RefCell::new(Vec::new()),
            dynamic_world_changed_macro_positions: RefCell::new(vec65(Vec::new)),
            world_changed_macro_positions: RefCell::new(Vec::new()),
            portal_positions: RefCell::new(IndexSet::new()),
            dynamic_object_id_count: Cell::new(0),
            active_blockhead_index: Cell::new(0),
            free_block_sound_delay: Cell::new(0.0),
            has_loaded_blockheads: Cell::new(false),
            has_recieved_initial_dynamic_objects_data_from_server: Cell::new(false),
            workbench_has_been_crafted: Cell::new(false),
            net_create_dynamic_objects: RefCell::new(vec65(GsDict::new)),
            net_update_dynamic_objects: RefCell::new(vec65(Vec::new)),
            net_update_creation_data_dynamic_objects: RefCell::new(vec65(Vec::new)),
            net_remove_dynamic_objects: RefCell::new(vec65(Vec::new)),
            random_numbers,
            random_index: Cell::new(0),
            animal_save_counter: Cell::new(0),
            save_unreliable_counter: Cell::new(0),
            send_light_counter: Cell::new(0),
            save_to_disk_counter: Cell::new(0),
            world_changed_simulate_counter: Cell::new(0),
            version_one_to_two_conversion_list: RefCell::new(None),
            send_unreliable_counter: Cell::new(0),
            client_tree_life_fraction: Cell::new([0.0; 4]),
            avoid_freeblock_dupe_object_ids: RefCell::new(VecDeque::new()),
            pole_item_taken_times: RefCell::new(None),
            pole_item_restore_recheck_timer: Cell::new(0.0),
            pole_item_restore_add_timer: Cell::new(0.0),
            conversion_next_due: Cell::new(0.0),
            conversion_snapshot: RefCell::new(None),
        })
    }

    /// `-setServer:serverClients:`: plain stores.
    pub fn set_server_server_clients(&self, server: Option<ServerRef>, clients: Option<ServerClientMap>) {
        *self.server.borrow_mut() = server;
        *self.server_clients.borrow_mut() = clients;
    }

    /// Install the cross-module call implementation.
    pub fn set_hooks(&self, hooks: Rc<dyn DwHooks>) {
        *self.hooks.borrow_mut() = hooks;
    }

    /// The installed hooks (cheap `Rc` clone; never hold a `RefCell` borrow across a hook call).
    pub fn hooks(&self) -> Rc<dyn DwHooks> {
        self.hooks.borrow().clone()
    }

    /// `-dealloc`: port for completeness; only reached at process exit. Must not run through `Drop`.
    pub fn dealloc(&self) {
        let hooks = self.hooks();
        let lists = [
            self.blockheads.borrow().clone(),
            self.net_blockheads.borrow().clone(),
            self.net_blockheads_with_disconnected_clients.borrow().clone(),
        ];
        for l in lists.iter() {
            for bh in l {
                hooks.blockhead_will_be_unloaded(bh);
            }
        }
        for t in [41usize, 42, 43, 44] {
            let a: Vec<ObjRef> = self.dynamic_objects.borrow()[t].values().cloned().collect();
            for o in a {
                hooks.cleanup(&o);
            }
            let b: Vec<ObjRef> = self.dynamic_objects_to_add.borrow()[t].values().cloned().collect();
            for o in b {
                hooks.cleanup(&o);
            }
        }
        self.blockheads.borrow_mut().clear();
        self.net_blockheads.borrow_mut().clear();
        self.net_blockheads_with_disconnected_clients.borrow_mut().clear();
        self.client_blockhead_inventories_to_save.borrow_mut().remove_all();
        for i in 0..MAX_DYNAMIC_OBJECT_TYPES {
            self.dynamic_objects.borrow_mut()[i].clear();
            self.dynamic_objects_to_add.borrow_mut()[i].clear();
            self.dynamic_objects_by_world_pos_index.borrow_mut()[i].clear();
        }
        *self.version_one_to_two_conversion_list.borrow_mut() = None;
        for i in 0..MAX_DYNAMIC_OBJECT_TYPES {
            self.net_create_dynamic_objects.borrow_mut()[i] = GsDict::new();
            self.net_update_dynamic_objects.borrow_mut()[i].clear();
            self.net_update_creation_data_dynamic_objects.borrow_mut()[i].clear();
            self.net_remove_dynamic_objects.borrow_mut()[i].clear();
        }
        self.live_server_client_blockhead_inventories.borrow_mut().remove_all();
        // Not released in the original: disconnectedClientsSaveDirNames, disconnectedClientsCachedSaveDictsNotDone, poleItemTakenTimes.
    }

    /// The owning `World`. The world outlives its dynamic world; a dangling reference is a programming error.
    pub fn world(&self) -> WorldRef {
        self.world.upgrade().expect("DynamicWorld: owning World was dropped")
    }

    /// `self.server != nil` (private `has_server`, contract 4.6).
    pub fn has_server(&self) -> bool {
        self.server.borrow().is_some()
    }

    /// `[serverClients allValues]` in GsDict order (snapshot).
    pub fn server_client_values(&self) -> Vec<ServerClientRef> {
        match &*self.server_clients.borrow() {
            Some(m) => m.borrow().values_in_order().into_iter().cloned().collect(),
            None => Vec::new(),
        }
    }

    /// `[serverClients allKeys]` in GsDict order (snapshot).
    pub fn server_client_keys(&self) -> Vec<String> {
        match &*self.server_clients.borrow() {
            Some(m) => m.borrow().keys_in_order(),
            None => Vec::new(),
        }
    }

    /// `[serverClients objectForKey:id]`
    pub fn server_client(&self, id: &str) -> Option<ServerClientRef> {
        match &*self.server_clients.borrow() {
            Some(m) => m.borrow().get(&id.to_string()).cloned(),
            None => None,
        }
    }

    /// `[serverClients count]`
    pub fn server_client_count(&self) -> usize {
        match &*self.server_clients.borrow() {
            Some(m) => m.borrow().len(),
            None => 0,
        }
    }

    /// `worldIndexAtWorldPos(pos, world)`; the map key is `(idx as i64) as u64`.
    pub fn world_index(&self, pos: IntPair) -> i32 {
        world_index_at_world_pos(pos, &self.world())
    }

    /// Sign-extended map key of a world index.
    pub fn world_index_key(&self, pos: IntPair) -> u64 {
        (self.world_index(pos) as i64) as u64
    }

    // ---- 2.1 table helpers: dynamic_objects (`do_*`), dynamic_objects_to_add (`ta_*`), by_world_pos_index (`bp_*`) ----

    /// `dynamicObjects[t].find(id)`
    pub fn do_get(&self, t: usize, id: u64) -> Option<ObjRef> {
        self.dynamic_objects.borrow()[t].get(&id).cloned()
    }
    /// `dynamicObjects[t].count(id) != 0`
    pub fn do_contains(&self, t: usize, id: u64) -> bool {
        self.dynamic_objects.borrow()[t].contains_key(&id)
    }
    /// `dynamicObjects[t][id] = o` (overwrites; the original leaks the old entry)
    pub fn do_set(&self, t: usize, id: u64, o: ObjRef) {
        self.dynamic_objects.borrow_mut()[t].insert(id, o);
    }
    /// `dynamicObjects[t].insert({id, o})`: keeps an existing entry; returns whether inserted
    pub fn do_insert_new(&self, t: usize, id: u64, o: ObjRef) -> bool {
        let mut m = self.dynamic_objects.borrow_mut();
        if m[t].contains_key(&id) {
            false
        } else {
            m[t].insert(id, o);
            true
        }
    }
    /// `dynamicObjects[t].erase(id)`
    pub fn do_erase(&self, t: usize, id: u64) {
        self.dynamic_objects.borrow_mut()[t].remove(&id);
    }
    /// Smallest key greater than `last` (smallest key when `last` is `None`): successor iteration, contract 3.3.
    pub fn do_next_after(&self, t: usize, last: Option<u64>) -> Option<(u64, ObjRef)> {
        next_after(&self.dynamic_objects.borrow()[t], last)
    }
    /// `dynamicObjects[t].size()`
    pub fn do_len(&self, t: usize) -> usize {
        self.dynamic_objects.borrow()[t].len()
    }

    /// `dynamicObjectsToAdd[t].find(id)`
    pub fn ta_get(&self, t: usize, id: u64) -> Option<ObjRef> {
        self.dynamic_objects_to_add.borrow()[t].get(&id).cloned()
    }
    /// `dynamicObjectsToAdd[t].count(id) != 0`
    pub fn ta_contains(&self, t: usize, id: u64) -> bool {
        self.dynamic_objects_to_add.borrow()[t].contains_key(&id)
    }
    /// `dynamicObjectsToAdd[t][id] = o`
    pub fn ta_set(&self, t: usize, id: u64, o: ObjRef) {
        self.dynamic_objects_to_add.borrow_mut()[t].insert(id, o);
    }
    /// `dynamicObjectsToAdd[t].insert({id, o})`
    pub fn ta_insert_new(&self, t: usize, id: u64, o: ObjRef) -> bool {
        let mut m = self.dynamic_objects_to_add.borrow_mut();
        if m[t].contains_key(&id) {
            false
        } else {
            m[t].insert(id, o);
            true
        }
    }
    /// `dynamicObjectsToAdd[t].erase(id)`
    pub fn ta_erase(&self, t: usize, id: u64) {
        self.dynamic_objects_to_add.borrow_mut()[t].remove(&id);
    }
    /// Successor lookup in `dynamicObjectsToAdd[t]`.
    pub fn ta_next_after(&self, t: usize, last: Option<u64>) -> Option<(u64, ObjRef)> {
        next_after(&self.dynamic_objects_to_add.borrow()[t], last)
    }
    /// `dynamicObjectsToAdd[t].size()`
    pub fn ta_len(&self, t: usize) -> usize {
        self.dynamic_objects_to_add.borrow()[t].len()
    }

    /// `dynamicObjectsByWorldPosIndex[t].find(idx)`
    pub fn bp_get(&self, t: usize, idx: u64) -> Option<ObjRef> {
        self.dynamic_objects_by_world_pos_index.borrow()[t].get(&idx).cloned()
    }
    /// `dynamicObjectsByWorldPosIndex[t].count(idx) != 0`
    pub fn bp_contains(&self, t: usize, idx: u64) -> bool {
        self.dynamic_objects_by_world_pos_index.borrow()[t].contains_key(&idx)
    }
    /// `dynamicObjectsByWorldPosIndex[t][idx] = o`
    pub fn bp_set(&self, t: usize, idx: u64, o: ObjRef) {
        self.dynamic_objects_by_world_pos_index.borrow_mut()[t].insert(idx, o);
    }
    /// `dynamicObjectsByWorldPosIndex[t].insert({idx, o})`
    pub fn bp_insert_new(&self, t: usize, idx: u64, o: ObjRef) -> bool {
        let mut m = self.dynamic_objects_by_world_pos_index.borrow_mut();
        if m[t].contains_key(&idx) {
            false
        } else {
            m[t].insert(idx, o);
            true
        }
    }
    /// `dynamicObjectsByWorldPosIndex[t].erase(idx)`
    pub fn bp_erase(&self, t: usize, idx: u64) {
        self.dynamic_objects_by_world_pos_index.borrow_mut()[t].remove(&idx);
    }
    /// "erase key if value is the same object" (update step 10A / 10B).
    pub fn bp_erase_if_same(&self, t: usize, idx: u64, o: &ObjRef) {
        let mut m = self.dynamic_objects_by_world_pos_index.borrow_mut();
        let same = match m[t].get(&idx) {
            Some(v) => crate::objects::handle::ptr_eq(v, o),
            None => false,
        };
        if same {
            m[t].remove(&idx);
        }
    }
    /// Successor lookup in `dynamicObjectsByWorldPosIndex[t]`.
    pub fn bp_next_after(&self, t: usize, last: Option<u64>) -> Option<(u64, ObjRef)> {
        next_after(&self.dynamic_objects_by_world_pos_index.borrow()[t], last)
    }
    /// `dynamicObjectsByWorldPosIndex[t].size()`
    pub fn bp_len(&self, t: usize) -> usize {
        self.dynamic_objects_by_world_pos_index.borrow()[t].len()
    }

    /// `freeBlocksByPosition[worldIndex].insert(o)` (set semantics, inner key = unique id).
    pub fn free_blocks_insert(&self, world_index: u64, o: &ObjRef) {
        let uid = o.unique_id();
        self.free_blocks_by_position.borrow_mut().entry(world_index).or_default().insert(uid, o.clone());
    }
    /// `freeBlocksByPosition[worldIndex].erase(o)`; erases the key when the set becomes empty (update step 10A).
    pub fn free_blocks_erase(&self, world_index: u64, o: &ObjRef) {
        let uid = o.unique_id();
        let mut m = self.free_blocks_by_position.borrow_mut();
        let mut empty = false;
        if let Some(set) = m.get_mut(&world_index) {
            set.remove(&uid);
            empty = set.is_empty();
        }
        if empty {
            m.remove(&world_index);
        }
    }

    /// Visit `dynamicObjects[t]` by successor lookup (inserts during the loop with larger keys are visited).
    /// The borrow is released before `f` runs.
    pub fn for_each_dynamic_object(&self, t: usize, mut f: impl FnMut(u64, &ObjRef)) {
        let mut last: Option<u64> = None;
        while let Some((id, o)) = self.do_next_after(t, last) {
            f(id, &o);
            last = Some(id);
        }
    }
}

fn next_after(m: &ObjMap, last: Option<u64>) -> Option<(u64, ObjRef)> {
    use std::ops::Bound::{Excluded, Unbounded};
    match last {
        None => m.iter().next().map(|(k, v)| (*k, v.clone())),
        Some(l) => m.range((Excluded(l), Unbounded)).next().map(|(k, v)| (*k, v.clone())),
    }
}

#[cfg(test)]
pub(crate) mod testutil {
    //! Test support shared by the dynworld unit tests: a plain dynamic object and a bare `DynamicWorld`.
    use super::*;
    use crate::objects::base::{DynamicObject, DynamicObjectBase};
    use crate::objects::class_info::{ClassInfo, DYNAMIC_OBJECT_CLASS};
    use std::any::Any;

    /// A minimal object with a configurable type, logging its update calls.
    pub struct Obj {
        pub base: RefCell<DynamicObjectBase>,
        pub ty: Cell<i32>,
        pub log: Rc<RefCell<Vec<String>>>,
        pub remove_after_update: Cell<bool>,
        pub create_on_update: RefCell<Option<Box<dyn Fn()>>>,
    }
    impl Obj {
        pub fn new(ty: i32, id: u64, log: Rc<RefCell<Vec<String>>>) -> Rc<Obj> {
            let o = crate::objects::handle::alloc_obj(|w| Obj {
                base: RefCell::new(DynamicObjectBase::alloc_zeroed_with(w)),
                ty: Cell::new(ty),
                log,
                remove_after_update: Cell::new(false),
                create_on_update: RefCell::new(None),
            });
            o.base.borrow_mut().unique_id = id;
            o
        }
    }
    impl DynamicObject for Obj {
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
            self.ty.get()
        }
        fn remove_from_macro_block(&self, _me: &ObjRef) {
            self.log.borrow_mut().push(format!("rfmb({})", self.base.borrow().unique_id));
        }
        fn update_accurate_dt_is_simulation(&self, _me: &ObjRef, dt: f32, _adt: f32, sim: bool) {
            self.log.borrow_mut().push(format!(
                "upd({},{},{},{})",
                self.ty.get(),
                self.base.borrow().unique_id,
                dt,
                sim
            ));
            if let Some(f) = &*self.create_on_update.borrow() {
                f();
            }
            if self.remove_after_update.get() {
                self.base.borrow_mut().needs_removed = true;
            }
        }
    }

    /// A `World` with a given width in macro tiles.
    pub fn world(width_macro: i32) -> WorldRef {
        World::alloc(width_macro)
    }

    /// Unique per-test scratch directory.
    pub fn scratch(name: &str) -> String {
        static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let n = N.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let p = std::env::temp_dir().join(format!("bh_dw_{}_{}_{}", name, std::process::id(), n));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        format!("{}/", p.display())
    }

    /// Open two sub-databases (`main`, `dw`) in a fresh LMDB environment.
    pub fn dbs(name: &str) -> (Rc<Database>, Rc<Database>) {
        use crate::storage::lmdb::DatabaseEnvironment;
        let dir = scratch(name);
        let env = DatabaseEnvironment::new(&format!("{}world_db", dir), 8, 64).expect("env");
        (Rc::new(Database::new(&env, "main").unwrap()), Rc::new(Database::new(&env, "dw").unwrap()))
    }

    /// A bare `DynamicWorld` on a fresh world.
    pub fn dw(name: &str, width_macro: i32) -> Rc<DynamicWorld> {
        let (main, dwdb) = dbs(name);
        let dir = scratch(&format!("{name}_save"));
        let w = world(width_macro);
        // The DynamicWorld holds only a Weak<World>: keep the test world alive for the process.
        std::mem::forget(w.clone());
        DynamicWorld::new(DynamicWorldInit {
            world: w,
            world_tile_loader: None,
            server: None,
            server_clients: None,
            cache: None,
            tree_density_noise_function: None,
            season_offset_noise_function: None,
            app_database: None,
            world_database: main,
            dynamic_object_database: dwdb,
            world_save_directory_override: Some(dir),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::testutil::*;
    use super::*;

    /// contract 8 item 16: successor iteration sees inserts with larger keys made during the loop, not smaller ones.
    #[test]
    fn do_next_after_sees_inserts() {
        let d = dw("state_next", 1);
        let log = Rc::new(RefCell::new(vec![]));
        for id in [10u64, 20, 30] {
            let o: ObjRef = Obj::new(14, id, log.clone());
            d.do_set(14, id, o);
        }
        let mut seen = vec![];
        let d2 = d.clone();
        let log2 = log.clone();
        d.for_each_dynamic_object(14, |id, _| {
            seen.push(id);
            if id == 20 {
                let a: ObjRef = Obj::new(14, 25, log2.clone());
                d2.do_set(14, 25, a);
                let b: ObjRef = Obj::new(14, 5, log2.clone());
                d2.do_set(14, 5, b);
            }
        });
        assert_eq!(seen, vec![10, 20, 25, 30]);
    }

    /// init: 65-way tables, 512 random numbers, counters zero.
    #[test]
    fn init_shapes() {
        let d = dw("state_init", 1);
        assert_eq!(d.dynamic_objects.borrow().len(), 65);
        assert_eq!(d.dynamic_world_changed_macro_positions.borrow().len(), 65);
        assert_eq!(d.light_changed_send_unreliably_macro_positions_single_client.borrow().len(), 32);
        assert_eq!(d.random_index.get(), 0);
        assert!(d.world_save_directory.ends_with('/'));
    }

    /// do_insert_new keeps the existing entry; do_set overwrites.
    #[test]
    fn insert_vs_set() {
        let d = dw("state_ins", 1);
        let log = Rc::new(RefCell::new(vec![]));
        let a: ObjRef = Obj::new(3, 1, log.clone());
        let b: ObjRef = Obj::new(3, 1, log.clone());
        assert!(d.do_insert_new(3, 1, a.clone()));
        assert!(!d.do_insert_new(3, 1, b.clone()));
        assert!(crate::objects::handle::ptr_eq(&d.do_get(3, 1).unwrap(), &a));
        d.do_set(3, 1, b.clone());
        assert!(crate::objects::handle::ptr_eq(&d.do_get(3, 1).unwrap(), &b));
    }

    /// free blocks: ascending uid iteration; key erased when the set becomes empty.
    #[test]
    fn free_block_sets() {
        let d = dw("state_fb", 1);
        let log = Rc::new(RefCell::new(vec![]));
        let a: ObjRef = Obj::new(14, 7, log.clone());
        let b: ObjRef = Obj::new(14, 3, log.clone());
        d.free_blocks_insert(99, &a);
        d.free_blocks_insert(99, &b);
        d.free_blocks_insert(99, &b);
        assert_eq!(d.free_blocks_by_position.borrow()[&99].keys().copied().collect::<Vec<_>>(), vec![3, 7]);
        d.free_blocks_erase(99, &a);
        d.free_blocks_erase(99, &b);
        assert!(d.free_blocks_by_position.borrow().is_empty());
    }
}
