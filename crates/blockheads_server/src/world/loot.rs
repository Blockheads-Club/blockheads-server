//! Loot and random tables. Consumes libc `rand()` (`rng::rand()` / `rng::r01()`); the ORDER of draws is the contract.
//!
//!  Loot / random tables".
//! `(float)rand() / 2147483648.0f` is `r01()`; tagged variants are used so `trace-rng` shows the caller.

use super::helpers::{tile_is_dead_tree, tile_is_solid, tile_is_tree_trunk};
use super::state::{TileRef, World};
use crate::rng::glibc_rand::{r01_tagged, rand_tagged};
use crate::types::enums::ChestType;
use crate::types::item_type::ItemType;
use crate::types::structs::IntPair;

const MEDITATION_BONUS_TYPES: [u32; 52] = [
    22, 21, 23, 27, 54, 46, 60, 61, 71, 62, 310, 316, 77, 112, 3, 28, 48, 78, 72, 65, 38, 37, 35, 92, 119, 109, 110,
    100, 99, 101, 57, 56, 144, 91, 30, 13, 29, 151, 82, 167, 166, 121, 120, 295, 316, 310, 313, 321, 323, 280, 262,
    329,
];
const ROCK_BONUS_TYPES: [u32; 11] = [51, 31, 36, 32, 73, 72, 65, 38, 37, 35, 92];
const DIRT_BONUS_TYPES: [u32; 18] = [22, 21, 23, 27, 54, 46, 60, 61, 71, 62, 310, 316, 77, 112, 3, 28, 78, 295];
const SAND_BONUS_TYPES: [u32; 4] = [46, 24, 1042, 1075];
const BLACK_SAND_BONUS_TYPES: [u32; 3] = [175, 1076, 32];

const ITEM_AMETHYST: u32 = 0x57;
const ITEM_SAPPHIRE: u32 = 0x56;
const ITEM_EMERALD: u32 = 0x4C;
const ITEM_RUBY: u32 = 0x4B;
const ITEM_DIAMOND: u32 = 0x58;

/// The gem ladder shared by the meditation bonus and the rock bonus: float32 thresholds 0.5, 0.25, 0.125, 0.04.
fn gem_ladder(r: f32) -> ItemType {
    ItemType(if r > 0.5f32 {
        ITEM_AMETHYST
    } else if r > 0.25f32 {
        ITEM_SAPPHIRE
    } else if r > 0.125f32 {
        ITEM_EMERALD
    } else if r > 0.04f32 {
        ITEM_RUBY
    } else {
        ITEM_DIAMOND
    })
}

/// `randomMeditationBonus()`: always exactly 2 draws. `r <= 0.95` (double compare) picks
/// `meditationBonusTypes[rand() % 52]`, else a gem by a second `r01()`.
pub fn random_meditation_bonus() -> ItemType {
    let r = r01_tagged("loot::random_meditation_bonus");
    if (r as f64) <= 0.95f64 {
        let n = rand_tagged("loot::random_meditation_bonus");
        ItemType(MEDITATION_BONUS_TYPES[(n % 52) as usize])
    } else {
        let r2 = r01_tagged("loot::random_meditation_bonus");
        gem_ladder(r2)
    }
}

/// `randomBonusItemTypeForTile(tile, pos, count, world)`. `None` tile -> 0 with `count` untouched.
/// Draw order: `n = rand()` always first; then one `r01()` only if `tile_is_solid && type != 4 && contents != 0`
/// (a result `< 0.1f` gives `count = 10` and `item_type_from_tile_is_foregorund(.., false,..)`); then by type.
/// Table indices use the first draw `n`.
pub fn random_bonus_item_type_for_tile(
    tile: Option<TileRef>,
    pos: IntPair,
    count: &mut i32,
    world: &World,
) -> ItemType {
    let Some(tile) = tile else { return ItemType(0) };
    *count = 1;
    let n = rand_tagged("loot::random_bonus_item_type_for_tile");
    let t = tile.get();
    if tile_is_solid(Some(tile))
        && t.type_index != 4
        && t.contents != 0
        && r01_tagged("loot::random_bonus_item_type_for_tile") < 0.1f32
    {
        *count = 10;
        return item_type_from_tile_is_foregorund(tile, pos, false, world);
    }
    let tag = "loot::random_bonus_item_type_for_tile";
    match t.type_index {
        // ROCK, GRAVEL
        1 | 70 => {
            let r = r01_tagged(tag) as f64;
            if r > 0.98f64 {
                if r > 0.99f64 {
                    let r2 = r01_tagged(tag) as f64;
                    ItemType(if r2 > 0.9f64 { 0x118 } else { 0x117 })
                } else {
                    let r2 = r01_tagged(tag) as f64;
                    ItemType(if r2 > 0.9f64 { 0x106 } else { 0x105 })
                }
            } else {
                let r3 = r01_tagged(tag) as f64;
                if r3 > 0.95f64 {
                    let r4 = r01_tagged(tag);
                    gem_ladder(r4)
                } else {
                    ItemType(ROCK_BONUS_TYPES[(n % 11) as usize])
                }
            }
        }
        // DIRT, DIRT_GRASS, DIRT_GRASS_FROZEN, COMPOST, COMPOST_GRASS, COMPOST_GRASS_FROZEN
        6 | 27 | 28 | 48 | 49 | 50 => ItemType(DIRT_BONUS_TYPES[(n % 18) as usize]),
        // SAND, SAND_BEACH
        7 | 8 => ItemType(SAND_BONUS_TYPES[(n % 4) as usize]),
        // BLACK_SAND
        58 => ItemType(BLACK_SAND_BONUS_TYPES[(n % 3) as usize]),
        12 => ItemType(0x30),
        14 => ItemType(0x40E),
        17 => ItemType(0x49),
        19 => ItemType(0x48),
        29 => ItemType(0x48),
        51 => ItemType(0x41),
        _ => ItemType(0),
    }
}

/// `itemTypeFromTileIsForegorund(tile, pos, isForeground, world)` (spelling preserved).
pub fn item_type_from_tile_is_foregorund(tile: TileRef, pos: IntPair, is_foreground: bool, world: &World) -> ItemType {
    let t = tile.get();
    let r = Some(tile);
    if !is_foreground {
        // Mods (not in the original): a variant block gives its own item.
        if let Some(item) =
            crate::mods::variant_drop(&t.padding, t.type_index, crate::world::tiles::wrapped_x(world, pos.x), pos.y)
        {
            return ItemType(item);
        }
        let v: u32 = match t.type_index {
            // AIR, WATER, SNOW: by contents
            2 | 3 | 5 => {
                let c = t.contents;
                if c == 0x60 {
                    0xB2
                } else if matches!(c, 0x03 | 0x15 | 0x18 | 0x09 | 0x06 | 0x0F) {
                    4
                } else if tile_is_tree_trunk(r) {
                    0x419
                } else if tile_is_dead_tree(r) {
                    4
                } else if matches!(c, 0x0C | 0x12 | 0x59 | 0x2B | 0x6E | 0x71 | 0x74 | 0x7A) {
                    // Note: E lists 0x71 twice in this set (0x77 is absent); kept literally.
                    4
                } else if c == 0x67 {
                    0x43F
                } else if c == 0x68 {
                    0x440
                } else if c == 0x65 {
                    match world.dynamic_world().and_then(|dw| dw.stairs_at_pos(pos)) {
                        Some(st) => return ItemType(st.item_type as u32),
                        None => 0,
                    }
                } else if c == 0x92 {
                    0x130
                } else {
                    0
                }
            }
            // DIRT, DIRT_GRASS, DIRT_GRASS_FROZEN
            6 | 27 | 28 => match t.contents {
                1 => 3,
                2 => 0x1C,
                _ => 0x418,
            },
            48 | 49 | 50 => 0x426,
            7 | 8 => 0x41B,
            58 => 0x433,
            1 => match t.contents {
                0x3D => 0x1F,
                0x3E => 0x24,
                0x3F => 0x20,
                0x41 => 0x33,
                0x4D => 0x49,
                0x6A => 0x105,
                0x6B => 0x117,
                _ => 0x400,
            },
            10 => 0x400,
            11 => 0x402,
            70 => 0x446,
            12 | 13 => {
                if t.contents == 0x40 {
                    0x30
                } else {
                    0x404
                }
            }
            76 => 0x44F,
            77 => 0x451,
            14 | 15 => 0x406,
            26 => 0x415,
            71 => 0x44A,
            72 => 0x44B,
            73 => 0x44C,
            74 => 0x44D,
            75 => 0x44E,
            67 => 0x441,
            68 => 0x443,
            69 => 0x442,
            53 => 0x42A,
            54 => 0x42B,
            55 => 0x42C,
            56 => 0x42D,
            57 => 0x42E,
            9 => 0x419,
            32 => 0x421,
            16 => 0x0B,
            17 | 18 => 0x40C,
            19 | 20 => 0x40E,
            21 => 0x40F,
            22 => 0x410,
            23 => 0x411,
            24 => 0x412,
            59 => 0x434,
            4 => 0x424,
            29 | 30 => 0x41E,
            51 | 52 => 0x427,
            _ => 0,
        };
        return ItemType(v);
    }
    // foreground
    if !matches!(t.type_index, 2 | 3 | 5) {
        return ItemType(0);
    }
    let dw = world.dynamic_world();
    let v: u32 = match t.foreground_contents {
        0x31 => 17,         // BASIC_TORCH
        0x61 => 183,        // ICE_TORCH
        0x32 => 47,         // CLAY_LANTERN_LIT
        0x57 => 150,        // STEEL_LANTERN
        0x66 => 254,        // STEEL_DOWNLIGHT
        0x69 => 258,        // STEEL_UPLIGHT
        0x55 => 148,        // RUBY_CHANDELIER
        0x54 => 147,        // EMERALD_CHANDELIER
        0x53 => 146,        // SAPPHIRE_CHANDELIER
        0x52 => 145,        // AMETHYST_CHANDELIER
        0x56 => 149,        // DIAMOND_CHANDELIER
        0x33 => 75,         // RUBY
        0x35 => 76,         // EMERALD
        0x37 => 86,         // SAPPHIRE
        0x39 => 87,         // AMETHYST
        0x3B => 88,         // DIAMOND
        0x58 => 157,        // SHARK_JAW
        0x4E => 91,         // DODO_EGG
        0x42 => 53,         // LADDER
        0x43 => 55,         // FLAX
        0x44 => 54,         // FLAX_SEED
        0x47 | 0x48 => 61,  // SUNFLOWER_SEED
        0x4C => 71,         // CARROT
        0x7D => 296,        // TULIP_BULB
        0x93 => 310,        // WHEAT
        0x94 => 316,        // TOMATO
        0x49 | 0x4A => 62,  // CORN
        0x4F | 0x50 => 112, // CHILLI
        0x51 => 144,        // KELP
        0x7C => 295,        // VINE
        0x62 => 199,        // RAIL
        0x60 => 178,        // COPPER_WIRE
        0x63 => dw.and_then(|d| d.painting_at_pos(pos)).map_or(0, |o| o.item_type as u32),
        0x64 => dw.and_then(|d| d.column_at_pos(pos)).map_or(0, |o| o.item_type as u32),
        _ => 0,
    };
    ItemType(v)
}

/// `randomBonusClothingItemTypeFromSharkAtLevel(level)`: exactly one draw for levels 0..=2, none otherwise.
pub fn random_bonus_clothing_item_type_from_shark_at_level(level: i32) -> ItemType {
    const LINEN_HAT: u32 = 117;
    let tag = "loot::random_bonus_clothing_item_type_from_shark_at_level";
    match level {
        0 => {
            let k = rand_tagged(tag) % 4;
            ItemType([117, 115, 84, 85][k as usize])
        }
        1 => {
            let k = rand_tagged(tag) % 3;
            ItemType([124, 122, 130][k as usize])
        }
        2 => {
            let k = rand_tagged(tag) % 4;
            match k {
                0 => ItemType(127),
                1 => ItemType(126),
                _ => ItemType(LINEN_HAT),
            }
        }
        _ => ItemType(LINEN_HAT),
    }
}

/// `numberOfItemsForChestType`: SHELF(2) and DISPLAY_CABINET(5) hold 4, else 16.
pub fn number_of_items_for_chest_type(c: ChestType) -> i32 {
    if c.0 == 2 || c.0 == 5 {
        4
    } else {
        16
    }
}

/// `itemTypeFromChestType`: SAFE 0x429, SHELF 0xA1, GOLD 0x430, PORTAL 0x432, DISPLAY_CABINET 0xCF, FEEDER 0x450,
/// STANDARD/other 0x413.
pub fn item_type_from_chest_type(c: ChestType) -> ItemType {
    ItemType(match c.0 {
        1 => 0x429,
        2 => 0xA1,
        3 => 0x430,
        4 => 0x432,
        5 => 0xCF,
        6 => 0x450,
        _ => 0x413,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rng::glibc_rand::{r01_from, rand, srand};
    use crate::world::helpers::testutil::{dw_obj, world_with_dw};

    /// The first `n` raw draws after `srand(seed)`; leaves the generator re-seeded.
    fn draws(seed: u32, n: usize) -> Vec<i32> {
        srand(seed);
        let v: Vec<i32> = (0..n).map(|_| rand()).collect();
        srand(seed);
        v
    }
    /// After calling `f`, the generator must be positioned right after `k` draws of `seed`.
    fn assert_consumed(seed: u32, k: usize) {
        let all = draws(seed, k + 1);
        srand(seed);
        for _ in 0..k {
            rand();
        }
        assert_eq!(rand(), all[k], "expected exactly {} draws", k);
    }

    #[test]
    fn meditation_bonus_two_draws() {
        for seed in 1..200u32 {
            let d = draws(seed, 2);
            let r = r01_from(d[0]) as f64;
            let got = random_meditation_bonus();
            if r <= 0.95 {
                assert_eq!(got.0, MEDITATION_BONUS_TYPES[(d[1] % 52) as usize]);
            } else {
                let r2 = r01_from(d[1]);
                assert!(matches!(got.0, 0x57 | 0x56 | 0x4C | 0x4B | 0x58));
                assert_eq!(got, gem_ladder(r2));
            }
            assert_consumed(seed, 2);
        }
    }

    #[test]
    fn gem_ladder_thresholds() {
        assert_eq!(gem_ladder(0.6).0, 0x57);
        assert_eq!(gem_ladder(0.5).0, 0x56);
        assert_eq!(gem_ladder(0.3).0, 0x56);
        assert_eq!(gem_ladder(0.2).0, 0x4C);
        assert_eq!(gem_ladder(0.1).0, 0x4B);
        assert_eq!(gem_ladder(0.04).0, 0x58);
        assert_eq!(gem_ladder(0.0).0, 0x58);
    }

    fn tile_of(type_index: u8, contents: u8) -> (std::rc::Rc<crate::world::state::World>, TileRef) {
        let (w, _dw, pb) = world_with_dw(32);
        let t = TileRef::new(pb, 0);
        t.set_type_index(type_index);
        t.set_contents(contents);
        (w, t)
    }

    #[test]
    fn bonus_none_tile() {
        let (w, _t) = tile_of(1, 0);
        let mut c = 77;
        srand(1);
        assert_eq!(random_bonus_item_type_for_tile(None, IntPair { x: 0, y: 0 }, &mut c, &w).0, 0);
        assert_eq!(c, 77);
        assert_consumed(1, 0);
    }

    #[test]
    fn bonus_table_types_use_first_draw_only() {
        let p = IntPair { x: 0, y: 0 };
        for (ty, table) in [
            (6u8, &DIRT_BONUS_TYPES[..]),
            (27, &DIRT_BONUS_TYPES[..]),
            (28, &DIRT_BONUS_TYPES[..]),
            (48, &DIRT_BONUS_TYPES[..]),
            (49, &DIRT_BONUS_TYPES[..]),
            (50, &DIRT_BONUS_TYPES[..]),
            (7, &SAND_BONUS_TYPES[..]),
            (8, &SAND_BONUS_TYPES[..]),
            (58, &BLACK_SAND_BONUS_TYPES[..]),
        ] {
            for seed in 1..40u32 {
                // contents == 0 => no second draw
                let (w, t) = tile_of(ty, 0);
                let d = draws(seed, 1);
                let mut c = 0;
                let got = random_bonus_item_type_for_tile(Some(t), p, &mut c, &w);
                assert_eq!(got.0, table[(d[0] as usize) % table.len()], "type {}", ty);
                assert_eq!(c, 1);
                assert_consumed(seed, 1);
            }
        }
    }

    #[test]
    fn bonus_second_draw_and_ten_count() {
        let p = IntPair { x: 0, y: 0 };
        let mut saw_ten = false;
        for seed in 1..400u32 {
            let (w, t) = tile_of(12, 0x40); // LIMESTONE with contents
            let d = draws(seed, 2);
            let mut c = 0;
            let got = random_bonus_item_type_for_tile(Some(t), p, &mut c, &w);
            if r01_from(d[1]) < 0.1 {
                saw_ten = true;
                assert_eq!(c, 10);
                assert_eq!(got.0, 0x30); // block drop of LIMESTONE with contents 0x40
            } else {
                assert_eq!(c, 1);
                assert_eq!(got.0, 0x30);
            }
            assert_consumed(seed, 2);
        }
        assert!(saw_ten);
        // ICE (type 4) never takes the extra draw
        let (w, t) = tile_of(4, 3);
        srand(9);
        let mut c = 0;
        random_bonus_item_type_for_tile(Some(t), p, &mut c, &w);
        assert_consumed(9, 1);
        // air is not solid: no extra draw either
        let (w, t) = tile_of(2, 3);
        srand(9);
        random_bonus_item_type_for_tile(Some(t), p, &mut c, &w);
        assert_consumed(9, 1);
    }

    #[test]
    fn rock_ladder_draw_counts() {
        let p = IntPair { x: 0, y: 0 };
        let mut kinds = [0u32; 4];
        for seed in 1..3000u32 {
            let (w, t) = tile_of(1, 0); // contents 0 => no 0.1 draw
            let d = draws(seed, 5);
            let r = r01_from(d[1]) as f64;
            let mut c = 0;
            let got = random_bonus_item_type_for_tile(Some(t), p, &mut c, &w);
            if r > 0.98 {
                let r2 = r01_from(d[2]) as f64;
                let want = if r > 0.99 {
                    if r2 > 0.9 {
                        0x118
                    } else {
                        0x117
                    }
                } else if r2 > 0.9 {
                    0x106
                } else {
                    0x105
                };
                assert_eq!(got.0, want);
                assert_consumed(seed, 3);
                kinds[0] += 1;
            } else {
                let r3 = r01_from(d[2]) as f64;
                if r3 > 0.95 {
                    assert_eq!(got, gem_ladder(r01_from(d[3])));
                    assert_consumed(seed, 4);
                    kinds[1] += 1;
                } else {
                    assert_eq!(got.0, ROCK_BONUS_TYPES[(d[0] % 11) as usize]);
                    assert_consumed(seed, 3);
                    kinds[2] += 1;
                }
            }
        }
        assert!(kinds[0] > 0 && kinds[1] > 0 && kinds[2] > 0);
    }

    #[test]
    fn fixed_bonus_types() {
        let p = IntPair { x: 0, y: 0 };
        for (ty, want) in
            [(12u8, 0x30u32), (14, 0x40E), (17, 0x49), (19, 0x48), (29, 0x48), (51, 0x41), (2, 0), (9, 0), (66, 0)]
        {
            let (w, t) = tile_of(ty, 0);
            let mut c = 0;
            assert_eq!(random_bonus_item_type_for_tile(Some(t), p, &mut c, &w).0, want, "type {}", ty);
        }
    }

    #[cfg(feature = "mods")]
    #[test]
    fn variant_block_gives_its_own_item() {
        use crate::mods::{set_tables, ContentTables, VariantInfo};
        let p = IntPair { x: 3, y: 0 };
        let (w, t) = tile_of(11, 0);
        assert_eq!(item_type_from_tile_is_foregorund(t, p, false, &w).0, 0x402);
        let mut tables = ContentTables::default();
        tables.variants.insert(5, VariantInfo { base: 11, drops: Some(1108) });
        tables.variants.insert(6, VariantInfo { base: 11, drops: None });
        set_tables(tables);
        let mark = |id: u16| {
            let v = bhmod_content::variant::Variant { id, base: 11, layer: 0, data: 0 };
            t.update(|x| x.padding = bhmod_content::variant::encode(&v, 3, 0));
        };
        mark(5);
        assert_eq!(item_type_from_tile_is_foregorund(t, p, false, &w).0, 1108);
        // The position wraps like the tile's: the same record at the wrapped x.
        let width = w.world_width_macro * 32;
        assert_eq!(item_type_from_tile_is_foregorund(t, IntPair { x: 3 + width, y: 0 }, false, &w).0, 1108);
        // No drop declared, another tile, a changed block: the vanilla drop.
        mark(6);
        assert_eq!(item_type_from_tile_is_foregorund(t, p, false, &w).0, 0x402);
        mark(5);
        assert_eq!(item_type_from_tile_is_foregorund(t, IntPair { x: 4, y: 0 }, false, &w).0, 0x402);
        t.set_type_index(10);
        assert_eq!(item_type_from_tile_is_foregorund(t, p, false, &w).0, 0x400);
        // Foreground contents never come from a variant.
        mark(5);
        assert_eq!(item_type_from_tile_is_foregorund(t, p, true, &w).0, 0);
        set_tables(ContentTables::default());
    }

    #[test]
    fn block_drops() {
        let p = IntPair { x: 0, y: 0 };
        let (w, t) = tile_of(6, 0);
        let f = |t: TileRef| item_type_from_tile_is_foregorund(t, p, false, &w).0;
        assert_eq!(f(t), 0x418);
        t.set_contents(1);
        assert_eq!(f(t), 3);
        t.set_contents(2);
        assert_eq!(f(t), 0x1C);
        t.set_type_index(1);
        t.set_contents(0x6B);
        assert_eq!(f(t), 0x117);
        t.set_contents(0);
        assert_eq!(f(t), 0x400);
        for (ty, want) in [
            (10u8, 0x400u32),
            (11, 0x402),
            (70, 0x446),
            (76, 0x44F),
            (77, 0x451),
            (26, 0x415),
            (71, 0x44A),
            (75, 0x44E),
            (53, 0x42A),
            (57, 0x42E),
            (9, 0x419),
            (32, 0x421),
            (16, 0x0B),
            (59, 0x434),
            (4, 0x424),
            (25, 0),
            (31, 0),
            (66, 0),
            (0, 0),
            (200, 0),
        ] {
            t.set_type_index(ty);
            assert_eq!(f(t), want, "type {}", ty);
        }
        t.set_type_index(12);
        t.set_contents(0x40);
        assert_eq!(f(t), 0x30);
        t.set_contents(0);
        assert_eq!(f(t), 0x404);
        // air contents
        t.set_type_index(2);
        for (c, want) in [
            (0x60u8, 0xB2u32),
            (0x03, 4),
            (0x07, 0x419),
            (0x1D, 0x419),
            (0x1B, 4),
            (0x0C, 4),
            (0x67, 0x43F),
            (0x68, 0x440),
            (0x92, 0x130),
            (0, 0),
            (0x65, 0),
        ] {
            t.set_contents(c);
            assert_eq!(f(t), want, "contents {:#x}", c);
        }
    }

    #[test]
    fn stairs_and_foreground_drops() {
        let p = IntPair { x: 3, y: 4 };
        let (w, dw, pb) = world_with_dw(32);
        let t = TileRef::new(pb, 0);
        t.set_type_index(2);
        t.set_contents(0x65);
        dw.stairs.borrow_mut().insert((3, 4), dw_obj(0x777, 0, 0, false, true));
        assert_eq!(item_type_from_tile_is_foregorund(t, p, false, &w).0, 0x777);
        // foreground
        t.set_contents(0);
        for (fg, want) in [
            (0x31u8, 17u32),
            (0x61, 183),
            (0x33, 75),
            (0x3B, 88),
            (0x42, 53),
            (0x43, 55),
            (0x44, 54),
            (0x47, 61),
            (0x48, 61),
            (0x49, 62),
            (0x4A, 62),
            (0x4C, 71),
            (0x4F, 112),
            (0x50, 112),
            (0x51, 144),
            (0x7C, 295),
            (0x7D, 296),
            (0x93, 310),
            (0x94, 316),
            (0x62, 199),
            (0x60, 178),
            (0x58, 157),
            (0x4E, 91),
            (0x01, 0),
        ] {
            t.set_foreground_contents(fg);
            assert_eq!(item_type_from_tile_is_foregorund(t, p, true, &w).0, want, "fg {:#x}", fg);
        }
        t.set_foreground_contents(0x63);
        assert_eq!(item_type_from_tile_is_foregorund(t, p, true, &w).0, 0);
        dw.painting.borrow_mut().insert((3, 4), dw_obj(0x500, 0, 0, false, true));
        assert_eq!(item_type_from_tile_is_foregorund(t, p, true, &w).0, 0x500);
        t.set_foreground_contents(0x64);
        assert_eq!(item_type_from_tile_is_foregorund(t, p, true, &w).0, 0);
        dw.column.borrow_mut().insert((3, 4), dw_obj(0x501, 0, 0, false, true));
        assert_eq!(item_type_from_tile_is_foregorund(t, p, true, &w).0, 0x501);
        // foreground drops only on air/water/snow
        t.set_type_index(6);
        t.set_foreground_contents(0x31);
        assert_eq!(item_type_from_tile_is_foregorund(t, p, true, &w).0, 0);
    }

    #[test]
    fn shark_clothing_one_draw() {
        for seed in 1..50u32 {
            let d = draws(seed, 1)[0];
            assert_eq!(random_bonus_clothing_item_type_from_shark_at_level(0).0, [117, 115, 84, 85][(d % 4) as usize]);
            assert_consumed(seed, 1);
            draws(seed, 1);
            assert_eq!(random_bonus_clothing_item_type_from_shark_at_level(1).0, [124, 122, 130][(d % 3) as usize]);
            assert_consumed(seed, 1);
            draws(seed, 1);
            assert_eq!(
                random_bonus_clothing_item_type_from_shark_at_level(2).0,
                [127, 126, 117, 117][(d % 4) as usize]
            );
            assert_consumed(seed, 1);
            draws(seed, 1);
            assert_eq!(random_bonus_clothing_item_type_from_shark_at_level(3).0, 117);
            assert_eq!(random_bonus_clothing_item_type_from_shark_at_level(-1).0, 117);
            assert_consumed(seed, 0);
        }
    }

    #[test]
    fn chest_helpers() {
        let counts: Vec<i32> = (0..=7).map(|c| number_of_items_for_chest_type(ChestType(c))).collect();
        assert_eq!(counts, vec![16, 16, 4, 16, 16, 4, 16, 16]);
        let items: Vec<u32> = (0..=7).map(|c| item_type_from_chest_type(ChestType(c)).0).collect();
        assert_eq!(items, vec![0x413, 0x429, 0xA1, 0x430, 0x432, 0xCF, 0x450, 0x413]);
    }
}

#[cfg(test)]
mod sweep_tests {
    use super::*;

    /// Every tile type with every contents and foreground contents value yields some item (usually 0).
    #[test]
    fn drops_for_every_tile_value() {
        let w = crate::world::state::World::alloc(32);
        let pb: crate::world::state::PhysicalBlockRef =
            Box::leak(Box::new(crate::world::state::PhysicalBlock::new_fresh()));
        let tile = TileRef::new(pb, 0);
        let pos = IntPair { x: 1, y: 1 };
        let mut nonzero = 0;
        for ty in 0..=0x50u8 {
            for c in 0..=0x7fu8 {
                tile.update(|t| {
                    t.type_index = ty;
                    t.contents = c;
                    t.foreground_contents = c;
                    t.background_contents = c;
                });
                if item_type_from_tile_is_foregorund(tile, pos, false, &w).0 != 0 {
                    nonzero += 1;
                }
                if item_type_from_tile_is_foregorund(tile, pos, true, &w).0 != 0 {
                    nonzero += 1;
                }
            }
        }
        assert!(nonzero > 200);
    }
}
