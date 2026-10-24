//! Tile predicates, small type maps, the quick sort helper and `impl PlacementWorld for World`.
//!
//!  / `q_sort`.
//! Predicates take `Option<TileRef>` and return `false` for `None`, except `tile_is_burnable_block` and
//! `tile_requires_glow_block` which the spec marks "no NULL check" (they take a `TileRef`).

use std::rc::Rc;

use super::state::{DwObject, DynamicWorld, TileRef, World};
use super::tiles::tile_at_world_position_loaded;
use crate::items::placement::{PTile, PlacementWorld, SignObjectInfo};
use crate::objects::handle::ObjRef;
use crate::objects::type_tables::{object_type_is_interaction_object, object_type_is_plant};
use crate::types::enums::{DynamicObjectType, NPCType, SignConnectionType, SignOffsetType};
use crate::types::structs::IntPair;
use crate::types::tile::Tile;

#[inline]
fn tl(t: Option<TileRef>) -> Option<Tile> {
    t.map(|t| t.get())
}

/// `tileIsAir`: `typeIndex == 2`.
pub fn tile_is_air(t: Option<TileRef>) -> bool {
    matches!(tl(t), Some(t) if t.type_index == 2)
}
/// `tileIsWater`: `typeIndex == 3`.
pub fn tile_is_water(t: Option<TileRef>) -> bool {
    matches!(tl(t), Some(t) if t.type_index == 3)
}
/// `tileIsSwimmable(t, below)`.
pub fn tile_is_swimmable(t: Option<TileRef>, below: Option<TileRef>) -> bool {
    match tl(t) {
        Some(t) if t.type_index == 3 => {
            t.partial_content_left == 0xFF
                || matches!(tl(below), Some(b) if b.type_index == 3 && b.partial_content_left == 0xFF)
        }
        _ => false,
    }
}
/// `tileIsSolid`: non-NULL and `typeIndex` not in {2,3,5}.
pub fn tile_is_solid(t: Option<TileRef>) -> bool {
    matches!(tl(t), Some(t) if !matches!(t.type_index, 2 | 3 | 5))
}
/// `tileIsAirWaterOrSnow`: `typeIndex` in {2,3,5}.
pub fn tile_is_air_water_or_snow(t: Option<TileRef>) -> bool {
    matches!(tl(t), Some(t) if matches!(t.type_index, 2 | 3 | 5))
}
/// `tileIsAirOrSnow`: `typeIndex` in {2,5}.
pub fn tile_is_air_or_snow(t: Option<TileRef>) -> bool {
    matches!(tl(t), Some(t) if matches!(t.type_index, 2 | 5))
}
/// `tileConductsElectricity`.
pub fn tile_conducts_electricity(t: Option<TileRef>) -> bool {
    matches!(tl(t), Some(t) if t.contents == 0x60 || t.foreground_contents == 0x60
        || matches!(t.type_index, 0x35 | 0x36 | 0x38 | 0x39 | 0x1a | 0x43 | 0x44))
}
/// `tileTypeIsGemBlock(type)`: {71..75}.
pub fn tile_type_is_gem_block(type_index: i32) -> bool {
    matches!(type_index, 71 | 72 | 73 | 74 | 75)
}
/// `tileContainsGate`: `contents == 0x92`.
pub fn tile_contains_gate(t: Option<TileRef>) -> bool {
    matches!(tl(t), Some(t) if t.contents == 0x92)
}
/// `tileContainsDoor`: `backgroundContents == 0x46`.
pub fn tile_contains_door(t: Option<TileRef>) -> bool {
    matches!(tl(t), Some(t) if t.background_contents == 0x46)
}
/// `tileContainsTrapDoor`: `backgroundContents == 0x4B`.
pub fn tile_contains_trap_door(t: Option<TileRef>) -> bool {
    matches!(tl(t), Some(t) if t.background_contents == 0x4B)
}
/// `tileIsSemiTransparentSolidBlock`: type in {0x18,0x3b,0x04} or gem block.
pub fn tile_is_semi_transparent_solid_block(t: Option<TileRef>) -> bool {
    matches!(tl(t), Some(t) if matches!(t.type_index, 0x18 | 0x3b | 0x04) || tile_type_is_gem_block(t.type_index as i32))
}

fn usable_door_like(
    t: Option<TileRef>,
    contains: bool,
    pos: IntPair,
    world: &World,
    path_user: Option<&ObjRef>,
) -> bool {
    let Some(tile) = tl(t) else { return false };
    if !contains || tile.terrain_slow_factor == 0xFF {
        return false;
    }
    match world.dynamic_world() {
        Some(dw) => dw.door_can_be_used_by_path_user(path_user, pos),
        None => false,
    }
}

/// `tileContainsUsableDoor`: `contains && terrainSlowFactor != 0xFF && [dw doorCanBeUsedByPathUser:atPos:]`.
pub fn tile_contains_usable_door(t: Option<TileRef>, pos: IntPair, world: &World, path_user: Option<&ObjRef>) -> bool {
    usable_door_like(t, tile_contains_door(t), pos, world, path_user)
}
/// `tileContainsUsableTrapDoor`.
pub fn tile_contains_usable_trap_door(
    t: Option<TileRef>,
    pos: IntPair,
    world: &World,
    path_user: Option<&ObjRef>,
) -> bool {
    usable_door_like(t, tile_contains_trap_door(t), pos, world, path_user)
}
/// `tileContainsUsableGate`.
pub fn tile_contains_usable_gate(t: Option<TileRef>, pos: IntPair, world: &World, path_user: Option<&ObjRef>) -> bool {
    usable_door_like(t, tile_contains_gate(t), pos, world, path_user)
}

/// `tileContainsUsableElevator(t, pos, world, pathUser, blacklist)`: `contents == 0x67` and the tile above
/// (`pos.x, pos.y+1`, via `tileAtWorldPositionLoaded`) also `0x67`; the shaft's motor must exist and have power
/// and its `pos` must not be in the blacklist. `pathUser` is unused.
pub fn tile_contains_usable_elevator(
    t: Option<TileRef>,
    pos: IntPair,
    world: &World,
    _path_user: Option<&ObjRef>,
    blacklist: &[IntPair],
) -> bool {
    let Some(tile) = tl(t) else { return false };
    if tile.contents != 0x67 {
        return false;
    }
    let above = tile_at_world_position_loaded(pos.x, pos.y.wrapping_add(1), world);
    match tl(above) {
        Some(a) if a.contents == 0x67 => {}
        _ => return false,
    }
    let Some(dw) = world.dynamic_world() else { return false };
    match dw.elevator_motor_for_shaft_at_pos(pos) {
        Some(m) if m.has_required_power => !blacklist.contains(&m.pos),
        _ => false,
    }
}

/// `tileIsClimable`: `tileIsTree || (t && (fg == 0x42 || usable trap door || contents == 0x67))`.
pub fn tile_is_climable(t: Option<TileRef>, pos: IntPair, world: &World, path_user: Option<&ObjRef>) -> bool {
    if tile_is_tree(t) {
        return true;
    }
    match tl(t) {
        Some(tile) => {
            tile.foreground_contents == 0x42
                || tile_contains_usable_trap_door(t, pos, world, path_user)
                || tile.contents == 0x67
        }
        None => false,
    }
}

const TRUNK_CONTENTS: [u8; 28] = [
    0x07, 0x0A, 0x0D, 0x10, 0x13, 0x5A, 0x04, 0x16, 0x19, 0x6D, 0x70, 0x73, 0x76, 0x79, 0x08, 0x0B, 0x0E, 0x11, 0x14,
    0x5B, 0x05, 0x17, 0x1A, 0x6F, 0x72, 0x75, 0x78, 0x7B,
];
const CROWN_CONTENTS: [u8; 15] =
    [0x03, 0x09, 0x0C, 0x06, 0x2B, 0x0F, 0x12, 0x59, 0x15, 0x18, 0x6E, 0x71, 0x74, 0x77, 0x7A];
const DEAD_TRUNK_CONTENTS: [u8; 9] = [0x1D, 0x1F, 0x21, 0x24, 0x26, 0x5D, 0x1C, 0x28, 0x2A];
const DEAD_CROWN_CONTENTS: [u8; 10] = [0x1B, 0x1E, 0x20, 0x22, 0x2C, 0x23, 0x25, 0x5C, 0x27, 0x29];

/// `tileIsTreeTrunk`: dead trunk or contents in the trunk set.
pub fn tile_is_tree_trunk(t: Option<TileRef>) -> bool {
    match tl(t) {
        Some(tile) => tile_is_dead_tree_trunk(t) || TRUNK_CONTENTS.contains(&tile.contents),
        None => false,
    }
}
/// `tileIsTree`: trunk or crown contents.
pub fn tile_is_tree(t: Option<TileRef>) -> bool {
    match tl(t) {
        Some(tile) => tile_is_tree_trunk(t) || CROWN_CONTENTS.contains(&tile.contents),
        None => false,
    }
}
/// `tileIsBush`: crown contents plus the dead-crown contents.
pub fn tile_is_bush(t: Option<TileRef>) -> bool {
    matches!(tl(t), Some(tile) if CROWN_CONTENTS.contains(&tile.contents) || DEAD_CROWN_CONTENTS.contains(&tile.contents))
}
/// `tileIsDeadTreeTrunk`.
pub fn tile_is_dead_tree_trunk(t: Option<TileRef>) -> bool {
    matches!(tl(t), Some(tile) if DEAD_TRUNK_CONTENTS.contains(&tile.contents))
}
/// `tileIsDeadTree`: dead trunk or dead crown.
pub fn tile_is_dead_tree(t: Option<TileRef>) -> bool {
    matches!(tl(t), Some(tile) if DEAD_TRUNK_CONTENTS.contains(&tile.contents) || DEAD_CROWN_CONTENTS.contains(&tile.contents))
}
/// `tileIsPlant`: `foregroundContents` in the plant set.
pub fn tile_is_plant(t: Option<TileRef>) -> bool {
    matches!(tl(t), Some(tile) if matches!(tile.foreground_contents,
        0x43 | 0x44 | 0x47 | 0x48 | 0x49 | 0x4A | 0x93 | 0x94 | 0x4F | 0x50 | 0x4C | 0x7D | 0x51 | 0x7C))
}
/// `tileIsBushOrScrub`: `tileIsBush || tileIsPlant`.
pub fn tile_is_bush_or_scrub(t: Option<TileRef>) -> bool {
    tile_is_bush(t) || tile_is_plant(t)
}

/// `tileTypeIsRock(typeIndex)`.
pub fn tile_type_is_rock(type_index: i32) -> bool {
    matches!(type_index, 1 | 10 | 11 | 12 | 13 | 76 | 77 | 14 | 15 | 26)
        || tile_type_is_gem_block(type_index)
        || matches!(
            type_index,
            0x43 | 0x44
                | 0x45
                | 0x35
                | 0x36
                | 0x37
                | 0x38
                | 0x39
                | 0x10
                | 0x11
                | 0x12
                | 0x13
                | 0x14
                | 0x1d
                | 0x33
                | 0x34
                | 0x1e
        )
}
/// `tileIsRock`.
pub fn tile_is_rock(t: Option<TileRef>) -> bool {
    matches!(tl(t), Some(tile) if tile_type_is_rock(tile.type_index as i32))
}
/// `tileIsUnplacedBlockMinableWithPickaxe`: `typeIndex` in {1,0x0C,0x0E,0x11,0x13,0x1D,0x33}.
pub fn tile_is_unplaced_block_minable_with_pickaxe(t: Option<TileRef>) -> bool {
    matches!(tl(t), Some(tile) if matches!(tile.type_index, 1 | 0x0C | 0x0E | 0x11 | 0x13 | 0x1D | 0x33))
}
/// `tileIsNotUnminedBlockSoCanBeRemovedOnRepair`: `t && !minable(t) && typeIndex != 0x1F`.
pub fn tile_is_not_unmined_block_so_can_be_removed_on_repair(t: Option<TileRef>) -> bool {
    match tl(t) {
        Some(tile) => !tile_is_unplaced_block_minable_with_pickaxe(t) && tile.type_index != 0x1F,
        None => false,
    }
}
/// `backWallIsMutable`: `backWallTypeIndex` not in {0x01,0x26,0x27,0x28,0x29,0x1F}.
pub fn back_wall_is_mutable(t: Option<TileRef>) -> bool {
    matches!(tl(t), Some(tile) if !matches!(tile.back_wall_type_index, 0x01 | 0x26 | 0x27 | 0x28 | 0x29 | 0x1F))
}
/// `backWallRemovesWithRepairTool` = `backWallIsMutable`.
pub fn back_wall_removes_with_repair_tool(t: Option<TileRef>) -> bool {
    back_wall_is_mutable(t)
}
/// `tileForegroundContentsCanCoExistWithBlocksAndElevatorShafts`.
pub fn tile_foreground_contents_can_co_exist_with_blocks_and_elevator_shafts(t: Option<TileRef>) -> bool {
    matches!(tl(t), Some(tile) if matches!(tile.foreground_contents,
        0x62 | 0x31 | 0x57 | 0x32 | 0x61 | 0x69 | 0x66 | 0x39 | 0x37 | 0x35 | 0x33 | 0x3B | 0x52 | 0x53 | 0x54 | 0x55 | 0x56))
}
/// `tileAllowsPlacingOfSolidBlocks`.
pub fn tile_allows_placing_of_solid_blocks(t: Option<TileRef>) -> bool {
    match tl(t) {
        None => false,
        Some(tile) if tile.contents == 0x67 => {
            tile.foreground_contents == 0 || tile_foreground_contents_can_co_exist_with_blocks_and_elevator_shafts(t)
        }
        Some(tile) => tile.foreground_contents == 0 && tile.contents == 0,
    }
}
/// `tileIsBurnableBlock` (no NULL check): `typeIndex` in {9,0x15,0x16,0x17,0x20}.
pub fn tile_is_burnable_block(t: TileRef) -> bool {
    matches!(t.get().type_index, 9 | 0x15 | 0x16 | 0x17 | 0x20)
}
/// `tileIsBurnable(t, dynamicWorld, pos)`: an interaction object of type 3 with `itemType != 0xAA` burns;
/// otherwise the tile itself must be burnable.
pub fn tile_is_burnable(t: Option<TileRef>, dynamic_world: Option<&Rc<DynamicWorld>>, pos: IntPair) -> bool {
    if let Some(dw) = dynamic_world {
        if let Some(o) = dw.interaction_object_at_pos(pos) {
            if o.interaction_object_type == 3 && o.item_type != 0xAA {
                return true;
            }
        }
    }
    match tl(t) {
        Some(tile) => {
            !tile_is_water(t)
                && (tile_is_tree(t)
                    || tile_is_dead_tree(t)
                    || tile_is_burnable_block(t.unwrap())
                    || tile.foreground_contents == 0x42
                    || tile_is_plant(t))
        }
        None => false,
    }
}
/// `tileIsWorkbench`: `contents` in {0x2E,0x2F}.
pub fn tile_is_workbench(t: Option<TileRef>) -> bool {
    matches!(tl(t), Some(tile) if matches!(tile.contents, 0x2E | 0x2F))
}
/// `tileIsInteractionObject`: workbench or `contents == 0x30`.
pub fn tile_is_interaction_object(t: Option<TileRef>) -> bool {
    tile_is_workbench(t) || matches!(tl(t), Some(tile) if tile.contents == 0x30)
}
/// `tileIsHalfDepth(t, world, pos)`.
pub fn tile_is_half_depth(t: Option<TileRef>, world: &World, pos: IntPair) -> bool {
    let Some(tile) = tl(t) else { return false };
    if matches!(tile.type_index, 2 | 3 | 5)
        && (tile_is_tree(t) || tile_is_dead_tree(t) || matches!(tile.contents, 0x2E | 0x67 | 0x68))
    {
        return true;
    }
    if tile.contents == 0x30 {
        if let Some(dw) = world.dynamic_world() {
            if let Some(o) = dw.interaction_object_at_pos(pos) {
                return o.interaction_object_type == 2 && matches!(o.chest_type, 0 | 1 | 3 | 4 | 5 | 6);
            }
        }
    }
    false
}
/// `tileTypeIsPortalBaseStone(type)`: {25, 33..37, 42..47, 60..65}.
pub fn tile_type_is_portal_base_stone(type_index: i32) -> bool {
    matches!(type_index, 25 | 33..=37 | 42..=47 | 60..=65)
}
/// `tileRequiresGlowBlock` (no NULL check): type in {0x10,0x1A,0x4D}, gem block, or `contents == 0x4D`.
pub fn tile_requires_glow_block(t: TileRef) -> bool {
    let tile = t.get();
    matches!(tile.type_index, 0x10 | 0x1A | 0x4D)
        || tile_type_is_gem_block(tile.type_index as i32)
        || tile.contents == 0x4D
}

const PAINT_SOLID: [u8; 40] = [
    0x01, 0x06, 0x07, 0x08, 0x09, 0x0A, 0x0B, 0x0C, 0x0D, 0x0E, 0x0F, 0x11, 0x12, 0x13, 0x14, 0x18, 0x1A, 0x1B, 0x1C,
    0x1D, 0x1E, 0x20, 0x30, 0x31, 0x32, 0x33, 0x34, 0x35, 0x36, 0x37, 0x38, 0x39, 0x3A, 0x3B, 0x43, 0x44, 0x45, 0x46,
    0x4C, 0x4D,
];

fn removable(o: &Option<DwObject>, path_user: Option<&ObjRef>) -> bool {
    // nil receiver => NO; a nil `path_user` is treated as NO (the object code assumes non-nil,).
    match (o, path_user) {
        (Some(o), Some(b)) => o.obj.can_be_removed_by_blockhead(&o.obj, b),
        _ => false,
    }
}

/// `tileIsPaintable(t, pos, world, pathUser)`: the type (solid) or back wall type (non-solid) is in the paintable
/// set, else one of the foreground-object checks passes.
pub fn tile_is_paintable(t: Option<TileRef>, pos: IntPair, world: &World, path_user: Option<&ObjRef>) -> bool {
    let Some(tile) = tl(t) else { return false };
    let solid = tile_is_solid(t);
    let v = if solid { tile.type_index } else { tile.back_wall_type_index };
    let in_set = if solid {
        PAINT_SOLID.contains(&v) || tile_type_is_gem_block(v as i32)
    } else {
        (PAINT_SOLID.contains(&v) && v != 0x20) || tile_type_is_gem_block(v as i32)
    };
    if in_set {
        return true;
    }
    let dw = world.dynamic_world();
    let q = |f: &dyn Fn(&DynamicWorld) -> Option<DwObject>| -> Option<DwObject> { dw.as_ref().and_then(|d| f(d)) };
    // (a)
    if tile.foreground_contents == 0x42 {
        if removable(&q(&|d| d.ladder_at_pos(pos)), path_user) {
            return true;
        }
    } else if tile.foreground_contents == 0x64 && removable(&q(&|d| d.column_at_pos(pos)), path_user) {
        return true;
    }
    // (b)
    if tile.foreground_contents == 0x30 || tile.contents == 0x30 {
        if let Some(o) = q(&|d| d.interaction_object_at_pos(pos)) {
            if o.is_paintable && removable(&Some(o), path_user) {
                return true;
            }
        }
    }
    // (c)
    if tile.contents == 0x65 {
        if removable(&q(&|d| d.stairs_at_pos(pos)), path_user) {
            return true;
        }
    } else if tile.contents == 0x67 && removable(&q(&|d| d.elevator_shaft_at_pos(pos)), path_user) {
        return true;
    }
    false
}

// ---------------------------------------------------------------------------------------------------
// Small type maps
// ---------------------------------------------------------------------------------------------------

/// `NPC_DYNAMIC_OBJECT_TYPES`: DODO, DROPBEAR, DONKEY, CLOWNFISH, SHARK, CAVETROLL,
/// SCORPION, YAK.
const NPC_DYNAMIC_OBJECT_TYPES: [u32; 8] = [13, 25, 28, 35, 36, 39, 51, 63];

/// `dynamicObjectTypeForNPCType`: 1..=8 -> table, else 0.
pub fn dynamic_object_type_for_npc_type(npc: NPCType) -> DynamicObjectType {
    match npc.0 {
        n @ 1..=8 => DynamicObjectType(NPC_DYNAMIC_OBJECT_TYPES[(n - 1) as usize]),
        _ => DynamicObjectType(0),
    }
}

/// `objectTypeMayHaveArtificalLight` (sic): 0x11, 0x10, 0x12, then plant, then interaction object.
pub fn object_type_may_have_artifical_light(t: i32) -> bool {
    if t == 0x11 || t == 0x10 || t == 0x12 {
        return true;
    }
    object_type_is_plant(t) || object_type_is_interaction_object(t)
}

/// `quickSort(numbers, array_size, orderedIndices)`: `orderedIndices[i] = i`, then `q_sort(0, size-1)`.
/// `array_size == 0` does nothing (the original reads/writes element 0 unchanged).
pub fn quick_sort(numbers: &mut [f32], size: u32, ordered_indices: &mut [u32]) {
    for i in 0..size as usize {
        ordered_indices[i] = i as u32;
    }
    if size == 0 {
        return;
    }
    q_sort(numbers, 0, (size as i32).wrapping_sub(1), ordered_indices);
}

/// `q_sort(numbers, left, right, orderedIndices)`: first-element pivot, parallel permutation, float32 round-trip
/// of the partition index in the recursion tests. Not stable.
pub fn q_sort(numbers: &mut [f32], left: i32, right: i32, ordered_indices: &mut [u32]) {
    let pivot = numbers[left as usize];
    let piv_idx = ordered_indices[left as usize];
    let mut l = left;
    let mut r = right;
    while l < r {
        while pivot <= numbers[r as usize] && l < r {
            r -= 1;
        }
        if l != r {
            numbers[l as usize] = numbers[r as usize];
            ordered_indices[l as usize] = ordered_indices[r as usize];
            l += 1;
        }
        while numbers[l as usize] <= pivot && l < r {
            l += 1;
        }
        if l != r {
            numbers[r as usize] = numbers[l as usize];
            ordered_indices[r as usize] = ordered_indices[l as usize];
            r -= 1;
        }
    }
    numbers[l as usize] = pivot;
    ordered_indices[l as usize] = piv_idx;
    let p = l as f32;
    if (left as f32) < p {
        q_sort(numbers, left, (p as i32) - 1, ordered_indices);
    }
    if p < (right as f32) {
        q_sort(numbers, (p as i32) + 1, right, ordered_indices);
    }
}

// ---------------------------------------------------------------------------------------------------
// impl PlacementWorld for World
// ---------------------------------------------------------------------------------------------------

impl PlacementWorld for World {
    fn tile_at_loaded(&self, x: i32, y: i32) -> Option<PTile> {
        tile_at_world_position_loaded(x, y, self).map(|t| {
            let t = t.get();
            PTile {
                type_index: t.type_index,
                back_wall_type_index: t.back_wall_type_index,
                contents: t.contents,
                foreground_contents: t.foreground_contents,
                background_contents: t.background_contents,
            }
        })
    }
    fn tile_is_half_depth_at(&self, x: i32, y: i32) -> bool {
        let t = tile_at_world_position_loaded(x, y, self);
        t.is_some() && tile_is_half_depth(t, self, IntPair { x, y })
    }
    fn interaction_object_type_for_object_at_pos(&self, x: i32, y: i32) -> u16 {
        match self.dynamic_world() {
            Some(dw) => dw.interaction_object_type_for_object_at_pos(IntPair { x, y }),
            None => 0,
        }
    }
    fn interaction_object_at_pos(&self, x: i32, y: i32) -> Option<SignObjectInfo> {
        let pos = IntPair { x, y };
        let o = self.dynamic_world()?.interaction_object_at_pos(pos)?;
        // Note: non-Sign objects have no connection/offset getters; they read as 0.
        let (conn, off) = o.sign_info.unwrap_or((0, 0));
        Some(SignObjectInfo {
            interaction_object_type: o.interaction_object_type,
            connection_type: SignConnectionType(conn as _),
            offset_type: SignOffsetType(off as _),
            pos,
        })
    }
}

/// Test fixtures shared by the world_state unit tests: a stub dynamic object and a scriptable `DynamicWorld`.
#[cfg(test)]
pub(crate) mod testutil {
    use super::*;
    use crate::objects::base::{DynamicObject, DynamicObjectBase};
    use crate::objects::class_info::{ClassInfo, DYNAMIC_OBJECT_CLASS};
    use crate::objects::handle;
    use crate::world::state::{DynamicWorldQueries, ElevatorMotorInfo, PhysicalBlockRef};
    use std::any::Any;
    use std::cell::{Cell, RefCell};
    use std::collections::HashMap;

    pub struct StubObj {
        pub base: RefCell<DynamicObjectBase>,
        pub removable: Cell<bool>,
        /// `objectType`; `MAX_DYNAMIC_OBJECT_TYPES` (the base answer) by default.
        pub ty: Cell<i32>,
    }
    impl DynamicObject for StubObj {
        fn object_type(&self) -> i32 {
            self.ty.get()
        }
        fn base(&self) -> &RefCell<DynamicObjectBase> {
            &self.base
        }
        fn class_info(&self) -> &'static ClassInfo {
            &DYNAMIC_OBJECT_CLASS
        }
        fn as_any(&self) -> &dyn Any {
            self
        }
        fn can_be_removed_by_blockhead(&self, _me: &ObjRef, _b: &ObjRef) -> bool {
            self.removable.get()
        }
    }
    pub fn new_obj() -> ObjRef {
        handle::alloc_obj(|w| StubObj {
            base: RefCell::new(DynamicObjectBase::alloc_zeroed_with(w)),
            removable: Cell::new(true),
            ty: Cell::new(crate::objects::base::MAX_DYNAMIC_OBJECT_TYPES),
        })
    }
    pub fn dw_obj_of_type(object_type: i32) -> DwObject {
        let o = dw_obj(0, 0, 0, false, true);
        let stub = crate::objects::handle::downcast::<StubObj>(&o.obj).unwrap();
        stub.ty.set(object_type);
        o
    }
    pub fn dw_obj(item_type: i32, io_type: u16, chest: i32, paintable: bool, removable: bool) -> DwObject {
        let o = handle::alloc_obj(|w| StubObj {
            base: RefCell::new(DynamicObjectBase::alloc_zeroed_with(w)),
            removable: Cell::new(removable),
            ty: Cell::new(crate::objects::base::MAX_DYNAMIC_OBJECT_TYPES),
        });
        DwObject {
            obj: o,
            item_type,
            interaction_object_type: io_type,
            chest_type: chest,
            is_paintable: paintable,
            sign_info: None,
        }
    }

    #[derive(Default)]
    pub struct FakeDw {
        pub door: Cell<bool>,
        pub motor: RefCell<HashMap<(i32, i32), ElevatorMotorInfo>>,
        pub io: RefCell<HashMap<(i32, i32), DwObject>>,
        pub ladder: RefCell<HashMap<(i32, i32), DwObject>>,
        pub column: RefCell<HashMap<(i32, i32), DwObject>>,
        pub stairs: RefCell<HashMap<(i32, i32), DwObject>>,
        pub shaft: RefCell<HashMap<(i32, i32), DwObject>>,
        pub painting: RefCell<HashMap<(i32, i32), DwObject>>,
    }
    fn k(p: IntPair) -> (i32, i32) {
        (p.x, p.y)
    }
    impl DynamicWorldQueries for Rc<FakeDw> {
        fn door_can_be_used_by_path_user(&self, _p: Option<&ObjRef>, _pos: IntPair) -> bool {
            self.door.get()
        }
        fn elevator_motor_for_shaft_at_pos(&self, pos: IntPair) -> Option<ElevatorMotorInfo> {
            self.motor.borrow().get(&k(pos)).copied()
        }
        fn interaction_object_at_pos(&self, pos: IntPair) -> Option<DwObject> {
            self.io.borrow().get(&k(pos)).cloned()
        }
        fn ladder_at_pos(&self, pos: IntPair) -> Option<DwObject> {
            self.ladder.borrow().get(&k(pos)).cloned()
        }
        fn column_at_pos(&self, pos: IntPair) -> Option<DwObject> {
            self.column.borrow().get(&k(pos)).cloned()
        }
        fn stairs_at_pos(&self, pos: IntPair) -> Option<DwObject> {
            self.stairs.borrow().get(&k(pos)).cloned()
        }
        fn elevator_shaft_at_pos(&self, pos: IntPair) -> Option<DwObject> {
            self.shaft.borrow().get(&k(pos)).cloned()
        }
        fn painting_at_pos(&self, pos: IntPair) -> Option<DwObject> {
            self.painting.borrow().get(&k(pos)).cloned()
        }
    }

    /// World of width `w_macro` with one fully valid block at macro (0,0) installed and a fake dynamic world.
    pub fn world_with_dw(w_macro: i32) -> (Rc<World>, Rc<FakeDw>, PhysicalBlockRef) {
        let world = World::alloc(w_macro);
        let dw = Rc::new(FakeDw::default());
        *world.dynamic_world.borrow_mut() = Some(Rc::new(DynamicWorld::new(Box::new(dw.clone()))));
        let pb = world.take_physical_block();
        pb.valid.set(true);
        *world.macro_tiles.get(0).unwrap().physical_block.borrow_mut() = Some(pb);
        (world, dw, pb)
    }
}

#[cfg(test)]
mod tests {
    use super::testutil::*;
    use super::*;
    use crate::world::state::ElevatorMotorInfo;

    fn tile_with(f: impl FnOnce(&mut Tile)) -> (Rc<World>, TileRef) {
        let (w, _dw, pb) = world_with_dw(32);
        let t = TileRef::new(pb, 0);
        t.update(f);
        (w, t)
    }

    #[test]
    fn basic_predicates_exhaustive_type_index() {
        let (_w, _dw, pb) = world_with_dw(32);
        let t = TileRef::new(pb, 0);
        for ty in 0..=255u8 {
            t.set_type_index(ty);
            let s = Some(t);
            assert_eq!(tile_is_air(s), ty == 2);
            assert_eq!(tile_is_water(s), ty == 3);
            assert_eq!(tile_is_solid(s), !matches!(ty, 2 | 3 | 5));
            assert_eq!(tile_is_air_water_or_snow(s), matches!(ty, 2 | 3 | 5));
            assert_eq!(tile_is_air_or_snow(s), matches!(ty, 2 | 5));
            assert_eq!(tile_is_semi_transparent_solid_block(s), matches!(ty, 0x18 | 0x3b | 0x04 | 71..=75));
            assert_eq!(tile_is_burnable_block(t), matches!(ty, 9 | 0x15 | 0x16 | 0x17 | 0x20));
            assert_eq!(
                tile_is_unplaced_block_minable_with_pickaxe(s),
                matches!(ty, 1 | 0x0C | 0x0E | 0x11 | 0x13 | 0x1D | 0x33)
            );
            assert_eq!(
                tile_is_not_unmined_block_so_can_be_removed_on_repair(s),
                !matches!(ty, 1 | 0x0C | 0x0E | 0x11 | 0x13 | 0x1D | 0x33) && ty != 0x1F
            );
            assert_eq!(tile_requires_glow_block(t), matches!(ty, 0x10 | 0x1A | 0x4D | 71..=75));
            assert_eq!(tile_is_rock(s), tile_type_is_rock(ty as i32));
            assert_eq!(tile_type_is_portal_base_stone(ty as i32), matches!(ty, 25 | 33..=37 | 42..=47 | 60..=65));
        }
        for ty in [1, 10, 11, 12, 13, 76, 77, 14, 15, 26, 71, 75, 0x43, 0x1e, 0x33] {
            assert!(tile_type_is_rock(ty));
        }
        assert!(!tile_type_is_rock(2) && !tile_type_is_rock(6));
    }

    #[test]
    fn none_is_false_everywhere() {
        let (w, _dw, _pb) = world_with_dw(32);
        let p = IntPair { x: 0, y: 0 };
        assert!(!tile_is_air(None) && !tile_is_water(None) && !tile_is_solid(None) && !tile_is_tree(None));
        assert!(!tile_is_swimmable(None, None) && !tile_conducts_electricity(None) && !tile_is_plant(None));
        assert!(!tile_is_paintable(None, p, &w, None) && !tile_is_half_depth(None, &w, p));
        assert!(!tile_allows_placing_of_solid_blocks(None) && !back_wall_is_mutable(None));
        assert!(!tile_is_burnable(None, None, p));
    }

    #[test]
    fn contents_predicates_exhaustive() {
        let (_w, _dw, pb) = world_with_dw(32);
        let t = TileRef::new(pb, 0);
        let trunk = [
            0x07u8, 0x0A, 0x0D, 0x10, 0x13, 0x5A, 0x04, 0x16, 0x19, 0x6D, 0x70, 0x73, 0x76, 0x79, 0x08, 0x0B, 0x0E,
            0x11, 0x14, 0x5B, 0x05, 0x17, 0x1A, 0x6F, 0x72, 0x75, 0x78, 0x7B,
        ];
        let crown = [0x03u8, 0x09, 0x0C, 0x06, 0x2B, 0x0F, 0x12, 0x59, 0x15, 0x18, 0x6E, 0x71, 0x74, 0x77, 0x7A];
        let dead_trunk = [0x1Du8, 0x1F, 0x21, 0x24, 0x26, 0x5D, 0x1C, 0x28, 0x2A];
        let dead_crown = [0x1Bu8, 0x1E, 0x20, 0x22, 0x2C, 0x23, 0x25, 0x5C, 0x27, 0x29];
        for c in 0..=255u8 {
            t.set_contents(c);
            let s = Some(t);
            let is_dt = dead_trunk.contains(&c);
            assert_eq!(tile_is_dead_tree_trunk(s), is_dt);
            assert_eq!(tile_is_dead_tree(s), is_dt || dead_crown.contains(&c));
            assert_eq!(tile_is_tree_trunk(s), is_dt || trunk.contains(&c));
            assert_eq!(tile_is_tree(s), is_dt || trunk.contains(&c) || crown.contains(&c));
            assert_eq!(tile_is_bush(s), crown.contains(&c) || dead_crown.contains(&c));
            assert_eq!(tile_contains_gate(s), c == 0x92);
            assert_eq!(tile_is_workbench(s), matches!(c, 0x2E | 0x2F));
            assert_eq!(tile_is_interaction_object(s), matches!(c, 0x2E | 0x2F | 0x30));
        }
        t.set_contents(0);
        for c in 0..=255u8 {
            t.set_foreground_contents(c);
            assert_eq!(
                tile_is_plant(Some(t)),
                matches!(
                    c,
                    0x43 | 0x44 | 0x47 | 0x48 | 0x49 | 0x4A | 0x93 | 0x94 | 0x4F | 0x50 | 0x4C | 0x7D | 0x51 | 0x7C
                )
            );
            assert_eq!(
                tile_foreground_contents_can_co_exist_with_blocks_and_elevator_shafts(Some(t)),
                matches!(
                    c,
                    0x62 | 0x31
                        | 0x57
                        | 0x32
                        | 0x61
                        | 0x69
                        | 0x66
                        | 0x39
                        | 0x37
                        | 0x35
                        | 0x33
                        | 0x3B
                        | 0x52
                        | 0x53
                        | 0x54
                        | 0x55
                        | 0x56
                )
            );
            assert_eq!(tile_allows_placing_of_solid_blocks(Some(t)), c == 0);
        }
        t.set_foreground_contents(0);
        for c in 0..=255u8 {
            t.set_background_contents(c);
            assert_eq!(tile_contains_door(Some(t)), c == 0x46);
            assert_eq!(tile_contains_trap_door(Some(t)), c == 0x4B);
        }
        for c in 0..=255u8 {
            t.set_back_wall_type_index(c);
            assert_eq!(back_wall_is_mutable(Some(t)), !matches!(c, 0x01 | 0x26 | 0x27 | 0x28 | 0x29 | 0x1F));
        }
    }

    #[test]
    fn electricity_swimmable_elevator_placing() {
        let (_w, t) = tile_with(|t| t.contents = 0x60);
        assert!(tile_conducts_electricity(Some(t)));
        t.set_contents(0);
        t.set_type_index(0x35);
        assert!(tile_conducts_electricity(Some(t)));
        t.set_type_index(3);
        t.set_partial_content_left(0xFF);
        assert!(tile_is_swimmable(Some(t), None));
        t.set_partial_content_left(1);
        assert!(!tile_is_swimmable(Some(t), None));
        t.set_contents(0x67);
        t.set_foreground_contents(0x31);
        assert!(tile_allows_placing_of_solid_blocks(Some(t)));
        t.set_foreground_contents(0x42);
        assert!(!tile_allows_placing_of_solid_blocks(Some(t)));
    }

    #[test]
    fn usable_doors_and_elevators() {
        let (w, dw, pb) = world_with_dw(32);
        let t = TileRef::new(pb, 0);
        t.set_background_contents(0x46);
        t.set_terrain_slow_factor(0);
        let p = IntPair { x: 0, y: 0 };
        assert!(!tile_contains_usable_door(Some(t), p, &w, None));
        dw.door.set(true);
        assert!(tile_contains_usable_door(Some(t), p, &w, None));
        t.set_terrain_slow_factor(0xFF);
        assert!(!tile_contains_usable_door(Some(t), p, &w, None));
        // nil dynamic world
        t.set_terrain_slow_factor(0);
        *w.dynamic_world.borrow_mut() = None;
        assert!(!tile_contains_usable_door(Some(t), p, &w, None));
        // elevator: tile (0,0) and (0,1)
        let (w, dw, pb) = world_with_dw(32);
        TileRef::new(pb, 0).set_contents(0x67);
        TileRef::new(pb, 32).set_contents(0x67);
        let p = IntPair { x: 0, y: 0 };
        let t = Some(TileRef::new(pb, 0));
        assert!(!tile_contains_usable_elevator(t, p, &w, None, &[]));
        dw.motor
            .borrow_mut()
            .insert((0, 0), ElevatorMotorInfo { has_required_power: true, pos: IntPair { x: 5, y: 5 } });
        assert!(tile_contains_usable_elevator(t, p, &w, None, &[]));
        assert!(!tile_contains_usable_elevator(t, p, &w, None, &[IntPair { x: 5, y: 5 }]));
        assert!(tile_contains_usable_elevator(t, p, &w, None, &[IntPair { x: 5, y: 6 }]));
        TileRef::new(pb, 32).set_contents(0);
        assert!(!tile_contains_usable_elevator(t, p, &w, None, &[]));
    }

    #[test]
    fn half_depth_and_burnable() {
        let (w, dw, pb) = world_with_dw(32);
        let t = TileRef::new(pb, 0);
        let p = IntPair { x: 0, y: 0 };
        t.set_type_index(2);
        t.set_contents(0x67);
        assert!(tile_is_half_depth(Some(t), &w, p));
        t.set_contents(0x30);
        assert!(!tile_is_half_depth(Some(t), &w, p));
        for (chest, expect) in [(0, true), (1, true), (2, false), (3, true), (4, true), (5, true), (6, true)] {
            dw.io.borrow_mut().insert((0, 0), dw_obj(0, 2, chest, false, true));
            assert_eq!(tile_is_half_depth(Some(t), &w, p), expect, "chest {}", chest);
        }
        dw.io.borrow_mut().insert((0, 0), dw_obj(0, 3, 0, false, true));
        assert!(!tile_is_half_depth(Some(t), &w, p));
        // burnable
        let dwr = w.dynamic_world();
        assert!(tile_is_burnable(Some(t), dwr.as_ref(), p)); // interaction object type 3, itemType != 0xAA
        dw.io.borrow_mut().insert((0, 0), dw_obj(0xAA, 3, 0, false, true));
        assert!(!tile_is_burnable(Some(t), dwr.as_ref(), p));
        t.set_type_index(9);
        assert!(tile_is_burnable(Some(t), dwr.as_ref(), p));
        t.set_type_index(3);
        assert!(!tile_is_burnable(Some(t), dwr.as_ref(), p));
    }

    #[test]
    fn paintable_branches() {
        let (w, dw, pb) = world_with_dw(32);
        let t = TileRef::new(pb, 0);
        let p = IntPair { x: 0, y: 0 };
        let user = new_obj();
        // solid, type in set
        t.set_type_index(6);
        assert!(tile_is_paintable(Some(t), p, &w, Some(&user)));
        // solid, 0x20 is in the solid set but not in the back wall set
        t.set_type_index(0x20);
        assert!(tile_is_paintable(Some(t), p, &w, Some(&user)));
        t.set_type_index(2); // air: back wall decides
        t.set_back_wall_type_index(0x20);
        assert!(!tile_is_paintable(Some(t), p, &w, Some(&user)));
        t.set_back_wall_type_index(0x06);
        assert!(tile_is_paintable(Some(t), p, &w, Some(&user)));
        t.set_back_wall_type_index(0x02);
        assert!(!tile_is_paintable(Some(t), p, &w, Some(&user)));
        // ladder
        t.set_foreground_contents(0x42);
        assert!(!tile_is_paintable(Some(t), p, &w, Some(&user)));
        dw.ladder.borrow_mut().insert((0, 0), dw_obj(0, 0, 0, false, true));
        assert!(tile_is_paintable(Some(t), p, &w, Some(&user)));
        assert!(!tile_is_paintable(Some(t), p, &w, None)); // nil path user => NO
        dw.ladder.borrow_mut().insert((0, 0), dw_obj(0, 0, 0, false, false));
        assert!(!tile_is_paintable(Some(t), p, &w, Some(&user)));
        // column
        t.set_foreground_contents(0x64);
        dw.column.borrow_mut().insert((0, 0), dw_obj(0, 0, 0, false, true));
        assert!(tile_is_paintable(Some(t), p, &w, Some(&user)));
        // interaction object
        t.set_foreground_contents(0);
        t.set_contents(0x30);
        dw.io.borrow_mut().insert((0, 0), dw_obj(0, 2, 0, false, true));
        assert!(!tile_is_paintable(Some(t), p, &w, Some(&user)));
        dw.io.borrow_mut().insert((0, 0), dw_obj(0, 2, 0, true, true));
        assert!(tile_is_paintable(Some(t), p, &w, Some(&user)));
        // stairs / elevator shaft
        t.set_contents(0x65);
        assert!(!tile_is_paintable(Some(t), p, &w, Some(&user)));
        dw.stairs.borrow_mut().insert((0, 0), dw_obj(0, 0, 0, false, true));
        assert!(tile_is_paintable(Some(t), p, &w, Some(&user)));
        t.set_contents(0x67);
        assert!(!tile_is_paintable(Some(t), p, &w, Some(&user)));
        dw.shaft.borrow_mut().insert((0, 0), dw_obj(0, 0, 0, false, true));
        assert!(tile_is_paintable(Some(t), p, &w, Some(&user)));
    }

    #[test]
    fn climable() {
        let (w, t) = tile_with(|t| t.foreground_contents = 0x42);
        let p = IntPair { x: 0, y: 0 };
        assert!(tile_is_climable(Some(t), p, &w, None));
        t.set_foreground_contents(0);
        assert!(!tile_is_climable(Some(t), p, &w, None));
        t.set_contents(0x67);
        assert!(tile_is_climable(Some(t), p, &w, None));
        t.set_contents(0x07);
        assert!(tile_is_climable(Some(t), p, &w, None)); // tree trunk
    }

    #[test]
    fn type_maps() {
        let m: Vec<u32> = (0..=9).map(|n| dynamic_object_type_for_npc_type(NPCType(n)).0).collect();
        assert_eq!(m, vec![0, 13, 25, 28, 35, 36, 39, 51, 63, 0]);
        assert!(
            object_type_may_have_artifical_light(0x11)
                && object_type_may_have_artifical_light(0x10)
                && object_type_may_have_artifical_light(0x12)
        );
        let (light, plant, io) = (
            object_type_may_have_artifical_light(0x13),
            object_type_is_plant(0x13),
            object_type_is_interaction_object(0x13),
        );
        assert!(!light || plant || io);
    }

    #[test]
    fn quick_sort_sorts_and_permutes() {
        let orig = [3.5f32, -1.0, 2.0, 2.0, 9.0, 0.0, 2.5, -7.0];
        let mut n = orig;
        let mut idx = [0u32; 8];
        quick_sort(&mut n, 8, &mut idx);
        let mut sorted = orig;
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
        assert_eq!(n, sorted);
        for (pos, &i) in idx.iter().enumerate() {
            assert_eq!(orig[i as usize], n[pos]);
        }
        let mut z: [f32; 0] = [];
        let mut zi: [u32; 0] = [];
        quick_sort(&mut z, 0, &mut zi);
        let mut one = [1.0f32];
        let mut oi = [9u32];
        quick_sort(&mut one, 1, &mut oi);
        assert_eq!(oi, [0]);
        // size 31 (distanceOrderedFoodTypes)
        let mut v: Vec<f32> = (0..31).map(|i| ((i * 17) % 31) as f32).collect();
        let mut vi = vec![0u32; 31];
        quick_sort(&mut v, 31, &mut vi);
        assert!(v.windows(2).all(|w| w[0] <= w[1]));
    }

    #[test]
    fn placement_world_impl() {
        let (w, dw, pb) = world_with_dw(32);
        TileRef::new(pb, 33).update(|t| {
            t.type_index = 6;
            t.contents = 3;
        });
        let pt = PlacementWorld::tile_at_loaded(&*w, 1, 1).unwrap();
        assert_eq!((pt.type_index, pt.contents), (6, 3));
        assert!(PlacementWorld::tile_at_loaded(&*w, 0, 2000).is_none());
        assert_eq!(PlacementWorld::interaction_object_type_for_object_at_pos(&*w, 4, 4), 0);
        let mut o = dw_obj(0, 7, 0, false, true);
        o.sign_info = Some((2, 3));
        dw.io.borrow_mut().insert((4, 4), o);
        assert_eq!(PlacementWorld::interaction_object_type_for_object_at_pos(&*w, 4, 4), 7);
        let s = PlacementWorld::interaction_object_at_pos(&*w, 4, 4).unwrap();
        assert_eq!((s.interaction_object_type, s.connection_type.0, s.offset_type.0), (7, 2, 3));
        assert!(!PlacementWorld::tile_is_half_depth_at(&*w, 0, 5000));
    }
}

#[cfg(test)]
mod nil_tests {
    use super::testutil::*;
    use super::*;
    use crate::items::placement::PlacementWorld;

    #[test]
    fn missing_tiles_and_objects_answer_no() {
        let (w, dw, pb) = world_with_dw(32);
        let pos = IntPair { x: 1, y: 1 };
        let obj = new_obj();
        assert!(!tile_contains_usable_gate(None, pos, &w, Some(&obj)));
        assert!(!tile_is_climable(None, pos, &w, None));
        assert!(!tile_is_tree_trunk(None));
        assert!(!tile_is_not_unmined_block_so_can_be_removed_on_repair(None));
        assert!(!tile_is_burnable(None, None, pos));
        let t = TileRef::new(pb, 0);
        // not a shaft
        t.update(|x| x.contents = 0);
        assert!(!tile_contains_usable_elevator(Some(t), pos, &w, None, &[]));
        // an interaction object of another kind does not burn
        dw.io.borrow_mut().insert((1, 1), dw_obj(1043, 2, 0, false, true));
        let dwr = w.dynamic_world();
        assert!(!tile_is_burnable(Some(t), dwr.as_ref(), pos));
        // paintable: an object that is not paintable, one that is not removable, stairs that cannot be removed
        t.update(|x| {
            x.type_index = 2;
            x.back_wall_type_index = 0x20;
            x.foreground_contents = 0x30;
        });
        assert!(!tile_is_paintable(Some(t), pos, &w, Some(&obj)));
        dw.io.borrow_mut().insert((1, 1), dw_obj(1043, 2, 0, true, false));
        assert!(!tile_is_paintable(Some(t), pos, &w, Some(&obj)));
        t.update(|x| {
            x.foreground_contents = 0;
            x.contents = 0x65;
        });
        dw.stairs.borrow_mut().insert((1, 1), dw_obj(229, 0, 0, false, false));
        assert!(!tile_is_paintable(Some(t), pos, &w, Some(&obj)));
        // objects that are not there
        let elsewhere = IntPair { x: 9, y: 9 };
        assert!(!tile_is_burnable(Some(t), dwr.as_ref(), elsewhere));
        t.update(|x| {
            x.contents = 0x30;
            x.foreground_contents = 0x30;
        });
        assert!(!tile_is_paintable(Some(t), elsewhere, &w, Some(&obj)));
        assert!(!tile_is_half_depth(Some(t), &w, elsewhere));
        assert!(!tile_is_half_depth(Some(t), &World::alloc(32), elsewhere));
        t.update(|x| x.contents = 0);
        assert!(!tile_is_half_depth(Some(t), &w, elsewhere));
        // the placement view of a world without a dynamic world
        let bare = World::alloc(32);
        assert_eq!(PlacementWorld::interaction_object_type_for_object_at_pos(&*bare, 1, 1), 0);
        // the stand-in object
        let _ = obj.class_info();
    }
}
