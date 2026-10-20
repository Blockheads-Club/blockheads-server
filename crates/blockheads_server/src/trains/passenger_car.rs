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
        if !train_car_init_with_world_dynamic_world_at_position_cache_save_dict_placed_by_client(
            &o, world, dw, pos, cache, save_dict, client_id,
        ) {
            return None;
        }
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
        if !train_car_init_with_world_dynamic_world_save_dict_cache(&o, world, dw, save_dict, cache) {
            return None;
        }
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
