//! Production integration layer ("glue"): the single object that implements every process-wide seam trait and
//! forwards each call to the real collaborator (BHServer, ServerClient, WorldTileLoader, DynamicWorld, PathCreator,
//! ProjectileManager, TipManager). The original has no seams (every message is live from `-[World init]` on), so
//! this object must be installed BEFORE `World::init`; objects that do not exist yet (loader, DynamicWorld) are
//! late-bound cells, and a nil receiver returns the trait's inert default (ObjC nil messaging).
//!
//!
//! RNG rule (r1_rng_0): `loader_init` constructs the WorldTileLoader exactly once and neither it nor anything else in
//! this file makes any `rand()` / `srand` call of its own.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::{Rc, Weak};

use crate::dynworld::lookups::DwOpsHooks;
use crate::dynworld::state::{
    DwHooks, DwNoise, DwServer, DwServerClient, DwWorldGen, DynamicWorld as RealDw, DynamicWorldInit, SaveLoadKind,
    ServerClientMap as DwClientMap,
};
use crate::foundation::gsdict::GsDict;
use crate::objects::handle::ObjRef;
use crate::plist::value::{PlistDict, PlistValue};
use crate::server::bhserver::{BHServer, ServerDynamicWorld};
use crate::server::server_client::ServerClient;
use crate::storage::lmdb::Database;
use crate::types::structs::IntPair;
use crate::world::lifecycle::WorldCoreHooks;
use crate::world::misc::WorldOpsDeps;
use crate::world::state::{
    BHServer as WorldServerHandle, DwObject, DynamicWorld as PlaceholderDw, DynamicWorldQueries, ElevatorMotorInfo,
    MacroTileRef, PhysicalBlockRef, TileRef, World,
};
use crate::worldgen::block_io::LoadOutcome;
use crate::worldgen::noise::NoiseFunction;
use crate::worldgen::tile_loader::{DynamicObjectsNetData, WorldTileLoader, WorldgenHost};

mod actor_hosts;
mod blockhead_host;
#[cfg(test)]
mod hook_fixture;
#[cfg(test)]
mod hook_tests;
#[cfg(test)]
mod hook_tests_more;
#[cfg(test)]
mod hook_tests_objects;

/// The dynamic object type whose class is `name` (`classNameForDynamicObjectType`); 0 when unknown.
fn dyn_type_named(name: &str) -> usize {
    (1..=64).find(|t| crate::objects::type_tables::class_name_for_dynamic_object_type(*t) == Some(name)).unwrap_or(0)
        as usize
}

/// The `Blockhead` behind an object handle (`None` for any other class).
fn bh_of(o: &ObjRef) -> Option<&crate::blockhead::blockhead::Blockhead> {
    crate::objects::handle::downcast::<crate::blockhead::blockhead::Blockhead>(o)
}

/// The `Chest` behind an object handle (`None` for any other class).
fn chest_of(o: &ObjRef) -> Option<&crate::interaction::chest::Chest> {
    crate::objects::handle::downcast::<crate::interaction::chest::Chest>(o)
}

/// The `Door` behind an object handle (`None` for any other class).
fn door_of(o: &ObjRef) -> Option<&crate::building::door::Door> {
    crate::objects::handle::downcast::<crate::building::door::Door>(o)
}

/// `-[obj itemType]` of every class that answers it (doors, windows, free blocks, paintings, building parts, rails,
/// beds); 0 for any other object.
fn item_type_of(o: &ObjRef) -> i32 {
    use crate::building::elevator::{ElevatorMotor, ElevatorShaft};
    use crate::building::{
        bed::Bed, column::Column, door::Door, ladder::Ladder, stairs::Stairs, window::Window, wire::Wire,
    };
    use crate::objects::handle::downcast;
    if let Some(x) = downcast::<Door>(o) {
        x.item_type()
    } else if let Some(x) = downcast::<Window>(o) {
        x.item_type()
    } else if let Some(x) = downcast::<crate::physics_objects::free_block::FreeBlock>(o) {
        x.item_type()
    } else if let Some(x) = downcast::<crate::interaction::painting::Painting>(o) {
        x.item_type()
    } else if let Some(x) = downcast::<Column>(o) {
        x.item_type()
    } else if let Some(x) = downcast::<Stairs>(o) {
        x.item_type()
    } else if let Some(x) = downcast::<Ladder>(o) {
        x.item_type()
    } else if let Some(x) = downcast::<Wire>(o) {
        x.item_type()
    } else if let Some(x) = downcast::<ElevatorShaft>(o) {
        x.item_type()
    } else if let Some(x) = downcast::<ElevatorMotor>(o) {
        x.item_type()
    } else if let Some(x) = downcast::<crate::trains::rail::Rail>(o) {
        x.item_type()
    } else if let Some(x) = downcast::<Bed>(o) {
        x.item_type()
    } else {
        0
    }
}

/// The `FreightCar` behind an object handle (`None` for any other class).
fn freight_of(o: &ObjRef) -> Option<&crate::trains::freight_car::FreightCar> {
    crate::objects::handle::downcast::<crate::trains::freight_car::FreightCar>(o)
}

/// A `PlistValue` narrowed to its dictionary (an ObjC non-dict argument is treated as nil).
fn dict_of(v: Option<&PlistValue>) -> Option<&PlistDict> {
    match v {
        Some(PlistValue::Dict(d)) => Some(d),
        _ => None,
    }
}

/// `Vec<Vec<InventoryItem>>` (DynamicWorld face) from the shared `SubSlot` form: item contents are shallow-copied,
/// the nested slot handles are kept.
fn sub_items_to_owned(
    subs: &[crate::items::inventory_item::SubSlot],
) -> Vec<Vec<crate::items::inventory_item::InventoryItem>> {
    subs.iter()
        .map(|slot| {
            slot.borrow()
                .iter()
                .map(|it| {
                    let i = it.borrow();
                    crate::items::inventory_item::InventoryItem {
                        item_type: i.item_type,
                        data_a: i.data_a,
                        data_b: i.data_b,
                        sub_items: i.sub_items.clone(),
                        selected_sub_item_index: i.selected_sub_item_index,
                        dynamic_object_save_dict: i.dynamic_object_save_dict.clone(),
                    }
                })
                .collect()
        })
        .collect()
}

/// `Vec<Vec<InventoryItem>>` -> `SubSlot`s (each item wrapped in `Rc<RefCell<..>>`, order kept, nothing filtered).
fn sub_items_to_slots(
    v: Vec<Vec<crate::items::inventory_item::InventoryItem>>,
) -> Vec<crate::items::inventory_item::SubSlot> {
    v.into_iter()
        .map(|slot| Rc::new(RefCell::new(slot.into_iter().map(|i| Rc::new(RefCell::new(i))).collect::<Vec<_>>())))
        .collect()
}

/// `NoiseFunction` seen through the dynworld noise trait.
pub struct NoiseAdapter(pub Rc<NoiseFunction>);
impl DwNoise for NoiseAdapter {
    fn get_x_y_octaves(&self, x: f64, y: f64, octaves: i32) -> f64 {
        self.0.get_x_y_octaves(x, y, octaves)
    }
}

/// `WorldTileLoader` seen through [`DwWorldGen`]. The three position arrays are copied once at DynamicWorld
/// creation (they are shared and mutated by the DynamicWorld only).
pub struct LoaderAdapter {
    loader: Rc<WorldTileLoader>,
    tree: Rc<RefCell<Vec<i32>>>,
    npc: Rc<RefCell<Vec<i32>>>,
    plant: Rc<RefCell<Vec<i32>>>,
}
impl LoaderAdapter {
    pub fn new(loader: Rc<WorldTileLoader>) -> LoaderAdapter {
        LoaderAdapter {
            tree: Rc::new(RefCell::new(loader.tree_positions().to_vec())),
            npc: Rc::new(RefCell::new(loader.npc_positions().to_vec())),
            plant: Rc::new(RefCell::new(loader.plant_positions().to_vec())),
            loader,
        }
    }
}
impl DwWorldGen for LoaderAdapter {
    fn tree_positions(&self) -> Rc<RefCell<Vec<i32>>> {
        self.tree.clone()
    }
    fn npc_positions(&self) -> Rc<RefCell<Vec<i32>>> {
        self.npc.clone()
    }
    fn plant_positions(&self) -> Rc<RefCell<Vec<i32>>> {
        self.plant.clone()
    }
    fn max_of_rock_and_dirt_height_for_x(&self, x: i32) -> i32 {
        self.loader.max_of_rock_and_dirt_height_for_x(x)
    }
    fn lake_height_for_x(&self, x: i32) -> i32 {
        self.loader.lake_height_for_x(x)
    }
}

/// The placeholder `world::state::DynamicWorld` queries, forwarded to the real DynamicWorld through the glue.
struct Queries {
    glue: Weak<Glue>,
}
impl Queries {
    fn wrap(&self, f: impl FnOnce(&RealDw) -> Option<ObjRef>) -> Option<DwObject> {
        let dw = self.glue.upgrade()?.dw()?;
        let obj = f(&dw)?;
        let item_type = dw.hooks().item_type(&obj);
        let (interaction_object_type, is_paintable) = crate::objects::registry::as_interaction_like(&obj)
            .map_or((0, false), |i| (i.interaction_object_type(), i.is_paintable()));
        Some(DwObject { obj, item_type, interaction_object_type, chest_type: 0, is_paintable, sign_info: None })
    }
}
impl DynamicWorldQueries for Queries {
    fn door_can_be_used_by_path_user(&self, path_user: Option<&ObjRef>, pos: IntPair) -> bool {
        match (self.glue.upgrade().and_then(|g| g.dw()), path_user) {
            (Some(dw), Some(pu)) => dw.door_can_be_used_by_path_user_at_pos(pu, pos),
            _ => false,
        }
    }
    fn elevator_motor_for_shaft_at_pos(&self, _pos: IntPair) -> Option<ElevatorMotorInfo> {
        None
    }
    fn interaction_object_at_pos(&self, pos: IntPair) -> Option<DwObject> {
        self.wrap(|d| d.interaction_object_at_pos(pos))
    }
    fn ladder_at_pos(&self, pos: IntPair) -> Option<DwObject> {
        self.wrap(|d| d.object_of_type_at_pos(dyn_type_named("Ladder") as i32, pos))
    }
    fn column_at_pos(&self, pos: IntPair) -> Option<DwObject> {
        self.wrap(|d| d.hooks().object_at_pos_for_item('c', pos))
    }
    fn stairs_at_pos(&self, pos: IntPair) -> Option<DwObject> {
        self.wrap(|d| d.hooks().object_at_pos_for_item('s', pos))
    }
    fn elevator_shaft_at_pos(&self, pos: IntPair) -> Option<DwObject> {
        self.wrap(|d| d.object_of_type_at_pos(dyn_type_named("ElevatorShaft") as i32, pos))
    }
    fn painting_at_pos(&self, pos: IntPair) -> Option<DwObject> {
        self.wrap(|d| d.hooks().object_at_pos_for_item('p', pos))
    }
}

/// `ServerDynamicWorld` face of the real DynamicWorld (what `BHServer` messages).
pub struct DwFace(pub Rc<RealDw>);
impl ServerDynamicWorld for DwFace {
    fn get_next_dynamic_object_id(&self) -> u64 {
        self.0.get_next_dynamic_object_id()
    }
    fn client_pickup_request_count_client_id_blockhead_requester_unique_id(
        &self,
        ids: &[u64],
        client_id: &str,
        requester: u64,
    ) {
        self.0.client_pickup_request_count_client_id_blockhead_requester_unique_id(ids, client_id, requester)
    }
    fn create_client_freeblocks_with_data(&self, payload: &[u8]) {
        self.0.create_client_freeblocks_with_data(payload)
    }
    fn load_client_blockheads_data_for_player_id(&self, id: &str) -> Option<Vec<u8>> {
        self.0.load_client_blockheads_data_for_player_id(id)
    }
    fn client_blockheads_recieved_for_player_id_data(&self, id: &str, payload: &[u8]) {
        self.0.client_blockheads_recieved_for_player_id_data(id, payload)
    }
    fn client_blockhead_inventory_recieved_for_player_id_blockhead_id_data(
        &self,
        id: &str,
        blockhead_id: u64,
        data: &[u8],
    ) {
        self.0.client_blockhead_inventory_recieved_for_player_id_blockhead_id_data(id, blockhead_id, data)
    }
    fn send_painting_data_for_painting_with_id_to_client(&self, painting_id: u64, id: &str) {
        self.0.send_painting_data_for_painting_with_id_to_client(painting_id, id)
    }
    fn new_found_list_recieved_from_client_list(&self, id: &str, list: &crate::foundation::indexset::IndexSet) {
        self.0.new_found_list_recieved_from_client_list(id, list)
    }
    fn client_blockhead_with_id_from_client_requests_dyamic_object_removal_with_id(
        &self,
        blockhead_id: u64,
        id: &str,
        object_id: u64,
    ) {
        self.0.client_blockhead_with_id_from_client_requests_dyamic_object_removal_with_id(blockhead_id, id, object_id)
    }
    fn user_ban_changed_is_banned(&self, id: &str, banned: bool) {
        self.0.user_ban_changed_is_banned(id, banned)
    }
}

/// The one production implementation of every seam. See the module docs.
pub struct Glue {
    weak_self: Weak<Glue>,
    world: RefCell<Weak<World>>,
    server: Weak<BHServer>,
    loader: RefCell<Option<Rc<WorldTileLoader>>>,
    dw: RefCell<Option<Rc<RealDw>>>,
    /// The dynworld view of the per-peer map (same keys / order as `World.server_clients`).
    dw_clients: DwClientMap,
    /// The real per-peer objects.
    clients: RefCell<HashMap<String, Rc<ServerClient>>>,
    path_creator: RefCell<Option<Rc<crate::path::path_creator::PathCreator>>>,
    projectile_manager: RefCell<Option<Rc<crate::physics_objects::projectile_manager::ProjectileManager>>>,
    weather_noise: RefCell<Option<Rc<NoiseFunction>>>,
    weather: RefCell<Option<Rc<crate::env::weather::Weather>>>,
}

impl Glue {
    pub fn new(server: &Rc<BHServer>) -> Rc<Glue> {
        Rc::new_cyclic(|w| Glue {
            weak_self: w.clone(),
            world: RefCell::new(Weak::new()),
            server: Rc::downgrade(server),
            loader: RefCell::new(None),
            dw: RefCell::new(None),
            dw_clients: Rc::new(RefCell::new(GsDict::new())),
            clients: RefCell::new(HashMap::new()),
            path_creator: RefCell::new(None),
            projectile_manager: RefCell::new(None),
            weather_noise: RefCell::new(None),
            weather: RefCell::new(None),
        })
    }

    /// Install every process-wide seam that can be installed before `World::init`.
    pub fn install_pre_init(self: &Rc<Self>) {
        crate::objects::registry::install();
        crate::world::lifecycle::set_core_hooks(self.clone());
        crate::world::misc::install_deps(self.clone());
        crate::dynworld::lookups::set_ops_hooks(self.clone());
        crate::worldgen::tile_loader::set_host(Some(self.clone()));
        crate::lighting::artificial_light::install_services(self.clone());
        crate::path::path_creator::set_dynamic_world_hooks(Some(self.clone()));
        crate::npc::npc::set_npc_env(self.clone());
        crate::npc::egg::set_passive_env(self.clone());
        crate::crafting::workbench::install_host(self.clone());
        let bh = Rc::new(BlockheadGlue(self.weak_self.clone()));
        crate::blockhead::blockhead::set_host(bh.clone());
        crate::blockhead::interaction_rules::set_actions_host(bh.clone());
        crate::blockhead::inventory_model::set_inventory_host(bh.clone());
        crate::physics_objects::fishing_rod::set_fishing_host(bh.clone());
        crate::physics_objects::fire_object::set_fire_host(bh.clone());
        crate::npc::cave_troll::set_hostile_host(bh);
    }

    /// Install the seams that need the `World` handle; called right after `World::init` returned `Some(w)`.
    pub fn install_post_init(&self, w: &Rc<World>) {
        *self.world.borrow_mut() = Rc::downgrade(w);
        crate::world::blocks::install_ops_hooks(w);
    }

    pub fn srv(&self) -> Option<Rc<BHServer>> {
        self.server.upgrade()
    }
    pub fn world(&self) -> Option<Rc<World>> {
        self.world.borrow().upgrade()
    }
    pub fn loader(&self) -> Option<Rc<WorldTileLoader>> {
        self.loader.borrow().clone()
    }
    pub fn dw(&self) -> Option<Rc<RealDw>> {
        self.dw.borrow().clone()
    }
    pub fn client(&self, id: &str) -> Option<Rc<ServerClient>> {
        self.clients.borrow().get(id).cloned()
    }
    /// `-[World dealloc]` releases: the DynamicWorld, then the WorldTileLoader (its dealloc commits a pending light
    /// block bulk transaction and closes the light environment). Also breaks the glue <-> DynamicWorld cycle.
    pub fn shutdown(&self) {
        let dw = self.dw.borrow_mut().take();
        if let Some(d) = dw {
            d.dealloc();
        }
        let l = self.loader.borrow_mut().take();
        if let Some(l) = l {
            l.dealloc();
        }
        self.clients.borrow_mut().clear();
        self.dw_clients.borrow_mut().remove_all();
    }
    /// The `ServerDynamicWorld` face of the live DynamicWorld (`None` until stage 12).
    pub fn dw_face(&self) -> Option<Rc<dyn ServerDynamicWorld>> {
        self.dw().map(|d| Rc::new(DwFace(d)) as Rc<dyn ServerDynamicWorld>)
    }
}

// =====================================================================================================
// BlockheadHost
// =====================================================================================================

/// The production [`BlockheadHost`](crate::blockhead::blockhead::BlockheadHost). Only the World / DynamicWorld
/// calls reached from `-[Blockhead updatePosition:]` (world loading, doors) are forwarded; every other method keeps
/// its inert default. The `ActionsHost` part of `updatePosition:` (glow blocks, torches, cave treasure) is not
/// wired yet.
///
/// Without it `updatePosition:` of a client's blockhead was the bare `DynamicObject` version: walking never ran
/// `fullyLoadIfNeededAroundPos:clientLightBlockIndex:forBlockhead:`, so the macro blocks the player walked into were
/// never loaded / explored for that client and stayed dark.
struct BlockheadGlue(Weak<Glue>);

impl BlockheadGlue {
    fn glue(&self) -> Option<Rc<Glue>> {
        self.0.upgrade()
    }
}

// =====================================================================================================
// WorldCoreHooks
// =====================================================================================================

impl WorldCoreHooks for Glue {
    // ---- BHServer
    fn server_send_network_data_to_peers_reliable(&self, data: &[u8], peers: Option<&[String]>, reliable: bool) {
        if let Some(s) = self.srv() {
            s.send_network_data_to_peers_reliable(data, peers, reliable);
        }
    }
    fn server_credit(&self) -> f32 {
        self.srv().map_or(0.0, |s| s.credit())
    }
    fn server_is_cloud_match(&self) -> bool {
        self.srv().map_or(false, |s| s.is_cloud_match())
    }
    fn server_privacy_string(&self) -> Option<String> {
        self.srv().and_then(|s| s.privacy_string())
    }
    fn server_match_privacy(&self) -> u64 {
        self.srv().and_then(|s| s.match_().and_then(|m| m.as_server_match().map(|sm| sm.privacy()))).unwrap_or(0)
    }
    fn server_database(&self) -> Option<Rc<Database>> {
        self.srv().and_then(|s| s.server_database())
    }
    fn server_start_bulk_transaction(&self) {
        if let Some(s) = self.srv() {
            s.start_bulk_transaction();
        }
    }
    fn server_finish_bulk_transaction(&self) -> bool {
        self.srv().map_or(true, |s| s.finish_bulk_transaction())
    }
    fn server_local_player_name(&self) -> String {
        self.srv().map(|s| s.local_player_name()).unwrap_or_default()
    }
    fn server_local_player_id(&self) -> Option<String> {
        self.srv().map(|s| s.local_player_id())
    }
    fn server_player_name_for_player_with_id_including_old_players(&self, id: Option<&str>) -> Option<String> {
        self.srv().and_then(|s| s.player_name_for_player_with_id_including_old_players(id))
    }

    // ---- ServerClient (one real object per peer)
    fn server_client_create(
        &self,
        client_id: &str,
        light_block_index: i32,
        light_block_database: Option<Rc<Database>>,
    ) {
        let Some(s) = self.srv() else { return };
        let sc = ServerClient::init_with_client_id_server_light_block_index_light_block_database(
            client_id,
            &s,
            light_block_index,
            light_block_database,
        );
        self.clients.borrow_mut().insert(client_id.to_string(), sc.clone());
        let face: Rc<dyn DwServerClient> = sc;
        self.dw_clients.borrow_mut().insert(client_id.to_string(), face);
    }
    fn server_client_connected(&self, client_id: &str) -> bool {
        self.client(client_id).map_or(false, |c| c.connected())
    }
    fn server_client_set_connected(&self, client_id: &str, connected: bool) {
        if let Some(c) = self.client(client_id) {
            c.set_connected(connected);
        }
    }
    fn server_client_reconnected(&self, client_id: &str) {
        if let Some(c) = self.client(client_id) {
            c.client_reconnected();
        }
    }
    fn server_client_light_block_index(&self, client_id: &str) -> i32 {
        self.client(client_id).map_or(0, |c| c.light_block_index())
    }
    fn server_client_request_for_block(
        &self,
        client_id: &str,
        request: crate::types::netdata::ClientMacroBlockRequest,
    ) {
        if let Some(c) = self.client(client_id) {
            c.request_for_block(request);
        }
    }
    fn server_client_block_removed(&self, client_id: &str, macro_index: i32) {
        if let Some(c) = self.client(client_id) {
            c.block_removed(macro_index);
        }
    }
    fn server_client_block_is_wired(&self, client_id: &str, macro_index: i32) -> bool {
        self.client(client_id).map_or(false, |c| c.block_is_wired(macro_index))
    }
    fn server_client_paused(&self, client_id: &str) -> bool {
        self.client(client_id).map_or(false, |c| c.paused())
    }
    fn server_client_set_paused(&self, client_id: &str, paused: bool) {
        if let Some(c) = self.client(client_id) {
            c.set_paused(paused);
        }
    }

    // ---- WorldTileLoader: constructed exactly once, retained for the World's lifetime
    fn loader_init(
        &self,
        world: &Rc<World>,
        random_seed: u32,
        is_new_world: bool,
        save_id: &str,
        loaded_version: i32,
        block_database: Option<Rc<Database>>,
    ) -> Option<u32> {
        let l = WorldTileLoader::init_with_world_random_seed_is_new_world_save_id_loaded_version_block_database(
            world,
            random_seed,
            is_new_world,
            save_id,
            loaded_version,
            block_database,
        );
        let seed = l.random_seed();
        *self.loader.borrow_mut() = Some(l);
        Some(seed)
    }
    fn loader_refine_terrain(&self) {
        if let Some(l) = self.loader() {
            l.refine_terrain();
        }
    }
    fn loader_highest_point(&self) -> IntPair {
        self.loader().map_or(IntPair { x: 0, y: 0 }, |l| l.highest_point())
    }
    fn loader_best_start_position(&self) -> IntPair {
        self.loader().map_or(IntPair { x: 0, y: 0 }, |l| l.best_start_position())
    }
    fn loader_distance_ordered_food_types(&self) -> Vec<i32> {
        self.loader().map_or_else(|| vec![0; 31], |l| l.distance_ordered_food_types().to_vec())
    }
    fn loader_x_frequency_multiplier(&self) -> i32 {
        self.loader().map_or(0, |l| l.x_frequency_multiplier())
    }
    fn loader_light_block_database(&self) -> Option<Rc<Database>> {
        self.loader().and_then(|l| l.light_block_database())
    }
    fn loader_start_bulk_light_block_transaction(&self) {
        if let Some(l) = self.loader() {
            l.start_bulk_light_block_transaction();
        }
    }
    fn loader_finish_bulk_light_block_transaction(&self) -> bool {
        self.loader().map_or(true, |l| l.finish_bulk_light_block_transaction())
    }
    fn loader_unarchive_light_blocks_for_client(&self, client_id: &str) {
        if let Some(l) = self.loader() {
            l.unarchive_light_blocks_for_client(client_id);
        }
    }
    fn loader_set_needs_to_exit(&self, needs_to_exit: bool) {
        if let Some(l) = self.loader() {
            l.set_needs_to_exit(needs_to_exit);
        }
    }
    fn loader_update_physical_block_to_latest_version(&self, pb: PhysicalBlockRef) {
        if let Some(l) = self.loader() {
            l.update_physical_block_to_latest_version(pb);
        }
    }

    // ---- weather / path / projectile
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
        let n = NoiseFunction::init_with_frequency_x_frequency_y_frequency_z_amplitude_seed_tileable_loop_persistance(
            fx,
            fy,
            fz,
            amplitude,
            seed,
            tileable,
            loop_,
            persistance,
        );
        *self.weather_noise.borrow_mut() = Some(Rc::new(n));
    }
    fn weather_init(&self, world: &Rc<World>, world_time: f32) {
        *self.weather.borrow_mut() =
            Some(crate::env::weather::Weather::init_with_cache_world_world_time(None, world, world_time));
    }
    fn path_creator_init(&self, world: &Rc<World>) {
        *self.path_creator.borrow_mut() = Some(crate::path::path_creator::PathCreator::init_with_world(world));
    }
    fn path_creator_in_progress(&self) -> bool {
        self.path_creator.borrow().as_ref().map_or(false, |p| p.in_progress())
    }
    fn path_creator_path_user(&self) -> Option<ObjRef> {
        self.path_creator.borrow().as_ref().and_then(|p| p.path_user())
    }
    fn path_creator_abort_path(&self) {
        let p = self.path_creator.borrow().clone();
        if let Some(p) = p {
            p.abort_path();
        }
    }
    fn projectile_manager_init(&self, world: &Rc<World>) {
        *self.projectile_manager.borrow_mut() =
            Some(crate::physics_objects::projectile_manager::ProjectileManager::init_with_world_cache(world, None));
    }

    // ---- DynamicWorld
    fn dynamic_world_create(&self, world: &Rc<World>) -> Option<Rc<PlaceholderDw>> {
        let main = world.main_database.borrow().clone()?;
        let dwdb = world.dynamic_object_database.borrow().clone()?;
        let loader = self.loader();
        let gen: Option<Rc<dyn DwWorldGen>> =
            loader.clone().map(|l| Rc::new(LoaderAdapter::new(l)) as Rc<dyn DwWorldGen>);
        let tree: Option<Rc<dyn DwNoise>> = loader
            .as_ref()
            .and_then(|l| l.tree_density_noise_function())
            .map(|n| Rc::new(NoiseAdapter(n)) as Rc<dyn DwNoise>);
        let season: Option<Rc<dyn DwNoise>> = loader
            .as_ref()
            .and_then(|l| l.season_offset_noise_function())
            .map(|n| Rc::new(NoiseAdapter(n)) as Rc<dyn DwNoise>);
        let server: Option<Rc<dyn DwServer>> = self.srv().map(|s| s as Rc<dyn DwServer>);
        let dw = RealDw::new(DynamicWorldInit {
            world: world.clone(),
            world_tile_loader: gen,
            server,
            server_clients: Some(self.dw_clients.clone()),
            cache: None,
            tree_density_noise_function: tree,
            season_offset_noise_function: season,
            app_database: None,
            world_database: main,
            dynamic_object_database: dwdb,
            world_save_directory_override: None,
        });
        if let Some(me) = self.weak_self.upgrade() {
            dw.set_hooks(me);
        }
        *self.dw.borrow_mut() = Some(dw);
        Some(Rc::new(PlaceholderDw::new(Box::new(Queries { glue: self.weak_self.clone() }))))
    }
    fn dynamic_world_load_dynamic_objects_reposition_blockhead_load_failures(
        &self,
        did_load_game: bool,
        reposition: bool,
    ) -> bool {
        match self.dw() {
            Some(d) => d.load_dynamic_objects_reposition_blockhead_load_failures(did_load_game, reposition),
            None => true,
        }
    }
    fn dynamic_world_conversion_pump(&self) {
        if let Some(d) = self.dw() {
            d.conversion_pump();
        }
    }
    fn dynamic_world_send_lightblocks_to_clients(&self) {
        if let Some(d) = self.dw() {
            d.send_lightblocks_to_clients();
        }
    }
    fn dynamic_world_save_and_send_only_blocks_that_need_to_be_sent(&self) {
        if let Some(d) = self.dw() {
            d.save_and_send_only_blocks_that_need_to_be_sent();
        }
    }
    fn dynamic_world_save_game_with_world_data_sign_ownership_data(
        &self,
        world_data: Option<&[u8]>,
        sign_data: Option<&[u8]>,
    ) {
        if let Some(d) = self.dw() {
            d.save_game_with_world_data_sign_ownership_data(world_data, sign_data);
        }
    }
    fn dynamic_world_portal_level(&self) -> Option<i32> {
        let d = self.dw()?;
        let p = d.portal()?;
        Some(d.hooks().workbench_level(&p))
    }
    fn dynamic_world_blockheads(&self) -> Vec<ObjRef> {
        self.dw().map_or_else(Vec::new, |d| d.blockheads())
    }
    fn dynamic_world_all_blockheads_including_net(&self) -> Vec<ObjRef> {
        self.dw().map_or_else(Vec::new, |d| d.all_blockheads_including_net())
    }
    fn dynamic_world_client_connected(&self, client_id: &str) {
        if let Some(d) = self.dw() {
            d.client_connected(Some(client_id));
        }
    }
    fn dynamic_world_client_disconnected_simulate(&self, client_id: &str, simulate: bool) {
        if let Some(d) = self.dw() {
            d.client_disconnected_simulate(Some(client_id), simulate);
        }
    }
    fn dynamic_world_stop_all_blockhead_actions_for_client_due_to_kick(&self, client_id: &str) {
        if let Some(d) = self.dw() {
            d.stop_all_blockhead_actions_for_client_due_to_kick(client_id);
        }
    }
    fn dynamic_world_is_controlling_blockheads_for_client_player(&self, player_id: &str) -> bool {
        self.dw().map_or(false, |d| d.is_controlling_blockheads_for_client_player(player_id))
    }
    fn dynamic_world_light_changed_at_macro_pos_send_reliably(&self, macro_pos: IntPair, send_reliably: bool) {
        if let Some(d) = self.dw() {
            d.light_changed_at_macro_pos_send_reliably(macro_pos, send_reliably);
        }
    }
    fn dynamic_world_remote_create(&self, objects: Vec<Vec<u8>>, object_type: i32, client_id: Option<&str>) {
        if let Some(d) = self.dw() {
            d.remote_create_for_objects_of_type_client_id(objects, object_type, client_id.unwrap_or(""));
        }
    }
    fn dynamic_world_remote_update(&self, objects: Vec<Vec<u8>>, object_type: i32, client_id: Option<&str>) {
        if let Some(d) = self.dw() {
            d.remote_update_for_objects_of_type_from_client(objects, object_type, client_id);
        }
    }
    fn dynamic_world_remote_creation_data_update(
        &self,
        objects: Vec<Vec<u8>>,
        object_type: i32,
        client_id: Option<&str>,
    ) {
        if let Some(d) = self.dw() {
            d.remote_creation_data_update_for_objects_of_type_from_client(objects, object_type, client_id);
        }
    }
    fn dynamic_world_remote_remove(&self, objects: Vec<Vec<u8>>, object_type: i32, client_id: Option<&str>) {
        if let Some(d) = self.dw() {
            d.remote_remove_for_objects_of_type_from_client(objects, object_type, client_id);
        }
    }
    fn dynamic_world_user_mute_changed(&self, player_id: &str) {
        if let Some(d) = self.dw() {
            d.user_mute_changed(player_id);
        }
    }
    fn dynamic_world_players_changed(&self) {
        if let Some(d) = self.dw() {
            d.players_changed();
        }
    }
    fn dynamic_world_append_debug_log(&self, log: &mut String) {
        if let Some(d) = self.dw() {
            d.append_debug_log(log);
        }
    }
    fn dynamic_world_finish_simulating(&self) {
        if let Some(d) = self.dw() {
            d.finish_simulating();
        }
    }
    fn dynamic_world_simulate(&self, dt: f32) {
        if let Some(d) = self.dw() {
            d.simulate(dt);
        }
    }
    fn dynamic_world_set_paused(&self, paused: bool) {
        if let Some(d) = self.dw() {
            d.set_paused(paused);
        }
    }

    // ---- World helpers
    fn world_fully_load_if_needed_around_pos_client_light_block_index(
        &self,
        world: &World,
        pos: IntPair,
        client_light_block_index: i32,
    ) {
        world.fully_load_if_needed_around_pos_client_light_block_index_for_blockhead(
            pos,
            client_light_block_index,
            None,
        );
    }
    fn world_place_workbench_of_type_at_pos(&self, world: &World, workbench_type: i32, pos: IntPair) {
        world.place_workbench_of_type_at_pos_save_dict_placed_by_client_placed_by_blockhead_placed_by_client_name(
            crate::types::enums::WorkbenchType(workbench_type as u32),
            pos,
            None,
            None,
            None,
            None,
        );
    }
    fn world_helper_check_if_macro_tile_can_be_decommissioned(
        &self,
        world: &World,
        mt: Option<&MacroTileRef>,
        min_age: f64,
        block_to_save: Option<PhysicalBlockRef>,
    ) {
        if let (Some(w), Some(mt)) = (self.world(), mt) {
            let _ = world;
            crate::lighting::sunlight::WorldHelper::check_if_macro_tile_can_be_decommissioned_world_min_age_block_to_save_physcial_block(mt, &w, min_age, block_to_save.is_some());
        }
    }
    fn tip_manager_set_world_and_reset(&self, world: &Rc<World>) {
        let t = crate::env::tip_manager::TipManager::instance();
        t.set_world(world);
        t.reset();
    }
    fn tip_manager_update(&self, dt: f32) {
        crate::env::tip_manager::TipManager::instance().update(dt);
    }

    // ---- Blockhead (r2_session_0 (b)): the owning client of a blockhead
    fn blockhead_client_id(&self, b: &ObjRef) -> Option<String> {
        bh_of(b).and_then(|bh| bh.client_id())
    }

    fn projectile_manager_fire(
        &self,
        from: crate::types::structs::Vector2,
        to: crate::types::structs::Vector2,
        at: Option<&crate::objects::handle::ObjRef>,
        item_type: i32,
        firer: Option<&crate::objects::handle::ObjRef>,
    ) {
        let p = self.projectile_manager.borrow().clone();
        if let Some(p) = p {
            p.fire_projectile_from_to_at_fire_item_type_firer(from, to, at, item_type, firer);
        }
    }
    fn dynamic_world_set_server_server_clients(&self, has_server: bool) {
        if let Some(d) = self.dw() {
            let server: Option<Rc<dyn DwServer>> =
                if has_server { self.srv().map(|s| s as Rc<dyn DwServer>) } else { None };
            d.set_server_server_clients(server, has_server.then(|| self.dw_clients.clone()));
        }
    }
    fn dynamic_world_blockhead_previews(&self) -> Vec<crate::plist::value::PlistValue> {
        self.dw().map_or_else(Vec::new, |d| {
            d.blockheads().iter().filter_map(bh_of).map(|b| PlistValue::data(b.preview_data())).collect()
        })
    }
    fn blockhead_name(&self, b: &crate::objects::handle::ObjRef) -> Option<String> {
        bh_of(b).and_then(|x| x.name())
    }
    fn blockhead_is_client_blockhead_being_controlled_by_server(&self, b: &crate::objects::handle::ObjRef) -> bool {
        bh_of(b).is_some_and(|x| x.is_client_blockhead_being_controlled_by_server())
    }
    fn blockhead_should_continue_simulating(&self, b: &crate::objects::handle::ObjRef) -> bool {
        bh_of(b).is_some_and(|x| x.should_continue_simulating())
    }
    fn blockhead_suffer_damage_is_simulation_recoil(
        &self,
        b: &crate::objects::handle::ObjRef,
        damage: f32,
        is_simulation: bool,
        recoil: bool,
    ) {
        if let Some(x) = bh_of(b) {
            x.suffer_damage_is_simulation_recoil(damage, is_simulation, recoil);
        }
    }
    fn blockhead_die_for_good(&self, b: &crate::objects::handle::ObjRef) {
        if let Some(x) = bh_of(b) {
            x.die_for_good();
        }
    }
    fn blockhead_asleep(&self, b: &crate::objects::handle::ObjRef) -> bool {
        bh_of(b).is_some_and(|x| x.asleep())
    }
    fn blockhead_regenerating(&self, b: &crate::objects::handle::ObjRef) -> bool {
        bh_of(b).is_some_and(|x| x.regenerating())
    }
    fn blockhead_meditating(&self, b: &crate::objects::handle::ObjRef) -> bool {
        bh_of(b).is_some_and(|x| x.meditating())
    }
    fn blockhead_on_trade_mission(&self, b: &crate::objects::handle::ObjRef) -> bool {
        bh_of(b).is_some_and(|x| x.on_trade_mission())
    }
    fn blockhead_has_actions(&self, b: &crate::objects::handle::ObjRef) -> bool {
        bh_of(b).is_some_and(|x| x.has_actions())
    }
    fn blockhead_action_count(&self, b: &crate::objects::handle::ObjRef) -> i32 {
        bh_of(b).map_or(0, |x| x.action_count())
    }
    fn blockhead_cancel_simulate_due_to_collapse(&self, b: &crate::objects::handle::ObjRef) -> bool {
        bh_of(b).is_some_and(|x| x.cancel_simulate_due_to_collapse())
    }
    fn blockhead_current_interaction_requires_human_input(&self, b: &crate::objects::handle::ObjRef) -> bool {
        bh_of(b).is_some_and(|x| x.current_interaction_requires_human_input())
    }
    fn world_load_light_block_for_client_light_block_index_into_physical_block(
        &self,
        world: &crate::world::state::World,
        index: i32,
        pb: crate::world::state::PhysicalBlockRef,
    ) {
        world.load_light_block_for_client_light_block_index_into_physical_block(index, &pb)
    }
    fn world_welcome_back_message_for_simulation_events(&self, world: &crate::world::state::World) -> Option<String> {
        world.welcome_back_message_for_simulation_events()
    }
    fn world_pause_button_tapped(&self, world: &crate::world::state::World) {
        // `-[World pauseButtonTapped]` is empty in the server build (CLIENT-ONLY UI): nothing to do.
        let _ = world;
    }
    fn world_path_request_pump(&self, world: &crate::world::state::World) {
        world.update_path_pump(true)
    }
    fn particle_emitter_set_world_width_macro(&self, width_macro: i32) {
        crate::env::particle_emitter::ParticleEmitter::instance().set_world_width_macro(width_macro)
    }
    fn particle_emitter_do_add_electricity_particle_with_path_size(
        &self,
        path: &[crate::types::structs::ElectrictyParticlePathIndex],
        size: f32,
    ) {
        crate::env::particle_emitter::ParticleEmitter::instance()
            .do_add_electricity_particle_with_path_size(path.to_vec(), size)
    }
    fn tip_manager_display_tip(&self, text: &str) {
        crate::env::tip_manager::TipManager::instance().display_tip_with_time_out_display_even_if_disabled_tip_color(
            text,
            10.0,
            false,
            [1.0, 1.0, 1.0, 1.0],
        )
    }
    fn tip_manager_current_tip(&self) -> Option<String> {
        crate::env::tip_manager::TipManager::instance().current_tip()
    }
}

// =====================================================================================================
// WorldgenHost
// =====================================================================================================

impl WorldgenHost for Glue {
    fn client_info(&self, _world: &World, client_id: &str) -> Option<(bool, i32)> {
        self.client(client_id).map(|c| (c.connected(), c.light_block_index()))
    }
    /// `[sc getAndRemoveAllRecieptDataForMacroPos:pos world:world]` (r2_session_0 (c)).
    fn client_receipts(&self, world: &World, client_id: &str, pos: IntPair) -> Option<Vec<u32>> {
        let sc = self.client(client_id)?;
        let w = self.world()?;
        let _ = world;
        let face = crate::app::game_controller::WorldServerAdapter { world: w, glue: None };
        let data = sc.get_and_remove_all_reciept_data_for_macro_pos_world(pos, &face)?;
        let PlistValue::Array(a) = data else { return None };
        Some(a.items.iter().map(|v| v.unsigned_int_value()).collect())
    }
    fn client_contains_light_block_index(&self, _world: &World, client_id: &str, index: u64) -> bool {
        self.client(client_id).map_or(false, |c| c.all_light_block_indices().borrow().contains_index(index))
    }
    fn client_add_light_block_index(&self, _world: &World, client_id: &str, index: u64) {
        if let Some(c) = self.client(client_id) {
            c.all_light_block_indices().borrow_mut().add_index(index);
        }
    }
    fn client_save_light_block_indices(&self, _world: &World, client_id: &str) {
        if let Some(c) = self.client(client_id) {
            c.save_light_block_indices();
        }
    }
    fn send_network_data_to_peers_reliable(
        &self,
        _server: &Rc<WorldServerHandle>,
        data: &[u8],
        peers: &[String],
        reliable: bool,
    ) {
        if let Some(s) = self.srv() {
            s.send_network_data_to_peers_reliable(data, peers, reliable);
        }
    }
    fn player_is_admin_with_id(&self, _server: &Rc<WorldServerHandle>, client_id: &str) -> bool {
        self.srv().map_or(false, |s| s.player_is_admin_with_id(Some(client_id)))
    }
    fn initial_dynamic_objects_net_data(
        &self,
        _world: &World,
        macro_index: i32,
        client_id: &str,
    ) -> Option<DynamicObjectsNetData> {
        let d = self.dw()?;
        let n = d.initial_dynamic_objects_net_data_for_macro_tile_index_wire_for_client(macro_index, client_id)?;
        let conv = |v: Vec<(u32, Vec<Vec<u8>>)>| -> Vec<(u32, PlistValue)> {
            v.into_iter()
                .map(|(t, blobs)| (t, PlistValue::array(blobs.into_iter().map(PlistValue::data).collect())))
                .collect()
        };
        Some(DynamicObjectsNetData { creation: conv(n.creation_data), update: conv(n.update_data) })
    }
    fn create_treasure_chest_or_troll_at_tile(
        &self,
        _world: &World,
        tile: TileRef,
        pos: IntPair,
        load_troll: bool,
        load_treasure: bool,
    ) {
        if let Some(d) = self.dw() {
            d.create_treasure_chest_or_troll_at_tile_at_pos_load_troll_load_treasure(
                &tile,
                pos,
                load_troll,
                load_treasure,
            );
        }
    }
    fn decommision_all_blocks(&self, world: &World) {
        world.decommision_all_blocks_block_to_save_physcial_block(true);
    }
    fn fully_load_if_needed_around_pos(&self, world: &World, pos: IntPair, client_light_block_index: i32) {
        world.fully_load_if_needed_around_pos_client_light_block_index_for_blockhead(
            pos,
            client_light_block_index,
            None,
        );
    }
}

// =====================================================================================================
// WorldOpsDeps (forwarding subset: loader, ServerClient, BHServer, DynamicWorld, World siblings, singletons)
// =====================================================================================================

impl WorldOpsDeps for Glue {
    // ---- DynamicWorld
    fn path_users(&self) -> Vec<ObjRef> {
        self.dw().map_or_else(Vec::new, |d| d.path_users())
    }
    fn workbench_at_pos(&self, pos: IntPair) -> Option<ObjRef> {
        self.dw().and_then(|d| d.workbench_at_pos(pos))
    }
    fn active_blockhead(&self) -> Option<ObjRef> {
        self.dw().and_then(|d| d.active_blockhead())
    }
    fn all_blockheads_including_net(&self) -> Vec<ObjRef> {
        self.dw().map_or_else(Vec::new, |d| d.all_blockheads_including_net())
    }
    fn blockhead_with_id_including_net(&self, id: u64) -> Option<ObjRef> {
        self.dw().and_then(|d| d.blockhead_with_id_including_net(id))
    }
    fn blockheads(&self) -> Vec<ObjRef> {
        self.dw().map_or_else(Vec::new, |d| d.blockheads())
    }
    fn remove_dynamic_objects_belonging_to_client(&self, client_id: &str) {
        if let Some(d) = self.dw() {
            d.remove_dynamic_objects_belonging_to_client(client_id);
        }
    }
    fn update_accurate_dt(&self, dt: f32, accurate_dt: f32) {
        if let Some(d) = self.dw() {
            d.update_accurate_dt(dt, accurate_dt);
        }
    }
    fn dynamic_world_set_paused(&self, paused: bool) {
        if let Some(d) = self.dw() {
            d.set_paused(paused);
        }
    }
    fn world_changed_at_pos_send_reliably(&self, pos: IntPair, reliably: bool) {
        if let Some(d) = self.dw() {
            d.world_changed_at_pos_send_reliably(pos, reliably);
        }
    }
    fn world_contents_changed_at_pos(&self, pos: IntPair) {
        if let Some(d) = self.dw() {
            d.world_contents_changed_at_pos(pos);
        }
    }
    fn water_changed_at_pos_full_block(&self, pos: IntPair, full_block: bool) {
        if let Some(d) = self.dw() {
            d.water_changed_at_pos_full_block(pos, full_block);
        }
    }
    fn load_surface_block_at_pos(&self, pos: IntPair) {
        if let Some(d) = self.dw() {
            d.load_surface_block_at_pos(pos);
        }
    }
    fn load_snow_surface_block_at_pos_load_snow(&self, pos: IntPair, load_snow: bool) {
        if let Some(d) = self.dw() {
            d.load_snow_surface_block_at_pos_load_snow(pos, load_snow);
        }
    }
    fn load_glow_block_if_needed_at_pos_tile(&self, pos: IntPair, tile: &TileRef) {
        if let Some(d) = self.dw() {
            d.load_glow_block_if_needed_at_pos_tile(pos, tile);
        }
    }
    fn load_gather_block_at_pos(&self, pos: IntPair) {
        if let Some(d) = self.dw() {
            d.load_gather_block_at_pos(pos);
        }
    }
    fn play_time_crystal_received_sound_at_pos(&self, pos: IntPair) {
        if let Some(d) = self.dw() {
            d.play_time_crystal_received_sound_at_pos(pos);
        }
    }
    fn client_blockhead_inventory_recieved_for_player_id_blockhead_id_data(
        &self,
        player_id: Option<&str>,
        blockhead_id: u64,
        data: &[u8],
    ) {
        if let (Some(d), Some(p)) = (self.dw(), player_id) {
            d.client_blockhead_inventory_recieved_for_player_id_blockhead_id_data(p, blockhead_id, data);
        }
    }
    fn load_dynamic_objects_for_macro_tile_include_surface_blocks(
        &self,
        mt: &MacroTileRef,
        include_surface_blocks: bool,
    ) {
        if let Some(d) = self.dw() {
            d.load_dynamic_objects_for_macro_tile_include_surface_blocks(mt, include_surface_blocks);
        }
    }
    fn remove_dynamic_objects_for_macro_tile(&self, mt: &MacroTileRef) {
        if let Some(d) = self.dw() {
            d.remove_dynamic_objects_for_macro_tile(mt);
        }
    }
    fn add_artificial_light_contribution_for_physical_block_loaded_at_x_pos_y_pos(&self, x: i32, y: i32) {
        if let Some(d) = self.dw() {
            d.add_artificial_light_contribution_for_physical_block_loaded_at_x_pos_y_pos(x, y);
        }
    }
    fn dynamic_world_do_repair_for_tile_at_pos(&self, pos: IntPair) {
        if let Some(d) = self.dw() {
            d.do_repair_for_tile_at_pos(pos);
        }
    }

    // ---- WorldTileLoader
    fn load_physical_block_at_x_pos_y_pos_create_if_not_created(
        &self,
        pb: PhysicalBlockRef,
        x: i32,
        y: i32,
        create_if_not_created: bool,
    ) {
        let (Some(l), Some(w)) = (self.loader(), self.world()) else { return };
        let mt = crate::world::tiles::macro_tile_at_macro_postion(x, y, &w.macro_tiles(), &w);
        let unexplored = mt.as_ref().map_or(false, |m| m.unexplored.get() != 0);
        let out =
            l.load_physical_block_at_x_pos_y_pos_create_if_not_created(pb, x, y, create_if_not_created, unexplored);
        for m in mt {
            if matches!(out, LoadOutcome::NotFound) {
                m.unexplored.set(1);
            } else if matches!(out, LoadOutcome::Loaded | LoadOutcome::Generated) {
                *m.physical_block.borrow_mut() = Some(pb);
                m.unexplored.set(0);
                m.dynamic_objects_loaded.set(false);
            }
        }
    }
    fn save_physical_block_macro_tile_send_to_clients_server_send_reliably(
        &self,
        pb: PhysicalBlockRef,
        _mt: &MacroTileRef,
        send_to_clients: &[String],
        send_reliably: bool,
    ) {
        if let Some(l) = self.loader() {
            l.save_physical_block_send_to_clients_server_send_reliably(
                pb,
                send_to_clients,
                self.world().and_then(|w| w.server()).as_ref(),
                send_reliably,
            );
        }
    }
    fn load_light_block_for_client_light_block_index_client_id_into_physical_block(
        &self,
        idx: i32,
        client_id: &str,
        pb: PhysicalBlockRef,
    ) {
        if let Some(l) = self.loader() {
            l.load_light_block_for_client_light_block_index_client_id_into_physical_block(idx, client_id, pb);
        }
    }
    fn save_light_block_for_client_light_block_index_client_id_physical_block_send_now(
        &self,
        idx: i32,
        client_id: &str,
        pb: PhysicalBlockRef,
        send_now: bool,
    ) {
        if let Some(l) = self.loader() {
            l.save_light_block_for_client_light_block_index_client_id_physical_block_send_now(
                idx, client_id, pb, send_now,
            );
        }
    }
    fn send_block_to_client_without_saving_for_block_pos_send_to_client_server_send_dynamic_objects_reliable(
        &self,
        pb: PhysicalBlockRef,
        macro_pos: IntPair,
        client_id: &str,
        send_dynamic_objects: bool,
        reliable: bool,
    ) -> bool {
        match self.loader() {
            Some(l) => l
                .send_block_to_client_without_saving_for_block_pos_send_to_client_server_send_dynamic_objects_reliable(
                    pb,
                    macro_pos,
                    Some(client_id),
                    self.world().and_then(|w| w.server()).as_ref(),
                    send_dynamic_objects,
                    reliable,
                ),
            None => false,
        }
    }
    fn unmodified_ground_level_for_x(&self, x: i32) -> i32 {
        self.loader().map_or(0, |l| l.unmodified_ground_level_for_x(x))
    }
    fn sand_fraction_for_pos_high_res(&self, pos: IntPair, high_res: bool) -> f32 {
        self.loader().map_or(0.0, |l| l.sand_fraction_for_pos_high_res(pos, high_res))
    }
    fn archive_light_blocks_for_client(&self, client_id: &str) {
        if let Some(l) = self.loader() {
            l.archive_light_blocks_for_client(client_id);
        }
    }

    // ---- ServerClient by peer id
    fn sc_connected(&self, peer: &str) -> bool {
        self.client(peer).map_or(false, |c| c.connected())
    }
    fn sc_requested_block_indices_first_n(&self, peer: &str, n: usize) -> Vec<u64> {
        match self.client(peer) {
            Some(c) => c.requested_block_indices().iter().take(n).collect(),
            None => Vec::new(),
        }
    }
    fn sc_create_if_not_created_for_block_request(&self, peer: &str, index: i32) -> bool {
        self.client(peer).map_or(false, |c| c.create_if_not_created_for_block_request(index))
    }
    fn sc_remove_block_request(&self, peer: &str, index: i32) {
        if let Some(c) = self.client(peer) {
            c.remove_block_request(index);
        }
    }
    fn sc_block_is_wired(&self, peer: &str, macro_index: i32) -> bool {
        self.client(peer).map_or(false, |c| c.block_is_wired(macro_index))
    }
    fn sc_requests_heart_beat(&self, peer: &str) -> bool {
        self.client(peer).map_or(false, |c| c.requests_heart_beat())
    }
    fn sc_set_requests_heart_beat(&self, peer: &str, v: bool) {
        if let Some(c) = self.client(peer) {
            c.set_requests_heart_beat(v);
        }
    }
    fn sc_set_has_ever_requested_heartbeat(&self, peer: &str, v: bool) {
        if let Some(c) = self.client(peer) {
            c.set_has_ever_requested_heartbeat(v);
        }
    }
    fn sc_set_time_since_last_heartbeat_request(&self, peer: &str, v: f32) {
        if let Some(c) = self.client(peer) {
            c.set_time_since_last_heartbeat_request(v);
        }
    }
    fn sc_light_block_index(&self, peer: &str) -> i32 {
        self.client(peer).map_or(-1, |c| c.light_block_index())
    }
    fn sc_paused(&self, peer: &str) -> bool {
        self.client(peer).map_or(false, |c| c.paused())
    }

    // ---- BHServer
    fn send_network_data_to_peers_reliable(&self, data: &[u8], peers: Option<&[String]>, reliable: bool) {
        if let Some(s) = self.srv() {
            s.send_network_data_to_peers_reliable(data, peers, reliable);
        }
    }
    fn player_is_admin_with_id(&self, client_id: Option<&str>) -> bool {
        self.srv().map_or(false, |s| s.player_is_admin_with_id(client_id))
    }
    fn update_player(&self, client_id: &str) {
        if let Some(s) = self.srv() {
            s.update_player(client_id);
        }
    }
    fn player_alias_for_peer_id(&self, client_id: Option<&str>) -> Option<String> {
        self.srv().and_then(|s| s.player_alias_for_peer_id(client_id))
    }
    fn client_finished_away_simulation(&self, client_id: &str) {
        if let Some(s) = self.srv() {
            s.client_finished_away_simulation(client_id);
        }
    }
    fn send_player_changed_notifcation_to_delegate(&self) {
        if let Some(s) = self.srv() {
            s.notify_players_changed();
        }
    }

    // ---- World siblings
    fn heartbeat_data(&self) -> Vec<u8> {
        self.world().map_or_else(Vec::new, |w| w.heartbeat_data())
    }
    fn send_heartbeat_data(&self) {
        if let Some(w) = self.world() {
            w.send_heartbeat_data();
        }
    }
    fn is_controlling_blockheads_for_client_player(&self, client_id: &str) -> bool {
        self.dw().map_or(false, |d| d.is_controlling_blockheads_for_client_player(client_id))
    }

    // ---- WorldHelper (lighting_env)
    fn update_sun_light_for_tile_at_pos_world(&self, tile: &TileRef, pos: IntPair) {
        if let Some(w) = self.world() {
            crate::lighting::sunlight::WorldHelper::update_sun_light_for_tile_at_pos_world(Some(*tile), pos, &w);
        }
    }
    fn update_sun_light_removed_for_tile_at_pos_world(&self, tile: &TileRef, pos: IntPair) {
        if let Some(w) = self.world() {
            crate::lighting::sunlight::WorldHelper::update_sun_light_removed_for_tile_at_pos_world(
                Some(*tile),
                pos,
                &w,
            );
        }
    }
    fn recalculate_lighting_for_physical_block_if_needed_world_client_light_block_index_for_blockhead(
        &self,
        pb: Option<PhysicalBlockRef>,
        client_light_block_index: i32,
        blockhead: Option<&ObjRef>,
    ) {
        if let Some(w) = self.world() {
            crate::lighting::sunlight::WorldHelper::recalculate_lighting_for_physical_block_if_needed_world_client_light_block_index_for_blockhead(pb.as_ref(), &w, client_light_block_index, blockhead);
        }
    }
    fn check_if_macro_tile_can_be_decommissioned_world_min_age_block_to_save_physcial_block(
        &self,
        mt: Option<&MacroTileRef>,
        min_age: f64,
        block_to_save: bool,
    ) {
        if let (Some(w), Some(mt)) = (self.world(), mt) {
            crate::lighting::sunlight::WorldHelper::check_if_macro_tile_can_be_decommissioned_world_min_age_block_to_save_physcial_block(mt, &w, min_age, block_to_save);
        }
    }

    // ---- singletons
    fn path_creator(&self) -> Option<Rc<crate::path::path_creator::PathCreator>> {
        self.path_creator.borrow().clone()
    }
    fn projectile_manager_update(&self, dt: f32) {
        let p = self.projectile_manager.borrow().clone();
        if let Some(p) = p {
            p.update(dt);
        }
    }
    fn weather_noise_get_x_y_z_octaves(&self, x: f64, y: f64, z: f64, octaves: i32) -> f64 {
        self.weather_noise.borrow().as_ref().map_or(0.0, |n| n.get_x_y_z_octaves(x, y, z, octaves))
    }
    fn weather_wind_strength(&self) -> f32 {
        self.weather.borrow().as_ref().map_or(0.0, |w| w.wind_strength())
    }

    // ---- DynamicWorld factories (r2_cli_0 / r2_worldgen_0 / r2_console_0/1 / r2_session_0)
    fn workbench_placed_at_position_of_type_save_dict_placed_by_client_client_name(
        &self,
        pos: IntPair,
        t: crate::types::enums::WorkbenchType,
        save_dict: Option<&PlistValue>,
        client_id: Option<&str>,
        client_name: Option<&str>,
    ) {
        if let Some(d) = self.dw() {
            let _ = d.workbench_placed_at_position_of_type_save_dict_placed_by_client_client_name(
                pos,
                t.0 as i32,
                dict_of(save_dict),
                client_id,
                client_name,
            );
        }
    }
    fn create_free_block_at_position_of_type_data_a_data_b_sub_items_dynamic_object_save_dict_hovers_play_sound_priority_blockhead(
        &self,
        pos: IntPair,
        item_type: crate::types::item_type::ItemType,
        data_a: u16,
        data_b: u16,
        sub_items: Option<&[crate::items::inventory_item::SubSlot]>,
        dynamic_object_save_dict: Option<&PlistValue>,
        hovers: bool,
        play_sound: i32,
        priority_blockhead: Option<&ObjRef>,
    ) {
        if let Some(d) = self.dw() {
            d.create_free_block_at_position_of_type_data_a_data_b_sub_items_dynamic_object_save_dict_hovers_play_sound_priority_blockhead(
                pos,
                item_type,
                data_a,
                data_b,
                sub_items.map(sub_items_to_owned),
                dynamic_object_save_dict.cloned(),
                hovers,
                play_sound,
                priority_blockhead,
            );
        }
    }
    fn create_free_block_at_position_for_foreground_contents_for_tile_priority_blockhead(
        &self,
        pos: IntPair,
        for_foreground_contents: bool,
        tile: &TileRef,
        priority_blockhead: Option<&ObjRef>,
    ) {
        if let Some(d) = self.dw() {
            d.create_free_block_at_position_for_foreground_contents_for_tile_priority_blockhead(
                pos,
                for_foreground_contents,
                Some(*tile),
                priority_blockhead,
            );
        }
    }
    fn create_background_content_free_block_at_position_for_tile_remove_blockhead(
        &self,
        pos: IntPair,
        tile: &TileRef,
        remove_blockhead: Option<&ObjRef>,
    ) {
        if let Some(d) = self.dw() {
            d.create_background_content_free_block_at_position_for_tile_remove_blockhead(pos, *tile, remove_blockhead);
        }
    }
    fn interaction_object_placed_at_position_with_item_flipped_save_dict_placed_by_client_client_name(
        &self,
        pos: IntPair,
        item: &crate::items::inventory_item::InventoryItem,
        flipped: bool,
        save_dict: Option<&PlistValue>,
        client_id: Option<&str>,
        client_name: Option<&str>,
    ) -> Option<ObjRef> {
        self.dw().and_then(|d| {
            d.interaction_object_placed_at_position_with_item_flipped_save_dict_placed_by_client_client_name(
                pos,
                item,
                flipped,
                dict_of(save_dict),
                client_id,
                client_name,
            )
        })
    }
    fn sow_tree_or_plant_at_position_item_type_max_height_growth_rate(
        &self,
        pos: IntPair,
        item_type: crate::types::item_type::ItemType,
        max_height: i32,
        growth_rate: i32,
    ) {
        if let Some(d) = self.dw() {
            d.sow_tree_or_plant_at_position_item_type_max_height_growth_rate(
                pos,
                item_type.0 as i32,
                max_height as i16,
                growth_rate as i16,
            );
        }
    }
    fn add_torch_at_pos_of_type_data_a_data_b_save_dict_placed_by_client(
        &self,
        pos: IntPair,
        item_type: crate::types::item_type::ItemType,
        data_a: u16,
        data_b: u16,
        save_dict: Option<&PlistValue>,
        client_id: Option<&str>,
    ) {
        if let Some(d) = self.dw() {
            d.add_torch_at_pos_of_type_data_a_data_b_save_dict_placed_by_client(
                pos,
                item_type.0 as i32,
                data_a,
                data_b,
                dict_of(save_dict),
                client_id,
            );
        }
    }
    fn add_egg_at_pos_save_dict(&self, pos: IntPair, save_dict: Option<&PlistValue>) {
        if let Some(d) = self.dw() {
            d.add_egg_at_pos_save_dict(pos, dict_of(save_dict));
        }
    }
    fn add_painting_at_pos_of_type_save_dict_placed_by_client_client_name(
        &self,
        pos: IntPair,
        item_type: crate::types::item_type::ItemType,
        save_dict: Option<&PlistValue>,
        client_id: Option<&str>,
        client_name: Option<&str>,
    ) {
        if let Some(d) = self.dw() {
            d.add_painting_at_pos_of_type_save_dict_placed_by_client_client_name(
                pos,
                item_type.0 as i32,
                dict_of(save_dict),
                client_id,
                client_name,
            );
        }
    }
    fn add_window_at_pos_of_type_save_dict_placed_by_client(
        &self,
        pos: IntPair,
        item_type: crate::types::item_type::ItemType,
        save_dict: Option<&PlistValue>,
        client_id: Option<&str>,
    ) {
        if let Some(d) = self.dw() {
            d.add_window_at_pos_of_type_save_dict_placed_by_client(
                pos,
                item_type.0 as i32,
                dict_of(save_dict),
                client_id,
            );
        }
    }
    fn add_ladder_at_pos_of_type_save_dict_placed_by_client(
        &self,
        pos: IntPair,
        item_type: crate::types::item_type::ItemType,
        save_dict: Option<&PlistValue>,
        client_id: Option<&str>,
    ) {
        if let Some(d) = self.dw() {
            d.add_ladder_at_pos_of_type_save_dict_placed_by_client(
                pos,
                item_type.0 as i32,
                dict_of(save_dict),
                client_id,
            );
        }
    }
    fn add_wire_at_pos_of_type_save_dict_placed_by_client(
        &self,
        pos: IntPair,
        item_type: crate::types::item_type::ItemType,
        save_dict: Option<&PlistValue>,
        client_id: Option<&str>,
    ) {
        if let Some(d) = self.dw() {
            d.add_wire_at_pos_of_type_save_dict_placed_by_client(
                pos,
                item_type.0 as i32,
                dict_of(save_dict),
                client_id,
            );
        }
    }
    fn add_elevator_shaft_at_pos_of_type_save_dict_placed_by_client(
        &self,
        pos: IntPair,
        item_type: crate::types::item_type::ItemType,
        save_dict: Option<&PlistValue>,
        client_id: Option<&str>,
    ) {
        if let Some(d) = self.dw() {
            d.add_elevator_shaft_at_pos_of_type_save_dict_placed_by_client(
                pos,
                item_type.0 as i32,
                dict_of(save_dict),
                client_id,
            );
        }
    }
    fn add_elevator_motor_at_pos_of_type_save_dict_placed_by_client(
        &self,
        pos: IntPair,
        item_type: crate::types::item_type::ItemType,
        save_dict: Option<&PlistValue>,
        client_id: Option<&str>,
    ) {
        if let Some(d) = self.dw() {
            d.add_elevator_motor_at_pos_of_type_save_dict_placed_by_client(
                pos,
                item_type.0 as i32,
                dict_of(save_dict),
                client_id,
            );
        }
    }
    fn add_column_at_pos_of_type_save_dict_placed_by_client(
        &self,
        pos: IntPair,
        item_type: crate::types::item_type::ItemType,
        save_dict: Option<&PlistValue>,
        client_id: Option<&str>,
    ) {
        if let Some(d) = self.dw() {
            d.add_column_at_pos_of_type_save_dict_placed_by_client(
                pos,
                item_type.0 as i32,
                dict_of(save_dict),
                client_id,
            );
        }
    }
    fn add_stairs_at_pos_of_type_save_dict_placed_by_client(
        &self,
        pos: IntPair,
        item_type: crate::types::item_type::ItemType,
        save_dict: Option<&PlistValue>,
        client_id: Option<&str>,
    ) {
        if let Some(d) = self.dw() {
            d.add_stairs_at_pos_of_type_save_dict_placed_by_client(
                pos,
                item_type.0 as i32,
                dict_of(save_dict),
                client_id,
            );
        }
    }
    fn add_door_at_pos_of_type_save_dict_placed_by_client(
        &self,
        pos: IntPair,
        item_type: crate::types::item_type::ItemType,
        save_dict: Option<&PlistValue>,
        client_id: Option<&str>,
    ) {
        if let Some(d) = self.dw() {
            d.add_door_at_pos_of_type_save_dict_placed_by_client(
                pos,
                item_type.0 as i32,
                dict_of(save_dict),
                client_id,
            );
        }
    }
    fn add_rail_at_pos_of_type_owned_by_station(
        &self,
        pos: IntPair,
        item_type: crate::types::item_type::ItemType,
        owned_by_station: bool,
    ) {
        if let Some(d) = self.dw() {
            d.add_rail_at_pos_of_type_owned_by_station(pos, item_type.0 as i32, owned_by_station);
        }
    }
    fn place_boat_in_water_at_pos_save_dict_placed_by_client(
        &self,
        pos: IntPair,
        save_dict: Option<&PlistValue>,
        client_id: Option<&str>,
    ) {
        if let Some(d) = self.dw() {
            d.place_boat_in_water_at_pos_save_dict_placed_by_client(pos, dict_of(save_dict), client_id);
        }
    }
    fn place_train_car_at_pos_of_type_save_dict_placed_by_client(
        &self,
        pos: IntPair,
        item_type: crate::types::item_type::ItemType,
        save_dict: Option<&PlistValue>,
        client_id: Option<&str>,
    ) {
        if let Some(d) = self.dw() {
            d.place_train_car_at_pos_of_type_save_dict_placed_by_client(
                pos,
                item_type.0 as i32,
                dict_of(save_dict),
                client_id,
            );
        }
    }
    fn place_fire_at_position(&self, pos: IntPair) {
        if let Some(d) = self.dw() {
            let _ = d.place_fire_at_position(pos);
        }
    }
    fn load_npc_at_position_type_save_dict_is_adult_was_placed_placed_by_client(
        &self,
        pos: IntPair,
        npc_type: crate::types::enums::NPCType,
        save_dict: Option<&PlistValue>,
        is_adult: bool,
        was_placed: bool,
        client_id: Option<&str>,
    ) {
        if let Some(d) = self.dw() {
            d.load_npc_at_position_type_save_dict_is_adult_was_placed_placed_by_client(
                pos,
                npc_type.0 as i32,
                dict_of(save_dict),
                is_adult,
                was_placed,
                client_id,
            );
        }
    }

    // ---- Workbench / Blockhead accessors
    fn workbench_pos(&self, wb: &ObjRef) -> IntPair {
        wb.base().borrow().pos
    }
    fn workbench_type(&self, wb: &ObjRef) -> crate::types::enums::WorkbenchType {
        crate::objects::handle::downcast::<crate::crafting::workbench::Workbench>(wb)
            .map_or(crate::types::enums::WorkbenchType(0), |w| w.workbench_type())
    }
    fn workbench_level(&self, wb: &ObjRef) -> i32 {
        crate::objects::handle::downcast::<crate::crafting::workbench::Workbench>(wb).map_or(0, |w| w.level())
    }
    fn workbench_upgrade_to_next_level(&self, wb: &ObjRef) {
        if let Some(w) = crate::objects::handle::downcast::<crate::crafting::workbench::Workbench>(wb) {
            w.upgrade_to_next_level(wb);
        }
    }
    fn bh_asleep(&self, bh: &ObjRef) -> bool {
        bh_of(bh).map_or(false, |b| b.asleep())
    }
    fn bh_meditating(&self, bh: &ObjRef) -> bool {
        bh_of(bh).map_or(false, |b| b.meditating())
    }
    fn bh_regenerating(&self, bh: &ObjRef) -> bool {
        bh_of(bh).map_or(false, |b| b.regenerating())
    }
    fn bh_on_trade_mission(&self, bh: &ObjRef) -> bool {
        bh_of(bh).map_or(false, |b| b.on_trade_mission())
    }
    fn bh_is_net(&self, bh: &ObjRef) -> bool {
        bh_of(bh).map_or(false, |b| b.is_net())
    }
    fn bh_is_client_blockhead_being_controlled_by_server(&self, bh: &ObjRef) -> bool {
        bh_of(bh).map_or(false, |b| b.is_client_blockhead_being_controlled_by_server())
    }
    fn bh_add_to_crystal_discrepency(&self, bh: &ObjRef, n: i32) {
        if let Some(b) = bh_of(bh) {
            b.add_to_crystal_discrepency(n);
        }
    }
    fn bh_add_expected_craft_item(&self, bh: &ObjRef, item: crate::types::item_type::ItemType) {
        if let Some(b) = bh_of(bh) {
            b.add_expected_craft_item(item);
        }
    }
    fn bh_local_net_id(&self, bh: &ObjRef) -> Option<String> {
        bh_of(bh).and_then(|b| b.local_net_id())
    }
    fn bh_stop_interacting(&self, bh: &ObjRef) {
        if let Some(b) = bh_of(bh) {
            b.stop_interacting();
        }
    }
    fn bh_can_use_dynamic_object(&self, bh: &ObjRef, o: &ObjRef) -> bool {
        bh_of(bh).map_or(false, |b| b.can_use_dynamic_object(o))
    }

    fn load_new_blockhead_at_pos_craftable_item_object_unique_id(
        &self,
        pos: crate::types::structs::IntPair,
        craftable_item_object: &crate::objects::handle::ObjRef,
        unique_id: u64,
    ) {
        if let Some(d) = self.dw() {
            d.load_new_blockhead_at_pos_craftable_item_object_unique_id(pos, craftable_item_object, unique_id);
        }
    }
    fn continue_simulate(&self) {
        if let Some(w) = self.world() {
            w.continue_simulate();
        }
    }
    fn delegate_set_paused(&self, paused: bool) {
        if let Some(d) = self.world().and_then(|w| w.core_delegate()) {
            d.set_paused(paused);
        }
    }
    fn crystal_manager_set_count_watcher_none(&self) {
        crate::env::crystal_manager::CrystalManager::instance().set_count_watcher(None)
    }
    fn crystal_manager_amount(&self) -> i32 {
        crate::env::crystal_manager::CrystalManager::instance().amount()
    }
    fn crystal_manager_modify_modify_string(&self, amount: i32, modify_string: &str) {
        crate::env::crystal_manager::CrystalManager::instance().modify_modify_string(amount, modify_string)
    }
    fn particle_emitter_add_particle_at_pos_velocity_color_gravity_type_life_scale(
        &self,
        pos: [f32; 3],
        velocity: [f32; 3],
        color: [f32; 4],
        gravity_type: i32,
        life: f32,
        scale: f32,
    ) {
        let v3 = |a: [f32; 3]| crate::fmath::vector::Vector::new(a[0], a[1], a[2]);
        crate::env::particle_emitter::ParticleEmitter::instance()
            .add_particle_at_pos_velocity_color_gravity_type_life_scale(
                v3(pos),
                v3(velocity),
                crate::fmath::vector::Vector::new4(color[0], color[1], color[2], color[3]),
                gravity_type,
                life,
                scale,
            )
    }
    fn bh_is_idle(&self, bh: &crate::objects::handle::ObjRef) -> bool {
        bh_of(bh).is_none_or(|b| b.idle())
    }
    fn bh_check_if_can_warp_in_second_blockhead_after_item_added_data_b(
        &self,
        bh: &crate::objects::handle::ObjRef,
        item: crate::types::item_type::ItemType,
        data_b: u16,
    ) {
        if let Some(b) = bh_of(bh) {
            b.check_if_can_warp_in_second_blockhead_after_item_added_data_b(item, data_b as i32);
        }
    }
    fn bh_current_interaction_type_for_tile_at_pos_pickup_rejected_due_to_inventory_full_include_actions_face_index_allow_protected_actions(
        &self,
        bh: &crate::objects::handle::ObjRef,
        tile: &crate::world::state::TileRef,
        pos: crate::types::structs::IntPair,
        pickup_rejected: &mut bool,
        include_actions: bool,
        face_index: u8,
        allow_protected_actions: bool,
    ) -> crate::types::enums::InteractionType {
        match bh_of(bh) {
            Some(b) => {
                let item = b.current_item();
                b.interaction_type_for_tile_at_pos_item_pickup_rejected_due_to_inventory_full_include_actions_face_index_allow_protected_actions(
                    Some(tile),
                    pos,
                    item.as_ref(),
                    pickup_rejected,
                    include_actions,
                    face_index as i32,
                    allow_protected_actions,
                )
            }
            None => crate::types::enums::InteractionType::INTERACTION_NONE,
        }
    }
    fn bh_queue_action_with_goal_pos_goal_interaction_path_type_interaction_object_id_craftable_item_object_craft_count_or_extra_data_is_ai(
        &self,
        bh: &crate::objects::handle::ObjRef,
        goal_pos: crate::types::structs::IntPair,
        goal_interaction: crate::types::enums::InteractionType,
        path_type: i32,
        interaction_object_id: u64,
        craftable_item_object: Option<&crate::objects::handle::ObjRef>,
        craft_count_or_extra_data: i32,
        is_ai: bool,
    ) {
        if let Some(b) = bh_of(bh) {
            let cio = craftable_item_object.and_then(crate::crafting::workbench_craft::craftable_item_object_of_shim);
            b.queue_action_with_goal_pos_goal_interaction_path_type_interaction_object_id_craftable_item_object_craft_count_or_extra_data_disable_cancel_check_is_ai(
                goal_pos,
                goal_interaction,
                crate::types::enums::PathType(path_type as u32),
                interaction_object_id,
                cio,
                craft_count_or_extra_data as i16,
                false,
                is_ai,
            );
        }
    }
    fn bh_cancel_any_action_at_goal_pos_or_with_interaction_object_id_goal_interaction_craft_count_or_extra_data(
        &self,
        bh: &crate::objects::handle::ObjRef,
        goal_pos: crate::types::structs::IntPair,
        interaction_object_id: u64,
        goal_interaction: crate::types::enums::InteractionType,
        craft_count_or_extra_data: i32,
    ) {
        if let Some(b) = bh_of(bh) {
            b.cancel_any_action_at_goal_pos_or_with_interaction_object_id_goal_interaction_craft_count_or_extra_data(
                goal_pos,
                interaction_object_id,
                goal_interaction.0 as i32,
                craft_count_or_extra_data as i16,
            );
        }
    }
    fn io_can_be_used_by_blockhead(
        &self,
        io: &crate::objects::handle::ObjRef,
        bh: &crate::objects::handle::ObjRef,
    ) -> bool {
        io.as_interaction_like().is_some_and(|i| i.can_be_used_by_blockhead(io, bh))
    }
    fn io_is_in_use(&self, io: &crate::objects::handle::ObjRef) -> bool {
        io.is_in_use()
    }
    fn io_start_interaction_with_blockhead(
        &self,
        io: &crate::objects::handle::ObjRef,
        bh: &crate::objects::handle::ObjRef,
    ) {
        if let Some(i) = io.as_interaction_like() {
            i.start_interaction_with_blockhead(io, bh);
        }
    }
    fn io_is_paintable(&self, io: &crate::objects::handle::ObjRef) -> bool {
        io.as_interaction_like().is_some_and(|i| i.is_paintable())
    }
    fn obj_needs_removed(&self, o: &crate::objects::handle::ObjRef) -> bool {
        o.needs_removed()
    }
    fn obj_can_be_removed_by_blockhead(
        &self,
        o: &crate::objects::handle::ObjRef,
        bh: &crate::objects::handle::ObjRef,
    ) -> bool {
        o.can_be_removed_by_blockhead(o, bh)
    }
    fn obj_paint(&self, o: &crate::objects::handle::ObjRef, color_index: u16) {
        use crate::building::{column::Column, elevator::ElevatorShaft, ladder::Ladder, stairs::Stairs};
        use crate::objects::handle::downcast;
        if let Some(x) = downcast::<Column>(o) {
            x.paint(o, color_index);
        } else if let Some(x) = downcast::<Ladder>(o) {
            x.paint(o, color_index);
        } else if let Some(x) = downcast::<Stairs>(o) {
            x.paint(o, color_index);
        } else if let Some(x) = downcast::<ElevatorShaft>(o) {
            x.paint(o, color_index);
        } else if let Some(i) = o.as_interaction_like() {
            i.paint(o, color_index);
        }
    }
    fn craftable_item(&self, obj: &crate::objects::handle::ObjRef) -> crate::types::structs::CraftableItem {
        match crate::crafting::workbench_craft::craftable_item_object_of_shim(obj) {
            Some(c) => c.craftable_item(),
            None => crate::world::misc::zero_craftable_item(),
        }
    }
    fn free_block_creation_item_save_dict(
        &self,
        obj: &crate::objects::handle::ObjRef,
    ) -> Option<crate::plist::value::PlistValue> {
        crate::crafting::workbench_craft::craftable_item_object_of_shim(obj)?
            .free_block_creation_item_save_dict()
            .map(PlistValue::Dict)
    }
    fn bh_subtract_items_from_inventory_of_type_count_data_b(
        &self,
        bh: &crate::objects::handle::ObjRef,
        item: crate::types::item_type::ItemType,
        count: i32,
        data_b: i32,
    ) -> i32 {
        bh_of(bh).map_or(0, |b| b.subtract_items_from_inventory_of_type_count_data_b(item, count, data_b).len() as i32)
    }
}

// =====================================================================================================
// DwHooks (World-side calls and registry construction)
// =====================================================================================================

impl DwHooks for Glue {
    fn save_physical_block_for_macro_tile(
        &self,
        mt: &MacroTileRef,
        send_reliably: bool,
        dont_send: bool,
        only_save_if_clients_need_it: bool,
    ) -> bool {
        self.world().map_or(false, |w| {
            w.save_physical_block_for_macro_tile_send_reliably_dont_send_only_save_if_clients_need_it(
                mt,
                send_reliably,
                dont_send,
                only_save_if_clients_need_it,
            )
        })
    }
    fn save_light_block_for_client_light_block_index(&self, index: i32, pb: Option<PhysicalBlockRef>, send_now: bool) {
        if let (Some(w), Some(pb)) = (self.world(), pb) {
            w.save_light_block_for_client_light_block_index_physical_block_send_now(index, &pb, send_now);
        }
    }
    fn fully_load_if_needed_around_pos(&self, pos: IntPair, client_light_block_index: u32, blockhead: &ObjRef) {
        if let Some(w) = self.world() {
            let idx = if client_light_block_index == 0xFFFF_FFFF { -1 } else { client_light_block_index as i32 };
            w.fully_load_if_needed_around_pos_client_light_block_index_for_blockhead(pos, idx, Some(blockhead));
        }
    }
    fn update_physical_block_to_latest_version(&self, pb: PhysicalBlockRef) {
        if let Some(l) = self.loader() {
            l.update_physical_block_to_latest_version(pb);
        }
    }
    fn place_basic_portal_workbench(&self, pos: IntPair) {
        if let Some(w) = self.world() {
            w.place_workbench_of_type_at_pos_save_dict_placed_by_client_placed_by_blockhead_placed_by_client_name(
                crate::types::enums::WorkbenchType::WORKBENCH_BASIC_PORTAL,
                pos,
                None,
                None,
                None,
                None,
            );
        }
    }
    fn tile_is_lit_for_client(&self, client_id: Option<&str>, pos: IntPair, tile: TileRef) -> bool {
        self.world().map_or(false, |w| w.tile_is_lit_for_client_at_pos_tile(client_id, pos, Some(tile)))
    }
    fn construct_from_save_dict(
        &self,
        kind: SaveLoadKind,
        object_type: i32,
        save_dict: &PlistDict,
        chest_save_dict: Option<&PlistDict>,
    ) -> Option<ObjRef> {
        let w = self.world()?;
        let dw = self.dw()?;
        use crate::objects::registry as r;
        match kind {
            SaveLoadKind::TreeOrPlant => {
                let loader = self.loader();
                if crate::objects::type_tables::object_type_is_plant(object_type) {
                    let t = loader.as_ref().and_then(|l| l.tree_density_noise_function());
                    let s = loader.as_ref().and_then(|l| l.season_offset_noise_function());
                    r::init_plant_from_save_dict_with_noise(object_type, &w, &dw, save_dict, None, t, s)
                } else {
                    let tree: crate::trees::tree::NoiseRef = loader
                        .as_ref()
                        .and_then(|l| l.tree_density_noise_function())
                        .map(|n| Rc::new(NoiseAdapter(n)) as Rc<dyn DwNoise>);
                    let season: crate::trees::tree::NoiseRef = loader
                        .as_ref()
                        .and_then(|l| l.season_offset_noise_function())
                        .map(|n| Rc::new(NoiseAdapter(n)) as Rc<dyn DwNoise>);
                    r::init_tree_or_plant_from_save_dict(object_type, &w, &dw, save_dict, None, tree, season)
                }
            }
            SaveLoadKind::Shark => r::init_shark_from_save_dict(&w, &dw, save_dict, None),
            SaveLoadKind::FreightCar => r::init_freight_car_from_save_dict(&w, &dw, save_dict, chest_save_dict, None),
            SaveLoadKind::Workbench => r::init_workbench_from_save_dict(&w, &dw, save_dict, None),
            SaveLoadKind::InteractionObject(ito) => {
                let f = crate::objects::type_tables::class_for_interaction_object_type(ito)?;
                let wr: crate::objects::base::WorldRef = w;
                let dr: crate::objects::base::DynamicWorldRef = dw;
                (f.from_save_dict?)(&wr, &dr, save_dict, None)
            }
            SaveLoadKind::Generic => {
                let f = crate::objects::type_tables::class_for_dynamic_object_type(object_type)?;
                let wr: crate::objects::base::WorldRef = w;
                let dr: crate::objects::base::DynamicWorldRef = dw;
                (f.from_save_dict?)(&wr, &dr, save_dict, None)
            }
        }
    }
    fn construct_from_net_data(
        &self,
        object_type: i32,
        interaction_type: Option<i32>,
        net_data: &[u8],
    ) -> Option<ObjRef> {
        let w = self.world()?;
        let dw = self.dw()?;
        let f = match interaction_type {
            Some(i) => crate::objects::type_tables::class_for_interaction_object_type(i)?,
            None => crate::objects::type_tables::class_for_dynamic_object_type(object_type)?,
        };
        let wr: crate::objects::base::WorldRef = w;
        let dr: crate::objects::base::DynamicWorldRef = dw;
        (f.from_net_data?)(&wr, &dr, None, net_data)
    }
    fn workbench_type(&self, wb: &ObjRef) -> i32 {
        crate::objects::handle::downcast::<crate::crafting::workbench::Workbench>(wb)
            .map_or(0, |w| w.workbench_type().0 as i32)
    }
    fn workbench_level(&self, wb: &ObjRef) -> i32 {
        crate::objects::handle::downcast::<crate::crafting::workbench::Workbench>(wb).map_or(0, |w| w.level())
    }
    fn workbench_set_level_silently(&self, wb: &ObjRef, level: i32) {
        if let Some(w) = crate::objects::handle::downcast::<crate::crafting::workbench::Workbench>(wb) {
            w.set_level_silently(wb, level);
        }
    }
    fn chest_type(&self, chest: &ObjRef) -> i32 {
        crate::objects::handle::downcast::<crate::interaction::chest::Chest>(chest).map_or(0, |c| c.chest_type())
    }
    fn destroy_item_type(&self, obj: &ObjRef) -> i32 {
        crate::objects::registry::as_interaction_like(obj).map_or(0, |i| i.destroy_item_type())
    }
    fn dynamic_world_changed_at_pos_object_type(&self, pos: IntPair, object_type: i32) {
        if let Some(d) = self.dw() {
            d.dynamic_world_changed_at_pos_object_type(pos, object_type);
        }
    }
    fn light_changed_at_macro_pos_send_reliably(&self, macro_pos: IntPair, send_reliably: bool) {
        if let Some(d) = self.dw() {
            d.light_changed_at_macro_pos_send_reliably(macro_pos, send_reliably);
        }
    }
    fn blockhead_with_id_including_net(&self, unique_id: u64) -> Option<ObjRef> {
        self.dw().and_then(|d| d.blockhead_with_id_including_net(unique_id))
    }
    fn blockhead_will_be_unloaded(&self, blockhead: &ObjRef) {
        if let Some(d) = self.dw() {
            d.blockhead_will_be_unloaded(blockhead);
        }
    }
    fn check_and_restore_pole_items(&self, dt: f32) {
        if let Some(d) = self.dw() {
            d.check_and_restore_pole_items(dt);
        }
    }
    fn load_tree_at_position(
        &self,
        pos: IntPair,
        tree_type: i32,
        max_height: i32,
        growth_rate: i32,
        adult_tree: bool,
        adult_max_age: f32,
    ) {
        if let Some(d) = self.dw() {
            d.load_tree_at_position_type_max_height_growth_rate_adult_tree_adult_max_age(
                pos,
                tree_type,
                max_height as i16,
                growth_rate as i16,
                adult_tree,
                adult_max_age,
            );
        }
    }
    fn load_plant_at_position(
        &self,
        pos: IntPair,
        plant_type: i32,
        max_age_gene: i32,
        growth_rate_gene: i32,
        adult: bool,
    ) {
        if let Some(d) = self.dw() {
            d.load_plant_at_position_type_max_age_gene_growth_rate_gene_adult(
                pos,
                plant_type,
                max_age_gene as i16,
                growth_rate_gene as i16,
                adult,
            );
        }
    }
    fn load_npc_at_position(&self, pos: IntPair, npc_type: i32, is_adult: bool, was_placed: bool) {
        if let Some(d) = self.dw() {
            d.load_npc_at_position_type_save_dict_is_adult_was_placed_placed_by_client(
                pos, npc_type, None, is_adult, was_placed, None,
            );
        }
    }
    fn add_torch_at_pos_of_type(&self, pos: IntPair, torch_type: i32) {
        if let Some(d) = self.dw() {
            d.add_torch_at_pos_of_type_data_a_data_b_save_dict_placed_by_client(pos, torch_type, 0, 0, None, None);
        }
    }
    fn create_treasure_chest_or_troll_at_tile(
        &self,
        tile: TileRef,
        pos: IntPair,
        load_troll: bool,
        load_treasure: bool,
    ) {
        if let Some(d) = self.dw() {
            d.create_treasure_chest_or_troll_at_tile_at_pos_load_troll_load_treasure(
                &tile,
                pos,
                load_troll,
                load_treasure,
            );
        }
    }
    fn load_glow_block_if_needed_at_pos_tile(&self, pos: IntPair, tile: TileRef) {
        if let Some(d) = self.dw() {
            d.load_glow_block_if_needed_at_pos_tile(pos, &tile);
        }
    }
    fn load_surface_block_at_pos(&self, pos: IntPair) {
        if let Some(d) = self.dw() {
            d.load_surface_block_at_pos(pos);
        }
    }
    fn load_snow_surface_block_at_pos_load_snow(&self, pos: IntPair, load_snow: bool) {
        if let Some(d) = self.dw() {
            d.load_snow_surface_block_at_pos_load_snow(pos, load_snow);
        }
    }
    fn workbench_at_pos(&self, pos: IntPair) -> Option<ObjRef> {
        self.dw().and_then(|d| d.workbench_at_pos(pos))
    }
    fn interaction_object_at_pos(&self, pos: IntPair) -> Option<ObjRef> {
        self.dw().and_then(|d| d.interaction_object_at_pos(pos))
    }
    fn object_at_pos_for_item(&self, which: char, pos: IntPair) -> Option<ObjRef> {
        let name = match which {
            'p' => "Painting",
            'c' => "Column",
            's' => "Stairs",
            't' => "Torch",
            'w' => "Wire",
            _ => return None,
        };
        // A class that the type table does not know has type 0 and no object.
        Some(dyn_type_named(name)).filter(|t| *t != 0).and_then(|t| self.dw()?.object_of_type_at_pos(t as i32, pos))
    }
    fn remove_door_at_pos(&self, pos: IntPair) -> Option<ObjRef> {
        self.dw().and_then(|d| d.remove_door_at_pos(pos))
    }
    fn blockhead_from_save_dict(
        &self,
        save_dict: &PlistDict,
        saved_inventory_slots: Option<PlistValue>,
        reposition: bool,
        client_save_dir: Option<&str>,
        client_locally_saved_dict: Option<PlistValue>,
    ) -> Option<ObjRef> {
        let w = self.world()?;
        let d: crate::objects::base::DynamicWorldRef = self.dw()?;
        crate::blockhead::blockhead::Blockhead::init_with_world_dynamic_world_save_dict_saved_inventory_slots_cache_reposition_on_load_failure_client_save_dir_client_locally_saved_dict(
            &w,
            &d,
            save_dict,
            saved_inventory_slots.as_ref(),
            None,
            reposition,
            client_save_dir,
            client_locally_saved_dict.as_ref(),
        )
    }
    fn blockhead_from_net_data(&self, net_data: &[u8]) -> Option<ObjRef> {
        let w = self.world()?;
        let d: crate::objects::base::DynamicWorldRef = self.dw()?;
        crate::blockhead::blockhead::Blockhead::init_with_world_dynamic_world_cache_net_data(&w, &d, None, net_data)
    }

    // ---- FreeBlock (r2_session_0 (a), r2_longrun_1 Defect A)
    fn freeblock_from_position(&self, init: crate::dynworld::state::FreeBlockInit<'_>) -> Option<ObjRef> {
        let w = self.world()?;
        let d = self.dw()?;
        let subs = init.sub_items.map(sub_items_to_slots);
        let sd = match init.dynamic_object_save_dict {
            Some(PlistValue::Dict(d)) => Some(d),
            _ => None,
        };
        crate::physics_objects::free_block::FreeBlock::new(
            &w,
            &d,
            init.position,
            init.item_type as i32,
            init.data_a,
            init.data_b,
            subs.as_deref(),
            sd,
            None,
            init.hovers,
            init.priority_blockhead,
        )
    }
    fn freeblock_from_net_data(
        &self,
        net_data: &[u8],
        avoid: Option<&mut std::collections::VecDeque<u64>>,
    ) -> Option<ObjRef> {
        let w = self.world()?;
        let d = self.dw()?;
        match avoid {
            Some(a) => crate::physics_objects::free_block::FreeBlock::from_net_data_avoid_freeblock_dupe_object_ids(
                &w, &d, None, net_data, a,
            ),
            None => crate::physics_objects::free_block::FreeBlock::from_net_data(&w, &d, None, net_data),
        }
    }
    fn freeblock_set_creation_sound(&self, fb: &ObjRef, creation_sound_play_time: f32, sound_type: i32) {
        if let Some(f) = crate::objects::handle::downcast::<crate::physics_objects::free_block::FreeBlock>(fb) {
            f.set_creation_sound_play_time(creation_sound_play_time);
            f.set_sound_type(sound_type);
        }
    }

    // ---- Blockhead accessors (r2_session_0 (b), r2_longrun_0 section 3)
    fn blockhead_client_id(&self, b: &ObjRef) -> Option<String> {
        bh_of(b).and_then(|x| x.client_id())
    }
    fn blockhead_set_client_id(&self, b: &ObjRef, id: &str) {
        if let Some(x) = bh_of(b) {
            x.set_client_id(id);
        }
    }
    fn blockhead_set_client_name(&self, b: &ObjRef, name: Option<&str>) {
        if let Some(x) = bh_of(b) {
            x.set_client_name(name);
        }
    }
    fn blockhead_client_save_dir(&self, b: &ObjRef) -> Option<String> {
        bh_of(b).and_then(|x| x.client_save_dir())
    }
    fn blockhead_save_dict_including_workbench_or_interaction_object(&self, b: &ObjRef, including: bool) -> PlistValue {
        match bh_of(b) {
            Some(x) => PlistValue::Dict(x.get_save_dict_including_workbench_or_interation_object(including)),
            None => PlistValue::mutable_dict(),
        }
    }
    fn blockhead_save_item_slots_array(&self, b: &ObjRef) -> PlistValue {
        match bh_of(b) {
            Some(x) => PlistValue::array(
                x.save_item_slots_array()
                    .into_iter()
                    .map(|slot| PlistValue::array(slot.into_iter().map(PlistValue::data).collect()))
                    .collect(),
            ),
            None => PlistValue::mutable_array(),
        }
    }
    fn blockhead_prepare_inventory_for_saving(&self, b: &ObjRef) {
        if let Some(x) = bh_of(b) {
            x.prepare_inventory_for_saving();
        }
    }
    fn blockhead_inventory_needs_saving(&self, b: &ObjRef) -> bool {
        bh_of(b).map_or(false, |x| x.inventory_needs_saving())
    }
    fn blockhead_set_inventory_needs_saving(&self, b: &ObjRef, v: bool) {
        if let Some(x) = bh_of(b) {
            x.set_inventory_needs_saving(v);
        }
    }
    fn blockhead_inventory_was_changed(&self, b: &ObjRef, index: i32, sub_index: i32, was_usage: bool) {
        if let Some(x) = bh_of(b) {
            x.inventory_was_changed_sub_index_was_usage(index, sub_index, was_usage);
        }
    }
    fn blockhead_stop_interacting_if_no_interaction_object(&self, b: &ObjRef) {
        if let Some(x) = bh_of(b) {
            x.stop_interacting_with_interaction_objects_if_no_interaction_object();
        }
    }
    fn blockhead_stop_riding(&self, b: &ObjRef) {
        if let Some(x) = bh_of(b) {
            x.stop_riding();
        }
    }
    fn blockhead_die(&self, b: &ObjRef) {
        if let Some(x) = bh_of(b) {
            x.die();
        }
    }
    // ---- chests and freight-car chests (`saveDynamicObjectsForMacroTile:`): without these the chest inventory
    // was never written to `<x>_<y>/chest_<id>` and freight-car chests never to `trainchest_<id>`.
    fn chest_inventory_data_allowing_empty(&self, chest: &ObjRef) -> Option<Vec<u8>> {
        chest_of(chest)?.inventory_data_allowing_empty(true)
    }
    fn chest_save_dict_excluding_inventory(&self, chest: &ObjRef) -> Option<PlistValue> {
        Some(PlistValue::Dict(chest_of(chest)?.get_save_dict_including_inventory(chest, false)))
    }
    fn freight_needs_chest_save(&self, car: &ObjRef) -> bool {
        freight_of(car).is_some_and(|f| f.needs_chest_save())
    }
    fn freight_set_needs_chest_save(&self, car: &ObjRef, v: bool) {
        if let Some(f) = freight_of(car) {
            f.set_needs_chest_save(v);
        }
    }
    fn freight_chest_save_dict(&self, car: &ObjRef) -> Option<PlistValue> {
        freight_of(car).map(|f| PlistValue::Dict(f.get_chest_save_dict()))
    }
    // item_type/remove_window_at_pos/tree_update_*/update_rain_dt/cleanup keep the inert defaults
    // (owning modules do not yet publish adapters; see cross_module_requests).

    fn remove_window_at_pos(&self, pos: crate::types::structs::IntPair) -> Option<crate::objects::handle::ObjRef> {
        self.dw()?.remove_window_at_pos(pos)
    }
    fn tree_update_all_owned_tiles_to_new_id_size(&self, tree: &crate::objects::handle::ObjRef) {
        if crate::trees::tree::as_tree(tree).is_some() {
            crate::trees::tree::tree_update_all_owned_tiles_to_new_id_size(tree);
        }
    }
    fn update_rain_dt(&self, obj: &crate::objects::handle::ObjRef, rain: f32, dt: f32) {
        if let Some(b) = crate::objects::handle::downcast::<crate::env::snow_surface_block::SnowSurfaceBlock>(obj) {
            b.update_rain_dt(rain, dt);
        }
    }
    fn item_type(&self, obj: &crate::objects::handle::ObjRef) -> i32 {
        item_type_of(obj)
    }
    fn cleanup(&self, obj: &crate::objects::handle::ObjRef) {
        if crate::trains::train_car::train_car_of(obj).is_some() {
            crate::trains::train_car::train_car_cleanup(obj);
        }
    }
}

// =====================================================================================================
// DwOpsHooks: constructors / accessors owned by the object modules keep the inert defaults.
// =====================================================================================================

impl crate::npc::npc::NpcEnv for Glue {
    fn world(&self) -> Option<Rc<World>> {
        Glue::world(self)
    }
    fn day_night_fraction(&self, x: f32, t: f64) -> f32 {
        Glue::world(self).map_or(0.0, |w| w.get_day_night_fraction_for_x_at_world_time(x, t))
    }
    fn weather_fraction(&self, pos: IntPair, t: f64) -> f32 {
        Glue::world(self).map_or(0.0, |w| w.get_weather_fraction_for_pos_at_world_time(pos, t))
    }
    fn server_clients_count(&self) -> usize {
        Glue::world(self).map_or(0, |w| w.server_clients_count())
    }
    fn blockhead_with_id_including_net(&self, id: u64) -> Option<ObjRef> {
        self.dw().and_then(|d| d.blockhead_with_id_including_net(id))
    }
    fn all_blockheads_including_net(&self) -> Vec<ObjRef> {
        self.dw().map_or_else(Vec::new, |d| d.all_blockheads_including_net())
    }
    fn free_blocks_at_pos(&self, pos: IntPair) -> Option<Vec<ObjRef>> {
        self.dw().and_then(|d| d.free_blocks_at_pos(pos))
    }
    fn interaction_object_at_pos(&self, pos: IntPair) -> Option<ObjRef> {
        self.dw().and_then(|d| d.interaction_object_at_pos(pos))
    }
    fn set_door_at_pos_to_open_direction(&self, pos: IntPair, open: bool, direction: i32) {
        if let Some(d) = self.dw() {
            d.set_door_at_pos_to_open_direction(pos, open, direction);
        }
    }

    fn mark_circum_navigate_x(&self, _x: i32) {
        if let Some(w) = self.world() {
            w.mark_circum_navigate_x(_x);
        }
    }
    fn is_client(&self) -> bool {
        self.dw().is_some_and(|d| d.is_client())
    }
    fn free_block_item_type(&self, _fb: &crate::objects::handle::ObjRef) -> i32 {
        crate::objects::handle::downcast::<crate::physics_objects::free_block::FreeBlock>(_fb)
            .map_or(0, |f| f.item_type())
    }
    fn feeder_chest_remove_item_if_available(&self, _io: &crate::objects::handle::ObjRef, _item: i32) -> Option<bool> {
        let c = chest_of(_io)?;
        let io = _io.as_interaction_like()?;
        if io.interaction_object_type() == 2 && c.chest_type() == 6 && !_io.is_in_use() {
            Some(c.remove_item_if_available(_io, _item))
        } else {
            None
        }
    }
    fn create_free_block_for_captured_npc(
        &self,
        _pos: crate::types::structs::IntPair,
        _item_type: i32,
        _save_dict: &crate::plist::value::PlistDict,
        _blockhead: &crate::objects::handle::ObjRef,
    ) {
        if let Some(d) = self.dw() {
            d.create_free_block_at_position_of_type_data_a_data_b_sub_items_dynamic_object_save_dict_hovers_play_sound_priority_blockhead(
                _pos,
                crate::types::item_type::ItemType(_item_type as u32),
                0,
                0,
                None,
                Some(PlistValue::Dict(_save_dict.clone())),
                false,
                0,
                Some(_blockhead),
            );
        }
    }
    fn remove_curse_words_from_blockhead_name(&self, name: &str) -> String {
        match self.srv() {
            Some(s) => s.remove_curse_words_from_blockhead_name(Some(name)).unwrap_or_default(),
            None => name.to_string(),
        }
    }
    fn blockhead_paused(&self, _b: &crate::objects::handle::ObjRef) -> bool {
        bh_of(_b).is_some_and(|b| b.paused())
    }
    fn blockhead_stop_riding(&self, _b: &crate::objects::handle::ObjRef) {
        if let Some(b) = bh_of(_b) {
            b.stop_riding();
        }
    }
    fn blockhead_set_riding_object(
        &self,
        _b: &crate::objects::handle::ObjRef,
        _o: Option<&crate::objects::handle::ObjRef>,
    ) {
        if let Some(b) = bh_of(_b) {
            b.set_riding_object(_o);
        }
    }
}

impl crate::crafting::workbench::WorkbenchHost for Glue {
    fn portal_is_being_removed_at_pos(&self, pos: IntPair) {
        if let Some(d) = self.dw() {
            d.portal_is_being_removed_at_pos(pos);
        }
    }
    fn find_and_subtract_all_power_up_to_for_user(&self, required: u16, user: &ObjRef) -> i32 {
        self.dw().map_or(0, |d| d.find_and_subtract_all_power_up_to_for_user(required, user))
    }

    fn bh_paused(&self, bh: &crate::objects::handle::ObjRef) -> bool {
        bh_of(bh).is_some_and(|b| b.paused())
    }
    fn bh_has_coffee_energy(&self, bh: &crate::objects::handle::ObjRef) -> bool {
        bh_of(bh).is_some_and(|b| b.has_coffee_energy())
    }
    fn bh_unable_to_work_reason(&self, bh: &crate::objects::handle::ObjRef) -> i32 {
        bh_of(bh).map_or(0, |b| b.unable_to_work_reason().0 as i32)
    }
    fn bh_should_continue_simulating(&self, bh: &crate::objects::handle::ObjRef) -> bool {
        bh_of(bh).is_some_and(|b| b.should_continue_simulating())
    }
    fn bh_is_crafting_at_any_workbench(&self, bh: &crate::objects::handle::ObjRef) -> bool {
        bh_of(bh).is_some_and(|b| b.is_crafting_at_any_workbench())
    }
    fn bh_is_adding_fuel_to_any_workbench(&self, bh: &crate::objects::handle::ObjRef) -> bool {
        bh_of(bh).is_some_and(|b| b.is_adding_fuel_to_any_workbench())
    }
    fn bh_interaction_workbench(&self, bh: &crate::objects::handle::ObjRef) -> Option<crate::objects::handle::ObjRef> {
        bh_of(bh).and_then(|b| b.interaction_workbench())
    }
    fn bh_set_interaction_workbench(
        &self,
        bh: &crate::objects::handle::ObjRef,
        wb: Option<&crate::objects::handle::ObjRef>,
    ) {
        if let Some(b) = bh_of(bh) {
            b.set_interaction_workbench(wb.cloned());
        }
    }
    fn bh_double_time_unlocked(&self, bh: &crate::objects::handle::ObjRef) -> bool {
        bh_of(bh).is_some_and(|b| b.double_time_unlocked())
    }
    fn bh_subtract_items_from_inventory_of_type_count_data_b(
        &self,
        bh: &crate::objects::handle::ObjRef,
        item_type: crate::types::item_type::ItemType,
        count: i32,
        data_b: i32,
    ) -> Vec<crate::items::inventory_item::ItemRef> {
        bh_of(bh)
            .map_or_else(Vec::new, |b| b.subtract_items_from_inventory_of_type_count_data_b(item_type, count, data_b))
    }
    fn bh_add_item_to_inventory(
        &self,
        bh: &crate::objects::handle::ObjRef,
        item: &crate::items::inventory_item::ItemRef,
    ) {
        if let Some(b) = bh_of(bh) {
            b.add_item_to_inventory(item);
        }
    }
    fn bh_craft_item_finished_at_workbench(
        &self,
        bh: &crate::objects::handle::ObjRef,
        completed: bool,
        wb: &crate::objects::handle::ObjRef,
    ) {
        if let Some(b) = bh_of(bh) {
            b.craft_item_finished_at_workbench(completed, Some(wb));
        }
    }
    fn bh_net_interaction_object_was_loaded(
        &self,
        bh: &crate::objects::handle::ObjRef,
        wb: &crate::objects::handle::ObjRef,
    ) {
        if let Some(b) = bh_of(bh) {
            b.net_interaction_object_was_loaded(wb);
        }
    }
}

impl crate::path::path_creator::PathDynamicWorldHooks for Glue {
    fn has_lights_to_add(&self) -> bool {
        self.dw().map_or(false, |d| d.has_lights_to_add())
    }
    fn blockhead_occupies_tile_at_pos_ignore_blockhead(&self, pos: IntPair, ignore: Option<&ObjRef>) -> bool {
        self.dw().map_or(false, |d| d.blockhead_occupies_tile_at_pos_ignore_blockhead(pos, ignore))
    }
    fn workbench_at_pos(&self, pos: IntPair) -> Option<Rc<dyn crate::path::wire_path::WirePowerSource>> {
        let _ = pos;
        None
    }
}

impl crate::lighting::artificial_light::LightingServices for Glue {
    fn save_sunlight_changed_at_pos(&self, world: &World, pos: IntPair) {
        world.save_sunlight_changed_at_pos(pos);
    }
    fn fill_tile_at_pos_with_type(&self, world: &World, tile: TileRef, pos: IntPair, item_type: i32) {
        world.fill_tile_at_pos_with_type(Some(&tile), pos, crate::types::item_type::ItemType(item_type as _));
    }
    fn get_weather_fraction_for_pos(&self, world: &World, pos: IntPair) -> f32 {
        world.get_weather_fraction_for_pos(pos)
    }
    fn get_weather_fraction_for_pos_at_world_time(&self, world: &World, pos: IntPair, world_time: f64) -> f32 {
        world.get_weather_fraction_for_pos_at_world_time(pos, world_time)
    }
    fn get_day_night_fraction_for_x_at_world_time(&self, world: &World, x: f32, world_time: f64) -> f32 {
        world.get_day_night_fraction_for_x_at_world_time(x, world_time)
    }
    fn load_light_block_for_client_light_block_index_into_physical_block(
        &self,
        world: &World,
        index: i32,
        pb: PhysicalBlockRef,
    ) {
        world.load_light_block_for_client_light_block_index_into_physical_block(index, &pb);
    }
    fn decommision_block_block_to_save_physcial_block(
        &self,
        world: &World,
        macro_tile: &MacroTileRef,
        block_to_save_physcial_block: bool,
    ) {
        world.decommision_block_block_to_save_physcial_block(macro_tile, block_to_save_physcial_block);
    }
    fn peers_interested_in_macro_index_count(&self, world: &World, macro_index: i32) -> usize {
        world.peers_interested_in_macro_index(macro_index).len()
    }
    fn remove_water_tile_at_pos(&self, world: &World, pos: IntPair) {
        world.remove_water_tile_at_pos(pos);
    }
    fn send_net_data_for_electricity_particle_path_if_required_size_ignore_client(
        &self,
        world: &World,
        path: Vec<crate::types::structs::ElectrictyParticlePathIndex>,
        size: f32,
        ignore_client: Option<&str>,
    ) {
        world.send_net_data_for_electricity_particle_path_if_required_size_ignore_client(path, size, ignore_client);
    }
    fn light_changed_at_macro_pos_send_reliably(&self, macro_pos: IntPair, send_reliably: bool) {
        if let Some(d) = self.dw() {
            d.light_changed_at_macro_pos_send_reliably(macro_pos, send_reliably);
        }
    }
    fn light_changed_at_macro_pos_send_reliably_send_at_all(
        &self,
        macro_pos: IntPair,
        send_reliably: bool,
        send_at_all: bool,
    ) {
        if let Some(d) = self.dw() {
            d.light_changed_at_macro_pos_send_reliably_send_at_all(macro_pos, send_reliably, send_at_all);
        }
    }
    fn explore_light_changed_at_macro_pos_client_light_block_index(
        &self,
        macro_pos: IntPair,
        client_light_block_index: i32,
    ) {
        if let Some(d) = self.dw() {
            d.explore_light_changed_at_macro_pos_client_light_block_index(macro_pos, client_light_block_index);
        }
    }
    fn snow_changed_at_macro_pos(&self, macro_pos: IntPair) {
        if let Some(d) = self.dw() {
            d.snow_changed_at_macro_pos(macro_pos);
        }
    }
    fn water_changed_at_pos_full_block(&self, pos: IntPair, full_block: bool) {
        if let Some(d) = self.dw() {
            d.water_changed_at_pos_full_block(pos, full_block);
        }
    }
    fn has_dynamic_objects_to_save_in_macro_pos(&self, macro_pos: IntPair) -> bool {
        self.dw().map_or(false, |d| d.has_dynamic_objects_to_save_in_macro_pos(macro_pos))
    }
    fn blockheads(&self) -> Vec<ObjRef> {
        self.dw().map_or_else(Vec::new, |d| d.blockheads())
    }
    fn add_torch_at_pos_of_type_data_a_data_b_save_dict_placed_by_client(
        &self,
        pos: IntPair,
        item_type: i32,
        data_a: u16,
        data_b: u16,
        save_dict: Option<&PlistDict>,
        client_id: Option<&str>,
    ) {
        if let Some(d) = self.dw() {
            d.add_torch_at_pos_of_type_data_a_data_b_save_dict_placed_by_client(
                pos, item_type, data_a, data_b, save_dict, client_id,
            );
        }
    }
    fn create_treasure_chest_or_troll_at_tile_at_pos_load_troll_load_treasure(
        &self,
        tile: TileRef,
        pos: IntPair,
        load_troll: bool,
        load_treasure: bool,
    ) {
        if let Some(d) = self.dw() {
            d.create_treasure_chest_or_troll_at_tile_at_pos_load_troll_load_treasure(
                &tile,
                pos,
                load_troll,
                load_treasure,
            );
        }
    }
    fn load_glow_block_if_needed_at_pos_tile(&self, pos: IntPair, tile: TileRef) {
        if let Some(d) = self.dw() {
            d.load_glow_block_if_needed_at_pos_tile(pos, &tile);
        }
    }
    fn load_surface_block_at_pos(&self, pos: IntPair) {
        if let Some(d) = self.dw() {
            d.load_surface_block_at_pos(pos);
        }
    }
    fn remove_standard_object(&self, obj: &ObjRef) {
        if let Some(d) = self.dw() {
            d.remove_standard_object(Some(obj));
        }
    }
    fn blockhead_view_radius(&self, blockhead: &ObjRef) -> i32 {
        bh_of(blockhead).map_or(0, |b| b.view_radius())
    }
    fn water_moved_from_from_tile_to_to_tile_amount(
        &self,
        world: &World,
        from_pos: IntPair,
        from_tile: TileRef,
        to_pos: IntPair,
        to_tile: TileRef,
        amount: i32,
    ) {
        world.water_moved_from_from_tile_to_to_tile_amount(from_pos, Some(from_tile), to_pos, Some(to_tile), amount);
    }
    fn create_free_block_at_position_of_type_data_a_data_b_dynamic_object_save_dict_hovers_play_sound(
        &self,
        pos: IntPair,
        item_type: i32,
        data_a: u16,
        data_b: u16,
        save_dict: PlistDict,
        hovers: bool,
        play_sound: bool,
    ) {
        if let Some(d) = self.dw() {
            d.create_free_block_at_position_of_type_data_a_data_b_sub_items_dynamic_object_save_dict_hovers_play_sound_priority_blockhead(
                pos,
                crate::types::item_type::ItemType(item_type as _),
                data_a,
                data_b,
                None,
                Some(PlistValue::Dict(save_dict)),
                hovers,
                play_sound as i32,
                None,
            );
        }
    }
}

impl Glue {
    #[cfg(test)]
    fn client_receipts_for_test(&self, id: &str) -> Option<Vec<u32>> {
        let w = self.world()?;
        WorldgenHost::client_receipts(self, &w, id, IntPair { x: 0, y: 0 })
    }
    fn tree_noise(&self) -> (crate::trees::tree::NoiseRef, crate::trees::tree::NoiseRef) {
        let l = self.loader();
        let t: crate::trees::tree::NoiseRef = l
            .as_ref()
            .and_then(|l| l.tree_density_noise_function())
            .map(|n| Rc::new(NoiseAdapter(n)) as Rc<dyn DwNoise>);
        let s: crate::trees::tree::NoiseRef = l
            .as_ref()
            .and_then(|l| l.season_offset_noise_function())
            .map(|n| Rc::new(NoiseAdapter(n)) as Rc<dyn DwNoise>);
        (t, s)
    }
    fn item(item_type: i32) -> crate::items::inventory_item::ItemRef {
        crate::items::inventory_item::InventoryItem::new(
            crate::types::item_type::ItemType(item_type as _),
            0,
            0,
            None,
            None,
        )
    }
}

impl crate::npc::egg::PassiveEnv for Glue {
    fn world(&self) -> Option<Rc<World>> {
        Glue::world(self)
    }
    fn dynamic_world(&self) -> Option<Rc<RealDw>> {
        self.dw()
    }
    fn plant_available_food(&self, o: &ObjRef) -> f32 {
        if let Some(p) = crate::plants::plant::as_plant(o) {
            return p.available_food();
        }
        let a = o.as_any();
        if let Some(t) = a.downcast_ref::<crate::trees::apple::AppleTree>() {
            return t.available_food();
        }
        if let Some(t) = a.downcast_ref::<crate::trees::cactus::CactusTree>() {
            return t.available_food();
        }
        if let Some(t) = a.downcast_ref::<crate::trees::pine::PineTree>() {
            return t.available_food();
        }
        0.0
    }
    fn plant_set_available_food(&self, o: &ObjRef, v: f32) {
        if let Some(p) = crate::plants::plant::as_plant(o) {
            p.set_available_food(v);
            return;
        }
        let a = o.as_any();
        if let Some(t) = a.downcast_ref::<crate::trees::apple::AppleTree>() {
            t.set_available_food(o, v);
        } else if let Some(t) = a.downcast_ref::<crate::trees::cactus::CactusTree>() {
            t.set_available_food(v);
        }
    }
    fn plant_type(&self, o: &ObjRef) -> i32 {
        crate::plants::plant::as_plant(o).map_or(0, |p| p.plant_type())
    }

    fn rod_valid(&self, r: &crate::blockhead::blockhead::FishingRodRef) -> bool {
        r.downcast_ref::<crate::physics_objects::fishing_rod::FishingRod>().is_some_and(|f| f.valid())
    }
    fn rod_hook_pos(&self, r: &crate::blockhead::blockhead::FishingRodRef) -> crate::types::structs::Vector2 {
        r.downcast_ref::<crate::physics_objects::fishing_rod::FishingRod>()
            .map_or(crate::types::structs::Vector2::ZERO, |f| f.hook_pos())
    }
    fn rod_hooked_fish(
        &self,
        r: &crate::blockhead::blockhead::FishingRodRef,
    ) -> Option<crate::objects::handle::ObjRef> {
        r.downcast_ref::<crate::physics_objects::fishing_rod::FishingRod>().and_then(|f| f.hooked_fish())
    }
    fn rod_set_hooked_fish(
        &self,
        r: &crate::blockhead::blockhead::FishingRodRef,
        f: Option<&crate::objects::handle::ObjRef>,
    ) {
        if let Some(rod) = r.downcast_ref::<crate::physics_objects::fishing_rod::FishingRod>() {
            rod.set_hooked_fish(f.cloned());
        }
    }
    fn rod_blockhead(&self, r: &crate::blockhead::blockhead::FishingRodRef) -> Option<crate::objects::handle::ObjRef> {
        r.downcast_ref::<crate::physics_objects::fishing_rod::FishingRod>().and_then(|f| f.blockhead())
    }
}

impl DwOpsHooks for Glue {
    fn server_send_network_data_to_peers_reliable(&self, data: Vec<u8>, peers: &[String], reliable: bool) {
        if let Some(s) = self.srv() {
            s.send_network_data_to_peers_reliable(&data, Some(peers), reliable);
        }
    }
    fn object_update_in_time_since_saved(&self, o: &ObjRef) {
        if let Some(b) = o.as_any().downcast_ref::<crate::env::snow_surface_block::SnowSurfaceBlock>() {
            b.update_in_time_since_saved();
        }
    }
    fn new_tree(
        &self,
        dw: &Rc<RealDw>,
        tree_type: i32,
        pos: IntPair,
        max_height: i16,
        growth_rate: i16,
        adult_tree: bool,
        adult_max_age: f32,
    ) -> Option<ObjRef> {
        let w = self.world()?;
        let (t, s) = self.tree_noise();
        crate::trees::tree::new_tree_for_type(
            tree_type,
            &w,
            dw,
            pos,
            None,
            max_height,
            growth_rate,
            t,
            s,
            adult_tree,
            adult_max_age,
        )
    }
    fn new_plant(
        &self,
        dw: &Rc<RealDw>,
        plant_type: i32,
        pos: IntPair,
        max_age_gene: i16,
        growth_rate_gene: i16,
        adult: bool,
    ) -> Option<ObjRef> {
        let w = self.world()?;
        let l = self.loader();
        let t = l.as_ref().and_then(|l| l.tree_density_noise_function());
        let s = l.as_ref().and_then(|l| l.season_offset_noise_function());
        crate::plants::plant::new_plant_for_type(
            plant_type,
            &w,
            dw,
            pos,
            None,
            max_age_gene,
            growth_rate_gene,
            t,
            s,
            adult,
        )
    }
    fn new_npc(
        &self,
        dw: &Rc<RealDw>,
        npc_type: i32,
        pos: IntPair,
        save_dict: Option<&PlistDict>,
        is_adult: bool,
        was_placed: bool,
        client_id: Option<&str>,
    ) -> Option<ObjRef> {
        let w: crate::objects::base::WorldRef = self.world()?;
        let d: crate::objects::base::DynamicWorldRef = dw.clone();
        crate::npc::npc::new_npc_for_type(npc_type, &w, &d, pos, None, save_dict, is_adult, was_placed, client_id)
    }
    fn new_workbench(
        &self,
        dw: &Rc<RealDw>,
        pos: IntPair,
        ty: i32,
        flipped: bool,
        save_dict: Option<&PlistDict>,
        client_id: Option<&str>,
        client_name: Option<&str>,
    ) -> Option<ObjRef> {
        let w = self.world()?;
        let d: crate::objects::base::DynamicWorldRef = dw.clone();
        crate::crafting::workbench::Workbench::init_with_world_dynamic_world_at_position_cache_type_flipped_save_dict_placed_by_client_client_name(
            &w,
            &d,
            pos,
            None,
            crate::types::enums::WorkbenchType(ty as u32),
            flipped,
            save_dict,
            client_id,
            client_name,
        )
        .map(|o| o as ObjRef)
    }
    fn new_trade_portal(
        &self,
        dw: &Rc<RealDw>,
        pos: IntPair,
        item_type: i32,
        flipped: bool,
        save_dict: Option<&PlistDict>,
        client_id: Option<&str>,
        client_name: Option<&str>,
    ) -> Option<ObjRef> {
        let w = self.world()?;
        let it = Self::item(item_type);
        let it = it.borrow();
        crate::interaction::trade_portal::TradePortal::new_at_position_item_flipped_save_dict_placed_by_client_client_name(&w, dw, pos, None, Some(&*it), flipped, save_dict, client_id, client_name)
            .map(|o| o as ObjRef)
    }
    fn new_chest(
        &self,
        dw: &Rc<RealDw>,
        pos: IntPair,
        item_type: i32,
        flipped: bool,
        save_dict: Option<&PlistDict>,
        client_id: Option<&str>,
        client_name: Option<&str>,
    ) -> Option<ObjRef> {
        let w = self.world()?;
        let it = Self::item(item_type);
        let it = it.borrow();
        crate::interaction::chest::Chest::new_at_position_item_flipped_save_dict_placed_by_client_client_name(
            &w,
            dw,
            pos,
            None,
            Some(&*it),
            flipped,
            save_dict,
            client_id,
            client_name,
        )
        .map(|o| o as ObjRef)
    }
    // r4_longrun_0/1: forwarders to the real object methods (the trait defaults are inert constants).
    /// `-[NPC canMate]` as seen by `npcCloseEnoughToBreedWithNPC:`.
    fn npc_can_mate(&self, npc: &ObjRef) -> bool {
        crate::npc::npc::as_npc(npc).map_or(false, |n| n.can_mate())
    }
    fn npc_can_be_fed_by_blockhead(&self, npc: &ObjRef, bh: &ObjRef) -> bool {
        crate::npc::npc::as_npc(npc).is_some() && crate::npc::npc::npc_can_be_fed_by_blockhead(npc, bh)
    }
    fn freeblock_hovers(&self, fb: &ObjRef) -> bool {
        fb.as_any().downcast_ref::<crate::physics_objects::free_block::FreeBlock>().map_or(false, |f| f.hovers())
    }
    fn tree_height(&self, tree: &ObjRef) -> i32 {
        if crate::trees::tree::as_tree(tree).is_some() {
            crate::trees::tree::tree_height(tree)
        } else {
            0
        }
    }
    fn tree_max_height_gene_variation(&self, tree: &ObjRef) -> i32 {
        if crate::trees::tree::as_tree(tree).is_some() {
            crate::trees::tree::tree_max_height_gene_variation(tree) as i32
        } else {
            0
        }
    }
    fn tree_growth_rate_gene_variation(&self, tree: &ObjRef) -> i32 {
        if crate::trees::tree::as_tree(tree).is_some() {
            crate::trees::tree::tree_growth_rate_gene_variation(tree) as i32
        } else {
            0
        }
    }
    /// The tree's own `-isRequiredSoilType:` (cactus / coconut override the dirt family of the base class).
    fn tree_is_required_soil_type(&self, tree: &ObjRef, soil: u8) -> bool {
        crate::trees::tree::as_tree(tree).is_some_and(|t| t.is_required_soil_type(soil as i32))
    }
    // ---- doors (`setDoorAtPos:toOpen:direction:`, `doorIsOpenAtPos:`, path admission)
    fn door_can_be_used_by_blockhead(&self, door: &ObjRef, bh: &ObjRef) -> bool {
        door_of(door).is_some_and(|d| d.can_be_used_by_blockhead(door, bh))
    }
    fn door_is_open(&self, door: &ObjRef) -> bool {
        door_of(door).is_some_and(|d| d.is_open())
    }
    fn door_set_open_direction(&self, door: &ObjRef, open: bool, direction: i32) {
        if let Some(d) = door_of(door) {
            d.set_open_direction(door, open, direction);
        }
    }
    // ---- chests: the server side of opening a chest (`sendChestInventoryForChest:toClientOwningBlockheadWithID:`)
    // and the treasure-chest fill. Inert, every chest opened by a client came back empty.
    fn chest_load_inventory_items_from_disk_if_needed(&self, chest: &ObjRef) {
        if let Some(c) = chest_of(chest) {
            c.load_inventory_items_from_disk_if_needed();
        }
    }
    fn chest_inventory_data(&self, chest: &ObjRef, allow_empty: bool) -> Option<Vec<u8>> {
        chest_of(chest)?.inventory_data_allowing_empty(allow_empty)
    }
    fn chest_remote_inventory_data_recieved(&self, chest: &ObjRef, data: Option<&[u8]>) {
        if let Some(c) = chest_of(chest) {
            c.remote_inventory_data_recieved(data);
        }
    }
    fn is_chest(&self, o: &ObjRef) -> bool {
        chest_of(o).is_some()
    }
    fn chest_move_inventory_items_from_array_to_index_count(
        &self,
        chest: &ObjRef,
        coins: &mut Vec<crate::items::inventory_item::ItemRef>,
        index: i32,
        count: i32,
    ) {
        if let Some(c) = chest_of(chest) {
            let from: crate::items::inventory_item::SubSlot = Rc::new(RefCell::new(std::mem::take(coins)));
            c.move_inventory_items_from_array_to_index_count(chest, &from, index, count);
            *coins = std::mem::take(&mut *from.borrow_mut());
        }
    }

    fn new_torch(
        &self,
        dw: &std::rc::Rc<crate::dynworld::state::DynamicWorld>,
        pos: crate::types::structs::IntPair,
        ty: i32,
        data_a: u16,
        data_b: u16,
        save_dict: Option<&crate::plist::value::PlistDict>,
        client_id: Option<&str>,
    ) -> Option<crate::objects::handle::ObjRef> {
        let w = self.world()?;
        let d: crate::objects::base::DynamicWorldRef = dw.clone();
        crate::lighting::torch::Torch::init_with_world_dynamic_world_at_position_cache_type_data_a_data_b_save_dict_placed_by_client(
            &w, &d, pos, None, ty, data_a, data_b, save_dict, client_id,
        )
        .map(|o| o as ObjRef)
    }
    fn new_egg(
        &self,
        dw: &std::rc::Rc<crate::dynworld::state::DynamicWorld>,
        pos: crate::types::structs::IntPair,
        save_dict: Option<&crate::plist::value::PlistDict>,
    ) -> Option<crate::objects::handle::ObjRef> {
        let w: crate::objects::base::WorldRef = self.world()?;
        let d: crate::objects::base::DynamicWorldRef = dw.clone();
        crate::npc::egg::Egg::init_with_world_dynamic_world_at_position_cache_save_dict(&w, &d, pos, None, save_dict)
    }
    fn new_painting(
        &self,
        dw: &std::rc::Rc<crate::dynworld::state::DynamicWorld>,
        pos: crate::types::structs::IntPair,
        ty: i32,
        save_dict: Option<&crate::plist::value::PlistDict>,
        client_id: Option<&str>,
        client_name: Option<&str>,
    ) -> Option<crate::objects::handle::ObjRef> {
        let w = self.world()?;
        crate::interaction::painting::Painting::new(&w, dw, pos, None, ty, save_dict, client_id, client_name)
            .map(|o| o as ObjRef)
    }
    fn new_rail(
        &self,
        dw: &std::rc::Rc<crate::dynworld::state::DynamicWorld>,
        pos: crate::types::structs::IntPair,
        ty: i32,
        owned_by_station: bool,
    ) -> Option<crate::objects::handle::ObjRef> {
        let w = self.world()?;
        crate::trains::rail::Rail::new(&w, dw, pos, None, ty, owned_by_station)
    }
    fn new_boat(
        &self,
        dw: &std::rc::Rc<crate::dynworld::state::DynamicWorld>,
        pos: crate::types::structs::IntPair,
        save_dict: Option<&crate::plist::value::PlistDict>,
        client_id: Option<&str>,
    ) -> Option<crate::objects::handle::ObjRef> {
        let w = self.world()?;
        crate::physics_objects::boat::Boat::new(&w, dw, pos, None, save_dict, client_id)
    }
    fn new_train_car(
        &self,
        dw: &std::rc::Rc<crate::dynworld::state::DynamicWorld>,
        item_type: i32,
        pos: crate::types::structs::IntPair,
        save_dict: Option<&crate::plist::value::PlistDict>,
        client_id: Option<&str>,
    ) -> Option<crate::objects::handle::ObjRef> {
        let w = self.world()?;
        match item_type {
            204 => crate::trains::hand_car::HandCar::new(&w, dw, pos, None, save_dict, client_id),
            205 => crate::trains::steam_train::SteamTrain::new(&w, dw, pos, None, save_dict, client_id),
            206 => crate::trains::freight_car::FreightCar::new(&w, dw, pos, None, save_dict, client_id),
            208 => crate::trains::passenger_car::PassengerCar::new(&w, dw, pos, None, save_dict, client_id),
            _ => None,
        }
    }
    fn new_glow_block(
        &self,
        dw: &std::rc::Rc<crate::dynworld::state::DynamicWorld>,
        pos: crate::types::structs::IntPair,
        tile: crate::world::state::TileRef,
    ) -> Option<crate::objects::handle::ObjRef> {
        let w = self.world()?;
        let d: crate::objects::base::DynamicWorldRef = dw.clone();
        crate::lighting::glow_block::GlowBlock::init_with_world_dynamic_world_at_position_cache_tile(
            &w, &d, pos, None, &tile,
        )
        .map(|o| o as ObjRef)
    }
    fn new_blockhead(
        &self,
        dw: &std::rc::Rc<crate::dynworld::state::DynamicWorld>,
        pos: crate::types::structs::IntPair,
        blockhead_number: i32,
        cio: &crate::objects::handle::ObjRef,
        unique_id: u64,
    ) -> Option<crate::objects::handle::ObjRef> {
        let w = self.world()?;
        let d: crate::objects::base::DynamicWorldRef = dw.clone();
        let shim = crate::crafting::workbench_craft::craftable_item_object_of_shim(cio);
        let bcio = shim.as_ref().and_then(|c| {
            c.as_any().downcast_ref::<crate::crafting::craftable_item_object::BlockheadCraftableItemObject>()
        });
        crate::blockhead::blockhead::Blockhead::init_with_world_dynamic_world_at_position_cache_blockhead_number_craftable_item_object_unique_id(
            &w,
            &d,
            pos,
            None,
            blockhead_number,
            bcio.map(|b| b as &dyn crate::blockhead::lifecycle::BlockheadCraftableItemObjectLike),
            unique_id,
        )
    }
    fn tree_type(&self, tree: &crate::objects::handle::ObjRef) -> i32 {
        crate::trees::tree::as_tree(tree).map_or(0, |t| t.tree_type())
    }
    fn plant_type(&self, plant: &crate::objects::handle::ObjRef) -> i32 {
        crate::plants::plant::as_plant(plant).map_or(0, |p| p.plant_type())
    }
    fn plant_max_age_gene_variation(&self, plant: &crate::objects::handle::ObjRef) -> i32 {
        crate::plants::plant::as_plant(plant).map_or(0, |p| p.max_age_gene_variation() as i32)
    }
    fn plant_growth_rate_gene_variation(&self, plant: &crate::objects::handle::ObjRef) -> i32 {
        crate::plants::plant::as_plant(plant).map_or(0, |p| p.growth_rate_gene_variation() as i32)
    }
    fn plant_is_required_soil_type(&self, plant: &crate::objects::handle::ObjRef, soil: u8) -> bool {
        crate::plants::plant::as_plant(plant).is_some_and(|p| p.is_required_soil_type(soil))
    }
    fn plant_can_breed(&self, plant: &crate::objects::handle::ObjRef) -> bool {
        crate::plants::plant::as_plant(plant).is_some_and(|p| p.can_breed())
    }
    fn plant_number_of_occupied_tiles_above(&self, plant: &crate::objects::handle::ObjRef) -> i32 {
        crate::plants::plant::as_plant(plant).map_or(0, |p| p.number_of_occupied_tiles_above())
    }
    fn plant_number_of_occupied_tiles_below(&self, plant: &crate::objects::handle::ObjRef) -> i32 {
        crate::plants::plant::as_plant(plant).map_or(0, |p| p.number_of_occupied_tiles_below())
    }
    fn tap_is_within_body_radius(
        &self,
        o: &crate::objects::handle::ObjRef,
        tap: crate::types::structs::Vector2,
    ) -> bool {
        if let Some(b) = crate::objects::handle::downcast::<crate::physics_objects::boat::Boat>(o) {
            b.tap_is_within_body_radius(tap)
        } else if let Some(t) = crate::trains::train_car::train_car_of(o) {
            t.tap_is_within_body_radius(tap)
        } else {
            false
        }
    }
    fn blockhead_current_item_type(&self, bh: &crate::objects::handle::ObjRef) -> i32 {
        bh_of(bh).and_then(|b| b.current_item()).map_or(0, |i| i.borrow().item_type.0 as i32)
    }
    fn blockhead_goal_interaction_for_npc_chase(
        &self,
        bh: &crate::objects::handle::ObjRef,
        npc: &crate::objects::handle::ObjRef,
        item_type: i32,
    ) -> i32 {
        bh_of(bh).map_or(0, |b| {
            b.goal_interaction_for_npc_chase_for_npc_with_item_type(
                npc,
                crate::types::item_type::ItemType(item_type as u32),
            ) as i32
        })
    }
    fn blockhead_crouching(&self, bh: &crate::objects::handle::ObjRef) -> bool {
        bh_of(bh).is_some_and(|b| b.crouching())
    }
    fn blockhead_teleport_to_pos(
        &self,
        bh: &crate::objects::handle::ObjRef,
        pos: crate::types::structs::IntPair,
    ) -> bool {
        bh_of(bh).is_some_and(|b| b.teleport_to_pos(pos))
    }
    fn blockhead_stop_all_actions(&self, bh: &crate::objects::handle::ObjRef) {
        if let Some(b) = bh_of(bh) {
            b.stop_all_actions();
        }
    }
    fn blockhead_set_paused(&self, bh: &crate::objects::handle::ObjRef, paused: bool) {
        if let Some(b) = bh_of(bh) {
            b.set_paused(paused);
        }
    }
    fn blockhead_remote_pickup_request_response(
        &self,
        bh: &crate::objects::handle::ObjRef,
        accepted: bool,
        ids: &[u64],
    ) {
        if let Some(b) = bh_of(bh) {
            b.remote_pickup_request_response(accepted, ids);
        }
    }
    fn blockhead_update_name_text_view(&self, bh: &crate::objects::handle::ObjRef) {
        // `-[Blockhead updateNameTextView]` is CLIENT-ONLY (name label rendering): nothing on the server.
        let _ = bh;
    }
    fn elevator_shaft_open(&self, shaft: &crate::objects::handle::ObjRef) {
        if let Some(s) = crate::objects::handle::downcast::<crate::building::elevator::ElevatorShaft>(shaft) {
            s.open();
        }
    }
    fn elevator_shaft_last_known_motor_pos(
        &self,
        shaft: &crate::objects::handle::ObjRef,
    ) -> crate::types::structs::IntPair {
        crate::objects::handle::downcast::<crate::building::elevator::ElevatorShaft>(shaft)
            .map_or(crate::types::structs::IntPair { x: 0, y: 0 }, |s| s.last_known_motor_pos())
    }
    fn elevator_motor_min_y(&self, motor: &crate::objects::handle::ObjRef) -> i32 {
        crate::objects::handle::downcast::<crate::building::elevator::ElevatorMotor>(motor).map_or(0, |m| m.min_y())
    }
    fn elevator_motor_max_y(&self, motor: &crate::objects::handle::ObjRef) -> i32 {
        crate::objects::handle::downcast::<crate::building::elevator::ElevatorMotor>(motor).map_or(0, |m| m.max_y())
    }
    fn train_car_rail_or_station_name_changed(&self, car: &crate::objects::handle::ObjRef) {
        if let Some(t) = crate::trains::train_car::train_car_of(car) {
            t.rail_or_station_name_changed();
        }
    }
    fn train_car_set_paused(&self, car: &crate::objects::handle::ObjRef, paused: bool) {
        // `-[TrainCar setPaused:]` is empty and no train car class overrides it.
        let _ = (car, paused);
    }
    fn painting_image_data(&self, painting: &crate::objects::handle::ObjRef) -> Option<Vec<u8>> {
        crate::objects::handle::downcast::<crate::interaction::painting::Painting>(painting)?.image_data()
    }
    fn painting_image_data_recieved(&self, painting: &crate::objects::handle::ObjRef, data: &[u8]) {
        // Client path (`-[DynamicWorld paintingDataRecievedFromServer:]` handles a received 0x31); the dedicated server
        // never receives painting data, so `-[Painting imageDataRecieved:]` is never sent there.
        let _ = (painting, data);
    }
    fn painting_user_mute_changed(&self, painting: &crate::objects::handle::ObjRef, player_id: &str) {
        if let Some(p) = crate::objects::handle::downcast::<crate::interaction::painting::Painting>(painting) {
            p.user_mute_changed(player_id);
        }
    }
    fn painting_user_ban_changed(&self, painting: &crate::objects::handle::ObjRef, player_id: &str, banned: bool) {
        if let Some(p) = crate::objects::handle::downcast::<crate::interaction::painting::Painting>(painting) {
            p.user_ban_changed_is_banned(painting, player_id, banned);
        }
    }
    fn trade_portal_randomize_local_trade_offsets(&self, portal: &crate::objects::handle::ObjRef) {
        if let Some(p) = crate::objects::handle::downcast::<crate::interaction::trade_portal::TradePortal>(portal) {
            p.randomize_local_trade_offsets(portal);
        }
    }
    fn interaction_object_land_owner_id(&self, o: &crate::objects::handle::ObjRef) -> Option<String> {
        crate::objects::handle::downcast::<crate::interaction::ownership_sign::OwnershipSign>(o)?.land_owner_id()
    }
    fn interaction_object_width_radius(&self, o: &crate::objects::handle::ObjRef) -> i32 {
        crate::objects::handle::downcast::<crate::interaction::ownership_sign::OwnershipSign>(o)
            .map_or(0, |s| s.width_radius())
    }
    fn interaction_object_height_radius(&self, o: &crate::objects::handle::ObjRef) -> i32 {
        crate::objects::handle::downcast::<crate::interaction::ownership_sign::OwnershipSign>(o)
            .map_or(0, |s| s.height_radius())
    }
    fn world_remove_tile_at_world_x_world_y(&self, x: i32, y: i32) {
        if let Some(w) = self.world() {
            w.remove_tile_at_world_x_world_y_create_contents_freeblock_count_create_foreground_contents_freeblock_count_remove_blockhead(
                x, y, 0, 0, None,
            );
        }
    }
    fn world_ownership_sign_was_placed_or_changed_at_pos(
        &self,
        pos: crate::types::structs::IntPair,
        land_owner_id: Option<&str>,
        width_radius: i32,
        height_radius: i32,
        was_removed: bool,
    ) {
        if let Some(w) = self.world() {
            w.ownership_sign_was_placed_or_changed_at_pos_with_land_owner_width_radius_height_radius_was_removed(
                pos,
                land_owner_id,
                width_radius,
                height_radius,
                was_removed,
            );
        }
    }
    fn world_welcome_back_events_message_for_client_id(
        &self,
        client_id: &str,
    ) -> Option<crate::plist::value::PlistValue> {
        self.world()?.welcome_back_events_message_for_client_id(client_id).map(PlistValue::string)
    }
    fn server_player_name_for_player_with_id_including_old_players(&self, player_id: &str) -> Option<String> {
        self.srv()?.player_name_for_player_with_id_including_old_players(Some(player_id))
    }
    fn server_player_is_banned_with_id(&self, player_id: &str) -> bool {
        self.srv().is_some_and(|s| s.player_is_banned_with_id(Some(player_id)))
    }
    fn found_items_list_snapshot(&self, client_id: &str) -> Option<crate::foundation::indexset::IndexSet> {
        self.client(client_id).map(|c| c.found_items_list().borrow().clone())
    }
    fn found_items_list_remove_all_indexes(&self, client_id: &str) {
        if let Some(c) = self.client(client_id) {
            c.found_items_list().borrow_mut().remove_all_indexes();
        }
    }
    fn found_items_list_add_indexes(&self, client_id: &str, set: &crate::foundation::indexset::IndexSet) {
        if let Some(c) = self.client(client_id) {
            c.found_items_list().borrow_mut().add_indexes(set);
        }
    }
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
        use crate::fmath::vector::Vector;
        crate::env::particle_emitter::ParticleEmitter::instance()
            .add_particle_at_pos_velocity_color_gravity_type_life_scale_center(
                Vector::new(pos[0], pos[1], pos[2]),
                Vector::new(velocity[0], velocity[1], velocity[2]),
                Vector::new4(color[0], color[1], color[2], color[3]),
                gravity_type,
                life,
                scale,
                Vector::new4(center[0], center[1], center[2], center[3]),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::server::bhserver::test_support::Harness;
    use crate::world::lifecycle::core_hooks;

    /// r4_longrun_0/1: `Glue` forwards `-[NPC canMate]` instead of the
    /// trait's constant `false`, so `npcCloseEnoughToBreedWithNPC:` can find a partner.
    #[test]
    fn glue_npc_can_mate_forwards_real_predicate() {
        isolated(|| {
            use crate::objects::base::{testutil::setup, DynamicWorldRef, WorldRef};
            let (w, dw, _l) = setup(4, 4);
            let h = Harness::new("glue_mate");
            let g = Glue::new(&h.srv);
            let wr: WorldRef = w.clone();
            let dr: DynamicWorldRef = dw.clone();
            let mk = |adult| {
                crate::npc::dodo::Dodo::init_with_world_dynamic_world_at_position_cache_save_dict_is_adult_was_placed_placed_by_client(
                    &wr, &dr, IntPair { x: 40, y: 3 }, None, None, adult, false, None,
                )
                .unwrap()
            };
            let adult = mk(true);
            let n = crate::npc::npc::as_npc(&adult).unwrap();
            n.npc().borrow_mut().age = 1000.0;
            n.npc().borrow_mut().mate_cooldown_timer = 0.0;
            assert!(DwOpsHooks::npc_can_mate(&*g, &adult));
            n.npc().borrow_mut().mate_cooldown_timer = 5.0;
            assert!(!DwOpsHooks::npc_can_mate(&*g, &adult));
            n.npc().borrow_mut().mate_cooldown_timer = 0.0;
            n.npc().borrow_mut().age = 100.0;
            assert!(!DwOpsHooks::npc_can_mate(&*g, &adult));
        });
    }

    /// Chest / freight-car / door hooks forward to the objects (inert for other classes). Before, every chest hook
    /// kept its inert default: chest contents were never sent back to a client nor saved.
    #[test]
    fn glue_chest_door_and_freight_hooks_forward_to_the_objects() {
        isolated(|| {
            use crate::interaction::chest::Chest;
            use crate::items::inventory_item::InventoryItem;
            use crate::types::item_type::ItemType;
            let h = Harness::new("glue_chest");
            let g = Glue::new(&h.srv);
            let d = crate::dynworld::state::testutil::dw("glue_chest", 4);
            let w = d.world();
            let it = InventoryItem::new(ItemType(0x413), 0, 0, None, None);
            let chest: ObjRef =
                Chest::new(&w, &d, IntPair { x: 40, y: 3 }, None, Some(&it.borrow()), false, None, Some("cid"), None)
                    .unwrap();
            assert!(DwOpsHooks::is_chest(&*g, &chest));
            DwOpsHooks::chest_load_inventory_items_from_disk_if_needed(&*g, &chest);
            assert!(DwOpsHooks::chest_inventory_data(&*g, &chest, false).is_none(), "empty chest");
            assert!(DwHooks::chest_inventory_data_allowing_empty(&*g, &chest).is_some());
            let mut coins: Vec<_> = (0..3).map(|_| InventoryItem::new(ItemType(166), 0, 0, None, None)).collect();
            DwOpsHooks::chest_move_inventory_items_from_array_to_index_count(&*g, &chest, &mut coins, 0, 2);
            assert_eq!(coins.len(), 1, "two coins moved into the chest, one left");
            assert!(DwOpsHooks::chest_inventory_data(&*g, &chest, false).is_some());
            let sd = DwHooks::chest_save_dict_excluding_inventory(&*g, &chest);
            let Some(PlistValue::Dict(sd)) = sd else { panic!("{:?}", sd.is_some()) };
            assert!(!sd.map.contains_key(&"saveItemSlots".to_string()));
            assert!(sd.map.contains_key(&"chestType".to_string()));
            DwOpsHooks::chest_remote_inventory_data_recieved(&*g, &chest, None);
            assert!(DwOpsHooks::chest_inventory_data(&*g, &chest, false).is_none(), "reset to empty slots");

            let door = crate::building::door::Door::init_with_world_dynamic_world_at_position_cache_type_save_dict_placed_by_client(
                &w,
                &d,
                IntPair { x: 50, y: 3 },
                None,
                0x89,
                None,
                None,
            )
            .unwrap();
            assert!(!DwOpsHooks::door_is_open(&*g, &door));
            DwOpsHooks::door_set_open_direction(&*g, &door, true, 1);
            assert!(DwOpsHooks::door_is_open(&*g, &door));
            let _ = DwOpsHooks::door_can_be_used_by_blockhead(&*g, &door, &chest);

            // wrong classes: the inert answers
            assert!(!DwOpsHooks::is_chest(&*g, &door));
            DwOpsHooks::chest_load_inventory_items_from_disk_if_needed(&*g, &door);
            DwOpsHooks::chest_remote_inventory_data_recieved(&*g, &door, None);
            let mut none = Vec::new();
            DwOpsHooks::chest_move_inventory_items_from_array_to_index_count(&*g, &door, &mut none, 0, 1);
            assert!(DwOpsHooks::chest_inventory_data(&*g, &door, true).is_none());
            assert!(DwHooks::chest_inventory_data_allowing_empty(&*g, &door).is_none());
            assert!(DwHooks::chest_save_dict_excluding_inventory(&*g, &door).is_none());
            assert!(!DwOpsHooks::door_is_open(&*g, &chest));
            assert!(!DwOpsHooks::door_can_be_used_by_blockhead(&*g, &chest, &door));
            DwOpsHooks::door_set_open_direction(&*g, &chest, true, 1);
            assert!(!DwHooks::freight_needs_chest_save(&*g, &door));
            DwHooks::freight_set_needs_chest_save(&*g, &door, true);
            assert!(DwHooks::freight_chest_save_dict(&*g, &door).is_none());

            let car = crate::trains::freight_car::FreightCar::new(&w, &d, IntPair { x: 60, y: 3 }, None, None, None)
                .expect("freight car not constructed");
            DwHooks::freight_set_needs_chest_save(&*g, &car, true);
            assert!(DwHooks::freight_needs_chest_save(&*g, &car));
            DwHooks::freight_set_needs_chest_save(&*g, &car, false);
            assert!(!DwHooks::freight_needs_chest_save(&*g, &car));
            assert!(matches!(DwHooks::freight_chest_save_dict(&*g, &car), Some(PlistValue::Dict(_))));
        });
    }

    /// The blockhead seams installed in production: `updatePosition:` of a client's blockhead loads / explores the
    /// area around it (the "new area stays dark" bug) and reaches doors.
    #[test]
    fn blockhead_glue_forwards_world_loading_and_is_inert_without_a_world() {
        isolated(|| {
            use crate::blockhead::blockhead::BlockheadHost;
            let (bw, _mock, bdw) = crate::blockhead::blockhead::testutil::setup();
            let bh_obj = crate::blockhead::blockhead::testutil::new_bh(&bw, &bdw, 1);
            let b = crate::blockhead::blockhead::testutil::bh_of(&bh_obj);
            let p = IntPair { x: 6, y: 8 };

            // no glue / no world / no DynamicWorld: inert
            let dead = BlockheadGlue(Weak::new());
            assert_eq!(dead.server_client_light_block_index_for_net_id(Some("x")), None);
            dead.fully_load_if_needed_around_pos_client_light_block_index_for_blockhead(p, 0, None);
            dead.set_door_at_pos_to_open_direction(p, true, 1);

            let h = Harness::new("glue_bh");
            let g = Glue::new(&h.srv);
            let live = BlockheadGlue(Rc::downgrade(&g));
            assert_eq!(live.server_client_light_block_index_for_net_id(None), None);
            assert_eq!(live.server_client_light_block_index_for_net_id(Some("nobody")), None);
            g.install_pre_init();
            core_hooks().server_client_create("abc", 7, None);
            assert_eq!(live.server_client_light_block_index_for_net_id(Some("abc")), Some(7));

            // update_position runs the full Blockhead version (moves the blockhead)
            live.update_position(b, p);
            assert_eq!(bh_obj.pos(), p);

            // with a world and a DynamicWorld the calls reach them
            let d = crate::dynworld::state::testutil::dw("glue_bh", 4);
            let w = d.world();
            *g.world.borrow_mut() = Rc::downgrade(&w);
            *g.dw.borrow_mut() = Some(d.clone());
            live.fully_load_if_needed_around_pos_client_light_block_index_for_blockhead(p, -1, None);
            live.set_door_at_pos_to_open_direction(p, true, 1);
            g.shutdown();
        });
    }

    /// The seams are thread-locals: run each body on its own thread so a test never leaks hooks into another.
    fn isolated(f: impl FnOnce() + Send + 'static) {
        std::thread::spawn(f).join().expect("test body panicked");
    }

    ///  A1/A2: after `install_pre_init` the World-side hooks are live (not `NullCoreHooks`):
    /// server queries reach the BHServer and `server_client_create` makes ONE real ServerClient per peer that the
    /// ServerWorld face, the dynworld map and the hooks all share.
    #[test]
    fn glue_forwards_server_queries_and_shares_one_client_object() {
        isolated(|| {
            let h = Harness::new("glue_basic");
            let g = Glue::new(&h.srv);
            g.install_pre_init();
            let hooks = core_hooks();
            assert!(!hooks.server_is_cloud_match());
            assert_eq!(hooks.server_credit(), 0.0);
            assert_eq!(hooks.server_privacy_string(), None);
            assert_eq!(hooks.server_local_player_id().as_deref(), Some(h.srv.local_player_id().as_str()));

            assert!(!hooks.server_client_connected("abc"));
            hooks.server_client_create("abc", 3, None);
            assert!(hooks.server_client_connected("abc"));
            assert_eq!(hooks.server_client_light_block_index("abc"), 3);
            assert_eq!(g.dw_clients.borrow().len(), 1);
            let a = g.client("abc").expect("client");
            a.set_requests_heart_beat(true);
            assert!(crate::world::misc::deps().sc_requests_heart_beat("abc"));
            hooks.server_client_set_connected("abc", false);
            assert!(!a.connected());
            assert!(!crate::world::misc::deps().sc_connected("abc"));
            assert_eq!(crate::world::misc::deps().sc_light_block_index("nobody"), -1);
        });
    }

    /// Nil receivers: before stage 1 / stage 12 there is no loader / DynamicWorld and the hooks answer with the
    /// ObjC nil-messaging defaults; `dw_face` is `None` (WorldServerAdapter::dynamic_world returns None).
    #[test]
    fn nil_loader_and_dynamic_world_give_defaults() {
        isolated(|| {
            let h = Harness::new("glue_nil");
            let g = Glue::new(&h.srv);
            assert!(g.dw_face().is_none());
            assert!(g.loader().is_none());
            assert_eq!(g.loader_highest_point(), IntPair { x: 0, y: 0 });
            assert_eq!(g.loader_distance_ordered_food_types(), vec![0; 31]);
            assert_eq!(g.loader_x_frequency_multiplier(), 0);
            assert!(g.loader_finish_bulk_light_block_transaction());
            assert_eq!(g.dynamic_world_portal_level(), None);
            assert!(g.dynamic_world_load_dynamic_objects_reposition_blockhead_load_failures(true, true));
            g.shutdown();
        });
    }

    /// The bulk transaction pair is forwarded to the BHServer (r1_cli_1 B3): the finish call after a start returns
    /// without panicking and a second finish is the BHServer's answer, not the inert `true`.
    #[test]
    fn bulk_transaction_is_forwarded() {
        isolated(|| {
            let h = Harness::new("glue_bulk");
            let g = Glue::new(&h.srv);
            g.server_start_bulk_transaction();
            let _ = g.server_finish_bulk_transaction();
        });
    }

    /// r2_cli_0 / r2_worldgen_0: the dict narrowing used by every DynamicWorld forwarder (non-dict -> nil).
    #[test]
    fn dict_of_narrows_only_dicts() {
        assert!(dict_of(None).is_none());
        assert!(dict_of(Some(&PlistValue::int(3))).is_none());
        assert!(dict_of(Some(&PlistValue::mutable_dict())).is_some());
    }

    /// r2_session_0 (a): SubSlot <-> owned conversion keeps order and does not filter.
    #[test]
    fn sub_items_roundtrip_keeps_order() {
        use crate::items::inventory_item::InventoryItem;
        use crate::types::item_type::ItemType;
        let slots = sub_items_to_slots(vec![
            vec![InventoryItem {
                item_type: ItemType(5),
                data_a: 1,
                data_b: 2,
                sub_items: None,
                selected_sub_item_index: 0,
                dynamic_object_save_dict: None,
            }],
            vec![],
        ]);
        assert_eq!(slots.len(), 2);
        assert_eq!(slots[0].borrow().len(), 1);
        assert!(slots[1].borrow().is_empty());
        let back = sub_items_to_owned(&slots);
        assert_eq!(back.len(), 2);
        assert_eq!((back[0][0].item_type, back[0][0].data_a, back[0][0].data_b), (ItemType(5), 1, 2));
    }

    /// r2_cli_0 / r2_console_1: with no DynamicWorld yet the forwarders are inert (ObjC nil messaging) and do not panic.
    #[test]
    fn dynamic_world_forwarders_are_inert_without_dw() {
        isolated(|| {
            let h = Harness::new("glue_fwd_nil");
            let g = Glue::new(&h.srv);
            WorldOpsDeps::workbench_placed_at_position_of_type_save_dict_placed_by_client_client_name(
                &*g,
                IntPair { x: 1, y: 2 },
                crate::types::enums::WorkbenchType::WORKBENCH_BASIC_PORTAL,
                None,
                None,
                None,
            );
            WorldOpsDeps::place_fire_at_position(&*g, IntPair { x: 1, y: 2 });
            assert!(g.client_receipts_for_test("nobody").is_none());
        });
    }
}
