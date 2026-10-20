//! `Sign` (DynamicObjectType 47, interaction type 4, item `ITEM_SIGN` 163): the placeable text sign, plus the
//! `SignLike` abstraction and the "super" free functions that `OwnershipSign` chains to.
//!
//!  Conventions");
//!
//! `Sign` and `OwnershipSign` share their behaviour through free functions over `&ObjRef` so that every virtual
//! call (`object_type`, `destroy_item_type`, `set_needs_removed`, `net_data_extra_data_dict`,...) dispatches to
//! the concrete subclass, exactly like the Objective-C message sends.

use std::cell::RefCell;
use std::rc::Rc;

use crate::dynworld::state::DynamicWorld;
use crate::interaction::chest::{dict_int, dict_str, gunzip_plist, gz_plist_bytes, subdata_from, Links};
use crate::interaction::ownership_sign::OwnershipSign;
use crate::items::inventory_item::InventoryItem;
use crate::items::placement::sign_connection_type_and_offset_for_pos;
use crate::objects::base::{padded_bytes, DynamicObject, DynamicObjectBase};
use crate::objects::class_info::{ClassInfo, INTERACTION_OBJECT_CLASS};
use crate::objects::handle::{self, CacheRef, ObjRef};
use crate::objects::interaction_object::*;
use crate::objects::protocols::SignTextDynamicObject;
use crate::plist::value::{PlistDict, PlistValue};
use crate::types::enums::{SignConnectionType, SignOffsetType};
use crate::types::item_type::ItemType;
use crate::types::netdata::SignCreationNetData;
use crate::types::structs::{IntPair, WireStruct};
use crate::world::state::World;
use crate::world::tiles::tile_at_world_position_loaded;

/// `Sign: InteractionObject <SignTextDynamicObject>`.
pub static SIGN_CLASS: ClassInfo =
    ClassInfo { name: "Sign", superclass: Some(&INTERACTION_OBJECT_CLASS), protocols: &["SignTextDynamicObject"] };

/// `DYNAMIC_OBJECT_TYPE_SIGN`
pub const DYNAMIC_OBJECT_TYPE_SIGN: i32 = 47;
/// `INTERACTION_OBJECT_TYPE_SIGN`
pub const INTERACTION_OBJECT_TYPE_SIGN: u16 = 4;
/// `ITEM_SIGN`
pub const ITEM_SIGN: i32 = 163;
/// `CONTENTS_INTERACTION_OBJECT`
const CONTENTS_INTERACTION_OBJECT: u8 = 48;

/// Sign ivars 0xa0..0xc8 (`bitmapString`, `tileDestructTexture`, `blackText` are client only).
#[derive(Default)]
pub struct SignState {
    /// 0xa0
    pub text: Option<String>,
    /// 0xb8 (`SignConnectionType`, NSInteger)
    pub connection_type: i64,
    /// 0xc0 (`SignOffsetType`, NSInteger)
    pub offset_type: i64,
}

/// Common face of `Sign` and `OwnershipSign` (see module docs).
pub trait SignLike: InteractionObjectLike {
    /// The `Sign` ivars.
    fn sign_state(&self) -> &RefCell<SignState>;
    /// World / dynamic world handles.
    fn links(&self) -> &Links;
    /// `-isSignSubclass`: Sign NO, OwnershipSign YES.
    fn is_sign_subclass(&self) -> bool;
    /// `-netDataExtraDataDict`
    fn net_data_extra_data_dict(&self) -> PlistDict;
    /// `-loadRemoteUpdateDataDict:`
    fn load_remote_update_data_dict(&self, me: &ObjRef, dict: &PlistDict);
    /// `Some((connectionType, offsetType))`.
    fn sign_placement_info(&self) -> Option<(SignConnectionType, SignOffsetType)> {
        let s = self.sign_state().borrow();
        Some((SignConnectionType(s.connection_type), SignOffsetType(s.offset_type)))
    }
}

/// Downcast to `Sign` or `OwnershipSign`.
pub fn sign_like(o: &ObjRef) -> Option<&dyn SignLike> {
    if let Some(s) = handle::downcast::<Sign>(o) {
        return Some(s);
    }
    if let Some(s) = handle::downcast::<OwnershipSign>(o) {
        return Some(s);
    }
    None
}

fn sl(me: &ObjRef) -> &dyn SignLike {
    sign_like(me).expect("not a Sign / OwnershipSign")
}

fn io(me: &ObjRef) -> &RefCell<InteractionObjectState> {
    me.as_interaction_object().expect("not an InteractionObject")
}

fn like(me: &ObjRef) -> &dyn InteractionObjectLike {
    me.as_interaction_like().expect("not an InteractionObject")
}

/// `Sign: InteractionObject`.
pub struct Sign {
    base: RefCell<DynamicObjectBase>,
    io: RefCell<InteractionObjectState>,
    /// The Sign ivars.
    pub st: RefCell<SignState>,
    links: Links,
}

impl Sign {
    fn alloc(world: &Rc<World>, dw: &Rc<DynamicWorld>) -> Rc<Sign> {
        let links = Links::new(world, dw);
        handle::alloc_obj(|w| Sign {
            base: RefCell::new(DynamicObjectBase::alloc_zeroed_with(w)),
            io: RefCell::new(InteractionObjectState::default()),
            st: RefCell::new(SignState::default()),
            links,
        })
    }

    /// Alias for [`Sign::new_at_position_item_flipped_save_dict_placed_by_client_client_name`].
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        world: &Rc<World>,
        dw: &Rc<DynamicWorld>,
        pos: IntPair,
        cache: CacheRef,
        item: Option<&InventoryItem>,
        flipped: bool,
        save_dict: Option<&PlistDict>,
        client_id: Option<&str>,
        client_name: Option<&str>,
    ) -> Option<Rc<Sign>> {
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

    /// `-[Sign initWithWorld:dynamicWorld:atPosition:cache:item:flipped:saveDict:placedByClient:clientName:]`.
    #[allow(clippy::too_many_arguments)]
    pub fn new_at_position_item_flipped_save_dict_placed_by_client_client_name(
        world: &Rc<World>,
        dw: &Rc<DynamicWorld>,
        pos: IntPair,
        cache: CacheRef,
        item: Option<&InventoryItem>,
        flipped: bool,
        save_dict: Option<&PlistDict>,
        client_id: Option<&str>,
        client_name: Option<&str>,
    ) -> Option<Rc<Sign>> {
        let me = Self::alloc(world, dw);
        let me_obj: ObjRef = me.clone();
        if !sign_init_with_world_dynamic_world_at_position_cache_item_flipped_save_dict_placed_by_client_client_name(
            &me_obj,
            pos,
            cache,
            item,
            flipped,
            save_dict,
            client_id,
            client_name,
        ) {
            return None;
        }
        Some(me)
    }

    /// `-[Sign initWithWorld:dynamicWorld:saveDict:cache:]`.
    pub fn from_save_dict(
        world: &Rc<World>,
        dw: &Rc<DynamicWorld>,
        save_dict: &PlistDict,
        cache: CacheRef,
    ) -> Option<Rc<Sign>> {
        let me = Self::alloc(world, dw);
        let me_obj: ObjRef = me.clone();
        if !sign_init_with_world_dynamic_world_save_dict_cache(&me_obj, save_dict, cache) {
            return None;
        }
        Some(me)
    }

    /// `-[Sign initWithWorld:dynamicWorld:cache:netData:]`.
    pub fn from_net_data(
        world: &Rc<World>,
        dw: &Rc<DynamicWorld>,
        cache: CacheRef,
        net_data: &[u8],
    ) -> Option<Rc<Sign>> {
        let me = Self::alloc(world, dw);
        let me_obj: ObjRef = me.clone();
        if !sign_init_with_world_dynamic_world_cache_net_data(&me_obj, cache, net_data) {
            return None;
        }
        Some(me)
    }

    /// `-text`
    pub fn text(&self) -> Option<String> {
        self.st.borrow().text.clone()
    }
    /// `-setText:`.
    pub fn set_text(&self, me: &ObjRef, new_text: Option<&str>) {
        sign_set_text(me, new_text)
    }
    /// `-connectionType`.
    pub fn connection_type(&self) -> i64 {
        self.st.borrow().connection_type
    }
    /// `-offsetType`.
    pub fn offset_type(&self) -> i64 {
        self.st.borrow().offset_type
    }
    /// `-twoBlocksWide`.
    pub fn two_blocks_wide(&self) -> bool {
        self.st.borrow().offset_type != 0
    }
}

// =====================================================================================================
// "super" free functions (names = sign_<selector>)
// =====================================================================================================

fn neighbour_pos(pos: IntPair, offset_type: i64) -> Option<IntPair> {
    match offset_type {
        1 => Some(IntPair { x: pos.x + 1, y: pos.y }),
        2 => Some(IntPair { x: pos.x - 1, y: pos.y }),
        _ => None,
    }
}

fn tile_at(me: &ObjRef, p: IntPair) -> Option<crate::world::state::TileRef> {
    let world = sl(me).links().world();
    tile_at_world_position_loaded(p.x, p.y, &world)
}

/// Frees the reserved neighbour tile: `foregroundContents == 48 -> 0`, then ALWAYS `worldContentsChangedAtPos:`.
fn free_neighbour(me: &ObjRef) {
    let (pos, off) = (me.pos(), sl(me).sign_state().borrow().offset_type);
    if let Some(np) = neighbour_pos(pos, off) {
        if let Some(t) = tile_at(me, np) {
            if t.foreground_contents() == CONTENTS_INTERACTION_OBJECT {
                t.set_foreground_contents(0);
            }
        }
        sl(me).links().dw().world_contents_changed_at_pos(np);
    }
}

/// Claims the neighbour tile when empty; `notify` selects "always send" (world_changed) or "only when changed".
fn claim_neighbour(me: &ObjRef, notify_always: bool) {
    let (pos, off) = (me.pos(), sl(me).sign_state().borrow().offset_type);
    if let Some(np) = neighbour_pos(pos, off) {
        let mut changed = false;
        if let Some(t) = tile_at(me, np) {
            if t.foreground_contents() == 0 {
                t.set_foreground_contents(CONTENTS_INTERACTION_OBJECT);
                changed = true;
            }
        }
        if notify_always || changed {
            sl(me).links().dw().world_contents_changed_at_pos(np);
        }
    }
}

/// `-[Sign initSubDerivedItems]` (server part: reserve the tiles).
pub fn sign_init_sub_derived_items(me: &ObjRef) {
    let pos = me.pos();
    if let Some(t) = tile_at(me, pos) {
        if t.foreground_contents() == 0 {
            t.set_foreground_contents(CONTENTS_INTERACTION_OBJECT);
            sl(me).links().dw().world_contents_changed_at_pos(pos);
        }
    }
    claim_neighbour(me, false);
}

/// `-[Sign initWithWorld:dynamicWorld:atPosition:cache:item:flipped:saveDict:placedByClient:clientName:]`.
#[allow(clippy::too_many_arguments)]
pub fn sign_init_with_world_dynamic_world_at_position_cache_item_flipped_save_dict_placed_by_client_client_name(
    me: &ObjRef,
    pos: IntPair,
    cache: CacheRef,
    item: Option<&InventoryItem>,
    flipped: bool,
    save_dict: Option<&PlistDict>,
    client_id: Option<&str>,
    client_name: Option<&str>,
) -> bool {
    let links = sl(me).links().clone();
    let (w, d) = links.refs();
    if !interaction_object_init_with_world_dynamic_world_at_position_cache_item_flipped_save_dict_placed_by_client_client_name(
        me, &w, &d, pos, cache, item, flipped, save_dict, client_id, client_name,
    ) {
        return false;
    }
    let text = save_dict.and_then(|d| d.map.get(&"text".to_string())).and_then(|v| v.as_str()).map(|s| s.to_string());
    sl(me).sign_state().borrow_mut().text = text;
    // no borrow of SignState may be live across the call (it reads our own connection / offset type back)
    let mut offset = SignOffsetType(sl(me).sign_state().borrow().offset_type);
    let world = links.world();
    let conn =
        sign_connection_type_and_offset_for_pos(me.pos(), Some(&mut offset), world.as_ref(), SignConnectionType(0));
    {
        let mut s = sl(me).sign_state().borrow_mut();
        s.connection_type = conn.0;
        s.offset_type = offset.0;
    }
    // reserve the neighbour tile WITHOUT notifying (trap 8)
    if let Some(np) = neighbour_pos(me.pos(), offset.0) {
        if let Some(t) = tile_at(me, np) {
            if t.foreground_contents() == 0 {
                t.set_foreground_contents(CONTENTS_INTERACTION_OBJECT);
            }
        }
    }
    io(me).borrow_mut().flipped = offset.0 == 2;
    sign_init_sub_derived_items(me);
    true
}

/// `-[Sign initWithWorld:dynamicWorld:saveDict:cache:]` (load).
pub fn sign_init_with_world_dynamic_world_save_dict_cache(me: &ObjRef, save_dict: &PlistDict, cache: CacheRef) -> bool {
    let links = sl(me).links().clone();
    let (w, d) = links.refs();
    if !interaction_object_init_with_world_dynamic_world_save_dict_cache(me, &w, &d, save_dict, cache) {
        return false;
    }
    {
        let mut s = sl(me).sign_state().borrow_mut();
        s.text = save_dict.map.get(&"text".to_string()).and_then(|v| v.as_str()).map(|x| x.to_string());
        s.connection_type = dict_int(save_dict, "connectionType") as i64;
        s.offset_type = dict_int(save_dict, "offsetType") as i64;
    }
    sign_init_sub_derived_items(me);
    true
}

/// `-[Sign initWithWorld:dynamicWorld:cache:netData:]`. A record shorter than 0x30 bytes raises `NSRangeException`
/// (`subdataWithRange:`). The blockhead notification loop runs only for non-subclasses.
pub fn sign_init_with_world_dynamic_world_cache_net_data(me: &ObjRef, cache: CacheRef, net_data: &[u8]) -> bool {
    let links = sl(me).links().clone();
    let (w, d) = links.refs();
    if !interaction_object_init_with_world_dynamic_world_cache_net_data(me, &w, &d, cache, net_data) {
        return false;
    }
    let hdr = SignCreationNetData::read_le(&padded_bytes(net_data, 0x30));
    let dict = gunzip_plist(subdata_from(net_data, 0x30));
    {
        let mut s = sl(me).sign_state().borrow_mut();
        s.connection_type = hdr.connection_type as i64;
        s.offset_type = hdr.offset_type as i64;
        s.text = dict_str(dict.as_ref(), "text");
    }
    me.set_owner_id(dict_str(dict.as_ref(), "ownerID"));
    io(me).borrow_mut().owner_name = dict_str(dict.as_ref(), "ownerName");
    sign_init_sub_derived_items(me);
    if !sl(me).is_sign_subclass() {
        for b in links.dw().blockheads() {
            if let Some(bh) = handle::downcast::<crate::blockhead::blockhead::Blockhead>(&b) {
                bh.net_interaction_object_was_loaded(me);
            }
        }
    }
    true
}

/// `-[Sign getSaveDict]`.
pub fn sign_get_save_dict(me: &ObjRef) -> PlistDict {
    let mut d = interaction_object_get_save_dict(me);
    let (text, conn, off) = {
        let s = sl(me).sign_state().borrow();
        (s.text.clone(), s.connection_type, s.offset_type)
    };
    if let Some(t) = text {
        d.map.insert("text".to_string(), PlistValue::string(t));
    }
    if let Some(o) = me.owner_id() {
        d.map.insert("ownerID".to_string(), PlistValue::string(o));
    }
    if let Some(n) = io(me).borrow().owner_name.clone() {
        d.map.insert("ownerName".to_string(), PlistValue::string(n));
    }
    d.map.insert("connectionType".to_string(), PlistValue::int(conn as i32 as i64));
    d.map.insert("offsetType".to_string(), PlistValue::int(off as i32 as i64));
    d
}

/// `-[Sign netDataExtraDataDict]` (the Sign version; subclasses extend it).
pub fn sign_net_data_extra_data_dict(me: &ObjRef) -> PlistDict {
    let mut d = crate::objects::base::new_mutable_dict();
    if let Some(t) = sl(me).sign_state().borrow().text.clone() {
        d.map.insert("text".to_string(), PlistValue::string(t));
    }
    if let Some(o) = me.owner_id() {
        d.map.insert("ownerID".to_string(), PlistValue::string(o));
    }
    if let Some(n) = io(me).borrow().owner_name.clone() {
        d.map.insert("ownerName".to_string(), PlistValue::string(n));
    }
    d
}

/// `-[Sign creationNetDataForClient:]` (= `updateNetDataForClient:`).
pub fn sign_creation_net_data_for_client(me: &ObjRef, _client: Option<&str>) -> Option<Vec<u8>> {
    let base = interaction_object_interaction_object_creation_net_data(me);
    let (conn, off) = {
        let s = sl(me).sign_state().borrow();
        (s.connection_type, s.offset_type)
    };
    let mut d = SignCreationNetData::default();
    d.interaction_object_creation_net_data = base;
    d.connection_type = conn as u8;
    d.offset_type = off as u8;
    let mut out = d.to_vec();
    let extra = PlistValue::Dict(sl(me).net_data_extra_data_dict());
    out.extend_from_slice(&gz_plist_bytes(&extra));
    Some(out)
}

/// `-[Sign setText:]` (trap 6: a nil text never equals, so it always clears; no `updateNeedsToBeSent`).
pub fn sign_set_text(me: &ObjRef, new_text: Option<&str>) {
    let changed = {
        let s = sl(me).sign_state().borrow();
        match (new_text, s.text.as_deref()) {
            (Some(n), Some(o)) => n != o,
            _ => true,
        }
    };
    if changed {
        sl(me).sign_state().borrow_mut().text = new_text.map(|s| s.to_string());
        sl(me).links().dw().dynamic_world_changed_at_pos_object_type(me.pos(), me.object_type());
    }
}

/// `-[Sign loadRemoteUpdateDataDict:]`.
pub fn sign_load_remote_update_data_dict(me: &ObjRef, dict: &PlistDict) {
    let get = |k: &str| dict.map.get(&k.to_string()).and_then(|v| v.as_str()).map(|s| s.to_string());
    sign_set_text(me, get("text").as_deref());
    me.set_owner_id(get("ownerID"));
    io(me).borrow_mut().owner_name = get("ownerName");
}

/// `-[Sign remoteUpdate:]`.
pub fn sign_remote_update(me: &ObjRef, net_data: &[u8]) {
    interaction_object_remote_update(me, net_data);
    if io(me).borrow().current_blockhead.is_none() {
        let dict = gunzip_plist(subdata_from(net_data, 0x30));
        let d = match dict {
            Some(PlistValue::Dict(d)) => d,
            _ => crate::objects::base::new_mutable_dict(),
        };
        sl(me).load_remote_update_data_dict(me, &d);
    }
    let hdr = SignCreationNetData::read_le(&padded_bytes(net_data, 0x30));
    let mut s = sl(me).sign_state().borrow_mut();
    s.connection_type = hdr.connection_type as i64;
    s.offset_type = hdr.offset_type as i64;
}

/// `-[Sign update:accurateDT:isSimulation:]`.
pub fn sign_update_accurate_dt_is_simulation(me: &ObjRef, dt: f32, accurate_dt: f32, is_simulation: bool) {
    if !me.needs_removed() {
        interaction_object_update_accurate_dt_is_simulation(me, dt, accurate_dt, is_simulation);
    }
}

/// `-[Sign worldChanged:]` (3x3 re-validation; no early return, S5).
pub fn sign_world_changed(me: &ObjRef, positions: &mut Vec<IntPair>) {
    if me.is_net() {
        return;
    }
    let links = sl(me).links().clone();
    let world = links.world();
    for cp in positions.clone() {
        let pos = me.pos();
        if !(pos.x - 1 <= cp.x && cp.x <= pos.x + 1 && pos.y - 1 <= cp.y && cp.y <= pos.y + 1) {
            continue;
        }
        let (cur_conn, cur_off) = {
            let s = sl(me).sign_state().borrow();
            (s.connection_type, s.offset_type)
        };
        let mut new_off = SignOffsetType(cur_off);
        // no mutable borrow of SignState is live here: the call reads the sign back through the dynamic world
        let new_conn = sign_connection_type_and_offset_for_pos(
            pos,
            Some(&mut new_off),
            world.as_ref(),
            SignConnectionType(cur_conn),
        );
        if new_conn.0 != cur_conn || new_off.0 != cur_off {
            free_neighbour(me);
            sl(me).sign_state().borrow_mut().offset_type = new_off.0;
            if new_conn.0 != 0 {
                claim_neighbour(me, true);
            }
            io(me).borrow_mut().flipped = new_off.0 == 2;
            sl(me).sign_state().borrow_mut().connection_type = new_conn.0;
            me.set_update_needs_to_be_sent(true);
            links.dw().dynamic_world_changed_at_pos_object_type(pos, me.object_type());
        }
        if sl(me).sign_state().borrow().connection_type == 0 {
            like(me).remove(me, None);
        }
    }
}

/// `-[Sign remove:]`.
pub fn sign_remove(me: &ObjRef, remove_blockhead: Option<&ObjRef>) {
    if me.needs_removed() || me.is_net() {
        return;
    }
    let (in_use, cur, remote) = {
        let s = io(me).borrow();
        (s.is_in_use, s.current_blockhead.clone(), s.remote_blockhead_in_use_unique_id)
    };
    if in_use {
        if cur.as_ref().map_or(false, |c| c.is_net()) || remote != 0 {
            io(me).borrow_mut().needs_to_be_removed_when_interaction_ends = true;
            return;
        }
        if let Some(bh) = cur.as_ref().and_then(|c| c.as_blockhead()) {
            bh.stop_interacting();
        }
    }
    free_neighbour(me);
    let links = sl(me).links().clone();
    let (world, dw) = (links.world(), links.dw());
    let pos = me.pos();
    let save_dict = me.get_save_dict(me);
    dw.create_free_block_at_position_of_type_data_a_data_b_sub_items_dynamic_object_save_dict_hovers_play_sound_priority_blockhead(
        pos,
        ItemType(like(me).destroy_item_type() as u32),
        0,
        0,
        None,
        Some(PlistValue::Dict(save_dict)),
        false,
        1,
        remove_blockhead,
    );
    world.remove_tile_at_world_x_world_y_create_contents_freeblock_count_create_foreground_contents_freeblock_count_remove_blockhead_only_remove_c_ontents_only_remove_foreground_contents(
        pos.x,
        pos.y,
        0,
        0,
        remove_blockhead,
        false,
        true,
    );
    me.set_needs_removed(me, true);
}

/// `-[Sign setNeedsRemoved:]` (trap 7: frees the neighbour even when `flag == false`).
pub fn sign_set_needs_removed(me: &ObjRef, flag: bool) {
    free_neighbour(me);
    me.base().borrow_mut().needs_removed = flag;
}

/// `-[Sign canBeUsedByBlockhead:]`.
pub fn sign_can_be_used_by_blockhead(me: &ObjRef, b: &ObjRef) -> bool {
    let controlled = b.as_blockhead().map_or(false, |x| x.is_client_blockhead_being_controlled_by_server());
    if sl(me).links().world().is_admin() && !controlled {
        return true;
    }
    let mut ok = interaction_object_can_be_used_by_blockhead(me, b);
    if ok {
        if let Some(owner) = me.owner_id() {
            let local = b.as_blockhead().and_then(|x| x.local_net_id());
            ok = local.as_deref() == Some(owner.as_str());
        }
    }
    ok
}

/// `-[Sign paint:]`.
pub fn sign_paint(me: &ObjRef, color_index: u16) {
    io(me).borrow_mut().paint_color = color_index;
    if !me.is_net() {
        me.set_update_needs_to_be_sent(true);
        sl(me).links().dw().dynamic_world_changed_at_pos_object_type(me.pos(), me.object_type());
    }
}

// =====================================================================================================
// trait impls
// =====================================================================================================

impl SignLike for Sign {
    fn sign_state(&self) -> &RefCell<SignState> {
        &self.st
    }
    fn links(&self) -> &Links {
        &self.links
    }
    fn is_sign_subclass(&self) -> bool {
        false
    }
    fn net_data_extra_data_dict(&self) -> PlistDict {
        sign_net_data_extra_data_dict(&self.me())
    }
    fn load_remote_update_data_dict(&self, me: &ObjRef, dict: &PlistDict) {
        sign_load_remote_update_data_dict(me, dict)
    }
}

impl DynamicObject for Sign {
    fn base(&self) -> &RefCell<DynamicObjectBase> {
        &self.base
    }
    fn class_info(&self) -> &'static ClassInfo {
        &SIGN_CLASS
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
    fn as_sign_text(&self) -> Option<&dyn SignTextDynamicObject> {
        Some(self)
    }
    fn should_add_to_macro_block(&self) -> bool {
        interaction_object_should_add_to_macro_block(self)
    }
    /// `-objectType`.
    fn object_type(&self) -> i32 {
        DYNAMIC_OBJECT_TYPE_SIGN
    }
    /// `-occupiesForegroundContents`.
    fn occupies_foreground_contents(&self) -> bool {
        true
    }
    /// `-freeblockCreationItemType` (= virtual `destroyItemType`).
    fn freeblock_creation_item_type(&self) -> i32 {
        like(&self.me()).destroy_item_type()
    }
    /// `-freeBlockCreationSaveDict`.
    fn free_block_creation_save_dict(&self, me: &ObjRef) -> Option<PlistDict> {
        Some(me.get_save_dict(me))
    }
    fn get_save_dict(&self, me: &ObjRef) -> PlistDict {
        sign_get_save_dict(me)
    }
    fn creation_net_data_for_client(&self, me: &ObjRef, client: Option<&str>) -> Option<Vec<u8>> {
        sign_creation_net_data_for_client(me, client)
    }
    /// `-updateNetDataForClient:` (= creation).
    fn update_net_data_for_client(&self, me: &ObjRef, client: Option<&str>) -> Option<Vec<u8>> {
        me.creation_net_data_for_client(me, client)
    }
    fn remote_update(&self, me: &ObjRef, net_data: &[u8]) {
        sign_remote_update(me, net_data)
    }
    fn update_accurate_dt_is_simulation(&self, me: &ObjRef, dt: f32, accurate_dt: f32, is_simulation: bool) {
        sign_update_accurate_dt_is_simulation(me, dt, accurate_dt, is_simulation)
    }
    fn world_changed(&self, me: &ObjRef, positions: &mut Vec<IntPair>) {
        sign_world_changed(me, positions)
    }
    fn set_needs_removed(&self, me: &ObjRef, v: bool) {
        sign_set_needs_removed(me, v)
    }
    fn dealloc(&self, me: &ObjRef) {
        self.st.borrow_mut().text = None;
        interaction_object_dealloc(me)
    }
    fn blockheads_loaded(&self, me: &ObjRef) {
        interaction_object_blockheads_loaded(me)
    }
    fn blockhead_unloaded(&self, me: &ObjRef, b: &ObjRef) {
        interaction_object_blockhead_unloaded(me, b)
    }
}

impl InteractionObjectLike for Sign {
    fn io_state(&self) -> &RefCell<InteractionObjectState> {
        &self.io
    }
    /// `-interactionObjectType`.
    fn interaction_object_type(&self) -> u16 {
        INTERACTION_OBJECT_TYPE_SIGN
    }
    fn two_blocks_wide(&self) -> bool {
        Sign::two_blocks_wide(self)
    }
    /// `-destroyItemType`.
    fn destroy_item_type(&self) -> i32 {
        ITEM_SIGN
    }
    fn title(&self) -> String {
        "SIGN".to_string()
    }
    fn action_title(&self) -> String {
        "CHANGE SIGN".to_string()
    }
    fn requires_human_interaction(&self) -> bool {
        true
    }
    fn is_paintable(&self) -> bool {
        true
    }
    fn can_be_used_by_blockhead(&self, me: &ObjRef, b: &ObjRef) -> bool {
        sign_can_be_used_by_blockhead(me, b)
    }
    fn remove(&self, me: &ObjRef, blockhead: Option<&ObjRef>) {
        sign_remove(me, blockhead)
    }
    fn paint(&self, me: &ObjRef, color_index: u16) {
        sign_paint(me, color_index)
    }
}

impl SignTextDynamicObject for Sign {
    fn text(&self) -> Option<String> {
        Sign::text(self)
    }
    fn set_text(&self, me: &ObjRef, t: Option<&str>) {
        sign_set_text(me, t)
    }
    /// `-requiresSingleLineTextEditing`.
    fn requires_single_line_text_editing(&self) -> bool {
        false
    }
}

#[cfg(test)]
pub(crate) mod testkit {
    use super::*;
    use crate::dynworld::state::testutil::dw;
    use crate::world::state::{PhysicalBlockRef, TileRef};

    /// A world + dynamic world with a fully valid block at macro (0,0) whose tiles are all AIR.
    pub fn world_with_block(name: &str) -> (Rc<World>, Rc<DynamicWorld>, PhysicalBlockRef) {
        let d = dw(name, 4);
        let w = d.world();
        let pb = w.take_physical_block();
        pb.valid.set(true);
        *w.macro_tiles().get(0).unwrap().physical_block.borrow_mut() = Some(pb);
        for i in 0..1024 {
            TileRef::new(pb, i).set_type_index(2);
        }
        (w, d, pb)
    }
    pub fn tile(pb: PhysicalBlockRef, x: i32, y: i32) -> TileRef {
        TileRef::new(pb, (y * 32 + x) as usize)
    }
}

#[cfg(test)]
mod tests {
    use super::testkit::*;
    use super::*;

    fn sign_at(w: &Rc<World>, d: &Rc<DynamicWorld>, x: i32, y: i32, text: Option<&str>) -> Rc<Sign> {
        let it = InventoryItem::new(ItemType(163), 0, 0, None, None);
        let it = it.borrow();
        let mut sd = crate::objects::base::new_mutable_dict();
        if let Some(t) = text {
            sd.map.insert("text".to_string(), PlistValue::string(t));
        }
        Sign::new(w, d, IntPair { x, y }, None, Some(&it), false, Some(&sd), Some("cid"), Some("Bob")).unwrap()
    }

    #[test]
    fn placement_on_ground_sets_connection_and_reserves() {
        let (w, d, pb) = world_with_block("sign_a");
        // solid ground under the sign
        for x in 0..32 {
            tile(pb, x, 9).set_type_index(6);
        }
        let s = sign_at(&w, &d, 10, 10, Some("hi"));
        assert_eq!(s.text().as_deref(), Some("hi"));
        assert_ne!(s.connection_type(), 0, "ground connection expected");
        // anchor tile claimed
        assert_eq!(tile(pb, 10, 10).foreground_contents(), 48);
        let off = s.offset_type();
        assert_eq!(s.io.borrow().flipped, off == 2);
        if off == 1 {
            assert_eq!(tile(pb, 11, 10).foreground_contents(), 48);
        } else if off == 2 {
            assert_eq!(tile(pb, 9, 10).foreground_contents(), 48);
        }
        assert_eq!(s.two_blocks_wide(), off != 0);
    }

    #[test]
    fn save_dict_keys_and_round_trip() {
        let (w, d, pb) = world_with_block("sign_b");
        for x in 0..32 {
            tile(pb, x, 9).set_type_index(6);
        }
        let s = sign_at(&w, &d, 10, 10, Some("hello"));
        let me: ObjRef = s.clone();
        let dict = me.get_save_dict(&me);
        for k in ["text", "ownerID", "ownerName", "connectionType", "offsetType", "interactionObjectType"] {
            assert!(dict.map.get(&k.to_string()).is_some(), "{k}");
        }
        assert_eq!(dict_str(Some(&PlistValue::Dict(dict.clone())), "text").as_deref(), Some("hello"));
        let s2 = Sign::from_save_dict(&w, &d, &dict, None).unwrap();
        assert_eq!(s2.text().as_deref(), Some("hello"));
        assert_eq!(s2.connection_type(), s.connection_type());
        assert_eq!(s2.offset_type(), s.offset_type());
    }

    #[test]
    fn set_text_semantics_and_net_data() {
        let (w, d, pb) = world_with_block("sign_c");
        for x in 0..32 {
            tile(pb, x, 9).set_type_index(6);
        }
        let s = sign_at(&w, &d, 10, 10, None);
        let me: ObjRef = s.clone();
        s.set_text(&me, Some("a"));
        assert_eq!(s.text().as_deref(), Some("a"));
        assert!(!me.update_needs_to_be_sent());
        s.set_text(&me, None);
        assert_eq!(s.text(), None);
        s.set_text(&me, Some("zz"));
        let b = me.creation_net_data_for_client(&me, None).unwrap();
        assert_eq!(b, me.update_net_data_for_client(&me, None).unwrap());
        assert_eq!(b[0x28] as i64, s.connection_type());
        assert_eq!(b[0x29] as i64, s.offset_type());
        assert!(b[0x2a..0x30].iter().all(|x| *x == 0));
        let p = gunzip_plist(&b[0x30..]).unwrap();
        assert_eq!(dict_str(Some(&p), "text").as_deref(), Some("zz"));
        assert_eq!(dict_str(Some(&p), "ownerID").as_deref(), Some("cid"));
    }

    #[test]
    fn remote_update_applies_text_and_owner_when_unused() {
        let (w, d, pb) = world_with_block("sign_d");
        for x in 0..32 {
            tile(pb, x, 9).set_type_index(6);
        }
        let s = sign_at(&w, &d, 10, 10, Some("old"));
        let me: ObjRef = s.clone();
        let mut upd = sign_creation_net_data_for_client(&me, None).unwrap();
        upd.truncate(0x30);
        let mut dict = PlistValue::mutable_dict();
        if let PlistValue::Dict(dd) = &mut dict {
            dd.map.insert("text".to_string(), PlistValue::string("new"));
        }
        upd.extend_from_slice(&gz_plist_bytes(&dict));
        me.remote_update(&me, &upd);
        assert_eq!(s.text().as_deref(), Some("new"));
        assert_eq!(me.owner_id(), None);
        assert_eq!(me.owner_name(), None);
    }

    #[test]
    fn set_needs_removed_frees_neighbour_even_when_false() {
        let (w, d, pb) = world_with_block("sign_e");
        let s = sign_at(&w, &d, 10, 10, None);
        let me: ObjRef = s.clone();
        s.st.borrow_mut().offset_type = 1;
        tile(pb, 11, 10).set_foreground_contents(48);
        me.set_needs_removed(&me, false);
        assert_eq!(tile(pb, 11, 10).foreground_contents(), 0);
        assert!(!me.needs_removed());
        me.set_needs_removed(&me, true);
        assert!(me.needs_removed());
    }

    #[test]
    fn world_changed_ignores_far_positions_and_net_signs() {
        let (w, d, pb) = world_with_block("sign_f");
        for x in 0..32 {
            tile(pb, x, 9).set_type_index(6);
        }
        let s = sign_at(&w, &d, 10, 10, None);
        let me: ObjRef = s.clone();
        let conn = s.connection_type();
        me.world_changed(&me, &mut vec![IntPair { x: 20, y: 20 }]);
        assert_eq!(s.connection_type(), conn);
        assert!(!me.update_needs_to_be_sent());
        me.base().borrow_mut().is_net = true;
        me.world_changed(&me, &mut vec![IntPair { x: 10, y: 9 }]);
        assert_eq!(s.connection_type(), conn);
    }

    #[test]
    fn constants() {
        let (w, d, _pb) = world_with_block("sign_g");
        let s = sign_at(&w, &d, 10, 10, None);
        assert_eq!(s.object_type(), 47);
        assert_eq!(s.interaction_object_type(), 4);
        assert_eq!(s.destroy_item_type(), 163);
        assert_eq!(s.freeblock_creation_item_type(), 163);
        assert_eq!(s.title(), "SIGN");
        assert_eq!(s.action_title(), "CHANGE SIGN");
        assert!(s.occupies_foreground_contents() && s.is_paintable() && s.requires_human_interaction());
        assert!(!SignTextDynamicObject::requires_single_line_text_editing(&*s));
        let me: ObjRef = s.clone();
        assert!(sign_like(&me).is_some());
        assert!(me.as_sign_text().is_some());
    }
}
