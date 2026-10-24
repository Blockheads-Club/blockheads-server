//! Class `Action`: one queued Blockhead task (go to `goalTilePos`, then perform `goalInteraction`).
//!
//!  its `## Synthesis resolutions`
//! (item 3: save-dict number types.
//!
//! The Blockhead keeps its queue as `Vec<Rc<dyn Any>>` (`blockhead::blockhead::ActionRef`, the placeholder alias of
//! blockhead_core); the queue entries created by this module are `Rc<Action>` coerced to `Rc<dyn Any>`, and
//! [`action_of`] recovers them.

use std::any::Any;
use std::cell::RefCell;
use std::rc::Rc;

use super::blockhead::{Blockhead, BlockheadHostStub};
use super::inventory_model::InventoryHostStub;
use crate::crafting::craftable_item_object::{craftable_item_object_from_save_dict, CraftableItemObjectRef};
use crate::foundation::exception::range_exception_index;
use crate::foundation::gsdict::GsDict;
use crate::items::inventory_item::{ItemRef, SubSlot};
use crate::objects::base::{new_mutable_dict, number_with_unsigned_long};
use crate::plist::value::{PlistDict, PlistValue};
use crate::types::enums::{InteractionType, PathType};
use crate::types::structs::{IntPair, InteractionTestResult, WireStruct};

/// `NSDictionary<NSString*, NSNumber*>`: the predicted inventory change (keys `"%d"` of the item type, values ints).
pub type InventoryChange = GsDict<String, PlistValue>;

/// `Action*`.
pub type ActionRef = Rc<Action>;

/// Recovers an `Action` from a queue entry of `Blockhead.action_queue` (`Rc<dyn Any>`).
pub fn action_of(a: &Rc<dyn Any>) -> Option<ActionRef> {
    a.clone().downcast::<Action>().ok()
}

/// Ivars of `Action`. `blockCube` (client render object, always nil) is not ported.
#[derive(Clone)]
pub struct ActionState {
    /// 0x08
    pub in_progress: bool,
    /// 0x09 (never saved, never restored)
    pub complete: bool,
    /// 0x0a
    pub is_ai: bool,
    /// 0x0c
    pub goal_tile_pos: IntPair,
    /// 0x18 (retained; identity matters)
    pub interaction_item: Option<ItemRef>,
    /// 0x20 inventory slot index, -1 = none
    pub interaction_item_index: i16,
    /// 0x22 sub-slot index, -1 = none
    pub interaction_item_sub_index: i16,
    /// 0x24
    pub goal_interaction: InteractionType,
    /// 0x28
    pub path_type: PathType,
    /// 0x30
    pub interaction_object_id: u64,
    /// 0x38
    pub craftable_item_object: Option<CraftableItemObjectRef>,
    /// 0x40
    pub craft_count_or_extra_data: i16,
    /// 0x50
    pub interaction_test_result: InteractionTestResult,
    /// 0x60
    pub inventory_change: Option<InventoryChange>,
    /// 0x68
    pub animation_timer: f32,
}

/// `Action: NSObject`.
pub struct Action {
    st: RefCell<ActionState>,
}

fn key(k: &str) -> String {
    k.to_string()
}

fn dict_get<'a>(d: &'a PlistDict, k: &str) -> Option<&'a PlistValue> {
    d.map.get(&key(k))
}

/// `[nil intValue]` is 0.
fn int_of(v: Option<&PlistValue>) -> i32 {
    v.map_or(0, |v| v.int_value())
}

fn zero_result() -> InteractionTestResult {
    InteractionTestResult { good_or_bad: 0, render_item_type: 0, render_item_data_a: 0, render_item_data_b: 0 }
}

impl Action {
    /// `-initWithGoalPos:goalInteraction:pathType:interactionItem:itemIndex:itemSubIndex:interactionObjectID:craftableItemObject:craftCountOrExtraData:isAI:inventoryChange:`
    /// `inProgress`, `complete`, `interactionTestResult` and `animationTimer` stay zero (trap 16: the timer is
    /// here and 1.0 after `initWithSaveDict`).
    #[allow(clippy::too_many_arguments)]
    pub fn init_with_goal_pos_goal_interaction_path_type_interaction_item_item_index_item_sub_index_interaction_object_id_craftable_item_object_craft_count_or_extra_data_is_ai_inventory_change(
        goal_tile_pos: IntPair,
        goal_interaction: InteractionType,
        path_type: PathType,
        interaction_item: Option<ItemRef>,
        item_index: i16,
        item_sub_index: i16,
        interaction_object_id: u64,
        craftable_item_object: Option<CraftableItemObjectRef>,
        craft_count_or_extra_data: i16,
        is_ai: bool,
        inventory_change: Option<InventoryChange>,
    ) -> ActionRef {
        Rc::new(Action {
            st: RefCell::new(ActionState {
                in_progress: false,
                complete: false,
                is_ai,
                goal_tile_pos,
                interaction_item,
                interaction_item_index: item_index,
                interaction_item_sub_index: item_sub_index,
                goal_interaction,
                path_type,
                interaction_object_id,
                craftable_item_object,
                craft_count_or_extra_data,
                interaction_test_result: zero_result(),
                inventory_change,
                animation_timer: 0.0,
            }),
        })
    }

    /// `-initWithSaveDict:inventoryItems:`. May raise `NSRangeException` (`objectAtIndex:` out of range);
    /// index 0 is never resolved (strictly `> 0`), `complete` is never restored (trap 17), `animationTimer = 1.0`.
    pub fn init_with_save_dict_inventory_items(save: &PlistDict, inventory_items: &[SubSlot]) -> ActionRef {
        let in_progress = dict_get(save, "inProgress").is_some_and(|v| v.bool_value());
        let is_ai = dict_get(save, "isAI").is_some_and(|v| v.bool_value());
        let goal_tile_pos =
            IntPair { x: int_of(dict_get(save, "goalTilePos.x")), y: int_of(dict_get(save, "goalTilePos.y")) };
        let mut item_index = int_of(dict_get(save, "interactionItemIndex")) as i16;
        let mut item_sub_index = int_of(dict_get(save, "interactionItemSubIndex")) as i16;
        let item_type = int_of(dict_get(save, "interactionItemType")) as u32;
        let mut interaction_item: Option<ItemRef> = None;
        if item_index > 0 {
            let idx = item_index as usize;
            // [inventoryItems objectAtIndex:]
            let slot = match inventory_items.get(idx) {
                Some(s) => s.clone(),
                None => range_exception_index(idx as u64, inventory_items.len() as u64, "objectAtIndex:"),
            };
            let slot = slot.borrow();
            if !slot.is_empty() {
                let it = slot[slot.len() - 1].clone();
                interaction_item = Some(it.clone());
                let subs = it.borrow().sub_items.clone();
                if let Some(subs) = subs {
                    if !subs.is_empty() && item_sub_index != -1 {
                        // [subs objectAtIndex:(NSUInteger)(long)subIndex]
                        let sidx = item_sub_index as i64 as u64;
                        let sub_slot = if sidx < subs.len() as u64 {
                            subs[sidx as usize].clone()
                        } else {
                            range_exception_index(sidx, subs.len() as u64, "objectAtIndex:")
                        };
                        let sub_slot = sub_slot.borrow();
                        if !sub_slot.is_empty() {
                            let inner = sub_slot[sub_slot.len() - 1].clone();
                            interaction_item = Some(inner.clone());
                            if item_type != inner.borrow().item_type.0 {
                                interaction_item = None;
                                item_index = -1;
                                item_sub_index = -1;
                            }
                        }
                    }
                }
            }
        }
        let goal_interaction = InteractionType(int_of(dict_get(save, "goalInteraction")) as u32);
        let path_type = PathType(int_of(dict_get(save, "pathType")) as u32);
        let interaction_object_id = dict_get(save, "interactionObjectID").map_or(0, |v| v.unsigned_long_value());
        let mut craftable_item_object = None;
        if let Some(PlistValue::Dict(cd)) = dict_get(save, "craftableItemObject") {
            // type 1 / 2 / other selection is inside the factory
            craftable_item_object = Some(craftable_item_object_from_save_dict(cd));
        }
        // (int16)[... unsignedLongValue]: truncation of u64 to s16
        let craft_count = dict_get(save, "craftCountOrExtraData").map_or(0, |v| v.unsigned_long_value()) as i16;
        let inventory_change = match dict_get(save, "inventoryChange") {
            Some(PlistValue::Dict(d)) => Some(d.map.clone()),
            _ => None,
        };
        let mut result = zero_result();
        if let Some(PlistValue::Data(d)) = dict_get(save, "interactionTestResult") {
            // [NSData getBytes:length:12]: GNUstep raises for short data; copy what exists otherwise
            let mut buf = [0u8; 12];
            let n = d.bytes.len().min(12);
            buf[..n].copy_from_slice(&d.bytes[..n]);
            result = InteractionTestResult::read_le(&buf);
        }
        Rc::new(Action {
            st: RefCell::new(ActionState {
                in_progress,
                complete: false,
                is_ai,
                goal_tile_pos,
                interaction_item,
                interaction_item_index: item_index,
                interaction_item_sub_index: item_sub_index,
                goal_interaction,
                path_type,
                interaction_object_id,
                craftable_item_object,
                craft_count_or_extra_data: craft_count,
                interaction_test_result: result,
                inventory_change,
                animation_timer: 1.0,
            }),
        })
    }

    /// `-getSaveDict`. Number types: `inProgress`/`isAI` bool, `interactionObjectID` unsigned long,
    /// `craftCountOrExtraData` `numberWithUnsignedInt((u32)(i32)(i16))` (so -1 -> 4294967295), the rest int;
    /// `inventoryChange` and `craftableItemObject` are omitted when nil.
    pub fn get_save_dict(&self) -> PlistDict {
        let st = self.st.borrow();
        let mut d = new_mutable_dict();
        let ins = |d: &mut PlistDict, k: &str, v: PlistValue| {
            d.map.insert(key(k), v);
        };
        ins(&mut d, "inProgress", PlistValue::Bool(st.in_progress));
        ins(&mut d, "isAI", PlistValue::Bool(st.is_ai));
        ins(&mut d, "goalTilePos.x", PlistValue::Int(st.goal_tile_pos.x as i64));
        ins(&mut d, "goalTilePos.y", PlistValue::Int(st.goal_tile_pos.y as i64));
        ins(&mut d, "interactionItemIndex", PlistValue::Int(st.interaction_item_index as i64));
        ins(&mut d, "interactionItemSubIndex", PlistValue::Int(st.interaction_item_sub_index as i64));
        let it = st.interaction_item.as_ref().map_or(0, |i| i.borrow().item_type.0);
        ins(&mut d, "interactionItemType", PlistValue::Int(it as i32 as i64));
        ins(&mut d, "goalInteraction", PlistValue::Int(st.goal_interaction.0 as i32 as i64));
        ins(&mut d, "pathType", PlistValue::Int(st.path_type.0 as i32 as i64));
        ins(&mut d, "interactionObjectID", number_with_unsigned_long(st.interaction_object_id));
        if let Some(ic) = &st.inventory_change {
            ins(&mut d, "inventoryChange", PlistValue::dict(ic.clone()));
        }
        if let Some(c) = &st.craftable_item_object {
            ins(&mut d, "craftableItemObject", PlistValue::Dict(c.get_save_dict()));
        }
        ins(&mut d, "craftCountOrExtraData", PlistValue::Int(st.craft_count_or_extra_data as i32 as u32 as i64));
        let mut buf = [0u8; 12];
        st.interaction_test_result.write_le(&mut buf);
        ins(&mut d, "interactionTestResult", PlistValue::data(buf.to_vec()));
        d
    }

    /// `-dealloc`: the autorelease of `interactionItem` has no observable effect; nothing to do.
    pub fn dealloc(&self) {}

    // ---- accessors --------------------------------------------------------------------

    /// Copy of all ivars.
    pub fn state(&self) -> ActionState {
        self.st.borrow().clone()
    }
    /// `-inProgress`
    pub fn in_progress(&self) -> bool {
        self.st.borrow().in_progress
    }
    /// `-setInProgress:`
    pub fn set_in_progress(&self, v: bool) {
        self.st.borrow_mut().in_progress = v;
    }
    /// `-complete`
    pub fn complete(&self) -> bool {
        self.st.borrow().complete
    }
    /// `-setComplete:`
    pub fn set_complete(&self, v: bool) {
        self.st.borrow_mut().complete = v;
    }
    /// `-goalTilePos`
    pub fn goal_tile_pos(&self) -> IntPair {
        self.st.borrow().goal_tile_pos
    }
    /// `-setGoalTilePos:`
    pub fn set_goal_tile_pos(&self, p: IntPair) {
        self.st.borrow_mut().goal_tile_pos = p;
    }
    /// `-inventoryChange`
    pub fn inventory_change(&self) -> Option<InventoryChange> {
        self.st.borrow().inventory_change.clone()
    }
    /// `-setInventoryChange:`
    pub fn set_inventory_change(&self, c: Option<InventoryChange>) {
        self.st.borrow_mut().inventory_change = c;
    }
    /// `-interactionItem`
    pub fn interaction_item(&self) -> Option<ItemRef> {
        self.st.borrow().interaction_item.clone()
    }
    /// `-setInteractionItem:`
    pub fn set_interaction_item(&self, i: Option<ItemRef>) {
        self.st.borrow_mut().interaction_item = i;
    }
    /// `-interactionItemIndex`
    pub fn interaction_item_index(&self) -> i16 {
        self.st.borrow().interaction_item_index
    }
    /// `-setInteractionItemIndex:`
    pub fn set_interaction_item_index(&self, v: i16) {
        self.st.borrow_mut().interaction_item_index = v;
    }
    /// `-interactionItemSubIndex`
    pub fn interaction_item_sub_index(&self) -> i16 {
        self.st.borrow().interaction_item_sub_index
    }
    /// `-setInteractionItemSubIndex:`
    pub fn set_interaction_item_sub_index(&self, v: i16) {
        self.st.borrow_mut().interaction_item_sub_index = v;
    }
    /// `-interactionObjectID`
    pub fn interaction_object_id(&self) -> u64 {
        self.st.borrow().interaction_object_id
    }
    /// `-setInteractionObjectID:`
    pub fn set_interaction_object_id(&self, v: u64) {
        self.st.borrow_mut().interaction_object_id = v;
    }
    /// `-goalInteraction`
    pub fn goal_interaction(&self) -> InteractionType {
        self.st.borrow().goal_interaction
    }
    /// `-setGoalInteraction:`
    pub fn set_goal_interaction(&self, v: InteractionType) {
        self.st.borrow_mut().goal_interaction = v;
    }
    /// `-pathType`
    pub fn path_type(&self) -> PathType {
        self.st.borrow().path_type
    }
    /// `-setPathType:`
    pub fn set_path_type(&self, v: PathType) {
        self.st.borrow_mut().path_type = v;
    }
    /// `-isAI`
    pub fn is_ai(&self) -> bool {
        self.st.borrow().is_ai
    }
    /// `-setIsAI:`
    pub fn set_is_ai(&self, v: bool) {
        self.st.borrow_mut().is_ai = v;
    }
    /// `-animationTimer`
    pub fn animation_timer(&self) -> f32 {
        self.st.borrow().animation_timer
    }
    /// `-setAnimationTimer:`
    pub fn set_animation_timer(&self, v: f32) {
        self.st.borrow_mut().animation_timer = v;
    }
    /// `-craftableItemObject`
    pub fn craftable_item_object(&self) -> Option<CraftableItemObjectRef> {
        self.st.borrow().craftable_item_object.clone()
    }
    /// `-setCraftableItemObject:`
    pub fn set_craftable_item_object(&self, c: Option<CraftableItemObjectRef>) {
        self.st.borrow_mut().craftable_item_object = c;
    }
    /// `-craftCountOrExtraData`
    pub fn craft_count_or_extra_data(&self) -> i16 {
        self.st.borrow().craft_count_or_extra_data
    }
    /// `-setCraftCountOrExtraData:`
    pub fn set_craft_count_or_extra_data(&self, v: i16) {
        self.st.borrow_mut().craft_count_or_extra_data = v;
    }
    /// `-interactionTestResult`
    pub fn interaction_test_result(&self) -> InteractionTestResult {
        self.st.borrow().interaction_test_result
    }
    /// `-setInteractionTestResult:`
    pub fn set_interaction_test_result(&self, r: InteractionTestResult) {
        self.st.borrow_mut().interaction_test_result = r;
    }
}

// ====================================================================================================
// Host bridge
// ====================================================================================================

/// Ready-made implementation of the `BlockheadHost` methods owned by this module (every `impl Blockhead` method of
/// blockhead_actions that `blockhead_core` calls through its seam) and of the `Action` accessors that `blockhead_core` ([`BlockheadHost`]) and
/// `blockhead_inventory` ([`InventoryHost`]) route through their seams (their `ActionRef` is `Rc<dyn Any>`). The
/// integrator's adapters forward the `action_*` methods to the same-named methods here, e.g.
/// `fn action_goal_tile_pos(&self, a: &ActionRef) -> IntPair { ActionBridge.action_goal_tile_pos(a) }`.
/// Entries that are not an [`Action`] answer like the inert defaults.
pub struct ActionBridge;

impl BlockheadHostStub for ActionBridge {
    // ---- the Blockhead methods owned by this module (blockhead_actions) ----
    fn update_position(&self, b: &Blockhead, p: IntPair) {
        b.update_position(p)
    }
    fn stop_riding(&self, b: &Blockhead) {
        b.stop_riding()
    }
    fn pickup_dynamic_object(&self, b: &Blockhead, o: &crate::objects::handle::ObjRef) {
        b.pickup_dynamic_object(o)
    }
    fn interaction_type_for_tile_at_pos_item_pickup_rejected_due_to_inventory_full_include_actions_face_index_allow_protected_actions(
        &self,
        b: &Blockhead,
        tile: Option<crate::world::state::TileRef>,
        pos: IntPair,
        item: Option<&ItemRef>,
        rejected: &mut bool,
        include_actions: bool,
        face_index: i32,
        allow_protected_actions: bool,
    ) -> u32 {
        b.interaction_type_for_tile_at_pos_item_pickup_rejected_due_to_inventory_full_include_actions_face_index_allow_protected_actions(
            tile.as_ref(), pos, item, rejected, include_actions, face_index, allow_protected_actions,
        )
        .0
    }
    fn good_or_bad_interaction_for_action(&self, b: &Blockhead, action: Option<&Rc<dyn Any>>) -> InteractionTestResult {
        match action.and_then(action_of) {
            Some(a) => b.good_or_bad_interaction_for_action(&a),
            None => zero_result(),
        }
    }
    fn update_gather_speed_and_animation_for_current_interation_and_item(&self, b: &Blockhead) {
        b.update_gather_speed_and_animation_for_current_interation_and_item()
    }
    fn is_adding_fuel_to_any_workbench(&self, b: &Blockhead) -> bool {
        b.is_adding_fuel_to_any_workbench()
    }
    fn current_traverse_to_key_frame(&self, b: &Blockhead) -> i32 {
        b.current_traverse_to_key_frame()
    }
    fn next_pos(&self, b: &Blockhead) -> IntPair {
        b.next_pos()
    }
    fn can_crawl(&self, b: &Blockhead) -> bool {
        b.can_crawl()
    }
    fn can_fly(&self, b: &Blockhead) -> bool {
        b.can_fly()
    }
    fn tap_is_within_body_radius(&self, b: &Blockhead, tap: crate::types::structs::Vector2) -> bool {
        b.tap_is_within_body_radius(tap)
    }
    fn hit_with_force_blockhead(&self, b: &Blockhead, force: i32, hitter: Option<&crate::objects::handle::ObjRef>) {
        b.hit_with_force_blockhead(force, hitter)
    }
    fn will_die_if_hit_by_force(&self, b: &Blockhead, force: i32) -> bool {
        b.will_die_if_hit_by_force(force)
    }
    fn center(&self, b: &Blockhead) -> crate::types::structs::Vector2 {
        b.center()
    }
    fn is_interacting_with_any_interaction_object(&self, b: &Blockhead) -> bool {
        b.is_interacting_with_any_interaction_object()
    }
    fn set_interaction_object(&self, b: &Blockhead, o: Option<&crate::objects::handle::ObjRef>) {
        b.set_interaction_object(o.cloned())
    }
    fn set_interaction_workbench(&self, b: &Blockhead, o: Option<&crate::objects::handle::ObjRef>) {
        b.set_interaction_workbench(o.cloned())
    }
    fn stop_all_actions(&self, b: &Blockhead) {
        b.stop_all_actions()
    }
    fn stop_fishing(&self, b: &Blockhead) {
        b.stop_fishing()
    }
    fn die_for_good(&self, b: &Blockhead) {
        b.die_for_good()
    }
    fn unable_to_work_reason(&self, b: &Blockhead) -> i32 {
        b.unable_to_work_reason().0 as i32
    }
    fn jetpack_fuel_count(&self, b: &Blockhead) -> i32 {
        b.jetpack_fuel_count()
    }
    fn update_jet_motion(&self, b: &Blockhead, dt: f32) {
        b.update_jet_motion(dt)
    }
    fn free_block_pickup_radius(&self, b: &Blockhead) -> f32 {
        b.free_block_pickup_radius()
    }
    fn dpad_should_be_displayed(&self, b: &Blockhead) -> bool {
        b.dpad_should_be_displayed()
    }
    fn current_interaction_requires_human_input(&self, b: &Blockhead) -> bool {
        b.current_interaction_requires_human_input()
    }
    // ---- the Action accessors ----
    fn action_in_progress(&self, a: &Rc<dyn Any>) -> bool {
        action_of(a).is_some_and(|a| a.in_progress())
    }
    fn action_set_in_progress(&self, a: &Rc<dyn Any>, v: bool) {
        if let Some(a) = action_of(a) {
            a.set_in_progress(v);
        }
    }
    fn action_goal_tile_pos(&self, a: &Rc<dyn Any>) -> IntPair {
        action_of(a).map_or(IntPair { x: 0, y: 0 }, |a| a.goal_tile_pos())
    }
    fn action_get_save_dict(&self, a: &Rc<dyn Any>) -> PlistValue {
        action_of(a).map_or(PlistValue::mutable_dict(), |a| PlistValue::Dict(a.get_save_dict()))
    }
    fn action_init_with_save_dict_inventory_items(
        &self,
        dict: &PlistValue,
        inventory_items: &[SubSlot],
    ) -> Rc<dyn Any> {
        match dict {
            PlistValue::Dict(d) => Action::init_with_save_dict_inventory_items(d, inventory_items),
            _ => Action::init_with_save_dict_inventory_items(&new_mutable_dict(), inventory_items),
        }
    }
    fn action_interaction_test_result(&self, a: &Rc<dyn Any>) -> InteractionTestResult {
        action_of(a).map_or(zero_result(), |a| a.interaction_test_result())
    }
    fn action_interaction_object_id(&self, a: &Rc<dyn Any>) -> i64 {
        action_of(a).map_or(0, |a| a.interaction_object_id() as i64)
    }
    fn action_inventory_change_int_for_key(&self, a: &Rc<dyn Any>, key: &str) -> Option<i32> {
        action_of(a)?.inventory_change()?.get(&key.to_string()).map(|v| v.int_value())
    }
    fn action_set_inventory_change_int_for_key(&self, a: &Rc<dyn Any>, key: &str, value: i32) {
        if let Some(a) = action_of(a) {
            let mut ic = a.inventory_change().unwrap_or_else(InventoryChange::new);
            ic.insert(key.to_string(), PlistValue::Int(value as i64));
            a.set_inventory_change(Some(ic));
        }
    }
}

impl InventoryHostStub for ActionBridge {
    fn action_goal_interaction(&self, a: &Rc<dyn Any>) -> i32 {
        action_of(a).map_or(0, |a| a.goal_interaction().0 as i32)
    }
    fn action_interaction_item(&self, a: &Rc<dyn Any>) -> Option<ItemRef> {
        action_of(a)?.interaction_item()
    }
    fn action_interaction_item_index(&self, a: &Rc<dyn Any>) -> i32 {
        action_of(a).map_or(0, |a| a.interaction_item_index() as i32)
    }
    fn action_interaction_item_sub_index(&self, a: &Rc<dyn Any>) -> i32 {
        action_of(a).map_or(0, |a| a.interaction_item_sub_index() as i32)
    }
    fn action_set_interaction_item(&self, a: &Rc<dyn Any>, item: Option<&ItemRef>) {
        if let Some(a) = action_of(a) {
            a.set_interaction_item(item.cloned());
        }
    }
    fn action_set_interaction_item_index(&self, a: &Rc<dyn Any>, v: i32) {
        if let Some(a) = action_of(a) {
            a.set_interaction_item_index(v as i16);
        }
    }
    fn action_set_interaction_item_sub_index(&self, a: &Rc<dyn Any>, v: i32) {
        if let Some(a) = action_of(a) {
            a.set_interaction_item_sub_index(v as i16);
        }
    }
    fn action_craft_count_or_extra_data(&self, a: &Rc<dyn Any>) -> i32 {
        action_of(a).map_or(0, |a| a.craft_count_or_extra_data() as i32)
    }
    fn action_path_type(&self, a: &Rc<dyn Any>) -> i32 {
        action_of(a).map_or(0, |a| a.path_type().0 as i32)
    }
    fn action_craftable_item_object(&self, a: &Rc<dyn Any>) -> Option<Rc<dyn Any>> {
        let c = action_of(a)?.craftable_item_object()?;
        Some(c.into_any_rc())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::items::inventory_item::InventoryItem;
    use crate::types::item_type::ItemType;

    fn sample(count: i16) -> ActionRef {
        let mut ic = InventoryChange::new();
        ic.insert("105".to_string(), PlistValue::Int(1));
        ic.insert("102".to_string(), PlistValue::Int(-1));
        let a = Action::init_with_goal_pos_goal_interaction_path_type_interaction_item_item_index_item_sub_index_interaction_object_id_craftable_item_object_craft_count_or_extra_data_is_ai_inventory_change(
            IntPair { x: 12, y: -3 },
            InteractionType::INTERACTION_FILL_BUCKET,
            PathType(2),
            None,
            -1,
            -1,
            0xdead_beef_0000_0001,
            None,
            count,
            true,
            Some(ic),
        );
        a.set_in_progress(true);
        a.set_interaction_test_result(InteractionTestResult {
            good_or_bad: -1,
            render_item_type: 0x66,
            render_item_data_a: 0,
            render_item_data_b: 7,
        });
        a
    }

    /// : key set, unsigned craft count, 12-byte result, timer 1.0 after load.
    #[test]
    fn save_round_trip() {
        let a = sample(-1);
        assert_eq!(a.animation_timer(), 0.0);
        let d = a.get_save_dict();
        let g = |k: &str| d.map.get(&k.to_string()).cloned().unwrap();
        assert!(matches!(g("inProgress"), PlistValue::Bool(true)));
        assert!(matches!(g("isAI"), PlistValue::Bool(true)));
        assert_eq!(g("craftCountOrExtraData").long_long_value(), 4294967295);
        assert_eq!(g("goalTilePos.y").int_value(), -3);
        assert_eq!(g("interactionItemType").int_value(), 0);
        assert_eq!(g("interactionObjectID").unsigned_long_value(), 0xdead_beef_0000_0001);
        assert!(d.map.get(&"craftableItemObject".to_string()).is_none());
        let PlistValue::Data(b) = g("interactionTestResult") else { unreachable!("data expected") };
        assert_eq!(b.bytes.len(), 12);
        assert_eq!(&b.bytes[..4], &(-1i32).to_le_bytes());
        assert_eq!(&b.bytes[4..8], &0x66u32.to_le_bytes());
        assert_eq!(&b.bytes[8..10], &[0, 0]);
        assert_eq!(&b.bytes[10..12], &7u16.to_le_bytes());
        let b = Action::init_with_save_dict_inventory_items(&d, &[]);
        assert_eq!(b.craft_count_or_extra_data(), -1);
        assert_eq!(b.animation_timer(), 1.0);
        assert!(b.in_progress() && b.is_ai() && !b.complete());
        assert_eq!(b.goal_tile_pos(), IntPair { x: 12, y: -3 });
        assert_eq!(b.goal_interaction(), InteractionType::INTERACTION_FILL_BUCKET);
        assert_eq!(b.interaction_object_id(), 0xdead_beef_0000_0001);
        assert_eq!(b.interaction_test_result(), a.interaction_test_result());
        let ic = b.inventory_change().unwrap();
        assert_eq!(ic.keys_in_order(), a.inventory_change().unwrap().keys_in_order());
        assert_eq!(ic.get(&"105".to_string()).unwrap().int_value(), 1);
        assert_eq!(ic.get(&"102".to_string()).unwrap().int_value(), -1);
        // second save is identical
        let d2 = b.get_save_dict();
        assert_eq!(d2.map.len(), d.map.len());
    }

    /// Item resolution: index 0 never resolved, last element used, type mismatch resets both indices.
    #[test]
    fn item_resolution() {
        let it = InventoryItem::new(ItemType(8), 0, 0, None, None);
        let slot1: SubSlot = Rc::new(RefCell::new(vec![it.clone()]));
        let slot0: SubSlot = Rc::new(RefCell::new(vec![it.clone()]));
        let items = vec![slot0, slot1];
        let mk = |idx: i32, ty: i32| {
            let mut d = new_mutable_dict();
            d.map.insert("interactionItemIndex".into(), PlistValue::Int(idx as i64));
            d.map.insert("interactionItemSubIndex".into(), PlistValue::Int(-1));
            d.map.insert("interactionItemType".into(), PlistValue::Int(ty as i64));
            d
        };
        let a = Action::init_with_save_dict_inventory_items(&mk(0, 8), &items);
        assert!(a.interaction_item().is_none());
        assert_eq!(a.interaction_item_index(), 0);
        let a = Action::init_with_save_dict_inventory_items(&mk(1, 8), &items);
        assert!(Rc::ptr_eq(&a.interaction_item().unwrap(), &it));
        // no sub items on the item: the type check is skipped
        let a = Action::init_with_save_dict_inventory_items(&mk(1, 99), &items);
        assert!(a.interaction_item().is_some());
        // out of range raises NSRangeException
        let r =
            crate::foundation::exception::catch_ns(|| Action::init_with_save_dict_inventory_items(&mk(5, 8), &items));
        assert!(r.is_err());
    }

    #[test]
    fn action_of_downcasts() {
        let a = sample(1);
        let q: Rc<dyn Any> = a.clone();
        assert!(Rc::ptr_eq(&action_of(&q).unwrap(), &a));
        let n: Rc<dyn Any> = Rc::new(());
        assert!(action_of(&n).is_none());
    }

    /// The bridge forwards the Blockhead methods of this module and the Action accessors.
    #[test]
    fn bridge_forwards() {
        use crate::blockhead::blockhead::testutil::*;
        let (world, dw, dwref) = setup();
        let o = new_bh(&world, &dwref, 0);
        std::mem::forget((dw, dwref));
        let b = bh_of(&o);
        let br = ActionBridge;
        let a = sample(3);
        let any: Rc<dyn Any> = a.clone();
        assert!(BlockheadHostStub::action_in_progress(&br, &any));
        assert_eq!(BlockheadHostStub::action_goal_tile_pos(&br, &any), IntPair { x: 12, y: -3 });
        assert_eq!(BlockheadHostStub::action_interaction_object_id(&br, &any), 0xdead_beef_0000_0001u64 as i64);
        assert_eq!(InventoryHostStub::action_goal_interaction(&br, &any), 14);
        assert_eq!(InventoryHostStub::action_craft_count_or_extra_data(&br, &any), 3);
        assert_eq!(BlockheadHostStub::action_inventory_change_int_for_key(&br, &any, "105"), Some(1));
        BlockheadHostStub::action_set_inventory_change_int_for_key(&br, &any, "7", 9);
        assert_eq!(a.inventory_change().unwrap().get(&"7".to_string()).unwrap().int_value(), 9);
        let r = BlockheadHostStub::good_or_bad_interaction_for_action(&br, b, Some(&any));
        assert_eq!(r.render_item_type, 0);
        assert_eq!(BlockheadHostStub::unable_to_work_reason(&br, b), 0);
        assert!(BlockheadHostStub::can_crawl(&br, b));
        assert_eq!(BlockheadHostStub::jetpack_fuel_count(&br, b), 0);
        assert_eq!(BlockheadHostStub::free_block_pickup_radius(&br, b), 0.5);
        let saved = BlockheadHostStub::action_get_save_dict(&br, &any);
        let back = BlockheadHostStub::action_init_with_save_dict_inventory_items(&br, &saved, &[]);
        assert_eq!(action_of(&back).unwrap().goal_tile_pos(), IntPair { x: 12, y: -3 });
    }

    fn basket_with(items: &[(usize, ItemRef)]) -> ItemRef {
        let mut input: Vec<Vec<ItemRef>> = vec![Vec::new(); 4];
        for (i, it) in items {
            input[*i].push(it.clone());
        }
        InventoryItem::new(ItemType::ITEM_BASKET, 0, 0, Some(&input), None)
    }

    fn resolve_dict(idx: i64, sub: i64, ty: i64) -> PlistDict {
        let mut d = new_mutable_dict();
        d.map.insert("interactionItemIndex".into(), PlistValue::Int(idx));
        d.map.insert("interactionItemSubIndex".into(), PlistValue::Int(sub));
        d.map.insert("interactionItemType".into(), PlistValue::Int(ty));
        d
    }

    /// Items inside containers are found through the sub index, and only when their type still matches.
    #[test]
    fn sub_item_resolution() {
        let inner = InventoryItem::new(ItemType(8), 0, 0, None, None);
        let basket = basket_with(&[(1, inner.clone())]);
        let empty_basket = basket_with(&[]);
        let items: Vec<SubSlot> = vec![
            Rc::new(RefCell::new(vec![])),
            Rc::new(RefCell::new(vec![basket.clone()])),
            Rc::new(RefCell::new(vec![empty_basket])),
        ];
        // the inner item is found and keeps both indices
        let a = Action::init_with_save_dict_inventory_items(&resolve_dict(1, 1, 8), &items);
        assert!(Rc::ptr_eq(&a.interaction_item().unwrap(), &inner));
        assert_eq!((a.interaction_item_index(), a.interaction_item_sub_index()), (1, 1));
        // another type in the sub slot: nothing is interacted with and the indices reset
        let a = Action::init_with_save_dict_inventory_items(&resolve_dict(1, 1, 9), &items);
        assert!(a.interaction_item().is_none());
        assert_eq!((a.interaction_item_index(), a.interaction_item_sub_index()), (-1, -1));
        // an empty sub slot keeps the container as the interaction item
        let a = Action::init_with_save_dict_inventory_items(&resolve_dict(1, 0, 8), &items);
        assert!(Rc::ptr_eq(&a.interaction_item().unwrap(), &basket));
        // sub index -1 means the container itself; an empty outer slot gives nothing
        let a = Action::init_with_save_dict_inventory_items(&resolve_dict(1, -1, 8), &items);
        assert!(Rc::ptr_eq(&a.interaction_item().unwrap(), &basket));
        let a = Action::init_with_save_dict_inventory_items(&resolve_dict(2, -1, 8), &items);
        assert_eq!(a.interaction_item().unwrap().borrow().item_type, ItemType::ITEM_BASKET);
        let a = Action::init_with_save_dict_inventory_items(&resolve_dict(0, 3, 8), &items);
        assert!(a.interaction_item().is_none());
        // a sub index past the slots raises
        let r = crate::foundation::exception::catch_ns(|| {
            Action::init_with_save_dict_inventory_items(&resolve_dict(1, 9, 8), &items).goal_tile_pos()
        });
        assert_eq!(r.unwrap_err().name, "NSRangeException");
    }

    #[test]
    fn craftable_objects_and_every_setter() {
        use crate::crafting::craftable_item_object::{zero_craftable_item, CraftableItemObject};
        let a = sample(2);
        // a craftable object survives the save dictionary
        let mut ci = zero_craftable_item();
        ci.crafted_item_type = 12;
        a.set_craftable_item_object(Some(CraftableItemObject::init_with_craftable_item(ci)));
        let d = a.get_save_dict();
        assert!(d.map.get(&"craftableItemObject".to_string()).is_some());
        let back = Action::init_with_save_dict_inventory_items(&d, &[]);
        assert_eq!(back.craftable_item_object().unwrap().craftable_item().crafted_item_type, 12);
        // plain accessors
        a.set_complete(true);
        a.set_goal_tile_pos(IntPair { x: 1, y: 2 });
        a.set_interaction_object_id(7);
        a.set_goal_interaction(InteractionType::INTERACTION_FILL_BUCKET);
        a.set_path_type(PathType(3));
        a.set_is_ai(false);
        a.set_animation_timer(0.5);
        a.set_craft_count_or_extra_data(9);
        let s = a.state();
        assert!(s.complete && !s.is_ai);
        assert_eq!((s.goal_tile_pos, s.interaction_object_id, s.path_type), (IntPair { x: 1, y: 2 }, 7, PathType(3)));
        assert_eq!((s.animation_timer, s.craft_count_or_extra_data), (0.5, 9));
        a.dealloc();
    }

    /// The bridge answers inertly for entries that are not actions and forwards the rest to the blockhead.
    #[test]
    fn bridge_edge_cases_and_blockhead_forwards() {
        use crate::blockhead::blockhead::testutil::*;
        let _h = TestHost::install();
        let (world, dw, dwref) = setup();
        let o = new_bh(&world, &dwref, 0);
        let b = bh_of(&o);
        let br = ActionBridge;
        let not_action: Rc<dyn Any> = Rc::new(());
        assert!(!BlockheadHostStub::action_in_progress(&br, &not_action));
        BlockheadHostStub::action_set_in_progress(&br, &not_action, true);
        assert_eq!(BlockheadHostStub::action_goal_tile_pos(&br, &not_action), IntPair { x: 0, y: 0 });
        assert_eq!(BlockheadHostStub::action_interaction_test_result(&br, &not_action), zero_result());
        assert_eq!(BlockheadHostStub::action_interaction_object_id(&br, &not_action), 0);
        assert_eq!(BlockheadHostStub::action_inventory_change_int_for_key(&br, &not_action, "1"), None);
        BlockheadHostStub::action_set_inventory_change_int_for_key(&br, &not_action, "1", 2);
        assert_eq!(BlockheadHostStub::good_or_bad_interaction_for_action(&br, b, Some(&not_action)), zero_result());
        assert_eq!(BlockheadHostStub::good_or_bad_interaction_for_action(&br, b, None), zero_result());
        assert!(matches!(BlockheadHostStub::action_get_save_dict(&br, &not_action), PlistValue::Dict(_)));
        let from_junk = BlockheadHostStub::action_init_with_save_dict_inventory_items(&br, &PlistValue::int(1), &[]);
        assert_eq!(action_of(&from_junk).unwrap().goal_tile_pos(), IntPair { x: 0, y: 0 });
        assert_eq!(InventoryHostStub::action_goal_interaction(&br, &not_action), 0);
        assert!(InventoryHostStub::action_interaction_item(&br, &not_action).is_none());
        assert_eq!(InventoryHostStub::action_interaction_item_index(&br, &not_action), 0);
        assert_eq!(InventoryHostStub::action_interaction_item_sub_index(&br, &not_action), 0);
        InventoryHostStub::action_set_interaction_item(&br, &not_action, None);
        InventoryHostStub::action_set_interaction_item_index(&br, &not_action, 1);
        InventoryHostStub::action_set_interaction_item_sub_index(&br, &not_action, 1);
        assert_eq!(InventoryHostStub::action_craft_count_or_extra_data(&br, &not_action), 0);
        assert_eq!(InventoryHostStub::action_path_type(&br, &not_action), 0);
        assert!(InventoryHostStub::action_craftable_item_object(&br, &not_action).is_none());
        // the setters reach a real action
        let a = sample(1);
        let any: Rc<dyn Any> = a.clone();
        let item = InventoryItem::new(ItemType(8), 0, 0, None, None);
        InventoryHostStub::action_set_interaction_item(&br, &any, Some(&item));
        InventoryHostStub::action_set_interaction_item_index(&br, &any, 3);
        InventoryHostStub::action_set_interaction_item_sub_index(&br, &any, 2);
        assert_eq!((a.interaction_item_index(), a.interaction_item_sub_index()), (3, 2));
        assert_eq!(InventoryHostStub::action_interaction_item_index(&br, &any), 3);
        assert_eq!(InventoryHostStub::action_interaction_item_sub_index(&br, &any), 2);
        assert!(Rc::ptr_eq(&InventoryHostStub::action_interaction_item(&br, &any).unwrap(), &item));
        assert_eq!(InventoryHostStub::action_path_type(&br, &any), 2);
        assert!(InventoryHostStub::action_craftable_item_object(&br, &any).is_none());
        a.set_craftable_item_object(Some(
            crate::crafting::craftable_item_object::CraftableItemObject::init_with_craftable_item(
                crate::crafting::craftable_item_object::zero_craftable_item(),
            ),
        ));
        assert!(InventoryHostStub::action_craftable_item_object(&br, &any).is_some());
        // the blockhead methods of the module
        BlockheadHostStub::update_position(&br, b, IntPair { x: 6, y: 8 });
        assert_eq!(b.pos(), IntPair { x: 6, y: 8 });
        BlockheadHostStub::stop_riding(&br, b);
        let stub_obj = stub(5, true);
        BlockheadHostStub::pickup_dynamic_object(&br, b, &stub_obj);
        let mut rejected = false;
        let _ = BlockheadHostStub::interaction_type_for_tile_at_pos_item_pickup_rejected_due_to_inventory_full_include_actions_face_index_allow_protected_actions(
            &br,
            b,
            Some(tile(&world, 5, 3)),
            IntPair { x: 5, y: 3 },
            None,
            &mut rejected,
            false,
            0,
            false,
        );
        BlockheadHostStub::update_gather_speed_and_animation_for_current_interation_and_item(&br, b);
        assert!(!BlockheadHostStub::is_adding_fuel_to_any_workbench(&br, b));
        assert_eq!(BlockheadHostStub::current_traverse_to_key_frame(&br, b), b.current_traverse_to_key_frame());
        assert_eq!(BlockheadHostStub::next_pos(&br, b), b.next_pos());
        assert_eq!(BlockheadHostStub::can_fly(&br, b), b.can_fly());
        assert_eq!(BlockheadHostStub::center(&br, b), b.center());
        let tap = b.center();
        assert!(BlockheadHostStub::tap_is_within_body_radius(&br, b, tap));
        assert_eq!(BlockheadHostStub::will_die_if_hit_by_force(&br, b, 1), b.will_die_if_hit_by_force(1));
        BlockheadHostStub::hit_with_force_blockhead(&br, b, 0, None);
        assert!(!BlockheadHostStub::is_interacting_with_any_interaction_object(&br, b));
        BlockheadHostStub::set_interaction_object(&br, b, None);
        BlockheadHostStub::set_interaction_workbench(&br, b, None);
        BlockheadHostStub::stop_all_actions(&br, b);
        BlockheadHostStub::stop_fishing(&br, b);
        BlockheadHostStub::update_jet_motion(&br, b, 0.1);
        let _ = BlockheadHostStub::dpad_should_be_displayed(&br, b);
        let _ = BlockheadHostStub::current_interaction_requires_human_input(&br, b);
        BlockheadHostStub::die_for_good(&br, b);
        std::mem::forget((dw, dwref));
    }
}

#[cfg(test)]
mod cov_tests;
