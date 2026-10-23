//! `Door: DynamicObject` (DynamicObjectType 20): door 52, trapdoor 69, iron door 164, iron trapdoor 165, gate 304.
//!
//! Client-only members (`draw:`, `openCloseTransition`, saved draw buffers) are not ported.

use std::cell::RefCell;
use std::rc::Rc;

use super::column::{
    append_plist_tail, owner_from_client_or_save, read_plist_tail, sd_get, sd_string, tail_string, Links,
};
use crate::dynworld::state::DynamicWorld;
use crate::objects::base::{
    base_dynamic_object_net_data, base_get_save_dict, base_init_with_world_dynamic_world_at_position_cache,
    base_init_with_world_dynamic_world_cache_net_data, base_init_with_world_dynamic_world_save_dict_cache,
    base_remote_update, padded_bytes, DynamicObject, DynamicObjectBase, DynamicWorldRef, WorldApi, WorldRef,
};
use crate::objects::class_info::{ClassInfo, DYNAMIC_OBJECT_CLASS};
use crate::objects::handle::{self, CacheRef, ObjRef};
use crate::plist::value::{PlistDict, PlistValue};
use crate::types::item_type::ItemType;
use crate::types::netdata::DoorCreationNetData;
use crate::types::structs::{IntPair, WireStruct};
use crate::world::helpers::{tile_contains_door, tile_contains_trap_door, tile_is_solid};
use crate::world::state::World;
use crate::world::tiles::tile_at_world_position_loaded;

/// `DYNAMIC_OBJECT_TYPE_DOOR`.
pub const DYNAMIC_OBJECT_TYPE_DOOR: i32 = 20;
pub const ITEM_DOOR: i32 = 52;
pub const ITEM_TRAPDOOR: i32 = 69;
pub const ITEM_IRON_DOOR: i32 = 164;
pub const ITEM_IRON_TRAPDOOR: i32 = 165;
pub const ITEM_GATE: i32 = 304;
/// `CONTENTS_ELEVATOR_SHAFT` (`contents` byte).
pub const CONTENTS_ELEVATOR_SHAFT: u8 = 103;

/// `Door` class descriptor.
pub static DOOR_CLASS: ClassInfo = ClassInfo { name: "Door", superclass: Some(&DYNAMIC_OBJECT_CLASS), protocols: &[] };

#[inline]
fn is_trapdoor_like(t: i32) -> bool {
    t == ITEM_TRAPDOOR || t == ITEM_IRON_TRAPDOOR
}
#[inline]
fn is_iron(t: i32) -> bool {
    t == ITEM_IRON_DOOR || t == ITEM_IRON_TRAPDOOR
}

/// Door ivars (render ivars omitted).
pub struct DoorState {
    /// 0x4d runtime only
    pub open: bool,
    /// 0x50
    pub open_direction: i32,
    /// 0x58
    pub item_type: i32,
    /// 0x5c
    pub blocked: bool,
    /// 0x60
    pub iron_place_client_id: Option<String>,
}

/// `Door: DynamicObject` (type 20).
pub struct Door {
    base: RefCell<DynamicObjectBase>,
    /// The Door ivars.
    pub st: RefCell<DoorState>,
    links: Links,
}

impl Door {
    fn alloc(world: &Rc<World>, dw: &Rc<DynamicWorld>) -> Rc<Door> {
        let links = Links::new(world, dw);
        handle::alloc_obj(|sw| Door {
            base: RefCell::new(DynamicObjectBase::alloc_zeroed_with(sw)),
            st: RefCell::new(DoorState {
                open: false,
                open_direction: 0,
                item_type: 0,
                blocked: false,
                iron_place_client_id: None,
            }),
            links,
        })
    }

    /// Placement constructor. Order: base, itemType, iron/gate owner, `initSubDerivedItems` (no-op),
    /// `checkAndUpdateBlockedStatus`.
    pub fn init_with_world_dynamic_world_at_position_cache_type_save_dict_placed_by_client(
        world: &Rc<World>,
        dw: &Rc<DynamicWorld>,
        pos: IntPair,
        cache: CacheRef,
        item_type: i32,
        save_dict: Option<&PlistDict>,
        client_id: Option<&str>,
    ) -> Option<ObjRef> {
        let me = Self::alloc(world, dw);
        let me_obj: ObjRef = me.clone();
        let (wref, dref): (WorldRef, DynamicWorldRef) = (world.clone(), dw.clone());
        // `base_init_with_world_dynamic_world_at_position_cache` answers true on every path
        let _ = base_init_with_world_dynamic_world_at_position_cache(&me_obj, &wref, &dref, pos, cache);
        me.st.borrow_mut().item_type = item_type;
        if is_iron(item_type) {
            let v = match client_id {
                Some(c) => Some(c.to_string()),
                None => sd_get(save_dict, "ironPlaceClientID").and_then(|v| v.as_str()).map(|s| s.to_string()),
            };
            if v.is_some() {
                me.st.borrow_mut().iron_place_client_id = v;
            }
        }
        if item_type == ITEM_GATE {
            if let Some(o) = owner_from_client_or_save(client_id, save_dict) {
                me.base.borrow_mut().owner_id = Some(o);
            }
        }
        me.check_and_update_blocked_status(&me_obj);
        Some(me_obj)
    }

    /// Save constructor: the saved `blocked` is trusted (no recompute, no tile write).
    pub fn init_with_world_dynamic_world_save_dict_cache(
        world: &Rc<World>,
        dw: &Rc<DynamicWorld>,
        save_dict: &PlistDict,
        cache: CacheRef,
    ) -> Option<ObjRef> {
        let me = Self::alloc(world, dw);
        let me_obj: ObjRef = me.clone();
        let (wref, dref): (WorldRef, DynamicWorldRef) = (world.clone(), dw.clone());
        // `base_init_with_world_dynamic_world_save_dict_cache` answers true on every path
        let _ = base_init_with_world_dynamic_world_save_dict_cache(&me_obj, &wref, &dref, save_dict, cache);
        {
            let g = |k: &str| save_dict.map.get(&k.to_string());
            let mut st = me.st.borrow_mut();
            st.item_type = g("itemType").map_or(0, |v| v.int_value());
            st.blocked = g("blocked").map_or(false, |v| v.bool_value());
            st.iron_place_client_id = sd_string(save_dict, "ironPlaceClientID");
        }
        me.base.borrow_mut().owner_id = sd_string(save_dict, "ownerID");
        Some(me_obj)
    }

    /// Net constructor (reachable: clients upload doors). The iron path has NO length guard (quirk kept).
    pub fn init_with_world_dynamic_world_cache_net_data(
        world: &Rc<World>,
        dw: &Rc<DynamicWorld>,
        cache: CacheRef,
        net_data: &[u8],
    ) -> Option<ObjRef> {
        let me = Self::alloc(world, dw);
        let me_obj: ObjRef = me.clone();
        let (wref, dref): (WorldRef, DynamicWorldRef) = (world.clone(), dw.clone());
        if !base_init_with_world_dynamic_world_cache_net_data(&me_obj, &wref, &dref, cache, net_data) {
            return None;
        }
        let s = DoorCreationNetData::read_le(&padded_bytes(net_data, 0x20));
        let item_type = s.item_type as i32;
        {
            let mut st = me.st.borrow_mut();
            st.item_type = item_type;
            st.blocked = s.blocked != 0;
        }
        if is_iron(item_type) {
            // UB-PARITY (E4 c): no `len > 0x20` guard; len < 0x20 raises NSRangeException in read_plist_tail
            let v = tail_string(&read_plist_tail(net_data, 0x20), "ironPlaceClientID");
            me.st.borrow_mut().iron_place_client_id = v;
        }
        if item_type == ITEM_GATE && net_data.len() > 0x20 {
            let v = tail_string(&read_plist_tail(net_data, 0x20), "ownerID");
            me.base.borrow_mut().owner_id = v;
        }
        Some(me_obj)
    }

    /// `-[Door itemType]`
    pub fn item_type(&self) -> i32 {
        self.st.borrow().item_type
    }
    /// `-[Door isOpen]`
    pub fn is_open(&self) -> bool {
        self.st.borrow().open
    }
    /// `-[Door blocked]`
    pub fn blocked(&self) -> bool {
        self.st.borrow().blocked
    }
    /// `-[Door ironPlaceClientID]`
    pub fn iron_place_client_id(&self) -> Option<String> {
        self.st.borrow().iron_place_client_id.clone()
    }

    /// `-[Door setOpen:direction:]`: the sound call is a nil-receiver no-op.
    pub fn set_open_direction(&self, _me: &ObjRef, open: bool, direction: i32) {
        let mut st = self.st.borrow_mut();
        if st.open == open {
            return;
        }
        st.open = open;
        if open {
            st.open_direction = direction;
        }
    }

    /// `-[Door checkAndUpdateBlockedStatus]`.
    pub fn check_and_update_blocked_status(&self, _me: &ObjRef) {
        let pos = self.base.borrow().pos;
        let world = self.links.world();
        let item_type = self.item_type();
        let old = self.st.borrow().blocked;
        let mut blocked = false;
        let t = tile_at_world_position_loaded(pos.x, pos.y, &world);
        if is_trapdoor_like(item_type) {
            if let Some(t) = t {
                if t.contents() != CONTENTS_ELEVATOR_SHAFT {
                    let up = tile_at_world_position_loaded(pos.x, pos.y + 1, &world);
                    if tile_is_solid(up) && !tile_contains_trap_door(up) {
                        blocked = true;
                    }
                    if !blocked {
                        let dn = tile_at_world_position_loaded(pos.x, pos.y - 1, &world);
                        if tile_is_solid(dn) && !tile_contains_trap_door(dn) {
                            blocked = true;
                        }
                    }
                }
                t.set_terrain_slow_factor(if blocked { 0xff } else { 0x7f });
            }
        } else if let Some(t) = t {
            if t.contents() != CONTENTS_ELEVATOR_SHAFT {
                for (dx, dy) in [(1, 0), (1, 1), (-1, 0), (-1, 1)] {
                    let n = tile_at_world_position_loaded(pos.x + dx, pos.y + dy, &world);
                    if tile_is_solid(n) && !tile_contains_door(n) {
                        blocked = true;
                        break;
                    }
                }
            }
            t.set_terrain_slow_factor(if blocked { 0xff } else { 0x7f });
        }
        self.st.borrow_mut().blocked = blocked;
        if old != blocked {
            self.base.borrow_mut().update_needs_to_be_sent = true;
            self.links.dw().world_changed_at_pos_send_reliably(pos, true);
        }
    }

    /// `-[Door canBeUsedByBlockhead:]` (server).
    pub fn can_be_used_by_blockhead(&self, _me: &ObjRef, blockhead: &ObjRef) -> bool {
        let Some(bh) = blockhead.as_blockhead() else {
            // `[nil isClientBlockheadBeingControlledByServer]` = NO
            return self.can_be_used_nil_blockhead();
        };
        let world = self.links.world();
        if WorldApi::is_admin(&*world) && !bh.is_client_blockhead_being_controlled_by_server() {
            return true;
        }
        let item_type = self.item_type();
        let placer = self.st.borrow().iron_place_client_id.clone();
        if is_iron(item_type) && placer.is_some() {
            return bh.local_net_id() == placer;
        }
        true
    }

    /// A non-Blockhead argument behaves like nil messaging: the flag reads NO, `localNetID` is nil.
    fn can_be_used_nil_blockhead(&self) -> bool {
        let world = self.links.world();
        if WorldApi::is_admin(&*world) {
            return true;
        }
        let item_type = self.item_type();
        let placer = self.st.borrow().iron_place_client_id.clone();
        if is_iron(item_type) && placer.is_some() {
            return false;
        }
        true
    }
}

impl DynamicObject for Door {
    fn base(&self) -> &RefCell<DynamicObjectBase> {
        &self.base
    }
    fn class_info(&self) -> &'static ClassInfo {
        &DOOR_CLASS
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn object_type(&self) -> i32 {
        DYNAMIC_OBJECT_TYPE_DOOR
    }
    fn occupies_background_contents(&self) -> bool {
        self.item_type() != ITEM_GATE
    }
    fn occupies_normal_contents(&self) -> bool {
        self.item_type() == ITEM_GATE
    }
    fn freeblock_creation_item_type(&self) -> i32 {
        self.item_type()
    }
    fn free_block_creation_save_dict(&self, me: &ObjRef) -> Option<PlistDict> {
        Some(self.get_save_dict(me))
    }

    /// `-[Door setNeedsRemoved:]`: after the store, a removed door frees the `terrainSlowFactor` of its tiles.
    fn set_needs_removed(&self, _me: &ObjRef, v: bool) {
        self.base.borrow_mut().needs_removed = v;
        if v {
            let world = self.links.world();
            let pos = self.base.borrow().pos;
            if let Some(t) = tile_at_world_position_loaded(pos.x, pos.y, &world) {
                t.set_terrain_slow_factor(0x7f);
            }
            let it = self.item_type();
            if it == ITEM_DOOR || it == ITEM_IRON_DOOR {
                if let Some(t2) = tile_at_world_position_loaded(pos.x, pos.y + 1, &world) {
                    t2.set_terrain_slow_factor(0x7f);
                }
            }
        }
    }

    fn get_save_dict(&self, me: &ObjRef) -> PlistDict {
        let mut d = base_get_save_dict(me);
        let (it, blocked, iron) = {
            let st = self.st.borrow();
            (st.item_type, st.blocked, st.iron_place_client_id.clone())
        };
        d.map.insert("itemType".to_string(), PlistValue::int(it as i64));
        d.map.insert("blocked".to_string(), PlistValue::int(blocked as i64));
        if let Some(i) = iron {
            d.map.insert("ironPlaceClientID".to_string(), PlistValue::string(i));
        }
        if let Some(o) = self.base.borrow().owner_id.clone() {
            d.map.insert("ownerID".to_string(), PlistValue::string(o));
        }
        d
    }

    fn creation_net_data_for_client(&self, me: &ObjRef, _client_id: Option<&str>) -> Option<Vec<u8>> {
        let (it, blocked, iron) = {
            let st = self.st.borrow();
            (st.item_type, st.blocked, st.iron_place_client_id.clone())
        };
        let s = DoorCreationNetData {
            dynamic_object_net_data: base_dynamic_object_net_data(me),
            item_type: it as u16,
            blocked: blocked as u8,
            padding: [0; 5],
        };
        let mut out = s.to_vec();
        let owner = self.base.borrow().owner_id.clone();
        append_plist_tail(&mut out, &[("ironPlaceClientID", iron.as_deref()), ("ownerID", owner.as_deref())]);
        Some(out)
    }
    fn update_net_data_for_client(&self, me: &ObjRef, client_id: Option<&str>) -> Option<Vec<u8>> {
        self.creation_net_data_for_client(me, client_id)
    }

    /// `-[Door remoteUpdate:]`: only `blocked` is copied (no tile write, no flag).
    fn remote_update(&self, me: &ObjRef, net_data: &[u8]) {
        base_remote_update(me, net_data);
        let u = DoorCreationNetData::read_le(&padded_bytes(net_data, 0x20));
        self.st.borrow_mut().blocked = u.blocked != 0;
    }

    /// `-[Door worldChanged:]`.
    fn world_changed(&self, me: &ObjRef, positions: &mut Vec<IntPair>) {
        if self.base.borrow().is_net {
            return;
        }
        let world = self.links.world();
        let pos = self.base.borrow().pos;
        let it = self.item_type();
        for p in positions.iter() {
            if is_trapdoor_like(it) {
                if p.x == pos.x && pos.y - 1 <= p.y && p.y <= pos.y + 1 {
                    self.check_and_update_blocked_status(me);
                }
            } else if pos.x - 1 <= p.x && p.x <= pos.x + 1 {
                if pos.y <= p.y && p.y <= pos.y + 1 {
                    self.check_and_update_blocked_status(me);
                }
                if it == ITEM_GATE && p.y == pos.y - 1 {
                    let t = tile_at_world_position_loaded(pos.x, pos.y - 1, &world);
                    if t.is_some() && !tile_is_solid(t) {
                        let sd = self.get_save_dict(me);
                        let dw = self.links.dw();
                        dw.create_free_block_at_position_of_type_data_a_data_b_sub_items_dynamic_object_save_dict_hovers_play_sound_priority_blockhead(
                            pos,
                            ItemType(it as u32),
                            0,
                            0,
                            None,
                            Some(PlistValue::Dict(sd)),
                            false,
                            1,
                            None,
                        );
                        dw.remove_door_at_pos(pos);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::column::testkit::*;
    use super::*;
    use crate::objects::base::BlockheadApi;
    use crate::objects::class_info::DYNAMIC_OBJECT_CLASS;

    struct MockBh {
        base: RefCell<DynamicObjectBase>,
        controlled: bool,
        net_id: Option<String>,
    }
    impl DynamicObject for MockBh {
        fn base(&self) -> &RefCell<DynamicObjectBase> {
            &self.base
        }
        fn class_info(&self) -> &'static ClassInfo {
            &DYNAMIC_OBJECT_CLASS
        }
        fn as_any(&self) -> &dyn std::any::Any {
            self
        }
        fn as_blockhead(&self) -> Option<&dyn BlockheadApi> {
            Some(self)
        }
    }
    impl BlockheadApi for MockBh {
        fn client_id(&self) -> Option<String> {
            self.net_id.clone()
        }
        fn local_net_id(&self) -> Option<String> {
            self.net_id.clone()
        }
        fn is_client_blockhead_being_controlled_by_server(&self) -> bool {
            self.controlled
        }
        fn is_interacting_with_any_interaction_object(&self) -> bool {
            false
        }
        fn set_interaction_object(&self, _o: Option<&ObjRef>) {}
        fn stop_interacting(&self) {}
    }
    fn bh(controlled: bool, id: Option<&str>) -> ObjRef {
        handle::alloc_obj(|w| MockBh {
            base: RefCell::new(DynamicObjectBase::alloc_zeroed_with(w)),
            controlled,
            net_id: id.map(|s| s.to_string()),
        })
    }

    fn mk(w: &Rc<World>, d: &Rc<DynamicWorld>, x: i32, y: i32, item: i32, client: Option<&str>) -> ObjRef {
        Door::init_with_world_dynamic_world_at_position_cache_type_save_dict_placed_by_client(
            w,
            d,
            pos(x, y),
            None,
            item,
            None,
            client,
        )
        .unwrap()
    }
    fn door(o: &ObjRef) -> &Door {
        handle::downcast::<Door>(o).unwrap()
    }

    /// : observed vector (empty dict -> 194 bytes; header 0x20 + 162 gzip bytes).
    #[test]
    fn net_layout_observed_lengths() {
        let (w, d) = env();
        let o = mk(&w, &d, 100, 40, ITEM_IRON_DOOR, None);
        o.base().borrow_mut().unique_id = 0;
        let b = o.creation_net_data_for_client(&o, None).unwrap();
        assert_eq!(b.len(), 194);
        assert_eq!(&b[0x18..0x1a], &164u16.to_le_bytes());
        assert!(b[0x1b..0x20].iter().all(|x| *x == 0));
        assert_eq!(&b[0x20..0x2a], &[0x1f, 0x8b, 8, 0, 0, 0, 0, 0, 0, 3]);
        // with an ironPlaceClientID the blob grows and round trips
        let o2 = mk(&w, &d, 100, 43, ITEM_IRON_DOOR, Some("0123456789abcdef0123456789abcdef"));
        let b2 = o2.creation_net_data_for_client(&o2, None).unwrap();
        assert!(b2.len() > 194);
        let o3 = Door::init_with_world_dynamic_world_cache_net_data(&w, &d, None, &b2).unwrap();
        assert!(o3.is_net());
        assert_eq!(door(&o3).iron_place_client_id().as_deref(), Some("0123456789abcdef0123456789abcdef"));
        // short iron blob raises NSRangeException (no length guard)
        let r = crate::foundation::exception::catch_ns(|| {
            Door::init_with_world_dynamic_world_cache_net_data(&w, &d, None, &b2[..0x1c])
        });
        assert_eq!(r.err().expect("raises").reason_or_null(), "Range location + length too great");
    }

    /// Both keys present: keys sorted (ironPlaceClientID before ownerID); gate owner from client id.
    #[test]
    fn save_dict_and_gate_owner() {
        let (w, d) = env();
        let g = mk(&w, &d, 40, 20, ITEM_GATE, Some("gate-owner"));
        assert_eq!(g.owner_id().as_deref(), Some("gate-owner"));
        let sd = g.get_save_dict(&g);
        assert!(sd.map.get(&"ownerID".to_string()).is_some());
        assert!(sd.map.get(&"ironPlaceClientID".to_string()).is_none());
        assert_eq!(sd.map.get(&"blocked".to_string()).unwrap().int_value(), 0);
        assert!(g.occupies_normal_contents() && !g.occupies_background_contents());
        let g2 = Door::init_with_world_dynamic_world_save_dict_cache(&w, &d, &sd, None).unwrap();
        assert_eq!(g2.owner_id().as_deref(), Some("gate-owner"));
        assert_eq!(door(&g2).item_type(), ITEM_GATE);
        let n = g.creation_net_data_for_client(&g, None).unwrap();
        let g3 = Door::init_with_world_dynamic_world_cache_net_data(&w, &d, None, &n).unwrap();
        assert_eq!(g3.owner_id().as_deref(), Some("gate-owner"));
        let wd = mk(&w, &d, 41, 20, ITEM_DOOR, None);
        assert!(wd.occupies_background_contents() && !wd.occupies_normal_contents());
    }

    #[test]
    fn blocked_status_follows_neighbours() {
        let (w, d) = env();
        let o = mk(&w, &d, 40, 20, ITEM_DOOR, None);
        assert!(!door(&o).blocked());
        assert_eq!(tile(&w, 40, 20).terrain_slow_factor(), 0x7f);
        solid(&w, 41, 21, true);
        o.update_needs_to_be_sent();
        o.set_update_needs_to_be_sent(false);
        o.world_changed(&o, &mut vec![pos(41, 21)]);
        assert!(door(&o).blocked());
        assert!(o.update_needs_to_be_sent());
        assert_eq!(tile(&w, 40, 20).terrain_slow_factor(), 0xff);
        // a neighbouring door tile does not block
        tile(&w, 41, 21).set_background_contents(70);
        o.world_changed(&o, &mut vec![pos(41, 21)]);
        assert!(!door(&o).blocked());
        assert_eq!(tile(&w, 40, 20).terrain_slow_factor(), 0x7f);
        // positions outside the window are ignored
        solid(&w, 41, 20, true);
        o.world_changed(&o, &mut vec![pos(45, 20)]);
        assert!(!door(&o).blocked());
        o.world_changed(&o, &mut vec![pos(41, 20)]);
        assert!(door(&o).blocked());
        // elevator shaft anchor never blocks but the slow factor is still written
        tile(&w, 40, 20).set_contents(CONTENTS_ELEVATOR_SHAFT);
        o.world_changed(&o, &mut vec![pos(41, 20)]);
        assert!(!door(&o).blocked());
        assert_eq!(tile(&w, 40, 20).terrain_slow_factor(), 0x7f);
        // removal frees both tiles of a two tall door
        tile(&w, 40, 21).set_terrain_slow_factor(0xff);
        o.set_needs_removed(&o, true);
        assert_eq!(tile(&w, 40, 21).terrain_slow_factor(), 0x7f);
    }

    #[test]
    fn trapdoor_blocked_by_solid_above_or_below() {
        let (w, d) = env();
        let o = mk(&w, &d, 60, 20, ITEM_TRAPDOOR, None);
        assert!(!door(&o).blocked());
        solid(&w, 60, 21, true);
        o.world_changed(&o, &mut vec![pos(60, 21)]);
        assert!(door(&o).blocked());
        tile(&w, 60, 21).set_background_contents(75);
        o.world_changed(&o, &mut vec![pos(60, 21)]);
        assert!(!door(&o).blocked());
        solid(&w, 60, 19, true);
        o.world_changed(&o, &mut vec![pos(60, 19)]);
        assert!(door(&o).blocked());
        // isNet objects ignore worldChanged
        let n = o.creation_net_data_for_client(&o, None).unwrap();
        let o2 = Door::init_with_world_dynamic_world_cache_net_data(&w, &d, None, &n).unwrap();
        solid(&w, 60, 19, false);
        o2.world_changed(&o2, &mut vec![pos(60, 19)]);
        assert!(door(&o2).blocked());
        // remoteUpdate copies only `blocked`
        let mut b = n.clone();
        b[0x1a] = 0;
        o2.remote_update(&o2, &b);
        assert!(!door(&o2).blocked());
    }

    #[test]
    fn gate_without_floor_is_dropped() {
        let (w, d) = env();
        let g = mk(&w, &d, 70, 20, ITEM_GATE, Some("go"));
        d.register_with_pos(&g, DYNAMIC_OBJECT_TYPE_DOOR, pos(70, 20));
        solid(&w, 70, 19, true);
        g.world_changed(&g, &mut vec![pos(70, 19)]);
        assert!(!g.needs_removed());
        solid(&w, 70, 19, false);
        g.world_changed(&g, &mut vec![pos(70, 19)]);
        assert!(g.needs_removed());
    }

    #[test]
    fn open_state_and_usage_rules() {
        let (w, d) = env();
        w.set_is_admin(true);
        let o = mk(&w, &d, 40, 20, ITEM_IRON_DOOR, Some("placer"));
        door(&o).set_open_direction(&o, true, 1);
        assert!(door(&o).is_open());
        door(&o).set_open_direction(&o, false, 5);
        assert!(!door(&o).is_open());
        assert_eq!(door(&o).st.borrow().open_direction, 1);
        // connected player (flag NO) always passes
        assert!(door(&o).can_be_used_by_blockhead(&o, &bh(false, Some("other"))));
        // offline simulated blockheads need the placer id
        assert!(!door(&o).can_be_used_by_blockhead(&o, &bh(true, Some("other"))));
        assert!(!door(&o).can_be_used_by_blockhead(&o, &bh(true, None)));
        assert!(door(&o).can_be_used_by_blockhead(&o, &bh(true, Some("placer"))));
        let plain = mk(&w, &d, 50, 20, ITEM_DOOR, None);
        assert!(door(&plain).can_be_used_by_blockhead(&plain, &bh(true, Some("other"))));
    }
}
