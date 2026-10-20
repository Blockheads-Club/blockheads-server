//! Dynamic-object type tables, predicates and the class factory table.
//!
//! `classForDynamicObjectType`); (`classForInteractionObjectType`); (`NPC_DYNAMIC_OBJECT_TYPES`,
//! The item/workbench maps of live in `items::` (item_tables); they are not duplicated here.

use std::cell::RefCell;

use super::base::{DynamicWorldRef, WorldRef};
use super::class_info::ClassInfo;
use super::handle::{CacheRef, ObjRef};
use crate::items::inventory_item::InventoryItem;
use crate::plist::value::PlistDict;
use crate::types::structs::IntPair;

// ---- DynamicObjectType constants; i32 as used by the C arrays) ----
pub const DYNAMIC_OBJECT_TYPE_UNDEFINED: i32 = 0;
pub const DYNAMIC_OBJECT_TYPE_APPLE_TREE: i32 = 1;
pub const DYNAMIC_OBJECT_TYPE_MAPLE_TREE: i32 = 2;
pub const DYNAMIC_OBJECT_TYPE_MANGO_TREE: i32 = 3;
pub const DYNAMIC_OBJECT_TYPE_PINE_TREE: i32 = 4;
pub const DYNAMIC_OBJECT_TYPE_CACTUS_TREE: i32 = 5;
pub const DYNAMIC_OBJECT_TYPE_COCONUT_TREE: i32 = 6;
pub const DYNAMIC_OBJECT_TYPE_ORANGE_TREE: i32 = 7;
pub const DYNAMIC_OBJECT_TYPE_CHERRY_TREE: i32 = 8;
pub const DYNAMIC_OBJECT_TYPE_COFFEE_TREE: i32 = 9;
pub const DYNAMIC_OBJECT_TYPE_FLAX_PLANT: i32 = 10;
pub const DYNAMIC_OBJECT_TYPE_SUNFLOWER_PLANT: i32 = 11;
pub const DYNAMIC_OBJECT_TYPE_CORN_PLANT: i32 = 12;
pub const DYNAMIC_OBJECT_TYPE_NPC_DODO: i32 = 13;
pub const DYNAMIC_OBJECT_TYPE_FREE_BLOCK: i32 = 14;
pub const DYNAMIC_OBJECT_TYPE_INTERACTION_OBJECT_DEPRECATED: i32 = 15;
pub const DYNAMIC_OBJECT_TYPE_FIRE: i32 = 16;
pub const DYNAMIC_OBJECT_TYPE_TORCH: i32 = 17;
pub const DYNAMIC_OBJECT_TYPE_GLOWBLOCK: i32 = 18;
pub const DYNAMIC_OBJECT_TYPE_LADDER: i32 = 19;
pub const DYNAMIC_OBJECT_TYPE_DOOR: i32 = 20;
pub const DYNAMIC_OBJECT_TYPE_ARTIFICIAL_LIGHT: i32 = 21;
pub const DYNAMIC_OBJECT_TYPE_SURFACEBLOCK: i32 = 22;
pub const DYNAMIC_OBJECT_TYPE_BED: i32 = 23;
pub const DYNAMIC_OBJECT_TYPE_BLOCKHEAD: i32 = 24;
pub const DYNAMIC_OBJECT_TYPE_NPC_DROPBEAR: i32 = 25;
pub const DYNAMIC_OBJECT_TYPE_GATHERBLOCK: i32 = 26;
pub const DYNAMIC_OBJECT_TYPE_CARROT_PLANT: i32 = 27;
pub const DYNAMIC_OBJECT_TYPE_NPC_DONKEY: i32 = 28;
pub const DYNAMIC_OBJECT_TYPE_SNOW_SURFACEBLOCK: i32 = 29;
pub const DYNAMIC_OBJECT_TYPE_EGG: i32 = 30;
pub const DYNAMIC_OBJECT_TYPE_WINDOW: i32 = 31;
pub const DYNAMIC_OBJECT_TYPE_BOAT: i32 = 32;
pub const DYNAMIC_OBJECT_TYPE_CHILLI_PLANT: i32 = 33;
pub const DYNAMIC_OBJECT_TYPE_KELP_PLANT: i32 = 34;
pub const DYNAMIC_OBJECT_TYPE_NPC_CLOWNFISH: i32 = 35;
pub const DYNAMIC_OBJECT_TYPE_NPC_SHARK: i32 = 36;
pub const DYNAMIC_OBJECT_TYPE_LIME_TREE: i32 = 37;
pub const DYNAMIC_OBJECT_TYPE_WIRE: i32 = 38;
pub const DYNAMIC_OBJECT_TYPE_NPC_CAVETROLL: i32 = 39;
pub const DYNAMIC_OBJECT_TYPE_RAIL: i32 = 40;
pub const DYNAMIC_OBJECT_TYPE_TRAIN_CAR_HANDCAR: i32 = 41;
pub const DYNAMIC_OBJECT_TYPE_TRAIN_CAR_STEAM_ENGINE: i32 = 42;
pub const DYNAMIC_OBJECT_TYPE_TRAIN_CAR_FREIGHT: i32 = 43;
pub const DYNAMIC_OBJECT_TYPE_TRAIN_CAR_PASSENGER: i32 = 44;
pub const DYNAMIC_OBJECT_TYPE_WORKBENCH: i32 = 45;
pub const DYNAMIC_OBJECT_TYPE_CHEST: i32 = 46;
pub const DYNAMIC_OBJECT_TYPE_SIGN: i32 = 47;
pub const DYNAMIC_OBJECT_TYPE_TRADING_POST: i32 = 48;
pub const DYNAMIC_OBJECT_TYPE_TRAIN_STATION: i32 = 49;
pub const DYNAMIC_OBJECT_TYPE_TRADE_PORTAL: i32 = 50;
pub const DYNAMIC_OBJECT_TYPE_NPC_SCORPION: i32 = 51;
pub const DYNAMIC_OBJECT_TYPE_PAINTING: i32 = 52;
pub const DYNAMIC_OBJECT_TYPE_COLUMN: i32 = 53;
pub const DYNAMIC_OBJECT_TYPE_STAIRS: i32 = 54;
pub const DYNAMIC_OBJECT_TYPE_ELEVATOR_MOTOR: i32 = 55;
pub const DYNAMIC_OBJECT_TYPE_ELEVATOR_SHAFT: i32 = 56;
pub const DYNAMIC_OBJECT_TYPE_GEM_TREE: i32 = 57;
pub const DYNAMIC_OBJECT_TYPE_VINE_PLANT: i32 = 58;
pub const DYNAMIC_OBJECT_TYPE_TULIP_PLANT: i32 = 59;
pub const DYNAMIC_OBJECT_TYPE_OWNERSHIP_SIGN: i32 = 60;
pub const DYNAMIC_OBJECT_TYPE_WHEAT_PLANT: i32 = 61;
pub const DYNAMIC_OBJECT_TYPE_TOMATO_PLANT: i32 = 62;
pub const DYNAMIC_OBJECT_TYPE_NPC_YAK: i32 = 63;
pub const DYNAMIC_OBJECT_TYPE_MIRROR: i32 = 64;
pub const MAX_DYNAMIC_OBJECT_TYPES: i32 = 65;

// ---- InteractionObjectType constants; B) ----
pub const INTERACTION_OBJECT_TYPE_UNDEFINED: i32 = 0;
pub const INTERACTION_OBJECT_TYPE_WORKBENCH: i32 = 1;
pub const INTERACTION_OBJECT_TYPE_CHEST: i32 = 2;
pub const INTERACTION_OBJECT_TYPE_BED: i32 = 3;
pub const INTERACTION_OBJECT_TYPE_SIGN: i32 = 4;
pub const INTERACTION_OBJECT_TYPE_TRADING_POST: i32 = 5;
pub const INTERACTION_OBJECT_TYPE_TRAIN_STATION: i32 = 6;
pub const INTERACTION_OBJECT_TYPE_TRADE_PORTAL: i32 = 7;
pub const INTERACTION_OBJECT_TYPE_OWNERSHIP_SIGN: i32 = 8;
pub const INTERACTION_OBJECT_TYPE_MIRROR: i32 = 9;

/// `TREE_DYNAMIC_OBJECT_TYPES`.
pub const TREE_DYNAMIC_OBJECT_TYPES: [i32; 11] = [1, 2, 3, 4, 5, 6, 7, 8, 9, 37, 57];
/// `PLANT_DYNAMIC_OBJECT_TYPES`.
pub const PLANT_DYNAMIC_OBJECT_TYPES: [i32; 10] = [10, 11, 12, 27, 33, 34, 58, 59, 61, 62];
/// `INTERACTION_OBJECT_DYNAMIC_OBJECT_TYPES` (order as in the binary).
pub const INTERACTION_OBJECT_DYNAMIC_OBJECT_TYPES: [i32; 9] = [23, 45, 46, 47, 60, 48, 49, 50, 64];
/// `NPC_DYNAMIC_OBJECT_TYPES` (iteration order matters).
pub const NPC_DYNAMIC_OBJECT_TYPES: [i32; 8] = [13, 25, 28, 35, 36, 39, 51, 63];

/// `objectTypeIsTree`.
pub fn object_type_is_tree(t: i32) -> bool {
    TREE_DYNAMIC_OBJECT_TYPES.contains(&t)
}
/// `objectTypeIsPlant`.
pub fn object_type_is_plant(t: i32) -> bool {
    PLANT_DYNAMIC_OBJECT_TYPES.contains(&t)
}
/// `objectTypeIsInteractionObject`.
pub fn object_type_is_interaction_object(t: i32) -> bool {
    INTERACTION_OBJECT_DYNAMIC_OBJECT_TYPES.contains(&t)
}
/// `objectTypeIsNPC`.
pub fn object_type_is_npc(t: i32) -> bool {
    NPC_DYNAMIC_OBJECT_TYPES.contains(&t)
}

/// `objectTypeHasStaticPosition`: plants, trees, interaction objects, or one of
/// {16..=22, 26, 29, 52, 30, 31, 38, 40, 53, 54, 55} or 56.
pub fn object_type_has_static_position(t: i32) -> bool {
    object_type_is_plant(t)
        || object_type_is_tree(t)
        || object_type_is_interaction_object(t)
        || matches!(t, 16 | 17 | 18 | 19 | 20 | 21 | 22 | 26 | 29 | 52 | 30 | 31 | 38 | 40 | 53 | 54 | 55)
        || t == 56
}

/// `objectPartialUpdateMax`: 29 -> 64, 22 -> 512, else -1.
pub fn object_partial_update_max(t: i32) -> i32 {
    match t {
        29 => 64,
        22 => 512,
        _ => -1,
    }
}
/// `objectTypeRequiresPartialUpdate`.
pub fn object_type_requires_partial_update(t: i32) -> bool {
    object_partial_update_max(t) != -1
}
/// `objectTypeCanBeLoadedOnlyWhenClientOwnerOnline` (= `objectTypeIsNPC`).
pub fn object_type_can_be_loaded_only_when_client_owner_online(t: i32) -> bool {
    object_type_is_npc(t)
}

/// `dynamicObjectTypeForInteractionObjectType`.
/// 7->50, 8->60, 9->64, else 0).
pub fn dynamic_object_type_for_interaction_object_type(i: i32) -> i32 {
    match i {
        1 => 45,
        2 => 46,
        3 => 23,
        4 => 47,
        5 => 48,
        6 => 49,
        7 => 50,
        8 => 60,
        9 => 64,
        _ => 0,
    }
}

/// The `classForDynamicObjectType` name table (1..=64).
pub fn class_name_for_dynamic_object_type(t: i32) -> Option<&'static str> {
    match t {
        1 => Some("AppleTree"),
        2 => Some("MapleTree"),
        3 => Some("MangoTree"),
        4 => Some("PineTree"),
        5 => Some("CactusTree"),
        6 => Some("CoconutTree"),
        7 => Some("OrangeTree"),
        8 => Some("CherryTree"),
        9 => Some("CoffeeTree"),
        10 => Some("FlaxPlant"),
        11 => Some("SunflowerPlant"),
        12 => Some("CornPlant"),
        13 => Some("Dodo"),
        14 => Some("FreeBlock"),
        15 => Some("InteractionObject"),
        16 => Some("FireObject"),
        17 => Some("Torch"),
        18 => Some("GlowBlock"),
        19 => Some("Ladder"),
        20 => Some("Door"),
        21 => Some("ArtificialLight"),
        22 => Some("SurfaceBlock"),
        23 => Some("Bed"),
        24 => Some("Blockhead"),
        25 => Some("DropBear"),
        26 => Some("GatherBlock"),
        27 => Some("CarrotPlant"),
        28 => Some("Donkey"),
        29 => Some("SnowSurfaceBlock"),
        30 => Some("Egg"),
        31 => Some("Window"),
        32 => Some("Boat"),
        33 => Some("ChilliPlant"),
        34 => Some("KelpPlant"),
        35 => Some("ClownFish"),
        36 => Some("Shark"),
        37 => Some("LimeTree"),
        38 => Some("Wire"),
        39 => Some("CaveTroll"),
        40 => Some("Rail"),
        41 => Some("HandCar"),
        42 => Some("SteamTrain"),
        43 => Some("FreightCar"),
        44 => Some("PassengerCar"),
        45 => Some("Workbench"),
        46 => Some("Chest"),
        47 => Some("Sign"),
        48 => Some("TradingPost"),
        49 => Some("TrainStation"),
        50 => Some("TradePortal"),
        51 => Some("Scorpion"),
        52 => Some("Painting"),
        53 => Some("Column"),
        54 => Some("Stairs"),
        55 => Some("ElevatorMotor"),
        56 => Some("ElevatorShaft"),
        57 => Some("GemTree"),
        58 => Some("VinePlant"),
        59 => Some("TulipPlant"),
        60 => Some("OwnershipSign"),
        61 => Some("WheatPlant"),
        62 => Some("TomatoPlant"),
        63 => Some("Yak"),
        64 => Some("Mirror"),
        _ => None,
    }
}

/// The `classForInteractionObjectType` name table (1..=9).
pub fn class_name_for_interaction_object_type(t: i32) -> Option<&'static str> {
    match t {
        1 => Some("Workbench"),
        2 => Some("Chest"),
        3 => Some("Bed"),
        4 => Some("Sign"),
        5 => Some("TradingPost"),
        6 => Some("TrainStation"),
        7 => Some("TradePortal"),
        8 => Some("OwnershipSign"),
        9 => Some("Mirror"),
        _ => None,
    }
}

// ---------------------------------------------------------------- factory table

/// `-initWithWorld:dynamicWorld:atPosition:cache:`
pub type FromPositionFn = fn(&WorldRef, &DynamicWorldRef, IntPair, CacheRef) -> Option<ObjRef>;
/// `-initWithWorld:dynamicWorld:saveDict:cache:`
pub type FromSaveDictFn = fn(&WorldRef, &DynamicWorldRef, &PlistDict, CacheRef) -> Option<ObjRef>;
/// `-initWithWorld:dynamicWorld:cache:netData:`
pub type FromNetDataFn = fn(&WorldRef, &DynamicWorldRef, CacheRef, &[u8]) -> Option<ObjRef>;
/// `-initWithWorld:dynamicWorld:atPosition:cache:type:saveDict:placedByClient:`
pub type FromPositionTypeSaveDictFn =
    fn(&WorldRef, &DynamicWorldRef, IntPair, CacheRef, i32, Option<&PlistDict>, Option<&str>) -> Option<ObjRef>;
/// `-initWithWorld:dynamicWorld:atPosition:cache:item:flipped:saveDict:placedByClient:clientName:`
pub type FromPositionInteractionFn = fn(
    &WorldRef,
    &DynamicWorldRef,
    IntPair,
    CacheRef,
    Option<&InventoryItem>,
    bool,
    Option<&PlistDict>,
    Option<&str>,
    Option<&str>,
) -> Option<ObjRef>;

/// The Rust value of an ObjC `Class` returned by `classForDynamicObjectType` / `classForInteractionObjectType`:
/// the class descriptor plus the constructors for the five message shapes of the DynamicWorld call sites.
#[derive(Clone, Copy)]
pub struct ClassFactory {
    pub class: &'static ClassInfo,
    pub object_type: i32,
    pub from_position: Option<FromPositionFn>,
    pub from_save_dict: Option<FromSaveDictFn>,
    pub from_net_data: Option<FromNetDataFn>,
    pub from_position_type_save_dict: Option<FromPositionTypeSaveDictFn>,
    pub from_position_interaction: Option<FromPositionInteractionFn>,
}

type Table = Vec<Option<&'static ClassFactory>>;

thread_local! {
    static DYNAMIC_TABLE: RefCell<Table> = RefCell::new(vec![None; 65]);
    static INTERACTION_TABLE: RefCell<Table> = RefCell::new(vec![None; 10]);
}

/// Install the factory for dynamic object type `object_type` (index 1..=64). Called by `registry::install()`.
///  Out-of-range indices are ignored.
pub fn register_dynamic_class(object_type: i32, f: ClassFactory) {
    if (1..=64).contains(&object_type) {
        let leaked: &'static ClassFactory = Box::leak(Box::new(f));
        DYNAMIC_TABLE.with(|t| t.borrow_mut()[object_type as usize] = Some(leaked));
    }
}

/// Install the factory for interaction object type `interaction_type` (index 1..=9).
pub fn register_interaction_class(interaction_type: i32, f: ClassFactory) {
    if (1..=9).contains(&interaction_type) {
        let leaked: &'static ClassFactory = Box::leak(Box::new(f));
        INTERACTION_TABLE.with(|t| t.borrow_mut()[interaction_type as usize] = Some(leaked));
    }
}

/// `classForDynamicObjectType(t)`: `None` for 0, 65 and anything outside 1..=64.
pub fn class_for_dynamic_object_type(t: i32) -> Option<&'static ClassFactory> {
    if !(1..=64).contains(&t) {
        return None;
    }
    DYNAMIC_TABLE.with(|tb| tb.borrow()[t as usize])
}

/// `classForInteractionObjectType(t)`: 1..=9 only.
pub fn class_for_interaction_object_type(t: i32) -> Option<&'static ClassFactory> {
    if !(1..=9).contains(&t) {
        return None;
    }
    INTERACTION_TABLE.with(|tb| tb.borrow()[t as usize])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::objects::class_info::{DYNAMIC_OBJECT_CLASS, INTERACTION_OBJECT_CLASS};

    #[test]
    fn predicates() {
        for t in -2..=70 {
            assert_eq!(object_type_is_tree(t), TREE_DYNAMIC_OBJECT_TYPES.contains(&t));
            assert_eq!(object_type_is_plant(t), PLANT_DYNAMIC_OBJECT_TYPES.contains(&t));
            assert_eq!(object_type_is_interaction_object(t), INTERACTION_OBJECT_DYNAMIC_OBJECT_TYPES.contains(&t));
            assert_eq!(object_type_is_npc(t), NPC_DYNAMIC_OBJECT_TYPES.contains(&t));
            assert_eq!(object_type_can_be_loaded_only_when_client_owner_online(t), object_type_is_npc(t));
        }
        let mut expected: Vec<i32> = Vec::new();
        expected.extend(PLANT_DYNAMIC_OBJECT_TYPES);
        expected.extend(TREE_DYNAMIC_OBJECT_TYPES);
        expected.extend(INTERACTION_OBJECT_DYNAMIC_OBJECT_TYPES);
        expected.extend([16, 17, 18, 19, 20, 21, 22, 26, 29, 52, 30, 31, 38, 40, 53, 54, 55, 56]);
        for t in -2..=70 {
            assert_eq!(object_type_has_static_position(t), expected.contains(&t), "type {t}");
        }
        assert_eq!(
            PLANT_DYNAMIC_OBJECT_TYPES.len()
                + TREE_DYNAMIC_OBJECT_TYPES.len()
                + INTERACTION_OBJECT_DYNAMIC_OBJECT_TYPES.len(),
            30
        );
    }

    #[test]
    fn partial_update() {
        assert_eq!(object_partial_update_max(29), 64);
        assert_eq!(object_partial_update_max(22), 512);
        assert_eq!(object_partial_update_max(0), -1);
        assert!(object_type_requires_partial_update(22) && object_type_requires_partial_update(29));
        assert!(!object_type_requires_partial_update(14));
    }

    #[test]
    fn interaction_mapping() {
        let want =
            [(0, 0), (1, 45), (2, 46), (3, 23), (4, 47), (5, 48), (6, 49), (7, 50), (8, 60), (9, 64), (10, 0), (-1, 0)];
        for (i, d) in want {
            assert_eq!(dynamic_object_type_for_interaction_object_type(i), d);
        }
        for i in 1..=9 {
            let d = dynamic_object_type_for_interaction_object_type(i);
            assert!(object_type_is_interaction_object(d));
            assert_eq!(class_name_for_interaction_object_type(i), class_name_for_dynamic_object_type(d));
        }
    }

    #[test]
    fn names() {
        assert_eq!(class_name_for_dynamic_object_type(0), None);
        assert_eq!(class_name_for_dynamic_object_type(65), None);
        assert_eq!(class_name_for_dynamic_object_type(1), Some("AppleTree"));
        assert_eq!(class_name_for_dynamic_object_type(63), Some("Yak"));
        assert_eq!(class_name_for_dynamic_object_type(64), Some("Mirror"));
        assert_eq!(class_name_for_dynamic_object_type(15), Some("InteractionObject"));
        assert_eq!((1..=64).filter(|t| class_name_for_dynamic_object_type(*t).is_some()).count(), 64);
        assert_eq!(class_name_for_interaction_object_type(0), None);
        assert_eq!(class_name_for_interaction_object_type(10), None);
        assert_eq!(MAX_DYNAMIC_OBJECT_TYPES, 65);
    }

    #[test]
    fn factory_table() {
        assert!(class_for_dynamic_object_type(0).is_none());
        assert!(class_for_dynamic_object_type(65).is_none());
        assert!(class_for_dynamic_object_type(-1).is_none());
        assert!(class_for_interaction_object_type(0).is_none());
        assert!(class_for_interaction_object_type(10).is_none());
        let f = ClassFactory {
            class: &DYNAMIC_OBJECT_CLASS,
            object_type: 15,
            from_position: None,
            from_save_dict: None,
            from_net_data: None,
            from_position_type_save_dict: None,
            from_position_interaction: None,
        };
        register_dynamic_class(15, f);
        assert!(class_for_dynamic_object_type(15).is_some());
        register_dynamic_class(65, f);
        assert!(class_for_dynamic_object_type(65).is_none());
        let g = ClassFactory { class: &INTERACTION_OBJECT_CLASS, ..f };
        register_interaction_class(2, g);
        assert!(std::ptr::eq(class_for_interaction_object_type(2).unwrap().class, &INTERACTION_OBJECT_CLASS));
    }
}
