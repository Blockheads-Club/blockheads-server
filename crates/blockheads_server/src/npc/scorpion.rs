//! `Scorpion: NPC`: the desert walker with a bite and a poison spit (NPC type 7, dynamic object type 51).
//!
//! " (feed / capture / rider methods).
//!
//! RNG: `loadDerivedStuff` 2 draws; `hitWithForce` 1; dead drop 1 (integer compare);
//! wander 2 to 4; birth scheduling 1. `getSaveDict` adds no keys: AI state is rebuilt by `loadDerivedStuff`
//! (new `rand()` draws on every load).

use std::any::Any;
use std::cell::RefCell;
use std::rc::Rc;

use crate::fmath::cconv::f32_to_i32;
use crate::fmath::libm::{fabsf, sinf};
use crate::fmath::vector::linear_interpolate;
use crate::impl_npc_protocols;
use crate::npc::cave_troll::{
    hostile_host, is_same, r01, scale_damage_for_difficulty, set_float_pos, snap_float_pos, tile_loaded, tile_unloaded,
    world_time, world_width_macro, wrap_dx, CUSTOM_RULE_NPC_EASY, CUSTOM_RULE_NPC_HARD, CUSTOM_RULE_NPC_PASSIVE,
    ITEM_CAGE,
};
use crate::npc::npc::{
    alloc_npc, npc_add_rider, npc_blockhead_unloaded, npc_die, npc_env, npc_hit_with_force_blockhead,
    npc_init_with_world_dynamic_world_at_position_cache_save_dict_is_adult_was_placed_placed_by_client,
    npc_init_with_world_dynamic_world_cache_net_data, npc_init_with_world_dynamic_world_save_dict_cache,
    npc_remote_creation_data_update, npc_remote_update, npc_remove_rider_opt, npc_update_accurate_dt_is_simulation,
    register_npc_ctor, Npc, NpcState, NPC_CLASS, NPC_SCORPION,
};
use crate::npc_forward;
use crate::objects::base::{padded_bytes, DynamicObject, DynamicObjectBase, DynamicWorldRef, WorldRef};
use crate::objects::class_info::ClassInfo;
use crate::objects::handle::{ptr_eq, CacheRef, ObjRef};
use crate::plist::value::PlistDict;
use crate::types::netdata::{ScorpionCreationNetData, ScorpionUpdateNetData};
use crate::types::structs::{make_intpair, IntPair, Vector2, WireStruct};
use crate::world::helpers::{tile_contains_gate, tile_is_air_or_snow, tile_is_solid, tile_is_water};
use crate::world::tiles::clear_line_of_sight_between_tiles;

/// `Scorpion: NPC`.
pub static SCORPION_CLASS: ClassInfo = ClassInfo { name: "Scorpion", superclass: Some(&NPC_CLASS), protocols: &[] };

/// `SCORPION_NAMES` (22 entries, "Herculies" spelling kept).
pub const SCORPION_NAMES: [&str; 22] = [
    "Muffet",
    "Spindra",
    "Aragog",
    "Fluffy",
    "Nightmare",
    "Sparkles",
    "Diablo",
    "Legs",
    "Pincers",
    "Claws",
    "Aladin",
    "Bandit",
    "Basil",
    "Herculies",
    "Nero",
    "Ringo",
    "Spike",
    "Snuggles",
    "Bubbles",
    "Venom",
    "Pinchy",
    "Scorpio",
];

/// `ITEM_PRICKLY_PEAR`
pub const ITEM_PRICKLY_PEAR: i32 = 24;
/// `ITEM_SCORPION_CAGE`
pub const ITEM_SCORPION_CAGE: i32 = 327;
/// `ITEM_POISON`
pub const ITEM_POISON: i32 = 255;

const PI_D: f64 = 3.141592653589793;

/// The own ivars of `Scorpion` that are sent or read by `update:` (client animation omitted).
#[derive(Clone)]
pub struct ScorpionData {
    pub movement_direction: i32,
    pub from_square: IntPair,
    pub to_square: IntPair,
    pub travel_fraction: f32,
    pub random_time_between_direction_changes: f32,
    /// client; kept 0
    pub body_rotation: f32,
    pub random_goal_rotation: f32,
    pub walk_timer: f32,
    pub walk_speed: f32,
    pub drown_timer: f32,
    pub fall_speed: f32,
    pub z_rotation: f32,
    pub attack_blockhead: Option<ObjRef>,
    pub attack_delay: f32,
    pub biting: bool,
    pub bite_timer: f32,
    pub spitting: bool,
    pub spit_timer: f32,
    pub provoked: bool,
    /// a single f32 (`setTargetVelocity:` keeps `v.x` only)
    pub rider_target_velocity: f32,
    pub remote_goal_square: IntPair,
    pub remote_walk_speed: f32,
    pub remote_movement_direction: i32,
}

impl ScorpionData {
    /// Zeroed ivars after `alloc`.
    pub fn new() -> ScorpionData {
        let z = IntPair { x: 0, y: 0 };
        ScorpionData {
            movement_direction: 0,
            from_square: z,
            to_square: z,
            travel_fraction: 0.0,
            random_time_between_direction_changes: 0.0,
            body_rotation: 0.0,
            random_goal_rotation: 0.0,
            walk_timer: 0.0,
            walk_speed: 0.0,
            drown_timer: 0.0,
            fall_speed: 0.0,
            z_rotation: 0.0,
            attack_blockhead: None,
            attack_delay: 0.0,
            biting: false,
            bite_timer: 0.0,
            spitting: false,
            spit_timer: 0.0,
            provoked: false,
            rider_target_velocity: 0.0,
            remote_goal_square: z,
            remote_walk_speed: 0.0,
            remote_movement_direction: 0,
        }
    }
}

/// `Scorpion`.
pub struct Scorpion {
    base: RefCell<DynamicObjectBase>,
    npc: RefCell<NpcState>,
    sc: RefCell<ScorpionData>,
}

/// `movementDirection` from one draw `r`: `(double)r <= 0.75 ? ((double)r < 0.25 ? -1: 0): 1`.
pub fn direction_from_roll(r: f32) -> i32 {
    if (r as f64) <= 0.75f64 {
        if (r as f64) < 0.25f64 {
            -1
        } else {
            0
        }
    } else {
        1
    }
}

impl Scorpion {
    fn alloc() -> Rc<Scorpion> {
        alloc_npc(|w| Scorpion {
            base: RefCell::new(DynamicObjectBase::alloc_zeroed_with(w)),
            npc: RefCell::new(NpcState::default()),
            sc: RefCell::new(ScorpionData::new()),
        })
    }

    /// Read access to the scorpion ivars.
    pub fn data(&self) -> std::cell::Ref<'_, ScorpionData> {
        self.sc.borrow()
    }

    /// Write access to the scorpion ivars (callers must not hold it across calls into other objects).
    pub fn data_mut(&self) -> std::cell::RefMut<'_, ScorpionData> {
        self.sc.borrow_mut()
    }

    fn g<R>(&self, f: impl FnOnce(&ScorpionData) -> R) -> R {
        f(&self.sc.borrow())
    }

    fn s(&self, f: impl FnOnce(&mut ScorpionData)) {
        f(&mut self.sc.borrow_mut())
    }

    // ------------------------------------------------------------------------------------ initialisers

    /// `-initWithWorld:dynamicWorld:atPosition:cache:saveDict:isAdult:wasPlaced:placedByClient:`.
    #[allow(clippy::too_many_arguments)]
    pub fn init_with_world_dynamic_world_at_position_cache_save_dict_is_adult_was_placed_placed_by_client(
        world: &WorldRef,
        dw: &DynamicWorldRef,
        pos: IntPair,
        cache: CacheRef,
        save_dict: Option<&PlistDict>,
        is_adult: bool,
        was_placed: bool,
        placed_by_client: Option<&str>,
    ) -> Option<Rc<Scorpion>> {
        let o = Scorpion::alloc();
        let me: ObjRef = o.clone();
        let inited = npc_init_with_world_dynamic_world_at_position_cache_save_dict_is_adult_was_placed_placed_by_client(
            &me,
            world,
            dw,
            pos,
            cache,
            save_dict,
            is_adult,
            was_placed,
            placed_by_client,
        );
        debug_assert!(inited); // the base initialisers cannot fail on this path
        o.load_derived_stuff();
        Some(o)
    }

    /// `-initWithWorld:dynamicWorld:saveDict:cache:`.
    pub fn init_with_world_dynamic_world_save_dict_cache(
        world: &WorldRef,
        dw: &DynamicWorldRef,
        save_dict: &PlistDict,
        cache: CacheRef,
    ) -> Option<Rc<Scorpion>> {
        let o = Scorpion::alloc();
        let me: ObjRef = o.clone();
        let inited = npc_init_with_world_dynamic_world_save_dict_cache(&me, world, dw, save_dict, cache);
        debug_assert!(inited); // the base initialisers cannot fail on this path
        o.load_derived_stuff();
        Some(o)
    }

    /// `-initWithWorld:dynamicWorld:cache:netData:`: the client replica path.
    pub fn init_with_world_dynamic_world_cache_net_data(
        world: &WorldRef,
        dw: &DynamicWorldRef,
        cache: CacheRef,
        net_data: &[u8],
    ) -> Option<Rc<Scorpion>> {
        let o = Scorpion::alloc();
        let me: ObjRef = o.clone();
        if !npc_init_with_world_dynamic_world_cache_net_data(&me, world, dw, cache, net_data) {
            return None;
        }
        o.load_derived_stuff();
        let pos = me.pos();
        o.s(|s| {
            s.remote_goal_square = pos;
            s.remote_walk_speed = 0.0f32;
            s.remote_movement_direction = 0;
        });
        o.remote_creation_data_update(&me, net_data);
        Some(o)
    }

    /// `-loadDerivedStuff`: two `rand()` draws (direction, then speed).
    pub fn load_derived_stuff(&self) {
        let r = r01("Scorpion.loadDerivedStuff.direction");
        let dir = direction_from_roll(r);
        let pos = self.base.borrow().pos;
        let speed = (r01("Scorpion.loadDerivedStuff.speed") + 0.25f32) * 2.0f32;
        self.s(|s| {
            s.movement_direction = dir;
            s.to_square = pos;
            s.from_square = s.to_square;
            s.travel_fraction = 1.0f32;
            s.random_time_between_direction_changes = 0.0f32;
            s.walk_speed = speed;
        });
    }

    // ------------------------------------------------------------------------------------ control / net

    /// `-isUnderLocalControl`: `isNet ? (rider != nil && !rider.isNet): !(rider != nil && rider.isNet)`.
    pub fn is_under_local_control(&self) -> bool {
        let rider = self.npc.borrow().rider.clone();
        if self.base.borrow().is_net {
            rider.as_ref().map_or(false, |r| !r.is_net())
        } else {
            !rider.as_ref().map_or(false, |r| r.is_net())
        }
    }

    /// `-scorpionUpdateNetDataForClient:`. `clearQueue` and the padding are written as zero.
    pub fn scorpion_update_net_data_for_client(&self, me: &ObjRef, client_id: Option<&str>) -> ScorpionUpdateNetData {
        let npc_upd = Npc::npc_update_net_data_for_client(self, me, client_id);
        let s = self.sc.borrow();
        let mut u = ScorpionUpdateNetData::default();
        u.npc_update_net_data = npc_upd;
        u.to_square_x = s.to_square.x as u32;
        u.to_square_y = s.to_square.y as u32;
        u.walk_speed = f32_to_i32(s.walk_speed * 255.0f32) as i16;
        u.movement_direction = s.movement_direction as i8;
        u
    }

    /// `-doRemoteUpdate:`: the server branch accepts the controlling client's position / speed / direction
    /// only while a remote rider is on the scorpion.
    pub fn do_remote_update(&self, me: &ObjRef, u: &ScorpionUpdateNetData) {
        let rider = self.npc.borrow().rider.clone();
        let ws = (u.walk_speed as i32 as f32) / 255.0f32;
        let goal = make_intpair(u.to_square_x as i32, u.to_square_y as i32);
        if !npc_env().is_client() {
            if rider.as_ref().map_or(false, |r| r.is_net()) {
                self.s(|s| {
                    s.remote_goal_square = goal;
                    s.remote_walk_speed = ws;
                    s.walk_speed = ws;
                    s.remote_movement_direction = u.movement_direction as i32;
                    s.movement_direction = u.movement_direction as i32;
                });
                me.set_update_needs_to_be_sent(true);
            }
        } else if !rider.as_ref().map_or(false, |r| !r.is_net()) {
            let att = self.g(|s| s.attack_blockhead.clone());
            if att.as_ref().map_or(false, |a| !a.is_net()) {
                return;
            }
            self.s(|s| {
                s.remote_goal_square = goal;
                s.remote_walk_speed = ws;
                s.remote_movement_direction = u.movement_direction as i32;
            });
        }
    }

    /// `-tapIsWithinBodyRadius:`: client UI hit test, ported for completeness.
    pub fn tap_is_within_body_radius_impl(&self, tap: Vector2) -> bool {
        if self.npc.borrow().dead {
            return false;
        }
        let fp = self.base.borrow().float_pos;
        let wwm = self.base.borrow().world.as_ref().and_then(|w| w.upgrade()).map_or(0, |w| w.world_width_macro());
        let half = ((wwm.wrapping_mul(32)) / 2) as f32;
        let dx = tap.x - fp.x;
        let dxw = if dx < half {
            if ((wwm.wrapping_mul(-32)) / 2) as f32 <= dx {
                dx
            } else {
                dx + (wwm.wrapping_mul(32)) as f32
            }
        } else {
            dx - (wwm.wrapping_mul(32)) as f32
        };
        if (dxw as f64) < 0.5f64 && -0.5f64 < (dxw as f64) {
            return (fp.y as f64) < (tap.y as f64) && (tap.y as f64) < (fp.y as f64) + 1.0f64;
        }
        false
    }
}

// =====================================================================================================
// update:accurateDT:isSimulation:
// =====================================================================================================

impl Scorpion {
    /// `-update:accurateDT:isSimulation:`. Steps 0 to 11 are marked in the body.
    fn update_impl(&self, me: &ObjRef, dt: f32, accurate_dt: f32, is_sim: bool) {
        // ---- 0. prologue
        npc_update_accurate_dt_is_simulation(me, dt, accurate_dt, is_sim);
        if me.needs_removed() {
            return;
        }
        let host = hostile_host();
        let is_net = me.is_net();

        // ---- 1. dead branch
        if self.npc.borrow().dead {
            let tf = self.g(|s| s.travel_fraction) + 4.0f32 * dt;
            self.s(|s| s.travel_fraction = tf);
            if is_net {
                return;
            }
            if tf < 1.0f32 {
                return;
            }
            let n = if crate::rng::glibc_rand::rand_tagged("Scorpion.update.drop") > 0x3fffffff { 2 } else { 1 };
            let kb = self.npc.borrow().kill_blockhead.clone();
            let pos = me.pos();
            for _ in 0..n {
                host.create_free_block(pos, ITEM_POISON, kb.as_ref());
            }
            me.set_needs_removed(me, true);
            return;
        }

        // ---- 2. effective walk speed
        let rider = self.npc.borrow().rider.clone();
        let rider_net = rider.as_ref().map_or(false, |r| r.is_net());
        let rider_local = rider.is_some() && !rider_net;
        let mut walk_speed_to_use = self.g(|s| s.walk_speed);
        let (fs, ts) = self.g(|s| (s.from_square, s.to_square));
        if fs.y != ts.y {
            walk_speed_to_use = 2.0f32;
            if ts.y < fs.y && fs.x == ts.x {
                let fall = self.sc.borrow_mut().fall_accel(dt);
                walk_speed_to_use = fall + 2.0f32;
            } else {
                self.s(|s| s.fall_speed = 0.0f32);
            }
            let pos = me.pos();
            let tile = tile_loaded(pos.x, pos.y);
            if tile_is_water(tile) {
                walk_speed_to_use *= 0.3f32;
            }
        }
        if is_net && !rider_local {
            let rws = self.g(|s| s.remote_walk_speed);
            walk_speed_to_use = ((walk_speed_to_use as f64) * 0.75f64 + 0.25f64 * (rws as f64)) as f32;
        }
        if rider.is_some() && rider_net {
            walk_speed_to_use = self.g(|s| s.remote_walk_speed);
        }
        self.s(|s| {
            s.random_time_between_direction_changes -= dt;
            s.travel_fraction += dt * walk_speed_to_use;
            s.attack_delay -= dt;
        });

        // ---- 3. local rider control
        if rider_local {
            self.apply_rider_control();
        }

        // ---- 4. attack-target validity
        {
            let att = self.g(|s| s.attack_blockhead.clone());
            if let Some(b) = att {
                if host.bh_regenerating(&b) || b.needs_removed() || host.bh_on_trade_mission(&b) {
                    self.s(|s| {
                        s.attack_blockhead = None;
                        s.biting = false;
                        s.spitting = false;
                        s.provoked = false;
                        s.bite_timer = 0.0f32;
                        s.spit_timer = 0.0f32;
                    });
                }
            }
        }

        // ---- 5. aggression flag
        let pos0 = me.pos();
        let f = if me.world().is_some() { host.day_night_fraction(pos0.x as f32, world_time(me)) } else { 0.0f32 };
        let night = f < 0.23f32;
        let mut aggro = night;
        if !night {
            let cr = host.custom_rules();
            aggro = if cr.custom_rules_enabled != 0 { cr.npc_difficulty == CUSTOM_RULE_NPC_HARD } else { false };
        }

        // ---- 6. walk animation timer
        let mut not_moving = self.g(|s| s.movement_direction) == 0;
        if !not_moving {
            self.s(|s| s.walk_timer += dt * walk_speed_to_use * 2.0f32);
        }

        // ---- 7. movement decision
        let wwm = world_width_macro(me);
        let inspecting = self.npc.borrow().inspecting_blockhead.as_ref().and_then(|w| w.upgrade()).is_some();
        if !inspecting {
            let mut approached = false;
            let coming = self.npc.borrow().coming_to_inspect_blockhead.as_ref().and_then(|w| w.upgrade());
            if let Some(ci) = coming {
                let dx = wrap_dx(ci.pos().x, pos0.x, wwm);
                if dx.wrapping_abs() < 20 {
                    if dx < 2 {
                        if dx < -1 {
                            self.s(|s| {
                                s.movement_direction = -1;
                                s.walk_speed = 1.5f32;
                            });
                        }
                    } else {
                        self.s(|s| {
                            s.movement_direction = 1;
                            s.walk_speed = 1.5f32;
                        });
                    }
                    approached = true;
                }
            }
            if !approached {
                self.wander(me, not_moving, &host);
            }
        } else {
            self.s(|s| {
                s.movement_direction = 0;
                s.walk_speed = 0.0f32;
            });
        }
        not_moving = self.g(|s| s.movement_direction) == 0;

        // ---- 8. non-replica housekeeping
        if !is_net {
            let pos = me.pos();
            let tile1 = tile_loaded(pos.x, pos.y);
            let lay = self.npc.borrow().lay_timer;
            if lay > 0.0f32 {
                let after = {
                    let mut n = self.npc.borrow_mut();
                    n.lay_timer -= dt;
                    n.lay_timer
                };
                if after <= 0.0f32 {
                    if let Some(t) = tile1 {
                        if t.get().foreground_contents == 0 && !host.tutorial_active() {
                            {
                                let mut n = self.npc.borrow_mut();
                                n.fullness = ((n.fullness as f64) - 2700.0f64) as f32;
                            }
                            host.load_baby_npc_at_position(pos, NPC_SCORPION);
                            self.npc.borrow_mut().lay_cooldown_timer = 900.0f32;
                            if let Some(dw) = me.dynamic_world() {
                                dw.dynamic_world_changed_at_pos_object_type(pos, me.object_type());
                            }
                        }
                    }
                }
            }
            let deep = tile_is_water(tile1) && tile1.map_or(0, |t| t.get().partial_content_left) >= 0x80;
            if !deep {
                self.s(|s| {
                    s.drown_timer = ((s.drown_timer as f64) - (dt as f64) * 0.5f64) as f32;
                    if s.drown_timer < 0.0f32 {
                        s.drown_timer = 0.0f32;
                    }
                });
            } else {
                let d = self.sc.borrow_mut().add_drown(dt);
                if d > 4.0f32 {
                    Npc::die(self, me, None);
                }
            }
        }

        // ---- 9. step completion
        if self.g(|s| s.travel_fraction) >= 1.0f32 {
            self.complete_step(me, &host, rider_local, &mut not_moving, is_net);
        }

        // ---- 10. attacks (only under local control)
        if self.is_under_local_control() {
            self.attacks(me, dt, is_sim, aggro, rider_local, &host);
        }

        // ---- 11. visual interpolation of floatPos / tilt
        let (fs, ts, tf) = self.g(|s| (s.from_square, s.to_square, s.travel_fraction));
        if fs != ts {
            let from_pos = Vector2 { x: (fs.x as f32) + 0.5f32, y: fs.y as f32 };
            let to_pos = Vector2 { x: (ts.x as f32) + 0.5f32, y: ts.y as f32 };
            let x_lerp = tf;
            let y_lerp = tf;
            let z = if (ts.y < fs.y && fs.x < ts.x) || (fs.y < ts.y && ts.x < fs.x) {
                sinf(((y_lerp as f64) * PI_D) as f32) * -0.9f32
            } else if (ts.y < fs.y && ts.x < fs.x) || (fs.y < ts.y && fs.x < ts.x) {
                sinf(((y_lerp as f64) * PI_D) as f32) * 0.9f32
            } else {
                0.0f32
            };
            self.s(|s| s.z_rotation = z);
            set_float_pos(
                me,
                linear_interpolate(from_pos.x, to_pos.x, x_lerp),
                linear_interpolate(from_pos.y, to_pos.y, y_lerp),
            );
        }
    }

    /// Step 3: the local rider steers through `riderTargetVelocity`.
    fn apply_rider_control(&self) {
        let v = self.g(|s| s.rider_target_velocity);
        self.s(|s| {
            if (v as f64) < 0.1f64 {
                if -0.1f64 < (v as f64) {
                    s.movement_direction = 0;
                    s.random_goal_rotation = 0.0f32;
                    s.walk_speed = 0.0f32;
                } else {
                    s.movement_direction = -1;
                    s.walk_speed = fabsf(3.0f32 * v);
                }
            } else {
                s.movement_direction = 1;
                s.walk_speed = 3.0f32 * v;
            }
        });
        let md = self.g(|s| s.movement_direction);
        if md != 0 {
            // `me` position is needed: read it from the base ivars
            let pos = self.base.borrow().pos;
            let d = md;
            let stop = |this: &Scorpion| {
                this.s(|s| {
                    s.movement_direction = 0;
                    s.walk_speed = 0.0f32;
                })
            };
            let right = tile_unloaded(pos.x + d, pos.y);
            if !tile_is_air_or_snow(right) {
                if !tile_is_solid(right) {
                    stop(self);
                } else {
                    let right_up = tile_unloaded(pos.x + d, pos.y + 1);
                    if !tile_is_air_or_snow(right_up) {
                        stop(self);
                    } else {
                        let up = tile_unloaded(pos.x, pos.y + 1);
                        if !tile_is_air_or_snow(up) {
                            stop(self);
                        }
                    }
                }
            } else {
                let right_down = tile_unloaded(pos.x + d, pos.y - 1);
                if tile_is_air_or_snow(right_down) {
                    let rdd = tile_unloaded(pos.x + d, pos.y - 2);
                    if !tile_is_solid(rdd) {
                        stop(self);
                    }
                }
            }
        }
    }

    /// Step 7 wander: draws #1 (direction timer), #2 (walk speed), #3 (new direction), #4 (goal rotation).
    fn wander(&self, me: &ObjRef, not_moving: bool, host: &Rc<dyn crate::npc::cave_troll::HostileHost>) {
        let is_net = me.is_net();
        let rider_none = self.npc.borrow().rider.is_none();
        let (rtbdc, tf) = self.g(|s| (s.random_time_between_direction_changes, s.travel_fraction));
        if !(rider_none && !is_net && rtbdc <= 0.0f32 && (tf >= 1.0f32 || not_moving)) {
            return;
        }
        let t = r01("Scorpion.update.wander.timer") * 2.0f32 + 1.0f32;
        self.s(|s| s.random_time_between_direction_changes = t);
        // the guard above returned for a replica: from here on `is_net` is false
        let r = r01("Scorpion.update.wander.speed");
        self.s(|s| s.walk_speed = ((s.walk_speed as f64) * 0.5f64 + (r as f64) + 0.25f64) as f32);
        me.set_update_needs_to_be_sent(true);
        let ws = self.g(|s| s.walk_speed);
        if ws < 1.5f32 || not_moving {
            let r = r01("Scorpion.update.wander.direction");
            let new_dir = direction_from_roll(r);
            if new_dir == 0 {
                let to = self.g(|s| s.to_square);
                let pos = me.pos();
                if !host.blockhead_occupies_tile_at_pos(to) && !host.npc_exists_at_pos_ignore_npc(pos, Some(me)) {
                    let g = r01("Scorpion.update.wander.rotation");
                    let rot = ((g as f64) * PI_D * 2.0f64 - PI_D) as f32;
                    self.s(|s| {
                        s.movement_direction = 0;
                        s.random_goal_rotation = rot;
                    });
                    me.set_update_needs_to_be_sent(true);
                }
            } else {
                let md = self.g(|s| s.movement_direction);
                if md != new_dir && (ws < 1.2f32 || not_moving) {
                    self.s(|s| s.movement_direction = new_dir);
                    me.set_update_needs_to_be_sent(true);
                }
            }
        }
    }

    /// Step 9: the current tile step finished.
    fn complete_step(
        &self,
        me: &ObjRef,
        host: &Rc<dyn crate::npc::cave_troll::HostileHost>,
        rider_local: bool,
        not_moving: &mut bool,
        is_net: bool,
    ) {
        self.s(|s| {
            s.travel_fraction -= 1.0f32;
            if s.travel_fraction >= 1.0f32 || s.travel_fraction < 0.0f32 {
                s.travel_fraction = 0.0;
            }
        });
        let (fs, ts) = self.g(|s| (s.from_square, s.to_square));
        if !is_net && fs.x != ts.x {
            let tile2 = tile_loaded(ts.x, ts.y);
            if let Some(t2) = tile2 {
                let t2c = t2.get();
                // eat cactus
                if (self.npc.borrow().fullness as f64) < 5400.0f64 && t2c.contents == 0x2b {
                    if let Some(avail) = host.cactus_available_food_at(ts) {
                        let take = if 900.0f32 <= avail { 900.0f32 } else { avail };
                        self.npc.borrow_mut().fullness += take;
                        host.set_cactus_available_food_at(ts, avail - take);
                    }
                }
                // schedule birth
                let (fullness, lcd, lt) = {
                    let n = self.npc.borrow();
                    (n.fullness, n.lay_cooldown_timer, n.lay_timer)
                };
                if 5400.0f64 < (fullness as f64) && lcd <= 0.0f32 && lt <= 0.0f32 {
                    let v = ((r01("Scorpion.update.lay") * 900.0f32) as f64 * 0.2f64 + 720.0f64) as f32;
                    self.npc.borrow_mut().lay_timer = v;
                    if let Some(dw) = me.dynamic_world() {
                        dw.dynamic_world_changed_at_pos_object_type(me.pos(), me.object_type());
                    }
                }
            }
        }
        self.s(|s| s.from_square = s.to_square);
        let to = self.g(|s| s.to_square);
        me.update_position(me, to);
        Npc::check_current_position_for_food(self, me);
        snap_float_pos(me, to);

        let mut teleport = false;
        let rider = self.npc.borrow().rider.clone();
        let rider_net = rider.as_ref().map_or(false, |r| r.is_net());
        let remote_driven = (is_net && !rider_local) || (rider.is_some() && rider_net);
        let pos = me.pos();
        if remote_driven {
            let (goal, rws, rmd) = self.g(|s| (s.remote_goal_square, s.remote_walk_speed, s.remote_movement_direction));
            let wwm = world_width_macro(me);
            let dx = wrap_dx(goal.x, pos.x, wwm);
            let max1 = |v: f32| if v <= 1.0f32 { 1.0f32 } else { v };
            if dx.wrapping_abs() < 9 && goal.y.wrapping_sub(pos.y).wrapping_abs() < 5 {
                if dx.wrapping_abs() < 3 {
                    if dx <= 0 {
                        if dx < 0 {
                            self.s(|s| {
                                s.movement_direction = -1;
                                s.walk_speed = max1(rws);
                            });
                        } else {
                            self.s(|s| {
                                s.walk_speed = rws;
                                s.movement_direction = 0;
                            });
                        }
                    } else {
                        self.s(|s| {
                            s.movement_direction = 1;
                            s.walk_speed = max1(rws);
                        });
                    }
                } else {
                    self.s(|s| {
                        s.walk_speed = 2.0f32;
                        s.movement_direction = if dx > 0 { 1 } else { -1 };
                    });
                }
            } else {
                teleport = true;
                self.s(|s| {
                    s.to_square = goal;
                    s.walk_speed = rws;
                    s.movement_direction = rmd;
                });
            }
            *not_moving = self.g(|s| s.movement_direction) == 0;
        }
        if !teleport {
            let ts = self.g(|s| s.to_square);
            let tile3 = tile_loaded(ts.x, ts.y);
            let below = tile_unloaded(pos.x, pos.y - 1);
            let set_to = |this: &Scorpion, sq: IntPair| {
                this.s(|s| s.to_square = sq);
                me.set_unreliable_update_needs_to_be_sent(true);
            };
            if !tile_is_solid(below) {
                if tile_is_water(tile3) {
                    set_to(self, IntPair { x: pos.x, y: pos.y + 1 });
                } else {
                    set_to(self, IntPair { x: pos.x, y: pos.y - 1 });
                }
            } else if !*not_moving {
                let d = self.g(|s| s.movement_direction);
                let right = tile_unloaded(pos.x + d, pos.y);
                if !tile_is_air_or_snow(right) || (rider.is_none() && tile_contains_gate(right)) {
                    if tile_is_solid(right) {
                        let right_up = tile_unloaded(pos.x + d, pos.y + 1);
                        if tile_is_air_or_snow(right_up) && (rider.is_some() || !tile_contains_gate(right_up)) {
                            let up = tile_unloaded(pos.x, pos.y + 1);
                            if tile_is_air_or_snow(up) {
                                set_to(self, IntPair { x: pos.x + d, y: pos.y + 1 });
                            }
                        }
                    }
                } else {
                    let right_down = tile_unloaded(pos.x + d, pos.y - 1);
                    if !tile_is_solid(right_down) {
                        if tile_is_air_or_snow(right_down) && (rider.is_some() || !tile_contains_gate(right_down)) {
                            let rdd = tile_unloaded(pos.x + d, pos.y - 2);
                            if tile_is_solid(rdd) {
                                set_to(self, IntPair { x: pos.x + d, y: pos.y - 1 });
                            }
                        }
                    } else {
                        set_to(self, IntPair { x: pos.x + d, y: pos.y });
                    }
                }
            }
            let (fs, ts) = self.g(|s| (s.from_square, s.to_square));
            if ts == fs {
                // blocked
                self.s(|s| s.movement_direction = -s.movement_direction);
                if self.is_under_local_control() {
                    me.set_update_needs_to_be_sent(true);
                }
            }
        }
    }

    /// Step 10: bite, spit, target search.
    fn attacks(
        &self,
        me: &ObjRef,
        dt: f32,
        is_sim: bool,
        aggro: bool,
        rider_local: bool,
        host: &Rc<dyn crate::npc::cave_troll::HostileHost>,
    ) {
        // 10a bite
        if self.g(|s| s.biting) {
            let bt = self.sc.borrow_mut().advance_bite(dt);
            let att = self.g(|s| s.attack_blockhead.clone());
            if bt > 0.5f32 && (bt - 2.0f32 * dt) <= 0.5f32 {
                if let Some(a) = &att {
                    let bp = a.pos();
                    let pos = me.pos();
                    if bp.y == pos.y && pos.x - 1 <= bp.x && bp.x <= pos.x + 1 {
                        let damage = self.npc.borrow().damage;
                        let ratio = (damage as f32) / (Npc::max_health(self) as f32);
                        let mut dmg = 0.1f32 * (1.0f32 - ratio);
                        if dmg <= 0.02f32 {
                            dmg = 0.02f32;
                        }
                        let cr = host.custom_rules();
                        dmg = scale_damage_for_difficulty(dmg, &cr);
                        host.bh_suffer_damage(a, dmg, is_sim, true);
                        if is_sim {
                            let s = if host.bh_regenerating(a) { "killed" } else { "attacked" };
                            host.add_damage_simulation_event(a, &format!("{} by a scorpion", s));
                        }
                    }
                }
            }
            if bt > 1.0f32 {
                self.s(|s| {
                    s.bite_timer = 0.0f32;
                    s.biting = false;
                    s.provoked = false;
                    s.attack_blockhead = None;
                    s.random_time_between_direction_changes = 0.0f32;
                    s.attack_delay = if aggro { 0.5f32 } else { 4.0f32 };
                });
                me.set_update_needs_to_be_sent(true);
            }
        }
        // 10b spit
        if self.g(|s| s.spitting) {
            let cr = host.custom_rules();
            let mult = scale_damage_for_difficulty(2.0f32, &cr);
            let st = self.sc.borrow_mut().advance_spit(dt * mult);
            let att = self.g(|s| s.attack_blockhead.clone());
            if st > 0.5f32 && (st - 2.0f32 * dt) <= 0.5f32 {
                if let Some(a) = &att {
                    let pos = me.pos();
                    let from =
                        Vector2 { x: 0.5f32 + (pos.x as f32), y: (((0.5f32 + (pos.y as f32)) as f64) + 0.5f64) as f32 };
                    let c = host.bh_center(a);
                    let to = Vector2 { x: c.x + 0.0f32, y: c.y + 0.5f32 };
                    host.fire_projectile(from, to, Some(a), ITEM_POISON, None);
                }
            }
            if st > 1.0f32 {
                self.s(|s| {
                    s.spit_timer = 0.0f32;
                    s.spitting = false;
                    s.provoked = false;
                    s.attack_blockhead = None;
                    s.random_time_between_direction_changes = 0.0f32;
                    s.attack_delay = if aggro { 0.5f32 } else { 4.0f32 };
                });
                me.set_update_needs_to_be_sent(true);
                let cr = host.custom_rules();
                if cr.custom_rules_enabled != 0 {
                    if cr.npc_difficulty == CUSTOM_RULE_NPC_EASY {
                        self.s(|s| s.attack_delay = 4.0f32 * s.attack_delay);
                    } else if cr.npc_difficulty == CUSTOM_RULE_NPC_HARD {
                        self.s(|s| s.attack_delay = 0.25f32 * s.attack_delay);
                    }
                }
            }
        }
        // 10c target search
        if self.g(|s| s.attack_delay) > 0.0f32 {
            return;
        }
        self.s(|s| s.attack_delay = 1.0f32);
        let blockheads = npc_env().all_blockheads_including_net();
        let mut closest: Option<ObjRef> = None;
        let mut closest_dist: f32 = 9999999.0f32;
        let cr = host.custom_rules();
        if !(cr.custom_rules_enabled != 0 && cr.npc_difficulty == CUSTOM_RULE_NPC_PASSIVE) {
            let pos = me.pos();
            let fullness = self.npc.borrow().fullness;
            for b in blockheads.iter() {
                if host.bh_paused(b) {
                    continue;
                }
                if is_sim && !host.bh_should_continue_simulating(b) {
                    continue;
                }
                if host.bh_regenerating(b) {
                    continue;
                }
                if host.bh_on_trade_mission(b) {
                    continue;
                }
                if Npc::belongs_to_player_with_blockhead(self, b) && (fullness as f64) > 1350.0f64 {
                    continue;
                }
                let att = self.g(|s| s.attack_blockhead.clone());
                if att.is_some() && !is_same(&att, b) {
                    continue;
                }
                let bp = b.pos();
                if !(pos.x < bp.x + 12 && bp.x - 12 < pos.x && pos.y < bp.y + 12 && bp.y - 12 < pos.y) {
                    continue;
                }
                let md = self.g(|s| s.movement_direction);
                if bp.y == pos.y && ((bp.x == pos.x + 1 && md == 1) || (bp.x == pos.x - 1 && md == -1) || bp.x == pos.x)
                {
                    // melee range
                    if !self.g(|s| s.biting) && att.is_none() {
                        closest = Some(b.clone());
                        closest_dist = -1.0f32;
                        break;
                    }
                } else if !self.g(|s| s.spitting) && att.is_none() {
                    if !aggro && !self.g(|s| s.provoked) && !Npc::tamed(self) {
                        continue;
                    }
                    let a = make_intpair(pos.x, pos.y + 1);
                    let c = make_intpair(bp.x, bp.y + 1);
                    let los = match host.world() {
                        Some(w) => clear_line_of_sight_between_tiles(a, c, &w),
                        None => false,
                    };
                    if los {
                        let d = Vector2 { x: pos.x as f32, y: (pos.y + 1) as f32 }
                            .sub(Vector2 { x: bp.x as f32, y: (bp.y + 1) as f32 });
                        let d2 = d.length_squared();
                        if d2 < closest_dist {
                            closest = Some(b.clone());
                            closest_dist = d2;
                        }
                    }
                }
            }
        }
        if let Some(c) = closest {
            if 0.0f32 <= closest_dist {
                self.s(|s| {
                    s.spitting = true;
                    s.attack_blockhead = Some(c.clone());
                    s.spit_timer = 0.0f32;
                });
                me.set_update_needs_to_be_sent(true);
            } else {
                self.s(|s| {
                    s.biting = true;
                    s.attack_blockhead = Some(c.clone());
                    s.bite_timer = 0.0f32;
                });
                me.set_update_needs_to_be_sent(true);
            }
            if !rider_local {
                if !aggro && !self.g(|s| s.provoked) && !Npc::tamed(self) {
                    return;
                }
                let bx = c.pos().x;
                let pos = me.pos();
                if pos.x + 1 < bx {
                    self.s(|s| {
                        s.movement_direction = 1;
                        s.walk_speed = if s.walk_speed <= 3.0f32 { 3.0f32 } else { s.walk_speed };
                    });
                } else if bx < pos.x - 1 {
                    self.s(|s| {
                        s.movement_direction = -1;
                        s.walk_speed = if s.walk_speed <= 3.0f32 { 3.0f32 } else { s.walk_speed };
                    });
                }
                me.set_update_needs_to_be_sent(true);
            }
        }
    }
}

impl ScorpionData {
    /// `fallSpeed += 4.0f * dt`; returns the new value.
    fn fall_accel(&mut self, dt: f32) -> f32 {
        self.fall_speed += 4.0f32 * dt;
        self.fall_speed
    }
    /// `drownTimer += dt`; returns the new value.
    fn add_drown(&mut self, dt: f32) -> f32 {
        self.drown_timer += dt;
        self.drown_timer
    }
    /// `biteTimer += 2.0f * dt`; returns the new value.
    fn advance_bite(&mut self, dt: f32) -> f32 {
        self.bite_timer += 2.0f32 * dt;
        self.bite_timer
    }
    /// `spitTimer += delta`; returns the new value.
    fn advance_spit(&mut self, delta: f32) -> f32 {
        self.spit_timer += delta;
        self.spit_timer
    }
}

impl DynamicObject for Scorpion {
    fn base(&self) -> &RefCell<DynamicObjectBase> {
        &self.base
    }
    fn class_info(&self) -> &'static ClassInfo {
        &SCORPION_CLASS
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    npc_forward!(
        object_type,
        get_save_dict,
        blockheads_loaded,
        can_be_removed_by_blockhead,
        update_position,
        should_save_every_change_in_position,
        client_id_for_saving_seperately_and_only_loading_while_player_online,
        requires_physical_block,
        render_pos,
        dealloc,
        as_harmable,
        as_ridable,
        as_tap_choice
    );

    fn update_accurate_dt_is_simulation(&self, me: &ObjRef, dt: f32, accurate_dt: f32, is_simulation: bool) {
        self.update_impl(me, dt, accurate_dt, is_simulation)
    }

    /// `-creationNetDataForClient:`: 0x80 bytes plus the NPC tail; unwritten bytes are 0.
    fn creation_net_data_for_client(&self, me: &ObjRef, client_id: Option<&str>) -> Option<Vec<u8>> {
        let att = self.g(|s| s.attack_blockhead.clone());
        let mut d = ScorpionCreationNetData::default();
        d.npc_creation_net_data = Npc::npc_creation_net_data_for_client(self, me, client_id);
        d.scorpion_update_net_data = self.scorpion_update_net_data_for_client(me, client_id);
        d.attack_blockhead = 0;
        if let Some(a) = att {
            d.attack_blockhead = a.unique_id();
            d.attack_is_spitting = self.g(|s| s.spitting) as u8;
        }
        Some(Npc::append_npc_creation_data_to_data(self, me, d.to_vec()))
    }

    /// `-updateNetDataForClient:`: exactly 0x28 bytes.
    fn update_net_data_for_client(&self, me: &ObjRef, client_id: Option<&str>) -> Option<Vec<u8>> {
        Some(self.scorpion_update_net_data_for_client(me, client_id).to_vec())
    }

    fn remote_update(&self, me: &ObjRef, net_data: &[u8]) {
        npc_remote_update(me, net_data);
        if !self.npc.borrow().dead {
            let u = <ScorpionUpdateNetData as WireStruct>::read_le(&padded_bytes(net_data, 0x28));
            self.do_remote_update(me, &u);
        }
    }

    /// `-remoteCreationDataUpdate:`: proceeds only when NOT under local control.
    fn remote_creation_data_update(&self, me: &ObjRef, net_data: &[u8]) {
        npc_remote_creation_data_update(me, net_data);
        if self.npc.borrow().dead {
            return;
        }
        let d = <ScorpionCreationNetData as WireStruct>::read_le(&padded_bytes(net_data, 0x80));
        if self.is_under_local_control() {
            return;
        }
        if d.attack_blockhead == 0 {
            self.s(|s| s.attack_blockhead = None);
        } else {
            for b in npc_env().all_blockheads_including_net() {
                if b.unique_id() == d.attack_blockhead {
                    let cur = self.g(|s| s.attack_blockhead.clone());
                    let mut changed = true;
                    if let Some(c) = &cur {
                        changed = c.unique_id() != d.attack_blockhead;
                        if changed {
                            self.s(|s| s.attack_blockhead = None);
                        }
                    }
                    if changed {
                        self.s(|s| {
                            s.attack_blockhead = Some(b.clone());
                            if d.attack_is_spitting == 0 {
                                s.biting = true;
                                s.bite_timer = 0.0f32;
                            } else {
                                s.spitting = true;
                                s.spit_timer = 0.0f32;
                            }
                        });
                        if !me.is_net() {
                            me.set_update_needs_to_be_sent(true);
                        }
                        self.s(|s| s.walk_speed = if s.walk_speed <= 3.0f32 { 3.0f32 } else { s.walk_speed });
                    }
                }
            }
        }
        self.do_remote_update(me, &d.scorpion_update_net_data);
    }

    /// `-blockheadUnloaded:`: zeroes `riderTargetVelocity` for the rider, then the NPC super.
    fn blockhead_unloaded(&self, me: &ObjRef, b: &ObjRef) {
        if is_same(&self.npc.borrow().rider.clone(), b) {
            self.s(|s| s.rider_target_velocity = 0.0f32);
        }
        npc_blockhead_unloaded(me, b);
    }
}

impl Npc for Scorpion {
    fn npc(&self) -> &RefCell<NpcState> {
        &self.npc
    }
    fn npc_type(&self) -> i32 {
        NPC_SCORPION
    }
    fn max_age(&self) -> f32 {
        7200.0f32
    }
    fn max_health(&self) -> u16 {
        20
    }
    fn creation_data_struct_size(&self) -> usize {
        0x80
    }
    fn species_name(&self) -> String {
        "SCORPION".to_string()
    }
    fn food_item_type(&self) -> i32 {
        ITEM_PRICKLY_PEAR
    }
    fn captured_item_type(&self) -> i32 {
        ITEM_SCORPION_CAGE
    }
    fn capture_required_item_type(&self) -> i32 {
        ITEM_CAGE
    }
    fn get_names_array(&self) -> Option<&'static [&'static str]> {
        Some(&SCORPION_NAMES)
    }
    fn get_names_array_count(&self) -> i32 {
        22
    }
    /// `-die:`: the death timer restarts at 0 (the sound is not played), then the NPC `die:`.
    fn die(&self, me: &ObjRef, killer: Option<&ObjRef>) {
        if !self.npc.borrow().dead {
            self.s(|s| s.travel_fraction = 0.0f32);
            npc_die(me, killer);
        }
    }
    /// `-reactToBeingHit`: sound only.
    fn react_to_being_hit(&self, _me: &ObjRef) {}
    /// `-reactToBeingFed`: `[super reactToBeingFed]` (empty in NPC), then the state resets.
    fn react_to_being_fed(&self, _me: &ObjRef) {
        self.s(|s| {
            s.movement_direction = 0;
            s.random_goal_rotation = 0.0f32;
            s.walk_speed = 1.0f32;
            s.walk_timer = 0.0f32;
        });
    }
    /// `-hitWithForce:blockhead:`: super, then provoke when under local control (one `rand()`).
    fn hit_with_force_blockhead(&self, me: &ObjRef, force: i32, b: Option<&ObjRef>) {
        npc_hit_with_force_blockhead(me, force, b);
        if !self.npc.borrow().dead && self.is_under_local_control() {
            let r = r01("Scorpion.hitWithForce");
            self.s(|s| {
                s.provoked = true;
                s.attack_delay = 0.0f32;
                s.movement_direction = if (r as f64) > 0.5f64 { 1 } else { -1 };
                s.walk_speed = 3.0f32;
                s.random_time_between_direction_changes = 5.0f32;
            });
            me.set_update_needs_to_be_sent(true);
        }
    }
    fn tap_is_within_body_radius(&self, tap: Vector2) -> bool {
        self.tap_is_within_body_radius_impl(tap)
    }
    fn can_be_captured_by_blockhead_with_item_type(&self, b: &ObjRef, item: i32) -> bool {
        item == ITEM_CAGE && Npc::belongs_to_player_with_blockhead(self, b)
    }
    fn cant_be_captured_tip_string_for_blockhead_with_item_type(&self, b: &ObjRef, item: i32) -> Option<String> {
        if item == ITEM_CAGE {
            if !Npc::tamed(self) {
                return Some(format!("{}S MUST BE TAMED\nBEFORE YOU CAN CAPTURE THEM", self.species_name()));
            }
            if !Npc::belongs_to_player_with_blockhead(self, b) {
                return Some(format!("THAT {} BELONGS TO\nSOMEONE ELSE", self.species_name()));
            }
        }
        None
    }
    /// `-riderPosForBlockhead:`: `renderPos + (0, -0.15)`, z `-0.25`.
    fn rider_pos_for_blockhead(&self, _b: &ObjRef) -> [f32; 4] {
        let r = DynamicObject::render_pos(self);
        [r.x + 0.0f32, r.y + -0.15f32, -0.25f32, 1.0f32]
    }
    fn set_target_velocity(&self, v: Vector2) {
        self.s(|s| s.rider_target_velocity = v.x);
    }
    /// `-rideDirection`: thresholds are f64 +-0.1.
    fn ride_direction(&self) -> i32 {
        if self.npc.borrow().rider.is_none() {
            return 0;
        }
        let v = self.g(|s| s.rider_target_velocity) as f64;
        if v > 0.1f64 {
            1
        } else if v < -0.1f64 {
            -1
        } else {
            0
        }
    }
    fn add_rider(&self, me: &ObjRef, b: &ObjRef) {
        npc_add_rider(me, b);
        if !self.npc.borrow().being_controlled {
            self.s(|s| s.rider_target_velocity = 0.0f32);
        }
    }
    fn remove_rider(&self, me: &ObjRef, b: &ObjRef) {
        self.remove_rider_opt(me, Some(b))
    }
    fn remove_rider_opt(&self, me: &ObjRef, b: Option<&ObjRef>) {
        let cur = self.npc.borrow().rider.clone();
        let same = match (&cur, b) {
            (None, None) => true,
            (Some(a), Some(b)) => ptr_eq(a, b),
            _ => false,
        };
        if same {
            npc_remove_rider_opt(me, b);
            self.s(|s| s.rider_target_velocity = 0.0f32);
        }
    }
    /// `-blockheadCanRide:usingItem:`
    fn blockhead_can_ride_using_item(&self, b: &ObjRef, _item: i32) -> bool {
        if Npc::tamed(self) && !Npc::belongs_to_player_with_blockhead(self, b) {
            return false;
        }
        !self.npc.borrow().dead && !self.base.borrow().needs_removed
    }
}

impl_npc_protocols!(Scorpion);

fn ctor(
    world: &WorldRef,
    dw: &DynamicWorldRef,
    pos: IntPair,
    cache: CacheRef,
    save_dict: Option<&PlistDict>,
    is_adult: bool,
    was_placed: bool,
    placed_by_client: Option<&str>,
) -> Option<ObjRef> {
    Scorpion::init_with_world_dynamic_world_at_position_cache_save_dict_is_adult_was_placed_placed_by_client(
        world,
        dw,
        pos,
        cache,
        save_dict,
        is_adult,
        was_placed,
        placed_by_client,
    )
    .map(|o| o as ObjRef)
}

/// Registers the scorpion constructor for NPC type 7 with the npc_base factory (`object_registry::install` calls this).
pub fn register() {
    register_npc_ctor(NPC_SCORPION, ctor);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::npc::cave_troll::test_support::*;
    use crate::path::path_creator::testutil::edit;
    use crate::rng::glibc_rand;

    fn ip(x: i32, y: i32) -> IntPair {
        IntPair { x, y }
    }

    fn mk(fx: &Fx, pos: IntPair) -> (Rc<Scorpion>, ObjRef) {
        let s =
            Scorpion::init_with_world_dynamic_world_at_position_cache_save_dict_is_adult_was_placed_placed_by_client(
                &fx.wref, &fx.dwr, pos, None, None, true, false, None,
            )
            .expect("scorpion");
        let me: ObjRef = s.clone();
        (s, me)
    }

    /// Tests 1 and 2: layouts and constants.
    #[test]
    fn layouts_and_constants() {
        assert_eq!(std::mem::size_of::<ScorpionUpdateNetData>(), 0x28);
        assert_eq!(std::mem::size_of::<ScorpionCreationNetData>(), 0x80);
        assert_eq!(SCORPION_NAMES.len(), 22);
        assert_eq!(SCORPION_NAMES[13], "Herculies");
        let f = fixture(10);
        let (s, me) = mk(&f, ip(20, 11));
        assert_eq!(me.object_type(), 51);
        assert_eq!(Npc::npc_type(&*s), 7);
        assert_eq!(Npc::max_age(&*s), 7200.0);
        assert_eq!(Npc::species_name(&*s), "SCORPION");
        assert_eq!(Npc::food_item_type(&*s), 24);
        assert_eq!(Npc::captured_item_type(&*s), 327);
        assert_eq!(Npc::capture_required_item_type(&*s), 302);
        assert_eq!(Npc::max_health(&*s), 20);
        assert_eq!(Npc::creation_data_struct_size(&*s), 0x80);
        assert_eq!(Npc::get_names_array_count(&*s), 22);
        let c = me.creation_net_data_for_client(&me, None).unwrap();
        assert_eq!(c.len(), 0x80);
        assert!(
            c[0x70..0x80].iter().all(|x| *x == 0),
            "no target: attackBlockhead, attackIsSpitting and padding are 0"
        );
        let u = me.update_net_data_for_client(&me, None).unwrap();
        assert_eq!(u.len(), 0x28);
        assert_eq!(u[0x23], 0, "clearQueue is 0");
        assert!(u[0x24..].iter().all(|x| *x == 0));
        assert_eq!(me.class_info().name, "Scorpion");
    }

    /// Tests 3 and 4: `loadDerivedStuff` consumes two draws; the direction mapping; walk speed in [0.5, 2.5).
    #[test]
    fn load_derived_stuff_rng() {
        assert_eq!(direction_from_roll(0.1), -1);
        assert_eq!(direction_from_roll(0.25), 0);
        assert_eq!(direction_from_roll(0.5), 0);
        assert_eq!(direction_from_roll(0.75), 0);
        assert_eq!(direction_from_roll(0.76), 1);
        let f = fixture(10);
        glibc_rand::srand(1);
        let (s, _me) = mk(&f, ip(20, 11));
        assert_eq!(draws_since(1), 6, "NPC adult init 4 + loadDerivedStuff 2");
        let d = s.data();
        assert!(d.walk_speed >= 0.5 && d.walk_speed < 2.5);
        assert_eq!(d.travel_fraction, 1.0);
        assert_eq!(d.from_square, ip(20, 11));
        assert_eq!(d.to_square, ip(20, 11));
        assert_eq!(d.random_time_between_direction_changes, 0.0);
        drop(d);
        glibc_rand::srand(6);
        let st = rand_stream(6, 2);
        glibc_rand::srand(6);
        s.load_derived_stuff();
        assert_eq!(draws_since(6), 2);
        assert_eq!(s.data().movement_direction, direction_from_roll(st[0] as f32 / 2147483648.0f32));
        assert_eq!(s.data().walk_speed, (st[1] as f32 / 2147483648.0f32 + 0.25f32) * 2.0f32);
    }

    /// Test 5: `getSaveDict` adds no scorpion keys; a reload re-rolls direction and speed (2 draws).
    #[test]
    fn save_dict_has_only_npc_keys() {
        let f = fixture(10);
        let (_s, me) = mk(&f, ip(20, 11));
        let d = me.get_save_dict(&me);
        let keys: Vec<&String> = d.map.iter().map(|(k, _)| k).collect();
        for k in ["movementDirection", "walkSpeed", "provoked", "attackBlockhead"] {
            assert!(!keys.iter().any(|x| x.as_str() == k), "unexpected key {k}");
        }
        assert!(d.map.get(&"saveTime".to_string()).is_some());
        glibc_rand::srand(8);
        let s2 = Scorpion::init_with_world_dynamic_world_save_dict_cache(&f.wref, &f.dwr, &d, None).expect("load");
        assert_eq!(draws_since(8), 3, "NPC load: 1 hunger draw + 2 loadDerivedStuff draws");
        let _ = s2;
    }

    /// Test 10: the dead branch needs `travelFraction >= 1.0` (4 * dt per update) and drops 1 or 2 poison.
    #[test]
    fn dead_branch() {
        for seed in [1u32, 2, 3, 4, 5, 6] {
            let f = fixture(10);
            let (s, me) = mk(&f, ip(20, 11));
            s.npc.borrow_mut().dead = true;
            s.data_mut().travel_fraction = 0.0;
            glibc_rand::srand(seed);
            let first = glibc_rand::rand();
            glibc_rand::srand(seed);
            me.update_accurate_dt_is_simulation(&me, 0.2, 0.2, false);
            assert!(f.host.free_blocks.borrow().is_empty(), "0.8 < 1.0");
            assert_eq!(draws_since(seed), 0);
            me.update_accurate_dt_is_simulation(&me, 0.1, 0.1, false);
            let fb = f.host.free_blocks.borrow().clone();
            let want = if first > 0x3fffffff { 2 } else { 1 };
            assert_eq!(fb.len(), want);
            assert!(fb.iter().all(|x| x.1 == 255));
            assert!(me.needs_removed());
            assert_eq!(draws_since(seed), 1);
        }
    }

    /// Test 10: bite window (crossing 0.5 of `biteTimer`), damage `0.1 * (1 - damage/20)` clamped 0.02, difficulty scaling.
    #[test]
    fn bite_window() {
        let run = |dmg: u16, diff: Option<i8>| {
            let f = fixture(10);
            if let Some(d) = diff {
                set_difficulty(&f, d);
            }
            let (s, me) = mk(&f, ip(20, 11));
            let v = TestBh::alloc(500, ip(21, 11));
            s.npc.borrow_mut().damage = dmg;
            {
                let mut d = s.data_mut();
                d.biting = true;
                d.bite_timer = 0.3;
                d.attack_blockhead = Some(v);
                d.random_time_between_direction_changes = 100.0;
                d.attack_delay = 100.0;
            }
            me.update_accurate_dt_is_simulation(&me, 0.05, 0.05, false);
            assert!(f.host.damage.borrow().is_empty(), "0.4 has not crossed 0.5");
            me.update_accurate_dt_is_simulation(&me, 0.05, 0.05, false);
            assert!(f.host.damage.borrow().is_empty(), "0.5 is not > 0.5");
            me.update_accurate_dt_is_simulation(&me, 0.05, 0.05, false);
            assert_eq!(f.host.damage.borrow().len(), 1, "0.6 crossed 0.5");
            me.update_accurate_dt_is_simulation(&me, 0.05, 0.05, false);
            assert_eq!(f.host.damage.borrow().len(), 1, "only once");
            // finish the animation
            for _ in 0..10 {
                me.update_accurate_dt_is_simulation(&me, 0.05, 0.05, false);
            }
            assert!(!s.data().biting);
            assert!(s.data().attack_blockhead.is_none());
            let r = f.host.damage.borrow().clone();
            r
        };
        assert_eq!(run(0, None)[0], (500, 0.1f32, false, true));
        assert_eq!(run(20, None)[0].1, 0.02f32);
        assert_eq!(run(0, Some(CUSTOM_RULE_NPC_EASY))[0].1, ((0.1f32 as f64) * 0.25) as f32);
        assert_eq!(run(0, Some(CUSTOM_RULE_NPC_HARD))[0].1, 4.0f32 * 0.1f32);
    }

    /// Test 10: the bite needs the target on the same row and within one column.
    #[test]
    fn bite_needs_adjacent_target() {
        let f = fixture(10);
        let (s, me) = mk(&f, ip(20, 11));
        let v = TestBh::alloc(500, ip(23, 11));
        {
            let mut d = s.data_mut();
            d.biting = true;
            d.bite_timer = 0.45;
            d.attack_blockhead = Some(v);
            d.random_time_between_direction_changes = 100.0;
            d.attack_delay = 100.0;
        }
        me.update_accurate_dt_is_simulation(&me, 0.05, 0.05, false);
        assert!(f.host.damage.borrow().is_empty());
    }

    /// Test 10: the spit fires one projectile from `(pos.x + 0.5, pos.y + 1.0)` to `center + (0, 0.5)`, item 255.
    #[test]
    fn spit_projectile() {
        let f = fixture(10);
        let (s, me) = mk(&f, ip(20, 11));
        let v = TestBh::alloc(500, ip(25, 11));
        {
            let mut d = s.data_mut();
            d.spitting = true;
            d.spit_timer = 0.45;
            d.attack_blockhead = Some(v.clone());
            d.random_time_between_direction_changes = 100.0;
            d.attack_delay = 100.0;
        }
        me.update_accurate_dt_is_simulation(&me, 0.05, 0.05, false);
        let p = f.host.projectiles.borrow().clone();
        assert_eq!(p.len(), 1);
        assert_eq!(p[0].0, Vector2 { x: 20.5, y: 12.0 });
        assert_eq!(p[0].1, Vector2 { x: v.float_pos().x, y: v.float_pos().y + 0.5 });
        assert_eq!(p[0].2, 255);
        // the timer exceeding 1.0 ends the spit and re-arms the delay (night: 0.5)
        f.host.day.set(0.1);
        for _ in 0..10 {
            me.update_accurate_dt_is_simulation(&me, 0.05, 0.05, false);
        }
        assert!(!s.data().spitting);
        assert_eq!(f.host.projectiles.borrow().len(), 1);
    }

    /// Test 10: target search: ranged targets only when aggressive (night), provoked or tamed; passive never.
    #[test]
    fn target_search_rules() {
        let setup = |day: f32, diff: Option<i8>, provoked: bool, tamed: bool| {
            let f = fixture(10);
            f.host.day.set(day);
            if let Some(d) = diff {
                set_difficulty(&f, d);
            }
            let (s, me) = mk(&f, ip(20, 11));
            if tamed {
                s.npc.borrow_mut().tamed_client_id = Some("server".to_string());
            }
            let v = TestBh::alloc(500, ip(26, 11));
            f.env.bhs.borrow_mut().push(v);
            {
                let mut d = s.data_mut();
                d.provoked = provoked;
                d.attack_delay = 0.0;
                d.random_time_between_direction_changes = 100.0;
            }
            me.update_accurate_dt_is_simulation(&me, 0.01, 0.01, false);
            (s, f)
        };
        let (s, _f) = setup(0.5, None, false, false);
        assert!(!s.data().spitting, "daytime, wild, not provoked");
        assert_eq!(s.data().attack_delay, 1.0, "the search ran and re-armed the 1 s delay");
        let (s, _f) = setup(0.1, None, false, false);
        assert!(s.data().spitting, "night");
        let (s, _f) = setup(0.5, None, true, false);
        assert!(s.data().spitting, "provoked");
        let (s, _f) = setup(0.5, None, false, true);
        assert!(s.data().spitting, "tamed");
        let (s, _f) = setup(0.5, Some(CUSTOM_RULE_NPC_HARD), false, false);
        assert!(s.data().spitting, "hard rules are always aggressive");
        let (s, _f) = setup(0.1, Some(CUSTOM_RULE_NPC_PASSIVE), false, false);
        assert!(!s.data().spitting && !s.data().biting, "passive never searches");
        // a well-fed owner is ignored
        let f = fixture(10);
        f.host.day.set(0.1);
        let (s, me) = mk(&f, ip(20, 11));
        s.npc.borrow_mut().tamed_client_id = Some("server".to_string());
        s.npc.borrow_mut().fullness = 2000.0;
        let v = TestBh::alloc(500, ip(26, 11));
        f.env.bhs.borrow_mut().push(v);
        {
            let mut d = s.data_mut();
            d.attack_delay = 0.0;
            d.random_time_between_direction_changes = 100.0;
        }
        me.update_accurate_dt_is_simulation(&me, 0.01, 0.01, false);
        // tamed to "server" and the test blockhead has no client id: it belongs to the owner and is spared
        assert!(!s.data().spitting);
    }

    /// Test 10: melee range starts a bite (and no ranged attack); the walker turns toward the target.
    #[test]
    fn melee_starts_bite() {
        let f = fixture(10);
        f.host.day.set(0.1);
        let (s, me) = mk(&f, ip(20, 11));
        let v = TestBh::alloc(500, ip(20, 11));
        f.env.bhs.borrow_mut().push(v);
        {
            let mut d = s.data_mut();
            d.attack_delay = 0.0;
            d.random_time_between_direction_changes = 100.0;
        }
        me.update_accurate_dt_is_simulation(&me, 0.01, 0.01, false);
        assert!(s.data().biting && !s.data().spitting);
        assert_eq!(s.data().bite_timer, 0.0);
        assert!(me.update_needs_to_be_sent());
    }

    /// Test 10: wander draws (#1 timer, #2 speed, #3 direction) when the direction timer expires.
    #[test]
    fn wander_draw_order() {
        for seed in 1..40u32 {
            let f = fixture(10);
            let (s, me) = mk(&f, ip(20, 11));
            {
                let mut d = s.data_mut();
                d.random_time_between_direction_changes = 0.0;
                d.attack_delay = 100.0;
                d.walk_speed = 1.0;
                d.movement_direction = 0;
            }
            glibc_rand::srand(seed);
            let st = rand_stream(seed, 4);
            glibc_rand::srand(seed);
            me.update_accurate_dt_is_simulation(&me, 0.01, 0.01, false);
            let r = |i: usize| st[i] as f32 / 2147483648.0f32;
            let timer = r(0) * 2.0f32 + 1.0f32;
            assert_eq!(s.data().random_time_between_direction_changes, timer);
            let speed = ((1.0f32 as f64) * 0.5f64 + (r(1) as f64) + 0.25f64) as f32;
            assert_eq!(s.data().walk_speed, speed);
            let n = draws_since(seed);
            // #3 always (standing still); #4 only when the new direction is 0 and the tile is free
            let new_dir = direction_from_roll(r(2));
            assert_eq!(n, if new_dir == 0 { 4 } else { 3 });
            if new_dir == 0 {
                let want = ((r(3) as f64) * PI_D * 2.0f64 - PI_D) as f32;
                assert_eq!(s.data().random_goal_rotation, want);
            } else {
                assert_eq!(s.data().movement_direction, new_dir);
            }
        }
    }

    /// Test 10: `hitWithForce` provokes (one draw) when under local control.
    #[test]
    fn hit_provokes() {
        let f = fixture(10);
        let (s, me) = mk(&f, ip(20, 11));
        let h = TestBh::alloc(1, ip(21, 11));
        glibc_rand::srand(13);
        let st = rand_stream(13, 1);
        glibc_rand::srand(13);
        Npc::hit_with_force_blockhead(&*s, &me, 2, Some(&h));
        assert_eq!(draws_since(13), 1);
        let d = s.data();
        assert!(d.provoked);
        assert_eq!(d.attack_delay, 0.0);
        assert_eq!(d.movement_direction, if (st[0] as f32 / 2147483648.0f32) as f64 > 0.5 { 1 } else { -1 });
        assert_eq!(d.walk_speed, 3.0);
        assert_eq!(d.random_time_between_direction_changes, 5.0);
        drop(d);
        // a remote rider means the server is not in control: no provocation, no draw
        let (s2, me2) = mk(&f, ip(24, 11));
        let rider = TestBh::alloc(2, ip(24, 11));
        rider.base().borrow_mut().is_net = true;
        s2.npc.borrow_mut().rider = Some(rider);
        assert!(!s2.is_under_local_control());
        glibc_rand::srand(14);
        Npc::hit_with_force_blockhead(&*s2, &me2, 2, Some(&h));
        assert_eq!(draws_since(14), 0);
        assert!(!s2.data().provoked);
    }

    /// Rider rules and `isUnderLocalControl` (E5: a connected player's blockhead is `isNet`).
    #[test]
    fn rider_rules() {
        let f = fixture(10);
        let (s, me) = mk(&f, ip(20, 11));
        assert!(s.is_under_local_control());
        assert_eq!(Npc::ride_direction(&*s), 0);
        let rider = TestBh::alloc(2, ip(20, 11));
        s.npc.borrow_mut().rider = Some(rider.clone());
        assert!(s.is_under_local_control(), "local rider: ride from the server side");
        Npc::set_target_velocity(&*s, Vector2 { x: 0.5, y: 9.0 });
        assert_eq!(s.data().rider_target_velocity, 0.5);
        assert_eq!(Npc::ride_direction(&*s), 1);
        Npc::set_target_velocity(&*s, Vector2 { x: -0.05, y: 0.0 });
        assert_eq!(Npc::ride_direction(&*s), 0);
        Npc::set_target_velocity(&*s, Vector2 { x: -0.11, y: 0.0 });
        assert_eq!(Npc::ride_direction(&*s), -1);
        assert_eq!(Npc::rider_pos_for_blockhead(&*s, &rider)[1], 11.0 - 0.15);
        assert_eq!(Npc::rider_pos_for_blockhead(&*s, &rider)[2], -0.25);
        s.blockhead_unloaded(&me, &rider);
        assert_eq!(s.data().rider_target_velocity, 0.0);
        rider.base().borrow_mut().is_net = true;
        s.npc.borrow_mut().rider = Some(rider.clone());
        assert!(!s.is_under_local_control());
        // remote rider: doRemoteUpdate accepts the controlling client's goal
        let mut u = ScorpionUpdateNetData::default();
        u.to_square_x = 22;
        u.to_square_y = 11;
        u.walk_speed = (2.0f32 * 255.0f32) as i16;
        u.movement_direction = -1;
        me.remote_update(&me, &u.to_vec());
        assert_eq!(s.data().remote_goal_square, ip(22, 11));
        assert_eq!(s.data().movement_direction, -1);
        assert!((s.data().remote_walk_speed - 2.0).abs() < 0.01);
        // without a rider the update is ignored
        s.npc.borrow_mut().rider = None;
        s.data_mut().remote_goal_square = ip(0, 0);
        me.remote_update(&me, &u.to_vec());
        assert_eq!(s.data().remote_goal_square, ip(0, 0));
        // rideability
        s.npc.borrow_mut().dead = false;
        assert!(Npc::blockhead_can_ride_using_item(&*s, &rider, 0));
        s.npc.borrow_mut().tamed_client_id = Some("other".to_string());
        assert!(!Npc::blockhead_can_ride_using_item(&*s, &rider, 0));
    }

    /// Feed / capture rules.
    #[test]
    fn feed_and_capture() {
        let f = fixture(10);
        let (s, me) = mk(&f, ip(20, 11));
        Npc::react_to_being_fed(&*s, &me);
        let d = s.data();
        assert_eq!((d.movement_direction, d.random_goal_rotation, d.walk_speed, d.walk_timer), (0, 0.0, 1.0, 0.0));
        drop(d);
        let h = TestBh::alloc(1, ip(21, 11));
        assert!(!Npc::can_be_captured_by_blockhead_with_item_type(&*s, &h, 302));
        assert_eq!(
            Npc::cant_be_captured_tip_string_for_blockhead_with_item_type(&*s, &h, 302).unwrap(),
            "SCORPIONS MUST BE TAMED\nBEFORE YOU CAN CAPTURE THEM"
        );
        s.npc.borrow_mut().tamed_client_id = Some("someone".to_string());
        assert_eq!(
            Npc::cant_be_captured_tip_string_for_blockhead_with_item_type(&*s, &h, 302).unwrap(),
            "THAT SCORPION BELONGS TO\nSOMEONE ELSE"
        );
        assert!(Npc::cant_be_captured_tip_string_for_blockhead_with_item_type(&*s, &h, 1).is_none());
    }

    /// Step 9: the walker follows the ground (wander off a ledge falls; blocked turns around).
    #[test]
    fn walking_and_turning() {
        let f = fixture(10);
        let (s, me) = mk(&f, ip(20, 11));
        {
            let mut d = s.data_mut();
            d.random_time_between_direction_changes = 100.0;
            d.attack_delay = 100.0;
            d.movement_direction = 1;
            d.walk_speed = 2.0;
            d.travel_fraction = 1.0;
        }
        me.update_accurate_dt_is_simulation(&me, 0.1, 0.1, false);
        assert_eq!(s.data().to_square, ip(21, 11));
        assert!(me.unreliable_update_needs_to_be_sent());
        // block the way with a two-high wall: the scorpion turns around
        edit(&f.world, 22, 11, |t| t.type_index = 1);
        edit(&f.world, 22, 12, |t| t.type_index = 1);
        s.data_mut().travel_fraction = 1.0;
        for _ in 0..15 {
            me.update_accurate_dt_is_simulation(&me, 0.1, 0.1, false);
        }
        assert_eq!(s.data().movement_direction, -1);
    }

    /// Birth scheduling: well fed, both lay timers expired: one draw and a layTimer in [720, 900).
    #[test]
    fn lay_scheduling() {
        let f = fixture(10);
        let (s, me) = mk(&f, ip(20, 11));
        {
            let mut n = s.npc.borrow_mut();
            n.fullness = 6000.0;
            n.lay_cooldown_timer = 0.0;
            n.lay_timer = 0.0;
        }
        {
            let mut d = s.data_mut();
            d.random_time_between_direction_changes = 100.0;
            d.attack_delay = 100.0;
            d.movement_direction = 1;
            d.walk_speed = 2.0;
            d.travel_fraction = 1.0;
            d.from_square = ip(19, 11); // a finished horizontal step
            d.to_square = ip(20, 11);
        }
        glibc_rand::srand(15);
        me.update_accurate_dt_is_simulation(&me, 0.1, 0.1, false);
        assert_eq!(draws_since(15), 1);
        let lt = s.npc.borrow().lay_timer;
        assert!(lt >= 720.0 && lt <= 900.0, "{lt}");
        // the timer then elapses and a baby is created
        s.npc.borrow_mut().lay_timer = 0.05;
        me.update_accurate_dt_is_simulation(&me, 0.1, 0.1, false);
        assert_eq!(f.host.babies.borrow().clone(), vec![(ip(20, 11), 7)]);
        assert_eq!(s.npc.borrow().lay_cooldown_timer, 900.0);
        assert!(s.npc.borrow().fullness < 6000.0 - 2600.0);
    }

    /// Drowning: 4 s submerged kills the scorpion.
    #[test]
    fn drowning() {
        let f = fixture(10);
        edit(&f.world, 20, 11, |t| {
            t.type_index = 3;
            t.partial_content_left = 255;
        });
        let (s, me) = mk(&f, ip(20, 11));
        {
            let mut d = s.data_mut();
            d.random_time_between_direction_changes = 100.0;
            d.attack_delay = 100.0;
            d.movement_direction = 0;
            d.walk_speed = 0.0;
        }
        for _ in 0..3 {
            me.update_accurate_dt_is_simulation(&me, 1.0, 1.0, false);
        }
        assert!(!s.npc.borrow().dead);
        me.update_accurate_dt_is_simulation(&me, 1.5, 1.5, false);
        assert!(s.npc.borrow().dead);
        assert_eq!(s.data().travel_fraction, 0.0, "die: restarts the death timer");
    }

    /// The registry constructor produces a type-51 scorpion.
    #[test]
    fn registered_ctor() {
        let f = fixture(10);
        register();
        let o = crate::npc::npc::new_npc_for_type(7, &f.wref, &f.dwr, ip(20, 11), None, None, true, false, None)
            .expect("scorpion");
        assert_eq!(o.object_type(), 51);
    }

    /// A creation packet with an attack target reports the target id and whether it is spitting.
    #[test]
    fn creation_data_reports_target() {
        let f = fixture(10);
        let (s, me) = mk(&f, ip(20, 11));
        let v = TestBh::alloc(0x1122, ip(23, 11));
        {
            let mut d = s.data_mut();
            d.attack_blockhead = Some(v);
            d.spitting = true;
        }
        let c = me.creation_net_data_for_client(&me, None).unwrap();
        assert_eq!(u64::from_le_bytes(c[0x70..0x78].try_into().unwrap()), 0x1122);
        assert_eq!(c[0x78], 1);
        assert!(c[0x79..0x80].iter().all(|x| *x == 0));
    }

    /// Tiny deterministic generator for the scenarios (the game's `rand()` stays the scorpion's own).
    struct Gen(u64);
    impl Gen {
        fn next(&mut self) -> u32 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            (self.0 >> 16) as u32
        }
        fn below(&mut self, n: u32) -> u32 {
            self.next() % n
        }
    }

    /// Uneven desert ground around x 14..=40 with gates, water, cacti and ledges.
    fn dunes(f: &Fx, g: &mut Gen) {
        for x in 12..=42 {
            let h = 10 + g.below(3) as i32;
            for y in 7..=16 {
                edit(&f.world, x, y, |t| {
                    t.type_index = if y <= h { 1 } else { 2 };
                    t.contents = 0;
                    t.foreground_contents = 0;
                    t.partial_content_left = 0;
                });
            }
        }
        for _ in 0..16 {
            let x = 12 + g.below(31) as i32;
            let y = 8 + g.below(7) as i32;
            let k = g.below(8);
            edit(&f.world, x, y, |t| match k {
                0 => t.contents = 0x92,
                1 => {
                    t.type_index = 3;
                    t.partial_content_left = 0x80 + g.below(0x7f) as u8
                }
                2 => t.contents = 0x2b,
                3 => t.type_index = 1,
                4 => t.type_index = 5,
                5 => t.foreground_contents = 0x42,
                6 => t.type_index = 3,
                _ => {}
            });
        }
    }

    #[test]
    fn seeded_dunes_cover_the_state_machine() {
        for seed in 1..=600u64 {
            let f = fixture(10);
            let mut g = Gen(seed.wrapping_mul(0x9E3779B97F4A7C15) | 1);
            dunes(&f, &mut g);
            glibc_rand::srand(seed as u32);
            let pos = ip(18 + g.below(17) as i32, 10 + g.below(4) as i32);
            let (s, me) = mk(&f, pos);
            if g.below(6) == 0 {
                me.base().borrow_mut().is_net = true;
            }
            if g.below(3) == 0 {
                me.base().borrow_mut().dynamic_world = None;
            }
            let mut rider: Option<ObjRef> = None;
            match g.below(8) {
                0 => rider = Some(TestBh::alloc(900, ip(0, 0))),
                1 => {
                    let r = TestBh::alloc(901, ip(0, 0));
                    r.base().borrow_mut().is_net = true;
                    rider = Some(r)
                }
                _ => {}
            }
            {
                let mut n = s.npc.borrow_mut();
                n.rider = rider.clone();
                n.fullness = [100.0, 2000.0, 5000.0, 7000.0][g.below(4) as usize];
                n.lay_cooldown_timer = [0.0, 0.0, 30.0][g.below(3) as usize];
                n.lay_timer = [0.0, 0.0, 0.05][g.below(3) as usize];
                if g.below(5) == 0 {
                    n.tamed_client_id = Some("server".into());
                }
            }
            {
                let mut d = s.data_mut();
                d.movement_direction = g.below(3) as i32 - 1;
                d.walk_speed = [0.0, 1.0, 1.4, 2.0, 3.5][g.below(5) as usize];
                d.provoked = g.below(3) == 0;
                d.attack_delay = [0.0, 0.0, 2.0][g.below(3) as usize];
                d.drown_timer = if g.below(5) == 0 { 3.9 } else { 0.0 };
                d.rider_target_velocity = [0.0, 0.5, -0.5, 0.05, -0.05][g.below(5) as usize];
                d.remote_goal_square = ip(14 + g.below(25) as i32, 9 + g.below(7) as i32);
                d.remote_walk_speed = [0.0, 0.5, 2.0][g.below(3) as usize];
                d.remote_movement_direction = g.below(3) as i32 - 1;
                if g.below(5) == 0 {
                    d.from_square = pos;
                    d.to_square = ip(pos.x + g.below(3) as i32 - 1, pos.y + g.below(3) as i32 - 1);
                }
                d.random_time_between_direction_changes = [0.0, 0.0, 5.0][g.below(3) as usize];
            }
            let mut bhs = vec![];
            for i in 0..g.below(4) {
                let v = TestBh::alloc(500 + i as u64, ip(pos.x + g.below(21) as i32 - 10, 10 + g.below(5) as i32));
                if g.below(5) == 0 {
                    v.base().borrow_mut().is_net = true;
                }
                bhs.push(v);
            }
            *f.env.bhs.borrow_mut() = bhs.clone();
            if g.below(6) == 0 {
                f.host.paused.set(Some(500));
            }
            if g.below(6) == 0 {
                f.host.nosim.set(Some(501));
            }
            if g.below(7) == 0 {
                f.host.regen.set(Some(500));
            }
            if g.below(7) == 0 {
                f.host.trade.set(Some(501));
            }
            if g.below(2) == 0 {
                f.host.cactus.set(Some(2000.0));
            }
            f.host.npc_at.set(g.below(3) == 0);
            f.host.day.set(if g.below(2) == 0 { 0.1 } else { 0.6 });
            match g.below(7) {
                0 => set_difficulty(&f, 0),
                1 => set_difficulty(&f, 2),
                2 => set_difficulty(&f, 3),
                3 => set_difficulty(&f, 1),
                _ => {}
            }
            if g.below(8) == 0 {
                if let Some(v) = bhs.first() {
                    Npc::begin_blockhead_inspection(&*s, v);
                }
            }
            if g.below(8) == 0 {
                if let Some(v) = bhs.first() {
                    Npc::blockhead_is_coming_to_inspect(&*s, v);
                }
            }
            if g.below(6) == 0 {
                let v = bhs.first().cloned();
                let mut d = s.data_mut();
                d.biting = g.below(2) == 0;
                d.spitting = !d.biting;
                d.attack_blockhead = v;
                d.bite_timer = 0.3;
                d.spit_timer = 0.3;
            }
            let dts = [0.05f32, 0.2, 0.6, 1.5];
            let dt = dts[g.below(4) as usize];
            for _ in 0..150 {
                if me.needs_removed() {
                    break;
                }
                if g.below(80) == 0 {
                    s.npc.borrow_mut().dead = true;
                }
                if g.below(30) == 0 {
                    Npc::set_target_velocity(&*s, Vector2 { x: [0.0, 0.5, -0.5][g.below(3) as usize], y: 0.0 });
                }
                me.update_accurate_dt_is_simulation(&me, dt, dt, g.below(2) == 0);
            }
            me.update_accurate_dt_is_simulation(&me, dt, dt, false);
        }
    }

    fn creation_bytes(me: &ObjRef, target: u64, spitting: u8) -> Vec<u8> {
        let mut nd = me.creation_net_data_for_client(me, None).unwrap();
        nd[0x70..0x78].copy_from_slice(&target.to_le_bytes());
        nd[0x78] = spitting;
        nd
    }

    #[test]
    fn replica_creation_data_names_the_attack_target() {
        let f = fixture(10);
        let (s, me) = mk(&f, ip(20, 11));
        let t1 = TestBh::alloc(600, ip(21, 11));
        let t2 = TestBh::alloc(601, ip(22, 11));
        *f.env.bhs.borrow_mut() = vec![t1.clone(), t2.clone()];
        // a fresh replica from the creation data
        let nd = creation_bytes(&me, 600, 0);
        let rep = Scorpion::init_with_world_dynamic_world_cache_net_data(&f.wref, &f.dwr, None, &nd).expect("replica");
        assert!(rep.base.borrow().is_net);
        assert_eq!(rep.data().remote_goal_square, ip(20, 11));
        assert!(rep.data().biting);
        assert!(rep.data().walk_speed >= 3.0);
        // an unloaded position is refused
        let mut bad = nd.clone();
        bad[12..16].copy_from_slice(&100000u32.to_le_bytes());
        assert!(Scorpion::init_with_world_dynamic_world_cache_net_data(&f.wref, &f.dwr, None, &bad).is_none());
        // the same target again changes nothing; another target switches, spitting this time
        let rme: ObjRef = rep.clone();
        rme.remote_creation_data_update(&rme, &nd);
        assert!(rep.data().biting);
        rme.remote_creation_data_update(&rme, &creation_bytes(&me, 601, 1));
        assert!(rep.data().spitting);
        assert_eq!(rep.data().attack_blockhead.as_ref().unwrap().unique_id(), 601);
        // an id nobody has: untouched; id 0: cleared
        rme.remote_creation_data_update(&rme, &creation_bytes(&me, 999, 0));
        assert_eq!(rep.data().attack_blockhead.as_ref().unwrap().unique_id(), 601);
        rme.remote_creation_data_update(&rme, &creation_bytes(&me, 0, 0));
        assert!(rep.data().attack_blockhead.is_none());
        // a server-side scorpion that was told about a target also asks for an update
        s.npc.borrow_mut().rider = Some(TestBh::alloc(900, ip(0, 0)).clone());
        me.base().borrow_mut().is_net = true;
        s.npc.borrow_mut().rider = None;
        me.remote_creation_data_update(&me, &creation_bytes(&me, 600, 0));
        // under local control (a local, non-replica scorpion): ignored
        let (s2, me2) = mk(&f, ip(24, 11));
        me2.remote_creation_data_update(&me2, &creation_bytes(&me2, 600, 0));
        assert!(s2.data().attack_blockhead.is_none());
        // dead: ignored
        rep.npc.borrow_mut().dead = true;
        rme.remote_creation_data_update(&rme, &creation_bytes(&me, 600, 0));
        assert!(rep.data().attack_blockhead.is_none());
        rme.remote_update(&rme, &me.update_net_data_for_client(&me, None).unwrap());
    }

    #[test]
    fn remote_updates_follow_the_controlling_side() {
        let f = fixture(10);
        let (s, me) = mk(&f, ip(20, 11));
        let mut u = ScorpionUpdateNetData {
            to_square_x: 25,
            to_square_y: 11,
            walk_speed: 102,
            movement_direction: 1,
            ..ScorpionUpdateNetData::default()
        };
        // server: no remote rider, nothing happens
        s.do_remote_update(&me, &u);
        assert_eq!(s.data().remote_goal_square, ip(0, 0));
        // server with a remote rider: accepted
        let rider = TestBh::alloc(901, ip(0, 0));
        rider.base().borrow_mut().is_net = true;
        s.npc.borrow_mut().rider = Some(rider.clone());
        s.do_remote_update(&me, &u);
        assert_eq!(s.data().remote_goal_square, ip(25, 11));
        assert_eq!(s.data().movement_direction, 1);
        assert_eq!(s.data().walk_speed, 0.4);
        // client with a remote rider: accepted as the goal only
        f.env.client.set(true);
        u.to_square_x = 27;
        s.do_remote_update(&me, &u);
        assert_eq!(s.data().remote_goal_square, ip(27, 11));
        // client, but the attack target is a local blockhead: ignored
        let local = TestBh::alloc(602, ip(21, 11));
        s.data_mut().attack_blockhead = Some(local);
        u.to_square_x = 29;
        s.do_remote_update(&me, &u);
        assert_eq!(s.data().remote_goal_square, ip(27, 11));
        // client with a local rider: ignored
        s.npc.borrow_mut().rider = Some(TestBh::alloc(903, ip(0, 0)));
        s.data_mut().attack_blockhead = None;
        s.do_remote_update(&me, &u);
        assert_eq!(s.data().remote_goal_square, ip(27, 11));
    }

    #[test]
    fn tap_hit_test_faces_and_riders() {
        let f = fixture(10);
        let (s, me) = mk(&f, ip(2, 11));
        let fp = me.float_pos();
        assert!(s.tap_is_within_body_radius_impl(Vector2 { x: fp.x + 128.0, y: fp.y + 0.5 }));
        assert!(s.tap_is_within_body_radius_impl(Vector2 { x: fp.x - 128.0, y: fp.y + 0.5 }));
        assert!(Npc::tap_is_within_body_radius(&*s, Vector2 { x: fp.x, y: fp.y + 0.5 }));
        assert!(!s.tap_is_within_body_radius_impl(Vector2 { x: fp.x, y: fp.y + 1.5 }));
        assert!(!s.tap_is_within_body_radius_impl(Vector2 { x: fp.x + 3.0, y: fp.y + 0.5 }));
        let bh: ObjRef = TestBh::alloc(77, ip(3, 11));
        crate::npc::npc::faces::exercise_faces(&me, &bh);
        // riders
        let r1: ObjRef = TestBh::alloc(80, ip(2, 11));
        let r2: ObjRef = TestBh::alloc(81, ip(2, 11));
        // a client blockhead the server controls cannot ride: no rider, steering reset
        let ctl: ObjRef = CtlBh::alloc(82);
        s.data_mut().rider_target_velocity = 0.7;
        Npc::add_rider(&*s, &me, &ctl);
        assert!(s.npc.borrow().rider.is_none());
        assert_eq!(s.data().rider_target_velocity, 0.0);
        s.data_mut().rider_target_velocity = 0.7;
        Npc::add_rider(&*s, &me, &r1);
        assert_eq!(s.data().rider_target_velocity, 0.7);
        // removing someone else keeps the rider; removing the rider clears the steering
        Npc::remove_rider(&*s, &me, &r2);
        assert_eq!(s.data().rider_target_velocity, 0.7);
        Npc::remove_rider(&*s, &me, &r1);
        assert_eq!(s.data().rider_target_velocity, 0.0);
        assert!(s.npc.borrow().rider.is_none());
        // no rider and no argument: the same (nothing)
        s.remove_rider_opt(&me, None);
        // an unloading rider stops steering
        s.npc.borrow_mut().rider = Some(r1.clone());
        s.data_mut().rider_target_velocity = 0.9;
        me.blockhead_unloaded(&me, &r1);
        assert_eq!(s.data().rider_target_velocity, 0.0);
    }

    #[test]
    fn far_inspector_is_ignored_and_dead_scorpion_is_not_hit_tested() {
        let f = fixture(10);
        let (s, me) = mk(&f, ip(20, 11));
        let far = TestBh::alloc(70, ip(60, 11));
        s.npc.borrow_mut().coming_to_inspect_blockhead = Some(std::rc::Rc::downgrade(&far));
        me.update_accurate_dt_is_simulation(&me, 0.01, 0.01, false);
        assert!(!s.tap_is_within_body_radius_impl(Vector2 { x: 1000.0, y: 1000.0 }));
        s.npc.borrow_mut().dead = true;
        assert!(!s.tap_is_within_body_radius_impl(me.float_pos()));
        // dying twice is harmless
        Npc::die(&*s, &me, None);
        Npc::die(&*s, &me, None);
    }

    #[test]
    fn creation_data_reaches_a_non_replica_with_a_remote_rider() {
        let f = fixture(10);
        let (s, me) = mk(&f, ip(20, 11));
        let t1 = TestBh::alloc(600, ip(21, 11));
        *f.env.bhs.borrow_mut() = vec![t1];
        let rider = TestBh::alloc(901, ip(0, 0));
        rider.base().borrow_mut().is_net = true;
        s.npc.borrow_mut().rider = Some(rider);
        me.remote_creation_data_update(&me, &creation_bytes(&me, 600, 0));
        assert_eq!(s.data().attack_blockhead.as_ref().unwrap().unique_id(), 600);
        assert!(me.update_needs_to_be_sent());
        // removing "nobody" while somebody rides keeps the rider
        s.remove_rider_opt(&me, None);
        assert!(s.npc.borrow().rider.is_some());
    }

    #[test]
    fn spit_target_needs_a_world_for_the_line_of_sight() {
        let f = fixture(10);
        let (s, me) = mk(&f, ip(20, 11));
        *f.env.bhs.borrow_mut() = vec![TestBh::alloc(600, ip(26, 11))];
        s.data_mut().attack_delay = 0.0;
        f.host.no_world.set(true);
        let host = hostile_host();
        s.attacks(&me, 0.1, false, true, false, &host);
        assert!(!s.data().spitting);
        f.host.no_world.set(false);
        s.data_mut().attack_delay = 0.0;
        s.attacks(&me, 0.1, false, true, false, &host);
        assert!(s.data().spitting);
    }

    #[test]
    fn tamed_scorpion_capture_tips() {
        let f = fixture(10);
        let (s, _me) = mk(&f, ip(20, 11));
        let b: ObjRef = TestBh::alloc(7, ip(21, 11));
        s.npc.borrow_mut().tamed_client_id = Some("server".into());
        assert!(Npc::can_be_captured_by_blockhead_with_item_type(&*s, &b, 302));
        assert!(Npc::cant_be_captured_tip_string_for_blockhead_with_item_type(&*s, &b, 302).is_none());
        s.npc.borrow_mut().tamed_client_id = Some("other".into());
        assert!(Npc::cant_be_captured_tip_string_for_blockhead_with_item_type(&*s, &b, 302)
            .unwrap()
            .contains("SOMEONE ELSE"));
    }

    #[test]
    fn laying_and_scheduling_need_loaded_tiles() {
        let f = fixture(10);
        let (s, me) = mk(&f, ip(20, 11));
        me.base().borrow_mut().pos = ip(20, 100000);
        s.npc.borrow_mut().lay_timer = 0.01;
        s.npc.borrow_mut().fullness = 6000.0;
        me.update_accurate_dt_is_simulation(&me, 0.1, 0.1, false);
        assert!(f.host.babies.borrow().is_empty());
        // a finished step onto an unloaded tile schedules nothing
        let (s, me) = mk(&f, ip(24, 11));
        {
            let mut n = s.npc.borrow_mut();
            n.fullness = 7000.0;
            n.lay_cooldown_timer = 0.0;
            n.lay_timer = 0.0;
        }
        {
            let mut d = s.data_mut();
            d.from_square = ip(24, 11);
            d.to_square = ip(25, 100000);
            d.travel_fraction = 0.99;
            d.walk_speed = 1.0;
            d.movement_direction = 1;
            d.random_time_between_direction_changes = 50.0;
        }
        me.update_accurate_dt_is_simulation(&me, 0.1, 0.1, false);
        assert_eq!(s.npc.borrow().lay_timer, 0.0);
    }
}
