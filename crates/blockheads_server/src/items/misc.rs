//! Misc scalar helpers, skin-option conversion, workbench / interaction mapping, dodo-egg table.
//!
//! `crafting_tables` must `pub use` the workbench / dodo-egg functions from here.

use crate::types::enums::{DodoBreed, InteractionObjectType, WorkbenchType};
use crate::types::item_type::ItemType;
use crate::types::structs::{BlockheadSkinOptions, MJColor8};

pub use crate::fmath::vector::{clamp, clampi, linear_interpolate};

type I = ItemType;

/// `getCreditTimeString`. All arithmetic float32; `(int)` truncates toward zero; branches use the
/// `!(x > c)` form so NaN falls into the first.
pub fn get_credit_time_string(seconds: f32) -> String {
    let plural = |n: i32| if n > 1 { "S" } else { "" };
    if !(seconds > 0.001f32) {
        return "NO CREDIT".to_string();
    }
    if !(seconds > 90.0f32) {
        return "< 1 MINUTE".to_string();
    }
    if !(seconds > 3600.0f32) {
        return format!("{} MINUTES", ((31.0f32 + seconds) / 60.0f32) as i32);
    }
    if !(seconds > 86400.0f32) {
        let h = ((seconds / 60.0f32) / 60.0f32) as i32;
        let m = ((seconds / 60.0f32) as i32).wrapping_add(h.wrapping_mul(-60));
        if m < 1 {
            format!("{} HOUR{}", h, plural(h))
        } else {
            format!("{} HOUR{}, {} MINUTE{}", h, plural(h), m, plural(m))
        }
    } else {
        let d = (((seconds / 60.0f32) / 60.0f32) / 24.0f32) as i32;
        let h = (((seconds / 60.0f32) / 60.0f32) as i32).wrapping_add(d.wrapping_mul(-24));
        if h < 1 {
            format!("{} DAY{}", d, plural(d))
        } else {
            format!("{} DAY{}, {} HOUR{}", d, plural(d), h, plural(h))
        }
    }
}

/// `MJColor8Make(r,g,b,a)`: each channel `(uint8)(int)clamp(v * 255.0f, 0, 255.0f)` (float32).
pub fn mj_color8_make(r: f32, g: f32, b: f32, a: f32) -> MJColor8 {
    let s = 255.0f32;
    let ch = |v: f32| clamp(v * s, 0.0f32, s) as i32 as u8;
    MJColor8 { r: ch(r), g: ch(g), b: ch(b), a: ch(a) }
}

/// `MJColor8Make(r,g,b)`: alpha 1.0f.
pub fn mj_color8_make3(r: f32, g: f32, b: f32) -> MJColor8 {
    mj_color8_make(r, g, b, 1.0f32)
}

const fn c8(r: u8, g: u8, b: u8) -> MJColor8 {
    MJColor8 { r, g, b, a: 255 }
}

/// `oldSkinColors[5]`.
const OLD_SKIN_COLORS: [MJColor8; 5] =
    [c8(249, 209, 188), c8(224, 171, 145), c8(209, 136, 103), c8(162, 97, 69), c8(117, 61, 36)];
/// `oldMaleSingletColors[1][2]`.
const OLD_MALE_SINGLET: [MJColor8; 2] = [c8(255, 255, 255), c8(0, 0, 0)];
/// `oldFemaleSingletColors[2][2]`.
const OLD_FEMALE_SINGLET: [[MJColor8; 2]; 2] =
    [[c8(252, 220, 220), c8(250, 163, 164)], [c8(255, 255, 255), c8(214, 37, 45)]];

/// `getNewOptionsFromOld`: legacy appearance indices -> `BlockheadSkinOptions`.
/// `skin_index % 5` is the C signed remainder; negative `skin_index` is UB in the original, here the table
/// index is clamped into 0..4 (UB-PARITY: not reachable). A `head_index` outside the switch keeps
/// `hair_texture_index = 0` and black hair.
pub fn get_new_options_from_old(
    is_male: bool,
    head_index: i32,
    skin_index: i32,
    hair_style_index: i32,
) -> BlockheadSkinOptions {
    let rem = skin_index % 5;
    let skin_slot = rem.clamp(0, 4) as usize;
    let black = c8(0, 0, 0);
    let mut tex: u8 = 0;
    let mut hair = black;
    let hair_style: u8;
    let (ua, ub);
    if !is_male {
        hair_style = hair_style_index.wrapping_add(1) as u8;
        let set = if rem == 1 { 1 } else { 0 };
        ua = OLD_FEMALE_SINGLET[set][0];
        ub = OLD_FEMALE_SINGLET[set][1];
        match head_index {
            0 => {}
            1 => hair = c8(144, 41, 22),
            2 => hair = c8(238, 197, 146),
            3 => {
                tex = 1;
                hair = c8(70, 37, 24)
            }
            4 => tex = 1,
            5 => {
                tex = 1;
                hair = c8(238, 197, 146)
            }
            6 => {
                tex = 2;
                hair = c8(238, 197, 146)
            }
            7 => {
                tex = 2;
                hair = c8(144, 41, 22)
            }
            8 => {
                tex = 3;
                hair = c8(55, 6, 6)
            }
            _ => {}
        }
    } else {
        hair_style = 0;
        ua = OLD_MALE_SINGLET[0];
        ub = OLD_MALE_SINGLET[1];
        match head_index {
            0 => {}
            1 => tex = 1,
            2 => {
                tex = 2;
                hair = c8(53, 27, 15)
            }
            3 => {
                tex = 3;
                hair = c8(67, 36, 32)
            }
            4 => tex = 4,
            5 => hair = c8(212, 149, 105),
            6 | 7 => {
                tex = 5;
                hair = c8(42, 22, 19)
            }
            _ => {}
        }
    }
    BlockheadSkinOptions {
        is_male: is_male as u8,
        face_index: if skin_index < 5 { 0 } else { 1 },
        hair_texture_index: tex,
        hair_style_index: hair_style,
        skin_color: OLD_SKIN_COLORS[skin_slot],
        hair_color: hair,
        underwear_color_a: ua,
        underwear_color_b: ub,
    }
}

/// `workbenchTypeForItemType`. `WORKBENCH_BASIC_PORTAL` (1) is never returned.
pub fn workbench_type_for_item_type(t: ItemType) -> WorkbenchType {
    let w = match t {
        I::ITEM_CAMPFIRE => 3,
        I::ITEM_WORKBENCH_PORTAL
        | I::ITEM_PORTAL_AMETHYST
        | I::ITEM_PORTAL_SAPPHIRE
        | I::ITEM_PORTAL_EMERALD
        | I::ITEM_PORTAL_RUBY
        | I::ITEM_PORTAL_DIAMOND => 13,
        I::ITEM_EASEL => 24,
        I::ITEM_WORKBENCH_REFINERY => 26,
        I::ITEM_WORKBENCH_SLUICE => 29,
        I::ITEM_WORKBENCH_KILN => 8,
        I::ITEM_WORKBENCH_FURNACE => 9,
        I::ITEM_WORKBENCH_WOOD => 5,
        I::ITEM_WORKBENCH_WEAVE => 4,
        I::ITEM_WORKBENCH_PRESS => 7,
        I::ITEM_WORKBENCH_WORKBENCH => 2,
        I::ITEM_WORKBENCH_TOOL => 6,
        I::ITEM_WORKBENCH_CRAFT => 10,
        I::ITEM_WORKBENCH_MIX => 11,
        I::ITEM_WORKBENCH_DYE => 12,
        I::ITEM_WORKBENCH_METALWORK => 14,
        I::ITEM_STEAM_GENERATOR => 15,
        I::ITEM_WORKBENCH_ELECTRIC_KILN => 16,
        I::ITEM_WORKBENCH_ELECTRIC_FURNACE => 17,
        I::ITEM_WORKBENCH_ELECTRIC_METALWORK_BENCH => 18,
        I::ITEM_WORKBENCH_ELECTRIC_STOVE => 19,
        I::ITEM_SOLAR_PANEL => 20,
        I::ITEM_FLYWHEEL => 21,
        I::ITEM_ARMOR_BENCH => 22,
        I::ITEM_TRAIN_YARD => 23,
        I::ITEM_WORKBENCH_BUILD => 25,
        I::ITEM_WORKBENCH_ELECTRIC_PRESS => 27,
        I::ITEM_COMPOST_BIN => 28,
        I::ITEM_WORKBENCH_EGG_EXTRACTOR => 30,
        I::ITEM_WORKBENCH_PIZZA_OVEN => 31,
        _ => 0,
    };
    WorkbenchType(w)
}

/// `itemTypeForWorkbenchTypeAndLevel`. Only types 1 and 13 look at `level` (1..=5 -> 135..=139,
/// else 134). Unknown types give ITEM_NONE.
pub fn item_type_for_workbench_type_and_level(t: WorkbenchType, level: i32) -> ItemType {
    match t.0 {
        1 | 13 => match level {
            1 => I::ITEM_PORTAL_AMETHYST,
            2 => I::ITEM_PORTAL_SAPPHIRE,
            3 => I::ITEM_PORTAL_EMERALD,
            4 => I::ITEM_PORTAL_RUBY,
            5 => I::ITEM_PORTAL_DIAMOND,
            _ => I::ITEM_WORKBENCH_PORTAL,
        },
        2 => I::ITEM_WORKBENCH_WORKBENCH,
        3 => I::ITEM_CAMPFIRE,
        4 => I::ITEM_WORKBENCH_WEAVE,
        5 => I::ITEM_WORKBENCH_WOOD,
        6 => I::ITEM_WORKBENCH_TOOL,
        7 => I::ITEM_WORKBENCH_PRESS,
        8 => I::ITEM_WORKBENCH_KILN,
        9 => I::ITEM_WORKBENCH_FURNACE,
        10 => I::ITEM_WORKBENCH_CRAFT,
        11 => I::ITEM_WORKBENCH_MIX,
        12 => I::ITEM_WORKBENCH_DYE,
        14 => I::ITEM_WORKBENCH_METALWORK,
        15 => I::ITEM_STEAM_GENERATOR,
        16 => I::ITEM_WORKBENCH_ELECTRIC_KILN,
        17 => I::ITEM_WORKBENCH_ELECTRIC_FURNACE,
        18 => I::ITEM_WORKBENCH_ELECTRIC_METALWORK_BENCH,
        19 => I::ITEM_WORKBENCH_ELECTRIC_STOVE,
        20 => I::ITEM_SOLAR_PANEL,
        21 => I::ITEM_FLYWHEEL,
        22 => I::ITEM_ARMOR_BENCH,
        23 => I::ITEM_TRAIN_YARD,
        24 => I::ITEM_EASEL,
        25 => I::ITEM_WORKBENCH_BUILD,
        26 => I::ITEM_WORKBENCH_REFINERY,
        27 => I::ITEM_WORKBENCH_ELECTRIC_PRESS,
        28 => I::ITEM_COMPOST_BIN,
        29 => I::ITEM_WORKBENCH_SLUICE,
        30 => I::ITEM_WORKBENCH_EGG_EXTRACTOR,
        31 => I::ITEM_WORKBENCH_PIZZA_OVEN,
        _ => I::ITEM_NONE,
    }
}

/// `interactionObjectTypeForItemType`: workbench -> 1, beds -> 3, chests/shelves -> 2,...
pub fn interaction_object_type_for_item_type(t: ItemType) -> InteractionObjectType {
    if workbench_type_for_item_type(t) != WorkbenchType::WORKBENCH_UNDEFINED {
        return InteractionObjectType::INTERACTION_OBJECT_TYPE_WORKBENCH;
    }
    match t {
        I::ITEM_WOOD_BED | I::ITEM_SOFT_BED | I::ITEM_GOLD_BED => InteractionObjectType::INTERACTION_OBJECT_TYPE_BED,
        I::ITEM_SHELF
        | I::ITEM_DISPLAY_CABINET
        | I::ITEM_CHEST
        | I::ITEM_SAFE
        | I::ITEM_CHEST_GOLD
        | I::ITEM_CHEST_PORTAL
        | I::ITEM_CHEST_FEEDER => InteractionObjectType::INTERACTION_OBJECT_TYPE_CHEST,
        I::ITEM_SIGN => InteractionObjectType::INTERACTION_OBJECT_TYPE_SIGN,
        I::ITEM_TRADING_POST => InteractionObjectType::INTERACTION_OBJECT_TYPE_TRADING_POST,
        I::ITEM_TRAIN_STATION => InteractionObjectType::INTERACTION_OBJECT_TYPE_TRAIN_STATION,
        I::ITEM_TRADE_PORTAL => InteractionObjectType::INTERACTION_OBJECT_TYPE_TRADE_PORTAL,
        I::ITEM_OWNERSHIP_SIGN => InteractionObjectType::INTERACTION_OBJECT_TYPE_OWNERSHIP_SIGN,
        I::ITEM_MIRROR => InteractionObjectType::INTERACTION_OBJECT_TYPE_MIRROR,
        _ => InteractionObjectType::INTERACTION_OBJECT_TYPE_UNDEFINED,
    }
}

/// `defaultDataAAndBForItemType`: sowable -> (0x7f, 0x7f) else (0, 0) (an `intpair` in the original).
pub fn default_data_a_and_b_for_item_type(t: ItemType) -> (i32, i32) {
    if crate::items::predicates::item_type_is_sowable(t) {
        (0x7f, 0x7f)
    } else {
        (0, 0)
    }
}

/// `fuelCountForWorkbenchAndLevel`: 3 for CAMPFIRE, PIZZA_OVEN, KILN, FURNACE, METALWORK,
/// STEAM_GENERATOR; else 0 (`level` unused).
pub fn fuel_count_for_workbench_and_level(t: WorkbenchType, _level: i32) -> i32 {
    match t.0 {
        3 | 31 | 8 | 9 | 14 | 15 => 3,
        _ => 0,
    }
}

/// `workbenchRequiresFuel`: `fuelCount > 0`.
pub fn workbench_requires_fuel(t: WorkbenchType, level: i32) -> bool {
    fuel_count_for_workbench_and_level(t, level) > 0
}

/// `workbenchRequiresElectricty` (typo kept from the original).
pub fn workbench_requires_electricty(t: WorkbenchType, _level: i32) -> bool {
    matches!(t.0, 16 | 27 | 17 | 18 | 19 | 26 | 29 | 30)
}

/// `itemTypeForDodoEggBreed`: static map lookup; absent (STANDARD 0, >= 33) -> ITEM_NONE.
pub fn item_type_for_dodo_egg_breed(b: DodoBreed) -> ItemType {
    match b.0 {
        1 => I::ITEM_COBBLESTONE,
        2 => I::ITEM_LIMESTONE_BLOCK,
        3 => I::ITEM_SANDSTONE_BLOCK,
        4 => I::ITEM_MARBLE_BLOCK,
        5 => I::ITEM_RED_MARBLE_BLOCK,
        6 => I::ITEM_LAPIS_BLOCK,
        7 => I::ITEM_DIRT,
        8 => I::ITEM_COMPOST,
        9 => I::ITEM_WOOD,
        10 => I::ITEM_GRAVEL,
        11 => I::ITEM_SAND,
        12 => I::ITEM_BLACK_SAND,
        13 => I::ITEM_GLASS,
        14 => I::ITEM_BLACK_GLASS,
        15 => I::ITEM_CLAY,
        16 => I::ITEM_RED_BRICKS,
        17 => I::ITEM_FLINT,
        18 => I::ITEM_COAL,
        19 => I::ITEM_OIL,
        20 => I::ITEM_FUEL,
        21 => I::ITEM_COPPER_ORE,
        22 => I::ITEM_TIN_ORE,
        23 => I::ITEM_IRON_ORE,
        24 => I::ITEM_GOLD_ORE,
        25 => I::ITEM_TITANIUM_ORE,
        26 => I::ITEM_PLATINUM_ORE,
        27 => I::ITEM_AMETHYST_SHARD,
        28 => I::ITEM_SAPPHIRE_SHARD,
        29 => I::ITEM_EMERALD_SHARD,
        30 => I::ITEM_RUBY_SHARD,
        31 => I::ITEM_DIAMOND_SHARD,
        32 => I::ITEM_RAINBOW_ESSENCE,
        _ => I::ITEM_NONE,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn credit_strings() {
        let cases: &[(f32, &str)] = &[
            (0.0, "NO CREDIT"),
            (0.001, "NO CREDIT"),
            (0.0011, "< 1 MINUTE"),
            (90.0, "< 1 MINUTE"),
            (90.5, "2 MINUTES"),
            (3600.0, "60 MINUTES"),
            (3601.0, "1 HOUR"),
            (7260.0, "2 HOURS, 1 MINUTE"),
            (5400.0, "1 HOUR, 30 MINUTES"),
            (86400.0, "24 HOURS"),
            (86401.0, "1 DAY"),
            (90000.0, "1 DAY, 1 HOUR"),
            (180000.0, "2 DAYS, 2 HOURS"),
            (f32::NAN, "NO CREDIT"),
        ];
        for (s, e) in cases {
            assert_eq!(get_credit_time_string(*s), *e, "{s}");
        }
    }

    #[test]
    fn color_make() {
        let c = mj_color8_make(0.98, 0.82, 0.741, 1.0);
        assert_eq!((c.r, c.g, c.b, c.a), (249, 209, 188, 255));
        let c = mj_color8_make3(2.0, -1.0, 0.5);
        assert_eq!((c.r, c.g, c.b, c.a), (255, 0, 127, 255));
    }

    #[test]
    fn skin_options_tables_match_floats() {
        // the constant tables equal MJColor8Make of the source floats
        let f = |r, g, b| mj_color8_make3(r, g, b);
        assert_eq!(OLD_SKIN_COLORS[0], f(0.98, 0.82, 0.741));
        assert_eq!(OLD_SKIN_COLORS[1], f(0.882, 0.671, 0.569));
        assert_eq!(OLD_SKIN_COLORS[2], f(0.82, 0.537, 0.404));
        assert_eq!(OLD_SKIN_COLORS[3], f(0.639, 0.384, 0.271));
        assert_eq!(OLD_SKIN_COLORS[4], f(0.459, 0.243, 0.145));
        assert_eq!(OLD_FEMALE_SINGLET[0][0], f(0.992, 0.863, 0.863));
        assert_eq!(OLD_FEMALE_SINGLET[0][1], f(0.984, 0.643, 0.647));
        assert_eq!(OLD_FEMALE_SINGLET[1][1], f(0.843, 0.149, 0.18));
        assert_eq!(c8(144, 41, 22), f(0.565, 0.161, 0.09));
        assert_eq!(c8(238, 197, 146), f(0.937, 0.776, 0.576));
        assert_eq!(c8(70, 37, 24), f(0.275, 0.149, 0.098));
        assert_eq!(c8(55, 6, 6), f(0.216, 0.024, 0.024));
        assert_eq!(c8(53, 27, 15), f(0.208, 0.106, 0.059));
        assert_eq!(c8(67, 36, 32), f(0.263, 0.145, 0.129));
        assert_eq!(c8(212, 149, 105), f(0.835, 0.588, 0.412));
        assert_eq!(c8(42, 22, 19), f(0.165, 0.09, 0.078));
    }

    #[test]
    fn skin_options_cases() {
        let o = get_new_options_from_old(false, 3, 6, 255);
        assert_eq!(o.is_male, 0);
        assert_eq!(o.face_index, 1);
        assert_eq!(o.hair_texture_index, 1);
        assert_eq!(o.hair_style_index, 0); // 255 + 1 wraps
        assert_eq!(o.skin_color, OLD_SKIN_COLORS[1]);
        assert_eq!(o.hair_color, c8(70, 37, 24));
        assert_eq!(o.underwear_color_a, c8(255, 255, 255));
        assert_eq!(o.underwear_color_b, c8(214, 37, 45));
        let o = get_new_options_from_old(true, 6, 0, 7);
        assert_eq!((o.hair_texture_index, o.hair_style_index, o.face_index), (5, 0, 0));
        assert_eq!(o.hair_color, c8(42, 22, 19));
        for h in [9, -1] {
            let o = get_new_options_from_old(true, h, 2, 0);
            assert_eq!(o.hair_texture_index, 0);
            assert_eq!(o.hair_color, c8(0, 0, 0));
            let o = get_new_options_from_old(false, h, 2, 0);
            assert_eq!(o.hair_texture_index, 0);
            assert_eq!(o.hair_color, c8(0, 0, 0));
            assert_eq!(o.hair_style_index, 1);
        }
        // male: underwear A white, B black
        let o = get_new_options_from_old(true, 0, 4, 0);
        assert_eq!((o.underwear_color_a, o.underwear_color_b), (c8(255, 255, 255), c8(0, 0, 0)));
    }

    #[test]
    fn workbench_maps() {
        assert_eq!(workbench_type_for_item_type(ItemType(1096)).0, 30);
        assert_eq!(workbench_type_for_item_type(ItemType(1097)).0, 31);
        assert_eq!(workbench_type_for_item_type(ItemType(132)).0, 0);
        for i in 134..=139 {
            assert_eq!(workbench_type_for_item_type(ItemType(i)).0, 13);
        }
        assert_eq!(workbench_type_for_item_type(ItemType(140)).0, 0);
        for wt in 2..=31u32 {
            if wt == 13 {
                continue;
            }
            let it = item_type_for_workbench_type_and_level(WorkbenchType(wt), 1);
            assert_eq!(workbench_type_for_item_type(it).0, wt);
        }
        assert_eq!(item_type_for_workbench_type_and_level(WorkbenchType(1), 3).0, 137);
        assert_eq!(item_type_for_workbench_type_and_level(WorkbenchType(13), 9).0, 134);
        assert_eq!(item_type_for_workbench_type_and_level(WorkbenchType(1), 0).0, 134);
        assert_eq!(item_type_for_workbench_type_and_level(WorkbenchType(0), 1).0, 0);
        assert_eq!(item_type_for_workbench_type_and_level(WorkbenchType(32), 1).0, 0);
        assert_eq!(interaction_object_type_for_item_type(I::ITEM_SOFT_BED).0, 3);
        assert_eq!(interaction_object_type_for_item_type(I::ITEM_SHELF).0, 2);
        assert_eq!(interaction_object_type_for_item_type(ItemType(331)).0, 9);
        assert_eq!(interaction_object_type_for_item_type(I::ITEM_CAMPFIRE).0, 1);
        for wt in 0..40u32 {
            let want = matches!(wt, 3 | 31 | 8 | 9 | 14 | 15);
            assert_eq!(workbench_requires_fuel(WorkbenchType(wt), 0), want);
            assert_eq!(fuel_count_for_workbench_and_level(WorkbenchType(wt), 5), if want { 3 } else { 0 });
            let el = matches!(wt, 16 | 27 | 17 | 18 | 19 | 26 | 29 | 30);
            assert_eq!(workbench_requires_electricty(WorkbenchType(wt), 0), el);
        }
        assert_eq!(default_data_a_and_b_for_item_type(I::ITEM_APPLE), (0x7f, 0x7f));
        assert_eq!(default_data_a_and_b_for_item_type(I::ITEM_STICK), (0, 0));
    }

    #[test]
    fn dodo_egg_table() {
        assert_eq!(item_type_for_dodo_egg_breed(DodoBreed(0)), I::ITEM_NONE);
        assert_eq!(item_type_for_dodo_egg_breed(DodoBreed(1)), I::ITEM_COBBLESTONE);
        assert_eq!(item_type_for_dodo_egg_breed(DodoBreed(32)), I::ITEM_RAINBOW_ESSENCE);
        assert_eq!(item_type_for_dodo_egg_breed(DodoBreed(33)), I::ITEM_NONE);
    }
}
