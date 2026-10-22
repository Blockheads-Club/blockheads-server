//! Per-client connect / disconnect, kick, the pickup-request protocol, `setPaused:` and
//! `isControllingBlockheadsForClientPlayer:`.
//!
//! `setPaused:`; `isControllingBlockheadsForClientPlayer:`.
//! `client` is always nil on the dedicated server, `server` is always set.

use super::lookups::{ops, TRAIN_CAR_DYNAMIC_OBJECT_TYPES};
use super::state::DynamicWorld;
use crate::objects::handle::ObjRef;
use crate::objects::type_tables::INTERACTION_OBJECT_DYNAMIC_OBJECT_TYPES;

/// `PICKUP_TYPES` (function-local static of `clientPickupRequest`): torch, egg, painting, ladder, door, column,
/// elevator motor, elevator shaft, stairs, wire, rail.
pub const PICKUP_TYPES: [i32; 11] = [17, 30, 52, 19, 20, 53, 55, 56, 54, 38, 40];

/// `BHNDT_SERVER_PICKUP_REPLY`.
pub const BHNDT_SERVER_PICKUP_REPLY: u8 = 0x13;

/// Build one pickup reply: 16-byte `PickupAcceptRejectHeader {u8 0x13; u8 accepted; u8 pad[6] = 0; u64 requester}`
/// followed by the ids (host-endian u64).
pub(crate) fn pickup_reply_bytes(accepted: bool, requester: u64, ids: &[u64]) -> Vec<u8> {
    let mut v = Vec::with_capacity(16 + ids.len() * 8);
    v.push(BHNDT_SERVER_PICKUP_REPLY);
    v.push(accepted as u8);
    v.extend_from_slice(&[0u8; 6]);
    v.extend_from_slice(&requester.to_le_bytes());
    for id in ids {
        v.extend_from_slice(&id.to_le_bytes());
    }
    v
}

impl DynamicWorld {
    /// Blockheads of `list` whose `clientID` equals `client_id` (snapshot).
    fn blockheads_of_client(&self, list: &std::cell::RefCell<Vec<ObjRef>>, client_id: &str) -> Vec<ObjRef> {
        let hooks = self.hooks();
        list.borrow().iter().filter(|b| hooks.blockhead_client_id(b).as_deref() == Some(client_id)).cloned().collect()
    }

    /// `-clientConnected:` (`DynamicWorld`'s, not `World`'s). No-op for a nil id. Step order is fixed:
    /// the blockheads are saved BEFORE `blockheadWillBeUnloaded:` / `removeFromMacroBlock`.
    pub fn client_connected(&self, client_id: Option<&str>) {
        let Some(cid) = client_id else { return };
        let hooks = self.hooks();
        if let Some(v) = self.disconnected_clients_save_dir_names.borrow_mut().as_mut() {
            v.retain(|s| s != cid);
        }
        let mut remove_list: Vec<ObjRef> = Vec::new();
        for bh in self.blockheads_of_client(&self.net_blockheads_with_disconnected_clients, cid) {
            hooks.blockhead_prepare_inventory_for_saving(&bh);
            if hooks.blockhead_inventory_needs_saving(&bh) {
                self.save_blockhead_inventory(&bh);
                hooks.blockhead_set_inventory_needs_saving(&bh, false);
            }
            bh.set_needs_removed(&bh, true);
            remove_list.push(bh);
        }
        if !remove_list.is_empty() {
            self.save_blockheads();
            for bh in &remove_list {
                self.blockhead_will_be_unloaded(bh);
                bh.remove_from_macro_block(bh);
                self.remove_from_list(&self.net_blockheads_with_disconnected_clients, bh);
            }
            if let Some(s) = self.server.borrow().clone() {
                s.notify_players_changed();
            }
        }
        let used = self.world().used_physical_blocks();
        for pb in used {
            if pb.valid.get() {
                self.load_client_owned_dynamic_objects_for_client_physical_block(cid, pb);
            }
        }
    }

    /// `-stopAllBlockheadActionsForClientDueToKick:`: `netBlockheads`, then the disconnected list.
    pub fn stop_all_blockhead_actions_for_client_due_to_kick(&self, client_id: &str) {
        let hooks = ops();
        for bh in self.blockheads_of_client(&self.net_blockheads, client_id) {
            hooks.blockhead_stop_all_actions(&bh);
        }
        for bh in self.blockheads_of_client(&self.net_blockheads_with_disconnected_clients, client_id) {
            hooks.blockhead_stop_all_actions(&bh);
        }
    }

    /// `-clientDisconnected:simulate:`. With `simulate` the blockheads stay and the id is recorded once in
    /// `disconnectedClientsSaveDirNames` (created on first use); `liveServerClientBlockheadInventories` loses the
    /// client unconditionally. No borrow is held across any call out.
    pub fn client_disconnected_simulate(&self, client_id: Option<&str>, simulate: bool) {
        let Some(cid) = client_id else { return };
        let hooks = self.hooks();
        for bh in self.blockheads_of_client(&self.net_blockheads, cid) {
            for t in INTERACTION_OBJECT_DYNAMIC_OBJECT_TYPES {
                for obj in self.dot_snapshot(t) {
                    if let Some(il) = obj.as_interaction_like() {
                        il.remote_blockhead_removed_with_id(&obj, bh.unique_id());
                    }
                }
            }
            if !simulate {
                bh.set_needs_removed(&bh, true);
            } else {
                let mut names = self.disconnected_clients_save_dir_names.borrow_mut();
                let v = names.get_or_insert_with(Vec::new);
                if !v.iter().any(|s| s == cid) {
                    v.push(cid.to_string());
                }
            }
        }
        if !simulate {
            self.save_blockheads();
            if let Some(v) = self.disconnected_clients_save_dir_names.borrow_mut().as_mut() {
                v.retain(|s| s != cid);
            }
            for bh in self.blockheads_of_client(&self.net_blockheads_with_disconnected_clients, cid) {
                hooks.blockhead_prepare_inventory_for_saving(&bh);
                if hooks.blockhead_inventory_needs_saving(&bh) {
                    self.save_blockhead_inventory(&bh);
                    hooks.blockhead_set_inventory_needs_saving(&bh, false);
                }
                bh.set_needs_removed(&bh, true);
            }
        }
        self.live_server_client_blockhead_inventories.borrow_mut().remove(&cid.to_string());
    }

    /// `-isControllingBlockheadsForClientPlayer:`.
    pub fn is_controlling_blockheads_for_client_player(&self, player_id: &str) -> bool {
        if let Some(v) = self.disconnected_clients_save_dir_names.borrow().as_ref() {
            if v.iter().any(|s| s == player_id) {
                return true;
            }
        }
        !self.blockheads_of_client(&self.net_blockheads_with_disconnected_clients, player_id).is_empty()
    }

    /// `-clientPickupRequest:count:clientID:blockheadRequesterUniqueID:`. Free block first, then the 11
    /// `PICKUP_TYPES` (first map containing the id wins, entry from `dynamicObjects` only). Ids repeated in one
    /// request are processed twice. Accepted message first, then rejected, each only if non-empty, reliable.
    /// `ids.len() == count`; a count <= 0 is an empty slice.
    pub fn client_pickup_request_count_client_id_blockhead_requester_unique_id(
        &self,
        ids: &[u64],
        client_id: &str,
        req_id: u64,
    ) {
        let mut accepted: Vec<u64> = Vec::new();
        let mut rejected: Vec<u64> = Vec::new();
        for &id in ids {
            if let Some(fb) = self.freeblock_with_unique_id(id) {
                if !fb.needs_removed() {
                    fb.set_needs_removed(&fb, true);
                    accepted.push(id);
                    continue;
                }
            }
            let mut found = false;
            for t in PICKUP_TYPES {
                if let Some(o) = self.dot_get(t, id) {
                    self.remove_standard_object(Some(&o));
                    found = true;
                    break;
                }
            }
            if found {
                accepted.push(id);
            } else {
                rejected.push(id);
            }
        }
        if !self.has_server() {
            return; // message to nil
        }
        let peers = vec![client_id.to_string()];
        if !accepted.is_empty() {
            ops().server_send_network_data_to_peers_reliable(pickup_reply_bytes(true, req_id, &accepted), &peers, true);
        }
        if !rejected.is_empty() {
            ops().server_send_network_data_to_peers_reliable(
                pickup_reply_bytes(false, req_id, &rejected),
                &peers,
                true,
            );
        }
    }

    /// `-remotePickupRequestReply:` (client-side handler, never reached on the server). A reply shorter
    /// than the 16-byte header is ignored (Synthesis resolutions).
    pub fn remote_pickup_request_reply(&self, raw: &[u8]) {
        if raw.len() < 16 {
            return;
        }
        let accepted = raw[1] != 0;
        let uid = u64::from_le_bytes(raw[8..16].try_into().unwrap());
        let ids: Vec<u64> = raw[16..].chunks_exact(8).map(|c| u64::from_le_bytes(c.try_into().unwrap())).collect();
        if let Some(bh) = self.blockhead_with_unique_id(uid) {
            ops().blockhead_remote_pickup_request_response(&bh, accepted, &ids);
        }
    }

    /// `-setPaused:`: local `blockheads` only, then train cars, then the interaction objects.
    pub fn set_paused(&self, paused: bool) {
        let hooks = ops();
        for bh in self.blockheads() {
            hooks.blockhead_set_paused(&bh, paused);
        }
        for t in TRAIN_CAR_DYNAMIC_OBJECT_TYPES {
            for o in self.dot_snapshot(t) {
                hooks.train_car_set_paused(&o, paused);
            }
        }
        for t in INTERACTION_OBJECT_DYNAMIC_OBJECT_TYPES {
            for o in self.dot_snapshot(t) {
                if let Some(il) = o.as_interaction_like() {
                    il.set_paused(&o, paused);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dynworld::lookups::{set_ops_hooks, DwOpsHooks};
    use crate::dynworld::state::testutil::*;
    use crate::dynworld::state::{DwHooks, DwServer};
    use crate::plist::value::PlistValue;
    use crate::types::structs::IntPair;
    use std::cell::RefCell;
    use std::rc::Rc;

    struct Srv;
    impl DwServer for Srv {
        fn player_info_for_peer_id(&self, _p: &str) -> Option<PlistValue> {
            None
        }
        fn player_is_admin_with_id(&self, _c: &str) -> bool {
            false
        }
        fn player_update(&self) -> Option<String> {
            None
        }
        fn notify_players_changed(&self) {}
    }

    struct Ops {
        sent: RefCell<Vec<(Vec<u8>, Vec<String>, bool)>>,
    }
    impl crate::dynworld::lookups::DwOpsHooksStub for Ops {
        fn server_send_network_data_to_peers_reliable(&self, data: Vec<u8>, peers: &[String], reliable: bool) {
            self.sent.borrow_mut().push((data, peers.to_vec(), reliable));
        }
    }

    struct Hk;
    impl crate::dynworld::state::DwHooksStub for Hk {
        fn blockhead_client_id(&self, b: &ObjRef) -> Option<String> {
            Some(format!("player{}", b.unique_id() % 10))
        }
    }

    fn obj(ty: i32, id: u64) -> ObjRef {
        let o: ObjRef = Obj::new(ty, id, Rc::new(RefCell::new(vec![])));
        o.base().borrow_mut().pos = IntPair { x: 3, y: 3 };
        o
    }

    /// header `13 01 00*6 <req id LE>` + ids, accepted first, rejected second.
    #[test]
    fn pickup_reply_layout() {
        let d = dw("cl_pickup", 2);
        let (main, dwdb) = dbs("cl_pickup2");
        let _ = (main, dwdb);
        d.set_server_server_clients(Some(Rc::new(Srv)), None);
        let h = Rc::new(Ops { sent: RefCell::new(vec![]) });
        set_ops_hooks(h.clone());
        let torch = obj(17, 41);
        d.do_set(17, 41, torch.clone());
        d.client_pickup_request_count_client_id_blockhead_requester_unique_id(&[41, 99, 41], "cid", 0x0102);
        assert!(torch.needs_removed());
        let sent = h.sent.borrow();
        assert_eq!(sent.len(), 2);
        let (acc, peers, rel) = &sent[0];
        assert_eq!(&acc[..16], &[0x13, 1, 0, 0, 0, 0, 0, 0, 0x02, 0x01, 0, 0, 0, 0, 0, 0]);
        // id 41 twice: the second pass finds the torch again in the type loop (the map still holds it)
        assert_eq!(&acc[16..24], &41u64.to_le_bytes());
        assert_eq!(acc.len(), 16 + 16);
        assert_eq!(peers, &vec!["cid".to_string()]);
        assert!(*rel);
        let (rej, _, _) = &sent[1];
        assert_eq!(rej[1], 0);
        assert_eq!(&rej[16..], &99u64.to_le_bytes());
        drop(sent);
        h.sent.borrow_mut().clear();
        d.client_pickup_request_count_client_id_blockhead_requester_unique_id(&[], "cid", 1);
        assert!(h.sent.borrow().is_empty());
    }

    #[test]
    fn reply_parse_short_is_ignored() {
        let d = dw("cl_reply", 1);
        d.remote_pickup_request_reply(&[1, 2, 3]);
        let bytes = pickup_reply_bytes(true, 5, &[1, 2]);
        assert_eq!(bytes.len(), 32);
        d.remote_pickup_request_reply(&bytes);
    }

    /// simulated disconnect records the id once; controlling test reads it.
    #[test]
    fn disconnect_simulate_and_controlling() {
        let d = dw("cl_disc", 1);
        set_ops_hooks(Rc::new(Ops { sent: RefCell::new(vec![]) }));
        d.set_hooks(Rc::new(Hk));
        let bh = obj(24, 21); // client "player1"
        d.net_blockheads.borrow_mut().push(bh.clone());
        d.live_server_client_blockhead_inventories.borrow_mut().insert("player1".into(), PlistValue::mutable_dict());
        assert!(!d.is_controlling_blockheads_for_client_player("player1"));
        d.client_disconnected_simulate(Some("player1"), true);
        d.client_disconnected_simulate(Some("player1"), true);
        assert_eq!(d.disconnected_clients_save_dir_names.borrow().as_ref().unwrap().len(), 1);
        assert!(!bh.needs_removed());
        assert!(d.is_controlling_blockheads_for_client_player("player1"));
        assert!(d.live_server_client_blockhead_inventories.borrow().is_empty());
        d.client_disconnected_simulate(None, true);
        d.client_connected(None);
    }

    #[test]
    fn kick_stops_actions_for_matching_only() {
        struct O(RefCell<Vec<u64>>);
        impl crate::dynworld::lookups::DwOpsHooksStub for O {
            fn blockhead_stop_all_actions(&self, bh: &ObjRef) {
                self.0.borrow_mut().push(bh.unique_id());
            }
        }
        let d = dw("cl_kick", 1);
        d.set_hooks(Rc::new(Hk));
        let o = Rc::new(O(RefCell::new(vec![])));
        set_ops_hooks(o.clone());
        d.net_blockheads.borrow_mut().push(obj(24, 21));
        d.net_blockheads.borrow_mut().push(obj(24, 22));
        d.net_blockheads_with_disconnected_clients.borrow_mut().push(obj(24, 31));
        d.stop_all_blockhead_actions_for_client_due_to_kick("player1");
        assert_eq!(*o.0.borrow(), vec![21, 31]);
    }
}
