//! owned by types_enums
//! Hand-written helpers for the generated enum newtypes.
use super::item_type::ItemType;
use super::net_types::{NetDataType, ServerConnectionRejection};

impl ItemType {
    /// Truncating store to the u16 storage/wire width (s2;).
    #[inline]
    pub const fn to_u16(self) -> u16 {
        self.0 as u16
    }
    /// Zero-extending load from a u16 field (s2).
    #[inline]
    pub const fn from_u16(v: u16) -> Self {
        ItemType(v as u32)
    }
}

impl NetDataType {
    /// First packet byte (s3).
    #[inline]
    pub const fn as_byte(self) -> u8 {
        self.0 as u8
    }
    /// Zero-extending parse of a packet type byte; unknown ids stay representable.
    #[inline]
    pub fn from_byte(b: u8) -> Self {
        NetDataType(b as u32)
    }
}

impl ServerConnectionRejection {
    /// Reject packet reason byte: packet is `[0x26, reason]` (s3/s4).
    #[inline]
    pub const fn as_byte(self) -> u8 {
        self.0 as u8
    }
}

#[cfg(test)]
mod tests {
    use super::super::enums::*;
    use super::super::item_type::ItemType;
    use super::super::net_types::*;
    use std::mem::size_of;

    #[test]
    fn tile_spot() {
        assert_eq!(TileType::TILE_ROCK.raw(), 1);
        assert_eq!(TileType::TILE_AIR.raw(), 2);
        assert_eq!(TileType::TILE_WATER.raw(), 3);
        assert_eq!(TileType::TILE_SNOW.raw(), 5);
        assert_eq!(TileType::TILE_LAVA.raw(), 31);
        assert_eq!(TileType::TILE_COMPOST.raw(), 48);
        assert_eq!(TileType(0).name(), None);
    }

    #[test]
    fn item_spot() {
        assert_eq!(ItemType::ITEM_NONE.raw(), 0);
        assert_eq!(ItemType::ITEM_DEFAULT_MARKER.raw(), 2);
        assert_eq!(ItemType::ITEM_EMPTY_MARKER.raw(), 5);
        assert_eq!(ItemType::ITEM_TIME_CRYSTAL.raw(), 11);
        assert_eq!(ItemType::ITEM_PRICKLY_PEAR.raw(), 24);
        assert_eq!(ItemType::ITEM_KELP.raw(), 144);
        assert_eq!(ItemType::ITEM_POISON.raw(), 255);
        assert_eq!(ItemType::ITEM_CAGE.raw(), 302);
        assert_eq!(ItemType::ITEM_SCORPION_CAGE.raw(), 327);
        assert_eq!(ItemType(1106).to_u16(), 1106);
        assert_eq!(ItemType(0x1_0005).to_u16(), 5);
        assert_eq!(ItemType::from_u16(65535).raw(), 65535);
        let n = (0..=2000u32).filter(|v| ItemType(*v).name().is_some()).count();
        assert_eq!(n, 428);
        assert_eq!(ItemType::ITEM_CLOTHING_SLOT.raw(), 1);
        assert_eq!((0..=2000u32).filter(|v| ItemType(*v).name().is_some()).max(), Some(1106));
    }

    #[test]
    fn counts_and_misc() {
        assert_eq!(DynamicObjectType::MAX_DYNAMIC_OBJECT_TYPES.raw(), 65);
        assert_eq!((0..=65u32).filter(|v| DynamicObjectType(*v).name().is_some()).count(), 66);
        assert_eq!((0..=200u32).filter(|v| TileType(*v).name().is_some()).count(), 78);
        assert_eq!((0..=300u32).filter(|v| TileContents(*v).name().is_some()).count(), 149);
        assert_eq!((0..=200u32).filter(|v| NetDataType(*v).name().is_some()).count(), 77);
        assert_eq!(NetDataType::BHNDT_CLIENT_FOUND_ITEM_LIST_CHANGED_V2.raw(), 76);
        assert_eq!(NetDataType::BHNDT_CLIENT_UNIQUE_ID_REQUEST.as_byte(), 0x0e);
        assert_eq!(NetDataType::from_byte(0x26).raw(), 0x26);
        assert_eq!((0..=100u32).filter(|v| ServerConnectionRejection(*v).name().is_some()).count(), 16);
        assert_eq!((0..=100u32).filter(|v| BhNetNodeType(*v).name().is_some()).count(), 6);
        assert_eq!(ServerConnectionRejection::SERVER_CONNECTION_REJECTION_SERVER_FULL.as_byte(), 5);
        assert_eq!(FoodType::FOOD_NONE.raw(), 32);
        assert_eq!(FoodType::FOOD_TYPE_COUNT.raw(), 31);
        assert_eq!(DynamicObjectType::DYNAMIC_OBJECT_TYPE_NPC_SHARK.raw(), 36);
        assert_eq!(DynamicObjectType::DYNAMIC_OBJECT_TYPE_NPC_SCORPION.raw(), 51);
    }

    #[test]
    fn widths() {
        assert_eq!(size_of::<DodoBreed>(), 2);
        assert_eq!(size_of::<DonkeyBreed>(), 2);
        assert_eq!(size_of::<BhNetPrivacy>(), 8);
        assert_eq!(size_of::<SignConnectionType>(), 8);
        assert_eq!(size_of::<SignOffsetType>(), 8);
        assert_eq!(size_of::<ItemType>(), 4);
    }

    #[test]
    fn unknown_values() {
        let t = TileType::from_raw(200);
        assert_eq!(t.name(), None);
        assert_eq!(format!("{:?}", t), "TileType(200)");
        assert_eq!(t.raw(), 200);
        assert_eq!(format!("{:?}", TileType::TILE_AIR), "TILE_AIR");
    }

    /// Checks every row of an enums.md-style table against the generated sources; returns (found, missing).
    fn check_enum_rows(text: &str, src: &str) -> (usize, usize) {
        let (mut found, mut missing) = (0, 0);
        for l in text.lines() {
            let c: Vec<&str> = l.split('|').map(|s| s.trim()).collect();
            if c.len() < 4 || !c[1].starts_with('`') {
                continue;
            }
            let name = c[1].trim_matches('`');
            let Ok(v) = c[2].parse::<i64>() else { continue };
            let pat = format!("pub const {}: ", name);
            if let Some(i) = src.find(&pat) {
                let line = src[i..].lines().next().unwrap();
                assert!(line.contains(&format!("({})", v)), "{} value mismatch: {}", name, line);
                found += 1;
            } else {
                missing += 1;
            }
        }
        (found, missing)
    }

    #[test]
    fn enums_md_rows_checker() {
        let src = "    pub const TILE_AIR: TileType = TileType(2);\n";
        let text = "| Name | Value | Note |\n|---|---|---|\n| `TILE_AIR` | 2 | x |\n| `NO_SUCH` | 7 | x |\n| `TILE_ROCK` | n/a | x |\n| short |\n";
        assert_eq!(check_enum_rows(text, src), (1, 1));
        let bad = std::panic::catch_unwind(|| check_enum_rows("| `TILE_AIR` | 3 | y |\n", src));
        assert!(bad.is_err());
    }

    /// `check_enum_rows` over an enums.md file against the three generated sources.
    fn check_enum_file(md: &std::path::Path) -> (usize, usize) {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let text = std::fs::read_to_string(md).unwrap();
        let src: String = ["enums.rs", "item_type.rs", "net_types.rs"]
            .iter()
            .map(|f| std::fs::read_to_string(root.join("src/types").join(f)).unwrap())
            .collect();
        check_enum_rows(&text, &src)
    }

    fn assert_rows_present(md: &std::path::Path, min_found: usize) {
        let (found, missing) = check_enum_file(md);
        assert!(found >= min_found, "found {found} missing {missing}");
    }

    #[test]
    fn enums_md_file_checker() {
        let md = std::env::temp_dir().join(format!("bh-enums-{}.md", std::process::id()));
        std::fs::write(&md, "| `TILE_AIR` | 2 | x |\n| `NO_SUCH_TILE` | 7 | x |\n").unwrap();
        assert_eq!(check_enum_file(&md), (1, 1));
        assert_rows_present(&md, 1);
        std::fs::remove_file(&md).unwrap();
    }

    /// Every `| `NAME` | value |` row of (non-ENet/MDB) must exist with same value
    /// somewhere in the generated sources (skipped without the private docs).
    #[test]
    fn enums_md_rows_present() {
        let md = crate::testenv::private_docs().join("enums.md");
        Some(md).filter(|m| m.exists()).iter().for_each(|m| assert_rows_present(m, 401));
    }

    #[test]
    fn cross_module_spot_values() {
        assert_eq!(NPCType::NPC_SCORPION.raw(), 7);
        assert_eq!(NPCType::NPC_CLOWNFISH.raw(), 4);
        assert_eq!(NPCType::NPC_SHARK.raw(), 5);
        assert_eq!(PlantType::PLANT_KELP.raw(), 6);
        assert_eq!(
            [
                SimulationEvent::SE_SLEEP,
                SimulationEvent::SE_CRAFT,
                SimulationEvent::SE_GOT,
                SimulationEvent::SE_PLACE,
                SimulationEvent::SE_DAMAGE,
                SimulationEvent::SE_COLLAPSE
            ]
            .map(|e| e.raw()),
            [0, 1, 2, 3, 4, 5]
        );
        assert_eq!(
            [
                CustomRuleNpc::CUSTOM_RULE_NPC_HARD,
                CustomRuleNpc::CUSTOM_RULE_NPC_NORMAL,
                CustomRuleNpc::CUSTOM_RULE_NPC_EASY,
                CustomRuleNpc::CUSTOM_RULE_NPC_PASSIVE
            ]
            .map(|e| e.raw()),
            [0, 1, 2, 3]
        );
        assert_eq!(CustomRuleTradePortal::CUSTOM_RULE_TRADE_PORTAL_MISSIONS_ONLY.raw(), 5);
        assert_eq!(CustomRuleTradePortal::CUSTOM_RULE_TRADE_PORTAL_EVERYTHING_FREE.raw(), 4);
        assert_eq!(BhNetNodeType::BHNetNodeTypeLocalServer.raw(), 0);
        assert_eq!(BhNetNodeType::BHNetNodeTypeOnlineServer.raw(), 2);
        let ids = [
            (NetDataType::BHNDT_CLIENT_BLOCK_REMOVAL_NOTIFICATION, 0x0d),
            (NetDataType::BHNDT_CLIENT_UNIQUE_ID_REQUEST, 0x0e),
            (NetDataType::BHNDT_CLIENT_FREEBLOCK_CREATION_REQUEST, 20),
            (NetDataType::BHNDT_SERVER_PLAYER_LIST_CHANGED_NOTIFICATION, 0x1e),
            (NetDataType::BHNDT_CLIENT_PLAYER_INFORMATION, 0x1f),
            (NetDataType::BHNDT_SERVER_INITIAL_CONTACT, 0x23),
            (NetDataType::BHNDT_CHAT_MESSAGE, 0x24),
            (NetDataType::BHNDT_SERVER_CONNECTION_REJECTION, 0x26),
            (NetDataType::BHNDT_SERVER_ADMIN_OR_MOD_CHANGED, 0x2b),
            (NetDataType::BHNDT_CLIENT_UPDATE_CUSTOM_RULE, b'B'),
            (NetDataType::BHNDT_CLIENT_FOUND_ITEM_LIST_CHANGED_V2, b'L'),
        ];
        for (t, b) in ids {
            assert_eq!(t.as_byte(), b);
            assert_eq!(NetDataType::from_byte(b), t);
        }
        assert_eq!(ServerConnectionRejection::SERVER_CONNECTION_REJECTION_STILL_LOADING.as_byte(), 15);
        assert_eq!(ServerConnectionRejection::SERVER_CONNECTION_REJECTION_NO_CREDIT.as_byte(), 13);
        assert_eq!(ServerConnectionRejection::SERVER_CONNECTION_REJECTION_KICKED.as_byte(), 11);
        assert_eq!(ServerConnectionRejection::SERVER_CONNECTION_REJECTION_BANNED.as_byte(), 12);
    }

    /// Calls the generated accessors of every enum wrapper on each declared value and on one undeclared value, so the
    /// layout tables are exercised end to end. The declared values are read back out of the generated sources.
    fn into_raw<T: Into<R>, R>(t: T, _like: R) -> R {
        t.into()
    }

    macro_rules! sweep {
        ($($t:ident),* $(,)?) => {
            &[$((stringify!($t), (|v: i64| {
                let t = $t(v as _);
                let raw = t.raw();
                assert_eq!($t::from_raw(raw), t);
                assert_eq!($t::from(raw), t);
                assert_eq!(into_raw(t, raw), raw);
                assert_eq!(format!("{t:?}"), t.name().map_or_else(|| format!("{}({})", stringify!($t), t.0), str::to_string));
                t.name().is_some()
            }) as fn(i64) -> bool)),*]
        };
    }

    #[test]
    fn generated_accessors_sweep() {
        let table: &[(&str, fn(i64) -> bool)] = sweep![
            LightDirectionType, ListType, BlockheadAnimationType, BlockheadSubAnimationType, BlockheadUnableToWorkReason,
            ActionFoundType, CaveTrollAnimationType, CaveTrollSubAnimationType, ChestType, ColumnConfiguration,
            DodoBreed, DonkeyBreed, DynamicObjectType, TileType, TileContents,
            ImageType, TileTraverseType, TileTraverseKeyFrameType, SimulationEvent, HideUIType,
            InteractionType, PathType, CustomRuleSeed, CustomRuleOceans, CustomRuleTemperature,
            CustomRuleRainfall, CustomRulePlants, CustomRuleTrees, CustomRuleTerrain, CustomRuleCaves,
            CustomRuleLakes, CustomRuleSkyIslands, CustomRuleOre, CustomRuleOreDistribution, CustomRuleGems,
            CustomRuleTreasure, CustomRuleHealth, CustomRuleHunger, CustomRuleHappiness, CustomRuleTradePortal,
            CustomRuleNpc, CustomRuleClothingDecay, CustomRuleToolDecay, CustomRuleDeath, CustomRuleDeathDrops,
            CustomRuleDayNight, CustomRuleWorldModification, CustomRulePoleItems, CustomRuleTeleportation, CustomRuleTradePortalPrices,
            CustomRuleTradePortalDiscovered, CustomRuleMeditation, CustomRuleCategory, InteractionObjectType, FoodType,
            WorkbenchType, ItemCategoryType, NPCType, NetNPCTameType, SendState,
            NetNPCInteractionType, PigmentColor, ParticleGravityType, PlantType, RailConfiguration,
            BlockRequestType, SignConnectionType, SignOffsetType, StairsConfiguration, TipType,
            TreeType, Vec2Axis, WireConfiguration, WireSolidConfiguration, IslandType,
            ItemType, BhPlayerState, BhMatchSendData, BhNetPrivacy, NetDataType, ServerConnectionRejection,
            BhNetNodeType,
        ];
        let mut seen = 0;
        for src in [include_str!("enums.rs"), include_str!("net_types.rs"), include_str!("item_type.rs")] {
            // rustfmt wraps declarations with long type names after the `=`
            for line in src.replace(" =\n        ", " = ").lines() {
                let Some((_, rest)) = line.trim().split_once(": ") else { continue };
                let Some((ty, val)) = rest.split_once(" = ") else { continue };
                let Some(arg) = val.strip_prefix(ty).and_then(|v| v.strip_prefix('(')).and_then(|v| v.strip_suffix(");")) else { continue };
                let v: i64 = arg.parse().unwrap();
                let (_, f) = table.iter().find(|(n, _)| *n == ty).unwrap_or_else(|| panic!("{ty} missing from the sweep"));
                assert!(f(v), "{ty}({v}) has no name");
                seen += 1;
            }
        }
        assert!(seen > 1500);
        for (n, f) in table {
            assert!(!f(0x7fff_fff3), "{n} names a bogus value");
        }
    }
}
