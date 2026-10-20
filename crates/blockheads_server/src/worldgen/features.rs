//! owned by worldgen
//!
//! Spawn tables (Step F of `init`), `findBestStartPosition`, gems / treasure in caves, the ore cascade and the
//! floating (sky) islands.
//!
//! 2.1 (islands, ore cascade, 2.1b), 2.2;, R7.

use std::rc::Rc;

use crate::fmath::cconv::{f32_to_i32, f64_to_i32};
use crate::fmath::libm::{cosf, fabsf, fmodf};
use crate::fmath::vector::{clamp, clampi};
use crate::types::enums::{IslandType, PlantType, TreeType};
use crate::types::structs::IntPair;
use crate::types::tile::Tile;
use crate::world::climate::base_temperature_for_world_pos;
use crate::world::growth::{
    growth_vigor_for_plant_type_at_pos, growth_vigor_for_tree_type_at_pos, tree_promise_type_for_island_type,
};
use crate::world::state::{PhysicalBlockRef, World};

use super::noise::{noise_xy, NoiseFunction};
use super::terrain::{r01, tt, GenParams, Terrain};

/// Ore spread parameters: coal, copper/tin/iron, gold/platinum, titanium.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OreSpread {
    pub coal: f32,
    pub copper_tin_iron: f32,
    pub gold_platinum: f32,
    pub titanium: f32,
}

impl OreSpread {
    /// `coalSpread=0.025f; copperTinIronSpread=0.004f; goldPlatinumSpread=0.001f; titaniumSpread=0.002f`,
    /// expert `*5.0f`, then by `customRules.oreQuantity` (0: negate, 1: x0.2 with f64 factors for gold/titanium,
    /// 3: x10).
    pub fn new(p: &GenParams) -> OreSpread {
        let mut coal = 0.025f32;
        let mut cti = 0.004f32;
        let mut gp = 0.001f32;
        let mut ti = 0.002f32;
        if p.expert {
            coal *= 5.0f32;
            cti *= 5.0f32;
            gp *= 5.0f32;
            ti *= 5.0f32;
        }
        match p.rules.ore_quantity {
            0 => {
                coal = -coal;
                cti = -cti;
                gp = -gp;
                ti = -ti;
            }
            1 => {
                coal *= 0.2f32;
                cti *= 0.2f32;
                gp = ((gp as f64) * 0.5000000074505806) as f32;
                ti = ((ti as f64) * 0.30000000447034836) as f32;
            }
            3 => {
                coal *= 10.0f32;
                cti *= 10.0f32;
                gp *= 10.0f32;
                ti *= 10.0f32;
            }
            _ => {}
        }
        OreSpread { coal, copper_tin_iron: cti, gold_platinum: gp, titanium: ti }
    }
}

/// `inBand(n, spread, A)` of the ore cascade (float math).
#[inline]
fn in_band(n: f32, spread: f32, a: f32) -> bool {
    ((0.3f32 - spread) - a < n && n < 0.3f32) || (n < -0.3f32 + spread + a && -0.3f32 < n)
}

/// Result of the once-per-block floating island planning. `-1` values mean "not set".
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IslandPlan {
    /// `IslandType` value (0 = none).
    pub island_type: u32,
    /// `floatingIslandLocalY`
    pub local_y: i32,
    /// `floatingIslandWidth`
    pub width: i32,
    /// `floatingIslandLocalMinX`
    pub local_min_x: i32,
    /// `cloudTreeX`
    pub cloud_tree_x: i32,
}

impl IslandPlan {
    /// `ISLAND_NONE` and all the `-1` sentinels.
    pub fn none() -> IslandPlan {
        IslandPlan { island_type: 0, local_y: -1, width: -1, local_min_x: -1, cloud_tree_x: -1 }
    }
}

const ISL_AMETHYST: u32 = 1;
const ISL_SAPPHIRE: u32 = 2;
const ISL_EMERALD: u32 = 3;
const ISL_RUBY: u32 = 4;
const ISL_DIAMOND: u32 = 5;
const ISL_COCONUT: u32 = 7;
const ISL_CACTUS: u32 = 11;

impl Terrain {
    // ------------------------------------------------------------------------------------------------
    //  ORE_CASCADE
    // ------------------------------------------------------------------------------------------------

    /// The ore cascade for a plain rock tile: the `contents` byte to set, if any (never cleared).
    /// `y` is the local row, `local_rock` the column's local rock height.
    #[allow(clippy::too_many_arguments)]
    pub fn ore_for_rock_tile(
        &self,
        p: &GenParams,
        sp: &OreSpread,
        world_x: i32,
        world_y: i32,
        y: i32,
        local_rock: i32,
        lime: f32,
        fault: i32,
    ) -> Option<u8> {
        let is_coal = (local_rock - 64 < y) && (lime < -0.3f32) && (-0.3f32 - sp.coal < lime);
        if is_coal {
            return Some(0x41);
        }
        let w_f = p.w as f32;
        let xf = ((((world_x as f32) + 5673.0f32) / 32.0f32) / w_f) * 4.0f32;
        let den = if p.w < 512 { 512 } else { p.w };
        let yf = ((((world_y + 0x18c7) - fault) as f32) / 32.0f32 / (den as f32)) * 8.0f32;
        let g = noise_xy(&self.flint, xf as f64 * 0.112, yf as f64 * 0.084, 2) as f32;
        if !(g > 0.0f32) {
            return None;
        }
        let rock_h = self.rock_heights[world_x as usize];
        let wy400 = if world_y < 400 { 400 } else { world_y };
        let a1 = (1.0f32 - (wy400 as f32) / (rock_h as f32)) * 0.004f32;
        let (xd, yd) = (xf as f64, yf as f64);
        let n = noise_xy(&self.tin, xd * 0.05 + 0.4, yd, 2) as f32;
        if in_band(n, sp.copper_tin_iron, a1) {
            return Some(0x3d);
        }
        let n = noise_xy(&self.tin, xd * 0.05, yd, 2) as f32;
        if in_band(n, sp.copper_tin_iron, a1) {
            return Some(0x3e);
        }
        let n = noise_xy(&self.tin, xd * 0.07 + 0.73, yd + 0.45, 2) as f32;
        let wy300 = if world_y < 300 { 300 } else { world_y };
        let a2 = (1.0f32 - ((wy300 + 0x15e) as f32) / 600.0f32) * 0.004f32;
        if in_band(n, sp.copper_tin_iron, a2) {
            return Some(0x3f);
        }
        let n = noise_xy(&self.tin, xd * 0.044 + 0.412, yd + 0.45, 2) as f32;
        if in_band(n, sp.gold_platinum, a2) {
            return Some(0x4d);
        }
        let n = noise_xy(&self.tin, xd * 0.012 + 0.212, yd + 0.47, 2) as f32;
        if in_band(n, sp.gold_platinum, a2) {
            let r = noise_xy(&self.flint, xd * 0.012, yd * 0.054, 2) as f32;
            if 0.1f32 < r && r < 0.15f32 {
                return Some(0x6a);
            }
        } else {
            let n = noise_xy(&self.tin, xd * 0.028 + 0.612, yd + 0.27, 2) as f32;
            if in_band(n, sp.titanium, a2) {
                let r = noise_xy(&self.flint, xd * 0.035, yd * 0.072, 2) as f32;
                if 0.1f32 < r && r < 0.15f32 {
                    return Some(0x6b);
                }
            }
        }
        None
    }

    // ------------------------------------------------------------------------------------------------
    //  placeGemsInCave
    // ------------------------------------------------------------------------------------------------

    /// `-placeGemsInCaveForPhysicalBlock:tileIndex:worldX:worldY:floatingIslandType:` on a tile value
    /// (writes only `typeIndex`, `partialContentLeft`, `contents`, `foregroundContents`).
    pub fn place_gems_in_cave_tile(&self, p: &GenParams, tile: &mut Tile, wx: i32, wy: i32, isl: u32) {
        let yh = self.y_height_divider;
        let mut x_f = (((wx as f32) + 1453.0f32) / 32.0f32) / yh;
        let mut y_f = (((wy as f32) + 1247.0f32) / 32.0f32) / yh;
        let mut placed = false;
        let below_is_cave = if isl == 0 {
            let fo = self.fault_offset(p, wx, wy - 1);
            self.is_cave(p, wx, wy - 1, fo)
        } else {
            self.is_floating_island_cave(p, wx, wy - 1)
        };
        if !below_is_cave {
            let mut tc = noise_xy(&self.flint, (1.423f32 * x_f) as f64, (1.4536f32 * y_f) as f64, 5) as f32;
            let mut min_v = 0.85f32;
            let y_mul = if isl == 0 { (0.5f64 * (((wy as f32) / 1024.0f32) as f64)) as f32 } else { 0.0f32 };
            let mut skip_crystal = false;
            if wx != self.best_start_position.x {
                let d = if p.expert { 0.22f32 } else { 0.11f32 };
                if tc <= min_v - d * (1.0f32 - y_mul) || min_v <= tc {
                    // OVERWRITES xF / yF: the gem stage below uses these values (verification note 3).
                    x_f = (((wx as f32) + 2453.0f32) / 32.0f32) / yh;
                    y_f = (((wy as f32) + 1147.0f32) / 32.0f32) / yh;
                    tc = noise_xy(&self.flint, (0.823f32 * x_f) as f64, (1.2536f32 * y_f) as f64, 5) as f32;
                    min_v = 0.83f32;
                    let mut fm = 1.0f32;
                    if p.rules.caves == 2 {
                        fm = 0.4f32;
                    }
                    if p.expert {
                        fm = 0.5f32 * fm;
                    }
                    let mut tw = min_v - 0.03f32 * fm * (1.0f32 - y_mul);
                    let troll = tw;
                    if p.rules.treasure == 1 {
                        tw = min_v - 0.03f32 * fm * 0.25f32 * (1.0f32 - y_mul);
                    } else if p.rules.treasure == 3 {
                        tw = min_v - 0.03f32 * fm * 4.0f32 * (1.0f32 - y_mul);
                    } else if p.rules.treasure == 0 {
                        tw = 1.0f32 + min_v;
                    }
                    let lo = if tw < troll { tw } else { troll };
                    if lo < tc && tc < min_v {
                        placed = true;
                        tile.type_index = tt::AIR;
                        tile.partial_content_left = 0;
                        if tc <= tw {
                            if troll < tc {
                                tile.contents = 0x90;
                            }
                        } else if tc <= troll {
                            tile.contents = 0x91;
                        } else {
                            tile.contents = 0x5e;
                        }
                    }
                    skip_crystal = true;
                }
            }
            if !skip_crystal && p.rules.custom_rules_enabled == 0 {
                placed = true;
                if isl == 0 {
                    tile.type_index = tt::TIME_CRYSTAL;
                }
            }
        }
        if !placed {
            tile.type_index = tt::AIR;
            tile.partial_content_left = 0;
            if p.rules.gems != 0 {
                let gem_type: u8 = match isl {
                    ISL_AMETHYST => 0x3a,
                    ISL_SAPPHIRE => 0x38,
                    ISL_EMERALD => 0x36,
                    ISL_RUBY => 0x34,
                    ISL_DIAMOND => 0x3c,
                    _ => 0,
                };
                let is_gem_island = gem_type != 0;
                let base = if is_gem_island {
                    1.0f64
                } else {
                    noise_xy(&self.flint, x_f as f64 * 0.112, y_f as f64 * 0.084, 2)
                } as f32;
                let mut fq = if is_gem_island {
                    4.0f32
                } else if p.rules.caves != 2 {
                    1.0f32
                } else {
                    0.21f32
                };
                if p.rules.gems == 1 {
                    fq = 0.25f32 * fq;
                } else if p.rules.gems == 3 {
                    fq = 4.0f32 * fq;
                }
                if 0.0f32 < base {
                    let wy_n = (wy as f32) / 1024.0f32;
                    let g = noise_xy(&self.gem, x_f as f64 * 0.15, y_f as f64 * 0.1, 5) as f32;
                    let near = |n: f64| -> bool { (n - 0.2000000029802322).abs() < 0.019999999552965164 };
                    let pick = |t: u8| -> u8 {
                        if is_gem_island {
                            gem_type
                        } else {
                            t
                        }
                    };
                    // AMETHYST
                    if -0.22f32 < g
                        && g < -0.22f32 + 0.15f32 * fq
                        && (isl != 0 || (fabsf(wy_n - 0.5f32) as f64) < 0.3f64)
                    {
                        let n = noise_xy(&self.gem, (1.5f32 * x_f) as f64 + 0.2, (1.34f32 * y_f) as f64 + 0.4, 7);
                        if near(n) {
                            tile.foreground_contents = pick(0x3a);
                        }
                    }
                    // SAPPHIRE
                    if -0.35f32 < g && g < -0.35f32 + 0.06f32 * fq && (isl != 0 || fabsf(wy_n - 0.4f32) < 0.3f32) {
                        let n = noise_xy(&self.gem, (1.4f32 * x_f) as f64 + 0.3, (1.6f32 * y_f) as f64 + 0.2, 7);
                        if near(n) {
                            tile.foreground_contents = pick(0x38);
                        }
                    }
                    // EMERALD
                    if 0.25f32 < g && g < 0.25f32 + 0.03f32 * fq && (isl != 0 || fabsf(wy_n - 0.3f32) < 0.2f32) {
                        let n = noise_xy(&self.gem, (1.2f32 * x_f) as f64 + 0.1, (1.1f32 * y_f) as f64 + 0.1, 7);
                        if near(n) {
                            tile.foreground_contents = pick(0x36);
                        }
                    }
                    // RUBY
                    if 0.28f32 < g && g < 0.28f32 + 0.02f32 * fq && (isl != 0 || fabsf(wy_n - 0.2f32) < 0.2f32) {
                        let n = noise_xy(&self.gem, (1.1f32 * x_f) as f64 + 0.44, (1.23f32 * y_f) as f64 + 0.3, 7);
                        if near(n) {
                            tile.foreground_contents = pick(0x34);
                        }
                    }
                    // DIAMOND
                    if 0.6f32 < g && g < 0.6f32 + 0.03f32 * fq && (isl != 0 || fabsf(wy_n - 0.1f32) < 0.2f32) {
                        let n = noise_xy(&self.gem, (1.6f32 * x_f) as f64 + 0.4, (1.13f32 * y_f) as f64 + 0.4, 7);
                        if near(n) {
                            tile.foreground_contents = pick(0x3c);
                        }
                    }
                }
            }
        }
    }

    /// `-placeGemsInCaveForPhysicalBlock:tileIndex:worldX:worldY:floatingIslandType:`: reads / writes `pb.tiles[ti]`.
    pub fn place_gems_in_cave_for_physical_block_tile_index_world_x_world_y_floating_island_type(
        &self,
        p: &GenParams,
        pb: PhysicalBlockRef,
        ti: usize,
        wx: i32,
        wy: i32,
        isl: u32,
    ) {
        let mut t = pb.tiles[ti].get();
        self.place_gems_in_cave_tile(p, &mut t, wx, wy, isl);
        pb.tiles[ti].set(t);
    }

    // ------------------------------------------------------------------------------------------------
    // : floating island planning and overlay
    // ------------------------------------------------------------------------------------------------

    /// Once-per-block floating island planning. `world` supplies the tree growth vigor.
    /// R7: the position passed to the vigor function is literally `(yPos<<5, yPos<<5)`.
    pub fn plan_floating_island(&self, p: &GenParams, x_pos: i32, y_pos: i32, world: Option<&World>) -> IslandPlan {
        let mut plan = IslandPlan::none();
        if !(p.rules.sky_islands != 0 && y_pos * 32 < 900 && y_pos * 32 > 0x240) {
            return plan;
        }
        let yh = self.y_height_divider;
        let x_f = ((((x_pos as f32) * 32.0f32) + 233.0f32) / 32.0f32) / yh;
        let y_f = ((((y_pos as f32) * 32.0f32) + 9234.0f32) / 32.0f32) / yh;
        let (xd, yd) = (x_f as f64, y_f as f64);
        let random_island_gen = (noise_xy(&self.flint, xd * 15.215, yd * 37.084, 2).abs() * 2.5) as f32;
        let mut island_threshold = 0.75f32;
        let mut gem_island_threshold = 1.4f32;
        if p.rules.sky_islands == 3 {
            island_threshold = 0.1f32;
            gem_island_threshold = 1.0f32;
        } else if p.rules.sky_islands == 1 {
            island_threshold = 1.4f32;
            gem_island_threshold = 1.65f32;
        }
        if !(island_threshold < random_island_gen) {
            return plan;
        }
        let noise_fraction = noise_xy(&self.gem, xd * 0.05, yd * 0.05, 5) as f32;
        if !(noise_fraction > 0.0f32) {
            return plan;
        }
        let (_c_r, c_d) = self.get_rock_and_dirt_height(p, x_pos * 32 + 16);
        if !(c_d + 64 < y_pos * 32) {
            return plan;
        }
        let island: u32;
        if random_island_gen <= gem_island_threshold {
            // TREE island
            let rn = (noise_xy(&self.flint, xd * 0.126 + 0.61, yd * 0.341 + 0.21, 2) * 0.5 + 0.5) as f32;
            let pos = IntPair { x: y_pos << 5, y: y_pos << 5 };
            let ok = |t: TreeType| -> bool { (growth_vigor_for_tree_type_at_pos(t, pos, world) as f64) > 0.2f64 };
            let rn = rn as f64;
            island = if rn < 0.2 && ok(TreeType::TREE_COFFEE) {
                IslandType::ISLAND_COFFEE.0
            } else if rn < 0.3 && ok(TreeType::TREE_MANGO) {
                IslandType::ISLAND_COCONUT.0 // sic (R7)
            } else if rn < 0.37 && ok(TreeType::TREE_ORANGE) {
                IslandType::ISLAND_ORANGE.0
            } else if rn < 0.44 && ok(TreeType::TREE_LIME) {
                IslandType::ISLAND_LIME.0
            } else if rn < 0.5 && ok(TreeType::TREE_CHERRY) {
                IslandType::ISLAND_CHERRY.0
            } else if rn < 0.56 && ok(TreeType::TREE_MAPLE) {
                IslandType::ISLAND_MAPLE.0
            } else if rn < 0.64 && ok(TreeType::TREE_MANGO) {
                IslandType::ISLAND_MANGO.0
            } else if rn < 0.7 && ok(TreeType::TREE_APPLE) {
                IslandType::ISLAND_APPLE.0
            } else if rn < 0.8 && ok(TreeType::TREE_PINE) {
                IslandType::ISLAND_PINE.0
            } else {
                IslandType::ISLAND_CACTUS.0
            };
        } else {
            // GEM island
            let r = (noise_xy(&self.flint, xd * 12.13 + 0.03, yd * 3.688 + 0.42, 2).abs() * 2.5) as f32;
            let mut gi = ISL_AMETHYST;
            if r < 0.5f32 {
                gi = if r >= 0.25f32 {
                    ISL_SAPPHIRE
                } else if r >= 0.125f32 {
                    ISL_EMERALD
                } else if r >= 0.04f32 {
                    ISL_RUBY
                } else {
                    ISL_DIAMOND
                };
            }
            island = gi;
        }
        if island != 0 {
            plan.island_type = island;
            let n1 = noise_xy(&self.flint, xd * 0.312 + 0.31, yd * 0.194 + 0.35, 2);
            let mut w = f64_to_i32(15.0 + (n1 * 0.5 + 0.5) * 15.0);
            if w > 29 {
                w = 30;
            }
            plan.width = w;
            let max_start_x = 32 - w;
            let n2 = noise_xy(&self.flint, xd * 0.212 + 0.23, yd * 0.094 + 0.45, 2);
            let min_x = clampi(f64_to_i32((n2 * 2.0 + 1.0) * 0.5 * (max_start_x as f64)), 0, max_start_x);
            plan.local_min_x = min_x;
            let n3 = noise_xy(&self.flint, xd * 0.156 + 0.17, yd * 0.122 + 0.21, 2);
            plan.local_y = clampi(f64_to_i32((w as f64) + (n3 * 2.0 + 1.0) * 0.5 * ((31 - w) as f64)), w, 31);
            if w > 12 {
                let n4 = noise_xy(&self.flint, xd * 0.126 + 0.19, yd * 0.1662 + 0.24, 2);
                let mut cx = f64_to_i32(((min_x + w / 2) as f64) + n4 * (w as f64));
                if cx > min_x + w - 4 || cx < min_x + 4 {
                    cx = -1;
                }
                plan.cloud_tree_x = cx;
            }
        }
        plan
    }

    /// Floating island overlay for one air-row tile `(x, y)` of a block;
    /// `t` is the tile being built (already air / water defaults), writes of the tile above go to `pb` directly.
    /// Draws `rand()` only on island grass tiles with plants enabled.
    #[allow(clippy::too_many_arguments)]
    pub fn overlay_island_tile(
        &self,
        p: &GenParams,
        pb: PhysicalBlockRef,
        plan: &IslandPlan,
        x: i32,
        y: i32,
        world_x: i32,
        y_pos: i32,
        t: &mut Tile,
    ) {
        let min_x = plan.local_min_x;
        let w = plan.width;
        let ly = plan.local_y;
        let island = plan.island_type;
        let yh = self.y_height_divider;
        let is_top = y == ly && x > min_x + 1 && x < min_x + w - 2;
        let mut is_dirt = is_top;
        let mut is_dirt_grass = is_top;
        if is_top && x == plan.cloud_tree_x {
            // `contents` is a function-local in the original; only the tile above receives the promise byte.
            let mut c: u8 = 0;
            if p.rules.trees != 0 {
                c = tree_promise_type_for_island_type(IslandType(island)) as u8;
            }
            if p.rules.trees == 1 && !matches!(c, 0x6c | 0x80 | 0x81 | 0x82 | 0x83) {
                c = 0;
            }
            if c != 0 && y + 1 < 32 {
                let above = ((y + 1) * 32 + x) as usize;
                let mut a = pb.tiles[above].get();
                a.contents = c;
                pb.tiles[above].set(a);
            }
        }
        if y == ly - 1 && x > min_x && x < min_x + w - 1 {
            let xf3 = (((world_x as f32) + 2313.0f32) / 32.0f32) / yh;
            let m = noise_xy(&self.flint, (1.1216f32 * xf3) as f64, 0.2316000014543533, 5) as f32;
            if m > 0.0f32 {
                is_dirt = true;
                is_dirt_grass = x < min_x + 2 || min_x + w - 3 < x;
            }
        }
        if !is_dirt {
            if y < ly && ly - w <= y {
                let pi = 3.141592653589793f64;
                let c = cosf(((((x as f32) - (min_x as f32)) / (w as f32)) as f64 * pi * 2.0 + pi) as f32);
                let nn = noise_xy(
                    &self.flint,
                    (0.086f32 * ((((world_x as f32) + 5313.0f32) / 32.0f32) / yh)) as f64,
                    0.27316001057624817,
                    5,
                );
                let a = f64_to_i32((w as f64) * (((c * 0.5f32 + 0.5f32) as f64) * 0.5 + 0.2000000029802322));
                let cc = f64_to_i32((a as f64) + ((nn + 0.4000000059604645) as f32 as f64) * 0.4 * (w as f64));
                let mut thick = f64_to_i32((cc as f64) * 0.5);
                if thick >= w {
                    thick = w;
                }
                if ly - thick < y {
                    let mut ty = tt::LIMESTONE;
                    if (ISL_AMETHYST..=ISL_DIAMOND).contains(&island) {
                        ty = tt::MARBLE;
                    } else if island == ISL_CACTUS {
                        ty = tt::SANDSTONE;
                    }
                    t.back_wall_type_index = ty;
                    let world_y = y_pos * 32 + y;
                    if !self.is_floating_island_cave(p, world_x, world_y) {
                        t.type_index = ty;
                        t.sun_light = 0;
                    } else {
                        self.place_gems_in_cave_tile(p, t, world_x, world_y, island);
                    }
                }
            }
        } else {
            let mut base = tt::DIRT;
            if is_dirt_grass {
                base = tt::DIRT_GRASS;
            }
            if island == ISL_COCONUT || island == ISL_CACTUS {
                base = tt::SAND;
            }
            t.type_index = base;
            t.back_wall_type_index = base;
            t.sun_light = 0;
            if base == tt::DIRT_GRASS && p.rules.plants != 0 {
                if (y + 1 >= 32) || r01("worldgen::island_vine_gate") <= 0.95f32 {
                    if 0.9f32 < r01("worldgen::island_vine") {
                        t.foreground_contents = 0x7f;
                    }
                } else {
                    let above = ((y + 1) * 32 + x) as usize;
                    let mut a = pb.tiles[above].get();
                    if (ISL_AMETHYST..=ISL_DIAMOND).contains(&island) {
                        a.foreground_contents = if island == ISL_DIAMOND { 0x8f } else { 0x8e };
                    } else if base == tt::DIRT_GRASS {
                        a.foreground_contents = 0x7e;
                    }
                    pb.tiles[above].set(a);
                }
            }
        }
    }

    // ------------------------------------------------------------------------------------------------
    //  findBestStartPosition
    // ------------------------------------------------------------------------------------------------

    /// `-findBestStartPosition` -> `(-1, -1)` when no strip is found. Draws `rand()` at entry and (when found) at
    /// the end.
    pub fn find_best_start_position(&self, p: &GenParams, world: Option<&World>) -> IntPair {
        let wall_w = p.wt();
        let w = p.w;
        let start_offset = crate::rng::glibc_rand::rand_tagged("worldgen::find_best_start_position_offset") % wall_w;
        let mut land_count: i32 = 0;
        let mut land_start_x: i32 = 0;
        let mut land_end_x: i32 = -1;
        let mut prev_height: i32 = 0x200;
        for i in 0..wall_w {
            let x = (start_offset + i) % wall_w;
            let wf = wall_w as f64;
            if (x as f64) < wf * 0.0625 {
                land_count = 0;
            } else if wf * 0.4375 < (x as f64) && (x as f64) < wf * 0.5625 {
                land_count = 0;
            } else if wf * 0.9375 < (x as f64) {
                land_count = 0;
            } else if (wall_w / 4) - w / 8 < x && x < (wall_w / 4) + w / 8 {
                land_count = 0;
            } else if ((wall_w / 4) * 3) - w / 8 < x && x < ((wall_w / 4) * 3) + w / 8 {
                land_count = 0;
            } else {
                let dirt = self.dirt_heights[x as usize];
                let rock = self.rock_heights[x as usize];
                let combined = if rock < dirt { dirt } else { rock };
                let temp = base_temperature_for_world_pos(IntPair { x, y: dirt }, 0.25f32, 0.25f32, 0.0f32, world);
                let temp_ok = if p.rules.temperature == 2 {
                    -5.0f32 < temp && temp < 25.0f32
                } else {
                    -10.0f32 < temp && temp < 40.0f32
                };
                let mut good = self.lake_heights[x as usize] == 0
                    && combined >= 0x200
                    && rock < dirt
                    && temp_ok
                    && (prev_height - combined).abs() < 2;
                if good {
                    let dr = self.is_desert_or_beach(p, IntPair { x, y: dirt - 1 }, dirt);
                    if dr {
                        good = if p.rules.rainfall == 1 { true } else { p.rules.rainfall == 0 };
                    }
                }
                if good {
                    if land_count == 0 {
                        land_start_x = x;
                    }
                    land_count += 1;
                    prev_height = combined;
                } else {
                    let need = if w / 2 < 0x101 { w / 2 } else { 0x100 };
                    if land_count < need
                        && (p.rules.oceans != 3 || land_count < 0x20)
                        && (p.rules.oceans != 4 || land_count < 8)
                    {
                        land_count = 0;
                        prev_height = combined;
                    } else {
                        land_end_x = x;
                        break;
                    }
                }
            }
        }
        if land_end_x == -1 {
            return IntPair { x: -1, y: -1 };
        }
        let r = r01("worldgen::find_best_start_position_pick");
        let px = f32_to_i32(((land_start_x + 4) as f32) + r * ((land_count - 8) as f32)) % wall_w;
        let py = self.unmodified_ground_level(p, px);
        IntPair { x: px, y: py }
    }

    // ------------------------------------------------------------------------------------------------
    //  Step F: spawn tables
    // ------------------------------------------------------------------------------------------------

    /// Step F of `init`: fills `tree_positions`, `npc_positions`, `plant_positions` and returns the 31 weighted
    /// food values. `rand()` draws are conditional and ordered exactly as the spec.
    pub fn generate_spawn_tables(&mut self, p: &GenParams, world: Option<&World>) -> [i32; 31] {
        let wt = p.wt();
        let n = wt.max(0) as usize;
        for v in [&mut self.tree_positions, &mut self.npc_positions, &mut self.plant_positions] {
            v.clear();
            v.resize(n, 0);
        }
        let mut food = [0i32; 31];
        let tree_add: f32 = if p.rules.trees == 3 {
            -0.2f32
        } else if p.rules.trees == 5 {
            0.7f32
        } else {
            0.0f32
        };
        let plant_add: f32 = if p.rules.plants == 1 {
            -0.2f32
        } else if p.rules.plants == 3 {
            0.7f32
        } else {
            0.0f32
        };
        let best = self.best_start_position;
        let td: Option<Rc<NoiseFunction>> = self.tree_density.clone();
        let wt_f = wt as f32;
        let nz = |xx: i32, y: f64| -> f64 { noise_xy(&td, ((xx as f32) / wt_f) as f64, y, 5) };
        let vt = |t: TreeType, pos: IntPair| growth_vigor_for_tree_type_at_pos(t, pos, world);
        let vp = |t: PlantType, pos: IntPair| growth_vigor_for_plant_type_at_pos(t, pos, world);
        let c01: f64 = 0.10000000149011612; // (double)0.1f
        let c02: f64 = 0.20000000298023224; // (double)0.2f

        for x in 2..(wt - 1) {
            if x == wt / 4 || x == (wt / 4) * 3 {
                continue;
            }
            let xi = x as usize;
            let rock = self.rock_heights[xi];
            let dirt = self.dirt_heights[xi];
            if rock < dirt && dirt > 0x1ff && self.lake_heights[xi] == 0 {
                // ================= LAND COLUMN =================
                let height = if dirt < rock { rock } else { dirt };
                let pos_h = IntPair { x, y: height };
                let sand_fraction = self.sand_fraction_for_pos_height(p, IntPair { x, y: height - 1 }, height, true);
                let is_desert = (0.3f64) < sand_fraction as f64;
                let is_beach = self.is_beach(p, IntPair { x, y: height - 1 }, height);
                let desert_or_beach = is_desert || is_beach;
                let away_start4 = x < best.x - 4 || best.x + 4 < x;
                let dist = (x - best.x).abs();
                // ---- trees
                if self.tree_positions[xi - 1] == 0
                    && away_start4
                    && p.rules.trees != 1
                    && p.rules.trees != 2
                    && p.rules.trees != 0
                {
                    // (type, x offset, Y, food slot and divisor)
                    let rows: [(u32, i32, f64, Option<(usize, i32)>); 6] = [
                        (TreeType::TREE_APPLE.0, 0, 0.34523, Some((0, 1))),
                        (TreeType::TREE_MAPLE.0, 0x8b51, 0.14523, None),
                        (TreeType::TREE_MANGO.0, 1000, 0.74523, Some((1, 1))),
                        (TreeType::TREE_ORANGE.0, 2000, 0.19523, Some((6, 1))),
                        (TreeType::TREE_CHERRY.0, 0x98d, 0.742, Some((0xb, 1))),
                        (TreeType::TREE_COFFEE.0, 0x5b0, 0.793, Some((0xc, 4))),
                    ];
                    for (i, (ty, off, yc, fd)) in rows.iter().enumerate() {
                        if desert_or_beach || (i > 0 && self.tree_positions[xi] != 0) {
                            continue;
                        }
                        let mut v = (nz(x + off, *yc) + c01 + tree_add as f64) as f32;
                        if v > 0.0f32 {
                            v = v * (vt(TreeType(*ty), pos_h) - 0.2f32);
                            if v > 0.0f32 {
                                let r = r01("worldgen::spawn_tree") * 2.0f32;
                                if r < v * v {
                                    self.tree_positions[xi] = *ty as i32;
                                    if let Some((slot, div)) = fd {
                                        if dist < 1000 {
                                            food[*slot] += (1000 - dist) / div;
                                        }
                                    }
                                }
                            }
                        }
                    }
                    // PINE / COCONUT
                    if !is_desert && self.tree_positions[xi] == 0 {
                        let mut pine_value = (nz(x + 0x5fff, 0.8754) + c01 + tree_add as f64) as f32;
                        if pine_value > 0.0f32 {
                            let mut ty = TreeType::TREE_PINE;
                            let dist_t: f32;
                            if p.rules.temperature == 2 {
                                let f = fmodf(((x as f32) / 32.0f32) / (p.w as f32), 0.5f32);
                                dist_t = fabsf(f - 0.25f32) / 0.25f32;
                            } else {
                                let temp = base_temperature_for_world_pos(pos_h, 0.25f32, 0.25f32, 0.0f32, world);
                                dist_t = 1.0f32 - clamp((temp - 25.0f32) / 5.0f32, 0.0f32, 1.0f32);
                            }
                            let vig;
                            if height > 0x20f || dist_t >= 0.25f32 {
                                vig = vt(TreeType::TREE_PINE, pos_h);
                            } else {
                                ty = TreeType::TREE_COCONUT;
                                vig = vt(TreeType::TREE_COCONUT, pos_h);
                            }
                            pine_value = pine_value * (vig - 0.2f32);
                            if pine_value > 0.0f32 && (ty == TreeType::TREE_COCONUT || !is_beach) {
                                let r = r01("worldgen::spawn_pine");
                                if r < pine_value * pine_value {
                                    self.tree_positions[xi] = ty.0 as i32;
                                    if ty == TreeType::TREE_COCONUT && dist < 1000 {
                                        food[5] += 1000 - dist;
                                    }
                                }
                            }
                        }
                    }
                    // CACTUS
                    if is_desert
                        && x > 3
                        && self.tree_positions[xi] == 0
                        && self.tree_positions[xi - 2] == 0
                        && self.tree_positions[xi - 3] == 0
                    {
                        let v = (nz(x, 0.205523) + tree_add as f64) as f32;
                        if v > 0.0f32 {
                            let r = r01("worldgen::spawn_cactus") * 2.0f32;
                            if r < v * v {
                                self.tree_positions[xi] = TreeType::TREE_CACTUS.0 as i32;
                                if dist < 1000 {
                                    food[2] += (1000 - dist) / 4;
                                }
                            }
                        }
                    }
                }
                // ---- plants
                if away_start4 && p.rules.plants != 0 {
                    if sand_fraction < 0.35f32 {
                        // FLAX
                        let mut v = (nz(x * 2 + 0x164, 0.8456) + c02 + plant_add as f64) as f32;
                        if v > 0.0f32 {
                            v = v * vp(PlantType::PLANT_FLAX, pos_h) - 0.1f32;
                            if v > 0.0f32 && r01("worldgen::spawn_flax") * 4.0f32 < v * v {
                                self.plant_positions[xi] = PlantType::PLANT_FLAX.0 as i32;
                            }
                        }
                    }
                    // (type, mul, offset, Y, add const, subtract-after-multiply form, rand multiplier, food)
                    #[derive(Clone, Copy)]
                    struct PRow {
                        ty: PlantType,
                        mul: i32,
                        off: i32,
                        y: f64,
                        add: f64,
                        sub: f32,
                        paren: bool,
                        rm: f32,
                        food: Option<(usize, i32)>,
                    }
                    let rows = [
                        PRow {
                            ty: PlantType::PLANT_SUNFLOWER,
                            mul: 3,
                            off: 2000,
                            y: 0.526,
                            add: c01,
                            sub: 0.2,
                            paren: false,
                            rm: 4.0,
                            food: None,
                        },
                        PRow {
                            ty: PlantType::PLANT_WHEAT,
                            mul: 3,
                            off: 0xf3c,
                            y: 0.816,
                            add: c02,
                            sub: 0.1,
                            paren: false,
                            rm: 4.0,
                            food: None,
                        },
                        PRow {
                            ty: PlantType::PLANT_TOMATO,
                            mul: 3,
                            off: 0x546,
                            y: 0.126,
                            add: c01,
                            sub: 0.2,
                            paren: false,
                            rm: 4.0,
                            food: Some((0x18, 1)),
                        },
                        PRow {
                            ty: PlantType::PLANT_CARROT,
                            mul: 3,
                            off: 0x1d4c,
                            y: 0.189,
                            add: c01,
                            sub: 0.2,
                            paren: false,
                            rm: 2.0,
                            food: Some((10, 4)),
                        },
                        PRow {
                            ty: PlantType::PLANT_CORN,
                            mul: 3,
                            off: 3000,
                            y: 0.126,
                            add: c01,
                            sub: 0.2,
                            paren: false,
                            rm: 8.0,
                            food: Some((9, 1)),
                        },
                        PRow {
                            ty: PlantType::PLANT_TULIP,
                            mul: 3,
                            off: 0x4ba,
                            y: 0.321,
                            add: c01,
                            sub: 0.2,
                            paren: false,
                            rm: 2.0,
                            food: None,
                        },
                        PRow {
                            ty: PlantType::PLANT_CHILLI,
                            mul: 3,
                            off: 0x2ef4,
                            y: 0.972,
                            add: c01,
                            sub: 0.2,
                            paren: true,
                            rm: 8.0,
                            food: Some((0xe, 1)),
                        },
                    ];
                    for r in rows.iter() {
                        if desert_or_beach || self.plant_positions[xi] != 0 {
                            continue;
                        }
                        let mut v = (nz(x * r.mul + r.off, r.y) + r.add + plant_add as f64) as f32;
                        if v > 0.0f32 {
                            let vig = vp(r.ty, pos_h);
                            v = if r.paren { v * (vig - r.sub) } else { v * vig - r.sub };
                            if v > 0.0f32 && r01("worldgen::spawn_plant") * r.rm < v * v {
                                self.plant_positions[xi] = r.ty.0 as i32;
                                if let Some((slot, div)) = r.food {
                                    if dist < 1000 {
                                        food[slot] += (1000 - dist) / div;
                                    }
                                }
                            }
                        }
                    }
                }
                // ---- land NPCs
                let land_npc = !desert_or_beach
                    && self.npc_positions[xi - 1] == 0
                    && away_start4
                    && self.rock_heights[xi] < self.dirt_heights[xi]
                    && (self.dirt_heights[xi] - self.dirt_heights[xi + 1]).abs() < 2
                    && (self.dirt_heights[xi] - self.dirt_heights[xi - 1]).abs() < 2;
                // DODO
                if land_npc {
                    let mut v = (nz(x, 0.34523) + c01) as f32;
                    if v > 0.0f32 {
                        v = v * (vt(TreeType::TREE_APPLE, pos_h) - 0.2f32);
                        if v > 0.0f32 && r01("worldgen::spawn_dodo") * 4.0f32 < v * v {
                            self.npc_positions[xi] = 1;
                            if dist < 1000 {
                                food[13] += (1000 - dist) / 2;
                                food[4] += (1000 - dist) / 8;
                            }
                        }
                    }
                }
                // SCORPION
                if is_desert
                    && x > 3
                    && self.npc_positions[xi] == 0
                    && self.npc_positions[xi - 2] == 0
                    && self.npc_positions[xi - 3] == 0
                    && (x < best.x - 0x100 || best.x + 0x100 < x)
                {
                    let v = nz(x, 0.205523) as f32;
                    if v > 0.0f32 && r01("worldgen::spawn_scorpion") * 1.5f32 < v * v {
                        self.npc_positions[xi] = 7;
                    }
                }
                // DONKEY
                if land_npc {
                    let mut v = (nz(x * 3 + 0x1d4c, 0.189) + c01) as f32;
                    if v > 0.0f32 {
                        v = v * (vp(PlantType::PLANT_CARROT, pos_h) - 0.2f32);
                        if v > 0.0f32 && r01("worldgen::spawn_donkey") * 4.0f32 < v * v {
                            self.npc_positions[xi] = 3;
                        }
                    }
                }
                // YAK
                if land_npc {
                    let mut v = (nz(x * 3 + 0xc23, 0.3829) + c01) as f32;
                    if v > 0.0f32 {
                        v = v * (vp(PlantType::PLANT_WHEAT, pos_h) - 0.2f32);
                        if v > 0.0f32 && r01("worldgen::spawn_yak") * 4.0f32 < v * v {
                            self.npc_positions[xi] = 8;
                        }
                    }
                }
            } else {
                // ================= WATER / NON-LAND COLUMN =================
                let m = if dirt < rock { rock } else { dirt };
                if m < 0x1ff {
                    if m < 0x1f1 {
                        // deep: sharks
                        if self.npc_positions[xi - 1] == 0 {
                            let v = nz(x * 2 + 0xb2, 0.68) as f32;
                            if v > 0.0f32 && r01("worldgen::spawn_shark") * 2.0f32 < v * v {
                                self.npc_positions[xi] = 5;
                            }
                        }
                    } else {
                        // shallow: kelp, clownfish
                        if self.plant_positions[xi - 1] == 0 {
                            let v = (nz(x * 2 + 0x91, 0.211) + c02) as f32;
                            if v > 0.0f32 && r01("worldgen::spawn_kelp") * 0.5f32 < v * v {
                                self.plant_positions[xi] = PlantType::PLANT_KELP.0 as i32;
                            }
                        }
                        if self.npc_positions[xi - 1] == 0 {
                            let v = (nz(x * 2 + 0x163, 0.511) + c01) as f32;
                            if v > 0.0f32 && r01("worldgen::spawn_clownfish") * 0.5f32 < v * v {
                                self.npc_positions[xi] = 4;
                            }
                        }
                    }
                }
            }
        }
        food
    }

    /// Step G: the highest non-cave column above 716.8.
    pub fn compute_highest_point(&mut self, p: &GenParams) {
        let wt = p.wt();
        for x in 0..wt {
            let xi = x as usize;
            let (r, d) = (self.rock_heights[xi], self.dirt_heights[xi]);
            let m = if r < d { d } else { r };
            if 716.8f64 < m as f64 && self.highest_point.y < m {
                let fo = self.fault_offset(p, x, m - 1);
                if !self.is_cave(p, x, m - 1, fo) {
                    self.highest_point.y = m;
                    self.highest_point.x = x;
                }
            }
        }
    }
}
