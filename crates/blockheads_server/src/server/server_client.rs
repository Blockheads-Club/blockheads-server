//! `ServerClient`: per-remote-player bookkeeping (block requests, wired blocks / objects, per-type object queues,
//! fill receipts, light-block index persistence).
//!

use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, HashSet};
use std::rc::{Rc, Weak};

use crate::dynworld::state::DwServerClient;
use crate::foundation::indexset::IndexSet;
use crate::plist::keyed_archiver::{archive_index_set, unarchive_index_set_opt};
use crate::plist::value::PlistValue;
use crate::protocol::messages::ClientMacroBlockRequest;
use crate::protocol::object_batches::{encode_batch, BatchKind, NUM_TYPE_BUCKETS};
use crate::server::bhserver::{BHServer, NetBlob, ServerWorld};
use crate::storage::lmdb::Database;
use crate::types::structs::IntPair;

/// One `NSMutableDictionary` of 65 `NSMutableArray`s (types 0..=64).
type Buckets = RefCell<Vec<Vec<NetBlob>>>;

fn new_buckets() -> Buckets {
    RefCell::new((0..NUM_TYPE_BUCKETS).map(|_| Vec::new()).collect())
}

/// First 8 bytes of a blob as a host-order `u64` (`getBytes:length:8`; a short blob is zero padded).
fn blob_id(b: &[u8]) -> u64 {
    let mut a = [0u8; 8];
    let n = b.len().min(8);
    a[..n].copy_from_slice(&b[..n]);
    u64::from_le_bytes(a)
}

/// `removeObjectsAtIndexes:` of every blob whose leading id equals `id`.
fn purge(bucket: &mut Vec<NetBlob>, id: u64) {
    if !bucket.is_empty() {
        bucket.retain(|b| blob_id(b) != id);
    }
}

/// Class `ServerClient`.
pub struct ServerClient {
    client_id: String,
    server: Weak<BHServer>,
    requested_block_indices: RefCell<IndexSet>,
    requested_block_request_types: RefCell<BTreeMap<u32, u8>>,
    wired_blocks: RefCell<IndexSet>,
    wired_dynamic_objects: RefCell<IndexSet>,
    creation_arrays_to_send: Buckets,
    update_arrays_to_send: Buckets,
    creation_data_update_arrays_to_send: Buckets,
    update_unreliable_arrays_to_send: Buckets,
    removal_arrays_to_send: Buckets,
    requests_heart_beat: Cell<bool>,
    has_ever_requested_heartbeat: Cell<bool>,
    time_since_last_heartbeat_request: Cell<f32>,
    light_block_index: i32,
    fill_indices: RefCell<Vec<u32>>,
    world_indices_containing_tamed_animals: RefCell<Vec<HashSet<u32>>>,
    paused: Cell<bool>,
    connected: Cell<bool>,
    is_admin: Cell<bool>,
    found_items_list: RefCell<IndexSet>,
    all_light_block_indices: RefCell<IndexSet>,
    light_block_database: Option<Rc<Database>>,
}

impl ServerClient {
    /// `-initWithClientID:server:lightBlockIndex:lightBlockDatabase:`.
    pub fn init_with_client_id_server_light_block_index_light_block_database(
        client_id: &str,
        server: &Rc<BHServer>,
        light_block_index: i32,
        light_block_database: Option<Rc<Database>>,
    ) -> Rc<ServerClient> {
        let alias = server.player_alias_for_peer_id(Some(client_id));
        let is_admin = server.player_is_admin_with_alias(alias.as_deref());
        let mut all = IndexSet::new();
        let saved = unarchive_index_set_opt(
            light_block_database
                .as_ref()
                .and_then(|d| d.data_for_key(Some(&format!("{}_allIndexes", client_id))))
                .as_deref(),
            "all",
        );
        if let Some(s) = saved {
            all.add_indexes(&s);
        }
        Rc::new(ServerClient {
            client_id: client_id.to_string(),
            server: Rc::downgrade(server),
            requested_block_indices: RefCell::new(IndexSet::new()),
            requested_block_request_types: RefCell::new(BTreeMap::new()),
            wired_blocks: RefCell::new(IndexSet::new()),
            wired_dynamic_objects: RefCell::new(IndexSet::new()),
            creation_arrays_to_send: new_buckets(),
            update_arrays_to_send: new_buckets(),
            creation_data_update_arrays_to_send: new_buckets(),
            update_unreliable_arrays_to_send: new_buckets(),
            removal_arrays_to_send: new_buckets(),
            requests_heart_beat: Cell::new(false),
            has_ever_requested_heartbeat: Cell::new(false),
            time_since_last_heartbeat_request: Cell::new(0.0),
            light_block_index,
            fill_indices: RefCell::new(Vec::new()),
            world_indices_containing_tamed_animals: RefCell::new(
                (0..NUM_TYPE_BUCKETS).map(|_| HashSet::new()).collect(),
            ),
            paused: Cell::new(false),
            connected: Cell::new(true),
            is_admin: Cell::new(is_admin),
            found_items_list: RefCell::new(IndexSet::new()),
            all_light_block_indices: RefCell::new(all),
            light_block_database,
        })
    }

    /// `-clientReconnected`: resets transient state (not the request-type map, paused, heartbeat flags, found items,
    /// light-block indices or tamed-animal table).
    pub fn client_reconnected(&self) {
        self.connected.set(true);
        if let Some(srv) = self.server.upgrade() {
            let alias = srv.player_alias_for_peer_id(Some(&self.client_id));
            self.is_admin.set(srv.player_is_admin_with_alias(alias.as_deref()));
        }
        self.wired_blocks.borrow_mut().remove_all_indexes();
        self.wired_dynamic_objects.borrow_mut().remove_all_indexes();
        self.requested_block_indices.borrow_mut().remove_all_indexes();
        self.fill_indices.borrow_mut().clear();
        for b in [
            &self.creation_arrays_to_send,
            &self.update_arrays_to_send,
            &self.creation_data_update_arrays_to_send,
            &self.update_unreliable_arrays_to_send,
            &self.removal_arrays_to_send,
        ] {
            *b.borrow_mut() = (0..NUM_TYPE_BUCKETS).map(|_| Vec::new()).collect();
        }
    }

    // ---- blocks ----

    /// `-requestForBlock:`.
    pub fn request_for_block(&self, request: ClientMacroBlockRequest) {
        let key = request.macro_index as u32;
        let current = self.requested_block_request_types.borrow().get(&key).copied().unwrap_or(0);
        if current == 0 {
            let idx = request.macro_index as i64 as u64;
            self.requested_block_indices.borrow_mut().add_index(idx);
            self.requested_block_request_types
                .borrow_mut()
                .insert(key, if request.create_if_not_created != 0 { 1 } else { 2 });
            self.wired_blocks.borrow_mut().add_index(idx);
        }
    }

    /// `-blockIsWired:`.
    pub fn block_is_wired(&self, macro_index: i32) -> bool {
        self.wired_blocks.borrow().contains_index(macro_index as i64 as u64)
    }

    /// `-blockIsRequested:` (uses the map, not the index set).
    pub fn block_is_requested(&self, macro_index: i32) -> bool {
        self.requested_block_request_types.borrow().get(&(macro_index as u32)).copied().unwrap_or(0) != 0
    }

    /// `-removeBlockRequest:` (does not unwire).
    pub fn remove_block_request(&self, macro_index: i32) {
        self.requested_block_request_types.borrow_mut().insert(macro_index as u32, 0);
        self.requested_block_indices.borrow_mut().remove_index(macro_index as i64 as u64);
    }

    /// `-createIfNotCreatedForBlockRequest:`.
    pub fn create_if_not_created_for_block_request(&self, macro_index: i32) -> bool {
        self.requested_block_request_types.borrow().get(&(macro_index as u32)).copied().unwrap_or(0) == 1
    }

    /// `-blockRemoved:` (unwire only).
    pub fn block_removed(&self, macro_index: i32) {
        self.wired_blocks.borrow_mut().remove_index(macro_index as i64 as u64);
    }

    /// `-requestedBlockIndices` (snapshot clone; use `.iter().take(4)` for `getIndexes:maxCount:4`).
    pub fn requested_block_indices(&self) -> IndexSet {
        self.requested_block_indices.borrow().clone()
    }

    // ---- dynamic objects ----

    /// `-dynamicObjectIsWired:`.
    pub fn dynamic_object_is_wired(&self, unique_id: u64) -> bool {
        self.wired_dynamic_objects.borrow().contains_index(unique_id)
    }
    /// `-wireDynamicObject:`.
    pub fn wire_dynamic_object(&self, unique_id: u64) {
        self.wired_dynamic_objects.borrow_mut().add_index(unique_id);
    }
    /// `-unwireDynamicObject:`.
    pub fn unwire_dynamic_object(&self, unique_id: u64) {
        self.wired_dynamic_objects.borrow_mut().remove_index(unique_id);
    }

    /// `-addCreationObjectDataToSend:ofType:objectID:`: push, THEN purge queued removals of the same id.
    pub fn add_creation_object_data_to_send_of_type_object_id(&self, net_data: &NetBlob, ty: u8, object_id: u64) {
        self.creation_arrays_to_send.borrow_mut()[ty as usize].push(net_data.clone());
        purge(&mut self.removal_arrays_to_send.borrow_mut()[ty as usize], object_id);
    }

    /// `-addCreationDataUpdateObjectDataToSend:ofType:` (no dedupe).
    pub fn add_creation_data_update_object_data_to_send_of_type(&self, net_data: &NetBlob, ty: u8) {
        self.creation_data_update_arrays_to_send.borrow_mut()[ty as usize].push(net_data.clone());
    }

    /// `-addUpdateObjectDataToSend:ofType:reliable:` (no dedupe).
    pub fn add_update_object_data_to_send_of_type_reliable(&self, net_data: &NetBlob, ty: u8, reliable: bool) {
        let target = if reliable { &self.update_arrays_to_send } else { &self.update_unreliable_arrays_to_send };
        target.borrow_mut()[ty as usize].push(net_data.clone());
    }

    /// `-addRemovalObjectDataToSend:ofType:objectID:`: push, THEN purge creation, update(reliable) and
    /// creation-data-update queues (not the unreliable updates).
    pub fn add_removal_object_data_to_send_of_type_object_id(&self, net_data: &NetBlob, ty: u8, object_id: u64) {
        self.removal_arrays_to_send.borrow_mut()[ty as usize].push(net_data.clone());
        purge(&mut self.creation_arrays_to_send.borrow_mut()[ty as usize], object_id);
        purge(&mut self.update_arrays_to_send.borrow_mut()[ty as usize], object_id);
        purge(&mut self.creation_data_update_arrays_to_send.borrow_mut()[ty as usize], object_id);
    }

    /// `-sendAllObjectData`: types ascending; per type creation (0x07), creation-data-update (0x46), update
    /// reliable (0x08), update unreliable (0x08), removal (0x09). Nothing when disconnected.
    pub fn send_all_object_data(&self) {
        if !self.connected.get() {
            return;
        }
        let Some(srv) = self.server.upgrade() else { return };
        let steps: [(&Buckets, BatchKind, bool); 5] = [
            (&self.creation_arrays_to_send, BatchKind::Create, true),
            (&self.creation_data_update_arrays_to_send, BatchKind::CreationDataUpdate, true),
            (&self.update_arrays_to_send, BatchKind::Update, true),
            (&self.update_unreliable_arrays_to_send, BatchKind::Update, false),
            (&self.removal_arrays_to_send, BatchKind::Remove, true),
        ];
        for i in 0..NUM_TYPE_BUCKETS {
            for (buckets, kind, reliable) in &steps {
                let items = std::mem::take(&mut buckets.borrow_mut()[i]);
                if items.is_empty() {
                    continue;
                }
                // dataFromPropertyList failure: nothing is appended after the two header bytes.
                let pkt = encode_batch(*kind, i as u8, &items).unwrap_or_else(|| vec![*kind as u8, i as u8]);
                srv.send_network_data_to_peers_reliable(pkt, &[self.client_id.clone()], *reliable);
            }
        }
    }

    // ---- fills ----

    /// `-addFillRequest:`.
    pub fn add_fill_request(&self, fill_index: u32) {
        self.fill_indices.borrow_mut().push(fill_index);
    }

    /// `-getAndRemoveAllRecieptDataForMacroPos:world:` (signed C division). `None` == nil.
    pub fn get_and_remove_all_reciept_data_for_macro_pos_world(
        &self,
        macro_pos: IntPair,
        world: &dyn ServerWorld,
    ) -> Option<PlistValue> {
        let w = world.world_width_macro();
        let row_width = w.wrapping_shl(5);
        let mut ret: Vec<u32> = Vec::new();
        for n in self.fill_indices.borrow().iter() {
            let idx = *n as i32;
            let row_t = idx.wrapping_div(row_width);
            let col_t = idx.wrapping_rem(row_width);
            if col_t / 32 == macro_pos.x && row_t / 32 == macro_pos.y {
                ret.push(*n);
            }
        }
        if ret.is_empty() {
            return None;
        }
        self.fill_indices.borrow_mut().retain(|x| !ret.contains(x));
        Some(PlistValue::array(ret.into_iter().map(|n| PlistValue::int(n as i64)).collect()))
    }

    // ---- light blocks ----

    /// `-saveLightBlockIndices`: NSKeyedArchiver record `<id>_allIndexes`.
    pub fn save_light_block_indices(&self) {
        let data = archive_index_set(&self.all_light_block_indices.borrow(), "all");
        if let Some(db) = &self.light_block_database {
            db.set_data(Some(&data), Some(&format!("{}_allIndexes", self.client_id)));
        }
    }

    /// `-allLightBlockIndices`.
    pub fn all_light_block_indices(&self) -> &RefCell<IndexSet> {
        &self.all_light_block_indices
    }
    /// `-foundItemsList`.
    pub fn found_items_list(&self) -> &RefCell<IndexSet> {
        &self.found_items_list
    }
    /// `-worldIndicesContainingTamedAnimals` (65 sets). The `DwServerClient` trait fixes the `HashSet` shape.
    pub fn world_indices_containing_tamed_animals(&self) -> &RefCell<Vec<HashSet<u32>>> {
        &self.world_indices_containing_tamed_animals
    }

    // ---- flags ----

    /// `-connected`.
    pub fn connected(&self) -> bool {
        self.connected.get()
    }
    /// `-setConnected:` (false also clears `paused`).
    pub fn set_connected(&self, c: bool) {
        self.connected.set(c);
        if !c {
            self.set_paused(false);
        }
    }
    /// `-paused`.
    pub fn paused(&self) -> bool {
        self.paused.get()
    }
    /// `-setPaused:`.
    pub fn set_paused(&self, p: bool) {
        self.paused.set(p);
    }
    /// `-isAdmin`.
    pub fn is_admin(&self) -> bool {
        self.is_admin.get()
    }
    /// `-setIsAdmin:`.
    pub fn set_is_admin(&self, a: bool) {
        self.is_admin.set(a);
    }
    /// `-clientID`.
    pub fn client_id(&self) -> &str {
        &self.client_id
    }
    /// `-lightBlockIndex`.
    pub fn light_block_index(&self) -> i32 {
        self.light_block_index
    }
    /// `-requestsHeartBeat`.
    pub fn requests_heart_beat(&self) -> bool {
        self.requests_heart_beat.get()
    }
    /// `-setRequestsHeartBeat:`.
    pub fn set_requests_heart_beat(&self, b: bool) {
        self.requests_heart_beat.set(b);
    }
    /// `-hasEverRequestedHeartbeat`.
    pub fn has_ever_requested_heartbeat(&self) -> bool {
        self.has_ever_requested_heartbeat.get()
    }
    /// `-setHasEverRequestedHeartbeat:`.
    pub fn set_has_ever_requested_heartbeat(&self, b: bool) {
        self.has_ever_requested_heartbeat.set(b);
    }
    /// `-timeSinceLastHeartbeatRequest`.
    pub fn time_since_last_heartbeat_request(&self) -> f32 {
        self.time_since_last_heartbeat_request.get()
    }
    /// `-setTimeSinceLastHeartbeatRequest:`.
    pub fn set_time_since_last_heartbeat_request(&self, t: f32) {
        self.time_since_last_heartbeat_request.set(t);
    }
    /// `-wiredDynamicObjects` (snapshot).
    pub fn wired_dynamic_objects(&self) -> IndexSet {
        self.wired_dynamic_objects.borrow().clone()
    }
}

impl DwServerClient for ServerClient {
    fn connected(&self) -> bool {
        ServerClient::connected(self)
    }
    fn client_id(&self) -> String {
        self.client_id.clone()
    }
    fn light_block_index(&self) -> i32 {
        self.light_block_index
    }
    fn block_is_wired(&self, macro_index: i32) -> bool {
        ServerClient::block_is_wired(self, macro_index)
    }
    fn dynamic_object_is_wired(&self, unique_id: u64) -> bool {
        ServerClient::dynamic_object_is_wired(self, unique_id)
    }
    fn wire_dynamic_object(&self, unique_id: u64) {
        ServerClient::wire_dynamic_object(self, unique_id)
    }
    fn unwire_dynamic_object(&self, unique_id: u64) {
        ServerClient::unwire_dynamic_object(self, unique_id)
    }
    fn add_creation_object_data_to_send_of_type_object_id(&self, data: &NetBlob, object_type: u8, object_id: u64) {
        ServerClient::add_creation_object_data_to_send_of_type_object_id(self, data, object_type, object_id)
    }
    fn add_creation_data_update_object_data_to_send_of_type(&self, data: &NetBlob, object_type: u8) {
        ServerClient::add_creation_data_update_object_data_to_send_of_type(self, data, object_type)
    }
    fn add_update_object_data_to_send_of_type_reliable(&self, data: &NetBlob, object_type: u8, reliable: bool) {
        ServerClient::add_update_object_data_to_send_of_type_reliable(self, data, object_type, reliable)
    }
    fn add_removal_object_data_to_send_of_type_object_id(&self, data: &NetBlob, object_type: u8, object_id: u64) {
        ServerClient::add_removal_object_data_to_send_of_type_object_id(self, data, object_type, object_id)
    }
    fn send_all_object_data(&self) {
        ServerClient::send_all_object_data(self)
    }
    fn world_indices_containing_tamed_animals(&self) -> &RefCell<Vec<HashSet<u32>>> {
        &self.world_indices_containing_tamed_animals
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::codec::gz_plist_from;
    use crate::server::bhserver::test_support::*;

    fn blob(id: u64, tag: u8) -> NetBlob {
        let mut v = id.to_le_bytes().to_vec();
        v.push(tag);
        v
    }

    fn client(h: &Harness, id: &str) -> Rc<ServerClient> {
        let lb = h.srv.server_database();
        ServerClient::init_with_client_id_server_light_block_index_light_block_database(id, &h.srv, 31, lb)
    }

    #[test]
    fn block_request_state_machine() {
        let h = Harness::new("sc_blocks");
        let c = client(&h, "c1");
        assert!(c.connected());
        assert!(!c.block_is_requested(5));
        c.request_for_block(ClientMacroBlockRequest { macro_index: 5, create_if_not_created: 1, padding: [0; 3] });
        assert!(c.block_is_requested(5) && c.block_is_wired(5) && c.create_if_not_created_for_block_request(5));
        // a second request while pending changes nothing
        c.request_for_block(ClientMacroBlockRequest { macro_index: 5, create_if_not_created: 0, padding: [0; 3] });
        assert!(c.create_if_not_created_for_block_request(5));
        assert!(c.requested_block_indices().contains_index(5));
        c.remove_block_request(5);
        assert!(!c.block_is_requested(5));
        assert!(c.block_is_wired(5), "removeBlockRequest does not unwire");
        assert!(!c.requested_block_indices().contains_index(5));
        c.block_removed(5);
        assert!(!c.block_is_wired(5));
        c.request_for_block(ClientMacroBlockRequest { macro_index: 6, create_if_not_created: 0, padding: [0; 3] });
        assert!(!c.create_if_not_created_for_block_request(6) && c.block_is_requested(6));
    }

    #[test]
    fn queue_purge_rules() {
        let h = Harness::new("sc_queue");
        let c = client(&h, "c2");
        // removal queued, then creation of the same id cancels it
        c.add_removal_object_data_to_send_of_type_object_id(&blob(7, 1), 3, 7);
        c.add_creation_object_data_to_send_of_type_object_id(&blob(7, 2), 3, 7);
        assert!(c.removal_arrays_to_send.borrow()[3].is_empty());
        assert_eq!(c.creation_arrays_to_send.borrow()[3].len(), 1);
        // removal purges creation, reliable update and creation-data-update, but not unreliable updates
        c.add_update_object_data_to_send_of_type_reliable(&blob(7, 3), 3, true);
        c.add_update_object_data_to_send_of_type_reliable(&blob(7, 4), 3, false);
        c.add_creation_data_update_object_data_to_send_of_type(&blob(7, 5), 3);
        c.add_creation_data_update_object_data_to_send_of_type(&blob(8, 5), 3);
        c.add_removal_object_data_to_send_of_type_object_id(&blob(7, 6), 3, 7);
        assert!(c.creation_arrays_to_send.borrow()[3].is_empty());
        assert!(c.update_arrays_to_send.borrow()[3].is_empty());
        assert_eq!(c.creation_data_update_arrays_to_send.borrow()[3].len(), 1);
        assert_eq!(c.update_unreliable_arrays_to_send.borrow()[3].len(), 1);
        assert_eq!(c.removal_arrays_to_send.borrow()[3].len(), 1);
    }

    #[test]
    fn send_all_framing_and_order() {
        let h = Harness::new("sc_send");
        let c = client(&h, "c3");
        c.add_removal_object_data_to_send_of_type_object_id(&blob(1, 0), 10, 1);
        c.add_update_object_data_to_send_of_type_reliable(&blob(2, 0), 10, false);
        c.add_update_object_data_to_send_of_type_reliable(&blob(2, 1), 10, true);
        c.add_creation_data_update_object_data_to_send_of_type(&blob(3, 0), 10);
        c.add_creation_object_data_to_send_of_type_object_id(&blob(4, 0), 10, 4);
        c.add_creation_object_data_to_send_of_type_object_id(&blob(5, 0), 2, 5);
        h.m.clear();
        c.send_all_object_data();
        let sent = h.m.packets_to("c3");
        let kinds: Vec<(u8, u8)> = sent.iter().map(|p| (p[0], p[1])).collect();
        assert_eq!(kinds, vec![(0x07, 2), (0x07, 10), (0x46, 10), (0x08, 10), (0x08, 10), (0x09, 10)]);
        let v = gz_plist_from(&sent[1][2..]).unwrap();
        if let PlistValue::Array(a) = v {
            assert_eq!(a.items.len(), 1);
        } else {
            panic!()
        }
        // queues are emptied
        h.m.clear();
        c.send_all_object_data();
        assert!(h.m.sent.borrow().is_empty());
        // disconnected: nothing sent, queues kept
        c.add_update_object_data_to_send_of_type_reliable(&blob(9, 0), 0, true);
        c.set_connected(false);
        c.send_all_object_data();
        assert!(h.m.sent.borrow().is_empty());
        assert_eq!(c.update_arrays_to_send.borrow()[0].len(), 1);
    }

    #[test]
    fn receipts_use_signed_c_division() {
        let h = Harness::new("sc_fill");
        let c = client(&h, "c4");
        // world width 512 macro -> row width 16384 tiles
        let w = &*h.w;
        let tile = |x: i32, y: i32| (y * 16384 + x) as u32;
        c.add_fill_request(tile(33, 2)); // macro (1, 0)
        c.add_fill_request(tile(40, 5)); // macro (1, 0)
        c.add_fill_request(tile(0, 40)); // macro (0, 1)
        let r = c.get_and_remove_all_reciept_data_for_macro_pos_world(IntPair { x: 1, y: 0 }, w).unwrap();
        if let PlistValue::Array(a) = r {
            assert_eq!(a.items.len(), 2);
        } else {
            panic!()
        }
        assert_eq!(c.fill_indices.borrow().len(), 1);
        assert!(c.get_and_remove_all_reciept_data_for_macro_pos_world(IntPair { x: 9, y: 9 }, w).is_none());
    }

    #[test]
    fn light_block_indices_persist() {
        let h = Harness::new("sc_light");
        let c = client(&h, "c5");
        c.all_light_block_indices().borrow_mut().add_index(8273);
        c.save_light_block_indices();
        let c2 = client(&h, "c5");
        assert!(c2.all_light_block_indices().borrow().contains_index(8273));
        assert!(!c2.all_light_block_indices().borrow().contains_index(1));
    }

    #[test]
    fn reconnect_resets_transient_state_only() {
        let h = Harness::new("sc_reconn");
        let c = client(&h, "c6");
        c.request_for_block(ClientMacroBlockRequest { macro_index: 1, create_if_not_created: 0, padding: [0; 3] });
        c.wire_dynamic_object(9);
        c.add_fill_request(1);
        c.set_paused(true);
        c.add_update_object_data_to_send_of_type_reliable(&blob(1, 0), 0, true);
        c.set_connected(false);
        assert!(!c.paused());
        c.set_paused(true);
        c.client_reconnected();
        assert!(c.connected());
        assert!(!c.block_is_wired(1) && !c.dynamic_object_is_wired(9));
        assert!(c.fill_indices.borrow().is_empty());
        assert!(c.update_arrays_to_send.borrow()[0].is_empty());
        assert!(c.block_is_requested(1), "request-type map is not reset");
        assert!(c.paused());
    }
}
