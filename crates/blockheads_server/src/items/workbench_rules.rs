//! Workbench rules: level counts, visible recipe counts, names, fuel and electricity tables.
//!
//! The seven functions owned by `item_tables` (`items/misc.rs`) are re-exported only (CRITIC resolution, same file).

use crate::items::names::{ns_string_from_item_type, plural_ns_string_from_item_type};
use crate::types::enums::WorkbenchType;
use crate::types::item_type::ItemType;

pub use crate::items::misc::{
    fuel_count_for_workbench_and_level, interaction_object_type_for_item_type, item_type_for_workbench_type_and_level,
    workbench_requires_electricty, workbench_requires_fuel, workbench_type_for_item_type,
};
pub use crate::items::predicates::workbench_type_is_double_height;

/// `numberOfLevelsForWorkbenchType`: jump table over types 1..25, otherwise 1.
pub fn number_of_levels_for_workbench_type(t: WorkbenchType, expert_mode: bool) -> i32 {
    let e = expert_mode;
    let pick = |n: i32, x: i32| if e { x } else { n };
    match t.0 as i32 {
        1 | 13 => 6,
        2 => pick(4, 29),
        3 => pick(1, 2),
        4 => 2,
        5 => 1,
        6 => 7,
        7 => pick(3, 7),
        8 => pick(1, 2),
        9 => 3,
        10 => pick(4, 7),
        11 => 3,
        12 => 3,
        14 => pick(2, 6),
        22 => 3,
        23 => 1,
        24 => 3,
        25 => 2,
        _ => 1,
    }
}

/// `numberOfCraftableItemsForWorkbenchTypeAndLevel`: number of recipes visible at `level`.
/// `UPG = (level >= current_level) ? 1: 0` is the "upgrade this workbench" row.
pub fn number_of_craftable_items_for_workbench_type_and_level(
    t: WorkbenchType,
    level: i32,
    current_level: i32,
    is_cloud_game: bool,
    portal_chests_allowed: bool,
    expert_mode: bool,
) -> i32 {
    let upg: i32 = if level < current_level { 0 } else { 1 };
    let e = expert_mode;
    let l = level;
    let pick = |n: i32, x: i32| if e { x } else { n };
    match t.0 as i32 {
        1 | 13 => match l {
            0 => 11 + upg + (is_cloud_game as i32),
            1..=4 => upg + 1,
            5 => 2 + (portal_chests_allowed as i32) + (e as i32),
            _ => 0,
        },
        2 => {
            if e {
                if l < 28 {
                    upg + 1
                } else {
                    1
                }
            } else {
                match l {
                    0 => upg + 4,
                    1 => upg + 8,
                    2 => upg + 7,
                    _ => 11,
                }
            }
        }
        3 => {
            if e {
                if l == 0 {
                    upg + 2
                } else {
                    4
                }
            } else {
                6
            }
        }
        4 => {
            if l == 0 {
                upg + 6
            } else {
                6
            }
        }
        5 => 12,
        6 => match l {
            0 => upg + 5,
            1 => upg + pick(4, 3),
            2 => upg + pick(4, 5),
            3 => upg + 4,
            4 => upg + 3,
            5 => upg + 3,
            6 => 2,
            _ => 0,
        },
        7 => {
            if e {
                match l {
                    0 => upg + 7,
                    1 => upg + 10,
                    2..=5 => upg + 2,
                    6 => 4,
                    _ => 0,
                }
            } else {
                match l {
                    0 => upg + 7,
                    1 => upg + 10,
                    _ => 12,
                }
            }
        }
        8 => {
            if e {
                if l == 0 {
                    upg + 2
                } else {
                    6
                }
            } else {
                8
            }
        }
        9 => match l {
            0 | 1 => upg + 3,
            2 => 5,
            _ => 0,
        },
        10 => {
            if e {
                match l {
                    0 => upg + 1,
                    1 => upg + 9,
                    2..=5 => upg + 1,
                    6 => 4,
                    _ => 0,
                }
            } else {
                match l {
                    0 => upg + 1,
                    1 => upg + 8,
                    2 => upg + 5,
                    3 => 4,
                    _ => 0,
                }
            }
        }
        11 => match l {
            0 => upg + 2,
            1 => upg,
            _ => 0,
        },
        12 => match l {
            0 | 1 => upg + 4,
            2 => 3,
            _ => 0,
        },
        14 => {
            if e {
                match l {
                    0 => upg + 3,
                    1 => upg + 5,
                    2 => upg + 3,
                    3 => upg + 8,
                    4 => upg + 4,
                    5 => 3,
                    _ => 0,
                }
            } else {
                match l {
                    0 => upg + 11,
                    1 => 16,
                    _ => 0,
                }
            }
        }
        15 | 20 | 21 => 0,
        16 => 8,
        17 => 19,
        18 => 27,
        19 => 8,
        22 => match l {
            0 | 1 => upg + 4,
            2 => 8,
            _ => 0,
        },
        23 => 8,
        24 => match l {
            0 | 1 => upg + 3,
            _ => 3,
        },
        25 => match l {
            0 => upg + 27,
            1 => 28,
            _ => 0,
        },
        26 => 5,
        27 => 31,
        28 => 24,
        29 => 1,
        30 => 32,
        31 => 2,
        _ => 0,
    }
}

/// `nameForWorkbenchTypeAndLevel`: `%d` is `level + 1`.
pub fn name_for_workbench_type_and_level(t: WorkbenchType, level: i32, expert_mode: bool) -> String {
    let n = level.wrapping_add(1);
    match t.0 as i32 {
        1 | 13 => match level {
            1 => "AMETHYST PORTAL",
            2 => "SAPPHIRE PORTAL",
            3 => "EMERALD PORTAL",
            4 => "RUBY PORTAL",
            5 => "DIAMOND PORTAL",
            _ => "PORTAL",
        }
        .to_string(),
        2 => format!("WORKBENCH LEVEL {}", n),
        3 => {
            if expert_mode {
                format!("CAMPFIRE LEVEL {}", n)
            } else {
                "CAMPFIRE".to_string()
            }
        }
        4 => format!("TAILOR'S BENCH LEVEL {}", n),
        5 => "WOODWORK BENCH".to_string(),
        6 => format!("TOOL BENCH LEVEL {}", n),
        7 => format!("PRESS LEVEL {}", n),
        8 => {
            if expert_mode {
                format!("KILN LEVEL {}", n)
            } else {
                "KILN".to_string()
            }
        }
        9 => format!("FURNACE LEVEL {}", n),
        10 => format!("CRAFT BENCH LEVEL {}", n),
        11 => format!("MIXING BENCH LEVEL {}", n),
        12 => format!("DYE BENCH LEVEL {}", n),
        14 => format!("METALWORK BENCH LEVEL {}", n),
        15 => "STEAM GENERATOR".to_string(),
        16 => "ELECTRIC KILN".to_string(),
        17 => "ELECTRIC FURNACE".to_string(),
        18 => "ELECTRIC METALWORK BENCH".to_string(),
        19 => "ELECTRIC STOVE".to_string(),
        20 => "SOLAR PANEL".to_string(),
        21 => "FLYWHEEL".to_string(),
        22 => format!("ARMOR BENCH LEVEL {}", n),
        23 => "TRAIN YARD".to_string(),
        24 => "EASEL".to_string(),
        25 => "BUILDER'S BENCH".to_string(),
        26 => "REFINERY".to_string(),
        27 => "ELECTRIC PRESS".to_string(),
        28 => "COMPOST BIN".to_string(),
        29 => "ELECTRIC SLUICE".to_string(),
        30 => "EGG EXTRACTOR".to_string(),
        31 => "PIZZA OVEN".to_string(),
        _ => "UNKNOWN".to_string(),
    }
}

/// `upgradeNameForWorkbenchTypeAndLevel`.
pub fn upgrade_name_for_workbench_type_and_level(t: WorkbenchType, level: i32) -> String {
    if t.0 == 1 || t.0 == 13 {
        match level {
            0 => return "AMETHYST UPGRADE".to_string(),
            1 => return "SAPPHIRE UPGRADE".to_string(),
            2 => return "EMERALD UPGRADE".to_string(),
            3 => return "RUBY UPGRADE".to_string(),
            4 => return "DIAMOND UPGRADE".to_string(),
            _ => {}
        }
    }
    format!("LEVEL {} UPGRADE", level.wrapping_add(2))
}

const CAMPFIRE_FUEL_TYPES: [ItemType; 3] = [ItemType(4), ItemType(27), ItemType(1049)];
const STONE_KILN_FUEL_TYPES: [ItemType; 3] = [ItemType(27), ItemType(1049), ItemType(14)];
const STONE_FURNACE_FUEL_TYPES: [ItemType; 3] = [ItemType(1049), ItemType(14), ItemType(51)];

/// `fuelTypesForWorkbenchAndLevel`: `None` = NULL.
pub fn fuel_types_for_workbench_and_level(t: WorkbenchType, _level: i32) -> Option<[ItemType; 3]> {
    match t.0 {
        3 | 31 => Some(CAMPFIRE_FUEL_TYPES),
        8 => Some(STONE_KILN_FUEL_TYPES),
        9 | 14 | 15 => Some(STONE_FURNACE_FUEL_TYPES),
        _ => None,
    }
}

/// `fuelToAddForItemAndWorkbenchAndLevel`: units of fuel per item.
pub fn fuel_to_add_for_item_and_workbench_and_level(item: ItemType, t: WorkbenchType, _level: i32) -> i32 {
    match t.0 {
        3 | 31 => match item.0 {
            27 => 2,
            1049 => 5,
            _ => 1,
        },
        8 => match item.0 {
            14 => 5,
            1049 => 2,
            27 => 1,
            _ => 1,
        },
        9 | 14 | 15 => match item.0 {
            51 => 5,
            14 => 2,
            1049 => 1,
            _ => 1,
        },
        _ => 1,
    }
}

/// `titleForItemTypeAndCount`: `count == 1` gives the singular name, otherwise `"<count> <plural>"`
/// (also for 0 and negative counts).
pub fn title_for_item_type_and_count(item: ItemType, count: i32) -> String {
    if count == 1 {
        ns_string_from_item_type(item).to_string()
    } else {
        format!("{} {}", count, plural_ns_string_from_item_type(item))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::items::recipes::craftable_items_for_workbench_type_and_level;

    fn w(t: i32) -> WorkbenchType {
        WorkbenchType(t as u32)
    }

    /// Case #1.
    #[test]
    fn levels_table() {
        for t in -1..=40 {
            for e in [false, true] {
                let exp = match (t, e) {
                    (1 | 13, _) => 6,
                    (2, false) => 4,
                    (2, true) => 29,
                    (3, false) => 1,
                    (3, true) => 2,
                    (4, _) => 2,
                    (6, _) => 7,
                    (7, false) => 3,
                    (7, true) => 7,
                    (8, false) => 1,
                    (8, true) => 2,
                    (9, _) | (11, _) | (12, _) | (22, _) | (24, _) => 3,
                    (10, false) => 4,
                    (10, true) => 7,
                    (14, false) => 2,
                    (14, true) => 6,
                    (25, _) => 2,
                    _ => 1,
                };
                assert_eq!(number_of_levels_for_workbench_type(w(t), e), exp, "t={t} e={e}");
            }
        }
    }

    /// Case #7 (visible count <= slice length, upgrade row at the end of visible rows).
    #[test]
    fn counts_fit_slices() {
        for t in 0..=31 {
            for cloud in [false, true] {
                for expert in [false, true] {
                    let nl = number_of_levels_for_workbench_type(w(t), expert);
                    for level in 0..nl {
                        let slice = craftable_items_for_workbench_type_and_level(w(t), level, cloud, expert);
                        for cur in 0..=level + 1 {
                            for pca in [false, true] {
                                let n = number_of_craftable_items_for_workbench_type_and_level(
                                    w(t),
                                    level,
                                    cur,
                                    cloud,
                                    pca,
                                    expert,
                                );
                                assert!(
                                    n as usize <= slice.len(),
                                    "t={t} level={level} cur={cur} cloud={cloud} expert={expert} n={n} len={}",
                                    slice.len()
                                );
                                for r in slice.iter().take(n as usize) {
                                    if r.crafted_item_type == 132 {
                                        assert_eq!(r.upgrade_level, level + 2, "t={t} level={level}");
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn spot_counts() {
        // portals, level 0 cloud: 11 + 1 + 1 = 13
        assert_eq!(number_of_craftable_items_for_workbench_type_and_level(w(1), 0, 0, true, false, false), 13);
        assert_eq!(number_of_craftable_items_for_workbench_type_and_level(w(1), 5, 0, false, true, true), 4);
        assert_eq!(number_of_craftable_items_for_workbench_type_and_level(w(28), 0, 0, false, false, false), 24);
        assert_eq!(number_of_craftable_items_for_workbench_type_and_level(w(2), 0, 1, false, false, false), 4);
        assert_eq!(number_of_craftable_items_for_workbench_type_and_level(w(2), 0, 0, false, false, false), 5);
        assert_eq!(number_of_craftable_items_for_workbench_type_and_level(w(2), 29, 0, false, false, true), 1);
        assert_eq!(number_of_craftable_items_for_workbench_type_and_level(w(40), 0, 0, false, false, true), 0);
        assert_eq!(number_of_craftable_items_for_workbench_type_and_level(w(30), 0, 0, false, false, true), 32);
    }

    /// Case #3.
    #[test]
    fn names() {
        assert_eq!(name_for_workbench_type_and_level(w(5), 3, false), "WOODWORK BENCH");
        assert_eq!(name_for_workbench_type_and_level(w(2), 0, false), "WORKBENCH LEVEL 1");
        assert_eq!(name_for_workbench_type_and_level(w(3), 1, true), "CAMPFIRE LEVEL 2");
        assert_eq!(name_for_workbench_type_and_level(w(3), 1, false), "CAMPFIRE");
        assert_eq!(name_for_workbench_type_and_level(w(13), 4, false), "RUBY PORTAL");
        assert_eq!(name_for_workbench_type_and_level(w(1), 0, false), "PORTAL");
        assert_eq!(name_for_workbench_type_and_level(w(0), 0, false), "UNKNOWN");
        assert_eq!(name_for_workbench_type_and_level(w(25), 0, false), "BUILDER'S BENCH");
        assert_eq!(upgrade_name_for_workbench_type_and_level(w(1), 0), "AMETHYST UPGRADE");
        assert_eq!(upgrade_name_for_workbench_type_and_level(w(13), 4), "DIAMOND UPGRADE");
        assert_eq!(upgrade_name_for_workbench_type_and_level(w(13), 5), "LEVEL 7 UPGRADE");
        assert_eq!(upgrade_name_for_workbench_type_and_level(w(2), 2), "LEVEL 4 UPGRADE");
    }

    /// Case #4, #5 (exercise the re-exports from item_tables).
    #[test]
    fn item_maps() {
        let mut nz = 0;
        for i in -1i32..=1110 {
            let it = ItemType(i as u32);
            if workbench_type_for_item_type(it).0 != 0 {
                nz += 1;
            }
        }
        // 29 workbench items + 6 portal items (the original count of "33" is an arithmetic slip
        assert_eq!(nz, 35);
        for t in 2..=31 {
            if t == 13 {
                continue;
            }
            let it = item_type_for_workbench_type_and_level(w(t), 0);
            assert_eq!(workbench_type_for_item_type(it).0, t as u32, "t={t}");
        }
        assert_eq!(workbench_type_for_item_type(item_type_for_workbench_type_and_level(w(1), 0)).0, 13);
        for i in -1i32..=1110 {
            let it = ItemType(i as u32);
            let exp = if workbench_type_for_item_type(it).0 != 0 {
                1
            } else {
                match i {
                    63 | 169 | 170 => 3,
                    161 | 207 | 1043 | 1065 | 1072 | 1074 | 1104 => 2,
                    163 => 4,
                    168 => 5,
                    200 => 6,
                    210 => 7,
                    301 => 8,
                    331 => 9,
                    _ => 0,
                }
            };
            assert_eq!(interaction_object_type_for_item_type(it).0, exp, "item {i}");
        }
    }

    /// Case #6.
    #[test]
    fn fuel() {
        for t in 0..=40u32 {
            let wt = WorkbenchType(t);
            let exp = if matches!(t, 3 | 31 | 8 | 9 | 14 | 15) { 3 } else { 0 };
            assert_eq!(fuel_count_for_workbench_and_level(wt, 0), exp);
            assert_eq!(workbench_requires_fuel(wt, 0), exp > 0);
            assert_eq!(workbench_requires_electricty(wt, 0), matches!(t, 16 | 27 | 17 | 18 | 19 | 26 | 29 | 30));
            assert_eq!(fuel_types_for_workbench_and_level(wt, 0).is_some(), exp > 0);
        }
        assert_eq!(fuel_to_add_for_item_and_workbench_and_level(ItemType(1049), w(3), 0), 5);
        assert_eq!(fuel_to_add_for_item_and_workbench_and_level(ItemType(27), w(31), 0), 2);
        assert_eq!(fuel_to_add_for_item_and_workbench_and_level(ItemType(14), w(8), 0), 5);
        assert_eq!(fuel_to_add_for_item_and_workbench_and_level(ItemType(1049), w(8), 0), 2);
        assert_eq!(fuel_to_add_for_item_and_workbench_and_level(ItemType(51), w(9), 0), 5);
        assert_eq!(fuel_to_add_for_item_and_workbench_and_level(ItemType(14), w(14), 0), 2);
        assert_eq!(fuel_to_add_for_item_and_workbench_and_level(ItemType(51), w(2), 0), 1);
        assert_eq!(fuel_types_for_workbench_and_level(w(8), 0), Some([ItemType(27), ItemType(1049), ItemType(14)]));
    }

    #[test]
    fn titles() {
        assert_eq!(title_for_item_type_and_count(ItemType(4), 1), ns_string_from_item_type(ItemType(4)));
        assert_eq!(
            title_for_item_type_and_count(ItemType(4), 0),
            format!("0 {}", plural_ns_string_from_item_type(ItemType(4)))
        );
        assert_eq!(title_for_item_type_and_count(ItemType(4), 3).starts_with("3 "), true);
    }
}
