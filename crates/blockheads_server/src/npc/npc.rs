//! `NPC`: the abstract base class of every animal / creature (Dodo, DropBear, Donkey/Yak, ClownFish, Shark,
//! CaveTroll, Scorpion).
//!
//!
//! ## Shape
//! * [`NpcState`] holds the ivars `0x4e..0x117`; a concrete class embeds it (`npc: RefCell<NpcState>`).
//! * [`Npc`] is the virtual-dispatch trait for every NPC selector that is not a `DynamicObject` selector. Its
//!   default bodies call the `npc_*` free functions, which are also the `[super...]` targets of subclasses.
//! * [`as_npc`] reaches the [`Npc`] view of an [`ObjRef`]
//! * [`NpcEnv`] is the seam to the layers that did not exist when this module was written (day/night, weather,
//!   `DynamicWorld` queries beyond `DynamicWorldApi`, `Blockhead` calls, `BHServer` profanity filter, chests).
//! * [`impl_npc_protocols!`] / [`npc_forward!`] remove the per-class boilerplate.
//!
//! ## RNG
//! All randomness is libc `rand()` through `rng::glibc_rand` (`R01 = (float)rand() / 2147483648.0f`).
//! The draw order of every function is preserved.

use std::any::{Any, TypeId};
use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use std::rc::Rc;

use crate::fmath::cconv::{f32_to_i16, f32_to_i32};
use crate::fmath::vector::clamp;
use crate::foundation::exception::unrecognized_selector;
use crate::foundation::nsstring;
use crate::nslog;
use crate::objects::base::{
    base_get_save_dict, base_init_with_world_dynamic_world_at_position_cache,
    base_init_with_world_dynamic_world_cache_net_data, base_init_with_world_dynamic_world_save_dict_cache,
    base_remote_creation_data_update, base_remote_update, base_update_position, new_mutable_dict, padded_bytes,
    DynamicObject, DynamicWorldRef, WorldRef,
};
use crate::objects::class_info::{ClassInfo, DYNAMIC_OBJECT_CLASS};
use crate::objects::handle::{alloc_obj, ptr_eq, CacheRef, ObjRef, ObjWeak};
use crate::plist::helpers::{data_from_property_list, property_list_with_data_raw};
use crate::plist::value::{PlistDict, PlistValue};
use crate::rng::glibc_rand;
use crate::types::enums::NPCType;
use crate::types::netdata::{NPCCreationNetData, NPCUpdateNetData};
use crate::types::structs::{IntPair, Vector2, WireStruct};
use crate::types::tile::Tile;
use crate::world::state::World;

// ------------------------------------------------------------------------------------------------ constants

/// `NPCType` values. Binding values from.
pub const NPC_NOTHING: i32 = 0;
pub const NPC_DODO: i32 = 1;
pub const NPC_DROPBEAR: i32 = 2;
pub const NPC_DONKEY: i32 = 3;
pub const NPC_CLOWNFISH: i32 = 4;
pub const NPC_SHARK: i32 = 5;
pub const NPC_CAVETROLL: i32 = 6;
pub const NPC_SCORPION: i32 = 7;
pub const NPC_YAK: i32 = 8;

/// `SendState` (`NPC.h:138`).
pub const SEND_STATE_NONE: i32 = 0;
pub const SEND_STATE_SEND: i32 = 1;
pub const SEND_STATE_SENT: i32 = 2;

/// `NetNPCTameType` (`NPC.h:132`).
pub const NPC_NOT_TAMED: u8 = 0;
pub const NPC_TAMED_BY_THIS_CLIENT: u8 = 1;
pub const NPC_TAMED_BY_OTHER_CLIENT: u8 = 2;

/// `NetNPCInteractionType` (`NPC.h:144`).
pub const NET_NPC_INTERACTION_UNDEFINED: u8 = 0;
pub const NET_NPC_INTERACTION_FEED: u8 = 1;
pub const NET_NPC_INTERACTION_SET_FREE: u8 = 2;
pub const NET_NPC_INTERACTION_CAPTURE: u8 = 3;
pub const NET_NPC_INTERACTION_HARVEST_MILK: u8 = 4;
pub const NET_NPC_INTERACTION_HARVEST_SHAVE: u8 = 5;

/// `TILE_ICE` / `TILE_LAVA` type indices.
const TILE_ICE: u8 = 4;
const TILE_LAVA: u8 = 31;

/// `md5hex("server")`: `[dynamicWorld localNetID]` on the dedicated server.
pub const SERVER_LOCAL_NET_ID: &str = "cf1e8c14e54505f60aa10ceb8d5d8ab3";

/// `NPC_NAMES_RARE`, 44 entries in index order, mixed case.
pub const NPC_NAMES_RARE: [&str; 44] = [
    "Lloyd",
    "Milla",
    "Majicdave",
    "Biblio",
    "Ace",
    "Asyc",
    "Abcboy",
    "Ashley",
    "Bilk",
    "Bloopy",
    "Buddy",
    "Best1",
    "Cockroach",
    "CJ",
    "Emma",
    "Thomas",
    "Ethan",
    "Noah",
    "Erick",
    "Georgie",
    "Gemv",
    "Ingeniare",
    "JoeU",
    "Jujubee",
    "Jemni",
    "Jlav",
    "Mfs",
    "Mummyk",
    "Nitrojuju",
    "Nycci",
    "Pie",
    "Pineapple",
    "Porky",
    "Sam.Boyer",
    "Skeeve",
    "Smeegle",
    "Redmarble",
    "Craig",
    "Stainedglass",
    "Starmi",
    "Thuthu",
    "Unihorse",
    "Troyortroy",
    "Yachtcrafter",
];

/// `NPC: DynamicObject <HarmableDynamicObject, TapChoiceDynamicObject, RidableDynamicObject>`.
pub static NPC_CLASS: ClassInfo = ClassInfo {
    name: "NPC",
    superclass: Some(&DYNAMIC_OBJECT_CLASS),
    protocols: &["HarmableDynamicObject", "TapChoiceDynamicObject", "RidableDynamicObject"],
};

/// `dynamicObjectTypeForNPCType` as `i32` (world_state owns the function).
pub fn dynamic_object_type_i32(npc_type: i32) -> i32 {
    crate::world::helpers::dynamic_object_type_for_npc_type(NPCType(npc_type as u32)).0 as i32
}

fn r01(_tag: &'static str) -> f32 {
    glibc_rand::r01_tagged(_tag)
}

// ------------------------------------------------------------------------------------------------ state

/// NPC ivars `0x4e..0x117`; every field is zero / None / false after `alloc`. `nameTextView` and `progressShader`
/// are client-only and omitted.
#[derive(Default)]
pub struct NpcState {
    /// 0x4e `damage` (wraps mod 65536)
    pub damage: u16,
    /// 0x50
    pub dead: bool,
    /// 0x51 (client use)
    pub visible: bool,
    /// 0x58 retained
    pub kill_blockhead: Option<ObjRef>,
    /// 0x60
    pub hit_force: u16,
    /// 0x64
    pub fullness: f32,
    /// 0x68
    pub tame_cooldown_timer: f32,
    /// 0x6c
    pub mate_cooldown_timer: f32,
    /// 0x70
    pub lay_timer: f32,
    /// 0x74
    pub lay_cooldown_timer: f32,
    /// 0x78
    pub age: f32,
    /// 0x80 upper-case display name
    pub name: Option<String>,
    /// 0x88
    pub breed: u16,
    /// 0x8a
    pub mate_breed: u16,
    /// 0x8c
    pub has_bred: bool,
    /// 0x8d
    pub has_been_fed_by_blockhead_or_chest: bool,
    /// 0x90: order is not observable; key `"server"` stands for a nil clientID
    pub tame_counts_by_client_id: Option<BTreeMap<String, i64>>,
    /// 0x98: nil = wild; `"server"` = owned by the host
    pub tamed_client_id: Option<String>,
    /// 0xa0 weak
    pub coming_to_inspect_blockhead: Option<ObjWeak>,
    /// 0xa8 weak
    pub inspecting_blockhead: Option<ObjWeak>,
    /// 0xc0
    pub being_controlled: bool,
    /// 0xc8 retained
    pub rider: Option<ObjRef>,
    /// 0xd0: ZERO after alloc (not -1); set to -1 by the atPosition / saveDict paths only
    pub saved_blockhead_index: i32,
    /// 0xd4
    pub remote_send_control_event_timer: f32,
    /// 0xd8
    pub random_harm_from_hunger_timer: f32,
    /// 0xdc
    pub current_temperature: f32,
    /// 0xe0
    pub damage_delay_timer: f32,
    /// 0xe4
    pub temperature_check_timer: f32,
    /// 0xe8 replica -> server request
    pub feed_blockhead_unique_id_to_send: u64,
    /// 0xf0
    pub set_free_blockhead_unique_id_to_send: u64,
    /// 0xf8
    pub capture_blockhead_unique_id_to_send: u64,
    /// 0x100 (set by subclasses)
    pub harvest_blockhead_unique_id_to_send: u64,
    /// 0x108
    pub net_tame_type: u8,
    /// 0x109 (set by subclasses)
    pub harvest_type_to_send: u8,
    /// 0x10a
    pub interaction_item_type_to_send: u16,
    /// 0x10c
    pub send_successful_tame_state: i32,
    /// 0x110
    pub send_successful_feed_state: i32,
    /// 0x114
    pub send_was_hit_state: i32,
}

// ------------------------------------------------------------------------------------------------ env seam

/// Copy of the tile fields the NPC reads (`typeIndex`, `onFire`, `sunLight`, `artificialHeat`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NpcTileCopy {
    pub type_index: u8,
    pub on_fire: u8,
    pub sun_light: u8,
    pub artificial_heat: i16,
}

crate::hooks::hook_trait! {
/// Seam to the layers that were stubs when `npc_base` was written.
/// Every method has a default; the integrator installs one implementation with [`set_npc_env`]. The tile /
/// season / temperature methods have working defaults on top of [`NpcEnv::world`].
pub trait NpcEnv, stub NpcEnvStub {
    /// The concrete `World` (`None` in tests that do not need tiles).
    fn world(&self) -> Option<Rc<World>> {
        None
    }
    /// `tileAtWorldPositionLoaded(x, y, world)` copied out (loads the physical block on demand).
    #[provided]
    fn tile_probe(&self, x: i32, y: i32) -> Option<NpcTileCopy> {
        let w = self.world()?;
        let t = crate::world::tiles::tile_at_world_position_loaded(x, y, &w)?.get();
        Some(NpcTileCopy {
            type_index: t.type_index,
            on_fire: t.on_fire,
            sun_light: t.sun_light,
            artificial_heat: t.artificial_heat,
        })
    }
    /// `seasonForWorldX(x, worldTime, world)`.
    #[provided]
    fn season_for_world_x(&self, x: i32, t: f64) -> f32 {
        let w = self.world();
        crate::world::climate::season_for_world_x(x, t, w.as_deref())
    }
    /// `[world getDayNightFractionForX:atWorldTime:]`.
    fn day_night_fraction(&self, _x: f32, _t: f64) -> f32 {
        0.0
    }
    /// `[world getWeatherFractionForPos:atWorldTime:]`.
    fn weather_fraction(&self, _pos: IntPair, _t: f64) -> f32 {
        0.0
    }
    /// `currentTemperatureForTileAtWorldPos(tile, pos, season, dayNight, weather, world)`.
    #[provided]
    fn current_temperature(&self, tile: &NpcTileCopy, pos: IntPair, season: f32, day: f32, weather: f32) -> f32 {
        let w = self.world();
        let mut t = Tile::default();
        t.sun_light = tile.sun_light;
        t.artificial_heat = tile.artificial_heat;
        crate::world::climate::current_temperature_for_tile_at_world_pos(&t, pos, season, day, weather, w.as_deref())
    }
    /// `tileContainsDoor(tileAtWorldPositionLoaded(x, y))` (NULL-safe).
    #[provided]
    fn tile_contains_door(&self, x: i32, y: i32) -> bool {
        match self.world() {
            Some(w) => {
                crate::world::helpers::tile_contains_door(crate::world::tiles::tile_at_world_position_loaded(x, y, &w))
            }
            None => false,
        }
    }
    /// `tileContainsDoor || tileContainsTrapDoor || tileContainsGate` of the tile at (x, y).
    #[provided]
    fn tile_contains_door_like(&self, x: i32, y: i32) -> bool {
        match self.world() {
            Some(w) => {
                let t = crate::world::tiles::tile_at_world_position_loaded(x, y, &w);
                crate::world::helpers::tile_contains_door(t)
                    || crate::world::helpers::tile_contains_trap_door(t)
                    || crate::world::helpers::tile_contains_gate(t)
            }
            None => false,
        }
    }
    /// `[world markCircumNavigateX:]`.
    fn mark_circum_navigate_x(&self, _x: i32) {}
    /// `[[world serverClients] count]` (World ivar @0x4d0,).
    fn server_clients_count(&self) -> usize {
        0
    }
    /// `[dynamicWorld localNetID]`; on the dedicated server `md5hex("server")`.
    #[provided]
    fn local_net_id(&self) -> Option<String> {
        Some(SERVER_LOCAL_NET_ID.to_string())
    }
    /// `[dynamicWorld isClient]` (`client != nil`): never true on the dedicated server.
    fn is_client(&self) -> bool {
        false
    }
    /// `[dynamicWorld blockheadWithIDIncludingNet:]`
    fn blockhead_with_id_including_net(&self, _id: u64) -> Option<ObjRef> {
        None
    }
    /// `[dynamicWorld allBlockheadsIncludingNet]`
    fn all_blockheads_including_net(&self) -> Vec<ObjRef> {
        Vec::new()
    }
    /// `[dynamicWorld freeBlocksAtPos:]` (None when empty).
    fn free_blocks_at_pos(&self, _pos: IntPair) -> Option<Vec<ObjRef>> {
        None
    }
    /// `[freeBlock itemType]`
    fn free_block_item_type(&self, _fb: &ObjRef) -> i32 {
        0
    }
    /// `[dynamicWorld interactionObjectAtPos:]`
    fn interaction_object_at_pos(&self, _pos: IntPair) -> Option<ObjRef> {
        None
    }
    /// `io.interactionObjectType == 2 && io.chestType == 6 && !io.isInUse` then `[io removeItemIfAvailable:item]`
    /// `None` when the object is not an unused feeder chest, else the removal result.
    fn feeder_chest_remove_item_if_available(&self, _io: &ObjRef, _item: i32) -> Option<bool> {
        None
    }
    /// `[dynamicWorld setDoorAtPos:toOpen:direction:]`
    fn set_door_at_pos_to_open_direction(&self, _pos: IntPair, _open: bool, _direction: i32) {}
    /// `[dynamicWorld createFreeBlockAtPosition:ofType:dataA:0 dataB:0 subItems:nil dynamicObjectSaveDict:hovers:NO
    /// playSound:NO priorityBlockhead:]`.
    fn create_free_block_for_captured_npc(
        &self,
        _pos: IntPair,
        _item_type: i32,
        _save_dict: &PlistDict,
        _blockhead: &ObjRef,
    ) {
    }
    /// `[[world server] removeCurseWordsFromBlockheadName:]`.
    fn remove_curse_words_from_blockhead_name(&self, name: &str) -> String {
        name.to_string()
    }
    /// `[blockhead paused]`
    fn blockhead_paused(&self, _b: &ObjRef) -> bool {
        false
    }
    /// `[blockhead stopRiding]` (re-enters `removeRider:` of the ridden object).
    fn blockhead_stop_riding(&self, _b: &ObjRef) {}
    /// `[blockhead setRidingObject:]`
    fn blockhead_set_riding_object(&self, _b: &ObjRef, _o: Option<&ObjRef>) {}
}
}

struct NullEnv;
impl NpcEnvStub for NullEnv {}

thread_local! {
    static ENV: RefCell<Rc<dyn NpcEnv>> = RefCell::new(Rc::new(NullEnv));
    static CTORS: RefCell<[Option<NpcCtor>; 9]> = const { RefCell::new([None; 9]) };
    static DOWNCASTERS: RefCell<HashMap<TypeId, for<'a> fn(&'a dyn Any) -> Option<&'a dyn Npc>>> = RefCell::new(HashMap::new());
}

/// Installs the environment seam (once at startup).
pub fn set_npc_env(e: Rc<dyn NpcEnv>) {
    ENV.with(|c| *c.borrow_mut() = e);
}

/// The installed environment (an inert default when none was installed).
pub fn npc_env() -> Rc<dyn NpcEnv> {
    ENV.with(|c| c.borrow().clone())
}

// ------------------------------------------------------------------------------------------------ as_npc shim

/// Registers the concrete NPC class `T` so [`as_npc`] can reach its [`Npc`] view. SHIM for the requested
/// `DynamicObject::as_npc` (;. Idempotent.
pub fn register_npc_downcast<T: Npc + 'static>() {
    fn cast<T: Npc + 'static>(a: &dyn Any) -> Option<&dyn Npc> {
        a.downcast_ref::<T>().map(|t| t as &dyn Npc)
    }
    DOWNCASTERS.with(|m| {
        m.borrow_mut().insert(TypeId::of::<T>(), cast::<T>);
    });
}

/// `Rc::new_cyclic` allocation (see `objects::handle::alloc_obj`) that also registers `T` for [`as_npc`].
/// Concrete NPC classes allocate through this.
pub fn alloc_npc<T: Npc + 'static>(init: impl FnOnce(ObjWeak) -> T) -> Rc<T> {
    register_npc_downcast::<T>();
    alloc_obj(init)
}

/// `isKindOfClass:NPC` plus the NPC message view of an object. `None` for non-NPC objects.
pub fn as_npc(o: &ObjRef) -> Option<&dyn Npc> {
    let a = o.as_any();
    let id = a.type_id();
    let f = DOWNCASTERS.with(|m| m.borrow().get(&id).copied())?;
    f(a)
}

fn vn(me: &ObjRef) -> &dyn Npc {
    as_npc(me).expect("object is not a registered NPC class (use alloc_npc / register_npc_downcast)")
}

/// The NPC ivars of `me`; panics on a non-NPC (programming error).
pub fn npc_state(me: &ObjRef) -> &RefCell<NpcState> {
    vn(me).npc()
}

// ------------------------------------------------------------------------------------------------ small helpers

fn dw_changed(me: &ObjRef) {
    if let Some(dw) = me.dynamic_world() {
        dw.dynamic_world_changed_at_pos_object_type(me.pos(), me.object_type());
    }
}

fn is_server_dw(me: &ObjRef) -> bool {
    me.dynamic_world().is_some_and(|d| d.is_server())
}

fn world_time(me: &ObjRef) -> f64 {
    me.world().map_or(0.0, |w| w.world_time())
}

/// `[blockhead clientID]`; a non-Blockhead (or nil) has none.
fn blockhead_client_id(b: &ObjRef) -> Option<String> {
    b.as_blockhead().and_then(|x| x.client_id())
}

fn is_client_blockhead_controlled_by_server(b: &ObjRef) -> bool {
    b.as_blockhead().is_some_and(|x| x.is_client_blockhead_being_controlled_by_server())
}

fn weak_is(w: &Option<ObjWeak>, b: &ObjRef) -> bool {
    match w.as_ref().and_then(|w| w.upgrade()) {
        Some(o) => ptr_eq(&o, b),
        None => false,
    }
}

/// UB-PARITY: the original dereferences a NULL own tile in the temperature block.
#[inline]
fn ub_null_own_tile() {
    #[cfg(all(feature = "faithful-crashes", not(test)))]
    crate::foundation::sys::faithful_segv();
}

// ------------------------------------------------------------------------------------------------ Npc trait

/// Every NPC selector that is not a `DynamicObject` selector, with the NPC base behaviour as default.
/// A subclass overrides by writing its own body and chains with the `npc_*` free function (`[super...]`);
/// it never calls the default through `self`.
pub trait Npc: DynamicObject {
    /// The NPC ivars.
    fn npc(&self) -> &RefCell<NpcState>;

    /// `riderRidesWithArmsDown`: true for Dodo and Scorpion, false otherwise.
    fn rider_rides_with_arms_down(&self) -> bool {
        false
    }

    // ---- constants a subclass overrides ----
    /// `npcType`
    fn npc_type(&self) -> i32 {
        NPC_NOTHING
    }
    /// `maxAge`
    fn max_age(&self) -> f32 {
        9000.0
    }
    /// `diesOfOldAge`
    fn dies_of_old_age(&self) -> bool {
        true
    }
    /// `minFullness`
    fn min_fullness(&self) -> f32 {
        -1800.0
    }
    /// `diesOfLowFullness`
    fn dies_of_low_fullness(&self) -> bool {
        true
    }
    /// `maxHealth`
    fn max_health(&self) -> u16 {
        16
    }
    /// `creationDataStructSize`
    fn creation_data_struct_size(&self) -> usize {
        0x48
    }
    /// `minRidableAge`
    fn min_ridable_age(&self) -> f32 {
        900.0
    }
    /// `speciesName`
    fn species_name(&self) -> String {
        "CRITTER".into()
    }
    /// `foodItemType` (ITEM_NONE)
    fn food_item_type(&self) -> i32 {
        0
    }
    /// `capturedItemType`
    fn captured_item_type(&self) -> i32 {
        0
    }
    /// `captureRequiredItemType`
    fn capture_required_item_type(&self) -> i32 {
        0
    }
    /// `getNamesArray`
    fn get_names_array(&self) -> Option<&'static [&'static str]> {
        None
    }
    /// `getNamesArrayCount`
    fn get_names_array_count(&self) -> i32 {
        0
    }
    /// `breedString`
    fn breed_string(&self) -> String {
        self.species_name()
    }
    /// `suffersDamageAtHighTemperatures`
    fn suffers_damage_at_high_temperatures(&self) -> bool {
        true
    }
    /// `ridableWhenTamed`
    fn ridable_when_tamed(&self) -> bool {
        true
    }
    /// `removeTitle`
    fn remove_title(&self) -> &'static str {
        "SET FREE"
    }
    /// `removeIsRed`
    fn remove_is_red(&self) -> bool {
        true
    }
    /// `secondChoiceIsBlue`
    fn second_choice_is_blue(&self) -> bool {
        true
    }

    // ---- hooks with bodies ----
    fn load_values_from_save_dict(&self, me: &ObjRef, d: &PlistDict) {
        npc_load_values_from_save_dict(me, d)
    }
    fn set_adult_creation_start_values(&self, me: &ObjRef) {
        npc_set_adult_creation_start_values(me)
    }
    fn set_baby_creation_start_values(&self, me: &ObjRef) {
        npc_set_baby_creation_start_values(me)
    }
    /// `createItemDropsForDeath`: empty hook in the base.
    fn create_item_drops_for_death(&self, _me: &ObjRef) {}
    fn npc_creation_net_data_for_client(&self, me: &ObjRef, cid: Option<&str>) -> NPCCreationNetData {
        npc_npc_creation_net_data_for_client(me, cid)
    }
    fn npc_update_net_data_for_client(&self, me: &ObjRef, cid: Option<&str>) -> NPCUpdateNetData {
        npc_npc_update_net_data_for_client(me, cid)
    }
    fn append_npc_creation_data_to_data(&self, me: &ObjRef, subclass_data: Vec<u8>) -> Vec<u8> {
        npc_append_npc_creation_data_to_data(me, subclass_data)
    }
    fn die(&self, me: &ObjRef, killer: Option<&ObjRef>) {
        npc_die(me, killer)
    }
    /// `reactToBeingHit`: no-op (client effects in subclasses).
    fn react_to_being_hit(&self, _me: &ObjRef) {}
    /// `reactToBeingFed`: no-op (subclass hook).
    fn react_to_being_fed(&self, _me: &ObjRef) {}
    fn successful_tame(&self, me: &ObjRef) {
        npc_successful_tame(me)
    }
    fn generate_new_name(&self, me: &ObjRef) {
        npc_generate_new_name(me)
    }
    fn can_be_fed_by_blockhead(&self, me: &ObjRef, b: &ObjRef) -> bool {
        npc_can_be_fed_by_blockhead(me, b)
    }
    fn feed_by_blockhead(&self, me: &ObjRef, b: &ObjRef) -> bool {
        npc_feed_by_blockhead(me, b)
    }
    fn cant_be_fed_tip_string_for_blockhead(&self, _b: &ObjRef) -> String {
        format!("THAT {} ISN'T HUNGRY\nTRY AGAIN LATER", self.species_name())
    }
    fn can_be_milked_by_blockhead(&self, _b: &ObjRef) -> bool {
        false
    }
    fn cant_be_milked_tip_string_for_blockhead(&self, _b: &ObjRef) -> String {
        format!("THAT {} HAS NO MILK\nTRY AGAIN LATER", self.species_name())
    }
    fn milk_by_blockhead(&self, _me: &ObjRef, _b: &ObjRef) -> bool {
        false
    }
    fn can_be_shaved_by_blockhead(&self, _b: &ObjRef) -> bool {
        false
    }
    fn cant_be_shaved_tip_string_for_blockhead(&self, _b: &ObjRef) -> String {
        format!("THAT {} IS ALREADY SHAVED\nTRY AGAIN LATER", self.species_name())
    }
    fn shave_by_blockhead(&self, _me: &ObjRef, _b: &ObjRef) -> bool {
        false
    }
    fn can_be_captured_by_blockhead_with_item_type(&self, _b: &ObjRef, _item: i32) -> bool {
        false
    }
    fn cant_be_captured_tip_string_for_blockhead_with_item_type(&self, _b: &ObjRef, _item: i32) -> Option<String> {
        None
    }
    fn capture_by_blockhead_with_item_type(&self, me: &ObjRef, b: &ObjRef, item: i32) -> bool {
        npc_capture_by_blockhead_with_item_type(me, b, item)
    }
    fn set_free_by_blockhead(&self, me: &ObjRef, b: &ObjRef) {
        npc_set_free_by_blockhead(me, b)
    }
    fn change_name(&self, me: &ObjRef, new_name: &str) {
        npc_change_name(me, new_name)
    }
    fn tamed(&self) -> bool {
        npc_tamed_state(self.npc(), self.base().borrow().is_net)
    }
    fn belongs_to_player_with_blockhead(&self, b: &ObjRef) -> bool {
        npc_belongs_to_player_state(self.npc(), self.base().borrow().is_net, b)
    }
    fn belongs_to_local_player(&self) -> bool {
        let is_net = self.base().borrow().is_net;
        let dw_server_side = !npc_env().is_client();
        npc_belongs_to_local_player_state(self.npc(), is_net, dw_server_side)
    }
    fn can_mate(&self) -> bool {
        let s = self.npc().borrow();
        s.mate_cooldown_timer <= 0.0 && !s.dead && 900.0f32 < s.age
    }
    fn mate_with_npc(&self, me: &ObjRef, other: &ObjRef) -> bool {
        npc_mate_with_npc(me, other)
    }
    fn check_current_position_for_food(&self, me: &ObjRef) {
        npc_check_current_position_for_food(me)
    }
    /// `secondOptionTitle`
    fn second_option_title(&self) -> Option<&'static str> {
        if !self.ridable_when_tamed() {
            return None;
        }
        let age = self.npc().borrow().age;
        if self.dies_of_old_age() && age < self.min_ridable_age() {
            return None;
        }
        Some("RIDE")
    }
    /// `actionTitle` = `name`
    fn action_title(&self) -> Option<String> {
        self.npc().borrow().name.clone()
    }
    fn health_fraction(&self) -> f32 {
        let d = self.npc().borrow().damage;
        clamp(1.0f32 - (d as f32) / (self.max_health() as f32), 0.0f32, 1.0f32)
    }
    fn fullness_fraction(&self) -> f32 {
        if !self.dies_of_low_fullness() {
            1.0f32
        } else {
            let f = self.npc().borrow().fullness;
            clamp(((f as f64) / 5400.0f64) as f32, 0.0f32, 1.0f32)
        }
    }
    fn name(&self) -> Option<String> {
        self.npc().borrow().name.clone()
    }
    fn begin_blockhead_inspection(&self, b: &ObjRef) {
        let mut s = self.npc().borrow_mut();
        s.coming_to_inspect_blockhead = None;
        s.inspecting_blockhead = Some(Rc::downgrade(b));
    }
    fn blockhead_is_coming_to_inspect(&self, b: &ObjRef) {
        self.npc().borrow_mut().coming_to_inspect_blockhead = Some(Rc::downgrade(b));
    }
    fn inspection_stopped(&self) {
        let mut s = self.npc().borrow_mut();
        s.inspecting_blockhead = None;
        s.coming_to_inspect_blockhead = None;
    }
    /// `namePos` = `renderPos + (0, 1)` (client UI).
    fn name_pos(&self) -> Vector2 {
        let r = self.render_pos();
        Vector2 { x: r.x + 0.0f32, y: r.y + 1.0f32 }
    }
    fn age(&self) -> f32 {
        self.npc().borrow().age
    }
    fn set_age(&self, v: f32) {
        self.npc().borrow_mut().age = v;
    }
    fn breed(&self) -> u16 {
        self.npc().borrow().breed
    }
    fn set_breed(&self, v: u16) {
        self.npc().borrow_mut().breed = v;
    }
    /// `drawName:...`: CLIENT-ONLY, empty.
    fn draw_name(&self) {}

    // ---- protocol selectors ----
    fn tap_is_within_body_radius(&self, _tap: Vector2) -> bool {
        false
    }
    fn hit_with_force_blockhead(&self, me: &ObjRef, force: i32, b: Option<&ObjRef>) {
        npc_hit_with_force_blockhead(me, force, b)
    }
    fn will_die_if_hit_by_force(&self, force: i32) -> bool {
        let d = self.npc().borrow().damage;
        (self.max_health() as i32) <= (d as i32).wrapping_add(force)
    }
    fn is_visible(&self) -> bool {
        self.npc().borrow().visible
    }
    fn center(&self) -> Vector2 {
        self.render_pos()
    }
    fn rider_pos_for_blockhead(&self, b: &ObjRef) -> [f32; 4] {
        let _ = b;
        let fp = self.base().borrow().float_pos;
        [fp.x + 0.0f32, fp.y + 0.8f32, -0.75f32, 0.0]
    }
    fn camera_pos_for_blockhead(&self, _b: &ObjRef) -> Vector2 {
        self.render_pos()
    }
    fn rider_body_y_rotation_for_blockhead(&self, _b: &ObjRef) -> f32 {
        0.0
    }
    fn rider_body_z_rotation_for_blockhead(&self, _b: &ObjRef) -> f32 {
        0.0
    }
    fn set_target_velocity(&self, _v: Vector2) {}
    fn ride_direction(&self) -> i32 {
        0
    }
    fn add_rider(&self, me: &ObjRef, b: &ObjRef) {
        npc_add_rider(me, b)
    }
    fn remove_rider(&self, me: &ObjRef, b: &ObjRef) {
        npc_remove_rider(me, b)
    }
    /// `[self removeRider:x]` where `x` may be nil (the argument is re-read after `[rider stopRiding]`,
    /// 5.10). Virtual like `removeRider:`: a non-nil argument goes to [`Npc::remove_rider`]; a subclass whose
    /// `removeRider:` does extra work after `rider_ == rider` (DonkeyLike, Scorpion) must
    /// override this too, because `nil == nil` passes its guard.
    fn remove_rider_opt(&self, me: &ObjRef, b: Option<&ObjRef>) {
        match b {
            Some(b) => self.remove_rider(me, b),
            None => npc_remove_rider_opt(me, None),
        }
    }
    fn swipe_up_gesture(&self, _me: &ObjRef) {}
    fn blockhead_can_ride_using_item(&self, _b: &ObjRef, _item: i32) -> bool {
        false
    }
    fn jumps_on_swipe(&self) -> bool {
        false
    }
    fn requires_fuel(&self) -> bool {
        false
    }
    fn acts_as_interaction_object(&self) -> Option<ObjRef> {
        None
    }
    fn rider_d_pad_should_allow_up_down(&self) -> bool {
        false
    }
    fn rider_d_pad_should_give_discrete_values(&self) -> bool {
        false
    }
    fn is_double_height(&self) -> bool {
        false
    }
    /// `canBeRemovedByBlockhead:` = `belongsToPlayerWithBlockhead:`
    fn can_be_removed_by_blockhead_npc(&self, b: &ObjRef) -> bool {
        self.belongs_to_player_with_blockhead(b)
    }
}

// ------------------------------------------------------------------------------------------------ macros

/// Implements `HarmableDynamicObject`, `RidableDynamicObject` and `TapChoiceDynamicObject` for a concrete NPC
/// class by forwarding every method to the [`Npc`] method of the same name.
#[macro_export]
macro_rules! impl_npc_protocols {
    ($T:ty) => {
        impl $crate::objects::protocols::HarmableDynamicObject for $T {
            fn tap_is_within_body_radius(&self, tap: $crate::types::structs::Vector2) -> bool {
                $crate::npc::npc::Npc::tap_is_within_body_radius(self, tap)
            }
            fn hit_with_force_blockhead(
                &self,
                me: &$crate::objects::handle::ObjRef,
                force: i32,
                b: Option<&$crate::objects::handle::ObjRef>,
            ) {
                $crate::npc::npc::Npc::hit_with_force_blockhead(self, me, force, b)
            }
            fn will_die_if_hit_by_force(&self, force: i32) -> bool {
                $crate::npc::npc::Npc::will_die_if_hit_by_force(self, force)
            }
            fn is_visible(&self) -> bool {
                $crate::npc::npc::Npc::is_visible(self)
            }
            fn center(&self) -> $crate::types::structs::Vector2 {
                $crate::npc::npc::Npc::center(self)
            }
        }
        impl $crate::objects::protocols::RidableDynamicObject for $T {
            fn rider_pos_for_blockhead(&self, b: &$crate::objects::handle::ObjRef) -> [f32; 4] {
                $crate::npc::npc::Npc::rider_pos_for_blockhead(self, b)
            }
            fn camera_pos_for_blockhead(&self, b: &$crate::objects::handle::ObjRef) -> $crate::types::structs::Vector2 {
                $crate::npc::npc::Npc::camera_pos_for_blockhead(self, b)
            }
            fn rider_body_y_rotation_for_blockhead(&self, b: &$crate::objects::handle::ObjRef) -> f32 {
                $crate::npc::npc::Npc::rider_body_y_rotation_for_blockhead(self, b)
            }
            fn set_target_velocity(&self, v: $crate::types::structs::Vector2) {
                $crate::npc::npc::Npc::set_target_velocity(self, v)
            }
            fn ride_direction(&self) -> i32 {
                $crate::npc::npc::Npc::ride_direction(self)
            }
            fn add_rider(&self, me: &$crate::objects::handle::ObjRef, b: &$crate::objects::handle::ObjRef) {
                $crate::npc::npc::Npc::add_rider(self, me, b)
            }
            fn remove_rider(&self, me: &$crate::objects::handle::ObjRef, b: &$crate::objects::handle::ObjRef) {
                $crate::npc::npc::Npc::remove_rider(self, me, b)
            }
            fn swipe_up_gesture(&self, me: &$crate::objects::handle::ObjRef) {
                $crate::npc::npc::Npc::swipe_up_gesture(self, me)
            }
            fn blockhead_can_ride_using_item(&self, b: &$crate::objects::handle::ObjRef, item: i32) -> bool {
                $crate::npc::npc::Npc::blockhead_can_ride_using_item(self, b, item)
            }
            fn jumps_on_swipe(&self) -> bool {
                $crate::npc::npc::Npc::jumps_on_swipe(self)
            }
            fn requires_fuel(&self) -> bool {
                $crate::npc::npc::Npc::requires_fuel(self)
            }
            fn acts_as_interaction_object(&self) -> Option<$crate::objects::handle::ObjRef> {
                $crate::npc::npc::Npc::acts_as_interaction_object(self)
            }
            fn rider_d_pad_should_allow_up_down(&self) -> bool {
                $crate::npc::npc::Npc::rider_d_pad_should_allow_up_down(self)
            }
            fn rider_d_pad_should_give_discrete_values(&self) -> bool {
                $crate::npc::npc::Npc::rider_d_pad_should_give_discrete_values(self)
            }
        }
        impl $crate::objects::protocols::TapChoiceDynamicObject for $T {
            fn action_title(&self) -> String {
                $crate::npc::npc::Npc::action_title(self).unwrap_or_default()
            }
            fn is_double_height(&self) -> bool {
                $crate::npc::npc::Npc::is_double_height(self)
            }
        }
    };
}

/// Expands, inside `impl DynamicObject for X {... }`, to the named `DynamicObject` methods, each forwarding to the
/// free function `npc_<name>`. A subclass lists only the methods it does NOT override.
#[macro_export]
macro_rules! npc_forward {
    () => {};
    ($name:ident $(, $($rest:tt)*)?) => {
        $crate::npc_forward!(@one $name);
        $( $crate::npc_forward!($($rest)*); )?
    };
    (@one object_type) => {
        fn object_type(&self) -> i32 {
            $crate::npc::npc::dynamic_object_type_i32($crate::npc::npc::Npc::npc_type(self))
        }
    };
    (@one update_accurate_dt_is_simulation) => {
        fn update_accurate_dt_is_simulation(&self, me: &$crate::objects::handle::ObjRef, dt: f32, accurate_dt: f32, is_simulation: bool) {
            $crate::npc::npc::npc_update_accurate_dt_is_simulation(me, dt, accurate_dt, is_simulation)
        }
    };
    (@one get_save_dict) => {
        fn get_save_dict(&self, me: &$crate::objects::handle::ObjRef) -> $crate::plist::value::PlistDict {
            $crate::npc::npc::npc_get_save_dict(me)
        }
    };
    (@one creation_net_data_for_client) => {
        fn creation_net_data_for_client(&self, me: &$crate::objects::handle::ObjRef, client_id: Option<&str>) -> Option<Vec<u8>> {
            $crate::npc::npc::npc_creation_net_data_for_client(me, client_id)
        }
    };
    (@one update_net_data_for_client) => {
        fn update_net_data_for_client(&self, me: &$crate::objects::handle::ObjRef, client_id: Option<&str>) -> Option<Vec<u8>> {
            $crate::npc::npc::npc_update_net_data_for_client(me, client_id)
        }
    };
    (@one remote_update) => {
        fn remote_update(&self, me: &$crate::objects::handle::ObjRef, net_data: &[u8]) {
            $crate::npc::npc::npc_remote_update(me, net_data)
        }
    };
    (@one remote_creation_data_update) => {
        fn remote_creation_data_update(&self, me: &$crate::objects::handle::ObjRef, net_data: &[u8]) {
            $crate::npc::npc::npc_remote_creation_data_update(me, net_data)
        }
    };
    (@one blockhead_unloaded) => {
        fn blockhead_unloaded(&self, me: &$crate::objects::handle::ObjRef, b: &$crate::objects::handle::ObjRef) {
            $crate::npc::npc::npc_blockhead_unloaded(me, b)
        }
    };
    (@one blockheads_loaded) => {
        fn blockheads_loaded(&self, me: &$crate::objects::handle::ObjRef) {
            $crate::npc::npc::npc_blockheads_loaded(me)
        }
    };
    (@one can_be_removed_by_blockhead) => {
        fn can_be_removed_by_blockhead(&self, me: &$crate::objects::handle::ObjRef, b: &$crate::objects::handle::ObjRef) -> bool {
            $crate::npc::npc::npc_can_be_removed_by_blockhead(me, b)
        }
    };
    (@one update_position) => {
        fn update_position(&self, me: &$crate::objects::handle::ObjRef, new_position: $crate::types::structs::IntPair) {
            $crate::npc::npc::npc_update_position(me, new_position)
        }
    };
    (@one should_save_every_change_in_position) => {
        fn should_save_every_change_in_position(&self) -> bool {
            $crate::npc::npc::Npc::npc(self).borrow().rider.is_some()
        }
    };
    (@one client_id_for_saving_seperately_and_only_loading_while_player_online) => {
        fn client_id_for_saving_seperately_and_only_loading_while_player_online(&self) -> Option<String> {
            $crate::npc::npc::Npc::npc(self).borrow().tamed_client_id.clone()
        }
    };
    (@one requires_physical_block) => {
        fn requires_physical_block(&self) -> bool {
            false
        }
    };
    (@one render_pos) => {
        fn render_pos(&self) -> $crate::types::structs::Vector2 {
            $crate::objects::base::DynamicObject::base(self).borrow().float_pos
        }
    };
    (@one dealloc) => {
        fn dealloc(&self, me: &$crate::objects::handle::ObjRef) {
            $crate::npc::npc::npc_dealloc(me)
        }
    };
    (@one as_harmable) => {
        fn as_harmable(&self) -> Option<&dyn $crate::objects::protocols::HarmableDynamicObject> {
            Some(self)
        }
    };
    (@one as_ridable) => {
        fn as_ridable(&self) -> Option<&dyn $crate::objects::protocols::RidableDynamicObject> {
            Some(self)
        }
    };
    (@one as_tap_choice) => {
        fn as_tap_choice(&self) -> Option<&dyn $crate::objects::protocols::TapChoiceDynamicObject> {
            Some(self)
        }
    };
}

// ------------------------------------------------------------------------------------------------ init

/// `-[NPC initWithWorld:dynamicWorld:atPosition:cache:saveDict:isAdult:wasPlaced:placedByClient:]`.
/// The caller did `alloc_npc`; this runs `[super init...]` then the NPC steps. `false` = "returned nil".
/// Draw order: start values (adult 3 / baby 2), then 1 for `randomHarmFromHungerTimer`.
#[allow(clippy::too_many_arguments)]
pub fn npc_init_with_world_dynamic_world_at_position_cache_save_dict_is_adult_was_placed_placed_by_client(
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
    let inited = base_init_with_world_dynamic_world_at_position_cache(me, world, dw, pos, cache);
    debug_assert!(inited); // the base initialisers cannot fail on this path
    let npc = vn(me);
    npc.npc().borrow_mut().saved_blockhead_index = -1;
    if is_adult {
        npc.set_adult_creation_start_values(me);
    } else {
        npc.set_baby_creation_start_values(me);
    }
    if let Some(sd) = save_dict {
        npc.load_values_from_save_dict(me, sd);
    }
    if was_placed {
        let has_owner = npc.npc().borrow().tamed_client_id.is_some();
        if has_owner {
            if let Some(cid) = client_id {
                let mut owner = cid.to_string();
                if dw.is_server() && npc_env().local_net_id().as_deref() == Some(cid) {
                    owner = "server".to_string();
                }
                npc.npc().borrow_mut().tamed_client_id = Some(owner);
            }
        }
    }
    npc.npc().borrow_mut().random_harm_from_hunger_timer = r01("NPC.init.hunger") * 20.0f32 + 1.0f32;
    true
}

/// `-[NPC initWithWorld:dynamicWorld:saveDict:cache:]`: load only, no start values.
pub fn npc_init_with_world_dynamic_world_save_dict_cache(
    me: &ObjRef,
    world: &WorldRef,
    dw: &DynamicWorldRef,
    save_dict: &PlistDict,
    cache: CacheRef,
) -> bool {
    let inited = base_init_with_world_dynamic_world_save_dict_cache(me, world, dw, save_dict, cache);
    debug_assert!(inited); // the base initialisers cannot fail on this path
    let npc = vn(me);
    npc.load_values_from_save_dict(me, save_dict);
    npc.npc().borrow_mut().random_harm_from_hunger_timer = r01("NPC.init.hunger") * 20.0f32 + 1.0f32;
    true
}

/// `-[NPC initWithWorld:dynamicWorld:cache:netData:]`: the `isNet` replica constructor, reachable
/// on the server through a client `remoteCreate:` (ERRATUM E5). Short data is zero padded
/// (`objects::base::padded_bytes`, E-F9).
pub fn npc_init_with_world_dynamic_world_cache_net_data(
    me: &ObjRef,
    world: &WorldRef,
    dw: &DynamicWorldRef,
    cache: CacheRef,
    net_data: &[u8],
) -> bool {
    if !base_init_with_world_dynamic_world_cache_net_data(me, world, dw, cache, net_data) {
        return false;
    }
    let d = <NPCCreationNetData as WireStruct>::read_le(&padded_bytes(net_data, 0x48));
    let mut s = npc_state(me).borrow_mut();
    s.dead = d.dead != 0;
    s.damage = d.damage;
    s.breed = d.breed as u16;
    s.net_tame_type = d.tame_type;
    s.tame_cooldown_timer = (d.tame_cooldown_timer as i32) as f32;
    true
}

/// `-[NPC loadValuesFromSaveDict:]`.
pub fn npc_load_values_from_save_dict(me: &ObjRef, d: &PlistDict) {
    let get = |k: &str| d.map.get(&k.to_string());
    let f = |k: &str| get(k).map_or(0.0f32, |v| v.float_value());
    let mut s = npc_state(me).borrow_mut();
    if get("fullness").is_some() {
        s.fullness = f("fullness");
        s.lay_timer = f("layTimer");
        s.damage = get("damage").map_or(0, |v| v.int_value()) as u16;
        s.age = f("age");
    }
    if get("layCooldownTimer").is_some() {
        s.lay_cooldown_timer = f("layCooldownTimer");
        s.tame_cooldown_timer = f("tameCooldownTimer");
        s.mate_cooldown_timer = f("mateCooldownTimer");
        s.has_bred = get("hasBred").is_some_and(|v| v.bool_value());
        s.has_been_fed_by_blockhead_or_chest = get("hasBeenFedByBlockheadOrChest").is_some_and(|v| v.bool_value());
        s.mate_breed = get("mateBreed").map_or(0, |v| v.int_value()) as u16;
    }
    if let Some(v) = get("breed") {
        s.breed = v.unsigned_long_value() as u16;
    }
    s.tamed_client_id = get("tamedClientID").and_then(|v| v.as_str()).map(|x| x.to_string());
    s.name = get("name").and_then(|v| v.as_str()).map(|x| x.to_string());
    s.tame_counts_by_client_id = None;
    #[allow(clippy::collapsible_match)] // mirrors the original's nested type check
    if let Some(v) = get("tameCountsByClientID") {
        if let PlistValue::Dict(dd) = v {
            let mut m = BTreeMap::new();
            for (k, val) in dd.map.iter() {
                m.insert(k.clone(), val.long_long_value());
            }
            s.tame_counts_by_client_id = Some(m);
        }
    }
    s.saved_blockhead_index = -1;
    if let Some(v) = get("currentBlockheadIndex") {
        s.saved_blockhead_index = v.int_value();
    }
}

/// `-[NPC setAdultCreationStartValues]`: 3 draws.
pub fn npc_set_adult_creation_start_values(me: &ObjRef) {
    let npc = vn(me);
    let r1 = r01("NPC.adult");
    let r2 = r01("NPC.adult");
    let r3 = r01("NPC.adult");
    let max_age = npc.max_age();
    let mut s = npc.npc().borrow_mut();
    s.age = ((r1 * max_age) as f64 * 0.5f64) as f32;
    s.fullness = 900.0f32 + r2 * 900.0f32;
    s.lay_cooldown_timer = 900.0f32 * r3;
}

/// `-[NPC setBabyCreationStartValues]`: 2 draws.
pub fn npc_set_baby_creation_start_values(me: &ObjRef) {
    let r1 = r01("NPC.baby");
    let r2 = r01("NPC.baby");
    let mut s = npc_state(me).borrow_mut();
    s.age = 0.0;
    s.fullness = 900.0f32 + r1 * 900.0f32;
    s.lay_cooldown_timer = 900.0f32 * r2 + 900.0f32;
    s.mate_cooldown_timer = 60.0f32;
}

// ------------------------------------------------------------------------------------------------ net + save

/// `-[NPC npcCreationNetDataForClient:]`. Bytes the original leaves uninitialised are zero
pub fn npc_npc_creation_net_data_for_client(me: &ObjRef, cid: Option<&str>) -> NPCCreationNetData {
    let dyn_nd = me.dynamic_object_net_data(me);
    let mut st = npc_state(me).borrow_mut();
    let mut out = NPCCreationNetData::default();
    out.dynamic_object_net_data = dyn_nd;
    out.dead = st.dead as u8;
    out.damage = st.damage;
    out.breed = st.breed as u8;
    out.fullness = f32_to_i16(st.fullness);
    out.age = f32_to_i16(st.age);
    out.tame_type = 0;
    if let Some(t) = &st.tamed_client_id {
        out.tame_type = if cid == Some(t.as_str()) { NPC_TAMED_BY_THIS_CLIENT } else { NPC_TAMED_BY_OTHER_CLIENT };
    }
    out.hit_force = 0;
    if st.send_was_hit_state != 0 {
        out.hit_force = st.hit_force;
        out.kill_blockhead_id = st.kill_blockhead.as_ref().map_or(0, |b| b.unique_id());
        st.send_was_hit_state = SEND_STATE_SENT;
    }
    out.interact_blockhead_id = 0;
    out.interaction_type = 0;
    out.interaction_item_type = 0;
    out.tame_cooldown_timer = f32_to_i16(st.tame_cooldown_timer);
    if st.feed_blockhead_unique_id_to_send != 0 {
        out.interact_blockhead_id = st.feed_blockhead_unique_id_to_send;
        out.interaction_type = NET_NPC_INTERACTION_FEED;
        st.feed_blockhead_unique_id_to_send = 0;
    } else if st.set_free_blockhead_unique_id_to_send != 0 {
        out.interact_blockhead_id = st.set_free_blockhead_unique_id_to_send;
        out.interaction_type = NET_NPC_INTERACTION_SET_FREE;
        st.set_free_blockhead_unique_id_to_send = 0;
    } else if st.capture_blockhead_unique_id_to_send != 0 {
        out.interact_blockhead_id = st.capture_blockhead_unique_id_to_send;
        out.interaction_type = NET_NPC_INTERACTION_CAPTURE;
        out.interaction_item_type = st.interaction_item_type_to_send;
        st.capture_blockhead_unique_id_to_send = 0;
        st.interaction_item_type_to_send = 0;
    } else if st.harvest_blockhead_unique_id_to_send != 0 && st.harvest_type_to_send != 0 {
        out.interact_blockhead_id = st.harvest_blockhead_unique_id_to_send;
        out.interaction_type = st.harvest_type_to_send;
        st.harvest_blockhead_unique_id_to_send = 0;
        st.harvest_type_to_send = 0;
    }
    out.rider_id = st.rider.as_ref().map_or(0, |r| r.unique_id());
    out.successful_feed_or_tame = 0;
    if st.send_successful_tame_state != 0 {
        out.successful_feed_or_tame = 2;
        st.send_successful_tame_state = SEND_STATE_SENT;
        st.send_successful_feed_state = SEND_STATE_SENT;
    } else if st.send_successful_feed_state != 0 {
        out.successful_feed_or_tame = 1;
        st.send_successful_feed_state = SEND_STATE_SENT;
    }
    out
}

/// `-[NPC npcUpdateNetDataForClient:]`.
pub fn npc_npc_update_net_data_for_client(me: &ObjRef, _cid: Option<&str>) -> NPCUpdateNetData {
    NPCUpdateNetData { dynamic_object_net_data: me.dynamic_object_net_data(me) }
}

/// `-[NPC appendNPCCreationDataToData:]`: appends the XML plist `{ "name" = name }` when named.
pub fn npc_append_npc_creation_data_to_data(me: &ObjRef, subclass_data: Vec<u8>) -> Vec<u8> {
    let name = npc_state(me).borrow().name.clone();
    let Some(name) = name else { return subclass_data };
    let mut out = subclass_data;
    let mut d = new_mutable_dict();
    d.map.insert("name".to_string(), PlistValue::string(name));
    if let Some(p) = data_from_property_list(&PlistValue::Dict(d)) {
        out.extend_from_slice(&p);
    }
    out
}

/// `-[NPC creationNetDataForClient:]`: 0x48 bytes plus the name tail.
pub fn npc_creation_net_data_for_client(me: &ObjRef, cid: Option<&str>) -> Option<Vec<u8>> {
    let npc = vn(me);
    let s = npc.npc_creation_net_data_for_client(me, cid);
    Some(npc.append_npc_creation_data_to_data(me, s.to_vec()))
}

/// `-[NPC updateNetDataForClient:]`: 0x18 bytes.
pub fn npc_update_net_data_for_client(me: &ObjRef, cid: Option<&str>) -> Option<Vec<u8>> {
    Some(vn(me).npc_update_net_data_for_client(me, cid).to_vec())
}

/// `-[NPC getSaveDict]`. Number kinds per.
pub fn npc_get_save_dict(me: &ObjRef) -> PlistDict {
    let mut d = base_get_save_dict(me);
    let (rider, bhs) = {
        let r = npc_state(me).borrow().rider.clone();
        (r, me.dynamic_world().map(|dw| dw.blockheads()).unwrap_or_default())
    };
    let save_time = world_time(me) as f32;
    let s = npc_state(me).borrow();
    let m = &mut d.map;
    let mut put = |k: &str, v: PlistValue| {
        m.insert(k.to_string(), v);
    };
    put("damage", PlistValue::int(s.damage as i64));
    put("breed", PlistValue::int(s.breed as i64));
    put("fullness", PlistValue::real_f32(s.fullness));
    put("layTimer", PlistValue::real_f32(s.lay_timer));
    put("mateBreed", PlistValue::int(s.mate_breed as i64));
    put("tameCooldownTimer", PlistValue::real_f32(s.tame_cooldown_timer));
    put("layCooldownTimer", PlistValue::real_f32(s.lay_cooldown_timer));
    put("mateCooldownTimer", PlistValue::real_f32(s.mate_cooldown_timer));
    put("age", PlistValue::real_f32(s.age));
    put("hasBred", PlistValue::boolean(s.has_bred));
    put("hasBeenFedByBlockheadOrChest", PlistValue::boolean(s.has_been_fed_by_blockhead_or_chest));
    if let Some(n) = &s.name {
        put("name", PlistValue::string(n.clone()));
    }
    put("saveTime", PlistValue::real_f32(save_time));
    if let Some(tc) = &s.tame_counts_by_client_id {
        let mut dd = new_mutable_dict();
        for (k, v) in tc {
            dd.map.insert(k.clone(), PlistValue::int(*v));
        }
        put("tameCountsByClientID", PlistValue::Dict(dd));
    }
    if let Some(t) = &s.tamed_client_id {
        put("tamedClientID", PlistValue::string(t.clone()));
    }
    if let Some(r) = &rider {
        let mut i: i32 = 0;
        let mut found = false;
        for bh in &bhs {
            if ptr_eq(bh, r) && !bh.needs_removed() {
                found = true;
                break;
            }
            i += 1;
        }
        if found {
            put("currentBlockheadIndex", PlistValue::int(i as i64));
        }
    }
    d
}

/// `-[NPC dealloc]`: drops the retained refs; no game effect.
pub fn npc_dealloc(me: &ObjRef) {
    let mut s = npc_state(me).borrow_mut();
    s.name = None;
    s.tame_counts_by_client_id = None;
    s.tamed_client_id = None;
    s.kill_blockhead = None;
}

// ------------------------------------------------------------------------------------------------ update

/// `-[NPC update:accurateDT:isSimulation:]`. Only send-state decay, timers, aging, temperature,
/// hunger and rider bookkeeping; no movement.
pub fn npc_update_accurate_dt_is_simulation(me: &ObjRef, dt: f32, _accurate_dt: f32, _is_simulation: bool) {
    let npc = vn(me);
    let env = npc_env();
    {
        let mut s = npc.npc().borrow_mut();
        if s.send_successful_tame_state == SEND_STATE_SENT {
            s.send_successful_tame_state = 0;
        }
        if s.send_successful_feed_state == SEND_STATE_SENT {
            s.send_successful_feed_state = 0;
        }
        if s.send_was_hit_state == SEND_STATE_SENT {
            s.send_was_hit_state = 0;
        }
    }
    if !me.is_net() {
        let pos = me.pos();
        let tile = env.tile_probe(pos.x, pos.y);
        let dead = npc.npc().borrow().dead;
        let on_ice = matches!(&tile, Some(t) if t.type_index == TILE_ICE);
        if !dead && !on_ice {
            {
                let mut s = npc.npc().borrow_mut();
                s.tame_cooldown_timer -= dt;
                if s.tame_cooldown_timer < 0.0 {
                    s.tame_cooldown_timer = 0.0;
                }
                s.lay_cooldown_timer -= dt;
                s.mate_cooldown_timer -= dt;
                if (s.send_successful_tame_state != 0 || s.send_successful_feed_state != 0 || s.send_was_hit_state != 0)
                    && is_server_dw(me)
                    && env.server_clients_count() == 0
                {
                    s.send_successful_tame_state = 0;
                    s.send_successful_feed_state = 0;
                    s.send_was_hit_state = 0;
                }
            }
            // aging
            let max_age = npc.max_age();
            {
                let mut s = npc.npc().borrow_mut();
                if s.tamed_client_id.is_none() {
                    s.age += dt;
                } else {
                    let half = (max_age as f64) / 2.0f64;
                    if half <= s.age as f64 {
                        s.age = ((max_age as f64) / 2.0f64) as f32;
                    } else {
                        s.age += dt;
                    }
                }
                s.fullness -= dt;
                s.temperature_check_timer -= dt;
            }
            // temperature
            if npc.npc().borrow().temperature_check_timer <= 0.0 {
                // [world worldTime] is read three times, in this order
                let day = env.day_night_fraction(pos.x as f32, world_time(me));
                let season = env.season_for_world_x(pos.x, world_time(me));
                let weather = env.weather_fraction(pos, world_time(me));
                let mut temp = match &tile {
                    Some(t) => env.current_temperature(t, pos, season, day, weather),
                    None => {
                        ub_null_own_tile(); // UB-PARITY:
                        0.0
                    }
                };
                let below = env.tile_probe(pos.x, pos.y - 1);
                let own_fire = tile.is_some_and(|t| t.on_fire != 0);
                if own_fire || matches!(&below, Some(b) if b.on_fire != 0 || b.type_index == TILE_LAVA) {
                    temp += 100.0f32;
                }
                let mut s = npc.npc().borrow_mut();
                s.current_temperature = temp;
                s.temperature_check_timer = 2.0f32;
            }
            let ct = npc.npc().borrow().current_temperature;
            if 100.0f32 < ct && npc.suffers_damage_at_high_temperatures() {
                let dd = {
                    let mut s = npc.npc().borrow_mut();
                    s.damage_delay_timer =
                        ((s.damage_delay_timer as f64) - ((dt * (ct - 100.0f32)) as f64) * 0.01f64) as f32;
                    s.damage_delay_timer
                };
                if dd <= 0.0 {
                    let mut f = (((ct - 100.0f32) as f64) * 0.2f64) as f32;
                    if f < 1.0f32 {
                        f = 1.0f32;
                    }
                    npc.hit_with_force_blockhead(me, f32_to_i32(f), None);
                    npc.npc().borrow_mut().damage_delay_timer = r01("NPC.update.heat") * 0.5f32 + 0.5f32;
                }
            }
            // hunger
            let min_f = npc.min_fullness();
            if npc.npc().borrow().fullness < min_f {
                npc.npc().borrow_mut().fullness = min_f;
                if npc.dies_of_low_fullness() {
                    let t = {
                        let mut s = npc.npc().borrow_mut();
                        s.random_harm_from_hunger_timer -= dt;
                        s.random_harm_from_hunger_timer
                    };
                    if t <= 0.0 {
                        npc.hit_with_force_blockhead(me, (npc.max_health() as i32) / 8, None);
                        npc.npc().borrow_mut().random_harm_from_hunger_timer =
                            r01("NPC.update.hunger") * 10.0f32 + 20.0f32;
                    }
                }
            }
            // old age
            let age = npc.npc().borrow().age;
            if npc.max_age() < age && npc.dies_of_old_age() {
                npc.die(me, None);
            }
            // rider bookkeeping (messages to a nil rider return 0/NO)
            let rider = npc.npc().borrow().rider.clone();
            let paused = rider.as_ref().is_some_and(|r| env.blockhead_paused(r));
            if !paused {
                let rider = npc.npc().borrow().rider.clone();
                if rider.as_ref().is_some_and(|r| r.needs_removed()) {
                    let mut s = npc.npc().borrow_mut();
                    if !rider.as_ref().is_some_and(|r| r.is_net()) {
                        drop(s);
                        me.set_creation_data_needs_to_be_sent(true);
                        s = npc.npc().borrow_mut();
                    }
                    s.rider = None;
                    s.being_controlled = false;
                }
                if npc.npc().borrow().being_controlled {
                    let rider = npc.npc().borrow().rider.clone();
                    if !rider.as_ref().is_some_and(|r| r.is_net()) {
                        let fire = {
                            let mut s = npc.npc().borrow_mut();
                            s.remote_send_control_event_timer += dt;
                            if 0.5f32 < s.remote_send_control_event_timer {
                                s.remote_send_control_event_timer -= 0.5f32;
                                true
                            } else {
                                false
                            }
                        };
                        if fire {
                            me.set_creation_data_needs_to_be_sent(true);
                        }
                    }
                }
            }
        }
    } else {
        // isNet replica path (reachable on the server,)
        let tamed = npc.tamed();
        let min_f = npc.min_fullness();
        let max_age = npc.max_age();
        let mut s = npc.npc().borrow_mut();
        s.tame_cooldown_timer -= dt;
        if s.tame_cooldown_timer < 0.0 {
            s.tame_cooldown_timer = 0.0;
        }
        s.fullness -= dt;
        if s.fullness < min_f {
            s.fullness = min_f;
        }
        if !tamed {
            s.age += dt;
        } else {
            let half = (max_age as f64) / 2.0f64;
            if half <= s.age as f64 {
                s.age = ((max_age as f64) / 2.0f64) as f32;
            } else {
                s.age += dt;
            }
        }
    }
}

/// `-[NPC remoteUpdate:]`: only `[super remoteUpdate:]`.
pub fn npc_remote_update(me: &ObjRef, net_data: &[u8]) {
    base_remote_update(me, net_data)
}

/// `-[NPC remoteCreationDataUpdate:]`. Server branch plus (for completeness) the client branch.
///  Short data is zero padded (E-F9).
pub fn npc_remote_creation_data_update(me: &ObjRef, net_data: &[u8]) {
    base_remote_creation_data_update(me, net_data);
    let npc = vn(me);
    let env = npc_env();
    let d = <NPCCreationNetData as WireStruct>::read_le(&padded_bytes(net_data, 0x48));
    if !env.is_client() {
        // === SERVER branch ===
        if d.hit_force != 0 {
            let hb = env.blockhead_with_id_including_net(d.kill_blockhead_id);
            npc.hit_with_force_blockhead(me, d.hit_force as i32, hb.as_ref());
        }
        if d.interact_blockhead_id != 0 && d.interaction_type != 0 {
            if let Some(b) = env.blockhead_with_id_including_net(d.interact_blockhead_id) {
                match d.interaction_type {
                    NET_NPC_INTERACTION_FEED => {
                        npc.feed_by_blockhead(me, &b);
                    }
                    NET_NPC_INTERACTION_SET_FREE => npc.set_free_by_blockhead(me, &b),
                    NET_NPC_INTERACTION_CAPTURE => {
                        npc.capture_by_blockhead_with_item_type(me, &b, d.interaction_item_type as i32);
                    }
                    NET_NPC_INTERACTION_HARVEST_MILK => {
                        npc.milk_by_blockhead(me, &b);
                    }
                    NET_NPC_INTERACTION_HARVEST_SHAVE => {
                        npc.shave_by_blockhead(me, &b);
                    }
                    _ => {}
                }
            }
        }
        // rider reconciliation (server): only when we currently have a rider
        let rider = npc.npc().borrow().rider.clone();
        if let Some(rider) = rider {
            if !rider.is_net() {
                if d.rider_id != 0 && d.rider_id != rider.unique_id() {
                    env.blockhead_stop_riding(&rider);
                    // the second `rider` is read AFTER stopRiding (usually nil by then):
                    let r2 = npc.npc().borrow().rider.clone();
                    npc.remove_rider_opt(me, r2.as_ref()); // virtual, also for a nil argument
                }
            } else if d.rider_id == 0 {
                env.blockhead_stop_riding(&rider);
                let mut s = npc.npc().borrow_mut();
                s.rider = None;
                s.being_controlled = false;
            } else if d.rider_id != rider.unique_id() {
                for bh in env.all_blockheads_including_net() {
                    if bh.is_net() && bh.unique_id() == d.rider_id {
                        npc.add_rider(me, &bh);
                    }
                    npc.npc().borrow_mut().being_controlled = true; // per element (quirk of the original)
                }
            }
        }
    } else {
        // === CLIENT branch (never reached on the dedicated server) ===
        if d.hit_force != 0 {
            npc.react_to_being_hit(me);
        }
        let rider = npc.npc().borrow().rider.clone();
        match rider {
            Some(rider) if !rider.is_net() => {
                if d.rider_id != 0 && d.rider_id != rider.unique_id() {
                    env.blockhead_stop_riding(&rider);
                    let r2 = npc.npc().borrow().rider.clone();
                    npc.remove_rider_opt(me, r2.as_ref()); // virtual, also for a nil argument
                }
            }
            rider => {
                if d.rider_id == 0 {
                    if let Some(r) = &rider {
                        env.blockhead_stop_riding(r);
                    }
                    let mut s = npc.npc().borrow_mut();
                    s.rider = None;
                    s.being_controlled = false;
                } else {
                    for bh in env.all_blockheads_including_net() {
                        if bh.is_net() && bh.unique_id() == d.rider_id {
                            npc.add_rider(me, &bh);
                        }
                        npc.npc().borrow_mut().being_controlled = true;
                    }
                }
            }
        }
        {
            let mut s = npc.npc().borrow_mut();
            s.damage = d.damage;
            s.fullness = (d.fullness as i32) as f32;
            s.age = (d.age as i32) as f32;
            s.tame_cooldown_timer = (d.tame_cooldown_timer as i32) as f32;
        }
        let dead = npc.npc().borrow().dead;
        if d.dead != 0 && !dead {
            npc.die(me, None);
        }
        npc.npc().borrow_mut().net_tame_type = d.tame_type;
        if d.successful_feed_or_tame == 2 {
            npc.successful_tame(me);
            npc.react_to_being_fed(me);
        } else if d.successful_feed_or_tame == 1 {
            npc.react_to_being_fed(me);
        }
    }
    // === name tail (both) ===
    let sz = npc.creation_data_struct_size();
    let len = (net_data.len() as i32).wrapping_sub(sz as i32);
    if len < 1 {
        if env.is_client() {
            npc.npc().borrow_mut().name = None;
        }
    } else {
        let other = &net_data[sz..];
        let plist = property_list_with_data_raw(other, 0);
        let name_val: Option<String> = match &plist {
            None => None,
            Some(PlistValue::Dict(dd)) => {
                dd.map.get(&"name".to_string()).and_then(|v| v.as_str()).map(|x| x.to_string())
            }
            Some(other) => unrecognized_selector(other.class_name(), "objectForKey:"),
        };
        match name_val {
            None => {
                if env.is_client() {
                    npc.npc().borrow_mut().name = None;
                }
            }
            Some(n) => {
                let has_server = me.world().and_then(|w| w.server()).is_some();
                let n = if has_server { env.remove_curse_words_from_blockhead_name(&n) } else { n };
                npc.npc().borrow_mut().name = Some(n);
            }
        }
    }
}

// ------------------------------------------------------------------------------------------------ damage + death

/// `-[NPC die:]`. A `die(None)` clears a stored killer.
pub fn npc_die(me: &ObjRef, killer: Option<&ObjRef>) {
    let npc = vn(me);
    if npc.npc().borrow().dead {
        return;
    }
    let rider = {
        let mut s = npc.npc().borrow_mut();
        s.kill_blockhead = killer.cloned();
        s.dead = true;
        s.rider.clone()
    };
    if let Some(r) = rider {
        npc_env().blockhead_stop_riding(&r); // re-entrant: may clear `rider`
    }
    dw_changed(me);
    me.set_creation_data_needs_to_be_sent(true);
}

/// `-[NPC hitWithForce:blockhead:]`.
pub fn npc_hit_with_force_blockhead(me: &ObjRef, force: i32, blockhead: Option<&ObjRef>) {
    let npc = vn(me);
    let max_h = npc.max_health();
    let survived = {
        let mut s = npc.npc().borrow_mut();
        s.damage = s.damage.wrapping_add(force as i16 as u16);
        s.damage < max_h
    };
    if !npc_env().is_client() {
        if survived {
            if let Some(b) = blockhead {
                let cid = blockhead_client_id(b).unwrap_or_else(|| "server".to_string());
                let mut s = npc.npc().borrow_mut();
                let counts = s.tame_counts_by_client_id.get_or_insert_with(BTreeMap::new);
                let n = counts.get(&cid).copied().unwrap_or(0);
                counts.insert(cid, n.wrapping_sub(1));
            }
        } else {
            npc.die(me, blockhead);
        }
        dw_changed(me);
    } else {
        let mut s = npc.npc().borrow_mut();
        s.kill_blockhead = blockhead.cloned();
    }
    {
        let mut s = npc.npc().borrow_mut();
        s.send_was_hit_state = SEND_STATE_SEND;
        s.hit_force = force as u16;
    }
    me.set_creation_data_needs_to_be_sent(true);
    if !me.is_net() {
        npc.react_to_being_hit(me);
    }
}

/// `-[NPC willDieIfHitByForce:]`.
pub fn npc_will_die_if_hit_by_force(me: &ObjRef, force: i32) -> bool {
    vn(me).will_die_if_hit_by_force(force)
}

/// `-[NPC healthFraction]`
pub fn npc_health_fraction(me: &ObjRef) -> f32 {
    vn(me).health_fraction()
}

/// `-[NPC fullnessFraction]`
pub fn npc_fullness_fraction(me: &ObjRef) -> f32 {
    vn(me).fullness_fraction()
}

// ------------------------------------------------------------------------------------------------ taming

/// `-[NPC canBeFedByBlockhead:]`.
pub fn npc_can_be_fed_by_blockhead(me: &ObjRef, _b: &ObjRef) -> bool {
    let npc = vn(me);
    let (damage, tcd, fullness) = {
        let s = npc.npc().borrow();
        (s.damage, s.tame_cooldown_timer, s.fullness)
    };
    if !npc.dies_of_low_fullness() {
        if damage != 0 {
            return true;
        }
        tcd <= 0.0f32 && !npc.tamed()
    } else if 5400.0f64 <= fullness as f64 {
        damage != 0
    } else {
        true
    }
}

/// `-[NPC feedByBlockhead:]` with the documented tame bug reproduced: the
/// `highest` loop looks up the FEEDER's key, so any other key in `tameCountsByClientID` blocks taming.
pub fn npc_feed_by_blockhead(me: &ObjRef, b: &ObjRef) -> bool {
    let npc = vn(me);
    if !npc.can_be_fed_by_blockhead(me, b) {
        return false;
    }
    let max_h = npc.max_health();
    {
        let mut s = npc.npc().borrow_mut();
        s.fullness = ((s.fullness as f64) + 2700.0f64) as f32;
        if (s.fullness as f64) > 8100.0f64 {
            s.fullness = 8100.0f32;
        }
        if s.damage != 0 {
            let half = (max_h as u32) / 2;
            let amount = if (s.damage as u32) < half { s.damage as u32 } else { half };
            s.damage = (s.damage as u32).wrapping_sub(amount) as u16;
        }
    }
    if !me.is_net() {
        let mut do_tame: Option<String> = None;
        {
            let mut s = npc.npc().borrow_mut();
            s.has_been_fed_by_blockhead_or_chest = true;
            if s.tame_cooldown_timer <= 0.0f32 {
                s.tame_cooldown_timer = 675.0f32;
                let cid = blockhead_client_id(b).unwrap_or_else(|| "server".to_string());
                let req = crate::items::breeds::tame_count_requirement_for_npc_type(NPCType(npc.npc_type() as u32));
                let counts = s.tame_counts_by_client_id.get_or_insert_with(BTreeMap::new);
                let count: i64 = counts.get(&cid).copied().unwrap_or(0).wrapping_add(1);
                counts.insert(cid.clone(), count);
                let count_i = count as i32; // (int) truncation of the integerValue
                if req <= count_i {
                    let already_owner = s.tamed_client_id.as_deref() == Some(cid.as_str());
                    if !already_owner {
                        let counts = s.tame_counts_by_client_id.as_ref().unwrap();
                        let mut highest: i32 = 0;
                        for k in counts.keys() {
                            if k != &cid {
                                // ORIGINAL BUG: the lookup uses the feeder's key, not `k`.
                                let v = counts.get(&cid).copied().unwrap_or(0) as i32;
                                if v > highest {
                                    highest = v;
                                }
                            }
                        }
                        if highest < count_i {
                            s.tamed_client_id = Some(cid.clone());
                            do_tame = Some(cid);
                        }
                    }
                }
            }
        }
        if do_tame.is_some() {
            npc.successful_tame(me);
            npc.npc().borrow_mut().send_successful_tame_state = SEND_STATE_SEND;
        }
        npc.npc().borrow_mut().send_successful_feed_state = SEND_STATE_SEND;
        npc.react_to_being_fed(me);
        dw_changed(me);
        me.set_creation_data_needs_to_be_sent(true);
        true
    } else {
        npc.npc().borrow_mut().feed_blockhead_unique_id_to_send = b.unique_id();
        me.set_creation_data_needs_to_be_sent(true);
        npc.npc().borrow_mut().tame_cooldown_timer = 675.0f32;
        true
    }
}

/// `-[NPC successfulTame]`. Chime and particles are client visuals; the particle loop still burns
/// exactly 128 x 7 = 896 `rand()` draws after `generateNewName`.
pub fn npc_successful_tame(me: &ObjRef) {
    let npc = vn(me);
    let need_name = npc.npc().borrow().name.is_none() && !me.is_net();
    if need_name {
        npc.generate_new_name(me);
    }
    for _ in 0..128 {
        for _ in 0..7 {
            glibc_rand::rand_tagged("NPC.tame.particle");
        }
    }
}

/// `-[NPC generateNewName]`: 2 draws.
pub fn npc_generate_new_name(me: &ObjRef) {
    let npc = vn(me);
    let r1 = r01("NPC.name");
    let r2 = r01("NPC.name");
    let lower: String = if (r2 as f64) <= 0.99f64 {
        let names = npc.get_names_array();
        let n = npc.get_names_array_count();
        match names {
            Some(a) if n >= 1 => {
                let idx = f32_to_i32(r1 * (n as f32)) % n;
                a.get(idx as usize).copied().unwrap_or("Bob").to_string()
            }
            _ => "Bob".to_string(),
        }
    } else {
        NPC_NAMES_RARE[(f32_to_i32(r1 * 44.0f32) % 44) as usize].to_string()
    };
    npc.npc().borrow_mut().name = Some(nsstring::uppercase_string(&lower));
    dw_changed(me);
    me.set_creation_data_needs_to_be_sent(true);
}

/// `-[NPC changeName:]`: accepts `1 <= utf16_len < 15`; no profanity filter here.
pub fn npc_change_name(me: &ObjRef, new_name: &str) {
    let len = nsstring::length(new_name);
    if len != 0 && len < 15 {
        npc_state(me).borrow_mut().name = Some(new_name.to_string());
        me.set_creation_data_needs_to_be_sent(true);
        dw_changed(me);
    }
}

fn npc_tamed_state(st: &RefCell<NpcState>, is_net: bool) -> bool {
    let s = st.borrow();
    if !is_net {
        s.tamed_client_id.is_some()
    } else {
        s.net_tame_type != 0
    }
}

fn npc_belongs_to_player_state(st: &RefCell<NpcState>, is_net: bool, b: &ObjRef) -> bool {
    let s = st.borrow();
    if !is_net {
        let Some(t) = &s.tamed_client_id else { return false };
        let cid = blockhead_client_id(b);
        if cid.as_deref() == Some(t.as_str()) {
            return true;
        }
        cid.is_none() && t == "server"
    } else {
        s.net_tame_type == 1 && !b.is_net()
    }
}

fn npc_belongs_to_local_player_state(st: &RefCell<NpcState>, is_net: bool, not_client: bool) -> bool {
    let s = st.borrow();
    if !is_net {
        let Some(t) = &s.tamed_client_id else { return false };
        if t == "server" {
            return not_client;
        }
        npc_env().local_net_id().as_deref() == Some(t.as_str())
    } else {
        s.net_tame_type == 1
    }
}

/// `-[NPC tamed]`
pub fn npc_tamed(me: &ObjRef) -> bool {
    vn(me).tamed()
}

/// `-[NPC belongsToPlayerWithBlockhead:]`
pub fn npc_belongs_to_player_with_blockhead(me: &ObjRef, b: &ObjRef) -> bool {
    vn(me).belongs_to_player_with_blockhead(b)
}

/// `-[NPC belongsToLocalPlayer]`
pub fn npc_belongs_to_local_player(me: &ObjRef) -> bool {
    vn(me).belongs_to_local_player()
}

/// `-[NPC setFreeByBlockhead:]`.
pub fn npc_set_free_by_blockhead(me: &ObjRef, b: &ObjRef) {
    let npc = vn(me);
    if !me.is_net() {
        if npc.belongs_to_player_with_blockhead(b) {
            npc.hit_with_force_blockhead(me, 4, Some(b));
            npc.npc().borrow_mut().tamed_client_id = None;
            dw_changed(me);
            me.set_creation_data_needs_to_be_sent(true);
            npc.npc().borrow_mut().name = None;
        }
    } else {
        npc.npc().borrow_mut().set_free_blockhead_unique_id_to_send = b.unique_id();
        me.set_creation_data_needs_to_be_sent(true);
    }
}

/// `-[NPC captureByBlockhead:withItemType:]`.
pub fn npc_capture_by_blockhead_with_item_type(me: &ObjRef, b: &ObjRef, item: i32) -> bool {
    let npc = vn(me);
    if !npc.can_be_captured_by_blockhead_with_item_type(b, item) {
        return false;
    }
    if !me.is_net() {
        let sd = me.get_save_dict(me); // virtual
        npc_env().create_free_block_for_captured_npc(me.pos(), npc.captured_item_type(), &sd, b);
        me.set_needs_removed(me, true); // virtual
        nslog!("Captured");
        true
    } else {
        {
            let mut s = npc.npc().borrow_mut();
            s.capture_blockhead_unique_id_to_send = b.unique_id();
            s.interaction_item_type_to_send = item as u16;
        }
        me.set_creation_data_needs_to_be_sent(true);
        true
    }
}

// ------------------------------------------------------------------------------------------------ breeding

/// `-[NPC canMate]`
pub fn npc_can_mate(me: &ObjRef) -> bool {
    vn(me).can_mate()
}

/// `-[NPC mateWithNPC:]`: one draw, f64 math then narrow.
pub fn npc_mate_with_npc(me: &ObjRef, other: &ObjRef) -> bool {
    let npc = vn(me);
    {
        let s = npc.npc().borrow();
        if me.is_net() || 0.0f32 < s.mate_cooldown_timer {
            return false;
        }
    }
    let r = r01("NPC.mate");
    npc.npc().borrow_mut().mate_cooldown_timer = (60.0f64 + (r as f64) * 60.0f64) as f32;
    dw_changed(me);
    let ob = as_npc(other).map_or(0, |o| o.breed());
    let mut s = npc.npc().borrow_mut();
    s.has_bred = true;
    s.mate_breed = ob;
    true
}

/// `-[NPC checkCurrentPositionForFood]`: eats a food free block, then a feeder chest.
pub fn npc_check_current_position_for_food(me: &ObjRef) {
    let npc = vn(me);
    let env = npc_env();
    if me.is_net() {
        return;
    }
    if (npc.npc().borrow().fullness as f64) >= 5400.0f64 {
        return;
    }
    let pos = me.pos();
    if let Some(fbs) = env.free_blocks_at_pos(pos) {
        let food = npc.food_item_type();
        for fb in fbs {
            if env.free_block_item_type(&fb) == food {
                fb.set_needs_removed(&fb, true);
                {
                    let mut s = npc.npc().borrow_mut();
                    s.fullness = ((s.fullness as f64) + 2700.0f64) as f32;
                }
                me.set_creation_data_needs_to_be_sent(true);
                dw_changed(me);
                npc.react_to_being_fed(me);
                break;
            }
        }
    }
    if (npc.npc().borrow().fullness as f64) < 5400.0f64 {
        if let Some(io) = env.interaction_object_at_pos(pos) {
            if env.feeder_chest_remove_item_if_available(&io, npc.food_item_type()) == Some(true) {
                {
                    let mut s = npc.npc().borrow_mut();
                    s.has_been_fed_by_blockhead_or_chest = true;
                }
                me.set_creation_data_needs_to_be_sent(true);
                {
                    let mut s = npc.npc().borrow_mut();
                    s.fullness = ((s.fullness as f64) + 2700.0f64) as f32;
                }
                dw_changed(me);
                npc.react_to_being_fed(me);
            }
        }
    }
}

// ------------------------------------------------------------------------------------------------ riding

/// `-[NPC addRider:]`.
pub fn npc_add_rider(me: &ObjRef, r: &ObjRef) {
    let npc = vn(me);
    let env = npc_env();
    let mut incoming: Option<ObjRef> = Some(r.clone());
    if is_client_blockhead_controlled_by_server(r) {
        env.blockhead_stop_riding(r);
        npc.npc().borrow_mut().rider = None;
        incoming = None;
    }
    npc.npc().borrow_mut().rider = incoming;
    dw_changed(me);
    let rider = npc.npc().borrow().rider.clone();
    if !(env.is_client() && rider.as_ref().is_some_and(|x| x.is_net())) {
        me.set_creation_data_needs_to_be_sent(true);
    }
    let mut s = npc.npc().borrow_mut();
    s.being_controlled = s.rider.is_some();
}

/// `-[NPC removeRider:]`.
pub fn npc_remove_rider(me: &ObjRef, r: &ObjRef) {
    npc_remove_rider_opt(me, Some(r))
}

/// `removeRider:` with a possibly nil argument: `nil == nil` does NOT return early.
pub fn npc_remove_rider_opt(me: &ObjRef, r: Option<&ObjRef>) {
    let npc = vn(me);
    let cur = npc.npc().borrow().rider.clone();
    let same = match (&cur, r) {
        (None, None) => true,
        (Some(a), Some(b)) => ptr_eq(a, b),
        _ => false,
    };
    if !same {
        return;
    }
    dw_changed(me);
    if !(npc_env().is_client() && cur.as_ref().is_some_and(|x| x.is_net())) {
        me.set_creation_data_needs_to_be_sent(true);
    }
    let mut s = npc.npc().borrow_mut();
    s.rider = None;
    s.being_controlled = false;
}

/// `-[NPC blockheadsLoaded]`: reattaches the saved rider. `beingControlled = YES` even with a nil rider.
pub fn npc_blockheads_loaded(me: &ObjRef) {
    let npc = vn(me);
    let env = npc_env();
    let idx = npc.npc().borrow().saved_blockhead_index;
    if idx == -1 {
        return;
    }
    let bhs = me.dynamic_world().map(|d| d.blockheads()).unwrap_or_default();
    if idx >= 0 && (idx as usize) < bhs.len() {
        npc.npc().borrow_mut().rider = Some(bhs[idx as usize].clone());
    }
    npc.npc().borrow_mut().saved_blockhead_index = -1;
    let rider = npc.npc().borrow().rider.clone();
    let ctl = rider.as_ref().is_some_and(is_client_blockhead_controlled_by_server);
    if !ctl {
        if let Some(r) = &rider {
            env.blockhead_set_riding_object(r, Some(me));
        }
        npc.npc().borrow_mut().being_controlled = true;
    } else {
        let r = rider.unwrap();
        env.blockhead_stop_riding(&r);
        let r2 = npc.npc().borrow().rider.clone();
        npc.remove_rider_opt(me, r2.as_ref()); // virtual, also for a nil argument
    }
}

/// `-[NPC blockheadUnloaded:]`.
pub fn npc_blockhead_unloaded(me: &ObjRef, b: &ObjRef) {
    let mut s = npc_state(me).borrow_mut();
    if s.kill_blockhead.as_ref().is_some_and(|k| ptr_eq(k, b)) {
        s.kill_blockhead = None;
    }
    if weak_is(&s.inspecting_blockhead, b) {
        s.inspecting_blockhead = None;
    }
    if weak_is(&s.coming_to_inspect_blockhead, b) {
        s.coming_to_inspect_blockhead = None;
    }
    if s.rider.as_ref().is_some_and(|r| ptr_eq(r, b)) {
        s.rider = None;
        s.being_controlled = false;
    }
}

/// `-[NPC updatePosition:]`. Door handling uses the OLD `pos`.
pub fn npc_update_position(me: &ObjRef, new_pos: IntPair) {
    let npc = vn(me);
    let env = npc_env();
    let pos = me.pos();
    let rider = npc.npc().borrow().rider.clone();
    let rider_net = rider.as_ref().is_some_and(|r| r.is_net());
    if rider.is_some() && !rider_net && new_pos.x != pos.x {
        env.mark_circum_navigate_x(new_pos.x);
    }
    let moved = new_pos.x != pos.x || new_pos.y != pos.y;
    let mut send = true;
    if me.is_net() && (rider.is_none() || rider_net) {
        send = false;
    }
    if send && moved {
        me.set_update_needs_to_be_sent(true);
        if rider.is_none() {
            me.set_unreliable_update_needs_to_be_sent(true);
        } else {
            me.set_update_needs_to_be_sent(true);
        }
    }
    if rider.is_some() && moved {
        let dwo = me.dynamic_world();
        let set_door = |p: IntPair, open: bool, dir: i32| {
            if dwo.is_some() {
                env.set_door_at_pos_to_open_direction(p, open, dir);
            }
        };
        if env.tile_contains_door_like(pos.x, pos.y) {
            set_door(pos, false, 0);
        } else if env.tile_contains_door(pos.x, pos.y + 1) {
            set_door(IntPair { x: pos.x, y: pos.y + 1 }, false, 0);
        }
        let direction = if pos.x < new_pos.x { 1 } else { -1 };
        if env.tile_contains_door_like(new_pos.x, new_pos.y) {
            set_door(new_pos, true, direction);
        } else if env.tile_contains_door(new_pos.x, new_pos.y + 1) {
            set_door(IntPair { x: new_pos.x, y: new_pos.y + 1 }, true, direction);
        }
    }
    base_update_position(me, new_pos);
}

/// `-[NPC canBeRemovedByBlockhead:]` = `belongsToPlayerWithBlockhead:`.
pub fn npc_can_be_removed_by_blockhead(me: &ObjRef, b: &ObjRef) -> bool {
    vn(me).belongs_to_player_with_blockhead(b)
}

/// `-[NPC riderPosForBlockhead:]` (lane 3 = 0.0).
pub fn npc_rider_pos_for_blockhead(me: &ObjRef, b: &ObjRef) -> [f32; 4] {
    let fp = me.float_pos();
    let _ = b;
    [fp.x + 0.0f32, fp.y + 0.8f32, -0.75f32, 1.0]
}

/// `-[NPC cameraPosForBlockhead:]` = `renderPos` (virtual).
pub fn npc_camera_pos_for_blockhead(me: &ObjRef, _b: &ObjRef) -> Vector2 {
    me.render_pos()
}

/// `-[NPC secondOptionTitle]`
pub fn npc_second_option_title(me: &ObjRef) -> Option<&'static str> {
    vn(me).second_option_title()
}

/// `-[NPC cantBeFedTipStringForBlockhead:]`
pub fn npc_cant_be_fed_tip_string_for_blockhead(me: &ObjRef) -> String {
    format!("THAT {} ISN'T HUNGRY\nTRY AGAIN LATER", vn(me).species_name())
}

// ------------------------------------------------------------------------------------------------ factory

/// A registered NPC constructor (`X::new_for_registry` of a subclass): `alloc_npc`, subclass init, `Some(obj)` or `None`.
pub type NpcCtor =
    fn(&WorldRef, &DynamicWorldRef, IntPair, CacheRef, Option<&PlistDict>, bool, bool, Option<&str>) -> Option<ObjRef>;

/// Registers the constructor of NPC type `npc_type` (1..=8); called by `object_registry::install`.
pub fn register_npc_ctor(npc_type: i32, f: NpcCtor) {
    if (1..=8).contains(&npc_type) {
        CTORS.with(|c| c.borrow_mut()[npc_type as usize] = Some(f));
    }
}

/// `[[classForNPCType(t) alloc] initWithWorld:...isAdult:wasPlaced:placedByClient:]`. A type
/// outside 1..=8 (including 0) has no class: the message to the nil class yields nil.
#[allow(clippy::too_many_arguments)]
pub fn new_npc_for_type(
    npc_type: i32,
    world: &WorldRef,
    dw: &DynamicWorldRef,
    pos: IntPair,
    cache: CacheRef,
    save_dict: Option<&PlistDict>,
    is_adult: bool,
    was_placed: bool,
    client_id: Option<&str>,
) -> Option<ObjRef> {
    if !(1..=8).contains(&npc_type) {
        return None;
    }
    let f = CTORS.with(|c| c.borrow()[npc_type as usize])?;
    f(world, dw, pos, cache, save_dict, is_adult, was_placed, client_id)
}

// ------------------------------------------------------------------------------------------------ tests

#[cfg(test)]
mod tests {
    use super::*;
    use crate::objects::base::testutil::{setup, MockDw, MockWorld};
    use crate::objects::base::DynamicObjectBase;
    use std::cell::Cell;

    static TEST_NPC_CLASS: ClassInfo = ClassInfo { name: "TestNpc", superclass: Some(&NPC_CLASS), protocols: &[] };

    struct TestNpc {
        base: RefCell<DynamicObjectBase>,
        npc: RefCell<NpcState>,
        ty: i32,
    }
    impl TestNpc {
        fn alloc(ty: i32) -> ObjRef {
            alloc_npc(|w| TestNpc {
                base: RefCell::new(DynamicObjectBase::alloc_zeroed_with(w)),
                npc: RefCell::new(NpcState::default()),
                ty,
            })
        }
    }
    impl DynamicObject for TestNpc {
        fn base(&self) -> &RefCell<DynamicObjectBase> {
            &self.base
        }
        fn class_info(&self) -> &'static ClassInfo {
            &TEST_NPC_CLASS
        }
        fn as_any(&self) -> &dyn Any {
            self
        }
        npc_forward!(
            object_type,
            update_accurate_dt_is_simulation,
            get_save_dict,
            creation_net_data_for_client,
            update_net_data_for_client,
            remote_update,
            remote_creation_data_update,
            blockhead_unloaded,
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
    }
    impl Npc for TestNpc {
        fn npc(&self) -> &RefCell<NpcState> {
            &self.npc
        }
        fn npc_type(&self) -> i32 {
            self.ty
        }
        fn get_names_array(&self) -> Option<&'static [&'static str]> {
            Some(&["a", "b", "c", "d"])
        }
        fn get_names_array_count(&self) -> i32 {
            4
        }
        fn can_be_captured_by_blockhead_with_item_type(&self, _b: &ObjRef, _i: i32) -> bool {
            true
        }
        fn captured_item_type(&self) -> i32 {
            77
        }
        fn food_item_type(&self) -> i32 {
            9
        }
        fn dies_of_low_fullness(&self) -> bool {
            !NO_HUNGER.with(|c| c.get())
        }
    }
    impl_npc_protocols!(TestNpc);
    thread_local! {
        static NO_HUNGER: Cell<bool> = const { Cell::new(false) };
    }

    /// A stand-in Blockhead.
    struct TestBh {
        base: RefCell<DynamicObjectBase>,
        cid: Option<String>,
        ctl: Cell<bool>,
    }
    impl TestBh {
        fn alloc(id: u64, cid: Option<&str>) -> ObjRef {
            let o = crate::objects::handle::alloc_obj(|w| TestBh {
                base: RefCell::new(DynamicObjectBase::alloc_zeroed_with(w)),
                cid: cid.map(|s| s.to_string()),
                ctl: Cell::new(false),
            });
            o.base().borrow_mut().unique_id = id;
            o
        }
    }
    impl crate::objects::base::BlockheadApi for TestBh {
        fn client_id(&self) -> Option<String> {
            self.cid.clone()
        }
        fn local_net_id(&self) -> Option<String> {
            None
        }
        fn is_client_blockhead_being_controlled_by_server(&self) -> bool {
            self.ctl.get()
        }
        fn is_interacting_with_any_interaction_object(&self) -> bool {
            false
        }
        fn set_interaction_object(&self, _o: Option<&ObjRef>) {}
        fn stop_interacting(&self) {}
    }
    impl DynamicObject for TestBh {
        fn base(&self) -> &RefCell<DynamicObjectBase> {
            &self.base
        }
        fn class_info(&self) -> &'static ClassInfo {
            &DYNAMIC_OBJECT_CLASS
        }
        fn as_any(&self) -> &dyn Any {
            self
        }
        fn as_blockhead(&self) -> Option<&dyn crate::objects::base::BlockheadApi> {
            Some(self)
        }
    }

    struct TestEnv {
        server_clients: Cell<usize>,
        stops: RefCell<Vec<u64>>,
        bhs: RefCell<Vec<ObjRef>>,
        made: RefCell<Vec<(i32, usize)>>,
        client: Cell<bool>,
        tiles: RefCell<Vec<((i32, i32), NpcTileCopy)>>,
        temp: Cell<f32>,
        door_like: RefCell<Vec<(i32, i32)>>,
        door: RefCell<Vec<(i32, i32)>>,
        door_calls: RefCell<Vec<(IntPair, bool, i32)>>,
        circum: RefCell<Vec<i32>>,
        fbs: RefCell<Vec<ObjRef>>,
        fb_item: Cell<i32>,
        io: RefCell<Option<ObjRef>>,
        feeder: Cell<Option<bool>>,
        paused: Cell<bool>,
        riding: RefCell<Vec<u64>>,
    }
    impl TestEnv {
        fn new() -> Rc<TestEnv> {
            Rc::new(TestEnv {
                server_clients: Cell::new(1),
                stops: RefCell::new(vec![]),
                bhs: RefCell::new(vec![]),
                made: RefCell::new(vec![]),
                client: Cell::new(false),
                tiles: RefCell::new(vec![]),
                temp: Cell::new(20.0),
                door_like: RefCell::new(vec![]),
                door: RefCell::new(vec![]),
                door_calls: RefCell::new(vec![]),
                circum: RefCell::new(vec![]),
                fbs: RefCell::new(vec![]),
                fb_item: Cell::new(9),
                io: RefCell::new(None),
                feeder: Cell::new(None),
                paused: Cell::new(false),
                riding: RefCell::new(vec![]),
            })
        }
    }
    impl NpcEnvStub for TestEnv {
        fn server_clients_count(&self) -> usize {
            self.server_clients.get()
        }
        fn blockhead_with_id_including_net(&self, id: u64) -> Option<ObjRef> {
            self.bhs.borrow().iter().find(|b| b.unique_id() == id).cloned()
        }
        fn all_blockheads_including_net(&self) -> Vec<ObjRef> {
            self.bhs.borrow().clone()
        }
        fn blockhead_stop_riding(&self, b: &ObjRef) {
            self.stops.borrow_mut().push(b.unique_id());
        }
        fn create_free_block_for_captured_npc(&self, _pos: IntPair, item: i32, sd: &PlistDict, _b: &ObjRef) {
            self.made.borrow_mut().push((item, sd.map.len()));
        }
        fn is_client(&self) -> bool {
            self.client.get()
        }
        fn tile_probe(&self, x: i32, y: i32) -> Option<NpcTileCopy> {
            self.tiles.borrow().iter().find(|(k, _)| *k == (x, y)).map(|(_, t)| *t)
        }
        fn current_temperature(&self, _t: &NpcTileCopy, _p: IntPair, _s: f32, _d: f32, _w: f32) -> f32 {
            self.temp.get()
        }
        fn tile_contains_door(&self, x: i32, y: i32) -> bool {
            self.door.borrow().contains(&(x, y))
        }
        fn tile_contains_door_like(&self, x: i32, y: i32) -> bool {
            self.door_like.borrow().contains(&(x, y))
        }
        fn mark_circum_navigate_x(&self, x: i32) {
            self.circum.borrow_mut().push(x);
        }
        fn free_blocks_at_pos(&self, _pos: IntPair) -> Option<Vec<ObjRef>> {
            let v = self.fbs.borrow().clone();
            (!v.is_empty()).then_some(v)
        }
        fn free_block_item_type(&self, _fb: &ObjRef) -> i32 {
            self.fb_item.get()
        }
        fn interaction_object_at_pos(&self, _pos: IntPair) -> Option<ObjRef> {
            self.io.borrow().clone()
        }
        fn feeder_chest_remove_item_if_available(&self, _io: &ObjRef, _item: i32) -> Option<bool> {
            self.feeder.get()
        }
        fn set_door_at_pos_to_open_direction(&self, pos: IntPair, open: bool, direction: i32) {
            self.door_calls.borrow_mut().push((pos, open, direction));
        }
        fn blockhead_paused(&self, _b: &ObjRef) -> bool {
            self.paused.get()
        }
        fn blockhead_set_riding_object(&self, b: &ObjRef, _o: Option<&ObjRef>) {
            self.riding.borrow_mut().push(b.unique_id());
        }
    }

    struct Fx {
        w: Rc<MockWorld>,
        dw: Rc<MockDw>,
        env: Rc<TestEnv>,
    }
    fn fixture() -> Fx {
        let (w, dw, _log) = setup(4, 4);
        let env = TestEnv::new();
        set_npc_env(env.clone());
        NO_HUNGER.with(|c| c.set(false));
        glibc_rand::srand(1);
        Fx { w, dw, env }
    }
    fn make(fx: &Fx, ty: i32, adult: bool, sd: Option<&PlistDict>) -> ObjRef {
        let me = TestNpc::alloc(ty);
        let wr: WorldRef = fx.w.clone();
        let dr: DynamicWorldRef = fx.dw.clone();
        assert!(npc_init_with_world_dynamic_world_at_position_cache_save_dict_is_adult_was_placed_placed_by_client(
            &me,
            &wr,
            &dr,
            IntPair { x: 40, y: 3 },
            None,
            sd,
            adult,
            false,
            None
        ));
        me
    }
    fn st(me: &ObjRef) -> std::cell::RefMut<'_, NpcState> {
        npc_state(me).borrow_mut()
    }
    fn rd(me: &ObjRef) -> std::cell::Ref<'_, NpcState> {
        npc_state(me).borrow()
    }
    fn expected_r01(n: usize) -> Vec<f32> {
        glibc_rand::srand(1);
        (0..n).map(|_| glibc_rand::r01()).collect()
    }

    #[test]
    fn adult_start_values_rng_and_formula() {
        let fx = fixture();
        let r = expected_r01(4);
        glibc_rand::srand(1);
        let me = make(&fx, NPC_DODO, true, None);
        let s = st(&me);
        assert_eq!(s.age, ((r[0] * 9000.0f32) as f64 * 0.5f64) as f32);
        assert_eq!(s.fullness, 900.0f32 + r[1] * 900.0f32);
        assert_eq!(s.lay_cooldown_timer, 900.0f32 * r[2]);
        assert_eq!(s.random_harm_from_hunger_timer, r[3] * 20.0f32 + 1.0f32);
        assert_eq!(s.saved_blockhead_index, -1);
    }

    #[test]
    fn baby_start_values() {
        let fx = fixture();
        let r = expected_r01(3);
        glibc_rand::srand(1);
        let me = make(&fx, NPC_DODO, false, None);
        let s = st(&me);
        assert_eq!(s.age, 0.0);
        assert_eq!(s.fullness, 900.0 + r[0] * 900.0);
        assert_eq!(s.lay_cooldown_timer, 900.0 * r[1] + 900.0);
        assert_eq!(s.mate_cooldown_timer, 60.0);
        assert_eq!(s.random_harm_from_hunger_timer, r[2] * 20.0 + 1.0);
    }

    #[test]
    fn save_dict_kinds_roundtrip_and_load_guards() {
        let fx = fixture();
        let me = make(&fx, NPC_DODO, false, None);
        {
            let mut s = st(&me);
            s.damage = 3;
            s.breed = 2;
            s.fullness = 1189.942;
            s.age = 0.0;
            s.name = Some("BOB".into());
            s.tamed_client_id = Some("abc".into());
            s.tame_counts_by_client_id = Some(BTreeMap::from([("abc".to_string(), 2)]));
        }
        let d = me.get_save_dict(&me);
        let keys: Vec<String> = d.map.keys_in_order();
        for k in [
            "age",
            "breed",
            "damage",
            "floatPos",
            "fullness",
            "hasBeenFedByBlockheadOrChest",
            "hasBred",
            "layCooldownTimer",
            "layTimer",
            "mateBreed",
            "mateCooldownTimer",
            "pos_x",
            "pos_y",
            "saveTime",
            "tameCooldownTimer",
            "uniqueID",
            "name",
            "tamedClientID",
            "tameCountsByClientID",
        ] {
            assert!(keys.iter().any(|x| x == k), "missing {k}");
        }
        assert!(!keys.iter().any(|x| x == "currentBlockheadIndex"));
        assert!(matches!(d.map.get(&"damage".to_string()), Some(PlistValue::Int(3))));
        assert!(matches!(d.map.get(&"age".to_string()), Some(PlistValue::Real { is_float: true, .. })));
        assert!(matches!(d.map.get(&"hasBred".to_string()), Some(PlistValue::Bool(false))));
        // saveTime = (float)worldTime (mock: 123.5)
        assert_eq!(d.map.get(&"saveTime".to_string()).unwrap().float_value(), 123.5);
        // load into a fresh one
        let me2 = make(&fx, NPC_DODO, true, Some(&d));
        let s = st(&me2);
        assert_eq!(s.damage, 3);
        assert_eq!(s.breed, 2);
        assert_eq!(s.fullness, 1189.942f32);
        assert_eq!(s.name.as_deref(), Some("BOB"));
        assert_eq!(s.tamed_client_id.as_deref(), Some("abc"));
        assert_eq!(s.tame_counts_by_client_id.as_ref().unwrap()["abc"], 2);
    }

    #[test]
    fn load_guard_group_a_missing_age() {
        let fx = fixture();
        let me = make(&fx, NPC_DODO, true, None);
        let before_fullness = st(&me).fullness;
        // dict without fullness: start values kept
        let mut d = new_mutable_dict();
        d.map.insert("age".to_string(), PlistValue::real_f32(5.0));
        npc_load_values_from_save_dict(&me, &d);
        assert_eq!(st(&me).fullness, before_fullness);
        // with fullness but no age: age = 0
        let mut d = new_mutable_dict();
        d.map.insert("fullness".to_string(), PlistValue::real_f32(10.0));
        npc_load_values_from_save_dict(&me, &d);
        assert_eq!(st(&me).age, 0.0);
        assert_eq!(st(&me).fullness, 10.0);
    }

    #[test]
    fn update_timers_and_aging() {
        let fx = fixture();
        let me = make(&fx, NPC_DODO, false, None);
        {
            let mut s = st(&me);
            s.fullness = 1000.0;
            s.tame_cooldown_timer = 0.1;
            s.temperature_check_timer = 5.0;
            s.lay_cooldown_timer = 0.1;
            s.mate_cooldown_timer = 0.1;
        }
        npc_update_accurate_dt_is_simulation(&me, 0.2, 0.2, false);
        let s = st(&me);
        assert_eq!(s.tame_cooldown_timer, 0.0);
        assert!(s.lay_cooldown_timer < 0.0 && s.mate_cooldown_timer < 0.0);
        assert_eq!(s.fullness, 1000.0 - 0.2);
        assert_eq!(s.age, 0.2);
    }

    #[test]
    fn tamed_age_caps_at_half_max_age() {
        let fx = fixture();
        let me = make(&fx, NPC_DODO, false, None);
        {
            let mut s = st(&me);
            s.tamed_client_id = Some("x".into());
            s.age = 4499.9;
            s.temperature_check_timer = 5.0;
        }
        // 4499.9 < half: still ages this tick; the next tick caps at exactly maxAge / 2
        npc_update_accurate_dt_is_simulation(&me, 0.2, 0.2, false);
        assert_eq!(st(&me).age, 4499.9f32 + 0.2f32);
        npc_update_accurate_dt_is_simulation(&me, 0.2, 0.2, false);
        assert_eq!(st(&me).age, 4500.0f32);
        npc_update_accurate_dt_is_simulation(&me, 0.2, 0.2, false);
        assert_eq!(st(&me).age, 4500.0f32);
    }

    #[test]
    fn old_age_death_and_no_rand_in_update() {
        let fx = fixture();
        let me = make(&fx, NPC_DODO, false, None);
        {
            let mut s = st(&me);
            s.age = 9000.5;
            s.temperature_check_timer = 5.0;
            s.kill_blockhead = Some(TestBh::alloc(9, None));
        }
        let before = glibc_rand::rand();
        glibc_rand::srand(1);
        let _ = before;
        npc_update_accurate_dt_is_simulation(&me, 0.2, 0.2, false);
        let s = st(&me);
        assert!(s.dead);
        assert!(s.kill_blockhead.is_none(), "die(nil) clears the killer");
        // rand stream untouched by update
        drop(s);
        assert_eq!(glibc_rand::rand(), expected_first_rand());
    }
    fn expected_first_rand() -> i32 {
        glibc_rand::srand(1);
        let v = glibc_rand::rand();
        glibc_rand::srand(1);
        v
    }

    #[test]
    fn hunger_hit_uses_one_draw() {
        let fx = fixture();
        let me = make(&fx, NPC_DODO, false, None);
        {
            let mut s = st(&me);
            s.fullness = -1800.0;
            s.random_harm_from_hunger_timer = 0.1;
            s.temperature_check_timer = 5.0;
        }
        glibc_rand::srand(1);
        let r = expected_r01(1);
        glibc_rand::srand(1);
        npc_update_accurate_dt_is_simulation(&me, 0.2, 0.2, false);
        let s = st(&me);
        assert_eq!(s.damage, 2); // 16 / 8
        assert_eq!(s.random_harm_from_hunger_timer, r[0] * 10.0 + 20.0);
        assert_eq!(s.send_was_hit_state, 1);
    }

    #[test]
    fn hit_survive_counts_and_lethal_dies() {
        let fx = fixture();
        let me = make(&fx, NPC_DODO, false, None);
        let b = TestBh::alloc(5, Some("p1"));
        npc_hit_with_force_blockhead(&me, 4, Some(&b));
        {
            let s = st(&me);
            assert_eq!(s.damage, 4);
            assert_eq!(s.tame_counts_by_client_id.as_ref().unwrap()["p1"], -1);
            assert_eq!(s.hit_force, 4);
            assert!(!s.dead);
        }
        assert!(me.creation_data_needs_to_be_sent());
        npc_hit_with_force_blockhead(&me, 20, Some(&b));
        let s = st(&me);
        assert!(s.dead);
        assert_eq!(s.tame_counts_by_client_id.as_ref().unwrap()["p1"], -1, "lethal hit does not touch counts");
        assert_eq!(s.kill_blockhead.as_ref().unwrap().unique_id(), 5);
    }

    #[test]
    fn damage_wraps_and_will_die() {
        let fx = fixture();
        let me = make(&fx, NPC_DODO, false, None);
        st(&me).damage = 65535;
        npc_hit_with_force_blockhead(&me, 1, None);
        // wrapped to 0 < 16: survived
        let s = st(&me);
        assert_eq!(s.damage, 0);
        assert!(!s.dead);
        drop(s);
        st(&me).damage = 10;
        assert!(npc_will_die_if_hit_by_force(&me, 6));
        assert!(!npc_will_die_if_hit_by_force(&me, 5));
    }

    #[test]
    fn feed_tames_after_requirement_single_feeder() {
        let fx = fixture();
        let me = make(&fx, NPC_DODO, true, None);
        let b = TestBh::alloc(5, Some("p1"));
        st(&me).fullness = 1000.0;
        assert!(npc_feed_by_blockhead(&me, &b));
        {
            let s = st(&me);
            assert_eq!(s.fullness, 3700.0);
            assert_eq!(s.tame_cooldown_timer, 675.0);
            assert!(s.tamed_client_id.is_none());
            assert_eq!(s.send_successful_feed_state, 1);
        }
        st(&me).tame_cooldown_timer = 0.0;
        let mark = glibc_rand::rand(); // advance
        let _ = mark;
        glibc_rand::srand(7);
        assert!(npc_feed_by_blockhead(&me, &b));
        let s = st(&me);
        assert_eq!(s.tamed_client_id.as_deref(), Some("p1"));
        assert_eq!(s.send_successful_tame_state, 1);
        assert!(s.name.is_some());
        assert!(s.fullness <= 8100.0);
    }

    #[test]
    fn tame_burns_exactly_898_draws() {
        let fx = fixture();
        let me = make(&fx, NPC_DODO, true, None);
        glibc_rand::srand(3);
        npc_successful_tame(&me);
        let after = glibc_rand::rand();
        glibc_rand::srand(3);
        for _ in 0..898 {
            glibc_rand::rand();
        }
        assert_eq!(after, glibc_rand::rand());
        // already named: 896
        glibc_rand::srand(3);
        npc_successful_tame(&me);
        let after = glibc_rand::rand();
        glibc_rand::srand(3);
        for _ in 0..896 {
            glibc_rand::rand();
        }
        assert_eq!(after, glibc_rand::rand());
    }

    #[test]
    fn tame_bug_other_key_blocks_forever() {
        let fx = fixture();
        let me = make(&fx, NPC_DODO, true, None);
        let a = TestBh::alloc(5, Some("A"));
        let bb = TestBh::alloc(6, Some("B"));
        // B hit it once: negative entry for B
        npc_hit_with_force_blockhead(&me, 1, Some(&bb));
        for _ in 0..10 {
            st(&me).tame_cooldown_timer = 0.0;
            st(&me).damage = 0;
            st(&me).fullness = 1000.0;
            assert!(npc_feed_by_blockhead(&me, &a));
        }
        let s = st(&me);
        assert!(s.tamed_client_id.is_none(), "documented bug: never tames");
        assert_eq!(s.tame_counts_by_client_id.as_ref().unwrap()["A"], 10);
    }

    #[test]
    fn feed_heals_half_max_health() {
        let fx = fixture();
        let me = make(&fx, NPC_DODO, true, None);
        let b = TestBh::alloc(5, None);
        st(&me).damage = 12;
        assert!(npc_feed_by_blockhead(&me, &b));
        assert_eq!(st(&me).damage, 4); // 12 - min(12, 8)
        assert!(st(&me).tame_counts_by_client_id.as_ref().unwrap().contains_key("server"));
    }

    #[test]
    fn generate_new_name_rare_and_common() {
        let fx = fixture();
        let me = make(&fx, NPC_DODO, true, None);
        glibc_rand::srand(5);
        let r = expected_r01_seed(5, 2);
        glibc_rand::srand(5);
        npc_generate_new_name(&me);
        let name = st(&me).name.clone().unwrap();
        assert!((r[1] as f64) <= 0.99, "this seed draws a common name; rare ones have their own test");
        let idx = (f32_to_i32(r[0] * 4.0) % 4) as usize;
        assert_eq!(name, ["A", "B", "C", "D"][idx]);
        assert_eq!(NPC_NAMES_RARE.len(), 44);
    }
    fn expected_r01_seed(seed: u32, n: usize) -> Vec<f32> {
        glibc_rand::srand(seed);
        (0..n).map(|_| glibc_rand::r01()).collect()
    }

    #[test]
    fn change_name_limits() {
        let fx = fixture();
        let me = make(&fx, NPC_DODO, true, None);
        npc_change_name(&me, "");
        assert!(st(&me).name.is_none());
        npc_change_name(&me, "12345678901234");
        assert_eq!(st(&me).name.as_deref(), Some("12345678901234"));
        npc_change_name(&me, "123456789012345");
        assert_eq!(st(&me).name.as_deref(), Some("12345678901234"));
    }

    #[test]
    fn creation_blob_layout_and_send_states() {
        let fx = fixture();
        let me = make(&fx, NPC_DODO, true, None);
        {
            let mut s = st(&me);
            s.fullness = 1234.9;
            s.age = -3.5;
            s.tamed_client_id = Some("me".into());
            s.breed = 0x1ff;
            s.tame_cooldown_timer = 70000.0;
            s.send_successful_feed_state = 1;
            s.feed_blockhead_unique_id_to_send = 11;
            s.set_free_blockhead_unique_id_to_send = 12;
        }
        let blob = me.creation_net_data_for_client(&me, Some("me")).unwrap();
        assert_eq!(blob.len(), 0x48);
        assert_eq!(&blob[0x38..0x3a], &(1234i16).to_le_bytes());
        assert_eq!(&blob[0x3a..0x3c], &(-3i16).to_le_bytes());
        assert_eq!(blob[0x35], 0xff);
        assert_eq!(blob[0x36], 1);
        assert_eq!(blob[0x37], 1);
        assert_eq!(&blob[0x3e..0x40], &(70000i32 as i16).to_le_bytes());
        assert_eq!(&blob[0x28..0x30], &11u64.to_le_bytes());
        assert_eq!(blob[0x40], 1);
        assert!(blob[0x41..0x48].iter().all(|&x| x == 0));
        // second blob: pending id cleared, set-free now emitted, feed state still reports (2 != 0), other recipient -> 2
        let blob2 = me.creation_net_data_for_client(&me, Some("other")).unwrap();
        assert_eq!(blob2[0x37], 2);
        assert_eq!(blob2[0x36], 1);
        assert_eq!(&blob2[0x28..0x30], &12u64.to_le_bytes());
        assert_eq!(blob2[0x40], 2);
        // update decays 2 -> 0
        npc_update_accurate_dt_is_simulation(&me, 0.2, 0.2, false);
        let blob3 = me.creation_net_data_for_client(&me, None).unwrap();
        assert_eq!(blob3[0x36], 0);
        assert_eq!(blob3[0x37], 2);
        assert_eq!(me.update_net_data_for_client(&me, None).unwrap().len(), 0x18);
    }

    #[test]
    fn name_tail_roundtrip_and_remote_update() {
        let fx = fixture();
        let me = make(&fx, NPC_DODO, true, None);
        st(&me).name = Some("ZED".into());
        let blob = me.creation_net_data_for_client(&me, None).unwrap();
        assert!(blob.len() > 0x48);
        assert!(std::str::from_utf8(&blob[0x48..]).unwrap().contains("<key>name</key>"));
        let me2 = make(&fx, NPC_DODO, true, None);
        npc_remote_creation_data_update(&me2, &blob);
        // no server on the mock world: name taken unfiltered
        assert_eq!(st(&me2).name.as_deref(), Some("ZED"));
    }

    #[test]
    fn remote_hit_and_set_free() {
        let fx = fixture();
        let me = make(&fx, NPC_DODO, true, None);
        let b = TestBh::alloc(21, Some("p"));
        fx.env.bhs.borrow_mut().push(b.clone());
        st(&me).tamed_client_id = Some("p".into());
        st(&me).name = Some("X".into());
        let mut d = NPCCreationNetData::default();
        d.hit_force = 2;
        d.kill_blockhead_id = 21;
        d.interact_blockhead_id = 21;
        d.interaction_type = NET_NPC_INTERACTION_SET_FREE;
        npc_remote_creation_data_update(&me, &d.to_vec());
        let s = st(&me);
        assert_eq!(s.damage, 2 + 4);
        assert!(s.tamed_client_id.is_none());
        assert!(s.name.is_none());
        assert_eq!(s.tame_counts_by_client_id.as_ref().unwrap()["p"], -2);
    }

    #[test]
    fn set_free_non_owner_does_nothing() {
        let fx = fixture();
        let me = make(&fx, NPC_DODO, true, None);
        st(&me).tamed_client_id = Some("owner".into());
        let b = TestBh::alloc(21, Some("p"));
        npc_set_free_by_blockhead(&me, &b);
        assert_eq!(st(&me).tamed_client_id.as_deref(), Some("owner"));
        assert_eq!(st(&me).damage, 0);
    }

    #[test]
    fn capture_makes_free_block_and_removes() {
        let fx = fixture();
        let me = make(&fx, NPC_DODO, true, None);
        let b = TestBh::alloc(21, Some("p"));
        assert!(npc_capture_by_blockhead_with_item_type(&me, &b, 5));
        assert!(me.needs_removed());
        assert_eq!(fx.env.made.borrow()[0].0, 77);
    }

    #[test]
    fn placement_owner_becomes_server_for_local_id() {
        let fx = fixture();
        let mut sd = new_mutable_dict();
        sd.map.insert("tamedClientID".to_string(), PlistValue::string("old"));
        sd.map.insert("fullness".to_string(), PlistValue::real_f32(5.0));
        let me = TestNpc::alloc(NPC_DODO);
        let wr: WorldRef = fx.w.clone();
        let dr: DynamicWorldRef = fx.dw.clone();
        assert!(npc_init_with_world_dynamic_world_at_position_cache_save_dict_is_adult_was_placed_placed_by_client(
            &me,
            &wr,
            &dr,
            IntPair { x: 40, y: 3 },
            None,
            Some(&sd),
            true,
            true,
            Some(SERVER_LOCAL_NET_ID)
        ));
        assert_eq!(st(&me).tamed_client_id.as_deref(), Some("server"));
        let me2 = TestNpc::alloc(NPC_DODO);
        assert!(npc_init_with_world_dynamic_world_at_position_cache_save_dict_is_adult_was_placed_placed_by_client(
            &me2,
            &wr,
            &dr,
            IntPair { x: 40, y: 3 },
            None,
            Some(&sd),
            true,
            true,
            Some("clientX")
        ));
        assert_eq!(st(&me2).tamed_client_id.as_deref(), Some("clientX"));
    }

    #[test]
    fn rider_add_remove_and_save_index() {
        let fx = fixture();
        let me = make(&fx, NPC_DONKEY, true, None);
        let b = TestBh::alloc(21, Some("p"));
        fx.dw.blockheads.borrow_mut().push(TestBh::alloc(1, None));
        fx.dw.blockheads.borrow_mut().push(b.clone());
        npc_add_rider(&me, &b);
        assert!(st(&me).being_controlled);
        let d = me.get_save_dict(&me);
        assert_eq!(d.map.get(&"currentBlockheadIndex".to_string()).unwrap().int_value(), 1);
        assert!(Rc::ptr_eq(&me, &me) && me.should_save_every_change_in_position());
        // reload path
        st(&me).rider = None;
        st(&me).being_controlled = false;
        st(&me).saved_blockhead_index = 1;
        npc_blockheads_loaded(&me);
        assert!(st(&me).being_controlled);
        assert!(ptr_eq(st(&me).rider.as_ref().unwrap(), &b));
        // remove(nil == nil) quirk: after clearing rider, removeRider(nil) still runs the body
        st(&me).rider = None;
        me.set_creation_data_needs_to_be_sent(false);
        npc_remove_rider_opt(&me, None);
        assert!(me.creation_data_needs_to_be_sent());
        npc_remove_rider(&me, &b);
        // non-matching removal returns early
        st(&me).rider = Some(b.clone());
        me.set_creation_data_needs_to_be_sent(false);
        npc_remove_rider(&me, &TestBh::alloc(2, None));
        assert!(!me.creation_data_needs_to_be_sent());
    }

    #[test]
    fn mate_and_can_mate() {
        let fx = fixture();
        let a = make(&fx, NPC_DODO, true, None);
        let b = make(&fx, NPC_DODO, true, None);
        st(&b).breed = 9;
        {
            let mut s = st(&a);
            s.mate_cooldown_timer = 0.0;
            s.age = 1000.0;
        }
        assert!(npc_can_mate(&a));
        glibc_rand::srand(2);
        let r = expected_r01_seed(2, 1);
        glibc_rand::srand(2);
        assert!(npc_mate_with_npc(&a, &b));
        let s = st(&a);
        assert_eq!(s.mate_cooldown_timer, (60.0f64 + (r[0] as f64) * 60.0) as f32);
        assert!(s.has_bred);
        assert_eq!(s.mate_breed, 9);
        drop(s);
        assert!(!npc_mate_with_npc(&a, &b));
    }

    #[test]
    fn ice_freezes_and_dead_do_nothing() {
        let fx = fixture();
        let me = make(&fx, NPC_DODO, true, None);
        {
            let mut s = st(&me);
            s.dead = true;
            s.fullness = 100.0;
            s.send_was_hit_state = 2;
        }
        npc_update_accurate_dt_is_simulation(&me, 0.2, 0.2, false);
        let s = st(&me);
        assert_eq!(s.fullness, 100.0);
        assert_eq!(s.send_was_hit_state, 0);
    }

    #[test]
    fn factory_rejects_unknown_types() {
        let fx = fixture();
        let wr: WorldRef = fx.w.clone();
        let dr: DynamicWorldRef = fx.dw.clone();
        assert!(new_npc_for_type(0, &wr, &dr, IntPair { x: 0, y: 0 }, None, None, true, false, None).is_none());
        assert!(new_npc_for_type(9, &wr, &dr, IntPair { x: 0, y: 0 }, None, None, true, false, None).is_none());
        fn ctor(
            w: &WorldRef,
            d: &DynamicWorldRef,
            p: IntPair,
            c: CacheRef,
            sd: Option<&PlistDict>,
            a: bool,
            pl: bool,
            cid: Option<&str>,
        ) -> Option<ObjRef> {
            let me = TestNpc::alloc(NPC_YAK);
            npc_init_with_world_dynamic_world_at_position_cache_save_dict_is_adult_was_placed_placed_by_client(
                &me, w, d, p, c, sd, a, pl, cid,
            )
            .then_some(me)
        }
        register_npc_ctor(NPC_YAK, ctor);
        let o = new_npc_for_type(NPC_YAK, &wr, &dr, IntPair { x: 40, y: 3 }, None, None, true, false, None).unwrap();
        assert_eq!(o.object_type(), 63);
    }

    #[test]
    fn object_type_mapping_and_protocols() {
        let fx = fixture();
        let me = make(&fx, NPC_SHARK, true, None);
        assert_eq!(me.object_type(), 36);
        assert!(me.as_harmable().is_some() && me.as_ridable().is_some() && me.as_tap_choice().is_some());
        assert!(me.class_info().conforms_to("RidableDynamicObject"));
        let n = as_npc(&me).unwrap();
        assert_eq!(n.species_name(), "CRITTER");
        st(&me).age = 100.0;
        assert_eq!(n.second_option_title(), None);
        st(&me).age = 1000.0;
        assert_eq!(n.second_option_title(), Some("RIDE"));
        assert_eq!(npc_cant_be_fed_tip_string_for_blockhead(&me), "THAT CRITTER ISN'T HUNGRY\nTRY AGAIN LATER");
        assert_eq!(npc_rider_pos_for_blockhead(&me, &me)[2], -0.75);
        assert_eq!(npc_rider_pos_for_blockhead(&me, &me)[3], 1.0);
    }

    #[test]
    fn new_net_replica_init_zero_pads_and_runs_short_update() {
        let fx = fixture();
        let me = TestNpc::alloc(NPC_DODO);
        let wr: WorldRef = fx.w.clone();
        let dr: DynamicWorldRef = fx.dw.clone();
        let mut d = NPCCreationNetData::default();
        d.dynamic_object_net_data = crate::types::netdata::DynamicObjectNetData::new(42, 40, 3, false);
        d.damage = 5;
        d.dead = 0;
        d.tame_type = 1;
        d.tame_cooldown_timer = -2;
        d.breed = 3;
        assert!(npc_init_with_world_dynamic_world_cache_net_data(&me, &wr, &dr, None, &d.to_vec()));
        assert!(me.is_net());
        {
            let s = st(&me);
            assert_eq!((s.damage, s.breed, s.net_tame_type, s.tame_cooldown_timer), (5, 3, 1, -2.0));
        }
        st(&me).fullness = 100.0;
        npc_update_accurate_dt_is_simulation(&me, 0.2, 0.2, false);
        let s = st(&me);
        assert_eq!(s.damage, 5, "no hunger / temperature damage on the isNet branch");
        assert_eq!(s.fullness, 100.0 - 0.2);
        assert_eq!(s.tame_cooldown_timer, 0.0);
    }

    fn net_bh(id: u64) -> ObjRef {
        let b = TestBh::alloc(id, Some("n"));
        b.base().borrow_mut().is_net = true;
        b
    }
    fn controlled_bh(id: u64) -> ObjRef {
        let b = TestBh::alloc(id, Some("c"));
        b.as_any().downcast_ref::<TestBh>().unwrap().ctl.set(true);
        b
    }
    fn tile_of(ty: u8) -> NpcTileCopy {
        NpcTileCopy { type_index: ty, ..Default::default() }
    }
    fn tick(me: &ObjRef, dt: f32) {
        npc_update_accurate_dt_is_simulation(me, dt, dt, false);
    }
    /// A replica of a Dodo, as the server holds one for a remote player's animal.
    fn replica(fx: &Fx) -> ObjRef {
        let me = TestNpc::alloc(NPC_DODO);
        let wr: WorldRef = fx.w.clone();
        let dr: DynamicWorldRef = fx.dw.clone();
        let d = NPCCreationNetData {
            dynamic_object_net_data: crate::types::netdata::DynamicObjectNetData::new(42, 40, 3, false),
            ..NPCCreationNetData::default()
        };
        assert!(npc_init_with_world_dynamic_world_cache_net_data(&me, &wr, &dr, None, &d.to_vec()));
        me
    }

    #[test]
    fn every_face_goes_through() {
        let fx = fixture();
        let me = make(&fx, NPC_DODO, true, None);
        faces::exercise_faces(&me, &TestBh::alloc(21, Some("p")));
    }

    /// The environment as production builds it: the world only, every other answer the default.
    #[test]
    fn default_env_bodies() {
        use crate::interaction::sign::testkit::world_with_block;
        struct Bare(Option<Rc<World>>);
        impl NpcEnvStub for Bare {
            fn world(&self) -> Option<Rc<World>> {
                self.0.clone()
            }
        }
        let (w, _dw, _pb) = world_with_block("npc_env_defaults");
        let o: ObjRef = crate::objects::base::testutil::TestObj::new();
        let pos = IntPair { x: 5, y: 5 };
        for e in [Rc::new(Bare(None)) as Rc<dyn NpcEnv>, Rc::new(Bare(Some(w)))] {
            let t = e.tile_probe(5, 5);
            let _ = (
                e.season_for_world_x(5, 1.0),
                e.current_temperature(&t.unwrap_or_default(), pos, 0.5, 0.5, 0.0),
                e.tile_contains_door(5, 5),
                e.tile_contains_door_like(5, 5),
                e.day_night_fraction(0.0, 0.0),
                e.weather_fraction(pos, 0.0),
                e.server_clients_count(),
                e.local_net_id(),
                e.is_client(),
                e.blockhead_with_id_including_net(1),
                e.all_blockheads_including_net(),
                e.free_blocks_at_pos(pos),
                e.free_block_item_type(&o),
                e.interaction_object_at_pos(pos),
                e.feeder_chest_remove_item_if_available(&o, 1),
                e.remove_curse_words_from_blockhead_name("x"),
                e.blockhead_paused(&o),
            );
            e.mark_circum_navigate_x(3);
            e.set_door_at_pos_to_open_direction(pos, true, 1);
            e.create_free_block_for_captured_npc(pos, 1, &new_mutable_dict(), &o);
            e.blockhead_stop_riding(&o);
            e.blockhead_set_riding_object(&o, None);
        }
    }

    #[test]
    fn load_guards_of_odd_save_dicts() {
        let fx = fixture();
        let mut sd = new_mutable_dict();
        sd.map.insert("tameCountsByClientID".to_string(), PlistValue::string("not a dict"));
        sd.map.insert("currentBlockheadIndex".to_string(), PlistValue::int(2));
        let me = make(&fx, NPC_DODO, true, Some(&sd));
        assert!(st(&me).tame_counts_by_client_id.is_none());
        assert_eq!(st(&me).saved_blockhead_index, 2);
    }

    #[test]
    fn update_clears_sent_states_and_drops_unsent_ones_without_clients() {
        let fx = fixture();
        let me = make(&fx, NPC_DODO, true, None);
        fx.env.tiles.borrow_mut().push(((40, 3), tile_of(2)));
        {
            let mut s = st(&me);
            s.send_successful_tame_state = SEND_STATE_SENT;
            s.send_successful_feed_state = SEND_STATE_SENT;
            s.send_was_hit_state = SEND_STATE_SENT;
        }
        tick(&me, 0.1);
        {
            let s = st(&me);
            assert_eq!((s.send_successful_tame_state, s.send_successful_feed_state, s.send_was_hit_state), (0, 0, 0));
        }
        {
            let mut s = st(&me);
            s.send_successful_tame_state = SEND_STATE_SEND;
            s.send_successful_feed_state = SEND_STATE_SEND;
            s.send_was_hit_state = SEND_STATE_SEND;
        }
        tick(&me, 0.1);
        assert_eq!(st(&me).send_was_hit_state, SEND_STATE_SEND, "a client is connected: kept");
        fx.env.server_clients.set(0);
        tick(&me, 0.1);
        let s = st(&me);
        assert_eq!((s.send_successful_tame_state, s.send_successful_feed_state, s.send_was_hit_state), (0, 0, 0));
    }

    #[test]
    fn heat_damage_from_lava_and_fire() {
        let fx = fixture();
        let me = make(&fx, NPC_DODO, true, None);
        fx.env.tiles.borrow_mut().push(((40, 3), tile_of(2)));
        fx.env.tiles.borrow_mut().push(((40, 2), tile_of(TILE_LAVA)));
        fx.env.temp.set(30.0);
        {
            let mut s = st(&me);
            s.temperature_check_timer = 0.0;
            s.damage_delay_timer = 0.0;
            s.fullness = 1000.0;
        }
        tick(&me, 0.2);
        assert_eq!(st(&me).current_temperature, 130.0);
        assert_eq!(st(&me).damage, 6);
        assert!(st(&me).damage_delay_timer >= 0.5);
        // a delay still running only counts down
        st(&me).temperature_check_timer = 0.0;
        st(&me).damage_delay_timer = 10.0;
        tick(&me, 0.2);
        assert_eq!(st(&me).damage, 6);
        // the animal's own tile burning; just over the limit still hurts by at least one point
        fx.env.tiles.borrow_mut().clear();
        fx.env.tiles.borrow_mut().push(((40, 3), NpcTileCopy { on_fire: 1, ..tile_of(2) }));
        fx.env.temp.set(0.5);
        {
            let mut s = st(&me);
            s.temperature_check_timer = 0.0;
            s.damage_delay_timer = 0.0;
        }
        tick(&me, 0.2);
        assert_eq!(st(&me).damage, 7);
        // a burning tile below counts too
        fx.env.tiles.borrow_mut().clear();
        fx.env.tiles.borrow_mut().push(((40, 3), tile_of(2)));
        fx.env.tiles.borrow_mut().push(((40, 2), NpcTileCopy { on_fire: 1, ..tile_of(2) }));
        st(&me).temperature_check_timer = 0.0;
        tick(&me, 0.2);
        assert_eq!(st(&me).current_temperature, 100.5);
    }

    #[test]
    fn hunger_floor_with_and_without_starvation() {
        let fx = fixture();
        let me = make(&fx, NPC_DODO, true, None);
        fx.env.tiles.borrow_mut().push(((40, 3), tile_of(2)));
        {
            let mut s = st(&me);
            s.fullness = -5000.0;
            s.random_harm_from_hunger_timer = 100.0;
            s.temperature_check_timer = 5.0;
        }
        tick(&me, 0.2);
        assert_eq!(st(&me).fullness, -1800.0);
        assert_eq!(st(&me).damage, 0, "the starvation timer is still running");
        NO_HUNGER.with(|c| c.set(true));
        st(&me).fullness = -5000.0;
        st(&me).random_harm_from_hunger_timer = 0.0;
        tick(&me, 0.2);
        let (f, d) = (rd(&me).fullness, rd(&me).damage);
        assert_eq!((f, d), (-1800.0, 0));
    }

    #[test]
    fn rider_bookkeeping_in_update() {
        let fx = fixture();
        let me = make(&fx, NPC_DODO, true, None);
        fx.env.tiles.borrow_mut().push(((40, 3), tile_of(2)));
        st(&me).temperature_check_timer = 50.0;
        let b = TestBh::alloc(21, Some("p"));
        {
            let mut s = st(&me);
            s.rider = Some(b.clone());
            s.being_controlled = true;
        }
        // the control event goes out every half second
        me.set_creation_data_needs_to_be_sent(false);
        tick(&me, 0.3);
        assert!(!me.creation_data_needs_to_be_sent());
        tick(&me, 0.3);
        assert!(me.creation_data_needs_to_be_sent());
        assert!((st(&me).remote_send_control_event_timer - 0.1).abs() < 1e-5);
        // a paused rider is left alone
        fx.env.paused.set(true);
        b.set_needs_removed(&b, true);
        tick(&me, 0.1);
        assert!(st(&me).rider.is_some());
        fx.env.paused.set(false);
        me.set_creation_data_needs_to_be_sent(false);
        tick(&me, 0.1);
        assert!(rd(&me).rider.is_none() && !rd(&me).being_controlled);
        assert!(me.creation_data_needs_to_be_sent());
        // a remote rider that goes away is dropped without telling anyone, and never triggers control events
        let n = net_bh(22);
        {
            let mut s = st(&me);
            s.rider = Some(n.clone());
            s.being_controlled = true;
        }
        me.set_creation_data_needs_to_be_sent(false);
        tick(&me, 0.6);
        assert!(!me.creation_data_needs_to_be_sent());
        n.set_needs_removed(&n, true);
        tick(&me, 0.1);
        assert!(st(&me).rider.is_none());
        assert!(!me.creation_data_needs_to_be_sent());
    }

    #[test]
    fn replica_update_floors_fullness_and_caps_tamed_age() {
        let fx = fixture();
        let me = replica(&fx);
        {
            let mut s = st(&me);
            s.fullness = -5000.0;
            s.age = 100.0;
        }
        tick(&me, 1.0);
        let (f, a) = (rd(&me).fullness, rd(&me).age);
        assert_eq!((f, a), (-1800.0, 101.0));
        {
            let mut s = st(&me);
            s.net_tame_type = 1;
            s.age = 8000.0;
        }
        tick(&me, 1.0);
        assert_eq!(st(&me).age, 4500.0);
        st(&me).age = 10.0;
        tick(&me, 1.0);
        assert_eq!(st(&me).age, 11.0);
    }

    #[test]
    fn creation_blob_reports_each_pending_interaction() {
        let fx = fixture();
        let me = make(&fx, NPC_DODO, true, None);
        let b = TestBh::alloc(21, Some("p"));
        let read = |me: &ObjRef| {
            let blob = me.creation_net_data_for_client(me, None).unwrap();
            <NPCCreationNetData as WireStruct>::read_le(&blob[..0x48])
        };
        {
            let mut s = st(&me);
            s.send_was_hit_state = SEND_STATE_SEND;
            s.hit_force = 3;
            s.kill_blockhead = Some(b.clone());
            s.capture_blockhead_unique_id_to_send = 5;
            s.interaction_item_type_to_send = 3;
            s.harvest_blockhead_unique_id_to_send = 6;
            s.harvest_type_to_send = NET_NPC_INTERACTION_HARVEST_MILK;
            s.send_successful_tame_state = SEND_STATE_SEND;
            s.rider = Some(b.clone());
        }
        let d = read(&me);
        assert_eq!((d.hit_force, d.kill_blockhead_id, d.rider_id), (3, 21, 21));
        assert_eq!(
            (d.interact_blockhead_id, d.interaction_type, d.interaction_item_type),
            (5, NET_NPC_INTERACTION_CAPTURE, 3)
        );
        assert_eq!(d.successful_feed_or_tame, 2);
        assert_eq!(st(&me).send_was_hit_state, SEND_STATE_SENT);
        assert_eq!(st(&me).interaction_item_type_to_send, 0);
        let d = read(&me);
        assert_eq!((d.interact_blockhead_id, d.interaction_type), (6, NET_NPC_INTERACTION_HARVEST_MILK));
        assert_eq!(st(&me).harvest_blockhead_unique_id_to_send, 0);
        // a harvest id without a harvest type is not reported
        st(&me).harvest_blockhead_unique_id_to_send = 7;
        let d = read(&me);
        assert_eq!((d.interact_blockhead_id, d.interaction_type), (0, 0));
    }

    #[test]
    fn remote_interactions_reach_the_animal_on_the_server() {
        let fx = fixture();
        let me = make(&fx, NPC_DODO, true, None);
        let b = TestBh::alloc(21, Some("p"));
        fx.env.bhs.borrow_mut().push(b.clone());
        let send = |kind: u8| {
            let d = NPCCreationNetData {
                interact_blockhead_id: 21,
                interaction_type: kind,
                interaction_item_type: 5,
                ..NPCCreationNetData::default()
            };
            npc_remote_creation_data_update(&me, &d.to_vec());
        };
        send(NET_NPC_INTERACTION_FEED);
        assert_eq!(st(&me).send_successful_feed_state, SEND_STATE_SEND);
        send(NET_NPC_INTERACTION_HARVEST_MILK);
        send(NET_NPC_INTERACTION_HARVEST_SHAVE);
        send(99);
        assert!(!me.needs_removed());
        send(NET_NPC_INTERACTION_CAPTURE);
        assert!(me.needs_removed());
        assert_eq!(fx.env.made.borrow().len(), 1);
        // an unknown blockhead id is ignored
        let d = NPCCreationNetData {
            interact_blockhead_id: 99,
            interaction_type: NET_NPC_INTERACTION_FEED,
            ..NPCCreationNetData::default()
        };
        npc_remote_creation_data_update(&me, &d.to_vec());
    }

    #[test]
    fn remote_rider_reconciliation_on_the_server() {
        let fx = fixture();
        let me = make(&fx, NPC_DODO, true, None);
        let local = TestBh::alloc(30, Some("p"));
        let send = |rider_id: u64| {
            let d = NPCCreationNetData { rider_id, ..NPCCreationNetData::default() };
            npc_remote_creation_data_update(&me, &d.to_vec());
        };
        // the local rider stays on a matching or empty id, leaves on a different one
        npc_add_rider(&me, &local);
        send(30);
        send(0);
        assert!(st(&me).rider.is_some());
        send(31);
        assert_eq!(*fx.env.stops.borrow(), vec![30]);
        assert!(st(&me).rider.is_none());
        // a remote rider that reports nobody leaves
        let n = net_bh(40);
        npc_add_rider(&me, &n);
        send(0);
        assert!(rd(&me).rider.is_none() && !rd(&me).being_controlled);
        assert_eq!(*fx.env.stops.borrow(), vec![30, 40]);
        // a remote rider that reports somebody else is replaced by that one
        npc_add_rider(&me, &n);
        fx.env.bhs.borrow_mut().push(TestBh::alloc(21, None));
        fx.env.bhs.borrow_mut().push(net_bh(41));
        send(41);
        assert_eq!(st(&me).rider.as_ref().unwrap().unique_id(), 41);
        assert!(st(&me).being_controlled);
        send(41);
    }

    #[test]
    fn remote_update_on_a_client() {
        let fx = fixture();
        fx.env.client.set(true);
        let me = make(&fx, NPC_DODO, true, None);
        let b = TestBh::alloc(21, Some("p"));
        let n = net_bh(41);
        fx.env.bhs.borrow_mut().push(b.clone());
        fx.env.bhs.borrow_mut().push(n.clone());
        let send = |d: &NPCCreationNetData| npc_remote_creation_data_update(&me, &d.to_vec());
        let mut d = NPCCreationNetData {
            hit_force: 2,
            damage: 5,
            fullness: 100,
            age: 200,
            tame_cooldown_timer: 7,
            tame_type: 1,
            ..NPCCreationNetData::default()
        };
        send(&d);
        {
            let s = st(&me);
            assert_eq!(
                (s.damage, s.fullness, s.age, s.tame_cooldown_timer, s.net_tame_type),
                (5, 100.0, 200.0, 7.0, 1)
            );
            assert!(!s.dead);
        }
        // a rider shows up, and a different one replaces it
        d.hit_force = 0;
        d.rider_id = 41;
        send(&d);
        assert_eq!(st(&me).rider.as_ref().unwrap().unique_id(), 41);
        d.rider_id = 0;
        send(&d);
        assert!(st(&me).rider.is_none());
        send(&d);
        // a local rider only leaves when somebody else is named
        npc_add_rider(&me, &b);
        send(&d);
        assert!(st(&me).rider.is_some());
        d.rider_id = 41;
        send(&d);
        assert_eq!(fx.env.stops.borrow().as_slice(), &[41, 21]);
        assert!(st(&me).rider.is_none());
        // tame / fed notices and death
        d.rider_id = 0;
        d.successful_feed_or_tame = 2;
        d.dead = 1;
        send(&d);
        assert!(rd(&me).dead && rd(&me).name.is_none(), "no name tail: a client clears the name the tame just made");
        d.successful_feed_or_tame = 1;
        send(&d);
        // a name tail without a name clears the name; so does no tail at all
        let mut blob = d.to_vec();
        blob.extend_from_slice(b"<?xml version=\"1.0\" encoding=\"UTF-8\"?><plist version=\"1.0\"><dict><key>x</key><integer>1</integer></dict></plist>");
        npc_remote_creation_data_update(&me, &blob);
        assert!(st(&me).name.is_none());
        st(&me).name = Some("K".into());
        npc_remote_creation_data_update(&me, &d.to_vec());
        assert!(st(&me).name.is_none());
        // an undecodable tail leaves the name alone on the server, clears it on a client
        st(&me).name = Some("K".into());
        let mut junk = d.to_vec();
        junk.extend_from_slice(b"not a plist at all");
        npc_remote_creation_data_update(&me, &junk);
        assert!(st(&me).name.is_none());
    }

    #[test]
    fn fed_checks_with_and_without_starvation() {
        let fx = fixture();
        let me = make(&fx, NPC_DODO, true, None);
        let b = TestBh::alloc(21, Some("p"));
        NO_HUNGER.with(|c| c.set(true));
        {
            let mut s = st(&me);
            s.damage = 2;
            s.tame_cooldown_timer = 5.0;
        }
        assert!(npc_can_be_fed_by_blockhead(&me, &b));
        {
            let mut s = st(&me);
            s.damage = 0;
        }
        assert!(!npc_can_be_fed_by_blockhead(&me, &b));
        st(&me).tame_cooldown_timer = 0.0;
        assert!(npc_can_be_fed_by_blockhead(&me, &b));
        st(&me).tamed_client_id = Some("p".into());
        assert!(!npc_can_be_fed_by_blockhead(&me, &b));
        NO_HUNGER.with(|c| c.set(false));
        st(&me).fullness = 6000.0;
        assert!(!npc_can_be_fed_by_blockhead(&me, &b));
        st(&me).damage = 3;
        assert!(npc_can_be_fed_by_blockhead(&me, &b));
        st(&me).fullness = 100.0;
        assert!(npc_can_be_fed_by_blockhead(&me, &b));
    }

    #[test]
    fn replica_requests_go_out_as_pending_ids() {
        let fx = fixture();
        let me = replica(&fx);
        let b = TestBh::alloc(21, Some("p"));
        assert!(npc_feed_by_blockhead(&me, &b));
        assert_eq!(st(&me).feed_blockhead_unique_id_to_send, 21);
        assert_eq!(st(&me).tame_cooldown_timer, 675.0);
        npc_set_free_by_blockhead(&me, &b);
        assert_eq!(st(&me).set_free_blockhead_unique_id_to_send, 21);
        assert!(npc_capture_by_blockhead_with_item_type(&me, &b, 6));
        let (c, i) = (rd(&me).capture_blockhead_unique_id_to_send, rd(&me).interaction_item_type_to_send);
        assert_eq!((c, i), (21, 6));
        // the replica has a rider that is local to this process
        me.set_update_needs_to_be_sent(false);
        npc_add_rider(&me, &b);
        npc_update_position(&me, IntPair { x: 41, y: 3 });
        assert!(me.update_needs_to_be_sent());
    }

    #[test]
    fn repeated_feeding_by_a_second_player_and_the_server() {
        let fx = fixture();
        let me = make(&fx, NPC_DODO, true, None);
        let req = crate::items::breeds::tame_count_requirement_for_npc_type(NPCType(NPC_DODO as u32));
        let a = TestBh::alloc(21, Some("a"));
        let nobody = TestBh::alloc(22, None);
        // the host (no client id) feeds up to ownership
        for _ in 0..req {
            st(&me).tame_cooldown_timer = 0.0;
            st(&me).fullness = 100.0;
            assert!(npc_feed_by_blockhead(&me, &nobody));
        }
        assert_eq!(st(&me).tamed_client_id.as_deref(), Some("server"));
        // the owner feeding on keeps ownership; a second player feeding up to the requirement does not take it
        for _ in 0..(req + 1) {
            st(&me).tame_cooldown_timer = 0.0;
            st(&me).fullness = 100.0;
            assert!(npc_feed_by_blockhead(&me, &a));
        }
        assert_eq!(st(&me).tamed_client_id.as_deref(), Some("server"));
        // the owner feeding on (its count passes the requirement again) stays the owner
        st(&me).tame_cooldown_timer = 0.0;
        st(&me).fullness = 100.0;
        assert!(npc_feed_by_blockhead(&me, &nobody));
        assert_eq!(st(&me).tamed_client_id.as_deref(), Some("server"));
        // a feed during the cooldown only refills
        st(&me).fullness = 100.0;
        assert!(npc_feed_by_blockhead(&me, &a));
        assert_eq!(st(&me).fullness, 2800.0);
    }

    #[test]
    fn belonging_and_locality_of_tamed_animals() {
        let fx = fixture();
        let me = make(&fx, NPC_DODO, true, None);
        let p = TestBh::alloc(21, Some("p"));
        let nobody = TestBh::alloc(22, None);
        assert!(!npc_belongs_to_player_with_blockhead(&me, &p) && !npc_belongs_to_local_player(&me));
        st(&me).tamed_client_id = Some("server".into());
        assert!(npc_belongs_to_player_with_blockhead(&me, &nobody) && !npc_belongs_to_player_with_blockhead(&me, &p));
        assert!(npc_belongs_to_local_player(&me));
        st(&me).tamed_client_id = Some(SERVER_LOCAL_NET_ID.into());
        assert!(npc_belongs_to_local_player(&me));
        st(&me).tamed_client_id = Some("someone else".into());
        assert!(!npc_belongs_to_local_player(&me));
        let r = replica(&fx);
        st(&r).net_tame_type = 1;
        assert!(npc_belongs_to_local_player(&r) && npc_belongs_to_player_with_blockhead(&r, &p));
        assert!(!npc_belongs_to_player_with_blockhead(&r, &net_bh(5)));
        st(&r).net_tame_type = 0;
        assert!(!npc_belongs_to_local_player(&r) && !npc_tamed(&r));
    }

    #[test]
    fn eating_from_the_ground_and_from_a_feeder_chest() {
        let fx = fixture();
        let me = make(&fx, NPC_DODO, true, None);
        let food: ObjRef = crate::objects::base::testutil::TestObj::new();
        let other: ObjRef = crate::objects::base::testutil::TestObj::new();
        // replicas and full bellies do not look
        npc_check_current_position_for_food(&replica(&fx));
        st(&me).fullness = 6000.0;
        fx.env.fbs.borrow_mut().push(food.clone());
        npc_check_current_position_for_food(&me);
        assert!(!food.needs_removed());
        // a block of the wrong kind is passed over, the right one is eaten
        st(&me).fullness = 100.0;
        fx.env.fb_item.set(3);
        npc_check_current_position_for_food(&me);
        assert!(!food.needs_removed() && st(&me).fullness == 100.0);
        fx.env.fb_item.set(9);
        fx.env.fbs.borrow_mut().push(other.clone());
        npc_check_current_position_for_food(&me);
        assert!(food.needs_removed() && !other.needs_removed());
        assert_eq!(st(&me).fullness, 2800.0);
        // the feeder chest: refused, then accepted
        fx.env.fbs.borrow_mut().clear();
        *fx.env.io.borrow_mut() = Some(other.clone());
        fx.env.feeder.set(Some(false));
        npc_check_current_position_for_food(&me);
        assert_eq!(st(&me).fullness, 2800.0);
        fx.env.feeder.set(Some(true));
        npc_check_current_position_for_food(&me);
        assert_eq!(st(&me).fullness, 5500.0);
        assert!(st(&me).has_been_fed_by_blockhead_or_chest);
        // now full enough that the chest is not asked
        fx.env.feeder.set(Some(true));
        npc_check_current_position_for_food(&me);
        assert_eq!(st(&me).fullness, 5500.0);
        // eating from the ground can fill the belly so far that the chest is skipped
        st(&me).fullness = 3000.0;
        fx.env.fbs.borrow_mut().push(food.clone());
        npc_check_current_position_for_food(&me);
        assert_eq!(st(&me).fullness, 5700.0);
    }

    #[test]
    fn riders_that_the_server_controls_and_reloaded_riders() {
        let fx = fixture();
        let me = make(&fx, NPC_DONKEY, true, None);
        // a blockhead the server is steering never becomes the rider
        let c = controlled_bh(30);
        npc_add_rider(&me, &c);
        assert!(rd(&me).rider.is_none() && !rd(&me).being_controlled);
        assert_eq!(*fx.env.stops.borrow(), vec![30]);
        // a replica in a client does not announce a remote rider
        fx.env.client.set(true);
        me.set_creation_data_needs_to_be_sent(false);
        npc_add_rider(&me, &net_bh(31));
        assert!(!me.creation_data_needs_to_be_sent());
        me.set_creation_data_needs_to_be_sent(false);
        let r = st(&me).rider.clone().unwrap();
        npc_remove_rider(&me, &r);
        assert!(!me.creation_data_needs_to_be_sent());
        fx.env.client.set(false);
        // reloading: no saved rider, an index past the end, a steered rider, a normal rider
        npc_blockheads_loaded(&me);
        st(&me).saved_blockhead_index = 5;
        npc_blockheads_loaded(&me);
        assert!(rd(&me).being_controlled && rd(&me).rider.is_none());
        assert_eq!(st(&me).saved_blockhead_index, -1);
        fx.dw.blockheads.borrow_mut().push(controlled_bh(32));
        fx.dw.blockheads.borrow_mut().push(TestBh::alloc(33, None));
        st(&me).saved_blockhead_index = 0;
        npc_blockheads_loaded(&me);
        assert!(rd(&me).rider.is_none() && !rd(&me).being_controlled);
        assert!(fx.env.stops.borrow().contains(&32));
        st(&me).saved_blockhead_index = 1;
        npc_blockheads_loaded(&me);
        assert_eq!(*fx.env.riding.borrow(), vec![33]);
        // blockheads that unload are forgotten wherever the animal remembered them
        let b = TestBh::alloc(40, None);
        {
            let mut s = st(&me);
            s.kill_blockhead = Some(b.clone());
            s.inspecting_blockhead = Some(Rc::downgrade(&b));
            s.coming_to_inspect_blockhead = Some(Rc::downgrade(&b));
            s.rider = Some(b.clone());
        }
        npc_blockhead_unloaded(&me, &TestBh::alloc(41, None));
        assert!(st(&me).rider.is_some());
        npc_blockhead_unloaded(&me, &b);
        let s = st(&me);
        assert!(s.kill_blockhead.is_none() && s.inspecting_blockhead.is_none());
        assert!(s.coming_to_inspect_blockhead.is_none() && s.rider.is_none());
    }

    #[test]
    fn a_riding_animal_opens_and_closes_doors_as_it_moves() {
        let fx = fixture();
        let me = make(&fx, NPC_DONKEY, true, None);
        let b = TestBh::alloc(21, Some("p"));
        st(&me).rider = Some(b.clone());
        fx.env.door_like.borrow_mut().push((40, 3));
        fx.env.door.borrow_mut().push((41, 4));
        me.update_position(&me, IntPair { x: 41, y: 3 });
        assert_eq!(
            *fx.env.door_calls.borrow(),
            vec![(IntPair { x: 40, y: 3 }, false, 0), (IntPair { x: 41, y: 4 }, true, 1)]
        );
        assert_eq!(*fx.env.circum.borrow(), vec![41]);
        fx.env.door_calls.borrow_mut().clear();
        fx.env.door_like.borrow_mut().clear();
        fx.env.door_like.borrow_mut().push((40, 3));
        me.update_position(&me, IntPair { x: 40, y: 3 });
        assert_eq!(
            *fx.env.door_calls.borrow(),
            vec![(IntPair { x: 41, y: 4 }, false, 0), (IntPair { x: 40, y: 3 }, true, -1)]
        );
        // no door anywhere: nothing is called; a move on the spot is not a move
        fx.env.door_calls.borrow_mut().clear();
        fx.env.door_like.borrow_mut().clear();
        fx.env.door.borrow_mut().clear();
        me.update_position(&me, IntPair { x: 42, y: 3 });
        me.update_position(&me, IntPair { x: 42, y: 3 });
        assert!(fx.env.door_calls.borrow().is_empty());
    }

    /// An NPC class that overrides nothing but the state accessor: every trait default answers.
    struct BareNpc {
        base: RefCell<DynamicObjectBase>,
        npc: RefCell<NpcState>,
        ridable: Cell<bool>,
    }
    impl DynamicObject for BareNpc {
        fn base(&self) -> &RefCell<DynamicObjectBase> {
            &self.base
        }
        fn class_info(&self) -> &'static ClassInfo {
            &TEST_NPC_CLASS
        }
        fn as_any(&self) -> &dyn Any {
            self
        }
        npc_forward!(
            object_type,
            update_accurate_dt_is_simulation,
            get_save_dict,
            creation_net_data_for_client,
            update_net_data_for_client,
            remote_update,
            remote_creation_data_update,
            blockhead_unloaded,
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
    }
    impl Npc for BareNpc {
        fn npc(&self) -> &RefCell<NpcState> {
            &self.npc
        }
        // the only override: lets one class run every branch of the defaults that read it
        fn ridable_when_tamed(&self) -> bool {
            self.ridable.get()
        }
    }
    impl_npc_protocols!(BareNpc);

    #[test]
    fn a_class_that_overrides_nothing_gets_the_base_answers() {
        let fx = fixture();
        let me: ObjRef = alloc_npc(|w| BareNpc {
            base: RefCell::new(DynamicObjectBase::alloc_zeroed_with(w)),
            npc: RefCell::new(NpcState::default()),
            ridable: Cell::new(true),
        });
        assert_eq!(me.class_info().name, "TestNpc");
        let wr: WorldRef = fx.w.clone();
        let dr: DynamicWorldRef = fx.dw.clone();
        assert!(npc_init_with_world_dynamic_world_at_position_cache_save_dict_is_adult_was_placed_placed_by_client(
            &me,
            &wr,
            &dr,
            IntPair { x: 40, y: 3 },
            None,
            None,
            true,
            false,
            None
        ));
        let n = as_npc(&me).unwrap();
        let b = TestBh::alloc(21, Some("p"));
        assert_eq!(
            (n.npc_type(), n.food_item_type(), n.captured_item_type(), n.capture_required_item_type()),
            (0, 0, 0, 0)
        );
        assert!(!n.can_be_captured_by_blockhead_with_item_type(&b, 0));
        assert!(!npc_capture_by_blockhead_with_item_type(&me, &b, 0));
        assert!(!me.needs_removed());
        // every branch of the ride option on this one class
        let bare = me.as_any().downcast_ref::<BareNpc>().unwrap();
        bare.npc.borrow_mut().age = 100.0;
        assert_eq!(bare.second_option_title(), None);
        bare.npc.borrow_mut().age = 1000.0;
        assert_eq!(bare.second_option_title(), Some("RIDE"));
        bare.ridable.set(false);
        assert_eq!(bare.second_option_title(), None);
        bare.ridable.set(true);
    }

    #[test]
    fn protocol_faces_and_free_functions_forward() {
        let fx = fixture();
        let me = make(&fx, NPC_DODO, true, None);
        let b = TestBh::alloc(21, Some("p"));
        me.update_accurate_dt_is_simulation(&me, 0.1, 0.1, false);
        me.as_harmable().unwrap().hit_with_force_blockhead(&me, 3, Some(&b));
        assert_eq!(st(&me).damage, 3);
        assert_eq!(npc_health_fraction(&me), 1.0 - 3.0 / 16.0);
        assert_eq!(npc_fullness_fraction(&me), (st(&me).fullness as f64 / 5400.0) as f32);
        NO_HUNGER.with(|c| c.set(true));
        assert_eq!(npc_fullness_fraction(&me), 1.0);
        assert_eq!(npc_camera_pos_for_blockhead(&me, &b), me.render_pos());
        assert_eq!(npc_second_option_title(&me), as_npc(&me).unwrap().second_option_title());
        assert!(npc_will_die_if_hit_by_force(&me, 100) && !npc_will_die_if_hit_by_force(&me, 1));
        assert!(!npc_can_mate(&me) || npc_can_mate(&me));
        assert!(!npc_tamed(&me));
        register_npc_ctor(99, |_, _, _, _, _, _, _, _| None);
    }

    #[test]
    fn a_dead_animal_dies_once_and_releases_its_rider() {
        let fx = fixture();
        let me = make(&fx, NPC_DODO, true, None);
        let b = TestBh::alloc(21, Some("p"));
        npc_add_rider(&me, &b);
        npc_die(&me, Some(&b));
        assert_eq!(*fx.env.stops.borrow(), vec![21]);
        me.set_creation_data_needs_to_be_sent(false);
        npc_die(&me, None);
        assert!(!me.creation_data_needs_to_be_sent());
        assert!(ptr_eq(st(&me).kill_blockhead.as_ref().unwrap(), &b), "the second death changes nothing");
    }

    #[test]
    fn a_hit_on_a_client_only_remembers_the_attacker() {
        let fx = fixture();
        fx.env.client.set(true);
        let me = make(&fx, NPC_DODO, true, None);
        let b = TestBh::alloc(21, Some("p"));
        npc_hit_with_force_blockhead(&me, 2, Some(&b));
        let s = st(&me);
        assert_eq!((s.damage, s.hit_force, s.send_was_hit_state), (2, 2, SEND_STATE_SEND));
        assert!(s.kill_blockhead.is_some() && s.tame_counts_by_client_id.is_none());
    }

    #[test]
    fn feeding_a_full_but_hurt_animal_caps_fullness() {
        let fx = fixture();
        let me = make(&fx, NPC_DODO, true, None);
        let b = TestBh::alloc(21, Some("p"));
        st(&me).fullness = 6000.0;
        assert!(!npc_feed_by_blockhead(&me, &b), "full and unhurt: refused");
        st(&me).damage = 4;
        assert!(npc_feed_by_blockhead(&me, &b));
        assert_eq!(st(&me).fullness, 8100.0);
        assert_eq!(st(&me).damage, 0);
    }

    #[test]
    fn rare_names_come_from_the_long_list() {
        let fx = fixture();
        let me = make(&fx, NPC_DODO, true, None);
        let seed = (1u32..100_000)
            .find(|&s| {
                glibc_rand::srand(s);
                let _ = glibc_rand::r01();
                glibc_rand::r01() as f64 > 0.99
            })
            .unwrap();
        glibc_rand::srand(seed);
        npc_generate_new_name(&me);
        let name = st(&me).name.clone().unwrap();
        assert!(NPC_NAMES_RARE.iter().any(|n| nsstring::uppercase_string(n) == name));
    }

    #[test]
    fn a_name_tail_that_is_not_a_dictionary_raises() {
        let fx = fixture();
        let me = make(&fx, NPC_DODO, true, None);
        let mut blob = NPCCreationNetData::default().to_vec();
        blob.extend_from_slice(
            b"<?xml version=\"1.0\" encoding=\"UTF-8\"?><plist version=\"1.0\"><array><integer>1</integer></array></plist>",
        );
        let r = crate::foundation::exception::catch_ns(|| npc_remote_creation_data_update(&me, &blob));
        assert!(r.is_err());
    }

    #[test]
    fn a_placed_tamed_animal_without_a_placing_client_keeps_its_owner() {
        let fx = fixture();
        let mut sd = new_mutable_dict();
        sd.map.insert("tamedClientID".to_string(), PlistValue::string("old"));
        sd.map.insert("fullness".to_string(), PlistValue::real_f32(5.0));
        let me = TestNpc::alloc(NPC_DODO);
        let wr: WorldRef = fx.w.clone();
        let dr: DynamicWorldRef = fx.dw.clone();
        assert!(npc_init_with_world_dynamic_world_at_position_cache_save_dict_is_adult_was_placed_placed_by_client(
            &me,
            &wr,
            &dr,
            IntPair { x: 40, y: 3 },
            None,
            Some(&sd),
            true,
            true,
            None
        ));
        assert_eq!(st(&me).tamed_client_id.as_deref(), Some("old"));
    }

    #[test]
    fn the_stand_in_blockhead_answers_its_api() {
        let b = TestBh::alloc(21, Some("p"));
        let api = b.as_blockhead().unwrap();
        assert_eq!(api.client_id().as_deref(), Some("p"));
        assert!(api.local_net_id().is_none() && !api.is_interacting_with_any_interaction_object());
        assert_eq!(b.class_info().name, DYNAMIC_OBJECT_CLASS.name);
        api.set_interaction_object(None);
        api.stop_interacting();
    }

    #[test]
    fn a_feeder_chest_that_is_not_there_feeds_nobody() {
        let fx = fixture();
        let me = make(&fx, NPC_DODO, true, None);
        st(&me).fullness = 100.0;
        npc_check_current_position_for_food(&me);
        assert_eq!(st(&me).fullness, 100.0);
    }
}

/// A walk over every selector a concrete NPC class forwards or overrides, for the species' own test modules:
/// whatever the class answers, the call has to go through.
#[cfg(test)]
pub(crate) mod faces {
    use super::*;
    use crate::types::structs::{IntPair, Vector2};

    /// Calls the `DynamicObject`, `Npc`, `Harmable`, `Ridable` and `TapChoice` faces of `me` with `b` (a plain
    /// object standing in for a blockhead). `me` is dealloc'ed at the end.
    pub(crate) fn exercise_faces(me: &ObjRef, b: &ObjRef) {
        let n = as_npc(me).expect("an NPC");
        // DynamicObject face
        let _ = (me.class_info().name, me.object_type(), me.render_pos(), me.requires_physical_block());
        let _ = (
            me.should_save_every_change_in_position(),
            me.client_id_for_saving_seperately_and_only_loading_while_player_online(),
        );
        let sd = me.get_save_dict(me);
        let c = me.creation_net_data_for_client(me, None).unwrap_or_default();
        let u = me.update_net_data_for_client(me, None).unwrap_or_default();
        me.remote_update(me, &u);
        me.remote_creation_data_update(me, &c);
        me.blockheads_loaded(me);
        me.blockhead_unloaded(me, b);
        let _ = me.can_be_removed_by_blockhead(me, b);
        me.update_position(me, IntPair { x: me.pos().x, y: me.pos().y });
        assert!(me.as_harmable().is_some() && me.as_ridable().is_some() && me.as_tap_choice().is_some());
        // Npc constants and hooks
        let _ = (
            n.rider_rides_with_arms_down(),
            n.npc_type(),
            n.max_age(),
            n.dies_of_old_age(),
            n.min_fullness(),
            n.dies_of_low_fullness(),
            n.max_health(),
            n.creation_data_struct_size(),
            n.min_ridable_age(),
            n.species_name(),
            n.food_item_type(),
            n.captured_item_type(),
            n.capture_required_item_type(),
            n.get_names_array().map(|a| a.len()),
            n.get_names_array_count(),
            n.breed_string(),
            n.suffers_damage_at_high_temperatures(),
            n.ridable_when_tamed(),
            n.remove_title(),
            n.remove_is_red(),
            n.second_choice_is_blue(),
            n.second_option_title(),
        );
        n.load_values_from_save_dict(me, &sd);
        n.set_adult_creation_start_values(me);
        n.set_baby_creation_start_values(me);
        let _ = (n.npc_creation_net_data_for_client(me, None), n.npc_update_net_data_for_client(me, None));
        let _ = n.append_npc_creation_data_to_data(me, vec![0u8; 4]);
        n.react_to_being_hit(me);
        n.react_to_being_fed(me);
        n.create_item_drops_for_death(me);
        n.generate_new_name(me);
        let _ = (
            n.can_be_fed_by_blockhead(me, b),
            n.cant_be_fed_tip_string_for_blockhead(b),
            n.can_be_milked_by_blockhead(b),
            n.cant_be_milked_tip_string_for_blockhead(b),
            n.can_be_shaved_by_blockhead(b),
            n.cant_be_shaved_tip_string_for_blockhead(b),
            n.can_be_captured_by_blockhead_with_item_type(b, 0),
            n.cant_be_captured_tip_string_for_blockhead_with_item_type(b, 0),
            n.tamed(),
            n.belongs_to_player_with_blockhead(b),
            n.belongs_to_local_player(),
            n.can_mate(),
            n.action_title(),
            n.health_fraction(),
            n.fullness_fraction(),
            n.name(),
            n.name_pos(),
            n.age(),
            n.breed(),
        );
        let _ = (n.milk_by_blockhead(me, b), n.shave_by_blockhead(me, b));
        n.check_current_position_for_food(me);
        n.begin_blockhead_inspection(b);
        n.blockhead_is_coming_to_inspect(b);
        n.inspection_stopped();
        n.set_age(n.age());
        n.set_breed(n.breed());
        n.draw_name();
        n.change_name(me, "Nameless");
        // protocol faces
        let h = me.as_harmable().unwrap();
        let _ = (
            h.tap_is_within_body_radius(Vector2 { x: 0.0, y: 0.0 }),
            h.will_die_if_hit_by_force(1),
            h.is_visible(),
            h.center(),
        );
        let r = me.as_ridable().unwrap();
        let _ = (
            r.rider_pos_for_blockhead(b),
            r.camera_pos_for_blockhead(b),
            r.rider_body_y_rotation_for_blockhead(b),
            r.ride_direction(),
            r.blockhead_can_ride_using_item(b, 0),
            r.jumps_on_swipe(),
            r.requires_fuel(),
            r.acts_as_interaction_object(),
            r.rider_d_pad_should_allow_up_down(),
            r.rider_d_pad_should_give_discrete_values(),
        );
        r.set_target_velocity(Vector2 { x: 0.0, y: 0.0 });
        r.swipe_up_gesture(me);
        let _ = (n.rider_body_z_rotation_for_blockhead(b), n.is_double_height(), n.can_be_removed_by_blockhead_npc(b));
        let t = me.as_tap_choice().unwrap();
        let _ = (t.action_title(), t.is_double_height());
        // riders last: they change state
        r.add_rider(me, b);
        r.remove_rider(me, b);
        n.remove_rider_opt(me, None);
        me.dealloc(me);
    }
}
