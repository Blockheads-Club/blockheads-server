//! types_structs: every wire struct (`*NetData`, `*CreationData`, `*UpdateData`, `*NetRequest`, headers).
//! All generated (netdata_gen.rs). Each struct is the raw `memcpy` image the original sends:
//! `[NSData dataWithBytes:&s length:sizeof(s)]`, little-endian, `SIZE` bytes, padding zero.
#[allow(unused_imports)]
use super::structs::{BlockheadSkinOptions, IntPair, MJColor8, Vector, Vector2, WireStruct};
#[allow(unused_imports)]
use super::tile::Tile;

include!("netdata_gen.rs");

impl DynamicObjectNetData {
    /// `-[DynamicObject dynamicObjectNetData]`: `posX = (pos.x < 0) ? (uint32)(pos.x + worldWidthMacro*32): (uint32)pos.x`,
    /// `posY = (uint32)pos.y`. `pos_x_wrapped` must already hold the wrapped value. Padding stays zero.
    pub fn new(unique_id: u64, pos_x_wrapped: u32, pos_y: u32, needs_removed: bool) -> Self {
        let mut d = Self::default();
        d.unique_id = unique_id;
        d.pos_x = pos_x_wrapped;
        d.pos_y = pos_y;
        d.needs_removed = needs_removed as u8;
        d
    }
}

#[cfg(test)]
mod hand_tests {
    use super::*;
    /// : `u64 uniqueID | u32 posX | u32 posY | u8 needsRemoved | 7 zero`
    #[test]
    fn dynamic_object_net_data_image() {
        let d = DynamicObjectNetData::new(0x1122334455667788, 0xfffffff0, 7, true);
        let b = d.to_vec();
        assert_eq!(b.len(), 0x18);
        assert_eq!(&b[0..8], &0x1122334455667788u64.to_le_bytes());
        assert_eq!(&b[8..12], &0xfffffff0u32.to_le_bytes());
        assert_eq!(&b[12..16], &7u32.to_le_bytes());
        assert_eq!(b[16], 1);
        assert!(b[17..].iter().all(|&x| x == 0));
    }
}
