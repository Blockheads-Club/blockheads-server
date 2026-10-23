//! `OwnershipSign` (DynamicObjectType 60, interaction type 8, item `ITEM_OWNERSHIP_SIGN` 301): the land
//! ownership / area protection sign. A `Sign` subclass.
//!
//! World side `ownershipSignWasPlacedOrChangedAtPos:...` is owned by world_core.

use std::cell::RefCell;
use std::rc::Rc;

use crate::dynworld::state::DynamicWorld;
use crate::interaction::chest::{dict_str, gunzip_plist, subdata_from, Links};
use crate::interaction::sign::*;
use crate::items::inventory_item::InventoryItem;
use crate::objects::base::{DynamicObject, DynamicObjectBase};
use crate::objects::class_info::ClassInfo;
use crate::objects::handle::{self, CacheRef, ObjRef};
use crate::objects::interaction_object::*;
use crate::objects::protocols::SignTextDynamicObject;
use crate::plist::value::{PlistDict, PlistValue};
use crate::types::structs::IntPair;
use crate::world::state::World;

/// `OwnershipSign: Sign` (instance size 0xe8).
pub static OWNERSHIP_SIGN_CLASS: ClassInfo =
    ClassInfo { name: "OwnershipSign", superclass: Some(&SIGN_CLASS), protocols: &[] };

/// `DYNAMIC_OBJECT_TYPE_OWNERSHIP_SIGN`
pub const DYNAMIC_OBJECT_TYPE_OWNERSHIP_SIGN: i32 = 60;
/// `INTERACTION_OBJECT_TYPE_OWNERSHIP_SIGN`
pub const INTERACTION_OBJECT_TYPE_OWNERSHIP_SIGN: u16 = 8;
/// `ITEM_OWNERSHIP_SIGN`
pub const ITEM_OWNERSHIP_SIGN: i32 = 301;
/// Default radius (omitted from dictionaries).
const DEFAULT_RADIUS: i32 = 15;

/// OwnershipSign ivars 0xd0..0xe8.
pub struct OwnershipSignState {
    /// 0xd0
    pub land_owner_id: Option<String>,
    /// 0xd8
    pub land_owner_name: Option<String>,
    /// 0xe0 (default 15, valid 1..30)
    pub width_radius: i32,
    /// 0xe4 (default 15, valid 1..30)
    pub height_radius: i32,
}

/// `OwnershipSign: Sign`.
pub struct OwnershipSign {
    base: RefCell<DynamicObjectBase>,
    io: RefCell<InteractionObjectState>,
    sign: RefCell<SignState>,
    /// The OwnershipSign ivars.
    pub st: RefCell<OwnershipSignState>,
    links: Links,
}

/// `(int)clamp((float)v, 1.0f, 30.0f)` with the if-chain `hi < v -> hi; v < lo -> lo` (trap 12).
pub fn clamp_radius(v: i32) -> i32 {
    let f = v as f32;
    let mut r = f;
    if 30.0f32 < f {
        r = 30.0;
    }
    if f < 1.0f32 {
        r = 1.0;
    }
    r as i32
}

impl OwnershipSign {
    fn alloc(world: &Rc<World>, dw: &Rc<DynamicWorld>) -> Rc<OwnershipSign> {
        let links = Links::new(world, dw);
        handle::alloc_obj(|w| OwnershipSign {
            base: RefCell::new(DynamicObjectBase::alloc_zeroed_with(w)),
            io: RefCell::new(InteractionObjectState::default()),
            sign: RefCell::new(SignState::default()),
            st: RefCell::new(OwnershipSignState {
                land_owner_id: None,
                land_owner_name: None,
                width_radius: DEFAULT_RADIUS,
                height_radius: DEFAULT_RADIUS,
            }),
            links,
        })
    }

    /// Alias for [`OwnershipSign::new_at_position_item_flipped_save_dict_placed_by_client_client_name`].
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
    ) -> Option<Rc<OwnershipSign>> {
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

    /// `-[OwnershipSign initWithWorld:dynamicWorld:atPosition:cache:item:flipped:saveDict:placedByClient:clientName:]`
    /// The radii stay 15/15 AFTER the Sign constructor ran (trap 12); the owner is nil.
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
    ) -> Option<Rc<OwnershipSign>> {
        let me = Self::alloc(world, dw);
        let me_obj: ObjRef = me.clone();
        let init =
            sign_init_with_world_dynamic_world_at_position_cache_item_flipped_save_dict_placed_by_client_client_name(
                &me_obj,
                pos,
                cache,
                item,
                flipped,
                save_dict,
                client_id,
                client_name,
            );
        init?;
        me.update_text();
        {
            let mut st = me.st.borrow_mut();
            st.width_radius = DEFAULT_RADIUS;
            st.height_radius = DEFAULT_RADIUS;
        }
        Some(me)
    }

    /// `-[OwnershipSign initWithWorld:dynamicWorld:saveDict:cache:]` (radii clamped to 1..30).
    pub fn from_save_dict(
        world: &Rc<World>,
        dw: &Rc<DynamicWorld>,
        save_dict: &PlistDict,
        cache: CacheRef,
    ) -> Option<Rc<OwnershipSign>> {
        let me = Self::alloc(world, dw);
        let me_obj: ObjRef = me.clone();
        sign_init_with_world_dynamic_world_save_dict_cache(&me_obj, save_dict, cache)?;
        if save_dict.map.get(&"landOwnerID".to_string()).is_some() {
            let mut st = me.st.borrow_mut();
            st.land_owner_id =
                save_dict.map.get(&"landOwnerID".to_string()).and_then(|v| v.as_str()).map(|s| s.to_string());
            st.land_owner_name =
                save_dict.map.get(&"landOwnerName".to_string()).and_then(|v| v.as_str()).map(|s| s.to_string());
        }
        {
            let mut st = me.st.borrow_mut();
            st.width_radius = DEFAULT_RADIUS;
            st.height_radius = DEFAULT_RADIUS;
            if let Some(v) = save_dict.map.get(&"w".to_string()) {
                st.width_radius = clamp_radius(v.int_value());
            }
            if let Some(v) = save_dict.map.get(&"h".to_string()) {
                st.height_radius = clamp_radius(v.int_value());
            }
        }
        me.update_text();
        Some(me)
    }

    /// `-[OwnershipSign initWithWorld:dynamicWorld:cache:netData:]` (client-created record).
    pub fn from_net_data(
        world: &Rc<World>,
        dw: &Rc<DynamicWorld>,
        cache: CacheRef,
        net_data: &[u8],
    ) -> Option<Rc<OwnershipSign>> {
        let me = Self::alloc(world, dw);
        let me_obj: ObjRef = me.clone();
        sign_init_with_world_dynamic_world_cache_net_data(&me_obj, cache, net_data)?;
        let dict = gunzip_plist(subdata_from(net_data, 0x30));
        {
            let mut st = me.st.borrow_mut();
            st.land_owner_id = dict_str(dict.as_ref(), "landOwnerID");
            st.land_owner_name = dict_str(dict.as_ref(), "landOwnerName");
            st.width_radius = DEFAULT_RADIUS;
            st.height_radius = DEFAULT_RADIUS;
            if let Some(v) = dict.as_ref().and_then(|d| d.object_for_key("w")) {
                st.width_radius = clamp_radius(v.int_value());
            }
            if let Some(v) = dict.as_ref().and_then(|d| d.object_for_key("h")) {
                st.height_radius = clamp_radius(v.int_value());
            }
        }
        me.update_text();
        let blockheads = dw.blockheads();
        let loaded = blockheads.iter().filter_map(|b| handle::downcast::<crate::blockhead::blockhead::Blockhead>(b));
        loaded.for_each(|bh| bh.net_interaction_object_was_loaded(&me_obj));
        Some(me)
    }

    /// `-landOwnerID`.
    pub fn land_owner_id(&self) -> Option<String> {
        self.st.borrow().land_owner_id.clone()
    }
    /// `-landOwnerName`.
    pub fn land_owner_name(&self) -> Option<String> {
        self.st.borrow().land_owner_name.clone()
    }
    /// `-widthRadius`.
    pub fn width_radius(&self) -> i32 {
        self.st.borrow().width_radius
    }
    /// `-heightRadius`.
    pub fn height_radius(&self) -> i32 {
        self.st.borrow().height_radius
    }
    /// `-connectionType` (inherited from Sign, ivar @0xb8).
    pub fn connection_type(&self) -> i64 {
        self.sign.borrow().connection_type
    }
    /// `-offsetType` (inherited from Sign, ivar @0xc0).
    pub fn offset_type(&self) -> i64 {
        self.sign.borrow().offset_type
    }
    /// `-text` (inherited).
    pub fn text(&self) -> Option<String> {
        self.sign.borrow().text.clone()
    }

    /// `-updateText`.
    pub fn update_text(&self) {
        let text = match self.st.borrow().land_owner_name.clone() {
            None => "\nAREA PROTECTED".to_string(),
            Some(n) => format!("AREA PROTECTED\nOWNED BY\n{}", n),
        };
        self.sign.borrow_mut().text = Some(text);
    }

    fn notify_world(&self, owner: Option<&str>, w: i32, h: i32, removed: bool) {
        let pos = self.base.borrow().pos;
        self.links
            .world()
            .ownership_sign_was_placed_or_changed_at_pos_with_land_owner_width_radius_height_radius_was_removed(
                pos, owner, w, h, removed,
            );
    }

    /// `-setLandOwnerID:name:` (no server caller, S2).
    pub fn set_land_owner_id_name(&self, me: &ObjRef, new_id: Option<&str>, new_name: Option<&str>) {
        let cur = self.land_owner_id();
        if new_id.is_some() || cur.is_none() {
            if new_id.is_none() {
                return;
            }
            if new_name.is_none() {
                return;
            }
            if let Some(c) = &cur {
                if crate::foundation::nsstring::safe_case_insensitive_compare(new_id, Some(c.as_str())) == 0 {
                    return;
                }
            }
        }
        {
            let mut st = self.st.borrow_mut();
            st.land_owner_id = new_id.map(|s| s.to_string());
            st.land_owner_name = new_name.map(|s| s.to_string());
        }
        self.update_text();
        self.links.dw().dynamic_world_changed_at_pos_object_type(me.pos(), me.object_type());
        if !self.base.borrow().is_net {
            let (id, w, h) = (self.land_owner_id(), self.width_radius(), self.height_radius());
            self.notify_world(id.as_deref(), w, h, false);
        }
    }

    /// `-setWidthRadius:` (no server caller, S2).
    pub fn set_width_radius(&self, me: &ObjRef, w: i32) {
        if w != self.width_radius() {
            self.st.borrow_mut().width_radius = clamp_radius(w);
            me.set_update_needs_to_be_sent(true);
            self.links.dw().dynamic_world_changed_at_pos_object_type(me.pos(), me.object_type());
            if !self.base.borrow().is_net {
                let (id, w, h) = (self.land_owner_id(), self.width_radius(), self.height_radius());
                self.notify_world(id.as_deref(), w, h, false);
            }
        }
    }

    /// `-setHeightRadius:` (no server caller, S2).
    pub fn set_height_radius(&self, me: &ObjRef, h: i32) {
        if h != self.height_radius() {
            self.st.borrow_mut().height_radius = clamp_radius(h);
            me.set_update_needs_to_be_sent(true);
            self.links.dw().dynamic_world_changed_at_pos_object_type(me.pos(), me.object_type());
            if !self.base.borrow().is_net {
                let (id, w, h) = (self.land_owner_id(), self.width_radius(), self.height_radius());
                self.notify_world(id.as_deref(), w, h, false);
            }
        }
    }
}

impl SignLike for OwnershipSign {
    fn sign_state(&self) -> &RefCell<SignState> {
        &self.sign
    }
    fn links(&self) -> &Links {
        &self.links
    }
    /// `-isSignSubclass`.
    fn is_sign_subclass(&self) -> bool {
        true
    }
    /// `-netDataExtraDataDict` (owner keys independent of each other; `w` / `h` only when != 15).
    fn net_data_extra_data_dict(&self) -> PlistDict {
        let mut d = sign_net_data_extra_data_dict(&self.me());
        let st = self.st.borrow();
        if let Some(id) = &st.land_owner_id {
            d.map.insert("landOwnerID".to_string(), PlistValue::string(id.clone()));
        }
        if let Some(n) = &st.land_owner_name {
            d.map.insert("landOwnerName".to_string(), PlistValue::string(n.clone()));
        }
        if st.width_radius != DEFAULT_RADIUS {
            d.map.insert("w".to_string(), PlistValue::int(st.width_radius as i64));
        }
        if st.height_radius != DEFAULT_RADIUS {
            d.map.insert("h".to_string(), PlistValue::int(st.height_radius as i64));
        }
        d
    }
    /// `-loadRemoteUpdateDataDict:`. Does NOT call super, `updateText` or touch `updateNeedsToBeSent` (S3).
    fn load_remote_update_data_dict(&self, _me: &ObjRef, dict: &PlistDict) {
        let get = |k: &str| dict.map.get(&k.to_string());
        let new_id = get("landOwnerID").and_then(|v| v.as_str()).map(|s| s.to_string());
        let mut changed = false;
        {
            let cur = self.st.borrow().land_owner_id.clone();
            match (&cur, &new_id) {
                (None, None) => {}
                (Some(c), Some(n)) => {
                    if crate::foundation::nsstring::safe_case_insensitive_compare(Some(c.as_str()), Some(n.as_str()))
                        != 0
                    {
                        changed = true;
                    }
                }
                _ => changed = true,
            }
        }
        let new_name = get("landOwnerName").and_then(|v| v.as_str()).map(|s| s.to_string());
        let nw = get("w").map_or(DEFAULT_RADIUS, |v| clamp_radius(v.int_value()));
        let nh = get("h").map_or(DEFAULT_RADIUS, |v| clamp_radius(v.int_value()));
        {
            let mut st = self.st.borrow_mut();
            st.land_owner_id = new_id;
            st.land_owner_name = new_name;
            if nw != st.width_radius {
                st.width_radius = nw;
                changed = true;
            }
            if nh != st.height_radius {
                st.height_radius = nh;
                changed = true;
            }
        }
        if !self.base.borrow().is_net && changed {
            let (id, w, h) = (self.land_owner_id(), self.width_radius(), self.height_radius());
            self.notify_world(id.as_deref(), w, h, false);
        }
    }
}

impl DynamicObject for OwnershipSign {
    fn base(&self) -> &RefCell<DynamicObjectBase> {
        &self.base
    }
    fn class_info(&self) -> &'static ClassInfo {
        &OWNERSHIP_SIGN_CLASS
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
        DYNAMIC_OBJECT_TYPE_OWNERSHIP_SIGN
    }
    /// `-occupiesForegroundContents`.
    fn occupies_foreground_contents(&self) -> bool {
        true
    }
    fn freeblock_creation_item_type(&self) -> i32 {
        ITEM_OWNERSHIP_SIGN
    }
    fn free_block_creation_save_dict(&self, me: &ObjRef) -> Option<PlistDict> {
        Some(me.get_save_dict(me))
    }
    /// `-getSaveDict` (owner keys only when BOTH are non-nil; radii only when != 15).
    fn get_save_dict(&self, me: &ObjRef) -> PlistDict {
        let mut d = sign_get_save_dict(me);
        let st = self.st.borrow();
        if let (Some(id), Some(n)) = (&st.land_owner_id, &st.land_owner_name) {
            d.map.insert("landOwnerID".to_string(), PlistValue::string(id.clone()));
            d.map.insert("landOwnerName".to_string(), PlistValue::string(n.clone()));
        }
        if st.width_radius != DEFAULT_RADIUS {
            d.map.insert("w".to_string(), PlistValue::int(st.width_radius as i64));
        }
        if st.height_radius != DEFAULT_RADIUS {
            d.map.insert("h".to_string(), PlistValue::int(st.height_radius as i64));
        }
        d
    }
    fn creation_net_data_for_client(&self, me: &ObjRef, client: Option<&str>) -> Option<Vec<u8>> {
        sign_creation_net_data_for_client(me, client)
    }
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
    /// `-setNeedsRemoved:`.
    fn set_needs_removed(&self, me: &ObjRef, v: bool) {
        sign_set_needs_removed(me, v);
        if self.base.borrow().needs_removed && !self.base.borrow().is_net {
            let (w, h) = (self.width_radius(), self.height_radius());
            self.notify_world(None, w, h, true);
        }
    }
    /// `-dealloc`.
    fn dealloc(&self, me: &ObjRef) {
        {
            let mut st = self.st.borrow_mut();
            st.land_owner_id = None;
            st.land_owner_name = None;
        }
        self.sign.borrow_mut().text = None;
        interaction_object_dealloc(me)
    }
    fn blockheads_loaded(&self, me: &ObjRef) {
        interaction_object_blockheads_loaded(me)
    }
    fn blockhead_unloaded(&self, me: &ObjRef, b: &ObjRef) {
        interaction_object_blockhead_unloaded(me, b)
    }
}

impl InteractionObjectLike for OwnershipSign {
    fn io_state(&self) -> &RefCell<InteractionObjectState> {
        &self.io
    }
    /// `-interactionObjectType`.
    fn interaction_object_type(&self) -> u16 {
        INTERACTION_OBJECT_TYPE_OWNERSHIP_SIGN
    }
    fn two_blocks_wide(&self) -> bool {
        self.sign.borrow().offset_type != 0
    }
    /// `-destroyItemType`.
    fn destroy_item_type(&self) -> i32 {
        ITEM_OWNERSHIP_SIGN
    }
    /// `-title`.
    fn title(&self) -> String {
        "OWNERSHIP SIGN".to_string()
    }
    /// `-actionTitle`.
    fn action_title(&self) -> String {
        "OWNERSHIP SIGN".to_string()
    }
    fn requires_human_interaction(&self) -> bool {
        true
    }
    /// `-isPaintable`.
    fn is_paintable(&self) -> bool {
        false
    }
    /// `-canBeUsedByBlockhead:`.
    fn can_be_used_by_blockhead(&self, _me: &ObjRef, b: &ObjRef) -> bool {
        let controlled = b.as_blockhead().map_or(false, |x| x.is_client_blockhead_being_controlled_by_server());
        self.links.world().is_admin() && !controlled
    }
    fn remove(&self, me: &ObjRef, blockhead: Option<&ObjRef>) {
        sign_remove(me, blockhead)
    }
    fn paint(&self, me: &ObjRef, color_index: u16) {
        sign_paint(me, color_index)
    }
}

impl SignTextDynamicObject for OwnershipSign {
    fn text(&self) -> Option<String> {
        OwnershipSign::text(self)
    }
    fn set_text(&self, me: &ObjRef, t: Option<&str>) {
        sign_set_text(me, t)
    }
    fn requires_single_line_text_editing(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::interaction::sign::testkit::*;
    use crate::types::item_type::ItemType;

    fn mk(name: &str) -> (Rc<World>, Rc<DynamicWorld>, Rc<OwnershipSign>) {
        let (w, d, pb) = world_with_block(name);
        for x in 0..32 {
            tile(pb, x, 9).set_type_index(6);
        }
        let it = InventoryItem::new(ItemType(301), 0, 0, None, None);
        let it = it.borrow();
        let s = OwnershipSign::new(
            &w,
            &d,
            IntPair { x: 10, y: 10 },
            None,
            Some(&it),
            false,
            None,
            Some("cid"),
            Some("Bob"),
        )
        .unwrap();
        (w, d, s)
    }

    #[test]
    fn sign_getters_match_sign_placement_info() {
        let (_w, _d, s) = mk("os_getters");
        let (c, o) = s.sign_placement_info().unwrap();
        assert_eq!((s.connection_type(), s.offset_type()), (c.0, o.0));
    }

    #[test]
    fn clamp_if_chain() {
        assert_eq!(clamp_radius(0), 1);
        assert_eq!(clamp_radius(-5), 1);
        assert_eq!(clamp_radius(15), 15);
        assert_eq!(clamp_radius(31), 30);
        assert_eq!(clamp_radius(1000), 30);
    }

    #[test]
    fn constants_and_text() {
        let (_w, _d, s) = mk("os_a");
        assert_eq!(s.object_type(), 60);
        assert_eq!(s.interaction_object_type(), 8);
        assert_eq!(s.destroy_item_type(), 301);
        assert_eq!(s.title(), "OWNERSHIP SIGN");
        assert_eq!(s.action_title(), "OWNERSHIP SIGN");
        assert!(!s.is_paintable());
        assert!(s.is_sign_subclass());
        assert_eq!(s.text().as_deref(), Some("\nAREA PROTECTED"));
        assert_eq!((s.width_radius(), s.height_radius()), (15, 15));
        let me: ObjRef = s.clone();
        s.set_land_owner_id_name(&me, Some("owner1"), Some("Alice"));
        assert_eq!(s.text().as_deref(), Some("AREA PROTECTED\nOWNED BY\nAlice"));
        assert_eq!(me.freeblock_creation_item_type(), 301);
    }

    #[test]
    fn save_dict_owner_pair_and_radii() {
        let (w, d, s) = mk("os_b");
        let me: ObjRef = s.clone();
        let base = me.get_save_dict(&me);
        assert!(base.map.get(&"landOwnerID".to_string()).is_none());
        assert!(base.map.get(&"w".to_string()).is_none());
        {
            let mut st = s.st.borrow_mut();
            st.land_owner_id = Some("o".to_string());
            st.width_radius = 7;
        }
        // only the id: the save dict needs BOTH keys
        let d1 = me.get_save_dict(&me);
        assert!(d1.map.get(&"landOwnerID".to_string()).is_none());
        assert_eq!(d1.map.get(&"w".to_string()).unwrap().int_value(), 7);
        // net extra dict writes each independently
        let extra = s.net_data_extra_data_dict();
        assert!(extra.map.get(&"landOwnerID".to_string()).is_some());
        s.st.borrow_mut().land_owner_name = Some("N".to_string());
        let d2 = me.get_save_dict(&me);
        let s2 = OwnershipSign::from_save_dict(&w, &d, &d2, None).unwrap();
        assert_eq!(s2.land_owner_id().as_deref(), Some("o"));
        assert_eq!(s2.land_owner_name().as_deref(), Some("N"));
        assert_eq!(s2.width_radius(), 7);
        assert_eq!(s2.height_radius(), 15);
        assert_eq!(s2.text().as_deref(), Some("AREA PROTECTED\nOWNED BY\nN"));
        // load clamps
        let mut d3 = d2.clone();
        d3.map.insert("w".to_string(), PlistValue::int(99));
        d3.map.insert("h".to_string(), PlistValue::int(-3));
        let s3 = OwnershipSign::from_save_dict(&w, &d, &d3, None).unwrap();
        assert_eq!((s3.width_radius(), s3.height_radius()), (30, 1));
    }

    #[test]
    fn remote_update_dict_changes() {
        let (_w, _d, s) = mk("os_c");
        let me: ObjRef = s.clone();
        let mut dict = crate::objects::base::new_mutable_dict();
        dict.map.insert("landOwnerID".to_string(), PlistValue::string("OWN"));
        dict.map.insert("landOwnerName".to_string(), PlistValue::string("Zed"));
        dict.map.insert("w".to_string(), PlistValue::int(40));
        s.load_remote_update_data_dict(&me, &dict);
        assert_eq!(s.land_owner_id().as_deref(), Some("OWN"));
        assert_eq!(s.land_owner_name().as_deref(), Some("Zed"));
        assert_eq!((s.width_radius(), s.height_radius()), (30, 15));
        // text stays stale (S3)
        assert_eq!(s.text().as_deref(), Some("\nAREA PROTECTED"));
        // case-only id difference + same radii: no change, but the name is still replaced
        let mut d2 = dict.clone();
        d2.map.insert("landOwnerID".to_string(), PlistValue::string("own"));
        d2.map.insert("landOwnerName".to_string(), PlistValue::string("Zed2"));
        d2.map.insert("w".to_string(), PlistValue::int(30));
        s.load_remote_update_data_dict(&me, &d2);
        assert_eq!(s.land_owner_id().as_deref(), Some("own"));
        assert_eq!(s.land_owner_name().as_deref(), Some("Zed2"));
    }

    #[test]
    fn set_land_owner_rules() {
        let (_w, _d, s) = mk("os_d");
        let me: ObjRef = s.clone();
        s.set_land_owner_id_name(&me, None, Some("x"));
        assert_eq!(s.land_owner_id(), None);
        s.set_land_owner_id_name(&me, Some("a"), None);
        assert_eq!(s.land_owner_id(), None);
        s.set_land_owner_id_name(&me, Some("a"), Some("A"));
        assert_eq!(s.land_owner_id().as_deref(), Some("a"));
        // same owner (case-insensitive): no-op
        s.set_land_owner_id_name(&me, Some("A"), Some("B"));
        assert_eq!(s.land_owner_name().as_deref(), Some("A"));
        // clear
        s.set_land_owner_id_name(&me, None, None);
        assert_eq!(s.land_owner_id(), None);
        assert_eq!(s.text().as_deref(), Some("\nAREA PROTECTED"));
    }

    #[test]
    fn plain_dictionaries_and_virtual_remote_updates() {
        let (w, d, s) = mk("os_plain");
        let me: ObjRef = s.clone();
        // a dictionary without owner keys or radii loads with defaults
        let os = OwnershipSign::from_save_dict(&w, &d, &me.get_save_dict(&me), None).unwrap();
        assert_eq!((os.land_owner_id(), os.width_radius(), os.height_radius()), (None, 15, 15));
        // a lone height radius is saved
        s.set_height_radius(&me, 7);
        let saved = me.get_save_dict(&me);
        assert_eq!(saved.map.get(&"h".to_string()).unwrap().int_value(), 7);
        assert!(saved.map.get(&"w".to_string()).is_none());
        // remote updates arrive through the object
        let mut upd = me.creation_net_data_for_client(&me, None).unwrap();
        me.remote_update(&me, &upd);
        upd.truncate(0x30);
        me.remote_update(&me, &upd);
        assert_eq!(s.width_radius(), 15);
    }

    #[test]
    fn set_needs_removed_notifies_world_only_when_removed() {
        let (_w, _d, s) = mk("os_e");
        let me: ObjRef = s.clone();
        me.set_needs_removed(&me, false);
        assert!(!me.needs_removed());
        me.set_needs_removed(&me, true);
        assert!(me.needs_removed());
    }

    use crate::objects::interaction_object::testutil::TestBh;

    #[test]
    fn replicas_carry_owner_and_radii() {
        let (w, d, _pb) = mk_world("os_net");
        let it = InventoryItem::new(ItemType(301), 0, 0, None, None);
        let s = OwnershipSign::new(&w, &d, IntPair { x: 10, y: 10 }, None, Some(&it.borrow()), false, None, None, None)
            .unwrap();
        let me: ObjRef = s.clone();
        s.set_land_owner_id_name(&me, Some("lo"), Some("Lana"));
        s.set_width_radius(&me, 40);
        s.set_height_radius(&me, 0);
        assert_eq!((s.width_radius(), s.height_radius()), (30, 1));
        let bytes = me.creation_net_data_for_client(&me, None).unwrap();
        assert_eq!(bytes, me.update_net_data_for_client(&me, None).unwrap());
        let (bw, _bd, bdw) = crate::blockhead::blockhead::testutil::setup();
        d.blockheads.borrow_mut().push(crate::blockhead::blockhead::testutil::new_bh(&bw, &bdw, 0));
        assert_eq!(d.blockheads().len(), 1);
        assert!(
            crate::objects::handle::downcast::<crate::blockhead::blockhead::Blockhead>(&d.blockheads()[0]).is_some()
        );
        let rep = OwnershipSign::from_net_data(&w, &d, None, &bytes).unwrap();
        d.blockheads.borrow_mut().clear();
        assert_eq!(rep.land_owner_id().as_deref(), Some("lo"));
        assert_eq!(rep.land_owner_name().as_deref(), Some("Lana"));
        assert_eq!((rep.width_radius(), rep.height_radius()), (30, 1));
        assert_eq!(rep.text().as_deref(), Some("AREA PROTECTED\nOWNED BY\nLana"));
        // a record without owner or radii keeps the defaults
        let bare =
            OwnershipSign::new(&w, &d, IntPair { x: 14, y: 10 }, None, Some(&it.borrow()), false, None, None, None)
                .unwrap();
        let bm: ObjRef = bare.clone();
        let rep2 =
            OwnershipSign::from_net_data(&w, &d, None, &bm.creation_net_data_for_client(&bm, None).unwrap()).unwrap();
        assert_eq!((rep2.width_radius(), rep2.height_radius()), (15, 15));
        assert_eq!(rep2.land_owner_id(), None);
        // refusals
        let mut far = bytes.clone();
        far[8..12].copy_from_slice(&900u32.to_le_bytes());
        far[12..16].copy_from_slice(&900u32.to_le_bytes());
        assert!(OwnershipSign::from_net_data(&w, &d, None, &far).is_none());
        let e = crate::foundation::exception::catch_ns(|| {
            OwnershipSign::from_net_data(&w, &d, None, &bytes[..0x2c]).is_some()
        })
        .unwrap_err();
        assert_eq!(e.name, "NSRangeException");
    }

    fn mk_world(name: &str) -> (Rc<World>, Rc<DynamicWorld>, crate::world::state::PhysicalBlockRef) {
        let (w, d, pb) = world_with_block(name);
        for x in 0..32 {
            tile(pb, x, 9).set_type_index(6);
        }
        (w, d, pb)
    }

    #[test]
    fn setters_tell_the_world_unless_replicated() {
        let (_w, _d, s) = mk("os_setters");
        let me: ObjRef = s.clone();
        // unchanged values do nothing
        s.set_width_radius(&me, 15);
        s.set_height_radius(&me, 15);
        assert!(!me.update_needs_to_be_sent());
        s.set_width_radius(&me, 20);
        s.set_height_radius(&me, 21);
        assert_eq!((s.width_radius(), s.height_radius()), (20, 21));
        assert!(me.update_needs_to_be_sent());
        // a new owner, a case-only change of the same owner, an owner without a name
        s.set_land_owner_id_name(&me, Some("a"), Some("Ann"));
        s.set_land_owner_id_name(&me, Some("b"), Some("Bob"));
        assert_eq!(s.land_owner_name().as_deref(), Some("Bob"));
        s.set_land_owner_id_name(&me, Some("B"), Some("Bobby"));
        assert_eq!(s.land_owner_name().as_deref(), Some("Bob"));
        s.set_land_owner_id_name(&me, Some("c"), None);
        assert_eq!(s.land_owner_id().as_deref(), Some("b"));
        // replicas never tell the world
        s.base.borrow_mut().is_net = true;
        s.set_width_radius(&me, 5);
        s.set_height_radius(&me, 6);
        s.set_land_owner_id_name(&me, Some("d"), Some("Dee"));
        assert_eq!((s.width_radius(), s.height_radius()), (5, 6));
        assert_eq!(s.land_owner_id().as_deref(), Some("d"));
    }

    #[test]
    fn remote_dictionaries_report_changes() {
        let (_w, _d, s) = mk("os_remote");
        let me: ObjRef = s.clone();
        let dict = |id: Option<&str>, w: Option<i64>| {
            let mut d = crate::objects::base::new_mutable_dict();
            if let Some(i) = id {
                d.map.insert("landOwnerID".to_string(), PlistValue::string(i));
            }
            if let Some(w) = w {
                d.map.insert("w".to_string(), PlistValue::int(w));
                d.map.insert("h".to_string(), PlistValue::int(w));
            }
            d
        };
        // none -> none, none -> some, some -> none, some -> same, some -> other
        s.load_remote_update_data_dict(&me, &dict(None, None));
        s.load_remote_update_data_dict(&me, &dict(Some("x"), None));
        assert_eq!(s.land_owner_id().as_deref(), Some("x"));
        s.load_remote_update_data_dict(&me, &dict(None, None));
        assert_eq!(s.land_owner_id(), None);
        s.load_remote_update_data_dict(&me, &dict(Some("x"), Some(4)));
        s.load_remote_update_data_dict(&me, &dict(Some("X"), Some(4)));
        s.load_remote_update_data_dict(&me, &dict(Some("y"), Some(4)));
        assert_eq!((s.width_radius(), s.height_radius()), (4, 4));
        // replicas do not report
        s.base.borrow_mut().is_net = true;
        s.load_remote_update_data_dict(&me, &dict(Some("z"), Some(9)));
        assert_eq!(s.land_owner_id().as_deref(), Some("z"));
    }

    #[test]
    fn object_views_and_removal() {
        let (w, d, pb) = mk_world("os_views");
        let it = InventoryItem::new(ItemType(301), 0, 0, None, None);
        let s = OwnershipSign::new(
            &w,
            &d,
            IntPair { x: 10, y: 10 },
            None,
            Some(&it.borrow()),
            false,
            None,
            Some("c7"),
            None,
        )
        .unwrap();
        let me: ObjRef = s.clone();
        assert_eq!(me.class_info().name, "OwnershipSign");
        assert!(me.as_any().downcast_ref::<OwnershipSign>().is_some());
        assert!(me.as_interaction_object().is_some() && me.as_interaction_like().is_some());
        assert!(me.as_sign_text().is_some());
        assert!(me.should_add_to_macro_block() && me.occupies_foreground_contents());
        assert!(me.free_block_creation_save_dict(&me).is_some());
        assert!(s.requires_human_interaction());
        assert_eq!(s.two_blocks_wide(), s.offset_type() != 0);
        assert!(s.io_state().borrow().current_blockhead.is_none());
        assert_eq!(s.links().world().custom_rules().custom_rules_enabled, 0);
        let st = SignTextDynamicObject::requires_single_line_text_editing(&*s);
        assert!(!st);
        SignTextDynamicObject::set_text(&*s, &me, Some("custom"));
        assert_eq!(SignTextDynamicObject::text(&*s).as_deref(), Some("custom"));
        me.update_accurate_dt_is_simulation(&me, 0.1, 0.1, false);
        me.blockheads_loaded(&me);
        let bh: ObjRef = TestBh::new(1, false);
        me.blockhead_unloaded(&me, &bh);
        me.world_changed(&me, &mut vec![IntPair { x: 30, y: 30 }]);
        // painting and using
        s.paint(&me, 3);
        assert!(me.update_needs_to_be_sent());
        let user: ObjRef = TestBh::new(7, false);
        assert!(!s.can_be_used_by_blockhead(&me, &user));
        w.set_is_admin(true);
        assert!(s.can_be_used_by_blockhead(&me, &user));
        // removal frees the tile and tells the world
        s.remove(&me, None);
        assert!(me.needs_removed());
        assert_eq!(tile(pb, 10, 10).foreground_contents(), 0);
        me.dealloc(&me);
        assert_eq!((s.land_owner_id(), s.text()), (None, None));
        // replicas that are removed stay quiet
        let r = OwnershipSign::new(&w, &d, IntPair { x: 20, y: 10 }, None, Some(&it.borrow()), false, None, None, None)
            .unwrap();
        r.base.borrow_mut().is_net = true;
        let rm: ObjRef = r.clone();
        rm.set_needs_removed(&rm, true);
        assert!(rm.needs_removed());
    }
}
