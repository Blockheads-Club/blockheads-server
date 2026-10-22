//! `Boat` (DynamicObject type 32): the rideable boat. Floats on water (buoyancy integrator on `y_speed`), moves
//! horizontally from `actual_x_speed`, collides with solid tiles, carries at most one rider and turns back into an
//! item when crushed.
//!
//! every `-[Boat...]` is cited at its item).
//! Only the SERVER branches are ported (`DynamicWorld::is_client()` is always false); `draw:`, `loadDerivedStuff`
//! assets and `cameraPosForBlockhead:` are client only. The class has no RNG and no log strings.
//!
//! Numeric rules: f32 unless the spec writes `(double)`; `min(0.5, x)` forms go through [`min_d`] (C `if` chain).

use std::any::Any;
use std::cell::RefCell;
use std::rc::{Rc, Weak};

use crate::dynworld::state::DynamicWorld;
use crate::fmath::cconv::{f32_to_i16, f32_to_i32};
use crate::foundation::gzip::{gzip_deflate, gzip_inflate};
use crate::objects::base::{
    base_dynamic_object_net_data, base_get_save_dict, base_init_with_world_dynamic_world_at_position_cache,
    base_init_with_world_dynamic_world_cache_net_data, base_init_with_world_dynamic_world_save_dict_cache,
    base_remote_update, base_update_position, new_mutable_dict, padded_bytes, DynamicObject, DynamicObjectBase,
    DynamicWorldRef, WorldRef,
};
use crate::objects::class_info::{ClassInfo, DYNAMIC_OBJECT_CLASS};
use crate::objects::handle::{alloc_obj, ptr_eq, CacheRef, ObjRef};
use crate::objects::protocols::{RidableDynamicObject, TapChoiceDynamicObject};
use crate::plist::helpers::{data_from_property_list, property_list_from_data_opt};
use crate::plist::value::{PlistDict, PlistValue};
use crate::types::item_type::ItemType;
use crate::types::netdata::BoatCreationNetData;
use crate::types::structs::{IntPair, Vector2, WireStruct};
use crate::world::helpers::{tile_is_air, tile_is_air_water_or_snow, tile_is_solid, tile_is_water};
use crate::world::state::World;
use crate::world::tiles::tile_at_world_position_loaded;

/// `DYNAMIC_OBJECT_TYPE_BOAT`.
pub const DYNAMIC_OBJECT_TYPE_BOAT: i32 = 32;
/// `ITEM_BOAT`.
pub const ITEM_BOAT: i32 = 111;
/// `pi` as f32 (`bodyRotation` initial value).
pub const BODY_ROTATION_INIT: f32 = 3.14159274f32;

/// `Boat: DynamicObject <RidableDynamicObject, TapChoiceDynamicObject>`.
pub static BOAT_CLASS: ClassInfo = ClassInfo {
    name: "Boat",
    superclass: Some(&DYNAMIC_OBJECT_CLASS),
    protocols: &["RidableDynamicObject", "TapChoiceDynamicObject"],
};

/// `min(c, x)` as compiled: `c` when `c <= x`, else `x`.
#[inline]
fn min_d(c: f64, x: f64) -> f64 {
    if c <= x {
        c
    } else {
        x
    }
}

/// Boat ivars (client-only ones omitted).
pub struct BoatState {
    /// 0x90, init pi, returned by `rider_body_y_rotation_for_blockhead`
    pub body_rotation: f32,
    /// 0x98
    pub goal_rotation_type: i32,
    /// 0x9c set from the rider's d-pad (6 * v.x)
    pub target_x_speed: f32,
    /// 0xa0
    pub actual_x_speed: f32,
    /// 0xa4
    pub y_speed: f32,
    /// 0xa8
    pub being_controlled: bool,
    /// 0xb0 retained
    pub rider: Option<ObjRef>,
    /// 0xb8: -1 after save-load init; 0 after the placement and net inits (zeroed alloc)
    pub saved_blockhead_index: i32,
    /// 0xc4
    pub remote_goal_pos: IntPair,
    /// 0xcc
    pub remote_send_control_event_timer: f32,
}

/// `Boat`. The owner id lives in `DynamicObjectBase.owner_id`.
pub struct Boat {
    base: RefCell<DynamicObjectBase>,
    /// The Boat ivars.
    pub st: RefCell<BoatState>,
    world: Weak<World>,
    dw: Weak<DynamicWorld>,
}

/// Downcast helper.
pub fn as_boat(o: &ObjRef) -> Option<&Boat> {
    o.as_any().downcast_ref::<Boat>()
}

/// Pointer identity of two possibly-nil handles (`nil == nil` is true).
fn same_opt(a: &Option<ObjRef>, b: &Option<ObjRef>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(a), Some(b)) => ptr_eq(a, b),
        _ => false,
    }
}

/// `[rider isNet]` with nil messaging (NO).
fn rider_is_net(r: &Option<ObjRef>) -> bool {
    r.as_ref().map_or(false, |r| r.is_net())
}

/// `[rider stopRiding]` (nil-safe; only a real Blockhead can ride).
fn stop_riding(r: &Option<ObjRef>) {
    if let Some(r) = r {
        if let Some(b) = crate::blockhead::blockhead::as_blockhead(r) {
            crate::blockhead::blockhead::host().stop_riding(b);
        }
    }
}

impl Boat {
    fn alloc(world: &Rc<World>, dw: &Rc<DynamicWorld>) -> Rc<Boat> {
        let (w, d) = (Rc::downgrade(world), Rc::downgrade(dw));
        alloc_obj(|sw| Boat {
            base: RefCell::new(DynamicObjectBase::alloc_zeroed_with(sw)),
            // `loadDerivedStuff` (run at the end of every init) sets bodyRotation / goalRotationType
            st: RefCell::new(BoatState {
                body_rotation: BODY_ROTATION_INIT,
                goal_rotation_type: 0,
                target_x_speed: 0.0,
                actual_x_speed: 0.0,
                y_speed: 0.0,
                being_controlled: false,
                rider: None,
                saved_blockhead_index: 0,
                remote_goal_pos: IntPair { x: 0, y: 0 },
                remote_send_control_event_timer: 0.0,
            }),
            world: w,
            dw: d,
        })
    }

    fn world(&self) -> Rc<World> {
        self.world.upgrade().expect("Boat: world dropped")
    }

    fn dw(&self) -> Rc<DynamicWorld> {
        self.dw.upgrade().expect("Boat: dynamic world dropped")
    }

    fn pos(&self) -> IntPair {
        self.base.borrow().pos
    }

    fn fp(&self) -> Vector2 {
        self.base.borrow().float_pos
    }

    fn set_fp(&self, v: Vector2) {
        self.base.borrow_mut().float_pos = v;
    }

    fn set_update_needs_to_be_sent(&self, v: bool) {
        self.base.borrow_mut().update_needs_to_be_sent = v;
    }

    fn notify_changed(&self) {
        self.dw().dynamic_world_changed_at_pos_object_type(self.pos(), DYNAMIC_OBJECT_TYPE_BOAT);
    }

    fn zero_speeds(&self) {
        let mut st = self.st.borrow_mut();
        st.actual_x_speed = 0.0;
        st.target_x_speed = 0.0;
    }

    // ------------------------------------------------------------------ initialisers

    /// `-initWithWorld:dynamicWorld:atPosition:cache:saveDict:placedByClient:` (a player places a boat).
    ///  `saved_blockhead_index` stays 0.
    pub fn new(
        world: &Rc<World>,
        dw: &Rc<DynamicWorld>,
        pos: IntPair,
        cache: CacheRef,
        save_dict: Option<&PlistDict>,
        client_id: Option<&str>,
    ) -> Option<ObjRef> {
        let me = Self::alloc(world, dw);
        let me_obj: ObjRef = me.clone();
        let wref: WorldRef = world.clone();
        let dref: DynamicWorldRef = dw.clone();
        if !base_init_with_world_dynamic_world_at_position_cache(&me_obj, &wref, &dref, pos, cache) {
            return None;
        }
        let mut fp = me.fp();
        fp.y = 0.5f32 + me.pos().y as f32;
        me.set_fp(fp);
        match client_id {
            None => {
                if let Some(v) = save_dict.and_then(|d| d.map.get(&"ownerID".to_string())) {
                    me_obj.set_owner_id(v.as_str().map(|s| s.to_string()));
                }
            }
            Some(c) => me_obj.set_owner_id(Some(c.to_string())),
        }
        Some(me_obj)
    }

    /// `-initWithWorld:dynamicWorld:saveDict:cache:`.
    pub fn from_save_dict(
        world: &Rc<World>,
        dw: &Rc<DynamicWorld>,
        save_dict: &PlistDict,
        cache: CacheRef,
    ) -> Option<ObjRef> {
        let me = Self::alloc(world, dw);
        let me_obj: ObjRef = me.clone();
        let wref: WorldRef = world.clone();
        let dref: DynamicWorldRef = dw.clone();
        if !base_init_with_world_dynamic_world_save_dict_cache(&me_obj, &wref, &dref, save_dict, cache) {
            return None;
        }
        {
            let mut st = me.st.borrow_mut();
            st.saved_blockhead_index = -1;
            if let Some(v) = save_dict.map.get(&"currentBlockheadIndex".to_string()) {
                st.saved_blockhead_index = v.int_value();
            }
        }
        me_obj.set_owner_id(save_dict.map.get(&"ownerID".to_string()).and_then(|v| v.as_str()).map(|s| s.to_string()));
        Some(me_obj)
    }

    /// `-initWithWorld:dynamicWorld:cache:netData:` (`is_net = true`; ends with the virtual `remoteUpdate:`).
    pub fn from_net_data(world: &Rc<World>, dw: &Rc<DynamicWorld>, cache: CacheRef, net_data: &[u8]) -> Option<ObjRef> {
        let me = Self::alloc(world, dw);
        let me_obj: ObjRef = me.clone();
        let wref: WorldRef = world.clone();
        let dref: DynamicWorldRef = dw.clone();
        if !base_init_with_world_dynamic_world_cache_net_data(&me_obj, &wref, &dref, cache, net_data) {
            return None;
        }
        let pos = me.pos();
        me.st.borrow_mut().remote_goal_pos = pos;
        let mut fp = me.fp();
        fp.y = 0.5f32 + pos.y as f32;
        me.set_fp(fp);
        if net_data.len() > 0x28 {
            let inflated = gzip_inflate(&net_data[0x28..]);
            let dict = property_list_from_data_opt(inflated.as_deref());
            me_obj.set_owner_id(
                dict.as_ref().and_then(|d| d.object_for_key("ownerID")).and_then(|v| v.as_str()).map(|s| s.to_string()),
            );
        }
        me_obj.remote_update(&me_obj, net_data);
        Some(me_obj)
    }

    // ------------------------------------------------------------------ accessors

    /// `-rider`
    pub fn rider(&self) -> Option<ObjRef> {
        self.st.borrow().rider.clone()
    }
    /// `-targetXSpeed`
    pub fn target_x_speed(&self) -> f32 {
        self.st.borrow().target_x_speed
    }
    /// `-actualXSpeed`
    pub fn actual_x_speed(&self) -> f32 {
        self.st.borrow().actual_x_speed
    }
    /// `-beingControlled`
    pub fn being_controlled(&self) -> bool {
        self.st.borrow().being_controlled
    }
    /// `-maxHealth`
    pub fn max_health(&self) -> u16 {
        0x20
    }

    /// `-tapIsWithinBodyRadius:` (side-effect free hit test; wrapped horizontal distance).
    pub fn tap_is_within_body_radius(&self, tap: Vector2) -> bool {
        let w = self.world().world_width_macro().wrapping_shl(5);
        let fp = self.fp();
        let dx = tap.x - fp.x;
        let dxw = if dx < (w / 2) as f32 {
            if ((self.world().world_width_macro().wrapping_mul(-32)) / 2) as f32 <= dx {
                dx
            } else {
                dx + w as f32
            }
        } else {
            dx - w as f32
        };
        (dxw as f64) < 0.5f64
            && -0.5f64 < (dxw as f64)
            && (fp.y as f64) - 0.5f64 < (tap.y as f64)
            && (tap.y as f64) < (fp.y as f64) + 0.5f64
    }

    /// `-addRider:` with a possibly-nil argument.
    pub fn add_rider_opt(&self, me: &ObjRef, rider_: Option<&ObjRef>) {
        let mut r: Option<ObjRef> = rider_.cloned();
        let controlled = r
            .as_ref()
            .and_then(|b| b.as_blockhead())
            .map_or(false, |b| b.is_client_blockhead_being_controlled_by_server());
        if controlled {
            stop_riding(&r);
            self.st.borrow_mut().rider = None; // [self.rider autorelease]
            r = None;
        }
        let cur = self.rider();
        if !same_opt(&r, &cur) {
            self.set_update_needs_to_be_sent(true); // !isClient
                                                    // the previous rider is NOT released here
            self.st.borrow_mut().rider = r;
            self.notify_changed();
            let now = self.rider();
            if now.is_some() && !rider_is_net(&now) {
                self.set_update_needs_to_be_sent(true);
            }
        }
        let _ = me;
        let has = self.rider().is_some();
        self.st.borrow_mut().being_controlled = has;
        if !has {
            self.zero_speeds();
        }
    }

    /// `-update:accurateDT:isSimulation:`, steps (1) to (7).
    fn update_impl(&self, me: &ObjRef, dt_in: f32, is_simulation: bool) {
        let world = self.world();
        let dw = self.dw();
        let mut dt = dt_in;
        if (dt as f64) > 0.05f64 {
            dt = 0.05f32;
        }
        // (1) drop a dead rider (no stopRiding)
        let rider = self.rider();
        if rider.as_ref().map_or(false, |r| r.needs_removed()) {
            if !rider_is_net(&rider) {
                self.set_update_needs_to_be_sent(true);
            }
            let mut st = self.st.borrow_mut();
            st.rider = None;
            st.being_controlled = false;
        }
        // (2) early outs
        let rider = self.rider();
        if let Some(b) = rider.as_ref().and_then(|r| crate::blockhead::blockhead::as_blockhead(r)) {
            if b.paused() {
                return;
            }
        }
        if is_simulation {
            return;
        }

        // (3) network position smoothing
        let is_net = me.is_net();
        let rider = self.rider();
        let run3 = is_net || rider_is_net(&rider);
        let goal = self.st.borrow().remote_goal_pos;
        let pos = self.pos();
        if run3 && !(rider.is_some() && !rider_is_net(&rider)) && (goal != pos) {
            let wwm = world.world_width_macro();
            let w = wwm.wrapping_shl(5);
            let d = goal.x.wrapping_sub(pos.x);
            let dxw = if d < w / 2 {
                if d < (wwm.wrapping_mul(-0x20)) / 2 {
                    d.wrapping_add(w)
                } else {
                    d
                }
            } else {
                d.wrapping_sub(w)
            };
            if (dxw.wrapping_abs() as f32) > 8.0f32 || goal.y.wrapping_sub(pos.y).wrapping_abs() > 2 {
                // teleport
                self.set_fp(Vector2 { x: 0.5f32 + goal.x as f32, y: 0.5f32 + goal.y as f32 });
                me.update_position(me, goal);
                if !is_net && !rider_is_net(&rider) {
                    // dead in every configuration (update (3)); kept literal
                    self.notify_changed();
                    self.set_update_needs_to_be_sent(true);
                }
            } else {
                if pos.x + 1 < goal.x || goal.x < pos.x - 1 {
                    self.st.borrow_mut().actual_x_speed += 0.2f32 * (dxw as f32);
                }
                let dy = (goal.y - pos.y) as f32;
                if pos.y + 1 < goal.y || goal.y < pos.y - 1 {
                    self.st.borrow_mut().y_speed += 0.2f32 * dy;
                }
            }
        }

        // (4) buoyancy / gravity
        let pos = self.pos();
        let mut x_addition = 0.0f32;
        let above_motion_threshold = self.actual_x_speed().abs() > 2.0f32;
        let tile = tile_at_world_position_loaded(pos.x, pos.y, &world);
        {
            let fpy = self.fp().y;
            let mut st = self.st.borrow_mut();
            if tile_is_water(tile.clone()) {
                let surface = (fpy + (-0.5f32)) - pos.y as f32;
                let pcl = tile.as_ref().map_or(0u8, |t| t.partial_content_left());
                if surface <= (pcl as f32) / 255.0f32 || pcl == 0xff {
                    let k = min_d(0.5f64, (1.0f32 * dt) as f64);
                    st.y_speed = ((st.y_speed as f64) * (1.0f64 - k) + (4.0f32 * dt) as f64) as f32;
                } else {
                    let k = min_d(0.5f64, (0.25f32 * dt) as f64);
                    st.y_speed = ((st.y_speed as f64) * (1.0f64 - k) - (8.0f32 * dt) as f64) as f32;
                }
            } else if tile_is_air(tile) {
                let k = min_d(0.5f64, (0.25f32 * dt) as f64);
                st.y_speed = ((st.y_speed as f64) * (1.0f64 - k) - (8.0f32 * dt) as f64) as f32;
            }
        }
        if above_motion_threshold {
            x_addition = self.actual_x_speed() * dt * 1.0f32;
        }
        let old_float_pos = self.fp();
        let y_speed = self.st.borrow().y_speed;
        let mut fp = old_float_pos;
        fp.x += x_addition;
        fp.y += (((y_speed * dt) as f64) * 0.5f64) as f32;
        self.set_fp(fp);
        let compute_new_pos = |fp: Vector2| IntPair { x: f32_to_i32(fp.x), y: f32_to_i32(fp.y - 0.5f32) };
        let free_at = |p: IntPair| -> bool {
            let t0 = tile_at_world_position_loaded(p.x, p.y, &world);
            let t1 = tile_at_world_position_loaded(p.x, p.y + 1, &world);
            tile_is_air_water_or_snow(t0) && tile_is_air_water_or_snow(t1)
        };
        let mut new_pos = compute_new_pos(fp);

        // (5) collision when the tile cell changed
        if new_pos != pos {
            if !free_at(new_pos) {
                if new_pos.x != pos.x {
                    let mut f = self.fp();
                    f.x = old_float_pos.x;
                    self.set_fp(f);
                    let mut st = self.st.borrow_mut();
                    if (pos.x < new_pos.x && st.actual_x_speed > 0.0f32)
                        || (new_pos.x < pos.x && st.actual_x_speed < 0.0f32)
                    {
                        st.actual_x_speed = 0.0;
                    }
                }
                new_pos = compute_new_pos(self.fp());
                if !free_at(new_pos) {
                    // still blocked
                    if new_pos.y != pos.y {
                        let mut f = self.fp();
                        f.y = old_float_pos.y;
                        self.set_fp(f);
                        let ys = self.st.borrow().y_speed;
                        if (pos.y < new_pos.y && ys > 0.0f32) || (new_pos.y < pos.y && ys < 0.0f32) {
                            if ys < 0.0f32 {
                                // hit the ground going down
                                let mut f = self.fp();
                                f.y = pos.y as f32;
                                self.set_fp(f);
                                new_pos = compute_new_pos(f);
                                if !free_at(new_pos) && !me.needs_removed() && !is_net {
                                    // the boat is crushed: it turns back into an item
                                    new_pos = pos;
                                    self.set_fp(old_float_pos);
                                    me.set_needs_removed(me, true);
                                    self.set_update_needs_to_be_sent(true);
                                    let save_dict = me.get_save_dict(me);
                                    let rider = self.rider(); // read after getSaveDict
                                    dw.create_free_block_at_position_of_type_data_a_data_b_sub_items_dynamic_object_save_dict_hovers_play_sound_priority_blockhead(
                                        self.pos(),
                                        ItemType(ITEM_BOAT as u32),
                                        0,
                                        0,
                                        None,
                                        Some(PlistValue::Dict(save_dict)),
                                        false,
                                        1,
                                        rider.as_ref(),
                                    );
                                }
                            }
                            self.st.borrow_mut().y_speed = 0.0;
                        }
                    }
                } else if !is_net {
                    self.set_update_needs_to_be_sent(true);
                }
            } else if !is_net {
                self.set_update_needs_to_be_sent(true);
            }
            me.update_position(me, new_pos);
        }

        // (6) clear the target speed unless actively controlled by the local active player
        let rider = self.rider();
        let being_controlled = self.being_controlled();
        if being_controlled {
            if !is_net {
                let active = dw.active_blockhead();
                if !same_opt(&active, &rider) {
                    self.st.borrow_mut().target_x_speed = 0.0;
                }
            }
        } else {
            self.st.borrow_mut().target_x_speed = 0.0;
        }

        // (7) speed control (only when ridden by a non-network blockhead)
        if being_controlled && !rider_is_net(&rider) {
            let pos = self.pos();
            let (actual, target) = {
                let st = self.st.borrow();
                (st.actual_x_speed, st.target_x_speed)
            };
            let mut new_x_speed = actual;
            if target.abs() <= 2.0f32 {
                new_x_speed = 0.0;
            } else {
                let dir: i32 = if target > 0.0f32 { 1 } else { -1 };
                let frac = (self.fp().x - 0.5f32) - pos.x as f32;
                if frac > 0.0f32 && dir == -1 {
                    new_x_speed = target * frac * 2.0f32;
                } else if frac < 0.0f32 && dir == 1 {
                    new_x_speed = target * (-frac) * 2.0f32;
                }
                let t = tile_at_world_position_loaded(pos.x + dir, pos.y, &world);
                if tile_is_air_water_or_snow(t.clone()) {
                    new_x_speed = target;
                } else if tile_is_solid(t) {
                    new_x_speed = 0.0;
                }
            }
            let a1 = 2.0f32 * dt;
            let a = if 0.5f32 <= a1 { 0.5f32 } else { a1 };
            let flag = {
                let mut st = self.st.borrow_mut();
                st.actual_x_speed = st.actual_x_speed * (1.0f32 - a) + new_x_speed * a;
                st.remote_send_control_event_timer += dt;
                if 0.5f32 < st.remote_send_control_event_timer {
                    st.remote_send_control_event_timer -= 0.5f32;
                    true
                } else {
                    false
                }
            };
            if flag {
                self.set_update_needs_to_be_sent(true);
            }
        }
    }
}

impl DynamicObject for Boat {
    fn base(&self) -> &RefCell<DynamicObjectBase> {
        &self.base
    }
    fn class_info(&self) -> &'static ClassInfo {
        &BOAT_CLASS
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    /// `-objectType`
    fn object_type(&self) -> i32 {
        DYNAMIC_OBJECT_TYPE_BOAT
    }
    /// `-freeblockCreationItemType`
    fn freeblock_creation_item_type(&self) -> i32 {
        ITEM_BOAT
    }
    /// `-blockheadsLoaded`: restores the saved rider.
    fn blockheads_loaded(&self, me: &ObjRef) {
        let saved = self.st.borrow().saved_blockhead_index;
        if saved != -1 {
            let blockheads = self.dw().blockheads();
            if saved >= 0 && (saved as usize) < blockheads.len() {
                self.st.borrow_mut().rider = Some(blockheads[saved as usize].clone());
            }
            self.st.borrow_mut().saved_blockhead_index = -1;
            let rider = self.rider();
            let controlled = rider
                .as_ref()
                .and_then(|r| r.as_blockhead())
                .map_or(false, |b| b.is_client_blockhead_being_controlled_by_server());
            if controlled {
                stop_riding(&rider);
                {
                    let mut st = self.st.borrow_mut();
                    st.rider = None;
                    st.being_controlled = false;
                }
                self.zero_speeds();
            } else {
                // `[rider setRidingObject:self]` (no-op for a nil rider)
                if let Some(b) = rider.as_ref().and_then(|r| crate::blockhead::blockhead::as_blockhead(r)) {
                    b.bh.borrow_mut().ride_object = Some(me.clone());
                }
                // even if the rider is nil (index out of range)
                self.st.borrow_mut().being_controlled = true;
            }
        }
    }
    /// `-getSaveDict`: `currentBlockheadIndex` only when the rider is in the local list (never for
    /// net riders), `ownerID`.
    fn get_save_dict(&self, me: &ObjRef) -> PlistDict {
        let mut d = base_get_save_dict(me);
        if let Some(rider) = self.rider() {
            let mut i = 0i32;
            let mut found = false;
            for bh in self.dw().blockheads() {
                if ptr_eq(&bh, &rider) && !bh.needs_removed() {
                    found = true;
                    break;
                }
                i += 1;
            }
            if found {
                d.map.insert("currentBlockheadIndex".to_string(), PlistValue::int(i as i64));
            }
        }
        if let Some(o) = me.owner_id() {
            d.map.insert("ownerID".to_string(), PlistValue::string(o));
        }
        d
    }
    /// `-creationNetDataForClient:`: 0x28-byte struct + gzip(XML plist). Padding zero.
    fn creation_net_data_for_client(&self, me: &ObjRef, _client_id: Option<&str>) -> Option<Vec<u8>> {
        let nd = BoatCreationNetData {
            dynamic_object_net_data: base_dynamic_object_net_data(me),
            rider_id: self.rider().map_or(0, |r| r.unique_id()),
            actual_x_speed: f32_to_i16(self.actual_x_speed() * 255.0f32),
            padding: [0; 6],
        };
        let mut data = nd.to_vec();
        let mut dict = new_mutable_dict();
        if let Some(o) = me.owner_id() {
            dict.map.insert("ownerID".to_string(), PlistValue::string(o));
        }
        if let Some(xml) = data_from_property_list(&PlistValue::Dict(dict)) {
            data.extend_from_slice(&gzip_deflate(&xml));
        }
        Some(data)
    }
    /// `-updateNetDataForClient:` = creation data.
    fn update_net_data_for_client(&self, me: &ObjRef, client_id: Option<&str>) -> Option<Vec<u8>> {
        self.creation_net_data_for_client(me, client_id)
    }
    /// `-remoteUpdate:` (SERVER branch).
    fn remote_update(&self, me: &ObjRef, net_data: &[u8]) {
        base_remote_update(me, net_data);
        if me.needs_removed() {
            return;
        }
        let u = BoatCreationNetData::read_le(&padded_bytes(net_data, 0x28));
        let dw = self.dw();
        let rider = self.rider();
        if rider.is_some() && !rider_is_net(&rider) {
            // server-local (non-network) rider
            if u.rider_id == 0 {
                return;
            }
            if u.rider_id == rider.as_ref().map_or(0, |r| r.unique_id()) {
                return;
            }
            stop_riding(&rider);
            self.st.borrow_mut().rider = None;
            self.zero_speeds();
            return;
        }
        let speed = (u.actual_x_speed as i32 as f32) / 255.0f32;
        let mut goal = IntPair { x: u.dynamic_object_net_data.pos_x as i32, y: u.dynamic_object_net_data.pos_y as i32 };
        goal.x = f32_to_i32(goal.x as f32 + 0.2f32 * speed);
        {
            let mut st = self.st.borrow_mut();
            st.actual_x_speed = speed;
            st.remote_goal_pos = goal;
        }
        self.set_update_needs_to_be_sent(true);
        if u.rider_id == 0 {
            if rider.is_some() {
                stop_riding(&rider);
                {
                    let mut st = self.st.borrow_mut();
                    st.rider = None;
                    st.being_controlled = false;
                }
                self.zero_speeds();
            }
        } else {
            if let Some(r) = &rider {
                if u.rider_id == r.unique_id() {
                    return;
                }
            }
            for bh in dw.all_blockheads_including_net() {
                if bh.is_net() && bh.unique_id() == u.rider_id {
                    self.add_rider_opt(me, Some(&bh));
                    self.st.borrow_mut().being_controlled = true;
                }
            }
        }
    }
    /// `-updatePosition:`
    fn update_position(&self, me: &ObjRef, new_position: IntPair) {
        let rider = self.rider();
        if rider.is_some() && !rider_is_net(&rider) && new_position.x != self.pos().x {
            self.world().mark_circum_navigate_x(new_position.x);
        }
        base_update_position(me, new_position);
    }
    /// `-update:accurateDT:isSimulation:`
    fn update_accurate_dt_is_simulation(&self, me: &ObjRef, dt: f32, _accurate_dt: f32, is_simulation: bool) {
        self.update_impl(me, dt, is_simulation);
    }
    /// `-setNeedsRemoved:`: the rider is told to stop riding when the flag changes false -> true.
    fn set_needs_removed(&self, _me: &ObjRef, v: bool) {
        let was = self.base.borrow().needs_removed;
        if v && !was {
            stop_riding(&self.rider());
        }
        self.base.borrow_mut().needs_removed = v;
    }
    /// `-blockheadUnloaded:` (no stopRiding, no flag).
    fn blockhead_unloaded(&self, _me: &ObjRef, b: &ObjRef) {
        let r = self.rider();
        if r.as_ref().map_or(false, |r| ptr_eq(r, b)) {
            {
                let mut st = self.st.borrow_mut();
                st.rider = None;
                st.being_controlled = false;
            }
            self.zero_speeds();
        }
    }
    /// `-worldChanged:`: no-op.
    fn world_changed(&self, _me: &ObjRef, _positions: &mut Vec<IntPair>) {}
    /// `-renderPos`: `floatPos + (0, -0.3f)`.
    fn render_pos(&self) -> Vector2 {
        let fp = self.fp();
        Vector2 { x: fp.x + 0.0f32, y: fp.y + (-0.3f32) }
    }
    /// `-dealloc`: Rust drops the rider handle.
    fn dealloc(&self, _me: &ObjRef) {}
    fn as_ridable(&self) -> Option<&dyn RidableDynamicObject> {
        Some(self)
    }
    fn as_tap_choice(&self) -> Option<&dyn TapChoiceDynamicObject> {
        Some(self)
    }
}

impl RidableDynamicObject for Boat {
    /// `-riderPosForBlockhead:` (the w lane is unspecified and unused; 1.0).
    fn rider_pos_for_blockhead(&self, _b: &ObjRef) -> [f32; 4] {
        let fp = self.fp();
        [fp.x, ((fp.y as f64) + (-0.4f64) + (-0.5f64)) as f32, -0.75f32, 1.0]
    }
    /// `-cameraPosForBlockhead:` (client only; returns the render position).
    fn camera_pos_for_blockhead(&self, _b: &ObjRef) -> Vector2 {
        self.render_pos()
    }
    /// `-riderBodyYRotationForBlockhead:`
    fn rider_body_y_rotation_for_blockhead(&self, _b: &ObjRef) -> f32 {
        self.st.borrow().body_rotation
    }
    /// `-setTargetVelocity:`
    fn set_target_velocity(&self, v: Vector2) {
        self.st.borrow_mut().target_x_speed = 6.0f32 * v.x;
    }
    /// `-rideDirection`
    fn ride_direction(&self) -> i32 {
        let st = self.st.borrow();
        if st.rider.is_none() {
            return 0;
        }
        if st.target_x_speed > 2.0f32 {
            return 1;
        }
        if st.target_x_speed <= -2.0f32 && st.target_x_speed != -2.0f32 {
            return -1;
        }
        0
    }
    /// `-addRider:`
    fn add_rider(&self, me: &ObjRef, b: &ObjRef) {
        self.add_rider_opt(me, Some(b));
    }
    /// `-removeRider:` (server branch)
    fn remove_rider(&self, _me: &ObjRef, b: &ObjRef) {
        let cur = self.rider();
        if cur.as_ref().map_or(false, |c| ptr_eq(c, b)) {
            self.set_update_needs_to_be_sent(true); // !isClient
            self.st.borrow_mut().rider = None;
            self.notify_changed();
        }
        let has = self.rider().is_some();
        self.st.borrow_mut().being_controlled = has;
        if !has {
            self.zero_speeds();
        }
    }
    /// `-swipeUpGesture`: no-op.
    fn swipe_up_gesture(&self, _me: &ObjRef) {}
    /// `-blockheadCanRide:usingItem:`
    fn blockhead_can_ride_using_item(&self, _b: &ObjRef, _item: i32) -> bool {
        !self.being_controlled() && !self.base.borrow().needs_removed
    }
    /// `-jumpsOnSwipe`
    fn jumps_on_swipe(&self) -> bool {
        false
    }
    /// `-requiresFuel`
    fn requires_fuel(&self) -> bool {
        false
    }
    /// `-actsAsInteractionObject`
    fn acts_as_interaction_object(&self) -> Option<ObjRef> {
        None
    }
    /// `-riderDPadShouldAllowUpDown`
    fn rider_d_pad_should_allow_up_down(&self) -> bool {
        false
    }
    /// `-riderDPadShouldGiveDiscreteValues`
    fn rider_d_pad_should_give_discrete_values(&self) -> bool {
        false
    }
}

impl TapChoiceDynamicObject for Boat {
    /// `-actionTitle`
    fn action_title(&self) -> String {
        "EMBARK".to_string()
    }
    /// `-isDoubleHeight`
    fn is_double_height(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::blockhead::blockhead::{set_host, Blockhead, BlockheadHost};
    use crate::dynworld::state::{DwHooks, FreeBlockInit};
    use crate::lighting::artificial_light::testkit::tile;
    use crate::physics_objects::free_block::testkit::flat;

    fn bm(o: &ObjRef) -> &Boat {
        as_boat(o).unwrap()
    }

    fn blockhead(id: u64, is_net: bool) -> ObjRef {
        let b = alloc_obj(|w| Blockhead::alloc(w));
        b.base.borrow_mut().unique_id = id;
        b.base.borrow_mut().is_net = is_net;
        b
    }

    /// Host whose `stopRiding` does what says: call back `removeRider:` and clear `rideObject`.
    struct RideHost;
    impl crate::blockhead::blockhead::BlockheadHostStub for RideHost {
        fn stop_riding(&self, b: &Blockhead) {
            let ro = b.bh.borrow().ride_object.clone();
            if let Some(ro) = ro {
                let me: ObjRef = b.me();
                ro.as_ridable().unwrap().remove_rider(&ro, &me);
                b.bh.borrow_mut().ride_object = None;
            }
        }
    }

    struct FbHook(RefCell<Vec<(i32, bool, bool, i32)>>);
    impl crate::dynworld::state::DwHooksStub for FbHook {
        fn freeblock_from_position(&self, init: FreeBlockInit<'_>) -> Option<ObjRef> {
            self.0.borrow_mut().push((
                init.item_type as i32,
                init.dynamic_object_save_dict.is_some(),
                init.priority_blockhead.is_some(),
                init.position.y,
            ));
            None
        }
    }

    fn water_column(w: &Rc<World>, x: i32, y0: i32, y1: i32) {
        for y in y0..=y1 {
            tile(w, x, y).update(|t| {
                t.type_index = 3;
                t.partial_content_left = 255;
            });
        }
    }

    #[test]
    fn constants_and_class() {
        let (w, dw, _r) = flat("boat_const", 5);
        let b = Boat::new(&w, &dw, IntPair { x: 10, y: 7 }, None, None, Some("cid")).unwrap();
        assert_eq!(b.object_type(), 32);
        assert_eq!(b.freeblock_creation_item_type(), 111);
        assert_eq!(b.owner_id().as_deref(), Some("cid"));
        assert_eq!(b.float_pos().y, 7.5f32);
        assert_eq!(bm(&b).st.borrow().body_rotation, 3.14159274f32);
        assert_eq!(bm(&b).st.borrow().saved_blockhead_index, 0);
        assert!(
            b.class_info().conforms_to("RidableDynamicObject") && b.class_info().conforms_to("TapChoiceDynamicObject")
        );
        assert_eq!(b.as_tap_choice().unwrap().action_title(), "EMBARK");
        assert_eq!(bm(&b).max_health(), 32);
        assert_eq!(b.render_pos().y, 7.5f32 + (-0.3f32));
        let rp = b.as_ridable().unwrap().rider_pos_for_blockhead(&b);
        assert_eq!(rp[2], -0.75);
        assert_eq!(rp[1], ((7.5f32 as f64) + (-0.4f64) + (-0.5f64)) as f32);
        assert!(bm(&b).tap_is_within_body_radius(Vector2 { x: 10.5, y: 7.9 }));
        assert!(!bm(&b).tap_is_within_body_radius(Vector2 { x: 11.1, y: 7.9 }));
        // placement without a clientID reads ownerID from the dict
        let mut sd = new_mutable_dict();
        sd.map.insert("ownerID".to_string(), PlistValue::string("zed"));
        let b2 = Boat::new(&w, &dw, IntPair { x: 12, y: 7 }, None, Some(&sd), None).unwrap();
        assert_eq!(b2.owner_id().as_deref(), Some("zed"));
    }

    /// item 13: a boat in a water column with a half-full top tile bobs around the surface and is never crushed.
    #[test]
    fn floats_in_water() {
        let (w, dw, _r) = flat("boat_float", 5);
        water_column(&w, 10, 6, 9);
        tile(&w, 10, 9).update(|t| t.partial_content_left = 128);
        let b = Boat::new(&w, &dw, IntPair { x: 10, y: 7 }, None, None, Some("c")).unwrap();
        let (mut lo, mut hi) = (f32::MAX, f32::MIN);
        for i in 0..600 {
            b.update_accurate_dt_is_simulation(&b, 0.05, 0.05, false);
            assert!(!b.needs_removed());
            if i >= 300 {
                let y = b.float_pos().y;
                lo = lo.min(y);
                hi = hi.max(y);
            }
        }
        assert!(lo >= 8.0 && hi <= 11.0, "lo={lo} hi={hi}");
        // dt is clamped to 0.05
        let before = b.float_pos();
        b.update_accurate_dt_is_simulation(&b, 5.0, 5.0, false);
        assert!((b.float_pos().y - before.y).abs() < 0.5);
    }

    /// item 13: a boat over air falls, lands on solid ground and is converted to a free block with the save dict.
    #[test]
    fn crushed_boat_becomes_item() {
        let (w, dw, _r) = flat("boat_crush", 5);
        let hook = Rc::new(FbHook(RefCell::new(vec![])));
        dw.set_hooks(hook.clone());
        let b = Boat::new(&w, &dw, IntPair { x: 10, y: 9 }, None, None, Some("owner")).unwrap();
        let mut n = 0;
        while !b.needs_removed() && n < 2000 {
            b.update_accurate_dt_is_simulation(&b, 0.05, 0.05, false);
            n += 1;
        }
        assert!(b.needs_removed(), "n={n}");
        assert!(b.update_needs_to_be_sent());
        let log = hook.0.borrow();
        assert_eq!(log.len(), 1);
        assert_eq!(log[0].0, 111);
        assert!(log[0].1);
        assert!(!log[0].2);
    }

    /// Save dict keys; `currentBlockheadIndex` only for a local rider; load round trip.
    #[test]
    fn save_and_net_blobs() {
        let (w, dw, _r) = flat("boat_save", 5);
        let b = Boat::new(&w, &dw, IntPair { x: 10, y: 7 }, None, None, Some("own")).unwrap();
        let rider = blockhead(5, true);
        bm(&b).add_rider_opt(&b, Some(&rider));
        let d = b.get_save_dict(&b);
        assert!(d.map.get(&"currentBlockheadIndex".to_string()).is_none());
        assert_eq!(d.map.get(&"ownerID".to_string()).unwrap().as_str(), Some("own"));
        // a local rider is found by index
        dw.blockheads.borrow_mut().push(blockhead(1, false));
        let local = blockhead(2, false);
        dw.blockheads.borrow_mut().push(local.clone());
        bm(&b).st.borrow_mut().rider = Some(local);
        let d2 = b.get_save_dict(&b);
        assert_eq!(d2.map.get(&"currentBlockheadIndex".to_string()).unwrap().int_value(), 1);
        let l = Boat::from_save_dict(&w, &dw, &d2, None).unwrap();
        assert_eq!(bm(&l).st.borrow().saved_blockhead_index, 1);
        assert_eq!(l.owner_id().as_deref(), Some("own"));
        let l0 = Boat::from_save_dict(&w, &dw, &d, None).unwrap();
        assert_eq!(bm(&l0).st.borrow().saved_blockhead_index, -1);
        // creation blob: 0x28 struct + gzip(plist), padding zero
        bm(&b).st.borrow_mut().actual_x_speed = 1.0;
        let c = b.creation_net_data_for_client(&b, None).unwrap();
        assert!(c.len() > 0x28);
        assert!(c[0x22..0x28].iter().all(|x| *x == 0));
        assert_eq!(i16::from_le_bytes([c[0x20], c[0x21]]), 255);
        assert_eq!(c[0x28], 0x1f);
        let inflated = gzip_inflate(&c[0x28..]).unwrap();
        let plist = crate::plist::helpers::property_list_from_data(&inflated).unwrap();
        assert_eq!(plist.object_for_key("ownerID").unwrap().as_str(), Some("own"));
        assert_eq!(b.update_net_data_for_client(&b, None).unwrap(), c);
    }

    /// item 13: a net client boards via BoatCreationNetData; remote goal; teleport; leaving resets.
    #[test]
    fn remote_update_ride_and_leave() {
        let (w, dw, _r) = flat("boat_remote", 5);
        set_host(Rc::new(RideHost));
        let net_rider = blockhead(77, true);
        dw.net_blockheads.borrow_mut().push(net_rider.clone());
        let b = Boat::new(&w, &dw, IntPair { x: 10, y: 7 }, None, None, Some("o")).unwrap();
        let mut nd = BoatCreationNetData::default();
        nd.dynamic_object_net_data.unique_id = b.unique_id();
        nd.dynamic_object_net_data.pos_x = 12;
        nd.dynamic_object_net_data.pos_y = 7;
        nd.rider_id = 77;
        nd.actual_x_speed = 510;
        b.remote_update(&b, &nd.to_vec());
        assert!(bm(&b).being_controlled());
        assert!(bm(&b).rider().is_some());
        assert_eq!(bm(&b).actual_x_speed(), 2.0);
        assert_eq!(bm(&b).st.borrow().remote_goal_pos, IntPair { x: f32_to_i32(12.0 + 0.2 * 2.0), y: 7 });
        assert!(b.update_needs_to_be_sent());
        // the same rider again returns early
        b.remote_update(&b, &nd.to_vec());
        // leaving
        nd.rider_id = 0;
        b.remote_update(&b, &nd.to_vec());
        assert!(!bm(&b).being_controlled());
        assert!(bm(&b).rider().is_none());
        assert_eq!(bm(&b).actual_x_speed(), 0.0);
        // net boat from net data: is_net, goal computed, owner from the gzip plist
        let mut nd2 = nd;
        nd2.dynamic_object_net_data.unique_id = u64::MAX;
        nd2.dynamic_object_net_data.pos_x = 40;
        nd2.dynamic_object_net_data.pos_y = 8;
        nd2.rider_id = 0;
        nd2.actual_x_speed = 0;
        let own = {
            let mut d = new_mutable_dict();
            d.map.insert("ownerID".to_string(), PlistValue::string("netowner"));
            let xml = data_from_property_list(&PlistValue::Dict(d)).unwrap();
            gzip_deflate(&xml)
        };
        let mut blob = nd2.to_vec();
        blob.extend_from_slice(&own);
        let nb = Boat::from_net_data(&w, &dw, None, &blob).unwrap();
        assert!(nb.is_net());
        assert_eq!(nb.owner_id().as_deref(), Some("netowner"));
        assert_eq!(nb.pos(), IntPair { x: 40, y: 8 });
        assert_eq!(nb.float_pos().y, 8.5);
        assert_eq!(bm(&nb).st.borrow().remote_goal_pos, IntPair { x: 40, y: 8 });
        assert!(nb.update_needs_to_be_sent());
        // teleport: goal farther than 8 tiles
        bm(&nb).st.borrow_mut().remote_goal_pos = IntPair { x: 60, y: 8 };
        nb.update_accurate_dt_is_simulation(&nb, 0.05, 0.05, false);
        assert_eq!(nb.pos().x, 60);
        assert_eq!(nb.float_pos().x, 60.5);
        set_host(Rc::new(crate::blockhead::blockhead::NullHost));
    }

    /// `set_needs_removed` makes the rider stop riding exactly once; speeds reset through `remove_rider`.
    #[test]
    fn needs_removed_stops_rider() {
        let (w, dw, _r) = flat("boat_nr", 5);
        set_host(Rc::new(RideHost));
        let b = Boat::new(&w, &dw, IntPair { x: 10, y: 7 }, None, None, Some("o")).unwrap();
        let rider = blockhead(3, true);
        bm(&b).add_rider_opt(&b, Some(&rider));
        crate::blockhead::blockhead::as_blockhead(&rider).unwrap().bh.borrow_mut().ride_object = Some(b.clone());
        bm(&b).st.borrow_mut().actual_x_speed = 3.0;
        assert!(!bm(&b).as_ridable_ok(&rider));
        b.set_needs_removed(&b, true);
        assert!(bm(&b).rider().is_none());
        assert!(!bm(&b).being_controlled());
        assert_eq!(bm(&b).actual_x_speed(), 0.0);
        set_host(Rc::new(crate::blockhead::blockhead::NullHost));
    }

    impl Boat {
        fn as_ridable_ok(&self, rider: &ObjRef) -> bool {
            self.blockhead_can_ride_using_item(rider, 111)
        }
    }

    #[test]
    fn ride_direction_and_target_velocity() {
        let (w, dw, _r) = flat("boat_dir", 5);
        let b = Boat::new(&w, &dw, IntPair { x: 10, y: 7 }, None, None, Some("o")).unwrap();
        let r = b.as_ridable().unwrap();
        assert_eq!(r.ride_direction(), 0);
        let rider = blockhead(3, true);
        bm(&b).add_rider_opt(&b, Some(&rider));
        r.set_target_velocity(Vector2 { x: 1.0, y: 0.0 });
        assert_eq!(bm(&b).target_x_speed(), 6.0);
        assert_eq!(r.ride_direction(), 1);
        r.set_target_velocity(Vector2 { x: -1.0, y: 0.0 });
        assert_eq!(r.ride_direction(), -1);
        r.set_target_velocity(Vector2 { x: -0.3333333, y: 0.0 });
        assert_eq!(r.ride_direction(), 0);
        // blockhead unloaded: no stopRiding, speeds reset
        b.blockhead_unloaded(&b, &rider);
        assert!(bm(&b).rider().is_none());
        assert_eq!(bm(&b).target_x_speed(), 0.0);
    }
}
