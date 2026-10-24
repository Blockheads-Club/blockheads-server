//! `PassengerCar` (DynamicObject type 44, item 208): two-seat carriage.
//!
//! [`rolling_car_update_after_super`]. Client-only parts (DrawCube setup, `draw:`, body
//! matrices, camera) and the empty-bodied `reportAchievementWithIdentifier:` are not ported.

use std::cell::RefCell;
use std::rc::{Rc, Weak};

use crate::dynworld::state::DynamicWorld;
use crate::fmath::libm::{cosf, sinf};
use crate::objects::base::{DynamicObject, DynamicObjectBase};
use crate::objects::class_info::ClassInfo;
use crate::objects::handle::{self, ptr_eq, CacheRef, ObjRef};
use crate::plist::value::PlistDict;
use crate::types::structs::{IntPair, Vector2};
use crate::world::state::World;

use super::train_car::*;

/// `PassengerCar` class descriptor.
pub static PASSENGER_CAR_CLASS: ClassInfo = ClassInfo {
    name: "PassengerCar",
    superclass: Some(&crate::trains::train_car::TRAIN_CAR_CLASS),
    protocols: &["RidableDynamicObject", "TapChoiceDynamicObject"],
};

/// `PassengerCar: TrainCar` (type 44). The own ivars are client-only DrawCubes.
pub struct PassengerCar {
    base: RefCell<DynamicObjectBase>,
    tc: RefCell<TrainCarState>,
    world: Weak<World>,
    dw: Weak<DynamicWorld>,
}

impl PassengerCar {
    fn alloc(world: &Rc<World>, dw: &Rc<DynamicWorld>) -> Rc<PassengerCar> {
        let (w, d) = (Rc::downgrade(world), Rc::downgrade(dw));
        handle::alloc_obj(|sw| PassengerCar {
            base: RefCell::new(DynamicObjectBase::alloc_zeroed_with(sw)),
            tc: RefCell::new(TrainCarState::alloc_zeroed()),
            world: w,
            dw: d,
        })
    }

    /// `-[PassengerCar initWithWorld:dynamicWorld:atPosition:cache:saveDict:placedByClient:]`: TrainCar
    /// init, then the placement coupling to neighbouring cars.
    pub fn new(
        world: &Rc<World>,
        dw: &Rc<DynamicWorld>,
        pos: IntPair,
        cache: CacheRef,
        save_dict: Option<&PlistDict>,
        client_id: Option<&str>,
    ) -> Option<ObjRef> {
        let me = Self::alloc(world, dw);
        let o: ObjRef = me;
        // the TrainCar initialiser cannot fail on the placement path (the base init always succeeds)
        let inited = train_car_init_with_world_dynamic_world_at_position_cache_save_dict_placed_by_client(
            &o, world, dw, pos, cache, save_dict, client_id,
        );
        debug_assert!(inited);
        couple_rolling_car_at_placement(&o);
        Some(o)
    }

    /// `-[PassengerCar initWithWorld:dynamicWorld:saveDict:cache:]` (TrainCar only).
    pub fn from_save_dict(
        world: &Rc<World>,
        dw: &Rc<DynamicWorld>,
        save_dict: &PlistDict,
        cache: CacheRef,
    ) -> Option<ObjRef> {
        let me = Self::alloc(world, dw);
        let o: ObjRef = me;
        let inited = train_car_init_with_world_dynamic_world_save_dict_cache(&o, world, dw, save_dict, cache);
        debug_assert!(inited);
        Some(o)
    }

    /// `-[PassengerCar initWithWorld:dynamicWorld:cache:netData:]`.
    pub fn from_net_data(world: &Rc<World>, dw: &Rc<DynamicWorld>, cache: CacheRef, net_data: &[u8]) -> Option<ObjRef> {
        let me = Self::alloc(world, dw);
        let o: ObjRef = me;
        if !train_car_init_with_world_dynamic_world_cache_net_data(&o, world, dw, cache, net_data) {
            return None;
        }
        read_owner_id_tail(&o, net_data, 0x68);
        Some(o)
    }

    fn acts_as_interaction_object_impl(&self) -> Option<ObjRef> {
        None
    }
}

impl TrainCarLike for PassengerCar {
    fn tc(&self) -> &RefCell<TrainCarState> {
        &self.tc
    }
    fn tc_world(&self) -> Rc<World> {
        self.world.upgrade().expect("PassengerCar: world dropped")
    }
    fn tc_dw(&self) -> Rc<DynamicWorld> {
        self.dw.upgrade().expect("PassengerCar: dynamic world dropped")
    }
    /// : `[super loadDerivedStuff]`, then wheels at `floatPos +- (1, 0)`.
    fn load_derived_stuff(&self, me: &ObjRef) {
        train_car_load_derived_stuff(me);
        let fp = self.base.borrow().float_pos;
        let mut s = self.tc.borrow_mut();
        s.left_wheel_pos = Vector2 { x: fp.x + (-1.0f32), y: fp.y + 0.0f32 };
        s.right_wheel_pos = Vector2 { x: fp.x + 1.0f32, y: fp.y + 0.0f32 };
    }
    fn max_number_of_riders(&self) -> i32 {
        2
    }
    fn item_type(&self) -> i32 {
        ITEM_PASSENGER_CAR
    }
    fn action_title(&self) -> String {
        "PASSENGER CAR".to_string()
    }
    ///  (R1): `ox = blockhead == riders[0] ? -0.4f: 0.4f`.
    fn rider_pos_for_blockhead(&self, b: &ObjRef) -> [f32; 4] {
        let s = self.tc.borrow();
        let is_first = s.riders[0].as_ref().map_or(false, |r| ptr_eq(r, b));
        let ox = if is_first { -0.4f32 } else { 0.4f32 };
        rider_pos_common(self.base.borrow().float_pos, s.left_wheel_pos, s.right_wheel_pos, ox)
    }
    ///  (R2): rotated body, windows 1.5 / 2.0.
    fn tap_is_within_body_radius(&self, tap: Vector2) -> bool {
        let s = self.tc.borrow();
        let m = self.tc_world().world_width_macro();
        rotated_body_tap_test(self.base.borrow().float_pos, s.left_wheel_pos, s.right_wheel_pos, tap, m, 2.0f32)
    }
    /// : `[super addRider:]`; the "grp.passenger" achievement call is an empty method on the server.
    fn add_rider(&self, me: &ObjRef, rider: &ObjRef) {
        train_car_add_rider(me, rider);
    }
}

impl DynamicObject for PassengerCar {
    train_car_dynamic_object_common!();
    fn class_info(&self) -> &'static ClassInfo {
        &PASSENGER_CAR_CLASS
    }
    fn object_type(&self) -> i32 {
        DOT_TRAIN_CAR_PASSENGER
    }
    fn freeblock_creation_item_type(&self) -> i32 {
        ITEM_PASSENGER_CAR
    }
    fn free_block_creation_save_dict(&self, me: &ObjRef) -> Option<PlistDict> {
        Some(train_car_free_block_creation_save_dict(me))
    }
    ///  (= TrainCar)
    fn get_save_dict(&self, me: &ObjRef) -> PlistDict {
        train_car_get_save_dict(me)
    }
    fn creation_net_data_for_client(&self, me: &ObjRef, _client_id: Option<&str>) -> Option<Vec<u8>> {
        Some(train_car_owner_tail_creation_net_data(me))
    }
    fn update_net_data_for_client(&self, me: &ObjRef, client_id: Option<&str>) -> Option<Vec<u8>> {
        self.creation_net_data_for_client(me, client_id)
    }
    fn remote_update(&self, me: &ObjRef, net_data: &[u8]) {
        train_car_remote_update(me, net_data)
    }
    /// : `[super update:]` with the unclamped dt, then the 0.05 clamp (double compare) and steps 2-13.
    fn update_accurate_dt_is_simulation(&self, me: &ObjRef, dt: f32, accurate_dt: f32, is_simulation: bool) {
        train_car_update_accurate_dt_is_simulation(me, dt, accurate_dt, is_simulation);
        let mut dtc = dt;
        if (dt as f64) > 0.05 {
            dtc = 0.05f32;
        }
        rolling_car_update_after_super(me, dtc);
    }
    fn update_position(&self, me: &ObjRef, p: IntPair) {
        train_car_update_position(me, p)
    }
    fn set_needs_removed(&self, me: &ObjRef, v: bool) {
        train_car_set_needs_removed(me, v)
    }
    fn blockheads_loaded(&self, me: &ObjRef) {
        train_car_blockheads_loaded(me)
    }
    /// : `floatPos + R(theta) * (0, 0.75)`.
    fn render_pos(&self) -> Vector2 {
        let s = self.tc.borrow();
        let fp = self.base.borrow().float_pos;
        let theta = wheel_axis_angle(s.left_wheel_pos, s.right_wheel_pos);
        let (x, y) = glk_xy(cosf(theta), sinf(theta), 0.0, 0.75, fp.x, fp.y);
        Vector2 { x, y }
    }
}

impl_train_car_protocols!(PassengerCar);

#[cfg(test)]
mod tests {
    use super::*;

    ///  on a flat car at floatPos (10.5, 4.0) with wheels at +-1.
    #[test]
    fn tap_windows() {
        let fp = Vector2 { x: 10.5, y: 4.0 };
        let (lw, rw) = (Vector2 { x: 9.5, y: 4.0 }, Vector2 { x: 11.5, y: 4.0 });
        // inside: |dx| < 1.5, 4 < y < 6
        assert!(rotated_body_tap_test(fp, lw, rw, Vector2 { x: 10.5, y: 5.0 }, 4, 2.0));
        assert!(rotated_body_tap_test(fp, lw, rw, Vector2 { x: 11.9, y: 5.9 }, 4, 2.0));
        // outside in x, y (strict bounds), steam window 2.5 differs
        assert!(!rotated_body_tap_test(fp, lw, rw, Vector2 { x: 12.1, y: 5.0 }, 4, 2.0));
        assert!(!rotated_body_tap_test(fp, lw, rw, Vector2 { x: 10.5, y: 4.0 }, 4, 2.0));
        assert!(!rotated_body_tap_test(fp, lw, rw, Vector2 { x: 10.5, y: 6.0 }, 4, 2.0));
        assert!(!rotated_body_tap_test(fp, lw, rw, Vector2 { x: 10.5, y: 6.2 }, 4, 2.0));
        assert!(rotated_body_tap_test(fp, lw, rw, Vector2 { x: 10.5, y: 6.2 }, 4, 2.5));
        // world wrap near x = 0: car at 0.5, tap at 127.9 (W = 128)
        let fp2 = Vector2 { x: 0.5, y: 4.0 };
        let (lw2, rw2) = (Vector2 { x: -0.5, y: 4.0 }, Vector2 { x: 1.5, y: 4.0 });
        assert!(rotated_body_tap_test(fp2, lw2, rw2, Vector2 { x: 127.9, y: 5.0 }, 4, 2.0));
    }
}

#[cfg(test)]
mod world_tests {
    use super::*;
    use crate::blockhead::blockhead::testutil::{new_bh, TestHost};
    use crate::objects::base::DynamicWorldRef;
    use crate::trains::train_car::world_tests::{find_car, lay_rails, setup, setup_wide};
    use crate::world::tiles::tile_at_world_position_loaded;

    fn place(dw: &Rc<DynamicWorld>, x: i32, item: i32, ty: usize) -> ObjRef {
        dw.place_train_car_at_pos_of_type_save_dict_placed_by_client(IntPair { x, y: 10 }, item, None, None);
        let c = find_car(dw, ty as i32);
        dw.do_set(ty, c.unique_id(), c.clone());
        c
    }

    fn tick(o: &ObjRef, n: usize) {
        for _ in 0..n {
            o.update_accurate_dt_is_simulation(o, 0.016, 0.016, false);
        }
    }

    fn rider(world: &Rc<World>, dw: &Rc<DynamicWorld>) -> ObjRef {
        *world.distance_ordered_food_types.borrow_mut() = (0..31).collect();
        new_bh(world, &(dw.clone() as DynamicWorldRef), 0)
    }

    fn pass_of(o: &ObjRef) -> &PassengerCar {
        handle::downcast::<PassengerCar>(o).unwrap()
    }

    /// Constants, riders (two seats), seat positions and the tap window.
    #[test]
    fn seats_and_constants() {
        let _h = TestHost::install();
        let (world, dw) = setup("pass_seats");
        lay_rails(&world, &dw, 3, 40);
        let car = place(&dw, 20, 208, 44);
        let p = pass_of(&car);
        assert_eq!(p.max_number_of_riders(), 2);
        assert_eq!(p.item_type(), ITEM_PASSENGER_CAR);
        assert_eq!(p.action_title(), "PASSENGER CAR");
        assert_eq!(car.freeblock_creation_item_type(), ITEM_PASSENGER_CAR);
        assert_eq!(car.class_info().name, "PassengerCar");
        assert!(p.acts_as_interaction_object_impl().is_none());
        assert!(p.tap_is_within_body_radius(Vector2 { x: 20.5, y: 11.0 }));
        let (a, b) = (rider(&world, &dw), rider(&world, &dw));
        TrainCarLike::add_rider(p, &car, &a);
        TrainCarLike::add_rider(p, &car, &b);
        let (pa, pb) = (p.rider_pos_for_blockhead(&a), p.rider_pos_for_blockhead(&b));
        assert!(pa[0] < pb[0]);
        assert!(!TrainCarLike::blockhead_can_ride_using_item(p, &car, &a, 0));
        let _ = car.render_pos();
        // both seats ride along a few ticks (one of them a long frame, clamped to 50 ms), then leave
        car.update_accurate_dt_is_simulation(&car, 0.2, 0.2, false);
        tick(&car, 20);
        for r in [&a, &b] {
            r.base().borrow_mut().needs_removed = true;
        }
        tick(&car, 1);
        assert!(p.tc.borrow().riders.iter().all(|r| r.is_none()));
        assert!(car.update_needs_to_be_sent());
    }

    /// Save dict and net images round trip; the replica reads the owner.
    #[test]
    fn save_and_net_round_trips() {
        let (world, dw) = setup("pass_rt");
        lay_rails(&world, &dw, 3, 40);
        let car = place(&dw, 20, 208, 44);
        car.set_owner_id(Some("own".to_string()));
        let sd = car.get_save_dict(&car);
        assert!(car.free_block_creation_save_dict(&car).is_some());
        let again = PassengerCar::from_save_dict(&world, &dw, &sd, None).unwrap();
        assert_eq!(again.pos(), car.pos());
        let nd = car.creation_net_data_for_client(&car, None).unwrap();
        assert_eq!(nd, car.update_net_data_for_client(&car, None).unwrap());
        let rep = PassengerCar::from_net_data(&world, &dw, None, &nd).unwrap();
        assert_eq!(rep.owner_id().as_deref(), Some("own"));
        car.remote_update(&car, &nd);
        car.set_needs_removed(&car, false);
        car.blockheads_loaded(&car);
        car.update_position(&car, IntPair { x: 21, y: 10 });
        // a replica image at an unloaded tile is refused
        let (w2, d2) = setup_wide("pass_far", 4);
        lay_rails(&w2, &d2, 3, 40);
        let far = place(&d2, 20, 208, 44);
        far.base().borrow_mut().pos = IntPair { x: 70, y: 10 };
        let nd = far.creation_net_data_for_client(&far, None).unwrap();
        assert!(PassengerCar::from_net_data(&w2, &d2, None, &nd).is_none());
    }

    /// Cars are dropped when a wheel is buried; a replica never drops; the flipped axis derails.
    #[test]
    fn crash_and_derail() {
        let (world, dw) = setup("pass_crash");
        lay_rails(&world, &dw, 3, 40);
        let car = place(&dw, 20, 208, 44);
        tile_at_world_position_loaded(19, 10, &world).unwrap().set_foreground_contents(0);
        tile_at_world_position_loaded(19, 10, &world).unwrap().set_type_index(1);
        let nd = car.creation_net_data_for_client(&car, None).unwrap();
        let rep = PassengerCar::from_net_data(&world, &dw, None, &nd).unwrap();
        tick(&rep, 2);
        assert!(!rep.needs_removed());
        tick(&car, 2);
        assert!(car.needs_removed());
        tick(&car, 2);

        let (world, dw) = setup("pass_flip");
        lay_rails(&world, &dw, 3, 40);
        let car = place(&dw, 20, 208, 44);
        {
            let mut tc = pass_of(&car).tc.borrow_mut();
            let (l, r) = (tc.left_wheel_pos, tc.right_wheel_pos);
            tc.left_wheel_pos = r;
            tc.right_wheel_pos = l;
        }
        tick(&car, 2);
        assert!(car.needs_removed());
    }

    /// A car at `x` that is not adjacent to anything (no placement coupling).
    fn lone(world: &Rc<World>, dw: &Rc<DynamicWorld>, x: i32, item: i32) -> ObjRef {
        let pos = IntPair { x, y: 10 };
        let c = match item {
            205 => crate::trains::steam_train::SteamTrain::new(world, dw, pos, None, None, None),
            206 => crate::trains::freight_car::FreightCar::new(world, dw, pos, None, None, None),
            _ => PassengerCar::new(world, dw, pos, None, None, None),
        }
        .unwrap();
        dw.do_set(c.object_type() as usize, c.unique_id(), c.clone());
        c
    }

    /// `a` left of `b`, linked both ways.
    fn link(a: &ObjRef, b: &ObjRef) {
        train_car_of(a).unwrap().tc().borrow_mut().right_car = Some(b.clone());
        train_car_of(b).unwrap().tc().borrow_mut().left_car = Some(a.clone());
    }

    fn engine_of(c: &ObjRef) -> Option<ObjRef> {
        train_car_of(c).unwrap().engine_car(c)
    }

    /// A removed neighbour is dropped from the links of the cars next to it and the cars beyond lose the engine.
    #[test]
    fn removed_neighbours_are_forgotten() {
        let (world, dw) = setup("pass_consist");
        lay_rails(&world, &dw, 3, 60);
        // engine - mid - tail - last
        let eng = lone(&world, &dw, 10, 205);
        let mid = lone(&world, &dw, 20, 208);
        let tail = lone(&world, &dw, 30, 206);
        let last = lone(&world, &dw, 40, 208);
        link(&eng, &mid);
        link(&mid, &tail);
        link(&tail, &last);
        train_car_of(&mid).unwrap().set_engine_car(&mid, Some(&eng));
        assert!(engine_of(&last).is_some());
        // the middle car goes: the tail forgets its left link, and the cars behind lose the engine
        mid.base().borrow_mut().needs_removed = true;
        tick(&tail, 1);
        assert!(train_car_of(&tail).unwrap().left_car().is_none());
        assert!(engine_of(&tail).is_none() && engine_of(&last).is_none());
        assert!(tail.update_needs_to_be_sent());
    }

    #[test]
    fn a_removed_engine_next_door() {
        let (world, dw) = setup("pass_consist2");
        lay_rails(&world, &dw, 3, 60);
        let eng = lone(&world, &dw, 10, 205);
        let first = lone(&world, &dw, 20, 208);
        let second = lone(&world, &dw, 30, 208);
        link(&eng, &first);
        link(&first, &second);
        train_car_of(&first).unwrap().set_engine_car(&first, Some(&eng));
        eng.base().borrow_mut().needs_removed = true;
        tick(&first, 1);
        assert!(train_car_of(&first).unwrap().left_car().is_none());
        assert!(engine_of(&second).is_none());
    }

    /// The same on the right-hand side of a car with its engine on the right.
    #[test]
    fn removed_right_neighbours() {
        let (world, dw) = setup("pass_consist3");
        lay_rails(&world, &dw, 3, 60);
        // first - mid - tail - engine
        let first = lone(&world, &dw, 10, 208);
        let mid = lone(&world, &dw, 20, 208);
        let tail = lone(&world, &dw, 30, 206);
        let eng = lone(&world, &dw, 40, 205);
        link(&first, &mid);
        link(&mid, &tail);
        link(&tail, &eng);
        train_car_of(&mid).unwrap().set_engine_car(&mid, Some(&eng));
        assert!(pass_of(&mid).tc.borrow().engine_is_right);
        // the car beyond the removed one is dropped from the links
        tail.base().borrow_mut().needs_removed = true;
        tick(&mid, 1);
        assert!(train_car_of(&mid).unwrap().right_car().is_none());
        assert!(engine_of(&first).is_none() && engine_of(&mid).is_none());
        // the engine itself right next to the car
        let (world, dw) = setup("pass_consist3b");
        lay_rails(&world, &dw, 3, 60);
        let car = lone(&world, &dw, 20, 208);
        let eng = lone(&world, &dw, 30, 205);
        link(&car, &eng);
        train_car_of(&car).unwrap().set_engine_car(&car, Some(&eng));
        tick(&car, 30);
        eng.base().borrow_mut().needs_removed = true;
        tick(&car, 1);
        assert!(train_car_of(&car).unwrap().right_car().is_none());
        assert!(engine_of(&car).is_none());
        // a stale engine entry alone
        let (world, dw) = setup("pass_consist3c");
        lay_rails(&world, &dw, 3, 60);
        let q = lone(&world, &dw, 20, 208);
        let e = lone(&world, &dw, 30, 205);
        pass_of(&q).tc.borrow_mut().engine_car = Some(e.clone());
        e.base().borrow_mut().needs_removed = true;
        tick(&q, 1);
        assert!(pass_of(&q).tc.borrow().engine_car.is_none());
    }

    /// Replica: pending ids resolve; a car in the air falls; water slows the wheels.
    #[test]
    fn replica_links_and_falling() {
        let (world, dw) = setup("pass_replica");
        lay_rails(&world, &dw, 3, 50);
        let other = place(&dw, 40, 205, 42);
        let car = place(&dw, 20, 208, 44);
        let nd = car.creation_net_data_for_client(&car, None).unwrap();
        let rep = PassengerCar::from_net_data(&world, &dw, None, &nd).unwrap();
        {
            let mut tc = pass_of(&rep).tc.borrow_mut();
            tc.remote_right_car_id = other.unique_id();
            tc.remote_left_car_id = other.unique_id();
            tc.remote_engine_car_id = other.unique_id();
        }
        tick(&rep, 2);
        {
            let mut tc = pass_of(&rep).tc.borrow_mut();
            tc.remote_right_car_id = 9999;
            tc.remote_left_car_id = 9998;
            tc.remote_engine_car_id = 9997;
            tc.right_car = None;
            tc.left_car = None;
            tc.engine_car = None;
        }
        tick(&rep, 2);
        assert!(pass_of(&rep).tc.borrow().right_car.is_none());
        // both wheels over empty air with a full tile of water in between: the car falls slowly
        for x in 18..=23 {
            let t = tile_at_world_position_loaded(x, 10, &world).unwrap();
            t.set_foreground_contents(0);
            t.set_type_index(2);
        }
        tick(&car, 40);
        assert!(car.float_pos().y < 10.0);
    }

    /// A car crossing the seam of the world.
    #[test]
    fn folds_back_over_the_seam() {
        for (name, l, r, fx) in [("pass_fold_hi", 64.2f32, 66.2f32, 65.2f32), ("pass_fold_lo", -1.8, 0.2, -0.8)] {
            let (world, dw) = setup(name);
            lay_rails(&world, &dw, 0, 63);
            let car = place(&dw, 30, 208, 44);
            {
                let mut tc = pass_of(&car).tc.borrow_mut();
                tc.left_wheel_pos = Vector2 { x: l, y: 10.0 };
                tc.right_wheel_pos = Vector2 { x: r, y: 10.0 };
            }
            car.base().borrow_mut().float_pos = Vector2 { x: fx, y: 10.0 };
            tick(&car, 1);
            let x = car.float_pos().x;
            assert!((0.0..64.0).contains(&x) || car.needs_removed(), "x={x}");
        }
    }
}
