//! `Yak: DonkeyLike` (NPC type 8, DynamicObject type 63): milk and hair, wheat eater.
//!
//!
//! The AI is `donkey_like.rs`; Yak adds `milk` / `hair`, their serialisation, the two harvest interactions and the death
//! drops.

use std::any::Any;
use std::cell::RefCell;

use crate::fmath::cconv::{f32_to_i32, f64_to_i32};
use crate::npc::donkey_like::{
    donkey_like_blockhead_can_ride_using_item, donkey_like_blockhead_unloaded,
    donkey_like_donkey_like_creation_net_data_for_client, donkey_like_donkey_like_update_net_data_for_client,
    donkey_like_init_with_world_dynamic_world_at_position_cache_save_dict_is_adult_was_placed_placed_by_client,
    donkey_like_init_with_world_dynamic_world_cache_net_data,
    donkey_like_init_with_world_dynamic_world_save_dict_cache, donkey_like_load_derived_stuff,
    donkey_like_remote_creation_data_update, donkey_like_remote_update, donkey_like_render_pos,
    donkey_like_update_accurate_dt_is_simulation, DonkeyLikeClass, DonkeyLikeState, DONKEY_LIKE_CLASS_INFO,
};
use crate::npc::egg::penv;
use crate::npc::npc::{
    alloc_npc, npc_append_npc_creation_data_to_data, npc_get_save_dict, npc_set_adult_creation_start_values, npc_state,
    Npc, NpcState,
};
use crate::objects::base::{padded_bytes, DynamicObject, DynamicObjectBase, DynamicWorldRef, WorldRef};
use crate::objects::class_info::ClassInfo;
use crate::objects::handle::{CacheRef, ObjRef};
use crate::plist::value::{PlistDict, PlistValue};
use crate::rng::glibc_rand;
use crate::types::netdata::{YakCreationNetData, YakUpdateNetData};
use crate::types::structs::{IntPair, Vector2, WireStruct};
use crate::{donkey_like_npc_methods, impl_npc_protocols, npc_forward};

pub use crate::npc::npc::NPC_YAK;
/// `DYNAMIC_OBJECT_TYPE_NPC_YAK`.
pub const DYNAMIC_OBJECT_TYPE_NPC_YAK: i32 = 63;

const ITEM_LEATHER: i32 = 120;
const ITEM_WHEAT: i32 = 310;
const ITEM_MILK_BUCKET: i32 = 319;
const ITEM_YAK_HORN: i32 = 321;
const ITEM_YAK_SHAVINGS: i32 = 323;
const ITEM_YAK_CAGE: i32 = 325;
const ITEM_CAGE: i32 = 302;
const PLANT_WHEAT: i32 = 9;

/// `Yak: DonkeyLike`.
pub static YAK_CLASS_INFO: ClassInfo =
    ClassInfo { name: "Yak", superclass: Some(&DONKEY_LIKE_CLASS_INFO), protocols: &[] };

/// Yak ivars 0x500, 0x504.
#[derive(Clone, Debug, Default)]
pub struct YakState {
    /// seconds since last milked (+dt when adult)
    pub milk: f32,
    /// seconds since last shaved (+dt when adult)
    pub hair: f32,
}

/// The Yak.
pub struct Yak {
    pub base: RefCell<DynamicObjectBase>,
    pub npc: RefCell<NpcState>,
    pub donkey_like: RefCell<DonkeyLikeState>,
    pub yak: RefCell<YakState>,
}

impl Yak {
    fn alloc() -> std::rc::Rc<Yak> {
        alloc_npc(|w| Yak {
            base: RefCell::new(DynamicObjectBase::alloc_zeroed_with(w)),
            npc: RefCell::new(NpcState::default()),
            donkey_like: RefCell::new(DonkeyLikeState::default()),
            yak: RefCell::new(YakState::default()),
        })
    }

    /// `-[Yak initWithWorld:...atPosition:...]`: DonkeyLike init (which runs `setAdultCreationStartValues` ->
    /// `hair = age` BEFORE the dict is applied), then `milk` / `hair` from the dict when there is one (0.0 when the
    /// key is missing). Totals: adult 4 `rand()`, baby 3.
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
        let o = Yak::alloc();
        let me: ObjRef = o.clone();
        if !donkey_like_init_with_world_dynamic_world_at_position_cache_save_dict_is_adult_was_placed_placed_by_client(
            &me, world, dw, pos, cache, save_dict, is_adult, was_placed, client_id,
        ) {
            return None;
        }
        if let Some(sd) = save_dict {
            let mut y = o.yak.borrow_mut();
            y.milk = sd.map.get(&"milk".to_string()).map_or(0.0f32, |v| v.float_value());
            y.hair = sd.map.get(&"hair".to_string()).map_or(0.0f32, |v| v.float_value());
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
        Yak::init_with_world_dynamic_world_at_position_cache_save_dict_is_adult_was_placed_placed_by_client(
            world, dw, pos, cache, save_dict, is_adult, was_placed, client_id,
        )
    }

    /// `-[Yak initWithWorld:dynamicWorld:saveDict:cache:]`: `milk` / `hair` from the dict. 1 `rand()`.
    pub fn init_with_world_dynamic_world_save_dict_cache(
        world: &WorldRef,
        dw: &DynamicWorldRef,
        save_dict: &PlistDict,
        cache: CacheRef,
    ) -> Option<ObjRef> {
        let o = Yak::alloc();
        let me: ObjRef = o.clone();
        if !donkey_like_init_with_world_dynamic_world_save_dict_cache(&me, world, dw, save_dict, cache) {
            return None;
        }
        {
            let mut y = o.yak.borrow_mut();
            y.milk = save_dict.map.get(&"milk".to_string()).map_or(0.0f32, |v| v.float_value());
            y.hair = save_dict.map.get(&"hair".to_string()).map_or(0.0f32, |v| v.float_value());
        }
        Some(me)
    }

    /// DonkeyLike `cache:netData:` init (inherited by Yak; the virtual `remoteCreationDataUpdate:` applies milk / hair).
    pub fn init_with_world_dynamic_world_cache_net_data(
        world: &WorldRef,
        dw: &DynamicWorldRef,
        cache: CacheRef,
        net_data: &[u8],
    ) -> Option<ObjRef> {
        let me: ObjRef = Yak::alloc();
        if !donkey_like_init_with_world_dynamic_world_cache_net_data(&me, world, dw, cache, net_data) {
            return None;
        }
        Some(me)
    }
}

fn yak_of(me: &ObjRef) -> &Yak {
    me.as_any().downcast_ref::<Yak>().expect("object is not a Yak")
}

fn dw_changed(me: &ObjRef) {
    if let Some(dw) = me.dynamic_world() {
        dw.dynamic_world_changed_at_pos_object_type(me.pos(), me.object_type());
    }
}

/// `-[Yak setAdultCreationStartValues]`: NPC start values, then `hair = age` (no extra `rand()`).
pub fn yak_set_adult_creation_start_values(me: &ObjRef) {
    npc_set_adult_creation_start_values(me);
    let age = npc_state(me).borrow().age;
    yak_of(me).yak.borrow_mut().hair = age;
}

/// `-[Yak createItemDropsForDeath]`: exactly three `rand()` (leather, wheat, horn); drops in the order leather,
/// horn, wheat. Wheat carries the sowable default genes 0x7f/0x7f.
pub fn yak_create_item_drops_for_death(me: &ObjRef) {
    let killer = npc_state(me).borrow().kill_blockhead.clone();
    let env = penv();
    let expert = env.expert_mode();
    let pos = me.pos();
    let r1 = glibc_rand::rand_tagged("Yak.drops.leather");
    let leather;
    let wheat;
    if expert {
        leather = if r1 > 0x3fffffff { 2 } else { 1 };
        let r2 = glibc_rand::rand_tagged("Yak.drops.wheat");
        wheat = if r2 > 0x3fffffff { 1 } else { 0 };
    } else {
        leather = f32_to_i32(1.1f32 + ((r1 as f32) / 2147483648.0f32) * 4.0f32);
        let r2 = glibc_rand::rand_tagged("Yak.drops.wheat");
        wheat = r2 % 3;
    }
    let r3 = glibc_rand::rand_tagged("Yak.drops.horn");
    let horn = if (((r3 as f32) / 2147483648.0f32) as f64) < 0.2f64 { 1 } else { 0 };
    for _ in 0..leather.max(0) {
        env.dw_create_free_block(pos, ITEM_LEATHER, 0, 0, None, 1, killer.as_ref());
    }
    for _ in 0..horn {
        env.dw_create_free_block(pos, ITEM_YAK_HORN, 0, 0, None, 1, killer.as_ref());
    }
    for _ in 0..wheat.max(0) {
        env.dw_create_free_block(pos, ITEM_WHEAT, 0x7f, 0x7f, None, 1, killer.as_ref());
    }
}

/// `-[Yak yakUpdateNetDataForClient:]`: the DonkeyLike update struct, then `milk` / `hair` as
/// `(int16)(int)min(x, 3600.0)` in double (NaN goes through to `(int)NaN` -> 0). Padding is zero.
pub fn yak_yak_update_net_data_for_client(me: &ObjRef, client_id: Option<&str>) -> YakUpdateNetData {
    let dl = donkey_like_donkey_like_update_net_data_for_client(me, client_id);
    let (milk, hair) = {
        let y = yak_of(me).yak.borrow();
        (y.milk, y.hair)
    };
    let q = |v: f32| -> i16 {
        let d = v as f64;
        if 3600.0f64 <= d {
            3600
        } else {
            f64_to_i32(d) as i16
        }
    };
    YakUpdateNetData { donkey_like_update_net_data: dl, milk: q(milk), hair: q(hair), padding: [0; 4] }
}

/// `-[Yak doYakRemoteUpdate:]`: only a replica takes milk / hair from the wire.
pub fn yak_do_yak_remote_update(me: &ObjRef, u: YakUpdateNetData) {
    if me.is_net() {
        let mut y = yak_of(me).yak.borrow_mut();
        y.milk = (u.milk as i32) as f32;
        y.hair = (u.hair as i32) as f32;
    }
}

impl Yak {
    /// `-[Yak canBeMilkedByBlockhead:]`: `age > 900 && milk > 900`.
    pub fn can_be_milked_by_blockhead(&self, _b: &ObjRef) -> bool {
        self.npc.borrow().age > 900.0f32 && self.yak.borrow().milk > 900.0f32
    }
    /// `-[Yak milkByBlockhead:]`: server branch resets `milk`, flags an update and creates a MILK_BUCKET
    /// (`playSound` 0); a replica requests the harvest (interaction type 4).
    pub fn milk_by_blockhead(me: &ObjRef, b: &ObjRef) -> bool {
        let y = yak_of(me);
        if !y.can_be_milked_by_blockhead(b) {
            return false;
        }
        if !me.is_net() {
            y.yak.borrow_mut().milk = 0.0f32;
            me.set_update_needs_to_be_sent(true);
            dw_changed(me);
            penv().dw_create_free_block(me.pos(), ITEM_MILK_BUCKET, 0, 0, None, 0, Some(b));
        } else {
            let mut n = npc_state(me).borrow_mut();
            n.harvest_blockhead_unique_id_to_send = b.unique_id();
            n.harvest_type_to_send = 4;
            drop(n);
            me.set_creation_data_needs_to_be_sent(true);
        }
        true
    }
    /// `-[Yak canBeShavedByBlockhead:]`: `age > 900 && hair > 900`.
    pub fn can_be_shaved_by_blockhead(&self, _b: &ObjRef) -> bool {
        self.npc.borrow().age > 900.0f32 && self.yak.borrow().hair > 900.0f32
    }
    /// `-[Yak shaveByBlockhead:]`: like milking with `hair` and YAK_SHAVINGS (interaction type 5).
    pub fn shave_by_blockhead(me: &ObjRef, b: &ObjRef) -> bool {
        let y = yak_of(me);
        if !y.can_be_shaved_by_blockhead(b) {
            return false;
        }
        if !me.is_net() {
            y.yak.borrow_mut().hair = 0.0f32;
            me.set_update_needs_to_be_sent(true);
            dw_changed(me);
            penv().dw_create_free_block(me.pos(), ITEM_YAK_SHAVINGS, 0, 0, None, 0, Some(b));
        } else {
            let mut n = npc_state(me).borrow_mut();
            n.harvest_blockhead_unique_id_to_send = b.unique_id();
            n.harvest_type_to_send = 5;
            drop(n);
            me.set_creation_data_needs_to_be_sent(true);
        }
        true
    }
}

impl DonkeyLikeClass for Yak {
    fn donkey_like(&self) -> &RefCell<DonkeyLikeState> {
        &self.donkey_like
    }
    ///  -> PLANT_WHEAT
    fn food_plant_type(&self) -> i32 {
        PLANT_WHEAT
    }
    ///  -> `age < 900.0f` (false for NaN)
    fn jumps(&self) -> bool {
        self.npc.borrow().age < 900.0f32
    }
    fn load_derived_stuff(&self, me: &ObjRef) {
        donkey_like_load_derived_stuff(me)
    }
}

impl DynamicObject for Yak {
    fn base(&self) -> &RefCell<DynamicObjectBase> {
        &self.base
    }
    fn class_info(&self) -> &'static ClassInfo {
        &YAK_CLASS_INFO
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
        as_harmable,
        as_ridable,
        as_tap_choice
    );

    /// : `wasBaby = age <= 900` before the super call; an adult accumulates
    /// `milk` and `hair` (+dt); a just-matured yak gets `hair = 900` first (so it is shaveable at once).
    fn update_accurate_dt_is_simulation(&self, me: &ObjRef, dt: f32, accurate_dt: f32, is_simulation: bool) {
        let was_baby = self.npc.borrow().age <= 900.0f32;
        donkey_like_update_accurate_dt_is_simulation(me, dt, accurate_dt, is_simulation);
        if self.npc.borrow().age > 900.0f32 {
            let mut y = self.yak.borrow_mut();
            if was_baby {
                y.hair = 900.0f32;
            }
            y.milk += dt;
            y.hair += dt;
        }
    }

    /// : NPC keys plus `milk`, `hair` (floats).
    fn get_save_dict(&self, me: &ObjRef) -> PlistDict {
        let mut d = npc_get_save_dict(me);
        let y = self.yak.borrow();
        d.map.insert("milk".to_string(), PlistValue::real_f32(y.milk));
        d.map.insert("hair".to_string(), PlistValue::real_f32(y.hair));
        d
    }

    fn render_pos(&self) -> Vector2 {
        donkey_like_render_pos(&self.me())
    }

    /// : 0x90 bytes (DonkeyLike creation struct, then the Yak update struct, which
    /// calls `donkeyLikeUpdateNetDataForClient:` a second time) plus the NPC name tail. Padding 0x8c..0x8f is zero.
    fn creation_net_data_for_client(&self, me: &ObjRef, client_id: Option<&str>) -> Option<Vec<u8>> {
        let dlc = donkey_like_donkey_like_creation_net_data_for_client(me, client_id);
        let yu = yak_yak_update_net_data_for_client(me, client_id);
        let s = YakCreationNetData { donkey_like_creation_net_data: dlc, yak_update_net_data: yu };
        Some(npc_append_npc_creation_data_to_data(me, s.to_vec()))
    }

    /// : the 0x28 struct, no NPC append.
    fn update_net_data_for_client(&self, me: &ObjRef, client_id: Option<&str>) -> Option<Vec<u8>> {
        Some(yak_yak_update_net_data_for_client(me, client_id).to_vec())
    }

    /// : DonkeyLike's, then (alive) the Yak part.
    fn remote_update(&self, me: &ObjRef, net_data: &[u8]) {
        donkey_like_remote_update(me, net_data);
        if !npc_state(me).borrow().dead {
            let u = YakUpdateNetData::read_le(&padded_bytes(net_data, 0x28));
            yak_do_yak_remote_update(me, u);
        }
    }

    /// : unconditional.
    fn remote_creation_data_update(&self, me: &ObjRef, net_data: &[u8]) {
        donkey_like_remote_creation_data_update(me, net_data);
        let c = YakCreationNetData::read_le(&padded_bytes(net_data, 0x90));
        yak_do_yak_remote_update(me, c.yak_update_net_data);
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

impl Npc for Yak {
    fn npc(&self) -> &RefCell<NpcState> {
        &self.npc
    }
    fn npc_type(&self) -> i32 {
        NPC_YAK
    }
    fn max_age(&self) -> f32 {
        10800.0
    }
    fn min_fullness(&self) -> f32 {
        -1800.0
    }
    fn species_name(&self) -> String {
        "YAK".to_string()
    }
    fn food_item_type(&self) -> i32 {
        ITEM_WHEAT
    }
    fn captured_item_type(&self) -> i32 {
        ITEM_YAK_CAGE
    }
    fn capture_required_item_type(&self) -> i32 {
        ITEM_CAGE
    }
    fn max_health(&self) -> u16 {
        64
    }
    fn creation_data_struct_size(&self) -> usize {
        0x90
    }
    fn jumps_on_swipe(&self) -> bool {
        false
    }
    fn set_adult_creation_start_values(&self, me: &ObjRef) {
        yak_set_adult_creation_start_values(me)
    }
    fn create_item_drops_for_death(&self, me: &ObjRef) {
        yak_create_item_drops_for_death(me)
    }
    fn blockhead_can_ride_using_item(&self, b: &ObjRef, item: i32) -> bool {
        donkey_like_blockhead_can_ride_using_item(&self.me(), b, item)
    }
    fn can_be_milked_by_blockhead(&self, b: &ObjRef) -> bool {
        Yak::can_be_milked_by_blockhead(self, b)
    }
    fn milk_by_blockhead(&self, me: &ObjRef, b: &ObjRef) -> bool {
        Yak::milk_by_blockhead(me, b)
    }
    fn can_be_shaved_by_blockhead(&self, b: &ObjRef) -> bool {
        Yak::can_be_shaved_by_blockhead(self, b)
    }
    fn shave_by_blockhead(&self, me: &ObjRef, b: &ObjRef) -> bool {
        Yak::shave_by_blockhead(me, b)
    }
    donkey_like_npc_methods!();
}

impl_npc_protocols!(Yak);

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
    fn make(fx: &Fx, adult: bool, sd: Option<&PlistDict>) -> ObjRef {
        let wr: WorldRef = fx.w.clone();
        let dr: DynamicWorldRef = fx.dw.clone();
        Yak::init_with_world_dynamic_world_at_position_cache_save_dict_is_adult_was_placed_placed_by_client(
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
    fn raw(seed: u32, n: usize) -> Vec<i32> {
        glibc_rand::srand(seed);
        (0..n).map(|_| glibc_rand::rand()).collect()
    }

    #[test]
    fn constants() {
        let fx = fixture();
        let me = make(&fx, true, None);
        let y = yak_of(&me);
        assert_eq!(Npc::npc_type(y), 8);
        assert_eq!(Npc::max_health(y), 64);
        assert_eq!(Npc::max_age(y), 10800.0);
        assert_eq!(Npc::species_name(y), "YAK");
        assert_eq!(Npc::food_item_type(y), 310);
        assert_eq!(Npc::captured_item_type(y), 325);
        assert_eq!(Npc::creation_data_struct_size(y), 0x90);
        assert_eq!(Npc::get_names_array_count(y), 0);
        assert!(!Npc::jumps_on_swipe(y));
        assert_eq!(DonkeyLikeClass::food_plant_type(y), 9);
        assert_eq!(me.object_type(), DYNAMIC_OBJECT_TYPE_NPC_YAK);
    }

    #[test]
    fn hair_equals_age_for_adult_without_dict_and_zero_for_baby_with_dict() {
        let fx = fixture();
        let me = make(&fx, true, None);
        let age = npc_state(&me).borrow().age;
        assert!(age > 0.0);
        assert_eq!(yak_of(&me).yak.borrow().hair, age);
        assert_eq!(yak_of(&me).yak.borrow().milk, 0.0);
        let mut sd = crate::objects::base::new_mutable_dict();
        sd.map.insert("breed".into(), PlistValue::int(0));
        let baby = make(&fx, false, Some(&sd));
        assert_eq!(yak_of(&baby).yak.borrow().hair, 0.0);
        assert_eq!(yak_of(&baby).yak.borrow().milk, 0.0);
        // with keys
        let mut sd2 = crate::objects::base::new_mutable_dict();
        sd2.map.insert("milk".into(), PlistValue::real_f32(12.5));
        sd2.map.insert("hair".into(), PlistValue::real_f32(33.25));
        let o = make(&fx, true, Some(&sd2));
        assert_eq!(yak_of(&o).yak.borrow().milk, 12.5);
        assert_eq!(yak_of(&o).yak.borrow().hair, 33.25);
    }

    #[test]
    fn constructor_draw_totals() {
        let fx = fixture();
        glibc_rand::srand(4);
        let _ = make(&fx, true, None);
        assert_eq!(glibc_rand::rand(), raw(4, 5)[4]);
        glibc_rand::srand(4);
        let _ = make(&fx, false, None);
        assert_eq!(glibc_rand::rand(), raw(4, 4)[3]);
    }

    #[test]
    fn maturing_tick_sets_hair_and_accumulates() {
        let fx = fixture();
        for x in 20..70 {
            fx.env.set_tile(x, 2, 1);
            for y in 3..8 {
                fx.env.set_tile(x, y, 2);
            }
        }
        let me = make(&fx, false, None);
        let y = yak_of(&me);
        npc_state(&me).borrow_mut().age = 899.99;
        y.yak.borrow_mut().milk = 5.0;
        me.base().borrow_mut().float_pos = Vector2 { x: 40.5, y: 3.0 };
        me.update_accurate_dt_is_simulation(&me, 0.5, 0.5, false);
        assert!(npc_state(&me).borrow().age > 900.0);
        assert_eq!(y.yak.borrow().hair, 900.0 + 0.5);
        assert_eq!(y.yak.borrow().milk, 5.5);
        // adult keeps accumulating, baby does not
        me.update_accurate_dt_is_simulation(&me, 0.5, 0.5, false);
        assert_eq!(y.yak.borrow().hair, 901.0);
        npc_state(&me).borrow_mut().age = 100.0;
        me.update_accurate_dt_is_simulation(&me, 0.5, 0.5, false);
        assert_eq!(y.yak.borrow().hair, 901.0);
    }

    #[test]
    fn milk_and_shave_thresholds_and_items() {
        let fx = fixture();
        let me = make(&fx, true, None);
        let y = yak_of(&me);
        let bh: ObjRef = crate::objects::base::testutil::TestObj::new();
        npc_state(&me).borrow_mut().age = 1000.0;
        y.yak.borrow_mut().milk = 900.0;
        assert!(!y.can_be_milked_by_blockhead(&bh)); // strict
        assert!(!Yak::milk_by_blockhead(&me, &bh));
        y.yak.borrow_mut().milk = 900.5;
        assert!(y.can_be_milked_by_blockhead(&bh));
        assert!(Yak::milk_by_blockhead(&me, &bh));
        assert_eq!(y.yak.borrow().milk, 0.0);
        assert_eq!(fx.env.drops.borrow().as_slice(), &[(319, 0, 0, 0)]);
        assert!(me.update_needs_to_be_sent());
        y.yak.borrow_mut().hair = 901.0;
        assert!(Yak::shave_by_blockhead(&me, &bh));
        assert_eq!(y.yak.borrow().hair, 0.0);
        assert_eq!(fx.env.drops.borrow()[1], (323, 0, 0, 0));
        npc_state(&me).borrow_mut().age = 900.0;
        y.yak.borrow_mut().hair = 5000.0;
        assert!(!y.can_be_shaved_by_blockhead(&bh));
        // replica: request instead
        npc_state(&me).borrow_mut().age = 1000.0;
        me.base().borrow_mut().is_net = true;
        y.yak.borrow_mut().milk = 1000.0;
        bh.base().borrow_mut().unique_id = 77;
        assert!(Yak::milk_by_blockhead(&me, &bh));
        assert_eq!(npc_state(&me).borrow().harvest_type_to_send, 4);
        assert_eq!(npc_state(&me).borrow().harvest_blockhead_unique_id_to_send, 77);
        assert_eq!(y.yak.borrow().milk, 1000.0);
    }

    #[test]
    fn save_dict_and_net_quantisation() {
        let fx = fixture();
        let me = make(&fx, true, None);
        let y = yak_of(&me);
        y.yak.borrow_mut().milk = 5000.0;
        y.yak.borrow_mut().hair = 123.9;
        let d = me.get_save_dict(&me);
        assert!(matches!(d.map.get(&"milk".to_string()), Some(PlistValue::Real { is_float: true, .. })));
        let nd = me.update_net_data_for_client(&me, None).unwrap();
        assert_eq!(nd.len(), 0x28);
        let u = YakUpdateNetData::read_le(&nd);
        assert_eq!(u.milk, 3600);
        assert_eq!(u.hair, 123);
        assert_eq!(&nd[0x24..0x28], &[0, 0, 0, 0]);
        y.yak.borrow_mut().milk = f32::NAN;
        let u = YakUpdateNetData::read_le(&me.update_net_data_for_client(&me, None).unwrap());
        assert_eq!(u.milk, 0);
        let c = me.creation_net_data_for_client(&me, None).unwrap();
        assert!(c.len() >= 0x90);
        // replica applies milk / hair
        let rep = Yak::init_with_world_dynamic_world_cache_net_data(
            &(fx.w.clone() as WorldRef),
            &(fx.dw.clone() as DynamicWorldRef),
            None,
            &c,
        )
        .unwrap();
        assert!(rep.is_net());
        assert_eq!(yak_of(&rep).yak.borrow().hair, 123.0);
    }

    #[test]
    fn drops_rand_order_and_formulas() {
        let fx = fixture();
        let me = make(&fx, true, None);
        let v = raw(6, 4);
        glibc_rand::srand(6);
        yak_create_item_drops_for_death(&me);
        let leather = f32_to_i32(1.1f32 + ((v[0] as f32) / 2147483648.0f32) * 4.0f32);
        let wheat = v[1] % 3;
        let horn = if (((v[2] as f32) / 2147483648.0f32) as f64) < 0.2 { 1 } else { 0 };
        let drops = fx.env.drops.borrow().clone();
        assert_eq!(drops.len() as i32, leather + wheat + horn);
        assert!(drops.iter().take(leather as usize).all(|d| d.0 == 120));
        assert!(drops.iter().skip(leather as usize).take(horn as usize).all(|d| d.0 == 321));
        assert!(drops.iter().rev().take(wheat as usize).all(|d| *d == (310, 0x7f, 0x7f, 1)));
        assert_eq!(glibc_rand::rand(), v[3]); // exactly three draws
        fx.env.drops.borrow_mut().clear();
        fx.env.expert.set(true);
        glibc_rand::srand(6);
        yak_create_item_drops_for_death(&me);
        let l = if v[0] > 0x3fffffff { 2 } else { 1 };
        let w = if v[1] > 0x3fffffff { 1 } else { 0 };
        assert_eq!(fx.env.drops.borrow().len() as i32, l + w + horn);
    }
}
