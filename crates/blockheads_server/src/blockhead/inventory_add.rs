//! `Blockhead` item pickup: `canPickUpItemOfType:...`, `addItemToInventory:...`, the second-blockhead warp check and
//! `pickupFreeblockIfPossible:inTile:intentional:`.
//!
//! Slot 0 (clothing) is never used here: every loop runs `i = 1..=7`.

use crate::env::crystal_manager::CrystalManager;
use crate::foundation::md5::string_from_md5;
use crate::items::food::item_type_from_food_type;
use crate::items::inventory_item::{InventoryItem, ItemRef, SubSlot};
use crate::items::predicates::{
    inventory_sub_items_count_for_item_type, item_type_carries_liquids, item_type_is_liquid, item_type_is_money,
    item_type_is_stackable, item_type_is_valid_inventory_item, item_type_requires_ownership_to_remove,
    item_type_sub_items_can_be_modified_while_carried, usage_increment_per_use,
};
use crate::objects::base::BlockheadApi;
use crate::objects::handle::{ptr_eq, ObjRef};
use crate::types::enums::FoodType;
use crate::types::item_type::ItemType;
use crate::types::structs::IntPair;
use crate::world::state::TileRef;

use super::blockhead::host;
use super::blockhead::Blockhead;
use super::inventory_model::{
    inventory_host, outer_slot_at, slot_first, sub_slots_of, user_defaults_bool_for_key, user_defaults_set_yes_for_key,
};

/// Same item kind test used by (A) and (B) of `canPickUp`: equal types, or `ITEM_MONEY` offered to a stack of any
/// money type.
fn same_kind(t: ItemType, other: ItemType) -> bool {
    other == t || (t == ItemType::ITEM_MONEY && item_type_is_money(other))
}

/// The money switch shared by (A) and (B): `Some(true)` = return 1, `None` = fall through.
fn money_rule_accepts(slot_item_type: ItemType, data_a: u16, data_b: u16) -> bool {
    if slot_item_type == ItemType::ITEM_PLATINUM_COIN {
        data_a != 0
    } else if slot_item_type == ItemType::ITEM_GOLD_COIN {
        data_a != 0 || data_b > 99
    } else if slot_item_type == ItemType::ITEM_COPPER_COIN {
        data_a != 0 || data_b != 0
    } else {
        false
    }
}

/// `STACK_INSERT(arr, item)` of: ascending `dataA` insertion for durable items, plain append otherwise.
fn stack_insert(arr: &SubSlot, item: &ItemRef) {
    let t = item.borrow().item_type;
    if usage_increment_per_use(t, 2, false, false) < 1 {
        arr.borrow_mut().push(item.clone());
    } else {
        let da = item.borrow().data_a;
        let n = arr.borrow().len();
        for j in 0..n {
            let other_a = arr.borrow()[j].borrow().data_a;
            if da < other_a {
                arr.borrow_mut().insert(j, item.clone());
                return;
            }
        }
        arr.borrow_mut().push(item.clone());
    }
}

impl Blockhead {
    // ================================================================================================
    // canPickUp
    // ================================================================================================

    /// `-canPickUpItemOfType:subItems:` = `...dataA:0 dataB:0`.
    pub fn can_pick_up_item_of_type_sub_items(&self, t: ItemType, sub_items: Option<&[SubSlot]>) -> i32 {
        self.can_pick_up_item_of_type_sub_items_data_a_data_b(t, sub_items, 0, 0)
    }

    /// `-canPickUpItemOfType:subItems:dataA:dataB:`. Returns 1 (fits), -1 (full), 0 (invalid type).
    /// Read-only.
    pub fn can_pick_up_item_of_type_sub_items_data_a_data_b(
        &self,
        t: ItemType,
        sub_items: Option<&[SubSlot]>,
        data_a: u16,
        data_b: u16,
    ) -> i32 {
        if !item_type_is_valid_inventory_item(t) {
            return 0;
        }
        // `[world worldUIDragging]`: uiManager is nil on the server => NO.
        let is_liquid = item_type_is_liquid(t);
        let inv = self.inv();
        for i in 1..8i32 {
            let slot = outer_slot_at(&inv, i);
            let n = slot.borrow().len();
            if !is_liquid && n == 0 {
                return 1;
            }
            // ---- (A) stack onto the slot's existing stack (non-liquids only)
            let mut try_container_branch = true;
            if !is_liquid && n <= 98 {
                let first = slot_first(&slot).expect("n > 0");
                let (ft, fdb) = {
                    let f = first.borrow();
                    (f.item_type, f.data_b)
                };
                if same_kind(t, ft) && item_type_is_stackable(t, fdb, data_b) {
                    if t != ItemType::ITEM_MONEY {
                        return 1;
                    }
                    if money_rule_accepts(ft, data_a, data_b) {
                        return 1;
                    }
                    // no return: fall through to the common tail (C); (B) is NOT run
                    try_container_branch = false;
                }
            }
            if try_container_branch {
                // ---- (B) the slot holds a container whose sub-slots might accept the item
                let carried = slot_first(&slot)
                    .filter(|f| item_type_sub_items_can_be_modified_while_carried(f.borrow().item_type));
                if let Some(first) = carried {
                    let ft = first.borrow().item_type;
                    let subs = sub_slots_of(&first);
                    if !subs.is_empty() && item_type_carries_liquids(ft) == is_liquid {
                        for sub in subs.iter() {
                            let sn = sub.borrow().len();
                            if sn == 0 {
                                return 1;
                            }
                            if sn < 99 {
                                let sf = slot_first(sub).expect("sn > 0");
                                let (st, sdb) = {
                                    let b = sf.borrow();
                                    (b.item_type, b.data_b)
                                };
                                if same_kind(t, st) && item_type_is_stackable(t, sdb, data_b) {
                                    if t != ItemType::ITEM_MONEY {
                                        return 1;
                                    }
                                    if money_rule_accepts(st, data_a, data_b) {
                                        return 1;
                                    }
                                }
                            }
                        }
                    }
                }
            }
            // ---- (C) common tail: the picked item is itself a container
            if item_type_sub_items_can_be_modified_while_carried(t) && n != 0 {
                let first = slot_first(&slot).expect("n > 0");
                let (ft, fdb) = {
                    let f = first.borrow();
                    (f.item_type, f.data_b)
                };
                if inventory_sub_items_count_for_item_type(ft) == 0
                    && item_type_carries_liquids(t) == item_type_is_liquid(ft)
                {
                    let subs = match sub_items {
                        None => return 1,
                        Some(s) => s,
                    };
                    if subs.is_empty() {
                        return 1;
                    }
                    for sub in subs.iter() {
                        if sub.borrow().is_empty() {
                            return 1;
                        }
                    }
                    // every sub-slot holds something here: an empty one returned above
                    for sub in subs.iter() {
                        let sn = sub.borrow().len();
                        let sf = slot_first(sub).expect("sn > 0");
                        let (st, sdb) = {
                            let b = sf.borrow();
                            (b.item_type, b.data_b)
                        };
                        if st == ft {
                            if ft == ItemType::ITEM_PAINT && sdb != fdb {
                                continue;
                            }
                            if (sn as u64) + (n as u64) < 100 {
                                return 1;
                            }
                        }
                    }
                }
            }
        }
        -1
    }

    // ================================================================================================
    // warp check
    // ================================================================================================

    /// `-checkIfCanWarpInSecondBlockheadAfterItemAdded:dataB:`.
    /// block (NOT always NO: reachable once a server-owned blockhead credits crystals). The side effects on success
    /// are the in-process `NSUserDefaults` flag and `showBlockheadAvailablePrompt:forBlockhead:`, whose only effect is
    /// an unread World ivar (+0xDC4) and an empty GameController method: omitted.
    pub fn check_if_can_warp_in_second_blockhead_after_item_added_data_b(&self, t: ItemType, data_b: i32) -> bool {
        let world = match self.world_rc() {
            Some(w) => w,
            None => return false,
        };
        if world.expert_mode() {
            return false;
        }
        let food_type = crate::items::food::food_type_for_item_type(t, data_b);
        if food_type == FoodType::FOOD_NONE && t != ItemType::ITEM_TIME_CRYSTAL {
            return false;
        }
        let blockhead_count = self.dw().map_or(0, |d| d.blockheads().len()) as i32;
        let cr = world.custom_rules();
        let cap: i32 = if cr.custom_rules_enabled == 0 {
            5
        } else if cr.max_blockheads < 5 {
            cr.max_blockheads as i32
        } else {
            5
        };
        if cap <= blockhead_count {
            return false;
        }
        let required_food =
            world.distance_ordered_food_types.borrow().get(blockhead_count as usize).copied().unwrap_or(0);
        if required_food != food_type.0 as i32 && t != ItemType::ITEM_TIME_CRYSTAL {
            return false;
        }
        let key = format!(
            "hasDisplayedBlockheadPromt_{}_{}",
            blockhead_count,
            world.save_id().unwrap_or_else(|| "(null)".to_string())
        );
        if user_defaults_bool_for_key(&key) {
            return false;
        }
        let food_item = item_type_from_food_type(FoodType(required_food as u32));
        if self.count_of_inventory_items_of_type_include_actions(food_item, false) < 5 {
            return false;
        }
        let required_crystal_count = match blockhead_count {
            2 => 100,
            3 | 4 => 200,
            _ => 50,
        };
        let cm = CrystalManager::instance();
        let mut ccount = cm.amount();
        let s = CrystalManager::instance().amount_string();
        let expected = string_from_md5(&format!("7acfe93afc08{}c65ae2c54ecaf07f", ccount));
        // anti-tamper: the crystal count is trusted only if its stored MD5 string matches
        let trusted = match &s {
            Some(s) => *s == expected,
            None => false,
        };
        if !trusted {
            ccount = 0;
        }
        if ccount < required_crystal_count {
            return false;
        }
        user_defaults_set_yes_for_key(&key);
        // [world showBlockheadAvailablePrompt:(blockheadCount + 1) forBlockhead:self]: no observable effect.
        true
    }

    // ================================================================================================
    // addItemToInventory
    // ================================================================================================

    /// `-addItemToInventory:` = `flash:NO`.
    pub fn add_item_to_inventory(&self, item: &ItemRef) -> i32 {
        self.add_item_to_inventory_flash(item, false)
    }

    /// `-addItemToInventory:flash:` = `disableWarpCheck:NO`.
    pub fn add_item_to_inventory_flash(&self, item: &ItemRef, flash: bool) -> i32 {
        self.add_item_to_inventory_flash_disable_warp_check(item, flash, false)
    }

    /// `-addItemToInventory:flash:disableWarpCheck:` = `forceSlotIndex:-1`.
    pub fn add_item_to_inventory_flash_disable_warp_check(
        &self,
        item: &ItemRef,
        flash: bool,
        disable_warp_check: bool,
    ) -> i32 {
        self.add_item_to_inventory_flash_disable_warp_check_force_slot_index(item, flash, disable_warp_check, -1)
    }

    /// The `FINISH(i, subIndex)` step of: set the changed flag (slot flag only for `sub == -1`), skip the
    /// client flash, run the warp check unless disabled, return the slot index.
    fn add_finish(&self, i: i32, sub: i32, item: &ItemRef, disable_warp_check: bool) -> i32 {
        {
            let mut f = self.bh.borrow_mut();
            if sub == -1 {
                f.inventory_changed_this_frame_slots[i as usize] = true;
            } else if sub < 4 {
                f.sub_inventory_changed_this_frame_slots[i as usize][sub as usize] = true;
            }
        }
        // flash: `[[world uiManager] flashInventoryAtIndex:...color:0]` is a nil-receiver no-op on the server.
        if !disable_warp_check {
            let (t, db) = {
                let b = item.borrow();
                (b.item_type, b.data_b)
            };
            let _ = self.check_if_can_warp_in_second_blockhead_after_item_added_data_b(t, db as i32);
        }
        i
    }

    /// `-addItemToInventory:flash:disableWarpCheck:forceSlotIndex:`. Returns the slot `1..=7` where the
    /// item (or the container that took it) landed, or -1. Side effects (found list) happen
    /// even when placement fails; phases 3 and 4 ignore `force_slot_index`.
    pub fn add_item_to_inventory_flash_disable_warp_check_force_slot_index(
        &self,
        item: &ItemRef,
        flash: bool,
        disable_warp_check: bool,
        force_slot_index: i32,
    ) -> i32 {
        let _ = flash;
        let item_type = item.borrow().item_type;
        if !item_type_is_valid_inventory_item(item_type) {
            return -1;
        }
        let is_liquid = item_type_is_liquid(item_type);

        // achievements for TITANIUM_ORE / PLATINUM_ORE: `reportAchievementWithIdentifier:` is empty => omitted.

        if !self.is_net() {
            if let Some(w) = self.world_rc() {
                w.add_item_to_found_list(&item.borrow());
            }
        }

        let inv = self.inv();
        let item_data_b = item.borrow().data_b;

        // ---- Phase 1: merge into an existing top-level stack (only for stackable item types)
        if item_type_is_stackable(item_type, 0, 0) {
            for i in 1..8i32 {
                if force_slot_index != -1 && force_slot_index != i {
                    continue;
                }
                let slot = outer_slot_at(&inv, i);
                let hit = {
                    let s = slot.borrow();
                    if !s.is_empty() && s.len() < 99 {
                        let f = s[0].borrow();
                        f.item_type == item_type && item_type_is_stackable(item_type, f.data_b, item_data_b)
                    } else {
                        false
                    }
                };
                if hit {
                    stack_insert(&slot, item);
                    return self.add_finish(i, -1, item, disable_warp_check);
                }
            }

            // ---- Phase 1b: merge into a stack inside a carried container (single-object slots only)
            for i in 1..8i32 {
                if force_slot_index != -1 && force_slot_index != i {
                    continue;
                }
                let slot = outer_slot_at(&inv, i);
                if slot.borrow().len() == 1 {
                    let first = slot_first(&slot).expect("len 1");
                    if item_type_sub_items_can_be_modified_while_carried(first.borrow().item_type) {
                        for (si, sub) in sub_slots_of(&first).into_iter().enumerate() {
                            let ok = {
                                let s = sub.borrow();
                                if !s.is_empty() {
                                    let f = s[0].borrow();
                                    item_type_is_stackable(item_type, f.data_b, item_data_b)
                                        && f.item_type == item_type
                                        && s.len() < 99
                                } else {
                                    false
                                }
                            };
                            if ok {
                                stack_insert(&sub, item);
                                return self.add_finish(i, si as i32, item, disable_warp_check);
                            }
                        }
                    }
                }
            }
        }

        // ---- Phase 2: first empty top-level slot (never for liquids; `itemTypeIsLiquid` is constant NO, so the
        // original's `!isLiquid` guard around this loop always held)
        for i in 1..8i32 {
            if force_slot_index != -1 && force_slot_index != i {
                continue;
            }
            let slot = outer_slot_at(&inv, i);
            if slot.borrow().is_empty() {
                slot.borrow_mut().push(item.clone());
                return self.add_finish(i, -1, item, disable_warp_check);
            }
        }

        // ---- Phase 3: the item is a container; pour a loose stack from a slot into it and take that slot
        if item_type_sub_items_can_be_modified_while_carried(item_type) {
            for i in 1..8i32 {
                let slot = outer_slot_at(&inv, i);
                if slot.borrow().is_empty() {
                    continue;
                }
                let first = slot_first(&slot).expect("non-empty");
                let (ft, fdb) = {
                    let f = first.borrow();
                    (f.item_type, f.data_b)
                };
                if inventory_sub_items_count_for_item_type(ft) == 0
                    && item_type_carries_liquids(item_type) == item_type_is_liquid(ft)
                {
                    let mut target: Option<SubSlot> = None;
                    let item_subs = sub_slots_of(item);
                    // 3a. an empty sub-slot of the new container
                    for sub in item_subs.iter() {
                        if sub.borrow().is_empty() {
                            target = Some(sub.clone());
                            break;
                        }
                    }
                    // 3b. otherwise a sub-slot that already holds the same item type and has room
                    if target.is_none() {
                        // (no sub-slot is empty here, 3a would have taken it)
                        for sub in item_subs.iter() {
                            let sn = sub.borrow().len();
                            let sf = slot_first(sub).expect("sn > 0");
                            let (st, sdb) = {
                                let b = sf.borrow();
                                (b.item_type, b.data_b)
                            };
                            if st == ft {
                                if ft == ItemType::ITEM_PAINT && sdb != fdb {
                                    continue;
                                }
                                if (sn as u64) + (slot.borrow().len() as u64) < 100 {
                                    target = Some(sub.clone());
                                    break;
                                }
                            }
                        }
                    }
                    if let Some(target) = target {
                        let moved: Vec<ItemRef> = slot.borrow().clone();
                        target.borrow_mut().extend(moved);
                        slot.borrow_mut().clear();
                        slot.borrow_mut().push(item.clone());
                        return self.add_finish(i, -1, item, disable_warp_check);
                    }
                }
            }
        }

        // ---- Phase 4: put the item into an empty sub-slot of a carried container
        for i in 1..8i32 {
            let slot = outer_slot_at(&inv, i);
            if slot.borrow().len() == 1 {
                let first = slot_first(&slot).expect("len 1");
                let ft = first.borrow().item_type;
                if item_type_sub_items_can_be_modified_while_carried(ft) && item_type_carries_liquids(ft) == is_liquid {
                    for (si, sub) in sub_slots_of(&first).into_iter().enumerate() {
                        if sub.borrow().is_empty() {
                            sub.borrow_mut().push(item.clone());
                            return self.add_finish(i, si as i32, item, disable_warp_check);
                        }
                    }
                }
            }
        }
        -1
    }

    // ================================================================================================
    // pickupFreeblockIfPossible
    // ================================================================================================

    /// `-pickupFreeblockIfPossible:inTile:intentional:`. Returns true if picked up.
    /// `FreeBlock` accessors come through `InventoryHost::free_block_data`. `zIndex` is overwritten with
    /// `world.isAdmin` (replicated). `unconfirmedPickups` is nil on the server (blockhead_core), so its writes are
    /// skipped unless the dictionary exists; the money variant (`NSNumber`) cannot be stored in the `ItemRef` map.
    pub fn pickup_freeblock_if_possible_in_tile_intentional(
        &self,
        fb: &ObjRef,
        tile: Option<&TileRef>,
        intentional: bool,
    ) -> bool {
        let ih = inventory_host();
        let fbd = match ih.free_block_data(fb) {
            Some(d) => d,
            None => return false,
        };
        let world = match self.world_rc() {
            Some(w) => w,
            None => return false,
        };
        let me = self.me();
        // [world worldUIDragging]: NO on the server.
        {
            let f = self.bh.borrow();
            if f.state.regenerating != 0 || f.state.on_trade_mission != 0 {
                return false;
            }
        }
        let pri = fbd.priority_blockhead.clone();
        let pri_is_self = pri.as_ref().is_some_and(|p| ptr_eq(p, &me));
        if !intentional {
            let ignoring = self.bh.borrow().ignoring_freeblocks_due_to_drop;
            if (ignoring || self.meditating()) && !pri_is_self {
                return false;
            }
        }
        if pri.is_some() && !pri_is_self {
            return false;
        }
        // forget it from the expected-craft list (first match only)
        {
            let mut f = self.bh.borrow_mut();
            if !f.expected_craft_items.is_empty() {
                let t = fbd.item_type.0 as u16;
                if let Some(p) = f.expected_craft_items.iter().position(|x| *x == t) {
                    f.expected_craft_items.remove(p);
                }
            }
        }
        let ty = fbd.item_type;
        let admin = world.is_admin();
        self.bh.borrow_mut().z_index = admin as u8 as i32;

        // ---- ownership check
        if item_type_requires_ownership_to_remove(ty) {
            let ctl = self.is_client_blockhead_being_controlled_by_server();
            let admin_override = admin && !ctl && (intentional || pri_is_self);
            if !admin_override {
                let local_id = BlockheadApi::local_net_id(self);
                let save_dict = fbd.dynamic_object_save_dict.as_ref();
                let get = |k: &str| -> Option<String> {
                    save_dict.and_then(|d| d.object_for_key(k)).and_then(|v| v.as_str()).map(|s| s.to_string())
                };
                let mut owner = get("ownerID");
                if owner.is_none() {
                    let tv = ty.0;
                    if tv.wrapping_sub(164) < 2 {
                        owner = get("ironPlaceClientID");
                    } else if ty == ItemType::ITEM_TRADING_POST {
                        owner = get("sellerClientID");
                    } else if ty == ItemType::ITEM_DISPLAY_CABINET || ty == ItemType::ITEM_SAFE {
                        owner = get("safeClientID");
                    }
                }
                let mut blocked = false;
                if let Some(owner) = &owner {
                    blocked = local_id.as_deref() != Some(owner.as_str());
                    if blocked
                        && !ih.dynamic_world_is_client()
                        && (owner == "server" || local_id.as_deref() == Some("server"))
                        && !ctl
                    {
                        blocked = false;
                    }
                }
                if blocked {
                    if pri_is_self {
                        ih.free_block_priority_blockhead_cannot_pickup(fb);
                    }
                    return false;
                }
            }
        }

        // ---- pickup
        if self.can_pick_up_item_of_type_sub_items_data_a_data_b(ty, fbd.sub_items.as_deref(), fbd.data_a, fbd.data_b)
            == 1
            && !fb.needs_removed()
        {
            if world.is_simulating() || self.is_client_blockhead_being_controlled_by_server() {
                ih.add_simulation_event_got(&me, fb);
            }
            // (the original also flagged a picked-up time crystal for removal here; `canPickUp...` above refuses
            // `ITEM_TIME_CRYSTAL` as no valid inventory item, so that could never run)
            let unique_id = fb.unique_id();
            if ty == ItemType::ITEM_MONEY {
                let p = fbd.data_a as i32;
                let r = fbd.data_b as i32;
                // 1. platinum
                let mut added_plat = 0;
                for _ in 0..p {
                    if self.can_pick_up_item_of_type_sub_items_data_a_data_b(ItemType::ITEM_PLATINUM_COIN, None, 0, 0)
                        != 1
                    {
                        break;
                    }
                    self.add_item_to_inventory_flash(
                        &InventoryItem::new(ItemType::ITEM_PLATINUM_COIN, 0, 0, None, None),
                        true,
                    );
                    added_plat += 1;
                }
                // 2. gold
                let gold_total = (p - added_plat) * 100 + r / 100;
                let mut added_gold = 0;
                for _ in 0..gold_total {
                    if self.can_pick_up_item_of_type_sub_items_data_a_data_b(ItemType::ITEM_GOLD_COIN, None, 0, 0) != 1
                    {
                        break;
                    }
                    self.add_item_to_inventory_flash(
                        &InventoryItem::new(ItemType::ITEM_GOLD_COIN, 0, 0, None, None),
                        true,
                    );
                    added_gold += 1;
                }
                // 3. copper
                let copper_total = (gold_total - added_gold) * 100 + r % 100;
                let mut added_copper = 0;
                for _ in 0..copper_total {
                    if self.can_pick_up_item_of_type_sub_items_data_a_data_b(ItemType::ITEM_COPPER_COIN, None, 0, 0)
                        != 1
                    {
                        break;
                    }
                    self.add_item_to_inventory_flash(
                        &InventoryItem::new(ItemType::ITEM_COPPER_COIN, 0, 0, None, None),
                        true,
                    );
                    added_copper += 1;
                }
                let leftover = copper_total - added_copper;
                if leftover > 0 {
                    let pos = self.pos();
                    host().create_free_block_at_position_of_type_data_a_data_b_sub_items_dynamic_object_save_dict_hovers_play_sound_priority_blockhead(
                        IntPair { x: pos.x, y: pos.y + 1 },
                        ItemType::ITEM_MONEY,
                        (leftover / 10000) as i16 as u16,
                        (leftover % 10000) as i16 as u16,
                        None,
                        None,
                        false,
                        0,
                        None,
                    );
                }
                fb.set_needs_removed(fb, true);
                // `if unconfirmedPickups[key] == nil`: a nil dictionary (always on the server) yields nil => true.
                let mut f = self.bh.borrow_mut();
                let present = f.unconfirmed_pickups.as_ref().is_some_and(|m| m.contains_key(&unique_id));
                if !present {
                    if let Some(set) = f.this_frame_pickup_requests.as_mut() {
                        set.add_index(unique_id);
                    }
                    // Note: unconfirmedPickups[key] = NSNumber(P*10000 + R - leftover) cannot be stored in
                    // blockhead_core's `BTreeMap<u64, ItemRef>`; the dictionary is nil on the server (no effect).
                }
            } else {
                // ITEM_DIAMOND hovering in a diamond tree: "grp.diamond_tree" (empty World method), tile unused.
                let _ = tile;
                let sub_vecs = fbd.sub_items.as_deref().map(super::inventory_model::slots_to_vecs);
                let item = InventoryItem::new(
                    ty,
                    fbd.data_a,
                    fbd.data_b,
                    sub_vecs.as_deref(),
                    fbd.dynamic_object_save_dict.clone(),
                );
                self.add_item_to_inventory_flash(&item, true);
                fb.set_needs_removed(fb, true);
                let mut f = self.bh.borrow_mut();
                // `if unconfirmedPickups[key] == nil`: a nil dictionary yields nil => the branch is taken.
                let present = f.unconfirmed_pickups.as_ref().is_some_and(|m| m.contains_key(&unique_id));
                if !present {
                    if let Some(set) = f.this_frame_pickup_requests.as_mut() {
                        set.add_index(unique_id);
                    }
                    if let Some(map) = f.unconfirmed_pickups.as_mut() {
                        map.insert(unique_id, item.clone());
                    }
                }
            }
            return true;
        }
        // fall-through (cannot pick up, or the free block was already removed)
        if pri_is_self {
            ih.free_block_priority_blockhead_cannot_pickup(fb);
        }
        false
    }
}
