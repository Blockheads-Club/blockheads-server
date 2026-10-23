//! `-[Blockhead update:accurateDT:isSimulation:]`, phases U1..U8.
//!
//! `#### U1`..`#### U8` and "Static helpers".
//!
//! `(d)` in the spec means the expression is evaluated in f64 and narrowed on store; unmarked arithmetic is f32.
//! `R(x)` is `(float)rand() / 2147483648.0f`. `simOrCtl = isSimulation || isClientBlockheadBeingControlledByServer`.
//! Every `uiManager`, sound, `activeBlockhead == self`, d-pad and `isClient` branch is CLIENT-ONLY (dead on the
//! server); the `libc rand()` calls that build particle arguments are executed.

use std::rc::Rc;

use crate::fmath::cconv::{f32_to_i32, f64_to_i32};
use crate::fmath::libm::{powf, sinf};
use crate::fmath::vector::{clamp, linear_interpolate};
use crate::items::misc::default_data_a_and_b_for_item_type;
use crate::items::misc::{workbench_requires_electricty, workbench_type_for_item_type};
use crate::items::predicates::{
    interaction_type_disallowed_on_protected_tiles, item_type_is_full_cage, item_type_is_interaction_object,
    item_type_is_metal, item_type_is_painting, item_type_is_workbench,
};
use crate::nslog;
use crate::objects::base::{DynamicObject, WorldApi};
use crate::objects::handle::ObjRef;
use crate::path::can_enter::{check_can_enter_tile, FromProps};
use crate::plist::value::PlistValue;
use crate::rng::glibc_rand::{r01_from, rand_tagged};
use crate::types::enums::{InteractionType, WorkbenchType};
use crate::types::item_type::ItemType;
use crate::types::structs::{CustomRules, IntPair, Vector2};
use crate::world::helpers::{
    tile_contains_door, tile_contains_gate, tile_contains_trap_door, tile_is_bush_or_scrub, tile_is_paintable,
    tile_is_plant, tile_is_solid, tile_is_tree_trunk, tile_is_unplaced_block_minable_with_pickaxe, tile_is_water,
};
use crate::world::loot::{random_bonus_item_type_for_tile, random_meditation_bonus};
use crate::world::state::{TileRef, World};
use crate::world::tiles::{
    clear_line_of_sight_between_tiles, get_world_pos_for_world_index, tile_at_world_index_loaded,
    tile_at_world_position_loaded, world_index_at_world_pos,
};

use super::blockhead::{
    host, it, kf, state_asleep, state_meditating, tt, va, vsa, Blockhead, BlockheadHost, ExtraValue, PathExtraData,
    SimulationEventExtra,
};

const M_PI: f64 = 3.141592653589793;

fn tile_at(world: &World, p: IntPair) -> Option<TileRef> {
    tile_at_world_position_loaded(p.x, p.y, world)
}

/// `a < b ? a: b` in f64 (the spec's `min`).
fn min_d(a: f64, b: f64) -> f64 {
    if a < b {
        a
    } else {
        b
    }
}
/// `a < b ? b: a` in f64 (the spec's `max`).
fn max_d(a: f64, b: f64) -> f64 {
    if a < b {
        b
    } else {
        a
    }
}
fn min_f(a: f32, b: f32) -> f32 {
    if a < b {
        a
    } else {
        b
    }
}
fn max_f(a: f32, b: f32) -> f32 {
    if a < b {
        b
    } else {
        a
    }
}

/// `particleColorForTile(tile).a > 0.5f`: true iff the tile is bush/scrub/plant, a tree trunk, or
/// `1 <= typeIndex <= 77` with typeIndex not in {2,21,22,23,25,31,33..47,60..66}.
/// 5 fact 5. `None` is UB in the original
/// (UB-PARITY): treated as `false`.
pub fn particle_color_alpha_is_visible(tile: Option<TileRef>) -> bool {
    let Some(t) = tile else { return false };
    if tile_is_bush_or_scrub(Some(t)) || tile_is_plant(Some(t)) || tile_is_tree_trunk(Some(t)) {
        return true;
    }
    let ti = t.type_index();
    if !(1..=77).contains(&ti) {
        return false;
    }
    !matches!(ti, 2 | 21 | 22 | 23 | 25 | 31 | 33..=47 | 60..=66)
}

enum Flow {
    Continue,
    Return,
}

struct Ctx<'a> {
    b: &'a Blockhead,
    me: &'a ObjRef,
    h: Rc<dyn BlockheadHost>,
    world: Option<Rc<World>>,
    rules: CustomRules,
    dt: f32,
    adt: f32,
    sim: bool,
    ctl: bool,
    sim_or_ctl: bool,
    is_net: bool,
    expert: bool,
}

impl<'a> Ctx<'a> {
    fn rules_on(&self) -> bool {
        self.rules.custom_rules_enabled != 0
    }
    fn event(&self, ty: i32, extra: SimulationEventExtra) {
        self.h.add_simulation_event_of_type_for_blockhead_extra_data(ty, self.me, extra);
    }
    fn killed_or(&self, other: &str) -> String {
        if self.b.bh.borrow().state.regenerating != 0 {
            "killed".to_string()
        } else {
            other.to_string()
        }
    }
    fn achievement(&self, id: &str) {
        self.h.report_achievement_with_identifier(id);
    }
    /// The `if (!isNet && !isClientBlockheadBeingControlledByServer)` guard of the achievements.
    fn guarded_achievement(&self, id: &str) {
        if !self.is_net && !self.ctl {
            self.achievement(id);
        }
    }
    fn pos(&self) -> IntPair {
        self.b.pos()
    }
    fn w_tiles(&self) -> i32 {
        self.world.as_ref().map_or(0, |w| w.world_width_macro().wrapping_mul(32))
    }
    /// `wrapdx(d)` with C integer division.
    fn wrapdx(&self, d: i32) -> i32 {
        let w = self.w_tiles();
        if d < w / 2 {
            if d < (-w) / 2 {
                d.wrapping_add(w)
            } else {
                d
            }
        } else {
            d.wrapping_sub(w)
        }
    }
    fn create_free_block(&self, pos: IntPair, ty: u32, a: u16, b: u16, play_sound: i32, priority: bool) {
        self.h.create_free_block_at_position_of_type_data_a_data_b_sub_items_dynamic_object_save_dict_hovers_play_sound_priority_blockhead(
            pos,
            ItemType(ty),
            a,
            b,
            None,
            None,
            false,
            play_sound,
            if priority { Some(self.me) } else { None },
        );
    }
    fn local_net_id(&self) -> Option<String> {
        crate::objects::base::BlockheadApi::local_net_id(self.b)
    }
    fn tile_protected(&self, pos: IntPair) -> bool {
        self.world.as_ref().map_or(false, |w| WorldApi::tile_is_protected_at_pos_against_blockhead(&**w, pos, self.me))
    }
    /// `interactionTypeForTile:...` with the protected-tile recheck (`allowProtectedActions:NO`).
    fn interaction_type_checked(
        &self,
        tile: Option<TileRef>,
        pos: IntPair,
        item: Option<&crate::items::inventory_item::ItemRef>,
        face: i32,
    ) -> u32 {
        let mut rejected = false;
        let mut cit = self
            .h
            .interaction_type_for_tile_at_pos_item_pickup_rejected_due_to_inventory_full_include_actions_face_index_allow_protected_actions(
                self.b, tile, pos, item, &mut rejected, false, face, true,
            );
        if interaction_type_disallowed_on_protected_tiles(InteractionType(cit)) && self.tile_protected(pos) {
            cit = self
                .h
                .interaction_type_for_tile_at_pos_item_pickup_rejected_due_to_inventory_full_include_actions_face_index_allow_protected_actions(
                    self.b, tile, pos, item, &mut rejected, false, face, false,
                );
        }
        cit
    }
    fn extra_face(&self) -> i32 {
        self.b.bh.borrow().path_extra_data.as_ref().map_or(0, |e| e.int_for_key("craftCountOrExtraData"))
    }
    /// The common damage-apply routine `D(decrement)`.
    fn damage_apply(&self, decrement: f32) {
        let mut die_now = false;
        {
            let mut f = self.b.bh.borrow_mut();
            if 0.0f32 < f.state.health - decrement * 0.5f32 {
                f.state.health -= decrement;
                if f.state.health <= 0.0f32 {
                    f.state.health = 0.0;
                }
            } else {
                if !(self.rules_on() && self.rules.death == 0) {
                    f.state.death += decrement;
                    if 1.0f32 <= f.state.death {
                        die_now = true;
                    }
                }
                if !die_now {
                    f.state.health = 0.0;
                }
            }
        }
        if die_now {
            // `[self die]` happens before `health = 0`
            self.b.die();
            self.b.bh.borrow_mut().state.health = 0.0;
        }
    }
    /// rules.health modifiers of a damage amount (0 -> 99999.9f, 1 -> *10, 3 -> *0.1).
    fn health_rule_modified(&self, d: f32) -> f32 {
        let mut d = d;
        if self.rules_on() {
            match self.rules.health {
                0 => d = 99999.9f32,
                1 => d *= 10.0f32,
                3 => d *= 0.1f32,
                _ => {}
            }
        }
        d
    }
}

/// `-[Blockhead update:accurateDT:isSimulation:]`.
/// Three `Return`s inside U3 (`pathNeedsRecalculated`, `waitingForPath`, plus the U2 early return) skip U4-U8
/// INCLUDING the tail `prepareInventoryForSaving`.
pub fn update_accurate_dt_is_simulation(b: &Blockhead, me: &ObjRef, dt: f32, accurate_dt: f32, is_simulation: bool) {
    let world = b.world_rc();
    let ctl = b.is_client_blockhead_being_controlled_by_server();
    let rules = world.as_ref().map(|w| w.custom_rules()).unwrap_or_default();
    let expert = world.as_ref().map_or(false, |w| w.expert_mode());
    let c = Ctx {
        b,
        me,
        h: host(),
        world,
        rules,
        dt,
        adt: accurate_dt,
        sim: is_simulation,
        ctl,
        sim_or_ctl: is_simulation || ctl,
        is_net: b.is_net(),
        expert,
    };
    let mut should_collapse = false;
    if let Flow::Return = u1(&c, &mut should_collapse) {
        return;
    }
    if let Flow::Return = u2(&c) {
        return;
    }
    if let Flow::Return = u3(&c, &mut should_collapse) {
        return;
    }
    u4(&c);
    u5(&c);
    u6_u7(&c, should_collapse);
    u8(&c);
}

// ====================================================================================================
// U1. Pre-checks and the controlled-by-server watchdog (Blockhead.mm 6470..6534)
// ====================================================================================================

fn u1(c: &Ctx, should_collapse: &mut bool) -> Flow {
    let b = c.b;
    // 1.
    {
        let (interacting, qlen) = {
            let f = b.bh.borrow();
            (f.state.interacting != 0, f.action_queue.len())
        };
        if !c.is_net && interacting && qlen == 0 {
            nslog!("interacting with zero length queue");
            b.stop_interacting();
        }
    }
    // 2.
    *should_collapse = false;
    // 3.
    if b.bh.borrow().paused && !c.sim {
        return Flow::Return;
    }
    // 4.
    if !c.sim {
        if !c.ctl {
            b.bh.borrow_mut().cancel_simulate_due_to_collapse = false;
        } else {
            let sanity = {
                let mut f = b.bh.borrow_mut();
                f.controlled_by_server_sanity_check_timeout += c.dt;
                f.controlled_by_server_sanity_check_timeout
            };
            if 7200.0f32 < sanity {
                DynamicObject::set_needs_removed(b, c.me, true);
            }
            if !b.should_continue_simulating() {
                let (itype, wb) = {
                    let f = b.bh.borrow();
                    (f.state.interaction_type, f.interaction_workbench.clone())
                };
                if itype == it::CRAFT {
                    if let Some(wb) = wb {
                        let ok = c.h.workbench_has_required_fuel(&wb) && c.h.unable_to_work_reason(b) == 0;
                        if !ok {
                            c.h.workbench_abort_immediately_and_restore_blockhead_items(&wb);
                        }
                    }
                }
                let t = {
                    let mut f = b.bh.borrow_mut();
                    f.controlled_by_server_inactivity_timeout += c.dt;
                    f.controlled_by_server_inactivity_timeout
                };
                if 4.0f32 < t {
                    DynamicObject::set_needs_removed(b, c.me, true);
                }
            } else {
                let mut f = b.bh.borrow_mut();
                f.controlled_by_server_inactivity_timeout = 0.0;
                f.cancel_simulate_due_to_collapse = false;
            }
        }
    } else {
        // 5.
        if !b.should_continue_simulating() {
            let t = {
                let mut f = b.bh.borrow_mut();
                f.controlled_by_server_inactivity_timeout += c.dt;
                f.controlled_by_server_inactivity_timeout
            };
            if 4.0f32 < t {
                return Flow::Return;
            }
        } else {
            let mut f = b.bh.borrow_mut();
            f.controlled_by_server_inactivity_timeout = 0.0;
            f.cancel_simulate_due_to_collapse = false;
        }
    }
    // 6.
    {
        let mut f = b.bh.borrow_mut();
        if !f.debug_bool {
            f.debug_bool = true;
        }
    }
    // 7.
    if b.base.borrow().needs_removed {
        return Flow::Return;
    }
    if b.bh.borrow().state.on_trade_mission != 0 {
        return Flow::Return;
    }
    Flow::Continue
}

// ====================================================================================================
// U2. Regeneration (6590..6669): runs only when state.regenerating, and always returns
// ====================================================================================================

fn u2(c: &Ctx) -> Flow {
    let b = c.b;
    if b.bh.borrow().state.regenerating == 0 {
        return Flow::Continue;
    }
    let mut a = c.dt;
    if c.world.as_ref().map_or(false, |w| w.fast_forward()) {
        a = c.dt / 20.0f32;
    }
    let prog = {
        let mut f = b.bh.borrow_mut();
        f.state.regeneration_progress += a / 600.0f32;
        f.state.regeneration_progress
    };
    if prog <= 1.0f32 {
        return Flow::Return;
    }
    if c.is_net {
        return Flow::Return;
    }
    if c.sim_or_ctl {
        c.event(4, SimulationEventExtra::Text("regenerated and teleported back to the portal".to_string()));
    }
    if !c.is_net && !c.ctl {
        c.achievement("grp.death");
    }
    {
        let mut f = b.bh.borrow_mut();
        f.state.regeneration_progress = 0.0;
        f.state.regenerating = 0;
    }
    if let Some(dw) = b.dw() {
        dw.dynamic_world_changed_at_pos_object_type(b.pos(), DynamicObject::object_type(b));
    }
    {
        let mut f = b.bh.borrow_mut();
        let s = &mut f.state;
        s.health = 1.0;
        s.death = 0.0;
        s.drown_fraction = 0.0;
        s.energy = 1.0;
        s.fullness = 1.0;
        s.happiness = 1.0;
        s.environment = 1.0;
        s.environment_temperature = 1.0;
        s.environment_exposure = 1.0;
        s.environment_light = 1.0;
        s.animation_type = 0;
        s.heat = 0.0;
        s.coffee_energy_timer = 0.0;
        s.food_pause_timer = 0.0;
    }
    // portalPos stays (0,0) if the world is nil
    let portal = c.world.as_ref().map_or(IntPair { x: 0, y: 0 }, |w| w.start_portal_pos());
    DynamicObject::update_position(b, c.me, portal);
    let fp = Vector2 { x: ((portal.x as f64) + 0.5f64) as f32, y: portal.y as f32 };
    b.base.borrow_mut().float_pos = fp;
    let pos = b.pos();
    let t = c.world.as_ref().and_then(|w| tile_at(w, pos));
    {
        let mut f = b.bh.borrow_mut();
        f.render_pos = fp;
        f.next_to_square = pos;
        f.to_square = pos;
        f.prev_from_square = pos;
        f.from_square = pos;
        f.to_tile = t;
        f.from_tile = t;
    }
    b.set_update_needs_to_be_sent(true);
    // fanfare.wav (CLIENT-ONLY); the particle loop executes 256 x 7 rand() calls (colour, offset x2, velocity x2, life, scale)
    for _p in 0..256 {
        for _k in 0..7 {
            let _ = rand_tagged("blockhead/update/regeneration_particle");
        }
    }
    // hideRegenerateUI / zoomToPos: CLIENT-ONLY
    Flow::Return
}

// ====================================================================================================
// U3. Survival stats: runs only if !isNet (6674..7330)
// ====================================================================================================

fn u3(c: &Ctx, should_collapse: &mut bool) -> Flow {
    let b = c.b;
    if c.is_net {
        b.bh.borrow_mut().path_needs_recalculated = false;
        return Flow::Continue;
    }
    let dt = c.dt;
    let rules_on = c.rules_on();
    // ---- environment
    {
        let mut f = b.bh.borrow_mut();
        let s = &mut f.state;
        let m1 = min_d(1.0, (s.environment_temperature as f64) / 0.75);
        let m2 = min_d(1.0, (s.environment_exposure as f64) / 0.75);
        let m3 = min_d(1.0, (s.environment_light as f64) / 0.75);
        let mut env_new: f32 = (m1 * m2 * m3) as f32;
        if s.sub_animation_type == vsa::LIE_ON_GROUND {
            env_new *= 0.75f32;
        } else if state_meditating(s) {
            env_new = 1.0f32;
        }
        s.environment =
            ((s.environment as f64) * (1.0f64 - (dt as f64) * 0.05f64) + ((env_new * dt) as f64) * 0.05f64) as f32;
        // `ehc` (environmentHapinessContribution) is not used afterwards in this function
    }
    // ---- custom rules overrides
    if rules_on && c.rules.health == 4 {
        let mut f = b.bh.borrow_mut();
        f.state.health = 1.0;
        f.state.death = 0.0;
    }
    if rules_on && c.rules.death == 0 {
        b.bh.borrow_mut().state.death = 0.0;
    }
    // ---- hunger
    {
        let (hat, asleep, meditating) = {
            let f = b.bh.borrow();
            (f.hat_item_type, state_asleep(&f.state), state_meditating(&f.state))
        };
        if rules_on && c.rules.hunger == 3 {
            b.bh.borrow_mut().state.fullness = 1.0;
        } else if !c.expert && hat == 0x8c {
            b.bh.borrow_mut().state.fullness = 1.0;
        } else if !asleep && !meditating {
            let due = {
                let mut f = b.bh.borrow_mut();
                f.state.food_pause_timer -= dt;
                f.state.food_pause_timer <= 0.0f32
            };
            if due {
                let mut hunger_rate = if c.expert { 2.0f32 } else { 1.0f32 };
                if rules_on && c.rules.hunger == 1 {
                    hunger_rate = 1.0;
                }
                if c.expert && hat == 0x8c {
                    hunger_rate = 0.25;
                } else if rules_on && c.rules.hunger == 2 {
                    hunger_rate = 0.25;
                } else if rules_on && c.rules.hunger == 0 {
                    hunger_rate = 4.0;
                }
                let fullness = {
                    let mut f = b.bh.borrow_mut();
                    f.state.fullness = f.state.fullness - (dt / 1800.0f32) * hunger_rate;
                    f.state.fullness
                };
                if fullness < -0.1f32 && !(rules_on && c.rules.health == 4) {
                    let mut decrement = (dt / 900.0f32) * 0.5f32;
                    if rules_on {
                        match c.rules.health {
                            0 => decrement = 99999.9f32,
                            1 => decrement *= 10.0f32,
                            3 => decrement *= 0.1f32,
                            _ => {}
                        }
                    }
                    c.damage_apply(decrement);
                    if c.sim_or_ctl {
                        let k = c.killed_or("injured");
                        c.event(4, SimulationEventExtra::Text(format!("{} by starvation", k)));
                    }
                    let mut f = b.bh.borrow_mut();
                    f.needs_harm_flash = true;
                    if f.state.fullness < -0.25f32 {
                        f.state.fullness = -0.25f32;
                        *should_collapse = true;
                    }
                }
            }
        }
    }
    // ---- BlockheadAI: hatTexture is a client texture (cache is None on the server): never runs.
    // ---- heat and coffee
    {
        let mut f = b.bh.borrow_mut();
        let s = &mut f.state;
        if 0.0f32 < s.heat {
            s.heat = s.heat - (dt * 100.0f32 * (s.heat / 100.0f32 + 1.0f32)) / 900.0f32;
            if s.heat < 0.0 {
                s.heat = 0.0;
            }
        }
        s.coffee_energy_timer -= dt;
    }
    // ---- sleep and meditation
    {
        let sub = b.bh.borrow().state.sub_animation_type;
        if sub == vsa::LIE_ON_BED {
            let bed = b.bh.borrow().interaction_object.clone();
            let bed_type = bed.as_ref().map_or(0, |o| c.h.object_item_type(o));
            let (mult, max) = match bed_type {
                0xa9 => (1.5f32, 1.5f32),
                0xaa => (10.0f32, 2.0f32),
                _ => (1.0f32, 1.2f32),
            };
            let mut f = b.bh.borrow_mut();
            let s = &mut f.state;
            if s.energy < 0.25f32 {
                s.energy = 0.25f32;
            }
            let inc = (dt / 630.0f32) * mult;
            s.energy += inc;
            if 1.0f32 < s.energy {
                s.energy = max;
            }
            let k = if c.expert { 0.25f64 } else { 0.5f64 };
            s.health = min_d(1.0, (s.health as f64) + (inc as f64) * k) as f32;
            s.death = 0.0;
        } else if sub == vsa::LIE_ON_GROUND {
            let mut wake = false;
            let done = {
                let mut f = b.bh.borrow_mut();
                let s = &mut f.state;
                if s.energy < 0.25f32 {
                    s.energy = 0.25f32;
                }
                if s.energy < 1.0f32 {
                    s.energy += dt / 1080.0f32;
                    if 1.0f32 < s.energy {
                        s.energy = 1.0f32;
                        wake = true;
                    }
                }
                wake || 1.0f32 <= s.energy
            };
            if done {
                if c.sim_or_ctl {
                    c.event(0, SimulationEventExtra::None);
                }
                b.stop_interacting();
                b.bh.borrow_mut().path_needs_recalculated = true;
            } else {
                let k = if c.expert { 0.125f64 } else { 0.25f64 };
                let mut f = b.bh.borrow_mut();
                let s = &mut f.state;
                s.health = min_d(1.0, (s.health as f64) + ((dt / 1080.0f32) as f64) * k) as f32;
                s.death = 0.0;
            }
        } else {
            let meditating = state_meditating(&b.bh.borrow().state);
            if !meditating {
                let mut f = b.bh.borrow_mut();
                let s = &mut f.state;
                s.energy = ((s.energy as f64) - (dt as f64) / 540.0f64) as f32;
                if s.energy < 0.0 {
                    s.energy = 0.0;
                    *should_collapse = true;
                }
            } else if !c.sim_or_ctl {
                let a = if c.world.as_ref().map_or(false, |w| w.fast_forward()) { dt / 20.0f32 } else { dt };
                let over = {
                    let mut f = b.bh.borrow_mut();
                    f.state.meditation_progress += a / 900.0f32;
                    f.state.meditation_progress > 1.0f32
                };
                if over {
                    b.bh.borrow_mut().state.meditation_progress = 0.0;
                    let r = r01_from(rand_tagged("blockhead/update/meditation_count"));
                    let n = f32_to_i32(r * 3.0f32 + 1.0f32);
                    for _ in 0..n.max(0) {
                        let t = random_meditation_bonus();
                        let (da, db) = default_data_a_and_b_for_item_type(t);
                        c.create_free_block(b.pos(), t.0, da as u16, db as u16, 1, false);
                    }
                }
            }
        }
    }
    // ---- happiness
    {
        let mut f = b.bh.borrow_mut();
        let s = &mut f.state;
        let a = min_d(1.0, (s.fullness as f64) / 0.5f64);
        let e = min_d(1.0, max_d(0.0, ((s.energy as f64) - 0.25f64) / 0.75f64));
        let bb = min_d(1.0, e / 0.25f64);
        let cc = min_d(1.0, (s.environment as f64) / 0.75f64);
        s.happiness = ((a + bb + cc) / 3.0f64) as f32;
        s.happiness = if s.health <= s.happiness { s.health } else { s.happiness };
        if s.fullness < 0.25f32 {
            s.happiness = s.happiness * s.fullness * 4.0f32;
        }
        let ee = ((s.energy as f64) - 0.25f64) / 0.75f64;
        if ee < 0.25f64 {
            s.happiness = ((s.happiness as f64) * max_d(0.0, ee) * 4.0f64) as f32;
        }
        if s.environment < 0.25f32 {
            s.happiness = s.happiness * s.environment * 4.0f32;
        }
        if f.pants_item_type == 0x8d {
            let cmin = if c.expert { 0.25f32 } else { 0.5f32 };
            let h = f.state.happiness;
            f.state.happiness = if h <= cmin { cmin } else { h };
        }
        let s = &mut f.state;
        s.happiness = if s.happiness <= 0.0f32 { 0.0 } else { s.happiness };
        if rules_on {
            match c.rules.happiness {
                3 => s.happiness = 1.0f32,
                0 => {
                    let h = s.happiness as f64;
                    if 0.001f64 < h && h < 1.0f64 {
                        s.happiness = s.happiness * s.happiness;
                    }
                }
                2 => {
                    let h = s.happiness as f64;
                    if 0.001f64 < h && h < 1.0f64 {
                        s.happiness = crate::fmath::libm::sqrt(h) as f32;
                    }
                }
                _ => {}
            }
        }
    }
    // ---- early exits (inside the !isNet branch)
    if b.bh.borrow().path_needs_recalculated {
        return Flow::Return;
    }
    if b.bh.borrow().waiting_for_path {
        return Flow::Return;
    }
    // ---- fire damage
    {
        let on_fire = b.bh.borrow().from_tile.map_or(false, |t| t.on_fire() != 0);
        if on_fire {
            let due = {
                let mut f = b.bh.borrow_mut();
                f.damage_delay_timer -= dt;
                f.damage_delay_timer <= 0.0
            };
            if due {
                b.suffer_damage_is_simulation_recoil(0.1f32, c.sim, true);
                b.bh.borrow_mut().damage_delay_timer = 1.0f32;
                if c.sim_or_ctl {
                    let k = c.killed_or("burned");
                    c.event(4, SimulationEventExtra::Text(format!("{} by fire", k)));
                }
            }
        }
        let temp = b.bh.borrow().current_temperature;
        if 100.0f32 < temp {
            let due = {
                let mut f = b.bh.borrow_mut();
                f.damage_delay_timer =
                    ((f.damage_delay_timer as f64) - ((dt * (temp - 100.0f32)) as f64) * 0.01f64) as f32;
                f.damage_delay_timer <= 0.0
            };
            if due {
                let mut dmg = (((temp - 100.0f32) as f64) * 0.01f64) as f32;
                dmg = if dmg < 0.1f32 { dmg } else { 0.1f32 };
                b.suffer_damage_is_simulation_recoil(dmg, c.sim, true);
                b.bh.borrow_mut().damage_delay_timer = 1.0f32;
                if c.sim_or_ctl {
                    let k = c.killed_or("burned");
                    c.event(4, SimulationEventExtra::Text(format!("{} by fire", k)));
                }
            }
        }
    }
    // ---- breathing: mheadMatrix is all-zero on the server, so `mouth` is (0,0,0) unless isSimulation
    // : the block is skipped when !isSimulation.
    {
        let fp = b.float_pos();
        let mouth = if c.sim { Vector2 { x: fp.x, y: fp.y + 1.5f32 } } else { Vector2 { x: 0.0, y: 0.0 } };
        if 0.5f64 < (mouth.y as f64) {
            breathing(c, mouth);
        }
    }
    // ---- clothing and fuel usage timers
    {
        let (sub, anim, regen) = {
            let f = b.bh.borrow();
            (f.state.sub_animation_type, f.state.animation_type, f.state.regenerating != 0)
        };
        let mut mult = 1.0f32;
        if sub == 0 || sub == 4 {
            mult = match anim {
                0 => 0.25f32,
                5 => 2.0f32,
                7 | 14 | 23 | 27 | 28 | 29 | 30 | 33 => 0.25f32,
                _ => 1.0f32,
            };
        } else if sub == 2 {
            mult = 0.5f32;
        } else if matches!(sub, 5 | 6 | 7) {
            mult = 0.0f32;
        }
        if regen {
            mult = 0.0;
        }
        let tick = |sel: fn(&mut super::blockhead::BlockheadFields) -> &mut f32, add: f32, limit: f32| -> bool {
            let mut f = b.bh.borrow_mut();
            let t = sel(&mut f);
            *t += add;
            if limit < *t {
                *t -= limit;
                true
            } else {
                false
            }
        };
        if tick(|f| &mut f.clothing_increment_timer, dt * mult, 263.671875f32) {
            c.h.increment_usage_of_clothing(b);
        }
        if tick(|f| &mut f.clothing_ice_increment_timer, dt, 263.671875f32) {
            c.h.increment_usage_of_ice_clothing(b);
        }
        let jfu = b.bh.borrow().jet_fuel_usage;
        if 0.1f32 < jfu {
            if tick(|f| &mut f.fuel_usage_increment_timer, dt * (jfu - 0.1f32), 2.197265625f32) {
                c.h.increment_fuel_usage(b);
            }
        }
        let (item_ty, ride) = {
            let f = b.bh.borrow();
            (f.interaction_item.as_ref().map(|i| i.borrow().item_type.0), f.ride_object.clone())
        };
        if item_ty == Some(0x4a) && ride.map_or(false, |r| c.h.is_donkey(&r)) {
            if tick(|f| &mut f.passive_item_usage_increment_timer, dt, 10.0f32) {
                c.h.increment_passive_item_usage(b);
            }
        }
    }
    Flow::Continue
}

/// The breathing / drowning block (7039..7111); reachable on the server only for `isSimulation`.
fn breathing(c: &Ctx, mouth: Vector2) {
    let b = c.b;
    let dt = c.dt;
    let Some(world) = c.world.clone() else { return };
    let mp = IntPair { x: f32_to_i32(mouth.x), y: f32_to_i32(mouth.y) };
    let mt = tile_at(&world, mp);
    let mut underwater = false;
    if tile_is_water(mt) {
        let pcl = mt.map_or(0, |t| t.partial_content_left());
        if (mouth.y - (mp.y as f32)) * 255.0f32 <= (pcl as f32) {
            underwater = true;
            if b.bh.borrow().state.animation_type == va::SWIM {
                let yy = f64_to_i32((mouth.y as f64) + 0.5f64);
                let above = tile_at(&world, IntPair { x: mp.x, y: yy });
                let apcl = above.map_or(0, |t| t.partial_content_left());
                let frac = ((((mouth.y as f64) + 0.5f64) - (yy as f64)) as f32) * 255.0f32;
                if (!tile_is_water(above) || (apcl as f32) < frac) && !tile_is_solid(above) {
                    underwater = false;
                }
            }
        }
    } else if tile_is_solid(mt) && tile_is_water(tile_at(&world, b.pos())) {
        underwater = true;
    }
    let space_fraction: f32 = {
        let v = (mouth.y - 960.0f32) / 32.0f32;
        if 0.0f32 < v {
            v
        } else {
            0.0
        }
    };
    let drowning_in_space = 0.001f32 < space_fraction;
    {
        let mut f = b.bh.borrow_mut();
        f.underwater = underwater;
        f.drowning_in_space = drowning_in_space;
    }
    let rules_on = c.rules_on();
    if (!underwater && !drowning_in_space) || (rules_on && c.rules.health == 4) {
        let mut f = b.bh.borrow_mut();
        let s = &mut f.state;
        if 0.0f32 < s.drown_fraction {
            if s.drown_fraction <= 1.0f32 {
                s.drown_fraction -= dt * 0.5f32;
                if s.drown_fraction < 0.0 {
                    s.drown_fraction = 0.0;
                }
            } else {
                s.drown_fraction = 1.0;
            }
        }
    } else if underwater {
        let hit = {
            let mut f = b.bh.borrow_mut();
            f.state.drown_fraction += dt / 30.0f32;
            if 1.0333333f32 <= f.state.drown_fraction {
                f.state.drown_fraction -= 0.033333335f32;
                true
            } else {
                false
            }
        };
        if hit {
            let dec = c.health_rule_modified(0.05f32);
            c.damage_apply(dec);
            if c.sim_or_ctl {
                let k = c.killed_or("injured");
                c.event(4, SimulationEventExtra::Text(format!("{} by drowning", k)));
            }
            b.bh.borrow_mut().needs_harm_flash = true;
        }
    } else {
        let hit = {
            let mut f = b.bh.borrow_mut();
            f.state.drown_fraction += (dt / 30.0f32) * space_fraction;
            if 1.1f32 <= f.state.drown_fraction {
                f.state.drown_fraction -= 0.1f32;
                true
            } else {
                false
            }
        };
        if hit {
            let dec = c.health_rule_modified(0.05f32);
            c.damage_apply(dec);
            if c.sim_or_ctl {
                let k = c.killed_or("injured");
                c.event(4, SimulationEventExtra::Text(format!("{} from lack of oxygen", k)));
            }
            b.bh.borrow_mut().needs_harm_flash = true;
        }
    }
}

// ====================================================================================================
// U4. Idle animation timer (7309..7318), all kinds including isNet
// ====================================================================================================

fn u4(c: &Ctx) {
    let b = c.b;
    let (sub, interacting, anim, coffee) = {
        let f = b.bh.borrow();
        (f.state.sub_animation_type, f.state.interacting != 0, f.state.animation_type, f.state.coffee_energy_timer)
    };
    if sub == 0 && !interacting && anim != 32 {
        let t = if 0.0f32 < coffee { c.dt * 4.0f32 } else { c.dt };
        let over = {
            let mut f = b.bh.borrow_mut();
            f.idle_timer += t;
            f.random_idle_wait < f.idle_timer
        };
        if over {
            let r0 = r01_from(rand_tagged("blockhead/update/idle_wait"));
            let ra = r01_from(rand_tagged("blockhead/update/idle_a"));
            let rb = r01_from(rand_tagged("blockhead/update/idle_b"));
            let rc = r01_from(rand_tagged("blockhead/update/idle_c"));
            let mut f = b.bh.borrow_mut();
            f.random_idle_wait = r0 * 10.0f32 + 5.0f32;
            f.idle_timer = 0.0;
            f.walk_timer = 0.0;
            f.state.sub_animation_type = 4;
            f.random_animation_value_a = ra;
            f.random_animation_value_b = rb;
            f.random_animation_value_c = rc;
        }
    }
}

// ====================================================================================================
// U5. D-pad (client) and tile re-validation (7322..7480)
// ====================================================================================================

fn u5(c: &Ctx) {
    let b = c.b;
    // D-pad: `dpadShouldBeDisplayed && activeBlockhead == self && dpad.currentlyInUse` is CLIENT-ONLY: always false
    // here, `beingControlledByDPad = 0`, `shouldUseDpadMovement = NO`.
    b.bh.borrow_mut().being_controlled_by_d_pad = false;
    let eligible = {
        let f = b.bh.borrow();
        !c.is_net
            && f.ride_object.is_none()
            && !f.path_needs_recalculated
            && !f.waiting_for_path
            && !f.current_movement_was_initiated_with_d_pad
            && f.traverse_to_key_frame as u32 != kf::STAND_ELEVATOR
            && !f.is_in_jet_pack_free_flight_mode // `|| [world dpadControl]` is constant NO
    };
    if eligible {
        if let Some(world) = c.world.clone() {
            let ckf = c.h.current_traverse_to_key_frame(b);
            let (mut to_tile, mut from_tile, fs, ts) = {
                let f = b.bh.borrow();
                (f.to_tile, f.from_tile, f.from_square, f.to_square)
            };
            if to_tile.is_none() {
                to_tile = tile_at(&world, ts);
            }
            if from_tile.is_none() {
                from_tile = tile_at(&world, fs);
            }
            {
                let mut f = b.bh.borrow_mut();
                f.to_tile = to_tile;
                f.from_tile = from_tile;
            }
            let props = FromProps { terrain_difficulty: 0, traverse_to_key_frame: ckf };
            let r = check_can_enter_tile(
                to_tile,
                from_tile,
                props,
                fs.x,
                fs.y,
                ts.x,
                ts.y,
                Some(c.me),
                &world,
                true,
                true,
                &[],
            );
            let path_empty = b.bh.borrow().path.as_ref().map_or(true, |p| p.is_empty());
            let falling = {
                let f = b.bh.borrow();
                f.path.as_ref().map_or(0, |p| p.len()) != 0 && f.traverse_type as u32 == tt::FALL
            };
            let bail = r.traverse_type == tt::INACCESSIBLE
                || (r.traverse_to_key_frame == kf::FALLING && path_empty)
                || (c.h.can_fly(b) && falling && r.traverse_type == tt::FLY);
            if bail {
                if to_tile == from_tile {
                    b.stop_interacting();
                    b.bh.borrow_mut().path_needs_recalculated = true;
                } else {
                    {
                        let mut f = b.bh.borrow_mut();
                        f.to_tile = f.from_tile;
                        f.to_square = f.from_square;
                    }
                    let ts2 = b.bh.borrow().to_square;
                    DynamicObject::update_position(b, c.me, ts2);
                    let fp = Vector2 { x: ((ts2.x as f64) + 0.5f64) as f32, y: ts2.y as f32 };
                    b.base.borrow_mut().float_pos = fp;
                    let mut f = b.bh.borrow_mut();
                    f.render_pos = fp;
                    f.path_needs_recalculated = true;
                    f.path = None;
                }
            } else {
                let (cur_kf,) = (b.bh.borrow().traverse_to_key_frame,);
                if r.traverse_to_key_frame as i32 != cur_kf
                    && r.traverse_to_key_frame != kf::STAND_ELEVATOR
                    && cur_kf as u32 != kf::STAND_ELEVATOR
                {
                    let new_type = match r.traverse_to_key_frame {
                        kf::BOB => va::BOB,
                        kf::HANG_BACK | kf::HANG_RIGHT | kf::HANG_LEFT => va::HANG,
                        kf::CRAWL => va::LIE,
                        kf::MONKEY_HANG => va::MONKEY_HANG,
                        _ => 0,
                    };
                    {
                        let mut f = b.bh.borrow_mut();
                        f.traverse_to_key_frame = r.traverse_to_key_frame as i32;
                        f.state.animation_type = new_type;
                    }
                    b.set_update_needs_to_be_sent(true);
                    let (anim, ty) = {
                        let f = b.bh.borrow();
                        (f.state.animation_type, f.to_square.y)
                    };
                    let mut bp = b.base.borrow_mut();
                    if anim == va::LIE {
                        bp.float_pos.y = (ty as f32) - 0.5f32;
                    } else if anim == va::MONKEY_HANG {
                        bp.float_pos.y = (ty as f32) + 0.2f32;
                    }
                }
            }
        }
    }
    // fishingRod update (everyone)
    let rod = b.bh.borrow().fishing_rod.clone();
    if let Some(r) = rod {
        c.h.fishing_rod_update(&r, c.dt, c.adt, c.sim);
    }
}

// ====================================================================================================
// U6 / U7 / U8
// ====================================================================================================

fn u6_u7(c: &Ctx, should_collapse: bool) {
    let b = c.b;
    let (coffee, stationary, run_u7) = {
        let f = b.bh.borrow();
        let coffee = f.state.coffee_energy_timer;
        // shouldUseDpadMovement is constant NO on the server
        let stationary = f.from_square == f.to_square;
        let path_n = f.path.as_ref().map_or(0, |p| p.len());
        let run_u7 = (path_n != 0 && !f.using_item)
            || f.use_item
            || (c.is_net
                && f.current_net_queue_size > 0
                && (f.state.interacting == 0 || 0.3f32 <= f.state.interaction_timer))
            || f.chasing_ride_object
            || (f.chasing_npc && f.state.interacting == 0);
        (coffee, stationary, run_u7)
    };
    let mut coffee_dt = if 0.001f32 < coffee { c.dt * 1.5f32 } else { c.dt };
    if stationary && !run_u7 {
        u6(c, coffee_dt, should_collapse);
    } else {
        u7(c, &mut coffee_dt);
    }
}

fn t_rule(prev: f32, timer: f32, a: f32, bb: f32) -> Option<bool> {
    // Some(true): fire; Some(false): stop; None: nothing
    if prev < a && a <= timer {
        Some(true)
    } else if bb <= timer {
        Some(false)
    } else {
        None
    }
}

fn u6(c: &Ctx, coffee_dt: f32, should_collapse: bool) {
    let b = c.b;
    b.bh.borrow_mut().travel_fraction = 1.0f32;
    let interacting = b.bh.borrow().state.interacting != 0;
    if !interacting {
        u6a(c, should_collapse);
    } else {
        u6b(c, coffee_dt);
    }
}

fn u6a(c: &Ctx, should_collapse: bool) {
    let b = c.b;
    if c.is_net {
        b.bh.borrow_mut().next_action_delay = 0.0;
    }
    if should_collapse && !c.is_net {
        b.collapse_if_possible();
    }
    let go = {
        let f = b.bh.borrow();
        !f.action_queue.is_empty()
            && !f.waiting_for_path
            && !f.path_needs_recalculated
            && !state_asleep(&f.state)
            && !state_meditating(&f.state)
            && !f.waiting_for_fill_response
            && f.expected_craft_items.is_empty()
    };
    if go {
        let a = {
            let mut f = b.bh.borrow_mut();
            f.next_action_delay = 0.0;
            f.action_queue[0].clone()
        };
        if !c.h.action_in_progress(&a) {
            b.bh.borrow_mut().path_needs_recalculated = true;
        } else {
            b.bh.borrow_mut().action_queue.remove(0);
            if !c.is_net {
                if let Some(dw) = b.dw() {
                    dw.dynamic_world_changed_at_pos_object_type(b.pos(), DynamicObject::object_type(b));
                }
            }
        }
    } else {
        let mut f = b.bh.borrow_mut();
        if f.waiting_for_fill_response {
            f.waiting_for_fill_response_timer += c.dt;
            if 30.0f32 < f.waiting_for_fill_response_timer {
                f.waiting_for_fill_response = false;
            }
        }
    }
}

/// U6b: interacting.
fn u6b(c: &Ctx, coffee_dt: f32) {
    let b = c.b;
    if b.bh.borrow().state.interaction_type == it::NONE {
        b.stop_interacting();
    }
    let (prev, itype, happiness, health) = {
        let f = b.bh.borrow();
        (f.state.interaction_timer, f.state.interaction_type, f.state.happiness, f.state.health)
    };
    {
        let mut f = b.bh.borrow_mut();
        if itype == it::CRAFT || itype == it::USE_INTERACTIVE_OBJECT {
            f.state.interaction_timer += coffee_dt;
        } else {
            let mul = if c.expert { happiness as f64 } else { 1.0f64 };
            f.state.interaction_timer = ((f.state.interaction_timer as f64)
                + (coffee_dt as f64) * (((happiness + health) * 0.4f32) as f64 * mul + (0.4f32 as f64)))
                as f32;
        }
    }
    if c.is_net {
        return;
    }
    let timer = b.bh.borrow().state.interaction_timer;
    match itype {
        it::DIG | it::DIG_FOREGROUND | it::DIG_BACK_WALL => u6_dig(c, coffee_dt),
        it::PAINT | it::STRIP_PAINT => {
            let (item, tile, sq) = {
                let f = b.bh.borrow();
                (f.interaction_item.clone(), f.interacting_tile, f.state.interacting_square)
            };
            let ok = item.is_some()
                && c.h.has_interaction_inventory_item_available(b)
                && c.world.as_ref().map_or(false, |w| tile_is_paintable(tile, sq, w, Some(c.me)));
            if !ok {
                b.stop_interacting();
            } else {
                match t_rule(prev, timer, 0.15, 0.3) {
                    Some(true) => {
                        let face = c.extra_face();
                        if itype == it::PAINT {
                            let color = item.as_ref().unwrap().borrow().data_b;
                            c.h.paint_tile_at_pos_color_index_face_index_paint_blockhead(tile, sq, color, face, c.me);
                            c.h.increment_usage_of_interaction_item(b, false);
                            c.guarded_achievement("grp.decorator");
                        } else {
                            c.h.remove_paint_at_tile_at_pos_face_index_paint_blockhead(tile, sq, face, c.me);
                            c.h.increment_usage_of_interaction_item(b, false);
                        }
                    }
                    Some(false) => b.stop_interacting(),
                    None => {}
                }
            }
        }
        it::FEED_NPC => {
            let item = b.bh.borrow().interaction_item.clone();
            let npc = b.bh.borrow().tapped_npc.clone();
            let ok = item.is_some()
                && c.h.has_interaction_inventory_item_available(b)
                && npc.as_ref().map_or(false, |n| c.h.npc_can_be_fed_by_blockhead(n, b));
            if !ok {
                b.stop_interacting();
            } else {
                match t_rule(prev, timer, 0.15, 0.3) {
                    Some(true) => {
                        c.h.npc_feed_by_blockhead(npc.as_ref().unwrap(), b);
                        c.h.remove_interaction_item(b, false);
                    }
                    Some(false) => b.stop_interacting(),
                    None => {}
                }
            }
        }
        it::SET_NPC_FREE => {
            let npc = b.bh.borrow().tapped_npc.clone();
            if !npc.as_ref().map_or(false, |n| c.h.npc_belongs_to_player_with_blockhead(n, b)) {
                b.stop_interacting();
            } else {
                match t_rule(prev, timer, 0.15, 0.3) {
                    Some(true) => c.h.npc_set_free_by_blockhead(npc.as_ref().unwrap(), b),
                    Some(false) => b.stop_interacting(),
                    None => {}
                }
            }
        }
        it::FILL_BUCKET | it::CAPTURE | it::HARVEST_NPC => u6_fill_bucket(c, itype, prev, timer),
        it::PLACE_ITEM | it::PLACE_SOLID_BLOCK => u6_place(c, itype, prev, timer),
        it::CRAFT => u6_craft(c, coffee_dt),
        it::USE_INTERACTIVE_OBJECT => {
            let obj = b.bh.borrow().interaction_object.clone();
            if obj.as_ref().map_or(false, |o| c.h.is_bed(o)) && 1.1f32 < b.bh.borrow().state.energy {
                if c.sim_or_ctl {
                    c.event(0, SimulationEventExtra::None);
                }
                b.stop_interacting();
                b.bh.borrow_mut().path_needs_recalculated = true;
            }
        }
        it::BURN => {
            let (item, tile, sq) = {
                let f = b.bh.borrow();
                (f.interaction_item.clone(), f.interacting_tile, f.state.interacting_square)
            };
            if !(item.is_some() && c.h.has_interaction_inventory_item_available(b)) {
                b.stop_interacting();
            } else {
                match t_rule(prev, timer, 0.15, 0.3) {
                    Some(true) => {
                        c.h.remove_interaction_item(b, false);
                        c.h.fill_tile_at_pos_with_type(tile, sq, 0xd);
                        c.guarded_achievement("grp.pyro");
                    }
                    Some(false) => b.stop_interacting(),
                    None => {}
                }
            }
        }
        it::SOW => {
            let (item, tile, sq) = {
                let f = b.bh.borrow();
                (f.interaction_item.clone(), f.interacting_tile, f.state.interacting_square)
            };
            if !(item.is_some() && c.h.has_interaction_inventory_item_available(b)) {
                b.stop_interacting();
            } else {
                match t_rule(prev, timer, 0.15, 0.3) {
                    Some(true) => {
                        let item = item.unwrap();
                        let (ity, da, db, sd) = {
                            let i = item.borrow();
                            (i.item_type.0, i.data_a, i.data_b, i.dynamic_object_save_dict.clone())
                        };
                        let mut sow = sq;
                        if ity != 0x127 {
                            sow.y = sow.y.wrapping_add(1);
                        }
                        if c.sim_or_ctl {
                            c.event(3, SimulationEventExtra::ItemType(ity as i32));
                        }
                        let cid = c.local_net_id();
                        c.h.fill_tile_at_pos_with_type_data_a_data_b_placed_by_client_save_dict_placed_by_blockhead_placed_by_client_name(
                            tile,
                            sow,
                            ity as i32,
                            da,
                            db,
                            cid.as_deref(),
                            sd.as_ref(),
                            None,
                            None,
                        );
                        c.h.remove_interaction_item(b, false);
                        // place.wav (if !ctl): CLIENT-ONLY
                        if !c.is_net && !c.ctl {
                            c.achievement("grp.greenfingers");
                            if ItemType(ity) == ItemType::ITEM_TULIP_SEED {
                                c.achievement("grp.tulip");
                            }
                            if tile.map_or(false, |t| matches!(t.type_index(), 0x30 | 0x31 | 0x32)) {
                                c.achievement("grp.compost");
                            }
                        }
                    }
                    Some(false) => b.stop_interacting(),
                    None => {}
                }
            }
        }
        it::PICKUP | it::PICKUP_FREEBLOCK => match t_rule(prev, timer, 0.1, 0.2) {
            Some(true) => {
                let (tile, sq) = {
                    let f = b.bh.borrow();
                    (f.interacting_tile, f.state.interacting_square)
                };
                b.pick_up_item_if_possible_in_tile_at_pos(tile.as_ref(), sq);
            }
            Some(false) => b.stop_interacting(),
            None => {}
        },
        it::STAB => u6_stab(c, prev, timer),
        it::FIRE_RANGED_WEAPON => u6_fire(c, prev, timer),
        _ => {}
    }
}

fn u6_dig(c: &Ctx, coffee_dt: f32) {
    let b = c.b;
    let h = &c.h;
    let me = c.me;
    let (tile, sq, item) = {
        let f = b.bh.borrow();
        (f.interacting_tile, f.state.interacting_square, f.interaction_item.clone())
    };
    let cit = c.interaction_type_checked(tile, sq, item.as_ref(), c.extra_face());
    if !matches!(cit, 1 | 23 | 24 | 4 | 5) || !h.has_interaction_inventory_item_available(b) {
        b.stop_interacting();
        return;
    }
    let action = b.bh.borrow().action_queue.first().cloned();
    let tr = h.good_or_bad_interaction_for_action(b, action.as_ref());
    if let Some(a) = &action {
        if tr.good_or_bad != h.action_interaction_test_result(a).good_or_bad {
            b.stop_interacting();
        }
    }
    let (happiness, health) = {
        let f = b.bh.borrow();
        (f.state.happiness, f.state.health)
    };
    {
        let mul = if c.expert { happiness as f64 } else { 1.0f64 };
        let mut f = b.bh.borrow_mut();
        f.state.gather_timer = ((f.state.gather_timer as f64)
            + (coffee_dt as f64) * (((happiness + health) as f64) * mul * (0.4f32 as f64) + (0.4f32 as f64)))
            as f32;
        if f.state.gather_timer < 0.3f32 {
            return;
        }
        f.state.gather_timer -= 0.3f32;
    }
    let (local_prog, gather_speed, left_over) = {
        let f = b.bh.borrow();
        (f.local_gather_progress, f.state.gather_speed, f.gather_timer_left_over)
    };
    let tile_prog = tile.map_or(0, |t| t.gather_progress());
    let prog: u8 = if tile_prog < local_prog { local_prog } else { tile_prog };
    let mut only_remove_contents = false;
    let mut increment = f32_to_i32(left_over + 8.0f32 * gather_speed);
    let mut done = increment.wrapping_add(prog as i32) >= 255;
    let mut is_removing_rail = false;
    let ity = item.as_ref().map_or(0, |i| i.borrow().item_type.0);
    let tile_contents = tile.map_or(0, |t| t.contents());
    let tile_type = tile.map_or(0, |t| t.type_index());
    if ity == 0xaf {
        if tile_contents == 0x3f {
            only_remove_contents = true;
            done = true;
            increment = 8;
            c.guarded_achievement("grp.magnet");
        } else {
            let is_magnet = tile_type == 0x3a;
            if is_magnet {
                increment = 8;
                done = true;
            }
        }
    } else if ity == 0xd1 && tile.map_or(false, |t| t.foreground_contents() == 0x62) {
        let io_type = h.interaction_object_at_pos(sq).map_or(0, |o| h.interaction_object_type(&o));
        is_removing_rail = !(tile_contents == 0x30 && io_type == 6);
        if is_removing_rail {
            increment = 8;
            done = true;
        }
    }
    // CLIENT-ONLY sounds; the particle block executes 8 x 4 rand() calls on the server too
    if !c.sim && particle_color_alpha_is_visible(tile) {
        for _ in 0..32 {
            let _ = rand_tagged("blockhead/update/dig_particle");
        }
    }
    if increment == 0 {
        b.bh.borrow_mut().gather_timer_left_over = 8.0f32 * gather_speed + left_over;
    } else {
        b.bh.borrow_mut().gather_timer_left_over = 0.0;
        if !tile_is_plant(tile) {
            // bonus roll
            if ity == 0x59 || ity == 0x5a {
                let r = r01_from(rand_tagged("blockhead/update/dig_bonus_roll"));
                if (r as f64) < 0.0625f64 {
                    let mut count = 1i32;
                    let t = match &c.world {
                        Some(w) => random_bonus_item_type_for_tile(tile, sq, &mut count, w),
                        None => ItemType(0),
                    };
                    if t.0 != 0 {
                        for _ in 0..count {
                            let da = (rand_tagged("blockhead/update/dig_bonus_data_a") % 255) as u16;
                            let db = (rand_tagged("blockhead/update/dig_bonus_data_b") % 255) as u16;
                            c.create_free_block(sq, t.0, da, db, 1, true);
                        }
                    }
                }
            }
            if done {
                let mut removed = only_remove_contents;
                if let Some(t) = tile {
                    if is_removing_rail {
                        c.create_free_block(sq, 199, 0, 0, 1, true);
                        h.remove_object_at_pos(super::blockhead::Remover::Rail, sq);
                        removed = true;
                        h.remove_tile_at_world_x_world_y_create_contents_freeblock_count_create_foreground_contents_freeblock_count_remove_blockhead_only_remove_c_ontents_only_remove_foreground_contents(
                            sq.x, sq.y, 0, 0, me, false, true,
                        );
                    } else if action.as_ref().map_or(0, |a| h.action_interaction_object_id(a)) != 0 {
                        let id = h.action_interaction_object_id(action.as_ref().unwrap());
                        let obj = h.interaction_object_with_id(id);
                        if let Some(o) = obj {
                            if !h.interaction_object_is_in_use(&o) {
                                // `[world client] == nil` (server): remove directly
                                h.interaction_object_remove(&o, b);
                            }
                        }
                        removed = true;
                    }
                    if !removed {
                        let ti = t.type_index();
                        let co = t.contents();
                        if matches!(ti, 6 | 0x1b | 0x1c) {
                            let r = r01_from(rand_tagged("blockhead/update/dig_trunk"));
                            let p = if ti == 6 { 0.03125f64 } else { 0.25f64 };
                            if (r as f64) < p {
                                c.create_free_block(sq, 0x9b, 0, 0, 1, true);
                            }
                        } else if matches!(ti, 0x30 | 0x31 | 0x32) {
                            let r = r01_from(rand_tagged("blockhead/update/dig_trunk"));
                            let p = if ti == 0x30 { 0.0625f64 } else { 0.5f64 };
                            if (r as f64) < p {
                                c.create_free_block(sq, 0x9b, 0, 0, 1, true);
                            }
                        } else if ti == 0x0c && co == 0 {
                            if rand_tagged("blockhead/update/dig_lime") % 16 == 0 {
                                let (da, db) = default_data_a_and_b_for_item_type(ItemType(0xa0));
                                c.create_free_block(sq, 0xa0, da as u16, db as u16, 1, true);
                                c.guarded_achievement("grp.lime");
                            }
                        } else if ti == 4 || !matches!(co, 0x2e | 0x2f) {
                            if ti != 4 && co == 0x30 {
                                h.remove_interaction_object_at_pos_remove_blockhead(sq, me);
                                removed = true;
                            }
                        } else {
                            h.remove_workbench_at_pos_remove_blockhead(sq, me);
                            removed = true;
                        }
                    }
                }
                if !removed || only_remove_contents {
                    let fbc = b.free_block_creation_count_for_tile_with_item(tile.as_ref(), item.as_ref());
                    let door = tile_contains_door(tile);
                    let trap = tile_contains_trap_door(tile);
                    if !c.is_net && !c.ctl {
                        if tile_type == 1 && matches!(ity, 8 | 0x22 | 0x5e) {
                            c.achievement("grp.mine");
                        } else if tile_type == 0x33 {
                            c.achievement("grp.basalt");
                        }
                    }
                    // `[dynamicWorld isClient]` waitingForFillResponse: CLIENT-ONLY
                    if cit == it::DIG_BACK_WALL {
                        h.remove_back_wall_at_pos_remove_blockhead(sq, me);
                    } else {
                        let fgc = tile.map_or(0, |t| t.foreground_contents());
                        let fg = fgc != 0 && cit == it::DIG_FOREGROUND && (fgc != 0x62 || is_removing_rail);
                        let make = !(door || trap || tile_contains_gate(tile));
                        h.remove_tile_at_world_x_world_y_create_contents_freeblock_count_create_foreground_contents_freeblock_count_remove_blockhead_only_remove_c_ontents_only_remove_foreground_contents(
                            sq.x,
                            sq.y,
                            if !fg && make { fbc.count } else { 0 },
                            if fg && make { fbc.count } else { 0 },
                            me,
                            only_remove_contents,
                            false,
                        );
                        // CLIENT bonus-particle loop: executed (2 rand() per particle)
                        // (`bonusMultiplier` is never 0; a zero divisor would count nothing)
                        let n = fbc.count.checked_div(fbc.bonus_multiplier).unwrap_or(0);
                        for _ in 0..n.max(0) {
                            let _ = rand_tagged("blockhead/update/dig_bonus_particle");
                            let _ = rand_tagged("blockhead/update/dig_bonus_particle");
                        }
                    }
                    if matches!(cit, 1 | 23 | 24) {
                        let skip = matches!(ity, 0x67 | 0x68 | 0x2a | 0x101) || (ity == 0xaf && !only_remove_contents);
                        if !skip {
                            h.increment_usage_of_interaction_item(b, false);
                        }
                    }
                    b.bh.borrow_mut().path_needs_recalculated = true;
                }
                b.stop_interacting();
                b.bh.borrow_mut().local_gather_progress = 0;
            } else {
                let g = (increment.wrapping_add(prog as i32)) as u8;
                if let Some(t) = tile {
                    t.set_gather_progress(g);
                }
                b.bh.borrow_mut().local_gather_progress = g;
                if !tile_is_unplaced_block_minable_with_pickaxe(tile) && tile_type != 0x10 {
                    h.load_gather_block_at_pos(sq);
                }
                h.send_gather_notification_for_tile_at_pos(tile, sq);
                if tile_contains_door(tile) {
                    let o = h.pos_of_doors_other_block_at_pos(sq);
                    if o.x != -1 {
                        let ot = c.world.as_ref().and_then(|w| tile_at(w, o));
                        if let (Some(otile), Some(t)) = (ot, tile) {
                            if otile != t {
                                otile.set_gather_progress(b.bh.borrow().local_gather_progress);
                                h.load_gather_block_at_pos(o);
                            }
                        }
                    }
                }
                // reloadDrawBlockGeometryForTile: empty function in the server binary
            }
        } else {
            // plant tile
            let plant = h.get_plant_at_pos(sq);
            match plant {
                None => {
                    b.stop_interacting();
                    if let Some(t) = tile {
                        t.set_foreground_contents(0);
                    }
                    b.bh.borrow_mut().local_gather_progress = 0;
                }
                Some(p) => {
                    let g = h.plant_gather_progress_for_tile(&p, sq);
                    if g.wrapping_add(increment) < 255 {
                        h.plant_set_gather_progress_for_tile(&p, g.wrapping_add(increment), sq);
                    } else {
                        let mut fbc = b.free_block_creation_count_for_tile_with_item(tile.as_ref(), item.as_ref());
                        if fbc.count < 1 {
                            fbc.count = 1;
                        }
                        let n = h.plant_tile_harvested_remove_blockhead_correct_tool_multiplier(&p, sq, b, fbc.count);
                        for _ in 0..n.max(0) {
                            let _ = rand_tagged("blockhead/update/dig_plant_particle");
                            let _ = rand_tagged("blockhead/update/dig_plant_particle");
                        }
                        b.stop_interacting();
                    }
                }
            }
        }
    }
    if !done && matches!(cit, 1 | 23 | 24) {
        let skip = matches!(ity, 0x67 | 0x68 | 0x2a | 0x101 | 0xaf);
        if !skip {
            h.increment_usage_of_interaction_item(b, false);
        }
    }
}

fn u6_fill_bucket(c: &Ctx, itype: u32, prev: f32, timer: f32) {
    let b = c.b;
    let h = &c.h;
    let (item, tile, sq) = {
        let f = b.bh.borrow();
        (f.interaction_item.clone(), f.interacting_tile, f.state.interacting_square)
    };
    if !(item.is_some() && h.has_interaction_inventory_item_available(b)) {
        b.bh.borrow_mut().chasing_npc = false;
        b.stop_interacting();
        return;
    }
    match t_rule(prev, timer, 0.15, 0.3) {
        None => {}
        Some(false) => {
            b.bh.borrow_mut().chasing_npc = false;
            b.stop_interacting();
        }
        Some(true) => {
            let ity = item.as_ref().unwrap().borrow().item_type.0;
            let tapped = b.bh.borrow().tapped_npc.clone();
            let lookup = |tapped: &Option<ObjRef>| -> Option<ObjRef> {
                match tapped {
                    Some(t) => Some(t.clone()),
                    None => {
                        let pvp = c.world.as_ref().map_or(false, |w| w.pvp_enabled());
                        h.check_for_harmable_dynamic_object_under_tap_ignore_local_blockheads_ignore_all_blockheads(
                            Vector2 { x: (sq.x as f32) + 0.5f32, y: (sq.y as f32) + 0.5f32 },
                            true,
                            !pvp,
                        )
                    }
                }
            };
            if ity == 0x12e {
                let npc = lookup(&tapped);
                if let Some(n) = npc {
                    if h.npc_capture_by_blockhead_with_item_type(&n, b, 0x12e) {
                        h.remove_interaction_item(b, false);
                    }
                }
            } else if ity == 0x142 {
                let _npc = lookup(&tapped);
                // binary quirk: `shaveByBlockhead:` is sent to tappedNPC, NOT to the looked-up npc
                if let Some(t) = &tapped {
                    if h.npc_shave_by_blockhead(t, b) {
                        h.increment_usage_of_interaction_item(b, false);
                    }
                }
            } else if ity == 0x66 {
                if itype == it::CAPTURE || itype == it::HARVEST_NPC {
                    let npc = lookup(&tapped);
                    let mut created = false;
                    if let Some(n) = npc {
                        let ot = n.object_type();
                        if ot == 0x23 || ot == 0x24 {
                            if h.npc_capture_by_blockhead_with_item_type(&n, b, 0x66) {
                                created = true;
                            }
                            if ot == 0x24 {
                                // no isNet / ctl guard here
                                c.achievement("grp.baby_shark");
                            }
                        } else if ot == 0x3f {
                            if let Some(t) = &tapped {
                                if h.npc_milk_by_blockhead(t, b) {
                                    created = true;
                                }
                            }
                        }
                    }
                    if created {
                        h.remove_interaction_item(b, false);
                    }
                } else if itype == it::FILL_BUCKET
                    && tile_is_water(tile)
                    && tile.map_or(0, |t| t.partial_content_left()) > 0x20
                {
                    let pcl = tile.unwrap().partial_content_left();
                    let enough = !c.expert || pcl > 0xe6;
                    h.remove_water_tile_at_pos(sq);
                    h.remove_interaction_item(b, false);
                    c.create_free_block(sq, if enough { 0x69 } else { 0x66 }, 0, 0, 0, true);
                }
            }
        }
    }
}

fn u6_place(c: &Ctx, itype: u32, prev: f32, timer: f32) {
    let b = c.b;
    let h = &c.h;
    let me = c.me;
    let (item, tile, sq) = {
        let f = b.bh.borrow();
        (f.interaction_item.clone(), f.interacting_tile, f.state.interacting_square)
    };
    if !(item.is_some() && h.has_interaction_inventory_item_available(b)) {
        b.stop_interacting();
        return;
    }
    let g = c.interaction_type_checked(tile, sq, item.as_ref(), c.extra_face());
    if g != itype {
        // noPath.wav (if !ctl): CLIENT-ONLY
        b.stop_interacting();
        return;
    }
    match t_rule(prev, timer, 0.15, 0.3) {
        None => {}
        Some(false) => b.stop_interacting(),
        Some(true) => {
            let item = b.bh.borrow().interaction_item.clone().unwrap_or_else(|| item.unwrap());
            let pt = item.borrow().item_type.0;
            if c.sim_or_ctl {
                c.event(3, SimulationEventExtra::ItemType(pt as i32));
            }
            let first = b.bh.borrow().action_queue.first().cloned();
            if let Some(a) = first {
                let key = format!("{}", pt);
                if let Some(v) = h.action_inventory_change_int_for_key(&a, &key) {
                    h.action_set_inventory_change_int_for_key(&a, &key, v.wrapping_sub(1));
                }
            }
            // place.wav (if !ctl): CLIENT-ONLY
            h.remove_interaction_item(b, false);
            let pty = ItemType(pt);
            if item_type_is_full_cage(pty) {
                c.create_free_block(sq, 0x12e, 0, 0, 0, true);
            }
            if pty == ItemType::ITEM_WATER_BUCKET
                || pty == ItemType::ITEM_FISH_BUCKET
                || pty == ItemType::ITEM_SHARK_BUCKET
            {
                c.create_free_block(sq, 0x66, 0, 0, 0, true);
            }
            // `clientName == nil` on the server: playerNameForPlayerWithIDIncludingOldPlayers: result unused
            let (sd, da, db) = {
                let i = item.borrow();
                (i.dynamic_object_save_dict.clone(), i.data_a, i.data_b)
            };
            let cid = c.local_net_id();
            if item_type_is_workbench(pty) {
                h.place_workbench_of_type_at_pos_save_dict_placed_by_client_placed_by_blockhead_placed_by_client_name(
                    workbench_type_for_item_type(pty).0 as i32,
                    sq,
                    sd.as_ref(),
                    cid.as_deref(),
                    me,
                    None,
                );
                if pty == ItemType::ITEM_CAMPFIRE {
                    c.guarded_achievement("grp.campfire");
                } else if pty == ItemType::ITEM_SOLAR_PANEL {
                    c.guarded_achievement("grp.solar");
                }
            } else if item_type_is_interaction_object(pty) {
                let obj = h.place_interaction_object_with_item_at_pos_save_dict_placed_by_client_placed_by_blockhead_placed_by_client_name(
                    &item,
                    sq,
                    sd.as_ref(),
                    cid.as_deref(),
                    me,
                    None,
                );
                if pty == ItemType::ITEM_SIGN && obj.as_ref().map_or(0, |o| h.sign_text_length(o)) == 0 {
                    // the old pathExtraData is NOT released (leak); replaced here
                    let mut e = PathExtraData::new();
                    if let Some(o) = obj {
                        e.set("interactionObject", ExtraValue::Obj(o));
                    }
                    b.bh.borrow_mut().path_extra_data = Some(e);
                    let it_tile = b.bh.borrow().interacting_tile;
                    let idx = c.world.as_ref().map_or(-1, |w| world_index_at_world_pos(sq, w));
                    b.start_interacting_with_tile_at_index_tile_interaction_type(
                        idx,
                        it_tile,
                        it::USE_INTERACTIVE_OBJECT,
                    );
                }
                if pty == ItemType::ITEM_SIGN || pty == ItemType::ITEM_OWNERSHIP_SIGN {
                    c.guarded_achievement("grp.sign");
                }
                if pty == ItemType::ITEM_OWNERSHIP_SIGN {
                    h.display_ownership_areas();
                }
            } else {
                h.fill_tile_at_pos_with_type_data_a_data_b_placed_by_client_save_dict_placed_by_blockhead_placed_by_client_name(
                    tile,
                    sq,
                    pt as i32,
                    da,
                    db,
                    cid.as_deref(),
                    sd.as_ref(),
                    Some(me),
                    None,
                );
                if pty == ItemType::ITEM_CLAY_LANTERN_LIT || pty == ItemType::ITEM_STEEL_LANTERN {
                    c.guarded_achievement("grp.lantern");
                } else if pty == ItemType::ITEM_GOLD_BLOCK {
                    c.guarded_achievement("grp.gold_builder");
                } else if pty == ItemType::ITEM_SHARK_JAW {
                    c.guarded_achievement("grp.jaws");
                } else if pty == ItemType::ITEM_STEEL_BLOCK {
                    c.guarded_achievement("grp.steel_block");
                } else if item_type_is_painting(pty) {
                    c.guarded_achievement("grp.painting");
                } else if pty == ItemType::ITEM_STEEL_DOWNLIGHT || pty == ItemType::ITEM_STEEL_UPLIGHT {
                    c.guarded_achievement("grp.spotlight");
                }
            }
        }
    }
}

fn u6_craft(c: &Ctx, coffee_dt: f32) {
    let b = c.b;
    let h = &c.h;
    let wb = b.bh.borrow().interaction_workbench.clone();
    let Some(wb) = wb else { return };
    let r = h.unable_to_work_reason(b);
    {
        let mut f = b.bh.borrow_mut();
        if r != 0 && f.tip_type == 0 {
            if r == 1 {
                f.tip_type = 3;
            } else if r == 2 {
                f.tip_type = 4;
            }
        }
    }
    let wtype = h.workbench_type(&wb);
    let has_fuel = h.workbench_has_required_fuel(&wb);
    if !matches!(wtype, 1 | 0xd | 9 | 8 | 0x1f | 0xe | 3)
        && !h.workbench_uses_stores_conducts_or_produces_electricity(&wb)
        && r == 0
        && has_fuel
    {
        let mut f = b.bh.borrow_mut();
        f.state.gather_timer += coffee_dt;
        if 0.3f32 <= f.state.gather_timer {
            f.state.gather_timer -= 0.3f32;
            // craft sounds (NSDate timing): CLIENT-ONLY
        }
    }
    let has_fuel2 = h.workbench_has_required_fuel(&wb);
    if b.bh.borrow().tip_type == 0 && !has_fuel2 {
        let level = h.workbench_level(&wb);
        let needs_el = workbench_requires_electricty(WorkbenchType(wtype as u32), level);
        b.bh.borrow_mut().tip_type = if needs_el { 6 } else { 5 };
    }
}

fn u6_stab(c: &Ctx, prev: f32, timer: f32) {
    let b = c.b;
    let h = &c.h;
    match t_rule(prev, timer, 0.15, 0.3) {
        None => {}
        Some(false) => b.stop_interacting(),
        Some(true) => {
            let item = b.bh.borrow().interaction_item.clone();
            let mut force: i32 = 4;
            let ity = item.as_ref().map(|i| i.borrow().item_type.0);
            match ity {
                None => {}
                Some(t) if matches!(t, 0x67 | 0x68 | 0x2a | 0x101 | 0xaf) => {}
                Some(t) => {
                    force = match t {
                        0x32 => 18,
                        0x44 => 24,
                        0x11e => 36,
                        7 => 12,
                        _ if item_type_is_metal(ItemType(t)) => 8,
                        _ => 4,
                    };
                    h.increment_usage_of_interaction_item(b, true);
                }
            }
            let npc = b.bh.borrow().tapped_npc.clone();
            if let Some(n) = &npc {
                let ot = n.object_type();
                if !c.is_net && !c.ctl {
                    if ot == 0x27 && n.as_harmable().map_or(false, |hm| hm.will_die_if_hit_by_force(force)) {
                        c.achievement("grp.troll");
                    } else if ot == 0x33 && n.as_harmable().map_or(false, |hm| hm.will_die_if_hit_by_force(force)) {
                        c.achievement("grp.scorpion");
                    }
                }
            }
            if c.expert {
                force /= 2;
            }
            if let Some(n) = &npc {
                if let Some(hm) = n.as_harmable() {
                    hm.hit_with_force_blockhead(n, force, Some(c.me));
                }
            }
            b.bh.borrow_mut().chasing_npc = false;
        }
    }
}

fn u6_fire(c: &Ctx, prev: f32, timer: f32) {
    let b = c.b;
    let h = &c.h;
    match t_rule(prev, timer, 0.15, 0.3) {
        None => {}
        Some(false) => b.stop_interacting(),
        Some(true) => {
            let pos = c.pos();
            let (lk, item, npc, crouching) = {
                let f = b.bh.borrow();
                (
                    f.last_known_npc_position,
                    f.interaction_item.clone(),
                    f.tapped_npc.clone(),
                    f.state.animation_type == 23 || f.state.animation_type == 6,
                )
            };
            let dx = c.wrapdx(lk.x.wrapping_sub(pos.x));
            let dy = lk.y.wrapping_sub(pos.y);
            if dx < 12 && -12 < dx && dy < 10 && -12 < dy {
                // bow.wav: CLIENT-ONLY
                let ity = item.as_ref().map_or(0, |i| i.borrow().item_type.0);
                if !c.is_net && !c.ctl {
                    let mut force = 2.0f32;
                    if ity == 0x101 {
                        force = 10.0f32;
                        c.achievement("grp.poison");
                    }
                    c.achievement("grp.bow");
                    if let Some(n) = &npc {
                        let ot = n.object_type();
                        let fi = f32_to_i32(force);
                        if ot == 0x27 && n.as_harmable().map_or(false, |hm| hm.will_die_if_hit_by_force(fi)) {
                            c.achievement("grp.troll");
                        } else if ot == 0x33 && n.as_harmable().map_or(false, |hm| hm.will_die_if_hit_by_force(fi)) {
                            c.achievement("grp.scorpion");
                        }
                    }
                }
                let from = Vector2 {
                    x: (pos.x as f32) + 0.5f32,
                    y: (pos.y as f32) + 0.5f32 + (if crouching { 0.0f32 } else { 0.5f32 }),
                };
                let center = npc
                    .as_ref()
                    .and_then(|n| n.as_harmable().map(|hm| hm.center()))
                    .unwrap_or(Vector2 { x: 0.0, y: 0.0 });
                let to = Vector2 { x: center.x, y: center.y + 0.5f32 };
                h.fire_projectile_from_to_at_fire_item_type_firer(
                    from,
                    to,
                    npc.as_ref(),
                    if ity == 0x101 { 0x100 } else { 0x29 },
                    Some(c.me),
                );
                h.increment_usage_of_interaction_item(b, true);
            }
            b.bh.borrow_mut().chasing_npc = false;
        }
    }
}

// ====================================================================================================
// U7. Moving along the path, or chasing (7493..8640)
// ====================================================================================================

fn u7(c: &Ctx, coffee_dt: &mut f32) {
    let b = c.b;
    let (qsize, next_action_delay, chasing_npc, chasing_ride) = {
        let f = b.bh.borrow();
        (f.current_net_queue_size, f.next_action_delay, f.chasing_npc, f.chasing_ride_object)
    };
    let mut delay = 0.2f32;
    if c.is_net && qsize == 0 {
        delay = 0.5f32;
    }
    if !(delay <= next_action_delay || chasing_npc || chasing_ride) {
        b.bh.borrow_mut().next_action_delay += *coffee_dt;
        return;
    }
    let mut need_update = true;
    // ---- travel fraction
    {
        let mut f = b.bh.borrow_mut();
        let mut hf = (f.state.happiness + f.state.health) * 0.4f32 + 0.4f32;
        if f.traverse_type as u32 == tt::FALL || f.traverse_type as u32 == tt::FLY {
            hf = 1.0f32;
            *coffee_dt = c.dt;
        }
        let ndf = if c.is_net { (f.current_net_queue_size as f32) * 0.2f32 + 0.9f32 } else { 1.0f32 };
        let mut of = *coffee_dt * f.travel_speed * hf * ndf;
        if f.traverse_type as u32 == tt::ELEVATOR {
            of = ((c.dt * f.travel_speed) as f64 * 2.0f64) as f32;
        }
        f.travel_fraction += if 1.0f32 <= of { 1.0f32 } else { of };
        let mut tfn = f.travel_fraction;
        if 1.0f32 <= tfn {
            tfn -= 1.0f32;
        }
        if hf < 1.0f32 {
            let half = (1.0f32 - hf) * 0.5f32;
            tfn = clamp((tfn - half) / (1.0f32 - half * 2.0f32), 0.0, 1.0);
        }
        f.travel_fraction_normalized = tfn;
    }
    if 1.0f32 <= b.bh.borrow().travel_fraction {
        u7_step(c, coffee_dt, &mut need_update);
    }
    if need_update {
        u7_interpolate(c);
    }
}

/// `td = door/trap/gate(tile) || door(tile above sq)`; if so `setDoorAtPos:sq toOpen:open direction:dir` with
/// `dir = -1`, or `1` when `(door or gate in tile) && fromSquare.x < toSquare.x` (`x_lt`).
fn door_dir_and_open(c: &Ctx, tile: Option<TileRef>, sq: IntPair, x_lt: bool, open: bool) {
    let Some(world) = c.world.as_ref() else { return };
    let above = tile_at(world, IntPair { x: sq.x, y: sq.y.wrapping_add(1) });
    let td = tile_contains_door(tile)
        || tile_contains_trap_door(tile)
        || tile_contains_gate(tile)
        || tile_contains_door(above);
    if td {
        let mut dir = -1;
        if (tile_contains_door(tile) || tile_contains_gate(tile)) && x_lt {
            dir = 1;
        }
        c.h.set_door_at_pos_to_open_direction(sq, open, dir);
    }
}

fn u7_step(c: &Ctx, _coffee_dt: &mut f32, need_update: &mut bool) {
    let b = c.b;
    let h = &c.h;
    let me = c.me;
    let world = c.world.clone();
    {
        b.bh.borrow_mut().travel_fraction -= 1.0f32;
    }
    // drop-ignore window
    {
        let (fs, ts) = {
            let f = b.bh.borrow();
            (f.from_square, f.to_square)
        };
        if fs != ts {
            let r = h.free_block_pickup_radius(b);
            let loc = b.bh.borrow().ignoring_freeblocks_due_to_drop_location;
            let dy = (ts.y.wrapping_sub(loc.y)).wrapping_abs();
            let dx = c.wrapdx(ts.x.wrapping_sub(loc.x)).wrapping_abs();
            if (dy as f32) <= r + 0.1f32 && (dx as f32) <= r + 0.1f32 {
                // keep it
            } else {
                b.bh.borrow_mut().ignoring_freeblocks_due_to_drop = false;
            }
        }
    }
    let to_square = b.bh.borrow().to_square;
    DynamicObject::update_position(b, me, to_square);
    {
        let fp = Vector2 { x: ((to_square.x as f64) + 0.5f64) as f32, y: to_square.y as f32 };
        b.bh.borrow_mut().current_movement_was_initiated_with_d_pad = false;
        b.base.borrow_mut().float_pos = fp;
    }
    let (from_tile, to_tile, from_sq) = {
        let f = b.bh.borrow();
        (f.from_tile, f.to_tile, f.from_square)
    };
    if from_tile != to_tile {
        // doors, trap doors and gates close behind
        if let Some(w) = &world {
            let mut fd =
                tile_contains_door(from_tile) || tile_contains_trap_door(from_tile) || tile_contains_gate(from_tile);
            if !fd {
                let above = tile_at(w, IntPair { x: from_sq.x, y: from_sq.y.wrapping_add(1) });
                if above.is_some() {
                    fd = tile_contains_door(above);
                }
            }
            if fd {
                h.set_door_at_pos_to_open_direction(from_sq, false, 0);
            }
        }
        // footstep ping-pong (snowStep.wav: CLIENT-ONLY)
        {
            let mut f = b.bh.borrow_mut();
            if to_square.y == f.from_square.y {
                f.footstep_ping_pong = !f.footstep_ping_pong;
            } else {
                f.footstep_ping_pong = true;
            }
        }
        // cactus
        let tkf = b.bh.borrow().traverse_to_key_frame;
        if !c.is_net {
            if let Some(tt_) = to_tile {
                if tt_.contents() == 0x2b && tkf as u32 == kf::HANG_BACK && tt_.foreground_contents() != 0x42 {
                    b.suffer_damage_is_simulation_recoil(0.01f32, c.sim, false);
                    if c.sim_or_ctl {
                        let k = c.killed_or("hurt");
                        c.event(4, SimulationEventExtra::Text(format!("{} by a cactus", k)));
                    }
                }
            }
        }
    }
    {
        let mut f = b.bh.borrow_mut();
        f.prev_from_square = f.from_square;
        f.from_square = f.to_square;
        f.from_tile = f.to_tile;
        f.traverse_from_key_frame = f.traverse_to_key_frame;
    }
    let mut arrived = true;
    let mut next_point_index: i32 = 0;
    let mut interact_tile: Option<TileRef> = None;
    let mut tile_interaction_type: u32 = it::NONE;

    // ---- chasing an NPC
    if b.bh.borrow().chasing_npc {
        let tapped = b.bh.borrow().tapped_npc.clone();
        match tapped {
            Some(t) if !t.needs_removed() => {
                let np = t.pos();
                let pos = c.pos();
                let (interacting, goal, lk, crouching) = {
                    let f = b.bh.borrow();
                    (
                        f.state.interacting != 0,
                        f.state.goal_interaction_type_while_walking,
                        f.last_known_npc_position,
                        f.state.animation_type == 23 || f.state.animation_type == 6,
                    )
                };
                let in_goals = matches!(goal, 10 | 12 | 19 | 27 | 28 | 25 | 26 | 20);
                if (!interacting || !in_goals) && (np != lk || goal == 20) {
                    let max_d: i32 = if goal == 20 { 16 } else { 8 };
                    let d1 = c.wrapdx(lk.x.wrapping_sub(pos.x));
                    let d2 = c.wrapdx(np.x.wrapping_sub(pos.x));
                    if (-max_d < d1 && d1 < max_d) || (-max_d < d2 && d2 < max_d) {
                        b.bh.borrow_mut().last_known_npc_position = np;
                        if goal == 20 {
                            let dy = np.y.wrapping_sub(pos.y);
                            let y_off = if crouching { 0 } else { 1 };
                            let los = world.as_ref().map_or(false, |w| {
                                clear_line_of_sight_between_tiles(
                                    IntPair { x: pos.x, y: pos.y.wrapping_add(y_off) },
                                    IntPair { x: np.x, y: np.y.wrapping_add(1) },
                                    w,
                                )
                            });
                            if -12 < d2 && d2 < 12 && -12 < dy && dy < 10 && los {
                                b.bh.borrow_mut().path = None;
                                let mut ip = IntPair { x: pos.x.wrapping_add(1), y: pos.y.wrapping_add(y_off) };
                                if np.x < pos.x {
                                    ip.x = pos.x.wrapping_sub(1);
                                }
                                if let Some(w) = &world {
                                    let idx = world_index_at_world_pos(ip, w);
                                    let tl = tile_at(w, ip);
                                    b.start_interacting_with_tile_at_index_tile_interaction_type(
                                        idx,
                                        tl,
                                        it::FIRE_RANGED_WEAPON,
                                    );
                                }
                            }
                        } else {
                            b.bh.borrow_mut().path_needs_recalculated = true;
                        }
                    }
                }
            }
            _ => {
                let mut f = b.bh.borrow_mut();
                f.chasing_npc = false;
                f.path_needs_recalculated = true;
            }
        }
    } else if b.bh.borrow().chasing_ride_object {
        let tapped = b.bh.borrow().tapped_ride_object.clone();
        match tapped {
            Some(t) if !t.needs_removed() => {
                let bp = t.pos();
                let pos = c.pos();
                let (interacting, goal, lk) = {
                    let f = b.bh.borrow();
                    (
                        f.state.interacting != 0,
                        f.state.goal_interaction_type_while_walking,
                        f.last_known_ride_object_position,
                    )
                };
                let skip = (interacting && matches!(goal, 17 | 18 | 21 | 22)) || bp == lk;
                if !skip {
                    let d1 = c.wrapdx(lk.x.wrapping_sub(pos.x));
                    let d2 = c.wrapdx(bp.x.wrapping_sub(pos.x));
                    if (-8 < d1 && d1 < 8) || (-8 < d2 && d2 < 8) {
                        let mut f = b.bh.borrow_mut();
                        f.last_known_ride_object_position = bp;
                        f.path_needs_recalculated = true;
                    }
                }
            }
            _ => {
                let mut f = b.bh.borrow_mut();
                f.chasing_ride_object = false;
                f.path_needs_recalculated = true;
            }
        }
    }

    // ---- next path step
    let want_step = {
        let f = b.bh.borrow();
        !f.path_needs_recalculated
            && (f.path.as_ref().map_or(0, |p| p.len()) != 0 || (c.is_net && f.current_net_queue_size > 0))
    };
    if want_step {
        b.set_update_needs_to_be_sent(true);
        let (old_td, old_tt) = {
            let mut f = b.bh.borrow_mut();
            let o = (f.terrain_difficulty, f.traverse_type);
            f.terrain_difficulty = 10;
            o
        };
        #[allow(unused_assignments)]
        let mut next_next = c.pos();
        let mut next = IntPair { x: 0, y: 0 };
        let mut next_tile: Option<TileRef> = None;
        let mut mv = true;
        if !c.is_net {
            // ---- local step
            let step = {
                let f = b.bh.borrow();
                f.path.as_ref().and_then(|p| p.last().cloned())
            };
            let geti = |v: &Option<PlistValue>, k: &str| -> i32 {
                v.as_ref().and_then(|d| d.object_for_key(k)).map_or(0, |x| x.int_value())
            };
            let idx = geti(&step, "index");
            {
                let mut f = b.bh.borrow_mut();
                f.traverse_type = geti(&step, "traverseType");
                f.traverse_to_key_frame = geti(&step, "traverseToKeyFrame");
                f.terrain_difficulty = geti(&step, "terrainDifficulty");
                if f.traverse_type as u32 == tt::FALL && old_tt as u32 == tt::FALL {
                    let v = old_td.wrapping_add(5);
                    f.terrain_difficulty = if v < 150 { v } else { 150 };
                }
                if let Some(p) = f.path.as_mut() {
                    p.pop();
                }
            }
            next_point_index = idx;
            if let Some(w) = &world {
                next_tile = tile_at_world_index_loaded(idx, w);
                next = get_world_pos_for_world_index(idx, w);
            }
            next_next = next;
            let remaining_last = b.bh.borrow().path.as_ref().and_then(|p| p.last().cloned());
            if let Some(l) = &remaining_last {
                if let Some(w) = &world {
                    next_next =
                        get_world_pos_for_world_index(l.object_for_key("index").map_or(0, |x| x.int_value()), w);
                }
            }
            {
                let tkf = b.bh.borrow().traverse_to_key_frame;
                if let Some(nt) = next_tile {
                    if nt.contents() == 0x2b && tkf as u32 == kf::HANG_BACK && nt.foreground_contents() != 0x42 {
                        let mut f = b.bh.borrow_mut();
                        f.terrain_difficulty /= 10;
                    }
                }
            }
            let path_empty = b.bh.borrow().path.as_ref().map_or(true, |p| p.is_empty());
            if path_empty {
                // final step
                if next == c.pos() {
                    mv = false;
                    let mut f = b.bh.borrow_mut();
                    f.traverse_from_key_frame = f.traverse_to_key_frame;
                }
                let (lpf, lpm) = {
                    let f = b.bh.borrow();
                    (f.last_path_was_falling, f.last_path_was_move_only)
                };
                if !lpf && !lpm {
                    tile_interaction_type = b.bh.borrow().state.goal_interaction_type_while_walking;
                    let face =
                        if matches!(tile_interaction_type, it::CRAFT | it::RIDE_TRAIN_CAR | it::USE_INTERACTIVE_OBJECT)
                        {
                            0
                        } else {
                            c.extra_face()
                        };
                    let item = b.bh.borrow().interaction_item.clone();
                    let nit = c.interaction_type_checked(next_tile, next, item.as_ref(), face);
                    if tile_interaction_type != it::NONE || nit != it::NONE {
                        mv = false;
                        let (interacting, itile, ride_none) = {
                            let f = b.bh.borrow();
                            (f.state.interacting != 0, f.interacting_tile, f.ride_object.is_none())
                        };
                        if (!interacting || itile != next_tile)
                            && (!matches!(tile_interaction_type, it::RIDE | it::RIDE_BOAT | it::RIDE_TRAIN_CAR)
                                || ride_none)
                        {
                            interact_tile = next_tile;
                        }
                    }
                }
            } else {
                // more path remains: elevator bell is CLIENT-ONLY, the achievement is not
                let nn_kf = remaining_last
                    .as_ref()
                    .and_then(|d| d.object_for_key("traverseToKeyFrame"))
                    .map_or(0, |x| x.int_value());
                let tkf = b.bh.borrow().traverse_to_key_frame;
                let nn_elev = nn_kf as u32 == kf::STAND_ELEVATOR;
                let cur_elev = tkf as u32 == kf::STAND_ELEVATOR;
                if nn_elev != cur_elev && nn_elev {
                    c.guarded_achievement("grp.elevator_mj");
                }
            }
        } else {
            // ---- replica: consume the net queue filled by remoteUpdate
            {
                let mut f = b.bh.borrow_mut();
                next = f.net_next_to_square[0];
                f.state.animation_type = f.next_animation_type[0] as u32;
                f.state.sub_animation_type = f.next_sub_animation_type[0] as u32;
            }
            if let Some(w) = &world {
                next_tile = tile_at(w, next);
            }
            {
                let mut f = b.bh.borrow_mut();
                if next == f.to_square && f.next_interacting[0] == (f.state.interacting != 0) {
                    f.traverse_type = 0;
                    mv = false;
                    f.traverse_from_key_frame = f.traverse_to_key_frame;
                } else {
                    f.traverse_type = f.next_traverse_type[0];
                    f.traverse_to_key_frame = f.next_traverse_to_key_frame[0];
                    f.terrain_difficulty = f.next_terrain_difficulty[0];
                    if next == f.to_square {
                        mv = false;
                    }
                }
            }
            let (ni0, interacting) = {
                let f = b.bh.borrow();
                (f.next_interacting[0], f.state.interacting != 0)
            };
            if ni0 && !interacting {
                let t0 = {
                    let mut f = b.bh.borrow_mut();
                    f.gather_timer_left_over = 0.0;
                    f.state.interaction_timer = 0.0;
                    f.state.interacting = 1;
                    f.state.interacting_square = f.next_interaction_square[0];
                    f.state.gather_speed = 1.0f32;
                    f.sound_timer = 0.0;
                    let t = f.next_interaction_type[0] as u32;
                    f.state.interaction_type = t;
                    f.state.goal_interaction_type_while_walking = t;
                    t
                };
                let isq = b.bh.borrow().state.interacting_square;
                if t0 == it::USE_INTERACTIVE_OBJECT {
                    let o = h.interaction_object_at_pos(isq);
                    let mut f = b.bh.borrow_mut();
                    f.interaction_object = o;
                    f.interaction_workbench = None;
                } else if t0 == it::CRAFT {
                    let wbo = h.workbench_at_pos(isq);
                    let mut f = b.bh.borrow_mut();
                    f.interaction_workbench = wbo;
                    f.interaction_object = None;
                } else {
                    let mut f = b.bh.borrow_mut();
                    f.interaction_object = None;
                    f.interaction_workbench = None;
                }
                h.update_gather_speed_and_animation_for_current_interation_and_item(b);
            } else if !ni0 && interacting {
                let mut f = b.bh.borrow_mut();
                f.interaction_object = None;
                f.interaction_workbench = None;
                f.interaction_item = None;
                f.interaction_item_index = -1;
                f.interaction_item_sub_index = -1;
                f.state.interacting = 0;
                f.state.interaction_type = 0;
                f.state.goal_interaction_type_while_walking = 0;
                f.tapped_npc = None;
                f.chasing_npc = false;
                f.chasing_ride_object = false;
            }
            // shift the queue down by one (all 9 arrays, for i < currentNetQueueSize-1)
            {
                let mut f = b.bh.borrow_mut();
                let n = (f.current_net_queue_size - 1).max(0) as usize;
                for i in 0..n.min(3) {
                    f.net_next_to_square[i] = f.net_next_to_square[i + 1];
                    f.next_terrain_difficulty[i] = f.next_terrain_difficulty[i + 1];
                    f.next_traverse_type[i] = f.next_traverse_type[i + 1];
                    f.next_traverse_to_key_frame[i] = f.next_traverse_to_key_frame[i + 1];
                    f.next_interacting[i] = f.next_interacting[i + 1];
                    f.next_interaction_square[i] = f.next_interaction_square[i + 1];
                    f.next_interaction_type[i] = f.next_interaction_type[i + 1];
                    f.next_animation_type[i] = f.next_animation_type[i + 1];
                    f.next_sub_animation_type[i] = f.next_sub_animation_type[i + 1];
                }
                f.current_net_queue_size -= 1;
                next_next = if f.current_net_queue_size < 1 { next } else { f.net_next_to_square[0] };
            }
        }
        // ---- the move itself
        let traverse_type = b.bh.borrow().traverse_type as u32;
        if mv && traverse_type != tt::INACCESSIBLE {
            arrived = false;
            {
                let mut f = b.bh.borrow_mut();
                f.state.interacting = 0;
                f.state.interaction_type = 0;
                if !c.is_net {
                    f.state.sub_animation_type = 0;
                }
            }
            if !c.is_net {
                b.set_update_needs_to_be_sent(true);
            }
            {
                let mut f = b.bh.borrow_mut();
                if traverse_type == tt::FLY {
                    f.terrain_difficulty = f64_to_i32((f.terrain_difficulty as f64) * 1.5f64);
                }
                f.travel_speed = 40.0f32 / (f.terrain_difficulty as f32);
            }
            if traverse_type == tt::FALL {
                let (td, tkf) = {
                    let mut f = b.bh.borrow_mut();
                    f.travel_speed = ((f.terrain_difficulty as f32) + 20.0f32) / 8.0f32;
                    if f.travel_speed < 1.0f32 {
                        f.travel_speed = 1.0f32;
                    }
                    (f.terrain_difficulty, f.traverse_to_key_frame)
                };
                if !c.is_net && tkf as u32 == kf::STAND {
                    let d = ((td as f32) - 50.0f32) / 50.0f32;
                    if 0.0f32 < d {
                        b.suffer_damage_is_simulation_recoil(d, c.sim, true);
                        if c.sim_or_ctl {
                            let k = c.killed_or("injured");
                            c.event(4, SimulationEventExtra::Text(format!("{} in a fall", k)));
                        }
                    }
                }
            }
            if traverse_type == tt::WALK || traverse_type == tt::STAIRS {
                let fur = if c.expert { 1.1f32 } else { 1.2f32 };
                let south = if c.expert { 1.2f32 } else { 1.5f32 };
                let shoes = b.bh.borrow().shoes_item_type;
                let mut f = b.bh.borrow_mut();
                if next_tile.map_or(true, |t| t.type_index() != 5) {
                    if shoes == 0x7c || shoes == 0x7f {
                        f.travel_speed *= fur;
                    } else if shoes == 0x8f {
                        f.travel_speed *= south;
                    }
                } else if shoes == 0x7f {
                    f.travel_speed *= fur;
                } else if shoes == 0x8f {
                    f.travel_speed *= south;
                } else {
                    let pcl = next_tile.map_or(0, |t| t.partial_content_left());
                    f.travel_speed *= 1.0f32 - ((pcl as f32) / 255.0f32) * 0.5f32;
                }
            }
            {
                let mut f = b.bh.borrow_mut();
                f.to_square = next;
                f.next_to_square = next_next;
                f.to_tile = next_tile;
            }
            let (ft, tt_tile) = {
                let f = b.bh.borrow();
                (f.from_tile, f.to_tile)
            };
            if ft != tt_tile && tt_tile.is_some() {
                if traverse_type == tt::STAIRS {
                    c.guarded_achievement("grp.staircase");
                }
                let (tkf, fkf, ride_none, fsq, tsq) = {
                    let f = b.bh.borrow();
                    (
                        f.traverse_to_key_frame as u32,
                        f.traverse_from_key_frame as u32,
                        f.ride_object.is_none(),
                        f.from_square,
                        f.to_square,
                    )
                };
                if (tkf != kf::STAND_ELEVATOR || fkf != kf::STAND_ELEVATOR) && ride_none {
                    door_dir_and_open(c, tt_tile, tsq, fsq.x < tsq.x, true);
                }
                if (tkf == kf::STAND_ELEVATOR) != (fkf == kf::STAND_ELEVATOR) {
                    let p = if tkf == kf::STAND_ELEVATOR { tsq } else { fsq };
                    h.open_elevator_at_pos(p);
                    if !c.is_net && tkf == kf::STAND_ELEVATOR {
                        h.elevator_motor_for_shaft_at_pos_use_power(p);
                    }
                    if fkf == kf::STAND_ELEVATOR {
                        // door check on fromTile as above, `setDoorAtPos:fromSquare toOpen:YES`
                        let ftile = b.bh.borrow().from_tile;
                        door_dir_and_open(c, ftile, fsq, fsq.x < tsq.x, true);
                    }
                }
                if !c.is_net {
                    if tkf == kf::STAND_ELEVATOR {
                        let go = {
                            let mut f = b.bh.borrow_mut();
                            f.elevator_power_use_counter += 1;
                            if 9 < f.elevator_power_use_counter {
                                f.elevator_power_use_counter = 0;
                                true
                            } else {
                                false
                            }
                        };
                        if go {
                            h.elevator_motor_for_shaft_at_pos_use_power(tsq);
                        }
                    } else {
                        b.bh.borrow_mut().elevator_power_use_counter = 0;
                    }
                }
            }
            if !c.is_net {
                let mut f = b.bh.borrow_mut();
                let travel_speed = f.travel_speed;
                let new_anim = match traverse_type {
                    1 => Some(1),
                    26 => Some(1),
                    4 => Some(4),
                    27 => Some(31),
                    24 => Some(26),
                    3 => Some(6),
                    5 => Some(2),
                    6 => Some(3),
                    13 => Some(9),
                    14 => Some(10),
                    15 => Some(11),
                    16 => Some(12),
                    19 => Some(13),
                    8 => Some(15),
                    11 => Some(16),
                    12 => Some(17),
                    20 => Some(18),
                    21 => Some(19),
                    22 => Some(20),
                    23 => Some(21),
                    17 => Some(if travel_speed <= 4.0f32 { 0 } else { 25 }),
                    7 => Some(22),
                    2 => Some(5),
                    28 => Some(32),
                    _ => None,
                };
                if let Some(a) = new_anim {
                    f.state.animation_type = a;
                }
            }
        }
    }
    // ---- arrival
    let using_item = b.bh.borrow().using_item;
    if arrived || interact_tile.is_some() || using_item {
        {
            let mut f = b.bh.borrow_mut();
            f.travel_fraction = 1.0f32;
            f.travel_speed = 4.0f32;
        }
        *need_update = false;
        b.set_update_needs_to_be_sent(true);
        {
            let mut f = b.bh.borrow_mut();
            if f.traverse_to_key_frame as u32 == kf::INACCESSIBLE || interact_tile.is_some() {
                f.traverse_to_key_frame = f.traverse_from_key_frame;
                if interact_tile.is_some() && f.traverse_to_key_frame as u32 == kf::STAND_ELEVATOR {
                    f.traverse_to_key_frame = 3;
                }
            }
        }
        b.update_animation();
    }
    if interact_tile.is_some() {
        b.start_interacting_with_tile_at_index_tile_interaction_type(
            next_point_index,
            interact_tile,
            tile_interaction_type,
        );
    } else if using_item {
        b.start_interacting_with_tile_at_index_tile_interaction_type(next_point_index, None, tile_interaction_type);
    }
    let _ = (me, h);
}

/// Interpolate `floatPos` (state the server keeps: pickups, projectiles).
fn u7_interpolate(c: &Ctx) {
    let b = c.b;
    let f = b.bh.borrow();
    let anim = f.state.animation_type;
    let (fs, ts) = (f.from_square, f.to_square);
    let tf = f.travel_fraction;
    let tfn = f.travel_fraction_normalized;
    let (tkf, fkf, ttype) = (f.traverse_to_key_frame as u32, f.traverse_from_key_frame as u32, f.traverse_type as u32);
    drop(f);
    let mut from = Vector2 { x: ((fs.x as f64) + 0.5f64) as f32, y: fs.y as f32 };
    let to = Vector2 { x: ((ts.x as f64) + 0.5f64) as f32, y: ts.y as f32 };
    let mut x_l = tf;
    let mut y_l = tf;
    let mut y_off = 0.0f32;
    // sinf of (float)(((double)x * M_PI) * k)
    let s_k = |x: f32, k: f64| -> f32 { sinf((((x as f64) * M_PI) * k) as f32) };
    let s_pi = |x: f32| -> f32 { sinf(((x as f64) * M_PI) as f32) };
    // shared arc for 2, 3 and 22
    let arc = |l: f32, x_l: &mut f32, y_l: &mut f32, y_off: &mut f32, level_sets_y: bool| {
        if fs.y < ts.y {
            *y_l = s_k(l, 0.75) / 0.7f32;
        } else if ts.y < fs.y {
            *y_l = 1.0f32 - s_k(1.0f32 - l, 0.75) / 0.7f32;
        } else {
            *y_off = s_pi(l) * 0.4f32;
            if level_sets_y {
                *y_l = *x_l;
            }
        }
    };
    match anim {
        8 => {
            let l = min_f(tfn * 2.0f32, 1.0f32);
            x_l = l;
            y_l = l;
        }
        2 | 3 => {
            let l = if anim == 2 { min_f(tfn * 2.0f32, 1.0f32) } else { max_f((tfn - 0.5f32) * 2.0f32, 0.0f32) };
            x_l = l;
            y_l = l;
            arc(l, &mut x_l, &mut y_l, &mut y_off, false);
        }
        22 => {
            x_l = clamp(tfn * 3.0f32 - 1.0f32, 0.0, 1.0);
            let l = x_l;
            arc(l, &mut x_l, &mut y_l, &mut y_off, true);
        }
        1 => {
            if fs.y < ts.y && tkf != kf::STAIRS && ttype != tt::STAIRS && fkf != kf::STAIRS {
                y_l = s_k(tfn, 0.75) / 0.7f32;
            } else if ts.y < fs.y && tkf != kf::STAIRS && ttype != tt::STAIRS && fkf != kf::STAIRS {
                y_l = 1.0f32 - s_k(1.0f32 - tfn, 0.75) / 0.7f32;
            }
        }
        26 => {
            y_off = ((s_pi(tfn)) as f64 * 0.35f64) as f32;
            x_l = tfn;
        }
        9 | 10 => {
            let mut fr = tf * tf;
            if anim == 10 {
                fr = 1.0f32 - fr;
            }
            y_off = s_pi(fr) * -0.3f32;
            x_l = s_k(fr, 0.5);
            y_l = x_l;
        }
        11 | 12 => {
            let mut fr = tf * tf;
            if anim == 12 {
                fr = 1.0f32 - fr;
            }
            x_l = s_k(fr, 0.5);
            y_l = x_l;
            y_off = fr * -0.5f32;
        }
        4 => {
            if ts.x != fs.x {
                x_l = if x_l < 0.25f32 {
                    0.0
                } else if x_l <= 0.75f32 {
                    s_k((x_l - 0.25f32) * 2.0f32, 0.5)
                } else {
                    1.0
                };
            }
        }
        6 => y_off = -0.5f32,
        31 => {}
        13 | 14 => y_off = 0.2f32,
        15 => {
            y_l = tfn;
            if y_l < 0.25f32 {
                y_l = (s_k(y_l * 4.0f32, 0.75) / 0.7f32) * 0.25f32;
            }
        }
        16 => {
            let v = tfn;
            let l = min_f(v * 2.0f32, 1.0f32);
            x_l = l;
            y_l = l * 0.5f32 + (s_k(l, 0.75) / 0.7f32) * 0.5f32;
            y_off = v * 0.2f32;
        }
        17 => {
            let v = 1.0f32 - tfn;
            y_l = max_f(0.0, (v - 0.2f32) / 0.8f32);
            x_l = min_f(1.0, v / 0.5f32);
            let w = min_f(1.0, (1.0f32 - v) / 0.75f32);
            y_off = w * 0.2f32;
        }
        18 | 19 => {
            let mut fr = tf;
            if anim == 18 {
                fr = 1.0f32 - fr;
            }
            y_l = max_f(0.0, (fr - 0.2f32) / 0.8f32);
            x_l = min_f(1.0, fr / 0.3f32);
            let w = min_f(1.0, (1.0f32 - fr) / 0.75f32);
            y_off = w * 0.2f32;
        }
        20 | 21 => {
            let mut fr = tfn;
            if anim == 21 {
                fr = 1.0f32 - fr;
            }
            x_l = if 0.5f32 <= fr { powf((fr - 0.5f32) * 2.0f32, 0.5f32) } else { 0.0 };
            y_l = if 0.5f32 <= fr {
                if fr <= 0.75f32 {
                    s_k((fr - 0.5f32) * 4.0f32, 0.5)
                } else {
                    1.0
                }
            } else {
                0.0
            };
            y_off = fr * 0.2f32;
        }
        _ => {}
    }
    // x-wrap of `from`
    let w = c.w_tiles();
    if from.x - to.x <= ((w / 2) as f32) {
        if from.x - to.x < (((-w) / 2) as f32) {
            from.x += w as f32;
        }
    } else {
        from.x -= w as f32;
    }
    let fp = if matches!(anim, 10 | 12 | 18 | 21 | 17) {
        Vector2 { x: linear_interpolate(to.x, from.x, x_l), y: linear_interpolate(to.y, from.y, y_l) + y_off }
    } else {
        Vector2 { x: linear_interpolate(from.x, to.x, x_l), y: linear_interpolate(from.y, to.y, y_l) + y_off }
    };
    b.base.borrow_mut().float_pos = fp;
}

// ====================================================================================================
// U8. Common tail (10140..10284)
// ====================================================================================================

fn u8(c: &Ctx) {
    let b = c.b;
    let h = &c.h;
    let ride = b.bh.borrow().ride_object.clone();
    if let Some(r) = ride {
        if !c.sim {
            if let Some(rid) = r.as_ridable() {
                let rp = rid.rider_pos_for_blockhead(c.me);
                let fp = Vector2 { x: rp[0], y: rp[1] };
                b.base.borrow_mut().float_pos = fp;
                b.bh.borrow_mut().render_pos = fp;
                let np = IntPair { x: f32_to_i32(rp[0]), y: f32_to_i32(rp[1]) };
                if np != b.pos() {
                    DynamicObject::update_position(b, c.me, np);
                }
                let t = c.world.as_ref().and_then(|w| tile_at(w, np));
                let mut f = b.bh.borrow_mut();
                f.next_to_square = np;
                f.to_square = np;
                f.prev_from_square = np;
                f.from_square = np;
                f.to_tile = t;
                f.from_tile = t;
            }
        }
    }
    // `isInJetPackFreeFlightMode && ![world dpadControl]` (dpadControl is constant NO)
    if b.bh.borrow().is_in_jet_pack_free_flight_mode {
        h.update_jet_motion(b, c.dt);
    }
    if !c.is_net && !c.sim {
        b.prepare_inventory_for_saving();
        // `[thisFramePickupRequests count] != 0`: nil on the server; `removeAllIndexes` is a nil message
        if let Some(s) = b.bh.borrow_mut().this_frame_pickup_requests.as_mut() {
            s.remove_all_indexes();
        }
    }
    if c.is_net {
        b.bh.borrow_mut().path_needs_recalculated = false;
    }
    // dismissJetPackUI: CLIENT-ONLY
    // jetFuelUsage drives `incrementFuelUsage` in U3: with jetSound == nil it is forced to 0.2f
    let (shirt, asleep, meditating, anim) = {
        let f = b.bh.borrow();
        (f.shirt_item_type, state_asleep(&f.state), state_meditating(&f.state), f.state.animation_type)
    };
    if shirt == 0x116
        && !asleep
        && !meditating
        && ((c.is_net && anim == 32) || (!c.is_net && h.jetpack_fuel_count(b) > 0))
    {
        b.bh.borrow_mut().jet_fuel_usage = 0.2f32;
    }
    // `else if jetSound != nil` (client only): never runs
}

#[cfg(test)]
pub(crate) mod cov_host;
#[cfg(test)]
mod coverage_tests;

#[cfg(test)]
mod tests {
    use super::super::blockhead::testutil::*;
    use super::*;
    use crate::items::inventory_item::InventoryItem;
    use crate::rng::glibc_rand::{rand, srand};
    use crate::types::structs::CustomRules;

    const DT: f32 = 0.2;

    fn mk() -> (Rc<World>, Rc<MockDw>, crate::objects::base::DynamicWorldRef, ObjRef, Rc<TestHost>) {
        let h = TestHost::install();
        let (world, dw, dwref) = setup();
        let o = new_bh(&world, &dwref, 0);
        // flush the constructor's `inventoryChangedThisFrameSlots[0]`
        bh_of(&o).prepare_inventory_for_saving();
        h.take();
        (world, dw, dwref, o, h)
    }

    fn tick(o: &ObjRef) {
        o.update_accurate_dt_is_simulation(o, DT, DT, false);
    }

    fn next_draw_after(seed: u32, n: usize) -> i32 {
        srand(seed);
        for _ in 0..n {
            rand();
        }
        rand()
    }

    fn steady(b: &Blockhead) {
        // no hunger tick, no sleep, full happiness inputs
        let mut f = b.bh.borrow_mut();
        f.state.food_pause_timer = 1.0e9;
        f.state.environment_temperature = 1.0;
        f.state.environment_exposure = 1.0;
        f.state.environment_light = 1.0;
        f.state.environment = 1.0;
        f.state.coffee_energy_timer = 0.0;
    }

    #[test]
    fn u1_paused_zero_queue_and_trade() {
        let (_w, _d, _dw, o, _h) = mk();
        let b = bh_of(&o);
        steady(b);
        // paused && !sim: nothing happens
        b.bh.borrow_mut().paused = true;
        b.bh.borrow_mut().state.energy = 1.0;
        tick(&o);
        assert_eq!(b.st().energy, 1.0);
        assert_eq!(b.st().coffee_energy_timer, 0.0);
        // paused but simulation: runs
        o.update_accurate_dt_is_simulation(&o, DT, DT, true);
        assert!(b.st().energy < 1.0);
        b.bh.borrow_mut().paused = false;
        // onTradeMission returns before anything else
        b.bh.borrow_mut().state.on_trade_mission = 1;
        let e = b.st().energy;
        tick(&o);
        assert_eq!(b.st().energy, e);
        b.bh.borrow_mut().state.on_trade_mission = 0;
        // interacting with a zero length queue stops the interaction
        b.bh.borrow_mut().state.interacting = 1;
        b.bh.borrow_mut().state.interaction_type = 2;
        tick(&o);
        assert_eq!(b.st().interacting, 0);
        // debugBOOL
        assert!(b.bh.borrow().debug_bool);
        // needsRemoved returns
        o.set_needs_removed(&o, true);
        let e = b.st().energy;
        tick(&o);
        assert_eq!(b.st().energy, e);
    }

    #[test]
    fn u1_watchdog_for_controlled_blockheads() {
        let (_w, _d, _dw, o, _h) = mk();
        let b = bh_of(&o);
        steady(b);
        b.bh.borrow_mut().is_client_blockhead_being_controlled_by_server = true;
        // sanity check timeout
        b.bh.borrow_mut().controlled_by_server_sanity_check_timeout = 7199.9;
        tick(&o);
        assert!(o.needs_removed());
        o.set_needs_removed(&o, false);
        b.bh.borrow_mut().controlled_by_server_sanity_check_timeout = 0.0;
        // inactivity: no actions -> shouldContinueSimulating false
        let mut n = 0;
        while !o.needs_removed() {
            tick(&o);
            n += 1;
            assert!(n < 100);
        }
        // the first tick (sanity check) also accumulated; 0.2f summed in f32 first exceeds 4.0f after k additions
        let mut acc = 0.0f32;
        let mut k = 0;
        while !(4.0f32 < acc) {
            acc += DT;
            k += 1;
        }
        assert_eq!(n, k - 1);
        // active again: reset
        o.set_needs_removed(&o, false);
        b.bh.borrow_mut().action_queue.push(Rc::new(()));
        b.bh.borrow_mut().state.interacting = 0;
        tick(&o);
        let f = b.bh.borrow();
        assert_eq!(f.controlled_by_server_inactivity_timeout, 0.0);
        assert!(!f.cancel_simulate_due_to_collapse);
    }

    /// : U2 regeneration.
    #[test]
    fn u2_regeneration() {
        let (world, _d, _dw, o, h) = mk();
        let b = bh_of(&o);
        world.set_start_portal_pos(IntPair { x: 3, y: 9 });
        {
            let mut f = b.bh.borrow_mut();
            f.state.regenerating = 1;
            f.state.regeneration_progress = 0.5;
            f.state.health = 0.0;
            f.state.death = 1.0;
        }
        // not finished: accumulates and ALWAYS returns (no inventory save tail)
        tick(&o);
        assert_eq!(b.st().regeneration_progress, 0.5f32 + DT / 600.0f32);
        assert!(h.take().is_empty());
        // fast forward is 20x slower
        world.set_fast_forward(true);
        let p = b.st().regeneration_progress;
        tick(&o);
        assert_eq!(b.st().regeneration_progress, p + (DT / 20.0f32) / 600.0f32);
        world.set_fast_forward(false);
        // replicas never finish
        b.bh.borrow_mut().state.regeneration_progress = 1.5;
        b.base.borrow_mut().is_net = true;
        tick(&o);
        assert_eq!(b.st().regenerating, 1);
        b.base.borrow_mut().is_net = false;
        // finish: 1792 draws, state reset, teleport
        b.bh.borrow_mut().state.regeneration_progress = 0.9999;
        srand(21);
        tick(&o);
        assert_eq!(rand(), next_draw_after(21, 1792));
        let s = b.st();
        assert_eq!((s.regenerating, s.regeneration_progress), (0, 0.0));
        assert_eq!(
            (s.health, s.death, s.drown_fraction, s.energy, s.fullness, s.happiness),
            (1.0, 0.0, 0.0, 1.0, 1.0, 1.0)
        );
        assert_eq!(
            (s.environment, s.environment_temperature, s.environment_exposure, s.environment_light),
            (1.0, 1.0, 1.0, 1.0)
        );
        assert_eq!((s.animation_type, s.heat, s.coffee_energy_timer, s.food_pause_timer), (0, 0.0, 0.0, 0.0));
        assert_eq!(b.pos(), IntPair { x: 3, y: 9 });
        assert_eq!(b.float_pos(), Vector2 { x: 3.5, y: 9.0 });
        let f = b.bh.borrow();
        assert_eq!(f.render_pos, Vector2 { x: 3.5, y: 9.0 });
        assert_eq!(f.from_square, IntPair { x: 3, y: 9 });
        assert_eq!(f.to_square, IntPair { x: 3, y: 9 });
        assert!(f.to_tile.is_some());
        drop(f);
        assert!(h.take().contains(&"ach:grp.death".to_string()));
        // a controlled blockhead records the away event instead of... in addition to
        b.bh.borrow_mut().state.regenerating = 1;
        b.bh.borrow_mut().state.regeneration_progress = 2.0;
        b.bh.borrow_mut().is_client_blockhead_being_controlled_by_server = true;
        tick(&o);
        let log = h.take();
        assert!(log.contains(&"sim:4:Text(\"regenerated and teleported back to the portal\")".to_string()));
        assert!(!log.contains(&"ach:grp.death".to_string()));
    }

    /// : U3 hunger recurrence and the rate matrix.
    #[test]
    fn u3_hunger() {
        let (world, _d, _dw, o, _h) = mk();
        let b = bh_of(&o);
        let run = |expert: bool, rules: Option<(i8, i8, i8)>, hat: u16| -> f32 {
            let mut f = b.bh.borrow_mut();
            f.state.fullness = 1.0;
            f.state.food_pause_timer = 0.0;
            f.hat_item_type = hat;
            f.state.health = 1.0;
            drop(f);
            world.set_expert_mode(expert);
            let mut r = CustomRules::default();
            if let Some((hunger, health, _)) = rules {
                r.custom_rules_enabled = 1;
                r.hunger = hunger;
                r.health = health;
            }
            world.set_custom_rules(r);
            for _ in 0..5 {
                {
                    let mut f = b.bh.borrow_mut();
                    f.state.food_pause_timer = 0.0;
                    f.state.environment_temperature = 1.0;
                    f.state.environment_exposure = 1.0;
                    f.state.environment_light = 1.0;
                    f.state.energy = 1.2;
                }
                tick(&o);
            }
            b.st().fullness
        };
        let rate_recurrence = |rate: f32| -> f32 {
            let mut full = 1.0f32;
            for _ in 0..5 {
                full = full - (DT / 1800.0f32) * rate;
            }
            full
        };
        assert_eq!(run(false, None, 0), rate_recurrence(1.0));
        assert_eq!(run(true, None, 0), rate_recurrence(2.0));
        assert_eq!(run(true, None, 0x8c), rate_recurrence(0.25));
        assert_eq!(run(false, Some((1, 0, 0)), 0), rate_recurrence(1.0));
        assert_eq!(run(true, Some((1, 0, 0)), 0), rate_recurrence(1.0));
        assert_eq!(run(false, Some((2, 0, 0)), 0), rate_recurrence(0.25));
        assert_eq!(run(false, Some((0, 0, 0)), 0), rate_recurrence(4.0));
        assert_eq!(run(false, Some((3, 0, 0)), 0), 1.0);
        // hat 0x8c outside expert keeps fullness at 1
        assert_eq!(run(false, None, 0x8c), 1.0);
        // asleep: no hunger
        b.bh.borrow_mut().state.sub_animation_type = 5;
        b.bh.borrow_mut().state.food_pause_timer = 0.0;
        b.bh.borrow_mut().state.fullness = 1.0;
        world.set_expert_mode(false);
        world.set_custom_rules(CustomRules::default());
        b.bh.borrow_mut().hat_item_type = 0;
        tick(&o);
        assert_eq!(b.st().fullness, 1.0);
    }

    #[test]
    fn u3_starvation_damage_and_collapse() {
        let (_w, _d, _dw, o, h) = mk();
        let b = bh_of(&o);
        steady(b);
        {
            let mut f = b.bh.borrow_mut();
            f.state.food_pause_timer = 0.0;
            f.state.fullness = -0.11;
            f.state.energy = 1.2;
        }
        tick(&o);
        let s = b.st();
        let dec = (DT / 900.0f32) * 0.5f32;
        assert_eq!(s.health, 1.0 - dec);
        assert!(b.bh.borrow().needs_harm_flash);
        // fullness < -0.25 clamps and requests a collapse
        {
            let mut f = b.bh.borrow_mut();
            f.state.food_pause_timer = 0.0;
            f.state.fullness = -0.3;
        }
        tick(&o);
        // clamped to -0.25 and shouldCollapse -> collapseIfPossible in U6a sets fullness = 0, energy >= 0.25
        assert_eq!(b.st().fullness, 0.0);
        assert_eq!(b.st().sub_animation_type, 6);
        assert!(b.bh.borrow().cancel_simulate_due_to_collapse);
        let _ = h;
        // health hitting zero accumulates death and dies at 1.0
        let (_w2, _d2, _dw2, o2, h2) = mk();
        let b2 = bh_of(&o2);
        steady(b2);
        {
            let mut f = b2.bh.borrow_mut();
            f.state.food_pause_timer = 0.0;
            f.state.fullness = -0.11;
            f.state.health = 0.00005;
            f.state.death = 0.9999;
            f.state.energy = 1.2;
        }
        tick(&o2);
        assert_eq!(b2.st().health, 0.0);
        assert_eq!(b2.st().regenerating, 1); // die()
        assert!(h2.take().iter().any(|l| l.starts_with("drop:")));
    }

    /// : happiness formula incl. rules 0/2/3.
    #[test]
    fn u3_happiness() {
        let (world, _d, _dw, o, _h) = mk();
        let b = bh_of(&o);
        let reference = |full: f32, energy: f32, env: f32, health: f32, pants: u16, expert: bool| -> f32 {
            let mn = |a: f64, b: f64| if a < b { a } else { b };
            let mx = |a: f64, b: f64| if a < b { b } else { a };
            let a = mn(1.0, full as f64 / 0.5);
            let e = mn(1.0, mx(0.0, (energy as f64 - 0.25) / 0.75));
            let bb = mn(1.0, e / 0.25);
            let c = mn(1.0, env as f64 / 0.75);
            let mut h = ((a + bb + c) / 3.0) as f32;
            h = if health <= h { health } else { h };
            if full < 0.25 {
                h = h * full * 4.0;
            }
            if (energy as f64 - 0.25) / 0.75 < 0.25 {
                h = (h as f64 * mx(0.0, (energy as f64 - 0.25) / 0.75) * 4.0) as f32;
            }
            if env < 0.25 {
                h = h * env * 4.0;
            }
            if pants == 0x8d {
                let c = if expert { 0.25f32 } else { 0.5 };
                h = if h <= c { c } else { h };
            }
            if h <= 0.0 {
                h = 0.0;
            }
            h
        };
        for (full, energy, health, pants, expert) in [
            (1.0f32, 1.2f32, 1.0f32, 0u16, false),
            (0.2, 1.2, 1.0, 0, false),
            (1.0, 0.3, 1.0, 0, false),
            (1.0, 0.3, 0.5, 0x8d, true),
            (0.1, 0.26, 0.9, 0x8d, false),
            (0.6, 0.5, 0.3, 0, true),
            // zero health, and (0.55) a low environment
            (1.0, 1.2, 0.0, 0, false),
            (0.55, 1.2, 1.0, 0, false),
        ] {
            world.set_expert_mode(expert);
            {
                let mut f = b.bh.borrow_mut();
                f.state.food_pause_timer = 1.0e9;
                f.state.fullness = full;
                f.state.energy = energy;
                f.state.health = health;
                f.state.environment_temperature = 1.0;
                f.state.environment_exposure = 1.0;
                f.state.environment_light = 1.0;
                f.state.environment = if full == 0.55 { 0.1 } else { 1.0 };
                f.pants_item_type = pants;
            }
            tick(&o);
            let env = b.st().environment;
            let energy_after = b.st().energy;
            let expect = reference(b.st().fullness, energy_after, env, b.st().health, pants, expert);
            assert_eq!(b.st().happiness, expect, "{full} {energy} {health} {pants:#x}");
        }
        // custom rules
        world.set_expert_mode(false);
        let rule_fns: [(i8, fn(f32) -> f32); 3] = [(3, |_| 1.0), (0, |h| h * h), (2, |h| (h as f64).sqrt() as f32)];
        for (rule, f_) in rule_fns {
            let mut r = CustomRules::default();
            r.custom_rules_enabled = 1;
            r.happiness = rule;
            r.hunger = 1;
            r.health = 2;
            world.set_custom_rules(r);
            {
                let mut f = b.bh.borrow_mut();
                f.state.food_pause_timer = 1.0e9;
                f.state.fullness = 0.4;
                f.state.energy = 1.2;
                f.state.health = 1.0;
                f.pants_item_type = 0;
                f.state.environment_temperature = 1.0;
                f.state.environment_exposure = 1.0;
                f.state.environment_light = 1.0;
                f.state.environment = 1.0;
            }
            tick(&o);
            let base = reference(b.st().fullness, b.st().energy, b.st().environment, 1.0, 0, false);
            assert_eq!(b.st().happiness, f_(base), "rule {rule}");
        }
    }

    #[test]
    fn u3_early_returns_skip_the_tail() {
        let (_w, _d, _dw, o, h) = mk();
        let b = bh_of(&o);
        steady(b);
        b.bh.borrow_mut().inventory_changed_this_frame_slots[0] = true;
        tick(&o);
        assert!(h.take().contains(&"invchanged:0:-1:false".to_string())); // the tail ran
        for (name, flip) in [("pathNeedsRecalculated", 0), ("waitingForPath", 1)] {
            {
                let mut f = b.bh.borrow_mut();
                f.inventory_changed_this_frame_slots[0] = true;
                if flip == 0 {
                    f.path_needs_recalculated = true;
                } else {
                    f.waiting_for_path = true;
                }
            }
            tick(&o);
            assert!(h.take().is_empty(), "{name}");
            let mut f = b.bh.borrow_mut();
            f.path_needs_recalculated = false;
            f.waiting_for_path = false;
        }
        // the net kind resets pathNeedsRecalculated and skips the tail's prepare
        b.base.borrow_mut().is_net = true;
        b.bh.borrow_mut().path_needs_recalculated = true;
        tick(&o);
        assert!(!b.bh.borrow().path_needs_recalculated);
        assert!(h.take().is_empty());
    }

    #[test]
    fn u3_sleep_heat_and_clothing() {
        let (_w, _d, _dw, o, h) = mk();
        let b = bh_of(&o);
        steady(b);
        // lying on the ground
        {
            let mut f = b.bh.borrow_mut();
            f.state.sub_animation_type = 6;
            f.state.energy = 0.1;
            f.state.health = 0.5;
        }
        tick(&o);
        assert_eq!(b.st().energy, 0.25 + DT / 1080.0f32);
        let k = b.st().health;
        let expect = ((0.5f32 as f64) + ((DT / 1080.0f32) as f64) * 0.25) as f32;
        assert_eq!(k, expect);
        // waking up at full energy: stopInteracting + pathNeedsRecalculated, with an away event for ctl
        {
            let mut f = b.bh.borrow_mut();
            f.state.energy = 1.0 - 0.0001;
            f.is_client_blockhead_being_controlled_by_server = true;
            f.action_queue.push(Rc::new(()));
        }
        h.take();
        tick(&o);
        assert_eq!(b.st().energy, 1.0);
        assert!(b.bh.borrow().path_needs_recalculated);
        assert_eq!(b.st().sub_animation_type, 0);
        assert!(h.take().contains(&"sim:0:None".to_string()));
        b.bh.borrow_mut().is_client_blockhead_being_controlled_by_server = false;
        b.bh.borrow_mut().action_queue.clear();
        b.bh.borrow_mut().path_needs_recalculated = false;
        // in a bed: soft bed multipliers
        let bed = stub(1, true);
        {
            let mut f = b.bh.borrow_mut();
            f.state.sub_animation_type = 5;
            f.state.energy = 1.0;
            f.state.health = 0.5;
            f.interaction_object = Some(bed);
        }
        tick(&o);
        // 1.0f < energy: energy = max (1.2 for a plain bed)
        assert_eq!(b.st().energy, 1.2);
        b.bh.borrow_mut().state.energy = 0.9;
        tick(&o);
        assert_eq!(b.st().energy, 0.9f32 + (DT / 630.0f32) * 1.0f32);
        // energy drains while awake; meditating does not; collapse at zero
        {
            let mut f = b.bh.borrow_mut();
            f.state.sub_animation_type = 0;
            f.interaction_object = None;
            f.state.energy = 1.0;
        }
        tick(&o);
        assert_eq!(b.st().energy, ((1.0f32 as f64) - (DT as f64) / 540.0f64) as f32);
        // heat decay
        b.bh.borrow_mut().state.heat = 50.0;
        tick(&o);
        assert_eq!(b.st().heat, 50.0f32 - (DT * 100.0f32 * (50.0f32 / 100.0f32 + 1.0f32)) / 900.0f32);
        b.bh.borrow_mut().state.heat = 0.0001;
        tick(&o);
        assert_eq!(b.st().heat, 0.0);
        // coffee timer always decrements
        b.bh.borrow_mut().state.coffee_energy_timer = 0.0;
        tick(&o);
        assert_eq!(b.st().coffee_energy_timer, -DT);
    }

    #[test]
    fn u3_clothing_timers_and_jet_fuel() {
        let (_w, _d, _dw, o, h) = mk();
        let b = bh_of(&o);
        steady(b);
        // idle stand: multiplier 0.25 for clothing, 1 for ice
        b.bh.borrow_mut().clothing_increment_timer = 263.6f32;
        b.bh.borrow_mut().clothing_ice_increment_timer = 263.6f32;
        tick(&o);
        // 263.6 + 0.2*0.25 = 263.65 < 263.671875: no increment; ice 263.8 > limit: yes
        let f = b.bh.borrow();
        assert_eq!(f.clothing_increment_timer, 263.6f32 + DT * 0.25f32);
        assert_eq!(f.clothing_ice_increment_timer, 263.6f32 + DT - 263.671875f32);
        drop(f);
        // fuel usage
        b.bh.borrow_mut().jet_fuel_usage = 0.2;
        b.bh.borrow_mut().fuel_usage_increment_timer = 0.0;
        tick(&o);
        assert_eq!(b.bh.borrow().fuel_usage_increment_timer, DT * (0.2f32 - 0.1f32));
        let _ = h;
        // jetFuelUsage set by U8 for a net jetpacker
        b.base.borrow_mut().is_net = true;
        {
            let mut f = b.bh.borrow_mut();
            f.jet_fuel_usage = 0.0;
            f.shirt_item_type = 0x116;
            f.state.animation_type = 32;
        }
        tick(&o);
        assert_eq!(b.bh.borrow().jet_fuel_usage, 0.2f32);
        // not for a sleeping one
        b.bh.borrow_mut().jet_fuel_usage = 0.0;
        b.bh.borrow_mut().state.sub_animation_type = 5;
        tick(&o);
        assert_eq!(b.bh.borrow().jet_fuel_usage, 0.0);
    }

    #[test]
    fn u3_fire_damage_cadence() {
        let (_w, _d, _dw, o, h) = mk();
        let b = bh_of(&o);
        steady(b);
        tile_on_fire(&b.bh.borrow().from_tile.unwrap(), 1);
        b.bh.borrow_mut().damage_delay_timer = 0.5;
        tick(&o);
        assert_eq!(b.bh.borrow().damage_delay_timer, 0.5 - DT);
        assert_eq!(b.health(), 1.0);
        tick(&o);
        tick(&o); // 0.5 - 0.6 <= 0 on the third tick
        assert_eq!(b.bh.borrow().damage_delay_timer, 1.0);
        assert_eq!(b.health(), 1.0 - 0.1);
        assert!(h.take().contains(&"armor:0.1".to_string()));
        tile_on_fire(&b.bh.borrow().from_tile.unwrap(), 0);
        // temperature path
        b.bh.borrow_mut().current_temperature = 120.0;
        b.bh.borrow_mut().damage_delay_timer = 0.01;
        tick(&o);
        let dmg = (((120.0f32 - 100.0f32) as f64) * 0.01f64) as f32;
        let dmg = if dmg < 0.1f32 { dmg } else { 0.1f32 };
        assert_eq!(b.bh.borrow().damage_delay_timer, 1.0);
        assert!(h.take().contains(&format!("armor:{}", dmg)));
    }

    fn tile_on_fire(t: &TileRef, v: u8) {
        t.set_on_fire(v);
    }

    /// : U4 idle draws exactly four rand() in order.
    #[test]
    fn u4_idle() {
        let (_w, _d, _dw, o, _h) = mk();
        let b = bh_of(&o);
        steady(b);
        b.bh.borrow_mut().idle_timer = 100.0;
        srand(31);
        tick(&o);
        assert_eq!(rand(), next_draw_after(31, 4));
        srand(31);
        let r: Vec<f32> = (0..4).map(|_| r01_from(rand())).collect();
        let f = b.bh.borrow();
        assert_eq!(f.random_idle_wait, r[0] * 10.0 + 5.0);
        assert_eq!(f.idle_timer, 0.0);
        assert_eq!(f.walk_timer, 0.0);
        assert_eq!(f.state.sub_animation_type, 4);
        assert_eq!(
            (f.random_animation_value_a, f.random_animation_value_b, f.random_animation_value_c),
            (r[1], r[2], r[3])
        );
        drop(f);
        // coffee: idle timer advances 4x
        {
            let mut f = b.bh.borrow_mut();
            f.state.sub_animation_type = 0;
            f.idle_timer = 0.0;
            f.random_idle_wait = 100.0;
            f.state.coffee_energy_timer = 5.0;
        }
        tick(&o);
        assert_eq!(b.bh.borrow().idle_timer, DT * 4.0);
        // a replica runs it too; interacting / animation 32 do not
        b.bh.borrow_mut().state.animation_type = 32;
        tick(&o);
        assert_eq!(b.bh.borrow().idle_timer, DT * 4.0);
    }

    fn dig_setup() -> (Rc<World>, ObjRef, Rc<TestHost>, crate::objects::base::DynamicWorldRef) {
        let (world, _d, dw, o, h) = mk();
        let b = bh_of(&o);
        steady(b);
        let t = tile(&world, 6, 7);
        t.set_type_index(1);
        t.set_contents(0);
        t.set_gather_progress(0);
        h.cit.set(it::DIG);
        h.item_available.set(true);
        {
            let mut f = b.bh.borrow_mut();
            f.state.interacting = 1;
            f.state.interaction_type = 1;
            f.state.interacting_square = IntPair { x: 6, y: 7 };
            f.interacting_tile = Some(t);
            f.state.gather_speed = 1.0;
            f.state.gather_timer = 0.5;
            f.interaction_item = Some(InventoryItem::new(ItemType(8), 0, 0, None, None));
            f.action_queue.push(Rc::new(()));
        }
        (world, o, h, dw)
    }

    /// : DIG gather tick: 32 particle draws before the increment branch, tile
    /// progress writes, network notification, usage.
    #[test]
    fn u6_dig_gather_progress_and_completion() {
        let (world, o, h, _dw) = dig_setup();
        let b = bh_of(&o);
        let t = tile(&world, 6, 7);
        srand(77);
        tick(&o);
        assert_eq!(rand(), next_draw_after(77, 32));
        assert_eq!(t.gather_progress(), 8);
        assert_eq!(b.bh.borrow().local_gather_progress, 8);
        assert_eq!(b.bh.borrow().gather_timer_left_over, 0.0);
        let log = h.take();
        assert_eq!(log, vec!["gather:6,7".to_string(), "usage:false".to_string()]);
        assert_eq!(b.st().interacting, 1);
        // the gather timer was reduced by 0.3 after being topped up
        assert!(b.st().gather_timer > 0.2 && b.st().gather_timer < 0.5 + 1.0);
        // accumulate until done: progress 8 + 8*k; remaining draws include the bonus particles (2 x count)
        t.set_gather_progress(250);
        b.bh.borrow_mut().local_gather_progress = 250;
        b.bh.borrow_mut().state.gather_timer = 0.5;
        srand(78);
        tick(&o);
        assert_eq!(rand(), next_draw_after(78, 32 + 2));
        let log = h.take();
        assert_eq!(
            log,
            vec!["ach:grp.mine".to_string(), "removetile:6,7:1:0:false:false".to_string(), "usage:false".to_string()]
        );
        assert_eq!(b.st().interacting, 0);
        assert!(b.bh.borrow().path_needs_recalculated);
        assert_eq!(b.bh.borrow().local_gather_progress, 0);
    }

    #[test]
    fn u6_dig_stops_without_item_or_wrong_type() {
        let (_world, o, h, _dw) = dig_setup();
        let b = bh_of(&o);
        h.item_available.set(false);
        tick(&o);
        assert_eq!(b.st().interacting, 0);
        assert!(h.take().is_empty());
        let (_world, o, h, _dw) = dig_setup();
        let b = bh_of(&o);
        h.cit.set(it::PLACE_ITEM); // not in {1, 23, 24, 4, 5}
        tick(&o);
        assert_eq!(b.st().interacting, 0);
        let _ = h;
    }

    #[test]
    fn u6_dig_timer_below_threshold_waits() {
        let (_world, o, h, _dw) = dig_setup();
        let b = bh_of(&o);
        b.bh.borrow_mut().state.gather_timer = 0.0;
        srand(5);
        tick(&o);
        // gatherTimer += 0.2 * ((1+1) * 0.4 + 0.4) = 0.24 < 0.3: nothing else
        assert_eq!(rand(), next_draw_after(5, 0));
        assert!(h.take().is_empty());
        assert!(b.st().gather_timer > 0.23 && b.st().gather_timer < 0.25);
    }

    #[test]
    fn u6_dig_magnet_and_magnetic_ore() {
        let (world, o, h, _dw) = dig_setup();
        let b = bh_of(&o);
        let t = tile(&world, 6, 7);
        t.set_contents(0x3f);
        b.bh.borrow_mut().interaction_item = Some(InventoryItem::new(ItemType(0xaf), 0, 0, None, None));
        tick(&o);
        let log = h.take();
        // onlyRemoveContents, done, increment 8; magnet usage is skipped unless onlyRemoveContents (it is)
        assert_eq!(log[0], "ach:grp.magnet");
        assert!(log.iter().any(|l| l.starts_with("removetile:6,7:")));
        assert!(log.iter().any(|l| l == "usage:false"));
        assert!(log.iter().any(|l| l.ends_with(":true:false")));
        assert_eq!(b.st().interacting, 0);
    }

    #[test]
    fn u6_place_stab_and_timing_rule() {
        let (world, _d, _dw, o, h) = mk();
        let b = bh_of(&o);
        steady(b);
        // STAB: effect fires exactly once when the timer crosses 0.15
        let npc = stub(1, true);
        h.item_available.set(true);
        {
            let mut f = b.bh.borrow_mut();
            f.state.interacting = 1;
            f.state.interaction_type = it::STAB;
            f.tapped_npc = Some(npc);
            f.chasing_npc = true;
            f.action_queue.push(Rc::new(()));
            f.interaction_item = None;
        }
        // interactionTimer += 0.2 * (((1+1)*0.4f) * 1 + 0.4f) -> crosses 0.15 on the first tick
        tick(&o);
        assert!(!b.bh.borrow().chasing_npc);
        assert_eq!(b.st().interacting, 1);
        // second tick: prev (0.24) >= 0.15 and < 0.3? 0.48 >= 0.3: stops
        tick(&o);
        assert_eq!(b.st().interacting, 0);
        let _ = world;
        // PLACE_SOLID_BLOCK with the wrong interaction type stops
        {
            let mut f = b.bh.borrow_mut();
            f.state.interacting = 1;
            f.state.interaction_type = it::PLACE_SOLID_BLOCK;
            f.state.interaction_timer = 0.0;
            f.interaction_item = Some(InventoryItem::new(ItemType(1), 0, 0, None, None));
            f.action_queue.push(Rc::new(()));
        }
        h.cit.set(it::PLACE_ITEM);
        tick(&o);
        assert_eq!(b.st().interacting, 0);
    }

    #[test]
    fn u6_place_item_effects() {
        let (world, _d, _dw, o, h) = mk();
        let b = bh_of(&o);
        steady(b);
        h.item_available.set(true);
        h.cit.set(it::PLACE_SOLID_BLOCK);
        {
            let mut f = b.bh.borrow_mut();
            f.state.interacting = 1;
            f.state.interaction_type = it::PLACE_SOLID_BLOCK;
            f.state.interacting_square = IntPair { x: 6, y: 9 };
            f.interacting_tile = Some(tile(&world, 6, 9));
            f.interaction_item = Some(InventoryItem::new(ItemType::ITEM_SHARK_JAW, 5, 6, None, None));
            f.action_queue.push(Rc::new(()));
        }
        tick(&o);
        let log = h.take();
        assert!(log.contains(&"removeitem:false".to_string()));
        assert!(log.contains(&"ach:grp.jaws".to_string()));
    }

    /// : U7 step completion and net-queue consumption.
    #[test]
    fn u7_walk_two_steps() {
        let (world, _d, _dw, o, h) = mk();
        let b = bh_of(&o);
        steady(b);
        let w = 32 * 32;
        let step = |x: i32, y: i32| -> PlistValue {
            let mut d = crate::objects::base::new_mutable_dict();
            d.map.insert("index".to_string(), PlistValue::int((y * w + x) as i64));
            d.map.insert("terrainDifficulty".to_string(), PlistValue::int(10));
            d.map.insert("traverseType".to_string(), PlistValue::int(1));
            d.map.insert("traverseToKeyFrame".to_string(), PlistValue::int(0));
            PlistValue::Dict(d)
        };
        // element 0 = GOAL, last = NEXT
        b.bh.borrow_mut().path = Some(vec![step(7, 8), step(6, 8)]);
        b.bh.borrow_mut().travel_fraction = 1.0;
        b.bh.borrow_mut().travel_speed = 4.0;
        let hf = (1.0f32 + 1.0f32) * 0.4f32 + 0.4f32;
        let of = DT * 4.0f32 * hf * 1.0f32;
        // tick 1: nextActionDelay 0 < 0.2: waits
        tick(&o);
        assert_eq!(b.bh.borrow().next_action_delay, DT);
        assert_eq!(b.bh.borrow().path.as_ref().unwrap().len(), 2);
        // tick 2: the step completes and the next path point is taken
        tick(&o);
        {
            let f = b.bh.borrow();
            assert_eq!(f.path.as_ref().unwrap().len(), 1);
            assert_eq!(f.to_square, IntPair { x: 6, y: 8 });
            assert_eq!(f.from_square, IntPair { x: 5, y: 8 });
            assert_eq!(f.next_to_square, IntPair { x: 7, y: 8 });
            assert_eq!(f.traverse_type, 1);
            assert_eq!(f.state.animation_type, 1);
            assert_eq!(f.travel_speed, 40.0f32 / 10.0f32);
            let tf = (1.0f32 + (if 1.0 <= of { 1.0 } else { of })) - 1.0f32;
            assert_eq!(f.travel_fraction, tf);
            // interpolation: floatPos = from + (to - from) * travelFraction
            assert_eq!(b.float_pos().x, 5.5f32 + (6.5f32 - 5.5f32) * tf);
            assert_eq!(b.float_pos().y, 8.0);
        }
        assert!(o.update_needs_to_be_sent());
        // tick 3: completes step 2 (final step, no interaction)
        tick(&o);
        {
            let f = b.bh.borrow();
            assert_eq!(f.to_square, IntPair { x: 7, y: 8 });
            assert_eq!(f.from_square, IntPair { x: 6, y: 8 });
            assert!(f.path.as_ref().unwrap().is_empty());
        }
        assert_eq!(b.pos(), IntPair { x: 6, y: 8 });
        // tick 4: arrival at (7, 8): travelFraction = 1, speed 4, animation back to stand
        tick(&o);
        {
            let f = b.bh.borrow();
            assert_eq!(f.from_square, IntPair { x: 7, y: 8 });
            assert_eq!(f.travel_fraction, 1.0);
            assert_eq!(f.travel_speed, 4.0);
            assert_eq!(f.state.animation_type, 0);
        }
        let _ = (world, h);
    }

    #[test]
    fn u7_final_step_with_interaction_starts_interacting() {
        let (world, _d, _dw, o, h) = mk();
        let b = bh_of(&o);
        steady(b);
        let w = 32 * 32;
        let mut d = crate::objects::base::new_mutable_dict();
        d.map.insert("index".to_string(), PlistValue::int((7 * w + 6) as i64));
        d.map.insert("terrainDifficulty".to_string(), PlistValue::int(10));
        d.map.insert("traverseType".to_string(), PlistValue::int(1));
        d.map.insert("traverseToKeyFrame".to_string(), PlistValue::int(0));
        {
            let mut f = b.bh.borrow_mut();
            f.path = Some(vec![PlistValue::Dict(d)]);
            f.state.goal_interaction_type_while_walking = it::DIG;
            f.next_action_delay = 1.0;
        }
        h.cit.set(it::DIG);
        tick(&o);
        let f = b.bh.borrow();
        // the final path point is the interaction target: move = NO, interactTile = nextTile
        assert_eq!(f.state.interacting, 1);
        assert_eq!(f.state.interaction_type, it::DIG);
        assert_eq!(f.state.interacting_square, IntPair { x: 6, y: 7 });
        assert_eq!(f.interacting_tile, Some(tile(&world, 6, 7)));
        assert_eq!(f.travel_fraction, 1.0);
        assert!(f.path.as_ref().unwrap().is_empty());
        assert_eq!(f.to_square, f.from_square);
    }

    #[test]
    fn u7_net_replica_consumes_the_queue() {
        let (world, _d, dwref, _o, _h) = mk();
        let src = new_bh(&world, &dwref, 0);
        let data = bh_of(&src).creation_net_data_for_client_impl(None);
        let rep = Blockhead::init_with_world_dynamic_world_cache_net_data(&world, &dwref, None, &data).unwrap();
        let r = bh_of(&rep);
        steady(r);
        {
            let mut f = r.bh.borrow_mut();
            f.current_net_queue_size = 2;
            f.net_next_to_square =
                [IntPair { x: 6, y: 8 }, IntPair { x: 7, y: 8 }, IntPair { x: 8, y: 8 }, IntPair { x: 9, y: 8 }];
            f.next_terrain_difficulty = [10, 20, 30, 40];
            f.next_traverse_type = [1, 1, 1, 1];
            f.next_traverse_to_key_frame = [0; 4];
            f.next_animation_type = [1, 1, 1, 1];
            f.next_sub_animation_type = [0; 4];
            f.next_action_delay = 1.0;
        }
        rep.update_accurate_dt_is_simulation(&rep, 0.2, 0.2, false);
        let f = r.bh.borrow();
        assert_eq!(f.to_square, IntPair { x: 6, y: 8 });
        assert_eq!(f.terrain_difficulty, 10);
        assert_eq!(f.current_net_queue_size, 1);
        // queue shifted down by one
        assert_eq!(f.net_next_to_square[0], IntPair { x: 7, y: 8 });
        assert_eq!(f.next_terrain_difficulty[0], 20);
        // nextNext = netNextToSquare[0] after the shift
        assert_eq!(f.next_to_square, IntPair { x: 7, y: 8 });
        assert_eq!(f.state.animation_type, 1);
        // travelSpeed = 40 / terrainDifficulty
        assert_eq!(f.travel_speed, 4.0);
        assert!(!f.path_needs_recalculated);
    }

    fn test_ctx<'a>(b: &'a Blockhead, me: &'a ObjRef, world: &Rc<World>) -> Ctx<'a> {
        Ctx {
            b,
            me,
            h: host(),
            world: Some(world.clone()),
            rules: CustomRules::default(),
            dt: DT,
            adt: DT,
            sim: false,
            ctl: false,
            sim_or_ctl: false,
            is_net: false,
            expert: false,
        }
    }

    /// : interpolation, one value per animation type against the closed form.
    #[test]
    fn u7_interpolation_table() {
        let (world, _d, _dw, o, _h) = mk();
        let b = bh_of(&o);
        let c = test_ctx(b, &o, &world);
        let pi = M_PI;
        let s = |x: f32, k: f64| sinf((((x as f64) * pi) * k) as f32);
        let lerp = |a: f32, b: f32, t: f32| a + (b - a) * t;
        let run = |anim: u32, fs: (i32, i32), ts: (i32, i32), tf: f32, tfn: f32| -> Vector2 {
            {
                let mut f = b.bh.borrow_mut();
                f.state.animation_type = anim;
                f.from_square = IntPair { x: fs.0, y: fs.1 };
                f.to_square = IntPair { x: ts.0, y: ts.1 };
                f.travel_fraction = tf;
                f.travel_fraction_normalized = tfn;
                f.traverse_type = 0;
                f.traverse_to_key_frame = 0;
                f.traverse_from_key_frame = 0;
            }
            u7_interpolate(&c);
            b.float_pos()
        };
        let fx = |x: i32| ((x as f64) + 0.5) as f32;
        // walk on the level: linear in travelFraction
        let p = run(1, (5, 8), (6, 8), 0.3, 0.3);
        assert_eq!((p.x, p.y), (lerp(fx(5), fx(6), 0.3), 8.0));
        // walk uphill: yL = sin(tfn*pi*0.75)/0.7
        let p = run(1, (5, 8), (6, 9), 0.3, 0.4);
        assert_eq!((p.x, p.y), (lerp(fx(5), fx(6), 0.3), lerp(8.0, 9.0, s(0.4, 0.75) / 0.7)));
        // walk downhill
        let p = run(1, (5, 9), (6, 8), 0.3, 0.4);
        assert_eq!(p.y, lerp(9.0, 8.0, 1.0 - s(0.6, 0.75) / 0.7));
        // stairs (26): yOff = sin(tfn * pi) * 0.35, xL = tfn
        let p = run(26, (5, 8), (6, 9), 0.3, 0.4);
        assert_eq!(p.x, lerp(fx(5), fx(6), 0.4));
        assert_eq!(p.y, lerp(8.0, 9.0, 0.3) + ((sinf(((0.4f32 as f64) * pi) as f32)) as f64 * 0.35) as f32);
        // jump up (2): l = min(tfn*2, 1)
        let p = run(2, (5, 8), (6, 9), 0.3, 0.4);
        let l = 0.8f32;
        assert_eq!((p.x, p.y), (lerp(fx(5), fx(6), l), lerp(8.0, 9.0, s(l, 0.75) / 0.7)));
        // jump on the level (3): yOff = sin(l * pi) * 0.4
        let p = run(3, (5, 8), (6, 8), 0.3, 0.75);
        let l = (0.75f32 - 0.5f32) * 2.0f32;
        assert_eq!(p.y, lerp(8.0, 8.0, l) + sinf(((l as f64) * pi) as f32) * 0.4f32);
        // monkey-hang: constant offsets
        assert_eq!(run(6, (5, 8), (5, 8), 0.5, 0.5).y, 8.0 + -0.5);
        assert_eq!(run(14, (5, 8), (5, 8), 0.5, 0.5).y, 8.0 + 0.2);
        assert_eq!(run(13, (5, 8), (5, 8), 0.5, 0.5).y, 8.0 + 0.2);
        // reversed lerp for 10: lerp(to, from)
        let tf = 0.4f32;
        let f_ = 1.0f32 - tf * tf;
        let xl = s(f_, 0.5);
        let p = run(10, (5, 8), (6, 7), tf, tf);
        assert_eq!(p.x, lerp(fx(6), fx(5), xl));
        assert_eq!(p.y, lerp(7.0, 8.0, xl) + sinf(((f_ as f64) * pi) as f32) * -0.3f32);
        // 21: powf branch
        let p = run(21, (5, 8), (6, 9), 0.5, 0.2);
        let f_ = 1.0f32 - 0.2f32;
        assert_eq!(p.x, lerp(fx(6), fx(5), powf((f_ - 0.5f32) * 2.0f32, 0.5f32)));
        // x-wrap: from.x - to.x huge
        let p = run(1, (1023, 8), (0, 8), 0.5, 0.5);
        assert_eq!(p.x, lerp(fx(1023) - 1024.0, fx(0), 0.5));
        // unknown animation: linear
        let p = run(0, (5, 8), (6, 8), 0.25, 0.25);
        assert_eq!(p.x, lerp(fx(5), fx(6), 0.25));
        // 4 (climb): xL piecewise
        let p = run(4, (5, 8), (6, 9), 0.1, 0.1);
        assert_eq!(p.x, fx(5));
        let p = run(4, (5, 8), (6, 9), 0.9, 0.9);
        assert_eq!(p.x, fx(6));
    }

    #[test]
    fn u7_cactus_fall_and_door_effects() {
        let (world, _d, _dw, o, h) = mk();
        let b = bh_of(&o);
        steady(b);
        let w = 32 * 32;
        let mk_step = |x: i32, y: i32, tt: i64, td: i64, kf: i64| -> PlistValue {
            let mut d = crate::objects::base::new_mutable_dict();
            d.map.insert("index".to_string(), PlistValue::int((y * w + x) as i64));
            d.map.insert("terrainDifficulty".to_string(), PlistValue::int(td));
            d.map.insert("traverseType".to_string(), PlistValue::int(tt));
            d.map.insert("traverseToKeyFrame".to_string(), PlistValue::int(kf));
            PlistValue::Dict(d)
        };
        // FALL onto a stand key frame with terrainDifficulty 150: damage (150 - 50) / 50 = 2.0 -> fatal
        {
            let mut f = b.bh.borrow_mut();
            f.path = Some(vec![mk_step(5, 7, 17, 150, 0), mk_step(5, 9, 17, 150, 0)]);
            f.next_action_delay = 1.0;
            f.is_client_blockhead_being_controlled_by_server = true;
            f.action_queue.push(Rc::new(()));
        }
        tick(&o);
        // ctl blockheads log the fall; the step taken first is the last element (5,9)
        let log = h.take();
        assert!(log.iter().any(|l| l.contains("in a fall")), "{log:?}");
        assert_eq!(b.bh.borrow().state.regenerating, 1);
        assert_eq!(b.bh.borrow().travel_speed, ((150.0f32 + 20.0f32) / 8.0f32));
        let _ = world;
        // door: closing behind when leaving a door tile
        let (world, _d, _dw, o, h) = mk();
        let b = bh_of(&o);
        steady(b);
        let ft = tile(&world, 5, 8);
        ft.set_contents(0x92); // door tile
        {
            let mut f = b.bh.borrow_mut();
            f.from_tile = Some(ft);
            f.to_tile = Some(tile(&world, 6, 8));
            f.from_square = IntPair { x: 5, y: 8 };
            f.to_square = IntPair { x: 6, y: 8 };
            f.travel_fraction = 1.5;
            f.next_action_delay = 1.0;
        }
        tick(&o);
        assert!(h.take().contains(&"door:5,8:false:0".to_string()));
    }

    #[test]
    fn u8_ride_position_tail() {
        let (_world, _d, _dw, o, _h) = mk();
        let b = bh_of(&o);
        steady(b);
        // a net replica resets pathNeedsRecalculated and never prepares the inventory
        b.base.borrow_mut().is_net = true;
        b.bh.borrow_mut().path_needs_recalculated = true;
        tick(&o);
        assert!(!b.bh.borrow().path_needs_recalculated);
    }

    #[test]
    fn particle_visibility() {
        let (world, _d, _dw, _o, _h) = mk();
        let t = tile(&world, 9, 20);
        for ty in 0..=255u8 {
            t.set_type_index(ty);
            t.set_contents(0);
            t.set_foreground_contents(0);
            let expect = (1..=77).contains(&ty) && !matches!(ty, 2 | 21 | 22 | 23 | 25 | 31 | 33..=47 | 60..=66);
            assert_eq!(particle_color_alpha_is_visible(Some(t)), expect, "type {ty}");
        }
        t.set_type_index(2);
        t.set_foreground_contents(0x43); // plant
        assert!(particle_color_alpha_is_visible(Some(t)));
        t.set_foreground_contents(0);
        t.set_contents(0x07); // trunk
        assert!(particle_color_alpha_is_visible(Some(t)));
        assert!(!particle_color_alpha_is_visible(None));
    }
}
