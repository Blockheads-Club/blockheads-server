//! Branch-coverage tests for `update.rs` (phases U1..U8), complementing the exact-value tests of `update::tests`.
//! Each test drives one scripted situation through `update_accurate_dt_is_simulation` and asserts the observable
//! effect (host calls, state fields).

use super::super::blockhead::testutil::{bh_of, new_bh, setup, stub, tile, MockDw};
use super::cov_host::CovHost as TestHost;
use super::*;
use crate::items::inventory_item::InventoryItem;
use crate::rng::glibc_rand::srand;
use crate::rng::glibc_rand::rand;
use crate::types::structs::CustomRules;
use std::cell::RefCell;

const DT: f32 = 0.2;

type Fixture = (Rc<World>, Rc<MockDw>, crate::objects::base::DynamicWorldRef, ObjRef, Rc<TestHost>);

fn mk() -> Fixture {
    let h = TestHost::install();
    let (world, dw, dwref) = setup();
    let o = new_bh(&world, &dwref, 0);
    bh_of(&o).prepare_inventory_for_saving();
    h.take();
    (world, dw, dwref, o, h)
}

fn tick(o: &ObjRef) {
    o.update_accurate_dt_is_simulation(o, DT, DT, false);
}

fn tick_sim(o: &ObjRef) {
    o.update_accurate_dt_is_simulation(o, DT, DT, true);
}

fn steady(b: &Blockhead) {
    let mut f = b.bh.borrow_mut();
    f.state.food_pause_timer = 1.0e9;
    f.state.environment_temperature = 1.0;
    f.state.environment_exposure = 1.0;
    f.state.environment_light = 1.0;
    f.state.environment = 1.0;
    f.state.coffee_energy_timer = 0.0;
}

/// Keeps a simulated blockhead simulating (`shouldContinueSimulating`) without the action queue being consumed.
fn sim_ready(b: &Blockhead) {
    let mut f = b.bh.borrow_mut();
    f.action_queue.push(Rc::new(()));
    f.waiting_for_fill_response = true;
}

fn rules(world: &World, f: impl FnOnce(&mut CustomRules)) {
    let mut r = CustomRules::default();
    r.custom_rules_enabled = 1;
    // neutral: 1 is the "normal" setting of the hunger / health / happiness dials, death on
    r.hunger = 1;
    r.health = 2;
    r.happiness = 1;
    r.death = 1;
    f(&mut r);
    world.set_custom_rules(r);
}

fn step(index: i32, td: i64, ty: i64, kf_: i64) -> PlistValue {
    let mut d = crate::objects::base::new_mutable_dict();
    d.map.insert("index".to_string(), PlistValue::int(index as i64));
    d.map.insert("terrainDifficulty".to_string(), PlistValue::int(td));
    d.map.insert("traverseType".to_string(), PlistValue::int(ty));
    d.map.insert("traverseToKeyFrame".to_string(), PlistValue::int(kf_));
    PlistValue::Dict(d)
}

fn idx(x: i32, y: i32) -> i32 {
    y * 32 * 32 + x
}

// ---------------------------------------------------------------------------------------------------
// helpers on Ctx
// ---------------------------------------------------------------------------------------------------

#[test]
fn min_max_helpers_both_ways() {
    assert_eq!(min_f(1.0, 2.0), 1.0);
    assert_eq!(min_f(2.0, 1.0), 1.0);
    assert_eq!(max_f(1.0, 2.0), 2.0);
    assert_eq!(max_f(2.0, 1.0), 2.0);
    assert_eq!(min_d(1.0, 2.0), 1.0);
    assert_eq!(min_d(2.0, 1.0), 1.0);
    assert_eq!(max_d(1.0, 2.0), 2.0);
    assert_eq!(max_d(2.0, 1.0), 2.0);
}

#[test]
fn ctx_helpers_wrapdx_killed_or_and_health_rules() {
    let (world, _d, _dw, o, h) = mk();
    let b = bh_of(&o);
    let c = Ctx {
        b,
        me: &o,
        h: host(),
        world: Some(world.clone()),
        rules: world.custom_rules(),
        dt: DT,
        adt: DT,
        sim: false,
        ctl: false,
        sim_or_ctl: false,
        is_net: false,
        expert: false,
    };
    let w = c.w_tiles();
    assert_eq!(w, 1024);
    assert_eq!(c.wrapdx(10), 10);
    assert_eq!(c.wrapdx(-10), -10);
    assert_eq!(c.wrapdx(-600), -600 + w);
    assert_eq!(c.wrapdx(600), 600 - w);
    // killed_or
    assert_eq!(c.killed_or("hurt"), "hurt");
    b.bh.borrow_mut().state.regenerating = 1;
    assert_eq!(c.killed_or("hurt"), "killed");
    b.bh.borrow_mut().state.regenerating = 0;
    // create_free_block with and without the priority blockhead
    c.create_free_block(IntPair { x: 1, y: 2 }, 5, 6, 7, 1, true);
    c.create_free_block(IntPair { x: 1, y: 2 }, 5, 6, 7, 0, false);
    assert_eq!(h.take(), vec!["fb:1,2:5:6:7:false:1:true".to_string(), "fb:1,2:5:6:7:false:0:false".to_string()]);
    // health_rule_modified
    assert_eq!(c.health_rule_modified(0.5), 0.5);
    for (rule, expect) in [(0i8, 99999.9f32), (1, 5.0), (2, 0.5), (3, 0.05)] {
        let mut c2 = Ctx { rules: CustomRules::default(), ..clone_ctx(&c) };
        c2.rules.custom_rules_enabled = 1;
        c2.rules.health = rule;
        assert_eq!(c2.health_rule_modified(0.5), expect, "rule {rule}");
    }
    // no world
    let c3 = Ctx { world: None, ..clone_ctx(&c) };
    assert_eq!(c3.w_tiles(), 0);
    assert!(!c3.tile_protected(IntPair { x: 1, y: 1 }));
}

fn clone_ctx<'a>(c: &Ctx<'a>) -> Ctx<'a> {
    Ctx {
        b: c.b,
        me: c.me,
        h: c.h.clone(),
        world: c.world.clone(),
        rules: c.rules,
        dt: c.dt,
        adt: c.adt,
        sim: c.sim,
        ctl: c.ctl,
        sim_or_ctl: c.sim_or_ctl,
        is_net: c.is_net,
        expert: c.expert,
    }
}

#[test]
fn damage_apply_branches() {
    let (world, _d, _dw, o, h) = mk();
    let b = bh_of(&o);
    let mkc = |r: CustomRules| Ctx {
        b,
        me: &o,
        h: host(),
        world: Some(world.clone()),
        rules: r,
        dt: DT,
        adt: DT,
        sim: false,
        ctl: false,
        sim_or_ctl: false,
        is_net: false,
        expert: false,
    };
    let c = mkc(CustomRules::default());
    // plenty of health: plain subtraction
    c.damage_apply(0.25);
    assert_eq!(b.st().health, 0.75);
    // health - d <= 0 but health - d/2 > 0: clamp to 0
    b.bh.borrow_mut().state.health = 0.3;
    c.damage_apply(0.5);
    assert_eq!(b.st().health, 0.0);
    // otherwise death accumulates, health 0
    b.bh.borrow_mut().state.health = 0.1;
    c.damage_apply(0.5);
    assert_eq!(b.st().health, 0.0);
    assert_eq!(b.st().death, 0.5);
    // rules.death == 0: no death accumulation
    let mut r = CustomRules::default();
    r.custom_rules_enabled = 1;
    r.death = 0;
    let c0 = mkc(r);
    b.bh.borrow_mut().state.health = 0.1;
    b.bh.borrow_mut().state.death = 0.0;
    c0.damage_apply(0.5);
    assert_eq!(b.st().death, 0.0);
    assert_eq!(b.st().health, 0.0);
    // death reaching 1.0 kills
    b.bh.borrow_mut().state.death = 0.9;
    c.damage_apply(0.5);
    assert_eq!(b.st().regenerating, 1);
    assert!(h.take().iter().any(|l| l.starts_with("drop:")));
}

#[test]
fn interaction_type_checked_rechecks_protected_tiles() {
    let (world, _d, _dw, o, h) = mk();
    let b = bh_of(&o);
    let c = Ctx {
        b,
        me: &o,
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
    };
    // DIG is disallowed on protected tiles; the tile is not protected in the bare world: no recheck
    h.cit.set(it::DIG);
    assert_eq!(c.interaction_type_checked(None, IntPair { x: 6, y: 7 }, None, 0), it::DIG);
    // the protection check is answered by the world; exercise the call
    assert!(!c.tile_protected(IntPair { x: 6, y: 7 }));
}

// ---------------------------------------------------------------------------------------------------
// U1
// ---------------------------------------------------------------------------------------------------

#[test]
fn u1_controlled_craft_aborts_the_workbench_without_fuel_or_ability() {
    let (_w, _d, _dw, o, h) = mk();
    let b = bh_of(&o);
    steady(b);
    {
        let mut f = b.bh.borrow_mut();
        f.is_client_blockhead_being_controlled_by_server = true;
        f.state.interaction_type = it::CRAFT;
        f.interaction_workbench = Some(stub(1, true));
    }
    h.set("nofuel", 1);
    tick(&o);
    assert!(h.take().contains(&"wbabort".to_string()));
    h.set("nofuel", 0);
    h.set("unable", 1);
    tick(&o);
    assert!(h.take().contains(&"wbabort".to_string()));
    h.set("unable", 0);
    tick(&o);
    assert!(!h.take().contains(&"wbabort".to_string()));
    // no workbench: nothing to abort
    b.bh.borrow_mut().interaction_workbench = None;
    h.set("nofuel", 1);
    tick(&o);
    assert!(!h.take().contains(&"wbabort".to_string()));
}

#[test]
fn u1_simulation_inactivity_returns_and_activity_resets() {
    let (_w, _d, _dw, o, _h) = mk();
    let b = bh_of(&o);
    steady(b);
    b.bh.borrow_mut().controlled_by_server_inactivity_timeout = 3.9;
    // not simulating: the call returns before U3 (the coffee timer would count down)
    tick_sim(&o);
    assert_eq!(b.st().coffee_energy_timer, 0.0);
    assert!(b.bh.borrow().controlled_by_server_inactivity_timeout > 4.0);
    // simulating again: the timers are reset
    sim_ready(b);
    tick_sim(&o);
    assert_eq!(b.bh.borrow().controlled_by_server_inactivity_timeout, 0.0);
    assert!(b.st().coffee_energy_timer < 0.0);
}

// ---------------------------------------------------------------------------------------------------
// U3
// ---------------------------------------------------------------------------------------------------

#[test]
fn u3_meditation_environment_and_bonus() {
    let (world, _d, _dw, o, h) = mk();
    let b = bh_of(&o);
    steady(b);
    {
        let mut f = b.bh.borrow_mut();
        f.state.sub_animation_type = vsa::MEDITATING;
        f.state.environment_temperature = 0.1;
        f.state.meditation_progress = 0.9999;
        f.state.energy = 1.2;
    }
    srand(11);
    tick(&o);
    // meditating: environment target is 1.0 whatever the temperature
    assert!(b.st().environment > 0.99);
    assert_eq!(b.st().meditation_progress, 0.0);
    let log = h.take();
    assert!(log.iter().any(|l| l.starts_with("fb:5,8:")), "{log:?}");
    // fast forward is 20x slower
    world.set_fast_forward(true);
    b.bh.borrow_mut().state.meditation_progress = 0.0;
    tick(&o);
    assert_eq!(b.st().meditation_progress, (DT / 20.0f32) / 900.0f32);
    world.set_fast_forward(false);
    // simulated meditation does not progress
    b.bh.borrow_mut().state.meditation_progress = 0.0;
    sim_ready(b);
    tick_sim(&o);
    assert_eq!(b.st().meditation_progress, 0.0);
}

#[test]
fn u3_rules_override_health_and_death() {
    let (world, _d, _dw, o, _h) = mk();
    let b = bh_of(&o);
    steady(b);
    rules(&world, |r| {
        r.health = 4;
        r.death = 0;
    });
    {
        let mut f = b.bh.borrow_mut();
        f.state.health = 0.3;
        f.state.death = 0.5;
        f.state.energy = 1.2;
    }
    tick(&o);
    assert_eq!(b.st().health, 1.0);
    assert_eq!(b.st().death, 0.0);
}

#[test]
fn u3_starvation_under_each_health_rule_and_the_sim_event() {
    for (rule, dec) in
        [(0i8, 99999.9f32), (1, (DT / 900.0f32) * 0.5f32 * 10.0f32), (3, (DT / 900.0f32) * 0.5f32 * 0.1f32)]
    {
        let (world, _d, _dw, o, h) = mk();
        let b = bh_of(&o);
        steady(b);
        rules(&world, |r| r.health = rule);
        {
            let mut f = b.bh.borrow_mut();
            f.state.food_pause_timer = 0.0;
            f.state.fullness = -0.11;
            f.state.energy = 1.2;
        }
        sim_ready(b);
        tick_sim(&o);
        let log = h.take();
        if rule == 0 {
            assert_eq!(b.st().regenerating, 1, "{log:?}");
            assert!(log.contains(&"sim:4:Text(\"killed by starvation\")".to_string()), "{log:?}");
        } else {
            assert_eq!(b.st().health, 1.0 - dec, "rule {rule}");
            assert!(log.contains(&"sim:4:Text(\"injured by starvation\")".to_string()), "{log:?}");
        }
    }
}

#[test]
fn u3_bed_kinds_set_the_energy_and_the_cap() {
    for (ty, expect_cap) in [(0xa9i64, 1.5f32), (0xaa, 2.0), (0, 1.2)] {
        let (_w, _d, _dw, o, h) = mk();
        let b = bh_of(&o);
        steady(b);
        h.set("objtype", ty);
        {
            let mut f = b.bh.borrow_mut();
            f.state.sub_animation_type = vsa::LIE_ON_BED;
            f.state.energy = 0.1; // below the 0.25 floor: raised first
            f.interaction_object = Some(stub(1, true));
        }
        tick(&o);
        assert!(b.st().energy > 0.25 && b.st().energy < 0.3);
        b.bh.borrow_mut().state.energy = 1.0;
        tick(&o);
        assert_eq!(b.st().energy, expect_cap);
    }
}

#[test]
fn u3_collapse_at_zero_energy_and_ground_sleep_floor() {
    let (_w, _d, _dw, o, _h) = mk();
    let b = bh_of(&o);
    steady(b);
    b.bh.borrow_mut().state.energy = 0.0001;
    tick(&o);
    assert_eq!(b.st().energy, 0.25); // collapse_if_possible resets it after U3 zeroed it
    assert!(b.bh.borrow().cancel_simulate_due_to_collapse);
}

#[test]
fn u3_happiness_low_environment_and_rules() {
    let (world, _d, _dw, o, _h) = mk();
    let b = bh_of(&o);
    steady(b);
    {
        let mut f = b.bh.borrow_mut();
        f.state.environment = 0.1;
        f.state.environment_temperature = 0.01;
        f.state.energy = 1.2;
    }
    tick(&o);
    assert!(b.st().happiness < 0.5);
    // rule 2 (sqrt) and 0 (square) for a mid happiness
    for rule in [2i8, 0] {
        let f_ = |h: f32| if rule == 2 { h.sqrt() } else { h * h };
        {
            let mut f = b.bh.borrow_mut();
            f.state.environment = 1.0;
            f.state.environment_temperature = 1.0;
            f.state.fullness = 0.7;
            f.state.health = 0.5;
            f.state.energy = 1.2;
        }
        rules(&world, |r| r.happiness = rule);
        tick(&o);
        let got = b.st().happiness;
        let base = 0.5f32;
        assert!((got - f_(base)).abs() < 0.01, "rule {rule}: {got}");
    }
}

#[test]
fn u3_fire_and_heat_damage_with_simulation_events() {
    let (_w, _d, _dw, o, h) = mk();
    let b = bh_of(&o);
    steady(b);
    sim_ready(b);
    b.bh.borrow().from_tile.unwrap().set_on_fire(1);
    b.bh.borrow_mut().damage_delay_timer = 0.0;
    tick_sim(&o);
    let log = h.take();
    assert!(log.contains(&"sim:4:Text(\"burned by fire\")".to_string()), "{log:?}");
    b.bh.borrow().from_tile.unwrap().set_on_fire(0);
    // temperature above 100
    {
        let mut f = b.bh.borrow_mut();
        f.current_temperature = 300.0;
        f.damage_delay_timer = 0.0;
        f.state.health = 1.0;
    }
    tick_sim(&o);
    let log = h.take();
    assert!(log.contains(&"sim:4:Text(\"burned by fire\")".to_string()), "{log:?}");
    // the capped amount is 0.1
    assert!(log.contains(&"armor:0.1".to_string()), "{log:?}");
}

#[test]
fn u3_clothing_multipliers_and_timer_events() {
    let (_w, _d, _dw, o, h) = mk();
    let b = bh_of(&o);
    steady(b);
    let run = |sub: u32, anim: u32, regen: bool| -> f32 {
        let mut f = b.bh.borrow_mut();
        f.clothing_increment_timer = 0.0;
        f.state.sub_animation_type = sub;
        f.state.animation_type = anim;
        f.state.regenerating = regen as u8;
        drop(f);
        // regenerating returns from U2; call U3 directly instead
        let mut sc = false;
        let c = Ctx {
            b,
            me: &o,
            h: host(),
            world: b.world_rc(),
            rules: CustomRules::default(),
            dt: DT,
            adt: DT,
            sim: false,
            ctl: false,
            sim_or_ctl: false,
            is_net: false,
            expert: false,
        };
        let _ = u3(&c, &mut sc);
        let t = b.bh.borrow().clothing_increment_timer;
        b.bh.borrow_mut().state.regenerating = 0;
        t
    };
    // sub 0/4 and the animation table
    assert_eq!(run(0, 5, false), DT * 2.0);
    for anim in [0u32, 7, 14, 23, 27, 28, 29, 30, 33] {
        assert_eq!(run(0, anim, false), DT * 0.25, "anim {anim}");
    }
    assert_eq!(run(4, 1, false), DT);
    assert_eq!(run(2, 0, false), DT * 0.5);
    for sub in [5u32, 6, 7] {
        assert_eq!(run(sub, 0, false), 0.0);
    }
    assert_eq!(run(0, 1, true), 0.0);
    // the events
    b.bh.borrow_mut().path_needs_recalculated = false;
    b.bh.borrow_mut().state.sub_animation_type = 0;
    b.bh.borrow_mut().clothing_increment_timer = 263.7;
    b.bh.borrow_mut().clothing_ice_increment_timer = 263.7;
    b.bh.borrow_mut().jet_fuel_usage = 0.5;
    b.bh.borrow_mut().fuel_usage_increment_timer = 2.2;
    tick(&o);
    let log = h.take();
    for e in ["clothing", "iceclothing", "fuel"] {
        assert!(log.contains(&e.to_string()), "{e} {log:?}");
    }
    // the passive donkey item
    h.set("donkey", 1);
    {
        let mut f = b.bh.borrow_mut();
        f.interaction_item = Some(InventoryItem::new(ItemType(0x4a), 0, 0, None, None));
        f.ride_object = Some(stub(1, true));
        f.passive_item_usage_increment_timer = 9.95;
    }
    tick(&o);
    assert!(h.take().contains(&"passive".to_string()));
}

// ---------------------------------------------------------------------------------------------------
// breathing
// ---------------------------------------------------------------------------------------------------

fn water(world: &World, x: i32, y: i32, pcl: u8) {
    let t = tile(world, x, y);
    t.set_type_index(3);
    t.set_partial_content_left(pcl);
}

#[test]
fn breathing_underwater_drowns_and_recovers() {
    let (world, _d, _dw, o, h) = mk();
    let b = bh_of(&o);
    steady(b);
    sim_ready(b);
    water(&world, 5, 9, 200);
    b.bh.borrow_mut().state.energy = 1.2;
    // underwater: the fraction accumulates
    tick_sim(&o);
    assert!(b.bh.borrow().underwater);
    assert!((b.st().drown_fraction - DT / 30.0).abs() < 1e-6);
    // the drowning hit
    b.bh.borrow_mut().state.drown_fraction = 1.03;
    tick_sim(&o);
    assert!(h.take().contains(&"sim:4:Text(\"injured by drowning\")".to_string()));
    assert!(b.bh.borrow().needs_harm_flash);
    assert!(b.st().health < 1.0);
    // out of the water the fraction recovers, above 1.0 it snaps to 1.0
    water(&world, 5, 9, 0);
    tile(&world, 5, 9).set_type_index(2);
    b.bh.borrow_mut().state.drown_fraction = 0.5;
    tick_sim(&o);
    assert!((b.st().drown_fraction - 0.4).abs() < 1e-6);
    b.bh.borrow_mut().state.drown_fraction = 0.05;
    tick_sim(&o);
    assert_eq!(b.st().drown_fraction, 0.0);
    b.bh.borrow_mut().state.drown_fraction = 1.5;
    tick_sim(&o);
    assert_eq!(b.st().drown_fraction, 1.0);
    // invulnerable rule: recovers even underwater
    water(&world, 5, 9, 200);
    rules(&world, |r| r.health = 4);
    b.bh.borrow_mut().state.drown_fraction = 0.5;
    tick_sim(&o);
    assert!(b.st().drown_fraction < 0.5);
}

#[test]
fn breathing_swimming_with_air_above_and_solid_mouth() {
    let (world, _d, _dw, o, _h) = mk();
    let b = bh_of(&o);
    steady(b);
    sim_ready(b);
    water(&world, 5, 9, 200);
    b.bh.borrow_mut().state.animation_type = va::SWIM;
    // air above the water: the head is out
    tick_sim(&o);
    assert!(!b.bh.borrow().underwater);
    // a solid block above keeps it under
    tile(&world, 5, 10).set_type_index(1);
    tick_sim(&o);
    assert!(b.bh.borrow().underwater);
    // water above with enough content too: stays under (the `!water || apcl < frac` test fails)
    water(&world, 5, 10, 255);
    tick_sim(&o);
    assert!(b.bh.borrow().underwater);
    // water above with less content than the fraction and not solid: the head is out
    b.base.borrow_mut().float_pos.y = 7.8;
    tick_sim(&o);
    assert!(!b.bh.borrow().underwater);
    b.base.borrow_mut().float_pos.y = 8.0;
    // the mouth in a solid block while the feet are in water
    tile(&world, 5, 9).set_type_index(1);
    water(&world, 5, 8, 255);
    b.bh.borrow_mut().state.animation_type = 0;
    tick_sim(&o);
    assert!(b.bh.borrow().underwater);
}

#[test]
fn breathing_in_space_and_rules() {
    let (world, _d, _dw, o, h) = mk();
    let b = bh_of(&o);
    steady(b);
    sim_ready(b);
    b.base.borrow_mut().float_pos = Vector2 { x: 5.5, y: 1000.0 };
    b.bh.borrow_mut().state.energy = 1.2;
    tick_sim(&o);
    assert!(b.bh.borrow().drowning_in_space);
    assert!(b.st().drown_fraction > 0.0);
    b.bh.borrow_mut().state.drown_fraction = 1.099;
    tick_sim(&o);
    assert!(h.take().contains(&"sim:4:Text(\"injured from lack of oxygen\")".to_string()));
    // fatal rule: health 0 modifies the damage; the death rule
    rules(&world, |r| {
        r.health = 0;
        r.death = 0;
    });
    b.bh.borrow_mut().state.drown_fraction = 1.099;
    b.bh.borrow_mut().state.health = 1.0;
    tick_sim(&o);
    assert_eq!(b.st().health, 0.0);
}

#[test]
fn breathing_without_world_does_nothing() {
    let (world, _d, _dw, o, _h) = mk();
    let b = bh_of(&o);
    let c = Ctx {
        b,
        me: &o,
        h: host(),
        world: None,
        rules: CustomRules::default(),
        dt: DT,
        adt: DT,
        sim: true,
        ctl: false,
        sim_or_ctl: true,
        is_net: false,
        expert: false,
    };
    breathing(&c, Vector2 { x: 5.0, y: 9.5 });
    assert!(!b.bh.borrow().underwater);
    let _ = world;
}

// ---------------------------------------------------------------------------------------------------
// direct-call fixtures
// ---------------------------------------------------------------------------------------------------

fn ctx<'a>(b: &'a Blockhead, o: &'a ObjRef, world: &Rc<World>) -> Ctx<'a> {
    Ctx {
        b,
        me: o,
        h: host(),
        world: Some(world.clone()),
        rules: world.custom_rules(),
        dt: DT,
        adt: DT,
        sim: false,
        ctl: false,
        sim_or_ctl: false,
        is_net: false,
        expert: false,
    }
}

/// A search for a seed whose first draw satisfies `pred`; leaves the generator seeded with it.
fn seed_first(pred: impl Fn(i32) -> bool) {
    for s in 1..100_000u32 {
        srand(s);
        if pred(rand()) {
            srand(s);
            return;
        }
    }
    panic!("no seed");
}

fn r01(v: i32) -> f64 {
    r01_from(v) as f64
}

/// A harmable stub NPC.
struct Npc {
    base: RefCell<crate::objects::base::DynamicObjectBase>,
    ty: i32,
    dies: bool,
    hits: RefCell<Vec<i32>>,
}
impl DynamicObject for Npc {
    fn base(&self) -> &RefCell<crate::objects::base::DynamicObjectBase> {
        &self.base
    }
    fn class_info(&self) -> &'static crate::objects::class_info::ClassInfo {
        &crate::objects::class_info::DYNAMIC_OBJECT_CLASS
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn object_type(&self) -> i32 {
        self.ty
    }
    fn as_harmable(&self) -> Option<&dyn crate::objects::protocols::HarmableDynamicObject> {
        Some(self)
    }
}
impl crate::objects::protocols::HarmableDynamicObject for Npc {
    fn tap_is_within_body_radius(&self, _tap: Vector2) -> bool {
        false
    }
    fn hit_with_force_blockhead(&self, _me: &ObjRef, force: i32, _b: Option<&ObjRef>) {
        self.hits.borrow_mut().push(force);
    }
    fn will_die_if_hit_by_force(&self, _force: i32) -> bool {
        self.dies
    }
    fn is_visible(&self) -> bool {
        true
    }
    fn center(&self) -> Vector2 {
        Vector2 { x: 3.0, y: 4.0 }
    }
}
fn npc(ty: i32, dies: bool) -> (ObjRef, Rc<Npc>) {
    let rc = crate::objects::handle::alloc_obj(|w| Npc {
        base: RefCell::new(crate::objects::base::DynamicObjectBase::alloc_zeroed_with(w)),
        ty,
        dies,
        hits: RefCell::new(Vec::new()),
    });
    (rc.clone(), rc)
}

// ---------------------------------------------------------------------------------------------------
// U5
// ---------------------------------------------------------------------------------------------------

fn place_between(b: &Blockhead, world: &World, from: IntPair, to: IntPair) {
    let mut f = b.bh.borrow_mut();
    f.from_square = from;
    f.to_square = to;
    f.from_tile = tile_at(world, from);
    f.to_tile = tile_at(world, to);
}

#[test]
fn u5_unloaded_tiles_bail_and_request_a_new_path() {
    let (_w, _d, _dw, o, _h) = mk();
    let b = bh_of(&o);
    steady(b);
    {
        let mut f = b.bh.borrow_mut();
        f.from_square = IntPair { x: 500, y: 8 };
        f.to_square = IntPair { x: 500, y: 8 };
        f.from_tile = None;
        f.to_tile = None;
        f.state.interacting = 1;
        f.state.interaction_type = it::DIG;
        f.action_queue.push(Rc::new(()));
    }
    tick(&o);
    assert!(b.bh.borrow().path_needs_recalculated);
    assert_eq!(b.st().interacting, 0);
}

#[test]
fn u5_blocked_destination_moves_back_to_the_origin() {
    let (world, _d, _dw, o, _h) = mk();
    let b = bh_of(&o);
    steady(b);
    tile(&world, 6, 8).set_type_index(1);
    place_between(b, &world, IntPair { x: 5, y: 8 }, IntPair { x: 6, y: 8 });
    b.bh.borrow_mut().path = Some(vec![step(idx(7, 8), 10, 1, 0)]);
    tick(&o);
    let f = b.bh.borrow();
    assert_eq!(f.to_square, IntPair { x: 5, y: 8 });
    assert!(f.path.is_none());
    assert!(f.path_needs_recalculated);
    drop(f);
    assert_eq!(b.float_pos(), Vector2 { x: 5.5, y: 8.0 });
}

#[test]
fn u5_pose_changes_follow_the_tile_environment() {
    // water: BOB
    let (world, _d, _dw, o, _h) = mk();
    let b = bh_of(&o);
    steady(b);
    water(&world, 6, 8, 255);
    water(&world, 6, 9, 255);
    place_between(b, &world, IntPair { x: 5, y: 8 }, IntPair { x: 6, y: 8 });
    tick(&o);
    assert_eq!(b.bh.borrow().traverse_to_key_frame, 1);
    assert_eq!(b.st().animation_type, va::BOB);
    // an elevator shaft with air below: a hang pose
    let (world, _d, _dw, o, _h) = mk();
    let b = bh_of(&o);
    steady(b);
    tile(&world, 6, 8).set_contents(0x67);
    tile(&world, 6, 7).set_type_index(2);
    tile(&world, 6, 6).set_type_index(2);
    place_between(b, &world, IntPair { x: 5, y: 8 }, IntPair { x: 6, y: 8 });
    tick(&o);
    assert_eq!(b.st().animation_type, va::HANG);
    // standing in a one-tile tunnel (floor and ceiling): crawl, the float position drops
    let (world, _d, _dw, o, _h) = mk();
    let b = bh_of(&o);
    steady(b);
    tile(&world, 5, 9).set_type_index(1);
    tick(&o);
    assert_eq!(b.st().animation_type, va::LIE);
    assert_eq!(b.float_pos().y, 7.5);
    // no floor, a ceiling two up: monkey hang
    let (world, _d, _dw, o, _h) = mk();
    let b = bh_of(&o);
    steady(b);
    tile(&world, 5, 7).set_type_index(2);
    tile(&world, 5, 10).set_type_index(1);
    tick(&o);
    assert_eq!(b.st().animation_type, va::MONKEY_HANG);
    assert_eq!(b.float_pos().y, 8.2);
    // no floor and a ceiling right above: falling. With an empty path this is a bail-out
    let (world, _d, _dw, o, _h) = mk();
    let b = bh_of(&o);
    steady(b);
    tile(&world, 5, 7).set_type_index(2);
    tile(&world, 5, 9).set_type_index(1);
    tick(&o);
    assert!(b.bh.borrow().path_needs_recalculated);
    // ... but with a path left the pose simply changes to falling
    let (world, _d, _dw, o, _h) = mk();
    let b = bh_of(&o);
    steady(b);
    tile(&world, 5, 7).set_type_index(2);
    tile(&world, 5, 9).set_type_index(1);
    b.bh.borrow_mut().path = Some(vec![step(idx(7, 8), 10, 1, 0)]);
    tick(&o);
    assert_eq!(b.bh.borrow().traverse_to_key_frame, kf::FALLING as i32);
    assert_eq!(b.st().animation_type, 0);
    let _ = world;
}

#[test]
fn u5_fishing_rod_is_updated_and_fliers_bail_while_falling() {
    let (world, _d, _dw, o, h) = mk();
    let b = bh_of(&o);
    steady(b);
    b.bh.borrow_mut().fishing_rod = Some(Rc::new(()));
    tick(&o);
    assert!(h.take().contains(&"rod:false".to_string()));
    // the flier branch: falling along a non-empty path and the test says fly
    h.set("canfly", 1);
    {
        let mut f = b.bh.borrow_mut();
        f.traverse_type = tt::FALL as i32;
        f.path = Some(vec![step(idx(7, 8), 10, 1, 0)]);
    }
    place_between(b, &world, IntPair { x: 5, y: 8 }, IntPair { x: 6, y: 8 });
    tick(&o);
    let _ = h.take();
}

// ---------------------------------------------------------------------------------------------------
// U6a
// ---------------------------------------------------------------------------------------------------

#[test]
fn u6a_actions_are_started_or_force_a_new_path() {
    let (_w, dw, _dwr, o, h) = mk();
    let b = bh_of(&o);
    steady(b);
    b.bh.borrow_mut().action_queue.push(Rc::new(()));
    // not in progress: a new path
    tick(&o);
    assert!(b.bh.borrow().path_needs_recalculated);
    assert_eq!(b.bh.borrow().action_queue.len(), 1);
    // in progress: popped, the dynamic world is told
    b.bh.borrow_mut().path_needs_recalculated = false;
    h.set("inprogress", 1);
    dw.changed.borrow_mut().clear();
    tick(&o);
    assert!(b.bh.borrow().action_queue.is_empty());
    assert_eq!(dw.changed.borrow().len(), 1);
    // a replica resets its action delay and does not tell the dynamic world
    b.base.borrow_mut().is_net = true;
    b.bh.borrow_mut().next_action_delay = 1.0;
    b.bh.borrow_mut().action_queue.push(Rc::new(()));
    dw.changed.borrow_mut().clear();
    tick(&o);
    assert!(dw.changed.borrow().is_empty());
}

#[test]
fn u6a_waiting_for_the_fill_response_times_out() {
    let (_w, _d, _dw, o, _h) = mk();
    let b = bh_of(&o);
    steady(b);
    {
        let mut f = b.bh.borrow_mut();
        f.action_queue.push(Rc::new(()));
        f.waiting_for_fill_response = true;
        f.waiting_for_fill_response_timer = 29.95;
    }
    tick(&o);
    assert!(!b.bh.borrow().waiting_for_fill_response);
}

// ---------------------------------------------------------------------------------------------------
// U6b: the interaction dispatch
// ---------------------------------------------------------------------------------------------------

struct Rig {
    world: Rc<World>,
    o: ObjRef,
    h: Rc<TestHost>,
}
impl Rig {
    fn b(&self) -> &Blockhead {
        bh_of(&self.o)
    }
    fn c(&self) -> Ctx<'_> {
        ctx(self.b(), &self.o, &self.world)
    }
}

/// Interacting at (6,7) (rock) with an item; low happiness / health make a tick add 0.08 to the timer.
fn rig(itype: u32, item: u32) -> Rig {
    let (world, _d, _dw, o, h) = mk();
    let b = bh_of(&o);
    steady(b);
    let t = tile(&world, 6, 7);
    h.item_available.set(true);
    {
        let mut f = b.bh.borrow_mut();
        f.state.interacting = 1;
        f.state.interaction_type = itype;
        f.state.interacting_square = IntPair { x: 6, y: 7 };
        f.interacting_tile = Some(t);
        f.interaction_item = Some(InventoryItem::new(ItemType(item), 0, 7, None, None));
        f.action_queue.push(Rc::new(()));
    }
    Rig { world, o, h }
}

/// timer before / after for a normal rate (+0.24 with happiness = health = 1)
fn fire(r: &Rig) {
    r.b().bh.borrow_mut().state.interaction_timer = 0.0;
    r.b().bh.borrow_mut().state.happiness = 1.0;
    u6b(&r.c(), DT);
}
fn expire(r: &Rig) {
    r.b().bh.borrow_mut().state.interaction_timer = 0.2;
    r.b().bh.borrow_mut().state.happiness = 1.0;
    u6b(&r.c(), DT);
}
fn wait(r: &Rig) {
    r.b().bh.borrow_mut().state.interaction_timer = 0.0;
    r.b().bh.borrow_mut().state.happiness = 0.0;
    r.b().bh.borrow_mut().state.health = 0.0;
    u6b(&r.c(), DT);
    r.b().bh.borrow_mut().state.health = 1.0;
}

#[test]
fn u6b_none_type_replica_and_unknown_types() {
    let r = rig(it::NONE, 8);
    fire(&r);
    assert_eq!(r.b().st().interacting, 0);
    // a replica only advances the timer
    let r = rig(it::STAB, 8);
    r.b().base.borrow_mut().is_net = true;
    let c = Ctx { is_net: true, ..r.c() };
    r.b().bh.borrow_mut().state.interaction_timer = 0.0;
    u6b(&c, DT);
    assert!(r.b().st().interaction_timer > 0.0);
    assert_eq!(r.b().st().interacting, 1);
    // an unhandled type
    let r = rig(it::RIDE, 8);
    fire(&r);
    assert_eq!(r.b().st().interacting, 1);
    // expert mode scales the rate by happiness
    let r = rig(it::RIDE, 8);
    let c = Ctx { expert: true, ..r.c() };
    r.b().bh.borrow_mut().state.interaction_timer = 0.0;
    u6b(&c, DT);
    assert!(r.b().st().interaction_timer > 0.0);
}

#[test]
fn u6b_paint_and_strip_paint() {
    for (ty, expect) in [(it::PAINT, "paint:7:0"), (it::STRIP_PAINT, "unpaint:0")] {
        let r = rig(ty, 8);
        fire(&r);
        let log = r.h.take();
        assert!(log.contains(&expect.to_string()), "{log:?}");
        assert!(log.contains(&"usage:false".to_string()));
        assert_eq!(log.contains(&"ach:grp.decorator".to_string()), ty == it::PAINT);
        // the face index comes from the path extra data
        let mut e = PathExtraData::new();
        e.set("craftCountOrExtraData", ExtraValue::Int(2));
        r.b().bh.borrow_mut().path_extra_data = Some(e);
        fire(&r);
        assert!(r.h.take().iter().any(|l| l.ends_with(":2")));
        // expiry stops, waiting does nothing
        wait(&r);
        assert!(r.h.take().is_empty());
        assert_eq!(r.b().st().interacting, 1);
        expire(&r);
        assert_eq!(r.b().st().interacting, 0);
        // not paintable / no item
        let r = rig(ty, 8);
        tile(&r.world, 6, 7).set_type_index(2);
        fire(&r);
        assert_eq!(r.b().st().interacting, 0);
        let r = rig(ty, 8);
        r.h.item_available.set(false);
        fire(&r);
        assert_eq!(r.b().st().interacting, 0);
    }
}

#[test]
fn u6b_feed_and_set_free() {
    let (n, _) = npc(0x23, false);
    let r = rig(it::FEED_NPC, 8);
    r.b().bh.borrow_mut().tapped_npc = Some(n.clone());
    r.h.set("canfeed", 1);
    fire(&r);
    let log = r.h.take();
    assert!(log.contains(&"feed".to_string()) && log.contains(&"removeitem:false".to_string()), "{log:?}");
    wait(&r);
    assert!(r.h.take().is_empty());
    expire(&r);
    assert_eq!(r.b().st().interacting, 0);
    let r = rig(it::FEED_NPC, 8);
    r.b().bh.borrow_mut().tapped_npc = Some(n.clone());
    fire(&r); // cannot be fed
    assert_eq!(r.b().st().interacting, 0);

    let r = rig(it::SET_NPC_FREE, 8);
    r.b().bh.borrow_mut().tapped_npc = Some(n.clone());
    r.h.belongs.set(true);
    fire(&r);
    assert!(r.h.take().contains(&"setfree".to_string()));
    wait(&r);
    assert!(r.h.take().is_empty());
    expire(&r);
    assert_eq!(r.b().st().interacting, 0);
    let r = rig(it::SET_NPC_FREE, 8);
    fire(&r); // no npc
    assert_eq!(r.b().st().interacting, 0);
}

#[test]
fn u6b_use_interactive_object_wakes_from_a_full_bed() {
    let r = rig(it::USE_INTERACTIVE_OBJECT, 8);
    r.h.set("bed", 1);
    r.b().bh.borrow_mut().interaction_object = Some(stub(1, true));
    r.b().bh.borrow_mut().state.energy = 1.15;
    let c = Ctx { sim_or_ctl: true, sim: true, ..r.c() };
    r.b().bh.borrow_mut().state.interaction_timer = 0.0;
    u6b(&c, DT);
    assert!(r.h.take().contains(&"sim:0:None".to_string()));
    assert!(r.b().bh.borrow().path_needs_recalculated);
    assert_eq!(r.b().st().interacting, 0);
    // not full: nothing
    let r = rig(it::USE_INTERACTIVE_OBJECT, 8);
    r.h.set("bed", 1);
    r.b().bh.borrow_mut().interaction_object = Some(stub(1, true));
    r.b().bh.borrow_mut().state.energy = 1.0;
    fire(&r);
    assert_eq!(r.b().st().interacting, 1);
}

#[test]
fn u6b_burn_sow_and_pickup() {
    let r = rig(it::BURN, 8);
    fire(&r);
    let log = r.h.take();
    assert!(log.contains(&"fill:6,7:13".to_string()) && log.contains(&"ach:grp.pyro".to_string()), "{log:?}");
    wait(&r);
    assert!(r.h.take().is_empty());
    expire(&r);
    assert_eq!(r.b().st().interacting, 0);
    let r = rig(it::BURN, 8);
    r.h.item_available.set(false);
    fire(&r);
    assert_eq!(r.b().st().interacting, 0);

    // sow: a plain seed lands one tile higher; a tulip seed also reports the tulip; compost soil the compost
    for (item, expect_y, extra) in [(0x127u32, 7, None), (ItemType::ITEM_TULIP_SEED.0, 8, Some("ach:grp.tulip"))] {
        let r = rig(it::SOW, item);
        tile(&r.world, 6, 7).set_type_index(0x30);
        fire(&r);
        let log = r.h.take();
        assert!(log.iter().any(|l| l.starts_with(&format!("fillx:6,{expect_y}:{item}:"))), "{log:?}");
        assert!(log.contains(&"ach:grp.greenfingers".to_string()));
        assert!(log.contains(&"ach:grp.compost".to_string()));
        if let Some(e) = extra {
            assert!(log.contains(&e.to_string()));
        }
        wait(&r);
        assert!(r.h.take().is_empty());
        expire(&r);
        assert_eq!(r.b().st().interacting, 0);
    }
    // simulated: the sow event and no achievements when controlled
    let r = rig(it::SOW, 0x127);
    let c = Ctx { sim: true, sim_or_ctl: true, ctl: true, ..r.c() };
    r.b().bh.borrow_mut().state.interaction_timer = 0.0;
    u6b(&c, DT);
    let log = r.h.take();
    assert!(log.contains(&"sim:3:ItemType(295)".to_string()), "{log:?}");
    assert!(!log.iter().any(|l| l.starts_with("ach:")));
    let r = rig(it::SOW, 8);
    r.h.item_available.set(false);
    fire(&r);
    assert_eq!(r.b().st().interacting, 0);

    // pickup
    let r = rig(it::PICKUP, 8);
    fire(&r);
    wait(&r);
    expire(&r);
    assert_eq!(r.b().st().interacting, 0);
}

#[test]
fn u6b_stab_force_table_and_achievements() {
    let (trollobj, troll) = npc(0x27, true);
    let (scorpobj, _) = npc(0x33, true);
    let (plain, _) = npc(1, false);
    let cases: [(Option<u32>, i32); 8] = [
        (None, 4),
        (Some(0x67), 4),
        (Some(0x32), 18),
        (Some(0x44), 24),
        (Some(0x11e), 36),
        (Some(7), 12),
        (Some(ItemType::ITEM_FLINT_PICKAXE.0), 8),
        (Some(1), 4),
    ];
    for (item, force) in cases {
        let r = rig(it::STAB, item.unwrap_or(8));
        if item.is_none() {
            r.b().bh.borrow_mut().interaction_item = None;
        }
        r.b().bh.borrow_mut().tapped_npc = Some(trollobj.clone());
        r.b().bh.borrow_mut().chasing_npc = true;
        troll.hits.borrow_mut().clear();
        u6_stab(&r.c(), 0.0, 0.2);
        assert_eq!(*troll.hits.borrow(), vec![force], "{item:?}");
        assert!(!r.b().bh.borrow().chasing_npc);
        assert!(r.h.take().contains(&"ach:grp.troll".to_string()));
    }
    // expert halves the force
    let r = rig(it::STAB, 0x32);
    r.b().bh.borrow_mut().tapped_npc = Some(trollobj.clone());
    troll.hits.borrow_mut().clear();
    u6_stab(&Ctx { expert: true, ..r.c() }, 0.0, 0.2);
    assert_eq!(*troll.hits.borrow(), vec![9]);
    // scorpion, plain npc, no npc, controlled (no achievements)
    let r = rig(it::STAB, 8);
    r.b().bh.borrow_mut().tapped_npc = Some(scorpobj);
    u6_stab(&r.c(), 0.0, 0.2);
    assert!(r.h.take().contains(&"ach:grp.scorpion".to_string()));
    r.b().bh.borrow_mut().tapped_npc = Some(plain);
    u6_stab(&r.c(), 0.0, 0.2);
    assert!(!r.h.take().iter().any(|l| l.starts_with("ach:")));
    r.b().bh.borrow_mut().tapped_npc = None;
    u6_stab(&r.c(), 0.0, 0.2);
    r.b().bh.borrow_mut().tapped_npc = Some(trollobj);
    u6_stab(&Ctx { ctl: true, ..r.c() }, 0.0, 0.2);
    assert!(!r.h.take().iter().any(|l| l.starts_with("ach:")));
    // timing
    u6_stab(&r.c(), 0.0, 0.1);
    assert_eq!(r.b().st().interacting, 1);
    u6_stab(&r.c(), 0.2, 0.4);
    assert_eq!(r.b().st().interacting, 0);
}

#[test]
fn u6b_fire_ranged_weapon() {
    let (target, _) = npc(0x27, true);
    let (scorp, _) = npc(0x33, true);
    for (item, proj, poison) in [(8u32, 0x29, false), (0x101, 0x100, true)] {
        let r = rig(it::FIRE_RANGED_WEAPON, item);
        {
            let mut f = r.b().bh.borrow_mut();
            f.tapped_npc = Some(target.clone());
            f.last_known_npc_position = IntPair { x: 7, y: 8 };
            f.chasing_npc = true;
        }
        u6_fire(&r.c(), 0.0, 0.2);
        let log = r.h.take();
        assert!(log.contains(&format!("fire:{proj}")), "{log:?}");
        assert!(log.contains(&"ach:grp.bow".to_string()));
        assert!(log.contains(&"ach:grp.troll".to_string()));
        assert_eq!(log.contains(&"ach:grp.poison".to_string()), poison);
        assert!(log.contains(&"usage:true".to_string()));
        assert!(!r.b().bh.borrow().chasing_npc);
    }
    // crouching shooter, scorpion, no npc, controlled
    let r = rig(it::FIRE_RANGED_WEAPON, 8);
    {
        let mut f = r.b().bh.borrow_mut();
        f.tapped_npc = Some(scorp);
        f.last_known_npc_position = IntPair { x: 7, y: 8 };
        f.state.animation_type = 23;
    }
    u6_fire(&r.c(), 0.0, 0.2);
    assert!(r.h.take().contains(&"ach:grp.scorpion".to_string()));
    r.b().bh.borrow_mut().tapped_npc = None;
    u6_fire(&r.c(), 0.0, 0.2);
    assert!(r.h.take().contains(&"fire:41".to_string()));
    u6_fire(&Ctx { ctl: true, ..r.c() }, 0.0, 0.2);
    assert!(!r.h.take().iter().any(|l| l.starts_with("ach:")));
    // out of range: nothing is fired
    r.b().bh.borrow_mut().last_known_npc_position = IntPair { x: 100, y: 8 };
    u6_fire(&r.c(), 0.0, 0.2);
    assert!(!r.h.take().iter().any(|l| l.starts_with("fire:")));
    // timing
    u6_fire(&r.c(), 0.0, 0.1);
    assert_eq!(r.b().st().interacting, 1);
    u6_fire(&r.c(), 0.2, 0.4);
    assert_eq!(r.b().st().interacting, 0);
}

#[test]
fn u6_craft_tips_and_gather_timer() {
    let r = rig(it::CRAFT, 8);
    // no workbench: nothing
    u6_craft(&r.c(), DT);
    assert_eq!(r.b().bh.borrow().tip_type, 0);
    r.b().bh.borrow_mut().interaction_workbench = Some(stub(1, true));
    // an ordinary bench with fuel advances the gather timer
    r.b().bh.borrow_mut().state.gather_timer = 0.2;
    u6_craft(&r.c(), DT);
    assert_eq!(r.b().st().gather_timer, 0.2f32 + DT - 0.3f32);
    // exhausted / starving tips
    for (reason, tip) in [(1i64, 3), (2, 4)] {
        r.b().bh.borrow_mut().tip_type = 0;
        r.h.set("unable", reason);
        u6_craft(&r.c(), DT);
        assert_eq!(r.b().bh.borrow().tip_type, tip);
    }
    // an existing tip is kept
    r.b().bh.borrow_mut().tip_type = 9;
    u6_craft(&r.c(), DT);
    assert_eq!(r.b().bh.borrow().tip_type, 9);
    r.h.set("unable", 0);
    // listed workbench kinds and electric benches skip the gather timer
    r.b().bh.borrow_mut().state.gather_timer = 0.0;
    r.h.set("wbtype", 1);
    u6_craft(&r.c(), DT);
    assert_eq!(r.b().st().gather_timer, 0.0);
    r.h.set("wbtype", 2);
    r.h.set("wbelec", 1);
    u6_craft(&r.c(), DT);
    assert_eq!(r.b().st().gather_timer, 0.0);
    r.h.set("wbelec", 0);
    // missing fuel: tip 5, or 6 when the bench needs electricity
    r.b().bh.borrow_mut().tip_type = 0;
    r.h.set("nofuel", 1);
    r.h.set("wbtype", 2);
    u6_craft(&r.c(), DT);
    assert_eq!(r.b().bh.borrow().tip_type, 5);
    r.b().bh.borrow_mut().tip_type = 0;
    r.h.set("wbtype", 0x1f);
    u6_craft(&r.c(), DT);
    assert!(matches!(r.b().bh.borrow().tip_type, 5 | 6));
    // the dispatch from u6b reaches it too
    r.b().bh.borrow_mut().state.interaction_type = it::CRAFT;
    fire(&r);
}

// ---------------------------------------------------------------------------------------------------
// U6 dig
// ---------------------------------------------------------------------------------------------------

/// A DIG rig on tile (6,7) ready to finish with the next gather step.
fn dig_rig(item: u32, ttype: u8, contents: u8, fg: u8, cit: u32, progress: u8) -> Rig {
    let r = rig(it::DIG, item);
    let t = tile(&r.world, 6, 7);
    t.set_type_index(ttype);
    t.set_contents(contents);
    t.set_foreground_contents(fg);
    t.set_gather_progress(progress);
    r.h.cit.set(cit);
    {
        let mut f = r.b().bh.borrow_mut();
        f.state.gather_timer = 0.5;
        f.state.gather_speed = 1.0;
        f.local_gather_progress = 0;
    }
    r
}

fn dig(r: &Rig) {
    let c = Ctx { sim: true, sim_or_ctl: true, ..r.c() };
    u6_dig(&c, DT);
}

#[test]
fn dig_rail_removal_and_its_exception() {
    let r = dig_rig(0xd1, 2, 0, 0x62, it::DIG_FOREGROUND, 0);
    dig(&r);
    let log = r.h.take();
    assert!(log.iter().any(|l| l.starts_with("fb:6,7:199:")), "{log:?}");
    assert!(log.contains(&"remove:Rail:6,7".to_string()));
    assert!(log.contains(&"removetile:6,7:0:0:false:true".to_string()), "{log:?}");
    // a rail on top of a type-6 interaction object at contents 0x30 is not removed as a rail
    let r = dig_rig(0xd1, 2, 0x30, 0x62, it::DIG_FOREGROUND, 0);
    r.h.set_obj("io_at", Some(stub(1, true)));
    r.h.set("iotype", 6);
    dig(&r);
    let log = r.h.take();
    assert!(!log.contains(&"remove:Rail:6,7".to_string()), "{log:?}");
}

#[test]
fn dig_magnet_without_ore_and_zero_increment() {
    let r = dig_rig(0xaf, 0x3a, 0, 0, it::DIG, 0);
    dig(&r);
    let log = r.h.take();
    assert!(log.iter().any(|l| l.starts_with("removetile:6,7:")), "{log:?}");
    assert!(!log.contains(&"usage:false".to_string()));
    // a speed of zero gathers nothing and remembers the leftover
    let r = dig_rig(8, 6, 0, 0, it::DIG, 0);
    r.b().bh.borrow_mut().state.gather_speed = 0.0;
    r.b().bh.borrow_mut().gather_timer_left_over = 0.5;
    dig(&r);
    assert_eq!(r.b().bh.borrow().gather_timer_left_over, 0.5);
}

#[test]
fn dig_action_result_mismatch_stops() {
    let r = dig_rig(8, 6, 0, 0, it::DIG, 0);
    r.h.set("gob", 1);
    r.h.set("gob_action", 2);
    dig(&r);
    assert_eq!(r.b().st().interacting, 0);
    // matching results keep going
    let r = dig_rig(8, 6, 0, 0, it::DIG, 0);
    r.h.set("gob", 2);
    r.h.set("gob_action", 2);
    dig(&r);
}

#[test]
fn dig_bonus_roll_creates_free_blocks() {
    for item in [0x59u32, 0x5a] {
        let r = dig_rig(item, 1, 0, 0, it::DIG, 0);
        seed_first(|v| r01(v) < 0.0625);
        dig(&r);
        let log = r.h.take();
        assert!(log.iter().any(|l| l.starts_with("fb:6,7:") && l.ends_with(":false:1:true")), "{log:?}");
    }
    // a failed roll makes nothing
    let r = dig_rig(0x59, 1, 0, 0, it::DIG, 0);
    seed_first(|v| r01(v) >= 0.0625);
    dig(&r);
    assert!(!r.h.take().iter().any(|l| l.starts_with("fb:")));
    // a tile without bonus: no free block
    let r = dig_rig(0x59, 2, 0, 0, it::DIG, 0);
    seed_first(|v| r01(v) < 0.0625);
    dig(&r);
    assert!(!r.h.take().iter().any(|l| l.starts_with("fb:")));
}

#[test]
fn dig_trunks_and_soil_drop_rolls() {
    for (ti, p) in [(6u8, 0.03125f64), (0x1b, 0.25), (0x1c, 0.25), (0x30, 0.0625), (0x31, 0.5), (0x32, 0.5)] {
        let r = dig_rig(8, ti, 0, 0, it::DIG, 250);
        seed_first(|v| r01(v) < p);
        dig(&r);
        assert!(r.h.take().iter().any(|l| l.starts_with("fb:6,7:155:")), "ti {ti}");
        let r = dig_rig(8, ti, 0, 0, it::DIG, 250);
        seed_first(|v| r01(v) >= p);
        dig(&r);
        assert!(!r.h.take().iter().any(|l| l.starts_with("fb:6,7:155:")), "ti {ti}");
    }
}

#[test]
fn dig_limestone_workbench_and_interaction_objects() {
    let r = dig_rig(8, 12, 0, 0, it::DIG, 250);
    seed_first(|v| v % 16 == 0);
    dig(&r);
    let log = r.h.take();
    assert!(log.iter().any(|l| l.starts_with("fb:6,7:160:")) && log.contains(&"ach:grp.lime".to_string()), "{log:?}");
    let r = dig_rig(8, 12, 0, 0, it::DIG, 250);
    seed_first(|v| v % 16 != 0);
    dig(&r);
    assert!(!r.h.take().contains(&"ach:grp.lime".to_string()));
    // an interaction object in the foreground contents of a block
    let r = dig_rig(8, 1, 0x30, 0, it::DIG, 250);
    dig(&r);
    assert!(r.h.take().contains(&"rmio".to_string()));
    for co in [0x2eu8, 0x2f] {
        let r = dig_rig(8, 1, co, 0, it::DIG, 250);
        dig(&r);
        assert!(r.h.take().contains(&"rmworkbench".to_string()));
    }
    let r = dig_rig(8, 4, 0x30, 0, it::DIG, 250);
    dig(&r);
    assert!(!r.h.take().contains(&"rmio".to_string()));
    // an interaction object named by the action
    let r = dig_rig(8, 6, 0, 0, it::DIG, 250);
    r.h.set("aoid", 5);
    *r.h.io_by_id.borrow_mut() = Some(stub(1, true));
    r.b().bh.borrow_mut().action_queue = vec![Rc::new(())];
    dig(&r);
    assert!(r.h.take().contains(&"ioremove".to_string()));
    let r = dig_rig(8, 6, 0, 0, it::DIG, 250);
    r.h.set("aoid", 5);
    r.h.set("iouse", 1);
    *r.h.io_by_id.borrow_mut() = Some(stub(1, true));
    dig(&r);
    assert!(!r.h.take().contains(&"ioremove".to_string()));
}

#[test]
fn dig_removal_variants_and_usage() {
    // back wall
    let r = dig_rig(8, 6, 0, 0, it::DIG_BACK_WALL, 250);
    dig(&r);
    assert!(r.h.take().contains(&"rmbackwall:6,7".to_string()));
    // foreground: the free blocks come from the foreground count
    let r = dig_rig(8, 6, 0, 0x30, it::DIG_FOREGROUND, 250);
    dig(&r);
    assert!(r.h.take().iter().any(|l| l.starts_with("removetile:6,7:0:") && l.ends_with(":false:false")));
    // doors, trap doors and gates make no free blocks
    for (bg, co) in [(0x46u8, 0u8), (0x4b, 0), (0, 0x92)] {
        let r = dig_rig(8, 6, co, 0, it::DIG, 250);
        tile(&r.world, 6, 7).set_background_contents(bg);
        dig(&r);
        assert!(r.h.take().iter().any(|l| l.starts_with("removetile:6,7:0:0:")), "{bg} {co}");
    }
    // achievements and usage exceptions
    let r = dig_rig(0x22, 1, 0, 0, it::DIG, 250);
    let c = Ctx { sim: true, sim_or_ctl: true, ..r.c() };
    u6_dig(&c, DT);
    assert!(r.h.take().contains(&"ach:grp.mine".to_string()));
    let r = dig_rig(8, 0x33, 0, 0, it::DIG, 250);
    dig(&r);
    assert!(r.h.take().contains(&"ach:grp.basalt".to_string()));
    let r = dig_rig(0x67, 6, 0, 0, it::DIG, 250);
    dig(&r);
    assert!(!r.h.take().contains(&"usage:false".to_string()));
    // the dropped magnet with `onlyRemoveContents`
    let r = dig_rig(0xaf, 1, 0x3f, 0, it::DIG, 250);
    dig(&r);
    // a replica or controlled blockhead reports nothing
    let r = dig_rig(0x22, 1, 0, 0, it::DIG, 250);
    u6_dig(&Ctx { sim: true, sim_or_ctl: true, ctl: true, ..r.c() }, DT);
    assert!(!r.h.take().iter().any(|l| l.starts_with("ach:")));
    // with particles (not simulated): the draws are made
    let r = dig_rig(8, 6, 0, 0, it::DIG, 0);
    u6_dig(&r.c(), DT);
}

#[test]
fn dig_progress_notifications_and_doors() {
    let r = dig_rig(8, 6, 0, 0, it::DIG, 0);
    dig(&r);
    let log = r.h.take();
    assert!(log.contains(&"gatherblock:6,7".to_string()) && log.contains(&"gather:6,7".to_string()), "{log:?}");
    assert_eq!(tile(&r.world, 6, 7).gather_progress(), 8);
    // a door shares its progress with the other half
    let r = dig_rig(8, 6, 0, 0, it::DIG, 0);
    tile(&r.world, 6, 7).set_background_contents(0x46);
    r.h.set("doorx", 6);
    r.h.set("doory", 8);
    dig(&r);
    let log = r.h.take();
    assert!(log.contains(&"gatherblock:6,8".to_string()), "{log:?}");
    assert_eq!(tile(&r.world, 6, 8).gather_progress(), 8);
    // the other half is the same tile: nothing more
    let r = dig_rig(8, 6, 0, 0, it::DIG, 0);
    tile(&r.world, 6, 7).set_background_contents(0x46);
    r.h.set("doorx", 6);
    r.h.set("doory", 7);
    dig(&r);
    // the rock that needs no loading
    let r = dig_rig(8, 1, 0, 0, it::DIG, 0);
    dig(&r);
    assert!(!r.h.take().contains(&"gatherblock:6,7".to_string()));
}

#[test]
fn dig_plants() {
    // no plant object: the dig ends and the foreground is cleared
    let r = dig_rig(8, 2, 0, 0x43, it::DIG, 0);
    dig(&r);
    assert_eq!(r.b().st().interacting, 0);
    assert_eq!(tile(&r.world, 6, 7).foreground_contents(), 0);
    // progress on the plant
    let r = dig_rig(8, 2, 0, 0x43, it::DIG, 0);
    r.h.set_obj("plant", Some(stub(1, true)));
    // TestHost answers `get_plant_at_pos` from its `plant` slot
    *r.h.plant.borrow_mut() = Some(stub(1, true));
    r.h.set("plantprog", 10);
    dig(&r);
    assert!(r.h.take().contains(&"plantset:18".to_string()));
    // the harvest
    r.h.set("plantprog", 250);
    r.h.set("plantn", 2);
    r.b().bh.borrow_mut().state.gather_timer = 0.5;
    dig(&r);
    let log = r.h.take();
    assert!(log.contains(&"plantharvest:1".to_string()), "{log:?}");
    assert_eq!(r.b().st().interacting, 0);
}

// ---------------------------------------------------------------------------------------------------
// U6 fill bucket / capture / harvest
// ---------------------------------------------------------------------------------------------------

#[test]
fn fill_bucket_prerequisites_and_timing() {
    let r = rig(it::FILL_BUCKET, 0x66);
    r.h.item_available.set(false);
    r.b().bh.borrow_mut().chasing_npc = true;
    u6_fill_bucket(&r.c(), it::FILL_BUCKET, 0.0, 0.2);
    assert_eq!(r.b().st().interacting, 0);
    assert!(!r.b().bh.borrow().chasing_npc);
    let r = rig(it::FILL_BUCKET, 0x66);
    u6_fill_bucket(&r.c(), it::FILL_BUCKET, 0.0, 0.1);
    assert_eq!(r.b().st().interacting, 1);
    r.b().bh.borrow_mut().chasing_npc = true;
    u6_fill_bucket(&r.c(), it::FILL_BUCKET, 0.2, 0.4);
    assert_eq!(r.b().st().interacting, 0);
    assert!(!r.b().bh.borrow().chasing_npc);
}

#[test]
fn capture_cages_and_shears() {
    let (n, _) = npc(0x23, false);
    // cage with the tapped npc
    let r = rig(it::CAPTURE, 0x12e);
    r.b().bh.borrow_mut().tapped_npc = Some(n.clone());
    r.h.set("capture_ok", 1);
    u6_fill_bucket(&r.c(), it::CAPTURE, 0.0, 0.2);
    let log = r.h.take();
    assert!(log.contains(&"capture:302".to_string()) && log.contains(&"removeitem:false".to_string()), "{log:?}");
    // cage without a tapped npc: looked up under the tap, with and without pvp
    for pvp in [false, true] {
        let r = rig(it::CAPTURE, 0x12e);
        r.world.set_pvp_enabled(pvp);
        r.h.set_obj("harm", Some(n.clone()));
        u6_fill_bucket(&r.c(), it::CAPTURE, 0.0, 0.2);
        let log = r.h.take();
        assert!(log.contains(&format!("harmcheck:{}", !pvp)), "{log:?}");
        assert!(log.contains(&"capture:302".to_string()));
    }
    // a failed capture keeps the item
    let r = rig(it::CAPTURE, 0x12e);
    r.b().bh.borrow_mut().tapped_npc = Some(n.clone());
    u6_fill_bucket(&r.c(), it::CAPTURE, 0.0, 0.2);
    assert!(!r.h.take().contains(&"removeitem:false".to_string()));
    // shears go to the tapped npc only
    let r = rig(it::HARVEST_NPC, 0x142);
    r.b().bh.borrow_mut().tapped_npc = Some(n.clone());
    r.h.set("shave_ok", 1);
    u6_fill_bucket(&r.c(), it::HARVEST_NPC, 0.0, 0.2);
    let log = r.h.take();
    assert!(log.contains(&"shave".to_string()) && log.contains(&"usage:false".to_string()), "{log:?}");
    let r = rig(it::HARVEST_NPC, 0x142);
    r.h.set_obj("harm", Some(n.clone()));
    u6_fill_bucket(&r.c(), it::HARVEST_NPC, 0.0, 0.2);
    assert!(!r.h.take().contains(&"shave".to_string()));
}

#[test]
fn buckets_capture_milk_and_fill() {
    // baby shark / fish
    for (ty, shark) in [(0x23, false), (0x24, true)] {
        let (n, _) = npc(ty, false);
        let r = rig(it::CAPTURE, 0x66);
        r.b().bh.borrow_mut().tapped_npc = Some(n);
        r.h.set("capture_ok", 1);
        u6_fill_bucket(&r.c(), it::CAPTURE, 0.0, 0.2);
        let log = r.h.take();
        assert!(log.contains(&"capture:102".to_string()), "{log:?}");
        assert!(log.contains(&"removeitem:false".to_string()));
        assert_eq!(log.contains(&"ach:grp.baby_shark".to_string()), shark);
    }
    // milking, with and without success
    let (cow, _) = npc(0x3f, false);
    for ok in [true, false] {
        let r = rig(it::HARVEST_NPC, 0x66);
        r.b().bh.borrow_mut().tapped_npc = Some(cow.clone());
        r.h.set("milk_ok", ok as i64);
        u6_fill_bucket(&r.c(), it::HARVEST_NPC, 0.0, 0.2);
        let log = r.h.take();
        assert_eq!(log.contains(&"removeitem:false".to_string()), ok, "{log:?}");
    }
    // milk without a tapped npc, another kind of npc
    let r = rig(it::HARVEST_NPC, 0x66);
    r.h.set_obj("harm", Some(cow.clone()));
    u6_fill_bucket(&r.c(), it::HARVEST_NPC, 0.0, 0.2);
    let (other, _) = npc(0x99, false);
    r.b().bh.borrow_mut().tapped_npc = Some(other);
    u6_fill_bucket(&r.c(), it::HARVEST_NPC, 0.0, 0.2);
    // scooping water
    for (expert, pcl, expect) in [(false, 0x40u8, 0x69), (true, 0x40, 0x66), (true, 0xf0, 0x69)] {
        let r = rig(it::FILL_BUCKET, 0x66);
        water(&r.world, 6, 7, pcl);
        r.b().bh.borrow_mut().interacting_tile = Some(tile(&r.world, 6, 7));
        u6_fill_bucket(&Ctx { expert, ..r.c() }, it::FILL_BUCKET, 0.0, 0.2);
        let log = r.h.take();
        assert!(log.contains(&"rmwater".to_string()), "{log:?}");
        assert!(log.iter().any(|l| l.starts_with(&format!("fb:6,7:{expect}:"))), "{log:?}");
    }
    // too little water
    let r = rig(it::FILL_BUCKET, 0x66);
    water(&r.world, 6, 7, 0x10);
    r.b().bh.borrow_mut().interacting_tile = Some(tile(&r.world, 6, 7));
    u6_fill_bucket(&r.c(), it::FILL_BUCKET, 0.0, 0.2);
    assert!(!r.h.take().contains(&"rmwater".to_string()));
    // other items do nothing
    let r = rig(it::FILL_BUCKET, 8);
    u6_fill_bucket(&r.c(), it::FILL_BUCKET, 0.0, 0.2);
}

// ---------------------------------------------------------------------------------------------------
// U6 place
// ---------------------------------------------------------------------------------------------------

#[test]
fn place_item_effects_by_kind() {
    let cases: Vec<(ItemType, Vec<&str>)> = vec![
        (ItemType::ITEM_DODO_CAGE, vec!["fb:6,7:302:"]),
        (ItemType::ITEM_WATER_BUCKET, vec!["fb:6,7:102:", "fillx:"]),
        (ItemType::ITEM_FISH_BUCKET, vec!["fb:6,7:102:"]),
        (ItemType::ITEM_SHARK_BUCKET, vec!["fb:6,7:102:"]),
        (ItemType::ITEM_CAMPFIRE, vec!["wbplace:", "ach:grp.campfire"]),
        (ItemType::ITEM_SOLAR_PANEL, vec!["wbplace:", "ach:grp.solar"]),
        (ItemType::ITEM_OWNERSHIP_SIGN, vec!["ioplace", "ach:grp.sign", "ownershipareas"]),
        (ItemType::ITEM_CLAY_LANTERN_LIT, vec!["fillx:", "ach:grp.lantern"]),
        (ItemType::ITEM_STEEL_LANTERN, vec!["ach:grp.lantern"]),
        (ItemType::ITEM_GOLD_BLOCK, vec!["ach:grp.gold_builder"]),
        (ItemType::ITEM_STEEL_BLOCK, vec!["ach:grp.steel_block"]),
        (ItemType::ITEM_PAINTING_L_S, vec!["ach:grp.painting"]),
        (ItemType::ITEM_STEEL_DOWNLIGHT, vec!["ach:grp.spotlight"]),
        (ItemType::ITEM_STEEL_UPLIGHT, vec!["ach:grp.spotlight"]),
        (ItemType(1), vec!["fillx:"]),
    ];
    for (item, expect) in cases {
        let r = rig(it::PLACE_ITEM, item.0);
        r.h.cit.set(it::PLACE_ITEM);
        r.h.set_obj("placed_io", Some(stub(1, true)));
        u6_place(&r.c(), it::PLACE_ITEM, 0.0, 0.2);
        let log = r.h.take();
        for e in expect {
            assert!(log.iter().any(|l| l.starts_with(e)), "{item:?}: {e} not in {log:?}");
        }
        assert!(log.contains(&"removeitem:false".to_string()));
    }
}

#[test]
fn place_sign_starts_the_text_dialog_and_events() {
    let r = rig(it::PLACE_ITEM, ItemType::ITEM_SIGN.0);
    r.h.cit.set(it::PLACE_ITEM);
    r.h.set_obj("placed_io", Some(stub(1, true)));
    r.h.set("invchg", 3);
    let c = Ctx { sim: true, sim_or_ctl: true, ..r.c() };
    u6_place(&c, it::PLACE_ITEM, 0.0, 0.2);
    let log = r.h.take();
    assert!(log.contains(&"ach:grp.sign".to_string()), "{log:?}");
    assert!(log.iter().any(|l| l.starts_with("sim:3:ItemType(163)")));
    assert!(log.contains(&"setinvchg:163:2".to_string()), "{log:?}");
    assert!(r.b().bh.borrow().path_extra_data.is_some());
    // a controlled blockhead gets no achievement
    let r = rig(it::PLACE_ITEM, ItemType::ITEM_SIGN.0);
    r.h.cit.set(it::PLACE_ITEM);
    u6_place(&Ctx { ctl: true, ..r.c() }, it::PLACE_ITEM, 0.0, 0.2);
    assert!(!r.h.take().iter().any(|l| l.starts_with("ach:")));
    // the timing rule and prerequisites
    let r = rig(it::PLACE_ITEM, 8);
    r.h.cit.set(it::PLACE_ITEM);
    u6_place(&r.c(), it::PLACE_ITEM, 0.0, 0.1);
    assert_eq!(r.b().st().interacting, 1);
    u6_place(&r.c(), it::PLACE_ITEM, 0.2, 0.4);
    assert_eq!(r.b().st().interacting, 0);
    let r = rig(it::PLACE_ITEM, 8);
    r.h.item_available.set(false);
    u6_place(&r.c(), it::PLACE_ITEM, 0.0, 0.2);
    assert_eq!(r.b().st().interacting, 0);
}
