//! `Blockhead` interaction start/stop, drop-count rules, collapse/sleep/meditate/die, the network update
//! receiver, item pickup and damage.
//!

use crate::items::inventory_item::{InventoryItem, ItemRef};
use crate::items::predicates::{
    interaction_type_disallowed_on_protected_tiles, item_type_is_column, item_type_is_painting, item_type_is_stairs,
    item_type_is_torch,
};
use crate::nslog;
use crate::objects::base::{
    base_remote_creation_data_update, base_remote_update, padded_bytes, DynamicObject, WorldApi,
};
use crate::objects::handle::{ptr_eq, ObjRef};
use crate::plist::value::PlistValue;
use crate::rng::glibc_rand::{r01_from, rand_tagged};
use crate::types::item_type::ItemType;
use crate::types::netdata::BlockheadUpdateData;
use crate::types::structs::{FreeBlockCreationCount, IntPair, Vector, Vector2, WireStruct};
use crate::world::helpers::{
    tile_is_bush_or_scrub, tile_is_plant, tile_is_solid, tile_is_tree_trunk,
    tile_is_unplaced_block_minable_with_pickaxe,
};
use crate::world::state::{TileRef, World};
use crate::world::tiles::{get_world_pos_for_world_index, tile_at_world_position_loaded};

use super::blockhead::{
    host, it, state_asleep, tip, va, vsa, Blockhead, ExtraValue, PathExtraData, Remover, SimulationEventExtra,
};

fn tile_at(world: &World, p: IntPair) -> Option<TileRef> {
    tile_at_world_position_loaded(p.x, p.y, world)
}

use crate::fmath::vector::clampi;

const SPADES: [u32; 4] = [0x10, 0x40, 0x28, 0x59];

impl Blockhead {
    // ================================================================================================
    // startInteractingWithTileAtIndex:tile:interactionType:
    // ================================================================================================

    /// `-startInteractingWithTileAtIndex:tile:interactionType:`.
    /// `t` is an `InteractionType` raw value. Every `uiManager` call is CLIENT-ONLY (dead on server).
    pub fn start_interacting_with_tile_at_index_tile_interaction_type(
        &self,
        tile_index: i32,
        tile: Option<TileRef>,
        t: u32,
    ) {
        let h = host();
        let me = self.me();
        let mut t = t;
        // 1.
        self.bh.borrow_mut().gather_timer_left_over = 0.0;
        // 2.
        if t != it::FIRE_RANGED_WEAPON && t != it::STAB {
            h.stop_riding(self);
        }
        let is_net = self.is_net();
        let ctl = self.is_client_blockhead_being_controlled_by_server();
        let item_type_of_interaction_item = |s: &Blockhead| -> i32 { s.interaction_item_type() };

        // 3. RIDE, 4. RIDE_BOAT, 6. RIDE_TRAIN_CAR without proxy
        if t == it::RIDE
            || t == it::RIDE_BOAT
            || (t == it::RIDE_TRAIN_CAR && {
                let tapped = self.bh.borrow().tapped_ride_object.clone();
                tapped.is_none_or(|o| o.as_ridable().is_none_or(|r| r.acts_as_interaction_object().is_none()))
            })
        {
            // UI: setFuelUIShouldBeDisplayed:NO (CLIENT-ONLY)
            let src = {
                let f = self.bh.borrow();
                if t == it::RIDE {
                    f.tapped_npc.clone()
                } else {
                    f.tapped_ride_object.clone()
                }
            };
            self.bh.borrow_mut().ride_object = src.clone();
            let itype = item_type_of_interaction_item(self);
            let can = src.as_ref().is_some_and(|r| h.ridable_blockhead_can_ride_using_item(r, self, itype));
            if !can {
                self.bh.borrow_mut().ride_object = None;
                return;
            }
            let r = src.unwrap();
            if let Some(rid) = r.as_ridable() {
                rid.add_rider(&r, &me);
                rid.set_target_velocity(Vector2 { x: 0.0, y: 0.0 });
            }
            if t == it::RIDE {
                let mut anim = va::RIDE;
                if h.ride_object_rider_rides_with_arms_down(&r) {
                    anim = va::RIDE_ARMS_DOWN;
                }
                let mut f = self.bh.borrow_mut();
                f.state.animation_type = anim;
                f.state.goal_interaction_type_while_walking = it::NONE;
                f.chasing_npc = false;
            } else if t == it::RIDE_BOAT {
                let mut f = self.bh.borrow_mut();
                f.state.animation_type = va::RIDE;
                f.state.goal_interaction_type_while_walking = it::NONE;
                f.chasing_ride_object = false;
            } else {
                // RIDE_TRAIN_CAR without proxy: no animation change, no isNet/ctl guard, no achievement
                let extra = self.bh.borrow().path_extra_data.clone();
                if let Some(e) = extra {
                    h.set_workbench_choice_ui_option(&r, e.int_for_key("craftCountOrExtraData"));
                }
                let mut f = self.bh.borrow_mut();
                f.state.goal_interaction_type_while_walking = it::NONE;
                f.chasing_ride_object = false;
                // `requiresFuel` -> setFuelUIShouldBeDisplayed:YES (CLIENT-ONLY)
                return;
            }
            if is_net || ctl {
                return;
            }
            if t == it::RIDE {
                if h.is_donkey(&r) && h.donkey_breed(&r) <= 0xb {
                    h.report_achievement_with_identifier("grp.donkey");
                }
            } else {
                h.report_achievement_with_identifier("grp.boat");
            }
            return;
        }
        // 5. REMOVE_BOAT
        if t == it::REMOVE_BOAT {
            self.bh.borrow_mut().chasing_ride_object = false;
            let o = self.bh.borrow().tapped_ride_object.clone();
            if let Some(o) = o {
                h.pickup_dynamic_object(self, &o);
            }
            return;
        }
        // 6. RIDE_TRAIN_CAR with proxy: after rewriting `t` it continues DIRECTLY to the common part (step 10);
        //    steps 7-9 are the other arms of the same if/else-if chain and are not re-tested.
        //    Note: spec text "continue to the common part".
        let mut via_proxy = false;
        if t == it::RIDE_TRAIN_CAR {
            let proxy = {
                let tapped = self.bh.borrow().tapped_ride_object.clone();
                tapped.and_then(|o| o.as_ridable().and_then(|r| r.acts_as_interaction_object()))
            };
            self.bh.borrow_mut().chasing_ride_object = false;
            let proxy = proxy.unwrap();
            if !h.can_use_dynamic_object(self, &proxy) {
                return;
            }
            let mut e = PathExtraData::new();
            e.set("interactionObject", ExtraValue::Obj(proxy));
            self.bh.borrow_mut().path_extra_data = Some(e);
            t = it::USE_INTERACTIVE_OBJECT;
            via_proxy = true;
        }
        // 7. REMOVE_TRAIN_CAR
        if !via_proxy && t == it::REMOVE_TRAIN_CAR {
            self.bh.borrow_mut().chasing_ride_object = false;
            let o = self.bh.borrow().tapped_ride_object.clone();
            if let Some(o) = o {
                h.pickup_dynamic_object(self, &o);
            }
            return;
        }
        // 8. USE_INTERACTIVE_OBJECT with pathExtraData
        if !via_proxy && t == it::USE_INTERACTIVE_OBJECT {
            let extra = self.bh.borrow().path_extra_data.clone();
            if let Some(e) = extra {
                let obj = e.object_for_key("interactionObject");
                // `[self canUseDynamicObject:nil]` is NO
                let ok = obj.as_ref().is_some_and(|o| h.can_use_dynamic_object(self, o));
                if !ok {
                    return;
                }
                let o = obj.unwrap();
                if h.interaction_object_type(&o) == 7 {
                    h.set_workbench_choice_ui_option(&o, e.int_for_key("craftCountOrExtraData"));
                }
            }
        }
        // 9. INSPECT_NPC
        if t == it::INSPECT_NPC {
            let npc = self.bh.borrow().tapped_npc.clone();
            if let Some(n) = npc {
                // [uiManager showPetUIForPet:] CLIENT-ONLY
                h.npc_begin_blockhead_inspection(&n, self);
            }
        }
        // 10. common part
        let world = self.world_rc();
        {
            let mut f = self.bh.borrow_mut();
            f.state.interaction_timer = 0.0;
            f.state.interacting = 1;
            if let Some(w) = &world {
                let p = get_world_pos_for_world_index(tile_index, w);
                f.state.interacting_square = p;
            }
            f.interacting_tile = tile;
            f.state.gather_speed = 1.0f32;
            f.sound_timer = 0.0;
            f.state.interaction_type = t;
            f.state.goal_interaction_type_while_walking = t;
        }
        h.update_gather_speed_and_animation_for_current_interation_and_item(self);
        self.set_update_needs_to_be_sent(true);
        // 11. by type
        let extra = self.bh.borrow().path_extra_data.clone();
        match t {
            it::CRAFT => {
                if let Some(e) = extra {
                    let item = e.any_for_key("craftableItemObject");
                    let wb = e.object_for_key("workbench");
                    let count = e.int_for_key("count");
                    let precond = item.is_some() && wb.as_ref().is_some_and(|w| !w.needs_removed()) && count != 0;
                    if precond {
                        let wb = wb.unwrap();
                        if h.workbench_is_in_use(&wb) {
                            let cur = h.workbench_current_blockhead(&wb);
                            if !cur.is_some_and(|c| ptr_eq(&c, &me)) {
                                self.stop_interacting();
                                return;
                            }
                        }
                        if h.workbench_craft_item_with_blockhead_count(&wb, &item.unwrap(), self, count) {
                            // [uiManager blockheadReachedCraftDestination:...] CLIENT-ONLY
                            return;
                        }
                        self.stop_interacting();
                    } else {
                        self.stop_interacting();
                    }
                }
            }
            it::USE_INTERACTIVE_OBJECT => {
                if let Some(e) = extra {
                    h.blockhead_reached_interaction_object_destination_path_extra_data(&me, Some(&e));
                }
            }
            it::ADD_FUEL => {
                if let Some(e) = extra {
                    if let Some(wb) = e.object_for_key("workbench") {
                        h.workbench_start_managing_fuel_with_blockhead(&wb, self);
                    }
                    // [uiManager blockheadReachedAddFuelDestination:workbench:] CLIENT-ONLY
                }
            }
            _ => {}
        }
    }

    // ================================================================================================
    // stopInteracting
    // ================================================================================================

    /// `-stopInteracting`.
    pub fn stop_interacting(&self) {
        let h = host();
        h.stop_fishing(self);
        let npc = self.bh.borrow().tapped_npc.clone();
        if let Some(n) = npc {
            if h.is_npc(&n) {
                h.npc_inspection_stopped(&n);
            }
        }
        let is_net = self.is_net();
        {
            let mut f = self.bh.borrow_mut();
            f.interaction_item = None;
            f.interaction_item_index = -1;
            f.interaction_item_sub_index = -1;
            f.state.interacting = 0;
            // subAnimationType 5/6 yawn sound: CLIENT-ONLY
            // [uiManager hideSleepUIForBlockhead:] CLIENT-ONLY
            f.state.interaction_type = 0;
            f.state.goal_interaction_type_while_walking = 0;
            f.state.sub_animation_type = 0;
            f.interaction_object = None;
            f.interaction_workbench = None;
            f.tapped_npc = None;
            f.tapped_ride_object = None;
            f.chasing_npc = false;
            f.chasing_ride_object = false;
        }
        if !is_net {
            if let Some(dw) = self.dw() {
                dw.dynamic_world_changed_at_pos_object_type(self.pos(), DynamicObject::object_type(self));
            }
        }
        self.set_update_needs_to_be_sent(true);
        self.bh.borrow_mut().local_gather_progress = 0;
        // `[dynamicWorld activeBlockhead] == self`: false on the server
    }

    // ================================================================================================
    // free block counts
    // ================================================================================================

    /// `-freeBlockBonusCreationCount`: 0.
    pub fn free_block_bonus_creation_count(&self) -> i32 {
        0
    }

    /// `-freeBlockCreationCountForTile:withItem:`.
    /// the verification notes: the count is KEPT when `r >= 0.7`). One `rand()` only for cactus contents.
    pub fn free_block_creation_count_for_tile_with_item(
        &self,
        tile: Option<&TileRef>,
        item: Option<&ItemRef>,
    ) -> FreeBlockCreationCount {
        let dflt =
            FreeBlockCreationCount { count: 1, bonus_multiplier: 1, bonus_color: Vector { v: [1.0, 1.0, 1.0, 1.0] } };
        let expert = self.world_rc().is_some_and(|w| w.expert_mode());
        let base_n: i32 = if expert { 2 } else { 3 };
        let ty: u32 = item.map_or(0, |i| i.borrow().item_type.0);
        let Some(tile) = tile else { return dflt };
        let tr = Some(*tile);
        let contents = tile.contents();
        let mut r = dflt;
        // 2. clay / flint
        if contents == 2 || contents == 1 {
            if item.is_none() {
                return dflt;
            }
            if SPADES.contains(&ty) {
                r.count = base_n;
            }
            return r;
        }
        // 3. not bush / scrub
        if !tile_is_bush_or_scrub(tr) {
            if tile_is_tree_trunk(tr) {
                if item.is_none() {
                    return dflt;
                }
                if ty == 6 || ty == 0x21 || ty == 0x46 {
                    r.count = base_n;
                }
                return r;
            }
            let c1 = (tile.type_index() == 0x10
                || matches!(contents, 0x3d | 0x3e | 0x3f | 0x40 | 0x41 | 0x4d | 0x6a | 0x6b))
                && item.is_some();
            let multi = |r: &mut FreeBlockCreationCount, mult: i32, color: [f32; 4], count: i32| {
                r.count = count;
                r.bonus_multiplier = mult;
                r.bonus_color = Vector { v: color };
            };
            if !c1 {
                if !tile_is_unplaced_block_minable_with_pickaxe(tr) {
                    return dflt;
                }
                match ty {
                    0x5e => multi(&mut r, 2, [0.53, 0.45, 1.0, 1.0], 2),
                    0x5f => multi(&mut r, 3, [0.0, 0.0, 1.0, 1.0], 3),
                    0x60 => multi(&mut r, 4, [0.0, 1.0, 0.0, 1.0], 4),
                    0x61 => multi(&mut r, 5, [1.0, 0.0, 0.0, 1.0], 5),
                    0x62 => multi(&mut r, 6, [0.25, 1.0, 1.0, 1.0], 6),
                    _ => {}
                }
                return r;
            }
            match ty {
                8 | 0x22 | 0x2b | 0x5a | 0x42 | 0x5d | 0x11d => {
                    r.count = base_n;
                    r.bonus_multiplier = 1;
                }
                0x5e => multi(&mut r, 2, [0.53, 0.45, 1.0, 1.0], base_n * 2),
                0x5f => multi(&mut r, 3, [0.0, 0.0, 1.0, 1.0], base_n * 3),
                0x60 => multi(&mut r, 4, [0.0, 1.0, 0.0, 1.0], base_n * 4),
                0x61 => multi(&mut r, 5, [1.0, 0.0, 0.0, 1.0], base_n * 5),
                0x62 => multi(&mut r, 6, [0.25, 1.0, 1.0, 1.0], base_n * 6),
                _ => {
                    if contents == 0x3f && ty == 0xaf {
                        // `expertMode` queried again in the original
                        r.count = if expert { 3 } else { 5 };
                    }
                }
            }
            return r;
        }
        // 4. bush or scrub
        if item.is_some() && matches!(ty, 0x19 | 0x31 | 0x43) {
            r.count = base_n;
        } else if item.is_some() && tile_is_plant(tr) && SPADES.contains(&ty) {
            r.count = base_n;
        }
        if contents == 0x2b || contents == 0x2c {
            let rr = r01_from(rand_tagged("blockhead/free_block_creation_count/cactus"));
            // (int)(0.3 (f64) + (double)r) * count
            r.count = crate::fmath::cconv::f64_to_i32(0.3f64 + rr as f64).wrapping_mul(r.count);
        }
        r
    }

    // ================================================================================================
    // state predicates
    // ================================================================================================

    /// `-canCollapse`. `(double)energy <= 0.251` uses the RAW stored energy.
    pub fn can_collapse(&self) -> bool {
        let Some(world) = self.world_rc() else { return false };
        let pos = self.pos();
        let tile = tile_at(&world, pos);
        let below = tile_at(&world, IntPair { x: pos.x, y: pos.y.wrapping_sub(1) });
        let f = self.bh.borrow();
        let s = &f.state;
        if s.sub_animation_type == vsa::LIE_ON_BED
            || s.sub_animation_type == vsa::LIE_ON_GROUND
            || s.sub_animation_type == vsa::MEDITATING
        {
            return false;
        }
        if !(s.animation_type == va::LIE || s.animation_type == va::STAND) {
            return false;
        }
        if !(s.interacting == 0 || matches!(s.interaction_type, 1 | 0x17 | 0x18)) {
            return false;
        }
        if f.ride_object.is_some() {
            return false;
        }
        // (the riding animations 0x1b..0x21 were already refused by the LIE / STAND test above)
        if f.waiting_for_path {
            return false;
        }
        if !((s.energy as f64) <= 0.251 || s.fullness <= -0.249f32) {
            return false;
        }
        if f.underwater || f.fishing_rod.is_some() || s.regenerating != 0 || s.on_trade_mission != 0 {
            return false;
        }
        match tile {
            Some(t) if t.type_index() != 3 => {}
            _ => return false,
        }
        below.is_some() && tile_is_solid(below)
    }

    /// `-canSleepOnSpot`.
    pub fn can_sleep_on_spot(&self) -> bool {
        self.can_rest_common(true)
    }

    /// `-canMeditate`: as `canSleepOnSpot` without the energy test and with `subAnimationType` not in
    /// {5,6,7}.
    pub fn can_meditate(&self) -> bool {
        self.can_rest_common(false)
    }

    fn can_rest_common(&self, sleep: bool) -> bool {
        let Some(world) = self.world_rc() else { return false };
        let pos = self.pos();
        let tile = tile_at(&world, pos);
        let below = tile_at(&world, IntPair { x: pos.x, y: pos.y.wrapping_sub(1) });
        let f = self.bh.borrow();
        let s = &f.state;
        if s.sub_animation_type == vsa::LIE_ON_BED || s.sub_animation_type == vsa::LIE_ON_GROUND {
            return false;
        }
        if sleep {
            if !matches!(s.animation_type, 0x17 | 0 | 0x20) {
                return false;
            }
        } else if !matches!(s.animation_type, 0x17 | 0 | 0x20) {
            return false;
        }
        if f.path.as_ref().map_or(0, |p| p.len()) != 0 {
            return false;
        }
        if s.interacting != 0 || f.ride_object.is_some() || f.waiting_for_path {
            return false;
        }
        if sleep && !((s.energy as f64) < 0.75) {
            return false;
        }
        if f.underwater || f.fishing_rod.is_some() || s.regenerating != 0 || s.on_trade_mission != 0 {
            return false;
        }
        if s.sub_animation_type == vsa::MEDITATING || f.drowning_in_space {
            return false;
        }
        match tile {
            Some(t) if t.type_index() != 3 => {}
            _ => return false,
        }
        if !f.action_queue.is_empty() {
            return false;
        }
        below.is_some() && tile_is_solid(below)
    }

    /// `-canEat`.
    pub fn can_eat(&self) -> bool {
        let s = self.bh.borrow().state;
        (s.fullness < 0.9999f32 || s.health < 0.9999f32) && s.regenerating == 0 && s.on_trade_mission == 0
    }

    // ================================================================================================
    // die
    // ================================================================================================

    /// `-die`: the rule engine (`dieForGood` is blockhead_actions).
    ///  `rand()` for drop-some).
    pub fn die(&self) {
        let h = host();
        let me = self.me();
        {
            let mut f = self.bh.borrow_mut();
            f.state.death = 1.0f32;
            if f.state.regenerating != 0 || f.state.on_trade_mission != 0 {
                return;
            }
        }
        // Mods (not in the original): `death` of a blockhead simulated by the server.
        if let Some(h) = crate::mods::hooks() {
            h.death(self.client_id().as_deref(), self.unique_id());
        }
        let world = self.world_rc();
        let rules = world.as_ref().map(|w| w.custom_rules()).unwrap_or_default();
        let enabled = rules.custom_rules_enabled != 0;
        let can_regenerate = !(enabled && !matches!(rules.death, 0..=2));
        // sound die.wav: CLIENT-ONLY
        let expert = world.as_ref().is_some_and(|w| w.expert_mode());
        let mut drop_all = expert;
        let mut drop_some = !drop_all;
        if enabled {
            if rules.death_drops == 0 {
                drop_all = true;
                drop_some = false;
            } else {
                drop_all = false;
                drop_some = rules.death_drops == 1;
            }
        }
        if drop_all {
            for i in 0..8 {
                if i == 0 {
                    for j in 0..4 {
                        h.drop_inventory_items_at_index_sub_index_count_ignore_freeblocks(self, 0, j, -1, true);
                    }
                } else {
                    h.drop_inventory_items_at_index_sub_index_count_ignore_freeblocks(self, i, -1, -1, true);
                }
            }
        } else if drop_some {
            for _ in 0..3 {
                let r1 = r01_from(rand_tagged("blockhead/die/drop_index"));
                let idx = clampi(crate::fmath::cconv::f64_to_i32((r1 as f64) * 7.99), 0, 7);
                let r2 = r01_from(rand_tagged("blockhead/die/drop_sub"));
                let mut sub = clampi(crate::fmath::cconv::f64_to_i32((r2 as f64) * 4.99), 0, 4);
                if sub == 4 {
                    sub = -1;
                }
                h.drop_inventory_items_at_index_sub_index_count_ignore_freeblocks(self, idx, sub, -1, true);
            }
        }
        self.bh.borrow_mut().state.regeneration_progress = 0.0;
        h.stop_all_actions(self);
        self.stop_interacting();
        h.stop_riding(self);
        h.set_path_type_goal_interaction_extra_data(self, None, 2, 0, None);
        h.abort_in_progress_path_if_for_blockhead(&me);
        let pos = self.pos();
        {
            let mut f = self.bh.borrow_mut();
            f.waiting_for_path = false;
            f.chasing_ride_object = false;
            f.chasing_npc = false;
            f.next_to_square = pos;
            f.to_square = pos;
            f.prev_from_square = pos;
            f.from_square = pos;
            f.travel_fraction = 0.0;
            f.action_queue.clear();
            f.state.regenerating = 1;
        }
        if let Some(dw) = self.dw() {
            dw.dynamic_world_changed_at_pos_object_type(pos, DynamicObject::object_type(self));
        }
        self.set_update_needs_to_be_sent(true);
        if !can_regenerate {
            h.die_for_good(self);
        } else {
            self.bh.borrow_mut().tip_type = tip::REGENERATING;
            // `activeBlockhead == self` -> displayRegenerateUI: dead on server
        }
    }

    // ================================================================================================
    // collapse / sleep / meditate
    // ================================================================================================

    /// `-collapseIfPossible`.
    pub fn collapse_if_possible(&self) {
        if !self.can_collapse() {
            return;
        }
        let h = host();
        let me = self.me();
        let first = self.bh.borrow().action_queue.first().cloned();
        if let Some(first) = first {
            if h.action_in_progress(&first) {
                h.action_set_in_progress(&first, false);
                // `insertObject:atIndex:1`: the action is NOT removed from index 0, so it is duplicated
                self.bh.borrow_mut().action_queue.insert(1, first);
            }
        }
        self.stop_interacting();
        h.stop_riding(self);
        self.set_update_needs_to_be_sent(true);
        self.bh.borrow_mut().state.sub_animation_type = vsa::LIE_ON_GROUND;
        // sigh sound: CLIENT-ONLY; displaySleepUI: dead on server
        self.bh.borrow_mut().cancel_simulate_due_to_collapse = true;
        let simulating = self.world_rc().is_some_and(|w| w.is_simulating());
        let ctl = self.is_client_blockhead_being_controlled_by_server();
        let energy = self.bh.borrow().state.energy;
        if !simulating && !ctl {
            self.bh.borrow_mut().tip_type =
                if energy >= 0.01f32 { tip::COLLAPSE_STARVATION } else { tip::COLLAPSE_EXHAUSTED };
        } else {
            let txt = if energy >= 0.01f32 { "STARVATION" } else { "EXHAUSTION" };
            h.add_simulation_event_of_type_for_blockhead_extra_data(
                5,
                &me,
                SimulationEventExtra::Text(txt.to_string()),
            );
        }
        let mut f = self.bh.borrow_mut();
        if f.state.fullness < 0.0 {
            f.state.fullness = 0.0;
            if (f.state.energy as f64) > 0.8 {
                f.state.energy = 0.8f32;
            }
        }
        if f.state.energy < 0.25f32 {
            f.state.energy = 0.25f32;
        }
    }

    /// `-sleepOnSpotIfPossible`.
    pub fn sleep_on_spot_if_possible(&self) {
        if self.can_sleep_on_spot() {
            self.bh.borrow_mut().action_queue.clear();
            self.stop_interacting();
            host().stop_riding(self);
            self.bh.borrow_mut().state.sub_animation_type = vsa::LIE_ON_GROUND;
            // sigh sound / displaySleepUI: CLIENT-ONLY
        }
    }

    /// `-meditateIfPossible`.
    pub fn meditate_if_possible(&self) {
        if self.can_meditate() {
            self.bh.borrow_mut().action_queue.clear();
            self.stop_interacting();
            host().stop_riding(self);
            self.bh.borrow_mut().state.sub_animation_type = vsa::MEDITATING;
        }
    }

    /// `-sleepOnSpotIfPossibleOtherwiseCancelActions`.
    pub fn sleep_on_spot_if_possible_otherwise_cancel_actions(&self) {
        if self.can_sleep_on_spot() {
            self.sleep_on_spot_if_possible();
        } else {
            host().stop_all_actions(self);
        }
    }

    // ================================================================================================
    // remoteUpdate:
    // ================================================================================================

    /// `-remoteUpdate:`: the receiver of `BlockheadUpdateData` (0x70). Applies only to net
    /// replicas.
    pub fn remote_update_impl(&self, me: &ObjRef, net_data: &[u8]) {
        if !self.is_net() {
            return;
        }
        let h = host();
        base_remote_update(me, net_data);
        self.bh.borrow_mut().jetpack_send_update_timer = 0.0;
        let d = BlockheadUpdateData::read_le(&padded_bytes(net_data, 0x70));
        let world = self.world_rc();
        let has_server = world.as_ref().is_some_and(|w| w.server().is_some());
        let dw_is_server = self.dw().is_some_and(|x| x.is_server());
        let client_id = self.client_id();
        // 3.
        if dw_is_server && d.z_index == 1 && has_server && !h.player_is_admin_with_id(client_id.as_deref()) {
            h.update_player(client_id.as_deref());
        }
        // 4.
        let (index, name_for_log, was_regen) = {
            let mut f = self.bh.borrow_mut();
            f.state.happiness = (d.happiness as f32) / 127.0f32;
            f.state.health = (d.health as f32) / 127.0f32;
            f.state.heat = d.heat as f32;
            f.state.coffee_energy_timer = if d.has_coffee_energy != 0 { 1.0 } else { 0.0 };
            // 5.
            let mut index = f.current_net_queue_size;
            if index > 3 {
                f.current_net_queue_size = 3;
                index = 3;
            }
            f.paused = d.paused != 0;
            (index, f.name.clone(), f.state.regenerating != 0)
        };
        // 6.
        if !was_regen && d.regenerating != 0 && has_server {
            // Mods (not in the original): `death` of a player's blockhead (the client simulates it).
            if let Some(m) = crate::mods::hooks() {
                m.death(client_id.as_deref(), self.unique_id());
            }
            let pn = h.player_name_for_player_with_id_including_old_players(client_id.as_deref());
            nslog!(
                "Blockhead died named {} owned by player {}.",
                name_for_log.as_deref().unwrap_or("(null)"),
                pn.as_deref().unwrap_or("(null)")
            );
        }
        // 7.
        {
            let mut f = self.bh.borrow_mut();
            f.state.regenerating = d.regenerating;
            f.state.on_trade_mission = d.on_trade_mission;
            f.is_in_jet_pack_free_flight_mode = d.is_in_jet_pack_free_flight_mode != 0;
        }
        let jet = d.is_in_jet_pack_free_flight_mode != 0;
        let pos = self.pos();
        let wm = world.as_ref().map_or(0, |w| w.world_width_macro());
        // 8.
        if jet {
            let mut vx = (d.fishing_rod_cast_x as f32) / 10000.0f32;
            let mut vy = (d.fishing_rod_cast_y as f32) / 10000.0f32;
            if f32::from_bits(vx.to_bits() & 0x7fff_ffff) < 0.1f32 {
                vx = 0.0;
            }
            if f32::from_bits(vy.to_bits() & 0x7fff_ffff) < 0.1f32 {
                vy = 0.0;
            }
            let mut remote_x = d.to_square_x as f32;
            if (d.to_square_x as i32) < 0 && remote_x > ((wm.wrapping_shl(5)) as f32) {
                remote_x = (d.to_square_x as i32) as f32;
                nslog!("wrapped found");
            }
            let w = wm.wrapping_shl(5);
            let mut dx = remote_x - (pos.x as f32);
            if dx < ((w / 2) as f32) {
                dx = if (((-w) / 2) as f32) <= dx { dx } else { dx + (w as f32) };
            } else {
                dx -= w as f32;
            }
            let dy = (d.to_square_y as f32) - (pos.y as f32);
            let mut f = self.bh.borrow_mut();
            f.jet_pack_velocity = Vector2 { x: vx + dx * 0.5f32, y: vy + dy * 0.5f32 };
            f.state.animation_type = va::FLY_JETPACK;
        } else {
            // 9.
            let mut f = self.bh.borrow_mut();
            let to_sq = IntPair { x: d.to_square_x as i32, y: d.to_square_y as i32 };
            let int_sq = IntPair { x: d.interacting_square_x as i32, y: d.interacting_square_y as i32 };
            if !f.has_had_remote_update {
                f.to_square = to_sq;
                f.terrain_difficulty = d.terrain_difficulty as i32;
                f.traverse_type = d.traverse_type as i32;
                f.traverse_to_key_frame = d.traverse_to_key_frame as i32;
                f.state.interacting = d.interacting;
                f.state.interaction_type = d.interaction_type as u32;
                f.state.animation_type = d.animation_type as u32;
                f.state.sub_animation_type = d.sub_animation_type as u32;
                f.state.interacting_square = int_sq;
                f.has_had_remote_update = true;
            } else {
                f.current_net_queue_size += 1;
                let i = index as usize;
                f.net_next_to_square[i] = to_sq;
                f.next_terrain_difficulty[i] = d.terrain_difficulty as i32;
                f.next_traverse_type[i] = d.traverse_type as i32;
                f.next_traverse_to_key_frame[i] = d.traverse_to_key_frame as i32;
                f.next_interacting[i] = d.interacting != 0;
                f.next_interaction_type[i] = d.interaction_type as i32;
                f.next_animation_type[i] = d.animation_type as i32;
                f.next_sub_animation_type[i] = d.sub_animation_type as i32;
                f.next_interaction_square[i] = int_sq;
            }
        }
        // 10.
        {
            let mut f = self.bh.borrow_mut();
            f.remote_interaction_item_type = d.interaction_item_type as i32;
            f.is_currently_active_blockhead_according_to_net_data = d.is_currently_active != 0;
        }
        // 11. ride
        if d.ride_object_id == 0 {
            if self.bh.borrow().ride_object.is_some() {
                h.stop_riding(self);
            }
        } else {
            let cur = self.bh.borrow().ride_object.clone();
            let same = cur.as_ref().is_some_and(|o| o.unique_id() == d.ride_object_id);
            if !same {
                if cur.is_some() {
                    h.stop_riding(self);
                }
                let r = h.ridable_object_with_id(d.ride_object_id);
                self.bh.borrow_mut().ride_object = r.clone();
                if let Some(r) = r {
                    if let Some(rid) = r.as_ridable() {
                        rid.add_rider(&r, me);
                    }
                    let mut anim = h.ride_object_animation_type(&r);
                    if anim == 0 {
                        anim = if h.ride_object_rider_rides_with_arms_down(&r) { va::RIDE_ARMS_DOWN } else { va::RIDE };
                    }
                    let mut f = self.bh.borrow_mut();
                    f.state.animation_type = anim;
                    f.state.goal_interaction_type_while_walking = 0;
                    f.chasing_npc = false;
                    f.chasing_ride_object = false;
                }
            }
        }
        // 12. interaction object
        if d.interaction_object_id == 0 {
            self.bh.borrow_mut().interaction_object = None;
        } else {
            let cur = self.bh.borrow().interaction_object.as_ref().map_or(0, |o| o.unique_id());
            if cur != d.interaction_object_id {
                let o = h.interaction_object_with_id(d.interaction_object_id as i64);
                self.bh.borrow_mut().interaction_object = o;
            }
        }
        // 13. workbench
        if d.interaction_workbench_id == 0 {
            self.bh.borrow_mut().interaction_workbench = None;
        } else {
            let cur = self
                .bh
                .borrow()
                .interaction_workbench
                .as_deref()
                .map_or(0, <dyn crate::objects::base::DynamicObject>::unique_id);
            if cur != d.interaction_workbench_id {
                let o = h.interaction_object_with_id(d.interaction_workbench_id as i64);
                self.bh.borrow_mut().interaction_workbench = o;
            }
        }
        // 14. fishing
        let cast = d.fishing_rod_cast_x != 0 || d.fishing_rod_cast_y != 0;
        if !jet && cast {
            let have_rod = self.bh.borrow().fishing_rod.is_some();
            if !have_rod {
                if d.fishing_rod_fish_unique_id == 0 {
                    let rod = h.fishing_rod_new_with_swipe(
                        self,
                        Vector2 { x: d.fishing_rod_cast_x as f32, y: d.fishing_rod_cast_y as f32 },
                    );
                    let mut f = self.bh.borrow_mut();
                    f.fishing_rod = Some(rod);
                    if dw_is_server {
                        f.fishing_rod_cast_x = d.fishing_rod_cast_x as i16;
                        f.fishing_rod_cast_y = d.fishing_rod_cast_y as i16;
                    }
                } else {
                    self.bh.borrow_mut().remote_client_fishing_rod_fish_unique_id = d.fishing_rod_fish_unique_id;
                }
            } else if d.fishing_rod_fish_unique_id != 0 {
                if let Some(fish) = h.npc_with_id(d.fishing_rod_fish_unique_id) {
                    h.npc_remote_blockhead_hooked_you(&fish, self);
                }
                self.bh.borrow_mut().remote_client_fishing_rod_fish_unique_id = d.fishing_rod_fish_unique_id;
            }
        } else if self.bh.borrow().fishing_rod.is_some() {
            h.stop_fishing(self);
        }
    }

    /// `-remoteCreationDataUpdate:`.
    pub fn remote_creation_data_update_impl(&self, me: &ObjRef, net_data: &[u8]) {
        if self.is_net() {
            base_remote_creation_data_update(me, net_data);
            self.setup_from_net_creation_data(net_data);
        }
    }

    // ================================================================================================
    // predicates
    // ================================================================================================

    /// `-isIdle`.
    pub fn is_idle(&self) -> bool {
        let h = host();
        let (asleep, regen, sub, has_actions) = {
            let f = self.bh.borrow();
            (state_asleep(&f.state), f.state.regenerating != 0, f.state.sub_animation_type, !f.action_queue.is_empty())
        };
        !asleep && !regen && sub != vsa::MEDITATING && !h.current_interaction_requires_human_input(self) && !has_actions
    }

    /// `-onTradeMission`.
    pub fn on_trade_mission(&self) -> bool {
        self.bh.borrow().state.on_trade_mission != 0
    }

    /// `-shouldContinueSimulating`.
    pub fn should_continue_simulating(&self) -> bool {
        let (cancel, asleep, regen, has_actions, sub, trade) = {
            let f = self.bh.borrow();
            (
                f.cancel_simulate_due_to_collapse,
                state_asleep(&f.state),
                f.state.regenerating != 0,
                !f.action_queue.is_empty(),
                f.state.sub_animation_type,
                f.state.on_trade_mission != 0,
            )
        };
        if cancel {
            return false;
        }
        if asleep || regen {
            return true;
        }
        if !has_actions {
            return false;
        }
        if host().current_interaction_requires_human_input(self) {
            return false;
        }
        if sub == vsa::MEDITATING {
            return false;
        }
        if regen || trade {
            return false;
        }
        true
    }

    // ================================================================================================
    // prepareInventoryForSaving
    // ================================================================================================

    /// `-prepareInventoryForSaving`.
    /// the `if subInventoryChanged[i][j]` branch).
    pub fn prepare_inventory_for_saving(&self) {
        let h = host();
        for i in 0..8usize {
            let changed = self.bh.borrow().inventory_changed_this_frame_slots[i];
            if !changed {
                for j in 0..4usize {
                    let sc = self.bh.borrow().sub_inventory_changed_this_frame_slots[i][j];
                    if sc {
                        h.inventory_was_changed_sub_index_was_usage(self, i as i32, j as i32, false);
                        let mut f = self.bh.borrow_mut();
                        f.sub_inventory_changed_this_frame_slots[i][j] = false;
                        f.sub_inventory_usage_changed_this_frame_slots[i][j] = false;
                    }
                }
                let usage = self.bh.borrow().inventory_usage_changed_this_frame_slots[i];
                if !usage {
                    for j in 0..4usize {
                        let su = self.bh.borrow().sub_inventory_usage_changed_this_frame_slots[i][j];
                        if su {
                            h.inventory_was_changed_sub_index_was_usage(self, i as i32, j as i32, true);
                            self.bh.borrow_mut().sub_inventory_usage_changed_this_frame_slots[i][j] = false;
                        }
                    }
                } else {
                    {
                        let mut f = self.bh.borrow_mut();
                        f.inventory_usage_changed_this_frame_slots[i] = false;
                        f.sub_inventory_usage_changed_this_frame_slots[i] = [false; 4];
                    }
                    h.inventory_was_changed_sub_index_was_usage(self, i as i32, -1, true);
                }
            } else {
                {
                    let mut f = self.bh.borrow_mut();
                    f.inventory_changed_this_frame_slots[i] = false;
                    f.inventory_usage_changed_this_frame_slots[i] = false;
                    f.sub_inventory_changed_this_frame_slots[i] = [false; 4];
                    f.sub_inventory_usage_changed_this_frame_slots[i] = [false; 4];
                }
                h.inventory_was_changed_sub_index_was_usage(self, i as i32, -1, false);
            }
        }
    }

    // ================================================================================================
    // pickupItemForTile:astPos:
    // ================================================================================================

    /// `-pickupItemForTile:astPos:` (sic): builds the item for the world object at `p` if the inventory
    /// can take it and the object is removable. Checks run in the order of the spec; a failed check falls
    /// through. A nil tile dereferences NULL in the original (UB-PARITY: returns `None`).
    pub fn pickup_item_for_tile_ast_pos(&self, t: Option<&TileRef>, p: IntPair) -> Option<ItemRef> {
        let t = t?;
        let h = host();
        let me = self.me();
        let fg = t.foreground_contents();
        let contents = t.contents();
        let bg = t.background_contents();
        let can = |ty: u32| -> bool { h.can_pick_up_item_of_type_sub_items(self, ItemType(ty), None) == 1 };
        let removable =
            |o: &Option<ObjRef>| -> bool { o.as_ref().is_some_and(|o| o.can_be_removed_by_blockhead(o, &me)) };
        let save =
            |o: &Option<ObjRef>| -> Option<PlistValue> { o.as_ref().map(|o| PlistValue::Dict(o.get_save_dict(o))) };
        let mk = |ty: u32, a: u16, b: u16, sd: Option<PlistValue>| -> Option<ItemRef> {
            Some(InventoryItem::new(ItemType(ty), a, b, None, sd))
        };

        // torch group 1
        let torch1: &[(u8, u32)] = &[
            (b'1', 0x11),
            (b'a', 0xb7),
            (b'X', 0x9d),
            (b'2', 0x2f),
            (b'W', 0x96),
            (b'f', 0xfe),
            (b'i', 0x102),
            (b'R', 0x91),
            (b'S', 0x92),
            (b'T', 0x93),
            (b'U', 0x94),
            (b'V', 0x95),
        ];
        let torch_branch = |tbl: &[(u8, u32)]| -> Option<ItemRef> {
            for (c, ty) in tbl {
                if fg == *c && can(*ty) {
                    let torch = h.torch_at_pos(p);
                    if removable(&torch) {
                        if *c == b'X' {
                            let (a, b) = h.torch_data_a_b(torch.as_ref().unwrap());
                            return mk(*ty, a, b, None);
                        }
                        return mk(*ty, 0, 0, None);
                    }
                }
            }
            None
        };
        if let Some(i) = torch_branch(torch1) {
            return Some(i);
        }
        // egg
        if fg == b'N' {
            let egg = h.egg_at_pos(p);
            if let Some(e) = &egg {
                let breed = h.egg_breed(e);
                if h.can_pick_up_item_of_type_sub_items_data_a_data_b(self, ItemType(0x5b), None, 0, breed) == 1 {
                    return mk(0x5b, 0, breed, Some(PlistValue::Dict(e.get_save_dict(e))));
                }
            }
        }
        // painting / column
        if fg == b'c' {
            let o = h.painting_at_pos(p);
            if removable(&o) {
                let ty = h.object_item_type(o.as_ref().unwrap()) as u32;
                if can(ty) {
                    return mk(ty, 0, 0, save(&o));
                }
            }
        }
        if fg == b'd' {
            let o = h.column_at_pos(p);
            if removable(&o) {
                let ty = h.object_item_type(o.as_ref().unwrap()) as u32;
                if can(ty) {
                    return mk(ty, 0, 0, save(&o));
                }
            }
        }
        // torch group 2
        let torch2: &[(u8, u32)] = &[(b'3', 0x4b), (b'5', 0x4c), (b'7', 0x56), (b'9', 0x57), (b';', 0x58)];
        if let Some(i) = torch_branch(torch2) {
            return Some(i);
        }
        // ladder
        if fg == b'B' {
            let o = h.ladder_at_pos(p);
            if o.is_some() && removable(&o) && can(0x35) {
                return mk(0x35, 0, 0, save(&o));
            }
        }
        // wire
        if (fg == b'`' || contents == b'`') && can(0xb2) {
            let o = h.wire_at_pos(p);
            if removable(&o) {
                return mk(0xb2, 0, 0, None);
            }
        }
        // elevator motor
        if contents == b'h' && can(0x440) {
            let o = h.elevator_motor_at_pos(p);
            if removable(&o) {
                return mk(0x440, 0, 0, None);
            }
        }
        // elevator shaft
        if contents == b'g' && can(0x43f) {
            let o = h.elevator_shaft_at_pos(p);
            if removable(&o) {
                return mk(0x43f, 0, 0, save(&o));
            }
        }
        // stairs
        if contents == b'e' {
            let o = h.stairs_at_pos(p);
            if o.is_some() && removable(&o) {
                let ty = h.object_item_type(o.as_ref().unwrap()) as u32;
                if can(ty) {
                    return mk(ty, 0, 0, save(&o));
                }
            }
        }
        // door
        if contents == 0x92 {
            let o = h.door_at_pos(p);
            if o.is_some() && removable(&o) {
                let ty = h.object_item_type(o.as_ref().unwrap()) as u32;
                if can(ty) {
                    return mk(ty, 0, 0, save(&o));
                }
            }
        }
        // windows
        if bg == b'E' && can(0x3a) {
            let o = h.window_at_pos(p);
            if o.is_some() && removable(&o) && can(0x3a) {
                return mk(0x3a, 0, 0, save(&o));
            }
        }
        if bg == b'_' {
            let o = h.window_at_pos(p);
            if removable(&o) && can(0xae) {
                return mk(0xae, 0, 0, save(&o));
            }
        }
        None
    }

    // ================================================================================================
    // pickUpItemIfPossibleInTile:atPos:
    // ================================================================================================

    /// `-pickUpItemIfPossibleInTile:atPos:`.
    pub fn pick_up_item_if_possible_in_tile_at_pos(&self, t: Option<&TileRef>, p: IntPair) {
        let h = host();
        let me = self.me();
        // 1. `[world worldUIDragging]`: no counterpart on the server (NO)
        // 2.
        let mut rejected = false;
        let (extra, item) = {
            let f = self.bh.borrow();
            (self.craft_count_or_extra_data(), f.interaction_item.clone())
        };
        // 3.
        let tr = t.copied();
        let mut ty = h.interaction_type_for_tile_at_pos_item_pickup_rejected_due_to_inventory_full_include_actions_face_index_allow_protected_actions(
            self, tr, p, item.as_ref(), &mut rejected, false, extra, true,
        );
        // 4.
        let world = self.world_rc();
        if interaction_type_disallowed_on_protected_tiles(crate::types::enums::InteractionType(ty))
            && world.as_ref().is_some_and(|w| WorldApi::tile_is_protected_at_pos_against_blockhead(&**w, p, &me))
        {
            ty = h.interaction_type_for_tile_at_pos_item_pickup_rejected_due_to_inventory_full_include_actions_face_index_allow_protected_actions(
                self, tr, p, item.as_ref(), &mut rejected, false, extra, false,
            );
        }
        let max_sound_play = 8;
        let _ = max_sound_play;
        // 6. PICKUP_FREEBLOCK
        if ty == it::PICKUP_FREEBLOCK {
            let mut fbs = h.free_blocks_at_pos(p);
            // comparator: Ascending if obj2.uniqueID < obj1.uniqueID else Descending -> descending by uniqueID
            fbs.sort_by_key(|b| std::cmp::Reverse(b.unique_id()));
            let mut max_sound_play = 8;
            let mut delay = 0.0f32;
            for fb in fbs.iter().rev() {
                if h.pickup_freeblock_if_possible_in_tile_intentional(self, fb, tr, true) && max_sound_play > 0 {
                    // littlePop.wav / pickupGem.wav: CLIENT-ONLY
                    delay += 0.05f32;
                    max_sound_play -= 1;
                }
            }
            let _ = delay;
        }
        // 7. PICKUP
        if ty == it::PICKUP {
            let item = self.pickup_item_for_tile_ast_pos(t, p);
            let item_type: u16 = item.as_ref().map_or(0, |i| i.borrow().item_type.0 as u16);
            {
                let mut f = self.bh.borrow_mut();
                if let Some(pos) = f.expected_craft_items.iter().position(|x| *x == item_type) {
                    f.expected_craft_items.remove(pos);
                }
            }
            let Some(item) = item else { return };
            let Some(t) = t else { return };
            let bg = t.background_contents();
            let ity = item.borrow().item_type.0;
            // window
            if (ity == 0x3a && bg == b'E') || (ity == 0xae && bg == b'_') {
                if let Some(w) = h.window_at_pos(p) {
                    h.pickup_dynamic_object(self, &w);
                }
                return;
            }
            // gate
            if ity == 0x130 && t.contents() == 0x92 {
                let door = h.door_at_pos(p);
                // `![dynamicWorld isClient]` is YES on the server
                if let Some(d) = &door {
                    let dpos = d.pos();
                    let sd = d.free_block_creation_save_dict(d).map(PlistValue::Dict);
                    h.create_free_block_at_position_of_type_data_a_data_b_sub_items_dynamic_object_save_dict_hovers_play_sound_priority_blockhead(
                        dpos,
                        ItemType(d.freeblock_creation_item_type() as u32),
                        0,
                        0,
                        None,
                        sd.as_ref(),
                        false,
                        1,
                        Some(&me),
                    );
                }
                h.remove_object_at_pos(Remover::Door, p);
                t.set_contents(0);
                // littlePop: CLIENT-ONLY
                return;
            }
            // otherwise
            let is_net = self.is_net();
            let ctl = self.is_client_blockhead_being_controlled_by_server();
            if !is_net && !ctl {
                let ach = match ity {
                    0x4b => Some("grp.ruby"),
                    0x4c => Some("grp.emerald"),
                    0x56 => Some("grp.sapphire"),
                    0x57 => Some("grp.amethyst"),
                    0x58 => Some("grp.diamond"),
                    0xa0 => Some("grp.lime"),
                    _ => None,
                };
                if let Some(a) = ach {
                    h.report_achievement_with_identifier(a);
                }
                if ity == 0x58 && matches!(t.contents(), 0x79..=0x7b) {
                    h.report_achievement_with_identifier("grp.diamond_tree");
                }
            }
            h.add_item_to_inventory_flash(self, &item, true);
            let ityp = ItemType(ity);
            let fg = t.foreground_contents();
            let contents = t.contents();
            // (remover, clears foreground?)
            let torch_fgs = *b"1a2WfiUTSRV3579;X";
            let row: Option<(Remover, bool)> = if item_type_is_torch(ityp) && torch_fgs.contains(&fg) {
                Some((Remover::Torch, true))
            } else if ity == 0x5b && fg == b'N' {
                Some((Remover::Egg, true))
            } else if item_type_is_painting(ityp) && fg == b'c' {
                Some((Remover::Painting, true))
            } else if ity == 0x35 && fg == b'B' {
                Some((Remover::Ladder, true))
            } else if item_type_is_column(ityp) && fg == b'd' {
                Some((Remover::Column, true))
            } else if ity == 0xb2 && fg == b'`' {
                Some((Remover::Wire, true))
            } else if ity == 0xb2 && contents == b'`' {
                Some((Remover::Wire, false))
            } else if item_type_is_stairs(ityp) && contents == b'e' {
                Some((Remover::Stairs, false))
            } else if ity == 0x43f && contents == b'g' {
                Some((Remover::ElevatorShaft, false))
            } else if ity == 0x440 && contents == b'h' {
                Some((Remover::ElevatorMotor, false))
            } else {
                None
            };
            if let Some((rem, clear_fg)) = row {
                let obj = h.remove_object_at_pos(rem, p);
                if clear_fg {
                    t.set_foreground_contents(0);
                } else {
                    t.set_contents(0);
                }
                // registration (nil-safe on the server: both containers are None)
                let key: u64 = obj.as_ref().map_or(0, |o| o.unique_id());
                let mut f = self.bh.borrow_mut();
                let absent = f.unconfirmed_pickups.as_ref().is_none_or(|m| !m.contains_key(&key));
                if absent {
                    if let Some(s) = f.this_frame_pickup_requests.as_mut() {
                        s.add_index(key);
                    }
                    if let Some(m) = f.unconfirmed_pickups.as_mut() {
                        m.insert(key, item.clone());
                    }
                }
            }
            // gem pickup sound / littlePop: CLIENT-ONLY
        }
    }

    // ================================================================================================
    // sufferDamage
    // ================================================================================================

    /// `-sufferDamage:isSimulation:recoil:`.
    pub fn suffer_damage_is_simulation_recoil(&self, damage: f32, sim: bool, recoil: bool) {
        let h = host();
        let me = self.me();
        let mut damage = damage;
        // 1.
        {
            let f = self.bh.borrow();
            if damage < 0.0 || (f.paused && !sim) || f.state.regenerating != 0 || f.state.on_trade_mission != 0 {
                return;
            }
        }
        // 2.
        let ctl = self.is_client_blockhead_being_controlled_by_server();
        if (sim || ctl) && !self.should_continue_simulating() {
            return;
        }
        // Mods (not in the original; `None` without mods): `damage` may cancel or change the amount.
        if let Some(h) = crate::mods::hooks() {
            let attacker = crate::mods::actor();
            match h.damage(self.client_id().as_deref(), self.unique_id(), damage, attacker.as_deref()) {
                Some(d) => damage = d,
                None => return,
            }
        }
        if self.is_net() {
            // 3.
            h.send_damage_for_net_blockhead_recoil(damage, &me, recoil);
            // womanOuch.wav / manOuch.wav: CLIENT-ONLY
        } else {
            // 4.
            let asleep_or_meditating = {
                let mut f = self.bh.borrow_mut();
                f.tip_type = if f.state.death <= 0.001f32 { tip::SUFFER_DAMAGE } else { tip::DYING };
                state_asleep(&f.state) || f.state.sub_animation_type == vsa::MEDITATING
            };
            if asleep_or_meditating {
                self.stop_interacting();
                self.bh.borrow_mut().path_needs_recalculated = true;
            }
            h.increment_damage_of_armor_clothing(self, damage);
            let (shirt, hat, pants, shoes) = {
                let f = self.bh.borrow();
                (f.shirt_item_type, f.hat_item_type, f.pants_item_type, f.shoes_item_type)
            };
            let mut m = 1.0f32;
            match shirt {
                0xbc => m *= 0.75f32,
                0xc0 | 0xc4 => m *= 0.5f32,
                0x120 | 0x116 | 0x124 => m *= 0.4f32,
                _ => {}
            }
            match hat {
                0xbd => m *= 0.8f32,
                0xc1 | 0xc5 => m *= 0.6f32,
                0x121 | 0x125 => m *= 0.5f32,
                _ => {}
            }
            match pants {
                0xbb => m *= 0.8f32,
                0xbf | 0xc3 => m *= 0.6f32,
                0x11f | 0x123 => m *= 0.5f32,
                _ => {}
            }
            match shoes {
                0xbe => m *= 0.9f32,
                0xc2 | 0xc6 => m *= 0.8f32,
                0x122 | 0x126 => m *= 0.7f32,
                _ => {}
            }
            m = if m <= 0.1f32 { 0.1f32 } else { m };
            damage *= m;
            let world = self.world_rc();
            let rules = world.as_ref().map(|w| w.custom_rules()).unwrap_or_default();
            if rules.custom_rules_enabled != 0 {
                match rules.health {
                    4 => damage = 0.0,
                    3 => damage *= 0.1f32,
                    1 => damage *= 10.0f32,
                    0 => damage = 99999.9f32,
                    _ => {}
                }
            }
            if world.as_ref().is_some_and(|w| w.expert_mode()) {
                damage *= 2.0f32;
            }
            let mut die_now = false;
            {
                let mut f = self.bh.borrow_mut();
                if f.state.health - damage < 0.0 {
                    damage -= f.state.health;
                    f.state.health = 0.0;
                    if rules.custom_rules_enabled != 0 && rules.death == 0 {
                        // skip the death accumulation
                    } else {
                        f.state.death += damage;
                        if f.state.death >= 1.0f32 {
                            die_now = true;
                        }
                    }
                } else {
                    f.state.health -= damage;
                }
            }
            if die_now {
                self.die();
            }
            // ouch sound: CLIENT-ONLY
            self.bh.borrow_mut().needs_harm_flash = true;
        }
        // 6.
        if recoil {
            let r = r01_from(rand_tagged("blockhead/suffer_damage/recoil"));
            let mut f = self.bh.borrow_mut();
            f.recoil_timer = 0.5f32;
            f.recoil_direction = r;
        }
    }
}

#[cfg(test)]
mod cov_tests;

#[cfg(test)]
mod tests {
    use super::super::blockhead::testutil::*;
    use super::*;
    use crate::rng::glibc_rand::{r01_from, rand, srand};
    use crate::types::structs::CustomRules;
    use std::rc::Rc;

    fn mk() -> (Rc<World>, Rc<MockDw>, crate::objects::base::DynamicWorldRef, ObjRef, Rc<TestHost>) {
        let h = TestHost::install();
        let (world, dw, dwref) = setup();
        let o = new_bh(&world, &dwref, 0);
        (world, dw, dwref, o, h)
    }

    /// : every conjunct flipped alone.
    #[test]
    fn can_collapse_truth_table() {
        let (world, _dw, _dwref, o, _h) = mk();
        let b = bh_of(&o);
        b.bh.borrow_mut().state.energy = 0.2;
        assert!(b.can_collapse());
        // the raw energy test: (double)energy <= 0.251 or fullness <= -0.249
        b.bh.borrow_mut().state.energy = 0.2511;
        assert!(!b.can_collapse()); // (double)0.2511f > 0.251
        b.bh.borrow_mut().state.energy = 1.0;
        b.bh.borrow_mut().state.fullness = -0.2489;
        assert!(!b.can_collapse());
        b.bh.borrow_mut().state.fullness = -0.249;
        assert!(b.can_collapse()); // -0.249f32 <= -0.249f32 holds
        b.bh.borrow_mut().state.fullness = -0.25;
        assert!(b.can_collapse());
        b.bh.borrow_mut().state.fullness = 1.0;
        b.bh.borrow_mut().state.energy = 0.2;
        let flips: Vec<(&str, Box<dyn Fn(&Blockhead)>)> = vec![
            ("sub5", Box::new(|b| b.bh.borrow_mut().state.sub_animation_type = 5)),
            ("sub6", Box::new(|b| b.bh.borrow_mut().state.sub_animation_type = 6)),
            ("sub7", Box::new(|b| b.bh.borrow_mut().state.sub_animation_type = 7)),
            ("anim5", Box::new(|b| b.bh.borrow_mut().state.animation_type = 5)),
            (
                "interacting2",
                Box::new(|b| {
                    b.bh.borrow_mut().state.interacting = 1;
                    b.bh.borrow_mut().state.interaction_type = 2;
                }),
            ),
            ("ride", Box::new(|b| b.bh.borrow_mut().ride_object = Some(stub(27, true)))),
            ("waiting", Box::new(|b| b.bh.borrow_mut().waiting_for_path = true)),
            ("underwater", Box::new(|b| b.bh.borrow_mut().underwater = true)),
            ("rod", Box::new(|b| b.bh.borrow_mut().fishing_rod = Some(Rc::new(())))),
            ("regen", Box::new(|b| b.bh.borrow_mut().state.regenerating = 1)),
            ("trade", Box::new(|b| b.bh.borrow_mut().state.on_trade_mission = 1)),
        ];
        for (name, flip) in flips {
            let saved = (b.st(), b.bh.borrow().ride_object.clone());
            flip(b);
            assert!(!b.can_collapse(), "{name}");
            let mut f = b.bh.borrow_mut();
            f.state = saved.0;
            f.ride_object = saved.1;
            f.waiting_for_path = false;
            f.underwater = false;
            f.fishing_rod = None;
        }
        // interacting with DIG (1) is allowed
        {
            let mut f = b.bh.borrow_mut();
            f.state.interacting = 1;
            f.state.interaction_type = 1;
        }
        assert!(b.can_collapse());
        b.bh.borrow_mut().state.interacting = 0;
        // the tile must not be water, the tile below must be solid
        let here = tile(&world, 5, 8);
        here.set_type_index(3);
        assert!(!b.can_collapse());
        here.set_type_index(2);
        let below = tile(&world, 5, 7);
        below.set_type_index(2);
        assert!(!b.can_collapse());
        below.set_type_index(1);
        assert!(b.can_collapse());
    }

    #[test]
    fn can_sleep_meditate_eat() {
        let (world, _dw, _dwref, o, _h) = mk();
        let b = bh_of(&o);
        assert!(!b.can_sleep_on_spot()); // energy 1.2 >= 0.75
        b.bh.borrow_mut().state.energy = 0.5;
        assert!(b.can_sleep_on_spot());
        assert!(b.can_meditate()); // no energy test
        b.bh.borrow_mut().state.energy = 1.2;
        assert!(b.can_meditate());
        b.bh.borrow_mut().state.energy = 0.5;
        let flips: Vec<(&str, bool, Box<dyn Fn(&Blockhead)>)> = vec![
            ("anim5", true, Box::new(|b| b.bh.borrow_mut().state.animation_type = 5)),
            ("anim32ok", false, Box::new(|b| b.bh.borrow_mut().state.animation_type = 32)),
            ("path", true, Box::new(|b| b.bh.borrow_mut().path = Some(vec![PlistValue::int(1)]))),
            ("interacting", true, Box::new(|b| b.bh.borrow_mut().state.interacting = 1)),
            ("ride", true, Box::new(|b| b.bh.borrow_mut().ride_object = Some(stub(27, true)))),
            ("waiting", true, Box::new(|b| b.bh.borrow_mut().waiting_for_path = true)),
            ("underwater", true, Box::new(|b| b.bh.borrow_mut().underwater = true)),
            ("rod", true, Box::new(|b| b.bh.borrow_mut().fishing_rod = Some(Rc::new(())))),
            ("regen", true, Box::new(|b| b.bh.borrow_mut().state.regenerating = 1)),
            ("trade", true, Box::new(|b| b.bh.borrow_mut().state.on_trade_mission = 1)),
            ("sub5", true, Box::new(|b| b.bh.borrow_mut().state.sub_animation_type = 5)),
            ("sub7", true, Box::new(|b| b.bh.borrow_mut().state.sub_animation_type = 7)),
            ("space", true, Box::new(|b| b.bh.borrow_mut().drowning_in_space = true)),
            ("actions", true, Box::new(|b| b.bh.borrow_mut().action_queue.push(Rc::new(())))),
        ];
        for (name, blocks, flip) in flips {
            let saved = b.st();
            flip(b);
            assert_eq!(!b.can_sleep_on_spot(), blocks, "sleep {name}");
            assert_eq!(!b.can_meditate(), blocks, "meditate {name}");
            let mut f = b.bh.borrow_mut();
            f.state = saved;
            f.path = None;
            f.ride_object = None;
            f.waiting_for_path = false;
            f.underwater = false;
            f.fishing_rod = None;
            f.drowning_in_space = false;
            f.action_queue.clear();
        }
        // sub 6 blocks both, sub 7 blocks meditate only through the shared list
        b.bh.borrow_mut().state.sub_animation_type = 6;
        assert!(!b.can_sleep_on_spot() && !b.can_meditate());
        b.bh.borrow_mut().state.sub_animation_type = 0;
        let here = tile(&world, 5, 8);
        here.set_type_index(3);
        assert!(!b.can_sleep_on_spot());
        here.set_type_index(2);
        // canEat
        let mut f = b.bh.borrow_mut();
        f.state.fullness = 1.0;
        f.state.health = 1.0;
        drop(f);
        assert!(!b.can_eat());
        b.bh.borrow_mut().state.fullness = 0.9998;
        assert!(b.can_eat());
        b.bh.borrow_mut().state.on_trade_mission = 1;
        assert!(!b.can_eat());
        b.bh.borrow_mut().state.on_trade_mission = 0;
        b.bh.borrow_mut().state.regenerating = 1;
        assert!(!b.can_eat());
    }

    /// : die with expertMode x rules.
    #[test]
    fn die_drop_rules_and_rng() {
        // default: drop some -> 6 draws, three drops
        let (world, _dw, _dwref, o, h) = mk();
        let b = bh_of(&o);
        b.bh.borrow_mut().action_queue.push(Rc::new(()));
        b.bh.borrow_mut().travel_fraction = 0.7;
        srand(2);
        b.die();
        assert_eq!(rand(), {
            srand(2);
            for _ in 0..6 {
                rand();
            }
            rand()
        });
        srand(2);
        let mut expect = Vec::new();
        for _ in 0..3 {
            let r1 = r01_from(rand());
            let idx = (((r1 as f64) * 7.99) as i32).clamp(0, 7);
            let r2 = r01_from(rand());
            let mut sub = (((r2 as f64) * 4.99) as i32).clamp(0, 4);
            if sub == 4 {
                sub = -1;
            }
            expect.push(format!("drop:{}:{}:-1:true", idx, sub));
        }
        let log = h.take();
        let drops: Vec<String> = log.iter().filter(|l| l.starts_with("drop:")).cloned().collect();
        assert_eq!(drops, expect);
        assert!(log.contains(&"abortpath".to_string()) && log.contains(&"stopriding".to_string()));
        {
            let f = b.bh.borrow();
            assert_eq!(f.state.death, 1.0);
            assert_eq!(f.state.regenerating, 1);
            assert_eq!(f.state.regeneration_progress, 0.0);
            assert_eq!(f.tip_type, 8);
            assert!(f.action_queue.is_empty());
            assert_eq!(f.travel_fraction, 0.0);
            assert_eq!(f.from_square, b.pos());
        }
        assert!(o.update_needs_to_be_sent());
        // already regenerating: only death = 1, no drops
        b.die();
        assert!(h.take().is_empty());
        // expert: drop all, no rand
        b.bh.borrow_mut().state.regenerating = 0;
        world.set_expert_mode(true);
        srand(5);
        b.die();
        assert_eq!(rand(), {
            srand(5);
            rand()
        });
        let log = h.take();
        let drops: Vec<&String> = log.iter().filter(|l| l.starts_with("drop:")).collect();
        assert_eq!(drops.len(), 4 + 7);
        assert_eq!(drops[0], "drop:0:0:-1:true");
        assert_eq!(drops[3], "drop:0:3:-1:true");
        assert_eq!(drops[4], "drop:1:-1:-1:true");
        assert_eq!(drops[10], "drop:7:-1:-1:true");
        // custom rules: death 3 -> die for good; deathDrops 2 -> no drops
        b.bh.borrow_mut().state.regenerating = 0;
        let mut r = CustomRules::default();
        r.custom_rules_enabled = 1;
        r.death = 3;
        r.death_drops = 2;
        world.set_custom_rules(r);
        b.die();
        let log = h.take();
        assert!(log.iter().all(|l| !l.starts_with("drop:")));
        assert!(log.contains(&"dieforgood".to_string()));
        assert_ne!(b.bh.borrow().tip_type, 0);
        // death 1 with deathDrops 1 -> drop some; deathDrops 0 -> drop all
        for (dd, n) in [(1i8, 3usize), (0, 11)] {
            b.bh.borrow_mut().state.regenerating = 0;
            let mut r = CustomRules::default();
            r.custom_rules_enabled = 1;
            r.death = 1;
            r.death_drops = dd;
            world.set_custom_rules(r);
            b.die();
            let log = h.take();
            assert_eq!(log.iter().filter(|l| l.starts_with("drop:")).count(), n, "dd {dd}");
            assert!(!log.contains(&"dieforgood".to_string()));
        }
    }

    #[test]
    fn suffer_damage_rules() {
        let (world, _dw, _dwref, o, h) = mk();
        let b = bh_of(&o);
        // early returns
        b.suffer_damage_is_simulation_recoil(-0.1, false, true);
        b.bh.borrow_mut().paused = true;
        b.suffer_damage_is_simulation_recoil(0.1, false, true);
        b.bh.borrow_mut().paused = false;
        b.bh.borrow_mut().state.regenerating = 1;
        b.suffer_damage_is_simulation_recoil(0.1, false, true);
        b.bh.borrow_mut().state.regenerating = 0;
        b.bh.borrow_mut().state.on_trade_mission = 1;
        b.suffer_damage_is_simulation_recoil(0.1, false, true);
        b.bh.borrow_mut().state.on_trade_mission = 0;
        assert_eq!(b.health(), 1.0);
        assert!(h.take().is_empty());
        // ctl && !shouldContinueSimulating returns
        b.bh.borrow_mut().is_client_blockhead_being_controlled_by_server = true;
        b.suffer_damage_is_simulation_recoil(0.1, false, true);
        assert_eq!(b.health(), 1.0);
        b.bh.borrow_mut().is_client_blockhead_being_controlled_by_server = false;
        // plain damage with recoil: exactly one rand()
        srand(11);
        b.suffer_damage_is_simulation_recoil(0.25, false, true);
        assert_eq!(b.health(), 0.75);
        assert_eq!(b.bh.borrow().recoil_timer, 0.5);
        srand(11);
        assert_eq!(b.bh.borrow().recoil_direction, r01_from(rand()));
        assert_eq!(rand(), {
            srand(11);
            rand();
            rand()
        });
        assert_eq!(b.bh.borrow().tip_type, 7);
        assert!(b.bh.borrow().needs_harm_flash);
        assert_eq!(h.take(), vec!["armor:0.25".to_string()]);
        // armour multiplier table
        let table: [(u16, u16, u16, u16, f32); 6] = [
            (0xbc, 0, 0, 0, 0.75),
            (0xc0, 0xc1, 0, 0, 0.5 * 0.6),
            (0x120, 0x121, 0x11f, 0x122, 0.4 * 0.5 * 0.5 * 0.7),
            (0, 0xbd, 0xbb, 0xbe, 0.8 * 0.8 * 0.9),
            (0x124, 0x125, 0x123, 0x126, 0.4 * 0.5 * 0.5 * 0.7),
            (0x116, 0xc5, 0xc3, 0xc2, 0.4 * 0.6 * 0.6 * 0.8),
        ];
        for (shirt, hat, pants, shoes, _m) in table {
            b.bh.borrow_mut().state.health = 1.0;
            {
                let mut f = b.bh.borrow_mut();
                f.shirt_item_type = shirt;
                f.hat_item_type = hat;
                f.pants_item_type = pants;
                f.shoes_item_type = shoes;
            }
            let mut m = 1.0f32;
            m *= match shirt {
                0xbc => 0.75f32,
                0xc0 | 0xc4 => 0.5,
                0x120 | 0x116 | 0x124 => 0.4,
                _ => 1.0,
            };
            m *= match hat {
                0xbd => 0.8f32,
                0xc1 | 0xc5 => 0.6,
                0x121 | 0x125 => 0.5,
                _ => 1.0,
            };
            m *= match pants {
                0xbb => 0.8f32,
                0xbf | 0xc3 => 0.6,
                0x11f | 0x123 => 0.5,
                _ => 1.0,
            };
            m *= match shoes {
                0xbe => 0.9f32,
                0xc2 | 0xc6 => 0.8,
                0x122 | 0x126 => 0.7,
                _ => 1.0,
            };
            let m = if m <= 0.1f32 { 0.1f32 } else { m };
            b.suffer_damage_is_simulation_recoil(0.5, false, false);
            assert_eq!(b.health(), 1.0 - 0.5 * m);
        }
        // floor 0.1 on the multiplier
        {
            let mut f = b.bh.borrow_mut();
            f.state.health = 1.0;
            f.shirt_item_type = 0x116;
            f.hat_item_type = 0x121;
            f.pants_item_type = 0x11f;
            f.shoes_item_type = 0x122;
        }
        b.suffer_damage_is_simulation_recoil(1.0, false, false);
        let m = 0.4f32 * 0.5 * 0.5 * 0.7;
        assert_eq!(b.health(), 1.0 - 1.0 * (if m <= 0.1 { 0.1 } else { m }));
        // expert doubles
        {
            let mut f = b.bh.borrow_mut();
            f.state.health = 1.0;
            f.shirt_item_type = 0;
            f.hat_item_type = 0;
            f.pants_item_type = 0;
            f.shoes_item_type = 0;
        }
        world.set_expert_mode(true);
        b.suffer_damage_is_simulation_recoil(0.1, false, false);
        assert_eq!(b.health(), 1.0 - 0.2f32);
        world.set_expert_mode(false);
        // rules.health
        let set_rules = |health: i8, death: i8| {
            let mut r = CustomRules::default();
            r.custom_rules_enabled = 1;
            r.health = health;
            r.death = death;
            world.set_custom_rules(r);
        };
        b.bh.borrow_mut().state.health = 1.0;
        set_rules(4, 1);
        b.suffer_damage_is_simulation_recoil(0.5, false, false);
        assert_eq!(b.health(), 1.0);
        set_rules(3, 1);
        b.suffer_damage_is_simulation_recoil(0.5, false, false);
        assert_eq!(b.health(), 1.0 - 0.5 * 0.1f32);
        b.bh.borrow_mut().state.health = 1.0;
        set_rules(1, 1);
        b.suffer_damage_is_simulation_recoil(0.05, false, false);
        assert_eq!(b.health(), 1.0 - 0.05f32 * 10.0);
        // health - damage < 0 and rules.death == 0: health 0, death untouched
        b.bh.borrow_mut().state.health = 0.1;
        b.bh.borrow_mut().state.death = 0.0;
        set_rules(1, 0);
        b.suffer_damage_is_simulation_recoil(0.5, false, false);
        assert_eq!(b.health(), 0.0);
        assert_eq!(b.death(), 0.0);
        assert_eq!(b.st().regenerating, 0);
        // rules.health 0 kills: damage 99999.9 -> death accumulates >= 1 -> die()
        b.bh.borrow_mut().state.health = 1.0;
        set_rules(0, 1);
        h.take();
        b.suffer_damage_is_simulation_recoil(0.01, false, false);
        assert_eq!(b.st().regenerating, 1);
        assert_eq!(b.health(), 0.0);
        // net replica: sends 0x2a data via the world, then recoil only
        world.set_custom_rules(CustomRules::default());
        b.bh.borrow_mut().state.regenerating = 0;
        b.bh.borrow_mut().state.health = 1.0;
        b.base.borrow_mut().is_net = true;
        h.take();
        b.suffer_damage_is_simulation_recoil(0.3, false, true);
        assert_eq!(h.take(), vec!["netdamage:0.3:true".to_string()]);
        assert_eq!(b.health(), 1.0);
    }

    #[test]
    fn free_block_creation_counts() {
        let (world, _dw, _dwref, o, _h) = mk();
        let b = bh_of(&o);
        let item = |t: u32| InventoryItem::new(ItemType(t), 0, 0, None, None);
        let t = tile(&world, 9, 20);
        let set = |ty: u8, contents: u8, fg: u8| {
            t.set_type_index(ty);
            t.set_contents(contents);
            t.set_foreground_contents(fg);
        };
        let tr = Some(&t);
        let d = |c: FreeBlockCreationCount| (c.count, c.bonus_multiplier);
        assert_eq!(d(b.free_block_creation_count_for_tile_with_item(None, None)), (1, 1));
        // clay / flint
        set(6, 2, 0);
        assert_eq!(d(b.free_block_creation_count_for_tile_with_item(tr, None)), (1, 1));
        for sp in [0x10, 0x40, 0x28, 0x59] {
            assert_eq!(d(b.free_block_creation_count_for_tile_with_item(tr, Some(&item(sp)))).0, 3);
        }
        assert_eq!(d(b.free_block_creation_count_for_tile_with_item(tr, Some(&item(8)))).0, 1);
        // tree trunk (contents 0x07), axes
        set(2, 0x07, 0);
        assert_eq!(d(b.free_block_creation_count_for_tile_with_item(tr, None)), (1, 1));
        assert_eq!(b.free_block_creation_count_for_tile_with_item(tr, Some(&item(6))).count, 3);
        assert_eq!(b.free_block_creation_count_for_tile_with_item(tr, Some(&item(0x46))).count, 3);
        assert_eq!(b.free_block_creation_count_for_tile_with_item(tr, Some(&item(8))).count, 1);
        // ore with C1, pickaxes
        set(1, 0x3d, 0);
        for pk in [8, 0x22, 0x2b, 0x5a, 0x42, 0x5d, 0x11d] {
            assert_eq!(d(b.free_block_creation_count_for_tile_with_item(tr, Some(&item(pk)))), (3, 1), "pk {pk:#x}");
        }
        let c = b.free_block_creation_count_for_tile_with_item(tr, Some(&item(0x5f)));
        assert_eq!((c.count, c.bonus_multiplier, c.bonus_color.v), (9, 3, [0.0, 0.0, 1.0, 1.0]));
        let c = b.free_block_creation_count_for_tile_with_item(tr, Some(&item(0x62)));
        assert_eq!((c.count, c.bonus_multiplier, c.bonus_color.v), (18, 6, [0.25, 1.0, 1.0, 1.0]));
        assert_eq!(d(b.free_block_creation_count_for_tile_with_item(tr, Some(&item(0xaf)))), (1, 1));
        // iron with the magnet: 5, expert 3
        set(1, 0x3f, 0);
        assert_eq!(b.free_block_creation_count_for_tile_with_item(tr, Some(&item(0xaf))).count, 5);
        world.set_expert_mode(true);
        assert_eq!(b.free_block_creation_count_for_tile_with_item(tr, Some(&item(0xaf))).count, 3);
        assert_eq!(b.free_block_creation_count_for_tile_with_item(tr, Some(&item(8))).count, 2);
        world.set_expert_mode(false);
        // not C1: minable rock without ore gets only the gem multipliers
        set(1, 0, 0);
        assert_eq!(d(b.free_block_creation_count_for_tile_with_item(tr, Some(&item(8)))), (1, 1));
        let c = b.free_block_creation_count_for_tile_with_item(tr, Some(&item(0x5e)));
        assert_eq!((c.count, c.bonus_multiplier, c.bonus_color.v), (2, 2, [0.53, 0.45, 1.0, 1.0]));
        set(2, 0, 0); // air is not minable
        assert_eq!(d(b.free_block_creation_count_for_tile_with_item(tr, Some(&item(0x5e)))), (1, 1));
        // time crystal tile type 0x10 counts as C1
        set(0x10, 0, 0);
        assert_eq!(b.free_block_creation_count_for_tile_with_item(tr, Some(&item(8))).count, 3);
        // bush: machetes, spades on plants
        set(2, 0x03, 0);
        assert_eq!(d(b.free_block_creation_count_for_tile_with_item(tr, Some(&item(0x19)))), (3, 1));
        assert_eq!(d(b.free_block_creation_count_for_tile_with_item(tr, Some(&item(0x10)))), (1, 1)); // not a plant
        set(2, 0, 0x43); // plant foreground
        assert_eq!(b.free_block_creation_count_for_tile_with_item(tr, Some(&item(0x10))).count, 3);
        // cactus: one rand(), count kept only for r >= 0.7
        set(2, 0x2b, 0);
        let mut kept = 0;
        for seed in 1..40u32 {
            srand(seed);
            let r = r01_from(rand());
            srand(seed);
            let c = b.free_block_creation_count_for_tile_with_item(tr, Some(&item(0x19))).count;
            assert_eq!(rand(), {
                srand(seed);
                rand();
                rand()
            });
            if (0.3f64 + r as f64) as i32 == 1 {
                kept += 1;
                assert_eq!(c, 3);
            } else {
                assert_eq!(c, 0);
            }
        }
        assert!(kept > 0 && kept < 39);
        // dead cactus
        set(2, 0x2c, 0);
        srand(3);
        b.free_block_creation_count_for_tile_with_item(tr, None);
        assert_eq!(rand(), {
            srand(3);
            rand();
            rand()
        });
        assert_eq!(b.free_block_bonus_creation_count(), 0);
    }

    #[test]
    fn start_and_stop_interacting() {
        let (world, _dw, dwref, o, h) = mk();
        let b = bh_of(&o);
        // DIG at tile index of (6, 7)
        let idx = crate::world::tiles::world_index_at_world_pos(IntPair { x: 6, y: 7 }, &world);
        b.bh.borrow_mut().gather_timer_left_over = 3.0;
        let tl = Some(tile(&world, 6, 7));
        b.start_interacting_with_tile_at_index_tile_interaction_type(idx, tl, it::DIG);
        {
            let f = b.bh.borrow();
            assert_eq!(f.state.interacting, 1);
            assert_eq!(f.state.interacting_square, IntPair { x: 6, y: 7 });
            assert_eq!(f.state.interaction_type, 1);
            assert_eq!(f.state.goal_interaction_type_while_walking, 1);
            assert_eq!(f.state.gather_speed, 1.0);
            assert_eq!(f.gather_timer_left_over, 0.0);
            assert_eq!(f.state.interaction_timer, 0.0);
            assert_eq!(f.interacting_tile, tl);
        }
        assert!(h.take().contains(&"stopriding".to_string()));
        // FIRE_RANGED_WEAPON / STAB do not stop riding
        b.start_interacting_with_tile_at_index_tile_interaction_type(idx, tl, it::STAB);
        assert!(!h.take().contains(&"stopriding".to_string()));
        // stopInteracting
        b.bh.borrow_mut().state.sub_animation_type = 6;
        b.bh.borrow_mut().chasing_npc = true;
        b.bh.borrow_mut().local_gather_progress = 7;
        b.bh.borrow_mut().interaction_item_index = 3;
        dwchanged_clear(&dwref);
        b.stop_interacting();
        let f = b.bh.borrow();
        assert_eq!(f.state.interacting, 0);
        assert_eq!(f.state.interaction_type, 0);
        assert_eq!(f.state.sub_animation_type, 0);
        assert!(!f.chasing_npc && f.interaction_item.is_none());
        assert_eq!((f.interaction_item_index, f.interaction_item_sub_index), (-1, -1));
        assert_eq!(f.local_gather_progress, 0);
        drop(f);
        assert!(o.update_needs_to_be_sent());
    }

    fn dwchanged_clear(_dw: &crate::objects::base::DynamicWorldRef) {}

    /// RIDE: rideObject cleared when the animal cannot be ridden; achievement only for non-net, non-ctl.
    #[test]
    fn start_interacting_ride() {
        let (_world, _dw, _dwref, o, h) = mk();
        let b = bh_of(&o);
        let idx = 0;
        // nil tappedNPC: the check fails, nothing set, interacting stays 0
        b.start_interacting_with_tile_at_index_tile_interaction_type(idx, None, it::RIDE);
        assert!(b.bh.borrow().ride_object.is_none());
        assert_eq!(b.st().interacting, 0);
        // cannot ride -> rideObject nil again
        b.bh.borrow_mut().tapped_npc = Some(stub(28, true));
        h.can_ride.set(false);
        b.start_interacting_with_tile_at_index_tile_interaction_type(idx, None, it::RIDE);
        assert!(b.bh.borrow().ride_object.is_none());
        // can ride: animation 27, goal NONE, chasing off, interacting NOT set
        h.can_ride.set(true);
        b.bh.borrow_mut().chasing_npc = true;
        b.bh.borrow_mut().state.goal_interaction_type_while_walking = 12;
        b.start_interacting_with_tile_at_index_tile_interaction_type(idx, None, it::RIDE);
        let f = b.bh.borrow();
        assert!(f.ride_object.is_some());
        assert_eq!(f.state.animation_type, 27);
        assert_eq!(f.state.goal_interaction_type_while_walking, 0);
        assert!(!f.chasing_npc);
        assert_eq!(f.state.interacting, 0);
    }

    /// : remote_update.
    #[test]
    fn remote_update_apply_and_queue() {
        let (world, _dw, dwref, _o, h) = mk();
        let _ = &dwref;
        // build the replica from creation data
        let src = new_bh(&world, &dwref, 0);
        bh_of(&src).set_client_id("cid");
        bh_of(&src).bh.borrow_mut().name = Some("N".into());
        let data = bh_of(&src).creation_net_data_for_client_impl(None);
        let rep = Blockhead::init_with_world_dynamic_world_cache_net_data(&world, &dwref, None, &data).unwrap();
        let r = bh_of(&rep);
        r.bh.borrow_mut().client_id = Some("cid".into());
        let mut u = BlockheadUpdateData::default();
        u.to_square_x = 7;
        u.to_square_y = 8;
        u.interacting_square_x = 7;
        u.interacting_square_y = 7;
        u.traverse_type = 1;
        u.traverse_to_key_frame = 0;
        u.happiness = 127;
        u.health = 64;
        u.heat = 9;
        u.has_coffee_energy = 1;
        u.terrain_difficulty = 33;
        u.animation_type = 1;
        u.interaction_item_type = 0x10;
        u.is_currently_active = 1;
        // non-net: ignored
        bh_of(&src).remote_update_impl(&src, &u.to_vec());
        assert_eq!(bh_of(&src).bh.borrow().to_square, IntPair { x: 5, y: 8 });
        // first update: direct apply
        rep.remote_update(&rep, &u.to_vec());
        {
            let f = r.bh.borrow();
            assert_eq!(f.to_square, IntPair { x: 7, y: 8 });
            assert_eq!(f.terrain_difficulty, 33);
            assert_eq!(f.traverse_type, 1);
            assert_eq!(f.state.animation_type, 1);
            assert_eq!(f.state.interacting_square, IntPair { x: 7, y: 7 });
            assert!(f.has_had_remote_update);
            assert_eq!(f.current_net_queue_size, 0);
            assert_eq!(f.state.health, 64.0 / 127.0);
            assert_eq!(f.state.happiness, 1.0);
            assert_eq!(f.state.heat, 9.0);
            assert_eq!(f.state.coffee_energy_timer, 1.0);
            assert_eq!(f.remote_interaction_item_type, 0x10);
            assert!(f.is_currently_active_blockhead_according_to_net_data);
        }
        // later updates: queue, capped at 3 (the 4th and later overwrite slot 3)
        for i in 0..6u32 {
            u.to_square_x = 10 + i;
            rep.remote_update(&rep, &u.to_vec());
        }
        {
            let f = r.bh.borrow();
            assert_eq!(f.current_net_queue_size, 4);
            assert_eq!(f.net_next_to_square[0].x, 10);
            assert_eq!(f.net_next_to_square[1].x, 11);
            assert_eq!(f.net_next_to_square[2].x, 12);
            assert_eq!(f.net_next_to_square[3].x, 15);
            // direct fields were not touched by queued updates
            assert_eq!(f.to_square.x, 7);
        }
        // regenerating edge: log "Blockhead died named..." only with a server; here none
        u.regenerating = 1;
        rep.remote_update(&rep, &u.to_vec());
        assert_eq!(r.st().regenerating, 1);
        let _ = h;
    }

    #[test]
    fn remote_update_jetpack_and_wrap() {
        let (world, _dw, dwref, _o, _h) = mk();
        let src = new_bh(&world, &dwref, 0);
        let data = bh_of(&src).creation_net_data_for_client_impl(None);
        let rep = Blockhead::init_with_world_dynamic_world_cache_net_data(&world, &dwref, None, &data).unwrap();
        let r = bh_of(&rep);
        let mut u = BlockheadUpdateData::default();
        u.is_in_jet_pack_free_flight_mode = 1;
        u.fishing_rod_cast_x = 5000; // 0.5
        u.fishing_rod_cast_y = 500; // 0.05 -> zeroed (|v| < 0.1)
        u.to_square_x = 9;
        u.to_square_y = 12;
        rep.remote_update(&rep, &u.to_vec());
        {
            let f = r.bh.borrow();
            // pos = (5, 8): dx = 4, dy = 4 -> v += (4, 4) * 0.5
            assert_eq!(f.jet_pack_velocity, Vector2 { x: 0.5 + 2.0, y: 0.0 + 2.0 });
            assert_eq!(f.state.animation_type, 32);
            assert!(f.is_in_jet_pack_free_flight_mode);
            assert!(!f.has_had_remote_update); // jetpack mode does not touch the walk fields
            assert_eq!(f.jetpack_send_update_timer, 0.0);
        }
        // x-wrap: remote x just below the world edge while we are near 0
        u.to_square_x = 1020;
        u.fishing_rod_cast_x = 0;
        u.fishing_rod_cast_y = 0;
        rep.remote_update(&rep, &u.to_vec());
        // dx = 1015 >= W/2 (512) -> dx - 1024 = -9 -> v.x = 0 + -4.5
        assert_eq!(r.bh.borrow().jet_pack_velocity.x, -4.5);
        // negative x: toSquareX as u32 is huge and > W, so the signed value is used and "wrapped found" logged
        u.to_square_x = (-4i32) as u32;
        rep.remote_update(&rep, &u.to_vec());
        assert_eq!(r.bh.borrow().jet_pack_velocity.x, (-4.0 - 5.0) * 0.5);
    }

    #[test]
    fn remote_update_ride_and_interaction_ids() {
        let (world, _dw, dwref, _o, _h) = mk();
        let src = new_bh(&world, &dwref, 0);
        let data = bh_of(&src).creation_net_data_for_client_impl(None);
        let rep = Blockhead::init_with_world_dynamic_world_cache_net_data(&world, &dwref, None, &data).unwrap();
        let r = bh_of(&rep);
        let mut u = BlockheadUpdateData::default();
        // ids with no matching object (default host): attach gives nil
        u.interaction_object_id = 9;
        u.interaction_workbench_id = 10;
        rep.remote_update(&rep, &u.to_vec());
        assert!(r.bh.borrow().interaction_object.is_none() && r.bh.borrow().interaction_workbench.is_none());
        // detach with id 0
        let o1 = stub(1, true);
        r.bh.borrow_mut().interaction_object = Some(o1);
        u.interaction_object_id = 0;
        u.interaction_workbench_id = 0;
        rep.remote_update(&rep, &u.to_vec());
        assert!(r.bh.borrow().interaction_object.is_none());
    }

    #[test]
    fn is_idle_and_should_continue_simulating() {
        let (_world, _dw, _dwref, o, h) = mk();
        let b = bh_of(&o);
        assert!(b.is_idle());
        b.bh.borrow_mut().action_queue.push(Rc::new(()));
        assert!(!b.is_idle());
        b.bh.borrow_mut().action_queue.clear();
        b.bh.borrow_mut().state.sub_animation_type = 7;
        assert!(!b.is_idle());
        b.bh.borrow_mut().state.sub_animation_type = 6;
        assert!(!b.is_idle());
        let _ = h;
        // shouldContinueSimulating
        b.bh.borrow_mut().state.sub_animation_type = 0;
        assert!(!b.should_continue_simulating()); // no actions
        b.bh.borrow_mut().action_queue.push(Rc::new(()));
        assert!(b.should_continue_simulating());
        b.bh.borrow_mut().state.sub_animation_type = 7;
        assert!(!b.should_continue_simulating());
        b.bh.borrow_mut().state.sub_animation_type = 5;
        assert!(b.should_continue_simulating());
        b.bh.borrow_mut().cancel_simulate_due_to_collapse = true;
        assert!(!b.should_continue_simulating());
        b.bh.borrow_mut().cancel_simulate_due_to_collapse = false;
        b.bh.borrow_mut().state.on_trade_mission = 1;
        assert!(b.should_continue_simulating()); // asleep wins (step 2)
        b.bh.borrow_mut().state.sub_animation_type = 0;
        assert!(!b.should_continue_simulating()); // step 6
        assert!(b.on_trade_mission());
    }

    /// collapse / sleep / meditate state changes.
    #[test]
    fn collapse_sleep_meditate() {
        let (_world, _dw, _dwref, o, h) = mk();
        let b = bh_of(&o);
        // cannot collapse with full energy
        b.collapse_if_possible();
        assert_eq!(b.st().sub_animation_type, 0);
        {
            let mut f = b.bh.borrow_mut();
            f.state.energy = 0.0;
            f.state.fullness = -0.5;
            f.action_queue.push(Rc::new(()));
        }
        b.collapse_if_possible();
        {
            let f = b.bh.borrow();
            assert_eq!(f.state.sub_animation_type, 6);
            assert!(f.cancel_simulate_due_to_collapse);
            assert_eq!(f.tip_type, 1); // energy < 0.01 -> exhausted
            assert_eq!(f.state.fullness, 0.0);
            assert_eq!(f.state.energy, 0.25);
        }
        assert!(h.take().contains(&"stopriding".to_string()));
        // starvation tip when energy >= 0.01
        {
            let mut f = b.bh.borrow_mut();
            f.state.sub_animation_type = 0;
            f.state.energy = 0.2;
            f.state.fullness = 0.1;
            f.tip_type = 0;
        }
        b.collapse_if_possible();
        assert_eq!(b.bh.borrow().tip_type, 2);
        // controlled by server: a simulation event 5 instead of the tip
        {
            let mut f = b.bh.borrow_mut();
            f.state.sub_animation_type = 0;
            f.state.energy = 0.2;
            f.is_client_blockhead_being_controlled_by_server = true;
            f.tip_type = 0;
        }
        b.collapse_if_possible();
        assert_eq!(b.bh.borrow().tip_type, 0);
        assert!(h.take().contains(&"sim:5:Text(\"STARVATION\")".to_string()));
        b.bh.borrow_mut().is_client_blockhead_being_controlled_by_server = false;
        // sleep on spot
        {
            let mut f = b.bh.borrow_mut();
            f.state.sub_animation_type = 0;
            f.state.energy = 0.5;
            f.action_queue.clear();
        }
        b.sleep_on_spot_if_possible_otherwise_cancel_actions();
        assert_eq!(b.st().sub_animation_type, 6);
        // meditate
        b.bh.borrow_mut().state.sub_animation_type = 0;
        b.meditate_if_possible();
        assert_eq!(b.st().sub_animation_type, 7);
        // otherwise cancel actions
        {
            let mut f = b.bh.borrow_mut();
            f.state.sub_animation_type = 0;
            f.state.energy = 1.2;
            f.action_queue.push(Rc::new(()));
        }
        b.sleep_on_spot_if_possible_otherwise_cancel_actions();
        assert!(b.bh.borrow().action_queue.is_empty());
        assert_eq!(b.st().sub_animation_type, 0);
    }

    #[test]
    fn prepare_inventory_for_saving_flags() {
        let (_world, _dw, _dwref, o, h) = mk();
        let b = bh_of(&o);
        h.take();
        b.prepare_inventory_for_saving(); // new character: slot 0..4 flags
        let log = h.take();
        // only slot 0 had its changed flag set by the constructor
        assert_eq!(log, vec!["invchanged:0:-1:false".to_string()]);
        {
            let mut f = b.bh.borrow_mut();
            f.sub_inventory_changed_this_frame_slots[2][1] = true;
            f.sub_inventory_usage_changed_this_frame_slots[2][1] = true;
            f.sub_inventory_usage_changed_this_frame_slots[2][3] = true;
            f.inventory_usage_changed_this_frame_slots[3] = true;
            f.sub_inventory_usage_changed_this_frame_slots[3][0] = true;
            f.inventory_changed_this_frame_slots[4] = true;
            f.inventory_usage_changed_this_frame_slots[4] = true;
            f.sub_inventory_changed_this_frame_slots[4][2] = true;
        }
        b.prepare_inventory_for_saving();
        assert_eq!(
            h.take(),
            vec!["invchanged:2:1:false", "invchanged:2:3:true", "invchanged:3:-1:true", "invchanged:4:-1:false"]
                .into_iter()
                .map(String::from)
                .collect::<Vec<_>>()
        );
        let f = b.bh.borrow();
        assert!(f.sub_inventory_changed_this_frame_slots.iter().all(|r| r.iter().all(|x| !x)));
        assert!(f.sub_inventory_usage_changed_this_frame_slots.iter().all(|r| r.iter().all(|x| !x)));
        assert!(f.inventory_usage_changed_this_frame_slots.iter().all(|x| !x));
    }

    ///  (subset): torch / egg rows, failed checks falling through, no-match nil.
    #[test]
    fn pickup_item_for_tile() {
        let (world, _dw, _dwref, o, h) = mk();
        let b = bh_of(&o);
        let t = tile(&world, 9, 20);
        let p = IntPair { x: 9, y: 20 };
        t.set_type_index(2);
        t.set_contents(0);
        t.set_background_contents(0);
        t.set_foreground_contents(b'1');
        // no torch object: removable check answers NO -> nil
        assert!(b.pickup_item_for_tile_ast_pos(Some(&t), p).is_none());
        *h.torch.borrow_mut() = Some(stub(1, true));
        let i = b.pickup_item_for_tile_ast_pos(Some(&t), p).unwrap();
        assert_eq!(i.borrow().item_type.0, 0x11);
        for (c, ty) in [
            (b'a', 0xb7),
            (b'2', 0x2f),
            (b'W', 0x96),
            (b'f', 0xfe),
            (b'i', 0x102),
            (b'R', 0x91),
            (b'S', 0x92),
            (b'T', 0x93),
            (b'U', 0x94),
            (b'V', 0x95),
            (b'3', 0x4b),
            (b'5', 0x4c),
            (b'7', 0x56),
            (b'9', 0x57),
            (b';', 0x58),
        ] {
            t.set_foreground_contents(c);
            let i = b.pickup_item_for_tile_ast_pos(Some(&t), p).unwrap();
            assert_eq!(i.borrow().item_type.0, ty, "{}", c as char);
        }
        // not removable: falls through to nil
        *h.torch.borrow_mut() = Some(stub(1, false));
        t.set_foreground_contents(b'1');
        assert!(b.pickup_item_for_tile_ast_pos(Some(&t), p).is_none());
        // cannot pick up
        *h.torch.borrow_mut() = Some(stub(1, true));
        h.can_pick.set(false);
        assert!(b.pickup_item_for_tile_ast_pos(Some(&t), p).is_none());
        h.can_pick.set(true);
        // no match and a nil tile
        t.set_foreground_contents(0);
        assert!(b.pickup_item_for_tile_ast_pos(Some(&t), p).is_none());
        assert!(b.pickup_item_for_tile_ast_pos(None, p).is_none());
    }

    #[test]
    fn pick_up_item_if_possible_pickup_rows() {
        let (world, _dw, _dwref, o, h) = mk();
        let b = bh_of(&o);
        let t = tile(&world, 9, 20);
        let p = IntPair { x: 9, y: 20 };
        t.set_type_index(2);
        t.set_contents(0);
        t.set_foreground_contents(b'1');
        *h.torch.borrow_mut() = Some(stub(1, true));
        // interaction type NONE: nothing happens
        h.cit.set(0);
        b.pick_up_item_if_possible_in_tile_at_pos(Some(&t), p);
        assert!(h.take().is_empty());
        // PICKUP: the item is added, then the torch row removes the object and clears the foreground
        h.cit.set(it::PICKUP);
        b.bh.borrow_mut().expected_craft_items = vec![5, 0x11, 0x11];
        b.pick_up_item_if_possible_in_tile_at_pos(Some(&t), p);
        let log = h.take();
        assert_eq!(log, vec!["additem:17:true".to_string(), "remove:Torch:9,20".to_string()]);
        assert_eq!(t.foreground_contents(), 0);
        assert_eq!(b.bh.borrow().expected_craft_items, vec![5, 0x11]);
        // unmatched removal row: the item (an ordinary door item) is still added, nothing removed
        t.set_foreground_contents(0);
        t.set_contents(0x92);
        let door = stub(1, true);
        let _ = door;
        // a door whose item type is not the gate: no door object available -> nil item, nothing happens
        b.pick_up_item_if_possible_in_tile_at_pos(Some(&t), p);
        assert!(h.take().is_empty());
        // achievements for gems: only for !isNet && !ctl
        t.set_contents(0);
        t.set_foreground_contents(b'3'); // ruby torch row 0x4b
        b.pick_up_item_if_possible_in_tile_at_pos(Some(&t), p);
        let log = h.take();
        assert_eq!(log[0], "ach:grp.ruby");
        t.set_foreground_contents(b';'); // diamond torch 0x58 on a diamond tree tile
        t.set_contents(0x7b);
        b.pick_up_item_if_possible_in_tile_at_pos(Some(&t), p);
        let log = h.take();
        assert_eq!(&log[..2], &["ach:grp.diamond".to_string(), "ach:grp.diamond_tree".to_string()]);
        // isNet: no achievements
        b.base.borrow_mut().is_net = true;
        t.set_foreground_contents(b'3');
        t.set_contents(0);
        b.pick_up_item_if_possible_in_tile_at_pos(Some(&t), p);
        assert!(h.take().iter().all(|l| !l.starts_with("ach:")));
    }
}
