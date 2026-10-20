//! `Donkey: DonkeyLike` (NPC type 3, DynamicObject type 28): donkeys (breeds 0..=11) and unicorns (breeds 12..=23).
//!
//!
//! All AI, movement, taming and net behaviour is in `donkey_like.rs`; Donkey supplies the per-species constants, the death
//! drops, the breeding hook and the ride check. Donkey has no server state of its own.

use std::any::Any;
use std::cell::RefCell;

use crate::fmath::cconv::f32_to_i32;
use crate::fmath::libm::fabsf;
use crate::items::breeds::{generate_donkey_child_breed_from_parent_breed, name_for_donkey_breed};
use crate::npc::donkey_like::{
    donkey_like_blockhead_unloaded, donkey_like_creation_data_struct_size, donkey_like_creation_net_data_for_client,
    donkey_like_init_with_world_dynamic_world_at_position_cache_save_dict_is_adult_was_placed_placed_by_client,
    donkey_like_init_with_world_dynamic_world_cache_net_data,
    donkey_like_init_with_world_dynamic_world_save_dict_cache, donkey_like_max_health,
    donkey_like_remote_creation_data_update, donkey_like_remote_update, donkey_like_render_pos,
    donkey_like_update_accurate_dt_is_simulation, donkey_like_update_net_data_for_client, DonkeyLikeClass,
    DonkeyLikeState, DONKEY_LIKE_CLASS_INFO,
};
use crate::npc::egg::penv;
use crate::npc::npc::{alloc_npc, Npc, NpcState};
use crate::objects::base::{DynamicObject, DynamicObjectBase, DynamicWorldRef, WorldRef};
use crate::objects::class_info::ClassInfo;
use crate::objects::handle::{CacheRef, ObjRef};
use crate::plist::value::PlistDict;
use crate::rng::glibc_rand;
use crate::types::enums::DonkeyBreed;
use crate::types::structs::{IntPair, Vector2};
use crate::{donkey_like_npc_methods, impl_npc_protocols, npc_forward};

pub use crate::npc::npc::NPC_DONKEY;
/// `DYNAMIC_OBJECT_TYPE_NPC_DONKEY`.
pub const DYNAMIC_OBJECT_TYPE_NPC_DONKEY: i32 = 28;

const ITEM_CARROT: i32 = 71;
const ITEM_CARROT_STICK: i32 = 74;
const ITEM_LEATHER: i32 = 120;
const ITEM_CAGE: i32 = 302;
const ITEM_DONKEY_CAGE: i32 = 324;
const ITEM_RAINBOW_CAKE: i32 = 328;
const ITEM_RAINBOW_ESSENCE: i32 = 329;
const ITEM_UNICORN_CAGE: i32 = 330;

/// `DONKEY_NAMES` (69) and `UNICORN_NAMES` (44), index order is observable.
pub static DONKEY_NAMES: [&str; 69] = [
    "Wookey",
    "Dookey",
    "Dopey",
    "Jubilee",
    "Herbert",
    "Wonkey",
    "Ronky",
    "Spud",
    "Oscar",
    "Honker",
    "Dougie",
    "Doug",
    "Doogie",
    "Bogart",
    "Trevor",
    "Elvis",
    "Wonkee",
    "Wonka",
    "Barry",
    "Willy",
    "Frank",
    "Fred",
    "Bob",
    "Dingbat",
    "Dynamo",
    "Dobbin",
    "Sid",
    "Humphrey",
    "Hilda",
    "Jasper",
    "Donkers",
    "Merlin",
    "Dave",
    "Desmond",
    "Winky",
    "Shaun",
    "Caramel",
    "Pascoe",
    "Geoff",
    "George",
    "Wilfred",
    "Barney",
    "Willie",
    "Arthur",
    "Dougal",
    "Biscuit",
    "Harry",
    "Dinky",
    "Milo",
    "Bear",
    "Duncan",
    "Lucky",
    "Hazel",
    "Krankey",
    "Poppins",
    "Eeyore",
    "Chewbacca",
    "Kong",
    "Eddie",
    "Mister Ed",
    "Charlie",
    "Burrito",
    "Donkey",
    "Pedro",
    "Chomsky",
    "Monty",
    "Grumpy",
    "Stumpy",
    "Lebowski",
];

pub static UNICORN_NAMES: [&str; 44] = [
    "Snowflake",
    "Icy",
    "Grace",
    "Snowball",
    "Glitzy",
    "Glitter",
    "Sparkle",
    "Sparkles",
    "Mystic",
    "Dazzle",
    "Gemini",
    "Diamond",
    "Amethyst",
    "Ruby",
    "Emerald",
    "Sapphire",
    "Sparkle",
    "Jewel",
    "Luna",
    "Moon",
    "Sabrina",
    "Rainbow",
    "Sophie",
    "Shine",
    "Twinkle",
    "Zoey",
    "Melody",
    "Harmony",
    "Cloud",
    "Cinderella",
    "Ava",
    "Annabell",
    "Majic",
    "Pegasus",
    "Rarity",
    "Twilight",
    "Shimmer",
    "Amalthea",
    "Stella",
    "Star",
    "Cornelia",
    "Athena",
    "Einhorn",
    "Fleur",
];

/// `Donkey: DonkeyLike`.
pub static DONKEY_CLASS_INFO: ClassInfo =
    ClassInfo { name: "Donkey", superclass: Some(&DONKEY_LIKE_CLASS_INFO), protocols: &[] };

/// `isUnicorn:= breed > 11`.
pub fn donkey_is_unicorn(breed: u16) -> bool {
    breed > 11
}

/// The Donkey.
pub struct Donkey {
    pub base: RefCell<DynamicObjectBase>,
    pub npc: RefCell<NpcState>,
    pub donkey_like: RefCell<DonkeyLikeState>,
}

impl Donkey {
    fn alloc() -> std::rc::Rc<Donkey> {
        alloc_npc(|w| Donkey {
            base: RefCell::new(DynamicObjectBase::alloc_zeroed_with(w)),
            npc: RefCell::new(NpcState::default()),
            donkey_like: RefCell::new(DonkeyLikeState::default()),
        })
    }

    /// DonkeyLike `atPosition` init (adult 4 `rand()`, baby 3; `loadDerivedStuff` draws none).
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
        let me: ObjRef = Donkey::alloc();
        if !donkey_like_init_with_world_dynamic_world_at_position_cache_save_dict_is_adult_was_placed_placed_by_client(
            &me, world, dw, pos, cache, save_dict, is_adult, was_placed, client_id,
        ) {
            return None;
        }
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
        Donkey::init_with_world_dynamic_world_at_position_cache_save_dict_is_adult_was_placed_placed_by_client(
            world, dw, pos, cache, save_dict, is_adult, was_placed, client_id,
        )
    }

    /// DonkeyLike `saveDict:cache:` init (1 `rand()`).
    pub fn init_with_world_dynamic_world_save_dict_cache(
        world: &WorldRef,
        dw: &DynamicWorldRef,
        save_dict: &PlistDict,
        cache: CacheRef,
    ) -> Option<ObjRef> {
        let me: ObjRef = Donkey::alloc();
        if !donkey_like_init_with_world_dynamic_world_save_dict_cache(&me, world, dw, save_dict, cache) {
            return None;
        }
        Some(me)
    }

    /// DonkeyLike `cache:netData:` init (no `rand()`).
    pub fn init_with_world_dynamic_world_cache_net_data(
        world: &WorldRef,
        dw: &DynamicWorldRef,
        cache: CacheRef,
        net_data: &[u8],
    ) -> Option<ObjRef> {
        let me: ObjRef = Donkey::alloc();
        if !donkey_like_init_with_world_dynamic_world_cache_net_data(&me, world, dw, cache, net_data) {
            return None;
        }
        Some(me)
    }
}

/// `-[Donkey createItemDropsForDeath]`. Rand order: leather draw, then carrot draw (donkey) or one
/// essence draw (unicorn). Leather drops are created before carrots. Carrots carry the sowable default genes 0x7f/0x7f.
pub fn donkey_create_item_drops_for_death(me: &ObjRef) {
    let (breed, killer) = {
        let n = crate::npc::npc::npc_state(me).borrow();
        (n.breed, n.kill_blockhead.clone())
    };
    let env = penv();
    let expert = env.expert_mode();
    let pos = me.pos();
    let normal_count = |r: i32| f32_to_i32(1.1f32 + ((r as f32) / 2147483648.0f32) * 4.0f32);
    if breed < 12 {
        let (leather, carrot);
        if !expert {
            let r1 = glibc_rand::rand_tagged("Donkey.drops.leather");
            leather = normal_count(r1);
            let r2 = glibc_rand::rand_tagged("Donkey.drops.carrot");
            carrot = r2 % 3;
        } else {
            let r1 = glibc_rand::rand_tagged("Donkey.drops.leather");
            leather = if r1 < 0x40000000 { 1 } else { 2 };
            let r2 = glibc_rand::rand_tagged("Donkey.drops.carrot");
            carrot = if r2 > 0x3fffffff { 1 } else { 0 };
        }
        for _ in 0..leather.max(0) {
            env.dw_create_free_block(pos, ITEM_LEATHER, 0, 0, None, 1, killer.as_ref());
        }
        for _ in 0..carrot.max(0) {
            env.dw_create_free_block(pos, ITEM_CARROT, 0x7f, 0x7f, None, 1, killer.as_ref());
        }
    } else {
        let essence = if !expert {
            normal_count(glibc_rand::rand_tagged("Donkey.drops.essence"))
        } else if glibc_rand::rand_tagged("Donkey.drops.essence") < 0x40000000 {
            1
        } else {
            2
        };
        for _ in 0..essence.max(0) {
            env.dw_create_free_block(pos, ITEM_RAINBOW_ESSENCE, 0, 0, None, 1, killer.as_ref());
        }
    }
}

impl DonkeyLikeClass for Donkey {
    fn donkey_like(&self) -> &RefCell<DonkeyLikeState> {
        &self.donkey_like
    }
    /// : unicorns eat no plant (`PLANT_NOTHING`)
    fn food_plant_type(&self) -> i32 {
        if donkey_is_unicorn(self.npc.borrow().breed) {
            0
        } else {
            4
        }
    }
    fn flies(&self) -> bool {
        self.npc.borrow().breed > 11
    }
    /// : only the rainbow unicorn (23)
    fn can_jump_multiple_tiles_while_flying(&self) -> bool {
        (self.npc.borrow().breed as i16) == 0x17
    }
    fn max_velocity(&self) -> f32 {
        if self.npc.borrow().breed < 12 {
            6.0
        } else {
            9.0
        }
    }
    /// : 1 or 2 draws (0 when not bred)
    fn generate_breed_for_child(&self) -> u16 {
        let n = self.npc.borrow();
        generate_donkey_child_breed_from_parent_breed(DonkeyBreed(n.breed), DonkeyBreed(n.mate_breed), n.has_bred).0
    }
    /// : `|actualXSpeed| > 5.0` else `falling` (NaN falls through to `falling`)
    fn galloping(&self) -> bool {
        if self.npc.borrow().breed <= 11 {
            return false;
        }
        let s = self.donkey_like.borrow();
        if fabsf(s.actual_x_speed) > 5.0f32 {
            true
        } else {
            s.falling
        }
    }
}

impl DynamicObject for Donkey {
    fn base(&self) -> &RefCell<DynamicObjectBase> {
        &self.base
    }
    fn class_info(&self) -> &'static ClassInfo {
        &DONKEY_CLASS_INFO
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
        donkey_like_update_accurate_dt_is_simulation(me, dt, accurate_dt, is_simulation)
    }
    fn render_pos(&self) -> Vector2 {
        donkey_like_render_pos(&self.me())
    }
    fn creation_net_data_for_client(&self, me: &ObjRef, client_id: Option<&str>) -> Option<Vec<u8>> {
        Some(donkey_like_creation_net_data_for_client(me, client_id))
    }
    fn update_net_data_for_client(&self, me: &ObjRef, client_id: Option<&str>) -> Option<Vec<u8>> {
        Some(donkey_like_update_net_data_for_client(me, client_id))
    }
    fn remote_update(&self, me: &ObjRef, net_data: &[u8]) {
        donkey_like_remote_update(me, net_data)
    }
    fn remote_creation_data_update(&self, me: &ObjRef, net_data: &[u8]) {
        donkey_like_remote_creation_data_update(me, net_data)
    }
    fn blockhead_unloaded(&self, me: &ObjRef, b: &ObjRef) {
        donkey_like_blockhead_unloaded(me, b)
    }
    /// : `[rider autorelease]`, then the NPC dealloc.
    fn dealloc(&self, me: &ObjRef) {
        self.npc.borrow_mut().rider = None;
        crate::npc::npc::npc_dealloc(me);
    }
}

impl Npc for Donkey {
    fn npc(&self) -> &RefCell<NpcState> {
        &self.npc
    }
    fn npc_type(&self) -> i32 {
        NPC_DONKEY
    }
    fn max_age(&self) -> f32 {
        10800.0
    }
    fn min_fullness(&self) -> f32 {
        -1800.0
    }
    fn species_name(&self) -> String {
        if donkey_is_unicorn(self.npc.borrow().breed) { "UNICORN" } else { "DONKEY" }.to_string()
    }
    fn food_item_type(&self) -> i32 {
        if donkey_is_unicorn(self.npc.borrow().breed) {
            ITEM_RAINBOW_CAKE
        } else {
            ITEM_CARROT
        }
    }
    fn captured_item_type(&self) -> i32 {
        if donkey_is_unicorn(self.npc.borrow().breed) {
            ITEM_UNICORN_CAGE
        } else {
            ITEM_DONKEY_CAGE
        }
    }
    fn capture_required_item_type(&self) -> i32 {
        ITEM_CAGE
    }
    fn get_names_array(&self) -> Option<&'static [&'static str]> {
        if donkey_is_unicorn(self.npc.borrow().breed) {
            Some(&UNICORN_NAMES)
        } else {
            Some(&DONKEY_NAMES)
        }
    }
    fn get_names_array_count(&self) -> i32 {
        if donkey_is_unicorn(self.npc.borrow().breed) {
            44
        } else {
            69
        }
    }
    fn max_health(&self) -> u16 {
        donkey_like_max_health()
    }
    fn creation_data_struct_size(&self) -> usize {
        donkey_like_creation_data_struct_size()
    }
    fn jumps_on_swipe(&self) -> bool {
        true
    }
    fn breed_string(&self) -> String {
        name_for_donkey_breed(DonkeyBreed(self.npc.borrow().breed))
    }
    fn create_item_drops_for_death(&self, me: &ObjRef) {
        donkey_create_item_drops_for_death(me)
    }
    /// : the carrot-stick rule (no `beingControlled` ownership term).
    fn blockhead_can_ride_using_item(&self, b: &ObjRef, item: i32) -> bool {
        let (age, controlled, breed, dead) = {
            let n = self.npc.borrow();
            (n.age, n.being_controlled, n.breed, n.dead)
        };
        if !(age > 900.0f32) {
            return false;
        }
        if controlled {
            return false;
        }
        let need_owner = Npc::tamed(self) || item != ITEM_CARROT_STICK || breed > 11;
        if need_owner && !Npc::belongs_to_player_with_blockhead(self, b) {
            return false;
        }
        if dead {
            return false;
        }
        !self.base.borrow().needs_removed
    }
    donkey_like_npc_methods!();
}

impl_npc_protocols!(Donkey);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::npc::donkey_like::as_donkey_like;
    use crate::npc::egg::test_support::TestPassiveEnv;
    use crate::npc::npc::{as_npc, npc_state};
    use crate::objects::base::testutil::{setup, MockDw, MockWorld};
    use crate::rng::glibc_rand;
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
    fn make(fx: &Fx, adult: bool, breed: Option<i64>) -> ObjRef {
        let wr: WorldRef = fx.w.clone();
        let dr: DynamicWorldRef = fx.dw.clone();
        let mut sd = crate::objects::base::new_mutable_dict();
        if let Some(b) = breed {
            sd.map.insert("breed".into(), crate::plist::value::PlistValue::int(b));
        }
        Donkey::init_with_world_dynamic_world_at_position_cache_save_dict_is_adult_was_placed_placed_by_client(
            &wr,
            &dr,
            IntPair { x: 40, y: 3 },
            None,
            Some(&sd),
            adult,
            false,
            None,
        )
        .unwrap()
    }
    fn raw(seed: u32, n: usize) -> Vec<i32> {
        glibc_rand::srand(seed);
        (0..n).map(|_| glibc_rand::rand()).collect()
    }

    #[test]
    fn constants_and_breed_switch() {
        let fx = fixture();
        let me = make(&fx, true, Some(0));
        let d = me.as_any().downcast_ref::<Donkey>().unwrap();
        assert_eq!(Npc::npc_type(d), 3);
        assert_eq!(Npc::max_age(d), 10800.0);
        assert_eq!(Npc::min_fullness(d), -1800.0);
        assert_eq!(Npc::max_health(d), 32);
        assert_eq!(Npc::creation_data_struct_size(d), 0x68);
        assert_eq!(Npc::species_name(d), "DONKEY");
        assert_eq!(Npc::food_item_type(d), 71);
        assert_eq!(Npc::captured_item_type(d), 324);
        assert_eq!(Npc::get_names_array_count(d), 69);
        assert_eq!(DonkeyLikeClass::food_plant_type(d), 4);
        assert!(!DonkeyLikeClass::flies(d));
        assert_eq!(DonkeyLikeClass::max_velocity(d), 6.0);
        assert_eq!(me.object_type(), DYNAMIC_OBJECT_TYPE_NPC_DONKEY);
        for (breed, uni) in [(11u16, false), (12, true), (23, true)] {
            npc_state(&me).borrow_mut().breed = breed;
            assert_eq!(donkey_is_unicorn(breed), uni);
            assert_eq!(Npc::species_name(d), if uni { "UNICORN" } else { "DONKEY" });
            assert_eq!(Npc::food_item_type(d), if uni { 328 } else { 71 });
            assert_eq!(Npc::captured_item_type(d), if uni { 330 } else { 324 });
            assert_eq!(Npc::get_names_array_count(d), if uni { 44 } else { 69 });
            assert_eq!(DonkeyLikeClass::food_plant_type(d), if uni { 0 } else { 4 });
            assert_eq!(DonkeyLikeClass::flies(d), uni);
            assert_eq!(DonkeyLikeClass::max_velocity(d), if uni { 9.0 } else { 6.0 });
            assert_eq!(DonkeyLikeClass::can_jump_multiple_tiles_while_flying(d), breed == 23);
        }
        assert_eq!(DONKEY_NAMES.len(), 69);
        assert_eq!(UNICORN_NAMES.len(), 44);
        assert_eq!(DONKEY_NAMES[68], "Lebowski");
        assert_eq!(UNICORN_NAMES[16], "Sparkle");
        assert_eq!(UNICORN_NAMES[43], "Fleur");
    }

    #[test]
    fn galloping_nan_falls_through_to_falling() {
        let fx = fixture();
        let me = make(&fx, true, Some(12));
        let d = me.as_any().downcast_ref::<Donkey>().unwrap();
        d.donkey_like.borrow_mut().actual_x_speed = f32::NAN;
        d.donkey_like.borrow_mut().falling = false;
        assert!(!DonkeyLikeClass::galloping(d));
        d.donkey_like.borrow_mut().falling = true;
        assert!(DonkeyLikeClass::galloping(d));
        d.donkey_like.borrow_mut().actual_x_speed = 5.5;
        d.donkey_like.borrow_mut().falling = false;
        assert!(DonkeyLikeClass::galloping(d));
        npc_state(&me).borrow_mut().breed = 3;
        assert!(!DonkeyLikeClass::galloping(d));
    }

    #[test]
    fn constructor_draw_totals_and_derived_stuff() {
        let fx = fixture();
        glibc_rand::srand(5);
        let me = make(&fx, true, None);
        assert_eq!(glibc_rand::rand(), raw(5, 5)[4]); // adult 3+1 = 4 draws
        let s = me.as_any().downcast_ref::<Donkey>().unwrap().donkey_like.borrow().clone();
        assert_eq!(s.random_time_between_direction_changes, -10.0);
        assert_eq!(s.munch_motion_random_speed, 1.0);
        assert_eq!(s.remote_stop_pos, IntPair { x: -1, y: -1 });
        assert_eq!(s.back_pos, me.float_pos());
        glibc_rand::srand(5);
        let _ = make(&fx, false, None);
        assert_eq!(glibc_rand::rand(), raw(5, 4)[3]); // baby 2+1 = 3
    }

    #[test]
    fn drops_normal_and_expert() {
        let fx = fixture();
        let me = make(&fx, true, Some(0));
        let v = raw(8, 2);
        // normal
        glibc_rand::srand(8);
        donkey_create_item_drops_for_death(&me);
        let leather = f32_to_i32(1.1f32 + ((v[0] as f32) / 2147483648.0f32) * 4.0f32);
        let carrots = v[1] % 3;
        let drops = fx.env.drops.borrow().clone();
        assert_eq!(drops.len() as i32, leather + carrots);
        assert!(drops.iter().take(leather as usize).all(|d| d.0 == 120));
        assert!(drops.iter().skip(leather as usize).all(|d| d == &(71, 0x7f, 0x7f, 1)));
        // expert
        fx.env.drops.borrow_mut().clear();
        fx.env.expert.set(true);
        glibc_rand::srand(8);
        donkey_create_item_drops_for_death(&me);
        let l = if v[0] < 0x40000000 { 1 } else { 2 };
        let c = if v[1] > 0x3fffffff { 1 } else { 0 };
        assert_eq!(fx.env.drops.borrow().len(), l + c);
        // unicorn: one draw
        fx.env.drops.borrow_mut().clear();
        npc_state(&me).borrow_mut().breed = 12;
        glibc_rand::srand(8);
        donkey_create_item_drops_for_death(&me);
        assert_eq!(fx.env.drops.borrow().len(), l);
        assert!(fx.env.drops.borrow().iter().all(|d| d.0 == 329));
        assert_eq!(glibc_rand::rand(), v[1]);
    }

    #[test]
    fn ride_rules() {
        let fx = fixture();
        let me = make(&fx, true, Some(0));
        let d = me.as_any().downcast_ref::<Donkey>().unwrap();
        let other: ObjRef = crate::objects::base::testutil::TestObj::new();
        npc_state(&me).borrow_mut().age = 1000.0;
        // untamed donkey, carrot stick: anyone may ride
        assert!(Npc::blockhead_can_ride_using_item(d, &other, 74));
        // other item: needs ownership, untamed belongs to nobody
        assert!(!Npc::blockhead_can_ride_using_item(d, &other, 71));
        npc_state(&me).borrow_mut().breed = 12;
        assert!(!Npc::blockhead_can_ride_using_item(d, &other, 74));
        npc_state(&me).borrow_mut().age = 900.0;
        assert!(!Npc::blockhead_can_ride_using_item(d, &other, 74));
        assert!(as_donkey_like(&me).is_some() && as_npc(&me).is_some());
    }

    #[test]
    fn net_structs_and_jump_send_value_reset() {
        let fx = fixture();
        let me = make(&fx, true, Some(0));
        let d = me.as_any().downcast_ref::<Donkey>().unwrap();
        {
            let mut s = d.donkey_like.borrow_mut();
            s.target_x_speed = -3.0;
            s.random_goal_rotation = 0.5;
            s.jump_action_send_value = 2;
        }
        let nd = me.update_net_data_for_client(&me, None).unwrap();
        assert_eq!(nd.len(), 0x20);
        use crate::types::netdata::DonkeyLikeUpdateNetData;
        use crate::types::structs::WireStruct;
        let u = DonkeyLikeUpdateNetData::read_le(&nd);
        assert_eq!(u.target_x_speed, (-3.0f32 * 255.0f32) as i32 as i16);
        assert_eq!(u.walk_speed, 0);
        assert_eq!(u.random_goal_rotation, (0.5f32 * 255.0f32) as i32 as i16);
        assert_eq!(u.jump_action_send_value, 2);
        assert_eq!(nd[0x1f], 0);
        assert_eq!(d.donkey_like.borrow().jump_action_send_value, 0);
        assert!(d.donkey_like.borrow().last_send_contained_movement);
        let c = me.creation_net_data_for_client(&me, None).unwrap();
        assert!(c.len() >= 0x68);
    }

    #[test]
    fn hit_flees_with_one_raw_rand() {
        let fx = fixture();
        for y in [2] {
            for x in 30..60 {
                fx.env.set_tile(x, y, 1);
                fx.env.set_tile(x, y + 1, 2);
                fx.env.set_tile(x, y + 2, 2);
                fx.env.set_tile(x, y + 3, 2);
                fx.env.set_tile(x, y + 4, 2);
            }
        }
        let me = make(&fx, true, Some(0));
        let v = raw(3, 2);
        glibc_rand::srand(3);
        Npc::hit_with_force_blockhead(me.as_any().downcast_ref::<Donkey>().unwrap(), &me, 1, None);
        let s = me.as_any().downcast_ref::<Donkey>().unwrap().donkey_like.borrow().clone();
        assert!(s.jump_action_queued);
        assert_eq!(s.jump_action_send_value, 2);
        assert_eq!(s.random_time_between_direction_changes, 5.0);
        assert_eq!(s.target_x_speed, if v[0] < 0x40000000 { -8.0 } else { 8.0 });
        assert_eq!(s.goal_x, 40 + (2.0 * s.target_x_speed) as i32);
        assert_eq!(glibc_rand::rand(), v[1]);
    }

    #[test]
    fn walking_ai_picks_goal_and_moves() {
        let fx = fixture();
        for x in 20..70 {
            fx.env.set_tile(x, 2, 1);
            for y in 3..8 {
                fx.env.set_tile(x, y, 2);
            }
        }
        let me = make(&fx, true, Some(0));
        let d = me.as_any().downcast_ref::<Donkey>().unwrap();
        npc_state(&me).borrow_mut().age = 2000.0;
        npc_state(&me).borrow_mut().fullness = 100.0;
        me.base().borrow_mut().float_pos = Vector2 { x: 40.5, y: 3.0 };
        d.donkey_like.borrow_mut().back_pos = Vector2 { x: 40.5, y: 3.0 };
        glibc_rand::srand(21);
        for _ in 0..200 {
            me.update_accurate_dt_is_simulation(&me, 0.05, 0.05, false);
        }
        // it stayed on the floor row and within a plausible distance
        assert_eq!(me.pos().y, 3);
        assert!(me.pos().x >= 20 && me.pos().x < 70);
        assert!(d.donkey_like.borrow().goal_x != 0);
    }

    #[test]
    fn dead_donkey_drops_then_is_removed() {
        let fx = fixture();
        let me = make(&fx, true, Some(0));
        npc_state(&me).borrow_mut().age = 1000.0;
        npc_state(&me).borrow_mut().dead = true;
        me.update_accurate_dt_is_simulation(&me, 0.5, 0.5, false);
        assert!(!me.needs_removed());
        me.update_accurate_dt_is_simulation(&me, 0.6, 0.6, false);
        assert!(me.needs_removed());
        assert!(!fx.env.drops.borrow().is_empty());
        assert_eq!(me.as_any().downcast_ref::<Donkey>().unwrap().donkey_like.borrow().death_timer, 1.0);
    }
}
