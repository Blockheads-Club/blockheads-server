//! `DynamicObject`: the abstract base of every world entity (ivars, virtual hooks, base implementations).
//!
//!
//! ## World-side interface (shim)
//! `World`, `DynamicWorld`, `BHServer`, `MacroTile` and `Blockhead` are owned by later layers (world_state,
//! dynamic_world_core, server, blockhead_core) and do not exist when this layer is built. This file therefore
//! declares the narrow interfaces object_model needs as traits ([`WorldApi`], [`DynamicWorldApi`], [`ServerApi`],
//! [`MacroTileApi`], [`BlockheadApi`]) with the exact method names asks for. The later modules
//! implement these traits for their concrete types.
use std::any::Any;
use std::cell::RefCell;
use std::rc::{Rc, Weak};

use super::class_info::ClassInfo;
use super::handle::{self, CacheRef, ObjRef, ObjWeak};
use super::interaction_object::{InteractionObjectLike, InteractionObjectState};
use super::protocols::{
    HarmableDynamicObject, PathUserDynamicObject, PowerUser, RidableDynamicObject, SignTextDynamicObject,
    TapChoiceDynamicObject,
};
use crate::fmath::cconv::f32_to_i32;
use crate::nslog;
use crate::plist::value::{ClassTag, Origin, PlistDict, PlistValue};
use crate::types::structs::{IntPair, Vector2, WireStruct};

/// `MAX_DYNAMIC_OBJECT_TYPES`: the "no type" sentinel returned by the base `objectType`.
pub const MAX_DYNAMIC_OBJECT_TYPES: i32 = 65;

// ---------------------------------------------------------------- world-side interface shim

/// `MacroTile` (struct, size 0x28) as seen by object code.
pub trait MacroTileApi {
    /// `macroTile->physicalBlock != NULL`
    fn has_physical_block(&self) -> bool;
    /// `macroTile->dynamicObjects` (the `NSMutableArray` may be nil, hence the `Option`).
    fn dynamic_objects(&self) -> &RefCell<Option<Vec<ObjRef>>>;
}
/// Shared handle to a macro tile (identity = pointer).
pub type MacroTileRef = Rc<dyn MacroTileApi>;

/// Pointer identity of a macro tile (ObjC/C `==` on `MacroTile*`).
pub fn tile_id(t: &MacroTileRef) -> usize {
    Rc::as_ptr(t) as *const () as usize
}

/// The `World` calls made by object_model.
pub trait WorldApi {
    /// `[world worldWidthMacro]`
    fn world_width_macro(&self) -> i32;
    /// `macroPosForWorldPos(pos, world)` followed by `macroTileAtMacroPostion(mp.x, mp.y, [world macroTiles], world)`.
    /// Returns the macro position and the tile (`None` = nil / out of range).
    fn macro_pos_and_tile_for_world_pos(&self, pos: IntPair) -> (IntPair, Option<MacroTileRef>);
    /// `[world isAdmin]` (true on the dedicated server,).
    fn is_admin(&self) -> bool;
    /// `[world server]`
    fn server(&self) -> Option<Rc<dyn ServerApi>>;
    /// `worldTime()` (0.0 when there is no world)
    fn world_time(&self) -> f64;
    /// `loadPhysicalBlockForMacroTile:atX:y:loadSurroundingBlocks:createIfNotCreated:`
    fn load_physical_block_for_macro_tile_at_x_y_load_surrounding_blocks_create_if_not_created(
        &self,
        tile: &MacroTileRef,
        mx: i32,
        my: i32,
        load_surrounding_blocks: bool,
        create_if_not_created: bool,
    ) -> bool;
    /// `loadDynamicObjectsIfNotAlreadyLoadedForMacroTile:includeSurfaceBlocks:`
    fn load_dynamic_objects_if_not_already_loaded_for_macro_tile_include_surface_blocks(
        &self,
        tile: &MacroTileRef,
        include_surface_blocks: bool,
    );
    /// `tileIsProtectedAtPos:againstBlockhead:`
    fn tile_is_protected_at_pos_against_blockhead(&self, pos: IntPair, b: &ObjRef) -> bool;
}
/// Strong handle to the `World`.
pub type WorldRef = Rc<dyn WorldApi>;

/// `BHServer` calls made by object_model.
pub trait ServerApi {
    /// `playerIsAdminWithID:`
    fn player_is_admin_with_id(&self, client_id: Option<&str>) -> bool;
}

/// The `DynamicWorld` calls made by object_model.
pub trait DynamicWorldApi {
    /// `getNextDynamicObjectID` (pre-increment;)
    fn get_next_dynamic_object_id(&self) -> u64;
    /// `dynamicWorldChangedAtPos:objectType:`
    fn dynamic_world_changed_at_pos_object_type(&self, pos: IntPair, object_type: i32);
    /// `isServer`
    fn is_server(&self) -> bool;
    /// the `blockheads` array in index order
    fn blockheads(&self) -> Vec<ObjRef>;
    /// `getOwnerNameForObjectOwnerID:`
    fn get_owner_name_for_object_owner_id(&self, owner_id: &str) -> Option<String>;
}
/// Strong handle to the `DynamicWorld`.
pub type DynamicWorldRef = Rc<dyn DynamicWorldApi>;

/// The `Blockhead` calls made by object_model (implemented by the blockhead module).
/// `needsRemoved`, `isNet` and `uniqueID` are the base ivars and are read through [`DynamicObject`].
pub trait BlockheadApi {
    /// `clientID`
    fn client_id(&self) -> Option<String>;
    /// `localNetID`
    fn local_net_id(&self) -> Option<String>;
    /// `isClientBlockheadBeingControlledByServer`
    fn is_client_blockhead_being_controlled_by_server(&self) -> bool;
    /// `isInteractingWithAnyInteractionObject`
    fn is_interacting_with_any_interaction_object(&self) -> bool;
    /// `setInteractionObject:`
    fn set_interaction_object(&self, o: Option<&ObjRef>);
    /// `stopInteracting`
    fn stop_interacting(&self);
}

// ---------------------------------------------------------------- UB parity (E4)

/// UB-PARITY (D, 7E): the original dereferences a NULL `MacroTile*` here and dies with
/// SIGSEGV. With feature `faithful-crashes` the process raises SIGSEGV; without it this returns and the caller
/// takes the guarded path.
#[inline]
pub fn ub_null_tile_deref() {
    #[cfg(feature = "faithful-crashes")]
    crate::foundation::sys::faithful_segv();
}

// ---------------------------------------------------------------- ivars

/// Ivars 0x08..0x4c of `DynamicObject` (no isa).
pub struct DynamicObjectBase {
    pub self_weak: ObjWeak,
    /// 0x08 assign
    pub world: Option<Weak<dyn WorldApi>>,
    /// 0x10 assign
    pub dynamic_world: Option<Weak<dyn DynamicWorldApi>>,
    /// 0x18: tile whose `dynamicObjects` contains self
    pub macro_tile_owner: Option<MacroTileRef>,
    /// 0x20
    pub pos: IntPair,
    /// 0x28 (f32, f32)
    pub float_pos: Vector2,
    /// 0x30 (client only, always None here)
    pub cache: CacheRef,
    /// 0x38 retained; set only by subclasses
    pub owner_id: Option<String>,
    /// 0x40
    pub unique_id: u64,
    /// 0x48
    pub needs_removed: bool,
    /// 0x49
    pub update_needs_to_be_sent: bool,
    /// 0x4a
    pub creation_data_needs_to_be_sent: bool,
    /// 0x4b
    pub unreliable_update_needs_to_be_sent: bool,
    /// 0x4c: created from a network message (E5: also true on the server for client-created objects)
    pub is_net: bool,
}

impl DynamicObjectBase {
    /// Zeroed ivars after `alloc`; `floatPos` is `(1.0, 1.0)` from `.cxx_construct`.
    pub fn alloc_zeroed_with(self_weak: ObjWeak) -> DynamicObjectBase {
        DynamicObjectBase {
            self_weak,
            world: None,
            dynamic_world: None,
            macro_tile_owner: None,
            pos: IntPair { x: 0, y: 0 },
            float_pos: Vector2 { x: 1.0, y: 1.0 },
            cache: None,
            owner_id: None,
            unique_id: 0,
            needs_removed: false,
            update_needs_to_be_sent: false,
            creation_data_needs_to_be_sent: false,
            unreliable_update_needs_to_be_sent: false,
            is_net: false,
        }
    }
}

/// `W = [world worldWidthMacro] * 32` (wrapping like C `int`).
pub fn world_width_tiles(world: &dyn WorldApi) -> i32 {
    world.world_width_macro().wrapping_mul(32)
}

/// Position wrap/clamp of the init and `updatePosition:` paths: `x > W -> x -= W` (strict), else `x < 0 -> x += W`;
/// `y >= 0x3ff -> 0x3ff`, else `y < 0 -> 0`.
pub fn wrap_clamp_pos(p: IntPair, w_tiles: i32) -> IntPair {
    let mut x = p.x;
    let mut y = p.y;
    if x > w_tiles {
        x = x.wrapping_sub(w_tiles);
    } else if x < 0 {
        x = x.wrapping_add(w_tiles);
    }
    if y >= 0x3ff {
        y = 0x3ff;
    } else if y < 0 {
        y = 0;
    }
    IntPair { x, y }
}

// ---------------------------------------------------------------- the trait

/// Every overridable `DynamicObject` selector, defaulted to the base behaviour.
/// A subclass override that `[super...]`-calls must call the `base_*` free function, never the default through `self`.
pub trait DynamicObject: Any {
    /// The `DynamicObject` ivars.
    fn base(&self) -> &RefCell<DynamicObjectBase>;
    /// Class descriptor (`isKindOfClass:` etc.).
    fn class_info(&self) -> &'static ClassInfo;
    /// `Any` access for downcasts.
    fn as_any(&self) -> &dyn Any;
    /// Strong handle to self (upgrades `base.self_weak`).
    fn me(&self) -> ObjRef {
        self.base().borrow().self_weak.upgrade().expect("object already deallocated")
    }

    // ---- constants / hooks (no `me`) ----
    /// `objectType`: `MAX_DYNAMIC_OBJECT_TYPES` in the base.
    fn object_type(&self) -> i32 {
        MAX_DYNAMIC_OBJECT_TYPES
    }
    /// `requiresPhysicalBlock`: NO.
    fn requires_physical_block(&self) -> bool {
        false
    }
    /// `shouldSaveEveryChangeInPosition`: YES.
    fn should_save_every_change_in_position(&self) -> bool {
        true
    }
    /// `shouldAddToMacroBlock`: YES.
    fn should_add_to_macro_block(&self) -> bool {
        true
    }
    /// `isDownlight`: NO.
    fn is_downlight(&self) -> bool {
        false
    }
    /// `isUplight`: NO.
    fn is_uplight(&self) -> bool {
        false
    }
    /// `getLightRGB`: (0,0,0).
    fn get_light_rgb(&self) -> [f32; 3] {
        [0.0; 3]
    }
    /// `lightPos`: `(floatPos.x, floatPos.y, -1.0f)`.
    fn light_pos(&self) -> [f32; 3] {
        let fp = self.base().borrow().float_pos;
        [fp.x, fp.y, -1.0]
    }
    /// `renderPos`: `floatPos`.
    fn render_pos(&self) -> Vector2 {
        self.base().borrow().float_pos
    }
    /// `occupiesForegroundContents`: NO.
    fn occupies_foreground_contents(&self) -> bool {
        false
    }
    /// `occupiesNormalContents`: NO.
    fn occupies_normal_contents(&self) -> bool {
        false
    }
    /// `occupiesBackgroundContents`: NO.
    fn occupies_background_contents(&self) -> bool {
        false
    }
    /// `mayOnlyBeRemovedByOwner`: `ownerID != nil`.
    fn may_only_be_removed_by_owner(&self) -> bool {
        self.base().borrow().owner_id.is_some()
    }
    /// `clientIDForSavingSeperatelyAndOnlyLoadingWhilePlayerOnline`: nil.
    fn client_id_for_saving_seperately_and_only_loading_while_player_online(&self) -> Option<String> {
        None
    }
    /// `freeblockCreationItemType`: ITEM_NONE.
    fn freeblock_creation_item_type(&self) -> i32 {
        0
    }
    /// `freeBlockCreationSaveDict`: nil.
    fn free_block_creation_save_dict(&self, _me: &ObjRef) -> Option<PlistDict> {
        None
    }
    /// `freeBlockCreationDataA`: 0.
    fn free_block_creation_data_a(&self) -> u16 {
        0
    }
    /// `freeBlockCreationDataB`: 0.
    fn free_block_creation_data_b(&self) -> u16 {
        0
    }

    // ---- with `me` ----
    /// `initDerivedStuff:loadPhysicalBlockIfNeeded:`.
    fn init_derived_stuff_load_physical_block_if_needed(
        &self,
        me: &ObjRef,
        was_save_load: bool,
        load_physical_block_if_needed: bool,
    ) -> bool {
        base_init_derived_stuff_load_physical_block_if_needed(me, was_save_load, load_physical_block_if_needed)
    }
    /// `removeFromMacroBlock`.
    fn remove_from_macro_block(&self, me: &ObjRef) {
        base_remove_from_macro_block(me)
    }
    /// `update:accurateDT:isSimulation:`: no-op in the base.
    fn update_accurate_dt_is_simulation(&self, _me: &ObjRef, _dt: f32, _accurate_dt: f32, _is_simulation: bool) {}
    /// `updatePosition:`.
    fn update_position(&self, me: &ObjRef, new_position: IntPair) {
        base_update_position(me, new_position)
    }
    /// `setFloatPosAndUpdatePosition:`.
    fn set_float_pos_and_update_position(&self, me: &ObjRef, fp: Vector2) {
        base_set_float_pos_and_update_position(me, fp)
    }
    /// `setNeedsRemoved:` (virtual: GlowBlock, OwnershipSign,... override).
    fn set_needs_removed(&self, _me: &ObjRef, v: bool) {
        self.base().borrow_mut().needs_removed = v;
    }
    /// `worldChanged:`: no-op.
    fn world_changed(&self, _me: &ObjRef, _positions: &mut Vec<IntPair>) {}
    /// `worldContentsChanged:` forwards to `worldChanged:`.
    fn world_contents_changed(&self, me: &ObjRef, positions: &mut Vec<IntPair>) {
        self.world_changed(me, positions)
    }
    /// `waterContentChanged:` forwards to `worldChanged:`.
    fn water_content_changed(&self, me: &ObjRef, positions: &mut Vec<IntPair>) {
        self.world_changed(me, positions)
    }
    /// `getSaveDict`.
    fn get_save_dict(&self, me: &ObjRef) -> PlistDict {
        base_get_save_dict(me)
    }
    /// `dynamicObjectNetData`.
    fn dynamic_object_net_data(&self, me: &ObjRef) -> crate::types::netdata::DynamicObjectNetData {
        base_dynamic_object_net_data(me)
    }
    /// `creationNetDataForClient:`: nil in the base.
    fn creation_net_data_for_client(&self, _me: &ObjRef, _client_id: Option<&str>) -> Option<Vec<u8>> {
        None
    }
    /// `updateNetDataForClient:`: nil in the base.
    fn update_net_data_for_client(&self, _me: &ObjRef, _client_id: Option<&str>) -> Option<Vec<u8>> {
        None
    }
    /// `remoteUpdate:`.
    fn remote_update(&self, me: &ObjRef, net_data: &[u8]) {
        base_remote_update(me, net_data)
    }
    /// `remoteCreationDataUpdate:`.
    fn remote_creation_data_update(&self, me: &ObjRef, net_data: &[u8]) {
        base_remote_creation_data_update(me, net_data)
    }
    /// `blockheadUnloaded:`: no-op.
    fn blockhead_unloaded(&self, _me: &ObjRef, _b: &ObjRef) {}
    /// `blockheadsLoaded`: no-op.
    fn blockheads_loaded(&self, _me: &ObjRef) {}
    /// `canBeRemovedByBlockhead:`.
    fn can_be_removed_by_blockhead(&self, me: &ObjRef, b: &ObjRef) -> bool {
        base_can_be_removed_by_blockhead(me, b)
    }
    /// `addArtificialLightContributionForPhysicalBlockLoadedAtXPos:yPos:`: no-op.
    fn add_artificial_light_contribution_for_physical_block_loaded_at_x_pos_y_pos(
        &self,
        _me: &ObjRef,
        _macro_x: i32,
        _macro_y: i32,
    ) {
    }
    /// Explicit `dealloc` (the base is a no-op).
    fn dealloc(&self, _me: &ObjRef) {}
    /// `childNeedsSaving:` (FreightCar and Chest override).
    fn child_needs_saving(&self, _me: &ObjRef, _child: &ObjRef) {}

    // ---- protocol accessors ----
    fn as_path_user(&self) -> Option<&dyn PathUserDynamicObject> {
        None
    }
    fn as_harmable(&self) -> Option<&dyn HarmableDynamicObject> {
        None
    }
    fn as_ridable(&self) -> Option<&dyn RidableDynamicObject> {
        None
    }
    fn as_tap_choice(&self) -> Option<&dyn TapChoiceDynamicObject> {
        None
    }
    fn as_sign_text(&self) -> Option<&dyn SignTextDynamicObject> {
        None
    }
    fn as_power_user(&self) -> Option<&dyn PowerUser> {
        None
    }
    /// `Some` for `InteractionObject` and every subclass.
    fn as_interaction_object(&self) -> Option<&RefCell<InteractionObjectState>> {
        None
    }
    /// `Some(self)` for `InteractionObject` and every subclass (generic access to the overridable selectors).
    fn as_interaction_like(&self) -> Option<&dyn InteractionObjectLike> {
        None
    }
    /// `Some` for `Blockhead` (: replaces `downcast::<Blockhead>`).
    fn as_blockhead(&self) -> Option<&dyn BlockheadApi> {
        None
    }
}

// ---------------------------------------------------------------- accessors on `dyn DynamicObject`

impl dyn DynamicObject {
    /// `[self world]` (unretained).
    pub fn world(&self) -> Option<WorldRef> {
        self.base().borrow().world.as_ref().and_then(|w| w.upgrade())
    }
    /// `[self dynamicWorld]` (unretained).
    pub fn dynamic_world(&self) -> Option<DynamicWorldRef> {
        self.base().borrow().dynamic_world.as_ref().and_then(|w| w.upgrade())
    }
    /// `pos`
    pub fn pos(&self) -> IntPair {
        self.base().borrow().pos
    }
    /// `floatPos`
    pub fn float_pos(&self) -> Vector2 {
        self.base().borrow().float_pos
    }
    /// `uniqueID`
    pub fn unique_id(&self) -> u64 {
        self.base().borrow().unique_id
    }
    /// `needsRemoved`
    pub fn needs_removed(&self) -> bool {
        self.base().borrow().needs_removed
    }
    /// `updateNeedsToBeSent`
    pub fn update_needs_to_be_sent(&self) -> bool {
        self.base().borrow().update_needs_to_be_sent
    }
    /// `creationDataNeedsToBeSent`
    pub fn creation_data_needs_to_be_sent(&self) -> bool {
        self.base().borrow().creation_data_needs_to_be_sent
    }
    /// `unreliableUpdateNeedsToBeSent`
    pub fn unreliable_update_needs_to_be_sent(&self) -> bool {
        self.base().borrow().unreliable_update_needs_to_be_sent
    }
    /// `isNet` (E5)
    pub fn is_net(&self) -> bool {
        self.base().borrow().is_net
    }
    /// `macroTileOwner`
    pub fn macro_tile_owner(&self) -> Option<MacroTileRef> {
        self.base().borrow().macro_tile_owner.clone()
    }
    /// `ownerID`
    pub fn owner_id(&self) -> Option<String> {
        self.base().borrow().owner_id.clone()
    }
    /// `setUpdateNeedsToBeSent:`
    pub fn set_update_needs_to_be_sent(&self, v: bool) {
        self.base().borrow_mut().update_needs_to_be_sent = v;
    }
    /// `setCreationDataNeedsToBeSent:`
    pub fn set_creation_data_needs_to_be_sent(&self, v: bool) {
        self.base().borrow_mut().creation_data_needs_to_be_sent = v;
    }
    /// `setUnreliableUpdateNeedsToBeSent:`
    pub fn set_unreliable_update_needs_to_be_sent(&self, v: bool) {
        self.base().borrow_mut().unreliable_update_needs_to_be_sent = v;
    }
    /// Subclasses assign the `ownerID` ivar directly.
    pub fn set_owner_id(&self, v: Option<String>) {
        self.base().borrow_mut().owner_id = v;
    }
    /// `needsNetDataToBeSent`: `update || unreliable || needsRemoved` (creation flag NOT included).
    pub fn needs_net_data_to_be_sent(&self) -> bool {
        let b = self.base().borrow();
        b.update_needs_to_be_sent || b.unreliable_update_needs_to_be_sent || b.needs_removed
    }
}

// ---------------------------------------------------------------- helpers

fn dw_notify(dw: &Option<DynamicWorldRef>, pos: IntPair, object_type: i32) {
    if let Some(dw) = dw {
        dw.dynamic_world_changed_at_pos_object_type(pos, object_type);
    }
}

/// `[netData getBytes:&x length:n]` on a short NSData copies what exists and leaves the rest untouched
/// (never raises; E-F9). The untouched destination is modelled as zeros.
pub fn padded_bytes(data: &[u8], n: usize) -> Vec<u8> {
    let mut v = vec![0u8; n];
    let k = data.len().min(n);
    v[..k].copy_from_slice(&data[..k]);
    v
}

/// Empty `NSMutableDictionary`.
pub fn new_mutable_dict() -> PlistDict {
    PlistDict {
        map: crate::foundation::gsdict::GsDict::new(),
        tag: ClassTag { origin: Origin::Built, mutable: true, alt: false },
    }
}

/// `[NSNumber numberWithUnsignedLong:]`
pub fn number_with_unsigned_long(v: u64) -> PlistValue {
    if v > i64::MAX as u64 {
        PlistValue::UInt(v)
    } else {
        PlistValue::Int(v as i64)
    }
}

fn set_world_links(me: &ObjRef, world: &WorldRef, dw: &DynamicWorldRef, cache: CacheRef) {
    let mut b = me.base().borrow_mut();
    b.world = Some(Rc::downgrade(world));
    b.dynamic_world = Some(Rc::downgrade(dw));
    b.cache = cache;
}

// ---------------------------------------------------------------- super-chain free functions

/// `-[DynamicObject initWithWorld:dynamicWorld:atPosition:cache:]` (server-side spawn). Always returns true.
pub fn base_init_with_world_dynamic_world_at_position_cache(
    me: &ObjRef,
    world: &WorldRef,
    dw: &DynamicWorldRef,
    pos: IntPair,
    cache: CacheRef,
) -> bool {
    set_world_links(me, world, dw, cache);
    let uid = dw.get_next_dynamic_object_id();
    let w = world_width_tiles(world.as_ref());
    let p = wrap_clamp_pos(pos, w);
    {
        let mut b = me.base().borrow_mut();
        b.unique_id = uid;
        b.pos = p;
        b.float_pos = Vector2 { x: p.x as f32 + 0.5f32, y: p.y as f32 };
    }
    // result ignored; the object is returned even if registration failed
    let _ = me.init_derived_stuff_load_physical_block_if_needed(me, false, true);
    true
}

/// `-[DynamicObject initWithWorld:dynamicWorld:atPosition:cache:type:saveDict:placedByClient:]`: ignores
/// `itemType`/`saveDict`/`clientID`.
pub fn base_init_with_world_dynamic_world_at_position_cache_type_save_dict_placed_by_client(
    me: &ObjRef,
    world: &WorldRef,
    dw: &DynamicWorldRef,
    pos: IntPair,
    cache: CacheRef,
    _item_type: i32,
    _save_dict: Option<&PlistDict>,
    _client_id: Option<&str>,
) -> bool {
    base_init_with_world_dynamic_world_at_position_cache(me, world, dw, pos, cache)
}

/// `-[DynamicObject initWithWorld:dynamicWorld:saveDict:cache:]` (load from save).
/// A present `floatPos` array with fewer than two elements raises `NSRangeException` (`objectAtIndex:`).
pub fn base_init_with_world_dynamic_world_save_dict_cache(
    me: &ObjRef,
    world: &WorldRef,
    dw: &DynamicWorldRef,
    save_dict: &PlistDict,
    cache: CacheRef,
) -> bool {
    set_world_links(me, world, dw, cache);
    let get = |k: &str| save_dict.map.get(&k.to_string());
    let uid = get("uniqueID").map_or(0, |v| v.unsigned_long_value());
    let px = get("pos_x").map_or(0, |v| v.int_value());
    let py = get("pos_y").map_or(0, |v| v.int_value());
    let fp = match get("floatPos") {
        None => Vector2 { x: 0.0, y: 0.0 },
        Some(PlistValue::Array(a)) => {
            let el = |i: usize| -> f32 {
                match a.items.get(i) {
                    Some(v) => v.float_value(),
                    None => crate::foundation::exception::range_exception_index(
                        i as u64,
                        a.items.len() as u64,
                        "objectAtIndex:",
                    ),
                }
            };
            let x = el(0);
            let y = el(1);
            Vector2 { x, y }
        }
        Some(other) => crate::foundation::exception::unrecognized_selector(other.class_name(), "objectAtIndex:"),
    };
    {
        let mut b = me.base().borrow_mut();
        b.unique_id = uid;
        b.pos = IntPair { x: px, y: py };
        b.float_pos = fp;
    }
    let _ = me.init_derived_stuff_load_physical_block_if_needed(me, true, true);
    true
}

/// `-[DynamicObject initWithWorld:dynamicWorld:cache:netData:]`. Returns `false` when the caller must yield `nil`
/// (the failure path already ran `removeFromMacroBlock`).
pub fn base_init_with_world_dynamic_world_cache_net_data(
    me: &ObjRef,
    world: &WorldRef,
    dw: &DynamicWorldRef,
    cache: CacheRef,
    net_data: &[u8],
) -> bool {
    set_world_links(me, world, dw, cache);
    me.base().borrow_mut().is_net = true;
    let nd = <crate::types::netdata::DynamicObjectNetData as WireStruct>::read_le(&padded_bytes(net_data, 0x18));
    let mut uid = nd.unique_id;
    if uid == u64::MAX {
        uid = dw.get_next_dynamic_object_id();
    }
    {
        let mut b = me.base().borrow_mut();
        b.unique_id = uid;
        b.pos = IntPair { x: nd.pos_x as i32, y: nd.pos_y as i32 };
        b.float_pos = Vector2 { x: b.pos.x as f32 + 0.5f32, y: b.pos.y as f32 };
    }
    let ok = me.init_derived_stuff_load_physical_block_if_needed(me, false, false);
    if !ok {
        // tile nil + shouldAddToMacroBlock: UB-PARITY crash inside base_remove_from_macro_block (7D)
        me.remove_from_macro_block(me);
        // `[self release]`: the alloc reference is the only one (the tile array never retained self on this path),
        // so the dealloc chain runs here (: explicit dealloc where the last reference goes).
        me.dealloc(me);
        return false;
    }
    true
}

/// `-[DynamicObject initDerivedStuff:loadPhysicalBlockIfNeeded:]`.
pub fn base_init_derived_stuff_load_physical_block_if_needed(me: &ObjRef, was_save_load: bool, load: bool) -> bool {
    let (world, dw, pos) = (me.world(), me.dynamic_world(), me.pos());
    let Some(world) = world else { return false };
    let (mp, tile) = world.macro_pos_and_tile_for_world_pos(pos);
    me.base().borrow_mut().macro_tile_owner = tile.clone();
    let Some(tile) = tile else { return false };
    if !tile.has_physical_block() {
        if !load {
            return false;
        }
        let _ = world.load_physical_block_for_macro_tile_at_x_y_load_surrounding_blocks_create_if_not_created(
            &tile, mp.x, mp.y, false, true,
        );
    }
    if !me.should_add_to_macro_block() {
        return true;
    }
    world.load_dynamic_objects_if_not_already_loaded_for_macro_tile_include_surface_blocks(&tile, true);
    if let Some(v) = tile.dynamic_objects().borrow_mut().as_mut() {
        v.push(me.clone());
    }
    if !was_save_load {
        dw_notify(&dw, me.pos(), me.object_type());
    }
    true
}

/// `-[DynamicObject removeFromMacroBlock]`.
pub fn base_remove_from_macro_block(me: &ObjRef) {
    if !me.should_add_to_macro_block() {
        return;
    }
    let Some(tile) = me.macro_tile_owner() else {
        // UB-PARITY: original dereferences NULL->dynamicObjects (offset 0x18).
        ub_null_tile_deref();
        return;
    };
    if let Some(v) = tile.dynamic_objects().borrow_mut().as_mut() {
        v.retain(|o| !handle::ptr_eq(o, me));
    }
    dw_notify(&me.dynamic_world(), me.pos(), me.object_type());
}

/// `-[DynamicObject updatePosition:]`.
pub fn base_update_position(me: &ObjRef, new_position: IntPair) {
    let Some(world) = me.world() else { return };
    let dw = me.dynamic_world();
    let w = world_width_tiles(world.as_ref());
    let np = wrap_clamp_pos(new_position, w);
    let old_pos = {
        let mut b = me.base().borrow_mut();
        let o = b.pos;
        b.pos = np;
        o
    };
    if !me.should_add_to_macro_block() {
        return;
    }
    let (mp, new_tile) = world.macro_pos_and_tile_for_world_pos(np);
    let Some(new_tile) = new_tile else {
        me.set_needs_removed(me, true);
        return;
    };
    let owner = me.macro_tile_owner();
    if let Some(o) = &owner {
        if tile_id(o) == tile_id(&new_tile) {
            // case b: same tile
            if me.should_save_every_change_in_position() {
                if np == old_pos {
                    return;
                }
                dw_notify(&dw, me.pos(), me.object_type());
            }
            return;
        }
    }
    // case c: crossed into another macro tile
    if !me.is_net() {
        me.set_update_needs_to_be_sent(true);
    }
    match &owner {
        None => ub_null_tile_deref(), // UB-PARITY (7E): original dereferences NULL here
        Some(o) => {
            if o.dynamic_objects().borrow().is_none() {
                nslog!("oops 2");
            }
            if let Some(v) = o.dynamic_objects().borrow_mut().as_mut() {
                v.retain(|x| !handle::ptr_eq(x, me));
            }
            dw_notify(&dw, old_pos, me.object_type());
        }
    }
    me.base().borrow_mut().macro_tile_owner = Some(new_tile.clone());
    let _ = world.load_physical_block_for_macro_tile_at_x_y_load_surrounding_blocks_create_if_not_created(
        &new_tile, mp.x, mp.y, false, true,
    );
    world.load_dynamic_objects_if_not_already_loaded_for_macro_tile_include_surface_blocks(&new_tile, true);
    if let Some(v) = new_tile.dynamic_objects().borrow_mut().as_mut() {
        v.push(me.clone());
    }
    dw_notify(&dw, me.pos(), me.object_type());
}

/// `-[DynamicObject setFloatPosAndUpdatePosition:]`: C float to int truncation (NaN / overflow -> `i32::MIN`).
pub fn base_set_float_pos_and_update_position(me: &ObjRef, fp: Vector2) {
    me.base().borrow_mut().float_pos = fp;
    me.update_position(me, IntPair { x: f32_to_i32(fp.x), y: f32_to_i32(fp.y) });
}

/// `-[DynamicObject getSaveDict]`: keys `floatPos`, `pos_x`, `pos_y`, `uniqueID`.
pub fn base_get_save_dict(me: &ObjRef) -> PlistDict {
    let (fp, pos, uid) = {
        let b = me.base().borrow();
        (b.float_pos, b.pos, b.unique_id)
    };
    let mut d = new_mutable_dict();
    d.map.insert(
        "floatPos".to_string(),
        PlistValue::array(vec![PlistValue::real_f32(fp.x), PlistValue::real_f32(fp.y)]),
    );
    d.map.insert("pos_x".to_string(), PlistValue::int(pos.x as i64));
    d.map.insert("pos_y".to_string(), PlistValue::int(pos.y as i64));
    d.map.insert("uniqueID".to_string(), number_with_unsigned_long(uid));
    d
}

/// `-[DynamicObject dynamicObjectNetData]`: 0x18 bytes, padding zero (A).
pub fn base_dynamic_object_net_data(me: &ObjRef) -> crate::types::netdata::DynamicObjectNetData {
    let (pos, uid, nr) = {
        let b = me.base().borrow();
        (b.pos, b.unique_id, b.needs_removed)
    };
    let pos_x = if pos.x < 0 {
        let w = me.world().map_or(0, |w| world_width_tiles(w.as_ref()));
        pos.x.wrapping_add(w) as u32
    } else {
        pos.x as u32
    };
    crate::types::netdata::DynamicObjectNetData::new(uid, pos_x, pos.y as u32, nr)
}

/// `-[DynamicObject remoteUpdate:]`.
pub fn base_remote_update(me: &ObjRef, _net_data: &[u8]) {
    let dw = me.dynamic_world();
    if dw.as_ref().is_some_and(|d| d.is_server()) {
        me.set_update_needs_to_be_sent(true);
    }
    dw_notify(&dw, me.pos(), me.object_type());
}

/// `-[DynamicObject remoteCreationDataUpdate:]`.
pub fn base_remote_creation_data_update(me: &ObjRef, _net_data: &[u8]) {
    let dw = me.dynamic_world();
    if dw.as_ref().is_some_and(|d| d.is_server()) {
        me.set_creation_data_needs_to_be_sent(true);
    }
    dw_notify(&dw, me.pos(), me.object_type());
}

/// `-[DynamicObject canBeRemovedByBlockhead:]`.
/// A `b` that is not a Blockhead behaves like nil (step 1 yields YES on the server).
pub fn base_can_be_removed_by_blockhead(me: &ObjRef, b: &ObjRef) -> bool {
    let world = me.world();
    let bh = b.as_blockhead();
    // nil world: `[nil isAdmin]` = NO, `[nil server]` = nil, `[nil tileIsProtectedAtPos:...]` = NO (nil messaging).
    let Some(world) = world else {
        if !me.may_only_be_removed_by_owner() {
            return true;
        }
        let local = bh.and_then(|x| x.local_net_id());
        return matches!((local, me.owner_id()), (Some(l), Some(o)) if l == o);
    };
    if world.is_admin() {
        let controlled = bh.is_some_and(|x| x.is_client_blockhead_being_controlled_by_server());
        let net = bh.is_some() && b.is_net();
        if !controlled && !net {
            return true;
        }
    }
    let client_id = bh.and_then(|x| x.client_id());
    if let Some(server) = world.server() {
        if server.player_is_admin_with_id(client_id.as_deref()) {
            return true;
        }
    }
    if !me.may_only_be_removed_by_owner() {
        return !world.tile_is_protected_at_pos_against_blockhead(me.pos(), b);
    }
    let local = bh.and_then(|x| x.local_net_id());
    match (local, me.owner_id()) {
        (Some(l), Some(o)) => l == o,
        _ => false,
    }
}

// ---------------------------------------------------------------- test support + tests

#[cfg(test)]
pub(crate) mod testutil {
    //! Mock World / DynamicWorld / MacroTile and a minimal concrete object for the unit tests.
    use super::*;
    use crate::objects::class_info::DYNAMIC_OBJECT_CLASS;
    use std::cell::Cell;

    pub struct MockTile {
        pub physical: Cell<bool>,
        pub objs: RefCell<Option<Vec<ObjRef>>>,
        #[allow(dead_code)] // test fixture field
        pub id: usize,
    }
    impl MacroTileApi for MockTile {
        fn has_physical_block(&self) -> bool {
            self.physical.get()
        }
        fn dynamic_objects(&self) -> &RefCell<Option<Vec<ObjRef>>> {
            &self.objs
        }
    }

    pub struct MockWorld {
        pub width_macro: i32,
        /// tiles by macro x only (y must be 0..32 rows => macro y 0)
        pub tiles: Vec<Rc<MockTile>>,
        pub log: Rc<RefCell<Vec<String>>>,
        pub create_physical: bool,
        pub protected: Cell<bool>,
        pub admin: bool,
        pub server: RefCell<Option<Rc<dyn ServerApi>>>,
    }
    impl MockWorld {
        pub fn new(width_macro: i32, ntiles: usize, log: Rc<RefCell<Vec<String>>>) -> MockWorld {
            let tiles = (0..ntiles)
                .map(|i| Rc::new(MockTile { physical: Cell::new(true), objs: RefCell::new(Some(vec![])), id: i }))
                .collect();
            MockWorld {
                width_macro,
                tiles,
                log,
                create_physical: true,
                protected: Cell::new(false),
                admin: true,
                server: RefCell::new(None),
            }
        }
    }
    impl WorldApi for MockWorld {
        fn world_width_macro(&self) -> i32 {
            self.width_macro
        }
        fn macro_pos_and_tile_for_world_pos(&self, pos: IntPair) -> (IntPair, Option<MacroTileRef>) {
            let mp = IntPair { x: pos.x.div_euclid(32), y: pos.y.div_euclid(32) };
            let t = if mp.y == 0 && mp.x >= 0 && (mp.x as usize) < self.tiles.len() {
                Some(self.tiles[mp.x as usize].clone() as MacroTileRef)
            } else {
                None
            };
            (mp, t)
        }
        fn is_admin(&self) -> bool {
            self.admin
        }
        fn server(&self) -> Option<Rc<dyn ServerApi>> {
            self.server.borrow().clone()
        }
        fn world_time(&self) -> f64 {
            123.5
        }
        fn load_physical_block_for_macro_tile_at_x_y_load_surrounding_blocks_create_if_not_created(
            &self,
            tile: &MacroTileRef,
            mx: i32,
            my: i32,
            ls: bool,
            c: bool,
        ) -> bool {
            self.log.borrow_mut().push(format!("loadPhysical({mx},{my},{ls},{c})"));
            if self.create_physical {
                // mark physical block as created
                let p = Rc::as_ptr(tile) as *const () as usize;
                for t in &self.tiles {
                    if Rc::as_ptr(t) as *const () as usize == p {
                        t.physical.set(true);
                    }
                }
            }
            true
        }
        fn load_dynamic_objects_if_not_already_loaded_for_macro_tile_include_surface_blocks(
            &self,
            _tile: &MacroTileRef,
            inc: bool,
        ) {
            self.log.borrow_mut().push(format!("loadDyn({inc})"));
        }
        fn tile_is_protected_at_pos_against_blockhead(&self, _pos: IntPair, _b: &ObjRef) -> bool {
            self.protected.get()
        }
    }

    pub struct MockDw {
        pub next_id: Cell<u64>,
        pub log: Rc<RefCell<Vec<String>>>,
        pub server: bool,
        pub blockheads: RefCell<Vec<ObjRef>>,
    }
    impl DynamicWorldApi for MockDw {
        fn get_next_dynamic_object_id(&self) -> u64 {
            self.next_id.set(self.next_id.get() + 1);
            self.next_id.get()
        }
        fn dynamic_world_changed_at_pos_object_type(&self, pos: IntPair, t: i32) {
            self.log.borrow_mut().push(format!("changed({},{},{})", pos.x, pos.y, t));
        }
        fn is_server(&self) -> bool {
            self.server
        }
        fn blockheads(&self) -> Vec<ObjRef> {
            self.blockheads.borrow().clone()
        }
        fn get_owner_name_for_object_owner_id(&self, id: &str) -> Option<String> {
            Some(format!("name-of-{id}"))
        }
    }

    pub fn setup(width_macro: i32, ntiles: usize) -> (Rc<MockWorld>, Rc<MockDw>, Rc<RefCell<Vec<String>>>) {
        let log = Rc::new(RefCell::new(Vec::new()));
        let w = Rc::new(MockWorld::new(width_macro, ntiles, log.clone()));
        let dw = Rc::new(MockDw {
            next_id: Cell::new(100),
            log: log.clone(),
            server: true,
            blockheads: RefCell::new(vec![]),
        });
        (w, dw, log)
    }

    /// Plain DynamicObject (type 99 so the log is recognisable).
    pub struct TestObj {
        pub base: RefCell<DynamicObjectBase>,
        pub add_to_block: Cell<bool>,
        pub save_every: Cell<bool>,
        pub deallocs: Cell<u32>,
    }
    impl TestObj {
        pub fn new() -> Rc<TestObj> {
            handle::alloc_obj(|w| TestObj {
                base: RefCell::new(DynamicObjectBase::alloc_zeroed_with(w)),
                add_to_block: Cell::new(true),
                save_every: Cell::new(true),
                deallocs: Cell::new(0),
            })
        }
    }
    impl DynamicObject for TestObj {
        fn base(&self) -> &RefCell<DynamicObjectBase> {
            &self.base
        }
        fn class_info(&self) -> &'static ClassInfo {
            &DYNAMIC_OBJECT_CLASS
        }
        fn as_any(&self) -> &dyn Any {
            self
        }
        fn object_type(&self) -> i32 {
            7
        }
        fn should_add_to_macro_block(&self) -> bool {
            self.add_to_block.get()
        }
        fn should_save_every_change_in_position(&self) -> bool {
            self.save_every.get()
        }
        fn dealloc(&self, _me: &ObjRef) {
            self.deallocs.set(self.deallocs.get() + 1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::testutil::*;
    use super::*;

    fn wr(w: &Rc<MockWorld>) -> WorldRef {
        w.clone()
    }
    fn dr(d: &Rc<MockDw>) -> DynamicWorldRef {
        d.clone()
    }

    #[test]
    fn wrap_clamp() {
        let w = 128;
        let f = |x, y| wrap_clamp_pos(IntPair { x, y }, w);
        assert_eq!(f(128, 0).x, 128);
        assert_eq!(f(129, 0).x, 1);
        assert_eq!(f(-1, 0).x, 127);
        assert_eq!(f(0, 1023).y, 1023);
        assert_eq!(f(0, 1022).y, 1022);
        assert_eq!(f(0, 5000).y, 1023);
        assert_eq!(f(0, -4).y, 0);
    }

    #[test]
    fn init_at_position() {
        let (w, dw, log) = setup(4, 4);
        let o = TestObj::new();
        let me: ObjRef = o.clone();
        assert!(base_init_with_world_dynamic_world_at_position_cache(
            &me,
            &wr(&w),
            &dr(&dw),
            IntPair { x: 129, y: 5000 },
            None
        ));
        assert_eq!(me.pos(), IntPair { x: 1, y: 1023 });
        assert_eq!(me.float_pos().x, 1.5);
        assert_eq!(me.float_pos().y, 1023.0);
        assert_eq!(me.unique_id(), 101);
        assert!(!me.is_net());
        assert_eq!(me.pos().y, 1023);
        // y=1023 => macro y 31 => no tile in the mock => not registered
        assert!(me.macro_tile_owner().is_none());
        let (_w2, _d2, log2) = (&w, &dw, &log);
        assert!(log2.borrow().is_empty());
        let o2 = TestObj::new();
        let me2: ObjRef = o2.clone();
        assert!(base_init_with_world_dynamic_world_at_position_cache(
            &me2,
            &wr(&w),
            &dr(&dw),
            IntPair { x: 40, y: 3 },
            None
        ));
        assert_eq!(w.tiles[1].objs.borrow().as_ref().unwrap().len(), 1);
        assert_eq!(*log.borrow(), vec!["loadDyn(true)".to_string(), "changed(40,3,7)".to_string()]);
        assert_eq!(me2.float_pos().x, 40.5);
        assert!(me2.owner_id().is_none());
    }

    #[test]
    fn net_data_layout() {
        let (w, dw, _l) = setup(4, 4);
        let o = TestObj::new();
        let me: ObjRef = o.clone();
        base_init_with_world_dynamic_world_at_position_cache(&me, &wr(&w), &dr(&dw), IntPair { x: 5, y: 7 }, None);
        me.base().borrow_mut().pos = IntPair { x: -3, y: 9 };
        let nd = base_dynamic_object_net_data(&me);
        assert_eq!(nd.pos_x, 125);
        assert_eq!(nd.pos_y, 9);
        let bytes = nd.to_vec();
        assert_eq!(bytes.len(), 0x18);
        assert_eq!(&bytes[0..8], &101u64.to_le_bytes());
        assert!(bytes[0x11..].iter().all(|b| *b == 0));
        me.base().borrow_mut().pos = IntPair { x: 0, y: 9 };
        assert_eq!(base_dynamic_object_net_data(&me).pos_x, 0);
    }

    #[test]
    fn needs_net_data_truth_table() {
        let o = TestObj::new();
        let me: ObjRef = o.clone();
        assert!(!me.needs_net_data_to_be_sent());
        me.set_creation_data_needs_to_be_sent(true);
        assert!(!me.needs_net_data_to_be_sent());
        me.set_update_needs_to_be_sent(true);
        assert!(me.needs_net_data_to_be_sent());
        me.set_update_needs_to_be_sent(false);
        me.set_unreliable_update_needs_to_be_sent(true);
        assert!(me.needs_net_data_to_be_sent());
        me.set_unreliable_update_needs_to_be_sent(false);
        me.set_needs_removed(&me, true);
        assert!(me.needs_net_data_to_be_sent());
    }

    #[test]
    fn update_position_cases() {
        let (w, dw, log) = setup(4, 4);
        let o = TestObj::new();
        let me: ObjRef = o.clone();
        base_init_with_world_dynamic_world_at_position_cache(&me, &wr(&w), &dr(&dw), IntPair { x: 5, y: 7 }, None);
        log.borrow_mut().clear();
        // case b: equal position -> nothing
        me.update_position(&me, IntPair { x: 5, y: 7 });
        assert!(log.borrow().is_empty());
        // case b: different position -> one notification
        me.update_position(&me, IntPair { x: 6, y: 7 });
        assert_eq!(*log.borrow(), vec!["changed(6,7,7)".to_string()]);
        assert!(!me.update_needs_to_be_sent());
        // case b with shouldSaveEveryChangeInPosition false: nothing
        log.borrow_mut().clear();
        o.save_every.set(false);
        me.update_position(&me, IntPair { x: 7, y: 7 });
        assert!(log.borrow().is_empty());
        o.save_every.set(true);
        // case c: cross tile
        w.tiles[1].physical.set(false);
        me.update_position(&me, IntPair { x: 33, y: 7 });
        assert_eq!(
            *log.borrow(),
            vec![
                "changed(7,7,7)".to_string(),
                "loadPhysical(1,0,false,true)".to_string(),
                "loadDyn(true)".to_string(),
                "changed(33,7,7)".to_string()
            ]
        );
        assert!(me.update_needs_to_be_sent());
        assert_eq!(w.tiles[0].objs.borrow().as_ref().unwrap().len(), 0);
        assert_eq!(w.tiles[1].objs.borrow().as_ref().unwrap().len(), 1);
        // net objects do not set update flag
        me.set_update_needs_to_be_sent(false);
        me.base().borrow_mut().is_net = true;
        me.update_position(&me, IntPair { x: 1, y: 7 });
        assert!(!me.update_needs_to_be_sent());
        // tile nil -> needs_removed
        me.update_position(&me, IntPair { x: 500, y: 7 });
        assert!(me.needs_removed());
        assert_eq!(me.pos().x, 500 - 128);
    }

    #[test]
    fn update_position_not_in_macro_block() {
        let (w, dw, log) = setup(4, 4);
        let o = TestObj::new();
        o.add_to_block.set(false);
        let me: ObjRef = o.clone();
        base_init_with_world_dynamic_world_at_position_cache(&me, &wr(&w), &dr(&dw), IntPair { x: 5, y: 7 }, None);
        log.borrow_mut().clear();
        me.update_position(&me, IntPair { x: 50, y: 7 });
        assert_eq!(me.pos().x, 50);
        assert!(log.borrow().is_empty());
    }

    #[test]
    fn set_float_pos() {
        let (w, dw, _log) = setup(4, 4);
        let o = TestObj::new();
        let me: ObjRef = o.clone();
        base_init_with_world_dynamic_world_at_position_cache(&me, &wr(&w), &dr(&dw), IntPair { x: 5, y: 7 }, None);
        me.set_float_pos_and_update_position(&me, Vector2 { x: -0.5, y: 3.9 });
        assert_eq!(me.pos(), IntPair { x: 0, y: 3 });
        assert_eq!(me.float_pos().x, -0.5);
        me.set_float_pos_and_update_position(&me, Vector2 { x: f32::NAN, y: 3.0 });
        // INT32_MIN + 128 (x < 0 wrap)
        assert_eq!(me.pos().x, i32::MIN.wrapping_add(128));
    }

    ///  (net data init). Failure with a nil tile crashes under faithful-crashes, so only the
    /// other paths are exercised here.
    #[test]
    fn net_data_init() {
        let (w, dw, log) = setup(4, 4);
        let o = TestObj::new();
        let me: ObjRef = o.clone();
        let mut nd = crate::types::netdata::DynamicObjectNetData::new(u64::MAX, 40, 3, true);
        nd.padding = [1; 7];
        assert!(base_init_with_world_dynamic_world_cache_net_data(&me, &wr(&w), &dr(&dw), None, &nd.to_vec()));
        assert!(me.is_net());
        assert_eq!(me.unique_id(), 101);
        assert_eq!(me.pos(), IntPair { x: 40, y: 3 });
        assert!(!me.needs_removed());
        assert_eq!(me.float_pos().x, 40.5);
        assert_eq!(*log.borrow(), vec!["loadDyn(true)".to_string(), "changed(40,3,7)".to_string()]);

        // tile exists but no physical block (load = NO): failure path sends the spurious notification
        w.tiles[2].physical.set(false);
        log.borrow_mut().clear();
        let o2 = TestObj::new();
        let me2: ObjRef = o2.clone();
        let nd2 = crate::types::netdata::DynamicObjectNetData::new(55, 70, 4, false);
        assert!(!base_init_with_world_dynamic_world_cache_net_data(&me2, &wr(&w), &dr(&dw), None, &nd2.to_vec()));
        assert_eq!(me2.unique_id(), 55);
        assert_eq!(*log.borrow(), vec!["changed(70,4,7)".to_string()]);
        assert_eq!(w.tiles[2].objs.borrow().as_ref().unwrap().len(), 0);
        // `[self release]` runs the dealloc chain on the failure path only
        assert_eq!(o2.deallocs.get(), 1);
        assert_eq!(o.deallocs.get(), 0);
    }

    #[cfg(not(feature = "faithful-crashes"))]
    #[test]
    fn net_data_init_nil_tile_guarded() {
        let (w, dw, log) = setup(4, 4);
        let o = TestObj::new();
        let me: ObjRef = o.clone();
        let nd = crate::types::netdata::DynamicObjectNetData::new(9, 4000, 4, false);
        assert!(!base_init_with_world_dynamic_world_cache_net_data(&me, &wr(&w), &dr(&dw), None, &nd.to_vec()));
        assert!(log.borrow().is_empty());
    }

    #[test]
    fn save_dict_round_trip() {
        let (w, dw, log) = setup(4, 4);
        let o = TestObj::new();
        let me: ObjRef = o.clone();
        base_init_with_world_dynamic_world_at_position_cache(&me, &wr(&w), &dr(&dw), IntPair { x: 40, y: 3 }, None);
        let d = base_get_save_dict(&me);
        assert_eq!(d.map.len(), 4);
        let xml = String::from_utf8(crate::plist::xml_write::write_xml(&PlistValue::Dict(d.clone()))).unwrap();
        assert!(xml.contains("<key>floatPos</key>"), "{xml}");
        assert!(xml.contains("40.5"));
        assert!(xml.contains("<key>uniqueID</key>"));
        log.borrow_mut().clear();
        let o2 = TestObj::new();
        let me2: ObjRef = o2.clone();
        assert!(base_init_with_world_dynamic_world_save_dict_cache(&me2, &wr(&w), &dr(&dw), &d, None));
        assert_eq!(me2.pos(), IntPair { x: 40, y: 3 });
        assert_eq!(me2.float_pos().x, 40.5);
        assert_eq!(me2.unique_id(), me.unique_id());
        // save-load registers without notifying
        assert_eq!(*log.borrow(), vec!["loadDyn(true)".to_string()]);
        // pos is not wrapped on load
        let mut d3 = new_mutable_dict();
        d3.map.insert("pos_x".into(), PlistValue::int(-5));
        d3.map.insert("pos_y".into(), PlistValue::int(3));
        let o3 = TestObj::new();
        let me3: ObjRef = o3.clone();
        base_init_with_world_dynamic_world_save_dict_cache(&me3, &wr(&w), &dr(&dw), &d3, None);
        assert_eq!(me3.pos().x, -5);
        assert_eq!(me3.float_pos().x, 0.0);
        assert_eq!(me3.unique_id(), 0);
    }

    #[test]
    fn short_float_pos_raises() {
        let (w, dw, _log) = setup(4, 4);
        let mut d = new_mutable_dict();
        d.map.insert("floatPos".into(), PlistValue::array(vec![PlistValue::real_f32(1.0)]));
        let o = TestObj::new();
        let me: ObjRef = o.clone();
        let r = crate::foundation::exception::catch_ns(|| {
            base_init_with_world_dynamic_world_save_dict_cache(&me, &wr(&w), &dr(&dw), &d, None)
        });
        let e = r.expect_err("must raise");
        assert_eq!(e.name, "NSRangeException");
    }

    #[test]
    fn remote_updates() {
        let (w, dw, log) = setup(4, 4);
        let o = TestObj::new();
        let me: ObjRef = o.clone();
        base_init_with_world_dynamic_world_at_position_cache(&me, &wr(&w), &dr(&dw), IntPair { x: 40, y: 3 }, None);
        log.borrow_mut().clear();
        me.remote_update(&me, &[]);
        assert!(me.update_needs_to_be_sent());
        me.remote_creation_data_update(&me, &[]);
        assert!(me.creation_data_needs_to_be_sent());
        assert_eq!(log.borrow().len(), 2);
    }

    #[test]
    fn remove_from_macro_block_removes_all() {
        let (w, dw, log) = setup(4, 4);
        let o = TestObj::new();
        let me: ObjRef = o.clone();
        base_init_with_world_dynamic_world_at_position_cache(&me, &wr(&w), &dr(&dw), IntPair { x: 40, y: 3 }, None);
        w.tiles[1].objs.borrow_mut().as_mut().unwrap().push(me.clone());
        log.borrow_mut().clear();
        me.remove_from_macro_block(&me);
        assert_eq!(w.tiles[1].objs.borrow().as_ref().unwrap().len(), 0);
        assert_eq!(*log.borrow(), vec!["changed(40,3,7)".to_string()]);
        // macroTileOwner is not cleared
        assert!(me.macro_tile_owner().is_some());
    }

    #[test]
    fn can_be_removed_non_blockhead_on_admin_world() {
        let (w, dw, _log) = setup(4, 4);
        let o = TestObj::new();
        let me: ObjRef = o.clone();
        base_init_with_world_dynamic_world_at_position_cache(&me, &wr(&w), &dr(&dw), IntPair { x: 40, y: 3 }, None);
        let other: ObjRef = TestObj::new();
        assert!(me.can_be_removed_by_blockhead(&me, &other)); // behaves as b == nil on an admin world
    }

    #[test]
    fn defaults() {
        let o = TestObj::new();
        let me: ObjRef = o.clone();
        assert_eq!(me.light_pos(), [1.0, 1.0, -1.0]);
        assert_eq!(me.render_pos(), Vector2 { x: 1.0, y: 1.0 });
        assert!(!me.may_only_be_removed_by_owner());
        me.set_owner_id(Some("x".into()));
        assert!(me.may_only_be_removed_by_owner());
        assert_eq!(me.freeblock_creation_item_type(), 0);
        assert!(me.free_block_creation_save_dict(&me).is_none());
        assert!(handle::downcast::<TestObj>(&me).is_some());
        assert!(handle::is_kind_of_class(&me, &crate::objects::class_info::DYNAMIC_OBJECT_CLASS));
        assert!(!handle::is_member_of_class(&me, &crate::objects::class_info::INTERACTION_OBJECT_CLASS));
    }

    struct AdminServer(bool);
    impl ServerApi for AdminServer {
        fn player_is_admin_with_id(&self, _client_id: Option<&str>) -> bool {
            self.0
        }
    }

    /// The constants and nil answers of a plain object, the type-save-dict initialiser (which ignores its extras).
    #[test]
    fn more_defaults() {
        let (w, dw, _log) = setup(4, 4);
        let o = TestObj::new();
        let me: ObjRef = o.clone();
        assert!(base_init_with_world_dynamic_world_at_position_cache_type_save_dict_placed_by_client(
            &me,
            &wr(&w),
            &dr(&dw),
            IntPair { x: 40, y: 3 },
            None,
            99,
            None,
            Some("cid"),
        ));
        assert_eq!(me.pos(), IntPair { x: 40, y: 3 });
        assert!(!me.is_downlight() && !me.is_uplight());
        assert_eq!(me.get_light_rgb(), [0.0; 3]);
        assert_eq!((me.free_block_creation_data_a(), me.free_block_creation_data_b()), (0, 0));
        assert!(me.update_net_data_for_client(&me, None).is_none());
        me.child_needs_saving(&me, &me);
        assert!(me.as_path_user().is_none() && me.as_tap_choice().is_none() && me.as_power_user().is_none());
    }

    #[test]
    #[should_panic]
    fn float_pos_of_the_wrong_kind_raises() {
        let (w, dw, _log) = setup(4, 4);
        let mut d = new_mutable_dict();
        d.map.insert("floatPos".into(), PlistValue::string("nope"));
        let o = TestObj::new();
        let me: ObjRef = o.clone();
        base_init_with_world_dynamic_world_save_dict_cache(&me, &wr(&w), &dr(&dw), &d, None);
    }

    /// An object that is not kept in macro tiles is neither removed from one, nor moved between them.
    #[test]
    fn objects_outside_macro_tiles() {
        let (w, dw, log) = setup(4, 4);
        let o = TestObj::new();
        let me: ObjRef = o.clone();
        base_init_with_world_dynamic_world_at_position_cache(&me, &wr(&w), &dr(&dw), IntPair { x: 40, y: 3 }, None);
        o.add_to_block.set(false);
        log.borrow_mut().clear();
        me.remove_from_macro_block(&me);
        assert!(log.borrow().is_empty());
    }

    /// Crossing into another macro tile when the old tile has no object list, or no tile is recorded at all.
    #[test]
    fn crossing_with_a_damaged_owner() {
        let (w, dw, _log) = setup(4, 4);
        let o = TestObj::new();
        let me: ObjRef = o.clone();
        base_init_with_world_dynamic_world_at_position_cache(&me, &wr(&w), &dr(&dw), IntPair { x: 40, y: 3 }, None);
        // the old tile lost its list
        *w.tiles[1].objs.borrow_mut() = None;
        me.update_position(&me, IntPair { x: 70, y: 3 });
        assert_eq!(w.tiles[2].objs.borrow().as_ref().unwrap().len(), 1);
        // no recorded tile (the original would dereference NULL; the port carries on)
        me.base().borrow_mut().macro_tile_owner = None;
        me.update_position(&me, IntPair { x: 100, y: 3 });
        assert_eq!(w.tiles[3].objs.borrow().as_ref().unwrap().len(), 1);
    }

    /// Who may remove an object: a world that is gone, a server admin, a protected tile, owners.
    #[test]
    fn removal_rules() {
        // no world at all: only the owner rule decides, and a non-blockhead has no net id
        let o = TestObj::new();
        let me: ObjRef = o.clone();
        let other: ObjRef = TestObj::new();
        assert!(me.can_be_removed_by_blockhead(&me, &other));
        me.set_owner_id(Some("owner".into()));
        assert!(!me.can_be_removed_by_blockhead(&me, &other));

        // a world that is not an admin world: the server's admin list, then protection, then the owner rule
        let log = Rc::new(RefCell::new(Vec::new()));
        let mut mw = MockWorld::new(4, 4, log.clone());
        mw.admin = false;
        let w = Rc::new(mw);
        let dw =
            Rc::new(MockDw { next_id: std::cell::Cell::new(100), log, server: true, blockheads: RefCell::new(vec![]) });
        let o = TestObj::new();
        let me: ObjRef = o.clone();
        base_init_with_world_dynamic_world_at_position_cache(&me, &wr(&w), &dr(&dw), IntPair { x: 40, y: 3 }, None);
        // a server that knows admins
        *w.server.borrow_mut() = Some(Rc::new(AdminServer(true)));
        assert!(me.can_be_removed_by_blockhead(&me, &other));
        *w.server.borrow_mut() = Some(Rc::new(AdminServer(false)));
        // unowned: protected tiles refuse, others allow
        w.protected.set(true);
        assert!(!me.can_be_removed_by_blockhead(&me, &other));
        w.protected.set(false);
        assert!(me.can_be_removed_by_blockhead(&me, &other));
        // owned: only the owner (a non-blockhead is nobody)
        me.set_owner_id(Some("owner".into()));
        assert!(!me.can_be_removed_by_blockhead(&me, &other));
    }

    /// A world that cannot create the physical block leaves the object unregistered.
    #[test]
    fn world_without_physical_blocks() {
        let log = Rc::new(RefCell::new(Vec::new()));
        let mut mw = MockWorld::new(4, 4, log.clone());
        mw.create_physical = false;
        mw.tiles[1].physical.set(false);
        let w = Rc::new(mw);
        let dw =
            Rc::new(MockDw { next_id: std::cell::Cell::new(100), log, server: true, blockheads: RefCell::new(vec![]) });
        let o = TestObj::new();
        let me: ObjRef = o.clone();
        base_init_with_world_dynamic_world_at_position_cache(&me, &wr(&w), &dr(&dw), IntPair { x: 40, y: 3 }, None);
        assert!(!w.tiles[1].physical.get());
        assert!(!w.protected.get() || me.pos().x == 40);
        assert!(!w.tile_is_protected_at_pos_against_blockhead(me.pos(), &me));
    }

    /// A blockhead as the one asking: net or server-controlled ones lose the admin shortcut, the owner's own
    /// blockhead may remove an owned object.
    #[test]
    fn removal_by_blockheads() {
        use crate::blockhead::blockhead::testutil::{bh_of, new_bh, setup as bh_setup, TestHost};
        let _h = TestHost::install();
        let (bw, _bmock, bdw) = bh_setup();
        let bh = new_bh(&bw, &bdw, 0);
        let (w, dw, _log) = setup(4, 4);
        let o = TestObj::new();
        let me: ObjRef = o.clone();
        base_init_with_world_dynamic_world_at_position_cache(&me, &wr(&w), &dr(&dw), IntPair { x: 40, y: 3 }, None);
        // admin world: an ordinary blockhead may remove
        assert!(me.can_be_removed_by_blockhead(&me, &bh));
        // a net blockhead and a server-controlled one do not get the shortcut; the unowned object still goes
        bh.base().borrow_mut().is_net = true;
        assert!(me.can_be_removed_by_blockhead(&me, &bh));
        w.protected.set(true);
        assert!(!me.can_be_removed_by_blockhead(&me, &bh));
        bh.base().borrow_mut().is_net = false;
        bh_of(&bh).bh.borrow_mut().is_client_blockhead_being_controlled_by_server = true;
        assert!(!me.can_be_removed_by_blockhead(&me, &bh));
        // owned by the blockhead's own net id
        bh_of(&bh).set_client_id("abc");
        let local = bh_of(&bh).local_net_id().expect("local net id");
        assert_eq!(local, "abc");
        me.set_owner_id(Some(local));
        assert!(me.can_be_removed_by_blockhead(&me, &bh));
        // and a world that is gone decides the same way
        let loose = TestObj::new();
        let loose_me: ObjRef = loose.clone();
        loose_me.set_owner_id(me.owner_id());
        assert!(loose_me.can_be_removed_by_blockhead(&loose_me, &bh));
    }

    /// A bare `DynamicObject` keeps every default, including `dealloc`.
    #[test]
    fn plain_object_defaults() {
        use crate::objects::class_info::DYNAMIC_OBJECT_CLASS;
        struct Plain(RefCell<DynamicObjectBase>);
        impl DynamicObject for Plain {
            fn base(&self) -> &RefCell<DynamicObjectBase> {
                &self.0
            }
            fn class_info(&self) -> &'static ClassInfo {
                &DYNAMIC_OBJECT_CLASS
            }
            fn as_any(&self) -> &dyn Any {
                self
            }
        }
        let p: ObjRef = handle::alloc_obj(|w| Plain(RefCell::new(DynamicObjectBase::alloc_zeroed_with(w))));
        p.dealloc(&p);
        assert_eq!(p.pos(), p.base().borrow().pos);
        assert_eq!(p.object_type(), MAX_DYNAMIC_OBJECT_TYPES);
        // the remaining no-op defaults, and the stand-in's own accessors
        let mut positions = vec![IntPair { x: 1, y: 1 }];
        p.update_accurate_dt_is_simulation(&p, 0.1, 0.1, false);
        p.world_changed(&p, &mut positions);
        p.world_contents_changed(&p, &mut positions);
        p.water_content_changed(&p, &mut positions);
        assert!(p.creation_net_data_for_client(&p, None).is_none());
        assert!(p.update_net_data_for_client(&p, None).is_none());
        p.blockhead_unloaded(&p, &p);
        p.blockheads_loaded(&p);
        p.add_artificial_light_contribution_for_physical_block_loaded_at_x_pos_y_pos(&p, 0, 0);
        p.child_needs_saving(&p, &p);
        assert_eq!(p.class_info().name, "DynamicObject");
        assert!(p.as_any().is::<Plain>());
    }
}
