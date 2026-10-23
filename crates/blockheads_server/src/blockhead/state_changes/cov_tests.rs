//! Coverage tests for `state_changes.rs`: interaction start / stop, pickups, the network update receiver, damage
//! and death under mods, all through the scripted `CovHost`.

use super::super::blockhead::testutil::{bh_of, new_bh, setup, stub, tile, MockDw};
use super::super::update::cov_host::CovHost;
use super::*;
use crate::items::inventory_item::InventoryItem;
use crate::mods::{ChatVerdict, LoaderKey, ModHooks, Optional};
use crate::objects::base::{DynamicObject, DynamicObjectBase};
use crate::objects::class_info::{ClassInfo, DYNAMIC_OBJECT_CLASS};
use crate::types::netdata::BlockheadUpdateData;
use crate::types::structs::IntPair;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

type Fixture = (Rc<World>, Rc<MockDw>, crate::objects::base::DynamicWorldRef, ObjRef, Rc<CovHost>);

fn mk() -> Fixture {
    let h = CovHost::install();
    let (world, dw, dwref) = setup();
    let o = new_bh(&world, &dwref, 0);
    bh_of(&o).prepare_inventory_for_saving();
    h.take();
    (world, dw, dwref, o, h)
}

fn idx(x: i32, y: i32) -> i32 {
    y * 32 * 32 + x
}

/// A ridable / harmable-less object with a class, an optional proxy interaction object and a rider position.
struct Ride {
    base: RefCell<DynamicObjectBase>,
    class: &'static ClassInfo,
    proxy: RefCell<Option<ObjRef>>,
    removable: Cell<bool>,
}
impl DynamicObject for Ride {
    fn base(&self) -> &RefCell<DynamicObjectBase> {
        &self.base
    }
    fn class_info(&self) -> &'static ClassInfo {
        self.class
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn as_ridable(&self) -> Option<&dyn crate::objects::protocols::RidableDynamicObject> {
        Some(self)
    }
}
impl crate::objects::protocols::RidableDynamicObject for Ride {
    fn rider_pos_for_blockhead(&self, _b: &ObjRef) -> [f32; 4] {
        [0.0; 4]
    }
    fn camera_pos_for_blockhead(&self, _b: &ObjRef) -> Vector2 {
        Vector2 { x: 0.0, y: 0.0 }
    }
    fn rider_body_y_rotation_for_blockhead(&self, _b: &ObjRef) -> f32 {
        0.0
    }
    fn set_target_velocity(&self, _v: Vector2) {}
    fn ride_direction(&self) -> i32 {
        0
    }
    fn add_rider(&self, _me: &ObjRef, _b: &ObjRef) {}
    fn remove_rider(&self, _me: &ObjRef, _b: &ObjRef) {}
    fn swipe_up_gesture(&self, _me: &ObjRef) {}
    fn blockhead_can_ride_using_item(&self, _b: &ObjRef, _item: i32) -> bool {
        true
    }
    fn jumps_on_swipe(&self) -> bool {
        false
    }
    fn requires_fuel(&self) -> bool {
        false
    }
    fn acts_as_interaction_object(&self) -> Option<ObjRef> {
        self.proxy.borrow().clone()
    }
    fn rider_d_pad_should_allow_up_down(&self) -> bool {
        false
    }
    fn rider_d_pad_should_give_discrete_values(&self) -> bool {
        false
    }
}
fn ride(class: &'static ClassInfo, proxy: Option<ObjRef>) -> ObjRef {
    crate::objects::handle::alloc_obj(|w| Ride {
        base: RefCell::new(DynamicObjectBase::alloc_zeroed_with(w)),
        class,
        proxy: RefCell::new(proxy),
        removable: Cell::new(true),
    })
}

fn t_(w: &World, x: i32, y: i32) -> Option<TileRef> {
    Some(tile(w, x, y))
}

// ---------------------------------------------------------------------------------------------------
// startInteracting
// ---------------------------------------------------------------------------------------------------

#[test]
fn start_riding_an_npc() {
    let (world, _d, _dw, o, h) = mk();
    let b = bh_of(&o);
    let start = |t: u32| b.start_interacting_with_tile_at_index_tile_interaction_type(idx(6, 8), t_(&world, 6, 8), t);
    // an ordinary mount, a mount ridden with arms down, a donkey
    for (class, expect_anim) in
        [(&DYNAMIC_OBJECT_CLASS, va::RIDE), (&crate::npc::dodo::DODO_CLASS_INFO, va::RIDE_ARMS_DOWN)]
    {
        b.bh.borrow_mut().tapped_npc = Some(ride(class, None));
        b.bh.borrow_mut().chasing_npc = true;
        h.set("canride_flag", 1);
        h.can_ride.set(true);
        start(it::RIDE);
        assert_eq!(b.st().animation_type, expect_anim);
        assert!(b.bh.borrow().ride_object.is_some());
        assert!(!b.bh.borrow().chasing_npc);
    }
    h.set("donkey", 1);
    for (breed, expect) in [(3, true), (12, false)] {
        h.set("breed", breed);
        h.take();
        start(it::RIDE);
        assert_eq!(h.take().contains(&"ach:grp.donkey".to_string()), expect);
    }
    h.set("donkey", 0);
    // replicas and controlled blockheads report nothing
    b.base.borrow_mut().is_net = true;
    h.take();
    start(it::RIDE);
    assert!(h.take().iter().all(|l| !l.starts_with("ach:")));
    b.base.borrow_mut().is_net = false;
    b.bh.borrow_mut().is_client_blockhead_being_controlled_by_server = true;
    start(it::RIDE);
    assert!(h.take().iter().all(|l| !l.starts_with("ach:")));
    b.bh.borrow_mut().is_client_blockhead_being_controlled_by_server = false;
    // a mount that refuses
    h.can_ride.set(false);
    start(it::RIDE);
    assert!(b.bh.borrow().ride_object.is_none());
}

#[test]
fn start_riding_boats_and_train_cars() {
    let (world, _d, _dw, o, h) = mk();
    let b = bh_of(&o);
    h.can_ride.set(true);
    let start = |t: u32| b.start_interacting_with_tile_at_index_tile_interaction_type(idx(6, 8), t_(&world, 6, 8), t);
    // boats
    b.bh.borrow_mut().tapped_ride_object = Some(ride(&DYNAMIC_OBJECT_CLASS, None));
    b.bh.borrow_mut().chasing_ride_object = true;
    start(it::RIDE_BOAT);
    assert_eq!(b.st().animation_type, va::RIDE);
    assert!(h.take().contains(&"ach:grp.boat".to_string()));
    assert!(!b.bh.borrow().chasing_ride_object);
    // a train car without a proxy: the workbench choice is passed on
    let mut e = PathExtraData::new();
    e.set("craftCountOrExtraData", ExtraValue::Int(3));
    b.bh.borrow_mut().path_extra_data = Some(e);
    b.bh.borrow_mut().tapped_ride_object = Some(ride(&DYNAMIC_OBJECT_CLASS, None));
    start(it::RIDE_TRAIN_CAR);
    assert!(h.take().contains(&"wbchoice:3".to_string()));
    b.bh.borrow_mut().path_extra_data = None;
    start(it::RIDE_TRAIN_CAR);
    // a train car without any tapped object cannot be ridden
    b.bh.borrow_mut().tapped_ride_object = None;
    start(it::RIDE_TRAIN_CAR);
    assert!(b.bh.borrow().ride_object.is_none());
    // a train car that stands in for an interaction object
    let proxy = stub(7, true);
    b.bh.borrow_mut().tapped_ride_object = Some(ride(&DYNAMIC_OBJECT_CLASS, Some(proxy.clone())));
    h.set("cantuse", 1);
    start(it::RIDE_TRAIN_CAR);
    assert_eq!(b.st().interacting, 0);
    h.set("cantuse", 0);
    b.bh.borrow_mut().tapped_ride_object = Some(ride(&DYNAMIC_OBJECT_CLASS, Some(proxy)));
    h.take();
    start(it::RIDE_TRAIN_CAR);
    assert_eq!(b.st().interacting, 1);
    assert_eq!(b.st().interaction_type, it::USE_INTERACTIVE_OBJECT);
    assert!(h.take().contains(&"reached".to_string()));
    // removing boats and cars picks them up
    b.stop_interacting();
    for t in [it::REMOVE_BOAT, it::REMOVE_TRAIN_CAR] {
        b.bh.borrow_mut().tapped_ride_object = Some(ride(&DYNAMIC_OBJECT_CLASS, None));
        h.take();
        start(t);
        assert!(h.take().contains(&"pickup".to_string()));
        b.bh.borrow_mut().tapped_ride_object = None;
        start(t);
        assert!(!h.take().contains(&"pickup".to_string()));
    }
}

#[test]
fn start_using_objects_inspecting_crafting_and_fuel() {
    let (world, _d, _dw, o, h) = mk();
    let b = bh_of(&o);
    let start = |t: u32| b.start_interacting_with_tile_at_index_tile_interaction_type(idx(6, 8), t_(&world, 6, 8), t);
    // an interactive object named by the path data: refused, accepted, a workbench-choice object
    let obj = stub(1, true);
    let extra = |count: i64| {
        let mut e = PathExtraData::new();
        e.set("interactionObject", ExtraValue::Obj(obj.clone()));
        e.set("craftCountOrExtraData", ExtraValue::Int(count));
        Some(e)
    };
    b.bh.borrow_mut().path_extra_data = extra(2);
    h.set("cantuse", 1);
    start(it::USE_INTERACTIVE_OBJECT);
    assert_eq!(b.st().interacting, 0);
    h.set("cantuse", 0);
    h.set("iotype", 7);
    h.take();
    start(it::USE_INTERACTIVE_OBJECT);
    let log = h.take();
    assert!(log.contains(&"wbchoice:2".to_string()) && log.contains(&"reached".to_string()), "{log:?}");
    b.stop_interacting();
    // no object in the extra data at all
    let mut e = PathExtraData::new();
    e.set("count", ExtraValue::Int(1));
    b.bh.borrow_mut().path_extra_data = Some(e);
    start(it::USE_INTERACTIVE_OBJECT);
    assert_eq!(b.st().interacting, 0);
    b.stop_interacting();
    // inspecting a pet
    b.bh.borrow_mut().tapped_npc = Some(stub(1, true));
    start(it::INSPECT_NPC);
    assert!(h.take().contains(&"inspect".to_string()));
    b.stop_interacting();
    h.take();
    // crafting: every precondition and outcome
    let wb = stub(2, true);
    let craft = |count: i64, item: bool, wb: Option<ObjRef>| {
        let mut e = PathExtraData::new();
        if item {
            e.set("craftableItemObject", ExtraValue::Any(Rc::new(())));
        }
        if let Some(w) = wb {
            e.set("workbench", ExtraValue::Obj(w));
        }
        e.set("count", ExtraValue::Int(count));
        b.bh.borrow_mut().path_extra_data = Some(e);
    };
    craft(0, true, Some(wb.clone()));
    start(it::CRAFT);
    assert_eq!(b.st().interacting, 0);
    craft(2, true, None);
    start(it::CRAFT);
    assert_eq!(b.st().interacting, 0);
    craft(2, false, Some(wb.clone()));
    start(it::CRAFT);
    assert_eq!(b.st().interacting, 0);
    // in use by someone else
    h.set("wbuse", 1);
    craft(2, true, Some(wb.clone()));
    start(it::CRAFT);
    assert_eq!(b.st().interacting, 0);
    // in use by us, the craft is accepted
    h.set_obj("wbcur", Some(o.clone()));
    h.set("craftok", 1);
    craft(2, true, Some(wb.clone()));
    h.take();
    start(it::CRAFT);
    assert_eq!(b.st().interacting, 1);
    assert!(h.take().contains(&"craft:2".to_string()));
    b.stop_interacting();
    // the workbench refuses
    h.set("craftok", 0);
    h.set("wbuse", 0);
    craft(2, true, Some(wb.clone()));
    start(it::CRAFT);
    assert_eq!(b.st().interacting, 0);
    // adding fuel
    let mut e = PathExtraData::new();
    e.set("workbench", ExtraValue::Obj(wb.clone()));
    b.bh.borrow_mut().path_extra_data = Some(e);
    h.take();
    start(it::ADD_FUEL);
    assert!(h.take().contains(&"managefuel".to_string()));
    b.bh.borrow_mut().path_extra_data = Some(PathExtraData::new());
    start(it::ADD_FUEL);
    b.bh.borrow_mut().path_extra_data = None;
    start(it::ADD_FUEL);
    start(it::CRAFT);
    start(it::USE_INTERACTIVE_OBJECT);
    // no world: the square is left alone
    let (w2, o2, ..) = {
        let (w, _d, _dw, o, h) = mk();
        (w, o, h)
    };
    drop(w2);
    bh_of(&o2).start_interacting_with_tile_at_index_tile_interaction_type(0, None, it::DIG);
}

#[test]
fn stop_interacting_ends_pet_inspection() {
    let (_world, _d, _dw, o, h) = mk();
    let b = bh_of(&o);
    b.bh.borrow_mut().tapped_npc = Some(stub(1, true));
    h.is_npc.set(true);
    b.stop_interacting();
    assert!(h.take().contains(&"inspectstop".to_string()));
    // replicas do not tell the dynamic world
    b.base.borrow_mut().is_net = true;
    b.stop_interacting();
}

#[test]
fn bonus_multiplier_tables() {
    let (world, _d, _dw, o, _h) = mk();
    let b = bh_of(&o);
    let t = tile(&world, 6, 7);
    for (ty, contents, mult) in [
        (0x5eu32, 0u8, 2),
        (0x5f, 0, 3),
        (0x60, 0, 4),
        (0x61, 0, 5),
        (0x62, 0, 6),
        (0x5e, 0x3d, 2),
        (0x5f, 0x3d, 3),
        (0x60, 0x3d, 4),
        (0x61, 0x3d, 5),
        (0x62, 0x3d, 6),
    ] {
        t.set_type_index(1);
        t.set_contents(contents);
        let item = InventoryItem::new(ItemType(ty), 0, 0, None, None);
        let r = b.free_block_creation_count_for_tile_with_item(Some(&t), Some(&item));
        assert_eq!(r.bonus_multiplier, mult, "{ty:#x} {contents:#x}");
    }
}

// ---------------------------------------------------------------------------------------------------
// the network receiver
// ---------------------------------------------------------------------------------------------------

fn replica(world: &Rc<World>, dwref: &crate::objects::base::DynamicWorldRef) -> (ObjRef, Vec<u8>) {
    let src = new_bh(world, dwref, 0);
    bh_of(&src).set_client_id("cid");
    let data = bh_of(&src).creation_net_data_for_client_impl(None);
    let rep = Blockhead::init_with_world_dynamic_world_cache_net_data(world, dwref, None, &data).unwrap();
    bh_of(&rep).bh.borrow_mut().client_id = Some("cid".into());
    (rep, data)
}

struct Mods {
    deaths: RefCell<Vec<u64>>,
    damage: Cell<Option<f32>>,
}
impl ModHooks for Mods {
    fn player_join(&self, _p: &str) {}
    fn player_quit(&self, _p: &str, _kick: bool) {}
    fn chat(&self, _p: &str, _t: &str) -> ChatVerdict {
        ChatVerdict::Keep
    }
    fn owns_command(&self, _l: &str) -> bool {
        false
    }
    fn command(&self, _i: Option<&str>, _l: &str) -> bool {
        false
    }
    fn tick(&self, _dt: f32) {}
    fn poll(&self) {}
    fn world_loaded(&self, _n: &str, _i: &str) {}
    fn world_save(&self, _post: bool) {}
    fn server_stop(&self) {}
    fn wants(&self, _what: Optional) -> bool {
        false
    }
    fn mod_packet(&self, _p: &str, _d: &[u8]) {}
    fn pre_login(&self, _p: &str, _i: &str, _a: &str, _l: Option<&LoaderKey>) -> Option<u8> {
        None
    }
    fn limbo_join(&self, _p: &str, _i: &str, _l: Option<&LoaderKey>) -> bool {
        false
    }
    fn limbo_packet(&self, _p: &str) -> bool {
        false
    }
    fn limbo_gone(&self, _p: &str) {}
    fn block_place(&self, _p: &str, _pos: IntPair, _i: u32, _a: u16, _b: u16) -> bool {
        false
    }
    fn block_break(&self, _p: &str, _pos: IntPair, _bw: bool, _t: u32) -> bool {
        false
    }
    fn block_changed(&self, _pos: IntPair, _t: u32, _p: Option<&str>) {}
    fn object_place(&self, _p: &str, _pos: IntPair, _w: bool, _i: u32) -> bool {
        false
    }
    fn damage(&self, _p: Option<&str>, _b: u64, a: f32, _at: Option<&str>) -> Option<f32> {
        self.damage.get().map(|d| d * a)
    }
    fn death(&self, _p: Option<&str>, b: u64) {
        self.deaths.borrow_mut().push(b);
    }
    fn player_move(&self, _p: &str, _from: IntPair, _to: IntPair) {}
}

/// Installs the recording mod host for the rest of the test (and removes it on drop).
struct ModGuard;
impl Drop for ModGuard {
    fn drop(&mut self) {
        crate::mods::install(None);
    }
}
fn install_mods(damage: Option<f32>) -> (Rc<Mods>, ModGuard) {
    let m = Rc::new(Mods { deaths: RefCell::new(Vec::new()), damage: Cell::new(damage) });
    crate::mods::install(Some(m.clone()));
    (m, ModGuard)
}

#[test]
fn remote_update_server_side_effects() {
    let (world, _d, dwref, _o, h) = mk();
    *world.server.borrow_mut() = Some(Rc::new(crate::world::state::BHServer { api: None }));
    let (rep, data) = replica(&world, &dwref);
    let r = bh_of(&rep);
    let (mods, _g) = install_mods(Some(1.0));
    let mut u = BlockheadUpdateData::default();
    u.z_index = 1;
    // a non-admin player moving with z-index 1 is refreshed
    rep.remote_update(&rep, &u.to_vec());
    assert!(h.take().contains(&"updateplayer".to_string()));
    h.set("admin", 1);
    rep.remote_update(&rep, &u.to_vec());
    assert!(!h.take().contains(&"updateplayer".to_string()));
    // death edge: logged, with and without the player's name, and told to the mods
    u.regenerating = 1;
    h.set("pname", 1);
    rep.remote_update(&rep, &u.to_vec());
    assert_eq!(mods.deaths.borrow().len(), 1);
    r.bh.borrow_mut().state.regenerating = 0;
    h.set("pname", 0);
    r.bh.borrow_mut().name = Some("Bob".into());
    rep.remote_update(&rep, &u.to_vec());
    assert_eq!(mods.deaths.borrow().len(), 2);
    // creation data on a replica is applied
    rep.remote_creation_data_update(&rep, &data);
    // and ignored on the original
    let src = new_bh(&world, &dwref, 1);
    src.remote_creation_data_update(&src, &data);
}

#[test]
fn remote_update_rides_and_fishing() {
    let (world, _d, dwref, _o, h) = mk();
    let (rep, _data) = replica(&world, &dwref);
    let r = bh_of(&rep);
    let mut u = BlockheadUpdateData::default();
    // riding an object that the dynamic world knows
    let mount = ride(&DYNAMIC_OBJECT_CLASS, None);
    mount.base().borrow_mut().unique_id = 77;
    h.set_obj("ride_by_id", Some(mount.clone()));
    u.ride_object_id = 77;
    rep.remote_update(&rep, &u.to_vec());
    assert_eq!(r.st().animation_type, va::RIDE);
    // the same object again changes nothing; riding with arms down; a hand car
    rep.remote_update(&rep, &u.to_vec());
    for (class, expect) in
        [(&crate::npc::dodo::DODO_CLASS_INFO, va::RIDE_ARMS_DOWN), (&crate::trains::hand_car::HAND_CAR_CLASS, 28)]
    {
        let m2 = ride(class, None);
        m2.base().borrow_mut().unique_id = 78;
        h.set_obj("ride_by_id", Some(m2));
        u.ride_object_id = 78;
        h.take();
        rep.remote_update(&rep, &u.to_vec());
        assert_eq!(r.st().animation_type, expect);
        let log = h.take();
        assert_eq!(log.contains(&"stopriding".to_string()), expect == va::RIDE_ARMS_DOWN, "{log:?}");
        u.ride_object_id = 0;
        rep.remote_update(&rep, &u.to_vec());
        r.bh.borrow_mut().ride_object = None;
    }
    // an id nobody knows
    h.objs.borrow_mut().remove("ride_by_id");
    u.ride_object_id = 79;
    rep.remote_update(&rep, &u.to_vec());
    assert!(r.bh.borrow().ride_object.is_none());
    u.ride_object_id = 0;
    // fishing: a new rod cast (the dynamic world is the server), a fish already hooked, hooking later, giving up
    u.fishing_rod_cast_x = 300;
    u.fishing_rod_cast_y = 400;
    h.take();
    rep.remote_update(&rep, &u.to_vec());
    assert!(h.take().iter().any(|l| l.starts_with("newrod:")));
    assert!(r.bh.borrow().fishing_rod.is_some());
    assert_eq!(r.bh.borrow().fishing_rod_cast_x, 300);
    u.fishing_rod_fish_unique_id = 5;
    h.set_obj("npc_by_id", Some(stub(1, true)));
    rep.remote_update(&rep, &u.to_vec());
    assert!(h.take().contains(&"hooked".to_string()));
    assert_eq!(r.bh.borrow().remote_client_fishing_rod_fish_unique_id, 5);
    h.objs.borrow_mut().remove("npc_by_id");
    rep.remote_update(&rep, &u.to_vec());
    r.bh.borrow_mut().fishing_rod = None;
    rep.remote_update(&rep, &u.to_vec());
    // a rod that is already out and no fish: nothing to do
    u.fishing_rod_fish_unique_id = 0;
    r.bh.borrow_mut().fishing_rod = Some(Rc::new(()));
    rep.remote_update(&rep, &u.to_vec());
    // no cast any more: the rod is dropped
    r.bh.borrow_mut().fishing_rod = Some(Rc::new(()));
    u.fishing_rod_cast_x = 0;
    u.fishing_rod_cast_y = 0;
    rep.remote_update(&rep, &u.to_vec());
}

// ---------------------------------------------------------------------------------------------------
// death, damage and collapse under mods
// ---------------------------------------------------------------------------------------------------

#[test]
fn die_tells_the_mods_and_damage_obeys_them() {
    let (_world, _d, _dw, o, h) = mk();
    let b = bh_of(&o);
    let (mods, _g) = install_mods(Some(0.5));
    // damage is scaled by the mod answer
    b.suffer_damage_is_simulation_recoil(0.2, false, false);
    assert_eq!(b.st().health, 1.0 - 0.1);
    // a mod can veto
    mods.damage.set(None);
    let before = b.st().health;
    b.suffer_damage_is_simulation_recoil(0.2, false, false);
    assert_eq!(b.st().health, before);
    // asleep: damage wakes the blockhead
    mods.damage.set(Some(1.0));
    b.bh.borrow_mut().state.sub_animation_type = vsa::LIE_ON_GROUND;
    b.suffer_damage_is_simulation_recoil(0.05, false, false);
    assert_eq!(b.st().sub_animation_type, 0);
    assert!(b.bh.borrow().path_needs_recalculated);
    // the health dial in the middle leaves the damage alone
    let mut r = crate::types::structs::CustomRules::default();
    r.custom_rules_enabled = 1;
    r.health = 2;
    _world.set_custom_rules(r);
    let before = b.st().health;
    b.suffer_damage_is_simulation_recoil(0.05, false, false);
    assert!(b.st().health < before);
    // dying tells the mods once
    b.die();
    assert_eq!(mods.deaths.borrow().len(), 1);
    let _ = h;
}

#[test]
fn collapse_keeps_an_action_in_progress_in_the_queue() {
    let (_world, _d, _dw, o, h) = mk();
    let b = bh_of(&o);
    {
        let mut f = b.bh.borrow_mut();
        f.state.energy = 0.1;
        f.action_queue.push(Rc::new(()));
    }
    h.set("inprogress", 1);
    b.collapse_if_possible();
    assert_eq!(b.bh.borrow().action_queue.len(), 2);
}

// ---------------------------------------------------------------------------------------------------
// item pickup
// ---------------------------------------------------------------------------------------------------

#[test]
fn pickup_item_for_every_object_row() {
    let (world, _d, _dw, o, h) = mk();
    let b = bh_of(&o);
    let p = IntPair { x: 6, y: 9 };
    let tl = tile(&world, 6, 9);
    let reset = |fg: u8, contents: u8, bg: u8| {
        tl.set_foreground_contents(fg);
        tl.set_contents(contents);
        tl.set_background_contents(bg);
    };
    let obj = || stub(5, true);
    // torches, group 1 then group 2; the 'X' torch carries its data
    for (fg, ty) in [
        (b'1', 0x11u32),
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
        (b'3', 0x4b),
        (b'5', 0x4c),
        (b'7', 0x56),
        (b'9', 0x57),
        (b';', 0x58),
    ] {
        reset(fg, 0, 0);
        h.set_obj("torch", Some(obj()));
        h.set("torch_a", 3);
        h.set("torch_b", 4);
        let i = b.pickup_item_for_tile_ast_pos(Some(&tl), p).expect("torch");
        let i = i.borrow();
        assert_eq!(i.item_type.0, ty);
        if fg == b'X' {
            assert_eq!((i.data_a, i.data_b), (3, 4));
        }
    }
    // an unremovable torch is left
    reset(b'1', 0, 0);
    h.set_obj("torch", Some(stub(5, false)));
    assert!(b.pickup_item_for_tile_ast_pos(Some(&tl), p).is_none());
    // a torch that does not fit
    h.set_obj("torch", Some(obj()));
    h.set("nopick", 1);
    assert!(b.pickup_item_for_tile_ast_pos(Some(&tl), p).is_none());
    h.set("nopick", 0);
    // eggs
    reset(b'N', 0, 0);
    h.set_obj("egg", Some(obj()));
    h.set("egg_breed", 4);
    let i = b.pickup_item_for_tile_ast_pos(Some(&tl), p).unwrap();
    assert_eq!((i.borrow().item_type.0, i.borrow().data_b), (0x5b, 4));
    h.set("noeggpick", 1);
    assert!(b.pickup_item_for_tile_ast_pos(Some(&tl), p).is_none());
    h.set("noeggpick", 0);
    h.objs.borrow_mut().remove("egg");
    assert!(b.pickup_item_for_tile_ast_pos(Some(&tl), p).is_none());
    // paintings, columns, ladders, wires, shafts, motors, stairs, doors, windows
    h.set("objtype", 0x8f);
    for (fg, contents, bg, key, expect) in [
        (b'c', 0u8, 0u8, "painting", 0x8fu32),
        (b'd', 0, 0, "column", 0x8f),
        (b'B', 0, 0, "ladder", 0x35),
        (b'`', 0, 0, "wire", 0xb2),
        (0, b'`', 0, "wire", 0xb2),
        (0, b'h', 0, "motor", 0x440),
        (0, b'g', 0, "shaft", 0x43f),
        (0, b'e', 0, "stairs", 0x8f),
        (0, 0x92, 0, "door", 0x8f),
        (0, 0, b'E', "window", 0x3a),
        (0, 0, b'_', "window", 0xae),
    ] {
        reset(fg, contents, bg);
        h.objs.borrow_mut().clear();
        // nothing there
        assert!(b.pickup_item_for_tile_ast_pos(Some(&tl), p).is_none(), "{key} empty");
        // present, removable
        h.set_obj(key, Some(obj()));
        let i = b.pickup_item_for_tile_ast_pos(Some(&tl), p).unwrap_or_else(|| panic!("{key}"));
        assert_eq!(i.borrow().item_type.0, expect, "{key}");
        // present, not removable
        h.set_obj(key, Some(stub(5, false)));
        assert!(b.pickup_item_for_tile_ast_pos(Some(&tl), p).is_none(), "{key} locked");
        // the item does not fit
        h.set_obj(key, Some(obj()));
        h.set("nopick", 1);
        assert!(b.pickup_item_for_tile_ast_pos(Some(&tl), p).is_none(), "{key} full");
        h.set("nopick", 0);
    }
    // no tile
    assert!(b.pickup_item_for_tile_ast_pos(None, p).is_none());
}

#[test]
fn picking_up_free_blocks_windows_gates_and_achievements() {
    let (world, dw, _dwref, o, h) = mk();
    let b = bh_of(&o);
    let p = IntPair { x: 6, y: 9 };
    let tl = tile(&world, 6, 9);
    // free blocks: the host picks them up, newest first
    h.cit.set(it::PICKUP_FREEBLOCK);
    let a = stub(1, true);
    a.base().borrow_mut().unique_id = 5;
    let c = stub(1, true);
    c.base().borrow_mut().unique_id = 9;
    *h.fbs.borrow_mut() = vec![a, c];
    h.set("fbok", 1);
    h.take();
    b.pick_up_item_if_possible_in_tile_at_pos(Some(&tl), p);
    assert_eq!(h.take(), vec!["pickupfb:5".to_string(), "pickupfb:9".to_string()]);
    // more than eight successes stop making noise (nothing to observe: every block is still tried)
    *h.fbs.borrow_mut() = (0..10)
        .map(|i| {
            let f = stub(1, true);
            f.base().borrow_mut().unique_id = 100 + i;
            f
        })
        .collect();
    b.pick_up_item_if_possible_in_tile_at_pos(Some(&tl), p);
    assert_eq!(h.take().len(), 10);
    h.fbs.borrow_mut().clear();
    h.cit.set(it::PICKUP);
    // windows are picked up as objects
    tl.set_foreground_contents(0);
    tl.set_background_contents(b'E');
    h.set_obj("window", Some(stub(3, true)));
    b.pick_up_item_if_possible_in_tile_at_pos(Some(&tl), p);
    assert!(h.take().contains(&"pickup".to_string()));
    // gates: the free block of the door is made, the door removed, the contents cleared
    tl.set_background_contents(0);
    tl.set_contents(0x92);
    h.set("objtype", 0x130);
    h.set_obj("door", Some(stub(3, true)));
    b.pick_up_item_if_possible_in_tile_at_pos(Some(&tl), p);
    let log = h.take();
    assert!(log.contains(&"remove:Door:6,9".to_string()), "{log:?}");
    assert_eq!(tl.contents(), 0);
    // a gate with no door object still clears the tile
    tl.set_contents(0x92);
    h.objs.borrow_mut().remove("door");
    h.set_obj("door", Some(stub(3, true)));
    // achievements for the gem torches and a lime found as a painting
    for (fg, ach) in [
        (b'3', "grp.ruby"),
        (b'5', "grp.emerald"),
        (b'7', "grp.sapphire"),
        (b'9', "grp.amethyst"),
        (b';', "grp.diamond"),
    ] {
        tl.set_contents(0);
        tl.set_foreground_contents(fg);
        h.set_obj("torch", Some(stub(3, true)));
        h.take();
        b.pick_up_item_if_possible_in_tile_at_pos(Some(&tl), p);
        let log = h.take();
        assert!(log.contains(&format!("ach:{ach}")), "{fg}: {log:?}");
        assert!(log.contains(&"remove:Torch:6,9".to_string()));
        assert_eq!(tl.foreground_contents(), 0);
    }
    tl.set_foreground_contents(b';');
    tl.set_contents(0x7b);
    h.take();
    b.pick_up_item_if_possible_in_tile_at_pos(Some(&tl), p);
    assert!(h.take().contains(&"ach:grp.diamond_tree".to_string()));
    tl.set_contents(0);
    tl.set_foreground_contents(b'c');
    h.set("objtype", 0xa0);
    h.set_obj("painting", Some(stub(3, true)));
    h.take();
    b.pick_up_item_if_possible_in_tile_at_pos(Some(&tl), p);
    assert!(h.take().contains(&"ach:grp.lime".to_string()));
    // controlled blockheads report nothing
    tl.set_foreground_contents(b'3');
    b.bh.borrow_mut().is_client_blockhead_being_controlled_by_server = true;
    b.pick_up_item_if_possible_in_tile_at_pos(Some(&tl), p);
    assert!(h.take().iter().all(|l| !l.starts_with("ach:")));
    b.bh.borrow_mut().is_client_blockhead_being_controlled_by_server = false;
    // every removal row, with the unconfirmed-pickup registration on a client-like blockhead
    {
        let mut f = b.bh.borrow_mut();
        f.unconfirmed_pickups = Some(Default::default());
        f.this_frame_pickup_requests = Some(Default::default());
    }
    let rows: Vec<(u8, u8, &str, i64, &str)> = vec![(b'E' - b'E', 0, "egg", 0, "Egg")];
    let _ = rows;
    for (fg, contents, key, objtype, remover, clears_fg) in [
        (b'1', 0u8, "torch", 0i64, "Torch", true),
        (b'N', 0, "egg", 0, "Egg", true),
        (b'c', 0, "painting", ItemType::ITEM_PAINTING_L_S.0 as i64, "Painting", true),
        (b'B', 0, "ladder", 0, "Ladder", true),
        (b'd', 0, "column", ItemType::ITEM_STONE_COLUMN.0 as i64, "Column", true),
        (b'`', 0, "wire", 0, "Wire", true),
        (0, b'`', "wire", 0, "Wire", false),
        (0, b'e', "stairs", ItemType::ITEM_STONE_STAIRS.0 as i64, "Stairs", false),
        (0, b'g', "shaft", 0, "ElevatorShaft", false),
        (0, b'h', "motor", 0, "ElevatorMotor", false),
    ] {
        tl.set_foreground_contents(fg);
        tl.set_contents(contents);
        h.objs.borrow_mut().clear();
        h.set("objtype", objtype);
        let picked = stub(3, true);
        picked.base().borrow_mut().unique_id = 4242;
        h.set_obj(key, Some(picked.clone()));
        h.set_obj("removed", Some(picked));
        h.take();
        b.pick_up_item_if_possible_in_tile_at_pos(Some(&tl), p);
        let log = h.take();
        assert!(log.contains(&format!("remove:{remover}:6,9")), "{key}: {log:?}");
        if clears_fg {
            assert_eq!(tl.foreground_contents(), 0, "{key}");
        } else {
            assert_eq!(tl.contents(), 0, "{key}");
        }
        assert!(b.bh.borrow().unconfirmed_pickups.as_ref().unwrap().contains_key(&4242), "{key}");
        b.bh.borrow_mut().unconfirmed_pickups.as_mut().unwrap().clear();
    }
    let _ = dw;
}

#[test]
fn pickup_requests_recheck_protected_tiles() {
    let (world, _d, _dw, o, h) = mk();
    let b = bh_of(&o);
    let tl = tile(&world, 6, 9);
    h.cit.set(it::DIG);
    world.set_ops_hooks(Some(Rc::new(Protect)));
    b.pick_up_item_if_possible_in_tile_at_pos(Some(&tl), IntPair { x: 6, y: 9 });
    world.set_ops_hooks(None);
    // PICKUP with nothing to pick up
    h.cit.set(it::PICKUP);
    b.pick_up_item_if_possible_in_tile_at_pos(Some(&tl), IntPair { x: 6, y: 9 });
    b.pick_up_item_if_possible_in_tile_at_pos(None, IntPair { x: 6, y: 9 });
    // a pickup that was expected by a craft is crossed off
    tl.set_foreground_contents(b'1');
    h.set_obj("torch", Some(stub(3, true)));
    b.bh.borrow_mut().expected_craft_items = vec![0x11];
    b.pick_up_item_if_possible_in_tile_at_pos(Some(&tl), IntPair { x: 6, y: 9 });
    assert!(b.bh.borrow().expected_craft_items.is_empty());
}

struct Protect;
impl crate::world::state::WorldOpsHooks for Protect {
    fn load_physical_block_for_macro_tile_at_x_y_load_surrounding_blocks_create_if_not_created(
        &self,
        _world: &World,
        _mt: &crate::world::state::MacroTileRef,
        _mx: i32,
        _my: i32,
        _surrounding: bool,
        _create: bool,
    ) -> bool {
        false
    }
    fn load_dynamic_objects_if_not_already_loaded_for_macro_tile_include_surface_blocks(
        &self,
        _world: &World,
        _mt: &crate::world::state::MacroTileRef,
        _include: bool,
    ) {
    }
    fn tile_is_protected_at_pos_against_blockhead(&self, _w: &World, _p: IntPair, _b: &ObjRef) -> bool {
        true
    }
}

#[test]
fn simulation_stops_where_a_human_is_needed() {
    let (_world, _d, _dw, o, h) = mk();
    let b = bh_of(&o);
    b.bh.borrow_mut().action_queue.push(Rc::new(()));
    assert!(b.should_continue_simulating());
    h.set("human", 1);
    assert!(!b.should_continue_simulating());
}

#[test]
fn the_same_pickup_is_registered_once() {
    let (world, _d, _dw, o, h) = mk();
    let b = bh_of(&o);
    let tl = tile(&world, 6, 9);
    h.cit.set(it::PICKUP);
    {
        let mut f = b.bh.borrow_mut();
        f.unconfirmed_pickups = Some(Default::default());
        f.this_frame_pickup_requests = Some(Default::default());
    }
    for _ in 0..2 {
        tl.set_foreground_contents(b'B');
        h.set_obj("ladder", Some(stub(3, true)));
        b.pick_up_item_if_possible_in_tile_at_pos(Some(&tl), IntPair { x: 6, y: 9 });
    }
    assert_eq!(b.bh.borrow().unconfirmed_pickups.as_ref().unwrap().len(), 1);
}
