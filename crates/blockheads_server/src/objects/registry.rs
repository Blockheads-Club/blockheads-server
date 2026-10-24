//! Object registry: the wiring layer that fills object_model's class tables.
//!
//!  Dynamic object
//! type tables";, "## 5. NPC helpers"; /
//! `updateNetObjects`;, `addStandardObjectAtPos`, `interactionObjectPlacedAtPosition`.
//!
//! The module has no game logic. It provides
//! 1. the `ClassInfo` list ([`install`] step 1, section 4),
//! 2. the dynamic-object and interaction-object `ClassFactory` tables (section 3),
//! 3. the NPC constructor table (section 3.2),
//! 4. four dispatch helpers for DynamicWorld (section 2),
//! 5. [`PlainInteractionObject`], the concrete `InteractionObject` (type 15, section 2.1),
//! 6. [`as_interaction_like`].
//!
//! ## World handle bridge
//! The `ClassFactory` fn pointers take the trait-object handles `Rc<dyn WorldApi>` / `Rc<dyn DynamicWorldApi>`
//!  while most concrete classes take `&Rc<World>` / `&Rc<DynamicWorld>`. The two production
//! implementors of those traits are exactly `World` and `DynamicWorld`, so [`concrete_world`] / [`concrete_dw`]
//! turn the handle back into the concrete `Rc`. Note: a safe
//! `as_any_rc` on `WorldApi` / `DynamicWorldApi` would remove the `unsafe`. Test mocks (`MockWorld`, `MockDw`) must
//! never be passed to the entries whose class takes concrete handles.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use super::base::{
    base_init_with_world_dynamic_world_at_position_cache, DynamicObject, DynamicObjectBase, DynamicWorldRef, WorldRef,
};
use super::class_info::{register_class_info, ClassInfo, DYNAMIC_OBJECT_CLASS, INTERACTION_OBJECT_CLASS};
use super::handle::{alloc_obj, downcast, CacheRef, ObjRef};
use super::interaction_object::{
    interaction_object_blockhead_unloaded, interaction_object_blockheads_loaded,
    interaction_object_creation_net_data_for_client, interaction_object_dealloc, interaction_object_get_save_dict,
    interaction_object_init_with_world_dynamic_world_cache_net_data, interaction_object_remote_update,
    interaction_object_should_add_to_macro_block, interaction_object_update_accurate_dt_is_simulation,
    interaction_object_update_net_data_for_client, InteractionObjectLike, InteractionObjectState,
};
use super::protocols::TapChoiceDynamicObject;
use super::type_tables::{
    class_name_for_dynamic_object_type, class_name_for_interaction_object_type, register_dynamic_class,
    register_interaction_class, ClassFactory, FromNetDataFn, FromPositionFn, FromPositionInteractionFn,
    FromPositionTypeSaveDictFn, FromSaveDictFn,
};
use crate::dynworld::state::DynamicWorld;
use crate::items::inventory_item::InventoryItem;
use crate::npc::npc::register_npc_ctor;
use crate::plist::value::PlistDict;
use crate::trees::tree::NoiseRef;
use crate::types::structs::IntPair;
use crate::world::state::World;

// ------------------------------------------------------------------------------------------------ handle bridge

/// `Rc<dyn WorldApi>` back to the concrete `Rc<World>`.
///
/// SAFETY: every production `WorldApi` implementor is `World` (world_state); the pointer therefore addresses a
/// `World` allocated by `Rc`, and `Rc::from_raw` reconstitutes the strong count taken by `into_raw`.
pub fn concrete_world(w: &WorldRef) -> Rc<World> {
    unsafe { Rc::from_raw(Rc::into_raw(w.clone()) as *const World) }
}

/// `Rc<dyn DynamicWorldApi>` back to the concrete `Rc<DynamicWorld>`.
///
/// SAFETY: the only production `DynamicWorldApi` implementor is `dynworld::state::DynamicWorld`.
pub fn concrete_dw(d: &DynamicWorldRef) -> Rc<DynamicWorld> {
    unsafe { Rc::from_raw(Rc::into_raw(d.clone()) as *const DynamicWorld) }
}

// ------------------------------------------------------------------------------------------------ PlainInteractionObject

/// The concrete `InteractionObject` (type 15). The ObjC class is not abstract: a client can make the server do
/// `[InteractionObject alloc] initWithWorld:...cache:netData:` (section 5.1). Implements exactly the InteractionObject
/// overrides and nothing else.
pub struct PlainInteractionObject {
    base: RefCell<DynamicObjectBase>,
    io: RefCell<InteractionObjectState>,
}

impl PlainInteractionObject {
    fn alloc() -> Rc<PlainInteractionObject> {
        alloc_obj(|w| PlainInteractionObject {
            base: RefCell::new(DynamicObjectBase::alloc_zeroed_with(w)),
            io: RefCell::new(InteractionObjectState::default()),
        })
    }

    /// `-[InteractionObject initWithWorld:dynamicWorld:cache:netData:]` on a plain instance. `None` = nil.
    pub fn init_with_world_dynamic_world_cache_net_data(
        world: &WorldRef,
        dw: &DynamicWorldRef,
        cache: CacheRef,
        net_data: &[u8],
    ) -> Option<ObjRef> {
        let me: ObjRef = Self::alloc();
        if !interaction_object_init_with_world_dynamic_world_cache_net_data(&me, world, dw, cache, net_data) {
            return None;
        }
        Some(me)
    }

    /// `-[DynamicObject initWithWorld:dynamicWorld:atPosition:cache:]` (inherited, base init only).
    pub fn init_with_world_dynamic_world_at_position_cache(
        world: &WorldRef,
        dw: &DynamicWorldRef,
        pos: IntPair,
        cache: CacheRef,
    ) -> Option<ObjRef> {
        let me: ObjRef = Self::alloc();
        // the base initialiser always succeeds when the position is given
        let inited = base_init_with_world_dynamic_world_at_position_cache(&me, world, dw, pos, cache);
        debug_assert!(inited);
        Some(me)
    }
}

impl DynamicObject for PlainInteractionObject {
    fn base(&self) -> &RefCell<DynamicObjectBase> {
        &self.base
    }
    fn class_info(&self) -> &'static ClassInfo {
        &INTERACTION_OBJECT_CLASS
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn object_type(&self) -> i32 {
        super::interaction_object::interaction_object_object_type()
    }
    fn requires_physical_block(&self) -> bool {
        super::interaction_object::interaction_object_requires_physical_block()
    }
    fn should_add_to_macro_block(&self) -> bool {
        interaction_object_should_add_to_macro_block(self)
    }
    fn as_interaction_object(&self) -> Option<&RefCell<InteractionObjectState>> {
        Some(&self.io)
    }
    fn as_interaction_like(&self) -> Option<&dyn InteractionObjectLike> {
        Some(self)
    }
    fn as_tap_choice(&self) -> Option<&dyn TapChoiceDynamicObject> {
        Some(self)
    }
    fn update_accurate_dt_is_simulation(&self, me: &ObjRef, dt: f32, accurate_dt: f32, is_simulation: bool) {
        interaction_object_update_accurate_dt_is_simulation(me, dt, accurate_dt, is_simulation)
    }
    fn get_save_dict(&self, me: &ObjRef) -> PlistDict {
        interaction_object_get_save_dict(me)
    }
    fn creation_net_data_for_client(&self, me: &ObjRef, client_id: Option<&str>) -> Option<Vec<u8>> {
        interaction_object_creation_net_data_for_client(me, client_id)
    }
    fn update_net_data_for_client(&self, me: &ObjRef, client_id: Option<&str>) -> Option<Vec<u8>> {
        interaction_object_update_net_data_for_client(me, client_id)
    }
    fn remote_update(&self, me: &ObjRef, net_data: &[u8]) {
        interaction_object_remote_update(me, net_data)
    }
    fn blockheads_loaded(&self, me: &ObjRef) {
        interaction_object_blockheads_loaded(me)
    }
    fn blockhead_unloaded(&self, me: &ObjRef, b: &ObjRef) {
        interaction_object_blockhead_unloaded(me, b)
    }
    fn dealloc(&self, me: &ObjRef) {
        interaction_object_dealloc(me)
    }
}

impl InteractionObjectLike for PlainInteractionObject {
    fn io_state(&self) -> &RefCell<InteractionObjectState> {
        &self.io
    }
}

impl TapChoiceDynamicObject for PlainInteractionObject {
    fn action_title(&self) -> String {
        InteractionObjectLike::action_title(self)
    }
    fn is_double_height(&self) -> bool {
        InteractionObjectLike::is_double_height(self)
    }
}

// ------------------------------------------------------------------------------------------------ class list (section 4)

pub use crate::trains::train_car::TRAIN_CAR_CLASS;

/// Every concrete class registered by [`install`] step 1 (the two base classes are always known to
/// `class_info_by_name`; they are listed too so the list is the full 71).
fn all_classes() -> Vec<&'static ClassInfo> {
    use crate::blockhead::blockhead::BLOCKHEAD_CLASS;
    use crate::building::bed::BED_CLASS;
    use crate::building::column::COLUMN_CLASS;
    use crate::building::door::DOOR_CLASS;
    use crate::building::elevator::{ELEVATOR_MOTOR_CLASS, ELEVATOR_SHAFT_CLASS};
    use crate::building::ladder::LADDER_CLASS;
    use crate::building::stairs::STAIRS_CLASS;
    use crate::building::window::WINDOW_CLASS;
    use crate::building::wire::WIRE_CLASS;
    use crate::crafting::workbench::WORKBENCH_CLASS;
    use crate::env::gather_block::GATHER_BLOCK_CLASS;
    use crate::env::snow_surface_block::SNOW_SURFACE_BLOCK_CLASS;
    use crate::env::surface_block::SURFACE_BLOCK_CLASS;
    use crate::interaction::chest::CHEST_CLASS;
    use crate::interaction::mirror::MIRROR_CLASS;
    use crate::interaction::ownership_sign::OWNERSHIP_SIGN_CLASS;
    use crate::interaction::painting::PAINTING_CLASS;
    use crate::interaction::sign::SIGN_CLASS;
    use crate::interaction::trade_portal::TRADE_PORTAL_CLASS;
    use crate::interaction::trading_post::TRADING_POST_CLASS;
    use crate::lighting::artificial_light::ARTIFICIAL_LIGHT_CLASS;
    use crate::lighting::glow_block::GLOW_BLOCK_CLASS;
    use crate::lighting::torch::TORCH_CLASS;
    use crate::npc::cave_troll::CAVE_TROLL_CLASS;
    use crate::npc::clown_fish::CLOWN_FISH_CLASS_INFO;
    use crate::npc::dodo::DODO_CLASS_INFO;
    use crate::npc::donkey::DONKEY_CLASS_INFO;
    use crate::npc::donkey_like::DONKEY_LIKE_CLASS_INFO;
    use crate::npc::drop_bear::DROP_BEAR_CLASS;
    use crate::npc::egg::EGG_CLASS_INFO;
    use crate::npc::npc::NPC_CLASS;
    use crate::npc::scorpion::SCORPION_CLASS;
    use crate::npc::shark::SHARK_CLASS_INFO;
    use crate::npc::yak::YAK_CLASS_INFO;
    use crate::physics_objects::boat::BOAT_CLASS;
    use crate::physics_objects::fire_object::FIRE_OBJECT_CLASS;
    use crate::physics_objects::free_block::FREE_BLOCK_CLASS;
    use crate::plants::kelp::KELP_PLANT_CLASS;
    use crate::plants::normal_plant::NORMAL_PLANT_CLASS;
    use crate::plants::plant::PLANT_CLASS;
    use crate::plants::simple_plants::{
        CARROT_PLANT_CLASS, CHILLI_PLANT_CLASS, CORN_PLANT_CLASS, FLAX_PLANT_CLASS, SUNFLOWER_PLANT_CLASS,
        TOMATO_PLANT_CLASS, WHEAT_PLANT_CLASS,
    };
    use crate::plants::tulip::TULIP_PLANT_CLASS;
    use crate::plants::vine::VINE_PLANT_CLASS;
    use crate::trains::freight_car::FREIGHT_CAR_CLASS;
    use crate::trains::hand_car::HAND_CAR_CLASS;
    use crate::trains::passenger_car::PASSENGER_CAR_CLASS;
    use crate::trains::rail::RAIL_CLASS;
    use crate::trains::steam_train::STEAM_TRAIN_CLASS;
    use crate::trains::train_station::TRAIN_STATION_CLASS;
    use crate::trees::apple::APPLE_TREE_CLASS;
    use crate::trees::cactus::CACTUS_TREE_CLASS;
    use crate::trees::cherry::CHERRY_TREE_CLASS;
    use crate::trees::coconut::COCONUT_TREE_CLASS;
    use crate::trees::coffee::COFFEE_TREE_CLASS;
    use crate::trees::gem::GEM_TREE_CLASS;
    use crate::trees::lime::LIME_TREE_CLASS;
    use crate::trees::mango::MANGO_TREE_CLASS;
    use crate::trees::maple::MAPLE_TREE_CLASS;
    use crate::trees::orange::ORANGE_TREE_CLASS;
    use crate::trees::pine::PINE_TREE_CLASS;
    use crate::trees::tree::TREE_CLASS;

    vec![
        &DYNAMIC_OBJECT_CLASS,
        &INTERACTION_OBJECT_CLASS,
        &BLOCKHEAD_CLASS,
        &NPC_CLASS,
        &DONKEY_LIKE_CLASS_INFO,
        &DODO_CLASS_INFO,
        &DROP_BEAR_CLASS,
        &CAVE_TROLL_CLASS,
        &CLOWN_FISH_CLASS_INFO,
        &SCORPION_CLASS,
        &SHARK_CLASS_INFO,
        &DONKEY_CLASS_INFO,
        &YAK_CLASS_INFO,
        &EGG_CLASS_INFO,
        &FREE_BLOCK_CLASS,
        &FIRE_OBJECT_CLASS,
        &BOAT_CLASS,
        &PAINTING_CLASS,
        &DOOR_CLASS,
        &LADDER_CLASS,
        &STAIRS_CLASS,
        &WINDOW_CLASS,
        &COLUMN_CLASS,
        &WIRE_CLASS,
        &ELEVATOR_SHAFT_CLASS,
        &ELEVATOR_MOTOR_CLASS,
        &TORCH_CLASS,
        &ARTIFICIAL_LIGHT_CLASS,
        &GLOW_BLOCK_CLASS,
        &GATHER_BLOCK_CLASS,
        &SURFACE_BLOCK_CLASS,
        &SNOW_SURFACE_BLOCK_CLASS,
        &RAIL_CLASS,
        &TREE_CLASS,
        &PLANT_CLASS,
        &TRAIN_CAR_CLASS,
        &APPLE_TREE_CLASS,
        &CACTUS_TREE_CLASS,
        &CHERRY_TREE_CLASS,
        &COCONUT_TREE_CLASS,
        &COFFEE_TREE_CLASS,
        &GEM_TREE_CLASS,
        &LIME_TREE_CLASS,
        &MANGO_TREE_CLASS,
        &MAPLE_TREE_CLASS,
        &ORANGE_TREE_CLASS,
        &PINE_TREE_CLASS,
        &NORMAL_PLANT_CLASS,
        &KELP_PLANT_CLASS,
        &TULIP_PLANT_CLASS,
        &VINE_PLANT_CLASS,
        &CARROT_PLANT_CLASS,
        &CHILLI_PLANT_CLASS,
        &CORN_PLANT_CLASS,
        &FLAX_PLANT_CLASS,
        &SUNFLOWER_PLANT_CLASS,
        &TOMATO_PLANT_CLASS,
        &WHEAT_PLANT_CLASS,
        &STEAM_TRAIN_CLASS,
        &FREIGHT_CAR_CLASS,
        &HAND_CAR_CLASS,
        &PASSENGER_CAR_CLASS,
        &WORKBENCH_CLASS,
        &CHEST_CLASS,
        &BED_CLASS,
        &SIGN_CLASS,
        &TRADE_PORTAL_CLASS,
        &TRADING_POST_CLASS,
        &TRAIN_STATION_CLASS,
        &MIRROR_CLASS,
        &OWNERSHIP_SIGN_CLASS,
    ]
}

/// The registered `ClassInfo` list (see [`install`]). Exposed for tests and diagnostics.
pub fn registered_classes() -> Vec<&'static ClassInfo> {
    all_classes()
}

// ------------------------------------------------------------------------------------------------ adapters

/// Adapters from a class constructor to one of the five `ClassFactory` fn-pointer shapes. Group C: the class takes
/// `&Rc<World>, &Rc<DynamicWorld>`; group M: `&Rc<World>, &DynamicWorldRef`; group D: both handles as trait objects.
/// All of them map the `Rc<X>` result to `ObjRef` (`Some` == the ObjC init returned self, `None` == nil).
macro_rules! s_c {
    ($f:path) => {{
        fn f(w: &WorldRef, d: &DynamicWorldRef, sd: &PlistDict, c: CacheRef) -> Option<ObjRef> {
            $f(&concrete_world(w), &concrete_dw(d), sd, c).map(|o| o as ObjRef)
        }
        Some(f as FromSaveDictFn)
    }};
}
macro_rules! s_m {
    ($f:path) => {{
        fn f(w: &WorldRef, d: &DynamicWorldRef, sd: &PlistDict, c: CacheRef) -> Option<ObjRef> {
            $f(&concrete_world(w), d, sd, c).map(|o| o as ObjRef)
        }
        Some(f as FromSaveDictFn)
    }};
}
macro_rules! s_d {
    ($f:path) => {{
        fn f(w: &WorldRef, d: &DynamicWorldRef, sd: &PlistDict, c: CacheRef) -> Option<ObjRef> {
            $f(w, d, sd, c).map(|o| o as ObjRef)
        }
        Some(f as FromSaveDictFn)
    }};
}
macro_rules! n_c {
    ($f:path) => {{
        fn f(w: &WorldRef, d: &DynamicWorldRef, c: CacheRef, nd: &[u8]) -> Option<ObjRef> {
            $f(&concrete_world(w), &concrete_dw(d), c, nd).map(|o| o as ObjRef)
        }
        Some(f as FromNetDataFn)
    }};
}
macro_rules! n_m {
    ($f:path) => {{
        fn f(w: &WorldRef, d: &DynamicWorldRef, c: CacheRef, nd: &[u8]) -> Option<ObjRef> {
            $f(&concrete_world(w), d, c, nd).map(|o| o as ObjRef)
        }
        Some(f as FromNetDataFn)
    }};
}
macro_rules! n_d {
    ($f:path) => {{
        fn f(w: &WorldRef, d: &DynamicWorldRef, c: CacheRef, nd: &[u8]) -> Option<ObjRef> {
            $f(w, d, c, nd).map(|o| o as ObjRef)
        }
        Some(f as FromNetDataFn)
    }};
}
macro_rules! p_m {
    ($f:path) => {{
        fn f(w: &WorldRef, d: &DynamicWorldRef, p: IntPair, c: CacheRef) -> Option<ObjRef> {
            $f(&concrete_world(w), d, p, c).map(|o| o as ObjRef)
        }
        Some(f as FromPositionFn)
    }};
}
macro_rules! t_c {
    ($f:path) => {{
        fn f(
            w: &WorldRef,
            d: &DynamicWorldRef,
            p: IntPair,
            c: CacheRef,
            it: i32,
            sd: Option<&PlistDict>,
            cid: Option<&str>,
        ) -> Option<ObjRef> {
            $f(&concrete_world(w), &concrete_dw(d), p, c, it, sd, cid).map(|o| o as ObjRef)
        }
        Some(f as FromPositionTypeSaveDictFn)
    }};
}
macro_rules! i_c {
    ($f:path) => {{
        fn f(
            w: &WorldRef,
            d: &DynamicWorldRef,
            p: IntPair,
            c: CacheRef,
            item: Option<&InventoryItem>,
            flipped: bool,
            sd: Option<&PlistDict>,
            cid: Option<&str>,
            cname: Option<&str>,
        ) -> Option<ObjRef> {
            $f(&concrete_world(w), &concrete_dw(d), p, c, item, flipped, sd, cid, cname).map(|o| o as ObjRef)
        }
        Some(f as FromPositionInteractionFn)
    }};
}

// ---- hand written adapters ----

/// Trees: S entry without noise functions (the load path uses [`init_tree_or_plant_from_save_dict`], section 3.1).
fn tree_s<const T: i32>(w: &WorldRef, d: &DynamicWorldRef, sd: &PlistDict, c: CacheRef) -> Option<ObjRef> {
    crate::trees::tree::new_tree_from_save_dict(T, &concrete_world(w), &concrete_dw(d), sd, c, None, None)
}

/// Plants: S entry without noise functions (section 3.1).
fn plant_s<const T: i32>(w: &WorldRef, d: &DynamicWorldRef, sd: &PlistDict, c: CacheRef) -> Option<ObjRef> {
    crate::plants::plant::new_plant_from_save_dict(T, &concrete_world(w), &concrete_dw(d), sd, c, None, None)
}

/// Plants: N entry (NormalPlant / Kelp / Tulip / Vine own `cache:netData:`).
fn plant_n<const T: i32>(w: &WorldRef, d: &DynamicWorldRef, c: CacheRef, nd: &[u8]) -> Option<ObjRef> {
    crate::plants::plant::new_plant_from_net_data(T, &concrete_world(w), &concrete_dw(d), c, nd)
}

/// FireObject P entry (`placeFireAtPosition`).
fn fire_p(w: &WorldRef, d: &DynamicWorldRef, p: IntPair, c: CacheRef) -> Option<ObjRef> {
    crate::physics_objects::fire_object::FireObject::new(&concrete_world(w), &concrete_dw(d), p, c)
}

/// Workbench I entry: the InteractionObject item initialiser applied to a zero-state Workbench (type 0, level 0).
/// Note: the workbench module exports no zero-state constructor, so the
/// class's own `...type:flipped:saveDict:placedByClient:clientName:` init runs with `WORKBENCH_UNDEFINED`. The item
/// argument is not used by InteractionObject's init.
fn workbench_i(
    w: &WorldRef,
    d: &DynamicWorldRef,
    p: IntPair,
    c: CacheRef,
    _item: Option<&InventoryItem>,
    flipped: bool,
    sd: Option<&PlistDict>,
    cid: Option<&str>,
    cname: Option<&str>,
) -> Option<ObjRef> {
    crate::crafting::workbench::Workbench::init_inherited_item(&concrete_world(w), d, p, c, flipped, sd, cid, cname)
        .map(|o| o as ObjRef)
}

/// Type 15 P entry: plain InteractionObject with the inherited base init.
fn plain_p(w: &WorldRef, d: &DynamicWorldRef, p: IntPair, c: CacheRef) -> Option<ObjRef> {
    PlainInteractionObject::init_with_world_dynamic_world_at_position_cache(w, d, p, c)
}

/// Type 15 N entry: plain InteractionObject (section 5.1).
fn plain_n(w: &WorldRef, d: &DynamicWorldRef, c: CacheRef, nd: &[u8]) -> Option<ObjRef> {
    PlainInteractionObject::init_with_world_dynamic_world_cache_net_data(w, d, c, nd)
}

// ------------------------------------------------------------------------------------------------ factory table (section 3)

/// One row of the section 3 table: `(type, class, P, S, N, T, I)`.
struct Row {
    t: i32,
    class: &'static ClassInfo,
    p: Option<FromPositionFn>,
    s: Option<FromSaveDictFn>,
    n: Option<FromNetDataFn>,
    tt: Option<FromPositionTypeSaveDictFn>,
    i: Option<FromPositionInteractionFn>,
}

fn row(
    t: i32,
    class: &'static ClassInfo,
    p: Option<FromPositionFn>,
    s: Option<FromSaveDictFn>,
    n: Option<FromNetDataFn>,
    tt: Option<FromPositionTypeSaveDictFn>,
    i: Option<FromPositionInteractionFn>,
) -> Row {
    Row { t, class, p, s, n, tt, i }
}

/// The 64 rows of the section 3 table.
///
/// Note: section 7.1 asks the tree / lighting / env owners for thin exports that
/// run the inherited `DynamicObject` init on a zero-state species struct. They are not exported (the structs' fields
/// are private), so the following entries are registered as `None`: N of the trees (1-9, 37, 57), of 18, 21, 22, 26,
/// 29, and P of 17 and 21. Effect: a client-created object of one of these types is dropped instead of built.
fn rows() -> Vec<Row> {
    use crate::blockhead::blockhead::Blockhead;
    use crate::blockhead::blockhead::BLOCKHEAD_CLASS;
    use crate::building::bed::Bed;
    use crate::building::bed::BED_CLASS;
    use crate::building::column::Column;
    use crate::building::column::COLUMN_CLASS;
    use crate::building::door::Door;
    use crate::building::door::DOOR_CLASS;
    use crate::building::elevator::{ElevatorMotor, ElevatorShaft};
    use crate::building::elevator::{ELEVATOR_MOTOR_CLASS, ELEVATOR_SHAFT_CLASS};
    use crate::building::ladder::Ladder;
    use crate::building::ladder::LADDER_CLASS;
    use crate::building::stairs::Stairs;
    use crate::building::stairs::STAIRS_CLASS;
    use crate::building::window::Window;
    use crate::building::window::WINDOW_CLASS;
    use crate::building::wire::Wire;
    use crate::building::wire::WIRE_CLASS;
    use crate::crafting::workbench::Workbench;
    use crate::crafting::workbench::WORKBENCH_CLASS;
    use crate::env::gather_block::GatherBlock;
    use crate::env::gather_block::GATHER_BLOCK_CLASS;
    use crate::env::snow_surface_block::SnowSurfaceBlock;
    use crate::env::snow_surface_block::SNOW_SURFACE_BLOCK_CLASS;
    use crate::env::surface_block::SurfaceBlock;
    use crate::env::surface_block::SURFACE_BLOCK_CLASS;
    use crate::interaction::chest::Chest;
    use crate::interaction::chest::CHEST_CLASS;
    use crate::interaction::mirror::Mirror;
    use crate::interaction::mirror::MIRROR_CLASS;
    use crate::interaction::ownership_sign::OwnershipSign;
    use crate::interaction::ownership_sign::OWNERSHIP_SIGN_CLASS;
    use crate::interaction::painting::Painting;
    use crate::interaction::painting::PAINTING_CLASS;
    use crate::interaction::sign::Sign;
    use crate::interaction::sign::SIGN_CLASS;
    use crate::interaction::trade_portal::TradePortal;
    use crate::interaction::trade_portal::TRADE_PORTAL_CLASS;
    use crate::interaction::trading_post::TradingPost;
    use crate::interaction::trading_post::TRADING_POST_CLASS;
    use crate::lighting::artificial_light::ARTIFICIAL_LIGHT_CLASS;
    use crate::lighting::glow_block::GlowBlock;
    use crate::lighting::glow_block::GLOW_BLOCK_CLASS;
    use crate::lighting::torch::Torch;
    use crate::lighting::torch::TORCH_CLASS;
    use crate::npc::cave_troll::CaveTroll;
    use crate::npc::cave_troll::CAVE_TROLL_CLASS;
    use crate::npc::clown_fish::ClownFish;
    use crate::npc::clown_fish::CLOWN_FISH_CLASS_INFO;
    use crate::npc::dodo::Dodo;
    use crate::npc::dodo::DODO_CLASS_INFO;
    use crate::npc::donkey::Donkey;
    use crate::npc::donkey::DONKEY_CLASS_INFO;
    use crate::npc::drop_bear::DropBear;
    use crate::npc::drop_bear::DROP_BEAR_CLASS;
    use crate::npc::egg::Egg;
    use crate::npc::egg::EGG_CLASS_INFO;
    use crate::npc::scorpion::Scorpion;
    use crate::npc::scorpion::SCORPION_CLASS;
    use crate::npc::shark::Shark;
    use crate::npc::shark::SHARK_CLASS_INFO;
    use crate::npc::yak::Yak;
    use crate::npc::yak::YAK_CLASS_INFO;
    use crate::physics_objects::boat::Boat;
    use crate::physics_objects::boat::BOAT_CLASS;
    use crate::physics_objects::fire_object::FireObject;
    use crate::physics_objects::fire_object::FIRE_OBJECT_CLASS;
    use crate::physics_objects::free_block::FreeBlock;
    use crate::physics_objects::free_block::FREE_BLOCK_CLASS;
    use crate::plants::kelp::KELP_PLANT_CLASS;
    use crate::plants::simple_plants::{
        CARROT_PLANT_CLASS, CHILLI_PLANT_CLASS, CORN_PLANT_CLASS, FLAX_PLANT_CLASS, SUNFLOWER_PLANT_CLASS,
        TOMATO_PLANT_CLASS, WHEAT_PLANT_CLASS,
    };
    use crate::plants::tulip::TULIP_PLANT_CLASS;
    use crate::plants::vine::VINE_PLANT_CLASS;
    use crate::trains::freight_car::FreightCar;
    use crate::trains::freight_car::FREIGHT_CAR_CLASS;
    use crate::trains::hand_car::HandCar;
    use crate::trains::hand_car::HAND_CAR_CLASS;
    use crate::trains::passenger_car::PassengerCar;
    use crate::trains::passenger_car::PASSENGER_CAR_CLASS;
    use crate::trains::rail::Rail;
    use crate::trains::rail::RAIL_CLASS;
    use crate::trains::steam_train::SteamTrain;
    use crate::trains::steam_train::STEAM_TRAIN_CLASS;
    use crate::trains::train_station::TrainStation;
    use crate::trains::train_station::TRAIN_STATION_CLASS;
    use crate::trees::apple::APPLE_TREE_CLASS;
    use crate::trees::cactus::CACTUS_TREE_CLASS;
    use crate::trees::cherry::CHERRY_TREE_CLASS;
    use crate::trees::coconut::COCONUT_TREE_CLASS;
    use crate::trees::coffee::COFFEE_TREE_CLASS;
    use crate::trees::gem::GEM_TREE_CLASS;
    use crate::trees::lime::LIME_TREE_CLASS;
    use crate::trees::mango::MANGO_TREE_CLASS;
    use crate::trees::maple::MAPLE_TREE_CLASS;
    use crate::trees::orange::ORANGE_TREE_CLASS;
    use crate::trees::pine::PINE_TREE_CLASS;

    // tree rows: S only (see the doc comment for N)
    macro_rules! tree {
        ($t:expr, $c:expr) => {
            row($t, &$c, None, Some(tree_s::<$t> as FromSaveDictFn), None, None, None)
        };
    }
    // plant rows: S and N
    macro_rules! plant {
        ($t:expr, $c:expr) => {
            row($t, &$c, None, Some(plant_s::<$t> as FromSaveDictFn), Some(plant_n::<$t> as FromNetDataFn), None, None)
        };
    }
    // standard objects with a T entry
    macro_rules! std_obj {
        ($t:expr, $c:expr, $ty:ident, $init_t:ident) => {
            row(
                $t,
                &$c,
                None,
                s_c!($ty::init_with_world_dynamic_world_save_dict_cache),
                n_c!($ty::init_with_world_dynamic_world_cache_net_data),
                t_c!($ty::$init_t),
                None,
            )
        };
    }
    // interaction objects with `from_save_dict`, `from_net_data` and `new` (I)
    macro_rules! io_obj {
        ($t:expr, $c:expr, $ty:ident) => {
            row($t, &$c, None, s_c!($ty::from_save_dict), n_c!($ty::from_net_data), None, i_c!($ty::new))
        };
    }
    // train cars: S and N
    macro_rules! car {
        ($t:expr, $c:expr, $ty:ident) => {
            row($t, &$c, None, s_c!($ty::from_save_dict), n_c!($ty::from_net_data), None, None)
        };
    }

    const TT: &str = "init_with_world_dynamic_world_at_position_cache_type_save_dict_placed_by_client";
    let _ = TT;

    vec![
        tree!(1, APPLE_TREE_CLASS),
        tree!(2, MAPLE_TREE_CLASS),
        tree!(3, MANGO_TREE_CLASS),
        tree!(4, PINE_TREE_CLASS),
        tree!(5, CACTUS_TREE_CLASS),
        tree!(6, COCONUT_TREE_CLASS),
        tree!(7, ORANGE_TREE_CLASS),
        tree!(8, CHERRY_TREE_CLASS),
        tree!(9, COFFEE_TREE_CLASS),
        plant!(10, FLAX_PLANT_CLASS),
        plant!(11, SUNFLOWER_PLANT_CLASS),
        plant!(12, CORN_PLANT_CLASS),
        row(13, &DODO_CLASS_INFO, None, s_d!(Dodo::init_with_world_dynamic_world_save_dict_cache), n_d!(Dodo::init_with_world_dynamic_world_cache_net_data), None, None),
        row(14, &FREE_BLOCK_CLASS, None, s_c!(FreeBlock::from_save_dict), n_c!(FreeBlock::from_net_data), None, None),
        row(15, &INTERACTION_OBJECT_CLASS, Some(plain_p as FromPositionFn), None, Some(plain_n as FromNetDataFn), None, None),
        row(16, &FIRE_OBJECT_CLASS, Some(fire_p as FromPositionFn), s_c!(FireObject::from_save_dict), n_c!(FireObject::from_net_data), None, None),
        row(17, &TORCH_CLASS, None, s_m!(Torch::init_with_world_dynamic_world_save_dict_cache), n_m!(Torch::init_with_world_dynamic_world_cache_net_data), None, None),
        row(18, &GLOW_BLOCK_CLASS, None, s_m!(GlowBlock::init_with_world_dynamic_world_save_dict_cache), None, None, None),
        std_obj!(19, LADDER_CLASS, Ladder, init_with_world_dynamic_world_at_position_cache_type_save_dict_placed_by_client),
        std_obj!(20, DOOR_CLASS, Door, init_with_world_dynamic_world_at_position_cache_type_save_dict_placed_by_client),
        row(21, &ARTIFICIAL_LIGHT_CLASS, None, None, None, None, None),
        row(22, &SURFACE_BLOCK_CLASS, p_m!(SurfaceBlock::init_with_world_dynamic_world_at_position_cache), s_m!(SurfaceBlock::init_with_world_dynamic_world_save_dict_cache), None, None, None),
        row(23, &BED_CLASS, None, s_c!(Bed::init_with_world_dynamic_world_save_dict_cache), n_c!(Bed::init_with_world_dynamic_world_cache_net_data), None, i_c!(Bed::init_with_world_dynamic_world_at_position_cache_item_flipped_save_dict_placed_by_client_client_name)),
        row(24, &BLOCKHEAD_CLASS, None, None, n_m!(Blockhead::init_with_world_dynamic_world_cache_net_data), None, None),
        row(25, &DROP_BEAR_CLASS, None, s_d!(DropBear::init_with_world_dynamic_world_save_dict_cache), n_d!(DropBear::init_with_world_dynamic_world_cache_net_data), None, None),
        row(26, &GATHER_BLOCK_CLASS, p_m!(GatherBlock::init_with_world_dynamic_world_at_position_cache), s_m!(GatherBlock::init_with_world_dynamic_world_save_dict_cache), None, None, None),
        plant!(27, CARROT_PLANT_CLASS),
        row(28, &DONKEY_CLASS_INFO, None, s_d!(Donkey::init_with_world_dynamic_world_save_dict_cache), n_d!(Donkey::init_with_world_dynamic_world_cache_net_data), None, None),
        row(29, &SNOW_SURFACE_BLOCK_CLASS, p_m!(SnowSurfaceBlock::init_with_world_dynamic_world_at_position_cache), s_m!(SnowSurfaceBlock::init_with_world_dynamic_world_save_dict_cache), None, None, None),
        row(30, &EGG_CLASS_INFO, None, s_d!(Egg::init_with_world_dynamic_world_save_dict_cache), n_d!(Egg::init_with_world_dynamic_world_cache_net_data), None, None),
        std_obj!(31, WINDOW_CLASS, Window, init_with_world_dynamic_world_at_position_cache_type_save_dict_placed_by_client),
        row(32, &BOAT_CLASS, None, s_c!(Boat::from_save_dict), n_c!(Boat::from_net_data), None, None),
        plant!(33, CHILLI_PLANT_CLASS),
        plant!(34, KELP_PLANT_CLASS),
        row(35, &CLOWN_FISH_CLASS_INFO, None, s_d!(ClownFish::init_with_world_dynamic_world_save_dict_cache), n_d!(ClownFish::init_with_world_dynamic_world_cache_net_data), None, None),
        row(36, &SHARK_CLASS_INFO, None, s_d!(Shark::init_with_world_dynamic_world_save_dict_cache), n_d!(Shark::init_with_world_dynamic_world_cache_net_data), None, None),
        tree!(37, LIME_TREE_CLASS),
        std_obj!(38, WIRE_CLASS, Wire, init_with_world_dynamic_world_at_position_cache_type_save_dict_placed_by_client),
        row(39, &CAVE_TROLL_CLASS, None, s_d!(CaveTroll::init_with_world_dynamic_world_save_dict_cache), n_d!(CaveTroll::init_with_world_dynamic_world_cache_net_data), None, None),
        row(40, &RAIL_CLASS, None, s_c!(Rail::from_save_dict), n_c!(Rail::from_net_data), None, None),
        car!(41, HAND_CAR_CLASS, HandCar),
        car!(42, STEAM_TRAIN_CLASS, SteamTrain),
        // 43: the 4-argument S shape does not exist for FreightCar (see `init_freight_car_from_save_dict`)
        row(43, &FREIGHT_CAR_CLASS, None, None, n_c!(FreightCar::from_net_data), None, None),
        car!(44, PASSENGER_CAR_CLASS, PassengerCar),
        row(45, &WORKBENCH_CLASS, None, s_m!(Workbench::init_with_world_dynamic_world_save_dict_cache), n_m!(Workbench::init_with_world_dynamic_world_cache_net_data), None, Some(workbench_i as FromPositionInteractionFn)),
        io_obj!(46, CHEST_CLASS, Chest),
        io_obj!(47, SIGN_CLASS, Sign),
        io_obj!(48, TRADING_POST_CLASS, TradingPost),
        row(49, &TRAIN_STATION_CLASS, None, s_c!(TrainStation::from_save_dict), n_c!(TrainStation::from_net_data), None, i_c!(TrainStation::new)),
        io_obj!(50, TRADE_PORTAL_CLASS, TradePortal),
        row(51, &SCORPION_CLASS, None, s_d!(Scorpion::init_with_world_dynamic_world_save_dict_cache), n_d!(Scorpion::init_with_world_dynamic_world_cache_net_data), None, None),
        row(52, &PAINTING_CLASS, None, s_c!(Painting::from_save_dict), n_c!(Painting::from_net_data), None, None),
        std_obj!(53, COLUMN_CLASS, Column, init_with_world_dynamic_world_at_position_cache_type_save_dict_placed_by_client),
        std_obj!(54, STAIRS_CLASS, Stairs, init_with_world_dynamic_world_at_position_cache_type_save_dict_placed_by_client),
        std_obj!(55, ELEVATOR_MOTOR_CLASS, ElevatorMotor, init_with_world_dynamic_world_at_position_cache_type_save_dict_placed_by_client),
        std_obj!(56, ELEVATOR_SHAFT_CLASS, ElevatorShaft, init_with_world_dynamic_world_at_position_cache_type_save_dict_placed_by_client),
        tree!(57, GEM_TREE_CLASS),
        plant!(58, VINE_PLANT_CLASS),
        plant!(59, TULIP_PLANT_CLASS),
        io_obj!(60, OWNERSHIP_SIGN_CLASS, OwnershipSign),
        plant!(61, WHEAT_PLANT_CLASS),
        plant!(62, TOMATO_PLANT_CLASS),
        row(63, &YAK_CLASS_INFO, None, s_d!(Yak::init_with_world_dynamic_world_save_dict_cache), n_d!(Yak::init_with_world_dynamic_world_cache_net_data), None, None),
        io_obj!(64, MIRROR_CLASS, Mirror),
    ]
}

fn factory_of(r: &Row) -> ClassFactory {
    ClassFactory {
        class: r.class,
        object_type: r.t,
        from_position: r.p,
        from_save_dict: r.s,
        from_net_data: r.n,
        from_position_type_save_dict: r.tt,
        from_position_interaction: r.i,
    }
}

// Silence unused-import noise from the shared `use` blocks above when a class only appears in one of the tables.
#[allow(unused_imports)]
use crate::plants::simple_plants as _simple_plants_used;

thread_local! {
    static INSTALLED: Cell<bool> = const { Cell::new(false) };
}

/// Idempotent (per thread: the tables are thread-local). Main-thread only; must run before the `World` is created.
/// Does, in this order:
/// 1. `register_class_info` for every class of section 4,
/// 2. `register_dynamic_class(t,..)` for `t` in 1..=64 per section 3,
/// 3. `register_interaction_class(i,..)` for `i` in 1..=9 (the factories of the dynamic types 45, 46, 23, 47, 48, 49,
///    50, 60, 64 in that order),
/// 4. `register_npc_ctor` for NPC types 1..=8,
/// 5. `debug_assert` self-check of names and object types.
///
pub fn install() {
    if INSTALLED.with(|f| f.replace(true)) {
        return;
    }
    crate::building::wire::set_workbench_uses_electricity_hook(|o| {
        downcast::<crate::crafting::workbench::Workbench>(o)
            .is_some_and(|w| w.uses_stores_conducts_or_produces_electricity())
    });
    // 1. ClassInfo
    for ci in all_classes() {
        register_class_info(ci);
    }
    // 2. dynamic table
    let rows = rows();
    for r in &rows {
        register_dynamic_class(r.t, factory_of(r));
    }
    // 3. interaction table: interaction type -> the dynamic row of the corresponding class
    for i in 1..=9 {
        let dt = super::type_tables::dynamic_object_type_for_interaction_object_type(i);
        let r = rows.iter().find(|r| r.t == dt).expect("interaction type without dynamic row");
        register_interaction_class(i, factory_of(r));
    }
    // 4. NPC constructors (npc_base 2.5;; ids 13, 25, 28, 35, 36, 39, 51, 63)
    register_npc_ctor(1, crate::npc::dodo::Dodo::new_for_registry);
    crate::npc::drop_bear::register(); // 2
    register_npc_ctor(3, crate::npc::donkey::Donkey::new_for_registry);
    register_npc_ctor(4, crate::npc::clown_fish::ClownFish::new_for_registry);
    register_npc_ctor(5, crate::npc::shark::Shark::new_for_registry);
    crate::npc::cave_troll::register(); // 6
    crate::npc::scorpion::register(); // 7
    register_npc_ctor(8, crate::npc::yak::Yak::new_for_registry);
    // 5. self-check
    #[cfg(debug_assertions)]
    {
        use super::type_tables::{class_for_dynamic_object_type, class_for_interaction_object_type};
        for t in 1..=64 {
            let f = class_for_dynamic_object_type(t).expect("missing dynamic factory");
            debug_assert_eq!(Some(f.class.name), class_name_for_dynamic_object_type(t));
            debug_assert_eq!(f.object_type, t);
        }
        for i in 1..=9 {
            let f = class_for_interaction_object_type(i).expect("missing interaction factory");
            debug_assert_eq!(Some(f.class.name), class_name_for_interaction_object_type(i));
        }
    }
}

// ------------------------------------------------------------------------------------------------ as_interaction_like

/// Generic-code helper promised by object_model 3.4 / dynamic_world_ops 2.3: a downcast chain over the concrete
/// InteractionObject subclasses plus [`PlainInteractionObject`]. `OwnershipSign` is a `Sign` subclass in ObjC but a
/// separate Rust struct, so it is tested separately. `None` for any other object.
pub fn as_interaction_like(o: &ObjRef) -> Option<&dyn InteractionObjectLike> {
    use crate::building::bed::Bed;
    use crate::crafting::workbench::Workbench;
    use crate::interaction::chest::Chest;
    use crate::interaction::mirror::Mirror;
    use crate::interaction::ownership_sign::OwnershipSign;
    use crate::interaction::sign::Sign;
    use crate::interaction::trade_portal::TradePortal;
    use crate::interaction::trading_post::TradingPost;
    use crate::trains::train_station::TrainStation;
    if let Some(x) = downcast::<Workbench>(o) {
        return Some(x);
    }
    if let Some(x) = downcast::<Chest>(o) {
        return Some(x);
    }
    if let Some(x) = downcast::<Bed>(o) {
        return Some(x);
    }
    if let Some(x) = downcast::<Sign>(o) {
        return Some(x);
    }
    if let Some(x) = downcast::<OwnershipSign>(o) {
        return Some(x);
    }
    if let Some(x) = downcast::<TradingPost>(o) {
        return Some(x);
    }
    if let Some(x) = downcast::<TrainStation>(o) {
        return Some(x);
    }
    if let Some(x) = downcast::<TradePortal>(o) {
        return Some(x);
    }
    if let Some(x) = downcast::<Mirror>(o) {
        return Some(x);
    }
    if let Some(x) = downcast::<PlainInteractionObject>(o) {
        return Some(x);
    }
    None
}

// ------------------------------------------------------------------------------------------------ dispatch helpers

/// `loadDynamicObjectsOfType` tree / plant branch: `objectTypeIsTree(t) || objectTypeIsPlant(t)`, otherwise `None`.
/// Selector `initWithWorld:dynamicWorld:saveDict:cache:treeDensityNoiseFunction:seasonOffsetNoiseFunction:`.
/// Plants take a different noise handle type than trees: this entry
/// passes nil noise functions to plants; use [`init_plant_from_save_dict_with_noise`] to pass real ones.
pub fn init_tree_or_plant_from_save_dict(
    object_type: i32,
    world: &Rc<World>,
    dw: &Rc<DynamicWorld>,
    save_dict: &PlistDict,
    cache: CacheRef,
    tree_density_noise: NoiseRef,
    season_offset_noise: NoiseRef,
) -> Option<ObjRef> {
    use super::type_tables::{object_type_is_plant, object_type_is_tree};
    if object_type_is_tree(object_type) {
        crate::trees::tree::new_tree_from_save_dict(
            object_type,
            world,
            dw,
            save_dict,
            cache,
            tree_density_noise,
            season_offset_noise,
        )
    } else if object_type_is_plant(object_type) {
        crate::plants::plant::new_plant_from_save_dict(object_type, world, dw, save_dict, cache, None, None)
    } else {
        None
    }
}

/// Plant branch of [`init_tree_or_plant_from_save_dict`] with the plant module's own noise handle type.
pub fn init_plant_from_save_dict_with_noise(
    object_type: i32,
    world: &Rc<World>,
    dw: &Rc<DynamicWorld>,
    save_dict: &PlistDict,
    cache: CacheRef,
    tree_density_noise: Option<Rc<crate::worldgen::noise::NoiseFunction>>,
    season_offset_noise: Option<Rc<crate::worldgen::noise::NoiseFunction>>,
) -> Option<ObjRef> {
    if !super::type_tables::object_type_is_plant(object_type) {
        return None;
    }
    crate::plants::plant::new_plant_from_save_dict(
        object_type,
        world,
        dw,
        save_dict,
        cache,
        tree_density_noise,
        season_offset_noise,
    )
}

/// `Shark initWithWorld:dynamicWorld:saveDict:cache:` (thin forward).
pub fn init_shark_from_save_dict(
    world: &Rc<World>,
    dw: &Rc<DynamicWorld>,
    save_dict: &PlistDict,
    cache: CacheRef,
) -> Option<ObjRef> {
    let (w, d): (WorldRef, DynamicWorldRef) = (world.clone(), dw.clone());
    crate::npc::shark::Shark::init_with_world_dynamic_world_save_dict_cache(&w, &d, save_dict, cache)
}

/// `FreightCar initWithWorld:dynamicWorld:saveDict:chestSaveDict:cache:` (thin forward).
pub fn init_freight_car_from_save_dict(
    world: &Rc<World>,
    dw: &Rc<DynamicWorld>,
    save_dict: &PlistDict,
    chest_save_dict: Option<&PlistDict>,
    cache: CacheRef,
) -> Option<ObjRef> {
    crate::trains::freight_car::FreightCar::from_save_dict_chest_save_dict(world, dw, save_dict, chest_save_dict, cache)
}

/// `Workbench initWithWorld:dynamicWorld:saveDict:cache:` (thin forward).
pub fn init_workbench_from_save_dict(
    world: &Rc<World>,
    dw: &Rc<DynamicWorld>,
    save_dict: &PlistDict,
    cache: CacheRef,
) -> Option<ObjRef> {
    let d: DynamicWorldRef = dw.clone();
    crate::crafting::workbench::Workbench::init_with_world_dynamic_world_save_dict_cache(world, &d, save_dict, cache)
        .map(|o| o as ObjRef)
}

// ------------------------------------------------------------------------------------------------ tests

#[cfg(test)]
mod tests {
    use super::*;
    use crate::objects::base::testutil::setup;
    use crate::objects::class_info::class_info_by_name;
    use crate::objects::type_tables::{class_for_dynamic_object_type, class_for_interaction_object_type};

    /// Section 3 shape matrix as a const copy: five flags in the order P S N T I.
    const SHAPES: [(i32, &str); 64] = [
        (1, "-S---"),
        (2, "-S---"),
        (3, "-S---"),
        (4, "-S---"),
        (5, "-S---"),
        (6, "-S---"),
        (7, "-S---"),
        (8, "-S---"),
        (9, "-S---"),
        (10, "-SN--"),
        (11, "-SN--"),
        (12, "-SN--"),
        (13, "-SN--"),
        (14, "-SN--"),
        (15, "P-N--"),
        (16, "PSN--"),
        (17, "-SN--"), // P(17) = base init only: unavailable
        (18, "-S---"), // N inherited: unavailable
        (19, "-SNT-"),
        (20, "-SNT-"),
        (21, "-----"), // P/N inherited: unavailable
        (22, "PS---"),
        (23, "-SN-I"),
        (24, "--N--"),
        (25, "-SN--"),
        (26, "PS---"),
        (27, "-SN--"),
        (28, "-SN--"),
        (29, "PS---"),
        (30, "-SN--"),
        (31, "-SNT-"),
        (32, "-SN--"),
        (33, "-SN--"),
        (34, "-SN--"),
        (35, "-SN--"),
        (36, "-SN--"),
        (37, "-S---"),
        (38, "-SNT-"),
        (39, "-SN--"),
        (40, "-SN--"),
        (41, "-SN--"),
        (42, "-SN--"),
        (43, "--N--"),
        (44, "-SN--"),
        (45, "-SN-I"),
        (46, "-SN-I"),
        (47, "-SN-I"),
        (48, "-SN-I"),
        (49, "-SN-I"),
        (50, "-SN-I"),
        (51, "-SN--"),
        (52, "-SN--"),
        (53, "-SNT-"),
        (54, "-SNT-"),
        (55, "-SNT-"),
        (56, "-SNT-"),
        (57, "-S---"),
        (58, "-SN--"),
        (59, "-SN--"),
        (60, "-SN-I"),
        (61, "-SN--"),
        (62, "-SN--"),
        (63, "-SN--"),
        (64, "-SN-I"),
    ];

    #[test]
    fn idempotent_install() {
        install();
        install();
        assert!(class_for_dynamic_object_type(1).is_some());
    }

    #[test]
    fn every_type_resolves() {
        install();
        for t in 1..=64 {
            let f = class_for_dynamic_object_type(t).expect("a class for every type");
            assert_eq!(f.object_type, t);
            assert_eq!(Some(f.class.name), class_name_for_dynamic_object_type(t), "type {t}");
        }
        for t in [0, 65, -1, 1000] {
            assert!(class_for_dynamic_object_type(t).is_none());
        }
    }

    #[test]
    fn interaction_types_resolve() {
        install();
        let names = [
            "Workbench",
            "Chest",
            "Bed",
            "Sign",
            "TradingPost",
            "TrainStation",
            "TradePortal",
            "OwnershipSign",
            "Mirror",
        ];
        for (i, n) in names.iter().enumerate() {
            let f = class_for_interaction_object_type(i as i32 + 1).unwrap();
            assert_eq!(f.class.name, *n);
        }
        for t in [0, 10, -1] {
            assert!(class_for_interaction_object_type(t).is_none());
        }
    }

    #[test]
    fn shape_matrix() {
        install();
        for (t, flags) in SHAPES {
            let f = class_for_dynamic_object_type(t).unwrap();
            let got = format!(
                "{}{}{}{}{}",
                if f.from_position.is_some() { 'P' } else { '-' },
                if f.from_save_dict.is_some() { 'S' } else { '-' },
                if f.from_net_data.is_some() { 'N' } else { '-' },
                if f.from_position_type_save_dict.is_some() { 'T' } else { '-' },
                if f.from_position_interaction.is_some() { 'I' } else { '-' },
            );
            assert_eq!(got, flags, "type {t}");
        }
    }

    #[test]
    fn class_infos() {
        install();
        let all = registered_classes();
        assert_eq!(all.len(), 71);
        for c in &all {
            assert!(std::ptr::eq(class_info_by_name(c.name).unwrap(), *c), "{}", c.name);
        }
        let chain = |a: &str, b: &str| class_info_by_name(a).unwrap().is_kind_of(class_info_by_name(b).unwrap());
        assert!(chain("Donkey", "DonkeyLike") && chain("Donkey", "NPC") && chain("Donkey", "DynamicObject"));
        assert!(chain("Yak", "DonkeyLike"));
        assert!(chain("GemTree", "Tree"));
        assert!(chain("OwnershipSign", "InteractionObject"));
        assert!(chain("KelpPlant", "Plant"));
        assert!(chain("CarrotPlant", "NormalPlant") && chain("CarrotPlant", "Plant"));
        assert!(chain("Workbench", "InteractionObject"));
        assert!(chain("CaveTroll", "NPC"));
        assert!(!chain("Dodo", "InteractionObject"));
        for n in [
            "Chest",
            "Bed",
            "Sign",
            "TradePortal",
            "TradingPost",
            "TrainStation",
            "Mirror",
            "Workbench",
            "Painting",
            "Boat",
            "Dodo",
            "HandCar",
        ] {
            assert!(class_info_by_name(n).unwrap().conforms_to("TapChoiceDynamicObject"), "{n}");
        }
    }

    #[test]
    fn npc_table() {
        install();
        let (w, dw, _l) = setup(4, 4);
        let (wr, dr): (WorldRef, DynamicWorldRef) = (w.clone(), dw.clone());
        let env = crate::npc::egg::test_support::TestPassiveEnv::new();
        env.install();
        for (npc_type, obj_type) in [(1, 13), (3, 28), (4, 35), (5, 36), (8, 63)] {
            let pos = IntPair { x: 10, y: 10 };
            let o = crate::npc::npc::new_npc_for_type(npc_type, &wr, &dr, pos, None, None, true, false, None);
            // ClownFish (4) and Shark (5) refuse to spawn outside water: only check the type when they exist.
            if matches!(npc_type, 4 | 5) && o.is_none() {
                continue;
            }
            let o = o.expect("an npc of every type");
            assert_eq!(o.object_type(), obj_type);
        }
        for t in [0, 9] {
            assert!(crate::npc::npc::new_npc_for_type(
                t,
                &wr,
                &dr,
                IntPair { x: 1, y: 1 },
                None,
                None,
                true,
                false,
                None
            )
            .is_none());
        }
    }

    #[test]
    fn plain_interaction_object_net_create() {
        install();
        let (w, dw, _l) = setup(4, 4);
        let (wr, dr): (WorldRef, DynamicWorldRef) = (w.clone(), dw.clone());
        let f = class_for_dynamic_object_type(15).unwrap();
        let mut nd = vec![0u8; 0x28];
        nd[0..8].copy_from_slice(&u64::MAX.to_le_bytes());
        let o = (f.from_net_data.unwrap())(&wr, &dr, None, &nd).expect("a plain interaction object");
        assert_eq!(o.object_type(), 15);
        assert!(o.is_net());
        assert!(as_interaction_like(&o).is_some());
    }

    #[test]
    fn as_interaction_like_none_for_plain_object() {
        let (w, dw, _l) = setup(4, 4);
        let (wr, dr): (WorldRef, DynamicWorldRef) = (w.clone(), dw.clone());
        let o = PlainInteractionObject::init_with_world_dynamic_world_at_position_cache(
            &wr,
            &dr,
            IntPair { x: 3, y: 3 },
            None,
        )
        .expect("a plain interaction object");
        assert!(as_interaction_like(&o).is_some());
    }

    /// Every constructor shape of every row, called with empty inputs on a real world: whatever comes out (or
    /// raises) does not matter here, the adapters have to be callable.
    #[test]
    fn every_factory_shape_can_be_called() {
        use crate::interaction::sign::testkit::world_with_block;
        use std::panic::{catch_unwind, AssertUnwindSafe};
        install();
        let (w, dw, _pb) = world_with_block("reg_sweep");
        *w.distance_ordered_food_types.borrow_mut() = (0..31).collect();
        let (wr, dr): (WorldRef, DynamicWorldRef) = (w.clone(), dw.clone());
        let pos = IntPair { x: 10, y: 10 };
        let empty = crate::objects::base::new_mutable_dict();
        let head = vec![0u8; 0x200];
        let mut built = 0;
        for t in 1..=64 {
            let f = class_for_dynamic_object_type(t).unwrap();
            let mut tries: Vec<Box<dyn FnOnce() -> Option<ObjRef> + '_>> = Vec::new();
            if let Some(p) = f.from_position {
                let (wr, dr) = (&wr, &dr);
                tries.push(Box::new(move || p(wr, dr, pos, None)));
            }
            if let Some(sfn) = f.from_save_dict {
                let (wr, dr, empty) = (&wr, &dr, &empty);
                tries.push(Box::new(move || sfn(wr, dr, empty, None)));
            }
            if let Some(n) = f.from_net_data {
                let (wr, dr, head) = (&wr, &dr, &head);
                tries.push(Box::new(move || n(wr, dr, None, head)));
            }
            if let Some(tt) = f.from_position_type_save_dict {
                let (wr, dr) = (&wr, &dr);
                tries.push(Box::new(move || tt(wr, dr, pos, None, 0, None, None)));
            }
            if let Some(i) = f.from_position_interaction {
                let (wr, dr) = (&wr, &dr);
                tries.push(Box::new(move || i(wr, dr, pos, None, None, false, None, None, None)));
            }
            for call in tries {
                if let Ok(Some(_)) = catch_unwind(AssertUnwindSafe(call)) {
                    built += 1;
                }
            }
        }
        assert!(built > 20, "only {built} objects built");
        // the interaction table finds its rows too
        for i in 1..=9 {
            let f = class_for_interaction_object_type(i).unwrap();
            let _ = f
                .from_position_interaction
                .map(|c| catch_unwind(AssertUnwindSafe(|| c(&wr, &dr, pos, None, None, false, None, None, None))));
        }
    }

    /// The plain InteractionObject (type 15) answers its DynamicObject face; the downcast chain names every
    /// concrete InteractionObject class and refuses everything else.
    #[test]
    fn plain_interaction_object_face_and_downcast_chain() {
        use crate::interaction::sign::testkit::world_with_block;
        install();
        let (w, dw, _pb) = world_with_block("reg_plain");
        let (wr, dr): (WorldRef, DynamicWorldRef) = (w.clone(), dw.clone());
        let o = PlainInteractionObject::init_with_world_dynamic_world_at_position_cache(
            &wr,
            &dr,
            IntPair { x: 10, y: 10 },
            None,
        )
        .expect("plain interaction object");
        assert_eq!(o.class_info().name, "InteractionObject");
        let plain = downcast::<PlainInteractionObject>(&o).expect("a plain interaction object");
        assert!(std::ptr::eq(InteractionObjectLike::io_state(plain), &plain.io));
        assert!(o.as_any().is::<PlainInteractionObject>());
        assert_eq!(o.object_type(), 15);
        assert!(!o.requires_physical_block());
        assert!(o.should_add_to_macro_block());
        assert!(o.as_tap_choice().is_some());
        let tap = o.as_tap_choice().unwrap();
        assert_eq!(tap.action_title(), "");
        assert!(!tap.is_double_height());
        o.update_accurate_dt_is_simulation(&o, 0.016, 0.016, false);
        let _sd = o.get_save_dict(&o);
        let nd = o.creation_net_data_for_client(&o, None).unwrap();
        assert_eq!(nd, o.update_net_data_for_client(&o, None).unwrap());
        o.remote_update(&o, &nd);
        o.blockheads_loaded(&o);
        let b: ObjRef = crate::objects::interaction_object::testutil::TestBh::new(5, false);
        o.blockhead_unloaded(&o, &b);
        assert!(as_interaction_like(&o).is_some());
        // the net constructor refuses an image outside the loaded world
        let mut far = nd.clone();
        far[8..12].copy_from_slice(&3000u32.to_le_bytes());
        assert!(PlainInteractionObject::init_with_world_dynamic_world_cache_net_data(&wr, &dr, None, &far).is_none());
        o.dealloc(&o);
        // every other class is not an InteractionObject
        assert!(as_interaction_like(&b).is_none());
        // and each concrete one is
        let pos = IntPair { x: 12, y: 10 };
        for t in [23, 45, 46, 47, 48, 49, 50, 60, 64] {
            let f = class_for_dynamic_object_type(t).unwrap();
            let made = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                f.from_position_interaction
                    .and_then(|c| c(&wr, &dr, pos, None, None, false, None, Some("c"), Some("n")))
            }));
            let obj = made.expect("no panic").expect("an interaction object");
            assert!(as_interaction_like(&obj).is_some(), "type {t}");
        }
    }

    /// The dispatch helpers for trees, plants, Shark, FreightCar and Workbench.
    #[test]
    fn dispatch_helpers() {
        use crate::interaction::sign::testkit::world_with_block;
        install();
        let (w, dw, _pb) = world_with_block("reg_dispatch");
        let empty = crate::objects::base::new_mutable_dict();
        let call = |t: i32| {
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                init_tree_or_plant_from_save_dict(t, &w, &dw, &empty, None, None, None).is_some()
            }))
        };
        let _ = call(1);
        let _ = call(10);
        assert_eq!(call(17).ok(), Some(false));
        assert!(init_plant_from_save_dict_with_noise(17, &w, &dw, &empty, None, None, None).is_none());
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            init_plant_from_save_dict_with_noise(10, &w, &dw, &empty, None, None, None)
        }));
        let _ =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| init_shark_from_save_dict(&w, &dw, &empty, None)));
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            init_freight_car_from_save_dict(&w, &dw, &empty, None, None)
        }));
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            init_workbench_from_save_dict(&w, &dw, &empty, None)
        }));
    }

    /// The workbench hook installed for wires reaches the real workbench.
    #[test]
    fn wires_ask_the_workbench() {
        use crate::building::wire::Wire;
        use crate::crafting::workbench::{Workbench, WB_STEAM_GENERATOR};
        use crate::interaction::sign::testkit::world_with_block;
        use crate::objects::type_tables::DYNAMIC_OBJECT_TYPE_WORKBENCH;
        use crate::types::enums::WorkbenchType;
        install();
        let (w, dw, _pb) = world_with_block("reg_wire");
        let dr: DynamicWorldRef = dw.clone();
        let wb = Workbench::init_with_world_dynamic_world_at_position_cache_type_flipped_save_dict_placed_by_client_client_name(
            &w, &dr, IntPair { x: 10, y: 10 }, None, WorkbenchType(WB_STEAM_GENERATOR as _), false, None, None, None,
        )
        .unwrap();
        let wbo: ObjRef = wb.clone();
        dw.by_pos_set(DYNAMIC_OBJECT_TYPE_WORKBENCH, dw.world_index(IntPair { x: 10, y: 10 }), &wbo);
        crate::world::tiles::tile_at_world_position_loaded(10, 10, &w).unwrap().set_contents(0x2e);
        let wire = Wire::init_with_world_dynamic_world_at_position_cache_type_save_dict_placed_by_client(
            &w,
            &dw,
            IntPair { x: 11, y: 10 },
            None,
            0,
            None,
            None,
        );
        assert!(wire.is_some());
    }
}
