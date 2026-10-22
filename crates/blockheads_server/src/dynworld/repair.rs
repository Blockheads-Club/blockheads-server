//! Removal and repair requests: `removeDynamicObjectsBelongingToClient:`, the client removal request,
//! `removeObjectDueToRepair:` and `doRepairForTileAtPos:`.
//!
//! `removeStandardObject:` never erases from the maps, so iterating snapshots here is faithful.

use super::lookups::wrap_dx;
use super::state::{DynamicWorld, MAX_DYNAMIC_OBJECT_TYPES};
use crate::objects::handle::ObjRef;
use crate::objects::type_tables::{
    object_type_can_be_loaded_only_when_client_owner_online, object_type_has_static_position,
    object_type_is_interaction_object,
};
use crate::plist::value::PlistValue;
use crate::types::item_type::ItemType;
use crate::types::structs::IntPair;

impl DynamicWorld {
    /// All objects of type `t`: `dynamicObjects[t]` then `dynamicObjectsToAdd[t]`, each ascending uid.
    fn all_of_type(&self, t: i32) -> Vec<ObjRef> {
        let mut v = self.dot_snapshot(t);
        v.extend(self.add_snapshot(t));
        v
    }

    /// `-removeDynamicObjectsBelongingToClient:`: NPC types whose
    /// `clientIDForSavingSeperatelyAndOnlyLoadingWhilePlayerOnline` equals the id get `needsRemoved`.
    pub fn remove_dynamic_objects_belonging_to_client(&self, client_id: &str) {
        for j in 0..MAX_DYNAMIC_OBJECT_TYPES as i32 {
            if object_type_can_be_loaded_only_when_client_owner_online(j) {
                for obj in self.all_of_type(j) {
                    if obj.client_id_for_saving_seperately_and_only_loading_while_player_online().as_deref()
                        == Some(client_id)
                    {
                        obj.set_needs_removed(&obj, true);
                    }
                }
            }
        }
    }

    /// Standard removal + free block creation shared by the client request and the repair path.
    fn remove_standard_and_drop(&self, o: &ObjRef, play_sound: i32, priority: Option<&ObjRef>) {
        self.remove_standard_object(Some(o));
        let it = o.freeblock_creation_item_type();
        if it != 0 {
            let sd = o.free_block_creation_save_dict(o).map(PlistValue::Dict);
            self.create_free_block_at_position_of_type_data_a_data_b_sub_items_dynamic_object_save_dict_hovers_play_sound_priority_blockhead(
                o.pos(),
                ItemType(it as u32),
                o.free_block_creation_data_a(),
                o.free_block_creation_data_b(),
                None,
                sd,
                false,
                play_sound,
                priority,
            );
        }
    }

    /// `-clientBlockheadWithID:fromClient:requestsDyamicObjectRemovalWithID:`. The object search scans types
    /// 0..64 (`dynamicObjects` then `ToAdd`) and a LATER match overwrites an earlier one.
    pub fn client_blockhead_with_id_from_client_requests_dyamic_object_removal_with_id(
        &self,
        bh_id: u64,
        client_id: &str,
        object_id: u64,
    ) {
        if !self.has_server() {
            return;
        }
        let mut found: Option<ObjRef> = None;
        for j in 0..MAX_DYNAMIC_OBJECT_TYPES as i32 {
            if let Some(o) = self.dot_snapshot(j).into_iter().find(|o| o.unique_id() == object_id) {
                found = Some(o);
            }
            if let Some(o) = self.add_snapshot(j).into_iter().find(|o| o.unique_id() == object_id) {
                found = Some(o);
            }
        }
        let Some(found) = found else { return };
        if found.needs_removed() {
            return;
        }
        let Some(bh) = self.blockhead_with_id_including_net(bh_id) else { return };
        if self.hooks().blockhead_client_id(&bh).as_deref() != Some(client_id) {
            return; // requester must own the blockhead
        }
        if !found.can_be_removed_by_blockhead(&found, &bh) {
            return;
        }
        if !object_type_is_interaction_object(found.object_type()) {
            self.remove_standard_and_drop(&found, 1, Some(&bh));
        } else if !found.is_in_use() {
            if let Some(il) = found.as_interaction_like() {
                il.remove(&found, Some(&bh));
            }
        }
    }

    /// `-removeObjectDueToRepair:` (server only).
    pub fn remove_object_due_to_repair(&self, o: &ObjRef) {
        if !self.has_server() {
            return;
        }
        if !object_type_is_interaction_object(o.object_type()) {
            self.remove_standard_and_drop(o, 1, None);
        } else if let Some(il) = o.as_interaction_like() {
            il.remove(o, None);
        }
    }

    /// `-doRepairForTileAtPos:`: removes the door (creating a free block with the door's save dict,
    /// `hovers: false`, `playSound: 1`), then every static-position object type except 20 and 21 whose `pos.y` equals
    /// the row and whose wrapped `dx == 0`.
    pub fn do_repair_for_tile_at_pos(&self, remove_pos: IntPair) {
        if !self.has_server() {
            return;
        }
        if let Some(door) = self.remove_door_at_pos(remove_pos) {
            let it = self.hooks().item_type(&door);
            let sd = Some(PlistValue::Dict(door.get_save_dict(&door)));
            self.create_free_block_at_position_of_type_data_a_data_b_sub_items_dynamic_object_save_dict_hovers_play_sound_priority_blockhead(
                remove_pos,
                ItemType(it as u32),
                0,
                0,
                None,
                sd,
                false,
                1,
                None,
            );
        }
        for i in 0..MAX_DYNAMIC_OBJECT_TYPES as i32 {
            if i != 0x15 && i != 0x14 && object_type_has_static_position(i) {
                for obj in self.all_of_type(i) {
                    let p = obj.pos();
                    if p.y == remove_pos.y {
                        let n = self.world().world_width_macro().wrapping_mul(32); // re-read each time (no side effects)
                        let d = wrap_dx(p.x.wrapping_sub(remove_pos.x), n);
                        if d == 0 {
                            self.remove_object_due_to_repair(&obj);
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dynworld::state::testutil::*;
    use crate::dynworld::state::{DwHooks, DwServer};
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

    struct Hk;
    impl crate::dynworld::state::DwHooksStub for Hk {
        fn blockhead_client_id(&self, _b: &ObjRef) -> Option<String> {
            Some("cl".into())
        }
    }

    fn obj(ty: i32, id: u64, x: i32, y: i32) -> ObjRef {
        let o: ObjRef = Obj::new(ty, id, Rc::new(RefCell::new(vec![])));
        o.base().borrow_mut().pos = IntPair { x, y };
        o
    }

    /// the repair crosses the world seam (x + N == x), skips types 20 / 21.
    #[test]
    fn repair_wraps_and_skips() {
        let d = dw("rp_wrap", 1); // N = 32
        d.set_server_server_clients(Some(Rc::new(Srv)), None);
        let torch_same_row = obj(17, 1, 4, 9); // dx = 0
        let torch_other_row = obj(17, 2, 4, 10);
        let ladder_far = obj(19, 3, 5, 9);
        let door = obj(20, 4, 4, 9);
        let light = obj(21, 5, 4, 9);
        for (t, o) in [(17, &torch_same_row), (17, &torch_other_row), (19, &ladder_far), (20, &door), (21, &light)] {
            d.do_set(t, o.unique_id(), o.clone());
        }
        d.do_repair_for_tile_at_pos(IntPair { x: 4, y: 9 });
        assert!(torch_same_row.needs_removed());
        assert!(!door.needs_removed()); // type 20 excluded from the loop (doors go through removeDoorAtPos)
        assert!(!torch_other_row.needs_removed());
        assert!(!ladder_far.needs_removed());
        assert!(!light.needs_removed());
        // wrap: object at x = 4 + 32 would be d = 32 -> d >= N/2 -> d - N = 0
        assert_eq!(wrap_dx(32, 32), 0);
        assert_eq!(wrap_dx(-32, 32), 0);
    }

    #[test]
    fn client_removal_checks_ownership() {
        let d = dw("rp_req", 1);
        d.set_hooks(Rc::new(Hk));
        d.set_server_server_clients(Some(Rc::new(Srv)), None);
        let torch = obj(17, 10, 2, 2);
        d.do_set(17, 10, torch.clone());
        let bh = obj(24, 20, 2, 2);
        d.net_blockheads.borrow_mut().push(bh);
        // wrong client
        d.client_blockhead_with_id_from_client_requests_dyamic_object_removal_with_id(20, "other", 10);
        assert!(!torch.needs_removed());
        // owner: Obj uses the default canBeRemovedByBlockhead (base rule)
        d.client_blockhead_with_id_from_client_requests_dyamic_object_removal_with_id(20, "cl", 10);
        // unknown object: no-op
        d.client_blockhead_with_id_from_client_requests_dyamic_object_removal_with_id(20, "cl", 999);
    }
}
