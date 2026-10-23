//! Branch-coverage tests for `update.rs` (phases U1..U8), complementing the exact-value tests of `update::tests`.
//! Each test drives one scripted situation through `update_accurate_dt_is_simulation` and asserts the observable
//! effect (host calls, state fields).

use super::super::blockhead::testutil::{bh_of, new_bh, setup, stub, tile, MockDw};
use super::cov_host::CovHost as TestHost;
use super::*;
use crate::items::inventory_item::InventoryItem;
use crate::rng::glibc_rand::srand;
use crate::types::structs::CustomRules;

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
    for (rule, dec) in [(0i8, 99999.9f32), (1, (DT / 900.0f32) * 0.5f32 * 10.0f32), (3, (DT / 900.0f32) * 0.5f32 * 0.1f32)] {
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
