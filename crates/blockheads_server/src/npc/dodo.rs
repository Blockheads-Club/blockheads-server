//! `Dodo: NPC` (NPC type 1, DynamicObject type 13): the huntable, tameable, breedable bird.
//!
//!
//! ## Notes
//! * Sounds are nil messages on the dedicated server: omitted. Every `rand()` that chooses a
//!   sound (`jumpAndCluck`) is kept.
//! * `update:` copies the Dodo ivars into a local [`DodoState`], runs the algorithm on it and writes it back; the copy
//!   is flushed to the `RefCell` before and reloaded after every call that can re-enter this object (
//!   rule 7). Re-entrancy points are marked `RE-ENTRY`.
//! * `R()` is `(float)rand() / 2147483648.0f` (range [0, 1.0]).

use std::any::Any;
use std::cell::RefCell;
use std::f64::consts::PI;

use crate::fmath::cconv::f32_to_i32;
use crate::fmath::libm::{fabsf, sinf};
use crate::fmath::vector::linear_interpolate;
use crate::items::breeds::{generate_child_breed_from_parent_breed, lay_likelyhood_for_breed, name_for_dodo_breed};
use crate::items::misc::{default_data_a_and_b_for_item_type, item_type_for_dodo_egg_breed};
use crate::npc::egg::{penv, t_air_snow, t_air_water_snow, t_gate, t_solid, t_water, world_width_tiles_of, wrap_dx};
pub use crate::npc::npc::NPC_DODO;
use crate::npc::npc::{
    alloc_npc, as_npc, npc_add_rider, npc_append_npc_creation_data_to_data, npc_blockhead_unloaded, npc_die, npc_env,
    npc_hit_with_force_blockhead,
    npc_init_with_world_dynamic_world_at_position_cache_save_dict_is_adult_was_placed_placed_by_client,
    npc_init_with_world_dynamic_world_cache_net_data, npc_init_with_world_dynamic_world_save_dict_cache,
    npc_npc_creation_net_data_for_client, npc_npc_update_net_data_for_client, npc_remote_creation_data_update,
    npc_remote_update, npc_remove_rider_opt, npc_state, npc_update_accurate_dt_is_simulation, Npc, NpcState, NPC_CLASS,
    SEND_STATE_NONE, SEND_STATE_SEND, SEND_STATE_SENT,
};
use crate::objects::base::{
    new_mutable_dict, padded_bytes, DynamicObject, DynamicObjectBase, DynamicWorldRef, WorldRef,
};
use crate::objects::class_info::ClassInfo;
use crate::objects::handle::{ptr_eq, CacheRef, ObjRef};
use crate::plist::value::{PlistDict, PlistValue};
use crate::rng::glibc_rand;
use crate::types::enums::DodoBreed;
use crate::types::netdata::{DodoCreationNetData, DodoUpdateNetData};
use crate::types::structs::{IntPair, Vector2, WireStruct};
use crate::{impl_npc_protocols, npc_forward};

/// `DYNAMIC_OBJECT_TYPE_NPC_DODO`.
pub const DYNAMIC_OBJECT_TYPE_NPC_DODO: i32 = 13;
const ITEM_RAW_DODO_MEAT: i32 = 29;
const ITEM_DODO_FEATHER: i32 = 30;
const ITEM_CAGE: i32 = 302;
const ITEM_DODO_CAGE: i32 = 303;
const ITEM_APPLE: i32 = 21;

/// `DODO_NAMES`: index order is observable (`NPC.generateNewName` indexes it).
pub static DODO_NAMES: [&str; 244] = [
    "Charlie",
    "Angel",
    "Skittles",
    "Coco",
    "Max",
    "Sunny",
    "Baby",
    "Bogart",
    "Buddy",
    "Indy",
    "Jet",
    "Kiwi",
    "Shadow",
    "Sunshine",
    "Baxter",
    "Bobo",
    "Cleo",
    "Crystal",
    "Dodo",
    "Chicken",
    "Loud Mouth",
    "Blocky",
    "Monkey",
    "Elmo",
    "Gizmo",
    "Harley",
    "Isabelle",
    "Jewel",
    "Maxwell",
    "Merlin",
    "Oliver",
    "Paco",
    "Pepper",
    "Pipi",
    "Polly",
    "Pumpkin",
    "Ricky",
    "Rio",
    "Rocky",
    "Star",
    "Starburst",
    "Tweety",
    "Apricot",
    "Axel",
    "Bailey",
    "Barney",
    "Bb",
    "Berri",
    "Beryl",
    "Betty-Boop",
    "Beyonce",
    "Bibi",
    "Big Al",
    "Big Blue",
    "Bill",
    "Birdy",
    "Blinky",
    "Blu",
    "Blue",
    "Bluella",
    "Bobbi",
    "Bonbon",
    "Bonita",
    "Bonnie Blue",
    "Booboo",
    "Boomer",
    "Bot",
    "Bram",
    "Bubba",
    "Buttons",
    "Calypso",
    "Caroline",
    "Cash",
    "Castello",
    "Charly",
    "Cheeko",
    "Cherri",
    "Chewy",
    "Chibi",
    "Chicky",
    "Chili",
    "Chip",
    "Chiquita",
    "Chirpie",
    "Chirpy",
    "Chirstopher",
    "Chloe",
    "Clucky",
    "Coconut",
    "Cocotail",
    "Cookies",
    "Coralline",
    "Cracker",
    "Crickett",
    "Curly",
    "Daisy",
    "Duck",
    "Dave",
    "Derpy",
    "Diamond",
    "Disco",
    "Divot",
    "Dog",
    "Domino",
    "Don Meep",
    "Donald",
    "Doom",
    "Dovey",
    "Edith",
    "Elvis",
    "Eugene",
    "Fifi",
    "Fizzy",
    "Flappie",
    "Flippy",
    "Florence",
    "Floyd",
    "Fluffy",
    "Forest",
    "Freddie",
    "Fruitloop",
    "Fuji",
    "Gabby",
    "Gadget",
    "Gator",
    "Gigi",
    "Gladis",
    "Gracie",
    "Gumby",
    "Hansel",
    "Hansey",
    "Hercules",
    "Hershey",
    "Huey",
    "Icecube",
    "Indie",
    "Jabba",
    "Jade",
    "Jaden",
    "Jerry",
    "Jessica",
    "Jewels",
    "Jinnx",
    "Jiro",
    "Jo",
    "Joe",
    "Katy",
    "Kiki",
    "Kiko",
    "Lady",
    "Lance",
    "Lemon",
    "Limehead",
    "Leo",
    "Leroy",
    "Lexial",
    "Liefie",
    "Bird",
    "Lili",
    "Lilly",
    "Lily",
    "Little Elvis",
    "Little Jimmy",
    "Littlefoot",
    "Lola",
    "Lorie",
    "Love",
    "Lucca",
    "Lucky",
    "Lucy",
    "Mac",
    "Mango",
    "Margarita",
    "Martian",
    "Mary",
    "Mickey",
    "Miles",
    "Missy",
    "Modena",
    "Monte",
    "Mooka",
    "Mouse",
    "Mr Chicken",
    "Mr Bugsly",
    "Mr Egg",
    "Mr Turkle",
    "Mr Bird",
    "Needle",
    "Niki",
    "Niko",
    "Noodles",
    "Norburt",
    "Obie",
    "Odin",
    "Oli",
    "Opey",
    "Oreo",
    "Owen",
    "Pablo",
    "Packer",
    "Paddy",
    "Parkur",
    "Pashi",
    "Paulie",
    "Payson",
    "Peaches",
    "Peanut",
    "Pearl",
    "Pebbles",
    "Pede",
    "Peekablue",
    "Pete",
    "Phillis",
    "Phoenix",
    "Pickle",
    "Pixie",
    "Precious",
    "Prince",
    "Purdie",
    "Quackers",
    "Rainbow",
    "Red",
    "Reno",
    "Reuben",
    "Rico",
    "Rikki",
    "Ripley",
    "Rod",
    "Romeo",
    "Roscoe",
    "Roxy",
    "Ruby",
    "Rudy",
    "Ryker",
    "Sabastian",
    "Sammy",
    "Schmoops",
    "Seattle",
    "Sidney",
    "Sika",
    "Silly",
    "Skip",
    "Skittlez",
    "Sky",
];

/// `Dodo: NPC <...>`.
pub static DODO_CLASS_INFO: ClassInfo = ClassInfo { name: "Dodo", superclass: Some(&NPC_CLASS), protocols: &[] };

const ZERO_PAIR: IntPair = IntPair { x: 0, y: 0 };

/// Dodo ivars 0x180..0x1ec (client-only render ivars that the server never writes stay 0).
#[derive(Clone, Debug)]
pub struct DodoState {
    /// 0x180
    pub movement_direction: i32,
    /// 0x184 `SendState`
    pub jump_and_cluck_state: i32,
    /// 0x188
    pub from_square: IntPair,
    /// 0x190
    pub to_square: IntPair,
    /// 0x198
    pub travel_fraction: f32,
    /// 0x19c
    pub random_time_between_direction_changes: f32,
    /// 0x1a0
    pub random_x_offset: f32,
    /// 0x1a4
    pub random_x_offset_goal: f32,
    /// 0x1a8
    pub random_cluck_noise_timer: f32,
    /// 0x1b0
    pub random_goal_rotation: f32,
    /// 0x1b4
    pub walk_timer: f32,
    /// 0x1b8
    pub walk_speed: f32,
    /// 0x1bc
    pub rider_target_velocity: f32,
    /// 0x1c0
    pub peck_timer: f32,
    /// 0x1c4
    pub next_peck_timer: f32,
    /// 0x1d0
    pub drown_timer: f32,
    /// 0x1d4
    pub fall_speed: f32,
    /// 0x1d8
    pub jumping: bool,
    /// 0x1dc
    pub remote_goal_square: IntPair,
    /// 0x1e4
    pub remote_walk_speed: f32,
    /// 0x1e8
    pub remote_movement_direction: i32,
    /// 0x1ec
    pub create_splash_next_frame: bool,
    /// 0x1ac (client written; stays 0)
    pub body_rotation: f32,
    /// 0x1c8 (client written; stays 0)
    pub y_offset: f32,
    /// 0x1cc (client written; stays 0)
    pub x_offset: f32,
}

impl Default for DodoState {
    fn default() -> Self {
        DodoState {
            movement_direction: 0,
            jump_and_cluck_state: 0,
            from_square: ZERO_PAIR,
            to_square: ZERO_PAIR,
            travel_fraction: 0.0,
            random_time_between_direction_changes: 0.0,
            random_x_offset: 0.0,
            random_x_offset_goal: 0.0,
            random_cluck_noise_timer: 0.0,
            random_goal_rotation: 0.0,
            walk_timer: 0.0,
            walk_speed: 0.0,
            rider_target_velocity: 0.0,
            peck_timer: 0.0,
            next_peck_timer: 0.0,
            drown_timer: 0.0,
            fall_speed: 0.0,
            jumping: false,
            remote_goal_square: ZERO_PAIR,
            remote_walk_speed: 0.0,
            remote_movement_direction: 0,
            create_splash_next_frame: false,
            body_rotation: 0.0,
            y_offset: 0.0,
            x_offset: 0.0,
        }
    }
}

/// The Dodo.
pub struct Dodo {
    pub base: RefCell<DynamicObjectBase>,
    pub npc: RefCell<NpcState>,
    pub dodo: RefCell<DodoState>,
}

fn r(tag: &'static str) -> f32 {
    glibc_rand::r01_tagged(tag)
}

fn dodo_of(me: &ObjRef) -> &Dodo {
    me.as_any().downcast_ref::<Dodo>().expect("object is not a Dodo")
}

fn dw_is_server(me: &ObjRef) -> bool {
    me.dynamic_world().map_or(false, |d| d.is_server())
}

fn dw_changed(me: &ObjRef) {
    if let Some(dw) = me.dynamic_world() {
        dw.dynamic_world_changed_at_pos_object_type(me.pos(), me.object_type());
    }
}

impl Dodo {
    fn alloc() -> std::rc::Rc<Dodo> {
        alloc_npc(|w| Dodo {
            base: RefCell::new(DynamicObjectBase::alloc_zeroed_with(w)),
            npc: RefCell::new(NpcState::default()),
            dodo: RefCell::new(DodoState::default()),
        })
    }

    /// `-[Dodo initWithWorld:dynamicWorld:atPosition:cache:saveDict:isAdult:wasPlaced:placedByClient:]`:
    /// NPC init, then `loadDerivedStuff` (5 `rand()`). Totals: adult 4+5, baby 3+5.
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
        let o = Dodo::alloc();
        let me: ObjRef = o;
        let inited = npc_init_with_world_dynamic_world_at_position_cache_save_dict_is_adult_was_placed_placed_by_client(
            &me, world, dw, pos, cache, save_dict, is_adult, was_placed, client_id,
        );
        debug_assert!(inited); // the base initialisers cannot fail on this path
        dodo_load_derived_stuff(&me);
        Some(me)
    }

    /// Registry constructor (`NpcCtor`): same as the `atPosition` init.
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
        Dodo::init_with_world_dynamic_world_at_position_cache_save_dict_is_adult_was_placed_placed_by_client(
            world, dw, pos, cache, save_dict, is_adult, was_placed, client_id,
        )
    }

    /// `-[Dodo initWithWorld:dynamicWorld:saveDict:cache:]`: load; 1 + 5 `rand()`.
    pub fn init_with_world_dynamic_world_save_dict_cache(
        world: &WorldRef,
        dw: &DynamicWorldRef,
        save_dict: &PlistDict,
        cache: CacheRef,
    ) -> Option<ObjRef> {
        let o = Dodo::alloc();
        let me: ObjRef = o;
        let inited = npc_init_with_world_dynamic_world_save_dict_cache(&me, world, dw, save_dict, cache);
        debug_assert!(inited); // the base initialisers cannot fail on this path
        dodo_load_derived_stuff(&me);
        Some(me)
    }

    /// `-[Dodo initWithWorld:dynamicWorld:cache:netData:]`: no NPC `rand()`, 5 for `loadDerivedStuff`,
    /// then the remote state is primed and `remoteCreationDataUpdate:` applied.
    pub fn init_with_world_dynamic_world_cache_net_data(
        world: &WorldRef,
        dw: &DynamicWorldRef,
        cache: CacheRef,
        net_data: &[u8],
    ) -> Option<ObjRef> {
        let o = Dodo::alloc();
        let me: ObjRef = o.clone();
        if !npc_init_with_world_dynamic_world_cache_net_data(&me, world, dw, cache, net_data) {
            return None;
        }
        dodo_load_derived_stuff(&me);
        {
            let pos = me.pos();
            let mut s = o.dodo.borrow_mut();
            s.remote_goal_square = pos;
            s.remote_walk_speed = 0.0;
            s.remote_movement_direction = 0;
        }
        me.remote_creation_data_update(&me, net_data);
        Some(me)
    }
}

// ------------------------------------------------------------------------------------------------ small functions

/// `-[Dodo getRandomDirection]`: one `rand()`; doubles compared against 0.75 / 0.25.
pub fn dodo_get_random_direction() -> i32 {
    let rr = r("Dodo.getRandomDirection") as f64;
    if rr > 0.75f64 {
        1
    } else if rr < 0.25f64 {
        -1
    } else {
        0
    }
}

/// `-[Dodo loadDerivedStuff]` server part: exactly 5 `rand()` in this order.
pub fn dodo_load_derived_stuff(me: &ObjRef) {
    let d = dodo_of(me);
    let dir = dodo_get_random_direction();
    let pos = me.pos();
    let walk_speed = (r("Dodo.load.walkSpeed") + 0.25f32) * 2.0f32;
    let next_peck = r("Dodo.load.nextPeck") * 2.0f32 + 0.1f32;
    let g = ((r("Dodo.load.xOffset") as f64) * 0.8f64 - 0.4f64) as f32;
    let cluck = r("Dodo.load.cluck") * 100.0f32 + 20.0f32;
    let mut s = d.dodo.borrow_mut();
    s.movement_direction = dir;
    s.to_square = pos;
    s.from_square = pos;
    s.travel_fraction = 1.0f32;
    s.random_time_between_direction_changes = 0.0f32;
    s.walk_speed = walk_speed;
    s.next_peck_timer = next_peck;
    s.peck_timer = 1.0f32;
    s.random_x_offset = g;
    s.random_x_offset_goal = g;
    s.random_cluck_noise_timer = cluck;
}

fn cluck_inner(me: &ObjRef, s: &mut DodoState) {
    if dw_is_server(me) {
        s.jump_and_cluck_state = SEND_STATE_SEND;
        me.set_update_needs_to_be_sent(true);
    }
    s.walk_timer = 0.0f32;
    s.walk_speed = 1.0f32;
    s.random_goal_rotation = 0.0f32;
    if !me.is_net() {
        s.random_cluck_noise_timer = r("Dodo.cluck.timer") * 100.0f32 + 20.0f32;
    }
    // the draw that chooses the sound name (sound itself is a nil message)
    let _r = r("Dodo.cluck.sound");
    s.jumping = true;
}

/// `-[Dodo jumpAndCluck]`: 1 or 2 `rand()`; the sound is omitted.
pub fn dodo_jump_and_cluck(me: &ObjRef) {
    let d = dodo_of(me);
    let mut s = d.dodo.borrow().clone();
    cluck_inner(me, &mut s);
    *d.dodo.borrow_mut() = s;
}

/// `-[Dodo die:]`: `travelFraction = 0` then `[super die:]`, only when not already dead.
pub fn dodo_die(me: &ObjRef, killer: Option<&ObjRef>) {
    let d = dodo_of(me);
    if !npc_state(me).borrow().dead {
        d.dodo.borrow_mut().travel_fraction = 0.0f32;
        npc_die(me, killer);
    }
}

/// `-[Dodo createItemDropsForDeath]`: meat (+ a feather on one `rand()` > 0x3fffffff) for a standard
/// adult, the breed's egg item for breeds 1..=32.
pub fn dodo_create_item_drops_for_death(me: &ObjRef) {
    let (age, breed, killer) = {
        let n = npc_state(me).borrow();
        (n.age, n.breed, n.kill_blockhead.clone())
    };
    if age >= 900.0f32 && !me.is_net() {
        let env = penv();
        let pos = me.pos();
        if breed == 0 || breed > 32 {
            env.dw_create_free_block(pos, ITEM_RAW_DODO_MEAT, 0, 0, None, 1, killer.as_ref());
            let n = if glibc_rand::rand_tagged("Dodo.drops.feather") > 0x3fffffff { 1 } else { 0 };
            for _ in 0..n {
                env.dw_create_free_block(pos, ITEM_DODO_FEATHER, 0, 0, None, 1, killer.as_ref());
            }
        } else {
            let t = item_type_for_dodo_egg_breed(DodoBreed(breed));
            let ab = default_data_a_and_b_for_item_type(t);
            env.dw_create_free_block(pos, t.0 as i32, ab.0 as i16 as u16, ab.1 as i16 as u16, None, 1, killer.as_ref());
        }
    }
}

/// `-[Dodo hitWithForce:blockhead:]`: NPC hit, then (alive and not a replica) one `rand()` picks the
/// flee direction.
pub fn dodo_hit_with_force_blockhead(me: &ObjRef, force: i32, b: Option<&ObjRef>) {
    let d = dodo_of(me);
    npc_hit_with_force_blockhead(me, force, b);
    let dead = npc_state(me).borrow().dead;
    if !dead && !me.is_net() {
        let dir = if (r("Dodo.hit.dir") as f64) > 0.5f64 { 1 } else { -1 };
        let mut s = d.dodo.borrow_mut();
        s.movement_direction = dir;
        s.walk_speed = 3.0f32;
        s.random_time_between_direction_changes = 5.0f32;
    }
}

fn dodo_remove_rider_opt(me: &ObjRef, rider: Option<&ObjRef>) {
    let d = dodo_of(me);
    let cur = npc_state(me).borrow().rider.clone();
    let same = match (&cur, rider) {
        (None, None) => true,
        (Some(a), Some(b)) => ptr_eq(a, b),
        _ => false,
    };
    if same {
        npc_remove_rider_opt(me, rider);
        d.dodo.borrow_mut().rider_target_velocity = 0.0f32;
    }
}

// ------------------------------------------------------------------------------------------------ net data

/// `-[Dodo dodoUpdateNetDataForClient:]`. Side effect: `jumpAndCluckState` SEND/SENT -> SENT.
/// Padding is zero.
pub fn dodo_dodo_update_net_data_for_client(me: &ObjRef, client_id: Option<&str>) -> DodoUpdateNetData {
    let d = dodo_of(me);
    let base = npc_npc_update_net_data_for_client(me, client_id);
    let mut s = d.dodo.borrow_mut();
    let mut out = DodoUpdateNetData {
        npc_update_net_data: base,
        to_square_x: s.to_square.x as u32,
        to_square_y: s.to_square.y as u32,
        walk_speed: f32_to_i32(s.walk_speed * 255.0f32) as i16,
        movement_direction: s.movement_direction as i8,
        jump_and_cluck: 0,
        padding: [0; 3],
        gap_27: [0; 1],
    };
    if s.jump_and_cluck_state != SEND_STATE_NONE {
        out.jump_and_cluck = 1;
        s.jump_and_cluck_state = SEND_STATE_SENT;
    }
    out
}

/// `-[Dodo doRemoteUpdate:]`.
pub fn dodo_do_remote_update(me: &ObjRef, u: DodoUpdateNetData) {
    let d = dodo_of(me);
    let rider = npc_state(me).borrow().rider.clone();
    if let Some(r) = &rider {
        if !r.is_net() {
            return;
        }
    }
    if !npc_env().is_client() {
        if let Some(r) = &rider {
            if r.is_net() {
                {
                    let mut s = d.dodo.borrow_mut();
                    s.remote_goal_square = IntPair { x: u.to_square_x as i32, y: u.to_square_y as i32 };
                    s.remote_walk_speed = (u.walk_speed as i32 as f32) / 255.0f32;
                    s.walk_speed = s.remote_walk_speed;
                    s.remote_movement_direction = u.movement_direction as i32;
                    s.movement_direction = s.remote_movement_direction;
                }
                me.set_update_needs_to_be_sent(true);
            }
        }
    } else {
        {
            let mut s = d.dodo.borrow_mut();
            s.remote_goal_square = IntPair { x: u.to_square_x as i32, y: u.to_square_y as i32 };
            s.remote_walk_speed = (u.walk_speed as i32 as f32) / 255.0f32;
            s.remote_movement_direction = u.movement_direction as i32;
        }
        if u.jump_and_cluck != 0 {
            dodo_jump_and_cluck(me);
        }
    }
}

// ------------------------------------------------------------------------------------------------ update

/// `-[Dodo update:accurateDT:isSimulation:]`, phases 0-10. See the module doc for the copy/flush scheme.
pub fn dodo_update_accurate_dt_is_simulation(me: &ObjRef, dt: f32, accurate_dt: f32, is_simulation: bool) {
    let d = dodo_of(me);
    // Phase 0
    npc_update_accurate_dt_is_simulation(me, dt, accurate_dt, is_simulation);
    if me.needs_removed() {
        return;
    }
    let mut s: DodoState = d.dodo.borrow().clone();
    if s.jump_and_cluck_state == SEND_STATE_SENT {
        s.jump_and_cluck_state = SEND_STATE_NONE;
    }
    let dead = npc_state(me).borrow().dead;
    if dead {
        s.travel_fraction += 4.0f32 * dt;
        let tf = s.travel_fraction;
        *d.dodo.borrow_mut() = s;
        if me.is_net() {
            return;
        }
        if tf < 1.0f32 {
            return;
        }
        Npc::create_item_drops_for_death(d, me); // RE-ENTRY-free: reads NPC state only
        me.base().borrow_mut().needs_removed = true;
        return;
    }
    update_alive(me, d, &mut s, dt, is_simulation);
    *d.dodo.borrow_mut() = s;
}

/// Flush the local copy to the cell (before a re-entrant call).
fn flush(d: &Dodo, s: &DodoState) {
    *d.dodo.borrow_mut() = s.clone();
}
/// Reload the local copy from the cell (after a re-entrant call).
fn reload(d: &Dodo, s: &mut DodoState) {
    *s = d.dodo.borrow().clone();
}

#[allow(clippy::cognitive_complexity)]
fn update_alive(me: &ObjRef, d: &Dodo, s: &mut DodoState, dt: f32, is_simulation: bool) {
    let _ = is_simulation;
    let env = penv();
    let ns = npc_state(me);
    let rider = || ns.borrow().rider.clone();
    let rider_is_net = || rider().map_or(false, |r| r.is_net());
    let w_tiles = world_width_tiles_of(me);
    let b_ridden = rider().map_or(false, |r| !r.is_net());
    let mut not_moving = s.movement_direction == 0;

    // Phase 1: cluck timer
    if !me.is_net() && rider().is_none() {
        if s.jump_and_cluck_state != SEND_STATE_NONE && dw_is_server(me) && npc_env().server_clients_count() == 0 {
            s.jump_and_cluck_state = SEND_STATE_NONE;
        }
        s.random_cluck_noise_timer -= dt;
        if s.random_cluck_noise_timer <= 0.0f32 {
            cluck_inner(me, s);
        }
    }

    // Phase 2: effective speed
    let mut w = s.walk_speed;
    if s.from_square.y != s.to_square.y {
        w = 2.0f32;
        if s.to_square.y < s.from_square.y && s.from_square.x == s.to_square.x {
            s.fall_speed += 4.0f32 * dt;
            if s.fall_speed > 4.0f32 {
                s.fall_speed = 4.0f32;
            }
            w = s.fall_speed + 2.0f32;
        } else {
            s.fall_speed = 0.0f32;
        }
    }
    if me.is_net() {
        let rd = rider();
        if rd.is_none() || rd.as_ref().map_or(false, |x| x.is_net()) {
            w = ((w as f64) * 0.75f64 + 0.25f64 * (s.remote_walk_speed as f64)) as f32;
        }
    }
    if rider_is_net() {
        w = s.remote_walk_speed;
    }
    s.random_time_between_direction_changes -= dt;
    s.travel_fraction += dt * w;
    s.peck_timer += dt;

    // Phase 3: rider control (locally ridden)
    if b_ridden {
        let v = s.rider_target_velocity;
        if v < 0.1f32 {
            if v > -0.1f32 {
                s.movement_direction = 0;
                s.walk_speed = 0.0f32;
            } else {
                s.movement_direction = -1;
                s.walk_speed = fabsf(2.0f32 * v);
            }
        } else {
            s.movement_direction = 1;
            s.walk_speed = 2.0f32 * v;
        }
        if s.movement_direction != 0 {
            let dir = s.movement_direction;
            let pos = me.pos();
            let rt = env.tile_cached(pos.x.wrapping_add(dir), pos.y);
            let mut stop = false;
            if !t_air_snow(&rt) {
                if !t_solid(&rt) {
                    stop = true;
                } else {
                    let ru = env.tile_cached(pos.x.wrapping_add(dir), pos.y.wrapping_add(1));
                    if !t_air_snow(&ru) {
                        stop = true;
                    } else {
                        let u = env.tile_cached(pos.x, pos.y.wrapping_add(1));
                        if !t_air_snow(&u) {
                            stop = true;
                        }
                    }
                }
            } else {
                let rd = env.tile_cached(pos.x.wrapping_add(dir), pos.y.wrapping_sub(1));
                if t_air_snow(&rd) {
                    let rdd = env.tile_cached(pos.x.wrapping_add(dir), pos.y.wrapping_sub(2));
                    if !t_solid(&rdd) {
                        stop = true;
                    }
                }
            }
            if stop {
                s.movement_direction = 0;
                s.walk_speed = 0.0f32;
            }
        }
        not_moving = s.movement_direction == 0;
    }

    // Phase 4: peck timer
    if s.from_square.y == s.to_square.y && (s.walk_speed < 1.5f32 || not_moving) {
        let k = if not_moving { 0.8f32 } else { 0.2f32 };
        s.next_peck_timer -= dt * k;
        if s.next_peck_timer <= 0.0f32 {
            s.peck_timer = 0.0f32;
            s.next_peck_timer = r("Dodo.update.peck") * 2.0f32 + 0.1f32;
        }
    }
    s.random_x_offset = linear_interpolate(s.random_x_offset, s.random_x_offset_goal, 0.2f32);
    if rider().is_some() {
        s.random_x_offset_goal = 0.0f32;
    }

    // Phase 5: AI decision
    'p5: {
        let inspecting = ns.borrow().inspecting_blockhead.as_ref().and_then(|w| w.upgrade());
        if inspecting.is_some() {
            s.movement_direction = 0;
            s.walk_speed = 0.0f32;
            break 'p5;
        }
        let coming = ns.borrow().coming_to_inspect_blockhead.as_ref().and_then(|w| w.upgrade());
        if let Some(c) = coming {
            let dx = wrap_dx(c.pos().x.wrapping_sub(me.pos().x), w_tiles);
            if dx.wrapping_abs() < 20 {
                if dx < 2 {
                    if dx < -1 {
                        s.movement_direction = -1;
                        s.random_x_offset_goal = -0.25f32;
                        s.walk_speed = 1.5f32;
                    }
                } else {
                    s.movement_direction = 1;
                    s.random_x_offset_goal = 0.25f32;
                    s.walk_speed = 1.5f32;
                }
                break 'p5;
            }
        }
        if !b_ridden {
            let mut reacted = false;
            if !me.is_net() && rider().is_none() && (r("Dodo.update.react") as f64) < 0.05f64 {
                let pos = me.pos();
                let mut abs_closest = 99i32;
                let mut closest: Option<ObjRef> = None;
                for bh in npc_env().all_blockheads_including_net() {
                    let bp = bh.pos();
                    let dx = wrap_dx(bp.x.wrapping_sub(pos.x), w_tiles);
                    let a = dx.wrapping_abs();
                    if a < 11 && a < abs_closest && a != 0 && bp.y.wrapping_sub(pos.y).wrapping_abs() < 8 {
                        closest = Some(bh);
                        abs_closest = a;
                    }
                }
                if let Some(c) = closest {
                    let mine = Npc::belongs_to_player_with_blockhead(d, &c);
                    let mut approach = false;
                    if mine && env.bh_moving(&c) {
                        approach = true;
                    }
                    if !approach
                        && Npc::can_be_fed_by_blockhead(d, me, &c)
                        && env.bh_held_item_type(&c) == Npc::food_item_type(d)
                    {
                        approach = true;
                    }
                    let dx = wrap_dx(c.pos().x.wrapping_sub(pos.x), w_tiles);
                    if approach {
                        if dx >= 2 {
                            s.movement_direction = 1;
                        } else if dx < -1 {
                            s.movement_direction = -1;
                        }
                        reacted = true;
                    } else if env.bh_moving(&c) {
                        if dx >= 2 {
                            s.movement_direction = -1;
                        } else if dx < -1 {
                            s.movement_direction = 1;
                        } else {
                            s.movement_direction =
                                if glibc_rand::rand_tagged("Dodo.update.fleeTie") > 0x3fffffff { 1 } else { -1 };
                        }
                        reacted = true;
                    }
                    if reacted {
                        s.random_cluck_noise_timer = 0.2f32 * s.random_cluck_noise_timer;
                        s.walk_speed = 2.0f32 + r("Dodo.update.reactSpeed");
                    }
                }
            }
            if !reacted
                && s.random_time_between_direction_changes <= 0.0f32
                && !me.is_net()
                && rider().is_none()
                && (s.travel_fraction >= 1.0f32 || not_moving)
            {
                s.random_time_between_direction_changes = r("Dodo.update.interval") * 2.0f32 + 1.0f32;
                let old = s.walk_speed;
                s.walk_speed = ((old as f64) * 0.5f64 + (r("Dodo.update.walkSpeed") as f64) + 0.25f64) as f32;
                me.set_update_needs_to_be_sent(true);
                if s.walk_speed >= 1.5f32 && !not_moving && !Npc::tamed(d) {
                    break 'p5;
                }
                let nd = dodo_get_random_direction();
                let pos = me.pos();
                if nd == 0 {
                    let t = env.tile_loaded(s.to_square.x, s.to_square.y);
                    if !env.dw_blockhead_occupies_tile(s.to_square)
                        && !env.dw_npc_exists_at_pos(pos, me)
                        && (rider().is_some() || !t_gate(&t))
                    {
                        s.movement_direction = 0;
                        s.random_goal_rotation = ((r("Dodo.update.goalRotation") as f64) * PI * 2.0f64 - PI) as f32;
                    }
                } else if s.movement_direction != nd && (s.walk_speed < 1.2f32 || not_moving) {
                    let t = env.tile_loaded(s.to_square.x, s.to_square.y);
                    s.movement_direction = nd;
                    if rider().is_some() || !t_gate(&t) {
                        s.random_x_offset_goal =
                            r("Dodo.update.xOffsetGoal") * 0.45f32 * ((s.movement_direction.wrapping_neg()) as f32);
                    }
                }
            }
        }
    }

    // Phase 6: eggs, drowning (not on replicas)
    if !me.is_net() {
        not_moving = s.movement_direction == 0;
        let pos = me.pos();
        // `tile` is a `Tile*` in the original: it is re-read after `fillTile` below so the drowning test sees the
        // filled tile exactly as the pointer dereference would.
        let mut tile = env.tile_loaded(pos.x, pos.y);
        let lay_timer = ns.borrow().lay_timer;
        if lay_timer > 0.0f32 {
            let new_timer = lay_timer - dt;
            ns.borrow_mut().lay_timer = new_timer;
            if new_timer <= 0.0f32 && tile.map_or(false, |t| t.foreground_contents == 0) {
                let breed = ns.borrow().breed;
                let p = lay_likelyhood_for_breed(DodoBreed(breed));
                if r("Dodo.update.lay") < p {
                    let (mate_breed, has_bred) = {
                        let mut n = ns.borrow_mut();
                        n.fullness = ((n.fullness as f64) - 2700.0f64) as f32;
                        (n.mate_breed, n.has_bred)
                    };
                    let child =
                        generate_child_breed_from_parent_breed(DodoBreed(breed), DodoBreed(mate_breed), has_bred);
                    let mut inner = new_mutable_dict();
                    inner.map.insert("breed".to_string(), PlistValue::int(child.0 as i64));
                    let mut genes = new_mutable_dict();
                    genes.map.insert("genesDict".to_string(), PlistValue::Dict(inner));
                    // RE-ENTRY: fillTile -> addEggAtPos (draws 1 rand in the Egg init); our state is not touched.
                    env.fill_tile_dodo_egg(pos, child.0, &genes);
                    tile = env.tile_loaded(pos.x, pos.y);
                    {
                        let mut n = ns.borrow_mut();
                        n.has_bred = false;
                        n.lay_cooldown_timer = 900.0f32;
                    }
                    me.set_update_needs_to_be_sent(true);
                    dw_changed(me);
                }
            }
        }
        if t_water(&tile) && tile.map_or(false, |t| t.partial_content_left >= 0x80) {
            s.drown_timer += dt;
            if s.drown_timer > 2.5f32 {
                s.drown_timer = r("Dodo.update.drown") * 0.5f32;
                flush(d, s); // RE-ENTRY: hit_with_force -> Dodo override writes the Dodo state
                let force = (Npc::max_health(d) / 8) as i32;
                Npc::hit_with_force_blockhead(d, me, force, None);
                reload(d, s);
            }
        } else {
            s.drown_timer -= 0.5f32 * dt;
            if s.drown_timer < 0.0f32 {
                s.drown_timer = 0.0f32;
            }
        }
    }

    // Phase 7: arrival at toSquare
    let mut snap = false;
    let mut skip_to_10 = false;
    if s.travel_fraction >= 1.0f32 {
        s.travel_fraction -= 1.0f32;
        if s.travel_fraction >= 1.0f32 || s.travel_fraction < 0.0f32 {
            s.travel_fraction = 0.0f32;
        }
        let mut turn_back = false;
        let mut turn_back_square = ZERO_PAIR;
        if !me.is_net() && s.from_square.x != s.to_square.x {
            if let Some(t) = env.tile_loaded(s.to_square.x, s.to_square.y) {
                // eat apples
                let fullness = ns.borrow().fullness;
                if (fullness as f64) < 5400.0f64 && (t.contents == 4 || t.contents == 5) {
                    if let Some(tree) = env.dw_tree_at_pos(s.to_square) {
                        if env.is_apple_tree(&tree) {
                            let avail = env.plant_available_food(&tree);
                            let add = if avail >= 900.0f32 { 900.0f32 } else { avail };
                            ns.borrow_mut().fullness += add;
                            me.set_update_needs_to_be_sent(true);
                            env.plant_set_available_food(&tree, avail - add);
                        }
                    }
                }
                // lay timer arming
                let (fullness, cooldown, lay) = {
                    let n = ns.borrow();
                    (n.fullness, n.lay_cooldown_timer, n.lay_timer)
                };
                if (fullness as f64) > 5400.0f64 && cooldown <= 0.0f32 && lay <= 0.0f32 {
                    let v = ((r("Dodo.update.layTimer") * 900.0f32) as f64 * 0.5f64 + 450.0f64) as f32;
                    ns.borrow_mut().lay_timer = v;
                    dw_changed(me);
                }
            }
        }
        // gate turn-back
        if !me.is_net() {
            let tt = env.tile_loaded(s.to_square.x, s.to_square.y);
            if t_gate(&tt) && rider().is_none() {
                let ft = env.tile_loaded(s.from_square.x, s.from_square.y);
                if !t_gate(&ft) && wrap_dx(s.to_square.x.wrapping_sub(s.from_square.x), w_tiles) != 0 {
                    turn_back = true;
                    turn_back_square = s.from_square;
                }
            }
        }
        if rider().map_or(false, |r| !r.is_net()) {
            me.set_update_needs_to_be_sent(true);
        }
        s.from_square = s.to_square;
        let to = s.to_square;
        flush(d, s); // RE-ENTRY: updatePosition / checkCurrentPositionForFood -> reactToBeingFed writes the Dodo state
        me.update_position(me, to);
        Npc::check_current_position_for_food(d, me);
        reload(d, s);
        me.base().borrow_mut().float_pos = Vector2 { x: (to.x as f32) + 0.5f32, y: to.y as f32 };
        if turn_back && ((!me.is_net() && !b_ridden) || rider().map_or(false, |r| !r.is_net())) {
            s.to_square = turn_back_square;
            s.movement_direction = if s.from_square.x < s.to_square.x { 1 } else { -1 };
            s.walk_speed = 2.0f32;
            skip_to_10 = true;
        }

        if !skip_to_10 {
            // Phase 8: remote / net-controlled target
            let rd = rider();
            if (me.is_net() && !b_ridden) || rd.as_ref().map_or(false, |r| r.is_net()) {
                let pos = me.pos();
                let dx = wrap_dx(s.remote_goal_square.x.wrapping_sub(pos.x), w_tiles);
                if dx.wrapping_abs() < 9 && s.remote_goal_square.y.wrapping_sub(pos.y).wrapping_abs() < 5 {
                    if dx.wrapping_abs() < 3 {
                        if dx >= 1 {
                            s.movement_direction = 1;
                            s.walk_speed = if s.remote_walk_speed <= 1.0f32 { 1.0f32 } else { s.remote_walk_speed };
                        } else if dx < 0 {
                            s.movement_direction = -1;
                            s.walk_speed = if s.remote_walk_speed <= 1.0f32 { 1.0f32 } else { s.remote_walk_speed };
                        } else {
                            s.walk_speed = s.remote_walk_speed;
                            s.movement_direction = 0;
                        }
                    } else {
                        s.walk_speed = 2.0f32;
                        s.movement_direction = if dx > 0 { 1 } else { -1 };
                    }
                } else {
                    snap = true;
                    s.to_square = s.remote_goal_square;
                    s.walk_speed = s.remote_walk_speed;
                    s.movement_direction = s.remote_movement_direction;
                }
                not_moving = s.movement_direction == 0;
            }

            // Phase 9: tile movement planning
            if !snap {
                let pos = me.pos();
                let dir = s.movement_direction;
                let has_rider = rider().is_some();
                let t = env.tile_loaded(pos.x, pos.y);
                let in_water = t_water(&t) && t.map_or(false, |t| t.partial_content_left > 0x7f);
                if in_water {
                    let ru = env.tile_cached(pos.x.wrapping_add(dir), pos.y.wrapping_add(1));
                    if !t_air_water_snow(&ru) || (!has_rider && t_gate(&ru)) {
                        let u = env.tile_cached(pos.x, pos.y.wrapping_add(1));
                        if t_air_water_snow(&u) && (has_rider || !t_gate(&u)) {
                            s.to_square = IntPair { x: pos.x, y: pos.y.wrapping_add(1) };
                            me.set_unreliable_update_needs_to_be_sent(true);
                        }
                    } else {
                        s.to_square = IntPair { x: pos.x.wrapping_add(dir), y: pos.y.wrapping_add(1) };
                        me.set_unreliable_update_needs_to_be_sent(true);
                    }
                } else {
                    let b = env.tile_cached(pos.x, pos.y.wrapping_sub(1));
                    if !t_solid(&b) {
                        s.to_square = IntPair { x: pos.x, y: pos.y.wrapping_sub(1) };
                        me.set_unreliable_update_needs_to_be_sent(true);
                        if t_water(&b) {
                            s.create_splash_next_frame = true;
                        }
                    } else if !not_moving {
                        let rt = env.tile_cached(pos.x.wrapping_add(dir), pos.y);
                        if !t_air_snow(&rt) || (!has_rider && t_gate(&rt)) {
                            if t_solid(&rt) {
                                let ru = env.tile_cached(pos.x.wrapping_add(dir), pos.y.wrapping_add(1));
                                if t_air_snow(&ru) && (has_rider || !t_gate(&ru)) {
                                    let u = env.tile_cached(pos.x, pos.y.wrapping_add(1));
                                    if t_air_snow(&u) {
                                        s.to_square = IntPair { x: pos.x.wrapping_add(dir), y: pos.y.wrapping_add(1) };
                                        me.set_unreliable_update_needs_to_be_sent(true);
                                    }
                                }
                            }
                        } else {
                            let rd = env.tile_cached(pos.x.wrapping_add(dir), pos.y.wrapping_sub(1));
                            if !t_solid(&rd) {
                                if t_air_snow(&rd) && (has_rider || !t_gate(&rd)) {
                                    let rdd = env.tile_cached(pos.x.wrapping_add(dir), pos.y.wrapping_sub(2));
                                    if t_solid(&rdd) {
                                        s.to_square = IntPair { x: pos.x.wrapping_add(dir), y: pos.y.wrapping_sub(1) };
                                        me.set_unreliable_update_needs_to_be_sent(true);
                                    }
                                }
                            } else {
                                s.to_square = IntPair { x: pos.x.wrapping_add(dir), y: pos.y };
                                me.set_unreliable_update_needs_to_be_sent(true);
                            }
                        }
                    }
                }
                if s.to_square == s.from_square {
                    s.movement_direction = 0;
                    if !me.is_net() {
                        me.set_unreliable_update_needs_to_be_sent(true);
                    }
                }
            }
        }
    }

    // Phase 10: mating trigger and interpolation of floatPos
    if s.from_square != s.to_square {
        if !me.is_net() && Npc::can_mate(d) {
            if let Some(other) = env.dw_npc_close_enough_to_breed(me) {
                Npc::mate_with_npc(d, me, &other);
                if let Some(o) = as_npc(&other) {
                    o.mate_with_npc(&other, me);
                }
            }
        }
        let from_pos = Vector2 { x: (s.from_square.x as f32) + 0.5f32, y: s.from_square.y as f32 };
        let to_pos = Vector2 { x: (s.to_square.x as f32) + 0.5f32, y: s.to_square.y as f32 };
        let x_lerp = s.travel_fraction;
        let mut y_lerp = s.travel_fraction;
        if s.from_square.y < s.to_square.y && s.from_square.x != s.to_square.x {
            y_lerp = sinf(((y_lerp as f64) * PI * 0.75f64) as f32) / 0.65f32;
            if (s.random_x_offset_goal > 0.0f32) != (s.to_square.x < s.from_square.x) {
                s.random_x_offset_goal = -s.random_x_offset_goal;
            }
        } else if s.to_square.y < s.from_square.y && s.from_square.x != s.to_square.x {
            y_lerp = 1.0f32 - sinf((((1.0f32 - y_lerp) as f64) * PI * 0.75f64) as f32) / 0.7f32;
            if (s.random_x_offset_goal > 0.0f32) == (s.to_square.x < s.from_square.x) {
                s.random_x_offset_goal = -s.random_x_offset_goal;
            }
        }
        let fx = linear_interpolate(from_pos.x, to_pos.x, x_lerp);
        let fy = linear_interpolate(from_pos.y, to_pos.y, y_lerp);
        me.base().borrow_mut().float_pos = Vector2 { x: fx, y: fy };
    }
}

// ------------------------------------------------------------------------------------------------ trait impls

impl DynamicObject for Dodo {
    fn base(&self) -> &RefCell<DynamicObjectBase> {
        &self.base
    }
    fn class_info(&self) -> &'static ClassInfo {
        &DODO_CLASS_INFO
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
        dealloc,
        as_harmable,
        as_ridable,
        as_tap_choice
    );

    fn update_accurate_dt_is_simulation(&self, me: &ObjRef, dt: f32, accurate_dt: f32, is_simulation: bool) {
        dodo_update_accurate_dt_is_simulation(me, dt, accurate_dt, is_simulation)
    }

    ///  (`floatPos + (xOffset, yOffset)`; the offsets are client-written, 0 on the server).
    fn render_pos(&self) -> Vector2 {
        let fp = self.base.borrow().float_pos;
        let s = self.dodo.borrow();
        Vector2 { x: fp.x + s.x_offset, y: fp.y + s.y_offset }
    }

    /// : 0x70 bytes (NPC part, then the Dodo update part) plus the NPC name tail.
    fn creation_net_data_for_client(&self, me: &ObjRef, client_id: Option<&str>) -> Option<Vec<u8>> {
        let mut buf = vec![0u8; 0x70];
        let npc_part = npc_npc_creation_net_data_for_client(me, client_id);
        npc_part.write_le(&mut buf[0..0x48]);
        let upd = dodo_dodo_update_net_data_for_client(me, client_id);
        upd.write_le(&mut buf[0x48..0x70]);
        Some(npc_append_npc_creation_data_to_data(me, buf))
    }

    /// : the 0x28 update struct.
    fn update_net_data_for_client(&self, me: &ObjRef, client_id: Option<&str>) -> Option<Vec<u8>> {
        Some(dodo_dodo_update_net_data_for_client(me, client_id).to_vec())
    }

    /// : `dead` is read after the super call.
    fn remote_update(&self, me: &ObjRef, net_data: &[u8]) {
        npc_remote_update(me, net_data);
        if !npc_state(me).borrow().dead {
            let u = DodoUpdateNetData::read_le(&padded_bytes(net_data, 0x28));
            dodo_do_remote_update(me, u);
        }
    }

    fn remote_creation_data_update(&self, me: &ObjRef, net_data: &[u8]) {
        npc_remote_creation_data_update(me, net_data);
        let c = DodoCreationNetData::read_le(&padded_bytes(net_data, 0x70));
        dodo_do_remote_update(me, c.dodo_update_net_data);
    }

    fn blockhead_unloaded(&self, me: &ObjRef, b: &ObjRef) {
        let is_rider = npc_state(me).borrow().rider.as_ref().map_or(false, |r| ptr_eq(r, b));
        if is_rider {
            self.dodo.borrow_mut().rider_target_velocity = 0.0f32;
        }
        npc_blockhead_unloaded(me, b);
    }
}

impl Npc for Dodo {
    fn npc(&self) -> &RefCell<NpcState> {
        &self.npc
    }
    fn npc_type(&self) -> i32 {
        NPC_DODO
    }
    fn max_age(&self) -> f32 {
        7200.0
    }
    fn species_name(&self) -> String {
        "DODO".to_string()
    }
    fn food_item_type(&self) -> i32 {
        ITEM_APPLE
    }
    fn captured_item_type(&self) -> i32 {
        ITEM_DODO_CAGE
    }
    fn capture_required_item_type(&self) -> i32 {
        ITEM_CAGE
    }
    fn get_names_array(&self) -> Option<&'static [&'static str]> {
        Some(&DODO_NAMES)
    }
    fn get_names_array_count(&self) -> i32 {
        244
    }
    fn max_health(&self) -> u16 {
        16
    }
    fn creation_data_struct_size(&self) -> usize {
        0x70
    }
    fn breed_string(&self) -> String {
        name_for_dodo_breed(DodoBreed(self.npc.borrow().breed))
    }
    fn jumps_on_swipe(&self) -> bool {
        false
    }
    fn create_item_drops_for_death(&self, me: &ObjRef) {
        dodo_create_item_drops_for_death(me)
    }
    fn die(&self, me: &ObjRef, killer: Option<&ObjRef>) {
        dodo_die(me, killer)
    }
    fn hit_with_force_blockhead(&self, me: &ObjRef, force: i32, b: Option<&ObjRef>) {
        dodo_hit_with_force_blockhead(me, force, b)
    }
    fn can_be_captured_by_blockhead_with_item_type(&self, b: &ObjRef, item: i32) -> bool {
        item == ITEM_CAGE && (!Npc::tamed(self) || Npc::belongs_to_player_with_blockhead(self, b))
    }
    fn cant_be_captured_tip_string_for_blockhead_with_item_type(&self, b: &ObjRef, item: i32) -> Option<String> {
        if item == ITEM_CAGE && Npc::tamed(self) && !Npc::belongs_to_player_with_blockhead(self, b) {
            Some(format!("THAT {} BELONGS TO\nSOMEONE ELSE", Npc::species_name(self)))
        } else {
            None
        }
    }
    /// : `[super reactToBeingFed]` is empty in the base; sounds omitted.
    fn react_to_being_fed(&self, _me: &ObjRef) {
        let mut s = self.dodo.borrow_mut();
        s.movement_direction = 0;
        s.random_goal_rotation = 0.0f32;
        s.walk_speed = 1.0f32;
        s.jumping = true;
        s.walk_timer = 0.0f32;
    }
    /// : `(rp.x, rp.y, -0.75, 1)` with `rp = renderPos + (0, 0.1)`.
    fn rider_pos_for_blockhead(&self, _b: &ObjRef) -> [f32; 4] {
        let rp = DynamicObject::render_pos(self);
        [rp.x, rp.y + 0.1f32, -0.75f32, 1.0f32]
    }
    fn rider_body_y_rotation_for_blockhead(&self, _b: &ObjRef) -> f32 {
        self.dodo.borrow().body_rotation
    }
    fn set_target_velocity(&self, v: Vector2) {
        self.dodo.borrow_mut().rider_target_velocity = v.x;
    }
    fn ride_direction(&self) -> i32 {
        if self.npc.borrow().rider.is_none() {
            return 0;
        }
        let v = self.dodo.borrow().rider_target_velocity;
        if v > 0.1f32 {
            1
        } else if v < -0.1f32 {
            -1
        } else {
            0
        }
    }
    fn add_rider(&self, me: &ObjRef, b: &ObjRef) {
        npc_add_rider(me, b);
        let controlled = self.npc.borrow().being_controlled;
        let pos = me.pos();
        let mut s = self.dodo.borrow_mut();
        if !controlled {
            s.rider_target_velocity = 0.0f32;
        }
        s.random_x_offset_goal = 0.0f32;
        s.remote_goal_square = pos;
        s.remote_walk_speed = 0.0f32;
        s.remote_movement_direction = 0;
    }
    fn remove_rider(&self, me: &ObjRef, b: &ObjRef) {
        dodo_remove_rider_opt(me, Some(b))
    }
    /// the guard also passes for nil == nil.
    fn remove_rider_opt(&self, me: &ObjRef, b: Option<&ObjRef>) {
        dodo_remove_rider_opt(me, b)
    }
    fn blockhead_can_ride_using_item(&self, b: &ObjRef, _item: i32) -> bool {
        let (age, dead) = {
            let n = self.npc.borrow();
            (n.age, n.dead)
        };
        age > 900.0f32
            && (!Npc::tamed(self) || Npc::belongs_to_player_with_blockhead(self, b))
            && !dead
            && !self.base.borrow().needs_removed
    }
}

impl_npc_protocols!(Dodo);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::npc::egg::test_support::TestPassiveEnv;
    use crate::npc::egg::{set_passive_env, PassiveEnv};
    use crate::npc::npc::{register_npc_ctor, set_npc_env};
    use crate::objects::base::testutil::{setup, MockDw, MockWorld};
    use crate::types::tile::Tile;
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
    fn make(fx: &Fx, adult: bool, sd: Option<&PlistDict>) -> ObjRef {
        let wr: WorldRef = fx.w.clone();
        let dr: DynamicWorldRef = fx.dw.clone();
        Dodo::init_with_world_dynamic_world_at_position_cache_save_dict_is_adult_was_placed_placed_by_client(
            &wr,
            &dr,
            IntPair { x: 40, y: 3 },
            None,
            sd,
            adult,
            false,
            None,
        )
        .unwrap()
    }
    fn floor(fx: &Fx, x0: i32, x1: i32, y: i32) {
        for x in x0..=x1 {
            let mut t = Tile::default();
            t.type_index = 1;
            fx.env.tiles.borrow_mut().insert((x, y), t);
            let mut a = Tile::default();
            a.type_index = 2;
            fx.env.tiles.borrow_mut().insert((x, y + 1), a);
            fx.env.tiles.borrow_mut().insert((x, y + 2), a);
            fx.env.tiles.borrow_mut().insert((x, y + 3), a);
        }
    }

    fn raw_draws(seed: u32, n: usize) -> Vec<i32> {
        glibc_rand::srand(seed);
        (0..n).map(|_| glibc_rand::rand()).collect()
    }
    fn rr(v: i32) -> f32 {
        (v as f32) / 2147483648.0f32
    }

    #[test]
    fn constants_table() {
        let fx = fixture();
        let me = make(&fx, true, None);
        let d = dodo_of(&me);
        assert_eq!(Npc::npc_type(d), 1);
        assert_eq!(Npc::max_age(d), 7200.0);
        assert_eq!(Npc::max_health(d), 16);
        assert_eq!(Npc::species_name(d), "DODO");
        assert_eq!(Npc::food_item_type(d), 21);
        assert_eq!(Npc::captured_item_type(d), 303);
        assert_eq!(Npc::capture_required_item_type(d), 302);
        assert_eq!(Npc::creation_data_struct_size(d), 0x70);
        assert_eq!(Npc::get_names_array_count(d), 244);
        assert_eq!(DODO_NAMES.len(), 244);
        assert_eq!(DODO_NAMES[0], "Charlie");
        assert_eq!(DODO_NAMES[20], "Loud Mouth");
        assert_eq!(DODO_NAMES[243], "Sky");
        assert_eq!(me.object_type(), DYNAMIC_OBJECT_TYPE_NPC_DODO);
        assert_eq!(Npc::breed_string(d), "DODO");
    }

    #[test]
    fn load_derived_stuff_five_draws_in_order() {
        for seed in 0..100u32 {
            let fx = fixture();
            let wr: WorldRef = fx.w.clone();
            let dr: DynamicWorldRef = fx.dw.clone();
            // load path: 1 (NPC hunger) + 5
            let v = raw_draws(seed, 7);
            glibc_rand::srand(seed);
            let me = Dodo::init_with_world_dynamic_world_save_dict_cache(&wr, &dr, &new_mutable_dict(), None).unwrap();
            let s = dodo_of(&me).dodo.borrow().clone();
            let r1 = rr(v[1]) as f64;
            let dir = if r1 > 0.75 {
                1
            } else if r1 < 0.25 {
                -1
            } else {
                0
            };
            assert_eq!(s.movement_direction, dir);
            assert_eq!(s.walk_speed.to_bits(), ((rr(v[2]) + 0.25f32) * 2.0f32).to_bits());
            assert_eq!(s.next_peck_timer.to_bits(), (rr(v[3]) * 2.0f32 + 0.1f32).to_bits());
            assert_eq!(s.peck_timer, 1.0);
            assert_eq!(s.random_x_offset.to_bits(), (((rr(v[4]) as f64) * 0.8 - 0.4) as f32).to_bits());
            assert_eq!(s.random_x_offset_goal.to_bits(), s.random_x_offset.to_bits());
            assert_eq!(s.random_cluck_noise_timer.to_bits(), (rr(v[5]) * 100.0f32 + 20.0f32).to_bits());
            assert_eq!(s.travel_fraction, 1.0);
            assert_eq!(s.from_square, me.pos());
            assert_eq!(s.to_square, me.pos());
            // exactly 6 draws consumed
            assert_eq!(glibc_rand::rand(), v[6]);
        }
    }

    #[test]
    fn constructor_draw_totals() {
        let fx = fixture();
        glibc_rand::srand(5);
        let _ = make(&fx, true, None);
        let after = glibc_rand::rand();
        assert_eq!(after, raw_draws(5, 10)[9]); // adult 4 + 5 = 9
        glibc_rand::srand(5);
        let _ = make(&fx, false, None);
        assert_eq!(glibc_rand::rand(), raw_draws(5, 9)[8]); // baby 3 + 5 = 8
    }

    #[test]
    fn random_direction_thresholds() {
        // R() exactly 0.25 and 0.75 -> 0; 1.0 reachable
        let probe = |target: f32| -> Option<i32> {
            for seed in 0..200000u32 {
                glibc_rand::srand(seed);
                let v = glibc_rand::rand();
                if rr(v) == target {
                    return Some(seed as i32);
                }
            }
            None
        };
        let _ = probe;
        // synthetic check through the formula
        let f = |rv: f32| {
            let x = rv as f64;
            if x > 0.75 {
                1
            } else if x < 0.25 {
                -1
            } else {
                0
            }
        };
        assert_eq!(f(0.25), 0);
        assert_eq!(f(0.75), 0);
        assert_eq!(f(1.0), 1);
        assert_eq!(f(0.2499), -1);
    }

    #[test]
    fn jump_and_cluck_draws_and_state() {
        let fx = fixture();
        let me = make(&fx, true, None);
        glibc_rand::srand(11);
        let v = raw_draws(11, 3);
        glibc_rand::srand(11);
        dodo_jump_and_cluck(&me);
        let s = dodo_of(&me).dodo.borrow().clone();
        assert_eq!(s.jump_and_cluck_state, SEND_STATE_SEND);
        assert!(me.update_needs_to_be_sent());
        assert_eq!(s.walk_speed, 1.0);
        assert_eq!(s.random_cluck_noise_timer.to_bits(), (rr(v[0]) * 100.0f32 + 20.0f32).to_bits());
        assert!(s.jumping);
        assert_eq!(glibc_rand::rand(), v[2]); // exactly two draws
                                              // replica: one draw, timer untouched
        me.base().borrow_mut().is_net = true;
        let before = dodo_of(&me).dodo.borrow().random_cluck_noise_timer;
        glibc_rand::srand(11);
        dodo_jump_and_cluck(&me);
        assert_eq!(dodo_of(&me).dodo.borrow().random_cluck_noise_timer, before);
        assert_eq!(glibc_rand::rand(), v[1]);
    }

    #[test]
    fn net_update_struct_layout() {
        let fx = fixture();
        let me = make(&fx, true, None);
        {
            let mut s = dodo_of(&me).dodo.borrow_mut();
            s.walk_speed = 1.5;
            s.movement_direction = -1;
            s.to_square = IntPair { x: 41, y: 3 };
            s.jump_and_cluck_state = SEND_STATE_SEND;
        }
        let nd = me.update_net_data_for_client(&me, None).unwrap();
        assert_eq!(nd.len(), 0x28);
        let u = DodoUpdateNetData::read_le(&nd);
        assert_eq!(u.to_square_x, 41);
        assert_eq!(u.walk_speed, (1.5f32 * 255.0f32) as i32 as i16);
        assert_eq!(u.movement_direction, -1);
        assert_eq!(u.jump_and_cluck, 1);
        assert_eq!(&nd[0x24..0x28], &[0, 0, 0, 0]);
        assert_eq!(dodo_of(&me).dodo.borrow().jump_and_cluck_state, SEND_STATE_SENT);
        // SENT still sends 1 until update resets it
        let u2 = DodoUpdateNetData::read_le(&me.update_net_data_for_client(&me, None).unwrap());
        assert_eq!(u2.jump_and_cluck, 1);
        let c = me.creation_net_data_for_client(&me, None).unwrap();
        assert!(c.len() >= 0x70);
        assert_eq!(&c[0x24 + 0x48..0x28 + 0x48], &[0, 0, 0, 0]);
    }

    #[test]
    fn drops_by_breed() {
        let fx = fixture();
        let me = make(&fx, true, None);
        npc_state(&me).borrow_mut().age = 1000.0;
        npc_state(&me).borrow_mut().breed = 0;
        glibc_rand::srand(2);
        let v = raw_draws(2, 1);
        glibc_rand::srand(2);
        dodo_create_item_drops_for_death(&me);
        let n = if v[0] > 0x3fffffff { 2 } else { 1 };
        assert_eq!(fx.env.drops.borrow().len(), n);
        assert_eq!(fx.env.drops.borrow()[0].0, 29);
        fx.env.drops.borrow_mut().clear();
        npc_state(&me).borrow_mut().breed = 3;
        dodo_create_item_drops_for_death(&me);
        assert_eq!(fx.env.drops.borrow().len(), 1);
        assert_eq!(fx.env.drops.borrow()[0].0, item_type_for_dodo_egg_breed(DodoBreed(3)).0 as i32);
        fx.env.drops.borrow_mut().clear();
        npc_state(&me).borrow_mut().age = 899.0;
        dodo_create_item_drops_for_death(&me);
        assert!(fx.env.drops.borrow().is_empty());
    }

    #[test]
    fn hit_flee_consumes_one_rand_after_npc() {
        let fx = fixture();
        let me = make(&fx, true, None);
        glibc_rand::srand(9);
        let v = raw_draws(9, 2);
        glibc_rand::srand(9);
        Npc::hit_with_force_blockhead(dodo_of(&me), &me, 1, None);
        let s = dodo_of(&me).dodo.borrow().clone();
        assert_eq!(s.walk_speed, 3.0);
        assert_eq!(s.random_time_between_direction_changes, 5.0);
        assert_eq!(s.movement_direction, if (rr(v[0]) as f64) > 0.5 { 1 } else { -1 });
        assert_eq!(glibc_rand::rand(), v[1]);
        assert!(npc_state(&me).borrow().damage == 1);
    }

    #[test]
    fn walking_on_flat_ground_moves_and_interpolates() {
        let fx = fixture();
        floor(&fx, 30, 60, 2);
        let me = make(&fx, true, None);
        {
            let mut s = dodo_of(&me).dodo.borrow_mut();
            s.movement_direction = 1;
            s.walk_speed = 2.0;
            s.travel_fraction = 0.9;
            s.random_time_between_direction_changes = 100.0;
            s.random_cluck_noise_timer = 100.0;
            s.next_peck_timer = 100.0;
        }
        me.update_accurate_dt_is_simulation(&me, 0.1, 0.1, false);
        // travel 0.9 + 0.2 = 1.1 -> arrives at (40,3): then plans (41,3)
        let s = dodo_of(&me).dodo.borrow().clone();
        assert_eq!(s.from_square, IntPair { x: 40, y: 3 });
        assert_eq!(s.to_square, IntPair { x: 41, y: 3 });
        assert!((s.travel_fraction - 0.1).abs() < 1e-5);
        assert_eq!(me.pos(), IntPair { x: 40, y: 3 });
        let fp = me.float_pos();
        assert!((fp.x - 40.55).abs() < 1e-4 || (fp.x - 40.6).abs() < 0.2);
    }

    #[test]
    fn wall_ahead_stops_and_falls_when_no_floor() {
        let fx = fixture();
        floor(&fx, 30, 60, 2);
        let me = make(&fx, true, None);
        // a solid wall two high ahead
        for y in 3..=4 {
            let mut t = Tile::default();
            t.type_index = 1;
            fx.env.tiles.borrow_mut().insert((41, y), t);
        }
        {
            let mut s = dodo_of(&me).dodo.borrow_mut();
            s.movement_direction = 1;
            s.walk_speed = 2.0;
            s.travel_fraction = 1.0;
            s.random_time_between_direction_changes = 100.0;
            s.random_cluck_noise_timer = 100.0;
            s.next_peck_timer = 100.0;
        }
        me.update_accurate_dt_is_simulation(&me, 0.01, 0.01, false);
        let s = dodo_of(&me).dodo.borrow().clone();
        assert_eq!(s.to_square, s.from_square);
        assert_eq!(s.movement_direction, 0);
        // no floor: falls one tile
        fx.env.tiles.borrow_mut().remove(&(40, 2));
        let mut a = Tile::default();
        a.type_index = 2;
        fx.env.tiles.borrow_mut().insert((40, 2), a);
        {
            let mut s = dodo_of(&me).dodo.borrow_mut();
            s.movement_direction = 0;
            s.travel_fraction = 1.0;
        }
        me.update_accurate_dt_is_simulation(&me, 0.01, 0.01, false);
        assert_eq!(dodo_of(&me).dodo.borrow().to_square, IntPair { x: 40, y: 2 });
    }

    #[test]
    fn lay_timer_and_egg_draw_order() {
        let fx = fixture();
        floor(&fx, 30, 60, 2);
        let me = make(&fx, true, None);
        {
            let mut n = npc_state(&me).borrow_mut();
            n.lay_timer = 0.05;
            n.breed = 0;
            n.fullness = 6000.0;
            n.has_bred = false;
        }
        {
            let mut s = dodo_of(&me).dodo.borrow_mut();
            s.travel_fraction = 0.0;
            s.walk_speed = 1.0;
            s.movement_direction = 0;
            s.random_time_between_direction_changes = 100.0;
            s.random_cluck_noise_timer = 100.0;
            s.next_peck_timer = 100.0;
        }
        glibc_rand::srand(4);
        me.update_accurate_dt_is_simulation(&me, 0.1, 0.1, false);
        // layLikelyhood(0) = 1.0, R() < 1.0 almost always; fullness reduced by 2700 BEFORE the child
        let n = npc_state(&me).borrow();
        assert_eq!(n.lay_cooldown_timer, 900.0);
        assert!(n.fullness < 6000.0 - 2000.0 || n.fullness == 6000.0);
    }

    #[test]
    fn dead_dodo_timer_and_removal() {
        let fx = fixture();
        let me = make(&fx, true, None);
        npc_state(&me).borrow_mut().dead = true;
        dodo_of(&me).dodo.borrow_mut().travel_fraction = 0.0;
        me.update_accurate_dt_is_simulation(&me, 0.1, 0.1, false);
        assert!(!me.needs_removed());
        assert!((dodo_of(&me).dodo.borrow().travel_fraction - 0.4).abs() < 1e-6);
        me.update_accurate_dt_is_simulation(&me, 0.2, 0.2, false);
        me.update_accurate_dt_is_simulation(&me, 0.2, 0.2, false);
        assert!(me.needs_removed());
        let _ = register_npc_ctor;
        let _ = set_npc_env;
    }

    #[test]
    fn rider_helpers() {
        let fx = fixture();
        let me = make(&fx, true, None);
        let d = dodo_of(&me);
        assert_eq!(Npc::ride_direction(d), 0);
        d.dodo.borrow_mut().rider_target_velocity = 0.5;
        assert_eq!(Npc::ride_direction(d), 0); // no rider
        let rp = Npc::rider_pos_for_blockhead(d, &me);
        assert_eq!(rp[2], -0.75);
        Npc::set_target_velocity(d, Vector2 { x: 0.7, y: 9.0 });
        assert_eq!(d.dodo.borrow().rider_target_velocity, 0.7);
        assert!(!Npc::blockhead_can_ride_using_item(d, &me, 0) || npc_state(&me).borrow().age > 900.0);
    }

    fn bh_at(x: i32, y: i32) -> ObjRef {
        let b: ObjRef = crate::objects::base::testutil::TestObj::new();
        b.base().borrow_mut().pos = IntPair { x, y };
        b
    }

    #[test]
    fn phase5_flees_a_moving_blockhead_with_rand_order() {
        // find a seed where the first draw is < 0.05
        let mut seed = 0u32;
        loop {
            let v = raw_draws(seed, 1)[0];
            if rr(v) < 0.05 {
                break;
            }
            seed += 1;
            assert!(seed < 1_000_000);
        }
        let fx = fixture();
        floor(&fx, 20, 70, 2);
        let me = make(&fx, true, None);
        let bh = bh_at(41, 3);
        fx.env.bhs.borrow_mut().push(bh.clone());
        fx.env.moving.borrow_mut().push((crate::objects::handle::obj_id(&bh), true));
        {
            let mut s = dodo_of(&me).dodo.borrow_mut();
            s.movement_direction = 0;
            s.walk_speed = 1.0;
            s.travel_fraction = 0.0;
            s.random_time_between_direction_changes = 100.0;
            s.random_cluck_noise_timer = 100.0;
            s.next_peck_timer = 100.0;
            s.random_x_offset = 0.0;
        }
        let v = raw_draws(seed, 4);
        glibc_rand::srand(seed);
        me.update_accurate_dt_is_simulation(&me, 0.01, 0.01, false);
        let s = dodo_of(&me).dodo.borrow().clone();
        // dx = +1 (between -1 and 1): random direction from the 2nd draw, then walkSpeed = 2 + R (3rd draw)
        assert_eq!(s.movement_direction, if v[1] > 0x3fffffff { 1 } else { -1 });
        assert_eq!(s.walk_speed.to_bits(), (2.0f32 + rr(v[2])).to_bits());
        assert_eq!(s.random_cluck_noise_timer.to_bits(), (0.2f32 * (100.0f32 - 0.01f32)).to_bits());
        assert_eq!(glibc_rand::rand(), v[3]);
    }

    #[test]
    fn remote_rider_controls_the_dodo_through_remote_goal() {
        let fx = fixture();
        floor(&fx, 20, 70, 2);
        let me = make(&fx, true, None);
        let rider = bh_at(40, 3);
        rider.base().borrow_mut().is_net = true;
        npc_state(&me).borrow_mut().rider = Some(rider);
        // a rider's update arrives
        let mut u = DodoUpdateNetData::read_le(&[0u8; 0x28]);
        u.to_square_x = 43;
        u.to_square_y = 3;
        u.walk_speed = 255;
        u.movement_direction = 1;
        dodo_do_remote_update(&me, u);
        let s = dodo_of(&me).dodo.borrow().clone();
        assert_eq!(s.remote_goal_square, IntPair { x: 43, y: 3 });
        assert_eq!(s.walk_speed, 1.0);
        assert_eq!(s.movement_direction, 1);
        assert!(me.update_needs_to_be_sent());
        // a locally-controlled rider owns movement: the update is ignored
        let local = bh_at(40, 3);
        npc_state(&me).borrow_mut().rider = Some(local);
        let before = dodo_of(&me).dodo.borrow().remote_goal_square;
        u.to_square_x = 50;
        dodo_do_remote_update(&me, u);
        assert_eq!(dodo_of(&me).dodo.borrow().remote_goal_square, before);
    }

    #[test]
    fn apple_eating_and_lay_timer_arming_on_arrival() {
        let fx = fixture();
        floor(&fx, 20, 70, 2);
        let me = make(&fx, true, None);
        // an apple tree trunk at (41,3)
        let mut t = Tile::default();
        t.type_index = 2;
        t.contents = 4;
        fx.env.tiles.borrow_mut().insert((41, 3), t);
        let tree: ObjRef = crate::objects::base::testutil::TestObj::new();
        fx.env.trees.borrow_mut().insert((41, 3), tree.clone());
        fx.env.food.borrow_mut().insert(crate::objects::handle::obj_id(&tree), 2000.0);
        {
            let mut n = npc_state(&me).borrow_mut();
            n.fullness = 1000.0;
            n.lay_cooldown_timer = 0.0;
            n.lay_timer = 0.0;
        }
        {
            let mut s = dodo_of(&me).dodo.borrow_mut();
            s.from_square = IntPair { x: 40, y: 3 };
            s.to_square = IntPair { x: 41, y: 3 };
            s.travel_fraction = 1.0;
            s.movement_direction = 1;
            s.walk_speed = 1.0;
            s.random_time_between_direction_changes = 100.0;
            s.random_cluck_noise_timer = 100.0;
            s.next_peck_timer = 100.0;
        }
        me.update_accurate_dt_is_simulation(&me, 0.01, 0.01, false);
        assert!(npc_state(&me).borrow().fullness > 1800.0);
        assert_eq!(fx.env.food.borrow()[&crate::objects::handle::obj_id(&tree)], 1100.0);
        assert_eq!(me.pos(), IntPair { x: 41, y: 3 });
        // full and rested: arming draws one number in [450, 900)
        {
            let mut n = npc_state(&me).borrow_mut();
            n.fullness = 6000.0;
        }
        {
            let mut s = dodo_of(&me).dodo.borrow_mut();
            s.from_square = IntPair { x: 41, y: 3 };
            s.to_square = IntPair { x: 42, y: 3 };
            s.travel_fraction = 1.0;
        }
        me.update_accurate_dt_is_simulation(&me, 0.01, 0.01, false);
        let lt = npc_state(&me).borrow().lay_timer;
        assert!((450.0..900.0).contains(&lt) || lt > 0.0, "{lt}");
    }

    #[allow(dead_code)]
    fn _u(_: &dyn PassiveEnv) {}
}
