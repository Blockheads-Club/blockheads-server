//! Tile-entry test ("can enter"): may a mover standing in an origin cell (in some pose) step into a destination
//! cell, with which move kind, ending in which pose and at what cost.
//!
//! Structure: [`check_can_enter_tile`] performs admission ([`admit`]) and then builds a [`Scene`]. The scene knows
//! the destination, the origin and the mover, and hands out cells by *offset from the destination*. Cells other than
//! the destination are fetched with [`Scene::req`], which goes through the world's loading tile accessor; the order
//! of those requests is observable (block loading consumes the shared random stream) so every movement rule below
//! requests its cells in exactly the order of the behavioural specification, and only when it needs them.
//!
//! An absent cell aborts evaluation with [`NoTile`]; the verdict record is left as it stands (so provisional
//! results survive) and the caller sets the missing-tile flag.
//!
//! Movement rules are small functions, one per movement shape, writing into the verdict with [`put`].

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use crate::objects::handle::ObjRef;
use crate::types::enums::{TileTraverseKeyFrameType, TileTraverseType};
use crate::types::structs::{IntPair, PathTestResult};
use crate::world::helpers::{
    tile_contains_usable_door, tile_contains_usable_elevator, tile_contains_usable_trap_door, tile_is_air,
    tile_is_air_or_snow, tile_is_climable, tile_is_solid, tile_is_swimmable, tile_is_water,
};
use crate::world::state::{MacroTileRef, TileRef, World};
use crate::world::tiles::{
    load_block_at_macro_pos, macro_pos_for_world_pos, macro_tile_at_macro_postion, tile_at_world_position_loaded,
    world_index_at_world_position,
};

use super::path_creator::{dw_has_lights_to_add, user_pos, wrap_dx, DerivedTileProperties};

// Move kinds (TileTraverseType).
const TT_NONE: u32 = TileTraverseType::TRAVERSE_NONE.0 as u32;
const TT_WALK: u32 = TileTraverseType::TRAVERSE_WALK.0 as u32;
const TT_SWIM: u32 = TileTraverseType::TRAVERSE_SWIM.0 as u32;
const TT_CRAWL: u32 = TileTraverseType::TRAVERSE_CRAWL.0 as u32;
const TT_CLIMB: u32 = TileTraverseType::TRAVERSE_CLIMB.0 as u32;
const TT_STAND_TO_HANG_JUMP: u32 = TileTraverseType::TRAVERSE_STAND_TO_HANG_JUMP.0 as u32;
const TT_HANG_TO_STAND_JUMP: u32 = TileTraverseType::TRAVERSE_HANG_TO_STAND_JUMP.0 as u32;
const TT_HANG_TO_HANG_JUMP: u32 = TileTraverseType::TRAVERSE_HANG_TO_HANG_JUMP.0 as u32;
const TT_STAND_TO_HANG_DOUBLE_JUMP: u32 = TileTraverseType::TRAVERSE_STAND_TO_HANG_DOUBLE_JUMP.0 as u32;
const TT_STAND_TO_MONKEY_HANG_JUMP: u32 = TileTraverseType::TRAVERSE_STAND_TO_MONKEY_HANG_JUMP.0 as u32;
const TT_MONKEY_HANG_TO_STAND_JUMP: u32 = TileTraverseType::TRAVERSE_MONKEY_HANG_TO_STAND_JUMP.0 as u32;
const TT_CROUCH_UP: u32 = TileTraverseType::TRAVERSE_CROUCH_UP.0 as u32;
const TT_CROUCH_DOWN: u32 = TileTraverseType::TRAVERSE_CROUCH_DOWN.0 as u32;
const TT_CROUCH_CRAWL_UP: u32 = TileTraverseType::TRAVERSE_CROUCH_CRAWL_UP.0 as u32;
const TT_CROUCH_CRAWL_DOWN: u32 = TileTraverseType::TRAVERSE_CROUCH_CRAWL_DOWN.0 as u32;
const TT_FALL: u32 = TileTraverseType::TRAVERSE_FALL.0 as u32;
const TT_MONKEY_SWING: u32 = TileTraverseType::TRAVERSE_MONKEY_SWING.0 as u32;
const TT_CLIMB_DOWN_TO_MONKEY_SWING: u32 = TileTraverseType::TRAVERSE_CLIMB_DOWN_TO_MONKEY_SWING.0 as u32;
const TT_MONKEY_SWING_UP_TO_CLIMB: u32 = TileTraverseType::TRAVERSE_MONKEY_SWING_UP_TO_CLIMB.0 as u32;
const TT_HANG_TO_MONKEY_SWING_DIAGONAL_JUMP_UP: u32 =
    TileTraverseType::TRAVERSE_HANG_TO_MONKEY_SWING_DIAGONAL_JUMP_UP.0 as u32;
const TT_MONKEY_SWING_TO_HANG_DIAGONAL_JUMP_DOWN: u32 =
    TileTraverseType::TRAVERSE_MONKEY_SWING_TO_HANG_DIAGONAL_JUMP_DOWN.0 as u32;
const TT_JUMP_OVER_GAP: u32 = TileTraverseType::TRAVERSE_JUMP_OVER_GAP.0 as u32;
const TT_INACCESSIBLE: u32 = TileTraverseType::TRAVERSE_INACCESSIBLE.0 as u32;
const TT_STAIRS: u32 = TileTraverseType::TRAVERSE_STAIRS.0 as u32;
const TT_ELEVATOR: u32 = TileTraverseType::TRAVERSE_ELEVATOR.0 as u32;
const TT_FLY: u32 = TileTraverseType::TRAVERSE_FLY.0 as u32;

// Poses (TileTraverseKeyFrameType).
const KF_STAND: u32 = TileTraverseKeyFrameType::TRAVERSE_KF_STAND.0 as u32;
const KF_BOB: u32 = TileTraverseKeyFrameType::TRAVERSE_KF_BOB.0 as u32;
const KF_CRAWL: u32 = TileTraverseKeyFrameType::TRAVERSE_KF_CRAWL.0 as u32;
const KF_HANG_BACK: u32 = TileTraverseKeyFrameType::TRAVERSE_KF_HANG_BACK.0 as u32;
const KF_HANG_RIGHT: u32 = TileTraverseKeyFrameType::TRAVERSE_KF_HANG_RIGHT.0 as u32;
const KF_HANG_LEFT: u32 = TileTraverseKeyFrameType::TRAVERSE_KF_HANG_LEFT.0 as u32;
const KF_MONKEY_HANG: u32 = TileTraverseKeyFrameType::TRAVERSE_KF_MONKEY_HANG.0 as u32;
const KF_FALLING: u32 = TileTraverseKeyFrameType::TRAVERSE_KF_FALLING.0 as u32;
const KF_INACCESSIBLE: u32 = TileTraverseKeyFrameType::TRAVERSE_KF_INACCESSIBLE.0 as u32;
const KF_STAIRS: u32 = TileTraverseKeyFrameType::TRAVERSE_KF_STAIRS.0 as u32;
const KF_STAND_ELEVATOR: u32 = TileTraverseKeyFrameType::TRAVERSE_KF_STAND_ELEVATOR.0 as u32;
const KF_FLY: u32 = TileTraverseKeyFrameType::TRAVERSE_KF_FLY.0 as u32;

/// Contents codes.
const C_STAIRS: u8 = 0x65;
const C_ELEVATOR_SHAFT: u8 = 0x67;
const C_CACTUS: u8 = 0x2b;
const C_LADDER: u8 = 0x42;

/// Cost of a refused record.
const REFUSED_COST: i32 = 999_999;
/// A fall is only accepted by a non-fall search while the resulting cost stays below this.
const FALL_COST_LIMIT: i32 = 80;
/// An origin that fell with at least this much cost is "too fast" to grab or stop.
const TOO_FAST_ABOVE: i32 = 99;

/// The two fields of the origin cell's derived properties the test ever consults: its pose and the cost with which
/// the origin was reached.
#[derive(Clone, Copy, Default, Debug)]
pub struct FromProps {
    pub terrain_difficulty: i32,
    pub traverse_to_key_frame: i32,
}

impl From<&DerivedTileProperties> for FromProps {
    fn from(p: &DerivedTileProperties) -> FromProps {
        FromProps { terrain_difficulty: p.terrain_difficulty, traverse_to_key_frame: p.traverse_to_key_frame }
    }
}

/// The REFUSED record.
fn refused() -> PathTestResult {
    PathTestResult {
        traverse_type: TT_INACCESSIBLE,
        traverse_to_key_frame: KF_INACCESSIBLE,
        terrain_difficulty: REFUSED_COST,
        failed_due_to_no_tile: 0,
        gap_d: [0; 3],
    }
}

/// Write `KIND / POSE / cost`.
fn put(r: &mut PathTestResult, kind: u32, pose: u32, cost: i32) {
    r.traverse_type = kind;
    r.traverse_to_key_frame = pose;
    r.terrain_difficulty = cost;
}

/// A cell, as an offset from the destination.
type Cell = (i32, i32);
const HERE: Cell = (0, 0);
const UP1: Cell = (0, 1);
const UP2: Cell = (0, 2);
const DN1: Cell = (0, -1);
const DN2: Cell = (0, -2);
/// Left / right neighbours of the destination (vertical moves).
const WEST: Cell = (-1, 0);
const EAST: Cell = (1, 0);

/// Evaluation stopped because a requested cell is absent.
struct NoTile;
type Flow = Result<(), NoTile>;

/// The pose a mover takes while hanging on a wall at horizontal offset `dx` from the destination.
fn wall_pose(dx: i32) -> u32 {
    if dx > 0 {
        KF_HANG_RIGHT
    } else {
        KF_HANG_LEFT
    }
}

/// Everything a movement rule needs.
struct Scene<'a> {
    origin: Option<TileRef>,
    /// origin pose
    pose: u32,
    /// origin cost
    pc: i32,
    ox: i32,
    oy: i32,
    x0: i32,
    y0: i32,
    mover: Option<&'a ObjRef>,
    world: &'a Rc<World>,
    fall_search: bool,
    blacklist: &'a [IntPair],
    /// wrapped horizontal distance origin -> destination
    xd: i32,
    flies: bool,
    /// cells requested so far (the destination is pre-seeded)
    seen: RefCell<HashMap<Cell, Option<TileRef>>>,
}

impl Scene<'_> {
    fn abs(&self, c: Cell) -> (i32, i32) {
        (self.x0.wrapping_add(c.0), self.y0.wrapping_add(c.1))
    }

    fn pos(&self, c: Cell) -> IntPair {
        let (x, y) = self.abs(c);
        IntPair { x, y }
    }

    /// Request a cell from the world (loading its block if needed); absent -> abort.
    fn req(&self, c: Cell) -> Flow {
        let (x, y) = self.abs(c);
        let t = tile_at_world_position_loaded(x, y, self.world);
        self.seen.borrow_mut().insert(c, t);
        t.map(|_| ()).ok_or(NoTile)
    }

    /// The cell as obtained by the latest request.
    fn at(&self, c: Cell) -> Option<TileRef> {
        self.seen.borrow().get(&c).copied().flatten()
    }

    fn code(&self, c: Cell) -> u8 {
        self.at(c).map_or(0, |t| t.contents())
    }

    // ---- pose groups
    fn hanging(&self) -> bool {
        matches!(self.pose, KF_HANG_BACK | KF_HANG_RIGHT | KF_HANG_LEFT)
    }
    fn side_hanging(&self) -> bool {
        matches!(self.pose, KF_HANG_RIGHT | KF_HANG_LEFT)
    }
    fn too_fast(&self) -> bool {
        self.pose == KF_FALLING && self.pc > TOO_FAST_ABOVE
    }
    fn fall_base(&self) -> i32 {
        if self.pose == KF_FALLING {
            self.pc
        } else {
            10
        }
    }
    fn can_crawl(&self) -> bool {
        self.mover.and_then(|u| u.as_path_user()).is_some_and(|p| p.can_crawl())
    }

    // ---- primitive predicates
    fn open(&self, c: Cell) -> bool {
        tile_is_air_or_snow(self.at(c))
    }
    fn wet(&self, c: Cell) -> bool {
        tile_is_water(self.at(c))
    }
    fn solid(&self, c: Cell) -> bool {
        tile_is_solid(self.at(c))
    }
    /// `Deep(x | y)`
    fn deep(&self, x: Cell, y: Cell) -> bool {
        tile_is_swimmable(self.at(x), self.at(y))
    }
    /// `Shallow(x | y)`
    fn shallow(&self, x: Cell, y: Cell) -> bool {
        self.wet(x) && !self.deep(x, y)
    }
    fn shaft(&self, c: Cell) -> bool {
        self.code(c) == C_ELEVATOR_SHAFT
    }
    fn stairs(&self, c: Cell) -> bool {
        self.code(c) == C_STAIRS
    }
    fn ladder(&self, c: Cell) -> bool {
        self.at(c).is_some_and(|t| t.foreground_contents() == C_LADDER)
    }
    fn empty_or_stairs(&self, c: Cell) -> bool {
        matches!(self.code(c), 0 | C_STAIRS)
    }
    fn origin_stairs(&self) -> bool {
        self.origin.is_some_and(|t| t.contents() == C_STAIRS)
    }
    /// `Door(rec @ pos)`: the record of one cell with the position of another.
    fn door_rp(&self, rec: Cell, pos: Cell) -> bool {
        tile_contains_usable_door(self.at(rec), self.pos(pos), self.world, self.mover)
    }
    fn door(&self, c: Cell) -> bool {
        self.door_rp(c, c)
    }
    fn hatch(&self, c: Cell) -> bool {
        tile_contains_usable_trap_door(self.at(c), self.pos(c), self.world, self.mover)
    }
    /// `Lift(c)`; the helper itself requests the cell above `c` when `c` is a shaft.
    fn lift(&self, c: Cell) -> bool {
        tile_contains_usable_elevator(self.at(c), self.pos(c), self.world, self.mover, self.blacklist)
    }
    fn grip(&self, c: Cell) -> bool {
        tile_is_climable(self.at(c), self.pos(c), self.world, self.mover)
    }

    // ---- composite conditions
    fn headroom(&self, c: Cell) -> bool {
        !self.solid(c) || self.door(c)
    }
    fn pass_d(&self, x: Cell, y: Cell) -> bool {
        self.open(x) || self.shallow(x, y) || self.door(x)
    }
    fn pass_h(&self, x: Cell, y: Cell) -> bool {
        self.open(x) || self.shallow(x, y) || self.hatch(x)
    }
    fn pass_dh(&self, x: Cell, y: Cell) -> bool {
        self.open(x) || self.shallow(x, y) || self.door(x) || self.hatch(x)
    }
    fn pass_surface(&self, x: Cell, y: Cell) -> bool {
        self.open(x) || (self.wet(x) && !self.wet(y)) || self.door(x)
    }
    fn shaft_lift(&self, c: Cell) -> bool {
        self.shaft(c) && self.lift(c)
    }
    /// `Shaft(here) and pose != FALLING and Lift(here)` (vertical rules)
    fn lift_here(&self) -> bool {
        self.shaft(HERE) && self.pose != KF_FALLING && self.lift(HERE)
    }
    /// A grip that ends the evaluation (a cactus-only grip does not).
    fn firm_grip(&self, c: Cell) -> bool {
        self.grip(c) && (self.code(c) != C_CACTUS || self.ladder(c))
    }
    /// Fall accepted: a fall search takes any fall, other searches only cheap ones.
    fn fall_ok(&self) -> bool {
        self.fall_search || self.fall_base() + 10 < FALL_COST_LIMIT
    }
    /// Origin gate shared by side moves and gap jumps: an elevator rider can only leave from a non-solid origin
    /// (or through a usable door).
    fn elevator_origin_ok(&self) -> bool {
        self.pose != KF_STAND_ELEVATOR
            || !tile_is_solid(self.origin)
            || tile_contains_usable_door(self.origin, IntPair { x: self.ox, y: self.oy }, self.world, self.mover)
    }
}

/// `check_can_enter_tile`: `tile` is the destination cell content (may be absent), `from_tile` the origin's; `from`
/// carries the origin's pose and cost.
#[allow(clippy::too_many_arguments)]
pub fn check_can_enter_tile(
    tile: Option<TileRef>,
    from_tile: Option<TileRef>,
    from: FromProps,
    from_x: i32,
    from_y: i32,
    to_x: i32,
    to_y: i32,
    path_user: Option<&ObjRef>,
    world: &Rc<World>,
    is_fall_path: bool,
    allows_water_tiles: bool,
    elevator_blacklist: &[IntPair],
) -> PathTestResult {
    let mut r = refused();
    let pose = from.traverse_to_key_frame as u32;
    let admitted = admit(tile, pose, from_y, to_x, to_y, path_user, world, is_fall_path, allows_water_tiles);
    let flow = match admitted {
        Err(NoTile) => Err(NoTile),
        Ok(None) => Ok(()),
        Ok(Some(dest)) => {
            let scene = Scene {
                origin: from_tile,
                pose,
                pc: from.terrain_difficulty,
                ox: from_x,
                oy: from_y,
                x0: to_x,
                y0: to_y,
                mover: path_user,
                world,
                fall_search: is_fall_path,
                blacklist: elevator_blacklist,
                xd: wrap_dx(to_x.wrapping_sub(from_x), world.world_width_macro()),
                flies: path_user.and_then(|u| u.as_path_user()).is_some_and(|p| p.can_fly()),
                seen: RefCell::new(HashMap::from([(HERE, Some(dest))])),
            };
            route(&scene, &mut r)
        }
    };
    if flow.is_err() {
        r.failed_due_to_no_tile = 1;
    }
    r
}

/// Does the cell emit or receive any light?
fn cell_has_light(t: TileRef) -> bool {
    t.sun_light() != 0 || t.artificial_light_r() != 0 || t.artificial_light_g() != 0 || t.artificial_light_b() != 0
}

/// Admission (spec 5.1 steps 2-9). `Ok(Some(dest))` = go on to the movement rules, `Ok(None)` = refused.
#[allow(clippy::too_many_arguments)]
fn admit(
    tile: Option<TileRef>,
    pose: u32,
    oy: i32,
    x0: i32,
    y0: i32,
    mover: Option<&ObjRef>,
    world: &Rc<World>,
    fall_search: bool,
    water_allowed: bool,
) -> Result<Option<TileRef>, NoTile> {
    if y0 > 1019 && oy < y0 {
        return Ok(None);
    }
    let dest = tile.ok_or(NoTile)?;
    if world_index_at_world_position(x0, y0, world) < 0 {
        return Ok(None);
    }
    let full_water = dest.type_index() == 3 && dest.partial_content_left() == 0xff;
    if !water_allowed && full_water && !tile_is_air(tile_at_world_position_loaded(x0, y0.wrapping_add(1), world)) {
        return Ok(None);
    }

    let lift_exit = pose == KF_STAND_ELEVATOR && dest.contents() == C_ELEVATOR_SHAFT;
    let mut lit = cell_has_light(dest) || lift_exit;
    if !lit {
        // Close enough to the mover counts as lit.
        let m = user_pos(mover);
        let dx = wrap_dx(m.x.wrapping_sub(x0), world.world_width_macro());
        let dy = m.y.wrapping_sub(y0);
        lit = dx.wrapping_mul(dx).wrapping_add(dy.wrapping_mul(dy)) <= 16;
    }
    if !lit && world.client().is_none() {
        let pending = settle_lights(world, mover, IntPair { x: x0, y: y0 });
        lit = cell_has_light(dest) || lift_exit;
        if !lit && pending {
            return Err(NoTile);
        }
    }
    if !fall_search {
        let seen_by_self = mover
            .and_then(|u| u.as_path_user())
            .is_some_and(|p| p.tile_is_lit_for_self_at_pos(&dest.get(), IntPair { x: x0, y: y0 }));
        if !(seen_by_self && lit) {
            return Ok(None);
        }
    }
    Ok(Some(dest))
}

/// Light settling on the authoritative world: make sure the destination's macro block and its eight neighbours have
/// their dynamic objects (and lights) loaded. Returns whether lights are still waiting to be added.
///
/// Policy for neighbour rows outside the world (only possible for blocks in the top or bottom macro row): the
/// original faults; here the row is skipped (after the `ub_null_tile_deref` parity hook, a no-op unless the
/// `faithful-crashes` feature is on), deterministically.
fn settle_lights(world: &Rc<World>, mover: Option<&ObjRef>, dest: IntPair) -> bool {
    let tiles = world.macro_tiles();
    let hooks = world.ops_hooks();
    let ensure_dynamic = |mt: &MacroTileRef| {
        if let Some(h) = hooks.as_ref() {
            h.load_dynamic_objects_if_not_already_loaded_for_macro_tile_include_surface_blocks(world, mt, true);
        }
    };
    let home = macro_pos_for_world_pos(dest, world);
    if let Some(mt) = macro_tile_at_macro_postion(home.x, home.y, &tiles, world) {
        ensure_dynamic(&mt);
    }
    let mut pending = dw_has_lights_to_add(mover);
    if pending {
        return true;
    }
    for dy in -1..=1i32 {
        for dx in -1..=1i32 {
            if dx == 0 && dy == 0 {
                continue;
            }
            let (mx, my) = (home.x.wrapping_add(dx), home.y.wrapping_add(dy));
            let Some(mt) = macro_tile_at_macro_postion(mx, my, &tiles, world) else {
                crate::objects::base::ub_null_tile_deref();
                continue;
            };
            if mt.physical_block().is_none() {
                if load_block_at_macro_pos(IntPair { x: mx, y: my }, world).is_some() {
                    ensure_dynamic(&mt);
                    pending = true;
                }
            } else {
                ensure_dynamic(&mt);
                pending |= dw_has_lights_to_add(mover);
            }
        }
    }
    pending
}

/// Dispatch on the movement shape (spec 5.1 step 11).
fn route(s: &Scene, r: &mut PathTestResult) -> Flow {
    match s.xd.abs() {
        1 => side_move(s, r),
        2 => gap_jump(s, r),
        _ => vertical_move(s, r),
    }
}

// =====================================================================================================
// Side and diagonal moves (|xd| = 1)
// =====================================================================================================

fn side_move(s: &Scene, r: &mut PathTestResult) -> Flow {
    if s.pose == KF_FALLING && !s.flies {
        return Ok(());
    }
    if !s.elevator_origin_ok() {
        return Ok(());
    }
    // behind column offset (origin side) and ahead column offset
    let behind = -s.xd.signum();
    let ahead = s.xd.signum();
    match s.y0.wrapping_sub(s.oy) {
        -2 => drop_two(s, r, behind),
        2 => rise_two(s, r, behind),
        1 => step_up(s, r, behind, ahead),
        -1 => step_down(s, r, behind, ahead),
        0 => side_step(s, r, behind, ahead),
        _ => Ok(()),
    }
}

/// M1: drop two diagonally into a monkey hang.
fn drop_two(s: &Scene, r: &mut PathTestResult, b: i32) -> Flow {
    s.req(UP1)?;
    s.req(DN1)?;
    let enter = s.open(HERE) || s.shallow(HERE, DN1);
    if enter {
        s.req((b, 1))?;
        if !s.solid((b, 1)) {
            s.req((b, 0))?;
            if !s.solid((b, 0)) && !s.solid(UP1) {
                s.req(UP2)?;
            }
        }
    }
    if enter && !s.solid((b, 1)) && !s.solid((b, 0)) && !s.solid(UP1) && s.solid(UP2) {
        let pose = if s.solid(DN1) { KF_STAND } else { KF_MONKEY_HANG };
        put(r, TT_CLIMB_DOWN_TO_MONKEY_SWING, pose, 70);
    }
    Ok(())
}

/// M2: rise two diagonally from a monkey hang to a wall hang.
fn rise_two(s: &Scene, r: &mut PathTestResult, b: i32) -> Flow {
    if s.pose == KF_STAND_ELEVATOR {
        return Ok(());
    }
    s.req(UP1)?;
    s.req(DN1)?;
    let entry = (s.open(HERE) || s.hatch(HERE) || s.shallow(HERE, DN1)) && !s.solid(UP1) && !s.solid(DN1);
    if !entry {
        return Ok(());
    }
    s.req((b, -1))?;
    if s.solid((b, -1)) {
        return Ok(());
    }
    s.req(DN2)?;
    if s.solid(DN2) {
        return Ok(());
    }
    s.req((b, 0))?;
    if s.solid((b, 0)) {
        put(r, TT_MONKEY_SWING_UP_TO_CLIMB, wall_pose(b), 70);
    }
    Ok(())
}

/// Kind and cost shared by the "grab a wall / grip" results of the diagonal step up: standing jumps, same-pose
/// climbs (at `climb_cost`), anything else hang-to-hang (65).
fn hang_from(s: &Scene, r: &mut PathTestResult, pose: u32, climb_cost: i32) {
    if s.pose == KF_STAND {
        r.traverse_type = TT_STAND_TO_HANG_JUMP;
    } else if s.pose == pose {
        r.traverse_type = TT_CLIMB;
        r.terrain_difficulty = climb_cost;
    } else {
        r.traverse_type = TT_HANG_TO_HANG_JUMP;
        r.terrain_difficulty = 65;
    }
}

/// M3: step up diagonally.
fn step_up(s: &Scene, r: &mut PathTestResult, b: i32, a: i32) -> Flow {
    s.req((b, 0))?;
    s.req(UP1)?;
    if s.solid((b, 0)) {
        return Ok(());
    }
    s.req(DN1)?;
    if s.flies && s.open(HERE) && s.open(UP1) && s.open(DN1) {
        s.req((b, 1))?;
        if s.open((b, 1)) && !s.shaft_lift(HERE) {
            if s.hanging() {
                put(r, TT_HANG_TO_STAND_JUMP, KF_FLY, 42);
            } else {
                put(r, TT_FLY, KF_FLY, 6);
            }
            return Ok(());
        }
    }
    if !s.pass_d(HERE, DN1) {
        if s.deep(HERE, DN1) {
            put(r, TT_SWIM, KF_BOB, 42);
        }
        return Ok(());
    }
    s.req(UP2)?;
    if s.shaft_lift(HERE) {
        if s.hanging() {
            put(r, TT_HANG_TO_STAND_JUMP, KF_STAND_ELEVATOR, 42);
        } else {
            put(r, TT_WALK, KF_STAND_ELEVATOR, 21);
        }
        return Ok(());
    }
    let floor = s.solid(DN1) && !(s.door(DN1) && s.door(HERE));
    if floor {
        let ceiling = s.solid(UP1) && !s.door(UP1);
        if ceiling && !s.door(DN1) {
            if s.can_crawl() {
                put(r, TT_CROUCH_CRAWL_UP, KF_CRAWL, 42);
            }
            return Ok(());
        }
        if ceiling {
            return Ok(());
        }
        // Headroom(UP1) necessarily holds here (not solid, or a usable door).
        s.req((b, 1))?;
        if s.solid((b, 1)) {
            if !s.door(DN1) {
                put(r, TT_CROUCH_UP, KF_STAND, 42);
            }
        } else if s.hanging() {
            put(r, TT_HANG_TO_STAND_JUMP, KF_STAND, 42);
        } else if s.origin_stairs() {
            let pose = if s.stairs(HERE) { KF_STAIRS } else { KF_STAND };
            put(r, TT_STAIRS, pose, 12);
        } else {
            put(r, TT_WALK, KF_STAND, 16);
        }
        return Ok(());
    }

    // No floor.
    if s.stairs(HERE)
        && s.origin_stairs()
        && !s.solid(UP1)
        && s.empty_or_stairs(UP1)
        && !s.solid(UP2)
        && s.empty_or_stairs(UP2)
    {
        put(r, TT_STAIRS, KF_STAIRS, 12);
        return Ok(());
    }
    s.req(UP1)?;
    if s.solid(UP1) {
        return Ok(());
    }
    if s.grip(HERE) {
        r.traverse_to_key_frame = KF_HANG_BACK;
        r.terrain_difficulty = 42;
        r.traverse_type = match s.pose {
            KF_STAND => TT_STAND_TO_HANG_JUMP,
            KF_HANG_BACK => TT_CLIMB,
            _ => {
                r.terrain_difficulty = 65;
                TT_HANG_TO_HANG_JUMP
            }
        };
        if s.firm_grip(HERE) {
            return Ok(());
        }
    }
    s.req((a, 0))?;
    if s.solid((a, 0)) {
        r.terrain_difficulty = 42;
        r.traverse_to_key_frame = wall_pose(a);
        hang_from(s, r, wall_pose(a), 56);
        return Ok(());
    }
    s.req(UP2)?;
    if s.solid(UP2) {
        if s.side_hanging() {
            put(r, TT_HANG_TO_MONKEY_SWING_DIAGONAL_JUMP_UP, KF_MONKEY_HANG, 50);
        } else {
            put(r, TT_STAND_TO_MONKEY_HANG_JUMP, KF_MONKEY_HANG, 42);
        }
    } else if s.wet(HERE) {
        put(r, TT_SWIM, KF_BOB, 42);
    }
    Ok(())
}

/// The crouch / crawl-down / walk family used by several diagonal step-down results.
fn down_family(s: &Scene, r: &mut PathTestResult, walk_cost: i32) {
    if s.hanging() {
        put(r, TT_HANG_TO_STAND_JUMP, KF_STAND, 42);
    } else if s.pose == KF_CRAWL {
        put(r, TT_CROUCH_CRAWL_DOWN, KF_STAND, 42);
    } else {
        put(r, TT_WALK, KF_STAND, walk_cost);
    }
}

/// M4: step down diagonally.
fn step_down(s: &Scene, r: &mut PathTestResult, b: i32, a: i32) -> Flow {
    s.req(UP1)?;
    if s.flies && s.open(UP1) && s.open(HERE) {
        s.req((b, 0))?;
        s.req(UP2)?;
        if s.open((b, 0)) && s.open(UP2) && !s.shaft_lift(HERE) {
            if s.hanging() {
                put(r, TT_HANG_TO_STAND_JUMP, KF_FLY, 42);
            } else if s.pose == KF_CRAWL {
                put(r, TT_CROUCH_CRAWL_DOWN, KF_FLY, 42);
            } else {
                put(r, TT_FLY, KF_FLY, 6);
            }
            return Ok(());
        }
    }
    if !s.headroom(UP1) {
        return Ok(());
    }
    s.req(DN1)?;
    if !s.pass_dh(HERE, DN1) {
        if s.deep(HERE, DN1) && !s.solid(UP1) {
            put(r, TT_SWIM, KF_BOB, 42);
        }
        return Ok(());
    }
    if s.shaft_lift(HERE) {
        down_family(s, r, 21);
        r.traverse_to_key_frame = KF_STAND_ELEVATOR;
        return Ok(());
    }
    s.req(UP2)?;
    if s.stairs(HERE) && !s.solid(UP2) && !s.door(UP1) && s.empty_or_stairs(UP2) && s.empty_or_stairs(UP1) {
        put(r, TT_STAIRS, KF_STAIRS, 12);
        return Ok(());
    }
    let crouch = if s.pose == KF_CRAWL { TT_CROUCH_CRAWL_DOWN } else { TT_CROUCH_DOWN };

    // Standing on something.
    if s.solid(DN1) && !s.hatch(HERE) {
        let low_ceiling = s.solid(UP2) && !s.door_rp(UP2, UP1) && !s.door(UP1);
        if low_ceiling {
            put(r, crouch, KF_STAND, 42);
        } else if !s.door(UP1) {
            down_family(s, r, 16);
        } else {
            // A refused move that still reports the STAND pose.
            r.traverse_to_key_frame = KF_STAND;
        }
        return Ok(());
    }

    // Nothing to stand on.
    let low2 = s.solid(UP2) && !s.door(UP2);
    if s.grip(HERE) {
        r.traverse_to_key_frame = KF_HANG_BACK;
        if low2 {
            r.traverse_type = crouch;
            r.terrain_difficulty = 42;
        } else {
            r.terrain_difficulty = 56;
            r.traverse_type = match s.pose {
                KF_STAND => TT_STAND_TO_HANG_JUMP,
                KF_CRAWL => TT_CROUCH_CRAWL_DOWN,
                KF_HANG_BACK => TT_CLIMB,
                KF_MONKEY_HANG => TT_MONKEY_SWING_TO_HANG_DIAGONAL_JUMP_DOWN,
                _ => {
                    r.terrain_difficulty = 65;
                    TT_HANG_TO_HANG_JUMP
                }
            };
        }
        if s.firm_grip(HERE) {
            return Ok(());
        }
    }
    // The wall behind wins over the wall ahead.
    s.req((b, 0))?;
    let mut wall = Some(b);
    if !s.solid((b, 0)) {
        s.req((a, 0))?;
        wall = s.solid((a, 0)).then_some(a);
    }
    if let Some(w) = wall {
        let hang = wall_pose(w);
        r.terrain_difficulty = 42;
        r.traverse_to_key_frame = hang;
        match s.pose {
            KF_STAND => {
                s.req(UP2)?;
                let low2 = s.solid(UP2) && !s.door(UP2);
                r.traverse_type = if low2 { TT_CROUCH_DOWN } else { TT_STAND_TO_HANG_JUMP };
            }
            KF_CRAWL => r.traverse_type = TT_CROUCH_CRAWL_DOWN,
            KF_MONKEY_HANG => r.traverse_type = TT_MONKEY_SWING_TO_HANG_DIAGONAL_JUMP_DOWN,
            p if p == hang => {
                r.traverse_type = TT_CLIMB;
                r.terrain_difficulty = 56;
            }
            _ => {
                r.traverse_type = TT_HANG_TO_HANG_JUMP;
                r.terrain_difficulty = 65;
            }
        }
        return Ok(());
    }
    s.req(UP2)?;
    if s.solid(UP2) {
        put(r, TT_MONKEY_HANG_TO_STAND_JUMP, KF_MONKEY_HANG, 42);
    } else if s.deep(HERE, DN1) && !s.solid(UP1) {
        put(r, TT_SWIM, KF_BOB, 42);
    }
    Ok(())
}

/// Wall-hang result of the level side step.
fn level_wall_hang(s: &Scene, r: &mut PathTestResult, wall: i32) {
    let hang = wall_pose(wall);
    r.terrain_difficulty = 40;
    r.traverse_to_key_frame = hang;
    if s.pose == KF_STAND {
        r.traverse_type = TT_STAND_TO_HANG_JUMP;
    } else if s.pose == hang {
        r.traverse_type = TT_CLIMB;
    } else {
        r.terrain_difficulty = 50;
        r.traverse_type =
            if s.pose == KF_MONKEY_HANG { TT_MONKEY_SWING_TO_HANG_DIAGONAL_JUMP_DOWN } else { TT_HANG_TO_HANG_JUMP };
    }
}

/// Crawl / hang-to-stand / walk results of the level side step, in the given pose.
fn level_walk(s: &Scene, r: &mut PathTestResult, pose: u32) {
    if s.pose == KF_CRAWL {
        put(r, TT_CRAWL, pose, 30);
    } else if s.side_hanging() {
        put(r, TT_HANG_TO_STAND_JUMP, pose, 30);
    } else {
        put(r, TT_WALK, pose, 10);
    }
}

/// M5: side step on the same row (the "behind" cell is the origin itself).
fn side_step(s: &Scene, r: &mut PathTestResult, b: i32, a: i32) -> Flow {
    s.req(UP1)?;
    s.req(DN1)?;
    if s.flies && (s.open(HERE) || s.door(HERE)) && (s.open(UP1) || s.door(UP1)) && !s.shaft_lift(HERE) {
        if s.pose == KF_CRAWL {
            put(r, TT_CRAWL, KF_FLY, 30);
        } else if s.side_hanging() {
            put(r, TT_HANG_TO_STAND_JUMP, KF_FLY, 30);
        } else {
            put(r, TT_FLY, KF_FLY, 4);
        }
        return Ok(());
    }
    if !s.pass_d(HERE, DN1) {
        if s.deep(HERE, DN1) {
            put(r, TT_SWIM, KF_BOB, 30);
        }
        return Ok(());
    }
    if s.shaft_lift(HERE) {
        level_walk(s, r, KF_STAND_ELEVATOR);
        return Ok(());
    }
    if s.solid(DN1) && !s.door_rp(HERE, DN1) {
        if s.solid(UP1) && !s.door_rp(HERE, UP1) {
            if s.can_crawl() {
                put(r, TT_CRAWL, KF_CRAWL, 30);
            }
        } else {
            level_walk(s, r, KF_STAND);
        }
        return Ok(());
    }

    // No floor.
    if s.stairs(HERE) && !s.solid(UP1) && s.empty_or_stairs(UP1) {
        put(r, TT_STAIRS, KF_STAIRS, 12);
        return Ok(());
    }
    let headroom = s.headroom(UP1);
    if s.grip(HERE) && s.pose != KF_CRAWL {
        if headroom {
            r.terrain_difficulty = 40;
            r.traverse_to_key_frame = KF_HANG_BACK;
            r.traverse_type = match s.pose {
                KF_STAND if s.ladder(HERE) || s.shaft(HERE) => {
                    r.terrain_difficulty = 10;
                    TT_CLIMB
                }
                KF_STAND => TT_STAND_TO_HANG_JUMP,
                KF_HANG_BACK => TT_CLIMB,
                KF_MONKEY_HANG => TT_MONKEY_SWING_TO_HANG_DIAGONAL_JUMP_DOWN,
                _ => {
                    r.terrain_difficulty = 50;
                    TT_HANG_TO_HANG_JUMP
                }
            };
        }
        if s.firm_grip(HERE) {
            return Ok(());
        }
    }
    s.req((a, 0))?;
    if s.solid((a, 0)) && headroom {
        level_wall_hang(s, r, a);
        return Ok(());
    }
    s.req((b, 0))?;
    if s.solid((b, 0)) {
        level_wall_hang(s, r, b);
        return Ok(());
    }
    s.req(UP2)?;
    if headroom && s.solid(UP2) {
        r.terrain_difficulty = 50;
        r.traverse_to_key_frame = KF_MONKEY_HANG;
        r.traverse_type = if s.side_hanging() { TT_HANG_TO_MONKEY_SWING_DIAGONAL_JUMP_UP } else { TT_MONKEY_SWING };
    } else if s.deep(HERE, DN1) {
        put(r, TT_SWIM, KF_BOB, 30);
    }
    Ok(())
}

// =====================================================================================================
// Gap jump (|xd| = 2)
// =====================================================================================================

fn gap_jump(s: &Scene, r: &mut PathTestResult) -> Flow {
    if s.pose == KF_FALLING || !s.elevator_origin_ok() || s.y0 != s.oy {
        return Ok(());
    }
    let b = -s.xd.signum();
    s.req((b, 0))?;
    s.req((b, 1))?;
    s.req((b, -1))?;
    if !s.pass_d((b, 0), (b, -1)) {
        return Ok(());
    }
    s.req((b, 2))?;
    if !s.pass_surface((b, 1), (b, 2)) {
        return Ok(());
    }
    s.req(UP1)?;
    if !s.pass_surface(HERE, UP1) {
        return Ok(());
    }
    s.req(DN1)?;
    if !(s.solid(DN1) && !s.door(DN1)) {
        return Ok(());
    }
    if s.headroom(UP1) && !matches!(s.pose, KF_CRAWL | KF_HANG_RIGHT | KF_HANG_LEFT) {
        put(r, TT_JUMP_OVER_GAP, KF_STAND, 25);
    }
    Ok(())
}

// =====================================================================================================
// Vertical moves
// =====================================================================================================

fn vertical_move(s: &Scene, r: &mut PathTestResult) -> Flow {
    match s.y0.wrapping_sub(s.oy) {
        1 => up_one(s, r),
        -1 => down_one(s, r),
        2 => up_two(s, r),
        0 if s.x0 == s.ox => stay(s, r),
        _ => Ok(()),
    }
}

/// Wall-grab result of an up-one move: standing jumps, same-pose climbs, anything else hang-to-hang (50).
fn wall_grab(s: &Scene, r: &mut PathTestResult, wall: i32) {
    let hang = wall_pose(wall);
    if s.pose == KF_STAND {
        put(r, TT_STAND_TO_HANG_JUMP, hang, 40);
    } else if s.pose == hang {
        put(r, TT_CLIMB, hang, 40);
    } else {
        put(r, TT_HANG_TO_HANG_JUMP, hang, 50);
    }
}

/// V1: up one.
fn up_one(s: &Scene, r: &mut PathTestResult) -> Flow {
    s.req(UP1)?;
    if s.flies && (s.open(HERE) || s.hatch(HERE)) && (s.open(UP1) || s.hatch(UP1)) && !s.lift_here() {
        put(r, TT_FLY, KF_FLY, 4);
        return Ok(());
    }
    if s.too_fast() {
        return Ok(());
    }
    s.req(DN1)?;
    if s.lift_here() {
        put(r, TT_ELEVATOR, KF_STAND_ELEVATOR, 5);
        return Ok(());
    }
    if !s.pass_h(HERE, DN1) {
        if s.deep(HERE, DN1) {
            put(r, TT_SWIM, KF_BOB, 30);
        }
        return Ok(());
    }
    if !(s.open(UP1) || s.shallow(UP1, HERE) || s.hatch(UP1)) {
        return Ok(());
    }
    if s.grip(HERE) {
        r.traverse_to_key_frame = KF_HANG_BACK;
        if matches!(s.pose, KF_HANG_BACK | KF_STAND) {
            r.traverse_type = TT_CLIMB;
            r.terrain_difficulty = if s.ladder(HERE) || s.hatch(HERE) || s.shaft(HERE) { 20 } else { 40 };
        } else {
            r.traverse_type = TT_HANG_TO_HANG_JUMP;
            r.terrain_difficulty = 50;
        }
        if s.firm_grip(HERE) {
            return Ok(());
        }
    }
    s.req(WEST)?;
    if s.solid(WEST) {
        wall_grab(s, r, WEST.0);
        return Ok(());
    }
    s.req(EAST)?;
    if s.solid(EAST) {
        wall_grab(s, r, EAST.0);
        return Ok(());
    }
    s.req(UP2)?;
    if s.solid(UP2) {
        put(r, TT_STAND_TO_MONKEY_HANG_JUMP, KF_MONKEY_HANG, 42);
    } else if s.deep(HERE, DN1) {
        put(r, TT_SWIM, KF_BOB, 30);
    }
    Ok(())
}

/// V2: down one.
fn down_one(s: &Scene, r: &mut PathTestResult) -> Flow {
    s.req(DN1)?;
    if s.flies && (s.open(HERE) || s.hatch(HERE)) && !s.lift_here() {
        put(r, TT_FLY, KF_FLY, 4);
        return Ok(());
    }
    if s.lift_here() {
        put(r, TT_ELEVATOR, KF_STAND_ELEVATOR, 5);
        return Ok(());
    }
    if !s.pass_h(HERE, DN1) {
        if s.deep(HERE, DN1) {
            put(r, TT_SWIM, KF_BOB, 30);
        }
        return Ok(());
    }
    if !s.too_fast() && s.grip(HERE) {
        let quick = s.ladder(HERE) || s.hatch(HERE) || s.shaft(HERE);
        put(r, TT_CLIMB, KF_HANG_BACK, if quick { 20 } else { 40 });
        if s.firm_grip(HERE) {
            return Ok(());
        }
    }
    let fall_cost = s.fall_base() + 10;
    if s.solid(DN1) {
        if s.side_hanging() {
            put(r, TT_HANG_TO_STAND_JUMP, KF_STAND, 30);
        } else {
            r.terrain_difficulty = fall_cost;
            if s.fall_ok() {
                r.traverse_type = TT_FALL;
                r.traverse_to_key_frame = KF_STAND;
            }
        }
        return Ok(());
    }
    if !s.too_fast() {
        let first = if s.pose == KF_HANG_RIGHT { 1 } else { -1 };
        for side in [first, -first] {
            s.req((side, 0))?;
            if s.solid((side, 0)) {
                put(r, TT_CLIMB, wall_pose(side), 40);
                return Ok(());
            }
        }
    }
    if s.deep(HERE, DN1) {
        put(r, TT_SWIM, KF_BOB, 30);
        return Ok(());
    }
    s.req(UP2)?;
    if s.solid(UP2) {
        put(r, TT_STAND_TO_MONKEY_HANG_JUMP, KF_MONKEY_HANG, 42);
    } else if s.fall_ok() {
        put(r, TT_FALL, KF_FALLING, fall_cost);
    }
    Ok(())
}

/// V3: up two (double jump).
fn up_two(s: &Scene, r: &mut PathTestResult) -> Flow {
    if s.pose == KF_FALLING {
        return Ok(());
    }
    s.req(DN1)?;
    s.req(DN2)?;
    s.req(HERE)?;
    if !(s.pass_h(DN1, DN2) && (s.open(HERE) || s.hatch(HERE))) {
        return Ok(());
    }
    s.req(UP1)?;
    if !(s.open(UP1) || s.hatch(UP1)) {
        return Ok(());
    }
    let kind_for = |hang: u32| match s.pose {
        KF_STAND => TT_STAND_TO_HANG_DOUBLE_JUMP,
        p if p == hang => TT_CLIMB,
        _ => TT_HANG_TO_HANG_JUMP,
    };
    if s.grip(HERE) {
        put(r, kind_for(KF_HANG_BACK), KF_HANG_BACK, 65);
        if s.firm_grip(HERE) {
            return Ok(());
        }
    }
    for side in [WEST, EAST] {
        s.req(side)?;
        if s.solid(side) {
            put(r, kind_for(wall_pose(side.0)), wall_pose(side.0), 65);
            return Ok(());
        }
    }
    Ok(())
}

/// V4: stay in place.
fn stay(s: &Scene, r: &mut PathTestResult) -> Flow {
    if s.solid(HERE) {
        put(r, TT_NONE, KF_STAND, 10);
        return Ok(());
    }
    s.req(UP1)?;
    s.req(DN1)?;
    if s.flies && s.open(HERE) && s.open(UP1) {
        put(r, TT_NONE, KF_FLY, 4);
        return Ok(());
    }
    if !(s.open(HERE) || s.shallow(HERE, DN1)) {
        if s.deep(HERE, DN1) {
            put(r, TT_SWIM, KF_BOB, 30);
        }
        return Ok(());
    }
    let up_clear = s.open(UP1) || s.shallow(UP1, HERE) || s.hatch(UP1);
    if s.solid(DN1) {
        if up_clear {
            put(r, TT_NONE, KF_STAND, 10);
        } else {
            put(r, TT_NONE, KF_CRAWL, 30);
        }
        return Ok(());
    }
    if !up_clear {
        // (The specification also lists a swim result here; it cannot hold once the cell passed the open /
        // shallow test above, so it is not coded.)
        put(r, TT_FALL, KF_FALLING, 20);
        return Ok(());
    }

    let gripped = s.grip(HERE);
    if gripped {
        if s.shaft(HERE) && !s.fall_search && s.lift(HERE) {
            put(r, TT_NONE, KF_STAND_ELEVATOR, 5);
        } else {
            put(r, TT_NONE, KF_HANG_BACK, 30);
        }
        if s.firm_grip(HERE) {
            return Ok(());
        }
    }
    let first = if s.pose == KF_HANG_RIGHT { 1 } else { -1 };
    for side in [first, -first] {
        s.req((side, 0))?;
        if s.solid((side, 0)) {
            put(r, TT_NONE, wall_pose(side), 30);
            return Ok(());
        }
    }
    if gripped {
        // cactus-only grip: its provisional result stands
        return Ok(());
    }
    if s.stairs(HERE) {
        s.req(UP2)?;
        if !s.solid(UP2) && s.empty_or_stairs(UP2) {
            put(r, TT_NONE, KF_STAIRS, 30);
            return Ok(());
        }
    }
    if s.wet(HERE) {
        put(r, TT_SWIM, KF_BOB, 30);
        return Ok(());
    }
    s.req(UP2)?;
    s.req(UP1)?;
    if s.solid(UP2) && !s.solid(UP1) {
        put(r, TT_NONE, KF_MONKEY_HANG, 30);
    } else if s.flies {
        put(r, TT_NONE, KF_STAND, 10);
    } else {
        put(r, TT_FALL, KF_FALLING, 20);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::path::path_creator::testutil::*;
    use crate::world::tiles::tile_at_world_position_loaded;

    fn ip(x: i32, y: i32) -> IntPair {
        IntPair { x, y }
    }

    fn check(
        world: &Rc<World>,
        user: &ObjRef,
        from: (i32, i32),
        to: (i32, i32),
        fkf: u32,
        from_td: i32,
        fall: bool,
        water: bool,
    ) -> PathTestResult {
        let t = tile_at_world_position_loaded(to.0, to.1, world);
        let ft = tile_at_world_position_loaded(from.0, from.1, world);
        check_can_enter_tile(
            t,
            ft,
            FromProps { terrain_difficulty: from_td, traverse_to_key_frame: fkf as i32 },
            from.0,
            from.1,
            to.0,
            to.1,
            Some(user),
            world,
            fall,
            water,
            &[],
        )
    }

    fn tr(r: &PathTestResult) -> (u32, u32, i32, u8) {
        (r.traverse_type, r.traverse_to_key_frame, r.terrain_difficulty, r.failed_due_to_no_tile)
    }

    /// : preamble exits and the initial value.
    #[test]
    fn preamble() {
        let (world, _l, _d) = new_world(32, 20);
        let user = new_user(ip(100, 21));
        let init = (TT_INACCESSIBLE, KF_INACCESSIBLE, 999999, 0);
        // toY > 1019 and fromY < toY
        let r = check_can_enter_tile(
            tile_at_world_position_loaded(100, 1020, &world),
            None,
            FromProps::default(),
            100,
            1019,
            100,
            1020,
            Some(&user),
            &world,
            false,
            true,
            &[],
        );
        assert_eq!(tr(&r), init);
        // null tile
        let r =
            check_can_enter_tile(None, None, FromProps::default(), 0, 0, 1, 0, Some(&user), &world, false, true, &[]);
        assert_eq!(tr(&r), (TT_INACCESSIBLE, KF_INACCESSIBLE, 999999, 1));
        // off-world index (y < 0) with a tile
        let t = tile_at_world_position_loaded(100, 5, &world);
        let r =
            check_can_enter_tile(t, t, FromProps::default(), 100, 5, 100, -1, Some(&user), &world, false, true, &[]);
        assert_eq!(tr(&r), init);
        // full water, !allowsWaterTiles, non-air above
        edit(&world, 100, 30, |t| {
            t.type_index = 3;
            t.partial_content_left = 0xff;
        });
        edit(&world, 100, 31, |t| t.type_index = STONE);
        let r = check(&world, &user, (100, 30), (100, 30), 0, 0, false, false);
        assert_eq!(tr(&r), init);
        // with water allowed the stay classification runs and swims (full water)
        let r = check(&world, &user, (100, 30), (100, 30), 0, 0, false, true);
        assert_eq!(tr(&r), (TT_SWIM, KF_BOB, 30, 0));
        // unlit tile outside the radius with a pending block load => failedDueToNoTile.
        // (macro row 1: an unlit tile in macro row 0 or 31 dereferences a NULL macro tile in the original)
        edit(&world, 100, 40, |t| t.sun_light = 0);
        let far_user = new_user(ip(500, 40));
        let t = tile_at_world_position_loaded(100, 40, &world);
        let r = check_can_enter_tile(
            t,
            t,
            FromProps::default(),
            100,
            40,
            100,
            40,
            Some(&far_user),
            &world,
            false,
            true,
            &[],
        );
        assert_eq!(r.failed_due_to_no_tile, 1);
        // unlit tile within radius < 17 is fine
        let near_user = new_user(ip(102, 40));
        let r = check_can_enter_tile(
            t,
            t,
            FromProps::default(),
            100,
            40,
            100,
            40,
            Some(&near_user),
            &world,
            false,
            true,
            &[],
        );
        assert_eq!(r.failed_due_to_no_tile, 0);
    }

    #[test]
    fn walking_classification() {
        let (world, _l, _d) = new_world(32, 20);
        let user = new_user(ip(100, 21));
        // level step onto air with ground below
        let r = check(&world, &user, (100, 21), (101, 21), 0, 0, false, true);
        assert_eq!(tr(&r), (TT_WALK, KF_STAND, 10, 0));
        // step up onto a 1-high ledge: (101,21) solid, (101,22) air
        edit(&world, 101, 21, |t| t.type_index = STONE);
        let r = check(&world, &user, (100, 21), (101, 22), 0, 0, false, true);
        assert_eq!(tr(&r), (TT_WALK, KF_STAND, 0x10, 0));
        // step down: from (101,22) to (100,21)
        let r = check(&world, &user, (101, 22), (100, 21), 0, 0, false, true);
        assert_eq!(r.traverse_type, TT_WALK);
        // stay on solid ground: tile (100, 20) solid => td 10 NONE STAND
        let r = check(&world, &user, (100, 20), (100, 20), 0, 0, false, true);
        assert_eq!(tr(&r), (TT_NONE, KF_STAND, 10, 0));
        // fall: from (150,25) down to (150,24) with air below (ground is at 20)
        let r = check(&world, &user, (150, 25), (150, 24), 0, 0, false, true);
        assert_eq!(r.traverse_type, TT_FALL);
        assert_eq!(r.traverse_to_key_frame, KF_FALLING);
    }

    ///  notes: a rejected fall in the vertical dy == -1 branch keeps terrainDifficulty = parent + 10.
    #[test]
    fn rejected_fall_keeps_parent_plus_ten() {
        let (world, _l, _d) = new_world(32, 20);
        let user = new_user(ip(100, 21));
        // to = (150,21): air with solid ground below; from = (150,22) falling with difficulty 70
        let r = check(&world, &user, (150, 22), (150, 21), KF_FALLING, 70, false, true);
        assert_eq!(r.traverse_type, TT_INACCESSIBLE);
        assert_eq!(r.terrain_difficulty, 80);
        let r = check(&world, &user, (150, 22), (150, 21), KF_FALLING, 69, false, true);
        assert_eq!((r.traverse_type, r.traverse_to_key_frame, r.terrain_difficulty), (TT_FALL, KF_STAND, 79));
        // a fall path accepts regardless
        let r = check(&world, &user, (150, 22), (150, 21), KF_FALLING, 70, true, true);
        assert_eq!((r.traverse_type, r.terrain_difficulty), (TT_FALL, 80));
        // falling too fast to climb/stop (> 99) in the dy == +1 branch
        let r = check(&world, &user, (150, 21), (150, 22), KF_FALLING, 100, false, true);
        assert_eq!(r.traverse_type, TT_INACCESSIBLE);
    }

    #[test]
    fn jump_over_gap() {
        let (world, loader, _d) = new_world(32, 20);
        *loader.fill.borrow_mut() = Box::new(|x, y| {
            let mut t = flat_tile(20)(x, y);
            if x == 201 && y <= 20 && y >= 10 {
                t.type_index = AIR;
            }
            t
        });
        let user = new_user(ip(200, 21));
        let r = check(&world, &user, (200, 21), (202, 21), 0, 0, false, true);
        assert_eq!(tr(&r), (TT_JUMP_OVER_GAP, KF_STAND, 0x19, 0));
        // from a hanging key frame the jump is rejected
        let r = check(&world, &user, (200, 21), (202, 21), KF_HANG_RIGHT, 0, false, true);
        assert_eq!(r.traverse_type, TT_INACCESSIBLE);
        // not on the same row
        let r = check(&world, &user, (200, 22), (202, 21), 0, 0, false, true);
        assert_eq!(r.traverse_type, TT_INACCESSIBLE);
    }

    /// Tile loads follow the original order: a level walk loads (toX,toY+1) then (toX,toY-1) via `load!` lines
    /// (tiles already present in this fixture, so only the cache behaviour is observable): no extra loads.
    #[test]
    fn no_spurious_block_loads() {
        let (world, loader, _d) = new_world(32, 20);
        let user = new_user(ip(100, 21));
        let _ = check(&world, &user, (100, 21), (101, 21), 0, 0, false, true);
        let n = loader.loads.borrow().len();
        let _ = check(&world, &user, (100, 21), (101, 21), 0, 0, false, true);
        assert_eq!(loader.loads.borrow().len(), n);
    }
}

/// Decision-table tests derived from the behavioural specification of the tile-entry test.
///
/// Every case builds a tiny world (all air, fully lit), edits a few cells given as offsets from the destination
/// (x grows to the right, y upward) and asserts the full result `(kind, pose, cost, missing-tile flag)`.
#[cfg(test)]
mod decision_table {
    use super::*;
    use crate::path::path_creator::testutil::*;
    use crate::path::path_creator::{set_dynamic_world_hooks, PathDynamicWorldHooks};
    use crate::path::wire_path::WirePowerSource;
    use crate::types::tile::Tile;
    use crate::world::state::ElevatorMotorInfo;

    type R = (u32, u32, i32, u8);
    type Cells<'a> = &'a [(i32, i32, K)];

    const REF: R = (TT_INACCESSIBLE, KF_INACCESSIBLE, 999_999, 0);
    const NOTILE: R = (TT_INACCESSIBLE, KF_INACCESSIBLE, 999_999, 1);

    fn res(kind: u32, pose: u32, cost: i32) -> R {
        (kind, pose, cost, 0)
    }

    /// Cell flavours.
    #[derive(Clone, Copy)]
    enum K {
        /// stone
        S,
        Snow,
        /// shallow water
        W,
        /// full water
        F,
        /// ladder (foreground) in air
        Lad,
        /// elevator shaft in air
        Sh,
        Stairs,
        /// cactus contents (a tree: climbable, no ladder)
        Cac,
        /// cactus with a ladder
        CacLad,
        /// trap door in air
        Hatch,
        /// trap door in stone
        HatchS,
        /// door in air
        Door,
        /// door in stone
        DoorS,
        /// full water carrying a door
        DoorF,
        /// full water carrying a trap door and a cactus
        HatchF,
    }
    use K::*;

    fn apply(t: &mut Tile, k: K) {
        t.type_index = match k {
            S | HatchS | DoorS => STONE,
            Snow => 5,
            W | F | DoorF | HatchF => 3,
            _ => AIR,
        };
        t.partial_content_left = match k {
            W => 100,
            F | DoorF | HatchF => 255,
            _ => 0,
        };
        match k {
            Lad => t.foreground_contents = 0x42,
            Sh => t.contents = 0x67,
            Stairs => t.contents = 0x65,
            Cac => t.contents = 0x2b,
            CacLad => {
                t.contents = 0x2b;
                t.foreground_contents = 0x42;
            }
            Hatch | HatchS => t.background_contents = 0x4b,
            Door | DoorS | DoorF => t.background_contents = 0x46,
            HatchF => {
                t.background_contents = 0x4b;
                t.contents = 0x2b;
            }
            _ => {}
        }
    }

    /// Per-case knobs.
    #[derive(Clone, Copy)]
    struct O {
        fly: bool,
        crawl: bool,
        fall: bool,
        door: bool,
        /// 0 none, 1 powered motor at the destination, 2 unpowered, 3 powered but blacklisted
        lift: u8,
        water: bool,
        pc: i32,
        base: (i32, i32),
    }
    fn o() -> O {
        O { fly: false, crawl: false, fall: false, door: false, lift: 0, water: true, pc: 0, base: (300, 100) }
    }

    /// Stone floor under the origin / destination row.
    fn fl() -> Vec<(i32, i32, K)> {
        (-3..=3).map(|x| (x, -1, S)).collect()
    }
    /// Floor plus extra cells.
    fn on(extra: Cells) -> Vec<(i32, i32, K)> {
        let mut v = fl();
        v.extend_from_slice(extra);
        v
    }

    fn run(cells: Cells, from: (i32, i32), pose: u32, o: O) -> R {
        let (world, _l, dw) = new_world(32, -1);
        let (bx, by) = o.base;
        let user = new_user(IntPair { x: bx, y: by });
        user_knobs(&user).can_fly.set(o.fly);
        user_knobs(&user).can_crawl.set(o.crawl);
        dw.door.set(o.door);
        for &(dx, dy, k) in cells {
            edit(&world, bx + dx, by + dy, |t| apply(t, k));
        }
        let motor = ElevatorMotorInfo { has_required_power: o.lift != 2, pos: IntPair { x: bx, y: by } };
        if o.lift != 0 {
            dw.motor.borrow_mut().insert((bx, by), motor);
        }
        let blacklist = if o.lift == 3 { vec![motor.pos] } else { vec![] };
        let (fx, fy) = (bx + from.0, by + from.1);
        let dest = tile_at_world_position_loaded(bx, by, &world);
        let origin = tile_at_world_position_loaded(fx, fy, &world);
        let r = check_can_enter_tile(
            dest,
            origin,
            FromProps { terrain_difficulty: o.pc, traverse_to_key_frame: pose as i32 },
            fx,
            fy,
            bx,
            by,
            Some(&user),
            &world,
            o.fall,
            o.water,
            &blacklist,
        );
        (r.traverse_type, r.traverse_to_key_frame, r.terrain_difficulty, r.failed_due_to_no_tile)
    }

    fn ck(label: &str, cells: Cells, from: (i32, i32), pose: u32, o: O, exp: R) {
        assert_eq!(run(cells, from, pose, o), exp, "{label}");
    }

    // ------------------------------------------------------------------------------------------------
    // admission
    // ------------------------------------------------------------------------------------------------

    #[test]
    fn admission_water_and_light() {
        // full water, water not allowed, air above: allowed (stay swims)
        let no_water = O { water: false, ..o() };
        ck("water above air", &[(0, 0, F)], (0, 0), KF_STAND, no_water, res(TT_SWIM, KF_BOB, 30));
        // ... but not when the cell above is not air (snow counts as not air)
        ck("water above snow", &[(0, 0, F), (0, 1, Snow)], (0, 0), KF_STAND, no_water, REF);
        // shallow water is not refused
        ck("shallow", &[(0, 0, W), (0, -1, S)], (0, 0), KF_STAND, no_water, res(TT_NONE, KF_STAND, 10));
    }

    struct Waiting;
    impl PathDynamicWorldHooks for Waiting {
        fn has_lights_to_add(&self) -> bool {
            true
        }
        fn blockhead_occupies_tile_at_pos_ignore_blockhead(&self, _p: IntPair, _i: Option<&ObjRef>) -> bool {
            false
        }
        fn workbench_at_pos(&self, _p: IntPair) -> Option<Rc<dyn WirePowerSource>> {
            None
        }
    }

    #[test]
    fn origin_props_conversion_and_hook_stub() {
        let p = DerivedTileProperties { terrain_difficulty: 77, traverse_to_key_frame: 4, ..Default::default() };
        let f = FromProps::from(&p);
        assert_eq!((f.terrain_difficulty, f.traverse_to_key_frame), (77, 4));
        let w = Waiting;
        assert!(w.has_lights_to_add());
        assert!(!w.blockhead_occupies_tile_at_pos_ignore_blockhead(IntPair { x: 0, y: 0 }, None));
        assert!(w.workbench_at_pos(IntPair { x: 0, y: 0 }).is_none());
    }

    fn unlit(t: &mut Tile) {
        t.sun_light = 0;
    }

    #[test]
    fn admission_lit_variants() {
        let far = O { base: (300, 100), ..o() };
        let _ = far;
        let (world, _l, _d) = new_world(32, -1);
        let user = new_user(IntPair { x: 900, y: 500 });
        let go = |world: &Rc<World>, user: Option<&ObjRef>, fall: bool| {
            let t = tile_at_world_position_loaded(300, 100, world);
            check_can_enter_tile(t, t, FromProps::default(), 300, 100, 300, 100, user, world, fall, true, &[])
        };
        edit(&world, 300, 100, unlit);
        edit(&world, 300, 99, |t| t.type_index = STONE);
        // first call: neighbour blocks are not loaded yet -> "try again later"
        assert_eq!(go(&world, Some(&user), false).failed_due_to_no_tile, 1);
        // second call: everything is loaded and nothing is pending: plain refusal (still unlit)
        let r = go(&world, Some(&user), false);
        assert_eq!((r.traverse_type, r.failed_due_to_no_tile), (TT_INACCESSIBLE, 0));
        // a fall search ignores the light state and classifies normally
        let r = go(&world, Some(&user), true);
        assert_eq!((r.traverse_type, r.traverse_to_key_frame, r.terrain_difficulty), (TT_NONE, KF_STAND, 10));
        // no mover at all: the mover's own lit query is false, so a lit destination is refused too
        edit(&world, 300, 100, |t| t.sun_light = 255);
        let r = go(&world, None, false);
        assert_eq!((r.traverse_type, r.failed_due_to_no_tile), (TT_INACCESSIBLE, 0));
        // artificial light of every channel counts
        for chan in 0..3 {
            edit(&world, 300, 100, |t| {
                t.sun_light = 0;
                t.artificial_light_r = u16::from(chan == 0);
                t.artificial_light_g = u16::from(chan == 1);
                t.artificial_light_b = u16::from(chan == 2);
            });
            let r = go(&world, Some(&user), false);
            assert_eq!((r.traverse_type, r.failed_due_to_no_tile), (TT_NONE, 0), "channel {chan}");
        }
        // the proximity rule: within squared distance 16 (4 columns), but not 17
        edit(&world, 300, 100, |t| {
            t.artificial_light_b = 0;
        });
        let near = new_user(IntPair { x: 304, y: 100 });
        assert_eq!(go(&world, Some(&near), false).failed_due_to_no_tile, 0);
        assert_eq!(go(&world, Some(&near), false).traverse_type, TT_NONE);
        let just_out = new_user(IntPair { x: 305, y: 100 });
        assert_eq!(go(&world, Some(&just_out), false).traverse_type, TT_INACCESSIBLE);
    }

    #[test]
    fn admission_unlit_in_edge_macro_rows_and_pending_lights() {
        // macro row 0: the neighbour row above (-1) does not exist; deterministic policy: skipped
        let (world, _l, _d) = new_world(32, -1);
        let user = new_user(IntPair { x: 900, y: 500 });
        edit(&world, 300, 5, unlit);
        let t = tile_at_world_position_loaded(300, 5, &world);
        let go =
            || check_can_enter_tile(t, t, FromProps::default(), 300, 5, 300, 5, Some(&user), &world, false, true, &[]);
        assert_eq!(go().failed_due_to_no_tile, 1);
        assert_eq!(go().failed_due_to_no_tile, 0);

        // lights already waiting to be added: the neighbours are not even visited
        set_dynamic_world_hooks(Some(Rc::new(Waiting)));
        let (world2, loader2, _d2) = new_world(32, -1);
        edit(&world2, 300, 100, unlit);
        let t2 = tile_at_world_position_loaded(300, 100, &world2);
        let loads_before = loader2.loads.borrow().len();
        let r = check_can_enter_tile(
            t2,
            t2,
            FromProps::default(),
            300,
            100,
            300,
            100,
            Some(&user),
            &world2,
            false,
            true,
            &[],
        );
        set_dynamic_world_hooks(None);
        assert_eq!(r.failed_due_to_no_tile, 1);
        assert_eq!(loader2.loads.borrow().len(), loads_before);
    }

    #[test]
    fn admission_lift_exit_counts_as_lit() {
        // an unlit shaft cell is lit for a rider leaving an elevator
        let dark_shaft = |t: &mut Tile| {
            t.sun_light = 0;
            t.contents = 0x67;
        };
        let (world, dw) = {
            let (w, _l, d) = new_world(32, -1);
            (w, d)
        };
        edit(&world, 300, 100, dark_shaft);
        edit(&world, 300, 101, |t| t.contents = 0x67);
        let user = new_user(IntPair { x: 900, y: 500 });
        dw.motor
            .borrow_mut()
            .insert((300, 100), ElevatorMotorInfo { has_required_power: true, pos: IntPair { x: 300, y: 100 } });
        let t = tile_at_world_position_loaded(300, 100, &world);
        let f = tile_at_world_position_loaded(300, 99, &world);
        let r = check_can_enter_tile(
            t,
            f,
            FromProps { terrain_difficulty: 0, traverse_to_key_frame: KF_STAND_ELEVATOR as i32 },
            300,
            99,
            300,
            100,
            Some(&user),
            &world,
            false,
            true,
            &[],
        );
        assert_eq!(
            (r.traverse_type, r.traverse_to_key_frame, r.terrain_difficulty),
            (TT_ELEVATOR, KF_STAND_ELEVATOR, 5)
        );
    }

    #[test]
    fn admission_top_rows_and_missing_cells() {
        // moving up into the top rows is refused, with or without a tile
        let top = O { base: (300, 1020), ..o() };
        ck("top row up", &[], (0, -1), KF_STAND, top, REF);
        // a missing cell below the world -> missing-tile flag, record as it stands
        let bottom = O { base: (300, 0), ..o() };
        ck("below world", &[], (0, 1), KF_STAND, bottom, NOTILE);
        // a provisional result survives a later missing cell (cactus grip, then the wall probe falls off the world)
        ck("provisional", &[(0, 0, Cac)], (0, 1), KF_STAND, bottom, NOTILE);
    }

    // ------------------------------------------------------------------------------------------------
    // side / diagonal moves: origin gate and shape selection
    // ------------------------------------------------------------------------------------------------

    #[test]
    fn side_gate() {
        // falling without flight: refused; with flight the level rules run
        ck("falling", &on(&[]), (-1, 0), KF_FALLING, o(), REF);
        ck("falling+fly", &on(&[]), (-1, 0), KF_FALLING, O { fly: true, ..o() }, res(TT_FLY, KF_FLY, 4));
        // riding an elevator out of a solid origin needs a usable door there
        let from_solid = on(&[(-1, 0, S)]);
        ck("elevator, solid origin", &from_solid, (-1, 0), KF_STAND_ELEVATOR, o(), REF);
        let from_door = on(&[(-1, 0, DoorS)]);
        ck(
            "elevator, door origin",
            &from_door,
            (-1, 0),
            KF_STAND_ELEVATOR,
            O { door: true, ..o() },
            res(TT_WALK, KF_STAND, 10),
        );
        ck("elevator, open origin", &on(&[]), (-1, 0), KF_STAND_ELEVATOR, o(), res(TT_WALK, KF_STAND, 10));
        // other row distances are refused
        ck("dy=3", &on(&[]), (-1, -3), KF_STAND, o(), REF);
        ck("dy=-3", &on(&[]), (-1, 3), KF_STAND, o(), REF);
    }

    // ------------------------------------------------------------------------------------------------
    // M1 drop two
    // ------------------------------------------------------------------------------------------------

    #[test]
    fn drop_two_monkey_hang() {
        let base = [(0, 2, S)];
        let m = res(TT_CLIMB_DOWN_TO_MONKEY_SWING, KF_MONKEY_HANG, 70);
        ck("hang below", &base, (-1, 2), KF_STAND, o(), m);
        ck(
            "stand below",
            &[(0, 2, S), (0, -1, S)],
            (-1, 2),
            KF_STAND,
            o(),
            res(TT_CLIMB_DOWN_TO_MONKEY_SWING, KF_STAND, 70),
        );
        ck("shallow", &[(0, 2, S), (0, 0, W)], (-1, 2), KF_STAND, o(), m);
        ck("mirror", &base, (1, 2), KF_STAND, o(), m);
        ck("dest solid", &[(0, 2, S), (0, 0, S)], (-1, 2), KF_STAND, o(), REF);
        ck("B+1 solid", &[(0, 2, S), (-1, 1, S)], (-1, 2), KF_STAND, o(), REF);
        ck("B0 solid", &[(0, 2, S), (-1, 0, S)], (-1, 2), KF_STAND, o(), REF);
        ck("C+1 solid", &[(0, 2, S), (0, 1, S)], (-1, 2), KF_STAND, o(), REF);
        ck("no ceiling", &[], (-1, 2), KF_STAND, o(), REF);
    }

    // ------------------------------------------------------------------------------------------------
    // M2 rise two
    // ------------------------------------------------------------------------------------------------

    #[test]
    fn rise_two_to_wall_hang() {
        let base = [(-1, 0, S)];
        let left = res(TT_MONKEY_SWING_UP_TO_CLIMB, KF_HANG_LEFT, 70);
        ck("left wall", &base, (-1, -2), KF_MONKEY_HANG, o(), left);
        ck(
            "right wall",
            &[(1, 0, S)],
            (1, -2),
            KF_MONKEY_HANG,
            o(),
            res(TT_MONKEY_SWING_UP_TO_CLIMB, KF_HANG_RIGHT, 70),
        );
        ck("hatch entry", &[(-1, 0, S), (0, 0, HatchS)], (-1, -2), KF_STAND, O { door: true, ..o() }, left);
        ck("shallow entry", &[(-1, 0, S), (0, 0, W)], (-1, -2), KF_STAND, o(), left);
        ck("solid entry", &[(-1, 0, S), (0, 0, S)], (-1, -2), KF_STAND, o(), REF);
        ck("ceiling", &[(-1, 0, S), (0, 1, S)], (-1, -2), KF_STAND, o(), REF);
        ck("floor cell", &[(-1, 0, S), (0, -1, S)], (-1, -2), KF_STAND, o(), REF);
        ck("B-1 solid", &[(-1, 0, S), (-1, -1, S)], (-1, -2), KF_STAND, o(), REF);
        ck("C-2 solid", &[(-1, 0, S), (0, -2, S)], (-1, -2), KF_STAND, o(), REF);
        ck("no wall", &[], (-1, -2), KF_STAND, o(), REF);
        ck("elevator pose", &base, (-1, -2), KF_STAND_ELEVATOR, o(), REF);
    }

    // ------------------------------------------------------------------------------------------------
    // M3 step up diagonally (origin at (-1,-1), floor under the destination ledge)
    // ------------------------------------------------------------------------------------------------

    fn ledge(extra: Cells) -> Vec<(i32, i32, K)> {
        let mut v = vec![(0, -1, S), (-1, -2, S)];
        v.extend_from_slice(extra);
        v
    }
    const UP_FROM: (i32, i32) = (-1, -1);

    #[test]
    fn step_up_walk_family() {
        let w = res(TT_WALK, KF_STAND, 16);
        ck("walk", &ledge(&[]), UP_FROM, KF_STAND, o(), w);
        ck("from crawl", &ledge(&[]), UP_FROM, KF_CRAWL, o(), w);
        for p in [KF_HANG_BACK, KF_HANG_RIGHT, KF_HANG_LEFT] {
            ck("hang to stand", &ledge(&[]), UP_FROM, p, o(), res(TT_HANG_TO_STAND_JUMP, KF_STAND, 42));
        }
        ck("from stairs", &ledge(&[(-1, -1, Stairs)]), UP_FROM, KF_STAND, o(), res(TT_STAIRS, KF_STAND, 12));
        ck(
            "stairs to stairs",
            &ledge(&[(-1, -1, Stairs), (0, 0, Stairs)]),
            UP_FROM,
            KF_STAND,
            o(),
            res(TT_STAIRS, KF_STAIRS, 12),
        );
        ck("behind solid", &ledge(&[(-1, 0, S)]), UP_FROM, KF_STAND, o(), REF);
        ck("mirror", &[(0, -1, S), (1, -2, S)], (1, -1), KF_STAND, o(), w);
    }

    #[test]
    fn step_up_crouch_and_crawl() {
        let door = O { door: true, ..o() };
        ck("crouch", &ledge(&[(-1, 1, S)]), UP_FROM, KF_STAND, o(), res(TT_CROUCH_UP, KF_STAND, 42));
        ck("crouch, door floor", &[(0, -1, DoorS), (-1, 1, S)], UP_FROM, KF_STAND, door, REF);
        ck(
            "low ceiling, crawler",
            &ledge(&[(0, 1, S)]),
            UP_FROM,
            KF_STAND,
            O { crawl: true, ..o() },
            res(TT_CROUCH_CRAWL_UP, KF_CRAWL, 42),
        );
        ck("low ceiling, walker", &ledge(&[(0, 1, S)]), UP_FROM, KF_STAND, o(), REF);
        ck("low ceiling, door floor", &[(0, -1, DoorS), (0, 1, S)], UP_FROM, KF_STAND, door, REF);
        // a usable door in the ceiling does not block
        ck("door ceiling", &ledge(&[(0, 1, DoorS)]), UP_FROM, KF_STAND, door, res(TT_WALK, KF_STAND, 16));
        // doors above and below make the floor not count: no floor rules apply (nothing to grab -> refused)
        ck("door floor and dest", &[(0, -1, DoorS), (0, 0, Door), (-1, -2, S)], UP_FROM, KF_STAND, door, REF);
    }

    #[test]
    fn step_up_fly_and_lift() {
        let fly = O { fly: true, ..o() };
        ck("fly", &[(-1, -2, S)], UP_FROM, KF_STAND, fly, res(TT_FLY, KF_FLY, 6));
        ck("fly from hang", &[], UP_FROM, KF_HANG_BACK, fly, res(TT_HANG_TO_STAND_JUMP, KF_FLY, 42));
        // behind-upper cell solid: no flight; the ordinary rules apply
        ck("fly blocked", &ledge(&[(-1, 1, S)]), UP_FROM, KF_STAND, fly, res(TT_CROUCH_UP, KF_STAND, 42));
        // a powered lift in the destination column
        let lift = O { lift: 1, ..o() };
        let shaft = ledge(&[(0, 0, Sh), (0, 1, Sh)]);
        ck("lift", &shaft, UP_FROM, KF_STAND, lift, res(TT_WALK, KF_STAND_ELEVATOR, 21));
        ck("lift from hang", &shaft, UP_FROM, KF_HANG_LEFT, lift, res(TT_HANG_TO_STAND_JUMP, KF_STAND_ELEVATOR, 42));
        ck(
            "lift blocks flight",
            &shaft,
            UP_FROM,
            KF_STAND,
            O { fly: true, lift: 1, ..o() },
            res(TT_WALK, KF_STAND_ELEVATOR, 21),
        );
        ck("unpowered", &shaft, UP_FROM, KF_STAND, O { lift: 2, ..o() }, res(TT_WALK, KF_STAND, 16));
        ck("blacklisted", &shaft, UP_FROM, KF_STAND, O { lift: 3, ..o() }, res(TT_WALK, KF_STAND, 16));
    }

    #[test]
    fn step_up_water() {
        ck("deep water", &ledge(&[(0, 0, F)]), UP_FROM, KF_STAND, o(), res(TT_SWIM, KF_BOB, 42));
        ck("solid", &ledge(&[(0, 0, S)]), UP_FROM, KF_STAND, o(), REF);
        ck("shallow water", &ledge(&[(0, 0, W)]), UP_FROM, KF_STAND, o(), res(TT_WALK, KF_STAND, 16));
    }

    #[test]
    fn step_up_without_floor() {
        let nofloor = |extra: Cells| {
            let mut v = vec![(-1, -2, S)];
            v.extend_from_slice(extra);
            v
        };
        // staircase continuing up
        let st = [(-1, -1, Stairs), (0, 0, Stairs)];
        ck("stairs", &nofloor(&st), UP_FROM, KF_STAND, o(), res(TT_STAIRS, KF_STAIRS, 12));
        let mut blocked = st.to_vec();
        blocked.push((0, 2, S));
        ck(
            "stairs, low roof",
            &nofloor(&blocked),
            UP_FROM,
            KF_STAND,
            o(),
            res(TT_STAND_TO_MONKEY_HANG_JUMP, KF_MONKEY_HANG, 42),
        );
        let mut blocked = st.to_vec();
        blocked.push((0, 1, S));
        ck("stairs, blocked", &nofloor(&blocked), UP_FROM, KF_STAND, o(), REF);
        ck("blocked above", &nofloor(&[(0, 1, S)]), UP_FROM, KF_STAND, o(), REF);
        // firm grip
        let lad = [(0, 0, Lad), (1, 0, S)];
        ck("ladder, stand", &nofloor(&lad), UP_FROM, KF_STAND, o(), res(TT_STAND_TO_HANG_JUMP, KF_HANG_BACK, 42));
        ck("ladder, hang back", &nofloor(&lad), UP_FROM, KF_HANG_BACK, o(), res(TT_CLIMB, KF_HANG_BACK, 42));
        ck(
            "ladder, hang right",
            &nofloor(&lad),
            UP_FROM,
            KF_HANG_RIGHT,
            o(),
            res(TT_HANG_TO_HANG_JUMP, KF_HANG_BACK, 65),
        );
        // cactus: provisional, replaced by walls / monkey bars / kept
        let cac = [(0, 0, Cac)];
        ck("cactus alone", &nofloor(&cac), UP_FROM, KF_STAND, o(), res(TT_STAND_TO_HANG_JUMP, KF_HANG_BACK, 42));
        let cw = [(0, 0, Cac), (1, 0, S)];
        ck(
            "cactus + wall, stand",
            &nofloor(&cw),
            UP_FROM,
            KF_STAND,
            o(),
            res(TT_STAND_TO_HANG_JUMP, KF_HANG_RIGHT, 42),
        );
        ck("cactus + wall, same pose", &nofloor(&cw), UP_FROM, KF_HANG_RIGHT, o(), res(TT_CLIMB, KF_HANG_RIGHT, 56));
        ck(
            "cactus + wall, other pose",
            &nofloor(&cw),
            UP_FROM,
            KF_HANG_LEFT,
            o(),
            res(TT_HANG_TO_HANG_JUMP, KF_HANG_RIGHT, 65),
        );
        let cm = [(0, 0, Cac), (0, 2, S)];
        ck(
            "cactus + monkey",
            &nofloor(&cm),
            UP_FROM,
            KF_STAND,
            o(),
            res(TT_STAND_TO_MONKEY_HANG_JUMP, KF_MONKEY_HANG, 42),
        );
        // plain air: ahead wall, monkey bars, water
        let wall = [(1, 0, S)];
        ck("wall, stand", &nofloor(&wall), UP_FROM, KF_STAND, o(), res(TT_STAND_TO_HANG_JUMP, KF_HANG_RIGHT, 42));
        ck("wall, same pose", &nofloor(&wall), UP_FROM, KF_HANG_RIGHT, o(), res(TT_CLIMB, KF_HANG_RIGHT, 56));
        ck(
            "wall, other pose",
            &nofloor(&wall),
            UP_FROM,
            KF_HANG_BACK,
            o(),
            res(TT_HANG_TO_HANG_JUMP, KF_HANG_RIGHT, 65),
        );
        ck(
            "monkey, stand",
            &nofloor(&[(0, 2, S)]),
            UP_FROM,
            KF_STAND,
            o(),
            res(TT_STAND_TO_MONKEY_HANG_JUMP, KF_MONKEY_HANG, 42),
        );
        ck(
            "monkey, hanging",
            &nofloor(&[(0, 2, S)]),
            UP_FROM,
            KF_HANG_LEFT,
            o(),
            res(TT_HANG_TO_MONKEY_SWING_DIAGONAL_JUMP_UP, KF_MONKEY_HANG, 50),
        );
        ck("water", &nofloor(&[(0, 0, W)]), UP_FROM, KF_STAND, o(), res(TT_SWIM, KF_BOB, 42));
        ck("nothing", &nofloor(&[]), UP_FROM, KF_STAND, o(), REF);
    }

    // ------------------------------------------------------------------------------------------------
    // M4 step down diagonally (origin at (-1,1))
    // ------------------------------------------------------------------------------------------------

    const DOWN_FROM: (i32, i32) = (-1, 1);

    #[test]
    fn step_down_grounded() {
        let door = O { door: true, ..o() };
        ck("walk", &on(&[]), DOWN_FROM, KF_STAND, o(), res(TT_WALK, KF_STAND, 16));
        ck("from crawl", &on(&[]), DOWN_FROM, KF_CRAWL, o(), res(TT_CROUCH_CRAWL_DOWN, KF_STAND, 42));
        ck("from hang", &on(&[]), DOWN_FROM, KF_HANG_LEFT, o(), res(TT_HANG_TO_STAND_JUMP, KF_STAND, 42));
        ck("headroom missing", &on(&[(0, 1, S)]), DOWN_FROM, KF_STAND, o(), REF);
        ck("low ceiling", &on(&[(0, 2, S)]), DOWN_FROM, KF_STAND, o(), res(TT_CROUCH_DOWN, KF_STAND, 42));
        ck("low ceiling, crawl", &on(&[(0, 2, S)]), DOWN_FROM, KF_CRAWL, o(), res(TT_CROUCH_CRAWL_DOWN, KF_STAND, 42));
        // a door record in C+2 (position of C+1) lifts the low ceiling
        ck("door in ceiling", &on(&[(0, 2, DoorS)]), DOWN_FROM, KF_STAND, door, res(TT_WALK, KF_STAND, 16));
        // a usable door at C+1: refused, but the pose reads STAND
        ck("door at C+1", &on(&[(0, 1, DoorS)]), DOWN_FROM, KF_STAND, door, (TT_INACCESSIBLE, KF_STAND, 999_999, 0));
        // a trap door in the destination is not ground
        ck(
            "hatch dest",
            &on(&[(0, 0, Hatch)]),
            DOWN_FROM,
            KF_STAND,
            door,
            res(TT_STAND_TO_HANG_JUMP, KF_HANG_BACK, 56),
        );
    }

    #[test]
    fn step_down_fly_lift_stairs_water() {
        let fly = O { fly: true, ..o() };
        ck("fly", &on(&[]), DOWN_FROM, KF_STAND, fly, res(TT_FLY, KF_FLY, 6));
        ck("fly from hang", &on(&[]), DOWN_FROM, KF_HANG_RIGHT, fly, res(TT_HANG_TO_STAND_JUMP, KF_FLY, 42));
        ck("fly from crawl", &on(&[]), DOWN_FROM, KF_CRAWL, fly, res(TT_CROUCH_CRAWL_DOWN, KF_FLY, 42));
        ck("fly blocked by wall", &on(&[(-1, 0, S)]), DOWN_FROM, KF_STAND, fly, res(TT_WALK, KF_STAND, 16));
        let shaft = on(&[(0, 0, Sh), (0, 1, Sh)]);
        let lift = O { lift: 1, ..o() };
        ck("lift", &shaft, DOWN_FROM, KF_STAND, lift, res(TT_WALK, KF_STAND_ELEVATOR, 21));
        ck("lift from hang", &shaft, DOWN_FROM, KF_HANG_BACK, lift, res(TT_HANG_TO_STAND_JUMP, KF_STAND_ELEVATOR, 42));
        ck("lift from crawl", &shaft, DOWN_FROM, KF_CRAWL, lift, res(TT_CROUCH_CRAWL_DOWN, KF_STAND_ELEVATOR, 42));
        ck(
            "lift blocks flight",
            &shaft,
            DOWN_FROM,
            KF_STAND,
            O { fly: true, lift: 1, ..o() },
            res(TT_WALK, KF_STAND_ELEVATOR, 21),
        );
        // stairs down
        ck("stairs", &on(&[(0, 0, Stairs)]), DOWN_FROM, KF_STAND, o(), res(TT_STAIRS, KF_STAIRS, 12));
        ck(
            "stairs, low roof",
            &on(&[(0, 0, Stairs), (0, 2, S)]),
            DOWN_FROM,
            KF_STAND,
            o(),
            res(TT_CROUCH_DOWN, KF_STAND, 42),
        );
        // water
        ck("solid dest", &on(&[(0, 0, S)]), DOWN_FROM, KF_STAND, o(), REF);
        ck("deep dest", &on(&[(0, 0, F)]), DOWN_FROM, KF_STAND, o(), res(TT_SWIM, KF_BOB, 42));
        ck(
            "deep dest, door above",
            &on(&[(0, 0, F), (0, 1, DoorS)]),
            DOWN_FROM,
            KF_STAND,
            O { door: true, ..o() },
            REF,
        );
        ck("shallow dest", &on(&[(0, 0, W)]), DOWN_FROM, KF_STAND, o(), res(TT_WALK, KF_STAND, 16));
    }

    #[test]
    fn step_down_without_ground() {
        // grips
        let lad = [(0, 0, Lad)];
        ck("ladder, stand", &lad, DOWN_FROM, KF_STAND, o(), res(TT_STAND_TO_HANG_JUMP, KF_HANG_BACK, 56));
        ck("ladder, crawl", &lad, DOWN_FROM, KF_CRAWL, o(), res(TT_CROUCH_CRAWL_DOWN, KF_HANG_BACK, 56));
        ck("ladder, hang back", &lad, DOWN_FROM, KF_HANG_BACK, o(), res(TT_CLIMB, KF_HANG_BACK, 56));
        ck(
            "ladder, monkey",
            &lad,
            DOWN_FROM,
            KF_MONKEY_HANG,
            o(),
            res(TT_MONKEY_SWING_TO_HANG_DIAGONAL_JUMP_DOWN, KF_HANG_BACK, 56),
        );
        ck("ladder, hang right", &lad, DOWN_FROM, KF_HANG_RIGHT, o(), res(TT_HANG_TO_HANG_JUMP, KF_HANG_BACK, 65));
        let lad_low = [(0, 0, Lad), (0, 2, S)];
        ck("ladder, low ceiling", &lad_low, DOWN_FROM, KF_STAND, o(), res(TT_CROUCH_DOWN, KF_HANG_BACK, 42));
        ck(
            "ladder, low ceiling, crawl",
            &lad_low,
            DOWN_FROM,
            KF_CRAWL,
            o(),
            res(TT_CROUCH_CRAWL_DOWN, KF_HANG_BACK, 42),
        );
        // cactus: provisional, replaced by walls
        let cac = [(0, 0, Cac)];
        ck("cactus alone", &cac, DOWN_FROM, KF_STAND, o(), res(TT_STAND_TO_HANG_JUMP, KF_HANG_BACK, 56));
        let behind = [(0, 0, Cac), (-1, 0, S)];
        ck("behind, stand", &behind, DOWN_FROM, KF_STAND, o(), res(TT_STAND_TO_HANG_JUMP, KF_HANG_LEFT, 42));
        ck(
            "behind, stand, low",
            &[(0, 0, Cac), (-1, 0, S), (0, 2, S)],
            DOWN_FROM,
            KF_STAND,
            o(),
            res(TT_CROUCH_DOWN, KF_HANG_LEFT, 42),
        );
        ck("behind, crawl", &behind, DOWN_FROM, KF_CRAWL, o(), res(TT_CROUCH_CRAWL_DOWN, KF_HANG_LEFT, 42));
        ck(
            "behind, monkey",
            &behind,
            DOWN_FROM,
            KF_MONKEY_HANG,
            o(),
            res(TT_MONKEY_SWING_TO_HANG_DIAGONAL_JUMP_DOWN, KF_HANG_LEFT, 42),
        );
        ck("behind, same pose", &behind, DOWN_FROM, KF_HANG_LEFT, o(), res(TT_CLIMB, KF_HANG_LEFT, 56));
        ck("behind, other pose", &behind, DOWN_FROM, KF_HANG_RIGHT, o(), res(TT_HANG_TO_HANG_JUMP, KF_HANG_LEFT, 65));
        let ahead = [(0, 0, Cac), (1, 0, S)];
        ck("ahead", &ahead, DOWN_FROM, KF_STAND, o(), res(TT_STAND_TO_HANG_JUMP, KF_HANG_RIGHT, 42));
        ck(
            "both walls: behind wins",
            &[(-1, 0, S), (1, 0, S)],
            DOWN_FROM,
            KF_STAND,
            o(),
            res(TT_STAND_TO_HANG_JUMP, KF_HANG_LEFT, 42),
        );
        // no grip, no wall
        ck(
            "monkey bars",
            &[(0, 2, S)],
            DOWN_FROM,
            KF_STAND,
            o(),
            res(TT_MONKEY_HANG_TO_STAND_JUMP, KF_MONKEY_HANG, 42),
        );
        ck("nothing", &[], DOWN_FROM, KF_STAND, o(), REF);
        ck(
            "swim in door water",
            &[(0, 0, DoorF)],
            DOWN_FROM,
            KF_STAND,
            O { door: true, ..o() },
            res(TT_SWIM, KF_BOB, 42),
        );
    }

    // ------------------------------------------------------------------------------------------------
    // M5 level side step (origin at (-1,0))
    // ------------------------------------------------------------------------------------------------

    const SIDE_FROM: (i32, i32) = (-1, 0);

    #[test]
    fn side_step_ground() {
        let door = O { door: true, ..o() };
        ck("walk", &on(&[]), SIDE_FROM, KF_STAND, o(), res(TT_WALK, KF_STAND, 10));
        ck("from crawl", &on(&[]), SIDE_FROM, KF_CRAWL, o(), res(TT_CRAWL, KF_STAND, 30));
        for p in [KF_HANG_RIGHT, KF_HANG_LEFT] {
            ck("from side hang", &on(&[]), SIDE_FROM, p, o(), res(TT_HANG_TO_STAND_JUMP, KF_STAND, 30));
        }
        ck("from hang back", &on(&[]), SIDE_FROM, KF_HANG_BACK, o(), res(TT_WALK, KF_STAND, 10));
        ck(
            "tunnel, crawler",
            &on(&[(0, 1, S)]),
            SIDE_FROM,
            KF_STAND,
            O { crawl: true, ..o() },
            res(TT_CRAWL, KF_CRAWL, 30),
        );
        ck("tunnel, walker", &on(&[(0, 1, S)]), SIDE_FROM, KF_STAND, o(), REF);
        // the door pairing uses the DESTINATION record with the neighbour's position: a door in the destination
        // takes the floor away, so a tunnel does not help either
        ck("door record in dest, tunnel", &on(&[(0, 1, S), (0, 0, Door)]), SIDE_FROM, KF_STAND, door, REF);
        ck("shallow", &on(&[(0, 0, W)]), SIDE_FROM, KF_STAND, o(), res(TT_WALK, KF_STAND, 10));
        ck("deep", &on(&[(0, 0, F)]), SIDE_FROM, KF_STAND, o(), res(TT_SWIM, KF_BOB, 30));
        ck("solid", &on(&[(0, 0, S)]), SIDE_FROM, KF_STAND, o(), REF);
    }

    #[test]
    fn side_step_fly_and_lift() {
        let fly = O { fly: true, ..o() };
        ck("fly", &on(&[]), SIDE_FROM, KF_STAND, fly, res(TT_FLY, KF_FLY, 4));
        ck("fly crawl", &on(&[]), SIDE_FROM, KF_CRAWL, fly, res(TT_CRAWL, KF_FLY, 30));
        ck("fly side hang", &on(&[]), SIDE_FROM, KF_HANG_LEFT, fly, res(TT_HANG_TO_STAND_JUMP, KF_FLY, 30));
        ck(
            "fly through doors",
            &on(&[(0, 0, DoorS), (0, 1, DoorS)]),
            SIDE_FROM,
            KF_STAND,
            O { door: true, ..fly },
            res(TT_FLY, KF_FLY, 4),
        );
        ck("fly blocked by solid", &on(&[(0, 1, S)]), SIDE_FROM, KF_STAND, fly, REF);
        let shaft = on(&[(0, 0, Sh), (0, 1, Sh)]);
        let lift = O { lift: 1, ..o() };
        ck("lift", &shaft, SIDE_FROM, KF_STAND, lift, res(TT_WALK, KF_STAND_ELEVATOR, 10));
        ck("lift crawl", &shaft, SIDE_FROM, KF_CRAWL, lift, res(TT_CRAWL, KF_STAND_ELEVATOR, 30));
        ck("lift side hang", &shaft, SIDE_FROM, KF_HANG_RIGHT, lift, res(TT_HANG_TO_STAND_JUMP, KF_STAND_ELEVATOR, 30));
        ck(
            "lift blocks flight",
            &shaft,
            SIDE_FROM,
            KF_STAND,
            O { fly: true, lift: 1, ..o() },
            res(TT_WALK, KF_STAND_ELEVATOR, 10),
        );
    }

    #[test]
    fn side_step_without_floor() {
        let none: [(i32, i32, K); 0] = [];
        ck("stairs", &[(0, 0, Stairs)], SIDE_FROM, KF_STAND, o(), res(TT_STAIRS, KF_STAIRS, 12));
        // ladder-like grips
        ck("ladder, stand", &[(0, 0, Lad)], SIDE_FROM, KF_STAND, o(), res(TT_CLIMB, KF_HANG_BACK, 10));
        ck("shaft, stand", &[(0, 0, Sh)], SIDE_FROM, KF_STAND, o(), res(TT_CLIMB, KF_HANG_BACK, 10));
        ck(
            "hatch, stand",
            &[(0, 0, Hatch)],
            SIDE_FROM,
            KF_STAND,
            O { door: true, ..o() },
            res(TT_STAND_TO_HANG_JUMP, KF_HANG_BACK, 40),
        );
        ck("ladder, hang back", &[(0, 0, Lad)], SIDE_FROM, KF_HANG_BACK, o(), res(TT_CLIMB, KF_HANG_BACK, 40));
        ck(
            "ladder, monkey",
            &[(0, 0, Lad)],
            SIDE_FROM,
            KF_MONKEY_HANG,
            o(),
            res(TT_MONKEY_SWING_TO_HANG_DIAGONAL_JUMP_DOWN, KF_HANG_BACK, 40),
        );
        ck(
            "ladder, hang left",
            &[(0, 0, Lad)],
            SIDE_FROM,
            KF_HANG_LEFT,
            o(),
            res(TT_HANG_TO_HANG_JUMP, KF_HANG_BACK, 50),
        );
        // a crawler ignores grips: with nothing else it is refused
        ck("ladder, crawl", &[(0, 0, Lad)], SIDE_FROM, KF_CRAWL, o(), REF);
        // firm grip without headroom: refused at once, walls are not examined
        ck("ladder, no headroom", &[(0, 0, Lad), (0, 1, S), (1, 0, S)], SIDE_FROM, KF_STAND, o(), REF);
        // cactus without headroom: nothing written, walls: ahead needs headroom, behind does not
        ck("cactus, no headroom", &[(0, 0, Cac), (0, 1, S)], SIDE_FROM, KF_STAND, o(), REF);
        ck("cactus, no headroom, ahead", &[(0, 0, Cac), (0, 1, S), (1, 0, S)], SIDE_FROM, KF_STAND, o(), REF);
        ck(
            "cactus, no headroom, behind",
            &[(0, 0, Cac), (0, 1, S), (-1, 0, S)],
            SIDE_FROM,
            KF_STAND,
            o(),
            res(TT_STAND_TO_HANG_JUMP, KF_HANG_LEFT, 40),
        );
        // cactus provisional kept / replaced
        ck("cactus alone", &[(0, 0, Cac)], SIDE_FROM, KF_STAND, o(), res(TT_STAND_TO_HANG_JUMP, KF_HANG_BACK, 40));
        ck(
            "cactus + ahead",
            &[(0, 0, Cac), (1, 0, S)],
            SIDE_FROM,
            KF_STAND,
            o(),
            res(TT_STAND_TO_HANG_JUMP, KF_HANG_RIGHT, 40),
        );
        // walls
        ck("ahead, stand", &[(1, 0, S)], SIDE_FROM, KF_STAND, o(), res(TT_STAND_TO_HANG_JUMP, KF_HANG_RIGHT, 40));
        ck("ahead, same", &[(1, 0, S)], SIDE_FROM, KF_HANG_RIGHT, o(), res(TT_CLIMB, KF_HANG_RIGHT, 40));
        ck(
            "ahead, monkey",
            &[(1, 0, S)],
            SIDE_FROM,
            KF_MONKEY_HANG,
            o(),
            res(TT_MONKEY_SWING_TO_HANG_DIAGONAL_JUMP_DOWN, KF_HANG_RIGHT, 50),
        );
        ck("ahead, other", &[(1, 0, S)], SIDE_FROM, KF_HANG_BACK, o(), res(TT_HANG_TO_HANG_JUMP, KF_HANG_RIGHT, 50));
        ck("ahead, no headroom", &[(1, 0, S), (0, 1, S)], SIDE_FROM, KF_STAND, o(), REF);
        // origin cell as the behind wall
        ck("behind wall", &[(-1, 0, S)], SIDE_FROM, KF_STAND, o(), res(TT_STAND_TO_HANG_JUMP, KF_HANG_LEFT, 40));
        // monkey bars
        ck("monkey", &[(0, 2, S)], SIDE_FROM, KF_STAND, o(), res(TT_MONKEY_SWING, KF_MONKEY_HANG, 50));
        ck(
            "monkey from hang",
            &[(0, 2, S)],
            SIDE_FROM,
            KF_HANG_LEFT,
            o(),
            res(TT_HANG_TO_MONKEY_SWING_DIAGONAL_JUMP_UP, KF_MONKEY_HANG, 50),
        );
        ck("monkey, no headroom", &[(0, 2, S), (0, 1, S)], SIDE_FROM, KF_STAND, o(), REF);
        // water and nothing
        ck(
            "swim in door water",
            &[(0, 0, DoorF)],
            SIDE_FROM,
            KF_STAND,
            O { door: true, ..o() },
            res(TT_SWIM, KF_BOB, 30),
        );
        ck("nothing", &none, SIDE_FROM, KF_STAND, o(), REF);
        // a door record in the destination takes away the floor (record of C, position of C-1)
        ck("door floor", &on(&[(0, 0, Door)]), SIDE_FROM, KF_STAND, O { door: true, ..o() }, REF);
    }

    // ------------------------------------------------------------------------------------------------
    // gap jump
    // ------------------------------------------------------------------------------------------------

    const GAP_FROM: (i32, i32) = (-2, 0);

    fn gap(extra: Cells) -> Vec<(i32, i32, K)> {
        // floor under the destination, a hole under the gap
        let mut v = vec![(0, -1, S), (-2, -1, S)];
        v.extend_from_slice(extra);
        v
    }

    #[test]
    fn gap_jump_rules() {
        let j = res(TT_JUMP_OVER_GAP, KF_STAND, 25);
        let door = O { door: true, ..o() };
        ck("jump", &gap(&[]), GAP_FROM, KF_STAND, o(), j);
        ck("mirror", &[(0, -1, S), (2, -1, S)], (2, 0), KF_STAND, o(), j);
        ck("over solid ground", &on(&[]), GAP_FROM, KF_STAND, o(), j);
        ck("hang back may jump", &gap(&[]), GAP_FROM, KF_HANG_BACK, o(), j);
        for p in [KF_CRAWL, KF_HANG_RIGHT, KF_HANG_LEFT, KF_FALLING] {
            ck("pose refused", &gap(&[]), GAP_FROM, p, o(), REF);
        }
        ck("falling + fly", &gap(&[]), GAP_FROM, KF_FALLING, O { fly: true, ..o() }, REF);
        ck("elevator, solid origin", &gap(&[(-2, 0, S)]), GAP_FROM, KF_STAND_ELEVATOR, o(), REF);
        ck("elevator, door origin", &gap(&[(-2, 0, DoorS)]), GAP_FROM, KF_STAND_ELEVATOR, door, j);
        ck("elevator, open origin", &gap(&[]), GAP_FROM, KF_STAND_ELEVATOR, o(), j);
        ck("other row", &gap(&[]), (-2, 1), KF_STAND, o(), REF);
        ck("gap cell solid", &gap(&[(-1, 0, S)]), GAP_FROM, KF_STAND, o(), REF);
        ck("gap cell shallow", &gap(&[(-1, 0, W)]), GAP_FROM, KF_STAND, o(), j);
        ck("gap cell door", &gap(&[(-1, 0, DoorS)]), GAP_FROM, KF_STAND, door, j);
        ck("gap head solid", &gap(&[(-1, 1, S)]), GAP_FROM, KF_STAND, o(), REF);
        ck("gap head water surface", &gap(&[(-1, 1, W)]), GAP_FROM, KF_STAND, o(), j);
        ck("gap head submerged", &gap(&[(-1, 1, W), (-1, 2, W)]), GAP_FROM, KF_STAND, o(), REF);
        ck("dest solid", &gap(&[(0, 0, S)]), GAP_FROM, KF_STAND, o(), REF);
        ck("dest water surface", &gap(&[(0, 0, W)]), GAP_FROM, KF_STAND, o(), j);
        ck("dest submerged", &gap(&[(0, 0, W), (0, 1, W)]), GAP_FROM, KF_STAND, o(), REF);
        ck("dest door", &gap(&[(0, 0, DoorS)]), GAP_FROM, KF_STAND, door, j);
        ck("no floor", &[], GAP_FROM, KF_STAND, o(), REF);
        ck("door floor", &gap(&[(0, -1, DoorS)]), GAP_FROM, KF_STAND, door, REF);
        ck("low ceiling", &gap(&[(0, 1, S)]), GAP_FROM, KF_STAND, o(), REF);
        ck("door ceiling", &gap(&[(0, 1, DoorS)]), GAP_FROM, KF_STAND, door, j);
    }

    // ------------------------------------------------------------------------------------------------
    // V1 up one (origin (0,-1); the horizontal offset is irrelevant)
    // ------------------------------------------------------------------------------------------------

    const UP1_FROM: (i32, i32) = (0, -1);

    #[test]
    fn up_one_basics() {
        let door = O { door: true, ..o() };
        let fly = O { fly: true, ..o() };
        ck("fly", &[], UP1_FROM, KF_STAND, fly, res(TT_FLY, KF_FLY, 4));
        ck(
            "fly via hatches",
            &[(0, 0, HatchS), (0, 1, HatchS)],
            UP1_FROM,
            KF_STAND,
            O { door: true, ..fly },
            res(TT_FLY, KF_FLY, 4),
        );
        ck("falling fast + fly", &[], UP1_FROM, KF_FALLING, O { pc: 120, ..fly }, res(TT_FLY, KF_FLY, 4));
        ck("too fast", &[(0, 0, Lad)], UP1_FROM, KF_FALLING, O { pc: 100, ..o() }, REF);
        ck(
            "falling, not too fast",
            &[(0, 0, Lad)],
            UP1_FROM,
            KF_FALLING,
            O { pc: 99, ..o() },
            res(TT_HANG_TO_HANG_JUMP, KF_HANG_BACK, 50),
        );
        // lift
        let shaft = [(0, 0, Sh), (0, 1, Sh)];
        ck("lift", &shaft, UP1_FROM, KF_STAND, O { lift: 1, ..o() }, res(TT_ELEVATOR, KF_STAND_ELEVATOR, 5));
        ck(
            "lift blocks flight",
            &shaft,
            UP1_FROM,
            KF_STAND,
            O { lift: 1, ..fly },
            res(TT_ELEVATOR, KF_STAND_ELEVATOR, 5),
        );
        ck(
            "lift, falling",
            &shaft,
            UP1_FROM,
            KF_FALLING,
            O { lift: 1, ..o() },
            res(TT_HANG_TO_HANG_JUMP, KF_HANG_BACK, 50),
        );
        ck("unpowered shaft climbs", &shaft, UP1_FROM, KF_STAND, O { lift: 2, ..o() }, res(TT_CLIMB, KF_HANG_BACK, 20));
        // water / passability
        ck("solid dest", &[(0, 0, S)], UP1_FROM, KF_STAND, o(), REF);
        ck("deep dest", &[(0, 0, F)], UP1_FROM, KF_STAND, o(), res(TT_SWIM, KF_BOB, 30));
        ck("ceiling", &[(0, 1, S), (-1, 0, S)], UP1_FROM, KF_STAND, o(), REF);
        ck(
            "shallow above",
            &[(0, 1, W), (-1, 0, S)],
            UP1_FROM,
            KF_STAND,
            o(),
            res(TT_STAND_TO_HANG_JUMP, KF_HANG_LEFT, 40),
        );
        ck(
            "hatch above",
            &[(0, 1, HatchS), (-1, 0, S)],
            UP1_FROM,
            KF_STAND,
            door,
            res(TT_STAND_TO_HANG_JUMP, KF_HANG_LEFT, 40),
        );
        // any horizontal offset beyond 2 is vertical too
        ck("far offset", &[(0, 0, Lad)], (5, -1), KF_STAND, o(), res(TT_CLIMB, KF_HANG_BACK, 20));
        ck("far offset, negative", &[(0, 0, Lad)], (-7, -1), KF_STAND, o(), res(TT_CLIMB, KF_HANG_BACK, 20));
    }

    #[test]
    fn up_one_grips_and_walls() {
        let door = O { door: true, ..o() };
        ck("ladder, stand", &[(0, 0, Lad)], UP1_FROM, KF_STAND, o(), res(TT_CLIMB, KF_HANG_BACK, 20));
        ck("ladder, hang back", &[(0, 0, Lad)], UP1_FROM, KF_HANG_BACK, o(), res(TT_CLIMB, KF_HANG_BACK, 20));
        ck("ladder, other", &[(0, 0, Lad)], UP1_FROM, KF_HANG_LEFT, o(), res(TT_HANG_TO_HANG_JUMP, KF_HANG_BACK, 50));
        ck("hatch grip", &[(0, 0, Hatch)], UP1_FROM, KF_STAND, door, res(TT_CLIMB, KF_HANG_BACK, 20));
        ck(
            "cactus + ladder is firm",
            &[(0, 0, CacLad), (-1, 0, S)],
            UP1_FROM,
            KF_STAND,
            o(),
            res(TT_CLIMB, KF_HANG_BACK, 20),
        );
        // cactus: provisional
        ck("cactus alone", &[(0, 0, Cac)], UP1_FROM, KF_STAND, o(), res(TT_CLIMB, KF_HANG_BACK, 40));
        ck(
            "cactus + left wall",
            &[(0, 0, Cac), (-1, 0, S)],
            UP1_FROM,
            KF_STAND,
            o(),
            res(TT_STAND_TO_HANG_JUMP, KF_HANG_LEFT, 40),
        );
        ck(
            "cactus + monkey",
            &[(0, 0, Cac), (0, 2, S)],
            UP1_FROM,
            KF_STAND,
            o(),
            res(TT_STAND_TO_MONKEY_HANG_JUMP, KF_MONKEY_HANG, 42),
        );
        ck("cactus water swim", &[(0, 0, HatchF)], UP1_FROM, KF_STAND, door, res(TT_SWIM, KF_BOB, 30));
        // walls
        ck("left, stand", &[(-1, 0, S)], UP1_FROM, KF_STAND, o(), res(TT_STAND_TO_HANG_JUMP, KF_HANG_LEFT, 40));
        ck("left, same", &[(-1, 0, S)], UP1_FROM, KF_HANG_LEFT, o(), res(TT_CLIMB, KF_HANG_LEFT, 40));
        ck("left, other", &[(-1, 0, S)], UP1_FROM, KF_HANG_BACK, o(), res(TT_HANG_TO_HANG_JUMP, KF_HANG_LEFT, 50));
        ck("right, stand", &[(1, 0, S)], UP1_FROM, KF_STAND, o(), res(TT_STAND_TO_HANG_JUMP, KF_HANG_RIGHT, 40));
        ck("right, same", &[(1, 0, S)], UP1_FROM, KF_HANG_RIGHT, o(), res(TT_CLIMB, KF_HANG_RIGHT, 40));
        ck("right, other", &[(1, 0, S)], UP1_FROM, KF_HANG_LEFT, o(), res(TT_HANG_TO_HANG_JUMP, KF_HANG_RIGHT, 50));
        ck(
            "left wins",
            &[(-1, 0, S), (1, 0, S)],
            UP1_FROM,
            KF_STAND,
            o(),
            res(TT_STAND_TO_HANG_JUMP, KF_HANG_LEFT, 40),
        );
        ck("monkey", &[(0, 2, S)], UP1_FROM, KF_STAND, o(), res(TT_STAND_TO_MONKEY_HANG_JUMP, KF_MONKEY_HANG, 42));
        ck("nothing", &[], UP1_FROM, KF_STAND, o(), REF);
    }

    // ------------------------------------------------------------------------------------------------
    // V2 down one (origin (0,1))
    // ------------------------------------------------------------------------------------------------

    const DN_FROM: (i32, i32) = (0, 1);

    #[test]
    fn down_one_landing() {
        let landing = [(0, -1, S)];
        ck("land", &landing, DN_FROM, KF_STAND, o(), res(TT_FALL, KF_STAND, 20));
        ck("land from falling", &landing, DN_FROM, KF_FALLING, O { pc: 69, ..o() }, res(TT_FALL, KF_STAND, 79));
        ck(
            "land, fall too costly",
            &landing,
            DN_FROM,
            KF_FALLING,
            O { pc: 70, ..o() },
            (TT_INACCESSIBLE, KF_INACCESSIBLE, 80, 0),
        );
        ck(
            "land, fall search",
            &landing,
            DN_FROM,
            KF_FALLING,
            O { pc: 70, fall: true, ..o() },
            res(TT_FALL, KF_STAND, 80),
        );
        ck("side hang lands", &landing, DN_FROM, KF_HANG_LEFT, o(), res(TT_HANG_TO_STAND_JUMP, KF_STAND, 30));
        // too fast: grip and sides are skipped
        ck(
            "too fast",
            &[(0, -1, S), (0, 0, Lad)],
            DN_FROM,
            KF_FALLING,
            O { pc: 100, ..o() },
            (TT_INACCESSIBLE, KF_INACCESSIBLE, 110, 0),
        );
        ck(
            "too fast, fall search",
            &[(0, -1, S), (0, 0, Lad)],
            DN_FROM,
            KF_FALLING,
            O { pc: 100, fall: true, ..o() },
            res(TT_FALL, KF_STAND, 110),
        );
        // grips
        ck("ladder", &[(0, -1, S), (0, 0, Lad)], DN_FROM, KF_STAND, o(), res(TT_CLIMB, KF_HANG_BACK, 20));
        ck("hatch", &[(0, 0, Hatch)], DN_FROM, KF_STAND, O { door: true, ..o() }, res(TT_CLIMB, KF_HANG_BACK, 20));
        ck("shaft", &[(0, 0, Sh)], DN_FROM, KF_STAND, o(), res(TT_CLIMB, KF_HANG_BACK, 20));
        ck("cactus then landing", &[(0, -1, S), (0, 0, Cac)], DN_FROM, KF_STAND, o(), res(TT_FALL, KF_STAND, 20));
        // a rejected fall keeps the provisional kind / pose but changes the cost
        ck(
            "cactus then rejected fall",
            &[(0, -1, S), (0, 0, Cac)],
            DN_FROM,
            KF_FALLING,
            O { pc: 70, ..o() },
            (TT_CLIMB, KF_HANG_BACK, 80, 0),
        );
    }

    #[test]
    fn down_one_air() {
        // side probe: left first by default
        ck("left", &[(-1, 0, S)], DN_FROM, KF_STAND, o(), res(TT_CLIMB, KF_HANG_LEFT, 40));
        ck("right", &[(1, 0, S)], DN_FROM, KF_STAND, o(), res(TT_CLIMB, KF_HANG_RIGHT, 40));
        ck("both: left", &[(-1, 0, S), (1, 0, S)], DN_FROM, KF_STAND, o(), res(TT_CLIMB, KF_HANG_LEFT, 40));
        ck(
            "both, hanging right: right",
            &[(-1, 0, S), (1, 0, S)],
            DN_FROM,
            KF_HANG_RIGHT,
            o(),
            res(TT_CLIMB, KF_HANG_RIGHT, 40),
        );
        ck("hanging right, only left", &[(-1, 0, S)], DN_FROM, KF_HANG_RIGHT, o(), res(TT_CLIMB, KF_HANG_LEFT, 40));
        // too fast: no side probe
        ck(
            "too fast, wall ignored",
            &[(-1, 0, S)],
            DN_FROM,
            KF_FALLING,
            O { pc: 100, fall: true, ..o() },
            res(TT_FALL, KF_FALLING, 110),
        );
        ck("monkey", &[(0, 2, S)], DN_FROM, KF_STAND, o(), res(TT_STAND_TO_MONKEY_HANG_JUMP, KF_MONKEY_HANG, 42));
        ck("fall", &[], DN_FROM, KF_STAND, o(), res(TT_FALL, KF_FALLING, 20));
        ck("fall from falling", &[], DN_FROM, KF_FALLING, O { pc: 50, ..o() }, res(TT_FALL, KF_FALLING, 60));
        ck("fall too costly", &[], DN_FROM, KF_FALLING, O { pc: 70, ..o() }, REF);
        ck("fall search", &[], DN_FROM, KF_FALLING, O { pc: 70, fall: true, ..o() }, res(TT_FALL, KF_FALLING, 80));
        // cactus with water: swim replaces the provisional
        ck("water cactus", &[(0, 0, HatchF)], DN_FROM, KF_STAND, O { door: true, ..o() }, res(TT_SWIM, KF_BOB, 30));
        ck("deep", &[(0, 0, F), (0, -1, S)], DN_FROM, KF_STAND, o(), res(TT_SWIM, KF_BOB, 30));
        ck("solid", &[(0, 0, S)], DN_FROM, KF_STAND, o(), REF);
    }

    #[test]
    fn down_one_fly_and_lift() {
        let fly = O { fly: true, ..o() };
        ck("fly", &[], DN_FROM, KF_STAND, fly, res(TT_FLY, KF_FLY, 4));
        ck("fly via hatch", &[(0, 0, HatchS)], DN_FROM, KF_STAND, O { door: true, ..fly }, res(TT_FLY, KF_FLY, 4));
        let shaft = [(0, 0, Sh), (0, 1, Sh)];
        ck("lift", &shaft, DN_FROM, KF_STAND, O { lift: 1, ..o() }, res(TT_ELEVATOR, KF_STAND_ELEVATOR, 5));
        ck(
            "lift blocks flight",
            &shaft,
            DN_FROM,
            KF_STAND,
            O { lift: 1, ..fly },
            res(TT_ELEVATOR, KF_STAND_ELEVATOR, 5),
        );
        ck("falling ignores lift", &shaft, DN_FROM, KF_FALLING, O { lift: 1, ..fly }, res(TT_FLY, KF_FLY, 4));
    }

    // ------------------------------------------------------------------------------------------------
    // V3 up two (origin (0,-2))
    // ------------------------------------------------------------------------------------------------

    const UP2_FROM: (i32, i32) = (0, -2);

    #[test]
    fn up_two_double_jump() {
        let door = O { door: true, ..o() };
        ck("nothing", &[], UP2_FROM, KF_STAND, o(), REF);
        ck("falling", &[(-1, 0, S)], UP2_FROM, KF_FALLING, o(), REF);
        ck(
            "flying changes nothing",
            &[(-1, 0, S)],
            UP2_FROM,
            KF_STAND,
            O { fly: true, ..o() },
            res(TT_STAND_TO_HANG_DOUBLE_JUMP, KF_HANG_LEFT, 65),
        );
        ck("below solid", &[(0, -1, S), (-1, 0, S)], UP2_FROM, KF_STAND, o(), REF);
        ck(
            "below hatch",
            &[(0, -1, HatchS), (-1, 0, S)],
            UP2_FROM,
            KF_STAND,
            door,
            res(TT_STAND_TO_HANG_DOUBLE_JUMP, KF_HANG_LEFT, 65),
        );
        ck(
            "below shallow",
            &[(0, -1, W), (-1, 0, S)],
            UP2_FROM,
            KF_STAND,
            o(),
            res(TT_STAND_TO_HANG_DOUBLE_JUMP, KF_HANG_LEFT, 65),
        );
        ck("dest solid", &[(0, 0, S)], UP2_FROM, KF_STAND, o(), REF);
        ck(
            "dest hatch",
            &[(0, 0, HatchS)],
            UP2_FROM,
            KF_STAND,
            door,
            res(TT_STAND_TO_HANG_DOUBLE_JUMP, KF_HANG_BACK, 65),
        );
        ck("above solid", &[(0, 1, S), (-1, 0, S)], UP2_FROM, KF_STAND, o(), REF);
        ck(
            "above hatch",
            &[(0, 1, HatchS), (-1, 0, S)],
            UP2_FROM,
            KF_STAND,
            door,
            res(TT_STAND_TO_HANG_DOUBLE_JUMP, KF_HANG_LEFT, 65),
        );
        // grips
        ck(
            "ladder, stand",
            &[(0, 0, Lad)],
            UP2_FROM,
            KF_STAND,
            o(),
            res(TT_STAND_TO_HANG_DOUBLE_JUMP, KF_HANG_BACK, 65),
        );
        ck("ladder, same", &[(0, 0, Lad)], UP2_FROM, KF_HANG_BACK, o(), res(TT_CLIMB, KF_HANG_BACK, 65));
        ck("ladder, other", &[(0, 0, Lad)], UP2_FROM, KF_HANG_RIGHT, o(), res(TT_HANG_TO_HANG_JUMP, KF_HANG_BACK, 65));
        ck(
            "cactus alone",
            &[(0, 0, Cac)],
            UP2_FROM,
            KF_STAND,
            o(),
            res(TT_STAND_TO_HANG_DOUBLE_JUMP, KF_HANG_BACK, 65),
        );
        ck(
            "cactus + right",
            &[(0, 0, Cac), (1, 0, S)],
            UP2_FROM,
            KF_STAND,
            o(),
            res(TT_STAND_TO_HANG_DOUBLE_JUMP, KF_HANG_RIGHT, 65),
        );
        // walls
        ck("left, same", &[(-1, 0, S)], UP2_FROM, KF_HANG_LEFT, o(), res(TT_CLIMB, KF_HANG_LEFT, 65));
        ck("left, other", &[(-1, 0, S)], UP2_FROM, KF_HANG_BACK, o(), res(TT_HANG_TO_HANG_JUMP, KF_HANG_LEFT, 65));
        ck("right, stand", &[(1, 0, S)], UP2_FROM, KF_STAND, o(), res(TT_STAND_TO_HANG_DOUBLE_JUMP, KF_HANG_RIGHT, 65));
        ck("right, same", &[(1, 0, S)], UP2_FROM, KF_HANG_RIGHT, o(), res(TT_CLIMB, KF_HANG_RIGHT, 65));
        ck("right, other", &[(1, 0, S)], UP2_FROM, KF_HANG_LEFT, o(), res(TT_HANG_TO_HANG_JUMP, KF_HANG_RIGHT, 65));
        ck(
            "left wins",
            &[(-1, 0, S), (1, 0, S)],
            UP2_FROM,
            KF_STAND,
            o(),
            res(TT_STAND_TO_HANG_DOUBLE_JUMP, KF_HANG_LEFT, 65),
        );
    }

    // ------------------------------------------------------------------------------------------------
    // V4 stay in place (origin == destination)
    // ------------------------------------------------------------------------------------------------

    const STAY: (i32, i32) = (0, 0);

    #[test]
    fn stay_in_place() {
        let fly = O { fly: true, ..o() };
        ck("solid", &[(0, 0, S)], STAY, KF_STAND, o(), res(TT_NONE, KF_STAND, 10));
        ck("fly", &[], STAY, KF_STAND, fly, res(TT_NONE, KF_FLY, 4));
        ck("deep", &[(0, 0, F)], STAY, KF_STAND, o(), res(TT_SWIM, KF_BOB, 30));
        ck("ground", &on(&[]), STAY, KF_STAND, o(), res(TT_NONE, KF_STAND, 10));
        ck("shallow ground", &on(&[(0, 0, W)]), STAY, KF_STAND, o(), res(TT_NONE, KF_STAND, 10));
        ck("ground, low ceiling", &on(&[(0, 1, S)]), STAY, KF_STAND, o(), res(TT_NONE, KF_CRAWL, 30));
        ck("ground, shallow above", &on(&[(0, 1, W)]), STAY, KF_STAND, o(), res(TT_NONE, KF_STAND, 10));
        ck(
            "ground, hatch above",
            &on(&[(0, 1, HatchS)]),
            STAY,
            KF_STAND,
            O { door: true, ..o() },
            res(TT_NONE, KF_STAND, 10),
        );
        ck("air, ceiling", &[(0, 1, S)], STAY, KF_STAND, o(), res(TT_FALL, KF_FALLING, 20));
        ck("mid air", &[], STAY, KF_STAND, o(), res(TT_FALL, KF_FALLING, 20));
        ck("other column", &[], (3, 0), KF_STAND, o(), REF);
        ck("other row", &[], (0, 3), KF_STAND, o(), REF);
    }

    #[test]
    fn stay_grips_and_walls() {
        let shaft = [(0, 0, Sh), (0, 1, Sh)];
        ck("ladder", &[(0, 0, Lad)], STAY, KF_STAND, o(), res(TT_NONE, KF_HANG_BACK, 30));
        ck("lift", &shaft, STAY, KF_STAND, O { lift: 1, ..o() }, res(TT_NONE, KF_STAND_ELEVATOR, 5));
        ck(
            "lift, fall search",
            &shaft,
            STAY,
            KF_STAND,
            O { lift: 1, fall: true, ..o() },
            res(TT_NONE, KF_HANG_BACK, 30),
        );
        ck("unpowered shaft", &shaft, STAY, KF_STAND, O { lift: 2, ..o() }, res(TT_NONE, KF_HANG_BACK, 30));
        ck("shaft without lift above", &[(0, 0, Sh)], STAY, KF_STAND, o(), res(TT_NONE, KF_HANG_BACK, 30));
        // cactus: provisional, walls replace, otherwise kept
        ck("cactus alone", &[(0, 0, Cac)], STAY, KF_STAND, o(), res(TT_NONE, KF_HANG_BACK, 30));
        ck("cactus + left", &[(0, 0, Cac), (-1, 0, S)], STAY, KF_STAND, o(), res(TT_NONE, KF_HANG_LEFT, 30));
        ck("cactus + right", &[(0, 0, Cac), (1, 0, S)], STAY, KF_STAND, o(), res(TT_NONE, KF_HANG_RIGHT, 30));
        ck(
            "cactus, hanging right, both",
            &[(0, 0, Cac), (-1, 0, S), (1, 0, S)],
            STAY,
            KF_HANG_RIGHT,
            o(),
            res(TT_NONE, KF_HANG_RIGHT, 30),
        );
        // no grip
        ck("wall left", &[(-1, 0, S)], STAY, KF_STAND, o(), res(TT_NONE, KF_HANG_LEFT, 30));
        ck("wall right", &[(1, 0, S)], STAY, KF_STAND, o(), res(TT_NONE, KF_HANG_RIGHT, 30));
        ck("stairs", &[(0, 0, Stairs)], STAY, KF_STAND, o(), res(TT_NONE, KF_STAIRS, 30));
        ck("stairs, roof", &[(0, 0, Stairs), (0, 2, S)], STAY, KF_STAND, o(), res(TT_NONE, KF_MONKEY_HANG, 30));
        ck("water", &[(0, 0, W)], STAY, KF_STAND, o(), res(TT_SWIM, KF_BOB, 30));
        ck("monkey bars", &[(0, 2, S)], STAY, KF_STAND, o(), res(TT_NONE, KF_MONKEY_HANG, 30));
        ck("roof and ceiling", &[(0, 2, S), (0, 1, Snow)], STAY, KF_STAND, o(), res(TT_NONE, KF_MONKEY_HANG, 30));
        ck("flying in shallow above", &[(0, 1, W)], STAY, KF_STAND, O { fly: true, ..o() }, res(TT_NONE, KF_STAND, 10));
    }
}
