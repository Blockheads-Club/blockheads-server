//! `Mirror` (DynamicObjectType 64, interaction type 9, item `ITEM_MIRROR` 331): a double-height decorative
//! interaction object with no ivars of its own.
//!

use std::cell::RefCell;
use std::rc::Rc;

use crate::dynworld::state::DynamicWorld;
use crate::interaction::chest::{dict_str, gunzip_plist, gz_plist_bytes, subdata_from, ub_null_tile, Links};
use crate::items::predicates::*;
use crate::objects::base::{padded_bytes, DynamicObject, DynamicObjectBase};
use crate::objects::class_info::{ClassInfo, INTERACTION_OBJECT_CLASS};
use crate::objects::handle::{self, CacheRef, ObjRef};
use crate::objects::interaction_object::*;
use crate::plist::value::{PlistDict, PlistValue};
use crate::types::item_type::ItemType;
use crate::types::structs::IntPair;
use crate::world::helpers::tile_is_solid;
use crate::world::state::World;
use crate::world::tiles::tile_at_world_position_loaded;

/// `Mirror: InteractionObject` (instance size 0xa0).
pub static MIRROR_CLASS: ClassInfo =
    ClassInfo { name: "Mirror", superclass: Some(&INTERACTION_OBJECT_CLASS), protocols: &[] };

/// `DYNAMIC_OBJECT_TYPE_MIRROR`.
pub const DYNAMIC_OBJECT_TYPE_MIRROR: i32 = 64;
/// `INTERACTION_OBJECT_TYPE_MIRROR`.
pub const INTERACTION_OBJECT_TYPE_MIRROR: u16 = 9;
/// `ITEM_MIRROR`
pub const ITEM_MIRROR: i32 = 331;
/// `CONTENTS_INTERACTION_OBJECT`
const CONTENTS_INTERACTION_OBJECT: u8 = 48;

/// `Mirror: InteractionObject`.
pub struct Mirror {
    base: RefCell<DynamicObjectBase>,
    io: RefCell<InteractionObjectState>,
    links: Links,
}

impl Mirror {
    fn alloc(world: &Rc<World>, dw: &Rc<DynamicWorld>) -> Rc<Mirror> {
        let links = Links::new(world, dw);
        handle::alloc_obj(|w| Mirror {
            base: RefCell::new(DynamicObjectBase::alloc_zeroed_with(w)),
            io: RefCell::new(InteractionObjectState::default()),
            links,
        })
    }

    /// `-[Mirror initSubDerivedItems]`: both tiles get `contents = 48`.
    fn init_sub_derived_items(&self) {
        let world = self.links.world();
        let pos = self.base.borrow().pos;
        if let Some(t) = tile_at_world_position_loaded(pos.x, pos.y, &world) {
            t.set_contents(CONTENTS_INTERACTION_OBJECT);
        }
        if let Some(t) = tile_at_world_position_loaded(pos.x, pos.y + 1, &world) {
            t.set_contents(CONTENTS_INTERACTION_OBJECT);
        }
    }

    /// Alias for [`Mirror::new_at_position_item_flipped_save_dict_placed_by_client_client_name`].
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        world: &Rc<World>,
        dw: &Rc<DynamicWorld>,
        pos: IntPair,
        cache: CacheRef,
        item: Option<&crate::items::inventory_item::InventoryItem>,
        flipped: bool,
        save_dict: Option<&PlistDict>,
        client_id: Option<&str>,
        client_name: Option<&str>,
    ) -> Option<Rc<Mirror>> {
        Self::new_at_position_item_flipped_save_dict_placed_by_client_client_name(
            world,
            dw,
            pos,
            cache,
            item,
            flipped,
            save_dict,
            client_id,
            client_name,
        )
    }

    /// `-[Mirror initWithWorld:dynamicWorld:atPosition:cache:item:flipped:saveDict:placedByClient:clientName:]`.
    #[allow(clippy::too_many_arguments)]
    pub fn new_at_position_item_flipped_save_dict_placed_by_client_client_name(
        world: &Rc<World>,
        dw: &Rc<DynamicWorld>,
        pos: IntPair,
        cache: CacheRef,
        item: Option<&crate::items::inventory_item::InventoryItem>,
        flipped: bool,
        save_dict: Option<&PlistDict>,
        client_id: Option<&str>,
        client_name: Option<&str>,
    ) -> Option<Rc<Mirror>> {
        let me = Self::alloc(world, dw);
        let me_obj: ObjRef = me.clone();
        let (w, d) = me.links.refs();
        if !interaction_object_init_with_world_dynamic_world_at_position_cache_item_flipped_save_dict_placed_by_client_client_name(
            &me_obj, &w, &d, pos, cache, item, flipped, save_dict, client_id, client_name,
        ) {
            return None;
        }
        me.init_sub_derived_items();
        Some(me)
    }

    /// `-[Mirror initWithWorld:dynamicWorld:saveDict:cache:]`.
    pub fn from_save_dict(
        world: &Rc<World>,
        dw: &Rc<DynamicWorld>,
        save_dict: &PlistDict,
        cache: CacheRef,
    ) -> Option<Rc<Mirror>> {
        let me = Self::alloc(world, dw);
        let me_obj: ObjRef = me.clone();
        let (w, d) = me.links.refs();
        if !interaction_object_init_with_world_dynamic_world_save_dict_cache(&me_obj, &w, &d, save_dict, cache) {
            return None;
        }
        me.init_sub_derived_items();
        Some(me)
    }

    /// `-[Mirror initWithWorld:dynamicWorld:cache:netData:]`. A record of exactly 0x28 bytes is accepted
    /// (no extra part).
    pub fn from_net_data(
        world: &Rc<World>,
        dw: &Rc<DynamicWorld>,
        cache: CacheRef,
        net_data: &[u8],
    ) -> Option<Rc<Mirror>> {
        let me = Self::alloc(world, dw);
        let me_obj: ObjRef = me.clone();
        let (w, d) = me.links.refs();
        if !interaction_object_init_with_world_dynamic_world_cache_net_data(&me_obj, &w, &d, cache, net_data) {
            return None;
        }
        let _ = padded_bytes(net_data, 0x28); // getBytes:length:0x28, value unused
        if net_data.len() > 0x28 {
            let dict = gunzip_plist(subdata_from(net_data, 0x28));
            me_obj.set_owner_id(dict_str(dict.as_ref(), "ownerID"));
            me.io.borrow_mut().owner_name = dict_str(dict.as_ref(), "ownerName");
        }
        me.init_sub_derived_items();
        for b in dw.blockheads() {
            if let Some(bh) = handle::downcast::<crate::blockhead::blockhead::Blockhead>(&b) {
                bh.net_interaction_object_was_loaded(&me_obj);
            }
        }
        Some(me)
    }
}

impl DynamicObject for Mirror {
    fn base(&self) -> &RefCell<DynamicObjectBase> {
        &self.base
    }
    fn class_info(&self) -> &'static ClassInfo {
        &MIRROR_CLASS
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn as_interaction_object(&self) -> Option<&RefCell<InteractionObjectState>> {
        Some(&self.io)
    }
    fn as_interaction_like(&self) -> Option<&dyn InteractionObjectLike> {
        Some(self)
    }
    fn should_add_to_macro_block(&self) -> bool {
        interaction_object_should_add_to_macro_block(self)
    }
    /// `-objectType`.
    fn object_type(&self) -> i32 {
        DYNAMIC_OBJECT_TYPE_MIRROR
    }
    /// `-occupiesNormalContents`.
    fn occupies_normal_contents(&self) -> bool {
        true
    }
    /// `-freeblockCreationItemType`.
    fn freeblock_creation_item_type(&self) -> i32 {
        ITEM_MIRROR
    }
    /// `-freeBlockCreationSaveDict`.
    fn free_block_creation_save_dict(&self, me: &ObjRef) -> Option<PlistDict> {
        Some(self.get_save_dict(me))
    }
    /// `-getSaveDict` (= super).
    fn get_save_dict(&self, me: &ObjRef) -> PlistDict {
        interaction_object_get_save_dict(me)
    }
    /// `-creationNetDataForClient:` (= update).
    fn creation_net_data_for_client(&self, me: &ObjRef, client: Option<&str>) -> Option<Vec<u8>> {
        self.update_net_data_for_client(me, client)
    }
    /// `-updateNetDataForClient:`: 0x28 struct + gzip plist, the gzip part is ALWAYS appended.
    fn update_net_data_for_client(&self, me: &ObjRef, _client: Option<&str>) -> Option<Vec<u8>> {
        let mut out =
            crate::types::structs::WireStruct::to_vec(&interaction_object_interaction_object_creation_net_data(me));
        let mut dict = PlistValue::mutable_dict();
        if let PlistValue::Dict(dd) = &mut dict {
            if let Some(o) = me.owner_id() {
                dd.map.insert("ownerID".to_string(), PlistValue::string(o));
            }
            if let Some(n) = self.io.borrow().owner_name.clone() {
                dd.map.insert("ownerName".to_string(), PlistValue::string(n));
            }
        }
        out.extend_from_slice(&gz_plist_bytes(&dict));
        Some(out)
    }
    /// `-remoteUpdate:`.
    fn remote_update(&self, me: &ObjRef, net_data: &[u8]) {
        interaction_object_remote_update(me, net_data);
        if self.io.borrow().current_blockhead.is_none() && net_data.len() > 0x28 {
            let dict = gunzip_plist(subdata_from(net_data, 0x28));
            me.set_owner_id(dict_str(dict.as_ref(), "ownerID"));
            self.io.borrow_mut().owner_name = dict_str(dict.as_ref(), "ownerName");
        }
    }
    /// `-update:accurateDT:isSimulation:`.
    fn update_accurate_dt_is_simulation(&self, me: &ObjRef, dt: f32, accurate_dt: f32, is_simulation: bool) {
        if !self.base.borrow().needs_removed {
            interaction_object_update_accurate_dt_is_simulation(me, dt, accurate_dt, is_simulation);
        }
    }
    /// `-worldChanged:`: only `(px, py-1)` matters.
    fn world_changed(&self, me: &ObjRef, positions: &mut Vec<IntPair>) {
        if self.base.borrow().is_net {
            return;
        }
        let pos = self.base.borrow().pos;
        let world = self.links.world();
        for p in positions.clone() {
            if p.x == pos.x && p.y == pos.y - 1 {
                let below = tile_at_world_position_loaded(pos.x, pos.y - 1, &world);
                if below.is_some() && !tile_is_solid(below) {
                    self.remove(me, None);
                }
            }
        }
    }
    /// `-worldContentsChanged:` (empty).
    fn world_contents_changed(&self, _me: &ObjRef, _positions: &mut Vec<IntPair>) {}
    /// `-dealloc` (super only).
    fn dealloc(&self, me: &ObjRef) {
        interaction_object_dealloc(me)
    }
    fn blockheads_loaded(&self, me: &ObjRef) {
        interaction_object_blockheads_loaded(me)
    }
    fn blockhead_unloaded(&self, me: &ObjRef, b: &ObjRef) {
        interaction_object_blockhead_unloaded(me, b)
    }
}

impl InteractionObjectLike for Mirror {
    fn io_state(&self) -> &RefCell<InteractionObjectState> {
        &self.io
    }
    /// `-interactionObjectType`.
    fn interaction_object_type(&self) -> u16 {
        INTERACTION_OBJECT_TYPE_MIRROR
    }
    /// `-isDoubleHeight`.
    fn is_double_height(&self) -> bool {
        true
    }
    /// `-destroyItemType`.
    fn destroy_item_type(&self) -> i32 {
        ITEM_MIRROR
    }
    /// `-title`.
    fn title(&self) -> String {
        "MIRROR".to_string()
    }
    /// `-actionTitle`.
    fn action_title(&self) -> String {
        format!("USE {}", self.title())
    }
    /// `-requiresHumanInteraction`.
    fn requires_human_interaction(&self) -> bool {
        true
    }
    /// `-remove:`.
    fn remove(&self, me: &ObjRef, remove_blockhead: Option<&ObjRef>) {
        let (needs_removed, is_net) = {
            let b = self.base.borrow();
            (b.needs_removed, b.is_net)
        };
        if needs_removed || is_net {
            return;
        }
        let (in_use, cur, remote) = {
            let io = self.io.borrow();
            (io.is_in_use, io.current_blockhead.clone(), io.remote_blockhead_in_use_unique_id)
        };
        if in_use {
            if cur.as_ref().map_or(false, |c| c.is_net()) || remote != 0 {
                self.io.borrow_mut().needs_to_be_removed_when_interaction_ends = true;
                return;
            }
            if let Some(bh) = cur.as_ref().and_then(|c| c.as_blockhead()) {
                bh.stop_interacting();
            }
        }
        let pos = self.base.borrow().pos;
        let dict = self.get_save_dict(me);
        let (world, dw) = (self.links.world(), self.links.dw());
        dw.create_free_block_at_position_of_type_data_a_data_b_sub_items_dynamic_object_save_dict_hovers_play_sound_priority_blockhead(
            pos,
            ItemType(ITEM_MIRROR as u32),
            0,
            0,
            None,
            Some(PlistValue::Dict(dict)),
            false,
            1,
            remove_blockhead,
        );
        world.remove_tile_at_world_x_world_y_create_contents_freeblock_count_create_foreground_contents_freeblock_count_remove_blockhead(pos.x, pos.y, 0, 0, remove_blockhead);
        world.remove_tile_at_world_x_world_y_create_contents_freeblock_count_create_foreground_contents_freeblock_count_remove_blockhead(
            pos.x,
            pos.y + 1,
            0,
            0,
            remove_blockhead,
        );
        self.set_needs_removed(me, true);
    }
}

#[allow(dead_code)]
fn _unused() -> bool {
    ub_null_tile()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dynworld::state::testutil::dw as test_dw;
    use crate::items::inventory_item::InventoryItem;

    fn mk(name: &str) -> (Rc<World>, Rc<DynamicWorld>, Rc<Mirror>) {
        let d = test_dw(name, 4);
        let w = d.world();
        let it = InventoryItem::new(ItemType(331), 0, 0, None, None);
        let it = it.borrow();
        let m = Mirror::new(&w, &d, IntPair { x: 40, y: 3 }, None, Some(&it), true, None, Some("cid"), Some("Bob"))
            .unwrap();
        (w, d, m)
    }

    /// constants and the always-present gzip plist.
    #[test]
    fn constants_and_net_data() {
        let (_w, _d, m) = mk("mirror_a");
        let me: ObjRef = m.clone();
        assert_eq!(m.object_type(), 64);
        assert_eq!(m.interaction_object_type(), 9);
        assert_eq!(m.destroy_item_type(), 331);
        assert_eq!(m.freeblock_creation_item_type(), 331);
        assert!(m.is_double_height() && m.occupies_normal_contents() && m.requires_human_interaction());
        assert_eq!(m.title(), "MIRROR");
        assert_eq!(m.action_title(), "USE MIRROR");
        let b = m.update_net_data_for_client(&me, None).unwrap();
        assert!(b.len() > 0x28);
        let p = gunzip_plist(&b[0x28..]).unwrap();
        assert_eq!(dict_str(Some(&p), "ownerID").as_deref(), Some("cid"));
        assert_eq!(dict_str(Some(&p), "ownerName").as_deref(), Some("Bob"));
        assert_eq!(b, m.creation_net_data_for_client(&me, None).unwrap());
        // ownerless mirror still appends the (empty dict) gzip part
        me.set_owner_id(None);
        m.io.borrow_mut().owner_name = None;
        let b2 = m.update_net_data_for_client(&me, None).unwrap();
        assert!(b2.len() > 0x28);
    }

    #[test]
    fn remote_update_replaces_owner_when_unused() {
        let (_w, _d, m) = mk("mirror_b");
        let me: ObjRef = m.clone();
        let mut upd =
            crate::types::structs::WireStruct::to_vec(&interaction_object_interaction_object_creation_net_data(&me));
        let mut dict = PlistValue::mutable_dict();
        if let PlistValue::Dict(dd) = &mut dict {
            dd.map.insert("ownerID".to_string(), PlistValue::string("zed"));
        }
        upd.extend_from_slice(&gz_plist_bytes(&dict));
        m.remote_update(&me, &upd);
        assert_eq!(me.owner_id().as_deref(), Some("zed"));
        assert_eq!(m.io.borrow().owner_name, None);
        // a bare 0x28 record leaves the owner alone
        m.remote_update(&me, &upd[..0x28]);
        assert_eq!(me.owner_id().as_deref(), Some("zed"));
    }

    #[test]
    fn save_dict_is_super() {
        let (_w, _d, m) = mk("mirror_c");
        let me: ObjRef = m.clone();
        let d = m.get_save_dict(&me);
        assert_eq!(d.map.get(&"interactionObjectType".to_string()).unwrap().int_value(), 9);
        assert!(m.free_block_creation_save_dict(&me).is_some());
    }
}
