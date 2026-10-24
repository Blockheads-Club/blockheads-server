//! `Chest` (DynamicObjectType 46, interaction type 2): the seven container variants (chest, safe, shelf, golden
//! chest, portal chest, display cabinet, feeder chest).
//!
//!
//! This file also hosts the small helpers shared by the other interaction object files (`Links`, plist blob
//! helpers, `subdata_from`, the E4 null-tile helper).

use std::cell::RefCell;
use std::collections::BTreeSet;
use std::rc::{Rc, Weak};

use crate::dynworld::state::DynamicWorld;
use crate::foundation::exception::range_exception_beyond_size;
use crate::foundation::gzip::{gzip_deflate, gzip_inflate};
use crate::interaction::trading_post::support_lost;
use crate::items::inventory_item::{InventoryItem, ItemRef, SubSlot};
use crate::items::predicates::{item_type_is_stackable, item_type_is_workbench};
use crate::nslog;
use crate::objects::base::{
    base_remove_from_macro_block, base_update_position, new_mutable_dict, padded_bytes, DynamicObject,
    DynamicObjectBase, DynamicWorldRef, WorldRef,
};
use crate::objects::class_info::{ClassInfo, INTERACTION_OBJECT_CLASS};
use crate::objects::handle::{self, CacheRef, ObjRef};
use crate::objects::interaction_object::*;
use crate::plist::helpers::{data_from_property_list, property_list_from_data};
use crate::plist::value::{PlistDict, PlistValue};
use crate::types::enums::ChestType;
use crate::types::netdata::ChestNetData;
use crate::types::structs::{IntPair, WireStruct};
use crate::world::loot::{item_type_from_chest_type, number_of_items_for_chest_type};
use crate::world::state::World;
use crate::world::tiles::tile_at_world_position_loaded;

// =====================================================================================================
// Shared helpers (used by every file of this module)
// =====================================================================================================

/// The concrete `World` / `DynamicWorld` handles of an interaction object (the object_model base only keeps
/// `dyn` handles, which cannot reach `removeTile...`, `createFreeBlock...` etc.). Both are `assign` (weak) in
/// the original.
#[derive(Clone)]
pub struct Links {
    pub world: Weak<World>,
    pub dw: Weak<DynamicWorld>,
}

impl Links {
    /// Links for a freshly allocated object.
    pub fn new(world: &Rc<World>, dw: &Rc<DynamicWorld>) -> Links {
        Links { world: Rc::downgrade(world), dw: Rc::downgrade(dw) }
    }
    /// `[self world]`
    pub fn world(&self) -> Rc<World> {
        self.world.upgrade().expect("interaction object: world dropped")
    }
    /// `[self dynamicWorld]`
    pub fn dw(&self) -> Rc<DynamicWorld> {
        self.dw.upgrade().expect("interaction object: dynamic world dropped")
    }
    /// Coercions for the object_model init functions.
    pub fn refs(&self) -> (WorldRef, DynamicWorldRef) {
        (self.world(), self.dw())
    }
}

/// `UB-PARITY` (E4, module decision 2.6): the original dereferences a NULL `Tile*`. With feature
/// `faithful-crashes` raise SIGSEGV; otherwise the caller treats the check as failed. Always returns `false`.
pub fn ub_null_tile() -> bool {
    #[cfg(feature = "faithful-crashes")]
    crate::foundation::sys::faithful_segv();
    false
}

/// A mutable `NSArray` built from the given elements.
pub fn mutable_array_of(items: Vec<PlistValue>) -> PlistValue {
    let mut v = PlistValue::mutable_array();
    if let PlistValue::Array(a) = &mut v {
        a.items = items;
    }
    v
}

/// `[dataFromPropertyList(v) gzipDeflate]`; a nil plist yields nil, which appends nothing.
pub fn gz_plist_bytes(v: &PlistValue) -> Vec<u8> {
    data_from_property_list(v).map(|b| gzip_deflate(&b)).unwrap_or_default()
}

/// `propertyListFromData([data gzipInflate])` (nil on any failure).
pub fn gunzip_plist(data: &[u8]) -> Option<PlistValue> {
    let inflated = gzip_inflate(data)?;
    property_list_from_data(&inflated)
}

/// `[netData subdataWithRange:NSMakeRange(loc, length - loc)]`; raises `NSRangeException` when `loc > length`
/// (: the length wraps as an unsigned subtraction).
pub fn subdata_from(data: &[u8], loc: usize) -> &[u8] {
    if loc > data.len() {
        range_exception_beyond_size(
            "subdataWithRange:",
            loc as u64,
            (data.len() as u64).wrapping_sub(loc as u64),
            data.len() as u64,
        );
    }
    &data[loc..]
}

/// `[dict objectForKey:key]` as a string (non-strings read as nil).
pub fn dict_str(d: Option<&PlistValue>, key: &str) -> Option<String> {
    d.and_then(|d| d.object_for_key(key)).and_then(|v| v.as_str()).map(|s| s.to_string())
}

/// `[[dict objectForKey:key] intValue]` (missing = 0).
pub fn dict_int(d: &PlistDict, key: &str) -> i32 {
    d.map.get(&key.to_string()).map_or(0, |v| v.int_value())
}

// =====================================================================================================
// Class, constants, state
// =====================================================================================================

/// `Chest: InteractionObject` (instance size 0xd0).
pub static CHEST_CLASS: ClassInfo =
    ClassInfo { name: "Chest", superclass: Some(&INTERACTION_OBJECT_CLASS), protocols: &[] };

/// `DYNAMIC_OBJECT_TYPE_CHEST`.
pub const DYNAMIC_OBJECT_TYPE_CHEST: i32 = 46;
/// `INTERACTION_OBJECT_TYPE_CHEST`.
pub const INTERACTION_OBJECT_TYPE_CHEST: u16 = 2;

/// `ChestType` values.
pub const CHEST_STANDARD: i32 = 0;
pub const CHEST_SAFE: i32 = 1;
pub const CHEST_SHELF: i32 = 2;
pub const CHEST_GOLD: i32 = 3;
pub const CHEST_PORTAL: i32 = 4;
pub const CHEST_DISPLAY_CABINET: i32 = 5;
pub const CHEST_FEEDER: i32 = 6;

/// `ITEM_TIME_CRYSTAL`: filtered out of every chest load path.
const ITEM_TIME_CRYSTAL: u32 = 0x0b;
const ITEM_SAFE: u32 = 0x429;
const ITEM_SHELF: u32 = 0xa1;
const ITEM_DISPLAY_CABINET: u32 = 0xcf;
const ITEM_CHEST_GOLD: u32 = 0x430;
const ITEM_CHEST_FEEDER: u32 = 0x450;
const ITEM_CHEST_PORTAL: u32 = 0x432;
/// Maximum stack size of one chest slot.
const MAX_STACK: i32 = 99;

/// Chest ivars 0xa0..0xd0.
pub struct ChestState {
    /// 0xa0: `None` for PORTAL and while not lazily loaded; N slots otherwise.
    pub inventory_items: Option<Vec<SubSlot>>,
    /// 0xa8, never written
    pub inventory_changed: bool,
    /// 0xac
    pub chest_type: i32,
    /// 0xb0
    pub inventory_needs_to_be_sent_with_update: bool,
    /// 0xb4
    pub shelf_render_items: [i32; 4],
    /// 0xc4
    pub shelf_item_data_bs: [u16; 4],
}

/// `Chest: InteractionObject`.
pub struct Chest {
    base: RefCell<DynamicObjectBase>,
    io: RefCell<InteractionObjectState>,
    /// The Chest ivars.
    pub st: RefCell<ChestState>,
    links: Links,
}

fn new_slot() -> SubSlot {
    Rc::new(RefCell::new(Vec::new()))
}

fn slots_for(chest_type: i32) -> usize {
    number_of_items_for_chest_type(ChestType(chest_type as u32)) as usize
}

/// Builds the item list of one saved slot (`NSArray` of `NSData`), skipping `ITEM_TIME_CRYSTAL`.
fn load_slot_items(saved_slot: Option<&PlistValue>) -> Vec<ItemRef> {
    let mut out = Vec::new();
    if let Some(PlistValue::Array(a)) = saved_slot {
        for e in &a.items {
            if let PlistValue::Data(d) = e {
                let it = InventoryItem::from_save_data(&d.bytes);
                if it.borrow().item_type.0 != ITEM_TIME_CRYSTAL {
                    out.push(it);
                }
            }
        }
    }
    out
}

/// `saveItemSlots[i]` through `objectAtIndex:` (raises on a short array).
fn saved_slot_at(arr: &PlistValue, i: usize) -> Option<&PlistValue> {
    match arr {
        PlistValue::Array(a) => {
            if i >= a.items.len() {
                crate::foundation::exception::range_exception_index(i as u64, a.items.len() as u64, "objectAtIndex:");
            }
            Some(&a.items[i])
        }
        _ => None,
    }
}

fn array_count(v: Option<&PlistValue>) -> usize {
    match v {
        Some(PlistValue::Array(a)) => a.items.len(),
        _ => 0,
    }
}

/// `saveItemSlots` plist from raw per-item blobs (outer = slots).
pub fn slots_plist_from_blobs(slots: Vec<Vec<Vec<u8>>>) -> PlistValue {
    mutable_array_of(
        slots.into_iter().map(|s| mutable_array_of(s.into_iter().map(PlistValue::data).collect())).collect(),
    )
}

impl Chest {
    fn alloc(world: &Rc<World>, dw: &Rc<DynamicWorld>) -> Rc<Chest> {
        let links = Links::new(world, dw);
        handle::alloc_obj(|w| Chest {
            base: RefCell::new(DynamicObjectBase::alloc_zeroed_with(w)),
            io: RefCell::new(InteractionObjectState::default()),
            st: RefCell::new(ChestState {
                inventory_items: None,
                inventory_changed: false,
                chest_type: CHEST_STANDARD,
                inventory_needs_to_be_sent_with_update: false,
                shelf_render_items: [0; 4],
                shelf_item_data_bs: [0; 4],
            }),
            links,
        })
    }

    fn world(&self) -> Rc<World> {
        self.links.world()
    }
    fn dw(&self) -> Rc<DynamicWorld> {
        self.links.dw()
    }

    /// `-[Chest initSubDerivedItems]`: render-cache refresh only; nothing to do on the server.
    fn init_sub_derived_items(&self) {}

    /// Alias for [`Chest::new_at_position_item_flipped_save_dict_placed_by_client_client_name`].
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
    ) -> Option<Rc<Chest>> {
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

    /// `-[Chest initWithWorld:dynamicWorld:atPosition:cache:item:flipped:saveDict:placedByClient:clientName:]`
    /// (placement / creation from an inventory item).
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
    ) -> Option<Rc<Chest>> {
        let me = Self::alloc(world, dw);
        let me_obj: ObjRef = me.clone();
        let (w, d) = me.links.refs();
        // the base initialiser always succeeds here, `None` is only the ObjC `self = nil` shape
        interaction_object_init_with_world_dynamic_world_at_position_cache_item_flipped_save_dict_placed_by_client_client_name(
            &me_obj, &w, &d, pos, cache, item, flipped, save_dict, client_id, client_name,
        )
            .then_some(())?;
        let item_type = item.map_or(0, |i| i.item_type.0);
        let mut chest_type = CHEST_STANDARD;
        match item_type {
            ITEM_SAFE => chest_type = CHEST_SAFE,
            ITEM_SHELF => chest_type = CHEST_SHELF,
            ITEM_DISPLAY_CABINET => chest_type = CHEST_DISPLAY_CABINET,
            ITEM_CHEST_GOLD => chest_type = CHEST_GOLD,
            ITEM_CHEST_FEEDER => chest_type = CHEST_FEEDER,
            ITEM_CHEST_PORTAL => {
                if world.custom_rules().custom_rules_enabled == 0 {
                    chest_type = CHEST_PORTAL;
                }
            }
            _ => {}
        }
        let n = slots_for(chest_type);
        let saved_slots = save_dict.and_then(|d| d.map.get(&"saveItemSlots".to_string()));
        let safe_client_id = || {
            save_dict
                .and_then(|d| d.map.get(&"safeClientID".to_string()))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
        };
        let mut inv: Option<Vec<SubSlot>> = Some(Vec::with_capacity(n));
        if chest_type == CHEST_PORTAL {
            inv = None;
        } else if array_count(saved_slots) == n {
            let saved = saved_slots.unwrap();
            let v = inv.as_mut().unwrap();
            for i in 0..n {
                v.push(Rc::new(RefCell::new(load_slot_items(saved_slot_at(saved, i)))));
            }
            if me_obj.owner_id().is_none() && (item_type == ITEM_SAFE || item_type == ITEM_DISPLAY_CABINET) {
                if let Some(s) = safe_client_id() {
                    me_obj.set_owner_id(Some(s));
                }
            }
        } else {
            let sub = item.and_then(|i| i.sub_items.as_ref());
            let v = inv.as_mut().unwrap();
            if let Some(sub) = sub.filter(|s| s.len() == n) {
                // The SAME InventoryItem objects are shared (arrayWithArray:), no 0xb filter.
                for s in sub.iter().take(n) {
                    v.push(Rc::new(RefCell::new(s.borrow().clone())));
                }
                if item_type == ITEM_SAFE || item_type == ITEM_DISPLAY_CABINET {
                    chest_type = if item_type == ITEM_SAFE { CHEST_SAFE } else { CHEST_DISPLAY_CABINET };
                    if me_obj.owner_id().is_none() {
                        if let Some(s) = safe_client_id() {
                            me_obj.set_owner_id(Some(s));
                        }
                    }
                }
            } else {
                for _ in 0..n {
                    v.push(new_slot());
                }
            }
        }
        {
            let mut st = me.st.borrow_mut();
            st.chest_type = chest_type;
            st.inventory_items = inv;
        }
        me.init_sub_derived_items();
        Some(me)
    }

    /// `-[Chest initWithWorld:dynamicWorld:saveDict:cache:]` (load from world save).
    pub fn from_save_dict(
        world: &Rc<World>,
        dw: &Rc<DynamicWorld>,
        save_dict: &PlistDict,
        cache: CacheRef,
    ) -> Option<Rc<Chest>> {
        let me = Self::alloc(world, dw);
        let me_obj: ObjRef = me.clone();
        let (w, d) = me.links.refs();
        // the base initialiser always succeeds here, `None` is only the ObjC `self = nil` shape
        interaction_object_init_with_world_dynamic_world_save_dict_cache(&me_obj, &w, &d, save_dict, cache)
            .then_some(())?;
        let mut chest_type = dict_int(save_dict, "chestType");
        if chest_type == CHEST_PORTAL && world.custom_rules().custom_rules_enabled != 0 {
            chest_type = CHEST_STANDARD;
        }
        if me_obj.owner_id().is_none() {
            me_obj.set_owner_id(
                save_dict.map.get(&"safeClientID".to_string()).and_then(|v| v.as_str()).map(|s| s.to_string()),
            );
        }
        let n = slots_for(chest_type);
        let mut inv: Option<Vec<SubSlot>> = None;
        if chest_type != CHEST_PORTAL {
            if let Some(saved) = save_dict.map.get(&"saveItemSlots".to_string()) {
                // legacy: inventory stored inside the dict; force a re-save so it migrates to separate storage
                dw.dynamic_world_changed_at_pos_object_type(me_obj.pos(), DYNAMIC_OBJECT_TYPE_CHEST);
                let mut v = Vec::with_capacity(n);
                if array_count(Some(saved)) >= n {
                    for i in 0..n {
                        v.push(Rc::new(RefCell::new(load_slot_items(saved_slot_at(saved, i)))));
                    }
                } else {
                    for _ in 0..n {
                        v.push(new_slot());
                    }
                }
                inv = Some(v);
            }
        }
        {
            let mut st = me.st.borrow_mut();
            st.chest_type = chest_type;
            if inv.is_none() {
                for i in 0..4 {
                    st.shelf_render_items[i] = dict_int(save_dict, &format!("shelfRenderItems_{}", i));
                    st.shelf_item_data_bs[i] = dict_int(save_dict, &format!("shelfItemDataBs_{}", i)) as u16;
                }
            }
            st.inventory_items = inv;
        }
        me.init_sub_derived_items();
        Some(me)
    }

    /// `-[Chest initWithWorld:dynamicWorld:cache:netData:]` (client-created chest, FreightCar child).
    /// A record shorter than 0x40 bytes raises `NSRangeException` from `subdataWithRange:`.
    pub fn from_net_data(
        world: &Rc<World>,
        dw: &Rc<DynamicWorld>,
        cache: CacheRef,
        net_data: &[u8],
    ) -> Option<Rc<Chest>> {
        let me = Self::alloc(world, dw);
        let me_obj: ObjRef = me.clone();
        let (w, d) = me.links.refs();
        if !interaction_object_init_with_world_dynamic_world_cache_net_data(&me_obj, &w, &d, cache, net_data) {
            return None;
        }
        let cnd = ChestNetData::read_le(&padded_bytes(net_data, 0x40));
        let mut chest_type = cnd.chest_type as i32;
        if chest_type == CHEST_PORTAL && world.custom_rules().custom_rules_enabled != 0 {
            chest_type = CHEST_STANDARD;
        }
        let extra = subdata_from(net_data, 0x40);
        let dict = gunzip_plist(extra);
        let n = slots_for(chest_type);
        let mut owner_id = dict_str(dict.as_ref(), "ownerID");
        let owner_name = dict_str(dict.as_ref(), "ownerName");
        let mut inv: Option<Vec<SubSlot>> = None;
        if chest_type != CHEST_PORTAL {
            if owner_id.is_none() {
                owner_id = dict_str(dict.as_ref(), "safeClientID");
            }
            if let Some(saved) = dict.as_ref().and_then(|d| d.object_for_key("saveItemSlots")) {
                let mut v = Vec::with_capacity(n);
                for i in 0..n {
                    v.push(Rc::new(RefCell::new(load_slot_items(saved_slot_at(saved, i)))));
                }
                inv = Some(v);
            }
        }
        {
            let mut st = me.st.borrow_mut();
            st.chest_type = chest_type;
            for i in 0..4 {
                st.shelf_render_items[i] = cnd.render_items[i] as i32;
                st.shelf_item_data_bs[i] = cnd.render_item_data_bs[i];
            }
            st.inventory_items = inv;
        }
        me_obj.set_owner_id(owner_id);
        me.io.borrow_mut().owner_name = owner_name;
        for b in dw.blockheads() {
            if let Some(bh) = handle::downcast::<crate::blockhead::blockhead::Blockhead>(&b) {
                bh.net_interaction_object_was_loaded(&me_obj);
            }
        }
        me.init_sub_derived_items();
        Some(me)
    }

    // ---- accessors -------------------------------------------------------------------------------

    /// `-chestType`.
    pub fn chest_type(&self) -> i32 {
        self.st.borrow().chest_type
    }

    /// `-inventoryChanged` (never written).
    pub fn inventory_changed(&self) -> bool {
        self.st.borrow().inventory_changed
    }

    /// `isInUse` convenience for npc_base.
    pub fn is_in_use(&self) -> bool {
        self.io.borrow().is_in_use
    }

    /// `-inventoryItems`: PORTAL -> `[[world portalChestManager] portalChestInventoryItems]` (always nil,
    /// resolution C1), else lazy-load then the handles.
    pub fn inventory_items(&self) -> Option<Vec<SubSlot>> {
        if self.chest_type() == CHEST_PORTAL {
            return None;
        }
        self.load_inventory_items_from_disk_if_needed();
        self.st.borrow().inventory_items.clone()
    }

    /// `-loadInventoryItemsFromDiskIfNeeded`.
    pub fn load_inventory_items_from_disk_if_needed(&self) {
        let need = {
            let st = self.st.borrow();
            st.inventory_items.is_none() && st.chest_type != CHEST_PORTAL
        };
        if need && !self.base.borrow().is_net {
            let data = self.dw().load_local_inventory_data_for_chest(&self.me());
            self.update_inventory_with_data(data.as_deref());
        }
    }

    fn slot_blobs(&self, filter_time_crystal: bool) -> Vec<Vec<Vec<u8>>> {
        let st = self.st.borrow();
        let mut out = Vec::new();
        if let Some(inv) = &st.inventory_items {
            for slot in inv {
                let mut s = Vec::new();
                for it in slot.borrow().iter() {
                    let it = it.borrow();
                    if filter_time_crystal && it.item_type.0 == ITEM_TIME_CRYSTAL {
                        continue;
                    }
                    s.push(it.save_data());
                }
                out.push(s);
            }
        }
        out
    }

    /// `-inventoryDataAllowingEmpty:`: `gzip(xml plist of saveItemSlots)`, NO 0xb filter.
    pub fn inventory_data_allowing_empty(&self, allow_empty: bool) -> Option<Vec<u8>> {
        self.st.borrow().inventory_items.as_ref()?;
        let blobs = self.slot_blobs(false);
        let has_item = blobs.iter().any(|s| !s.is_empty());
        if !has_item && !allow_empty {
            return None;
        }
        let xml = data_from_property_list(&slots_plist_from_blobs(blobs))?;
        Some(gzip_deflate(&xml))
    }

    /// `-updateInventoryWithData:`.
    pub fn update_inventory_with_data(&self, data: Option<&[u8]>) {
        let ct = self.chest_type();
        if ct == CHEST_PORTAL {
            return;
        }
        let n = slots_for(ct);
        let mut v: Vec<SubSlot> = Vec::with_capacity(n);
        let mut ok = false;
        if let Some(d) = data {
            if let Some(saved) = gunzip_plist(d) {
                for i in 0..n {
                    v.push(Rc::new(RefCell::new(load_slot_items(saved_slot_at(&saved, i)))));
                }
                ok = true;
                // `currentBlockhead == [dynamicWorld activeBlockhead]` -> addChestItemsToFoundList: (client UI path,
                // activeBlockhead is nil on the dedicated server)
            }
        }
        if !ok {
            v.clear();
            for _ in 0..n {
                v.push(new_slot());
            }
        }
        self.st.borrow_mut().inventory_items = Some(v);
    }

    /// `-remoteInventoryDataRecieved:` (sic).
    pub fn remote_inventory_data_recieved(&self, data: Option<&[u8]>) {
        self.update_inventory_with_data(data);
    }

    /// `-getSaveDictIncludingInventory:`.
    pub fn get_save_dict_including_inventory(&self, me: &ObjRef, include: bool) -> PlistDict {
        let mut d = interaction_object_get_save_dict(me);
        let ct = self.chest_type();
        d.map.insert("chestType".to_string(), PlistValue::int(ct as i64));
        if !include {
            if ct == CHEST_SHELF || ct == CHEST_DISPLAY_CABINET {
                let st = self.st.borrow();
                match &st.inventory_items {
                    None => {
                        for i in 0..4 {
                            d.map.insert(
                                format!("shelfRenderItems_{}", i),
                                PlistValue::int(st.shelf_render_items[i] as i64),
                            );
                            d.map.insert(
                                format!("shelfItemDataBs_{}", i),
                                PlistValue::int(st.shelf_item_data_bs[i] as i64),
                            );
                        }
                    }
                    Some(inv) => {
                        for i in 0..4usize {
                            // objectAtIndex: on a short array raises
                            if i >= inv.len() {
                                crate::foundation::exception::range_exception_index(
                                    i as u64,
                                    inv.len() as u64,
                                    "objectAtIndex:",
                                );
                            }
                            let s = inv[i].borrow();
                            if let Some(it) = s.last() {
                                let it = it.borrow();
                                d.map.insert(format!("shelfRenderItems_{}", i), PlistValue::int(it.item_type.0 as i64));
                                let b = if item_type_is_workbench(it.item_type) {
                                    it.dynamic_object_save_dict
                                        .as_ref()
                                        .and_then(|x| x.object_for_key("level"))
                                        .map_or(0, |v| v.int_value()) as i64
                                } else {
                                    it.data_b as i64
                                };
                                d.map.insert(format!("shelfItemDataBs_{}", i), PlistValue::int(b));
                            }
                        }
                    }
                }
            }
        } else {
            self.load_inventory_items_from_disk_if_needed();
            let arr = slots_plist_from_blobs(self.slot_blobs(true));
            d.map.insert("saveItemSlots".to_string(), arr);
        }
        d
    }

    /// `-contentsDidChange`.
    pub fn contents_did_change(&self, me: &ObjRef) {
        interaction_object_notify_save(me);
        me.set_update_needs_to_be_sent(true);
        self.st.borrow_mut().inventory_needs_to_be_sent_with_update = true;
    }

    /// `-moveInventoryItemsFromArray:toIndex:count:`.
    pub fn move_inventory_items_from_array_to_index_count(
        &self,
        me: &ObjRef,
        from: &SubSlot,
        to_index: i32,
        count: i32,
    ) -> i32 {
        let mut set = BTreeSet::new();
        self.move_inventory_items_from_array_to_index_count_assigned_indexes(me, from, to_index, count, &mut set)
    }

    /// `-moveInventoryItemsFromArray:toIndex:count:assignedIndexes:`.
    pub fn move_inventory_items_from_array_to_index_count_assigned_indexes(
        &self,
        me: &ObjRef,
        from: &SubSlot,
        to_index: i32,
        count: i32,
        assigned: &mut BTreeSet<i32>,
    ) -> i32 {
        let mut to_index = to_index;
        let mut count = count;
        let assign_index = to_index == -1;
        let ct = self.chest_type();
        if ct == CHEST_PORTAL {
            nslog!("This function should not be called on portal chests.");
            return 0;
        }
        if count == 0 || from.borrow().is_empty() {
            return 0;
        }
        let (from_type, from_data_b) = {
            let f = from.borrow();
            let f0 = f[0].borrow();
            (f0.item_type, f0.data_b)
        };
        if assign_index {
            if item_type_is_stackable(from_type, 0, 0) {
                let st = self.st.borrow();
                if let Some(inv) = &st.inventory_items {
                    for (i, slot) in inv.iter().enumerate() {
                        let s = slot.borrow();
                        if !s.is_empty() && s.len() < MAX_STACK as usize {
                            let s0 = s[0].borrow();
                            if s0.item_type == from_type && item_type_is_stackable(s0.item_type, s0.data_b, from_data_b)
                            {
                                to_index = i as i32;
                                break;
                            }
                        }
                    }
                }
            }
            if to_index == -1 {
                let st = self.st.borrow();
                if let Some(inv) = &st.inventory_items {
                    for (i, slot) in inv.iter().enumerate() {
                        if slot.borrow().is_empty() {
                            to_index = i as i32;
                            break;
                        }
                    }
                }
            }
        }
        if to_index == -1 {
            return 0;
        }
        // `inventoryItems[toIndex]`; a nil array makes the destination nil (C5, UB-PARITY)
        let to_slot: Option<SubSlot> = {
            let st = self.st.borrow();
            match &st.inventory_items {
                None => None,
                Some(inv) => {
                    if (to_index as i64 as u64) >= inv.len() as u64 {
                        crate::foundation::exception::range_exception_index(
                            to_index as i64 as u64,
                            inv.len() as u64,
                            "objectAtIndex:",
                        );
                    }
                    Some(inv[to_index as usize].clone())
                }
            }
        };
        let cur: i32 = to_slot.as_ref().map_or(0, |s| s.borrow().len() as i32);
        let can = if cur < 1 {
            true
        } else {
            let ts = to_slot.as_ref().unwrap().borrow();
            let t0 = ts[0].borrow();
            from_type == t0.item_type && item_type_is_stackable(from_type, from_data_b, t0.data_b)
        };
        if !can {
            return 0;
        }
        let from_count = from.borrow().len();
        if (from_count as u64) < (count as i64 as u64) {
            count = from_count as i32;
        }
        let amount = count.min(MAX_STACK - cur);
        if amount > 0 {
            for _ in 0..amount {
                let it = from.borrow_mut().remove(from_count - amount as usize);
                if let Some(ts) = &to_slot {
                    ts.borrow_mut().push(it);
                }
            }
        }
        let mut total = amount;
        if assign_index && amount > 0 && amount < count {
            total = self
                .move_inventory_items_from_array_to_index_count_assigned_indexes(me, from, -1, count - amount, assigned)
                .wrapping_add(amount);
        }
        self.contents_did_change(me);
        if ct == CHEST_SHELF {
            self.world().report_achievement_with_identifier("grp.shelf");
        }
        if total > 0 {
            assigned.insert(to_index);
        }
        total
    }

    /// `-removeItemIfAvailable:`.
    pub fn remove_item_if_available(&self, me: &ObjRef, item_type: i32) -> bool {
        self.load_inventory_items_from_disk_if_needed();
        let inv = self.st.borrow().inventory_items.clone();
        if let Some(inv) = inv {
            for slot in &inv {
                let hit = {
                    let s = slot.borrow();
                    !s.is_empty() && s[0].borrow().item_type.0 as i32 == item_type
                };
                if hit {
                    slot.borrow_mut().pop();
                    self.contents_did_change(me);
                    return true;
                }
            }
        }
        false
    }

    fn is_shelf_like(&self) -> bool {
        let ct = self.chest_type();
        ct == CHEST_SHELF || ct == CHEST_DISPLAY_CABINET
    }

    /// `-title`.
    pub fn title_string(&self) -> String {
        match self.chest_type() {
            1 => "SAFE",
            2 => "SHELF",
            5 => "CABINET",
            3 => "GOLDEN CHEST",
            6 => "FEEDER CHEST",
            4 => "PORTAL CHEST",
            _ => "CHEST",
        }
        .to_string()
    }
}

impl DynamicObject for Chest {
    fn base(&self) -> &RefCell<DynamicObjectBase> {
        &self.base
    }
    fn class_info(&self) -> &'static ClassInfo {
        &CHEST_CLASS
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
    /// `shouldAddToMacroBlock` is inherited from InteractionObject (`proxyObjectOwner == nil`).
    fn should_add_to_macro_block(&self) -> bool {
        interaction_object_should_add_to_macro_block(self)
    }
    /// `-objectType`.
    fn object_type(&self) -> i32 {
        DYNAMIC_OBJECT_TYPE_CHEST
    }
    /// `-occupiesNormalContents`.
    fn occupies_normal_contents(&self) -> bool {
        true
    }
    /// `-freeblockCreationItemType`.
    fn freeblock_creation_item_type(&self) -> i32 {
        item_type_from_chest_type(ChestType(self.chest_type() as u32)).0 as i32
    }
    /// `-freeBlockCreationSaveDict` (= `getSaveDict`, keeps the inventory).
    fn free_block_creation_save_dict(&self, me: &ObjRef) -> Option<PlistDict> {
        Some(self.get_save_dict(me))
    }
    /// `-getSaveDict`.
    fn get_save_dict(&self, me: &ObjRef) -> PlistDict {
        let inc = self.chest_type() != CHEST_PORTAL;
        self.get_save_dict_including_inventory(me, inc)
    }
    /// `-creationNetDataForClient:` is not overridden by Chest: the InteractionObject version delegates to the
    /// virtual `updateNetDataForClient:` (module decision 2.4).
    fn creation_net_data_for_client(&self, me: &ObjRef, client: Option<&str>) -> Option<Vec<u8>> {
        self.update_net_data_for_client(me, client)
    }
    /// `-updateNetDataForClient:`. Unwritten struct bytes are zero (resolution C3).
    fn update_net_data_for_client(&self, me: &ObjRef, _client: Option<&str>) -> Option<Vec<u8>> {
        let mut cnd = ChestNetData::default();
        cnd.interaction_object_creation_net_data = interaction_object_interaction_object_creation_net_data(me);
        let ct = self.chest_type();
        cnd.chest_type = ct as u8;
        let owner_id = me.owner_id();
        let owner_name = self.io.borrow().owner_name.clone();
        let proxy = self.io.borrow().proxy_object_owner.is_some();
        let mut dd = new_mutable_dict();
        {
            let mut st = self.st.borrow_mut();
            if ct == CHEST_SHELF || ct == CHEST_DISPLAY_CABINET {
                match &st.inventory_items {
                    None => {
                        for i in 0..4 {
                            cnd.render_items[i] = st.shelf_render_items[i] as u16;
                            cnd.render_item_data_bs[i] = st.shelf_item_data_bs[i];
                        }
                    }
                    Some(inv) => {
                        for i in 0..4usize {
                            if i >= inv.len() {
                                crate::foundation::exception::range_exception_index(
                                    i as u64,
                                    inv.len() as u64,
                                    "objectAtIndex:",
                                );
                            }
                            let s = inv[i].borrow();
                            if s.is_empty() {
                                cnd.render_items[i] = 0;
                            } else {
                                let it = s.last().unwrap().borrow();
                                cnd.render_items[i] = it.item_type.0 as u16;
                                cnd.render_item_data_bs[i] = it.data_b;
                                if item_type_is_workbench(it.item_type) {
                                    cnd.render_item_data_bs[i] = it
                                        .dynamic_object_save_dict
                                        .as_ref()
                                        .and_then(|x| x.object_for_key("level"))
                                        .map_or(0, |v| v.int_value())
                                        as u16;
                                }
                            }
                        }
                    }
                }
            }
            if let Some(o) = owner_id {
                dd.map.insert("ownerID".to_string(), PlistValue::string(o));
            }
            if let Some(n) = owner_name {
                dd.map.insert("ownerName".to_string(), PlistValue::string(n));
            }
            let embed = ct != CHEST_PORTAL
                && st.inventory_items.is_some()
                && (st.inventory_needs_to_be_sent_with_update || proxy);
            if embed {
                let mut arr = Vec::new();
                for slot in st.inventory_items.as_ref().unwrap() {
                    let mut sl = Vec::new();
                    for it in slot.borrow().iter() {
                        let it = it.borrow();
                        if it.item_type.0 != ITEM_TIME_CRYSTAL {
                            sl.push(PlistValue::data(it.save_data()));
                        }
                    }
                    arr.push(mutable_array_of(sl));
                }
                dd.map.insert("saveItemSlots".to_string(), mutable_array_of(arr));
                st.inventory_needs_to_be_sent_with_update = false;
            }
        }
        let mut out = cnd.to_vec();
        out.extend_from_slice(&gz_plist_bytes(&PlistValue::Dict(dd)));
        Some(out)
    }
    /// `-remoteUpdate:`.
    fn remote_update(&self, me: &ObjRef, net_data: &[u8]) {
        let old_remote = {
            let io = self.io.borrow();
            if io.is_in_use {
                io.remote_blockhead_in_use_unique_id
            } else {
                0
            }
        };
        interaction_object_remote_update(me, net_data);
        let cnd = ChestNetData::read_le(&padded_bytes(net_data, 0x40));
        if self.is_shelf_like() {
            let mut st = self.st.borrow_mut();
            for i in 0..4 {
                st.shelf_render_items[i] = cnd.render_items[i] as i32;
                st.shelf_item_data_bs[i] = cnd.render_item_data_bs[i];
            }
        }
        if self.io.borrow().current_blockhead.is_none() {
            let dict = gunzip_plist(subdata_from(net_data, 0x40));
            me.set_owner_id(dict_str(dict.as_ref(), "ownerID"));
            self.io.borrow_mut().owner_name = dict_str(dict.as_ref(), "ownerName");
            let ct = self.chest_type();
            if ct != CHEST_PORTAL {
                if let Some(saved) = dict.as_ref().and_then(|d| d.object_for_key("saveItemSlots")) {
                    let n = slots_for(ct);
                    if let Some(inv) = self.st.borrow_mut().inventory_items.as_mut() {
                        inv.clear();
                    }
                    for i in 0..n {
                        let slot = Rc::new(RefCell::new(load_slot_items(saved_slot_at(saved, i))));
                        // `addObject:` on a nil array drops the slot (the array is not allocated here)
                        if let Some(inv) = self.st.borrow_mut().inventory_items.as_mut() {
                            inv.push(slot);
                        }
                    }
                }
            }
        }
        let (remote_now, is_net) = (self.io.borrow().remote_blockhead_in_use_unique_id, self.base.borrow().is_net);
        if !is_net && old_remote != remote_now && remote_now != 0 && self.chest_type() != CHEST_PORTAL {
            self.load_inventory_items_from_disk_if_needed();
            self.dw().send_chest_inventory_for_chest_to_client_owning_blockhead_with_id(me, remote_now);
        }
    }
    /// `-update:accurateDT:isSimulation:` (client memory drop is live for net-created chests, C2).
    fn update_accurate_dt_is_simulation(&self, me: &ObjRef, dt: f32, accurate_dt: f32, is_simulation: bool) {
        if !self.base.borrow().needs_removed {
            interaction_object_update_accurate_dt_is_simulation(me, dt, accurate_dt, is_simulation);
            let drop_it = {
                let io = self.io.borrow();
                let st = self.st.borrow();
                io.current_blockhead.is_none()
                    && self.base.borrow().is_net
                    && !st.inventory_needs_to_be_sent_with_update
                    && io.proxy_object_owner.is_none()
                    && st.chest_type != CHEST_PORTAL
            };
            if drop_it {
                self.st.borrow_mut().inventory_items = None;
            }
        }
    }
    /// `-worldChanged:` (support check).
    fn world_changed(&self, me: &ObjRef, positions: &mut Vec<IntPair>) {
        if self.base.borrow().is_net {
            return;
        }
        let world = self.world();
        let pos = self.base.borrow().pos;
        let ct = self.chest_type();
        if ct != CHEST_SHELF && ct != CHEST_DISPLAY_CABINET {
            // STANDARD, SAFE, GOLD, PORTAL, FEEDER: the shared support test (cylinder wrap, E4 for left / right / up)
            if support_lost(&world, pos, positions) {
                self.remove(me, None);
            }
            return;
        }
        for p in positions.clone() {
            // wall-mounted: only the column matters, `y` of the changed tile is NOT compared; no `return` after remove
            if p.x == pos.x {
                match tile_at_world_position_loaded(pos.x, pos.y, &world) {
                    Some(t) => {
                        if t.back_wall_type_index() == 2 {
                            self.remove(me, None);
                        }
                    }
                    None => {
                        // UB-PARITY: unguarded NULL dereference in the original
                        ub_null_tile();
                    }
                }
            }
        }
    }
    /// `-worldContentsChanged:`.
    fn world_contents_changed(&self, me: &ObjRef, positions: &mut Vec<IntPair>) {
        self.world_changed(me, positions)
    }
    /// `-setNeedsRemoved:`.
    fn set_needs_removed(&self, _me: &ObjRef, v: bool) {
        self.base.borrow_mut().needs_removed = v;
        if v && self.io.borrow().is_in_use {
            let cur = self.io.borrow().current_blockhead.clone();
            if let Some(bh) = cur.as_ref().and_then(|c| c.as_blockhead()) {
                bh.stop_interacting();
            }
        }
    }
    /// `-updatePosition:`.
    fn update_position(&self, me: &ObjRef, new_position: IntPair) {
        let (in_use, cur) = {
            let io = self.io.borrow();
            (io.is_in_use, io.current_blockhead.clone())
        };
        if in_use && new_position != self.base.borrow().pos {
            if let Some(bh) = cur.as_ref().and_then(|c| c.as_blockhead()) {
                bh.stop_interacting();
            }
        }
        base_update_position(me, new_position);
    }
    /// `-removeFromMacroBlock` (deletes the stored inventory record).
    fn remove_from_macro_block(&self, me: &ObjRef) {
        self.dw().remove_saved_inventory_for_chest(me);
        base_remove_from_macro_block(me);
    }
    /// `-dealloc`: the inventory is plain data.
    fn dealloc(&self, me: &ObjRef) {
        self.st.borrow_mut().inventory_items = None;
        interaction_object_dealloc(me);
    }
    /// `-blockheadsLoaded` (inherited).
    fn blockheads_loaded(&self, me: &ObjRef) {
        interaction_object_blockheads_loaded(me)
    }
    /// `-blockheadUnloaded:` (inherited).
    fn blockhead_unloaded(&self, me: &ObjRef, b: &ObjRef) {
        interaction_object_blockhead_unloaded(me, b)
    }
}

impl InteractionObjectLike for Chest {
    fn io_state(&self) -> &RefCell<InteractionObjectState> {
        &self.io
    }
    /// `-interactionObjectType`.
    fn interaction_object_type(&self) -> u16 {
        INTERACTION_OBJECT_TYPE_CHEST
    }
    /// `-destroyItemType`.
    fn destroy_item_type(&self) -> i32 {
        item_type_from_chest_type(ChestType(self.chest_type() as u32)).0 as i32
    }
    /// `-title`.
    fn title(&self) -> String {
        self.title_string()
    }
    /// `-actionTitle`.
    fn action_title(&self) -> String {
        format!("USE {}", self.title_string())
    }
    /// `-requiresHumanInteraction`.
    fn requires_human_interaction(&self) -> bool {
        true
    }
    /// `-isPaintable`.
    fn is_paintable(&self) -> bool {
        true
    }
    /// `-canBeUsedByBlockhead:`.
    fn can_be_used_by_blockhead(&self, me: &ObjRef, b: &ObjRef) -> bool {
        let controlled = b.as_blockhead().map_or(false, |x| x.is_client_blockhead_being_controlled_by_server());
        if self.world().is_admin() && !controlled {
            return true;
        }
        let mut ok = interaction_object_can_be_used_by_blockhead(me, b);
        let ct = self.chest_type();
        if ok && (ct == CHEST_SAFE || ct == CHEST_DISPLAY_CABINET || ct == CHEST_PORTAL) {
            if let Some(owner) = me.owner_id() {
                let local = b.as_blockhead().and_then(|x| x.local_net_id());
                // `[[blockhead localNetID] isEqualToString:ownerID]`: nil receiver -> NO
                ok = local.as_deref() == Some(owner.as_str());
            }
        }
        ok
    }
    /// `-startInteractionWithBlockhead:` (the found-list part is the client UI path).
    fn start_interaction_with_blockhead(&self, me: &ObjRef, b: &ObjRef) {
        interaction_object_start_interaction_with_blockhead(me, b);
    }
    /// `-remove:`.
    fn remove(&self, me: &ObjRef, remove_blockhead: Option<&ObjRef>) {
        let (needs_removed, is_net) = {
            let b = self.base.borrow();
            (b.needs_removed, b.is_net)
        };
        if !needs_removed && !is_net {
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
            self.world().remove_tile_at_world_x_world_y_create_contents_freeblock_count_create_foreground_contents_freeblock_count_remove_blockhead(
                pos.x,
                pos.y,
                1,
                0,
                remove_blockhead,
            );
            self.set_needs_removed(me, true);
        }
    }
    /// `-paint:`.
    fn paint(&self, me: &ObjRef, color_index: u16) {
        self.io.borrow_mut().paint_color = color_index;
        if !self.base.borrow().is_net {
            me.set_update_needs_to_be_sent(true);
            let pos = self.base.borrow().pos;
            self.dw().dynamic_world_changed_at_pos_object_type(pos, DYNAMIC_OBJECT_TYPE_CHEST);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dynworld::state::testutil::dw as test_dw;
    use crate::types::item_type::ItemType;

    fn setup(name: &str) -> (Rc<World>, Rc<DynamicWorld>) {
        let d = test_dw(name, 4);
        (d.world(), d)
    }

    fn mk_item(t: u32) -> Rc<RefCell<InventoryItem>> {
        InventoryItem::new(ItemType(t), 0, 0, None, None)
    }

    fn place(world: &Rc<World>, d: &Rc<DynamicWorld>, item_type: u32) -> Rc<Chest> {
        let it = mk_item(item_type);
        let it = it.borrow();
        Chest::new(world, d, IntPair { x: 40, y: 3 }, None, Some(&it), false, None, Some("cid"), Some("Bob")).unwrap()
    }

    ///  observed vector: `@[@[<010203>], @[]]` is 251 bytes of GNUstep XML.
    #[test]
    fn observed_inventory_xml_vector() {
        let v = slots_plist_from_blobs(vec![vec![vec![1, 2, 3]], vec![]]);
        let xml = data_from_property_list(&v).unwrap();
        let want = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE plist PUBLIC \"-//GNUstep//DTD plist 0.9//EN\" \"http://www.gnustep.org/plist-0_9.xml\">\n<plist version=\"0.9\">\n<array>\n    <array>\n\t<data>\nAQID</data>\n    </array>\n    <array>\n    </array>\n</array>\n</plist>";
        assert_eq!(String::from_utf8(xml.clone()).unwrap(), want);
        assert_eq!(xml.len(), 251);
    }

    #[test]
    fn placement_types_and_slot_counts() {
        let (w, d) = setup("chest_place");
        for (item, ct, n) in
            [(0x413, 0, 16), (0x429, 1, 16), (0xa1, 2, 4), (0x430, 3, 16), (0x450, 6, 16), (0xcf, 5, 4)]
        {
            let c = place(&w, &d, item);
            assert_eq!(c.chest_type(), ct);
            assert_eq!(c.st.borrow().inventory_items.as_ref().unwrap().len(), n);
            assert_eq!(c.interaction_object_type(), 2);
            assert_eq!(c.object_type(), 46);
            assert_eq!(c.destroy_item_type(), item as i32);
        }
        // portal chest with custom rules disabled: no inventory
        let c = place(&w, &d, 0x432);
        assert_eq!(c.chest_type(), CHEST_PORTAL);
        assert!(c.st.borrow().inventory_items.is_none());
        assert!(c.inventory_items().is_none());
        assert!(c.inventory_data_allowing_empty(true).is_none());
        assert_eq!(c.title(), "PORTAL CHEST");
        assert_eq!(c.action_title(), "USE PORTAL CHEST");
    }

    #[test]
    fn save_dict_keys_and_round_trip() {
        let (w, d) = setup("chest_save");
        let c = place(&w, &d, 0x413);
        let me: ObjRef = c.clone();
        let with = c.get_save_dict(&me);
        assert!(with.map.get(&"saveItemSlots".to_string()).is_some());
        assert_eq!(dict_int(&with, "chestType"), 0);
        assert_eq!(with.map.get(&"ownerID".to_string()).unwrap().as_str(), Some("cid"));
        let without = c.get_save_dict_including_inventory(&me, false);
        assert!(without.map.get(&"saveItemSlots".to_string()).is_none());
        c.st.borrow().inventory_items.as_ref().unwrap()[0].borrow_mut().push(mk_item(5));
        let dict = c.get_save_dict(&me);
        let c2 = Chest::from_save_dict(&w, &d, &dict, None).unwrap();
        assert_eq!(c2.chest_type(), 0);
        let inv = c2.st.borrow().inventory_items.clone().unwrap();
        assert_eq!(inv.len(), 16);
        assert_eq!(inv[0].borrow().len(), 1);
        assert_eq!(inv[0].borrow()[0].borrow().item_type.0, 5);
    }

    #[test]
    fn time_crystal_filtered_in_dict_but_not_in_blob() {
        let (w, d) = setup("chest_tc");
        let c = place(&w, &d, 0x413);
        let me: ObjRef = c.clone();
        c.st.borrow().inventory_items.as_ref().unwrap()[1].borrow_mut().push(mk_item(11));
        let dict = c.get_save_dict(&me);
        let slot_len = |v: &PlistValue| {
            let PlistValue::Array(a) = v else { unreachable!("slots") };
            let PlistValue::Array(s) = &a.items[1] else { unreachable!("slot") };
            s.items.len()
        };
        assert_eq!(slot_len(dict.map.get(&"saveItemSlots".to_string()).unwrap()), 0);
        let blob = c.inventory_data_allowing_empty(false).unwrap();
        assert_eq!(slot_len(&gunzip_plist(&blob).unwrap()), 1);
    }

    #[test]
    fn empty_inventory_blob_rule() {
        let (w, d) = setup("chest_empty");
        let c = place(&w, &d, 0x413);
        assert!(c.inventory_data_allowing_empty(false).is_none());
        assert!(c.inventory_data_allowing_empty(true).is_some());
    }

    #[test]
    fn update_net_data_layout_and_flag() {
        let (w, d) = setup("chest_net");
        let c = place(&w, &d, 0xa1);
        let me: ObjRef = c.clone();
        c.st.borrow().inventory_items.as_ref().unwrap()[2].borrow_mut().push(InventoryItem::new(
            ItemType(7),
            0,
            9,
            None,
            None,
        ));
        let b = c.update_net_data_for_client(&me, None).unwrap();
        assert_eq!(b[0x38], 2);
        assert_eq!(u16::from_le_bytes([b[0x28 + 4], b[0x28 + 5]]), 7);
        assert_eq!(u16::from_le_bytes([b[0x30 + 4], b[0x30 + 5]]), 9);
        assert!(b[0x39..0x40].iter().all(|x| *x == 0));
        let dict = gunzip_plist(&b[0x40..]).unwrap();
        assert_eq!(dict_str(Some(&dict), "ownerID").as_deref(), Some("cid"));
        assert_eq!(dict_str(Some(&dict), "ownerName").as_deref(), Some("Bob"));
        assert!(dict.object_for_key("saveItemSlots").is_none());
        c.contents_did_change(&me);
        let b2 = c.creation_net_data_for_client(&me, None).unwrap();
        assert!(gunzip_plist(&b2[0x40..]).unwrap().object_for_key("saveItemSlots").is_some());
        let b3 = c.update_net_data_for_client(&me, None).unwrap();
        assert!(gunzip_plist(&b3[0x40..]).unwrap().object_for_key("saveItemSlots").is_none());
    }

    #[test]
    fn move_items_stack_limit_and_auto_index() {
        let (w, d) = setup("chest_move");
        let c = place(&w, &d, 0x413);
        let me: ObjRef = c.clone();
        let from: SubSlot = Rc::new(RefCell::new((0..120).map(|_| mk_item(5)).collect()));
        let mut set = BTreeSet::new();
        let n = c.move_inventory_items_from_array_to_index_count_assigned_indexes(&me, &from, -1, 120, &mut set);
        assert_eq!(n, 120);
        let inv = c.st.borrow().inventory_items.clone().unwrap();
        assert_eq!(inv[0].borrow().len(), 99);
        assert_eq!(inv[1].borrow().len(), 21);
        assert!(from.borrow().is_empty());
        assert_eq!(set.iter().copied().collect::<Vec<_>>(), vec![0, 1]);
        assert!(c.st.borrow().inventory_needs_to_be_sent_with_update);
        let from2: SubSlot = Rc::new(RefCell::new(vec![mk_item(6), mk_item(6)]));
        assert_eq!(c.move_inventory_items_from_array_to_index_count(&me, &from2, 3, -1), 2);
        assert_eq!(inv[3].borrow().len(), 2);
        let from3: SubSlot = Rc::new(RefCell::new(vec![mk_item(7)]));
        assert_eq!(c.move_inventory_items_from_array_to_index_count(&me, &from3, 3, 1), 0);
    }

    #[test]
    fn remove_item_if_available_takes_last_of_first_match() {
        let (w, d) = setup("chest_rm");
        let c = place(&w, &d, 0x413);
        let me: ObjRef = c.clone();
        assert!(!c.remove_item_if_available(&me, 9));
        {
            let st = c.st.borrow();
            let inv = st.inventory_items.as_ref().unwrap();
            inv[2].borrow_mut().push(mk_item(9));
            inv[2].borrow_mut().push(mk_item(9));
        }
        assert!(c.remove_item_if_available(&me, 9));
        assert_eq!(c.st.borrow().inventory_items.as_ref().unwrap()[2].borrow().len(), 1);
    }

    #[test]
    fn net_ctor_short_record_raises() {
        // the 0x28-byte record is fine for the base header but `subdataWithRange:{0x40,..}` raises
        let r = crate::foundation::exception::catch_ns(|| subdata_from(&[0u8; 0x28], 0x40).len());
        let e = r.err().expect("raise");
        assert_eq!(e.name, "NSRangeException");
        assert_eq!(subdata_from(&[0u8; 0x40], 0x40).len(), 0);
    }

    #[test]
    fn shelf_keys_only_for_non_empty_slots() {
        let (w, d) = setup("chest_shelf");
        let c = place(&w, &d, 0xa1);
        let me: ObjRef = c.clone();
        c.st.borrow().inventory_items.as_ref().unwrap()[1].borrow_mut().push(InventoryItem::new(
            ItemType(8),
            0,
            3,
            None,
            None,
        ));
        let dict = c.get_save_dict_including_inventory(&me, false);
        assert!(dict.map.get(&"shelfRenderItems_1".to_string()).is_some());
        assert!(dict.map.get(&"shelfRenderItems_0".to_string()).is_none());
        assert_eq!(dict_int(&dict, "shelfItemDataBs_1"), 3);
        c.st.borrow_mut().inventory_items = None;
        let dict2 = c.get_save_dict_including_inventory(&me, false);
        assert_eq!(dict2.map.iter().filter(|(k, _)| k.starts_with("shelf")).count(), 8);
    }

    /// support truth table for a standard chest and for a shelf.
    #[test]
    fn world_changed_truth_table() {
        use crate::interaction::sign::testkit::*;
        let (w, d, pb) = world_with_block("chest_wc");
        let it = mk_item(0x413);
        let it = it.borrow();
        let c = Chest::new(&w, &d, IntPair { x: 10, y: 10 }, None, Some(&it), false, None, Some("cid"), None).unwrap();
        let me: ObjRef = c.clone();
        // own back wall is not 2 -> stays
        me.world_changed(&me, &mut vec![IntPair { x: 10, y: 9 }]);
        assert!(!me.needs_removed());
        tile(pb, 10, 10).set_back_wall_type_index(2);
        // changed tile beside the column (2 away) -> stays
        me.world_changed(&me, &mut vec![IntPair { x: 12, y: 9 }]);
        assert!(!me.needs_removed());
        // solid ground below -> stays
        tile(pb, 10, 9).set_type_index(6);
        me.world_changed(&me, &mut vec![IntPair { x: 10, y: 9 }]);
        assert!(!me.needs_removed());
        // support gone, everything around is air, back wall 2 -> removed
        tile(pb, 10, 9).set_type_index(2);
        me.world_changed(&me, &mut vec![IntPair { x: 10, y: 9 }]);
        assert!(me.needs_removed());

        // shelf: only the column matters, y is NOT compared
        let it = mk_item(0xa1);
        let it = it.borrow();
        let s = Chest::new(&w, &d, IntPair { x: 20, y: 10 }, None, Some(&it), false, None, Some("cid"), None).unwrap();
        let sm: ObjRef = s.clone();
        tile(pb, 20, 10).set_back_wall_type_index(9);
        sm.world_changed(&sm, &mut vec![IntPair { x: 20, y: 1 }]);
        assert!(!sm.needs_removed());
        tile(pb, 20, 10).set_back_wall_type_index(2);
        sm.world_changed(&sm, &mut vec![IntPair { x: 21, y: 10 }]);
        assert!(!sm.needs_removed());
        sm.world_changed(&sm, &mut vec![IntPair { x: 20, y: 1 }]);
        assert!(sm.needs_removed());
    }

    /// net-created chests are inert on `worldChanged:` and drop their inventory on update (C2).
    #[test]
    fn net_chest_rules() {
        let (w, d) = setup("chest_netrules");
        let c = place(&w, &d, 0x413);
        let me: ObjRef = c.clone();
        c.base.borrow_mut().is_net = true;
        tile_noop();
        me.world_changed(&me, &mut vec![IntPair { x: 40, y: 2 }]);
        assert!(!me.needs_removed());
        c.update_accurate_dt_is_simulation(&me, 0.1, 0.1, false);
        assert!(c.st.borrow().inventory_items.is_none());
    }

    fn tile_noop() {}

    use crate::interaction::sign::testkit::*;
    use crate::objects::base::new_mutable_dict;
    use crate::objects::interaction_object::testutil::TestBh;

    fn blob(t: u32, a: u16, b: u16) -> Vec<u8> {
        InventoryItem::new(ItemType(t), a, b, None, None).borrow().save_data()
    }

    /// `n` saved slots, the given ones filled.
    fn saved(n: usize, fill: &[(usize, Vec<Vec<u8>>)]) -> PlistValue {
        let mut slots = vec![Vec::new(); n];
        for (i, items) in fill {
            slots[*i] = items.clone();
        }
        slots_plist_from_blobs(slots)
    }

    fn obj(c: &Rc<Chest>) -> ObjRef {
        c.clone()
    }

    fn chest_at(
        w: &Rc<World>,
        d: &Rc<DynamicWorld>,
        x: i32,
        item: &Rc<RefCell<InventoryItem>>,
        sd: Option<&PlistDict>,
        client: Option<&str>,
    ) -> Rc<Chest> {
        let it = item.borrow();
        Chest::new(w, d, IntPair { x, y: 10 }, None, Some(&it), false, sd, client, None).unwrap()
    }

    #[test]
    fn helper_edge_cases() {
        assert!(!ub_null_tile());
        assert!(saved_slot_at(&PlistValue::int(1), 0).is_none());
        let arr = saved(2, &[]);
        assert!(saved_slot_at(&arr, 1).is_some());
        let e = crate::foundation::exception::catch_ns(|| saved_slot_at(&arr, 2).is_some()).unwrap_err();
        assert_eq!(e.name, "NSRangeException");
        assert!(gunzip_plist(&[1, 2, 3]).is_none());
        assert_eq!(array_count(None), 0);
        // saved slots keep data entries only
        let mixed = mutable_array_of(vec![PlistValue::int(3), PlistValue::data(blob(5, 0, 0))]);
        assert_eq!(load_slot_items(Some(&mixed)).len(), 1);
        assert!(load_slot_items(Some(&PlistValue::int(1))).is_empty());
        assert!(load_slot_items(None).is_empty());
        assert_eq!(array_count(Some(&arr)), 2);
        assert_eq!(dict_int(&new_mutable_dict(), "x"), 0);
        assert_eq!(dict_str(None, "x"), None);
    }

    #[test]
    fn creation_from_saved_slots_and_carried_inventories() {
        let (w, d, _pb) = world_with_block("chest_cr");
        // saved slots: the time crystal is dropped, the safe client id becomes the owner
        let mut sd = new_mutable_dict();
        sd.map.insert("saveItemSlots".into(), saved(16, &[(2, vec![blob(5, 1, 2), blob(11, 0, 0)])]));
        sd.map.insert("safeClientID".into(), PlistValue::string("sc"));
        let safe = chest_at(&w, &d, 4, &mk_item(0x429), Some(&sd), None);
        let me: ObjRef = safe.clone();
        assert_eq!(me.owner_id().as_deref(), Some("sc"));
        assert_eq!(safe.st.borrow().inventory_items.as_ref().unwrap()[2].borrow().len(), 1);
        // an explicit owner wins, and a plain chest ignores the safe client id
        let owned = chest_at(&w, &d, 5, &mk_item(0x429), Some(&sd), Some("me"));
        assert_eq!(obj(&owned).owner_id().as_deref(), Some("me"));
        let plain = chest_at(&w, &d, 6, &mk_item(0x413), Some(&sd), None);
        assert_eq!(obj(&plain).owner_id(), None);
        // a display cabinet takes four saved slots
        let mut sd4 = new_mutable_dict();
        sd4.map.insert("saveItemSlots".into(), saved(4, &[(1, vec![blob(7, 0, 3)])]));
        sd4.map.insert("safeClientID".into(), PlistValue::string("sc4"));
        let cab = chest_at(&w, &d, 7, &mk_item(0xcf), Some(&sd4), None);
        assert_eq!(obj(&cab).owner_id().as_deref(), Some("sc4"));
        // a safe carried as an item: its sub slots become the contents
        let mut input: Vec<Vec<ItemRef>> = vec![Vec::new(); 16];
        input[1].push(mk_item(8));
        let carried = InventoryItem::new(ItemType(0x429), 0, 0, Some(&input), None);
        let mut sdc = new_mutable_dict();
        sdc.map.insert("safeClientID".into(), PlistValue::string("sc2"));
        let c = chest_at(&w, &d, 8, &carried, Some(&sdc), None);
        assert_eq!(c.chest_type(), CHEST_SAFE);
        assert_eq!(obj(&c).owner_id().as_deref(), Some("sc2"));
        assert_eq!(c.st.borrow().inventory_items.as_ref().unwrap()[1].borrow().len(), 1);
        // the same without a safe client id, and for a plain chest item
        let c = chest_at(&w, &d, 9, &carried, None, None);
        assert_eq!(obj(&c).owner_id(), None);
        let carried_chest = InventoryItem::new(ItemType(0x413), 0, 0, Some(&input), None);
        let c = chest_at(&w, &d, 10, &carried_chest, Some(&sdc), None);
        assert_eq!(c.chest_type(), CHEST_STANDARD);
        assert_eq!(obj(&c).owner_id(), None);
        // an item without a matching slot list falls back to empty slots
        let c = chest_at(&w, &d, 11, &mk_item(5), None, None);
        assert_eq!(c.st.borrow().inventory_items.as_ref().unwrap().len(), 16);
        let shelf_item = mk_item(0xa1);
        let c = chest_at(&w, &d, 12, &shelf_item, None, None);
        assert!(c.st.borrow().inventory_items.as_ref().unwrap().iter().all(|s| s.borrow().is_empty()));
    }

    #[test]
    fn loading_from_a_save_dict() {
        let (w, d, _pb) = world_with_block("chest_load");
        // portal chests become standard ones when custom rules are on
        let mut sd = new_mutable_dict();
        sd.map.insert("chestType".into(), PlistValue::int(4));
        let portal = Chest::from_save_dict(&w, &d, &sd, None).unwrap();
        assert_eq!(portal.chest_type(), CHEST_PORTAL);
        assert!(portal.st.borrow().inventory_items.is_none());
        let mut rules = w.custom_rules();
        rules.custom_rules_enabled = 1;
        w.set_custom_rules(rules);
        assert_eq!(Chest::from_save_dict(&w, &d, &sd, None).unwrap().chest_type(), CHEST_STANDARD);
        rules.custom_rules_enabled = 0;
        w.set_custom_rules(rules);
        // shelf render caches, safe owner and a legacy inventory inside the dictionary
        let mut sd = new_mutable_dict();
        sd.map.insert("chestType".into(), PlistValue::int(2));
        sd.map.insert("safeClientID".into(), PlistValue::string("sc"));
        sd.map.insert("shelfRenderItems_1".into(), PlistValue::int(7));
        sd.map.insert("shelfItemDataBs_1".into(), PlistValue::int(9));
        let shelf = Chest::from_save_dict(&w, &d, &sd, None).unwrap();
        assert_eq!(shelf.st.borrow().shelf_render_items, [0, 7, 0, 0]);
        assert_eq!(shelf.st.borrow().shelf_item_data_bs, [0, 9, 0, 0]);
        assert_eq!(obj(&shelf).owner_id().as_deref(), Some("sc"));
        sd.map.insert("saveItemSlots".into(), saved(4, &[(3, vec![blob(5, 0, 0)])]));
        let legacy = Chest::from_save_dict(&w, &d, &sd, None).unwrap();
        assert_eq!(legacy.st.borrow().inventory_items.as_ref().unwrap()[3].borrow().len(), 1);
        sd.map.insert("saveItemSlots".into(), saved(1, &[]));
        let short = Chest::from_save_dict(&w, &d, &sd, None).unwrap();
        assert_eq!(short.st.borrow().inventory_items.as_ref().unwrap().len(), 4);
    }

    #[test]
    fn net_created_chests_and_remote_updates() {
        let (w, d, _pb) = world_with_block("chest_netc");
        let mut sd = new_mutable_dict();
        sd.map.insert("saveItemSlots".into(), saved(4, &[(0, vec![blob(7, 0, 3)])]));
        let shelf = chest_at(&w, &d, 4, &mk_item(0xa1), Some(&sd), Some("own"));
        let me: ObjRef = shelf.clone();
        shelf.contents_did_change(&me);
        let bytes = shelf.update_net_data_for_client(&me, None).unwrap();
        let rep = Chest::from_net_data(&w, &d, None, &bytes).unwrap();
        let rme: ObjRef = rep.clone();
        assert_eq!(rep.chest_type(), CHEST_SHELF);
        assert_eq!(rep.st.borrow().shelf_render_items[0], 7);
        assert_eq!(rme.owner_id().as_deref(), Some("own"));
        assert_eq!(rep.st.borrow().inventory_items.as_ref().unwrap()[0].borrow().len(), 1);
        // without the inventory the replica has none; a safe client id stands in for the owner
        let plain = chest_at(&w, &d, 5, &mk_item(0x429), None, None);
        let pme: ObjRef = plain.clone();
        let mut b = plain.update_net_data_for_client(&pme, None).unwrap();
        let mut dict = PlistValue::mutable_dict();
        if let PlistValue::Dict(dd) = &mut dict {
            dd.map.insert("safeClientID".into(), PlistValue::string("sf"));
        }
        b.truncate(0x40);
        b.extend_from_slice(&gz_plist_bytes(&dict));
        let rep2 = Chest::from_net_data(&w, &d, None, &b).unwrap();
        assert_eq!(obj(&rep2).owner_id().as_deref(), Some("sf"));
        assert!(rep2.st.borrow().inventory_items.is_none());
        // portal chests carry no inventory, and with custom rules they are standard ones
        let portal = chest_at(&w, &d, 6, &mk_item(0x432), None, None);
        let pob: ObjRef = portal.clone();
        let pbytes = portal.update_net_data_for_client(&pob, None).unwrap();
        assert_eq!(pbytes[0x38], CHEST_PORTAL as u8);
        let prep = Chest::from_net_data(&w, &d, None, &pbytes).unwrap();
        assert_eq!(prep.chest_type(), CHEST_PORTAL);
        assert!(prep.st.borrow().inventory_items.is_none());
        let mut rules = w.custom_rules();
        rules.custom_rules_enabled = 1;
        w.set_custom_rules(rules);
        assert_eq!(Chest::from_net_data(&w, &d, None, &pbytes).unwrap().chest_type(), CHEST_STANDARD);
        rules.custom_rules_enabled = 0;
        w.set_custom_rules(rules);
        // every blockhead of the world hears about a new replica
        let (bw, _bd, bdw) = crate::blockhead::blockhead::testutil::setup();
        d.blockheads.borrow_mut().push(crate::blockhead::blockhead::testutil::new_bh(&bw, &bdw, 0));
        assert!(Chest::from_net_data(&w, &d, None, &bytes).is_some());
        d.blockheads.borrow_mut().clear();
        // a replica of an unloaded position is refused
        let mut far = bytes.clone();
        far[8..12].copy_from_slice(&900u32.to_le_bytes());
        far[12..16].copy_from_slice(&900u32.to_le_bytes());
        assert!(Chest::from_net_data(&w, &d, None, &far).is_none());
        // remote updates: shelf caches, owner + inventory when nobody is using the chest
        let up = shelf.update_net_data_for_client(&me, None).unwrap();
        rep.remote_update(&rme, &up);
        assert_eq!(rep.st.borrow().shelf_render_items[0], 7);
        shelf.contents_did_change(&me);
        let up = shelf.creation_net_data_for_client(&me, None).unwrap();
        rep.remote_update(&rme, &up);
        assert_eq!(rep.st.borrow().inventory_items.as_ref().unwrap().len(), 4);
        // a remote user appearing makes the server push the inventory
        let mut nd = bytes.clone();
        nd[0x18..0x20].copy_from_slice(&5u64.to_le_bytes());
        nd[0x22] = 1;
        shelf.remote_update(&me, &nd);
        assert_eq!(shelf.io.borrow().remote_blockhead_in_use_unique_id, 5);
        // portal chests never send inventories, whoever uses them
        let mut pnd = pbytes.clone();
        pnd[0x18..0x20].copy_from_slice(&6u64.to_le_bytes());
        pnd[0x22] = 1;
        portal.remote_update(&pob, &pnd);
    }

    #[test]
    fn lazy_inventory_loading_and_blobs() {
        let (w, d, _pb) = world_with_block("chest_lazy");
        let c = chest_at(&w, &d, 4, &mk_item(0x413), None, Some("o"));
        let me = obj(&c);
        c.st.borrow().inventory_items.as_ref().unwrap()[0].borrow_mut().push(mk_item(5));
        let blob_data = c.inventory_data_allowing_empty(false).unwrap();
        // forgetting the slots and asking again loads them (here: nothing stored, so empty slots)
        c.st.borrow_mut().inventory_items = None;
        assert!(c.inventory_data_allowing_empty(true).is_none());
        let inv = c.inventory_items().unwrap();
        assert_eq!(inv.len(), 16);
        assert!(inv[0].borrow().is_empty());
        // update with data and with garbage
        c.remote_inventory_data_recieved(Some(&blob_data));
        assert_eq!(c.inventory_items().unwrap()[0].borrow().len(), 1);
        c.update_inventory_with_data(Some(&[1, 2, 3]));
        assert!(c.inventory_items().unwrap()[0].borrow().is_empty());
        c.update_inventory_with_data(None);
        assert_eq!(c.inventory_items().unwrap().len(), 16);
        // replicas and portal chests never load
        c.base.borrow_mut().is_net = true;
        c.st.borrow_mut().inventory_items = None;
        c.load_inventory_items_from_disk_if_needed();
        assert!(c.st.borrow().inventory_items.is_none());
        c.base.borrow_mut().is_net = false;
        c.st.borrow_mut().chest_type = CHEST_PORTAL;
        c.update_inventory_with_data(Some(&blob_data));
        assert!(c.st.borrow().inventory_items.is_none());
        assert!(!c.inventory_changed());
        assert!(!c.is_in_use());
        drop(me);
    }

    #[test]
    fn save_dict_for_shelves_with_items() {
        let (w, d, _pb) = world_with_block("chest_shelfsave");
        let shelf = chest_at(&w, &d, 4, &mk_item(0xa1), None, Some("o"));
        let me = obj(&shelf);
        let camp = {
            let mut dict = new_mutable_dict();
            dict.map.insert("level".into(), PlistValue::int(2));
            InventoryItem::new(ItemType::ITEM_CAMPFIRE, 0, 5, None, Some(PlistValue::Dict(dict)))
        };
        {
            let st = shelf.st.borrow();
            let inv = st.inventory_items.as_ref().unwrap();
            inv[0].borrow_mut().push(camp.clone());
            inv[1].borrow_mut().push(InventoryItem::new(ItemType(8), 0, 4, None, None));
        }
        let dict = shelf.get_save_dict_including_inventory(&me, false);
        assert_eq!(dict_int(&dict, "shelfRenderItems_0"), ItemType::ITEM_CAMPFIRE.0 as i32);
        assert_eq!(dict_int(&dict, "shelfItemDataBs_0"), 2);
        assert_eq!(dict_int(&dict, "shelfItemDataBs_1"), 4);
        // the wire form reads the same numbers
        let b = shelf.update_net_data_for_client(&me, None).unwrap();
        assert_eq!(u16::from_le_bytes([b[0x28], b[0x29]]) as u32, ItemType::ITEM_CAMPFIRE.0);
        assert_eq!(u16::from_le_bytes([b[0x30], b[0x31]]), 2);
        // a short slot array raises from both
        shelf.st.borrow_mut().inventory_items.as_mut().unwrap().truncate(2);
        let e =
            crate::foundation::exception::catch_ns(|| shelf.get_save_dict_including_inventory(&me, false).map.len())
                .unwrap_err();
        assert_eq!(e.name, "NSRangeException");
        let e = crate::foundation::exception::catch_ns(|| shelf.update_net_data_for_client(&me, None).unwrap().len())
            .unwrap_err();
        assert_eq!(e.name, "NSRangeException");
    }

    #[test]
    fn moving_items_into_chests() {
        let (w, d, _pb) = world_with_block("chest_mv");
        let c = chest_at(&w, &d, 4, &mk_item(0x413), None, Some("o"));
        let me = obj(&c);
        let from = |n: usize, t: u32| -> SubSlot { Rc::new(RefCell::new((0..n).map(|_| mk_item(t)).collect())) };
        // nothing to move
        assert_eq!(c.move_inventory_items_from_array_to_index_count(&me, &from(0, 5), 0, 3), 0);
        assert_eq!(c.move_inventory_items_from_array_to_index_count(&me, &from(2, 5), 0, 0), 0);
        // auto index prefers a stack with room, then an empty slot
        assert_eq!(c.move_inventory_items_from_array_to_index_count(&me, &from(3, 5), -1, 3), 3);
        assert_eq!(c.move_inventory_items_from_array_to_index_count(&me, &from(2, 5), -1, 2), 2);
        assert_eq!(c.st.borrow().inventory_items.as_ref().unwrap()[0].borrow().len(), 5);
        // a full stack is skipped
        c.st.borrow().inventory_items.as_ref().unwrap()[0].borrow_mut().extend((0..94).map(|_| mk_item(5)));
        assert_eq!(c.move_inventory_items_from_array_to_index_count(&me, &from(1, 5), -1, 1), 1);
        assert_eq!(c.st.borrow().inventory_items.as_ref().unwrap()[1].borrow().len(), 1);
        // removing takes the last item of the first matching slot
        assert!(c.remove_item_if_available(&me, 5));
        assert_eq!(c.st.borrow().inventory_items.as_ref().unwrap()[0].borrow().len(), 98);
        assert!(!c.remove_item_if_available(&me, 99));
        // non-stackable items fill empty slots one by one
        assert_eq!(c.move_inventory_items_from_array_to_index_count(&me, &from(1, 20), -1, 1), 1);
        // a chest with no free slot refuses
        for s in c.st.borrow().inventory_items.as_ref().unwrap().iter() {
            if s.borrow().is_empty() {
                s.borrow_mut().push(mk_item(30));
            }
        }
        assert_eq!(c.move_inventory_items_from_array_to_index_count(&me, &from(1, 31), -1, 1), 0);
        // a fixed slot only takes the same stackable kind
        assert_eq!(c.move_inventory_items_from_array_to_index_count(&me, &from(1, 31), 0, 1), 0);
        // portal chests log and refuse
        c.st.borrow_mut().chest_type = CHEST_PORTAL;
        assert_eq!(c.move_inventory_items_from_array_to_index_count(&me, &from(1, 5), 0, 1), 0);
        // no inventory loaded: the destination is nil and the items are lost
        c.st.borrow_mut().chest_type = CHEST_STANDARD;
        c.st.borrow_mut().inventory_items = None;
        let f = from(2, 5);
        assert_eq!(c.move_inventory_items_from_array_to_index_count(&me, &f, 3, 2), 2);
        assert!(f.borrow().is_empty());
        // an out-of-range slot raises
        c.update_inventory_with_data(None);
        let e = crate::foundation::exception::catch_ns(|| {
            c.move_inventory_items_from_array_to_index_count(&me, &from(1, 5), 40, 1)
        })
        .unwrap_err();
        assert_eq!(e.name, "NSRangeException");
        // shelves report their achievement
        let shelf = chest_at(&w, &d, 8, &mk_item(0xa1), None, Some("o"));
        assert_eq!(shelf.move_inventory_items_from_array_to_index_count(&obj(&shelf), &from(1, 5), 0, 1), 1);
    }

    #[test]
    fn titles_and_object_views() {
        let (w, d, _pb) = world_with_block("chest_views");
        for (item, title) in [
            (0x413, "CHEST"),
            (0x429, "SAFE"),
            (0xa1, "SHELF"),
            (0x430, "GOLDEN CHEST"),
            (0x432, "PORTAL CHEST"),
            (0xcf, "CABINET"),
            (0x450, "FEEDER CHEST"),
        ] {
            let c = chest_at(&w, &d, 4, &mk_item(item), None, Some("o"));
            let me = obj(&c);
            assert_eq!(c.title_string(), title);
            assert_eq!(c.action_title(), format!("USE {title}"));
            assert_eq!(me.class_info().name, "Chest");
            assert!(me.occupies_normal_contents());
            assert_eq!(me.freeblock_creation_item_type(), item as i32);
            assert!(me.free_block_creation_save_dict(&me).is_some());
            assert!(c.requires_human_interaction() && c.is_paintable());
            me.world_contents_changed(&me, &mut vec![]);
            me.set_needs_removed(&me, false);
            me.blockheads_loaded(&me);
        }
    }

    #[test]
    fn using_a_chest() {
        let (w, d, _pb) = world_with_block("chest_use");
        let me_bh = TestBh::new(7, false);
        let bh: ObjRef = me_bh.clone();
        let other: ObjRef = TestBh::new(8, false);
        let safe = chest_at(&w, &d, 4, &mk_item(0x429), None, Some("c7"));
        let sme = obj(&safe);
        // the server's own blockheads may use anything
        w.set_is_admin(true);
        assert!(safe.can_be_used_by_blockhead(&sme, &bh));
        w.set_is_admin(false);
        // an owner sees their safe, strangers do not, and a busy safe belongs to its user
        assert!(safe.can_be_used_by_blockhead(&sme, &bh));
        assert!(!safe.can_be_used_by_blockhead(&sme, &other));
        let plain = chest_at(&w, &d, 5, &mk_item(0x413), None, Some("c7"));
        assert!(plain.can_be_used_by_blockhead(&obj(&plain), &other));
        safe.start_interaction_with_blockhead(&sme, &bh);
        assert!(!safe.can_be_used_by_blockhead(&sme, &other));
        // removing while someone from another machine is using it only marks it
        let remote: ObjRef = TestBh::new(9, true);
        safe.io.borrow_mut().current_blockhead = Some(remote);
        safe.remove(&sme, None);
        assert!(!sme.needs_removed());
        assert!(safe.io.borrow().needs_to_be_removed_when_interaction_ends);
        safe.io.borrow_mut().current_blockhead = Some(bh.clone());
        safe.remove(&sme, None);
        assert!(sme.needs_removed());
        assert!(me_bh.log.borrow().iter().any(|l| l == "stop"));
        // a second removal and replicas do nothing
        safe.remove(&sme, None);
        // moving or deleting a used chest stops its user
        let c2 = chest_at(&w, &d, 6, &mk_item(0x413), None, Some("c7"));
        let m2 = obj(&c2);
        c2.start_interaction_with_blockhead(&m2, &bh);
        c2.update_position(&m2, IntPair { x: 6, y: 10 });
        c2.update_position(&m2, IntPair { x: 7, y: 10 });
        c2.set_needs_removed(&m2, true);
        // (removing the safe stopped its user twice: once in remove, once in set_needs_removed)
        assert_eq!(me_bh.log.borrow().iter().filter(|l| *l == "stop").count(), 4);
        c2.remove_from_macro_block(&m2);
        c2.dealloc(&m2);
        assert!(c2.st.borrow().inventory_items.is_none());
        // painting asks for a send on servers only
        let c3 = chest_at(&w, &d, 9, &mk_item(0x413), None, Some("c7"));
        let m3 = obj(&c3);
        c3.paint(&m3, 3);
        assert_eq!(c3.io.borrow().paint_color, 3);
        assert!(m3.update_needs_to_be_sent());
        m3.set_update_needs_to_be_sent(false);
        c3.base.borrow_mut().is_net = true;
        c3.paint(&m3, 4);
        assert!(!m3.update_needs_to_be_sent());
    }

    #[test]
    fn chests_ignore_world_changes_outside_their_column() {
        let (w, d, pb) = world_with_block("chest_wc2");
        let shelf = chest_at(&w, &d, 20, &mk_item(0xa1), None, Some("o"));
        let sm = obj(&shelf);
        tile(pb, 20, 10).set_back_wall_type_index(2);
        sm.world_changed(&sm, &mut vec![IntPair { x: 20, y: 5 }]);
        assert!(sm.needs_removed());
        // a shelf at the world edge: the tile of its own column is unloaded
        let edge = chest_at(&w, &d, 0, &mk_item(0xa1), None, Some("o"));
        let em = obj(&edge);
        em.world_changed(&em, &mut vec![IntPair { x: 0, y: 5 }]);
        assert!(!em.needs_removed());
    }

    #[test]
    fn move_and_remove_edge_cases() {
        let (w, d, _pb) = world_with_block("chest_edge");
        let c = chest_at(&w, &d, 4, &mk_item(0x413), None, Some("o"));
        let me = obj(&c);
        let from = |n: usize, t: u32| -> SubSlot { Rc::new(RefCell::new((0..n).map(|_| mk_item(t)).collect())) };
        // stacking looks past slots of other kinds, then falls back to an empty slot
        c.st.borrow().inventory_items.as_ref().unwrap()[0].borrow_mut().push(mk_item(6));
        assert_eq!(c.move_inventory_items_from_array_to_index_count(&me, &from(1, 5), -1, 1), 1);
        assert_eq!(c.st.borrow().inventory_items.as_ref().unwrap()[1].borrow().len(), 1);
        // a full destination takes nothing
        c.st.borrow().inventory_items.as_ref().unwrap()[2].borrow_mut().extend((0..99).map(|_| mk_item(7)));
        assert_eq!(c.move_inventory_items_from_array_to_index_count(&me, &from(2, 7), 2, 2), 0);
        // no inventory and no index to assign
        c.st.borrow_mut().inventory_items = None;
        assert_eq!(c.move_inventory_items_from_array_to_index_count(&me, &from(1, 5), -1, 1), 0);
        assert!(c.slot_blobs(false).is_empty());
        // portal chests have nothing to remove
        c.st.borrow_mut().chest_type = CHEST_PORTAL;
        assert!(!c.remove_item_if_available(&me, 5));
    }

    #[test]
    fn shelves_without_slots_and_busy_chests() {
        let (w, d, pb) = world_with_block("chest_busy");
        let shelf = chest_at(&w, &d, 4, &mk_item(0xa1), None, Some("o"));
        let me = obj(&shelf);
        shelf.st.borrow_mut().inventory_items = None;
        shelf.st.borrow_mut().shelf_render_items = [1, 2, 3, 4];
        shelf.st.borrow_mut().shelf_item_data_bs = [5, 6, 7, 8];
        let b = shelf.update_net_data_for_client(&me, None).unwrap();
        assert_eq!(u16::from_le_bytes([b[0x2a], b[0x2b]]), 2);
        assert_eq!(u16::from_le_bytes([b[0x32], b[0x33]]), 6);
        // a user on the owning machine: the remote user id is read back and the inventory pushed again
        let chest = chest_at(&w, &d, 8, &mk_item(0x413), None, Some("o"));
        let cme = obj(&chest);
        let mut nd = chest.update_net_data_for_client(&cme, None).unwrap();
        nd[0x18..0x20].copy_from_slice(&5u64.to_le_bytes());
        nd[0x22] = 1;
        chest.remote_update(&cme, &nd);
        chest.remote_update(&cme, &nd);
        assert_eq!(chest.io.borrow().remote_blockhead_in_use_unique_id, 5);
        // removed chests do not tick, replicas without users forget their inventory
        chest.base.borrow_mut().needs_removed = true;
        chest.update_accurate_dt_is_simulation(&cme, 0.1, 0.1, false);
        // a shelf whose own tile is unloaded still survives the column check
        *w.macro_tiles().get(0).unwrap().physical_block.borrow_mut() = None;
        me.world_changed(&me, &mut vec![IntPair { x: 4, y: 5 }]);
        assert!(!me.needs_removed());
        let _ = pb;
        // the plain object hooks
        let other: ObjRef = TestBh::new(3, false);
        chest.blockhead_unloaded(&cme, &other);
        assert!(chest.io_state().borrow().current_blockhead.is_none());
    }

    #[test]
    fn containers_and_ownerless_safes() {
        let (w, d, _pb) = world_with_block("chest_cont");
        let c = chest_at(&w, &d, 4, &mk_item(0x413), None, Some("o"));
        let me = obj(&c);
        // a basket is not stackable: it goes to an empty slot without looking for a stack
        let basket: SubSlot = Rc::new(RefCell::new(vec![InventoryItem::new(ItemType(0xc), 0, 0, None, None)]));
        assert_eq!(c.move_inventory_items_from_array_to_index_count(&me, &basket, -1, 1), 1);
        assert_eq!(c.st.borrow().inventory_items.as_ref().unwrap()[0].borrow().len(), 1);
        // a safe nobody owns can be used by anyone
        let safe = chest_at(&w, &d, 8, &mk_item(0x429), None, None);
        let sme = obj(&safe);
        assert_eq!(sme.owner_id(), None);
        let user: ObjRef = TestBh::new(7, false);
        assert!(safe.can_be_used_by_blockhead(&sme, &user));
        // a remote update while someone uses the chest leaves owner and slots alone
        let mut nd = c.update_net_data_for_client(&me, None).unwrap();
        c.io.borrow_mut().current_blockhead = Some(user.clone());
        nd[0x40..].fill(0);
        c.remote_update(&me, &nd);
        c.io.borrow_mut().current_blockhead = None;
        // an occupied chest is for its user only
        let other: ObjRef = TestBh::new(8, false);
        c.start_interaction_with_blockhead(&me, &user);
        assert!(!c.can_be_used_by_blockhead(&me, &other));
    }
}
