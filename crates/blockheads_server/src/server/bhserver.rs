//! `BHServer`: struct, init, lifecycle, transport glue, kick / ban, trait implementations.
//!
//! ## Seams
//! `World` and `DynamicWorld` are concrete types of lower layers whose server-facing entry points are owned by
//! other modules. `BHServer` reaches them through the traits [`ServerWorld`] / [`ServerDynamicWorld`] declared
//! here (method names are the ObjC selectors in snake case,); the integrator implements them
//! for `World` / `DynamicWorld` by forwarding.

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::rc::{Rc, Weak};

use crate::foundation::clock::reference_time;
use crate::foundation::exception::unrecognized_selector;
use crate::foundation::gsdict::GsDict;
use crate::foundation::nsstring;
use crate::foundation::paths;
use crate::net::bhmatch::{
    BhMatch, BhNetNode, MatchDelegate, NetNodeType, NodeDelegate, NodeVirtuals, PlayerConnectionState, PlayerInfo,
    SendDataMode,
};
use crate::objects::base::ServerApi;
use crate::plist::helpers::{data_from_property_list, property_list_from_data_with_options};
use crate::plist::value::{ClassTag, Origin, PlistDict, PlistValue};
use crate::protocol::messages::{self, ClientMacroBlockRequest};
use crate::rng::glibc_rand::rand_tagged;
use crate::server::server_client::ServerClient;
use crate::storage::lmdb::{Database, DatabaseEnvironment};
use crate::types::structs::IntPair;

/// Opaque binary blob (`NSData`) as queued for the object batches.
pub type NetBlob = Vec<u8>;

// ====================================================================================================
// Delegate and World seams
// ====================================================================================================

/// The `BHServer.delegate` protocol. Implemented by `app::GameController`; `respondsToSelector:` is always YES.
pub trait ServerDelegate: NodeDelegate {
    /// `-chatMessageRecieved:displayNotification:`
    fn chat_message_recieved_display_notification(&self, msg: PlistDict, display_notification: bool);
    /// `-clearChat`
    fn clear_chat(&self);
    /// `-stop` (may drop the last strong reference of the BHServer,)
    fn stop(&self);
}

/// The `World` calls made by `BHServer`. Implemented for `World` by the
/// integrator; `None` for a method result is ObjC `nil`.
pub trait ServerWorld {
    /// `-loadComplete`
    fn load_complete(&self) -> bool;
    /// `-worldName`
    fn world_name(&self) -> Option<String>;
    /// `-saveID`
    fn save_id(&self) -> Option<String>;
    /// `-worldWidthMacro`
    fn world_width_macro(&self) -> i32;
    /// `[world customRules].customRulesEnabled != 0` (byte 0 of the 64-byte struct)
    fn custom_rules_enabled(&self) -> bool;
    /// `-customRulesDict`
    fn custom_rules_dict(&self) -> Option<PlistValue>;
    /// `-serverPassword`
    fn server_password(&self) -> Option<String>;
    /// `-welcomeMessage`
    fn welcome_message(&self) -> Option<String>;
    /// `-pvpEnabled`
    fn pvp_enabled(&self) -> bool;
    /// `-setPVPEnabled:displayNotifciation:`
    fn set_pvp_enabled_display_notifciation(&self, enabled: bool, display: bool);
    /// `-newServerPasswordSet:`
    fn new_server_password_set(&self, password: Option<&str>);
    /// `[[world serverClients] count]`
    fn server_client_count(&self) -> usize;
    /// `[[world serverClients] objectForKey:id]`
    fn server_client(&self, id: &str) -> Option<Rc<ServerClient>>;
    /// `-dynamicWorld`
    fn dynamic_world(&self) -> Option<Rc<dyn ServerDynamicWorld>>;
    /// `-distanceOrderedFoodTypes` (31 entries; the dispatcher zero-pads)
    fn distance_ordered_food_types(&self) -> Vec<i32>;
    /// `-hasFinishedDatabaseMigrationTo17`
    fn has_finished_database_migration_to17(&self) -> bool;
    /// `-appendDebugLog:`
    fn append_debug_log(&self, out: &mut String);
    /// `-summaryNetDataForAdmin:mod:owner:cloudMode:`
    fn summary_net_data_for_admin_mod_owner_cloud_mode(
        &self,
        admin: bool,
        moderator: bool,
        owner: bool,
        cloud_mode: i32,
    ) -> Vec<u8>;
    /// `-clientConnected:`
    fn client_connected(&self, id: &str) -> bool;
    /// `-clientDisconnected:wasKick:`
    fn client_disconnected_was_kick(&self, id: &str, was_kick: bool);
    /// `-requestForBlock:fromClient:`
    fn request_for_block_from_client(&self, req: ClientMacroBlockRequest, id: &str);
    /// `-remoteBlockRemoved:byClient:`
    fn remote_block_removed_by_client(&self, macro_index: i32, id: &str);
    /// `-remoteCreate:forObjectsOfType:clientID:`
    fn remote_create_for_objects_of_type_client_id(&self, items: Vec<NetBlob>, ty: i32, id: &str);
    /// `-remoteUpdate:forObjectsOfType:fromClient:`
    fn remote_update_for_objects_of_type_from_client(&self, items: Vec<NetBlob>, ty: i32, id: &str);
    /// `-remoteRemove:forObjectsOfType:fromClient:`
    fn remote_remove_for_objects_of_type_from_client(&self, items: Vec<NetBlob>, ty: i32, id: &str);
    /// `-remoteCreationDataUpdate:forObjectsOfType:fromClient:`
    fn remote_creation_data_update_for_objects_of_type_from_client(&self, items: Vec<NetBlob>, ty: i32, id: &str);
    /// `-remoteFillRequest:placedByClient:`
    fn remote_fill_request_placed_by_client(&self, payload: &[u8], id: &str) -> u32;
    /// `-remoteRemoveRequest:fromClient:`
    fn remote_remove_request_from_client(&self, payload: &[u8], id: &str) -> u32;
    /// `-remoteRemoveBackWallRequest:fromClient:`
    fn remote_remove_back_wall_request_from_client(&self, payload: &[u8], id: &str) -> u32;
    /// `-remotePlaceWorkbenchRequest:fromPeer:`
    fn remote_place_workbench_request_from_peer(&self, payload: &[u8], id: &str);
    /// `-remotePlaceInteractionObjectRequest:fromPeer:`
    fn remote_place_interaction_object_request_from_peer(&self, payload: &[u8], id: &str);
    /// `-remotePaintRequest:fromClient:`
    fn remote_paint_request_from_client(&self, payload: &[u8], id: &str);
    /// `-heartbeatDataRecieved:fromPeer:`
    fn heartbeat_data_recieved_from_peer(&self, payload: &[u8], id: &str);
    /// `-remoteGatherRequest:`
    fn remote_gather_request(&self, payload: &[u8]);
    /// `-remoteProjectileRequest:`
    fn remote_projectile_request(&self, payload: &[u8]);
    /// `-remoteTipNotification:`
    fn remote_tip_notification(&self, payload: &[u8]);
    /// `-remoteBlockheadDamageRequest:requestedByClientName:`
    fn remote_blockhead_damage_request_requested_by_client_name(&self, payload: &[u8], name: Option<&str>);
    /// `-electricityPathDataRecieved:fromClient:`
    fn electricity_path_data_recieved_from_client(&self, payload: &[u8], id: &str);
    /// `-getBlockheadZoomPosForClient:requesterClient:cycleIndex:`
    fn get_blockhead_zoom_pos_for_client_requester_client_cycle_index(
        &self,
        player: Option<&str>,
        requester: Option<&str>,
        cycle_index: i32,
    ) -> IntPair;
    /// `-transactionDataRecieved:fromClient:`
    fn transaction_data_recieved_from_client(&self, dict: Option<&PlistValue>, id: &str);
    /// `-welcomeMessageDictRecieved:fromClient:`
    fn welcome_message_dict_recieved_from_client(&self, dict: Option<&PlistValue>, id: &str);
    /// `-reportUserWithName:reporterName:reporterMessage:reportedAvatarImagePath:`
    fn report_user_with_name_reporter_name_reporter_message_reported_avatar_image_path(
        &self,
        name: Option<&str>,
        reporter_name: Option<&str>,
        reporter_message: Option<&str>,
        avatar_path: Option<&str>,
    );
    /// `-verifyClientCustomRulesData:`
    fn verify_client_custom_rules_data(&self, payload: &[u8]) -> bool;
    /// `-customRuleDictRecievedFromNet:`
    fn custom_rule_dict_recieved_from_net(&self, dict: &PlistValue);
    /// `-archiveLightBlocksForClient:`
    fn archive_light_blocks_for_client(&self, id: &str);
    /// `-doRepairForTileAtPos:`
    fn do_repair_for_tile_at_pos(&self, pos: IntPair);
    /// `worldPosAtWorldIndex(index, world)`
    fn world_pos_at_world_index(&self, index: i32) -> IntPair;
}

/// The `DynamicWorld` calls made by `BHServer`.
pub trait ServerDynamicWorld {
    /// `-getNextDynamicObjectID`
    fn get_next_dynamic_object_id(&self) -> u64;
    /// `-clientPickupRequest:count:clientID:blockheadRequesterUniqueID:`
    fn client_pickup_request_count_client_id_blockhead_requester_unique_id(
        &self,
        ids: &[u64],
        client_id: &str,
        requester: u64,
    );
    /// `-createClientFreeblocksWithData:`
    fn create_client_freeblocks_with_data(&self, payload: &[u8]);
    /// `-loadClientBlockheadsDataForPlayerID:`
    fn load_client_blockheads_data_for_player_id(&self, id: &str) -> Option<Vec<u8>>;
    /// `-clientBlockheadsRecievedForPlayerID:data:`
    fn client_blockheads_recieved_for_player_id_data(&self, id: &str, payload: &[u8]);
    /// `-clientBlockheadInventoryRecievedForPlayerID:blockheadID:data:`
    fn client_blockhead_inventory_recieved_for_player_id_blockhead_id_data(
        &self,
        id: &str,
        blockhead_id: u64,
        data: &[u8],
    );
    /// `-sendPaintingDataForPaintingWithID:toClient:`
    fn send_painting_data_for_painting_with_id_to_client(&self, painting_id: u64, id: &str);
    /// `-newFoundListRecievedFromClient:list:`
    fn new_found_list_recieved_from_client_list(&self, id: &str, list: &crate::foundation::indexset::IndexSet);
    /// `-clientBlockheadWithID:fromClient:requestsDyamicObjectRemovalWithID:`
    fn client_blockhead_with_id_from_client_requests_dyamic_object_removal_with_id(
        &self,
        blockhead_id: u64,
        id: &str,
        object_id: u64,
    );
    /// `-userBanChanged:isBanned:`
    fn user_ban_changed_is_banned(&self, id: &str, banned: bool);
}

/// A delayed callback: seconds and the call.
type DelayedCall = (f64, Box<dyn FnOnce()>);

/// Scheduler used for `performSelector:withObject:afterDelay:`. Default: `app::run_loop::perform_after_delay`.
thread_local! {
    /// Delayed callbacks queued while no scheduler is installed; drained by the run loop.
    static PENDING_DELAYS: RefCell<Vec<DelayedCall>> = RefCell::new(Vec::new());
}

/// Drain callbacks queued by `performSelector:afterDelay:` when no scheduler was installed.
pub fn take_pending_delays() -> Vec<DelayedCall> {
    PENDING_DELAYS.with(|q| std::mem::take(&mut *q.borrow_mut()))
}

pub type DelayScheduler = Rc<dyn Fn(f64, Box<dyn FnOnce()>)>;

// ====================================================================================================
// IntoPeers
// ====================================================================================================

/// Anything the callers pass as `toPeers:`; `None` == nil == all connected clients.
pub trait IntoPeers {
    /// Convert to the peer list (`None` == nil).
    fn into_peers(self) -> Option<Vec<String>>;
}
impl IntoPeers for Option<&[String]> {
    fn into_peers(self) -> Option<Vec<String>> {
        self.map(|s| s.to_vec())
    }
}
impl IntoPeers for &[String] {
    fn into_peers(self) -> Option<Vec<String>> {
        Some(self.to_vec())
    }
}
impl IntoPeers for &Vec<String> {
    fn into_peers(self) -> Option<Vec<String>> {
        Some(self.clone())
    }
}
impl IntoPeers for Vec<String> {
    fn into_peers(self) -> Option<Vec<String>> {
        Some(self)
    }
}
impl IntoPeers for Option<&Vec<String>> {
    fn into_peers(self) -> Option<Vec<String>> {
        self.cloned()
    }
}
impl IntoPeers for Option<Vec<String>> {
    fn into_peers(self) -> Option<Vec<String>> {
        self
    }
}
impl IntoPeers for &str {
    fn into_peers(self) -> Option<Vec<String>> {
        Some(vec![self.to_string()])
    }
}
impl IntoPeers for &String {
    fn into_peers(self) -> Option<Vec<String>> {
        Some(vec![self.clone()])
    }
}
impl<const N: usize> IntoPeers for &[String; N] {
    fn into_peers(self) -> Option<Vec<String>> {
        Some(self.to_vec())
    }
}
impl<const N: usize> IntoPeers for [String; N] {
    fn into_peers(self) -> Option<Vec<String>> {
        Some(self.to_vec())
    }
}

// ====================================================================================================
// small helpers
// ====================================================================================================

/// `[dict objectForKey:key]` read as an `NSString` (non-strings are nil for the string compares).
pub(crate) fn dict_str(d: &PlistDict, key: &str) -> Option<String> {
    d.map.get(&key.to_string()).and_then(|v| v.as_str()).map(|s| s.to_string())
}

/// `[info objectForKey:key]`.
pub(crate) fn dict_get<'a>(d: &'a PlistDict, key: &str) -> Option<&'a PlistValue> {
    d.map.get(&key.to_string())
}

/// New empty `NSMutableDictionary`.
pub(crate) fn new_mutable_dict() -> PlistDict {
    PlistDict { map: GsDict::new(), tag: ClassTag { origin: Origin::Built, mutable: true, alt: false } }
}

/// `[NSMutableDictionary dictionaryWithDictionary:d]`.
pub(crate) fn mutable_copy(d: &PlistDict) -> PlistDict {
    PlistDict { map: GsDict::copy_from(&d.map), tag: ClassTag { origin: Origin::Built, mutable: true, alt: false } }
}

/// The `PlistDict` inside a `PlistValue::Dict`.
pub(crate) fn pv_into_dict(v: PlistValue) -> Option<PlistDict> {
    match v {
        PlistValue::Dict(d) => Some(d),
        _ => None,
    }
}

/// Write `data` to `path` like `-[NSData writeToFile:atomically:YES]` (temp file in the same directory + rename).
pub(crate) fn write_data_atomic(path: &str, data: &[u8]) -> bool {
    let tmp = format!("{}.tmp{}", path, std::process::id());
    if std::fs::write(&tmp, data).is_err() {
        let _ = std::fs::remove_file(&tmp);
        return false;
    }
    if std::fs::rename(&tmp, path).is_err() {
        let _ = std::fs::remove_file(&tmp);
        return false;
    }
    true
}

// ====================================================================================================
// BHServer
// ====================================================================================================

/// Class `BHServer: BHNetNode`. Every ivar of has its own interior-mutable field.
pub struct BHServer {
    /// Base class `BHNetNode` (ivars 0x00-0x37).
    pub node: BhNetNode,
    pub(crate) weak_self: Weak<BHServer>,
    pub(crate) delegate: Option<Weak<dyn ServerDelegate>>,
    /// 0x38 `world` (assign)
    pub(crate) world: RefCell<Option<Weak<dyn ServerWorld>>>,
    /// 0x40 `saveID`
    pub(crate) save_id: Option<String>,
    /// 0x48
    pub(crate) unapproved_clients: RefCell<Vec<String>>,
    /// 0x50 (approval order)
    pub(crate) connected_clients: RefCell<Vec<String>>,
    /// 0x58 (max 20)
    pub(crate) chat_history: RefCell<Vec<PlistDict>>,
    /// 0x60 (max 128, most recent first)
    pub(crate) recent_players: RefCell<Vec<PlistDict>>,
    /// 0x68 `blackList`; `None` == nil (distinct from `Some(vec![""])`)
    pub(crate) black_list: RefCell<Option<Vec<String>>>,
    /// 0x70
    pub(crate) curse_list: RefCell<Option<Vec<String>>>,
    /// 0x78
    pub(crate) white_list: RefCell<Option<Vec<String>>>,
    /// 0x80
    pub(crate) admin_list: RefCell<Option<Vec<String>>>,
    /// 0x88
    pub(crate) cloud_wide_admin_list: RefCell<Option<Vec<String>>>,
    /// 0x90
    pub(crate) cloud_wide_invisible_admin_list: RefCell<Option<Vec<String>>>,
    /// 0x98
    pub(crate) mod_list: RefCell<Option<Vec<String>>>,
    /// 0xa0
    pub(crate) reset_list: RefCell<Option<Vec<String>>>,
    /// 0xa8 (membership only)
    pub(crate) repair_set: RefCell<HashSet<String>>,
    /// 0xb0
    pub(crate) max_players: Cell<i32>,
    /// 0xc0 (membership only)
    pub(crate) cached_player_banned: RefCell<Option<HashMap<String, bool>>>,
    /// 0xc8
    pub(crate) needs_to_send_updated_list_to_clients: Cell<bool>,
    /// 0xd8
    pub(crate) clients_connected_with_photos_unsent: RefCell<Vec<String>>,
    /// 0xe0 (lazily created by 0x3e)
    pub(crate) trade_portal_transactions: RefCell<Option<HashMap<String, u32>>>,
    /// 0xe8
    pub(crate) player_update: RefCell<Option<String>>,
    /// 0xf0
    pub(crate) player_update_timer: Cell<f32>,
    /// 0xf8
    pub(crate) last_player_update: Cell<f64>,
    /// 0x100
    pub(crate) rules_changed_time: Cell<f64>,
    /// 0x108
    pub(crate) server_database_environment: Option<Rc<DatabaseEnvironment>>,
    /// 0x110 (named DB "main")
    pub(crate) server_database: Option<Rc<Database>>,
    /// `performSelector:afterDelay:` backend (None == the run loop of `app::run_loop`).
    pub(crate) scheduler: RefCell<Option<DelayScheduler>>,
}

impl BHServer {
    /// `-initWithDelegate:match:netNodeType:saveID:maxPlayers:`. `None` == nil (cannot happen).
    /// Steps 1-12 of in the original order (log lines and the `rand()` draw are observable).
    pub fn init_with_delegate_match_net_node_type_save_id_max_players(
        delegate: Weak<dyn ServerDelegate>,
        m: Rc<dyn BhMatch>,
        net_node_type: NetNodeType,
        save_id: Option<&str>,
        max_players: i32,
    ) -> Option<Rc<BHServer>> {
        // 1. super init
        let node_delegate: Weak<dyn NodeDelegate> = delegate.clone();
        let node = BhNetNode::init_with_delegate_match_net_node_type(Some(node_delegate), m.clone(), net_node_type);
        // 2.-3. saveID, repairSet, server_db
        let docs = paths::appsup();
        let sid_text = save_id.unwrap_or("(null)");
        let env = DatabaseEnvironment::new(&format!("{}/saves/{}/server_db/", docs, sid_text), 1, 0x80);
        let db = env.as_ref().and_then(|e| Database::new(e, "main")).map(Rc::new);
        // 4. RNG draw: f32 operations in this order
        let timer = ((rand_tagged("BHServer:init:playerUpdateTimer") as f32) / 2147483648.0f32) * 20.0f32 + 10.0f32;
        let last = reference_time();
        let srv = Rc::new_cyclic(|w| BHServer {
            node,
            weak_self: w.clone(),
            delegate: Some(delegate),
            world: RefCell::new(None),
            save_id: save_id.map(|s| s.to_string()),
            unapproved_clients: RefCell::new(Vec::with_capacity(8)),
            connected_clients: RefCell::new(Vec::with_capacity(8)),
            chat_history: RefCell::new(Vec::new()),
            recent_players: RefCell::new(Vec::new()),
            black_list: RefCell::new(None),
            curse_list: RefCell::new(None),
            white_list: RefCell::new(None),
            admin_list: RefCell::new(None),
            cloud_wide_admin_list: RefCell::new(None),
            cloud_wide_invisible_admin_list: RefCell::new(None),
            mod_list: RefCell::new(None),
            reset_list: RefCell::new(None),
            repair_set: RefCell::new(HashSet::new()),
            max_players: Cell::new(0),
            cached_player_banned: RefCell::new(None),
            needs_to_send_updated_list_to_clients: Cell::new(false),
            clients_connected_with_photos_unsent: RefCell::new(Vec::with_capacity(0x20)),
            trade_portal_transactions: RefCell::new(None),
            player_update: RefCell::new(None),
            player_update_timer: Cell::new(timer),
            last_player_update: Cell::new(last),
            rules_changed_time: Cell::new(0.0),
            server_database_environment: env,
            server_database: db,
            scheduler: RefCell::new(None),
        });
        // 6. reloadLists (max_players still 0, no clients)
        srv.reload_lists();
        // 7.
        srv.max_players.set(max_players);
        // 8.-9. legacy migrations
        srv.migrate_players_list_1_4_to_1_5();
        srv.migrate_players_list_1_5_to_1_5_1();
        // 10. recentPlayers
        srv.load_recent_players();
        // 11. resetList.plist
        let reset_path = format!("{}/saves/{}/resetList.plist", docs, sid_text);
        if let Ok(bytes) = std::fs::read(&reset_path) {
            if let Some(PlistValue::Array(a)) = property_list_from_data_with_options(&bytes, 2) {
                let list: Vec<String> = a.items.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect();
                *srv.reset_list.borrow_mut() = Some(list);
            }
        }
        // 12. match.setDelegate(self)
        let weak: Weak<BHServer> = Rc::downgrade(&srv);
        let md: Weak<dyn MatchDelegate> = weak;
        m.set_delegate(Some(md));
        // 13. cloud log lines
        if srv.is_cloud_match() {
            let p =
                srv.privacy_string().map(|s| nsstring::uppercase_string(&s)).unwrap_or_else(|| "PUBLIC".to_string());
            crate::nslog!("CLOUD:SETTING:PRIVACY:{}", p);
            crate::nslog!("CLOUD:STAT:PLAYERS:0");
        }
        Some(srv)
    }

    fn save_dir_path(&self) -> String {
        format!("{}/saves/{}", paths::appsup(), self.save_id.as_deref().unwrap_or("(null)"))
    }

    /// Legacy migration A (1.4 -> 1.5.0).
    fn migrate_players_list_1_4_to_1_5(&self) {
        let docs = paths::appsup();
        let sid = self.save_id.as_deref().unwrap_or("(null)");
        let old = format!("{}/allPlayers-{}", docs, sid);
        if !std::path::Path::new(&old).exists() {
            return;
        }
        crate::nslog!("Upgrading players list from 1.4 format to 1.5.0...");
        let bytes = std::fs::read(&old).ok();
        let parsed = bytes.and_then(|b| property_list_from_data_with_options(&b, 2));
        if let Some(PlistValue::Array(mut a)) = parsed {
            for item in a.items.iter_mut() {
                if let PlistValue::Dict(d) = item {
                    for k in ["connected", "micOrSpeakerOn", "voiceConnected", "photo"] {
                        d.map.remove(&k.to_string());
                    }
                }
            }
            for data in data_from_property_list(&PlistValue::Array(a)) {
                let target = format!("{}/saves/{}/allPlayers.plist", docs, sid);
                if write_data_atomic(&target, &data) {
                    let _ = std::fs::remove_file(&old);
                    crate::nslog!("Player list upgrade complete.");
                }
            }
        }
    }

    /// Legacy migration B (1.5 -> 1.5.1).
    fn migrate_players_list_1_5_to_1_5_1(&self) {
        let docs = paths::appsup();
        let sid = self.save_id.as_deref().unwrap_or("(null)");
        let save = format!("{}/saves/{}", docs, sid);
        let src = format!("{}/allPlayers.plist", save);
        if !std::path::Path::new(&src).exists() {
            return;
        }
        crate::nslog!("Upgrading players list from 1.5 format to 1.5.1...");
        let players_dir = format!("{}/players/", save);
        let _ = std::fs::create_dir_all(&players_dir);
        for rd in std::fs::read_dir(&save) {
            let mut names: Vec<String> =
                rd.filter_map(|e| e.ok()).filter_map(|e| e.file_name().into_string().ok()).collect();
            names.sort();
            for name in names {
                if nsstring::length(&name) == 0x20 {
                    let c0 = nsstring::substring_with_range(&name, 0, 1);
                    let c1 = nsstring::substring_with_range(&name, 1, 1);
                    let rest = nsstring::substring_with_range(&name, 2, 30);
                    let dir = format!("{}/players/{}/{}/", save, c0, c1);
                    let _ = std::fs::create_dir_all(&dir);
                    let _ = std::fs::rename(format!("{}/{}", save, name), format!("{}{}", dir, rest));
                }
            }
        }
        let parsed = std::fs::read(&src).ok().and_then(|b| property_list_from_data_with_options(&b, 0));
        let mut recent: Vec<PlistValue> = Vec::new();
        if let Some(PlistValue::Array(a)) = parsed {
            for (i, item) in a.items.iter().enumerate() {
                if let PlistValue::Dict(d) = item {
                    let pid = dict_str(d, "playerID").unwrap_or_default();
                    let len = nsstring::length(&pid);
                    let a0 = nsstring::substring_with_range(&pid, 0, 1.min(len));
                    let a1 = if len >= 2 { nsstring::substring_with_range(&pid, 1, 1) } else { String::new() };
                    let a2 = if len >= 2 { nsstring::substring_from_index(&pid, 2) } else { String::new() };
                    let player_dir = format!("{}/players/{}/{}/{}/", save, a0, a1, a2);
                    if !std::path::Path::new(&player_dir).exists() {
                        let _ = std::fs::create_dir_all(&player_dir);
                    }
                    let info_path = format!("{}playerInfo.plist", player_dir);
                    if !std::path::Path::new(&info_path).exists() {
                        for data in data_from_property_list(item) {
                            write_data_atomic(&info_path, &data);
                        }
                    }
                    if i < 0x80 {
                        recent.push(item.clone());
                    }
                }
            }
        }
        for data in data_from_property_list(&PlistValue::array(recent)) {
            let target = format!("{}/recentPlayers.plist", save);
            if write_data_atomic(&target, &data) {
                let _ = std::fs::remove_file(&src);
                crate::nslog!("Player list upgrade complete.");
            }
        }
    }

    /// Step 9 of init: `recentPlayers` from `server_db`, else `SAVE/recentPlayers.plist`.
    fn load_recent_players(&self) {
        let mut data = self.server_database.as_ref().and_then(|d| d.data_for_key(Some("recentPlayers")));
        if data.is_none() {
            let p = format!("{}/recentPlayers.plist", self.save_dir_path());
            if let Ok(b) = std::fs::read(&p) {
                data = Some(b);
            }
        }
        let list: Vec<PlistDict> = match data.and_then(|d| property_list_from_data_with_options(&d, 2)) {
            Some(PlistValue::Array(a)) => a.items.into_iter().filter_map(pv_into_dict).collect(),
            _ => Vec::new(),
        };
        *self.recent_players.borrow_mut() = list;
    }

    // ---- accessors for the pieces other files need ----

    /// Strong self reference (GNUstep retains the target of `performSelector:`).
    pub(crate) fn rc(&self) -> Rc<BHServer> {
        self.weak_self.upgrade().expect("BHServer is alive while a method runs")
    }

    /// `world` as a strong reference; `None` == nil.
    pub(crate) fn world(&self) -> Option<Rc<dyn ServerWorld>> {
        self.world.borrow().as_ref().and_then(|w| w.upgrade())
    }

    /// `delegate` as a strong reference.
    pub(crate) fn delegate(&self) -> Option<Rc<dyn ServerDelegate>> {
        self.delegate.as_ref().and_then(|w| w.upgrade())
    }

    /// `[delegate playersChanged]` (respondsToSelector is always YES).
    pub(crate) fn players_changed(&self) {
        if let Some(d) = self.delegate() {
            d.players_changed();
        }
    }

    /// Install a custom `performSelector:afterDelay:` backend (tests); `None` restores the run loop.
    pub fn set_delay_scheduler(&self, s: Option<DelayScheduler>) {
        *self.scheduler.borrow_mut() = s;
    }

    /// `performSelector:withObject:afterDelay:`. Never cancelled.
    pub(crate) fn perform_after_delay(&self, delay: f64, cb: Box<dyn FnOnce()>) {
        let s = self.scheduler.borrow().clone();
        match s {
            Some(s) => s(delay, cb),
            None => {
                // Note: app::run_loop is not yet implemented (app runtime); queue until the run loop drains it.
                PENDING_DELAYS.with(|q| q.borrow_mut().push((delay, cb)));
            }
        }
    }

    /// `-setWorld:`.
    pub fn set_world(&self, world: &Rc<dyn ServerWorld>) {
        *self.world.borrow_mut() = Some(Rc::downgrade(world));
        let ids = self.connected_clients.borrow().clone();
        for id in ids {
            let alias = self.player_alias_for_peer_id(Some(&id));
            let cloud_mode = self.cloud_mode_for_alias(alias.as_deref());
            let admin = self.player_is_admin_with_alias(alias.as_deref());
            let moderator = self.player_is_mod_with_alias(alias.as_deref());
            let owner = self.player_is_owner_with_alias(alias.as_deref());
            let summary = world.summary_net_data_for_admin_mod_owner_cloud_mode(admin, moderator, owner, cloud_mode);
            self.send_network_data_to_peers_reliable(summary, std::slice::from_ref(&id), true);
            world.client_connected(&id);
        }
        self.players_changed();
    }

    /// `cloudMode`: 0, 0xa8f4 (cloud-wide admin) or 0x4d83 (invisible).
    pub(crate) fn cloud_mode_for_alias(&self, alias: Option<&str>) -> i32 {
        let mut cloud_mode = 0;
        if self.player_is_cloud_wide_admin_with_alias(alias) {
            cloud_mode = 43252;
            if self.player_is_cloud_wide_invisible_admin_with_alias(alias) {
                cloud_mode = 19843;
            }
        }
        cloud_mode
    }

    /// `-cleanup`.
    pub fn cleanup(&self) {
        let ids = self.connected_clients.borrow().clone();
        for id in ids {
            self.client_disconnected_was_kick(&id, false);
        }
        self.connected_clients.borrow_mut().clear();
        self.unapproved_clients.borrow_mut().clear();
        self.players_changed();
        self.node.cleanup();
    }

    // ---- transport glue ----

    /// `-sendNetworkData:toPeers:reliable:`. `peers` None == nil == all connected clients.
    pub fn send_network_data_to_peers_reliable<D: AsRef<[u8]>, P: IntoPeers>(&self, data: D, peers: P, reliable: bool) {
        self.send_to_peers(data.as_ref(), peers.into_peers(), reliable);
    }

    /// The body of [`BHServer::send_network_data_to_peers_reliable`] (not generic, so its lines count once).
    fn send_to_peers(&self, data: &[u8], peers: Option<Vec<String>>, reliable: bool) {
        let peers = peers.unwrap_or_else(|| self.connected_clients.borrow().clone());
        let m = self.node.match_();
        if let (Some(m), false) = (m, peers.is_empty()) {
            let mode = if reliable { SendDataMode::Reliable } else { SendDataMode::Unreliable };
            m.send_data_to_players_with_data_mode_error(Some(data), &peers, mode);
        }
    }

    /// `-sendChatMessage:sendToClients:` (BHNetNode 2-argument form). Nil text is a no-op.
    pub fn send_chat_message_send_to_clients<'a>(&self, text: impl Into<Option<&'a str>>, clients: Option<&[String]>) {
        if let Some(t) = text.into() {
            self.node.send_chat_message_send_to_clients(t, clients, self);
        }
    }

    /// `-sendChatMessage:displayNotification:sendToClients:`.
    pub fn send_chat_message_display_notification_send_to_clients(
        &self,
        text: Option<&str>,
        display_notification: bool,
        clients: Option<&[String]>,
    ) {
        let Some(text) = text else { return };
        let player_id = self.node.local_player_id();
        let alias = self
            .node
            .player_info_for_peer_id(&player_id)
            .and_then(|i| dict_str(&i.borrow(), "alias"))
            .unwrap_or_else(|| "SERVER".to_string());
        let send = messages::server_chat_dict(text, &alias, &player_id, reference_time());
        let packet = messages::chat_message(&send);
        let send_dict = pv_into_dict(send).unwrap_or_else(new_mutable_dict);
        if clients.is_none() {
            let mut h = self.chat_history.borrow_mut();
            h.push(send_dict.clone());
            if h.len() > 20 {
                h.remove(0);
            }
        }
        self.send_network_data_to_peers_reliable(packet, clients, true);
        if let Some(d) = self.delegate() {
            d.chat_message_recieved_display_notification(mutable_copy(&send_dict), display_notification);
        }
    }

    // ---- players / lifecycle ----

    /// `-full`.
    pub fn full(&self) -> bool {
        let a = self.world().map_or(0, |w| w.server_client_count());
        let b = self.connected_clients.borrow().len();
        let n = a.max(b) as i32;
        n >= self.max_players.get() || n >= 32
    }

    /// `-clientDisconnected:wasKick:`.
    pub fn client_disconnected_was_kick(&self, player_id: &str, was_kick: bool) {
        if let Some(w) = self.world() {
            w.client_disconnected_was_kick(player_id, was_kick);
        }
        self.repair_set.borrow_mut().remove(player_id);
    }

    /// `-bootPlayer:wasBan:`: 2-byte packet, delayed disconnect after 2.0 s.
    pub fn boot_player_was_ban(&self, player_id: &str, was_ban: bool) {
        let reason =
            if was_ban { crate::protocol::codec::reject::BANNED } else { crate::protocol::codec::reject::KICKED };
        self.send_network_data_to_peers_reliable(messages::rejection(reason), &[player_id.to_string()], true);
        let srv = self.rc();
        let id = player_id.to_string();
        self.perform_after_delay(2.0, Box::new(move || srv.delayed_disconnect_player_due_to_kick_with_id(&id)));
    }

    /// `-bootPlayerNamed:wasBan:`.
    pub fn boot_player_named_was_ban(&self, name: &str, was_ban: bool) {
        let mut found: Option<String> = None;
        let ids = self.connected_clients.borrow().clone();
        for pid in ids {
            let alias = self.player_alias_for_peer_id(Some(&pid));
            if nsstring::safe_case_insensitive_compare(Some(name), alias.as_deref()) == 0 {
                found = Some(pid);
                break;
            }
        }
        if found.is_none() {
            let recent = self.recent_players.borrow().clone();
            let mut i = 0usize;
            for d in recent.iter() {
                if nsstring::safe_case_insensitive_compare(Some(name), dict_str(d, "alias").as_deref()) == 0 {
                    found = dict_str(d, "playerID");
                    break;
                }
                i += 1;
                if i > 0x40 {
                    break;
                }
            }
        }
        match found {
            None => crate::nslog!("No player found named {}", name),
            Some(id) => {
                crate::nslog!("Kicking player:{}", name);
                self.boot_player_was_ban(&id, was_ban);
            }
        }
    }

    /// `-delayedDisconnectPlayerDueToKickWithID:`.
    pub fn delayed_disconnect_player_due_to_kick_with_id(&self, player_id: &str) {
        self.delayed_disconnect(player_id, true);
    }

    /// `-delayedDisconnectPlayerDueToNoCreditWithID:`: like the kick variant with `wasKick:NO`.
    pub fn delayed_disconnect_player_due_to_no_credit_with_id(&self, player_id: &str) {
        self.delayed_disconnect(player_id, false);
    }

    fn delayed_disconnect(&self, player_id: &str, was_kick: bool) {
        let present = self
            .connected_clients
            .borrow()
            .iter()
            .any(|e| nsstring::safe_case_insensitive_compare(Some(e), Some(player_id)) == 0);
        if !present {
            return;
        }
        if let Some(h) = crate::mods::hooks() {
            h.player_quit(player_id, was_kick);
        }
        if let Some(m) = self.node.match_() {
            m.disconnect_player(player_id);
        }
        self.client_disconnected_was_kick(player_id, was_kick);
        self.players_changed();
        self.send_updated_player_list_to_clients();
    }

    /// `-clientFinishedAwaySimulation:`: drop the first live info with this id.
    pub fn client_finished_away_simulation(&self, client_id: &str) {
        let live = self.node.live_player_infos();
        for (i, d) in live.iter().enumerate() {
            let pid = dict_str(&d.borrow(), "playerID");
            if pid.as_deref() == Some(client_id) {
                self.node.remove_live_player_info_at(i);
                self.send_updated_player_list_to_clients();
                return;
            }
        }
    }

    /// `-updatePlayer:` (arms the anti-cheat kick).
    pub fn update_player(&self, client_id: &str) {
        *self.player_update.borrow_mut() = Some(client_id.to_string());
    }

    /// `-updatePlayers`: once per received packet.
    pub fn update_players(&self) {
        let pu = self.player_update.borrow().clone();
        let Some(pu) = pu else { return };
        let now = reference_time();
        let mut dt = (now - self.last_player_update.get()) as f32;
        if dt < 0.0 {
            dt = 0.0;
        }
        if dt > 5.0 {
            dt = 5.0;
        }
        self.last_player_update.set(now);
        let mut timer = self.player_update_timer.get() - dt;
        if timer <= 0.0 {
            timer = ((rand_tagged("BHServer:updatePlayers") as f32) / 2147483648.0f32) * 50.0f32 + 10.0f32;
            self.player_update_timer.set(timer);
            self.delayed_disconnect_player_due_to_kick_with_id(&pu);
            *self.player_update.borrow_mut() = None;
        } else {
            self.player_update_timer.set(timer);
        }
    }

    /// `-playerUpdate`.
    pub fn player_update(&self) -> Option<String> {
        self.player_update.borrow().clone()
    }

    /// Snapshot of `connectedClients`.
    pub fn connected_client_ids(&self) -> Vec<String> {
        self.connected_clients.borrow().clone()
    }

    /// `-playerInfoForPeerID:`.
    pub fn player_info_for_peer_id(&self, peer: &str) -> Option<PlayerInfo> {
        self.node.player_info_for_peer_id(peer)
    }

    /// `[[self playerInfoForPeerID:peer] objectForKey:@"alias"]` (nil-safe helper).
    pub fn player_alias_for_peer_id(&self, peer: Option<&str>) -> Option<String> {
        let info = self.node.player_info_for_peer_id(peer?)?;
        let a = dict_str(&info.borrow(), "alias");
        a
    }

    /// Snapshot of the player info as a plain dictionary.
    pub fn player_info_dict_for_peer_id(&self, peer: &str) -> Option<PlistDict> {
        self.node.player_info_for_peer_id(peer).map(|i| i.borrow().clone())
    }

    /// `-localPlayerID`.
    pub fn local_player_id(&self) -> String {
        self.node.local_player_id()
    }

    /// `-localPlayerName`.
    pub fn local_player_name(&self) -> String {
        self.node.local_player_name()
    }

    /// `-notifyPlayersChanged`.
    pub fn notify_players_changed(&self) {
        self.players_changed();
    }

    /// `-sendPlayerChangedNotifcationToDelegate`.
    pub fn send_player_changed_notifcation_to_delegate(&self) {
        self.players_changed();
    }

    /// `-match`.
    pub fn match_(&self) -> Option<Rc<dyn BhMatch>> {
        self.node.match_()
    }

    /// `-serverDatabase`.
    pub fn server_database(&self) -> Option<Rc<Database>> {
        self.server_database.clone()
    }

    /// `-startBulkTransaction` (nil environment: no-op).
    pub fn start_bulk_transaction(&self) {
        if let Some(e) = &self.server_database_environment {
            e.start_bulk_transaction();
        }
    }

    /// `-finishBulkTransaction` (nil environment gives NO).
    pub fn finish_bulk_transaction(&self) -> bool {
        match &self.server_database_environment {
            Some(e) => e.finish_bulk_transaction(),
            None => false,
        }
    }

    /// `-doRepairForTileAtPos:`.
    pub fn do_repair_for_tile_at_pos(&self, pos: IntPair) {
        if let Some(w) = self.world() {
            w.do_repair_for_tile_at_pos(pos);
        }
    }

    /// `-isWaitingForMultiPartCommandResponse`: always NO.
    pub fn is_waiting_for_multi_part_command_response(&self) -> bool {
        false
    }

    /// `-serverPlayerID`.
    pub fn server_player_id(&self) -> String {
        self.local_player_id()
    }

    // ---- MatchDelegate bodies ----

    fn on_player_did_change_state(&self, m: &dyn BhMatch, player_id: &str, state: PlayerConnectionState) {
        self.node.match_player_did_change_state(m, player_id, state);
        match state {
            PlayerConnectionState::Connected => {
                if !self.unapproved_clients.borrow().iter().any(|e| e == player_id) {
                    self.unapproved_clients.borrow_mut().push(player_id.to_string());
                }
                let world = self.world();
                let requires_password = world.as_ref().is_some_and(|w| w.server_password().is_some());
                let world_id = match &self.save_id {
                    Some(s) => s.clone(),
                    None => world.as_ref().and_then(|w| w.save_id()).unwrap_or_default(),
                };
                let packet = messages::initial_contact(m.owner_name().as_deref(), requires_password, &world_id);
                self.send_network_data_to_peers_reliable(packet, &[player_id.to_string()], true);
            }
            PlayerConnectionState::Disconnected => {
                let was_connected = self.connected_clients.borrow().iter().any(|e| e == player_id);
                if let (true, Some(h)) = (was_connected, crate::mods::hooks()) {
                    h.player_quit(player_id, false);
                }
                if was_connected {
                    self.connected_clients.borrow_mut().retain(|e| e != player_id);
                    if let Some(t) = self.trade_portal_transactions.borrow_mut().as_mut() {
                        t.remove(player_id);
                    }
                } else {
                    self.unapproved_clients.borrow_mut().retain(|e| e != player_id);
                    if let Some(h) = crate::mods::hooks() {
                        h.limbo_gone(player_id);
                    }
                }
                self.client_disconnected_was_kick(player_id, false);
                let mut found: Option<(usize, PlistDict)> = None;
                {
                    let rp = self.recent_players.borrow();
                    let mut i = 0usize;
                    for d in rp.iter() {
                        if dict_str(d, "playerID").as_deref() == Some(player_id) {
                            found = Some((i, d.clone()));
                            break;
                        }
                        i += 1;
                        if i > 0x40 {
                            break;
                        }
                    }
                }
                if let Some((idx, mut dict)) = found {
                    let alias = dict_str(&dict, "alias").unwrap_or("(null)".to_string());
                    let wn = self.world().and_then(|w| w.world_name()).unwrap_or("(null)".to_string());
                    crate::nslog!("{} - Player Disconnected {}", wn, alias);
                    dict.map.remove(&"connected".to_string());
                    self.recent_players.borrow_mut().remove(idx);
                    self.add_player_dict_to_all_players_ever(&dict);
                }
                self.players_changed();
                self.send_updated_player_list_to_clients();
            }
            PlayerConnectionState::Unknown => {}
        }
    }
}

impl MatchDelegate for BHServer {
    fn match_player_did_change_state(&self, m: &dyn BhMatch, player_id: &str, state: PlayerConnectionState) {
        self.on_player_did_change_state(m, player_id, state);
    }

    fn match_did_receive_data_from_player(&self, _m: &dyn BhMatch, data: &[u8], player_id: &str) {
        let srv = self.rc();
        crate::server::dispatch::match_did_receive_data_from_player(&srv, data, player_id);
    }

    fn info_arrived_for_player(&self, info: &PlayerInfo, player_id: &str) {
        self.node.info_arrived_for_player(info, player_id);
        self.full_player_information_now_available_for_player(player_id);
        self.send_updated_player_list_to_clients();
    }

    fn match_did_fail_with_error(&self, _m: &dyn BhMatch) {
        self.cleanup();
    }

    fn boot_all_clients_due_to_no_credit(&self) {
        BHServer::boot_all_clients_due_to_no_credit(self);
    }

    fn match_connection_with_player_failed_with_error(&self, _m: &dyn BhMatch, player_id: &str) {
        crate::nslog!("connectionWithPlayerFailed {}", player_id);
        if let Some(h) = crate::mods::hooks() {
            h.limbo_gone(player_id);
        }
        self.unapproved_clients.borrow_mut().retain(|e| e != player_id);
        self.connected_clients.borrow_mut().retain(|e| e != player_id);
        if let Some(t) = self.trade_portal_transactions.borrow_mut().as_mut() {
            t.remove(player_id);
        }
        self.client_disconnected_was_kick(player_id, false);
        self.players_changed();
        self.send_updated_player_list_to_clients();
    }
}

impl NodeVirtuals for BHServer {
    fn cleanup(&self) {
        BHServer::cleanup(self);
    }

    fn send_chat_message_display_notification_send_to_clients(
        &self,
        text: &str,
        display: bool,
        clients: Option<&[String]>,
    ) {
        BHServer::send_chat_message_display_notification_send_to_clients(self, Some(text), display, clients);
    }
}

impl ServerApi for BHServer {
    fn player_is_admin_with_id(&self, client_id: Option<&str>) -> bool {
        BHServer::player_is_admin_with_id(self, client_id)
    }
}

impl crate::dynworld::state::DwServer for BHServer {
    fn player_info_for_peer_id(&self, peer_id: &str) -> Option<PlistValue> {
        self.node.player_info_for_peer_id(peer_id).map(|i| PlistValue::Dict(i.borrow().clone()))
    }
    fn player_is_admin_with_id(&self, client_id: &str) -> bool {
        BHServer::player_is_admin_with_id(self, Some(client_id))
    }
    fn player_update(&self) -> Option<String> {
        BHServer::player_update(self)
    }
    fn notify_players_changed(&self) {
        BHServer::notify_players_changed(self)
    }
}

/// Raise `<Class>(instance) does not recognize <sel>` for a hostile non-string player-info value.
pub(crate) fn raise_unrecognized(v: &PlistValue, selector: &str) -> ! {
    unrecognized_selector(v.class_name(), selector)
}

#[cfg(test)]
pub(crate) mod test_support {
    //! Mock transport, world and delegate used by the unit tests of this module.
    use super::*;
    use crate::net::bhmatch::{BhMatchBase, PlayerInfo};
    use crate::plist::helpers::property_list_from_data;
    use std::sync::Once;

    static INIT: Once = Once::new();

    /// Point `$HOME` at a private temp directory (once per test process).
    pub fn init_env() {
        // Other test modules also repoint $HOME; re-assert ours on every call (tests run single-threaded).
        let d = std::env::temp_dir().join(format!("bh_server_tests_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&d);
        crate::foundation::paths::set_home_for_test(&d);
        INIT.call_once(|| {});
    }

    /// Recording match.
    pub struct MockMatch {
        pub base: BhMatchBase,
        pub sent: RefCell<Vec<(Vec<String>, Vec<u8>)>>,
        pub owner: RefCell<Option<String>>,
        pub cloud: Cell<bool>,
        pub salt: RefCell<Option<String>>,
        pub removed: RefCell<Vec<String>>,
        pub disconnected: RefCell<Vec<String>>,
        pub infos: RefCell<HashMap<String, PlayerInfo>>,
        pub persistent: RefCell<HashMap<String, String>>,
        pub approved: RefCell<Vec<PlayerInfo>>,
        /// Whether the transport authenticates players (the real one does).
        pub authenticates: Cell<bool>,
        /// When set, approving a client is not announced to the delegate and unknown players have no stored info.
        pub quiet_approval: Cell<bool>,
    }

    impl MockMatch {
        pub fn new() -> Rc<MockMatch> {
            Rc::new(MockMatch {
                base: BhMatchBase::default(),
                sent: RefCell::new(Vec::new()),
                owner: RefCell::new(None),
                cloud: Cell::new(false),
                salt: RefCell::new(None),
                removed: RefCell::new(Vec::new()),
                disconnected: RefCell::new(Vec::new()),
                infos: RefCell::new(HashMap::new()),
                persistent: RefCell::new(HashMap::new()),
                approved: RefCell::new(Vec::new()),
                authenticates: Cell::new(true),
                quiet_approval: Cell::new(false),
            })
        }
        pub fn packets_to(&self, id: &str) -> Vec<Vec<u8>> {
            self.sent.borrow().iter().filter(|(p, _)| p.iter().any(|x| x == id)).map(|(_, d)| d.clone()).collect()
        }
        pub fn clear(&self) {
            self.sent.borrow_mut().clear();
        }
    }

    impl BhMatch for MockMatch {
        fn set_delegate(&self, d: Option<Weak<dyn MatchDelegate>>) {
            *self.base.delegate.borrow_mut() = d;
        }
        fn local_player_id(&self) -> String {
            crate::foundation::md5::string_from_md5("server")
        }
        fn send_data_to_players_with_data_mode_error(
            &self,
            data: Option<&[u8]>,
            players: &[String],
            _mode: SendDataMode,
        ) -> bool {
            self.sent.borrow_mut().push((players.to_vec(), data.unwrap_or(&[]).to_vec()));
            true
        }
        fn requires_player_authentication(&self) -> bool {
            self.authenticates.get()
        }
        fn is_cloud_match(&self) -> bool {
            self.cloud.get()
        }
        fn owner_name(&self) -> Option<String> {
            self.owner.borrow().clone()
        }
        fn cloud_salt(&self) -> Option<String> {
            self.salt.borrow().clone()
        }
        fn remove_client(&self, id: &str) {
            self.removed.borrow_mut().push(id.to_string());
        }
        fn disconnect_player(&self, id: &str) {
            self.disconnected.borrow_mut().push(id.to_string());
        }
        fn persistent_id_for_net_id(&self, net_id: &str) -> Option<String> {
            self.persistent.borrow().get(net_id).cloned().or_else(|| Some(net_id.to_string()))
        }
        fn client_player_information_recieved_from_peer(
            &self,
            client: Option<&PlistDict>,
            enet: &str,
        ) -> Option<PlayerInfo> {
            let c = client?;
            let alias = dict_str(c, "alias")?;
            if alias.eq_ignore_ascii_case("server") {
                return None;
            }
            let pid = self.persistent.borrow().get(enet).cloned().unwrap_or(enet.to_string());
            let mut d = mutable_copy(c);
            d.insert("playerID".to_string(), PlistValue::string(pid));
            d.insert("local".to_string(), PlistValue::boolean(false));
            d.insert("ip".to_string(), PlistValue::string("127.0.0.1"));
            let info = Rc::new(RefCell::new(d));
            Some(info)
        }
        fn client_approved_with_info(&self, info: &PlayerInfo) {
            self.approved.borrow_mut().push(info.clone());
            // transport: delegate.info_arrived_for_player(info, P)
            let pid = dict_str(&info.borrow(), "playerID").unwrap_or_default();
            let d = self.base.delegate.borrow().as_ref().and_then(|w| w.upgrade());
            for d in d.filter(|_| !self.quiet_approval.get()) {
                d.info_arrived_for_player(info, &pid);
            }
        }
        fn load_info_for_player(&self, player_id: &str) -> Option<PlayerInfo> {
            let known = self.infos.borrow().get(player_id).cloned();
            known.or_else(|| {
                Some(crate::net::bhmatch::bhmatch_load_info_for_player(player_id))
                    .filter(|_| !self.quiet_approval.get())
            })
        }
    }

    /// Recording delegate.
    pub struct MockDelegate {
        pub changed: Cell<u32>,
        pub chats: RefCell<Vec<(PlistDict, bool)>>,
        pub cleared: Cell<u32>,
        pub stopped: Cell<u32>,
    }
    impl MockDelegate {
        pub fn new() -> Rc<MockDelegate> {
            Rc::new(MockDelegate {
                changed: Cell::new(0),
                chats: RefCell::new(Vec::new()),
                cleared: Cell::new(0),
                stopped: Cell::new(0),
            })
        }
        pub fn messages(&self) -> Vec<String> {
            self.chats.borrow().iter().filter_map(|(d, _)| dict_str(d, "message")).collect()
        }
    }
    impl NodeDelegate for MockDelegate {
        fn players_changed(&self) {
            self.changed.set(self.changed.get() + 1);
        }
    }
    impl ServerDelegate for MockDelegate {
        fn chat_message_recieved_display_notification(&self, msg: PlistDict, display: bool) {
            self.chats.borrow_mut().push((msg, display));
        }
        fn clear_chat(&self) {
            self.cleared.set(self.cleared.get() + 1);
        }
        fn stop(&self) {
            self.stopped.set(self.stopped.get() + 1);
        }
    }

    /// Recording world.
    pub struct MockWorld {
        pub loaded: Cell<bool>,
        pub name: String,
        pub id: String,
        pub width: Cell<i32>,
        pub rules: Cell<bool>,
        pub password: RefCell<Option<String>>,
        pub pvp: Cell<bool>,
        pub calls: RefCell<Vec<String>>,
        pub clients: RefCell<HashMap<String, Rc<ServerClient>>>,
        pub accept_clients: Cell<bool>,
        /// Answer of the custom-rules verification.
        pub rules_ok: Cell<bool>,
        /// Whether the 1.7 database migration is done (legacy player files are only read before).
        pub migrated: Cell<bool>,
        /// Whether the world has a dynamic world yet (it does not before load stage 12).
        pub dw_present: Cell<bool>,
        /// What the summary method returns instead of the fixed bytes.
        pub summary: RefCell<Option<Vec<u8>>>,
        pub dw: Rc<MockDw>,
    }
    impl MockWorld {
        pub fn new(id: &str) -> Rc<MockWorld> {
            Rc::new(MockWorld {
                loaded: Cell::new(true),
                name: "TestWorld".to_string(),
                id: id.to_string(),
                width: Cell::new(512),
                rules: Cell::new(false),
                password: RefCell::new(None),
                pvp: Cell::new(true),
                calls: RefCell::new(Vec::new()),
                clients: RefCell::new(HashMap::new()),
                accept_clients: Cell::new(true),
                rules_ok: Cell::new(true),
                migrated: Cell::new(true),
                dw_present: Cell::new(true),
                summary: RefCell::new(None),
                dw: Rc::new(MockDw { calls: RefCell::new(Vec::new()) }),
            })
        }
        pub fn rec_log(&self, s: impl Into<String>) {
            self.calls.borrow_mut().push(s.into());
        }
    }
    pub struct MockDw {
        pub calls: RefCell<Vec<String>>,
    }
    impl ServerDynamicWorld for MockDw {
        fn get_next_dynamic_object_id(&self) -> u64 {
            0x94
        }
        fn client_pickup_request_count_client_id_blockhead_requester_unique_id(
            &self,
            ids: &[u64],
            client_id: &str,
            requester: u64,
        ) {
            self.calls.borrow_mut().push(format!("pickup {:?} {} {}", ids, client_id, requester));
        }
        fn create_client_freeblocks_with_data(&self, payload: &[u8]) {
            self.calls.borrow_mut().push(format!("freeblocks {}", payload.len()));
        }
        fn load_client_blockheads_data_for_player_id(&self, _id: &str) -> Option<Vec<u8>> {
            None
        }
        fn client_blockheads_recieved_for_player_id_data(&self, id: &str, payload: &[u8]) {
            self.calls.borrow_mut().push(format!("blockheads {} {}", id, payload.len()));
        }
        fn client_blockhead_inventory_recieved_for_player_id_blockhead_id_data(&self, id: &str, bid: u64, data: &[u8]) {
            self.calls.borrow_mut().push(format!("inventory {} {} {}", id, bid, data.len()));
        }
        fn send_painting_data_for_painting_with_id_to_client(&self, pid: u64, id: &str) {
            self.calls.borrow_mut().push(format!("painting {} {}", pid, id));
        }
        fn new_found_list_recieved_from_client_list(&self, id: &str, list: &crate::foundation::indexset::IndexSet) {
            self.calls.borrow_mut().push(format!("found {} {}", id, list.count()));
        }
        fn client_blockhead_with_id_from_client_requests_dyamic_object_removal_with_id(
            &self,
            bh: u64,
            id: &str,
            obj: u64,
        ) {
            self.calls.borrow_mut().push(format!("removal {} {} {}", bh, id, obj));
        }
        fn user_ban_changed_is_banned(&self, id: &str, banned: bool) {
            self.calls.borrow_mut().push(format!("userban {} {}", id, banned));
        }
    }
    impl ServerWorld for MockWorld {
        fn load_complete(&self) -> bool {
            self.loaded.get()
        }
        fn world_name(&self) -> Option<String> {
            Some(self.name.clone())
        }
        fn save_id(&self) -> Option<String> {
            Some(self.id.clone())
        }
        fn world_width_macro(&self) -> i32 {
            self.width.get()
        }
        fn custom_rules_enabled(&self) -> bool {
            self.rules.get()
        }
        fn custom_rules_dict(&self) -> Option<PlistValue> {
            None
        }
        fn server_password(&self) -> Option<String> {
            self.password.borrow().clone()
        }
        fn welcome_message(&self) -> Option<String> {
            None
        }
        fn pvp_enabled(&self) -> bool {
            self.pvp.get()
        }
        fn set_pvp_enabled_display_notifciation(&self, e: bool, _d: bool) {
            self.pvp.set(e);
        }
        fn new_server_password_set(&self, p: Option<&str>) {
            *self.password.borrow_mut() = p.map(|s| s.to_string());
        }
        fn server_client_count(&self) -> usize {
            self.clients.borrow().len()
        }
        fn server_client(&self, id: &str) -> Option<Rc<ServerClient>> {
            self.clients.borrow().get(id).cloned()
        }
        fn dynamic_world(&self) -> Option<Rc<dyn ServerDynamicWorld>> {
            let dw: Rc<dyn ServerDynamicWorld> = self.dw.clone();
            Some(dw).filter(|_| self.dw_present.get())
        }
        fn distance_ordered_food_types(&self) -> Vec<i32> {
            (0..31).collect()
        }
        fn has_finished_database_migration_to17(&self) -> bool {
            self.migrated.get()
        }
        fn append_debug_log(&self, out: &mut String) {
            out.push_str("loaded macro blocks:7\n");
        }
        fn summary_net_data_for_admin_mod_owner_cloud_mode(&self, admin: bool, m: bool, o: bool, cm: i32) -> Vec<u8> {
            self.summary
                .borrow()
                .clone()
                .unwrap_or_else(|| vec![0x01, admin as u8, m as u8, o as u8, (cm & 0xff) as u8])
        }
        fn client_connected(&self, id: &str) -> bool {
            self.rec_log(format!("client_connected {}", id));
            self.accept_clients.get()
        }
        fn client_disconnected_was_kick(&self, id: &str, k: bool) {
            self.rec_log(format!("client_disconnected {} {}", id, k));
        }
        fn request_for_block_from_client(&self, req: ClientMacroBlockRequest, id: &str) {
            self.rec_log(format!("block_request {} {} {}", req.macro_index, req.create_if_not_created, id));
        }
        fn remote_block_removed_by_client(&self, i: i32, id: &str) {
            self.rec_log(format!("block_removed {} {}", i, id));
        }
        fn remote_create_for_objects_of_type_client_id(&self, items: Vec<NetBlob>, ty: i32, id: &str) {
            self.rec_log(format!("create {} {} {}", items.len(), ty, id));
        }
        fn remote_update_for_objects_of_type_from_client(&self, items: Vec<NetBlob>, ty: i32, id: &str) {
            self.rec_log(format!("update {} {} {}", items.len(), ty, id));
        }
        fn remote_remove_for_objects_of_type_from_client(&self, items: Vec<NetBlob>, ty: i32, id: &str) {
            self.rec_log(format!("remove {} {} {}", items.len(), ty, id));
        }
        fn remote_creation_data_update_for_objects_of_type_from_client(&self, items: Vec<NetBlob>, ty: i32, id: &str) {
            self.rec_log(format!("cdu {} {} {}", items.len(), ty, id));
        }
        fn remote_fill_request_placed_by_client(&self, _p: &[u8], id: &str) -> u32 {
            self.rec_log(format!("fill {}", id));
            7
        }
        fn remote_remove_request_from_client(&self, _p: &[u8], id: &str) -> u32 {
            self.rec_log(format!("rm {}", id));
            8
        }
        fn remote_remove_back_wall_request_from_client(&self, _p: &[u8], id: &str) -> u32 {
            self.rec_log(format!("rmbw {}", id));
            9
        }
        fn remote_place_workbench_request_from_peer(&self, _p: &[u8], id: &str) {
            self.rec_log(format!("workbench {}", id));
        }
        fn remote_place_interaction_object_request_from_peer(&self, _p: &[u8], id: &str) {
            self.rec_log(format!("ito {}", id));
        }
        fn remote_paint_request_from_client(&self, _p: &[u8], id: &str) {
            self.rec_log(format!("paint {}", id));
        }
        fn heartbeat_data_recieved_from_peer(&self, _p: &[u8], id: &str) {
            self.rec_log(format!("heartbeat {}", id));
        }
        fn remote_gather_request(&self, _p: &[u8]) {
            self.rec_log("gather");
        }
        fn remote_projectile_request(&self, _p: &[u8]) {
            self.rec_log("projectile");
        }
        fn remote_tip_notification(&self, _p: &[u8]) {
            self.rec_log("tip");
        }
        fn remote_blockhead_damage_request_requested_by_client_name(&self, _p: &[u8], name: Option<&str>) {
            self.rec_log(format!("damage {:?}", name));
        }
        fn electricity_path_data_recieved_from_client(&self, _p: &[u8], id: &str) {
            self.rec_log(format!("electricity {}", id));
        }
        fn get_blockhead_zoom_pos_for_client_requester_client_cycle_index(
            &self,
            p: Option<&str>,
            _r: Option<&str>,
            _c: i32,
        ) -> IntPair {
            // A player called "up" is found at (3, 4); anyone else is not on the map.
            IntPair { x: 3 * (p == Some("up")) as i32, y: 4 * (p == Some("up")) as i32 }
        }
        fn transaction_data_recieved_from_client(&self, d: Option<&PlistValue>, id: &str) {
            self.rec_log(format!("transaction {} {}", d.is_some(), id));
        }
        fn welcome_message_dict_recieved_from_client(&self, d: Option<&PlistValue>, id: &str) {
            self.rec_log(format!("welcome_set {} {}", d.is_some(), id));
        }
        fn report_user_with_name_reporter_name_reporter_message_reported_avatar_image_path(
            &self,
            n: Option<&str>,
            r: Option<&str>,
            _m: Option<&str>,
            a: Option<&str>,
        ) {
            self.rec_log(format!("report {:?} {:?} {:?}", n, r, a));
        }
        fn verify_client_custom_rules_data(&self, _p: &[u8]) -> bool {
            self.rules_ok.get()
        }
        fn custom_rule_dict_recieved_from_net(&self, _d: &PlistValue) {
            self.rec_log("custom_rule");
        }
        fn archive_light_blocks_for_client(&self, id: &str) {
            self.rec_log(format!("archive_light {}", id));
        }
        fn do_repair_for_tile_at_pos(&self, p: IntPair) {
            self.rec_log(format!("repair {} {}", p.x, p.y));
        }
        fn world_pos_at_world_index(&self, i: i32) -> IntPair {
            IntPair { x: i, y: i }
        }
    }

    /// A scriptable mod host: every call is logged, the answers are set by the test.
    pub struct StubHooks {
        pub log: RefCell<Vec<String>>,
        pub verdict: RefCell<crate::mods::ChatVerdict>,
        pub owns: Cell<bool>,
        pub consumes: Cell<bool>,
        pub limbo: Cell<bool>,
        pub limbo_packet: Cell<bool>,
        pub pre_login: Cell<Option<u8>>,
    }

    impl StubHooks {
        /// Installs a stub as the mod host of this thread; `uninstall` (or the end of the thread) removes it.
        pub fn install() -> Rc<StubHooks> {
            let h = Rc::new(StubHooks {
                log: RefCell::new(Vec::new()),
                verdict: RefCell::new(crate::mods::ChatVerdict::Keep),
                owns: Cell::new(false),
                consumes: Cell::new(true),
                limbo: Cell::new(false),
                limbo_packet: Cell::new(false),
                pre_login: Cell::new(None),
            });
            let dynh: Rc<dyn crate::mods::ModHooks> = h.clone();
            crate::mods::install(Some(dynh));
            h
        }
        pub fn uninstall() {
            crate::mods::install(None);
        }
        fn rec(&self, s: String) {
            self.log.borrow_mut().push(s);
        }
        pub fn saw(&self, prefix: &str) -> bool {
            self.log.borrow().iter().any(|l| l.starts_with(prefix))
        }
    }

    impl crate::mods::ModHooks for StubHooks {
        fn player_join(&self, id: &str) {
            self.rec(format!("join {id}"));
        }
        fn player_quit(&self, id: &str, kick: bool) {
            self.rec(format!("quit {id} {kick}"));
        }
        fn chat(&self, id: &str, text: &str) -> crate::mods::ChatVerdict {
            self.rec(format!("chat {id} {text}"));
            self.verdict.borrow().clone()
        }
        fn owns_command(&self, line: &str) -> bool {
            self.rec(format!("owns {line}"));
            self.owns.get()
        }
        fn command(&self, issuer: Option<&str>, line: &str) -> bool {
            self.rec(format!("command {issuer:?} {line}"));
            self.consumes.get()
        }
        fn tick(&self, dt: f32) {
            self.rec(format!("tick {dt}"));
        }
        fn poll(&self) {
            self.rec("poll".to_string());
        }
        fn world_loaded(&self, name: &str, id: &str) {
            self.rec(format!("loaded {name} {id}"));
        }
        fn world_save(&self, post: bool) {
            self.rec(format!("save {post}"));
        }
        fn server_stop(&self) {
            self.rec("stop".to_string());
        }
        fn wants(&self, what: crate::mods::Optional) -> bool {
            self.rec(format!("wants {what:?}"));
            true
        }
        fn mod_packet(&self, peer: &str, data: &[u8]) {
            self.rec(format!("packet {peer} {}", data.len()));
        }
        fn pre_login(&self, peer: &str, id: &str, alias: &str, _loader: Option<&crate::mods::LoaderKey>) -> Option<u8> {
            self.rec(format!("pre_login {peer} {id} {alias}"));
            self.pre_login.get()
        }
        fn limbo_join(&self, peer: &str, id: &str, _loader: Option<&crate::mods::LoaderKey>) -> bool {
            self.rec(format!("limbo_join {peer} {id}"));
            self.limbo.get()
        }
        fn limbo_packet(&self, peer: &str) -> bool {
            self.rec(format!("limbo_packet {peer}"));
            self.limbo_packet.get()
        }
        fn limbo_gone(&self, peer: &str) {
            self.rec(format!("limbo_gone {peer}"));
        }
        fn block_place(&self, id: &str, pos: IntPair, item: u32, a: u16, b: u16) -> bool {
            self.rec(format!("block_place {id} {} {} {item} {a} {b}", pos.x, pos.y));
            false
        }
        fn block_break(&self, id: &str, pos: IntPair, back_wall: bool, tile: u32) -> bool {
            self.rec(format!("block_break {id} {} {} {back_wall} {tile}", pos.x, pos.y));
            false
        }
        fn block_changed(&self, pos: IntPair, tile: u32, id: Option<&str>) {
            self.rec(format!("block_changed {} {} {tile} {id:?}", pos.x, pos.y));
        }
        fn object_place(&self, id: &str, pos: IntPair, workbench: bool, item: u32) -> bool {
            self.rec(format!("object_place {id} {} {} {workbench} {item}", pos.x, pos.y));
            false
        }
        fn damage(&self, id: Option<&str>, blockhead: u64, amount: f32, attacker: Option<&str>) -> Option<f32> {
            self.rec(format!("damage {id:?} {blockhead} {amount} {attacker:?}"));
            Some(amount)
        }
        fn death(&self, id: Option<&str>, blockhead: u64) {
            self.rec(format!("death {id:?} {blockhead}"));
        }
        fn player_move(&self, id: &str, from: IntPair, to: IntPair) {
            self.rec(format!("move {id} {} {} {} {}", from.x, from.y, to.x, to.y));
        }
    }

    /// No-op timers for a real server match.
    struct NoTimers;
    impl crate::net::bhmatch::TimerScheduler for NoTimers {
        fn schedule_repeating(&self, _interval: f64, _cb: Box<dyn FnMut()>) -> crate::net::bhmatch::TimerId {
            1
        }
        fn invalidate(&self, _id: crate::net::bhmatch::TimerId) {}
    }

    /// A server over a real `BhNetServerMatch` (ephemeral port, no peers): for the cloud, credit and privacy paths.
    pub struct RealMatchHarness {
        pub srv: Rc<BHServer>,
        pub m: Rc<crate::net::server_match::BhNetServerMatch>,
        pub d: Rc<MockDelegate>,
        pub w: Rc<MockWorld>,
        #[allow(dead_code)] // keeps the world alive for the server's weak reference
        pub world_dyn: Rc<dyn ServerWorld>,
    }

    impl RealMatchHarness {
        /// `salt` makes it a cloud match; `privacy` and `credit` are the match's.
        pub fn new(
            tag: &str,
            salt: Option<&str>,
            owner: Option<&str>,
            privacy: Option<&str>,
            credit: f32,
        ) -> RealMatchHarness {
            init_env();
            let id = format!("{:032x}", {
                use std::hash::{Hash, Hasher};
                let mut h = std::collections::hash_map::DefaultHasher::new();
                tag.hash(&mut h);
                std::process::id().hash(&mut h);
                h.finish() as u128
            });
            let _ = std::fs::remove_dir_all(format!("{}/saves/{}", paths::appsup(), id));
            let m = crate::net::server_match::BhNetServerMatch::init_with_port_user_name_photo_has_local_player_world_name_credit_cloud_salt_owner_name_privacy(
                crate::net::server_match::ServerMatchArgs {
                    port: Some("0".to_string()),
                    user_name: Some("server".to_string()),
                    has_local_player: false,
                    world_name: Some("TestWorld".to_string()),
                    credit,
                    cloud_salt: salt.map(|s| s.to_string()),
                    owner_name: owner.map(|s| s.to_string()),
                    privacy: privacy.map(|s| s.to_string()),
                },
                Rc::new(NoTimers),
            )
            .expect("server match");
            let d = MockDelegate::new();
            let dd: Rc<dyn ServerDelegate> = d.clone();
            let mm: Rc<dyn BhMatch> = m.clone();
            let srv = BHServer::init_with_delegate_match_net_node_type_save_id_max_players(
                Rc::downgrade(&dd),
                mm,
                NetNodeType::OnlineServer,
                Some(&id),
                32,
            )
            .expect("init");
            let delayed: Rc<RefCell<Vec<DelayedCall>>> = Rc::new(RefCell::new(Vec::new()));
            srv.set_delay_scheduler(Some(Rc::new(move |t, cb| delayed.borrow_mut().push((t, cb)))));
            let w = MockWorld::new(&id);
            let world_dyn: Rc<dyn ServerWorld> = w.clone();
            srv.set_world(&world_dyn);
            RealMatchHarness { srv, m, d, w, world_dyn }
        }
    }

    /// Everything a test needs.
    pub struct Harness {
        pub srv: Rc<BHServer>,
        pub m: Rc<MockMatch>,
        pub d: Rc<MockDelegate>,
        pub w: Rc<MockWorld>,
        #[allow(dead_code)] // keeps the world alive for the server's weak reference
        pub world_dyn: Rc<dyn ServerWorld>,
        pub delayed: Rc<RefCell<Vec<DelayedCall>>>,
    }

    impl Harness {
        /// Build a server on a fresh save id with a loaded mock world.
        pub fn new(tag: &str) -> Harness {
            Self::with_files(tag, |_| {})
        }

        /// Like `new` but runs `pre(save_dir)` before the server initialises (list files).
        pub fn with_files(tag: &str, pre: impl FnOnce(&str)) -> Harness {
            Self::build(tag, NetNodeType::OnlineServer, Box::new(pre), Box::new(|_| {}))
        }

        /// Like `with_files`, with the mock match adjusted before the server initialises.
        pub fn tuned(tag: &str, pre: impl FnOnce(&str), tune: impl FnOnce(&MockMatch)) -> Harness {
            Self::build(tag, NetNodeType::OnlineServer, Box::new(pre), Box::new(tune))
        }

        /// A server in a local-network game (some commands differ).
        pub fn local_server(tag: &str) -> Harness {
            Self::build(tag, NetNodeType::LocalServer, Box::new(|_| {}), Box::new(|_| {}))
        }

        // Not generic: every test shares one copy of the set-up code.
        fn build(
            tag: &str,
            node_type: NetNodeType,
            pre: Box<dyn FnOnce(&str) + '_>,
            tune: Box<dyn FnOnce(&MockMatch) + '_>,
        ) -> Harness {
            init_env();
            let id = format!("{:032x}", {
                use std::hash::{Hash, Hasher};
                let mut h = std::collections::hash_map::DefaultHasher::new();
                tag.hash(&mut h);
                std::process::id().hash(&mut h);
                h.finish() as u128
            });
            let save = format!("{}/saves/{}", paths::appsup(), id);
            let _ = std::fs::remove_dir_all(&save);
            let _ = std::fs::create_dir_all(&save);
            pre(&save);
            let m = MockMatch::new();
            tune(&m);
            let d = MockDelegate::new();
            let dd: Rc<dyn ServerDelegate> = d.clone();
            let srv = BHServer::init_with_delegate_match_net_node_type_save_id_max_players(
                Rc::downgrade(&dd),
                m.clone(),
                node_type,
                Some(&id),
                32,
            )
            .expect("init");
            let delayed: Rc<RefCell<Vec<DelayedCall>>> = Rc::new(RefCell::new(Vec::new()));
            let dl = delayed.clone();
            srv.set_delay_scheduler(Some(Rc::new(move |t, cb| dl.borrow_mut().push((t, cb)))));
            let w = MockWorld::new(&id);
            let world_dyn: Rc<dyn ServerWorld> = w.clone();
            srv.set_world(&world_dyn);
            Harness { srv, m, d, w, world_dyn, delayed }
        }

        /// Run (and drain) all pending delayed callbacks.
        pub fn run_delayed(&self) {
            loop {
                let items: Vec<_> = self.delayed.borrow_mut().drain(..).collect();
                if items.is_empty() {
                    break;
                }
                for (_, cb) in items {
                    cb();
                }
            }
        }

        /// ENet connect (state 1).
        pub fn connect(&self, enet: &str) {
            self.srv.match_player_did_change_state(&*self.m, enet, PlayerConnectionState::Connected);
        }

        /// Send a packet.
        pub fn send(&self, peer: &str, data: &[u8]) {
            self.srv.match_did_receive_data_from_player(&*self.m, data, peer);
        }

        /// Build a 0x1f packet.
        pub fn handshake_packet(alias: &str, udid: &str, minor: i64) -> Vec<u8> {
            let mut d = new_mutable_dict();
            d.insert("alias".to_string(), PlistValue::string(alias));
            d.insert("udidNew".to_string(), PlistValue::string(udid));
            d.insert("minorVersion".to_string(), PlistValue::int(minor));
            let body = data_from_property_list(&PlistValue::Dict(d)).unwrap();
            let mut p = vec![0x1f];
            p.extend(body);
            p
        }

        /// Full join of `alias`; returns the persistent id (== enet id in the mock).
        pub fn join(&self, alias: &str) -> String {
            let pid = crate::foundation::md5::string_from_md5(&nsstring::lowercase_string(alias));
            self.m.persistent.borrow_mut().insert(pid.clone(), pid.clone());
            self.connect(&pid);
            self.send(&pid, &Self::handshake_packet(alias, &format!("udid-{}", alias), 5));
            pid
        }

        /// Decode a packet body plist.
        pub fn plist_of(packet: &[u8], skip: usize) -> Option<PlistValue> {
            property_list_from_data(&packet[skip..])
        }
    }
}

#[cfg(test)]
#[path = "bhserver_tests.rs"]
mod server_tests;
