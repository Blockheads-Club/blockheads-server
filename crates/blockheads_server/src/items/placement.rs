//! Torch and sign connection geometry.
//!

use crate::types::enums::{SignConnectionType, SignOffsetType};
use crate::types::item_type::ItemType;
use crate::types::structs::IntPair;

/// The five `Tile` fields the geometry reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PTile {
    pub type_index: u8,
    pub back_wall_type_index: u8,
    pub contents: u8,
    pub foreground_contents: u8,
    pub background_contents: u8,
}

/// `InteractionObject` reduced to the four getters the sign code reads.
#[derive(Clone, Copy, Debug)]
pub struct SignObjectInfo {
    pub interaction_object_type: u16,
    pub connection_type: SignConnectionType,
    pub offset_type: SignOffsetType,
    pub pos: IntPair,
}

/// World access needed by the placement geometry; implemented by the world layer.
pub trait PlacementWorld {
    /// `tileAtWorldPositionLoaded(x, y, world)`: `None` when the macro block is not loaded.
    fn tile_at_loaded(&self, x: i32, y: i32) -> Option<PTile>;
    /// `tileIsHalfDepth(tile, world, makeIntpair(x, y))` for the tile at that position (false when no tile).
    fn tile_is_half_depth_at(&self, x: i32, y: i32) -> bool;
    /// `[[world dynamicWorld] interactionObjectTypeForObjectAtPos:(x, y)]` (0 when no object).
    fn interaction_object_type_for_object_at_pos(&self, x: i32, y: i32) -> u16;
    /// `[[world dynamicWorld] interactionObjectAtPos:(x, y)]`; `None` for nil.
    fn interaction_object_at_pos(&self, x: i32, y: i32) -> Option<SignObjectInfo>;
}

/// `tileIsSolid`: non-NULL and `typeIndex` not in {2 AIR, 3 WATER, 5 SNOW}.
pub fn tile_is_solid(t: Option<PTile>) -> bool {
    matches!(t, Some(t) if !matches!(t.type_index, 2 | 3 | 5))
}

/// `tileIsAirWaterOrSnow`: non-NULL and `typeIndex` in {2, 3, 5}.
pub fn tile_is_air_water_or_snow(t: Option<PTile>) -> bool {
    matches!(t, Some(t) if matches!(t.type_index, 2 | 3 | 5))
}

/// `tileContainsDoor`: non-NULL and `backgroundContents == 70`.
pub fn tile_contains_door(t: Option<PTile>) -> bool {
    matches!(t, Some(t) if t.background_contents == 70)
}

/// `open(t)` = air/water/snow and foreground empty or an interaction object (48).
pub fn open(t: Option<PTile>) -> bool {
    tile_is_air_water_or_snow(t) && matches!(t, Some(t) if t.foreground_contents == 0 || t.foreground_contents == 48)
}

/// Foreground is a column (100).
fn is_column(t: Option<PTile>) -> bool {
    matches!(t, Some(t) if t.foreground_contents == 100)
}

/// `torchConnectionTypeForPos`: 5 hanging, 0 ground, 3 column, 2 half depth, 1 wall right, -1 wall left,
/// -2 free floating, -99 none. Reads only.
pub fn torch_connection_type_for_pos(pos: IntPair, world: &dyn PlacementWorld, item: ItemType) -> i32 {
    let (x, y) = (pos.x, pos.y);
    let at = |dx: i32, dy: i32| world.tile_at_loaded(x.wrapping_add(dx), y.wrapping_add(dy));
    let chandelier = matches!(item.0, 148 | 147 | 146 | 145 | 149);
    let gem = matches!(item.0, 75 | 76 | 86 | 87 | 88);
    let mut conn: i32 = -99;
    if chandelier || item.0 == 157 {
        conn = 5;
        if item.0 == 157 {
            let b = at(0, -1);
            if tile_is_solid(b) {
                conn = 0;
            } else if is_column(b) {
                conn = 3;
            }
        }
    } else if gem {
        let t = at(0, 0);
        if t.is_none() || t.unwrap().back_wall_type_index == 2 || t.unwrap().contents != 0 {
            let half = t.is_some() && world.tile_is_half_depth_at(x, y);
            if half {
                conn = 2;
            } else {
                let b = at(0, -1);
                if tile_is_solid(b) {
                    conn = 0;
                } else if is_column(b) {
                    conn = 3;
                } else {
                    let r = at(1, 0);
                    if tile_is_solid(r) && !tile_contains_door(r) {
                        conn = 1;
                    } else {
                        let l = at(-1, 0);
                        if tile_is_solid(l) && !tile_contains_door(l) {
                            conn = -1;
                        }
                    }
                }
            }
        } else {
            conn = -2;
        }
    } else {
        let b = at(0, -1);
        if tile_is_solid(b) {
            conn = 0;
        } else if is_column(b) {
            conn = 3;
        } else {
            let t = at(0, 0);
            let half = t.is_some() && world.tile_is_half_depth_at(x, y);
            if half {
                conn = 2;
            } else {
                let r = at(1, 0);
                if tile_is_solid(r) && !tile_contains_door(r) {
                    conn = 1;
                } else {
                    let l = at(-1, 0);
                    if tile_is_solid(l) && !tile_contains_door(l) {
                        conn = -1;
                    } else if matches!(t, Some(t) if t.back_wall_type_index != 2 && t.contents == 0) {
                        conn = -2;
                    }
                }
            }
        }
    }
    conn
}

/// `signConnectionTypeAndOffsetForPos`. `offset_type` is an optional in/out; `preferred == 0` means any.
/// Candidates are tried strictly in order (1)..(7); the first whose type/offset constraints match returns, otherwise
/// the first candidate seen is the fallback written/returned at END.
pub fn sign_connection_type_and_offset_for_pos(
    pos: IntPair,
    mut offset_type: Option<&mut SignOffsetType>,
    world: &dyn PlacementWorld,
    preferred: SignConnectionType,
) -> SignConnectionType {
    let (x, y) = (pos.x, pos.y);
    let at = |dx: i32, dy: i32| world.tile_at_loaded(x.wrapping_add(dx), y.wrapping_add(dy));
    let pref = preferred.0;
    let mut fallback_type: i64 = 0;
    let mut fallback_offset: i64 = 0;

    macro_rules! cur_off {
        () => {
            offset_type.as_deref().map(|o| o.0)
        };
    }
    macro_rules! set_off {
        ($v:expr) => {
            if let Some(o) = offset_type.as_deref_mut() {
                o.0 = $v;
            }
        };
    }
    // accept(T, off, prefOK): immediate return when prefOK and the offset filter matches, else remember the first.
    macro_rules! accept {
        ($t:expr, $off:expr, $pref_ok:expr) => {{
            let t_: i64 = $t;
            let off_: i64 = $off;
            let pref_ok_: bool = $pref_ok;
            if pref_ok_ && (offset_type.is_none() || cur_off!() == Some(off_)) {
                set_off!(off_);
                return SignConnectionType(t_);
            }
            if fallback_type == 0 {
                fallback_type = t_;
                fallback_offset = off_;
            }
        }};
    }
    macro_rules! accept_default {
        ($t:expr, $off:expr) => {
            accept!($t, $off, pref == ($t) || pref == 0)
        };
    }

    'end: {
        let tile = match at(0, 0) {
            Some(t) => t,
            None => break 'end,
        };
        let (left_o, right_o) = (at(-1, 0), at(1, 0));
        let (left, right) = match (left_o, right_o) {
            (Some(l), Some(r)) => (l, r),
            _ => {
                set_off!(0);
                return SignConnectionType(0);
            }
        };

        // do not place next to an existing sign (unless it would be shifted away from it)
        if left.foreground_contents == 48 && cur_off!() != Some(2) {
            let ot = world.interaction_object_type_for_object_at_pos(x.wrapping_sub(1), y);
            if ot == 4 || ot == 8 {
                set_off!(0);
                return SignConnectionType(0);
            }
        }
        if right.foreground_contents == 48 && cur_off!() != Some(1) {
            let ot = world.interaction_object_type_for_object_at_pos(x.wrapping_add(1), y);
            if ot == 4 || ot == 8 {
                set_off!(0);
                return SignConnectionType(0);
            }
        }

        let tile_o = Some(tile);
        if !tile_is_air_water_or_snow(tile_o) {
            if tile_is_solid(tile_o) {
                if pref == 0 {
                    if tile_is_solid(left_o) && open(right_o) {
                        set_off!(2);
                    } else if tile_is_solid(right_o) && open(left_o) {
                        set_off!(1);
                    }
                }
                return SignConnectionType(3);
            }
            break 'end;
        }

        // (1)
        if tile_is_solid(right_o) && tile_is_solid(left_o) {
            accept_default!(3, 0);
        }
        // (2)
        let below = at(0, -1);
        if matches!(below, Some(b) if b.foreground_contents == 48) {
            if let Some(obj) = world.interaction_object_at_pos(x, y.wrapping_sub(1)) {
                if obj.interaction_object_type == 4 || obj.interaction_object_type == 8 {
                    let ct = obj.connection_type.0;
                    if ct != 5 && ct != 4 {
                        let mut off = obj.offset_type.0;
                        if off == 1 && obj.pos.x == x.wrapping_sub(1) {
                            off = 2;
                        } else if off == 2 && obj.pos.x == x.wrapping_add(1) {
                            off = 1;
                        }
                        let mut blocked = (off == 0 || off == 1) && !open(right_o);
                        if !blocked && (off == 0 || off == 2) && !open(left_o) {
                            blocked = true;
                        }
                        if !blocked {
                            accept!(ct, off, pref == ct || pref == 0);
                        }
                    }
                }
            }
        }
        // (3)
        let above = at(0, 1);
        if matches!(above, Some(a) if a.foreground_contents == 48) {
            if let Some(obj) = world.interaction_object_at_pos(x, y.wrapping_add(1)) {
                if obj.interaction_object_type == 4 || obj.interaction_object_type == 8 {
                    let off = obj.offset_type.0;
                    let mut blocked = (off == 0 || off == 1) && !open(right_o);
                    if !blocked && (off == 0 || off == 2) && !open(left_o) {
                        blocked = true;
                    }
                    if !blocked {
                        let mut ct = obj.connection_type.0;
                        if ct == 4 {
                            ct = 5;
                        }
                        accept!(ct, off, ct == pref || pref == 0 || pref == 4);
                    }
                }
            }
        }
        // (4)
        if open(left_o) && open(right_o) {
            let (bl, br) = (at(-1, -1), at(1, -1));
            if bl.is_some() && br.is_some() && tile_is_solid(bl) && tile_is_solid(br) {
                accept_default!(1, 0);
            }
            let b = at(0, -1);
            if tile_is_solid(b) || is_column(b) {
                accept_default!(2, 0);
            }
            let (al, ar) = (at(-1, 1), at(1, 1));
            if tile_is_solid(al) && tile_is_solid(ar) {
                accept_default!(5, 0);
            }
        }
        // (5)
        if open(right_o) {
            let b = at(0, -1);
            if (tile_is_solid(b) || is_column(b)) && tile_is_solid(at(1, -1)) {
                accept_default!(1, 1);
            }
            if tile_is_solid(at(0, 1)) && tile_is_solid(at(1, 1)) {
                accept_default!(5, 1);
            }
        }
        // (6)
        if open(left_o) {
            let b = at(0, -1);
            if (tile_is_solid(b) || is_column(b)) && tile_is_solid(at(-1, -1)) {
                accept_default!(1, 2);
            }
            if tile_is_solid(at(0, 1)) && tile_is_solid(at(-1, 1)) {
                accept_default!(5, 2);
            }
        }
        // (7)
        if tile_is_air_water_or_snow(above) {
            if tile_is_solid(right_o) && open(left_o) {
                accept_default!(4, 2);
            }
            if tile_is_solid(left_o) && open(right_o) {
                accept_default!(4, 1);
            }
        }
    }
    // END
    set_off!(fallback_offset);
    SignConnectionType(fallback_type)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[derive(Default)]
    struct W {
        tiles: HashMap<(i32, i32), PTile>,
        half: Vec<(i32, i32)>,
        objs: HashMap<(i32, i32), SignObjectInfo>,
    }
    impl PlacementWorld for W {
        fn tile_at_loaded(&self, x: i32, y: i32) -> Option<PTile> {
            self.tiles.get(&(x, y)).copied()
        }
        fn tile_is_half_depth_at(&self, x: i32, y: i32) -> bool {
            self.half.contains(&(x, y))
        }
        fn interaction_object_type_for_object_at_pos(&self, x: i32, y: i32) -> u16 {
            self.objs.get(&(x, y)).map_or(0, |o| o.interaction_object_type)
        }
        fn interaction_object_at_pos(&self, x: i32, y: i32) -> Option<SignObjectInfo> {
            self.objs.get(&(x, y)).copied()
        }
    }
    const AIR: PTile =
        PTile { type_index: 2, back_wall_type_index: 2, contents: 0, foreground_contents: 0, background_contents: 0 };
    const AIR_BW: PTile = PTile { back_wall_type_index: 7, ..AIR };
    const SOLID: PTile =
        PTile { type_index: 10, back_wall_type_index: 7, contents: 0, foreground_contents: 0, background_contents: 0 };

    /// A 7x7 world of air with the given overrides, centred on (5,5).
    fn world(over: &[((i32, i32), PTile)]) -> W {
        let mut w = W::default();
        for x in 2..9 {
            for y in 2..9 {
                w.tiles.insert((x, y), AIR);
            }
        }
        for (k, t) in over {
            w.tiles.insert(*k, *t);
        }
        w
    }
    const P: IntPair = IntPair { x: 5, y: 5 };

    #[test]
    fn torch_cases() {
        let torch = ItemType(6);
        let w = world(&[((5, 4), SOLID)]);
        assert_eq!(torch_connection_type_for_pos(P, &w, torch), 0);
        let mut col = SOLID;
        col.type_index = 2;
        col.foreground_contents = 100;
        let w = world(&[((5, 4), col)]);
        assert_eq!(torch_connection_type_for_pos(P, &w, torch), 3);
        let mut w = world(&[]);
        w.half.push((5, 5));
        assert_eq!(torch_connection_type_for_pos(P, &w, torch), 2);
        let w = world(&[((6, 5), SOLID)]);
        assert_eq!(torch_connection_type_for_pos(P, &w, torch), 1);
        let w = world(&[((4, 5), SOLID)]);
        assert_eq!(torch_connection_type_for_pos(P, &w, torch), -1);
        let mut door = SOLID;
        door.background_contents = 70;
        let w = world(&[((6, 5), door), ((4, 5), SOLID)]);
        assert_eq!(torch_connection_type_for_pos(P, &w, torch), -1);
        let w = world(&[((5, 5), AIR_BW)]);
        assert_eq!(torch_connection_type_for_pos(P, &w, torch), -2);
        assert_eq!(torch_connection_type_for_pos(P, &world(&[]), torch), -99);
        // chandelier and shark jaw
        assert_eq!(torch_connection_type_for_pos(P, &world(&[]), ItemType(148)), 5);
        assert_eq!(torch_connection_type_for_pos(P, &world(&[]), ItemType(157)), 5);
        assert_eq!(torch_connection_type_for_pos(P, &world(&[((5, 4), SOLID)]), ItemType(157)), 0);
        // gem torch: tile with back wall and empty contents is free floating
        assert_eq!(torch_connection_type_for_pos(P, &world(&[((5, 5), AIR_BW)]), ItemType(75)), -2);
        assert_eq!(torch_connection_type_for_pos(P, &world(&[((5, 4), SOLID)]), ItemType(75)), 0);
        // missing tile
        let mut w = world(&[]);
        w.tiles.remove(&(5, 5));
        assert_eq!(torch_connection_type_for_pos(P, &w, ItemType(75)), -99);
    }

    #[test]
    fn sign_cases() {
        let none = SignConnectionType(0);
        // free air with ground below and open sides: GROUND_SINGLE (2)
        let w = world(&[((5, 4), SOLID)]);
        let mut off = SignOffsetType(0);
        assert_eq!(sign_connection_type_and_offset_for_pos(P, Some(&mut off), &w, none).0, 2);
        assert_eq!(off.0, 0);
        // surrounded by two solid sides: SOLID_BLOCK (3)
        let w = world(&[((4, 5), SOLID), ((6, 5), SOLID)]);
        assert_eq!(sign_connection_type_and_offset_for_pos(P, None, &w, none).0, 3);
        // on a solid tile: 3, with offset hint
        let w = world(&[((5, 5), SOLID), ((4, 5), SOLID)]);
        let mut off = SignOffsetType(0);
        assert_eq!(sign_connection_type_and_offset_for_pos(P, Some(&mut off), &w, none).0, 3);
        assert_eq!(off.0, 2);
        // nothing matches: 0
        let w = world(&[]);
        assert_eq!(sign_connection_type_and_offset_for_pos(P, None, &w, none).0, 0);
        // missing neighbour
        let mut w = world(&[]);
        w.tiles.remove(&(4, 5));
        let mut off = SignOffsetType(2);
        assert_eq!(sign_connection_type_and_offset_for_pos(P, Some(&mut off), &w, none).0, 0);
        assert_eq!(off.0, 0);
        // hanging from ceiling between solids: HANG (5)
        let w = world(&[((4, 6), SOLID), ((6, 6), SOLID)]);
        assert_eq!(sign_connection_type_and_offset_for_pos(P, None, &w, none).0, 5);
        // side wall: solid to the right, air above
        let w = world(&[((6, 5), SOLID)]);
        let mut off = SignOffsetType(0);
        // (7) gives SIDE_WALL with offset 2, but offset filter 0 does not match: fallback is returned and written
        assert_eq!(sign_connection_type_and_offset_for_pos(P, Some(&mut off), &w, none).0, 4);
        assert_eq!(off.0, 2);
        // adjacent sign blocks placement
        let mut fg = AIR;
        fg.foreground_contents = 48;
        let mut w = world(&[((4, 5), fg), ((5, 4), SOLID)]);
        w.objs.insert(
            (4, 5),
            SignObjectInfo {
                interaction_object_type: 4,
                connection_type: SignConnectionType(2),
                offset_type: SignOffsetType(0),
                pos: IntPair { x: 4, y: 5 },
            },
        );
        assert_eq!(sign_connection_type_and_offset_for_pos(P, None, &w, none).0, 0);
    }
}
