//! Workbench crafting, fuel, power and the per-tick update.
//!
//! Behaviour follows the clean-room specification `workbench_craft_spec.md`; section numbers in the comments
//! refer to it. Covers: the crystal wallet, starting / aborting / completing a craft, hurrying, fuel
//! management, the generators (campfire, solar panel, flywheel, steam generator) and the tick phases.

use std::cell::RefCell;
use std::convert::Infallible;

use crate::crafting::craftable_item_object::{zero_craftable_item, CraftableItemObject, CraftableItemObjectRef};
use crate::crafting::workbench::{
    host, particle_burst_rands, Workbench, WB_CAMPFIRE, WB_FLYWHEEL, WB_REFINERY, WB_SOLAR_PANEL, WB_STEAM_GENERATOR,
};
use crate::env::crystal_manager::CrystalManager;
use crate::fmath::cconv::{f32_to_i32, f64_to_i32};
use crate::fmath::libm::fmodf;
use crate::fmath::vector::clamp;
use crate::foundation::md5::string_from_md5;
use crate::items::inventory_item::ItemRef;
use crate::items::recipes::preserve_item_data_a_in_crafted_item;
use crate::objects::base::{DynamicObject, DynamicObjectBase};
use crate::objects::class_info::{ClassInfo, DYNAMIC_OBJECT_CLASS};
use crate::objects::handle::{self, ptr_eq, ObjRef};
use crate::objects::interaction_object::interaction_object_start_interaction_with_blockhead;
use crate::rng::glibc_rand::r01;
use crate::types::item_type::ItemType;
use crate::types::structs::{CraftableItem, IntPair};
use crate::world::helpers::{tile_is_burnable, tile_is_water};
use crate::world::misc::deps;
use crate::world::tiles::tile_at_world_position;

const TIME_CRYSTAL: u32 = 11;
const WORKBENCH_UPGRADE: u32 = 132;
const DYE_BENCH: i32 = 12;
/// Refunds above this size are refused.
const REFUND_LIMIT: i32 = 50001;

// =====================================================================================================
// 1. Crystal wallet
// =====================================================================================================

/// `md5hex("7acfe93afc08" + v + "c65ae2c54ecaf07f")`: the signature a stored balance must carry.
fn sig_a(v: i32) -> String {
    string_from_md5(&["7acfe93afc08", &v.to_string(), "c65ae2c54ecaf07f"].concat())
}

/// `md5hex("7acfe93afc08c" + v + "65ae2c54ecaf07f")`: the signature a balance change must carry.
fn sig_b(v: i32) -> String {
    string_from_md5(&["7acfe93afc08c", &v.to_string(), "65ae2c54ecaf07f"].concat())
}

/// Sets the shared wallet to a validly signed balance (test / integration helper).
pub fn set_crystal_wallet(amount: i32) {
    let cm = CrystalManager::instance();
    let old = cm.amount();
    let delta = old.wrapping_sub(amount);
    cm.modify_modify_string(delta, &sig_b(old.wrapping_sub(delta).wrapping_add(73)));
}

/// Sets the shared wallet balance to 0 (test helper).
pub fn reset_crystal_wallet() {
    set_crystal_wallet(0);
}

/// Raw stored balance.
pub fn crystal_amount() -> i32 {
    CrystalManager::instance().amount()
}

/// Raw stored signature.
pub fn crystal_amount_string() -> Option<String> {
    CrystalManager::instance().amount_string()
}

/// The balance if its signature verifies, else 0.
pub fn crystal_balance_checked() -> i32 {
    let amount = crystal_amount();
    if crystal_amount_string().as_deref() == Some(sig_a(amount).as_str()) {
        amount
    } else {
        0
    }
}

/// Spends `n` crystals.
pub fn crystal_spend(n: i32) {
    let cm = CrystalManager::instance();
    cm.set_count_watcher(None);
    let amount = cm.amount();
    cm.modify_modify_string(n, &sig_b(amount.wrapping_sub(n).wrapping_add(73)));
}

/// Refunds `n` crystals (ignored for `n >= 50001`).
pub fn crystal_refund(n: i32) {
    let cm = CrystalManager::instance();
    cm.set_count_watcher(None);
    if n < REFUND_LIMIT {
        let amount = cm.amount();
        cm.modify_modify_string(n.wrapping_neg(), &sig_b(amount.wrapping_add(n).wrapping_add(73)));
    }
}

// =====================================================================================================
// CraftableItemObject as a World argument
// =====================================================================================================

/// The world callback types its first argument as a dynamic object, but a craftable item object is not one;
/// this shim carries it through that parameter and [`craftable_item_object_of_shim`] unwraps it.
pub struct CraftableItemShim {
    base: RefCell<DynamicObjectBase>,
    cio: CraftableItemObjectRef,
}

impl DynamicObject for CraftableItemShim {
    fn base(&self) -> &RefCell<DynamicObjectBase> {
        &self.base
    }
    fn class_info(&self) -> &'static ClassInfo {
        &DYNAMIC_OBJECT_CLASS
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

/// Wraps a craftable item object for the world callback.
pub fn craftable_item_shim(cio: CraftableItemObjectRef) -> ObjRef {
    handle::alloc_obj(|w| CraftableItemShim { base: RefCell::new(DynamicObjectBase::alloc_zeroed_with(w)), cio })
}

/// Unwraps a shim made by [`craftable_item_shim`].
pub fn craftable_item_object_of_shim(o: &ObjRef) -> Option<CraftableItemObjectRef> {
    o.as_any().downcast_ref::<CraftableItemShim>().map(|s| s.cio.clone())
}

// =====================================================================================================
// Small helpers
// =====================================================================================================

/// Number of valid required-item entries of a recipe.
fn required_len(ci: &CraftableItem) -> usize {
    ci.number_of_required_items.clamp(0, 8) as usize
}

/// A user that is a network copy (absent users are not).
fn is_net_copy(user: Option<&ObjRef>) -> bool {
    user.is_some_and(|u| u.is_net())
}

/// A client blockhead being controlled by the server (absent users are not).
fn is_server_controlled(user: Option<&ObjRef>) -> bool {
    user.is_some_and(|u| deps().bh_is_client_blockhead_being_controlled_by_server(u))
}

/// Achievement identifier for a finished craft (section 5, step 7). Explicit crafted types win over the dye rule.
fn achievement_for(crafted: u32, bench_type: i32) -> Option<&'static str> {
    Some(match crafted {
        38 => "grp.bronze",
        65 => "grp.iron",
        92 => "grp.steel",
        94 => "grp.amethyst_pickaxe",
        95 => "grp.sapphire_pickaxe",
        96 => "grp.emerald_pickaxe",
        97 => "grp.ruby_pickaxe",
        98 => "grp.diamond_pickaxe",
        90 => "grp.gold_pickaxe",
        145 => "grp.amethyst_chandelier",
        146 => "grp.sapphire_chandelier",
        147 => "grp.emerald_chandelier",
        148 => "grp.ruby_chandelier",
        149 => "grp.diamond_chandelier",
        181 | 182 => "grp.chef",
        _ if bench_type == DYE_BENCH && crafted != WORKBENCH_UPGRADE => "grp.dye",
        _ => return None,
    })
}

/// Electricity profile of a bench type: (consumption per unit, counter rate, refill target).
fn power_profile(ty: i32) -> (u16, f32, i32) {
    match ty {
        WB_REFINERY => (40, 2.0, 200),
        0x11 | 0x12 | 0x1e => (20, 1.0, 100),
        _ => (10, 1.0, 100),
    }
}

/// Target cell of a campfire's fire spread for the roll `u`; `None` when the spec defines no target.
fn spread_target(u: f32, has_fuel: bool, p: IntPair) -> Option<IntPair> {
    if u > 0.4 {
        Some(IntPair { x: p.x, y: p.y + 1 })
    } else if u > 0.2 {
        has_fuel.then_some(IntPair { x: p.x, y: p.y + 2 })
    } else if u > 0.1 {
        Some(IntPair { x: p.x + 1, y: p.y })
    } else {
        Some(IntPair { x: p.x - 1, y: p.y })
    }
}

/// Values fixed for the duration of one tick.
struct Tick {
    me: ObjRef,
    dt: f32,
    /// Accurate dt, scaled by 1.5 for a caffeinated craft user.
    work: f32,
    accurate_dt: f32,
    simulation: bool,
    ty: i32,
}

impl Workbench {
    fn recipe(&self) -> CraftableItem {
        self.crafting_item_object().map_or_else(zero_craftable_item, |c| c.craftable_item())
    }

    fn user(&self) -> Option<ObjRef> {
        self.io.borrow().current_blockhead.clone()
    }

    fn set_in_use(&self, v: bool) {
        self.io.borrow_mut().is_in_use = v;
    }

    fn in_use(&self) -> bool {
        self.io.borrow().is_in_use
    }

    /// Tell the dynamic world and mark the object as needing a network update.
    fn announce(&self, me: &ObjRef) {
        self.notify();
        me.set_update_needs_to_be_sent(true);
    }

    fn out_cell(&self) -> IntPair {
        let p = self.pos();
        IntPair { x: p.x, y: p.y + 1 }
    }

    /// Drops a stored item as a free block at `at`.
    fn drop_item(&self, at: IntPair, item: &ItemRef, priority: Option<&ObjRef>) {
        let it = item.borrow();
        deps().create_free_block_at_position_of_type_data_a_data_b_sub_items_dynamic_object_save_dict_hovers_play_sound_priority_blockhead(
            at,
            it.item_type,
            it.data_a,
            it.data_b,
            it.sub_items.as_deref(),
            it.dynamic_object_save_dict.as_ref(),
            false,
            2,
            priority,
        );
    }

    /// Drops a plain (data 0/0) item of `ty` as a free block at `at`.
    fn drop_plain(&self, at: IntPair, ty: u32, priority: Option<&ObjRef>) {
        deps().create_free_block_at_position_of_type_data_a_data_b_sub_items_dynamic_object_save_dict_hovers_play_sound_priority_blockhead(
            at,
            ItemType(ty),
            0,
            0,
            None,
            None,
            false,
            2,
            priority,
        );
    }

    fn take_slot(&self, k: usize) -> Option<Vec<ItemRef>> {
        self.st.borrow_mut().source_items[k].take()
    }

    fn world_item_finished(
        &self,
        me: &ObjRef,
        cio: &CraftableItemObjectRef,
        all_finished: bool,
        user: Option<&ObjRef>,
    ) {
        let shim = craftable_item_shim(cio.clone());
        if let Some(w) = self.w() {
            w.craft_item_finished_at_workbench_all_finished_blockhead(Some(&shim), Some(me), all_finished, user);
        }
    }

    // ------------------------------------------------------------------------------------------------
    // 2. Starting a craft
    // ------------------------------------------------------------------------------------------------

    /// Takes the time-crystal price (or records the discrepancy). `None` when the wallet cannot pay.
    fn pay_crystals(&self, ci: &CraftableItem, count: i32, controlled: bool) -> Option<i32> {
        let mut discrepancy = 0i32;
        for k in 0..required_len(ci) {
            if ci.required_items[k] != TIME_CRYSTAL {
                continue;
            }
            let need = ci.required_item_counts[k].wrapping_mul(count);
            if controlled {
                discrepancy = discrepancy.wrapping_sub(need);
            } else if crystal_balance_checked() < need {
                return None;
            } else if need > 0 {
                crystal_spend(need);
            }
        }
        Some(discrepancy)
    }

    /// Undo of a failed start: drop what was collected and refund the crystals.
    fn roll_back_start(&self, ci: &CraftableItem, count: i32, upto: usize, user: &ObjRef) {
        let out = self.out_cell();
        for k in 0..=upto {
            for it in self.take_slot(k).unwrap_or_default() {
                self.drop_item(out, &it, Some(user));
            }
        }
        for j in 0..required_len(ci) {
            if ci.required_items[j] == TIME_CRYSTAL {
                crystal_refund(ci.required_item_counts[j].wrapping_mul(count));
            }
        }
    }

    /// `craft(recipeObject, blockhead, count)`.
    pub fn craft_item_with_blockhead_craft_progress_ui_count(
        &self,
        me: &ObjRef,
        crafting: CraftableItemObjectRef,
        blockhead: &ObjRef,
        _craft_progress_ui: Option<Infallible>,
        count: i32,
    ) -> bool {
        if self.in_use() {
            return false;
        }
        {
            let mut s = self.st.borrow_mut();
            s.crafting_item_object = Some(crafting.clone());
            s.source_items = Default::default();
        }
        let ci = crafting.craftable_item();
        if ci.crafted_item_type == WORKBENCH_UPGRADE && ci.upgrade_level != self.level().wrapping_add(2) {
            return false;
        }
        let now = self.world_time();
        {
            let mut s = self.st.borrow_mut();
            s.last_world_time = now;
            s.hurrying = false;
            s.craft_progress_count = 0.0;
            s.count = count;
            s.count_left = count;
            s.count_created = 0;
            s.fuel_counter = 1.0;
            if s.requires_electricity {
                s.available_electricity = 0;
            }
        }
        let Some(discrepancy) = self.pay_crystals(&ci, count, is_server_controlled(Some(blockhead))) else {
            return false;
        };
        let wb_host = host();
        for k in 0..required_len(&ci) {
            if ci.required_items[k] == TIME_CRYSTAL {
                continue;
            }
            let need = ci.required_item_counts[k].wrapping_mul(count);
            let got = wb_host.bh_subtract_items_from_inventory_of_type_count_data_b(
                blockhead,
                ItemType(ci.required_items[k]),
                need,
                ci.required_item_data_bs[k],
            );
            let short = (got.len() as i32) < need;
            self.st.borrow_mut().source_items[k] = Some(got);
            if short {
                self.roll_back_start(&ci, count, k, blockhead);
                return false;
            }
        }
        deps().bh_add_to_crystal_discrepency(blockhead, discrepancy);
        interaction_object_start_interaction_with_blockhead(me, blockhead);
        wb_host.bh_set_interaction_workbench(blockhead, Some(me));
        self.st.borrow_mut().fraction_complete = 0.0;
        self.notify();
        true
    }

    // ------------------------------------------------------------------------------------------------
    // 3. Fuel management
    // ------------------------------------------------------------------------------------------------

    /// `startManagingFuel(blockhead)`.
    pub fn start_managing_fuel_with_blockhead(&self, me: &ObjRef, blockhead: &ObjRef) {
        let (busy, previous) = {
            let s = self.st.borrow();
            (s.is_in_use_fuel, s.current_fuel_blockhead.clone())
        };
        let same = previous.as_ref().is_some_and(|p| ptr_eq(p, blockhead));
        if busy && !same {
            if let Some(p) = &previous {
                deps().bh_stop_interacting(p);
            }
        }
        let fuel_user = {
            let mut s = self.st.borrow_mut();
            s.is_in_use_fuel = true;
            if !same {
                s.current_fuel_blockhead = Some(blockhead.clone());
            }
            s.current_fuel_blockhead.clone()
        };
        if let Some(u) = fuel_user {
            host().bh_set_interaction_workbench(&u, Some(me));
        }
        self.announce(me);
    }

    // ------------------------------------------------------------------------------------------------
    // 4. Aborting
    // ------------------------------------------------------------------------------------------------

    /// Crystal refund for an abort: `count_k * countLeft`, unless the user is server-controlled.
    fn refund_for_abort(&self, ci: &CraftableItem, k: usize, user: Option<&ObjRef>) {
        if !is_server_controlled(user) {
            crystal_refund(ci.required_item_counts[k].wrapping_mul(self.st.borrow().count_left));
        }
    }

    /// Closing steps shared by both aborts.
    fn finish_abort(&self, me: &ObjRef, user: Option<&ObjRef>) {
        if let Some(w) = self.w() {
            w.craft_aborted_for_workbench_with_blockhead(Some(me), user);
        }
        self.set_in_use(false);
        if let Some(u) = user {
            host().bh_craft_item_finished_at_workbench(u, false, me);
        }
        self.announce(me);
        self.io.borrow_mut().current_blockhead = None;
    }

    /// `abort()`.
    pub fn abort_craft(&self, me: &ObjRef) {
        if self.user().is_some_and(|u| u.needs_removed()) {
            self.io.borrow_mut().current_blockhead = None;
        }
        self.st.borrow_mut().fraction_complete = 0.0;
        let out = self.out_cell();
        let ci = self.recipe();
        let user = self.user();
        let left = self.st.borrow().count_left;
        for k in 0..required_len(&ci) {
            if ci.required_items[k] == TIME_CRYSTAL {
                self.refund_for_abort(&ci, k, user.as_ref());
                continue;
            }
            let stored = self.take_slot(k);
            if stored.is_none() {
                for _ in 0..ci.required_item_counts[k].wrapping_mul(left).max(0) {
                    self.drop_plain(out, ci.required_items[k], user.as_ref());
                }
            }
            for it in stored.iter().flatten() {
                self.drop_item(out, it, user.as_ref());
            }
        }
        self.finish_abort(me, user.as_ref());
    }

    /// `abortImmediatelyAndRestoreItems()`.
    pub fn abort_immediately_and_restore_blockhead_items(&self, me: &ObjRef) {
        let Some(user) = self.user() else { return };
        if !self.in_use() {
            return;
        }
        let ci = self.recipe();
        for k in 0..required_len(&ci) {
            if ci.required_items[k] == TIME_CRYSTAL {
                self.refund_for_abort(&ci, k, Some(&user));
            } else {
                for it in self.take_slot(k).iter().flatten() {
                    host().bh_add_item_to_inventory(&user, it);
                }
            }
        }
        self.finish_abort(me, Some(&user));
    }

    // ------------------------------------------------------------------------------------------------
    // 5. Completing one unit
    // ------------------------------------------------------------------------------------------------

    /// Consumes the ingredients of one unit and books it as created (steps 3 and 4).
    fn finish_unit(&self, me: &ObjRef, ci: &mut CraftableItem, cio: &CraftableItemObjectRef, user: Option<&ObjRef>) {
        for k in 0..required_len(ci) {
            if ci.required_items[k] == TIME_CRYSTAL {
                continue;
            }
            for _ in 0..ci.required_item_counts[k] {
                let front = {
                    let mut s = self.st.borrow_mut();
                    match s.source_items[k].as_mut() {
                        Some(v) if !v.is_empty() => v.remove(0),
                        _ => break,
                    }
                };
                if preserve_item_data_a_in_crafted_item(ItemType(ci.required_items[k]), ItemType(ci.crafted_item_type))
                {
                    ci.crafted_item_data_a = front.borrow().data_a;
                    cio.set_craftable_item(*ci);
                }
            }
        }
        let left = {
            let mut s = self.st.borrow_mut();
            s.count_left = s.count_left.wrapping_sub(1);
            s.count_created = s.count_created.wrapping_add(1);
            s.count_left
        };
        self.world_item_finished(me, cio, left == 0, user);
    }

    /// `craftCompleted()`.
    pub fn craft_completed(&self, me: &ObjRef) {
        let user = self.user();
        if user.as_ref().map_or(true, |u| u.needs_removed()) {
            self.abort_craft(me);
            return;
        }
        self.notify();
        self.st.borrow_mut().fraction_complete = 1.0;
        let cio = self
            .crafting_item_object()
            .unwrap_or_else(|| CraftableItemObject::init_with_craftable_item(zero_craftable_item()));
        let mut ci = cio.craftable_item();
        let crafted = ci.crafted_item_type;
        let pig_iron = crafted == 65 && ci.required_items[0] == 201;
        self.finish_unit(me, &mut ci, &cio, user.as_ref());
        if self.st.borrow().hurrying {
            let cost = self.st.borrow().hurry_cost;
            if cost > 0 {
                crystal_spend(cost);
            }
            while self.st.borrow().count_left > 0 {
                self.finish_unit(me, &mut ci, &cio, user.as_ref());
            }
            self.st.borrow_mut().hurrying = false;
        }
        // World callbacks may have replaced the user.
        let user = self.user();
        if self.st.borrow().count_left == 0 {
            if let Some(u) = &user {
                host().bh_craft_item_finished_at_workbench(u, true, me);
            }
            let mut io = self.io.borrow_mut();
            io.is_in_use = false;
            io.current_blockhead = None;
            drop(io);
            me.set_update_needs_to_be_sent(true);
        }
        if is_server_controlled(user.as_ref()) {
            return;
        }
        let bench_type = self.st.borrow().ty;
        if let Some(w) = self.w() {
            if let Some(id) = achievement_for(crafted, bench_type) {
                w.report_achievement_with_identifier(id);
            }
            if pig_iron {
                w.report_achievement_with_identifier("grp.pig_iron");
            }
        }
    }

    // ------------------------------------------------------------------------------------------------
    // 6 and 7. Hurry and queries
    // ------------------------------------------------------------------------------------------------

    /// Whether a craft is running with a user and a recipe.
    fn actively_crafting(&self) -> Option<CraftableItem> {
        if self.in_use() && self.user().is_some() {
            self.crafting_item_object().map(|c| c.craftable_item())
        } else {
            None
        }
    }

    /// Items still to be produced.
    pub fn total_items_left_to_craft(&self) -> i32 {
        self.actively_crafting().map_or(0, |ci| self.st.borrow().count_left.wrapping_mul(ci.crafted_item_count))
    }

    /// Item type being crafted, 0 when idle.
    pub fn currently_crafting_item_type(&self) -> ItemType {
        ItemType(self.actively_crafting().map_or(0, |ci| ci.crafted_item_type))
    }

    /// `hurry(cost)`: 112 `rand()` draws for the particle burst.
    pub fn hurry_completion(&self, _me: &ObjRef, hurry_cost: i32) {
        if self.st.borrow().hurrying {
            return;
        }
        {
            let mut s = self.st.borrow_mut();
            s.hurry_seconds = s.craft_progress_count;
            s.hurrying = true;
            s.hurry_timer = 0.0;
            s.hurry_cost = hurry_cost;
        }
        self.notify();
        particle_burst_rands(16, self.pos());
    }

    // ------------------------------------------------------------------------------------------------
    // 8. Per-tick update
    // ------------------------------------------------------------------------------------------------

    /// `update(dt, accurateDt, simulation)`.
    pub fn update_accurate_dt_is_simulation_impl(&self, me: &ObjRef, dt: f32, accurate_dt: f32, is_simulation: bool) {
        if me.needs_removed() {
            return;
        }
        self.drop_removed_users(me);
        let caffeinated = self.user().is_some_and(|u| host().bh_has_coffee_energy(&u));
        let tick = Tick {
            me: me.clone(),
            dt,
            work: if caffeinated { 1.5f32 * accurate_dt } else { accurate_dt },
            accurate_dt,
            simulation: is_simulation,
            ty: self.st.borrow().ty,
        };
        if !me.is_net() {
            self.run_generator(&tick);
        }
        self.burn_fuel(&tick);
        self.draw_power(&tick);
        if !self.advance_progress(&tick) {
            return;
        }
        self.clear_stale_state(&tick);
    }

    /// Step 1: users that are being removed.
    fn drop_removed_users(&self, me: &ObjRef) {
        if self.user().is_some_and(|u| u.needs_removed()) {
            self.set_in_use(false);
            self.announce(me);
            self.io.borrow_mut().current_blockhead = None;
        }
        let fuel_gone = self.st.borrow().current_fuel_blockhead.as_ref().is_some_and(|u| u.needs_removed());
        if fuel_gone {
            {
                let mut s = self.st.borrow_mut();
                s.current_fuel_blockhead = None;
                s.is_in_use_fuel = false;
            }
            self.announce(me);
        }
    }

    /// Step 3: authoritative generators and burners.
    fn run_generator(&self, t: &Tick) {
        match t.ty {
            WB_CAMPFIRE => self.tick_campfire(t),
            WB_SOLAR_PANEL => self.tick_solar(t),
            WB_FLYWHEEL => self.tick_flywheel(t),
            WB_STEAM_GENERATOR => self.tick_steam(t),
            _ => {}
        }
    }

    /// Adds `accurate_dt` to the counter; true (and subtracts `period`) once it exceeds `period`.
    fn charge_counter(&self, t: &Tick, period: f32) -> bool {
        let mut s = self.st.borrow_mut();
        s.fuel_counter += t.accurate_dt;
        let ready = s.fuel_counter > period;
        if ready {
            s.fuel_counter -= period;
        }
        ready
    }

    fn tick_solar(&self, t: &Tick) {
        if self.st.borrow().available_electricity >= 100 || !self.charge_counter(t, 4.0) {
            return;
        }
        let gain = f32_to_i32(10.0f32 * self.combined_light_for_solar_panel());
        if gain > 0 {
            let mut s = self.st.borrow_mut();
            s.available_electricity = s.available_electricity.wrapping_add(gain as u16);
            drop(s);
            self.announce(&t.me);
        }
    }

    fn tick_flywheel(&self, t: &Tick) {
        if self.st.borrow().available_electricity >= 0x2000 || !self.charge_counter(t, 5.0) {
            return;
        }
        let avail = self.st.borrow().available_electricity;
        let want = 0x2000i32.wrapping_sub(avail as i16 as i32) as u16;
        let got = host().find_and_subtract_all_power_up_to_for_user(want, &t.me);
        if got < 1 {
            self.st.borrow_mut().time_since_found_electricty += 5.0f32;
        } else {
            {
                let mut s = self.st.borrow_mut();
                s.available_electricity = s.available_electricity.wrapping_add(got as u16);
                s.time_since_found_electricty = 0.0;
            }
            self.announce(&t.me);
        }
    }

    fn tick_steam(&self, t: &Tick) {
        let runs = {
            let s = self.st.borrow();
            s.fuel_fraction > 0.0 && s.available_electricity < 100 && !self.in_use()
        };
        if runs {
            {
                let mut s = self.st.borrow_mut();
                while s.fuel_fraction > 0.0 && s.available_electricity < 100 {
                    s.fuel_fraction -= 0.000583f32;
                    s.available_electricity = s.available_electricity.wrapping_add(10);
                    s.particle_create_timer_steam_engine += 0.1f32;
                }
                s.particle_create_timer_steam_engine = s.particle_create_timer_steam_engine.min(4.0f32);
            }
            self.announce(&t.me);
            self.update_has_fuel(&t.me);
        }
        let mut s = self.st.borrow_mut();
        s.fuel_fraction = clamp(s.fuel_fraction, 0.0, 1.0);
    }

    /// Section 9.
    fn tick_campfire(&self, t: &Tick) {
        let Some(world) = self.w() else { return };
        let p = self.pos();
        let own = tile_at_world_position(p.x, p.y, &world.macro_tiles(), &world);
        if tile_is_water(own) && self.st.borrow().has_fuel {
            self.st.borrow_mut().fuel_fraction = 0.0;
            self.announce(&t.me);
            self.update_has_fuel(&t.me);
        }
        let due = {
            let mut s = self.st.borrow_mut();
            let rate = if s.has_fuel { 1.0f32 } else { 0.05f32 };
            s.fire_spread_timer -= t.dt * rate;
            s.fire_spread_timer <= 0.0
        };
        if !due {
            return;
        }
        let next = r01() * 20.0f32 + 30.0f32;
        self.st.borrow_mut().fire_spread_timer = next;
        let roll = r01();
        let has_fuel = self.st.borrow().has_fuel;
        let Some(target) = spread_target(roll, has_fuel, p) else { return };
        let tile = tile_at_world_position(target.x, target.y, &world.macro_tiles(), &world);
        if tile.is_some() && tile_is_burnable(tile, world.dynamic_world().as_ref(), target) {
            deps().place_fire_at_position(target);
        }
    }

    /// Whether the user may make progress right now: not paused, and not a simulation tick it should skip.
    fn user_is_working(&self, user: &ObjRef, simulation: bool) -> bool {
        let wb_host = host();
        !wb_host.bh_paused(user) && !(simulation && !wb_host.bh_should_continue_simulating(user))
    }

    /// Step 4: fuel burn while crafting.
    fn burn_fuel(&self, t: &Tick) {
        if !(self.st.borrow().has_fuel && self.in_use()) {
            return;
        }
        let user = self.user();
        if is_net_copy(user.as_ref()) {
            return;
        }
        let wb_host = host();
        let may_burn = self.in_use()
            && user.as_ref().map_or(true, |u| self.user_is_working(u, t.simulation))
            && user.as_ref().map_or(true, |u| wb_host.bh_unable_to_work_reason(u) == 0);
        if may_burn {
            let rate = if t.ty == WB_CAMPFIRE || t.ty == 0x1f { 6.0f32 } else { 1.0f32 };
            self.st.borrow_mut().fuel_counter += t.work * rate;
        }
        let burn = self.st.borrow().fuel_counter >= 6.0f32;
        if burn {
            {
                let mut s = self.st.borrow_mut();
                s.fuel_counter -= 6.0f32;
                s.fuel_fraction -= 0.007f32;
            }
            self.announce(&t.me);
        }
        self.update_has_fuel(&t.me);
    }

    /// Step 5: electricity while crafting.
    fn draw_power(&self, t: &Tick) {
        if !(self.st.borrow().requires_electricity && self.in_use()) {
            return;
        }
        let user = self.user();
        let wb_host = host();
        let paused = user.as_ref().is_some_and(|u| wb_host.bh_paused(u));
        let blocked = user.as_ref().is_some_and(|u| wb_host.bh_unable_to_work_reason(u) != 0);
        if is_net_copy(user.as_ref()) || paused || blocked {
            return;
        }
        let (need, rate, base) = power_profile(t.ty);
        let tick_due = {
            let mut s = self.st.borrow_mut();
            s.fuel_counter += t.work * rate;
            let due = s.fuel_counter >= 1.0f32;
            if due {
                s.fuel_counter -= 1.0f32;
            }
            due
        };
        if !tick_due {
            return;
        }
        let left = {
            let mut s = self.st.borrow_mut();
            s.available_electricity = s.available_electricity.saturating_sub(need);
            s.available_electricity
        };
        if left < need {
            let want = base.wrapping_sub(left as i32) as i16 as u16;
            let got = wb_host.find_and_subtract_all_power_up_to_for_user(want, &t.me);
            if got > 0 {
                let mut s = self.st.borrow_mut();
                s.available_electricity = s.available_electricity.wrapping_add(got as u16);
                drop(s);
                self.announce(&t.me);
            }
        }
    }

    /// Step 6. Returns false when the tick must end (the craft was aborted).
    fn advance_progress(&self, t: &Tick) -> bool {
        let Some(user) = self.user().filter(|_| self.in_use()) else { return true };
        if user.is_net() || !self.user_is_working(&user, t.simulation) {
            return true;
        }
        let wb_host = host();
        let at_this_bench = wb_host.bh_is_crafting_at_any_workbench(&user)
            && wb_host.bh_interaction_workbench(&user).is_some_and(|w| ptr_eq(&w, &t.me));
        if !at_this_bench {
            self.abort_craft(&t.me);
            return false;
        }
        if !self.in_use() {
            return true;
        }
        let ci = self.recipe();
        let mult: f64 = if wb_host.bh_double_time_unlocked(&user) { 0.5 } else { 1.0 };
        let secs = ci.seconds_required;
        if self.st.borrow().hurrying {
            let mut s = self.st.borrow_mut();
            let total = secs.wrapping_mul(s.count) as f64;
            s.craft_progress_count =
                (s.craft_progress_count as f64 + (total * mult - s.hurry_seconds as f64) * 0.3f64) as f32;
        } else {
            let can_work = {
                let s = self.st.borrow();
                (s.has_fuel || !s.requires_fuel) && (s.available_electricity != 0 || !s.requires_electricity)
            };
            if can_work && wb_host.bh_unable_to_work_reason(&user) == 0 {
                self.st.borrow_mut().craft_progress_count += t.work;
            }
        }
        let (progress, created) = {
            let s = self.st.borrow();
            (s.craft_progress_count, s.count_created)
        };
        let remaining = f64_to_i32(secs as f64 * mult * created.wrapping_add(1) as f64 - f32_to_i32(progress) as f64);
        if remaining < 1 {
            self.craft_completed(&t.me);
        }
        if self.in_use() {
            let mut s = self.st.borrow_mut();
            let ratio = (s.craft_progress_count as f64 / (secs as f64 * mult)) as f32;
            s.fraction_complete = fmodf(ratio, 1.0f32);
        }
        true
    }

    /// Step 7: stale interaction state.
    fn clear_stale_state(&self, t: &Tick) {
        if self.in_use() {
            if let Some(u) = self.user() {
                if !u.is_net() && !host().bh_is_crafting_at_any_workbench(&u) {
                    self.set_in_use(false);
                    self.io.borrow_mut().current_blockhead = None;
                    self.announce(&t.me);
                }
            }
        }
        let (fuel_busy, fuel_user) = {
            let s = self.st.borrow();
            (s.is_in_use_fuel, s.current_fuel_blockhead.clone())
        };
        let Some(fu) = fuel_user.filter(|u| fuel_busy && !u.is_net()) else { return };
        let wb_host = host();
        if wb_host.bh_is_adding_fuel_to_any_workbench(&fu)
            && wb_host.bh_interaction_workbench(&fu).is_some_and(|w| ptr_eq(&w, &t.me))
        {
            return;
        }
        {
            let mut s = self.st.borrow_mut();
            s.is_in_use_fuel = false;
            s.current_fuel_blockhead = None;
        }
        self.announce(&t.me);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crafting::craftable_item_object::{CraftableItemObject, CraftableItemObjectLike};
    use crate::crafting::workbench::testkit::*;
    use crate::items::inventory_item::InventoryItem;
    use crate::plist::value::PlistValue;

    fn recipe() -> CraftableItem {
        let mut ci = zero_craftable_item();
        ci.crafted_item_type = 12;
        ci.crafted_item_count = 1;
        ci.required_items[0] = 20;
        ci.required_item_counts[0] = 2;
        ci.number_of_required_items = 1;
        ci.seconds_required = 10;
        ci
    }

    fn cio(ci: CraftableItem) -> CraftableItemObjectRef {
        CraftableItemObject::init_with_craftable_item(ci)
    }

    #[test]
    fn refused_upgrade_still_wipes_state() {
        let f = fixture();
        let (wb, me) = make(&f, 2, 10, 10);
        let bh = blockhead(5, false);
        wb.st.borrow_mut().source_items[0] = Some(vec![]);
        let mut ci = recipe();
        ci.crafted_item_type = 132;
        ci.upgrade_level = 5; // level 0 needs 2
        assert!(!wb.craft_item_with_blockhead_craft_progress_ui_count(&me, cio(ci), &bh, None, 1));
        assert!(wb.st.borrow().source_items[0].is_none());
        assert!(wb.crafting_item_object().is_some());
        assert!(!me.is_in_use());
    }

    #[test]
    fn insufficient_items_refund_slots_and_return_false() {
        let f = fixture();
        let (wb, me) = make(&f, 2, 10, 10);
        let bh = blockhead(5, false);
        f.host.stock.borrow_mut().push((20, 3)); // needs 2 * 2 = 4
        assert!(!wb.craft_item_with_blockhead_craft_progress_ui_count(&me, cio(recipe()), &bh, None, 2));
        let log = f.deps.log.borrow().clone();
        assert_eq!(log.iter().filter(|l| *l == "free(10,11,20,0,0,false,2,false)").count(), 3, "{:?}", log);
        assert!(!me.is_in_use());
        assert!(wb.st.borrow().source_items.iter().all(|s| s.is_none()));
        // quirk: timers were already set
        assert_eq!(wb.count(), 2);
        assert_eq!(wb.st.borrow().fuel_counter, 1.0);
    }

    #[test]
    fn successful_craft_starts_interaction() {
        let f = fixture();
        let (wb, me) = make(&f, 2, 10, 10);
        let bh = blockhead(5, false);
        f.host.stock.borrow_mut().push((20, 4));
        assert!(wb.craft_item_with_blockhead_craft_progress_ui_count(&me, cio(recipe()), &bh, None, 2));
        assert!(me.is_in_use());
        assert!(me.current_blockhead().is_some());
        assert_eq!((wb.count(), wb.count_left()), (2, 2));
        assert_eq!(wb.st.borrow().source_items[0].as_ref().unwrap().len(), 4);
        assert!(f.deps.log.borrow().iter().any(|l| l == "discrepency(0)"));
        assert!(f.host.log.borrow().iter().any(|l| l == "setIW(true)"));
        assert_eq!(wb.total_items_left_to_craft(), 2);
        assert_eq!(wb.currently_crafting_item_type(), ItemType(12));
    }

    #[test]
    fn crystal_craft_requires_valid_balance() {
        let f = fixture();
        let (wb, me) = make(&f, 2, 10, 10);
        let bh = blockhead(5, false);
        let mut ci = recipe();
        ci.required_items[0] = 11;
        ci.required_item_counts[0] = 3;
        // fresh wallet: no signed balance -> refused before any item pull
        assert!(!wb.craft_item_with_blockhead_craft_progress_ui_count(&me, cio(ci), &bh, None, 1));
        set_crystal_wallet(10);
        assert!(wb.craft_item_with_blockhead_craft_progress_ui_count(&me, cio(ci), &bh, None, 2));
        assert_eq!(crystal_amount(), 4);
        // abort refunds countLeft * count crystals
        wb.abort_craft(&me);
        assert_eq!(crystal_amount(), 10);
        // controlled blockhead: only the discrepancy
        f.deps.controlled.set(true);
        let (wb2, me2) = make(&f, 2, 20, 10);
        assert!(wb2.craft_item_with_blockhead_craft_progress_ui_count(&me2, cio(ci), &bh, None, 2));
        assert_eq!(crystal_amount(), 10);
        assert!(f.deps.log.borrow().iter().any(|l| l == "discrepency(-6)"));
    }

    #[test]
    fn abort_craft_free_block_counts() {
        let f = fixture();
        let (wb, me) = make(&f, 2, 10, 10);
        let bh = blockhead(5, false);
        {
            let mut s = wb.st.borrow_mut();
            s.crafting_item_object = Some(cio(recipe()));
            s.count_left = 3;
        }
        // nil sourceItems -> required * countLeft plain items
        wb.abort_craft(&me);
        let n = f.deps.log.borrow().iter().filter(|l| *l == "free(10,11,20,0,0,false,2,false)").count();
        assert_eq!(n, 6);
        f.deps.log.borrow_mut().clear();
        // non-nil (even empty) array -> exactly its items
        wb.st.borrow_mut().source_items[0] = Some(vec![InventoryItem::new(ItemType(20), 7, 8, None, None)]);
        wb.io.borrow_mut().current_blockhead = Some(bh);
        wb.io.borrow_mut().is_in_use = true;
        wb.abort_craft(&me);
        assert_eq!(f.deps.log.borrow().iter().filter(|l| l.starts_with("free(")).count(), 1);
        assert!(f.deps.log.borrow().iter().any(|l| l == "free(10,11,20,7,8,false,2,false)"));
        assert!(!me.is_in_use());
        assert!(wb.io.borrow().current_blockhead.is_none());
        assert!(f.host.log.borrow().iter().any(|l| l == "craftItemFinished(false)"));
        f.deps.log.borrow_mut().clear();
        wb.st.borrow_mut().source_items[0] = Some(vec![]);
        wb.abort_craft(&me);
        assert_eq!(f.deps.log.borrow().iter().filter(|l| l.starts_with("free(")).count(), 0);
    }

    #[test]
    fn abort_immediately_gives_items_back() {
        let f = fixture();
        let (wb, me) = make(&f, 2, 10, 10);
        let bh = blockhead(5, false);
        f.host.stock.borrow_mut().push((20, 2));
        assert!(wb.craft_item_with_blockhead_craft_progress_ui_count(&me, cio(recipe()), &bh, None, 1));
        wb.st.borrow_mut().fraction_complete = 0.5;
        wb.abort_immediately_and_restore_blockhead_items(&me);
        assert_eq!(f.host.log.borrow().iter().filter(|l| *l == "addItem(20)").count(), 2);
        assert_eq!(wb.fraction_complete(), 0.5);
        assert!(!me.is_in_use());
        // no-op when nothing runs
        let before = f.host.log.borrow().len();
        wb.abort_immediately_and_restore_blockhead_items(&me);
        assert_eq!(f.host.log.borrow().len(), before);
    }

    #[test]
    fn progress_and_completion_arithmetic() {
        let f = fixture();
        let (wb, me) = make(&f, 2, 10, 10);
        let bh = blockhead(5, false);
        f.host.stock.borrow_mut().push((20, 4));
        f.host.crafting_any.set(true);
        assert!(wb.craft_item_with_blockhead_craft_progress_ui_count(&me, cio(recipe()), &bh, None, 2));
        // the host mock stored the workbench as the interaction workbench in craftItem
        wb.update_accurate_dt_is_simulation_impl(&me, 3.0, 3.0, false);
        assert_eq!(wb.st.borrow().craft_progress_count, 3.0);
        assert!((wb.fraction_complete() - 0.3).abs() < 1e-6);
        assert_eq!(wb.count_left(), 2);
        // 7 more seconds: (int)(10*1*1 - 10) = 0 < 1 -> completes the first item
        wb.update_accurate_dt_is_simulation_impl(&me, 7.0, 7.0, false);
        assert_eq!(wb.count_left(), 1);
        assert_eq!(wb.st.borrow().count_created, 1);
        assert!(me.is_in_use(), "batch continues");
        // progress is NOT reset between batch items
        assert_eq!(wb.st.borrow().craft_progress_count, 10.0);
        // crafted item spawned one tile above the bench; two source items consumed per unit
        assert!(f.deps.log.borrow().iter().any(|l| l.starts_with("free(10,11,12,")), "{:?}", f.deps.log.borrow());
        assert_eq!(wb.st.borrow().source_items[0].as_ref().unwrap().len(), 2);
        // second item needs 20 total seconds: (int)(10*2 - 10) = 10 -> not yet; 10 more completes
        wb.update_accurate_dt_is_simulation_impl(&me, 5.0, 5.0, false);
        assert_eq!(wb.count_left(), 1);
        wb.update_accurate_dt_is_simulation_impl(&me, 5.0, 5.0, false);
        assert_eq!(wb.count_left(), 0);
        assert!(!me.is_in_use());
        assert!(me.current_blockhead().is_none());
        assert!(f.host.log.borrow().iter().any(|l| l == "craftItemFinished(true)"));
    }

    #[test]
    fn double_time_halves_required_seconds() {
        let f = fixture();
        let (wb, me) = make(&f, 2, 10, 10);
        let bh = blockhead(5, false);
        f.host.stock.borrow_mut().push((20, 2));
        f.host.crafting_any.set(true);
        f.host.double_time.set(true);
        assert!(wb.craft_item_with_blockhead_craft_progress_ui_count(&me, cio(recipe()), &bh, None, 1));
        wb.update_accurate_dt_is_simulation_impl(&me, 5.0, 5.0, false);
        assert!(!me.is_in_use());
    }

    #[test]
    fn hurrying_formula() {
        let f = fixture();
        let (wb, me) = make(&f, 2, 10, 10);
        let bh = blockhead(5, false);
        f.host.stock.borrow_mut().push((20, 2));
        f.host.crafting_any.set(true);
        assert!(wb.craft_item_with_blockhead_craft_progress_ui_count(&me, cio(recipe()), &bh, None, 1));
        wb.update_accurate_dt_is_simulation_impl(&me, 2.0, 2.0, false);
        wb.hurry_completion(&me, 0);
        // cpc = 2 + (10*1*1.0 - 2) * 0.3 = 4.4 (f32 of the double result), not complete
        wb.update_accurate_dt_is_simulation_impl(&me, 0.1, 0.1, false);
        let expect = (2.0f64 + (10.0f64 - 2.0f64) * 0.3f64) as f32;
        assert_eq!(wb.st.borrow().craft_progress_count, expect);
        assert!(me.is_in_use());
    }

    #[test]
    fn paused_or_unable_blockhead_does_not_progress() {
        let f = fixture();
        let (wb, me) = make(&f, 2, 10, 10);
        let bh = blockhead(5, false);
        f.host.stock.borrow_mut().push((20, 2));
        f.host.crafting_any.set(true);
        assert!(wb.craft_item_with_blockhead_craft_progress_ui_count(&me, cio(recipe()), &bh, None, 1));
        f.host.unable.set(3);
        wb.update_accurate_dt_is_simulation_impl(&me, 4.0, 4.0, false);
        assert_eq!(wb.st.borrow().craft_progress_count, 0.0);
        f.host.unable.set(0);
        f.host.paused.set(true);
        wb.update_accurate_dt_is_simulation_impl(&me, 4.0, 4.0, false);
        assert_eq!(wb.st.borrow().craft_progress_count, 0.0);
        assert!(me.is_in_use());
    }

    #[test]
    fn stale_interaction_aborts_craft() {
        let f = fixture();
        let (wb, me) = make(&f, 2, 10, 10);
        let bh = blockhead(5, false);
        f.host.stock.borrow_mut().push((20, 2));
        f.host.crafting_any.set(false);
        assert!(wb.craft_item_with_blockhead_craft_progress_ui_count(&me, cio(recipe()), &bh, None, 1));
        wb.update_accurate_dt_is_simulation_impl(&me, 1.0, 1.0, false);
        assert!(!me.is_in_use());
        assert_eq!(f.deps.log.borrow().iter().filter(|l| l.starts_with("free(10,11,20")).count(), 2);
    }

    #[test]
    fn fuel_burn_while_crafting() {
        let f = fixture();
        let (wb, me) = make(&f, 3, 10, 10); // campfire: fuel, 6.0 per second of coffeeDt
        let bh = blockhead(5, false);
        f.host.stock.borrow_mut().push((20, 2));
        f.host.crafting_any.set(true);
        assert!(wb.craft_item_with_blockhead_craft_progress_ui_count(&me, cio(recipe()), &bh, None, 1));
        wb.st.borrow_mut().fuel_counter = 0.0;
        wb.update_accurate_dt_is_simulation_impl(&me, 1.0, 1.0, false);
        assert!((wb.st.borrow().fuel_fraction - (1.0 - 0.007)).abs() < 1e-6);
        assert!(wb.st.borrow().fuel_counter.abs() < 1e-6);
    }

    #[test]
    fn electricity_burn_requests_refill() {
        let f = fixture();
        let (wb, me) = make(&f, 17, 10, 10); // electric furnace: need 20
        let bh = blockhead(5, false);
        f.host.stock.borrow_mut().push((20, 2));
        f.host.crafting_any.set(true);
        assert!(wb.craft_item_with_blockhead_craft_progress_ui_count(&me, cio(recipe()), &bh, None, 1));
        assert_eq!(wb.available_electricity(), 0);
        f.host.power.set(100);
        wb.st.borrow_mut().fuel_counter = 0.0;
        wb.update_accurate_dt_is_simulation_impl(&me, 1.0, 1.0, false);
        assert!(f.host.log.borrow().iter().any(|l| l == "findPower(100)"));
        assert_eq!(wb.available_electricity(), 100);
    }

    #[test]
    fn campfire_spreads_fire() {
        let f = fixture();
        let (wb, me) = make(&f, 3, 10, 10);
        // tile above is air: not burnable -> no fire; turn it into a tree-ish burnable block (type 9)
        crate::world::tiles::tile_at_world_position_loaded(10, 11, &f.world).unwrap().update(|t| t.type_index = 9);
        let mut placed = false;
        for _ in 0..40 {
            wb.st.borrow_mut().fire_spread_timer = 0.0;
            wb.update_accurate_dt_is_simulation_impl(&me, 0.1, 0.1, false);
            if f.deps.log.borrow().iter().any(|l| l == "fire(10,11)") {
                placed = true;
                break;
            }
        }
        assert!(placed);
    }

    #[test]
    fn save_and_restore_craft_state() {
        let f = fixture();
        let (wb, me) = make(&f, 2, 10, 10);
        let bh = blockhead(5, false);
        f.dw.blockheads.borrow_mut().push(bh.clone());
        f.host.stock.borrow_mut().push((20, 4));
        assert!(wb.craft_item_with_blockhead_craft_progress_ui_count(&me, cio(recipe()), &bh, None, 2));
        let d = wb.get_save_dict_impl(&me);
        assert!(d.map.contains_key(&"craftingItemDatav2".to_string()));
        assert_eq!(d.map.get(&"count".to_string()).unwrap().int_value(), 2);
        match d.map.get(&"sourceItems_0".to_string()) {
            Some(PlistValue::Array(a)) => assert_eq!(a.items.len(), 4),
            _ => panic!("sourceItems_0"),
        }
        assert_eq!(d.map.get(&"currentBlockheadIndex".to_string()).unwrap().int_value(), 0);
        let wb2 = Workbench::init_with_world_dynamic_world_save_dict_cache(&f.world, &f.dwr, &d, None).expect("reload");
        let me2: ObjRef = wb2.clone();
        assert!(wb2.crafting_item_object().is_some());
        assert_eq!((wb2.count(), wb2.count_left()), (2, 2));
        assert_eq!(wb2.st.borrow().source_items[0].as_ref().unwrap().len(), 4);
        // blockheadsLoaded relinks the craft blockhead
        wb2.blockheads_loaded_impl(&me2);
        assert!(wb2.io.borrow().current_blockhead.is_some());
        assert!(me2.is_in_use());
    }

    #[test]
    fn take_ownership_restores_craft() {
        let f = fixture();
        let (wb, me) = make(&f, 2, 10, 10);
        let bh = blockhead(5, false);
        f.host.stock.borrow_mut().push((20, 4));
        assert!(wb.craft_item_with_blockhead_craft_progress_ui_count(&me, cio(recipe()), &bh, None, 2));
        wb.st.borrow_mut().craft_progress_count = 4.0;
        let d = wb.get_save_dict_impl(&me);
        let (wb2, me2) = make(&f, 2, 20, 10);
        let bh2 = blockhead(6, false);
        wb2.blockhead_would_like_to_take_ownership_with_save_dict_impl(&me2, &bh2, Some(&d));
        assert!(me2.is_in_use());
        assert_eq!(wb2.st.borrow().craft_progress_count, 4.0);
        assert_eq!(wb2.count_left(), 2);
        // does nothing when a blockhead is already attached
        let bh3 = blockhead(7, false);
        wb2.blockhead_would_like_to_take_ownership_with_save_dict_impl(&me2, &bh3, Some(&d));
        assert_eq!(wb2.io.borrow().current_blockhead.as_ref().unwrap().unique_id(), 6);
    }

    #[test]
    fn start_managing_fuel_sets_state() {
        let f = fixture();
        let (wb, me) = make(&f, 3, 10, 10);
        let bh = blockhead(5, false);
        wb.start_managing_fuel_with_blockhead(&me, &bh);
        assert!(wb.requires_human_interaction());
        assert!(wb.st.borrow().current_fuel_blockhead.is_some());
        assert!(me.requires_physical_block());
        // stale fuel blockhead (not adding fuel any more) is dropped by update
        wb.update_accurate_dt_is_simulation_impl(&me, 0.1, 0.1, false);
        assert!(!wb.requires_human_interaction());
    }

    #[test]
    fn crystal_wallet_token_scheme() {
        assert_eq!(crystal_balance_checked(), 0, "fresh manager: nil amountString");
        set_crystal_wallet(7);
        assert_eq!(crystal_balance_checked(), 7);
        crystal_spend(3);
        assert_eq!(crystal_amount(), 4);
        assert_eq!(crystal_balance_checked(), 4);
        crystal_refund(3);
        assert_eq!(crystal_amount(), 7);
        crystal_refund(50001); // refused
        assert_eq!(crystal_amount(), 7);
        CrystalManager::instance().modify_modify_string(1, "bad"); // a wrongly signed modify is not applied
        assert_eq!(crystal_amount(), 7);
    }

    #[test]
    fn shim_round_trip() {
        let c = cio(recipe());
        let s = craftable_item_shim(c.clone());
        let back = craftable_item_object_of_shim(&s).unwrap();
        assert_eq!(back.craftable_item(), recipe());
    }

    // ---------------------------------------------------------------------------------------------
    // Tests derived from the specification's decision table
    // ---------------------------------------------------------------------------------------------

    use crate::objects::base::DynamicObject as _;
    use std::rc::Rc;

    /// Bench of `ty` with a running one-unit craft of `recipe()` (host reports "crafting at this bench").
    fn running(f: &Fixture, ty: i32) -> (Rc<Workbench>, ObjRef, ObjRef) {
        let (wb, me) = make(f, ty as u32, 10, 10);
        let bh = blockhead(5, false);
        *f.host.stock.borrow_mut() = vec![(20, 2)];
        f.host.crafting_any.set(true);
        assert!(wb.craft_item_with_blockhead_craft_progress_ui_count(&me, cio(recipe()), &bh, None, 1));
        (wb, me, bh)
    }

    fn tick(wb: &Workbench, me: &ObjRef, dt: f32) {
        wb.update_accurate_dt_is_simulation_impl(me, dt, dt, false);
    }

    fn log_count(f: &Fixture, prefix: &str) -> usize {
        f.deps.log.borrow().iter().filter(|l| l.starts_with(prefix)).count()
    }

    #[test]
    fn helper_tables() {
        assert_eq!(achievement_for(38, 12), Some("grp.bronze"));
        assert_eq!(achievement_for(65, 0), Some("grp.iron"));
        assert_eq!(achievement_for(92, 0), Some("grp.steel"));
        for (t, n) in [
            (94, "amethyst_pickaxe"),
            (95, "sapphire_pickaxe"),
            (96, "emerald_pickaxe"),
            (97, "ruby_pickaxe"),
            (98, "diamond_pickaxe"),
            (90, "gold_pickaxe"),
            (145, "amethyst_chandelier"),
            (146, "sapphire_chandelier"),
            (147, "emerald_chandelier"),
            (148, "ruby_chandelier"),
            (149, "diamond_chandelier"),
            (181, "chef"),
            (182, "chef"),
        ] {
            assert_eq!(achievement_for(t, 0), Some(format!("grp.{}", n)).as_deref());
        }
        assert_eq!(achievement_for(1, 12), Some("grp.dye"));
        assert_eq!(achievement_for(132, 12), None);
        assert_eq!(achievement_for(1, 3), None);
        assert_eq!(power_profile(WB_REFINERY), (40, 2.0, 200));
        assert_eq!(power_profile(0x11), (20, 1.0, 100));
        assert_eq!(power_profile(0x12).0, 20);
        assert_eq!(power_profile(0x1e).0, 20);
        assert_eq!(power_profile(2), (10, 1.0, 100));
        let p = IntPair { x: 5, y: 5 };
        assert_eq!(spread_target(0.9, false, p), Some(IntPair { x: 5, y: 6 }));
        assert_eq!(spread_target(0.3, true, p), Some(IntPair { x: 5, y: 7 }));
        assert_eq!(spread_target(0.3, false, p), None);
        assert_eq!(spread_target(0.15, true, p), Some(IntPair { x: 6, y: 5 }));
        assert_eq!(spread_target(0.05, true, p), Some(IntPair { x: 4, y: 5 }));
        assert_eq!(
            required_len(&{
                let mut c = zero_craftable_item();
                c.number_of_required_items = 99;
                c
            }),
            8
        );
        assert!(!is_net_copy(None));
        assert!(!is_server_controlled(None));
    }

    #[test]
    fn second_craft_while_in_use_is_refused() {
        let f = fixture();
        let (wb, me, bh) = running(&f, 2);
        assert!(!wb.craft_item_with_blockhead_craft_progress_ui_count(&me, cio(recipe()), &bh, None, 1));
    }

    #[test]
    fn upgrade_recipe_with_matching_level_proceeds() {
        let f = fixture();
        let (wb, me) = make(&f, 2, 10, 10);
        let bh = blockhead(5, false);
        let mut ci = recipe();
        ci.crafted_item_type = 132;
        ci.upgrade_level = 2;
        f.host.stock.borrow_mut().push((20, 2));
        assert!(wb.craft_item_with_blockhead_craft_progress_ui_count(&me, cio(ci), &bh, None, 1));
    }

    #[test]
    fn failed_start_refunds_crystals_and_drops_collected_slots() {
        let f = fixture();
        let (wb, me) = make(&f, 2, 10, 10);
        let bh = blockhead(5, false);
        set_crystal_wallet(10);
        let mut ci = recipe();
        ci.number_of_required_items = 3;
        ci.required_items[0] = 11;
        ci.required_item_counts[0] = 2;
        ci.required_items[1] = 20;
        ci.required_item_counts[1] = 2;
        ci.required_items[2] = 21;
        ci.required_item_counts[2] = 1;
        f.host.stock.borrow_mut().push((20, 2));
        assert!(!wb.craft_item_with_blockhead_craft_progress_ui_count(&me, cio(ci), &bh, None, 1));
        assert_eq!(crystal_amount(), 10, "spent then refunded");
        assert_eq!(log_count(&f, "free(10,11,20"), 2);
        assert!(wb.st.borrow().source_items.iter().all(|s| s.is_none()));
    }

    #[test]
    fn zero_price_crystal_entry_needs_no_balance_and_spends_nothing() {
        let f = fixture();
        let (wb, me) = make(&f, 2, 10, 10);
        let bh = blockhead(5, false);
        let mut ci = recipe();
        ci.required_items[0] = 11;
        ci.required_item_counts[0] = 0;
        assert!(wb.craft_item_with_blockhead_craft_progress_ui_count(&me, cio(ci), &bh, None, 1));
        assert_eq!(crystal_amount(), 0);
    }

    #[test]
    fn electric_bench_starts_with_no_stored_power() {
        let f = fixture();
        let (wb, me) = make(&f, 17, 10, 10);
        wb.st.borrow_mut().available_electricity = 55;
        let bh = blockhead(5, false);
        f.host.stock.borrow_mut().push((20, 2));
        assert!(wb.craft_item_with_blockhead_craft_progress_ui_count(&me, cio(recipe()), &bh, None, 1));
        assert_eq!(wb.available_electricity(), 0);
    }

    #[test]
    fn abort_clears_removed_user_and_refunds_crystals() {
        let f = fixture();
        let (wb, me) = make(&f, 2, 10, 10);
        let bh = blockhead(5, false);
        set_crystal_wallet(10);
        let mut ci = recipe();
        ci.required_items[0] = 11;
        ci.required_item_counts[0] = 3;
        assert!(wb.craft_item_with_blockhead_craft_progress_ui_count(&me, cio(ci), &bh, None, 2));
        assert_eq!(crystal_amount(), 4);
        bh.base().borrow_mut().needs_removed = true;
        wb.abort_craft(&me);
        assert_eq!(crystal_amount(), 10);
        assert!(wb.io.borrow().current_blockhead.is_none());
        assert!(!me.is_in_use());
        assert_eq!(wb.fraction_complete(), 0.0);
        // controlled user: no refund
        f.deps.controlled.set(true);
        let (wb2, me2) = make(&f, 2, 20, 10);
        assert!(wb2.craft_item_with_blockhead_craft_progress_ui_count(&me2, cio(ci), &blockhead(6, false), None, 2));
        wb2.abort_craft(&me2);
        assert_eq!(crystal_amount(), 10);
    }

    #[test]
    fn abort_immediately_refunds_crystals_unless_controlled() {
        let f = fixture();
        let (wb, me) = make(&f, 2, 10, 10);
        let bh = blockhead(5, false);
        set_crystal_wallet(10);
        let mut ci = recipe();
        ci.required_items[0] = 11;
        ci.required_item_counts[0] = 3;
        assert!(wb.craft_item_with_blockhead_craft_progress_ui_count(&me, cio(ci), &bh, None, 2));
        wb.abort_immediately_and_restore_blockhead_items(&me);
        assert_eq!(crystal_amount(), 10);
        assert!(f.host.log.borrow().iter().any(|l| l == "craftItemFinished(false)"));
        // in use but without a user: nothing happens
        let (wb2, me2) = make(&f, 2, 20, 10);
        wb2.set_in_use(true);
        let before = f.host.log.borrow().len();
        wb2.abort_immediately_and_restore_blockhead_items(&me2);
        assert_eq!(f.host.log.borrow().len(), before);
        // controlled user
        f.deps.controlled.set(true);
        let (wb3, me3) = make(&f, 2, 30, 10);
        assert!(wb3.craft_item_with_blockhead_craft_progress_ui_count(&me3, cio(ci), &bh, None, 2));
        assert_eq!(crystal_amount(), 10);
        wb3.abort_immediately_and_restore_blockhead_items(&me3);
        assert_eq!(crystal_amount(), 10);
    }

    #[test]
    fn complete_without_user_aborts() {
        let f = fixture();
        let (wb, me) = make(&f, 2, 10, 10);
        wb.st.borrow_mut().crafting_item_object = Some(cio(recipe()));
        wb.craft_completed(&me);
        assert_eq!(wb.fraction_complete(), 0.0);
        // user marked for removal
        let (wb, me, bh) = running(&f, 2);
        bh.base().borrow_mut().needs_removed = true;
        wb.craft_completed(&me);
        assert!(!me.is_in_use());
    }

    #[test]
    fn complete_preserves_data_a_and_reports_pig_iron() {
        let f = fixture();
        let (wb, me) = make(&f, 2, 10, 10);
        let bh = blockhead(5, false);
        let mut ci = recipe();
        ci.crafted_item_type = 65;
        ci.required_items[0] = 201;
        ci.required_item_counts[0] = 1;
        ci.number_of_required_items = 2;
        ci.required_items[1] = 84; // preserves data A
        ci.required_item_counts[1] = 1;
        f.host.stock.borrow_mut().push((201, 1));
        f.host.stock.borrow_mut().push((84, 1));
        let c = cio(ci);
        assert!(wb.craft_item_with_blockhead_craft_progress_ui_count(&me, c.clone(), &bh, None, 1));
        wb.st.borrow_mut().source_items[1].as_mut().unwrap()[0].borrow_mut().data_a = 9;
        wb.craft_completed(&me);
        assert_eq!(c.craftable_item().crafted_item_data_a, 9);
        assert!(!me.is_in_use());
        assert_eq!(wb.fraction_complete(), 1.0);
    }

    #[test]
    fn complete_with_units_left_keeps_running_and_controlled_skips_achievements() {
        let f = fixture();
        let (wb, me) = make(&f, 12, 10, 10);
        let bh = blockhead(5, false);
        f.host.stock.borrow_mut().push((20, 4));
        assert!(wb.craft_item_with_blockhead_craft_progress_ui_count(&me, cio(recipe()), &bh, None, 2));
        wb.craft_completed(&me);
        assert_eq!(wb.count_left(), 1);
        assert!(me.is_in_use());
        f.deps.controlled.set(true);
        wb.craft_completed(&me);
        assert_eq!(wb.count_left(), 0);
        assert!(!me.is_in_use());
    }

    #[test]
    fn complete_without_recipe_object_uses_empty_recipe() {
        let f = fixture();
        let (wb, me) = make(&f, 2, 10, 10);
        wb.io.borrow_mut().current_blockhead = Some(blockhead(5, false));
        wb.set_in_use(true);
        wb.craft_completed(&me);
        assert_eq!(wb.count_left(), -1);
    }

    #[test]
    fn hurried_completion_finishes_every_unit_and_charges_cost() {
        let f = fixture();
        let (wb, me) = make(&f, 2, 10, 10);
        let bh = blockhead(5, false);
        set_crystal_wallet(20);
        f.host.stock.borrow_mut().push((20, 6));
        assert!(wb.craft_item_with_blockhead_craft_progress_ui_count(&me, cio(recipe()), &bh, None, 3));
        wb.hurry_completion(&me, 7);
        wb.craft_completed(&me);
        assert_eq!(wb.count_left(), 0);
        assert_eq!(wb.st.borrow().count_created, 3);
        assert!(!wb.st.borrow().hurrying);
        assert_eq!(crystal_amount(), 13);
        // free hurry
        let (wb2, me2) = make(&f, 2, 20, 10);
        *f.host.stock.borrow_mut() = vec![(20, 2)];
        assert!(wb2.craft_item_with_blockhead_craft_progress_ui_count(&me2, cio(recipe()), &bh, None, 1));
        wb2.hurry_completion(&me2, 0);
        wb2.craft_completed(&me2);
        assert_eq!(crystal_amount(), 13);
    }

    #[test]
    fn hurry_burst_draws_112_values_once() {
        let f = fixture();
        let (wb, me) = make(&f, 2, 10, 10);
        assert_eq!(draws_during(|| wb.hurry_completion(&me, 3)), 112);
        assert_eq!(wb.st.borrow().hurry_cost, 3);
        assert_eq!(draws_during(|| wb.hurry_completion(&me, 9)), 0);
        assert_eq!(wb.st.borrow().hurry_cost, 3);
    }

    #[test]
    fn queries_are_zero_without_user_or_recipe() {
        let f = fixture();
        let (wb, _me) = make(&f, 2, 10, 10);
        assert_eq!((wb.total_items_left_to_craft(), wb.currently_crafting_item_type()), (0, ItemType(0)));
        wb.set_in_use(true);
        wb.io.borrow_mut().current_blockhead = Some(blockhead(5, false));
        assert_eq!(wb.total_items_left_to_craft(), 0, "no recipe");
        assert_eq!(wb.currently_crafting_item_type(), ItemType(0));
    }

    #[test]
    fn fuel_management_replaces_previous_user() {
        let f = fixture();
        let (wb, me) = make(&f, 3, 10, 10);
        let a = blockhead(5, false);
        let b = blockhead(6, false);
        wb.start_managing_fuel_with_blockhead(&me, &a);
        wb.start_managing_fuel_with_blockhead(&me, &a);
        wb.start_managing_fuel_with_blockhead(&me, &b);
        assert_eq!(wb.st.borrow().current_fuel_blockhead.as_ref().unwrap().unique_id(), 6);
    }

    #[test]
    fn removed_users_are_forgotten_by_the_tick() {
        let f = fixture();
        let (wb, me, bh) = running(&f, 2);
        let fu = blockhead(8, false);
        wb.start_managing_fuel_with_blockhead(&me, &fu);
        bh.base().borrow_mut().needs_removed = true;
        fu.base().borrow_mut().needs_removed = true;
        tick(&wb, &me, 0.1);
        assert!(!me.is_in_use());
        assert!(wb.io.borrow().current_blockhead.is_none());
        assert!(wb.st.borrow().current_fuel_blockhead.is_none());
        assert!(!wb.st.borrow().is_in_use_fuel);
        // a removed bench does nothing at all
        me.base().borrow_mut().needs_removed = true;
        let before = f.dw.changed.borrow().len();
        tick(&wb, &me, 0.1);
        assert_eq!(f.dw.changed.borrow().len(), before);
    }

    #[test]
    fn coffee_speeds_up_work() {
        let f = fixture();
        let (wb, me, _bh) = running(&f, 2);
        f.host.coffee.set(true);
        tick(&wb, &me, 2.0);
        assert_eq!(wb.st.borrow().craft_progress_count, 3.0);
    }

    #[test]
    fn simulation_tick_skips_users_that_should_not_continue() {
        let f = fixture();
        let (wb, me, _bh) = running(&f, 3);
        f.host.should_continue.set(false);
        wb.st.borrow_mut().fuel_counter = 0.0;
        wb.update_accurate_dt_is_simulation_impl(&me, 1.0, 1.0, true);
        assert_eq!(wb.st.borrow().craft_progress_count, 0.0);
        assert_eq!(wb.st.borrow().fuel_counter, 0.0);
        wb.update_accurate_dt_is_simulation_impl(&me, 1.0, 1.0, false);
        assert_eq!(wb.st.borrow().craft_progress_count, 1.0);
    }

    #[test]
    fn fuel_burn_rates_and_blockers() {
        let f = fixture();
        // generic bench with fuel: rate 1, so 1 second adds 1
        let (wb, me, _bh) = running(&f, 4);
        wb.st.borrow_mut().has_fuel = true;
        wb.st.borrow_mut().fuel_fraction = 1.0;
        wb.st.borrow_mut().fuel_counter = 0.0;
        tick(&wb, &me, 1.0);
        assert_eq!(wb.st.borrow().fuel_counter, 1.0);
        // unable-to-work user burns nothing
        f.host.unable.set(1);
        tick(&wb, &me, 1.0);
        assert_eq!(wb.st.borrow().fuel_counter, 1.0);
        f.host.unable.set(0);
        // paused user burns nothing
        f.host.paused.set(true);
        tick(&wb, &me, 1.0);
        assert_eq!(wb.st.borrow().fuel_counter, 1.0);
    }

    #[test]
    fn fuel_burn_skipped_for_network_user() {
        let f = fixture();
        let (wb, me) = make(&f, 4, 10, 10);
        wb.st.borrow_mut().has_fuel = true;
        wb.st.borrow_mut().fuel_fraction = 1.0;
        wb.st.borrow_mut().fuel_counter = 2.0;
        wb.set_in_use(true);
        wb.io.borrow_mut().current_blockhead = Some(blockhead(5, true));
        tick(&wb, &me, 3.0);
        assert_eq!(wb.st.borrow().fuel_counter, 2.0);
    }

    #[test]
    fn electricity_use_cases() {
        let f = fixture();
        // enough charge left: no refill request
        let (wb, me, _bh) = running(&f, 17);
        wb.st.borrow_mut().available_electricity = 90;
        wb.st.borrow_mut().fuel_counter = 0.0;
        tick(&wb, &me, 1.0);
        assert_eq!(wb.available_electricity(), 70);
        assert!(!f.host.log.borrow().iter().any(|l| l.starts_with("findPower")));
        // low and the network has nothing: stays empty
        wb.st.borrow_mut().available_electricity = 10;
        wb.st.borrow_mut().fuel_counter = 0.0;
        tick(&wb, &me, 1.0);
        assert_eq!(wb.available_electricity(), 0);
        assert!(f.host.log.borrow().iter().any(|l| l == "findPower(100)"));
        // counter below one: nothing is consumed
        wb.st.borrow_mut().available_electricity = 50;
        wb.st.borrow_mut().fuel_counter = 0.0;
        tick(&wb, &me, 0.5);
        assert_eq!(wb.available_electricity(), 50);
        // blocked users draw nothing
        f.host.unable.set(2);
        wb.st.borrow_mut().fuel_counter = 0.9;
        tick(&wb, &me, 1.0);
        assert_eq!(wb.available_electricity(), 50);
        f.host.unable.set(0);
        f.host.paused.set(true);
        tick(&wb, &me, 1.0);
        assert_eq!(wb.available_electricity(), 50);
    }

    #[test]
    fn refinery_doubles_rate_and_refills_to_200() {
        let f = fixture();
        let (wb, me, _bh) = running(&f, WB_REFINERY);
        f.host.power.set(500);
        wb.st.borrow_mut().fuel_counter = 0.0;
        tick(&wb, &me, 0.5);
        assert!(f.host.log.borrow().iter().any(|l| l == "findPower(200)"));
        assert_eq!(wb.available_electricity(), 200);
        // network copy user draws nothing
        let (wb2, me2) = make(&f, 17, 20, 10);
        wb2.st.borrow_mut().available_electricity = 50;
        wb2.set_in_use(true);
        wb2.io.borrow_mut().current_blockhead = Some(blockhead(9, true));
        tick(&wb2, &me2, 5.0);
        assert_eq!(wb2.available_electricity(), 50);
    }

    #[test]
    fn progress_requires_resources_and_attachment() {
        let f = fixture();
        // fuel bench without fuel: no progress
        let (wb, me, _bh) = running(&f, 3);
        wb.st.borrow_mut().has_fuel = false;
        tick(&wb, &me, 1.0);
        assert_eq!(wb.st.borrow().craft_progress_count, 0.0);
        // electric bench without power: no progress
        let (wb2, me2, _bh2) = running(&f, 17);
        f.host.power.set(0);
        tick(&wb2, &me2, 1.0);
        assert_eq!(wb2.st.borrow().craft_progress_count, 0.0);
        // user attached to another workbench: abort ends the tick
        let (wb3, me3, _bh3) = running(&f, 2);
        *f.host.iw.borrow_mut() = None;
        tick(&wb3, &me3, 1.0);
        assert!(!me3.is_in_use());
    }

    #[test]
    fn network_user_is_left_alone() {
        let f = fixture();
        let (wb, me) = make(&f, 2, 10, 10);
        wb.set_in_use(true);
        wb.io.borrow_mut().current_blockhead = Some(blockhead(5, true));
        f.host.crafting_any.set(false);
        tick(&wb, &me, 1.0);
        assert!(me.is_in_use());
        assert_eq!(wb.st.borrow().craft_progress_count, 0.0);
    }

    #[test]
    fn stale_craft_user_is_released_when_not_working() {
        let f = fixture();
        let (wb, me, _bh) = running(&f, 2);
        f.host.paused.set(true);
        f.host.crafting_any.set(false);
        tick(&wb, &me, 1.0);
        assert!(!me.is_in_use());
        assert!(wb.io.borrow().current_blockhead.is_none());
    }

    #[test]
    fn fuel_user_stays_while_adding_fuel_here() {
        let f = fixture();
        let (wb, me) = make(&f, 3, 10, 10);
        let fu = blockhead(5, false);
        wb.start_managing_fuel_with_blockhead(&me, &fu);
        f.host.adding_fuel.set(true);
        tick(&wb, &me, 0.1);
        assert!(wb.st.borrow().is_in_use_fuel);
        // adding fuel elsewhere: dropped
        *f.host.iw.borrow_mut() = None;
        tick(&wb, &me, 0.1);
        assert!(!wb.st.borrow().is_in_use_fuel);
        // network fuel user is left alone
        let (wb2, me2) = make(&f, 3, 20, 10);
        wb2.start_managing_fuel_with_blockhead(&me2, &blockhead(6, true));
        f.host.adding_fuel.set(false);
        tick(&wb2, &me2, 0.1);
        assert!(wb2.st.borrow().is_in_use_fuel);
    }

    fn sunlit(f: &Fixture) {
        for y in 8..16 {
            for x in 8..13 {
                if let Some(t) = crate::world::tiles::tile_at_world_position_loaded(x, y, &f.world) {
                    t.update(|t| {
                        t.sun_light = 255;
                        t.contents = 0;
                    });
                }
            }
        }
    }

    #[test]
    fn solar_panel_charges_in_four_second_steps() {
        let f = fixture();
        sunlit(&f);
        let (wb, me) = make(&f, WB_SOLAR_PANEL as u32, 10, 10);
        let daytime = (0..36).map(|i| f64::from(i) * 25.0).find(|t| {
            f.world.set_world_time(*t);
            wb.combined_light_for_solar_panel() >= 0.1
        });
        assert!(daytime.is_some(), "some time of day lights the panel");
        wb.st.borrow_mut().available_electricity = 0;
        wb.st.borrow_mut().fuel_counter = 0.0;
        tick(&wb, &me, 3.0);
        assert_eq!(wb.available_electricity(), 0, "not yet");
        tick(&wb, &me, 2.0);
        assert!(wb.available_electricity() > 0);
        assert!((wb.st.borrow().fuel_counter - 1.0).abs() < 1e-6);
        // full panels stop charging
        wb.st.borrow_mut().available_electricity = 100;
        let before = wb.st.borrow().fuel_counter;
        tick(&wb, &me, 9.0);
        assert_eq!(wb.st.borrow().fuel_counter, before);
        // a dark panel gains nothing
        let (wb2, me2) = make(&f, WB_SOLAR_PANEL as u32, 40, 10);
        wb2.st.borrow_mut().available_electricity = 0;
        wb2.st.borrow_mut().fuel_counter = 0.0;
        tick(&wb2, &me2, 5.0);
        assert_eq!(wb2.available_electricity(), 0);
    }

    #[test]
    fn flywheel_pulls_power_every_five_seconds() {
        let f = fixture();
        let (wb, me) = make(&f, WB_FLYWHEEL as u32, 10, 10);
        wb.st.borrow_mut().fuel_counter = 0.0;
        tick(&wb, &me, 4.0);
        assert!(!f.host.log.borrow().iter().any(|l| l.starts_with("findPower")));
        tick(&wb, &me, 2.0);
        assert!(f.host.log.borrow().iter().any(|l| l == "findPower(8192)"));
        assert_eq!(wb.st.borrow().time_since_found_electricty, 5.0);
        f.host.power.set(300);
        tick(&wb, &me, 6.0);
        assert_eq!(wb.available_electricity(), 300);
        assert_eq!(wb.st.borrow().time_since_found_electricty, 0.0);
        // full flywheel does nothing
        wb.st.borrow_mut().available_electricity = 0x2000;
        let n = f.host.log.borrow().len();
        tick(&wb, &me, 6.0);
        assert_eq!(f.host.log.borrow().len(), n);
    }

    #[test]
    fn steam_generator_converts_fuel_to_power() {
        let f = fixture();
        let (wb, me) = make(&f, WB_STEAM_GENERATOR as u32, 10, 10);
        wb.st.borrow_mut().fuel_fraction = 0.5;
        wb.st.borrow_mut().available_electricity = 0;
        tick(&wb, &me, 0.1);
        assert_eq!(wb.available_electricity(), 100);
        let ff = wb.st.borrow().fuel_fraction;
        assert!(ff < 0.5 && ff > 0.4, "{}", ff);
        assert!(wb.st.borrow().particle_create_timer_steam_engine <= 4.0);
        // too much fuel is clamped, and a charged generator does not burn
        wb.st.borrow_mut().fuel_fraction = 3.0;
        tick(&wb, &me, 0.1);
        assert_eq!(wb.st.borrow().fuel_fraction, 1.0);
        // empty tank: nothing happens
        wb.st.borrow_mut().fuel_fraction = 0.0;
        wb.st.borrow_mut().available_electricity = 0;
        tick(&wb, &me, 0.1);
        assert_eq!(wb.available_electricity(), 0);
        // in use: no generation
        wb.st.borrow_mut().fuel_fraction = 0.5;
        wb.set_in_use(true);
        tick(&wb, &me, 0.1);
        assert_eq!(wb.available_electricity(), 0);
    }

    #[test]
    fn network_copy_runs_no_generator() {
        let f = fixture();
        let (wb, me) = make(&f, WB_STEAM_GENERATOR as u32, 10, 10);
        me.base().borrow_mut().is_net = true;
        wb.st.borrow_mut().available_electricity = 0;
        wb.st.borrow_mut().fuel_fraction = 0.5;
        tick(&wb, &me, 0.1);
        assert_eq!(wb.available_electricity(), 0);
    }

    #[test]
    fn campfire_in_water_loses_its_fuel() {
        let f = fixture();
        let (wb, me) = make(&f, 3, 10, 10);
        crate::world::tiles::tile_at_world_position_loaded(10, 10, &f.world).unwrap().update(|t| t.type_index = 3);
        wb.st.borrow_mut().has_fuel = true;
        wb.st.borrow_mut().fuel_fraction = 0.8;
        tick(&wb, &me, 0.1);
        assert!(!wb.st.borrow().has_fuel);
        assert_eq!(wb.st.borrow().fuel_fraction, 0.0);
    }

    #[test]
    fn campfire_spread_timer_and_draw_order() {
        let f = fixture();
        let (wb, me) = make(&f, 3, 10, 10);
        // unlit fires count down at 5 percent speed
        wb.st.borrow_mut().has_fuel = false;
        wb.st.borrow_mut().fire_spread_timer = 10.0;
        let n = draws_during(|| tick(&wb, &me, 10.0));
        assert_eq!(n, 0);
        assert!((wb.st.borrow().fire_spread_timer - 9.5).abs() < 1e-5);
        // expiry draws the timer first, then the direction
        wb.st.borrow_mut().fire_spread_timer = 0.0;
        let n = draws_during(|| tick(&wb, &me, 0.1));
        assert_eq!(n, 2);
        let t = wb.st.borrow().fire_spread_timer;
        assert!((30.0..50.0).contains(&t));
        // every direction (including "no target") is reachable
        for tag in 0..300 {
            crate::rng::glibc_rand::srand(tag);
            wb.st.borrow_mut().fire_spread_timer = 0.0;
            tick(&wb, &me, 0.1);
        }
        // fuelled fires burn on at full speed and may reach two cells up
        wb.st.borrow_mut().has_fuel = true;
        wb.st.borrow_mut().fuel_fraction = 1.0;
        for tag in 0..300 {
            crate::rng::glibc_rand::srand(tag);
            wb.st.borrow_mut().fire_spread_timer = 0.0;
            tick(&wb, &me, 0.1);
        }
    }

    #[test]
    fn complete_stops_consuming_when_a_slot_runs_dry() {
        let f = fixture();
        let (wb, me, _bh) = running(&f, 2);
        wb.st.borrow_mut().source_items[0] = Some(vec![]);
        wb.craft_completed(&me);
        assert_eq!(wb.count_left(), 0);
        wb.st.borrow_mut().source_items[0] = None;
        wb.st.borrow_mut().count_left = 1;
        wb.io.borrow_mut().current_blockhead = Some(blockhead(5, false));
        wb.craft_completed(&me);
        assert_eq!(wb.count_left(), 0);
    }
}
