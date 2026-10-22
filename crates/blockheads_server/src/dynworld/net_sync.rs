//! Network ingestion of client object requests and the outgoing per-client replication of one object.
//!
//! `remoteCreationDataUpdate:`, `remoteUpdate:`, `remoteRemove:`, `updateNetObjects`, `sendNetDataIfNeededForObject:isCreation:`.
//! `client` is always nil on the dedicated server (decision 1): client branches are not ported.

use std::collections::BTreeSet;

use super::state::{DynamicWorld, NetBlob, MAX_DYNAMIC_OBJECT_TYPES};
use crate::nslog;
use crate::objects::handle::ObjRef;
use crate::objects::type_tables::{
    object_type_has_static_position, object_type_is_interaction_object, object_type_is_npc,
    DYNAMIC_OBJECT_TYPE_BLOCKHEAD, DYNAMIC_OBJECT_TYPE_DOOR, DYNAMIC_OBJECT_TYPE_FREE_BLOCK,
    DYNAMIC_OBJECT_TYPE_OWNERSHIP_SIGN, DYNAMIC_OBJECT_TYPE_WINDOW,
};
use crate::types::item_type::ItemType;
use crate::world::tiles::macro_pos_for_world_pos;

/// `uniqueID` at +0 of every `DynamicObjectNetData`. `None` for blobs shorter than 8 bytes (the original's
/// `getBytes:length:` would raise NSRangeException; UB-PARITY: such blobs are treated as unparseable).
fn nd_unique_id(b: &[u8]) -> Option<u64> {
    if b.len() >= 8 {
        Some(u64::from_le_bytes(b[0..8].try_into().unwrap()))
    } else {
        None
    }
}

/// Valid `DynamicObjectType` index (UB-PARITY: out-of-range indices corrupt memory in the original; ignored).
fn valid_type(t: i32) -> Option<usize> {
    if (0..MAX_DYNAMIC_OBJECT_TYPES as i32).contains(&t) {
        Some(t as usize)
    } else {
        None
    }
}

impl DynamicWorld {
    /// `-remoteCreate:forObjectsOfType:clientID:`.
    pub fn remote_create_for_objects_of_type_client_id(
        &self,
        creation_array: Vec<NetBlob>,
        object_type: i32,
        client_id: &str,
    ) {
        let Some(t) = valid_type(object_type) else { return }; // UB-PARITY
        if self.has_server() && object_type == DYNAMIC_OBJECT_TYPE_FREE_BLOCK {
            return; // clients may not create free blocks
        }
        let mut q = self.net_create_dynamic_objects.borrow_mut();
        let d = &mut q[t];
        let key = client_id.to_string();
        if d.get(&key).is_none() {
            d.insert(key.clone(), Vec::new());
        }
        d.get_mut(&key).unwrap().extend(creation_array);
    }

    /// `-remoteCreationDataUpdate:forObjectsOfType:fromClient:` (no permission checks).
    pub fn remote_creation_data_update_for_objects_of_type_from_client(
        &self,
        update_array: Vec<NetBlob>,
        object_type: i32,
        _client_id: Option<&str>,
    ) {
        let Some(t) = valid_type(object_type) else { return }; // UB-PARITY
        self.net_update_creation_data_dynamic_objects.borrow_mut()[t].extend(update_array);
    }

    /// `-remoteUpdate:forObjectsOfType:fromClient:`.
    pub fn remote_update_for_objects_of_type_from_client(
        &self,
        update_array: Vec<NetBlob>,
        object_type: i32,
        client_id: Option<&str>,
    ) {
        let Some(t) = valid_type(object_type) else { return }; // UB-PARITY
        if object_type == DYNAMIC_OBJECT_TYPE_OWNERSHIP_SIGN && self.has_server() {
            let server = self.server.borrow().clone().unwrap();
            // `playerIsAdminWithID:nil` is NO
            if !client_id.map_or(false, |c| server.player_is_admin_with_id(c)) {
                return; // only admins
            }
            if let (Some(pu), Some(c)) = (server.player_update(), client_id) {
                if pu == c {
                    return;
                }
            }
        }
        self.net_update_dynamic_objects.borrow_mut()[t].extend(update_array);
    }

    /// `-remoteRemove:forObjectsOfType:fromClient:`: permission-filters removal requests.
    pub fn remote_remove_for_objects_of_type_from_client(
        &self,
        remove_array: Vec<NetBlob>,
        object_type: i32,
        client_id: Option<&str>,
    ) {
        let Some(t) = valid_type(object_type) else { return }; // UB-PARITY
        let hooks = self.hooks();
        let mut modified: Option<Vec<NetBlob>> = None;
        if self.has_server() {
            let server = self.server.borrow().clone().unwrap();
            if object_type == DYNAMIC_OBJECT_TYPE_OWNERSHIP_SIGN {
                if !client_id.map_or(false, |c| server.player_is_admin_with_id(c)) {
                    return;
                }
            } else {
                if object_type == DYNAMIC_OBJECT_TYPE_FREE_BLOCK
                    || object_type == DYNAMIC_OBJECT_TYPE_WINDOW
                    || object_type_is_npc(object_type)
                {
                    return; // clients can never remove these
                }
                if let (Some(pu), Some(c)) = (server.player_update(), client_id) {
                    if pu == c {
                        return;
                    }
                }
            }
            if !client_id.map_or(false, |c| server.player_is_admin_with_id(c)) {
                let mut m: Vec<NetBlob> = Vec::new();
                for nd in remove_array.iter() {
                    let Some(id) = nd_unique_id(nd) else { continue };
                    if object_type == DYNAMIC_OBJECT_TYPE_BLOCKHEAD {
                        for bh in self.net_blockheads.borrow().iter() {
                            let cid = hooks.blockhead_client_id(bh);
                            if bh.unique_id() == id && cid.is_some() && cid.as_deref() == client_id {
                                m.push(nd.clone());
                            }
                        }
                    } else if let Some(o) = self.do_get(t, id) {
                        let owner = o.owner_id();
                        if o.may_only_be_removed_by_owner() && !(owner.is_some() && owner.as_deref() == client_id) {
                            // skip
                        } else if object_type == DYNAMIC_OBJECT_TYPE_DOOR
                            && hooks.item_type(&o) == ItemType::ITEM_GATE.0 as i32
                        {
                            // skip
                        } else {
                            m.push(nd.clone());
                        }
                    }
                }
                modified = Some(m);
            }
        }
        self.net_remove_dynamic_objects.borrow_mut()[t].extend(modified.unwrap_or(remove_array));
    }

    /// `-updateNetObjects`: per-tick drain of the four net queues. No-op unless `server != nil`.
    pub fn update_net_objects(&self) {
        if !self.has_server() {
            return;
        }
        let hooks = self.hooks();
        let world = self.world();
        for i in 0..MAX_DYNAMIC_OBJECT_TYPES {
            let ti = i as i32;
            // ---- creations ----
            let client_ids: Vec<String> = self.net_create_dynamic_objects.borrow()[i].keys_in_order();
            for client_id in client_ids {
                let arr: Vec<NetBlob> = match self.net_create_dynamic_objects.borrow()[i].get(&client_id) {
                    Some(a) => a.clone(),
                    None => continue,
                };
                if arr.is_empty() {
                    continue;
                }
                let mut remove_idx: BTreeSet<usize> = BTreeSet::new();
                let mut index = 0usize;
                for net in arr.iter() {
                    self.process_net_creation(i, ti, &client_id, net, index, &mut remove_idx, &hooks);
                    index += 1;
                }
                // removeObjectsAtIndexes on the live array
                let mut q = self.net_create_dynamic_objects.borrow_mut();
                if let Some(live) = q[i].get_mut(&client_id) {
                    let mut k = 0usize;
                    live.retain(|_| {
                        let keep = !remove_idx.contains(&k);
                        k += 1;
                        keep
                    });
                }
            }
            // ---- creation data updates ----
            let list: Vec<NetBlob> = self.net_update_creation_data_dynamic_objects.borrow()[i].clone();
            for net in list.iter() {
                let Some(uid) = nd_unique_id(net) else { continue };
                if ti == DYNAMIC_OBJECT_TYPE_BLOCKHEAD {
                    for bh in self.net_blockheads.borrow().clone() {
                        if bh.unique_id() == uid {
                            bh.remote_creation_data_update(&bh, net);
                        }
                    }
                } else if let Some(o) = self.do_get(i, uid) {
                    o.remote_creation_data_update(&o, net);
                }
            }
            self.net_update_creation_data_dynamic_objects.borrow_mut()[i].clear();
            // ---- updates ----
            let list: Vec<NetBlob> = self.net_update_dynamic_objects.borrow()[i].clone();
            for net in list.iter() {
                let Some(uid) = nd_unique_id(net) else { continue };
                if ti == DYNAMIC_OBJECT_TYPE_BLOCKHEAD {
                    for bh in self.net_blockheads.borrow().clone() {
                        if bh.unique_id() == uid {
                            bh.remote_update(&bh, net);
                        }
                    }
                } else if let Some(o) = self.do_get(i, uid) {
                    o.remote_update(&o, net);
                }
            }
            self.net_update_dynamic_objects.borrow_mut()[i].clear();
            // ---- removals ----
            let list: Vec<NetBlob> = self.net_remove_dynamic_objects.borrow()[i].clone();
            for net in list.iter() {
                let Some(uid) = nd_unique_id(net) else { continue };
                if ti == DYNAMIC_OBJECT_TYPE_BLOCKHEAD {
                    for bh in self.net_blockheads.borrow().clone() {
                        if bh.unique_id() == uid {
                            bh.set_needs_removed(&bh, true);
                        }
                    }
                } else if let Some(o) = self.do_get(i, uid) {
                    o.set_needs_removed(&o, true);
                }
            }
            self.net_remove_dynamic_objects.borrow_mut()[i].clear();
        }
        let _ = world;
    }

    /// One element of the creation queue of `updateNetObjects`. `index` is the position in the queue; the caller
    /// increments it once per element on every path.
    #[allow(clippy::too_many_arguments)]
    fn process_net_creation(
        &self,
        i: usize,
        ti: i32,
        client_id: &str,
        net: &NetBlob,
        index: usize,
        remove_idx: &mut BTreeSet<usize>,
        hooks: &std::rc::Rc<dyn super::state::DwHooks>,
    ) {
        // `getBytes:&nd length:0x18`; short blobs: UB-PARITY, consumed
        if net.len() < 0x18 {
            remove_idx.insert(index);
            return;
        }
        let uid = nd_unique_id(net).unwrap();
        let needs_removed = net[0x10] != 0;
        if needs_removed {
            remove_idx.insert(index);
            return;
        }
        let pending_removal = |uid: u64| -> bool {
            self.net_remove_dynamic_objects.borrow()[i].iter().any(|r| nd_unique_id(r) == Some(uid))
        };
        if pending_removal(uid) {
            remove_idx.insert(index);
            return;
        }
        if self.do_contains(i, uid) || self.ta_contains(i, uid) {
            remove_idx.insert(index);
            return;
        }
        if ti == DYNAMIC_OBJECT_TYPE_BLOCKHEAD {
            if hooks.blockhead_with_id_including_net(uid).is_some() {
                remove_idx.insert(index);
                return;
            }
            let mut bh: Option<ObjRef> = hooks.blockhead_from_net_data(net);
            // cid captured BEFORE setClientID
            let cid: Option<String> = bh.as_ref().and_then(|b| hooks.blockhead_client_id(b));
            if let (Some(c), Some(b)) = (&cid, &bh) {
                if *c == hooks.local_net_id() {
                    nslog!("trying to load dupe blockhead");
                    b.remove_from_macro_block(b);
                    bh = None;
                    remove_idx.insert(index);
                }
            }
            match bh {
                None => {
                    // an init-nil element stays queued unless a pending removal for the same id exists
                    if pending_removal(uid) {
                        remove_idx.insert(index);
                    }
                }
                Some(bh) => {
                    hooks.blockhead_set_client_id(&bh, client_id);
                    let use_id: &str = cid.as_deref().unwrap_or(client_id);
                    let alias = self
                        .server
                        .borrow()
                        .clone()
                        .and_then(|s| s.player_info_for_peer_id(use_id))
                        .and_then(|d| d.object_for_key("alias").and_then(|v| v.as_str().map(|s| s.to_string())));
                    if let Some(a) = alias {
                        hooks.blockhead_set_client_name(&bh, Some(&a));
                    }
                    self.net_blockheads.borrow_mut().push(bh.clone());
                    remove_idx.insert(index);
                    let light = self.server_client(client_id).map_or(0, |sc| sc.light_block_index());
                    hooks.fully_load_if_needed_around_pos(bh.pos(), light as u32, &bh);
                    self.send_net_data_if_needed_for_object_is_creation(&bh, true);
                }
            }
        } else if ti == DYNAMIC_OBJECT_TYPE_FREE_BLOCK {
            // fb == nil: `[client waitingForBlocks]` is NO on the server -> element dropped
            if let Some(fb) = hooks.construct_from_net_data(ti, None, net) {
                self.do_set(i, fb.unique_id(), fb.clone());
                let key = self.world_index_key(fb.pos());
                self.free_blocks_insert(key, &fb);
            }
            remove_idx.insert(index);
        } else if !object_type_is_interaction_object(ti) {
            if let Some(o) = hooks.construct_from_net_data(ti, None, net) {
                self.do_set(i, o.unique_id(), o.clone());
                if object_type_has_static_position(ti) {
                    let key = self.world_index_key(o.pos());
                    self.bp_set(i, key, o);
                }
            }
            remove_idx.insert(index);
        } else {
            // interaction object: InteractionObjectCreationNetData (0x28): interactionObjectType u16 @0x20
            if net.len() < 0x28 {
                remove_idx.insert(index);
                return;
            }
            let ito = u16::from_le_bytes([net[0x20], net[0x21]]) as i32;
            if let Some(o) = hooks.construct_from_net_data(ti, Some(ito), net) {
                self.do_set(i, o.unique_id(), o.clone());
                let key = self.world_index_key(o.pos());
                self.bp_set(i, key, o.clone()); // unconditionally
                if ito == 1 {
                    let wt = hooks.workbench_type(&o);
                    if wt != 1 {
                        self.workbench_has_been_crafted.set(true);
                    }
                    if wt == 1 || wt == 0xd {
                        self.portal_positions.borrow_mut().add_index(key);
                    }
                }
            }
            remove_idx.insert(index);
        }
    }

    /// `-sendNetDataIfNeededForObject:isCreation:`: server per-client replication of one object.
    ///  The three "needs" flags are cleared on every path that reaches the end.
    pub fn send_net_data_if_needed_for_object_is_creation(&self, o: &ObjRef, is_creation: bool) {
        if !is_creation && !o.needs_net_data_to_be_sent() && !o.creation_data_needs_to_be_sent() {
            return; // WITHOUT clearing flags
        }
        // all four evaluated, no short-circuit side effects
        let a = o.update_needs_to_be_sent();
        let b = o.creation_data_needs_to_be_sent();
        let c = o.needs_removed();
        let reliable = a | b | c | is_creation;
        if !reliable && self.send_unreliable_counter.get() != 0 {
            return; // WITHOUT clearing flags
        }
        let ot = o.object_type() as u8;
        if self.has_server() && self.server_client_count() != 0 {
            let world = self.world();
            let macro_pos = macro_pos_for_world_pos(o.pos(), &world);
            let macro_id = macro_pos.y.wrapping_mul(world.world_width_macro()).wrapping_add(macro_pos.x);
            let uid = o.unique_id();
            for sc in self.server_client_values() {
                if !sc.connected() {
                    continue;
                }
                if o.needs_removed() {
                    let rm: NetBlob = uid.to_le_bytes().to_vec();
                    sc.add_removal_object_data_to_send_of_type_object_id(&rm, ot, uid);
                    sc.unwire_dynamic_object(uid);
                } else if !sc.dynamic_object_is_wired(uid) {
                    if sc.block_is_wired(macro_id) {
                        // wire BEFORE the data: an object that returns None stays wired
                        sc.wire_dynamic_object(uid);
                        let cid = sc.client_id();
                        if let Some(cd) = o.creation_net_data_for_client(o, Some(&cid)) {
                            sc.add_creation_object_data_to_send_of_type_object_id(&cd, ot, uid);
                            if let Some(ud) = o.update_net_data_for_client(o, Some(&cid)) {
                                sc.add_update_object_data_to_send_of_type_reliable(&ud, ot, true);
                            }
                        }
                    }
                } else {
                    let cid = sc.client_id();
                    if o.creation_data_needs_to_be_sent() {
                        if let Some(cd) = o.creation_net_data_for_client(o, Some(&cid)) {
                            sc.add_creation_data_update_object_data_to_send_of_type(&cd, ot);
                        }
                    }
                    if o.update_needs_to_be_sent() || o.unreliable_update_needs_to_be_sent() {
                        if let Some(ud) = o.update_net_data_for_client(o, Some(&cid)) {
                            sc.add_update_object_data_to_send_of_type_reliable(&ud, ot, reliable);
                            if !sc.block_is_wired(macro_id) {
                                sc.unwire_dynamic_object(uid);
                            }
                        }
                    }
                }
            }
        }
        o.set_update_needs_to_be_sent(false);
        o.set_unreliable_update_needs_to_be_sent(false);
        o.set_creation_data_needs_to_be_sent(false);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dynworld::state::testutil::*;
    use crate::dynworld::state::{DwHooks, DwServer, DwServerClient, ServerClientMap};
    use crate::foundation::gsdict::GsDict;
    use crate::plist::value::PlistValue;
    use std::cell::{Cell, RefCell};
    use std::collections::HashSet;
    use std::rc::Rc;

    struct Srv {
        admins: Vec<String>,
        pending: RefCell<Option<String>>,
    }
    impl DwServer for Srv {
        fn player_info_for_peer_id(&self, _p: &str) -> Option<PlistValue> {
            None
        }
        fn player_is_admin_with_id(&self, c: &str) -> bool {
            self.admins.iter().any(|a| a == c)
        }
        fn player_update(&self) -> Option<String> {
            self.pending.borrow().clone()
        }
        fn notify_players_changed(&self) {}
    }

    struct Sc {
        id: String,
        wired_block: Cell<bool>,
        wired: RefCell<HashSet<u64>>,
        log: Rc<RefCell<Vec<String>>>,
        tamed: RefCell<Vec<HashSet<u32>>>,
    }
    impl DwServerClient for Sc {
        fn connected(&self) -> bool {
            true
        }
        fn client_id(&self) -> String {
            self.id.clone()
        }
        fn light_block_index(&self) -> i32 {
            0
        }
        fn block_is_wired(&self, _m: i32) -> bool {
            self.wired_block.get()
        }
        fn dynamic_object_is_wired(&self, u: u64) -> bool {
            self.wired.borrow().contains(&u)
        }
        fn wire_dynamic_object(&self, u: u64) {
            self.log.borrow_mut().push(format!("wire({u})"));
            self.wired.borrow_mut().insert(u);
        }
        fn unwire_dynamic_object(&self, u: u64) {
            self.log.borrow_mut().push(format!("unwire({u})"));
            self.wired.borrow_mut().remove(&u);
        }
        fn add_creation_object_data_to_send_of_type_object_id(&self, _d: &NetBlob, t: u8, id: u64) {
            self.log.borrow_mut().push(format!("create({t},{id})"));
        }
        fn add_creation_data_update_object_data_to_send_of_type(&self, _d: &NetBlob, t: u8) {
            self.log.borrow_mut().push(format!("cdu({t})"));
        }
        fn add_update_object_data_to_send_of_type_reliable(&self, _d: &NetBlob, t: u8, r: bool) {
            self.log.borrow_mut().push(format!("update({t},{r})"));
        }
        fn add_removal_object_data_to_send_of_type_object_id(&self, _d: &NetBlob, t: u8, id: u64) {
            self.log.borrow_mut().push(format!("remove({t},{id})"));
        }
        fn send_all_object_data(&self) {
            self.log.borrow_mut().push("sendAll".into());
        }
        fn world_indices_containing_tamed_animals(&self) -> &RefCell<Vec<HashSet<u32>>> {
            &self.tamed
        }
    }

    fn setup(admins: &[&str]) -> (Rc<crate::dynworld::state::DynamicWorld>, Rc<Sc>, Rc<Srv>, Rc<RefCell<Vec<String>>>) {
        let d = dw("net", 1);
        let log = Rc::new(RefCell::new(vec![]));
        let sc = Rc::new(Sc {
            id: "c1".into(),
            wired_block: Cell::new(true),
            wired: RefCell::new(HashSet::new()),
            log: log.clone(),
            tamed: RefCell::new(vec![HashSet::new(); 65]),
        });
        let srv = Rc::new(Srv { admins: admins.iter().map(|s| s.to_string()).collect(), pending: RefCell::new(None) });
        let mut m: GsDict<String, Rc<dyn DwServerClient>> = GsDict::new();
        m.insert("c1".to_string(), sc.clone());
        let map: ServerClientMap = Rc::new(RefCell::new(m));
        d.set_server_server_clients(Some(srv.clone()), Some(map));
        (d, sc, srv, log)
    }

    /// contract 8 item 10: wire before data, exactly once; flags cleared; removal unwires.
    #[test]
    fn fanout_wiring() {
        let (d, sc, _srv, log) = setup(&[]);
        let o = Obj::new(13, 5, log.clone());
        let oref: ObjRef = o.clone();
        d.send_net_data_if_needed_for_object_is_creation(&oref, true);
        // default creation_net_data_for_client returns None: wired, no data
        assert_eq!(&*log.borrow(), &vec!["wire(5)".to_string()]);
        assert!(sc.wired.borrow().contains(&5));
        // second call: already wired, nothing to send -> early out without clearing
        o.base.borrow_mut().update_needs_to_be_sent = false;
        d.send_net_data_if_needed_for_object_is_creation(&oref, false);
        assert_eq!(log.borrow().len(), 1);
        // removal
        o.base.borrow_mut().needs_removed = true;
        d.send_net_data_if_needed_for_object_is_creation(&oref, false);
        assert_eq!(log.borrow()[1..].to_vec(), vec!["remove(13,5)".to_string(), "unwire(5)".to_string()]);
    }

    /// not wired + block not wired: nothing, flags still cleared.
    #[test]
    fn block_not_wired_clears_flags() {
        let (d, sc, _srv, log) = setup(&[]);
        sc.wired_block.set(false);
        let o = Obj::new(13, 5, log.clone());
        o.base.borrow_mut().update_needs_to_be_sent = true;
        let oref: ObjRef = o.clone();
        d.send_net_data_if_needed_for_object_is_creation(&oref, false);
        assert!(log.borrow().is_empty());
        assert!(!o.base.borrow().update_needs_to_be_sent);
    }

    /// unreliable gating: not reliable and counter != 0 returns without clearing flags.
    #[test]
    fn unreliable_gate() {
        let (d, _sc, _srv, log) = setup(&[]);
        let o = Obj::new(13, 5, log.clone());
        o.base.borrow_mut().unreliable_update_needs_to_be_sent = true;
        let oref: ObjRef = o.clone();
        d.send_unreliable_counter.set(3);
        d.send_net_data_if_needed_for_object_is_creation(&oref, false);
        assert!(o.base.borrow().unreliable_update_needs_to_be_sent);
        d.send_unreliable_counter.set(0);
        d.send_net_data_if_needed_for_object_is_creation(&oref, false);
        assert!(!o.base.borrow().unreliable_update_needs_to_be_sent);
    }

    /// contract 8 item 9a: FREE_BLOCK create is ignored on the server.
    #[test]
    fn remote_create_free_block_ignored() {
        let (d, _sc, _srv, _log) = setup(&[]);
        d.remote_create_for_objects_of_type_client_id(vec![vec![0; 0x18]], 14, "c1");
        assert!(d.net_create_dynamic_objects.borrow()[14].is_empty());
        d.remote_create_for_objects_of_type_client_id(vec![vec![0; 0x18]], 13, "c1");
        assert_eq!(d.net_create_dynamic_objects.borrow()[13].len(), 1);
        // out of range ignored, no panic
        d.remote_create_for_objects_of_type_client_id(vec![vec![0; 0x18]], 200, "c1");
    }

    /// contract 8 item 9b: removal filters.
    #[test]
    fn remote_remove_filters() {
        let (d, _sc, _srv, log) = setup(&["admin"]);
        let id = |n: u64| n.to_le_bytes().to_vec();
        // NPC / window / free block are dropped for everyone
        d.remote_remove_for_objects_of_type_from_client(vec![id(1)], 13, Some("admin"));
        d.remote_remove_for_objects_of_type_from_client(vec![id(1)], 31, Some("admin"));
        d.remote_remove_for_objects_of_type_from_client(vec![id(1)], 14, Some("admin"));
        assert!(d.net_remove_dynamic_objects.borrow().iter().all(|v| v.is_empty()));
        // a non-admin only removes ids that exist in dynamic_objects
        let o: ObjRef = Obj::new(20, 7, log.clone());
        d.do_set(20, 7, o);
        d.remote_remove_for_objects_of_type_from_client(vec![id(7), id(8)], 20, Some("bob"));
        assert_eq!(d.net_remove_dynamic_objects.borrow()[20], vec![id(7)]);
        // admin passes everything
        d.remote_remove_for_objects_of_type_from_client(vec![id(9)], 20, Some("admin"));
        assert_eq!(d.net_remove_dynamic_objects.borrow()[20].len(), 2);
        // anti-cheat flagged client is ignored for non-sign types
        *_srv.pending.borrow_mut() = Some("bob".into());
        d.remote_remove_for_objects_of_type_from_client(vec![id(7)], 20, Some("bob"));
        assert_eq!(d.net_remove_dynamic_objects.borrow()[20].len(), 2);
    }

    /// contract 8 item 9c: ownership sign updates need admin.
    #[test]
    fn remote_update_ownership_sign() {
        let (d, _sc, _srv, _log) = setup(&["admin"]);
        d.remote_update_for_objects_of_type_from_client(vec![vec![1; 0x18]], 60, Some("bob"));
        assert!(d.net_update_dynamic_objects.borrow()[60].is_empty());
        d.remote_update_for_objects_of_type_from_client(vec![vec![1; 0x18]], 60, Some("admin"));
        assert_eq!(d.net_update_dynamic_objects.borrow()[60].len(), 1);
        d.remote_update_for_objects_of_type_from_client(vec![vec![1; 0x18]], 13, Some("bob"));
        assert_eq!(d.net_update_dynamic_objects.borrow()[13].len(), 1);
    }

    struct Hk {
        made: RefCell<Vec<(i32, Option<i32>)>>,
        log: Rc<RefCell<Vec<String>>>,
    }
    impl crate::dynworld::state::DwHooksStub for Hk {
        fn construct_from_net_data(&self, t: i32, ito: Option<i32>, net: &[u8]) -> Option<ObjRef> {
            self.made.borrow_mut().push((t, ito));
            let id = u64::from_le_bytes(net[0..8].try_into().unwrap());
            let o: ObjRef = Obj::new(t, id, self.log.clone());
            Some(o)
        }
    }

    /// creation queue: needsRemoved / pending removal / already existing elements are consumed, others created.
    #[test]
    fn update_net_objects_creation() {
        let (d, _sc, _srv, log) = setup(&[]);
        let hk = Rc::new(Hk { made: RefCell::new(vec![]), log: log.clone() });
        d.set_hooks(hk.clone());
        let blob = |id: u64, removed: bool| {
            let mut b = vec![0u8; 0x18];
            b[0..8].copy_from_slice(&id.to_le_bytes());
            b[0x10] = removed as u8;
            b
        };
        d.remote_create_for_objects_of_type_client_id(
            vec![blob(1, false), blob(2, true), blob(3, false), blob(4, false)],
            13,
            "c1",
        );
        let existing: ObjRef = Obj::new(13, 4, log.clone());
        d.do_set(13, 4, existing);
        d.net_remove_dynamic_objects.borrow_mut()[13].push(3u64.to_le_bytes().to_vec());
        d.update_net_objects();
        assert!(d.do_contains(13, 1));
        assert!(!d.do_contains(13, 2));
        assert!(!d.do_contains(13, 3));
        assert!(d.net_create_dynamic_objects.borrow()[13].get(&"c1".to_string()).unwrap().is_empty());
        // the removal queue was applied and emptied
        assert!(d.net_remove_dynamic_objects.borrow()[13].is_empty());
    }
}
