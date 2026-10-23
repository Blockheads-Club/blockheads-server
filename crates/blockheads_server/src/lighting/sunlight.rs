//! `WorldHelper`: the server-authoritative sunlight simulation (flood fill and removal), the per-viewer light /
//! explore update, and the macro tile decommission test. All class methods become associated functions.
//!
//! `clientMacroTileRequestCanBeCancelledAtMacroPos:world:` is CLIENT-ONLY and not ported);

use std::collections::VecDeque;
use std::rc::Rc;

use crate::fmath::cconv::f32_to_i32;
use crate::fmath::libm::sqrtf;
use crate::foundation::clock::reference_time;
use crate::foundation::indexset::IndexSet;
use crate::lighting::artificial_light::services;
use crate::objects::handle::ObjRef;
use crate::types::structs::IntPair;
use crate::world::climate::{current_temperature_for_tile_at_world_pos, season_for_world_x};
use crate::world::helpers::{
    back_wall_is_mutable, tile_is_air_or_snow, tile_is_semi_transparent_solid_block, tile_is_solid,
    tile_requires_glow_block,
};
use crate::world::state::{MacroTileRef, PhysicalBlockRef, TileRef, World};
use crate::world::tiles::{
    get_world_pos_for_world_index, macro_pos_for_world_pos, macro_tile_has_dynamic_objects_that_need_physical_block,
    tile_at_world_position, tile_at_world_position_loaded, world_index_at_world_position,
};

/// `-[World mapVisible]` is `[uiManager mapDisplayed]` and `uiManager` is nil on the server, so it is always NO
/// : the Workbench-portal scan is omitted.
pub const MAP_VISIBLE: bool = false;

/// `ITEM_FREEZE_WATER`
const ITEM_FREEZE_WATER: i32 = 0x422;

/// `WorldHelper` (stateless; no ivars).
pub struct WorldHelper;

/// Gem promise -> real gem and treasure chest / troll promise handling, shared by `recursivelyUpdateSunLight`
/// and `ArtificialLight addToTiles`. `pos` is the tile position.
fn promise_or_glow(tile: TileRef, pos: IntPair) {
    let sv = services();
    if !tile_requires_glow_block(tile) {
        let gem = match tile.foreground_contents() {
            0x34 => Some((0x33u8, 0x4b)),
            0x36 => Some((0x35, 0x4c)),
            0x38 => Some((0x37, 0x56)),
            0x3a => Some((0x39, 0x57)),
            0x3c => Some((0x3b, 0x58)),
            _ => None,
        };
        if let Some((real, item)) = gem {
            tile.set_foreground_contents(real);
            sv.add_torch_at_pos_of_type_data_a_data_b_save_dict_placed_by_client(pos, item, 0, 0, None, None);
        } else {
            let c = tile.contents();
            if matches!(c, 0x5e | 0x91 | 0x90) {
                // c1 and c2 are both sampled before clearing
                tile.set_contents(0);
                if tile.type_index() == 2 {
                    sv.create_treasure_chest_or_troll_at_tile_at_pos_load_troll_load_treasure(
                        tile,
                        pos,
                        c != 0x91,
                        c != 0x90,
                    );
                }
            }
        }
    } else {
        sv.load_glow_block_if_needed_at_pos_tile(pos, tile);
    }
}

fn enqueue(open_list: &mut VecDeque<i32>, open_indices: &mut IndexSet, j: i32) {
    if j >= 0 && !open_indices.contains_index(j as u64) {
        open_list.push_back(j);
        open_indices.add_index(j as u64);
    }
}

impl WorldHelper {
    /// `+[WorldHelper recursivelyUpdateSunLightWithList:openIndices:world:]`: one BFS step.
    pub fn recursively_update_sun_light_with_list_open_indices_world(
        open_list: &mut VecDeque<i32>,
        open_indices: &mut IndexSet,
        world: &Rc<World>,
    ) {
        let Some(idx) = open_list.pop_front() else { return };
        if idx >= 0 {
            open_indices.remove_index(idx as u64);
        }
        // isClient: always NO on the server
        let wp = get_world_pos_for_world_index(idx, world);
        let (wx, wy) = (wp.x, wp.y);
        let macro_tiles = world.macro_tiles();
        let Some(tile) = tile_at_world_position(wx, wy, &macro_tiles, world) else { return };
        if tile.explored_fraction() == 0 {
            return;
        }
        let sky_tile = tile_is_air_or_snow(Some(tile)) && tile.back_wall_type_index() == 2;
        let bg_window = tile.background_contents() == 0x45 || tile.background_contents() == 0x5f;
        if !sky_tile && !bg_window {
            // ---- propagation branch ----
            let mut inh = tile.sun_light() as i32;
            // above, below, right, left, aboveRight, aboveLeft
            const NB: [(i32, i32); 6] = [(0, 1), (0, -1), (1, 0), (-1, 0), (1, 1), (-1, 1)];
            let mut contrib = [0i32; 6];
            for (n, (dx, dy)) in NB.iter().enumerate() {
                let nt = tile_at_world_position_loaded(wx.wrapping_add(*dx), wy.wrapping_add(*dy), world);
                let mut c = 0i32;
                if let Some(nb) = nt {
                    let ntile = nb.get();
                    if !tile_is_air_or_snow(nt) || (ntile.back_wall_type_index != 2 && ntile.back_wall_type_index != 3)
                    {
                        let sub = if n == 0 {
                            // rule A (above only)
                            let mut sub = 2;
                            if ntile.sun_light < 0xf0 {
                                sub = 8;
                            }
                            if ntile.type_index == 3 || tile_is_semi_transparent_solid_block(nt) {
                                sub = 0x10;
                            } else if !tile_is_air_or_snow(nt) {
                                sub = 0x41;
                            }
                            sub
                        } else {
                            // rule B
                            let mut sub = 4;
                            if ntile.sun_light < 0xf0 {
                                sub = 0x10;
                            }
                            if !tile_is_air_or_snow(nt)
                                && ntile.type_index != 3
                                && !tile_is_semi_transparent_solid_block(nt)
                            {
                                sub = 0x41;
                            }
                            sub
                        };
                        c = (ntile.sun_light as i32) - sub;
                        if c < 0 {
                            c = 0;
                        }
                    } else {
                        c = if n == 0 { 0xff } else { 0xe0 };
                    }
                }
                contrib[n] = c;
                if inh < c {
                    inh = c;
                }
            }
            if (tile.sun_light() as i32) < inh {
                tile.set_sun_light(inh as u8);
                let sv = services();
                sv.save_sunlight_changed_at_pos(world, IntPair { x: wx, y: wy });
                // !isClient
                promise_or_glow(tile, IntPair { x: wx, y: wy });
                // recalculateDrawBlockLightingForTile: empty in the server binary
                for (n, (dx, dy)) in NB.iter().enumerate() {
                    if contrib[n] < inh {
                        let j = world_index_at_world_position(wx.wrapping_add(*dx), wy.wrapping_add(*dy), world);
                        enqueue(open_list, open_indices, j);
                    }
                }
            }
        } else {
            // ---- sky / window branch ----
            tile.set_sun_light(0xff);
            if bg_window {
                tile.set_sun_light(0xef);
            }
            for (dx, dy) in [(0, 1), (0, -1), (1, 0), (-1, 0)] {
                let nt = tile_at_world_position_loaded(wx.wrapping_add(dx), wy.wrapping_add(dy), world);
                if let Some(nb) = nt {
                    let n = nb.get();
                    if n.sun_light != 0xff && n.background_contents != 0x45 && n.background_contents != 0x5f {
                        let j = world_index_at_world_position(wx.wrapping_add(dx), wy.wrapping_add(dy), world);
                        enqueue(open_list, open_indices, j);
                    }
                }
            }
        }
    }

    /// `+[WorldHelper updateSunLightForTile:atPos:world:]`. `tile` is unused by the original.
    pub fn update_sun_light_for_tile_at_pos_world(_tile: Option<TileRef>, pos: IntPair, world: &Rc<World>) {
        let mut open_list: VecDeque<i32> = VecDeque::new();
        let mut open_indices = IndexSet::new();
        for p in [
            pos,
            IntPair { x: pos.x, y: pos.y + 1 },
            IntPair { x: pos.x, y: pos.y - 1 },
            IntPair { x: pos.x + 1, y: pos.y },
            IntPair { x: pos.x - 1, y: pos.y },
        ] {
            let j = world_index_at_world_position(p.x, p.y, world);
            if j >= 0 {
                open_list.push_back(j);
                open_indices.add_index(j as u64);
            }
        }
        while !open_list.is_empty() {
            Self::recursively_update_sun_light_with_list_open_indices_world(&mut open_list, &mut open_indices, world);
        }
    }

    /// `+[WorldHelper recursivelyRemoveAllSunLightWithList:openIndices:lightWasRemovedList:removeIndices:world:minx:maxX:]`
    /// : one BFS step of removal. `min_x`/`max_x` are compared against the WRAPPED world x.
    #[allow(clippy::too_many_arguments)]
    pub fn recursively_remove_all_sun_light_with_list_open_indices_light_was_removed_list_remove_indices_world_minx_max_x(
        open_list: &mut VecDeque<i32>,
        open_indices: &mut IndexSet,
        light_was_removed_list: &mut VecDeque<i32>,
        remove_indices: &mut IndexSet,
        world: &Rc<World>,
        min_x: i32,
        max_x: i32,
    ) {
        let Some(idx) = open_list.pop_front() else { return };
        if idx >= 0 {
            open_indices.remove_index(idx as u64);
        }
        let wp = get_world_pos_for_world_index(idx, world);
        let (wx, wy) = (wp.x, wp.y);
        let Some(tile) = tile_at_world_position_loaded(wx, wy, world) else { return };
        if tile_is_air_or_snow(Some(tile)) && (tile.back_wall_type_index() == 2 || tile.back_wall_type_index() == 3) {
            return;
        }
        let old = tile.sun_light();
        if old == 0 {
            return;
        }
        tile.set_sun_light(0);
        // recalculateDrawBlockLightingForTile: empty in the server binary
        if idx >= 0 {
            light_was_removed_list.push_back(idx);
            remove_indices.add_index(idx as u64);
        } else {
            light_was_removed_list.push_back(idx);
        }
        // above, below, right [wx+1 < maxX], left [minX <= wx-1]
        for (dx, dy) in [(0, 1), (0, -1), (1, 0), (-1, 0)] {
            if dx == 1 && !(wx.wrapping_add(1) < max_x) {
                continue;
            }
            if dx == -1 && !(min_x <= wx.wrapping_sub(1)) {
                continue;
            }
            let nt = tile_at_world_position_loaded(wx.wrapping_add(dx), wy.wrapping_add(dy), world);
            if let Some(nb) = nt {
                let n = nb.get();
                if n.sun_light != 0
                    && n.sun_light <= old
                    && (!tile_is_air_or_snow(nt) || (n.back_wall_type_index != 2 && n.back_wall_type_index != 3))
                {
                    let j = world_index_at_world_position(wx.wrapping_add(dx), wy.wrapping_add(dy), world);
                    enqueue(open_list, open_indices, j);
                }
            }
        }
    }

    /// `+[WorldHelper updateSunLightRemovedForTile:atPos:world:]`. `tile` is unused; callers
    /// (TradePortal, Workbench) pass `None`.
    pub fn update_sun_light_removed_for_tile_at_pos_world(_tile: Option<TileRef>, pos: IntPair, world: &Rc<World>) {
        let mut open_list: VecDeque<i32> = VecDeque::new();
        let mut light_was_removed_list: VecDeque<i32> = VecDeque::new();
        let mut open_indices = IndexSet::new();
        let mut remove_indices = IndexSet::new();
        let idx = world_index_at_world_position(pos.x, pos.y, world);
        if idx >= 0 {
            open_list.push_back(idx); // NB: openIndices does NOT get idx
            while !open_list.is_empty() {
                Self::recursively_remove_all_sun_light_with_list_open_indices_light_was_removed_list_remove_indices_world_minx_max_x(
                    &mut open_list,
                    &mut open_indices,
                    &mut light_was_removed_list,
                    &mut remove_indices,
                    world,
                    pos.x.wrapping_sub(0x20),
                    pos.x.wrapping_add(0x20),
                );
            }
            if !remove_indices.contains_index(idx as u64) {
                light_was_removed_list.push_back(idx);
                remove_indices.add_index(idx as u64);
            }
            while !light_was_removed_list.is_empty() {
                Self::recursively_update_sun_light_with_list_open_indices_world(
                    &mut light_was_removed_list,
                    &mut remove_indices,
                    world,
                );
            }
        }
    }

    /// `+[WorldHelper recalculateLightingForPhysicalBlockIfNeeded:world:clientLightBlockIndex:forBlockhead:]`
    /// `blockhead` may be `None` (start portal position, radius 4).
    pub fn recalculate_lighting_for_physical_block_if_needed_world_client_light_block_index_for_blockhead(
        pb: Option<&PhysicalBlockRef>,
        world: &Rc<World>,
        client_light_block_index: i32,
        blockhead: Option<&ObjRef>,
    ) {
        let Some(pb) = pb else { return };
        let pb: PhysicalBlockRef = *pb;
        // [[world dynamicWorld] isClient] is NO on the server
        if let Some(b) = blockhead {
            if b.as_blockhead().map_or(false, |x| x.is_client_blockhead_being_controlled_by_server()) {
                return;
            }
        }
        let sv = services();
        let bx = pb.x_pos.get().wrapping_mul(32);
        let by = pb.y_pos.get().wrapping_mul(32);
        // clientLightBlocks[clbIndex]: no bounds check in the original; out of range is treated as NULL here.
        let slot = if client_light_block_index != -1 {
            usize::try_from(client_light_block_index).ok().filter(|i| *i < 32)
        } else {
            None
        };
        let mut has_clb = false;
        if let Some(s) = slot {
            has_clb = pb.client_light_blocks.borrow()[s].is_some();
            if !has_clb {
                sv.load_light_block_for_client_light_block_index_into_physical_block(
                    world,
                    client_light_block_index,
                    pb,
                );
                has_clb = pb.client_light_blocks.borrow()[s].is_some();
            }
        }
        let mut explore_change = false;
        let mut light_change = false;
        let portal_pos = world.start_portal_pos();
        let (mut pos, radius) = match blockhead {
            None => (portal_pos, 4),
            Some(b) => (b.pos(), sv.blockhead_view_radius(b)),
        };
        let k = if 0x16 - radius > 0 { 0x16 - radius } else { 0 };
        let w = world.world_width_macro();
        let half = w.wrapping_mul(32) / 2;
        let neg_half = (0i32.wrapping_sub(w)).wrapping_mul(32) / 2;
        if pos.x.wrapping_sub(bx) < neg_half {
            pos.x = pos.x.wrapping_add(w.wrapping_mul(32));
        } else if pos.x.wrapping_sub(bx) > half {
            pos.x = pos.x.wrapping_sub(w.wrapping_mul(32));
        }
        if pos.x < bx + 32 + radius && bx - radius < pos.x && pos.y < by + 32 + radius && by - radius < pos.y {
            for y in 0..32i32 {
                for x in 0..32i32 {
                    let i = (y * 32 + x) as usize;
                    let tx = bx + x;
                    let ty = by + y;
                    let tile = pb.tile_ref(i);
                    let old_light: i32 = if has_clb {
                        pb.client_light_blocks.borrow()[slot.unwrap()].as_ref().unwrap()[i] as i32
                    } else {
                        tile.light() as i32
                    };
                    let mut light = old_light;
                    let mut xl = pos.x.wrapping_sub(tx);
                    if xl < 0 {
                        xl += 1;
                    }
                    if xl < half {
                        if xl <= neg_half {
                            xl = xl.wrapping_add(w.wrapping_mul(32));
                        }
                    } else {
                        xl = xl.wrapping_sub(w.wrapping_mul(32));
                    }
                    let mut yl = pos.y.wrapping_sub(ty);
                    if yl < 0 {
                        yl += 1;
                    }
                    let lsq = xl.wrapping_mul(xl).wrapping_add(yl.wrapping_mul(yl));
                    if lsq < 0x1e4 {
                        let f = (sqrtf(lsq as f32) + (k as f32)) / 22.0f32;
                        let nl = f32_to_i32(255.0f32 * (4.0f32 - 4.0f32 * f));
                        if old_light < nl {
                            light = if nl < 255 { nl } else { 255 };
                        }
                        if tile.explored_fraction() == 0 {
                            if light > 0 {
                                tile.update(|t| t.explored_fraction = light as u8);
                                explore_change = true;
                                if !back_wall_is_mutable(Some(tile))
                                    || tile_is_solid(Some(tile))
                                    || tile.contents() != 0
                                    || tile.type_index() == 3
                                {
                                    Self::update_sun_light_for_tile_at_pos_world(
                                        Some(tile),
                                        IntPair { x: tx, y: ty },
                                        world,
                                    );
                                }
                                if tile.type_index() == 3 {
                                    let pos2 = IntPair { x: tx, y: ty };
                                    let weather = sv.get_weather_fraction_for_pos(world, pos2);
                                    let t = world.world_time();
                                    let day_night =
                                        sv.get_day_night_fraction_for_x_at_world_time(world, pos2.x as f32, t);
                                    let season = season_for_world_x(pos2.x, world.world_time(), Some(world));
                                    let temp = current_temperature_for_tile_at_world_pos(
                                        &tile.get(),
                                        pos2,
                                        season,
                                        day_night,
                                        weather,
                                        Some(world),
                                    );
                                    if temp < 0.0f32 {
                                        sv.fill_tile_at_pos_with_type(world, tile, pos2, ITEM_FREEZE_WATER);
                                    }
                                }
                            }
                        } else if (tile.explored_fraction() as i32) < light {
                            tile.update(|t| t.explored_fraction = light as u8);
                            explore_change = true;
                        }
                    }
                    if light != old_light {
                        if has_clb {
                            pb.client_light_blocks.borrow_mut()[slot.unwrap()].as_mut().unwrap()[i] = light as u8;
                        } else {
                            tile.update(|t| t.light = light as u8);
                        }
                        light_change = true;
                    }
                }
            }
        }
        let mp = IntPair { x: pb.x_pos.get(), y: pb.y_pos.get() };
        if explore_change {
            sv.light_changed_at_macro_pos_send_reliably_send_at_all(mp, false, false);
        }
        if light_change {
            sv.explore_light_changed_at_macro_pos_client_light_block_index(mp, client_light_block_index);
        }
    }

    /// `+[WorldHelper checkIfMacroTileCanBeDecommissioned:world:minAge:blockToSavePhyscialBlock:]`.
    /// Returns `true` iff it called `World::decommision_block` itself.
    pub fn check_if_macro_tile_can_be_decommissioned_world_min_age_block_to_save_physcial_block(
        macro_tile: &MacroTileRef,
        world: &Rc<World>,
        min_age: f64,
        block_to_save_physcial_block: bool,
    ) -> bool {
        let sv = services();
        let mut needs_pb = false;
        let mut has_save = false;
        let pb = macro_tile.physical_block();
        let wm = world.world_width_macro();
        if let Some(pb) = pb {
            let mx = pb.x_pos.get();
            let my = pb.y_pos.get();
            needs_pb = macro_tile_has_dynamic_objects_that_need_physical_block(Some(macro_tile));
            // drawBlock == NULL on the server
            let mut keep = needs_pb;
            if sv.peers_interested_in_macro_index_count(world, my.wrapping_mul(wm).wrapping_add(mx)) != 0 {
                keep = true;
            }
            has_save = sv.has_dynamic_objects_to_save_in_macro_pos(IntPair { x: mx, y: my });
            if has_save {
                keep = true;
            }
            if keep {
                pb.last_required_time.set(reference_time());
            }
        }
        if let Some(pb) = pb {
            let now = reference_time();
            if (min_age < now - pb.last_required_time.get() || min_age < 1.5) && !needs_pb {
                let mx = pb.x_pos.get();
                let my = pb.y_pos.get();
                let pm = macro_pos_for_world_pos(world.start_portal_pos(), world);
                if pm.x - 1 <= mx && mx <= pm.x + 1 && pm.y - 1 <= my && my <= pm.y + 1 {
                    return false; // never unload the 3x3 around the start portal
                }
                if !has_save {
                    let idx = wm.wrapping_mul(my).wrapping_add(mx);
                    if min_age > 1.5 && sv.peers_interested_in_macro_index_count(world, idx) != 0 {
                        return false;
                    }
                    // `minAge > 1.5 && [world mapVisible]` is never true on the server (R11, MAP_VISIBLE)
                    debug_assert!(!MAP_VISIBLE);
                    let mut near = false;
                    for bh in sv.blockheads() {
                        let p = bh.pos();
                        let bm = IntPair { x: p.x / 32, y: p.y / 32 };
                        let mut dx = bm.x.wrapping_sub(mx);
                        if dx < wm / 2 {
                            if dx < (0i32.wrapping_sub(wm)) / 2 {
                                dx = dx.wrapping_add(wm);
                            }
                        } else {
                            dx = dx.wrapping_sub(wm);
                        }
                        if -2 < dx && dx < 2 && (bm.y == my || bm.y == my - 1 || bm.y == my + 1) {
                            near = true;
                            break;
                        }
                    }
                    if !near {
                        sv.decommision_block_block_to_save_physcial_block(
                            world,
                            macro_tile,
                            block_to_save_physcial_block,
                        );
                        return true;
                    }
                }
            }
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lighting::artificial_light::testkit::*;

    /// macro wide world, blocks (0..4)x(0..2) loaded; everything solid (type 1) except what a test carves.
    fn solid_world() -> Rc<World> {
        let blocks: Vec<(i32, i32)> = (0..4).flat_map(|x| (0..2).map(move |y| (x, y))).collect();
        let w = world_with_blocks(4, &blocks);
        for x in 0..128 {
            for y in 0..64 {
                tile(&w, x, y).update(|t| {
                    t.type_index = 1;
                    t.back_wall_type_index = 1;
                    t.explored_fraction = 255;
                });
            }
        }
        w
    }

    fn sky(w: &Rc<World>, x: i32, y: i32) {
        tile(w, x, y).update(|t| {
            t.type_index = 2;
            t.back_wall_type_index = 2;
        });
    }

    #[test]
    fn sky_floods_shaft_and_cave_with_expected_values() {
        let rec = install_rec();
        let w = solid_world();
        // vertical shaft x=40, y=63 down to y=50, open sky at y=63..: surface tile y=63 is sky
        for y in 50..64 {
            tile(&w, 40, y).update(|t| {
                t.type_index = 2;
                t.back_wall_type_index = 1; // cave air, no sky wall
            });
        }
        sky(&w, 40, 63);
        WorldHelper::update_sun_light_for_tile_at_pos_world(None, IntPair { x: 40, y: 63 }, &w);
        assert_eq!(tile(&w, 40, 63).get().sun_light, 0xff);
        // y=62: above is sky (c = 0xff) -> inh = 0xff? contribution rule: sky above gives 0xff for 'above'
        assert_eq!(tile(&w, 40, 62).get().sun_light, 0xff);
        // y=61: above tile is cave air with sun 0xff, rule A: sub = 2 -> 0xfd
        assert_eq!(tile(&w, 40, 61).get().sun_light, 0xfd);
        assert_eq!(tile(&w, 40, 60).get().sun_light, 0xfb);
        // the penalty depends on the SENDING neighbour's tile: (41,61) gets 0xff (aboveLeft (40,62)) - 4
        assert_eq!(tile(&w, 41, 61).get().sun_light, 0xfb);
        assert!(rec.take().iter().any(|s| s.starts_with("saveSun(40,61)")));
    }

    #[test]
    fn removal_and_relight() {
        install_rec();
        let w = solid_world();
        for y in 50..64 {
            tile(&w, 40, y).update(|t| {
                t.type_index = 2;
                t.back_wall_type_index = 1;
            });
        }
        sky(&w, 40, 63);
        WorldHelper::update_sun_light_for_tile_at_pos_world(None, IntPair { x: 40, y: 63 }, &w);
        let before = tile(&w, 40, 55).get().sun_light;
        assert!(before > 0);
        // block the shaft at y=58 with a solid block and run the removal driver
        tile(&w, 40, 58).update(|t| t.type_index = 1);
        WorldHelper::update_sun_light_removed_for_tile_at_pos_world(None, IntPair { x: 40, y: 58 }, &w);
        // below the plug the light comes only through attenuating neighbours: strictly dimmer
        assert!(tile(&w, 40, 55).get().sun_light < before);
        // the plugged tile itself was relit from the sky side (above contribution through rule A minus 0x41 or similar)
        assert!(tile(&w, 40, 59).get().sun_light > 0);
    }

    #[test]
    fn window_background_and_gem_promise() {
        let rec = install_rec();
        let w = solid_world();
        // a window block: background contents 0x45 forces 0xef
        tile(&w, 60, 60).update(|t| t.background_contents = 0x45);
        WorldHelper::update_sun_light_for_tile_at_pos_world(None, IntPair { x: 60, y: 60 }, &w);
        assert_eq!(tile(&w, 60, 60).get().sun_light, 0xef);
        // sky tile next to a gem promise: neighbour gets light, promise turns real, torch spawned
        sky(&w, 70, 60);
        tile(&w, 71, 60).update(|t| {
            t.foreground_contents = 0x3c;
            t.type_index = 2;
            t.back_wall_type_index = 1;
        });
        tile(&w, 70, 61).update(|t| {
            t.contents = 0x90;
            t.type_index = 1;
            t.back_wall_type_index = 1;
        });
        rec.take();
        WorldHelper::update_sun_light_for_tile_at_pos_world(None, IntPair { x: 70, y: 60 }, &w);
        let log = rec.take();
        assert!(log.contains(&"addTorch(71,60,0x58,0,0)".to_string()), "{log:?}");
        assert_eq!(tile(&w, 71, 60).get().foreground_contents, 0x3b);
        // contents cleared even though type != AIR; no chest creation then
        assert_eq!(tile(&w, 70, 61).get().contents, 0);
        assert!(log.iter().all(|s| !s.starts_with("chestOrTroll")));
    }

    #[test]
    fn recalc_lighting_radius_and_notifications() {
        let rec = install_rec();
        let w = solid_world();
        w.set_start_portal_pos(IntPair { x: 16, y: 16 });
        let pb = tile(&w, 0, 0).physical_block();
        // block (0,0): viewer = start portal, radius 4, k = 18
        WorldHelper::recalculate_lighting_for_physical_block_if_needed_world_client_light_block_index_for_blockhead(
            Some(&pb),
            &w,
            -1,
            None,
        );
        let t = |x, y| tile(&w, x, y).get();
        // centre: lsq = 0: f = 18/22, nl = (int)(255 * (4 - 4*18/22))
        let f = (0.0f32 + 18.0f32) / 22.0f32;
        let nl = f32_to_i32(255.0f32 * (4.0f32 - 4.0f32 * f));
        assert_eq!(t(16, 16).light as i32, nl.min(255));
        // beyond r^2 >= 484 nothing changes
        assert_eq!(t(0, 0).light, 0);
        let log = rec.take();
        // everything is already fully explored in this fixture: only the light notification fires
        assert!(!log.iter().any(|s| s.starts_with("lightChangedAll")));
        assert!(log.contains(&"exploreLight(0,0,-1)".to_string()));
        // second call: light only increases, nothing changes -> no notifications
        WorldHelper::recalculate_lighting_for_physical_block_if_needed_world_client_light_block_index_for_blockhead(
            Some(&pb),
            &w,
            -1,
            None,
        );
        assert!(rec.take().is_empty());
        // None pb is a no-op
        WorldHelper::recalculate_lighting_for_physical_block_if_needed_world_client_light_block_index_for_blockhead(
            None, &w, -1, None,
        );
    }

    #[test]
    fn recalc_lighting_explores_and_freezes_water() {
        let rec = install_rec();
        let w = solid_world();
        w.set_start_portal_pos(IntPair { x: 16, y: 16 });
        // unexplored water tile at the viewer: explored_fraction set; temperature is high on a default world -> no freeze
        tile(&w, 16, 16).update(|t| {
            t.explored_fraction = 0;
            t.type_index = 3;
            t.partial_content_left = 255;
        });
        let pb = tile(&w, 0, 0).physical_block();
        WorldHelper::recalculate_lighting_for_physical_block_if_needed_world_client_light_block_index_for_blockhead(
            Some(&pb),
            &w,
            -1,
            None,
        );
        assert_ne!(tile(&w, 16, 16).get().explored_fraction, 0);
        let log = rec.take();
        assert!(log.contains(&"lightChangedAll(0,0,false,false)".to_string()), "{log:?}");
        assert!(log.contains(&"exploreLight(0,0,-1)".to_string()));
    }

    #[test]
    fn decommission_decision_table() {
        let rec = install_rec();
        let w = solid_world();
        w.set_start_portal_pos(IntPair { x: 0, y: 16 });
        let mt = w.macro_tiles().get((1 * 4 + 3) as usize).unwrap(); // macro (3,1), far from the portal 3x3 at (0..1, 0..1) wrap? portal macro (0,0)
        let pb = mt.physical_block().unwrap();
        // idle long enough, nothing needs it -> decommissioned
        pb.last_required_time.set(reference_time() - 1000.0);
        assert!(WorldHelper::check_if_macro_tile_can_be_decommissioned_world_min_age_block_to_save_physcial_block(
            &mt, &w, 30.0, false
        ));
        assert_eq!(rec.take(), vec!["decommission(false)".to_string()]);
        // has dynamic objects to save -> refreshes lastRequiredTime and keeps
        rec.has_save.set(true);
        pb.last_required_time.set(reference_time() - 1000.0);
        assert!(!WorldHelper::check_if_macro_tile_can_be_decommissioned_world_min_age_block_to_save_physcial_block(
            &mt, &w, 30.0, false
        ));
        assert!(pb.last_required_time.get() > reference_time() - 10.0);
        rec.has_save.set(false);
        // minAge < 1.5 forces even with a fresh timestamp
        pb.last_required_time.set(reference_time());
        assert!(WorldHelper::check_if_macro_tile_can_be_decommissioned_world_min_age_block_to_save_physcial_block(
            &mt, &w, -1.0, true
        ));
        assert_eq!(rec.take(), vec!["decommission(true)".to_string()]);
        // interested peers keep it when minAge > 1.5, not when forcing
        rec.peers.set(1);
        pb.last_required_time.set(reference_time() - 1000.0);
        assert!(!WorldHelper::check_if_macro_tile_can_be_decommissioned_world_min_age_block_to_save_physcial_block(
            &mt, &w, 30.0, false
        ));
        pb.last_required_time.set(reference_time() - 1000.0);
        // note: peers != 0 refreshes lastRequiredTime, so min_age=1.0 forces
        assert!(WorldHelper::check_if_macro_tile_can_be_decommissioned_world_min_age_block_to_save_physcial_block(
            &mt, &w, 1.0, false
        ));
        rec.peers.set(0);
        rec.take();
        // portal 3x3 is never unloaded
        let near_portal = w.macro_tiles().get(0).unwrap();
        near_portal.physical_block().unwrap().last_required_time.set(reference_time() - 1000.0);
        assert!(!WorldHelper::check_if_macro_tile_can_be_decommissioned_world_min_age_block_to_save_physcial_block(
            &near_portal,
            &w,
            30.0,
            false
        ));
        // no physical block -> false
        let empty = World::alloc(4).macro_tiles().get(0).unwrap();
        assert!(!WorldHelper::check_if_macro_tile_can_be_decommissioned_world_min_age_block_to_save_physcial_block(
            &empty, &w, 30.0, false
        ));
    }
}

#[cfg(test)]
mod fuzz_tests {
    use super::*;
    use crate::lighting::artificial_light::testkit::*;
    use crate::objects::base::{BlockheadApi, DynamicObject, DynamicObjectBase};
    use crate::objects::handle;
    use std::cell::RefCell;

    struct Viewer {
        base: RefCell<DynamicObjectBase>,
        controlled: bool,
    }
    impl DynamicObject for Viewer {
        fn base(&self) -> &RefCell<DynamicObjectBase> {
            &self.base
        }
        fn class_info(&self) -> &'static crate::objects::class_info::ClassInfo {
            &crate::objects::class_info::DYNAMIC_OBJECT_CLASS
        }
        fn as_any(&self) -> &dyn std::any::Any {
            self
        }
        fn as_blockhead(&self) -> Option<&dyn BlockheadApi> {
            Some(self)
        }
    }
    impl BlockheadApi for Viewer {
        fn client_id(&self) -> Option<String> {
            None
        }
        fn local_net_id(&self) -> Option<String> {
            None
        }
        fn is_client_blockhead_being_controlled_by_server(&self) -> bool {
            self.controlled
        }
        fn is_interacting_with_any_interaction_object(&self) -> bool {
            false
        }
        fn set_interaction_object(&self, _o: Option<&ObjRef>) {}
        fn stop_interacting(&self) {}
    }
    fn viewer(x: i32, y: i32, controlled: bool) -> ObjRef {
        let o: ObjRef =
            handle::alloc_obj(|w| Viewer { base: RefCell::new(DynamicObjectBase::alloc_zeroed_with(w)), controlled });
        o.base().borrow_mut().pos = IntPair { x, y };
        o
    }

    struct Lcg(u64);
    impl Lcg {
        fn next(&mut self) -> u64 {
            self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            self.0 >> 33
        }
        fn pick<T: Copy>(&mut self, v: &[T]) -> T {
            v[(self.next() % v.len() as u64) as usize]
        }
    }

    fn scramble(w: &World, r: &mut Lcg, cx: i32, cy: i32, n: i32) {
        for y in cy - n..cy + n {
            for x in cx - n..cx + n {
                let (ty, bw, bg, fg, c, sun, ex, pl) = (
                    r.pick(&[2u8, 2, 2, 3, 6, 6, 1, 0x1f]),
                    r.pick(&[2u8, 2, 3, 6]),
                    r.pick(&[0u8, 0, 0, 0x45, 0x5f]),
                    r.pick(&[0u8, 0, 0, 0x34, 0x36, 0x38, 0x3a, 0x3c, 0x31, 0x4d]),
                    r.pick(&[0u8, 0, 0x5e, 0x90, 0x91, 0x4d]),
                    r.pick(&[0u8, 0xff, 0x80, 0xf0, 0x20]),
                    r.pick(&[0u8, 1, 255]),
                    r.pick(&[0u8, 100, 255]),
                );
                tile(w, x, y).update(|t| {
                    t.type_index = ty;
                    t.back_wall_type_index = bw;
                    t.background_contents = bg;
                    t.foreground_contents = fg;
                    t.contents = c;
                    t.sun_light = sun;
                    t.explored_fraction = ex;
                    t.partial_content_left = pl;
                });
            }
        }
    }

    #[test]
    fn random_worlds_through_every_sunlight_entry_point() {
        let rec = install_rec();
        rec.load_light.set(true);
        let blocks: Vec<(i32, i32)> = (0..4).flat_map(|mx| (0..2).map(move |my| (mx, my))).collect();
        let w = world_with_blocks(4, &blocks);
        let mut r = Lcg(42);
        w.set_start_portal_pos(IntPair { x: 40, y: 40 });
        for round in 0..400 {
            scramble(&w, &mut r, 40, 40, 9);
            let pos = IntPair { x: 40 + (r.next() % 12) as i32 - 6, y: 40 + (r.next() % 12) as i32 - 6 };
            match round % 4 {
                0 => WorldHelper::update_sun_light_for_tile_at_pos_world(None, pos, &w),
                1 => WorldHelper::update_sun_light_removed_for_tile_at_pos_world(None, pos, &w),
                2 => {
                    let pb = tile(&w, 40, 40).physical_block();
                    let bh = match r.next() % 4 {
                        0 => None,
                        1 => Some(viewer(40 + (r.next() % 40) as i32 - 20, 40, false)),
                        2 => Some(viewer(100, 100, false)),
                        _ => Some(viewer(40, 40, true)),
                    };
                    rec.view_radius.set(r.pick(&[1, 4, 9, 22]));
                    let idx = r.pick(&[-1, 0, 3, 31, 40]);
                    WorldHelper::recalculate_lighting_for_physical_block_if_needed_world_client_light_block_index_for_blockhead(
                        Some(&pb),
                        &w,
                        idx,
                        bh.as_ref(),
                    );
                }
                _ => {
                    rec.peers.set(r.pick(&[0, 1]));
                    rec.has_save.set(r.next() % 3 == 0);
                    rec.blockheads.borrow_mut().clear();
                    if r.next() % 2 == 0 {
                        let bx = r.pick(&[0, 1, 2, 3]) * 32 + 5;
                        rec.blockheads.borrow_mut().push(viewer(bx, r.pick(&[5, 40]), false));
                    }
                    let mt = w.macro_tiles().get((r.next() % 8) as usize).unwrap();
                    if let Some(pb) = mt.physical_block() {
                        pb.last_required_time.set(reference_time() - r.pick(&[0.0, 5.0, 1000.0]));
                    }
                    let _ = WorldHelper::check_if_macro_tile_can_be_decommissioned_world_min_age_block_to_save_physcial_block(
                        &mt,
                        &w,
                        r.pick(&[1.0, 30.0, -1.0]),
                        r.next() % 2 == 0,
                    );
                    w.set_start_portal_pos(IntPair { x: r.pick(&[10, 70, 120]), y: r.pick(&[10, 40]) });
                }
            }
            rec.take();
        }
        let v = viewer(1, 1, false);
        let _ = v.class_info();
        assert!(v.as_any().is::<Viewer>());
        let b = v.as_blockhead().unwrap();
        assert!(
            b.client_id().is_none() && b.local_net_id().is_none() && !b.is_interacting_with_any_interaction_object()
        );
        b.set_interaction_object(None);
        b.stop_interacting();
        // wrapping around the seam of the world
        for x in [0, 1, 126, 127] {
            WorldHelper::update_sun_light_for_tile_at_pos_world(None, IntPair { x, y: 40 }, &w);
            WorldHelper::update_sun_light_removed_for_tile_at_pos_world(None, IntPair { x, y: 40 }, &w);
        }
        // the single step helpers (a negative index wraps to the east end of the bottom row)
        tile(&w, 127, 0).update(|t| {
            t.type_index = 6;
            t.sun_light = 100;
        });
        let mut rem = VecDeque::from(vec![-1i32]);
        WorldHelper::recursively_remove_all_sun_light_with_list_open_indices_light_was_removed_list_remove_indices_world_minx_max_x(
            &mut rem,
            &mut IndexSet::new(),
            &mut VecDeque::new(),
            &mut IndexSet::new(),
            &w,
            0,
            127,
        );
        assert_eq!(tile(&w, 127, 0).get().sun_light, 0);
        let mut ol = VecDeque::from(vec![0i32]);
        let mut oi = IndexSet::new();
        WorldHelper::recursively_update_sun_light_with_list_open_indices_world(&mut ol, &mut oi, &w);
        WorldHelper::recursively_update_sun_light_with_list_open_indices_world(&mut ol, &mut oi, &w);
    }

    #[test]
    fn removal_stops_at_the_horizontal_limits() {
        install_rec();
        let blocks: Vec<(i32, i32)> = (0..4).flat_map(|mx| (0..2).map(move |my| (mx, my))).collect();
        let w = world_with_blocks(4, &blocks);
        for x in 0..128 {
            tile(&w, x, 40).update(|t| {
                t.type_index = 6;
                t.back_wall_type_index = 6;
                t.sun_light = 200;
                t.explored_fraction = 255;
            });
        }
        WorldHelper::update_sun_light_removed_for_tile_at_pos_world(None, IntPair { x: 64, y: 40 }, &w);
        // at the top of the loaded world the tile above is missing
        for x in 0..128 {
            tile(&w, x, 63).update(|t| {
                t.type_index = 6;
                t.back_wall_type_index = 6;
                t.sun_light = 200;
            });
        }
        WorldHelper::update_sun_light_removed_for_tile_at_pos_world(None, IntPair { x: 64, y: 63 }, &w);
        // the reach is 32 tiles each way
        assert!(tile(&w, 100, 40).get().sun_light > 0 || tile(&w, 28, 40).get().sun_light > 0);
    }

    #[test]
    fn viewers_across_the_seam_and_cold_water() {
        let rec = install_rec();
        rec.load_light.set(true);
        rec.view_radius.set(9);
        let blocks: Vec<(i32, i32)> = (0..4).flat_map(|mx| (0..2).map(move |my| (mx, my))).collect();
        let w = world_with_blocks(4, &blocks);
        // blocks east and west of a viewer at the other end of the world
        for (bx, viewer_x) in [(40, 120), (100, 2), (100, 40), (10, 120)] {
            let pb = tile(&w, bx, 40).physical_block();
            for idx in [-1, 2] {
                let v = viewer(viewer_x, 40, false);
                WorldHelper::recalculate_lighting_for_physical_block_if_needed_world_client_light_block_index_for_blockhead(
                    Some(&pb),
                    &w,
                    idx,
                    Some(&v),
                );
            }
            // a second pass with a lit client block and a stronger viewer
            rec.view_radius.set(22);
            let v = viewer(viewer_x, 40, false);
            WorldHelper::recalculate_lighting_for_physical_block_if_needed_world_client_light_block_index_for_blockhead(
                Some(&pb),
                &w,
                2,
                Some(&v),
            );
        }
        // unexplored water in the cold freezes
        rec.weather.set(2.0);
        let mut froze = false;
        for t in [0.0, 1350.0, 2700.0, 4050.0] {
            w.set_world_time(t);
            tile(&w, 40, 40).update(|t| {
                t.type_index = 3;
                t.partial_content_left = 255;
                t.explored_fraction = 0;
                t.artificial_heat = -800;
            });
            let pb = tile(&w, 40, 40).physical_block();
            WorldHelper::recalculate_lighting_for_physical_block_if_needed_world_client_light_block_index_for_blockhead(
                Some(&pb),
                &w,
                -1,
                Some(&viewer(40, 40, false)),
            );
            froze |= rec.take().iter().any(|l| l.starts_with("fill(40,40,0x422"));
        }
        assert!(froze);
    }

    #[test]
    fn blockheads_across_the_seam_keep_blocks_loaded() {
        let rec = install_rec();
        let blocks: Vec<(i32, i32)> = (0..4).flat_map(|mx| (0..2).map(move |my| (mx, my))).collect();
        let w = world_with_blocks(4, &blocks);
        w.set_start_portal_pos(IntPair { x: 0, y: 0 });
        let mt = w.macro_tiles().get(7).unwrap(); // macro (3, 1)
        let pb = mt.physical_block().unwrap();
        // a blockhead at the far west is still next to this block over the seam
        pb.last_required_time.set(reference_time() - 1000.0);
        rec.blockheads.borrow_mut().push(viewer(5, 40, false));
        assert!(!WorldHelper::check_if_macro_tile_can_be_decommissioned_world_min_age_block_to_save_physcial_block(
            &mt, &w, 30.0, false
        ));
        // one two blocks away is not
        rec.blockheads.borrow_mut().clear();
        rec.blockheads.borrow_mut().push(viewer(40, 40, false));
        assert!(WorldHelper::check_if_macro_tile_can_be_decommissioned_world_min_age_block_to_save_physcial_block(
            &mt, &w, 30.0, false
        ));
    }

    #[test]
    fn narrow_worlds_wrap_the_light_radius_both_ways() {
        let rec = install_rec();
        rec.view_radius.set(22);
        let w = world_with_blocks(2, &[(0, 0), (1, 0), (0, 1), (1, 1)]);
        for vx in [0, 10, 31, 32, 50, 63] {
            for (tx, ty) in [(5, 40), (40, 40)] {
                let pb = tile(&w, tx, ty).physical_block();
                let v = viewer(vx, 40, false);
                WorldHelper::recalculate_lighting_for_physical_block_if_needed_world_client_light_block_index_for_blockhead(
                    Some(&pb),
                    &w,
                    -1,
                    Some(&v),
                );
            }
        }
    }
}
