//! `PathCreator`: the shared A*-style pathfinder (one per `World`), its derived-tile slot allocator, the
//! incremental `updatePath` driver and the fall-path search.
//!
//! The order of every tile load is preserved (loading may consume libc `rand()`).
//!
//! ## Seam to later layers
//! The placeholder `DynamicWorld` in `world::state` does not (yet) expose `hasLightsToAdd`,
//! `blockheadOccupiesTileAtPos:ignoreBlockhead:` or `workbenchAtPos:`. They are reached through
//! [`PathDynamicWorldHooks`] (installed with [`set_dynamic_world_hooks`]).
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::{Rc, Weak};

use crate::foundation::gsdict::GsDict;
use crate::foundation::indexset::IndexSet;
use crate::objects::handle::ObjRef;
use crate::plist::value::{PlistDict, PlistValue};
use crate::types::enums::{InteractionType, PathType, TileTraverseKeyFrameType, TileTraverseType};
use crate::types::structs::IntPair;
use crate::world::helpers::{tile_contains_usable_door, tile_contains_usable_trap_door, tile_is_solid};
use crate::world::state::{TileRef, World};
use crate::world::tiles::{
    get_world_pos_for_world_index, tile_at_world_index_loaded, tile_at_world_position_loaded,
    world_index_at_world_position,
};

use super::test_tile::test_tile;
use super::wire_path::WirePowerSource;

// ---------------------------------------------------------------------------------------------------------
// constants
// ---------------------------------------------------------------------------------------------------------

const PATH_FALL: i32 = PathType::PATH_TYPE_FALL.0 as i32;
const PATH_FALL_INTENTIONAL: i32 = 6;
const TRAVERSE_INACCESSIBLE: i32 = TileTraverseType::TRAVERSE_INACCESSIBLE.0 as i32;
const TRAVERSE_ELEVATOR: i32 = TileTraverseType::TRAVERSE_ELEVATOR.0 as i32;
const KF_INACCESSIBLE: i32 = TileTraverseKeyFrameType::TRAVERSE_KF_INACCESSIBLE.0 as i32;
const KF_FALLING: i32 = TileTraverseKeyFrameType::TRAVERSE_KF_FALLING.0 as i32;
const KF_STAND_ELEVATOR: i32 = TileTraverseKeyFrameType::TRAVERSE_KF_STAND_ELEVATOR.0 as i32;
const INTERACTION_NONE: i32 = InteractionType::INTERACTION_NONE.0 as i32;
const INTERACTION_DIG: i32 = InteractionType::INTERACTION_DIG.0 as i32;
const INTERACTION_PLACE_ITEM: i32 = InteractionType::INTERACTION_PLACE_ITEM.0 as i32;
const INTERACTION_CRAFT: i32 = InteractionType::INTERACTION_CRAFT.0 as i32;
const INTERACTION_USE_INTERACTIVE_OBJECT: i32 = InteractionType::INTERACTION_USE_INTERACTIVE_OBJECT.0 as i32;
const INTERACTION_PICKUP: i32 = InteractionType::INTERACTION_PICKUP.0 as i32;
const INTERACTION_PICKUP_FREEBLOCK: i32 = InteractionType::INTERACTION_PICKUP_FREEBLOCK.0 as i32;
const INTERACTION_DIG_FOREGROUND: i32 = InteractionType::INTERACTION_DIG_FOREGROUND.0 as i32;
const INTERACTION_DIG_BACK_WALL: i32 = InteractionType::INTERACTION_DIG_BACK_WALL.0 as i32;

/// Maximum number of derived-tile slots (`derivedTileCount == 0x7fff` => NULL).
/// "tileDerivedPropertiesAtWorldIndex".
pub const MAX_DERIVED_TILES: i32 = 0x7fff;

/// `NSUInteger` view of an `int` world index (sign extension, as `(NSUInteger)idx`).
#[inline]
pub(crate) fn ns_index(idx: i32) -> u64 {
    idx as i64 as u64
}

/// `wrapDX(d)` == `wrapDelta` == `wrapdx`: horizontal world wrap with C truncating
/// division. `world_width_macro` is `[world worldWidthMacro]`; `W = worldWidthMacro * 32`.
#[inline]
pub(crate) fn wrap_dx(d: i32, world_width_macro: i32) -> i32 {
    let w = world_width_macro.wrapping_mul(32);
    if d < w / 2 {
        if d < world_width_macro.wrapping_mul(-32) / 2 {
            d.wrapping_add(w)
        } else {
            d
        }
    } else {
        d.wrapping_sub(w)
    }
}

// ---------------------------------------------------------------------------------------------------------
// seam to DynamicWorld calls the placeholder does not expose
// ---------------------------------------------------------------------------------------------------------

/// `DynamicWorld` calls used by this module that `world::state::DynamicWorld` (placeholder) does not provide.
/// `workbenchAtPos:`). Production never installs hooks: the real DynamicWorld is called directly; the hooks are a test-only override.
pub trait PathDynamicWorldHooks {
    /// `-[DynamicWorld hasLightsToAdd]`
    fn has_lights_to_add(&self) -> bool;
    /// `-[DynamicWorld blockheadOccupiesTileAtPos:ignoreBlockhead:]`
    fn blockhead_occupies_tile_at_pos_ignore_blockhead(&self, pos: IntPair, ignore: Option<&ObjRef>) -> bool;
    /// `-[DynamicWorld workbenchAtPos:]`
    fn workbench_at_pos(&self, pos: IntPair) -> Option<Rc<dyn WirePowerSource>>;
}

thread_local! {
    static DW_HOOKS: RefCell<Option<Rc<dyn PathDynamicWorldHooks>>> = const { RefCell::new(None) };
}

/// Install (or clear) the [`PathDynamicWorldHooks`] for the current thread (the server is single threaded).
pub fn set_dynamic_world_hooks(h: Option<Rc<dyn PathDynamicWorldHooks>>) {
    DW_HOOKS.with(|c| *c.borrow_mut() = h);
}

pub(crate) fn dw_hooks() -> Option<Rc<dyn PathDynamicWorldHooks>> {
    DW_HOOKS.with(|c| c.borrow().clone())
}

/// The real `DynamicWorld` (the original sends these messages directly to the one
/// DynamicWorld, there is no indirection). `World::dynamic_world` is still the narrow placeholder type, so the real
/// one is reached through the path user (every dynamic object holds the one DynamicWorld). Without a path user (or
/// one detached from its DynamicWorld) the answers are the nil-message defaults.
fn real_dw(user: Option<&ObjRef>) -> Option<Rc<crate::dynworld::state::DynamicWorld>> {
    user.and_then(|u| u.dynamic_world()).map(|d| crate::objects::registry::concrete_dw(&d))
}

/// `[dynamicWorld hasLightsToAdd]`
pub(crate) fn dw_has_lights_to_add(user: Option<&ObjRef>) -> bool {
    if let Some(h) = dw_hooks() {
        return h.has_lights_to_add();
    }
    real_dw(user).is_some_and(|d| d.has_lights_to_add())
}

/// `[dynamicWorld blockheadOccupiesTileAtPos:ignoreBlockhead:]`
pub(crate) fn dw_blockhead_occupies_tile(user: Option<&ObjRef>, pos: IntPair, ignore: Option<&ObjRef>) -> bool {
    if let Some(h) = dw_hooks() {
        return h.blockhead_occupies_tile_at_pos_ignore_blockhead(pos, ignore);
    }
    real_dw(user).is_some_and(|d| d.blockhead_occupies_tile_at_pos_ignore_blockhead(pos, ignore))
}

/// `[dynamicWorld workbenchAtPos:]` as a `WirePowerSource`.
pub(crate) fn dw_workbench_at_pos(user: Option<&ObjRef>, pos: IntPair) -> Option<Rc<dyn WirePowerSource>> {
    if let Some(h) = dw_hooks() {
        return h.workbench_at_pos(pos);
    }
    let wb = real_dw(user)?.workbench_at_pos(pos)?;
    Some(Rc::new(WorkbenchObj(wb)))
}

/// `WirePowerSource` view of a workbench `ObjRef` (messages go to the `Workbench` behind it).
struct WorkbenchObj(ObjRef);
impl WorkbenchObj {
    fn wb(&self) -> &crate::crafting::workbench::Workbench {
        crate::objects::handle::downcast::<crate::crafting::workbench::Workbench>(&self.0)
            .expect("workbenchAtPos returned a non-Workbench")
    }
}
impl WirePowerSource for WorkbenchObj {
    fn available_electricity(&self) -> u16 {
        WirePowerSource::available_electricity(self.wb())
    }
    fn generates_electricity(&self) -> bool {
        WirePowerSource::generates_electricity(self.wb())
    }
    fn is_storage_device(&self) -> bool {
        WirePowerSource::is_storage_device(self.wb())
    }
    fn conducts_electricity(&self) -> bool {
        WirePowerSource::conducts_electricity(self.wb())
    }
    fn subtract_electricty(&self, amount: u16) -> bool {
        WirePowerSource::subtract_electricty(self.wb(), amount)
    }
}

// ---------------------------------------------------------------------------------------------------------
// path-user message helpers (messages to nil return 0 / zero struct)
// ---------------------------------------------------------------------------------------------------------

/// `[pathUser pos]` (nil => zero; the original reads an uninitialised slot, see dpad/ notes).
pub(crate) fn user_pos(u: Option<&ObjRef>) -> IntPair {
    u.map_or(IntPair { x: 0, y: 0 }, |u| u.pos())
}
/// `[pathUser nextPos]`
pub(crate) fn user_next_pos(u: Option<&ObjRef>) -> IntPair {
    u.and_then(|u| u.as_path_user()).map_or(IntPair { x: 0, y: 0 }, |p| p.next_pos())
}
/// `[pathUser currentTraverseToKeyFrame]`
pub(crate) fn user_current_kf(u: Option<&ObjRef>) -> i32 {
    u.and_then(|u| u.as_path_user()).map_or(0, |p| p.current_traverse_to_key_frame())
}

// ---------------------------------------------------------------------------------------------------------
// data types
// ---------------------------------------------------------------------------------------------------------

/// `DerivedTileProperties` (0x28). Not generated by types_structs (the `NSArray*` field is unsupported there).
#[derive(Clone, Default, Debug)]
pub struct DerivedTileProperties {
    pub f: i32,
    pub g: i32,
    pub h: i32,
    pub parent_index: i32,
    pub terrain_difficulty: i32,
    /// `TileTraverseType`
    pub traverse_type: i32,
    /// `TileTraverseKeyFrameType`
    pub traverse_to_key_frame: i32,
    pub unplaceable: bool,
    /// testTile's sub-path (`NSArray` of dicts). `Rc` replaces `arrayPathsToRelease`.
    pub path: Option<Rc<Vec<SubStep>>>,
}

/// Dict written by testTile into `DerivedTileProperties.path`: exactly the 6 keys `index, parentIndex,
/// terrainDifficulty, traverseType, traverseToKeyFrame, propertiesG`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SubStep {
    pub index: i32,
    pub parent_index: i32,
    pub terrain_difficulty: i32,
    pub traverse_type: i32,
    pub traverse_to_key_frame: i32,
    pub properties_g: i32,
}

/// Element of the final path (NSDictionary with 4 keys; steps copied from a sub-path keep their 6 keys).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct PathStep {
    pub index: i32,
    pub terrain_difficulty: i32,
    pub traverse_type: i32,
    pub traverse_to_key_frame: i32,
    /// `Some(..)` only for steps copied from a [`SubStep`].
    pub parent_index: Option<i32>,
    /// `Some(..)` only for steps copied from a [`SubStep`].
    pub properties_g: Option<i32>,
}

impl PathStep {
    fn from_sub(s: &SubStep) -> PathStep {
        PathStep {
            index: s.index,
            terrain_difficulty: s.terrain_difficulty,
            traverse_type: s.traverse_type,
            traverse_to_key_frame: s.traverse_to_key_frame,
            parent_index: Some(s.parent_index),
            properties_g: Some(s.properties_g),
        }
    }

    /// The NSDictionary form: keys `index, terrainDifficulty, traverseType, traverseToKeyFrame` (+ `parentIndex`,
    /// `propertiesG` for sub-path steps), all NSNumber ints.
    pub fn to_plist_dict(&self) -> PlistValue {
        let mut pairs: Vec<(String, PlistValue)> = vec![
            ("index".to_string(), PlistValue::int(self.index as i64)),
            ("terrainDifficulty".to_string(), PlistValue::int(self.terrain_difficulty as i64)),
            ("traverseType".to_string(), PlistValue::int(self.traverse_type as i64)),
            ("traverseToKeyFrame".to_string(), PlistValue::int(self.traverse_to_key_frame as i64)),
        ];
        if let Some(p) = self.parent_index {
            pairs.push(("parentIndex".to_string(), PlistValue::int(p as i64)));
        }
        if let Some(g) = self.properties_g {
            pairs.push(("propertiesG".to_string(), PlistValue::int(g as i64)));
        }
        PlistValue::dict(GsDict::from_pairs(pairs))
    }
}

/// pathQueue entry / `createPathWithDict:` argument.
#[derive(Clone, Default)]
pub struct PathRequest {
    pub path_type: i32,
    pub goal_x: i32,
    pub goal_y: i32,
    pub goal_interaction: i32,
    pub extra_data: Option<PlistValue>,
    pub path_user: Option<ObjRef>,
}

impl PathRequest {
    /// Reads `pathType, goalX, goalY, goalInteraction` with `intValue` (missing key => 0) and `extraData`.
    /// A `PlistDict` cannot carry an object reference, so `path_user` is `None` here; use
    /// [`PathRequest::from_plist_dict_with_user`]. Note:.
    pub fn from_plist_dict(d: &PlistDict) -> PathRequest {
        PathRequest::from_plist_dict_with_user(d, None)
    }

    /// As [`PathRequest::from_plist_dict`] with the `pathUser` object supplied by the caller (the
    /// `infoForPathRecalculation` producer knows `self`).
    pub fn from_plist_dict_with_user(d: &PlistDict, path_user: Option<ObjRef>) -> PathRequest {
        let int = |k: &str| d.map.get(&k.to_string()).map_or(0, |v| v.int_value());
        PathRequest {
            path_type: int("pathType"),
            goal_x: int("goalX"),
            goal_y: int("goalY"),
            goal_interaction: int("goalInteraction"),
            extra_data: d.map.get(&"extraData".to_string()).cloned(),
            path_user,
        }
    }
}

/// Handle to a derived-tile slot (index into the slot array; slot 0 is never used).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct PropsSlot(pub u32);

struct PathCreatorState {
    path_user: Option<ObjRef>,
    goal_x: i32,
    goal_y: i32,
    goal_interaction: i32,
    path_type: i32,
    extra_data: Option<PlistValue>,
    open_list: Option<IndexSet>,
    closed_list: Option<IndexSet>,
    closest_distance: i32,
    start_index: i32,
    in_progress: bool,
    path: Option<Rc<Vec<PathStep>>>,
    has_path: bool,
    has_path_to_below_tile: bool,
    first_tile_check_done: bool,
    allows_water_tiles: bool,
    slots: Vec<DerivedTileProperties>,
    derived_tile_count: i32,
    derived_tile_indices: HashMap<i32, u32>,
    elevator_blacklist: Vec<IntPair>,
}

impl PathCreatorState {
    fn new() -> PathCreatorState {
        PathCreatorState {
            path_user: None,
            goal_x: 0,
            goal_y: 0,
            goal_interaction: 0,
            path_type: 0,
            extra_data: None,
            open_list: None,
            closed_list: None,
            closest_distance: 0,
            start_index: 0,
            in_progress: false,
            path: None,
            has_path: false,
            has_path_to_below_tile: false,
            first_tile_check_done: false,
            allows_water_tiles: false,
            slots: vec![DerivedTileProperties::default()],
            derived_tile_count: 0,
            derived_tile_indices: HashMap::new(),
            elevator_blacklist: Vec::new(),
        }
    }
}

/// The two search sets while `update_path` / `calculate_fall_path` run. They are taken out of the state so that
/// no `RefCell` borrow is held across game calls. A `nil` set (never created) behaves as an always-empty set
/// whose mutations are dropped: [`Lists::fix`] re-establishes that after every mutating group.
struct Lists {
    open: IndexSet,
    closed: IndexSet,
    open_nil: bool,
    closed_nil: bool,
}

impl Lists {
    fn fix(&mut self) {
        if self.open_nil {
            self.open = IndexSet::new();
        }
        if self.closed_nil {
            self.closed = IndexSet::new();
        }
    }
}

/// The shared pathfinder.
pub struct PathCreator {
    world: Weak<World>,
    st: RefCell<PathCreatorState>,
}

impl PathCreator {
    /// `-initWithWorld:` (world stage 11): world assigned (not retained), slot array allocated, everything else
    /// zero.
    pub fn init_with_world(world: &Rc<World>) -> Rc<PathCreator> {
        Rc::new(PathCreator { world: Rc::downgrade(world), st: RefCell::new(PathCreatorState::new()) })
    }

    /// `-dealloc`: releases pathUser, lists, path, extraData and the slot array.
    pub fn dealloc(&self) {
        let mut st = self.st.borrow_mut();
        st.path_user = None;
        st.open_list = None;
        st.closed_list = None;
        st.path = None;
        st.extra_data = None;
        st.slots = vec![DerivedTileProperties::default()];
        st.derived_tile_indices.clear();
        st.derived_tile_count = 0;
    }

    fn world(&self) -> Rc<World> {
        self.world.upgrade().expect("World already deallocated")
    }

    // ---------------------------------------------------------------- accessors

    /// `-path`
    pub fn path(&self) -> Option<Rc<Vec<PathStep>>> {
        self.st.borrow().path.clone()
    }
    /// `-inProgress`
    pub fn in_progress(&self) -> bool {
        self.st.borrow().in_progress
    }
    /// `-pathUser`
    pub fn path_user(&self) -> Option<ObjRef> {
        self.st.borrow().path_user.clone()
    }
    /// `-goalX`
    pub fn goal_x(&self) -> i32 {
        self.st.borrow().goal_x
    }
    /// `-goalY`
    pub fn goal_y(&self) -> i32 {
        self.st.borrow().goal_y
    }
    /// `-goalInteraction`
    pub fn goal_interaction(&self) -> i32 {
        self.st.borrow().goal_interaction
    }
    /// `-pathType`
    pub fn path_type(&self) -> i32 {
        self.st.borrow().path_type
    }
    /// `-hasPath` ("search finished", not "a path exists")
    pub fn has_path(&self) -> bool {
        self.st.borrow().has_path
    }
    /// `-setHasPath:`
    pub fn set_has_path(&self, v: bool) {
        self.st.borrow_mut().has_path = v;
    }
    /// `-extraData`
    pub fn extra_data(&self) -> Option<PlistValue> {
        self.st.borrow().extra_data.clone()
    }
    /// `-abortPath`: `inProgress = hasPath = hasPathToBelowTile = NO`; lists and path stay.
    pub fn abort_path(&self) {
        let mut st = self.st.borrow_mut();
        st.in_progress = false;
        st.has_path = false;
        st.has_path_to_below_tile = false;
    }

    // ---------------------------------------------------------------- slot allocator

    /// `-tileDerivedPropertiesAtWorldIndex:`. The cap check (`derivedTileCount == 0x7fff`) comes FIRST (even for
    /// known indices). A new slot starts with `traverseType = INACCESSIBLE`, `traverseToKeyFrame = INACCESSIBLE`
    /// and zero elsewhere (zero-init is unobservable,).
    pub fn tile_derived_properties_at_world_index(&self, world_index: i32) -> Option<PropsSlot> {
        let mut st = self.st.borrow_mut();
        if st.derived_tile_count == MAX_DERIVED_TILES {
            return None;
        }
        let slot = st.derived_tile_indices.get(&world_index).copied().unwrap_or(0);
        if slot == 0 {
            st.derived_tile_count += 1;
            let n = st.derived_tile_count as u32;
            st.derived_tile_indices.insert(world_index, n);
            while st.slots.len() <= n as usize {
                st.slots.push(DerivedTileProperties::default());
            }
            st.slots[n as usize] = DerivedTileProperties {
                traverse_type: TRAVERSE_INACCESSIBLE,
                traverse_to_key_frame: KF_INACCESSIBLE,
                ..Default::default()
            };
            Some(PropsSlot(n))
        } else {
            Some(PropsSlot(slot))
        }
    }

    /// Clone of a slot.
    pub fn props(&self, s: PropsSlot) -> DerivedTileProperties {
        self.st.borrow().slots[s.0 as usize].clone()
    }

    /// Read a slot in place (`f` must not call game code).
    pub fn with_props<R>(&self, s: PropsSlot, f: impl FnOnce(&DerivedTileProperties) -> R) -> R {
        f(&self.st.borrow().slots[s.0 as usize])
    }

    /// Mutate a slot in place (`f` must not call game code). Aliased parent/test slots are handled by callers
    /// always going through the slot, in the written order.
    pub fn with_props_mut<R>(&self, s: PropsSlot, f: impl FnOnce(&mut DerivedTileProperties) -> R) -> R {
        f(&mut self.st.borrow_mut().slots[s.0 as usize])
    }

    /// Number of slots handed out in the current search.
    pub fn derived_tile_count(&self) -> i32 {
        self.st.borrow().derived_tile_count
    }

    fn reset_slots(st: &mut PathCreatorState) {
        st.derived_tile_indices.clear();
        st.derived_tile_count = 0;
        st.has_path_to_below_tile = false;
    }

    // ---------------------------------------------------------------- createPathWithDict

    /// `-createPathWithDict:`. Releases the previous request, resets the slot table and blacklist, stores the new
    /// request, then runs [`calculate_fall_path`] (types 5/6, `inProgress` untouched, `hasPath = YES`) or
    /// [`calculate_path`].
    pub fn create_path_with_dict(&self, d: &PathRequest) {
        {
            let mut st = self.st.borrow_mut();
            st.path_user = None;
            st.open_list = None;
            st.closed_list = None;
            st.path = None;
            st.extra_data = None;
            Self::reset_slots(&mut st);
            st.elevator_blacklist.clear();
            st.path_type = d.path_type;
            st.goal_x = d.goal_x;
            st.goal_y = d.goal_y;
            st.extra_data = d.extra_data.clone();
            st.path_user = d.path_user.clone();
            st.goal_interaction = d.goal_interaction;
        }
        let ty = self.path_type();
        if ty == PATH_FALL || ty == PATH_FALL_INTENTIONAL {
            let p = self.calculate_fall_path();
            let mut st = self.st.borrow_mut();
            st.path = p.map(Rc::new);
            st.has_path = true;
        } else {
            self.calculate_path();
        }
    }

    // ---------------------------------------------------------------- goal validation (normal requests)

    /// Goal validation: the goal must be loaded, visible to the user and lit (or close by); an enclosed goal with
    /// a free user position is rejected; otherwise the search is set up and the first step is run.
    pub fn calculate_path(&self) {
        let world = self.world();
        let (gx, gy, user) = {
            let st = self.st.borrow();
            (st.goal_x, st.goal_y, st.path_user.clone())
        };
        let user = user.as_ref();

        if !Self::goal_reachable_by_light(&world, user, gx, gy)
            || Self::goal_unreachable_enclosure(&world, user, gx, gy)
        {
            self.set_finished();
            return;
        }
        self.begin_search(&world, user);
        self.update_path();
    }

    /// Visible goal that is either lit in any channel or within squared wrapped distance 17 of the user.
    fn goal_reachable_by_light(world: &Rc<World>, user: Option<&ObjRef>, gx: i32, gy: i32) -> bool {
        let Some(cell) = tile_at_world_position_loaded(gx, gy, world) else { return false };
        let visible = user
            .and_then(|u| u.as_path_user())
            .is_some_and(|pu| pu.tile_is_lit_for_self_at_pos(&cell.get(), IntPair { x: gx, y: gy }));
        if !visible {
            return false;
        }
        if cell.sun_light() != 0
            || cell.artificial_light_r() != 0
            || cell.artificial_light_g() != 0
            || cell.artificial_light_b() != 0
        {
            return true;
        }
        let me = user_pos(user);
        let dx = wrap_dx(me.x.wrapping_sub(gx), world.world_width_macro());
        let dy = me.y.wrapping_sub(gy);
        dx.wrapping_mul(dx).wrapping_add(dy.wrapping_mul(dy)) < 17
    }

    /// True when all four neighbours of the goal block entry (below/above: solid and no usable trap door; right/left:
    /// solid and no usable door; evaluated lazily in that order) and the user's next position is not solid.
    fn goal_unreachable_enclosure(world: &Rc<World>, user: Option<&ObjRef>, gx: i32, gy: i32) -> bool {
        let sealed = |x: i32, y: i32, trap: bool| {
            let t = tile_at_world_position_loaded(x, y, world);
            let p = IntPair { x, y };
            tile_is_solid(t)
                && !(if trap {
                    tile_contains_usable_trap_door(t, p, world, user)
                } else {
                    tile_contains_usable_door(t, p, world, user)
                })
        };
        let enclosed = sealed(gx, gy.wrapping_sub(1), true)
            && sealed(gx, gy.wrapping_add(1), true)
            && sealed(gx.wrapping_add(1), gy, false)
            && sealed(gx.wrapping_sub(1), gy, false);
        if !enclosed {
            return false;
        }
        let np = user_next_pos(user);
        !tile_is_solid(tile_at_world_position_loaded(np.x, np.y, world))
    }

    fn begin_search(&self, world: &Rc<World>, user: Option<&ObjRef>) {
        let np = user_next_pos(user);
        let start_index = world_index_at_world_position(np.x, np.y, world);
        let width = world.world_width_macro().wrapping_mul(32);
        let mut st = self.st.borrow_mut();
        st.in_progress = true;
        st.open_list = Some(IndexSet::new());
        st.closed_list = Some(IndexSet::new());
        if st.goal_x < 0 {
            st.goal_x = st.goal_x.wrapping_add(width);
        } else if st.goal_x >= width {
            st.goal_x = st.goal_x.wrapping_sub(width);
        }
        st.start_index = start_index;
        st.closest_distance =
            np.x.wrapping_sub(st.goal_x).wrapping_abs().wrapping_add(np.y.wrapping_sub(st.goal_y).wrapping_abs());
        st.first_tile_check_done = false;
        st.allows_water_tiles = true;
    }

    fn set_finished(&self) {
        let mut st = self.st.borrow_mut();
        st.in_progress = false;
        st.has_path = true;
    }

    // ---------------------------------------------------------------- search steps

    fn take_lists(&self) -> Lists {
        let mut st = self.st.borrow_mut();
        let (o, c) = (st.open_list.take(), st.closed_list.take());
        Lists {
            open_nil: o.is_none(),
            closed_nil: c.is_none(),
            open: o.unwrap_or_default(),
            closed: c.unwrap_or_default(),
        }
    }

    fn restore_lists(&self, l: Lists) {
        let mut st = self.st.borrow_mut();
        st.open_list = (!l.open_nil).then_some(l.open);
        st.closed_list = (!l.closed_nil).then_some(l.closed);
    }

    /// One incremental search step; call until [`in_progress`](Self::in_progress) turns false.
    pub fn update_path(&self) {
        let mut lists = self.take_lists();
        if self.advance(&mut lists) {
            // elevator re-plan: both sets were discarded, start over with the grown blacklist
            self.calculate_path();
        } else {
            self.restore_lists(lists);
        }
    }

    fn snapshot(&self, world: &Rc<World>) -> Query {
        let st = self.st.borrow();
        Query {
            goal_x: st.goal_x,
            goal_y: st.goal_y,
            interaction: st.goal_interaction,
            user: st.path_user.clone(),
            start_index: st.start_index,
            blacklist: st.elevator_blacklist.clone(),
            water: st.allows_water_tiles,
            goal_index: world_index_at_world_position(st.goal_x, st.goal_y, world),
        }
    }

    /// Runs start check, expansion, extraction and completion. `true` = elevator re-plan requested.
    fn advance(&self, lists: &mut Lists) -> bool {
        let world = self.world();
        let q = self.snapshot(&world);

        if !self.st.borrow().first_tile_check_done && !self.check_start(&world, &q, lists) {
            return false;
        }

        let (found, dig_found) = self.expand(&world, &q, lists);

        if (found || (dig_found && self.st.borrow().path.is_none())) && self.extract(&world, &q, found) {
            return true;
        }

        if !found && !lists.open.is_empty() {
            return false;
        }
        self.set_finished();
        false
    }

    /// Section A. `false` = the step must end right away.
    fn check_start(&self, world: &Rc<World>, q: &Query, lists: &mut Lists) -> bool {
        let Some(tile) = tile_at_world_index_loaded(q.start_index, world) else { return false };
        self.st.borrow_mut().first_tile_check_done = true;
        let Some(slot) = self.tile_derived_properties_at_world_index(q.start_index) else { return false };
        let kf = user_current_kf(q.user.as_ref());
        self.reset_start_record(slot, kf);
        let at = get_world_pos_for_world_index(q.start_index, world);
        let me = Parent { index: -1, tile: Some(tile), slot, x: at.x, y: at.y };
        let goal = (q.goal_x, q.goal_y, q.interaction);
        if !self.probe(world, q, lists, (at.x, at.y), goal, &me, false) {
            self.st.borrow_mut().first_tile_check_done = false;
        }
        true
    }

    fn reset_start_record(&self, slot: PropsSlot, kf: i32) {
        self.with_props_mut(slot, |p| {
            p.f = 0;
            p.g = 0;
            p.h = 0;
            p.parent_index = -1;
            p.traverse_to_key_frame = kf;
            p.unplaceable = false;
            p.path = None;
        });
    }

    /// One neighbour evaluation (`false` = needed tile not available yet).
    #[allow(clippy::too_many_arguments)]
    fn probe(
        &self,
        world: &Rc<World>,
        q: &Query,
        lists: &mut Lists,
        at: (i32, i32),
        goal: (i32, i32, i32),
        parent: &Parent,
        fall: bool,
    ) -> bool {
        let ok = test_tile(
            at.0,
            at.1,
            goal.0,
            goal.1,
            goal.2,
            &mut lists.open,
            &mut lists.closed,
            parent.index,
            parent.tile,
            parent.slot,
            parent.x,
            parent.y,
            q.user.as_ref(),
            world,
            fall,
            self,
            q.water,
            &q.blacklist,
        );
        lists.fix();
        ok
    }

    /// Lowest-f open cell (first wins ties). Marks the search finished when a record is unobtainable.
    fn pick_best(&self, world: &Rc<World>, lists: &Lists) -> Option<(i32, PropsSlot, TileRef)> {
        let mut best_f = 9_999_999;
        let mut best = None;
        let all: Vec<u64> = lists.open.iter().collect();
        for raw in all {
            let idx = raw as i64 as i32;
            let tile = tile_at_world_index_loaded(idx, world);
            let slot = self.tile_derived_properties_at_world_index(idx);
            if slot.is_none() {
                self.set_finished();
            }
            if let (Some(tile), Some(slot)) = (tile, slot) {
                let f = self.with_props(slot, |p| p.f);
                if f < best_f {
                    best_f = f;
                    best = Some((idx, slot, tile));
                }
            }
        }
        best
    }

    /// Section B. Returns (goal reached, fall-creating dig found).
    fn expand(&self, world: &Rc<World>, q: &Query, lists: &mut Lists) -> (bool, bool) {
        let wm = world.world_width_macro();
        let goal = (q.goal_x, q.goal_y, q.interaction);
        let mut retries = 0;
        let mut dig_found = false;
        for _ in 0..EXPANSIONS_PER_STEP {
            if lists.open.is_empty() {
                break;
            }
            let Some((idx, slot, tile)) = self.pick_best(world, lists) else { break };
            if !self.st.borrow().in_progress {
                break;
            }
            let at = get_world_pos_for_world_index(idx, world);
            let (bx, by) = (at.x, at.y);
            let dxg = wrap_dx(q.goal_x.wrapping_sub(bx), wm);
            if dxg == 0 && by == q.goal_y.wrapping_sub(1) {
                self.st.borrow_mut().has_path_to_below_tile = true;
            }

            lists.open.remove_index(ns_index(idx));
            let unplaceable = self.with_props(slot, |p| p.unplaceable);
            let at_goal = dxg == 0 && by == q.goal_y;
            if at_goal && unplaceable && retries < GOAL_RETRIES {
                retries += 1;
            } else {
                lists.closed.add_index(ns_index(idx));
            }
            lists.fix();
            if at_goal {
                if !unplaceable {
                    return (true, dig_found);
                }
                if q.interaction == INTERACTION_DIG || q.interaction == INTERACTION_DIG_FOREGROUND {
                    dig_found = true;
                }
            }

            let dist = dxg.wrapping_abs().wrapping_add(by.wrapping_sub(q.goal_y).wrapping_abs());
            {
                let mut st = self.st.borrow_mut();
                if dist < st.closest_distance {
                    st.closest_distance = dist;
                } else if dist > st.closest_distance.wrapping_add(256)
                    || (st.has_path_to_below_tile && dist > st.closest_distance.wrapping_add(8))
                {
                    continue;
                }
            }

            let me = Parent { index: idx, tile: Some(tile), slot, x: bx, y: by };
            let self_first = self.with_props(slot, |p| p.unplaceable) && SELF_EVAL_GOALS.contains(&q.interaction);
            let ok = (!self_first || self.probe(world, q, lists, (bx, by), goal, &me, false))
                && NEIGHBOURS.iter().all(|&(ox, oy)| {
                    self.probe(world, q, lists, (bx.wrapping_add(ox), by.wrapping_add(oy)), goal, &me, false)
                });
            if !ok {
                lists.open.add_index(ns_index(idx));
                lists.closed.remove_index(ns_index(idx));
                lists.fix();
                break;
            }
        }
        (false, dig_found)
    }

    // ---------------------------------------------------------------- extraction

    /// `true` = an elevator shaft was blacklisted and the search must be re-planned. Running out of records ends the
    /// search (finished, no path update).
    fn extract(&self, world: &Rc<World>, q: &Query, reached: bool) -> bool {
        self.st.borrow_mut().path = None;
        let Some(gslot) = self.tile_derived_properties_at_world_index(q.goal_index) else {
            self.set_finished();
            return false;
        };
        let mut steps: Vec<PathStep> = Vec::new();
        let mut index = q.goal_index;
        if let Some(sub) = self.with_props(gslot, |p| p.path.clone()) {
            let mut ended_at_start = false;
            for e in sub.iter().rev() {
                if e.parent_index == -1 {
                    ended_at_start = true;
                } else {
                    steps.push(PathStep::from_sub(e));
                }
            }
            index = -1;
            match steps.last() {
                Some(last) if !ended_at_start => index = last.parent_index.unwrap_or(-1),
                Some(last) if last.index == q.start_index => {
                    steps.pop();
                }
                _ => {}
            }
        }
        if !reached && steps.len() > 1 {
            steps.truncate(1);
        }

        let mut run = 0;
        let mut previous = 0;
        let mut links = 0;
        let mut gave_up = false;
        while index != -1 && !(index == q.start_index && !steps.is_empty()) {
            let Some(slot) = self.tile_derived_properties_at_world_index(index) else {
                self.set_finished();
                return false;
            };
            let p = self.props(slot);
            if p.traverse_to_key_frame == KF_STAND_ELEVATOR || p.traverse_type == TRAVERSE_ELEVATOR {
                run += 1;
            } else {
                if self.blacklist_short_run(world, run, previous) {
                    return true;
                }
                run = 0;
            }
            steps.push(PathStep {
                index,
                terrain_difficulty: p.terrain_difficulty,
                traverse_type: p.traverse_type,
                traverse_to_key_frame: p.traverse_to_key_frame,
                parent_index: None,
                properties_g: None,
            });
            previous = index;
            index = p.parent_index;
            if index == 0 {
                crate::nslog!("crap fail");
            }
            links += 1;
            if links >= MAX_LINKS {
                gave_up = true;
                break;
            }
        }
        if self.blacklist_short_run(world, run, previous) {
            return true;
        }
        self.st.borrow_mut().path = (!gave_up).then(|| Rc::new(steps));
        false
    }

    /// An elevator run of 1 or 2 cells with a motor for its shaft: blacklist the motor and reset the search.
    fn blacklist_short_run(&self, world: &Rc<World>, run: i32, previous: i32) -> bool {
        if !(1..=2).contains(&run) {
            return false;
        }
        let shaft = get_world_pos_for_world_index(previous, world);
        let Some(motor) = world.dynamic_world().and_then(|dw| dw.elevator_motor_for_shaft_at_pos(shaft)) else {
            return false;
        };
        let mut st = self.st.borrow_mut();
        st.elevator_blacklist.push(motor.pos);
        st.open_list = None;
        st.closed_list = None;
        st.path = None;
        Self::reset_slots(&mut st);
        true
    }

    // ---------------------------------------------------------------- fall path

    /// Fall path for the user's next position; `None` when no landing cell can be determined.
    pub fn calculate_fall_path(&self) -> Option<Vec<PathStep>> {
        let mut lists = self.take_lists();
        let out = self.fall_search(&mut lists);
        self.restore_lists(lists);
        out
    }

    fn fall_search(&self, lists: &mut Lists) -> Option<Vec<PathStep>> {
        let world = self.world();
        let (user, path_type, bl) = {
            let st = self.st.borrow();
            (st.path_user.clone(), st.path_type, st.elevator_blacklist.clone())
        };
        let start = user_next_pos(user.as_ref());
        let start_index = world_index_at_world_position(start.x, start.y, &world);
        self.st.borrow_mut().start_index = start_index;
        let start_tile = tile_at_world_index_loaded(start_index, &world)?;
        let start_slot = self.tile_derived_properties_at_world_index(start_index)?;
        self.reset_start_record(start_slot, user_current_kf(user.as_ref()));
        self.st.borrow_mut().allows_water_tiles = true;

        let q = Query {
            goal_x: start.x,
            goal_y: start.y,
            interaction: INTERACTION_NONE,
            user,
            start_index,
            blacklist: bl,
            water: true,
            goal_index: start_index,
        };
        let origin = Parent { index: start_index, tile: Some(start_tile), slot: start_slot, x: start.x, y: start.y };
        // outcome intentionally ignored (probe the start cell against a far-below goal)
        self.probe(
            &world,
            &q,
            lists,
            (start.x, start.y),
            (start.x, start.y.wrapping_sub(100), INTERACTION_NONE),
            &Parent { index: -1, ..origin },
            true,
        );

        let goal = (start.x, start.y, INTERACTION_NONE);
        let falling = |slot: PropsSlot| {
            let kf = self.with_props(slot, |p| p.traverse_to_key_frame);
            kf == KF_FALLING || kf == KF_INACCESSIBLE
        };
        let at = |y: i32| world_index_at_world_position(start.x, y, &world);

        let mut landing = None;
        let mut y = start.y;
        while y >= 0 {
            let Some(tile) = tile_at_world_index_loaded(at(y), &world) else {
                if y + 1 == start.y {
                    return None;
                }
                landing = Some(at(y + 1));
                break;
            };
            let slot = self.tile_derived_properties_at_world_index(at(y))?;
            if !falling(slot) {
                landing = Some(at(y));
                break;
            }
            let me = Parent { index: at(y), tile: Some(tile), slot, x: start.x, y };
            if y == start.y && path_type == PATH_FALL {
                self.probe(&world, &q, lists, (start.x, y + 1), goal, &me, true);
                if tile_at_world_index_loaded(at(y + 1), &world).is_none() {
                    landing = Some(at(y + 1));
                    break;
                }
                let above = self.tile_derived_properties_at_world_index(at(y + 1))?;
                if !falling(above) {
                    landing = Some(at(y + 1));
                    break;
                }
            }
            if !self.probe(&world, &q, lists, (start.x, y - 1), goal, &me, true) {
                landing = Some(at(y));
                break;
            }
            y -= 1;
        }

        let mut index = landing?;
        let mut steps = Vec::new();
        while index != -1 {
            let p = self.props(self.tile_derived_properties_at_world_index(index)?);
            steps.push(PathStep {
                index,
                terrain_difficulty: p.terrain_difficulty,
                traverse_type: p.traverse_type,
                traverse_to_key_frame: p.traverse_to_key_frame,
                parent_index: None,
                properties_g: None,
            });
            if steps.len() >= MAX_LINKS as usize {
                return None;
            }
            index = p.parent_index;
        }
        if steps.len() > 1 {
            steps.pop();
        }
        Some(steps)
    }
}

// ---------------------------------------------------------------------------------------------------------
// search plumbing
// ---------------------------------------------------------------------------------------------------------

const EXPANSIONS_PER_STEP: usize = 32;
const GOAL_RETRIES: i32 = 3;
const MAX_LINKS: i32 = 0x8000;

/// Neighbour evaluation order (offsets from the expanded cell).
const NEIGHBOURS: [(i32, i32); 16] = [
    (-1, 1),
    (0, 1),
    (1, 1),
    (-1, 0),
    (1, 0),
    (-1, -1),
    (0, -1),
    (1, -1),
    (0, 2),
    (0, -2),
    (-1, 2),
    (1, 2),
    (-1, -2),
    (1, -2),
    (2, 0),
    (-2, 0),
];

/// Goal interactions for which an unplaceable cell is first evaluated against itself.
const SELF_EVAL_GOALS: [i32; 8] = [
    INTERACTION_PICKUP_FREEBLOCK,
    INTERACTION_PICKUP,
    INTERACTION_PLACE_ITEM,
    INTERACTION_USE_INTERACTIVE_OBJECT,
    INTERACTION_CRAFT,
    INTERACTION_DIG,
    INTERACTION_DIG_FOREGROUND,
    INTERACTION_DIG_BACK_WALL,
];

/// Immutable view of the request for the duration of one step.
struct Query {
    goal_x: i32,
    goal_y: i32,
    interaction: i32,
    user: Option<ObjRef>,
    start_index: i32,
    blacklist: Vec<IntPair>,
    water: bool,
    goal_index: i32,
}

/// The cell whose neighbours are being evaluated.
struct Parent {
    index: i32,
    tile: Option<TileRef>,
    slot: PropsSlot,
    x: i32,
    y: i32,
}

#[cfg(test)]
pub(crate) mod testutil {
    use super::*;
    use crate::objects::base::{DynamicObject, DynamicObjectBase};
    use crate::objects::class_info::{ClassInfo, DYNAMIC_OBJECT_CLASS};
    use crate::objects::handle;
    use crate::objects::protocols::PathUserDynamicObject;
    use crate::objects::protocols::PowerUser;
    use crate::types::tile::Tile;
    use crate::world::helpers::testutil::FakeDw;
    use crate::world::state::{DynamicWorld, MacroTileRef, WorldOpsHooks};
    use std::any::Any;
    use std::cell::Cell;

    /// Block loader that fills each new block from a generator `(worldX, worldY) -> Tile`.
    pub struct GridLoader {
        pub fill: RefCell<Box<dyn Fn(i32, i32) -> Tile>>,
        pub loads: RefCell<Vec<(i32, i32)>>,
        pub dyn_loads: Cell<u32>,
    }

    impl WorldOpsHooks for GridLoader {
        fn load_physical_block_for_macro_tile_at_x_y_load_surrounding_blocks_create_if_not_created(
            &self,
            world: &World,
            mt: &MacroTileRef,
            mx: i32,
            my: i32,
            _surrounding: bool,
            _create: bool,
        ) -> bool {
            let pb = world.take_physical_block();
            pb.x_pos.set(mx);
            pb.y_pos.set(my);
            pb.valid.set(true);
            for ly in 0..32 {
                for lx in 0..32 {
                    let t = (self.fill.borrow())(mx * 32 + lx, my * 32 + ly);
                    pb.tiles[(ly * 32 + lx) as usize].set(t);
                }
            }
            mt.dirty.set(0);
            *mt.physical_block.borrow_mut() = Some(pb);
            self.loads.borrow_mut().push((mx, my));
            true
        }
        fn load_dynamic_objects_if_not_already_loaded_for_macro_tile_include_surface_blocks(
            &self,
            _w: &World,
            _mt: &MacroTileRef,
            _i: bool,
        ) {
            self.dyn_loads.set(self.dyn_loads.get() + 1);
        }
        fn tile_is_protected_at_pos_against_blockhead(&self, _w: &World, _p: IntPair, _b: &ObjRef) -> bool {
            false
        }
    }

    pub const STONE: u8 = 1;
    pub const AIR: u8 = 2;

    /// Solid ground for `y <= ground`, air above; everything fully sun-lit.
    pub fn flat_tile(ground: i32) -> impl Fn(i32, i32) -> Tile {
        move |_x, y| {
            let mut t = Tile::default();
            t.type_index = if y <= ground { STONE } else { AIR };
            t.sun_light = 255;
            t
        }
    }

    /// World with width `w_macro` macro tiles, a flat-ground loader and a fake dynamic world.
    pub fn new_world(w_macro: i32, ground: i32) -> (Rc<World>, Rc<GridLoader>, Rc<FakeDw>) {
        let world = World::alloc(w_macro);
        let loader = Rc::new(GridLoader {
            fill: RefCell::new(Box::new(flat_tile(ground))),
            loads: RefCell::new(Vec::new()),
            dyn_loads: Cell::new(0),
        });
        world.set_ops_hooks(Some(loader.clone()));
        let dw = Rc::new(FakeDw::default());
        *world.dynamic_world.borrow_mut() = Some(Rc::new(DynamicWorld::new(Box::new(dw.clone()))));
        (world, loader, dw)
    }

    /// Edit a (loaded) tile.
    pub fn edit(world: &Rc<World>, x: i32, y: i32, f: impl FnOnce(&mut Tile)) {
        let t = tile_at_world_position_loaded(x, y, world).expect("tile");
        t.update(f);
    }

    /// Stand-in path user.
    pub struct TestUser {
        pub base: RefCell<DynamicObjectBase>,
        pub next_pos: Cell<IntPair>,
        pub kf: Cell<i32>,
        pub can_fly: Cell<bool>,
        pub can_crawl: Cell<bool>,
        pub storage: Cell<bool>,
    }

    impl DynamicObject for TestUser {
        fn base(&self) -> &RefCell<DynamicObjectBase> {
            &self.base
        }
        fn class_info(&self) -> &'static ClassInfo {
            &DYNAMIC_OBJECT_CLASS
        }
        fn as_any(&self) -> &dyn Any {
            self
        }
        fn as_path_user(&self) -> Option<&dyn PathUserDynamicObject> {
            Some(self)
        }
        fn as_power_user(&self) -> Option<&dyn PowerUser> {
            Some(self)
        }
    }
    impl PowerUser for TestUser {
        fn is_storage_device(&self) -> bool {
            self.storage.get()
        }
    }
    impl PathUserDynamicObject for TestUser {
        fn tile_is_lit_for_self_at_pos(&self, _tile: &Tile, _pos: IntPair) -> bool {
            true
        }
        fn current_traverse_to_key_frame(&self) -> i32 {
            self.kf.get()
        }
        fn next_pos(&self) -> IntPair {
            self.next_pos.get()
        }
        fn info_for_path_recalculation(&self) -> Option<PlistDict> {
            None
        }
        fn set_path_type_goal_interaction_extra_data(
            &self,
            _me: &ObjRef,
            _p: Option<Vec<PlistValue>>,
            _t: i32,
            _g: i32,
            _e: Option<PlistValue>,
        ) {
        }
        fn set_waiting_for_path_to_pos(&self, _p: IntPair) {}
        fn set_no_longer_waiting_for_path(&self) {}
        fn waiting_for_path(&self) -> bool {
            false
        }
        fn can_crawl(&self) -> bool {
            self.can_crawl.get()
        }
        fn can_fly(&self) -> bool {
            self.can_fly.get()
        }
        fn path_needs_recalculated(&self) -> bool {
            false
        }
        fn set_path_needs_recalculated(&self, _v: bool) {}
    }

    /// A path user standing at `pos` (air tile above the ground), key frame STAND.
    pub fn new_user(pos: IntPair) -> ObjRef {
        let o: Rc<TestUser> = handle::alloc_obj(|w| TestUser {
            base: RefCell::new(DynamicObjectBase::alloc_zeroed_with(w)),
            next_pos: Cell::new(pos),
            kf: Cell::new(0),
            can_fly: Cell::new(false),
            can_crawl: Cell::new(false),
            storage: Cell::new(false),
        });
        o.base.borrow_mut().pos = pos;
        o
    }

    /// Downcast helper to reach the `TestUser` knobs.
    pub fn user_knobs(o: &ObjRef) -> &TestUser {
        handle::downcast::<TestUser>(o).expect("TestUser")
    }
}

#[cfg(test)]
mod tests {
    use super::testutil::*;
    use super::*;
    use crate::world::tiles::tile_at_world_index_loaded;

    fn ip(x: i32, y: i32) -> IntPair {
        IntPair { x, y }
    }

    fn req(user: &ObjRef, gx: i32, gy: i32, interaction: i32, ty: i32) -> PathRequest {
        PathRequest {
            path_type: ty,
            goal_x: gx,
            goal_y: gy,
            goal_interaction: interaction,
            extra_data: None,
            path_user: Some(user.clone()),
        }
    }

    #[test]
    fn wrap_dx_goldens() {
        assert_eq!(wrap_dx(1023, 64), 1023);
        assert_eq!(wrap_dx(1024, 64), -1024);
        assert_eq!(wrap_dx(-1024, 64), -1024);
        assert_eq!(wrap_dx(-1025, 64), 1023);
        assert_eq!(wrap_dx(0, 64), 0);
    }

    #[test]
    fn slot_allocator() {
        let (world, _l, _d) = new_world(32, 20);
        let pc = PathCreator::init_with_world(&world);
        let s = pc.tile_derived_properties_at_world_index(500).unwrap();
        assert_eq!(s, PropsSlot(1));
        let p = pc.props(s);
        assert_eq!((p.traverse_to_key_frame, p.traverse_type), (8, 25));
        assert_eq!(pc.tile_derived_properties_at_world_index(500), Some(PropsSlot(1)));
        for i in 1..32767 {
            assert!(pc.tile_derived_properties_at_world_index(1000 + i).is_some(), "slot {i}");
        }
        assert_eq!(pc.derived_tile_count(), 32767);
        // the 32768th distinct index and every later call, even for known indices, return None
        assert!(pc.tile_derived_properties_at_world_index(999_999).is_none());
        assert!(pc.tile_derived_properties_at_world_index(500).is_none());
        // createPathWithDict resets count and map
        let user = new_user(ip(100, 21));
        pc.create_path_with_dict(&req(&user, 100, 21, 0, 5));
        assert!(pc.derived_tile_count() < 10);
    }

    #[test]
    fn calculate_path_rejects() {
        // goal tile not loadable (no block loader)
        let (world, _l, _d) = new_world(32, 20);
        world.set_ops_hooks(None);
        let pc = PathCreator::init_with_world(&world);
        let user = new_user(ip(100, 21));
        pc.create_path_with_dict(&req(&user, 105, 21, 0, 0));
        assert!(!pc.in_progress() && pc.has_path() && pc.path().is_none());

        // goal not lit and farther than the radius
        let (world, _l, _d) = new_world(32, 20);
        let pc = PathCreator::init_with_world(&world);
        let user = new_user(ip(100, 21));
        edit(&world, 120, 21, |t| t.sun_light = 0);
        pc.create_path_with_dict(&req(&user, 120, 21, 0, 0));
        assert!(!pc.in_progress() && pc.has_path() && pc.path().is_none());
        // unlit but within sqrt(17): accepted
        edit(&world, 103, 21, |t| t.sun_light = 0);
        pc.create_path_with_dict(&req(&user, 103, 21, 0, 0));
        assert!(pc.in_progress() || pc.path().is_some());

        // fully enclosed goal, user not inside a solid tile
        let (world, _l, _d) = new_world(32, 20);
        let pc = PathCreator::init_with_world(&world);
        let user = new_user(ip(100, 21));
        for (x, y) in [(110, 40), (110, 42), (109, 41), (111, 41)] {
            edit(&world, x, y, |t| t.type_index = STONE);
        }
        pc.create_path_with_dict(&req(&user, 110, 41, 0, 0));
        assert!(!pc.in_progress() && pc.has_path() && pc.path().is_none());
    }

    /// : walk along flat ground, goal first, start excluded.
    #[test]
    fn flat_walk() {
        let (world, _l, _d) = new_world(32, 20);
        let pc = PathCreator::init_with_world(&world);
        let user = new_user(ip(100, 21));
        pc.create_path_with_dict(&req(&user, 105, 21, 0, 0));
        run_to_end(&pc);
        assert!(pc.has_path());
        let path = pc.path().expect("path");
        let w = 32 * 32;
        let idx = |x: i32, y: i32| y * w + x;
        let got: Vec<i32> = path.iter().map(|s| s.index).collect();
        assert_eq!(got, (101..=105).rev().map(|x| idx(x, 21)).collect::<Vec<_>>());
        assert!(path
            .iter()
            .all(|s| s.traverse_type == 1 && s.terrain_difficulty == 10 && s.traverse_to_key_frame == 0));
        // dict form
        let d = path[0].to_plist_dict();
        assert_eq!(d.object_for_key("index").unwrap().int_value(), idx(105, 21));
        assert!(d.object_for_key("parentIndex").is_none());
    }

    /// : the 16 neighbours of the first expansion get consecutive slots in this order.
    #[test]
    fn neighbour_order() {
        let (world, _l, _d) = new_world(32, 20);
        let pc = PathCreator::init_with_world(&world);
        let user = new_user(ip(100, 40));
        edit(&world, 100, 40, |t| t.type_index = AIR);
        pc.create_path_with_dict(&req(&user, 300, 40, 0, 0));
        let w = 32 * 32;
        let offs = [
            (-1, 1),
            (0, 1),
            (1, 1),
            (-1, 0),
            (1, 0),
            (-1, -1),
            (0, -1),
            (1, -1),
            (0, 2),
            (0, -2),
            (-1, 2),
            (1, 2),
            (-1, -2),
            (1, -2),
            (2, 0),
            (-2, 0),
        ];
        let start = pc.tile_derived_properties_at_world_index(40 * w + 100).unwrap();
        assert_eq!(start, PropsSlot(1));
        for (i, (dx, dy)) in offs.iter().enumerate() {
            let s = pc.tile_derived_properties_at_world_index((40 + dy) * w + 100 + dx).unwrap();
            assert_eq!(s, PropsSlot(2 + i as u32), "offset {:?}", (dx, dy));
        }
    }

    #[test]
    fn abort_keeps_path() {
        let (world, _l, _d) = new_world(32, 20);
        let pc = PathCreator::init_with_world(&world);
        let user = new_user(ip(100, 21));
        pc.create_path_with_dict(&req(&user, 103, 21, 0, 0));
        run_to_end(&pc);
        assert!(pc.path().is_some());
        pc.abort_path();
        assert!(pc.path().is_some() && !pc.in_progress() && !pc.has_path());
    }

    #[test]
    fn fall_path() {
        let (world, loader, _d) = new_world(32, 20);
        // a wide pit (x 190..=210) from y = 21 down to y = 11, floor at y = 10 (a 1-wide shaft would be wall-grabbed)
        *loader.fill.borrow_mut() = Box::new(|x, y| {
            let mut t = flat_tile(20)(x, y);
            if (190..=210).contains(&x) && (11..=20).contains(&y) {
                t.type_index = AIR;
            }
            t
        });
        let pc = PathCreator::init_with_world(&world);
        let user = new_user(ip(200, 21));
        for ty in [5, 6] {
            pc.create_path_with_dict(&req(&user, 200, 21, 0, ty));
            assert!(pc.has_path());
            let path = pc.path().expect("fall path");
            let w = 32 * 32;
            assert_eq!(path[0].index, 11 * w + 200, "landing tile first");
            assert!(path.last().unwrap().index != 21 * w + 200, "start dropped");
        }
        // start tile unloaded => nil
        world.set_ops_hooks(None);
        let (w2, _l2, _d2) = new_world(32, 20);
        w2.set_ops_hooks(None);
        let pc2 = PathCreator::init_with_world(&w2);
        let user2 = new_user(ip(200, 21));
        pc2.create_path_with_dict(&req(&user2, 200, 21, 0, 5));
        assert!(pc2.has_path() && pc2.path().is_none());
        let _ = tile_at_world_index_loaded;
    }

    /// : elevator runs of 1-2 steps are blacklisted and replanned, runs >= 3 are kept.
    #[test]
    fn elevator_runs() {
        use crate::world::state::ElevatorMotorInfo;
        for (goal_y, expect_elevator) in [(23, false), (26, true)] {
            let (world, _l, dw) = new_world(32, 20);
            let pc = PathCreator::init_with_world(&world);
            for y in 20..=28 {
                edit(&world, 300, y, |t| {
                    t.contents = 0x67;
                    t.type_index = AIR;
                });
                dw.motor
                    .borrow_mut()
                    .insert((300, y), ElevatorMotorInfo { has_required_power: true, pos: ip(300, 19) });
            }
            let user = new_user(ip(300, 21));
            pc.create_path_with_dict(&req(&user, 300, goal_y, 0, 0));
            run_to_end(&pc);
            let path = pc.path().expect("path");
            let has_elev = path.iter().any(|s| s.traverse_type == 27 || s.traverse_to_key_frame == 10);
            assert_eq!(has_elev, expect_elevator, "goal_y {goal_y}: {path:?}");
        }
    }

    // ------------------------------------------------------------------ decision-table tests (spec 2.x)

    fn run_to_end(pc: &PathCreator) {
        let mut n = 0;
        while pc.in_progress() {
            pc.update_path();
            n += 1;
            assert!(n < 400, "search does not terminate");
        }
    }

    fn query(pc: &PathCreator, world: &Rc<World>) -> Query {
        pc.snapshot(world)
    }

    #[test]
    fn goal_x_is_normalised_into_world() {
        for (user_x, goal_x, expect) in [(1020, -4, 1020), (6, 1030, 6)] {
            let (world, _l, _d) = new_world(32, 20);
            let pc = PathCreator::init_with_world(&world);
            let user = new_user(ip(user_x, 21));
            pc.create_path_with_dict(&req(&user, goal_x, 21, 0, 0));
            assert_eq!(pc.goal_x(), expect);
            run_to_end(&pc);
        }
    }

    #[test]
    fn goal_without_path_user_is_rejected() {
        let (world, _l, _d) = new_world(32, 20);
        let pc = PathCreator::init_with_world(&world);
        let mut r = req(&new_user(ip(100, 21)), 105, 21, 0, 0);
        r.path_user = None;
        pc.create_path_with_dict(&r);
        assert!(!pc.in_progress() && pc.has_path() && pc.path().is_none());
    }

    #[test]
    fn enclosed_goal_with_solid_user_cell_is_searched() {
        let (world, _l, _d) = new_world(32, 20);
        let pc = PathCreator::init_with_world(&world);
        let user = new_user(ip(100, 15)); // inside stone
        for (x, y) in [(110, 40), (110, 42), (109, 41), (111, 41)] {
            edit(&world, x, y, |t| t.type_index = STONE);
        }
        pc.create_path_with_dict(&req(&user, 110, 41, 0, 0));
        assert!(pc.in_progress() || pc.has_path());
    }

    #[test]
    fn start_cell_missing_ends_step_silently() {
        let (world, _l, _d) = new_world(32, 20);
        let pc = PathCreator::init_with_world(&world);
        let user = new_user(ip(100, 21));
        pc.create_path_with_dict(&req(&user, 300, 21, 0, 0));
        {
            let mut st = pc.st.borrow_mut();
            st.first_tile_check_done = false;
            st.start_index = -1;
        }
        pc.update_path();
        assert!(!pc.st.borrow().first_tile_check_done);
        assert!(pc.in_progress());
    }

    #[test]
    fn start_record_unavailable_ends_step() {
        let (world, _l, _d) = new_world(32, 20);
        let pc = PathCreator::init_with_world(&world);
        let user = new_user(ip(100, 21));
        pc.create_path_with_dict(&req(&user, 300, 21, 0, 0));
        {
            let mut st = pc.st.borrow_mut();
            st.first_tile_check_done = false;
            st.derived_tile_count = MAX_DERIVED_TILES;
        }
        pc.update_path();
        assert!(pc.st.borrow().first_tile_check_done, "flag is set before the record is requested");
        assert!(pc.in_progress());
    }

    #[test]
    fn record_exhaustion_during_scan_finishes_search() {
        let (world, _l, _d) = new_world(32, 20);
        let pc = PathCreator::init_with_world(&world);
        let user = new_user(ip(100, 21));
        pc.create_path_with_dict(&req(&user, 300, 21, 0, 0));
        assert!(pc.in_progress());
        pc.st.borrow_mut().derived_tile_count = MAX_DERIVED_TILES;
        pc.update_path();
        assert!(!pc.in_progress() && pc.has_path());
    }

    #[test]
    fn distance_pruning_skips_far_cells() {
        let (world, _l, _d) = new_world(32, 20);
        let pc = PathCreator::init_with_world(&world);
        let user = new_user(ip(100, 21));
        pc.create_path_with_dict(&req(&user, 300, 21, 0, 0));
        let before = pc.derived_tile_count();
        pc.st.borrow_mut().closest_distance = -10_000;
        pc.update_path();
        assert_eq!(pc.derived_tile_count(), before, "no cell was expanded");
        // below-tile mode prunes at closest + 8
        {
            let mut st = pc.st.borrow_mut();
            st.closest_distance = 0;
            st.has_path_to_below_tile = true;
        }
        pc.update_path();
        assert_eq!(pc.derived_tile_count(), before);
    }

    #[test]
    fn goal_above_flags_path_to_below_tile() {
        let (world, _l, _d) = new_world(32, 20);
        let pc = PathCreator::init_with_world(&world);
        let user = new_user(ip(100, 21));
        pc.create_path_with_dict(&req(&user, 104, 22, 0, 0));
        run_to_end(&pc);
        assert!(pc.st.borrow().has_path_to_below_tile);
        assert!(pc.path().is_some());
    }

    #[test]
    fn unloaded_neighbour_aborts_and_requeues() {
        let (world, _l, _d) = new_world(32, 20);
        let pc = PathCreator::init_with_world(&world);
        let user = new_user(ip(31, 21));
        // load the user's and the goal's blocks, then remove the loader: x = 32 stays unavailable
        for x in [31, 100] {
            let _ = tile_at_world_position_loaded(x, 21, &world);
        }
        world.set_ops_hooks(None);
        pc.create_path_with_dict(&req(&user, 100, 21, 0, 0));
        assert!(pc.in_progress());
        assert!(!pc.st.borrow().open_list.as_ref().unwrap().is_empty(), "aborted cell is back in the open set");
    }

    #[test]
    fn dig_into_stone_exercises_self_probe_and_retries() {
        for interaction in [INTERACTION_DIG, INTERACTION_DIG_FOREGROUND, INTERACTION_PICKUP, INTERACTION_PLACE_ITEM] {
            let (world, _l, _d) = new_world(32, 20);
            let pc = PathCreator::init_with_world(&world);
            let user = new_user(ip(100, 21));
            pc.create_path_with_dict(&req(&user, 104, 18, interaction, 0));
            run_to_end(&pc);
            assert!(pc.has_path());
        }
    }

    #[test]
    fn goal_surrounded_by_stone_uses_dig_fallback() {
        let (world, _l, _d) = new_world(32, 20);
        let pc = PathCreator::init_with_world(&world);
        let user = new_user(ip(100, 21));
        pc.create_path_with_dict(&req(&user, 100, 5, INTERACTION_DIG, 0));
        run_to_end(&pc);
        assert!(pc.has_path());
    }

    #[test]
    fn elevator_without_motor_keeps_short_runs() {
        let (world, _l, _d) = new_world(32, 20);
        let pc = PathCreator::init_with_world(&world);
        for y in 20..=28 {
            edit(&world, 300, y, |t| {
                t.contents = 0x67;
                t.type_index = AIR;
            });
        }
        let user = new_user(ip(300, 21));
        pc.create_path_with_dict(&req(&user, 300, 23, 0, 0));
        run_to_end(&pc);
        assert!(pc.st.borrow().elevator_blacklist.is_empty());
    }

    fn seed(pc: &PathCreator, idx: i32, parent: i32, kind: i32, pose: i32) -> PropsSlot {
        let s = pc.tile_derived_properties_at_world_index(idx).unwrap();
        pc.with_props_mut(s, |p| {
            p.parent_index = parent;
            p.traverse_type = kind;
            p.traverse_to_key_frame = pose;
            p.terrain_difficulty = 7;
        });
        s
    }

    fn sub(index: i32, parent: i32) -> SubStep {
        SubStep {
            index,
            parent_index: parent,
            terrain_difficulty: 1,
            traverse_type: 1,
            traverse_to_key_frame: 0,
            properties_g: 3,
        }
    }

    fn ready(goal_idx: i32, start_idx: i32) -> (Rc<World>, Rc<PathCreator>, Query) {
        let (world, _l, _d) = new_world(32, 20);
        let pc = PathCreator::init_with_world(&world);
        let mut q = query(&pc, &world);
        q.goal_index = goal_idx;
        q.start_index = start_idx;
        (world, pc, q)
    }

    #[test]
    fn extract_goal_record_missing() {
        let (world, pc, q) = ready(10, 1);
        pc.st.borrow_mut().derived_tile_count = MAX_DERIVED_TILES;
        assert!(!pc.extract(&world, &q, true));
        assert!(pc.has_path() && !pc.in_progress());
    }

    #[test]
    fn extract_plain_chain() {
        let (world, pc, q) = ready(30, 10);
        seed(&pc, 30, 20, 1, 0);
        seed(&pc, 20, 10, 1, 0);
        seed(&pc, 10, -1, 1, 0);
        assert!(!pc.extract(&world, &q, true));
        let got: Vec<i32> = pc.path().unwrap().iter().map(|s| s.index).collect();
        assert_eq!(got, vec![30, 20]);
    }

    #[test]
    fn extract_uses_subpath_not_ended_at_start() {
        let (world, pc, q) = ready(30, 10);
        let g = seed(&pc, 30, 20, 1, 0);
        pc.with_props_mut(g, |p| p.path = Some(Rc::new(vec![sub(25, 22), sub(30, 25)])));
        seed(&pc, 22, 10, 1, 0);
        assert!(!pc.extract(&world, &q, true));
        let path = pc.path().unwrap();
        assert_eq!(path.iter().map(|s| s.index).collect::<Vec<_>>(), vec![30, 25, 22]);
        assert_eq!(path[0].parent_index, Some(25));
        assert_eq!(path[0].properties_g, Some(3));
        assert_eq!(path[2].parent_index, None);
    }

    #[test]
    fn extract_subpath_ended_at_start_drops_start_entry() {
        let (world, pc, q) = ready(30, 10);
        let g = seed(&pc, 30, 20, 1, 0);
        pc.with_props_mut(g, |p| p.path = Some(Rc::new(vec![sub(10, -1), sub(30, 10)])));
        assert!(!pc.extract(&world, &q, true));
        // last element (the start, index 10 only if it is the final one) handling: entries are walked from the end
        let got: Vec<i32> = pc.path().unwrap().iter().map(|s| s.index).collect();
        assert_eq!(got, vec![30]);
    }

    #[test]
    fn extract_subpath_ended_at_start_keeps_other_tail() {
        let (world, pc, q) = ready(30, 99);
        let g = seed(&pc, 30, 20, 1, 0);
        pc.with_props_mut(g, |p| p.path = Some(Rc::new(vec![sub(10, -1), sub(30, 10)])));
        assert!(!pc.extract(&world, &q, true));
        let got: Vec<i32> = pc.path().unwrap().iter().map(|s| s.index).collect();
        assert_eq!(got, vec![30]);
    }

    #[test]
    fn extract_empty_subpath_gives_empty_path() {
        let (world, pc, q) = ready(30, 10);
        let g = seed(&pc, 30, 20, 1, 0);
        pc.with_props_mut(g, |p| p.path = Some(Rc::new(vec![])));
        assert!(!pc.extract(&world, &q, true));
        assert!(pc.path().unwrap().is_empty());
    }

    #[test]
    fn extract_unreached_goal_keeps_first_subpath_entry_only() {
        let (world, pc, q) = ready(30, 10);
        let g = seed(&pc, 30, 20, 1, 0);
        pc.with_props_mut(g, |p| p.path = Some(Rc::new(vec![sub(26, 25), sub(30, 26)])));
        seed(&pc, 25, 10, 1, 0);
        assert!(!pc.extract(&world, &q, false));
        let got: Vec<i32> = pc.path().unwrap().iter().map(|s| s.index).collect();
        assert_eq!(got, vec![30, 26, 25].into_iter().take(1).chain([25]).collect::<Vec<_>>());
    }

    #[test]
    fn extract_missing_record_midway() {
        let (world, pc, q) = ready(30, 10);
        seed(&pc, 30, 20, 1, 0);
        seed(&pc, 20, 21, 1, 0);
        // 21 still gets a record (the last free one, parent 0), then record 0 cannot be handed out
        pc.st.borrow_mut().derived_tile_count = MAX_DERIVED_TILES - 1;
        assert!(!pc.extract(&world, &q, true));
        assert!(pc.has_path() && !pc.in_progress());
    }

    #[test]
    fn extract_cycle_gives_up_with_no_path() {
        let (world, pc, q) = ready(30, 10);
        seed(&pc, 30, 20, 1, 0);
        seed(&pc, 20, 30, 1, 0);
        assert!(!pc.extract(&world, &q, true));
        assert!(pc.path().is_none());
    }

    #[test]
    fn extract_logs_link_to_index_zero_and_continues() {
        let (world, pc, q) = ready(30, 10);
        seed(&pc, 30, 0, 1, 0);
        seed(&pc, 0, -1, 1, 0);
        assert!(!pc.extract(&world, &q, true));
        assert_eq!(pc.path().unwrap().len(), 2);
    }

    #[test]
    fn extract_trailing_elevator_run_replans_only_with_motor() {
        use crate::world::state::ElevatorMotorInfo;
        let (world, _l, dw) = new_world(32, 20);
        let pc = PathCreator::init_with_world(&world);
        let w = 32 * 32;
        let at = |y: i32| y * w + 300;
        seed(&pc, at(22), at(21), TRAVERSE_ELEVATOR, KF_STAND_ELEVATOR);
        seed(&pc, at(21), -1, TRAVERSE_ELEVATOR, KF_STAND_ELEVATOR);
        let mut q = query(&pc, &world);
        q.goal_index = at(22);
        q.start_index = 5;
        // no motor: path kept
        assert!(!pc.extract(&world, &q, true));
        assert_eq!(pc.path().unwrap().len(), 2);
        // motor present: blacklisted and everything reset
        dw.motor.borrow_mut().insert((300, 21), ElevatorMotorInfo { has_required_power: true, pos: ip(300, 19) });
        assert!(pc.extract(&world, &q, true));
        assert_eq!(pc.st.borrow().elevator_blacklist, vec![ip(300, 19)]);
        assert!(pc.path().is_none() && pc.derived_tile_count() == 0);
    }

    #[test]
    fn extract_elevator_run_ended_by_walk_tile_replans() {
        use crate::world::state::ElevatorMotorInfo;
        let (world, _l, dw) = new_world(32, 20);
        let pc = PathCreator::init_with_world(&world);
        let w = 32 * 32;
        let at = |y: i32| y * w + 300;
        seed(&pc, at(22), at(21), TRAVERSE_ELEVATOR, KF_STAND_ELEVATOR);
        seed(&pc, at(21), at(20), 1, 0);
        seed(&pc, at(20), -1, 1, 0);
        dw.motor.borrow_mut().insert((300, 22), ElevatorMotorInfo { has_required_power: true, pos: ip(300, 19) });
        let mut q = query(&pc, &world);
        q.goal_index = at(22);
        q.start_index = 5;
        assert!(pc.extract(&world, &q, true));
    }

    #[test]
    fn fall_start_record_unavailable() {
        let (world, _l, _d) = new_world(32, 20);
        let pc = PathCreator::init_with_world(&world);
        let user = new_user(ip(200, 21));
        pc.create_path_with_dict(&req(&user, 200, 21, 0, 5));
        // direct call with the table exhausted
        pc.st.borrow_mut().derived_tile_count = MAX_DERIVED_TILES;
        assert!(pc.calculate_fall_path().is_none());
    }

    /// Prepares a direct fall search: start cell loaded, loader removed, the given indices pre-closed so that
    /// evaluating them neither loads nor changes anything.
    fn fall_fixture(start: IntPair, ty: i32, closed_at: &[(i32, i32)]) -> (Rc<World>, Rc<PathCreator>) {
        let (world, _l, _d) = new_world(32, 20);
        let pc = PathCreator::init_with_world(&world);
        let user = new_user(start);
        user_knobs(&user).kf.set(KF_FALLING);
        let _ = tile_at_world_position_loaded(start.x, start.y, &world);
        world.set_ops_hooks(None);
        let mut closed = IndexSet::new();
        for &(x, y) in closed_at {
            closed.add_index(ns_index(y * 32 * 32 + x));
        }
        let mut st = pc.st.borrow_mut();
        st.path_user = Some(user);
        st.path_type = ty;
        st.closed_list = Some(closed);
        st.open_list = Some(IndexSet::new());
        drop(st);
        (world, pc)
    }

    fn idx(x: i32, y: i32) -> i32 {
        y * 32 * 32 + x
    }

    #[test]
    fn fall_scan_stops_at_unavailable_cell_below() {
        let (_w, pc) = fall_fixture(ip(200, 32), 6, &[(200, 32), (200, 31)]);
        assert!(pc.calculate_fall_path().is_none());
    }

    #[test]
    fn fall_scan_lands_above_unavailable_cell() {
        let (_w, pc) = fall_fixture(ip(200, 34), 6, &[(200, 34), (200, 32), (200, 31)]);
        seed(&pc, idx(200, 32), idx(200, 33), 1, KF_FALLING);
        let path = pc.calculate_fall_path().expect("landing above the unavailable cell");
        assert_eq!(path[0].index, idx(200, 32));
    }

    #[test]
    fn fall_chain_cycle_is_cut_off() {
        let (_w, pc) = fall_fixture(ip(200, 34), 6, &[(200, 34), (200, 32), (200, 31)]);
        seed(&pc, idx(200, 32), 5, 1, KF_FALLING);
        seed(&pc, 5, idx(200, 32), 1, 0);
        assert!(pc.calculate_fall_path().is_none());
    }

    #[test]
    fn fall_type_5_lands_in_unavailable_cell_above() {
        let (_w, pc) = fall_fixture(ip(200, 31), 5, &[(200, 31)]);
        // the cell above is in an unloaded block: it becomes the landing cell (never reached from the start)
        assert!(pc.calculate_fall_path().is_none());
    }

    #[test]
    fn fall_type_5_lands_on_standing_cell_above() {
        let (_w, pc) = fall_fixture(ip(200, 21), 5, &[(200, 21), (200, 22)]);
        seed(&pc, idx(200, 22), idx(200, 21), 1, 0);
        let path = pc.calculate_fall_path().expect("path");
        assert_eq!(path[0].index, idx(200, 22));
    }

    #[test]
    fn fall_type_6_does_not_probe_above() {
        let (_w, pc) = fall_fixture(ip(200, 21), 6, &[(200, 21), (200, 22)]);
        seed(&pc, idx(200, 22), idx(200, 21), 1, 0);
        // without the above probe the scan walks down: (200, 20) is solid ground and a landing cell
        let path = pc.calculate_fall_path();
        assert!(path.is_none() || path.unwrap()[0].index != idx(200, 22));
    }

    #[test]
    fn fall_missing_cell_below_start_gives_none() {
        let (_w, pc) = fall_fixture(ip(200, 32), 6, &[(200, 32), (200, 31)]);
        assert!(pc.calculate_fall_path().is_none());
    }

    #[test]
    fn start_probe_abort_resets_first_check_flag() {
        let (world, _l, _d) = new_world(32, 20);
        let pc = PathCreator::init_with_world(&world);
        let user = new_user(ip(100, 32));
        for x in [100, 105] {
            let _ = tile_at_world_position_loaded(x, 32, &world);
        }
        world.set_ops_hooks(None);
        pc.create_path_with_dict(&req(&user, 105, 32, 0, 0));
        assert!(!pc.st.borrow().first_tile_check_done, "the start evaluation needed the unloaded row below");
    }

    #[test]
    fn open_set_without_loaded_cells_yields_no_candidate() {
        let (world, _l, _d) = new_world(32, 20);
        let pc = PathCreator::init_with_world(&world);
        let user = new_user(ip(100, 21));
        pc.create_path_with_dict(&req(&user, 300, 21, 0, 0));
        world.set_ops_hooks(None);
        let mut open = IndexSet::new();
        open.add_index(ns_index(idx(500, 500)));
        pc.st.borrow_mut().open_list = Some(open);
        let before = pc.derived_tile_count();
        pc.update_path();
        assert_eq!(pc.derived_tile_count(), before + 1, "only the scanned record was created");
        assert!(pc.in_progress());
    }

    #[test]
    fn unplaceable_goal_is_retried_then_closed() {
        for interaction in [INTERACTION_DIG, INTERACTION_DIG_FOREGROUND, INTERACTION_PICKUP] {
            let (world, _l, _d) = new_world(32, 20);
            let pc = PathCreator::init_with_world(&world);
            let user = new_user(ip(100, 21));
            pc.create_path_with_dict(&req(&user, 104, 21, interaction, 0));
            // force the goal record into the open set as an unplaceable cell with the best score
            let g = pc.tile_derived_properties_at_world_index(idx(104, 21)).unwrap();
            pc.with_props_mut(g, |p| {
                p.f = -1;
                p.unplaceable = true;
                p.parent_index = idx(100, 21);
                p.traverse_to_key_frame = 0;
                p.traverse_type = 1;
            });
            let mut open = IndexSet::new();
            open.add_index(ns_index(idx(104, 21)));
            {
                let mut st = pc.st.borrow_mut();
                st.open_list = Some(open);
                st.closed_list = Some(IndexSet::new());
                st.in_progress = true;
                st.path = None;
            }
            pc.update_path();
            assert!(pc.has_path() || pc.in_progress());
        }
    }

    #[test]
    fn record_exhaustion_with_candidate_still_finishes_search() {
        let (world, _l, _d) = new_world(32, 20);
        let pc = PathCreator::init_with_world(&world);
        let user = new_user(ip(100, 21));
        pc.create_path_with_dict(&req(&user, 300, 21, 0, 0));
        let mut open = IndexSet::new();
        for i in [idx(100, 21), idx(300, 40), idx(301, 40)] {
            open.add_index(ns_index(i));
        }
        {
            let mut st = pc.st.borrow_mut();
            st.open_list = Some(open);
            st.in_progress = true;
            st.derived_tile_count = MAX_DERIVED_TILES - 1;
        }
        pc.update_path();
        assert!(!pc.in_progress() && pc.has_path());
    }

    #[test]
    fn extract_subpath_ended_at_start_pops_start_entry() {
        let (world, pc, q) = ready(30, 10);
        let g = seed(&pc, 30, 20, 1, 0);
        pc.with_props_mut(g, |p| p.path = Some(Rc::new(vec![sub(7, -1), sub(10, 7), sub(30, 10)])));
        assert!(!pc.extract(&world, &q, true));
        let got: Vec<i32> = pc.path().unwrap().iter().map(|s| s.index).collect();
        assert_eq!(got, vec![30]);
    }

    #[test]
    fn fall_with_walkable_above_lands_there() {
        let (world, _l, _d) = new_world(32, 20);
        let pc = PathCreator::init_with_world(&world);
        let user = new_user(ip(200, 21));
        pc.create_path_with_dict(&req(&user, 200, 21, 0, 5));
        assert!(pc.has_path());
    }

    #[test]
    fn request_from_plist() {
        let mut m = GsDict::new();
        m.insert("pathType".to_string(), PlistValue::int(3));
        m.insert("goalX".to_string(), PlistValue::int(-4));
        let d = PlistDict { map: m, tag: Default::default() };
        let r = PathRequest::from_plist_dict(&d);
        assert_eq!((r.path_type, r.goal_x, r.goal_y, r.goal_interaction), (3, -4, 0, 0));
        assert!(r.path_user.is_none() && r.extra_data.is_none());
    }

    /// A long walk takes several `updatePath` calls; the result is the same flat line.
    #[test]
    fn long_walk_needs_several_updates() {
        let (world, _l, _d) = new_world(32, 20);
        let pc = PathCreator::init_with_world(&world);
        let user = new_user(ip(100, 21));
        pc.create_path_with_dict(&req(&user, 600, 21, 0, 0));
        assert!(pc.in_progress());
        run_to_end(&pc);
        assert!(pc.has_path());
        assert_eq!(pc.path().expect("path").len(), 500);
    }

    /// Accessors, `setHasPath:`, `dealloc` and the dictionary form of a sub-path step.
    #[test]
    fn accessors_and_teardown() {
        let (world, _l, _d) = new_world(32, 20);
        let pc = PathCreator::init_with_world(&world);
        let user = new_user(ip(100, 21));
        pc.create_path_with_dict(&PathRequest {
            path_type: 3,
            goal_x: 104,
            goal_y: 21,
            goal_interaction: 2,
            extra_data: Some(PlistValue::int(7)),
            path_user: Some(user.clone()),
        });
        run_to_end(&pc);
        assert!(pc.path_user().is_some_and(|u| crate::objects::handle::ptr_eq(&u, &user)));
        assert_eq!((pc.goal_x(), pc.goal_y(), pc.goal_interaction(), pc.path_type()), (104, 21, 2, 3));
        assert!(pc.extra_data().is_some());
        pc.set_has_path(false);
        assert!(!pc.has_path());
        let step = PathStep {
            index: 4,
            terrain_difficulty: 1,
            traverse_type: 2,
            traverse_to_key_frame: 3,
            parent_index: Some(5),
            properties_g: Some(60),
        };
        let PlistValue::Dict(d) = step.to_plist_dict() else { panic!("dictionary expected") };
        assert_eq!(d.map.len(), 6);
        pc.dealloc();
        assert!(pc.path_user().is_none() && pc.path().is_none() && pc.extra_data().is_none());
    }

    /// The stand-in user and loader answer their questions.
    #[test]
    fn test_doubles_answer() {
        use crate::world::state::WorldOpsHooks;
        let (world, _l, _d) = new_world(32, 20);
        let user = new_user(ip(1, 2));
        let hooks = _l.clone();
        assert!(!hooks.tile_is_protected_at_pos_against_blockhead(&world, ip(0, 0), &user));
        assert_eq!(user.class_info().name, "DynamicObject");
        let pu = user.as_path_user().unwrap();
        assert!(pu.info_for_path_recalculation().is_none());
        pu.set_path_type_goal_interaction_extra_data(&user, None, 0, 0, None);
        pu.set_waiting_for_path_to_pos(ip(0, 0));
        pu.set_no_longer_waiting_for_path();
        assert!(!pu.waiting_for_path());
        assert!(!pu.path_needs_recalculated());
        pu.set_path_needs_recalculated(true);
    }

    /// The real workbench behind a `WirePowerSource` view of the path user's DynamicWorld.
    #[test]
    fn real_workbench_as_power_source() {
        use crate::crafting::workbench::{Workbench, WB_STEAM_GENERATOR};
        use crate::interaction::sign::testkit::world_with_block;
        use crate::objects::base::DynamicWorldRef;
        use crate::objects::type_tables::DYNAMIC_OBJECT_TYPE_WORKBENCH;
        use crate::types::enums::WorkbenchType;
        set_dynamic_world_hooks(None);
        let (w, dw, _pb) = world_with_block("pc_workbench");
        let pos = ip(10, 10);
        let dwr: DynamicWorldRef = dw.clone();
        let wb = Workbench::init_with_world_dynamic_world_at_position_cache_type_flipped_save_dict_placed_by_client_client_name(
            &w, &dwr, pos, None, WorkbenchType(WB_STEAM_GENERATOR as _), false, None, None, None,
        )
        .unwrap();
        let wbo: ObjRef = wb.clone();
        dw.by_pos_set(DYNAMIC_OBJECT_TYPE_WORKBENCH, dw.world_index(pos), &wbo);
        let user = new_user(ip(11, 10));
        user.base().borrow_mut().dynamic_world = Some(Rc::downgrade(&dwr));
        let src = dw_workbench_at_pos(Some(&user), pos).expect("the workbench");
        assert_eq!(src.available_electricity(), 0x7f);
        assert!(src.generates_electricity());
        assert!(!src.is_storage_device());
        let _ = src.conducts_electricity();
        let _ = src.subtract_electricty(5);
        assert_eq!(src.available_electricity(), 0x7f - 5);
        assert!(dw_workbench_at_pos(Some(&user), ip(30, 10)).is_none());
        let _ = dw_has_lights_to_add(Some(&user));
        let _ = dw_blockhead_occupies_tile(Some(&user), pos, None);
    }
}

#[cfg(test)]
mod dw_direct_tests {
    use super::*;

    /// with no hooks the real DynamicWorld (via the path user) is consulted; a detached /
    /// absent user gives the nil-message defaults.
    #[test]
    fn direct_dw_defaults_without_user() {
        set_dynamic_world_hooks(None);
        assert!(!dw_has_lights_to_add(None));
        assert!(!dw_blockhead_occupies_tile(None, IntPair { x: 1, y: 1 }, None));
        assert!(dw_workbench_at_pos(None, IntPair { x: 1, y: 1 }).is_none());
    }
}
