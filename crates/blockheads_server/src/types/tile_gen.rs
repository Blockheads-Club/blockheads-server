// @generated layout table (wire/save format). DO NOT EDIT.
// Included via include!() from the hand-written sibling file; `WireStruct` must be in scope.

/// size 0x40
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Tile {
    /// uint8_t @ 0x0
    pub type_index: u8,
    /// uint8_t @ 0x1
    pub back_wall_type_index: u8,
    /// uint8_t @ 0x2
    pub zone_type_index: u8,
    /// uint8_t @ 0x3
    pub contents: u8,
    /// uint8_t @ 0x4
    pub partial_content_left: u8,
    /// uint8_t @ 0x5
    pub gather_progress: u8,
    /// uint8_t @ 0x6
    pub light: u8,
    /// uint8_t @ 0x7
    pub sun_light: u8,
    /// uint8_t @ 0x8
    pub season_offset: u8,
    /// uint8_t @ 0x9
    pub explored_fraction: u8,
    /// uint8_t @ 0xa
    pub terrain_slow_factor: u8,
    /// uint8_t @ 0xb
    pub foreground_contents: u8,
    /// uint8_t @ 0xc
    pub background_contents: u8,
    /// implicit padding at 0xd; preserved on read, zero by default
    pub gap_d: [u8; 1],
    /// uint16_t @ 0xe
    pub artificial_light_r: u16,
    /// uint16_t @ 0x10
    pub artificial_light_g: u16,
    /// uint16_t @ 0x12
    pub artificial_light_b: u16,
    /// int16_t @ 0x14
    pub artificial_heat: i16,
    /// uint8_t @ 0x16
    pub on_fire: u8,
    /// implicit padding at 0x17; preserved on read, zero by default
    pub gap_17: [u8; 1],
    /// uint32_t @ 0x18
    pub dynamic_object_owner_old: u32,
    /// uint16_t @ 0x1c
    pub paint_front: u16,
    /// uint16_t @ 0x1e
    pub paint_top: u16,
    /// uint16_t @ 0x20
    pub paint_right: u16,
    /// uint16_t @ 0x22
    pub paint_left: u16,
    /// uint16_t @ 0x24
    pub paint_bot: u16,
    /// implicit padding at 0x26; preserved on read, zero by default
    pub gap_26: [u8; 2],
    /// uint64_t @ 0x28
    pub dynamic_object_owner: u64,
    /// explicit padding at 0x30; preserved on read, zero by default
    pub padding: [u8; 16],
}
impl WireStruct for Tile {
    const SIZE: usize = 0x40;
    fn write_le(&self, out: &mut [u8]) {
        assert!(out.len() >= Self::SIZE);
        out[..Self::SIZE].fill(0);
        out[0x0..0x1].copy_from_slice(&self.type_index.to_le_bytes());
        out[0x1..0x2].copy_from_slice(&self.back_wall_type_index.to_le_bytes());
        out[0x2..0x3].copy_from_slice(&self.zone_type_index.to_le_bytes());
        out[0x3..0x4].copy_from_slice(&self.contents.to_le_bytes());
        out[0x4..0x5].copy_from_slice(&self.partial_content_left.to_le_bytes());
        out[0x5..0x6].copy_from_slice(&self.gather_progress.to_le_bytes());
        out[0x6..0x7].copy_from_slice(&self.light.to_le_bytes());
        out[0x7..0x8].copy_from_slice(&self.sun_light.to_le_bytes());
        out[0x8..0x9].copy_from_slice(&self.season_offset.to_le_bytes());
        out[0x9..0xa].copy_from_slice(&self.explored_fraction.to_le_bytes());
        out[0xa..0xb].copy_from_slice(&self.terrain_slow_factor.to_le_bytes());
        out[0xb..0xc].copy_from_slice(&self.foreground_contents.to_le_bytes());
        out[0xc..0xd].copy_from_slice(&self.background_contents.to_le_bytes());
        out[0xd..0xe].copy_from_slice(&self.gap_d);
        out[0xe..0x10].copy_from_slice(&self.artificial_light_r.to_le_bytes());
        out[0x10..0x12].copy_from_slice(&self.artificial_light_g.to_le_bytes());
        out[0x12..0x14].copy_from_slice(&self.artificial_light_b.to_le_bytes());
        out[0x14..0x16].copy_from_slice(&self.artificial_heat.to_le_bytes());
        out[0x16..0x17].copy_from_slice(&self.on_fire.to_le_bytes());
        out[0x17..0x18].copy_from_slice(&self.gap_17);
        out[0x18..0x1c].copy_from_slice(&self.dynamic_object_owner_old.to_le_bytes());
        out[0x1c..0x1e].copy_from_slice(&self.paint_front.to_le_bytes());
        out[0x1e..0x20].copy_from_slice(&self.paint_top.to_le_bytes());
        out[0x20..0x22].copy_from_slice(&self.paint_right.to_le_bytes());
        out[0x22..0x24].copy_from_slice(&self.paint_left.to_le_bytes());
        out[0x24..0x26].copy_from_slice(&self.paint_bot.to_le_bytes());
        out[0x26..0x28].copy_from_slice(&self.gap_26);
        out[0x28..0x30].copy_from_slice(&self.dynamic_object_owner.to_le_bytes());
        out[0x30..0x40].copy_from_slice(&self.padding);
    }
    fn read_le(b: &[u8]) -> Self {
        assert!(b.len() >= Self::SIZE);
        Self {
            type_index: u8::from_le_bytes(b[0x0..0x1].try_into().unwrap()),
            back_wall_type_index: u8::from_le_bytes(b[0x1..0x2].try_into().unwrap()),
            zone_type_index: u8::from_le_bytes(b[0x2..0x3].try_into().unwrap()),
            contents: u8::from_le_bytes(b[0x3..0x4].try_into().unwrap()),
            partial_content_left: u8::from_le_bytes(b[0x4..0x5].try_into().unwrap()),
            gather_progress: u8::from_le_bytes(b[0x5..0x6].try_into().unwrap()),
            light: u8::from_le_bytes(b[0x6..0x7].try_into().unwrap()),
            sun_light: u8::from_le_bytes(b[0x7..0x8].try_into().unwrap()),
            season_offset: u8::from_le_bytes(b[0x8..0x9].try_into().unwrap()),
            explored_fraction: u8::from_le_bytes(b[0x9..0xa].try_into().unwrap()),
            terrain_slow_factor: u8::from_le_bytes(b[0xa..0xb].try_into().unwrap()),
            foreground_contents: u8::from_le_bytes(b[0xb..0xc].try_into().unwrap()),
            background_contents: u8::from_le_bytes(b[0xc..0xd].try_into().unwrap()),
            gap_d: b[0xd..0xe].try_into().unwrap(),
            artificial_light_r: u16::from_le_bytes(b[0xe..0x10].try_into().unwrap()),
            artificial_light_g: u16::from_le_bytes(b[0x10..0x12].try_into().unwrap()),
            artificial_light_b: u16::from_le_bytes(b[0x12..0x14].try_into().unwrap()),
            artificial_heat: i16::from_le_bytes(b[0x14..0x16].try_into().unwrap()),
            on_fire: u8::from_le_bytes(b[0x16..0x17].try_into().unwrap()),
            gap_17: b[0x17..0x18].try_into().unwrap(),
            dynamic_object_owner_old: u32::from_le_bytes(b[0x18..0x1c].try_into().unwrap()),
            paint_front: u16::from_le_bytes(b[0x1c..0x1e].try_into().unwrap()),
            paint_top: u16::from_le_bytes(b[0x1e..0x20].try_into().unwrap()),
            paint_right: u16::from_le_bytes(b[0x20..0x22].try_into().unwrap()),
            paint_left: u16::from_le_bytes(b[0x22..0x24].try_into().unwrap()),
            paint_bot: u16::from_le_bytes(b[0x24..0x26].try_into().unwrap()),
            gap_26: b[0x26..0x28].try_into().unwrap(),
            dynamic_object_owner: u64::from_le_bytes(b[0x28..0x30].try_into().unwrap()),
            padding: b[0x30..0x40].try_into().unwrap(),
        }
    }
}
impl Default for Tile { fn default() -> Self { Self::read_le(&[0u8; 64]) } }
#[cfg(test)]
mod test_tile {
    use super::*;
    #[test]
    fn layout() {
        assert_eq!(core::mem::size_of::<Tile>(), 0x40);
        assert_eq!(<Tile as WireStruct>::SIZE, 0x40);
        assert_eq!(core::mem::offset_of!(Tile, type_index), 0x0);
        assert_eq!(core::mem::offset_of!(Tile, back_wall_type_index), 0x1);
        assert_eq!(core::mem::offset_of!(Tile, zone_type_index), 0x2);
        assert_eq!(core::mem::offset_of!(Tile, contents), 0x3);
        assert_eq!(core::mem::offset_of!(Tile, partial_content_left), 0x4);
        assert_eq!(core::mem::offset_of!(Tile, gather_progress), 0x5);
        assert_eq!(core::mem::offset_of!(Tile, light), 0x6);
        assert_eq!(core::mem::offset_of!(Tile, sun_light), 0x7);
        assert_eq!(core::mem::offset_of!(Tile, season_offset), 0x8);
        assert_eq!(core::mem::offset_of!(Tile, explored_fraction), 0x9);
        assert_eq!(core::mem::offset_of!(Tile, terrain_slow_factor), 0xa);
        assert_eq!(core::mem::offset_of!(Tile, foreground_contents), 0xb);
        assert_eq!(core::mem::offset_of!(Tile, background_contents), 0xc);
        assert_eq!(core::mem::offset_of!(Tile, artificial_light_r), 0xe);
        assert_eq!(core::mem::offset_of!(Tile, artificial_light_g), 0x10);
        assert_eq!(core::mem::offset_of!(Tile, artificial_light_b), 0x12);
        assert_eq!(core::mem::offset_of!(Tile, artificial_heat), 0x14);
        assert_eq!(core::mem::offset_of!(Tile, on_fire), 0x16);
        assert_eq!(core::mem::offset_of!(Tile, dynamic_object_owner_old), 0x18);
        assert_eq!(core::mem::offset_of!(Tile, paint_front), 0x1c);
        assert_eq!(core::mem::offset_of!(Tile, paint_top), 0x1e);
        assert_eq!(core::mem::offset_of!(Tile, paint_right), 0x20);
        assert_eq!(core::mem::offset_of!(Tile, paint_left), 0x22);
        assert_eq!(core::mem::offset_of!(Tile, paint_bot), 0x24);
        assert_eq!(core::mem::offset_of!(Tile, dynamic_object_owner), 0x28);
        assert_eq!(core::mem::offset_of!(Tile, padding), 0x30);
    }
    #[test]
    fn roundtrip() {
        let mut raw = [0u8; 0x40];
        for (i, x) in raw.iter_mut().enumerate() { *x = (i as u8).wrapping_mul(37).wrapping_add(11); }
        let v = Tile::read_le(&raw);
        let mut back = vec![0u8; raw.len()];
        v.write_le(&mut back);
        assert_eq!(&back[..], &raw[..]);   // byte-exact incl. padding (floats compared as bytes, NaN-safe)
        assert!(<Tile>::default().to_vec().iter().all(|&x| x == 0));
    }
}

