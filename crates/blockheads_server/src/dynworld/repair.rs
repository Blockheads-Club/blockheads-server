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

    use std::cell::RefCell;
    use std::rc::Rc;

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
        d.set_server_server_clients(Some(Rc::new(Srv { admin: false })), None);
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
        d.set_server_server_clients(Some(Rc::new(Srv { admin: false })), None);
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

/// Scenario tests of the removal protocol with objects that drop things and refuse to be removed.
#[cfg(test)]
mod scenario_tests {
    use super::*;
    use crate::dynworld::state::testutil::*;
    use crate::objects::base::{DynamicObject, DynamicObjectBase};
    use crate::objects::class_info::{ClassInfo, DYNAMIC_OBJECT_CLASS};
    use crate::plist::value::PlistDict;
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;

    /// An object that drops a configurable item when removed and may refuse the removal.
    struct Droppy {
        base: RefCell<DynamicObjectBase>,
        ty: i32,
        drops: i32,
        removable: Cell<bool>,
        owner: Option<String>,
    }
    fn droppy(ty: i32, id: u64, drops: i32, owner: Option<&str>) -> ObjRef {
        let o = crate::objects::handle::alloc_obj(|w| Droppy {
            base: RefCell::new(DynamicObjectBase::alloc_zeroed_with(w)),
            ty,
            drops,
            removable: Cell::new(true),
            owner: owner.map(str::to_string),
        });
        o.base.borrow_mut().unique_id = id;
        o.base.borrow_mut().pos = IntPair { x: 2, y: 2 };
        o
    }
    impl DynamicObject for Droppy {
        fn base(&self) -> &RefCell<DynamicObjectBase> {
            &self.base
        }
        fn class_info(&self) -> &'static ClassInfo {
            &DYNAMIC_OBJECT_CLASS
        }
        fn as_any(&self) -> &dyn std::any::Any {
            self
        }
        fn object_type(&self) -> i32 {
            self.ty
        }
        fn freeblock_creation_item_type(&self) -> i32 {
            self.drops
        }
        fn free_block_creation_save_dict(&self, _me: &ObjRef) -> Option<PlistDict> {
            let PlistValue::Dict(d) = plist_dict(vec![("x", PlistValue::int(1))]) else { unreachable!() };
            Some(d)
        }
        fn free_block_creation_data_a(&self) -> u16 {
            7
        }
        fn free_block_creation_data_b(&self) -> u16 {
            8
        }
        fn can_be_removed_by_blockhead(&self, _me: &ObjRef, _b: &ObjRef) -> bool {
            self.removable.get()
        }
        fn client_id_for_saving_seperately_and_only_loading_while_player_online(&self) -> Option<String> {
            self.owner.clone()
        }
    }

    fn setup(name: &str) -> (Rc<DynamicWorld>, Log, Rc<Hk>) {
        let (d, _scs) = dw_served(name, &["cl"]);
        let log: Log = Rc::new(RefCell::new(vec![]));
        let h = hk(&log);
        d.set_hooks(h.clone());
        (d, log, h)
    }

    #[test]
    fn animals_of_a_client_are_removed_with_the_client() {
        let (d, _log, _h) = setup("rs_owned");
        let mine = droppy(13, 1, 0, Some("cl"));
        assert_eq!(mine.class_info().name, DYNAMIC_OBJECT_CLASS.name);
        let theirs = droppy(13, 2, 0, Some("other"));
        let queued = droppy(25, 3, 0, Some("cl"));
        let unowned = droppy(13, 4, 0, None);
        d.do_set(13, 1, mine.clone());
        d.do_set(13, 2, theirs.clone());
        d.ta_set(25, 3, queued.clone());
        d.do_set(13, 4, unowned.clone());
        d.remove_dynamic_objects_belonging_to_client("cl");
        assert!(mine.needs_removed() && queued.needs_removed());
        assert!(!theirs.needs_removed() && !unowned.needs_removed());
    }

    #[test]
    fn requested_removals_drop_their_item() {
        let (d, log, h) = setup("rs_req");
        let bh = tobj(24, 20, &log, None, None, None);
        d.net_blockheads.borrow_mut().push(bh);
        h.client_of.borrow_mut().push((20, "cl".to_string()));
        // no server world: ignored
        let plain = dw("rs_plain", 1);
        plain.client_blockhead_with_id_from_client_requests_dyamic_object_removal_with_id(20, "cl", 10);
        // an object that drops something, one that drops nothing, one that refuses, an unknown blockhead
        let a = droppy(17, 10, 17, None);
        let b = droppy(17, 11, 0, None);
        let c = droppy(17, 12, 17, None);
        c.base().borrow_mut().unique_id = 12;
        for o in [&a, &b, &c] {
            d.do_set(17, o.unique_id(), o.clone());
        }
        let c_ref = crate::objects::handle::downcast::<Droppy>(&c).unwrap();
        c_ref.removable.set(false);
        d.client_blockhead_with_id_from_client_requests_dyamic_object_removal_with_id(20, "cl", 10);
        d.client_blockhead_with_id_from_client_requests_dyamic_object_removal_with_id(20, "cl", 11);
        d.client_blockhead_with_id_from_client_requests_dyamic_object_removal_with_id(20, "cl", 12);
        d.client_blockhead_with_id_from_client_requests_dyamic_object_removal_with_id(21, "cl", 10);
        assert!(a.needs_removed() && b.needs_removed() && !c.needs_removed());
        // already removed objects are left alone
        d.client_blockhead_with_id_from_client_requests_dyamic_object_removal_with_id(20, "cl", 10);
        assert_eq!(d.do_len(14), 1);
        // the repair path drops items too, but not twice
        let e = droppy(19, 30, 53, None);
        d.do_set(19, 30, e.clone());
        d.remove_object_due_to_repair(&e);
        assert!(e.needs_removed());
        assert_eq!(d.do_len(14), 2);
    }

    #[test]
    fn interaction_objects_remove_themselves() {
        use crate::types::item_type::ItemType;
        crate::objects::registry::install();
        let (d, log, h) = setup("rs_inter");
        let bh = tobj(24, 20, &log, None, None, None);
        d.net_blockheads.borrow_mut().push(bh);
        h.client_of.borrow_mut().push((20, "cl".to_string()));
        let place = |t: ItemType, x: i32| {
            let item = crate::items::inventory_item::InventoryItem::new(t, 0, 0, None, None);
            let r = d.interaction_object_placed_at_position_with_item_flipped_save_dict_placed_by_client_client_name(
                IntPair { x, y: 12 },
                &item.borrow(),
                false,
                None,
                None,
                None,
            );
            r.unwrap()
        };
        let chest = place(ItemType::ITEM_CHEST, 5);
        // an interaction-like object that is in use is left alone; a free one is removed
        chest.as_interaction_object().unwrap().borrow_mut().is_in_use = true;
        d.client_blockhead_with_id_from_client_requests_dyamic_object_removal_with_id(20, "cl", chest.unique_id());
        chest.as_interaction_object().unwrap().borrow_mut().is_in_use = false;
        d.client_blockhead_with_id_from_client_requests_dyamic_object_removal_with_id(20, "cl", chest.unique_id());
        d.remove_object_due_to_repair(&chest);
        // an interaction type that is not interaction-like only gets nothing done
        let fake = droppy(46, 99, 0, None);
        d.remove_object_due_to_repair(&fake);
        d.do_set(46, 99, fake.clone());
        d.client_blockhead_with_id_from_client_requests_dyamic_object_removal_with_id(20, "cl", 99);
        // the server-less world does nothing
        let plain = dw("rs_inter_plain", 1);
        plain.remove_object_due_to_repair(&fake);
        plain.do_repair_for_tile_at_pos(IntPair { x: 1, y: 1 });
    }

    #[test]
    fn repairs_drop_the_door() {
        let (d, log, h) = setup("rs_door");
        let door = droppy(20, 40, 0, None);
        h.item.set(52);
        d.register_with_pos(&door, 20, IntPair { x: 2, y: 2 });
        let _ = log;
        d.do_repair_for_tile_at_pos(IntPair { x: 2, y: 2 });
        assert!(door.needs_removed());
        assert_eq!(d.do_len(14), 1);
    }
}
