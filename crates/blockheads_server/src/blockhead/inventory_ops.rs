//! `Blockhead` inventory queries, removals, cash, AI helper queries and item wear.
//!
//! `## Synthesis resolutions`;. Float arithmetic is f32
//! with the order `mulss/divss/addss` then `cvttss2si`; slot 0 (clothing) is excluded from the
//! queries (`i = 1..=7`) and used by the clothing wear functions.

use crate::fmath::cconv::f32_to_i32;
use crate::items::food::food_type_for_item_type;
use crate::items::inventory_item::{InventoryItem, ItemRef, SubSlot};
use crate::items::predicates::{
    armour_item_damage_usage_rate, clothing_item_pasively_used_rate, item_type_is_pigment, item_type_is_sowable,
    usage_increment_per_use,
};
use crate::plist::value::PlistValue;
use crate::types::enums::FoodType;
use crate::types::item_type::ItemType;
use crate::types::structs::IntPair;
use crate::world::helpers::{tile_is_bush_or_scrub, tile_is_plant, tile_is_rock, tile_is_tree_trunk};
use crate::world::state::TileRef;

use super::blockhead::host;
use super::blockhead::Blockhead;
use super::inventory_model::{
    inventory_host, outer_slot_at, slot_first, slot_last, slot_remove_object, slots_to_vecs, sub_slot_at, sub_slots_of,
};

const IT_PIGMENT_GENERIC: ItemType = ItemType::ITEM_PIGMENT_GENERIC;

/// `matches(t)` of `countOfInventoryItemsWithSpecificDataB...`: equal, or generic pigment against any pigment.
fn type_matches(want: ItemType, have: ItemType) -> bool {
    have == want || (want == IT_PIGMENT_GENERIC && item_type_is_pigment(have))
}

impl Blockhead {
    // ================================================================================================
    // shared helpers
    // ================================================================================================

    /// `customToolDecayLevel()`: rule 2 when custom rules are disabled, else the sign-extended int8 `toolDecay`.
    fn custom_tool_decay_level(&self) -> i32 {
        match self.world_rc() {
            None => 2,
            Some(w) => {
                let r = w.custom_rules();
                if r.custom_rules_enabled == 0 {
                    2
                } else {
                    r.tool_decay as i32
                }
            }
        }
    }

    /// `customClothingDecayLevel()`: as above with `clothingDecay`.
    fn custom_clothing_decay_level(&self) -> i32 {
        match self.world_rc() {
            None => 2,
            Some(w) => {
                let r = w.custom_rules();
                if r.custom_rules_enabled == 0 {
                    2
                } else {
                    r.clothing_decay as i32
                }
            }
        }
    }

    /// `customPoleItems()`: 0 when custom rules are disabled, else `poleItems`.
    fn custom_pole_items(&self) -> i32 {
        match self.world_rc() {
            None => 0,
            Some(w) => {
                let r = w.custom_rules();
                if r.custom_rules_enabled == 0 {
                    0
                } else {
                    r.pole_items as i32
                }
            }
        }
    }

    /// `removeIfUsedUp(item, itemSlot, subIndex)` of the clothing functions.
    fn clothing_remove_if_used_up(&self, item: &ItemRef, item_slot: &SubSlot, sub_index: i32) {
        if sub_index < 4 {
            self.bh.borrow_mut().sub_inventory_changed_this_frame_slots[0][sub_index as usize] = true;
        }
        self.item_will_be_removed_from_inventory(item);
        slot_remove_object(item_slot, item);
        // flash (uiManager nil) and tear.wav (client audio): no effect
    }

    /// "Usage-changed flag rule" of the clothing functions.
    fn mark_clothing_usage_changed(&self, sub_index: i32) {
        let mut f = self.bh.borrow_mut();
        if sub_index == -1 || sub_index > 3 {
            f.inventory_usage_changed_this_frame_slots[0] = true;
        } else {
            f.sub_usage_slot_set(sub_index);
        }
    }

    /// The four clothing sub-slots (`inventoryItems[0][0].subItems`), or `None` when slot 0 is empty / missing.
    /// Guards of `incrementUsageOfClothing`: `inventoryItems.count == 0` and `clothingSlot.count == 0` return.
    fn clothing_sub_slots(&self) -> Option<Vec<SubSlot>> {
        let inv = self.inv();
        if inv.is_empty() {
            return None;
        }
        let cs = inv[0].clone();
        let first = slot_first(&cs)?;
        Some(sub_slots_of(&first))
    }

    // ================================================================================================
    // 5.2 / 5.3 queries
    // ================================================================================================

    /// `-inventoryItems`: the live array of slots (shared handles; mutation goes through the slots).
    pub fn inventory_items(&self) -> Vec<SubSlot> {
        self.inv()
    }

    /// `-countOfInventoryItemsWithSpecificDataBOfType:dataB:includeActions:`. `data_b == -1` = any.
    pub fn count_of_inventory_items_with_specific_data_b_of_type_data_b_include_actions(
        &self,
        t: ItemType,
        data_b: i32,
        include_actions: bool,
    ) -> i32 {
        let mut count: i32 = 0;
        let inv = self.inv();
        for i in 1..8i32 {
            let slot = outer_slot_at(&inv, i);
            let n = slot.borrow().len();
            if n == 0 {
                continue;
            }
            let first = slot_first(&slot).expect("n > 0");
            let (ft, fdb) = {
                let f = first.borrow();
                (f.item_type, f.data_b)
            };
            if type_matches(t, ft) && (data_b == -1 || fdb as i32 == data_b) {
                count = count.wrapping_add(n as i32);
            }
            if n == 1 {
                for sub in sub_slots_of(&first) {
                    let sn = sub.borrow().len();
                    if sn != 0 {
                        let f = slot_first(&sub).expect("sn > 0");
                        let (st, sdb) = {
                            let b = f.borrow();
                            (b.item_type, b.data_b)
                        };
                        if type_matches(t, st) && (data_b == -1 || sdb as i32 == data_b) {
                            count = count.wrapping_add(sn as i32);
                        }
                    }
                }
            }
        }
        if include_actions {
            let h = host();
            let ih = inventory_host();
            let queue = self.bh.borrow().action_queue.clone();
            let me = self.me();
            for action in queue.iter() {
                let key = format!("{}", t.0 as i32);
                let mut change = h.action_inventory_change_int_for_key(action, &key).unwrap_or(0);
                let mut add = true;
                let (interacting, waiting_fill, wb) = {
                    let f = self.bh.borrow();
                    (f.state.interacting != 0, f.waiting_for_fill_response, f.interaction_workbench.clone())
                };
                if h.action_in_progress(action) && (interacting || waiting_fill) {
                    let mut crafting_here = false;
                    if ih.action_goal_interaction(action) == 4 {
                        if let Some(wb) = &wb {
                            let cur = h.workbench_current_blockhead(wb);
                            if cur.as_ref().map_or(false, |c| crate::objects::handle::ptr_eq(c, &me))
                                && ih.workbench_currently_crafting_item_type(wb) == t.0 as i32
                            {
                                change = ih.workbench_total_items_left_to_craft(wb);
                                crafting_here = true;
                            }
                        }
                    }
                    if !crafting_here {
                        add = false;
                    }
                }
                if add || change > 0 {
                    count = count.wrapping_add(change);
                }
            }
        }
        count
    }

    /// `-countOfInventoryItemsOfType:includeActions:` = `...dataB:-1...`.
    pub fn count_of_inventory_items_of_type_include_actions(&self, t: ItemType, include_actions: bool) -> i32 {
        self.count_of_inventory_items_with_specific_data_b_of_type_data_b_include_actions(t, -1, include_actions)
    }

    /// `-usageMultiplierForFirstItemOfType:`. Returns `float32` (the contract text says `i32`; the spec
    /// returns a float, see the: `1.0f - (float)dataA / 16384.0f` of the top item of the smallest stack.
    pub fn usage_multiplier_for_first_item_of_type(&self, t: ItemType) -> f32 {
        let mut lowest: usize = 100;
        let mut best: Option<SubSlot> = None;
        let inv = self.inv();
        for i in (1..8i32).rev() {
            let slot = outer_slot_at(&inv, i);
            let n = slot.borrow().len();
            if n != 0 && slot_first(&slot).expect("n > 0").borrow().item_type == t {
                if n < lowest {
                    best = Some(slot.clone());
                    lowest = n;
                }
            } else if n != 0 {
                let first = slot_first(&slot).expect("n > 0");
                for sub in sub_slots_of(&first) {
                    let sn = sub.borrow().len();
                    if sn != 0 && slot_first(&sub).expect("sn > 0").borrow().item_type == t && sn < lowest {
                        best = Some(sub.clone());
                        lowest = sn;
                    }
                }
            }
        }
        match best {
            Some(b) if lowest != 100 => {
                let da = slot_last(&b).map_or(0, |x| x.borrow().data_a);
                1.0f32 - (da as f32) / 16384.0f32
            }
            _ => 1.0f32,
        }
    }

    /// `-inventoryLocationOfFirstInstanceOfItemType:`: `(slot, -1)` / `(slot, sub)` / `(-1, -1)`.
    pub fn inventory_location_of_first_instance_of_item_type(&self, t: ItemType) -> IntPair {
        let inv = self.inv();
        for i in 1..8i32 {
            let slot = outer_slot_at(&inv, i);
            let n = slot.borrow().len();
            if n == 0 {
                continue;
            }
            let first = slot_first(&slot).expect("n > 0");
            let ft = first.borrow().item_type;
            if ft == t || (t == IT_PIGMENT_GENERIC && item_type_is_pigment(ft)) {
                return IntPair { x: i, y: -1 };
            }
            if n == 1 {
                for (j, sub) in sub_slots_of(&first).into_iter().enumerate() {
                    if !sub.borrow().is_empty() {
                        let st = slot_first(&sub).expect("non-empty").borrow().item_type;
                        if st == t || (t == IT_PIGMENT_GENERIC && item_type_is_pigment(st)) {
                            return IntPair { x: i, y: j as i32 };
                        }
                    }
                }
            }
        }
        IntPair { x: -1, y: -1 }
    }

    /// `-totalCash`: copper-equivalent total, `int32` wrapping.
    pub fn total_cash(&self) -> i32 {
        fn coin_value(t: ItemType, n: i32) -> i32 {
            if t == ItemType::ITEM_COPPER_COIN {
                n
            } else if t == ItemType::ITEM_GOLD_COIN {
                n.wrapping_mul(100)
            } else if t == ItemType::ITEM_PLATINUM_COIN {
                n.wrapping_mul(10000)
            } else {
                0
            }
        }
        let mut count: i32 = 0;
        let inv = self.inv();
        for i in 1..8i32 {
            let slot = outer_slot_at(&inv, i);
            let n = slot.borrow().len();
            if n == 0 {
                continue;
            }
            let first = slot_first(&slot).expect("n > 0");
            let t = first.borrow().item_type;
            count = count.wrapping_add(coin_value(t, n as i32));
            if n == 1 {
                for sub in sub_slots_of(&first) {
                    let sn = sub.borrow().len();
                    if sn != 0 {
                        let st = slot_first(&sub).expect("sn > 0").borrow().item_type;
                        count = count.wrapping_add(coin_value(st, sn as i32));
                    }
                }
            }
        }
        count
    }

    // ================================================================================================
    // 5.4 subtractCash
    // ================================================================================================

    /// `-subtractCash:`. Removes coins to pay `amount` copper (never checks the funds); any overpayment is
    /// refunded as an `ITEM_MONEY` pile at the blockhead's feet (`playSound 0`, `priorityBlockhead = self`). The
    /// original returns void; this returns `true` so that contract-shaped callers compile.
    pub fn subtract_cash(&self, amount: i32) -> bool {
        let mut remaining: i32 = amount;
        let inv = self.inv();
        // phase(coinType, step, threshold)
        let phase = |coin: ItemType, step: i32, threshold: i32, remaining: &mut i32| {
            let mut i: i32 = 7;
            while 0 < i && *remaining > threshold {
                let slot = outer_slot_at(&inv, i);
                if !slot.borrow().is_empty() {
                    let first = slot_first(&slot).expect("non-empty");
                    if first.borrow().item_type == coin {
                        while *remaining > threshold && !slot.borrow().is_empty() {
                            let it = slot_last(&slot).expect("non-empty");
                            self.item_will_be_removed_from_inventory(&it);
                            slot.borrow_mut().pop();
                            *remaining = remaining.wrapping_sub(step);
                            self.bh.borrow_mut().inventory_changed_this_frame_slots[i as usize] = true;
                        }
                    } else {
                        // no `count == 1` requirement here
                        for (si, sub) in sub_slots_of(&first).into_iter().enumerate() {
                            let ok = !sub.borrow().is_empty()
                                && slot_first(&sub).expect("non-empty").borrow().item_type == coin;
                            if ok {
                                while *remaining > threshold && !sub.borrow().is_empty() {
                                    let it = slot_last(&sub).expect("non-empty");
                                    self.item_will_be_removed_from_inventory(&it);
                                    sub.borrow_mut().pop();
                                    *remaining = remaining.wrapping_sub(step);
                                    if si < 4 {
                                        self.bh.borrow_mut().sub_inventory_changed_this_frame_slots[i as usize][si] =
                                            true;
                                    }
                                }
                            }
                        }
                    }
                }
                i -= 1;
            }
        };
        phase(ItemType::ITEM_PLATINUM_COIN, 10000, 9999, &mut remaining);
        phase(ItemType::ITEM_GOLD_COIN, 100, 99, &mut remaining);
        phase(ItemType::ITEM_COPPER_COIN, 1, 0, &mut remaining);
        phase(ItemType::ITEM_GOLD_COIN, 100, 0, &mut remaining);
        phase(ItemType::ITEM_PLATINUM_COIN, 10000, 0, &mut remaining);
        if remaining < 0 {
            let refund = remaining.wrapping_neg();
            let me = self.me();
            host().create_free_block_at_position_of_type_data_a_data_b_sub_items_dynamic_object_save_dict_hovers_play_sound_priority_blockhead(
                self.pos(),
                ItemType::ITEM_MONEY,
                (refund / 10000) as i16 as u16,
                (refund % 10000) as i16 as u16,
                None,
                None,
                false,
                0,
                Some(&me),
            );
        }
        true
    }

    // ================================================================================================
    // 5.5 - 5.8 AI helper queries
    // ================================================================================================

    /// `-itemIndexWithGoodInteractionTypeForTile:`: `(slot, sub)` of the first tool suited to dig the tile,
    /// or `(-1, -1)`. Tile type values and tool tables are quoted from.
    pub fn item_index_with_good_interaction_type_for_tile(&self, tile: &TileRef) -> IntPair {
        let tt = tile.type_index() as u32;
        let tr = Some(*tile);
        // DIRT 6, DIRT_GRASS 27, DIRT_GRASS_FROZEN 28, SAND 7, SAND_BEACH 8, GRAVEL 70, BLACK_SAND 58, COMPOST 48,
        // COMPOST_GRASS 49, COMPOST_GRASS_FROZEN 50
        let spades = matches!(tt, 6 | 27 | 28 | 7 | 8 | 70 | 58 | 48 | 49 | 50) || tile_is_plant(tr);
        let machetes = tile_is_bush_or_scrub(tr);
        // GLASS 24, BLACK_GLASS 59, ICE 4
        let pickaxes = tile_is_rock(tr) || matches!(tt, 24 | 59 | 4);
        // WOOD 9, GLASS 24, BLACK_GLASS 59, ICE 4, WOODEN_PLATFORM 32
        let axes = tile_is_tree_trunk(tr) || matches!(tt, 9 | 24 | 59 | 4 | 32);
        const SPADE: [u32; 4] = [16, 40, 64, 89];
        const MACHETE: [u32; 3] = [25, 49, 67];
        const PICKAXE: [u32; 12] = [8, 34, 43, 66, 90, 93, 285, 94, 95, 96, 97, 98];
        const AXE: [u32; 3] = [6, 33, 70];
        let good = |t: u32| -> bool {
            (spades && SPADE.contains(&t))
                || (machetes && MACHETE.contains(&t))
                || (pickaxes && PICKAXE.contains(&t))
                || (axes && AXE.contains(&t))
        };
        let inv = self.inv();
        for i in 1..8i32 {
            let slot = outer_slot_at(&inv, i);
            let n = slot.borrow().len();
            if n == 0 {
                continue;
            }
            let first = slot_first(&slot).expect("n > 0");
            if good(first.borrow().item_type.0) {
                return IntPair { x: i, y: -1 };
            }
            if n == 1 {
                for (j, sub) in sub_slots_of(&first).into_iter().enumerate() {
                    if !sub.borrow().is_empty() && good(slot_first(&sub).expect("non-empty").borrow().item_type.0) {
                        return IntPair { x: i, y: j as i32 };
                    }
                }
            }
        }
        IntPair { x: -1, y: -1 }
    }

    /// Shared scan of `sowableItemForAIItemType` / `...Index`: biggest sowable stack of more than 10 items.
    fn sowable_scan(&self) -> (ItemType, IntPair) {
        let mut max_count: usize = 0;
        let mut best_type = ItemType::ITEM_NONE;
        let mut best_idx = IntPair { x: -1, y: -1 };
        let inv = self.inv();
        for i in 1..8i32 {
            let slot = outer_slot_at(&inv, i);
            let n = slot.borrow().len();
            if n == 0 {
                continue;
            }
            let first = slot_first(&slot).expect("n > 0");
            let t = first.borrow().item_type;
            if item_type_is_sowable(t) && n > 10 && n > max_count {
                max_count = n;
                best_type = t;
                best_idx = IntPair { x: i, y: -1 };
            }
            if n == 1 {
                for (j, sub) in sub_slots_of(&first).into_iter().enumerate() {
                    let sn = sub.borrow().len();
                    if sn != 0 {
                        let st = slot_first(&sub).expect("sn > 0").borrow().item_type;
                        if item_type_is_sowable(st) && sn > 10 && sn > max_count {
                            max_count = sn;
                            best_type = st;
                            best_idx = IntPair { x: i, y: j as i32 };
                        }
                    }
                }
            }
        }
        (best_type, best_idx)
    }

    /// `-sowableItemForAIItemType` (default `ITEM_NONE`).
    pub fn sowable_item_for_ai_item_type(&self) -> ItemType {
        self.sowable_scan().0
    }

    /// `-sowableItemForAIItemIndex` (default `(-1, -1)`).
    pub fn sowable_item_for_ai_item_index(&self) -> IntPair {
        self.sowable_scan().1
    }

    /// First light in the inventory: `(type, (slot, sub))`. Light set {17, 183, 47, 150, 254, 258}.
    fn placable_light_scan(&self) -> (ItemType, IntPair) {
        let is_light = |t: ItemType| matches!(t.0, 17 | 183 | 47 | 150 | 254 | 258);
        let inv = self.inv();
        for i in 1..8i32 {
            let slot = outer_slot_at(&inv, i);
            let n = slot.borrow().len();
            if n == 0 {
                continue;
            }
            let first = slot_first(&slot).expect("n > 0");
            let t = first.borrow().item_type;
            if is_light(t) {
                return (t, IntPair { x: i, y: -1 });
            }
            if n == 1 {
                for (j, sub) in sub_slots_of(&first).into_iter().enumerate() {
                    if !sub.borrow().is_empty() {
                        let st = slot_first(&sub).expect("non-empty").borrow().item_type;
                        if is_light(st) {
                            return (st, IntPair { x: i, y: j as i32 });
                        }
                    }
                }
            }
        }
        (ItemType::ITEM_NONE, IntPair { x: -1, y: -1 })
    }

    /// `-placableLightForAIItemType`.
    pub fn placable_light_for_ai_item_type(&self) -> ItemType {
        self.placable_light_scan().0
    }

    /// `-placableLightForAIItemIndex`.
    pub fn placable_light_for_ai_item_index(&self) -> IntPair {
        self.placable_light_scan().1
    }

    /// `-mostCommonFoodTypeIndex`: location of the LAST scanned stack of the food kind with the highest
    /// total count (coffee excluded; lowest FoodType wins ties).
    pub fn most_common_food_type_index(&self) -> IntPair {
        let mut food_counts = [0i32; 31];
        let mut food_indices = [IntPair { x: -1, y: -1 }; 31];
        let mut note = |f: FoodType, n: usize, idx: IntPair| {
            if f != FoodType::FOOD_NONE && f != FoodType::FOOD_COFFEE && (f.0 as usize) < 31 {
                food_counts[f.0 as usize] = food_counts[f.0 as usize].wrapping_add(n as i32);
                food_indices[f.0 as usize] = idx;
            }
        };
        let inv = self.inv();
        for i in 1..8i32 {
            let slot = outer_slot_at(&inv, i);
            let n = slot.borrow().len();
            if n == 0 {
                continue;
            }
            let first = slot_first(&slot).expect("n > 0");
            let (ft, fdb) = {
                let b = first.borrow();
                (b.item_type, b.data_b)
            };
            note(food_type_for_item_type(ft, fdb as u32 as i32), n, IntPair { x: i, y: -1 });
            if n == 1 {
                for (j, sub) in sub_slots_of(&first).into_iter().enumerate() {
                    let sn = sub.borrow().len();
                    if sn != 0 {
                        let sf = slot_first(&sub).expect("sn > 0");
                        let (st, sdb) = {
                            let b = sf.borrow();
                            (b.item_type, b.data_b)
                        };
                        note(food_type_for_item_type(st, sdb as u32 as i32), sn, IntPair { x: i, y: j as i32 });
                    }
                }
            }
        }
        let mut max_count = 0;
        let mut max_idx = IntPair { x: -1, y: -1 };
        for k in 0..31 {
            if max_count < food_counts[k] {
                max_count = food_counts[k];
                max_idx = food_indices[k];
            }
        }
        max_idx
    }

    // ================================================================================================
    // 5.9 - 5.11 removals
    // ================================================================================================

    /// `-itemWillBeRemovedFromInventory:`: recursion over nested items, then clears `interactionItem`
    /// (and its indices) when it is this item (pointer identity).
    pub fn item_will_be_removed_from_inventory(&self, item: &ItemRef) {
        for sub_slot in sub_slots_of(item) {
            let items: Vec<ItemRef> = sub_slot.borrow().clone();
            for sub_item in items.iter() {
                self.item_will_be_removed_from_inventory(sub_item);
            }
        }
        let is_it = self.bh.borrow().interaction_item.as_ref().map_or(false, |x| std::rc::Rc::ptr_eq(x, item));
        if is_it {
            let mut f = self.bh.borrow_mut();
            f.interaction_item = None;
            f.interaction_item_index = -1;
            f.interaction_item_sub_index = -1;
        }
    }

    /// `-subtractItemsFromInventoryOfType:count:` = `...dataB:-1`. Returns the items actually removed.
    pub fn subtract_items_from_inventory_of_type_count(&self, t: ItemType, count: i32) -> Vec<ItemRef> {
        self.subtract_items_from_inventory_of_type_count_data_b(t, count, -1)
    }

    /// `-subtractItemsFromInventoryOfType:count:dataB:`. Removes up to `count` items of `t` (stacks with
    /// the smallest count first; the contents of removed containers spill as free blocks with `playSound 2`,
    /// `priorityBlockhead = self`, time crystals vanish) and returns copies of the removed items (containers
    /// stripped of contents). The contract text says `bool`; the spec returns the `NSMutableArray`.
    pub fn subtract_items_from_inventory_of_type_count_data_b(
        &self,
        t: ItemType,
        count: i32,
        data_b: i32,
    ) -> Vec<ItemRef> {
        let h = host();
        let me = self.me();
        let mut removed: Vec<ItemRef> = Vec::new();
        let mut remaining = count;
        let mut last = -1;
        let inv = self.inv();
        loop {
            if remaining < 1 || last == remaining {
                return removed;
            }
            last = remaining;
            let mut lowest: usize = 100;
            let mut sel: Option<SubSlot> = None;
            let mut sel_i: i32 = -1;
            let mut sel_sub: i32 = -1;
            for i in (1..8i32).rev() {
                let slot = outer_slot_at(&inv, i);
                let n = slot.borrow().len();
                let top_match = n != 0 && {
                    let f = slot_first(&slot).expect("n > 0");
                    let b = f.borrow();
                    b.item_type == t && (data_b == -1 || b.data_b as i32 == data_b)
                };
                if top_match {
                    if n < lowest {
                        sel = Some(slot.clone());
                        sel_i = i;
                        sel_sub = -1;
                        lowest = n;
                    }
                } else if n != 0 {
                    let first = slot_first(&slot).expect("n > 0");
                    for (si, sub) in sub_slots_of(&first).into_iter().enumerate() {
                        let sn = sub.borrow().len();
                        let m = sn != 0 && {
                            let f = slot_first(&sub).expect("sn > 0");
                            let b = f.borrow();
                            b.item_type == t && (data_b == -1 || b.data_b as i32 == data_b)
                        };
                        if m && sn < lowest {
                            sel = Some(sub.clone());
                            sel_i = i;
                            sel_sub = si as i32;
                            lowest = sn;
                        }
                    }
                }
            }
            let sel = match sel {
                Some(s) if lowest != 100 => s,
                _ => return removed,
            };
            let n = std::cmp::min(lowest as i32, remaining);
            for _ in 0..n {
                let item = match slot_last(&sel) {
                    Some(i) => i,
                    None => break,
                };
                self.item_will_be_removed_from_inventory(&item);
                let (ity, da, db) = {
                    let b = item.borrow();
                    (b.item_type, b.data_a, b.data_b)
                };
                let mut out_item = item.clone();
                let spill = |sub: &InventoryItem| {
                    if sub.item_type != ItemType::ITEM_TIME_CRYSTAL {
                        let sv = sub.sub_items.as_ref().map(|s| slots_to_vecs(s));
                        h.create_free_block_at_position_of_type_data_a_data_b_sub_items_dynamic_object_save_dict_hovers_play_sound_priority_blockhead(
                            self.pos(),
                            sub.item_type,
                            sub.data_a,
                            sub.data_b,
                            sv.as_deref(),
                            sub.dynamic_object_save_dict.as_ref(),
                            false,
                            2,
                            Some(&me),
                        );
                    }
                };
                if matches!(ity.0, 1043 | 1072 | 1065 | 161 | 207 | 206) {
                    let sd = item.borrow().dynamic_object_save_dict.clone();
                    if let Some(sd) = sd {
                        let slots_val = if ity.0 == 206 {
                            sd.object_for_key("chest").and_then(|c| c.object_for_key("saveItemSlots"))
                        } else {
                            sd.object_for_key("saveItemSlots")
                        };
                        if let Some(PlistValue::Array(slots)) = slots_val {
                            for save_slot in slots.items.iter() {
                                if let PlistValue::Array(sa) = save_slot {
                                    for save_item in sa.items.iter() {
                                        if let PlistValue::Data(bytes) = save_item {
                                            let sub = InventoryItem::from_save_data(&bytes.bytes);
                                            spill(&sub.borrow());
                                        }
                                    }
                                }
                            }
                        }
                    }
                    out_item = InventoryItem::new(ity, da, db, None, None);
                } else if ity == ItemType::ITEM_BASKET {
                    for sub_slot in sub_slots_of(&item) {
                        let subs: Vec<ItemRef> = sub_slot.borrow().clone();
                        for sub_item in subs.iter() {
                            spill(&sub_item.borrow());
                        }
                    }
                    out_item = InventoryItem::new(ity, da, db, None, None);
                }
                removed.push(out_item);
                sel.borrow_mut().pop();
                self.mark_changed(sel_i, sel_sub);
            }
            remaining -= n;
        }
    }

    // ================================================================================================
    // 5.12 wear
    // ================================================================================================

    /// `-incrementUsageOfItem:indexToUse:wasAttack:` = `multiplier:1`.
    pub fn increment_usage_of_item_index_to_use_was_attack(&self, item: &ItemRef, index: i32, was_attack: bool) {
        self.increment_usage_of_item_index_to_use_was_attack_multiplier(item, index, was_attack, 1)
    }

    /// `-incrementUsageOfItem:indexToUse:wasAttack:multiplier:`. Adds wear to `dataA` (16-bit wrap); at
    /// `dataA >= 0x4000` the tool is removed (`removeItem:index:wasUsedUp:YES`).
    pub fn increment_usage_of_item_index_to_use_was_attack_multiplier(
        &self,
        item: &ItemRef,
        index: i32,
        was_attack: bool,
        multiplier: i32,
    ) {
        if index < 1 {
            return;
        }
        let ih = inventory_host();
        let mut was_interaction_item =
            self.bh.borrow().interaction_item.as_ref().map_or(false, |x| std::rc::Rc::ptr_eq(x, item));
        let queue0 = self.bh.borrow().action_queue.first().cloned();
        if let Some(a) = &queue0 {
            was_interaction_item = ih.action_interaction_item(a).map_or(false, |x| std::rc::Rc::ptr_eq(&x, item));
        }
        let ty = item.borrow().item_type;
        if ty == ItemType::ITEM_FISHING_ROD && self.bh.borrow().fishing_rod.is_none() {
            return;
        }
        let level = self.custom_tool_decay_level();
        let exp = self.world_rc().map_or(false, |w| w.expert_mode());
        let inc = usage_increment_per_use(ty, level, exp, was_attack).wrapping_mul(multiplier);
        if inc < 1 {
            return;
        }
        {
            let mut b = item.borrow_mut();
            b.data_a = (b.data_a as i16).wrapping_add(inc as i16) as u16;
        }
        if (0..8).contains(&index) {
            self.bh.borrow_mut().inventory_usage_changed_this_frame_slots[index as usize] = true;
        }
        if item.borrow().data_a < 0x4000 {
            return;
        }
        // used up
        let kept = item.clone();
        self.remove_item_index_was_used_up(item, index, true);
        if was_interaction_item {
            let kt = kept.borrow().item_type;
            if kt == ItemType::ITEM_PAINT || kt == ItemType::ITEM_PAINT_STRIPPER {
                let sq = self.bh.borrow().state.interacting_square;
                let me = self.me();
                host().create_free_block_at_position_of_type_data_a_data_b_sub_items_dynamic_object_save_dict_hovers_play_sound_priority_blockhead(
                    sq,
                    ItemType::ITEM_TIN_BUCKET,
                    0,
                    0,
                    None,
                    None,
                    false,
                    2,
                    Some(&me),
                );
            }
            let queue0 = self.bh.borrow().action_queue.first().cloned();
            match queue0 {
                None => self.stop_interacting(),
                Some(a) => {
                    let inv = self.inv();
                    let slot = outer_slot_at(&inv, ih.action_interaction_item_index(&a) as i16 as i32);
                    let mut cur: Option<ItemRef> = None;
                    if !slot.borrow().is_empty() {
                        cur = slot_last(&slot);
                        let c = cur.clone().expect("non-empty");
                        if !super::inventory_model::sub_slots_of(&c).is_empty() {
                            let sub = sub_slot_at(&c, ih.action_interaction_item_sub_index(&a) as i16 as i64);
                            if let Some(sub) = sub {
                                if !sub.borrow().is_empty() {
                                    cur = slot_last(&sub);
                                }
                            }
                        }
                    }
                    let cur_type = cur.as_ref().map_or(0, |c| c.borrow().item_type.0);
                    if cur_type == kept.borrow().item_type.0 {
                        ih.action_set_interaction_item(&a, cur.as_ref());
                        let ai = ih.action_interaction_item(&a);
                        self.bh.borrow_mut().interaction_item = ai;
                    } else {
                        self.stop_interacting();
                    }
                }
            }
        }
        // toolBreak.wav: client audio, no effect
    }

    /// `-incrementPassiveItemUsage` = `incrementUsageOfInteractionItem:NO`.
    pub fn increment_passive_item_usage(&self) {
        self.increment_usage_of_interaction_item(false);
    }

    /// `-incrementUsageOfInteractionItem:`.
    pub fn increment_usage_of_interaction_item(&self, was_attack: bool) {
        let (mut item, mut idx, queue0) = {
            let f = self.bh.borrow();
            (f.interaction_item.clone(), f.interaction_item_index, f.action_queue.first().cloned())
        };
        if let Some(a) = queue0 {
            let ih = inventory_host();
            item = ih.action_interaction_item(&a);
            idx = ih.action_interaction_item_index(&a);
        }
        if let Some(item) = item {
            self.increment_usage_of_item_index_to_use_was_attack(&item, idx, was_attack);
        }
    }

    /// `-incrementUsageOfClothing`: passive wear of each worn item (only the first item of each sub-slot).
    /// `dataA += (int)((float)dataA + rate * 200.0f)` in f32.
    pub fn increment_usage_of_clothing(&self) {
        let subs = match self.clothing_sub_slots() {
            Some(s) => s,
            None => return,
        };
        for (sub_index, item_slot) in subs.into_iter().enumerate() {
            let sub_index = sub_index as i32;
            if !item_slot.borrow().is_empty() {
                let item = slot_first(&item_slot).expect("non-empty");
                let ty = item.borrow().item_type;
                let lvl = self.custom_clothing_decay_level();
                let pole = self.custom_pole_items();
                let expert = self.world_rc().map_or(false, |w| w.expert_mode());
                let rate = clothing_item_pasively_used_rate(ty, lvl, pole, expert);
                if rate > 0.0f32 {
                    {
                        let mut b = item.borrow_mut();
                        b.data_a = f32_to_i32((b.data_a as f32) + rate * 200.0f32) as i16 as u16;
                    }
                    self.mark_clothing_usage_changed(sub_index);
                    if item.borrow().data_a > 0x3fff {
                        self.clothing_remove_if_used_up(&item, &item_slot, sub_index);
                    }
                }
            }
        }
    }

    /// `-incrementUsageOfIceClothing`: cold wear of ice gear, `(currentTemperature / 30.0f) * 10.0f`, no
    /// custom-rule scaling.
    pub fn increment_usage_of_ice_clothing(&self) {
        let subs = match self.clothing_sub_slots() {
            Some(s) => s,
            None => return,
        };
        for (sub_index, sub) in subs.into_iter().enumerate() {
            let sub_index = sub_index as i32;
            if !sub.borrow().is_empty() {
                let item = slot_first(&sub).expect("non-empty");
                let ty = item.borrow().item_type;
                // ICE_BOOTS 198, ICE_PANTS 195, ICE_CHEST_PLATE 196, ICE_HELMET 197
                if matches!(ty.0, 198 | 195 | 196 | 197) {
                    let r = self.bh.borrow().current_temperature / 30.0f32;
                    if r > 0.0f32 {
                        {
                            let mut b = item.borrow_mut();
                            b.data_a = f32_to_i32((b.data_a as f32) + r * 10.0f32) as i16 as u16;
                        }
                        self.mark_clothing_usage_changed(sub_index);
                        if item.borrow().data_a > 0x3fff {
                            self.clothing_remove_if_used_up(&item, &sub, sub_index);
                        }
                    }
                }
            }
        }
    }

    /// `-incrementDamageOfArmorClothing:`: wear proportional to the damage (capped at 1.0, not floored).
    /// `dataA += (int)((float)dataA + (rate * 100.0f) * d)` in f32.
    pub fn increment_damage_of_armor_clothing(&self, damage: f32) {
        let d = if 1.0f32 <= damage { 1.0f32 } else { damage };
        let subs = match self.clothing_sub_slots() {
            Some(s) => s,
            None => return,
        };
        for (sub_index, sub) in subs.into_iter().enumerate() {
            let sub_index = sub_index as i32;
            if !sub.borrow().is_empty() {
                let item = slot_first(&sub).expect("non-empty");
                let ty = item.borrow().item_type;
                let lvl = self.custom_clothing_decay_level();
                let rate = armour_item_damage_usage_rate(ty, lvl);
                if rate > 0.0f32 {
                    {
                        let mut b = item.borrow_mut();
                        b.data_a = f32_to_i32((b.data_a as f32) + (rate * 100.0f32) * d) as i16 as u16;
                    }
                    self.mark_clothing_usage_changed(sub_index);
                    if item.borrow().data_a > 0x3fff {
                        self.clothing_remove_if_used_up(&item, &sub, sub_index);
                    }
                }
            }
        }
    }

    /// `-incrementFuelUsage`: jetpack fuel in `dataB` (+100 per call, 16-bit wrap, capped at 0x4000); sets
    /// the "changed" flag (not the usage flag).
    pub fn increment_fuel_usage(&self) {
        let subs = match self.clothing_sub_slots() {
            Some(s) => s,
            None => return,
        };
        for (sub_index, sub) in subs.into_iter().enumerate() {
            let sub_index = sub_index as i32;
            if !sub.borrow().is_empty() {
                let j = slot_first(&sub).expect("non-empty");
                if j.borrow().item_type == ItemType::ITEM_JETPACK {
                    {
                        let mut b = j.borrow_mut();
                        b.data_b = (b.data_b as i16).wrapping_add(100) as u16;
                    }
                    {
                        let mut f = self.bh.borrow_mut();
                        if sub_index == -1 || sub_index > 3 {
                            f.inventory_changed_this_frame_slots[0] = true;
                        } else {
                            f.sub_inventory_changed_this_frame_slots[0][sub_index as usize] = true;
                        }
                    }
                    if j.borrow().data_b > 0x3fff {
                        j.borrow_mut().data_b = 0x4000;
                    }
                }
            }
        }
    }
}

/// Local helper trait: sets `subInventoryUsageChangedThisFrameSlots[0][sub]`.
trait SubUsageSet {
    fn sub_usage_slot_set(&mut self, sub: i32);
}
impl SubUsageSet for super::blockhead::BlockheadFields {
    fn sub_usage_slot_set(&mut self, sub: i32) {
        self.sub_inventory_usage_changed_this_frame_slots[0][sub as usize] = true;
    }
}
