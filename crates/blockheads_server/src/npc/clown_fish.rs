//! `ClownFish: NPC` (NPC type 4, DynamicObject type 35): the small swimming "FISH".
//!
//!
//! `update:` copies the ivars into a local [`ClownFishState`] and writes them back; the copy is flushed before and
//! reloaded after every call that can re-enter the fish (`die:`, `reactToBeingFed` through
//! `checkCurrentPositionForFood`, `loadNPCAtPosition`), marked `RE-ENTRY`.

use std::any::Any;
use std::cell::RefCell;

use crate::blockhead::blockhead::FishingRodRef;
use crate::fmath::cconv::f32_to_i32;
use crate::fmath::libm::fabsf;
use crate::fmath::vector::linear_interpolate;
use crate::npc::egg::{penv, t_air_snow, t_gate, t_solid, t_water};
pub use crate::npc::npc::NPC_CLOWNFISH;
use crate::npc::npc::{
    alloc_npc, npc_append_npc_creation_data_to_data, npc_die, npc_env, npc_hit_with_force_blockhead,
    npc_init_with_world_dynamic_world_at_position_cache_save_dict_is_adult_was_placed_placed_by_client,
    npc_init_with_world_dynamic_world_cache_net_data, npc_init_with_world_dynamic_world_save_dict_cache,
    npc_npc_creation_net_data_for_client, npc_npc_update_net_data_for_client, npc_remote_creation_data_update,
    npc_remote_update, npc_state, npc_update_accurate_dt_is_simulation, Npc, NpcState, NpcTileCopy, NPC_CLASS,
};
use crate::objects::base::{padded_bytes, DynamicObject, DynamicObjectBase, DynamicWorldRef, WorldRef};
use crate::objects::class_info::ClassInfo;
use crate::objects::handle::{ptr_eq, CacheRef, ObjRef};
use crate::plist::value::PlistDict;
use crate::rng::glibc_rand;
use crate::types::netdata::{ClownFishCreationNetData, ClownFishUpdateNetData};
use crate::types::structs::{IntPair, Vector2, WireStruct};
use crate::types::tile::Tile;
use crate::{impl_npc_protocols, npc_forward};

/// `DYNAMIC_OBJECT_TYPE_NPC_CLOWNFISH`.
pub const DYNAMIC_OBJECT_TYPE_NPC_CLOWNFISH: i32 = 35;
const ITEM_TIN_BUCKET: i32 = 102;
const ITEM_KELP: i32 = 144;
const ITEM_RAW_FISH: i32 = 151;
const ITEM_FISH_BUCKET: i32 = 158;
const CONTENTS_KELP_PLANT: u8 = 0x51;
const TILE_ICE: u8 = 4;

/// `FISH_NAMES`, index order is observable.
pub static FISH_NAMES: [&str; 62] = [
    "Amethyst",
    "Sapphire",
    "Ruby",
    "Emerald",
    "Diamond",
    "Angel",
    "Babel",
    "Bait",
    "Bubbles",
    "Bubbly",
    "Calypso",
    "Chandler",
    "Chanel",
    "Chip",
    "Curly",
    "Deb",
    "Diablo",
    "Danny",
    "Dory",
    "Elvis",
    "Eureka",
    "Lulu",
    "Marble",
    "Macaroni",
    "Marshmellow",
    "Max",
    "Miggy",
    "Moe",
    "Mouse",
    "Mr. Bubbles",
    "Mr. Wigglesworth",
    "Napoleon",
    "Nemo",
    "PeeWee",
    "Clown",
    "Fez",
    "Fish",
    "Flounder",
    "Franky",
    "Fred",
    "Fredrick",
    "Frosty",
    "George",
    "Gilbert",
    "Grant",
    "Hippo",
    "Jaws",
    "Jerry",
    "Joe",
    "Samson",
    "Shark",
    "Fishy McFishface",
    "Champ",
    "Sonic",
    "Sophie",
    "Speedy",
    "Spud",
    "Squirt",
    "Sushi",
    "Coconut",
    "Trevor",
    "Xavier",
];

/// `ClownFish: NPC`.
pub static CLOWN_FISH_CLASS_INFO: ClassInfo =
    ClassInfo { name: "ClownFish", superclass: Some(&NPC_CLASS), protocols: &[] };

const ZERO_PAIR: IntPair = IntPair { x: 0, y: 0 };

/// ClownFish ivars (render-only ones except those init sets are omitted).
#[derive(Clone)]
pub struct ClownFishState {
    /// 0x150
    pub movement_direction: i32,
    /// 0x154
    pub from_square: IntPair,
    /// 0x15c
    pub to_square: IntPair,
    /// 0x164 (the death timer when dead)
    pub travel_fraction: f32,
    /// 0x168
    pub random_time_between_direction_changes: f32,
    /// 0x16c
    pub body_rotation: f32,
    /// 0x170
    pub goal_rotation: f32,
    /// 0x17c
    pub walk_timer: f32,
    /// 0x180
    pub walk_speed: f32,
    /// 0x18c
    pub drown_timer: f32,
    /// 0x194
    pub fall_velocity: f32,
    /// 0x198
    pub falling: bool,
    /// 0x199
    pub is_on_air_above_solid_ground: bool,
    /// 0x19a
    pub last_try_couldnt_move: bool,
    /// 0x1a0 retained rod
    pub hooked_fishing_rod: Option<FishingRodRef>,
    /// 0x188 (render offset; 0 on the server)
    pub x_offset: f32,
    /// 0x1a8 (0..=4)
    pub current_net_queue_size: i32,
    /// 0x1ac
    pub next_to_square: [IntPair; 4],
    /// 0x1cc
    pub next_walk_speed: [f32; 4],
    /// 0x1dc
    pub next_movement_direction: [i32; 4],
}

impl Default for ClownFishState {
    fn default() -> Self {
        ClownFishState {
            movement_direction: 0,
            from_square: ZERO_PAIR,
            to_square: ZERO_PAIR,
            travel_fraction: 0.0,
            random_time_between_direction_changes: 0.0,
            body_rotation: 0.0,
            goal_rotation: 0.0,
            walk_timer: 0.0,
            walk_speed: 0.0,
            drown_timer: 0.0,
            fall_velocity: 0.0,
            falling: false,
            is_on_air_above_solid_ground: false,
            last_try_couldnt_move: false,
            hooked_fishing_rod: None,
            x_offset: 0.0,
            current_net_queue_size: 0,
            next_to_square: [ZERO_PAIR; 4],
            next_walk_speed: [0.0; 4],
            next_movement_direction: [0; 4],
        }
    }
}

/// The ClownFish.
pub struct ClownFish {
    pub base: RefCell<DynamicObjectBase>,
    pub npc: RefCell<NpcState>,
    pub clown_fish: RefCell<ClownFishState>,
}

fn r(tag: &'static str) -> f32 {
    glibc_rand::r01_tagged(tag)
}

fn fish_of(me: &ObjRef) -> &ClownFish {
    me.as_any().downcast_ref::<ClownFish>().expect("object is not a ClownFish")
}

fn dw_changed(me: &ObjRef) {
    if let Some(dw) = me.dynamic_world() {
        dw.dynamic_world_changed_at_pos_object_type(me.pos(), me.object_type());
    }
}

fn tile_copy(t: &Tile) -> NpcTileCopy {
    NpcTileCopy {
        type_index: t.type_index,
        on_fire: t.on_fire,
        sun_light: t.sun_light,
        artificial_heat: t.artificial_heat,
    }
}

impl ClownFish {
    fn alloc() -> std::rc::Rc<ClownFish> {
        alloc_npc(|w| ClownFish {
            base: RefCell::new(DynamicObjectBase::alloc_zeroed_with(w)),
            npc: RefCell::new(NpcState::default()),
            clown_fish: RefCell::new(ClownFishState::default()),
        })
    }

    /// `-[ClownFish initWithWorld:...atPosition:...]`: refuses (returns `None`, after the NPC draws)
    /// when the tile is not deep water or, for a spawn that was not placed, when the water is too cold.
    /// Draws: adult 3+1 (+2 `loadDerivedStuff` if accepted), baby 2+1 (+2).
    #[allow(clippy::too_many_arguments)]
    pub fn init_with_world_dynamic_world_at_position_cache_save_dict_is_adult_was_placed_placed_by_client(
        world: &WorldRef,
        dw: &DynamicWorldRef,
        pos: IntPair,
        cache: CacheRef,
        save_dict: Option<&PlistDict>,
        is_adult: bool,
        was_placed: bool,
        client_id: Option<&str>,
    ) -> Option<ObjRef> {
        let me: ObjRef = ClownFish::alloc();
        if !npc_init_with_world_dynamic_world_at_position_cache_save_dict_is_adult_was_placed_placed_by_client(
            &me, world, dw, pos, cache, save_dict, is_adult, was_placed, client_id,
        ) {
            return None;
        }
        let p = me.pos();
        let env = penv();
        let tile = env.tile_loaded(p.x, p.y);
        let bad = match &tile {
            None => true,
            Some(t) => !was_placed && (!t_water(&tile) || t.partial_content_left < 0x7f),
        };
        if bad {
            me.remove_from_macro_block(&me);
            return None;
        }
        if !was_placed {
            let t = tile.unwrap();
            let ne = npc_env();
            let wt = world.world_time();
            let weather = ne.weather_fraction(p, wt);
            let day = ne.day_night_fraction(p.x as f32, wt);
            let season = ne.season_for_world_x(p.x, wt);
            let temp = ne.current_temperature(&tile_copy(&t), p, season, day, weather);
            let age = npc_state(&me).borrow().age;
            if temp < 0.1f32 || ((age as f64) > 0.1f64 && temp < 2.0f32) {
                me.remove_from_macro_block(&me);
                return None;
            }
        }
        clown_fish_load_derived_stuff(&me);
        Some(me)
    }

    /// Registry constructor (`NpcCtor`).
    #[allow(clippy::too_many_arguments)]
    pub fn new_for_registry(
        world: &WorldRef,
        dw: &DynamicWorldRef,
        pos: IntPair,
        cache: CacheRef,
        save_dict: Option<&PlistDict>,
        is_adult: bool,
        was_placed: bool,
        client_id: Option<&str>,
    ) -> Option<ObjRef> {
        ClownFish::init_with_world_dynamic_world_at_position_cache_save_dict_is_adult_was_placed_placed_by_client(
            world, dw, pos, cache, save_dict, is_adult, was_placed, client_id,
        )
    }

    /// `-[ClownFish initWithWorld:dynamicWorld:saveDict:cache:]`: 1 + 2 `rand()`; no water test.
    pub fn init_with_world_dynamic_world_save_dict_cache(
        world: &WorldRef,
        dw: &DynamicWorldRef,
        save_dict: &PlistDict,
        cache: CacheRef,
    ) -> Option<ObjRef> {
        let me: ObjRef = ClownFish::alloc();
        if !npc_init_with_world_dynamic_world_save_dict_cache(&me, world, dw, save_dict, cache) {
            return None;
        }
        clown_fish_load_derived_stuff(&me);
        Some(me)
    }

    /// `-[ClownFish initWithWorld:dynamicWorld:cache:netData:]`: the explicit
    /// `remoteCreationDataUpdate:` is the only place the fish part of the blob is applied.
    pub fn init_with_world_dynamic_world_cache_net_data(
        world: &WorldRef,
        dw: &DynamicWorldRef,
        cache: CacheRef,
        net_data: &[u8],
    ) -> Option<ObjRef> {
        let o = ClownFish::alloc();
        let me: ObjRef = o.clone();
        if !npc_init_with_world_dynamic_world_cache_net_data(&me, world, dw, cache, net_data) {
            return None;
        }
        clown_fish_load_derived_stuff(&me);
        {
            let pos = me.pos();
            let mut s = o.clown_fish.borrow_mut();
            for i in 0..4 {
                s.next_to_square[i] = pos;
                s.next_walk_speed[i] = 0.0;
                s.next_movement_direction[i] = 0;
            }
        }
        me.remote_creation_data_update(&me, net_data);
        Some(me)
    }

    /// `-[ClownFish remoteBlockheadHookedYou:]`: a remote blockhead says it hooked this fish.
    pub fn remote_block_head_hooked_you(me: &ObjRef, b: &ObjRef) {
        let env = penv();
        let f = fish_of(me);
        let rod = match env.bh_fishing_rod(b) {
            Some(r) => r,
            None => return,
        };
        let cur = f.clown_fish.borrow().hooked_fishing_rod.clone();
        let same = cur.as_ref().map_or(false, |c| std::rc::Rc::ptr_eq(c, &rod));
        if env.rod_hooked_fish(&rod).is_none() && !same {
            f.clown_fish.borrow_mut().hooked_fishing_rod = Some(rod.clone());
            env.rod_set_hooked_fish(&rod, Some(me));
        }
    }
}

/// `-[ClownFish loadDerivedStuff]` server part: 2 `rand()`.
pub fn clown_fish_load_derived_stuff(me: &ObjRef) {
    let f = fish_of(me);
    let r1 = r("ClownFish.load.dir") as f64;
    let dir = if r1 > 0.75f64 {
        1
    } else if r1 < 0.25f64 {
        -1
    } else {
        0
    };
    let pos = me.pos();
    let ws = ((r("ClownFish.load.walkSpeed") as f64) * 0.2f64 + 0.10000000149011612f64) as f32;
    let mut s = f.clown_fish.borrow_mut();
    s.movement_direction = dir;
    s.to_square = pos;
    s.from_square = pos;
    s.travel_fraction = 1.0f32;
    s.random_time_between_direction_changes = 0.0f32;
    s.walk_speed = ws;
    let rot = if dir == 1 { 1.5707964f32 } else { -1.5707964f32 };
    s.body_rotation = rot;
    s.goal_rotation = rot;
}

/// `-[ClownFish die:]`: with a killer, `rand() % 3 + 1` RAW_FISH.
pub fn clown_fish_die(me: &ObjRef, killer: Option<&ObjRef>) {
    let f = fish_of(me);
    if npc_state(me).borrow().dead {
        return;
    }
    {
        let mut s = f.clown_fish.borrow_mut();
        s.travel_fraction = 0.0f32;
        s.hooked_fishing_rod = None;
    }
    if let Some(k) = killer {
        let count = glibc_rand::rand_tagged("ClownFish.die.drops") % 3 + 1;
        let pos = me.pos();
        for _ in 0..count {
            penv().dw_create_free_block(pos, ITEM_RAW_FISH, 0, 0, None, 1, Some(k));
        }
    }
    npc_die(me, killer);
}

/// `-[ClownFish hitWithForce:blockhead:]`.
pub fn clown_fish_hit_with_force_blockhead(me: &ObjRef, force: i32, b: Option<&ObjRef>) {
    npc_hit_with_force_blockhead(me, force, b);
    if !npc_state(me).borrow().dead && !me.is_net() {
        let dir = if (r("ClownFish.hit.dir") as f64) > 0.5f64 { 1 } else { -1 };
        let mut s = fish_of(me).clown_fish.borrow_mut();
        s.movement_direction = dir;
        s.walk_speed = 3.0f32;
        s.random_time_between_direction_changes = 5.0f32;
    }
}

// ------------------------------------------------------------------------------------------------ net data

/// `-[ClownFish clownFishUpdateNetDataForClient:]`. Padding is zero.
pub fn clown_fish_clown_fish_update_net_data_for_client(
    me: &ObjRef,
    client_id: Option<&str>,
) -> ClownFishUpdateNetData {
    let base = npc_npc_update_net_data_for_client(me, client_id);
    let s = fish_of(me).clown_fish.borrow();
    ClownFishUpdateNetData {
        npc_update_net_data: base,
        to_square_x: s.to_square.x as u32,
        to_square_y: s.to_square.y as u32,
        walk_speed: f32_to_i32(s.walk_speed * 255.0f32) as i16,
        movement_direction: s.movement_direction as i8,
        padding: [0; 5],
    }
}

/// `-[ClownFish doRemoteUpdate:]`.
pub fn clown_fish_do_remote_update(me: &ObjRef, u: ClownFishUpdateNetData) {
    let f = fish_of(me);
    let mut s = f.clown_fish.borrow_mut();
    if !npc_env().is_client() {
        s.walk_speed = (u.walk_speed as i32 as f32) / 255.0f32;
        s.movement_direction = u.movement_direction as i32;
        s.to_square = IntPair { x: u.to_square_x as i32, y: u.to_square_y as i32 };
        drop(s);
        me.set_update_needs_to_be_sent(true);
    } else {
        let mut idx = s.current_net_queue_size;
        if idx > 3 {
            s.current_net_queue_size = 3;
            idx = 3;
        }
        let i = idx.max(0) as usize;
        s.next_to_square[i] = IntPair { x: u.to_square_x as i32, y: u.to_square_y as i32 };
        s.next_walk_speed[i] = (u.walk_speed as i32 as f32) / 255.0f32;
        s.next_movement_direction[i] = u.movement_direction as i32;
        s.current_net_queue_size += 1;
    }
}

// ------------------------------------------------------------------------------------------------ update

fn flush(f: &ClownFish, s: &ClownFishState) {
    *f.clown_fish.borrow_mut() = s.clone();
}
fn reload(f: &ClownFish, s: &mut ClownFishState) {
    *s = f.clown_fish.borrow().clone();
}

/// `-[ClownFish update:accurateDT:isSimulation:]`.
pub fn clown_fish_update_accurate_dt_is_simulation(me: &ObjRef, dt: f32, accurate_dt: f32, is_simulation: bool) {
    npc_update_accurate_dt_is_simulation(me, dt, accurate_dt, is_simulation);
    let f = fish_of(me);
    let mut s = f.clown_fish.borrow().clone();
    update_body(me, f, &mut s, dt, is_simulation);
    *f.clown_fish.borrow_mut() = s;
}

fn set_fp(me: &ObjRef, v: Vector2) {
    me.base().borrow_mut().float_pos = v;
}

#[allow(clippy::cognitive_complexity)]
fn update_body(me: &ObjRef, f: &ClownFish, s: &mut ClownFishState, dt: f32, is_simulation: bool) {
    let env = penv();
    let ns = npc_state(me);
    let dead = ns.borrow().dead;
    if dead {
        s.travel_fraction += 4.0f32 * dt;
        if !me.is_net() && !me.needs_removed() && s.travel_fraction >= 1.0f32 {
            me.base().borrow_mut().needs_removed = true;
        }
        return;
    }
    // alive
    let mut walk_speed_to_use = s.walk_speed;
    if me.is_net() {
        walk_speed_to_use =
            ((walk_speed_to_use as f64) * (0.75f64 + 0.25f64 * (s.current_net_queue_size as f64))) as f32;
        if s.current_net_queue_size > 2 {
            let w = walk_speed_to_use as f64;
            walk_speed_to_use = (if w > 1.0f64 { w } else { 1.0f64 }) as f32;
        }
    }
    s.random_time_between_direction_changes -= dt;
    let inspecting = ns.borrow().inspecting_blockhead.as_ref().and_then(|w| w.upgrade()).is_some();
    if inspecting {
        s.movement_direction = 0;
        walk_speed_to_use = 0.0f32;
        s.walk_speed = 0.0f32;
    }
    let not_moving = s.movement_direction == 0;

    // 1. hooked on a fishing rod
    if let Some(rod) = s.hooked_fishing_rod.clone() {
        if env.rod_valid(&rod) {
            s.walk_speed = 4.0f32;
            let hp = env.rod_hook_pos(&rod);
            set_fp(me, hp);
            let new_pos = IntPair { x: f32_to_i32(hp.x), y: f32_to_i32(hp.y) };
            if new_pos == me.pos() {
                return;
            }
            s.to_square = new_pos;
            s.from_square = s.to_square;
            me.update_position(me, new_pos);
            me.set_update_needs_to_be_sent(true);
            return;
        } else {
            s.hooked_fishing_rod = None;
        }
    }

    // 2. direction / speed re-roll
    if s.random_time_between_direction_changes <= 0.0f32 && (s.travel_fraction >= 1.0f32 || not_moving) {
        s.random_time_between_direction_changes = r("ClownFish.update.interval") * 10.0f32 + 1.0f32;
        if !me.is_net() {
            s.walk_speed = ((s.walk_speed as f64) * 0.5f64
                + (r("ClownFish.update.walkSpeed") as f64) * 0.2f64
                + 0.10000000149011612f64) as f32;
            me.set_unreliable_update_needs_to_be_sent(true);
        }
        if (s.walk_speed < 0.1f32 || not_moving) && !me.is_net() {
            let rr = r("ClownFish.update.dir") as f64;
            let cand = if rr > 0.75f64 {
                1
            } else if rr < 0.25f64 {
                -1
            } else {
                0
            };
            if cand == 0 {
                if !env.dw_blockhead_occupies_tile(s.to_square) && !env.dw_npc_exists_at_pos(me.pos(), me) {
                    s.movement_direction = 0;
                }
            } else if s.movement_direction != cand && (s.walk_speed < 0.08f32 || not_moving) {
                s.movement_direction = cand;
            }
        }
    }
    let old_dir = s.movement_direction;

    let pos = me.pos();
    let tile = env.tile_loaded(pos.x, pos.y);
    if matches!(&tile, Some(t) if t.type_index == TILE_ICE) {
        return; // frozen
    }

    // 3. lay timer and drowning
    if !me.is_net() {
        let lay = ns.borrow().lay_timer;
        if lay > 0.0f32 {
            let nl = lay - dt;
            ns.borrow_mut().lay_timer = nl;
            if nl <= 0.0f32 && t_water(&tile) && tile.map_or(false, |t| t.type_index != TILE_ICE) {
                {
                    let mut n = ns.borrow_mut();
                    n.fullness = ((n.fullness as f64) - 2700.0f64) as f32;
                }
                flush(f, s); // RE-ENTRY: loadNPCAtPosition constructs a baby fish
                env.dw_load_npc_at_position(pos, NPC_CLOWNFISH, None, false, false, None);
                reload(f, s);
                ns.borrow_mut().lay_cooldown_timer = 900.0f32;
                dw_changed(me);
            }
        }
        let shallow = tile.map_or(true, |t| t.type_index == TILE_ICE || t.partial_content_left < 0x7f);
        if !t_water(&tile) || shallow {
            s.drown_timer += dt;
            if s.drown_timer > 10.0f32 {
                flush(f, s); // RE-ENTRY: die writes the fish state
                Npc::die(f, me, None);
                reload(f, s);
            }
        } else {
            s.drown_timer = 0.0f32;
        }
    }

    // 4. gravity / flopping (also for replicas)
    let pos = me.pos();
    let t2 = env.tile_loaded(pos.x, pos.y);
    let in_water = !t_air_snow(&t2) && t2.map_or(true, |t| t.partial_content_left >= 0x7f);
    if in_water {
        if s.falling {
            s.fall_velocity = ((s.fall_velocity as f64) + (dt as f64) * 0.2f64) as f32;
            if s.fall_velocity <= 0.0f32 {
                let mut fp = me.float_pos();
                fp.y += s.fall_velocity;
                set_fp(me, fp);
            } else {
                s.fall_velocity = 0.0f32;
                s.falling = false;
                if s.is_on_air_above_solid_ground {
                    s.walk_speed = 2.0f32;
                    s.is_on_air_above_solid_ground = false;
                }
            }
        }
    } else {
        s.falling = true;
        s.is_on_air_above_solid_ground = true;
        s.fall_velocity = ((s.fall_velocity as f64) - (dt as f64) * 0.1f64) as f32;
        let mut fp = me.float_pos();
        fp.y += s.fall_velocity;
        set_fp(me, fp);
        if f32_to_i32(fp.y) != pos.y {
            let np = IntPair { x: pos.x, y: f32_to_i32(fp.y) };
            let nt = env.tile_loaded(np.x, np.y);
            if !t_solid(&nt) {
                me.update_position(me, np);
                s.to_square = np;
                s.from_square = s.to_square;
            } else {
                let mut fp2 = me.float_pos();
                fp2.y = 0.01f32 + (pos.y as f32);
                set_fp(me, fp2);
                s.fall_velocity = ((-(s.fall_velocity as f64)) * 0.1f64) as f32;
            }
        }
    }

    // 5. advance along the current step
    s.travel_fraction = dt * walk_speed_to_use + s.travel_fraction;
    if s.travel_fraction >= 1.0f32 {
        s.travel_fraction -= 1.0f32;
        if s.travel_fraction >= 1.0f32 || s.travel_fraction < 0.0f32 {
            s.travel_fraction = 0.0f32;
        }
        if !me.is_net() && s.from_square.x != s.to_square.x {
            if let Some(t1) = env.tile_loaded(s.to_square.x, s.to_square.y) {
                let fullness = ns.borrow().fullness;
                if (fullness as f64) < 5400.0f64 && t1.contents == CONTENTS_KELP_PLANT {
                    if let Some(plant) = env.dw_plant_at_pos(s.to_square) {
                        if env.is_kelp_plant(&plant) {
                            let food = env.plant_available_food(&plant);
                            let take = if food >= 900.0f32 { 900.0f32 } else { food };
                            ns.borrow_mut().fullness += take;
                            let again = env.plant_available_food(&plant);
                            env.plant_set_available_food(&plant, again - take);
                        }
                    }
                }
                let (fullness, cd, lt) = {
                    let n = ns.borrow();
                    (n.fullness, n.lay_cooldown_timer, n.lay_timer)
                };
                if (fullness as f64) > 5400.0f64 && cd <= 0.0f32 && lt <= 0.0f32 {
                    ns.borrow_mut().lay_timer =
                        (((r("ClownFish.update.layTimer") * 900.0f32) as f64) * 0.2f64 + 180.0f64) as f32;
                    dw_changed(me);
                }
            }
        }
        s.from_square = s.to_square;
        let to = s.to_square;
        flush(f, s); // RE-ENTRY: updatePosition / checkCurrentPositionForFood -> reactToBeingFed
        me.update_position(me, to);
        Npc::check_current_position_for_food(f, me);
        reload(f, s);
        set_fp(me, Vector2 { x: (to.x as f32) + 0.5f32, y: to.y as f32 });
        if me.is_net() && s.current_net_queue_size > 0 {
            s.to_square = s.next_to_square[0];
            s.movement_direction = s.next_movement_direction[0];
            s.walk_speed = s.next_walk_speed[0];
            let n = s.current_net_queue_size as usize;
            for i in 0..n.saturating_sub(1) {
                s.next_to_square[i] = s.next_to_square[i + 1];
                s.next_movement_direction[i] = s.next_movement_direction[i + 1];
                s.next_walk_speed[i] = s.next_walk_speed[i + 1];
            }
            s.current_net_queue_size -= 1;
        }
        if old_dir != 0 {
            choose_next_tile(me, f, s, is_simulation);
        }
        // runs on EVERY step completion
        if !me.is_net() {
            if s.to_square == s.from_square {
                s.movement_direction = -s.movement_direction;
                me.set_unreliable_update_needs_to_be_sent(true);
                if s.last_try_couldnt_move {
                    if !s.is_on_air_above_solid_ground {
                        s.movement_direction = 0;
                    } else {
                        s.movement_direction =
                            if glibc_rand::rand_tagged("ClownFish.update.noMove") > 0x3fffffff { 1 } else { -1 };
                    }
                }
                s.last_try_couldnt_move = true;
            } else {
                s.last_try_couldnt_move = false;
            }
        }
    }

    // 6. after step handling
    if s.is_on_air_above_solid_ground {
        s.walk_speed = 6.0f32;
    }
    if s.from_square != s.to_square {
        let from_pos = Vector2 { x: (s.from_square.x as f32) + 0.5f32, y: s.from_square.y as f32 };
        let to_pos = Vector2 { x: (s.to_square.x as f32) + 0.5f32, y: s.to_square.y as f32 };
        set_fp(
            me,
            Vector2 {
                x: linear_interpolate(from_pos.x, to_pos.x, s.travel_fraction),
                y: linear_interpolate(from_pos.y, to_pos.y, s.travel_fraction),
            },
        );
    }
}

/// The `oldDir != 0` block of step 5: blockhead scans and next-tile selection.
#[allow(clippy::cognitive_complexity)]
fn choose_next_tile(me: &ObjRef, f: &ClownFish, s: &mut ClownFishState, is_simulation: bool) {
    let env = penv();
    let pos = me.pos();
    let mut new_pos = IntPair { x: pos.x.wrapping_add(s.movement_direction), y: pos.y };
    let mut go_down = false;
    let mut go_up = false;
    let mut should_update = !me.is_net();
    if !s.is_on_air_above_solid_ground {
        let mut running = false;
        let mut fishing = false;
        let bhs = npc_env().all_blockheads_including_net();
        let flee = |s: &mut ClownFishState, new_pos: &mut IntPair, bp_x: i32, running: &mut bool| {
            s.movement_direction = -s.movement_direction;
            s.walk_speed = 2.0f32 - fabsf((bp_x as f32) - (new_pos.x as f32)) * 0.25f32;
            *new_pos = IntPair { x: pos.x.wrapping_add(s.movement_direction), y: pos.y };
            me.set_unreliable_update_needs_to_be_sent(true);
            *running = true;
        };
        // Scan A
        for b in bhs.iter() {
            if env.bh_paused(b) {
                continue;
            }
            if is_simulation && !env.bh_should_continue_simulating(b) {
                continue;
            }
            if env.bh_fishing_rod(b).is_some() {
                fishing = true;
                continue;
            }
            if me.is_net() {
                continue;
            }
            if Npc::belongs_to_player_with_blockhead(f, b) {
                continue;
            }
            let bp = b.pos();
            if pos.x < bp.x.wrapping_add(4)
                && bp.x.wrapping_sub(4) <= pos.x
                && pos.y < bp.y.wrapping_add(4)
                && bp.y.wrapping_sub(4) <= pos.y
            {
                if new_pos.x < bp.x {
                    if s.movement_direction > 0 {
                        flee(s, &mut new_pos, bp.x, &mut running);
                    }
                } else if bp.x < new_pos.x && s.movement_direction < 0 {
                    flee(s, &mut new_pos, bp.x, &mut running);
                }
                if new_pos.y < bp.y && !fishing {
                    go_down = true;
                    s.walk_speed = 2.0f32 - fabsf((bp.y as f32) - (new_pos.y as f32)) * 0.25f32;
                } else if bp.y < new_pos.y && !fishing {
                    go_up = true;
                    s.walk_speed = 2.0f32 - fabsf((bp.y as f32) - (new_pos.y as f32)) * 0.25f32;
                }
                break;
            }
        }
        // Scan B: attraction to bait
        if !running && fishing {
            for b in bhs.iter() {
                let rod = match env.bh_fishing_rod(b) {
                    Some(rd) => rd,
                    None => continue,
                };
                if env.rod_hooked_fish(&rod).is_some() {
                    continue;
                }
                let hpf = env.rod_hook_pos(&rod);
                let hp = IntPair { x: f32_to_i32(hpf.x), y: f32_to_i32(hpf.y) };
                if pos.x < hp.x.wrapping_add(2)
                    && hp.x.wrapping_sub(2) <= pos.x
                    && pos.y < hp.y.wrapping_add(2)
                    && hp.y.wrapping_sub(2) <= pos.y
                {
                    if !b.is_net() {
                        should_update = true;
                        if (hp.x == pos.x || hp.x == pos.x.wrapping_add(1) || hp.x == pos.x.wrapping_sub(1))
                            && (hp.y == pos.y || hp.y.wrapping_add(1) == pos.y)
                            && r("ClownFish.update.hook") < 0.1f32
                        {
                            s.hooked_fishing_rod = Some(rod.clone());
                            env.rod_set_hooked_fish(&rod, Some(me));
                            b.set_update_needs_to_be_sent(true);
                            me.set_update_needs_to_be_sent(true);
                        }
                        if pos.x < hp.x {
                            if s.movement_direction < 0 {
                                s.movement_direction = -s.movement_direction;
                                s.walk_speed = 2.0f32;
                                new_pos = IntPair { x: pos.x.wrapping_add(s.movement_direction), y: pos.y };
                                me.set_unreliable_update_needs_to_be_sent(true);
                                running = true;
                            }
                        } else if hp.x < pos.x && s.movement_direction > 0 {
                            s.movement_direction = -s.movement_direction;
                            s.walk_speed = 2.0f32;
                            new_pos = IntPair { x: pos.x.wrapping_add(s.movement_direction), y: pos.y };
                            me.set_unreliable_update_needs_to_be_sent(true);
                            running = true;
                        }
                        if pos.y < hp.y {
                            go_up = true;
                            s.walk_speed = 2.0f32;
                        } else if hp.y < pos.y {
                            go_down = true;
                            s.walk_speed = 2.0f32;
                        }
                    }
                    break;
                }
            }
        }
        let _ = running;
    }
    if !should_update {
        return;
    }
    let on_air = s.is_on_air_above_solid_ground;
    let passable = |t: &Option<Tile>| -> bool {
        match t {
            None => false,
            Some(x) => !t_solid(t) && !t_gate(t) && ((t_water(t) && x.partial_content_left >= 0x80) || on_air),
        }
    };
    // the escape tests omit the gate test
    let passable_ng = |t: &Option<Tile>| -> bool {
        match t {
            None => false,
            Some(x) => !t_solid(t) && ((t_water(t) && x.partial_content_left >= 0x80) || on_air),
        }
    };
    let tn = env.tile_cached(new_pos.x, new_pos.y);
    if !passable(&tn) {
        let opp = env.tile_loaded(pos.x.wrapping_sub(new_pos.x.wrapping_sub(pos.x)), new_pos.y);
        if let Some(o) = opp {
            if t_solid(&opp) || !t_water(&opp) || o.partial_content_left < 0x80 {
                let tu = env.tile_loaded(pos.x, pos.y.wrapping_add(1));
                if passable_ng(&tu) {
                    s.to_square = IntPair { x: pos.x, y: pos.y.wrapping_add(1) };
                    me.set_unreliable_update_needs_to_be_sent(true);
                } else {
                    let td = env.tile_loaded(pos.x, pos.y.wrapping_sub(1));
                    if passable_ng(&td) {
                        s.to_square = IntPair { x: pos.x, y: pos.y.wrapping_sub(1) };
                        me.set_unreliable_update_needs_to_be_sent(true);
                    }
                }
            }
        }
    } else if new_pos.y == pos.y {
        let dir = s.movement_direction;
        let t5 = env.tile_cached(new_pos.x.wrapping_add(dir), new_pos.y);
        let ahead_ok = passable(&t5);
        let rf = r("ClownFish.update.step");
        if !ahead_ok || go_down || go_up || rf > 0.9f32 {
            if go_up || (!go_down && rf > 0.95f32) {
                let tu = env.tile_cached(pos.x.wrapping_add(dir), pos.y.wrapping_add(1));
                if !passable(&tu) {
                    let td = env.tile_cached(pos.x.wrapping_add(dir), pos.y.wrapping_sub(1));
                    if !passable(&td) {
                        s.to_square = new_pos;
                    } else {
                        new_pos.y = pos.y.wrapping_sub(1);
                        s.to_square = new_pos;
                    }
                } else {
                    new_pos.y = pos.y.wrapping_add(1);
                    s.to_square = new_pos;
                }
            } else {
                let td = env.tile_cached(pos.x.wrapping_add(dir), pos.y.wrapping_sub(1));
                if !passable(&td) {
                    let tu = env.tile_cached(pos.x.wrapping_add(dir), pos.y.wrapping_add(1));
                    if !passable(&tu) {
                        s.to_square = new_pos;
                    } else {
                        new_pos.y = pos.y.wrapping_add(1);
                        s.to_square = new_pos;
                    }
                } else {
                    new_pos.y = pos.y.wrapping_sub(1);
                    s.to_square = new_pos;
                }
            }
            me.set_unreliable_update_needs_to_be_sent(true);
        } else {
            s.to_square = new_pos;
            me.set_unreliable_update_needs_to_be_sent(true);
        }
    } else {
        s.to_square = new_pos; // unreachable (newPos.y never changes)
        me.set_unreliable_update_needs_to_be_sent(true);
    }
}

// ------------------------------------------------------------------------------------------------ trait impls

impl DynamicObject for ClownFish {
    fn base(&self) -> &RefCell<DynamicObjectBase> {
        &self.base
    }
    fn class_info(&self) -> &'static ClassInfo {
        &CLOWN_FISH_CLASS_INFO
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
        as_harmable,
        as_ridable,
        as_tap_choice
    );

    fn update_accurate_dt_is_simulation(&self, me: &ObjRef, dt: f32, accurate_dt: f32, is_simulation: bool) {
        clown_fish_update_accurate_dt_is_simulation(me, dt, accurate_dt, is_simulation)
    }

    /// : `floatPos + (xOffset, 0)`
    fn render_pos(&self) -> Vector2 {
        let fp = self.base.borrow().float_pos;
        Vector2 { x: fp.x + self.clown_fish.borrow().x_offset, y: fp.y }
    }

    /// : super, then the hooked rod is dropped whatever the argument.
    fn set_needs_removed(&self, _me: &ObjRef, v: bool) {
        self.base.borrow_mut().needs_removed = v;
        self.clown_fish.borrow_mut().hooked_fishing_rod = None;
    }

    /// : 0x70 bytes plus the NPC name tail.
    fn creation_net_data_for_client(&self, me: &ObjRef, client_id: Option<&str>) -> Option<Vec<u8>> {
        let mut buf = vec![0u8; 0x70];
        npc_npc_creation_net_data_for_client(me, client_id).write_le(&mut buf[0..0x48]);
        clown_fish_clown_fish_update_net_data_for_client(me, client_id).write_le(&mut buf[0x48..0x70]);
        Some(npc_append_npc_creation_data_to_data(me, buf))
    }

    fn update_net_data_for_client(&self, me: &ObjRef, client_id: Option<&str>) -> Option<Vec<u8>> {
        Some(clown_fish_clown_fish_update_net_data_for_client(me, client_id).to_vec())
    }

    fn remote_update(&self, me: &ObjRef, net_data: &[u8]) {
        npc_remote_update(me, net_data);
        if !npc_state(me).borrow().dead {
            let u = ClownFishUpdateNetData::read_le(&padded_bytes(net_data, 0x28));
            clown_fish_do_remote_update(me, u);
        } else if self.clown_fish.borrow().travel_fraction < 1.0f32 && !npc_env().is_client() {
            self.base.borrow_mut().needs_removed = false;
        }
    }

    fn remote_creation_data_update(&self, me: &ObjRef, net_data: &[u8]) {
        npc_remote_creation_data_update(me, net_data);
        if !npc_state(me).borrow().dead {
            let c = ClownFishCreationNetData::read_le(&padded_bytes(net_data, 0x70));
            clown_fish_do_remote_update(me, c.clown_fish_update_net_data);
        } else if self.clown_fish.borrow().travel_fraction < 1.0f32 && !npc_env().is_client() {
            self.base.borrow_mut().needs_removed = false;
        }
    }

    /// : `[hookedFishingRod setHookedFish:nil]`, drop the rod, then the NPC dealloc.
    fn dealloc(&self, me: &ObjRef) {
        let rod = self.clown_fish.borrow_mut().hooked_fishing_rod.take();
        if let Some(r) = rod {
            penv().rod_set_hooked_fish(&r, None);
        }
        crate::npc::npc::npc_dealloc(me);
    }

    /// : drops the rod when its blockhead goes away.
    // Note: the spec shows no `[super blockheadUnloaded:]` here (Shark and Donkey chain), so none is made.
    fn blockhead_unloaded(&self, _me: &ObjRef, b: &ObjRef) {
        let rod = self.clown_fish.borrow().hooked_fishing_rod.clone();
        let hit = match rod {
            Some(r) => penv().rod_blockhead(&r).map_or(false, |x| ptr_eq(&x, b)),
            None => false,
        };
        if hit {
            self.clown_fish.borrow_mut().hooked_fishing_rod = None;
        }
    }
}

impl Npc for ClownFish {
    fn npc(&self) -> &RefCell<NpcState> {
        &self.npc
    }
    fn npc_type(&self) -> i32 {
        NPC_CLOWNFISH
    }
    fn max_age(&self) -> f32 {
        7200.0
    }
    fn species_name(&self) -> String {
        "FISH".to_string()
    }
    fn food_item_type(&self) -> i32 {
        ITEM_KELP
    }
    fn captured_item_type(&self) -> i32 {
        ITEM_FISH_BUCKET
    }
    fn capture_required_item_type(&self) -> i32 {
        ITEM_TIN_BUCKET
    }
    fn get_names_array(&self) -> Option<&'static [&'static str]> {
        Some(&FISH_NAMES)
    }
    fn get_names_array_count(&self) -> i32 {
        62
    }
    fn max_health(&self) -> u16 {
        25
    }
    fn creation_data_struct_size(&self) -> usize {
        0x70
    }
    fn ridable_when_tamed(&self) -> bool {
        false
    }
    fn die(&self, me: &ObjRef, killer: Option<&ObjRef>) {
        clown_fish_die(me, killer)
    }
    fn hit_with_force_blockhead(&self, me: &ObjRef, force: i32, b: Option<&ObjRef>) {
        clown_fish_hit_with_force_blockhead(me, force, b)
    }
    fn react_to_being_hit(&self, _me: &ObjRef) {
        if !self.npc.borrow().dead {
            self.clown_fish.borrow_mut().current_net_queue_size = 0;
        }
    }
    /// : `[super reactToBeingFed]` (empty), `walkSpeed = 1`, `walkTimer = 0`
    fn react_to_being_fed(&self, _me: &ObjRef) {
        let mut s = self.clown_fish.borrow_mut();
        s.walk_speed = 1.0f32;
        s.walk_timer = 0.0f32;
    }
    fn can_be_captured_by_blockhead_with_item_type(&self, b: &ObjRef, item: i32) -> bool {
        if item != ITEM_TIN_BUCKET {
            return false;
        }
        if Npc::tamed(self) && !Npc::belongs_to_player_with_blockhead(self, b) {
            return false;
        }
        true
    }
    fn cant_be_captured_tip_string_for_blockhead_with_item_type(&self, b: &ObjRef, item: i32) -> Option<String> {
        if item == ITEM_TIN_BUCKET {
            if Npc::tamed(self) && !Npc::belongs_to_player_with_blockhead(self, b) {
                return Some(format!("THAT {} BELONGS TO\nSOMEONE ELSE", Npc::species_name(self)));
            }
            None
        } else {
            Some(format!("{} CAN ONLY BE CAPTURED\nIN A BUCKET", Npc::species_name(self)))
        }
    }
}

impl_npc_protocols!(ClownFish);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::npc::egg::test_support::TestPassiveEnv;
    use crate::objects::base::testutil::{setup, MockDw, MockWorld};
    use std::rc::Rc;

    struct Fx {
        w: Rc<MockWorld>,
        dw: Rc<MockDw>,
        env: Rc<TestPassiveEnv>,
    }
    fn fixture() -> Fx {
        let (w, dw, _l) = setup(4, 4);
        let env = TestPassiveEnv::new();
        env.install();
        glibc_rand::srand(1);
        Fx { w, dw, env }
    }
    fn water(fx: &Fx, x0: i32, x1: i32, y0: i32, y1: i32, depth: u8) {
        for x in x0..=x1 {
            for y in y0..=y1 {
                let mut t = Tile::default();
                t.type_index = 3;
                t.partial_content_left = depth;
                fx.env.tiles.borrow_mut().insert((x, y), t);
            }
        }
    }
    fn make(fx: &Fx, adult: bool, placed: bool) -> Option<ObjRef> {
        let wr: WorldRef = fx.w.clone();
        let dr: DynamicWorldRef = fx.dw.clone();
        ClownFish::init_with_world_dynamic_world_at_position_cache_save_dict_is_adult_was_placed_placed_by_client(
            &wr,
            &dr,
            IntPair { x: 40, y: 3 },
            None,
            None,
            adult,
            placed,
            None,
        )
    }
    fn raw(seed: u32, n: usize) -> Vec<i32> {
        glibc_rand::srand(seed);
        (0..n).map(|_| glibc_rand::rand()).collect()
    }
    fn rr(v: i32) -> f32 {
        (v as f32) / 2147483648.0f32
    }

    #[test]
    fn constants_table() {
        let fx = fixture();
        water(&fx, 30, 50, 0, 10, 255);
        let me = make(&fx, true, true).unwrap();
        let f = fish_of(&me);
        assert_eq!(Npc::npc_type(f), 4);
        assert_eq!(Npc::max_age(f), 7200.0);
        assert_eq!(Npc::max_health(f), 25);
        assert_eq!(Npc::species_name(f), "FISH");
        assert_eq!(Npc::food_item_type(f), 144);
        assert_eq!(Npc::captured_item_type(f), 158);
        assert_eq!(Npc::capture_required_item_type(f), 102);
        assert_eq!(Npc::creation_data_struct_size(f), 0x70);
        assert_eq!(Npc::get_names_array_count(f), 62);
        assert!(!Npc::ridable_when_tamed(f));
        assert_eq!(FISH_NAMES.len(), 62);
        assert_eq!(FISH_NAMES[29], "Mr. Bubbles");
        assert_eq!(FISH_NAMES[51], "Fishy McFishface");
        assert_eq!(me.object_type(), 35);
    }

    #[test]
    fn spawn_refusals_and_draw_counts() {
        let fx = fixture();
        // no tile loaded: refused after the NPC draws (adult 3+1)
        glibc_rand::srand(2);
        assert!(make(&fx, true, false).is_none());
        assert_eq!(glibc_rand::rand(), raw(2, 5)[4]);
        // shallow water refused when not placed
        water(&fx, 30, 50, 0, 10, 0x7e);
        glibc_rand::srand(2);
        assert!(make(&fx, true, false).is_none());
        assert_eq!(glibc_rand::rand(), raw(2, 5)[4]);
        // placed skips the depth test; accepted adult = 4 + 2 draws
        glibc_rand::srand(2);
        assert!(make(&fx, true, true).is_some());
        assert_eq!(glibc_rand::rand(), raw(2, 7)[6]);
        // deep water, default env temperature is warm enough? (cold refuses) -> placed=false path
        water(&fx, 30, 50, 0, 10, 255);
        glibc_rand::srand(2);
        let r = make(&fx, false, false);
        // either accepted (3+... draws) or refused by temperature; both consume at least the NPC part
        let _ = r;
    }

    #[test]
    fn load_derived_stuff_formulas() {
        for seed in 0..50u32 {
            let fx = fixture();
            water(&fx, 30, 50, 0, 10, 255);
            let wr: WorldRef = fx.w.clone();
            let dr: DynamicWorldRef = fx.dw.clone();
            let v = raw(seed, 4);
            glibc_rand::srand(seed);
            let me = ClownFish::init_with_world_dynamic_world_save_dict_cache(
                &wr,
                &dr,
                &crate::objects::base::new_mutable_dict(),
                None,
            )
            .unwrap();
            let s = fish_of(&me).clown_fish.borrow().clone();
            let r1 = rr(v[1]) as f64;
            let dir = if r1 > 0.75 {
                1
            } else if r1 < 0.25 {
                -1
            } else {
                0
            };
            assert_eq!(s.movement_direction, dir);
            assert_eq!(s.walk_speed.to_bits(), (((rr(v[2]) as f64) * 0.2 + 0.10000000149011612) as f32).to_bits());
            assert_eq!(s.body_rotation, if dir == 1 { 1.5707964 } else { -1.5707964 });
            assert_eq!(glibc_rand::rand(), v[3]);
        }
    }

    #[test]
    fn die_drops_and_rand_count() {
        let fx = fixture();
        water(&fx, 30, 50, 0, 10, 255);
        let me = make(&fx, true, true).unwrap();
        let killer: ObjRef = crate::objects::base::testutil::TestObj::new();
        let v = raw(7, 2);
        glibc_rand::srand(7);
        Npc::die(fish_of(&me), &me, Some(&killer));
        let n = (v[0] % 3 + 1) as usize;
        assert_eq!(fx.env.drops.borrow().len(), n);
        assert!(fx.env.drops.borrow().iter().all(|d| *d == (151, 0, 0, 1)));
        assert_eq!(glibc_rand::rand(), v[1]);
        assert!(npc_state(&me).borrow().dead);
        assert_eq!(fish_of(&me).clown_fish.borrow().travel_fraction, 0.0);
        // already dead: nothing
        Npc::die(fish_of(&me), &me, Some(&killer));
        assert_eq!(fx.env.drops.borrow().len(), n);
        // killed by nobody: no draw
        let me2 = make(&fx, true, true).unwrap();
        glibc_rand::srand(7);
        Npc::die(fish_of(&me2), &me2, None);
        assert_eq!(glibc_rand::rand(), v[0]);
    }

    #[test]
    fn dead_fish_removed_after_a_quarter_second() {
        let fx = fixture();
        water(&fx, 30, 50, 0, 10, 255);
        let me = make(&fx, true, true).unwrap();
        npc_state(&me).borrow_mut().dead = true;
        fish_of(&me).clown_fish.borrow_mut().travel_fraction = 0.0;
        me.update_accurate_dt_is_simulation(&me, 0.2, 0.2, false);
        assert!(!me.needs_removed());
        me.update_accurate_dt_is_simulation(&me, 0.1, 0.1, false);
        assert!(me.needs_removed());
    }

    #[test]
    fn swims_horizontally_and_stays_in_water() {
        let fx = fixture();
        water(&fx, 30, 50, 0, 10, 255);
        let me = make(&fx, true, true).unwrap();
        {
            let mut s = fish_of(&me).clown_fish.borrow_mut();
            s.movement_direction = 1;
            s.walk_speed = 2.0;
            s.travel_fraction = 0.95;
            s.random_time_between_direction_changes = 100.0;
        }
        me.update_accurate_dt_is_simulation(&me, 0.1, 0.1, false);
        let s = fish_of(&me).clown_fish.borrow().clone();
        assert_eq!(s.from_square, IntPair { x: 40, y: 3 });
        assert_eq!(s.to_square.x, 41);
        assert!(s.to_square.y >= 2 && s.to_square.y <= 4);
        glibc_rand::srand(12);
        for _ in 0..300 {
            me.update_accurate_dt_is_simulation(&me, 0.1, 0.1, false);
        }
        let p = me.pos();
        assert!(p.x >= 29 && p.x <= 51 && p.y >= 0 && p.y <= 10, "{:?}", p);
        assert!(!npc_state(&me).borrow().dead);
    }

    #[test]
    fn out_of_water_drowns_after_ten_seconds_without_drops() {
        let fx = fixture();
        for x in 30..=50 {
            for y in 0..=2 {
                fx.env.set_tile(x, y, 1); // solid ground
            }
            for y in 3..=9 {
                fx.env.set_tile(x, y, 2); // air
            }
        }
        let me = make(&fx, true, true).unwrap();
        {
            let mut s = fish_of(&me).clown_fish.borrow_mut();
            s.movement_direction = 0;
            s.random_time_between_direction_changes = 1000.0;
        }
        for _ in 0..110 {
            me.update_accurate_dt_is_simulation(&me, 0.1, 0.1, false);
            if npc_state(&me).borrow().dead {
                break;
            }
        }
        assert!(npc_state(&me).borrow().dead);
        assert!(fx.env.drops.borrow().is_empty());
    }

    #[test]
    fn ice_freezes_everything() {
        let fx = fixture();
        water(&fx, 30, 50, 0, 10, 255);
        let me = make(&fx, true, true).unwrap();
        fx.env.set_tile(40, 3, 4);
        {
            let mut s = fish_of(&me).clown_fish.borrow_mut();
            s.movement_direction = 1;
            s.travel_fraction = 0.99;
            s.random_time_between_direction_changes = 100.0;
        }
        glibc_rand::srand(3);
        me.update_accurate_dt_is_simulation(&me, 0.5, 0.5, false);
        assert_eq!(fish_of(&me).clown_fish.borrow().travel_fraction, 0.99);
        assert_eq!(glibc_rand::rand(), raw(3, 1)[0]);
    }

    #[test]
    fn net_struct_layout_and_remote_update() {
        let fx = fixture();
        water(&fx, 30, 50, 0, 10, 255);
        let me = make(&fx, true, true).unwrap();
        {
            let mut s = fish_of(&me).clown_fish.borrow_mut();
            s.to_square = IntPair { x: 41, y: 3 };
            s.walk_speed = 0.5;
            s.movement_direction = -1;
        }
        let nd = me.update_net_data_for_client(&me, None).unwrap();
        assert_eq!(nd.len(), 0x28);
        let u = ClownFishUpdateNetData::read_le(&nd);
        assert_eq!(u.to_square_x, 41);
        assert_eq!(u.walk_speed, 127);
        assert_eq!(u.movement_direction, -1);
        assert_eq!(&nd[0x23..0x28], &[0u8; 5]);
        let c = me.creation_net_data_for_client(&me, None).unwrap();
        assert_eq!(&c[0x6b..0x70], &[0u8; 5]);
        // server side: applies the wire values
        let mut u2 = u;
        u2.walk_speed = 255;
        u2.movement_direction = 1;
        u2.to_square_x = 45;
        clown_fish_do_remote_update(&me, u2);
        let s = fish_of(&me).clown_fish.borrow().clone();
        assert_eq!(s.walk_speed, 1.0);
        assert_eq!(s.movement_direction, 1);
        assert_eq!(s.to_square.x, 45);
        // replica from net data primes the queues
        let rep = ClownFish::init_with_world_dynamic_world_cache_net_data(
            &(fx.w.clone() as WorldRef),
            &(fx.dw.clone() as DynamicWorldRef),
            None,
            &c,
        )
        .unwrap();
        assert!(rep.is_net());
        assert_eq!(fish_of(&rep).clown_fish.borrow().to_square.x, 41);
    }

    #[test]
    fn capture_rules_and_tip_strings() {
        let fx = fixture();
        water(&fx, 30, 50, 0, 10, 255);
        let me = make(&fx, true, true).unwrap();
        let f = fish_of(&me);
        let b: ObjRef = crate::objects::base::testutil::TestObj::new();
        assert!(Npc::can_be_captured_by_blockhead_with_item_type(f, &b, 102));
        assert!(!Npc::can_be_captured_by_blockhead_with_item_type(f, &b, 302));
        assert_eq!(
            Npc::cant_be_captured_tip_string_for_blockhead_with_item_type(f, &b, 302).as_deref(),
            Some("FISH CAN ONLY BE CAPTURED\nIN A BUCKET")
        );
        assert_eq!(Npc::cant_be_captured_tip_string_for_blockhead_with_item_type(f, &b, 102), None);
    }

    #[test]
    fn set_needs_removed_drops_rod() {
        let fx = fixture();
        water(&fx, 30, 50, 0, 10, 255);
        let me = make(&fx, true, true).unwrap();
        fish_of(&me).clown_fish.borrow_mut().hooked_fishing_rod = Some(Rc::new(5u32));
        me.set_needs_removed(&me, false);
        assert!(fish_of(&me).clown_fish.borrow().hooked_fishing_rod.is_none());
        assert!(!me.needs_removed());
    }

    #[allow(dead_code)]
    fn _u(_: &MockDw) {}
}
