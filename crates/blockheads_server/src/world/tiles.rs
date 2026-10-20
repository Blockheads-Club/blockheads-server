//! World indexing helpers: world index math, macro tile / tile lookup, block loading entry points and the
//! line-of-sight test.
//!
//!  World indexing helpers".
//! `W = world.world_width_macro`, `Wt = W * 32` tiles. The draw-block stubs and `drawBlockAtWorldPostion`
//! are not ported (empty / always NULL on the server, ).

use super::helpers::tile_is_air_water_or_snow;
use super::state::{MacroTileRef, MacroTiles, PhysicalBlockRef, TileRef, World};
use crate::fmath::cconv::f32_to_i32;
use crate::objects::base::MacroTileApi;
use crate::types::structs::{IntPair, Vector2};

#[inline]
fn wt(world: &World) -> i32 {
    world.world_width_macro.wrapping_mul(32)
}

/// `while x >= Wt { x -= Wt } while x < 0 { x += Wt }`, computed as `rem_euclid` (equivalent for
/// `Wt > 0`; for `Wt <= 0` the C loops would not terminate sensibly, so x is returned unchanged).
#[inline]
fn wrap_x(x: i32, w: i32) -> i32 {
    if w <= 0 {
        x
    } else {
        x.rem_euclid(w)
    }
}

/// `getWorldPosForWorldIndex`: `(idx % Wt, idx / Wt)`, signed C ops, no range check.
pub fn get_world_pos_for_world_index(idx: i32, world: &World) -> IntPair {
    world_pos_at_world_index(idx, world)
}

/// `worldPosAtWorldIndex`: `(idx % Wt, idx / Wt)`.
pub fn world_pos_at_world_index(idx: i32, world: &World) -> IntPair {
    let w = wt(world);
    if w == 0 {
        // C integer division by zero traps (SIGFPE); mirror with a defined value.
        return IntPair { x: 0, y: 0 };
    }
    IntPair { x: idx.wrapping_rem(w), y: idx.wrapping_div(w) }
}

/// `worldIndexAtWorldPosition`: y in `[0,0x400)` else -1; wrap x; `y*Wt + x`.
pub fn world_index_at_world_position(x: i32, y: i32, world: &World) -> i32 {
    if (0..0x400).contains(&y) {
        let w = wt(world);
        let x = wrap_x(x, w);
        y.wrapping_mul(w).wrapping_add(x)
    } else {
        -1
    }
}

/// `worldIndexAtWorldPos`: same result; the "packed pair >= 0" test is the sign of y only (R1).
pub fn world_index_at_world_pos(p: IntPair, world: &World) -> i32 {
    world_index_at_world_position(p.x, p.y, world)
}

/// `macroPosForWorldPos`: y clamped to `[0,0x3FF]` (not rejected), x wrapped, `(x/32, y/32)`.
pub fn macro_pos_for_world_pos(p: IntPair, world: &World) -> IntPair {
    let mut y = p.y;
    if y >= 0x400 {
        y = 0x3FF;
    } else if y < 0 {
        y = 0;
    }
    let x = wrap_x(p.x, wt(world));
    IntPair { x: x / 32, y: y / 32 }
}

/// Wrap a macro x into `[0,W)` by +-W loops.
#[inline]
fn wrap_macro_x(mx: i32, w: i32) -> i32 {
    wrap_x(mx, w)
}

/// `macroTileAtMacroPostion`: `my` in `[0,32)` else None; `mx` wrapped into `[0,W)`.
pub fn macro_tile_at_macro_postion(mx: i32, my: i32, macro_tiles: &MacroTiles, world: &World) -> Option<MacroTileRef> {
    if !(0..0x20).contains(&my) {
        return None;
    }
    let w = world.world_width_macro;
    let mx = wrap_macro_x(mx, w);
    let idx = my.wrapping_mul(w).wrapping_add(mx);
    if idx < 0 {
        return None;
    }
    macro_tiles.get(idx as usize)
}

/// `macroTileAtWorldPostion`: wx wrapped by Wt; `my = wy/32` (C truncation: wy in -31..-1 gives my = 0).
pub fn macro_tile_at_world_postion(wx: i32, wy: i32, macro_tiles: &MacroTiles, world: &World) -> Option<MacroTileRef> {
    let w = world.world_width_macro;
    let wx = wrap_x(wx, wt(world));
    let my = wy / 32;
    if (0..0x20).contains(&my) {
        let idx = my.wrapping_mul(w).wrapping_add(wx / 32);
        if idx < 0 {
            return None;
        }
        macro_tiles.get(idx as usize)
    } else {
        None
    }
}

/// `macroIndexAtMacroPosition`: wrap mx into `[0,W)`; `my*W + mx` (no range check on my).
pub fn macro_index_at_macro_position(mx: i32, my: i32, world: &World) -> i32 {
    let w = world.world_width_macro;
    my.wrapping_mul(w).wrapping_add(wrap_macro_x(mx, w))
}

/// `macroIndexAtWorldIndex`: `mx = (idx % Wt)/32`, `my = (idx / Wt)/32`.
pub fn macro_index_at_world_index(idx: i32, world: &World) -> i32 {
    let w = wt(world);
    if w == 0 {
        return 0;
    }
    let mx = idx.wrapping_rem(w) / 32;
    let my = idx.wrapping_div(w) / 32;
    macro_index_at_macro_position(mx, my, world)
}

/// `macroPosForMacroIndex`: `(i % W, min(i / W, 0x1f))`.
pub fn macro_pos_for_macro_index(i: i32, world: &World) -> IntPair {
    let w = world.world_width_macro;
    if w == 0 {
        return IntPair { x: 0, y: 0 };
    }
    let mut y = i.wrapping_div(w);
    if y > 0x1f {
        y = 0x1f;
    }
    IntPair { x: i.wrapping_rem(w), y }
}

/// `tileAtWorldPosition`: y in `[0,0x400)` else None; wrap x; None when the block is not loaded. Never loads.
pub fn tile_at_world_position(wx: i32, wy: i32, macro_tiles: &MacroTiles, world: &World) -> Option<TileRef> {
    if !(0..0x400).contains(&wy) {
        return None;
    }
    let w = world.world_width_macro;
    let wx = wrap_x(wx, wt(world));
    let idx = (wy / 32).wrapping_mul(w).wrapping_add(wx / 32);
    if idx < 0 {
        return None;
    }
    let mt = macro_tiles.get(idx as usize)?;
    let pb = mt.physical_block()?;
    Some(TileRef::new(pb, ((wy % 32) * 32 + wx % 32) as usize))
}

/// `tileAtWorldIndex`: `0 <= idx < W*0x8000` else None; never loads.
pub fn tile_at_world_index(idx: i32, macro_tiles: &MacroTiles, world: &World) -> Option<TileRef> {
    let w = world.world_width_macro;
    if idx < 0 || idx >= w.wrapping_mul(0x8000) {
        return None;
    }
    let wtv = wt(world);
    let mx = (idx % wtv) / 32;
    let my = (idx / wtv) / 32;
    let local = ((idx / wtv) % 32) * 32 + idx % 32;
    let mt = macro_tiles.get((my.wrapping_mul(w).wrapping_add(mx)) as usize)?;
    let pb = mt.physical_block()?;
    Some(TileRef::new(pb, local as usize))
}

/// Loads the block of `mt` through the world_ops seam (`load...LoadSurroundingBlocks:NO createIfNotCreated:YES`),
/// ignoring the result.
fn load_for_tile(mt: &MacroTileRef, mx: i32, my: i32, world: &World) {
    let _ = world.run_block_loader(mt, mx, my, false, true);
}

/// `tileAtWorldIndexLoaded`: like `tile_at_world_index` but loads a missing block (`createIfNotCreated` YES).
pub fn tile_at_world_index_loaded(idx: i32, world: &World) -> Option<TileRef> {
    let w = world.world_width_macro;
    if idx < 0 || idx >= w.wrapping_mul(0x8000) {
        return None;
    }
    let wtv = wt(world);
    let mx = (idx % wtv) / 32;
    let my = (idx / wtv) / 32;
    let local = ((idx / wtv) % 32) * 32 + idx % 32;
    let macro_tiles = world.macro_tiles();
    let slot = macro_tiles.get((my.wrapping_mul(w).wrapping_add(mx)) as usize);
    let mut pb = slot.as_ref().and_then(|m| m.physical_block());
    if pb.is_none() {
        if let Some(mt) = macro_tile_at_macro_postion(mx, my, &macro_tiles, world) {
            load_for_tile(&mt, mx, my, world);
        }
        // re-read after the load (R3)
        pb = slot.as_ref().and_then(|m| m.physical_block());
    }
    pb.map(|pb| TileRef::new(pb, local as usize))
}

/// `tileAtWorldPositionLoaded`: y in `[0,0x400)`; wrap x; `load_block_at_macro_pos((x/32, y/32))`. Returns a tile
/// for any non-NULL block, even when `valid` is false.
pub fn tile_at_world_position_loaded(wx: i32, wy: i32, world: &World) -> Option<TileRef> {
    if !(0..0x400).contains(&wy) {
        return None;
    }
    let x = wrap_x(wx, wt(world));
    let pb = load_block_at_macro_pos(IntPair { x: x / 32, y: wy / 32 }, world)?;
    Some(TileRef::new(pb, ((wy % 32) * 32 + x % 32) as usize))
}

/// `loadBlockAtMacroPos`: loads when the block is missing or `dirty != 0`, returns `mt.physicalBlock`.
pub fn load_block_at_macro_pos(pos: IntPair, world: &World) -> Option<PhysicalBlockRef> {
    let macro_tiles = world.macro_tiles();
    let mt = macro_tile_at_macro_postion(pos.x, pos.y, &macro_tiles, world)?;
    if mt.physical_block().is_none() || mt.dirty.get() != 0 {
        load_for_tile(&mt, pos.x, pos.y, world);
    }
    mt.physical_block()
}

/// `macroTileHasDynamicObjectsThatNeedPhysicalBlock`: any object in the tile's array with
/// `requiresPhysicalBlock`.
pub fn macro_tile_has_dynamic_objects_that_need_physical_block(mt: Option<&MacroTileRef>) -> bool {
    let Some(mt) = mt else { return false };
    let objs: Vec<_> = match mt.dynamic_objects().borrow().as_ref() {
        Some(v) if !v.is_empty() => v.clone(),
        _ => return false,
    };
    objs.iter().any(|o| o.requires_physical_block())
}

/// `clearLineOfSightBetweenTiles(start, end, world)`. Each iteration loads its tile (rand/IO side
/// effects of the loader are part of the contract); `None` tiles count as blocked.
pub fn clear_line_of_sight_between_tiles(start: IntPair, end: IntPair, world: &World) -> bool {
    let air_at = |x: i32, y: i32| tile_is_air_water_or_snow(tile_at_world_position_loaded(x, y, world));
    if start == end {
        return air_at(start.x, start.y);
    }
    if start.x != end.x {
        let (lo, hi) = if start.x <= end.x { (start, end) } else { (end, start) };
        let d = Vector2::new((hi.x - lo.x) as f32, (hi.y - lo.y) as f32);
        let n = d.normal();
        let k: f64 = 1.0f64 / (n.x as f64);
        let mut x = lo.x;
        while x < hi.x {
            let y = f32_to_i32((lo.y as f32) + ((x - lo.x) as f32) * ((k * (n.y as f64)) as f32));
            if !air_at(x, y) {
                return false;
            }
            x += 1;
        }
    }
    if start.y != end.y {
        let (lo, hi) = if start.y <= end.y { (start, end) } else { (end, start) };
        let d = Vector2::new((hi.x - lo.x) as f32, (hi.y - lo.y) as f32);
        let n = d.normal();
        let k: f64 = 1.0f64 / (n.y as f64);
        let mut yy = lo.y;
        while yy < hi.y {
            let x = f32_to_i32((lo.x as f32) + ((yy - lo.y) as f32) * ((k * (n.x as f64)) as f32));
            if !air_at(x, yy) {
                return false;
            }
            yy += 1;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::objects::handle::ObjRef;
    use crate::world::state::{PhysicalBlock, WorldOpsHooks};
    use std::cell::Cell;
    use std::rc::Rc;

    fn ip(x: i32, y: i32) -> IntPair {
        IntPair { x, y }
    }

    #[test]
    fn index_math_for_widths() {
        for wm in [32, 128, 8192] {
            let w = World::alloc(wm);
            let wtv = wm * 32;
            assert_eq!(world_index_at_world_position(0, -1, &w), -1);
            assert_eq!(world_index_at_world_position(0, 1024, &w), -1);
            assert_eq!(world_index_at_world_position(0, 0, &w), 0);
            assert_eq!(world_index_at_world_position(-1, 0, &w), wtv - 1);
            assert_eq!(world_index_at_world_position(wtv, 0, &w), 0);
            assert_eq!(world_index_at_world_position(wtv + 5, 1, &w), wtv + 5);
            assert_eq!(world_index_at_world_position(-wtv - 3, 1023, &w), 1023 * wtv + wtv - 3);
            assert_eq!(world_index_at_world_pos(ip(-1, 7), &w), 7 * wtv + wtv - 1);
            // inverse for scattered indices
            let mut s = 12345u32;
            for _ in 0..20 {
                s = s.wrapping_mul(1664525).wrapping_add(1013904223);
                let idx = (s % (wtv as u32 * 1024)) as i32;
                let p = world_pos_at_world_index(idx, &w);
                assert_eq!(world_index_at_world_position(p.x, p.y, &w), idx);
                assert_eq!(get_world_pos_for_world_index(idx, &w), p);
            }
            assert_eq!(macro_pos_for_world_pos(ip(10, -5), &w), ip(0, 0));
            assert_eq!(macro_pos_for_world_pos(ip(10, 5000), &w), ip(0, 31));
            assert_eq!(macro_pos_for_world_pos(ip(-1, 0), &w), ip(wm - 1, 0));
            assert_eq!(macro_pos_for_macro_index(wm * 40 + 3, &w), ip(3, 31));
            assert_eq!(macro_index_at_macro_position(-1, 2, &w), 2 * wm + wm - 1);
            assert_eq!(macro_index_at_world_index(3 * wtv + 33, &w), 0 * wm + 1);
        }
    }

    #[test]
    fn macro_tile_lookup() {
        let w = World::alloc(32);
        let mt = w.macro_tiles();
        let last_of_row0 = macro_tile_at_macro_postion(-1, 0, &mt, &w).unwrap();
        assert!(Rc::ptr_eq(&last_of_row0, &mt.get(31).unwrap()));
        assert!(macro_tile_at_macro_postion(0, 32, &mt, &w).is_none());
        assert!(macro_tile_at_macro_postion(0, -1, &mt, &w).is_none());
        // C truncation: wy in -31..-1 gives row 0
        let a = macro_tile_at_world_postion(0, -5, &mt, &w).unwrap();
        assert!(Rc::ptr_eq(&a, &mt.get(0).unwrap()));
        assert!(macro_tile_at_world_postion(0, -32, &mt, &w).is_none());
        assert!(macro_tile_at_world_postion(0, 1024, &mt, &w).is_none());
        let b = macro_tile_at_world_postion(-1, 40, &mt, &w).unwrap();
        assert!(Rc::ptr_eq(&b, &mt.get(32 + 31).unwrap()));
    }

    struct Loader {
        calls: Cell<u32>,
        valid: bool,
    }
    impl WorldOpsHooks for Loader {
        fn load_physical_block_for_macro_tile_at_x_y_load_surrounding_blocks_create_if_not_created(
            &self,
            world: &World,
            mt: &MacroTileRef,
            mx: i32,
            my: i32,
            surrounding: bool,
            create: bool,
        ) -> bool {
            assert!(!surrounding && create);
            self.calls.set(self.calls.get() + 1);
            let pb = world.take_physical_block();
            pb.x_pos.set(mx);
            pb.y_pos.set(my);
            pb.valid.set(self.valid);
            mt.dirty.set(0);
            *mt.physical_block.borrow_mut() = Some(pb);
            true
        }
        fn load_dynamic_objects_if_not_already_loaded_for_macro_tile_include_surface_blocks(
            &self,
            _w: &World,
            _mt: &MacroTileRef,
            _i: bool,
        ) {
        }
        fn tile_is_protected_at_pos_against_blockhead(&self, _w: &World, _p: IntPair, _b: &ObjRef) -> bool {
            true
        }
    }

    #[test]
    fn loaded_lookups_load_and_never_load_variants_do_not() {
        let w = World::alloc(32);
        let l = Rc::new(Loader { calls: Cell::new(0), valid: false });
        w.set_ops_hooks(Some(l.clone()));
        let mt = w.macro_tiles();
        assert!(tile_at_world_position(5, 5, &mt, &w).is_none());
        assert!(tile_at_world_index(5 * 1024 + 5, &mt, &w).is_none());
        assert_eq!(l.calls.get(), 0);
        // loads; returns a tile even though valid == false
        let t = tile_at_world_position_loaded(5, 33, &w).unwrap();
        assert_eq!(l.calls.get(), 1);
        assert_eq!(t.local_index(), 32 + 5);
        assert_eq!((t.physical_block().x_pos.get(), t.physical_block().y_pos.get()), (0, 1));
        // second access: cached
        tile_at_world_position_loaded(6, 33, &w).unwrap();
        assert_eq!(l.calls.get(), 1);
        // non-loading variants now find it
        let a = tile_at_world_position(5, 33, &mt, &w).unwrap();
        assert!(a == t);
        let b = tile_at_world_index(33 * 1024 + 5, &mt, &w).unwrap();
        assert!(b == t);
        // dirty reload
        mt.get(32).unwrap().dirty.set(1);
        load_block_at_macro_pos(ip(0, 1), &w).unwrap();
        assert_eq!(l.calls.get(), 2);
        // loaded index variant
        let c = tile_at_world_index_loaded(40 * 1024 + 70, &w).unwrap();
        assert_eq!(l.calls.get(), 3);
        assert_eq!((c.physical_block().x_pos.get(), c.physical_block().y_pos.get()), (2, 1));
        assert_eq!(c.local_index(), (40 % 32) * 32 + 70 % 32);
        // out of range
        assert!(tile_at_world_index_loaded(-1, &w).is_none());
        assert!(tile_at_world_index_loaded(32 * 0x8000, &w).is_none());
        assert!(tile_at_world_position_loaded(0, 1024, &w).is_none());
        // writes through the handle are seen through every path (the dirty reload installed a new block)
        let t = tile_at_world_position_loaded(5, 33, &w).unwrap();
        t.set_type_index(77);
        assert_eq!(tile_at_world_position(5, 33, &mt, &w).unwrap().type_index(), 77);
    }

    #[test]
    fn no_hooks_means_no_tile() {
        let w = World::alloc(32);
        assert!(tile_at_world_position_loaded(1, 1, &w).is_none());
        let _ = PhysicalBlock::new_fresh();
    }

    #[test]
    fn line_of_sight() {
        let w = World::alloc(32);
        let l = Rc::new(Loader { calls: Cell::new(0), valid: true });
        w.set_ops_hooks(Some(l));
        // fill macro (0,0) with air
        for y in 0..32 {
            for x in 0..32 {
                tile_at_world_position_loaded(x, y, &w).unwrap().set_type_index(2);
            }
        }
        assert!(clear_line_of_sight_between_tiles(ip(1, 1), ip(1, 1), &w));
        assert!(clear_line_of_sight_between_tiles(ip(1, 1), ip(10, 1), &w));
        assert!(clear_line_of_sight_between_tiles(ip(1, 1), ip(10, 8), &w));
        tile_at_world_position_loaded(5, 1, &w).unwrap().set_type_index(6);
        assert!(!clear_line_of_sight_between_tiles(ip(1, 1), ip(10, 1), &w));
        assert!(!clear_line_of_sight_between_tiles(ip(10, 1), ip(1, 1), &w));
        // the end tile itself is not tested
        tile_at_world_position_loaded(10, 1, &w).unwrap().set_type_index(6);
        tile_at_world_position_loaded(5, 1, &w).unwrap().set_type_index(2);
        assert!(clear_line_of_sight_between_tiles(ip(1, 1), ip(10, 1), &w));
        // start tile equal to end: tests that tile
        assert!(!clear_line_of_sight_between_tiles(ip(10, 1), ip(10, 1), &w));
        // vertical
        tile_at_world_position_loaded(3, 4, &w).unwrap().set_type_index(6);
        assert!(!clear_line_of_sight_between_tiles(ip(3, 1), ip(3, 9), &w));
        // unloaded (None) tile blocks: row y=40 in macro row 1 gets loaded by the hook, but y>1023 is None
        assert!(!clear_line_of_sight_between_tiles(ip(0, 1020), ip(0, 1030), &w));
    }

    #[test]
    fn dynamic_objects_need_block() {
        let w = World::alloc(32);
        let mt = w.macro_tiles().get(0).unwrap();
        assert!(!macro_tile_has_dynamic_objects_that_need_physical_block(Some(&mt)));
        assert!(!macro_tile_has_dynamic_objects_that_need_physical_block(None));
        *mt.dynamic_objects.borrow_mut() = Some(vec![crate::world::helpers::testutil::new_obj()]);
        assert!(!macro_tile_has_dynamic_objects_that_need_physical_block(Some(&mt)));
    }
}
