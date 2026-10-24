//! `TradingPost` (DynamicObjectType 48, interaction type 5, item `ITEM_TRADING_POST` 168): the "SHOP" object.
//!
//! The client-UI selectors (`buyButtonTappedWithQuantity:`, `moveInventoryItemsToSellSlotFromArray:count:`,
//! `setPriceTier:`,...) are unreachable on the server but are implemented for completeness.
//!
//! This file also hosts [`BlockheadInventoryApi`], the seam to the Blockhead cash / inventory selectors used only by
//! the unreachable buy / sell code of TradingPost and TradePortal (the blockhead_inventory module is built in
//! parallel;

use std::cell::RefCell;
use std::rc::Rc;

use crate::dynworld::state::DynamicWorld;
use crate::interaction::chest::{
    dict_str, gunzip_plist, gz_plist_bytes, mutable_array_of, subdata_from, ub_null_tile, Links,
};
use crate::items::inventory_item::{InventoryItem, ItemRef, SubSlot};
use crate::items::predicates::item_type_is_stackable;
use crate::objects::base::{padded_bytes, DynamicObject, DynamicObjectBase};
use crate::objects::class_info::{ClassInfo, INTERACTION_OBJECT_CLASS};
use crate::objects::handle::{self, CacheRef, ObjRef};
use crate::objects::interaction_object::*;
use crate::plist::value::{PlistDict, PlistValue};
use crate::types::item_type::ItemType;
use crate::types::netdata::TradingPostNetData;
use crate::types::structs::{IntPair, WireStruct};
use crate::world::helpers::{tile_is_half_depth, tile_is_solid};
use crate::world::state::World;
use crate::world::tiles::tile_at_world_position_loaded;

/// `TradingPost: InteractionObject` (instance size 0xd0).
pub static TRADING_POST_CLASS: ClassInfo =
    ClassInfo { name: "TradingPost", superclass: Some(&INTERACTION_OBJECT_CLASS), protocols: &[] };

/// `DYNAMIC_OBJECT_TYPE_TRADING_POST`
pub const DYNAMIC_OBJECT_TYPE_TRADING_POST: i32 = 48;
/// `INTERACTION_OBJECT_TYPE_TRADING_POST`
pub const INTERACTION_OBJECT_TYPE_TRADING_POST: u16 = 5;
/// `ITEM_TRADING_POST`
pub const ITEM_TRADING_POST: i32 = 168;
const ITEM_TIME_CRYSTAL: u32 = 11;
const CONTENTS_COPPER_WIRE: u8 = 0x60;

/// `priceTiers[]` (86 entries).
pub static PRICE_TIERS: [i32; 86] = [
    1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 15, 20, 25, 30, 35, 40, 45, 50, 60, 70, 80, 90, 100, 125, 150, 175, 200, 250, 300,
    350, 400, 450, 500, 600, 700, 800, 900, 1000, 2000, 3000, 4000, 5000, 6000, 7000, 8000, 9000, 10000, 15000, 20000,
    25000, 30000, 35000, 40000, 45000, 50000, 60000, 70000, 80000, 90000, 100000, 150000, 200000, 250000, 300000,
    350000, 400000, 450000, 500000, 600000, 700000, 800000, 900000, 1000000, 1500000, 2000000, 2500000, 3000000,
    3500000, 4000000, 4500000, 5000000, 6000000, 7000000, 8000000, 9000000, 10000000,
];

// =====================================================================================================
// Seam to the Blockhead cash / inventory selectors (unreachable server code only)
// =====================================================================================================

/// The Blockhead selectors used by the unreachable buy / sell / upgrade code. All defaults
/// are inert (`0`, `1.0`). Installed by the integration layer.
#[allow(unused_variables)]
pub trait BlockheadInventoryApi {
    /// `-totalCash`
    fn total_cash(&self, b: &ObjRef) -> i32 {
        0
    }
    /// `-subtractCash:`
    fn subtract_cash(&self, b: &ObjRef, amount: i32) {}
    /// `-countOfInventoryItemsOfType:includeActions:`
    fn count_of_inventory_items_of_type_include_actions(
        &self,
        b: &ObjRef,
        item_type: i32,
        include_actions: bool,
    ) -> i32 {
        0
    }
    /// `-countOfInventoryItemsWithSpecificDataBOfType:dataB:includeActions:`
    fn count_of_inventory_items_with_specific_data_b_of_type_data_b_include_actions(
        &self,
        b: &ObjRef,
        item_type: i32,
        data_b: u16,
        include_actions: bool,
    ) -> i32 {
        0
    }
    /// `-usageMultiplierForFirstItemOfType:`
    fn usage_multiplier_for_first_item_of_type(&self, b: &ObjRef, item_type: i32) -> f32 {
        1.0
    }
    /// `-subtractItemsFromInventoryOfType:count:` (`data_b == -1`) and `...count:dataB:`: the NUMBER of removed items
    /// (the original returns one array element per removed unit, resolution T6).
    fn subtract_items_from_inventory_of_type_count_data_b(
        &self,
        b: &ObjRef,
        item_type: i32,
        count: i32,
        data_b: i32,
    ) -> i32 {
        0
    }
}

struct NullBlockheadInventory;
impl BlockheadInventoryApi for NullBlockheadInventory {}

thread_local! {
    static BH_API: RefCell<Rc<dyn BlockheadInventoryApi>> = RefCell::new(Rc::new(NullBlockheadInventory));
}

/// Installs the Blockhead inventory seam (main thread, once).
pub fn set_blockhead_inventory_api(api: Rc<dyn BlockheadInventoryApi>) {
    BH_API.with(|c| *c.borrow_mut() = api);
}

/// The installed seam.
pub fn blockhead_inventory_api() -> Rc<dyn BlockheadInventoryApi> {
    BH_API.with(|c| c.borrow().clone())
}

/// A deep-enough copy of an `InventoryItem` for `FreeBlockInit` (sub-slots stay shared by identity).
pub fn clone_inventory_item(it: &InventoryItem) -> InventoryItem {
    InventoryItem {
        item_type: it.item_type,
        data_a: it.data_a,
        data_b: it.data_b,
        sub_items: it.sub_items.clone(),
        selected_sub_item_index: it.selected_sub_item_index,
        dynamic_object_save_dict: it.dynamic_object_save_dict.clone(),
    }
}

/// `[item subItems]` as the owned nested vectors `createFreeBlock...subItems:` takes.
fn sub_items_owned(it: &InventoryItem) -> Option<Vec<Vec<InventoryItem>>> {
    it.sub_items.as_ref().map(|slots| {
        slots.iter().map(|s| s.borrow().iter().map(|i| clone_inventory_item(&i.borrow())).collect()).collect()
    })
}

// =====================================================================================================
// TradingPost
// =====================================================================================================

/// TradingPost ivars 0xa0..0xca (client-only shader / bitmap string omitted).
pub struct TradingPostState {
    /// 0xa0; never contains item_type 11 after init / remote update
    pub sell_slot: Vec<ItemRef>,
    /// 0xa8
    pub coin_count: i32,
    /// 0xac (stored verbatim, no clamp)
    pub price_tier: i32,
    /// 0xb0
    pub seller_client_name: Option<String>,
    /// 0xc8 (only consumed by client rendering)
    pub needs_to_update_bitmap_string: bool,
    /// 0xc9
    pub inventory_needs_to_be_sent_with_update: bool,
}

/// `TradingPost: InteractionObject`.
pub struct TradingPost {
    base: RefCell<DynamicObjectBase>,
    io: RefCell<InteractionObjectState>,
    /// The TradingPost ivars.
    pub st: RefCell<TradingPostState>,
    links: Links,
}

impl TradingPost {
    fn alloc(world: &Rc<World>, dw: &Rc<DynamicWorld>) -> Rc<TradingPost> {
        let links = Links::new(world, dw);
        handle::alloc_obj(|w| TradingPost {
            base: RefCell::new(DynamicObjectBase::alloc_zeroed_with(w)),
            io: RefCell::new(InteractionObjectState::default()),
            st: RefCell::new(TradingPostState {
                sell_slot: Vec::new(),
                coin_count: 0,
                price_tier: 0,
                seller_client_name: None,
                needs_to_update_bitmap_string: false,
                inventory_needs_to_be_sent_with_update: false,
            }),
            links,
        })
    }

    /// `-initSlotsWithSaveDict:` (`saveDict` may be nil or any plist dictionary).
    fn init_slots_with_save_dict(&self, save_dict: Option<&PlistValue>) {
        let mut slot: Vec<ItemRef> = Vec::new();
        if let Some(PlistValue::Array(a)) = save_dict.and_then(|d| d.object_for_key("sellSlot")) {
            for e in &a.items {
                if let PlistValue::Data(d) = e {
                    let it = InventoryItem::from_save_data(&d.bytes);
                    if it.borrow().item_type.0 != ITEM_TIME_CRYSTAL {
                        slot.push(it);
                    }
                }
            }
        }
        let mut st = self.st.borrow_mut();
        st.sell_slot = slot;
        st.needs_to_update_bitmap_string = true;
    }

    /// `-initSubDerivedItems`: the seller alias lookup is the only server-visible part.
    fn init_sub_derived_items(&self) {
        if self.st.borrow().seller_client_name.is_none() {
            let owner = self.base.borrow().owner_id.clone();
            // `[[[world server] playerInfoForPeerID:ownerID] objectForKey:@"alias"]`; a nil ownerID gives nil
            if let Some(o) = owner {
                let server = self.links.dw().server.borrow().clone();
                if let Some(s) = server {
                    let alias = s
                        .player_info_for_peer_id(&o)
                        .and_then(|i| i.object_for_key("alias").and_then(|v| v.as_str()).map(|x| x.to_string()));
                    self.st.borrow_mut().seller_client_name = alias;
                }
            }
        }
    }

    /// Alias for [`TradingPost::new_at_position_item_flipped_save_dict_placed_by_client_client_name`].
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
    ) -> Option<Rc<TradingPost>> {
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

    /// `-[TradingPost initWithWorld:dynamicWorld:atPosition:cache:item:flipped:saveDict:placedByClient:clientName:]`.
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
    ) -> Option<Rc<TradingPost>> {
        let me = Self::alloc(world, dw);
        let me_obj: ObjRef = me.clone();
        let (w, d) = me.links.refs();
        // the base initialiser always succeeds here, `None` is only the ObjC `self = nil` shape
        interaction_object_init_with_world_dynamic_world_at_position_cache_item_flipped_save_dict_placed_by_client_client_name(
            &me_obj, &w, &d, pos, cache, item, flipped, save_dict, client_id, client_name,
        )
            .then_some(())?;
        let sd = save_dict.map(|d| PlistValue::Dict(d.clone()));
        me.init_slots_with_save_dict(sd.as_ref());
        {
            let mut st = me.st.borrow_mut();
            st.coin_count =
                save_dict.and_then(|d| d.map.get(&"coinCount".to_string())).map_or(0, |v| v.long_long_value() as i32);
            st.price_tier =
                save_dict.and_then(|d| d.map.get(&"priceTier".to_string())).map_or(0, |v| v.long_long_value() as i32);
            st.seller_client_name = match client_name {
                None => dict_str(sd.as_ref(), "sellerClientName"),
                Some(n) => Some(n.to_string()),
            };
        }
        if me_obj.owner_id().is_none() {
            me_obj.set_owner_id(dict_str(sd.as_ref(), "sellerClientID"));
        }
        me.init_sub_derived_items();
        Some(me)
    }

    /// `-[TradingPost initWithWorld:dynamicWorld:saveDict:cache:]` (note the different assignment order).
    pub fn from_save_dict(
        world: &Rc<World>,
        dw: &Rc<DynamicWorld>,
        save_dict: &PlistDict,
        cache: CacheRef,
    ) -> Option<Rc<TradingPost>> {
        let me = Self::alloc(world, dw);
        let me_obj: ObjRef = me.clone();
        let (w, d) = me.links.refs();
        // the base initialiser always succeeds here, `None` is only the ObjC `self = nil` shape
        interaction_object_init_with_world_dynamic_world_save_dict_cache(&me_obj, &w, &d, save_dict, cache)
            .then_some(())?;
        let sd = PlistValue::Dict(save_dict.clone());
        if me_obj.owner_id().is_none() {
            me_obj.set_owner_id(dict_str(Some(&sd), "sellerClientID"));
        }
        {
            let mut st = me.st.borrow_mut();
            st.seller_client_name = dict_str(Some(&sd), "sellerClientName");
            st.coin_count = save_dict.map.get(&"coinCount".to_string()).map_or(0, |v| v.long_long_value() as i32);
            st.price_tier = save_dict.map.get(&"priceTier".to_string()).map_or(0, |v| v.long_long_value() as i32);
        }
        me.init_slots_with_save_dict(Some(&sd));
        me.init_sub_derived_items();
        Some(me)
    }

    /// `-[TradingPost initWithWorld:dynamicWorld:cache:netData:]` (client-created record, `isNet = YES`).
    pub fn from_net_data(
        world: &Rc<World>,
        dw: &Rc<DynamicWorld>,
        cache: CacheRef,
        net_data: &[u8],
    ) -> Option<Rc<TradingPost>> {
        let me = Self::alloc(world, dw);
        let me_obj: ObjRef = me.clone();
        let (w, d) = me.links.refs();
        if !interaction_object_init_with_world_dynamic_world_cache_net_data(&me_obj, &w, &d, cache, net_data) {
            return None;
        }
        let hdr = TradingPostNetData::read_le(&padded_bytes(net_data, 0x30));
        let dict = gunzip_plist(subdata_from(net_data, 0x30));
        me_obj.set_owner_id(dict_str(dict.as_ref(), "ownerID"));
        me.io.borrow_mut().owner_name = dict_str(dict.as_ref(), "ownerName");
        me.st.borrow_mut().seller_client_name = dict_str(dict.as_ref(), "sellerClientName");
        me.init_slots_with_save_dict(dict.as_ref());
        {
            let mut st = me.st.borrow_mut();
            st.coin_count = hdr.coin_count as i32;
            st.price_tier = hdr.price_tier as i32;
        }
        let blockheads = dw.blockheads();
        let loaded = blockheads.iter().filter_map(|b| handle::downcast::<crate::blockhead::blockhead::Blockhead>(b));
        loaded.for_each(|bh| bh.net_interaction_object_was_loaded(&me_obj));
        me.init_sub_derived_items();
        Some(me)
    }

    /// `-sellSlot` @0xa0 (handles).
    pub fn sell_slot(&self) -> Vec<ItemRef> {
        self.st.borrow().sell_slot.clone()
    }
    /// `-coinCount`
    pub fn coin_count(&self) -> i32 {
        self.st.borrow().coin_count
    }
    /// `-setCoinCount:`
    pub fn set_coin_count(&self, v: i32) {
        self.st.borrow_mut().coin_count = v;
    }
    /// `-priceTier`
    pub fn price_tier(&self) -> i32 {
        self.st.borrow().price_tier
    }
    /// `-setPriceTier:` (does NOT set `updateNeedsToBeSent`). Unreachable on the server (P1).
    pub fn set_price_tier(&self, me: &ObjRef, v: i32) {
        self.st.borrow_mut().price_tier = v;
        self.links.dw().dynamic_world_changed_at_pos_object_type(me.pos(), DYNAMIC_OBJECT_TYPE_TRADING_POST);
        self.st.borrow_mut().needs_to_update_bitmap_string = true;
    }
    /// `-price`: `priceTiers[priceTier]` (unchecked in the original; 86 and 87 read 0, anything else is out of range).
    pub fn price(&self) -> i32 {
        let t = self.st.borrow().price_tier;
        if t < 0 {
            return 0;
        }
        PRICE_TIERS.get(t as usize).copied().unwrap_or(0)
    }

    /// `-contentsDidChange`.
    pub fn contents_did_change(&self, me: &ObjRef) {
        me.set_update_needs_to_be_sent(true);
        {
            let mut st = self.st.borrow_mut();
            st.inventory_needs_to_be_sent_with_update = true;
        }
        self.links.dw().dynamic_world_changed_at_pos_object_type(me.pos(), DYNAMIC_OBJECT_TYPE_TRADING_POST);
        self.st.borrow_mut().needs_to_update_bitmap_string = true;
    }

    /// `-moveInventoryItemsToSellSlotFromArray:count:` (unreachable on the server, P1).
    pub fn move_inventory_items_to_sell_slot_from_array_count(&self, me: &ObjRef, from: &SubSlot, count: i32) -> i32 {
        let mut count = count;
        if count == 0 || from.borrow().is_empty() {
            return 0;
        }
        let drag_to_count = self.st.borrow().sell_slot.len() as i32;
        let (from_type, from_data_b) = {
            let f = from.borrow();
            let f0 = f[0].borrow();
            (f0.item_type, f0.data_b)
        };
        if drag_to_count >= 1 {
            let (t, db) = {
                let st = self.st.borrow();
                let s0 = st.sell_slot[0].borrow();
                (s0.item_type, s0.data_b)
            };
            if t != from_type {
                return 0;
            }
            if !item_type_is_stackable(from_type, from_data_b, db) {
                return 0;
            }
        }
        let from_count = from.borrow().len();
        if (from_count as u64) < (count as i64 as u64) {
            count = from_count as i32;
        }
        let max_can_hold = 99 - drag_to_count;
        let amount = max_can_hold.min(count);
        let start = (from_count as i64 - amount as i64) as usize;
        let dw = self.links.dw();
        let pos = me.pos();
        let cur = me.current_blockhead();
        let mut j = 0;
        while j < amount {
            let item = from.borrow()[start].clone();
            let t = item.borrow().item_type.0;
            let mut replacement: Option<ItemRef> = None;
            let spill = |sub: &ItemRef| {
                let s = sub.borrow();
                if s.item_type.0 != ITEM_TIME_CRYSTAL {
                    dw.create_free_block_at_position_of_type_data_a_data_b_sub_items_dynamic_object_save_dict_hovers_play_sound_priority_blockhead(
                        pos,
                        s.item_type,
                        s.data_a,
                        s.data_b,
                        sub_items_owned(&s),
                        s.dynamic_object_save_dict.clone(),
                        false,
                        2,
                        cur.as_ref(),
                    );
                }
            };
            match t {
                0x413 | 0x430 | 0x450 | 0x429 | 0xa1 | 0xcf | 0xce => {
                    let sd = item.borrow().dynamic_object_save_dict.clone();
                    if let Some(sd) = sd {
                        let slots = if t == 0xce {
                            sd.object_for_key("chest").and_then(|c| c.object_for_key("saveItemSlots")).cloned()
                        } else {
                            sd.object_for_key("saveItemSlots").cloned()
                        };
                        if let Some(PlistValue::Array(slots)) = slots {
                            for slot in &slots.items {
                                if let PlistValue::Array(items) = slot {
                                    for e in &items.items {
                                        if let PlistValue::Data(d) = e {
                                            let sub = InventoryItem::from_save_data(&d.bytes);
                                            spill(&sub);
                                        }
                                    }
                                }
                            }
                        }
                    }
                    let (ty, da, db) = {
                        let i = item.borrow();
                        (i.item_type, i.data_a, i.data_b)
                    };
                    replacement = Some(InventoryItem::new(ty, da, db, None, None));
                }
                0xc => {
                    let subs = item.borrow().sub_items.clone();
                    if let Some(subs) = subs {
                        for slot in &subs {
                            for sub in slot.borrow().iter() {
                                spill(sub);
                            }
                        }
                    }
                    let (ty, da, db) = {
                        let i = item.borrow();
                        (i.item_type, i.data_a, i.data_b)
                    };
                    replacement = Some(InventoryItem::new(ty, da, db, None, None));
                }
                _ => {}
            }
            self.st.borrow_mut().sell_slot.push(replacement.unwrap_or(item));
            from.borrow_mut().remove(start);
            j += 1;
        }
        self.contents_did_change(me);
        amount
    }

    /// `-buyButtonTappedWithQuantity:` (unreachable on the server, P1).
    pub fn buy_button_tapped_with_quantity(&self, me: &ObjRef, quantity: i32) {
        let Some(buyer) = me.current_blockhead() else { return };
        if (quantity as i64 as u64) > self.st.borrow().sell_slot.len() as u64 {
            return;
        }
        let api = blockhead_inventory_api();
        let total = quantity.wrapping_mul(self.price());
        if total > api.total_cash(&buyer) {
            return;
        }
        {
            let mut st = self.st.borrow_mut();
            st.coin_count = st.coin_count.wrapping_add(total);
        }
        api.subtract_cash(&buyer, total);
        let dw = self.links.dw();
        for _ in 0..quantity {
            let item = self.st.borrow().sell_slot.last().cloned();
            let Some(item) = item else { break };
            let mut sd = item.borrow().dynamic_object_save_dict.clone();
            if let Some(PlistValue::Dict(d)) = &mut sd {
                for k in
                    ["sellerClientID", "sellerClientName", "safeClientID", "ironPlaceClientID", "ownerID", "ownerName"]
                {
                    d.map.remove(&k.to_string());
                }
            }
            {
                let i = item.borrow();
                if i.item_type.0 != ITEM_TIME_CRYSTAL {
                    dw.create_free_block_at_position_of_type_data_a_data_b_sub_items_dynamic_object_save_dict_hovers_play_sound_priority_blockhead(
                        me.pos(),
                        i.item_type,
                        i.data_a,
                        i.data_b,
                        sub_items_owned(&i),
                        sd,
                        false,
                        4,
                        Some(&buyer),
                    );
                }
            }
            self.st.borrow_mut().sell_slot.pop();
        }
        me.set_update_needs_to_be_sent(true);
        {
            let mut st = self.st.borrow_mut();
            st.needs_to_update_bitmap_string = true;
            st.inventory_needs_to_be_sent_with_update = true;
        }
        self.links.world().report_achievement_with_identifier("grp.tycoon");
    }

    /// `-blockheadIsSeller:` (never invoked by the server).
    pub fn blockhead_is_seller(&self, me: &ObjRef, b: &ObjRef) -> bool {
        let bh = b.as_blockhead();
        let local = bh.and_then(|x| x.local_net_id());
        let owner = me.owner_id();
        if local.is_none() && owner.is_none() {
            return true;
        }
        if local.is_some() && local == owner {
            return true;
        }
        // `[dynamicWorld isClient]` is NO on the server
        if let Some(o) = &owner {
            if o != "server" {
                return false;
            }
        }
        !bh.is_some_and(|x| x.is_client_blockhead_being_controlled_by_server())
    }

    /// `-sellTitle` (never invoked by the server).
    pub fn sell_title(&self) -> String {
        match &self.st.borrow().seller_client_name {
            None => "SHOP".to_string(),
            Some(n) => format!("SHOP ({})", n),
        }
    }

    fn sell_slot_blobs(&self) -> PlistValue {
        let st = self.st.borrow();
        mutable_array_of(
            st.sell_slot
                .iter()
                .filter(|i| i.borrow().item_type.0 != ITEM_TIME_CRYSTAL)
                .map(|i| PlistValue::data(i.borrow().save_data()))
                .collect(),
        )
    }
}

impl DynamicObject for TradingPost {
    fn base(&self) -> &RefCell<DynamicObjectBase> {
        &self.base
    }
    fn class_info(&self) -> &'static ClassInfo {
        &TRADING_POST_CLASS
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
    /// `-objectType`
    fn object_type(&self) -> i32 {
        DYNAMIC_OBJECT_TYPE_TRADING_POST
    }
    /// `-occupiesNormalContents`
    fn occupies_normal_contents(&self) -> bool {
        true
    }
    /// `-freeblockCreationItemType`
    fn freeblock_creation_item_type(&self) -> i32 {
        ITEM_TRADING_POST
    }
    /// `-freeBlockCreationSaveDict`
    fn free_block_creation_save_dict(&self, me: &ObjRef) -> Option<PlistDict> {
        Some(self.get_save_dict(me))
    }
    /// `-getSaveDict`: `sellerClientName`, `sellSlot` (0xb filtered), `coinCount`, `priceTier`.
    fn get_save_dict(&self, me: &ObjRef) -> PlistDict {
        let mut d = interaction_object_get_save_dict(me);
        if let Some(n) = self.st.borrow().seller_client_name.clone() {
            d.map.insert("sellerClientName".to_string(), PlistValue::string(n));
        }
        d.map.insert("sellSlot".to_string(), self.sell_slot_blobs());
        let st = self.st.borrow();
        d.map.insert("coinCount".to_string(), PlistValue::int(st.coin_count as i64));
        d.map.insert("priceTier".to_string(), PlistValue::int(st.price_tier as i64));
        d
    }
    /// `-creationNetDataForClient:`: forces the sell slot into the record.
    fn creation_net_data_for_client(&self, me: &ObjRef, client: Option<&str>) -> Option<Vec<u8>> {
        self.st.borrow_mut().inventory_needs_to_be_sent_with_update = true;
        self.update_net_data_for_client(me, client)
    }
    /// `-updateNetDataForClient:` (trap 4: `sellerClientName` only inside the `ownerID != nil` branch).
    fn update_net_data_for_client(&self, me: &ObjRef, _client: Option<&str>) -> Option<Vec<u8>> {
        let mut s = TradingPostNetData::default();
        s.interaction_object_creation_net_data = interaction_object_interaction_object_creation_net_data(me);
        {
            let st = self.st.borrow();
            s.coin_count = st.coin_count as u32;
            s.price_tier = st.price_tier as u16;
        }
        let mut out = s.to_vec();
        let mut dd = crate::objects::base::new_mutable_dict();
        let embed = self.st.borrow().inventory_needs_to_be_sent_with_update;
        let slot_blobs = if embed { Some(self.sell_slot_blobs()) } else { None };
        if let Some(o) = me.owner_id() {
            dd.map.insert("ownerID".to_string(), PlistValue::string(o));
            if let Some(n) = self.st.borrow().seller_client_name.clone() {
                dd.map.insert("sellerClientName".to_string(), PlistValue::string(n));
            }
        }
        if let Some(n) = self.io.borrow().owner_name.clone() {
            dd.map.insert("ownerName".to_string(), PlistValue::string(n));
        }
        if let Some(b) = slot_blobs {
            dd.map.insert("sellSlot".to_string(), b);
            self.st.borrow_mut().inventory_needs_to_be_sent_with_update = false;
        }
        out.extend_from_slice(&gz_plist_bytes(&PlistValue::Dict(dd)));
        Some(out)
    }
    /// `-remoteUpdate:` (trap 5: super first, the overwrite runs even on a just-removed object).
    fn remote_update(&self, me: &ObjRef, net_data: &[u8]) {
        interaction_object_remote_update(me, net_data);
        if self.io.borrow().current_blockhead.is_none() {
            let hdr = TradingPostNetData::read_le(&padded_bytes(net_data, 0x30));
            {
                let mut st = self.st.borrow_mut();
                st.coin_count = hdr.coin_count as i32;
                st.price_tier = hdr.price_tier as i32;
            }
            let dict = gunzip_plist(subdata_from(net_data, 0x30));
            me.set_owner_id(dict_str(dict.as_ref(), "ownerID"));
            self.io.borrow_mut().owner_name = dict_str(dict.as_ref(), "ownerName");
            self.st.borrow_mut().seller_client_name = dict_str(dict.as_ref(), "sellerClientName");
            if dict.as_ref().and_then(|d| d.object_for_key("sellSlot")).is_some() {
                self.st.borrow_mut().sell_slot.clear();
                self.init_slots_with_save_dict(dict.as_ref());
                if !self.base.borrow().is_net {
                    self.st.borrow_mut().inventory_needs_to_be_sent_with_update = true;
                }
            }
        }
    }
    /// `-update:accurateDT:isSimulation:`
    fn update_accurate_dt_is_simulation(&self, me: &ObjRef, dt: f32, accurate_dt: f32, is_simulation: bool) {
        if !self.base.borrow().needs_removed {
            interaction_object_update_accurate_dt_is_simulation(me, dt, accurate_dt, is_simulation);
        }
    }
    /// `-worldChanged:` (support check, E4 for the NULL left / right / up tiles).
    fn world_changed(&self, me: &ObjRef, positions: &mut Vec<IntPair>) {
        if self.base.borrow().is_net {
            return;
        }
        let world = self.links.world();
        let pos = self.base.borrow().pos;
        if support_lost(&world, pos, positions) {
            self.remove(me, None);
        }
    }
    /// `-worldContentsChanged:` (empty).
    fn world_contents_changed(&self, _me: &ObjRef, _positions: &mut Vec<IntPair>) {}
    fn dealloc(&self, me: &ObjRef) {
        {
            let mut st = self.st.borrow_mut();
            st.seller_client_name = None;
            st.sell_slot.clear();
        }
        interaction_object_dealloc(me)
    }
    fn blockheads_loaded(&self, me: &ObjRef) {
        interaction_object_blockheads_loaded(me)
    }
    fn blockhead_unloaded(&self, me: &ObjRef, b: &ObjRef) {
        interaction_object_blockhead_unloaded(me, b)
    }
}

/// The shared "support lost" test of Chest (standard branch), TradingPost and TradePortal `worldChanged:`:
/// true when the object must remove itself.
pub fn support_lost(world: &Rc<World>, pos: IntPair, positions: &[IntPair]) -> bool {
    let w = world.world_width_macro();
    for c in positions {
        let d = pos.x.wrapping_sub(c.x);
        let dx = if d < w.wrapping_mul(32) / 2 {
            if d < w.wrapping_mul(-32) / 2 {
                d.wrapping_add(w.wrapping_mul(32))
            } else {
                d
            }
        } else {
            d.wrapping_sub(w.wrapping_mul(32))
        };
        if dx.wrapping_abs() < 2 && (c.y == pos.y - 1 || pos.y <= c.y) {
            let below = tile_at_world_position_loaded(pos.x, pos.y - 1, world);
            if let Some(b) = below {
                if !tile_is_solid(Some(b)) {
                    let ok = !tile_is_half_depth(Some(b), world, IntPair { x: pos.x, y: pos.y - 1 })
                        && b.contents() != CONTENTS_COPPER_WIRE;
                    if ok {
                        let here = tile_at_world_position_loaded(pos.x, pos.y, world);
                        if let Some(h) = here {
                            if h.back_wall_type_index() == 2 {
                                let ok_n = |x: i32, y: i32| -> bool {
                                    match tile_at_world_position_loaded(x, y, world) {
                                        // UB-PARITY: unguarded NULL dereference of left / right / above in the original
                                        None => ub_null_tile(),
                                        Some(t) => {
                                            !tile_is_half_depth(Some(t), world, IntPair { x, y })
                                                && t.contents() != CONTENTS_COPPER_WIRE
                                                && !tile_is_solid(Some(t))
                                        }
                                    }
                                };
                                if ok_n(pos.x - 1, pos.y) && ok_n(pos.x + 1, pos.y) && ok_n(pos.x, pos.y + 1) {
                                    return true;
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    false
}

impl InteractionObjectLike for TradingPost {
    fn io_state(&self) -> &RefCell<InteractionObjectState> {
        &self.io
    }
    fn interaction_object_type(&self) -> u16 {
        INTERACTION_OBJECT_TYPE_TRADING_POST
    }
    fn destroy_item_type(&self) -> i32 {
        ITEM_TRADING_POST
    }
    fn title(&self) -> String {
        "SHOP".to_string()
    }
    fn action_title(&self) -> String {
        format!("USE {}", self.title())
    }
    fn requires_human_interaction(&self) -> bool {
        true
    }
    fn is_paintable(&self) -> bool {
        true
    }
    /// `-remove:` (drops the post with its save dict, `playSound = 1`).
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
            if cur.as_ref().is_some_and(|c| c.is_net()) || remote != 0 {
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
            ItemType(ITEM_TRADING_POST as u32),
            0,
            0,
            None,
            Some(PlistValue::Dict(dict)),
            false,
            1,
            remove_blockhead,
        );
        world.remove_tile_at_world_x_world_y_create_contents_freeblock_count_create_foreground_contents_freeblock_count_remove_blockhead(pos.x, pos.y, 0, 0, remove_blockhead);
        self.set_needs_removed(me, true);
    }
    /// `-paint:`
    fn paint(&self, me: &ObjRef, color_index: u16) {
        self.io.borrow_mut().paint_color = color_index;
        if !self.base.borrow().is_net {
            me.set_update_needs_to_be_sent(true);
            self.links
                .dw()
                .dynamic_world_changed_at_pos_object_type(self.base.borrow().pos, DYNAMIC_OBJECT_TYPE_TRADING_POST);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::interaction::sign::testkit::*;

    fn mk(name: &str, owner: Option<&str>, sd: Option<PlistDict>) -> (Rc<World>, Rc<DynamicWorld>, Rc<TradingPost>) {
        let (w, d, _pb) = world_with_block(name);
        let it = InventoryItem::new(ItemType(168), 0, 0, None, None);
        let it = it.borrow();
        let p =
            TradingPost::new(&w, &d, IntPair { x: 10, y: 10 }, None, Some(&it), false, sd.as_ref(), owner, Some("Bob"))
                .unwrap();
        (w, d, p)
    }

    fn slot_dict(items: &[(u32, u16)], coins: i32, tier: i32) -> PlistDict {
        let mut sd = crate::objects::base::new_mutable_dict();
        let blobs: Vec<PlistValue> = items
            .iter()
            .map(|(t, b)| PlistValue::data(InventoryItem::new(ItemType(*t), 0, *b, None, None).borrow().save_data()))
            .collect();
        sd.map.insert("sellSlot".to_string(), mutable_array_of(blobs));
        sd.map.insert("coinCount".to_string(), PlistValue::int(coins as i64));
        sd.map.insert("priceTier".to_string(), PlistValue::int(tier as i64));
        sd
    }

    #[test]
    fn price_table_shape() {
        assert_eq!(PRICE_TIERS.len(), 86);
        assert_eq!(PRICE_TIERS[85], 10_000_000);
        assert_eq!(PRICE_TIERS[10], 15);
    }

    /// placement from an item save dict (time crystals skipped, owner name from the client).
    #[test]
    fn placement_and_save_dict() {
        let (_w, _d, p) = mk("tp_a", Some("cid"), Some(slot_dict(&[(5, 0), (11, 0), (5, 0)], 40, 12)));
        let me: ObjRef = p.clone();
        assert_eq!(p.sell_slot().len(), 2);
        assert_eq!(p.coin_count(), 40);
        assert_eq!(p.price_tier(), 12);
        assert_eq!(p.price(), 25);
        assert_eq!(p.st.borrow().seller_client_name.as_deref(), Some("Bob"));
        let d = me.get_save_dict(&me);
        for k in ["sellerClientName", "sellSlot", "coinCount", "priceTier", "interactionObjectType"] {
            assert!(d.map.get(&k.to_string()).is_some(), "{k}");
        }
        let Some(PlistValue::Array(a)) = d.map.get(&"sellSlot".to_string()) else { unreachable!("sellSlot") };
        assert_eq!(a.items.len(), 2);
        assert_eq!(me.object_type(), 48);
        assert_eq!(p.interaction_object_type(), 5);
        assert_eq!(p.title(), "SHOP");
        assert_eq!(p.action_title(), "USE SHOP");
        // reload keeps everything; the legacy sellerClientID fills a missing owner
        let mut d2 = d.clone();
        d2.map.remove(&"ownerID".to_string());
        d2.map.insert("sellerClientID".to_string(), PlistValue::string("legacy"));
        let p2 = TradingPost::from_save_dict(&_w, &_d, &d2, None).unwrap();
        let me2: ObjRef = p2.clone();
        assert_eq!(me2.owner_id().as_deref(), Some("legacy"));
        assert_eq!(p2.sell_slot().len(), 2);
        assert_eq!(p2.price_tier(), 12);
    }

    /// header, padding, the embed-once flag and the nested sellerClientName.
    #[test]
    fn net_data_layout_and_embed_once() {
        let (_w, _d, p) = mk("tp_b", Some("cid"), Some(slot_dict(&[(5, 0)], 7, 3)));
        let me: ObjRef = p.clone();
        let b = me.creation_net_data_for_client(&me, None).unwrap();
        assert_eq!(u32::from_le_bytes(b[0x28..0x2c].try_into().unwrap()), 7);
        assert_eq!(u16::from_le_bytes(b[0x2c..0x2e].try_into().unwrap()), 3);
        assert_eq!(&b[0x2e..0x30], &[0, 0]);
        let dict = gunzip_plist(&b[0x30..]).unwrap();
        assert_eq!(dict_str(Some(&dict), "sellerClientName").as_deref(), Some("Bob"));
        assert!(dict.object_for_key("sellSlot").is_some());
        // the update that follows creation does not repeat the inventory
        let b2 = me.update_net_data_for_client(&me, None).unwrap();
        assert!(gunzip_plist(&b2[0x30..]).unwrap().object_for_key("sellSlot").is_none());
        // no owner -> no sellerClientName in the plist
        me.set_owner_id(None);
        let b3 = me.update_net_data_for_client(&me, None).unwrap();
        assert!(gunzip_plist(&b3[0x30..]).unwrap().object_for_key("sellerClientName").is_none());
    }

    #[test]
    fn remote_update_adopts_state_verbatim() {
        let (_w, _d, p) = mk("tp_c", Some("cid"), None);
        let me: ObjRef = p.clone();
        let mut hdr = TradingPostNetData::default();
        hdr.coin_count = 99;
        hdr.price_tier = 65535;
        let mut upd = hdr.to_vec();
        let mut dict = PlistValue::mutable_dict();
        if let PlistValue::Dict(dd) = &mut dict {
            dd.map.insert("ownerID".to_string(), PlistValue::string("zed"));
            dd.map.insert(
                "sellSlot".to_string(),
                mutable_array_of(vec![PlistValue::data(
                    InventoryItem::new(ItemType(9), 0, 0, None, None).borrow().save_data(),
                )]),
            );
        }
        upd.extend_from_slice(&gz_plist_bytes(&dict));
        p.remote_update(&me, &upd);
        assert_eq!(p.coin_count(), 99);
        assert_eq!(p.price_tier(), 65535);
        assert_eq!(me.owner_id().as_deref(), Some("zed"));
        assert_eq!(p.st.borrow().seller_client_name, None);
        assert_eq!(p.sell_slot().len(), 1);
        assert!(p.st.borrow().inventory_needs_to_be_sent_with_update);
        assert_eq!(p.price(), 0, "out-of-range tier reads 0 here");
    }

    #[test]
    fn move_items_to_sell_slot() {
        let (_w, _d, p) = mk("tp_d", Some("cid"), None);
        let me: ObjRef = p.clone();
        let from: SubSlot =
            Rc::new(RefCell::new((0..5).map(|_| InventoryItem::new(ItemType(5), 0, 0, None, None)).collect()));
        assert_eq!(p.move_inventory_items_to_sell_slot_from_array_count(&me, &from, -1), 5);
        assert_eq!(p.sell_slot().len(), 5);
        assert!(from.borrow().is_empty());
        let other: SubSlot = Rc::new(RefCell::new(vec![InventoryItem::new(ItemType(6), 0, 0, None, None)]));
        assert_eq!(p.move_inventory_items_to_sell_slot_from_array_count(&me, &other, 1), 0);
        assert!(p.st.borrow().inventory_needs_to_be_sent_with_update);
    }

    #[test]
    fn support_check_wrap_and_pure_air() {
        let (w, _d, pb) = world_with_block("tp_e");
        // everything is air: below non-solid, own back wall 0 != 2 -> no removal
        assert!(!support_lost(&w, IntPair { x: 10, y: 10 }, &[IntPair { x: 10, y: 9 }]));
        tile(pb, 10, 10).set_back_wall_type_index(2);
        assert!(support_lost(&w, IntPair { x: 10, y: 10 }, &[IntPair { x: 10, y: 9 }]));
        // a changed tile two columns away does not matter
        assert!(!support_lost(&w, IntPair { x: 10, y: 10 }, &[IntPair { x: 12, y: 9 }]));
        // a solid left neighbour keeps the post
        tile(pb, 9, 10).set_type_index(6);
        assert!(!support_lost(&w, IntPair { x: 10, y: 10 }, &[IntPair { x: 10, y: 9 }]));
    }

    use crate::objects::interaction_object::testutil::TestBh;
    use std::cell::Cell;

    /// Cash and inventory seam double.
    struct Api {
        cash: Cell<i32>,
        spent: Cell<i32>,
    }
    impl BlockheadInventoryApi for Api {
        fn total_cash(&self, _b: &ObjRef) -> i32 {
            self.cash.get()
        }
        fn subtract_cash(&self, _b: &ObjRef, amount: i32) {
            self.spent.set(self.spent.get() + amount);
        }
    }

    #[test]
    fn inventory_seam_defaults_and_item_copies() {
        let b: ObjRef = TestBh::new(1, false);
        let api = blockhead_inventory_api();
        assert_eq!(api.total_cash(&b), 0);
        api.subtract_cash(&b, 5);
        assert_eq!(api.count_of_inventory_items_of_type_include_actions(&b, 5, false), 0);
        assert_eq!(
            api.count_of_inventory_items_with_specific_data_b_of_type_data_b_include_actions(&b, 5, 1, false),
            0
        );
        assert_eq!(api.usage_multiplier_for_first_item_of_type(&b, 5), 1.0);
        assert_eq!(api.subtract_items_from_inventory_of_type_count_data_b(&b, 5, 2, -1), 0);
        set_blockhead_inventory_api(Rc::new(Api { cash: Cell::new(77), spent: Cell::new(0) }));
        assert_eq!(blockhead_inventory_api().total_cash(&b), 77);
        // copies share their sub slots
        let basket = InventoryItem::new(ItemType(0xc), 1, 2, None, None);
        let copy = clone_inventory_item(&basket.borrow());
        assert_eq!((copy.item_type, copy.data_a, copy.data_b), (ItemType(0xc), 1, 2));
        assert!(Rc::ptr_eq(&copy.sub_items.as_ref().unwrap()[0], &basket.borrow().sub_items.as_ref().unwrap()[0]));
        assert_eq!(sub_items_owned(&basket.borrow()).unwrap().len(), 4);
        basket.borrow().sub_items.as_ref().unwrap()[1].borrow_mut().push(InventoryItem::new(
            ItemType(5),
            3,
            4,
            None,
            None,
        ));
        let owned = sub_items_owned(&basket.borrow()).unwrap();
        assert_eq!((owned[0].len(), owned[1].len()), (0, 1));
        assert_eq!((owned[1][0].item_type, owned[1][0].data_a, owned[1][0].data_b), (ItemType(5), 3, 4));
        assert!(sub_items_owned(&InventoryItem::new(ItemType(5), 0, 0, None, None).borrow()).is_none());
    }

    #[test]
    fn replicas_and_server_aliases() {
        let (w, d, _pb) = world_with_block("tp_f");
        install_server(&d);
        // the owner's alias becomes the seller name on a server
        let it = InventoryItem::new(ItemType(168), 0, 0, None, None);
        let p = TradingPost::new(
            &w,
            &d,
            IntPair { x: 10, y: 10 },
            None,
            Some(&it.borrow()),
            false,
            None,
            Some("owner1"),
            None,
        )
        .unwrap();
        assert_eq!(p.st.borrow().seller_client_name.as_deref(), Some("Ally"));
        let unknown = TradingPost::new(
            &w,
            &d,
            IntPair { x: 12, y: 10 },
            None,
            Some(&it.borrow()),
            false,
            None,
            Some("zed"),
            None,
        )
        .unwrap();
        assert_eq!(unknown.st.borrow().seller_client_name, None);
        // net replica with a blockhead in the world
        let me: ObjRef = p.clone();
        p.set_coin_count(3);
        let bytes = me.creation_net_data_for_client(&me, None).unwrap();
        let (bw, _bd, bdw) = crate::blockhead::blockhead::testutil::setup();
        d.blockheads.borrow_mut().push(crate::blockhead::blockhead::testutil::new_bh(&bw, &bdw, 0));
        assert_eq!(d.blockheads().len(), 1);
        assert!(
            crate::objects::handle::downcast::<crate::blockhead::blockhead::Blockhead>(&d.blockheads()[0]).is_some()
        );
        let rep = TradingPost::from_net_data(&w, &d, None, &bytes).unwrap();
        d.blockheads.borrow_mut().clear();
        assert_eq!(rep.coin_count(), 3);
        assert_eq!(rep.st.borrow().seller_client_name.as_deref(), Some("Ally"));
        let mut far = bytes.clone();
        far[8..12].copy_from_slice(&900u32.to_le_bytes());
        far[12..16].copy_from_slice(&900u32.to_le_bytes());
        assert!(TradingPost::from_net_data(&w, &d, None, &far).is_none());
        let e = crate::foundation::exception::catch_ns(|| {
            TradingPost::from_net_data(&w, &d, None, &bytes[..0x2c]).is_some()
        })
        .unwrap_err();
        assert_eq!(e.name, "NSRangeException");
    }

    #[test]
    fn price_tier_and_contents_changes() {
        let (_w, _d, p) = mk("tp_g", Some("cid"), None);
        let me: ObjRef = p.clone();
        p.set_price_tier(&me, 4);
        assert_eq!((p.price_tier(), p.price()), (4, 5));
        assert!(p.st.borrow().needs_to_update_bitmap_string);
        p.set_price_tier(&me, -1);
        assert_eq!(p.price(), 0);
        me.set_update_needs_to_be_sent(false);
        p.contents_did_change(&me);
        assert!(me.update_needs_to_be_sent() && p.st.borrow().inventory_needs_to_be_sent_with_update);
    }

    #[test]
    fn moving_containers_into_the_sell_slot_spills_their_contents() {
        let (_w, _d, p) = mk("tp_h", Some("cid"), None);
        let me: ObjRef = p.clone();
        let user: ObjRef = TestBh::new(3, false);
        p.io.borrow_mut().current_blockhead = Some(user);
        let blob = |t: u32| PlistValue::data(InventoryItem::new(ItemType(t), 1, 2, None, None).borrow().save_data());
        // a chest item with saved slots: every stored item spills out as a free block, the chest itself stays empty
        let mut sd = crate::objects::base::new_mutable_dict();
        sd.map.insert(
            "saveItemSlots".to_string(),
            mutable_array_of(vec![mutable_array_of(vec![blob(5), blob(11), PlistValue::int(1)]), PlistValue::int(2)]),
        );
        let chest = InventoryItem::new(ItemType(0x413), 0, 0, None, Some(PlistValue::Dict(sd)));
        let from: SubSlot = Rc::new(RefCell::new(vec![chest.clone()]));
        assert_eq!(p.move_inventory_items_to_sell_slot_from_array_count(&me, &from, 1), 1);
        assert!(p.sell_slot()[0].borrow().dynamic_object_save_dict.is_none());
        // the same for a cabinet-like item that nests its chest in the dictionary, and for one without anything saved
        let mut inner = crate::objects::base::new_mutable_dict();
        inner.map.insert("saveItemSlots".to_string(), mutable_array_of(vec![mutable_array_of(vec![blob(6)])]));
        let mut outer = crate::objects::base::new_mutable_dict();
        outer.map.insert("chest".to_string(), PlistValue::Dict(inner));
        let nested = InventoryItem::new(ItemType(0xce), 0, 0, None, Some(PlistValue::Dict(outer)));
        let bare = InventoryItem::new(ItemType(0x429), 0, 0, None, None);
        let junk = InventoryItem::new(ItemType(0xa1), 0, 0, None, Some(PlistValue::int(1)));
        for item in [nested, bare, junk] {
            let f: SubSlot = Rc::new(RefCell::new(vec![item]));
            let before = p.sell_slot().len();
            // containers only stack with their own kind
            p.st.borrow_mut().sell_slot.clear();
            assert_eq!(p.move_inventory_items_to_sell_slot_from_array_count(&me, &f, 1), 1);
            assert_eq!(p.sell_slot().len(), 1, "{before}");
        }
        // a basket spills its sub items
        let basket = InventoryItem::new(ItemType(0xc), 0, 0, None, None);
        basket.borrow().sub_items.as_ref().unwrap()[1].borrow_mut().extend([
            InventoryItem::new(ItemType(5), 0, 0, None, None),
            InventoryItem::new(ItemType(11), 0, 0, None, None),
        ]);
        p.st.borrow_mut().sell_slot.clear();
        let f: SubSlot = Rc::new(RefCell::new(vec![basket]));
        assert_eq!(p.move_inventory_items_to_sell_slot_from_array_count(&me, &f, 5), 1);
        // a slot only takes its own kind
        p.st.borrow_mut().sell_slot.clear();
        let f: SubSlot = Rc::new(RefCell::new(vec![InventoryItem::new(ItemType(5), 0, 0, None, None)]));
        assert_eq!(p.move_inventory_items_to_sell_slot_from_array_count(&me, &f, 1), 1);
        let again: SubSlot = Rc::new(RefCell::new(vec![InventoryItem::new(ItemType(5), 0, 0, None, None)]));
        assert_eq!(p.move_inventory_items_to_sell_slot_from_array_count(&me, &again, 1), 1);
        assert_eq!(p.sell_slot().len(), 2);
        let g: SubSlot = Rc::new(RefCell::new(vec![InventoryItem::new(ItemType(6), 0, 0, None, None)]));
        assert_eq!(p.move_inventory_items_to_sell_slot_from_array_count(&me, &g, 1), 0);
        // a basket without any slots has nothing to spill
        p.st.borrow_mut().sell_slot.clear();
        let bare = InventoryItem::new(ItemType(0xc), 0, 0, None, None);
        bare.borrow_mut().sub_items = None;
        let h: SubSlot = Rc::new(RefCell::new(vec![bare]));
        assert_eq!(p.move_inventory_items_to_sell_slot_from_array_count(&me, &h, 1), 1);
        // containers cannot be stacked
        let sword = || InventoryItem::new(ItemType(0xc), 0, 0, None, None);
        p.st.borrow_mut().sell_slot.clear();
        let f: SubSlot = Rc::new(RefCell::new(vec![sword(), sword()]));
        assert_eq!(p.move_inventory_items_to_sell_slot_from_array_count(&me, &f, 1), 1);
        assert_eq!(p.move_inventory_items_to_sell_slot_from_array_count(&me, &f, 1), 0);
        // nothing to move
        let empty: SubSlot = Rc::new(RefCell::new(vec![]));
        assert_eq!(p.move_inventory_items_to_sell_slot_from_array_count(&me, &empty, 1), 0);
        assert_eq!(p.move_inventory_items_to_sell_slot_from_array_count(&me, &f, 0), 0);
    }

    #[test]
    fn buying_from_a_shop() {
        let (w, _d, p) = mk("tp_i", Some("seller"), Some(slot_dict(&[(5, 0), (11, 0), (5, 0), (5, 0)], 0, 3)));
        let me: ObjRef = p.clone();
        let buyer: ObjRef = TestBh::new(5, false);
        // no buyer, too many wanted, not enough cash
        p.buy_button_tapped_with_quantity(&me, 1);
        p.io.borrow_mut().current_blockhead = Some(buyer.clone());
        p.buy_button_tapped_with_quantity(&me, 9);
        let api = Rc::new(Api { cash: Cell::new(3), spent: Cell::new(0) });
        set_blockhead_inventory_api(api.clone());
        p.buy_button_tapped_with_quantity(&me, 2);
        assert_eq!(p.sell_slot().len(), 3);
        // enough cash for two at 4 each; the seller's keys never travel with the goods
        api.cash.set(100);
        let mut sd = crate::objects::base::new_mutable_dict();
        sd.map.insert("sellerClientID".to_string(), PlistValue::string("x"));
        sd.map.insert("keep".to_string(), PlistValue::int(1));
        p.st.borrow().sell_slot[2].borrow_mut().dynamic_object_save_dict = Some(PlistValue::Dict(sd));
        p.buy_button_tapped_with_quantity(&me, 2);
        assert_eq!((api.spent.get(), p.coin_count()), (8, 8));
        assert_eq!(p.sell_slot().len(), 1);
        assert!(me.update_needs_to_be_sent());
        // crystals in the slot are never handed out
        p.st.borrow_mut().sell_slot.push(InventoryItem::new(ItemType(11), 0, 0, None, None));
        p.buy_button_tapped_with_quantity(&me, 1);
        assert_eq!(p.sell_slot().len(), 1);
        // seller test and title
        let seller_bh: ObjRef = TestBh::new(6, false);
        me.set_owner_id(Some("c6".into()));
        assert!(p.blockhead_is_seller(&me, &seller_bh));
        assert!(!p.blockhead_is_seller(&me, &buyer));
        me.set_owner_id(None);
        assert!(p.blockhead_is_seller(&me, &buyer));
        let stub = crate::blockhead::blockhead::testutil::stub(1, true);
        assert!(p.blockhead_is_seller(&me, &stub), "no net id and no owner");
        me.set_owner_id(Some("server".into()));
        assert!(p.blockhead_is_seller(&me, &buyer));
        assert_eq!(p.sell_title(), "SHOP (Bob)");
        p.st.borrow_mut().seller_client_name = None;
        assert_eq!(p.sell_title(), "SHOP");
        let _ = w;
    }

    #[test]
    fn using_removing_and_hooks() {
        let (w, d, pb) = world_with_block("tp_j");
        let it = InventoryItem::new(ItemType(168), 0, 0, None, None);
        let p =
            TradingPost::new(&w, &d, IntPair { x: 10, y: 10 }, None, Some(&it.borrow()), false, None, Some("c"), None)
                .unwrap();
        let me: ObjRef = p.clone();
        assert_eq!(me.class_info().name, "TradingPost");
        assert!(me.as_any().downcast_ref::<TradingPost>().is_some());
        assert!(me.occupies_normal_contents());
        assert_eq!(me.freeblock_creation_item_type(), 168);
        assert!(me.free_block_creation_save_dict(&me).is_some());
        assert_eq!(p.destroy_item_type(), 168);
        assert!(p.requires_human_interaction() && p.is_paintable());
        assert!(p.io_state().borrow().current_blockhead.is_none());
        me.update_accurate_dt_is_simulation(&me, 0.1, 0.1, false);
        me.world_contents_changed(&me, &mut vec![]);
        me.blockheads_loaded(&me);
        let other: ObjRef = TestBh::new(1, false);
        me.blockhead_unloaded(&me, &other);
        // painting
        p.paint(&me, 4);
        assert!(me.update_needs_to_be_sent());
        // remote update with a user present only touches the base state; without a sell slot key it is kept
        let mut upd = me.update_net_data_for_client(&me, None).unwrap();
        p.io.borrow_mut().current_blockhead = Some(other.clone());
        me.remote_update(&me, &upd);
        p.io.borrow_mut().current_blockhead = None;
        me.remote_update(&me, &upd);
        upd.truncate(0x30);
        let mut dict = PlistValue::mutable_dict();
        if let PlistValue::Dict(dd) = &mut dict {
            dd.map.insert("ownerID".to_string(), PlistValue::string("c"));
        }
        upd.extend_from_slice(&gz_plist_bytes(&dict));
        me.remote_update(&me, &upd);
        // support check on a real change; replicas ignore it
        tile(pb, 10, 10).set_back_wall_type_index(2);
        me.world_changed(&me, &mut vec![IntPair { x: 10, y: 9 }]);
        assert!(me.needs_removed());
        // removal in use by a foreign user is postponed, by a local one it stops them
        let q =
            TradingPost::new(&w, &d, IntPair { x: 14, y: 10 }, None, Some(&it.borrow()), false, None, Some("c"), None)
                .unwrap();
        let qm: ObjRef = q.clone();
        q.io.borrow_mut().is_in_use = true;
        q.io.borrow_mut().current_blockhead = Some(TestBh::new(2, true));
        q.remove(&qm, None);
        assert!(!qm.needs_removed());
        q.io.borrow_mut().current_blockhead = None;
        q.io.borrow_mut().remote_blockhead_in_use_unique_id = 4;
        q.remove(&qm, None);
        assert!(!qm.needs_removed());
        q.io.borrow_mut().remote_blockhead_in_use_unique_id = 0;
        let local = TestBh::new(3, false);
        q.io.borrow_mut().current_blockhead = Some(local.clone());
        q.remove(&qm, None);
        assert!(qm.needs_removed());
        assert!(local.log.borrow().iter().any(|l| l == "stop"));
        q.remove(&qm, None);
        // replicas
        let r =
            TradingPost::new(&w, &d, IntPair { x: 18, y: 10 }, None, Some(&it.borrow()), false, None, Some("c"), None)
                .unwrap();
        r.base.borrow_mut().is_net = true;
        let rm: ObjRef = r.clone();
        rm.world_changed(&rm, &mut vec![IntPair { x: 18, y: 9 }]);
        r.remove(&rm, None);
        r.paint(&rm, 2);
        me.dealloc(&me);
        assert!(p.sell_slot().is_empty());
    }

    #[test]
    fn support_check_details() {
        let (w, _d, pb) = world_with_block("tp_k");
        let pos = IntPair { x: 10, y: 10 };
        let hit = [IntPair { x: 10, y: 9 }];
        tile(pb, 10, 10).set_back_wall_type_index(2);
        assert!(support_lost(&w, pos, &hit));
        // copper wire below, a half depth tile below, a wire beside it each keep the post
        tile(pb, 10, 9).set_contents(CONTENTS_COPPER_WIRE);
        assert!(!support_lost(&w, pos, &hit));
        tile(pb, 10, 9).set_contents(0);
        for (x, y) in [(9, 10), (11, 10), (10, 11)] {
            tile(pb, x, y).set_contents(CONTENTS_COPPER_WIRE);
            assert!(!support_lost(&w, pos, &hit), "wire at {x},{y}");
            tile(pb, x, y).set_contents(0);
            tile(pb, x, y).set_type_index(6);
            assert!(!support_lost(&w, pos, &hit), "solid at {x},{y}");
            tile(pb, x, y).set_type_index(2);
        }
        assert!(support_lost(&w, pos, &hit));
        // wrap-around: a post at the far edge hears of a change on the other side of the seam
        assert!(!support_lost(&w, IntPair { x: 127, y: 10 }, &[IntPair { x: 0, y: 9 }]));
        assert!(!support_lost(&w, IntPair { x: 0, y: 10 }, &[IntPair { x: 127, y: 9 }]));
        // an own tile outside the loaded area is no support problem
        assert!(!support_lost(&w, IntPair { x: 10, y: 32 }, &[IntPair { x: 10, y: 31 }]));
        // an unloaded neighbour counts as blocked
        tile(pb, 0, 10).set_back_wall_type_index(2);
        assert!(!support_lost(&w, IntPair { x: 0, y: 10 }, &[IntPair { x: 0, y: 9 }]));
    }

    #[test]
    fn legacy_seller_keys_and_ownerless_posts() {
        let (w, d, _pb) = world_with_block("tp_l");
        install_server(&d);
        let mut sd = slot_dict(&[(5, 0)], 1, 2);
        if let Some(PlistValue::Array(a)) = sd.map.get_mut(&"sellSlot".to_string()) {
            a.items.push(PlistValue::int(4));
        }
        sd.map.insert("sellerClientID".to_string(), PlistValue::string("legacy"));
        sd.map.insert("sellerClientName".to_string(), PlistValue::string("Old"));
        let it = InventoryItem::new(ItemType(168), 0, 0, None, None);
        // no client placed it: the dictionary supplies seller id and name, foreign slot entries are ignored
        let p =
            TradingPost::new(&w, &d, IntPair { x: 10, y: 10 }, None, Some(&it.borrow()), false, Some(&sd), None, None)
                .unwrap();
        assert_eq!((p.clone() as ObjRef).owner_id().as_deref(), Some("legacy"));
        assert_eq!(p.st.borrow().seller_client_name.as_deref(), Some("Old"));
        assert_eq!(p.sell_slot().len(), 1);
        // nobody at all: no alias lookup either
        let bare =
            TradingPost::new(&w, &d, IntPair { x: 12, y: 10 }, None, Some(&it.borrow()), false, None, None, None)
                .unwrap();
        assert_eq!((bare.clone() as ObjRef).owner_id(), None);
        assert_eq!(bare.st.borrow().seller_client_name, None);
        // a stored name is kept, not looked up
        let known = TradingPost::new(
            &w,
            &d,
            IntPair { x: 14, y: 10 },
            None,
            Some(&it.borrow()),
            false,
            Some(&sd),
            Some("owner1"),
            Some("Mine"),
        )
        .unwrap();
        assert_eq!(known.st.borrow().seller_client_name.as_deref(), Some("Mine"));
        // the same shape when loading
        let me: ObjRef = known.clone();
        let loaded = TradingPost::from_save_dict(&w, &d, &me.get_save_dict(&me), None).unwrap();
        assert_eq!(loaded.st.borrow().seller_client_name.as_deref(), Some("Mine"));
    }
}
