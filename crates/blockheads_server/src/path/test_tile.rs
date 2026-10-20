//! Neighbour evaluation (`test_tile`) of the best-first path search driven by
//! [`super::path_creator::PathCreator`].
//!
//! One call evaluates a single candidate cell `T` reached from a parent cell `P`: it asks the single-move
//! rules (`check_can_enter_tile`) whether the move is possible, adjusts the verdict for the goal cell,
//! decides whether the goal can actually be acted on from `P` ("placeable"), and finally updates the open
//! set and `T`'s search record.
//!
//! The pure decision rules (goal classification, goal-cell acceptance, record improvement) are small
//! free functions over plain values so that every row of the decision table can be tested directly. The
//! cell-dependent rules live on [`Eval`].
//!
//! Aliasing: `T` and `P` may be the same cell (start cell, self-evaluation of an unplaceable goal), in which
//! case both record handles name one slot. All record reads therefore go through the slot at the moment of
//! use, in the documented order.

use std::rc::Rc;

use crate::foundation::indexset::IndexSet;
use crate::objects::handle::ObjRef;
use crate::types::enums::{InteractionType, TileTraverseKeyFrameType};
use crate::types::structs::{IntPair, PathTestResult};
use crate::world::helpers::{
    tile_contains_usable_door, tile_contains_usable_trap_door, tile_is_air_water_or_snow, tile_is_solid, tile_is_water,
};
use crate::world::state::{TileRef, World};
use crate::world::tiles::{tile_at_world_index_loaded, tile_at_world_position_loaded, world_index_at_world_position};

use super::can_enter::{check_can_enter_tile, FromProps};
use super::path_creator::{dw_blockhead_occupies_tile, ns_index, wrap_dx, PathCreator, PropsSlot, SubStep};

const POSE_CRAWL: u32 = TileTraverseKeyFrameType::TRAVERSE_KF_CRAWL.0;
const POSE_HANG_BACK: u32 = TileTraverseKeyFrameType::TRAVERSE_KF_HANG_BACK.0;
const POSE_BOB: u32 = TileTraverseKeyFrameType::TRAVERSE_KF_BOB.0;
const POSE_FALLING: u32 = TileTraverseKeyFrameType::TRAVERSE_KF_FALLING.0;
const POSE_INACCESSIBLE: u32 = TileTraverseKeyFrameType::TRAVERSE_KF_INACCESSIBLE.0;
const POSE_STAND_ELEVATOR: u32 = TileTraverseKeyFrameType::TRAVERSE_KF_STAND_ELEVATOR.0;
const KIND_INACCESSIBLE: u32 = 25;

const TILE_AIR: u8 = 2;
const CONTENTS_CACTUS: u8 = 43;
const FOREGROUND_LADDER: u8 = 66;
/// Longest sub-path (in steps) a cell record may carry.
const SUBPATH_LIMIT: usize = 0x40;

const A_NONE: i32 = InteractionType::INTERACTION_NONE.0 as i32;
const A_STAB: i32 = InteractionType::INTERACTION_STAB.0 as i32;
const A_RIDE: i32 = InteractionType::INTERACTION_RIDE.0 as i32;
const A_CAPTURE: i32 = InteractionType::INTERACTION_CAPTURE.0 as i32;
const A_FIRE_RANGED: i32 = InteractionType::INTERACTION_FIRE_RANGED_WEAPON.0 as i32;
const A_FEED_NPC: i32 = InteractionType::INTERACTION_FEED_NPC.0 as i32;
const A_INSPECT_NPC: i32 = InteractionType::INTERACTION_INSPECT_NPC.0 as i32;
const A_HARVEST_NPC: i32 = InteractionType::INTERACTION_HARVEST_NPC.0 as i32;
const A_SET_NPC_FREE: i32 = InteractionType::INTERACTION_SET_NPC_FREE.0 as i32;
const A_DIG: i32 = InteractionType::INTERACTION_DIG.0 as i32;
const A_DIG_FOREGROUND: i32 = InteractionType::INTERACTION_DIG_FOREGROUND.0 as i32;
const A_PLACE_ITEM: i32 = InteractionType::INTERACTION_PLACE_ITEM.0 as i32;
const A_PLACE_SOLID: i32 = InteractionType::INTERACTION_PLACE_SOLID_BLOCK.0 as i32;
const A_PICKUP: i32 = InteractionType::INTERACTION_PICKUP.0 as i32;
const A_PICKUP_FREEBLOCK: i32 = InteractionType::INTERACTION_PICKUP_FREEBLOCK.0 as i32;

// ------------------------------------------------------------------------------------------------
// Pure decision rules
// ------------------------------------------------------------------------------------------------

/// True when the goal is an action on a cell (as opposed to on a creature or nothing).
fn is_tile_goal(interaction: i32) -> bool {
    !matches!(
        interaction,
        A_NONE
            | A_STAB
            | A_RIDE
            | A_CAPTURE
            | A_HARVEST_NPC
            | A_SET_NPC_FREE
            | A_FEED_NPC
            | A_INSPECT_NPC
            | A_FIRE_RANGED
    )
}

/// Interactions that only need a standing place when the goal cell would make the parent fall away.
fn is_fall_tolerant(interaction: i32) -> bool {
    matches!(interaction, A_PLACE_ITEM | A_PLACE_SOLID | A_PICKUP | A_PICKUP_FREEBLOCK)
}

/// Interactions that remove the goal tile and therefore need the parent to stay supported.
fn removes_tile(interaction: i32) -> bool {
    matches!(interaction, A_DIG | A_DIG_FOREGROUND | A_PICKUP)
}

/// Whether the move into `T` is usable at all: any pose but INACCESSIBLE, except that a fall that lands
/// exactly on the goal cell does not count.
fn base_acceptance(pose: u32, lands_on_goal_column_and_row: bool) -> bool {
    pose != POSE_INACCESSIBLE && !(pose == POSE_FALLING && lands_on_goal_column_and_row)
}

/// Standing next to (or on) the target without falling is required for the goal cell.
fn proximity_ok(dxp: i32, parent_pose: u32) -> bool {
    dxp.wrapping_abs() < 2 && parent_pose != POSE_FALLING
}

/// Inputs of the goal-cell acceptance table.
#[derive(Clone, Copy, Debug)]
struct GoalCase {
    /// Verdict from [`base_acceptance`].
    accepted: bool,
    tile_goal: bool,
    /// `Ty - Py`.
    dy: i32,
    /// `Py - Ty`.
    parent_above: i32,
    /// Wrapped `Px - Tx`.
    dxp: i32,
    parent_pose: u32,
}

/// Result of the goal-cell acceptance table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct GoalVerdict {
    accept: bool,
    unplaceable: bool,
    /// The lift-exit adjustment must be applied by the caller (it needs the world).
    lift_exit: bool,
}

/// Goal-cell adjustments (spec 3, step 7). Only meaningful when `T` is the goal cell.
fn goal_verdict(c: GoalCase) -> GoalVerdict {
    let mut v = GoalVerdict { accept: c.accepted, unplaceable: false, lift_exit: false };
    let gate = |v: &mut GoalVerdict| {
        if !proximity_ok(c.dxp, c.parent_pose) {
            v.accept = false;
        }
        v.lift_exit = c.tile_goal && c.parent_pose == POSE_STAND_ELEVATOR;
    };
    if !c.accepted {
        // rescue: the goal may be acted upon from a neighbouring standing place
        v.accept = c.tile_goal && c.dy >= -1 && (c.dy <= 1 || c.parent_pose != POSE_CRAWL);
        if !v.accept {
            v.accept = c.dxp == 0 && (-1..=1).contains(&c.dy);
        }
        if v.accept {
            gate(&mut v);
        }
    } else {
        if c.tile_goal {
            if c.dxp.wrapping_abs() >= 2 {
                v.accept = false;
            } else if c.parent_above > 1 {
                v.unplaceable = true;
            }
        }
        if v.accept {
            gate(&mut v);
        }
    }
    v
}

/// Cost multiplier for hanging on the back of a cactus without a ladder (spec 3, step 4).
fn cactus_cost(cost: i32, contents: u8, foreground: u8, pose: u32) -> i32 {
    if contents == CONTENTS_CACTUS && pose == POSE_HANG_BACK && foreground != FOREGROUND_LADDER {
        cost.wrapping_mul(10)
    } else {
        cost
    }
}

/// Whether a second visit of an already-open cell replaces its record (spec 3, step 9.3).
fn revisit_improves(new_g: i32, old: (i32, u32, bool), new_pose: u32, placeable: bool) -> bool {
    let (old_g, old_pose, old_unplaceable) = old;
    let was_inaccessible = old_pose == POSE_INACCESSIBLE;
    let now_accessible = new_pose != POSE_INACCESSIBLE;
    (new_g < old_g && (was_inaccessible || now_accessible))
        || (was_inaccessible && now_accessible)
        || (placeable && old_unplaceable)
}

/// Heuristic: ten per cell of wrapped manhattan distance.
fn heuristic(dx_wrapped: i32, dy: i32) -> i32 {
    dx_wrapped.wrapping_abs().wrapping_add(dy.wrapping_abs()).wrapping_mul(10)
}

// ------------------------------------------------------------------------------------------------
// Evaluation context
// ------------------------------------------------------------------------------------------------

struct Eval<'a> {
    tx: i32,
    ty: i32,
    gx: i32,
    gy: i32,
    interaction: i32,
    px: i32,
    py: i32,
    parent_cell: Option<TileRef>,
    parent: PropsSlot,
    user: Option<&'a ObjRef>,
    world: &'a Rc<World>,
    pc: &'a PathCreator,
    blacklist: &'a [IntPair],
    wm: i32,
}

impl Eval<'_> {
    fn parent_pos(&self) -> IntPair {
        IntPair { x: self.px, y: self.py }
    }

    fn parent_pose(&self) -> u32 {
        self.pc.with_props(self.parent, |p| p.traverse_to_key_frame as u32)
    }

    fn cell(&self, x: i32, y: i32) -> Option<TileRef> {
        tile_at_world_position_loaded(x, y, self.world)
    }

    fn above_parent(&self) -> Option<TileRef> {
        self.cell(self.px, self.py.wrapping_add(1))
    }

    fn door_or_trap_door_at(&self, cell: Option<TileRef>, pos: IntPair) -> bool {
        tile_contains_usable_door(cell, pos, self.world, self.user)
            || tile_contains_usable_trap_door(cell, pos, self.world, self.user)
    }

    /// Single-move rules from the parent to `dest`, using the parent record as it is right now.
    fn can_enter(
        &self,
        dest: Option<TileRef>,
        origin: Option<TileRef>,
        (ox, oy): (i32, i32),
        (dx, dy): (i32, i32),
        fall: bool,
        water: bool,
    ) -> PathTestResult {
        let from = self.pc.with_props(self.parent, |p| FromProps::from(p));
        check_can_enter_tile(dest, origin, from, ox, oy, dx, dy, self.user, self.world, fall, water, self.blacklist)
    }

    /// Lift-exit adjustment: the parent stands in a lift shaft; it needs headroom or the goal is unplaceable.
    fn lift_exit(&self, unplaceable: &mut bool) {
        if tile_is_solid(self.parent_cell) {
            *unplaceable = true;
        } else {
            let above = self.above_parent();
            if above.is_none() || tile_is_solid(above) {
                *unplaceable = true;
            } else {
                self.pc.with_props_mut(self.parent, |p| p.traverse_to_key_frame = POSE_HANG_BACK as i32);
            }
        }
    }

    /// Would removing `cell` leave the parent without support (spec 3, step 8.9)?
    fn parent_would_be_trapped(&self, cell: TileRef) -> bool {
        let saved = (cell.type_index(), cell.contents(), cell.foreground_contents());
        cell.set_type_index(TILE_AIR);
        cell.set_contents(0);
        cell.set_foreground_contents(0);
        let here = (self.px, self.py);
        let stay = self.can_enter(self.parent_cell, self.parent_cell, here, here, true, true);
        let mut trapped = false;
        if stay.traverse_type == KIND_INACCESSIBLE || stay.traverse_to_key_frame == POSE_FALLING {
            trapped = match self.above_parent() {
                None => true,
                Some(up) => {
                    let r = self.can_enter(
                        Some(up),
                        self.parent_cell,
                        here,
                        (self.px, self.py.wrapping_add(1)),
                        true,
                        true,
                    );
                    r.traverse_type == KIND_INACCESSIBLE || r.traverse_to_key_frame == POSE_FALLING
                }
            };
        }
        cell.set_type_index(saved.0);
        cell.set_contents(saved.1);
        cell.set_foreground_contents(saved.2);
        trapped
    }

    /// Placement validity at the goal cell (spec 3, step 8). Returns `(accept, unplaceable)`; the incoming
    /// `unplaceable` may already be set.
    fn placement(&self, cell: TileRef, pose: u32, tile_goal: bool, mut unplaceable: bool) -> (bool, bool) {
        let mut accept = true;
        let dy = self.ty.wrapping_sub(self.py);
        let ppos = self.parent_pos();
        // always-evaluated items (they only ever set the flag)
        unplaceable |= dy < -1 && tile_goal;
        unplaceable |= tile_goal && self.parent_pose() == POSE_STAND_ELEVATOR;

        if !unplaceable && tile_goal && self.door_or_trap_door_at(self.parent_cell, ppos) {
            unplaceable = true;
        }
        if !unplaceable && self.py == self.ty.wrapping_sub(2) {
            let above = self.above_parent();
            let apos = IntPair { x: self.px, y: self.py.wrapping_add(1) };
            if above.is_some() && self.door_or_trap_door_at(above, apos) {
                unplaceable = true;
            }
        }
        if !unplaceable && pose == POSE_BOB && tile_is_solid(self.cell(self.tx, self.ty.wrapping_add(1))) {
            unplaceable = true;
        }
        if !unplaceable && pose == POSE_FALLING && !is_fall_tolerant(self.interaction) {
            unplaceable = true;
        }
        if !unplaceable
            && self.interaction == A_PLACE_SOLID
            && tile_is_air_water_or_snow(Some(cell))
            && wrap_dx(self.px.wrapping_sub(self.tx), self.wm) == 0
            && self.py == self.ty
        {
            unplaceable = true;
        }
        if !unplaceable && tile_goal {
            if dw_blockhead_occupies_tile(self.user, ppos, self.user)
                || (self.py == self.ty.wrapping_add(1) && self.parent_pose() == POSE_FALLING)
            {
                unplaceable = true;
            }
            if !unplaceable && self.py != self.ty && self.tx != self.px && self.corner_blocked() {
                accept = false;
            }
        }
        if !unplaceable && removes_tile(self.interaction) && !tile_is_water(self.parent_cell) {
            unplaceable = self.parent_would_be_trapped(cell);
        }
        (accept, unplaceable)
    }

    /// Diagonal step whose two orthogonal neighbours are both solid.
    fn corner_blocked(&self) -> bool {
        if !tile_is_solid(self.cell(self.px, self.ty)) {
            return false;
        }
        let step = self.py.wrapping_sub(self.ty).clamp(-1, 1);
        tile_is_solid(self.cell(self.tx, self.ty.wrapping_add(step)))
    }
}

// ------------------------------------------------------------------------------------------------
// Entry point
// ------------------------------------------------------------------------------------------------

/// Evaluate neighbour `(test_x, test_y)` of the parent cell. Returns `false` ("abort") when a needed cell
/// is not available yet and `true` otherwise (including "already closed" and "not usable"); `true` means
/// "keep going", not "added".
#[allow(clippy::too_many_arguments)]
pub fn test_tile(
    test_x: i32,
    test_y: i32,
    goal_x: i32,
    goal_y: i32,
    goal_interaction: i32,
    open_list: &mut IndexSet,
    closed_list: &mut IndexSet,
    parent_index: i32,
    parent_tile: Option<TileRef>,
    parent: PropsSlot,
    parent_x: i32,
    parent_y: i32,
    path_user: Option<&ObjRef>,
    world: &Rc<World>,
    is_fall_path: bool,
    path_creator: &PathCreator,
    allows_water_tiles: bool,
    elevator_blacklist: &[IntPair],
) -> bool {
    let index = world_index_at_world_position(test_x, test_y, world);
    if closed_list.contains_index(ns_index(index)) {
        return true;
    }
    let Some(cell) = tile_at_world_index_loaded(index, world) else { return false };
    let Some(slot) = path_creator.tile_derived_properties_at_world_index(index) else { return false };

    let ev = Eval {
        tx: test_x,
        ty: test_y,
        gx: goal_x,
        gy: goal_y,
        interaction: goal_interaction,
        px: parent_x,
        py: parent_y,
        parent_cell: parent_tile,
        parent,
        user: path_user,
        world,
        pc: path_creator,
        blacklist: elevator_blacklist,
        wm: world.world_width_macro(),
    };

    let mut r =
        ev.can_enter(Some(cell), parent_tile, (parent_x, parent_y), (test_x, test_y), is_fall_path, allows_water_tiles);
    if r.failed_due_to_no_tile != 0 {
        return false;
    }
    r.terrain_difficulty =
        cactus_cost(r.terrain_difficulty, cell.contents(), cell.foreground_contents(), r.traverse_to_key_frame);

    let tile_goal = is_tile_goal(goal_interaction);
    let at_goal = wrap_dx(ev.gx.wrapping_sub(ev.tx), ev.wm) == 0 && ev.ty == ev.gy;
    let mut accept = base_acceptance(r.traverse_to_key_frame, ev.tx == ev.gx && ev.ty == ev.gy);
    let mut unplaceable = false;

    if at_goal {
        let v = goal_verdict(GoalCase {
            accepted: accept,
            tile_goal,
            dy: ev.ty.wrapping_sub(ev.py),
            parent_above: ev.py.wrapping_sub(ev.ty),
            dxp: wrap_dx(ev.px.wrapping_sub(ev.tx), ev.wm),
            parent_pose: ev.parent_pose(),
        });
        accept = v.accept;
        unplaceable = v.unplaceable;
        if v.lift_exit {
            ev.lift_exit(&mut unplaceable);
        }
        if accept {
            (accept, unplaceable) = ev.placement(cell, r.traverse_to_key_frame, tile_goal, unplaceable);
        }
    }
    if !accept {
        return true;
    }

    // ---- bookkeeping
    if at_goal && tile_goal && !unplaceable {
        r.terrain_difficulty = 0;
    }
    let parent_g = || {
        if parent_tile.is_none() {
            return 0;
        }
        path_creator.with_props(parent, |p| p.path.as_ref().and_then(|v| v.last()).map_or(p.g, |s| s.properties_g))
    };
    let improved = if open_list.contains_index(ns_index(index)) {
        let new_g = r.terrain_difficulty.wrapping_add(parent_g());
        let old = path_creator.with_props(slot, |p| (p.g, p.traverse_to_key_frame as u32, p.unplaceable));
        let better = revisit_improves(new_g, old, r.traverse_to_key_frame, !unplaceable);
        if better {
            path_creator.with_props_mut(slot, |p| {
                p.parent_index = parent_index;
                p.g = new_g;
            });
        }
        better
    } else {
        open_list.add_index(ns_index(index));
        path_creator.with_props_mut(slot, |p| p.parent_index = parent_index);
        let g = r.terrain_difficulty.wrapping_add(parent_g());
        path_creator.with_props_mut(slot, |p| p.g = g);
        true
    };
    if !improved {
        return true;
    }

    let h = heuristic(wrap_dx(test_x.wrapping_sub(goal_x), ev.wm), test_y.wrapping_sub(goal_y));
    path_creator.with_props_mut(slot, |p| {
        p.h = h;
        p.f = p.g.wrapping_add(p.h);
        p.traverse_type = r.traverse_type as i32;
        p.terrain_difficulty = r.terrain_difficulty;
        p.traverse_to_key_frame = r.traverse_to_key_frame as i32;
        p.unplaceable = unplaceable;
    });
    if unplaceable && path_creator.with_props(parent, |p| p.path.is_none()) {
        path_creator.with_props_mut(parent, |p| p.path = Some(Rc::new(Vec::new())));
    }
    let inherited = path_creator.with_props(parent, |p| p.path.clone()).filter(|v| v.len() < SUBPATH_LIMIT);
    let sub = inherited.map(|prefix| {
        let step = path_creator.with_props(slot, |p| SubStep {
            index,
            parent_index,
            terrain_difficulty: p.terrain_difficulty,
            traverse_type: p.traverse_type,
            traverse_to_key_frame: p.traverse_to_key_frame,
            properties_g: p.g,
        });
        let mut v = Vec::with_capacity(prefix.len() + 1);
        v.extend_from_slice(&prefix);
        v.push(step);
        Rc::new(v)
    });
    path_creator.with_props_mut(slot, |p| p.path = sub);
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::path::path_creator::testutil::*;
    use crate::path::path_creator::DerivedTileProperties;

    fn ip(x: i32, y: i32) -> IntPair {
        IntPair { x, y }
    }

    struct Fx {
        dw: Rc<crate::world::helpers::testutil::FakeDw>,
        world: Rc<World>,
        pc: Rc<PathCreator>,
        user: ObjRef,
        open: IndexSet,
        closed: IndexSet,
    }

    fn fx() -> Fx {
        let (world, _l, dw) = new_world(32, 20);
        let pc = PathCreator::init_with_world(&world);
        let user = new_user(ip(100, 21));
        Fx { dw, world, pc, user, open: IndexSet::new(), closed: IndexSet::new() }
    }

    impl Fx {
        fn idx(&self, x: i32, y: i32) -> i32 {
            y * 1024 + x
        }
        /// Parent slot at (px,py) with G = g and kf.
        fn parent(&self, px: i32, py: i32, g: i32, kf: i32) -> (PropsSlot, Option<TileRef>) {
            let s = self.pc.tile_derived_properties_at_world_index(self.idx(px, py)).unwrap();
            self.pc.with_props_mut(s, |p| {
                p.g = g;
                p.f = g;
                p.parent_index = -1;
                p.traverse_to_key_frame = kf;
                p.unplaceable = false;
                p.path = None;
            });
            (s, tile_at_world_position_loaded(px, py, &self.world))
        }
        #[allow(clippy::too_many_arguments)]
        fn run(
            &mut self,
            t: (i32, i32),
            goal: (i32, i32),
            gi: i32,
            p: (i32, i32),
            ps: PropsSlot,
            pt: Option<TileRef>,
        ) -> bool {
            let parent_index = self.idx(p.0, p.1);
            test_tile(
                t.0,
                t.1,
                goal.0,
                goal.1,
                gi,
                &mut self.open,
                &mut self.closed,
                parent_index,
                pt,
                ps,
                p.0,
                p.1,
                Some(&self.user),
                &self.world,
                false,
                &self.pc,
                true,
                &[],
            )
        }
        fn props_at(&self, x: i32, y: i32) -> DerivedTileProperties {
            let s = self.pc.tile_derived_properties_at_world_index(self.idx(x, y)).unwrap();
            self.pc.props(s)
        }
    }

    /// : level step => G = parent G + terrainDifficulty, H = 10*(|dx|+|dy|), F = G+H.
    #[test]
    fn level_step_costs() {
        let mut f = fx();
        let (ps, pt) = f.parent(100, 21, 5, 0);
        assert!(f.run((101, 21), (110, 21), 0, (100, 21), ps, pt));
        let p = f.props_at(101, 21);
        assert_eq!((p.g, p.h, p.f), (15, 90, 105));
        assert_eq!(
            (p.traverse_type, p.traverse_to_key_frame, p.terrain_difficulty, p.parent_index),
            (1, 0, 10, f.idx(100, 21))
        );
        assert!(!p.unplaceable);
        assert!(f.open.contains_index(f.idx(101, 21) as u64));
        // second visit with a worse G does not replace
        let (ps2, pt2) = f.parent(99, 21, 50, 0);
        assert!(f.run((101, 21), (110, 21), 0, (99, 21), ps2, pt2));
        assert_eq!(f.props_at(101, 21).g, 15);
        // closed tiles return true immediately and are untouched
        f.closed.add_index(f.idx(102, 21) as u64);
        assert!(f.run((102, 21), (110, 21), 0, (101, 21), ps, pt));
        assert!(!f.open.contains_index(f.idx(102, 21) as u64));
    }

    /// H uses the wrapped horizontal distance.
    #[test]
    fn heuristic_wraps() {
        let mut f = fx();
        let (ps, pt) = f.parent(2, 21, 0, 0);
        assert!(f.run((1, 21), (1022, 21), 0, (2, 21), ps, pt));
        // distance 3 around the world, not 1021
        assert_eq!(f.props_at(1, 21).h, 30);
    }

    /// unloaded tile / slot cap => false
    #[test]
    fn missing_neighbour_cells_abort() {
        let mut f = fx();
        let (ps, pt) = f.parent(4, 31, 0, 0);
        f.world.set_ops_hooks(None);
        // the rows above the loaded block are not available: the single-move rules cannot decide
        assert!(!f.run((5, 31), (20, 31), 0, (4, 31), ps, pt));
        assert!(!f.open.contains_index(f.idx(5, 31) as u64));
    }

    #[test]
    fn false_results() {
        let mut f = fx();
        let (ps, pt) = f.parent(100, 21, 0, 0);
        f.world.set_ops_hooks(None);
        assert!(!f.run((500, 21), (510, 21), 0, (100, 21), ps, pt));
    }

    /// : the goal tile of a tile interaction costs nothing; PLACE_SOLID_BLOCK on air
    /// with the parent at the goal is unplaceable; a DIG goal straight below a parent that would fall is
    /// unplaceable (trapped parent test).
    #[test]
    fn goal_tile_interactions() {
        // DIG: goal is the ground tile (106,20), parent standing at (105,21) (stays supported by (105,20))
        let mut f = fx();
        let (ps, pt) = f.parent(105, 21, 0, 0);
        assert!(f.run((106, 20), (106, 20), 1, (105, 21), ps, pt));
        let p = f.props_at(106, 20);
        assert!(f.open.contains_index(f.idx(106, 20) as u64), "{p:?}");
        assert_eq!(p.terrain_difficulty, 0);
        assert!(!p.unplaceable);
        assert_eq!(p.g, 0);

        // DIG straight down: the parent would have nothing to stand on => unplaceable
        let mut f = fx();
        let (ps, pt) = f.parent(105, 21, 0, 0);
        assert!(f.run((105, 20), (105, 20), 1, (105, 21), ps, pt));
        assert!(f.props_at(105, 20).unplaceable);

        // PLACE_SOLID_BLOCK on air with the parent at the goal
        let mut f = fx();
        let (ps, pt) = f.parent(105, 21, 0, 0);
        assert!(f.run((105, 21), (105, 21), 3, (105, 21), ps, pt));
        let p = f.props_at(105, 21);
        assert!(p.unplaceable);
        // the parent slot is the same slot: its sub-path now holds the one step
        assert_eq!(p.path.as_ref().map(|v| v.len()), Some(1));
        let st = p.path.unwrap()[0];
        assert_eq!((st.index, st.parent_index), (f.idx(105, 21), f.idx(105, 21)));
    }

    /// Reaching the goal from above with parentY - testY > 1 is unplaceable (monkey-swing-down step).
    #[test]
    fn goal_from_above_unplaceable() {
        let mut f = fx();
        edit(&f.world, 105, 23, |t| t.type_index = STONE);
        let (ps, pt) = f.parent(106, 23, 0, 0);
        assert!(f.run((105, 21), (105, 21), 2, (106, 23), ps, pt));
        let p = f.props_at(105, 21);
        assert_eq!(p.traverse_type, 20, "CLIMB_DOWN_TO_MONKEY_SWING");
        assert!(p.unplaceable);
    }

    /// The temporary DIG mutation is restored and the parent props are mutated only as specified.
    #[test]
    fn dig_restores_tile() {
        let mut f = fx();
        let (ps, pt) = f.parent(105, 21, 0, 0);
        let t = tile_at_world_position_loaded(105, 20, &f.world).unwrap();
        t.update(|t| {
            t.contents = 9;
            t.foreground_contents = 8;
        });
        let before = t.get();
        assert!(f.run((105, 20), (105, 20), 1, (105, 21), ps, pt));
        assert_eq!(t.get(), before);
    }

    /// Sub-path arrays are capped at 64 entries and propertiesG comes from the last entry.
    #[test]
    fn subpath_cap_and_g() {
        let mut f = fx();
        let (ps, pt) = f.parent(105, 21, 7, 0);
        let steps: Vec<SubStep> = (0..64)
            .map(|i| SubStep {
                index: i,
                parent_index: i - 1,
                terrain_difficulty: 1,
                traverse_type: 1,
                traverse_to_key_frame: 0,
                properties_g: 100 + i,
            })
            .collect();
        f.pc.with_props_mut(ps, |p| p.path = Some(Rc::new(steps)));
        assert!(f.run((106, 21), (120, 21), 0, (105, 21), ps, pt));
        let p = f.props_at(106, 21);
        assert_eq!(p.g, 10 + 163, "G uses the last sub-path entry's propertiesG");
        assert!(p.path.is_none(), "count >= 64 => no sub-path appended");
        let steps: Vec<SubStep> = (0..63)
            .map(|i| SubStep {
                index: i,
                parent_index: i - 1,
                terrain_difficulty: 1,
                traverse_type: 1,
                traverse_to_key_frame: 0,
                properties_g: 100 + i,
            })
            .collect();
        f.pc.with_props_mut(ps, |p| p.path = Some(Rc::new(steps)));
        assert!(f.run((104, 21), (120, 21), 0, (105, 21), ps, pt));
        let p = f.props_at(104, 21);
        assert_eq!(p.path.as_ref().unwrap().len(), 64);
        assert_eq!(p.path.as_ref().unwrap()[63].properties_g, p.g);
    }

    // ------------------------------------------------------------------------------------------
    // decision-table tests (spec section 3)
    // ------------------------------------------------------------------------------------------

    const STAND: u32 = 0;

    #[test]
    fn tile_goal_classification() {
        for a in
            [A_NONE, A_STAB, A_RIDE, A_CAPTURE, A_HARVEST_NPC, A_SET_NPC_FREE, A_FEED_NPC, A_INSPECT_NPC, A_FIRE_RANGED]
        {
            assert!(!is_tile_goal(a), "{a}");
        }
        for a in [A_DIG, A_DIG_FOREGROUND, A_PLACE_ITEM, A_PLACE_SOLID, A_PICKUP, A_PICKUP_FREEBLOCK] {
            assert!(is_tile_goal(a), "{a}");
        }
        assert!(is_fall_tolerant(A_PLACE_ITEM) && is_fall_tolerant(A_PICKUP_FREEBLOCK));
        assert!(!is_fall_tolerant(A_DIG) && !is_fall_tolerant(A_NONE));
        assert!(removes_tile(A_DIG) && removes_tile(A_DIG_FOREGROUND) && removes_tile(A_PICKUP));
        assert!(!removes_tile(A_PICKUP_FREEBLOCK) && !removes_tile(A_NONE));
    }

    #[test]
    fn base_acceptance_table() {
        assert!(base_acceptance(STAND, false));
        assert!(base_acceptance(STAND, true));
        assert!(!base_acceptance(POSE_INACCESSIBLE, false));
        assert!(base_acceptance(POSE_FALLING, false));
        assert!(!base_acceptance(POSE_FALLING, true));
        assert!(proximity_ok(0, STAND) && proximity_ok(-1, POSE_CRAWL) && proximity_ok(1, STAND));
        assert!(!proximity_ok(2, STAND) && !proximity_ok(-2, STAND) && !proximity_ok(0, POSE_FALLING));
    }

    fn case(accepted: bool, tile_goal: bool, dy: i32, dxp: i32, parent_pose: u32) -> GoalCase {
        GoalCase { accepted, tile_goal, dy, parent_above: -dy, dxp, parent_pose }
    }

    fn verdict(c: GoalCase) -> (bool, bool, bool) {
        let v = goal_verdict(c);
        (v.accept, v.unplaceable, v.lift_exit)
    }

    #[test]
    fn goal_verdict_rejected_move() {
        // rescue by tile goal
        assert_eq!(verdict(case(false, true, 0, 0, STAND)), (true, false, false));
        assert_eq!(verdict(case(false, true, -1, 1, STAND)), (true, false, false));
        assert_eq!(verdict(case(false, true, 3, 1, STAND)), (true, false, false));
        // crawling parent far above cannot reach
        assert!(!verdict(case(false, true, 3, 0, POSE_CRAWL)).0);
        assert!(!verdict(case(false, true, 2, 1, POSE_CRAWL)).0);
        assert!(verdict(case(false, true, 1, 1, POSE_CRAWL)).0);
        // too far below
        assert!(!verdict(case(false, true, -2, 0, STAND)).0);
        // rescue by directly adjacent column
        assert_eq!(verdict(case(false, false, 1, 0, STAND)), (true, false, false));
        assert!(verdict(case(false, false, -1, 0, STAND)).0);
        assert!(!verdict(case(false, false, 2, 0, STAND)).0);
        assert!(!verdict(case(false, false, 0, 1, STAND)).0);
        // proximity gate after rescue
        assert!(!verdict(case(false, true, 0, 2, STAND)).0);
        assert!(!verdict(case(false, true, 0, 0, POSE_FALLING)).0);
        // lift exit requested for tile goals with a lift-standing parent (even when the gate fails)
        assert_eq!(verdict(case(false, true, 0, 0, POSE_STAND_ELEVATOR)), (true, false, true));
        assert_eq!(verdict(case(false, true, 0, 3, POSE_STAND_ELEVATOR)), (false, false, true));
        assert_eq!(verdict(case(false, false, 0, 0, POSE_STAND_ELEVATOR)), (true, false, false));
    }

    #[test]
    fn goal_verdict_accepted_move() {
        assert_eq!(verdict(case(true, true, 0, 0, STAND)), (true, false, false));
        assert_eq!(verdict(case(true, false, 0, 0, STAND)), (true, false, false));
        // tile goals reject a far parent column outright; creature goals only fail the gate
        assert_eq!(verdict(case(true, true, 0, 2, STAND)), (false, false, false));
        assert_eq!(verdict(case(true, true, 0, -2, STAND)), (false, false, false));
        assert_eq!(verdict(case(true, false, 0, 2, STAND)), (false, false, false));
        // parent more than one row above => unplaceable but accepted
        assert_eq!(verdict(case(true, true, -2, 0, STAND)), (true, true, false));
        assert_eq!(verdict(case(true, true, -1, 0, STAND)), (true, false, false));
        assert_eq!(verdict(case(true, false, -3, 0, STAND)), (true, false, false));
        // falling parent fails the gate
        assert_eq!(verdict(case(true, true, 0, 0, POSE_FALLING)), (false, false, false));
        // lift exit
        assert_eq!(verdict(case(true, true, 0, 1, POSE_STAND_ELEVATOR)), (true, false, true));
        assert_eq!(verdict(case(true, false, 0, 1, POSE_STAND_ELEVATOR)), (true, false, false));
    }

    #[test]
    fn cactus_and_record_rules() {
        assert_eq!(cactus_cost(7, 43, 0, POSE_HANG_BACK), 70);
        assert_eq!(cactus_cost(7, 43, 65, POSE_HANG_BACK), 70);
        assert_eq!(cactus_cost(7, 43, 66, POSE_HANG_BACK), 7);
        assert_eq!(cactus_cost(7, 42, 0, POSE_HANG_BACK), 7);
        assert_eq!(cactus_cost(7, 43, 0, STAND), 7);
        assert_eq!(cactus_cost(i32::MAX, 43, 0, POSE_HANG_BACK), i32::MAX.wrapping_mul(10));

        // (new_g, (old_g, old_pose, old_unplaceable), new_pose, placeable)
        assert!(revisit_improves(5, (6, STAND, false), STAND, true));
        assert!(!revisit_improves(6, (6, STAND, false), STAND, true));
        assert!(!revisit_improves(5, (6, STAND, false), POSE_INACCESSIBLE, true));
        assert!(revisit_improves(5, (6, POSE_INACCESSIBLE, false), POSE_INACCESSIBLE, true));
        assert!(revisit_improves(50, (6, POSE_INACCESSIBLE, false), STAND, true));
        assert!(!revisit_improves(50, (6, POSE_INACCESSIBLE, false), POSE_INACCESSIBLE, true));
        assert!(revisit_improves(50, (6, STAND, true), STAND, true));
        assert!(!revisit_improves(50, (6, STAND, true), STAND, false));
        assert!(!revisit_improves(50, (6, STAND, false), STAND, true));
        assert_eq!(heuristic(-3, 4), 70);
        assert_eq!(heuristic(0, 0), 0);
    }

    // ---- cell dependent rules

    struct Occupied(std::cell::Cell<bool>);
    impl crate::path::path_creator::PathDynamicWorldHooks for Occupied {
        fn has_lights_to_add(&self) -> bool {
            false
        }
        fn blockhead_occupies_tile_at_pos_ignore_blockhead(&self, _p: IntPair, _i: Option<&ObjRef>) -> bool {
            self.0.get()
        }
        fn workbench_at_pos(&self, _p: IntPair) -> Option<Rc<dyn crate::path::wire_path::WirePowerSource>> {
            None
        }
    }

    impl Fx {
        fn eval(&self, t: (i32, i32), p: (i32, i32), gi: i32, ps: PropsSlot, pt: Option<TileRef>) -> Eval<'_> {
            Eval {
                tx: t.0,
                ty: t.1,
                gx: t.0,
                gy: t.1,
                interaction: gi,
                px: p.0,
                py: p.1,
                parent_cell: pt,
                parent: ps,
                user: Some(&self.user),
                world: &self.world,
                pc: &self.pc,
                blacklist: &[],
                wm: self.world.world_width_macro(),
            }
        }
        fn tile(&self, x: i32, y: i32) -> TileRef {
            tile_at_world_position_loaded(x, y, &self.world).unwrap()
        }
        /// Placement verdict for T=(tx,ty) from P=(px,py) with the given pose / interaction.
        fn place(&self, t: (i32, i32), p: (i32, i32), pose: u32, gi: i32, parent_pose: i32) -> (bool, bool) {
            let (ps, pt) = self.parent(p.0, p.1, 0, parent_pose);
            let ev = self.eval(t, p, gi, ps, pt);
            ev.placement(self.tile(t.0, t.1), pose, is_tile_goal(gi), false)
        }
    }

    #[test]
    fn lift_exit_cases() {
        // parent cell solid
        let f = fx();
        let (ps, pt) = f.parent(105, 20, 0, 0);
        let ev = f.eval((106, 20), (105, 20), A_DIG, ps, pt);
        let mut u = false;
        ev.lift_exit(&mut u);
        assert!(u);

        // headroom solid
        let f = fx();
        edit(&f.world, 105, 22, |t| t.type_index = STONE);
        let (ps, pt) = f.parent(105, 21, 0, 0);
        let ev = f.eval((106, 21), (105, 21), A_DIG, ps, pt);
        let mut u = false;
        ev.lift_exit(&mut u);
        assert!(u);
        assert_eq!(f.pc.props(ps).traverse_to_key_frame, 0);

        // headroom free: parent pose becomes HANG_BACK, flag untouched
        let f = fx();
        let (ps, pt) = f.parent(105, 21, 0, POSE_STAND_ELEVATOR as i32);
        let ev = f.eval((106, 21), (105, 21), A_DIG, ps, pt);
        let mut u = false;
        ev.lift_exit(&mut u);
        assert!(!u);
        assert_eq!(f.pc.props(ps).traverse_to_key_frame, POSE_HANG_BACK as i32);

        // headroom cell not loaded
        let f = fx();
        let (ps, pt) = f.parent(500, 21, 0, 0);
        assert!(pt.is_some());
        f.world.set_ops_hooks(None);
        let ev = f.eval((501, 21), (500, 31), A_DIG, ps, pt);
        let mut u = false;
        let ev = Eval { py: 31, ..ev };
        ev.lift_exit(&mut u);
        assert!(u);
    }

    #[test]
    fn placement_vertical_and_pose_rules() {
        let f = fx();
        // parent more than one row above, tile goal => unplaceable; creature goal => not by this rule
        assert_eq!(f.place((105, 21), (105, 24), STAND, A_DIG, 0), (true, true));
        assert_eq!(f.place((105, 21), (105, 24), STAND, A_NONE, 0), (true, false));
        // parent standing in a lift
        assert_eq!(f.place((105, 21), (106, 21), STAND, A_PLACE_ITEM, POSE_STAND_ELEVATOR as i32), (true, true));
        assert_eq!(f.place((105, 21), (106, 21), STAND, A_NONE, POSE_STAND_ELEVATOR as i32), (true, false));
        // falling into the goal only suits placement/pickup interactions
        assert_eq!(f.place((105, 21), (106, 21), POSE_FALLING, A_NONE, 0), (true, true));
        assert_eq!(f.place((105, 21), (106, 21), POSE_FALLING, A_PLACE_ITEM, 0), (true, false));
        // parent just above a falling parent
        assert_eq!(f.place((105, 21), (106, 22), STAND, A_PLACE_ITEM, POSE_FALLING as i32), (true, true));
        assert_eq!(f.place((105, 21), (106, 22), STAND, A_NONE, POSE_FALLING as i32), (true, false));
    }

    #[test]
    fn placement_bob_needs_headroom() {
        let f = fx();
        assert_eq!(f.place((105, 21), (106, 21), POSE_BOB, A_NONE, 0), (true, false));
        edit(&f.world, 105, 22, |t| t.type_index = STONE);
        assert_eq!(f.place((105, 21), (106, 21), POSE_BOB, A_NONE, 0), (true, true));
        assert_eq!(f.place((105, 21), (106, 21), STAND, A_NONE, 0), (true, false));
    }

    #[test]
    fn placement_place_solid_block_on_own_cell() {
        let f = fx();
        assert_eq!(f.place((105, 21), (105, 21), STAND, A_PLACE_SOLID, 0), (true, true));
        assert_eq!(f.place((105, 21), (106, 21), STAND, A_PLACE_SOLID, 0), (true, false));
        assert_eq!(f.place((105, 21), (105, 22), STAND, A_PLACE_SOLID, 0), (true, false));
        // a solid goal cell is not "air, water or snow"
        let f = fx();
        assert_eq!(f.place((105, 20), (105, 20), STAND, A_PLACE_SOLID, 0), (true, false));
    }

    #[test]
    fn placement_doors_at_parent_and_above() {
        let f = fx();
        // door in the parent cell
        let (ps, pt) = f.parent(105, 21, 0, 0);
        edit(&f.world, 105, 21, |t| t.background_contents = 0x46);
        let _ = (ps, pt);
        assert!(!f.place((106, 21), (105, 21), STAND, A_PLACE_ITEM, 0).1, "door not usable");
        door_usable(&f, true);
        assert_eq!(f.place((106, 21), (105, 21), STAND, A_PLACE_ITEM, 0), (true, true));
        assert_eq!(f.place((106, 21), (105, 21), STAND, A_NONE, 0), (true, false), "creature goals ignore it");
        // trap door in the parent cell
        edit(&f.world, 105, 21, |t| t.background_contents = 0x4B);
        assert_eq!(f.place((106, 21), (105, 21), STAND, A_PLACE_ITEM, 0), (true, true));

        // door above the parent, goal two rows above the parent
        let f = fx();
        door_usable(&f, true);
        edit(&f.world, 105, 22, |t| t.background_contents = 0x46);
        assert_eq!(f.place((106, 23), (105, 21), STAND, A_NONE, 0), (true, true));
        edit(&f.world, 105, 22, |t| t.background_contents = 0x4B);
        assert_eq!(f.place((106, 23), (105, 21), STAND, A_NONE, 0), (true, true));
        edit(&f.world, 105, 22, |t| t.background_contents = 0);
        assert_eq!(f.place((106, 23), (105, 21), STAND, A_NONE, 0), (true, false));
        // only when exactly two rows apart
        edit(&f.world, 105, 22, |t| t.background_contents = 0x46);
        assert_eq!(f.place((106, 22), (105, 21), STAND, A_NONE, 0), (true, false));
    }

    fn door_usable(f: &Fx, v: bool) {
        f.dw.door.set(v);
    }

    #[test]
    fn placement_blockhead_occupied_parent() {
        let f = fx();
        {
            use crate::path::path_creator::PathDynamicWorldHooks;
            let h = Occupied(std::cell::Cell::new(false));
            assert!(!h.has_lights_to_add());
            assert!(h.workbench_at_pos(IntPair { x: 0, y: 0 }).is_none());
        }
        let hooks = Rc::new(Occupied(std::cell::Cell::new(true)));
        crate::path::path_creator::set_dynamic_world_hooks(Some(hooks.clone()));
        assert_eq!(f.place((105, 21), (106, 21), STAND, A_PLACE_ITEM, 0), (true, true));
        assert_eq!(f.place((105, 21), (106, 21), STAND, A_NONE, 0), (true, false));
        hooks.0.set(false);
        assert_eq!(f.place((105, 21), (106, 21), STAND, A_PLACE_ITEM, 0), (true, false));
        crate::path::path_creator::set_dynamic_world_hooks(None);
    }

    #[test]
    fn placement_diagonal_corner_cutting() {
        // T=(106,22) from P=(105,21): needs (105,22) and (106,21) both solid to block
        let f = fx();
        assert!(f.place((106, 22), (105, 21), STAND, A_PLACE_ITEM, 0).0);
        edit(&f.world, 105, 22, |t| t.type_index = STONE);
        assert!(f.place((106, 22), (105, 21), STAND, A_PLACE_ITEM, 0).0);
        edit(&f.world, 106, 21, |t| t.type_index = STONE);
        assert!(!f.place((106, 22), (105, 21), STAND, A_PLACE_ITEM, 0).0);
        assert!(f.place((106, 22), (105, 21), STAND, A_NONE, 0).0, "creature goals are exempt");
        // the same column or the same row is never a corner
        assert!(f.place((105, 22), (105, 21), STAND, A_PLACE_ITEM, 0).0);
        assert!(f.place((106, 21), (105, 21), STAND, A_PLACE_ITEM, 0).0);
        // downward diagonal consults the row below the goal... clamp(-1)
        let f = fx();
        edit(&f.world, 105, 22, |t| t.type_index = STONE);
        edit(&f.world, 106, 23, |t| t.type_index = STONE);
        assert!(!f.place((106, 22), (105, 23), STAND, A_PLACE_ITEM, 0).0);
    }

    #[test]
    fn placement_dig_trapped_parent() {
        // standing on solid ground, digging a side wall: parent keeps its footing
        let f = fx();
        edit(&f.world, 106, 21, |t| t.type_index = STONE);
        assert_eq!(f.place((106, 21), (105, 21), STAND, A_DIG, 0), (true, false));
        let f = fx();
        // digging the floor under the parent (nothing else to stand on) traps it
        assert_eq!(f.place((105, 20), (105, 21), STAND, A_DIG_FOREGROUND, 0), (true, true));
        assert_eq!(f.place((105, 20), (105, 21), STAND, A_PICKUP, 0), (true, true));
        // other interactions never run the test
        assert_eq!(f.place((105, 20), (105, 21), STAND, A_PICKUP_FREEBLOCK, 0), (true, false));
        // water parents are exempt
        let f = fx();
        edit(&f.world, 105, 21, |t| t.type_index = 3);
        assert_eq!(f.place((105, 20), (105, 21), STAND, A_DIG, 0), (true, false));
        // restoration of the probed cell
        let f = fx();
        let t = f.tile(105, 20);
        t.update(|t| t.contents = 9);
        let before = t.get();
        let _ = f.place((105, 20), (105, 21), STAND, A_DIG, 0);
        assert_eq!(t.get(), before);
    }

    #[test]
    fn placement_trapped_parent_without_headroom_cell() {
        // parent at the world's top edge in a column whose cell above is not loaded
        let f = fx();
        let (ps, pt) = f.parent(105, 21, 0, 0);
        let cell = f.tile(105, 20);
        let ev = f.eval((105, 20), (105, 21), A_DIG, ps, pt);
        assert!(ev.parent_would_be_trapped(cell));
        let _ = ev;
        // unload the cell above the parent
        let f = fx();
        let (ps, pt) = f.parent(500, 21, 0, 0);
        let cell = f.tile(500, 20);
        f.world.set_ops_hooks(None);
        let ev = f.eval((500, 20), (500, 31), A_DIG, ps, pt);
        let ev = Eval { py: 31, ..ev };
        assert!(ev.parent_would_be_trapped(cell));
    }

    // ---- whole-evaluation scenarios

    #[test]
    fn unreachable_goal_cell_is_not_added() {
        let mut f = fx();
        // goal inside rock, creature goal, parent diagonal: nothing can act on it
        let (ps, pt) = f.parent(104, 21, 0, 0);
        assert!(f.run((105, 20), (105, 20), 0, (104, 21), ps, pt));
        assert!(!f.open.contains_index(f.idx(105, 20) as u64));
    }

    #[test]
    fn falling_arrival_at_goal_is_rescued_only_by_adjacency() {
        // two above the goal: the drop is FALLING and lands exactly on the goal => not a valid arrival,
        // and the parent is neither adjacent nor within one row, so the goal stays unreached
        let mut f = fx();
        let (ps, pt) = f.parent(105, 24, 0, 0);
        assert!(f.run((105, 22), (105, 22), A_NONE, (105, 24), ps, pt));
        assert!(!f.open.contains_index(f.idx(105, 22) as u64));
        // one above: adjacent column within one row => rescued
        let (ps, pt) = f.parent(105, 23, 0, 0);
        assert!(f.run((105, 22), (105, 22), A_NONE, (105, 23), ps, pt));
        assert!(f.open.contains_index(f.idx(105, 22) as u64));
    }

    #[test]
    fn record_table_exhaustion_aborts() {
        let mut f = fx();
        let (ps, pt) = f.parent(100, 21, 0, 0);
        for i in 0..40000 {
            let _ = f.pc.tile_derived_properties_at_world_index(i);
        }
        assert!(!f.run((101, 21), (110, 21), 0, (100, 21), ps, pt));
    }

    #[test]
    fn missing_parent_cell_contributes_zero_g() {
        let mut f = fx();
        let (ps, _) = f.parent(100, 21, 99, 0);
        assert!(f.run((101, 21), (110, 21), 0, (100, 21), ps, None));
        let p = f.props_at(101, 21);
        assert_eq!(p.g, p.terrain_difficulty);
    }

    #[test]
    fn revisit_replaces_when_cheaper_and_inherits_subpath() {
        let mut f = fx();
        let (ps, pt) = f.parent(99, 21, 50, 0);
        assert!(f.run((101, 21), (110, 21), 0, (99, 21), ps, pt));
        let g_first = f.props_at(101, 21).g;
        let (ps2, pt2) = f.parent(100, 21, 0, 0);
        assert!(f.run((101, 21), (110, 21), 0, (100, 21), ps2, pt2));
        let p = f.props_at(101, 21);
        assert!(p.g < g_first);
        assert_eq!(p.parent_index, f.idx(100, 21));
        // parents without a sub-path give the child none
        assert!(p.path.is_none());
    }

    #[test]
    fn lift_standing_parent_flags_goal_unplaceable() {
        let mut f = fx();
        let (ps, pt) = f.parent(105, 21, 0, POSE_STAND_ELEVATOR as i32);
        assert!(f.run((106, 21), (106, 21), A_PLACE_ITEM, (105, 21), ps, pt));
        // headroom is free: the parent is re-posed and the live pose no longer marks the goal unplaceable
        assert!(!f.props_at(106, 21).unplaceable);
        assert_eq!(f.pc.props(ps).traverse_to_key_frame, POSE_HANG_BACK as i32, "lift exit adjusted the parent");

        // no headroom: unplaceable, parent keeps the lift pose
        let mut f = fx();
        edit(&f.world, 105, 22, |t| t.type_index = STONE);
        let (ps, pt) = f.parent(105, 21, 0, POSE_STAND_ELEVATOR as i32);
        assert!(f.run((106, 21), (106, 21), A_PLACE_ITEM, (105, 21), ps, pt));
        assert!(f.props_at(106, 21).unplaceable);
        assert_eq!(f.pc.props(ps).traverse_to_key_frame, POSE_STAND_ELEVATOR as i32);
    }
}
