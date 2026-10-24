//! `queueActionWithGoalPos:...`, queue helpers and predicates, teleport / jetpack, damage / death / naming / skin and
//! the trivial accessors of.
//!
//! `isNet == YES` on the server, so `updateJetMotion:` Branch B and `jetpackFuelCount == 10` are live for them);
//!
//! Accessors that `blockhead_core` already defines (`name`, `paused`, `view_radius`, `client_id`, `client_name`,
//! `client_save_dir`, `set_client_id`, `set_client_name`, `is_client_blockhead_being_controlled_by_server`,
//! `regenerating`, `waiting_for_path`, `fishing_rod_cast`,...) are NOT repeated here (would be E0592);

use std::any::Any;
use std::rc::Rc;

use crate::crafting::craftable_item_object::CraftableItemObjectRef;
use crate::crafting::workbench::Workbench;
use crate::fmath::cconv::f32_to_i32;
use crate::fmath::libm::powf;
use crate::fmath::vector::clamp;
use crate::foundation::nsstring;
use crate::items::inventory_item::ItemRef;
use crate::objects::base::{BlockheadApi, DynamicObject};
use crate::objects::class_info::class_info_by_name;
use crate::objects::handle::{downcast, is_kind_of_class, ObjRef};
use crate::physics_objects::fishing_rod::FishingRod;
use crate::plist::value::PlistValue;
use crate::types::enums::{BlockheadUnableToWorkReason, InteractionType, PathType};
use crate::types::item_type::ItemType;
use crate::types::structs::{BlockheadSkinOptions, IntPair, TradeMission, Vector2};
use crate::world::helpers::*;
use crate::world::loot::item_type_from_tile_is_foregorund;
use crate::world::state::World;
use crate::world::tiles::{tile_at_world_position_loaded, world_index_at_world_pos};

use super::action::{action_of, Action, InventoryChange};
use super::blockhead::{host, va, Blockhead, SaveSlot};
use super::interaction_rules::{sort_freeblocks_desc, wrap_x_distance};

/// `wrapX_float(d)` of (`half = (W<<5)/2` as float).
pub(crate) fn wrap_x_float(d: f32, world_width_macro: i32) -> f32 {
    let w = world_width_macro.wrapping_shl(5);
    let half = w / 2;
    if d < half as f32 {
        if ((w.wrapping_neg()) / 2) as f32 <= d {
            d
        } else {
            d + w as f32
        }
    } else {
        d - w as f32
    }
}

fn tat(world: &World, x: i32, y: i32) -> Option<crate::world::state::TileRef> {
    tile_at_world_position_loaded(x, y, world)
}

/// `[NSString stringWithFormat:@"%d", itemType]`.
fn key(t: u32) -> String {
    format!("{}", t as i32)
}

/// `[[d objectForKey:k] intValue]` (0 when absent).
fn ic_int(ic: &InventoryChange, k: &str) -> i32 {
    ic.get(&k.to_string()).map_or(0, |v| v.int_value())
}

impl Blockhead {
    // ================================================================================================
    // 3.1 queueActionWithGoalPos
    // ================================================================================================

    /// `-queueActionWithGoalPos:goalInteraction:pathType:interactionObjectID:craftableItemObject:craftCountOrExtraData:disableCancelCheck:isAI:`
    /// Returns true iff an `Action` was created and queued; every early exit returns false with no side
    /// effects (the cancel check may already have mutated the queue). Step order is observable (trap 7).
    #[allow(clippy::too_many_arguments)]
    pub fn queue_action_with_goal_pos_goal_interaction_path_type_interaction_object_id_craftable_item_object_craft_count_or_extra_data_disable_cancel_check_is_ai(
        &self,
        goal_pos: IntPair,
        goal_interaction: InteractionType,
        path_type: PathType,
        interaction_object_id: u64,
        craftable_item_object: Option<CraftableItemObjectRef>,
        craft_count_or_extra_data: i16,
        disable_cancel_check: bool,
        is_ai: bool,
    ) -> bool {
        let Some(world) = self.world_rc() else { return false };
        let h = host();
        let mut goal_interaction = goal_interaction;
        // 1. cancel check
        if !disable_cancel_check
            && self
                .cancel_any_action_at_goal_pos_or_with_interaction_object_id_goal_interaction_craft_count_or_extra_data(
                    goal_pos,
                    interaction_object_id,
                    goal_interaction.0 as i32,
                    craft_count_or_extra_data,
                )
        {
            return false;
        }
        // 2. AI gate
        if is_ai && !(self.is_run_by_ai() && self.bh.borrow().hat_item_type == 0x9a) {
            return false;
        }
        // 3. tile
        let Some(tile) = tat(&world, goal_pos.x, goal_pos.y) else { return false };
        // 4. visibility / knowledge
        let ctl = self.is_client_blockhead_being_controlled_by_server();
        if !ctl {
            if !self.tile_is_lit_for_self_at_pos_impl(&tile.get(), goal_pos) {
                return false;
            }
        } else if tile.get().explored_fraction == 0 {
            return false;
        }
        // 5. closeOrRevealed
        let mut sun_mul: f32 = 1.0;
        let cr = world.custom_rules();
        if cr.custom_rules_enabled != 0 {
            let sum = cr.sun_color[0] as u32 + cr.sun_color[1] as u32 + cr.sun_color[2] as u32;
            sun_mul = ((sum as f32) / 127.0f32) / 3.0f32;
        }
        let tl = tile.get();
        let lit = (tl.sun_light as f32) * sun_mul > 4.0f32
            || tl.artificial_light_r != 0
            || tl.artificial_light_g != 0
            || tl.artificial_light_b != 0;
        let wm = world.world_width_macro();
        let pos = self.pos();
        let close_or_revealed = if lit {
            true
        } else {
            let xl = wrap_x_distance(pos.x.wrapping_sub(goal_pos.x), wm);
            let yl = pos.y.wrapping_sub(goal_pos.y);
            xl.wrapping_mul(xl).wrapping_add(yl.wrapping_mul(yl)) < 9
        };
        if !close_or_revealed {
            return false;
        }
        // 6. DIG adjacency (plain DIG only)
        if goal_interaction == InteractionType::INTERACTION_DIG {
            let dx = wrap_x_distance(goal_pos.x.wrapping_sub(pos.x), wm);
            if !(dx == 0 && pos.y <= goal_pos.y && goal_pos.y <= pos.y + 1) {
                let (gx, gy) = (goal_pos.x, goal_pos.y);
                let open = tile_is_air_water_or_snow(tat(&world, gx, gy + 1))
                    || tile_is_air_water_or_snow(tat(&world, gx + 1, gy))
                    || tile_is_air_water_or_snow(tat(&world, gx - 1, gy))
                    || tile_is_air_water_or_snow(tat(&world, gx, gy - 1));
                if !open {
                    let queue = self.bh.borrow().action_queue.clone();
                    let mut found = false;
                    for q in queue.iter() {
                        let Some(a) = action_of(q) else { continue };
                        if a.goal_interaction() != InteractionType::INTERACTION_DIG {
                            continue;
                        }
                        let g = a.goal_tile_pos();
                        if (g.x == gx && (g.y == gy + 1 || g.y == gy - 1))
                            || (g.y == gy && (g.x == gx + 1 || g.x == gx - 1))
                        {
                            found = true;
                            break;
                        }
                    }
                    if !found {
                        return false;
                    }
                }
            }
        }
        // 7. predicted inventory change
        let mut ic: InventoryChange = InventoryChange::new();
        let item = self.current_item();
        match goal_interaction {
            InteractionType::INTERACTION_DIG | InteractionType::INTERACTION_DIG_FOREGROUND => {
                let cc = self.free_block_creation_count_for_tile_with_item(Some(&tile), item.as_ref());
                let fg = if goal_interaction == InteractionType::INTERACTION_DIG_FOREGROUND {
                    tile.get().foreground_contents != 0
                } else {
                    false
                };
                let mut ty = item_type_from_tile_is_foregorund(tile, goal_pos, fg, &world);
                let mut count = cc.count;
                if ty.0 == 11 && !world.should_be_crystal_block_at_pos(goal_pos) {
                    count = 0;
                }
                if ty.0 == 0 && tile.get().contents != 0 {
                    if let Some(obj) = h.interaction_object_at_pos(goal_pos) {
                        ty = ItemType(obj.as_interaction_like().map_or(0, |i| i.destroy_item_type()) as u32);
                        if ty.0 != 0 {
                            count = 1;
                        }
                    }
                }
                if count > 0 && ty.0 != 0 {
                    // the stored value is the ORIGINAL cc.count
                    ic.insert(key(ty.0), PlistValue::Int(cc.count as i64));
                }
            }
            InteractionType::INTERACTION_CRAFT => {
                if let Some(cio) = &craftable_item_object {
                    let ci = cio.craftable_item();
                    let n = craft_count_or_extra_data as i32;
                    ic.insert(key(ci.crafted_item_type), PlistValue::Int(ci.crafted_item_count.wrapping_mul(n) as i64));
                    let cnt = (ci.number_of_required_items.max(0) as usize).min(8);
                    for i in 0..cnt {
                        let k = key(ci.required_items[i]);
                        let cur = ic_int(&ic, &k);
                        ic.insert(
                            k,
                            PlistValue::Int(cur.wrapping_sub(ci.required_item_counts[i].wrapping_mul(n)) as i64),
                        );
                    }
                }
            }
            InteractionType::INTERACTION_FILL_BUCKET => {
                ic.insert(key(105), PlistValue::Int(1));
                ic.insert(key(102), PlistValue::Int(-1));
            }
            InteractionType::INTERACTION_PICKUP => {
                if let Some(pi) = self.pickup_item_for_tile_ast_pos(Some(&tile), goal_pos) {
                    ic.insert(key(pi.borrow().item_type.0), PlistValue::Int(1));
                }
            }
            InteractionType::INTERACTION_PICKUP_FREEBLOCK => {
                let mut list = h.free_blocks_at_pos(goal_pos);
                sort_freeblocks_desc(&mut list);
                let me = self.me();
                for fb in list.iter().rev() {
                    let Some(f) = crate::physics_objects::free_block::as_free_block(fb) else { continue };
                    let ty = ItemType(f.item_type() as u32);
                    self.bh.borrow_mut().z_index = world.is_admin() as i32;
                    let unable = self.freeblock_ownership_blocks(f, ty, &world);
                    if !unable {
                        let subs = f.sub_items();
                        let r = self.can_pick_up_item_of_type_sub_items_data_a_data_b(
                            ty,
                            subs.as_deref(),
                            f.data_a(),
                            f.data_b(),
                        );
                        if r == 1 && !fb.needs_removed() {
                            let k = key(ty.0);
                            let cur = ic_int(&ic, &k);
                            ic.insert(k, PlistValue::Int(cur.wrapping_add(1) as i64));
                        }
                    }
                }
                let _ = me;
            }
            InteractionType::INTERACTION_PLACE_ITEM
            | InteractionType::INTERACTION_PLACE_SOLID_BLOCK
            | InteractionType::INTERACTION_SOW => {
                if let Some(i) = &item {
                    ic.insert(key(i.borrow().item_type.0), PlistValue::Int(-1));
                }
            }
            InteractionType::INTERACTION_STAB => {
                if let Some(i) = &item {
                    let t = i.borrow().item_type.0;
                    if t == 42 || t == 257 {
                        goal_interaction = InteractionType::INTERACTION_FIRE_RANGED_WEAPON;
                    }
                }
            }
            _ => {}
        }
        // 8. create the action
        let item_index = self.bh.borrow().selected_tool_index as i16;
        let item_sub_index = self.current_item_sub_index() as i16;
        let a = Action::init_with_goal_pos_goal_interaction_path_type_interaction_item_item_index_item_sub_index_interaction_object_id_craftable_item_object_craft_count_or_extra_data_is_ai_inventory_change(
            goal_pos,
            goal_interaction,
            path_type,
            item,
            item_index,
            item_sub_index,
            interaction_object_id,
            craftable_item_object,
            craft_count_or_extra_data,
            is_ai,
            Some(ic),
        );
        // 9
        let res = self.good_or_bad_interaction_for_action(&a);
        a.set_interaction_test_result(res);
        // 10
        let any: Rc<dyn Any> = a;
        self.bh.borrow_mut().action_queue.push(any);
        self.stop_fishing();
        // 11
        if let Some(dw) = self.dw() {
            dw.dynamic_world_changed_at_pos_object_type(self.pos(), DynamicObject::object_type(self));
        }
        true
    }

    // ================================================================================================
    // queue helpers and predicates (3.3 - 3.24)
    // ================================================================================================

    /// `-hasActions`.
    pub fn has_actions(&self) -> bool {
        !self.bh.borrow().action_queue.is_empty()
    }

    /// `-actionCount`.
    pub fn action_count(&self) -> i32 {
        self.bh.borrow().action_queue.len() as i32
    }

    /// `-actionQueue` (clone of the vector of shared handles).
    pub fn action_queue(&self) -> Vec<Rc<dyn Any>> {
        self.bh.borrow().action_queue.clone()
    }

    /// `-stopAllActions`.
    pub fn stop_all_actions(&self) {
        {
            let mut f = self.bh.borrow_mut();
            f.waiting_for_fill_response = false;
            f.waiting_for_path = false;
        }
        let first = self.bh.borrow().action_queue.first().cloned();
        if let Some(first) = first {
            if action_of(&first).is_some_and(|a| a.in_progress()) {
                self.set_path_type_goal_interaction_extra_data(None, 2, 0, None);
            }
            self.stop_interacting();
            self.bh.borrow_mut().action_queue.clear();
        }
        if let Some(world) = self.world_rc() {
            world.abort_in_progress_path_if_for_blockhead(&self.me());
        }
    }

    /// `-fillReceiptsReturned:`.
    pub fn fill_receipts_returned(&self, receipts: &[u32]) {
        let mut f = self.bh.borrow_mut();
        if !f.waiting_for_fill_response {
            return;
        }
        for r in receipts {
            if *r == f.waiting_for_fill_response_index {
                f.waiting_for_fill_response = false;
                return;
            }
        }
    }

    /// `-waitingForFillRequestAtPos:`.
    pub fn waiting_for_fill_request_at_pos(&self, pos: IntPair) -> bool {
        let f = self.bh.borrow();
        if !f.waiting_for_fill_response {
            return false;
        }
        let Some(world) = self.world_rc() else { return false };
        f.waiting_for_fill_response_index as i32 == world_index_at_world_pos(pos, &world)
    }

    /// `-isRunByAI`: `blockheadAI != nil` (the AI is never constructed on the server).
    pub fn is_run_by_ai(&self) -> bool {
        super::inventory_model::inventory_host().has_blockhead_ai(self)
    }

    /// `-currentInteractionRequiresHumanInput`.
    pub fn current_interaction_requires_human_input(&self) -> bool {
        let (ride, t, wb, io) = {
            let f = self.bh.borrow();
            (
                f.ride_object.is_some(),
                f.state.interaction_type,
                f.interaction_workbench.clone(),
                f.interaction_object.clone(),
            )
        };
        if ride {
            return true;
        }
        if t == 13 || t == 25 || t == 26 {
            return true;
        }
        if t == 4 {
            // a nil workbench answers NO to `hasRequiredFuel` (INFERRED nil messaging)
            let has_fuel = match &wb {
                Some(w) => match downcast::<Workbench>(w) {
                    Some(w) => w.has_required_fuel(),
                    None => host().workbench_has_required_fuel(w),
                },
                None => false,
            };
            if !has_fuel {
                return true;
            }
            if self.unable_to_work_reason() != BlockheadUnableToWorkReason::WBBH_ABLE {
                return true;
            }
        }
        t == 5
            && io
                .as_deref()
                .and_then(crate::objects::base::DynamicObject::as_interaction_like)
                .is_some_and(crate::objects::interaction_object::InteractionObjectLike::requires_human_interaction)
    }

    /// `-setPaused:`. The `FishingRod` has no pause state in this port.
    // Note: `[fishingRod setPaused:]` has no counterpart on physics_objects::FishingRod.
    pub fn set_paused(&self, p: bool) {
        let changed = {
            let mut f = self.bh.borrow_mut();
            if p != f.paused {
                f.paused = p;
                true
            } else {
                false
            }
        };
        if changed {
            self.set_update_needs_to_be_sent(true);
        }
    }

    /// `-canFish`.
    pub fn can_fish(&self) -> bool {
        let (empty, anim, ride) = {
            let f = self.bh.borrow();
            (f.action_queue.is_empty(), f.state.animation_type, f.ride_object.clone())
        };
        empty
            && !self.meditating()
            && !self.on_trade_mission()
            && (anim == va::STAND || ride.is_some_and(|r| r.object_type() == 32))
    }

    /// `-stopFishing`.
    pub fn stop_fishing(&self) {
        if !self.is_net() {
            self.set_update_needs_to_be_sent(true);
        }
        let rod = {
            let mut f = self.bh.borrow_mut();
            f.fishing_rod_cast_x = 0;
            f.fishing_rod_cast_y = 0;
            f.fishing_rod.take()
        };
        if let Some(r) = rod {
            if let Some(rod) = r.downcast_ref::<FishingRod>() {
                rod.set_valid(false);
            }
        }
    }

    /// `-addToCrystalDiscrepency:` (sic).
    pub fn add_to_crystal_discrepency(&self, n: i32) {
        let mut f = self.bh.borrow_mut();
        f.controlled_by_server_crystal_discrepency = f.controlled_by_server_crystal_discrepency.wrapping_add(n);
    }

    /// `-doubleTimeUnlocked`.
    pub fn double_time_unlocked(&self) -> bool {
        if self.is_client_blockhead_being_controlled_by_server() {
            self.bh.borrow().client_double_time_unlocked
        } else {
            self.world_rc().is_some_and(|w| w.double_time_unlocked())
        }
    }

    /// `-localNetID` (`clientID` for net / controlled Blockheads).
    pub fn local_net_id(&self) -> Option<String> {
        <Self as BlockheadApi>::local_net_id(self)
    }

    /// `-addExpectedCraftItem:`.
    pub fn add_expected_craft_item(&self, t: ItemType) {
        self.bh.borrow_mut().expected_craft_items.push(t.0 as u16);
    }

    /// `-canCrawl`: always YES.
    pub fn can_crawl(&self) -> bool {
        true
    }

    /// `-hungerPaused`.
    pub fn hunger_paused(&self) -> bool {
        let f = self.bh.borrow();
        !(f.state.food_pause_timer <= 0.001f32) || f.hat_item_type == 140
    }

    /// `-unableToWorkReason`.
    pub fn unable_to_work_reason(&self) -> BlockheadUnableToWorkReason {
        if self.on_trade_mission() {
            return BlockheadUnableToWorkReason::WBBH_ABLE;
        }
        // the binary tests `-0.249f <= fullness` / `0.001f <= energy` for "able" (a NaN is therefore not able)
        if !(-0.249f32 <= self.fullness()) {
            return BlockheadUnableToWorkReason::WBBH_STARVING;
        }
        if !(0.001f32 <= self.energy()) {
            return BlockheadUnableToWorkReason::WBBH_TIRED;
        }
        BlockheadUnableToWorkReason::WBBH_ABLE
    }

    // ================================================================================================
    // teleport / jetpack (3.16 - 3.18, 3.33 - 3.41)
    // ================================================================================================

    /// The conjunction shared by `canTeleportToPos:` and `canEnterFreeFlightMode`.
    fn free_state_for_teleport(&self) -> bool {
        let f = self.bh.borrow();
        let sub = f.state.sub_animation_type;
        !(sub == 5 || sub == 6 || sub == 7)
            && f.state.regenerating == 0
            && f.state.on_trade_mission == 0
            && f.path.as_ref().is_none_or(|p| p.is_empty())
            && f.state.interacting == 0
            && !f.waiting_for_path
            && f.ride_object.is_none()
            && f.fishing_rod.is_none()
            && f.action_queue.is_empty()
    }

    /// `-canTeleportToPos:`.
    pub fn can_teleport_to_pos(&self, p: IntPair) -> bool {
        if !self.free_state_for_teleport() {
            return false;
        }
        let Some(world) = self.world_rc() else { return false };
        let pos = self.pos();
        let dx = wrap_x_distance(p.x.wrapping_sub(pos.x), world.world_width_macro());
        if dx.wrapping_abs() >= 5 {
            return true;
        }
        p.y.wrapping_sub(pos.y).wrapping_abs() > 4
    }

    /// `-teleportToPos:`.
    pub fn teleport_to_pos(&self, p: IntPair) -> bool {
        let ok = self.can_teleport_to_pos(p);
        if ok {
            self.update_position(p);
            let fp = Vector2 { x: (0.5f64 + (p.x as f64)) as f32, y: p.y as f32 };
            self.base.borrow_mut().float_pos = fp;
            self.bh.borrow_mut().render_pos = fp;
            let pos = self.pos();
            let t = self.world_rc().and_then(|w| tat(&w, pos.x, pos.y));
            {
                let mut f = self.bh.borrow_mut();
                f.next_to_square = pos;
                f.to_square = pos;
                f.prev_from_square = pos;
                f.from_square = pos;
                f.from_tile = t;
                f.to_tile = t;
            }
            self.set_update_needs_to_be_sent(true);
        }
        ok
    }

    /// `-canEnterFreeFlightMode`.
    pub fn can_enter_free_flight_mode(&self) -> bool {
        self.free_state_for_teleport()
    }

    /// The `subItems[2][0]` jetpack item of slot 0, when it is a jetpack.
    fn jetpack_item(&self) -> Option<ItemRef> {
        let f = self.bh.borrow();
        let slot0 = f.inventory_items.first()?;
        let first = slot0.borrow().first()?.clone();
        let subs = first.borrow().sub_items.clone()?;
        if subs.len() <= 2 {
            return None;
        }
        let fuel_slot = subs[2].borrow();
        let it = fuel_slot.first()?.clone();
        if it.borrow().item_type.0 != 278 {
            return None;
        }
        Some(it)
    }

    /// `-jetpackFuelCount` (all float32; `isNet` returns 10 first).
    pub fn jetpack_fuel_count(&self) -> i32 {
        if self.is_net() {
            return 10;
        }
        let Some(it) = self.jetpack_item() else { return 0 };
        let db = it.borrow().data_b;
        let f = 1.0f32 - (db as f32) / 16384.0f32;
        // (`dataB` is unsigned, so `f` is at most 1.0 and `n` at most 10)
        let n = f32_to_i32(10.0f32 * f + 0.99f32);
        if n < 1 {
            0
        } else {
            n
        }
    }

    /// `-addToJetFuel`.
    pub fn add_to_jet_fuel(&self) -> bool {
        let Some(it) = self.jetpack_item() else { return false };
        let mut i = it.borrow_mut();
        let mut nu = (i.data_b as i32) - 0x666;
        if nu < 0 {
            nu = 0;
        }
        i.data_b = (nu & 0xffff) as u16;
        drop(i);
        self.bh.borrow_mut().sub_inventory_changed_this_frame_slots[0][2] = true;
        true
    }

    /// `-canFly`.
    pub fn can_fly(&self) -> bool {
        self.bh.borrow().shirt_item_type == 278 && self.jetpack_fuel_count() > 0
    }

    /// `-jetPackIsLowOnFuel`.
    pub fn jet_pack_is_low_on_fuel(&self) -> bool {
        self.bh.borrow().shirt_item_type == 278 && self.jetpack_fuel_count() < 2
    }

    /// `-hasJetPackEquipped`.
    pub fn has_jet_pack_equipped(&self) -> bool {
        self.bh.borrow().shirt_item_type == 278
    }

    /// `-isInJetPackFreeFlightMode`.
    pub fn is_in_jet_pack_free_flight_mode(&self) -> bool {
        self.bh.borrow().is_in_jet_pack_free_flight_mode
    }

    /// `-freeBlockPickupRadius`.
    pub fn free_block_pickup_radius(&self) -> f32 {
        let (no_ride, mode, shirt) = {
            let f = self.bh.borrow();
            (f.ride_object.is_none(), f.is_in_jet_pack_free_flight_mode, f.shirt_item_type)
        };
        if no_ride && !mode {
            if shirt == 278 && self.jetpack_fuel_count() > 0 {
                return 1.0;
            }
            return 0.5;
        }
        1.0
    }

    /// `-freeFlightButtonTapped` (no static sender on the server).
    pub fn free_flight_button_tapped(&self) {
        self.stop_interacting();
        let mut f = self.bh.borrow_mut();
        f.smoothed_jet_pack_velocity = Vector2 { x: 0.0, y: 0.0 };
        f.jet_pack_velocity = Vector2 { x: 0.0, y: 0.0 };
        let rp = f.render_pos;
        drop(f);
        self.base.borrow_mut().float_pos = rp;
        let mut f = self.bh.borrow_mut();
        f.is_in_jet_pack_free_flight_mode = true;
        f.state.animation_type = va::FLY_JETPACK;
    }

    /// `-updateJetMotion:`. Branch A (`isNet == NO`: offline and AI Blockheads) and Branch B (`isNet == YES`:
    /// connected players, the replica branch). All float32 unless noted; plain `if`s for the control shaping and the
    /// project's C `clamp` (trap 10).
    pub fn update_jet_motion(&self, dt: f32) {
        let Some(world) = self.world_rc() else { return };
        let me = self.me();
        let wm = world.world_width_macro();
        let air = |t: Option<crate::world::state::TileRef>| tile_is_air_or_snow(t);
        let door_ok =
            |t: Option<crate::world::state::TileRef>, p: IntPair| tile_contains_usable_door(t, p, &world, Some(&me));
        let trap_ok = |t: Option<crate::world::state::TileRef>, p: IntPair| {
            tile_contains_usable_trap_door(t, p, &world, Some(&me))
        };
        let tile = |x: i32, y: i32| tat(&world, x, y);
        let open = |t: Option<crate::world::state::TileRef>, p: IntPair| air(t) || door_ok(t, p);

        if !self.is_net() {
            // ---- Branch A
            if self.bh.borrow().is_in_jet_pack_free_flight_mode && self.jetpack_fuel_count() == 0 {
                self.stop_riding();
            }
            if !self.bh.borrow().is_in_jet_pack_free_flight_mode {
                return;
            }
            {
                let mut f = self.bh.borrow_mut();
                f.jetpack_send_update_timer += dt;
                if f.jetpack_send_update_timer > 1.0f32 {
                    drop(f);
                    self.set_update_needs_to_be_sent(true);
                    self.bh.borrow_mut().jetpack_send_update_timer = 0.0;
                }
            }
            let (inc, mut v) = {
                let f = self.bh.borrow();
                (f.jet_pack_incoming_acceleration, f.jet_pack_velocity)
            };
            let mut hc = inc.x;
            let mut vc = inc.y;
            // control shaping
            let sgn = |h: f32| if h > 0.0f32 { 1.0f32 } else { -1.0f32 };
            let ah = hc.abs();
            if ah >= 0.2f32 {
                if ah >= 0.8f32 {
                    let base = if hc > 0.0f32 { 0.8f32 } else { -0.8f32 };
                    let t = hc - 0.2f32 * sgn(hc);
                    hc = base + (t - base) * 2.0f32;
                    hc = clamp(hc, -1.3f32, 1.3f32);
                } else {
                    hc = hc - 0.2f32 * sgn(hc);
                }
            } else {
                hc = 0.0;
            }
            let av = vc.abs();
            if av >= 0.1f32 {
                if av >= 0.8f32 {
                    let base = if vc > 0.0f32 { 0.7f32 } else { -0.7f32 };
                    let t = vc - 0.1f32 * sgn(vc);
                    vc = base + (t - base) * 2.0f32;
                    vc = clamp(vc, -1.3f32, 1.3f32);
                    if vc > 0.0f32 {
                        vc *= 0.75f32;
                    }
                } else {
                    vc = vc - 0.1f32 * sgn(vc);
                }
            } else {
                vc = 0.0;
            }
            let pos = self.pos();
            let fp = self.float_pos();
            // LEFT
            if hc < 0.0f32 || v.x < 0.0f32 {
                let p1 = IntPair { x: pos.x - 1, y: pos.y };
                let p2 = IntPair { x: pos.x - 1, y: pos.y + 1 };
                let blocked = !open(tile(p1.x, p1.y), p1) || !open(tile(p2.x, p2.y), p2);
                if blocked {
                    let d = wrap_x_float(fp.x - pos.x as f32, wm);
                    let lift = hc < 0.0f32 && open(tile(p2.x, p2.y), p2);
                    if lift {
                        vc += hc.abs();
                    }
                    if d >= 0.5f32 {
                        if hc < 0.0f32 {
                            hc *= d - 0.5f32;
                        }
                    } else {
                        hc = 1.0f32 - 2.0f32 * d;
                    }
                }
            }
            // RIGHT (uses the already modified hc)
            if hc > 0.0f32 || v.x > 0.0f32 {
                let p1 = IntPair { x: pos.x + 1, y: pos.y };
                let p2 = IntPair { x: pos.x + 1, y: pos.y + 1 };
                let blocked = !open(tile(p1.x, p1.y), p1) || !open(tile(p2.x, p2.y), p2);
                if blocked {
                    let d = wrap_x_float(fp.x - pos.x as f32, wm);
                    let lift = hc > 0.0f32 && open(tile(p2.x, p2.y), p2);
                    if lift {
                        vc += hc.abs();
                    }
                    if d <= 0.5f32 {
                        if hc > 0.0f32 {
                            hc *= 0.5f32 - (d - 0.5f32);
                        }
                    } else {
                        hc = -((d - 0.5f32) * 2.0f32);
                    }
                }
            }
            // DOWN
            if vc < 0.0f32 || v.y < 0.0f32 {
                let p = IntPair { x: pos.x, y: pos.y - 1 };
                let t = tile(p.x, p.y);
                let blocked = !(air(t) || trap_ok(t, p));
                if blocked {
                    let d = fp.y - pos.y as f32;
                    if d >= 0.5f32 {
                        if vc < 0.0f32 {
                            vc *= 0.1f32;
                        }
                    } else {
                        vc = 1.0f32 - 2.0f32 * d;
                    }
                }
            }
            // UP
            if vc > 0.0f32 || v.y > 0.0f32 {
                if pos.y + 1 < 0x3fc {
                    let p1 = IntPair { x: pos.x, y: pos.y + 1 };
                    let t1 = tile(p1.x, p1.y);
                    let mut blocked = !(air(t1) || trap_ok(t1, p1));
                    if !blocked {
                        let p2 = IntPair { x: pos.x, y: pos.y + 2 };
                        let t2 = tile(p2.x, p2.y);
                        blocked = !(air(t2) || trap_ok(t2, p2));
                    }
                    if blocked {
                        let d = fp.y - pos.y as f32;
                        if d <= 0.0f32 {
                            if vc > 0.0f32 {
                                vc *= 0.1f32;
                            }
                        } else {
                            vc = -(2.0f32 * d);
                        }
                    }
                } else {
                    if v.y > 0.0f32 {
                        v.y = 0.0;
                    }
                    if vc > 0.0f32 {
                        vc = 0.0;
                    }
                }
            }
            // integration
            v.x = v.x + (6.0f32 * hc - v.x) * (5.0f32 * dt);
            v.y = v.y + (6.0f32 * vc - v.y) * (5.0f32 * dt);
            if (v.x.abs() as f64) < 0.01f64 {
                v.x = 0.0;
            }
            if (v.y.abs() as f64) < 0.01f64 {
                v.y = 0.0;
            }
            let t = (vc as f64) * 0.1f64 * 6.0f64;
            let extra = if t <= 0.0f64 { 0.0f64 } else { t };
            let len = (v.x * v.x + v.y * v.y).sqrt();
            let k4 = dt * 4.0f32;
            let old_fp = fp;
            let new_fp = {
                let mut f = self.bh.borrow_mut();
                f.jet_pack_velocity = v;
                f.clamped_y_acceleration_for_fuel_usage_when_in_free_flight = (extra + (len as f64) * 0.1f64) as f32;
                let keep = (1.0f64 - (k4 as f64)) as f32;
                f.smoothed_jet_pack_velocity = Vector2 {
                    x: f.smoothed_jet_pack_velocity.x * keep + v.x * k4,
                    y: f.smoothed_jet_pack_velocity.y * keep + v.y * k4,
                };
                let nf = Vector2 { x: fp.x + v.x * dt, y: fp.y + v.y * dt };
                f.render_pos = nf;
                nf
            };
            self.base.borrow_mut().float_pos = new_fp;
            let new_pos = IntPair { x: f32_to_i32(new_fp.x), y: f32_to_i32(new_fp.y) };
            if new_pos == pos {
                return;
            }
            let nt = tile(new_pos.x, new_pos.y);
            if !air(nt)
                && (new_pos.x == pos.x || !door_ok(nt, new_pos))
                && (new_pos.y == pos.y || !trap_ok(nt, new_pos))
            {
                self.bh.borrow_mut().render_pos = old_fp;
                self.base.borrow_mut().float_pos = old_fp;
                return;
            }
            let (ft, tt) = (tile(pos.x, pos.y), nt);
            {
                let mut f = self.bh.borrow_mut();
                f.from_tile = ft;
                f.to_tile = tt;
            }
            self.set_update_needs_to_be_sent(true);
            self.update_position(new_pos);
            self.finish_jet_step(&world, new_pos);
        } else {
            // ---- Branch B
            if !self.bh.borrow().is_in_jet_pack_free_flight_mode {
                return;
            }
            {
                let mut f = self.bh.borrow_mut();
                f.jetpack_send_update_timer += dt;
                if f.jetpack_send_update_timer > 2.0f32 {
                    f.jet_pack_velocity = Vector2 { x: 0.0, y: 0.0 };
                }
            }
            let vel = self.bh.borrow().jet_pack_velocity;
            let vy01 = (vel.y as f64) * 0.1f64;
            let len = (vel.x * vel.x + vel.y * vel.y).sqrt();
            let clamped = ((if vy01 > 0.0f64 { vy01 } else { 0.0f64 }) + (len as f64) * 0.1f64) as f32;
            let mut inc = Vector2 { x: vel.x * 0.16666667f32, y: vel.y * 0.16666667f32 };
            if inc.y.abs() < 0.1f32 {
                inc.y = 0.0;
            }
            if inc.x.abs() < 0.1f32 {
                inc.x = 0.0;
            }
            let pos = self.pos();
            let old_fp = self.float_pos();
            let nf = Vector2 { x: old_fp.x + vel.x * dt, y: old_fp.y + vel.y * dt };
            {
                let mut f = self.bh.borrow_mut();
                f.clamped_y_acceleration_for_fuel_usage_when_in_free_flight = clamped;
                f.jet_pack_incoming_acceleration = inc;
                f.render_pos = nf;
            }
            self.base.borrow_mut().float_pos = nf;
            let new_pos = IntPair { x: f32_to_i32(nf.x), y: f32_to_i32(nf.y) };
            if new_pos == pos {
                return;
            }
            let nt = tile(new_pos.x, new_pos.y);
            if !air(nt)
                && !(new_pos.x != pos.x && door_ok(nt, new_pos))
                && !(new_pos.y != pos.y && trap_ok(nt, new_pos))
            {
                self.bh.borrow_mut().render_pos = old_fp;
                self.base.borrow_mut().float_pos = old_fp;
                return;
            }
            let ft = tile(pos.x, pos.y);
            {
                let mut f = self.bh.borrow_mut();
                f.from_tile = ft;
                f.to_tile = nt;
            }
            self.update_position(new_pos);
            self.finish_jet_step(&world, new_pos);
        }
    }

    /// The tail shared by both branches of `updateJetMotion:` (squares and tiles follow the new position).
    fn finish_jet_step(&self, world: &World, new_pos: IntPair) {
        let t = {
            let mut f = self.bh.borrow_mut();
            f.next_to_square = new_pos;
            f.to_square = new_pos;
            f.prev_from_square = new_pos;
            f.from_square = new_pos;
            f.from_square
        };
        let tl = tat(world, t.x, t.y);
        let mut f = self.bh.borrow_mut();
        f.from_tile = tl;
        f.to_tile = tl;
    }

    // ================================================================================================
    // damage, death, naming, skin (3.26 - 3.31, 3.42 - 3.45, 3.51 - 3.59)
    // ================================================================================================

    /// `-hitWithForce:blockhead:` (int -> f32 -> f64 `* 0.01` -> f32; trap 12).
    pub fn hit_with_force_blockhead(&self, force: i32, blockhead: Option<&ObjRef>) {
        let mut damage = (((force as f32) as f64) * 0.01f64) as f32;
        if blockhead.is_some() {
            damage *= 0.5f32;
        }
        let sim = self.world_rc().is_some_and(|w| w.is_simulating());
        self.suffer_damage_is_simulation_recoil(damage, sim, true);
        if !self.is_net() {
            let ride = self.bh.borrow().ride_object.clone();
            if let Some(ride) = ride {
                let is_npc = class_info_by_name("NPC").is_some_and(|c| is_kind_of_class(&ride, c));
                if is_npc {
                    if let Some(hm) = ride.as_harmable() {
                        hm.hit_with_force_blockhead(&ride, force / 4, blockhead);
                    }
                }
            }
        }
    }

    /// `-willDieIfHitByForce:`: always NO.
    pub fn will_die_if_hit_by_force(&self, _force: i32) -> bool {
        false
    }

    /// `-isVisible`.
    pub fn is_visible(&self) -> bool {
        self.bh.borrow().visible
    }

    /// `-center`.
    pub fn center(&self) -> Vector2 {
        let fp = self.float_pos();
        let (anim, sub) = {
            let f = self.bh.borrow();
            (f.state.animation_type, f.state.sub_animation_type)
        };
        let mut c = Vector2 { x: fp.x + 0.0f32, y: fp.y + 0.5f32 };
        if anim == 23 || sub == 5 || sub == 6 {
            c.y = fp.y;
        }
        c
    }

    /// `-interacting`.
    pub fn interacting(&self) -> bool {
        self.bh.borrow().state.interacting != 0
    }

    /// `-dieForGood` (acts only while regenerating; the sound line is dropped; trap 14).
    pub fn die_for_good(&self) {
        if self.bh.borrow().state.regenerating == 0 {
            return;
        }
        for sub in 0..4 {
            self.drop_inventory_items_at_index_sub_index_count_ignore_freeblocks(0, sub, -1, true);
        }
        for i in 1..=7 {
            self.drop_inventory_items_at_index_sub_index_count_ignore_freeblocks(i, -1, -1, true);
        }
        if let Some(dw) = self.dw() {
            dw.dynamic_world_changed_at_pos_object_type(self.pos(), DynamicObject::object_type(self));
        }
        let me = self.me();
        DynamicObject::set_needs_removed(self, &me, true);
        self.set_update_needs_to_be_sent(true);
    }

    /// `-isMale`.
    pub fn is_male(&self) -> bool {
        self.bh.borrow().skin_options.is_male != 0
    }

    /// `-changeName:`: 1..=15 UTF-16 units, no filter; the world-UI call is client-only (trap 15).
    pub fn change_name(&self, new_name: Option<&str>) {
        let Some(n) = new_name else { return };
        let len = nsstring::length(n);
        if len != 0 && len < 16 {
            self.bh.borrow_mut().name = Some(n.to_string());
            self.base.borrow_mut().creation_data_needs_to_be_sent = true;
        }
    }

    /// `-skinOptions`.
    pub fn skin_options(&self) -> BlockheadSkinOptions {
        self.bh.borrow().skin_options
    }

    /// `-customizationChanged:`.
    pub fn customization_changed(&self, o: BlockheadSkinOptions) {
        self.bh.borrow_mut().skin_options = o;
        self.update_skin();
    }

    /// `-customizationComplete:`.
    pub fn customization_complete(&self, o: BlockheadSkinOptions) {
        self.bh.borrow_mut().skin_options = o;
        self.update_skin();
        self.base.borrow_mut().creation_data_needs_to_be_sent = true;
        if let Some(dw) = self.dw() {
            dw.dynamic_world_changed_at_pos_object_type(self.pos(), DynamicObject::object_type(self));
        }
    }

    /// `-heldItemType`.
    pub fn held_item_type(&self) -> ItemType {
        let mut t = ItemType(self.bh.borrow().remote_interaction_item_type as u32);
        if !self.is_net() {
            let (mut it, interacting) = {
                let f = self.bh.borrow();
                (f.interaction_item.clone(), f.state.interacting != 0)
            };
            if it.is_none() && !interacting {
                it = self.current_item();
            }
            t = it.map_or(ItemType(0), |i| i.borrow().item_type);
        }
        t
    }

    /// `-moving`.
    pub fn moving(&self) -> bool {
        if self.meditating() || self.asleep() {
            return false;
        }
        if self.has_jet_pack_equipped() {
            return true;
        }
        let f = self.bh.borrow();
        f.from_square != f.to_square
    }

    /// `-isDoubleHeight`: always YES.
    pub fn is_double_height(&self) -> bool {
        true
    }

    /// `-abortCraft`: empty.
    pub fn abort_craft(&self) {}
    /// `-hurryCompletion:`: empty.
    pub fn hurry_completion(&self, _hurry_cost: i32) {}
    /// `-craftProgressUICompleteButtonTapped`: empty.
    pub fn craft_progress_ui_complete_button_tapped(&self) {}
    /// `-craftProgressUIRequiresCollectButtonWhenCompleted`: always YES.
    pub fn craft_progress_ui_requires_collect_button_when_completed(&self) -> bool {
        true
    }
    /// `-titleForCraftProgressUI` (CLIENT-ONLY): `name`.
    pub fn title_for_craft_progress_ui(&self) -> Option<String> {
        self.name()
    }
    /// `-customizeBlockheadUIShouldHaveOKButton` (CLIENT-ONLY): NO.
    pub fn customize_blockhead_ui_should_have_ok_button(&self) -> bool {
        false
    }

    /// `-hurryCostForCraftTimeRemaining:totalCraftTime:` (glibc 2.19 `powf`; trap 13).
    pub fn hurry_cost_for_craft_time_remaining_total_craft_time(
        &self,
        seconds_left: i32,
        total_craft_time: i32,
    ) -> i32 {
        let f = powf((((total_craft_time as f32) as f64) * 2.0f64) as f32, 0.5f32);
        let mut base = f32_to_i32(f);
        if base > 400 {
            base = 400;
        }
        if base < 2 {
            base = 1;
        }
        let cost0 = f32_to_i32(((seconds_left as f32) / (total_craft_time as f32)) * (base as f32));
        let mut cost = cost0.wrapping_add(1);
        if cost > 400 {
            cost = 400;
        }
        if cost < 2 {
            return 1;
        }
        cost
    }

    // ================================================================================================
    // CLIENT-ONLY but pure (3.8, 3.25, 3.46 - 3.48)
    // ================================================================================================

    /// `-tapIsWithinBodyRadius:` (pure; needed by `HarmableDynamicObject`).
    pub fn tap_is_within_body_radius(&self, tap: Vector2) -> bool {
        if self.regenerating() || self.on_trade_mission() {
            return false;
        }
        let fp = self.float_pos();
        let wm = self.world_rc().map_or(0, |w| w.world_width_macro());
        let d = wrap_x_float(tap.x - fp.x, wm);
        if (d as f64) < 0.5f64 && -0.5f64 < (d as f64) {
            return ((fp.y as f64) - 0.5f64) + 0.5f64 < (tap.y as f64)
                && (tap.y as f64) < (fp.y as f64) + 1.5f64 + 0.5f64;
        }
        false
    }

    /// `-dpadShouldBeDisplayed` (CLIENT-ONLY): NO on the dedicated server (no UI).
    pub fn dpad_should_be_displayed(&self) -> bool {
        false
    }

    /// `-beingControlledByDPad` (CLIENT-ONLY).
    pub fn being_controlled_by_d_pad(&self) -> bool {
        self.bh.borrow().being_controlled_by_d_pad
    }

    /// `-distanceTravelledThisDPadMovementSinceLastRequest` (CLIENT-ONLY, stateful).
    pub fn distance_travelled_this_d_pad_movement_since_last_request(&self) -> Vector2 {
        let fp = self.float_pos();
        let mut f = self.bh.borrow_mut();
        let mut total = f.last_d_pad_total_distance;
        if f.being_controlled_by_d_pad {
            total = Vector2 {
                x: fp.x - f.float_pos_when_d_pad_movement_started.x,
                y: fp.y - f.float_pos_when_d_pad_movement_started.y,
            };
        }
        let old = f.last_d_pad_total_distance_smoothed;
        let sm = Vector2 { x: old.x * 0.9f32 + total.x * 0.1f32, y: old.y * 0.9f32 + total.y * 0.1f32 };
        let diff = Vector2 { x: sm.x - old.x, y: sm.y - old.y };
        f.last_d_pad_total_distance = total;
        f.last_d_pad_total_distance_smoothed = sm;
        diff
    }

    // ================================================================================================
    // trivial accessors (3.61 - 3.85) not defined by blockhead_core
    // ================================================================================================

    /// `-nextPos`: `toSquare`.
    pub fn next_pos(&self) -> IntPair {
        self.bh.borrow().to_square
    }
    /// `-renderPos`.
    pub fn render_pos(&self) -> Vector2 {
        self.bh.borrow().render_pos
    }
    /// `-saveItemSlotsArray`.
    pub fn save_item_slots_array(&self) -> Vec<SaveSlot> {
        self.bh.borrow().save_item_slots_array.clone()
    }
    /// `-setClientSaveDir:`.
    pub fn set_client_save_dir(&self, d: Option<&str>) {
        self.bh.borrow_mut().client_save_dir = d.map(|s| s.to_string());
    }
    /// `-needsHarmFlash` (CLIENT-ONLY).
    pub fn needs_harm_flash(&self) -> bool {
        self.bh.borrow().needs_harm_flash
    }
    /// `-setNeedsHarmFlash:` (CLIENT-ONLY).
    pub fn set_needs_harm_flash(&self, v: bool) {
        self.bh.borrow_mut().needs_harm_flash = v;
    }
    /// `-rideObject`.
    pub fn ride_object(&self) -> Option<ObjRef> {
        self.bh.borrow().ride_object.clone()
    }
    /// `-isCurrentlyActiveBlockheadAccordingToNetData`.
    pub fn is_currently_active_blockhead_according_to_net_data(&self) -> bool {
        self.bh.borrow().is_currently_active_blockhead_according_to_net_data
    }
    /// `-setIsClientBlockheadBeingControlledByServer:`.
    pub fn set_is_client_blockhead_being_controlled_by_server(&self, v: bool) {
        self.bh.borrow_mut().is_client_blockhead_being_controlled_by_server = v;
    }
    /// `-currentTemperature`.
    pub fn current_temperature(&self) -> f32 {
        self.bh.borrow().current_temperature
    }
    /// `-cancelSimulateDueToCollapse`.
    pub fn cancel_simulate_due_to_collapse(&self) -> bool {
        self.bh.borrow().cancel_simulate_due_to_collapse
    }
    /// `-tipType` (CLIENT-ONLY).
    pub fn tip_type(&self) -> i32 {
        self.bh.borrow().tip_type
    }
    /// `-setTipType:` (CLIENT-ONLY).
    pub fn set_tip_type(&self, v: i32) {
        self.bh.borrow_mut().tip_type = v;
    }
    /// `-currentTradeMission`.
    pub fn current_trade_mission(&self) -> TradeMission {
        self.bh.borrow().current_trade_mission
    }
}

#[cfg(test)]
mod tests {
    use super::super::interaction_rules::testhost::*;
    use super::*;
    use crate::blockhead::blockhead::testutil::*;
    use crate::crafting::craftable_item_object::CraftableItemObject;
    use crate::items::inventory_item::InventoryItem;
    use crate::types::enums::PathType;
    use crate::types::structs::CraftableItem;
    use crate::world::state::TileRef;

    fn queue(
        b: &Blockhead,
        pos: IntPair,
        g: InteractionType,
        cio: Option<CraftableItemObjectRef>,
        count: i16,
        ai: bool,
    ) -> bool {
        b.queue_action_with_goal_pos_goal_interaction_path_type_interaction_object_id_craftable_item_object_craft_count_or_extra_data_disable_cancel_check_is_ai(
            pos, g, PathType(0), 0, cio, count, false, ai,
        )
    }

    /// The entries of the last queued action's `inventoryChange`, sorted by key (GSIMap enumeration order is the
    /// consumer's business; the dictionary itself is checked key by key).
    fn last_ic(b: &Blockhead) -> Vec<(String, i32)> {
        let q = b.bh.borrow().action_queue.clone();
        let a = action_of(q.last().unwrap()).unwrap();
        let ic = a.inventory_change().unwrap();
        let mut v: Vec<(String, i32)> =
            ic.keys_in_order().into_iter().map(|k| (k.clone(), ic.get(&k).unwrap().int_value())).collect();
        v.sort();
        v
    }

    fn setup_q() -> (Rc<World>, Rc<MockDwPub>, Rc<dyn crate::objects::base::DynamicObject>, Rc<AHost>) {
        let h = AHost::install();
        let (world, dw, dwref) = setup();
        let o = new_bh(&world, &dwref, 0);
        std::mem::forget(dwref);
        light_all(&world);
        (world, dw, o, h)
    }
    type MockDwPub = crate::blockhead::blockhead::testutil::MockDw;

    /// : early exits and the predicted inventory change per interaction.
    #[test]
    fn queue_action_rows() {
        let (w, dw, o, h) = setup_q();
        let b = bh_of(&o);
        let goal = IntPair { x: 10, y: 7 };
        // the AI gate needs a BlockheadAI and the tin foil hat
        assert!(!queue(b, goal, InteractionType::INTERACTION_DIG, None, 0, true));
        assert!(b.bh.borrow().action_queue.is_empty());
        // unlit tile
        tile(&w, 10, 7).update(|t| t.light = 0);
        assert!(!queue(b, goal, InteractionType::INTERACTION_DIG, None, 0, false));
        tile(&w, 10, 7).update(|t| t.light = 1);
        // fill bucket: keys in insertion order
        assert!(queue(b, IntPair { x: 10, y: 12 }, InteractionType::INTERACTION_FILL_BUCKET, None, 0, false));
        assert_eq!(last_ic(b), vec![("102".to_string(), -1), ("105".to_string(), 1)]);
        // queue bookkeeping: appended, dynamic world informed with (pos, Blockhead object type)
        assert_eq!(b.action_count(), 1);
        assert!(b.has_actions());
        assert_eq!(dw.changed.borrow().last().copied(), Some((IntPair { x: 5, y: 8 }, 24)));
        let a = action_of(&b.action_queue()[0]).unwrap();
        assert_eq!(a.goal_interaction(), InteractionType::INTERACTION_FILL_BUCKET);
        assert_eq!(a.interaction_object_id(), 0);
        assert!(!a.in_progress());
        // the object id and craft count are stored as given
        assert!(b
            .queue_action_with_goal_pos_goal_interaction_path_type_interaction_object_id_craftable_item_object_craft_count_or_extra_data_disable_cancel_check_is_ai(
                IntPair { x: 11, y: 12 },
                InteractionType::INTERACTION_USE_INTERACTIVE_OBJECT,
                PathType(4),
                77,
                None,
                -5,
                false,
                false
            ));
        let a2 = action_of(b.action_queue().last().unwrap()).unwrap();
        assert_eq!((a2.interaction_object_id(), a2.craft_count_or_extra_data(), a2.path_type()), (77, -5, PathType(4)));
        b.bh.borrow_mut().action_queue.truncate(1);
        // place / sow with an item: -1
        let it = give(b, 0x69, 0);
        b.set_current_item_to_item_at_index(IntPair { x: 1, y: -1 });
        assert!(Rc::ptr_eq(&b.current_item().unwrap(), &it));
        assert!(queue(b, IntPair { x: 12, y: 12 }, InteractionType::INTERACTION_PLACE_SOLID_BLOCK, None, 0, false));
        assert_eq!(last_ic(b), vec![("105".to_string(), -1)]);
        assert!(queue(b, IntPair { x: 13, y: 12 }, InteractionType::INTERACTION_SOW, None, 0, false));
        assert_eq!(last_ic(b), vec![("105".to_string(), -1)]);
        let a = action_of(b.action_queue().last().unwrap()).unwrap();
        assert!(Rc::ptr_eq(&a.interaction_item().unwrap(), &it));
        assert_eq!(a.interaction_item_index(), 1);
        // the result of goodOrBad is stored
        assert_eq!(a.interaction_test_result().render_item_type, 0x69);
        // stab with a bow becomes FIRE_RANGED_WEAPON and records nothing
        let bow = give(b, 42, 0);
        b.set_current_item_to_item_at_index(IntPair { x: 1, y: -1 });
        assert!(queue(b, IntPair { x: 14, y: 12 }, InteractionType::INTERACTION_STAB, None, 0, false));
        let a = action_of(b.action_queue().last().unwrap()).unwrap();
        assert_eq!(a.goal_interaction(), InteractionType::INTERACTION_FIRE_RANGED_WEAPON);
        assert!(a.inventory_change().unwrap().is_empty());
        let _ = bow;
        let sword = give(b, 0x32, 0);
        b.set_current_item_to_item_at_index(IntPair { x: 1, y: -1 });
        assert!(queue(b, IntPair { x: 15, y: 12 }, InteractionType::INTERACTION_STAB, None, 0, false));
        assert_eq!(
            action_of(b.action_queue().last().unwrap()).unwrap().goal_interaction(),
            InteractionType::INTERACTION_STAB
        );
        let _ = sword;
        // craft: sequential accumulation when a required item equals the crafted item
        let mut ci = CraftableItem::default();
        ci.crafted_item_type = 19;
        ci.crafted_item_count = 2;
        ci.required_items[0] = 11;
        ci.required_item_counts[0] = 3;
        ci.required_items[1] = 19;
        ci.required_item_counts[1] = 1;
        ci.number_of_required_items = 2;
        let cio: CraftableItemObjectRef = CraftableItemObject::init_with_craftable_item(ci);
        assert!(queue(b, IntPair { x: 16, y: 12 }, InteractionType::INTERACTION_CRAFT, Some(cio), 3, false));
        assert_eq!(last_ic(b), vec![("11".to_string(), -9), ("19".to_string(), 3)]);
        // craft without an object records nothing
        assert!(queue(b, IntPair { x: 17, y: 12 }, InteractionType::INTERACTION_CRAFT, None, 3, false));
        assert!(last_ic(b).is_empty());
        // other interactions: empty change
        assert!(queue(
            b,
            IntPair { x: 18, y: 12 },
            InteractionType::INTERACTION_USE_INTERACTIVE_OBJECT,
            None,
            0,
            false
        ));
        assert!(last_ic(b).is_empty());
        // negative craft count survives the save round trip as an unsigned number
        assert!(queue(b, IntPair { x: 19, y: 12 }, InteractionType::INTERACTION_ADD_FUEL, None, -1, false));
        let a = action_of(b.action_queue().last().unwrap()).unwrap();
        assert_eq!(a.craft_count_or_extra_data(), -1);
        // stopFishing was part of every queue
        assert!(b.bh.borrow().fishing_rod.is_none());
        let _ = h;
    }

    /// A tap on an already queued action cancels it instead of queueing a second one.
    #[test]
    fn queue_action_cancel_check() {
        let (_w, _dw, o, _h) = setup_q();
        let b = bh_of(&o);
        let g = IntPair { x: 10, y: 12 };
        assert!(queue(b, g, InteractionType::INTERACTION_USE_INTERACTIVE_OBJECT, None, 0, false));
        assert_eq!(b.action_count(), 1);
        assert!(!queue(b, g, InteractionType::INTERACTION_USE_INTERACTIVE_OBJECT, None, 0, false));
        assert_eq!(b.action_count(), 0);
        // with the cancel check disabled the second request is queued
        for _ in 0..2 {
            assert!(b
                .queue_action_with_goal_pos_goal_interaction_path_type_interaction_object_id_craftable_item_object_craft_count_or_extra_data_disable_cancel_check_is_ai(
                    g,
                    InteractionType::INTERACTION_USE_INTERACTIVE_OBJECT,
                    PathType(0),
                    0,
                    None,
                    0,
                    true,
                    false
                ));
        }
        assert_eq!(b.action_count(), 2);
    }

    /// Visibility, the closeOrRevealed test and the DIG adjacency rule.
    #[test]
    fn queue_action_visibility_and_dig_adjacency() {
        let (w, _dw, o, _h) = setup_q();
        let b = bh_of(&o);
        // enclosed rock tile far below: not open, not lit enough? make it dark and far
        let deep = IntPair { x: 20, y: 3 };
        tile(&w, 20, 3).update(|t| {
            t.sun_light = 0;
            t.artificial_light_r = 0;
            t.artificial_light_g = 0;
            t.artificial_light_b = 0;
        });
        // far and dark: dx = 15
        assert!(!queue(b, deep, InteractionType::INTERACTION_USE_INTERACTIVE_OBJECT, None, 0, false));
        // close and dark: (5,8) -> (6,7): 1 + 1 < 9
        tile(&w, 6, 7).update(|t| t.sun_light = 0);
        assert!(queue(b, IntPair { x: 6, y: 7 }, InteractionType::INTERACTION_USE_INTERACTIVE_OBJECT, None, 0, false));
        // a tile lit only by the custom-rules sun colour: 200 * 0.0 > 4 is false
        let mut cr = w.custom_rules();
        cr.custom_rules_enabled = 1;
        cr.sun_color = [0, 0, 0];
        w.set_custom_rules(cr);
        tile(&w, 20, 12).update(|t| t.sun_light = 200);
        assert!(!queue(
            b,
            IntPair { x: 20, y: 12 },
            InteractionType::INTERACTION_USE_INTERACTIVE_OBJECT,
            None,
            0,
            false
        ));
        cr.sun_color = [127, 127, 127];
        w.set_custom_rules(cr);
        assert!(queue(
            b,
            IntPair { x: 20, y: 12 },
            InteractionType::INTERACTION_USE_INTERACTIVE_OBJECT,
            None,
            0,
            false
        ));
        b.bh.borrow_mut().action_queue.clear();
        // DIG on an enclosed tile needs a queued neighbouring dig
        let enclosed = IntPair { x: 10, y: 4 };
        assert!(!queue(b, enclosed, InteractionType::INTERACTION_DIG, None, 0, false));
        assert!(queue(b, IntPair { x: 10, y: 7 }, InteractionType::INTERACTION_DIG, None, 0, false));
        // (10,7) is open (air above); a dig of the tile below it, (10,6), is enclosed unless (10,7) is queued
        assert!(queue(b, IntPair { x: 10, y: 6 }, InteractionType::INTERACTION_DIG, None, 0, false));
        assert!(!queue(b, IntPair { x: 12, y: 5 }, InteractionType::INTERACTION_DIG, None, 0, false));
        assert!(queue(b, IntPair { x: 12, y: 5 }, InteractionType::INTERACTION_DIG_FOREGROUND, None, 0, false));
        // the tile the Blockhead stands on / above it needs no neighbour
        let _ = tile(&w, 5, 8);
        // controlled by server: explored fraction decides
        b.bh.borrow_mut().is_client_blockhead_being_controlled_by_server = true;
        tile(&w, 15, 12).update(|t| t.explored_fraction = 0);
        assert!(!queue(
            b,
            IntPair { x: 15, y: 12 },
            InteractionType::INTERACTION_USE_INTERACTIVE_OBJECT,
            None,
            0,
            false
        ));
        tile(&w, 15, 12).update(|t| t.explored_fraction = 1);
        assert!(queue(
            b,
            IntPair { x: 15, y: 12 },
            InteractionType::INTERACTION_USE_INTERACTIVE_OBJECT,
            None,
            0,
            false
        ));
        let _: Option<TileRef> = None;
    }

    #[test]
    fn queue_action_pickup_freeblocks() {
        let (w, _dw, o, h) = setup_q();
        let b = bh_of(&o);
        let rdw = real_dw(&w, "aq_fb");
        let pos = IntPair { x: 10, y: 12 };
        let list = vec![mk_fb(&w, &rdw, pos, 3, 5), mk_fb(&w, &rdw, pos, 3, 9), mk_fb(&w, &rdw, pos, 4, 7)];
        *h.fbs.borrow_mut() = list.clone();
        b.bh.borrow_mut().z_index = 0;
        assert!(queue(b, pos, InteractionType::INTERACTION_PICKUP_FREEBLOCK, None, 0, false));
        // iteration is from the back of the descending list (smallest id first): item 3 (id 5), then 4, then 3 again
        assert_eq!(last_ic(b), vec![("3".to_string(), 2), ("4".to_string(), 1)]);
        assert_eq!(b.bh.borrow().z_index, 1);
        // a block that is being removed is not counted
        list[1].base().borrow_mut().needs_removed = true;
        b.bh.borrow_mut().action_queue.clear();
        assert!(queue(b, pos, InteractionType::INTERACTION_PICKUP_FREEBLOCK, None, 0, false));
        assert_eq!(last_ic(b), vec![("3".to_string(), 1), ("4".to_string(), 1)]);
    }

    #[test]
    fn stop_all_actions_flow() {
        let (_w, _dw, o, h) = setup_q();
        let b = bh_of(&o);
        let _ = h;
        assert!(queue(
            b,
            IntPair { x: 10, y: 12 },
            InteractionType::INTERACTION_USE_INTERACTIVE_OBJECT,
            None,
            0,
            false
        ));
        action_of(&b.action_queue()[0]).unwrap().set_in_progress(true);
        {
            let mut f = b.bh.borrow_mut();
            f.waiting_for_fill_response = true;
            f.waiting_for_path = true;
            f.state.interacting = 1;
            f.path = Some(vec![]);
        }
        b.stop_all_actions();
        assert!(b.bh.borrow().action_queue.is_empty());
        assert!(!b.bh.borrow().waiting_for_fill_response && !b.bh.borrow().waiting_for_path);
        assert_eq!(b.bh.borrow().state.interacting, 0);
        // an empty queue only resets the flags
        b.bh.borrow_mut().waiting_for_path = true;
        b.stop_all_actions();
        assert!(!b.bh.borrow().waiting_for_path);
    }

    #[test]
    fn fill_receipts_and_waiting() {
        let (world, _dw, o, _h) = setup_q();
        let b = bh_of(&o);
        let idx = crate::world::tiles::world_index_at_world_pos(IntPair { x: 3, y: 4 }, &world);
        assert!(!b.waiting_for_fill_request_at_pos(IntPair { x: 3, y: 4 }));
        {
            let mut f = b.bh.borrow_mut();
            f.waiting_for_fill_response = true;
            f.waiting_for_fill_response_index = idx as u32;
        }
        assert!(b.waiting_for_fill_request_at_pos(IntPair { x: 3, y: 4 }));
        assert!(!b.waiting_for_fill_request_at_pos(IntPair { x: 3, y: 5 }));
        b.fill_receipts_returned(&[1, 2]);
        assert!(b.bh.borrow().waiting_for_fill_response);
        b.fill_receipts_returned(&[1, idx as u32, 2]);
        assert!(!b.bh.borrow().waiting_for_fill_response);
        b.fill_receipts_returned(&[idx as u32]);
        assert!(!b.is_run_by_ai());
    }

    #[test]
    fn predicates_and_small_rules() {
        let (_w, _dw, o, _h) = setup_q();
        let b = bh_of(&o);
        // canFish
        assert!(b.can_fish());
        b.bh.borrow_mut().state.animation_type = 5;
        assert!(!b.can_fish());
        let boat = stub(32, true);
        b.bh.borrow_mut().ride_object = Some(boat);
        assert!(b.can_fish());
        b.bh.borrow_mut().ride_object = Some(stub(17, true));
        assert!(!b.can_fish());
        b.bh.borrow_mut().ride_object = None;
        b.bh.borrow_mut().state.animation_type = 0;
        // human input
        assert!(!b.current_interaction_requires_human_input());
        for t in [13, 25, 26] {
            b.bh.borrow_mut().state.interaction_type = t;
            assert!(b.current_interaction_requires_human_input());
        }
        b.bh.borrow_mut().state.interaction_type = 4; // nil workbench -> NO fuel -> YES
        assert!(b.current_interaction_requires_human_input());
        b.bh.borrow_mut().state.interaction_type = 5;
        assert!(!b.current_interaction_requires_human_input());
        b.bh.borrow_mut().state.interaction_type = 0;
        // hunger paused
        b.bh.borrow_mut().state.food_pause_timer = 0.001;
        assert!(!b.hunger_paused());
        b.bh.borrow_mut().state.food_pause_timer = 0.002;
        assert!(b.hunger_paused());
        b.bh.borrow_mut().state.food_pause_timer = 0.0;
        b.bh.borrow_mut().hat_item_type = 140;
        assert!(b.hunger_paused());
        b.bh.borrow_mut().hat_item_type = 0;
        // unable to work
        assert_eq!(b.unable_to_work_reason(), BlockheadUnableToWorkReason::WBBH_ABLE);
        b.bh.borrow_mut().state.fullness = -0.249;
        assert_eq!(b.unable_to_work_reason(), BlockheadUnableToWorkReason::WBBH_ABLE);
        b.bh.borrow_mut().state.fullness = -0.25;
        assert_eq!(b.unable_to_work_reason(), BlockheadUnableToWorkReason::WBBH_STARVING);
        b.bh.borrow_mut().state.energy = 0.0;
        assert_eq!(b.unable_to_work_reason(), BlockheadUnableToWorkReason::WBBH_STARVING);
        b.bh.borrow_mut().state.fullness = 0.5;
        assert_eq!(b.unable_to_work_reason(), BlockheadUnableToWorkReason::WBBH_TIRED);
        b.bh.borrow_mut().state.on_trade_mission = 1;
        assert_eq!(b.unable_to_work_reason(), BlockheadUnableToWorkReason::WBBH_ABLE);
        b.bh.borrow_mut().state.on_trade_mission = 0;
        // paused sets the update flag only on change
        o.set_update_needs_to_be_sent(false);
        b.set_paused(false);
        assert!(!o.update_needs_to_be_sent() && !b.paused());
        b.set_paused(true);
        assert!(o.update_needs_to_be_sent() && b.paused());
        // misc
        assert!(b.can_crawl() && b.is_double_height() && b.craft_progress_ui_requires_collect_button_when_completed());
        b.add_expected_craft_item(ItemType(19));
        assert_eq!(b.bh.borrow().expected_craft_items, vec![19u16]);
        b.bh.borrow_mut().controlled_by_server_crystal_discrepency = i32::MAX;
        b.add_to_crystal_discrepency(1);
        assert_eq!(b.bh.borrow().controlled_by_server_crystal_discrepency, i32::MIN);
        assert!(!b.double_time_unlocked());
        b.bh.borrow_mut().is_client_blockhead_being_controlled_by_server = true;
        b.bh.borrow_mut().client_double_time_unlocked = true;
        assert!(b.double_time_unlocked());
        assert!(!b.will_die_if_hit_by_force(1000));
        // heldItemType
        b.bh.borrow_mut().is_client_blockhead_being_controlled_by_server = false;
        b.bh.borrow_mut().remote_interaction_item_type = 99;
        assert_eq!(b.held_item_type(), ItemType(0));
        let it = give(b, 8, 0);
        b.set_current_item_to_item_at_index(IntPair { x: 1, y: -1 });
        assert_eq!(b.held_item_type(), ItemType(8));
        b.bh.borrow_mut().interaction_item = Some(InventoryItem::new(ItemType(6), 0, 0, None, None));
        assert_eq!(b.held_item_type(), ItemType(6));
        b.base.borrow_mut().is_net = true;
        assert_eq!(b.held_item_type(), ItemType(99));
        b.base.borrow_mut().is_net = false;
        let _ = it;
        // moving
        assert!(!b.moving());
        b.bh.borrow_mut().from_square = IntPair { x: 1, y: 1 };
        assert!(b.moving());
        b.bh.borrow_mut().state.sub_animation_type = 7;
        assert!(!b.moving());
        b.bh.borrow_mut().state.sub_animation_type = 0;
        let ts = b.bh.borrow().to_square;
        b.bh.borrow_mut().from_square = ts;
        b.bh.borrow_mut().shirt_item_type = 278;
        assert!(b.moving());
        // center / body radius
        b.bh.borrow_mut().shirt_item_type = 0;
        let fp = b.float_pos();
        assert_eq!(b.center(), Vector2 { x: fp.x, y: fp.y + 0.5 });
        b.bh.borrow_mut().state.sub_animation_type = 5;
        assert_eq!(b.center(), fp);
        b.bh.borrow_mut().state.sub_animation_type = 0;
        assert!(b.tap_is_within_body_radius(Vector2 { x: fp.x + 0.4, y: fp.y + 1.9 }));
        assert!(!b.tap_is_within_body_radius(Vector2 { x: fp.x + 0.5, y: fp.y + 1.0 }));
        assert!(!b.tap_is_within_body_radius(Vector2 { x: fp.x, y: fp.y + 2.0 }));
        assert!(!b.tap_is_within_body_radius(Vector2 { x: fp.x, y: fp.y }));
        // wrap: a tap at x - 1023.5 is 0.5 to the right across the seam only when exactly wrapping
        assert!(b.tap_is_within_body_radius(Vector2 { x: fp.x - 1024.0, y: fp.y + 1.0 }));
    }

    /// item 9: teleport.
    #[test]
    fn teleport_rules() {
        let (_w, _dw, o, h) = setup_q();
        h.dw_client.set(true);
        let b = bh_of(&o);
        assert!(!b.can_teleport_to_pos(IntPair { x: 8, y: 8 })); // dx 3, dy 0
        assert!(!b.can_teleport_to_pos(IntPair { x: 9, y: 12 })); // dx 4, dy 4
        assert!(b.can_teleport_to_pos(IntPair { x: 10, y: 8 })); // dx 5
        assert!(b.can_teleport_to_pos(IntPair { x: 5, y: 13 })); // dy 5
        assert!(b.can_teleport_to_pos(IntPair { x: 5, y: 3 }));
        assert!(!b.can_teleport_to_pos(IntPair { x: 2, y: 8 }));
        b.bh.borrow_mut().action_queue.push(Rc::new(()));
        assert!(!b.can_teleport_to_pos(IntPair { x: 20, y: 8 }));
        assert!(!b.can_enter_free_flight_mode());
        b.bh.borrow_mut().action_queue.clear();
        assert!(b.can_enter_free_flight_mode());
        for (name, f) in [
            (
                "sleeping",
                Box::new(|b: &Blockhead| b.bh.borrow_mut().state.sub_animation_type = 5) as Box<dyn Fn(&Blockhead)>,
            ),
            ("lying", Box::new(|b: &Blockhead| b.bh.borrow_mut().state.sub_animation_type = 6)),
            ("meditating", Box::new(|b: &Blockhead| b.bh.borrow_mut().state.sub_animation_type = 7)),
            ("regenerating", Box::new(|b: &Blockhead| b.bh.borrow_mut().state.regenerating = 1)),
            ("trade", Box::new(|b: &Blockhead| b.bh.borrow_mut().state.on_trade_mission = 1)),
            (
                "path",
                Box::new(|b: &Blockhead| b.bh.borrow_mut().path = Some(vec![crate::plist::value::PlistValue::int(1)])),
            ),
            ("interacting", Box::new(|b: &Blockhead| b.bh.borrow_mut().state.interacting = 1)),
            ("waiting", Box::new(|b: &Blockhead| b.bh.borrow_mut().waiting_for_path = true)),
            ("riding", Box::new(|b: &Blockhead| b.bh.borrow_mut().ride_object = Some(stub(32, true)))),
            ("fishing", Box::new(|b: &Blockhead| b.bh.borrow_mut().fishing_rod = Some(Rc::new(())))),
        ] {
            let saved = (b.bh.borrow().state, b.bh.borrow().path.clone(), b.bh.borrow().waiting_for_path);
            f(b);
            assert!(!b.can_teleport_to_pos(IntPair { x: 20, y: 8 }), "{name}");
            assert!(!b.can_enter_free_flight_mode(), "{name}");
            let mut fl = b.bh.borrow_mut();
            fl.state = saved.0;
            fl.path = saved.1;
            fl.waiting_for_path = saved.2;
            fl.ride_object = None;
            fl.fishing_rod = None;
        }
        // seam wrap: from x = 2 to x = 1020 is 6 tiles away
        b.base.borrow_mut().pos = IntPair { x: 2, y: 8 };
        assert!(b.can_teleport_to_pos(IntPair { x: 1020, y: 8 }));
        b.base.borrow_mut().pos = IntPair { x: 5, y: 8 };
        o.set_update_needs_to_be_sent(false);
        assert!(b.teleport_to_pos(IntPair { x: 20, y: 9 }));
        assert_eq!(b.pos(), IntPair { x: 20, y: 9 });
        assert_eq!(b.float_pos(), Vector2 { x: 20.5, y: 9.0 });
        assert_eq!(b.render_pos(), b.float_pos());
        let f = b.bh.borrow();
        assert!(
            f.from_square == f.to_square
                && f.to_square == f.next_to_square
                && f.prev_from_square == IntPair { x: 20, y: 9 }
        );
        drop(f);
        assert!(o.update_needs_to_be_sent());
        assert!(!b.teleport_to_pos(IntPair { x: 21, y: 9 }));
    }

    #[test]
    fn die_name_skin_and_hit() {
        let (_w, dw, o, h) = setup_q();
        let b = bh_of(&o);
        // dieForGood only acts while regenerating
        let _ = give(b, 3, 0);
        h.take();
        b.die_for_good();
        assert!(!o.needs_removed());
        assert!(h.take().is_empty());
        b.bh.borrow_mut().state.regenerating = 1;
        o.set_update_needs_to_be_sent(false);
        b.die_for_good();
        assert!(o.needs_removed() && o.update_needs_to_be_sent());
        assert!(h.take().iter().any(|l| l.starts_with("fb:")), "the carried item was dropped");
        assert_eq!(b.bh.borrow().inventory_items[1].borrow().len(), 0);
        assert!(dw.changed.borrow().contains(&(IntPair { x: 5, y: 8 }, 24)));
        // changeName: 1..=15 UTF-16 units
        let nm = |s: &str| {
            b.change_name(Some(s));
            b.name()
        };
        let before = b.name();
        o.base().borrow_mut().creation_data_needs_to_be_sent = false;
        assert_eq!(nm(""), before);
        assert!(!o.creation_data_needs_to_be_sent());
        assert_eq!(nm("a"), Some("a".to_string()));
        assert!(o.creation_data_needs_to_be_sent());
        assert_eq!(nm("123456789012345"), Some("123456789012345".to_string()));
        assert_eq!(nm("1234567890123456"), Some("123456789012345".to_string()));
        assert_eq!(
            nm("\u{1F600}\u{1F600}\u{1F600}\u{1F600}\u{1F600}\u{1F600}\u{1F600}"),
            Some("\u{1F600}\u{1F600}\u{1F600}\u{1F600}\u{1F600}\u{1F600}\u{1F600}".to_string())
        );
        assert_eq!(
            nm("\u{1F600}\u{1F600}\u{1F600}\u{1F600}\u{1F600}\u{1F600}\u{1F600}\u{1F600}"),
            Some("\u{1F600}\u{1F600}\u{1F600}\u{1F600}\u{1F600}\u{1F600}\u{1F600}".to_string())
        );
        b.change_name(None);
        // skin
        let mut so = b.skin_options();
        so.is_male = 1;
        so.face_index = 9;
        o.base().borrow_mut().creation_data_needs_to_be_sent = false;
        b.customization_changed(so);
        assert!(b.is_male() && b.skin_options().face_index == 9 && !o.creation_data_needs_to_be_sent());
        so.face_index = 4;
        b.customization_complete(so);
        assert_eq!(b.skin_options().face_index, 4);
        assert!(o.creation_data_needs_to_be_sent());
        // hitWithForce: damage = force * 0.01, halved when hit by another Blockhead
        b.bh.borrow_mut().state.regenerating = 0;
        let hp = b.health();
        b.hit_with_force_blockhead(20, None);
        let d1 = hp - b.health();
        assert!((d1 - 0.2).abs() < 1e-5, "{d1}");
        let hp = b.health();
        b.hit_with_force_blockhead(20, Some(&o));
        let d2 = hp - b.health();
        assert!((d2 - 0.1).abs() < 1e-5, "{d2}");
    }

    #[test]
    fn hurry_cost_values() {
        let (_w, _dw, o, _h) = setup_q();
        let b = bh_of(&o);
        let c = |s: i32, t: i32| b.hurry_cost_for_craft_time_remaining_total_craft_time(s, t);
        assert_eq!(c(100, 100), 15); // base 14 -> 14 + 1
        assert_eq!(c(50, 100), 8);
        assert_eq!(c(0, 100), 1);
        assert_eq!(c(5, 0), 1); // INT_MIN path
        assert_eq!(c(0, 0), 1);
        assert_eq!(c(1_000_000, 1_000_000), 400);
        assert_eq!(c(1, 1), 2); // base 1 (powf(2,.5)=1.41 -> 1), 1*1 + 1
        assert_eq!(c(3, 2), 4); // base 2: (1.5 * 2) = 3, + 1
    }

    #[test]
    fn jetpack_fuel_and_state() {
        let (_w, _dw, o, _h) = setup_q();
        let b = bh_of(&o);
        assert_eq!(b.jetpack_fuel_count(), 0);
        assert!(!b.add_to_jet_fuel());
        for (db, want) in [(0u16, 10), (1638, 9), (8192, 5), (14000, 2), (15000, 1), (16384, 0), (65535, 0)] {
            equip_jetpack(b, db);
            assert_eq!(b.jetpack_fuel_count(), want, "dataB {db}");
        }
        b.base.borrow_mut().is_net = true;
        assert_eq!(b.jetpack_fuel_count(), 10);
        b.base.borrow_mut().is_net = false;
        equip_jetpack(b, 5000);
        assert!(b.add_to_jet_fuel());
        assert_eq!(b.jetpack_item().unwrap().borrow().data_b, 5000 - 0x666);
        assert!(b.bh.borrow().sub_inventory_changed_this_frame_slots[0][2]);
        equip_jetpack(b, 100);
        assert!(b.add_to_jet_fuel());
        assert_eq!(b.jetpack_item().unwrap().borrow().data_b, 0);
        // flags
        equip_jetpack(b, 0);
        assert!(b.can_fly() && !b.jet_pack_is_low_on_fuel() && b.has_jet_pack_equipped());
        equip_jetpack(b, 15000);
        assert_eq!(b.jetpack_fuel_count(), 1);
        assert!(b.jet_pack_is_low_on_fuel() && b.can_fly());
        equip_jetpack(b, 16384);
        assert!(!b.can_fly() && b.jet_pack_is_low_on_fuel());
        // pickup radius
        assert_eq!(b.free_block_pickup_radius(), 0.5);
        equip_jetpack(b, 0);
        assert_eq!(b.free_block_pickup_radius(), 1.0);
        b.bh.borrow_mut().shirt_item_type = 0;
        assert_eq!(b.free_block_pickup_radius(), 0.5);
        b.bh.borrow_mut().is_in_jet_pack_free_flight_mode = true;
        assert_eq!(b.free_block_pickup_radius(), 1.0);
        assert!(b.is_in_jet_pack_free_flight_mode());
        b.bh.borrow_mut().is_in_jet_pack_free_flight_mode = false;
        b.bh.borrow_mut().ride_object = Some(stub(32, true));
        assert_eq!(b.free_block_pickup_radius(), 1.0);
        b.bh.borrow_mut().ride_object = None;
        // free flight button
        b.bh.borrow_mut().render_pos = Vector2 { x: 7.25, y: 9.5 };
        b.free_flight_button_tapped();
        let f = b.bh.borrow();
        assert!(f.is_in_jet_pack_free_flight_mode && f.state.animation_type == 32);
        assert_eq!(f.jet_pack_velocity, Vector2 { x: 0.0, y: 0.0 });
        assert_eq!(f.smoothed_jet_pack_velocity, Vector2 { x: 0.0, y: 0.0 });
        drop(f);
        assert_eq!(b.float_pos(), Vector2 { x: 7.25, y: 9.5 });
    }

    /// Branch A (offline / AI Blockheads) step function against hand-computed f32 values.
    #[test]
    fn jet_motion_branch_a() {
        let (w, _dw, o, h) = setup_q();
        h.dw_client.set(true);
        let b = bh_of(&o);
        equip_jetpack(b, 0);
        {
            let mut f = b.bh.borrow_mut();
            f.is_in_jet_pack_free_flight_mode = true;
            f.jet_pack_velocity = Vector2 { x: 0.0, y: 0.0 };
            f.smoothed_jet_pack_velocity = Vector2 { x: 0.0, y: 0.0 };
            f.jet_pack_incoming_acceleration = Vector2 { x: 0.0, y: 1.0 };
        }
        // vc: 1.0 -> base 0.7, t = 0.9, 0.7 + 0.2 * 2 = 1.1 -> clamp -> 0.75 * 1.1
        let vc = 0.75f32 * (0.7f32 + (1.0f32 - 0.1f32 - 0.7f32) * 2.0f32);
        let vy = 0.0f32 + (6.0f32 * vc - 0.0f32) * (5.0f32 * 0.1f32);
        b.update_jet_motion(0.1);
        let f = b.bh.borrow();
        assert_eq!(f.jet_pack_velocity, Vector2 { x: 0.0, y: vy });
        let fp_y = 8.0f32 + vy * 0.1f32;
        assert_eq!(f.render_pos.y, fp_y);
        assert_eq!(
            f.clamped_y_acceleration_for_fuel_usage_when_in_free_flight,
            ((vc as f64 * 0.1 * 6.0) + (vy as f64) * 0.1) as f32
        );
        drop(f);
        assert_eq!(b.pos(), IntPair { x: 5, y: 8 });
        // a rock ceiling above: vc is damped (d = 0.2475 > 0 -> vc = -(2 d))
        tile(&w, 5, 9).update(|t| t.type_index = 1);
        b.bh.borrow_mut().jet_pack_velocity = Vector2 { x: 0.0, y: 0.0 };
        b.base.borrow_mut().float_pos = Vector2 { x: 5.5, y: 8.0 };
        b.update_jet_motion(0.1);
        // d = 0 -> `d <= 0` -> vc = 0.1 * vc
        let vc2 = 0.1f32 * vc;
        let vy2 = 0.0f32 + (6.0f32 * vc2) * (5.0f32 * 0.1f32);
        assert_eq!(b.bh.borrow().jet_pack_velocity.y, vy2);
        tile(&w, 5, 9).update(|t| t.type_index = 2);
        // large dt moves across cells: updatePosition + squares + update flag
        b.bh.borrow_mut().jet_pack_velocity = Vector2 { x: 0.0, y: 0.0 };
        b.base.borrow_mut().float_pos = Vector2 { x: 5.5, y: 8.0 };
        o.set_update_needs_to_be_sent(false);
        b.update_jet_motion(0.5);
        assert_eq!(b.pos().x, 5);
        assert!(b.pos().y > 8);
        assert!(o.update_needs_to_be_sent());
        let f = b.bh.borrow();
        assert_eq!(f.from_square, b.pos());
        assert_eq!(f.to_square, b.pos());
        assert_eq!(f.next_to_square, b.pos());
        drop(f);
        // fuel 0 stops the flight (only the flag is cleared by stopRiding)
        equip_jetpack(b, 16384);
        b.update_jet_motion(0.1);
        assert!(!b.bh.borrow().is_in_jet_pack_free_flight_mode);
        // not in flight mode: nothing happens
        let before = b.float_pos();
        b.update_jet_motion(0.1);
        assert_eq!(b.float_pos(), before);
    }

    /// Branch B (connected players, `isNet == YES`).
    #[test]
    fn jet_motion_branch_b() {
        let (_w, _dw, o, h) = setup_q();
        h.dw_client.set(true);
        let b = bh_of(&o);
        b.base.borrow_mut().is_net = true;
        {
            let mut f = b.bh.borrow_mut();
            f.is_in_jet_pack_free_flight_mode = true;
            f.jet_pack_velocity = Vector2 { x: 3.0, y: 0.0 };
            f.jetpack_send_update_timer = 0.0;
        }
        b.update_jet_motion(0.1);
        let f = b.bh.borrow();
        assert_eq!(f.jet_pack_incoming_acceleration, Vector2 { x: 3.0f32 * 0.16666667f32, y: 0.0 });
        assert_eq!(f.clamped_y_acceleration_for_fuel_usage_when_in_free_flight, (0.0f64 + 3.0f64 * 0.1) as f32);
        assert_eq!(f.render_pos.x, 5.5f32 + 3.0f32 * 0.1f32);
        assert_eq!(f.jetpack_send_update_timer, 0.1);
        drop(f);
        assert_eq!(b.pos(), IntPair { x: 5, y: 8 });
        // no updateNeedsToBeSent for replicas, but the position follows
        o.set_update_needs_to_be_sent(false);
        b.update_jet_motion(1.0);
        assert_eq!(b.pos(), IntPair { x: 8, y: 8 });
        assert!(!o.update_needs_to_be_sent());
        // small control inputs are zeroed
        b.bh.borrow_mut().jet_pack_velocity = Vector2 { x: 0.3, y: 0.3 };
        b.update_jet_motion(0.0001);
        assert_eq!(b.bh.borrow().jet_pack_incoming_acceleration, Vector2 { x: 0.0, y: 0.0 });
        // the velocity is dropped after 2 s without an update
        b.bh.borrow_mut().jetpack_send_update_timer = 2.5;
        b.bh.borrow_mut().jet_pack_velocity = Vector2 { x: 5.0, y: 5.0 };
        let p = b.float_pos();
        b.update_jet_motion(0.1);
        assert_eq!(b.bh.borrow().jet_pack_velocity, Vector2 { x: 0.0, y: 0.0 });
        assert_eq!(b.float_pos(), p);
        // not flying
        b.bh.borrow_mut().is_in_jet_pack_free_flight_mode = false;
        b.bh.borrow_mut().jet_pack_velocity = Vector2 { x: 5.0, y: 5.0 };
        b.update_jet_motion(1.0);
        assert_eq!(b.float_pos(), p);
    }

    #[test]
    fn dpad_and_accessors() {
        let (_w, _dw, o, _h) = setup_q();
        let b = bh_of(&o);
        assert!(!b.dpad_should_be_displayed());
        // total distance smoothing
        b.bh.borrow_mut().last_d_pad_total_distance = Vector2 { x: 0.0, y: 0.0 };
        b.bh.borrow_mut().last_d_pad_total_distance_smoothed = Vector2 { x: 0.0, y: 0.0 };
        b.bh.borrow_mut().being_controlled_by_d_pad = true;
        b.bh.borrow_mut().float_pos_when_d_pad_movement_started = Vector2 { x: 4.5, y: 8.0 };
        let d = b.distance_travelled_this_d_pad_movement_since_last_request();
        assert_eq!(d, Vector2 { x: 0.0f32 * 0.9 + 1.0f32 * 0.1, y: 0.0 });
        assert!(b.being_controlled_by_d_pad());
        b.set_tip_type(3);
        assert_eq!(b.tip_type(), 3);
        b.set_needs_harm_flash(true);
        assert!(b.needs_harm_flash());
        b.set_client_save_dir(Some("/tmp/x"));
        assert_eq!(b.client_save_dir().as_deref(), Some("/tmp/x"));
        b.set_is_client_blockhead_being_controlled_by_server(true);
        assert!(b.is_client_blockhead_being_controlled_by_server());
        assert_eq!(b.next_pos(), b.bh.borrow().to_square);
        assert!(b.ride_object().is_none() && b.title_for_craft_progress_ui() == b.name());
        assert!(!b.customize_blockhead_ui_should_have_ok_button());
        b.abort_craft();
        b.hurry_completion(3);
        b.craft_progress_ui_complete_button_tapped();
        assert_eq!(
            b.local_net_id(),
            Some("cf1e8c14e54505f60aa10ceb8d5d8ab3".to_string()).filter(|_| false).or(b.client_id())
        );
    }

    use crate::objects::base::DynamicObjectBase;
    use crate::objects::class_info::{register_class_info, ClassInfo};
    use crate::objects::handle;
    use crate::objects::protocols::HarmableDynamicObject;
    use std::cell::{Cell, RefCell};

    /// An NPC-class mount that records the damage it is handed.
    struct RideNpc {
        base: RefCell<DynamicObjectBase>,
        hits: Cell<i32>,
    }
    impl crate::objects::base::DynamicObject for RideNpc {
        fn base(&self) -> &RefCell<DynamicObjectBase> {
            &self.base
        }
        fn class_info(&self) -> &'static ClassInfo {
            &crate::npc::npc::NPC_CLASS
        }
        fn as_any(&self) -> &dyn Any {
            self
        }
        fn as_harmable(&self) -> Option<&dyn HarmableDynamicObject> {
            Some(self)
        }
    }
    impl HarmableDynamicObject for RideNpc {
        fn tap_is_within_body_radius(&self, _tap: Vector2) -> bool {
            false
        }
        fn hit_with_force_blockhead(&self, _me: &ObjRef, force: i32, _b: Option<&ObjRef>) {
            self.hits.set(force);
        }
        fn will_die_if_hit_by_force(&self, _force: i32) -> bool {
            false
        }
        fn is_visible(&self) -> bool {
            true
        }
        fn center(&self) -> Vector2 {
            Vector2 { x: 0.0, y: 0.0 }
        }
    }

    fn mount_obj_base_is_zero(m: &RideNpc) -> bool {
        let any: &dyn Any = m.as_any();
        any.downcast_ref::<RideNpc>().is_some() && m.base().borrow().unique_id == 0
    }

    #[test]
    fn pure_helpers_and_trivial_accessors() {
        assert_eq!(wrap_x_float(100.0, 1), 68.0);
        assert_eq!(wrap_x_float(-100.0, 1), -68.0);
        assert_eq!(wrap_x_float(5.0, 1), 5.0);
        let (_w, _dw, o, _h) = setup_q();
        let b = bh_of(&o);
        assert!(b.is_visible() == b.bh.borrow().visible);
        assert!(!b.interacting());
        assert_eq!(b.save_item_slots_array().len(), b.bh.borrow().save_item_slots_array.len());
        assert!(!b.is_currently_active_blockhead_according_to_net_data());
        assert_eq!(b.current_temperature(), b.bh.borrow().current_temperature);
        assert!(!b.cancel_simulate_due_to_collapse());
        assert_eq!(b.render_pos(), b.bh.borrow().render_pos);
        let _ = b.current_trade_mission();
        assert!(b.is_double_height() && b.craft_progress_ui_requires_collect_button_when_completed());
        b.set_paused(true);
        b.set_paused(true);
        assert!(b.paused());
    }

    #[test]
    fn hits_pass_on_to_an_npc_mount() {
        register_class_info(&crate::npc::npc::NPC_CLASS);
        let (_w, _dw, o, _h) = setup_q();
        let b = bh_of(&o);
        let mount = handle::alloc_obj(|w| RideNpc {
            base: RefCell::new(DynamicObjectBase::alloc_zeroed_with(w)),
            hits: Cell::new(-1),
        });
        let mount_obj: ObjRef = mount.clone();
        b.bh.borrow_mut().ride_object = Some(mount_obj);
        b.hit_with_force_blockhead(40, None);
        assert_eq!(mount.hits.get(), 10);
        // the mount's other protocol methods
        assert!(!mount.tap_is_within_body_radius(Vector2 { x: 0.0, y: 0.0 }));
        assert!(!mount.will_die_if_hit_by_force(1));
        assert!(mount.is_visible());
        assert_eq!(mount.center(), Vector2 { x: 0.0, y: 0.0 });
        assert!(mount_obj_base_is_zero(&mount));
        // a mount that is not an NPC takes nothing; replicas pass nothing on
        mount.hits.set(-1);
        b.bh.borrow_mut().ride_object = Some(stub(1, true));
        b.hit_with_force_blockhead(40, None);
        b.base.borrow_mut().is_net = true;
        b.bh.borrow_mut().ride_object = Some(mount.clone() as ObjRef);
        b.hit_with_force_blockhead(40, None);
        assert_eq!(mount.hits.get(), -1);
    }

    #[test]
    fn human_input_rules_for_workbenches() {
        let (_w, _dw, o, h) = setup_q();
        let b = bh_of(&o);
        let _ = &h;
        // crafting without a workbench, or at one without fuel, needs a human
        b.bh.borrow_mut().state.interaction_type = 4;
        assert!(b.current_interaction_requires_human_input());
        // a fuelled workbench and a rested blockhead do not
        let f = crate::crafting::workbench::testkit::fixture();
        let (wb, wme) = crate::crafting::workbench::testkit::make(&f, 2, 10, 10);
        b.bh.borrow_mut().interaction_workbench = Some(wme.clone());
        assert!(wb.has_required_fuel());
        b.bh.borrow_mut().state.fullness = 1.0;
        b.bh.borrow_mut().state.energy = 1.0;
        assert!(!b.current_interaction_requires_human_input());
        // ... unless the blockhead is too tired to work
        b.bh.borrow_mut().state.energy = 0.0;
        assert!(b.current_interaction_requires_human_input());
        // other workbench-like objects ask the host
        b.bh.borrow_mut().interaction_workbench = Some(stub(45, true));
        b.bh.borrow_mut().state.energy = 1.0;
        let _ = b.current_interaction_requires_human_input();
        // interaction objects decide for themselves
        b.bh.borrow_mut().state.interaction_type = 5;
        b.bh.borrow_mut().interaction_object = None;
        assert!(!b.current_interaction_requires_human_input());
        b.bh.borrow_mut().state.interaction_type = 13;
        assert!(b.current_interaction_requires_human_input());
    }

    #[test]
    fn queue_action_predictions_for_objects_and_pickups() {
        let (w, _dw, o, h) = setup_q();
        let b = bh_of(&o);
        // digging an interaction object's tile predicts its item
        let tl = tile(&w, 10, 12);
        tl.update(|t| t.contents = 48);
        let chest = crate::interaction::chest::Chest::new(
            &w,
            &real_dw(&w, "aq_io"),
            IntPair { x: 10, y: 12 },
            None,
            Some(&InventoryItem::new(ItemType(0x413), 0, 0, None, None).borrow()),
            false,
            None,
            None,
            None,
        )
        .unwrap();
        *h.io.borrow_mut() = Some(chest.clone() as ObjRef);
        assert!(queue(b, IntPair { x: 10, y: 12 }, InteractionType::INTERACTION_DIG, None, 0, false));
        assert_eq!(last_ic(b), vec![("1043".to_string(), 1)]);
        // a queued non-dig action next to an enclosed dig does not count as a neighbour
        b.bh.borrow_mut().action_queue.clear();
        assert!(queue(
            b,
            IntPair { x: 11, y: 12 },
            InteractionType::INTERACTION_USE_INTERACTIVE_OBJECT,
            None,
            0,
            false
        ));
        b.bh.borrow_mut().action_queue.push(Rc::new(()));
        assert!(!queue(b, IntPair { x: 11, y: 5 }, InteractionType::INTERACTION_DIG, None, 0, false));
        b.bh.borrow_mut().action_queue.clear();
        // a free block that is no FreeBlock is skipped when predicting a pickup
        *h.fbs.borrow_mut() = vec![stub(14, true)];
        assert!(queue(b, IntPair { x: 12, y: 12 }, InteractionType::INTERACTION_PICKUP_FREEBLOCK, None, 0, false));
        assert!(last_ic(b).is_empty());
        // picking up a torch predicts the torch item
        tile(&w, 13, 12).update(|t| t.foreground_contents = b'1');
        *h.torch.borrow_mut() = Some(stub(20, true));
        assert!(queue(b, IntPair { x: 13, y: 12 }, InteractionType::INTERACTION_PICKUP, None, 0, false));
        assert_eq!(last_ic(b), vec![("17".to_string(), 1)]);
    }

    #[test]
    fn stopping_fishing_invalidates_the_rod() {
        let (w, _dw, o, _h) = setup_q();
        let b = bh_of(&o);
        let rod = crate::physics_objects::fishing_rod::FishingRod::init_with_world_blockhead_cache(&w, &o, None);
        rod.set_valid(true);
        b.bh.borrow_mut().fishing_rod = Some(rod.clone());
        b.bh.borrow_mut().fishing_rod_cast_x = 3;
        b.stop_fishing();
        assert!(b.bh.borrow().fishing_rod.is_none());
        assert_eq!(b.bh.borrow().fishing_rod_cast_x, 0);
    }

    /// Puts the blockhead (always at tile (10,16), fully airborne) into free flight with the given inputs.
    fn fly(b: &Blockhead, inc: (f32, f32), v: (f32, f32), fp: (f32, f32)) {
        equip_jetpack(b, 0);
        b.base.borrow_mut().pos = IntPair { x: 10, y: 16 };
        b.base.borrow_mut().float_pos = Vector2 { x: fp.0, y: fp.1 };
        let mut f = b.bh.borrow_mut();
        f.is_in_jet_pack_free_flight_mode = true;
        f.jet_pack_incoming_acceleration = Vector2 { x: inc.0, y: inc.1 };
        f.jet_pack_velocity = Vector2 { x: v.0, y: v.1 };
        f.smoothed_jet_pack_velocity = Vector2 { x: 0.0, y: 0.0 };
        f.jetpack_send_update_timer = 0.0;
    }

    fn air_around(w: &World) {
        for x in 6..15 {
            for y in 12..22 {
                tile(w, x, y).update(|t| t.type_index = 2);
            }
        }
    }

    #[test]
    fn free_flight_control_shaping_and_walls() {
        let (w, _dw, o, h) = setup_q();
        h.dw_client.set(true);
        let b = bh_of(&o);
        air_around(&w);
        let vel = |b: &Blockhead| b.bh.borrow().jet_pack_velocity;
        // weak inputs are dead zones, mid-sized ones are shifted, large ones are boosted
        fly(b, (0.05, 0.05), (0.0, 0.0), (10.5, 16.0));
        b.update_jet_motion(0.05);
        assert_eq!(vel(b), Vector2 { x: 0.0, y: 0.0 });
        fly(b, (0.5, 0.3), (0.0, 0.0), (10.5, 16.0));
        b.update_jet_motion(0.05);
        assert!(vel(b).x > 0.0 && vel(b).y > 0.0);
        fly(b, (-1.0, -1.0), (0.0, 0.0), (10.5, 16.0));
        b.update_jet_motion(0.05);
        assert!(vel(b).x < 0.0 && vel(b).y < 0.0);
        // the update flag is raised once a second has passed
        fly(b, (0.0, 0.0), (0.0, 0.0), (10.5, 16.0));
        b.bh.borrow_mut().jetpack_send_update_timer = 0.95;
        o.set_update_needs_to_be_sent(false);
        b.update_jet_motion(0.1);
        assert!(o.update_needs_to_be_sent());
        // every wall is compared with the same push in open air
        let step = |inc: (f32, f32), v: (f32, f32), fp: (f32, f32)| {
            fly(b, inc, v, fp);
            b.update_jet_motion(0.05);
            vel(b)
        };
        let free_left = step((-1.0, 0.0), (-1.0, 0.0), (10.5, 16.0)).x;
        let free_right = step((1.0, 0.0), (1.0, 0.0), (10.5, 16.0)).x;
        let free_down = step((0.0, -1.0), (0.0, -1.0), (10.5, 16.8)).y;
        let free_up = step((0.0, 1.0), (0.0, 1.0), (10.5, 16.4)).y;
        // a wall on the left slows the push; right at the cell edge it is cancelled, nearer the middle it is reversed
        tile(&w, 9, 16).update(|t| t.type_index = 1);
        assert!(step((-1.0, 0.0), (-1.0, 0.0), (10.5, 16.0)).x > free_left);
        assert!(step((-1.0, 0.0), (-1.0, 0.0), (10.3, 16.0)).x > free_left);
        tile(&w, 9, 16).update(|t| t.type_index = 2);
        // ... and one on the right the same way
        tile(&w, 11, 16).update(|t| t.type_index = 1);
        assert!(step((1.0, 0.0), (1.0, 0.0), (10.5, 16.0)).x < free_right);
        assert!(step((1.0, 0.0), (1.0, 0.0), (10.8, 16.0)).x < free_right);
        assert!(step((1.0, 0.0), (1.0, 0.0), (10.2, 16.0)).x < free_right);
        // pushing into a low wall lifts the body over it
        tile(&w, 11, 16).update(|t| t.type_index = 2);
        tile(&w, 9, 16).update(|t| t.type_index = 1);
        assert!(step((-1.0, 0.0), (0.0, 0.0), (10.5, 16.0)).y > 0.0);
        tile(&w, 9, 16).update(|t| t.type_index = 2);
        tile(&w, 11, 16).update(|t| t.type_index = 1);
        assert!(step((1.0, 0.0), (0.0, 0.0), (10.5, 16.0)).y > 0.0);
        tile(&w, 11, 16).update(|t| t.type_index = 2);
        // ground below: sinking is damped near the cell edge and turned into a bounce lower down
        tile(&w, 10, 15).update(|t| t.type_index = 1);
        assert!(step((0.0, -1.0), (0.0, -1.0), (10.5, 16.8)).y > free_down);
        assert!(step((0.0, -1.0), (0.0, -1.0), (10.5, 16.1)).y > free_down);
        tile(&w, 10, 15).update(|t| t.type_index = 2);
        // a ceiling one tile above, then two tiles above
        tile(&w, 10, 17).update(|t| t.type_index = 1);
        assert!(step((0.0, 1.0), (0.0, 1.0), (10.5, 16.0)).y < free_up);
        assert!(step((0.0, 1.0), (0.0, 1.0), (10.5, 16.4)).y < free_up);
        tile(&w, 10, 17).update(|t| t.type_index = 2);
        tile(&w, 10, 18).update(|t| t.type_index = 1);
        assert!(step((0.0, 1.0), (0.0, 1.0), (10.5, 16.4)).y < free_up);
        tile(&w, 10, 18).update(|t| t.type_index = 2);
        // the top of the world stops the climb
        fly(b, (0.0, 1.0), (0.0, 1.0), (10.5, 1020.0));
        b.base.borrow_mut().pos = IntPair { x: 10, y: 1020 };
        b.update_jet_motion(0.05);
        assert!(vel(b).y <= 0.0);
        // a big step into solid rock is undone
        fly(b, (1.0, 0.0), (0.0, 0.0), (10.5, 16.0));
        tile(&w, 16, 16).update(|t| t.type_index = 1);
        let before = b.float_pos();
        b.update_jet_motion(0.5);
        assert_eq!(b.float_pos(), before);
        assert_eq!(b.pos(), IntPair { x: 10, y: 16 });
        tile(&w, 16, 16).update(|t| t.type_index = 2);
    }

    #[test]
    fn replica_flight_collisions() {
        let (w, _dw, o, h) = setup_q();
        h.dw_client.set(true);
        let b = bh_of(&o);
        air_around(&w);
        b.base.borrow_mut().is_net = true;
        tile(&w, 13, 16).update(|t| t.type_index = 1);
        fly(b, (0.0, 0.0), (6.0, 0.0), (10.5, 16.0));
        let before = b.float_pos();
        b.update_jet_motion(0.5);
        assert_eq!(b.float_pos(), before, "the replica does not enter rock either");
        tile(&w, 13, 16).update(|t| t.type_index = 2);
        fly(b, (0.0, 0.0), (6.0, 0.0), (10.5, 16.0));
        b.update_jet_motion(0.5);
        assert_eq!(b.pos().x, 13);
    }

    #[test]
    fn jetpack_fuel_without_the_container() {
        let (_w, _dw, o, _h) = setup_q();
        let b = bh_of(&o);
        // no clothing slot item with enough sub slots, or no jetpack in the fuel slot
        b.bh.borrow().inventory_items[0].borrow_mut().clear();
        assert_eq!(b.jetpack_fuel_count(), 0);
        assert!(!b.add_to_jet_fuel());
        assert!(!b.can_fly());
    }
}

#[cfg(test)]
mod cov_tests;
