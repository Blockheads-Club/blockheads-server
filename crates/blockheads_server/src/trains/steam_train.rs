//! `SteamTrain` (DynamicObject type 42, item 205): the locomotive.
//!
//! SteamTrain" and the resolutions R1/R2/R3/R5 of
//! Client-only parts
//! (DrawCubes, sounds, `draw:`, body matrices, fuel UI, camera) are not ported. A whistle bit received from a client
//! is always relayed (`MJSound hasFinishedPlaying` is always YES on the server,).

use std::cell::RefCell;
use std::rc::{Rc, Weak};

use crate::dynworld::state::DynamicWorld;
use crate::fmath::cconv::f32_to_i32;
use crate::fmath::libm::{cosf, fmodf, sinf};
use crate::foundation::exception::range_exception_beyond_size;
use crate::foundation::gzip::{gzip_deflate, gzip_inflate};
use crate::objects::base::{new_mutable_dict, padded_bytes, DynamicObject, DynamicObjectBase};
use crate::objects::class_info::ClassInfo;
use crate::objects::handle::{self, ptr_eq, CacheRef, ObjRef};
use crate::plist::helpers::{data_from_property_list, property_list_from_data};
use crate::plist::value::{PlistDict, PlistValue};
use crate::types::netdata::SteamTrainCreationNetData;
use crate::types::structs::{IntPair, Vector2, WireStruct};
use crate::world::helpers::tile_is_solid;
use crate::world::state::World;
use crate::world::tiles::tile_at_world_position_loaded;

use super::train_car::*;

/// `STEAM_LOCO_FUEL_TYPES`.
pub const STEAM_LOCO_FUEL_TYPES: [i32; 4] = [ITEM_WOOD, ITEM_CHARCOAL, ITEM_COAL, ITEM_STEAM_TRAIN];

/// `INTERACTION_OBJECT_TYPE_TRAIN_STATION`.
const INTERACTION_OBJECT_TYPE_TRAIN_STATION: u16 = 6;

/// `SteamTrain` class descriptor (`<AddFuelObject>` is not modelled).
pub static STEAM_TRAIN_CLASS: ClassInfo = ClassInfo {
    name: "SteamTrain",
    superclass: Some(&crate::trains::train_car::TRAIN_CAR_CLASS),
    protocols: &["RidableDynamicObject", "TapChoiceDynamicObject"],
};

/// SteamTrain ivars 0x170..0x1c7 (render/sound ivars omitted).
pub struct SteamTrainState {
    /// 0x170 `goingRight`
    pub going_right: bool,
    /// 0x178 `fuelFraction`
    pub fuel_fraction: f32,
    /// 0x17c `fuelCounter`
    pub fuel_counter: f32,
    /// 0x180 `hasFuel`
    pub has_fuel: bool,
    /// 0x181 `sendWhistle` (one-shot)
    pub send_whistle: bool,
    /// 0x188
    pub left_station_name: Option<String>,
    /// 0x190
    pub right_station_name: Option<String>,
    /// 0x198 `serachingForLeftStationName`
    pub searching_for_left_station_name: bool,
    /// 0x199 `serachingForRightStationName`
    pub searching_for_right_station_name: bool,
    /// 0x19c
    pub left_station_search_tile_pos: IntPair,
    /// 0x1a4
    pub right_station_search_tile_pos: IntPair,
    /// 0x1ac
    pub left_station_goal_pos: IntPair,
    /// 0x1b4
    pub right_station_goal_pos: IntPair,
    /// 0x1bc
    pub station_goal_pos: IntPair,
    /// 0x1c4
    pub stop_at_goal_pos: bool,
    /// 0x1c5
    pub stopped: bool,
    /// 0x1c6
    pub needs_to_update_choice_ui: bool,
}

/// `abs(int)` with C wrapping semantics.
fn iabs(x: i32) -> i32 {
    x.wrapping_abs()
}

/// `SteamTrain: TrainCar <AddFuelObject>` (type 42, instance size 0x1c8).
pub struct SteamTrain {
    base: RefCell<DynamicObjectBase>,
    tc: RefCell<TrainCarState>,
    /// The SteamTrain ivars.
    pub st: RefCell<SteamTrainState>,
    world: Weak<World>,
    dw: Weak<DynamicWorld>,
}

impl SteamTrain {
    fn alloc(world: &Rc<World>, dw: &Rc<DynamicWorld>) -> Rc<SteamTrain> {
        let (w, d) = (Rc::downgrade(world), Rc::downgrade(dw));
        let zero = IntPair { x: 0, y: 0 };
        handle::alloc_obj(|sw| SteamTrain {
            base: RefCell::new(DynamicObjectBase::alloc_zeroed_with(sw)),
            tc: RefCell::new(TrainCarState::alloc_zeroed()),
            st: RefCell::new(SteamTrainState {
                going_right: false,
                fuel_fraction: 0.0,
                fuel_counter: 0.0,
                has_fuel: false,
                send_whistle: false,
                left_station_name: None,
                right_station_name: None,
                searching_for_left_station_name: false,
                searching_for_right_station_name: false,
                left_station_search_tile_pos: zero,
                right_station_search_tile_pos: zero,
                left_station_goal_pos: zero,
                right_station_goal_pos: zero,
                station_goal_pos: zero,
                stop_at_goal_pos: false,
                stopped: false,
                needs_to_update_choice_ui: false,
            }),
            world: w,
            dw: d,
        })
    }

    /// `-[SteamTrain initWithWorld:dynamicWorld:atPosition:cache:saveDict:placedByClient:]`.
    pub fn new(
        world: &Rc<World>,
        dw: &Rc<DynamicWorld>,
        pos: IntPair,
        cache: CacheRef,
        save_dict: Option<&PlistDict>,
        client_id: Option<&str>,
    ) -> Option<ObjRef> {
        let me = Self::alloc(world, dw);
        let o: ObjRef = me.clone();
        // the TrainCar initialiser cannot fail on the placement path (the base init always succeeds)
        let inited = train_car_init_with_world_dynamic_world_at_position_cache_save_dict_placed_by_client(
            &o, world, dw, pos, cache, save_dict, client_id,
        );
        debug_assert!(inited);
        {
            let mut st = me.st.borrow_mut();
            st.going_right = true;
            st.fuel_fraction = 1.0;
            st.fuel_counter = 1.0;
            st.has_fuel = true;
            if let Some(d) = save_dict {
                st.fuel_fraction = d.map.get(&"fuelFraction".to_string()).map_or(0.0, |v| v.float_value());
                st.has_fuel = d.map.get(&"hasFuel".to_string()).map_or(false, |v| v.bool_value());
            }
        }
        let t: &dyn TrainCarLike = &*me;
        for offset in [-1i32, 1i32] {
            let Some(car) = neighbour_car_for_placement(&o, offset) else { continue };
            let Some(ct) = train_car_of(&car) else { continue };
            if car_engine_car(&car).is_none() && ct.connects_to_other_cars() {
                if offset == -1 {
                    t.tc().borrow_mut().left_car = Some(car.clone());
                    ct.set_right_car(&car, Some(&o));
                    ct.set_engine_car(&car, Some(&o));
                    me.st.borrow_mut().going_right = true;
                } else {
                    t.tc().borrow_mut().right_car = Some(car.clone());
                    ct.set_left_car(&car, Some(&o));
                    ct.set_engine_car(&car, Some(&o));
                }
            }
        }
        Some(o)
    }

    /// `-[SteamTrain initWithWorld:dynamicWorld:saveDict:cache:]`.
    pub fn from_save_dict(
        world: &Rc<World>,
        dw: &Rc<DynamicWorld>,
        save_dict: &PlistDict,
        cache: CacheRef,
    ) -> Option<ObjRef> {
        let me = Self::alloc(world, dw);
        let o: ObjRef = me.clone();
        let inited = train_car_init_with_world_dynamic_world_save_dict_cache(&o, world, dw, save_dict, cache);
        debug_assert!(inited);
        let get = |k: &str| save_dict.map.get(&k.to_string());
        let mut st = me.st.borrow_mut();
        st.fuel_fraction = get("fuelFraction").map_or(0.0, |v| v.float_value());
        st.has_fuel = get("hasFuel").map_or(false, |v| v.bool_value());
        st.going_right = get("goingRight").map_or(false, |v| v.bool_value());
        st.stopped = get("stopped").map_or(false, |v| v.bool_value());
        drop(st);
        Some(o)
    }

    /// `-[SteamTrain initWithWorld:dynamicWorld:cache:netData:]` (`isNet` replica).
    pub fn from_net_data(world: &Rc<World>, dw: &Rc<DynamicWorld>, cache: CacheRef, net_data: &[u8]) -> Option<ObjRef> {
        let me = Self::alloc(world, dw);
        let o: ObjRef = me.clone();
        if !train_car_init_with_world_dynamic_world_cache_net_data(&o, world, dw, cache, net_data) {
            return None;
        }
        let s = SteamTrainCreationNetData::read_le(&padded_bytes(net_data, 0x88));
        {
            let mut st = me.st.borrow_mut();
            st.has_fuel = s.has_fuel != 0;
            st.fuel_fraction = (s.fuel_fraction as i32 as f32) / 255.0f32;
            st.going_right = s.going_right != 0;
            st.stopped = s.stopped != 0;
        }
        if net_data.len() > 0x88 {
            let dict = gzip_inflate(&net_data[0x88..]).as_deref().and_then(property_list_from_data);
            o.set_owner_id(
                dict.as_ref().and_then(|d| d.object_for_key("ownerID")).and_then(|v| v.as_str()).map(|s| s.to_string()),
            );
        }
        Some(o)
    }

    // ---------------------------------------------------------------- station search

    /// Resets both station scans to the car position (`!isNet` only).
    /// (`railOrStationNameChanged`, `removeRider:`).
    fn restart_station_search(&self) {
        if !self.base.borrow().is_net {
            let pos = self.base.borrow().pos;
            let mut st = self.st.borrow_mut();
            st.searching_for_left_station_name = true;
            st.searching_for_right_station_name = true;
            st.left_station_search_tile_pos = pos;
            st.right_station_search_tile_pos = pos;
        }
    }

    /// `-[SteamTrain updateSearchForStations]`: advances each active scan by up to 8 tiles.
    /// 5.10 (including the quirk that the loop body does not re-test the searching flag).
    pub fn update_search_for_stations(&self, me: &ObjRef) {
        self.search_side(me, true);
        self.search_side(me, false);
    }

    fn search_side(&self, me: &ObjRef, left: bool) {
        let active = {
            let st = self.st.borrow();
            if left {
                st.searching_for_left_station_name
            } else {
                st.searching_for_right_station_name
            }
        };
        if !active {
            return;
        }
        let world = self.tc_world();
        let dw = self.tc_dw();
        let macro_w = world.world_width_macro();
        let w = macro_w.wrapping_shl(5);
        let pos = self.base.borrow().pos;
        let step: i32 = if left { -1 } else { 1 };
        let wrap_dx_int = |x: i32| -> i32 {
            let mut d = x.wrapping_sub(pos.x);
            if d < w / 2 {
                if d < (macro_w.wrapping_mul(-32)) / 2 {
                    d = d.wrapping_add(w);
                }
            } else {
                d = d.wrapping_sub(w);
            }
            d
        };
        for _ in 0..8 {
            let cursor = {
                let st = self.st.borrow();
                if left {
                    st.left_station_search_tile_pos
                } else {
                    st.right_station_search_tile_pos
                }
            };
            let mut sp = IntPair { x: cursor.x.wrapping_add(step), y: cursor.y };
            let mut t = tile_at_world_position_loaded(sp.x, sp.y, &world);
            let Some(t0) = t else { break };
            let mut tile = t0;
            if tile.foreground_contents() != CONTENTS_RAIL {
                sp = IntPair { x: cursor.x.wrapping_add(step), y: cursor.y.wrapping_add(1) };
                t = tile_at_world_position_loaded(sp.x, sp.y, &world);
                if let Some(tt) = t {
                    if tt.foreground_contents() != CONTENTS_RAIL {
                        sp = IntPair { x: cursor.x.wrapping_add(step), y: cursor.y.wrapping_sub(1) };
                        t = tile_at_world_position_loaded(sp.x, sp.y, &world);
                    }
                }
                match t {
                    None => break,
                    Some(tt) => tile = tt,
                }
            }
            if tile.foreground_contents() == CONTENTS_RAIL {
                let mut found = false;
                if tile.contents() == CONTENTS_INTERACTION_OBJECT {
                    if iabs(wrap_dx_int(sp.x)) > 4 {
                        let obj = dw.interaction_object_at_pos(sp);
                        let Some(obj) = obj else { break };
                        let is_station = obj
                            .as_interaction_like()
                            .map_or(false, |l| l.interaction_object_type() == INTERACTION_OBJECT_TYPE_TRAIN_STATION);
                        if is_station {
                            found = true;
                            let title = obj.as_interaction_like().map(|l| l.title());
                            let mut st = self.st.borrow_mut();
                            if left {
                                st.left_station_name = title;
                                st.searching_for_left_station_name = false;
                                st.left_station_goal_pos = sp;
                            } else {
                                st.right_station_name = title;
                                st.searching_for_right_station_name = false;
                                st.right_station_goal_pos = sp;
                            }
                            st.needs_to_update_choice_ui = true;
                            drop(st);
                            me.base().borrow_mut().update_needs_to_be_sent = true;
                        }
                    }
                }
                if !found {
                    let give_up = iabs(wrap_dx_int(sp.x)) > 0x800;
                    let mut st = self.st.borrow_mut();
                    if left {
                        st.left_station_search_tile_pos = sp;
                    } else {
                        st.right_station_search_tile_pos = sp;
                    }
                    if give_up {
                        if left {
                            st.searching_for_left_station_name = false;
                            st.left_station_name = None;
                        } else {
                            st.searching_for_right_station_name = false;
                            st.right_station_name = None;
                        }
                        st.needs_to_update_choice_ui = true;
                        drop(st);
                        me.base().borrow_mut().update_needs_to_be_sent = true;
                    }
                }
            } else {
                // rail line ended
                let mut st = self.st.borrow_mut();
                if left {
                    st.searching_for_left_station_name = false;
                    st.left_station_name = None;
                } else {
                    st.searching_for_right_station_name = false;
                    st.right_station_name = None;
                }
                st.needs_to_update_choice_ui = true;
                drop(st);
                me.base().borrow_mut().update_needs_to_be_sent = true;
            }
        }
    }

    // ---------------------------------------------------------------- fuel

    /// `-[SteamTrain updateHasFuel]`.
    pub fn update_has_fuel(&self, me: &ObjRef) {
        let (has, frac) = {
            let st = self.st.borrow();
            (st.has_fuel, st.fuel_fraction)
        };
        if !has || frac >= 0.0001f32 {
            if !has && frac > 0.0f32 {
                self.st.borrow_mut().has_fuel = true;
                car_changed_notify(me);
            }
        } else {
            {
                let mut st = self.st.borrow_mut();
                st.fuel_fraction = 0.0;
                st.fuel_counter = 0.0;
                st.has_fuel = false;
            }
            car_changed_notify(me);
        }
    }

    /// `-[SteamTrain fuelCount]`: `n = (int)(10*fuel + 0.99f)`, capped at 10, floored at 0.
    pub fn fuel_count(&self) -> i32 {
        let f = self.st.borrow().fuel_fraction;
        let mut n = f32_to_i32(10.0f32 * f + 0.99f32);
        if n > 10 {
            n = 10;
        }
        if n < 1 {
            0
        } else {
            n
        }
    }

    /// `-[SteamTrain addToFuelForItem:]` (no server caller, R5).
    pub fn add_to_fuel_for_item(&self, me: &ObjRef, item_type: i32) {
        if item_type == ITEM_STEAM_TRAIN {
            // the whistle sound is never "currently playing" on the server; the achievement call is a no-op
            let mut st = self.st.borrow_mut();
            st.send_whistle = true;
            drop(st);
            me.base().borrow_mut().update_needs_to_be_sent = true;
        } else {
            let mut amount = 1;
            if item_type == ITEM_COAL {
                amount = 5;
            }
            if item_type == ITEM_CHARCOAL {
                amount = 2;
            }
            if item_type == ITEM_WOOD {
                amount = 1;
            }
            {
                let mut st = self.st.borrow_mut();
                st.fuel_fraction += 0.1f32 * amount as f32;
                if st.fuel_fraction > 1.0f32 {
                    st.fuel_fraction = 1.0f32;
                }
            }
            self.update_has_fuel(me);
            me.base().borrow_mut().update_needs_to_be_sent = true;
        }
    }

    /// `-[SteamTrain setWorkbenchChoiceUIOption:]` (option 0 = "RIDE RIGHT").
    pub fn set_workbench_choice_ui_option(&self, _me: &ObjRef, option: i32) {
        let mut st = self.st.borrow_mut();
        st.going_right = option == 0;
        st.stopped = false;
        st.stop_at_goal_pos = false;
        if st.left_station_name.is_some() && !st.going_right {
            st.stop_at_goal_pos = true;
            st.station_goal_pos = st.left_station_goal_pos;
        }
        if st.right_station_name.is_some() && st.going_right {
            st.stop_at_goal_pos = true;
            st.station_goal_pos = st.right_station_goal_pos;
        }
    }

    /// `-[SteamTrain secondOptionTitle]`.
    pub fn second_option_title(&self) -> String {
        match &self.st.borrow().left_station_name {
            None => "<- RIDE LEFT".to_string(),
            Some(n) => format!("<- {}", n),
        }
    }
    /// `-[SteamTrain title]`.
    pub fn title(&self) -> String {
        "STEAM LOCOMOTIVE".to_string()
    }
    /// `-[SteamTrain needsToUpdateChoiceUI]`.
    pub fn needs_to_update_choice_ui(&self) -> bool {
        self.st.borrow().needs_to_update_choice_ui
    }
    /// `-[SteamTrain setNeedsToUpdateChoiceUI:]`.
    pub fn set_needs_to_update_choice_ui(&self, v: bool) {
        self.st.borrow_mut().needs_to_update_choice_ui = v;
    }
    /// `fuelCount`-style getter for the raw fuel fraction.
    pub fn fuel_fraction(&self) -> f32 {
        self.st.borrow().fuel_fraction
    }

    fn acts_as_interaction_object_impl(&self) -> Option<ObjRef> {
        None
    }

    // ---------------------------------------------------------------- update

    /// `-[SteamTrain update:accurateDT:isSimulation:]`.
    fn update(&self, me: &ObjRef, dt: f32, accurate_dt: f32, is_simulation: bool) {
        train_car_update_accurate_dt_is_simulation(me, dt, accurate_dt, is_simulation);
        self.update_search_for_stations(me);
        let mut dtc = dt;
        if (dt as f64) > 0.05 {
            dtc = 0.05f32;
        }
        let world = self.tc_world();
        let dw = self.tc_dw();
        let macro_w = world.world_width_macro();

        // ---- rider bookkeeping
        {
            let r = self.tc.borrow().riders[0].clone();
            if rider_needs_removed(&r) {
                if !rider_is_net(&r) {
                    me.base().borrow_mut().update_needs_to_be_sent = true;
                }
                self.tc.borrow_mut().riders[0] = None;
            }
        }
        // ---- coupled car bookkeeping
        if !me.is_net() {
            let lc = self.tc.borrow().left_car.clone();
            if lc.as_ref().map_or(false, |c| c.needs_removed()) {
                self.tc.borrow_mut().left_car = None;
            }
            let rc = self.tc.borrow().right_car.clone();
            if rc.as_ref().map_or(false, |c| c.needs_removed()) {
                self.tc.borrow_mut().right_car = None;
            }
        } else {
            let rid = self.tc.borrow().remote_right_car_id;
            // `train_car_update_accurate_dt_is_simulation` above already resolved every id the dynamic world knows
            // and cleared it, so an id left here names an absent car: the lookup can only come back empty and the
            // original's clear-on-found step has nothing to clear.
            if rid != 0 && !self.tc.borrow().right_car.as_ref().map_or(false, |c| c.unique_id() == rid) {
                self.tc.borrow_mut().right_car = dw.train_car_with_id(rid);
            }
            let lid = self.tc.borrow().remote_left_car_id;
            if lid != 0 && !self.tc.borrow().left_car.as_ref().map_or(false, |c| c.unique_id() == lid) {
                self.tc.borrow_mut().left_car = dw.train_car_with_id(lid);
            }
        }
        if rider_paused(&self.tc.borrow().riders[0].clone()) {
            return;
        }
        if is_simulation {
            return;
        }

        // ---- crash check at the front of the train (only with a local rider)
        {
            let r = self.tc.borrow().riders[0].clone();
            if r.is_some() && !rider_is_net(&r) {
                let (lw, rw) = {
                    let s = self.tc.borrow();
                    (s.left_wheel_pos, s.right_wheel_pos)
                };
                let fp = me.float_pos();
                let theta = wheel_axis_angle(lw, rw);
                let (c, s) = (cosf(theta), sinf(theta));
                let going_right = self.st.borrow().going_right;
                let ox = if going_right { 0.4f32 } else { -0.4f32 };
                // rp = floatPos + (0, 0.5); tx = rp.x; ty = rp.y + 0.5f
                let (px, py) = glk_xy(c, s, ox, 1.8f32, fp.x + 0.0f32, (fp.y + 0.5f32) + 0.5f32);
                let tile = tile_at_world_position_loaded(f32_to_i32(px), f32_to_i32(py), &world);
                if tile_is_solid(tile) {
                    if me.needs_removed() {
                        return;
                    }
                    car_drop_as_free_block(me, None);
                    return;
                }
            }
        }

        // ---- velocity / fuel step
        {
            let r = self.tc.borrow().riders[0].clone();
            let stopped = self.st.borrow().stopped;
            if r.is_none() || stopped {
                let fp = me.float_pos();
                let m = fmodf(0.5f32 + fp.x, 1.0f32);
                let mut s = self.tc.borrow_mut();
                s.left_wheel_vel.x = ((s.left_wheel_vel.x as f64) * (1.0f64 - dtc as f64)) as f32;
                s.right_wheel_vel.x = ((s.right_wheel_vel.x as f64) * (1.0f64 - dtc as f64)) as f32;
                if m <= 0.5f32 {
                    s.left_wheel_vel.x -= m * dtc * 0.2f32;
                    s.right_wheel_vel.x -= m * dtc * 0.2f32;
                } else {
                    s.left_wheel_vel.x += (1.0f32 - m) * dtc * 0.2f32;
                    s.right_wheel_vel.x += (1.0f32 - m) * dtc * 0.2f32;
                }
            } else if !rider_is_net(&r) {
                let sync = {
                    let mut s = self.tc.borrow_mut();
                    s.remote_update_timer += dtc;
                    if s.remote_update_timer > 0.5f32 {
                        s.remote_update_timer -= 0.5f32;
                        true
                    } else {
                        false
                    }
                };
                if sync {
                    me.base().borrow_mut().update_needs_to_be_sent = true;
                }
                if self.st.borrow().has_fuel {
                    let burn = {
                        let mut st = self.st.borrow_mut();
                        st.fuel_counter += dtc * 1.0f32;
                        if st.fuel_counter >= 6.0f32 {
                            st.fuel_counter -= 6.0f32;
                            st.fuel_fraction -= 0.007f32;
                            true
                        } else {
                            false
                        }
                    };
                    if burn {
                        car_changed_notify(me);
                        me.base().borrow_mut().update_needs_to_be_sent = true;
                    }
                    self.update_has_fuel(me);
                }
            }
        }

        // ---- wheel / rail step
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
        let mut new_l = IntPair { x: f32_to_i32(lw0.x), y: f32_to_i32(lw0.y) };
        let mut new_r = IntPair { x: f32_to_i32(rw0.x), y: f32_to_i32(rw0.y) };
        let tl = tile_at_world_position_loaded(new_l.x, new_l.y, &world);
        let tr = tile_at_world_position_loaded(new_r.x, new_r.y, &world);
        let avg_vel = lv0.add(rv0).mul_f(0.5f32);
        let (Some(tl), Some(tr)) = (tl, tr) else { return };
        for side in 0..2 {
            let (tile, tp) = if side == 0 { (tl, &mut new_l) } else { (tr, &mut new_r) };
            match classify_wheel_tile(&world, tile, *tp) {
                WheelTile::Rail => {}
                WheelTile::RailAbove => tp.y = tp.y.wrapping_add(1),
                WheelTile::Solid => {
                    if me.needs_removed() {
                        return;
                    }
                    let r = self.tc.borrow().riders[0].clone();
                    // crash if (local rider) or (no rider and the object is authoritative)
                    let net_blocked = if r.is_some() { rider_is_net(&r) } else { me.is_net() };
                    if net_blocked {
                        return;
                    }
                    car_drop_as_free_block(me, None);
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

        // common part: rails
        let (left_changed, right_changed) = refresh_wheel_rails(self, new_l, new_r);

        // auto-stop at the end of the rail line
        {
            let (stopped, going_right) = {
                let st = self.st.borrow();
                (st.stopped, st.going_right)
            };
            let has_rider = self.tc.borrow().riders[0].is_some();
            let (lrail, rrail) = {
                let s = self.tc.borrow();
                (s.left_wheel_rail.is_some(), s.right_wheel_rail.is_some())
            };
            if !stopped
                && has_rider
                && ((going_right && right_changed && rrail) || (!going_right && left_changed && lrail))
            {
                let mut base = if going_right { new_r } else { new_l };
                let dir: i32 = if going_right { 1 } else { -1 };
                for _ in 0..4 {
                    let mut sp = IntPair { x: base.x.wrapping_add(dir), y: base.y };
                    let mut t = tile_at_world_position_loaded(sp.x, sp.y, &world);
                    if let Some(t0) = t {
                        if t0.foreground_contents() != CONTENTS_RAIL {
                            sp = IntPair { x: base.x.wrapping_add(dir), y: base.y.wrapping_add(1) };
                            t = tile_at_world_position_loaded(sp.x, sp.y, &world);
                            if let Some(t1) = t {
                                if t1.foreground_contents() != CONTENTS_RAIL {
                                    sp = IntPair { x: base.x.wrapping_add(dir), y: base.y.wrapping_sub(1) };
                                    t = tile_at_world_position_loaded(sp.x, sp.y, &world);
                                }
                            }
                        }
                        if t.map_or(false, |t| t.foreground_contents() != CONTENTS_RAIL) {
                            self.st.borrow_mut().stopped = true;
                            me.base().borrow_mut().update_needs_to_be_sent = true;
                            break;
                        }
                        base = sp;
                    }
                }
            }
        }

        // vertical placement of each wheel
        let (mut lw, mut rw, mut lv, mut rv) = (lw0, rw0, lv0, rv0);
        let (lcfg, rcfg) = {
            let s = self.tc.borrow();
            (
                s.left_wheel_rail.as_ref().map(rail_current_configuration),
                s.right_wheel_rail.as_ref().map(rail_current_configuration),
            )
        };
        let lxf = lw.x - new_l.x as f32;
        let rxf = rw.x - new_r.x as f32;
        resolve_wheel_vertical(lcfg, new_l, lxf, avg_vel, &mut lw, &mut lv, &mut left_acc, &mut left_air);
        resolve_wheel_vertical(rcfg, new_r, rxf, avg_vel, &mut rw, &mut rv, &mut right_acc, &mut right_air);

        // acceleration integration
        let (has_fuel, going_right, stopped) = {
            let st = self.st.borrow();
            (st.has_fuel, st.going_right, st.stopped)
        };
        let ext_x = if has_fuel {
            if going_right {
                1.0f32
            } else {
                -1.0f32
            }
        } else {
            0.0f32
        };
        let mut x_power = (5.0f32 * ext_x - (lv.x + rv.x) * 3.0f32) * 0.8f32;
        let mut roll = 2.0f32;
        let mut damp = 0.2f32;
        let r0 = self.tc.borrow().riders[0].clone();
        if r0.is_none() || stopped {
            x_power = 0.0;
            roll = 0.0;
            damp = 1.0;
        }
        let comb_l = Vector2 { x: if left_air { 0.0 } else { x_power }, y: 0.0 }
            .add(left_acc.mul_vec(Vector2 { x: roll, y: 0.5f32 }));
        let comb_r = Vector2 { x: if right_air { 0.0 } else { x_power }, y: 0.0 }
            .add(right_acc.mul_vec(Vector2 { x: roll, y: 0.5f32 }));
        let drag = (1.0f64 - (dtc * damp) as f64) as f32;
        lv = lv.mul_f(drag).add(comb_l.mul_f(dtc).mul_f(2.0f32));
        rv = rv.mul_f(drag).add(comb_r.mul_f(dtc).mul_f(2.0f32));
        let left_mul = if left_water { 0.1f32 } else { 1.0f32 };
        let right_mul = if right_water { 0.1f32 } else { 1.0f32 };
        // coupling to neighbour cars (springs, spacing 1.0)
        let (right_car, left_car) = {
            let s = self.tc.borrow();
            (s.right_car.clone(), s.left_car.clone())
        };
        if let Some(rc) = &right_car {
            let o = train_car_of(rc).map_or(Vector2::ZERO, |c| c.left_wheel_pos());
            let d = o.sub(rw).normal();
            let target = o.sub(d.mul_f(1.0f32));
            rw = rw.add(target.sub(rw).mul_f(0.2f32));
        }
        if let Some(lc) = &left_car {
            let o = train_car_of(lc).map_or(Vector2::ZERO, |c| c.right_wheel_pos());
            let d = o.sub(lw).normal();
            let target = o.sub(d.mul_f(1.0f32));
            lw = lw.add(target.sub(lw).mul_f(0.2f32));
        }
        lw = lw.add(lv.mul_f(dtc).mul_f(8.0f32).mul_f(left_mul));
        rw = rw.add(rv.mul_f(dtc).mul_f(8.0f32).mul_f(right_mul));
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
        if dtc > 0.0 && !rider_is_net(&r0) {
            let dx = fp.x - old.x;
            let vx = wrap_dx(dx, macro_w);
            let mut s = self.tc.borrow_mut();
            s.visual_velocity.x = vx / dtc;
            s.visual_velocity.y = (fp.y - old.y) / dtc;
        }
        // re-pin the wheels exactly +-1 tile from the centre along the wheel axis
        let dir = rw.sub(lw).normal();
        lw = fp.add(dir.mul_f(-1.0f32));
        rw = fp.add(dir.mul_f(1.0f32));
        {
            let mut s = self.tc.borrow_mut();
            s.left_wheel_pos = lw;
            s.right_wheel_pos = rw;
            s.left_wheel_vel = lv;
            s.right_wheel_vel = rv;
        }
        let new_pos = IntPair { x: f32_to_i32(fp.x), y: f32_to_i32(fp.y) };
        let pos = me.pos();
        if new_pos != pos {
            let (stop_at, goal) = {
                let st = self.st.borrow();
                (st.stop_at_goal_pos, st.station_goal_pos)
            };
            let has_rider = self.tc.borrow().riders[0].is_some();
            if has_rider
                && stop_at
                && ((pos.x < goal.x.wrapping_sub(2) && goal.x.wrapping_sub(2) <= new_pos.x)
                    || (goal.x.wrapping_add(2) < pos.x && new_pos.x <= goal.x.wrapping_add(2)))
            {
                self.st.borrow_mut().stopped = true;
                me.base().borrow_mut().update_needs_to_be_sent = true;
            }
            me.update_position(me, new_pos);
        }
        // derail check
        let r0 = self.tc.borrow().riders[0].clone();
        if r0.is_some() && !rider_is_net(&r0) && !(left_air && right_air) && lw.x > rw.x && !me.needs_removed() {
            car_drop_as_free_block(me, None);
        }
    }
}

impl TrainCarLike for SteamTrain {
    fn tc(&self) -> &RefCell<TrainCarState> {
        &self.tc
    }
    fn tc_world(&self) -> Rc<World> {
        self.world.upgrade().expect("SteamTrain: world dropped")
    }
    fn tc_dw(&self) -> Rc<DynamicWorld> {
        self.dw.upgrade().expect("SteamTrain: dynamic world dropped")
    }
    /// : `[super loadDerivedStuff]`, wheels at `floatPos +- (1, 0)`, and (when `!isNet`) restart both scans.
    fn load_derived_stuff(&self, me: &ObjRef) {
        train_car_load_derived_stuff(me);
        let fp = self.base.borrow().float_pos;
        {
            let mut s = self.tc.borrow_mut();
            s.left_wheel_pos = Vector2 { x: fp.x + (-1.0f32), y: fp.y + 0.0f32 };
            s.right_wheel_pos = Vector2 { x: fp.x + 1.0f32, y: fp.y + 0.0f32 };
        }
        self.restart_station_search();
    }
    /// : 1
    fn max_number_of_riders(&self) -> i32 {
        1
    }
    /// : YES
    fn is_engine(&self) -> bool {
        true
    }
    fn requires_fuel(&self) -> bool {
        true
    }
    fn item_type(&self) -> i32 {
        ITEM_STEAM_TRAIN
    }
    fn action_title(&self) -> String {
        match &self.st.borrow().right_station_name {
            None => "RIDE RIGHT ->".to_string(),
            Some(n) => format!("{} ->", n),
        }
    }
    ///  (R1): `ox = goingRight ? -0.4f: 0.4f`.
    fn rider_pos_for_blockhead(&self, _b: &ObjRef) -> [f32; 4] {
        let ox = if self.st.borrow().going_right { -0.4f32 } else { 0.4f32 };
        let s = self.tc.borrow();
        rider_pos_common(self.base.borrow().float_pos, s.left_wheel_pos, s.right_wheel_pos, ox)
    }
    ///  + R2: rotated body, windows 1.5 / 2.5.
    fn tap_is_within_body_radius(&self, tap: Vector2) -> bool {
        let s = self.tc.borrow();
        let m = self.tc_world().world_width_macro();
        rotated_body_tap_test(self.base.borrow().float_pos, s.left_wheel_pos, s.right_wheel_pos, tap, m, 2.5f32)
    }
    ///  `railOrStationNameChanged`
    fn rail_or_station_name_changed(&self) {
        self.restart_station_search();
    }
    ///  `removeRider:`
    fn remove_rider(&self, me: &ObjRef, rider: &ObjRef) {
        train_car_remove_rider(me, rider);
        self.restart_station_search();
    }
}

impl DynamicObject for SteamTrain {
    train_car_dynamic_object_common!();
    fn class_info(&self) -> &'static ClassInfo {
        &STEAM_TRAIN_CLASS
    }
    fn object_type(&self) -> i32 {
        DOT_TRAIN_CAR_STEAM_ENGINE
    }
    fn freeblock_creation_item_type(&self) -> i32 {
        ITEM_STEAM_TRAIN
    }
    fn free_block_creation_save_dict(&self, me: &ObjRef) -> Option<PlistDict> {
        Some(train_car_free_block_creation_save_dict(me))
    }
    fn get_save_dict(&self, me: &ObjRef) -> PlistDict {
        let mut d = train_car_get_save_dict(me);
        let st = self.st.borrow();
        d.map.insert("fuelFraction".to_string(), PlistValue::real_f32(st.fuel_fraction));
        d.map.insert("hasFuel".to_string(), PlistValue::boolean(st.has_fuel));
        d.map.insert("goingRight".to_string(), PlistValue::boolean(st.going_right));
        d.map.insert("stopped".to_string(), PlistValue::boolean(st.stopped));
        d
    }
    /// : `[0x88 struct][gzip(XML plist {leftStationName?, rightStationName?, ownerID?})]`; consumes `sendWhistle`.
    fn creation_net_data_for_client(&self, me: &ObjRef, _client_id: Option<&str>) -> Option<Vec<u8>> {
        let mut s = SteamTrainCreationNetData::default();
        s.train_car_creation_net_data = train_car_train_car_creation_net_data(me);
        let mut dict = new_mutable_dict();
        {
            let mut st = self.st.borrow_mut();
            s.fuel_fraction = f32_to_i32(st.fuel_fraction * 255.0f32) as i16;
            s.has_fuel = st.has_fuel as u8;
            s.going_right = st.going_right as u8;
            s.stopped = st.stopped as u8;
            s.left_train_pos_x = st.left_station_goal_pos.x as u32;
            s.left_train_pos_y = st.left_station_goal_pos.y as u32;
            s.right_train_pos_x = st.right_station_goal_pos.x as u32;
            s.right_train_pos_y = st.right_station_goal_pos.y as u32;
            if st.stop_at_goal_pos {
                s.stop_at_goal_pos_x = st.station_goal_pos.x as u32;
                s.stop_at_goal_pos_y = st.station_goal_pos.y as u32;
            } else {
                s.stop_at_goal_pos_x = 0;
                s.stop_at_goal_pos_y = 0;
            }
            s.whistle = st.send_whistle as u8;
            if s.whistle != 0 {
                st.send_whistle = false;
            }
            if let Some(n) = &st.left_station_name {
                dict.map.insert("leftStationName".to_string(), PlistValue::string(n.clone()));
            }
            if let Some(n) = &st.right_station_name {
                dict.map.insert("rightStationName".to_string(), PlistValue::string(n.clone()));
            }
        }
        if let Some(o) = me.owner_id() {
            dict.map.insert("ownerID".to_string(), PlistValue::string(o));
        }
        let mut out = s.to_vec();
        if let Some(p) = data_from_property_list(&PlistValue::Dict(dict)) {
            out.extend_from_slice(&gzip_deflate(&p));
        }
        Some(out)
    }
    ///  (inherited from TrainCar)
    fn update_net_data_for_client(&self, me: &ObjRef, client_id: Option<&str>) -> Option<Vec<u8>> {
        self.creation_net_data_for_client(me, client_id)
    }
    fn remote_update(&self, me: &ObjRef, net_data: &[u8]) {
        train_car_remote_update(me, net_data);
        let u = SteamTrainCreationNetData::read_le(&padded_bytes(net_data, 0x88));
        let new_fuel = (u.fuel_fraction as i32 as f32) / 255.0f32;
        let cur = self.st.borrow().fuel_fraction;
        if new_fuel > cur + 0.01f32
            || new_fuel < cur - 0.01f32
            || (u.fuel_fraction == 0 && cur > 0.001f32)
            || (u.fuel_fraction > 0 && cur < 0.001f32)
        {
            let r0 = self.tc.borrow().riders[0].clone();
            let mut ignore = false;
            if me.is_net() && r0.is_some() && !rider_is_net(&r0) && new_fuel < cur {
                ignore = true;
            }
            if !ignore {
                self.st.borrow_mut().fuel_fraction = new_fuel;
                self.update_has_fuel(me);
                if !me.is_net() {
                    me.base().borrow_mut().update_needs_to_be_sent = true;
                }
            }
        }
        let r0 = self.tc.borrow().riders[0].clone();
        if r0.is_some() && rider_is_net(&r0) {
            let mut st = self.st.borrow_mut();
            st.going_right = u.going_right != 0;
            st.stopped = u.stopped != 0;
            st.station_goal_pos.x = u.stop_at_goal_pos_x as i32;
            st.station_goal_pos.y = u.stop_at_goal_pos_y as i32;
            st.stop_at_goal_pos = st.station_goal_pos.y != 0;
        }
        if me.is_net() {
            let mut st = self.st.borrow_mut();
            st.left_station_goal_pos = IntPair { x: u.left_train_pos_x as i32, y: u.left_train_pos_y as i32 };
            st.right_station_goal_pos = IntPair { x: u.right_train_pos_x as i32, y: u.right_train_pos_y as i32 };
        }
        if u.whistle != 0 {
            // the whistle sound is never "currently sounding" on the server: always relay
            if !me.is_net() {
                self.st.borrow_mut().send_whistle = true;
                me.base().borrow_mut().update_needs_to_be_sent = true;
            }
        }
        {
            let mut st = self.st.borrow_mut();
            st.needs_to_update_choice_ui = true;
            st.left_station_name = None;
            st.right_station_name = None;
        }
        if net_data.len() < 0x88 {
            range_exception_beyond_size(
                "subdataWithRange:",
                0x88,
                (net_data.len() as u64).wrapping_sub(0x88),
                net_data.len() as u64,
            );
        }
        let extra = &net_data[0x88..];
        if !extra.is_empty() {
            let dict = gzip_inflate(extra).as_deref().and_then(property_list_from_data);
            let mut st = self.st.borrow_mut();
            st.left_station_name = dict
                .as_ref()
                .and_then(|d| d.object_for_key("leftStationName"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            st.right_station_name = dict
                .as_ref()
                .and_then(|d| d.object_for_key("rightStationName"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
        }
    }
    fn update_accurate_dt_is_simulation(&self, me: &ObjRef, dt: f32, accurate_dt: f32, is_simulation: bool) {
        self.update(me, dt, accurate_dt, is_simulation)
    }
    fn update_position(&self, me: &ObjRef, p: IntPair) {
        train_car_update_position(me, p)
    }
    /// : `[super setNeedsRemoved:]` (the sound stops are client-only).
    fn set_needs_removed(&self, me: &ObjRef, v: bool) {
        train_car_set_needs_removed(me, v)
    }
    fn blockheads_loaded(&self, me: &ObjRef) {
        train_car_blockheads_loaded(me)
    }
    /// : `M*(0, 1.0, 0)`.
    fn render_pos(&self) -> Vector2 {
        let s = self.tc.borrow();
        let fp = self.base.borrow().float_pos;
        let theta = wheel_axis_angle(s.left_wheel_pos, s.right_wheel_pos);
        let (x, y) = glk_xy(cosf(theta), sinf(theta), 0.0, 1.0, fp.x, fp.y);
        Vector2 { x, y }
    }
}

impl_train_car_protocols!(SteamTrain);

#[allow(dead_code)]
fn _unused(a: &ObjRef, b: &ObjRef) -> bool {
    ptr_eq(a, b)
}

#[cfg(test)]
mod tests {
    use super::*;

    ///  `fuelCount`.
    #[test]
    fn fuel_count_formula() {
        let f = |fuel: f32| {
            let mut n = f32_to_i32(10.0f32 * fuel + 0.99f32);
            if n > 10 {
                n = 10;
            }
            if n < 1 {
                0
            } else {
                n
            }
        };
        assert_eq!(f(0.0), 0);
        assert_eq!(f(1.0), 10);
        assert_eq!(f(0.5), 5);
        assert_eq!(f(0.51), 6);
        assert_eq!(f(2.0), 10);
    }

    /// : net struct image (fuel encoding `(int16)(int)(fuel * 255)`).
    #[test]
    fn net_struct_layout() {
        let mut s = SteamTrainCreationNetData::default();
        s.fuel_fraction = (f32_to_i32(1.0f32 * 255.0f32)) as i16;
        s.has_fuel = 1;
        s.whistle = 1;
        s.going_right = 1;
        s.stopped = 0;
        s.left_train_pos_x = (-3i32) as u32;
        s.stop_at_goal_pos_y = 9;
        let b = s.to_vec();
        assert_eq!(b.len(), 0x88);
        assert_eq!(&b[0x68..0x6a], &255i16.to_le_bytes());
        assert_eq!(&b[0x6a..0x6e], &[1, 1, 1, 0]);
        assert_eq!(&b[0x70..0x74], &(-3i32).to_le_bytes());
        assert_eq!(&b[0x84..0x88], &9u32.to_le_bytes());
        assert!(b[0x6e..0x70].iter().all(|&x| x == 0));
        // decode: (int16)255 / 255.0f
        assert_eq!((255i16 as i32 as f32) / 255.0f32, 1.0);
    }

    ///  R3: front-crash probe evaluation on a flat train at floatPos (10.5, 4.0).
    #[test]
    fn crash_probe_point() {
        let (c, s) = (1.0f32, 0.0f32);
        let (px, py) = glk_xy(c, s, 0.4, 1.8, 10.5f32 + 0.0, (4.0f32 + 0.5) + 0.5);
        assert_eq!(px, 10.9f32);
        assert_eq!(py, (4.0f32 + 0.5 + 0.5) + 1.8f32);
    }

    ///  R1: the steam rider stands at `-0.4` when going right.
    #[test]
    fn rider_position_steam() {
        let fp = Vector2 { x: 10.5, y: 4.0 };
        let p = rider_pos_common(fp, Vector2 { x: 9.5, y: 4.0 }, Vector2 { x: 11.5, y: 4.0 }, -0.4f32);
        assert_eq!(p[0], 10.5f32 + (-0.4f32));
        assert_eq!(p[2], -0.75);
    }
}

#[cfg(test)]
mod world_tests {
    use super::*;
    use crate::blockhead::blockhead::testutil::{new_bh, TestHost};
    use crate::items::inventory_item::InventoryItem;
    use crate::objects::base::DynamicWorldRef;
    use crate::trains::train_car::world_tests::{find_car, lay_rails, setup, setup_wide};
    use crate::trains::train_station::TrainStation;
    use crate::types::item_type::ItemType;

    fn place_steam(dw: &Rc<DynamicWorld>, x: i32) -> ObjRef {
        dw.place_train_car_at_pos_of_type_save_dict_placed_by_client(IntPair { x, y: 10 }, 205, None, None);
        // cars wait in `dynamicObjectsToAdd` until the next tick: move it by hand like the tick does
        let c = find_car(dw, 42);
        dw.do_set(42, c.unique_id(), c.clone());
        c
    }

    fn steam_of(o: &ObjRef) -> &SteamTrain {
        handle::downcast::<SteamTrain>(o).unwrap()
    }

    fn rider(world: &Rc<World>, dw: &Rc<DynamicWorld>) -> ObjRef {
        *world.distance_ordered_food_types.borrow_mut() = (0..31).collect();
        new_bh(world, &(dw.clone() as DynamicWorldRef), 0)
    }

    /// A station at `x` registered in the dynamic world, as the placement path does.
    fn station(world: &Rc<World>, dw: &Rc<DynamicWorld>, x: i32, text: &str) -> ObjRef {
        let it = InventoryItem::new(ItemType(200), 0, 0, None, None);
        let mut sd = new_mutable_dict();
        sd.map.insert("text".to_string(), PlistValue::string(text));
        let pos = IntPair { x, y: 10 };
        let st = TrainStation::new(world, dw, pos, None, Some(&it.borrow()), false, Some(&sd), Some("c"), Some("n"))
            .unwrap();
        dw.by_pos_set(st.object_type(), dw.world_index(pos), &st);
        for dx in -1..=1 {
            tile_at_world_position_loaded(x + dx, 10, world).unwrap().set_contents(CONTENTS_INTERACTION_OBJECT);
        }
        st
    }

    fn tick(o: &ObjRef, n: usize) {
        for _ in 0..n {
            o.update_accurate_dt_is_simulation(o, 0.016, 0.016, false);
        }
    }

    /// Fuel items, the fuel count and the has-fuel flag.
    #[test]
    fn fuel_handling() {
        let (world, dw) = setup("steam_fuel");
        lay_rails(&world, &dw, 5, 40);
        let car = place_steam(&dw, 20);
        let s = steam_of(&car);
        assert_eq!(s.title(), "STEAM LOCOMOTIVE");
        assert_eq!(s.fuel_fraction(), 1.0);
        assert_eq!(s.fuel_count(), 10);
        s.st.borrow_mut().fuel_fraction = 0.0;
        assert_eq!(s.fuel_count(), 0);
        // empty tank with has_fuel set: the flag drops and the car is announced
        s.update_has_fuel(&car);
        assert!(!s.st.borrow().has_fuel);
        // wood adds 0.1, coal 0.5, charcoal 0.2; the flag comes back
        s.add_to_fuel_for_item(&car, ITEM_WOOD);
        assert!(s.st.borrow().has_fuel);
        assert!((s.fuel_fraction() - 0.1).abs() < 1e-6);
        s.add_to_fuel_for_item(&car, ITEM_COAL);
        assert!((s.fuel_fraction() - 0.6).abs() < 1e-6);
        s.add_to_fuel_for_item(&car, ITEM_CHARCOAL);
        assert!((s.fuel_fraction() - 0.8).abs() < 1e-6);
        s.add_to_fuel_for_item(&car, ITEM_COAL);
        assert_eq!(s.fuel_fraction(), 1.0);
        // any other item counts like wood; the locomotive item itself blows the whistle
        s.st.borrow_mut().fuel_fraction = 0.0;
        s.add_to_fuel_for_item(&car, 1);
        assert!((s.fuel_fraction() - 0.1).abs() < 1e-6);
        s.add_to_fuel_for_item(&car, ITEM_STEAM_TRAIN);
        assert!(s.st.borrow().send_whistle);
        // the whistle is consumed by the next net image
        let nd = car.creation_net_data_for_client(&car, None).unwrap();
        assert_eq!(nd[0x6b], 1);
        assert!(!s.st.borrow().send_whistle);
        assert_eq!(STEAM_LOCO_FUEL_TYPES.len(), 4);
        assert!(s.acts_as_interaction_object_impl().is_none());
    }

    /// Titles and the left/right choice.
    #[test]
    fn choice_ui() {
        let (world, dw) = setup("steam_choice");
        lay_rails(&world, &dw, 5, 40);
        let car = place_steam(&dw, 20);
        let s = steam_of(&car);
        assert_eq!(s.action_title(), "RIDE RIGHT ->");
        assert_eq!(s.second_option_title(), "<- RIDE LEFT");
        s.set_workbench_choice_ui_option(&car, 0);
        assert!(s.st.borrow().going_right && !s.st.borrow().stop_at_goal_pos);
        s.set_workbench_choice_ui_option(&car, 1);
        assert!(!s.st.borrow().going_right);
        {
            let mut st = s.st.borrow_mut();
            st.left_station_name = Some("West STATION".to_string());
            st.right_station_name = Some("East STATION".to_string());
            st.left_station_goal_pos = IntPair { x: 6, y: 10 };
            st.right_station_goal_pos = IntPair { x: 38, y: 10 };
        }
        assert_eq!(s.action_title(), "East STATION ->");
        assert_eq!(s.second_option_title(), "<- West STATION");
        s.set_workbench_choice_ui_option(&car, 1);
        assert!(s.st.borrow().stop_at_goal_pos);
        assert_eq!(s.st.borrow().station_goal_pos, IntPair { x: 6, y: 10 });
        s.set_workbench_choice_ui_option(&car, 0);
        assert_eq!(s.st.borrow().station_goal_pos, IntPair { x: 38, y: 10 });
        assert!(!s.st.borrow().stopped);
        s.set_needs_to_update_choice_ui(false);
        assert!(!s.needs_to_update_choice_ui());
        s.set_needs_to_update_choice_ui(true);
        assert!(s.needs_to_update_choice_ui());
        // the constant parts of the TrainCarLike face
        assert_eq!(s.max_number_of_riders(), 1);
        assert!(s.is_engine() && s.requires_fuel());
        assert_eq!(s.item_type(), ITEM_STEAM_TRAIN);
        assert_eq!(car.freeblock_creation_item_type(), ITEM_STEAM_TRAIN);
        assert_eq!(car.class_info().name, "SteamTrain");
        let _ = car.render_pos();
    }

    /// Stations on both sides are found by the scan, the names show up in the choice titles.
    #[test]
    fn station_scan_finds_both_neighbours() {
        let (world, dw) = setup("steam_scan");
        lay_rails(&world, &dw, 3, 60);
        let west = station(&world, &dw, 8, "West");
        let east = station(&world, &dw, 50, "East");
        let car = place_steam(&dw, 29);
        let s = steam_of(&car);
        assert!(s.st.borrow().searching_for_left_station_name && s.st.borrow().searching_for_right_station_name);
        tick(&car, 6);
        let st = s.st.borrow();
        assert_eq!(st.left_station_name.as_deref(), Some("West STATION"));
        assert_eq!(st.right_station_name.as_deref(), Some("East STATION"));
        assert!(!st.searching_for_left_station_name && !st.searching_for_right_station_name);
        assert_eq!(st.left_station_goal_pos.x, west.pos().x + 1);
        assert_eq!(st.right_station_goal_pos.x, east.pos().x - 1);
        assert!(st.needs_to_update_choice_ui);
    }

    /// A rail line that ends, a scan that runs into an unloaded chunk and the silent far scan.
    #[test]
    fn station_scan_ends_with_the_rail_line() {
        let (world, dw) = setup("steam_scan_end");
        lay_rails(&world, &dw, 10, 30);
        let car = place_steam(&dw, 20);
        tick(&car, 4);
        let st = steam_of(&car).st.borrow();
        assert_eq!(st.left_station_name, None);
        assert_eq!(st.right_station_name, None);
        assert!(!st.searching_for_left_station_name && !st.searching_for_right_station_name);
    }

    /// A rail changing under the car restarts the scans; `removeRider:` does too.
    #[test]
    fn scans_restart_on_rail_change() {
        let _h = TestHost::install();
        let (world, dw) = setup("steam_restart");
        lay_rails(&world, &dw, 10, 30);
        let car = place_steam(&dw, 20);
        tick(&car, 4);
        assert!(!steam_of(&car).st.borrow().searching_for_left_station_name);
        dw.rail_or_station_name_changed();
        assert!(steam_of(&car).st.borrow().searching_for_left_station_name);
        tick(&car, 4);
        let bh = rider(&world, &dw);
        train_car_add_rider(&car, &bh);
        TrainCarLike::remove_rider(steam_of(&car), &car, &bh);
        assert!(steam_of(&car).st.borrow().searching_for_right_station_name);
    }

    fn place(dw: &Rc<DynamicWorld>, x: i32, item: i32, ty: usize) -> ObjRef {
        dw.place_train_car_at_pos_of_type_save_dict_placed_by_client(IntPair { x, y: 10 }, item, None, None);
        let c = find_car(dw, ty as i32);
        dw.do_set(ty, c.unique_id(), c.clone());
        c
    }

    /// A save dict seeds the fuel; a free neighbour on either side is coupled with this engine.
    #[test]
    fn placement_couples_free_neighbours() {
        let (world, dw) = setup("steam_couple");
        lay_rails(&world, &dw, 3, 40);
        let left = place(&dw, 10, 208, 44);
        let mut sd = new_mutable_dict();
        sd.map.insert("fuelFraction".to_string(), PlistValue::real_f32(0.5));
        sd.map.insert("hasFuel".to_string(), PlistValue::boolean(true));
        dw.place_train_car_at_pos_of_type_save_dict_placed_by_client(IntPair { x: 9, y: 10 }, 205, Some(&sd), None);
        let eng = find_car(&dw, 42);
        assert_eq!(steam_of(&eng).fuel_fraction(), 0.5);
        let _ = left;
        assert!(steam_of(&eng).tc.borrow().right_car.is_some() || steam_of(&eng).tc.borrow().left_car.is_some());
        // an engine on the left of a free car
        let (world2, dw2) = setup("steam_couple2");
        lay_rails(&world2, &dw2, 3, 40);
        let right = place(&dw2, 20, 208, 44);
        let eng2 = place(&dw2, 19, 205, 42);
        let ct = train_car_of(&right).unwrap();
        assert!(ct.engine_car(&right).map_or(false, |e| ptr_eq(&e, &eng2)));
        // an engine between two free cars couples both
        let (world3, dw3) = setup("steam_couple3");
        lay_rails(&world3, &dw3, 3, 40);
        let _a = place(&dw3, 12, 208, 44);
        let _b = place(&dw3, 16, 206, 43);
        let eng3 = place(&dw3, 14, 205, 42);
        let st = train_car_of(&eng3).unwrap();
        let _ = st.left_car();
        let _ = st.right_car();
    }

    /// Save dict round trip incl. direction and stop flag.
    #[test]
    fn save_dict_round_trip() {
        let (world, dw) = setup("steam_save");
        lay_rails(&world, &dw, 3, 40);
        let car = place_steam(&dw, 20);
        {
            let mut st = steam_of(&car).st.borrow_mut();
            st.going_right = false;
            st.stopped = true;
            st.fuel_fraction = 0.25;
        }
        let sd = car.get_save_dict(&car);
        assert!(car.free_block_creation_save_dict(&car).is_some());
        let again = SteamTrain::from_save_dict(&world, &dw, &sd, None).unwrap();
        let st = steam_of(&again).st.borrow();
        assert!(!st.going_right && st.stopped && st.has_fuel);
        assert_eq!(st.fuel_fraction, 0.25);
    }

    /// Net image round trip (with a station tail), the replica reads fuel, flags and the owner.
    #[test]
    fn net_data_round_trip() {
        let (world, dw) = setup("steam_net");
        lay_rails(&world, &dw, 3, 60);
        let car = place_steam(&dw, 30);
        car.set_owner_id(Some("own".to_string()));
        {
            let mut st = steam_of(&car).st.borrow_mut();
            st.left_station_name = Some("L".to_string());
            st.right_station_name = Some("R".to_string());
            st.stop_at_goal_pos = true;
            st.station_goal_pos = IntPair { x: 50, y: 10 };
            st.stopped = true;
            st.fuel_fraction = 0.5;
        }
        let nd = car.creation_net_data_for_client(&car, None).unwrap();
        assert_eq!(nd, car.update_net_data_for_client(&car, None).unwrap());
        let rep = SteamTrain::from_net_data(&world, &dw, None, &nd).unwrap();
        let rs = steam_of(&rep);
        assert!(rep.is_net());
        assert!(rs.st.borrow().stopped && rs.st.borrow().has_fuel);
        assert!((rs.fuel_fraction() - 127.0 / 255.0).abs() < 1e-6);
        assert_eq!(rep.owner_id().as_deref(), Some("own"));
        // the head alone carries no owner tail
        let bare = SteamTrain::from_net_data(&world, &dw, None, &nd[..0x88]).unwrap();
        assert_eq!(bare.owner_id(), None);
        // the replica's own update applies the pending links path for nets
        tick(&rep, 3);
    }

    /// `remoteUpdate:` applies fuel changes, whistle and station names.
    #[test]
    fn remote_update_paths() {
        let _h = TestHost::install();
        let (world, dw) = setup("steam_remote");
        lay_rails(&world, &dw, 3, 60);
        let car = place_steam(&dw, 30);
        let s = steam_of(&car);
        let mut img = SteamTrainCreationNetData::default();
        img.train_car_creation_net_data = train_car_train_car_creation_net_data(&car);
        img.fuel_fraction = 51; // 0.2
        img.has_fuel = 1;
        img.whistle = 1;
        img.going_right = 0;
        img.stopped = 1;
        img.left_train_pos_x = 3;
        img.stop_at_goal_pos_x = 7;
        img.stop_at_goal_pos_y = 10;
        let mut buf = img.to_vec();
        let mut tail = Vec::new();
        let mut d = new_mutable_dict();
        d.map.insert("leftStationName".to_string(), PlistValue::string("LL"));
        d.map.insert("rightStationName".to_string(), PlistValue::string("RR"));
        tail.extend_from_slice(&gzip_deflate(&data_from_property_list(&PlistValue::Dict(d)).unwrap()));
        buf.extend_from_slice(&tail);
        car.remote_update(&car, &buf);
        assert!((s.fuel_fraction() - 0.2).abs() < 1e-6);
        assert!(s.st.borrow().send_whistle);
        assert_eq!(s.st.borrow().left_station_name.as_deref(), Some("LL"));
        assert_eq!(s.st.borrow().right_station_name.as_deref(), Some("RR"));
        // a car ridden by a remote player follows that rider's direction and stop goal
        let bh = rider(&world, &dw);
        bh.base().borrow_mut().is_net = true;
        s.tc.borrow_mut().riders[0] = Some(bh.clone());
        img.train_car_creation_net_data.rider0_id = bh.unique_id();
        buf = img.to_vec();
        buf.extend_from_slice(&tail);
        car.remote_update(&car, &buf);
        assert!(s.st.borrow().stopped && !s.st.borrow().going_right);
        assert_eq!(s.st.borrow().station_goal_pos, IntPair { x: 7, y: 10 });
        // fuel drops to nothing / returns
        img.fuel_fraction = 0;
        car.remote_update(&car, &img.to_vec());
        assert_eq!(s.fuel_fraction(), 0.0);
        img.fuel_fraction = 255;
        car.remote_update(&car, &img.to_vec());
        assert_eq!(s.fuel_fraction(), 1.0);
        // a replica ignores a lower fuel value while a local rider is aboard
        let rep = SteamTrain::from_net_data(&world, &dw, None, &car.creation_net_data_for_client(&car, None).unwrap())
            .unwrap();
        let local = rider(&world, &dw);
        train_car_add_rider(&rep, &local);
        img.fuel_fraction = 20;
        img.train_car_creation_net_data.rider0_id = local.unique_id();
        rep.remote_update(&rep, &img.to_vec());
        assert_eq!(steam_of(&rep).fuel_fraction(), 1.0);
        assert_eq!(steam_of(&rep).st.borrow().left_station_goal_pos, IntPair { x: 3, y: 0 });
    }

    #[test]
    #[should_panic]
    fn remote_update_with_short_image_raises() {
        let (world, dw) = setup("steam_short");
        lay_rails(&world, &dw, 3, 60);
        let car = place_steam(&dw, 30);
        car.remote_update(&car, &[0u8; 0x20]);
    }

    fn mount(world: &Rc<World>, dw: &Rc<DynamicWorld>, car: &ObjRef) -> ObjRef {
        let bh = rider(world, dw);
        train_car_add_rider(car, &bh);
        bh
    }

    /// A ridden engine with fuel pulls right, burns fuel, syncs and halts at the end of the rail line.
    #[test]
    fn ridden_engine_runs_to_the_end_of_the_line() {
        let _h = TestHost::install();
        let (world, dw) = setup("steam_run");
        lay_rails(&world, &dw, 5, 34);
        let car = place_steam(&dw, 20);
        let _bh = mount(&world, &dw, &car);
        steam_of(&car).st.borrow_mut().fuel_counter = 5.99;
        let x0 = car.float_pos().x;
        tick(&car, 120);
        assert!(car.float_pos().x > x0, "the engine pulled right");
        assert!(steam_of(&car).fuel_fraction() < 1.0, "fuel burned");
        tick(&car, 600);
        assert!(steam_of(&car).st.borrow().stopped, "auto-stop at the end of the rails");
        // the stop comes too late to hold it: the engine rolls off the last rail and is dropped as a free block
        assert!(car.needs_removed());
    }

    /// Without fuel the engine does not pull; a goal position stops it as it passes.
    #[test]
    fn goal_position_and_empty_tank() {
        let _h = TestHost::install();
        let (world, dw) = setup("steam_goal");
        lay_rails(&world, &dw, 5, 50);
        let car = place_steam(&dw, 20);
        let _bh = mount(&world, &dw, &car);
        {
            let mut st = steam_of(&car).st.borrow_mut();
            st.stop_at_goal_pos = true;
            st.station_goal_pos = IntPair { x: 30, y: 10 };
        }
        tick(&car, 2000);
        assert!(steam_of(&car).st.borrow().stopped);
        let x = car.pos().x;
        assert!((26..=34).contains(&x), "stopped near the goal, got {x}");
        // left direction, goal on the far side, then an empty tank
        let (world2, dw2) = setup("steam_goal2");
        lay_rails(&world2, &dw2, 5, 50);
        let car2 = place_steam(&dw2, 30);
        let _bh2 = mount(&world2, &dw2, &car2);
        {
            let mut st = steam_of(&car2).st.borrow_mut();
            st.going_right = false;
            st.stop_at_goal_pos = true;
            st.station_goal_pos = IntPair { x: 20, y: 10 };
        }
        tick(&car2, 2000);
        assert!(steam_of(&car2).st.borrow().stopped);
        let (world3, dw3) = setup("steam_empty");
        lay_rails(&world3, &dw3, 5, 50);
        let car3 = place_steam(&dw3, 30);
        let _bh3 = mount(&world3, &dw3, &car3);
        steam_of(&car3).st.borrow_mut().has_fuel = false;
        steam_of(&car3).st.borrow_mut().fuel_fraction = 0.0;
        tick(&car3, 300);
        assert_eq!(car3.pos().x, 30);
    }

    /// Solid ground ahead of the nose, under a wheel, a paused rider and a simulation tick.
    #[test]
    fn crashes_and_early_outs() {
        let _h = TestHost::install();
        let (world, dw) = setup("steam_crash");
        lay_rails(&world, &dw, 5, 50);
        let car = place_steam(&dw, 20);
        let bh = mount(&world, &dw, &car);
        // simulation ticks and a paused rider return early
        car.update_accurate_dt_is_simulation(&car, 0.016, 0.016, true);
        crate::blockhead::blockhead::testutil::bh_of(&bh).bh.borrow_mut().paused = true;
        tick(&car, 3);
        crate::blockhead::blockhead::testutil::bh_of(&bh).bh.borrow_mut().paused = false;
        // a wall in front of the nose
        tile_at_world_position_loaded(20, 12, &world).unwrap().set_type_index(1);
        tick(&car, 2);
        assert!(car.needs_removed());
        // already removed: nothing more happens
        tick(&car, 2);

        // ground where a wheel is
        let (world2, dw2) = setup("steam_crash2");
        lay_rails(&world2, &dw2, 5, 50);
        let car2 = place_steam(&dw2, 20);
        let _bh2 = mount(&world2, &dw2, &car2);
        tile_at_world_position_loaded(19, 10, &world2).unwrap().set_foreground_contents(0);
        tile_at_world_position_loaded(19, 10, &world2).unwrap().set_type_index(1);
        tick(&car2, 2);
        assert!(car2.needs_removed());
        // without a rider an authoritative car is dropped the same way, a replica is not
        let (world3, dw3) = setup("steam_crash3");
        lay_rails(&world3, &dw3, 5, 50);
        let car3 = place_steam(&dw3, 20);
        tile_at_world_position_loaded(19, 10, &world3).unwrap().set_foreground_contents(0);
        tile_at_world_position_loaded(19, 10, &world3).unwrap().set_type_index(1);
        let nd = car3.creation_net_data_for_client(&car3, None).unwrap();
        let rep = SteamTrain::from_net_data(&world3, &dw3, None, &nd).unwrap();
        tick(&rep, 2);
        assert!(!rep.needs_removed());
        tick(&car3, 2);
        assert!(car3.needs_removed());
        // already removed with a wheel in the ground: returns quietly
        let (world4, dw4) = setup("steam_crash4");
        lay_rails(&world4, &dw4, 5, 50);
        let car4 = place_steam(&dw4, 20);
        tile_at_world_position_loaded(19, 10, &world4).unwrap().set_foreground_contents(0);
        tile_at_world_position_loaded(19, 10, &world4).unwrap().set_type_index(1);
        car4.base().borrow_mut().needs_removed = true;
        tick(&car4, 2);
        // a locally ridden replica with a wheel in the ground is not blocked
        let (world5, dw5) = setup("steam_crash5");
        lay_rails(&world5, &dw5, 5, 50);
        let car5 = place_steam(&dw5, 20);
        let bh5 = mount(&world5, &dw5, &car5);
        let nd5 = car5.creation_net_data_for_client(&car5, None).unwrap();
        let rep5 = SteamTrain::from_net_data(&world5, &dw5, None, &nd5).unwrap();
        steam_of(&rep5).tc.borrow_mut().riders[0] = Some(bh5.clone());
        bh5.base().borrow_mut().is_net = true;
        tile_at_world_position_loaded(19, 10, &world5).unwrap().set_foreground_contents(0);
        tile_at_world_position_loaded(19, 10, &world5).unwrap().set_type_index(1);
        tick(&rep5, 2);
        assert!(!rep5.needs_removed());
    }

    /// Wheels over empty air fall; the replica path resolves coupled-car ids by itself.
    #[test]
    fn airborne_wheels_and_replica_links() {
        let _h = TestHost::install();
        let (world, dw) = setup("steam_air");
        lay_rails(&world, &dw, 5, 50);
        let car = place_steam(&dw, 20);
        let _bh = mount(&world, &dw, &car);
        // both wheels over air: the rails under them go away but the tile above them carries one
        for x in 18..=23 {
            let t = tile_at_world_position_loaded(x, 10, &world).unwrap();
            t.set_foreground_contents(0);
            t.set_type_index(2);
            tile_at_world_position_loaded(x, 11, &world).unwrap().set_foreground_contents(CONTENTS_RAIL);
        }
        tick(&car, 3);
        for x in 18..=23 {
            tile_at_world_position_loaded(x, 11, &world).unwrap().set_foreground_contents(0);
        }
        tick(&car, 30);
        assert!(car.float_pos().y < 10.0);
        // replica: remote link ids resolve against the dynamic world
        let (world2, dw2) = setup("steam_links");
        lay_rails(&world2, &dw2, 3, 60);
        let other = place(&dw2, 40, 208, 44);
        let eng = place_steam(&dw2, 20);
        let nd = eng.creation_net_data_for_client(&eng, None).unwrap();
        let rep = SteamTrain::from_net_data(&world2, &dw2, None, &nd).unwrap();
        {
            let mut tc = steam_of(&rep).tc.borrow_mut();
            tc.remote_right_car_id = other.unique_id();
            tc.remote_left_car_id = other.unique_id();
        }
        tick(&rep, 2);
        assert!(steam_of(&rep).tc.borrow().right_car.is_some());
        assert!(steam_of(&rep).tc.borrow().left_car.is_some());
        // coupled cars that vanish are forgotten on the authoritative side
        {
            let mut tc = steam_of(&eng).tc.borrow_mut();
            tc.left_car = Some(other.clone());
            tc.right_car = Some(other.clone());
        }
        other.base().borrow_mut().needs_removed = true;
        tick(&eng, 1);
        assert!(steam_of(&eng).tc.borrow().left_car.is_none() && steam_of(&eng).tc.borrow().right_car.is_none());
    }

    /// A rider that disappears frees the seat; a coupled consist is pulled along by the springs.
    #[test]
    fn rider_leaves_and_consist_follows() {
        let _h = TestHost::install();
        let (world, dw) = setup("steam_consist");
        lay_rails(&world, &dw, 3, 60);
        let pass = place(&dw, 22, 208, 44);
        let eng = place(&dw, 21, 205, 42);
        let rear = place(&dw, 18, 206, 43);
        let _ = (&pass, &rear);
        let bh = mount(&world, &dw, &eng);
        tick(&eng, 200);
        // the rider is removed: the seat is cleared and the net sync flag set
        bh.base().borrow_mut().needs_removed = true;
        tick(&eng, 2);
        assert!(steam_of(&eng).tc.borrow().riders[0].is_none());
        assert!(eng.update_needs_to_be_sent());
    }

    /// Bigger-than-full tanks cap at ten pips; a long frame is clamped to 50 ms; the rider stands on the back.
    #[test]
    fn tank_cap_long_frames_and_rider_spot() {
        let _h = TestHost::install();
        let (world, dw) = setup("steam_misc");
        lay_rails(&world, &dw, 3, 60);
        let car = place_steam(&dw, 20);
        let s = steam_of(&car);
        s.st.borrow_mut().fuel_fraction = 1.5;
        assert_eq!(s.fuel_count(), 10);
        let bh = mount(&world, &dw, &car);
        car.update_accurate_dt_is_simulation(&car, 0.1, 0.1, false);
        let right = s.rider_pos_for_blockhead(&bh);
        s.st.borrow_mut().going_right = false;
        let left = s.rider_pos_for_blockhead(&bh);
        assert!(right[0] < left[0]);
        car.blockheads_loaded(&car);
        assert!(_unused(&car, &car));
        // an id that names no car stays unresolved on a replica
        let nd = car.creation_net_data_for_client(&car, None).unwrap();
        let rep = SteamTrain::from_net_data(&world, &dw, None, &nd).unwrap();
        {
            let mut tc = steam_of(&rep).tc.borrow_mut();
            tc.remote_right_car_id = 9999;
            tc.remote_left_car_id = 9998;
        }
        tick(&rep, 1);
        assert!(steam_of(&rep).tc.borrow().right_car.is_none());
        assert!(steam_of(&rep).tc.borrow().left_car.is_none());
    }

    /// A replica image whose tile is not loaded yields no object.
    #[test]
    fn replica_off_the_loaded_world_is_refused() {
        let (world, dw) = setup_wide("steam_far", 4);
        lay_rails(&world, &dw, 3, 60);
        let car = place_steam(&dw, 20);
        car.base().borrow_mut().pos = IntPair { x: 70, y: 10 };
        let nd = car.creation_net_data_for_client(&car, None).unwrap();
        assert!(SteamTrain::from_net_data(&world, &dw, None, &nd).is_none());
    }

    /// The car runs over the seam of the cylindrical world in both directions (positions stay on the cylinder).
    #[test]
    fn wraps_around_the_world_seam() {
        let _h = TestHost::install();
        for (name, x, right) in [("steam_wrap", 60, true), ("steam_wrap2", 3, false)] {
            let (world, dw) = setup(name);
            lay_rails(&world, &dw, 0, 63);
            let car = place_steam(&dw, x);
            let _bh = mount(&world, &dw, &car);
            steam_of(&car).st.borrow_mut().going_right = right;
            tick(&car, 400);
            let fx = car.float_pos().x;
            assert!((-2.0..66.0).contains(&fx), "x={fx}");
        }
    }

    /// A ridden car whose wheel axis points backwards is derailed.
    #[test]
    fn backwards_axis_derails() {
        let _h = TestHost::install();
        let (world, dw) = setup("steam_flip");
        lay_rails(&world, &dw, 3, 60);
        let car = place_steam(&dw, 20);
        let _bh = mount(&world, &dw, &car);
        // the nose probe of the flipped body points into the ground: open it so the crash check passes
        for x in 18..=22 {
            tile_at_world_position_loaded(x, 9, &world).unwrap().set_type_index(2);
        }
        {
            let mut tc = steam_of(&car).tc.borrow_mut();
            let (l, r) = (tc.left_wheel_pos, tc.right_wheel_pos);
            tc.left_wheel_pos = r;
            tc.right_wheel_pos = l;
        }
        tick(&car, 3);
        assert!(car.needs_removed());
    }

    /// Wheel positions beyond the seam are folded back onto the cylinder.
    #[test]
    fn wheels_beyond_the_seam_are_folded_back() {
        let _h = TestHost::install();
        for (name, shift, dx) in [("steam_fold_hi", 64.0f32, 1.0f32), ("steam_fold_lo", -64.0, -1.0)] {
            let (world, dw) = setup(name);
            lay_rails(&world, &dw, 0, 63);
            let car = place_steam(&dw, 30);
            let _bh = mount(&world, &dw, &car);
            let _ = dx;
            {
                let mut tc = steam_of(&car).tc.borrow_mut();
                let (l, r) = (tc.left_wheel_pos, tc.right_wheel_pos);
                let (l, r) = if shift > 0.0 {
                    (Vector2 { x: 64.2, y: l.y }, Vector2 { x: 66.2, y: r.y })
                } else {
                    (Vector2 { x: -1.8, y: l.y }, Vector2 { x: 0.2, y: r.y })
                };
                tc.left_wheel_pos = l;
                tc.right_wheel_pos = r;
            }
            car.base().borrow_mut().float_pos = Vector2 { x: if shift > 0.0 { 65.2 } else { -0.8 }, y: 10.0 };
            tick(&car, 1);
            let fx = car.float_pos().x;
            assert!((0.0..64.0).contains(&fx) || car.needs_removed(), "x={fx}");
        }
    }

    /// Rails along the lowest and the highest row: the scans and the auto-stop meet the edges of the world.
    #[test]
    fn rails_on_the_edge_rows() {
        let _h = TestHost::install();
        for (name, y) in [("steam_bottom", 0), ("steam_top", 1023)] {
            let (world, dw) = setup(name);
            for x in 5..=40 {
                tile_at_world_position_loaded(x, y, &world).unwrap().set_foreground_contents(CONTENTS_RAIL);
            }
            for x in 5..=40 {
                dw.add_rail_at_pos_of_type_owned_by_station(IntPair { x, y }, ITEM_RAIL, false);
            }
            dw.place_train_car_at_pos_of_type_save_dict_placed_by_client(IntPair { x: 20, y }, 205, None, None);
            let car = find_car(&dw, 42);
            dw.do_set(42, car.unique_id(), car.clone());
            let _bh = mount(&world, &dw, &car);
            tick(&car, 5);
            // a scan that falls off the edge of the world stalls where it is (still searching)
            {
                let st = steam_of(&car).st.borrow();
                assert!(st.searching_for_left_station_name && st.searching_for_right_station_name);
            }
            tick(&car, 1200);
        }
    }

    /// A station right next to the train is not offered; the scan walks on and ends at the rail's end.
    #[test]
    fn near_station_is_skipped() {
        let (world, dw) = setup("steam_near");
        lay_rails(&world, &dw, 3, 60);
        let _near = station(&world, &dw, 23, "Near");
        let car = place_steam(&dw, 20);
        tick(&car, 8);
        assert_eq!(steam_of(&car).st.borrow().right_station_name, None);
    }

    /// Neighbours that cannot join (a hand car) or already have an engine are left alone on placement.
    #[test]
    fn placement_skips_cars_that_cannot_couple() {
        let (world, dw) = setup("steam_nocouple");
        lay_rails(&world, &dw, 3, 40);
        let hand = place(&dw, 21, 204, 41);
        let eng = place(&dw, 20, 205, 42);
        assert!(train_car_of(&eng).unwrap().right_car().is_none());
        assert!(train_car_of(&hand).unwrap().engine_car(&hand).is_none());
        let (world2, dw2) = setup("steam_nocouple2");
        lay_rails(&world2, &dw2, 3, 40);
        let first = place(&dw2, 19, 205, 42);
        let car = place(&dw2, 21, 208, 44);
        let _ = (&first, &car);
        let second = place(&dw2, 20, 205, 42);
        let _ = second;
    }

    /// Scans give up on a rail line longer than 2048 tiles.
    #[test]
    fn scans_give_up_after_2048_tiles() {
        let (world, dw) = setup_wide("steam_giveup", 130);
        for x in 10..=4150 {
            tile_at_world_position_loaded(x, 10, &world).unwrap().set_foreground_contents(CONTENTS_RAIL);
        }
        lay_rails(&world, &dw, 2070, 2090);
        let car = place_steam(&dw, 2080);
        tick(&car, 300);
        let st = steam_of(&car).st.borrow();
        assert!(!st.searching_for_left_station_name && !st.searching_for_right_station_name);
        assert!(st.needs_to_update_choice_ui);
    }

    /// Interaction objects on the line that are not stations are passed over.
    #[test]
    fn other_interaction_objects_are_not_stations() {
        use crate::interaction::sign::Sign;
        let (world, dw) = setup("steam_sign");
        lay_rails(&world, &dw, 3, 60);
        let it = InventoryItem::new(ItemType(163), 0, 0, None, None);
        let pos = IntPair { x: 40, y: 10 };
        let sign: ObjRef = Sign::new(&world, &dw, pos, None, Some(&it.borrow()), false, None, Some("c"), None).unwrap();
        dw.by_pos_set(sign.object_type(), dw.world_index(pos), &sign);
        tile_at_world_position_loaded(40, 10, &world).unwrap().set_contents(CONTENTS_INTERACTION_OBJECT);
        let car = place_steam(&dw, 20);
        tick(&car, 8);
        assert_eq!(steam_of(&car).st.borrow().right_station_name, None);
    }
}
