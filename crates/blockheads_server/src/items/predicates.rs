//! Item-type predicates and durability/decay rates (pure functions over ids).
//!
//! Every predicate is a literal set membership over the ids.
#![allow(clippy::match_like_matches_macro)]

use crate::items::misc::{interaction_object_type_for_item_type, workbench_type_for_item_type};
use crate::types::enums::{InteractionType, NPCType, TileType, WorkbenchType};
use crate::types::item_type::ItemType;

type I = ItemType;

/// `itemTypeIsSolid`.
#[inline]
pub fn item_type_is_solid(t: ItemType) -> bool {
    matches!(
        t,
        I::ITEM_ROCK
            | I::ITEM_DIRT
            | I::ITEM_SAND
            | I::ITEM_BLACK_SAND
            | I::ITEM_WOOD
            | I::ITEM_WOODEN_PLATFORM
            | I::ITEM_COBBLESTONE
            | I::ITEM_RED_BRICKS
            | I::ITEM_GRAVEL
            | I::ITEM_LIMESTONE
            | I::ITEM_LIMESTONE_BLOCK
            | I::ITEM_PLASTER
            | I::ITEM_LUMINOUS_PLASTER
            | I::ITEM_MARBLE
            | I::ITEM_MARBLE_BLOCK
            | I::ITEM_SANDSTONE
            | I::ITEM_SANDSTONE_BLOCK
            | I::ITEM_RED_MARBLE
            | I::ITEM_RED_MARBLE_BLOCK
            | I::ITEM_FLAX_MAT
            | I::ITEM_FLAX_MAT_YELLOW
            | I::ITEM_FLAX_MAT_RED
            | I::ITEM_GLASS
            | I::ITEM_BLACK_GLASS
            | I::ITEM_ICE
            | I::ITEM_DOOR
            | I::ITEM_TRAPDOOR
            | I::ITEM_IRON_DOOR
            | I::ITEM_IRON_TRAPDOOR
            | I::ITEM_GOLD_BLOCK
            | I::ITEM_AMETHYST_BLOCK
            | I::ITEM_SAPPHIRE_BLOCK
            | I::ITEM_EMERALD_BLOCK
            | I::ITEM_RUBY_BLOCK
            | I::ITEM_DIAMOND_BLOCK
            | I::ITEM_PLATINUM_BLOCK
            | I::ITEM_CARBON_FIBER_BLOCK
            | I::ITEM_TITANIUM_BLOCK
            | I::ITEM_COPPER_BLOCK
            | I::ITEM_TIN_BLOCK
            | I::ITEM_BRONZE_BLOCK
            | I::ITEM_IRON_BLOCK
            | I::ITEM_STEEL_BLOCK
            | I::ITEM_LAPIS
            | I::ITEM_LAPIS_BLOCK
            | I::ITEM_BASALT
            | I::ITEM_BASALT_UNMINED
            | I::ITEM_COMPOST
    )
}

/// `itemTypeIsSowable`.
#[inline]
pub fn item_type_is_sowable(t: ItemType) -> bool {
    matches!(
        t,
        I::ITEM_MANGO
            | I::ITEM_APPLE
            | I::ITEM_PRICKLY_PEAR
            | I::ITEM_MAPLE_SEED
            | I::ITEM_PINE_CONE
            | I::ITEM_FLAX_SEED
            | I::ITEM_COCONUT
            | I::ITEM_ORANGE
            | I::ITEM_LIME
            | I::ITEM_SUNFLOWER_SEED
            | I::ITEM_CARROT
            | I::ITEM_TULIP_BULB
            | I::ITEM_TULIP_SEED
            | I::ITEM_CORN
            | I::ITEM_WHEAT
            | I::ITEM_TOMATO
            | I::ITEM_CHILLI
            | I::ITEM_KELP
            | I::ITEM_VINE
            | I::ITEM_CHERRY
            | I::ITEM_COFFEE_CHERRY
    )
}

/// `itemTypeIsTwoBlocksWide`.
#[inline]
pub fn item_type_is_two_blocks_wide(t: ItemType) -> bool {
    matches!(t, I::ITEM_WOOD_BED | I::ITEM_SOFT_BED | I::ITEM_GOLD_BED)
}

/// `itemTypeOccupiesForegroundContents`: torch || {SIGN, OWNERSHIP_SIGN, DODO_EGG, RAIL} || painting.
pub fn item_type_occupies_foreground_contents(t: ItemType) -> bool {
    item_type_is_torch(t)
        || matches!(t, I::ITEM_SIGN | I::ITEM_OWNERSHIP_SIGN | I::ITEM_DODO_EGG | I::ITEM_RAIL)
        || item_type_is_painting(t)
}

/// `interactionObjectItemTypeOccupiesForeground`: {SIGN, OWNERSHIP_SIGN}.
pub fn interaction_object_item_type_occupies_foreground(t: ItemType) -> bool {
    matches!(t, I::ITEM_SIGN | I::ITEM_OWNERSHIP_SIGN)
}

/// `itemTypeIsTrainCar`.
#[inline]
pub fn item_type_is_train_car(t: ItemType) -> bool {
    matches!(t, I::ITEM_HANDCAR | I::ITEM_STEAM_TRAIN | I::ITEM_FREIGHT_CAR | I::ITEM_PASSENGER_CAR)
}

/// `itemTypeOccupiesBackgroundContents`.
#[inline]
pub fn item_type_occupies_background_contents(t: ItemType) -> bool {
    matches!(t, I::ITEM_WINDOW | I::ITEM_BLACK_WINDOW)
}

/// `itemTypeIsBlock`: `(solid && t not in {DOOR, TRAPDOOR, IRON_DOOR, IRON_TRAPDOOR}) || t in OBJ`.
pub fn item_type_is_block(t: ItemType) -> bool {
    (item_type_is_solid(t) && !matches!(t, I::ITEM_DOOR | I::ITEM_TRAPDOOR | I::ITEM_IRON_DOOR | I::ITEM_IRON_TRAPDOOR))
        || matches!(
            t,
            I::ITEM_WORKBENCH_WORKBENCH
                | I::ITEM_WORKBENCH_TOOL
                | I::ITEM_WORKBENCH_FURNACE
                | I::ITEM_STEAM_GENERATOR
                | I::ITEM_SOLAR_PANEL
                | I::ITEM_FLYWHEEL
                | I::ITEM_ARMOR_BENCH
                | I::ITEM_WORKBENCH_METALWORK
                | I::ITEM_WORKBENCH_WOOD
                | I::ITEM_WORKBENCH_KILN
                | I::ITEM_WORKBENCH_PIZZA_OVEN
                | I::ITEM_WORKBENCH_ELECTRIC_KILN
                | I::ITEM_WORKBENCH_ELECTRIC_PRESS
                | I::ITEM_WORKBENCH_ELECTRIC_FURNACE
                | I::ITEM_WORKBENCH_ELECTRIC_METALWORK_BENCH
                | I::ITEM_WORKBENCH_ELECTRIC_STOVE
                | I::ITEM_WORKBENCH_EGG_EXTRACTOR
                | I::ITEM_WORKBENCH_WEAVE
                | I::ITEM_WORKBENCH_PRESS
                | I::ITEM_WORKBENCH_CRAFT
                | I::ITEM_WORKBENCH_BUILD
                | I::ITEM_WORKBENCH_MIX
                | I::ITEM_WORKBENCH_DYE
                | I::ITEM_TRAIN_YARD
                | I::ITEM_COMPOST_BIN
                | I::ITEM_CHEST
                | I::ITEM_CHEST_GOLD
                | I::ITEM_CHEST_FEEDER
                | I::ITEM_CHEST_PORTAL
                | I::ITEM_ELEVATOR_MOTOR
                | I::ITEM_ELEVATOR_SHAFT
                | I::ITEM_SAFE
        )
}

/// `itemTypeIsPlacableOnTrees`.
#[inline]
pub fn item_type_is_placable_on_trees(t: ItemType) -> bool {
    matches!(
        t,
        I::ITEM_BASIC_TORCH
            | I::ITEM_ICE_TORCH
            | I::ITEM_CLAY_LANTERN_LIT
            | I::ITEM_STEEL_LANTERN
            | I::ITEM_STEEL_DOWNLIGHT
            | I::ITEM_STEEL_UPLIGHT
            | I::ITEM_RUBY
            | I::ITEM_EMERALD
            | I::ITEM_SAPPHIRE
            | I::ITEM_AMETHYST
            | I::ITEM_DIAMOND
            | I::ITEM_LADDER
    )
}

/// `itemTypeIsValidFillItem`. SIGNED compares: false for `344 <= t <= 1023` and `t > 1105`;
/// negative ids are TRUE unless listed.
pub fn item_type_is_valid_fill_item(t: ItemType) -> bool {
    let v = t.0 as i32;
    if v > 343 && v < 1024 {
        return false;
    }
    // Mods (not in the original): the items of the loaded content mods are real item types above the vanilla ones.
    if v > 1105 && !crate::mods::is_mod_item(t.0) {
        return false;
    }
    !matches!(
        t,
        I::ITEM_NONE
            | I::ITEM_TIME_CRYSTAL
            | I::ITEM_CLOTHING_SLOT
            | I::ITEM_DEFAULT_MARKER
            | I::ITEM_EMPTY_MARKER
            | I::ITEM_UNDEFINED_C
            | I::ITEM_UNDEFINED_D
            | I::ITEM_UNDEFINED_E
            | I::ITEM_BLOCKHEAD
            | I::ITEM_DOUBLE_TIME
            | I::ITEM_CLAY_JUG
            | I::ITEM_FLAX_MAT
            | I::ITEM_FLAX_MAT_RED
            | I::ITEM_FLAX_MAT_YELLOW
            | I::ITEM_ROCK
            | I::ITEM_LAPIS
            | I::ITEM_LIMESTONE
            | I::ITEM_SANDSTONE
            | I::ITEM_MARBLE
            | I::ITEM_RED_MARBLE
            | I::ITEM_WORKBENCH_UPGRADE_OLD_UNUSED
            | I::ITEM_TRADING_POST_OLD
    )
}

/// `itemTypeIsLightEmittingBlock`.
pub fn item_type_is_light_emitting_block(t: ItemType) -> bool {
    item_type_is_gem_block(t) || t == I::ITEM_LUMINOUS_PLASTER || t == I::ITEM_GOLD_BLOCK
}

/// `itemTypeIsGemBlock`.
#[inline]
pub fn item_type_is_gem_block(t: ItemType) -> bool {
    matches!(
        t,
        I::ITEM_AMETHYST_BLOCK
            | I::ITEM_SAPPHIRE_BLOCK
            | I::ITEM_EMERALD_BLOCK
            | I::ITEM_RUBY_BLOCK
            | I::ITEM_DIAMOND_BLOCK
    )
}

/// `itemTypeIsTorch`.
#[inline]
pub fn item_type_is_torch(t: ItemType) -> bool {
    matches!(
        t,
        I::ITEM_BASIC_TORCH
            | I::ITEM_ICE_TORCH
            | I::ITEM_CLAY_LANTERN_LIT
            | I::ITEM_STEEL_LANTERN
            | I::ITEM_STEEL_DOWNLIGHT
            | I::ITEM_STEEL_UPLIGHT
            | I::ITEM_RUBY
            | I::ITEM_EMERALD
            | I::ITEM_SAPPHIRE
            | I::ITEM_AMETHYST
            | I::ITEM_DIAMOND
            | I::ITEM_AMETHYST_CHANDELIER
            | I::ITEM_SAPPHIRE_CHANDELIER
            | I::ITEM_EMERALD_CHANDELIER
            | I::ITEM_RUBY_CHANDELIER
            | I::ITEM_DIAMOND_CHANDELIER
            | I::ITEM_SHARK_JAW
    )
}

/// `itemTypeCanBeColored`.
#[inline]
pub fn item_type_can_be_colored(t: ItemType) -> bool {
    matches!(
        t,
        I::ITEM_LINEN_SHIRT
            | I::ITEM_LINEN_PANTS
            | I::ITEM_LINEN_HAT
            | I::ITEM_LINEN_CAP
            | I::ITEM_LEATHER_JACKET
            | I::ITEM_LEATHER_PANTS
            | I::ITEM_LEATHER_BOOTS
            | I::ITEM_FUR_COAT
            | I::ITEM_FUR_BOOTS
            | I::ITEM_SOFT_BED
            | I::ITEM_GOLD_BED
    )
}

/// `itemTypeIsPainting`.
#[inline]
pub fn item_type_is_painting(t: ItemType) -> bool {
    matches!(
        t,
        I::ITEM_PAINTING_L_S
            | I::ITEM_PAINTING_L_L
            | I::ITEM_PAINTING_L_P
            | I::ITEM_PAINTING_M_S
            | I::ITEM_PAINTING_M_L
            | I::ITEM_PAINTING_M_P
            | I::ITEM_PAINTING_S_S
            | I::ITEM_PAINTING_S_L
            | I::ITEM_PAINTING_S_P
    )
}

/// `itemTypeSubItemsCanBeModifiedWhileCarried`: BASKET or CLOTHING_SLOT.
pub fn item_type_sub_items_can_be_modified_while_carried(t: ItemType) -> bool {
    t == I::ITEM_BASKET || t == I::ITEM_CLOTHING_SLOT
}

/// `itemTypeIsStackable`.
pub fn item_type_is_stackable(t: ItemType, data_b_left: u16, data_b_right: u16) -> bool {
    if item_type_sub_items_can_be_modified_while_carried(t) {
        return false;
    }
    if data_b_left == data_b_right {
        return true;
    }
    if t == I::ITEM_PAINT || t == I::ITEM_DODO_EGG {
        return false;
    }
    if item_type_can_be_colored(t) {
        return (data_b_left != 0) == (data_b_right != 0);
    }
    true
}

/// `inventorySubItemsCountForItemType`: number of sub-slots (0 for non-containers).
pub fn inventory_sub_items_count_for_item_type(t: ItemType) -> i32 {
    match t {
        I::ITEM_CLOTHING_SLOT | I::ITEM_BASKET | I::ITEM_SHELF | I::ITEM_DISPLAY_CABINET => 4,
        I::ITEM_CLAY_JUG => 2,
        I::ITEM_CHEST | I::ITEM_SAFE | I::ITEM_CHEST_GOLD | I::ITEM_CHEST_PORTAL | I::ITEM_CHEST_FEEDER => 16,
        _ => 0,
    }
}

/// `workbenchTypeIsDoubleHeight`: {BASIC_PORTAL 1, PLACED_PORTAL 13, EASEL 24, REFINERY 26}.
pub fn workbench_type_is_double_height(t: WorkbenchType) -> bool {
    matches!(t.0, 1 | 13 | 24 | 26)
}

/// `interactionTypeDisallowedOnProtectedTiles`.
pub fn interaction_type_disallowed_on_protected_tiles(t: InteractionType) -> bool {
    matches!(t.0, 1 | 2 | 3 | 6 | 7 | 9 | 14 | 15 | 16 | 23 | 24)
}

/// `npcTypeFromCageItemType`. UNICORN_CAGE maps to NPC_DONKEY.
pub fn npc_type_from_cage_item_type(t: ItemType) -> NPCType {
    match t {
        I::ITEM_DODO_CAGE => NPCType::NPC_DODO,
        I::ITEM_DONKEY_CAGE => NPCType::NPC_DONKEY,
        I::ITEM_YAK_CAGE => NPCType::NPC_YAK,
        I::ITEM_DROPBEAR_CAGE => NPCType::NPC_DROPBEAR,
        I::ITEM_SCORPION_CAGE => NPCType::NPC_SCORPION,
        I::ITEM_UNICORN_CAGE => NPCType::NPC_DONKEY,
        _ => NPCType::NPC_NOTHING,
    }
}

/// `itemTypeIsWorkbench`: `workbenchTypeForItemType(t) != WORKBENCH_UNDEFINED`.
pub fn item_type_is_workbench(t: ItemType) -> bool {
    workbench_type_for_item_type(t) != WorkbenchType::WORKBENCH_UNDEFINED
}

/// `itemTypeIsInteractionObject`: `interactionObjectTypeForItemType(t) != 0`.
pub fn item_type_is_interaction_object(t: ItemType) -> bool {
    interaction_object_type_for_item_type(t).0 != 0
}

/// `itemTypeIsGem`.
#[inline]
pub fn item_type_is_gem(t: ItemType) -> bool {
    matches!(t, I::ITEM_AMETHYST | I::ITEM_SAPPHIRE | I::ITEM_EMERALD | I::ITEM_RUBY | I::ITEM_DIAMOND)
}

/// `itemTypeIsStairs`.
#[inline]
pub fn item_type_is_stairs(t: ItemType) -> bool {
    matches!(
        t,
        I::ITEM_STONE_STAIRS
            | I::ITEM_LIMESTONE_STAIRS
            | I::ITEM_PLASTER_STAIRS
            | I::ITEM_AMETHYST_STAIRS
            | I::ITEM_SAPPHIRE_STAIRS
            | I::ITEM_EMERALD_STAIRS
            | I::ITEM_RUBY_STAIRS
            | I::ITEM_DIAMOND_STAIRS
            | I::ITEM_MARBLE_STAIRS
            | I::ITEM_SANDSTONE_STAIRS
            | I::ITEM_RED_MARBLE_STAIRS
            | I::ITEM_LAPIS_STAIRS
            | I::ITEM_BASALT_STAIRS
            | I::ITEM_COPPER_STAIRS
            | I::ITEM_TIN_STAIRS
            | I::ITEM_BRONZE_STAIRS
            | I::ITEM_IRON_STAIRS
            | I::ITEM_STEEL_STAIRS
            | I::ITEM_GOLD_STAIRS
            | I::ITEM_WOOD_STAIRS
            | I::ITEM_BRICK_STAIRS
            | I::ITEM_GLASS_STAIRS
            | I::ITEM_BLACK_GLASS_STAIRS
            | I::ITEM_PLATINUM_STAIRS
            | I::ITEM_TITANIUM_STAIRS
            | I::ITEM_CARBON_FIBER_STAIRS
            | I::ITEM_ICE_STAIRS
    )
}

/// `itemTypeIsValidInventoryItem`: valid fill item and not FREEZE_WATER / MELT_WATER / BASALT_UNMINED.
pub fn item_type_is_valid_inventory_item(t: ItemType) -> bool {
    item_type_is_valid_fill_item(t)
        && t != I::ITEM_FREEZE_WATER
        && t != I::ITEM_MELT_WATER
        && t != I::ITEM_BASALT_UNMINED
}

/// `itemTypeIsLiquid`: always false.
pub fn item_type_is_liquid(_t: ItemType) -> bool {
    false
}

/// `itemTypeIsMoney`.
#[inline]
pub fn item_type_is_money(t: ItemType) -> bool {
    matches!(t, I::ITEM_COPPER_COIN | I::ITEM_GOLD_COIN | I::ITEM_PLATINUM_COIN)
}

/// `itemTypeCarriesLiquids`: always false.
pub fn item_type_carries_liquids(_t: ItemType) -> bool {
    false
}

/// `itemTypeIsPigment`.
#[inline]
pub fn item_type_is_pigment(t: ItemType) -> bool {
    matches!(
        t,
        I::ITEM_BLACK_PIGMENT
            | I::ITEM_WHITE_PIGMENT
            | I::ITEM_RED_PIGMENT
            | I::ITEM_BLUE_PIGMENT
            | I::ITEM_YELLOW_PIGMENT
            | I::ITEM_PIGMENT_EMERALD_GREEN
            | I::ITEM_PIGMENT_TYRIAN_PURPLE
            | I::ITEM_PIGMENT_COPPER_BLUE
    )
}

/// `itemTypeCanBeWornOnHead`.
#[inline]
pub fn item_type_can_be_worn_on_head(t: ItemType) -> bool {
    matches!(
        t,
        I::ITEM_LINEN_CAP
            | I::ITEM_LINEN_HAT
            | I::ITEM_SANTA_HAT
            | I::ITEM_TIN_FOIL_HAT
            | I::ITEM_TIN_HELMET
            | I::ITEM_IRON_HELMET
            | I::ITEM_TITANIUM_HELMET
            | I::ITEM_CARBON_FIBER_HELMET
            | I::ITEM_ICE_HELMET
            | I::ITEM_FRUIT_HAT
    )
}

/// `itemTypeCanBeWornOnTorso`.
#[inline]
pub fn item_type_can_be_worn_on_torso(t: ItemType) -> bool {
    matches!(
        t,
        I::ITEM_LINEN_SHIRT
            | I::ITEM_LEATHER_JACKET
            | I::ITEM_TIN_CHEST_PLATE
            | I::ITEM_IRON_CHEST_PLATE
            | I::ITEM_JETPACK
            | I::ITEM_TITANIUM_CHEST_PLATE
            | I::ITEM_CARBON_FIBER_CHEST_PLATE
            | I::ITEM_ICE_CHEST_PLATE
            | I::ITEM_FUR_COAT
    )
}

/// `itemTypeCanBeWornOnLegs`.
#[inline]
pub fn item_type_can_be_worn_on_legs(t: ItemType) -> bool {
    matches!(
        t,
        I::ITEM_LINEN_PANTS
            | I::ITEM_LEATHER_PANTS
            | I::ITEM_TIN_PANTS
            | I::ITEM_IRON_PANTS
            | I::ITEM_TITANIUM_PANTS
            | I::ITEM_CARBON_FIBER_PANTS
            | I::ITEM_ICE_PANTS
            | I::ITEM_GRASS_SKIRT
    )
}

/// `itemTypeCanBeWornOnFeet`.
#[inline]
pub fn item_type_can_be_worn_on_feet(t: ItemType) -> bool {
    matches!(
        t,
        I::ITEM_LEATHER_BOOTS
            | I::ITEM_FUR_BOOTS
            | I::ITEM_TIN_BOOTS
            | I::ITEM_IRON_BOOTS
            | I::ITEM_TITANIUM_BOOTS
            | I::ITEM_CARBON_FIBER_BOOTS
            | I::ITEM_ICE_BOOTS
            | I::ITEM_SOUTH_POLE_BOOTS
    )
}

/// `itemTypeCanBeWorn`: legs || torso || feet || head.
pub fn item_type_can_be_worn(t: ItemType) -> bool {
    item_type_can_be_worn_on_legs(t)
        || item_type_can_be_worn_on_torso(t)
        || item_type_can_be_worn_on_feet(t)
        || item_type_can_be_worn_on_head(t)
}

/// `itemTypeForColoredOverlay`: SOFT_BED / GOLD_BED -> BED_QUILT_TEXTURE else NONE.
pub fn item_type_for_colored_overlay(t: ItemType) -> ItemType {
    if t == I::ITEM_SOFT_BED || t == I::ITEM_GOLD_BED {
        I::ITEM_BED_QUILT_TEXTURE
    } else {
        I::ITEM_NONE
    }
}

/// `xOffsetForItemType`: CARROT_STICK / FISHING_ROD -> -0.1875, else 0.
pub fn x_offset_for_item_type(t: ItemType) -> f32 {
    if t == I::ITEM_CARROT_STICK || t == I::ITEM_FISHING_ROD {
        -0.1875f32
    } else {
        0.0f32
    }
}

/// `itemTypeRequiresOwnershipToRemove`.
pub fn item_type_requires_ownership_to_remove(t: ItemType) -> bool {
    matches!(
        t,
        I::ITEM_SAFE
            | I::ITEM_DISPLAY_CABINET
            | I::ITEM_IRON_DOOR
            | I::ITEM_IRON_TRAPDOOR
            | I::ITEM_TRADING_POST
            | I::ITEM_CHEST
            | I::ITEM_CHEST_GOLD
            | I::ITEM_CHEST_FEEDER
            | I::ITEM_CHEST_PORTAL
            | I::ITEM_TRADE_PORTAL
            | I::ITEM_FREIGHT_CAR
            | I::ITEM_WOOD_BED
            | I::ITEM_MIRROR
            | I::ITEM_GOLD_BED
            | I::ITEM_HANDCAR
            | I::ITEM_PASSENGER_CAR
            | I::ITEM_SIGN
            | I::ITEM_OWNERSHIP_SIGN
            | I::ITEM_SHELF
            | I::ITEM_SOFT_BED
            | I::ITEM_STEAM_TRAIN
            | I::ITEM_TRAIN_STATION
            | I::ITEM_LADDER
            | I::ITEM_WINDOW
            | I::ITEM_BLACK_WINDOW
            | I::ITEM_DOOR
            | I::ITEM_GATE
            | I::ITEM_TRAPDOOR
            | I::ITEM_ELEVATOR_MOTOR
            | I::ITEM_ELEVATOR_SHAFT
            | I::ITEM_COPPER_WIRE
    ) || item_type_is_workbench(t)
        || item_type_is_torch(t)
        || item_type_is_stairs(t)
        || item_type_is_column(t)
        || item_type_is_painting(t)
}

/// `itemTypeIsFullCage`.
#[inline]
pub fn item_type_is_full_cage(t: ItemType) -> bool {
    matches!(
        t,
        I::ITEM_DODO_CAGE
            | I::ITEM_YAK_CAGE
            | I::ITEM_DONKEY_CAGE
            | I::ITEM_UNICORN_CAGE
            | I::ITEM_DROPBEAR_CAGE
            | I::ITEM_SCORPION_CAGE
    )
}

/// `itemTypeIsPlacableOnBackWall`.
pub fn item_type_is_placable_on_back_wall(t: ItemType) -> bool {
    item_type_is_placable_on_wall(t)
        || matches!(
            t,
            I::ITEM_WINDOW
                | I::ITEM_BLACK_WINDOW
                | I::ITEM_SHARK_JAW
                | I::ITEM_SHELF
                | I::ITEM_DISPLAY_CABINET
                | I::ITEM_WORKBENCH_WORKBENCH
                | I::ITEM_WORKBENCH_PORTAL
                | I::ITEM_PORTAL_AMETHYST
                | I::ITEM_PORTAL_SAPPHIRE
                | I::ITEM_PORTAL_EMERALD
                | I::ITEM_PORTAL_RUBY
                | I::ITEM_PORTAL_DIAMOND
                | I::ITEM_TRADE_PORTAL
                | I::ITEM_WORKBENCH_TOOL
                | I::ITEM_WORKBENCH_FURNACE
                | I::ITEM_STEAM_GENERATOR
                | I::ITEM_SOLAR_PANEL
                | I::ITEM_FLYWHEEL
                | I::ITEM_WORKBENCH_METALWORK
                | I::ITEM_WORKBENCH_WOOD
                | I::ITEM_WORKBENCH_KILN
                | I::ITEM_WORKBENCH_PIZZA_OVEN
                | I::ITEM_WORKBENCH_ELECTRIC_KILN
                | I::ITEM_WORKBENCH_ELECTRIC_PRESS
                | I::ITEM_WORKBENCH_ELECTRIC_FURNACE
                | I::ITEM_WORKBENCH_ELECTRIC_METALWORK_BENCH
                | I::ITEM_WORKBENCH_ELECTRIC_STOVE
                | I::ITEM_WORKBENCH_EGG_EXTRACTOR
                | I::ITEM_WORKBENCH_WEAVE
                | I::ITEM_WORKBENCH_PRESS
                | I::ITEM_WORKBENCH_CRAFT
                | I::ITEM_WORKBENCH_BUILD
                | I::ITEM_WORKBENCH_MIX
                | I::ITEM_WORKBENCH_DYE
                | I::ITEM_TRAIN_YARD
                | I::ITEM_COMPOST_BIN
                | I::ITEM_CHEST
                | I::ITEM_TRADING_POST
                | I::ITEM_CHEST_GOLD
                | I::ITEM_CHEST_FEEDER
                | I::ITEM_CHEST_PORTAL
                | I::ITEM_SAFE
        )
        || item_type_is_painting(t)
        || item_type_is_stairs(t)
}

/// `itemTypeCanBePlacedOnBackgroundRock`: not WINDOW / BLACK_WINDOW.
pub fn item_type_can_be_placed_on_background_rock(t: ItemType) -> bool {
    !(t == I::ITEM_WINDOW || t == I::ITEM_BLACK_WINDOW)
}

/// `itemTypeIsSowableInTileType`. Prickly pear is not sowable on plain dirt; KELP is never
/// accepted here.
pub fn item_type_is_sowable_in_tile_type(t: ItemType, tile: TileType) -> bool {
    if !item_type_is_sowable(t) {
        return false;
    }
    let compost =
        matches!(tile, TileType::TILE_COMPOST | TileType::TILE_COMPOST_GRASS | TileType::TILE_COMPOST_GRASS_FROZEN);
    if compost || matches!(tile, TileType::TILE_DIRT | TileType::TILE_DIRT_GRASS | TileType::TILE_DIRT_GRASS_FROZEN) {
        if matches!(
            t,
            I::ITEM_MANGO
                | I::ITEM_APPLE
                | I::ITEM_MAPLE_SEED
                | I::ITEM_PINE_CONE
                | I::ITEM_FLAX_SEED
                | I::ITEM_COCONUT
                | I::ITEM_ORANGE
                | I::ITEM_LIME
                | I::ITEM_SUNFLOWER_SEED
                | I::ITEM_CARROT
                | I::ITEM_TULIP_BULB
                | I::ITEM_TULIP_SEED
                | I::ITEM_CORN
                | I::ITEM_WHEAT
                | I::ITEM_TOMATO
                | I::ITEM_CHILLI
                | I::ITEM_CHERRY
                | I::ITEM_COFFEE_CHERRY
                | I::ITEM_VINE
        ) {
            return true;
        }
        if t == I::ITEM_PRICKLY_PEAR && compost {
            return true;
        }
        return false;
    } else if matches!(tile, TileType::TILE_SAND | TileType::TILE_SAND_BEACH | TileType::TILE_BLACK_SAND)
        && matches!(t, I::ITEM_PRICKLY_PEAR | I::ITEM_FLAX_SEED | I::ITEM_COCONUT)
    {
        return true;
    }
    false
}

/// `sowableItemTypeOccupiesForeground`.
#[inline]
pub fn sowable_item_type_occupies_foreground(t: ItemType) -> bool {
    matches!(
        t,
        I::ITEM_FLAX_SEED
            | I::ITEM_SUNFLOWER_SEED
            | I::ITEM_CARROT
            | I::ITEM_TULIP_BULB
            | I::ITEM_TULIP_SEED
            | I::ITEM_CORN
            | I::ITEM_WHEAT
            | I::ITEM_TOMATO
            | I::ITEM_CHILLI
            | I::ITEM_KELP
            | I::ITEM_VINE
    )
}

/// `itemTypeIsPlacableInWater`.
pub fn item_type_is_placable_in_water(t: ItemType) -> bool {
    item_type_is_solid(t)
        || matches!(
            t,
            I::ITEM_STEEL_LANTERN
                | I::ITEM_STEEL_DOWNLIGHT
                | I::ITEM_STEEL_UPLIGHT
                | I::ITEM_RUBY
                | I::ITEM_SAPPHIRE
                | I::ITEM_EMERALD
                | I::ITEM_AMETHYST
                | I::ITEM_DIAMOND
                | I::ITEM_FISH_BUCKET
                | I::ITEM_SHARK_BUCKET
                | I::ITEM_WATER_BUCKET
                | I::ITEM_GATE
        )
        || item_type_is_column(t)
        || item_type_is_stairs(t)
}

/// `itemTypeIsColumn`.
#[inline]
pub fn item_type_is_column(t: ItemType) -> bool {
    matches!(
        t,
        I::ITEM_STONE_COLUMN
            | I::ITEM_LIMESTONE_COLUMN
            | I::ITEM_PLASTER_COLUMN
            | I::ITEM_AMETHYST_COLUMN
            | I::ITEM_SAPPHIRE_COLUMN
            | I::ITEM_EMERALD_COLUMN
            | I::ITEM_RUBY_COLUMN
            | I::ITEM_DIAMOND_COLUMN
            | I::ITEM_MARBLE_COLUMN
            | I::ITEM_SANDSTONE_COLUMN
            | I::ITEM_RED_MARBLE_COLUMN
            | I::ITEM_LAPIS_COLUMN
            | I::ITEM_BASALT_COLUMN
            | I::ITEM_COPPER_COLUMN
            | I::ITEM_TIN_COLUMN
            | I::ITEM_BRONZE_COLUMN
            | I::ITEM_IRON_COLUMN
            | I::ITEM_STEEL_COLUMN
            | I::ITEM_GOLD_COLUMN
            | I::ITEM_WOOD_COLUMN
            | I::ITEM_BRICK_COLUMN
            | I::ITEM_GLASS_COLUMN
            | I::ITEM_BLACK_GLASS_COLUMN
            | I::ITEM_PLATINUM_COLUMN
            | I::ITEM_TITANIUM_COLUMN
            | I::ITEM_CARBON_FIBER_COLUMN
            | I::ITEM_ICE_COLUMN
    )
}

/// `itemTypeIsHangable`.
#[inline]
pub fn item_type_is_hangable(t: ItemType) -> bool {
    matches!(
        t,
        I::ITEM_ROCK
            | I::ITEM_DIRT
            | I::ITEM_SAND
            | I::ITEM_BLACK_SAND
            | I::ITEM_WOOD
            | I::ITEM_WOODEN_PLATFORM
            | I::ITEM_COBBLESTONE
            | I::ITEM_RED_BRICKS
            | I::ITEM_GRAVEL
            | I::ITEM_LIMESTONE
            | I::ITEM_LIMESTONE_BLOCK
            | I::ITEM_PLASTER
            | I::ITEM_LUMINOUS_PLASTER
            | I::ITEM_MARBLE
            | I::ITEM_MARBLE_BLOCK
            | I::ITEM_SANDSTONE
            | I::ITEM_SANDSTONE_BLOCK
            | I::ITEM_RED_MARBLE
            | I::ITEM_RED_MARBLE_BLOCK
            | I::ITEM_LADDER
            | I::ITEM_ELEVATOR_SHAFT
            | I::ITEM_ELEVATOR_MOTOR
            | I::ITEM_FLAX_MAT
            | I::ITEM_FLAX_MAT_YELLOW
            | I::ITEM_FLAX_MAT_RED
            | I::ITEM_GLASS
            | I::ITEM_BLACK_GLASS
            | I::ITEM_ICE
            | I::ITEM_TRAPDOOR
            | I::ITEM_IRON_TRAPDOOR
            | I::ITEM_GOLD_BLOCK
            | I::ITEM_PLATINUM_BLOCK
            | I::ITEM_TITANIUM_BLOCK
            | I::ITEM_AMETHYST_BLOCK
            | I::ITEM_SAPPHIRE_BLOCK
            | I::ITEM_EMERALD_BLOCK
            | I::ITEM_RUBY_BLOCK
            | I::ITEM_DIAMOND_BLOCK
            | I::ITEM_CARBON_FIBER_BLOCK
            | I::ITEM_COPPER_BLOCK
            | I::ITEM_TIN_BLOCK
            | I::ITEM_BRONZE_BLOCK
            | I::ITEM_IRON_BLOCK
            | I::ITEM_STEEL_BLOCK
            | I::ITEM_LAPIS
            | I::ITEM_LAPIS_BLOCK
            | I::ITEM_WATER_BUCKET
            | I::ITEM_AMETHYST_CHANDELIER
            | I::ITEM_SAPPHIRE_CHANDELIER
            | I::ITEM_EMERALD_CHANDELIER
            | I::ITEM_RUBY_CHANDELIER
            | I::ITEM_DIAMOND_CHANDELIER
            | I::ITEM_BASALT
            | I::ITEM_BASALT_UNMINED
            | I::ITEM_COMPOST
            | I::ITEM_SIGN
            | I::ITEM_OWNERSHIP_SIGN
            | I::ITEM_COPPER_WIRE
            | I::ITEM_WORKBENCH_WORKBENCH
            | I::ITEM_WORKBENCH_TOOL
            | I::ITEM_WORKBENCH_FURNACE
            | I::ITEM_STEAM_GENERATOR
            | I::ITEM_SOLAR_PANEL
            | I::ITEM_FLYWHEEL
            | I::ITEM_ARMOR_BENCH
            | I::ITEM_WORKBENCH_METALWORK
            | I::ITEM_WORKBENCH_WOOD
            | I::ITEM_WORKBENCH_KILN
            | I::ITEM_WORKBENCH_PIZZA_OVEN
            | I::ITEM_WORKBENCH_ELECTRIC_KILN
            | I::ITEM_WORKBENCH_ELECTRIC_PRESS
            | I::ITEM_WORKBENCH_ELECTRIC_FURNACE
            | I::ITEM_WORKBENCH_ELECTRIC_METALWORK_BENCH
            | I::ITEM_WORKBENCH_ELECTRIC_STOVE
            | I::ITEM_WORKBENCH_EGG_EXTRACTOR
            | I::ITEM_WORKBENCH_WEAVE
            | I::ITEM_WORKBENCH_PRESS
            | I::ITEM_WORKBENCH_CRAFT
            | I::ITEM_WORKBENCH_BUILD
            | I::ITEM_WORKBENCH_MIX
            | I::ITEM_WORKBENCH_DYE
            | I::ITEM_TRAIN_YARD
            | I::ITEM_COMPOST_BIN
            | I::ITEM_CHEST
            | I::ITEM_CHEST_GOLD
            | I::ITEM_CHEST_FEEDER
            | I::ITEM_CHEST_PORTAL
            | I::ITEM_SAFE
    )
}

/// `itemTypeCanBeAttachedNextToToHalfDepthTiles`.
pub fn item_type_can_be_attached_next_to_to_half_depth_tiles(t: ItemType) -> bool {
    if !item_type_is_workbench(t)
        || matches!(t, I::ITEM_CAMPFIRE | I::ITEM_EASEL | I::ITEM_WORKBENCH_REFINERY | I::ITEM_WORKBENCH_SLUICE)
    {
        matches!(t, I::ITEM_CHEST | I::ITEM_CHEST_GOLD | I::ITEM_CHEST_PORTAL | I::ITEM_CHEST_FEEDER)
    } else {
        true
    }
}

/// `itemTypeIsPlacableOnWall`: hangable and not a chandelier, or a torch-like item / RAIL.
pub fn item_type_is_placable_on_wall(t: ItemType) -> bool {
    (item_type_is_hangable(t)
        && !matches!(
            t,
            I::ITEM_AMETHYST_CHANDELIER
                | I::ITEM_SAPPHIRE_CHANDELIER
                | I::ITEM_EMERALD_CHANDELIER
                | I::ITEM_RUBY_CHANDELIER
                | I::ITEM_DIAMOND_CHANDELIER
        ))
        || matches!(
            t,
            I::ITEM_BASIC_TORCH
                | I::ITEM_ICE_TORCH
                | I::ITEM_CLAY_LANTERN_LIT
                | I::ITEM_STEEL_LANTERN
                | I::ITEM_STEEL_DOWNLIGHT
                | I::ITEM_STEEL_UPLIGHT
                | I::ITEM_RUBY
                | I::ITEM_EMERALD
                | I::ITEM_SAPPHIRE
                | I::ITEM_AMETHYST
                | I::ITEM_DIAMOND
                | I::ITEM_RAIL
        )
}

/// `itemTypeIsPlacableOnFloor`.
pub fn item_type_is_placable_on_floor(t: ItemType) -> bool {
    item_type_is_placable_on_wall(t)
        || matches!(
            t,
            I::ITEM_CAMPFIRE
                | I::ITEM_EASEL
                | I::ITEM_TRAIN_STATION
                | I::ITEM_WORKBENCH_PORTAL
                | I::ITEM_PORTAL_AMETHYST
                | I::ITEM_PORTAL_SAPPHIRE
                | I::ITEM_PORTAL_EMERALD
                | I::ITEM_PORTAL_RUBY
                | I::ITEM_PORTAL_DIAMOND
                | I::ITEM_TRADE_PORTAL
                | I::ITEM_TRADING_POST
                | I::ITEM_RAIL
                | I::ITEM_WORKBENCH_REFINERY
                | I::ITEM_WORKBENCH_SLUICE
                | I::ITEM_WOOD_BED
                | I::ITEM_MIRROR
                | I::ITEM_SOFT_BED
                | I::ITEM_GOLD_BED
                | I::ITEM_DOOR
                | I::ITEM_GATE
                | I::ITEM_IRON_DOOR
                | I::ITEM_DODO_EGG
                | I::ITEM_SHARK_JAW
                | I::ITEM_DODO_CAGE
                | I::ITEM_YAK_CAGE
                | I::ITEM_DONKEY_CAGE
                | I::ITEM_UNICORN_CAGE
                | I::ITEM_DROPBEAR_CAGE
                | I::ITEM_SCORPION_CAGE
        )
        || item_type_is_stairs(t)
}

/// `itemTypeCanBePlacedOnColumns`.
pub fn item_type_can_be_placed_on_columns(t: ItemType) -> bool {
    matches!(
        t,
        I::ITEM_BASIC_TORCH
            | I::ITEM_ICE_TORCH
            | I::ITEM_CLAY_LANTERN_LIT
            | I::ITEM_STEEL_LANTERN
            | I::ITEM_STEEL_DOWNLIGHT
            | I::ITEM_STEEL_UPLIGHT
            | I::ITEM_RUBY
            | I::ITEM_EMERALD
            | I::ITEM_SAPPHIRE
            | I::ITEM_AMETHYST
            | I::ITEM_DIAMOND
            | I::ITEM_SHARK_JAW
            | I::ITEM_SIGN
            | I::ITEM_OWNERSHIP_SIGN
            | I::ITEM_DODO_EGG
            | I::ITEM_RAIL
    ) || item_type_is_solid(t)
}

/// `itemTypeIsMetal`: despite the name, the "metal tool" set.
pub fn item_type_is_metal(t: ItemType) -> bool {
    matches!(
        t,
        I::ITEM_FLINT_PICKAXE
            | I::ITEM_FLINT_MACHETE
            | I::ITEM_FLINT_SPADE
            | I::ITEM_FLINT_SPEAR
            | I::ITEM_FLINT_AXE
            | I::ITEM_STONE_AXE
            | I::ITEM_STONE_PICKAXE
            | I::ITEM_TIN_SPADE
            | I::ITEM_BRONZE_PICKAXE
            | I::ITEM_BRONZE_MACHETE
            | I::ITEM_BRONZE_SWORD
            | I::ITEM_STONE_SPADE
            | I::ITEM_IRON_PICKAXE
            | I::ITEM_IRON_MACHETE
            | I::ITEM_IRON_SWORD
            | I::ITEM_TITANIUM_SWORD
            | I::ITEM_TITANIUM_PICKAXE
            | I::ITEM_IRON_AXE
            | I::ITEM_GOLD_SPADE
            | I::ITEM_GOLD_PICKAXE
            | I::ITEM_STEEL_PICKAXE
            | I::ITEM_AMETHYST_PICKAXE
            | I::ITEM_SAPPHIRE_PICKAXE
            | I::ITEM_EMERALD_PICKAXE
            | I::ITEM_RUBY_PICKAXE
            | I::ITEM_RAZOR
            | I::ITEM_CROWBAR
            | I::ITEM_DIAMOND_PICKAXE
    )
}

/// `usageIncrementPerUse`. Raw increment added to `dataA` per use; `-1` means "no durability".
/// Order of operations is exact (rule 0 first; attack `<<2` else expert `/2`; rule 3 `<<1` / rule 1 `/2`;
/// minimum 1).
pub fn usage_increment_per_use(t: ItemType, tool_decay_rule: i32, expert_mode: bool, used_in_attack: bool) -> i32 {
    if tool_decay_rule == 0 {
        return 0;
    }
    let mut v: i32 = -1;
    if t == I::ITEM_FISHING_ROD {
        v = 0x200;
    } else if t == I::ITEM_CARROT_STICK {
        v = 0x100;
    } else if matches!(t, I::ITEM_PAINT | I::ITEM_PAINT_STRIPPER) {
        v = 0x80;
    } else if t == I::ITEM_MAGNET {
        v = 0x100;
    } else if t == I::ITEM_RAZOR {
        v = 0x400;
    } else if matches!(t, I::ITEM_COPPER_BOW | I::ITEM_POISON_BOW) {
        v = 0x29;
    } else if matches!(
        t,
        I::ITEM_FLINT_PICKAXE
            | I::ITEM_FLINT_MACHETE
            | I::ITEM_FLINT_SPADE
            | I::ITEM_FLINT_SPEAR
            | I::ITEM_AMETHYST_PICKAXE
            | I::ITEM_FLINT_AXE
            | I::ITEM_GOLD_SPADE
            | I::ITEM_GOLD_PICKAXE
    ) {
        v = 0x40;
    } else if matches!(
        t,
        I::ITEM_STONE_AXE
            | I::ITEM_STONE_PICKAXE
            | I::ITEM_SAPPHIRE_PICKAXE
            | I::ITEM_COPPER_SPEAR
            | I::ITEM_STONE_SPADE
    ) {
        v = 0x20;
    } else if matches!(
        t,
        I::ITEM_BRONZE_PICKAXE
            | I::ITEM_TIN_SPADE
            | I::ITEM_BRONZE_MACHETE
            | I::ITEM_EMERALD_PICKAXE
            | I::ITEM_BRONZE_SWORD
            | I::ITEM_CROWBAR
    ) {
        v = 0x10;
    } else if matches!(
        t,
        I::ITEM_IRON_PICKAXE | I::ITEM_IRON_MACHETE | I::ITEM_IRON_SWORD | I::ITEM_RUBY_PICKAXE | I::ITEM_IRON_AXE
    ) {
        v = 8;
    } else if matches!(t, I::ITEM_STEEL_PICKAXE | I::ITEM_DIAMOND_PICKAXE) {
        v = 4;
    } else if matches!(t, I::ITEM_TITANIUM_PICKAXE | I::ITEM_TITANIUM_SWORD) {
        v = 3;
    }
    if v > 0 {
        if used_in_attack {
            v <<= 2;
        } else if expert_mode {
            v /= 2;
        }
        if tool_decay_rule == 3 {
            v <<= 1;
        } else if tool_decay_rule == 1 {
            v /= 2;
        }
        if v < 1 {
            v = 1;
        }
    }
    v
}

/// `usageIncrementAppliesToIncorrectUse`: not {MAGNET, FISHING_ROD, COPPER_BOW, POISON_BOW,
/// CARROT_STICK}.
pub fn usage_increment_applies_to_incorrect_use(t: ItemType) -> bool {
    !matches!(t, I::ITEM_MAGNET | I::ITEM_FISHING_ROD | I::ITEM_COPPER_BOW | I::ITEM_POISON_BOW | I::ITEM_CARROT_STICK)
}

/// `clothingItemPasivelyUsedRate`: float32 `const * mult`; -1.0 = no passive decay. `expert_mode`
/// is tested by the original but has no effect.
pub fn clothing_item_pasively_used_rate(
    t: ItemType,
    clothing_decay_rule: i32,
    pole_item_rule: i32,
    _expert_mode: bool,
) -> f32 {
    if clothing_decay_rule == 0 {
        return 0.0;
    }
    let mut mult: f32 = 1.0;
    if clothing_decay_rule == 3 {
        mult = 4.0;
    } else if clothing_decay_rule == 1 {
        mult = 0.25;
    }
    if t.0.wrapping_sub(84) <= 0xd2 {
        match t {
            I::ITEM_LINEN_PANTS
            | I::ITEM_LINEN_SHIRT
            | I::ITEM_LINEN_CAP
            | I::ITEM_LINEN_HAT
            | I::ITEM_TIN_FOIL_HAT
            | I::ITEM_ICE_PANTS
            | I::ITEM_ICE_CHEST_PLATE
            | I::ITEM_ICE_HELMET
            | I::ITEM_ICE_BOOTS => 1.0f32 * mult,
            I::ITEM_LEATHER_JACKET | I::ITEM_LEATHER_BOOTS | I::ITEM_LEATHER_PANTS => 0.75f32 * mult,
            I::ITEM_FUR_COAT | I::ITEM_FUR_BOOTS => 0.5f32 * mult,
            I::ITEM_TIN_PANTS | I::ITEM_TIN_CHEST_PLATE | I::ITEM_TIN_HELMET | I::ITEM_TIN_BOOTS => 0.25f32 * mult,
            I::ITEM_IRON_PANTS | I::ITEM_IRON_CHEST_PLATE | I::ITEM_IRON_HELMET | I::ITEM_IRON_BOOTS => 0.125f32 * mult,
            I::ITEM_JETPACK
            | I::ITEM_TITANIUM_PANTS
            | I::ITEM_TITANIUM_CHEST_PLATE
            | I::ITEM_TITANIUM_HELMET
            | I::ITEM_TITANIUM_BOOTS
            | I::ITEM_CARBON_FIBER_PANTS
            | I::ITEM_CARBON_FIBER_CHEST_PLATE
            | I::ITEM_CARBON_FIBER_HELMET
            | I::ITEM_CARBON_FIBER_BOOTS => 0.1f32 * mult,
            I::ITEM_FRUIT_HAT | I::ITEM_GRASS_SKIRT | I::ITEM_SANTA_HAT | I::ITEM_SOUTH_POLE_BOOTS => {
                if pole_item_rule == 1 {
                    -1.0
                } else {
                    0.05f32 * mult
                }
            }
            _ => -1.0,
        }
    } else {
        -1.0
    }
}

/// `armourItemDamageUsageRate`.
pub fn armour_item_damage_usage_rate(t: ItemType, clothing_decay_rule: i32) -> f32 {
    if clothing_decay_rule == 0 {
        return 0.0;
    }
    let mut mult: f32 = 1.0;
    if clothing_decay_rule == 3 {
        mult = 4.0;
    } else if clothing_decay_rule == 1 {
        mult = 0.25;
    }
    if t.0.wrapping_sub(187) <= 0x6b {
        match t {
            I::ITEM_TIN_PANTS | I::ITEM_TIN_CHEST_PLATE | I::ITEM_TIN_HELMET | I::ITEM_TIN_BOOTS => 20.0f32 * mult,
            I::ITEM_IRON_PANTS
            | I::ITEM_IRON_CHEST_PLATE
            | I::ITEM_IRON_HELMET
            | I::ITEM_IRON_BOOTS
            | I::ITEM_ICE_PANTS
            | I::ITEM_ICE_CHEST_PLATE
            | I::ITEM_ICE_HELMET
            | I::ITEM_ICE_BOOTS => 10.0f32 * mult,
            I::ITEM_JETPACK
            | I::ITEM_TITANIUM_PANTS
            | I::ITEM_TITANIUM_CHEST_PLATE
            | I::ITEM_TITANIUM_HELMET
            | I::ITEM_TITANIUM_BOOTS
            | I::ITEM_CARBON_FIBER_PANTS
            | I::ITEM_CARBON_FIBER_CHEST_PLATE
            | I::ITEM_CARBON_FIBER_HELMET
            | I::ITEM_CARBON_FIBER_BOOTS => 5.0f32 * mult,
            _ => -1.0,
        }
    } else {
        -1.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fill_item_signed_compares() {
        for v in [344u32, 500, 1023, 1106] {
            assert!(!item_type_is_valid_fill_item(ItemType(v)), "{v}");
        }
        assert!(!item_type_is_valid_fill_item(I::ITEM_ROCK));
        assert!(item_type_is_valid_fill_item(I::ITEM_COBBLESTONE));
        assert!(item_type_is_valid_fill_item(ItemType((-5i32) as u32)));
        assert!(!item_type_is_valid_inventory_item(I::ITEM_FREEZE_WATER));
    }

    #[test]
    fn mod_items_are_valid_inventory_items() {
        use crate::mods::{set_tables, ContentTables};
        assert!(!item_type_is_valid_inventory_item(ItemType(1107)));
        let mut t = ContentTables::default();
        t.items.insert(1107);
        set_tables(t);
        assert!(item_type_is_valid_inventory_item(ItemType(1107)));
        // Only the ones the world registered, and vanilla ids are unaffected.
        assert!(!item_type_is_valid_inventory_item(ItemType(1108)));
        assert!(!item_type_is_valid_fill_item(I::ITEM_ROCK));
        set_tables(ContentTables::default());
        assert!(!item_type_is_valid_inventory_item(ItemType(1107)));
    }

    #[test]
    fn stackable_and_misc() {
        assert!(!item_type_is_stackable(I::ITEM_BASKET, 1, 1));
        assert!(!item_type_is_stackable(I::ITEM_PAINT, 1, 2));
        assert!(!item_type_is_stackable(I::ITEM_LINEN_SHIRT, 0, 5));
        assert!(item_type_is_stackable(I::ITEM_LINEN_SHIRT, 3, 5));
        assert!(item_type_is_stackable(I::ITEM_APPLE, 1, 2));
        assert!(item_type_is_hangable(I::ITEM_LADDER) && !item_type_is_placable_on_wall(I::ITEM_AMETHYST_CHANDELIER));
        assert!(!item_type_is_sowable_in_tile_type(I::ITEM_PRICKLY_PEAR, TileType::TILE_DIRT));
        assert!(item_type_is_sowable_in_tile_type(I::ITEM_PRICKLY_PEAR, TileType::TILE_COMPOST));
        assert!(item_type_is_sowable_in_tile_type(I::ITEM_PRICKLY_PEAR, TileType::TILE_SAND));
        for tl in 0..80u32 {
            assert!(!item_type_is_sowable_in_tile_type(I::ITEM_KELP, TileType(tl)));
        }
        assert_eq!(inventory_sub_items_count_for_item_type(I::ITEM_CHEST), 16);
        assert_eq!(inventory_sub_items_count_for_item_type(I::ITEM_CLAY_JUG), 2);
        assert_eq!(npc_type_from_cage_item_type(I::ITEM_UNICORN_CAGE), NPCType::NPC_DONKEY);
        assert!(item_type_is_block(I::ITEM_WORKBENCH_KILN) && !item_type_is_block(I::ITEM_DOOR));
        assert!(item_type_is_block(I::ITEM_ROCK));
        assert!(interaction_type_disallowed_on_protected_tiles(InteractionType::INTERACTION_DIG_BACK_WALL));
        assert!(!interaction_type_disallowed_on_protected_tiles(InteractionType(4)));
        assert!(workbench_type_is_double_height(WorkbenchType(26)));
    }

    #[test]
    fn usage_increments() {
        let u = usage_increment_per_use;
        assert_eq!(u(I::ITEM_FISHING_ROD, 2, false, false), 512);
        assert_eq!(u(I::ITEM_FISHING_ROD, 1, false, false), 256);
        assert_eq!(u(I::ITEM_FISHING_ROD, 3, false, false), 1024);
        assert_eq!(u(I::ITEM_FISHING_ROD, 2, true, false), 256);
        assert_eq!(u(I::ITEM_FISHING_ROD, 2, true, true), 2048);
        assert_eq!(u(I::ITEM_TITANIUM_PICKAXE, 1, false, false), 1);
        assert_eq!(u(I::ITEM_STEEL_PICKAXE, 1, true, false), 1);
        // halved twice below one, clamped to the minimum of one
        assert_eq!(u(I::ITEM_TITANIUM_PICKAXE, 1, true, false), 1);
        assert_eq!(u(I::ITEM_APPLE, 2, false, false), -1);
        assert_eq!(u(I::ITEM_APPLE, 0, false, false), 0);
        assert_eq!(u(I::ITEM_FISHING_ROD, 7, false, false), 512);
    }

    #[test]
    fn clothing_rates() {
        let c = clothing_item_pasively_used_rate;
        assert_eq!(c(I::ITEM_LINEN_SHIRT, 2, 0, false), 1.0);
        assert_eq!(c(I::ITEM_IRON_HELMET, 3, 0, false), 0.5);
        assert_eq!(c(I::ITEM_IRON_HELMET, 1, 0, false), 0.03125);
        assert_eq!(c(I::ITEM_SANTA_HAT, 2, 1, false), -1.0);
        assert_eq!(c(I::ITEM_SANTA_HAT, 2, 0, false), 0.05);
        assert_eq!(c(I::ITEM_JETPACK, 2, 0, false), 0.1);
        assert_eq!(c(I::ITEM_ICE_PANTS, 2, 0, false), 1.0);
        assert_eq!(c(I::ITEM_GRASS_SKIRT, 0, 0, false), 0.0);
        assert_eq!(c(I::ITEM_APPLE, 2, 0, false), -1.0);
        assert_eq!(c(ItemType(299), 2, 0, false), -1.0);
        let a = armour_item_damage_usage_rate;
        assert_eq!(a(I::ITEM_TIN_BOOTS, 2), 20.0);
        assert_eq!(a(I::ITEM_IRON_PANTS, 3), 40.0);
        assert_eq!(a(I::ITEM_JETPACK, 1), 1.25);
        assert_eq!(a(I::ITEM_LINEN_SHIRT, 2), -1.0);
    }

    #[test]
    fn cage_gem_and_head_tables() {
        for (cage, npc) in [
            (I::ITEM_DODO_CAGE, NPCType::NPC_DODO),
            (I::ITEM_DONKEY_CAGE, NPCType::NPC_DONKEY),
            (I::ITEM_YAK_CAGE, NPCType::NPC_YAK),
            (I::ITEM_DROPBEAR_CAGE, NPCType::NPC_DROPBEAR),
            (I::ITEM_SCORPION_CAGE, NPCType::NPC_SCORPION),
            (I::ITEM_UNICORN_CAGE, NPCType::NPC_DONKEY),
            (I::ITEM_APPLE, NPCType::NPC_NOTHING),
        ] {
            assert_eq!(npc_type_from_cage_item_type(cage), npc);
        }
        for g in [I::ITEM_AMETHYST, I::ITEM_SAPPHIRE, I::ITEM_EMERALD, I::ITEM_RUBY, I::ITEM_DIAMOND] {
            assert!(item_type_is_gem(g));
        }
        assert!(!item_type_is_gem(I::ITEM_APPLE));
        for h in [
            I::ITEM_LINEN_CAP,
            I::ITEM_LINEN_HAT,
            I::ITEM_SANTA_HAT,
            I::ITEM_TIN_FOIL_HAT,
            I::ITEM_TIN_HELMET,
            I::ITEM_IRON_HELMET,
            I::ITEM_TITANIUM_HELMET,
            I::ITEM_CARBON_FIBER_HELMET,
            I::ITEM_ICE_HELMET,
            I::ITEM_FRUIT_HAT,
        ] {
            assert!(item_type_can_be_worn_on_head(h));
        }
        assert!(!item_type_can_be_worn_on_head(I::ITEM_LINEN_SHIRT));
    }

    #[test]
    fn overlay_offset_and_background_rock() {
        assert_eq!(item_type_for_colored_overlay(I::ITEM_SOFT_BED), I::ITEM_BED_QUILT_TEXTURE);
        assert_eq!(item_type_for_colored_overlay(I::ITEM_GOLD_BED), I::ITEM_BED_QUILT_TEXTURE);
        assert_eq!(item_type_for_colored_overlay(I::ITEM_APPLE), I::ITEM_NONE);
        assert_eq!(x_offset_for_item_type(I::ITEM_CARROT_STICK), -0.1875);
        assert_eq!(x_offset_for_item_type(I::ITEM_FISHING_ROD), -0.1875);
        assert_eq!(x_offset_for_item_type(I::ITEM_APPLE), 0.0);
        assert!(!item_type_can_be_placed_on_background_rock(I::ITEM_WINDOW));
        assert!(!item_type_can_be_placed_on_background_rock(I::ITEM_BLACK_WINDOW));
        assert!(item_type_can_be_placed_on_background_rock(I::ITEM_DOOR));
    }

    #[test]
    fn sowing_foreground_and_attachment() {
        for t in [I::ITEM_APPLE, I::ITEM_CARROT, I::ITEM_CORN, I::ITEM_VINE] {
            assert!(item_type_is_sowable_in_tile_type(t, TileType::TILE_DIRT), "{t:?}");
            assert!(item_type_is_sowable_in_tile_type(t, TileType::TILE_COMPOST), "{t:?}");
        }
        for t in [
            I::ITEM_FLAX_SEED,
            I::ITEM_SUNFLOWER_SEED,
            I::ITEM_CARROT,
            I::ITEM_TULIP_BULB,
            I::ITEM_TULIP_SEED,
            I::ITEM_CORN,
            I::ITEM_WHEAT,
            I::ITEM_TOMATO,
            I::ITEM_CHILLI,
            I::ITEM_KELP,
            I::ITEM_VINE,
        ] {
            assert!(sowable_item_type_occupies_foreground(t));
        }
        assert!(!sowable_item_type_occupies_foreground(I::ITEM_APPLE));
        // workbenches attach beside half depth tiles except the listed ones, which behave like plain items
        assert!(item_type_can_be_attached_next_to_to_half_depth_tiles(I::ITEM_WORKBENCH_KILN));
        assert!(item_type_can_be_attached_next_to_to_half_depth_tiles(I::ITEM_CHEST));
        assert!(!item_type_can_be_attached_next_to_to_half_depth_tiles(I::ITEM_CAMPFIRE));
        assert!(!item_type_can_be_attached_next_to_to_half_depth_tiles(I::ITEM_APPLE));
        assert!(!item_type_can_be_attached_next_to_to_half_depth_tiles(I::ITEM_EASEL));
        for t in [I::ITEM_BASIC_TORCH, I::ITEM_SIGN, I::ITEM_RAIL, I::ITEM_DODO_EGG, I::ITEM_COBBLESTONE] {
            assert!(item_type_can_be_placed_on_columns(t), "{t:?}");
        }
        assert!(!item_type_can_be_placed_on_columns(I::ITEM_APPLE));
    }

    #[test]
    fn usage_increment_tiers() {
        let u = |t| usage_increment_per_use(t, 2, false, false);
        assert_eq!(u(I::ITEM_CARROT_STICK), 0x100);
        assert_eq!(u(I::ITEM_MAGNET), 0x100);
        assert_eq!(u(I::ITEM_RAZOR), 0x400);
        assert_eq!(u(I::ITEM_COPPER_BOW), 0x29);
        assert_eq!(u(I::ITEM_POISON_BOW), 0x29);
        assert_eq!(u(I::ITEM_PAINT), 0x80);
        assert_eq!(u(I::ITEM_FLINT_AXE), 0x40);
        assert_eq!(u(I::ITEM_STONE_AXE), 0x20);
        assert_eq!(u(I::ITEM_CROWBAR), 0x10);
        assert_eq!(u(I::ITEM_IRON_AXE), 8);
        assert_eq!(u(I::ITEM_STEEL_PICKAXE), 4);
        assert_eq!(u(I::ITEM_TITANIUM_SWORD), 3);
        assert!(usage_increment_applies_to_incorrect_use(I::ITEM_STONE_AXE));
        assert!(!usage_increment_applies_to_incorrect_use(I::ITEM_MAGNET));
    }

    #[test]
    fn clothing_and_armour_tiers() {
        let c = |t| clothing_item_pasively_used_rate(t, 2, 0, false);
        assert_eq!(c(I::ITEM_LEATHER_JACKET), 0.75);
        assert_eq!(c(I::ITEM_FUR_COAT), 0.5);
        assert_eq!(c(I::ITEM_TIN_PANTS), 0.25);
        assert_eq!(c(I::ITEM_IRON_BOOTS), 0.125);
        assert_eq!(c(I::ITEM_CARBON_FIBER_BOOTS), 0.1);
        // inside the clothing id window but not a clothing item
        assert_eq!(c(I::ITEM_STEEL_INGOT), -1.0);
        assert_eq!(armour_item_damage_usage_rate(I::ITEM_ICE_HELMET, 2), 10.0);
        assert_eq!(armour_item_damage_usage_rate(I::ITEM_CARBON_FIBER_PANTS, 2), 5.0);
        // inside the armour id window but not armour
        assert_eq!(armour_item_damage_usage_rate(I::ITEM_RAIL, 2), -1.0);
        assert_eq!(armour_item_damage_usage_rate(I::ITEM_LINEN_SHIRT, 0), 0.0);
        assert_eq!(armour_item_damage_usage_rate(I::ITEM_APPLE, 2), -1.0);
    }
}
