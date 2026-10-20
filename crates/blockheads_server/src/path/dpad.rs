//! `dpadFindPath`: the D-pad fallback that tries up to three alternative target tiles.
//!

use std::rc::Rc;

use crate::objects::handle::ObjRef;
use crate::types::enums::TileTraverseType;
use crate::types::structs::{DPadPathResult, IntPair};
use crate::world::state::World;
use crate::world::tiles::tile_at_world_position_loaded;

use super::can_enter::{check_can_enter_tile, FromProps};
use super::path_creator::{user_pos, wrap_dx};

const TT_FALL: u32 = TileTraverseType::TRAVERSE_FALL.0 as u32;
const TT_INACCESSIBLE: u32 = TileTraverseType::TRAVERSE_INACCESSIBLE.0 as u32;

/// `dpadFindPath(fromDerived, fromPos, desiredPos, pathUser, world)`. Every check is
/// `checkCanEnterTile(T(target), T(fromPos), from, fromPos, target, pathUser, world, isFallPath=NO,
/// allowsWaterTiles=YES, emptyBlacklist)`. `pos` is `[pathUser pos]` (NOT `fromPos`); with a nil path user
/// the original reads an uninitialised slot (all callers pass non-nil).
pub fn dpad_find_path(
    from: FromProps,
    from_pos: IntPair,
    desired_pos: IntPair,
    path_user: Option<&ObjRef>,
    world: &Rc<World>,
) -> DPadPathResult {
    debug_assert!(path_user.is_some(), "dpadFindPath: nil pathUser reads an uninitialised stack slot in the original");
    let wm = world.world_width_macro();
    let wrap = |d: i32| wrap_dx(d, wm);
    let pos = user_pos(path_user);
    let from_tile = tile_at_world_position_loaded(from_pos.x, from_pos.y, world);
    let no_blacklist: [IntPair; 0] = [];
    let check = |target: IntPair| {
        let t = tile_at_world_position_loaded(target.x, target.y, world);
        check_can_enter_tile(
            t,
            from_tile,
            from,
            from_pos.x,
            from_pos.y,
            target.x,
            target.y,
            path_user,
            world,
            false,
            true,
            &no_blacklist,
        )
    };

    let mut out = DPadPathResult { path_test_result: check(desired_pos), movement_pos: IntPair { x: 0, y: 0 } };
    let mut try_pos = desired_pos;
    if out.path_test_result.traverse_type == TT_INACCESSIBLE {
        // attempt 1 (no FALL demotion)
        let mut give_up = false;
        if desired_pos.x == pos.x {
            if pos.y < desired_pos.y {
                try_pos = IntPair { x: desired_pos.x, y: desired_pos.y.wrapping_add(1) };
            } else {
                give_up = true;
            }
        } else {
            let mut y_try = desired_pos.y;
            let mut x_try = desired_pos.x;
            if desired_pos.y == pos.y {
                let d = wrap(desired_pos.x.wrapping_sub(pos.x));
                x_try = if d < 1 { desired_pos.x.wrapping_sub(1) } else { desired_pos.x.wrapping_add(1) };
            } else {
                y_try = pos.y;
            }
            try_pos = IntPair { x: x_try, y: y_try };
        }
        if !give_up {
            out.path_test_result = check(try_pos);
            if out.path_test_result.traverse_type == TT_FALL {
                out.path_test_result.traverse_type = TT_INACCESSIBLE;
            }
            if out.path_test_result.traverse_type == TT_INACCESSIBLE {
                // attempt 2
                let d = wrap(desired_pos.x.wrapping_sub(pos.x));
                if d == 0 {
                    give_up = true;
                } else if desired_pos.y == pos.y {
                    try_pos = IntPair { x: desired_pos.x, y: pos.y.wrapping_add(1) };
                } else if pos.y < desired_pos.y {
                    try_pos = IntPair { x: desired_pos.x, y: desired_pos.y.wrapping_add(1) };
                } else {
                    try_pos = IntPair { x: desired_pos.x, y: desired_pos.y.wrapping_sub(1) };
                }
                if !give_up {
                    out.path_test_result = check(try_pos);
                    if out.path_test_result.traverse_type == TT_FALL {
                        out.path_test_result.traverse_type = TT_INACCESSIBLE;
                    }
                    if out.path_test_result.traverse_type == TT_INACCESSIBLE {
                        // attempt 3
                        let d = wrap(desired_pos.x.wrapping_sub(pos.x));
                        if d == 0 {
                            give_up = true;
                        } else if desired_pos.y == pos.y {
                            try_pos = IntPair { x: desired_pos.x, y: pos.y.wrapping_sub(1) };
                        } else {
                            try_pos = IntPair { x: pos.x, y: desired_pos.y };
                        }
                        if !give_up {
                            out.path_test_result = check(try_pos);
                            if out.path_test_result.traverse_type == TT_FALL {
                                out.path_test_result.traverse_type = TT_INACCESSIBLE;
                            }
                        }
                    }
                }
            }
        }
    }
    out.movement_pos = try_pos;
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::path::path_creator::testutil::*;

    fn ip(x: i32, y: i32) -> IntPair {
        IntPair { x, y }
    }

    #[test]
    fn fallback_targets() {
        let (world, _l, _d) = new_world(32, 20);
        let user = new_user(ip(100, 21));
        let from = FromProps { terrain_difficulty: 0, traverse_to_key_frame: 0 };
        // directly enterable
        let r = dpad_find_path(from, ip(100, 21), ip(101, 21), Some(&user), &world);
        assert_eq!(r.path_test_result.traverse_type, 1);
        assert_eq!(r.movement_pos, ip(101, 21));
        // desired tile inside the ground: attempt 1 retargets to the user's row
        let r = dpad_find_path(from, ip(100, 21), ip(101, 15), Some(&user), &world);
        assert_eq!(r.movement_pos, ip(101, 21));
        assert_eq!(r.path_test_result.traverse_type, 1);
        // same column, desired below: give up after attempt 0; movementPos stays the desired position
        let r = dpad_find_path(from, ip(100, 21), ip(100, 15), Some(&user), &world);
        assert_eq!(r.path_test_result.traverse_type, 25);
        assert_eq!(r.movement_pos, ip(100, 15));
        // same column, desired above: tries (x, y+1)
        let r = dpad_find_path(from, ip(100, 21), ip(100, 40), Some(&user), &world);
        assert_eq!(r.movement_pos, ip(100, 41));
    }
}
