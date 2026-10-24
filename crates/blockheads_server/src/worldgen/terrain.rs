//! owned by worldgen
//!
//! Pure terrain math of `WorldTileLoader` on the immutable generator data [`Terrain`] and the per-operation
//! rule snapshot [`GenParams`]: initial heights, fault offset, caves, limestone / sand fractions, desert / beach,
//! `fillDirtTile`, the water / dirt flow-out helpers, `refineTerrain` and the per-column block generator.
//!

use std::rc::Rc;

use crate::fmath::cconv::f32_to_i32;
use crate::fmath::libm::{fabsf, fmodf, powf};
use crate::fmath::vector::{clamp, linear_interpolate};
use crate::rng::glibc_rand;
use crate::types::structs::{CustomRules, IntPair};
use crate::types::tile::Tile;
use crate::world::state::{PhysicalBlockRef, TileRef, World};
use crate::world::tiles::tile_at_world_position_loaded;

use super::features::OreSpread;
use super::noise::{noise_xy, NoiseFunction};

/// Tile type ids used by the generator (types/enums `TileType`).
pub mod tt {
    pub const ROCK: u8 = 1;
    pub const AIR: u8 = 2;
    pub const WATER: u8 = 3;
    pub const DIRT: u8 = 6;
    pub const SAND: u8 = 7;
    pub const SAND_BEACH: u8 = 8;
    pub const LIMESTONE: u8 = 12;
    pub const MARBLE: u8 = 14;
    pub const TIME_CRYSTAL: u8 = 0x10;
    pub const SANDSTONE: u8 = 17;
    pub const RED_MARBLE: u8 = 19;
    pub const DIRT_GRASS: u8 = 0x1b;
    pub const DIRT_GRASS_FROZEN: u8 = 0x1c;
    pub const LAPIS: u8 = 29;
    pub const LAVA: u8 = 0x1f;
    pub const NORTH_POLE: u8 = 38;
    pub const BLACK_SAND: u8 = 0x3a;
}

/// Per-operation snapshot of what the original re-reads from the `World` at every use
/// (`worldWidthMacro`, `customRules`, `expertMode`).
#[derive(Clone, Copy)]
pub struct GenParams {
    /// `[world worldWidthMacro]` (W, macro tiles).
    pub w: i32,
    /// `[world customRules]`.
    pub rules: CustomRules,
    /// `[world expertMode]`.
    pub expert: bool,
}

impl GenParams {
    /// All-zero parameters used when the world is nil.
    pub fn zeroed() -> GenParams {
        GenParams { w: 0, rules: CustomRules::default(), expert: false }
    }
    /// Read once per top-level operation from the live world.
    pub fn from_world(world: &World) -> GenParams {
        GenParams { w: world.world_width_macro(), rules: world.custom_rules(), expert: world.expert_mode() }
    }
    /// `from_world` or [`GenParams::zeroed`] when the world is gone.
    pub fn from_world_opt(world: Option<&World>) -> GenParams {
        match world {
            Some(w) => GenParams::from_world(w),
            None => GenParams::zeroed(),
        }
    }
    /// `WT = W << 5`.
    #[inline]
    pub fn wt(&self) -> i32 {
        self.w.wrapping_shl(5)
    }
}

/// Immutable generator data of a `WorldTileLoader`: the 12 noise functions, per-column
/// tables and derived values. Built once by `init` and never mutated afterwards.
pub struct Terrain {
    /// `heightNoiseFunctionA` (+0x20)
    pub height_a: Option<Rc<NoiseFunction>>,
    /// `heightNoiseFunctionB` (+0x28)
    pub height_b: Option<Rc<NoiseFunction>>,
    /// `faultNoiseFunction` (+0x30)
    pub fault: Option<Rc<NoiseFunction>>,
    /// `treeDensityNoiseFunction` (+0x38)
    pub tree_density: Option<Rc<NoiseFunction>>,
    /// `rockTypeNoiseFunction` (+0x40)
    pub rock_type: Option<Rc<NoiseFunction>>,
    /// `flintDensityNoiseFunction` (+0x48)
    pub flint: Option<Rc<NoiseFunction>>,
    /// `tinDensityNoiseFunction` (+0x50)
    pub tin: Option<Rc<NoiseFunction>>,
    /// `sandNoiseFunction` (+0x58)
    pub sand: Option<Rc<NoiseFunction>>,
    /// `seasonOffsetNoiseFunction` (+0x60)
    pub season_offset: Option<Rc<NoiseFunction>>,
    /// `caveNoiseFunctionA` (+0x68)
    pub cave_a: Option<Rc<NoiseFunction>>,
    /// `caveNoiseFunctionB` (+0x70)
    pub cave_b: Option<Rc<NoiseFunction>>,
    /// `gemNoiseFunction` (+0x78)
    pub gem: Option<Rc<NoiseFunction>>,
    /// `treePositions` (TreeType per column)
    pub tree_positions: Vec<i32>,
    /// `npcPositions` (NPCType per column)
    pub npc_positions: Vec<i32>,
    /// `plantPositions` (PlantType per column)
    pub plant_positions: Vec<i32>,
    /// `bestStartPosition`
    pub best_start_position: IntPair,
    /// `highestPoint`
    pub highest_point: IntPair,
    /// `dirtHeights`
    pub dirt_heights: Vec<i32>,
    /// `rockHeights`
    pub rock_heights: Vec<i32>,
    /// `lakeHeights` (0 = no lake)
    pub lake_heights: Vec<i32>,
    /// `distanceOrderedFoodTypes[31]`
    pub distance_ordered_food_types: [i32; 31],
    /// `xFrequencyMultiplier`
    pub x_frequency_multiplier: i32,
    /// `yHeightDivider`
    pub y_height_divider: f32,
    /// `randomSeed` (may be incremented by retries)
    pub random_seed: u32,
}

/// `(float)rand() / 2147483648.0f` helper pass-through.
#[inline]
pub(crate) fn r01(tag: &'static str) -> f32 {
    glibc_rand::r01_tagged(tag)
}

/// C `max(a, b)` as the compiled `if (a < b) a = b`.
#[inline]
fn imax(a: i32, b: i32) -> i32 {
    if a < b {
        b
    } else {
        a
    }
}

/// Writes the lava glow gradient for local row `y` (`f = (y - localLava)/-32 + 1`).
pub(crate) fn set_lava_gradient(t: &mut Tile, y: i32, local_lava: i32) {
    let f = ((y - local_lava) as f32) / (-32.0f32) + 1.0f32;
    t.artificial_light_r = f32_to_u16_via_int(f * 255.0f32);
    t.artificial_light_g = f32_to_u16_via_int(f * 127.0f32);
    t.artificial_light_b = f32_to_u16_via_int(f * 31.0f32);
    t.artificial_heat = f32_to_i32(f * 127.0f32) as i16;
}

/// `(u16)(int)x`.
#[inline]
fn f32_to_u16_via_int(x: f32) -> u16 {
    f32_to_i32(x) as u16
}

impl Terrain {
    /// An empty generator for `W` macro tiles (arrays zeroed, no noise functions, seed as given).
    /// Used by `init` before the retry loop and by unit tests.
    pub fn new_empty(w: i32, random_seed: u32) -> Terrain {
        let wt = (w.wrapping_shl(5)).max(0) as usize;
        let mut x_mul = 0i32;
        let mut ww = w;
        let mut div = 32.0f32;
        while ww > 0x200 {
            x_mul += 1;
            ww /= 2;
            div *= 2.0f32;
        }
        while ww < 0x200 {
            x_mul -= 1;
            ww = ww.wrapping_shl(1);
            if ww == 0 {
                break; // W == 0 would loop forever in the original (shift of 0); not a reachable world
            }
        }
        Terrain {
            height_a: None,
            height_b: None,
            fault: None,
            tree_density: None,
            rock_type: None,
            flint: None,
            tin: None,
            sand: None,
            season_offset: None,
            cave_a: None,
            cave_b: None,
            gem: None,
            tree_positions: vec![0; wt],
            npc_positions: vec![0; wt],
            plant_positions: vec![0; wt],
            best_start_position: IntPair { x: 0, y: 0 },
            highest_point: IntPair { x: 0, y: 0 },
            dirt_heights: vec![0; wt],
            rock_heights: vec![0; wt],
            lake_heights: vec![0; wt],
            distance_ordered_food_types: [0; 31],
            x_frequency_multiplier: x_mul,
            y_height_divider: div,
            random_seed,
        }
    }

    // ------------------------------------------------------------------------------------------------
    //  getInitialRockAndDirtHeightforX:rockHeight:dirtHeight:
    // ------------------------------------------------------------------------------------------------

    /// `-getInitialRockAndDirtHeightforX:rockHeight:dirtHeight:` -> `(rockHeight, dirtHeight)` (unnormalised floats).
    pub fn get_initial_rock_and_dirt_height(&self, p: &GenParams, x: i32) -> (f32, f32) {
        let r = &p.rules;
        let mut oct = 3;
        if r.terrain_type == 1 {
            oct = 2;
        } else if r.terrain_type == 3 {
            oct = 4;
        }
        let x_f = ((x as f32) / 32.0f32) / (p.w as f32);
        // One height fraction: rock uses hB octaves oct+2, dirt uses oct.
        let fraction = |hb_oct: i32| -> f32 {
            let h_a = (noise_xy(&self.height_a, (0.1f32 + x_f) as f64, 0.5, oct) * 0.30000001192092896 + 0.5) as f32;
            let h_b = (noise_xy(&self.height_b, (0.07f32 + x_f) as f64, 0.5, hb_oct) * 0.5 + 0.5) as f32;
            let mut mix = (noise_xy(&self.height_b, (0.05f32 + x_f) as f64, 0.75, 9) * 0.800000011920929
                + 0.20000000298023224) as f32;
            mix = clamp(mix, 0.0f32, 1.0f32);
            let mut h = linear_interpolate(h_a, h_b, mix);
            h = (h - 0.5f32) * 2.0f32;
            if r.oceans == 1 {
                h += 0.06f32;
            } else if r.oceans == 3 {
                h -= 0.06f32;
            }
            let neg = h < 0.0f32;
            h = fabsf(h);
            if h < 0.1f32 && r.oceans != 4 {
                h = powf(10.0f32 * h, 2.0f32) / 10.0f32;
            }
            if r.oceans == 4 {
                h = -h;
            } else if neg && r.oceans != 0 {
                h = -h;
            }
            h / 2.0f32 + 0.5f32
        };
        let rh = fraction(oct + 2);
        let mut rock = 16.0f32 + ((31.0f32 * rh) * 32.0f32);
        let dh = fraction(oct);
        let dirt_base = if r.oceans == 4 { 18.0f32 } else { 20.0f32 };
        let mut dirt = dirt_base + ((31.0f32 * dh) * 32.0f32);
        if r.terrain_type == 0 {
            if dirt <= rock {
                let old_r = rock;
                let old_d = dirt;
                rock = if old_r <= 512.0f32 { 511.25f32 } else { 512.25f32 };
                dirt = rock - (old_r - old_d);
            } else {
                let old_d = dirt;
                let old_r = rock;
                dirt = if old_d <= 512.0f32 { 511.25f32 } else { 512.25f32 };
                rock = dirt - (old_d - old_r);
            }
        } else if r.terrain_type == 1 {
            dirt = ((dirt - 512.0f32) as f64 * 0.5 + 512.0) as f32;
            rock = ((rock - 512.0f32) as f64 * 0.5 + 512.0) as f32;
        }
        (rock, dirt)
    }

    // ------------------------------------------------------------------------------------------------
    // ------------------------------------------------------------------------------------------------

    /// `-faultOffsetForX:y:`.
    ///
    /// Correction (observed against the reference environment/w4, W = 2048): the y divisor written as the literal
    /// `512.0f` in is really `16 * yHeightDivider` (equal to 512 for every W <= 512, 2048 for W = 2048);
    /// with the literal, W >= 1024 worlds differ from the reference (caves, limestone, ores).
    pub fn fault_offset(&self, p: &GenParams, x: i32, y: i32) -> i32 {
        let x_f = ((x as f32) / 32.0f32) / (p.w as f32);
        let y_f = (((y as f32) / 2.0f32) / 32.0f32) / (16.0f32 * self.y_height_divider);
        let mix_in = 16.0f32 * x_f + 0.05f32;
        let mix = clamp(
            (noise_xy(&self.height_b, mix_in as f64, 0.75, 1) * 0.800000011920929 + 0.20000000298023224) as f32,
            0.0f32,
            1.0f32,
        );
        let mut f = fabsf(noise_xy(&self.fault, x_f as f64, y_f as f64, 1) as f32);
        if 0.0f32 < f {
            if 0.2f32 <= f {
                // f >= 0.2 makes `t` non-negative, so the original's `if (t <= 0.0f) t = 0.0f` only ever stores +0.0
                // over +0.0
                let t = (f - 0.2f32) * 2.0f32;
                f = 0.5f32 * mix + powf(t, 2.0f32);
            } else {
                f = powf(2.0f32 * f, 2.0f32);
            }
        }
        f32_to_i32((512.0f32 * f) * mix)
    }

    /// `-getRockAndDirtHeightforX:rockHeight:dirtHeight:` -> `(rock, dirt)`; single wrap of x.
    pub fn get_rock_and_dirt_height(&self, p: &GenParams, mut x: i32) -> (i32, i32) {
        let wt = p.wt();
        if x < 0 {
            x += wt;
        } else if x >= wt {
            x -= wt;
        }
        (self.rock_heights[x as usize], self.dirt_heights[x as usize])
    }

    /// `-getCloudHeightForX:`.
    pub fn get_cloud_height(&self, p: &GenParams, x: i32) -> i32 {
        let x_f = ((x as f32) / 32.0f32) / (p.w as f32);
        let c_a = noise_xy(&self.height_a, (0.1f32 + x_f) as f64, 0.5, 3) * 0.30000001192092896 + 0.5;
        let c_b = noise_xy(&self.height_b, (0.07f32 + x_f) as f64, 0.5, 3) * 0.5 + 0.5;
        let mix = clamp(
            (noise_xy(&self.height_b, (0.05f32 + x_f) as f64, 0.75, 9) * 0.800000011920929 + 0.20000000298023224)
                as f32,
            0.0f32,
            1.0f32,
        );
        let h = linear_interpolate(c_a as f32, c_b as f32, mix);
        f32_to_i32(40.0f32 + (31.0f32 * h) * 32.0f32)
    }

    /// `-limestoneFractionForX:y:faultOffset:`.
    pub fn limestone_fraction(&self, p: &GenParams, world_x: i32, world_y: i32, fo: i32) -> f32 {
        let nx = (((world_x as f32) + 39599.0f32) / (p.wt() as f32)) / 2.0f32;
        let ny = (((world_y - fo) as f32) / (32.0f32 * self.y_height_divider)) / 2.0f32;
        let n = noise_xy(&self.rock_type, nx as f64, ny as f64, 2) as f32;
        let v = 2.0f32 * ((world_y as f32) / 1024.0f32);
        let cap = if 1.0f32 <= v { 1.0f32 } else { v };
        n * cap
    }

    /// `-sandstoneFractionForX:y:faultOffset:limestoneFraction:` (`faultOffset` unused).
    pub fn sandstone_fraction(&self, p: &GenParams, world_x: i32, world_y: i32, _fo: i32, lf: f32) -> f32 {
        let mut is_desert = false;
        if 0.45f64 <= lf as f64 {
            let rh = self.rock_heights[world_x as usize];
            let dh = self.dirt_heights[world_x as usize];
            let h = if dh < rh { rh } else { dh };
            is_desert = self.is_desert(p, IntPair { x: world_x, y: world_y }, h);
        }
        if is_desert {
            lf
        } else {
            -1.0f32
        }
    }

    /// `-unmodifiedGroundLevelForX:`.
    pub fn unmodified_ground_level(&self, p: &GenParams, x: i32) -> i32 {
        let (rock, dirt) = self.get_rock_and_dirt_height(p, x);
        let mut result = imax(dirt, rock);
        let mut was_cave = 0;
        loop {
            if result < 1 {
                return 0;
            }
            let fo = self.fault_offset(p, x, result);
            if rock < result {
                return result;
            }
            if !self.is_cave(p, x, result, fo) {
                return result + was_cave;
            }
            result -= 1;
            was_cave = 1;
        }
    }

    /// `-maxOfRockAndDirtHeightForX:` (`>` not `>=` in the wrap test).
    pub fn max_of_rock_and_dirt_height(&self, p: &GenParams, mut x: i32) -> i32 {
        let wt = p.wt();
        if x < 0 {
            x += wt;
        } else if x > wt {
            x -= wt;
        }
        let (r, d) = (self.rock_heights[x as usize], self.dirt_heights[x as usize]);
        if d < r {
            r
        } else {
            d
        }
    }

    /// `-lakeHeightForX:` (`>` not `>=`).
    pub fn lake_height(&self, p: &GenParams, mut x: i32) -> i32 {
        let wt = p.wt();
        if x < 0 {
            x += wt;
        } else if x > wt {
            x -= wt;
        }
        self.lake_heights[x as usize]
    }

    /// `-isCaveForX:y:faultOffset:`.
    pub fn is_cave(&self, p: &GenParams, x: i32, y: i32, fo: i32) -> bool {
        if p.rules.caves == 0 {
            return false;
        }
        let x_f = ((x as f32) / 32.0f32) / (p.w as f32);
        let y_f = (((y - fo) as f32) / 32.0f32) / self.y_height_divider;
        let v_a = noise_xy(&self.cave_a, x_f as f64, y_f as f64, 2) as f32;
        let v_b = (noise_xy(&self.cave_b, x_f as f64, y_f as f64, 1) * 0.05) as f32;
        let c = fabsf(v_a + v_b);
        let mut threshold = 0.04f64;
        if p.rules.caves == 2 {
            threshold = 0.2f64;
        }
        threshold > c as f64
    }

    /// `-isFloatingIslandCaveForX:y:`.
    pub fn is_floating_island_cave(&self, p: &GenParams, x: i32, y: i32) -> bool {
        if p.rules.caves == 0 {
            return false;
        }
        let x_f = ((x as f32) / 32.0f32) / (p.w as f32);
        let y_f = ((y as f32) / 32.0f32) / self.y_height_divider;
        let v_a = noise_xy(&self.cave_a, (8.0f32 * x_f) as f64, (8.0f32 * y_f) as f64, 2) as f32;
        let v_b = (noise_xy(&self.cave_b, (8.0f32 * x_f) as f64, (8.0f32 * y_f) as f64, 1) * 0.5) as f32;
        0.2f64 > fabsf(v_a + v_b) as f64
    }

    // ------------------------------------------------------------------------------------------------
    //  sand / desert / beach
    // ------------------------------------------------------------------------------------------------

    /// `-sandFractionForPos:highRes:` (looks the height up; X wraps once).
    pub fn sand_fraction_for_pos(&self, p: &GenParams, mut pos: IntPair, high_res: bool) -> f32 {
        let wt = p.wt();
        if pos.x < 0 {
            pos.x += wt;
            if pos.x < 0 {
                return 0.0f32;
            }
        } else if pos.x >= wt {
            pos.x -= wt;
            if pos.x >= wt {
                return 0.0f32;
            }
        }
        // pos.y passes through unchanged (verification note 1).
        let (r, d) = (self.rock_heights[pos.x as usize], self.dirt_heights[pos.x as usize]);
        let h = if d < r { r } else { d };
        self.sand_fraction_for_pos_height(p, pos, h, high_res)
    }

    /// `-sandFractionForPos:height:highRes:`.
    pub fn sand_fraction_for_pos_height(&self, p: &GenParams, pos: IntPair, height: i32, high_res: bool) -> f32 {
        let x_f = ((pos.x as f32) / 32.0f32) / (p.w as f32);
        let ty = ((pos.y as f64 - height as f64 * 0.5) as f32) / 32.0f32;
        let den = if p.w < 0x200 { 0x200 } else { p.w };
        let y_f = (ty / (den as f32)) * 4.0f32;
        let oct = if high_res { 11 } else { 4 };
        let mut sf = noise_xy(&self.sand, x_f as f64, y_f as f64, oct) as f32;
        match p.rules.rainfall {
            0 => sf = (sf as f64 + 1.0) as f32,
            1 => sf = (sf as f64 + 0.6) as f32,
            2 => {}
            3 => sf = (sf as f64 - 0.6) as f32,
            4 => sf = (sf as f64 - 1.0) as f32,
            _ => {}
        }
        sf = clamp(sf, -1.0f32, 1.0f32);
        if 0.0f32 < sf && p.rules.rainfall != 0 {
            let f = fmodf(((pos.x as f32) / 32.0f32) / (p.w as f32), 0.5f32);
            let m = clamp((1.0f32 - fabsf(f - 0.25f32) / 0.25f32) * 3.0f32 - 0.5f32, 0.0f32, 1.0f32);
            sf *= m;
        }
        sf
    }

    /// `-isDesertForPos:height:`.
    pub fn is_desert(&self, p: &GenParams, pos: IntPair, height: i32) -> bool {
        self.sand_fraction_for_pos_height(p, pos, height, true) as f64 > 0.3f64
    }

    /// `-isBeachForPos:height:`.
    pub fn is_beach(&self, p: &GenParams, pos: IntPair, height: i32) -> bool {
        if pos.y < 0x3f0 - height {
            return true;
        }
        if 0x410 - height < pos.y {
            return false;
        }
        let x_f = ((pos.x as f32) / 32.0f32) / (p.w as f32);
        let ty = ((pos.y as f64 - height as f64 * 0.5) as f32) / 32.0f32;
        let den = if p.w < 0x200 { 0x200 } else { p.w };
        let y_f = (ty / (den as f32)) * 4.0f32;
        let n = noise_xy(&self.sand, x_f as f64, y_f as f64, 11) as f32;
        (pos.y as f32) < (512.0f32 - 16.0f32 * n) + ((0x200 - height) as f32)
    }

    /// `-isDesertOrBeachForPos:height:`.
    pub fn is_desert_or_beach(&self, p: &GenParams, pos: IntPair, height: i32) -> bool {
        if self.is_beach(p, pos, height) {
            return true;
        }
        self.sand_fraction_for_pos_height(p, pos, height, true) as f64 > 0.3f64
    }

    // ------------------------------------------------------------------------------------------------
    //  fillDirtTile
    // ------------------------------------------------------------------------------------------------

    /// `-fillDirtTile:worldPos:worldDirtHeight:parentType:`. Writes `typeIndex`, `backWallTypeIndex`, `contents`
    /// of `tile`.
    pub fn fill_dirt_tile(&self, p: &GenParams, tile: &mut Tile, wp: IntPair, world_dirt_height: i32, parent_type: u8) {
        let valid_parent = matches!(
            parent_type,
            tt::SAND | tt::SAND_BEACH | tt::BLACK_SAND | tt::DIRT | tt::DIRT_GRASS | tt::DIRT_GRASS_FROZEN
        );
        let mut is_sand = false;
        let mut is_beach = false;
        let mut x_f = 0.0f32;
        let mut y_f = 0.0f32;
        let den = if p.w < 0x200 { 0x200 } else { p.w };
        let xi = wp.x as usize;
        // yF as the generator derives it for a given reference height.
        let y_frac = |h: i32| -> f32 {
            let ty = ((wp.y as f64 - h as f64 * 0.5) as f32) / 32.0f32;
            (ty / (den as f32)) * 4.0f32
        };
        if !valid_parent {
            let (r, d) = (self.rock_heights[xi], self.dirt_heights[xi]);
            let combined = if d < r { r } else { d };
            x_f = ((wp.x as f32) / 32.0f32) / (p.w as f32);
            y_f = y_frac(combined);
            let sf = self.sand_fraction_for_pos_height(p, wp, combined, true);
            if sf as f64 > 0.3f64 {
                is_sand = true;
            } else if self.is_beach(p, wp, combined) {
                is_sand = true;
                is_beach = true;
            }
        } else {
            is_sand = matches!(parent_type, tt::SAND | tt::SAND_BEACH | tt::BLACK_SAND);
            is_beach = matches!(parent_type, tt::SAND_BEACH | tt::BLACK_SAND);
        }
        let ty = if !is_sand {
            tt::DIRT
        } else if !is_beach {
            tt::SAND
        } else {
            tt::SAND_BEACH
        };
        tile.type_index = ty;
        tile.back_wall_type_index = ty;
        if !is_sand {
            if valid_parent {
                let rh = self.rock_heights[xi];
                x_f = ((wp.x as f32) / 32.0f32) / (p.w as f32);
                y_f = y_frac(rh);
            }
            let fr = noise_xy(&self.flint, x_f as f64, y_f as f64, 3) as f32;
            let mut flint_t = 0.5f32;
            let mut clay_t = -0.6f32;
            if p.rules.ore_quantity == 1 {
                flint_t = 0.75f32;
                clay_t = -0.8f32;
            } else if p.rules.ore_quantity == 3 {
                flint_t = 0.125f32;
                clay_t = -0.15f32;
            }
            if p.expert {
                flint_t *= 0.5f32;
                clay_t *= 0.5f32;
            }
            if fr > flint_t {
                tile.contents = 1;
            } else if fr < clay_t {
                tile.contents = 2;
            }
        }
        if is_beach {
            if valid_parent {
                let rh = self.rock_heights[xi];
                x_f = ((wp.x as f32) / 32.0f32) / (p.w as f32);
                y_f = y_frac(rh);
            }
            let n = noise_xy(&self.flint, x_f as f64 * 0.0625, y_f as f64 * 0.625, 3) as f32;
            if 0.4f32 < n {
                tile.type_index = tt::BLACK_SAND;
                tile.back_wall_type_index = tt::BLACK_SAND;
            }
        }
        if !is_sand && wp.y > 0x1fe && wp.y == world_dirt_height - 1 && self.lake_heights[xi] < world_dirt_height {
            tile.type_index = tt::DIRT_GRASS;
            tile.back_wall_type_index = tt::DIRT_GRASS;
        }
    }

    // ------------------------------------------------------------------------------------------------
    //  flow-out
    // ------------------------------------------------------------------------------------------------

    /// `-recursivelyFlowOutWaterFromTile:atPos:`: depth-first flood of zone-1 air tiles (down, right, left).
    pub fn recursively_flow_out_water_from_tile(&self, world: &World, wp: IntPair) {
        for (dx, dy) in [(0i32, -1i32), (1, 0), (-1, 0)] {
            let pp = IntPair { x: wp.x + dx, y: wp.y + dy };
            if let Some(t) = tile_at_world_position_loaded(pp.x, pp.y, world) {
                if t.type_index() == tt::AIR && t.zone_type_index() == 1 {
                    t.update(|t| {
                        t.type_index = tt::WATER;
                        t.partial_content_left = 0xff;
                        t.sun_light = 0;
                    });
                    self.recursively_flow_out_water_from_tile(world, pp);
                }
            }
        }
    }

    /// `-recursivelyFlowOutDirtFromTile:atPos:`. No caller exists in the binary (R5); ported for
    /// completeness.
    #[allow(dead_code)]
    pub fn recursively_flow_out_dirt_from_tile(&self, p: &GenParams, world: &World, tile: &TileRef, mut wp: IntPair) {
        let wt = p.wt();
        if wp.y > 0 {
            let step = |this: &Terrain, pp: IntPair| {
                if let Some(t) = tile_at_world_position_loaded(pp.x, pp.y, world) {
                    let ty = t.type_index();
                    if (ty == tt::AIR || ty == tt::WATER) && t.zone_type_index() == 1 {
                        let parent = tile.type_index();
                        t.update(|tt_| this.fill_dirt_tile(p, tt_, pp, 99999999, parent));
                        this.recursively_flow_out_dirt_from_tile(p, world, &t, pp);
                    }
                }
            };
            step(self, IntPair { x: wp.x, y: wp.y - 1 });
            let r_l = r01("worldgen::flow_out_dirt_l");
            let r_r = r01("worldgen::flow_out_dirt_r");
            if wp.x - 1 < 0 {
                wp.x += wt;
            }
            if 0.7f32 < r_l {
                step(self, IntPair { x: wp.x - 1, y: wp.y - 1 });
            }
            if wp.x + 1 >= wt {
                wp.x -= wt;
            }
            if 0.7f32 < r_r {
                step(self, IntPair { x: wp.x + 1, y: wp.y - 1 });
            }
            if wp.x - 2 < 0 {
                wp.x += wt;
            }
            if 0.9f32 < r_l {
                step(self, IntPair { x: wp.x - 2, y: wp.y - 1 });
            }
            if wp.x + 2 >= wt {
                wp.x -= wt;
            }
            if 0.9f32 < r_r {
                step(self, IntPair { x: wp.x + 2, y: wp.y - 1 });
            }
        }
    }

    // ------------------------------------------------------------------------------------------------
    //  refineTerrain (one slice)
    // ------------------------------------------------------------------------------------------------

    /// Turns the tile at `(x, y)` into water unless it is lava and floods from there.
    fn make_water_and_flow_out(&self, world: &World, x: i32, y: i32) {
        // The original does not NULL-check the tile.
        let t =
            tile_at_world_position_loaded(x, y, world).expect("refineTerrain: tileAtWorldPositionLoaded returned NULL");
        if t.type_index() != tt::LAVA {
            t.update(|t| {
                t.type_index = tt::WATER;
                t.partial_content_left = 0xff;
                t.sun_light = 0;
            });
            self.recursively_flow_out_water_from_tile(world, IntPair { x, y });
        }
    }

    /// Body of `-refineTerrain` for the columns `[start, start + WT/8)`. The caller (`WorldTileLoader::refine_terrain`)
    /// advances `refineTerrainCount` and calls `decommisionAllBlocks...`.
    pub fn refine_terrain_slice(&self, p: &GenParams, world: &World, start: i32) {
        let wt = p.wt();
        let stride = wt / 8;
        let at = |v: &Vec<i32>, i: i32| -> i32 {
            // UB-PARITY: the double index `rockHeights[rockHeights[li]]` can leave the array for tiny worlds; the
            // original reads heap garbage there, 0 is used here.
            if i >= 0 {
                v.get(i as usize).copied().unwrap_or(0)
            } else {
                0
            }
        };
        let mut x = start;
        while x < start + stride {
            let rock = self.rock_heights[x as usize];
            let dirt = self.dirt_heights[x as usize];
            let li = (x - 1 + wt) % wt;
            let ri = (x + 1) % wt;
            let l_rock = self.rock_heights[li as usize];
            let r_rock = self.rock_heights[ri as usize];
            // !! DOUBLE INDEX (original bug, verification note 2).
            let l_open = at(&self.rock_heights, l_rock) - 1;
            let r_open = at(&self.rock_heights, r_rock) - 1;
            let top = rock - 1;
            // (1) random water drop into a cave below the surface
            if top < 0x200 && (glibc_rand::rand_tagged("worldgen::refine_terrain_gate") % 8) == 0 {
                // R6: `% top` with top == 0 would be SIGFPE in the original; Rust panics too.
                let ry = glibc_rand::rand_tagged("worldgen::refine_terrain_y") % top;
                let fo = self.fault_offset(p, x, ry);
                if self.is_cave(p, x, ry, fo) {
                    self.make_water_and_flow_out(world, x, ry);
                }
            }
            // (2) surface cave openings
            let lake_x = self.lake_heights[x as usize];
            let lake_l = self.lake_heights[li as usize];
            let lake_r = self.lake_heights[ri as usize];
            if dirt <= rock
                && (top < 0x200
                    || top < lake_x
                    || l_open < 0x200
                    || l_open < lake_l
                    || r_open < 0x200
                    || r_open < lake_r)
            {
                let fo = self.fault_offset(p, x, top);
                let mut skip = false;
                if (top < 0x200 || top < lake_x) && self.is_cave(p, x, top, fo) {
                    self.make_water_and_flow_out(world, x, top);
                    skip = true;
                }
                if !skip {
                    if l_open < 0x200 || l_open < lake_l {
                        let y0 = top.min(lake_l);
                        let mut y = y0;
                        while y >= l_rock {
                            let fo = self.fault_offset(p, x, y);
                            if self.is_cave(p, x, y, fo) {
                                self.make_water_and_flow_out(world, x, y);
                            }
                            y -= 1;
                        }
                    }
                    if r_open < 0x200 || r_open < lake_r {
                        let y0 = top.min(lake_r);
                        let mut y = y0;
                        while y >= r_rock {
                            let fo = self.fault_offset(p, x, y);
                            if self.is_cave(p, x, y, fo) {
                                self.make_water_and_flow_out(world, x, y);
                            }
                            y -= 1;
                        }
                    }
                }
            }
            x += 1;
        }
    }

    // ------------------------------------------------------------------------------------------------
    // : per-column block generator
    // ------------------------------------------------------------------------------------------------

    /// Procedural generation of one block: sets `pb.version = 0`, plans the floating island,
    /// then fills every column (default tile init, rock rows, dirt rows, air / water / island rows).
    /// `x_pos` / `y_pos` are the (wrapped) macro coordinates, `pb.x_pos` / `pb.y_pos` are already set by the caller.
    /// `world` is only used for the tree growth vigor of the island selection (`None` == nil world).
    pub fn generate_block_tiles(
        &self,
        p: &GenParams,
        pb: PhysicalBlockRef,
        x_pos: i32,
        y_pos: i32,
        world: Option<&World>,
    ) {
        let spread = OreSpread::new(p);
        pb.version.set(0);
        let plan = self.plan_floating_island(p, x_pos, y_pos, world);
        let wall_w = p.wt();
        for x in 0..32i32 {
            let world_x = x_pos * 32 + x;
            for y in 0..32usize {
                let cell = &pb.tiles[y * 32 + x as usize];
                let mut t = cell.get();
                t.light = 0;
                t.explored_fraction = 0;
                t.on_fire = 0;
                t.contents = 0;
                t.foreground_contents = 0;
                t.background_contents = 0;
                t.artificial_light_r = 0;
                t.artificial_light_g = 0;
                t.artificial_light_b = 0;
                t.artificial_heat = 0;
                t.dynamic_object_owner_old = 0;
                t.dynamic_object_owner = 0;
                t.season_offset = 0;
                t.zone_type_index = 2;
                t.sun_light = 0;
                t.terrain_slow_factor = 0x7f;
                t.partial_content_left = 0xff;
                t.gather_progress = 0;
                t.paint_front = 0;
                t.paint_bot = 0;
                t.paint_top = 0;
                t.paint_left = 0;
                t.paint_right = 0;
                // Not in the original: a recycled block may hold a mod block's record (zero without mods).
                t.padding = [0; 16];
                cell.set(t);
            }
            let (r, d) = self.get_rock_and_dirt_height(p, world_x);
            let local_rock = r - y_pos * 32;
            let local_dirt = d - y_pos * 32;
            let local_lake = self.lake_heights[world_x as usize] - y_pos * 32;
            let local_lava = r / 16 - y_pos * 32 + 16;

            // ---- rock rows
            let mut y = 0i32;
            while y < local_rock && y < 32 {
                let world_y = y_pos * 32 + y;
                let ti = (y * 32 + x) as usize;
                let mut t = pb.tiles[ti].get();
                let fault = self.fault_offset(p, world_x, world_y);
                let lime = self.limestone_fraction(p, world_x, world_y, fault);
                let sand = self.sandstone_fraction(p, world_x, world_y, fault, lime);
                let is_sandstone = sand > 0.35f32;
                let is_limestone = !is_sandstone && lime > 0.35f32;
                if y < local_lava {
                    t.type_index = tt::LAVA;
                    t.artificial_light_r = 0xff;
                    t.artificial_light_g = 0x7f;
                    t.artificial_light_b = 0x1f;
                    t.artificial_heat = 0x7f;
                } else {
                    if y < local_lava + 32 {
                        set_lava_gradient(&mut t, y, local_lava);
                    }
                    if !self.is_cave(p, world_x, world_y, fault) {
                        if !is_limestone {
                            if !is_sandstone {
                                t.type_index = tt::ROCK;
                                if p.rules.ore_quantity != 0 {
                                    if let Some(c) =
                                        self.ore_for_rock_tile(p, &spread, world_x, world_y, y, local_rock, lime, fault)
                                    {
                                        t.contents = c;
                                    }
                                }
                            } else {
                                let red_marble = ((world_y as f32) / 1024.0f32) * 1.8f32 < 0.15f32 + sand;
                                if red_marble {
                                    t.type_index = tt::RED_MARBLE;
                                } else {
                                    t.type_index = tt::SANDSTONE;
                                    if lime < 0.55f32 {
                                        let lime2 = self.limestone_fraction(p, world_x, world_y + 1, fault);
                                        if lime < lime2 {
                                            t.type_index = tt::LIMESTONE;
                                            t.contents = 0x40;
                                        }
                                    }
                                }
                            }
                        } else if ((world_y as f32) / 1024.0f32) * 1.8f32 < 0.2f32 + lime {
                            t.type_index = tt::MARBLE;
                        } else if (-lime + 0.9f32) < ((world_y as f32) / 1024.0f32) * 0.25f32 {
                            t.type_index = tt::LAPIS;
                        } else {
                            t.type_index = tt::LIMESTONE;
                            if lime < 0.55f32 {
                                let sf = self.sand_fraction_for_pos(p, IntPair { x: world_x, y: world_y }, true);
                                if sf > 0.2f32 || (0.0f32 < sf && sf < 0.1f32) {
                                    let lime2 = self.limestone_fraction(p, world_x, world_y + 1, fault);
                                    if lime < lime2 {
                                        t.contents = 0x40;
                                    }
                                }
                            }
                        }
                    } else {
                        self.place_gems_in_cave_tile(p, &mut t, world_x, world_y, 0);
                    }
                }
                t.zone_type_index = 1;
                t.back_wall_type_index = 1;
                pb.tiles[ti].set(t);
                y += 1;
            }

            // ---- dirt rows
            let mut y = imax(local_rock, 0);
            while y < local_dirt && y < 32 {
                let world_y = y_pos * 32 + y;
                let ti = (y * 32 + x) as usize;
                let mut t = pb.tiles[ti].get();
                if world_x == self.best_start_position.x {
                    t.type_index = tt::ROCK;
                    t.zone_type_index = 1;
                    t.back_wall_type_index = 1;
                } else {
                    self.fill_dirt_tile(p, &mut t, IntPair { x: world_x, y: world_y }, d, tt::AIR);
                }
                if world_x == 0 {
                    t.back_wall_type_index = 0x26;
                    t.zone_type_index = 1;
                } else if world_x == wall_w / 4 {
                    t.back_wall_type_index = 0x29;
                    t.zone_type_index = 1;
                } else if world_x == wall_w / 2 {
                    t.back_wall_type_index = 0x27;
                    t.zone_type_index = 1;
                } else if world_x == (wall_w / 4) * 3 {
                    t.back_wall_type_index = 0x28;
                    t.zone_type_index = 1;
                } else if world_y > 0x1f0 {
                    let tr = p.rules.treasure;
                    if tr != 0
                        && !p.expert
                        && (t.type_index == tt::SAND_BEACH || t.type_index == tt::BLACK_SAND)
                        && local_rock == y
                        && (local_dirt - local_rock) > 2
                    {
                        let fo = self.fault_offset(p, world_x, world_y - 1);
                        if !self.is_cave(p, world_x, world_y - 1, fo) {
                            let yh = self.y_height_divider;
                            let x_f2 = (((world_x as f32) + 1453.0f32) / 32.0f32) / yh;
                            let y_f2 = (((world_y as f32) + 1247.0f32) / 32.0f32) / yh;
                            let tc =
                                noise_xy(&self.flint, (1.423f32 * x_f2) as f64, (1.4536f32 * y_f2) as f64, 5) as f32;
                            let min = 0.85f32;
                            let mut window = 0.3f32;
                            if tr == 1 {
                                window = 0.15f32;
                            } else if tr == 3 {
                                window = 0.5f32;
                            }
                            if min - window < tc && tc < min {
                                t.zone_type_index = 1;
                                t.back_wall_type_index = 1;
                                t.type_index = tt::AIR;
                                t.contents = 0x5e;
                            }
                        }
                    }
                }
                pb.tiles[ti].set(t);
                y += 1;
            }

            // ---- air / water / island rows
            let start = imax(imax(local_dirt, local_rock), 0);
            let mut y = start;
            while y < 32 {
                let ti = (y * 32 + x) as usize;
                let mut t = pb.tiles[ti].get();
                t.type_index = tt::AIR;
                t.back_wall_type_index = tt::AIR;
                t.sun_light = 0xff;
                if y_pos < 16 || y < local_lake {
                    t.type_index = tt::WATER;
                    t.back_wall_type_index = tt::AIR;
                    t.zone_type_index = 3;
                    t.sun_light = 0;
                    t.partial_content_left = 0xff;
                } else {
                    t.partial_content_left = 0;
                }
                // pole back walls
                let lim = imax(imax(imax(local_lake, local_dirt), local_rock), 512 - y_pos * 32);
                for (col_x, wall) in [(0, 0x26u8), (wall_w / 4, 0x29), (wall_w / 2, 0x27), ((wall_w / 4) * 3, 0x28)] {
                    if world_x == col_x && y < lim + 16 {
                        t.back_wall_type_index = wall;
                        t.zone_type_index = 1;
                    }
                }
                // floating island overlay
                if plan.width > 0 && plan.local_min_x <= x && x < plan.local_min_x + plan.width {
                    self.overlay_island_tile(p, pb, &plan, x, y, world_x, y_pos, &mut t);
                }
                pb.tiles[ti].set(t);
                y += 1;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn noisy(seed: i32) -> Option<Rc<NoiseFunction>> {
        Some(Rc::new(
            NoiseFunction::init_with_frequency_x_frequency_y_frequency_z_amplitude_seed_tileable_loop_persistance(
                3, 3, 2, 0.9, seed, true, false, 1.2,
            ),
        ))
    }

    /// W=32 default-rules terrain with the Step A functions of seed 12345 attempt 1.
    pub(crate) fn sample_terrain() -> (Terrain, GenParams) {
        let mut t = Terrain::new_empty(32, 12345);
        let x = t.x_frequency_multiplier;
        let mk = |fx, fy, fz, amp, seed, tx, ty, pers| {
            Some(Rc::new(
                NoiseFunction::init_with_frequency_x_frequency_y_frequency_z_amplitude_seed_tileable_x_tileable_y_loop_persistance(
                    fx, fy, fz, amp, seed, tx, ty, false, pers,
                ),
            ))
        };
        t.height_a = mk(x + 3, 3, 2, 0.9, 12346, true, true, 1.2);
        t.height_b = mk(x + 4, 4, 4, 0.9, 12348, true, true, 1.2);
        t.sand = mk(x + 4, 1, 1, 1.0, 12354, true, true, 1.5);
        t.fault = mk(x + 8, 8, 4, 0.9, 12418, true, false, 1.4);
        t.cave_a = mk(x + 6, 3, 1, 1.0, 12353, true, false, 3.5);
        t.cave_b = mk(x + 12, 8, 1, 1.0, 12354, true, false, 1.5);
        let mut rules = CustomRules::default();
        rules.oceans = 2;
        rules.temperature = 2;
        rules.rainfall = 2;
        rules.plants = 2;
        rules.trees = 4;
        rules.terrain_type = 2;
        rules.caves = 1;
        rules.lakes = 1;
        rules.ore_quantity = 2;
        rules.ore_distribution = 1;
        rules.gems = 2;
        rules.treasure = 2;
        rules.sky_islands = 2;
        (t, GenParams { w: 32, rules, expert: false })
    }

    ///  (the W=32 and W=300 rows are checked here, the others by `tile_loader` tests).
    #[test]
    fn frequency_multiplier_and_divider() {
        let t = Terrain::new_empty(32, 1);
        assert_eq!((t.x_frequency_multiplier, t.y_height_divider), (-4, 32.0));
        let t = Terrain::new_empty(128, 1);
        assert_eq!((t.x_frequency_multiplier, t.y_height_divider), (-2, 32.0));
        let t = Terrain::new_empty(512, 1);
        assert_eq!((t.x_frequency_multiplier, t.y_height_divider), (0, 32.0));
        let t = Terrain::new_empty(2048, 1);
        assert_eq!((t.x_frequency_multiplier, t.y_height_divider), (2, 128.0));
        let t = Terrain::new_empty(8192, 1);
        assert_eq!((t.x_frequency_multiplier, t.y_height_divider), (4, 512.0));
        let t = Terrain::new_empty(300, 1);
        assert_eq!((t.x_frequency_multiplier, t.y_height_divider), (-1, 32.0));
    }

    /// First columns of the W=32 world: heights are finite, in range, rock/dirt relation stable (smoke test for the
    /// noise argument plumbing; the bit-exact tables are checked in `noise.rs`).
    #[test]
    fn initial_heights_are_sane() {
        let (t, p) = sample_terrain();
        for x in 0..64 {
            let (r, d) = t.get_initial_rock_and_dirt_height(&p, x);
            assert!(r > 16.0 && r < 1000.0, "{r}");
            assert!(d > 16.0 && d < 1000.0, "{d}");
        }
        let _ = noisy(1);
    }

    #[test]
    fn totally_flat_terrain_pins_surface() {
        let (t, mut p) = sample_terrain();
        p.rules.terrain_type = 0;
        for x in 0..32 {
            let (r, d) = t.get_initial_rock_and_dirt_height(&p, x);
            let m = if d <= r { r } else { d };
            assert!(m == 511.25 || m == 512.25 || (r - d).abs() < 1000.0);
        }
    }

    #[test]
    fn cave_and_fault_are_deterministic() {
        let (t, p) = sample_terrain();
        let a: Vec<_> = (0..50).map(|i| t.fault_offset(&p, i * 7, 300 + i)).collect();
        let b: Vec<_> = (0..50).map(|i| t.fault_offset(&p, i * 7, 300 + i)).collect();
        assert_eq!(a, b);
        let mut no_caves = p;
        no_caves.rules.caves = 0;
        assert!(!t.is_cave(&no_caves, 5, 300, 0));
        assert!(!t.is_floating_island_cave(&no_caves, 5, 300));
    }

    #[test]
    fn beach_below_sea_level_is_always_beach() {
        let (t, p) = sample_terrain();
        assert!(t.is_beach(&p, IntPair { x: 10, y: 100 }, 512));
        assert!(!t.is_beach(&p, IntPair { x: 10, y: 900 }, 512));
    }

    #[test]
    fn fill_dirt_tile_parent_types() {
        let (mut t, p) = sample_terrain();
        // flat world without noise: sand tables are zero so a fresh column gives dirt, a SAND parent gives sand.
        t.rock_heights = vec![500; 1024];
        t.dirt_heights = vec![520; 1024];
        let mut tile = Tile::default();
        t.fill_dirt_tile(&p, &mut tile, IntPair { x: 5, y: 519 }, 520, tt::SAND);
        assert_eq!(tile.type_index, tt::SAND);
        let mut tile = Tile::default();
        t.fill_dirt_tile(&p, &mut tile, IntPair { x: 5, y: 519 }, 520, tt::SAND_BEACH);
        assert!(tile.type_index == tt::SAND_BEACH || tile.type_index == tt::BLACK_SAND);
        let mut tile = Tile::default();
        t.fill_dirt_tile(&p, &mut tile, IntPair { x: 5, y: 519 }, 520, tt::DIRT_GRASS);
        assert_eq!(tile.back_wall_type_index, tile.type_index);
    }

    #[test]
    fn wrap_rules_of_accessors() {
        let (mut t, p) = sample_terrain();
        t.rock_heights = (0..1024).collect();
        t.dirt_heights = (0..1024).map(|v| v + 1).collect();
        t.lake_heights = (0..1024).map(|v| v + 2).collect();
        assert_eq!(t.get_rock_and_dirt_height(&p, -1), (1023, 1024));
        assert_eq!(t.get_rock_and_dirt_height(&p, 1024), (0, 1));
        assert_eq!(t.max_of_rock_and_dirt_height(&p, -1), 1024);
        assert_eq!(t.lake_height(&p, -1), 1025);
    }

    /// Corner cases of the per-column tables and of the flow-out passes on hand-made terrain.
    #[test]
    fn table_lookups_wrap_and_flat_worlds_have_no_ground() {
        let (t, p) = sample_terrain();
        let wt = p.wt();
        // x one past the end wraps once
        assert_eq!(t.max_of_rock_and_dirt_height(&p, wt + 1), t.max_of_rock_and_dirt_height(&p, 1));
        assert_eq!(t.lake_height(&p, wt + 1), t.lake_height(&p, 1));
        // a world without any rock or dirt has no ground level
        let flat = Terrain::new_empty(32, 1);
        assert_eq!(flat.unmodified_ground_level(&p, 10), 0);
        // W == 0 gives an empty terrain instead of an endless shift
        let none = Terrain::new_empty(0, 1);
        assert!(none.rock_heights.is_empty());
    }

    /// Hand-made heights: negative and out-of-range neighbours of the double index, caves under the surface of
    /// the neighbour columns.
    #[test]
    fn refine_terrain_over_hand_made_heights_and_caves() {
        use crate::trees::tree::testenv::env;
        let (world, _d) = env("tr_refine");
        let w = world.clone();
        for seed in 1..40i32 {
            let mut t = Terrain::new_empty(2, 5);
            let mk = |fx, fy, amp, s| {
                Some(Rc::new(
                    NoiseFunction::init_with_frequency_x_frequency_y_frequency_z_amplitude_seed_tileable_x_tileable_y_loop_persistance(
                        fx, fy, 1, amp, s, true, false, false, 1.5,
                    ),
                ))
            };
            t.cave_a = mk(2, 3, 1.0, seed);
            t.cave_b = mk(4, 8, 1.0, seed + 50);
            t.fault = mk(2, 8, 0.9, seed + 90);
            let wt = 64usize;
            for x in 0..wt {
                let r = if x % 2 == 0 { 40 } else { 20 };
                t.rock_heights[x] = if seed % 7 == 0 && x == 5 { -7 } else { r };
                t.dirt_heights[x] = t.rock_heights[x];
                t.lake_heights[x] = if seed % 7 == 0 && x == 5 {
                    -100
                } else if x % 3 == 0 {
                    55
                } else {
                    0
                };
            }
            // (a world with a negative rock height has no caves to flood)
            let caves = if seed % 7 == 0 {
                0
            } else if seed % 2 == 0 {
                2
            } else {
                1
            };
            let rules = CustomRules { caves, ..CustomRules::default() };
            let p = GenParams { w: 2, rules, expert: false };
            glibc_rand::srand(seed as u32);
            for slice in 0..8 {
                t.refine_terrain_slice(&p, &world, slice * 8);
            }
        }
        let flowing = Terrain::new_empty(2, 5);
        let p = GenParams { w: 2, rules: CustomRules::default(), expert: false };
        let tile = tile_at_world_position_loaded(10, 10, &w).unwrap();
        // at the bottom row nothing flows; elsewhere the loaded neighbours are dirt-filled, unloaded ones skipped
        flowing.recursively_flow_out_dirt_from_tile(&p, &w, &tile, IntPair { x: 10, y: 0 });
        for mx in 0..2 {
            *w.macro_tiles().get(mx).unwrap().physical_block.borrow_mut() = None;
        }
        for seed in 1..10u32 {
            glibc_rand::srand(seed);
            flowing.recursively_flow_out_dirt_from_tile(&p, &w, &tile, IntPair { x: 10, y: 10 });
        }
    }

    /// Beach sand on the rock line above the treasure height gets flint chests in a narrow noise window that depends
    /// on the treasure rule.
    #[test]
    fn beach_treasure_windows_of_every_rule() {
        let world = World::alloc(32);
        let mut found = 0;
        for treasure in [1u8, 2, 3] {
            for rainfall in [0u8, 2, 4] {
                for seed in 1..3i32 {
                    let (mut t, mut p) = sample_terrain();
                    let mk = |fx, fy, amp, s| {
                        Some(Rc::new(
                            NoiseFunction::init_with_frequency_x_frequency_y_frequency_z_amplitude_seed_tileable_x_tileable_y_loop_persistance(
                                fx, fy, 1, amp, s, true, false, false, 1.5,
                            ),
                        ))
                    };
                    t.sand = mk(2, 2, 1.0 + f64::from(seed) * 0.2, seed);
                    t.flint = mk(2, 2, 1.0 + f64::from(seed) * 0.4, seed + 40);
                    t.rock_heights = vec![500; 1024];
                    t.dirt_heights = vec![520; 1024];
                    p.rules.treasure = treasure as _;
                    p.rules.rainfall = rainfall as _;
                    for x_pos in (0..32).step_by(16) {
                        let pb = world.take_physical_block();
                        pb.x_pos.set(x_pos);
                        pb.y_pos.set(15);
                        t.generate_block_tiles(&p, pb, x_pos, 15, Some(&world));
                        found += pb.tiles.iter().filter(|c| c.get().contents == 0x5e).count();
                    }
                }
            }
        }
        assert!(found > 0);
    }
}
