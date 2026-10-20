//! `HandCar` (DynamicObject type 41, item 204): the pump trolley. One rider, no fuel, never couples.
//!
//! resolutions" (R1 rider position, R2 tap test = TrainCar base, R4 plist is XML, R6 air-time tip packet);
//! Client-only parts (DrawCube setup, sounds, `draw:`, camera and body
//! matrices, achievements, `riderAnimationTimer` getter) are not ported; `riderAnimationTimer` itself is kept and
//! advanced because it is an ivar updated on the server.

use std::cell::RefCell;
use std::rc::{Rc, Weak};

use crate::dynworld::state::DynamicWorld;
use crate::fmath::cconv::f32_to_i32;
use crate::fmath::libm::{cosf, sinf};
use crate::objects::base::{DynamicObject, DynamicObjectBase};
use crate::objects::class_info::ClassInfo;
use crate::objects::handle::{self, CacheRef, ObjRef};
use crate::plist::value::PlistDict;
use crate::types::structs::{IntPair, Vector2};
use crate::world::helpers::tile_is_solid;
use crate::world::state::World;
use crate::world::tiles::tile_at_world_position_loaded;

use super::train_car::*;

/// `HandCar` class descriptor (`HandCar: TrainCar <RidableDynamicObject, TapChoiceDynamicObject>`).
pub static HAND_CAR_CLASS: ClassInfo = ClassInfo {
    name: "HandCar",
    superclass: Some(&crate::trains::train_car::TRAIN_CAR_CLASS),
    protocols: &["RidableDynamicObject", "TapChoiceDynamicObject"],
};

/// HandCar ivars 0x138..0x158 (the DrawCube/sound ivars are client-only).
pub struct HandCarState {
    /// 0x138 `externalXAcceleration`: pump force from the rider (`1.5 * target velocity x`)
    pub external_x_acceleration: f32,
    /// 0x13c `riderAnimationTimer`, initial `-pi/2`
    pub rider_animation_timer: f32,
    /// 0x148 `trackingAir`
    pub tracking_air: bool,
    /// 0x14c `airTrackStartPos`, `.cxx_construct` default (1,1) (R4)
    pub air_track_start_pos: Vector2,
}

/// `HandCar: TrainCar` (type 41).
pub struct HandCar {
    base: RefCell<DynamicObjectBase>,
    tc: RefCell<TrainCarState>,
    /// The HandCar ivars.
    pub st: RefCell<HandCarState>,
    world: Weak<World>,
    dw: Weak<DynamicWorld>,
}

impl HandCar {
    fn alloc(world: &Rc<World>, dw: &Rc<DynamicWorld>) -> Rc<HandCar> {
        let (w, d) = (Rc::downgrade(world), Rc::downgrade(dw));
        handle::alloc_obj(|sw| HandCar {
            base: RefCell::new(DynamicObjectBase::alloc_zeroed_with(sw)),
            tc: RefCell::new(TrainCarState::alloc_zeroed()),
            st: RefCell::new(HandCarState {
                external_x_acceleration: 0.0,
                rider_animation_timer: 0.0,
                tracking_air: false,
                air_track_start_pos: Vector2::cxx_new(),
            }),
            world: w,
            dw: d,
        })
    }

    /// `-[HandCar initWithWorld:dynamicWorld:atPosition:cache:saveDict:placedByClient:]` (pure forwarding
    /// to TrainCar).
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
        Some(o)
    }

    /// `-[HandCar initWithWorld:dynamicWorld:saveDict:cache:]` (pure forwarding).
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

    /// `-[HandCar initWithWorld:dynamicWorld:cache:netData:]`: after TrainCar's init, a tail longer than
    /// 0x68 bytes is `gzip(plist {ownerID})`.
    pub fn from_net_data(world: &Rc<World>, dw: &Rc<DynamicWorld>, cache: CacheRef, net_data: &[u8]) -> Option<ObjRef> {
        let me = Self::alloc(world, dw);
        let o: ObjRef = me;
        if !train_car_init_with_world_dynamic_world_cache_net_data(&o, world, dw, cache, net_data) {
            return None;
        }
        read_owner_id_tail(&o, net_data, 0x68);
        Some(o)
    }

    /// `-[HandCar update:accurateDT:isSimulation:]`, stages A-J.
    fn update(&self, me: &ObjRef, dt: f32, is_simulation: bool) {
        let world = self.tc_world();
        let dw = self.tc_dw();
        let macro_w = world.world_width_macro();

        // ---- Stage A
        let mut dt = dt;
        if (dt as f64) > 0.05 {
            dt = 0.05f32;
        }
        {
            let r = self.tc.borrow().riders[0].clone();
            if rider_needs_removed(&r) {
                if !rider_is_net(&r) {
                    me.base().borrow_mut().update_needs_to_be_sent = true;
                }
                self.tc.borrow_mut().riders[0] = None;
            }
        }
        if rider_paused(&self.tc.borrow().riders[0].clone()) {
            return;
        }
        if is_simulation {
            return;
        }

        // ---- Stage B: head collision
        {
            let rider = self.tc.borrow().riders[0].clone();
            if rider.is_some() && !rider_is_net(&rider) {
                let (lw, rw, lv, rv) = {
                    let s = self.tc.borrow();
                    (s.left_wheel_pos, s.right_wheel_pos, s.left_wheel_vel, s.right_wheel_vel)
                };
                let ext = self.st.borrow().external_x_acceleration;
                let fp = me.float_pos();
                let theta = wheel_axis_angle(lw, rw);
                let (c, s) = (cosf(theta), sinf(theta));
                // table: [0] = 0.9 (<= 0 or NaN), [1] = (double)(-0.9f) (> 0)
                let ox = if ext > 0.0 { -0.9f32 } else { 0.9f32 };
                let (px, py) = glk_xy(c, s, ox, 0.8f32, fp.x, (fp.y + 0.5f32) + 0.5f32);
                let t = tile_at_world_position_loaded(f32_to_i32(px), f32_to_i32(py), &world);
                if tile_is_solid(t) {
                    if me.needs_removed() {
                        return;
                    }
                    let avg = lv.add(rv).mul_f(0.5f32);
                    let speed = avg.length();
                    if speed > 0.5f32 {
                        if let Some(r) = &rider {
                            rider_suffer_damage(r, 0.2f32 * speed, is_simulation, true);
                        }
                    }
                    let prio = self.tc.borrow().riders[0].clone();
                    car_drop_as_free_block(me, prio.as_ref());
                    return;
                }
            }
        }

        // ---- Stage C: periodic sync
        {
            let rider = self.tc.borrow().riders[0].clone();
            if rider.is_some() && !rider_is_net(&rider) {
                let mut s = self.tc.borrow_mut();
                s.remote_update_timer += dt;
                if s.remote_update_timer > 0.5f32 {
                    drop(s);
                    me.base().borrow_mut().update_needs_to_be_sent = true;
                    self.tc.borrow_mut().remote_update_timer -= 0.5f32;
                }
            }
        }

        // ---- Stage D: wheel tile classification
        let mut left_air = false;
        let mut right_air = false;
        let mut left_water = false;
        let mut right_water = false;
        let mut left_acc = Vector2::ZERO;
        let mut right_acc = Vector2::ZERO;
        let (lw0, rw0, lv0, rv0) = {
            let s = self.tc.borrow();
            (s.left_wheel_pos, s.right_wheel_pos, s.left_wheel_vel, s.right_wheel_vel)
        };
        let mut lp = IntPair { x: f32_to_i32(lw0.x), y: f32_to_i32(lw0.y) };
        let mut rp = IntPair { x: f32_to_i32(rw0.x), y: f32_to_i32(rw0.y) };
        let lt = tile_at_world_position_loaded(lp.x, lp.y, &world);
        let rt = tile_at_world_position_loaded(rp.x, rp.y, &world);
        let avg_vel = lv0.add(rv0).mul_f(0.5f32);
        if let (Some(lt), Some(rt)) = (lt, rt) {
            for side in 0..2 {
                let (tile, tp) = if side == 0 { (lt, &mut lp) } else { (rt, &mut rp) };
                match classify_wheel_tile(&world, tile, *tp) {
                    WheelTile::Rail => {}
                    WheelTile::RailAbove => tp.y = tp.y.wrapping_add(1),
                    WheelTile::Solid => {
                        // buried wheel
                        if me.needs_removed() {
                            return;
                        }
                        let rider = self.tc.borrow().riders[0].clone();
                        let rider_net = rider_is_net(&rider);
                        let proceed = !(rider.is_some() && rider_net) && !(rider.is_none() && me.is_net());
                        if !proceed {
                            return;
                        }
                        let (lv, rv) = {
                            let s = self.tc.borrow();
                            (s.left_wheel_vel, s.right_wheel_vel)
                        };
                        let speed = lv.add(rv).mul_f(0.5f32).length();
                        if speed > 1.5f32 {
                            if let Some(r) = &rider {
                                rider_suffer_damage(r, 0.05f32 * speed, is_simulation, true);
                            }
                        }
                        self.st.borrow_mut().tracking_air = false;
                        let prio = self.tc.borrow().riders[0].clone();
                        car_drop_as_free_block(me, prio.as_ref());
                        return;
                    }
                    WheelTile::Air { water } => {
                        if side == 0 {
                            left_acc.y = -2.0f32;
                            left_air = true;
                            left_water = water;
                        } else {
                            right_acc.y = -2.0f32;
                            right_air = true;
                            right_water = water;
                        }
                    }
                }
            }

            // ---- Stage E: rail lookup
            refresh_wheel_rails(self, lp, rp);

            // ---- Stage F: rail / floor height
            let (mut lw, mut rw, mut lv, mut rv) = (lw0, rw0, lv0, rv0);
            let (lcfg, rcfg) = {
                let s = self.tc.borrow();
                (
                    s.left_wheel_rail.as_ref().map(rail_current_configuration),
                    s.right_wheel_rail.as_ref().map(rail_current_configuration),
                )
            };
            let lxf = lw.x - lp.x as f32;
            let rxf = rw.x - rp.x as f32;
            resolve_wheel_vertical(lcfg, lp, lxf, avg_vel, &mut lw, &mut lv, &mut left_acc, &mut left_air);
            resolve_wheel_vertical(rcfg, rp, rxf, avg_vel, &mut rw, &mut rv, &mut right_acc, &mut right_air);

            // ---- Stage G: drive and damping
            let ext = self.st.borrow().external_x_acceleration;
            let rider = self.tc.borrow().riders[0].clone();
            let mut x_power = (2.0f32 * ext - (lv.x + rv.x)) * 0.5f32;
            let mut roll = 2.0f32;
            if rider.is_none() {
                x_power = 0.0;
                roll = 0.0;
            } else if rider_is_net(&rider) {
                x_power = 0.0;
            }
            let mut damp = 0.2f32;
            if !left_air && !right_air {
                damp = 0.01f32;
            }
            if rider.is_none() {
                damp = 2.0f32;
            }
            if rider.is_some() && !rider_is_net(&rider) {
                let fp = me.float_pos();
                if left_air && right_air {
                    let mut st = self.st.borrow_mut();
                    if !st.tracking_air {
                        st.tracking_air = true;
                        st.air_track_start_pos = fp;
                    }
                } else {
                    let (tracking, start) = {
                        let st = self.st.borrow();
                        (st.tracking_air, st.air_track_start_pos)
                    };
                    if tracking {
                        self.st.borrow_mut().tracking_air = false;
                        let distance = fp.sub(start).length();
                        if distance > 10.0f32 {
                            world.send_and_display_air_time_tip_with_distance(distance);
                        }
                        // distance >= 30: reportAchievementWithIdentifier:@"grp.handcar" is an empty method
                    }
                }
            }

            // ---- Stage H: integration
            let scale = |x_power: f32, air: bool| -> f32 {
                let m = if air { 0.01f64 } else { 1.0f64 };
                ((x_power as f64) * m) as f32
            };
            let comb_l =
                Vector2 { x: scale(x_power, left_air), y: 0.0 }.add(left_acc.mul_vec(Vector2 { x: roll, y: 0.25f32 }));
            let comb_r = Vector2 { x: scale(x_power, right_air), y: 0.0 }
                .add(right_acc.mul_vec(Vector2 { x: roll, y: 0.25f32 }));
            let water_l = if left_water { 15.0f32 } else { 1.0f32 };
            let water_r = if right_water { 15.0f32 } else { 1.0f32 };
            lv = lv
                .mul_f((1.0f64 - (dt * damp * water_l) as f64) as f32)
                .add(comb_l.mul_f(dt).mul_f(2.0f32).div_f(water_l));
            rv = rv
                .mul_f((1.0f64 - (dt * damp * water_r) as f64) as f32)
                .add(comb_r.mul_f(dt).mul_f(2.0f32).div_f(water_r));
            lw = lw.add(lv.mul_f(dt).mul_f(8.0f32));
            rw = rw.add(rv.mul_f(dt).mul_f(8.0f32));
            let wf = macro_w.wrapping_shl(5) as f32;
            if lw.x >= 0.0 {
                if lw.x >= wf {
                    lw.x -= wf;
                    rw.x -= wf;
                }
            } else {
                lw.x += wf;
                rw.x += wf;
            }
            let old = me.float_pos();
            let fp = lw.mul_f(0.5f32).add(rw.mul_f(0.5f32));
            me.base().borrow_mut().float_pos = fp;
            if dt > 0.0 && !rider_is_net(&rider) {
                let dx = fp.x - old.x;
                let d = wrap_dx(dx, macro_w);
                let mut s = self.tc.borrow_mut();
                s.visual_velocity.x = d / dt;
                s.visual_velocity.y = (fp.y - old.y) / dt;
            }

            // ---- Stage I: re-rigidify and post
            let dir = rw.sub(lw).normal();
            lw = fp.add(dir.mul_f(-0.5f32));
            rw = fp.add(dir.mul_f(0.5f32));
            {
                let mut s = self.tc.borrow_mut();
                s.left_wheel_pos = lw;
                s.right_wheel_pos = rw;
                s.left_wheel_vel = lv;
                s.right_wheel_vel = rv;
            }
            let new_pos = IntPair { x: f32_to_i32(fp.x), y: f32_to_i32(fp.y) };
            if new_pos != me.pos() {
                me.update_position(me, new_pos);
                // visualVelocity.lengthSquared > 0.1 && !(leftAir && rightAir): railSound (client audio)
            }
            let rider = self.tc.borrow().riders[0].clone();
            if rider.is_some() && !rider_is_net(&rider) && !(left_air && right_air) && lw.x > rw.x {
                if me.needs_removed() {
                    return;
                }
                let prio = self.tc.borrow().riders[0].clone();
                car_drop_as_free_block(me, prio.as_ref());
                return;
            }
        }

        // ---- Stage J (also reached when a wheel tile is not loaded)
        let vvx = self.tc.borrow().visual_velocity.x;
        let _ = &dw;
        let mut st = self.st.borrow_mut();
        st.rider_animation_timer = ((st.external_x_acceleration + vvx) * dt) * 0.75f32 + st.rider_animation_timer;
    }
}

impl TrainCarLike for HandCar {
    fn tc(&self) -> &RefCell<TrainCarState> {
        &self.tc
    }
    fn tc_world(&self) -> Rc<World> {
        self.world.upgrade().expect("HandCar: world dropped")
    }
    fn tc_dw(&self) -> Rc<DynamicWorld> {
        self.dw.upgrade().expect("HandCar: dynamic world dropped")
    }
    /// : `riderAnimationTimer = -pi/2`, then `[super loadDerivedStuff]` (cubes/sound are client-only).
    fn load_derived_stuff(&self, me: &ObjRef) {
        self.st.borrow_mut().rider_animation_timer = -1.5707964f32;
        train_car_load_derived_stuff(me);
    }
    fn max_number_of_riders(&self) -> i32 {
        1
    }
    fn item_type(&self) -> i32 {
        ITEM_HANDCAR
    }
    fn connects_to_other_cars(&self) -> bool {
        false
    }
    ///  (R1): `ox = externalXAcceleration > 0 ? -0.4f: 0.4f`.
    fn rider_pos_for_blockhead(&self, _b: &ObjRef) -> [f32; 4] {
        let ext = self.st.borrow().external_x_acceleration;
        let ox = if ext > 0.0 { -0.4f32 } else { 0.4f32 };
        let s = self.tc.borrow();
        rider_pos_common(self.base.borrow().float_pos, s.left_wheel_pos, s.right_wheel_pos, ox)
    }
    fn set_target_velocity(&self, v: Vector2) {
        self.st.borrow_mut().external_x_acceleration = 1.5f32 * v.x;
    }
    fn remove_rider(&self, me: &ObjRef, rider: &ObjRef) {
        train_car_remove_rider(me, rider);
        let mut st = self.st.borrow_mut();
        st.tracking_air = false;
        st.external_x_acceleration = 0.0;
    }
}

impl HandCar {
    fn acts_as_interaction_object_impl(&self) -> Option<ObjRef> {
        None
    }
}

impl DynamicObject for HandCar {
    train_car_dynamic_object_common!();
    fn class_info(&self) -> &'static ClassInfo {
        &HAND_CAR_CLASS
    }
    fn object_type(&self) -> i32 {
        DOT_TRAIN_CAR_HANDCAR
    }
    fn freeblock_creation_item_type(&self) -> i32 {
        ITEM_HANDCAR
    }
    fn free_block_creation_save_dict(&self, me: &ObjRef) -> Option<PlistDict> {
        Some(train_car_free_block_creation_save_dict(me))
    }
    ///  (= TrainCar)
    fn get_save_dict(&self, me: &ObjRef) -> PlistDict {
        train_car_get_save_dict(me)
    }
    /// : `[0x68 struct][gzip(XML plist {ownerID})]`.
    fn creation_net_data_for_client(&self, me: &ObjRef, _client_id: Option<&str>) -> Option<Vec<u8>> {
        Some(train_car_owner_tail_creation_net_data(me))
    }
    fn update_net_data_for_client(&self, me: &ObjRef, client_id: Option<&str>) -> Option<Vec<u8>> {
        self.creation_net_data_for_client(me, client_id)
    }
    ///  (TrainCar parses everything)
    fn remote_update(&self, me: &ObjRef, net_data: &[u8]) {
        train_car_remote_update(me, net_data)
    }
    /// HandCar's update does NOT dispatch to super: TrainCar's link
    /// resolution / chunk force-load never runs from a HandCar update.
    fn update_accurate_dt_is_simulation(&self, me: &ObjRef, dt: f32, _accurate_dt: f32, is_simulation: bool) {
        self.update(me, dt, is_simulation)
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
    /// : `M*(0, 0.5, 0)` with `M = Translate(floatPos, -0.5) * RotateZ(theta)`.
    fn render_pos(&self) -> Vector2 {
        let s = self.tc.borrow();
        let fp = self.base.borrow().float_pos;
        let theta = wheel_axis_angle(s.left_wheel_pos, s.right_wheel_pos);
        let (x, y) = glk_xy(cosf(theta), sinf(theta), 0.0, 0.5, fp.x, fp.y);
        Vector2 { x, y }
    }
}

impl_train_car_protocols!(HandCar);

#[cfg(test)]
mod tests {
    use super::*;

    ///  stage F table (f32 evaluation order), left wheel at x fraction 0.25 on tile y 10, vx = 2.
    #[test]
    fn rail_height_table() {
        let (f, ty, vx) = (0.25f32, 10.0f32, 2.0f32);
        assert_eq!(rail_surface(1, f, ty, vx), RailSurface { height: 10.0, acc_x: None, cvy: 0.0 });
        assert_eq!(rail_surface(8, f, ty, vx).height, 10.5);
        let r2 = rail_surface(2, f, ty, vx);
        assert_eq!((r2.height, r2.acc_x, r2.cvy), (10.75, Some(1.0), -1.0));
        let r3 = rail_surface(3, f, ty, vx);
        assert_eq!((r3.height, r3.acc_x, r3.cvy), ((0.5f32 - 0.5f32 * 0.25f32) + 10.0f32, Some(0.5), -0.5));
        let r4 = rail_surface(4, f, ty, vx);
        assert_eq!((r4.height, r4.acc_x, r4.cvy), ((1.0f32 - 0.5f32 * 0.25f32) + 10.0f32, Some(0.5), -0.5));
        let r5 = rail_surface(5, f, ty, vx);
        assert_eq!((r5.height, r5.acc_x, r5.cvy), (10.25, Some(-1.0), 1.0));
        let r6 = rail_surface(6, f, ty, vx);
        assert_eq!((r6.height, r6.acc_x, r6.cvy), (10.0 + 0.125, Some(-0.5), 0.5));
        let r7 = rail_surface(7, f, ty, vx);
        assert_eq!((r7.height, r7.acc_x, r7.cvy), (10.625, Some(-0.5), 0.5));
        // unknown configurations are flat
        assert_eq!(rail_surface(9, f, ty, vx).height, 10.0);
    }

    /// Wheel snap / airborne decision incl. NaN (unordered compares take the air branch, note).
    #[test]
    fn vertical_resolution() {
        let avg = Vector2 { x: 3.0, y: 0.0 };
        // rail cfg 5 (up right): wheel below the surface snaps up and takes avgVel.x
        let (mut p, mut v, mut a, mut air) =
            (Vector2 { x: 5.25, y: 10.0 }, Vector2 { x: 2.0, y: 7.0 }, Vector2::ZERO, false);
        resolve_wheel_vertical(Some(5), IntPair { x: 5, y: 10 }, 0.25, avg, &mut p, &mut v, &mut a, &mut air);
        assert_eq!((p.y, v.x, v.y, a.x, air), (10.25, 3.0, 2.0, -1.0, false));
        // above the surface: falls with acc (0, -2) (the slope x acceleration is discarded)
        let (mut p, mut v, mut a, mut air) =
            (Vector2 { x: 5.25, y: 11.0 }, Vector2 { x: 2.0, y: 0.0 }, Vector2::ZERO, false);
        resolve_wheel_vertical(Some(5), IntPair { x: 5, y: 10 }, 0.25, avg, &mut p, &mut v, &mut a, &mut air);
        assert!(air);
        assert_eq!((a.x, a.y, p.y), (0.0, -2.0, 11.0));
        // no rail: only when not already airborne
        let (mut p, mut v, mut a, mut air) =
            (Vector2 { x: 5.25, y: 12.0 }, Vector2 { x: 2.0, y: 0.0 }, Vector2::ZERO, true);
        resolve_wheel_vertical(None, IntPair { x: 5, y: 10 }, 0.25, avg, &mut p, &mut v, &mut a, &mut air);
        assert_eq!((p.y, a.y), (12.0, 0.0));
        let (mut p, mut v, mut a, mut air) =
            (Vector2 { x: 5.25, y: f32::NAN }, Vector2 { x: 2.0, y: 0.0 }, Vector2::ZERO, false);
        resolve_wheel_vertical(None, IntPair { x: 5, y: 10 }, 0.25, avg, &mut p, &mut v, &mut a, &mut air);
        assert!(air && a.y == -2.0);
    }

    ///  R1: rider position of a flat, motionless car.
    #[test]
    fn rider_position() {
        let fp = Vector2 { x: 10.5, y: 4.0 };
        let (lw, rw) = (Vector2 { x: 10.0, y: 4.0 }, Vector2 { x: 11.0, y: 4.0 });
        let p = rider_pos_common(fp, lw, rw, 0.4f32);
        assert_eq!(p, [10.5 + 0.4f32, (4.0f32 + 0.5f32) + 0.5f32, -0.75, 1.0]);
        let q = rider_pos_common(fp, lw, rw, -0.4f32);
        assert_eq!(q[0], 10.5f32 + (-0.4f32));
    }

    ///  world wrap (W = 128 tiles).
    #[test]
    fn wrap_dx_rule() {
        assert_eq!(wrap_dx(1.0, 4), 1.0);
        assert_eq!(wrap_dx(64.0, 4), 64.0 - 128.0);
        assert_eq!(wrap_dx(-64.5, 4), -64.5 + 128.0);
        assert_eq!(wrap_dx(-64.0, 4), -64.0);
    }
}
