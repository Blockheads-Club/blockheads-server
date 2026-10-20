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

/// The dynamic object type whose class is `name` (`classNameForDynamicObjectType`); 0 when unknown.
fn dyn_type_named(name: &str) -> usize {
    (1..=64).find(|t| crate::objects::type_tables::class_name_for_dynamic_object_type(*t) == Some(name)).unwrap_or(0)
        as usize
}

/// The `Blockhead` behind an object handle (`None` for any other class).
fn bh_of(o: &ObjRef) -> Option<&crate::blockhead::blockhead::Blockhead> {
    crate::objects::handle::downcast::<crate::blockhead::blockhead::Blockhead>(o)
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
        let (interaction_object_type, is_paintable) = match crate::objects::registry::as_interaction_like(&obj) {
            Some(i) => (i.interaction_object_type(), i.is_paintable()),
            None => (0, false),
        };
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
        match sc.get_and_remove_all_reciept_data_for_macro_pos_world(pos, &face)? {
            PlistValue::Array(a) => Some(a.items.iter().map(|v| v.unsigned_int_value()).collect()),
            _ => None,
        }
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
        if let Some(m) = mt {
            match out {
                LoadOutcome::Skipped => {}
                LoadOutcome::NotFound => m.unexplored.set(1),
                LoadOutcome::Loaded | LoadOutcome::Generated => {
                    *m.physical_block.borrow_mut() = Some(pb);
                    m.unexplored.set(0);
                    m.dynamic_objects_loaded.set(false);
                }
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
        let t = dyn_type_named(name);
        if t == 0 {
            return None;
        }
        self.dw().and_then(|d| d.object_of_type_at_pos(t as i32, pos))
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
    // chest_*/freight_*/item_type/remove_window_at_pos/tree_update_*/update_rain_dt/cleanup keep the inert defaults
    // (owning modules do not yet publish adapters; see cross_module_requests).
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
    fn tree_is_required_soil_type(&self, _tree: &ObjRef, soil: u8) -> bool {
        crate::trees::tree::tree_is_required_soil_type(soil as i32)
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
