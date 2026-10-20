//! `DonkeyLike: NPC`: the abstract base of Donkey and Yak (rideable herbivorous quadrupeds).
//!
//! `cameraPosForBlockhead:` and the GL half of `loadDerivedStuff` are not ported);
//!
//! ## Shape
//! There is no `DonkeyLike` object. [`DonkeyLikeClass`] is the virtual-hook trait that `Donkey` and `Yak` implement
//! (`foodPlantType`, `flies`, `jumps`,...); the `donkey_like_*` free functions are the `[super...]` targets and the
//! bodies of the shared overrides. [`donkey_like_npc_methods!`] expands the shared `Npc` overrides inside the
//! `impl Npc for Donkey/Yak` blocks.
//!
//! ## update
//! `update:` copies the ivars and `floatPos` into a [`Tick`], runs steps 0 to 13 on the copy and writes it back; the copy
//! is flushed before and reloaded after every call that can re-enter the object (marked `RE-ENTRY`).

use std::cell::RefCell;
use std::f64::consts::PI;

use crate::fmath::cconv::f32_to_i32;
use crate::fmath::libm::{fabsf, sinf};
use crate::fmath::vector::clamp;
use crate::npc::donkey::Donkey;
use crate::npc::egg::{penv, t_air_water_snow, t_gate, t_solid, t_water, world_width_tiles_of, wrap_dx, PassiveEnv};
use crate::npc::npc::{
    as_npc, npc_add_rider, npc_append_npc_creation_data_to_data, npc_blockhead_unloaded, npc_die,
    npc_hit_with_force_blockhead,
    npc_init_with_world_dynamic_world_at_position_cache_save_dict_is_adult_was_placed_placed_by_client,
    npc_init_with_world_dynamic_world_cache_net_data, npc_init_with_world_dynamic_world_save_dict_cache,
    npc_npc_creation_net_data_for_client, npc_npc_update_net_data_for_client, npc_remote_creation_data_update,
    npc_remote_update, npc_remove_rider_opt, npc_state, npc_update_accurate_dt_is_simulation, Npc, NPC_CLASS,
};
use crate::npc::yak::Yak;
use crate::objects::base::{new_mutable_dict, padded_bytes, DynamicWorldRef, WorldRef};
use crate::objects::class_info::ClassInfo;
use crate::objects::handle::{ptr_eq, CacheRef, ObjRef};
use crate::plist::value::{PlistDict, PlistValue};
use crate::rng::glibc_rand;
use crate::types::enums::PlantType;
use crate::types::netdata::{DonkeyLikeCreationNetData, DonkeyLikeUpdateNetData};
use crate::types::structs::{IntPair, Vector2, WireStruct};

/// `ITEM_CAGE`
pub const ITEM_CAGE: i32 = 302;
/// `PLANT_CARROT`
pub const PLANT_CARROT: i32 = 4;

/// `DonkeyLike: NPC` descriptor for the `isKindOfClass:` chain of Donkey and Yak.
pub static DONKEY_LIKE_CLASS_INFO: ClassInfo =
    ClassInfo { name: "DonkeyLike", superclass: Some(&NPC_CLASS), protocols: &[] };

const NEG1_PAIR: IntPair = IntPair { x: -1, y: -1 };

/// DonkeyLike ivars (server relevant ones; client-written ones stay 0).
#[derive(Clone, Debug)]
pub struct DonkeyLikeState {
    /// 0x188 smoothed rear position; `.cxx_construct` makes it (1,1), `loadDerivedStuff` sets it to `floatPos`
    pub back_pos: Vector2,
    /// 0x190
    pub target_x_speed: f32,
    /// 0x194
    pub actual_x_speed: f32,
    /// 0x198
    pub goal_x: i32,
    /// 0x19c
    pub y_speed: f32,
    /// 0x1a0
    pub jump_action_queued: bool,
    /// 0x1a4 (-10.0 after `loadDerivedStuff`)
    pub random_time_between_direction_changes: f32,
    /// 0x1a8
    pub random_goal_rotation: f32,
    /// 0x1ac
    pub death_timer: f32,
    /// 0x1b0
    pub bob_timer: f32,
    /// 0x1b4
    pub falling: bool,
    /// 0x1b5
    pub in_water: bool,
    /// 0x1b8
    pub fall_timer: f32,
    /// 0x1bc
    pub has_had_extra_mid_air_jump: bool,
    /// 0x1bd
    pub jump_action_send_value: u8,
    /// 0x380
    pub munch_head_down_timer: f32,
    /// 0x384
    pub head_down_fraction: f32,
    /// 0x388
    pub target_head_down_fraction: f32,
    /// 0x38c
    pub munch_motion_timer: f32,
    /// 0x390 (1.0 after `loadDerivedStuff`)
    pub munch_motion_random_speed: f32,
    /// 0x394
    pub munch_motion_random_speed_change_timer: f32,
    /// 0x3ac ((-1,-1) = none)
    pub remote_stop_pos: IntPair,
    /// 0x3b4
    pub last_send_contained_movement: bool,
    /// 0x178 client written
    pub body_rotation: f32,
    /// 0x180 client written, zeroed by `reactToBeingFed`
    pub walk_timer: f32,
    /// 0x184 client written
    pub y_offset: f32,
}

impl Default for DonkeyLikeState {
    fn default() -> Self {
        DonkeyLikeState {
            back_pos: Vector2::cxx_new(),
            target_x_speed: 0.0,
            actual_x_speed: 0.0,
            goal_x: 0,
            y_speed: 0.0,
            jump_action_queued: false,
            random_time_between_direction_changes: 0.0,
            random_goal_rotation: 0.0,
            death_timer: 0.0,
            bob_timer: 0.0,
            falling: false,
            in_water: false,
            fall_timer: 0.0,
            has_had_extra_mid_air_jump: false,
            jump_action_send_value: 0,
            munch_head_down_timer: 0.0,
            head_down_fraction: 0.0,
            target_head_down_fraction: 0.0,
            munch_motion_timer: 0.0,
            munch_motion_random_speed: 0.0,
            munch_motion_random_speed_change_timer: 0.0,
            remote_stop_pos: IntPair { x: 0, y: 0 },
            last_send_contained_movement: false,
            body_rotation: 0.0,
            walk_timer: 0.0,
            y_offset: 0.0,
        }
    }
}

/// Virtual hooks that Donkey and Yak implement and DonkeyLike calls on `self`.
pub trait DonkeyLikeClass {
    /// The DonkeyLike ivars.
    fn donkey_like(&self) -> &RefCell<DonkeyLikeState>;
    /// `foodPlantType` (default `PLANT_CARROT`)
    fn food_plant_type(&self) -> i32 {
        PLANT_CARROT
    }
    /// `flies` (default NO)
    fn flies(&self) -> bool {
        false
    }
    /// `canJumpMultipleTilesWhileFlying` (default NO)
    fn can_jump_multiple_tiles_while_flying(&self) -> bool {
        false
    }
    /// `jumps` (default YES)
    fn jumps(&self) -> bool {
        true
    }
    /// `maxVelocity` (default 6.0)
    fn max_velocity(&self) -> f32 {
        6.0
    }
    /// `generateBreedForChild` (default 0)
    fn generate_breed_for_child(&self) -> u16 {
        0
    }
    /// `galloping` (client animation hook; default NO)
    fn galloping(&self) -> bool {
        false
    }
    /// `loadDerivedStuff` (virtual: Donkey / Yak override and chain to the DonkeyLike one first).
    fn load_derived_stuff(&self, me: &ObjRef) {
        donkey_like_load_derived_stuff(me)
    }
}

/// Reaches the [`DonkeyLikeClass`] view of a Donkey or Yak.
pub fn as_donkey_like(me: &ObjRef) -> Option<&dyn DonkeyLikeClass> {
    let a = me.as_any();
    if let Some(d) = a.downcast_ref::<Donkey>() {
        return Some(d);
    }
    if let Some(y) = a.downcast_ref::<Yak>() {
        return Some(y);
    }
    None
}

fn dl(me: &ObjRef) -> &dyn DonkeyLikeClass {
    as_donkey_like(me).expect("object is not a Donkey or Yak")
}

fn npc_of(me: &ObjRef) -> &dyn Npc {
    as_npc(me).expect("object is not an NPC")
}

fn r(tag: &'static str) -> f32 {
    glibc_rand::r01_tagged(tag)
}

fn dw_changed(me: &ObjRef) {
    if let Some(dw) = me.dynamic_world() {
        dw.dynamic_world_changed_at_pos_object_type(me.pos(), me.object_type());
    }
}

/// `wrapFloat(a)`: the float flavour of the horizontal wrap.
fn wrap_float(a: f32, w: i32) -> f32 {
    let half = w.wrapping_div(2) as f32;
    let nhalf = w.wrapping_neg().wrapping_div(2) as f32;
    if a < half {
        if a >= nhalf {
            a
        } else {
            a + (w as f32)
        }
    } else {
        a - (w as f32)
    }
}

/// `min(1.0f, x)` as the compiler emits it: `x < 1.0 ? x: 1.0`.
fn min1(x: f32) -> f32 {
    if x < 1.0f32 {
        x
    } else {
        1.0f32
    }
}

fn same_opt(a: &Option<ObjRef>, b: &Option<ObjRef>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(a), Some(b)) => ptr_eq(a, b),
        _ => false,
    }
}

/// `contentsTypeForPlantType(plantType, NO)`.
fn contents_for_plant(plant_type: i32) -> u8 {
    crate::world::growth::contents_type_for_plant_type(PlantType(plant_type as u32), false).0 as u8
}

// ------------------------------------------------------------------------------------------------ init

/// `-[DonkeyLike loadDerivedStuff]` server part: `backPos = floatPos`, `-10.0`, `1.0`, `(-1,-1)`.
/// No `rand()`. Donkey and Yak chain to it.
pub fn donkey_like_load_derived_stuff(me: &ObjRef) {
    let fp = me.float_pos();
    let s = dl(me).donkey_like();
    let mut s = s.borrow_mut();
    s.back_pos = fp;
    s.random_time_between_direction_changes = -10.0f32;
    s.munch_motion_random_speed = 1.0f32;
    s.remote_stop_pos = NEG1_PAIR;
}

/// `-[DonkeyLike initWithWorld:dynamicWorld:atPosition:cache:saveDict:isAdult:wasPlaced:placedByClient:]`.
#[allow(clippy::too_many_arguments)]
pub fn donkey_like_init_with_world_dynamic_world_at_position_cache_save_dict_is_adult_was_placed_placed_by_client(
    me: &ObjRef,
    world: &WorldRef,
    dw: &DynamicWorldRef,
    pos: IntPair,
    cache: CacheRef,
    save_dict: Option<&PlistDict>,
    is_adult: bool,
    was_placed: bool,
    client_id: Option<&str>,
) -> bool {
    if !npc_init_with_world_dynamic_world_at_position_cache_save_dict_is_adult_was_placed_placed_by_client(
        me, world, dw, pos, cache, save_dict, is_adult, was_placed, client_id,
    ) {
        return false;
    }
    dl(me).load_derived_stuff(me);
    true
}

/// `-[DonkeyLike initWithWorld:dynamicWorld:saveDict:cache:]`.
pub fn donkey_like_init_with_world_dynamic_world_save_dict_cache(
    me: &ObjRef,
    world: &WorldRef,
    dw: &DynamicWorldRef,
    save_dict: &PlistDict,
    cache: CacheRef,
) -> bool {
    if !npc_init_with_world_dynamic_world_save_dict_cache(me, world, dw, save_dict, cache) {
        return false;
    }
    dl(me).load_derived_stuff(me);
    true
}

/// `-[DonkeyLike initWithWorld:dynamicWorld:cache:netData:]`: `loadDerivedStuff`, then
/// `[self remoteCreationDataUpdate:netData]` (virtual).
pub fn donkey_like_init_with_world_dynamic_world_cache_net_data(
    me: &ObjRef,
    world: &WorldRef,
    dw: &DynamicWorldRef,
    cache: CacheRef,
    net_data: &[u8],
) -> bool {
    if !npc_init_with_world_dynamic_world_cache_net_data(me, world, dw, cache, net_data) {
        return false;
    }
    dl(me).load_derived_stuff(me);
    me.remote_creation_data_update(me, net_data);
    true
}

// ------------------------------------------------------------------------------------------------ net data

/// `-[DonkeyLike donkeyLikeUpdateNetDataForClient:]`. Side effects: `jumpActionSendValue` is reset,
/// `lastSendContainedMovement` is set. `walkSpeed` (0x1a) and the padding (0x1f) are zero.
pub fn donkey_like_donkey_like_update_net_data_for_client(
    me: &ObjRef,
    client_id: Option<&str>,
) -> DonkeyLikeUpdateNetData {
    let base = npc_npc_update_net_data_for_client(me, client_id);
    let cell = dl(me).donkey_like();
    let mut s = cell.borrow_mut();
    let out = DonkeyLikeUpdateNetData {
        npc_update_net_data: base,
        target_x_speed: f32_to_i32(s.target_x_speed * 255.0f32) as i16,
        walk_speed: 0,
        random_goal_rotation: f32_to_i32(s.random_goal_rotation * 255.0f32) as i16,
        jump_action_send_value: s.jump_action_send_value,
        padding: [0],
    };
    s.jump_action_send_value = 0;
    s.last_send_contained_movement = fabsf(s.target_x_speed) > 2.0f32;
    out
}

/// `-[DonkeyLike donkeyLikeCreationNetDataForClient:]`: NPC part, then the update part.
pub fn donkey_like_donkey_like_creation_net_data_for_client(
    me: &ObjRef,
    client_id: Option<&str>,
) -> DonkeyLikeCreationNetData {
    let n = npc_npc_creation_net_data_for_client(me, client_id);
    let u = donkey_like_donkey_like_update_net_data_for_client(me, client_id);
    DonkeyLikeCreationNetData { npc_creation_net_data: n, donkey_like_update_net_data: u }
}

/// `-[DonkeyLike creationNetDataForClient:]`: 0x68 bytes plus the NPC name tail.
pub fn donkey_like_creation_net_data_for_client(me: &ObjRef, client_id: Option<&str>) -> Vec<u8> {
    let d = donkey_like_donkey_like_creation_net_data_for_client(me, client_id);
    npc_append_npc_creation_data_to_data(me, d.to_vec())
}

/// `-[DonkeyLike updateNetDataForClient:]`: the 0x20 struct.
pub fn donkey_like_update_net_data_for_client(me: &ObjRef, client_id: Option<&str>) -> Vec<u8> {
    donkey_like_donkey_like_update_net_data_for_client(me, client_id).to_vec()
}

/// `-[DonkeyLike creationDataStructSize]`
pub fn donkey_like_creation_data_struct_size() -> usize {
    0x68
}

/// `-[DonkeyLike maxHealth]`
pub fn donkey_like_max_health() -> u16 {
    32
}

/// `-[DonkeyLike doDonkeyLikeRemoteUpdate:]`: the path of a remote rider's input.
pub fn donkey_like_do_donkey_like_remote_update(me: &ObjRef, u: DonkeyLikeUpdateNetData) {
    let d = dl(me);
    let cell = d.donkey_like();
    cell.borrow_mut().remote_stop_pos = NEG1_PAIR;
    let rider = npc_state(me).borrow().rider.clone();
    // gate: (isNet && rider == nil) || (rider != nil && [rider isNet])
    if !me.is_net() || rider.is_some() {
        match &rider {
            None => return,
            Some(r) if !r.is_net() => return,
            _ => {}
        }
    }
    let w = world_width_tiles_of(me);
    let pos = me.pos();
    let target = (u.target_x_speed as i32 as f32) / 255.0f32;
    cell.borrow_mut().target_x_speed = target;
    let rp = IntPair {
        x: u.npc_update_net_data.dynamic_object_net_data.pos_x as i32,
        y: u.npc_update_net_data.dynamic_object_net_data.pos_y as i32,
    };
    if fabsf(target) < 2.0f32 {
        cell.borrow_mut().remote_stop_pos = rp;
    }
    let dx = wrap_dx(rp.x.wrapping_sub(pos.x), w);
    if dx.wrapping_abs() < 9 && rp.y.wrapping_sub(pos.y).wrapping_abs() < 5 {
        if dx.wrapping_abs() > 2 {
            let t = cell.borrow().target_x_speed;
            if ((t > 0.0f32) != (dx > 0)) || fabsf(t) < 2.0f32 {
                cell.borrow_mut().target_x_speed = 3.5f32 * (if dx < 1 { -1.0f32 } else { 1.0f32 });
            }
        }
    } else {
        // RE-ENTRY: updatePosition
        me.update_position(me, rp);
        let bp = Vector2 { x: 0.5f32 + (rp.x as f32), y: rp.y as f32 };
        cell.borrow_mut().back_pos = bp;
        me.base().borrow_mut().float_pos = bp;
    }
    if u.jump_action_send_value != 0 {
        let mut s = cell.borrow_mut();
        s.jump_action_queued = true;
        // jumpActionSendValue == 2 plays a sound on the receiver (omitted)
        if !me.is_net() {
            s.jump_action_send_value = u.jump_action_send_value;
        }
    }
    cell.borrow_mut().random_goal_rotation = (u.random_goal_rotation as i32 as f32) / 255.0f32;
}

/// `-[DonkeyLike remoteUpdate:]`: `dead` is read after the super call.
pub fn donkey_like_remote_update(me: &ObjRef, net_data: &[u8]) {
    npc_remote_update(me, net_data);
    if !npc_state(me).borrow().dead {
        let u = DonkeyLikeUpdateNetData::read_le(&padded_bytes(net_data, 0x20));
        donkey_like_do_donkey_like_remote_update(me, u);
    }
}

/// `-[DonkeyLike remoteCreationDataUpdate:]`.
pub fn donkey_like_remote_creation_data_update(me: &ObjRef, net_data: &[u8]) {
    npc_remote_creation_data_update(me, net_data);
    let c = DonkeyLikeCreationNetData::read_le(&padded_bytes(net_data, 0x68));
    donkey_like_do_donkey_like_remote_update(me, c.donkey_like_update_net_data);
}

// ------------------------------------------------------------------------------------------------ small overrides

/// `-[DonkeyLike die:]`: the super call is inside the `!dead` guard (sounds omitted).
pub fn donkey_like_die(me: &ObjRef, b: Option<&ObjRef>) {
    if !npc_state(me).borrow().dead {
        npc_die(me, b);
    }
}

/// `-[DonkeyLike hitWithForce:blockhead:]`: after the NPC hit (alive, not a replica) the animal
/// jumps and flees; ONE raw `rand()` picks the direction.
pub fn donkey_like_hit_with_force_blockhead(me: &ObjRef, force: i32, b: Option<&ObjRef>) {
    npc_hit_with_force_blockhead(me, force, b);
    if npc_state(me).borrow().dead || me.is_net() {
        return;
    }
    let cell = dl(me).donkey_like();
    {
        let mut s = cell.borrow_mut();
        s.jump_action_queued = true;
        s.jump_action_send_value = 2;
    }
    if npc_state(me).borrow().being_controlled {
        return;
    }
    let env = penv();
    let w = world_width_tiles_of(me);
    let pos = me.pos();
    let rider_some = npc_state(me).borrow().rider.is_some();
    let target = if glibc_rand::rand_tagged("DonkeyLike.hit.dir") < 0x40000000 { -8.0f32 } else { 8.0f32 };
    {
        let mut s = cell.borrow_mut();
        s.random_time_between_direction_changes = 5.0f32;
        s.target_x_speed = target;
    }
    let xo = if target > 0.0f32 { 1 } else { -1 };
    let new_pos = IntPair { x: pos.x.wrapping_add(xo), y: pos.y };
    let ok = |nt: &Option<crate::types::tile::Tile>, na: &Option<crate::types::tile::Tile>| {
        t_air_water_snow(nt)
            && t_air_water_snow(na)
            && (wrap_dx(new_pos.x.wrapping_sub(pos.x), w) == 0 || rider_some || (!t_gate(nt) && !t_gate(na)))
    };
    let nt = env.tile_loaded(new_pos.x, new_pos.y);
    let na = env.tile_loaded(new_pos.x, new_pos.y.wrapping_add(1));
    if !ok(&nt, &na) {
        let nt2 = na;
        let na2 = env.tile_loaded(new_pos.x, new_pos.y.wrapping_add(2));
        if !ok(&nt2, &na2) {
            let mut s = cell.borrow_mut();
            s.target_x_speed = -s.target_x_speed;
        }
    }
    let mut s = cell.borrow_mut();
    s.goal_x = f32_to_i32((pos.x as f32) + 2.0f32 * s.target_x_speed);
    me.set_update_needs_to_be_sent(true);
}

/// `-[DonkeyLike reactToBeingFed]`: jump, `randomGoalRotation = 0`, `walkTimer = 0` (not announced).
pub fn donkey_like_react_to_being_fed(me: &ObjRef) {
    let mut s = dl(me).donkey_like().borrow_mut();
    s.jump_action_queued = true;
    s.random_goal_rotation = 0.0f32;
    s.walk_timer = 0.0f32;
}

/// `-[DonkeyLike reactToBeingHit]`: sound only.
pub fn donkey_like_react_to_being_hit(_me: &ObjRef) {}

/// `-[DonkeyLike addRider:]`.
pub fn donkey_like_add_rider(me: &ObjRef, rider: Option<&ObjRef>) {
    if let Some(r) = rider {
        npc_add_rider(me, r);
    }
    let controlled = npc_state(me).borrow().being_controlled;
    let mut s = dl(me).donkey_like().borrow_mut();
    if !controlled {
        s.target_x_speed = 0.0f32;
        s.actual_x_speed = 0.0f32;
        me.set_update_needs_to_be_sent(true);
    } else {
        s.random_goal_rotation = 0.0f32;
    }
}

/// `-[DonkeyLike removeRider:]`: the guard is compared BEFORE the super call; nil == nil passes
///.
pub fn donkey_like_remove_rider_opt(me: &ObjRef, rider: Option<&ObjRef>) {
    let cur = npc_state(me).borrow().rider.clone();
    let rr: Option<ObjRef> = rider.cloned();
    if same_opt(&rr, &cur) {
        npc_remove_rider_opt(me, rider);
        let mut s = dl(me).donkey_like().borrow_mut();
        s.target_x_speed = 0.0f32;
        s.actual_x_speed = 0.0f32;
    }
}

/// `-[DonkeyLike removeRider:]`
pub fn donkey_like_remove_rider(me: &ObjRef, rider: &ObjRef) {
    donkey_like_remove_rider_opt(me, Some(rider))
}

/// `-[DonkeyLike blockheadCanRide:usingItem:]`; Donkey overrides, Yak inherits.
pub fn donkey_like_blockhead_can_ride_using_item(me: &ObjRef, b: &ObjRef, _item_type: i32) -> bool {
    let npc = npc_of(me);
    let (age, controlled, dead) = {
        let n = npc_state(me).borrow();
        (n.age, n.being_controlled, n.dead)
    };
    age > 900.0f32 && !controlled && npc.belongs_to_player_with_blockhead(b) && !dead && !me.needs_removed()
}

/// `-[DonkeyLike blockheadUnloaded:]`.
pub fn donkey_like_blockhead_unloaded(me: &ObjRef, b: &ObjRef) {
    let is_rider = npc_state(me).borrow().rider.as_ref().map_or(false, |r| ptr_eq(r, b));
    if is_rider {
        let mut s = dl(me).donkey_like().borrow_mut();
        s.actual_x_speed = 0.0f32;
        s.target_x_speed = 0.0f32;
    }
    npc_blockhead_unloaded(me, b);
}

/// `-[DonkeyLike setTargetVelocity:]`: only a local rider steers.
pub fn donkey_like_set_target_velocity(me: &ObjRef, v: Vector2) {
    let rider = npc_state(me).borrow().rider.clone();
    if rider.map_or(false, |r| !r.is_net()) {
        let mv = dl(me).max_velocity();
        let cell = dl(me).donkey_like();
        let changed = {
            let mut s = cell.borrow_mut();
            s.target_x_speed = v.x * mv;
            s.last_send_contained_movement != (fabsf(s.target_x_speed) > 2.0f32)
        };
        if changed {
            me.set_update_needs_to_be_sent(true);
        }
    }
}

/// `-[DonkeyLike swipeUpGesture]`.
pub fn donkey_like_swipe_up_gesture(me: &ObjRef) {
    let mut s = dl(me).donkey_like().borrow_mut();
    s.jump_action_queued = true;
    s.jump_action_send_value = 1;
    me.set_update_needs_to_be_sent(true);
}

/// `-[DonkeyLike rideDirection]`.
pub fn donkey_like_ride_direction(me: &ObjRef) -> i32 {
    if npc_state(me).borrow().rider.is_none() {
        return 0;
    }
    let t = dl(me).donkey_like().borrow().target_x_speed;
    if t > 2.0f32 {
        1
    } else if t < -2.0f32 {
        -1
    } else {
        0
    }
}

/// `-[DonkeyLike riderPosForBlockhead:]`: `(fp.x, fp.y + (yOffset + 0.8), -0.75, 1)`.
pub fn donkey_like_rider_pos_for_blockhead(me: &ObjRef, _b: &ObjRef) -> [f32; 4] {
    let fp = me.float_pos();
    let yo = dl(me).donkey_like().borrow().y_offset;
    [fp.x, fp.y + (((yo as f64) + 0.8f64) as f32), -0.75f32, 1.0f32]
}

/// `-[DonkeyLike renderPos]`: `floatPos + (0, 0.3)`.
pub fn donkey_like_render_pos(me: &ObjRef) -> Vector2 {
    let fp = me.float_pos();
    Vector2 { x: fp.x, y: fp.y + 0.3f32 }
}

/// `-[DonkeyLike canBeCapturedByBlockhead:withItemType:]`.
pub fn donkey_like_can_be_captured_by_blockhead_with_item_type(me: &ObjRef, b: &ObjRef, item: i32) -> bool {
    let npc = npc_of(me);
    item == ITEM_CAGE && (!npc.tamed() || npc.belongs_to_player_with_blockhead(b))
}

/// `-[DonkeyLike cantBeCapturedTipStringForBlockhead:withItemType:]`.
pub fn donkey_like_cant_be_captured_tip_string_for_blockhead_with_item_type(
    me: &ObjRef,
    b: &ObjRef,
    item: i32,
) -> Option<String> {
    let npc = npc_of(me);
    if item == ITEM_CAGE && npc.tamed() && !npc.belongs_to_player_with_blockhead(b) {
        Some(format!("THAT {} BELONGS TO\nSOMEONE ELSE", npc.species_name()))
    } else {
        None
    }
}

/// Expands, inside `impl Npc for Donkey/Yak {... }`, the `Npc` overrides that DonkeyLike provides. The class adds its
/// own constants and the overrides it replaces (`blockhead_can_ride_using_item` for Donkey, `jumps_on_swipe` for Yak).
#[macro_export]
macro_rules! donkey_like_npc_methods {
    () => {
        fn die(&self, me: &$crate::objects::handle::ObjRef, killer: Option<&$crate::objects::handle::ObjRef>) {
            $crate::npc::donkey_like::donkey_like_die(me, killer)
        }
        fn hit_with_force_blockhead(
            &self,
            me: &$crate::objects::handle::ObjRef,
            force: i32,
            b: Option<&$crate::objects::handle::ObjRef>,
        ) {
            $crate::npc::donkey_like::donkey_like_hit_with_force_blockhead(me, force, b)
        }
        fn react_to_being_fed(&self, me: &$crate::objects::handle::ObjRef) {
            $crate::npc::donkey_like::donkey_like_react_to_being_fed(me)
        }
        fn add_rider(&self, me: &$crate::objects::handle::ObjRef, b: &$crate::objects::handle::ObjRef) {
            $crate::npc::donkey_like::donkey_like_add_rider(me, Some(b))
        }
        fn remove_rider(&self, me: &$crate::objects::handle::ObjRef, b: &$crate::objects::handle::ObjRef) {
            $crate::npc::donkey_like::donkey_like_remove_rider(me, b)
        }
        fn remove_rider_opt(&self, me: &$crate::objects::handle::ObjRef, b: Option<&$crate::objects::handle::ObjRef>) {
            $crate::npc::donkey_like::donkey_like_remove_rider_opt(me, b)
        }
        fn swipe_up_gesture(&self, me: &$crate::objects::handle::ObjRef) {
            $crate::npc::donkey_like::donkey_like_swipe_up_gesture(me)
        }
        fn ride_direction(&self) -> i32 {
            $crate::npc::donkey_like::donkey_like_ride_direction(&$crate::objects::base::DynamicObject::me(self))
        }
        fn rider_pos_for_blockhead(&self, b: &$crate::objects::handle::ObjRef) -> [f32; 4] {
            $crate::npc::donkey_like::donkey_like_rider_pos_for_blockhead(
                &$crate::objects::base::DynamicObject::me(self),
                b,
            )
        }
        fn set_target_velocity(&self, v: $crate::types::structs::Vector2) {
            $crate::npc::donkey_like::donkey_like_set_target_velocity(
                &$crate::objects::base::DynamicObject::me(self),
                v,
            )
        }
        fn rider_body_y_rotation_for_blockhead(&self, _b: &$crate::objects::handle::ObjRef) -> f32 {
            $crate::npc::donkey_like::DonkeyLikeClass::donkey_like(self).borrow().body_rotation
        }
        fn can_be_captured_by_blockhead_with_item_type(&self, b: &$crate::objects::handle::ObjRef, item: i32) -> bool {
            $crate::npc::donkey_like::donkey_like_can_be_captured_by_blockhead_with_item_type(
                &$crate::objects::base::DynamicObject::me(self),
                b,
                item,
            )
        }
        fn cant_be_captured_tip_string_for_blockhead_with_item_type(
            &self,
            b: &$crate::objects::handle::ObjRef,
            item: i32,
        ) -> Option<String> {
            $crate::npc::donkey_like::donkey_like_cant_be_captured_tip_string_for_blockhead_with_item_type(
                &$crate::objects::base::DynamicObject::me(self),
                b,
                item,
            )
        }
    };
}

// ------------------------------------------------------------------------------------------------ update

/// Working copy of one tick: the ivars plus `floatPos`.
struct Tick<'a> {
    me: &'a ObjRef,
    d: &'a dyn DonkeyLikeClass,
    npc: &'a dyn Npc,
    env: std::rc::Rc<dyn PassiveEnv>,
    s: DonkeyLikeState,
    fp: Vector2,
    w: i32,
}

impl<'a> Tick<'a> {
    fn flush(&self) {
        *self.d.donkey_like().borrow_mut() = self.s.clone();
        self.me.base().borrow_mut().float_pos = self.fp;
    }
    fn reload(&mut self) {
        self.s = self.d.donkey_like().borrow().clone();
        self.fp = self.me.float_pos();
    }
    fn tile(&self, x: i32, y: i32) -> Option<crate::types::tile::Tile> {
        self.env.tile_loaded(x, y)
    }
    fn rider(&self) -> Option<ObjRef> {
        npc_state(self.me).borrow().rider.clone()
    }
    fn being_controlled(&self) -> bool {
        npc_state(self.me).borrow().being_controlled
    }
    fn update_pos(&mut self, p: IntPair) {
        // RE-ENTRY: updatePosition then checkCurrentPositionForFood (-> reactToBeingFed writes the DonkeyLike state)
        self.flush();
        self.me.update_position(self.me, p);
        self.npc.check_current_position_for_food(self.me);
        self.reload();
    }
}

/// `-[DonkeyLike update:accurateDT:isSimulation:]`, steps 0 to 13.
pub fn donkey_like_update_accurate_dt_is_simulation(me: &ObjRef, dt: f32, accurate_dt: f32, is_simulation: bool) {
    // Step 0
    npc_update_accurate_dt_is_simulation(me, dt, accurate_dt, is_simulation);
    if me.needs_removed() {
        return;
    }
    let d = dl(me);
    let npc = npc_of(me);
    let mut t = Tick {
        me,
        d,
        npc,
        env: penv(),
        s: d.donkey_like().borrow().clone(),
        fp: me.float_pos(),
        w: world_width_tiles_of(me),
    };
    run_steps(&mut t, dt);
    t.flush();
}

#[allow(clippy::cognitive_complexity)]
fn run_steps(t: &mut Tick, dt: f32) {
    let me = t.me;
    let ns = npc_state(me);

    // Step 1: dead
    if ns.borrow().dead {
        t.s.death_timer += dt;
        if t.s.death_timer < 1.0f32 {
            return;
        }
        if !me.is_net() {
            if ns.borrow().age > 900.0f32 {
                t.npc.create_item_drops_for_death(me);
            }
            me.base().borrow_mut().needs_removed = true;
        }
        t.s.death_timer = 1.0f32;
        return;
    }

    // Step 2: displacement
    let cdt = if dt >= 0.1f32 { 0.1f32 } else { dt };
    let moving = fabsf(t.s.actual_x_speed) > 2.0f32;
    let mut y_add = t.s.y_speed * cdt;
    let mut x_add = 0.0f32;
    let pos0 = me.pos();
    let tile0 = t.tile(pos0.x, pos0.y);
    if t_water(&tile0) {
        t.s.bob_timer += cdt;
        t.s.y_speed += sinf(((t.s.bob_timer as f64) * PI * 2.0f64) as f32) * cdt * 8.0f32;
    }
    if moving {
        let fast = if ns.borrow().being_controlled { true } else { ns.borrow().age < 900.0f32 };
        let mult = if fast { 0.8f32 } else { 0.5f32 };
        x_add = t.s.actual_x_speed * cdt * mult;
        if t_water(&tile0) {
            let full = (tile0.map_or(0, |x| x.partial_content_left) / 255) as i32;
            x_add = (1.0f32 - 0.8f32 * (full as f32)) * x_add;
            y_add = 0.5f32 * y_add;
        }
    }
    if t.fp.x <= 0.0f32 && t.fp.x != 0.0f32 {
        t.fp.x += t.w as f32;
    }
    let old_fp = t.fp;
    t.fp = Vector2 { x: t.fp.x + x_add, y: t.fp.y + y_add };
    let pos = me.pos();
    let mut new_pos = IntPair { x: f32_to_i32(t.fp.x), y: f32_to_i32(t.fp.y) };
    let dx_new = wrap_dx(new_pos.x.wrapping_sub(pos.x), t.w);

    // Step 3: tile transition
    if dx_new != 0 || new_pos.y != pos.y {
        while pos.y.wrapping_sub(new_pos.y) > 1 {
            t.fp.y += 0.5f32;
            new_pos.y = f32_to_i32(t.fp.y);
        }
        let mut new_tile = t.tile(new_pos.x, new_pos.y);
        let mut new_above = t.tile(new_pos.x, new_pos.y.wrapping_add(1));
        let rider_some = t.rider().is_some();
        let free = t_air_water_snow(&new_tile) && t_air_water_snow(&new_above);
        if free && !(dx_new != 0 && !rider_some && (t_gate(&new_tile) || t_gate(&new_above))) {
            t.update_pos(new_pos);
        } else {
            let dx = wrap_dx(new_pos.x.wrapping_sub(pos.x), t.w);
            if dx != 0 {
                t.fp.x = old_fp.x;
                let into = (dx > 0 && t.s.actual_x_speed > 0.0f32) || (dx < 0 && t.s.actual_x_speed < 0.0f32);
                if into {
                    if !t.env.dw_blockhead_occupies_tile(pos) && !t.env.dw_npc_exists_at_pos(pos, me) {
                        t.s.actual_x_speed = 0.0f32;
                    } else {
                        t.s.actual_x_speed = -t.s.actual_x_speed;
                    }
                }
            }
            new_pos = IntPair { x: f32_to_i32(t.fp.x), y: f32_to_i32(t.fp.y) };
            new_tile = t.tile(new_pos.x, new_pos.y);
            new_above = t.tile(new_pos.x, new_pos.y.wrapping_add(1));
            let rider_some = t.rider().is_some();
            if t_air_water_snow(&new_tile)
                && t_air_water_snow(&new_above)
                && (wrap_dx(new_pos.x.wrapping_sub(pos.x), t.w) == 0
                    || rider_some
                    || (!t_gate(&new_tile) && !t_gate(&new_above)))
            {
                t.update_pos(new_pos);
            } else if new_pos.y != pos.y {
                t.fp.y = old_fp.y;
                if (pos.y < new_pos.y && t.s.y_speed > 0.0f32) || (new_pos.y < pos.y && t.s.y_speed < 0.0f32) {
                    if t.s.y_speed < 0.0f32 {
                        t.fp.y = pos.y as f32;
                        if t.s.fall_timer > 1.0f32 && !t.d.flies() {
                            let dmg = f32_to_i32((t.s.fall_timer / 8.0f32) * (t.npc.max_health() as f32));
                            let rider = t.rider();
                            // RE-ENTRY: the DonkeyLike hit override writes the state
                            t.flush();
                            t.npc.hit_with_force_blockhead(me, dmg, rider.as_ref());
                            t.reload();
                            t.s.fall_timer = 0.0f32;
                        }
                    }
                    t.s.y_speed = 0.0f32;
                }
            }
        }
        // still inside the "tile transition" block
        if !me.is_net() {
            if t.npc.can_mate() {
                if let Some(other) = t.env.dw_npc_close_enough_to_breed(me) {
                    t.npc.mate_with_npc(me, &other);
                    if let Some(o) = as_npc(&other) {
                        o.mate_with_npc(&other, me);
                    }
                }
            }
            let fullness = ns.borrow().fullness;
            if (fullness as f64) < 5400.0f64 {
                if let Some(nt) = &new_tile {
                    if nt.foreground_contents != 0 {
                        let food = t.d.food_plant_type();
                        if nt.foreground_contents == contents_for_plant(food) {
                            if let Some(plant) = t.env.dw_plant_at_pos(new_pos) {
                                if t.env.plant_type(&plant) == food {
                                    let avail = t.env.plant_available_food(&plant);
                                    let take = if avail >= 900.0f32 { 900.0f32 } else { avail };
                                    {
                                        let mut n = ns.borrow_mut();
                                        n.fullness = ((n.fullness as f64) + (take as f64) * 0.2f64) as f32;
                                    }
                                    let again = t.env.plant_available_food(&plant);
                                    t.env.plant_set_available_food(&plant, again - take);
                                }
                            }
                        }
                    }
                }
            }
            let (fullness, cooldown, lay) = {
                let n = ns.borrow();
                (n.fullness, n.lay_cooldown_timer, n.lay_timer)
            };
            if (fullness as f64) > 5400.0f64 && cooldown <= 0.0f32 && lay <= 0.0f32 {
                ns.borrow_mut().lay_timer = 900.0f32 + r("DonkeyLike.update.layTimer") * 20.0f32;
                dw_changed(me);
            }
        }
    }

    // Step 4: backPos y smoothing (current pos)
    let pos = me.pos();
    let below = t.tile(pos.x, pos.y.wrapping_sub(1));
    if t.s.back_pos.y <= t.fp.y {
        if t.s.back_pos.y < t.fp.y {
            if t.s.back_pos.y < t.fp.y - 1.0f32 {
                t.s.back_pos.y = t.fp.y - 1.0f32;
            }
            t.s.back_pos.y += 2.0f32 * dt;
            if t.s.back_pos.y > t.fp.y {
                t.s.back_pos.y = t.fp.y;
            }
        }
    } else {
        let by = f32_to_i32(t.s.back_pos.y);
        let bb = t.tile(f32_to_i32(t.s.back_pos.x), by.wrapping_sub(1));
        let air_tile = t_air_water_snow(&bb);
        if air_tile || (t.s.back_pos.y - (by as f32)) > 0.0f32 {
            if t.fp.y + 1.0f32 < t.s.back_pos.y {
                t.s.back_pos.y = t.fp.y + 1.0f32;
            }
            t.s.back_pos.y -= 8.0f32 * dt;
            if t.fp.y <= t.s.back_pos.y {
                if !air_tile && t.s.back_pos.y < (by as f32) {
                    t.s.back_pos.y = by as f32;
                }
            } else {
                t.s.back_pos.y = t.fp.y;
            }
        }
    }

    // Step 5: x smoothing
    if moving {
        let dd = wrap_float(t.s.back_pos.x - t.fp.x, t.w);
        if t.s.actual_x_speed <= 0.0f32 {
            if dd > 1.0f32 {
                t.s.back_pos.x = t.fp.x + 1.0f32;
            }
        } else if dd < -1.0f32 {
            t.s.back_pos.x = t.fp.x - 1.0f32;
        }
    } else {
        let xo = wrap_float(t.fp.x - (pos.x as f32), t.w);
        if xo > 0.5f32 {
            t.fp.x -= 4.0f32 * dt;
            if wrap_float(t.fp.x - (pos.x as f32), t.w) < 0.5f32 {
                t.fp.x = 0.5f32 + (pos.x as f32);
            }
        } else if xo < 0.5f32 {
            t.fp.x += 4.0f32 * dt;
            if wrap_float(t.fp.x - (pos.x as f32), t.w) > 0.5f32 {
                t.fp.x = 0.5f32 + (pos.x as f32);
            }
        }
        let dd = wrap_float(t.s.back_pos.x - t.fp.x, t.w);
        if dd <= 0.0f32 {
            if wrap_float(t.s.back_pos.x - t.fp.x, t.w) < 0.0f32 {
                t.s.back_pos.x += 4.0f32 * dt;
                if t.s.back_pos.x > t.fp.x {
                    t.s.back_pos.x = t.fp.x;
                }
            }
        } else {
            t.s.back_pos.x -= 4.0f32 * dt;
            if t.s.back_pos.x < t.fp.x {
                t.s.back_pos.x = t.fp.x;
            }
        }
    }

    // Step 6: support / gravity
    t.s.falling = false;
    t.s.in_water = false;
    if !t_air_water_snow(&below) {
        if t_solid(&below) {
            let above = t.tile(pos.x, pos.y.wrapping_add(1));
            if !t_water(&above) {
                if ((t.fp.y - (pos.y as f32)) as f64) > 0.0001f64 {
                    t.s.y_speed = ((t.s.y_speed as f64) * (1.0f64 - (clamp(dt, 0.0f32, 0.99f32) as f64))
                        - ((40.0f32 * dt) as f64)) as f32;
                    t.s.falling = true;
                }
            } else {
                t.s.y_speed = ((t.s.y_speed as f64) * (1.0f64 - (clamp(dt, 0.0f32, 0.99f32) as f64))
                    + ((1.0f32 * dt) as f64)) as f32;
                t.s.falling = true;
            }
        }
    } else {
        let above = t.tile(pos.x, pos.y.wrapping_add(1));
        if !t_water(&above) {
            let gravity = if t.d.flies() { 10.0f32 } else { 40.0f32 };
            t.s.y_speed = ((t.s.y_speed as f64) * 0.95f64 - ((dt * gravity) as f64)) as f32;
        } else {
            t.s.in_water = true;
            t.s.y_speed = ((t.s.y_speed as f64) * 0.95f64 + ((1.0f32 * dt) as f64)) as f32;
        }
        t.s.falling = true;
    }
    if !t.s.falling {
        t.s.actual_x_speed = 0.0f32;
        t.s.y_speed = 0.0f32;
        t.s.has_had_extra_mid_air_jump = false;
    } else if t_water(&tile0) {
        t.s.in_water = true;
        let full = (tile0.map_or(0, |x| x.partial_content_left) / 255) as f64;
        t.s.actual_x_speed = ((t.s.actual_x_speed as f64) * (1.0f64 - (dt as f64) * 0.5f64 * full)) as f32;
    }

    // Step 7: remote control sanity
    let rider = t.rider();
    let rider_is_net = rider.as_ref().map_or(false, |r| r.is_net());
    let controlled = t.being_controlled();
    if !rider_is_net {
        let active = t.env.dw_active_blockhead();
        if !same_opt(&active, &rider) && controlled {
            t.s.target_x_speed = 0.0f32;
        }
    }
    if t.s.remote_stop_pos.y != -1 && wrap_dx(t.s.remote_stop_pos.x.wrapping_sub(pos.x), t.w) == 0 {
        t.s.target_x_speed = 0.0f32;
    }

    // Step 8: fallTimer
    let mut reset = false;
    if !t.s.falling || t.s.in_water {
        reset = true;
    } else if me.is_net() || controlled {
        if !(controlled && !rider_is_net) {
            reset = true;
        }
    }
    if !reset && t.s.y_speed >= 0.0f32 {
        reset = true;
    }
    if reset {
        t.s.fall_timer = 0.0f32;
    } else {
        t.s.fall_timer += cdt;
    }

    // Step 9: jump
    let can_act = !t.s.falling || t.d.flies();
    if can_act && t.s.jump_action_queued {
        t.s.actual_x_speed = t.s.target_x_speed;
        t.s.jump_action_queued = false;
        let mut can_jump = t.d.jumps() && !t.env.fast_forward() && pos.y < 0x3fc;
        if can_jump && !t.d.can_jump_multiple_tiles_while_flying() && t.s.has_had_extra_mid_air_jump && t.s.falling {
            can_jump = false;
        }
        if can_jump {
            t.s.y_speed = if controlled { 16.0f32 + fabsf(t.s.target_x_speed) } else { 9.0f32 };
            if t.s.falling {
                t.s.has_had_extra_mid_air_jump = true;
            }
        }
    }

    // Step 10: step-up / obstacle
    let rider_some = rider.is_some();
    let mut new_x_speed = t.s.actual_x_speed;
    if fabsf(t.s.target_x_speed) > 2.0f32 {
        let xo = if t.s.target_x_speed > 0.0f32 { 1 } else { -1 };
        let tt = t.tile(pos.x.wrapping_add(xo), pos.y);
        if !t_air_water_snow(&tt) || (!rider_some && t_gate(&tt)) {
            if t_solid(&tt) {
                let up = t.tile(pos.x.wrapping_add(xo), pos.y.wrapping_add(1));
                if t_air_water_snow(&up) && (rider_some || !t_gate(&up)) {
                    let m = if controlled { 1.0f32 } else { 2.0f32 };
                    new_x_speed = (if t.s.target_x_speed > 0.0f32 { 4.0f32 } else { -4.0f32 }) * m;
                    t.s.actual_x_speed = new_x_speed;
                    if !t.s.falling {
                        t.s.y_speed = if t_water(&tile0) { 16.0f32 } else { 12.0f32 };
                    }
                }
            }
        } else {
            new_x_speed = t.s.target_x_speed;
        }
    }
    if !t.s.falling || controlled {
        t.s.actual_x_speed = new_x_speed;
    } else {
        let f = min1(10.0f32 * dt);
        t.s.actual_x_speed = t.s.actual_x_speed + (new_x_speed - t.s.actual_x_speed) * f;
    }

    // Step 11: AI (skipped while controlled)
    if !controlled {
        // 11a: baby spawn / egg timer
        if !me.is_net() && ns.borrow().lay_timer > 0.0f32 {
            let lt = {
                let mut n = ns.borrow_mut();
                n.lay_timer -= dt;
                n.lay_timer
            };
            if lt <= 0.0f32 {
                let npc_type = t.npc.npc_type();
                let breed = t.d.generate_breed_for_child();
                let mut sd = new_mutable_dict();
                sd.map.insert("breed".to_string(), PlistValue::int(breed as i64));
                // RE-ENTRY: loadNPCAtPosition constructs another NPC
                t.flush();
                let ok = t.env.dw_load_npc_at_position(me.pos(), npc_type, Some(&sd), false, false, None);
                t.reload();
                if ok {
                    let mut n = ns.borrow_mut();
                    n.fullness = ((n.fullness as f64) + (-2700.0f64)) as f32;
                }
                {
                    let mut n = ns.borrow_mut();
                    n.has_bred = false;
                    n.lay_cooldown_timer = 3600.0f32;
                }
                dw_changed(me);
            }
        }
        // 11b: direction change
        if !me.is_net() {
            t.s.random_time_between_direction_changes -= dt;
            let skip = t.s.random_time_between_direction_changes > 0.0f32
                || (wrap_dx(t.s.goal_x.wrapping_sub(pos.x), t.w) != 0
                    && t.s.random_time_between_direction_changes >= -10.0f32);
            if !skip {
                let age = ns.borrow().age;
                let thr = if age >= 900.0f32 { 0.97f64 } else { 0.8f64 };
                if (r("DonkeyLike.ai.jump") as f64) > thr {
                    t.s.jump_action_queued = true;
                    t.s.jump_action_send_value = 2;
                    // the sound choice draw (sound itself is a nil message)
                    let _ = (r("DonkeyLike.ai.jumpSound") as f64) > 0.8f64;
                }
                if age >= 900.0f32 {
                    t.s.random_time_between_direction_changes = r("DonkeyLike.ai.interval") * 8.0f32 + 4.0f32;
                } else {
                    t.s.random_time_between_direction_changes = r("DonkeyLike.ai.interval") * 4.0f32 + 1.0f32;
                }
                t.s.goal_x = f32_to_i32((pos.x as f32) + (r("DonkeyLike.ai.goalX") - 0.5f32) * 10.0f32);
                if pos.x < t.s.goal_x {
                    t.s.target_x_speed = 3.5f32 * r("DonkeyLike.ai.speed");
                } else if t.s.goal_x < pos.x {
                    t.s.target_x_speed = -3.5f32 * r("DonkeyLike.ai.speed");
                } else {
                    t.s.target_x_speed = 0.0f32;
                }
                if fabsf(t.s.target_x_speed) < 2.0f32
                    && (t.env.dw_blockhead_occupies_tile(pos) || t.env.dw_npc_exists_at_pos(pos, me))
                    && wrap_dx(t.s.goal_x.wrapping_sub(pos.x), t.w) == 0
                {
                    t.s.goal_x = if glibc_rand::rand_tagged("DonkeyLike.ai.overlap") < 0x40000000 {
                        pos.x.wrapping_sub(4)
                    } else {
                        pos.x.wrapping_add(4)
                    };
                    let dg = wrap_dx(t.s.goal_x.wrapping_sub(pos.x), t.w);
                    if dg > 0 {
                        t.s.target_x_speed = 3.5f32;
                    } else if wrap_dx(t.s.goal_x.wrapping_sub(pos.x), t.w) < 0 {
                        t.s.target_x_speed = -3.5f32;
                    }
                }
                me.set_update_needs_to_be_sent(true);
                let not_moving = fabsf(t.s.target_x_speed) < 2.0f32;
                if not_moving {
                    let mut g = (((r("DonkeyLike.ai.goalRotation") as f64) * PI * 2.0f64 - PI) * 0.5f64) as f32;
                    if g > 0.1f32 {
                        let l = t.tile(pos.x.wrapping_sub(1), pos.y.wrapping_sub(1));
                        if !t_air_water_snow(&l) {
                            let ll = t.tile(pos.x.wrapping_sub(1), pos.y);
                            if t_solid(&ll) {
                                g = ((g as f64) * 0.2f64) as f32;
                            }
                        } else {
                            g = ((g as f64) * 0.2f64) as f32;
                        }
                    } else if g < -0.1f32 {
                        let l = t.tile(pos.x.wrapping_add(1), pos.y.wrapping_sub(1));
                        if !t_air_water_snow(&l) {
                            let ll = t.tile(pos.x.wrapping_add(1), pos.y);
                            if t_solid(&ll) {
                                g = ((g as f64) * 0.2f64) as f32;
                            }
                        } else {
                            g = ((g as f64) * 0.2f64) as f32;
                        }
                    }
                    t.s.random_goal_rotation = g;
                }
            }
        }
    }

    // Step 12: cliff / edge turning
    if !controlled && rider.is_none() && fabsf(t.s.target_x_speed) > 2.0f32 {
        if !me.is_net() && wrap_dx(t.s.goal_x.wrapping_sub(pos.x), t.w) == 0 {
            t.s.target_x_speed = 0.0f32;
            me.set_update_needs_to_be_sent(true);
            return; // skips step 13
        }
        let b = t.tile(pos.x, pos.y.wrapping_sub(1));
        if t_solid(&b) {
            let xo = if t.s.target_x_speed > 0.0f32 { 1 } else { -1 };
            let rt = t.tile(pos.x.wrapping_add(xo), pos.y);
            if t_air_water_snow(&rt) && !t_gate(&rt) {
                let rd = t.tile(pos.x.wrapping_add(xo), pos.y.wrapping_sub(1));
                if t_air_water_snow(&rd) && !t_gate(&rd) {
                    let rdd = t.tile(pos.x.wrapping_add(xo), pos.y.wrapping_sub(2));
                    if t_air_water_snow(&rdd) || t_water(&rd) {
                        let l = t.tile(pos.x.wrapping_sub(xo), pos.y);
                        let lu = t.tile(pos.x.wrapping_sub(xo), pos.y.wrapping_add(1));
                        let mut flip = false;
                        if !t_air_water_snow(&l) || !t_air_water_snow(&lu) {
                            let l2 = t.tile(pos.x.wrapping_sub(xo), pos.y.wrapping_add(2));
                            if t_air_water_snow(&l2) {
                                flip = true;
                            }
                        } else {
                            let ld = t.tile(pos.x.wrapping_sub(xo), pos.y.wrapping_sub(1));
                            if !t_air_water_snow(&ld) {
                                flip = true;
                            } else {
                                let ldd = t.tile(pos.x.wrapping_sub(xo), pos.y.wrapping_sub(2));
                                if !t_air_water_snow(&ldd) {
                                    flip = true;
                                }
                            }
                        }
                        if flip {
                            t.s.target_x_speed = -t.s.target_x_speed;
                            if !me.is_net() {
                                me.set_update_needs_to_be_sent(true);
                            }
                        }
                    }
                }
            }
        }
    }

    // Step 13: munching animation state (advanced on the server too)
    t.s.munch_motion_timer += dt * t.s.munch_motion_random_speed;
    if t.s.munch_motion_timer > 900.0f32 {
        t.s.munch_motion_timer -= 900.0f32;
    }
    t.s.munch_motion_random_speed_change_timer -= dt;
    if t.s.munch_motion_random_speed_change_timer <= 0.0f32 {
        t.s.munch_motion_random_speed_change_timer = r("DonkeyLike.munch.changeTimer");
        t.s.munch_motion_random_speed =
            0.5f32 * t.s.munch_motion_random_speed + 0.25f32 + r("DonkeyLike.munch.speed") * 0.75f32;
    }
    let below_not_grass = below.map_or(false, |b| b.type_index != 27 && b.type_index != 49);
    if fabsf(t.s.actual_x_speed) > 2.0f32 || t.s.falling || below_not_grass {
        t.s.target_head_down_fraction = 0.0f32;
    } else {
        t.s.munch_head_down_timer -= dt;
        if t.s.munch_head_down_timer < 0.0f32 {
            let was = t.s.target_head_down_fraction > 0.5f32;
            t.s.munch_head_down_timer = r("DonkeyLike.munch.headTimer") * (if was { 2.0f32 } else { 16.0f32 }) + 1.0f32;
            t.s.target_head_down_fraction = if was { 0.0f32 } else { 1.0f32 };
        }
    }
    let f = min1(10.0f32 * dt);
    t.s.head_down_fraction += (t.s.target_head_down_fraction - t.s.head_down_fraction) * f;
}
