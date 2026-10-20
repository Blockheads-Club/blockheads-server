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

    /// Every `| `NAME` | value |` row of (non-ENet/MDB) must exist with same value
    /// somewhere in the generated sources.
    #[test]
    fn enums_md_rows_present() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let md = crate::testenv::private_docs().join("enums.md");
        if !md.exists() {
            return;
        }
        let text = std::fs::read_to_string(md).unwrap();
        let src: String = ["enums.rs", "item_type.rs", "net_types.rs"]
            .iter()
            .map(|f| std::fs::read_to_string(root.join("src/types").join(f)).unwrap())
            .collect();
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
        assert!(found > 400, "found {found} missing {missing}");
    }

    ///  s3/s4 spot values consumed by other modules.
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
}
