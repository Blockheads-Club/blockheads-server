//! `DropBear: NPC`: the tree-ambush NPC (NPC type 2, dynamic object type 25).
//!
//! Class `DropBear`" (hit / feed / tame / capture rules).
//!
//! The AI is one state machine in [`DropBear::update_impl`]: CLIMBING (in a tree, courage rising), DROPPING
//! (falling), ON GROUND (hunting / going home), ATTACKING (0.25 s bite animation) and DEAD (fur / leather drop).
//! RNG: `loadDerivedStuff` 1 draw on every init path, `startAttack` 1 draw, death drop 2 draws (normal rules) or 1
//! (expert rules); nothing else.

use std::any::Any;
use std::cell::RefCell;
use std::rc::Rc;

use crate::fmath::cconv::f32_to_i32;
use crate::fmath::libm::sinf;
use crate::fmath::vector::linear_interpolate;
use crate::impl_npc_protocols;
use crate::npc::cave_troll::{
    climable, dict_bool, dict_float, dict_int, hostile_host, is_same, r01, set_float_pos, snap_float_pos, tile_loaded,
    world_time, CUSTOM_RULE_NPC_HARD, CUSTOM_RULE_NPC_PASSIVE, ITEM_CAGE,
};
use crate::npc::npc::{
    alloc_npc, npc_env, npc_get_save_dict, npc_hit_with_force_blockhead,
    npc_init_with_world_dynamic_world_at_position_cache_save_dict_is_adult_was_placed_placed_by_client,
    npc_init_with_world_dynamic_world_cache_net_data, npc_init_with_world_dynamic_world_save_dict_cache,
    npc_remote_creation_data_update, npc_remote_update, npc_successful_tame, npc_update_accurate_dt_is_simulation,
    register_npc_ctor, Npc, NpcState, NPC_CLASS, NPC_DROPBEAR,
};
use crate::npc_forward;
use crate::objects::base::{
    base_remove_from_macro_block, padded_bytes, DynamicObject, DynamicObjectBase, DynamicWorldRef, WorldRef,
};
use crate::objects::class_info::ClassInfo;
use crate::objects::handle::{CacheRef, ObjRef};
use crate::plist::value::{PlistDict, PlistValue};
use crate::types::netdata::{DropBearCreationNetData, DropBearUpdateNetData};
use crate::types::structs::{make_intpair, IntPair, Vector2, WireStruct};
use crate::world::helpers::{
    tile_contains_gate, tile_is_air_or_snow, tile_is_dead_tree, tile_is_solid, tile_is_tree_trunk, tile_is_water,
};
use crate::world::state::TileRef;

/// `DropBear: NPC`.
pub static DROP_BEAR_CLASS: ClassInfo = ClassInfo { name: "DropBear", superclass: Some(&NPC_CLASS), protocols: &[] };

/// `DROPBEAR_NAMES` (38 entries, exact strings).
pub const DROPBEAR_NAMES: [&str; 38] = [
    "Ted",
    "Yogi",
    "Mr. Bear",
    "Fluffy",
    "Cuddles",
    "Winnie",
    "Wuzzy",
    "Grizzlee",
    "Franklin",
    "Claws",
    "Theodore",
    "Barney",
    "Bobo",
    "Bongo",
    "Disco",
    "Flippy",
    "Humphrey",
    "Snuggles",
    "Bubbles",
    "Lulu",
    "Max",
    "Panda",
    "Koala",
    "Papa",
    "Paws",
    "Sonya",
    "Sebastian",
    "Paddington",
    "Jasper",
    "Bungle",
    "Fozzie",
    "Kevin",
    "Rupert",
    "Freddy",
    "Droppy",
    "Vegemite",
    "Bruno",
    "Jazz",
];

/// `ITEM_PINE_CONE`
pub const ITEM_PINE_CONE: i32 = 27;
/// `ITEM_DROPBEAR_CAGE`
pub const ITEM_DROPBEAR_CAGE: i32 = 326;
/// `ITEM_FUR`
pub const ITEM_FUR: i32 = 121;
/// `ITEM_LEATHER`
pub const ITEM_LEATHER: i32 = 120;

/// The own ivars of `DropBear` that are saved, sent, or read by `update:`.
#[derive(Clone)]
pub struct DropBearData {
    /// 0x188, INIT `3.14159274f`
    pub body_rotation: f32,
    pub goal_rotation_type: i32,
    pub walk_timer: f32,
    pub walk_speed: f32,
    pub travel_fraction: f32,
    pub from_square: IntPair,
    pub to_square: IntPair,
    pub provoke_meter: f32,
    pub courage_meter: f32,
    pub dropping: bool,
    pub drop_speed: f32,
    pub attacking: bool,
    pub attack_fraction: f32,
    pub attack_blockhead: Option<ObjRef>,
    /// INIT (1.0, 1.0) (R2)
    pub attack_offset: Vector2,
    pub attack_pos: IntPair,
    pub on_ground: bool,
    pub drop_pos: IntPair,
    pub goal_tree_direction: i32,
    pub should_save_when_tree_found: bool,
    pub drown_timer: f32,
    pub current_net_queue_size: i32,
    pub next_to_square: [IntPair; 4],
    pub next_walk_speed: [f32; 4],
    pub next_on_ground: [bool; 4],
}

impl DropBearData {
    /// Zeroed ivars after `alloc` plus the `.cxx_construct` default of `attackOffset`.
    pub fn new() -> DropBearData {
        let z = IntPair { x: 0, y: 0 };
        DropBearData {
            body_rotation: 0.0,
            goal_rotation_type: 0,
            walk_timer: 0.0,
            walk_speed: 0.0,
            travel_fraction: 0.0,
            from_square: z,
            to_square: z,
            provoke_meter: 0.0,
            courage_meter: 0.0,
            dropping: false,
            drop_speed: 0.0,
            attacking: false,
            attack_fraction: 0.0,
            attack_blockhead: None,
            attack_offset: Vector2::cxx_new(),
            attack_pos: z,
            on_ground: false,
            drop_pos: z,
            goal_tree_direction: 0,
            should_save_when_tree_found: false,
            drown_timer: 0.0,
            current_net_queue_size: 0,
            next_to_square: [z; 4],
            next_walk_speed: [0.0; 4],
            next_on_ground: [false; 4],
        }
    }
}

/// `DropBear`.
pub struct DropBear {
    base: RefCell<DynamicObjectBase>,
    npc: RefCell<NpcState>,
    bear: RefCell<DropBearData>,
}

impl DropBear {
    fn alloc() -> Rc<DropBear> {
        alloc_npc(|w| DropBear {
            base: RefCell::new(DynamicObjectBase::alloc_zeroed_with(w)),
            npc: RefCell::new(NpcState::default()),
            bear: RefCell::new(DropBearData::new()),
        })
    }

    /// Read access to the bear ivars.
    pub fn data(&self) -> std::cell::Ref<'_, DropBearData> {
        self.bear.borrow()
    }

    /// Write access to the bear ivars (callers must not hold it across calls into other objects).
    pub fn data_mut(&self) -> std::cell::RefMut<'_, DropBearData> {
        self.bear.borrow_mut()
    }

    fn g<R>(&self, f: impl FnOnce(&DropBearData) -> R) -> R {
        f(&self.bear.borrow())
    }

    fn s(&self, f: impl FnOnce(&mut DropBearData)) {
        f(&mut self.bear.borrow_mut())
    }

    // ------------------------------------------------------------------------------------ initialisers

    /// `-initWithWorld:dynamicWorld:atPosition:cache:saveDict:isAdult:wasPlaced:placedByClient:`. `isAdult` is
    /// ignored by the class.
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
    ) -> Option<Rc<DropBear>> {
        let o = DropBear::alloc();
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
        o.s(|b| b.courage_meter = 1.0f32);
        if was_placed {
            let tile = tile_loaded(pos.x, pos.y);
            if !climable(tile, pos) {
                o.s(|b| b.dropping = true);
            }
        }
        o.load_derived_stuff();
        Some(o)
    }

    /// `-initWithWorld:dynamicWorld:saveDict:cache:`. Returns `None` (after `removeFromMacroBlock`) when the
    /// bear died of old age while unloaded.
    pub fn init_with_world_dynamic_world_save_dict_cache(
        world: &WorldRef,
        dw: &DynamicWorldRef,
        save_dict: &PlistDict,
        cache: CacheRef,
    ) -> Option<Rc<DropBear>> {
        let o = DropBear::alloc();
        let me: ObjRef = o.clone();
        let inited = npc_init_with_world_dynamic_world_save_dict_cache(&me, world, dw, save_dict, cache);
        debug_assert!(inited); // the base initialisers cannot fail on this path
        o.s(|b| {
            b.provoke_meter = dict_float(save_dict, "provokeMeter");
            b.courage_meter = dict_float(save_dict, "courageMeter");
            b.dropping = dict_bool(save_dict, "dropping");
            b.drop_speed = dict_float(save_dict, "dropSpeed");
            b.on_ground = dict_bool(save_dict, "onGround");
            b.drop_pos.x = dict_int(save_dict, "dropPos.x");
            b.drop_pos.y = dict_int(save_dict, "dropPos.y");
            b.goal_tree_direction = dict_int(save_dict, "goalTreeDirection");
        });
        let save_time = dict_float(save_dict, "saveTime");
        let now_time = world_time(&me) as f32;
        let age = {
            let mut n = o.npc.borrow_mut();
            n.age += now_time - save_time;
            n.age
        };
        if age <= Npc::max_age(&*o) {
            o.load_derived_stuff();
            Some(o)
        } else {
            base_remove_from_macro_block(&me);
            None
        }
    }

    /// `-initWithWorld:dynamicWorld:cache:netData:`: the client replica path.
    pub fn init_with_world_dynamic_world_cache_net_data(
        world: &WorldRef,
        dw: &DynamicWorldRef,
        cache: CacheRef,
        net_data: &[u8],
    ) -> Option<Rc<DropBear>> {
        let o = DropBear::alloc();
        let me: ObjRef = o.clone();
        if !npc_init_with_world_dynamic_world_cache_net_data(&me, world, dw, cache, net_data) {
            return None;
        }
        o.load_derived_stuff();
        let pos = me.pos();
        o.s(|b| {
            for i in 0..4 {
                b.next_to_square[i] = pos;
                b.next_walk_speed[i] = 0.0f32;
                b.next_on_ground[i] = false;
            }
        });
        o.remote_creation_data_update(&me, net_data);
        Some(o)
    }

    /// `-loadDerivedStuff`: one `rand()`: `walkSpeed = 4.0f + R * 0.5f`; `bodyRotation = 3.14159274f`;
    /// `toSquare = fromSquare = pos`; `travelFraction = 1.0f`.
    pub fn load_derived_stuff(&self) {
        let r = r01("DropBear.loadDerivedStuff");
        let pos = self.base.borrow().pos;
        self.s(|b| {
            b.walk_speed = 4.0f32 + r * 0.5f32;
            b.goal_rotation_type = 0;
            b.body_rotation = 3.14159274f32;
            b.to_square = pos;
            b.from_square = b.to_square;
            b.travel_fraction = 1.0f32;
        });
    }

    // ------------------------------------------------------------------------------------ net

    /// `-dropBearUpdateNetDataForClient:`. The padding is written as zero.
    pub fn drop_bear_update_net_data_for_client(&self, me: &ObjRef, client_id: Option<&str>) -> DropBearUpdateNetData {
        let npc_upd = Npc::npc_update_net_data_for_client(self, me, client_id);
        let b = self.bear.borrow();
        let mut u = DropBearUpdateNetData::default();
        u.npc_update_net_data = npc_upd;
        u.to_square_x = b.to_square.x as u32;
        u.to_square_y = b.to_square.y as u32;
        u.walk_speed = f32_to_i32(b.walk_speed * 255.0f32) as i16;
        u.on_ground = b.on_ground as u8;
        u.dropping = b.dropping as u8;
        u
    }

    /// `-doRemoteUpdate:`: acts only on a client (`[dynamicWorld isClient]`); ported for completeness.
    pub fn do_remote_update(&self, me: &ObjRef, u: &DropBearUpdateNetData) {
        if !npc_env().is_client() {
            return;
        }
        let pos = me.pos();
        let mut play_drop = false;
        {
            let mut b = self.bear.borrow_mut();
            let mut idx = b.current_net_queue_size;
            if idx > 3 {
                b.current_net_queue_size = 3;
                idx = 3;
            }
            let i = idx as usize;
            b.next_to_square[i] = make_intpair(u.to_square_x as i32, u.to_square_y as i32);
            b.next_walk_speed[i] = (u.walk_speed as i32 as f32) / 255.0f32;
            b.next_on_ground[i] = u.on_ground != 0;
            b.current_net_queue_size += 1;
            if u.dropping != 0 && !b.dropping && !b.on_ground && u.on_ground == 0 {
                b.from_square = make_intpair(u.to_square_x as i32, u.to_square_y as i32);
                b.walk_speed = (u.walk_speed as i32 as f32) / 255.0f32;
                b.current_net_queue_size = 0;
                b.dropping = true;
                b.drop_speed = 10.0f32;
                b.drop_pos = pos;
                play_drop = true;
            }
        }
        if play_drop {
            let fp = me.float_pos();
            set_float_pos(me, (pos.x as f32) + 0.5f32, fp.y);
        }
    }

    // ------------------------------------------------------------------------------------ AI helpers

    /// `findTarget(initDist, initPos, filtered)`: the first Blockhead with the strictly smallest squared
    /// distance wins. Returns `(best, bestPos, bestDist)`.
    fn find_target(
        &self,
        me: &ObjRef,
        init_pos: IntPair,
        filtered: bool,
        is_sim: bool,
    ) -> (Option<ObjRef>, IntPair, i32) {
        let host = hostile_host();
        let pos = me.pos();
        let fullness = self.npc.borrow().fullness;
        let mut best_dist: i32 = 9999999;
        let mut best_pos = init_pos;
        let mut best: Option<ObjRef> = None;
        for bh in npc_env().all_blockheads_including_net() {
            if filtered {
                if host.bh_paused(&bh) {
                    continue;
                }
                if is_sim && !host.bh_should_continue_simulating(&bh) {
                    continue;
                }
                if Npc::belongs_to_player_with_blockhead(self, &bh) && (fullness as f64) > 1350.0f64 {
                    continue;
                }
            }
            let p = bh.pos();
            let dx = p.x.wrapping_sub(pos.x);
            let dy = p.y.wrapping_sub(pos.y);
            let d = dx.wrapping_mul(dx).wrapping_add(dy.wrapping_mul(dy));
            if d < best_dist {
                best = Some(bh.clone());
                best_pos = p;
                best_dist = d;
            }
        }
        (best, best_pos, best_dist)
    }

    /// `startAttack(target)`: one `rand()` for `attackOffset.y`.
    fn start_attack(&self, me: &ObjRef, target: &ObjRef) {
        let r = r01("DropBear.startAttack");
        let changed = !self.g(|b| is_same(&b.attack_blockhead, target));
        self.s(|b| {
            b.attacking = true;
            b.attack_fraction = 0.0f32;
            b.travel_fraction = 0.0f32;
            if changed {
                b.attack_blockhead = Some(target.clone());
            }
            b.attack_pos = target.pos();
            let fp = target.float_pos();
            b.attack_offset = Vector2 { x: fp.x + 0.0f32, y: fp.y + r };
        });
        if changed {
            me.set_creation_data_needs_to_be_sent(true);
        }
    }

    /// `step(g, to)`: `onGround = g; fromSquare = pos; toSquare = to; updateNeedsToBeSent = YES`.
    fn step(&self, me: &ObjRef, on_ground: bool, to: IntPair) {
        let pos = me.pos();
        self.s(|b| {
            b.on_ground = on_ground;
            b.from_square = pos;
            b.to_square = to;
        });
        me.set_update_needs_to_be_sent(true);
    }

    /// The fall-start block shared by the tree branches.
    fn begin_drop(&self, me: &ObjRef) {
        let pos = me.pos();
        let fp = me.float_pos();
        set_float_pos(me, (pos.x as f32) + 0.5f32, fp.y);
        self.s(|b| {
            b.to_square = pos;
            b.from_square = b.to_square;
            b.dropping = true;
            b.should_save_when_tree_found = true;
            b.drop_speed = 10.0f32;
            b.drop_pos = pos;
        });
    }

    /// `-update:accurateDT:isSimulation:`: the server-side AI.
    fn update_impl(&self, me: &ObjRef, dt: f32, accurate_dt: f32, is_sim: bool) {
        npc_update_accurate_dt_is_simulation(me, dt, accurate_dt, is_sim);
        if me.needs_removed() {
            return;
        }
        let host = hostile_host();
        let is_net = me.is_net();

        // ---- DEAD
        if self.npc.borrow().dead {
            if is_net {
                return;
            }
            let (mut fur, mut leather) = (0i32, 0i32);
            if !host.expert_mode() {
                fur = f32_to_i32(0.5f32 + r01("DropBear.dead.fur") * 2.0f32);
                leather = f32_to_i32(1.1f32 + r01("DropBear.dead.leather") * 2.0f32);
            } else if (r01("DropBear.dead.expert") as f64) >= 0.2f64 {
                leather = 1;
            } else {
                fur = 1;
            }
            let kb = self.npc.borrow().kill_blockhead.clone();
            let pos = me.pos();
            for _ in 0..fur {
                host.create_free_block(pos, ITEM_FUR, kb.as_ref());
            }
            for _ in 0..leather {
                host.create_free_block(pos, ITEM_LEATHER, kb.as_ref());
            }
            me.set_needs_removed(me, true);
            return;
        }

        // ---- MOVE interpolation
        {
            let (fs, ts, qs) = self.g(|b| (b.from_square, b.to_square, b.current_net_queue_size));
            if fs != ts || (is_net && qs > 0) {
                let arrived = self.bear.borrow_mut().travel_fraction_step(dt);
                if arrived {
                    let to = self.g(|b| b.to_square);
                    self.s(|b| b.from_square = b.to_square);
                    me.update_position(me, to);
                    Npc::check_current_position_for_food(self, me);
                    snap_float_pos(me, to);
                    if is_net && self.g(|b| b.current_net_queue_size) > 0 {
                        self.s(|b| {
                            b.to_square = b.next_to_square[0];
                            b.walk_speed = b.next_walk_speed[0];
                            b.on_ground = b.next_on_ground[0];
                            for i in 0..3 {
                                b.next_to_square[i] = b.next_to_square[i + 1];
                                b.next_walk_speed[i] = b.next_walk_speed[i + 1];
                                b.next_on_ground[i] = b.next_on_ground[i + 1];
                            }
                            b.current_net_queue_size -= 1;
                        });
                    }
                }
                let (fs, ts, tf) = self.g(|b| (b.from_square, b.to_square, b.travel_fraction));
                let from_pos = Vector2 { x: ((fs.x as f32) + 0.5f32), y: fs.y as f32 };
                let to_pos = Vector2 { x: ((ts.x as f32) + 0.5f32), y: ts.y as f32 };
                let x_t = tf;
                let mut y_lerp = tf;
                if fs.x != ts.x {
                    if fs.y < ts.y {
                        y_lerp = sinf(((y_lerp as f64) * 3.141592653589793f64 * 0.75f64) as f32) / 0.7f32;
                    } else if ts.y < fs.y {
                        y_lerp = 1.0f32
                            - sinf((((1.0f32 - y_lerp) as f64) * 3.141592653589793f64 * 0.75f64) as f32) / 0.7f32;
                    }
                }
                set_float_pos(
                    me,
                    linear_interpolate(from_pos.x, to_pos.x, x_t),
                    linear_interpolate(from_pos.y, to_pos.y, y_lerp),
                );
            }
        }

        let pos = me.pos();
        let tile0 = tile_loaded(pos.x, pos.y);
        let tile1 = tile_loaded(pos.x, pos.y - 1);
        if tile0.is_none() || tile1.is_none() {
            return;
        }
        let t0 = tile0.unwrap().get();

        // ---- eating pine bush / trunk
        if (self.npc.borrow().fullness as f64) < 2700.0f64 && matches!(t0.contents, 6..=8) {
            self.npc.borrow_mut().fullness += 2.0f32 * dt;
        }

        // ---- drowning / fire
        if !tile_is_water(tile0) || t0.partial_content_left < 0x80 {
            self.s(|b| b.drown_timer = 0.0f32);
            if t0.on_fire != 0 {
                Npc::die(self, me, None);
            }
        } else {
            let d = self.bear.borrow_mut().add_drown(dt);
            if d > 8.0f32 {
                Npc::die(self, me, None);
            }
        }

        self.s(|b| b.provoke_meter -= dt / 60.0f32);

        let cr = host.custom_rules();
        let rules_on = cr.custom_rules_enabled != 0;
        let npc_peaceful = rules_on && cr.npc_difficulty == CUSTOM_RULE_NPC_PASSIVE;
        let npc_hard = rules_on && cr.npc_difficulty == CUSTOM_RULE_NPC_HARD;

        // ---- ATTACKING
        if self.g(|b| b.attacking) {
            self.s(|b| b.attack_fraction += dt);
            let target = self.g(|b| b.attack_blockhead.clone());
            let mut release = false;
            if let Some(t) = &target {
                if t.needs_removed() {
                    release = true;
                } else if host.bh_regenerating(t) {
                    release = true;
                } else if host.bh_on_trade_mission(t) {
                    release = true;
                } else if Npc::belongs_to_player_with_blockhead(self, t)
                    && (self.npc.borrow().fullness as f64) > 1350.0f64
                {
                    release = true;
                }
            }
            if release {
                self.s(|b| b.attack_blockhead = None);
            }
            let target = self.g(|b| b.attack_blockhead.clone());
            let af = self.g(|b| b.attack_fraction);
            let Some(target) = target.filter(|_| af > 0.125f32) else {
                if af > 0.25f32 {
                    self.s(|b| b.attacking = false);
                }
                return;
            };
            // the bite lands (attackFraction > 0.125)
            let bp = target.pos();
            if bp == self.g(|b| b.attack_pos) {
                if !target.is_net() {
                    let damage = self.npc.borrow().damage;
                    let ratio = (damage as f32) / (Npc::max_health(self) as f32);
                    let mut dmg = 0.2f32 * (1.0f32 - ratio);
                    if dmg <= 0.02f32 {
                        dmg = 0.02f32;
                    }
                    host.bh_suffer_damage(&target, dmg, is_sim, true);
                    // the difficulty scaling that follows in the original is a dead store: no effect
                    if is_sim {
                        let s = if host.bh_regenerating(&target) { "killed" } else { "attacked" };
                        host.add_damage_simulation_event(&target, &format!("{} by a dropbear", s));
                    }
                }
                self.s(|b| b.courage_meter = 0.0f32);
            }
            self.s(|b| b.attack_blockhead = None);
            me.set_update_needs_to_be_sent(true);
            return;
        }

        // ---- DROPPING (falling)
        if self.g(|b| b.dropping) {
            let ds = self.bear.borrow_mut().accelerate_drop(dt);
            let mut fp = me.float_pos();
            fp.y -= ds * dt;
            set_float_pos(me, fp.x, fp.y);
            if f32_to_i32(fp.y) == pos.y {
                return;
            }
            if tile_is_solid(tile1) {
                self.s(|b| b.dropping = false);
                set_float_pos(me, fp.x, pos.y as f32);
                self.s(|b| {
                    b.to_square = pos;
                    b.from_square = b.to_square;
                });
                me.update_position(me, pos);
                snap_float_pos(me, pos);
                return;
            }
            if (pos.y as f32) - fp.y <= 1.0f32 {
                me.update_position(me, make_intpair(pos.x, f32_to_i32(fp.y)));
            } else {
                me.update_position(me, make_intpair(pos.x, pos.y - 1));
                let np = me.pos();
                let fx = me.float_pos().x;
                set_float_pos(me, fx, np.y as f32);
            }
            if npc_peaceful {
                return;
            }
            // UB-PARITY: the original reads an uninitialised clPos when no Blockhead qualifies; "no target -> return".
            let (cl, cl_pos, _d) = self.find_target(me, IntPair { x: 0, y: 0 }, true, is_sim);
            let Some(cl) = cl else { return };
            let np = me.pos();
            if cl_pos.x != np.x {
                return;
            }
            if cl_pos.y != np.y {
                return;
            }
            self.s(|b| {
                b.dropping = false;
                b.travel_fraction = 0.0f32;
            });
            self.start_attack(me, &cl);
            me.set_update_needs_to_be_sent(true);
            return;
        }

        if self.npc.borrow().inspecting_blockhead.as_ref().and_then(|w| w.upgrade()).is_some() {
            return;
        }

        let air_or_gate_free = |t: Option<TileRef>| tile_is_air_or_snow(t) && !tile_contains_gate(t);

        // ---- ON GROUND (tile below is solid)
        if tile_is_solid(tile1) {
            let (prev, now) = self.bear.borrow_mut().decay_courage(dt);
            if now <= 0.0f32 {
                // go home: walk toward dropPos, then climb the tree
                if is_net {
                    return;
                }
                if prev >= 0.0f32 {
                    let dp = self.g(|b| b.drop_pos);
                    self.s(|b| {
                        b.goal_tree_direction = if pos.x < dp.x { 1 } else { -1 };
                        b.to_square = pos;
                        b.from_square = b.to_square;
                    });
                }
                if self.g(|b| b.from_square != b.to_square) {
                    return;
                }
                let above = tile_loaded(pos.x, pos.y + 1);
                let can_climb = tile_is_tree_trunk(tile0)
                    && tile_is_tree_trunk(above)
                    && !tile_is_dead_tree(tile0)
                    && !host.npc_exists_at_pos_ignore_npc(IntPair { x: pos.x, y: pos.y + 1 }, None)
                    && !host.npc_exists_at_pos_ignore_npc(pos, Some(me));
                if !can_climb {
                    let x_off = self.g(|b| b.goal_tree_direction);
                    let right = tile_loaded(pos.x + x_off, pos.y);
                    if !tile_is_air_or_snow(right) || tile_contains_gate(right) {
                        if !tile_is_solid(right) {
                            self.s(|b| b.goal_tree_direction = -b.goal_tree_direction);
                        } else {
                            let right_up = tile_loaded(pos.x + x_off, pos.y + 1);
                            if !tile_is_air_or_snow(right_up) || tile_contains_gate(right_up) {
                                self.s(|b| b.goal_tree_direction = -b.goal_tree_direction);
                            } else {
                                self.step(me, true, make_intpair(pos.x + x_off, pos.y + 1));
                            }
                        }
                    } else {
                        let right_down = tile_loaded(pos.x + x_off, pos.y - 1);
                        if !tile_is_air_or_snow(right_down) || tile_contains_gate(right_down) {
                            if tile_is_solid(right_down) {
                                self.step(me, true, make_intpair(pos.x + x_off, pos.y));
                            } else {
                                self.s(|b| b.goal_tree_direction = -b.goal_tree_direction);
                            }
                        } else {
                            let rdd = tile_loaded(pos.x + x_off, pos.y - 2);
                            if tile_is_solid(rdd) {
                                self.step(me, true, make_intpair(pos.x + x_off, pos.y - 1));
                            } else {
                                self.s(|b| b.goal_tree_direction = -b.goal_tree_direction);
                            }
                        }
                    }
                    self.s(|b| b.travel_fraction = 0.0f32);
                    return;
                } else {
                    // start climbing
                    self.s(|b| {
                        b.on_ground = false;
                        b.from_square = pos;
                        b.to_square = IntPair { x: pos.x, y: pos.y + 1 };
                    });
                    me.set_update_needs_to_be_sent(true);
                    return;
                }
            }
            // courage > 0: hunt
            if npc_peaceful {
                return;
            }
            let (cl, cl_pos, _d) = self.find_target(me, IntPair { x: -99, y: -99 }, true, is_sim);
            if cl_pos.x == pos.x {
                if cl_pos.y != pos.y {
                    return;
                }
                self.s(|b| b.on_ground = true);
                if let Some(cl) = &cl {
                    self.start_attack(me, cl);
                }
                me.set_update_needs_to_be_sent(true);
                return;
            }
            if self.g(|b| b.from_square != b.to_square) {
                return;
            }
            if cl_pos.y == pos.y && (cl_pos.x == pos.x + 1 || cl_pos.x == pos.x - 1) {
                self.s(|b| b.on_ground = true);
                if let Some(cl) = &cl {
                    self.start_attack(me, cl);
                }
                me.set_update_needs_to_be_sent(true);
                return;
            }
            let x_off = if cl_pos.x < pos.x { -1 } else { 1 };
            let right = tile_loaded(pos.x + x_off, pos.y);
            if !tile_is_air_or_snow(right) || tile_contains_gate(right) {
                if tile_is_solid(right) {
                    let right_up = tile_loaded(pos.x + x_off, pos.y + 1);
                    if air_or_gate_free(right_up) {
                        self.step(me, true, make_intpair(pos.x + x_off, pos.y + 1));
                    }
                }
            } else {
                let right_down = tile_loaded(pos.x + x_off, pos.y - 1);
                if !tile_is_air_or_snow(right_down) || tile_contains_gate(right_down) {
                    if tile_is_solid(right_down) {
                        self.step(me, true, make_intpair(pos.x + x_off, pos.y));
                    }
                } else {
                    let rdd = tile_loaded(pos.x + x_off, pos.y - 2);
                    if tile_is_solid(rdd) {
                        self.step(me, true, make_intpair(pos.x + x_off, pos.y - 1));
                    }
                }
            }
            self.s(|b| b.travel_fraction = 0.0f32);
            return;
        }

        // ---- IN TREE (tile below not solid)
        if tile_is_dead_tree(tile0) {
            me.set_needs_removed(me, true);
            return;
        }
        let rate = if npc_hard { 0.2f32 } else { 0.02f32 };
        self.s(|b| {
            b.courage_meter += dt * rate;
            b.on_ground = false;
        });
        if !climable(tile0, pos) {
            // no tree here: fall
            if is_net {
                return;
            }
            self.begin_drop(me);
            self.s(|b| b.provoke_meter = 1.0f32);
            me.set_update_needs_to_be_sent(true);
            return;
        }
        if self.g(|b| b.should_save_when_tree_found) {
            if let Some(dw) = me.dynamic_world() {
                dw.dynamic_world_changed_at_pos_object_type(pos, me.object_type());
            }
            self.s(|b| b.should_save_when_tree_found = false);
        }
        if self.g(|b| b.provoke_meter) > 0.01f32 {
            // provoked
            self.s(|b| b.provoke_meter = 0.0f32);
            if npc_peaceful {
                return;
            }
            let (cl, cl_pos, _d) = self.find_target(me, IntPair { x: 9999999, y: 9999999 }, true, is_sim);
            if (cl_pos.y.wrapping_sub(pos.y)).wrapping_abs() < 3
                && (cl_pos.x == pos.x || cl_pos.x == pos.x + 1 || cl_pos.x == pos.x - 1)
            {
                if let Some(cl) = &cl {
                    self.start_attack(me, cl);
                }
                me.set_update_needs_to_be_sent(true);
                return;
            }
            // otherwise jump
            self.begin_drop(me);
            me.set_update_needs_to_be_sent(true);
            return;
        }
        let courage = self.g(|b| b.courage_meter);
        if courage < 0.9f32 {
            if is_net {
                return;
            }
            if self.g(|b| b.from_square != b.to_square) {
                return;
            }
            let above = tile_loaded(pos.x, pos.y + 1);
            let ok = tile_is_tree_trunk(tile0)
                && tile_is_tree_trunk(above)
                && !host.npc_exists_at_pos_ignore_npc(IntPair { x: pos.x, y: pos.y + 1 }, None)
                && !host.npc_exists_at_pos_ignore_npc(pos, Some(me));
            if !ok {
                return;
            }
            self.s(|b| b.to_square = IntPair { x: pos.x, y: pos.y + 1 });
            me.set_update_needs_to_be_sent(true);
            return;
        }
        // courage >= 0.9: ambush from the tree at night
        if is_net {
            return;
        }
        let tod = if me.world().is_none() { 0.0f32 } else { host.day_night_fraction(pos.x as f32, world_time(me)) };
        let need_night_checks = !rules_on || cr.npc_difficulty != CUSTOM_RULE_NPC_HARD;
        if need_night_checks {
            if (tod as f64) >= 0.23f64 {
                return;
            }
            if t0.artificial_light_r > 0x1f || t0.artificial_light_g > 0x1f || t0.artificial_light_b > 0x1f {
                return;
            }
        }
        let mut dist3: i32 = 9999999;
        let mut cl_pos = IntPair { x: 0, y: 0 };
        if !npc_peaceful {
            let (_cl, p, d) = self.find_target(me, IntPair { x: 0, y: 0 }, false, is_sim);
            cl_pos = p;
            dist3 = d;
        }
        if dist3 > 0x3ff {
            return;
        }
        if self.g(|b| b.courage_meter) <= 0.9f32 {
            return;
        }
        if pos.y <= cl_pos.y {
            return;
        }
        if cl_pos.x <= pos.x - 6 {
            return;
        }
        if pos.x + 6 <= cl_pos.x {
            return;
        }
        // path check: simulate a descent
        let (mut y, mut x) = (pos.y - 1, pos.x);
        let mut found = false;
        let mut sanity = 32;
        let off = if pos.x < cl_pos.x { 1 } else { -1 };
        loop {
            if sanity < 1 {
                break;
            }
            sanity -= 1;
            let t = tile_loaded(x, y);
            if t.is_none() || !tile_is_air_or_snow(t) {
                break;
            }
            let below = tile_loaded(x, y - 1);
            if tile_is_air_or_snow(below) {
                if y - 1 == cl_pos.y && x == cl_pos.x {
                    found = true;
                    break;
                }
                y -= 1;
                continue;
            }
            if !tile_is_solid(below) {
                break;
            }
            let r = tile_loaded(x + off, y);
            if !tile_is_air_or_snow(r) || tile_contains_gate(r) {
                if !tile_is_solid(r) {
                    break;
                }
                let ru = tile_loaded(x + off, y + 1);
                if !tile_is_air_or_snow(ru) || tile_contains_gate(ru) {
                    break;
                }
                y += 1;
                x += off;
            } else {
                if y == cl_pos.y && x + off == cl_pos.x {
                    found = true;
                    break;
                }
                let rd = tile_loaded(x + off, y - 1);
                if tile_is_solid(rd) {
                    x += off;
                } else {
                    let rdd = tile_loaded(x + off, y - 2);
                    if !tile_is_solid(rdd) {
                        break;
                    }
                    y -= 1;
                    x += off;
                }
            }
            // overshoot guard after any x move
            let over = if off < 1 { x < cl_pos.x } else { cl_pos.x < x };
            if over {
                break;
            }
        }
        if !found {
            return;
        }
        self.begin_drop(me);
        me.set_update_needs_to_be_sent(true);
    }
}

impl DropBearData {
    /// `travelFraction += dt * walkSpeed; if >= 1.0 { -= 1.0; if >= 1.0 { = 0 } }`. Returns whether the step arrived.
    fn travel_fraction_step(&mut self, dt: f32) -> bool {
        self.travel_fraction += dt * self.walk_speed;
        if self.travel_fraction >= 1.0f32 {
            self.travel_fraction -= 1.0f32;
            if self.travel_fraction >= 1.0f32 {
                self.travel_fraction = 0.0;
            }
            true
        } else {
            false
        }
    }

    /// `drownTimer += dt`; returns the new value.
    fn add_drown(&mut self, dt: f32) -> f32 {
        self.drown_timer += dt;
        self.drown_timer
    }

    /// `dropSpeed = (float)((double)dropSpeed + (double)dt * 10.0)`; returns the new value.
    fn accelerate_drop(&mut self, dt: f32) -> f32 {
        self.drop_speed = ((self.drop_speed as f64) + (dt as f64) * 10.0f64) as f32;
        self.drop_speed
    }

    /// `prev = courageMeter; courageMeter -= dt / 4.0f`; returns `(prev, new)`.
    fn decay_courage(&mut self, dt: f32) -> (f32, f32) {
        let prev = self.courage_meter;
        self.courage_meter -= dt / 4.0f32;
        (prev, self.courage_meter)
    }
}

impl DynamicObject for DropBear {
    fn base(&self) -> &RefCell<DynamicObjectBase> {
        &self.base
    }
    fn class_info(&self) -> &'static ClassInfo {
        &DROP_BEAR_CLASS
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    npc_forward!(
        object_type,
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

    /// `-getSaveDict`. `dropPos.x/y` and `goalTreeDirection` are written with `numberWithBool:(char)v`, i.e.
    /// a plist boolean `((v as i8) != 0)` (R9).
    fn get_save_dict(&self, me: &ObjRef) -> PlistDict {
        let mut d = npc_get_save_dict(me);
        let b = self.bear.borrow();
        let m = &mut d.map;
        m.insert("provokeMeter".to_string(), PlistValue::real_f32(b.provoke_meter));
        m.insert("courageMeter".to_string(), PlistValue::real_f32(b.courage_meter));
        m.insert("dropping".to_string(), PlistValue::boolean(b.dropping));
        m.insert("dropSpeed".to_string(), PlistValue::real_f32(b.drop_speed));
        m.insert("onGround".to_string(), PlistValue::boolean(b.on_ground));
        m.insert("dropPos.x".to_string(), PlistValue::boolean((b.drop_pos.x as i8) != 0));
        m.insert("dropPos.y".to_string(), PlistValue::boolean((b.drop_pos.y as i8) != 0));
        m.insert("goalTreeDirection".to_string(), PlistValue::boolean((b.goal_tree_direction as i8) != 0));
        d
    }

    /// `-creationNetDataForClient:`: 0x78 bytes plus the NPC tail; bytes never written by the original are 0.
    fn creation_net_data_for_client(&self, me: &ObjRef, client_id: Option<&str>) -> Option<Vec<u8>> {
        let cd = DropBearCreationNetData {
            npc_creation_net_data: Npc::npc_creation_net_data_for_client(self, me, client_id),
            drop_bear_update_net_data: self.drop_bear_update_net_data_for_client(me, client_id),
            unused: 0,
        };
        Some(Npc::append_npc_creation_data_to_data(self, me, cd.to_vec()))
    }

    /// `-updateNetDataForClient:`: exactly 0x28 bytes.
    fn update_net_data_for_client(&self, me: &ObjRef, client_id: Option<&str>) -> Option<Vec<u8>> {
        Some(self.drop_bear_update_net_data_for_client(me, client_id).to_vec())
    }

    fn remote_update(&self, me: &ObjRef, net_data: &[u8]) {
        npc_remote_update(me, net_data);
        if !self.npc.borrow().dead {
            let u = <DropBearUpdateNetData as WireStruct>::read_le(&padded_bytes(net_data, 0x28));
            self.do_remote_update(me, &u);
        }
    }

    fn remote_creation_data_update(&self, me: &ObjRef, net_data: &[u8]) {
        npc_remote_creation_data_update(me, net_data);
        if !self.npc.borrow().dead {
            let cd = <DropBearCreationNetData as WireStruct>::read_le(&padded_bytes(net_data, 0x78));
            self.do_remote_update(me, &cd.drop_bear_update_net_data);
        }
    }

    /// `-blockheadUnloaded:`: clears `attackBlockhead` when it is the unloaded one. The NPC super is not
    /// called by the original; only the `attackBlockhead` identity is checked.
    fn blockhead_unloaded(&self, me: &ObjRef, b: &ObjRef) {
        if self.g(|d| is_same(&d.attack_blockhead, b)) {
            self.s(|d| d.attack_blockhead = None);
        }
        //  lists no `[super blockheadUnloaded:]` call for the bear; NPC's kill/rider clearing is not performed.
        let _ = me;
    }

    /// `-worldChanged:` is empty.
    fn world_changed(&self, _me: &ObjRef, _positions: &mut Vec<IntPair>) {}
}

impl DropBear {
    /// `-tapIsWithinBodyRadius:` (DropBear override): client hit test.
    pub fn tap_is_within_body_radius_impl(&self, tap: Vector2) -> bool {
        if self.npc.borrow().dead {
            return false;
        }
        let fp = self.base.borrow().float_pos;
        let wwm = self.base.borrow().world.as_ref().and_then(|w| w.upgrade()).map_or(0, |w| w.world_width_macro());
        let w = wwm.wrapping_mul(32);
        let half = (w / 2) as f32;
        let dx = tap.x - fp.x;
        let dxw = if dx < half {
            if ((wwm.wrapping_mul(-32)) / 2) as f32 <= dx {
                dx
            } else {
                dx + w as f32
            }
        } else {
            dx - w as f32
        };
        if (dxw as f64) < 0.5f64 && -0.5f64 < (dxw as f64) {
            let y = fp.y as f64;
            return y < (tap.y as f64) && (tap.y as f64) < y + 0.5f64 + 0.7f64;
        }
        false
    }
}

impl Npc for DropBear {
    fn npc(&self) -> &RefCell<NpcState> {
        &self.npc
    }
    fn tap_is_within_body_radius(&self, tap: Vector2) -> bool {
        self.tap_is_within_body_radius_impl(tap)
    }
    fn npc_type(&self) -> i32 {
        NPC_DROPBEAR
    }
    fn max_age(&self) -> f32 {
        7200.0f32
    }
    fn max_health(&self) -> u16 {
        16
    }
    fn creation_data_struct_size(&self) -> usize {
        0x78
    }
    fn species_name(&self) -> String {
        "DROPBEAR".to_string()
    }
    fn food_item_type(&self) -> i32 {
        ITEM_PINE_CONE
    }
    fn captured_item_type(&self) -> i32 {
        ITEM_DROPBEAR_CAGE
    }
    fn capture_required_item_type(&self) -> i32 {
        ITEM_CAGE
    }
    fn get_names_array(&self) -> Option<&'static [&'static str]> {
        Some(&DROPBEAR_NAMES)
    }
    fn get_names_array_count(&self) -> i32 {
        38
    }
    fn ridable_when_tamed(&self) -> bool {
        false
    }
    /// `-reactToBeingHit`: sound only.
    fn react_to_being_hit(&self, _me: &ObjRef) {}
    /// `-reactToBeingFed`: `walkSpeed = 1.0f; walkTimer = 0`.
    fn react_to_being_fed(&self, _me: &ObjRef) {
        self.s(|b| {
            b.walk_speed = 1.0f32;
            b.walk_timer = 0.0f32;
        });
    }
    /// `-successfulTame`: super, then a sound.
    fn successful_tame(&self, me: &ObjRef) {
        npc_successful_tame(me);
    }
    /// `-hitWithForce:blockhead:`: super, then `provokeMeter = 1.0f`.
    fn hit_with_force_blockhead(&self, me: &ObjRef, force: i32, b: Option<&ObjRef>) {
        npc_hit_with_force_blockhead(me, force, b);
        self.s(|d| d.provoke_meter = 1.0f32);
    }
    /// `-canBeCapturedByBlockhead:withItemType:`
    fn can_be_captured_by_blockhead_with_item_type(&self, b: &ObjRef, item: i32) -> bool {
        item == ITEM_CAGE && Npc::belongs_to_player_with_blockhead(self, b)
    }
    /// `-cantBeCapturedTipStringForBlockhead:withItemType:`
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
}

impl_npc_protocols!(DropBear);

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
    DropBear::init_with_world_dynamic_world_at_position_cache_save_dict_is_adult_was_placed_placed_by_client(
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

/// Registers the bear constructor for NPC type 2 with the npc_base factory (`object_registry::install` calls this).
pub fn register() {
    register_npc_ctor(NPC_DROPBEAR, ctor);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::npc::cave_troll::test_support::*;
    use crate::npc::cave_troll::{CUSTOM_RULE_NPC_HARD, CUSTOM_RULE_NPC_PASSIVE};
    use crate::path::path_creator::testutil::edit;
    use crate::rng::glibc_rand;

    fn ip(x: i32, y: i32) -> IntPair {
        IntPair { x, y }
    }

    fn mk_bear(fx: &Fx, pos: IntPair, placed: bool) -> (Rc<DropBear>, ObjRef) {
        let b =
            DropBear::init_with_world_dynamic_world_at_position_cache_save_dict_is_adult_was_placed_placed_by_client(
                &fx.wref, &fx.dwr, pos, None, None, true, placed, None,
            )
            .expect("bear");
        let me: ObjRef = b.clone();
        (b, me)
    }

    /// A pine trunk column at x from y0..=y1 (tile type stays air, contents = PINE_TREE_TRUNK).
    fn trunk(fx: &Fx, x: i32, y0: i32, y1: i32) {
        for y in y0..=y1 {
            edit(&fx.world, x, y, |t| t.contents = 7);
        }
    }

    #[test]
    fn tap_within_body_radius_override() {
        let f = fixture(10);
        let (b, me) = mk_bear(&f, ip(20, 13), false);
        let fp = me.float_pos();
        assert!(b.tap_is_within_body_radius_impl(Vector2 { x: fp.x, y: fp.y + 1.0 }));
        assert!(!b.tap_is_within_body_radius_impl(Vector2 { x: fp.x, y: fp.y + 1.3 }));
        assert!(!b.tap_is_within_body_radius_impl(Vector2 { x: fp.x + 0.6, y: fp.y + 0.5 }));
        b.npc.borrow_mut().dead = true;
        assert!(!b.tap_is_within_body_radius_impl(Vector2 { x: fp.x, y: fp.y + 0.5 }));
    }

    /// Tests 1 and 2: layouts and constants.
    #[test]
    fn layouts_and_constants() {
        assert_eq!(std::mem::size_of::<DropBearUpdateNetData>(), 0x28);
        assert_eq!(std::mem::size_of::<DropBearCreationNetData>(), 0x78);
        assert_eq!(DROPBEAR_NAMES.len(), 38);
        assert_eq!(DROPBEAR_NAMES[0], "Ted");
        assert_eq!(DROPBEAR_NAMES[37], "Jazz");
        let f = fixture(10);
        let (b, me) = mk_bear(&f, ip(20, 13), false);
        assert_eq!(me.object_type(), 25);
        assert_eq!(Npc::npc_type(&*b), 2);
        assert_eq!(Npc::max_age(&*b), 7200.0);
        assert_eq!(Npc::species_name(&*b), "DROPBEAR");
        assert_eq!(Npc::food_item_type(&*b), 27);
        assert_eq!(Npc::captured_item_type(&*b), 326);
        assert_eq!(Npc::capture_required_item_type(&*b), 302);
        assert_eq!(Npc::max_health(&*b), 16);
        assert_eq!(Npc::creation_data_struct_size(&*b), 0x78);
        assert_eq!(Npc::get_names_array_count(&*b), 38);
        assert!(!Npc::ridable_when_tamed(&*b));
        assert!(Npc::suffers_damage_at_high_temperatures(&*b));
        let c = me.creation_net_data_for_client(&me, None).unwrap();
        assert_eq!(c.len(), 0x78);
        let u = me.update_net_data_for_client(&me, None).unwrap();
        assert_eq!(u.len(), 0x28);
        assert!(u[0x24..].iter().all(|x| *x == 0));
        assert_eq!(i16::from_le_bytes([u[0x20], u[0x21]]), (b.data().walk_speed * 255.0) as i16);
        assert!(c[0x6c..].iter().all(|x| *x == 0));
        assert_eq!(me.class_info().name, "DropBear");
    }

    /// Test 3: initial values; the RNG draw counts of the init paths.
    #[test]
    fn initial_values_and_rng() {
        let f = fixture(10);
        glibc_rand::srand(1);
        let (b, _me) = mk_bear(&f, ip(20, 13), false);
        assert_eq!(draws_since(1), 5, "NPC adult init 3 + hunger 1 + loadDerivedStuff 1");
        let d = b.data();
        assert_eq!(d.courage_meter, 1.0);
        assert!(!d.dropping);
        assert!(d.walk_speed >= 4.0 && d.walk_speed <= 4.5);
        assert_eq!(d.body_rotation, 3.14159274f32);
        assert_eq!(d.travel_fraction, 1.0);
        assert_eq!(d.from_square, ip(20, 13));
        assert_eq!(d.to_square, ip(20, 13));
        assert_eq!(d.attack_offset, Vector2 { x: 1.0, y: 1.0 });
        drop(d);
        // loadDerivedStuff alone: exactly one draw, walkSpeed = 4.0 + R * 0.5
        glibc_rand::srand(5);
        let s = rand_stream(5, 1);
        glibc_rand::srand(5);
        b.load_derived_stuff();
        assert_eq!(draws_since(5), 1);
        assert_eq!(b.data().walk_speed, 4.0f32 + (s[0] as f32 / 2147483648.0f32) * 0.5f32);
        // placed bear in a place without a tree starts dropping
        let (b2, _m2) = mk_bear(&f, ip(24, 13), true);
        assert!(b2.data().dropping);
        trunk(&f, 26, 11, 15);
        let (b3, _m3) = mk_bear(&f, ip(26, 13), true);
        assert!(!b3.data().dropping);
    }

    /// Test 5 / 7: save dict key set and plist types; the `numberWithBool:(char)` quirk; the round trip.
    #[test]
    fn save_dict_bool_quirk_and_round_trip() {
        let f = fixture(10);
        let (b, me) = mk_bear(&f, ip(20, 13), false);
        {
            let mut d = b.data_mut();
            d.drop_pos = ip(256, -1);
            d.goal_tree_direction = -1;
            d.provoke_meter = 0.5;
            d.dropping = true;
            d.drop_speed = 12.5;
        }
        let d = me.get_save_dict(&me);
        let get = |k: &str| d.map.get(&k.to_string()).unwrap().clone();
        assert!(matches!(get("dropPos.x"), PlistValue::Bool(false)), "256 has a zero low byte");
        assert!(matches!(get("dropPos.y"), PlistValue::Bool(true)));
        assert!(matches!(get("goalTreeDirection"), PlistValue::Bool(true)));
        assert!(matches!(get("dropping"), PlistValue::Bool(true)));
        assert!(matches!(get("onGround"), PlistValue::Bool(false)));
        assert!(matches!(get("provokeMeter"), PlistValue::Real { is_float: true, .. }));
        assert!(matches!(get("courageMeter"), PlistValue::Real { is_float: true, .. }));
        assert!(matches!(get("dropSpeed"), PlistValue::Real { is_float: true, .. }));
        assert!(d.map.get(&"saveTime".to_string()).is_some(), "saveTime comes from the NPC super");
        // reload: bools read back through intValue
        let b2 = DropBear::init_with_world_dynamic_world_save_dict_cache(&f.wref, &f.dwr, &d, None).expect("load");
        let r = b2.data();
        assert_eq!(r.drop_pos, ip(0, 1));
        assert_eq!(r.goal_tree_direction, 1);
        assert_eq!(r.provoke_meter, 0.5);
        assert_eq!(r.drop_speed, 12.5);
        assert!(r.dropping);
    }

    /// Test 6: a bear whose `age + (now - saveTime)` exceeds 7200 is not loaded.
    #[test]
    fn load_old_bear_returns_none() {
        let f = fixture(10);
        let (_b, me) = mk_bear(&f, ip(20, 13), false);
        let mut d = me.get_save_dict(&me);
        d.map.insert("age".to_string(), PlistValue::real_f32(7000.0));
        d.map.insert("saveTime".to_string(), PlistValue::real_f32(0.0));
        f.world.set_world_time(500.0);
        assert!(DropBear::init_with_world_dynamic_world_save_dict_cache(&f.wref, &f.dwr, &d, None).is_none());
        f.world.set_world_time(100.0);
        assert!(DropBear::init_with_world_dynamic_world_save_dict_cache(&f.wref, &f.dwr, &d, None).is_some());
    }

    /// Test 9: courage grows 0.02/s (0.2/s hard); one tile climbed per update while `courage < 0.9`.
    #[test]
    fn courage_and_climbing() {
        let f = fixture(10);
        trunk(&f, 20, 11, 20);
        let (b, me) = mk_bear(&f, ip(20, 13), false);
        b.data_mut().courage_meter = 0.5;
        me.update_accurate_dt_is_simulation(&me, 1.0, 1.0, false);
        assert!((b.data().courage_meter - 0.52).abs() < 1e-6);
        assert_eq!(b.data().to_square, ip(20, 14), "climbs one tile");
        assert!(me.update_needs_to_be_sent());
        // hard rules: ten times faster
        let f2 = fixture(10);
        set_difficulty(&f2, CUSTOM_RULE_NPC_HARD);
        trunk(&f2, 20, 11, 20);
        let (b2, me2) = mk_bear(&f2, ip(20, 13), false);
        b2.data_mut().courage_meter = 0.1;
        me2.update_accurate_dt_is_simulation(&me2, 1.0, 1.0, false);
        assert!((b2.data().courage_meter - 0.3).abs() < 1e-6);
    }

    /// Test 9: exactly 0.9 does nothing; above 0.9 at night with a target below it drops.
    #[test]
    fn ambush_requires_courage_above_point_nine() {
        let setup = |courage: f32, day: f32| {
            let f = fixture(10);
            trunk(&f, 20, 11, 20);
            f.host.day.set(day);
            let (b, me) = mk_bear(&f, ip(20, 13), false);
            f.env.bhs.borrow_mut().push(TestBh::alloc(500, ip(22, 11)));
            b.data_mut().courage_meter = courage;
            me.update_accurate_dt_is_simulation(&me, 0.0, 0.0, false);
            (f, b, me)
        };
        let (_f, b, _m) = setup(0.9, 0.1);
        assert!(!b.data().dropping, "exactly 0.9 does nothing");
        let (_f, b, me) = setup(0.95, 0.1);
        assert!(b.data().dropping);
        assert_eq!(b.data().drop_speed, 10.0);
        assert_eq!(b.data().drop_pos, ip(20, 13));
        assert!(b.data().should_save_when_tree_found);
        assert!(me.update_needs_to_be_sent());
        // daytime: no ambush
        let (_f, b, _m) = setup(0.95, 0.5);
        assert!(!b.data().dropping);
        // lit area: no ambush
        let f = fixture(10);
        trunk(&f, 20, 11, 20);
        f.host.day.set(0.1);
        edit(&f.world, 20, 13, |t| t.artificial_light_r = 0x20);
        let (b, me) = mk_bear(&f, ip(20, 13), false);
        f.env.bhs.borrow_mut().push(TestBh::alloc(500, ip(22, 11)));
        b.data_mut().courage_meter = 0.95;
        me.update_accurate_dt_is_simulation(&me, 0.0, 0.0, false);
        assert!(!b.data().dropping);
        // hard rules ignore night and light
        let f = fixture(10);
        set_difficulty(&f, CUSTOM_RULE_NPC_HARD);
        trunk(&f, 20, 11, 20);
        f.host.day.set(0.9);
        let (b, me) = mk_bear(&f, ip(20, 13), false);
        f.env.bhs.borrow_mut().push(TestBh::alloc(500, ip(22, 11)));
        b.data_mut().courage_meter = 0.95;
        me.update_accurate_dt_is_simulation(&me, 0.0, 0.0, false);
        assert!(b.data().dropping);
        // passive rules: never
        let f = fixture(10);
        set_difficulty(&f, CUSTOM_RULE_NPC_PASSIVE);
        trunk(&f, 20, 11, 20);
        f.host.day.set(0.1);
        let (b, me) = mk_bear(&f, ip(20, 13), false);
        f.env.bhs.borrow_mut().push(TestBh::alloc(500, ip(22, 11)));
        b.data_mut().courage_meter = 0.95;
        me.update_accurate_dt_is_simulation(&me, 0.0, 0.0, false);
        assert!(!b.data().dropping);
    }

    /// Test 9: no tree under the bear: it falls; landing clears `dropping`.
    #[test]
    fn fall_and_landing() {
        let f = fixture(10);
        let (b, me) = mk_bear(&f, ip(24, 20), false);
        me.update_accurate_dt_is_simulation(&me, 0.1, 0.1, false);
        assert!(b.data().dropping);
        assert_eq!(b.data().drop_speed, 10.0);
        assert_eq!(b.data().drop_pos, ip(24, 20));
        assert_eq!(b.data().provoke_meter, 1.0);
        // land: a dropping bear standing right above the ground
        let f2 = fixture(10);
        let (b2, me2) = mk_bear(&f2, ip(24, 11), false);
        b2.data_mut().dropping = true;
        b2.data_mut().drop_speed = 10.0;
        me2.update_accurate_dt_is_simulation(&me2, 0.05, 0.05, false);
        assert!(!b2.data().dropping);
        assert_eq!(me2.float_pos().y, 11.0);
        assert_eq!(b2.data().from_square, ip(24, 11));
    }

    /// Test 9: bite lands at `attackFraction > 0.125`; damage `0.2 * (1 - damage/16)` clamped to 0.02; courage 0.
    #[test]
    fn bite_damage() {
        let run = |dmg: u16| {
            let f = fixture(10);
            let (b, me) = mk_bear(&f, ip(24, 11), false);
            let v = TestBh::alloc(500, ip(24, 11));
            b.npc.borrow_mut().damage = dmg;
            {
                let mut d = b.data_mut();
                d.attacking = true;
                d.attack_fraction = 0.1;
                d.attack_blockhead = Some(v.clone());
                d.attack_pos = ip(24, 11);
                d.courage_meter = 0.7;
            }
            me.update_accurate_dt_is_simulation(&me, 0.01, 0.01, false);
            assert!(f.host.damage.borrow().is_empty(), "0.11 <= 0.125: no bite yet");
            me.update_accurate_dt_is_simulation(&me, 0.05, 0.05, false);
            assert_eq!(b.data().courage_meter, 0.0);
            assert!(b.data().attack_blockhead.is_none());
            me.update_accurate_dt_is_simulation(&me, 0.2, 0.2, false);
            assert!(!b.data().attacking, "attack animation ends after 0.25 s");
            let r = f.host.damage.borrow().clone();
            r
        };
        let r = run(0);
        assert_eq!(r.len(), 1);
        assert_eq!(r[0], (500, 0.2f32, false, true));
        let r = run(16);
        assert_eq!(r[0].1, 0.02f32);
        // the target moved away: no damage, but courage still drops
        let f = fixture(10);
        let (b, me) = mk_bear(&f, ip(24, 11), false);
        let v = TestBh::alloc(500, ip(25, 11));
        {
            let mut d = b.data_mut();
            d.attacking = true;
            d.attack_fraction = 0.2;
            d.attack_blockhead = Some(v);
            d.attack_pos = ip(24, 11);
        }
        me.update_accurate_dt_is_simulation(&me, 0.01, 0.01, false);
        assert!(f.host.damage.borrow().is_empty());
    }

    /// Test 9: death drop counts, normal rules (2 draws) and expert rules (1 draw).
    #[test]
    fn death_drop() {
        let f = fixture(10);
        let (b, me) = mk_bear(&f, ip(24, 11), false);
        b.npc.borrow_mut().dead = true;
        let s = rand_stream(3, 3);
        glibc_rand::srand(3);
        me.update_accurate_dt_is_simulation(&me, 0.1, 0.1, false);
        let fur = f32_to_i32(0.5f32 + (s[0] as f32 / 2147483648.0f32) * 2.0f32);
        let leather = f32_to_i32(1.1f32 + (s[1] as f32 / 2147483648.0f32) * 2.0f32);
        let fb = f.host.free_blocks.borrow().clone();
        assert_eq!(fb.iter().filter(|x| x.1 == 121).count() as i32, fur);
        assert_eq!(fb.iter().filter(|x| x.1 == 120).count() as i32, leather);
        assert!(me.needs_removed());
        assert_eq!(draws_since(3), 2);
        // expert rules: one draw, either 1 leather or 1 fur
        let f = fixture(10);
        f.host.expert.set(true);
        let (b, me) = mk_bear(&f, ip(24, 11), false);
        b.npc.borrow_mut().dead = true;
        let s = rand_stream(4, 2);
        glibc_rand::srand(4);
        me.update_accurate_dt_is_simulation(&me, 0.1, 0.1, false);
        let want = if (s[0] as f32 / 2147483648.0f32) as f64 >= 0.2 { 120 } else { 121 };
        let fb = f.host.free_blocks.borrow().clone();
        assert_eq!(fb.len(), 1);
        assert_eq!(fb[0].1, want);
        assert_eq!(draws_since(4), 1);
    }

    /// Hit, feed, capture rules.
    #[test]
    fn hit_feed_capture() {
        let f = fixture(10);
        let (b, me) = mk_bear(&f, ip(24, 11), false);
        let h = TestBh::alloc(9, ip(25, 11));
        Npc::hit_with_force_blockhead(&*b, &me, 3, Some(&h));
        assert_eq!(b.data().provoke_meter, 1.0);
        assert_eq!(b.npc.borrow().damage, 3);
        Npc::react_to_being_fed(&*b, &me);
        assert_eq!(b.data().walk_speed, 1.0);
        // wild bear: cannot be captured, tip says it must be tamed
        assert!(!Npc::can_be_captured_by_blockhead_with_item_type(&*b, &h, 302));
        assert_eq!(
            Npc::cant_be_captured_tip_string_for_blockhead_with_item_type(&*b, &h, 302).unwrap(),
            "DROPBEARS MUST BE TAMED\nBEFORE YOU CAN CAPTURE THEM"
        );
        assert!(Npc::cant_be_captured_tip_string_for_blockhead_with_item_type(&*b, &h, 9).is_none());
        // tamed to the server owner: a blockhead without clientID belongs
        b.npc.borrow_mut().tamed_client_id = Some("someone".to_string());
        assert_eq!(
            Npc::cant_be_captured_tip_string_for_blockhead_with_item_type(&*b, &h, 302).unwrap(),
            "THAT DROPBEAR BELONGS TO\nSOMEONE ELSE"
        );
        // unloading the target clears the attack target
        b.data_mut().attack_blockhead = Some(h.clone());
        b.blockhead_unloaded(&me, &h);
        assert!(b.data().attack_blockhead.is_none());
    }

    /// Test 4: `startAttack` consumes one draw; the target is retained and `creationDataNeedsToBeSent` set.
    #[test]
    fn start_attack_draw() {
        let f = fixture(10);
        let (b, me) = mk_bear(&f, ip(24, 11), false);
        let v = TestBh::alloc(500, ip(24, 11));
        glibc_rand::srand(2);
        let s = rand_stream(2, 1);
        glibc_rand::srand(2);
        b.start_attack(&me, &v);
        assert_eq!(draws_since(2), 1);
        let d = b.data();
        assert!(d.attacking);
        assert_eq!(d.attack_fraction, 0.0);
        assert_eq!(d.travel_fraction, 0.0);
        assert_eq!(d.attack_pos, ip(24, 11));
        assert_eq!(d.attack_offset.y, v.float_pos().y + s[0] as f32 / 2147483648.0f32);
        assert!(me.creation_data_needs_to_be_sent());
    }

    /// Hunting on the ground: a target one tile away on the same row starts an attack; otherwise the bear steps.
    #[test]
    fn hunting_on_ground() {
        let f = fixture(10);
        let (b, me) = mk_bear(&f, ip(24, 11), false);
        {
            let mut d = b.data_mut();
            d.courage_meter = 1.0;
            d.travel_fraction = 1.0;
        }
        let v = TestBh::alloc(500, ip(25, 11));
        f.env.bhs.borrow_mut().push(v);
        me.update_accurate_dt_is_simulation(&me, 0.01, 0.01, false);
        assert!(b.data().attacking);
        assert!(b.data().on_ground);
        // a far target: the bear takes a step toward it
        let f = fixture(10);
        let (b, me) = mk_bear(&f, ip(24, 11), false);
        let v = TestBh::alloc(500, ip(30, 11));
        f.env.bhs.borrow_mut().push(v);
        b.data_mut().courage_meter = 1.0;
        me.update_accurate_dt_is_simulation(&me, 0.01, 0.01, false);
        assert_eq!(b.data().to_square, ip(25, 11));
        assert!(b.data().on_ground);
        // courage exhausted: it turns back toward dropPos
        let f = fixture(10);
        trunk(&f, 20, 11, 16);
        let (b, me) = mk_bear(&f, ip(24, 11), false);
        {
            let mut d = b.data_mut();
            d.courage_meter = 0.01;
            d.drop_pos = ip(20, 14);
        }
        me.update_accurate_dt_is_simulation(&me, 0.1, 0.1, false);
        assert_eq!(b.data().goal_tree_direction, -1);
        assert_eq!(b.data().to_square, ip(23, 11), "walks toward the home tree");
    }

    /// Rider and protocol plumbing: the registry constructor produces a type-25 bear.
    #[test]
    fn registered_ctor() {
        let f = fixture(10);
        register();
        let o = crate::npc::npc::new_npc_for_type(2, &f.wref, &f.dwr, ip(24, 11), None, None, true, false, None)
            .expect("bear");
        assert_eq!(o.object_type(), 25);
    }

    /// The `isNet` client path of `doRemoteUpdate:` is a no-op on the server.
    #[test]
    fn remote_update_noop_on_server() {
        let f = fixture(10);
        let (b, me) = mk_bear(&f, ip(24, 11), false);
        let mut u = DropBearUpdateNetData::default();
        u.to_square_x = 30;
        u.to_square_y = 11;
        u.dropping = 1;
        me.remote_update(&me, &u.to_vec());
        assert_eq!(b.data().current_net_queue_size, 0);
        assert!(!b.data().dropping);
    }

    /// Tiny deterministic generator for the scenarios (the game's `rand()` stays the bear's own).
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

    /// Scribbles a random wooded landscape around x 14..=34 above the flat ground.
    fn landscape(f: &Fx, g: &mut Gen) {
        for x in 14..=34 {
            let h = 10 + g.below(3) as i32;
            for y in 9..=24 {
                edit(&f.world, x, y, |t| {
                    t.type_index = if y <= h { 1 } else { 2 };
                    t.contents = 0;
                    t.on_fire = 0;
                    t.artificial_light_r = 0;
                    t.partial_content_left = 0;
                });
            }
        }
        for _ in 0..4 {
            let x = 14 + g.below(21) as i32;
            let top = 13 + g.below(8) as i32;
            for y in 11..=top {
                let c = [7u8, 7, 7, 8, 0x1D, 6][g.below(6) as usize];
                edit(&f.world, x, y, |t| {
                    if t.type_index == 2 {
                        t.contents = c
                    }
                });
            }
        }
        for _ in 0..14 {
            let x = 14 + g.below(21) as i32;
            let y = 9 + g.below(14) as i32;
            let k = g.below(8);
            edit(&f.world, x, y, |t| match k {
                0 => t.contents = 0x92,
                1 => {
                    t.type_index = 3;
                    t.partial_content_left = 0x80 + g.below(0x7f) as u8
                }
                2 => t.on_fire = 1,
                3 => t.artificial_light_g = 0x40,
                4 => t.type_index = 1,
                5 => t.type_index = 5,
                6 => t.contents = 7,
                _ => t.foreground_contents = 0x42,
            });
        }
    }

    #[test]
    fn seeded_forests_cover_the_state_machine() {
        for seed in 1..=500u64 {
            let f = fixture(10);
            let mut g = Gen(seed.wrapping_mul(0x9E3779B97F4A7C15) | 1);
            landscape(&f, &mut g);
            glibc_rand::srand(seed as u32);
            let pos = ip(16 + g.below(17) as i32, 11 + g.below(8) as i32);
            let placed = g.below(3) == 0;
            let (b, me) = mk_bear(&f, pos, placed);
            if g.below(6) == 0 {
                me.base().borrow_mut().is_net = true;
            }
            {
                let mut d = b.data_mut();
                d.courage_meter = [0.01, 0.3, 0.85, 0.95, 1.0][g.below(5) as usize];
                d.provoke_meter = [0.0, 0.0, 1.0][g.below(3) as usize];
                d.dropping = g.below(5) == 0;
                d.drop_speed = 10.0;
                d.drop_pos = ip(14 + g.below(21) as i32, 11 + g.below(6) as i32);
                d.goal_tree_direction = if g.below(2) == 0 { 1 } else { -1 };
                d.on_ground = g.below(3) == 0;
                d.should_save_when_tree_found = g.below(3) == 0;
                d.drown_timer = if g.below(5) == 0 { 7.9 } else { 0.0 };
                d.current_net_queue_size = g.below(5) as i32;
                for i in 0..4 {
                    d.next_to_square[i] = ip(14 + g.below(21) as i32, 11 + g.below(8) as i32);
                    d.next_walk_speed[i] = 1.0 + g.below(6) as f32;
                    d.next_on_ground[i] = g.below(2) == 0;
                }
                if g.below(4) == 0 {
                    d.from_square = pos;
                    d.to_square = ip(pos.x + g.below(3) as i32 - 1, pos.y + g.below(3) as i32 - 1);
                }
            }
            b.npc.borrow_mut().fullness = [100.0, 2000.0, 3000.0][g.below(3) as usize];
            if g.below(5) == 0 {
                b.npc.borrow_mut().tamed_client_id = Some("server".into());
            }
            let mut bhs = vec![];
            for i in 0..g.below(4) {
                let v = TestBh::alloc(500 + i as u64, ip(pos.x + g.below(9) as i32 - 4, pos.y + g.below(7) as i32 - 3));
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
            if g.below(6) == 0 {
                f.host.regen.set(Some(500));
            }
            if g.below(8) == 0 {
                f.host.trade.set(Some(501));
            }
            f.host.npc_at.set(g.below(3) == 0);
            f.host.day.set(if g.below(2) == 0 { 0.1 } else { 0.6 });
            f.host.expert.set(g.below(4) == 0);
            match g.below(5) {
                0 => set_difficulty(&f, CUSTOM_RULE_NPC_HARD),
                1 => set_difficulty(&f, CUSTOM_RULE_NPC_PASSIVE),
                _ => {}
            }
            if g.below(7) == 0 {
                if let Some(v) = bhs.first() {
                    b.begin_blockhead_inspection(v);
                }
            }
            if g.below(7) == 0 {
                let v = bhs.first().cloned();
                let mut d = b.data_mut();
                d.attacking = true;
                d.attack_fraction = 0.0;
                d.attack_pos = v.as_ref().map_or(ip(0, 0), |v| v.pos());
                d.attack_blockhead = v;
            }
            let dts = [0.05f32, 0.2, 0.6, 1.5];
            let dt = dts[g.below(4) as usize];
            for _ in 0..120 {
                if me.needs_removed() {
                    break;
                }
                if g.below(60) == 0 {
                    b.npc.borrow_mut().dead = true;
                }
                me.update_accurate_dt_is_simulation(&me, dt, dt, g.below(2) == 0);
            }
        }
    }

    #[test]
    fn ambush_path_check_over_random_slopes() {
        for seed in 1..=700u64 {
            let f = fixture(10);
            let mut g = Gen(seed.wrapping_mul(0xD1B54A32D192ED03) | 1);
            f.host.day.set(0.1);
            // a tall tree at x 24 over uneven ground
            for x in 12..=38 {
                let h = 10 + g.below(4) as i32;
                for y in 9..=24 {
                    edit(&f.world, x, y, |t| {
                        t.type_index = if y <= h { 1 } else { 2 };
                        t.contents = 0;
                        t.artificial_light_r = 0;
                    });
                }
            }
            for y in 11..=20 {
                edit(&f.world, 24, y, |t| {
                    t.type_index = 2;
                    t.contents = 7
                });
            }
            for _ in 0..g.below(5) {
                let x = 12 + g.below(27) as i32;
                let y = 11 + g.below(5) as i32;
                let k = g.below(3);
                edit(&f.world, x, y, |t| match k {
                    0 => t.contents = 0x92,
                    1 => t.type_index = 3,
                    _ => t.type_index = 1,
                });
            }
            let (b, me) = mk_bear(&f, ip(24, 19), false);
            b.data_mut().courage_meter = 0.95;
            let tx = 24 + g.below(41) as i32 - 20;
            let ty = 11 + g.below(5) as i32;
            f.env.bhs.borrow_mut().push(TestBh::alloc(500, ip(tx, ty)));
            if g.below(3) == 0 {
                f.env
                    .bhs
                    .borrow_mut()
                    .push(TestBh::alloc(501, ip(24 + g.below(13) as i32 - 6, 11 + g.below(4) as i32)));
            }
            me.update_accurate_dt_is_simulation(&me, 0.0, 0.0, false);
        }
    }

    #[test]
    fn unloaded_surroundings_and_removed_bear_do_nothing() {
        let f = fixture(10);
        let (b, me) = mk_bear(&f, ip(24, 11), false);
        me.base().borrow_mut().needs_removed = true;
        me.update_accurate_dt_is_simulation(&me, 0.1, 0.1, false);
        assert!(!b.data().dropping);
        let f = fixture(10);
        let (_b, me) = mk_bear(&f, ip(24, 11), false);
        me.base().borrow_mut().pos = ip(24, 100000);
        me.update_accurate_dt_is_simulation(&me, 0.1, 0.1, false);
        assert_eq!(me.pos(), ip(24, 100000));
    }

    #[test]
    fn faces_net_replica_and_remote_updates() {
        let f = fixture(10);
        trunk(&f, 20, 11, 20);
        let (b, me) = mk_bear(&f, ip(20, 13), false);
        let bh: ObjRef = TestBh::alloc(77, ip(21, 13));
        crate::npc::npc::faces::exercise_faces(&me, &bh);
        // replica from the creation data
        let nd = me.creation_net_data_for_client(&me, None).unwrap();
        let rep = DropBear::init_with_world_dynamic_world_cache_net_data(&f.wref, &f.dwr, None, &nd).expect("replica");
        assert!(rep.base.borrow().is_net);
        assert_eq!(rep.data().next_to_square[3], ip(20, 13));
        // an unloaded position is refused
        let mut bad = nd.clone();
        bad[12..16].copy_from_slice(&100000u32.to_le_bytes());
        assert!(DropBear::init_with_world_dynamic_world_cache_net_data(&f.wref, &f.dwr, None, &bad).is_none());
        // client: updates are queued, a drop starts the fall at once
        f.env.client.set(true);
        let mut u = DropBearUpdateNetData {
            to_square_x: 21,
            to_square_y: 13,
            walk_speed: 51,
            ..DropBearUpdateNetData::default()
        };
        for _ in 0..6 {
            b.do_remote_update(&me, &u);
        }
        assert_eq!(b.data().current_net_queue_size, 4);
        assert_eq!(b.data().next_to_square[3], ip(21, 13));
        u.dropping = 1;
        me.remote_update(&me, &u.to_vec());
        assert!(b.data().dropping);
        assert_eq!(b.data().drop_pos, ip(20, 13));
        assert_eq!(b.data().current_net_queue_size, 0);
        // creation data takes the same path; a dead bear ignores both
        b.data_mut().dropping = false;
        let mut cd = nd.clone();
        cd[0x48 + 0x24] = 1;
        me.remote_creation_data_update(&me, &cd);
        b.npc.borrow_mut().dead = true;
        me.remote_update(&me, &u.to_vec());
        me.remote_creation_data_update(&me, &cd);
        // the capture tip when the bear belongs to the asker
        b.npc.borrow_mut().tamed_client_id = Some("server".to_string());
        assert!(Npc::can_be_captured_by_blockhead_with_item_type(&*b, &bh, 302));
        assert!(Npc::cant_be_captured_tip_string_for_blockhead_with_item_type(&*b, &bh, 302).is_none());
        // unloading some other blockhead keeps the target
        b.data_mut().attack_blockhead = Some(bh.clone());
        let other: ObjRef = TestBh::alloc(78, ip(0, 0));
        b.blockhead_unloaded(&me, &other);
        assert!(b.data().attack_blockhead.is_some());
        let mut wm = vec![];
        me.world_changed(&me, &mut wm);
    }

    #[test]
    fn tap_hit_test_wraps_around_the_world() {
        let f = fixture(10);
        let (b, me) = mk_bear(&f, ip(2, 13), false);
        let fp = me.float_pos();
        // 128 tiles wide: a tap at the far side of the seam still hits
        assert!(b.tap_is_within_body_radius_impl(Vector2 { x: fp.x + 128.0, y: fp.y + 0.5 }));
        assert!(b.tap_is_within_body_radius_impl(Vector2 { x: fp.x - 128.0, y: fp.y + 0.5 }));
        assert!(Npc::tap_is_within_body_radius(&*b, Vector2 { x: fp.x, y: fp.y + 0.5 }));
    }

    #[test]
    fn ambush_path_check_gives_up() {
        // water straight below the bear: not a descent
        let f = fixture(10);
        f.host.day.set(0.1);
        trunk(&f, 24, 11, 20);
        edit(&f.world, 24, 18, |t| {
            t.type_index = 3;
            t.contents = 7
        });
        let (b, me) = mk_bear(&f, ip(24, 19), false);
        b.data_mut().courage_meter = 0.95;
        f.env.bhs.borrow_mut().push(TestBh::alloc(500, ip(22, 11)));
        me.update_accurate_dt_is_simulation(&me, 0.0, 0.0, false);
        assert!(!b.data().dropping);
        // a drop taller than the 32-step budget
        let f = fixture(10);
        f.host.day.set(0.1);
        for y in 11..=60 {
            edit(&f.world, 24, y, |t| t.contents = 7);
        }
        let (b, me) = mk_bear(&f, ip(24, 41), false);
        b.data_mut().courage_meter = 0.95;
        f.env.bhs.borrow_mut().push(TestBh::alloc(500, ip(28, 11)));
        me.update_accurate_dt_is_simulation(&me, 0.0, 0.0, false);
        assert!(!b.data().dropping);
    }

    #[test]
    fn attack_is_dropped_when_the_target_is_gone() {
        let f = fixture(10);
        let (b, me) = mk_bear(&f, ip(24, 11), false);
        let v = TestBh::alloc(500, ip(24, 11));
        v.base().borrow_mut().needs_removed = true;
        {
            let mut d = b.data_mut();
            d.attacking = true;
            d.attack_blockhead = Some(v);
        }
        me.update_accurate_dt_is_simulation(&me, 0.01, 0.01, false);
        assert!(b.data().attack_blockhead.is_none());
        Npc::successful_tame(&*b, &me);
    }
}
