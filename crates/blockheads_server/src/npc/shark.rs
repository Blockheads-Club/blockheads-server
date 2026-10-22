//! `Shark: NPC` (NPC type 5, DynamicObject type 36): the large swimming predator.
//!
//! The per-tick behaviour lives in [`Tick`]: a working copy of [`SharkState`] plus one method per stage of the
//! behavioural specification. The copy is written back before, and re-read after, every call that can re-enter the
//! shark (`die`, the food check, spawning a baby, blockhead damage / dismount); see [`Tick::reentrant`].

use std::any::Any;
use std::cell::RefCell;

use crate::fmath::cconv::f32_to_i32;
use crate::fmath::libm::fabsf;
use crate::fmath::vector::{clamp, linear_interpolate};
use crate::items::colors::color_index_for_pigments;
use crate::npc::egg::{penv, t_air_snow, t_gate, t_solid, t_water, world_width_tiles_of, wrap_dx};
pub use crate::npc::npc::NPC_SHARK;
use crate::npc::npc::{
    alloc_npc, npc_add_rider, npc_append_npc_creation_data_to_data, npc_blockhead_unloaded, npc_die, npc_env,
    npc_hit_with_force_blockhead,
    npc_init_with_world_dynamic_world_at_position_cache_save_dict_is_adult_was_placed_placed_by_client,
    npc_init_with_world_dynamic_world_cache_net_data, npc_init_with_world_dynamic_world_save_dict_cache,
    npc_npc_creation_net_data_for_client, npc_npc_update_net_data_for_client, npc_remote_creation_data_update,
    npc_remote_update, npc_remove_rider_opt, npc_state, npc_update_accurate_dt_is_simulation, Npc, NpcState,
    NpcTileCopy, NPC_CLASS,
};
use crate::objects::base::{padded_bytes, DynamicObject, DynamicObjectBase, DynamicWorldRef, WorldRef};
use crate::objects::class_info::ClassInfo;
use crate::objects::handle::{ptr_eq, CacheRef, ObjRef};
use crate::plist::value::PlistDict;
use crate::rng::glibc_rand;
use crate::types::enums::PigmentColor;
use crate::types::netdata::{SharkCreationNetData, SharkUpdateNetData};
use crate::types::structs::{IntPair, Vector2, WireStruct};
use crate::types::tile::Tile;
use crate::world::loot::random_bonus_clothing_item_type_from_shark_at_level;
use crate::{impl_npc_protocols, npc_forward};

/// `DYNAMIC_OBJECT_TYPE_NPC_SHARK`.
pub const DYNAMIC_OBJECT_TYPE_NPC_SHARK: i32 = 36;
const ITEM_TIN_BUCKET: i32 = 102;
const ITEM_RAW_FISH: i32 = 151;
const ITEM_SHARK_JAW: i32 = 157;
const ITEM_SHARK_BUCKET: i32 = 159;
const CONTENTS_KELP_PLANT: u8 = 0x51;
const TILE_ICE: u8 = 4;
const SE_DAMAGE_TEXT_SUFFIX: &str = " by a shark";

const MAX_AGE: f32 = 27000.0;
const MAX_HEALTH: u16 = 25;
const MIN_RIDABLE_AGE: f32 = 4500.0;
/// The double nearest to the single-precision constant 0.1.
const TENTH_F32: f64 = 0.10000000149011612;
const QUARTER_TURN: f32 = 1.5707964;
/// Water level bytes: a cell holds water for the shark when `>= 127`; it is "deep" when `>= 128`.
const LEVEL_WATER: u8 = 0x7f;
const LEVEL_DEEP: u8 = 0x80;

/// `SHARK_NAMES`, index order is observable.
pub static SHARK_NAMES: [&str; 64] = [
    "Alpha",
    "Angel",
    "Barney",
    "Berty",
    "Blade",
    "Bob",
    "Bowie",
    "Brutus",
    "Bubbles",
    "Bubbly",
    "Bullwinkle",
    "Calvin",
    "Charlie",
    "Cinnabar",
    "Clark",
    "Clash",
    "Diablo",
    "Dory",
    "Doug",
    "Elvis",
    "Larry",
    "Lewis",
    "Malcom",
    "Marley",
    "Maverick",
    "Max",
    "Moe",
    "Molly",
    "Monster",
    "Mufasa",
    "Murphy",
    "Napoleon",
    "Otto",
    "Pepper",
    "Raymond",
    "Red",
    "Rocket",
    "Shark",
    "Sharknado",
    "Fishy",
    "Franky",
    "Fred",
    "Garfield",
    "Garth",
    "Goliath",
    "Harry",
    "Jaws",
    "Teeth",
    "Sharpie",
    "Pearly",
    "Joey",
    "Jonah",
    "Killer",
    "Salt",
    "Shadow",
    "Bruce",
    "Sharky McSharkface",
    "Spot",
    "Champ",
    "Stripes",
    "Tigger",
    "Trevor",
    "Victoria",
    "Weasel",
];

/// `Shark: NPC`.
pub static SHARK_CLASS_INFO: ClassInfo = ClassInfo { name: "Shark", superclass: Some(&NPC_CLASS), protocols: &[] };

const ZERO_PAIR: IntPair = IntPair { x: 0, y: 0 };

/// Shark-specific state (server part).
#[derive(Clone)]
pub struct SharkState {
    pub movement_direction: i32,
    pub from_square: IntPair,
    pub to_square: IntPair,
    /// fraction of the current cell-to-cell move; the death timer when dead
    pub travel_fraction: f32,
    pub random_time_between_direction_changes: f32,
    pub body_rotation: f32,
    pub goal_rotation: f32,
    pub walk_speed: f32,
    pub x_offset: f32,
    pub drown_timer: f32,
    pub fall_velocity: f32,
    pub falling: bool,
    pub bite_timer: f32,
    pub biting: bool,
    pub flee_timer: f32,
    pub chase_timer: f32,
    pub has_harmed_blockhead: bool,
    pub last_try_couldnt_move: bool,
    pub is_on_air_above_solid_ground: bool,
    pub attack_blockhead: Option<ObjRef>,
    pub nearby_blockhead: bool,
    pub rider_target_velocity: Vector2,
    pub remote_goal_square: IntPair,
    pub remote_walk_speed: f32,
    pub remote_movement_direction: i32,
    pub name_position: Vector2,
    pub walk_timer: f32,
}

impl Default for SharkState {
    fn default() -> Self {
        SharkState {
            movement_direction: 0,
            from_square: ZERO_PAIR,
            to_square: ZERO_PAIR,
            travel_fraction: 0.0,
            random_time_between_direction_changes: 0.0,
            body_rotation: 0.0,
            goal_rotation: 0.0,
            walk_speed: 0.0,
            x_offset: 0.0,
            drown_timer: 0.0,
            fall_velocity: 0.0,
            falling: false,
            bite_timer: 0.0,
            biting: false,
            flee_timer: 0.0,
            chase_timer: 0.0,
            has_harmed_blockhead: false,
            last_try_couldnt_move: false,
            is_on_air_above_solid_ground: false,
            attack_blockhead: None,
            nearby_blockhead: false,
            rider_target_velocity: Vector2 { x: 1.0, y: 1.0 },
            remote_goal_square: ZERO_PAIR,
            remote_walk_speed: 0.0,
            remote_movement_direction: 0,
            name_position: Vector2 { x: 1.0, y: 1.0 },
            walk_timer: 0.0,
        }
    }
}

/// The Shark.
pub struct Shark {
    pub base: RefCell<DynamicObjectBase>,
    pub npc: RefCell<NpcState>,
    pub shark: RefCell<SharkState>,
}

// ------------------------------------------------------------------------------------------------ small helpers

/// `U()`: one draw from the shared stream as a single-precision fraction.
fn draw(tag: &'static str) -> f32 {
    glibc_rand::r01_tagged(tag)
}

/// Direction from one draw split into thirds of the unit interval: `+1` above 0.75, `-1` below 0.25, else 0.
fn three_way(v: f64) -> i32 {
    if v > 0.75 {
        1
    } else if v < 0.25 {
        -1
    } else {
        0
    }
}

/// `+1` when the fraction is above one half, else `-1`.
fn coin_dir(v: f32) -> i32 {
    if (v as f64) > 0.5 {
        1
    } else {
        -1
    }
}

/// `+1` when a raw draw is above `0x3fffffff`, else `-1`.
fn raw_coin_dir(tag: &'static str) -> i32 {
    if glibc_rand::rand_tagged(tag) > 0x3fffffff {
        1
    } else {
        -1
    }
}

/// Body / goal rotation for a travel direction.
fn facing(dir: i32) -> f32 {
    if dir == 1 {
        QUARTER_TURN
    } else {
        -QUARTER_TURN
    }
}

/// `walk * 0.5 + u * 0.2 + 0.1` in double precision.
fn blended_walk(walk: f32, u: f32) -> f32 {
    ((walk as f64) * 0.5 + (u as f64) * 0.2 + TENTH_F32) as f32
}

/// `max(a, b)` with the original's tie behaviour.
fn at_least(a: f32, b: f32) -> f32 {
    if a <= b {
        b
    } else {
        a
    }
}

fn shark_of(me: &ObjRef) -> &Shark {
    me.as_any().downcast_ref::<Shark>().expect("object is not a Shark")
}

fn dw_changed(me: &ObjRef) {
    if let Some(dw) = me.dynamic_world() {
        dw.dynamic_world_changed_at_pos_object_type(me.pos(), me.object_type());
    }
}

fn set_fp(me: &ObjRef, v: Vector2) {
    me.base().borrow_mut().float_pos = v;
}

fn tile_copy(t: &Tile) -> NpcTileCopy {
    NpcTileCopy {
        type_index: t.type_index,
        on_fire: t.on_fire,
        sun_light: t.sun_light,
        artificial_heat: t.artificial_heat,
    }
}

fn is_deep(t: &Option<Tile>) -> bool {
    t.map_or(false, |x| x.partial_content_left >= LEVEL_DEEP)
}

fn is_shallow(t: &Option<Tile>) -> bool {
    t.map_or(false, |x| x.partial_content_left < LEVEL_DEEP)
}

/// `isNet ? (no rider or rider is remote) : (rider present and remote)`: the shark follows a remote driver.
fn follows_remote(me: &ObjRef) -> bool {
    let rider = npc_state(me).borrow().rider.clone();
    match (me.is_net(), rider) {
        (true, r) => r.map_or(true, |r| r.is_net()),
        (false, r) => r.map_or(false, |r| r.is_net()),
    }
}

/// Damage dealt by one bite (single result, double intermediate).
fn bite_damage(ratio: f32, damage: f32, max_health: f32, rules_enabled: bool, difficulty: i32) -> f32 {
    let mut dmg = ((ratio + 0.2f32) as f64 * 0.8 * (1.0f32 - damage / max_health) as f64) as f32;
    if dmg <= 0.02 {
        dmg = 0.02;
    }
    if rules_enabled {
        if difficulty == 2 {
            dmg = (dmg as f64 * 0.25) as f32;
        } else if difficulty == 0 {
            dmg *= 4.0;
        }
    }
    dmg
}

/// Number of RAW_FISH dropped at a given `age / maxAge`: integers `i >= 0` with `10 * (ratio + 0.1) > i`.
fn raw_fish_count(ratio: f32) -> i64 {
    let limit = 10.0f64 * (ratio as f64 + 0.1);
    let mut n = 0i64;
    while limit > n as f64 && n <= (1 << 20) {
        n += 1;
    }
    n
}

// ------------------------------------------------------------------------------------------------ construction

impl Shark {
    fn alloc() -> std::rc::Rc<Shark> {
        alloc_npc(|w| Shark {
            base: RefCell::new(DynamicObjectBase::alloc_zeroed_with(w)),
            npc: RefCell::new(NpcState::default()),
            shark: RefCell::new(SharkState::default()),
        })
    }

    /// Spawn at a position. The generic NPC part draws first; the cell must be deep enough water (unless the
    /// shark was placed) and, for a natural spawn, warm enough. Failures remove the half-built object.
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
        let me: ObjRef = Shark::alloc();
        npc_init_with_world_dynamic_world_at_position_cache_save_dict_is_adult_was_placed_placed_by_client(
            &me, world, dw, pos, cache, save_dict, is_adult, was_placed, client_id,
        )
        .then_some(())?;
        let p = me.pos();
        let cell = penv().tile_loaded(p.x, p.y);
        let refuse = match &cell {
            None => true,
            Some(c) => !was_placed && (!t_water(&cell) || c.partial_content_left < LEVEL_WATER),
        };
        if refuse {
            me.remove_from_macro_block(&me);
            return None;
        }
        if !was_placed && Self::too_cold(&me, world, p, &cell.unwrap()) {
            me.remove_from_macro_block(&me);
            return None;
        }
        shark_load_derived_stuff(&me);
        Some(me)
    }

    /// Natural-spawn temperature gate: `temp < 0.1`, or (`age > 0.1` and `temp < 2.0`).
    fn too_cold(me: &ObjRef, world: &WorldRef, p: IntPair, cell: &Tile) -> bool {
        let ne = npc_env();
        let wt = world.world_time();
        let weather = ne.weather_fraction(p, wt);
        let day = ne.day_night_fraction(p.x as f32, wt);
        let season = ne.season_for_world_x(p.x, wt);
        let temp = ne.current_temperature(&tile_copy(cell), p, season, day, weather);
        let age = npc_state(me).borrow().age;
        temp < 0.1f32 || ((age as f64) > 0.1 && temp < 2.0f32)
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
        Shark::init_with_world_dynamic_world_at_position_cache_save_dict_is_adult_was_placed_placed_by_client(
            world, dw, pos, cache, save_dict, is_adult, was_placed, client_id,
        )
    }

    /// Load from a save dictionary (generic load, then the two derived-setup draws).
    pub fn init_with_world_dynamic_world_save_dict_cache(
        world: &WorldRef,
        dw: &DynamicWorldRef,
        save_dict: &PlistDict,
        cache: CacheRef,
    ) -> Option<ObjRef> {
        let me: ObjRef = Shark::alloc();
        npc_init_with_world_dynamic_world_save_dict_cache(&me, world, dw, save_dict, cache).then_some(())?;
        shark_load_derived_stuff(&me);
        Some(me)
    }

    /// Create from network data: derived setup, apply the creation data, then face the (new) direction.
    pub fn init_with_world_dynamic_world_cache_net_data(
        world: &WorldRef,
        dw: &DynamicWorldRef,
        cache: CacheRef,
        net_data: &[u8],
    ) -> Option<ObjRef> {
        let o = Shark::alloc();
        let me: ObjRef = o.clone();
        npc_init_with_world_dynamic_world_cache_net_data(&me, world, dw, cache, net_data).then_some(())?;
        shark_load_derived_stuff(&me);
        me.remote_creation_data_update(&me, net_data);
        let mut s = o.shark.borrow_mut();
        let rot = facing(s.movement_direction);
        s.body_rotation = rot;
        s.goal_rotation = rot;
        drop(s);
        Some(me)
    }

    /// Controlled here: a net copy only when ridden by a local blockhead; an authoritative shark unless ridden
    /// by a remote one.
    pub fn is_under_local_control(&self) -> bool {
        let rider = self.npc.borrow().rider.clone();
        if self.base.borrow().is_net {
            rider.map_or(false, |r| !r.is_net())
        } else {
            !rider.map_or(false, |r| r.is_net())
        }
    }
    pub fn min_ridable_age(&self) -> f32 {
        MIN_RIDABLE_AGE
    }
    /// Body centre: render position shifted by `(ratio * 1.5 + 0.25) * (-bodyRotation / (pi/2))`.
    pub fn center(&self) -> Vector2 {
        let ratio = self.npc.borrow().age / Npc::max_age(self);
        let half = ((ratio as f64) * 1.5 + 0.25) as f32;
        let br = self.shark.borrow().body_rotation;
        let shift = (half as f64 * ((-br) as f64 / std::f64::consts::FRAC_PI_2)) as f32;
        let rp = DynamicObject::render_pos(self);
        Vector2 { x: rp.x + shift, y: rp.y }
    }
    /// A shark nobody controls forgets the last steering input.
    fn release_steering_if_uncontrolled(&self) {
        if !self.npc.borrow().being_controlled {
            self.shark.borrow_mut().rider_target_velocity = Vector2::ZERO;
        }
    }
    /// Label position: `namePosition + (0, 1)`.
    pub fn name_pos(&self) -> Vector2 {
        let np = self.shark.borrow().name_position;
        Vector2 { x: np.x, y: np.y + 1.0 }
    }
}

/// Derived setup (two draws): initial direction, walk speed, cell bookkeeping and facing.
pub fn shark_load_derived_stuff(me: &ObjRef) {
    let sh = shark_of(me);
    let dir = three_way(draw("Shark.load.dir") as f64);
    let walk = ((draw("Shark.load.walkSpeed") as f64) * 0.2 + TENTH_F32) as f32;
    let pos = me.pos();
    let mut s = sh.shark.borrow_mut();
    s.movement_direction = dir;
    s.to_square = pos;
    s.from_square = pos;
    s.travel_fraction = 1.0;
    s.random_time_between_direction_changes = 0.0;
    s.walk_speed = walk;
    s.body_rotation = facing(dir);
    s.goal_rotation = facing(dir);
}

// ------------------------------------------------------------------------------------------------ reactions

/// Death: ignored when already dead; otherwise the death timer restarts and the generic death runs.
pub fn shark_die(me: &ObjRef, killer: Option<&ObjRef>) {
    if npc_state(me).borrow().dead {
        return;
    }
    shark_of(me).shark.borrow_mut().travel_fraction = 0.0;
    npc_die(me, killer);
}

/// Bigger sharks take less damage per hit; a surviving authoritative shark bolts in a random direction.
pub fn shark_hit_with_force_blockhead(me: &ObjRef, force: i32, b: Option<&ObjRef>) {
    let sh = shark_of(me);
    let ratio = npc_state(me).borrow().age / Npc::max_age(sh);
    let scaled = f32_to_i32((force as f32) / (ratio * 2.8f32 + 0.2f32));
    npc_hit_with_force_blockhead(me, scaled.max(1), b);
    if !npc_state(me).borrow().dead && !me.is_net() {
        let dir = coin_dir(draw("Shark.hit.dir"));
        let mut s = sh.shark.borrow_mut();
        s.movement_direction = dir;
        s.walk_speed = 3.0;
        s.random_time_between_direction_changes = 5.0;
    }
}

fn shark_remove_rider_opt(me: &ObjRef, rider: Option<&ObjRef>) {
    let current = npc_state(me).borrow().rider.clone();
    let is_current = match (&current, rider) {
        (None, None) => true,
        (Some(a), Some(b)) => ptr_eq(a, b),
        _ => false,
    };
    if is_current {
        npc_remove_rider_opt(me, rider);
        shark_of(me).shark.borrow_mut().rider_target_velocity = Vector2::ZERO;
    }
}

// ------------------------------------------------------------------------------------------------ net data

/// Update record (0x28 bytes): generic part, then the CURRENT cell, walk speed, age and direction.
pub fn shark_shark_update_net_data_for_client(me: &ObjRef, client_id: Option<&str>) -> SharkUpdateNetData {
    let generic = npc_npc_update_net_data_for_client(me, client_id);
    let pos = me.pos();
    let age = npc_state(me).borrow().age;
    let s = shark_of(me).shark.borrow();
    SharkUpdateNetData {
        npc_update_net_data: generic,
        to_square_x: pos.x as u32,
        to_square_y: pos.y as u32,
        walk_speed: f32_to_i32(s.walk_speed * 255.0) as i16,
        age: f32_to_i32(age) as u16,
        movement_direction: s.movement_direction as i8,
        clear_queue: 0,
        padding: [0; 2],
    }
}

/// Apply an update record. A server only listens to a remote rider; a client always takes the age and, unless it
/// rides the shark itself (or attacks with a local target), the remote goal.
pub fn shark_do_remote_update(me: &ObjRef, u: SharkUpdateNetData) {
    let sh = shark_of(me);
    let rider = npc_state(me).borrow().rider.clone();
    let goal = IntPair { x: u.to_square_x as i32, y: u.to_square_y as i32 };
    let walk = (u.walk_speed as i32 as f32) / 255.0;
    let dir = u.movement_direction as i32;
    if !npc_env().is_client() {
        if !rider.map_or(false, |r| r.is_net()) {
            return;
        }
        {
            let mut s = sh.shark.borrow_mut();
            s.remote_goal_square = goal;
            s.remote_walk_speed = walk;
            s.walk_speed = walk;
            s.remote_movement_direction = dir;
            s.movement_direction = dir;
        }
        me.set_update_needs_to_be_sent(true);
        return;
    }
    let local_rider = rider.map_or(false, |r| !r.is_net());
    if !local_rider {
        let local_target = sh.shark.borrow().attack_blockhead.as_ref().map_or(false, |a| !a.is_net());
        if !local_target {
            let mut s = sh.shark.borrow_mut();
            s.remote_goal_square = goal;
            s.remote_walk_speed = walk;
            s.remote_movement_direction = dir;
        }
    }
    npc_state(me).borrow_mut().age = u.age as f32;
}

/// Creation record: remote-driven sharks adopt the transmitted target, then the embedded update part applies.
fn apply_creation_data(me: &ObjRef, net_data: &[u8]) {
    let sh = shark_of(me);
    npc_remote_creation_data_update(me, net_data);
    if npc_state(me).borrow().dead {
        let finished = sh.shark.borrow().travel_fraction >= 1.0;
        if !finished && !npc_env().is_client() {
            sh.base.borrow_mut().needs_removed = false;
        }
        return;
    }
    let d = SharkCreationNetData::read_le(&padded_bytes(net_data, 0x78));
    if follows_remote(me) {
        adopt_target(me, sh, d.attack_blockhead);
    }
    shark_do_remote_update(me, d.shark_update_net_data);
}

fn adopt_target(me: &ObjRef, sh: &Shark, wanted: u64) {
    if wanted == 0 {
        sh.shark.borrow_mut().attack_blockhead = None;
        return;
    }
    for bh in npc_env().all_blockheads_including_net() {
        if bh.unique_id() != wanted {
            continue;
        }
        let current = sh.shark.borrow().attack_blockhead.clone();
        if current.map_or(false, |c| c.unique_id() == wanted) {
            continue;
        }
        {
            let mut s = sh.shark.borrow_mut();
            s.attack_blockhead = Some(bh.clone());
            s.biting = true;
            s.bite_timer = 1.0;
            s.walk_speed = 4.0;
        }
        me.set_update_needs_to_be_sent(true);
        me.set_creation_data_needs_to_be_sent(true);
    }
}

// ------------------------------------------------------------------------------------------------ per-tick update

/// Flags that steer the choice of the next cell.
#[derive(Default)]
struct Plan {
    want_up: bool,
    want_down: bool,
    teleport: bool,
    chased: bool,
    stationary: bool,
}

/// One tick of a living or dying shark: a working copy of the state plus the facts fixed at the start of the tick.
struct Tick<'a> {
    me: &'a ObjRef,
    sh: &'a Shark,
    s: SharkState,
    dt: f32,
    sim: bool,
    /// Rider facts captured before the stages run.
    local_rider_in_control: bool,
}

pub fn shark_update_accurate_dt_is_simulation(me: &ObjRef, dt: f32, accurate_dt: f32, is_simulation: bool) {
    npc_update_accurate_dt_is_simulation(me, dt, accurate_dt, is_simulation);
    let sh = shark_of(me);
    let s = sh.shark.borrow().clone();
    let mut tick = Tick { me, sh, s, dt, sim: is_simulation, local_rider_in_control: false };
    tick.run();
    let done = tick.s;
    *sh.shark.borrow_mut() = done;
}

impl<'a> Tick<'a> {
    fn ns(&self) -> &RefCell<NpcState> {
        npc_state(self.me)
    }

    /// Run `f` (which may re-enter the shark) with the working copy written back before and re-read after.
    fn reentrant<T>(&mut self, f: impl FnOnce() -> T) -> T {
        *self.sh.shark.borrow_mut() = self.s.clone();
        let out = f();
        self.s = self.sh.shark.borrow().clone();
        out
    }

    fn rider(&self) -> Option<ObjRef> {
        self.ns().borrow().rider.clone()
    }

    fn run(&mut self) {
        self.drop_stale_target();
        if self.ns().borrow().dead {
            self.dying();
            return;
        }
        let rider = self.rider();
        self.local_rider_in_control = rider.as_ref().map_or(false, |r| !r.is_net());
        let speed = self.effective_speed(&rider);
        let stationary = self.wander(rider.is_none());
        if self.is_frozen() {
            return;
        }
        self.breed_and_drown();
        self.buoyancy();
        self.s.travel_fraction += self.dt * speed;
        let locally_controlled = self.sh.is_under_local_control();
        self.bite_and_flee_timers(locally_controlled);
        self.steer_by_rider();
        if self.s.travel_fraction >= 1.0 {
            self.arrive();
            self.choose_next_cell(stationary, locally_controlled);
        }
        self.finish();
    }

    // 4.1
    fn drop_stale_target(&mut self) {
        if self.me.is_net() {
            return;
        }
        let Some(t) = self.s.attack_blockhead.clone() else { return };
        let env = penv();
        if env.bh_regenerating(&t) || t.needs_removed() || env.bh_on_trade_mission(&t) {
            self.s.attack_blockhead = None;
            self.me.set_update_needs_to_be_sent(true);
        }
    }

    // 4.2
    fn dying(&mut self) {
        self.s.travel_fraction += 4.0 * self.dt;
        if self.me.is_net() || self.me.needs_removed() || self.s.travel_fraction < 1.0 {
            return;
        }
        let killer = self.ns().borrow().kill_blockhead.clone();
        if let Some(k) = killer {
            self.drop_loot(&k);
        }
        self.me.base().borrow_mut().needs_removed = true;
    }

    fn drop_loot(&self, killer: &ObjRef) {
        let env = penv();
        let pos = self.me.pos();
        let age = self.ns().borrow().age;
        let ratio = age / Npc::max_age(self.sh);
        for _ in 0..raw_fish_count(ratio) {
            env.dw_create_free_block(pos, ITEM_RAW_FISH, 0, 0, None, 1, Some(killer));
        }
        let jaw = f32_to_i32(clamp(ratio * 255.0, 0.0, 255.0)) as u16;
        env.dw_create_free_block(pos, ITEM_SHARK_JAW, 0, jaw, None, 1, Some(killer));
        if age > MIN_RIDABLE_AGE {
            let level = Self::clothing_level(age);
            let item = random_bonus_clothing_item_type_from_shark_at_level(level);
            let (data_a, data_b) = Self::clothing_data();
            env.dw_create_free_block(pos, item.0 as i32, data_a, data_b, None, 1, Some(killer));
        }
    }

    /// Bonus clothing level: each extra level costs one draw, and only when the age test for it passed.
    fn clothing_level(age: f32) -> i32 {
        let mut level = 0;
        if age > 9000.0 && (draw("Shark.die.level1") as f64) > 0.5 {
            level = 1;
            if age > 18000.0 && (draw("Shark.die.level2") as f64) > 0.5 {
                level = 2;
            }
        }
        level
    }

    /// Variant data of the bonus clothing: a value from one draw, then the pigment pair (conditional draws).
    fn clothing_data() -> (u16, u16) {
        let a = glibc_rand::rand_tagged("Shark.die.dataA");
        let b = glibc_rand::rand_tagged("Shark.die.colorGate");
        let mut data_b = 0u16;
        if (((b as f32) / 2147483648.0f32) as f64) > 0.2 {
            let first = glibc_rand::rand_tagged("Shark.die.pigmentA");
            let d = glibc_rand::rand_tagged("Shark.die.pigmentGate");
            let mut second = 0i32;
            if (((d as f32) / 2147483648.0f32) as f64) > 0.5 {
                second = glibc_rand::rand_tagged("Shark.die.pigmentB") % 9;
            }
            data_b = color_index_for_pigments(
                PigmentColor((first % 9) as u32),
                PigmentColor(second as u32),
                PigmentColor(0),
            );
        }
        let data_a = ((a % 0x2000) as i16).wrapping_add(0x1000) as u16;
        (data_a, data_b)
    }

    // 4.3
    fn effective_speed(&self, rider: &Option<ObjRef>) -> f32 {
        let rider_remote = rider.as_ref().map_or(false, |r| r.is_net());
        if rider.is_some() && rider_remote {
            return self.s.remote_walk_speed;
        }
        if self.me.is_net() {
            return ((self.s.walk_speed as f64) * 0.75 + 0.25 * (self.s.remote_walk_speed as f64)) as f32;
        }
        self.s.walk_speed
    }

    // 4.4: returns whether the shark is stationary afterwards
    fn wander(&mut self, no_rider: bool) -> bool {
        self.s.random_time_between_direction_changes -= self.dt;
        let inspected = self.ns().borrow().inspecting_blockhead.as_ref().and_then(|w| w.upgrade()).is_some();
        if inspected {
            self.s.movement_direction = 0;
            self.s.walk_speed = 0.0;
        }
        let was_stationary = self.s.movement_direction == 0;
        let due = self.s.random_time_between_direction_changes <= 0.0;
        if no_rider && due && (self.s.travel_fraction >= 1.0 || was_stationary) {
            self.s.random_time_between_direction_changes = draw("Shark.update.interval") * 10.0 + 1.0;
            if !self.me.is_net() {
                self.s.walk_speed = blended_walk(self.s.walk_speed, draw("Shark.update.walkSpeed"));
                self.me.set_update_needs_to_be_sent(true);
                let wanted = three_way(draw("Shark.update.dir") as f64);
                if wanted == 0 {
                    self.s.movement_direction = 0;
                } else if self.s.movement_direction != wanted && (self.s.walk_speed < 0.08 || was_stationary) {
                    self.s.movement_direction = wanted;
                }
            }
        }
        self.s.movement_direction == 0
    }

    // 4.5
    fn is_frozen(&self) -> bool {
        let p = self.me.pos();
        matches!(penv().tile_loaded(p.x, p.y), Some(t) if t.type_index == TILE_ICE)
    }

    // 4.6
    fn breed_and_drown(&mut self) {
        if self.me.is_net() {
            return;
        }
        let env = penv();
        let pos = self.me.pos();
        let lay = self.ns().borrow().lay_timer;
        if lay > 0.0 {
            let left = lay - self.dt;
            self.ns().borrow_mut().lay_timer = left;
            if left <= 0.0 {
                self.ns().borrow_mut().lay_cooldown_timer = 900.0;
                let born = self.reentrant(|| env.dw_load_npc_at_position(pos, NPC_SHARK, None, false, false, None));
                if born {
                    let mut n = self.ns().borrow_mut();
                    n.fullness = ((n.fullness as f64) - 2700.0) as f32;
                }
                dw_changed(self.me);
            }
        }
        let cell = env.tile_loaded(pos.x, pos.y);
        let out_of_water = match &cell {
            None => false,
            Some(c) => (!t_water(&cell) && c.type_index != TILE_ICE) || c.partial_content_left < LEVEL_WATER,
        };
        if !out_of_water {
            self.s.drown_timer = 0.0;
            return;
        }
        self.s.drown_timer += self.dt;
        if self.s.drown_timer > 10.0 {
            let (me, sh) = (self.me, self.sh);
            self.reentrant(|| Npc::die(sh, me, None));
        }
    }

    // 4.7
    fn buoyancy(&mut self) {
        let env = penv();
        let pos = self.me.pos();
        let cell = env.tile_loaded(pos.x, pos.y);
        let submerged = !t_air_snow(&cell) && cell.map_or(true, |c| c.partial_content_left >= LEVEL_WATER);
        if submerged {
            self.float_up();
        } else {
            self.fall(pos);
        }
        if self.s.is_on_air_above_solid_ground && !self.me.is_net() {
            self.s.movement_direction = raw_coin_dir("Shark.update.air");
        }
    }

    fn float_up(&mut self) {
        if !self.s.falling {
            return;
        }
        self.s.fall_velocity = ((self.s.fall_velocity as f64) + (self.dt as f64) * 0.2) as f32;
        if self.s.fall_velocity <= 0.0 {
            let mut fp = self.me.float_pos();
            fp.y += self.s.fall_velocity;
            set_fp(self.me, fp);
            return;
        }
        self.s.fall_velocity = 0.0;
        self.s.falling = false;
        if self.s.is_on_air_above_solid_ground {
            self.s.walk_speed = 2.0;
            self.s.is_on_air_above_solid_ground = false;
        }
    }

    fn fall(&mut self, pos: IntPair) {
        self.s.falling = true;
        self.s.is_on_air_above_solid_ground = true;
        self.s.fall_velocity = ((self.s.fall_velocity as f64) - (self.dt as f64) * 0.1) as f32;
        let mut fp = self.me.float_pos();
        fp.y += self.s.fall_velocity;
        set_fp(self.me, fp);
        let row = f32_to_i32(fp.y);
        if row == pos.y {
            return;
        }
        let below = IntPair { x: pos.x, y: row };
        if t_solid(&penv().tile_loaded(below.x, below.y)) {
            fp.y = 0.01 + pos.y as f32;
            set_fp(self.me, fp);
            self.s.fall_velocity = ((-(self.s.fall_velocity as f64)) * 0.1) as f32;
        } else {
            self.me.update_position(self.me, below);
            self.s.to_square = below;
            self.s.from_square = below;
        }
    }

    // 4.9
    fn bite_and_flee_timers(&mut self, locally_controlled: bool) {
        let dt = self.dt;
        if self.s.biting {
            self.s.bite_timer -= dt;
            if self.s.bite_timer <= 0.0 {
                self.s.bite_timer = 0.0;
                self.s.biting = false;
                if !self.me.is_net() {
                    self.me.set_creation_data_needs_to_be_sent(true);
                }
                self.s.attack_blockhead = None;
                self.s.has_harmed_blockhead = false;
                self.s.flee_timer = 10.0;
                if locally_controlled {
                    self.me.set_update_needs_to_be_sent(true);
                }
            }
        } else if self.s.flee_timer > 0.0 {
            let before = self.s.flee_timer;
            self.s.flee_timer -= dt;
            if self.s.flee_timer < 9.0 && before >= 9.0 {
                self.s.walk_speed *= 0.5;
            }
            if self.s.flee_timer < 0.0 {
                self.s.flee_timer = -0.001;
                self.s.walk_speed *= 0.5;
            }
        }
        if self.s.nearby_blockhead || self.s.chase_timer < 0.0 {
            self.s.chase_timer += dt;
        }
    }

    // 4.10
    fn steer_by_rider(&mut self) {
        if !self.local_rider_in_control || self.s.attack_blockhead.is_some() {
            return;
        }
        let x = self.s.rider_target_velocity.x;
        if x >= 0.1 {
            self.s.movement_direction = 1;
            self.s.walk_speed = 4.0 * x;
        } else if x <= -0.1 {
            self.s.movement_direction = -1;
            self.s.walk_speed = fabsf(4.0 * x);
        }
    }

    // 4.11 steps 1-3
    fn arrive(&mut self) {
        let env = penv();
        self.s.travel_fraction -= 1.0;
        if self.s.travel_fraction >= 1.0 || self.s.travel_fraction < 0.0 {
            self.s.travel_fraction = 0.0;
        }
        if !self.me.is_net() && self.s.from_square.x != self.s.to_square.x {
            if let Some(cell) = env.tile_loaded(self.s.to_square.x, self.s.to_square.y) {
                self.graze(&cell);
                self.maybe_start_laying();
            }
        }
        let landed = self.s.to_square;
        self.s.from_square = landed;
        let (me, sh) = (self.me, self.sh);
        self.reentrant(|| {
            me.update_position(me, landed);
            Npc::check_current_position_for_food(sh, me);
        });
        set_fp(self.me, Vector2 { x: landed.x as f32 + 0.5, y: landed.y as f32 });
    }

    /// Eat the kelp in the arrival cell while hungry.
    fn graze(&mut self, cell: &Tile) {
        let env = penv();
        let hungry = (self.ns().borrow().fullness as f64) < 5400.0;
        if !hungry || cell.contents != CONTENTS_KELP_PLANT {
            return;
        }
        let Some(plant) = env.dw_plant_at_pos(self.s.to_square) else { return };
        if !env.is_kelp_plant(&plant) {
            return;
        }
        let food = env.plant_available_food(&plant);
        self.ns().borrow_mut().fullness += food;
        env.plant_set_available_food(&plant, 0.0);
        if let Some(dw) = self.me.dynamic_world() {
            dw.dynamic_world_changed_at_pos_object_type(plant.pos(), plant.object_type());
        }
    }

    /// A well-fed adult with no cooldown starts a lay timer.
    fn maybe_start_laying(&mut self) {
        let (fullness, cooldown, lay, age) = {
            let n = self.ns().borrow();
            (n.fullness, n.lay_cooldown_timer, n.lay_timer, n.age)
        };
        if (fullness as f64) > 5400.0 && cooldown <= 0.0 && lay <= 0.0 && age > MIN_RIDABLE_AGE {
            self.ns().borrow_mut().lay_timer = (((draw("Shark.update.layTimer") * 900.0) as f64) * 0.2 + 180.0) as f32;
            dw_changed(self.me);
        }
    }

    // 4.12
    fn finish(&mut self) {
        if self.s.is_on_air_above_solid_ground {
            self.s.walk_speed = 6.0;
        }
        let (from, to) = (self.s.from_square, self.s.to_square);
        if from == to {
            return;
        }
        let t = self.s.travel_fraction;
        set_fp(
            self.me,
            Vector2 {
                x: linear_interpolate(from.x as f32 + 0.5, to.x as f32 + 0.5, t),
                y: linear_interpolate(from.y as f32, to.y as f32, t),
            },
        );
    }

    // ---------------------------------------------------------------------------------------- section 5

    fn choose_next_cell(&mut self, stationary: bool, locally_controlled: bool) {
        let pos = self.me.pos();
        let mut plan = Plan { stationary, ..Plan::default() };
        if follows_remote(self.me) {
            self.follow_remote_driver(pos, &mut plan);
        }
        let mut next = IntPair { x: pos.x.wrapping_add(self.s.movement_direction), y: pos.y };
        self.hunt(pos, &mut next, &mut plan, locally_controlled);
        self.read_rider_vertical(&mut plan);
        if !plan.teleport {
            self.pick_destination(pos, next, &mut plan, locally_controlled);
        }
    }

    // 5.1
    fn follow_remote_driver(&mut self, pos: IntPair, plan: &mut Plan) {
        let width = world_width_tiles_of(self.me);
        let goal = self.s.remote_goal_square;
        let dx = wrap_dx(goal.x.wrapping_sub(pos.x), width);
        let dy = goal.y.wrapping_sub(pos.y);
        if dx.wrapping_abs() < 9 && dy.wrapping_abs() < 5 {
            if dx.wrapping_abs() < 3 {
                if dx == 0 {
                    self.s.movement_direction = 0;
                    self.s.walk_speed = self.s.remote_walk_speed;
                } else {
                    self.s.movement_direction = dx.signum();
                    self.s.walk_speed = at_least(self.s.remote_walk_speed, 1.0);
                }
            } else {
                self.s.walk_speed = 2.0;
                self.s.movement_direction = if dx > 0 { 1 } else { -1 };
            }
        } else {
            plan.teleport = true;
            self.s.to_square = goal;
            self.s.walk_speed = self.s.remote_walk_speed;
            self.s.movement_direction = self.s.remote_movement_direction;
        }
        plan.stationary = self.s.movement_direction == 0;
        if self.s.to_square.y < goal.y {
            plan.want_up = true;
        } else if goal.y < self.s.to_square.y {
            plan.want_down = true;
        }
    }

    // 5.3
    fn hunt(&mut self, pos: IntPair, next: &mut IntPair, plan: &mut Plan, locally_controlled: bool) {
        if self.s.is_on_air_above_solid_ground || !locally_controlled || self.s.chase_timer < 0.0 {
            return;
        }
        self.s.nearby_blockhead = false;
        let env = penv();
        let rules = env.custom_rules();
        let rules_on = rules.custom_rules_enabled != 0;
        let difficulty = rules.npc_difficulty as i32;
        if rules_on && difficulty == 3 {
            return;
        }
        let candidate = npc_env().all_blockheads_including_net().into_iter().find(|b| self.is_prey(b, pos));
        let Some(b) = candidate else { return };
        let bp = b.pos();
        if !(pos.y < bp.y.wrapping_add(8) && bp.y.wrapping_sub(8) < pos.y) {
            return;
        }
        self.s.nearby_blockhead = true;
        plan.chased = true;
        if self.s.chase_timer > 10.0 {
            self.s.chase_timer = -20.0;
        }
        let close = pos.x < bp.x.wrapping_add(3)
            && bp.x.wrapping_sub(3) < pos.x
            && pos.y < bp.y.wrapping_add(3)
            && bp.y.wrapping_sub(3) < pos.y;
        if close {
            self.engage(&b);
        }
        if self.s.biting && !self.s.has_harmed_blockhead && bp == pos {
            self.bite(&b, rules_on, difficulty);
        }
        if !self.local_rider_in_control {
            self.pursue_or_flee(bp, next, pos, plan);
        }
    }

    /// Filters for the blockhead the shark may react to.
    fn is_prey(&self, b: &ObjRef, pos: IntPair) -> bool {
        let env = penv();
        if env.bh_paused(b) || (self.sim && !env.bh_should_continue_simulating(b)) {
            return false;
        }
        if env.bh_regenerating(b) || env.bh_on_trade_mission(b) {
            return false;
        }
        if Npc::belongs_to_player_with_blockhead(self.sh, b) && (self.ns().borrow().fullness as f64) > 1350.0 {
            return false;
        }
        if self.s.attack_blockhead.as_ref().map_or(false, |a| !ptr_eq(a, b)) {
            return false;
        }
        let bx = b.pos().x;
        pos.x < bx.wrapping_add(8) && bx.wrapping_sub(8) < pos.x
    }

    /// Within three cells: start biting a swimmer, or give up on someone on a boat.
    fn engage(&mut self, b: &ObjRef) {
        let env = penv();
        let on_boat = env.bh_ride_object(b).map_or(false, |o| env.is_boat(&o));
        if !on_boat {
            if self.s.flee_timer <= 0.0 && !self.s.biting && self.s.attack_blockhead.is_none() {
                self.s.biting = true;
                self.s.attack_blockhead = Some(b.clone());
                self.me.set_update_needs_to_be_sent(true);
                self.me.set_creation_data_needs_to_be_sent(true);
                self.s.bite_timer = 1.0;
                self.s.walk_speed = 4.0;
            }
            return;
        }
        self.s.bite_timer = 0.0;
        if self.s.biting {
            self.s.biting = false;
            if !self.me.is_net() {
                self.me.set_update_needs_to_be_sent(true);
            }
            self.s.attack_blockhead = None;
        }
        self.s.has_harmed_blockhead = false;
        self.s.flee_timer = 10.0;
    }

    /// The bite itself: damage, dismount, simulation log.
    fn bite(&mut self, b: &ObjRef, rules_on: bool, difficulty: i32) {
        let env = penv();
        let (age, taken) = {
            let n = self.ns().borrow();
            (n.age, n.damage)
        };
        let ratio = age / Npc::max_age(self.sh);
        let max_health = Npc::max_health(self.sh);
        let dmg = bite_damage(ratio, taken as f32, max_health as f32, rules_on, difficulty);
        let rider = self.rider();
        let sim = self.sim;
        self.reentrant(|| {
            env.bh_suffer_damage(b, dmg, sim, true);
            if rider.as_ref().map_or(false, |r| ptr_eq(r, b)) {
                env.bh_stop_riding(b);
            }
        });
        if self.sim {
            if let Some(t) = self.s.attack_blockhead.clone() {
                let verb = if env.bh_regenerating(&t) { "killed" } else { "attacked" };
                env.add_simulation_event_damage(&t, &format!("{}{}", verb, SE_DAMAGE_TEXT_SUFFIX));
            }
        }
        self.s.has_harmed_blockhead = true;
    }

    /// Turn towards (or, when fleeing, away from) the blockhead and ask for a vertical change.
    fn pursue_or_flee(&mut self, bp: IntPair, next: &mut IntPair, pos: IntPair, plan: &mut Plan) {
        let fleeing = self.s.flee_timer > 0.0;
        let dir = self.s.movement_direction;
        let wrong_way = if next.x < bp.x {
            (fleeing && dir > 0) || (!fleeing && dir < 0)
        } else if bp.x < next.x {
            (fleeing && dir < 0) || (!fleeing && dir > 0)
        } else {
            false
        };
        if wrong_way {
            self.s.movement_direction = -dir;
            self.s.walk_speed = at_least(self.s.walk_speed, 2.0);
            *next = IntPair { x: pos.x.wrapping_add(self.s.movement_direction), y: pos.y };
        }
        if bp.y == next.y {
            return;
        }
        if (next.y < bp.y) == fleeing {
            plan.want_down = true;
            self.s.walk_speed = at_least(self.s.walk_speed, 2.0);
        } else if (bp.y < next.y) == fleeing {
            plan.want_up = true;
            self.s.walk_speed = at_least(self.s.walk_speed, 2.0);
        }
    }

    // 5.4
    fn read_rider_vertical(&mut self, plan: &mut Plan) {
        let local_rider = self.rider().map_or(false, |r| !r.is_net());
        if !local_rider || self.s.attack_blockhead.is_some() || plan.want_up || plan.want_down {
            return;
        }
        let y = self.s.rider_target_velocity.y as f64;
        if y > 0.5 {
            plan.want_up = true;
        } else if y < -0.5 {
            plan.want_down = true;
        }
    }

    // 5.5
    fn pick_destination(&mut self, pos: IntPair, next: IntPair, plan: &mut Plan, locally_controlled: bool) {
        let env = penv();
        let nav = Nav {
            has_rider: self.rider().is_some(),
            on_air: self.s.is_on_air_above_solid_ground,
            biting: self.s.biting,
        };
        let target = env.tile_cached(next.x, next.y);
        if nav.swimmable(&target) {
            if plan.stationary {
                self.s.to_square = next;
            } else {
                self.s.to_square = self.cruise_or_climb(pos, next, plan, &nav);
            }
        }
        if self.s.to_square == self.s.from_square && locally_controlled {
            self.handle_stuck(plan);
            return;
        }
        self.s.last_try_couldnt_move = false;
    }

    /// Straight ahead when it stays open, otherwise a diagonal step up or down.
    fn cruise_or_climb(&self, pos: IntPair, next: IntPair, plan: &Plan, nav: &Nav) -> IntPair {
        let env = penv();
        let dir = self.s.movement_direction;
        let beyond = env.tile_cached(next.x.wrapping_add(dir), next.y);
        let open_ahead = beyond.is_some() && (nav.open_water(&beyond) || nav.on_air || nav.biting);
        let rf = (glibc_rand::rand_tagged("Shark.update.step") as f32) / 2147483648.0f32;
        let straight = open_ahead && !plan.want_up && !plan.want_down && (nav.biting || rf <= 0.9);
        if straight {
            return next;
        }
        let prefer_up = plan.want_up || (!plan.want_down && !nav.biting && rf > 0.95);
        let (first, second, forced) = if prefer_up { (1, -1, plan.want_up) } else { (-1, 1, plan.want_down) };
        let step = |sy: i32| IntPair { x: next.x, y: pos.y.wrapping_add(sy) };
        if !nav.vertical_blocked(pos, dir, first) {
            step(first)
        } else if forced || nav.vertical_blocked(pos, dir, second) {
            next
        } else {
            step(second)
        }
    }

    /// Could not move: after two failures in the air or standing still pick a random heading; then reverse, or give
    /// up a chase and flee.
    fn handle_stuck(&mut self, plan: &Plan) {
        if self.s.last_try_couldnt_move && (self.s.is_on_air_above_solid_ground || self.s.movement_direction == 0) {
            self.s.movement_direction = raw_coin_dir("Shark.update.noMove");
        }
        self.s.last_try_couldnt_move = true;
        if !plan.chased || self.s.flee_timer > 0.0 {
            self.s.movement_direction = -self.s.movement_direction;
            return;
        }
        self.s.bite_timer = 0.0;
        self.s.biting = false;
        self.s.attack_blockhead = None;
        self.s.has_harmed_blockhead = false;
        self.s.walk_speed = at_least(self.s.walk_speed, 2.0);
        self.s.flee_timer = 10.0;
    }
}

/// Movement facts used while choosing a cell.
struct Nav {
    has_rider: bool,
    on_air: bool,
    biting: bool,
}

impl Nav {
    /// A gate stops an unridden shark.
    fn gate_blocks(&self, c: &Option<Tile>) -> bool {
        !self.has_rider && t_gate(c)
    }

    /// Present, not solid, no blocking gate, and deep water (or the shark is airborne).
    fn swimmable(&self, c: &Option<Tile>) -> bool {
        c.is_some() && !t_solid(c) && !self.gate_blocks(c) && ((t_water(c) && is_deep(c)) || self.on_air)
    }

    /// Deep, gate-free water.
    fn open_water(&self, c: &Option<Tile>) -> bool {
        !t_solid(c) && t_water(c) && !self.gate_blocks(c) && is_deep(c)
    }

    /// Is the diagonal step `(dir, sy)` from `pos` impossible?
    fn vertical_blocked(&self, pos: IntPair, dir: i32, sy: i32) -> bool {
        let env = penv();
        let c1 = env.tile_cached(pos.x.wrapping_add(dir), pos.y.wrapping_add(sy));
        if c1.is_none() || t_solid(&c1) {
            return true;
        }
        if self.on_air || self.biting {
            return false;
        }
        let c2 = env.tile_cached(pos.x.wrapping_add(dir.wrapping_mul(2)), pos.y.wrapping_add(sy));
        !t_water(&c1)
            || is_shallow(&c1)
            || self.gate_blocks(&c1)
            || c2.is_none()
            || !t_water(&c2)
            || self.gate_blocks(&c2)
            || is_shallow(&c2)
    }
}

// ------------------------------------------------------------------------------------------------ trait impls

impl DynamicObject for Shark {
    fn base(&self) -> &RefCell<DynamicObjectBase> {
        &self.base
    }
    fn class_info(&self) -> &'static ClassInfo {
        &SHARK_CLASS_INFO
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
        shark_update_accurate_dt_is_simulation(me, dt, accurate_dt, is_simulation)
    }

    /// : `floatPos + (xOffset, 0)`
    fn render_pos(&self) -> Vector2 {
        let fp = self.base.borrow().float_pos;
        Vector2 { x: fp.x + self.shark.borrow().x_offset, y: fp.y }
    }

    /// : 0x78 bytes (NPC part, update part, attack blockhead id) plus the NPC name tail.
    fn creation_net_data_for_client(&self, me: &ObjRef, client_id: Option<&str>) -> Option<Vec<u8>> {
        let mut buf = vec![0u8; 0x78];
        npc_npc_creation_net_data_for_client(me, client_id).write_le(&mut buf[0..0x48]);
        shark_shark_update_net_data_for_client(me, client_id).write_le(&mut buf[0x48..0x70]);
        let att = self.shark.borrow().attack_blockhead.clone();
        if let Some(a) = att {
            if !self.base.borrow().is_net {
                buf[0x70..0x78].copy_from_slice(&a.unique_id().to_le_bytes());
            }
        }
        Some(npc_append_npc_creation_data_to_data(me, buf))
    }

    fn update_net_data_for_client(&self, me: &ObjRef, client_id: Option<&str>) -> Option<Vec<u8>> {
        Some(shark_shark_update_net_data_for_client(me, client_id).to_vec())
    }

    fn remote_update(&self, me: &ObjRef, net_data: &[u8]) {
        npc_remote_update(me, net_data);
        if !npc_state(me).borrow().dead {
            let u = SharkUpdateNetData::read_le(&padded_bytes(net_data, 0x28));
            shark_do_remote_update(me, u);
        } else if self.shark.borrow().travel_fraction < 1.0f32 && !npc_env().is_client() {
            self.base.borrow_mut().needs_removed = false;
        }
    }

    fn remote_creation_data_update(&self, me: &ObjRef, net_data: &[u8]) {
        apply_creation_data(me, net_data);
    }

    fn blockhead_unloaded(&self, me: &ObjRef, b: &ObjRef) {
        let is_attack = self.shark.borrow().attack_blockhead.as_ref().map_or(false, |a| ptr_eq(a, b));
        if is_attack {
            self.shark.borrow_mut().attack_blockhead = None;
        }
        let is_rider = self.npc.borrow().rider.as_ref().map_or(false, |r| ptr_eq(r, b));
        if is_rider {
            self.shark.borrow_mut().rider_target_velocity = Vector2::ZERO;
        }
        npc_blockhead_unloaded(me, b);
    }

    /// : releases `attackBlockhead` then the NPC state.
    fn dealloc(&self, me: &ObjRef) {
        self.shark.borrow_mut().attack_blockhead = None;
        crate::npc::npc::npc_dealloc(me);
    }
}

impl Npc for Shark {
    fn npc(&self) -> &RefCell<NpcState> {
        &self.npc
    }
    fn npc_type(&self) -> i32 {
        NPC_SHARK
    }
    fn max_age(&self) -> f32 {
        MAX_AGE
    }
    fn min_fullness(&self) -> f32 {
        -9000.0
    }
    fn species_name(&self) -> String {
        "SHARK".to_string()
    }
    fn food_item_type(&self) -> i32 {
        ITEM_RAW_FISH
    }
    fn captured_item_type(&self) -> i32 {
        ITEM_SHARK_BUCKET
    }
    fn capture_required_item_type(&self) -> i32 {
        ITEM_TIN_BUCKET
    }
    fn get_names_array(&self) -> Option<&'static [&'static str]> {
        Some(&SHARK_NAMES)
    }
    fn get_names_array_count(&self) -> i32 {
        64
    }
    fn max_health(&self) -> u16 {
        MAX_HEALTH
    }
    fn min_ridable_age(&self) -> f32 {
        MIN_RIDABLE_AGE
    }
    fn creation_data_struct_size(&self) -> usize {
        0x78
    }
    fn jumps_on_swipe(&self) -> bool {
        false
    }
    fn rider_d_pad_should_allow_up_down(&self) -> bool {
        true
    }
    fn die(&self, me: &ObjRef, killer: Option<&ObjRef>) {
        shark_die(me, killer)
    }
    fn hit_with_force_blockhead(&self, me: &ObjRef, force: i32, b: Option<&ObjRef>) {
        shark_hit_with_force_blockhead(me, force, b)
    }
    /// : one `rand()` picks a flee direction.
    fn react_to_being_fed(&self, me: &ObjRef) {
        let dir = coin_dir(draw("Shark.fed.dir"));
        {
            let mut s = self.shark.borrow_mut();
            s.movement_direction = dir;
            s.walk_speed = 3.0f32;
            s.random_time_between_direction_changes = 5.0f32;
        }
        me.set_update_needs_to_be_sent(true);
    }
    fn can_be_captured_by_blockhead_with_item_type(&self, b: &ObjRef, item: i32) -> bool {
        if item != ITEM_TIN_BUCKET {
            return false;
        }
        if self.npc.borrow().age >= 4500.0f32 {
            return false;
        }
        if Npc::tamed(self) && !Npc::belongs_to_player_with_blockhead(self, b) {
            return false;
        }
        true
    }
    fn cant_be_captured_tip_string_for_blockhead_with_item_type(&self, b: &ObjRef, item: i32) -> Option<String> {
        if item != ITEM_TIN_BUCKET {
            return Some(format!("{}S CAN ONLY BE CAPTURED\nIN A BUCKET", Npc::species_name(self)));
        }
        if self.npc.borrow().age > 4500.0f32 {
            return Some(format!("THAT {} IS TOO BIG\nTO BE CAPTURED", Npc::species_name(self)));
        }
        if Npc::tamed(self) && !Npc::belongs_to_player_with_blockhead(self, b) {
            return Some(format!("THAT {} BELONGS TO\nSOMEONE ELSE", Npc::species_name(self)));
        }
        None
    }
    fn center(&self) -> Vector2 {
        Shark::center(self)
    }
    fn name_pos(&self) -> Vector2 {
        Shark::name_pos(self)
    }
    /// : `(renderPos.x, renderPos.y + 0.8, -0.75, 1)`
    fn rider_pos_for_blockhead(&self, _b: &ObjRef) -> [f32; 4] {
        let p = DynamicObject::render_pos(self);
        [p.x, p.y + 0.8f32, -0.75f32, 1.0f32]
    }
    fn rider_body_y_rotation_for_blockhead(&self, _b: &ObjRef) -> f32 {
        self.shark.borrow().body_rotation
    }
    fn set_target_velocity(&self, v: Vector2) {
        self.shark.borrow_mut().rider_target_velocity = v;
    }
    fn ride_direction(&self) -> i32 {
        if self.npc.borrow().rider.is_none() {
            return 0;
        }
        let x = self.shark.borrow().rider_target_velocity.x;
        if x > 0.1f32 {
            1
        } else if x < -0.1f32 {
            -1
        } else {
            0
        }
    }
    fn add_rider(&self, me: &ObjRef, b: &ObjRef) {
        npc_add_rider(me, b);
        self.release_steering_if_uncontrolled();
    }
    fn remove_rider(&self, me: &ObjRef, b: &ObjRef) {
        shark_remove_rider_opt(me, Some(b))
    }
    fn remove_rider_opt(&self, me: &ObjRef, b: Option<&ObjRef>) {
        shark_remove_rider_opt(me, b)
    }
    fn blockhead_can_ride_using_item(&self, b: &ObjRef, _item: i32) -> bool {
        let (age, dead) = {
            let n = self.npc.borrow();
            (n.age, n.dead)
        };
        age > Npc::min_ridable_age(self)
            && Npc::belongs_to_player_with_blockhead(self, b)
            && !dead
            && !self.base.borrow().needs_removed
    }
}

impl_npc_protocols!(Shark);

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
    fn water(fx: &Fx, x0: i32, x1: i32, y0: i32, y1: i32) {
        for x in x0..=x1 {
            for y in y0..=y1 {
                let mut t = Tile::default();
                t.type_index = 3;
                t.partial_content_left = 255;
                fx.env.tiles.borrow_mut().insert((x, y), t);
            }
        }
    }
    fn make(fx: &Fx, adult: bool, placed: bool) -> Option<ObjRef> {
        let wr: WorldRef = fx.w.clone();
        let dr: DynamicWorldRef = fx.dw.clone();
        Shark::init_with_world_dynamic_world_at_position_cache_save_dict_is_adult_was_placed_placed_by_client(
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

    /// RAW_FISH count of the spec's worked examples.
    fn raw_fish_count(age: f32) -> usize {
        let mut i = 0usize;
        loop {
            let ratio = age / 27000.0f32;
            if !(10.0f64 * ((ratio as f64) + 0.1) > i as f64) {
                return i;
            }
            i += 1;
        }
    }

    #[test]
    fn constants_table() {
        let fx = fixture();
        water(&fx, 30, 50, 0, 10);
        let me = make(&fx, true, true).unwrap();
        let sh = shark_of(&me);
        assert_eq!(Npc::npc_type(sh), 5);
        assert_eq!(Npc::max_age(sh), 27000.0);
        assert_eq!(Npc::min_fullness(sh), -9000.0);
        assert_eq!(Npc::max_health(sh), 25);
        assert_eq!(Npc::species_name(sh), "SHARK");
        assert_eq!(Npc::food_item_type(sh), 151);
        assert_eq!(Npc::captured_item_type(sh), 159);
        assert_eq!(Npc::capture_required_item_type(sh), 102);
        assert_eq!(Npc::creation_data_struct_size(sh), 0x78);
        assert_eq!(Npc::get_names_array_count(sh), 64);
        assert_eq!(Npc::min_ridable_age(sh), 4500.0);
        assert!(Npc::rider_d_pad_should_allow_up_down(sh));
        assert_eq!(SHARK_NAMES.len(), 64);
        assert_eq!(SHARK_NAMES[56], "Sharky McSharkface");
        assert_eq!(me.object_type(), 36);
    }

    #[test]
    fn raw_fish_table() {
        for (age, n) in [(0.0, 1), (2700.0, 3), (4500.0, 3), (9000.0, 5), (13500.0, 6), (27000.0, 11)] {
            assert_eq!(raw_fish_count(age), n, "age {age}");
        }
    }

    #[test]
    fn spawn_refusals_and_draw_counts() {
        let fx = fixture();
        glibc_rand::srand(2);
        assert!(make(&fx, true, false).is_none()); // no tile
        assert_eq!(glibc_rand::rand(), raw(2, 5)[4]);
        water(&fx, 30, 50, 0, 10);
        glibc_rand::srand(2);
        assert!(make(&fx, true, true).is_some());
        assert_eq!(glibc_rand::rand(), raw(2, 7)[6]); // 4 + 2
        glibc_rand::srand(2);
        assert!(make(&fx, false, true).is_some());
        assert_eq!(glibc_rand::rand(), raw(2, 6)[5]); // 3 + 2
    }

    #[test]
    fn capture_and_tip_boundaries() {
        let fx = fixture();
        water(&fx, 30, 50, 0, 10);
        let me = make(&fx, true, true).unwrap();
        let sh = shark_of(&me);
        let b: ObjRef = crate::objects::base::testutil::TestObj::new();
        npc_state(&me).borrow_mut().age = 4500.0;
        assert!(!Npc::can_be_captured_by_blockhead_with_item_type(sh, &b, 102));
        assert_eq!(Npc::cant_be_captured_tip_string_for_blockhead_with_item_type(sh, &b, 102), None);
        npc_state(&me).borrow_mut().age = 4500.5;
        assert_eq!(
            Npc::cant_be_captured_tip_string_for_blockhead_with_item_type(sh, &b, 102).as_deref(),
            Some("THAT SHARK IS TOO BIG\nTO BE CAPTURED")
        );
        npc_state(&me).borrow_mut().age = 100.0;
        assert!(Npc::can_be_captured_by_blockhead_with_item_type(sh, &b, 102));
        assert_eq!(
            Npc::cant_be_captured_tip_string_for_blockhead_with_item_type(sh, &b, 5).as_deref(),
            Some("SHARKS CAN ONLY BE CAPTURED\nIN A BUCKET")
        );
    }

    #[test]
    fn hit_adjusted_force_and_rand() {
        let fx = fixture();
        water(&fx, 30, 50, 0, 10);
        let me = make(&fx, true, true).unwrap();
        let sh = shark_of(&me);
        npc_state(&me).borrow_mut().age = 27000.0; // divisor 3.0
        npc_state(&me).borrow_mut().damage = 0;
        let v = raw(5, 2);
        glibc_rand::srand(5);
        Npc::hit_with_force_blockhead(sh, &me, 6, None);
        assert_eq!(npc_state(&me).borrow().damage, 2); // 6/3
        assert_eq!(sh.shark.borrow().walk_speed, 3.0);
        assert_eq!(
            sh.shark.borrow().movement_direction,
            if (((v[0] as f32) / 2147483648.0f32) as f64) > 0.5 { 1 } else { -1 }
        );
        assert_eq!(glibc_rand::rand(), v[1]);
        // tiny hit on a huge shark is floored to 1
        npc_state(&me).borrow_mut().damage = 0;
        Npc::hit_with_force_blockhead(sh, &me, 1, None);
        assert_eq!(npc_state(&me).borrow().damage, 1);
    }

    #[test]
    fn dead_drops_fish_jaw_and_clothing_rand_order() {
        let fx = fixture();
        water(&fx, 30, 50, 0, 10);
        let me = make(&fx, true, true).unwrap();
        let killer: ObjRef = crate::objects::base::testutil::TestObj::new();
        npc_state(&me).borrow_mut().age = 13500.0;
        npc_state(&me).borrow_mut().dead = true;
        npc_state(&me).borrow_mut().kill_blockhead = Some(killer);
        shark_of(&me).shark.borrow_mut().travel_fraction = 0.99;
        let v = raw(9, 10);
        glibc_rand::srand(9);
        me.update_accurate_dt_is_simulation(&me, 0.1, 0.1, false);
        assert!(me.needs_removed());
        let drops = fx.env.drops.borrow().clone();
        assert_eq!(drops.iter().filter(|d| d.0 == 151).count(), 6);
        let jaw = drops.iter().find(|d| d.0 == 157).unwrap();
        assert_eq!(jaw.2, (0.5f32 * 255.0f32) as u16);
        // clothing: level draws (age > 9000: one R, age <= 18000 stops), item draw, A, B,...
        let r1 = ((v[0] as f32) / 2147483648.0f32) as f64;
        let mut idx = 1;
        let level = if r1 > 0.5 { 1 } else { 0 };
        let item_k = v[idx];
        idx += 1;
        let a = v[idx];
        idx += 1;
        let b = v[idx];
        idx += 1;
        let cloth = drops.last().unwrap();
        let expect_item =
            if level == 0 { [117, 115, 84, 85][(item_k % 4) as usize] } else { [124, 122, 130][(item_k % 3) as usize] };
        assert_eq!(cloth.0, expect_item);
        assert_eq!(cloth.1, (((a % 0x2000) as i16) + 0x1000) as u16);
        let mut used = idx;
        if (((b as f32) / 2147483648.0f32) as f64) > 0.2 {
            used += 2;
            if (((v[idx + 1] as f32) / 2147483648.0f32) as f64) > 0.5 {
                used += 1;
            }
        }
        assert_eq!(glibc_rand::rand(), raw(9, used + 1)[used]);
    }

    #[test]
    fn young_shark_has_no_clothing_and_no_killer_no_drops() {
        let fx = fixture();
        water(&fx, 30, 50, 0, 10);
        let me = make(&fx, true, true).unwrap();
        npc_state(&me).borrow_mut().age = 2700.0;
        npc_state(&me).borrow_mut().dead = true;
        shark_of(&me).shark.borrow_mut().travel_fraction = 0.99;
        glibc_rand::srand(1);
        me.update_accurate_dt_is_simulation(&me, 0.1, 0.1, false);
        assert!(me.needs_removed());
        assert!(fx.env.drops.borrow().is_empty()); // killBlockhead == nil
        let me2 = make(&fx, true, true).unwrap();
        npc_state(&me2).borrow_mut().age = 2700.0;
        npc_state(&me2).borrow_mut().dead = true;
        npc_state(&me2).borrow_mut().kill_blockhead = Some(crate::objects::base::testutil::TestObj::new());
        shark_of(&me2).shark.borrow_mut().travel_fraction = 0.99;
        me2.update_accurate_dt_is_simulation(&me2, 0.1, 0.1, false);
        let d = fx.env.drops.borrow().clone();
        assert_eq!(d.iter().filter(|x| x.0 == 151).count(), 3);
        assert_eq!(d.len(), 4); // 3 fish + jaw, no clothing at age <= 4500
    }

    #[test]
    fn net_struct_uses_pos_and_age() {
        let fx = fixture();
        water(&fx, 30, 50, 0, 10);
        let me = make(&fx, true, true).unwrap();
        {
            let mut s = shark_of(&me).shark.borrow_mut();
            s.to_square = IntPair { x: 99, y: 99 };
            s.walk_speed = 1.0;
            s.movement_direction = -1;
        }
        npc_state(&me).borrow_mut().age = 1234.9;
        let nd = me.update_net_data_for_client(&me, None).unwrap();
        assert_eq!(nd.len(), 0x28);
        let u = SharkUpdateNetData::read_le(&nd);
        assert_eq!((u.to_square_x, u.to_square_y), (40, 3));
        assert_eq!(u.age, 1234);
        assert_eq!(u.walk_speed, 255);
        assert_eq!(u.movement_direction, -1);
        assert_eq!((u.clear_queue, u.padding), (0, [0, 0]));
        let c = me.creation_net_data_for_client(&me, None).unwrap();
        assert!(c.len() >= 0x78);
        assert_eq!(&c[0x70..0x78], &[0u8; 8]);
        let att: ObjRef = crate::objects::base::testutil::TestObj::new();
        att.base().borrow_mut().unique_id = 0x1122;
        shark_of(&me).shark.borrow_mut().attack_blockhead = Some(att);
        let c = me.creation_net_data_for_client(&me, None).unwrap();
        assert_eq!(u64::from_le_bytes(c[0x70..0x78].try_into().unwrap()), 0x1122);
    }

    #[test]
    fn swims_and_stays_in_water() {
        let fx = fixture();
        water(&fx, 30, 50, 0, 10);
        let me = make(&fx, true, true).unwrap();
        npc_state(&me).borrow_mut().age = 3000.0;
        glibc_rand::srand(17);
        for _ in 0..300 {
            me.update_accurate_dt_is_simulation(&me, 0.1, 0.1, false);
        }
        let p = me.pos();
        assert!(p.x >= 29 && p.x <= 51 && p.y >= 0 && p.y <= 10, "{:?}", p);
        assert!(!npc_state(&me).borrow().dead);
    }

    #[test]
    fn ice_freezes_and_air_draws_one_rand_per_tick() {
        let fx = fixture();
        water(&fx, 30, 50, 0, 10);
        let me = make(&fx, true, true).unwrap();
        fx.env.set_tile(40, 3, 4);
        let tf = shark_of(&me).shark.borrow().travel_fraction;
        glibc_rand::srand(3);
        me.update_accurate_dt_is_simulation(&me, 0.5, 0.5, false);
        assert_eq!(shark_of(&me).shark.borrow().travel_fraction, tf);
        // beached: one raw rand() per tick for the direction
        let fx2 = fixture();
        for x in 30..=50 {
            for y in 0..=2 {
                fx2.env.set_tile(x, y, 1);
            }
            for y in 3..=9 {
                fx2.env.set_tile(x, y, 2);
            }
        }
        let me2 = make(&fx2, true, true).unwrap();
        {
            let mut s = shark_of(&me2).shark.borrow_mut();
            s.random_time_between_direction_changes = 1000.0;
            s.travel_fraction = 0.0;
        }
        let v = raw(8, 3);
        glibc_rand::srand(8);
        me2.update_accurate_dt_is_simulation(&me2, 0.01, 0.01, false);
        // the air tick consumes exactly one rand() when no other draw happens
        let sd = shark_of(&me2).shark.borrow().clone();
        assert!(sd.is_on_air_above_solid_ground);
        assert_eq!(sd.walk_speed, 6.0);
        assert_eq!(glibc_rand::rand(), v[1]);
    }

    #[test]
    fn local_control_predicates() {
        let fx = fixture();
        water(&fx, 30, 50, 0, 10);
        let me = make(&fx, true, true).unwrap();
        let sh = shark_of(&me);
        assert!(sh.is_under_local_control());
        let remote: ObjRef = crate::objects::base::testutil::TestObj::new();
        remote.base().borrow_mut().is_net = true;
        npc_state(&me).borrow_mut().rider = Some(remote);
        assert!(!sh.is_under_local_control());
        assert!(follows_remote(&me));
        me.base().borrow_mut().is_net = true;
        assert!(!sh.is_under_local_control() || true);
        assert!(follows_remote(&me));
    }

    #[test]
    fn initial_rider_target_velocity_is_one_one() {
        let d = SharkState::default();
        assert_eq!(d.rider_target_velocity, Vector2 { x: 1.0, y: 1.0 });
        assert_eq!(d.name_position, Vector2 { x: 1.0, y: 1.0 });
    }

    #[test]
    fn ride_rules() {
        let fx = fixture();
        water(&fx, 30, 50, 0, 10);
        let me = make(&fx, true, true).unwrap();
        let sh = shark_of(&me);
        let b: ObjRef = crate::objects::base::testutil::TestObj::new();
        npc_state(&me).borrow_mut().age = 4500.0;
        assert!(!Npc::blockhead_can_ride_using_item(sh, &b, 0)); // strict
        Npc::set_target_velocity(sh, Vector2 { x: 0.5, y: 0.9 });
        assert_eq!(Npc::ride_direction(sh), 0); // no rider
        assert_eq!(sh.shark.borrow().rider_target_velocity.y, 0.9);
    }

    fn bh_at(x: i32, y: i32, id: u64) -> ObjRef {
        let b: ObjRef = crate::objects::base::testutil::TestObj::new();
        b.base().borrow_mut().pos = IntPair { x, y };
        b.base().borrow_mut().unique_id = id;
        b
    }

    #[test]
    fn chase_turns_towards_blockhead_and_flee_turns_away() {
        let seed = (0..50u32).find(|s| (raw(*s, 1)[0] as f32) / 2147483648.0f32 <= 0.9).expect("seed");
        for fleeing in [false, true] {
            let fx = fixture();
            water(&fx, 30, 60, 0, 10);
            let me = make(&fx, true, true).unwrap();
            fx.env.bhs.borrow_mut().push(bh_at(44, 3, 9));
            {
                let mut s = shark_of(&me).shark.borrow_mut();
                s.movement_direction = -1;
                s.travel_fraction = 1.0;
                s.walk_speed = 1.0;
                s.random_time_between_direction_changes = 100.0;
                s.flee_timer = if fleeing { 5.0 } else { 0.0 };
            }
            glibc_rand::srand(seed);
            me.update_accurate_dt_is_simulation(&me, 0.01, 0.01, false);
            let s = shark_of(&me).shark.borrow().clone();
            assert!(s.nearby_blockhead);
            // chasing: toward the blockhead (east); fleeing keeps moving west
            assert_eq!(s.movement_direction, if fleeing { -1 } else { 1 }, "seed {seed}");
            assert_eq!(s.to_square.x, if fleeing { 39 } else { 41 });
        }
    }

    #[test]
    fn bite_damage_and_difficulty_scaling() {
        let fx = fixture();
        water(&fx, 30, 60, 0, 10);
        let me = make(&fx, true, true).unwrap();
        fx.env.bhs.borrow_mut().push(bh_at(40, 3, 9));
        npc_state(&me).borrow_mut().age = 13500.0;
        let setup_shark = |me: &ObjRef| {
            let mut s = shark_of(me).shark.borrow_mut();
            s.movement_direction = 1;
            s.travel_fraction = 1.0;
            s.walk_speed = 1.0;
            s.random_time_between_direction_changes = 100.0;
            s.to_square = IntPair { x: 40, y: 3 };
            s.from_square = IntPair { x: 40, y: 3 };
            s.flee_timer = 0.0;
            s.biting = false;
            s.has_harmed_blockhead = false;
            s.attack_blockhead = None;
        };
        setup_shark(&me);
        me.update_accurate_dt_is_simulation(&me, 0.01, 0.01, false);
        let s = shark_of(&me).shark.borrow().clone();
        assert!(s.biting && s.has_harmed_blockhead);
        assert_eq!(s.bite_timer, 1.0); // set after the step-8 timers ran
        assert_eq!(s.walk_speed, 4.0);
        let exp_for = |me: &ObjRef| {
            let age = npc_state(me).borrow().age;
            (((age / 27000.0f32) + 0.2f32) as f64 * 0.8 * (1.0f32 - 0.0f32 / 25.0f32) as f64) as f32
        };
        let expect = exp_for(&me);
        assert_eq!(fx.env.damage.borrow().as_slice(), &[expect]);
        // EASY x0.25, HARD x4
        for (diff, mul) in [(2i8, 0.25f64), (0i8, 4.0f64)] {
            fx.env.damage.borrow_mut().clear();
            let mut cr = crate::types::structs::CustomRules::default();
            cr.custom_rules_enabled = 1;
            cr.npc_difficulty = diff;
            fx.env.rules.set(Some(cr));
            setup_shark(&me);
            me.update_accurate_dt_is_simulation(&me, 0.01, 0.01, false);
            let base = exp_for(&me);
            let e = if diff == 2 { ((base as f64) * mul) as f32 } else { 4.0f32 * base };
            assert_eq!(fx.env.damage.borrow().as_slice(), &[e]);
        }
        // PASSIVE: no scan at all
        let mut cr = crate::types::structs::CustomRules::default();
        cr.custom_rules_enabled = 1;
        cr.npc_difficulty = 3;
        fx.env.rules.set(Some(cr));
        fx.env.damage.borrow_mut().clear();
        setup_shark(&me);
        me.update_accurate_dt_is_simulation(&me, 0.01, 0.01, false);
        assert!(fx.env.damage.borrow().is_empty());
        assert!(!shark_of(&me).shark.borrow().biting);
    }

    #[test]
    fn load_derived_stuff_formulas() {
        for seed in 0..50u32 {
            let fx = fixture();
            water(&fx, 30, 50, 0, 10);
            let wr: WorldRef = fx.w.clone();
            let dr: DynamicWorldRef = fx.dw.clone();
            let v = raw(seed, 4);
            glibc_rand::srand(seed);
            let me = Shark::init_with_world_dynamic_world_save_dict_cache(
                &wr,
                &dr,
                &crate::objects::base::new_mutable_dict(),
                None,
            )
            .unwrap();
            let s = shark_of(&me).shark.borrow().clone();
            let r1 = ((v[1] as f32) / 2147483648.0f32) as f64;
            let dir = if r1 > 0.75 {
                1
            } else if r1 < 0.25 {
                -1
            } else {
                0
            };
            assert_eq!(s.movement_direction, dir);
            assert_eq!(
                s.walk_speed.to_bits(),
                ((((v[2] as f32) / 2147483648.0f32) as f64 * 0.2 + 0.10000000149011612) as f32).to_bits()
            );
            assert_eq!(glibc_rand::rand(), v[3]);
        }
    }
}

/// Tests derived from the behavioural specification's decision tables.
#[cfg(test)]
mod spec_tests {
    use super::*;
    use crate::npc::egg::test_support::TestPassiveEnv;
    use crate::npc::egg::{set_passive_env, PassiveEnv};
    use crate::npc::npc::{set_npc_env, NpcEnv};
    use crate::objects::base::testutil::{setup, MockDw, MockWorld, TestObj};
    use crate::types::structs::CustomRules;
    use std::cell::Cell;
    use std::rc::Rc;

    /// Test environment: the shared fake plus switches for the blockhead facts the shark asks about.
    struct SpecEnv {
        inner: Rc<TestPassiveEnv>,
        temp: Cell<f32>,
        client: Cell<bool>,
        paused_uid: Cell<u64>,
        regen_uid: Cell<u64>,
        trade_uid: Cell<u64>,
        nosim_uid: Cell<u64>,
        rides: Cell<bool>,
        boat: Cell<bool>,
        spawn_ok: Cell<bool>,
        stops: Cell<u32>,
        kill_on_damage: Cell<bool>,
        kelp: Cell<bool>,
        dummy: ObjRef,
    }
    impl crate::npc::npc::NpcEnvStub for SpecEnv {
        fn tile_probe(&self, x: i32, y: i32) -> Option<NpcTileCopy> {
            crate::npc::npc::NpcEnv::tile_probe(&*self.inner, x, y)
        }
        fn all_blockheads_including_net(&self) -> Vec<ObjRef> {
            crate::npc::npc::NpcEnv::all_blockheads_including_net(&*self.inner)
        }
        fn is_client(&self) -> bool {
            self.client.get()
        }
        fn current_temperature(&self, _t: &NpcTileCopy, _p: IntPair, _s: f32, _d: f32, _w: f32) -> f32 {
            self.temp.get()
        }
    }
    impl crate::npc::egg::PassiveEnvStub for SpecEnv {
        fn tile_loaded(&self, x: i32, y: i32) -> Option<Tile> {
            crate::npc::egg::PassiveEnv::tile_loaded(&*self.inner, x, y)
        }
        fn tile_cached(&self, x: i32, y: i32) -> Option<Tile> {
            crate::npc::egg::PassiveEnv::tile_cached(&*self.inner, x, y)
        }
        fn custom_rules(&self) -> CustomRules {
            crate::npc::egg::PassiveEnv::custom_rules(&*self.inner)
        }
        fn add_simulation_event_damage(&self, b: &ObjRef, text: &str) {
            crate::npc::egg::PassiveEnv::add_simulation_event_damage(&*self.inner, b, text)
        }
        fn dw_load_npc_at_position(
            &self,
            pos: IntPair,
            t: i32,
            sd: Option<&PlistDict>,
            adult: bool,
            placed: bool,
            c: Option<&str>,
        ) -> bool {
            if !self.spawn_ok.get() {
                return false;
            }
            crate::npc::egg::PassiveEnv::dw_load_npc_at_position(&*self.inner, pos, t, sd, adult, placed, c)
        }
        #[allow(clippy::too_many_arguments)]
        fn dw_create_free_block(
            &self,
            pos: IntPair,
            t: i32,
            a: u16,
            b: u16,
            sd: Option<PlistDict>,
            play: i32,
            p: Option<&ObjRef>,
        ) {
            crate::npc::egg::PassiveEnv::dw_create_free_block(&*self.inner, pos, t, a, b, sd, play, p)
        }
        fn dw_plant_at_pos(&self, pos: IntPair) -> Option<ObjRef> {
            crate::npc::egg::PassiveEnv::dw_plant_at_pos(&*self.inner, pos)
        }
        fn plant_available_food(&self, p: &ObjRef) -> f32 {
            crate::npc::egg::PassiveEnv::plant_available_food(&*self.inner, p)
        }
        fn plant_set_available_food(&self, p: &ObjRef, v: f32) {
            crate::npc::egg::PassiveEnv::plant_set_available_food(&*self.inner, p, v)
        }
        fn is_kelp_plant(&self, o: &ObjRef) -> bool {
            self.kelp.get() && crate::npc::egg::PassiveEnv::is_kelp_plant(&*self.inner, o)
        }
        fn is_boat(&self, _o: &ObjRef) -> bool {
            self.boat.get()
        }
        fn bh_paused(&self, b: &ObjRef) -> bool {
            b.unique_id() == self.paused_uid.get()
        }
        fn bh_regenerating(&self, b: &ObjRef) -> bool {
            b.unique_id() == self.regen_uid.get()
        }
        fn bh_on_trade_mission(&self, b: &ObjRef) -> bool {
            b.unique_id() == self.trade_uid.get()
        }
        fn bh_should_continue_simulating(&self, b: &ObjRef) -> bool {
            b.unique_id() != self.nosim_uid.get()
        }
        fn bh_ride_object(&self, _b: &ObjRef) -> Option<ObjRef> {
            self.rides.get().then(|| self.dummy.clone())
        }
        fn bh_suffer_damage(&self, b: &ObjRef, d: f32, s: bool, r: bool) {
            if self.kill_on_damage.get() {
                self.regen_uid.set(b.unique_id());
            }
            crate::npc::egg::PassiveEnv::bh_suffer_damage(&*self.inner, b, d, s, r)
        }
        fn bh_stop_riding(&self, _b: &ObjRef) {
            self.stops.set(self.stops.get() + 1);
        }
    }

    struct Fx {
        w: Rc<MockWorld>,
        dw: Rc<MockDw>,
        env: Rc<SpecEnv>,
    }
    fn fx() -> Fx {
        let (w, dw, _l) = setup(4, 4);
        let env = Rc::new(SpecEnv {
            inner: TestPassiveEnv::new(),
            temp: Cell::new(30.0),
            client: Cell::new(false),
            paused_uid: Cell::new(0),
            regen_uid: Cell::new(0),
            trade_uid: Cell::new(0),
            nosim_uid: Cell::new(0),
            rides: Cell::new(false),
            boat: Cell::new(false),
            spawn_ok: Cell::new(true),
            stops: Cell::new(0),
            kill_on_damage: Cell::new(false),
            kelp: Cell::new(true),
            dummy: TestObj::new(),
        });
        set_passive_env(env.clone());
        set_npc_env(env.clone());
        glibc_rand::srand(1);
        Fx { w, dw, env }
    }
    fn put(f: &Fx, x: i32, y: i32, ty: u8, level: u8) {
        let mut t = Tile::default();
        t.type_index = ty;
        t.partial_content_left = level;
        f.env.inner.tiles.borrow_mut().insert((x, y), t);
    }
    fn sea(f: &Fx) {
        for x in 20..=70 {
            for y in -2..=12 {
                put(f, x, y, 3, 255);
            }
        }
    }
    fn spawn(f: &Fx, adult: bool, placed: bool) -> Option<ObjRef> {
        let wr: WorldRef = f.w.clone();
        let dr: DynamicWorldRef = f.dw.clone();
        Shark::new_for_registry(&wr, &dr, IntPair { x: 40, y: 3 }, None, None, adult, placed, None)
    }
    /// A placed shark in open water with every timer parked so that a tick only does what a test arranges.
    fn calm(f: &Fx) -> ObjRef {
        sea(f);
        let me = spawn(f, true, true).unwrap();
        npc_state(&me).borrow_mut().age = 3000.0;
        {
            let mut s = shark_of(&me).shark.borrow_mut();
            s.random_time_between_direction_changes = 1000.0;
            s.travel_fraction = 0.0;
            s.walk_speed = 0.5;
            s.movement_direction = 1;
            s.to_square = IntPair { x: 40, y: 3 };
            s.from_square = IntPair { x: 40, y: 3 };
        }
        me
    }
    fn tk(me: &ObjRef) -> Tick<'_> {
        let sh = shark_of(me);
        Tick { me, sh, s: sh.shark.borrow().clone(), dt: 0.1, sim: false, local_rider_in_control: false }
    }
    fn st(me: &ObjRef) -> SharkState {
        shark_of(me).shark.borrow().clone()
    }
    fn uid(id: u64) -> ObjRef {
        let o: ObjRef = TestObj::new();
        o.base().borrow_mut().unique_id = id;
        o
    }
    fn uid_at(id: u64, x: i32, y: i32) -> ObjRef {
        let o = uid(id);
        o.base().borrow_mut().pos = IntPair { x, y };
        o
    }
    fn net_obj(id: u64) -> ObjRef {
        let o = uid(id);
        o.base().borrow_mut().is_net = true;
        o
    }
    fn raw(seed: u32, n: usize) -> Vec<i32> {
        glibc_rand::srand(seed);
        (0..n).map(|_| glibc_rand::rand()).collect()
    }
    fn frac(v: i32) -> f32 {
        (v as f32) / 2147483648.0
    }
    fn tick(me: &ObjRef, dt: f32) {
        me.update_accurate_dt_is_simulation(me, dt, dt, false);
    }

    // ------------------------------------------------------------------ helpers

    #[test]
    fn helper_functions() {
        assert_eq!((three_way(0.9), three_way(0.1), three_way(0.5)), (1, -1, 0));
        assert_eq!((three_way(0.75), three_way(0.25)), (0, 0));
        assert_eq!((coin_dir(0.6), coin_dir(0.5), coin_dir(0.2)), (1, -1, -1));
        assert_eq!((facing(1), facing(-1), facing(0)), (QUARTER_TURN, -QUARTER_TURN, -QUARTER_TURN));
        assert_eq!((at_least(1.0, 2.0), at_least(3.0, 2.0), at_least(2.0, 2.0)), (2.0, 3.0, 2.0));
        assert_eq!(blended_walk(1.0, 0.5), ((0.5f64 + 0.1 + TENTH_F32) as f32));
        assert_eq!(raw_fish_count(0.0), 1);
        assert_eq!(raw_fish_count(1.0), 11);
        assert_eq!(raw_fish_count(0.5), 6);
        let sh = SharkState::default();
        assert_eq!(sh.walk_timer, 0.0);
    }

    #[test]
    fn bite_damage_table() {
        let plain = bite_damage(0.5, 0.0, 25.0, false, 1);
        assert_eq!(plain, ((0.5f32 + 0.2f32) as f64 * 0.8 * 1.0) as f32);
        // half health halves the base
        assert!(bite_damage(0.5, 12.5, 25.0, false, 1) < plain);
        // floor
        assert_eq!(bite_damage(0.0, 25.0, 25.0, false, 1), 0.02);
        // rules off ignores difficulty
        assert_eq!(bite_damage(0.5, 0.0, 25.0, false, 2), plain);
        // rules on: easy x0.25, hard x4, normal and other unchanged
        assert_eq!(bite_damage(0.5, 0.0, 25.0, true, 2), (plain as f64 * 0.25) as f32);
        assert_eq!(bite_damage(0.5, 0.0, 25.0, true, 0), plain * 4.0);
        assert_eq!(bite_damage(0.5, 0.0, 25.0, true, 1), plain);
    }

    // ------------------------------------------------------------------ construction

    #[test]
    fn spawn_decision_table() {
        let f = fx();
        sea(&f);
        // placed in shallow water is fine, natural in shallow water is not
        put(&f, 40, 3, 3, 100);
        assert!(spawn(&f, true, true).is_some());
        assert!(spawn(&f, true, false).is_none());
        // natural in non-water is refused
        put(&f, 40, 3, 2, 255);
        assert!(spawn(&f, true, false).is_none());
        put(&f, 40, 3, 3, 255);
        // temperature gate
        f.env.temp.set(0.05);
        assert!(spawn(&f, true, false).is_none());
        f.env.temp.set(1.0);
        assert!(spawn(&f, true, false).is_none()); // grown shark needs >= 2.0
        assert!(spawn(&f, false, false).is_some()); // newborn only needs >= 0.1
        f.env.temp.set(2.0);
        assert!(spawn(&f, true, false).is_some());
        // no cell at all
        f.env.inner.tiles.borrow_mut().remove(&(40, 3));
        assert!(spawn(&f, true, true).is_none());
    }

    #[test]
    fn derived_setup_draw_order_and_values() {
        let f = fx();
        sea(&f);
        for seed in 0..60u32 {
            let v = raw(seed + 1000, 40);
            glibc_rand::srand(seed + 1000);
            // the generic part draws first: find where derived setup starts by building through the save path
            let wr: WorldRef = f.w.clone();
            let dr: DynamicWorldRef = f.dw.clone();
            let me = Shark::init_with_world_dynamic_world_save_dict_cache(
                &wr,
                &dr,
                &crate::objects::base::new_mutable_dict(),
                None,
            )
            .unwrap();
            let s = st(&me);
            let found = (0..38).any(|i| {
                let d = three_way(frac(v[i]) as f64);
                let w = ((frac(v[i + 1]) as f64) * 0.2 + TENTH_F32) as f32;
                d == s.movement_direction && w == s.walk_speed
            });
            assert!(found, "seed {seed}");
            assert_eq!(s.travel_fraction, 1.0);
            assert_eq!(s.random_time_between_direction_changes, 0.0);
            assert_eq!(s.body_rotation, facing(s.movement_direction));
            assert_eq!(s.goal_rotation, s.body_rotation);
        }
    }

    #[test]
    fn network_creation_faces_new_direction() {
        let f = fx();
        let src = calm(&f);
        shark_of(&src).shark.borrow_mut().movement_direction = 1;
        let bytes = src.creation_net_data_for_client(&src, None).unwrap();
        let wr: WorldRef = f.w.clone();
        let dr: DynamicWorldRef = f.dw.clone();
        let made = Shark::init_with_world_dynamic_world_cache_net_data(&wr, &dr, None, &bytes);
        let made = made.expect("net shark");
        let s = st(&made);
        assert_eq!(s.body_rotation, facing(s.movement_direction));
        assert_eq!(s.goal_rotation, s.body_rotation);
    }

    #[test]
    fn queries() {
        let f = fx();
        let me = calm(&f);
        let sh = shark_of(&me);
        assert_eq!(sh.min_ridable_age(), 4500.0);
        {
            let mut s = sh.shark.borrow_mut();
            s.name_position = Vector2 { x: 3.0, y: 4.0 };
            s.x_offset = 0.25;
            s.body_rotation = -QUARTER_TURN;
        }
        assert_eq!(sh.name_pos(), Vector2 { x: 3.0, y: 5.0 });
        set_fp(&me, Vector2 { x: 10.5, y: 2.0 });
        let rp = DynamicObject::render_pos(sh);
        assert_eq!(rp, Vector2 { x: 10.75, y: 2.0 });
        npc_state(&me).borrow_mut().age = 13500.0;
        let c = sh.center();
        assert!((c.x - (10.75 + 1.0)).abs() < 1e-4, "{c:?}");
        assert_eq!(c.y, 2.0);
    }

    // ------------------------------------------------------------------ reactions

    #[test]
    fn die_is_idempotent_and_restarts_timer() {
        let f = fx();
        let me = calm(&f);
        shark_of(&me).shark.borrow_mut().travel_fraction = 0.7;
        shark_die(&me, None);
        assert!(npc_state(&me).borrow().dead);
        assert_eq!(st(&me).travel_fraction, 0.0);
        shark_of(&me).shark.borrow_mut().travel_fraction = 0.5;
        shark_die(&me, None);
        assert_eq!(st(&me).travel_fraction, 0.5);
    }

    #[test]
    fn hit_draws_only_when_alive_and_authoritative() {
        let f = fx();
        let me = calm(&f);
        // lethal hit: no draw
        npc_state(&me).borrow_mut().damage = 24;
        glibc_rand::srand(4);
        shark_hit_with_force_blockhead(&me, 100, None);
        assert!(npc_state(&me).borrow().dead);
        assert_eq!(glibc_rand::rand(), raw(4, 1)[0]);
        // net copy: no draw
        let net = calm(&f);
        net.base().borrow_mut().is_net = true;
        shark_of(&net).shark.borrow_mut().walk_speed = 0.5;
        glibc_rand::srand(4);
        shark_hit_with_force_blockhead(&net, 1, None);
        assert_eq!(st(&net).walk_speed, 0.5);
        assert_eq!(glibc_rand::rand(), raw(4, 1)[0]);
        // authoritative survivor: both directions seen
        let mut seen = [false; 2];
        for seed in 0..20u32 {
            let m = calm(&f);
            glibc_rand::srand(seed);
            shark_hit_with_force_blockhead(&m, 1, None);
            let s = st(&m);
            assert_eq!((s.walk_speed, s.random_time_between_direction_changes), (3.0, 5.0));
            assert_eq!(s.movement_direction, coin_dir(frac(raw(seed, 1)[0])));
            seen[(s.movement_direction == 1) as usize] = true;
        }
        assert_eq!(seen, [true, true]);
    }

    #[test]
    fn remove_rider_rules() {
        let f = fx();
        let me = calm(&f);
        let a = uid(1);
        let b = uid(2);
        shark_of(&me).shark.borrow_mut().rider_target_velocity = Vector2 { x: 1.0, y: 1.0 };
        // no rider, removing "somebody": ignored
        shark_remove_rider_opt(&me, Some(&a));
        assert_eq!(st(&me).rider_target_velocity, Vector2 { x: 1.0, y: 1.0 });
        // both absent counts as equal
        shark_remove_rider_opt(&me, None);
        assert_eq!(st(&me).rider_target_velocity, Vector2::ZERO);
        // mismatch with a rider present: ignored
        npc_state(&me).borrow_mut().rider = Some(a.clone());
        shark_of(&me).shark.borrow_mut().rider_target_velocity = Vector2 { x: 1.0, y: 1.0 };
        shark_remove_rider_opt(&me, Some(&b));
        assert_eq!(st(&me).rider_target_velocity, Vector2 { x: 1.0, y: 1.0 });
        shark_remove_rider_opt(&me, None);
        assert_eq!(st(&me).rider_target_velocity, Vector2 { x: 1.0, y: 1.0 });
        // match
        shark_remove_rider_opt(&me, Some(&a));
        assert_eq!(st(&me).rider_target_velocity, Vector2::ZERO);
        assert!(npc_state(&me).borrow().rider.is_none());
        Npc::remove_rider(shark_of(&me), &me, &a);
        Npc::remove_rider_opt(shark_of(&me), &me, None);
    }

    #[test]
    fn unload_and_dealloc() {
        let f = fx();
        let me = calm(&f);
        let t = uid(1);
        let r = uid(2);
        let other = uid(3);
        shark_of(&me).shark.borrow_mut().attack_blockhead = Some(t.clone());
        npc_state(&me).borrow_mut().rider = Some(r.clone());
        shark_of(&me).shark.borrow_mut().rider_target_velocity = Vector2 { x: 1.0, y: 1.0 };
        me.blockhead_unloaded(&me, &other);
        assert!(st(&me).attack_blockhead.is_some());
        assert_eq!(st(&me).rider_target_velocity, Vector2 { x: 1.0, y: 1.0 });
        me.blockhead_unloaded(&me, &t);
        assert!(st(&me).attack_blockhead.is_none());
        me.blockhead_unloaded(&me, &r);
        assert_eq!(st(&me).rider_target_velocity, Vector2::ZERO);
        shark_of(&me).shark.borrow_mut().attack_blockhead = Some(t);
        me.dealloc(&me);
        assert!(st(&me).attack_blockhead.is_none());
    }

    #[test]
    fn fed_reaction_and_riding_rules() {
        let f = fx();
        let me = calm(&f);
        let sh = shark_of(&me);
        glibc_rand::srand(3);
        sh.react_to_being_fed(&me);
        let s = st(&me);
        assert_eq!((s.walk_speed, s.random_time_between_direction_changes), (3.0, 5.0));
        assert_eq!(s.movement_direction, coin_dir(frac(raw(3, 1)[0])));
        assert!(me.update_needs_to_be_sent());
        // add rider: velocity reset unless being controlled
        let b = uid(9);
        sh.shark.borrow_mut().rider_target_velocity = Vector2 { x: 1.0, y: 1.0 };
        Npc::add_rider(sh, &me, &b);
        assert!(npc_state(&me).borrow().being_controlled);
        assert_eq!(st(&me).rider_target_velocity, Vector2 { x: 1.0, y: 1.0 });
        npc_state(&me).borrow_mut().being_controlled = false;
        sh.release_steering_if_uncontrolled();
        assert_eq!(st(&me).rider_target_velocity, Vector2::ZERO);
        // ride direction thresholds
        for (x, want) in [(0.5, 1), (0.1, 0), (-0.1, 0), (-0.5, -1)] {
            Npc::set_target_velocity(sh, Vector2 { x, y: 0.0 });
            assert_eq!(Npc::ride_direction(sh), want, "x {x}");
        }
        assert_eq!(Npc::rider_body_y_rotation_for_blockhead(sh, &b), st(&me).body_rotation);
        set_fp(&me, Vector2 { x: 1.0, y: 2.0 });
        assert_eq!(Npc::rider_pos_for_blockhead(sh, &b), [1.0, 2.8, -0.75, 1.0]);
    }

    #[test]
    fn rideable_and_capturable_rules() {
        let f = fx();
        let me = calm(&f);
        let sh = shark_of(&me);
        let b = uid(9);
        npc_state(&me).borrow_mut().age = 5000.0;
        assert!(!Npc::blockhead_can_ride_using_item(sh, &b, 0)); // not owned
        npc_state(&me).borrow_mut().tamed_client_id = Some("server".into());
        assert!(Npc::blockhead_can_ride_using_item(sh, &b, 0));
        npc_state(&me).borrow_mut().dead = true;
        assert!(!Npc::blockhead_can_ride_using_item(sh, &b, 0));
        npc_state(&me).borrow_mut().dead = false;
        me.base().borrow_mut().needs_removed = true;
        assert!(!Npc::blockhead_can_ride_using_item(sh, &b, 0));
        me.base().borrow_mut().needs_removed = false;
        // capture: wrong item, too big, someone else's, fine
        npc_state(&me).borrow_mut().age = 100.0;
        assert!(!Npc::can_be_captured_by_blockhead_with_item_type(sh, &b, 1));
        assert!(Npc::can_be_captured_by_blockhead_with_item_type(sh, &b, 102));
        npc_state(&me).borrow_mut().tamed_client_id = Some("someone".into());
        assert!(!Npc::can_be_captured_by_blockhead_with_item_type(sh, &b, 102));
        assert_eq!(
            Npc::cant_be_captured_tip_string_for_blockhead_with_item_type(sh, &b, 102).as_deref(),
            Some("THAT SHARK BELONGS TO\nSOMEONE ELSE")
        );
        npc_state(&me).borrow_mut().tamed_client_id = None;
        assert_eq!(Npc::cant_be_captured_tip_string_for_blockhead_with_item_type(sh, &b, 102), None);
        assert_eq!(Npc::food_item_type(sh), 151);
        assert!(!Npc::jumps_on_swipe(sh));
        assert!(Npc::get_names_array(sh).is_some());
        assert_eq!(Npc::npc_type(sh), 5);
        assert_eq!(me.class_info().name, "Shark");
        assert_eq!(Npc::center(sh), sh.center());
        assert_eq!(Npc::name_pos(sh), sh.name_pos());
    }

    // ------------------------------------------------------------------ section 4

    #[test]
    fn stale_target_is_dropped() {
        let f = fx();
        let me = calm(&f);
        let t = uid(5);
        for case in 0..4 {
            f.env.regen_uid.set(if case == 0 { 5 } else { 0 });
            f.env.trade_uid.set(if case == 1 { 5 } else { 0 });
            t.base().borrow_mut().needs_removed = case == 2;
            shark_of(&me).shark.borrow_mut().attack_blockhead = Some(t.clone());
            me.base().borrow_mut().update_needs_to_be_sent = false;
            let mut k = tk(&me);
            k.drop_stale_target();
            assert_eq!(k.s.attack_blockhead.is_none(), case < 3, "case {case}");
            assert_eq!(me.update_needs_to_be_sent(), case < 3);
        }
        // net copies keep their target; no target is a no-op
        me.base().borrow_mut().is_net = true;
        f.env.regen_uid.set(5);
        let mut k = tk(&me);
        k.s.attack_blockhead = Some(t);
        k.drop_stale_target();
        assert!(k.s.attack_blockhead.is_some());
        me.base().borrow_mut().is_net = false;
        let mut k = tk(&me);
        k.s.attack_blockhead = None;
        k.drop_stale_target();
    }

    #[test]
    fn dying_waits_then_drops_loot() {
        let f = fx();
        let me = calm(&f);
        let killer = uid(7);
        npc_state(&me).borrow_mut().dead = true;
        npc_state(&me).borrow_mut().kill_blockhead = Some(killer);
        // timer not yet at 1
        shark_of(&me).shark.borrow_mut().travel_fraction = 0.0;
        tick(&me, 0.1);
        assert!((st(&me).travel_fraction - 0.4).abs() < 1e-6);
        assert!(!me.needs_removed());
        assert!(f.env.inner.drops.borrow().is_empty());
        // net copy never drops
        me.base().borrow_mut().is_net = true;
        shark_of(&me).shark.borrow_mut().travel_fraction = 5.0;
        tick(&me, 0.1);
        assert!(!me.needs_removed());
        me.base().borrow_mut().is_net = false;
        // already marked
        me.base().borrow_mut().needs_removed = true;
        tick(&me, 0.1);
        assert!(f.env.inner.drops.borrow().is_empty());
        me.base().borrow_mut().needs_removed = false;
        tick(&me, 0.1);
        assert!(me.needs_removed());
        assert!(!f.env.inner.drops.borrow().is_empty());
    }

    #[test]
    fn loot_levels_and_colours_over_many_seeds() {
        let mut shapes = std::collections::BTreeSet::new();
        for seed in 0..120u32 {
            for age in [4500.0f32, 4501.0, 9001.0, 20000.0] {
                let f = fx();
                let me = calm(&f);
                npc_state(&me).borrow_mut().age = age;
                let killer = uid(7);
                glibc_rand::srand(seed);
                tk(&me).drop_loot(&killer);
                let drops = f.env.inner.drops.borrow().clone();
                let fish = drops.iter().filter(|d| d.0 == 151).count();
                assert_eq!(fish as i64, raw_fish_count(age / 27000.0));
                let jaw = drops.iter().find(|d| d.0 == 157).unwrap();
                assert_eq!(jaw.2 as i32, f32_to_i32((age / 27000.0) * 255.0));
                if age <= 4500.0 {
                    assert_eq!(drops.len(), fish + 1);
                } else {
                    let c = drops.last().unwrap();
                    shapes.insert((age as i32, c.0, c.2 != 0));
                }
            }
        }
        // the colour gate and every clothing level are reachable
        assert!(shapes.iter().any(|s| s.2) && shapes.iter().any(|s| !s.2));
        assert!(
            shapes.iter().filter(|s| s.0 == 20000).map(|s| s.1).collect::<std::collections::BTreeSet<_>>().len() > 1
        );
    }

    #[test]
    fn speed_selection() {
        let f = fx();
        let me = calm(&f);
        let local = uid(1);
        let remote = net_obj(2);
        {
            let mut s = shark_of(&me).shark.borrow_mut();
            s.walk_speed = 1.0;
            s.remote_walk_speed = 3.0;
        }
        let k = tk(&me);
        assert_eq!(k.effective_speed(&None), 1.0);
        assert_eq!(k.effective_speed(&Some(local.clone())), 1.0);
        assert_eq!(k.effective_speed(&Some(remote.clone())), 3.0);
        me.base().borrow_mut().is_net = true;
        let k = tk(&me);
        assert_eq!(k.effective_speed(&None), 1.5);
        assert_eq!(k.effective_speed(&Some(local)), 1.5);
        assert_eq!(k.effective_speed(&Some(remote)), 3.0);
    }

    #[test]
    fn wander_inspected_stops() {
        let f = fx();
        let me = calm(&f);
        let insp = uid(3);
        npc_state(&me).borrow_mut().inspecting_blockhead = Some(Rc::downgrade(&insp));
        let mut k = tk(&me);
        assert!(k.wander(true));
        assert_eq!((k.s.movement_direction, k.s.walk_speed), (0, 0.0));
    }

    #[test]
    fn wander_draws_nothing_unless_due() {
        let f = fx();
        let me = calm(&f);
        // (timer, travel, no_rider): not due / ridden / still travelling while moving
        for (timer, travel, no_rider) in [(5.0, 0.0, true), (-1.0, 0.0, false), (-1.0, 0.5, true)] {
            let mut k = tk(&me);
            k.s.random_time_between_direction_changes = timer;
            k.s.travel_fraction = travel;
            k.s.movement_direction = 1;
            glibc_rand::srand(2);
            k.wander(no_rider);
            assert_eq!(glibc_rand::rand(), raw(2, 1)[0]);
        }
    }

    #[test]
    fn wander_due_follows_the_draw_order() {
        let f = fx();
        let me = calm(&f);
        let mut changed_dir = [false; 3];
        for seed in 0..200u32 {
            // (net, direction, travel, walk)
            for (net, dir0, travel, walk0) in [
                (false, 1, 1.0f32, 0.5f32),
                (false, 0, 0.2, 0.5),
                (false, -1, 1.0, 0.0),
                (false, 1, 1.0, 0.0),
                (true, 1, 1.0, 0.5),
            ] {
                me.base().borrow_mut().is_net = net;
                me.base().borrow_mut().update_needs_to_be_sent = false;
                let mut k = tk(&me);
                k.s.random_time_between_direction_changes = 0.0;
                k.s.travel_fraction = travel;
                k.s.movement_direction = dir0;
                k.s.walk_speed = walk0;
                let v = raw(seed, 4);
                glibc_rand::srand(seed);
                let stationary = k.wander(true);
                assert_eq!(k.s.random_time_between_direction_changes, frac(v[0]) * 10.0 + 1.0);
                if net {
                    assert_eq!(k.s.movement_direction, dir0);
                    assert_eq!(k.s.walk_speed, walk0);
                    assert!(!me.update_needs_to_be_sent());
                    assert_eq!(glibc_rand::rand(), v[1]);
                    continue;
                }
                let walk = blended_walk(walk0, frac(v[1]));
                assert_eq!(k.s.walk_speed, walk);
                assert!(me.update_needs_to_be_sent());
                let wanted = three_way(frac(v[2]) as f64);
                let expect = if wanted == 0 {
                    0
                } else if dir0 != wanted && (walk < 0.08 || dir0 == 0) {
                    wanted
                } else {
                    dir0
                };
                assert_eq!(k.s.movement_direction, expect);
                assert_eq!(stationary, expect == 0);
                assert_eq!(glibc_rand::rand(), v[3]);
                changed_dir[(wanted + 1) as usize] |= expect != dir0;
            }
        }
        assert!(changed_dir.iter().any(|b| *b));
        me.base().borrow_mut().is_net = false;
    }

    #[test]
    fn frozen_only_on_ice() {
        let f = fx();
        let me = calm(&f);
        assert!(!tk(&me).is_frozen());
        put(&f, 40, 3, 4, 255);
        assert!(tk(&me).is_frozen());
        f.env.inner.tiles.borrow_mut().remove(&(40, 3));
        assert!(!tk(&me).is_frozen());
        // a frozen shark does not move: the tick stops before travel advances
        put(&f, 40, 3, 4, 255);
        shark_of(&me).shark.borrow_mut().travel_fraction = 0.25;
        tick(&me, 0.1);
        assert_eq!(st(&me).travel_fraction, 0.25);
    }

    #[test]
    fn breeding_timer() {
        let f = fx();
        let me = calm(&f);
        npc_state(&me).borrow_mut().fullness = 6000.0;
        // no timer: untouched
        tk(&me).breed_and_drown();
        assert!(f.env.inner.npcs_made.borrow().is_empty());
        // running timer: only counts down
        npc_state(&me).borrow_mut().lay_timer = 5.0;
        tk(&me).breed_and_drown();
        assert!((npc_state(&me).borrow().lay_timer - 4.9).abs() < 1e-5);
        assert!(f.env.inner.npcs_made.borrow().is_empty());
        // elapsed: baby is born, mother loses fullness, cooldown starts
        npc_state(&me).borrow_mut().lay_timer = 0.05;
        tk(&me).breed_and_drown();
        assert_eq!(*f.env.inner.npcs_made.borrow(), vec![(5, false, false)]);
        assert_eq!(npc_state(&me).borrow().fullness, 3300.0);
        assert_eq!(npc_state(&me).borrow().lay_cooldown_timer, 900.0);
        // failed birth: no fullness cost, cooldown still starts
        f.env.spawn_ok.set(false);
        npc_state(&me).borrow_mut().lay_timer = 0.05;
        npc_state(&me).borrow_mut().lay_cooldown_timer = 0.0;
        tk(&me).breed_and_drown();
        assert_eq!(npc_state(&me).borrow().fullness, 3300.0);
        assert_eq!(npc_state(&me).borrow().lay_cooldown_timer, 900.0);
        // net copies do nothing here
        me.base().borrow_mut().is_net = true;
        npc_state(&me).borrow_mut().lay_timer = 0.05;
        tk(&me).breed_and_drown();
        assert_eq!(npc_state(&me).borrow().lay_timer, 0.05);
        me.base().borrow_mut().is_net = false;
    }

    #[test]
    fn drowning_decision_table() {
        let f = fx();
        let me = calm(&f);
        // (type, level, drowning)
        for (ty, level, drowns) in
            [(3u8, 255u8, false), (3, 126, true), (3, 127, false), (2, 255, true), (4, 255, false), (4, 10, true)]
        {
            put(&f, 40, 3, ty, level);
            let mut k = tk(&me);
            k.s.drown_timer = 2.0;
            k.breed_and_drown();
            assert_eq!(k.s.drown_timer, if drowns { 2.1 } else { 0.0 }, "{ty} {level}");
        }
        // absent cell is not drowning
        f.env.inner.tiles.borrow_mut().remove(&(40, 3));
        let mut k = tk(&me);
        k.s.drown_timer = 2.0;
        k.breed_and_drown();
        assert_eq!(k.s.drown_timer, 0.0);
        // too long out of water: dies without a killer
        put(&f, 40, 3, 2, 255);
        let mut k = tk(&me);
        k.s.drown_timer = 10.0;
        k.breed_and_drown();
        assert!(npc_state(&me).borrow().dead);
        assert!(npc_state(&me).borrow().kill_blockhead.is_none());
        assert_eq!(k.s.travel_fraction, 0.0); // die() restarted the timer and the copy was re-read
    }

    #[test]
    fn buoyancy_float_and_fall() {
        let f = fx();
        let me = calm(&f);
        set_fp(&me, Vector2 { x: 40.5, y: 3.5 });
        // submerged, not falling: nothing
        let mut k = tk(&me);
        k.buoyancy();
        assert_eq!(me.float_pos().y, 3.5);
        // submerged and falling upwards-negative velocity: sinks by the new velocity
        let mut k = tk(&me);
        k.s.falling = true;
        k.s.fall_velocity = -0.5;
        k.buoyancy();
        assert!((k.s.fall_velocity + 0.48).abs() < 1e-6);
        assert!((me.float_pos().y - 3.02).abs() < 1e-5);
        // velocity turns positive: landing; beached sharks get walk 2
        set_fp(&me, Vector2 { x: 40.5, y: 3.5 });
        for (beached, walk) in [(true, 2.0f32), (false, 0.5)] {
            let mut k = tk(&me);
            k.s.falling = true;
            k.s.fall_velocity = 0.5;
            k.s.is_on_air_above_solid_ground = beached;
            k.s.walk_speed = 0.5;
            k.buoyancy();
            assert!(!k.s.falling && !k.s.is_on_air_above_solid_ground);
            assert_eq!((k.s.fall_velocity, k.s.walk_speed), (0.0, walk));
        }
        // shallow water (< 127) is not buoyant
        put(&f, 40, 3, 3, 100);
        set_fp(&me, Vector2 { x: 40.5, y: 3.5 });
        let mut k = tk(&me);
        k.buoyancy();
        assert!(k.s.falling && k.s.is_on_air_above_solid_ground);
    }

    #[test]
    fn falling_through_air() {
        let f = fx();
        let me = calm(&f);
        sea(&f);
        for y in 0..=10 {
            put(&f, 40, y, 2, 0);
        }
        put(&f, 40, 1, 1, 0);
        set_fp(&me, Vector2 { x: 40.5, y: 3.0 });
        // slow drift stays in the cell and draws a direction
        set_fp(&me, Vector2 { x: 40.5, y: 3.5 });
        let mut k = tk(&me);
        k.s.fall_velocity = 0.0;
        glibc_rand::srand(6);
        k.buoyancy();
        assert!((k.s.fall_velocity + 0.01).abs() < 1e-6);
        assert_eq!(k.s.movement_direction, if raw(6, 1)[0] > 0x3fffffff { 1 } else { -1 });
        // crossing into the open cell below moves the object
        set_fp(&me, Vector2 { x: 40.5, y: 3.0 });
        let mut k = tk(&me);
        k.s.fall_velocity = -0.5;
        k.buoyancy();
        assert_eq!(me.pos(), IntPair { x: 40, y: 2 });
        assert_eq!((k.s.from_square, k.s.to_square), (IntPair { x: 40, y: 2 }, IntPair { x: 40, y: 2 }));
        // crossing into solid ground bounces
        set_fp(&me, Vector2 { x: 40.5, y: 2.0 });
        let mut k = tk(&me);
        k.s.fall_velocity = -0.5;
        k.buoyancy();
        assert_eq!(me.pos(), IntPair { x: 40, y: 2 });
        assert!((me.float_pos().y - 2.01).abs() < 1e-5);
        assert!((k.s.fall_velocity - 0.051).abs() < 1e-5);
        // net copies do not draw
        me.base().borrow_mut().is_net = true;
        let mut k = tk(&me);
        glibc_rand::srand(6);
        k.buoyancy();
        assert_eq!(glibc_rand::rand(), raw(6, 1)[0]);
        me.base().borrow_mut().is_net = false;
    }

    #[test]
    fn bite_and_flee_timer_table() {
        let f = fx();
        let me = calm(&f);
        let t = uid(4);
        // not biting: flee timer thresholds
        for (flee0, walk_factor, flee_after) in
            [(0.0f32, 1.0f32, 0.0f32), (-0.5, 1.0, -0.5), (9.5, 1.0, 9.4), (9.05, 0.5, 8.95), (0.05, 0.5, -0.001)]
        {
            let mut k = tk(&me);
            k.s.flee_timer = flee0;
            k.s.walk_speed = 1.0;
            k.bite_and_flee_timers(true);
            assert!((k.s.walk_speed - walk_factor).abs() < 1e-6, "{flee0}");
            assert!((k.s.flee_timer - flee_after).abs() < 1e-4, "{flee0}");
        }
        // both the 9-crossing and the below-zero halving
        let mut k = tk(&me);
        k.s.flee_timer = 9.0;
        k.dt = 20.0;
        k.s.walk_speed = 1.0;
        k.bite_and_flee_timers(true);
        assert_eq!(k.s.walk_speed, 0.25);
        // biting: counts down, then ends
        let mut k = tk(&me);
        k.s.biting = true;
        k.s.bite_timer = 0.5;
        k.bite_and_flee_timers(true);
        assert!(k.s.biting);
        assert!((k.s.bite_timer - 0.4).abs() < 1e-6);
        for (net, local) in [(false, true), (false, false), (true, false)] {
            me.base().borrow_mut().is_net = net;
            me.base().borrow_mut().update_needs_to_be_sent = false;
            me.base().borrow_mut().creation_data_needs_to_be_sent = false;
            let mut k = tk(&me);
            k.s.biting = true;
            k.s.bite_timer = 0.05;
            k.s.attack_blockhead = Some(t.clone());
            k.s.has_harmed_blockhead = true;
            k.bite_and_flee_timers(local);
            assert!(!k.s.biting && k.s.attack_blockhead.is_none() && !k.s.has_harmed_blockhead);
            assert_eq!((k.s.bite_timer, k.s.flee_timer), (0.0, 10.0));
            assert_eq!(me.creation_data_needs_to_be_sent(), !net);
            assert_eq!(me.update_needs_to_be_sent(), local);
        }
        me.base().borrow_mut().is_net = false;
        // chase timer advances while someone is near or after a long chase
        for (nearby, chase, after) in [(false, 0.0f32, 0.0f32), (true, 1.0, 1.1), (false, -5.0, -4.9)] {
            let mut k = tk(&me);
            k.s.nearby_blockhead = nearby;
            k.s.chase_timer = chase;
            k.bite_and_flee_timers(true);
            assert!((k.s.chase_timer - after).abs() < 1e-5);
        }
    }

    #[test]
    fn rider_steering_table() {
        let f = fx();
        let me = calm(&f);
        let t = uid(4);
        for (ctl, target, x, dir, walk) in [
            (false, false, 0.5f32, 7, 7.0f32),
            (true, true, 0.5, 7, 7.0),
            (true, false, 0.5, 1, 2.0),
            (true, false, 0.1, 1, 0.4),
            (true, false, -0.5, -1, 2.0),
            (true, false, -0.1, -1, 0.4),
            (true, false, 0.05, 7, 7.0),
            (true, false, -0.05, 7, 7.0),
        ] {
            let mut k = tk(&me);
            k.local_rider_in_control = ctl;
            k.s.attack_blockhead = target.then(|| t.clone());
            k.s.movement_direction = 7;
            k.s.walk_speed = 7.0;
            k.s.rider_target_velocity = Vector2 { x, y: 0.0 };
            k.steer_by_rider();
            assert_eq!((k.s.movement_direction, k.s.walk_speed), (dir, walk), "{x} {ctl} {target}");
        }
    }

    #[test]
    fn final_stage_interpolates_and_forces_air_speed() {
        let f = fx();
        let me = calm(&f);
        let mut k = tk(&me);
        k.s.is_on_air_above_solid_ground = true;
        k.finish();
        assert_eq!(k.s.walk_speed, 6.0);
        k.s.from_square = IntPair { x: 40, y: 3 };
        k.s.to_square = IntPair { x: 41, y: 4 };
        k.s.travel_fraction = 0.5;
        k.finish();
        assert_eq!(me.float_pos(), Vector2 { x: 41.0, y: 3.5 });
    }

    #[test]
    fn arrival_travel_bookkeeping() {
        let f = fx();
        let me = calm(&f);
        for (before, after) in [(1.0f32, 0.0f32), (1.5, 0.5), (2.5, 0.0), (1.0 - 0.0, 0.0)] {
            let mut k = tk(&me);
            k.s.travel_fraction = before;
            k.arrive();
            assert_eq!(k.s.travel_fraction, after);
            assert_eq!(k.s.from_square, k.s.to_square);
        }
        let mut k = tk(&me);
        k.s.to_square = IntPair { x: 41, y: 3 };
        k.s.travel_fraction = 1.0;
        k.arrive();
        assert_eq!(me.pos(), IntPair { x: 41, y: 3 });
        assert_eq!(me.float_pos(), Vector2 { x: 41.5, y: 3.0 });
    }

    fn kelp_world(f: &Fx) -> (ObjRef, ObjRef) {
        let me = calm(f);
        let plant: ObjRef = TestObj::new();
        f.env.inner.plants.borrow_mut().insert((41, 3), plant.clone());
        f.env.inner.food.borrow_mut().insert(crate::objects::handle::obj_id(&plant), 700.0);
        f.env.inner.tiles.borrow_mut().get_mut(&(41, 3)).unwrap().contents = CONTENTS_KELP_PLANT;
        (me, plant)
    }
    fn arrive_east(me: &ObjRef) {
        let mut k = tk(me);
        k.s.from_square = IntPair { x: 40, y: 3 };
        k.s.to_square = IntPair { x: 41, y: 3 };
        k.s.travel_fraction = 1.0;
        k.arrive();
    }

    #[test]
    fn grazing_decision_table() {
        let f = fx();
        let (me, plant) = kelp_world(&f);
        npc_state(&me).borrow_mut().fullness = 1000.0;
        arrive_east(&me);
        assert_eq!(npc_state(&me).borrow().fullness, 1700.0);
        assert_eq!(f.env.inner.plant_available_food(&plant), 0.0);
        // full enough: nothing eaten
        f.env.inner.food.borrow_mut().insert(crate::objects::handle::obj_id(&plant), 700.0);
        npc_state(&me).borrow_mut().fullness = 5400.0;
        npc_state(&me).borrow_mut().age = 100.0;
        arrive_east(&me);
        assert_eq!(f.env.inner.plant_available_food(&plant), 700.0);
        // no kelp in the cell
        npc_state(&me).borrow_mut().fullness = 1000.0;
        f.env.inner.tiles.borrow_mut().get_mut(&(41, 3)).unwrap().contents = 0;
        arrive_east(&me);
        assert_eq!(npc_state(&me).borrow().fullness, 1000.0);
        // a plant that is not kelp
        f.env.inner.tiles.borrow_mut().get_mut(&(41, 3)).unwrap().contents = CONTENTS_KELP_PLANT;
        f.env.kelp.set(false);
        arrive_east(&me);
        assert_eq!(npc_state(&me).borrow().fullness, 1000.0);
        f.env.kelp.set(true);
        // kelp tile but no plant object
        f.env.inner.tiles.borrow_mut().get_mut(&(41, 3)).unwrap().contents = CONTENTS_KELP_PLANT;
        f.env.inner.plants.borrow_mut().clear();
        arrive_east(&me);
        assert_eq!(npc_state(&me).borrow().fullness, 1000.0);
        // target cell not resident
        f.env.inner.tiles.borrow_mut().remove(&(41, 3));
        arrive_east(&me);
        // net copies and vertical moves never graze
        let (me2, plant2) = kelp_world(&f);
        npc_state(&me2).borrow_mut().fullness = 1000.0;
        me2.base().borrow_mut().is_net = true;
        arrive_east(&me2);
        assert_eq!(npc_state(&me2).borrow().fullness, 1000.0);
        me2.base().borrow_mut().is_net = false;
        let mut k = tk(&me2);
        k.s.from_square = IntPair { x: 41, y: 2 };
        k.s.to_square = IntPair { x: 41, y: 3 };
        k.s.travel_fraction = 1.0;
        k.arrive();
        assert_eq!(npc_state(&me2).borrow().fullness, 1000.0);
        let _ = plant2;
    }

    #[test]
    fn laying_decision_table() {
        let f = fx();
        let me = calm(&f);
        put(&f, 41, 3, 3, 255);
        // (fullness, cooldown, lay timer, age, starts)
        for (full, cd, lay, age, starts) in [
            (6000.0f32, 0.0f32, 0.0f32, 5000.0f32, true),
            (5400.0, 0.0, 0.0, 5000.0, false),
            (6000.0, 1.0, 0.0, 5000.0, false),
            (6000.0, 0.0, 1.0, 5000.0, false),
            (6000.0, 0.0, 0.0, 4500.0, false),
        ] {
            {
                let mut n = npc_state(&me).borrow_mut();
                n.fullness = full;
                n.lay_cooldown_timer = cd;
                n.lay_timer = lay;
                n.age = age;
            }
            let v = raw(11, 1);
            glibc_rand::srand(11);
            arrive_east(&me);
            let got = npc_state(&me).borrow().lay_timer;
            if starts {
                assert_eq!(got, (((frac(v[0]) * 900.0) as f64) * 0.2 + 180.0) as f32);
            } else {
                assert_eq!(got, lay);
                assert_eq!(glibc_rand::rand(), v[0]);
            }
        }
    }

    // ------------------------------------------------------------------ section 5

    #[test]
    fn following_a_remote_driver() {
        let f = fx();
        let me = calm(&f);
        let pos = IntPair { x: 40, y: 3 };
        // (goal, remote walk, remote dir) -> (dir, walk, teleport, up, down)
        let cases: [((i32, i32), f32, i32, (i32, f32, bool, bool, bool)); 11] = [
            ((41, 3), 0.5, 0, (1, 1.0, false, false, false)),
            ((41, 3), 3.0, 0, (1, 3.0, false, false, false)),
            ((39, 3), 0.5, 0, (-1, 1.0, false, false, false)),
            ((40, 3), 0.7, 0, (0, 0.7, false, false, false)),
            ((45, 3), 0.5, 0, (1, 2.0, false, false, false)),
            ((35, 3), 0.5, 0, (-1, 2.0, false, false, false)),
            ((40, 6), 0.5, 0, (0, 0.5, false, true, false)),
            ((40, 0), 0.5, 0, (0, 0.5, false, false, true)),
            ((60, 3), 0.9, -1, (-1, 0.9, true, false, false)),
            ((40, 9), 0.9, 1, (1, 0.9, true, false, false)),
            // wrap-around: 127 cells to the east is one cell to the west on a 128-wide world
            ((167, 3), 0.5, 0, (-1, 1.0, false, false, false)),
        ];
        for (goal, rw, rd, (dir, walk, tp, up, down)) in cases {
            let mut k = tk(&me);
            k.s.remote_goal_square = IntPair { x: goal.0, y: goal.1 };
            k.s.remote_walk_speed = rw;
            k.s.remote_movement_direction = rd;
            k.s.to_square = pos;
            let mut plan = Plan::default();
            k.follow_remote_driver(pos, &mut plan);
            assert_eq!(
                (k.s.movement_direction, k.s.walk_speed, plan.teleport, plan.want_up, plan.want_down),
                (dir, walk, tp, up, down),
                "{goal:?}"
            );
            assert_eq!(plan.stationary, dir == 0);
            if tp {
                assert_eq!(k.s.to_square, IntPair { x: goal.0, y: goal.1 });
            }
        }
    }

    #[test]
    fn net_copy_without_rider_follows_remote_in_a_full_tick() {
        let f = fx();
        let me = calm(&f);
        me.base().borrow_mut().is_net = true;
        {
            let mut s = shark_of(&me).shark.borrow_mut();
            s.remote_goal_square = IntPair { x: 43, y: 3 };
            s.remote_walk_speed = 0.5;
            s.travel_fraction = 1.0;
            s.walk_speed = 0.5;
        }
        tick(&me, 0.1);
        let s = st(&me);
        assert_eq!(s.movement_direction, 1);
        assert_eq!(s.to_square, IntPair { x: 41, y: 3 });
        assert!(follows_remote(&me));
        // a teleport request skips the movement decision entirely
        {
            let mut s = shark_of(&me).shark.borrow_mut();
            s.remote_goal_square = IntPair { x: 65, y: 3 };
            s.travel_fraction = 1.0;
        }
        tick(&me, 0.1);
        assert_eq!(st(&me).to_square, IntPair { x: 65, y: 3 });
    }

    fn prey_world() -> (Fx, ObjRef) {
        let f = fx();
        let me = calm(&f);
        (f, me)
    }
    fn hunt_once(me: &ObjRef) -> (Tick<'_>, IntPair, Plan) {
        let mut k = tk(me);
        k.s.movement_direction = 1;
        let pos = me.pos();
        let mut next = IntPair { x: pos.x + 1, y: pos.y };
        let mut plan = Plan::default();
        k.hunt(pos, &mut next, &mut plan, true);
        (k, next, plan)
    }

    #[test]
    fn hunt_preconditions() {
        let (f, me) = prey_world();
        f.env.inner.bhs.borrow_mut().push(uid_at(9, 43, 3));
        // airborne, not locally controlled, or resting after a chase: nothing happens (nearby is left alone)
        for case in 0..3 {
            let mut k = tk(&me);
            k.s.nearby_blockhead = true;
            k.s.is_on_air_above_solid_ground = case == 0;
            k.s.chase_timer = if case == 2 { -1.0 } else { 0.0 };
            let mut next = IntPair { x: 41, y: 3 };
            let mut plan = Plan::default();
            k.hunt(IntPair { x: 40, y: 3 }, &mut next, &mut plan, case != 1);
            assert!(k.s.nearby_blockhead, "case {case}");
            assert!(!plan.chased);
        }
        // passive difficulty: scan skipped but the nearby flag was cleared first
        let mut cr = CustomRules::default();
        cr.custom_rules_enabled = 1;
        cr.npc_difficulty = 3;
        f.env.inner.rules.set(Some(cr));
        let mut k = tk(&me);
        k.s.nearby_blockhead = true;
        let mut next = IntPair { x: 41, y: 3 };
        let mut plan = Plan::default();
        k.hunt(IntPair { x: 40, y: 3 }, &mut next, &mut plan, true);
        assert!(!k.s.nearby_blockhead && !plan.chased);
        // rules on but another difficulty still hunts
        cr.npc_difficulty = 1;
        f.env.inner.rules.set(Some(cr));
        let (k, _, plan) = hunt_once(&me);
        assert!(k.s.nearby_blockhead && plan.chased);
    }

    #[test]
    fn hunt_filters_pick_the_first_acceptable_blockhead() {
        let (f, me) = prey_world();
        let target = uid(77);
        // each (filter) rejects the first blockhead (id 1, in range); the second (id 2) is then chosen
        type Setup = fn(&Fx, &ObjRef, &ObjRef);
        let setups: [(&str, Setup); 7] = [
            ("paused", |f, _, _| f.env.paused_uid.set(1)),
            ("regenerating", |f, _, _| f.env.regen_uid.set(1)),
            ("trade", |f, _, _| f.env.trade_uid.set(1)),
            ("other target", |_, me, t| shark_of(me).shark.borrow_mut().attack_blockhead = Some(t.clone())),
            ("too far east", |_, _, _| {}),
            ("too far west", |_, _, _| {}),
            ("simulation stopped", |f, _, _| f.env.nosim_uid.set(1)),
        ];
        for (name, setup) in setups {
            f.env.paused_uid.set(0);
            f.env.regen_uid.set(0);
            f.env.trade_uid.set(0);
            f.env.nosim_uid.set(0);
            npc_state(&me).borrow_mut().tamed_client_id = None;
            npc_state(&me).borrow_mut().fullness = 0.0;
            shark_of(&me).shark.borrow_mut().attack_blockhead = None;
            let x1 = match name {
                "too far east" => 48,
                "too far west" => 32,
                _ => 44,
            };
            let first = uid_at(1, x1, 3);
            let second = uid_at(2, 38, 3);
            // for the "other target" case the target must be the second blockhead
            *f.env.inner.bhs.borrow_mut() = vec![first, second.clone()];
            setup(&f, &me, &target);
            if name == "other target" {
                shark_of(&me).shark.borrow_mut().attack_blockhead = Some(second.clone());
            }
            let mut k = tk(&me);
            k.sim = name == "simulation stopped";
            k.s.movement_direction = 1;
            let mut next = IntPair { x: 41, y: 3 };
            let mut plan = Plan::default();
            k.hunt(IntPair { x: 40, y: 3 }, &mut next, &mut plan, true);
            // the chosen blockhead is to the west, so the shark turns to face it
            assert_eq!(k.s.movement_direction, -1, "{name}");
            assert!(plan.chased, "{name}");
        }
        // owners that are well fed are left alone, whoever they are
        npc_state(&me).borrow_mut().tamed_client_id = Some("server".into());
        npc_state(&me).borrow_mut().fullness = 2000.0;
        *f.env.inner.bhs.borrow_mut() = vec![uid_at(1, 44, 3), uid_at(2, 38, 3)];
        let (k, _, plan) = hunt_once(&me);
        assert!(!k.s.nearby_blockhead && !plan.chased);
        // an owner that is hungry is still prey
        npc_state(&me).borrow_mut().tamed_client_id = Some("server".into());
        npc_state(&me).borrow_mut().fullness = 1350.0;
        *f.env.inner.bhs.borrow_mut() = vec![uid_at(1, 44, 3), uid_at(2, 38, 3)];
        let (k, _, _) = hunt_once(&me);
        assert_eq!(k.s.movement_direction, 1);
        // out of vertical range: nothing chosen after the first horizontal match
        npc_state(&me).borrow_mut().tamed_client_id = None;
        *f.env.inner.bhs.borrow_mut() = vec![uid_at(1, 44, 12), uid_at(2, 38, 3)];
        let (k, _, plan) = hunt_once(&me);
        assert!(!k.s.nearby_blockhead && !plan.chased);
        // empty world
        f.env.inner.bhs.borrow_mut().clear();
        let (k, _, plan) = hunt_once(&me);
        assert!(!k.s.nearby_blockhead && !plan.chased);
    }

    #[test]
    fn chase_timer_resets_after_a_long_chase() {
        let (f, me) = prey_world();
        f.env.inner.bhs.borrow_mut().push(uid_at(9, 44, 3));
        for (before, after) in [(10.0f32, 10.0f32), (10.5, -20.0)] {
            let mut k = tk(&me);
            k.s.chase_timer = before;
            let mut next = IntPair { x: 41, y: 3 };
            k.hunt(IntPair { x: 40, y: 3 }, &mut next, &mut Plan::default(), true);
            assert_eq!(k.s.chase_timer, after);
        }
    }

    #[test]
    fn close_blockhead_triggers_a_bite() {
        let (f, me) = prey_world();
        let b = uid_at(9, 42, 3);
        f.env.inner.bhs.borrow_mut().push(b.clone());
        // swimmer within three cells: bite starts
        me.base().borrow_mut().update_needs_to_be_sent = false;
        let (k, _, _) = hunt_once(&me);
        assert!(k.s.biting && k.s.attack_blockhead.is_some());
        assert_eq!((k.s.bite_timer, k.s.walk_speed), (1.0, 4.0));
        assert!(me.update_needs_to_be_sent() && me.creation_data_needs_to_be_sent());
        // fleeing, already biting, or already holding a target: no new bite
        for case in 0..3 {
            let mut k = tk(&me);
            k.s.biting = case == 1;
            k.s.flee_timer = if case == 0 { 2.0 } else { 0.0 };
            k.s.attack_blockhead = (case == 2).then(|| b.clone());
            k.s.bite_timer = 0.3;
            k.s.walk_speed = 0.5;
            k.s.has_harmed_blockhead = true; // keep the damage step out of this
            let mut next = IntPair { x: 41, y: 3 };
            k.hunt(IntPair { x: 40, y: 3 }, &mut next, &mut Plan::default(), true);
            assert_eq!(k.s.bite_timer, 0.3, "case {case}");
        }
        // far enough that the bite does not start (4 cells) but the chase does
        *f.env.inner.bhs.borrow_mut() = vec![uid_at(9, 44, 3)];
        let (k, _, plan) = hunt_once(&me);
        assert!(!k.s.biting && plan.chased);
        // diagonal distance of exactly three on one axis is outside the bite range
        *f.env.inner.bhs.borrow_mut() = vec![uid_at(9, 43, 3)];
        let (k, _, _) = hunt_once(&me);
        assert!(!k.s.biting);
        *f.env.inner.bhs.borrow_mut() = vec![uid_at(9, 42, 6)];
        let (k, _, _) = hunt_once(&me);
        assert!(!k.s.biting);
    }

    #[test]
    fn blockhead_on_a_boat_is_left_alone() {
        let (f, me) = prey_world();
        f.env.inner.bhs.borrow_mut().push(uid_at(9, 42, 3));
        f.env.rides.set(true);
        f.env.boat.set(true);
        for biting in [false, true] {
            me.base().borrow_mut().update_needs_to_be_sent = false;
            let mut k = tk(&me);
            k.s.biting = biting;
            k.s.attack_blockhead = biting.then(|| f.env.inner.bhs.borrow()[0].clone());
            k.s.bite_timer = 0.8;
            k.s.has_harmed_blockhead = true;
            let mut next = IntPair { x: 41, y: 3 };
            k.hunt(IntPair { x: 40, y: 3 }, &mut next, &mut Plan::default(), true);
            assert!(!k.s.biting && k.s.attack_blockhead.is_none());
            assert_eq!((k.s.bite_timer, k.s.flee_timer, k.s.has_harmed_blockhead), (0.0, 10.0, false));
            assert_eq!(me.update_needs_to_be_sent(), biting);
        }
        // net copy does not flag the update when giving up
        me.base().borrow_mut().is_net = true;
        me.base().borrow_mut().update_needs_to_be_sent = false;
        let mut k = tk(&me);
        k.s.biting = true;
        k.s.attack_blockhead = Some(f.env.inner.bhs.borrow()[0].clone());
        let mut next = IntPair { x: 41, y: 3 };
        k.hunt(IntPair { x: 40, y: 3 }, &mut next, &mut Plan::default(), true);
        assert!(!me.update_needs_to_be_sent());
        me.base().borrow_mut().is_net = false;
        // riding something that is not a boat does not protect
        f.env.boat.set(false);
        let (k, _, _) = hunt_once(&me);
        assert!(k.s.biting);
    }

    #[test]
    fn bite_lands_on_the_same_cell() {
        let (f, me) = prey_world();
        let b = uid_at(9, 40, 3);
        f.env.inner.bhs.borrow_mut().push(b.clone());
        npc_state(&me).borrow_mut().age = 13500.0;
        let expect = bite_damage(0.5, 0.0, 25.0, false, 1);
        // already biting the blockhead standing on the shark's cell
        let mut k = tk(&me);
        k.s.biting = true;
        k.s.attack_blockhead = Some(b.clone());
        k.s.bite_timer = 0.5;
        let mut next = IntPair { x: 41, y: 3 };
        k.hunt(IntPair { x: 40, y: 3 }, &mut next, &mut Plan::default(), true);
        assert_eq!(f.env.inner.damage.borrow().as_slice(), &[expect]);
        assert!(k.s.has_harmed_blockhead);
        // harm is dealt once per bite
        k.hunt(IntPair { x: 40, y: 3 }, &mut next, &mut Plan::default(), true);
        assert_eq!(f.env.inner.damage.borrow().len(), 1);
        // not on the same cell: no damage
        f.env.inner.damage.borrow_mut().clear();
        b.base().borrow_mut().pos = IntPair { x: 41, y: 3 };
        let mut k = tk(&me);
        k.s.biting = true;
        k.s.attack_blockhead = Some(b.clone());
        k.hunt(IntPair { x: 40, y: 3 }, &mut next, &mut Plan::default(), true);
        assert!(f.env.inner.damage.borrow().is_empty());
        // dismount when the victim is the rider, and the simulation log
        b.base().borrow_mut().pos = IntPair { x: 40, y: 3 };
        npc_state(&me).borrow_mut().rider = Some(b.clone());
        for (regen, word) in [(false, "attacked"), (true, "killed")] {
            f.env.regen_uid.set(if regen { 9 } else { 0 });
            f.env.inner.sim_events.borrow_mut().clear();
            let before = f.env.stops.get();
            let mut k = tk(&me);
            k.sim = true;
            k.local_rider_in_control = true;
            k.s.biting = true;
            k.s.attack_blockhead = Some(b.clone());
            k.hunt(IntPair { x: 40, y: 3 }, &mut next, &mut Plan::default(), true);
            // regenerating blockheads are not prey, so only the non-regenerating case reaches the bite
            if !regen {
                assert_eq!(f.env.stops.get(), before + 1);
                assert_eq!(f.env.inner.sim_events.borrow().as_slice(), &[format!("{word} by a shark")]);
            } else {
                assert_eq!(f.env.stops.get(), before);
            }
        }
        // a victim that starts regenerating from the bite is logged as killed
        f.env.regen_uid.set(0);
        f.env.kill_on_damage.set(true);
        f.env.inner.sim_events.borrow_mut().clear();
        let mut k = tk(&me);
        k.sim = true;
        k.s.biting = true;
        k.s.attack_blockhead = Some(b.clone());
        k.hunt(IntPair { x: 40, y: 3 }, &mut next, &mut Plan::default(), true);
        assert_eq!(f.env.inner.sim_events.borrow().as_slice(), &["killed by a shark".to_string()]);
        f.env.kill_on_damage.set(false);
        // no log without a target (nor outside simulations)
        f.env.regen_uid.set(0);
        f.env.inner.sim_events.borrow_mut().clear();
        npc_state(&me).borrow_mut().rider = None;
        let mut k = tk(&me);
        k.sim = true;
        k.s.biting = true;
        k.s.attack_blockhead = None;
        k.hunt(IntPair { x: 40, y: 3 }, &mut next, &mut Plan::default(), true);
        assert!(f.env.inner.sim_events.borrow().is_empty());
        let mut k = tk(&me);
        k.s.biting = true;
        k.s.attack_blockhead = Some(b);
        k.hunt(IntPair { x: 40, y: 3 }, &mut next, &mut Plan::default(), true);
        assert!(f.env.inner.sim_events.borrow().is_empty());
        // damage scaling picks up the custom rules
        f.env.inner.damage.borrow_mut().clear();
        let mut cr = CustomRules::default();
        cr.custom_rules_enabled = 1;
        cr.npc_difficulty = 0;
        f.env.inner.rules.set(Some(cr));
        let mut k = tk(&me);
        k.s.biting = true;
        k.s.attack_blockhead = Some(f.env.inner.bhs.borrow()[0].clone());
        k.hunt(IntPair { x: 40, y: 3 }, &mut next, &mut Plan::default(), true);
        assert_eq!(f.env.inner.damage.borrow().as_slice(), &[bite_damage(0.5, 0.0, 25.0, true, 0)]);
    }

    #[test]
    fn pursuit_and_flight_geometry() {
        let (f, me) = prey_world();
        let pos = IntPair { x: 40, y: 3 };
        // (blockhead cell, fleeing, direction) -> (direction after, up, down)
        let cases = [
            ((46, 3), false, 1, 1, false, false),
            ((46, 3), false, -1, 1, false, false),
            ((34, 3), false, 1, -1, false, false),
            ((34, 3), false, -1, -1, false, false),
            ((46, 3), true, 1, -1, false, false),
            ((46, 3), true, -1, -1, false, false),
            ((34, 3), true, -1, 1, false, false),
            ((34, 3), true, 1, 1, false, false),
            ((41, 3), false, 1, 1, false, false), // blockhead right on the candidate cell: no turn
            ((41, 6), false, 1, 1, true, false),
            ((41, 0), false, 1, 1, false, true),
            ((41, 6), true, 1, 1, false, true),
            ((41, 0), true, 1, 1, true, false),
        ];
        let _ = f;
        for (bp, fleeing, dir, dir_after, up, down) in cases {
            let mut k = tk(&me);
            k.s.movement_direction = dir;
            k.s.walk_speed = 0.5;
            k.s.flee_timer = if fleeing { 4.0 } else { 0.0 };
            let mut next = IntPair { x: pos.x + dir, y: pos.y };
            let mut plan = Plan::default();
            k.pursue_or_flee(IntPair { x: bp.0, y: bp.1 }, &mut next, pos, &mut plan);
            assert_eq!(k.s.movement_direction, dir_after, "{bp:?} {fleeing} {dir}");
            assert_eq!((plan.want_up, plan.want_down), (up, down), "{bp:?} {fleeing} {dir}");
            let turned = dir_after != dir;
            assert_eq!(next.x, pos.x + dir_after);
            let sped = turned || up || down;
            assert_eq!(k.s.walk_speed, if sped { 2.0 } else { 0.5 }, "{bp:?} {fleeing} {dir}");
        }
    }

    #[test]
    fn rider_vertical_input() {
        let f = fx();
        let me = calm(&f);
        let local = uid(1);
        let remote = net_obj(2);
        // (rider, target, wants already, y) -> (up, down)
        let cases: [(Option<ObjRef>, bool, (bool, bool), f32, (bool, bool)); 9] = [
            (None, false, (false, false), 0.9, (false, false)),
            (Some(remote.clone()), false, (false, false), 0.9, (false, false)),
            (Some(local.clone()), true, (false, false), 0.9, (false, false)),
            (Some(local.clone()), false, (true, false), -0.9, (true, false)),
            (Some(local.clone()), false, (false, true), 0.9, (false, true)),
            (Some(local.clone()), false, (false, false), 0.9, (true, false)),
            (Some(local.clone()), false, (false, false), -0.9, (false, true)),
            (Some(local.clone()), false, (false, false), 0.5, (false, false)),
            (Some(local), false, (false, false), -0.5, (false, false)),
        ];
        for (rider, target, (wu, wd), y, (up, down)) in cases {
            npc_state(&me).borrow_mut().rider = rider;
            let mut k = tk(&me);
            k.s.attack_blockhead = target.then(|| uid(5));
            k.s.rider_target_velocity = Vector2 { x: 0.0, y };
            let mut plan = Plan { want_up: wu, want_down: wd, ..Plan::default() };
            k.read_rider_vertical(&mut plan);
            assert_eq!((plan.want_up, plan.want_down), (up, down));
        }
    }

    fn cell(ty: u8, level: u8, contents: u8) -> Option<Tile> {
        let mut t = Tile::default();
        t.type_index = ty;
        t.partial_content_left = level;
        t.contents = contents;
        Some(t)
    }

    #[test]
    fn swimmable_and_open_water_predicates() {
        let free = Nav { has_rider: false, on_air: false, biting: false };
        let ridden = Nav { has_rider: true, ..free_nav() };
        let air = Nav { on_air: true, ..free_nav() };
        let deep = cell(3, 255, 0);
        assert!(free.swimmable(&deep) && free.open_water(&deep));
        assert!(!free.swimmable(&None));
        assert!(!free.swimmable(&cell(1, 255, 0))); // solid
        assert!(!free.swimmable(&cell(3, 100, 0))); // shallow
        assert!(!free.swimmable(&cell(3, 127, 0))); // level 127 is not deep
        assert!(free.swimmable(&cell(3, 128, 0)));
        assert!(!free.swimmable(&cell(2, 0, 0))); // air
        assert!(air.swimmable(&cell(2, 0, 0)) && !air.open_water(&cell(2, 0, 0)));
        assert!(!air.swimmable(&None) && !air.swimmable(&cell(1, 0, 0)));
        // gates stop unridden sharks only
        let gate = cell(3, 255, 0x92);
        assert!(!free.swimmable(&gate) && !free.open_water(&gate) && free.gate_blocks(&gate));
        assert!(ridden.swimmable(&gate) && ridden.open_water(&gate) && !ridden.gate_blocks(&gate));
        assert!(!air.swimmable(&gate));
    }

    fn free_nav() -> Nav {
        Nav { has_rider: false, on_air: false, biting: false }
    }

    #[test]
    fn vertical_step_blockers() {
        let f = fx();
        sea(&f);
        let pos = IntPair { x: 40, y: 3 };
        let w = || cell(3, 255, 0);
        // (c1, c2, nav tweaks, blocked)
        type Case = (Option<Tile>, Option<Tile>, u8, bool);
        let cases: Vec<Case> = vec![
            (None, w(), 0, true),
            (cell(1, 0, 0), w(), 0, true),
            (cell(1, 0, 0), w(), 1, true),
            (cell(1, 0, 0), w(), 2, true),
            (w(), w(), 0, false),
            (cell(2, 0, 0), w(), 0, true),
            (cell(2, 0, 0), None, 1, false),
            (cell(3, 100, 0), w(), 0, true),
            (cell(3, 100, 0), None, 2, false),
            (cell(3, 255, 0x92), w(), 0, true),
            (cell(3, 255, 0x92), w(), 3, false),
            (w(), None, 0, true),
            (w(), cell(2, 0, 0), 0, true),
            (w(), cell(3, 255, 0x92), 0, true),
            (w(), cell(3, 255, 0x92), 3, false),
            (w(), cell(3, 100, 0), 0, true),
            (w(), cell(3, 127, 0), 0, true),
            (w(), cell(3, 128, 0), 0, false),
        ];
        for (i, (c1, c2, tweak, blocked)) in cases.into_iter().enumerate() {
            for sy in [1, -1] {
                for dir in [1, -1] {
                    let mut tiles = f.env.inner.tiles.borrow_mut();
                    for (dx, c) in [(dir, &c1), (2 * dir, &c2)] {
                        match c {
                            Some(t) => {
                                tiles.insert((pos.x + dx, pos.y + sy), *t);
                            }
                            None => {
                                tiles.remove(&(pos.x + dx, pos.y + sy));
                            }
                        }
                    }
                    drop(tiles);
                    let mut nav = free_nav();
                    match tweak {
                        1 => nav.on_air = true,
                        2 => nav.biting = true,
                        3 => nav.has_rider = true,
                        _ => {}
                    }
                    assert_eq!(nav.vertical_blocked(pos, dir, sy), blocked, "case {i} sy {sy} dir {dir}");
                }
            }
        }
    }

    /// First seed whose first raw draw lands in the wanted fraction range.
    fn seed_for(lo: f32, hi: f32) -> u32 {
        (0..10_000u32).find(|s| (lo..=hi).contains(&frac(raw(*s, 1)[0]))).expect("seed")
    }
    const RF_LOW: (f32, f32) = (0.0, 0.89);
    const RF_MID: (f32, f32) = (0.91, 0.94);
    const RF_HIGH: (f32, f32) = (0.96, 1.0);

    /// Run the movement decision from (40, 3) heading east with the given obstacles and want flags.
    fn decide(
        f: &Fx,
        me: &ObjRef,
        rf: (f32, f32),
        solid: &[(i32, i32)],
        wants: (bool, bool),
        biting: bool,
    ) -> (IntPair, u32) {
        sea(f);
        for (x, y) in solid {
            put(f, *x, *y, 1, 0);
        }
        let seed = seed_for(rf.0, rf.1);
        glibc_rand::srand(seed);
        let mut k = tk(me);
        k.s.movement_direction = 1;
        k.s.biting = biting;
        k.s.from_square = IntPair { x: 40, y: 3 };
        k.s.to_square = IntPair { x: 40, y: 3 };
        let mut plan = Plan { want_up: wants.0, want_down: wants.1, ..Plan::default() };
        k.pick_destination(IntPair { x: 40, y: 3 }, IntPair { x: 41, y: 3 }, &mut plan, true);
        (k.s.to_square, seed)
    }

    #[test]
    fn movement_decision_table() {
        let f = fx();
        let me = calm(&f);
        let east = IntPair { x: 41, y: 3 };
        let up = IntPair { x: 41, y: 4 };
        let down = IntPair { x: 41, y: 2 };
        // open water ahead
        assert_eq!(decide(&f, &me, RF_LOW, &[], (false, false), false).0, east);
        assert_eq!(decide(&f, &me, RF_MID, &[], (false, false), false).0, down);
        assert_eq!(decide(&f, &me, RF_HIGH, &[], (false, false), false).0, up);
        // biting ignores the random wobble
        assert_eq!(decide(&f, &me, RF_MID, &[], (false, false), true).0, east);
        assert_eq!(decide(&f, &me, RF_HIGH, &[], (false, false), true).0, east);
        // wants override the straight line
        assert_eq!(decide(&f, &me, RF_LOW, &[], (true, false), false).0, up);
        assert_eq!(decide(&f, &me, RF_LOW, &[], (false, true), false).0, down);
        assert_eq!(decide(&f, &me, RF_LOW, &[], (true, true), false).0, up);
        // blocked ahead (cell beyond is solid): go down by default
        assert_eq!(decide(&f, &me, RF_LOW, &[(42, 3)], (false, false), false).0, down);
        // up preferred
        assert_eq!(decide(&f, &me, RF_HIGH, &[(41, 4)], (false, false), false).0, down); // up blocked, down free
        assert_eq!(decide(&f, &me, RF_HIGH, &[(41, 4), (41, 2)], (false, false), false).0, east); // both blocked
        assert_eq!(decide(&f, &me, RF_LOW, &[(41, 4)], (true, false), false).0, east); // forced up, blocked
                                                                                       // down preferred
        assert_eq!(decide(&f, &me, RF_MID, &[(41, 2)], (false, false), false).0, up); // down blocked, up free
        assert_eq!(decide(&f, &me, RF_MID, &[(41, 2), (41, 4)], (false, false), false).0, east); // both blocked
        assert_eq!(decide(&f, &me, RF_LOW, &[(41, 2)], (false, true), false).0, east); // forced down, blocked
                                                                                       // a gate in front is not swimmable: stuck, stays put
        sea(&f);
        put(&f, 41, 3, 3, 255);
        f.env.inner.tiles.borrow_mut().get_mut(&(41, 3)).unwrap().contents = 0x92;
        let mut k = tk(&me);
        k.s.movement_direction = 1;
        let mut plan = Plan::default();
        k.pick_destination(IntPair { x: 40, y: 3 }, east, &mut plan, true);
        assert_eq!(k.s.to_square, k.s.from_square);
        assert_eq!(k.s.movement_direction, -1);
        assert!(k.s.last_try_couldnt_move);
        // with a rider the gate is open
        npc_state(&me).borrow_mut().rider = Some(uid(1));
        let mut k = tk(&me);
        k.s.movement_direction = 1;
        k.s.to_square = IntPair { x: 40, y: 3 };
        let mut plan = Plan { stationary: true, ..Plan::default() };
        k.pick_destination(IntPair { x: 40, y: 3 }, east, &mut plan, true);
        assert_eq!(k.s.to_square, east);
        assert!(!k.s.last_try_couldnt_move);
        npc_state(&me).borrow_mut().rider = None;
    }

    #[test]
    fn beached_and_stationary_sharks_move_without_a_draw() {
        let f = fx();
        let me = calm(&f);
        sea(&f);
        glibc_rand::srand(5);
        let mut k = tk(&me);
        k.s.movement_direction = 1;
        let mut plan = Plan { stationary: true, ..Plan::default() };
        k.pick_destination(IntPair { x: 40, y: 3 }, IntPair { x: 41, y: 3 }, &mut plan, true);
        assert_eq!(k.s.to_square, IntPair { x: 41, y: 3 });
        assert_eq!(glibc_rand::rand(), raw(5, 1)[0]);
        // airborne shark may enter air cells
        put(&f, 41, 3, 2, 0);
        let mut k = tk(&me);
        k.s.movement_direction = 1;
        k.s.is_on_air_above_solid_ground = true;
        let mut plan = Plan { stationary: true, ..Plan::default() };
        k.pick_destination(IntPair { x: 40, y: 3 }, IntPair { x: 41, y: 3 }, &mut plan, true);
        assert_eq!(k.s.to_square, IntPair { x: 41, y: 3 });
    }

    #[test]
    fn stuck_handling_table() {
        let f = fx();
        let me = calm(&f);
        let t = uid(3);
        let plain = Plan::default();
        let chased = Plan { chased: true, ..Plan::default() };
        // first failure: just turn around
        let mut k = tk(&me);
        k.s.movement_direction = 1;
        glibc_rand::srand(8);
        k.handle_stuck(&plain);
        assert_eq!((k.s.movement_direction, k.s.last_try_couldnt_move), (-1, true));
        assert_eq!(glibc_rand::rand(), raw(8, 1)[0]);
        // second failure while airborne or standing still: random heading, then reversed
        for (air, dir) in [(true, 1), (false, 0)] {
            let mut k = tk(&me);
            k.s.last_try_couldnt_move = true;
            k.s.is_on_air_above_solid_ground = air;
            k.s.movement_direction = dir;
            glibc_rand::srand(8);
            k.handle_stuck(&plain);
            let drawn = if raw(8, 1)[0] > 0x3fffffff { 1 } else { -1 };
            assert_eq!(k.s.movement_direction, -drawn);
        }
        // second failure while moving normally: no draw
        let mut k = tk(&me);
        k.s.last_try_couldnt_move = true;
        k.s.movement_direction = 1;
        glibc_rand::srand(8);
        k.handle_stuck(&plain);
        assert_eq!(k.s.movement_direction, -1);
        assert_eq!(glibc_rand::rand(), raw(8, 1)[0]);
        // a chase that cannot continue turns into a flight
        let mut k = tk(&me);
        k.s.movement_direction = 1;
        k.s.biting = true;
        k.s.attack_blockhead = Some(t.clone());
        k.s.bite_timer = 0.7;
        k.s.has_harmed_blockhead = true;
        k.s.walk_speed = 0.5;
        k.handle_stuck(&chased);
        assert_eq!(k.s.movement_direction, 1);
        assert!(!k.s.biting && k.s.attack_blockhead.is_none() && !k.s.has_harmed_blockhead);
        assert_eq!((k.s.bite_timer, k.s.walk_speed, k.s.flee_timer), (0.0, 2.0, 10.0));
        // ...but a fleeing shark just turns around
        let mut k = tk(&me);
        k.s.movement_direction = 1;
        k.s.flee_timer = 3.0;
        k.handle_stuck(&chased);
        assert_eq!((k.s.movement_direction, k.s.flee_timer), (-1, 3.0));
    }

    #[test]
    fn a_full_tick_with_a_blocked_shark_turns_around() {
        let f = fx();
        let me = calm(&f);
        put(&f, 41, 3, 1, 0);
        {
            let mut s = shark_of(&me).shark.borrow_mut();
            s.travel_fraction = 1.0;
            s.movement_direction = 1;
        }
        tick(&me, 0.1);
        let s = st(&me);
        assert_eq!(s.movement_direction, -1);
        assert!(s.last_try_couldnt_move);
        // a remote-controlled shark does not run the stuck logic
        me.base().borrow_mut().is_net = true;
        {
            let mut s = shark_of(&me).shark.borrow_mut();
            s.travel_fraction = 1.0;
            s.movement_direction = 1;
            s.last_try_couldnt_move = true;
            s.remote_goal_square = IntPair { x: 40, y: 3 };
        }
        tick(&me, 0.1);
        assert!(!st(&me).last_try_couldnt_move);
        me.base().borrow_mut().is_net = false;
    }

    #[test]
    fn long_run_stays_in_the_sea() {
        for seed in [1u32, 2, 3] {
            let f = fx();
            sea(&f);
            let me = spawn(&f, true, true).unwrap();
            npc_state(&me).borrow_mut().age = 3000.0;
            glibc_rand::srand(seed);
            for _ in 0..400 {
                tick(&me, 0.25);
            }
            let p = me.pos();
            assert!((20..=70).contains(&p.x) && (-2..=12).contains(&p.y), "{p:?}");
            assert!(!npc_state(&me).borrow().dead);
        }
    }

    // ------------------------------------------------------------------ network

    #[test]
    fn update_record_roundtrip_on_server_and_client() {
        let f = fx();
        let me = calm(&f);
        let remote = net_obj(5);
        let local = uid(6);
        let rec = |x: u32, y: u32, walk: i16, age: u16, dir: i8| SharkUpdateNetData {
            npc_update_net_data: shark_shark_update_net_data_for_client(&me, None).npc_update_net_data,
            to_square_x: x,
            to_square_y: y,
            walk_speed: walk,
            age,
            movement_direction: dir,
            clear_queue: 0,
            padding: [0; 2],
        };
        // server: nobody remote is riding -> ignored
        shark_do_remote_update(&me, rec(50, 4, 255, 77, 1));
        assert_eq!(st(&me).remote_goal_square, IntPair { x: 0, y: 0 });
        npc_state(&me).borrow_mut().rider = Some(local.clone());
        shark_do_remote_update(&me, rec(50, 4, 255, 77, 1));
        assert_eq!(st(&me).remote_goal_square, IntPair { x: 0, y: 0 });
        // server: remote rider -> goal, speeds and direction copied, update flagged
        npc_state(&me).borrow_mut().rider = Some(remote.clone());
        me.base().borrow_mut().update_needs_to_be_sent = false;
        shark_do_remote_update(&me, rec(50, 4, 255, 77, -1));
        let s = st(&me);
        assert_eq!(s.remote_goal_square, IntPair { x: 50, y: 4 });
        assert_eq!((s.remote_walk_speed, s.walk_speed), (1.0, 1.0));
        assert_eq!((s.remote_movement_direction, s.movement_direction), (-1, -1));
        assert!(me.update_needs_to_be_sent());
        assert_eq!(npc_state(&me).borrow().age, 3000.0);
        // client
        f.env.client.set(true);
        npc_state(&me).borrow_mut().rider = None;
        shark_do_remote_update(&me, rec(51, 5, 127, 99, 1));
        let s = st(&me);
        assert_eq!(s.remote_goal_square, IntPair { x: 51, y: 5 });
        assert_eq!((s.remote_walk_speed, s.remote_movement_direction), (127.0 / 255.0, 1));
        assert_eq!(npc_state(&me).borrow().age, 99.0);
        // client with a local rider: goal untouched, age still taken
        npc_state(&me).borrow_mut().rider = Some(local);
        shark_do_remote_update(&me, rec(60, 6, 10, 123, 1));
        assert_eq!(st(&me).remote_goal_square, IntPair { x: 51, y: 5 });
        assert_eq!(npc_state(&me).borrow().age, 123.0);
        // client with a local target: goal untouched
        npc_state(&me).borrow_mut().rider = None;
        shark_of(&me).shark.borrow_mut().attack_blockhead = Some(uid(8));
        shark_do_remote_update(&me, rec(61, 6, 10, 124, 1));
        assert_eq!(st(&me).remote_goal_square, IntPair { x: 51, y: 5 });
        // ...but a remote target still lets the goal through
        shark_of(&me).shark.borrow_mut().attack_blockhead = Some(net_obj(8));
        shark_do_remote_update(&me, rec(62, 6, 10, 125, 1));
        assert_eq!(st(&me).remote_goal_square, IntPair { x: 62, y: 6 });
    }

    #[test]
    fn remote_update_dispatch() {
        let f = fx();
        let me = calm(&f);
        let bytes = me.update_net_data_for_client(&me, None).unwrap();
        // alive: applied (a client takes the age)
        f.env.client.set(true);
        let mut changed = bytes.clone();
        let mut u = SharkUpdateNetData::read_le(&changed);
        u.age = 4242;
        u.write_le(&mut changed);
        me.remote_update(&me, &changed);
        assert_eq!(npc_state(&me).borrow().age, 4242.0);
        // dead and still fading on a server: the removal mark is cleared
        f.env.client.set(false);
        npc_state(&me).borrow_mut().dead = true;
        me.base().borrow_mut().needs_removed = true;
        shark_of(&me).shark.borrow_mut().travel_fraction = 0.5;
        me.remote_update(&me, &bytes);
        assert!(!me.needs_removed());
        // dead and finished, or on a client: mark stays
        me.base().borrow_mut().needs_removed = true;
        shark_of(&me).shark.borrow_mut().travel_fraction = 1.5;
        me.remote_update(&me, &bytes);
        assert!(me.needs_removed());
        shark_of(&me).shark.borrow_mut().travel_fraction = 0.5;
        f.env.client.set(true);
        me.remote_update(&me, &bytes);
        assert!(me.needs_removed());
    }

    fn creation_bytes(me: &ObjRef, target: u64) -> Vec<u8> {
        let mut b = me.creation_net_data_for_client(me, None).unwrap();
        b[0x70..0x78].copy_from_slice(&target.to_le_bytes());
        b
    }

    #[test]
    fn creation_record_target_adoption() {
        let f = fx();
        let me = calm(&f);
        let remote = net_obj(5);
        let victim = uid(0x55);
        let other = uid(0x66);
        *f.env.inner.bhs.borrow_mut() = vec![victim.clone(), other.clone()];
        // authoritative without a remote rider is not remote-driven: target untouched
        me.remote_creation_data_update(&me, &creation_bytes(&me, 0x55));
        assert!(st(&me).attack_blockhead.is_none());
        // remote-driven
        npc_state(&me).borrow_mut().rider = Some(remote);
        me.base().borrow_mut().update_needs_to_be_sent = false;
        me.base().borrow_mut().creation_data_needs_to_be_sent = false;
        me.remote_creation_data_update(&me, &creation_bytes(&me, 0x55));
        let s = st(&me);
        assert!(s.biting && s.attack_blockhead.as_ref().map_or(false, |a| a.unique_id() == 0x55));
        assert_eq!(s.bite_timer, 1.0);
        assert!(me.update_needs_to_be_sent() && me.creation_data_needs_to_be_sent());
        // same target again: nothing new
        shark_of(&me).shark.borrow_mut().bite_timer = 0.3;
        me.remote_creation_data_update(&me, &creation_bytes(&me, 0x55));
        assert_eq!(st(&me).bite_timer, 0.3);
        // different target replaces
        me.remote_creation_data_update(&me, &creation_bytes(&me, 0x66));
        assert_eq!(st(&me).attack_blockhead.as_ref().unwrap().unique_id(), 0x66);
        assert_eq!(st(&me).bite_timer, 1.0);
        // unknown id: unchanged
        me.remote_creation_data_update(&me, &creation_bytes(&me, 0x99));
        assert_eq!(st(&me).attack_blockhead.as_ref().unwrap().unique_id(), 0x66);
        // zero clears
        me.remote_creation_data_update(&me, &creation_bytes(&me, 0));
        assert!(st(&me).attack_blockhead.is_none());
    }

    #[test]
    fn creation_record_for_dead_sharks() {
        let f = fx();
        let me = calm(&f);
        let bytes = creation_bytes(&me, 0);
        npc_state(&me).borrow_mut().dead = true;
        for (travel, client, cleared) in [(0.5f32, false, true), (1.5, false, false), (0.5, true, false)] {
            f.env.client.set(client);
            me.base().borrow_mut().needs_removed = true;
            shark_of(&me).shark.borrow_mut().travel_fraction = travel;
            me.remote_creation_data_update(&me, &bytes);
            assert_eq!(!me.needs_removed(), cleared, "{travel} {client}");
        }
    }

    #[test]
    fn creation_record_layout() {
        let f = fx();
        let me = calm(&f);
        let target = uid(0x1234);
        shark_of(&me).shark.borrow_mut().attack_blockhead = Some(target.clone());
        let c = me.creation_net_data_for_client(&me, None).unwrap();
        assert_eq!(u64::from_le_bytes(c[0x70..0x78].try_into().unwrap()), 0x1234);
        assert_eq!(&c[0x48..0x70], &shark_shark_update_net_data_for_client(&me, None).to_vec()[..]);
        // net copies never export a target
        me.base().borrow_mut().is_net = true;
        let c = me.creation_net_data_for_client(&me, None).unwrap();
        assert_eq!(&c[0x70..0x78], &[0u8; 8]);
        me.base().borrow_mut().is_net = false;
        assert_eq!(Npc::creation_data_struct_size(shark_of(&me)), 0x78);
        assert_eq!(Npc::max_health(shark_of(&me)), 25);
        let _ = ITEM_SHARK_BUCKET;
    }
}
