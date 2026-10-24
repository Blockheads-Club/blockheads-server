//! `InteractionObject`: abstract base of Bed, Chest, Mirror, Sign, TradePortal, TradingPost, Workbench,...
//!
//! of the class) and "Data used" (`InteractionObjectCreationNetData`). `isNet` follows ERRATUM E5
//! : the `!isNet && needsToBeRemovedWhenInteractionEnds` branches do not fire for client-created
//! objects. Client-only parts (`shader`, `texture`, `interactionRenderItemType`) are not ported.

use std::cell::RefCell;
use std::rc::Rc;

use super::base::{
    base_get_save_dict, base_init_with_world_dynamic_world_at_position_cache,
    base_init_with_world_dynamic_world_cache_net_data, base_init_with_world_dynamic_world_save_dict_cache,
    base_remote_update, padded_bytes, DynamicObject, DynamicWorldRef, WorldRef,
};
use super::handle::{opt_ptr_eq, ptr_eq, CacheRef, ObjRef, ObjWeak};
use crate::items::inventory_item::InventoryItem;
use crate::plist::value::{PlistDict, PlistValue};
use crate::types::netdata::InteractionObjectCreationNetData;
use crate::types::structs::{IntPair, WireStruct};

/// `DYNAMIC_OBJECT_TYPE_INTERACTION_OBJECT_DEPRECATED`.
pub const INTERACTION_OBJECT_OBJECT_TYPE: i32 = 15;

/// Ivars 0x50..0x98 of `InteractionObject` (`shader`/`texture` are client-only and omitted).
#[derive(Default)]
pub struct InteractionObjectState {
    /// 0x50 retained
    pub current_blockhead: Option<ObjRef>,
    /// 0x68
    pub is_in_use: bool,
    /// 0x69
    pub flipped: bool,
    /// 0x70: uniqueID of the using blockhead when it is not yet resolved
    pub remote_blockhead_in_use_unique_id: u64,
    /// 0x78: index into `[dynamicWorld blockheads]`; -1 = none. NOTE the default is 0 (G).
    pub saved_blockhead_index: i32,
    /// 0x80 retained
    pub owner_name: Option<String>,
    /// 0x88
    pub paint_color: u16,
    /// 0x90 assign: non-nil => this object is a child of another dynamic object
    pub proxy_object_owner: Option<ObjWeak>,
    /// 0x98
    pub needs_to_be_removed_when_interaction_ends: bool,
}

/// The overridable `InteractionObject` selectors; every InteractionObject subclass implements it (and returns
/// `Some(self)` from `DynamicObject::as_interaction_like`).
pub trait InteractionObjectLike: DynamicObject {
    /// The `InteractionObject` ivars.
    fn io_state(&self) -> &RefCell<InteractionObjectState>;
    /// `interactionObjectType`: 0 in the base.
    fn interaction_object_type(&self) -> u16 {
        0
    }
    /// `isDoubleHeight`: NO.
    fn is_double_height(&self) -> bool {
        false
    }
    /// `twoBlocksWide`: NO.
    fn two_blocks_wide(&self) -> bool {
        false
    }
    /// `destroyItemType`: ITEM_NONE.
    fn destroy_item_type(&self) -> i32 {
        0
    }
    /// `title`: `@""`.
    fn title(&self) -> String {
        String::new()
    }
    /// `actionTitle`: `[self title]`.
    fn action_title(&self) -> String {
        self.title()
    }
    /// `requiresHumanInteraction`: NO.
    fn requires_human_interaction(&self) -> bool {
        false
    }
    /// `canBeUsedInExpertModeWhenNotOwned`: YES.
    fn can_be_used_in_expert_mode_when_not_owned(&self) -> bool {
        true
    }
    /// `isPaintable`: NO.
    fn is_paintable(&self) -> bool {
        false
    }
    /// `remove:`: empty in the base; Chest/Bed/... override.
    fn remove(&self, _me: &ObjRef, _blockhead: Option<&ObjRef>) {}
    /// `setPaused:`: empty.
    fn set_paused(&self, _me: &ObjRef, _paused: bool) {}
    /// `canBeUsedByBlockhead:`
    fn can_be_used_by_blockhead(&self, me: &ObjRef, b: &ObjRef) -> bool {
        interaction_object_can_be_used_by_blockhead(me, b)
    }
    /// `blockheadWouldLikeToTakeOwnership:withSaveDict:`
    fn blockhead_would_like_to_take_ownership_with_save_dict(
        &self,
        me: &ObjRef,
        b: &ObjRef,
        save_dict: Option<&PlistDict>,
    ) {
        interaction_object_blockhead_would_like_to_take_ownership_with_save_dict(me, b, save_dict)
    }
    /// `startInteractionWithBlockhead:`
    fn start_interaction_with_blockhead(&self, me: &ObjRef, b: &ObjRef) {
        interaction_object_start_interaction_with_blockhead(me, b)
    }
    /// `remoteBlockheadRemovedWithID:`
    fn remote_blockhead_removed_with_id(&self, me: &ObjRef, id: u64) {
        interaction_object_remote_blockhead_removed_with_id(me, id)
    }
    /// `paint:`
    fn paint(&self, me: &ObjRef, color_index: u16) {
        interaction_object_paint(me, color_index)
    }
    /// `setProxyObjectOwner:`
    fn set_proxy_object_owner(&self, me: &ObjRef, p: Option<&ObjRef>) {
        interaction_object_set_proxy_object_owner(me, p)
    }
}

// ---------------------------------------------------------------- helpers

fn io(me: &ObjRef) -> &RefCell<InteractionObjectState> {
    me.as_interaction_object().expect("InteractionObject state missing: not an InteractionObject")
}

fn like(me: &ObjRef) -> &dyn InteractionObjectLike {
    me.as_interaction_like().expect("InteractionObjectLike missing: not an InteractionObject")
}

fn call_remove(me: &ObjRef) {
    like(me).remove(me, None);
}

fn set_blockhead_interaction(b: &Option<ObjRef>, o: Option<&ObjRef>) {
    if let Some(b) = b {
        if let Some(bh) = b.as_blockhead() {
            bh.set_interaction_object(o);
        }
    }
}

/// `notifySave()`.
pub fn interaction_object_notify_save(me: &ObjRef) {
    let proxy = io(me).borrow().proxy_object_owner.clone();
    match proxy {
        None => {
            if let Some(dw) = me.dynamic_world() {
                dw.dynamic_world_changed_at_pos_object_type(me.pos(), me.object_type());
            }
        }
        Some(p) => {
            if let Some(p) = p.upgrade() {
                p.child_needs_saving(&p, me);
            }
        }
    }
}

// ---------------------------------------------------------------- inherent getters

impl dyn DynamicObject {
    /// `currentBlockhead` (nil when not an InteractionObject).
    pub fn current_blockhead(&self) -> Option<ObjRef> {
        self.as_interaction_object().and_then(|s| s.borrow().current_blockhead.clone())
    }
    /// `isInUse`
    pub fn is_in_use(&self) -> bool {
        self.as_interaction_object().is_some_and(|s| s.borrow().is_in_use)
    }
    /// `flipped`
    pub fn flipped(&self) -> bool {
        self.as_interaction_object().is_some_and(|s| s.borrow().flipped)
    }
    /// `ownerName`
    pub fn owner_name(&self) -> Option<String> {
        self.as_interaction_object().and_then(|s| s.borrow().owner_name.clone())
    }
}

// ---------------------------------------------------------------- free functions (super-chain targets)

/// `-[InteractionObject objectType]` = 15.
pub fn interaction_object_object_type() -> i32 {
    INTERACTION_OBJECT_OBJECT_TYPE
}

/// `-[InteractionObject requiresPhysicalBlock]` = NO.
pub fn interaction_object_requires_physical_block() -> bool {
    false
}

/// `-[InteractionObject initWithWorld:dynamicWorld:atPosition:cache:item:flipped:saveDict:placedByClient:clientName:]`.
/// `savedBlockheadIndex` stays 0 (G).
pub fn interaction_object_init_with_world_dynamic_world_at_position_cache_item_flipped_save_dict_placed_by_client_client_name(
    me: &ObjRef,
    world: &WorldRef,
    dw: &DynamicWorldRef,
    pos: IntPair,
    cache: CacheRef,
    _item: Option<&InventoryItem>,
    flipped: bool,
    save_dict: Option<&PlistDict>,
    client_id: Option<&str>,
    client_name: Option<&str>,
) -> bool {
    // the base initialiser always succeeds when the position is given
    let inited = base_init_with_world_dynamic_world_at_position_cache(me, world, dw, pos, cache);
    debug_assert!(inited);
    let sd = |k: &str| save_dict.and_then(|d| d.map.get(&k.to_string()));
    let owner_name = match client_name {
        Some(n) => Some(n.to_string()),
        None => sd("ownerName").and_then(|v| v.as_str()).map(|s| s.to_string()),
    };
    {
        let mut s = io(me).borrow_mut();
        s.flipped = flipped;
        s.owner_name = owner_name;
        s.paint_color = sd("paintColor").map_or(0, |v| v.unsigned_int_value()) as u16;
    }
    if let Some(c) = client_id {
        me.set_owner_id(Some(c.to_string()));
    } else if let Some(v) = sd("ownerID") {
        me.set_owner_id(v.as_str().map(|s| s.to_string()));
    }
    true
}

/// `-[InteractionObject initWithWorld:dynamicWorld:saveDict:cache:]` (load from save).
pub fn interaction_object_init_with_world_dynamic_world_save_dict_cache(
    me: &ObjRef,
    world: &WorldRef,
    dw: &DynamicWorldRef,
    save_dict: &PlistDict,
    cache: CacheRef,
) -> bool {
    let inited = base_init_with_world_dynamic_world_save_dict_cache(me, world, dw, save_dict, cache);
    debug_assert!(inited);
    let sd = |k: &str| save_dict.map.get(&k.to_string());
    let owner_id = sd("ownerID").and_then(|v| v.as_str()).map(|s| s.to_string());
    let mut owner_name = sd("ownerName").and_then(|v| v.as_str()).map(|s| s.to_string());
    {
        let mut s = io(me).borrow_mut();
        s.is_in_use = sd("isInUse").is_some_and(|v| v.bool_value());
        s.flipped = sd("flipped").is_some_and(|v| v.bool_value());
        s.paint_color = sd("paintColor").map_or(0, |v| v.unsigned_int_value()) as u16;
        s.saved_blockhead_index = -1;
        if let Some(v) = sd("currentBlockheadIndex") {
            s.saved_blockhead_index = v.int_value();
        }
    }
    me.set_owner_id(owner_id.clone());
    if dw.is_server() && owner_id.is_some() && owner_name.is_none() {
        owner_name = dw.get_owner_name_for_object_owner_id(owner_id.as_deref().unwrap());
    }
    io(me).borrow_mut().owner_name = owner_name;
    true
}

/// `-[InteractionObject initWithWorld:dynamicWorld:cache:netData:]`: consumes isInUse, flipped, paintColor and
/// isInUseBlockheadUniqueID of the first 0x28 bytes.
pub fn interaction_object_init_with_world_dynamic_world_cache_net_data(
    me: &ObjRef,
    world: &WorldRef,
    dw: &DynamicWorldRef,
    cache: CacheRef,
    net_data: &[u8],
) -> bool {
    if !base_init_with_world_dynamic_world_cache_net_data(me, world, dw, cache, net_data) {
        return false;
    }
    let d = InteractionObjectCreationNetData::read_le(&padded_bytes(net_data, 0x28));
    let mut s = io(me).borrow_mut();
    s.is_in_use = d.is_in_use != 0;
    s.flipped = d.flipped != 0;
    s.paint_color = d.paint_color;
    s.remote_blockhead_in_use_unique_id = d.is_in_use_blockhead_unique_id;
    true
}

/// `-[InteractionObject blockheadsLoaded]`: resolves `savedBlockheadIndex`.
pub fn interaction_object_blockheads_loaded(me: &ObjRef) {
    let saved = io(me).borrow().saved_blockhead_index;
    if saved != -1 {
        let bl = me.dynamic_world().map_or_else(Vec::new, |d| d.blockheads());
        if saved >= 0 && (saved as u32 as usize) < bl.len() {
            let b = bl[saved as usize].clone();
            let mut s = io(me).borrow_mut();
            if !opt_ptr_eq(&s.current_blockhead, &b) {
                s.current_blockhead = Some(b);
            }
        }
        let (in_use, cur) = {
            let mut s = io(me).borrow_mut();
            s.saved_blockhead_index = -1;
            (s.is_in_use, s.current_blockhead.clone())
        };
        if in_use {
            set_blockhead_interaction(&cur, Some(me));
        }
    }
    let mut s = io(me).borrow_mut();
    if s.current_blockhead.is_none() && s.remote_blockhead_in_use_unique_id == 0 {
        s.is_in_use = false;
    }
}

/// `-[InteractionObject getSaveDict]`.
pub fn interaction_object_get_save_dict(me: &ObjRef) -> PlistDict {
    let mut d = base_get_save_dict(me);
    let world_time = me.world().map_or(0.0, |w| w.world_time());
    let (in_use, flipped, paint, cur, owner_name) = {
        let s = io(me).borrow();
        (s.is_in_use, s.flipped, s.paint_color, s.current_blockhead.clone(), s.owner_name.clone())
    };
    d.map.insert("saveTime".to_string(), PlistValue::real_f64(world_time));
    d.map.insert("isInUse".to_string(), PlistValue::boolean(in_use));
    d.map.insert("flipped".to_string(), PlistValue::boolean(flipped));
    d.map.insert("interactionObjectType".to_string(), PlistValue::int(like(me).interaction_object_type() as i64));
    d.map.insert("paintColor".to_string(), PlistValue::int(paint as i64));
    if let Some(cur) = cur {
        let bl = me.dynamic_world().map_or_else(Vec::new, |d| d.blockheads());
        if let Some(i) = bl.iter().position(|b| ptr_eq(b, &cur)) {
            d.map.insert("currentBlockheadIndex".to_string(), PlistValue::int(i as i64));
        }
    }
    if let Some(o) = me.owner_id() {
        d.map.insert("ownerID".to_string(), PlistValue::string(o));
    }
    if let Some(n) = owner_name {
        d.map.insert("ownerName".to_string(), PlistValue::string(n));
    }
    d
}

/// `-[InteractionObject interactionObjectCreationNetData]` (0x28 bytes, padding zero: A).
pub fn interaction_object_interaction_object_creation_net_data(me: &ObjRef) -> InteractionObjectCreationNetData {
    let (in_use, flipped, paint, cur, remote) = {
        let s = io(me).borrow();
        (s.is_in_use, s.flipped, s.paint_color, s.current_blockhead.clone(), s.remote_blockhead_in_use_unique_id)
    };
    let mut r = InteractionObjectCreationNetData::default();
    r.dynamic_object_net_data = me.dynamic_object_net_data(me);
    r.interaction_object_type = like(me).interaction_object_type();
    r.is_in_use = in_use as u8;
    r.flipped = flipped as u8;
    r.paint_color = paint;
    r.is_in_use_blockhead_unique_id = if !in_use {
        0
    } else {
        match cur {
            Some(c) => c.unique_id(),
            None => remote,
        }
    };
    r
}

/// `-[InteractionObject updateNetDataForClient:]`: the 0x28-byte payload; the client id is ignored.
pub fn interaction_object_update_net_data_for_client(me: &ObjRef, _client: Option<&str>) -> Option<Vec<u8>> {
    Some(interaction_object_interaction_object_creation_net_data(me).to_vec())
}

/// `-[InteractionObject creationNetDataForClient:]` = `updateNetDataForClient:`.
pub fn interaction_object_creation_net_data_for_client(me: &ObjRef, client: Option<&str>) -> Option<Vec<u8>> {
    // Pure forwarder to the VIRTUAL `updateNetDataForClient:`.
    me.update_net_data_for_client(me, client)
}

/// `-[InteractionObject dealloc]`: `[currentBlockhead setInteractionObject:nil]` and release of the retained ivars.
pub fn interaction_object_dealloc(me: &ObjRef) {
    let cur = {
        let mut s = io(me).borrow_mut();
        s.owner_name = None;
        s.current_blockhead.take()
    };
    me.set_owner_id(None);
    set_blockhead_interaction(&cur, None);
}

/// `-[InteractionObject update:accurateDT:isSimulation:]`. Note the order difference between the two branches:
/// `notifySave()` before `remove:` in branch 1, after it in branch 2.
pub fn interaction_object_update_accurate_dt_is_simulation(
    me: &ObjRef,
    _dt: f32,
    _accurate_dt: f32,
    _is_simulation: bool,
) {
    if me.needs_removed() {
        return;
    }
    let cur = io(me).borrow().current_blockhead.clone();
    if cur.as_ref().is_some_and(|c| c.needs_removed()) {
        {
            let mut s = io(me).borrow_mut();
            s.current_blockhead = None;
            s.is_in_use = false;
        }
        interaction_object_notify_save(me);
        me.set_update_needs_to_be_sent(true);
        // the ivar is read after notifySave (which may reach a proxy owner's childNeedsSaving:)
        let flag = io(me).borrow().needs_to_be_removed_when_interaction_ends;
        if flag {
            call_remove(me);
        }
    }
    let (in_use, cur) = {
        let s = io(me).borrow();
        (s.is_in_use, s.current_blockhead.clone())
    };
    if in_use {
        if let Some(c) = cur {
            if !c.is_net() {
                let interacting = c.as_blockhead().is_some_and(|b| b.is_interacting_with_any_interaction_object());
                if !interacting {
                    let flag = {
                        let mut s = io(me).borrow_mut();
                        s.is_in_use = false;
                        s.current_blockhead = None;
                        s.needs_to_be_removed_when_interaction_ends
                    };
                    if flag {
                        call_remove(me);
                    }
                    interaction_object_notify_save(me);
                    me.set_update_needs_to_be_sent(true);
                }
            }
        }
    }
}

/// `-[InteractionObject startInteractionWithBlockhead:]`.
pub fn interaction_object_start_interaction_with_blockhead(me: &ObjRef, b: &ObjRef) {
    {
        let mut s = io(me).borrow_mut();
        s.is_in_use = true;
        if !opt_ptr_eq(&s.current_blockhead, b) {
            s.current_blockhead = Some(b.clone());
        }
    }
    interaction_object_notify_save(me);
    me.set_update_needs_to_be_sent(true);
    let cur = io(me).borrow().current_blockhead.clone();
    set_blockhead_interaction(&cur, Some(me));
}

/// `-[InteractionObject remoteUpdate:]` (applies a net update).
pub fn interaction_object_remote_update(me: &ObjRef, net_data: &[u8]) {
    base_remote_update(me, net_data);
    let d = InteractionObjectCreationNetData::read_le(&padded_bytes(net_data, 0x28));
    let was_in_use = {
        let mut s = io(me).borrow_mut();
        let w = s.is_in_use;
        s.is_in_use = d.is_in_use != 0;
        w
    };
    let (in_use, cur) = {
        let s = io(me).borrow();
        (s.is_in_use, s.current_blockhead.clone())
    };
    if in_use && was_in_use {
        let cur_uid = cur.as_ref().map_or(0, |c| c.unique_id());
        if cur_uid != d.is_in_use_blockhead_unique_id {
            if let Some(c) = &cur {
                if let Some(bh) = c.as_blockhead() {
                    bh.stop_interacting();
                }
            }
            let mut s = io(me).borrow_mut();
            s.current_blockhead = None;
            s.is_in_use = true;
        }
    }
    let (in_use, cur_none) = {
        let s = io(me).borrow();
        (s.is_in_use, s.current_blockhead.is_none())
    };
    if in_use && cur_none {
        io(me).borrow_mut().remote_blockhead_in_use_unique_id = d.is_in_use_blockhead_unique_id;
        if d.is_in_use_blockhead_unique_id == 0 {
            io(me).borrow_mut().is_in_use = false;
            me.set_update_needs_to_be_sent(true);
        } else {
            let bl = me.dynamic_world().map_or_else(Vec::new, |d| d.blockheads());
            for b in bl {
                // NOTE: the loop does not break and compares against the (possibly just zeroed) ivar.
                let remote = io(me).borrow().remote_blockhead_in_use_unique_id;
                if b.unique_id() == remote {
                    let mut s = io(me).borrow_mut();
                    s.remote_blockhead_in_use_unique_id = 0;
                    s.is_in_use = false;
                    drop(s);
                    me.set_update_needs_to_be_sent(true);
                }
            }
        }
    }
    let (in_use, flag) = {
        let s = io(me).borrow();
        (s.is_in_use, s.needs_to_be_removed_when_interaction_ends)
    };
    if !in_use && !me.is_net() && flag {
        call_remove(me);
    }
    io(me).borrow_mut().paint_color = d.paint_color;
}

/// `-[InteractionObject remoteBlockheadRemovedWithID:]`.
pub fn interaction_object_remote_blockhead_removed_with_id(me: &ObjRef, id: u64) {
    let matches = io(me).borrow().remote_blockhead_in_use_unique_id == id;
    if matches {
        let flag = {
            let mut s = io(me).borrow_mut();
            s.is_in_use = false;
            s.needs_to_be_removed_when_interaction_ends
        };
        if !me.is_net() && flag {
            call_remove(me);
        }
        io(me).borrow_mut().remote_blockhead_in_use_unique_id = 0;
    }
}

/// `-[InteractionObject canBeUsedByBlockhead:]` = `!isInUse || currentBlockhead == blockhead`.
pub fn interaction_object_can_be_used_by_blockhead(me: &ObjRef, b: &ObjRef) -> bool {
    let s = io(me).borrow();
    !s.is_in_use || opt_ptr_eq(&s.current_blockhead, b)
}

/// `-[InteractionObject blockheadWouldLikeToTakeOwnership:withSaveDict:]` (`saveDict` unused; the
/// `[[world uiManager]...]` send is skipped because `uiManager` is nil on the server).
pub fn interaction_object_blockhead_would_like_to_take_ownership_with_save_dict(
    me: &ObjRef,
    b: &ObjRef,
    _save_dict: Option<&PlistDict>,
) {
    if io(me).borrow().current_blockhead.is_none() {
        {
            let mut s = io(me).borrow_mut();
            s.current_blockhead = Some(b.clone());
            s.is_in_use = true;
        }
        set_blockhead_interaction(&Some(b.clone()), Some(me));
        interaction_object_notify_save(me);
        me.set_update_needs_to_be_sent(true);
    }
}

/// `-[InteractionObject shouldAddToMacroBlock]` = `proxyObjectOwner == nil`.
pub fn interaction_object_should_add_to_macro_block(me: &dyn DynamicObject) -> bool {
    me.as_interaction_object().is_none_or(|s| s.borrow().proxy_object_owner.is_none())
}

/// `-[InteractionObject setProxyObjectOwner:]`: a non-nil owner first removes self from its macro tile; then the
/// (non-retained) ivar is assigned.
// Note: the spec text `if p != nil: [self removeFromMacroBlock]; proxyObjectOwner = p` does not say whether
// the assignment is inside the `if`; assigned unconditionally here (only FreightCar calls this, always with non-nil).
pub fn interaction_object_set_proxy_object_owner(me: &ObjRef, p: Option<&ObjRef>) {
    if p.is_some() {
        me.remove_from_macro_block(me);
    }
    io(me).borrow_mut().proxy_object_owner = p.map(Rc::downgrade);
}

/// `-[InteractionObject blockheadUnloaded:]`.
pub fn interaction_object_blockhead_unloaded(me: &ObjRef, b: &ObjRef) {
    let is_cur = opt_ptr_eq(&io(me).borrow().current_blockhead, b);
    if is_cur {
        let flag = {
            let mut s = io(me).borrow_mut();
            s.current_blockhead = None;
            s.is_in_use = false;
            s.needs_to_be_removed_when_interaction_ends
        };
        if !me.is_net() && flag {
            call_remove(me);
        }
    }
}

/// `-[InteractionObject paint:]`.
pub fn interaction_object_paint(me: &ObjRef, color_index: u16) {
    io(me).borrow_mut().paint_color = color_index;
}

#[cfg(test)]
pub(crate) mod testutil {
    //! A minimal concrete InteractionObject and Blockhead for the unit tests.
    use super::*;
    use crate::objects::base::{BlockheadApi, DynamicObjectBase};
    use crate::objects::class_info::{ClassInfo, DYNAMIC_OBJECT_CLASS, INTERACTION_OBJECT_CLASS};
    use crate::objects::handle;
    use std::any::Any;
    use std::cell::Cell;

    pub struct TestIo {
        pub base: RefCell<DynamicObjectBase>,
        pub io: RefCell<InteractionObjectState>,
        pub removes: RefCell<Vec<String>>,
        pub child_saves: Cell<u32>,
    }
    impl TestIo {
        pub fn new() -> Rc<TestIo> {
            handle::alloc_obj(|w| TestIo {
                base: RefCell::new(DynamicObjectBase::alloc_zeroed_with(w)),
                io: RefCell::new(InteractionObjectState::default()),
                removes: RefCell::new(vec![]),
                child_saves: Cell::new(0),
            })
        }
    }
    impl DynamicObject for TestIo {
        fn base(&self) -> &RefCell<DynamicObjectBase> {
            &self.base
        }
        fn class_info(&self) -> &'static ClassInfo {
            &INTERACTION_OBJECT_CLASS
        }
        fn as_any(&self) -> &dyn Any {
            self
        }
        fn object_type(&self) -> i32 {
            46
        }
        fn should_add_to_macro_block(&self) -> bool {
            interaction_object_should_add_to_macro_block(self)
        }
        fn update_net_data_for_client(&self, me: &ObjRef, client: Option<&str>) -> Option<Vec<u8>> {
            interaction_object_update_net_data_for_client(me, client)
        }
        fn child_needs_saving(&self, _me: &ObjRef, _c: &ObjRef) {
            self.child_saves.set(self.child_saves.get() + 1);
        }
        fn as_interaction_object(&self) -> Option<&RefCell<InteractionObjectState>> {
            Some(&self.io)
        }
        fn as_interaction_like(&self) -> Option<&dyn InteractionObjectLike> {
            Some(self)
        }
    }
    impl InteractionObjectLike for TestIo {
        fn io_state(&self) -> &RefCell<InteractionObjectState> {
            &self.io
        }
        fn interaction_object_type(&self) -> u16 {
            2
        }
        fn remove(&self, _me: &ObjRef, b: Option<&ObjRef>) {
            self.removes.borrow_mut().push(format!("remove({})", b.is_some()));
        }
    }

    pub struct TestBh {
        pub base: RefCell<DynamicObjectBase>,
        pub interacting: Cell<bool>,
        pub log: RefCell<Vec<String>>,
        #[allow(dead_code)] // test fixture field
        pub net: bool,
        pub uid: u64,
    }
    impl TestBh {
        pub fn new(uid: u64, net: bool) -> Rc<TestBh> {
            let b = handle::alloc_obj(|w| TestBh {
                base: RefCell::new(DynamicObjectBase::alloc_zeroed_with(w)),
                interacting: Cell::new(true),
                log: RefCell::new(vec![]),
                net,
                uid,
            });
            b.base.borrow_mut().unique_id = uid;
            b.base.borrow_mut().is_net = net;
            b
        }
    }
    impl DynamicObject for TestBh {
        fn base(&self) -> &RefCell<DynamicObjectBase> {
            &self.base
        }
        fn class_info(&self) -> &'static ClassInfo {
            &DYNAMIC_OBJECT_CLASS
        }
        fn as_any(&self) -> &dyn Any {
            self
        }
        fn as_blockhead(&self) -> Option<&dyn BlockheadApi> {
            Some(self)
        }
    }
    impl BlockheadApi for TestBh {
        fn client_id(&self) -> Option<String> {
            Some(format!("c{}", self.uid))
        }
        fn local_net_id(&self) -> Option<String> {
            Some(format!("c{}", self.uid))
        }
        fn is_client_blockhead_being_controlled_by_server(&self) -> bool {
            false
        }
        fn is_interacting_with_any_interaction_object(&self) -> bool {
            self.interacting.get()
        }
        fn set_interaction_object(&self, o: Option<&ObjRef>) {
            self.log.borrow_mut().push(format!("setIO({})", o.is_some()));
        }
        fn stop_interacting(&self) {
            self.log.borrow_mut().push("stop".into());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::testutil::*;
    use super::*;
    use crate::objects::base::testutil::setup;
    use crate::objects::base::{new_mutable_dict, DynamicWorldApi};

    fn mk(
        width: i32,
    ) -> (
        Rc<TestIo>,
        ObjRef,
        Rc<crate::objects::base::testutil::MockWorld>,
        Rc<crate::objects::base::testutil::MockDw>,
        Rc<RefCell<Vec<String>>>,
    ) {
        let (w, dw, log) = setup(width, 4);
        let o = TestIo::new();
        let me: ObjRef = o.clone();
        let wr: WorldRef = w.clone();
        let dr: DynamicWorldRef = dw.clone();
        assert!(interaction_object_init_with_world_dynamic_world_at_position_cache_item_flipped_save_dict_placed_by_client_client_name(
            &me, &wr, &dr, IntPair { x: 40, y: 3 }, None, None, true, None, Some("cid"), Some("Bob")
        ));
        (o, me, w, dw, log)
    }

    #[test]
    fn defaults_and_init() {
        let (o, me, _w, _dw, _log) = mk(4);
        assert_eq!(o.io.borrow().saved_blockhead_index, 0);
        assert!(me.flipped());
        assert_eq!(me.owner_id().as_deref(), Some("cid"));
        assert_eq!(me.owner_name().as_deref(), Some("Bob"));
        assert_eq!(like(&me).title(), "");
        assert_eq!(like(&me).action_title(), "");
        assert_eq!(interaction_object_object_type(), 15);
        assert!(!interaction_object_requires_physical_block());
    }

    /// : getSaveDict key set and the 0x28 payload round trip.
    #[test]
    fn save_dict_keys_and_net_data() {
        let (o, me, _w, dw, _log) = mk(4);
        let bh = TestBh::new(77, false);
        let b: ObjRef = bh.clone();
        dw.blockheads.borrow_mut().push(b.clone());
        interaction_object_start_interaction_with_blockhead(&me, &b);
        let d = interaction_object_get_save_dict(&me);
        let mut keys: Vec<String> = d.map.keys_in_order();
        keys.sort();
        let mut want: Vec<&str> = vec![
            "uniqueID",
            "pos_x",
            "pos_y",
            "floatPos",
            "saveTime",
            "isInUse",
            "flipped",
            "interactionObjectType",
            "paintColor",
            "currentBlockheadIndex",
            "ownerID",
            "ownerName",
        ];
        want.sort();
        assert_eq!(keys, want);
        assert_eq!(d.map.get(&"currentBlockheadIndex".to_string()).unwrap().int_value(), 0);
        assert_eq!(d.map.get(&"interactionObjectType".to_string()).unwrap().int_value(), 2);
        assert_eq!(d.map.get(&"saveTime".to_string()).unwrap().double_value(), 123.5);
        // payload
        let bytes = interaction_object_update_net_data_for_client(&me, Some("x")).unwrap();
        assert_eq!(bytes.len(), 0x28);
        assert_eq!(interaction_object_creation_net_data_for_client(&me, None).unwrap(), bytes);
        let nd = InteractionObjectCreationNetData::read_le(&bytes);
        assert_eq!(nd.is_in_use, 1);
        assert_eq!(nd.flipped, 1);
        assert_eq!(nd.interaction_object_type, 2);
        assert_eq!(nd.is_in_use_blockhead_unique_id, 77);
        assert_eq!(nd.dynamic_object_net_data.pos_x, 40);
        assert!(bytes[0x11..0x18].iter().all(|x| *x == 0) && bytes[0x26..].iter().all(|x| *x == 0));

        // net-created mirror
        let (w2, dw2, _l2) = setup(4, 4);
        let o2 = TestIo::new();
        let me2: ObjRef = o2.clone();
        let (wr2, dr2): (WorldRef, DynamicWorldRef) = (w2.clone(), dw2.clone());
        assert!(interaction_object_init_with_world_dynamic_world_cache_net_data(&me2, &wr2, &dr2, None, &bytes));
        assert!(me2.is_net());
        assert!(me2.is_in_use() && me2.flipped());
        assert_eq!(o2.io.borrow().remote_blockhead_in_use_unique_id, 77);
        // not in use => id 0
        interaction_object_remote_blockhead_removed_with_id(&me, 5);
        o.io_set_not_in_use();
        let nd = interaction_object_interaction_object_creation_net_data(&me);
        assert_eq!(nd.is_in_use_blockhead_unique_id, 0);
    }

    impl TestIo {
        fn io_set_not_in_use(&self) {
            self.io.borrow_mut().is_in_use = false;
        }
    }

    #[test]
    fn save_dict_round_trip_and_resolution_g() {
        let (_o, me, w, dw, _log) = mk(4);
        let d = interaction_object_get_save_dict(&me);
        assert!(d.map.get(&"currentBlockheadIndex".to_string()).is_none());
        let o2 = TestIo::new();
        let me2: ObjRef = o2.clone();
        let (wr, dr): (WorldRef, DynamicWorldRef) = (w.clone(), dw.clone());
        assert!(interaction_object_init_with_world_dynamic_world_save_dict_cache(&me2, &wr, &dr, &d, None));
        assert_eq!(o2.io.borrow().saved_blockhead_index, -1);
        assert_eq!(me2.owner_id().as_deref(), Some("cid"));
        assert_eq!(me2.owner_name().as_deref(), Some("Bob"));
        // ownerName looked up on the server when missing
        let mut d2 = new_mutable_dict();
        d2.map.insert("ownerID".into(), PlistValue::string("zz"));
        let o3 = TestIo::new();
        let me3: ObjRef = o3.clone();
        interaction_object_init_with_world_dynamic_world_save_dict_cache(&me3, &wr, &dr, &d2, None);
        assert_eq!(me3.owner_name().as_deref(), Some("name-of-zz"));

        // resolution G: a fresh object adopts blockheads[0]
        let bh = TestBh::new(9, false);
        let b: ObjRef = bh.clone();
        dw.blockheads.borrow_mut().push(b.clone());
        interaction_object_blockheads_loaded(&me);
        assert!(opt_ptr_eq(&me.current_blockhead(), &b));
        assert_eq!(_o.io.borrow().saved_blockhead_index, -1);
        assert!(!me.is_in_use());
        // from save with isInUse and an index
        let mut d4 = d.clone();
        d4.map.insert("isInUse".into(), PlistValue::boolean(true));
        d4.map.insert("currentBlockheadIndex".into(), PlistValue::int(0));
        let o4 = TestIo::new();
        let me4: ObjRef = o4.clone();
        interaction_object_init_with_world_dynamic_world_save_dict_cache(&me4, &wr, &dr, &d4, None);
        interaction_object_blockheads_loaded(&me4);
        assert!(me4.is_in_use());
        assert_eq!(bh.log.borrow().last().unwrap(), "setIO(true)");
        // an out-of-range index with no remote id clears isInUse
        let mut d5 = d.clone();
        d5.map.insert("isInUse".into(), PlistValue::boolean(true));
        d5.map.insert("currentBlockheadIndex".into(), PlistValue::int(5));
        let o5 = TestIo::new();
        let me5: ObjRef = o5.clone();
        interaction_object_init_with_world_dynamic_world_save_dict_cache(&me5, &wr, &dr, &d5, None);
        interaction_object_blockheads_loaded(&me5);
        assert!(!me5.is_in_use());
    }

    /// : update branches and their order.
    #[test]
    fn update_branch_order() {
        let (o, me, _w, dw, log) = mk(4);
        let bh = TestBh::new(1, false);
        let b: ObjRef = bh.clone();
        o.io.borrow_mut().needs_to_be_removed_when_interaction_ends = true;
        interaction_object_start_interaction_with_blockhead(&me, &b);
        assert_eq!(*bh.log.borrow(), vec!["setIO(true)".to_string()]);
        assert!(me.update_needs_to_be_sent());
        let _ = &dw;
        // branch 2: blockhead stopped interacting: remove before notify
        bh.interacting.set(false);
        me.set_update_needs_to_be_sent(false);
        log.borrow_mut().clear();
        o.removes.borrow_mut().clear();
        interaction_object_update_accurate_dt_is_simulation(&me, 0.1, 0.1, false);
        assert!(!me.is_in_use());
        assert!(me.current_blockhead().is_none());
        assert_eq!(*o.removes.borrow(), vec!["remove(false)".to_string()]);
        assert_eq!(*log.borrow(), vec!["changed(40,3,46)".to_string()]);
        assert!(me.update_needs_to_be_sent());
        // still interacting: nothing
        interaction_object_start_interaction_with_blockhead(&me, &b);
        bh.interacting.set(true);
        me.set_update_needs_to_be_sent(false);
        interaction_object_update_accurate_dt_is_simulation(&me, 0.1, 0.1, false);
        assert!(me.is_in_use() && !me.update_needs_to_be_sent());
        // net blockheads are not checked
        let nb = TestBh::new(2, true);
        let nbr: ObjRef = nb.clone();
        interaction_object_start_interaction_with_blockhead(&me, &nbr);
        nb.interacting.set(false);
        interaction_object_update_accurate_dt_is_simulation(&me, 0.1, 0.1, false);
        assert!(me.is_in_use());
        // branch 1: blockhead needs removed
        nb.base.borrow_mut().needs_removed = true;
        o.removes.borrow_mut().clear();
        interaction_object_update_accurate_dt_is_simulation(&me, 0.1, 0.1, false);
        assert!(!me.is_in_use() && me.current_blockhead().is_none());
        assert_eq!(o.removes.borrow().len(), 1);
        // needsRemoved short-circuits
        interaction_object_start_interaction_with_blockhead(&me, &nbr);
        me.set_needs_removed(&me, true);
        interaction_object_update_accurate_dt_is_simulation(&me, 0.1, 0.1, false);
        assert!(me.is_in_use());
    }

    #[test]
    fn can_be_used_and_ownership() {
        let (_o, me, _w, _dw, _log) = mk(4);
        let a: ObjRef = TestBh::new(1, false);
        let b: ObjRef = TestBh::new(2, false);
        assert!(interaction_object_can_be_used_by_blockhead(&me, &a));
        interaction_object_blockhead_would_like_to_take_ownership_with_save_dict(&me, &a, None);
        assert!(me.is_in_use());
        assert!(interaction_object_can_be_used_by_blockhead(&me, &a));
        assert!(!interaction_object_can_be_used_by_blockhead(&me, &b));
        // second ownership attempt is ignored
        interaction_object_blockhead_would_like_to_take_ownership_with_save_dict(&me, &b, None);
        assert!(opt_ptr_eq(&me.current_blockhead(), &a));
    }

    /// : blockheadUnloaded with isNet true/false.
    #[test]
    fn blockhead_unloaded_is_net() {
        let (o, me, _w, _dw, _log) = mk(4);
        let a: ObjRef = TestBh::new(1, false);
        o.io.borrow_mut().needs_to_be_removed_when_interaction_ends = true;
        interaction_object_start_interaction_with_blockhead(&me, &a);
        let other: ObjRef = TestBh::new(3, false);
        interaction_object_blockhead_unloaded(&me, &other);
        assert!(me.is_in_use());
        interaction_object_blockhead_unloaded(&me, &a);
        assert!(!me.is_in_use());
        assert_eq!(o.removes.borrow().len(), 1);
        // net-created object: no remove
        me.base().borrow_mut().is_net = true;
        interaction_object_start_interaction_with_blockhead(&me, &a);
        interaction_object_blockhead_unloaded(&me, &a);
        assert_eq!(o.removes.borrow().len(), 1);
        // remote id removal
        o.io.borrow_mut().remote_blockhead_in_use_unique_id = 8;
        o.io.borrow_mut().is_in_use = true;
        interaction_object_remote_blockhead_removed_with_id(&me, 9);
        assert!(me.is_in_use());
        interaction_object_remote_blockhead_removed_with_id(&me, 8);
        assert!(!me.is_in_use());
        assert_eq!(o.io.borrow().remote_blockhead_in_use_unique_id, 0);
        assert_eq!(o.removes.borrow().len(), 1);
        me.base().borrow_mut().is_net = false;
        o.io.borrow_mut().remote_blockhead_in_use_unique_id = 8;
        interaction_object_remote_blockhead_removed_with_id(&me, 8);
        assert_eq!(o.removes.borrow().len(), 2);
    }

    #[test]
    fn remote_update_paths() {
        let (o, me, _w, dw, _log) = mk(4);
        let a: ObjRef = TestBh::new(7, false);
        dw.blockheads.borrow_mut().push(a.clone());
        // client says in use by id 7 (no current blockhead): the matching blockhead exists => cleared (spec quirk)
        let mut nd = interaction_object_interaction_object_creation_net_data(&me);
        nd.is_in_use = 1;
        nd.is_in_use_blockhead_unique_id = 7;
        nd.paint_color = 5;
        me.set_update_needs_to_be_sent(false);
        interaction_object_remote_update(&me, &nd.to_vec());
        assert!(!me.is_in_use());
        assert_eq!(o.io.borrow().remote_blockhead_in_use_unique_id, 0);
        assert!(me.update_needs_to_be_sent());
        assert_eq!(o.io.borrow().paint_color, 5);
        // unknown id stays in use
        nd.is_in_use_blockhead_unique_id = 99;
        interaction_object_remote_update(&me, &nd.to_vec());
        assert!(me.is_in_use());
        assert_eq!(o.io.borrow().remote_blockhead_in_use_unique_id, 99);
        // id 0 while in use clears
        nd.is_in_use_blockhead_unique_id = 0;
        interaction_object_remote_update(&me, &nd.to_vec());
        assert!(!me.is_in_use());
        // different blockhead while in use: stop interacting
        let bh = TestBh::new(1, false);
        let b: ObjRef = bh.clone();
        interaction_object_start_interaction_with_blockhead(&me, &b);
        nd.is_in_use = 1;
        nd.is_in_use_blockhead_unique_id = 2;
        interaction_object_remote_update(&me, &nd.to_vec());
        assert_eq!(bh.log.borrow().last().unwrap(), "stop");
        assert!(me.current_blockhead().is_none());
    }

    #[test]
    fn proxy_owner_and_notify_save() {
        let (o, me, w, dw, log) = mk(4);
        assert_eq!(w.tiles[1].objs.borrow().as_ref().unwrap().len(), 1);
        let parent = TestIo::new();
        let pr: ObjRef = parent.clone();
        log.borrow_mut().clear();
        interaction_object_set_proxy_object_owner(&me, Some(&pr));
        assert_eq!(w.tiles[1].objs.borrow().as_ref().unwrap().len(), 0);
        assert_eq!(*log.borrow(), vec!["changed(40,3,46)".to_string()]);
        assert!(!me.should_add_to_macro_block());
        // after proxy set, notifySave goes to the proxy owner
        log.borrow_mut().clear();
        interaction_object_notify_save(&me);
        assert!(log.borrow().is_empty());
        assert_eq!(parent.child_saves.get(), 1);
        let _ = (&o, &dw);
    }

    #[test]
    fn paint_and_dealloc() {
        let (o, me, _w, _dw, _log) = mk(4);
        interaction_object_paint(&me, 0x1234);
        assert_eq!(o.io.borrow().paint_color, 0x1234);
        let bh = TestBh::new(1, false);
        let b: ObjRef = bh.clone();
        interaction_object_start_interaction_with_blockhead(&me, &b);
        interaction_object_dealloc(&me);
        assert_eq!(bh.log.borrow().last().unwrap(), "setIO(false)");
        assert!(me.current_blockhead().is_none());
        assert!(me.owner_id().is_none());
    }

    /// An interaction object that overrides nothing: the defaults of the trait, and the stand-ins' own faces.
    #[test]
    fn trait_defaults_and_stand_ins() {
        struct Plain {
            base: RefCell<DynamicObjectBase>,
            io: RefCell<InteractionObjectState>,
        }
        impl DynamicObject for Plain {
            fn base(&self) -> &RefCell<DynamicObjectBase> {
                &self.base
            }
            fn class_info(&self) -> &'static ClassInfo {
                &crate::objects::class_info::INTERACTION_OBJECT_CLASS
            }
            fn as_any(&self) -> &dyn Any {
                self
            }
            fn as_interaction_object(&self) -> Option<&RefCell<InteractionObjectState>> {
                Some(&self.io)
            }
            fn as_interaction_like(&self) -> Option<&dyn InteractionObjectLike> {
                Some(self)
            }
        }
        impl InteractionObjectLike for Plain {
            fn io_state(&self) -> &RefCell<InteractionObjectState> {
                &self.io
            }
        }
        use crate::objects::base::DynamicObjectBase;
        use crate::objects::class_info::ClassInfo;
        use std::any::Any;
        let p: Rc<Plain> = crate::objects::handle::alloc_obj(|w| Plain {
            base: RefCell::new(DynamicObjectBase::alloc_zeroed_with(w)),
            io: RefCell::new(InteractionObjectState::default()),
        });
        let me: ObjRef = p.clone();
        let l = like(&me);
        assert_eq!(l.interaction_object_type(), 0);
        assert_eq!(l.destroy_item_type(), 0);
        assert!(!l.requires_human_interaction());
        assert!(l.can_be_used_in_expert_mode_when_not_owned());
        assert!(!l.is_paintable());
        assert!(!l.is_double_height());
        assert!(!l.two_blocks_wide());
        assert_eq!(me.class_info().name, "InteractionObject");
        assert!(me.as_any().is::<Plain>());
        let _ = l.io_state();
        l.remove(&me, None);
        l.set_paused(&me, true);
        l.paint(&me, 5);
        assert_eq!(p.io.borrow().paint_color, 5);
        l.set_proxy_object_owner(&me, Some(&me));
        l.set_proxy_object_owner(&me, None);
        let b: ObjRef = TestBh::new(3, false);
        assert!(l.can_be_used_by_blockhead(&me, &b));
        l.remote_blockhead_removed_with_id(&me, 3);
        // the stand-ins answer for themselves
        let t = TestIo::new();
        let tm: ObjRef = t.clone();
        assert!(std::ptr::eq(InteractionObjectLike::io_state(&*t), &t.io));
        assert_eq!(tm.class_info().name, "InteractionObject");
        assert_eq!(tm.object_type(), 46);
        assert!(tm.as_any().is::<TestIo>());
        let bh = TestBh::new(9, false);
        let bm: ObjRef = bh.clone();
        assert_eq!(bm.class_info().name, "DynamicObject");
        assert!(bm.as_any().is::<TestBh>());
        let api = bm.as_blockhead().unwrap();
        assert_eq!(api.client_id().as_deref(), Some("c9"));
        assert_eq!(api.local_net_id().as_deref(), Some("c9"));
        assert!(!api.is_client_blockhead_being_controlled_by_server());
        assert!(api.is_interacting_with_any_interaction_object());
        let _: Option<&dyn DynamicWorldApi> = None;
    }

    /// A net image whose macro tile is not loaded is refused.
    #[cfg(not(feature = "faithful-crashes"))]
    #[test]
    fn net_image_at_an_unloaded_tile_is_refused() {
        let (w, dw, _log) = setup(4, 4);
        let o = TestIo::new();
        let me: ObjRef = o.clone();
        let (wr, dr): (WorldRef, DynamicWorldRef) = (w.clone(), dw.clone());
        let nd = crate::types::netdata::DynamicObjectNetData::new(9, 4000, 4, false);
        assert!(!interaction_object_init_with_world_dynamic_world_cache_net_data(&me, &wr, &dr, None, &nd.to_vec()));
    }

    /// A save dict may carry the owner when no client placed the object; a flagged object goes once its use ends.
    #[test]
    fn owner_from_the_save_dict_and_deferred_removal() {
        let (w, dw, _log) = setup(4, 4);
        let o = TestIo::new();
        let me: ObjRef = o.clone();
        let (wr, dr): (WorldRef, DynamicWorldRef) = (w.clone(), dw.clone());
        let mut sd = new_mutable_dict();
        sd.map.insert("ownerID".to_string(), PlistValue::string("saved-owner"));
        assert!(interaction_object_init_with_world_dynamic_world_at_position_cache_item_flipped_save_dict_placed_by_client_client_name(
            &me, &wr, &dr, IntPair { x: 40, y: 3 }, None, None, false, Some(&sd), None, None
        ));
        assert_eq!(me.owner_id().as_deref(), Some("saved-owner"));
        // an update that ends the use removes an object flagged for it
        o.io.borrow_mut().needs_to_be_removed_when_interaction_ends = true;
        let img = InteractionObjectCreationNetData { is_in_use: 0, ..InteractionObjectCreationNetData::default() };
        interaction_object_remote_update(&me, &img.to_vec());
        assert_eq!(*o.removes.borrow(), vec!["remove(false)".to_string()]);
    }

    #[test]
    fn owner_name_from_the_save_dict_when_no_client_name_is_given() {
        let (w, dw, _log) = setup(4, 4);
        let o = TestIo::new();
        let me: ObjRef = o.clone();
        let wr: WorldRef = w.clone();
        let dr: DynamicWorldRef = dw.clone();
        let mut sd = crate::objects::base::new_mutable_dict();
        sd.map.insert("ownerName".to_string(), PlistValue::string("Alice"));
        assert!(interaction_object_init_with_world_dynamic_world_at_position_cache_item_flipped_save_dict_placed_by_client_client_name(
            &me, &wr, &dr, IntPair { x: 40, y: 3 }, None, None, false, Some(&sd), Some("cid"), None
        ));
        assert_eq!(me.owner_name().as_deref(), Some("Alice"));
    }

    #[test]
    fn in_use_without_a_known_blockhead() {
        let (o, me, _w, _dw, _log) = mk(4);
        // a second blockheadsLoaded finds the index already resolved
        interaction_object_blockheads_loaded(&me);
        o.io.borrow_mut().saved_blockhead_index = -1;
        interaction_object_blockheads_loaded(&me);
        // in use by somebody the replica has not met yet: the creation image names the remote id
        {
            let mut s = o.io.borrow_mut();
            s.is_in_use = true;
            s.current_blockhead = None;
            s.remote_blockhead_in_use_unique_id = 9;
        }
        let nd = interaction_object_interaction_object_creation_net_data(&me);
        assert_eq!(nd.is_in_use_blockhead_unique_id, 9);
        // the update has nobody to check
        interaction_object_update_accurate_dt_is_simulation(&me, 0.1, 0.1, false);
        assert!(me.is_in_use());
        // a remote update that names another user, while the current one is no blockhead at all
        let plain: ObjRef = crate::objects::base::testutil::TestObj::new();
        o.io.borrow_mut().current_blockhead = Some(plain);
        let mut img = interaction_object_interaction_object_creation_net_data(&me);
        img.is_in_use = 1;
        img.is_in_use_blockhead_unique_id = 12345;
        interaction_object_remote_update(&me, &img.to_vec());
        assert!(me.current_blockhead().is_none());
        // and again with nobody current
        o.io.borrow_mut().is_in_use = true;
        interaction_object_remote_update(&me, &img.to_vec());
    }
}
