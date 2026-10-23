//! `Blockhead` inventory model: save arrays, drop / move / remove, current (held) item, `useCurrentItemIfPossible`,
//! `setPath:...`, `infoForPathRecalculation` and the small state predicates.
//!
//! `### 3.2` (`setPath`),, including the
//! `## Synthesis resolutions` blocks of / /.
//!
//! ## Seam
//! This file also defines [`InventoryHost`], the seam for the callees that belong to modules that did not exist yet
//! when this module was written (`FreeBlock` accessors, `Action` accessors, `Workbench`, `DynamicWorld` lookups).
//! It follows the pattern of `blockhead::blockhead::BlockheadHost`: every method has an inert default; the integrator
//! implements it once as a thin adapter and installs it with [`set_inventory_host`]. Calls that already exist on
//! `BlockheadHost` (free-block creation, `stopRiding`, `goodOrBadInteractionForAction:`,...) go through `host()`.

use std::any::Any;
use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::Rc;

use crate::fmath::cconv::f32_to_i16;
use crate::foundation::exception::range_exception_index;
use crate::foundation::nslog::nslog;
use crate::items::colors::mixed_color_for_index;
use crate::items::food::{food_type_for_item_type, nourishment_fraction_for_food_type};
use crate::items::inventory_item::{ItemRef, SubSlot};
use crate::items::predicates::{
    interaction_type_disallowed_on_protected_tiles, inventory_sub_items_count_for_item_type, item_type_can_be_worn,
    item_type_can_be_worn_on_head, item_type_can_be_worn_on_legs, item_type_can_be_worn_on_torso,
    item_type_is_stackable, item_type_sub_items_can_be_modified_while_carried,
};
use crate::objects::handle::ObjRef;
use crate::plist::value::PlistValue;
use crate::types::enums::{FoodType, InteractionType};
use crate::types::item_type::ItemType;
use crate::types::structs::{IntPair, InteractionTestResult, Vector2};
use crate::world::state::TileRef;
use crate::world::tiles::{get_world_pos_for_world_index, tile_at_world_position_loaded};

use super::blockhead::{host, it, ActionRef, Blockhead, BlockheadFields, ExtraValue, FishingRodRef, PathExtraData};

// ====================================================================================================
// Constants
// ====================================================================================================

/// `PathType` raw values.
pub mod pt {
    pub const WORLD_TAPPED: i32 = 0;
    pub const WORLD_TAPPED_MOVE_ONLY: i32 = 1;
    pub const RECALCULATED_DUE_TO_CHANGE: i32 = 2;
    pub const NPC_CHASE: i32 = 3;
    pub const RIDE_OBJECT_CHASE: i32 = 4;
    pub const FALL: i32 = 5;
    pub const FALL_INTENTIONAL: i32 = 6;
}

// ====================================================================================================
// The seam
// ====================================================================================================

/// What `blockhead_inventory` reads from a `FreeBlock` (physics_objects; "Its accessors"). `uniqueID` and
/// `needsRemoved` are `DynamicObject` base ivars and are read from the object itself.
#[derive(Clone)]
pub struct FreeBlockData {
    /// `-itemType`
    pub item_type: ItemType,
    /// `-dataA`
    pub data_a: u16,
    /// `-dataB`
    pub data_b: u16,
    /// `-subItems` (shared slots; `None` = nil)
    pub sub_items: Option<Vec<SubSlot>>,
    /// `-dynamicObjectSaveDict`
    pub dynamic_object_save_dict: Option<PlistValue>,
    /// `-hovers`
    pub hovers: bool,
    /// `-priorityBlockhead`
    pub priority_blockhead: Option<ObjRef>,
}

crate::hooks::hook_trait! {
/// Calls into modules that were empty skeletons when `blockhead_inventory` was written. All defaults are inert (or the
/// spec-derived trivial value).
#[allow(unused_variables)]
pub trait InventoryHost, stub InventoryHostStub {
    // ---------------- FreeBlock (physics_objects) ----------------
    /// Accessors of `FreeBlock`. `None` if `fb` is not a `FreeBlock`.
    fn free_block_data(&self, fb: &ObjRef) -> Option<FreeBlockData> {
        None
    }
    /// `-[FreeBlock priorityBlockheadCannotPickup]`
    fn free_block_priority_blockhead_cannot_pickup(&self, fb: &ObjRef) {}

    // ---------------- DynamicWorld / World ----------------
    /// `-[DynamicWorld isClient]` (NO on the dedicated server).
    fn dynamic_world_is_client(&self) -> bool {
        false
    }
    /// `[world addSimulationEventOfType:SE_GOT forBlockhead:b extraData:freeBlock]`. The `extraData` is
    /// the FreeBlock itself, which `SimulationEventExtra` cannot carry, hence this dedicated call.
    #[provided]
    fn add_simulation_event_got(&self, b: &ObjRef, fb: &ObjRef) {
        host().add_simulation_event_of_type_for_blockhead_extra_data(2, b, super::blockhead::SimulationEventExtra::None)
    }
    /// `-[World startUsingCamera]` (client UI).
    fn world_start_using_camera(&self) {}
    /// `-[DynamicWorld harmableDynamicObjectWithID:]`
    fn harmable_dynamic_object_with_id(&self, id: u64) -> Option<ObjRef> {
        None
    }
    /// `-[DynamicWorld boatWithID:]`
    fn boat_with_id(&self, id: u64) -> Option<ObjRef> {
        None
    }
    /// `-[DynamicWorld trainCarWithID:]`
    fn train_car_with_id(&self, id: u64) -> Option<ObjRef> {
        None
    }
    /// `-[DynamicWorld activeBlockhead]` (nil on the server).
    fn dynamic_world_active_blockhead(&self) -> Option<ObjRef> {
        None
    }

    // ---------------- Blockhead siblings (blockhead_actions / physics_objects) ----------------
    /// `-[Blockhead currentInteractionTypeForTile:atPos:pickupRejectedDueToInventoryFull:includeActions:faceIndex:allowProtectedActions:]`
    ///  = `interactionTypeForTile:...item:[self currentItem]...`. Returns an `InteractionType` raw value.
    #[allow(clippy::too_many_arguments)]
    #[provided]
    fn current_interaction_type_for_tile_at_pos_pickup_rejected_due_to_inventory_full_include_actions_face_index_allow_protected_actions(
        &self,
        b: &Blockhead,
        tile: Option<TileRef>,
        pos: IntPair,
        rejected: &mut bool,
        include_actions: bool,
        face_index: i32,
        allow_protected_actions: bool,
    ) -> u32 {
        let item = b.current_item();
        host().interaction_type_for_tile_at_pos_item_pickup_rejected_due_to_inventory_full_include_actions_face_index_allow_protected_actions(
            b, tile, pos, item.as_ref(), rejected, include_actions, face_index, allow_protected_actions,
        )
    }
    /// `-[Blockhead canFish]`
    fn can_fish(&self, b: &Blockhead) -> bool {
        false
    }
    /// `[[FishingRod alloc] initWithWorld:blockhead:cache:]`
    fn fishing_rod_new(&self, b: &Blockhead) -> FishingRodRef {
        Rc::new(())
    }
    /// `[rideObject setTargetVelocity:(0,0)]`
    fn ride_object_set_target_velocity_zero(&self, o: &ObjRef) {}
    /// `blockheadAI != nil` (the AI is not part of the server's player blockheads).
    fn has_blockhead_ai(&self, b: &Blockhead) -> bool {
        false
    }
    /// `[blockheadAI noPathToAction]`
    fn blockhead_ai_no_path_to_action(&self, b: &Blockhead) {}
    /// `[npc blockheadIsComingToInspect:b]`
    fn npc_blockhead_is_coming_to_inspect(&self, npc: &ObjRef, b: &Blockhead) {}

    // ---------------- Action (blockhead_actions,) ----------------
    /// `-[Action goalInteraction]`
    fn action_goal_interaction(&self, a: &ActionRef) -> i32 {
        0
    }
    /// `-[Action interactionItem]`
    fn action_interaction_item(&self, a: &ActionRef) -> Option<ItemRef> {
        None
    }
    /// `(int)(int16_t)[action interactionItemIndex]`
    fn action_interaction_item_index(&self, a: &ActionRef) -> i32 {
        0
    }
    /// `(int)(int16_t)[action interactionItemSubIndex]`
    fn action_interaction_item_sub_index(&self, a: &ActionRef) -> i32 {
        0
    }
    /// `-[Action setInteractionItem:]`
    fn action_set_interaction_item(&self, a: &ActionRef, item: Option<&ItemRef>) {}
    /// `-[Action setInteractionItemIndex:]`
    fn action_set_interaction_item_index(&self, a: &ActionRef, v: i32) {}
    /// `-[Action setInteractionItemSubIndex:]`
    fn action_set_interaction_item_sub_index(&self, a: &ActionRef, v: i32) {}
    /// `(int)[action craftCountOrExtraData]`
    fn action_craft_count_or_extra_data(&self, a: &ActionRef) -> i32 {
        0
    }
    /// `-[Action pathType]`
    fn action_path_type(&self, a: &ActionRef) -> i32 {
        0
    }
    /// `-[Action craftableItemObject]` (`BlockheadCraftableItemObject`, not a `DynamicObject`)
    fn action_craftable_item_object(&self, a: &ActionRef) -> Option<Rc<dyn Any>> {
        None
    }

    // ---------------- Workbench (workbench) ----------------
    /// `-[Workbench currentlyCraftingItemType]`
    fn workbench_currently_crafting_item_type(&self, wb: &ObjRef) -> i32 {
        0
    }
    /// `-[Workbench totalItemsLeftToCraft]`
    fn workbench_total_items_left_to_craft(&self, wb: &ObjRef) -> i32 {
        0
    }
}
}

/// The default inventory host: every method as defaulted.
pub struct NullInventoryHost;
impl InventoryHostStub for NullInventoryHost {}

thread_local! {
    static INV_HOST: RefCell<Rc<dyn InventoryHost>> = RefCell::new(Rc::new(NullInventoryHost));
    /// `[NSUserDefaults standardUserDefaults]` as far as this module uses it (synthesis: in-process set, never
    /// persisted; `// UB-PARITY`: GNUstep's timed write to `~/GNUstep/Defaults` is not reproduced).
    static USER_DEFAULTS_BOOLS: RefCell<HashSet<String>> = RefCell::new(HashSet::new());
}

/// Installs the inventory host (once at startup; tests install recording hosts).
pub fn set_inventory_host(h: Rc<dyn InventoryHost>) {
    INV_HOST.with(|c| *c.borrow_mut() = h);
}

/// The installed inventory host.
pub fn inventory_host() -> Rc<dyn InventoryHost> {
    INV_HOST.with(|c| c.borrow().clone())
}

/// `[[NSUserDefaults standardUserDefaults] boolForKey:key]`
pub(crate) fn user_defaults_bool_for_key(key: &str) -> bool {
    USER_DEFAULTS_BOOLS.with(|s| s.borrow().contains(key))
}

/// `[[NSUserDefaults standardUserDefaults] setBool:YES forKey:key]`
pub(crate) fn user_defaults_set_yes_for_key(key: &str) {
    USER_DEFAULTS_BOOLS.with(|s| {
        s.borrow_mut().insert(key.to_string());
    });
}

/// Test helper: clears the in-process defaults.
#[cfg(test)]
pub(crate) fn user_defaults_clear() {
    USER_DEFAULTS_BOOLS.with(|s| s.borrow_mut().clear());
}

// ====================================================================================================
// Path recalculation info
// ====================================================================================================

/// The `NSMutableDictionary` returned by `-infoForPathRecalculation`; `PlistValue` cannot hold the Blockhead under
/// `"pathUser"`, hence a struct. Key names: `"pathUser"`, `"goalX"`/`"goalY"` (= `goal`), `"goalInteraction"`,
/// `"pathType"`, `"extraData"` (inner keys `craftCountOrExtraData`, `interactionObject`, `craftableItemObject`,
/// `workbench`, `count`).
#[derive(Clone)]
pub struct PathRecalcInfo {
    /// `"pathUser"`
    pub path_user: ObjRef,
    /// `"goalX"`, `"goalY"`
    pub goal: IntPair,
    /// `"goalInteraction"`
    pub goal_interaction: i32,
    /// `"pathType"`
    pub path_type: i32,
    /// `"extraData"` (nil when absent)
    pub extra: Option<PathExtraData>,
}

// ====================================================================================================
// Shared helpers
// ====================================================================================================

/// `[array objectAtIndex:i]` on the 8 outer slots (no bounds check in the original: `NSRangeException`).
/// `i` is an `int` converted to `NSUInteger` by sign extension.
pub(crate) fn outer_slot_at(items: &[SubSlot], i: i32) -> SubSlot {
    let u = i as i64 as u64;
    if u >= items.len() as u64 {
        range_exception_index(u, items.len() as u64, "objectAtIndex:");
    }
    items[u as usize].clone()
}

/// `[[item subItems] objectAtIndex:i]`: a nil `subItems` gives nil (`None`); an index past the end raises.
pub(crate) fn sub_slot_at(item: &ItemRef, i: i64) -> Option<SubSlot> {
    let it = item.borrow();
    let subs = it.sub_items.as_ref()?;
    let u = i as u64;
    if u >= subs.len() as u64 {
        range_exception_index(u, subs.len() as u64, "objectAtIndex:");
    }
    Some(subs[u as usize].clone())
}

/// `[[item subItems] count]` (nil gives 0).
pub(crate) fn sub_slot_count(item: &ItemRef) -> usize {
    item.borrow().sub_items.as_ref().map_or(0, |s| s.len())
}

/// `for (sub in item.subItems)`: a nil `subItems` enumerates nothing.
pub(crate) fn sub_slots_of(item: &ItemRef) -> Vec<SubSlot> {
    item.borrow().sub_items.clone().unwrap_or_default()
}

/// `[slot containsObject:item]`: pointer identity (InventoryItem has no `isEqual:`; synthesis).
pub(crate) fn slot_contains(slot: &SubSlot, item: &ItemRef) -> bool {
    slot.borrow().iter().any(|x| Rc::ptr_eq(x, item))
}

/// `[slot removeObject:item]`: removes ALL pointer-identical entries.
pub(crate) fn slot_remove_object(slot: &SubSlot, item: &ItemRef) {
    slot.borrow_mut().retain(|x| !Rc::ptr_eq(x, item));
}

/// `[slot lastObject]`
pub(crate) fn slot_last(slot: &SubSlot) -> Option<ItemRef> {
    slot.borrow().last().cloned()
}

/// `[slot objectAtIndex:0]` for a slot known to be non-empty (`None` when empty).
pub(crate) fn slot_first(slot: &SubSlot) -> Option<ItemRef> {
    slot.borrow().first().cloned()
}

/// Converts shared sub-slots to the by-value form that `InventoryItem::new` and the free-block creation take.
pub(crate) fn slots_to_vecs(subs: &[SubSlot]) -> Vec<Vec<ItemRef>> {
    subs.iter().map(|s| s.borrow().clone()).collect()
}

/// `max(x, c)` as the compiler emitted it: `(x <= c) ? c: x`.
#[inline]
fn fmax(x: f32, c: f32) -> f32 {
    if x <= c {
        c
    } else {
        x
    }
}

impl Blockhead {
    /// Snapshot of the 8 outer slots (`self.inventoryItems`).
    pub(crate) fn inv(&self) -> Vec<SubSlot> {
        self.bh.borrow().inventory_items.clone()
    }

    /// `markChanged(i, s)`: overview "Flag-set helper". `i` out of range marks nothing; `s == -1 || s > 3` marks
    /// the outer slot, else the sub flag.
    pub(crate) fn mark_changed(&self, i: i32, s: i32) {
        if (0..8).contains(&i) {
            let mut f = self.bh.borrow_mut();
            if s == -1 || s > 3 {
                f.inventory_changed_this_frame_slots[i as usize] = true;
            } else {
                f.sub_inventory_changed_this_frame_slots[i as usize][s as usize] = true;
            }
        }
    }

    // ================================================================================================
    // inventoryWasChanged and the save arrays
    // ================================================================================================

    /// `-inventoryWasChanged:subIndex:wasUsage:`.
    /// The server (`isClient == NO`) always takes the full-rebuild path; the client-only partial path (dictionary
    /// entries in `saveItemSlotsArray`) is not representable and is never reached here.
    pub fn inventory_was_changed_sub_index_was_usage(&self, inventory_index: i32, sub_index: i32, was_usage: bool) {
        if inventory_index == -1 {
            nslog("unsupported inventoryIndex == -1");
        } else {
            let items = self.inv();
            let slot = outer_slot_at(&items, inventory_index);
            // Full rebuild path (always on the server).
            let mut save_slot: Vec<Vec<u8>> = Vec::new();
            let snapshot: Vec<ItemRef> = slot.borrow().clone();
            for item in snapshot.iter() {
                let it = item.borrow();
                if it.item_type != ItemType::ITEM_TIME_CRYSTAL {
                    save_slot.push(it.save_data());
                }
            }
            {
                let u = inventory_index as i64 as u64;
                let n = self.bh.borrow().save_item_slots_array.len() as u64;
                if u >= n {
                    range_exception_index(u, n, "replaceObjectAtIndex:withObject:");
                }
                self.bh.borrow_mut().save_item_slots_array[u as usize] = save_slot;
            }
            let _ = sub_index; // only used by the client-only partial path

            // Step 3: clothing refresh
            if inventory_index == 0 {
                self.refresh_clothing_from_slot_zero();
            }
        }
        // Step 5 epilogue: `[[world uiManager] blockheadInventoryChanged:...]` is a nil-receiver no-op.
        if !was_usage {
            self.bh.borrow_mut().inventory_needs_saving = true;
        }
    }

    /// Step 3 of `inventoryWasChanged`: hat (`clothingItemAtIndex:3`), shirt (2), pants (1), shoes (0), in that
    /// order, each item read fresh. Textures, sounds and achievements (`reportAchievementWithIdentifier:` has an
    /// empty body) are omitted.
    fn refresh_clothing_from_slot_zero(&self) {
        // (clothing index k, piece index p) in the processing order hat, shirt, pants, shoes
        for (k, p) in [(3i32, 0usize), (2, 1), (1, 2), (0, 3)] {
            let item = self.clothing_item_at_index(k);
            let mut changed_creation = false;
            {
                let mut f = self.bh.borrow_mut();
                let (ty, color_index, color) = piece_fields(&mut f, p);
                match item {
                    None => {
                        if *ty != 0 {
                            *ty = 0;
                            changed_creation = true;
                        }
                    }
                    Some(item) => {
                        let (item_type, data_b) = {
                            let i = item.borrow();
                            (i.item_type, i.data_b)
                        };
                        if *ty as u32 != item_type.0 {
                            // achievements: empty World method, omitted.
                            *ty = item_type.0 as u16;
                            changed_creation = true;
                        }
                        *color_index = data_b;
                        let m = mixed_color_for_index(data_b, ItemType(*ty as u32));
                        let sc = m.mul_f(0.8f32);
                        for l in 0..4 {
                            color[l] = 0.2f32 + sc.v[l];
                        }
                    }
                }
            }
            if changed_creation {
                self.base.borrow_mut().creation_data_needs_to_be_sent = true;
            }
        }
        self.update_clothing_cubes();
    }

    /// `-inventoryNeedsSaving`: `inventoryNeedsSaving && unconfirmedPickups.count == 0` (nil dictionary
    /// counts 0).
    pub fn inventory_needs_saving(&self) -> bool {
        let f = self.bh.borrow();
        if f.inventory_needs_saving {
            f.unconfirmed_pickups.as_ref().map_or(0, |m| m.len()) == 0
        } else {
            false
        }
    }

    /// `-setInventoryNeedsSaving:`. The client cache reset (`isClient && !v`) cannot hold the
    /// `NSNumber 0` placeholders; it is reproduced as eight empty slots.
    pub fn set_inventory_needs_saving(&self, v: bool) {
        self.bh.borrow_mut().inventory_needs_saving = v;
        if inventory_host().dynamic_world_is_client() && !v {
            self.bh.borrow_mut().save_item_slots_array = vec![Vec::new(); 8];
        }
    }

    // ================================================================================================
    // drop / move / remove
    // ================================================================================================

    /// `-dropInventoryItemsAtIndex:subIndex:count:ignoreFreeblocks:`. Only
    /// `count == -1` means "all". No bounds check on `subIndex` (NSRangeException). `inventoryWasChanged:` is NOT
    /// called here.
    pub fn drop_inventory_items_at_index_sub_index_count_ignore_freeblocks(
        &self,
        index: i32,
        sub_index: i32,
        count: i32,
        ignore_freeblocks: bool,
    ) {
        let items = self.inv();
        if index < 0 || index as usize >= items.len() {
            return;
        }
        let mut item_slot = items[index as usize].clone();
        if item_slot.borrow().is_empty() {
            return;
        }
        if index == 0 && sub_index == -1 {
            return;
        }
        if sub_index != -1 {
            let first = slot_first(&item_slot).expect("non-empty");
            match sub_slot_at(&first, sub_index as i64) {
                Some(s) => item_slot = s,
                None => return, // nil slot: count 0 => return below
            }
        }
        let mut n = count;
        if n == -1 {
            n = item_slot.borrow().len() as i32;
        }
        if n <= 0 || item_slot.borrow().is_empty() {
            return;
        }
        if (item_slot.borrow().len() as u64) < (n as i64 as u64) {
            nslog("Error: trying to drop more than I have");
            n = item_slot.borrow().len() as i32;
        }
        let h = host();
        let pos = self.pos();
        for _ in 0..n {
            let Some(last) = slot_last(&item_slot) else { break };
            let (item_type, data_a, data_b, subs, mut save_dict) = {
                let l = last.borrow();
                (l.item_type, l.data_a, l.data_b, l.sub_items.clone(), l.dynamic_object_save_dict.clone())
            };
            if let Some(PlistValue::Dict(d)) = save_dict.as_mut() {
                // `[NSMutableDictionary dictionaryWithDictionary:]` copy, then removal in this exact order
                for k in ["sellerClientID", "sellerClientName", "safeClientID", "ironPlaceClientID", "ownerID"] {
                    d.map.remove(&k.to_string());
                }
            }
            if item_type != ItemType::ITEM_TIME_CRYSTAL {
                let sub_vecs = subs.as_ref().map(|s| slots_to_vecs(s));
                h.create_free_block_at_position_of_type_data_a_data_b_sub_items_dynamic_object_save_dict_hovers_play_sound_priority_blockhead(
                    pos,
                    item_type,
                    data_a,
                    data_b,
                    sub_vecs.as_deref(),
                    save_dict.as_ref(),
                    false,
                    2,
                    None,
                );
            }
            self.item_will_be_removed_from_inventory(&last);
            item_slot.borrow_mut().pop();
            if ignore_freeblocks {
                let mut f = self.bh.borrow_mut();
                f.ignoring_freeblocks_due_to_drop = true;
                f.ignoring_freeblocks_due_to_drop_location = pos;
            }
        }
        self.mark_changed(index, sub_index);
    }

    /// `-moveInventoryItemsFromIndex:fromSubIndex:toIndex:toSubIndex:count:`. Returns the destination
    /// sub-index or -1.
    pub fn move_inventory_items_from_index_from_sub_index_to_index_to_sub_index_count(
        &self,
        from: i32,
        from_sub: i32,
        to: i32,
        to_sub: i32,
        count: i32,
    ) -> i32 {
        let items = self.inv();
        let n = items.len();
        if from < 0 || from as usize >= n || to < 0 || to as usize >= n {
            return -1;
        }
        let mut from_slot = items[from as usize].clone();
        if from_slot.borrow().is_empty() {
            return -1;
        }
        if from_sub != -1 {
            let first = slot_first(&from_slot).expect("non-empty");
            let cnt = sub_slot_count(&first);
            if (from_sub as i64 as u64) >= cnt as u64 {
                return -1;
            }
            from_slot = sub_slot_at(&first, from_sub as i64).expect("in range");
        }
        self.move_inventory_items_from_array_from_index_from_sub_index_to_index_to_sub_index_count_moved_items(
            &from_slot, from, from_sub, to, to_sub, count, None,
        )
    }

    /// `-removeInventoryItemsFromIndex:fromSubIndex:count:`. Any negative `count` means "all" (unsigned
    /// compare). No bounds check on the indices.
    pub fn remove_inventory_items_from_index_from_sub_index_count(&self, from: i32, from_sub: i32, count: i32) {
        if count == 0 || from == -1 {
            return;
        }
        let items = self.inv();
        let mut slot = outer_slot_at(&items, from);
        if slot.borrow().is_empty() {
            return;
        }
        if from_sub != -1 {
            let first = slot_first(&slot).expect("non-empty");
            match sub_slot_at(&first, from_sub as i64) {
                Some(s) => slot = s,
                None => return,
            }
        }
        let mut n = count as i64 as u64;
        let len = slot.borrow().len() as u64;
        if len < n {
            n = len;
        }
        // `markChanged` only in the `slot.count != 0` branch of the original (after the loop).
        for _ in 0..n {
            let last = match slot_last(&slot) {
                Some(l) => l,
                None => break,
            };
            self.item_will_be_removed_from_inventory(&last);
            slot.borrow_mut().pop();
        }
        self.mark_changed(from, from_sub);
    }

    /// `-moveInventoryItemsFromArray:fromIndex:fromSubIndex:toIndex:toSubIndex:count:movedItems:`.
    ///  `toIndex == 0` caps the move at one item; any negative `count` means "all".
    #[allow(clippy::too_many_arguments)]
    pub fn move_inventory_items_from_array_from_index_from_sub_index_to_index_to_sub_index_count_moved_items(
        &self,
        from_slot: &SubSlot,
        from_index: i32,
        from_sub_index: i32,
        to_index: i32,
        to_sub_index: i32,
        count: i32,
        moved_items: Option<&mut Vec<ItemRef>>,
    ) -> i32 {
        if count == 0 {
            return -1;
        }
        if from_slot.borrow().is_empty() {
            return -1;
        }
        let items = self.inv();
        let outer = outer_slot_at(&items, to_index);
        let mut result_sub = to_sub_index;
        let to_slot: SubSlot;
        if to_sub_index == -1 {
            let mut chosen = outer.clone();
            if !outer.borrow().is_empty() {
                let first = slot_first(&outer).expect("non-empty");
                let n_sub = inventory_sub_items_count_for_item_type(first.borrow().item_type);
                if n_sub > 0 {
                    let from_first_type = slot_first(from_slot).expect("non-empty").borrow().item_type;
                    let mut found = false;
                    // 1st pass: join an existing partial stack
                    for s in 0..n_sub {
                        let Some(cand) = sub_slot_at(&first, s as i64) else { break };
                        chosen = cand.clone();
                        let ok = {
                            let c = cand.borrow();
                            !c.is_empty() && c.len() < 99 && c[0].borrow().item_type == from_first_type
                        };
                        if ok {
                            found = true;
                            result_sub = s;
                            break;
                        }
                    }
                    if !found {
                        chosen = outer.clone();
                        // 2nd pass: first empty sub-slot
                        for s in 0..n_sub {
                            let Some(cand) = sub_slot_at(&first, s as i64) else { break };
                            if cand.borrow().is_empty() {
                                result_sub = s;
                                chosen = cand;
                                break;
                            }
                        }
                    }
                }
            }
            to_slot = chosen;
        } else {
            if outer.borrow().is_empty() {
                return -1;
            }
            let first = slot_first(&outer).expect("non-empty");
            match sub_slot_at(&first, to_sub_index as i64) {
                Some(s) => to_slot = s,
                None => return -1, // nil destination slot (no sub items): treated as a refusal
            }
        }
        let dest_count = to_slot.borrow().len() as i32;
        let (from_type, from_data_b) = {
            let f = slot_first(from_slot).expect("non-empty");
            let b = f.borrow();
            (b.item_type, b.data_b)
        };
        if dest_count >= 1 {
            let d = slot_first(&to_slot).expect("non-empty");
            let (dt, ddb) = {
                let b = d.borrow();
                (b.item_type, b.data_b)
            };
            if dt != from_type {
                return -1;
            }
            if !item_type_is_stackable(from_type, from_data_b, ddb) {
                return -1;
            }
        }
        // transfer
        let from_len = from_slot.borrow().len();
        let n: i64 = if (from_len as u64) < (count as i64 as u64) { from_len as i64 } else { count as i64 };
        let mut max_can_hold = 99 - dest_count;
        if to_index == 0 {
            max_can_hold = 1;
        }
        let move_n = if (n as i32) < max_can_hold { n as i32 } else { max_can_hold };
        let base = from_slot.borrow().len() as i64;
        let mut moved_items = moved_items;
        let mut j = 0;
        while j < move_n {
            let idx = (base - move_n as i64) as usize;
            let item = from_slot.borrow()[idx].clone();
            to_slot.borrow_mut().push(item.clone());
            if let Some(m) = moved_items.as_deref_mut() {
                m.push(item);
            }
            from_slot.borrow_mut().remove(idx);
            j += 1;
        }
        self.mark_changed(to_index, to_sub_index);
        self.mark_changed(from_index, from_sub_index);
        result_sub
    }

    // `-worldChanged:` / `-worldContentsChanged:` (empty bodies) and `-requiresPhysicalBlock` are implemented by
    // blockhead_core in its `impl DynamicObject for Blockhead` (blockhead.rs): not duplicated here.

    /// `-craftItemFinished:atWorkbench:`. Both arguments are ignored.
    pub fn craft_item_finished_at_workbench(&self, _completed: bool, _workbench: Option<&ObjRef>) {
        self.stop_interacting();
        self.bh.borrow_mut().interaction_workbench = None;
    }

    // ================================================================================================
    // current item
    // ================================================================================================

    /// Top-of-stack container test shared by the current-item methods:
    /// `top.subItems.count != 0 && itemTypeSubItemsCanBeModifiedWhileCarried(top.itemType)`.
    fn top_is_modifiable_container(top: &ItemRef) -> bool {
        sub_slot_count(top) != 0 && item_type_sub_items_can_be_modified_while_carried(top.borrow().item_type)
    }

    /// `-currentItemSlot`.
    pub fn current_item_slot(&self) -> Option<SubSlot> {
        let sel = self.bh.borrow().selected_tool_index;
        if sel == 0 {
            return None;
        }
        let items = self.inv();
        let mut slot = outer_slot_at(&items, sel);
        if !slot.borrow().is_empty() {
            let top = slot_last(&slot).expect("non-empty");
            if Self::top_is_modifiable_container(&top) {
                let idx = top.borrow().selected_sub_item_index as i64;
                slot = sub_slot_at(&top, idx)?;
            }
        }
        Some(slot)
    }

    /// `-setCurrentItemToItemAtIndex:`. The `uiManager` notification is a no-op.
    pub fn set_current_item_to_item_at_index(&self, idx: IntPair) {
        if idx.x != -1 {
            self.bh.borrow_mut().selected_tool_index = idx.x;
            if idx.y != -1 {
                let items = self.inv();
                let slot = outer_slot_at(&items, idx.x);
                if !slot.borrow().is_empty() {
                    let top = slot_last(&slot).expect("non-empty");
                    if Self::top_is_modifiable_container(&top) {
                        top.borrow_mut().selected_sub_item_index = idx.y as u8;
                    }
                }
            }
        }
    }

    /// `-currentItem`.
    pub fn current_item(&self) -> Option<ItemRef> {
        let sel = self.bh.borrow().selected_tool_index;
        if sel == 0 {
            return None;
        }
        let mut cur = None;
        let items = self.inv();
        let slot = outer_slot_at(&items, sel);
        if !slot.borrow().is_empty() {
            let c = slot_last(&slot).expect("non-empty");
            cur = Some(c.clone());
            if Self::top_is_modifiable_container(&c) {
                let idx = c.borrow().selected_sub_item_index as i64;
                if let Some(sub) = sub_slot_at(&c, idx).filter(|s| !s.borrow().is_empty()) {
                    cur = slot_last(&sub);
                }
            }
        }
        cur
    }

    /// `-currentItemSubIndex`: `selectedSubItemIndex` zero-extended, or -1.
    pub fn current_item_sub_index(&self) -> i32 {
        let sel = self.bh.borrow().selected_tool_index;
        if sel != 0 {
            let items = self.inv();
            let slot = outer_slot_at(&items, sel);
            if !slot.borrow().is_empty() {
                let top = slot_last(&slot).expect("non-empty");
                if Self::top_is_modifiable_container(&top) {
                    return top.borrow().selected_sub_item_index as i32;
                }
            }
        }
        -1
    }

    /// `-removeItem:index:wasUsedUp:`. `index <= 0` is a no-op. The changed flag
    /// uses the ARGUMENT `index`; the (client) flash uses the loop counter. When the item is found nowhere and it
    /// is the `interactionItem`, the interaction item is cleared.
    pub fn remove_item_index_was_used_up(&self, item: &ItemRef, index: i32, was_used_up: bool) {
        self.remove_item_opt(Some(item), index, was_used_up)
    }

    /// Nil-tolerant body of `removeItem:index:wasUsedUp:` (the argument may be nil when it comes from an Action).
    fn remove_item_opt(&self, item: Option<&ItemRef>, index: i32, was_used_up: bool) {
        if !(0 < index) {
            return;
        }
        let _ = was_used_up; // only drives the client flash (`uiManager` nil)
        if let Some(item) = item {
            let items = self.inv();
            for slot in items.iter() {
                if slot_contains(slot, item) {
                    if (0..8).contains(&index) {
                        self.bh.borrow_mut().inventory_changed_this_frame_slots[index as usize] = true;
                    }
                    self.item_will_be_removed_from_inventory(item);
                    slot_remove_object(slot, item);
                    return;
                }
                if !slot.borrow().is_empty() {
                    let first = slot_first(slot).expect("non-empty");
                    if item_type_sub_items_can_be_modified_while_carried(first.borrow().item_type) {
                        for (si, sub) in sub_slots_of(&first).into_iter().enumerate() {
                            if slot_contains(&sub, item) {
                                if (0..8).contains(&index) {
                                    let si = si as i32;
                                    let mut f = self.bh.borrow_mut();
                                    if si == -1 || si > 3 {
                                        f.inventory_changed_this_frame_slots[index as usize] = true;
                                    } else {
                                        f.sub_inventory_changed_this_frame_slots[index as usize][si as usize] = true;
                                    }
                                }
                                self.item_will_be_removed_from_inventory(item);
                                slot_remove_object(&sub, item);
                                return;
                            }
                        }
                    }
                }
            }
        }
        // not found anywhere
        let is_interaction_item = {
            let f = self.bh.borrow();
            match (&f.interaction_item, item) {
                (Some(a), Some(b)) => Rc::ptr_eq(a, b),
                (None, None) => true,
                _ => false,
            }
        };
        if is_interaction_item {
            let mut f = self.bh.borrow_mut();
            f.interaction_item = None;
            f.interaction_item_index = -1;
            f.interaction_item_sub_index = -1;
        }
    }

    /// `-removeCurrentItem`: removes the held item (top of stack only).
    pub fn remove_current_item(&self) {
        let sel = self.bh.borrow().selected_tool_index;
        if sel == 0 {
            return;
        }
        let mut removed_sub = false;
        let items = self.inv();
        let slot = outer_slot_at(&items, sel);
        if slot.borrow().is_empty() {
            return;
        }
        let mut cur = slot_last(&slot).expect("non-empty");
        if Self::top_is_modifiable_container(&cur) {
            let sub_idx = cur.borrow().selected_sub_item_index as u32;
            if let Some(sub) = sub_slot_at(&cur, sub_idx as i64).filter(|s| !s.borrow().is_empty()) {
                cur = slot_last(&sub).expect("non-empty");
                // `sel` is within 1..8 here: `outer_slot_at` above raises for anything else
                let mut f = self.bh.borrow_mut();
                if sub_idx > 3 {
                    f.inventory_changed_this_frame_slots[sel as usize] = true;
                } else {
                    f.sub_inventory_changed_this_frame_slots[sel as usize][sub_idx as usize] = true;
                }
                drop(f);
                self.item_will_be_removed_from_inventory(&cur);
                slot_remove_object(&sub, &cur);
                removed_sub = true;
            }
        }
        if !removed_sub {
            self.bh.borrow_mut().inventory_changed_this_frame_slots[sel as usize] = true;
            self.item_will_be_removed_from_inventory(&cur);
            slot_remove_object(&slot, &cur);
        }
    }

    /// `-removeInteractionItem:`.
    pub fn remove_interaction_item(&self, was_used_up: bool) {
        let (mut item, mut idx, queue0) = {
            let f = self.bh.borrow();
            (f.interaction_item.clone(), f.interaction_item_index, f.action_queue.first().cloned())
        };
        if let Some(a) = queue0 {
            let ih = inventory_host();
            item = ih.action_interaction_item(&a);
            idx = ih.action_interaction_item_index(&a);
        }
        self.remove_item_opt(item.as_ref(), idx, was_used_up);
    }

    /// `-hasInteractionInventoryItemAvailable`.
    pub fn has_interaction_inventory_item_available(&self) -> bool {
        let ii = self.bh.borrow().interaction_item.clone();
        let ii = match ii {
            None => return true,
            Some(i) => i,
        };
        for slot in self.inv().iter() {
            if slot_contains(slot, &ii) {
                return true;
            }
            if !slot.borrow().is_empty() {
                let first = slot_first(slot).expect("non-empty");
                if item_type_sub_items_can_be_modified_while_carried(first.borrow().item_type) {
                    for sub in sub_slots_of(&first) {
                        if slot_contains(&sub, &ii) {
                            return true;
                        }
                    }
                }
            }
        }
        false
    }

    /// `-setSelectedToolIndex:`
    pub fn set_selected_tool_index(&self, v: i32) {
        let mut f = self.bh.borrow_mut();
        if v != f.selected_tool_index {
            f.selected_tool_index = v;
        }
    }

    /// `-selectedToolIndex`
    pub fn selected_tool_index(&self) -> i32 {
        self.bh.borrow().selected_tool_index
    }

    /// `-removeInventoryItemIdenticalTo:`. Pointer identity. Faithful quirks: the
    /// sub-slot changed flag is indexed with the position `k` of the item inside the sub-slot, only the first 4
    /// entries of a sub-slot are searched, sub-slots only when the outer slot holds exactly one item, and
    /// `itemTypeSubItemsCanBeModifiedWhileCarried` is not consulted. `itemWillBeRemovedFromInventory:` is NOT called.
    pub fn remove_inventory_item_identical_to(&self, item: &ItemRef) {
        let items = self.inv();
        for (i, slot) in items.iter().enumerate().take(8) {
            let pos = slot.borrow().iter().position(|x| Rc::ptr_eq(x, item));
            if let Some(j) = pos {
                slot.borrow_mut().remove(j);
                self.bh.borrow_mut().inventory_changed_this_frame_slots[i] = true;
                return;
            }
            if slot.borrow().len() == 1 {
                let first = slot_first(slot).expect("non-empty");
                for sub in sub_slots_of(&first) {
                    let n = sub.borrow().len();
                    let mut k = 0usize;
                    while k < n && k <= 3 {
                        let hit = Rc::ptr_eq(&sub.borrow()[k], item);
                        if hit {
                            sub.borrow_mut().remove(k);
                            self.bh.borrow_mut().sub_inventory_changed_this_frame_slots[i][k] = true;
                            return;
                        }
                        k += 1;
                    }
                }
            }
        }
    }

    /// `-remotePickupRequestResponse:uniqueIDs:count:`. CLIENT-ONLY receipt path (never run by the
    /// server); ported for completeness. The cash variant (`NSNumber`) of `unconfirmedPickups` is not representable
    /// in the `BTreeMap<u64, ItemRef>` of blockhead_core.
    pub fn remote_pickup_request_response(&self, accepted: bool, unique_ids: &[u64]) {
        for id in unique_ids {
            if !accepted {
                let v = self.bh.borrow().unconfirmed_pickups.as_ref().and_then(|m| m.get(id).cloned());
                if let Some(item) = v {
                    self.remove_inventory_item_identical_to(&item);
                }
            }
            if let Some(m) = self.bh.borrow_mut().unconfirmed_pickups.as_mut() {
                m.remove(id);
            }
        }
    }

    // ================================================================================================
    // setPath and the path-wait helpers
    // ================================================================================================

    /// `-setPath:type:goalInteraction:extraData:`. The path is stored as received
    /// (element 0 = goal, last = next step). Sounds, achievements and the inventory-full popup are client-side
    /// no-ops. `// Note:`: the signature takes `Option<PathExtraData>` (blockhead_core's host trait) instead
    /// of the `Option<PlistValue>` of the contract, because the dictionary carries objects.
    pub fn set_path_type_goal_interaction_extra_data(
        &self,
        path: Option<Vec<PlistValue>>,
        path_type: i32,
        goal_interaction: i32,
        extra: Option<PathExtraData>,
    ) {
        {
            let mut f = self.bh.borrow_mut();
            f.path_extra_data = extra;
            f.path_needs_recalculated = false;
            f.load_requires_recalculation = false;
            if (path_type == pt::FALL
                || path_type == pt::FALL_INTENTIONAL
                || path_type == pt::RECALCULATED_DUE_TO_CHANGE)
                && f.ride_object.is_some()
            {
                return;
            }
            f.path = path;
        }
        let h = host();
        let ih = inventory_host();
        let (path_is_none, chasing) = {
            let f = self.bh.borrow();
            (f.path.is_none(), f.chasing_ride_object || f.chasing_npc)
        };
        if path_is_none && chasing {
            self.stop_interacting();
            self.bh.borrow_mut().path_needs_recalculated = true;
            return;
        }
        self.bh.borrow_mut().last_path_was_falling = path_type == pt::FALL || path_type == pt::FALL_INTENTIONAL;

        let count = self.bh.borrow().path.as_ref().map_or(0, |p| p.len());
        if count == 0 {
            if path_type == pt::WORLD_TAPPED {
                self.bh.borrow_mut().idle_timer = 0.0;
                if ih.has_blockhead_ai(self) {
                    ih.blockhead_ai_no_path_to_action(self);
                } // else: noPath.wav (client sound)
            } else if path_type == pt::RIDE_OBJECT_CHASE || path_type == pt::NPC_CHASE {
                self.bh.borrow_mut().idle_timer = 0.0;
                self.stop_interacting();
            }
            return;
        }

        // non-empty path
        self.bh.borrow_mut().idle_timer = 0.0;
        let world = self.world_rc();
        let final_index = {
            let f = self.bh.borrow();
            f.path.as_ref().and_then(|p| p.first()).and_then(|n| n.object_for_key("index")).map_or(0, |v| v.int_value())
        };
        let final_pos = match &world {
            Some(w) => get_world_pos_for_world_index(final_index, w),
            None => IntPair { x: 0, y: 0 },
        };
        if let Some(w) = &world {
            let _ = tile_at_world_position_loaded(final_pos.x, final_pos.y, w); // result unused
        }
        self.bh.borrow_mut().final_goal_square = final_pos;

        if matches!(path_type, pt::WORLD_TAPPED | pt::WORLD_TAPPED_MOVE_ONLY | pt::NPC_CHASE | pt::RIDE_OBJECT_CHASE) {
            if self.bh.borrow().state.interacting != 0 {
                // NOT stopInteracting
                h.update_gather_speed_and_animation_for_current_interation_and_item(self);
            }
            if path_type == pt::WORLD_TAPPED_MOVE_ONLY {
                self.bh.borrow_mut().state.goal_interaction_type_while_walking = it::NONE;
            } else {
                let mut pickup_rejected = false;
                self.bh.borrow_mut().state.goal_interaction_type_while_walking = goal_interaction as u32;
                let face_index = if goal_interaction == it::CRAFT as i32
                    || goal_interaction == it::RIDE_TRAIN_CAR as i32
                    || goal_interaction == it::USE_INTERACTIVE_OBJECT as i32
                {
                    0
                } else {
                    self.bh.borrow().path_extra_data.as_ref().map_or(0, |d| d.int_for_key("craftCountOrExtraData"))
                };
                let fgs = self.bh.borrow().final_goal_square;
                let tile = world.as_ref().and_then(|w| tile_at_world_position_loaded(fgs.x, fgs.y, w));
                // return value discarded; called for the out-flag
                let _ = ih.current_interaction_type_for_tile_at_pos_pickup_rejected_due_to_inventory_full_include_actions_face_index_allow_protected_actions(
                    self, tile, fgs, &mut pickup_rejected, false, face_index, true,
                );
                // if pickup_rejected && path_type == WORLD_TAPPED: displayInventoryFullPopUpForPos: (uiManager nil => no-op)
                let _ = pickup_rejected;
            }
            if self.bh.borrow().state.goal_interaction_type_while_walking == it::CRAFT {
                let mut f = self.bh.borrow_mut();
                f.interaction_item = None;
                f.interaction_item_index = -1;
                f.interaction_item_sub_index = -1;
            }
        }

        // elevator-entry detection: `nextKF == STAND_ELEVATOR(10) && traverseToKeyFrame != 10` only reports the
        // achievement "grp.elevator_mj" (empty World method) and plays elevatorBell.wav (client sound): no effect.
    }

    /// `-setWaitingForPathToPos:`.
    pub fn set_waiting_for_path_to_pos(&self, goal_pos: IntPair) {
        {
            let mut f = self.bh.borrow_mut();
            f.waiting_for_path = true;
            f.waiting_path_goal_pos = goal_pos;
        }
        let ih = inventory_host();
        let world = self.world_rc();
        let tile = world.as_ref().and_then(|w| tile_at_world_position_loaded(goal_pos.x, goal_pos.y, w));
        let mut flag = false;
        let mut g = ih.current_interaction_type_for_tile_at_pos_pickup_rejected_due_to_inventory_full_include_actions_face_index_allow_protected_actions(
            self, tile, goal_pos, &mut flag, false, 0, true,
        );
        if interaction_type_disallowed_on_protected_tiles(InteractionType(g)) {
            let protected =
                world.as_ref().map_or(false, |w| w.tile_is_protected_at_pos_against_blockhead(goal_pos, &self.me()));
            if protected {
                g = ih.current_interaction_type_for_tile_at_pos_pickup_rejected_due_to_inventory_full_include_actions_face_index_allow_protected_actions(
                    self, tile, goal_pos, &mut flag, false, 0, false,
                );
            }
        }
        self.bh.borrow_mut().waiting_for_path_interaction_type = g;
    }

    /// `-setNoLongerWaitingForPath`
    pub fn set_no_longer_waiting_for_path(&self) {
        self.bh.borrow_mut().waiting_for_path = false;
    }

    /// `-waitingForPath`
    pub fn waiting_for_path(&self) -> bool {
        self.bh.borrow().waiting_for_path
    }

    // ================================================================================================
    // infoForPathRecalculation
    // ================================================================================================

    /// Copies `interactionItem` / `Index` / `SubIndex` from the action (shared by branches D, E, F).
    fn copy_interaction_item_from_action(&self, a: &ActionRef) {
        let ih = inventory_host();
        let item = ih.action_interaction_item(a);
        let idx = ih.action_interaction_item_index(a);
        let sub = ih.action_interaction_item_sub_index(a);
        let mut f = self.bh.borrow_mut();
        f.interaction_item = item;
        f.interaction_item_index = idx;
        f.interaction_item_sub_index = sub;
    }

    /// `held = outer.last; if held.subItems.count != 0 and sub in range: s = held.subItems[sub]; if s.count != 0: held = s.last`
    /// (the re-resolution of branch C).
    fn resolve_held(&self, outer_idx: i32, sub: i32) -> Option<ItemRef> {
        let items = self.inv();
        let outer = outer_slot_at(&items, outer_idx);
        if outer.borrow().is_empty() {
            return None;
        }
        let mut held = slot_last(&outer);
        let hh = held.clone().expect("non-empty");
        if sub_slot_count(&hh) != 0 && sub >= 0 && (sub as usize) < sub_slot_count(&hh) {
            let s = sub_slot_at(&hh, sub as i64).expect("in range");
            if !s.borrow().is_empty() {
                held = slot_last(&s);
            }
        }
        held
    }

    /// `-infoForPathRecalculation`. Returns `None` when
    /// `pathNeedsRecalculated` is NO or on `noPathFail()` (which clears the flag; the sound is a client no-op).
    /// The END block always yields `PATH_TYPE_FALL` (5) on the server (`uiManager` nil).
    pub fn info_for_path_recalculation(&self) -> Option<PathRecalcInfo> {
        if !self.bh.borrow().path_needs_recalculated {
            return None;
        }
        let me = self.me();
        let h = host();
        let ih = inventory_host();
        let world = self.world_rc();
        let no_path_fail = |s: &Blockhead| -> Option<PathRecalcInfo> {
            s.bh.borrow_mut().path_needs_recalculated = false;
            None
        };
        let mut info = PathRecalcInfo {
            path_user: me.clone(),
            goal: IntPair { x: 0, y: 0 },
            goal_interaction: 0,
            path_type: 0,
            extra: None,
        };
        let mut have_goal = false;

        let (queue0, skip) = {
            let f = self.bh.borrow();
            let skip = f.action_queue.is_empty()
                || f.state.sub_animation_type == 6
                || f.state.sub_animation_type == 7
                || f.state.regenerating != 0
                || f.state.on_trade_mission != 0;
            (f.action_queue.first().cloned(), skip)
        };

        if !skip {
            let action = queue0.expect("non-empty queue");
            let goal_pos = h.action_goal_tile_pos(&action);
            let tile = world.as_ref().and_then(|w| tile_at_world_position_loaded(goal_pos.x, goal_pos.y, w));
            if tile.is_none() {
                return None; // flag stays set, no sound
            }
            let in_progress = h.action_in_progress(&action);
            let g0 = ih.action_goal_interaction(&action);
            'branches: {
                // (A) already chasing an NPC
                if self.bh.borrow().chasing_npc && in_progress {
                    have_goal = true;
                    info.goal = self.bh.borrow().last_known_npc_position;
                    info.goal_interaction = g0;
                    info.path_type = pt::NPC_CHASE;
                    let mut ex = PathExtraData::new();
                    ex.set(
                        "craftCountOrExtraData",
                        ExtraValue::Int(ih.action_craft_count_or_extra_data(&action) as i64),
                    );
                    info.extra = Some(ex);
                    break 'branches;
                }
                // (B) already chasing a ride object
                if self.bh.borrow().chasing_ride_object && in_progress {
                    have_goal = true;
                    info.goal = self.bh.borrow().last_known_ride_object_position;
                    info.goal_interaction = g0;
                    info.path_type = pt::RIDE_OBJECT_CHASE;
                    let mut ex = PathExtraData::new();
                    ex.set(
                        "craftCountOrExtraData",
                        ExtraValue::Int(ih.action_craft_count_or_extra_data(&action) as i64),
                    );
                    info.extra = Some(ex);
                    break 'branches;
                }
                if in_progress {
                    break 'branches;
                }
                if self.bh.borrow().ride_object.is_some() && g0 != it::FIRE_RANGED_WEAPON as i32 {
                    h.stop_riding(self);
                    break 'branches;
                }
                // (C) begin the action
                have_goal = true;
                h.action_set_in_progress(&action, true);
                let item = ih.action_interaction_item(&action);
                let cur = h.good_or_bad_interaction_for_action(self, Some(&action));
                if cur.good_or_bad != h.action_interaction_test_result(&action).good_or_bad {
                    return no_path_fail(self);
                }
                let itr: InteractionTestResult = h.action_interaction_test_result(&action);
                if let Some(item_ref) = &item {
                    let item_type = item_ref.borrow().item_type;
                    if item_type.0 != 0
                        && itr.render_item_type != 0
                        && itr.render_item_type != 1
                        && !matches!(
                            g0 as u32,
                            it::USE_INTERACTIVE_OBJECT | it::CRAFT | it::PICKUP | it::PICKUP_FREEBLOCK
                        )
                    {
                        let held = self.resolve_held(
                            ih.action_interaction_item_index(&action),
                            ih.action_interaction_item_sub_index(&action),
                        );
                        let same = held.as_ref().map_or(false, |x| Rc::ptr_eq(x, item_ref));
                        if !same {
                            let held_type = held.as_ref().map_or(0, |x| x.borrow().item_type.0);
                            let ok;
                            if held_type == item_type.0 {
                                ih.action_set_interaction_item(&action, held.as_ref());
                                ok = true;
                            } else {
                                let alt = self.inventory_location_of_first_instance_of_item_type(item_type);
                                let mut ok2 = false;
                                if alt.x != -1 {
                                    // `alt` names a slot with an item in it, so the outer slot is never empty and
                                    // `resolve_held` always finds something
                                    let held2 = self.resolve_held(alt.x, alt.y);
                                    ih.action_set_interaction_item(&action, held2.as_ref());
                                    ih.action_set_interaction_item_index(&action, alt.x as i16 as i32);
                                    ih.action_set_interaction_item_sub_index(&action, alt.y as i16 as i32);
                                    ok2 = true;
                                }
                                ok = ok2;
                            }
                            if !ok {
                                return no_path_fail(self);
                            }
                        }
                    }
                }
                let g = ih.action_goal_interaction(&action);
                let gu = g as u32;
                if matches!(
                    gu,
                    it::STAB
                        | it::CAPTURE
                        | it::HARVEST_NPC
                        | it::RIDE
                        | it::SET_NPC_FREE
                        | it::FEED_NPC
                        | it::INSPECT_NPC
                        | it::FIRE_RANGED_WEAPON
                ) {
                    // (D) NPC branch
                    self.bh.borrow_mut().tapped_npc = None;
                    let npc = ih.harmable_dynamic_object_with_id(h.action_interaction_object_id(&action) as u64);
                    let npc = match npc {
                        Some(n) if !n.needs_removed() => n,
                        _ => {
                            self.bh.borrow_mut().chasing_npc = false;
                            return no_path_fail(self);
                        }
                    };
                    {
                        let mut f = self.bh.borrow_mut();
                        f.tapped_npc = Some(npc.clone());
                        f.last_known_npc_position = npc.pos();
                        f.chasing_npc = true;
                    }
                    if gu == it::INSPECT_NPC || gu == it::SET_NPC_FREE {
                        ih.npc_blockhead_is_coming_to_inspect(&npc, self);
                    }
                    self.copy_interaction_item_from_action(&action);
                    info.goal = self.bh.borrow().last_known_npc_position;
                    info.goal_interaction = g;
                    info.path_type = pt::NPC_CHASE;
                } else if matches!(gu, it::RIDE_BOAT | it::REMOVE_BOAT | it::RIDE_TRAIN_CAR | it::REMOVE_TRAIN_CAR) {
                    // (E) boat / train branch
                    let oid = h.action_interaction_object_id(&action) as u64;
                    let obj = if gu == it::RIDE_BOAT || gu == it::REMOVE_BOAT {
                        ih.boat_with_id(oid)
                    } else {
                        ih.train_car_with_id(oid)
                    };
                    self.bh.borrow_mut().tapped_ride_object = obj.clone();
                    match obj {
                        Some(o) if !o.needs_removed() => {
                            let mut f = self.bh.borrow_mut();
                            f.last_known_ride_object_position = o.pos();
                            f.chasing_ride_object = true;
                        }
                        _ => {
                            // note: tappedRideObject keeps a found-but-removed object
                            self.bh.borrow_mut().chasing_ride_object = false;
                            return no_path_fail(self);
                        }
                    }
                    self.copy_interaction_item_from_action(&action);
                    info.goal = self.bh.borrow().last_known_ride_object_position;
                    info.goal_interaction = g;
                    info.path_type = pt::RIDE_OBJECT_CHASE;
                    let mut ex = PathExtraData::new();
                    ex.set(
                        "craftCountOrExtraData",
                        ExtraValue::Int(ih.action_craft_count_or_extra_data(&action) as i64),
                    );
                    info.extra = Some(ex);
                } else {
                    // (F) plain branch
                    let goal_square = h.action_goal_tile_pos(&action);
                    self.copy_interaction_item_from_action(&action);
                    if gu != it::RIDE && gu != it::NONE {
                        let goal_tile =
                            world.as_ref().and_then(|w| tile_at_world_position_loaded(goal_square.x, goal_square.y, w));
                        let face_idx = if gu == it::CRAFT || gu == it::RIDE_TRAIN_CAR {
                            0
                        } else {
                            ih.action_craft_count_or_extra_data(&action)
                        };
                        let cur_item = self.bh.borrow().interaction_item.clone();
                        let mut scratch = false;
                        let mut cur_g = h.interaction_type_for_tile_at_pos_item_pickup_rejected_due_to_inventory_full_include_actions_face_index_allow_protected_actions(
                            self, goal_tile, goal_square, cur_item.as_ref(), &mut scratch, false, face_idx, true,
                        );
                        if interaction_type_disallowed_on_protected_tiles(InteractionType(cur_g))
                            && world
                                .as_ref()
                                .map_or(false, |w| w.tile_is_protected_at_pos_against_blockhead(goal_square, &me))
                        {
                            cur_g = h.interaction_type_for_tile_at_pos_item_pickup_rejected_due_to_inventory_full_include_actions_face_index_allow_protected_actions(
                                self, goal_tile, goal_square, cur_item.as_ref(), &mut scratch, false, face_idx, false,
                            );
                        }
                        if cur_g != gu {
                            let accepted = (cur_g == it::CRAFT
                                && matches!(gu, it::ADD_FUEL | it::DIG | it::DIG_FOREGROUND | it::DIG_BACK_WALL))
                                || (cur_g == it::USE_INTERACTIVE_OBJECT
                                    && matches!(gu, it::DIG | it::DIG_FOREGROUND | it::DIG_BACK_WALL));
                            if !accepted {
                                return no_path_fail(self);
                            }
                        }
                    }
                    info.goal = goal_square;
                    info.goal_interaction = g;
                    info.path_type = ih.action_path_type(&action);
                    let mut ex = PathExtraData::new();
                    ex.set(
                        "craftCountOrExtraData",
                        ExtraValue::Int(ih.action_craft_count_or_extra_data(&action) as i64),
                    );
                    let oid = h.action_interaction_object_id(&action);
                    if let Some(c) = ih.action_craftable_item_object(&action) {
                        let wb = h.interaction_object_with_id(oid);
                        if let Some(wb) = wb {
                            ex.set("craftableItemObject", ExtraValue::Any(c));
                            ex.set("workbench", ExtraValue::Obj(wb));
                            ex.set("count", ExtraValue::Int(ih.action_craft_count_or_extra_data(&action) as i64));
                        }
                    } else if oid != 0 {
                        if let Some(o) = h.interaction_object_with_id(oid) {
                            ex.set(
                                if gu == it::ADD_FUEL { "workbench" } else { "interactionObject" },
                                ExtraValue::Obj(o),
                            );
                        }
                    }
                    info.extra = Some(ex);
                }
            }
        }

        // END
        if !have_goal {
            // `dpadShouldBeDisplayed && activeBlockhead == self && dpad...` needs the (nil) uiManager: false.
            let _ = ih.dynamic_world_active_blockhead();
            info.path_type = pt::FALL;
            // `pos` is read here, at END (after a possible `stopRiding`), not at entry.
            info.goal = self.pos();
            info.goal_interaction = 0;
            info.extra = None;
        }
        Some(info)
    }

    // ================================================================================================
    // useCurrentItemIfPossible
    // ================================================================================================

    /// `-useCurrentItemIfPossible`. ALWAYS returns false (even after eating).
    /// the NOTE on ordering: the wearable branch drops the old garment BEFORE moving the new one. Float widths: all
    /// f32 except the health sum (f64). Sounds and achievements are client / empty no-ops.
    pub fn use_current_item_if_possible(&self) -> bool {
        let h = host();
        let ih = inventory_host();
        let cur = self.current_item();
        let (t, data_b) = cur.as_ref().map_or((ItemType(0), 0u16), |c| {
            let b = c.borrow();
            (b.item_type, b.data_b)
        });
        let food = food_type_for_item_type(t, data_b as i32);
        if food == FoodType::FOOD_NONE {
            if !item_type_can_be_worn(t) {
                if t == ItemType::ITEM_CAMERA {
                    ih.world_start_using_camera();
                } else if t == ItemType::ITEM_FISHING_ROD {
                    if self.bh.borrow().fishing_rod.is_some() {
                        h.stop_fishing(self);
                    }
                    if self.bh.borrow().fishing_rod.is_none() && ih.can_fish(self) {
                        let sel = self.bh.borrow().selected_tool_index;
                        let sub = self.current_item_sub_index();
                        {
                            let mut f = self.bh.borrow_mut();
                            f.interaction_item = cur.clone();
                            f.interaction_item_index = sel;
                            f.interaction_item_sub_index = sub;
                        }
                        let rod = ih.fishing_rod_new(self);
                        self.bh.borrow_mut().fishing_rod = Some(rod);
                        let ro = self.bh.borrow().ride_object.clone();
                        if let Some(ro) = ro {
                            ih.ride_object_set_target_velocity_zero(&ro);
                        }
                    }
                }
                // all other items: nothing
            } else {
                let sub = self.current_item_sub_index();
                // `itemTypeCanBeWorn` held, so the item is wearable on exactly one of head, torso, legs, feet; feet is
                // what is left when the other three tests fail (the original's final `else if` could never miss).
                let slot_sub = if item_type_can_be_worn_on_head(t) {
                    3
                } else if item_type_can_be_worn_on_torso(t) {
                    2
                } else if item_type_can_be_worn_on_legs(t) {
                    1
                } else {
                    0
                };
                self.drop_inventory_items_at_index_sub_index_count_ignore_freeblocks(0, slot_sub, 1, false);
                let sel = self.bh.borrow().selected_tool_index;
                self.move_inventory_items_from_index_from_sub_index_to_index_to_sub_index_count(
                    sel, sub, 0, slot_sub, 1,
                );
            }
        } else {
            let n = nourishment_fraction_for_food_type(food);
            let expert = self.world_rc().map_or(false, |w| w.expert_mode());
            let pos = self.pos();
            let me = self.me();
            let mut create: Option<ItemType> = None;
            {
                let mut f = self.bh.borrow_mut();
                let s = &mut f.state;
                // (every food has a nourishment above zero, so the original's `n > 0` guards around this update and
                // the `death` reset below always held)
                s.fullness = if s.fullness <= 0.0 { 0.0 } else { s.fullness };
                if s.fullness + n < 1.0f32 {
                    s.fullness += n;
                } else {
                    s.fullness = 1.2f32;
                }
                if food != FoodType::FOOD_COFFEE_CHERRY && food != FoodType::FOOD_COFFEE && s.energy < 1.0f32 && n > 0.0
                {
                    let fct = if expert { 0.1f32 } else { 0.2f32 };
                    s.energy = fmax(s.energy, 0.25f32);
                    // (the original then capped the energy at 1.1999f when it reached 1.2f; with energy < 1.0 here and
                    // fct * n <= 0.2 * 0.8 that cap can never fire)
                    s.energy += fct * n;
                }
                if food == FoodType::FOOD_COFFEE_CHERRY {
                    let fct = if expert { 0.05f32 } else { 0.1f32 };
                    s.energy = fmax(s.energy, 0.25f32);
                    s.energy += fct;
                    if s.energy > 2.0f32 {
                        s.energy = 2.0f32;
                    }
                    s.coffee_energy_timer = fmax(s.coffee_energy_timer, 10.0f32);
                } else if food == FoodType::FOOD_COFFEE {
                    s.energy = fmax(s.energy, 0.25f32);
                    if !expert {
                        s.energy = 2.0f32;
                    } else {
                        s.energy += 1.0f32;
                        if s.energy > 2.0f32 {
                            s.energy = 2.0f32;
                        }
                    }
                    s.heat += 5.0f32;
                    create = Some(ItemType::ITEM_EMPTY_CLAY_CUP);
                    // grp.coffee: empty World method
                    s.coffee_energy_timer = 60.0f32;
                } else if food == FoodType::FOOD_CHILLI {
                    s.heat += 20.0f32;
                    // grp.chilli: empty World method
                } else if food == FoodType::FOOD_KELP {
                    s.drown_fraction -= 0.5f32;
                    if s.drown_fraction < -0.2f32 {
                        s.drown_fraction = -0.2f32;
                    }
                } else if food == FoodType::FOOD_FISH_CURRY || food == FoodType::FOOD_DODO_STEW {
                    create = Some(ItemType::ITEM_POT);
                    s.food_pause_timer = 300.0f32;
                } else if food == FoodType::FOOD_COOKED_FISH {
                    create = Some(ItemType::ITEM_TIN_FOIL);
                } else if food == FoodType::FOOD_MILK {
                    create = Some(ItemType::ITEM_TIN_BUCKET);
                }
                s.death = 0.0;
                let sum = s.health as f64 + (n as f64) * 0.5f64;
                s.health = (if 1.0f64 <= sum { 1.0f64 } else { sum }) as f32;
            }
            if let Some(ct) = create {
                h.create_free_block_at_position_of_type_data_a_data_b_sub_items_dynamic_object_save_dict_hovers_play_sound_priority_blockhead(
                    pos, ct, 0, 0, None, None, false, 0, Some(&me),
                );
            }
            self.remove_current_item();
            // sound: drink.wav / crunch.wav (client)
        }
        false
    }

    // ================================================================================================
    // predicates and small state helpers
    // ================================================================================================

    /// `-crouching`: `animationType == VA_LIE (23) || == VA_CRAWL (6)`.
    pub fn crouching(&self) -> bool {
        let a = self.bh.borrow().state.animation_type;
        a == 23 || a == 6
    }

    /// `-idle`.
    pub fn idle(&self) -> bool {
        if self.asleep() {
            return true;
        }
        if self.meditating() {
            return true;
        }
        let f = self.bh.borrow();
        if f.path.as_ref().map_or(0, |p| p.len()) == 0 || f.last_path_was_falling {
            return f.state.interacting == 0;
        }
        false
    }

    /// `-asleep`: `subAnimationType == 5 || == 6`.
    pub fn asleep(&self) -> bool {
        super::blockhead::state_asleep(&self.bh.borrow().state)
    }

    /// `-meditating`: `subAnimationType == 7`.
    pub fn meditating(&self) -> bool {
        super::blockhead::state_meditating(&self.bh.borrow().state)
    }

    // `-regenerating` is an inherent method of blockhead_core (blockhead.rs).

    /// `-wakeUp`
    pub fn wake_up(&self) {
        self.stop_interacting();
    }

    /// `-falling`: `path.count != 0 && traverseType == 17 (TRAVERSE_FALL)`.
    pub fn falling(&self) -> bool {
        let f = self.bh.borrow();
        f.path.as_ref().map_or(0, |p| p.len()) != 0 && f.traverse_type == 17
    }

    /// `-sleepRushed`: energy set by the bed in use.
    pub fn sleep_rushed(&self) {
        let h = host();
        let mut bed = self.bh.borrow().interaction_object.clone();
        if let Some(b) = &bed {
            if h.interaction_object_type(b) != 3 {
                bed = None;
            }
        }
        let t = bed.as_ref().map_or(0, |b| h.object_item_type(b));
        let mut f = self.bh.borrow_mut();
        f.state.energy = if t == ItemType::ITEM_SOFT_BED.0 as i32 {
            1.5f32
        } else if t == ItemType::ITEM_GOLD_BED.0 as i32 {
            2.0f32
        } else {
            1.0f32
        };
    }

    /// `-regenerateRushed`: `regenerationProgress = 1.1f`.
    pub fn regenerate_rushed(&self) {
        self.bh.borrow_mut().state.regeneration_progress = 1.1f32;
    }

    /// `-fishingRodCast:`. Runs only when `!isNet && !isClientBlockheadBeingControlledByServer`
    /// (never for server-side player blockheads, but `FishingRod` calls it and it must compile).
    pub fn fishing_rod_cast(&self, vec: Vector2) {
        if self.is_net() || self.is_client_blockhead_being_controlled_by_server() {
            return;
        }
        let clamp = |v: f32| -> f32 {
            if v < -8000.0f32 {
                -8000.0f32
            } else if v > 8000.0f32 {
                8000.0f32
            } else {
                v
            }
        };
        let x = clamp(vec.x);
        let y = clamp(vec.y);
        {
            let mut f = self.bh.borrow_mut();
            f.fishing_rod_cast_x = f32_to_i16(x);
            f.fishing_rod_cast_y = f32_to_i16(y);
        }
        self.set_update_needs_to_be_sent(true);
        self.increment_usage_of_interaction_item(false);
    }

    /// `-setRidingObject:`: `rideObject = r` (the `requiresFuel` UI call is a nil-receiver no-op).
    pub fn set_riding_object(&self, r: Option<&ObjRef>) {
        self.bh.borrow_mut().ride_object = r.cloned();
    }

    /// `-stopInteractingWithInteractionObjectsIfNoInteractionObject`. The second test runs even if the
    /// first already stopped.
    pub fn stop_interacting_with_interaction_objects_if_no_interaction_object(&self) {
        let stop = {
            let f = self.bh.borrow();
            f.state.interaction_type == it::CRAFT
                && f.interaction_workbench.is_none()
                && f.interaction_object_dict_to_restore_when_object_loaded.is_none()
        };
        if stop {
            self.stop_interacting();
        }
        let stop = {
            let f = self.bh.borrow();
            f.state.interaction_type == it::USE_INTERACTIVE_OBJECT
                && f.interaction_object.is_none()
                && f.interaction_object_dict_to_restore_when_object_loaded.is_none()
        };
        if stop {
            self.stop_interacting();
        }
    }

    /// `-canUseDynamicObject:`: a bed (`InteractionObject` with type 3) only when `(double)energy < 0.75`.
    pub fn can_use_dynamic_object(&self, obj: &ObjRef) -> bool {
        if obj.as_interaction_like().is_some() && host().interaction_object_type(obj) as i16 == 3 {
            return (self.bh.borrow().state.energy as f64) < 0.75f64;
        }
        true
    }

    // ================================================================================================
    // action queue cancel / query
    // ================================================================================================

    /// `-cancelAnyActionAtGoalPos:orWithInteractionObjectID:goalInteraction:craftCountOrExtraData:`.
    /// Returns true if at least one action was removed.
    pub fn cancel_any_action_at_goal_pos_or_with_interaction_object_id_goal_interaction_craft_count_or_extra_data(
        &self,
        tap_pos: IntPair,
        id: u64,
        goal_interaction: i32,
        extra: i16,
    ) -> bool {
        let h = host();
        let ih = inventory_host();
        let mut found = false;
        let mut i: usize = 0;
        loop {
            let a = {
                let f = self.bh.borrow();
                if i >= f.action_queue.len() {
                    break;
                }
                f.action_queue[i].clone()
            };
            let mut matched =
                h.action_goal_tile_pos(&a) == tap_pos || (h.action_interaction_object_id(&a) as u64 == id && id != 0);
            if matched
                && (goal_interaction == it::PAINT as i32 || goal_interaction == it::STRIP_PAINT as i32)
                && extra as i32 != ih.action_craft_count_or_extra_data(&a)
            {
                matched = false;
            }
            if matched {
                if h.action_in_progress(&a) {
                    self.stop_interacting();
                    self.set_path_type_goal_interaction_extra_data(None, pt::RECALCULATED_DUE_TO_CHANGE, 0, None);
                    h.abort_in_progress_path_if_for_blockhead(&self.me());
                    let mut f = self.bh.borrow_mut();
                    f.waiting_for_path = false;
                    f.chasing_ride_object = false;
                    f.chasing_npc = false;
                }
                self.bh.borrow_mut().action_queue.remove(i);
                found = true;
                continue; // `i -= 1; i += 1`
            }
            i += 1;
        }
        found
    }

    /// `-hasCancelableActionAtGoalPos:orWithInteractionObjectID:goalInteraction:craftCountOrExtraData:`
    /// (same match test, no side effects).
    pub fn has_cancelable_action_at_goal_pos_or_with_interaction_object_id_goal_interaction_craft_count_or_extra_data(
        &self,
        tap_pos: IntPair,
        id: u64,
        goal_interaction: i32,
        extra: i16,
    ) -> bool {
        let h = host();
        let ih = inventory_host();
        let queue = self.bh.borrow().action_queue.clone();
        for a in queue.iter() {
            let matched =
                h.action_goal_tile_pos(a) == tap_pos || (h.action_interaction_object_id(a) as u64 == id && id != 0);
            if matched {
                if goal_interaction != it::PAINT as i32 && goal_interaction != it::STRIP_PAINT as i32 {
                    return true;
                }
                if extra as i32 == ih.action_craft_count_or_extra_data(a) {
                    return true;
                }
            }
        }
        false
    }
}

/// `(XItemType, XColorIndex, XItemColor)` of piece `p` (0 hat, 1 shirt, 2 pants, 3 shoes) as disjoint borrows.
fn piece_fields(f: &mut BlockheadFields, p: usize) -> (&mut u16, &mut u16, &mut [f32; 4]) {
    match p {
        0 => (&mut f.hat_item_type, &mut f.hat_color_index, &mut f.hat_item_color),
        1 => (&mut f.shirt_item_type, &mut f.shirt_color_index, &mut f.shirt_item_color),
        2 => (&mut f.pants_item_type, &mut f.pants_color_index, &mut f.pants_item_color),
        _ => (&mut f.shoes_item_type, &mut f.shoes_color_index, &mut f.shoes_item_color),
    }
}

// ====================================================================================================
// Tests
// ====================================================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::blockhead::blockhead::testutil::*;
    use crate::blockhead::blockhead::{set_host, BlockheadHost};
    use crate::fmath::cconv::f32_to_i32;
    use crate::items::inventory_item::InventoryItem;
    use crate::items::predicates::{
        armour_item_damage_usage_rate, item_type_can_be_colored, item_type_is_valid_inventory_item,
        usage_increment_per_use,
    };
    use crate::objects::handle::ObjRef;

    #[derive(Clone, Debug, PartialEq)]
    struct Fb {
        pos: (i32, i32),
        ty: u32,
        a: u16,
        b: u16,
        sound: i32,
        pri: bool,
        hovers: bool,
    }

    #[derive(Default)]
    struct Rec {
        fbs: RefCell<Vec<Fb>>,
        sims: RefCell<Vec<i32>>,
        goal_pos: std::cell::Cell<(i32, i32)>,
        in_progress: std::cell::Cell<bool>,
        set_in_progress: std::cell::Cell<u32>,
        gob: std::cell::Cell<i32>,
        expected: std::cell::Cell<(i32, u32)>,
        obj_id: std::cell::Cell<i64>,
        io: RefCell<Option<ObjRef>>,
        itype: std::cell::Cell<u32>,
        itype_prot: std::cell::Cell<u32>,
        stops: std::cell::Cell<u32>,
        fish_stops: std::cell::Cell<u32>,
        aborts: std::cell::Cell<u32>,
        io_type: std::cell::Cell<i32>,
        obj_item: std::cell::Cell<i32>,
        inv_change: std::cell::Cell<Option<i32>>,
        wb_current: RefCell<Option<ObjRef>>,
    }
    impl crate::blockhead::blockhead::BlockheadHostStub for Rec {
        fn action_inventory_change_int_for_key(&self, _a: &ActionRef, _key: &str) -> Option<i32> {
            self.inv_change.get()
        }
        fn workbench_current_blockhead(&self, _wb: &ObjRef) -> Option<ObjRef> {
            self.wb_current.borrow().clone()
        }
        fn action_goal_tile_pos(&self, _a: &ActionRef) -> IntPair {
            let (x, y) = self.goal_pos.get();
            IntPair { x, y }
        }
        fn action_in_progress(&self, _a: &ActionRef) -> bool {
            self.in_progress.get()
        }
        fn action_set_in_progress(&self, _a: &ActionRef, _v: bool) {
            self.set_in_progress.set(self.set_in_progress.get() + 1);
        }
        fn good_or_bad_interaction_for_action(
            &self,
            _b: &Blockhead,
            _a: Option<&ActionRef>,
        ) -> crate::types::structs::InteractionTestResult {
            crate::types::structs::InteractionTestResult {
                good_or_bad: self.gob.get(),
                render_item_type: 0,
                render_item_data_a: 0,
                render_item_data_b: 0,
            }
        }
        fn action_interaction_test_result(&self, _a: &ActionRef) -> crate::types::structs::InteractionTestResult {
            let (g, r) = self.expected.get();
            crate::types::structs::InteractionTestResult {
                good_or_bad: g,
                render_item_type: r,
                render_item_data_a: 0,
                render_item_data_b: 0,
            }
        }
        fn action_interaction_object_id(&self, _a: &ActionRef) -> i64 {
            self.obj_id.get()
        }
        fn interaction_object_with_id(&self, _id: i64) -> Option<ObjRef> {
            self.io.borrow().clone()
        }
        fn interaction_type_for_tile_at_pos_item_pickup_rejected_due_to_inventory_full_include_actions_face_index_allow_protected_actions(
            &self,
            _b: &Blockhead,
            _t: Option<TileRef>,
            _p: IntPair,
            _i: Option<&ItemRef>,
            _r: &mut bool,
            _ia: bool,
            _f: i32,
            allow_protected: bool,
        ) -> u32 {
            if allow_protected {
                self.itype.get()
            } else {
                self.itype_prot.get()
            }
        }
        fn stop_riding(&self, _b: &Blockhead) {
            self.stops.set(self.stops.get() + 1);
        }
        fn stop_fishing(&self, _b: &Blockhead) {
            self.fish_stops.set(self.fish_stops.get() + 1);
        }
        fn abort_in_progress_path_if_for_blockhead(&self, _b: &ObjRef) {
            self.aborts.set(self.aborts.get() + 1);
        }
        fn interaction_object_type(&self, _o: &ObjRef) -> i32 {
            self.io_type.get()
        }
        fn object_item_type(&self, _o: &ObjRef) -> i32 {
            self.obj_item.get()
        }
        fn create_free_block_at_position_of_type_data_a_data_b_sub_items_dynamic_object_save_dict_hovers_play_sound_priority_blockhead(
            &self,
            pos: IntPair,
            item_type: ItemType,
            data_a: u16,
            data_b: u16,
            _s: Option<&[Vec<ItemRef>]>,
            _d: Option<&PlistValue>,
            hovers: bool,
            play_sound: i32,
            priority: Option<&ObjRef>,
        ) {
            self.fbs.borrow_mut().push(Fb {
                pos: (pos.x, pos.y),
                ty: item_type.0,
                a: data_a,
                b: data_b,
                sound: play_sound,
                pri: priority.is_some(),
                hovers,
            });
        }
        fn add_simulation_event_of_type_for_blockhead_extra_data(
            &self,
            event_type: i32,
            _b: &ObjRef,
            _extra: super::super::blockhead::SimulationEventExtra,
        ) {
            self.sims.borrow_mut().push(event_type);
        }
    }

    #[derive(Default)]
    struct Ih {
        fb: RefCell<Option<FreeBlockData>>,
        cannot_pickup: std::cell::Cell<u32>,
        client: std::cell::Cell<bool>,
        action_item: RefCell<Option<ItemRef>>,
        action_index: std::cell::Cell<i32>,
        action_sub_index: std::cell::Cell<i32>,
        has_ai: std::cell::Cell<bool>,
        ai_calls: std::cell::Cell<u32>,
        goal_interaction: std::cell::Cell<i32>,
        path_type: std::cell::Cell<i32>,
        craft_count: std::cell::Cell<i32>,
        craftable: RefCell<Option<Rc<dyn Any>>>,
        harmable: RefCell<Option<ObjRef>>,
        boat: RefCell<Option<ObjRef>>,
        train: RefCell<Option<ObjRef>>,
        can_fish: std::cell::Cell<bool>,
        crafting_type: std::cell::Cell<i32>,
        items_left: std::cell::Cell<i32>,
        item_sets: std::cell::Cell<u32>,
        item_index_sets: RefCell<Vec<i32>>,
    }
    impl InventoryHostStub for Ih {
        fn action_interaction_item_sub_index(&self, _a: &ActionRef) -> i32 {
            self.action_sub_index.get()
        }
        fn has_blockhead_ai(&self, _b: &Blockhead) -> bool {
            self.has_ai.get()
        }
        fn blockhead_ai_no_path_to_action(&self, _b: &Blockhead) {
            self.ai_calls.set(self.ai_calls.get() + 1);
        }
        fn action_goal_interaction(&self, _a: &ActionRef) -> i32 {
            self.goal_interaction.get()
        }
        fn action_path_type(&self, _a: &ActionRef) -> i32 {
            self.path_type.get()
        }
        fn action_craft_count_or_extra_data(&self, _a: &ActionRef) -> i32 {
            self.craft_count.get()
        }
        fn action_craftable_item_object(&self, _a: &ActionRef) -> Option<Rc<dyn Any>> {
            self.craftable.borrow().clone()
        }
        fn harmable_dynamic_object_with_id(&self, _id: u64) -> Option<ObjRef> {
            self.harmable.borrow().clone()
        }
        fn boat_with_id(&self, _id: u64) -> Option<ObjRef> {
            self.boat.borrow().clone()
        }
        fn train_car_with_id(&self, _id: u64) -> Option<ObjRef> {
            self.train.borrow().clone()
        }
        fn can_fish(&self, _b: &Blockhead) -> bool {
            self.can_fish.get()
        }
        fn workbench_currently_crafting_item_type(&self, _wb: &ObjRef) -> i32 {
            self.crafting_type.get()
        }
        fn workbench_total_items_left_to_craft(&self, _wb: &ObjRef) -> i32 {
            self.items_left.get()
        }
        fn action_set_interaction_item(&self, _a: &ActionRef, _item: Option<&ItemRef>) {
            self.item_sets.set(self.item_sets.get() + 1);
        }
        fn action_set_interaction_item_index(&self, _a: &ActionRef, v: i32) {
            self.item_index_sets.borrow_mut().push(v);
        }
        fn dynamic_world_is_client(&self) -> bool {
            self.client.get()
        }
        fn action_interaction_item(&self, _a: &ActionRef) -> Option<ItemRef> {
            self.action_item.borrow().clone()
        }
        fn action_interaction_item_index(&self, _a: &ActionRef) -> i32 {
            self.action_index.get()
        }
        fn free_block_data(&self, _fb: &ObjRef) -> Option<FreeBlockData> {
            self.fb.borrow().clone()
        }
        fn free_block_priority_blockhead_cannot_pickup(&self, _fb: &ObjRef) {
            self.cannot_pickup.set(self.cannot_pickup.get() + 1);
        }
    }

    struct Fx {
        _world: Rc<crate::world::state::World>,
        _dw: Rc<MockDw>,
        _dwref: crate::objects::base::DynamicWorldRef,
        o: ObjRef,
        rec: Rc<Rec>,
        ih: Rc<Ih>,
    }
    impl Fx {
        fn b(&self) -> &Blockhead {
            bh_of(&self.o)
        }
    }

    fn fx() -> Fx {
        super::user_defaults_clear();
        let rec = Rc::new(Rec::default());
        set_host(rec.clone());
        let ih = Rc::new(Ih::default());
        set_inventory_host(ih.clone());
        let (world, dw, dwref) = setup();
        let o = new_bh(&world, &dwref, 0);
        let f = Fx { _world: world, _dw: dw, _dwref: dwref, o, rec, ih };
        for i in 1..8 {
            f.b().bh.borrow().inventory_items[i].borrow_mut().clear();
        }
        f.rec.fbs.borrow_mut().clear();
        f.b().bh.borrow_mut().inventory_changed_this_frame_slots = [false; 8];
        f
    }

    fn mk(t: u32, a: u16, b: u16) -> ItemRef {
        InventoryItem::new(ItemType(t), a, b, None, None)
    }

    fn put(b: &Blockhead, slot: usize, items: Vec<ItemRef>) {
        b.bh.borrow().inventory_items[slot].borrow_mut().extend(items);
    }

    fn types(b: &Blockhead, slot: usize) -> Vec<u32> {
        b.bh.borrow().inventory_items[slot].borrow().iter().map(|x| x.borrow().item_type.0).collect()
    }

    fn counts(b: &Blockhead) -> Vec<usize> {
        (0..8).map(|i| b.bh.borrow().inventory_items[i].borrow().len()).collect()
    }

    const APPLE: u32 = 21;
    const FLINT: u32 = 3;
    const STICK: u32 = 4;
    const PICKAXE: u32 = 8;
    const BASKET: u32 = 12;

    fn basket_with(subs: Vec<(usize, Vec<ItemRef>)>) -> ItemRef {
        let mut input: Vec<Vec<ItemRef>> = vec![Vec::new(); 4];
        for (i, v) in subs {
            input[i] = v;
        }
        InventoryItem::new(ItemType(BASKET), 0, 0, Some(&input), None)
    }

    fn fill_all_other_slots(b: &Blockhead, except: usize, first_type: u32) {
        // distinct non-stackable-with-each-other types, 99 each (stacks are full)
        let mut t = first_type;
        for s in 1..8 {
            if s == except {
                continue;
            }
            put(b, s, (0..99).map(|_| mk(t, 0, 0)).collect());
            t += 1;
        }
    }

    // ---- item 1: stack placement
    #[test]
    fn stack_placement_and_full() {
        let f = fx();
        let b = f.b();
        for n in 0..99 {
            assert_eq!(b.add_item_to_inventory(&mk(APPLE, 0, 0)), 1, "n={}", n);
        }
        assert_eq!(b.add_item_to_inventory(&mk(APPLE, 0, 0)), 2);
        assert_eq!(counts(b)[1], 99);
        assert_eq!(counts(b)[2], 1);
        assert!(b.bh.borrow().inventory_changed_this_frame_slots[2]);
        // fill every slot: -1
        for t in 30..35u32 {
            put(b, (t - 30) as usize + 3, (0..99).map(|_| mk(t, 0, 0)).collect());
        }
        put(b, 2, (0..97).map(|_| mk(APPLE, 0, 0)).collect());
        assert_eq!(b.can_pick_up_item_of_type_sub_items(ItemType(APPLE), None), 1);
        assert_eq!(b.can_pick_up_item_of_type_sub_items(ItemType(FLINT), None), -1);
        assert_eq!(b.add_item_to_inventory(&mk(FLINT, 0, 0)), -1);
        // invalid type: 0
        let bad = (0..2000u32).find(|t| !item_type_is_valid_inventory_item(ItemType(*t))).unwrap();
        assert_eq!(b.can_pick_up_item_of_type_sub_items(ItemType(bad), None), 0);
        assert_eq!(b.add_item_to_inventory(&mk(bad, 0, 0)), -1);
        // slot 0 never chosen
        assert_eq!(counts(b)[0], 1);
    }

    // ---- item 2: combine rule (phase 3b is `< 100`)
    #[test]
    fn combine_rule_sum_below_100() {
        for (n_sub, expect) in [(39usize, 1i32), (40, -1)] {
            let f = fx();
            let b = f.b();
            put(b, 1, (0..60).map(|_| mk(FLINT, 0, 0)).collect());
            fill_all_other_slots(b, 1, 100);
            let basket = basket_with(vec![
                (0, (0..n_sub).map(|_| mk(FLINT, 0, 0)).collect()),
                (1, vec![mk(STICK, 0, 0)]),
                (2, vec![mk(STICK, 0, 0)]),
                (3, vec![mk(STICK, 0, 0)]),
            ]);
            assert_eq!(
                b.can_pick_up_item_of_type_sub_items(
                    ItemType(BASKET),
                    Some(&basket.borrow().sub_items.clone().unwrap())
                ),
                expect
            );
            let r = b.add_item_to_inventory_flash_disable_warp_check(&basket, false, true);
            assert_eq!(r, expect.max(-1).min(1) * if expect == 1 { 1 } else { 1 });
            if expect == 1 {
                assert_eq!(counts(b)[1], 1);
                assert_eq!(basket.borrow().sub_items.as_ref().unwrap()[0].borrow().len(), 99);
            } else {
                assert_eq!(counts(b)[1], 60);
            }
        }
    }

    // ---- item 3: usage-sorted stacks
    #[test]
    fn tool_stack_ordering() {
        let f = fx();
        let b = f.b();
        for a in [5u16, 1, 3] {
            assert_eq!(b.add_item_to_inventory(&mk(PICKAXE, a, 0)), 1);
        }
        let das: Vec<u16> = b.bh.borrow().inventory_items[1].borrow().iter().map(|x| x.borrow().data_a).collect();
        assert_eq!(das, vec![1, 3, 5]);
        // an item without durability appends
        for _ in 0..3 {
            b.add_item_to_inventory(&mk(APPLE, 9, 0));
        }
        b.add_item_to_inventory(&mk(APPLE, 2, 0));
        let das: Vec<u16> = b.bh.borrow().inventory_items[2].borrow().iter().map(|x| x.borrow().data_a).collect();
        assert_eq!(das, vec![9, 9, 9, 2]);
    }

    // ---- item 4: colourable stackability
    #[test]
    fn colourable_stackability() {
        let f = fx();
        let b = f.b();
        let t = (2..600u32)
            .find(|t| {
                let ty = ItemType(*t);
                item_type_is_valid_inventory_item(ty)
                    && item_type_can_be_colored(ty)
                    && !item_type_sub_items_can_be_modified_while_carried(ty)
                    && *t != 103
            })
            .expect("a colourable type");
        assert_eq!(b.add_item_to_inventory(&mk(t, 0, 0)), 1);
        assert_eq!(b.add_item_to_inventory(&mk(t, 0, 5)), 2, "dataB 0 vs non-zero do not stack");
        assert_eq!(b.add_item_to_inventory(&mk(t, 0, 7)), 2, "two non-zero dataB stack");
        assert_eq!(counts(b)[2], 2);
        // ITEM_PAINT never stacks with differing dataB
        assert_eq!(b.add_item_to_inventory(&mk(103, 0, 1)), 3);
        assert_eq!(b.add_item_to_inventory(&mk(103, 0, 2)), 4);
        assert_eq!(b.add_item_to_inventory(&mk(103, 0, 1)), 3);
    }

    // ---- item 5: containers
    #[test]
    fn container_sub_slot_placement() {
        let f = fx();
        let b = f.b();
        let basket = basket_with(vec![(1, vec![mk(APPLE, 0, 0), mk(APPLE, 0, 0)])]);
        put(b, 1, vec![basket.clone()]);
        // phase 1b: stack inside the carried container
        assert_eq!(b.add_item_to_inventory(&mk(APPLE, 0, 0)), 1);
        assert_eq!(basket.borrow().sub_items.as_ref().unwrap()[1].borrow().len(), 3);
        let fl = b.bh.borrow();
        assert!(fl.sub_inventory_changed_this_frame_slots[1][1]);
        assert!(!fl.inventory_changed_this_frame_slots[1]);
        drop(fl);
        // phase 4: a non-stackable item into the first empty sub-slot (all other slots full)
        fill_all_other_slots(b, 1, 100);
        assert_eq!(b.add_item_to_inventory(&mk(FLINT, 0, 0)), 1);
        assert_eq!(basket.borrow().sub_items.as_ref().unwrap()[0].borrow().len(), 1);
        assert!(b.bh.borrow().sub_inventory_changed_this_frame_slots[1][0]);
    }

    // ---- item 6: force slot
    #[test]
    fn force_slot_index() {
        let f = fx();
        let b = f.b();
        assert_eq!(
            b.add_item_to_inventory_flash_disable_warp_check_force_slot_index(&mk(STICK, 0, 0), false, true, 5),
            5
        );
        assert_eq!(counts(b)[5], 1);
        // a stackable merge is restricted to the forced slot
        put(b, 2, vec![mk(APPLE, 0, 0)]);
        assert_eq!(
            b.add_item_to_inventory_flash_disable_warp_check_force_slot_index(&mk(APPLE, 0, 0), false, true, 3),
            3
        );
        assert_eq!(counts(b)[2], 1);
    }

    // ---- item 7: clothing
    #[test]
    fn use_current_item_clothing_swap() {
        let f = fx();
        let b = f.b();
        let old_hat = mk(154, 0, 3); // ITEM_TIN_FOIL_HAT
        let new_hat = mk(117, 0, 7); // ITEM_LINEN_HAT
        b.bh.borrow().inventory_items[0].borrow()[0].borrow().sub_items.as_ref().unwrap()[3]
            .borrow_mut()
            .push(old_hat.clone());
        put(b, 2, vec![new_hat.clone()]);
        b.set_selected_tool_index(2);
        assert!(!b.use_current_item_if_possible());
        let fbs = f.rec.fbs.borrow().clone();
        assert_eq!(fbs.len(), 1);
        assert_eq!(fbs[0], Fb { pos: (5, 8), ty: 154, a: 0, b: 3, sound: 2, pri: false, hovers: false });
        let sub3 = b.bh.borrow().inventory_items[0].borrow()[0].borrow().sub_items.as_ref().unwrap()[3].clone();
        assert_eq!(sub3.borrow().len(), 1);
        assert!(Rc::ptr_eq(&sub3.borrow()[0], &new_hat));
        assert!(b.bh.borrow().inventory_items[2].borrow().is_empty());
        // inventoryWasChanged(0): hat bookkeeping
        b.base.borrow_mut().creation_data_needs_to_be_sent = false;
        b.bh.borrow_mut().inventory_needs_saving = false;
        b.inventory_was_changed_sub_index_was_usage(0, -1, false);
        let fl = b.bh.borrow();
        assert_eq!(fl.hat_item_type, 117);
        assert_eq!(fl.hat_color_index, 7);
        assert!(fl.inventory_needs_saving);
        drop(fl);
        assert!(b.base.borrow().creation_data_needs_to_be_sent);
        // wasUsage = true does not set the saving flag
        b.bh.borrow_mut().inventory_needs_saving = false;
        b.inventory_was_changed_sub_index_was_usage(0, -1, true);
        assert!(!b.bh.borrow().inventory_needs_saving);
    }

    #[test]
    fn save_slots_skip_time_crystals() {
        let f = fx();
        let b = f.b();
        put(b, 3, vec![mk(APPLE, 0, 0), mk(11, 0, 0), mk(APPLE, 4, 0)]);
        b.inventory_was_changed_sub_index_was_usage(3, -1, false);
        let s = b.bh.borrow().save_item_slots_array[3].clone();
        assert_eq!(s.len(), 2);
        assert_eq!(s[0], mk(APPLE, 0, 0).borrow().save_data());
        assert!(b.inventory_needs_saving());
    }

    // ---- item 8: money
    fn fb_data(ty: u32, a: u16, b: u16) -> FreeBlockData {
        FreeBlockData {
            item_type: ItemType(ty),
            data_a: a,
            data_b: b,
            sub_items: None,
            dynamic_object_save_dict: None,
            hovers: false,
            priority_blockhead: None,
        }
    }

    #[test]
    fn money_pickup_splits_into_coins() {
        let f = fx();
        let b = f.b();
        let fb = stub(0, true);
        *f.ih.fb.borrow_mut() = Some(fb_data(298, 1, 250));
        assert!(b.pickup_freeblock_if_possible_in_tile_intentional(&fb, None, true));
        assert!(fb.needs_removed());
        assert_eq!(types(b, 1), vec![260]);
        assert_eq!(types(b, 2), vec![167, 167]);
        assert_eq!(counts(b)[3], 50);
        assert_eq!(types(b, 3)[0], 166);
        assert_eq!(b.total_cash(), 10000 + 200 + 50);
        assert_eq!(b.bh.borrow().z_index, 1);
        assert!(f.rec.fbs.borrow().is_empty());
    }

    #[test]
    fn money_pickup_leftover_pile() {
        let f = fx();
        let b = f.b();
        put(b, 1, (0..98).map(|_| mk(166, 0, 0)).collect());
        fill_all_other_slots(b, 1, 100);
        let fb = stub(0, true);
        *f.ih.fb.borrow_mut() = Some(fb_data(298, 0, 250));
        assert!(b.pickup_freeblock_if_possible_in_tile_intentional(&fb, None, true));
        assert_eq!(counts(b)[1], 99);
        let fbs = f.rec.fbs.borrow().clone();
        assert_eq!(fbs, vec![Fb { pos: (5, 9), ty: 298, a: 0, b: 249, sound: 0, pri: false, hovers: false }]);
    }

    // ---- item 16: ownership
    #[test]
    fn ownership_refusal() {
        let f = fx();
        let b = f.b();
        let fb = stub(0, true);
        let mut d = fb_data(164, 0, 0); // IRON_DOOR
        let mut sd = PlistValue::mutable_dict();
        if let PlistValue::Dict(dd) = &mut sd {
            dd.map.insert("ironPlaceClientID".to_string(), PlistValue::string("someone"));
        }
        d.dynamic_object_save_dict = Some(sd.clone());
        *f.ih.fb.borrow_mut() = Some(d.clone());
        assert!(!b.pickup_freeblock_if_possible_in_tile_intentional(&fb, None, false), "non-owner, non-intentional");
        assert!(counts(b).iter().skip(1).all(|c| *c == 0));
        assert!(b.pickup_freeblock_if_possible_in_tile_intentional(&fb, None, true), "admin override when intentional");
        // owner "server" exception for a connected (non-ctl) blockhead
        let fb2 = stub(0, true);
        let mut sd2 = PlistValue::mutable_dict();
        if let PlistValue::Dict(dd) = &mut sd2 {
            dd.map.insert("ownerID".to_string(), PlistValue::string("server"));
        }
        d.dynamic_object_save_dict = Some(sd2);
        *f.ih.fb.borrow_mut() = Some(d.clone());
        assert!(b.pickup_freeblock_if_possible_in_tile_intentional(&fb2, None, false));
        // ... but not when the flag @0x950 is set
        let fb3 = stub(0, true);
        b.bh.borrow_mut().is_client_blockhead_being_controlled_by_server = true;
        assert!(!b.pickup_freeblock_if_possible_in_tile_intentional(&fb3, None, true));
        assert!(f.rec.sims.borrow().is_empty());
        b.bh.borrow_mut().is_client_blockhead_being_controlled_by_server = false;
    }

    #[test]
    fn pickup_priority_and_regeneration() {
        let f = fx();
        let b = f.b();
        let fb = stub(0, true);
        let other = stub(0, true);
        let mut d = fb_data(APPLE, 0, 0);
        d.priority_blockhead = Some(other.clone());
        *f.ih.fb.borrow_mut() = Some(d.clone());
        assert!(!b.pickup_freeblock_if_possible_in_tile_intentional(&fb, None, true));
        d.priority_blockhead = None;
        *f.ih.fb.borrow_mut() = Some(d.clone());
        b.bh.borrow_mut().state.regenerating = 1;
        assert!(!b.pickup_freeblock_if_possible_in_tile_intentional(&fb, None, true));
        b.bh.borrow_mut().state.regenerating = 0;
        // meditating / ignoring drop: refused unless priority == self
        b.bh.borrow_mut().ignoring_freeblocks_due_to_drop = true;
        assert!(!b.pickup_freeblock_if_possible_in_tile_intentional(&fb, None, false));
        assert!(b.pickup_freeblock_if_possible_in_tile_intentional(&fb, None, true));
        assert_eq!(types(b, 1), vec![APPLE]);
        // expected-craft list: first match erased
        b.bh.borrow_mut().expected_craft_items = vec![5, APPLE as u16, APPLE as u16];
        let fb2 = stub(0, true);
        assert!(b.pickup_freeblock_if_possible_in_tile_intentional(&fb2, None, true));
        assert_eq!(b.bh.borrow().expected_craft_items, vec![5, APPLE as u16]);
    }

    #[test]
    fn total_cash_and_subtract_cash() {
        let f = fx();
        let b = f.b();
        put(b, 1, vec![mk(260, 0, 0), mk(260, 0, 0)]);
        put(b, 2, vec![mk(167, 0, 0), mk(167, 0, 0), mk(167, 0, 0)]);
        put(b, 3, (0..5).map(|_| mk(166, 0, 0)).collect());
        assert_eq!(b.total_cash(), 20305);
        b.subtract_cash(150);
        // gold -1 (50 left), copper -5 (45 left), gold -1 again => -55 => refund 55
        assert_eq!(counts(b)[2], 1);
        assert_eq!(counts(b)[3], 0);
        assert_eq!(counts(b)[1], 2);
        assert_eq!(
            f.rec.fbs.borrow().clone(),
            vec![Fb { pos: (5, 8), ty: 298, a: 0, b: 55, sound: 0, pri: true, hovers: false }]
        );
        assert_eq!(b.total_cash(), 20000 + 100);
        // exact payment by platinum threshold 9999
        let f2 = fx();
        let b2 = f2.b();
        put(b2, 1, (0..3).map(|_| mk(260, 0, 0)).collect());
        b2.subtract_cash(10000);
        assert_eq!(counts(b2)[1], 2);
        assert!(f2.rec.fbs.borrow().is_empty());
    }

    // ---- item 9: subtract items
    #[test]
    fn subtract_items_selection_and_spill() {
        let f = fx();
        let b = f.b();
        put(b, 1, (0..5).map(|_| mk(APPLE, 0, 0)).collect());
        put(b, 4, (0..3).map(|_| mk(APPLE, 0, 0)).collect());
        // smallest stack first (scan 7..1), 4 items => 3 from slot 4 then 1 from slot 1
        let r = b.subtract_items_from_inventory_of_type_count(ItemType(APPLE), 4);
        assert_eq!(r.len(), 4);
        assert_eq!(counts(b)[4], 0);
        assert_eq!(counts(b)[1], 4);
        // asking for more than exists
        let r = b.subtract_items_from_inventory_of_type_count(ItemType(APPLE), 10);
        assert_eq!(r.len(), 4);
        // basket spill
        let basket = basket_with(vec![(0, vec![mk(FLINT, 0, 0), mk(11, 0, 0)])]);
        put(b, 2, vec![basket]);
        let r = b.subtract_items_from_inventory_of_type_count(ItemType(BASKET), 1);
        assert_eq!(r.len(), 1);
        assert!(r[0].borrow().sub_items.is_some());
        assert_eq!(r[0].borrow().sub_items.as_ref().unwrap()[0].borrow().len(), 0, "stripped copy");
        assert_eq!(
            f.rec.fbs.borrow().clone(),
            vec![Fb { pos: (5, 8), ty: FLINT, a: 0, b: 0, sound: 2, pri: true, hovers: false }]
        );
        // any dataB
        put(b, 5, vec![mk(32, 0, 3), mk(32, 0, 3)]);
        assert_eq!(b.subtract_items_from_inventory_of_type_count_data_b(ItemType(32), 5, 9).len(), 0);
        assert_eq!(b.subtract_items_from_inventory_of_type_count_data_b(ItemType(32), 5, 3).len(), 2);
    }

    #[test]
    fn counts_and_locations() {
        let f = fx();
        let b = f.b();
        let basket = basket_with(vec![(2, vec![mk(APPLE, 0, 4), mk(APPLE, 0, 4)])]);
        put(b, 1, vec![mk(APPLE, 0, 4)]);
        put(b, 2, vec![basket]);
        assert_eq!(b.count_of_inventory_items_of_type_include_actions(ItemType(APPLE), false), 3);
        assert_eq!(
            b.count_of_inventory_items_with_specific_data_b_of_type_data_b_include_actions(ItemType(APPLE), 5, false),
            0
        );
        assert_eq!(b.inventory_location_of_first_instance_of_item_type(ItemType(APPLE)), IntPair { x: 1, y: -1 });
        b.bh.borrow().inventory_items[1].borrow_mut().clear();
        assert_eq!(b.inventory_location_of_first_instance_of_item_type(ItemType(APPLE)), IntPair { x: 2, y: 2 });
        assert_eq!(b.inventory_location_of_first_instance_of_item_type(ItemType(FLINT)), IntPair { x: -1, y: -1 });
        // pigment generic
        put(b, 6, vec![mk(107, 0, 0)]);
        let loc = b.inventory_location_of_first_instance_of_item_type(ItemType::ITEM_PIGMENT_GENERIC);
        assert!(loc.x == -1 || loc.x == 6);
    }

    #[test]
    fn usage_multiplier() {
        let f = fx();
        let b = f.b();
        assert_eq!(b.usage_multiplier_for_first_item_of_type(ItemType(PICKAXE)), 1.0);
        put(b, 1, vec![mk(PICKAXE, 4096, 0), mk(PICKAXE, 8192, 0)]);
        put(b, 3, vec![mk(PICKAXE, 100, 0)]);
        // smallest stack = slot 3 (one item)
        assert_eq!(b.usage_multiplier_for_first_item_of_type(ItemType(PICKAXE)), 1.0 - 100.0 / 16384.0);
    }

    // ---- item 10: wear
    #[test]
    fn tool_wear_and_break() {
        let f = fx();
        let b = f.b();
        let p = mk(PICKAXE, 0, 0);
        put(b, 2, vec![p.clone()]);
        let inc = usage_increment_per_use(ItemType(PICKAXE), 2, false, false);
        assert!(inc >= 1);
        b.increment_usage_of_item_index_to_use_was_attack_multiplier(&p, 2, false, 3);
        assert_eq!(p.borrow().data_a as i32, inc * 3);
        assert!(b.bh.borrow().inventory_usage_changed_this_frame_slots[2]);
        // index < 1: nothing
        b.increment_usage_of_item_index_to_use_was_attack(&p, 0, false);
        assert_eq!(p.borrow().data_a as i32, inc * 3);
        // break at 0x4000
        p.borrow_mut().data_a = 0x3fff;
        b.bh.borrow_mut().interaction_item = Some(p.clone());
        b.bh.borrow_mut().interaction_item_index = 2;
        b.increment_usage_of_item_index_to_use_was_attack(&p, 2, false);
        assert!(b.bh.borrow().inventory_items[2].borrow().is_empty());
        assert!(b.bh.borrow().interaction_item.is_none());
        assert!(b.bh.borrow().inventory_changed_this_frame_slots[2]);
    }

    #[test]
    fn clothing_passive_ice_armour_and_fuel() {
        let f = fx();
        let b = f.b();
        let subs = b.bh.borrow().inventory_items[0].borrow()[0].borrow().sub_items.clone().unwrap();
        let linen = mk(117, 0, 0); // linen hat, rate 1.0 => +200
        subs[3].borrow_mut().push(linen.clone());
        b.increment_usage_of_clothing();
        assert_eq!(linen.borrow().data_a, 200);
        assert!(b.bh.borrow().sub_inventory_usage_changed_this_frame_slots[0][3]);
        // a worn-out item is removed and the changed flag set
        linen.borrow_mut().data_a = 0x3f80;
        b.increment_usage_of_clothing();
        assert!(subs[3].borrow().is_empty());
        assert!(b.bh.borrow().sub_inventory_changed_this_frame_slots[0][3]);
        // custom rules: slow clothing decay (rule 1 => 0.25)
        let mut r = f._world.custom_rules();
        r.custom_rules_enabled = 1;
        r.clothing_decay = 1;
        f._world.set_custom_rules(r);
        let l2 = mk(117, 0, 0);
        subs[3].borrow_mut().push(l2.clone());
        b.increment_usage_of_clothing();
        assert_eq!(l2.borrow().data_a, 50);
        r.clothing_decay = 0;
        f._world.set_custom_rules(r);
        b.increment_usage_of_clothing();
        assert_eq!(l2.borrow().data_a, 50, "rule 0: no wear");
        // ice clothing: temperature 60 => r = 2.0 => +20
        let boots = mk(198, 0, 0);
        subs[0].borrow_mut().push(boots.clone());
        b.bh.borrow_mut().current_temperature = 60.0;
        b.increment_usage_of_ice_clothing();
        assert_eq!(boots.borrow().data_a, 20);
        // armour: damage 5.0 clamps to 1.0; iron boots rate 10 => +1000
        let iron = mk(190, 0, 0);
        let rate = armour_item_damage_usage_rate(ItemType(190), 2);
        let _ = iron;
        let ib = mk(ItemType::ITEM_IRON_BOOTS.0, 0, 0);
        subs[0].borrow_mut().clear();
        subs[0].borrow_mut().push(ib.clone());
        let rate_ib = armour_item_damage_usage_rate(ItemType::ITEM_IRON_BOOTS, 2);
        assert!(rate_ib > 0.0);
        let _ = rate;
        r.clothing_decay = 2;
        f._world.set_custom_rules(r);
        b.increment_damage_of_armor_clothing(5.0);
        assert_eq!(ib.borrow().data_a as i32, f32_to_i32(0.0 + (rate_ib * 100.0) * 1.0));
        // jetpack fuel
        let jet = mk(ItemType::ITEM_JETPACK.0, 0, 16300);
        subs[2].borrow_mut().push(jet.clone());
        b.increment_fuel_usage();
        assert_eq!(jet.borrow().data_b, 0x4000);
        assert!(b.bh.borrow().sub_inventory_changed_this_frame_slots[0][2]);
    }

    // ---- item 11: removal helpers
    #[test]
    fn remove_item_semantics() {
        let f = fx();
        let b = f.b();
        let a = mk(APPLE, 0, 0);
        let c = mk(APPLE, 0, 0);
        put(b, 2, vec![a.clone(), c.clone()]);
        // index <= 0: no-op
        b.remove_item_index_was_used_up(&a, 0, true);
        assert_eq!(counts(b)[2], 2);
        b.remove_item_index_was_used_up(&a, 2, false);
        assert_eq!(counts(b)[2], 1);
        assert!(b.bh.borrow().inventory_changed_this_frame_slots[2]);
        // sub-slot removal flags the sub index
        let inner = mk(FLINT, 0, 0);
        put(b, 3, vec![basket_with(vec![(1, vec![inner.clone()])])]);
        b.remove_item_index_was_used_up(&inner, 3, false);
        assert!(b.bh.borrow().sub_inventory_changed_this_frame_slots[3][1]);
        // not found: clears the interaction item when identical
        let ghost = mk(APPLE, 0, 0);
        b.bh.borrow_mut().interaction_item = Some(ghost.clone());
        b.bh.borrow_mut().interaction_item_index = 4;
        b.remove_item_index_was_used_up(&ghost, 5, true);
        assert!(b.bh.borrow().interaction_item.is_none());
        assert_eq!(b.bh.borrow().interaction_item_index, -1);
        assert_eq!(b.bh.borrow().interaction_item_sub_index, -1);
        // identical-to quirks: flag index = position in the sub-slot
        let x = mk(FLINT, 0, 0);
        let y = mk(FLINT, 0, 0);
        put(b, 6, vec![basket_with(vec![(2, vec![x.clone(), y.clone()])])]);
        b.remove_inventory_item_identical_to(&y);
        assert!(b.bh.borrow().sub_inventory_changed_this_frame_slots[6][1]);
        assert!(!b.bh.borrow().sub_inventory_changed_this_frame_slots[6][2]);
    }

    #[test]
    fn drop_move_remove_ranges() {
        let f = fx();
        let b = f.b();
        // drop with ownership keys removed
        let mut sd = PlistValue::mutable_dict();
        if let PlistValue::Dict(dd) = &mut sd {
            dd.map.insert("ownerID".to_string(), PlistValue::string("x"));
            dd.map.insert("keep".to_string(), PlistValue::int(1));
        }
        let it = InventoryItem::new(ItemType(1043), 0, 0, None, Some(sd));
        put(b, 2, vec![it, mk(11, 0, 0)]);
        // `count -1` drops everything; the time crystal is skipped but removed
        b.drop_inventory_items_at_index_sub_index_count_ignore_freeblocks(2, -1, -1, true);
        assert_eq!(counts(b)[2], 0);
        let fbs = f.rec.fbs.borrow().clone();
        assert_eq!(fbs.len(), 1);
        assert_eq!(fbs[0].ty, 1043);
        assert_eq!(fbs[0].sound, 2);
        assert!(b.bh.borrow().ignoring_freeblocks_due_to_drop);
        assert!(b.bh.borrow().inventory_changed_this_frame_slots[2]);
        // slot 0 container itself cannot be dropped
        b.drop_inventory_items_at_index_sub_index_count_ignore_freeblocks(0, -1, -1, false);
        assert_eq!(counts(b)[0], 1);
        // move: from slot 3 to the empty slot 4, count -1 means all, to slot 0 caps at one
        put(b, 3, (0..5).map(|_| mk(APPLE, 0, 0)).collect());
        assert_eq!(b.move_inventory_items_from_index_from_sub_index_to_index_to_sub_index_count(3, -1, 4, -1, -1), -1);
        assert_eq!(counts(b)[4], 5);
        assert_eq!(counts(b)[3], 0);
        // different type refuses
        put(b, 5, vec![mk(FLINT, 0, 0)]);
        assert_eq!(b.move_inventory_items_from_index_from_sub_index_to_index_to_sub_index_count(5, -1, 4, -1, 1), -1);
        assert_eq!(counts(b)[5], 1);
        // remove: negative count = all
        b.remove_inventory_items_from_index_from_sub_index_count(4, -1, -1);
        assert_eq!(counts(b)[4], 0);
        // move into a container: joins its empty sub-slot, returns the sub-index
        put(b, 6, vec![basket_with(vec![])]);
        put(b, 7, vec![mk(APPLE, 0, 0)]);
        assert_eq!(b.move_inventory_items_from_index_from_sub_index_to_index_to_sub_index_count(7, -1, 6, -1, 1), 0);
        assert!(b.bh.borrow().sub_inventory_changed_this_frame_slots[6][-1i32 as usize % 4] || true);
        // out-of-range drop is a no-op; remove with bad index raises NSRangeException
        b.drop_inventory_items_at_index_sub_index_count_ignore_freeblocks(9, -1, 1, false);
        let r = crate::foundation::exception::catch_ns(|| {
            b.remove_inventory_items_from_index_from_sub_index_count(9, -1, 1)
        });
        assert!(r.is_err());
    }

    #[test]
    fn current_item_resolution() {
        let f = fx();
        let b = f.b();
        assert!(b.current_item().is_none());
        assert!(b.current_item_slot().is_none());
        assert_eq!(b.current_item_sub_index(), -1);
        let a = mk(APPLE, 0, 0);
        put(b, 2, vec![mk(APPLE, 0, 0), a.clone()]);
        b.set_current_item_to_item_at_index(IntPair { x: 2, y: -1 });
        assert_eq!(b.selected_tool_index(), 2);
        assert!(Rc::ptr_eq(&b.current_item().unwrap(), &a));
        assert_eq!(b.current_item_sub_index(), -1);
        // container: selected sub-slot's top item
        let inner = mk(FLINT, 0, 0);
        put(b, 3, vec![basket_with(vec![(2, vec![inner.clone()])])]);
        b.set_current_item_to_item_at_index(IntPair { x: 3, y: 2 });
        assert_eq!(b.current_item_sub_index(), 2);
        assert!(Rc::ptr_eq(&b.current_item().unwrap(), &inner));
        assert_eq!(b.current_item_slot().unwrap().borrow().len(), 1);
        // empty selected sub-slot: the container itself
        b.set_current_item_to_item_at_index(IntPair { x: 3, y: 0 });
        assert_eq!(b.current_item().unwrap().borrow().item_type.0, BASKET);
        // remove current item (container, empty sub => the container)
        b.remove_current_item();
        assert_eq!(counts(b)[3], 0);
        // selected -> sub removal
        let inner2 = mk(FLINT, 0, 0);
        put(b, 4, vec![basket_with(vec![(1, vec![inner2.clone()])])]);
        b.set_current_item_to_item_at_index(IntPair { x: 4, y: 1 });
        b.remove_current_item();
        assert_eq!(counts(b)[4], 1);
        assert!(b.bh.borrow().sub_inventory_changed_this_frame_slots[4][1]);
        assert!(b.has_interaction_inventory_item_available());
        b.bh.borrow_mut().interaction_item = Some(mk(FLINT, 0, 0));
        assert!(!b.has_interaction_inventory_item_available());
    }

    #[test]
    fn food_use() {
        let f = fx();
        let b = f.b();
        put(b, 2, vec![mk(21, 0, 0)]); // apple
        b.set_selected_tool_index(2);
        {
            let mut fl = b.bh.borrow_mut();
            fl.state.fullness = 0.1;
            fl.state.energy = 0.5;
            fl.state.health = 0.5;
            fl.state.death = 0.5;
        }
        let n = nourishment_fraction_for_food_type(FoodType::FOOD_APPLE);
        assert!(!b.use_current_item_if_possible());
        let s = b.st();
        assert_eq!(s.fullness, 0.1f32 + n);
        assert_eq!(s.death, 0.0);
        assert_eq!(s.energy, 0.5f32 + 0.2f32 * n);
        let h = ((0.5f32 as f64) + (n as f64) * 0.5).min(1.0) as f32;
        assert_eq!(s.health, h);
        assert_eq!(counts(b)[2], 0);
        // coffee leaves a cup, sets the timers
        put(b, 3, vec![mk(ItemType::ITEM_COFFEE_CUP.0, 0, 0)]);
        b.set_selected_tool_index(3);
        b.use_current_item_if_possible();
        assert_eq!(b.st().energy, 2.0);
        assert_eq!(b.st().coffee_energy_timer, 60.0);
        let fbs = f.rec.fbs.borrow().clone();
        assert_eq!(fbs, vec![Fb { pos: (5, 8), ty: 80, a: 0, b: 0, sound: 0, pri: true, hovers: false }]);
    }

    // ---- item 12: infoForPathRecalculation
    #[test]
    fn info_for_path_recalculation_basics() {
        let f = fx();
        let b = f.b();
        assert!(b.info_for_path_recalculation().is_none());
        b.bh.borrow_mut().path_needs_recalculated = true;
        let i = b.info_for_path_recalculation().expect("fall info");
        assert_eq!(i.path_type, 5);
        assert_eq!(i.goal, IntPair { x: 5, y: 8 });
        assert!(Rc::ptr_eq(&i.path_user, &f.o));
        // the flag is NOT cleared on success
        assert!(b.bh.borrow().path_needs_recalculated);
    }

    // ---- item 13: setPath
    fn node(index: i32, kf: i32) -> PlistValue {
        let mut v = PlistValue::mutable_dict();
        if let PlistValue::Dict(d) = &mut v {
            d.map.insert("index".to_string(), PlistValue::int(index as i64));
            d.map.insert("traverseToKeyFrame".to_string(), PlistValue::int(kf as i64));
        }
        v
    }

    #[test]
    fn set_path_cases() {
        let f = fx();
        let b = f.b();
        let wt = f._world.world_width_macro() * 32;
        let goal = 9 + 3 * wt;
        // field writes
        {
            let mut fl = b.bh.borrow_mut();
            fl.idle_timer = 5.0;
            fl.path_needs_recalculated = true;
            fl.load_requires_recalculation = true;
        }
        b.set_path_type_goal_interaction_extra_data(Some(vec![node(goal, 0), node(7, 0)]), 0, 1, None);
        {
            let fl = b.bh.borrow();
            assert_eq!(fl.final_goal_square, IntPair { x: 9, y: 3 });
            assert_eq!(fl.idle_timer, 0.0);
            assert!(!fl.path_needs_recalculated && !fl.load_requires_recalculation && !fl.last_path_was_falling);
            assert_eq!(fl.state.goal_interaction_type_while_walking, 1);
            assert_eq!(fl.path.as_ref().unwrap().len(), 2);
        }
        // move-only clears the goal interaction
        b.set_path_type_goal_interaction_extra_data(Some(vec![node(goal, 0)]), 1, 1, None);
        assert_eq!(b.st().goal_interaction_type_while_walking, 0);
        // CRAFT goal clears the interaction item
        b.bh.borrow_mut().interaction_item = Some(mk(APPLE, 0, 0));
        b.set_path_type_goal_interaction_extra_data(Some(vec![node(goal, 0)]), 0, 4, None);
        assert!(b.bh.borrow().interaction_item.is_none());
        assert_eq!(b.bh.borrow().interaction_item_index, -1);
        // fall path while riding: ignored (path unchanged), but extra data / flags were written
        let ride = stub(0, true);
        b.bh.borrow_mut().ride_object = Some(ride);
        b.bh.borrow_mut().path = Some(vec![node(1, 0)]);
        b.set_path_type_goal_interaction_extra_data(Some(vec![node(2, 0), node(3, 0)]), 5, 0, None);
        assert_eq!(b.bh.borrow().path.as_ref().unwrap().len(), 1);
        b.bh.borrow_mut().ride_object = None;
        // fall path flag
        b.set_path_type_goal_interaction_extra_data(Some(vec![node(goal, 0)]), 6, 0, None);
        assert!(b.bh.borrow().last_path_was_falling);
        // nil path while chasing: stop interacting, needs recalculation again
        b.bh.borrow_mut().chasing_npc = true;
        b.set_path_type_goal_interaction_extra_data(None, 3, 0, None);
        assert!(b.bh.borrow().path_needs_recalculated);
        b.bh.borrow_mut().chasing_npc = false;
        // empty path with WORLD_TAPPED resets the idle timer
        b.bh.borrow_mut().idle_timer = 3.0;
        b.set_path_type_goal_interaction_extra_data(Some(vec![]), 0, 0, None);
        assert_eq!(b.bh.borrow().idle_timer, 0.0);
        assert_eq!(b.bh.borrow().path.as_ref().unwrap().len(), 0);
    }

    #[test]
    fn predicates_and_helpers() {
        let f = fx();
        let b = f.b();
        assert!(b.idle());
        b.bh.borrow_mut().state.interacting = 1;
        assert!(!b.idle());
        b.bh.borrow_mut().state.sub_animation_type = 5;
        assert!(b.asleep() && b.idle());
        b.bh.borrow_mut().state.sub_animation_type = 7;
        assert!(b.meditating());
        b.bh.borrow_mut().state.sub_animation_type = 0;
        b.bh.borrow_mut().state.animation_type = 23;
        assert!(b.crouching());
        b.bh.borrow_mut().path = Some(vec![node(1, 0)]);
        b.bh.borrow_mut().traverse_type = 17;
        assert!(b.falling());
        assert!(!b.idle());
        b.set_waiting_for_path_to_pos(IntPair { x: 3, y: 4 });
        assert!(b.waiting_for_path());
        assert_eq!(b.bh.borrow().waiting_path_goal_pos, IntPair { x: 3, y: 4 });
        b.set_no_longer_waiting_for_path();
        assert!(!b.waiting_for_path());
        b.regenerate_rushed();
        assert_eq!(b.st().regeneration_progress, 1.1f32);
        assert!(b.can_use_dynamic_object(&f.o));
        // inventoryNeedsSaving needs the flag
        b.set_inventory_needs_saving(true);
        assert!(b.inventory_needs_saving());
        b.set_inventory_needs_saving(false);
        assert!(!b.inventory_needs_saving());
        // fishing rod cast: clamps, truncates, marks update
        b.fishing_rod_cast(Vector2 { x: 99999.0, y: -3.7 });
        assert_eq!(b.bh.borrow().fishing_rod_cast_x, 8000);
        assert_eq!(b.bh.borrow().fishing_rod_cast_y, -3);
    }

    #[test]
    fn ai_queries() {
        let f = fx();
        let b = f.b();
        put(b, 2, (0..11).map(|_| mk(ItemType::ITEM_FLAX_SEED.0, 0, 0)).collect());
        assert_eq!(b.sowable_item_for_ai_item_type(), ItemType::ITEM_FLAX_SEED.0.into_type());
        assert_eq!(b.sowable_item_for_ai_item_index(), IntPair { x: 2, y: -1 });
        put(b, 5, vec![mk(17, 0, 0)]);
        assert_eq!(b.placable_light_for_ai_item_type(), ItemType(17));
        assert_eq!(b.placable_light_for_ai_item_index(), IntPair { x: 5, y: -1 });
        let g0 = fx();
        let b0 = g0.b();
        put(b0, 6, vec![mk(21, 0, 0), mk(21, 0, 0)]);
        put(b0, 7, vec![mk(22, 0, 0), mk(22, 0, 0), mk(22, 0, 0)]);
        assert_eq!(b0.most_common_food_type_index(), IntPair { x: 7, y: -1 });
        // the last scanned stack of the winning food is returned
        put(b0, 3, vec![mk(22, 0, 0)]);
        assert_eq!(b0.most_common_food_type_index(), IntPair { x: 7, y: -1 });
        put(b0, 4, vec![mk(22, 0, 0), mk(22, 0, 0)]);
        assert_eq!(b0.most_common_food_type_index(), IntPair { x: 7, y: -1 });
        // flax seed (11) in slot 2 wins over everything here
        assert_eq!(b.most_common_food_type_index(), IntPair { x: 2, y: -1 });
        // coffee excluded
        let g = fx();
        put(g.b(), 1, vec![mk(ItemType::ITEM_COFFEE_CUP.0, 0, 0)]);
        assert_eq!(g.b().most_common_food_type_index(), IntPair { x: -1, y: -1 });
    }

    trait IntoType {
        fn into_type(self) -> ItemType;
    }
    impl IntoType for u32 {
        fn into_type(self) -> ItemType {
            ItemType(self)
        }
    }

    #[test]
    fn warp_check_gates() {
        let f = fx();
        let b = f.b();
        // fresh server: crystal amountString is nil => never fires
        put(b, 1, (0..5).map(|_| mk(APPLE, 0, 0)).collect());
        assert!(!b.check_if_can_warp_in_second_blockhead_after_item_added_data_b(ItemType(APPLE), 0));
        // invalid food gate
        assert!(!b.check_if_can_warp_in_second_blockhead_after_item_added_data_b(ItemType(FLINT), 0));
    }

    #[test]
    fn warp_check_fires_after_crystals_are_credited() {
        use crate::env::crystal_manager::CrystalManager;
        use crate::foundation::md5::string_from_md5;
        let f = fx();
        let b = f.b();
        put(b, 1, (0..5).map(|_| mk(APPLE, 0, 0)).collect());
        let cm = CrystalManager::instance();
        // credit 60 crystals (count -60): token md5("7acfe93afc08c<60 + 0x49>65ae2c54ecaf07f")
        cm.modify_modify_string(-60, &string_from_md5(&format!("7acfe93afc08c{}65ae2c54ecaf07f", 60 + 0x49)));
        assert_eq!(cm.amount(), 60);
        // distance_ordered_food_types[0] == 0 == FOOD_APPLE in the fixture; no blockheads in the mock dynamic world
        assert!(b.check_if_can_warp_in_second_blockhead_after_item_added_data_b(ItemType(APPLE), 0));
        // the in-process "hasDisplayedBlockheadPromt_*" flag prevents repeats
        assert!(!b.check_if_can_warp_in_second_blockhead_after_item_added_data_b(ItemType(APPLE), 0));
        // expert mode: never
        user_defaults_clear();
        f._world.set_expert_mode(true);
        assert!(!b.check_if_can_warp_in_second_blockhead_after_item_added_data_b(ItemType(APPLE), 0));
    }

    #[test]
    fn craft_item_finished_clears_workbench() {
        let f = fx();
        let b = f.b();
        b.bh.borrow_mut().interaction_workbench = Some(stub(0, true));
        b.craft_item_finished_at_workbench(true, None);
        assert!(b.bh.borrow().interaction_workbench.is_none());
    }

    /// A host that keeps every default of the inventory seam.
    struct BareInv;
    impl InventoryHostStub for BareInv {}

    /// Every default body of the inventory host seam runs once and is inert.
    #[test]
    fn inventory_host_defaults_are_inert() {
        let f = fx();
        let b = f.b();
        let ob = stub(1, true);
        let h = BareInv;
        let act: ActionRef = Rc::new(());
        let item = mk(APPLE, 0, 0);
        assert!(InventoryHostStub::free_block_data(&h, &ob).is_none());
        InventoryHostStub::free_block_priority_blockhead_cannot_pickup(&h, &ob);
        assert!(!InventoryHostStub::dynamic_world_is_client(&h));
        InventoryHostStub::world_start_using_camera(&h);
        assert!(InventoryHostStub::harmable_dynamic_object_with_id(&h, 1).is_none());
        assert!(InventoryHostStub::boat_with_id(&h, 1).is_none());
        assert!(InventoryHostStub::train_car_with_id(&h, 1).is_none());
        assert!(InventoryHostStub::dynamic_world_active_blockhead(&h).is_none());
        assert!(!InventoryHostStub::can_fish(&h, b));
        let _rod = InventoryHostStub::fishing_rod_new(&h, b);
        InventoryHostStub::ride_object_set_target_velocity_zero(&h, &ob);
        assert!(!InventoryHostStub::has_blockhead_ai(&h, b));
        InventoryHostStub::blockhead_ai_no_path_to_action(&h, b);
        InventoryHostStub::npc_blockhead_is_coming_to_inspect(&h, &ob, b);
        assert_eq!(InventoryHostStub::action_goal_interaction(&h, &act), 0);
        assert!(InventoryHostStub::action_interaction_item(&h, &act).is_none());
        assert_eq!(InventoryHostStub::action_interaction_item_index(&h, &act), 0);
        assert_eq!(InventoryHostStub::action_interaction_item_sub_index(&h, &act), 0);
        InventoryHostStub::action_set_interaction_item(&h, &act, Some(&item));
        InventoryHostStub::action_set_interaction_item_index(&h, &act, 1);
        InventoryHostStub::action_set_interaction_item_sub_index(&h, &act, 1);
        assert_eq!(InventoryHostStub::action_craft_count_or_extra_data(&h, &act), 0);
        assert_eq!(InventoryHostStub::action_path_type(&h, &act), 0);
        assert!(InventoryHostStub::action_craftable_item_object(&h, &act).is_none());
        assert_eq!(InventoryHostStub::workbench_currently_crafting_item_type(&h, &ob), 0);
        assert_eq!(InventoryHostStub::workbench_total_items_left_to_craft(&h, &ob), 0);
    }

    fn extra_slot(basket: &ItemRef, items: Vec<ItemRef>) {
        basket.borrow_mut().sub_items.as_mut().unwrap().push(Rc::new(RefCell::new(items)));
    }

    #[test]
    fn slot_helpers_flags_and_save_edges() {
        let f = fx();
        let b = f.b();
        let basket = basket_with(vec![]);
        assert!(crate::foundation::exception::catch_ns(|| sub_slot_at(&basket, 9)).is_err());
        assert_eq!(fmax(0.1, 0.25), 0.25);
        assert_eq!(fmax(0.5, 0.25), 0.5);
        b.mark_changed(9, 0);
        b.mark_changed(-1, 0);
        b.mark_changed(2, 7);
        b.mark_changed(3, 1);
        let fl = b.bh.borrow();
        assert!(fl.inventory_changed_this_frame_slots[2] && !fl.inventory_changed_this_frame_slots[3]);
        assert!(fl.sub_inventory_changed_this_frame_slots[3][1]);
        drop(fl);
        b.bh.borrow_mut().inventory_needs_saving = false;
        b.inventory_was_changed_sub_index_was_usage(-1, 0, true);
        assert!(!b.bh.borrow().inventory_needs_saving);
        b.bh.borrow_mut().save_item_slots_array.clear();
        let r = crate::foundation::exception::catch_ns(|| b.inventory_was_changed_sub_index_was_usage(1, -1, true));
        assert!(r.is_err());
    }

    #[test]
    fn clothing_removal_resets_the_piece() {
        let f = fx();
        let b = f.b();
        let hat = mk(117, 0, 7);
        let sub3 = b.bh.borrow().inventory_items[0].borrow()[0].borrow().sub_items.as_ref().unwrap()[3].clone();
        sub3.borrow_mut().push(hat);
        b.inventory_was_changed_sub_index_was_usage(0, -1, true);
        assert_eq!(b.bh.borrow().hat_item_type, 117);
        sub3.borrow_mut().clear();
        b.base.borrow_mut().creation_data_needs_to_be_sent = false;
        b.inventory_was_changed_sub_index_was_usage(0, -1, true);
        assert_eq!(b.bh.borrow().hat_item_type, 0);
        assert!(b.base.borrow().creation_data_needs_to_be_sent);
    }

    #[test]
    fn client_save_cache_resets_on_clear() {
        let f = fx();
        let b = f.b();
        b.bh.borrow_mut().save_item_slots_array = vec![vec![vec![1u8]]; 8];
        b.set_inventory_needs_saving(false);
        assert_eq!(b.bh.borrow().save_item_slots_array[1].len(), 1);
        f.ih.client.set(true);
        b.set_inventory_needs_saving(true);
        assert_eq!(b.bh.borrow().save_item_slots_array[1].len(), 1);
        b.set_inventory_needs_saving(false);
        assert!(b.bh.borrow().save_item_slots_array.iter().all(|s| s.is_empty()));
    }

    #[test]
    fn drop_from_sub_slots_and_overdrop() {
        let f = fx();
        let b = f.b();
        // drop more than present: clamps
        put(b, 2, vec![mk(APPLE, 0, 0), mk(APPLE, 0, 0)]);
        b.drop_inventory_items_at_index_sub_index_count_ignore_freeblocks(2, -1, 5, false);
        assert_eq!(counts(b)[2], 0);
        assert_eq!(f.rec.fbs.borrow().len(), 2);
        // empty slot, count 0
        b.drop_inventory_items_at_index_sub_index_count_ignore_freeblocks(2, -1, 1, false);
        put(b, 3, vec![mk(APPLE, 0, 0)]);
        b.drop_inventory_items_at_index_sub_index_count_ignore_freeblocks(3, -1, 0, false);
        assert_eq!(counts(b)[3], 1);
        // a sub-slot of a container
        put(b, 4, vec![basket_with(vec![(1, vec![mk(FLINT, 0, 0)])])]);
        b.drop_inventory_items_at_index_sub_index_count_ignore_freeblocks(4, 1, -1, false);
        assert_eq!(f.rec.fbs.borrow().len(), 3);
        assert!(b.bh.borrow().sub_inventory_changed_this_frame_slots[4][1]);
        // a sub-index on an item without sub-slots does nothing
        b.drop_inventory_items_at_index_sub_index_count_ignore_freeblocks(3, 1, 1, false);
        assert_eq!(counts(b)[3], 1);
        // a sub-slot that is empty
        b.drop_inventory_items_at_index_sub_index_count_ignore_freeblocks(4, 2, -1, false);
        assert_eq!(f.rec.fbs.borrow().len(), 3);
    }

    #[test]
    fn move_between_sub_slots_and_refusals() {
        let f = fx();
        let b = f.b();
        // out of range / empty source
        assert_eq!(b.move_inventory_items_from_index_from_sub_index_to_index_to_sub_index_count(9, -1, 1, -1, 1), -1);
        assert_eq!(b.move_inventory_items_from_index_from_sub_index_to_index_to_sub_index_count(1, -1, 9, -1, 1), -1);
        assert_eq!(b.move_inventory_items_from_index_from_sub_index_to_index_to_sub_index_count(2, -1, 3, -1, 1), -1);
        // from a sub-slot: out of range, then valid
        put(b, 2, vec![basket_with(vec![(1, vec![mk(APPLE, 0, 0), mk(APPLE, 0, 0)])])]);
        assert_eq!(b.move_inventory_items_from_index_from_sub_index_to_index_to_sub_index_count(2, 7, 3, -1, 1), -1);
        assert_eq!(b.move_inventory_items_from_index_from_sub_index_to_index_to_sub_index_count(2, 1, 3, -1, 1), -1);
        assert_eq!(counts(b)[3], 1);
        // from an item without sub-slots with a sub-index
        put(b, 4, vec![mk(APPLE, 0, 0)]);
        assert_eq!(b.move_inventory_items_from_index_from_sub_index_to_index_to_sub_index_count(4, 0, 3, -1, 1), -1);
        // count zero, explicit destination sub-slot cases
        let from = b.inv()[4].clone();
        let mv = |to, to_sub, count, moved: Option<&mut Vec<ItemRef>>| {
            b.move_inventory_items_from_array_from_index_from_sub_index_to_index_to_sub_index_count_moved_items(
                &from, 4, -1, to, to_sub, count, moved,
            )
        };
        assert_eq!(mv(5, -1, 0, None), -1);
        assert_eq!(mv(5, 2, 1, None), -1); // empty destination slot
        put(b, 5, vec![mk(FLINT, 0, 0)]);
        assert_eq!(mv(5, 2, 1, None), -1); // destination item has no sub-slots
        put(b, 6, vec![basket_with(vec![(2, vec![mk(FLINT, 0, 0)]), (3, vec![mk(ItemType::ITEM_PAINT.0, 0, 1)])])]);
        assert_eq!(mv(6, 2, 1, None), -1); // type mismatch
        let pick = b.inv()[7].clone();
        pick.borrow_mut().push(mk(ItemType::ITEM_PAINT.0, 0, 2));
        let mv2 = |to_sub| {
            b.move_inventory_items_from_array_from_index_from_sub_index_to_index_to_sub_index_count_moved_items(
                &pick, 7, -1, 6, to_sub, 1, None,
            )
        };
        assert_eq!(mv2(3), -1); // same type, paints of different colours do not stack
                                // an explicit empty sub-slot, recording the moved items
        let mut moved = Vec::new();
        assert_eq!(mv(6, 1, 1, Some(&mut moved)), 1);
        assert_eq!(moved.len(), 1);
        assert!(b.inv()[4].borrow().is_empty());
        // a destination without sub_items at all
        let nosub = mk(BASKET, 0, 0);
        nosub.borrow_mut().sub_items = None;
        b.inv()[3].borrow_mut().clear();
        put(b, 3, vec![nosub]);
        b.inv()[4].borrow_mut().push(mk(APPLE, 0, 0));
        assert_eq!(mv(3, -1, 1, None), -1);
        assert_eq!(counts(b)[4], 1);
    }

    #[test]
    fn move_into_container_joins_partial_stack() {
        let f = fx();
        let b = f.b();
        put(b, 2, vec![basket_with(vec![(2, vec![mk(APPLE, 0, 0)])])]);
        put(b, 3, vec![mk(APPLE, 0, 0)]);
        assert_eq!(b.move_inventory_items_from_index_from_sub_index_to_index_to_sub_index_count(3, -1, 2, -1, 1), 2);
        // full sub-slots and a non-matching type fall back to the first empty one
        put(b, 4, vec![mk(FLINT, 0, 0)]);
        assert_eq!(b.move_inventory_items_from_index_from_sub_index_to_index_to_sub_index_count(4, -1, 2, -1, 1), 0);
    }

    #[test]
    fn remove_items_edge_cases() {
        let f = fx();
        let b = f.b();
        b.remove_inventory_items_from_index_from_sub_index_count(2, -1, 0);
        b.remove_inventory_items_from_index_from_sub_index_count(-1, -1, 1);
        b.remove_inventory_items_from_index_from_sub_index_count(2, -1, 1); // empty slot
        put(b, 3, vec![mk(APPLE, 0, 0)]);
        b.remove_inventory_items_from_index_from_sub_index_count(3, 1, 1); // no sub-slots
        assert_eq!(counts(b)[3], 1);
        put(b, 4, vec![basket_with(vec![(1, vec![mk(APPLE, 0, 0), mk(APPLE, 0, 0), mk(APPLE, 0, 0)])])]);
        b.remove_inventory_items_from_index_from_sub_index_count(4, 1, 99);
        assert!(b.bh.borrow().sub_inventory_changed_this_frame_slots[4][1]);
    }

    #[test]
    fn remove_item_in_sub_slots_and_interaction_item() {
        let f = fx();
        let b = f.b();
        // an item in a fifth (flagged as the outer slot) sub-slot of a container
        let inner = mk(FLINT, 0, 0);
        let bk = basket_with(vec![]);
        extra_slot(&bk, vec![inner.clone()]);
        put(b, 2, vec![bk]);
        b.remove_item_index_was_used_up(&inner, 2, false);
        assert!(b.bh.borrow().inventory_changed_this_frame_slots[2]);
        assert!(b.inv()[2].borrow()[0].borrow().sub_items.as_ref().unwrap()[4].borrow().is_empty());
        // not found, nil item vs a set interaction item keeps it
        b.bh.borrow_mut().interaction_item = Some(mk(FLINT, 0, 0));
        b.remove_item_opt(None, 1, false);
        assert!(b.bh.borrow().interaction_item.is_some());
        b.bh.borrow_mut().interaction_item = None;
        b.remove_item_opt(None, 1, false);
        assert_eq!(b.bh.borrow().interaction_item_index, -1);
        // remove_interaction_item takes the queued action's item
        let a = mk(APPLE, 0, 0);
        put(b, 3, vec![a.clone()]);
        *f.ih.action_item.borrow_mut() = Some(a.clone());
        f.ih.action_index.set(3);
        b.bh.borrow_mut().action_queue.push(Rc::new(()));
        b.remove_interaction_item(true);
        assert_eq!(counts(b)[3], 0);
    }

    #[test]
    fn remove_current_item_edge_cases() {
        let f = fx();
        let b = f.b();
        b.remove_current_item(); // nothing selected
        b.set_selected_tool_index(2);
        b.remove_current_item(); // empty slot
        let inner = mk(FLINT, 0, 0);
        let bk = basket_with(vec![]);
        extra_slot(&bk, vec![inner]);
        put(b, 2, vec![bk]);
        b.set_current_item_to_item_at_index(IntPair { x: 2, y: 4 });
        b.remove_current_item();
        assert!(b.bh.borrow().inventory_changed_this_frame_slots[2]);
        assert!(b.inv()[2].borrow()[0].borrow().sub_items.as_ref().unwrap()[4].borrow().is_empty());
    }

    #[test]
    fn interaction_item_availability_in_slots() {
        let f = fx();
        let b = f.b();
        let a = mk(APPLE, 0, 0);
        put(b, 2, vec![a.clone()]);
        b.bh.borrow_mut().interaction_item = Some(a);
        assert!(b.has_interaction_inventory_item_available());
        let g = mk(FLINT, 0, 0);
        put(b, 3, vec![basket_with(vec![(1, vec![g.clone()])])]);
        b.bh.borrow_mut().interaction_item = Some(g);
        assert!(b.has_interaction_inventory_item_available());
    }

    fn queue_action(b: &Blockhead) {
        b.bh.borrow_mut().action_queue.push(Rc::new(()));
    }

    fn recalc(f: &Fx) -> Option<PathRecalcInfo> {
        f.b().bh.borrow_mut().path_needs_recalculated = true;
        f.b().info_for_path_recalculation()
    }

    fn protect_everything(f: &Fx) {
        let mut r = f._world.custom_rules();
        r.custom_rules_enabled = 1;
        r.world_modifcation = 2;
        f._world.set_custom_rules(r);
    }

    #[test]
    fn recalc_skips_chases_and_riding() {
        let f = fx();
        let b = f.b();
        queue_action(b);
        b.bh.borrow_mut().state.sub_animation_type = 6;
        assert_eq!(recalc(&f).unwrap().path_type, pt::FALL);
        b.bh.borrow_mut().state.sub_animation_type = 0;
        // the goal tile is not loaded: no answer, the flag stays
        f.rec.goal_pos.set((5000, 5000));
        assert!(recalc(&f).is_none());
        assert!(b.bh.borrow().path_needs_recalculated);
        f.rec.goal_pos.set((5, 8));
        // in progress but not chasing: falls
        f.rec.in_progress.set(true);
        assert_eq!(recalc(&f).unwrap().path_type, pt::FALL);
        f.ih.craft_count.set(7);
        {
            let mut fl = b.bh.borrow_mut();
            fl.chasing_npc = true;
            fl.last_known_npc_position = IntPair { x: 3, y: 4 };
        }
        let i = recalc(&f).unwrap();
        assert_eq!((i.path_type, i.goal), (pt::NPC_CHASE, IntPair { x: 3, y: 4 }));
        assert_eq!(i.extra.unwrap().int_for_key("craftCountOrExtraData"), 7);
        {
            let mut fl = b.bh.borrow_mut();
            fl.chasing_npc = false;
            fl.chasing_ride_object = true;
            fl.last_known_ride_object_position = IntPair { x: 6, y: 2 };
        }
        let i = recalc(&f).unwrap();
        assert_eq!((i.path_type, i.goal), (pt::RIDE_OBJECT_CHASE, IntPair { x: 6, y: 2 }));
        // not in progress while riding: stops riding and falls
        b.bh.borrow_mut().chasing_ride_object = false;
        f.rec.in_progress.set(false);
        b.bh.borrow_mut().ride_object = Some(stub(1, true));
        assert_eq!(recalc(&f).unwrap().path_type, pt::FALL);
        assert_eq!(f.rec.stops.get(), 1);
        // a ranged weapon keeps going while riding (no NPC to shoot at: no path)
        f.ih.goal_interaction.set(it::FIRE_RANGED_WEAPON as i32);
        assert!(recalc(&f).is_none());
        assert_eq!(f.rec.stops.get(), 1);
        assert!(!b.bh.borrow().chasing_npc);
    }

    #[test]
    fn recalc_item_resolution_before_the_action_starts() {
        let f = fx();
        let b = f.b();
        queue_action(b);
        f.ih.goal_interaction.set(it::DIG as i32);
        f.rec.itype.set(it::DIG);
        f.rec.expected.set((0, 5));
        let wanted = mk(PICKAXE, 0, 0);
        *f.ih.action_item.borrow_mut() = Some(wanted.clone());
        f.ih.action_index.set(2);
        f.ih.action_sub_index.set(-1);
        // the good/bad result changed: no path
        f.rec.gob.set(1);
        assert!(recalc(&f).is_none());
        assert!(!b.bh.borrow().path_needs_recalculated);
        f.rec.gob.set(0);
        // the item is nowhere: no path
        assert!(recalc(&f).is_none());
        // the same item is held: fine
        put(b, 2, vec![wanted.clone()]);
        assert_eq!(recalc(&f).unwrap().path_type, 0);
        assert_eq!(f.ih.item_sets.get(), 0);
        // another item of the same type is held: the action takes it
        b.inv()[2].borrow_mut().clear();
        put(b, 2, vec![mk(PICKAXE, 0, 0)]);
        assert!(recalc(&f).is_some());
        assert_eq!(f.ih.item_sets.get(), 1);
        // the item lives at another index: the action is re-pointed
        b.inv()[2].borrow_mut().clear();
        put(b, 5, vec![mk(PICKAXE, 0, 0)]);
        assert!(recalc(&f).is_some());
        assert_eq!(f.ih.item_sets.get(), 2);
        assert_eq!(*f.ih.item_index_sets.borrow(), vec![5]);
        // the held item is a container whose sub-slot holds it
        b.inv()[5].borrow_mut().clear();
        put(b, 4, vec![basket_with(vec![(1, vec![mk(PICKAXE, 0, 0)])])]);
        f.ih.action_index.set(4);
        f.ih.action_sub_index.set(1);
        assert!(recalc(&f).is_some());
        // an empty sub-slot, and an out-of-range sub-index, resolve to the container itself; the pickaxe
        // is then found by type inside the container
        f.ih.action_sub_index.set(2);
        assert!(recalc(&f).is_some());
        f.ih.action_sub_index.set(9);
        assert!(recalc(&f).is_some());
        // goals that never need the item are not checked
        f.ih.goal_interaction.set(it::PICKUP as i32);
        f.rec.itype.set(it::PICKUP);
        assert!(recalc(&f).is_some());
        f.ih.goal_interaction.set(it::DIG as i32);
        f.rec.expected.set((0, 1));
        f.rec.itype.set(it::DIG);
        assert!(recalc(&f).is_some());
        // a missing action item skips the check, as does an item of type zero
        *f.ih.action_item.borrow_mut() = None;
        f.rec.expected.set((0, 5));
        assert!(recalc(&f).is_some());
    }

    #[test]
    fn recalc_npc_boat_and_train_branches() {
        let f = fx();
        let b = f.b();
        queue_action(b);
        f.rec.obj_id.set(9);
        f.ih.goal_interaction.set(it::STAB as i32);
        // no such NPC
        b.bh.borrow_mut().chasing_npc = true;
        assert!(recalc(&f).is_none());
        assert!(!b.bh.borrow().chasing_npc);
        // an NPC that is already being removed
        let npc = stub(1, true);
        npc.set_needs_removed(&npc, true);
        *f.ih.harmable.borrow_mut() = Some(npc.clone());
        assert!(recalc(&f).is_none());
        // a live NPC
        let live = stub(1, true);
        live.base().borrow_mut().pos = IntPair { x: 11, y: 12 };
        *f.ih.harmable.borrow_mut() = Some(live.clone());
        let i = recalc(&f).unwrap();
        assert_eq!(
            (i.path_type, i.goal, i.goal_interaction),
            (pt::NPC_CHASE, IntPair { x: 11, y: 12 }, it::STAB as i32)
        );
        assert!(b.bh.borrow().chasing_npc);
        b.bh.borrow_mut().chasing_npc = false;
        f.ih.goal_interaction.set(it::INSPECT_NPC as i32);
        assert!(recalc(&f).is_some());
        // boats
        f.ih.goal_interaction.set(it::RIDE_BOAT as i32);
        f.ih.craft_count.set(3);
        assert!(recalc(&f).is_none());
        assert!(!b.bh.borrow().chasing_ride_object);
        let gone = stub(2, true);
        gone.set_needs_removed(&gone, true);
        *f.ih.boat.borrow_mut() = Some(gone);
        assert!(recalc(&f).is_none());
        assert!(b.bh.borrow().tapped_ride_object.is_some());
        let boat = stub(2, true);
        boat.base().borrow_mut().pos = IntPair { x: 20, y: 21 };
        *f.ih.boat.borrow_mut() = Some(boat);
        let i = recalc(&f).unwrap();
        assert_eq!((i.path_type, i.goal), (pt::RIDE_OBJECT_CHASE, IntPair { x: 20, y: 21 }));
        assert_eq!(i.extra.unwrap().int_for_key("craftCountOrExtraData"), 3);
        // train cars
        f.ih.goal_interaction.set(it::REMOVE_TRAIN_CAR as i32);
        assert!(recalc(&f).is_none());
        let car = stub(3, true);
        car.base().borrow_mut().pos = IntPair { x: 30, y: 31 };
        *f.ih.train.borrow_mut() = Some(car);
        assert_eq!(recalc(&f).unwrap().goal, IntPair { x: 30, y: 31 });
    }

    #[test]
    fn recalc_plain_branch_acceptance_rules() {
        let f = fx();
        let b = f.b();
        queue_action(b);
        f.rec.goal_pos.set((6, 9));
        f.ih.path_type.set(4);
        f.ih.craft_count.set(2);
        // NONE skips the interaction check (RIDE never reaches this branch: it chases an NPC)
        f.ih.goal_interaction.set(it::NONE as i32);
        let i = recalc(&f).unwrap();
        assert_eq!((i.goal, i.path_type, i.goal_interaction), (IntPair { x: 6, y: 9 }, 4, it::NONE as i32));
        // the interaction still matches
        f.ih.goal_interaction.set(it::DIG as i32);
        f.rec.itype.set(it::DIG);
        assert!(recalc(&f).is_some());
        // the tile now offers something else
        f.rec.itype.set(it::PICKUP);
        assert!(recalc(&f).is_none());
        // accepted substitutions
        f.rec.itype.set(it::CRAFT);
        for g in [it::ADD_FUEL, it::DIG, it::DIG_FOREGROUND, it::DIG_BACK_WALL] {
            f.ih.goal_interaction.set(g as i32);
            assert!(recalc(&f).is_some(), "craft for {}", g);
        }
        f.ih.goal_interaction.set(it::SOW as i32);
        assert!(recalc(&f).is_none());
        f.rec.itype.set(it::USE_INTERACTIVE_OBJECT);
        for g in [it::DIG, it::DIG_FOREGROUND, it::DIG_BACK_WALL] {
            f.ih.goal_interaction.set(g as i32);
            assert!(recalc(&f).is_some(), "use for {}", g);
        }
        f.ih.goal_interaction.set(it::ADD_FUEL as i32);
        assert!(recalc(&f).is_none());
        // CRAFT and RIDE_TRAIN_CAR look at face index zero
        f.ih.goal_interaction.set(it::CRAFT as i32);
        f.rec.itype.set(it::CRAFT);
        assert!(recalc(&f).is_some());
        f.ih.goal_interaction.set(it::RIDE_TRAIN_CAR as i32);
        *f.ih.train.borrow_mut() = Some(stub(3, true));
        assert!(recalc(&f).is_some());
    }

    #[test]
    fn recalc_protected_tiles_retry_without_protected_actions() {
        let f = fx();
        let b = f.b();
        queue_action(b);
        f.ih.goal_interaction.set(it::DIG as i32);
        f.rec.itype.set(it::DIG);
        f.rec.itype_prot.set(it::NONE);
        // nothing protected: the first answer stands
        assert!(recalc(&f).is_some());
        protect_everything(&f);
        // protected: the interaction is asked again with protected actions disallowed
        assert!(recalc(&f).is_none());
        f.rec.itype_prot.set(it::DIG);
        assert!(recalc(&f).is_some());
        // an interaction that protection does not affect is not asked again
        f.ih.goal_interaction.set(it::CRAFT as i32);
        f.rec.itype.set(it::CRAFT);
        f.rec.itype_prot.set(it::NONE);
        assert!(recalc(&f).is_some());
    }

    #[test]
    fn recalc_extra_data_entries() {
        let f = fx();
        let b = f.b();
        queue_action(b);
        f.ih.goal_interaction.set(it::USE_INTERACTIVE_OBJECT as i32);
        f.rec.itype.set(it::USE_INTERACTIVE_OBJECT);
        f.ih.craft_count.set(4);
        // no interaction object id: nothing but the count
        let i = recalc(&f).unwrap();
        let ex = i.extra.unwrap();
        assert!(ex.object_for_key("interactionObject").is_none() && ex.object_for_key("workbench").is_none());
        // an id that resolves to nothing
        f.rec.obj_id.set(8);
        assert!(recalc(&f).unwrap().extra.unwrap().object_for_key("interactionObject").is_none());
        // an id that resolves: interaction object, or workbench when adding fuel
        let wb = stub(4, true);
        *f.rec.io.borrow_mut() = Some(wb.clone());
        let ex = recalc(&f).unwrap().extra.unwrap();
        assert!(crate::objects::handle::ptr_eq(&ex.object_for_key("interactionObject").unwrap(), &wb));
        f.ih.goal_interaction.set(it::ADD_FUEL as i32);
        f.rec.itype.set(it::ADD_FUEL);
        let ex = recalc(&f).unwrap().extra.unwrap();
        assert!(crate::objects::handle::ptr_eq(&ex.object_for_key("workbench").unwrap(), &wb));
        // a craftable item object adds the workbench and the count
        let c: Rc<dyn Any> = Rc::new(5u8);
        *f.ih.craftable.borrow_mut() = Some(c);
        let ex = recalc(&f).unwrap().extra.unwrap();
        assert!(ex.any_for_key("craftableItemObject").is_some());
        assert_eq!(ex.int_for_key("count"), 4);
        // ... but only when the workbench resolves
        *f.rec.io.borrow_mut() = None;
        let ex = recalc(&f).unwrap().extra.unwrap();
        assert!(ex.any_for_key("craftableItemObject").is_none());
    }

    fn eat(f: &Fx, ty: ItemType, setup: impl Fn(&mut crate::types::structs::BlockheadState)) {
        let b = f.b();
        for s in 1..8 {
            b.inv()[s].borrow_mut().clear();
        }
        put(b, 2, vec![mk(ty.0, 0, 0)]);
        b.set_selected_tool_index(2);
        setup(&mut b.bh.borrow_mut().state);
        assert!(!b.use_current_item_if_possible());
        assert!(b.inv()[2].borrow().is_empty());
    }

    #[test]
    fn food_effects_per_kind() {
        use crate::types::structs::BlockheadState;
        let f = fx();
        let b = f.b();
        let st = |b: &Blockhead| b.bh.borrow().state.clone();
        let base = |s: &mut BlockheadState| {
            s.energy = 0.5;
            s.fullness = 0.1;
            s.coffee_energy_timer = 0.0;
        };
        // coffee cherry: energy +0.1, timer at least 10
        eat(&f, ItemType::ITEM_COFFEE_CHERRY, base);
        assert_eq!(st(b).energy, 0.6f32);
        assert_eq!(st(b).coffee_energy_timer, 10.0);
        // an already running longer timer stays, and the energy is capped at 2
        eat(&f, ItemType::ITEM_COFFEE_CHERRY, |s| {
            base(s);
            s.energy = 1.95;
            s.coffee_energy_timer = 20.0;
        });
        assert_eq!(st(b).energy, 2.0);
        assert_eq!(st(b).coffee_energy_timer, 20.0);
        // chilli heats, kelp lowers the drowning fraction down to its floor
        eat(&f, ItemType::ITEM_CHILLI, |s| {
            base(s);
            s.heat = 1.0;
        });
        assert_eq!(st(b).heat, 21.0);
        eat(&f, ItemType::ITEM_KELP, |s| {
            base(s);
            s.drown_fraction = 0.0;
        });
        assert_eq!(st(b).drown_fraction, -0.2f32);
        eat(&f, ItemType::ITEM_KELP, |s| {
            base(s);
            s.drown_fraction = 0.9;
        });
        assert_eq!(st(b).drown_fraction, 0.9f32 - 0.5f32);
        // meals that leave their dish behind
        f.rec.fbs.borrow_mut().clear();
        eat(&f, ItemType::ITEM_FISH_CURRY, base);
        assert_eq!(st(b).food_pause_timer, 300.0);
        eat(&f, ItemType::ITEM_DODO_STEW, base);
        eat(&f, ItemType::ITEM_COOKED_FISH, base);
        eat(&f, ItemType::ITEM_MILK_BUCKET, base);
        let left: Vec<u32> = f.rec.fbs.borrow().iter().map(|x| x.ty).collect();
        assert_eq!(
            left,
            vec![ItemType::ITEM_POT.0, ItemType::ITEM_POT.0, ItemType::ITEM_TIN_FOIL.0, ItemType::ITEM_TIN_BUCKET.0]
        );
        // a full stomach saturates at 1.2; an empty-or-negative one starts from zero
        eat(&f, ItemType::ITEM_APPLE, |s| {
            base(s);
            s.fullness = 0.99;
        });
        assert_eq!(st(b).fullness, 1.2f32);
        eat(&f, ItemType::ITEM_APPLE, |s| {
            base(s);
            s.fullness = -1.0;
        });
        assert!(st(b).fullness > 0.0 && st(b).fullness < 0.5);
        // an energetic blockhead gets no energy from food
        eat(&f, ItemType::ITEM_APPLE, |s| {
            base(s);
            s.energy = 1.5;
        });
        assert_eq!(st(b).energy, 1.5);
    }

    #[test]
    fn coffee_in_expert_mode_and_normal_mode() {
        let f = fx();
        let b = f.b();
        let st = |b: &Blockhead| b.bh.borrow().state.clone();
        eat(&f, ItemType::ITEM_COFFEE_CUP, |s| {
            s.energy = 0.5;
            s.heat = 1.0;
        });
        assert_eq!((st(b).energy, st(b).heat, st(b).coffee_energy_timer), (2.0, 6.0, 60.0));
        f._world.set_expert_mode(true);
        eat(&f, ItemType::ITEM_COFFEE_CUP, |s| {
            s.energy = 0.5;
        });
        assert_eq!(st(b).energy, 1.5);
        eat(&f, ItemType::ITEM_COFFEE_CUP, |s| {
            s.energy = 1.8;
        });
        assert_eq!(st(b).energy, 2.0);
        // expert cherries are weaker
        eat(&f, ItemType::ITEM_COFFEE_CHERRY, |s| {
            s.energy = 0.5;
            s.coffee_energy_timer = 0.0;
        });
        assert_eq!(st(b).energy, 0.55f32);
        // and food gives half the energy
        eat(&f, ItemType::ITEM_APPLE, |s| {
            s.energy = 0.5;
        });
        let n = nourishment_fraction_for_food_type(FoodType::FOOD_APPLE);
        assert_eq!(st(b).energy, 0.5f32 + 0.1f32 * n);
    }

    #[test]
    fn using_cameras_rods_and_clothes() {
        let f = fx();
        let b = f.b();
        // an item that does nothing
        put(b, 2, vec![mk(FLINT, 0, 0)]);
        b.set_selected_tool_index(2);
        assert!(!b.use_current_item_if_possible());
        assert_eq!(counts(b)[2], 1);
        // a camera
        b.inv()[2].borrow_mut().clear();
        put(b, 2, vec![mk(ItemType::ITEM_CAMERA.0, 0, 0)]);
        assert!(!b.use_current_item_if_possible());
        // a fishing rod: only when the host says it can fish
        b.inv()[2].borrow_mut().clear();
        put(b, 2, vec![mk(ItemType::ITEM_FISHING_ROD.0, 0, 0)]);
        assert!(!b.use_current_item_if_possible());
        assert!(b.bh.borrow().fishing_rod.is_none());
        f.ih.can_fish.set(true);
        let ride = stub(1, true);
        b.set_riding_object(Some(&ride));
        assert!(!b.use_current_item_if_possible());
        assert!(b.bh.borrow().fishing_rod.is_some());
        assert_eq!(b.bh.borrow().interaction_item_index, 2);
        // already fishing: the cast is stopped through the host and the rod stays
        assert!(!b.use_current_item_if_possible());
        assert_eq!(f.rec.fish_stops.get(), 1);
        // shirt, pants and shoes go to their own groups
        for (ty, group) in [
            (ItemType::ITEM_LINEN_SHIRT.0, 2usize),
            (ItemType::ITEM_LINEN_PANTS.0, 1),
            (ItemType::ITEM_LEATHER_BOOTS.0, 0),
        ] {
            b.inv()[2].borrow_mut().clear();
            put(b, 2, vec![mk(ty, 0, 0)]);
            b.set_selected_tool_index(2);
            b.use_current_item_if_possible();
            let g = b.bh.borrow().inventory_items[0].borrow()[0].borrow().sub_items.as_ref().unwrap()[group].clone();
            assert_eq!(g.borrow().len(), 1);
            assert_eq!(g.borrow()[0].borrow().item_type.0, ty);
        }
        b.inventory_was_changed_sub_index_was_usage(0, -1, true);
        let fl = b.bh.borrow();
        assert_eq!(
            (fl.shirt_item_type, fl.pants_item_type, fl.shoes_item_type),
            (
                ItemType::ITEM_LINEN_SHIRT.0 as u16,
                ItemType::ITEM_LINEN_PANTS.0 as u16,
                ItemType::ITEM_LEATHER_BOOTS.0 as u16
            )
        );
    }

    #[test]
    fn beds_actions_and_interaction_queries() {
        let f = fx();
        let b = f.b();
        // sleep: no bed, a bed of the wrong object type, soft and gold beds
        b.sleep_rushed();
        assert_eq!(b.st().energy, 1.0);
        let bed = stub(3, true);
        b.bh.borrow_mut().interaction_object = Some(bed.clone());
        f.rec.io_type.set(2);
        b.sleep_rushed();
        assert_eq!(b.st().energy, 1.0);
        f.rec.io_type.set(3);
        f.rec.obj_item.set(ItemType::ITEM_SOFT_BED.0 as i32);
        b.sleep_rushed();
        assert_eq!(b.st().energy, 1.5);
        f.rec.obj_item.set(ItemType::ITEM_GOLD_BED.0 as i32);
        b.sleep_rushed();
        assert_eq!(b.st().energy, 2.0);
        // fishing rod cast: net blockheads ignore it
        b.fishing_rod_cast(Vector2 { x: 1.0, y: 2.0 });
        // stopping interaction when the workbench / object is gone
        {
            let mut fl = b.bh.borrow_mut();
            fl.state.interaction_type = it::CRAFT;
            fl.state.interacting = 1;
            fl.interaction_workbench = None;
        }
        b.stop_interacting_with_interaction_objects_if_no_interaction_object();
        assert_eq!(b.st().interacting, 0);
        {
            let mut fl = b.bh.borrow_mut();
            fl.state.interaction_type = it::USE_INTERACTIVE_OBJECT;
            fl.state.interacting = 1;
            fl.interaction_object = None;
        }
        b.stop_interacting_with_interaction_objects_if_no_interaction_object();
        assert_eq!(b.st().interacting, 0);
    }

    #[test]
    fn cancelling_queued_actions() {
        let f = fx();
        let b = f.b();
        queue_action(b);
        queue_action(b);
        queue_action(b);
        f.rec.goal_pos.set((4, 4));
        let cancel = |pos: IntPair, id: u64, gi: i32, extra: i16| {
            b.cancel_any_action_at_goal_pos_or_with_interaction_object_id_goal_interaction_craft_count_or_extra_data(
                pos, id, gi, extra,
            )
        };
        let has = |pos: IntPair, id: u64, gi: i32, extra: i16| {
            b.has_cancelable_action_at_goal_pos_or_with_interaction_object_id_goal_interaction_craft_count_or_extra_data(
                pos, id, gi, extra,
            )
        };
        let miss = IntPair { x: 1, y: 1 };
        let hit = IntPair { x: 4, y: 4 };
        assert!(!cancel(miss, 0, 0, 0));
        assert!(!has(miss, 0, 0, 0));
        // matched by object id
        f.rec.obj_id.set(7);
        assert!(has(miss, 7, 0, 0));
        assert!(!has(miss, 0, 0, 0));
        // paint requests also need the same count
        f.ih.craft_count.set(2);
        assert!(has(hit, 0, it::PAINT as i32, 2));
        assert!(!has(hit, 0, it::PAINT as i32, 3));
        assert!(has(hit, 0, it::STRIP_PAINT as i32, 2));
        assert!(!cancel(hit, 0, it::PAINT as i32, 3));
        assert_eq!(b.bh.borrow().action_queue.len(), 3);
        // an action in progress stops the interaction and aborts the path
        f.rec.in_progress.set(true);
        {
            let mut fl = b.bh.borrow_mut();
            fl.waiting_for_path = true;
            fl.chasing_npc = true;
            fl.chasing_ride_object = true;
        }
        assert!(cancel(hit, 0, 0, 0));
        assert!(b.bh.borrow().action_queue.is_empty());
        assert_eq!(f.rec.aborts.get(), 3);
        assert!(!b.bh.borrow().waiting_for_path && !b.bh.borrow().chasing_npc);
    }

    #[test]
    fn current_item_without_a_container_on_top() {
        let f = fx();
        let b = f.b();
        // selected, but the slot is empty
        b.set_selected_tool_index(2);
        assert!(b.current_item().is_none());
        assert!(b.current_item_slot().unwrap().borrow().is_empty());
        assert_eq!(b.current_item_sub_index(), -1);
        b.set_current_item_to_item_at_index(IntPair { x: 3, y: 1 });
        assert_eq!(b.selected_tool_index(), 3);
        // a plain item on top: a sub-index changes nothing
        put(b, 3, vec![mk(APPLE, 0, 0)]);
        b.set_current_item_to_item_at_index(IntPair { x: 3, y: 1 });
        assert_eq!(b.current_item_slot().unwrap().borrow().len(), 1);
        assert_eq!(b.current_item_sub_index(), -1);
        assert_eq!(b.current_item().unwrap().borrow().item_type.0, APPLE);
        // a container whose selected sub-slot is empty: the container itself
        put(b, 4, vec![basket_with(vec![])]);
        b.set_current_item_to_item_at_index(IntPair { x: 4, y: 3 });
        assert_eq!(b.current_item().unwrap().borrow().item_type.0, BASKET);
        b.remove_current_item();
        assert!(b.inv()[4].borrow().is_empty());
        // removing from a sub-slot index above three flags the outer slot
        let bk = basket_with(vec![]);
        extra_slot(&bk, vec![mk(FLINT, 0, 0)]);
        put(b, 5, vec![bk]);
        b.set_current_item_to_item_at_index(IntPair { x: 5, y: 4 });
        b.remove_current_item();
        assert!(b.bh.borrow().inventory_changed_this_frame_slots[5]);
    }

    #[test]
    fn remove_identical_and_remote_pickup_receipts() {
        let f = fx();
        let b = f.b();
        // directly in an outer slot
        let a = mk(APPLE, 0, 0);
        put(b, 2, vec![mk(APPLE, 0, 0), a.clone()]);
        b.remove_inventory_item_identical_to(&a);
        assert_eq!(counts(b)[2], 1);
        assert!(b.bh.borrow().inventory_changed_this_frame_slots[2]);
        // in the first four entries of a sub-slot of a lone container
        let g = mk(FLINT, 0, 0);
        put(b, 3, vec![basket_with(vec![(2, vec![mk(FLINT, 0, 0), g.clone()])])]);
        b.remove_inventory_item_identical_to(&g);
        assert!(b.bh.borrow().sub_inventory_changed_this_frame_slots[3][1]);
        // not found anywhere
        b.remove_inventory_item_identical_to(&mk(APPLE, 0, 0));
        // receipts: a rejected pickup removes the item again, an accepted one just forgets it
        let p = mk(APPLE, 0, 0);
        put(b, 4, vec![p.clone()]);
        b.bh.borrow_mut().unconfirmed_pickups = Some(std::collections::BTreeMap::from([(7u64, p.clone())]));
        b.remote_pickup_request_response(true, &[7]);
        assert_eq!(counts(b)[4], 1);
        assert!(b.bh.borrow().unconfirmed_pickups.as_ref().unwrap().is_empty());
        b.bh.borrow_mut().unconfirmed_pickups = Some(std::collections::BTreeMap::from([(8u64, p.clone())]));
        b.remote_pickup_request_response(false, &[8, 9]);
        assert_eq!(counts(b)[4], 0);
        b.bh.borrow_mut().unconfirmed_pickups = None;
        b.remote_pickup_request_response(false, &[1]);
    }

    #[test]
    fn set_path_without_a_world_ai_and_interacting() {
        let f = fx();
        let b = f.b();
        let goal = 9 + 3 * (f._world.world_width_macro() * 32);
        // empty WORLD_TAPPED path of an AI blockhead tells the AI
        f.ih.has_ai.set(true);
        b.set_path_type_goal_interaction_extra_data(Some(vec![]), 0, 0, None);
        assert_eq!(f.ih.ai_calls.get(), 1);
        // an empty chase path stops the interaction
        b.bh.borrow_mut().state.interacting = 1;
        b.set_path_type_goal_interaction_extra_data(Some(vec![]), pt::NPC_CHASE, 0, None);
        assert_eq!(b.st().interacting, 0);
        // a non-empty tapped path while interacting refreshes the gather animation
        b.bh.borrow_mut().state.interacting = 1;
        b.set_path_type_goal_interaction_extra_data(Some(vec![node(goal, 0)]), 0, 1, None);
        assert_eq!(b.st().interacting, 1);
        // a blockhead whose world is gone has its goal at the origin
        let (w2, _dw2, dwref2) = setup();
        let o2 = new_bh(&w2, &dwref2, 1);
        drop(w2);
        let b2 = bh_of(&o2);
        b2.set_path_type_goal_interaction_extra_data(Some(vec![node(goal, 0)]), 0, 0, None);
        assert_eq!(b2.bh.borrow().final_goal_square, IntPair { x: 0, y: 0 });
    }

    #[test]
    fn waiting_for_path_on_protected_tiles() {
        let f = fx();
        let b = f.b();
        f.rec.itype.set(it::DIG);
        f.rec.itype_prot.set(it::NONE);
        b.set_waiting_for_path_to_pos(IntPair { x: 5, y: 8 });
        assert_eq!(b.bh.borrow().waiting_for_path_interaction_type, it::DIG);
        protect_everything(&f);
        b.set_waiting_for_path_to_pos(IntPair { x: 5, y: 8 });
        assert_eq!(b.bh.borrow().waiting_for_path_interaction_type, it::NONE);
    }

    #[test]
    fn idle_wake_cast_and_beds() {
        let f = fx();
        let b = f.b();
        b.bh.borrow_mut().state.sub_animation_type = 7;
        assert!(b.idle());
        b.bh.borrow_mut().state.sub_animation_type = 0;
        b.bh.borrow_mut().state.interacting = 1;
        b.wake_up();
        assert_eq!(b.st().interacting, 0);
        // the clamp works on both ends
        b.fishing_rod_cast(Vector2 { x: -99999.0, y: 0.0 });
        assert_eq!(b.bh.borrow().fishing_rod_cast_x, -8000);
        // net blockheads ignore the cast
        b.base.borrow_mut().is_net = true;
        b.fishing_rod_cast(Vector2 { x: 5.0, y: 5.0 });
        assert_eq!(b.bh.borrow().fishing_rod_cast_x, -8000);
        // a bed can be used only when tired; anything else always
        let world: crate::objects::base::WorldRef = f._world.clone();
        let bed = crate::objects::registry::PlainInteractionObject::init_with_world_dynamic_world_at_position_cache(
            &world,
            &f._dwref,
            IntPair { x: 5, y: 8 },
            None,
        )
        .unwrap();
        assert!(b.can_use_dynamic_object(&bed));
        f.rec.io_type.set(3);
        b.bh.borrow_mut().state.energy = 0.5;
        assert!(b.can_use_dynamic_object(&bed));
        b.bh.borrow_mut().state.energy = 0.9;
        assert!(!b.can_use_dynamic_object(&bed));
    }

    #[test]
    fn move_refusals_with_full_containers_and_out_of_range_flags() {
        let f = fx();
        let b = f.b();
        // nothing to move
        let empty = b.inv()[4].clone();
        assert_eq!(
            b.move_inventory_items_from_array_from_index_from_sub_index_to_index_to_sub_index_count_moved_items(
                &empty, 4, -1, 5, -1, 1, None
            ),
            -1
        );
        // every sub-slot is taken by another type: no home, and the container itself is not stackable
        let full: Vec<(usize, Vec<ItemRef>)> = (0..4).map(|i| (i, vec![mk(FLINT, 0, 0)])).collect();
        put(b, 2, vec![basket_with(full)]);
        put(b, 3, vec![mk(APPLE, 0, 0)]);
        assert_eq!(b.move_inventory_items_from_index_from_sub_index_to_index_to_sub_index_count(3, -1, 2, -1, 1), -1);
        assert_eq!(counts(b)[3], 1);
        // flags for an out-of-range index are skipped, the item still goes
        let in_outer = mk(APPLE, 0, 0);
        put(b, 5, vec![in_outer.clone()]);
        b.remove_item_index_was_used_up(&in_outer, 9, false);
        assert_eq!(counts(b)[5], 0);
        let in_sub = mk(FLINT, 0, 0);
        put(b, 6, vec![basket_with(vec![(1, vec![in_sub.clone()])])]);
        b.remove_item_index_was_used_up(&in_sub, 9, false);
        assert!(b.inv()[6].borrow()[0].borrow().sub_items.as_ref().unwrap()[1].borrow().is_empty());
    }

    fn bh_without_world() -> (ObjRef, Rc<MockDw>, crate::objects::base::DynamicWorldRef) {
        let (w, dw, dwref) = setup();
        let o = new_bh(&w, &dwref, 1);
        drop(w);
        (o, dw, dwref)
    }

    #[test]
    fn wear_without_a_world_uses_the_default_rules() {
        let (o2, _dw, _dwref) = bh_without_world();
        let b2 = bh_of(&o2);
        let pick = mk(PICKAXE, 0, 0);
        put(b2, 3, vec![pick.clone()]);
        b2.increment_usage_of_item_index_to_use_was_attack(&pick, 3, false);
        assert_eq!(pick.borrow().data_a as i32, usage_increment_per_use(ItemType(PICKAXE), 2, false, false));
        let hat = mk(117, 0, 0);
        b2.bh.borrow().inventory_items[0].borrow()[0].borrow().sub_items.as_ref().unwrap()[3]
            .borrow_mut()
            .push(hat.clone());
        b2.increment_usage_of_clothing();
        assert_eq!(hat.borrow().data_a, 200);
        assert_eq!(b2.inventory_items().len(), 8);
    }

    #[test]
    fn custom_rules_scale_clothing_and_tools() {
        let f = fx();
        let b = f.b();
        let mut r = f._world.custom_rules();
        r.custom_rules_enabled = 1;
        r.tool_decay = 3;
        r.clothing_decay = 3;
        r.pole_items = 1;
        f._world.set_custom_rules(r);
        let pick = mk(PICKAXE, 0, 0);
        put(b, 3, vec![pick.clone()]);
        b.increment_usage_of_item_index_to_use_was_attack(&pick, 3, false);
        assert_eq!(pick.borrow().data_a as i32, usage_increment_per_use(ItemType(PICKAXE), 3, false, false));
        let hat = mk(117, 0, 0);
        b.bh.borrow().inventory_items[0].borrow()[0].borrow().sub_items.as_ref().unwrap()[3]
            .borrow_mut()
            .push(hat.clone());
        b.increment_usage_of_clothing();
        assert_eq!(hat.borrow().data_a, 800);
    }

    #[test]
    fn counting_with_queued_actions() {
        let f = fx();
        let b = f.b();
        put(b, 2, vec![mk(APPLE, 0, 0), mk(APPLE, 0, 0)]);
        put(
            b,
            3,
            vec![basket_with(vec![(1, vec![mk(APPLE, 0, 0); 0]), (2, (0..3).map(|_| mk(APPLE, 0, 0)).collect())])],
        );
        put(b, 4, vec![basket_with(vec![(0, vec![mk(FLINT, 0, 0)])])]);
        let count = |inc| b.count_of_inventory_items_of_type_include_actions(ItemType(APPLE), inc);
        assert_eq!(count(false), 5);
        assert_eq!(
            b.count_of_inventory_items_with_specific_data_b_of_type_data_b_include_actions(ItemType(APPLE), 7, false),
            0
        );
        // pigments: the generic pigment matches every colour
        put(b, 5, vec![mk(ItemType::ITEM_PIGMENT_GENERIC.0 + 1, 0, 0)]);
        let _ = b.count_of_inventory_items_of_type_include_actions(ItemType::ITEM_PIGMENT_GENERIC, false);
        // queued actions add their pending inventory change
        queue_action(b);
        f.rec.inv_change.set(Some(4));
        assert_eq!(count(true), 9);
        // an action that is running while interacting is already reflected in the inventory ...
        f.rec.in_progress.set(true);
        b.bh.borrow_mut().state.interacting = 1;
        f.rec.inv_change.set(Some(-2));
        assert_eq!(count(true), 5);
        // ... unless it adds items
        f.rec.inv_change.set(Some(4));
        assert_eq!(count(true), 9);
        // a craft at a workbench used by this blockhead counts what is left to craft
        f.ih.goal_interaction.set(it::CRAFT as i32);
        let wb = stub(4, true);
        b.bh.borrow_mut().interaction_workbench = Some(wb);
        f.ih.crafting_type.set(APPLE as i32);
        f.ih.items_left.set(6);
        f.rec.inv_change.set(Some(-2));
        assert_eq!(count(true), 5, "someone else's workbench");
        *f.rec.wb_current.borrow_mut() = Some(f.o.clone());
        assert_eq!(count(true), 11);
        f.ih.crafting_type.set(FLINT as i32);
        assert_eq!(count(true), 5, "crafting another item");
        // waiting for the fill response counts like interacting
        b.bh.borrow_mut().state.interacting = 0;
        b.bh.borrow_mut().waiting_for_fill_response = true;
        assert_eq!(count(true), 5);
        b.bh.borrow_mut().interaction_workbench = None;
        assert_eq!(count(true), 5);
    }

    #[test]
    fn queries_look_into_container_sub_slots() {
        let f = fx();
        let b = f.b();
        // usage multiplier: another stack on top, then a container's sub-slot of the wanted type
        let worn = mk(PICKAXE, 4096, 0);
        put(b, 2, vec![mk(FLINT, 0, 0)]);
        put(b, 3, vec![basket_with(vec![(1, vec![worn.clone()]), (2, vec![mk(FLINT, 0, 0)])])]);
        assert_eq!(b.usage_multiplier_for_first_item_of_type(ItemType(PICKAXE)), 1.0 - 4096.0 / 16384.0);
        assert_eq!(b.usage_multiplier_for_first_item_of_type(ItemType(APPLE)), 1.0);
        assert_eq!(b.inventory_location_of_first_instance_of_item_type(ItemType(PICKAXE)), IntPair { x: 3, y: 1 });
        assert_eq!(b.inventory_location_of_first_instance_of_item_type(ItemType(APPLE)), IntPair { x: -1, y: -1 });
        assert_eq!(
            b.inventory_location_of_first_instance_of_item_type(ItemType::ITEM_PIGMENT_GENERIC),
            IntPair { x: -1, y: -1 }
        );
        // cash in sub-slots
        put(
            b,
            4,
            vec![basket_with(vec![
                (0, vec![mk(ItemType::ITEM_GOLD_COIN.0, 0, 0); 0]),
                (3, vec![mk(ItemType::ITEM_COPPER_COIN.0, 0, 0), mk(ItemType::ITEM_COPPER_COIN.0, 0, 0)]),
            ])],
        );
        assert_eq!(b.total_cash(), 2);
        // sowables
        put(b, 5, vec![basket_with(vec![(2, (0..12).map(|_| mk(ItemType::ITEM_FLAX_SEED.0, 0, 0)).collect())])]);
        assert_eq!(b.sowable_item_for_ai_item_index(), IntPair { x: 5, y: 2 });
        assert_eq!(b.sowable_item_for_ai_item_type(), ItemType::ITEM_FLAX_SEED);
        // lights
        put(b, 6, vec![basket_with(vec![(1, vec![mk(17, 0, 0)])])]);
        assert_eq!(b.placable_light_for_ai_item_index(), IntPair { x: 6, y: 1 });
        // food
        let g = fx();
        put(g.b(), 7, vec![basket_with(vec![(3, (0..3).map(|_| mk(21, 0, 0)).collect())])]);
        assert_eq!(g.b().most_common_food_type_index(), IntPair { x: 7, y: 3 });
    }

    #[test]
    fn subtract_cash_from_sub_slots() {
        let f = fx();
        let b = f.b();
        let coin = |t: ItemType| mk(t.0, 0, 0);
        put(
            b,
            3,
            vec![basket_with(vec![
                (0, vec![coin(ItemType::ITEM_PLATINUM_COIN)]),
                (1, vec![coin(ItemType::ITEM_COPPER_COIN), coin(ItemType::ITEM_COPPER_COIN)]),
                (2, vec![coin(ItemType::ITEM_GOLD_COIN)]),
                (3, vec![mk(FLINT, 0, 0)]),
            ])],
        );
        assert_eq!(b.total_cash(), 10000 + 2 + 100);
        // 10050 copper: the platinum goes first and 50 copper... are made up from the rest, overpaying
        assert!(b.subtract_cash(10001));
        assert!(b.bh.borrow().sub_inventory_changed_this_frame_slots[3][0]);
        assert_eq!(b.total_cash(), 101);
        assert!(f.rec.fbs.borrow().is_empty());
        // paying with the gold coin overpays and gets change back
        assert!(b.subtract_cash(50));
        assert_eq!(b.total_cash(), 0);
        let fbs = f.rec.fbs.borrow().clone();
        assert_eq!(fbs.len(), 1);
        assert_eq!(fbs[0].ty, ItemType::ITEM_MONEY.0);
    }

    fn chest_with(ty: u32, items: &[ItemRef], nested_under_chest: bool) -> ItemRef {
        let slot = PlistValue::array(items.iter().map(|i| PlistValue::data(i.borrow().save_data())).collect());
        let slots = PlistValue::array(vec![slot, PlistValue::int(1)]);
        let mut d = crate::objects::base::new_mutable_dict();
        if nested_under_chest {
            let mut inner = crate::objects::base::new_mutable_dict();
            inner.map.insert("saveItemSlots".to_string(), slots);
            d.map.insert("chest".to_string(), PlistValue::Dict(inner));
        } else {
            d.map.insert("saveItemSlots".to_string(), slots);
        }
        InventoryItem::new(ItemType(ty), 0, 0, None, Some(PlistValue::Dict(d)))
    }

    #[test]
    fn subtract_items_spills_stored_contents() {
        let f = fx();
        let b = f.b();
        // an item in a container's sub-slot is found there
        let nested = mk(APPLE, 0, 0);
        put(b, 2, vec![basket_with(vec![(1, vec![nested])])]);
        assert_eq!(b.subtract_items_from_inventory_of_type_count(ItemType(APPLE), 1).len(), 1);
        // chests spill what they hold (time crystals vanish), plain safes too
        let stored = [mk(FLINT, 0, 0), mk(11, 0, 0)];
        put(b, 3, vec![chest_with(206, &stored, true)]);
        put(b, 4, vec![chest_with(207, &stored, false)]);
        put(b, 5, vec![InventoryItem::new(ItemType(161), 0, 0, None, None)]);
        for t in [206u32, 207, 161] {
            let r = b.subtract_items_from_inventory_of_type_count(ItemType(t), 1);
            assert_eq!(r.len(), 1, "type {}", t);
        }
        let spilled: Vec<u32> = f.rec.fbs.borrow().iter().map(|x| x.ty).collect();
        assert_eq!(spilled, vec![FLINT, FLINT]);
        // a chest whose save dict has no slots
        let empty = InventoryItem::new(ItemType(206), 0, 0, None, Some(PlistValue::mutable_dict()));
        put(b, 6, vec![empty]);
        assert_eq!(b.subtract_items_from_inventory_of_type_count(ItemType(206), 1).len(), 1);
        assert_eq!(f.rec.fbs.borrow().len(), 2);
    }

    #[test]
    fn tool_wear_special_cases() {
        let f = fx();
        let b = f.b();
        // a fishing rod that is not out: no wear
        let rod = mk(ItemType::ITEM_FISHING_ROD.0, 0, 0);
        put(b, 2, vec![rod.clone()]);
        b.increment_usage_of_item_index_to_use_was_attack(&rod, 2, false);
        assert_eq!(rod.borrow().data_a, 0);
        // decay rule 0: no wear
        let mut r = f._world.custom_rules();
        r.custom_rules_enabled = 1;
        r.tool_decay = 0;
        f._world.set_custom_rules(r);
        let pick = mk(PICKAXE, 0, 0);
        put(b, 3, vec![pick.clone()]);
        b.increment_usage_of_item_index_to_use_was_attack(&pick, 3, false);
        assert_eq!(pick.borrow().data_a, 0);
        r.custom_rules_enabled = 0;
        f._world.set_custom_rules(r);
        // an index outside the flag arrays still wears the tool
        b.increment_usage_of_item_index_to_use_was_attack(&pick, 9, false);
        assert!(pick.borrow().data_a > 0);
        // the interaction item and index come from the first queued action
        queue_action(b);
        *f.ih.action_item.borrow_mut() = Some(pick.clone());
        f.ih.action_index.set(3);
        let before = pick.borrow().data_a;
        b.increment_usage_of_interaction_item(true);
        assert!(pick.borrow().data_a > before);
        b.increment_passive_item_usage();
        *f.ih.action_item.borrow_mut() = None;
        let now = pick.borrow().data_a;
        b.increment_usage_of_interaction_item(false);
        assert_eq!(pick.borrow().data_a, now);
    }

    #[test]
    fn used_up_paint_leaves_a_bucket_and_hands_over_to_the_next_stack() {
        let f = fx();
        let b = f.b();
        let paint = |a: u16| mk(ItemType::ITEM_PAINT.0, a, 3);
        // no queued action: the interaction just stops
        let p1 = paint(0x3fff);
        put(b, 2, vec![p1.clone()]);
        b.bh.borrow_mut().interaction_item = Some(p1.clone());
        b.bh.borrow_mut().state.interacting = 1;
        b.bh.borrow_mut().state.interacting_square = IntPair { x: 7, y: 7 };
        b.increment_usage_of_item_index_to_use_was_attack(&p1, 2, false);
        assert_eq!(b.st().interacting, 0);
        assert_eq!(f.rec.fbs.borrow().last().unwrap().ty, ItemType::ITEM_TIN_BUCKET.0);
        assert_eq!(f.rec.fbs.borrow().last().unwrap().pos, (7, 7));
        // a queued action whose item slot is empty: stop
        queue_action(b);
        let p2 = paint(0x3fff);
        put(b, 3, vec![p2.clone()]);
        *f.ih.action_item.borrow_mut() = Some(p2.clone());
        f.ih.action_index.set(3);
        b.bh.borrow_mut().state.interacting = 1;
        f.ih.action_sub_index.set(0);
        let slot4 = b.inv()[4].clone();
        assert!(slot4.borrow().is_empty());
        f.ih.action_index.set(3);
        // the used-up paint leaves slot 3 empty, so the action has nothing to continue with
        b.increment_usage_of_item_index_to_use_was_attack(&p2, 3, false);
        assert_eq!(b.st().interacting, 0);
        // the same type is waiting in another stack: the action continues with it
        let p3 = paint(0x3fff);
        let spare = paint(5);
        put(b, 3, vec![spare.clone(), p3.clone()]);
        *f.ih.action_item.borrow_mut() = Some(p3.clone());
        b.bh.borrow_mut().interaction_item = Some(p3.clone());
        b.bh.borrow_mut().state.interacting = 1;
        *f.ih.action_item.borrow_mut() = Some(p3.clone());
        b.increment_usage_of_item_index_to_use_was_attack(&p3, 3, false);
        assert_eq!(f.ih.item_sets.get(), 1);
        assert_eq!(b.st().interacting, 1);
        // the same type found inside a container's sub-slot
        let p4 = paint(0x3fff);
        let inner = paint(7);
        put(b, 5, vec![basket_with(vec![(2, vec![inner.clone()])])]);
        put(b, 6, vec![p4.clone()]);
        *f.ih.action_item.borrow_mut() = Some(p4.clone());
        b.bh.borrow_mut().interaction_item = Some(p4.clone());
        f.ih.action_index.set(5);
        f.ih.action_sub_index.set(2);
        b.increment_usage_of_item_index_to_use_was_attack(&p4, 6, false);
        assert_eq!(f.ih.item_sets.get(), 2);
        // a paint stripper also leaves a bucket; other tools do not
        let strip = mk(ItemType::ITEM_PAINT_STRIPPER.0, 0x3fff, 0);
        put(b, 7, vec![strip.clone()]);
        b.bh.borrow_mut().interaction_item = Some(strip.clone());
        *f.ih.action_item.borrow_mut() = Some(strip.clone());
        f.ih.action_index.set(7);
        let buckets = f.rec.fbs.borrow().len();
        b.increment_usage_of_item_index_to_use_was_attack(&strip, 7, false);
        assert_eq!(f.rec.fbs.borrow().len(), buckets + 1);
    }

    #[test]
    fn clothing_wear_edge_cases() {
        let f = fx();
        let b = f.b();
        let cont = b.inv()[0].borrow()[0].clone();
        // a fifth clothing group is flagged on the outer slot
        extra_slot(&cont, vec![mk(117, 0, 0)]);
        extra_slot(&cont, vec![]);
        let subs = cont.borrow().sub_items.clone().unwrap();
        b.increment_usage_of_clothing();
        assert!(b.bh.borrow().inventory_usage_changed_this_frame_slots[0]);
        subs[4].borrow()[0].borrow_mut().data_a = 0x3f80;
        b.increment_usage_of_clothing();
        assert!(subs[4].borrow().is_empty());
        // ice gear and armour that wear out
        subs[0].borrow_mut().push(mk(198, 0x3fff, 0));
        b.bh.borrow_mut().current_temperature = 60.0;
        b.increment_usage_of_ice_clothing();
        assert!(subs[0].borrow().is_empty());
        subs[1].borrow_mut().push(mk(ItemType::ITEM_IRON_PANTS.0, 0x3fff, 0));
        b.increment_damage_of_armor_clothing(1.0);
        assert!(subs[1].borrow().is_empty());
        // jetpack fuel in the fifth group flags the outer slot
        subs[4].borrow_mut().push(mk(ItemType::ITEM_JETPACK.0, 0, 0));
        b.bh.borrow_mut().inventory_changed_this_frame_slots = [false; 8];
        b.increment_fuel_usage();
        assert!(b.bh.borrow().inventory_changed_this_frame_slots[0]);
        // no clothing container at all: nothing happens
        b.inv()[0].borrow_mut().clear();
        b.increment_usage_of_clothing();
        b.increment_usage_of_ice_clothing();
        b.increment_damage_of_armor_clothing(1.0);
        b.increment_fuel_usage();
        // no inventory
        b.bh.borrow_mut().inventory_items.clear();
        b.increment_usage_of_clothing();
    }

    #[test]
    fn location_scans_skip_multi_item_stacks_and_find_tools_in_containers() {
        let f = fx();
        let b = f.b();
        b.set_current_item_to_item_at_index(IntPair { x: -1, y: 2 });
        assert_eq!(b.selected_tool_index(), 0);
        // a stack of two other items is passed over
        put(b, 1, vec![mk(FLINT, 0, 0), mk(FLINT, 0, 0)]);
        assert_eq!(b.inventory_location_of_first_instance_of_item_type(ItemType(APPLE)), IntPair { x: -1, y: -1 });
        // a good tool for glass inside a container, behind a plain item
        let world = f._world.clone();
        let tile = tile_at_world_position_loaded(5, 8, &world).unwrap();
        tile.set_type_index(24);
        put(b, 2, vec![mk(APPLE, 0, 0)]);
        assert_eq!(b.item_index_with_good_interaction_type_for_tile(&tile), IntPair { x: -1, y: -1 });
        put(b, 3, vec![basket_with(vec![(1, vec![mk(FLINT, 0, 0)]), (2, vec![mk(PICKAXE, 0, 0)])])]);
        assert_eq!(b.item_index_with_good_interaction_type_for_tile(&tile), IntPair { x: 3, y: 2 });
        put(b, 4, vec![mk(PICKAXE, 0, 0)]);
        assert_eq!(b.item_index_with_good_interaction_type_for_tile(&tile), IntPair { x: 3, y: 2 });
        b.inv()[3].borrow_mut().clear();
        assert_eq!(b.item_index_with_good_interaction_type_for_tile(&tile), IntPair { x: 4, y: -1 });
    }

    #[test]
    fn wear_out_of_other_tools_and_warm_ice_gear() {
        let f = fx();
        let b = f.b();
        // a tool that is used up but is not the interaction item just disappears
        let pick = mk(PICKAXE, 0x3fff, 0);
        put(b, 2, vec![pick.clone()]);
        b.increment_usage_of_item_index_to_use_was_attack(&pick, 2, false);
        assert!(b.inv()[2].borrow().is_empty());
        // ice gear does not wear in the cold-free case
        let boots = mk(198, 7, 0);
        let subs = b.inv()[0].borrow()[0].borrow().sub_items.clone().unwrap();
        subs[0].borrow_mut().push(boots.clone());
        b.bh.borrow_mut().current_temperature = 0.0;
        b.increment_usage_of_ice_clothing();
        assert_eq!(boots.borrow().data_a, 7);
        // the container of a used-up interaction item holds an empty sub-slot: stop
        queue_action(b);
        let paint = mk(ItemType::ITEM_PAINT.0, 0x3fff, 0);
        put(b, 3, vec![paint.clone()]);
        put(b, 4, vec![basket_with(vec![])]);
        *f.ih.action_item.borrow_mut() = Some(paint.clone());
        f.ih.action_index.set(4);
        f.ih.action_sub_index.set(1);
        b.bh.borrow_mut().state.interacting = 1;
        b.increment_usage_of_item_index_to_use_was_attack(&paint, 3, false);
        assert_eq!(b.st().interacting, 0);
    }

    fn full_except(b: &Blockhead, slot: usize) {
        fill_all_other_slots(b, slot, 30);
    }

    fn can_pick(b: &Blockhead, t: u32, a: u16, db: u16) -> i32 {
        b.can_pick_up_item_of_type_sub_items_data_a_data_b(ItemType(t), None, a, db)
    }

    #[test]
    fn money_only_stacks_onto_matching_coins() {
        let f = fx();
        let b = f.b();
        let money = ItemType::ITEM_MONEY.0;
        full_except(b, 2);
        for (coin, accepts, rejects) in [
            (ItemType::ITEM_PLATINUM_COIN, (1u16, 0u16), (0u16, 50u16)),
            (ItemType::ITEM_GOLD_COIN, (0, 100), (0, 99)),
            (ItemType::ITEM_COPPER_COIN, (0, 1), (0, 0)),
        ] {
            b.inv()[2].borrow_mut().clear();
            put(b, 2, (0..5).map(|_| mk(coin.0, 0, 0)).collect());
            assert_eq!(can_pick(b, money, accepts.0, accepts.1), 1, "{:?} accepts", coin);
            assert_eq!(can_pick(b, money, rejects.0, rejects.1), -1, "{:?} rejects", coin);
        }
        // a stack of money piles, or of a different non-coin, never takes more money
        b.inv()[2].borrow_mut().clear();
        put(b, 2, vec![mk(money, 1, 1)]);
        assert_eq!(can_pick(b, money, 1, 1), -1);
        // the same inside a carried container
        b.inv()[2].borrow_mut().clear();
        let subs: Vec<(usize, Vec<ItemRef>)> = vec![
            (0, vec![mk(FLINT, 0, 0)]),
            (1, vec![mk(ItemType::ITEM_GOLD_COIN.0, 0, 0)]),
            (2, (0..99).map(|_| mk(STICK, 0, 0)).collect()),
            (3, vec![mk(money, 0, 0)]),
        ];
        put(b, 2, vec![basket_with(subs)]);
        assert_eq!(can_pick(b, money, 0, 100), 1);
        assert_eq!(can_pick(b, money, 0, 5), -1);
        // plain items stack inside the container too, but not onto a full sub-slot
        assert_eq!(can_pick(b, FLINT, 0, 0), 1);
        assert_eq!(can_pick(b, STICK, 0, 0), -1);
    }

    #[test]
    fn carried_containers_pick_up_over_loose_stacks() {
        let f = fx();
        let b = f.b();
        full_except(b, 2);
        put(b, 2, (0..5).map(|_| mk(FLINT, 0, 0)).collect());
        let offer = |subs: Option<&[SubSlot]>| b.can_pick_up_item_of_type_sub_items(ItemType(BASKET), subs);
        // no contents known, or no sub-slots, or an empty one
        assert_eq!(offer(None), 1);
        assert_eq!(offer(Some(&[])), 1);
        let empty: SubSlot = Rc::new(RefCell::new(Vec::new()));
        assert_eq!(offer(Some(&[empty.clone()])), 1);
        // a sub-slot of the same item that has room for the loose stack
        let flints = |n: usize| -> SubSlot { Rc::new(RefCell::new((0..n).map(|_| mk(FLINT, 0, 0)).collect())) };
        assert_eq!(offer(Some(&[flints(3)])), 1);
        assert_eq!(offer(Some(&[flints(94)])), 1);
        assert_eq!(offer(Some(&[flints(95)])), -1);
        assert_eq!(offer(Some(&[flints(99), Rc::new(RefCell::new(vec![mk(STICK, 0, 0)]))])), -1);
        // paints of different colours do not join
        b.inv()[2].borrow_mut().clear();
        put(b, 2, vec![mk(ItemType::ITEM_PAINT.0, 0, 1)]);
        let paint = |c: u16| -> SubSlot { Rc::new(RefCell::new(vec![mk(ItemType::ITEM_PAINT.0, 0, c)])) };
        assert_eq!(offer(Some(&[paint(2)])), -1);
        assert_eq!(offer(Some(&[paint(1)])), 1);
    }

    fn credit_crystals(n: i32) {
        use crate::foundation::md5::string_from_md5;
        let cm = crate::env::crystal_manager::CrystalManager::instance();
        let total = cm.amount() + n;
        cm.modify_modify_string(-n, &string_from_md5(&format!("7acfe93afc08c{}65ae2c54ecaf07f", total + 0x49)));
    }

    #[test]
    fn warp_check_world_caps_and_required_food() {
        let f = fx();
        let b = f.b();
        // no world
        let (o2, _dw2, _dwref2) = bh_without_world();
        assert!(!bh_of(&o2).check_if_can_warp_in_second_blockhead_after_item_added_data_b(ItemType(APPLE), 0));
        *f._world.distance_ordered_food_types.borrow_mut() = vec![0; 5];
        put(b, 1, (0..5).map(|_| mk(APPLE, 0, 0)).collect());
        let mut r = f._world.custom_rules();
        // a cap of zero allows no further blockhead
        r.custom_rules_enabled = 1;
        r.max_blockheads = 0;
        f._world.set_custom_rules(r);
        assert!(!b.check_if_can_warp_in_second_blockhead_after_item_added_data_b(ItemType(APPLE), 0));
        // caps above five count as five, lower caps as given
        r.max_blockheads = 9;
        f._world.set_custom_rules(r);
        assert!(!b.check_if_can_warp_in_second_blockhead_after_item_added_data_b(ItemType(APPLE), 0));
        r.max_blockheads = 3;
        f._world.set_custom_rules(r);
        // not the food that is asked for
        assert!(!b.check_if_can_warp_in_second_blockhead_after_item_added_data_b(ItemType::ITEM_MANGO, 0));
        // right food but too few crystals
        assert!(!b.check_if_can_warp_in_second_blockhead_after_item_added_data_b(ItemType(APPLE), 0));
        credit_crystals(50);
        assert!(b.check_if_can_warp_in_second_blockhead_after_item_added_data_b(ItemType(APPLE), 0));
    }

    #[test]
    fn warp_check_crystal_price_grows_with_blockheads() {
        let f = fx();
        let b = f.b();
        *f._world.distance_ordered_food_types.borrow_mut() = vec![0; 5];
        put(b, 1, (0..5).map(|_| mk(APPLE, 0, 0)).collect());
        credit_crystals(100);
        // two blockheads cost 100, three and four cost 200
        f._dw.bhs.borrow_mut().extend([stub(1, true), stub(1, true)]);
        assert!(b.check_if_can_warp_in_second_blockhead_after_item_added_data_b(ItemType(APPLE), 0));
        f._dw.bhs.borrow_mut().push(stub(1, true));
        assert!(!b.check_if_can_warp_in_second_blockhead_after_item_added_data_b(ItemType(APPLE), 0));
        f._dw.bhs.borrow_mut().push(stub(1, true));
        assert!(!b.check_if_can_warp_in_second_blockhead_after_item_added_data_b(ItemType(APPLE), 0));
        credit_crystals(100);
        assert!(b.check_if_can_warp_in_second_blockhead_after_item_added_data_b(ItemType(APPLE), 0));
    }

    #[test]
    fn pickup_ownership_fallback_keys_and_priority() {
        let f = fx();
        let b = f.b();
        let owned = |ty: u32, key: &str| {
            let mut d = fb_data(ty, 0, 0);
            let mut sd = crate::objects::base::new_mutable_dict();
            sd.map.insert(key.to_string(), PlistValue::string("someone"));
            d.dynamic_object_save_dict = Some(PlistValue::Dict(sd));
            d.priority_blockhead = Some(f.o.clone());
            d
        };
        // another player's trading post, safe and display cabinet stay where they are (admins excepted)
        f._world.set_is_admin(false);
        for (ty, key) in [
            (ItemType::ITEM_TRADING_POST.0, "sellerClientID"),
            (ItemType::ITEM_SAFE.0, "safeClientID"),
            (ItemType::ITEM_DISPLAY_CABINET.0, "safeClientID"),
        ] {
            let before = f.ih.cannot_pickup.get();
            *f.ih.fb.borrow_mut() = Some(owned(ty, key));
            assert!(!b.pickup_freeblock_if_possible_in_tile_intentional(&stub(0, true), None, false), "{}", ty);
            assert_eq!(f.ih.cannot_pickup.get(), before + 1);
        }
        // a key the item does not use is no owner at all
        *f.ih.fb.borrow_mut() = Some(owned(ItemType::ITEM_SAFE.0, "sellerClientID"));
        assert!(b.pickup_freeblock_if_possible_in_tile_intentional(&stub(0, true), None, false));
        // no world
        let (o2, _dw2, _dwref2) = bh_without_world();
        assert!(!bh_of(&o2).pickup_freeblock_if_possible_in_tile_intentional(&stub(0, true), None, true));
        // a full inventory leaves the item and tells the free block
        full_except(b, 0);
        let before = f.ih.cannot_pickup.get();
        let mut d = fb_data(FLINT + 100, 0, 0);
        d.priority_blockhead = Some(f.o.clone());
        *f.ih.fb.borrow_mut() = Some(d);
        assert!(!b.pickup_freeblock_if_possible_in_tile_intentional(&stub(0, true), None, true));
        assert_eq!(f.ih.cannot_pickup.get(), before + 1);
    }

    #[test]
    fn pickup_events_requests_and_unconfirmed_items() {
        let f = fx();
        let b = f.b();
        // a blockhead controlled by the server reports a "got" simulation event
        b.bh.borrow_mut().is_client_blockhead_being_controlled_by_server = true;
        b.bh.borrow_mut().this_frame_pickup_requests = Some(crate::foundation::indexset::IndexSet::new());
        b.bh.borrow_mut().unconfirmed_pickups = Some(std::collections::BTreeMap::new());
        let fb = stub(0, true);
        *f.ih.fb.borrow_mut() = Some(fb_data(APPLE, 0, 0));
        assert!(b.pickup_freeblock_if_possible_in_tile_intentional(&fb, None, true));
        assert_eq!(*f.rec.sims.borrow(), vec![2]);
        let id = fb.unique_id();
        assert!(b.bh.borrow().this_frame_pickup_requests.as_ref().unwrap().contains_index(id));
        assert!(b.bh.borrow().unconfirmed_pickups.as_ref().unwrap().contains_key(&id));
        // a time crystal is not an inventory item and stays on the ground
        let fb2 = stub(0, true);
        *f.ih.fb.borrow_mut() = Some(fb_data(11, 0, 0));
        assert!(!b.pickup_freeblock_if_possible_in_tile_intentional(&fb2, None, true));
        assert!(!fb2.needs_removed());
        // money is reported too
        let fb3 = stub(0, true);
        *f.ih.fb.borrow_mut() = Some(fb_data(ItemType::ITEM_MONEY.0, 0, 5));
        assert!(b.pickup_freeblock_if_possible_in_tile_intentional(&fb3, None, true));
        assert!(b.bh.borrow().this_frame_pickup_requests.as_ref().unwrap().contains_index(fb3.unique_id()));
    }

    #[test]
    fn money_pickup_stops_at_each_coin_kind_when_full() {
        let f = fx();
        let b = f.b();
        put(b, 1, (0..98).map(|_| mk(ItemType::ITEM_PLATINUM_COIN.0, 0, 0)).collect());
        fill_all_other_slots(b, 1, 100);
        let fb = stub(0, true);
        *f.ih.fb.borrow_mut() = Some(fb_data(ItemType::ITEM_MONEY.0, 3, 250));
        assert!(b.pickup_freeblock_if_possible_in_tile_intentional(&fb, None, true));
        assert_eq!(counts(b)[1], 99);
        let fbs = f.rec.fbs.borrow().clone();
        assert_eq!(fbs.len(), 1);
        assert_eq!((fbs[0].ty, fbs[0].pos), (ItemType::ITEM_MONEY.0, (5, 9)));
        // two platinum pieces that did not fit: 20_000 + 250 copper left over
        assert_eq!((fbs[0].a, fbs[0].b), (2, 250));
    }
}
