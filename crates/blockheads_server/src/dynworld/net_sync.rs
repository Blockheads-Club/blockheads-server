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
                let arr: Vec<NetBlob> =
                    self.net_create_dynamic_objects.borrow()[i].get(&client_id).cloned().unwrap_or_default();
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
                let live = q[i].get_mut(&client_id).expect("the client's queue is only ever drained here");
                let mut k = 0usize;
                live.retain(|_| {
                    let keep = !remove_idx.contains(&k);
                    k += 1;
                    keep
                });
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
                // an init-nil element stays queued (a pending removal for its id was handled above)
                None => {}
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

/// Tests of the ingestion queues: permission filters, per-type updates, every creation path and the per-client
/// replication of one object.
#[cfg(test)]
mod tests {
    use super::*;
    use crate::dynworld::state::testutil::*;
    use crate::dynworld::state::{DwServer, DynamicWorldInit, ServerClientMap};
    use crate::foundation::gsdict::GsDict;
    use crate::plist::value::PlistValue;
    use crate::types::structs::IntPair;
    use std::cell::RefCell;
    use std::rc::Rc;

    fn blob(id: u64, len: usize) -> NetBlob {
        let mut b = vec![0u8; len];
        let n = len.min(8);
        b[..n].copy_from_slice(&id.to_le_bytes()[..n]);
        b
    }

    fn id_blob(id: u64) -> NetBlob {
        id.to_le_bytes().to_vec()
    }

    /// A server whose only admin is `admin` and whose anti-cheat may have flagged one player.
    struct Flagged {
        pending: RefCell<Option<String>>,
    }
    impl DwServer for Flagged {
        fn player_info_for_peer_id(&self, p: &str) -> Option<PlistValue> {
            (p == "ab1234").then(|| plist_dict(vec![("alias", PlistValue::string("Alice"))]))
        }
        fn player_is_admin_with_id(&self, c: &str) -> bool {
            c == "admin"
        }
        fn player_update(&self) -> Option<String> {
            self.pending.borrow().clone()
        }
        fn notify_players_changed(&self) {}
    }

    struct Fx {
        d: Rc<DynamicWorld>,
        sc: Rc<Sc>,
        srv: Rc<Flagged>,
        log: Log,
        h: Rc<Hk>,
    }

    fn fx(name: &str) -> Fx {
        let (main, dwdb) = dbs(name);
        let w = world(1);
        std::mem::forget(w.clone());
        let sc = Rc::new(Sc::new("ab1234"));
        let map: ServerClientMap = Rc::new(RefCell::new(GsDict::new()));
        map.borrow_mut().insert("ab1234".to_string(), sc.clone());
        let srv = Rc::new(Flagged { pending: RefCell::new(None) });
        let d = DynamicWorld::new(DynamicWorldInit {
            world: w,
            world_tile_loader: None,
            server: Some(srv.clone()),
            server_clients: Some(map),
            cache: None,
            tree_density_noise_function: None,
            season_offset_noise_function: None,
            app_database: None,
            world_database: main,
            dynamic_object_database: dwdb,
            world_save_directory_override: Some(scratch(&format!("{name}_save"))),
        });
        let log: Log = Rc::new(RefCell::new(vec![]));
        let h = hk(&log);
        d.set_hooks(h.clone());
        Fx { d, sc, srv, log, h }
    }

    /// Wire before data, exactly once; flags cleared; removal unwires.
    #[test]
    fn fanout_wiring() {
        let f = fx("ns_wire");
        f.sc.wired_block.set(true);
        let o = tobj(13, 5, &f.log, None, None, None);
        f.d.send_net_data_if_needed_for_object_is_creation(&o, true);
        // no data to send: wired, nothing else
        assert_eq!(*f.sc.events.borrow(), vec!["wire(5)".to_string()]);
        // already wired and nothing to send: early out without clearing
        f.d.send_net_data_if_needed_for_object_is_creation(&o, false);
        assert_eq!(f.sc.events.borrow().len(), 1);
        o.set_needs_removed(&o, true);
        f.d.send_net_data_if_needed_for_object_is_creation(&o, false);
        assert_eq!(f.sc.events.borrow()[1..].to_vec(), vec!["remove(13,5)".to_string(), "unwire(5)".to_string()]);
    }

    #[test]
    fn block_not_wired_clears_flags() {
        let f = fx("ns_nowire");
        let o = tobj(13, 5, &f.log, None, None, None);
        o.set_update_needs_to_be_sent(true);
        f.d.send_net_data_if_needed_for_object_is_creation(&o, false);
        assert!(f.sc.events.borrow().is_empty());
        assert!(!o.update_needs_to_be_sent());
    }

    /// Not reliable and the counter is not zero: returns without clearing flags.
    #[test]
    fn unreliable_gate() {
        let f = fx("ns_unrel");
        let o = tobj(13, 5, &f.log, None, None, None);
        o.set_unreliable_update_needs_to_be_sent(true);
        f.d.send_unreliable_counter.set(3);
        f.d.send_net_data_if_needed_for_object_is_creation(&o, false);
        assert!(o.unreliable_update_needs_to_be_sent());
        f.d.send_unreliable_counter.set(0);
        f.d.send_net_data_if_needed_for_object_is_creation(&o, false);
        assert!(!o.unreliable_update_needs_to_be_sent());
    }

    /// Data goes out as creation, creation-data update and update, depending on the wiring.
    #[test]
    fn per_client_replication() {
        let f = fx("ns_data");
        let sc = &f.sc;
        sc.wired_block.set(true);
        let o = tobj(13, 5, &f.log, Some(b"create"), Some(b"update"), None);
        f.d.send_net_data_if_needed_for_object_is_creation(&o, true);
        assert_eq!(
            *sc.events.borrow(),
            vec!["wire(5)".to_string(), "create(13,5)".to_string(), "update(13,true)".to_string()]
        );
        // wired: creation data changes and updates are sent separately
        sc.events.borrow_mut().clear();
        o.set_creation_data_needs_to_be_sent(true);
        o.set_update_needs_to_be_sent(true);
        f.d.send_net_data_if_needed_for_object_is_creation(&o, false);
        assert_eq!(*sc.events.borrow(), vec!["cdu(13)".to_string(), "update(13,true)".to_string()]);
        // the block went away from the client: the object is unwired after the update
        sc.events.borrow_mut().clear();
        sc.wired_block.set(false);
        o.set_update_needs_to_be_sent(true);
        f.d.send_net_data_if_needed_for_object_is_creation(&o, false);
        assert_eq!(*sc.events.borrow(), vec!["update(13,true)".to_string(), "unwire(5)".to_string()]);
        // an object without data stays wired and silent
        let quiet = tobj(13, 6, &f.log, None, None, None);
        sc.wired.borrow_mut().push(6);
        sc.events.borrow_mut().clear();
        quiet.set_creation_data_needs_to_be_sent(true);
        quiet.set_update_needs_to_be_sent(true);
        f.d.send_net_data_if_needed_for_object_is_creation(&quiet, false);
        assert!(sc.events.borrow().is_empty());
        // creation data but no update data
        let half = tobj(13, 7, &f.log, Some(b"create"), None, None);
        sc.wired_block.set(true);
        f.d.send_net_data_if_needed_for_object_is_creation(&half, true);
        assert_eq!(sc.events.borrow().len(), 2);
        // wired, with creation data only
        let c = tobj(13, 8, &f.log, Some(b"create"), Some(b"update"), None);
        sc.wired.borrow_mut().push(8);
        sc.events.borrow_mut().clear();
        c.set_creation_data_needs_to_be_sent(true);
        f.d.send_net_data_if_needed_for_object_is_creation(&c, false);
        assert_eq!(*sc.events.borrow(), vec!["cdu(13)".to_string()]);
        // clients that are not connected get nothing
        sc.online.set(false);
        sc.events.borrow_mut().clear();
        f.d.send_net_data_if_needed_for_object_is_creation(&c, true);
        assert!(sc.events.borrow().is_empty());
        sc.online.set(true);
        // a world tick sends everything pending
        f.d.update_accurate_dt(0.1, 0.1);
        assert_eq!(sc.events.borrow().last().unwrap(), "sendAll");
        f.srv.notify_players_changed();
    }

    #[test]
    fn remote_create_free_block_ignored() {
        let f = fx("ns_free");
        f.d.remote_create_for_objects_of_type_client_id(vec![vec![0; 0x18]], 14, "c1");
        assert!(f.d.net_create_dynamic_objects.borrow()[14].is_empty());
        f.d.remote_create_for_objects_of_type_client_id(vec![vec![0; 0x18]], 13, "c1");
        f.d.remote_create_for_objects_of_type_client_id(vec![vec![0; 0x18]], 13, "c1");
        assert_eq!(f.d.net_create_dynamic_objects.borrow()[13].get(&"c1".to_string()).unwrap().len(), 2);
        f.d.remote_create_for_objects_of_type_client_id(vec![vec![0; 0x18]], 200, "c1");
    }

    #[test]
    fn updates_from_clients_are_filtered() {
        let f = fx("ns_upd");
        let d = &f.d;
        // out-of-range types are dropped by every entry point
        d.remote_creation_data_update_for_objects_of_type_from_client(vec![id_blob(1)], 99, None);
        d.remote_update_for_objects_of_type_from_client(vec![id_blob(1)], -1, None);
        d.remote_remove_for_objects_of_type_from_client(vec![id_blob(1)], 65, None);
        // ownership signs: only admins, and not while flagged by the anti-cheat
        d.remote_update_for_objects_of_type_from_client(vec![id_blob(1)], 60, None);
        d.remote_update_for_objects_of_type_from_client(vec![id_blob(1)], 60, Some("bob"));
        assert!(d.net_update_dynamic_objects.borrow()[60].is_empty());
        d.remote_update_for_objects_of_type_from_client(vec![id_blob(1)], 60, Some("admin"));
        assert_eq!(d.net_update_dynamic_objects.borrow()[60].len(), 1);
        *f.srv.pending.borrow_mut() = Some("someone else".to_string());
        d.remote_update_for_objects_of_type_from_client(vec![id_blob(1)], 60, Some("admin"));
        assert_eq!(d.net_update_dynamic_objects.borrow()[60].len(), 2);
        *f.srv.pending.borrow_mut() = Some("admin".to_string());
        d.remote_update_for_objects_of_type_from_client(vec![id_blob(1)], 60, Some("admin"));
        assert_eq!(d.net_update_dynamic_objects.borrow()[60].len(), 2);
        // everything else, and creation data updates, have no checks
        d.remote_update_for_objects_of_type_from_client(vec![id_blob(1)], 13, Some("bob"));
        assert_eq!(d.net_update_dynamic_objects.borrow()[13].len(), 1);
        d.remote_creation_data_update_for_objects_of_type_from_client(vec![id_blob(1)], 13, Some("bob"));
        assert_eq!(d.net_update_creation_data_dynamic_objects.borrow()[13].len(), 1);
    }

    #[test]
    fn removals_from_clients_are_filtered() {
        let f = fx("ns_rem");
        let (d, log, h) = (&f.d, &f.log, &f.h);
        // NPCs, windows and free blocks are dropped for everyone
        for t in [13, 31, 14] {
            d.remote_remove_for_objects_of_type_from_client(vec![id_blob(1)], t, Some("admin"));
        }
        assert!(d.net_remove_dynamic_objects.borrow().iter().all(|v| v.is_empty()));
        // ownership signs: admins only, then everything they ask for is queued
        d.remote_remove_for_objects_of_type_from_client(vec![id_blob(1)], 60, Some("bob"));
        d.remote_remove_for_objects_of_type_from_client(vec![id_blob(1)], 60, None);
        assert!(d.net_remove_dynamic_objects.borrow()[60].is_empty());
        d.remote_remove_for_objects_of_type_from_client(vec![id_blob(1)], 60, Some("admin"));
        assert_eq!(d.net_remove_dynamic_objects.borrow()[60].len(), 1);
        // blockheads: only the ones the client owns
        d.net_blockheads.borrow_mut().push(tobj(24, 5, log, None, None, None));
        d.net_blockheads.borrow_mut().push(tobj(24, 6, log, None, None, None));
        h.client_of.borrow_mut().push((5, "bob".to_string()));
        h.client_of.borrow_mut().push((6, "eve".to_string()));
        d.remote_remove_for_objects_of_type_from_client(vec![id_blob(5), id_blob(6), vec![1, 2]], 24, Some("bob"));
        assert_eq!(d.net_remove_dynamic_objects.borrow()[24], vec![id_blob(5)]);
        // owned objects can only be removed by their owner; gates never
        let owned = tobj(46, 7, log, None, None, None);
        owned.set_owner_id(Some("alice".to_string()));
        d.do_set(46, 7, owned);
        d.do_set(46, 8, tobj(46, 8, log, None, None, None));
        d.remote_remove_for_objects_of_type_from_client(vec![id_blob(7), id_blob(8), id_blob(9)], 46, Some("bob"));
        assert_eq!(d.net_remove_dynamic_objects.borrow()[46], vec![id_blob(8)]);
        d.remote_remove_for_objects_of_type_from_client(vec![id_blob(7)], 46, Some("alice"));
        assert_eq!(d.net_remove_dynamic_objects.borrow()[46].len(), 2);
        d.do_set(20, 9, tobj(20, 9, log, None, None, None));
        h.item.set(crate::types::item_type::ItemType::ITEM_GATE.0 as i32);
        d.remote_remove_for_objects_of_type_from_client(vec![id_blob(9)], 20, Some("bob"));
        assert!(d.net_remove_dynamic_objects.borrow()[20].is_empty());
        h.item.set(0);
        d.remote_remove_for_objects_of_type_from_client(vec![id_blob(9)], 20, Some("bob"));
        assert_eq!(d.net_remove_dynamic_objects.borrow()[20].len(), 1);
        // an admin passes everything, a flagged client nothing
        d.remote_remove_for_objects_of_type_from_client(vec![id_blob(99)], 20, Some("admin"));
        assert_eq!(d.net_remove_dynamic_objects.borrow()[20].len(), 2);
        *f.srv.pending.borrow_mut() = Some("someone else".to_string());
        d.remote_remove_for_objects_of_type_from_client(vec![id_blob(9)], 20, Some("bob"));
        assert_eq!(d.net_remove_dynamic_objects.borrow()[20].len(), 3);
        *f.srv.pending.borrow_mut() = Some("bob".to_string());
        d.remote_remove_for_objects_of_type_from_client(vec![id_blob(9)], 20, Some("bob"));
        assert_eq!(d.net_remove_dynamic_objects.borrow()[20].len(), 3);
        // without a server nothing is checked
        let plain = dw("ns_plain_rm", 1);
        plain.remote_remove_for_objects_of_type_from_client(vec![id_blob(1)], 13, None);
        assert_eq!(plain.net_remove_dynamic_objects.borrow()[13].len(), 1);
    }

    #[test]
    fn queued_updates_reach_blockheads_and_objects() {
        let f = fx("ns_queues");
        let (d, log) = (&f.d, &f.log);
        let bh = tobj(24, 5, log, None, None, None);
        d.net_blockheads.borrow_mut().push(bh.clone());
        d.do_set(19, 6, tobj(19, 6, log, None, None, None));
        for t in [24, 19] {
            for id in [5u64, 6, 7] {
                d.remote_creation_data_update_for_objects_of_type_from_client(vec![blob(id, 0x18)], t, None);
                d.remote_update_for_objects_of_type_from_client(vec![blob(id, 0x18)], t, None);
                d.remote_remove_for_objects_of_type_from_client(vec![id_blob(id)], t, Some("admin"));
            }
            // blobs without an id are ignored
            d.remote_creation_data_update_for_objects_of_type_from_client(vec![vec![1]], t, None);
            d.remote_update_for_objects_of_type_from_client(vec![vec![1]], t, None);
            d.net_remove_dynamic_objects.borrow_mut()[t as usize].push(vec![1]);
        }
        d.update_net_objects();
        assert!(bh.needs_removed());
        assert!(d.do_get(19, 6).unwrap().needs_removed());
        assert!(d.net_update_dynamic_objects.borrow()[24].is_empty());
        // nothing happens without a server
        let plain = dw("ns_plain", 1);
        plain.update_net_objects();
    }

    #[test]
    fn creation_queue_paths() {
        let f = fx("ns_create");
        let (d, log, h) = (&f.d, &f.log, &f.h);
        // straight into the queue: free blocks are refused by the remote entry point
        let create = |t: i32, blobs: Vec<NetBlob>| {
            {
                let mut q = d.net_create_dynamic_objects.borrow_mut();
                let key = "ab1234".to_string();
                if q[t as usize].get(&key).is_none() {
                    q[t as usize].insert(key.clone(), Vec::new());
                }
                q[t as usize].get_mut(&key).unwrap().extend(blobs);
            }
            d.update_net_objects();
        };
        // an empty queue and a client without elements are skipped
        d.net_create_dynamic_objects.borrow_mut()[13].insert("nobody".to_string(), Vec::new());
        d.update_net_objects();
        // blockheads: short blob, removed flag, queued removal, already known, dupe of the local player, loaded
        d.net_blockheads.borrow_mut().push(tobj(24, 30, log, None, None, None));
        h.known_ids.borrow_mut().push(30);
        let mut removed = blob(11, 0x18);
        removed[0x10] = 1;
        d.net_remove_dynamic_objects.borrow_mut()[24].push(id_blob(12));
        h.client_of.borrow_mut().push((13, d.hooks().local_net_id()));
        create(24, vec![blob(10, 4), removed, blob(12, 0x18), blob(30, 0x18), blob(13, 0x18), blob(14, 0x18)]);
        let ids: Vec<u64> = d.net_blockheads.borrow().iter().map(|b| b.unique_id()).collect();
        assert_eq!(ids, vec![30, 14]);
        let l = log.borrow().clone();
        assert!(l.contains(&"client_id(14,ab1234)".to_string()), "{l:?}");
        // a blockhead the hooks cannot build stays queued unless it is being removed
        h.fail_blockhead.set(true);
        d.net_remove_dynamic_objects.borrow_mut()[24].push(id_blob(15));
        create(24, vec![blob(15, 0x18), blob(16, 0x18)]);
        assert_eq!(d.net_create_dynamic_objects.borrow()[24].get(&"ab1234".to_string()).unwrap().len(), 1);
        h.fail_blockhead.set(false);
        // a blockhead of a client with a known alias
        h.client_of.borrow_mut().push((17, "ab1234".to_string()));
        create(24, vec![blob(17, 0x18)]);
        assert!(log.borrow().contains(&"client_name(17,Some(\"Alice\"))".to_string()));
        // free blocks, plain objects with and without a static position, refused constructions
        create(14, vec![blob(20, 0x18)]);
        assert_eq!(d.free_blocks_by_position.borrow().len(), 1);
        create(16, vec![blob(21, 0x18)]);
        assert!(d.do_contains(16, 21) && d.bp_contains(16, d.world_index_key(IntPair { x: 3, y: 4 })));
        create(13, vec![blob(22, 0x18)]);
        assert!(d.do_contains(13, 22));
        // an object that exists already is not created again
        create(13, vec![blob(22, 0x18)]);
        assert_eq!(h.net_made.borrow().iter().filter(|m| m.0 == 13).count(), 1);
        h.net_none.set(true);
        create(14, vec![blob(23, 0x18)]);
        create(13, vec![blob(24, 0x18)]);
        create(46, vec![blob(25, 0x28)]);
        h.net_none.set(false);
        // interaction objects: too short, workbenches of the three kinds, other interaction objects
        let with_ito = |id: u64, ito: u16| {
            let mut b = blob(id, 0x28);
            b[0x20..0x22].copy_from_slice(&ito.to_le_bytes());
            b
        };
        create(46, vec![blob(26, 0x20)]);
        h.wb_type.set(1);
        create(45, vec![with_ito(27, 1)]);
        assert!(!d.workbench_has_been_crafted.get());
        h.wb_type.set(0xd);
        create(45, vec![with_ito(28, 1)]);
        h.wb_type.set(5);
        create(45, vec![with_ito(29, 1)]);
        assert!(d.workbench_has_been_crafted.get());
        create(46, vec![with_ito(31, 2)]);
        assert!(d.portal_positions.borrow().contains_index(d.world_index_key(IntPair { x: 3, y: 4 })));
        assert!(d.net_create_dynamic_objects.borrow()[45].get(&"ab1234".to_string()).unwrap().is_empty());
        assert!(!h.net_made.borrow().is_empty());
    }
}
