//! Plant and tree growth helpers: growth vigor, spawn altitude, promise / seed maps.
//!
//!  Plants";
//! (`growthVigorForTreeTypeAtPos`, `spawnAltidtudeConstraintsForTreeType`);
//! `treeTypeForTreePromise`, `treeTypeForSeedItemType`, `plantTypeForSeedItemType`;
//! No RNG, no side effects. `world: None` behaves as ObjC nil (W = 0, zeroed `CustomRules`).

use super::climate::base_temperature_for_world_pos;
use super::state::World;
use crate::fmath::libm::{fabsf, fmodf};
use crate::fmath::vector::clamp;
use crate::types::enums::{IslandType, PlantType, TileContents, TreeType};
use crate::types::item_type::ItemType;
use crate::types::structs::{CustomRules, IntPair};

fn rules_of(world: Option<&World>) -> CustomRules {
    match world {
        Some(w) => w.custom_rules(),
        None => CustomRules::default(),
    }
}

fn width_macro_f(world: Option<&World>) -> f32 {
    world.map_or(0, |w| w.world_width_macro) as f32
}

/// `contentsTypeForPlantType(plantType, flowerType)` -> the `foregroundContents` value.
pub fn contents_type_for_plant_type(plant_type: PlantType, flower_type: bool) -> TileContents {
    let pick = |plain: u32, flower: u32| TileContents(if flower_type { flower } else { plain });
    match plant_type.0 {
        1 => pick(0x43, 0x44),
        2 => pick(0x47, 0x48),
        3 => pick(0x49, 0x4A),
        4 => TileContents(0x4C),
        5 => pick(0x4F, 0x50),
        6 => TileContents(0x51),
        7 => TileContents(0x7C),
        8 => TileContents(0x7D),
        9 => TileContents(0x93),
        10 => TileContents(0x94),
        _ => TileContents(0),
    }
}

/// `growthVigorForPlantTypeAtPos(plantType, pos, world)` -> float in [0,1].
pub fn growth_vigor_for_plant_type_at_pos(plant_type: PlantType, pos: IntPair, world: Option<&World>) -> f32 {
    let t = base_temperature_for_world_pos(pos, 0.25f32, 0.25f32, 0.0f32, world);
    match plant_type.0 {
        // SUNFLOWER, CORN, TULIP, TOMATO
        2 | 3 | 8 | 10 => clamp((t - 5.0f32) / 5.0f32, 0.0, 1.0),
        // CARROT
        4 => clamp(t / 5.0f32, 0.0, 1.0),
        // WHEAT
        9 => clamp(t / 5.0f32, 0.0, 1.0) * clamp(((pos.y as f32) - 512.0f32) / 50.0f32, 0.0, 1.0),
        // CHILLI (customRulesEnabled is NOT checked)
        5 => {
            let r = rules_of(world);
            let v = if r.temperature == 2 {
                fabsf(fmodf(((pos.x as f32) / 32.0f32) / width_macro_f(world), 0.5f32) - 0.25f32) / 0.25f32
            } else {
                1.0f32 - clamp((t - 25.0f32) / 5.0f32, 0.0, 1.0)
            };
            let u = 1.0f32 - 2.0f32 * v;
            if u > 0.2f32 {
                u
            } else {
                0.2f32
            }
        }
        // FLAX, KELP, VINE and anything else
        _ => 1.0f32,
    }
}

/// `spawnAltidtudeConstraintsForTreeType(t)` (sic): `(min, max)` altitude; default `(0, 768)`.
pub fn spawn_altidtude_constraints_for_tree_type(t: TreeType) -> IntPair {
    let (x, y) = match t.0 {
        1 => (0, 700),
        2 => (0, 650),
        3 => (550, 768),
        4 => (0, 768),
        5 => (0, 768),
        6 => (0, 562),
        7 => (0, 650),
        8 => (550, 768),
        9 => (580, 768),
        10 => (0, 650),
        _ => (0, 768),
    };
    IntPair { x, y }
}

/// `growthVigorForTreeTypeAtPos(t, pos, world)`; types outside 1..=10 return 1.0 (the `_` arm).
pub fn growth_vigor_for_tree_type_at_pos(t: TreeType, pos: IntPair, world: Option<&World>) -> f32 {
    let c = spawn_altidtude_constraints_for_tree_type(t);
    let x = pos.x;
    let y = pos.y;
    let u = || ((x as f32) / 32.0f32) / width_macro_f(world);
    let dist_t_a = || fabsf(fmodf(u(), 0.25f32) - 0.125f32) / 0.125f32;
    let dist_e_b = || fabsf(fmodf(u(), 0.5f32) - 0.25f32) / 0.25f32;
    let temp01 = || -> f32 {
        let r = rules_of(world);
        if r.temperature == 2 {
            dist_e_b()
        } else {
            1.0f32
                - clamp(
                    (base_temperature_for_world_pos(pos, 0.25f32, 0.25f32, 0.0f32, world) - 25.0f32) / 5.0f32,
                    0.0,
                    1.0,
                )
        }
    };
    let h = |div: f32, hi: f32| clamp(((c.y as f32) - (y as f32)) / div, 0.2f32, hi);
    let floor02 = |v: f32| if v > 0.2f32 { v } else { 0.2f32 };
    let cap1 = |s: f32| if 1.0f32 > s { s } else { 1.0f32 };
    match t.0 {
        // APPLE
        1 => {
            let d = dist_t_a();
            let hh = h(50.0, 1.0);
            floor02((1.0f32 - 1.5f32 * d) * hh)
        }
        // MANGO
        2 => {
            let tt = temp01();
            let hh = h(50.0, 1.0);
            floor02((1.0f32 - 2.0f32 * tt) * hh)
        }
        // MAPLE
        3 => {
            let d = dist_t_a();
            let hh = h(50.0, 1.0);
            floor02((1.0f32 - 2.0f32 * d) * hh)
        }
        // PINE
        4 => {
            let d = dist_e_b();
            let hh = h(50.0, 1.0);
            let s = cap1(10.1f32 - 10.0f32 * d);
            s * hh
        }
        // CACTUS
        5 => {
            let d = dist_e_b();
            let hh = h(200.0, 1.0);
            let s = cap1(10.1f32 - 10.0f32 * d);
            s * hh
        }
        // COCONUT
        6 => {
            let tt = temp01();
            let hh = h(40.0, 1.5);
            let hh = hh * hh;
            floor02((1.0f32 - 2.0f32 * tt) * hh)
        }
        // ORANGE, LIME
        7 | 10 => {
            let tt = temp01();
            let hh = h(50.0, 1.0);
            let s = cap1(4.1f32 - 4.0f32 * tt);
            s * hh
        }
        // CHERRY
        8 => {
            let d = dist_t_a();
            let hh = h(50.0, 1.0);
            floor02((1.0f32 - 1.5f32 * d) * hh)
        }
        // COFFEE
        9 => {
            let tt = temp01();
            let hh = h(50.0, 1.0);
            floor02((1.0f32 - 4.0f32 * tt) * hh)
        }
        // types outside 1..=10
        _ => 1.0f32,
    }
}

/// `treeTypeForTreePromise(promiseType)`: contents ids 108..=141 -> TreeType, else 0.
pub fn tree_type_for_tree_promise(promise_contents: i32) -> TreeType {
    TreeType(match promise_contents {
        108 => 11,
        128 => 12,
        129 => 13,
        130 => 14,
        131 => 15,
        132 => 1,
        133 => 7,
        134 => 8,
        135 => 4,
        136 => 5,
        137 => 10,
        138 => 9,
        139 => 6,
        140 => 3,
        141 => 2,
        _ => 0,
    })
}

/// `treePromiseTypeForIslandType(island)`: IslandType 1..=15 -> tree promise contents id, else 0.
pub fn tree_promise_type_for_island_type(island: IslandType) -> i32 {
    match island.0 {
        1 => 108,
        2 => 128,
        3 => 129,
        4 => 130,
        5 => 131,
        6 => 132,
        7 => 139,
        8 => 133,
        9 => 135,
        10 => 137,
        11 => 136,
        12 => 134,
        13 => 138,
        14 => 140,
        15 => 141,
        _ => 0,
    }
}

/// `treeTypeForSeedItemType(item)`.
pub fn tree_type_for_seed_item_type(item: ItemType) -> TreeType {
    TreeType(match item.0 {
        21 => 1,
        22 => 2,
        23 => 3,
        24 => 5,
        27 => 4,
        46 => 6,
        60 => 7,
        77 => 8,
        78 => 9,
        160 => 10,
        _ => 0,
    })
}

/// `plantTypeForSeedItemType(item)`: items 296 and 297 -> TULIP.
pub fn plant_type_for_seed_item_type(item: ItemType) -> PlantType {
    PlantType(match item.0 {
        54 => 1,
        61 => 2,
        62 => 3,
        71 => 4,
        112 => 5,
        144 => 6,
        295 => 7,
        296 | 297 => 8,
        310 => 9,
        316 => 10,
        _ => 0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ip(x: i32, y: i32) -> IntPair {
        IntPair { x, y }
    }

    #[test]
    fn contents_for_plants() {
        let expect = [
            (1, 0x43, 0x44),
            (2, 0x47, 0x48),
            (3, 0x49, 0x4A),
            (4, 0x4C, 0x4C),
            (5, 0x4F, 0x50),
            (6, 0x51, 0x51),
            (7, 0x7C, 0x7C),
            (8, 0x7D, 0x7D),
            (9, 0x93, 0x93),
            (10, 0x94, 0x94),
            (0, 0, 0),
            (11, 0, 0),
        ];
        for (p, a, b) in expect {
            assert_eq!(contents_type_for_plant_type(PlantType(p), false).0, a);
            assert_eq!(contents_type_for_plant_type(PlantType(p), true).0, b);
        }
    }

    #[test]
    fn spawn_altitude_table() {
        let t: Vec<(i32, i32)> = (0..=16)
            .map(|i| {
                let p = spawn_altidtude_constraints_for_tree_type(TreeType(i));
                (p.x, p.y)
            })
            .collect();
        assert_eq!(t[0], (0, 768));
        assert_eq!(t[1], (0, 700));
        assert_eq!(t[2], (0, 650));
        assert_eq!(t[3], (550, 768));
        assert_eq!(t[4], (0, 768));
        assert_eq!(t[5], (0, 768));
        assert_eq!(t[6], (0, 562));
        assert_eq!(t[7], (0, 650));
        assert_eq!(t[8], (550, 768));
        assert_eq!(t[9], (580, 768));
        assert_eq!(t[10], (0, 650));
        assert_eq!(t[11], (0, 768));
        assert_eq!(t[16], (0, 768));
    }

    #[test]
    fn promise_and_seed_maps() {
        let island: Vec<i32> = (0..=16).map(|i| tree_promise_type_for_island_type(IslandType(i))).collect();
        assert_eq!(island, vec![0, 108, 128, 129, 130, 131, 132, 139, 133, 135, 137, 136, 134, 138, 140, 141, 0]);
        let promise: Vec<u32> = (100..=145).map(|i| tree_type_for_tree_promise(i).0).collect();
        assert_eq!(promise[8], 11); // 108
        assert_eq!(promise[28], 12); // 128
        assert_eq!(promise[32], 1); // 132
        assert_eq!(promise[41], 2); // 141
        assert_eq!(promise[42], 0); // 142
        assert_eq!(promise[0], 0);
        // island -> promise -> tree type is consistent
        for i in 1..=15 {
            assert_ne!(tree_type_for_tree_promise(tree_promise_type_for_island_type(IslandType(i))).0, 0);
        }
        assert_eq!(plant_type_for_seed_item_type(ItemType(296)).0, 8);
        assert_eq!(plant_type_for_seed_item_type(ItemType(297)).0, 8);
        assert_eq!(plant_type_for_seed_item_type(ItemType(295)).0, 7);
        assert_eq!(plant_type_for_seed_item_type(ItemType(298)).0, 0);
        assert_eq!(tree_type_for_seed_item_type(ItemType(24)).0, 5);
        assert_eq!(tree_type_for_seed_item_type(ItemType(160)).0, 10);
        assert_eq!(tree_type_for_seed_item_type(ItemType(25)).0, 0);
    }

    #[test]
    fn tree_vigor_nil_world_and_gems() {
        for t in [0u32, 11, 12, 13, 14, 15, 16] {
            assert_eq!(growth_vigor_for_tree_type_at_pos(TreeType(t), ip(5, 5), None), 1.0);
        }
        // nil world: x-only variants see NaN: APPLE/MAPLE/CHERRY floor to 0.2, PINE is 1.0 * H
        assert_eq!(growth_vigor_for_tree_type_at_pos(TreeType(1), ip(5, 5), None), 0.2);
        assert_eq!(growth_vigor_for_tree_type_at_pos(TreeType(3), ip(5, 5), None), 0.2);
        assert_eq!(growth_vigor_for_tree_type_at_pos(TreeType(8), ip(5, 5), None), 0.2);
        assert_eq!(growth_vigor_for_tree_type_at_pos(TreeType(4), ip(5, 0), None), 1.0);
        assert_eq!(growth_vigor_for_tree_type_at_pos(TreeType(4), ip(5, 760), None), 0.2);
    }

    #[test]
    fn tree_vigor_real_world() {
        let w = World::alloc(32); // Wt = 1024
                                  // MAPLE at u = 0.125 (x = 128): dT = 0 => v = H = 1 at y = 0
        assert_eq!(growth_vigor_for_tree_type_at_pos(TreeType(3), ip(128, 0), Some(&w)), 1.0);
        // x = 0: dT = 1 => (1 - 2) * H < 0 => floor 0.2
        assert_eq!(growth_vigor_for_tree_type_at_pos(TreeType(3), ip(0, 0), Some(&w)), 0.2);
        // APPLE at dT = 0: (1 - 0) * H; H = clamp((700 - 650)/50) = 1.0, at y = 690 H = 0.2
        assert_eq!(growth_vigor_for_tree_type_at_pos(TreeType(1), ip(128, 650), Some(&w)), 1.0);
        assert_eq!(growth_vigor_for_tree_type_at_pos(TreeType(1), ip(128, 690), Some(&w)), 0.2);
        // PINE at the equator band center x = 256 (u = 0.25 => dE = 0): s = min(1, 10.1) = 1
        assert_eq!(growth_vigor_for_tree_type_at_pos(TreeType(4), ip(256, 0), Some(&w)), 1.0);
        // at dE = 1 (x = 0): s = 10.1 - 10 = 0.1 (f32) times H
        let v = growth_vigor_for_tree_type_at_pos(TreeType(4), ip(0, 0), Some(&w));
        assert!((v - 0.1).abs() < 1e-5, "{}", v);
        // custom temperature == 2 uses distE for MANGO: x = 256 => T = 0 => 1 * H
        let mut r = CustomRules::default();
        r.temperature = 2;
        w.set_custom_rules(r);
        assert_eq!(growth_vigor_for_tree_type_at_pos(TreeType(2), ip(256, 0), Some(&w)), 1.0);
        assert_eq!(growth_vigor_for_tree_type_at_pos(TreeType(2), ip(0, 0), Some(&w)), 0.2);
    }

    #[test]
    fn plant_vigor() {
        let w = World::alloc(32);
        for p in [0u32, 1, 6, 7, 11] {
            assert_eq!(growth_vigor_for_plant_type_at_pos(PlantType(p), ip(10, 10), Some(&w)), 1.0);
        }
        // equator, base 30 (disabled rules): T = base_temperature(pos,.25,.25, 0)
        let pos = ip(256, 600);
        let t = base_temperature_for_world_pos(pos, 0.25, 0.25, 0.0, Some(&w));
        for p in [2u32, 3, 8, 10] {
            let want = clamp((t - 5.0) / 5.0, 0.0, 1.0);
            assert_eq!(growth_vigor_for_plant_type_at_pos(PlantType(p), pos, Some(&w)).to_bits(), want.to_bits());
        }
        let want = clamp(t / 5.0, 0.0, 1.0) * clamp((600.0 - 512.0) / 50.0, 0.0, 1.0);
        assert_eq!(growth_vigor_for_plant_type_at_pos(PlantType(9), pos, Some(&w)).to_bits(), want.to_bits());
        // chilli, temperature == 2: distE at x = 256 is 0 => v = 0 => u = 1.0
        let mut r = CustomRules::default();
        r.temperature = 2;
        w.set_custom_rules(r);
        assert_eq!(growth_vigor_for_plant_type_at_pos(PlantType(5), ip(256, 0), Some(&w)), 1.0);
        assert_eq!(growth_vigor_for_plant_type_at_pos(PlantType(5), ip(0, 0), Some(&w)), 0.2);
    }
}

#[cfg(test)]
mod range_tests {
    use super::*;

    /// Every type and spot, with and without a world, in the two temperature modes.
    #[test]
    fn vigor_over_all_types_places_and_rules() {
        let w = World::alloc(32);
        let mut all = vec![];
        for mode in [0i8, 2] {
            let mut r = w.custom_rules();
            r.temperature = mode;
            w.set_custom_rules(r);
            for tree in 0..14u32 {
                for (x, y) in [(5, 100), (300, 400), (900, 700), (1023, 760), (200, 520), (600, 10)] {
                    let p = IntPair { x, y };
                    all.push(growth_vigor_for_tree_type_at_pos(TreeType(tree), p, Some(&w)));
                    all.push(growth_vigor_for_tree_type_at_pos(TreeType(tree), p, None));
                }
            }
            for plant in 0..12 {
                for (x, y) in [(5, 100), (300, 600), (900, 700)] {
                    let p = IntPair { x, y };
                    all.push(growth_vigor_for_plant_type_at_pos(PlantType(plant), p, Some(&w)));
                    all.push(growth_vigor_for_plant_type_at_pos(PlantType(plant), p, None));
                }
            }
        }
        assert_eq!(all.len(), 480);
        assert_eq!(growth_vigor_for_tree_type_at_pos(TreeType(0), IntPair { x: 1, y: 1 }, None), 1.0);
        assert_eq!(growth_vigor_for_tree_type_at_pos(TreeType(11), IntPair { x: 1, y: 1 }, None), 1.0);
    }

    #[test]
    fn seed_and_promise_tables() {
        let trees: Vec<u32> = (0..400).map(|i| tree_type_for_seed_item_type(ItemType(i)).0).collect();
        assert!(trees.contains(&10) && trees.iter().filter(|&&t| t != 0).count() >= 10);
        let plants: Vec<u32> = (0..400).map(|i| plant_type_for_seed_item_type(ItemType(i)).0).collect();
        assert!(plants.contains(&10) && plants.iter().filter(|&&t| t != 0).count() >= 10);
        for c in 100..150 {
            let _ = tree_type_for_tree_promise(c);
        }
    }
}
