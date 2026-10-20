// @generated layout table (wire/save format). DO NOT EDIT.
// Included via include!() from the hand-written sibling file; `WireStruct` must be in scope.

/// size 0x8
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ClientMacroBlockRequest {
    /// int32_t @ 0x0
    pub macro_index: i32,
    /// uint8_t @ 0x4
    pub create_if_not_created: u8,
    /// explicit padding at 0x5; preserved on read, zero by default
    pub padding: [u8; 3],
}
impl WireStruct for ClientMacroBlockRequest {
    const SIZE: usize = 0x8;
    fn write_le(&self, out: &mut [u8]) {
        assert!(out.len() >= Self::SIZE);
        out[..Self::SIZE].fill(0);
        out[0x0..0x4].copy_from_slice(&self.macro_index.to_le_bytes());
        out[0x4..0x5].copy_from_slice(&self.create_if_not_created.to_le_bytes());
        out[0x5..0x8].copy_from_slice(&self.padding);
    }
    fn read_le(b: &[u8]) -> Self {
        assert!(b.len() >= Self::SIZE);
        Self {
            macro_index: i32::from_le_bytes(b[0x0..0x4].try_into().unwrap()),
            create_if_not_created: u8::from_le_bytes(b[0x4..0x5].try_into().unwrap()),
            padding: b[0x5..0x8].try_into().unwrap(),
        }
    }
}
impl Default for ClientMacroBlockRequest { fn default() -> Self { Self::read_le(&[0u8; 8]) } }
#[cfg(test)]
mod test_client_macro_block_request {
    use super::*;
    #[test]
    fn layout() {
        assert_eq!(core::mem::size_of::<ClientMacroBlockRequest>(), 0x8);
        assert_eq!(<ClientMacroBlockRequest as WireStruct>::SIZE, 0x8);
        assert_eq!(core::mem::offset_of!(ClientMacroBlockRequest, macro_index), 0x0);
        assert_eq!(core::mem::offset_of!(ClientMacroBlockRequest, create_if_not_created), 0x4);
        assert_eq!(core::mem::offset_of!(ClientMacroBlockRequest, padding), 0x5);
    }
    #[test]
    fn roundtrip() {
        let mut raw = [0u8; 0x8];
        for (i, x) in raw.iter_mut().enumerate() { *x = (i as u8).wrapping_mul(37).wrapping_add(11); }
        let v = ClientMacroBlockRequest::read_le(&raw);
        let mut back = vec![0u8; raw.len()];
        v.write_le(&mut back);
        assert_eq!(&back[..], &raw[..]);   // byte-exact incl. padding (floats compared as bytes, NaN-safe)
        assert!(<ClientMacroBlockRequest>::default().to_vec().iter().all(|&x| x == 0));
    }
}

/// size 0x8
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PortalChestTransaction {
    /// uint16_t @ 0x0
    pub transaction_identifier: u16,
    /// explicit padding at 0x2; preserved on read, zero by default
    pub padding: [u8; 6],
}
impl WireStruct for PortalChestTransaction {
    const SIZE: usize = 0x8;
    fn write_le(&self, out: &mut [u8]) {
        assert!(out.len() >= Self::SIZE);
        out[..Self::SIZE].fill(0);
        out[0x0..0x2].copy_from_slice(&self.transaction_identifier.to_le_bytes());
        out[0x2..0x8].copy_from_slice(&self.padding);
    }
    fn read_le(b: &[u8]) -> Self {
        assert!(b.len() >= Self::SIZE);
        Self {
            transaction_identifier: u16::from_le_bytes(b[0x0..0x2].try_into().unwrap()),
            padding: b[0x2..0x8].try_into().unwrap(),
        }
    }
}
impl Default for PortalChestTransaction { fn default() -> Self { Self::read_le(&[0u8; 8]) } }
#[cfg(test)]
mod test_portal_chest_transaction {
    use super::*;
    #[test]
    fn layout() {
        assert_eq!(core::mem::size_of::<PortalChestTransaction>(), 0x8);
        assert_eq!(<PortalChestTransaction as WireStruct>::SIZE, 0x8);
        assert_eq!(core::mem::offset_of!(PortalChestTransaction, transaction_identifier), 0x0);
        assert_eq!(core::mem::offset_of!(PortalChestTransaction, padding), 0x2);
    }
    #[test]
    fn roundtrip() {
        let mut raw = [0u8; 0x8];
        for (i, x) in raw.iter_mut().enumerate() { *x = (i as u8).wrapping_mul(37).wrapping_add(11); }
        let v = PortalChestTransaction::read_le(&raw);
        let mut back = vec![0u8; raw.len()];
        v.write_le(&mut back);
        assert_eq!(&back[..], &raw[..]);   // byte-exact incl. padding (floats compared as bytes, NaN-safe)
        assert!(<PortalChestTransaction>::default().to_vec().iter().all(|&x| x == 0));
    }
}

/// size 0x30
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct BedNetData {
    /// InteractionObjectCreationNetData @ 0x0
    pub interaction_object_creation_net_data: InteractionObjectCreationNetData,
    /// uint16_t @ 0x28
    pub item_type: u16,
    /// uint16_t @ 0x2a
    pub bedding_color: u16,
    /// explicit padding at 0x2c; preserved on read, zero by default
    pub padding: [u8; 4],
}
impl WireStruct for BedNetData {
    const SIZE: usize = 0x30;
    fn write_le(&self, out: &mut [u8]) {
        assert!(out.len() >= Self::SIZE);
        out[..Self::SIZE].fill(0);
        self.interaction_object_creation_net_data.write_le(&mut out[0x0..]);
        out[0x28..0x2a].copy_from_slice(&self.item_type.to_le_bytes());
        out[0x2a..0x2c].copy_from_slice(&self.bedding_color.to_le_bytes());
        out[0x2c..0x30].copy_from_slice(&self.padding);
    }
    fn read_le(b: &[u8]) -> Self {
        assert!(b.len() >= Self::SIZE);
        Self {
            interaction_object_creation_net_data: InteractionObjectCreationNetData::read_le(&b[0x0..]),
            item_type: u16::from_le_bytes(b[0x28..0x2a].try_into().unwrap()),
            bedding_color: u16::from_le_bytes(b[0x2a..0x2c].try_into().unwrap()),
            padding: b[0x2c..0x30].try_into().unwrap(),
        }
    }
}
impl Default for BedNetData { fn default() -> Self { Self::read_le(&[0u8; 48]) } }
#[cfg(test)]
mod test_bed_net_data {
    use super::*;
    #[test]
    fn layout() {
        assert_eq!(core::mem::size_of::<BedNetData>(), 0x30);
        assert_eq!(<BedNetData as WireStruct>::SIZE, 0x30);
        assert_eq!(core::mem::offset_of!(BedNetData, interaction_object_creation_net_data), 0x0);
        assert_eq!(core::mem::offset_of!(BedNetData, item_type), 0x28);
        assert_eq!(core::mem::offset_of!(BedNetData, bedding_color), 0x2a);
        assert_eq!(core::mem::offset_of!(BedNetData, padding), 0x2c);
    }
    #[test]
    fn roundtrip() {
        let mut raw = [0u8; 0x30];
        for (i, x) in raw.iter_mut().enumerate() { *x = (i as u8).wrapping_mul(37).wrapping_add(11); }
        let v = BedNetData::read_le(&raw);
        let mut back = vec![0u8; raw.len()];
        v.write_le(&mut back);
        assert_eq!(&back[..], &raw[..]);   // byte-exact incl. padding (floats compared as bytes, NaN-safe)
        assert!(<BedNetData>::default().to_vec().iter().all(|&x| x == 0));
    }
}

/// size 0x28
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct BlockheadPreviewDataNew {
    /// struct BlockheadSkinOptions @ 0x0
    pub skin_options: BlockheadSkinOptions,
    /// uint16_t @ 0x14
    pub hat_item_type: u16,
    /// uint16_t @ 0x16
    pub pants_item_type: u16,
    /// uint16_t @ 0x18
    pub shirt_item_type: u16,
    /// uint16_t @ 0x1a
    pub shoes_item_type: u16,
    /// uint16_t @ 0x1c
    pub hat_color: u16,
    /// uint16_t @ 0x1e
    pub pants_color: u16,
    /// uint16_t @ 0x20
    pub shirt_color: u16,
    /// uint16_t @ 0x22
    pub shoes_color: u16,
    /// uint8_t @ 0x24
    pub happiness: u8,
    /// uint8_t @ 0x25
    pub dead: u8,
    /// uint8_t @ 0x26
    pub _padding: [u8; 2],
}
impl WireStruct for BlockheadPreviewDataNew {
    const SIZE: usize = 0x28;
    fn write_le(&self, out: &mut [u8]) {
        assert!(out.len() >= Self::SIZE);
        out[..Self::SIZE].fill(0);
        self.skin_options.write_le(&mut out[0x0..]);
        out[0x14..0x16].copy_from_slice(&self.hat_item_type.to_le_bytes());
        out[0x16..0x18].copy_from_slice(&self.pants_item_type.to_le_bytes());
        out[0x18..0x1a].copy_from_slice(&self.shirt_item_type.to_le_bytes());
        out[0x1a..0x1c].copy_from_slice(&self.shoes_item_type.to_le_bytes());
        out[0x1c..0x1e].copy_from_slice(&self.hat_color.to_le_bytes());
        out[0x1e..0x20].copy_from_slice(&self.pants_color.to_le_bytes());
        out[0x20..0x22].copy_from_slice(&self.shirt_color.to_le_bytes());
        out[0x22..0x24].copy_from_slice(&self.shoes_color.to_le_bytes());
        out[0x24..0x25].copy_from_slice(&self.happiness.to_le_bytes());
        out[0x25..0x26].copy_from_slice(&self.dead.to_le_bytes());
        for (k, e) in self._padding.iter().enumerate() { out[0x26 + k * 1..0x26 + (k + 1) * 1].copy_from_slice(&e.to_le_bytes()); }
    }
    fn read_le(b: &[u8]) -> Self {
        assert!(b.len() >= Self::SIZE);
        Self {
            skin_options: BlockheadSkinOptions::read_le(&b[0x0..]),
            hat_item_type: u16::from_le_bytes(b[0x14..0x16].try_into().unwrap()),
            pants_item_type: u16::from_le_bytes(b[0x16..0x18].try_into().unwrap()),
            shirt_item_type: u16::from_le_bytes(b[0x18..0x1a].try_into().unwrap()),
            shoes_item_type: u16::from_le_bytes(b[0x1a..0x1c].try_into().unwrap()),
            hat_color: u16::from_le_bytes(b[0x1c..0x1e].try_into().unwrap()),
            pants_color: u16::from_le_bytes(b[0x1e..0x20].try_into().unwrap()),
            shirt_color: u16::from_le_bytes(b[0x20..0x22].try_into().unwrap()),
            shoes_color: u16::from_le_bytes(b[0x22..0x24].try_into().unwrap()),
            happiness: u8::from_le_bytes(b[0x24..0x25].try_into().unwrap()),
            dead: u8::from_le_bytes(b[0x25..0x26].try_into().unwrap()),
            _padding: core::array::from_fn(|k| u8::from_le_bytes(b[0x26 + k * 1..0x26 + (k + 1) * 1].try_into().unwrap())),
        }
    }
}
impl Default for BlockheadPreviewDataNew { fn default() -> Self { Self::read_le(&[0u8; 40]) } }
#[cfg(test)]
mod test_blockhead_preview_data_new {
    use super::*;
    #[test]
    fn layout() {
        assert_eq!(core::mem::size_of::<BlockheadPreviewDataNew>(), 0x28);
        assert_eq!(<BlockheadPreviewDataNew as WireStruct>::SIZE, 0x28);
        assert_eq!(core::mem::offset_of!(BlockheadPreviewDataNew, skin_options), 0x0);
        assert_eq!(core::mem::offset_of!(BlockheadPreviewDataNew, hat_item_type), 0x14);
        assert_eq!(core::mem::offset_of!(BlockheadPreviewDataNew, pants_item_type), 0x16);
        assert_eq!(core::mem::offset_of!(BlockheadPreviewDataNew, shirt_item_type), 0x18);
        assert_eq!(core::mem::offset_of!(BlockheadPreviewDataNew, shoes_item_type), 0x1a);
        assert_eq!(core::mem::offset_of!(BlockheadPreviewDataNew, hat_color), 0x1c);
        assert_eq!(core::mem::offset_of!(BlockheadPreviewDataNew, pants_color), 0x1e);
        assert_eq!(core::mem::offset_of!(BlockheadPreviewDataNew, shirt_color), 0x20);
        assert_eq!(core::mem::offset_of!(BlockheadPreviewDataNew, shoes_color), 0x22);
        assert_eq!(core::mem::offset_of!(BlockheadPreviewDataNew, happiness), 0x24);
        assert_eq!(core::mem::offset_of!(BlockheadPreviewDataNew, dead), 0x25);
        assert_eq!(core::mem::offset_of!(BlockheadPreviewDataNew, _padding), 0x26);
    }
    #[test]
    fn roundtrip() {
        let mut raw = [0u8; 0x28];
        for (i, x) in raw.iter_mut().enumerate() { *x = (i as u8).wrapping_mul(37).wrapping_add(11); }
        let v = BlockheadPreviewDataNew::read_le(&raw);
        let mut back = vec![0u8; raw.len()];
        v.write_le(&mut back);
        assert_eq!(&back[..], &raw[..]);   // byte-exact incl. padding (floats compared as bytes, NaN-safe)
        assert!(<BlockheadPreviewDataNew>::default().to_vec().iter().all(|&x| x == 0));
    }
}

/// size 0x48
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct BlockheadCreationData {
    /// DynamicObjectNetData @ 0x0
    pub dynamic_object_net_data: DynamicObjectNetData,
    /// struct BlockheadSkinOptions @ 0x18
    pub skin_options: BlockheadSkinOptions,
    /// uint16_t @ 0x2c
    pub hat_item_type: u16,
    /// uint16_t @ 0x2e
    pub pants_item_type: u16,
    /// uint16_t @ 0x30
    pub shirt_item_type: u16,
    /// uint16_t @ 0x32
    pub shoes_item_type: u16,
    /// uint16_t @ 0x34
    pub hat_color: u16,
    /// uint16_t @ 0x36
    pub pants_color: u16,
    /// uint16_t @ 0x38
    pub shirt_color: u16,
    /// uint16_t @ 0x3a
    pub shoes_color: u16,
    /// uint16_t @ 0x3c
    pub view_radius: u16,
    /// uint8_t @ 0x3e
    pub happiness: u8,
    /// uint8_t @ 0x3f
    pub dead: u8,
    /// uint8_t @ 0x40
    pub animation_type: u8,
    /// uint8_t @ 0x41
    pub sub_animation_type: u8,
    /// uint8_t @ 0x42
    pub traverse_from_key_frame: u8,
    /// uint8_t @ 0x43
    pub traverse_to_key_frame: u8,
    /// uint8_t @ 0x44
    pub regenerating: u8,
    /// uint8_t @ 0x45
    pub on_trade_mission: u8,
    /// explicit padding at 0x46; preserved on read, zero by default
    pub padding: [u8; 2],
}
impl WireStruct for BlockheadCreationData {
    const SIZE: usize = 0x48;
    fn write_le(&self, out: &mut [u8]) {
        assert!(out.len() >= Self::SIZE);
        out[..Self::SIZE].fill(0);
        self.dynamic_object_net_data.write_le(&mut out[0x0..]);
        self.skin_options.write_le(&mut out[0x18..]);
        out[0x2c..0x2e].copy_from_slice(&self.hat_item_type.to_le_bytes());
        out[0x2e..0x30].copy_from_slice(&self.pants_item_type.to_le_bytes());
        out[0x30..0x32].copy_from_slice(&self.shirt_item_type.to_le_bytes());
        out[0x32..0x34].copy_from_slice(&self.shoes_item_type.to_le_bytes());
        out[0x34..0x36].copy_from_slice(&self.hat_color.to_le_bytes());
        out[0x36..0x38].copy_from_slice(&self.pants_color.to_le_bytes());
        out[0x38..0x3a].copy_from_slice(&self.shirt_color.to_le_bytes());
        out[0x3a..0x3c].copy_from_slice(&self.shoes_color.to_le_bytes());
        out[0x3c..0x3e].copy_from_slice(&self.view_radius.to_le_bytes());
        out[0x3e..0x3f].copy_from_slice(&self.happiness.to_le_bytes());
        out[0x3f..0x40].copy_from_slice(&self.dead.to_le_bytes());
        out[0x40..0x41].copy_from_slice(&self.animation_type.to_le_bytes());
        out[0x41..0x42].copy_from_slice(&self.sub_animation_type.to_le_bytes());
        out[0x42..0x43].copy_from_slice(&self.traverse_from_key_frame.to_le_bytes());
        out[0x43..0x44].copy_from_slice(&self.traverse_to_key_frame.to_le_bytes());
        out[0x44..0x45].copy_from_slice(&self.regenerating.to_le_bytes());
        out[0x45..0x46].copy_from_slice(&self.on_trade_mission.to_le_bytes());
        out[0x46..0x48].copy_from_slice(&self.padding);
    }
    fn read_le(b: &[u8]) -> Self {
        assert!(b.len() >= Self::SIZE);
        Self {
            dynamic_object_net_data: DynamicObjectNetData::read_le(&b[0x0..]),
            skin_options: BlockheadSkinOptions::read_le(&b[0x18..]),
            hat_item_type: u16::from_le_bytes(b[0x2c..0x2e].try_into().unwrap()),
            pants_item_type: u16::from_le_bytes(b[0x2e..0x30].try_into().unwrap()),
            shirt_item_type: u16::from_le_bytes(b[0x30..0x32].try_into().unwrap()),
            shoes_item_type: u16::from_le_bytes(b[0x32..0x34].try_into().unwrap()),
            hat_color: u16::from_le_bytes(b[0x34..0x36].try_into().unwrap()),
            pants_color: u16::from_le_bytes(b[0x36..0x38].try_into().unwrap()),
            shirt_color: u16::from_le_bytes(b[0x38..0x3a].try_into().unwrap()),
            shoes_color: u16::from_le_bytes(b[0x3a..0x3c].try_into().unwrap()),
            view_radius: u16::from_le_bytes(b[0x3c..0x3e].try_into().unwrap()),
            happiness: u8::from_le_bytes(b[0x3e..0x3f].try_into().unwrap()),
            dead: u8::from_le_bytes(b[0x3f..0x40].try_into().unwrap()),
            animation_type: u8::from_le_bytes(b[0x40..0x41].try_into().unwrap()),
            sub_animation_type: u8::from_le_bytes(b[0x41..0x42].try_into().unwrap()),
            traverse_from_key_frame: u8::from_le_bytes(b[0x42..0x43].try_into().unwrap()),
            traverse_to_key_frame: u8::from_le_bytes(b[0x43..0x44].try_into().unwrap()),
            regenerating: u8::from_le_bytes(b[0x44..0x45].try_into().unwrap()),
            on_trade_mission: u8::from_le_bytes(b[0x45..0x46].try_into().unwrap()),
            padding: b[0x46..0x48].try_into().unwrap(),
        }
    }
}
impl Default for BlockheadCreationData { fn default() -> Self { Self::read_le(&[0u8; 72]) } }
#[cfg(test)]
mod test_blockhead_creation_data {
    use super::*;
    #[test]
    fn layout() {
        assert_eq!(core::mem::size_of::<BlockheadCreationData>(), 0x48);
        assert_eq!(<BlockheadCreationData as WireStruct>::SIZE, 0x48);
        assert_eq!(core::mem::offset_of!(BlockheadCreationData, dynamic_object_net_data), 0x0);
        assert_eq!(core::mem::offset_of!(BlockheadCreationData, skin_options), 0x18);
        assert_eq!(core::mem::offset_of!(BlockheadCreationData, hat_item_type), 0x2c);
        assert_eq!(core::mem::offset_of!(BlockheadCreationData, pants_item_type), 0x2e);
        assert_eq!(core::mem::offset_of!(BlockheadCreationData, shirt_item_type), 0x30);
        assert_eq!(core::mem::offset_of!(BlockheadCreationData, shoes_item_type), 0x32);
        assert_eq!(core::mem::offset_of!(BlockheadCreationData, hat_color), 0x34);
        assert_eq!(core::mem::offset_of!(BlockheadCreationData, pants_color), 0x36);
        assert_eq!(core::mem::offset_of!(BlockheadCreationData, shirt_color), 0x38);
        assert_eq!(core::mem::offset_of!(BlockheadCreationData, shoes_color), 0x3a);
        assert_eq!(core::mem::offset_of!(BlockheadCreationData, view_radius), 0x3c);
        assert_eq!(core::mem::offset_of!(BlockheadCreationData, happiness), 0x3e);
        assert_eq!(core::mem::offset_of!(BlockheadCreationData, dead), 0x3f);
        assert_eq!(core::mem::offset_of!(BlockheadCreationData, animation_type), 0x40);
        assert_eq!(core::mem::offset_of!(BlockheadCreationData, sub_animation_type), 0x41);
        assert_eq!(core::mem::offset_of!(BlockheadCreationData, traverse_from_key_frame), 0x42);
        assert_eq!(core::mem::offset_of!(BlockheadCreationData, traverse_to_key_frame), 0x43);
        assert_eq!(core::mem::offset_of!(BlockheadCreationData, regenerating), 0x44);
        assert_eq!(core::mem::offset_of!(BlockheadCreationData, on_trade_mission), 0x45);
        assert_eq!(core::mem::offset_of!(BlockheadCreationData, padding), 0x46);
    }
    #[test]
    fn roundtrip() {
        let mut raw = [0u8; 0x48];
        for (i, x) in raw.iter_mut().enumerate() { *x = (i as u8).wrapping_mul(37).wrapping_add(11); }
        let v = BlockheadCreationData::read_le(&raw);
        let mut back = vec![0u8; raw.len()];
        v.write_le(&mut back);
        assert_eq!(&back[..], &raw[..]);   // byte-exact incl. padding (floats compared as bytes, NaN-safe)
        assert!(<BlockheadCreationData>::default().to_vec().iter().all(|&x| x == 0));
    }
}

/// size 0x70
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct BlockheadUpdateData {
    /// DynamicObjectNetData @ 0x0
    pub dynamic_object_net_data: DynamicObjectNetData,
    /// uint32_t @ 0x18
    pub from_square_x: u32,
    /// uint32_t @ 0x1c
    pub from_square_y: u32,
    /// uint32_t @ 0x20
    pub to_square_x: u32,
    /// uint32_t @ 0x24
    pub to_square_y: u32,
    /// uint32_t @ 0x28
    pub interacting_square_x: u32,
    /// uint32_t @ 0x2c
    pub interacting_square_y: u32,
    /// uint8_t @ 0x30
    pub traverse_type: u8,
    /// uint8_t @ 0x31
    pub traverse_from_key_frame: u8,
    /// uint8_t @ 0x32
    pub traverse_to_key_frame: u8,
    /// uint8_t @ 0x33
    pub happiness: u8,
    /// uint16_t @ 0x34
    pub interaction_item_type: u16,
    /// uint16_t @ 0x36
    pub terrain_difficulty: u16,
    /// uint8_t @ 0x38
    pub interacting: u8,
    /// uint8_t @ 0x39
    pub dead: u8,
    /// uint8_t @ 0x3a
    pub paused: u8,
    /// uint8_t @ 0x3b
    pub interaction_type: u8,
    /// uint8_t @ 0x3c
    pub is_currently_active: u8,
    /// uint8_t @ 0x3d
    pub animation_type: u8,
    /// uint8_t @ 0x3e
    pub sub_animation_type: u8,
    /// uint8_t @ 0x3f
    pub health: u8,
    /// int32_t @ 0x40
    pub fishing_rod_cast_x: i32,
    /// int32_t @ 0x44
    pub fishing_rod_cast_y: i32,
    /// uint64_t @ 0x48
    pub ride_object_id: u64,
    /// uint64_t @ 0x50
    pub interaction_object_id: u64,
    /// uint64_t @ 0x58
    pub interaction_workbench_id: u64,
    /// uint64_t @ 0x60
    pub fishing_rod_fish_unique_id: u64,
    /// uint8_t @ 0x68
    pub heat: u8,
    /// uint8_t @ 0x69
    pub has_coffee_energy: u8,
    /// uint8_t @ 0x6a
    pub is_offline: u8,
    /// uint8_t @ 0x6b
    pub regenerating: u8,
    /// uint8_t @ 0x6c
    pub is_in_jet_pack_free_flight_mode: u8,
    /// uint8_t @ 0x6d
    pub z_index: u8,
    /// uint8_t @ 0x6e
    pub on_trade_mission: u8,
    /// explicit padding at 0x6f; preserved on read, zero by default
    pub padding: [u8; 1],
}
impl WireStruct for BlockheadUpdateData {
    const SIZE: usize = 0x70;
    fn write_le(&self, out: &mut [u8]) {
        assert!(out.len() >= Self::SIZE);
        out[..Self::SIZE].fill(0);
        self.dynamic_object_net_data.write_le(&mut out[0x0..]);
        out[0x18..0x1c].copy_from_slice(&self.from_square_x.to_le_bytes());
        out[0x1c..0x20].copy_from_slice(&self.from_square_y.to_le_bytes());
        out[0x20..0x24].copy_from_slice(&self.to_square_x.to_le_bytes());
        out[0x24..0x28].copy_from_slice(&self.to_square_y.to_le_bytes());
        out[0x28..0x2c].copy_from_slice(&self.interacting_square_x.to_le_bytes());
        out[0x2c..0x30].copy_from_slice(&self.interacting_square_y.to_le_bytes());
        out[0x30..0x31].copy_from_slice(&self.traverse_type.to_le_bytes());
        out[0x31..0x32].copy_from_slice(&self.traverse_from_key_frame.to_le_bytes());
        out[0x32..0x33].copy_from_slice(&self.traverse_to_key_frame.to_le_bytes());
        out[0x33..0x34].copy_from_slice(&self.happiness.to_le_bytes());
        out[0x34..0x36].copy_from_slice(&self.interaction_item_type.to_le_bytes());
        out[0x36..0x38].copy_from_slice(&self.terrain_difficulty.to_le_bytes());
        out[0x38..0x39].copy_from_slice(&self.interacting.to_le_bytes());
        out[0x39..0x3a].copy_from_slice(&self.dead.to_le_bytes());
        out[0x3a..0x3b].copy_from_slice(&self.paused.to_le_bytes());
        out[0x3b..0x3c].copy_from_slice(&self.interaction_type.to_le_bytes());
        out[0x3c..0x3d].copy_from_slice(&self.is_currently_active.to_le_bytes());
        out[0x3d..0x3e].copy_from_slice(&self.animation_type.to_le_bytes());
        out[0x3e..0x3f].copy_from_slice(&self.sub_animation_type.to_le_bytes());
        out[0x3f..0x40].copy_from_slice(&self.health.to_le_bytes());
        out[0x40..0x44].copy_from_slice(&self.fishing_rod_cast_x.to_le_bytes());
        out[0x44..0x48].copy_from_slice(&self.fishing_rod_cast_y.to_le_bytes());
        out[0x48..0x50].copy_from_slice(&self.ride_object_id.to_le_bytes());
        out[0x50..0x58].copy_from_slice(&self.interaction_object_id.to_le_bytes());
        out[0x58..0x60].copy_from_slice(&self.interaction_workbench_id.to_le_bytes());
        out[0x60..0x68].copy_from_slice(&self.fishing_rod_fish_unique_id.to_le_bytes());
        out[0x68..0x69].copy_from_slice(&self.heat.to_le_bytes());
        out[0x69..0x6a].copy_from_slice(&self.has_coffee_energy.to_le_bytes());
        out[0x6a..0x6b].copy_from_slice(&self.is_offline.to_le_bytes());
        out[0x6b..0x6c].copy_from_slice(&self.regenerating.to_le_bytes());
        out[0x6c..0x6d].copy_from_slice(&self.is_in_jet_pack_free_flight_mode.to_le_bytes());
        out[0x6d..0x6e].copy_from_slice(&self.z_index.to_le_bytes());
        out[0x6e..0x6f].copy_from_slice(&self.on_trade_mission.to_le_bytes());
        out[0x6f..0x70].copy_from_slice(&self.padding);
    }
    fn read_le(b: &[u8]) -> Self {
        assert!(b.len() >= Self::SIZE);
        Self {
            dynamic_object_net_data: DynamicObjectNetData::read_le(&b[0x0..]),
            from_square_x: u32::from_le_bytes(b[0x18..0x1c].try_into().unwrap()),
            from_square_y: u32::from_le_bytes(b[0x1c..0x20].try_into().unwrap()),
            to_square_x: u32::from_le_bytes(b[0x20..0x24].try_into().unwrap()),
            to_square_y: u32::from_le_bytes(b[0x24..0x28].try_into().unwrap()),
            interacting_square_x: u32::from_le_bytes(b[0x28..0x2c].try_into().unwrap()),
            interacting_square_y: u32::from_le_bytes(b[0x2c..0x30].try_into().unwrap()),
            traverse_type: u8::from_le_bytes(b[0x30..0x31].try_into().unwrap()),
            traverse_from_key_frame: u8::from_le_bytes(b[0x31..0x32].try_into().unwrap()),
            traverse_to_key_frame: u8::from_le_bytes(b[0x32..0x33].try_into().unwrap()),
            happiness: u8::from_le_bytes(b[0x33..0x34].try_into().unwrap()),
            interaction_item_type: u16::from_le_bytes(b[0x34..0x36].try_into().unwrap()),
            terrain_difficulty: u16::from_le_bytes(b[0x36..0x38].try_into().unwrap()),
            interacting: u8::from_le_bytes(b[0x38..0x39].try_into().unwrap()),
            dead: u8::from_le_bytes(b[0x39..0x3a].try_into().unwrap()),
            paused: u8::from_le_bytes(b[0x3a..0x3b].try_into().unwrap()),
            interaction_type: u8::from_le_bytes(b[0x3b..0x3c].try_into().unwrap()),
            is_currently_active: u8::from_le_bytes(b[0x3c..0x3d].try_into().unwrap()),
            animation_type: u8::from_le_bytes(b[0x3d..0x3e].try_into().unwrap()),
            sub_animation_type: u8::from_le_bytes(b[0x3e..0x3f].try_into().unwrap()),
            health: u8::from_le_bytes(b[0x3f..0x40].try_into().unwrap()),
            fishing_rod_cast_x: i32::from_le_bytes(b[0x40..0x44].try_into().unwrap()),
            fishing_rod_cast_y: i32::from_le_bytes(b[0x44..0x48].try_into().unwrap()),
            ride_object_id: u64::from_le_bytes(b[0x48..0x50].try_into().unwrap()),
            interaction_object_id: u64::from_le_bytes(b[0x50..0x58].try_into().unwrap()),
            interaction_workbench_id: u64::from_le_bytes(b[0x58..0x60].try_into().unwrap()),
            fishing_rod_fish_unique_id: u64::from_le_bytes(b[0x60..0x68].try_into().unwrap()),
            heat: u8::from_le_bytes(b[0x68..0x69].try_into().unwrap()),
            has_coffee_energy: u8::from_le_bytes(b[0x69..0x6a].try_into().unwrap()),
            is_offline: u8::from_le_bytes(b[0x6a..0x6b].try_into().unwrap()),
            regenerating: u8::from_le_bytes(b[0x6b..0x6c].try_into().unwrap()),
            is_in_jet_pack_free_flight_mode: u8::from_le_bytes(b[0x6c..0x6d].try_into().unwrap()),
            z_index: u8::from_le_bytes(b[0x6d..0x6e].try_into().unwrap()),
            on_trade_mission: u8::from_le_bytes(b[0x6e..0x6f].try_into().unwrap()),
            padding: b[0x6f..0x70].try_into().unwrap(),
        }
    }
}
impl Default for BlockheadUpdateData { fn default() -> Self { Self::read_le(&[0u8; 112]) } }
#[cfg(test)]
mod test_blockhead_update_data {
    use super::*;
    #[test]
    fn layout() {
        assert_eq!(core::mem::size_of::<BlockheadUpdateData>(), 0x70);
        assert_eq!(<BlockheadUpdateData as WireStruct>::SIZE, 0x70);
        assert_eq!(core::mem::offset_of!(BlockheadUpdateData, dynamic_object_net_data), 0x0);
        assert_eq!(core::mem::offset_of!(BlockheadUpdateData, from_square_x), 0x18);
        assert_eq!(core::mem::offset_of!(BlockheadUpdateData, from_square_y), 0x1c);
        assert_eq!(core::mem::offset_of!(BlockheadUpdateData, to_square_x), 0x20);
        assert_eq!(core::mem::offset_of!(BlockheadUpdateData, to_square_y), 0x24);
        assert_eq!(core::mem::offset_of!(BlockheadUpdateData, interacting_square_x), 0x28);
        assert_eq!(core::mem::offset_of!(BlockheadUpdateData, interacting_square_y), 0x2c);
        assert_eq!(core::mem::offset_of!(BlockheadUpdateData, traverse_type), 0x30);
        assert_eq!(core::mem::offset_of!(BlockheadUpdateData, traverse_from_key_frame), 0x31);
        assert_eq!(core::mem::offset_of!(BlockheadUpdateData, traverse_to_key_frame), 0x32);
        assert_eq!(core::mem::offset_of!(BlockheadUpdateData, happiness), 0x33);
        assert_eq!(core::mem::offset_of!(BlockheadUpdateData, interaction_item_type), 0x34);
        assert_eq!(core::mem::offset_of!(BlockheadUpdateData, terrain_difficulty), 0x36);
        assert_eq!(core::mem::offset_of!(BlockheadUpdateData, interacting), 0x38);
        assert_eq!(core::mem::offset_of!(BlockheadUpdateData, dead), 0x39);
        assert_eq!(core::mem::offset_of!(BlockheadUpdateData, paused), 0x3a);
        assert_eq!(core::mem::offset_of!(BlockheadUpdateData, interaction_type), 0x3b);
        assert_eq!(core::mem::offset_of!(BlockheadUpdateData, is_currently_active), 0x3c);
        assert_eq!(core::mem::offset_of!(BlockheadUpdateData, animation_type), 0x3d);
        assert_eq!(core::mem::offset_of!(BlockheadUpdateData, sub_animation_type), 0x3e);
        assert_eq!(core::mem::offset_of!(BlockheadUpdateData, health), 0x3f);
        assert_eq!(core::mem::offset_of!(BlockheadUpdateData, fishing_rod_cast_x), 0x40);
        assert_eq!(core::mem::offset_of!(BlockheadUpdateData, fishing_rod_cast_y), 0x44);
        assert_eq!(core::mem::offset_of!(BlockheadUpdateData, ride_object_id), 0x48);
        assert_eq!(core::mem::offset_of!(BlockheadUpdateData, interaction_object_id), 0x50);
        assert_eq!(core::mem::offset_of!(BlockheadUpdateData, interaction_workbench_id), 0x58);
        assert_eq!(core::mem::offset_of!(BlockheadUpdateData, fishing_rod_fish_unique_id), 0x60);
        assert_eq!(core::mem::offset_of!(BlockheadUpdateData, heat), 0x68);
        assert_eq!(core::mem::offset_of!(BlockheadUpdateData, has_coffee_energy), 0x69);
        assert_eq!(core::mem::offset_of!(BlockheadUpdateData, is_offline), 0x6a);
        assert_eq!(core::mem::offset_of!(BlockheadUpdateData, regenerating), 0x6b);
        assert_eq!(core::mem::offset_of!(BlockheadUpdateData, is_in_jet_pack_free_flight_mode), 0x6c);
        assert_eq!(core::mem::offset_of!(BlockheadUpdateData, z_index), 0x6d);
        assert_eq!(core::mem::offset_of!(BlockheadUpdateData, on_trade_mission), 0x6e);
        assert_eq!(core::mem::offset_of!(BlockheadUpdateData, padding), 0x6f);
    }
    #[test]
    fn roundtrip() {
        let mut raw = [0u8; 0x70];
        for (i, x) in raw.iter_mut().enumerate() { *x = (i as u8).wrapping_mul(37).wrapping_add(11); }
        let v = BlockheadUpdateData::read_le(&raw);
        let mut back = vec![0u8; raw.len()];
        v.write_le(&mut back);
        assert_eq!(&back[..], &raw[..]);   // byte-exact incl. padding (floats compared as bytes, NaN-safe)
        assert!(<BlockheadUpdateData>::default().to_vec().iter().all(|&x| x == 0));
    }
}

/// size 0x10
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PickupAcceptRejectHeader {
    /// uint8_t @ 0x0
    pub type_: u8,
    /// uint8_t @ 0x1
    pub accepted: u8,
    /// explicit padding at 0x2; preserved on read, zero by default
    pub padding: [u8; 6],
    /// uint64_t @ 0x8
    pub blockhead_unique_id: u64,
}
impl WireStruct for PickupAcceptRejectHeader {
    const SIZE: usize = 0x10;
    fn write_le(&self, out: &mut [u8]) {
        assert!(out.len() >= Self::SIZE);
        out[..Self::SIZE].fill(0);
        out[0x0..0x1].copy_from_slice(&self.type_.to_le_bytes());
        out[0x1..0x2].copy_from_slice(&self.accepted.to_le_bytes());
        out[0x2..0x8].copy_from_slice(&self.padding);
        out[0x8..0x10].copy_from_slice(&self.blockhead_unique_id.to_le_bytes());
    }
    fn read_le(b: &[u8]) -> Self {
        assert!(b.len() >= Self::SIZE);
        Self {
            type_: u8::from_le_bytes(b[0x0..0x1].try_into().unwrap()),
            accepted: u8::from_le_bytes(b[0x1..0x2].try_into().unwrap()),
            padding: b[0x2..0x8].try_into().unwrap(),
            blockhead_unique_id: u64::from_le_bytes(b[0x8..0x10].try_into().unwrap()),
        }
    }
}
impl Default for PickupAcceptRejectHeader { fn default() -> Self { Self::read_le(&[0u8; 16]) } }
#[cfg(test)]
mod test_pickup_accept_reject_header {
    use super::*;
    #[test]
    fn layout() {
        assert_eq!(core::mem::size_of::<PickupAcceptRejectHeader>(), 0x10);
        assert_eq!(<PickupAcceptRejectHeader as WireStruct>::SIZE, 0x10);
        assert_eq!(core::mem::offset_of!(PickupAcceptRejectHeader, type_), 0x0);
        assert_eq!(core::mem::offset_of!(PickupAcceptRejectHeader, accepted), 0x1);
        assert_eq!(core::mem::offset_of!(PickupAcceptRejectHeader, padding), 0x2);
        assert_eq!(core::mem::offset_of!(PickupAcceptRejectHeader, blockhead_unique_id), 0x8);
    }
    #[test]
    fn roundtrip() {
        let mut raw = [0u8; 0x10];
        for (i, x) in raw.iter_mut().enumerate() { *x = (i as u8).wrapping_mul(37).wrapping_add(11); }
        let v = PickupAcceptRejectHeader::read_le(&raw);
        let mut back = vec![0u8; raw.len()];
        v.write_le(&mut back);
        assert_eq!(&back[..], &raw[..]);   // byte-exact incl. padding (floats compared as bytes, NaN-safe)
        assert!(<PickupAcceptRejectHeader>::default().to_vec().iter().all(|&x| x == 0));
    }
}

/// size 0x28
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct BoatCreationNetData {
    /// DynamicObjectNetData @ 0x0
    pub dynamic_object_net_data: DynamicObjectNetData,
    /// uint64_t @ 0x18
    pub rider_id: u64,
    /// int16_t @ 0x20
    pub actual_x_speed: i16,
    /// explicit padding at 0x22; preserved on read, zero by default
    pub padding: [u8; 6],
}
impl WireStruct for BoatCreationNetData {
    const SIZE: usize = 0x28;
    fn write_le(&self, out: &mut [u8]) {
        assert!(out.len() >= Self::SIZE);
        out[..Self::SIZE].fill(0);
        self.dynamic_object_net_data.write_le(&mut out[0x0..]);
        out[0x18..0x20].copy_from_slice(&self.rider_id.to_le_bytes());
        out[0x20..0x22].copy_from_slice(&self.actual_x_speed.to_le_bytes());
        out[0x22..0x28].copy_from_slice(&self.padding);
    }
    fn read_le(b: &[u8]) -> Self {
        assert!(b.len() >= Self::SIZE);
        Self {
            dynamic_object_net_data: DynamicObjectNetData::read_le(&b[0x0..]),
            rider_id: u64::from_le_bytes(b[0x18..0x20].try_into().unwrap()),
            actual_x_speed: i16::from_le_bytes(b[0x20..0x22].try_into().unwrap()),
            padding: b[0x22..0x28].try_into().unwrap(),
        }
    }
}
impl Default for BoatCreationNetData { fn default() -> Self { Self::read_le(&[0u8; 40]) } }
#[cfg(test)]
mod test_boat_creation_net_data {
    use super::*;
    #[test]
    fn layout() {
        assert_eq!(core::mem::size_of::<BoatCreationNetData>(), 0x28);
        assert_eq!(<BoatCreationNetData as WireStruct>::SIZE, 0x28);
        assert_eq!(core::mem::offset_of!(BoatCreationNetData, dynamic_object_net_data), 0x0);
        assert_eq!(core::mem::offset_of!(BoatCreationNetData, rider_id), 0x18);
        assert_eq!(core::mem::offset_of!(BoatCreationNetData, actual_x_speed), 0x20);
        assert_eq!(core::mem::offset_of!(BoatCreationNetData, padding), 0x22);
    }
    #[test]
    fn roundtrip() {
        let mut raw = [0u8; 0x28];
        for (i, x) in raw.iter_mut().enumerate() { *x = (i as u8).wrapping_mul(37).wrapping_add(11); }
        let v = BoatCreationNetData::read_le(&raw);
        let mut back = vec![0u8; raw.len()];
        v.write_le(&mut back);
        assert_eq!(&back[..], &raw[..]);   // byte-exact incl. padding (floats compared as bytes, NaN-safe)
        assert!(<BoatCreationNetData>::default().to_vec().iter().all(|&x| x == 0));
    }
}

/// size 0x40
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CaveTrollUpdateData {
    /// NPCUpdateNetData @ 0x0
    pub dynamic_object_net_data: NPCUpdateNetData,
    /// uint32_t @ 0x18
    pub from_square_x: u32,
    /// uint32_t @ 0x1c
    pub from_square_y: u32,
    /// uint32_t @ 0x20
    pub to_square_x: u32,
    /// uint32_t @ 0x24
    pub to_square_y: u32,
    /// uint32_t @ 0x28
    pub interacting_square_x: u32,
    /// uint32_t @ 0x2c
    pub interacting_square_y: u32,
    /// uint8_t @ 0x30
    pub traverse_type: u8,
    /// uint8_t @ 0x31
    pub traverse_from_key_frame: u8,
    /// uint8_t @ 0x32
    pub traverse_to_key_frame: u8,
    /// uint8_t @ 0x33
    pub dead: u8,
    /// uint16_t @ 0x34
    pub terrain_difficulty: u16,
    /// uint8_t @ 0x36
    pub interacting: u8,
    /// uint8_t @ 0x37
    pub interaction_type: u8,
    /// uint8_t @ 0x38
    pub animation_type: u8,
    /// uint8_t @ 0x39
    pub sub_animation_type: u8,
    /// uint8_t @ 0x3a
    pub health: u8,
    /// explicit padding at 0x3b; preserved on read, zero by default
    pub padding: [u8; 5],
}
impl WireStruct for CaveTrollUpdateData {
    const SIZE: usize = 0x40;
    fn write_le(&self, out: &mut [u8]) {
        assert!(out.len() >= Self::SIZE);
        out[..Self::SIZE].fill(0);
        self.dynamic_object_net_data.write_le(&mut out[0x0..]);
        out[0x18..0x1c].copy_from_slice(&self.from_square_x.to_le_bytes());
        out[0x1c..0x20].copy_from_slice(&self.from_square_y.to_le_bytes());
        out[0x20..0x24].copy_from_slice(&self.to_square_x.to_le_bytes());
        out[0x24..0x28].copy_from_slice(&self.to_square_y.to_le_bytes());
        out[0x28..0x2c].copy_from_slice(&self.interacting_square_x.to_le_bytes());
        out[0x2c..0x30].copy_from_slice(&self.interacting_square_y.to_le_bytes());
        out[0x30..0x31].copy_from_slice(&self.traverse_type.to_le_bytes());
        out[0x31..0x32].copy_from_slice(&self.traverse_from_key_frame.to_le_bytes());
        out[0x32..0x33].copy_from_slice(&self.traverse_to_key_frame.to_le_bytes());
        out[0x33..0x34].copy_from_slice(&self.dead.to_le_bytes());
        out[0x34..0x36].copy_from_slice(&self.terrain_difficulty.to_le_bytes());
        out[0x36..0x37].copy_from_slice(&self.interacting.to_le_bytes());
        out[0x37..0x38].copy_from_slice(&self.interaction_type.to_le_bytes());
        out[0x38..0x39].copy_from_slice(&self.animation_type.to_le_bytes());
        out[0x39..0x3a].copy_from_slice(&self.sub_animation_type.to_le_bytes());
        out[0x3a..0x3b].copy_from_slice(&self.health.to_le_bytes());
        out[0x3b..0x40].copy_from_slice(&self.padding);
    }
    fn read_le(b: &[u8]) -> Self {
        assert!(b.len() >= Self::SIZE);
        Self {
            dynamic_object_net_data: NPCUpdateNetData::read_le(&b[0x0..]),
            from_square_x: u32::from_le_bytes(b[0x18..0x1c].try_into().unwrap()),
            from_square_y: u32::from_le_bytes(b[0x1c..0x20].try_into().unwrap()),
            to_square_x: u32::from_le_bytes(b[0x20..0x24].try_into().unwrap()),
            to_square_y: u32::from_le_bytes(b[0x24..0x28].try_into().unwrap()),
            interacting_square_x: u32::from_le_bytes(b[0x28..0x2c].try_into().unwrap()),
            interacting_square_y: u32::from_le_bytes(b[0x2c..0x30].try_into().unwrap()),
            traverse_type: u8::from_le_bytes(b[0x30..0x31].try_into().unwrap()),
            traverse_from_key_frame: u8::from_le_bytes(b[0x31..0x32].try_into().unwrap()),
            traverse_to_key_frame: u8::from_le_bytes(b[0x32..0x33].try_into().unwrap()),
            dead: u8::from_le_bytes(b[0x33..0x34].try_into().unwrap()),
            terrain_difficulty: u16::from_le_bytes(b[0x34..0x36].try_into().unwrap()),
            interacting: u8::from_le_bytes(b[0x36..0x37].try_into().unwrap()),
            interaction_type: u8::from_le_bytes(b[0x37..0x38].try_into().unwrap()),
            animation_type: u8::from_le_bytes(b[0x38..0x39].try_into().unwrap()),
            sub_animation_type: u8::from_le_bytes(b[0x39..0x3a].try_into().unwrap()),
            health: u8::from_le_bytes(b[0x3a..0x3b].try_into().unwrap()),
            padding: b[0x3b..0x40].try_into().unwrap(),
        }
    }
}
impl Default for CaveTrollUpdateData { fn default() -> Self { Self::read_le(&[0u8; 64]) } }
#[cfg(test)]
mod test_cave_troll_update_data {
    use super::*;
    #[test]
    fn layout() {
        assert_eq!(core::mem::size_of::<CaveTrollUpdateData>(), 0x40);
        assert_eq!(<CaveTrollUpdateData as WireStruct>::SIZE, 0x40);
        assert_eq!(core::mem::offset_of!(CaveTrollUpdateData, dynamic_object_net_data), 0x0);
        assert_eq!(core::mem::offset_of!(CaveTrollUpdateData, from_square_x), 0x18);
        assert_eq!(core::mem::offset_of!(CaveTrollUpdateData, from_square_y), 0x1c);
        assert_eq!(core::mem::offset_of!(CaveTrollUpdateData, to_square_x), 0x20);
        assert_eq!(core::mem::offset_of!(CaveTrollUpdateData, to_square_y), 0x24);
        assert_eq!(core::mem::offset_of!(CaveTrollUpdateData, interacting_square_x), 0x28);
        assert_eq!(core::mem::offset_of!(CaveTrollUpdateData, interacting_square_y), 0x2c);
        assert_eq!(core::mem::offset_of!(CaveTrollUpdateData, traverse_type), 0x30);
        assert_eq!(core::mem::offset_of!(CaveTrollUpdateData, traverse_from_key_frame), 0x31);
        assert_eq!(core::mem::offset_of!(CaveTrollUpdateData, traverse_to_key_frame), 0x32);
        assert_eq!(core::mem::offset_of!(CaveTrollUpdateData, dead), 0x33);
        assert_eq!(core::mem::offset_of!(CaveTrollUpdateData, terrain_difficulty), 0x34);
        assert_eq!(core::mem::offset_of!(CaveTrollUpdateData, interacting), 0x36);
        assert_eq!(core::mem::offset_of!(CaveTrollUpdateData, interaction_type), 0x37);
        assert_eq!(core::mem::offset_of!(CaveTrollUpdateData, animation_type), 0x38);
        assert_eq!(core::mem::offset_of!(CaveTrollUpdateData, sub_animation_type), 0x39);
        assert_eq!(core::mem::offset_of!(CaveTrollUpdateData, health), 0x3a);
        assert_eq!(core::mem::offset_of!(CaveTrollUpdateData, padding), 0x3b);
    }
    #[test]
    fn roundtrip() {
        let mut raw = [0u8; 0x40];
        for (i, x) in raw.iter_mut().enumerate() { *x = (i as u8).wrapping_mul(37).wrapping_add(11); }
        let v = CaveTrollUpdateData::read_le(&raw);
        let mut back = vec![0u8; raw.len()];
        v.write_le(&mut back);
        assert_eq!(&back[..], &raw[..]);   // byte-exact incl. padding (floats compared as bytes, NaN-safe)
        assert!(<CaveTrollUpdateData>::default().to_vec().iter().all(|&x| x == 0));
    }
}

/// size 0x88
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CaveTrollCreationData {
    /// NPCCreationNetData @ 0x0
    pub dynamic_object_net_data: NPCCreationNetData,
    /// CaveTrollUpdateData @ 0x48
    pub cave_troll_update_data: CaveTrollUpdateData,
}
impl WireStruct for CaveTrollCreationData {
    const SIZE: usize = 0x88;
    fn write_le(&self, out: &mut [u8]) {
        assert!(out.len() >= Self::SIZE);
        out[..Self::SIZE].fill(0);
        self.dynamic_object_net_data.write_le(&mut out[0x0..]);
        self.cave_troll_update_data.write_le(&mut out[0x48..]);
    }
    fn read_le(b: &[u8]) -> Self {
        assert!(b.len() >= Self::SIZE);
        Self {
            dynamic_object_net_data: NPCCreationNetData::read_le(&b[0x0..]),
            cave_troll_update_data: CaveTrollUpdateData::read_le(&b[0x48..]),
        }
    }
}
impl Default for CaveTrollCreationData { fn default() -> Self { Self::read_le(&[0u8; 136]) } }
#[cfg(test)]
mod test_cave_troll_creation_data {
    use super::*;
    #[test]
    fn layout() {
        assert_eq!(core::mem::size_of::<CaveTrollCreationData>(), 0x88);
        assert_eq!(<CaveTrollCreationData as WireStruct>::SIZE, 0x88);
        assert_eq!(core::mem::offset_of!(CaveTrollCreationData, dynamic_object_net_data), 0x0);
        assert_eq!(core::mem::offset_of!(CaveTrollCreationData, cave_troll_update_data), 0x48);
    }
    #[test]
    fn roundtrip() {
        let mut raw = [0u8; 0x88];
        for (i, x) in raw.iter_mut().enumerate() { *x = (i as u8).wrapping_mul(37).wrapping_add(11); }
        let v = CaveTrollCreationData::read_le(&raw);
        let mut back = vec![0u8; raw.len()];
        v.write_le(&mut back);
        assert_eq!(&back[..], &raw[..]);   // byte-exact incl. padding (floats compared as bytes, NaN-safe)
        assert!(<CaveTrollCreationData>::default().to_vec().iter().all(|&x| x == 0));
    }
}

/// size 0x40
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ChestNetData {
    /// InteractionObjectCreationNetData @ 0x0
    pub interaction_object_creation_net_data: InteractionObjectCreationNetData,
    /// uint16_t @ 0x28
    pub render_items: [u16; 4],
    /// uint16_t @ 0x30
    pub render_item_data_bs: [u16; 4],
    /// uint8_t @ 0x38
    pub chest_type: u8,
    /// explicit padding at 0x39; preserved on read, zero by default
    pub padding: [u8; 7],
}
impl WireStruct for ChestNetData {
    const SIZE: usize = 0x40;
    fn write_le(&self, out: &mut [u8]) {
        assert!(out.len() >= Self::SIZE);
        out[..Self::SIZE].fill(0);
        self.interaction_object_creation_net_data.write_le(&mut out[0x0..]);
        for (k, e) in self.render_items.iter().enumerate() { out[0x28 + k * 2..0x28 + (k + 1) * 2].copy_from_slice(&e.to_le_bytes()); }
        for (k, e) in self.render_item_data_bs.iter().enumerate() { out[0x30 + k * 2..0x30 + (k + 1) * 2].copy_from_slice(&e.to_le_bytes()); }
        out[0x38..0x39].copy_from_slice(&self.chest_type.to_le_bytes());
        out[0x39..0x40].copy_from_slice(&self.padding);
    }
    fn read_le(b: &[u8]) -> Self {
        assert!(b.len() >= Self::SIZE);
        Self {
            interaction_object_creation_net_data: InteractionObjectCreationNetData::read_le(&b[0x0..]),
            render_items: core::array::from_fn(|k| u16::from_le_bytes(b[0x28 + k * 2..0x28 + (k + 1) * 2].try_into().unwrap())),
            render_item_data_bs: core::array::from_fn(|k| u16::from_le_bytes(b[0x30 + k * 2..0x30 + (k + 1) * 2].try_into().unwrap())),
            chest_type: u8::from_le_bytes(b[0x38..0x39].try_into().unwrap()),
            padding: b[0x39..0x40].try_into().unwrap(),
        }
    }
}
impl Default for ChestNetData { fn default() -> Self { Self::read_le(&[0u8; 64]) } }
#[cfg(test)]
mod test_chest_net_data {
    use super::*;
    #[test]
    fn layout() {
        assert_eq!(core::mem::size_of::<ChestNetData>(), 0x40);
        assert_eq!(<ChestNetData as WireStruct>::SIZE, 0x40);
        assert_eq!(core::mem::offset_of!(ChestNetData, interaction_object_creation_net_data), 0x0);
        assert_eq!(core::mem::offset_of!(ChestNetData, render_items), 0x28);
        assert_eq!(core::mem::offset_of!(ChestNetData, render_item_data_bs), 0x30);
        assert_eq!(core::mem::offset_of!(ChestNetData, chest_type), 0x38);
        assert_eq!(core::mem::offset_of!(ChestNetData, padding), 0x39);
    }
    #[test]
    fn roundtrip() {
        let mut raw = [0u8; 0x40];
        for (i, x) in raw.iter_mut().enumerate() { *x = (i as u8).wrapping_mul(37).wrapping_add(11); }
        let v = ChestNetData::read_le(&raw);
        let mut back = vec![0u8; raw.len()];
        v.write_le(&mut back);
        assert_eq!(&back[..], &raw[..]);   // byte-exact incl. padding (floats compared as bytes, NaN-safe)
        assert!(<ChestNetData>::default().to_vec().iter().all(|&x| x == 0));
    }
}

/// size 0x28
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ClownFishUpdateNetData {
    /// NPCUpdateNetData @ 0x0
    pub npc_update_net_data: NPCUpdateNetData,
    /// uint32_t @ 0x18
    pub to_square_x: u32,
    /// uint32_t @ 0x1c
    pub to_square_y: u32,
    /// int16_t @ 0x20
    pub walk_speed: i16,
    /// int8_t @ 0x22
    pub movement_direction: i8,
    /// explicit padding at 0x23; preserved on read, zero by default
    pub padding: [u8; 5],
}
impl WireStruct for ClownFishUpdateNetData {
    const SIZE: usize = 0x28;
    fn write_le(&self, out: &mut [u8]) {
        assert!(out.len() >= Self::SIZE);
        out[..Self::SIZE].fill(0);
        self.npc_update_net_data.write_le(&mut out[0x0..]);
        out[0x18..0x1c].copy_from_slice(&self.to_square_x.to_le_bytes());
        out[0x1c..0x20].copy_from_slice(&self.to_square_y.to_le_bytes());
        out[0x20..0x22].copy_from_slice(&self.walk_speed.to_le_bytes());
        out[0x22..0x23].copy_from_slice(&self.movement_direction.to_le_bytes());
        out[0x23..0x28].copy_from_slice(&self.padding);
    }
    fn read_le(b: &[u8]) -> Self {
        assert!(b.len() >= Self::SIZE);
        Self {
            npc_update_net_data: NPCUpdateNetData::read_le(&b[0x0..]),
            to_square_x: u32::from_le_bytes(b[0x18..0x1c].try_into().unwrap()),
            to_square_y: u32::from_le_bytes(b[0x1c..0x20].try_into().unwrap()),
            walk_speed: i16::from_le_bytes(b[0x20..0x22].try_into().unwrap()),
            movement_direction: i8::from_le_bytes(b[0x22..0x23].try_into().unwrap()),
            padding: b[0x23..0x28].try_into().unwrap(),
        }
    }
}
impl Default for ClownFishUpdateNetData { fn default() -> Self { Self::read_le(&[0u8; 40]) } }
#[cfg(test)]
mod test_clown_fish_update_net_data {
    use super::*;
    #[test]
    fn layout() {
        assert_eq!(core::mem::size_of::<ClownFishUpdateNetData>(), 0x28);
        assert_eq!(<ClownFishUpdateNetData as WireStruct>::SIZE, 0x28);
        assert_eq!(core::mem::offset_of!(ClownFishUpdateNetData, npc_update_net_data), 0x0);
        assert_eq!(core::mem::offset_of!(ClownFishUpdateNetData, to_square_x), 0x18);
        assert_eq!(core::mem::offset_of!(ClownFishUpdateNetData, to_square_y), 0x1c);
        assert_eq!(core::mem::offset_of!(ClownFishUpdateNetData, walk_speed), 0x20);
        assert_eq!(core::mem::offset_of!(ClownFishUpdateNetData, movement_direction), 0x22);
        assert_eq!(core::mem::offset_of!(ClownFishUpdateNetData, padding), 0x23);
    }
    #[test]
    fn roundtrip() {
        let mut raw = [0u8; 0x28];
        for (i, x) in raw.iter_mut().enumerate() { *x = (i as u8).wrapping_mul(37).wrapping_add(11); }
        let v = ClownFishUpdateNetData::read_le(&raw);
        let mut back = vec![0u8; raw.len()];
        v.write_le(&mut back);
        assert_eq!(&back[..], &raw[..]);   // byte-exact incl. padding (floats compared as bytes, NaN-safe)
        assert!(<ClownFishUpdateNetData>::default().to_vec().iter().all(|&x| x == 0));
    }
}

/// size 0x70
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ClownFishCreationNetData {
    /// NPCCreationNetData @ 0x0
    pub npc_creation_net_data: NPCCreationNetData,
    /// ClownFishUpdateNetData @ 0x48
    pub clown_fish_update_net_data: ClownFishUpdateNetData,
}
impl WireStruct for ClownFishCreationNetData {
    const SIZE: usize = 0x70;
    fn write_le(&self, out: &mut [u8]) {
        assert!(out.len() >= Self::SIZE);
        out[..Self::SIZE].fill(0);
        self.npc_creation_net_data.write_le(&mut out[0x0..]);
        self.clown_fish_update_net_data.write_le(&mut out[0x48..]);
    }
    fn read_le(b: &[u8]) -> Self {
        assert!(b.len() >= Self::SIZE);
        Self {
            npc_creation_net_data: NPCCreationNetData::read_le(&b[0x0..]),
            clown_fish_update_net_data: ClownFishUpdateNetData::read_le(&b[0x48..]),
        }
    }
}
impl Default for ClownFishCreationNetData { fn default() -> Self { Self::read_le(&[0u8; 112]) } }
#[cfg(test)]
mod test_clown_fish_creation_net_data {
    use super::*;
    #[test]
    fn layout() {
        assert_eq!(core::mem::size_of::<ClownFishCreationNetData>(), 0x70);
        assert_eq!(<ClownFishCreationNetData as WireStruct>::SIZE, 0x70);
        assert_eq!(core::mem::offset_of!(ClownFishCreationNetData, npc_creation_net_data), 0x0);
        assert_eq!(core::mem::offset_of!(ClownFishCreationNetData, clown_fish_update_net_data), 0x48);
    }
    #[test]
    fn roundtrip() {
        let mut raw = [0u8; 0x70];
        for (i, x) in raw.iter_mut().enumerate() { *x = (i as u8).wrapping_mul(37).wrapping_add(11); }
        let v = ClownFishCreationNetData::read_le(&raw);
        let mut back = vec![0u8; raw.len()];
        v.write_le(&mut back);
        assert_eq!(&back[..], &raw[..]);   // byte-exact incl. padding (floats compared as bytes, NaN-safe)
        assert!(<ClownFishCreationNetData>::default().to_vec().iter().all(|&x| x == 0));
    }
}

/// size 0x20
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ColumnCreationNetData {
    /// DynamicObjectNetData @ 0x0
    pub dynamic_object_net_data: DynamicObjectNetData,
    /// uint16_t @ 0x18
    pub item_type: u16,
    /// uint8_t @ 0x1a
    pub configuration: u8,
    /// uint8_t @ 0x1b
    pub padding_a: u8,
    /// uint16_t @ 0x1c
    pub paint_color: u16,
    /// explicit padding at 0x1e; preserved on read, zero by default
    pub padding: [u8; 2],
}
impl WireStruct for ColumnCreationNetData {
    const SIZE: usize = 0x20;
    fn write_le(&self, out: &mut [u8]) {
        assert!(out.len() >= Self::SIZE);
        out[..Self::SIZE].fill(0);
        self.dynamic_object_net_data.write_le(&mut out[0x0..]);
        out[0x18..0x1a].copy_from_slice(&self.item_type.to_le_bytes());
        out[0x1a..0x1b].copy_from_slice(&self.configuration.to_le_bytes());
        out[0x1b..0x1c].copy_from_slice(&self.padding_a.to_le_bytes());
        out[0x1c..0x1e].copy_from_slice(&self.paint_color.to_le_bytes());
        out[0x1e..0x20].copy_from_slice(&self.padding);
    }
    fn read_le(b: &[u8]) -> Self {
        assert!(b.len() >= Self::SIZE);
        Self {
            dynamic_object_net_data: DynamicObjectNetData::read_le(&b[0x0..]),
            item_type: u16::from_le_bytes(b[0x18..0x1a].try_into().unwrap()),
            configuration: u8::from_le_bytes(b[0x1a..0x1b].try_into().unwrap()),
            padding_a: u8::from_le_bytes(b[0x1b..0x1c].try_into().unwrap()),
            paint_color: u16::from_le_bytes(b[0x1c..0x1e].try_into().unwrap()),
            padding: b[0x1e..0x20].try_into().unwrap(),
        }
    }
}
impl Default for ColumnCreationNetData { fn default() -> Self { Self::read_le(&[0u8; 32]) } }
#[cfg(test)]
mod test_column_creation_net_data {
    use super::*;
    #[test]
    fn layout() {
        assert_eq!(core::mem::size_of::<ColumnCreationNetData>(), 0x20);
        assert_eq!(<ColumnCreationNetData as WireStruct>::SIZE, 0x20);
        assert_eq!(core::mem::offset_of!(ColumnCreationNetData, dynamic_object_net_data), 0x0);
        assert_eq!(core::mem::offset_of!(ColumnCreationNetData, item_type), 0x18);
        assert_eq!(core::mem::offset_of!(ColumnCreationNetData, configuration), 0x1a);
        assert_eq!(core::mem::offset_of!(ColumnCreationNetData, padding_a), 0x1b);
        assert_eq!(core::mem::offset_of!(ColumnCreationNetData, paint_color), 0x1c);
        assert_eq!(core::mem::offset_of!(ColumnCreationNetData, padding), 0x1e);
    }
    #[test]
    fn roundtrip() {
        let mut raw = [0u8; 0x20];
        for (i, x) in raw.iter_mut().enumerate() { *x = (i as u8).wrapping_mul(37).wrapping_add(11); }
        let v = ColumnCreationNetData::read_le(&raw);
        let mut back = vec![0u8; raw.len()];
        v.write_le(&mut back);
        assert_eq!(&back[..], &raw[..]);   // byte-exact incl. padding (floats compared as bytes, NaN-safe)
        assert!(<ColumnCreationNetData>::default().to_vec().iter().all(|&x| x == 0));
    }
}

/// size 0x28
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct DodoUpdateNetData {
    /// NPCUpdateNetData @ 0x0
    pub npc_update_net_data: NPCUpdateNetData,
    /// uint32_t @ 0x18
    pub to_square_x: u32,
    /// uint32_t @ 0x1c
    pub to_square_y: u32,
    /// int16_t @ 0x20
    pub walk_speed: i16,
    /// int8_t @ 0x22
    pub movement_direction: i8,
    /// int8_t @ 0x23
    pub jump_and_cluck: i8,
    /// explicit padding at 0x24; preserved on read, zero by default
    pub padding: [u8; 3],
    /// implicit padding at 0x27; preserved on read, zero by default
    pub gap_27: [u8; 1],
}
impl WireStruct for DodoUpdateNetData {
    const SIZE: usize = 0x28;
    fn write_le(&self, out: &mut [u8]) {
        assert!(out.len() >= Self::SIZE);
        out[..Self::SIZE].fill(0);
        self.npc_update_net_data.write_le(&mut out[0x0..]);
        out[0x18..0x1c].copy_from_slice(&self.to_square_x.to_le_bytes());
        out[0x1c..0x20].copy_from_slice(&self.to_square_y.to_le_bytes());
        out[0x20..0x22].copy_from_slice(&self.walk_speed.to_le_bytes());
        out[0x22..0x23].copy_from_slice(&self.movement_direction.to_le_bytes());
        out[0x23..0x24].copy_from_slice(&self.jump_and_cluck.to_le_bytes());
        out[0x24..0x27].copy_from_slice(&self.padding);
        out[0x27..0x28].copy_from_slice(&self.gap_27);
    }
    fn read_le(b: &[u8]) -> Self {
        assert!(b.len() >= Self::SIZE);
        Self {
            npc_update_net_data: NPCUpdateNetData::read_le(&b[0x0..]),
            to_square_x: u32::from_le_bytes(b[0x18..0x1c].try_into().unwrap()),
            to_square_y: u32::from_le_bytes(b[0x1c..0x20].try_into().unwrap()),
            walk_speed: i16::from_le_bytes(b[0x20..0x22].try_into().unwrap()),
            movement_direction: i8::from_le_bytes(b[0x22..0x23].try_into().unwrap()),
            jump_and_cluck: i8::from_le_bytes(b[0x23..0x24].try_into().unwrap()),
            padding: b[0x24..0x27].try_into().unwrap(),
            gap_27: b[0x27..0x28].try_into().unwrap(),
        }
    }
}
impl Default for DodoUpdateNetData { fn default() -> Self { Self::read_le(&[0u8; 40]) } }
#[cfg(test)]
mod test_dodo_update_net_data {
    use super::*;
    #[test]
    fn layout() {
        assert_eq!(core::mem::size_of::<DodoUpdateNetData>(), 0x28);
        assert_eq!(<DodoUpdateNetData as WireStruct>::SIZE, 0x28);
        assert_eq!(core::mem::offset_of!(DodoUpdateNetData, npc_update_net_data), 0x0);
        assert_eq!(core::mem::offset_of!(DodoUpdateNetData, to_square_x), 0x18);
        assert_eq!(core::mem::offset_of!(DodoUpdateNetData, to_square_y), 0x1c);
        assert_eq!(core::mem::offset_of!(DodoUpdateNetData, walk_speed), 0x20);
        assert_eq!(core::mem::offset_of!(DodoUpdateNetData, movement_direction), 0x22);
        assert_eq!(core::mem::offset_of!(DodoUpdateNetData, jump_and_cluck), 0x23);
        assert_eq!(core::mem::offset_of!(DodoUpdateNetData, padding), 0x24);
    }
    #[test]
    fn roundtrip() {
        let mut raw = [0u8; 0x28];
        for (i, x) in raw.iter_mut().enumerate() { *x = (i as u8).wrapping_mul(37).wrapping_add(11); }
        let v = DodoUpdateNetData::read_le(&raw);
        let mut back = vec![0u8; raw.len()];
        v.write_le(&mut back);
        assert_eq!(&back[..], &raw[..]);   // byte-exact incl. padding (floats compared as bytes, NaN-safe)
        assert!(<DodoUpdateNetData>::default().to_vec().iter().all(|&x| x == 0));
    }
}

/// size 0x70
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct DodoCreationNetData {
    /// NPCCreationNetData @ 0x0
    pub npc_creation_net_data: NPCCreationNetData,
    /// DodoUpdateNetData @ 0x48
    pub dodo_update_net_data: DodoUpdateNetData,
}
impl WireStruct for DodoCreationNetData {
    const SIZE: usize = 0x70;
    fn write_le(&self, out: &mut [u8]) {
        assert!(out.len() >= Self::SIZE);
        out[..Self::SIZE].fill(0);
        self.npc_creation_net_data.write_le(&mut out[0x0..]);
        self.dodo_update_net_data.write_le(&mut out[0x48..]);
    }
    fn read_le(b: &[u8]) -> Self {
        assert!(b.len() >= Self::SIZE);
        Self {
            npc_creation_net_data: NPCCreationNetData::read_le(&b[0x0..]),
            dodo_update_net_data: DodoUpdateNetData::read_le(&b[0x48..]),
        }
    }
}
impl Default for DodoCreationNetData { fn default() -> Self { Self::read_le(&[0u8; 112]) } }
#[cfg(test)]
mod test_dodo_creation_net_data {
    use super::*;
    #[test]
    fn layout() {
        assert_eq!(core::mem::size_of::<DodoCreationNetData>(), 0x70);
        assert_eq!(<DodoCreationNetData as WireStruct>::SIZE, 0x70);
        assert_eq!(core::mem::offset_of!(DodoCreationNetData, npc_creation_net_data), 0x0);
        assert_eq!(core::mem::offset_of!(DodoCreationNetData, dodo_update_net_data), 0x48);
    }
    #[test]
    fn roundtrip() {
        let mut raw = [0u8; 0x70];
        for (i, x) in raw.iter_mut().enumerate() { *x = (i as u8).wrapping_mul(37).wrapping_add(11); }
        let v = DodoCreationNetData::read_le(&raw);
        let mut back = vec![0u8; raw.len()];
        v.write_le(&mut back);
        assert_eq!(&back[..], &raw[..]);   // byte-exact incl. padding (floats compared as bytes, NaN-safe)
        assert!(<DodoCreationNetData>::default().to_vec().iter().all(|&x| x == 0));
    }
}

/// size 0x20
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct DonkeyLikeUpdateNetData {
    /// NPCUpdateNetData @ 0x0
    pub npc_update_net_data: NPCUpdateNetData,
    /// int16_t @ 0x18
    pub target_x_speed: i16,
    /// int16_t @ 0x1a
    pub walk_speed: i16,
    /// int16_t @ 0x1c
    pub random_goal_rotation: i16,
    /// uint8_t @ 0x1e
    pub jump_action_send_value: u8,
    /// explicit padding at 0x1f; preserved on read, zero by default
    pub padding: [u8; 1],
}
impl WireStruct for DonkeyLikeUpdateNetData {
    const SIZE: usize = 0x20;
    fn write_le(&self, out: &mut [u8]) {
        assert!(out.len() >= Self::SIZE);
        out[..Self::SIZE].fill(0);
        self.npc_update_net_data.write_le(&mut out[0x0..]);
        out[0x18..0x1a].copy_from_slice(&self.target_x_speed.to_le_bytes());
        out[0x1a..0x1c].copy_from_slice(&self.walk_speed.to_le_bytes());
        out[0x1c..0x1e].copy_from_slice(&self.random_goal_rotation.to_le_bytes());
        out[0x1e..0x1f].copy_from_slice(&self.jump_action_send_value.to_le_bytes());
        out[0x1f..0x20].copy_from_slice(&self.padding);
    }
    fn read_le(b: &[u8]) -> Self {
        assert!(b.len() >= Self::SIZE);
        Self {
            npc_update_net_data: NPCUpdateNetData::read_le(&b[0x0..]),
            target_x_speed: i16::from_le_bytes(b[0x18..0x1a].try_into().unwrap()),
            walk_speed: i16::from_le_bytes(b[0x1a..0x1c].try_into().unwrap()),
            random_goal_rotation: i16::from_le_bytes(b[0x1c..0x1e].try_into().unwrap()),
            jump_action_send_value: u8::from_le_bytes(b[0x1e..0x1f].try_into().unwrap()),
            padding: b[0x1f..0x20].try_into().unwrap(),
        }
    }
}
impl Default for DonkeyLikeUpdateNetData { fn default() -> Self { Self::read_le(&[0u8; 32]) } }
#[cfg(test)]
mod test_donkey_like_update_net_data {
    use super::*;
    #[test]
    fn layout() {
        assert_eq!(core::mem::size_of::<DonkeyLikeUpdateNetData>(), 0x20);
        assert_eq!(<DonkeyLikeUpdateNetData as WireStruct>::SIZE, 0x20);
        assert_eq!(core::mem::offset_of!(DonkeyLikeUpdateNetData, npc_update_net_data), 0x0);
        assert_eq!(core::mem::offset_of!(DonkeyLikeUpdateNetData, target_x_speed), 0x18);
        assert_eq!(core::mem::offset_of!(DonkeyLikeUpdateNetData, walk_speed), 0x1a);
        assert_eq!(core::mem::offset_of!(DonkeyLikeUpdateNetData, random_goal_rotation), 0x1c);
        assert_eq!(core::mem::offset_of!(DonkeyLikeUpdateNetData, jump_action_send_value), 0x1e);
        assert_eq!(core::mem::offset_of!(DonkeyLikeUpdateNetData, padding), 0x1f);
    }
    #[test]
    fn roundtrip() {
        let mut raw = [0u8; 0x20];
        for (i, x) in raw.iter_mut().enumerate() { *x = (i as u8).wrapping_mul(37).wrapping_add(11); }
        let v = DonkeyLikeUpdateNetData::read_le(&raw);
        let mut back = vec![0u8; raw.len()];
        v.write_le(&mut back);
        assert_eq!(&back[..], &raw[..]);   // byte-exact incl. padding (floats compared as bytes, NaN-safe)
        assert!(<DonkeyLikeUpdateNetData>::default().to_vec().iter().all(|&x| x == 0));
    }
}

/// size 0x68
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct DonkeyLikeCreationNetData {
    /// NPCCreationNetData @ 0x0
    pub npc_creation_net_data: NPCCreationNetData,
    /// DonkeyLikeUpdateNetData @ 0x48
    pub donkey_like_update_net_data: DonkeyLikeUpdateNetData,
}
impl WireStruct for DonkeyLikeCreationNetData {
    const SIZE: usize = 0x68;
    fn write_le(&self, out: &mut [u8]) {
        assert!(out.len() >= Self::SIZE);
        out[..Self::SIZE].fill(0);
        self.npc_creation_net_data.write_le(&mut out[0x0..]);
        self.donkey_like_update_net_data.write_le(&mut out[0x48..]);
    }
    fn read_le(b: &[u8]) -> Self {
        assert!(b.len() >= Self::SIZE);
        Self {
            npc_creation_net_data: NPCCreationNetData::read_le(&b[0x0..]),
            donkey_like_update_net_data: DonkeyLikeUpdateNetData::read_le(&b[0x48..]),
        }
    }
}
impl Default for DonkeyLikeCreationNetData { fn default() -> Self { Self::read_le(&[0u8; 104]) } }
#[cfg(test)]
mod test_donkey_like_creation_net_data {
    use super::*;
    #[test]
    fn layout() {
        assert_eq!(core::mem::size_of::<DonkeyLikeCreationNetData>(), 0x68);
        assert_eq!(<DonkeyLikeCreationNetData as WireStruct>::SIZE, 0x68);
        assert_eq!(core::mem::offset_of!(DonkeyLikeCreationNetData, npc_creation_net_data), 0x0);
        assert_eq!(core::mem::offset_of!(DonkeyLikeCreationNetData, donkey_like_update_net_data), 0x48);
    }
    #[test]
    fn roundtrip() {
        let mut raw = [0u8; 0x68];
        for (i, x) in raw.iter_mut().enumerate() { *x = (i as u8).wrapping_mul(37).wrapping_add(11); }
        let v = DonkeyLikeCreationNetData::read_le(&raw);
        let mut back = vec![0u8; raw.len()];
        v.write_le(&mut back);
        assert_eq!(&back[..], &raw[..]);   // byte-exact incl. padding (floats compared as bytes, NaN-safe)
        assert!(<DonkeyLikeCreationNetData>::default().to_vec().iter().all(|&x| x == 0));
    }
}

/// size 0x20
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct DoorCreationNetData {
    /// DynamicObjectNetData @ 0x0
    pub dynamic_object_net_data: DynamicObjectNetData,
    /// uint16_t @ 0x18
    pub item_type: u16,
    /// uint8_t @ 0x1a
    pub blocked: u8,
    /// explicit padding at 0x1b; preserved on read, zero by default
    pub padding: [u8; 5],
}
impl WireStruct for DoorCreationNetData {
    const SIZE: usize = 0x20;
    fn write_le(&self, out: &mut [u8]) {
        assert!(out.len() >= Self::SIZE);
        out[..Self::SIZE].fill(0);
        self.dynamic_object_net_data.write_le(&mut out[0x0..]);
        out[0x18..0x1a].copy_from_slice(&self.item_type.to_le_bytes());
        out[0x1a..0x1b].copy_from_slice(&self.blocked.to_le_bytes());
        out[0x1b..0x20].copy_from_slice(&self.padding);
    }
    fn read_le(b: &[u8]) -> Self {
        assert!(b.len() >= Self::SIZE);
        Self {
            dynamic_object_net_data: DynamicObjectNetData::read_le(&b[0x0..]),
            item_type: u16::from_le_bytes(b[0x18..0x1a].try_into().unwrap()),
            blocked: u8::from_le_bytes(b[0x1a..0x1b].try_into().unwrap()),
            padding: b[0x1b..0x20].try_into().unwrap(),
        }
    }
}
impl Default for DoorCreationNetData { fn default() -> Self { Self::read_le(&[0u8; 32]) } }
#[cfg(test)]
mod test_door_creation_net_data {
    use super::*;
    #[test]
    fn layout() {
        assert_eq!(core::mem::size_of::<DoorCreationNetData>(), 0x20);
        assert_eq!(<DoorCreationNetData as WireStruct>::SIZE, 0x20);
        assert_eq!(core::mem::offset_of!(DoorCreationNetData, dynamic_object_net_data), 0x0);
        assert_eq!(core::mem::offset_of!(DoorCreationNetData, item_type), 0x18);
        assert_eq!(core::mem::offset_of!(DoorCreationNetData, blocked), 0x1a);
        assert_eq!(core::mem::offset_of!(DoorCreationNetData, padding), 0x1b);
    }
    #[test]
    fn roundtrip() {
        let mut raw = [0u8; 0x20];
        for (i, x) in raw.iter_mut().enumerate() { *x = (i as u8).wrapping_mul(37).wrapping_add(11); }
        let v = DoorCreationNetData::read_le(&raw);
        let mut back = vec![0u8; raw.len()];
        v.write_le(&mut back);
        assert_eq!(&back[..], &raw[..]);   // byte-exact incl. padding (floats compared as bytes, NaN-safe)
        assert!(<DoorCreationNetData>::default().to_vec().iter().all(|&x| x == 0));
    }
}

/// size 0x28
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct DropBearUpdateNetData {
    /// NPCUpdateNetData @ 0x0
    pub npc_update_net_data: NPCUpdateNetData,
    /// uint32_t @ 0x18
    pub to_square_x: u32,
    /// uint32_t @ 0x1c
    pub to_square_y: u32,
    /// int16_t @ 0x20
    pub walk_speed: i16,
    /// uint8_t @ 0x22
    pub dropping: u8,
    /// uint8_t @ 0x23
    pub on_ground: u8,
    /// explicit padding at 0x24; preserved on read, zero by default
    pub padding: [u8; 4],
}
impl WireStruct for DropBearUpdateNetData {
    const SIZE: usize = 0x28;
    fn write_le(&self, out: &mut [u8]) {
        assert!(out.len() >= Self::SIZE);
        out[..Self::SIZE].fill(0);
        self.npc_update_net_data.write_le(&mut out[0x0..]);
        out[0x18..0x1c].copy_from_slice(&self.to_square_x.to_le_bytes());
        out[0x1c..0x20].copy_from_slice(&self.to_square_y.to_le_bytes());
        out[0x20..0x22].copy_from_slice(&self.walk_speed.to_le_bytes());
        out[0x22..0x23].copy_from_slice(&self.dropping.to_le_bytes());
        out[0x23..0x24].copy_from_slice(&self.on_ground.to_le_bytes());
        out[0x24..0x28].copy_from_slice(&self.padding);
    }
    fn read_le(b: &[u8]) -> Self {
        assert!(b.len() >= Self::SIZE);
        Self {
            npc_update_net_data: NPCUpdateNetData::read_le(&b[0x0..]),
            to_square_x: u32::from_le_bytes(b[0x18..0x1c].try_into().unwrap()),
            to_square_y: u32::from_le_bytes(b[0x1c..0x20].try_into().unwrap()),
            walk_speed: i16::from_le_bytes(b[0x20..0x22].try_into().unwrap()),
            dropping: u8::from_le_bytes(b[0x22..0x23].try_into().unwrap()),
            on_ground: u8::from_le_bytes(b[0x23..0x24].try_into().unwrap()),
            padding: b[0x24..0x28].try_into().unwrap(),
        }
    }
}
impl Default for DropBearUpdateNetData { fn default() -> Self { Self::read_le(&[0u8; 40]) } }
#[cfg(test)]
mod test_drop_bear_update_net_data {
    use super::*;
    #[test]
    fn layout() {
        assert_eq!(core::mem::size_of::<DropBearUpdateNetData>(), 0x28);
        assert_eq!(<DropBearUpdateNetData as WireStruct>::SIZE, 0x28);
        assert_eq!(core::mem::offset_of!(DropBearUpdateNetData, npc_update_net_data), 0x0);
        assert_eq!(core::mem::offset_of!(DropBearUpdateNetData, to_square_x), 0x18);
        assert_eq!(core::mem::offset_of!(DropBearUpdateNetData, to_square_y), 0x1c);
        assert_eq!(core::mem::offset_of!(DropBearUpdateNetData, walk_speed), 0x20);
        assert_eq!(core::mem::offset_of!(DropBearUpdateNetData, dropping), 0x22);
        assert_eq!(core::mem::offset_of!(DropBearUpdateNetData, on_ground), 0x23);
        assert_eq!(core::mem::offset_of!(DropBearUpdateNetData, padding), 0x24);
    }
    #[test]
    fn roundtrip() {
        let mut raw = [0u8; 0x28];
        for (i, x) in raw.iter_mut().enumerate() { *x = (i as u8).wrapping_mul(37).wrapping_add(11); }
        let v = DropBearUpdateNetData::read_le(&raw);
        let mut back = vec![0u8; raw.len()];
        v.write_le(&mut back);
        assert_eq!(&back[..], &raw[..]);   // byte-exact incl. padding (floats compared as bytes, NaN-safe)
        assert!(<DropBearUpdateNetData>::default().to_vec().iter().all(|&x| x == 0));
    }
}

/// size 0x78
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct DropBearCreationNetData {
    /// NPCCreationNetData @ 0x0
    pub npc_creation_net_data: NPCCreationNetData,
    /// DropBearUpdateNetData @ 0x48
    pub drop_bear_update_net_data: DropBearUpdateNetData,
    /// uint64_t @ 0x70
    pub unused: u64,
}
impl WireStruct for DropBearCreationNetData {
    const SIZE: usize = 0x78;
    fn write_le(&self, out: &mut [u8]) {
        assert!(out.len() >= Self::SIZE);
        out[..Self::SIZE].fill(0);
        self.npc_creation_net_data.write_le(&mut out[0x0..]);
        self.drop_bear_update_net_data.write_le(&mut out[0x48..]);
        out[0x70..0x78].copy_from_slice(&self.unused.to_le_bytes());
    }
    fn read_le(b: &[u8]) -> Self {
        assert!(b.len() >= Self::SIZE);
        Self {
            npc_creation_net_data: NPCCreationNetData::read_le(&b[0x0..]),
            drop_bear_update_net_data: DropBearUpdateNetData::read_le(&b[0x48..]),
            unused: u64::from_le_bytes(b[0x70..0x78].try_into().unwrap()),
        }
    }
}
impl Default for DropBearCreationNetData { fn default() -> Self { Self::read_le(&[0u8; 120]) } }
#[cfg(test)]
mod test_drop_bear_creation_net_data {
    use super::*;
    #[test]
    fn layout() {
        assert_eq!(core::mem::size_of::<DropBearCreationNetData>(), 0x78);
        assert_eq!(<DropBearCreationNetData as WireStruct>::SIZE, 0x78);
        assert_eq!(core::mem::offset_of!(DropBearCreationNetData, npc_creation_net_data), 0x0);
        assert_eq!(core::mem::offset_of!(DropBearCreationNetData, drop_bear_update_net_data), 0x48);
        assert_eq!(core::mem::offset_of!(DropBearCreationNetData, unused), 0x70);
    }
    #[test]
    fn roundtrip() {
        let mut raw = [0u8; 0x78];
        for (i, x) in raw.iter_mut().enumerate() { *x = (i as u8).wrapping_mul(37).wrapping_add(11); }
        let v = DropBearCreationNetData::read_le(&raw);
        let mut back = vec![0u8; raw.len()];
        v.write_le(&mut back);
        assert_eq!(&back[..], &raw[..]);   // byte-exact incl. padding (floats compared as bytes, NaN-safe)
        assert!(<DropBearCreationNetData>::default().to_vec().iter().all(|&x| x == 0));
    }
}

/// size 0x18
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct DynamicObjectNetData {
    /// uint64_t @ 0x0
    pub unique_id: u64,
    /// uint32_t @ 0x8
    pub pos_x: u32,
    /// uint32_t @ 0xc
    pub pos_y: u32,
    /// uint8_t @ 0x10
    pub needs_removed: u8,
    /// explicit padding at 0x11; preserved on read, zero by default
    pub padding: [u8; 7],
}
impl WireStruct for DynamicObjectNetData {
    const SIZE: usize = 0x18;
    fn write_le(&self, out: &mut [u8]) {
        assert!(out.len() >= Self::SIZE);
        out[..Self::SIZE].fill(0);
        out[0x0..0x8].copy_from_slice(&self.unique_id.to_le_bytes());
        out[0x8..0xc].copy_from_slice(&self.pos_x.to_le_bytes());
        out[0xc..0x10].copy_from_slice(&self.pos_y.to_le_bytes());
        out[0x10..0x11].copy_from_slice(&self.needs_removed.to_le_bytes());
        out[0x11..0x18].copy_from_slice(&self.padding);
    }
    fn read_le(b: &[u8]) -> Self {
        assert!(b.len() >= Self::SIZE);
        Self {
            unique_id: u64::from_le_bytes(b[0x0..0x8].try_into().unwrap()),
            pos_x: u32::from_le_bytes(b[0x8..0xc].try_into().unwrap()),
            pos_y: u32::from_le_bytes(b[0xc..0x10].try_into().unwrap()),
            needs_removed: u8::from_le_bytes(b[0x10..0x11].try_into().unwrap()),
            padding: b[0x11..0x18].try_into().unwrap(),
        }
    }
}
impl Default for DynamicObjectNetData { fn default() -> Self { Self::read_le(&[0u8; 24]) } }
#[cfg(test)]
mod test_dynamic_object_net_data {
    use super::*;
    #[test]
    fn layout() {
        assert_eq!(core::mem::size_of::<DynamicObjectNetData>(), 0x18);
        assert_eq!(<DynamicObjectNetData as WireStruct>::SIZE, 0x18);
        assert_eq!(core::mem::offset_of!(DynamicObjectNetData, unique_id), 0x0);
        assert_eq!(core::mem::offset_of!(DynamicObjectNetData, pos_x), 0x8);
        assert_eq!(core::mem::offset_of!(DynamicObjectNetData, pos_y), 0xc);
        assert_eq!(core::mem::offset_of!(DynamicObjectNetData, needs_removed), 0x10);
        assert_eq!(core::mem::offset_of!(DynamicObjectNetData, padding), 0x11);
    }
    #[test]
    fn roundtrip() {
        let mut raw = [0u8; 0x18];
        for (i, x) in raw.iter_mut().enumerate() { *x = (i as u8).wrapping_mul(37).wrapping_add(11); }
        let v = DynamicObjectNetData::read_le(&raw);
        let mut back = vec![0u8; raw.len()];
        v.write_le(&mut back);
        assert_eq!(&back[..], &raw[..]);   // byte-exact incl. padding (floats compared as bytes, NaN-safe)
        assert!(<DynamicObjectNetData>::default().to_vec().iter().all(|&x| x == 0));
    }
}

/// size 0x20
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct EggCreationNetData {
    /// DynamicObjectNetData @ 0x0
    pub dynamic_object_net_data: DynamicObjectNetData,
    /// uint16_t @ 0x18
    pub breed: u16,
    /// uint16_t @ 0x1a
    pub hatch_timer: u16,
    /// explicit padding at 0x1c; preserved on read, zero by default
    pub padding: [u8; 4],
}
impl WireStruct for EggCreationNetData {
    const SIZE: usize = 0x20;
    fn write_le(&self, out: &mut [u8]) {
        assert!(out.len() >= Self::SIZE);
        out[..Self::SIZE].fill(0);
        self.dynamic_object_net_data.write_le(&mut out[0x0..]);
        out[0x18..0x1a].copy_from_slice(&self.breed.to_le_bytes());
        out[0x1a..0x1c].copy_from_slice(&self.hatch_timer.to_le_bytes());
        out[0x1c..0x20].copy_from_slice(&self.padding);
    }
    fn read_le(b: &[u8]) -> Self {
        assert!(b.len() >= Self::SIZE);
        Self {
            dynamic_object_net_data: DynamicObjectNetData::read_le(&b[0x0..]),
            breed: u16::from_le_bytes(b[0x18..0x1a].try_into().unwrap()),
            hatch_timer: u16::from_le_bytes(b[0x1a..0x1c].try_into().unwrap()),
            padding: b[0x1c..0x20].try_into().unwrap(),
        }
    }
}
impl Default for EggCreationNetData { fn default() -> Self { Self::read_le(&[0u8; 32]) } }
#[cfg(test)]
mod test_egg_creation_net_data {
    use super::*;
    #[test]
    fn layout() {
        assert_eq!(core::mem::size_of::<EggCreationNetData>(), 0x20);
        assert_eq!(<EggCreationNetData as WireStruct>::SIZE, 0x20);
        assert_eq!(core::mem::offset_of!(EggCreationNetData, dynamic_object_net_data), 0x0);
        assert_eq!(core::mem::offset_of!(EggCreationNetData, breed), 0x18);
        assert_eq!(core::mem::offset_of!(EggCreationNetData, hatch_timer), 0x1a);
        assert_eq!(core::mem::offset_of!(EggCreationNetData, padding), 0x1c);
    }
    #[test]
    fn roundtrip() {
        let mut raw = [0u8; 0x20];
        for (i, x) in raw.iter_mut().enumerate() { *x = (i as u8).wrapping_mul(37).wrapping_add(11); }
        let v = EggCreationNetData::read_le(&raw);
        let mut back = vec![0u8; raw.len()];
        v.write_le(&mut back);
        assert_eq!(&back[..], &raw[..]);   // byte-exact incl. padding (floats compared as bytes, NaN-safe)
        assert!(<EggCreationNetData>::default().to_vec().iter().all(|&x| x == 0));
    }
}

/// size 0x28
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ElevatorMotorCreationNetData {
    /// DynamicObjectNetData @ 0x0
    pub dynamic_object_net_data: DynamicObjectNetData,
    /// uint16_t @ 0x18
    pub item_type: u16,
    /// uint16_t @ 0x1a
    pub available_electricity: u16,
    /// uint16_t @ 0x1c
    pub min_y: u16,
    /// uint16_t @ 0x1e
    pub max_y: u16,
    /// uint16_t @ 0x20
    pub client_power_usage: u16,
    /// explicit padding at 0x22; preserved on read, zero by default
    pub padding: [u8; 6],
}
impl WireStruct for ElevatorMotorCreationNetData {
    const SIZE: usize = 0x28;
    fn write_le(&self, out: &mut [u8]) {
        assert!(out.len() >= Self::SIZE);
        out[..Self::SIZE].fill(0);
        self.dynamic_object_net_data.write_le(&mut out[0x0..]);
        out[0x18..0x1a].copy_from_slice(&self.item_type.to_le_bytes());
        out[0x1a..0x1c].copy_from_slice(&self.available_electricity.to_le_bytes());
        out[0x1c..0x1e].copy_from_slice(&self.min_y.to_le_bytes());
        out[0x1e..0x20].copy_from_slice(&self.max_y.to_le_bytes());
        out[0x20..0x22].copy_from_slice(&self.client_power_usage.to_le_bytes());
        out[0x22..0x28].copy_from_slice(&self.padding);
    }
    fn read_le(b: &[u8]) -> Self {
        assert!(b.len() >= Self::SIZE);
        Self {
            dynamic_object_net_data: DynamicObjectNetData::read_le(&b[0x0..]),
            item_type: u16::from_le_bytes(b[0x18..0x1a].try_into().unwrap()),
            available_electricity: u16::from_le_bytes(b[0x1a..0x1c].try_into().unwrap()),
            min_y: u16::from_le_bytes(b[0x1c..0x1e].try_into().unwrap()),
            max_y: u16::from_le_bytes(b[0x1e..0x20].try_into().unwrap()),
            client_power_usage: u16::from_le_bytes(b[0x20..0x22].try_into().unwrap()),
            padding: b[0x22..0x28].try_into().unwrap(),
        }
    }
}
impl Default for ElevatorMotorCreationNetData { fn default() -> Self { Self::read_le(&[0u8; 40]) } }
#[cfg(test)]
mod test_elevator_motor_creation_net_data {
    use super::*;
    #[test]
    fn layout() {
        assert_eq!(core::mem::size_of::<ElevatorMotorCreationNetData>(), 0x28);
        assert_eq!(<ElevatorMotorCreationNetData as WireStruct>::SIZE, 0x28);
        assert_eq!(core::mem::offset_of!(ElevatorMotorCreationNetData, dynamic_object_net_data), 0x0);
        assert_eq!(core::mem::offset_of!(ElevatorMotorCreationNetData, item_type), 0x18);
        assert_eq!(core::mem::offset_of!(ElevatorMotorCreationNetData, available_electricity), 0x1a);
        assert_eq!(core::mem::offset_of!(ElevatorMotorCreationNetData, min_y), 0x1c);
        assert_eq!(core::mem::offset_of!(ElevatorMotorCreationNetData, max_y), 0x1e);
        assert_eq!(core::mem::offset_of!(ElevatorMotorCreationNetData, client_power_usage), 0x20);
        assert_eq!(core::mem::offset_of!(ElevatorMotorCreationNetData, padding), 0x22);
    }
    #[test]
    fn roundtrip() {
        let mut raw = [0u8; 0x28];
        for (i, x) in raw.iter_mut().enumerate() { *x = (i as u8).wrapping_mul(37).wrapping_add(11); }
        let v = ElevatorMotorCreationNetData::read_le(&raw);
        let mut back = vec![0u8; raw.len()];
        v.write_le(&mut back);
        assert_eq!(&back[..], &raw[..]);   // byte-exact incl. padding (floats compared as bytes, NaN-safe)
        assert!(<ElevatorMotorCreationNetData>::default().to_vec().iter().all(|&x| x == 0));
    }
}

/// size 0x28
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ElevatorShaftCreationNetData {
    /// DynamicObjectNetData @ 0x0
    pub dynamic_object_net_data: DynamicObjectNetData,
    /// uint32_t @ 0x18
    pub last_known_motor_pos_x: u32,
    /// uint32_t @ 0x1c
    pub last_known_motor_pos_y: u32,
    /// uint16_t @ 0x20
    pub item_type: u16,
    /// uint16_t @ 0x22
    pub paint_color: u16,
    /// uint8_t @ 0x24
    pub solid_tile: u8,
    /// explicit padding at 0x25; preserved on read, zero by default
    pub padding: [u8; 3],
}
impl WireStruct for ElevatorShaftCreationNetData {
    const SIZE: usize = 0x28;
    fn write_le(&self, out: &mut [u8]) {
        assert!(out.len() >= Self::SIZE);
        out[..Self::SIZE].fill(0);
        self.dynamic_object_net_data.write_le(&mut out[0x0..]);
        out[0x18..0x1c].copy_from_slice(&self.last_known_motor_pos_x.to_le_bytes());
        out[0x1c..0x20].copy_from_slice(&self.last_known_motor_pos_y.to_le_bytes());
        out[0x20..0x22].copy_from_slice(&self.item_type.to_le_bytes());
        out[0x22..0x24].copy_from_slice(&self.paint_color.to_le_bytes());
        out[0x24..0x25].copy_from_slice(&self.solid_tile.to_le_bytes());
        out[0x25..0x28].copy_from_slice(&self.padding);
    }
    fn read_le(b: &[u8]) -> Self {
        assert!(b.len() >= Self::SIZE);
        Self {
            dynamic_object_net_data: DynamicObjectNetData::read_le(&b[0x0..]),
            last_known_motor_pos_x: u32::from_le_bytes(b[0x18..0x1c].try_into().unwrap()),
            last_known_motor_pos_y: u32::from_le_bytes(b[0x1c..0x20].try_into().unwrap()),
            item_type: u16::from_le_bytes(b[0x20..0x22].try_into().unwrap()),
            paint_color: u16::from_le_bytes(b[0x22..0x24].try_into().unwrap()),
            solid_tile: u8::from_le_bytes(b[0x24..0x25].try_into().unwrap()),
            padding: b[0x25..0x28].try_into().unwrap(),
        }
    }
}
impl Default for ElevatorShaftCreationNetData { fn default() -> Self { Self::read_le(&[0u8; 40]) } }
#[cfg(test)]
mod test_elevator_shaft_creation_net_data {
    use super::*;
    #[test]
    fn layout() {
        assert_eq!(core::mem::size_of::<ElevatorShaftCreationNetData>(), 0x28);
        assert_eq!(<ElevatorShaftCreationNetData as WireStruct>::SIZE, 0x28);
        assert_eq!(core::mem::offset_of!(ElevatorShaftCreationNetData, dynamic_object_net_data), 0x0);
        assert_eq!(core::mem::offset_of!(ElevatorShaftCreationNetData, last_known_motor_pos_x), 0x18);
        assert_eq!(core::mem::offset_of!(ElevatorShaftCreationNetData, last_known_motor_pos_y), 0x1c);
        assert_eq!(core::mem::offset_of!(ElevatorShaftCreationNetData, item_type), 0x20);
        assert_eq!(core::mem::offset_of!(ElevatorShaftCreationNetData, paint_color), 0x22);
        assert_eq!(core::mem::offset_of!(ElevatorShaftCreationNetData, solid_tile), 0x24);
        assert_eq!(core::mem::offset_of!(ElevatorShaftCreationNetData, padding), 0x25);
    }
    #[test]
    fn roundtrip() {
        let mut raw = [0u8; 0x28];
        for (i, x) in raw.iter_mut().enumerate() { *x = (i as u8).wrapping_mul(37).wrapping_add(11); }
        let v = ElevatorShaftCreationNetData::read_le(&raw);
        let mut back = vec![0u8; raw.len()];
        v.write_le(&mut back);
        assert_eq!(&back[..], &raw[..]);   // byte-exact incl. padding (floats compared as bytes, NaN-safe)
        assert!(<ElevatorShaftCreationNetData>::default().to_vec().iter().all(|&x| x == 0));
    }
}

/// size 0x18
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FireCreationNetData {
    /// DynamicObjectNetData @ 0x0
    pub dynamic_object_net_data: DynamicObjectNetData,
}
impl WireStruct for FireCreationNetData {
    const SIZE: usize = 0x18;
    fn write_le(&self, out: &mut [u8]) {
        assert!(out.len() >= Self::SIZE);
        out[..Self::SIZE].fill(0);
        self.dynamic_object_net_data.write_le(&mut out[0x0..]);
    }
    fn read_le(b: &[u8]) -> Self {
        assert!(b.len() >= Self::SIZE);
        Self {
            dynamic_object_net_data: DynamicObjectNetData::read_le(&b[0x0..]),
        }
    }
}
impl Default for FireCreationNetData { fn default() -> Self { Self::read_le(&[0u8; 24]) } }
#[cfg(test)]
mod test_fire_creation_net_data {
    use super::*;
    #[test]
    fn layout() {
        assert_eq!(core::mem::size_of::<FireCreationNetData>(), 0x18);
        assert_eq!(<FireCreationNetData as WireStruct>::SIZE, 0x18);
        assert_eq!(core::mem::offset_of!(FireCreationNetData, dynamic_object_net_data), 0x0);
    }
    #[test]
    fn roundtrip() {
        let mut raw = [0u8; 0x18];
        for (i, x) in raw.iter_mut().enumerate() { *x = (i as u8).wrapping_mul(37).wrapping_add(11); }
        let v = FireCreationNetData::read_le(&raw);
        let mut back = vec![0u8; raw.len()];
        v.write_le(&mut back);
        assert_eq!(&back[..], &raw[..]);   // byte-exact incl. padding (floats compared as bytes, NaN-safe)
        assert!(<FireCreationNetData>::default().to_vec().iter().all(|&x| x == 0));
    }
}

/// size 0x38
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FreeblockCreationNetData {
    /// DynamicObjectNetData @ 0x0
    pub dynamic_object_net_data: DynamicObjectNetData,
    /// uint64_t @ 0x18
    pub priority_blockhead_uinque_id: u64,
    /// uint16_t @ 0x20
    pub item_type: u16,
    /// uint16_t @ 0x22
    pub data_a: u16,
    /// uint16_t @ 0x24
    pub data_b: u16,
    /// int16_t @ 0x26
    pub fall_speed: i16,
    /// int16_t @ 0x28
    pub x_velocity: i16,
    /// int16_t @ 0x2a
    pub y_velocity: i16,
    /// Float32 @ 0x2c
    pub creation_sound_play_time: f32,
    /// uint8_t @ 0x30
    pub sound_type: u8,
    /// uint8_t @ 0x31
    pub hovers: u8,
    /// uint8_t @ 0x32
    pub has_extra_data: u8,
    /// explicit padding at 0x33; preserved on read, zero by default
    pub padding: [u8; 5],
}
impl WireStruct for FreeblockCreationNetData {
    const SIZE: usize = 0x38;
    fn write_le(&self, out: &mut [u8]) {
        assert!(out.len() >= Self::SIZE);
        out[..Self::SIZE].fill(0);
        self.dynamic_object_net_data.write_le(&mut out[0x0..]);
        out[0x18..0x20].copy_from_slice(&self.priority_blockhead_uinque_id.to_le_bytes());
        out[0x20..0x22].copy_from_slice(&self.item_type.to_le_bytes());
        out[0x22..0x24].copy_from_slice(&self.data_a.to_le_bytes());
        out[0x24..0x26].copy_from_slice(&self.data_b.to_le_bytes());
        out[0x26..0x28].copy_from_slice(&self.fall_speed.to_le_bytes());
        out[0x28..0x2a].copy_from_slice(&self.x_velocity.to_le_bytes());
        out[0x2a..0x2c].copy_from_slice(&self.y_velocity.to_le_bytes());
        out[0x2c..0x30].copy_from_slice(&self.creation_sound_play_time.to_le_bytes());
        out[0x30..0x31].copy_from_slice(&self.sound_type.to_le_bytes());
        out[0x31..0x32].copy_from_slice(&self.hovers.to_le_bytes());
        out[0x32..0x33].copy_from_slice(&self.has_extra_data.to_le_bytes());
        out[0x33..0x38].copy_from_slice(&self.padding);
    }
    fn read_le(b: &[u8]) -> Self {
        assert!(b.len() >= Self::SIZE);
        Self {
            dynamic_object_net_data: DynamicObjectNetData::read_le(&b[0x0..]),
            priority_blockhead_uinque_id: u64::from_le_bytes(b[0x18..0x20].try_into().unwrap()),
            item_type: u16::from_le_bytes(b[0x20..0x22].try_into().unwrap()),
            data_a: u16::from_le_bytes(b[0x22..0x24].try_into().unwrap()),
            data_b: u16::from_le_bytes(b[0x24..0x26].try_into().unwrap()),
            fall_speed: i16::from_le_bytes(b[0x26..0x28].try_into().unwrap()),
            x_velocity: i16::from_le_bytes(b[0x28..0x2a].try_into().unwrap()),
            y_velocity: i16::from_le_bytes(b[0x2a..0x2c].try_into().unwrap()),
            creation_sound_play_time: f32::from_le_bytes(b[0x2c..0x30].try_into().unwrap()),
            sound_type: u8::from_le_bytes(b[0x30..0x31].try_into().unwrap()),
            hovers: u8::from_le_bytes(b[0x31..0x32].try_into().unwrap()),
            has_extra_data: u8::from_le_bytes(b[0x32..0x33].try_into().unwrap()),
            padding: b[0x33..0x38].try_into().unwrap(),
        }
    }
}
impl Default for FreeblockCreationNetData { fn default() -> Self { Self::read_le(&[0u8; 56]) } }
#[cfg(test)]
mod test_freeblock_creation_net_data {
    use super::*;
    #[test]
    fn layout() {
        assert_eq!(core::mem::size_of::<FreeblockCreationNetData>(), 0x38);
        assert_eq!(<FreeblockCreationNetData as WireStruct>::SIZE, 0x38);
        assert_eq!(core::mem::offset_of!(FreeblockCreationNetData, dynamic_object_net_data), 0x0);
        assert_eq!(core::mem::offset_of!(FreeblockCreationNetData, priority_blockhead_uinque_id), 0x18);
        assert_eq!(core::mem::offset_of!(FreeblockCreationNetData, item_type), 0x20);
        assert_eq!(core::mem::offset_of!(FreeblockCreationNetData, data_a), 0x22);
        assert_eq!(core::mem::offset_of!(FreeblockCreationNetData, data_b), 0x24);
        assert_eq!(core::mem::offset_of!(FreeblockCreationNetData, fall_speed), 0x26);
        assert_eq!(core::mem::offset_of!(FreeblockCreationNetData, x_velocity), 0x28);
        assert_eq!(core::mem::offset_of!(FreeblockCreationNetData, y_velocity), 0x2a);
        assert_eq!(core::mem::offset_of!(FreeblockCreationNetData, creation_sound_play_time), 0x2c);
        assert_eq!(core::mem::offset_of!(FreeblockCreationNetData, sound_type), 0x30);
        assert_eq!(core::mem::offset_of!(FreeblockCreationNetData, hovers), 0x31);
        assert_eq!(core::mem::offset_of!(FreeblockCreationNetData, has_extra_data), 0x32);
        assert_eq!(core::mem::offset_of!(FreeblockCreationNetData, padding), 0x33);
    }
    #[test]
    fn roundtrip() {
        let mut raw = [0u8; 0x38];
        for (i, x) in raw.iter_mut().enumerate() { *x = (i as u8).wrapping_mul(37).wrapping_add(11); }
        let v = FreeblockCreationNetData::read_le(&raw);
        let mut back = vec![0u8; raw.len()];
        v.write_le(&mut back);
        assert_eq!(&back[..], &raw[..]);   // byte-exact incl. padding (floats compared as bytes, NaN-safe)
        assert!(<FreeblockCreationNetData>::default().to_vec().iter().all(|&x| x == 0));
    }
}

/// size 0x28
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FreeblockUpdateNetData {
    /// DynamicObjectNetData @ 0x0
    pub dynamic_object_net_data: DynamicObjectNetData,
    /// uint64_t @ 0x18
    pub priority_blockhead_uinque_id: u64,
    /// int16_t @ 0x20
    pub fall_speed: i16,
    /// int16_t @ 0x22
    pub x_velocity: i16,
    /// int16_t @ 0x24
    pub y_velocity: i16,
    /// uint8_t @ 0x26
    pub hovers: u8,
    /// explicit padding at 0x27; preserved on read, zero by default
    pub padding: [u8; 1],
}
impl WireStruct for FreeblockUpdateNetData {
    const SIZE: usize = 0x28;
    fn write_le(&self, out: &mut [u8]) {
        assert!(out.len() >= Self::SIZE);
        out[..Self::SIZE].fill(0);
        self.dynamic_object_net_data.write_le(&mut out[0x0..]);
        out[0x18..0x20].copy_from_slice(&self.priority_blockhead_uinque_id.to_le_bytes());
        out[0x20..0x22].copy_from_slice(&self.fall_speed.to_le_bytes());
        out[0x22..0x24].copy_from_slice(&self.x_velocity.to_le_bytes());
        out[0x24..0x26].copy_from_slice(&self.y_velocity.to_le_bytes());
        out[0x26..0x27].copy_from_slice(&self.hovers.to_le_bytes());
        out[0x27..0x28].copy_from_slice(&self.padding);
    }
    fn read_le(b: &[u8]) -> Self {
        assert!(b.len() >= Self::SIZE);
        Self {
            dynamic_object_net_data: DynamicObjectNetData::read_le(&b[0x0..]),
            priority_blockhead_uinque_id: u64::from_le_bytes(b[0x18..0x20].try_into().unwrap()),
            fall_speed: i16::from_le_bytes(b[0x20..0x22].try_into().unwrap()),
            x_velocity: i16::from_le_bytes(b[0x22..0x24].try_into().unwrap()),
            y_velocity: i16::from_le_bytes(b[0x24..0x26].try_into().unwrap()),
            hovers: u8::from_le_bytes(b[0x26..0x27].try_into().unwrap()),
            padding: b[0x27..0x28].try_into().unwrap(),
        }
    }
}
impl Default for FreeblockUpdateNetData { fn default() -> Self { Self::read_le(&[0u8; 40]) } }
#[cfg(test)]
mod test_freeblock_update_net_data {
    use super::*;
    #[test]
    fn layout() {
        assert_eq!(core::mem::size_of::<FreeblockUpdateNetData>(), 0x28);
        assert_eq!(<FreeblockUpdateNetData as WireStruct>::SIZE, 0x28);
        assert_eq!(core::mem::offset_of!(FreeblockUpdateNetData, dynamic_object_net_data), 0x0);
        assert_eq!(core::mem::offset_of!(FreeblockUpdateNetData, priority_blockhead_uinque_id), 0x18);
        assert_eq!(core::mem::offset_of!(FreeblockUpdateNetData, fall_speed), 0x20);
        assert_eq!(core::mem::offset_of!(FreeblockUpdateNetData, x_velocity), 0x22);
        assert_eq!(core::mem::offset_of!(FreeblockUpdateNetData, y_velocity), 0x24);
        assert_eq!(core::mem::offset_of!(FreeblockUpdateNetData, hovers), 0x26);
        assert_eq!(core::mem::offset_of!(FreeblockUpdateNetData, padding), 0x27);
    }
    #[test]
    fn roundtrip() {
        let mut raw = [0u8; 0x28];
        for (i, x) in raw.iter_mut().enumerate() { *x = (i as u8).wrapping_mul(37).wrapping_add(11); }
        let v = FreeblockUpdateNetData::read_le(&raw);
        let mut back = vec![0u8; raw.len()];
        v.write_le(&mut back);
        assert_eq!(&back[..], &raw[..]);   // byte-exact incl. padding (floats compared as bytes, NaN-safe)
        assert!(<FreeblockUpdateNetData>::default().to_vec().iter().all(|&x| x == 0));
    }
}

/// size 0x68
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FreightCarCreationNetData {
    /// TrainCarCreationNetData @ 0x0
    pub train_car_creation_net_data: TrainCarCreationNetData,
}
impl WireStruct for FreightCarCreationNetData {
    const SIZE: usize = 0x68;
    fn write_le(&self, out: &mut [u8]) {
        assert!(out.len() >= Self::SIZE);
        out[..Self::SIZE].fill(0);
        self.train_car_creation_net_data.write_le(&mut out[0x0..]);
    }
    fn read_le(b: &[u8]) -> Self {
        assert!(b.len() >= Self::SIZE);
        Self {
            train_car_creation_net_data: TrainCarCreationNetData::read_le(&b[0x0..]),
        }
    }
}
impl Default for FreightCarCreationNetData { fn default() -> Self { Self::read_le(&[0u8; 104]) } }
#[cfg(test)]
mod test_freight_car_creation_net_data {
    use super::*;
    #[test]
    fn layout() {
        assert_eq!(core::mem::size_of::<FreightCarCreationNetData>(), 0x68);
        assert_eq!(<FreightCarCreationNetData as WireStruct>::SIZE, 0x68);
        assert_eq!(core::mem::offset_of!(FreightCarCreationNetData, train_car_creation_net_data), 0x0);
    }
    #[test]
    fn roundtrip() {
        let mut raw = [0u8; 0x68];
        for (i, x) in raw.iter_mut().enumerate() { *x = (i as u8).wrapping_mul(37).wrapping_add(11); }
        let v = FreightCarCreationNetData::read_le(&raw);
        let mut back = vec![0u8; raw.len()];
        v.write_le(&mut back);
        assert_eq!(&back[..], &raw[..]);   // byte-exact incl. padding (floats compared as bytes, NaN-safe)
        assert!(<FreightCarCreationNetData>::default().to_vec().iter().all(|&x| x == 0));
    }
}

/// size 0x68
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HandCarCreationNetData {
    /// TrainCarCreationNetData @ 0x0
    pub train_car_creation_net_data: TrainCarCreationNetData,
}
impl WireStruct for HandCarCreationNetData {
    const SIZE: usize = 0x68;
    fn write_le(&self, out: &mut [u8]) {
        assert!(out.len() >= Self::SIZE);
        out[..Self::SIZE].fill(0);
        self.train_car_creation_net_data.write_le(&mut out[0x0..]);
    }
    fn read_le(b: &[u8]) -> Self {
        assert!(b.len() >= Self::SIZE);
        Self {
            train_car_creation_net_data: TrainCarCreationNetData::read_le(&b[0x0..]),
        }
    }
}
impl Default for HandCarCreationNetData { fn default() -> Self { Self::read_le(&[0u8; 104]) } }
#[cfg(test)]
mod test_hand_car_creation_net_data {
    use super::*;
    #[test]
    fn layout() {
        assert_eq!(core::mem::size_of::<HandCarCreationNetData>(), 0x68);
        assert_eq!(<HandCarCreationNetData as WireStruct>::SIZE, 0x68);
        assert_eq!(core::mem::offset_of!(HandCarCreationNetData, train_car_creation_net_data), 0x0);
    }
    #[test]
    fn roundtrip() {
        let mut raw = [0u8; 0x68];
        for (i, x) in raw.iter_mut().enumerate() { *x = (i as u8).wrapping_mul(37).wrapping_add(11); }
        let v = HandCarCreationNetData::read_le(&raw);
        let mut back = vec![0u8; raw.len()];
        v.write_le(&mut back);
        assert_eq!(&back[..], &raw[..]);   // byte-exact incl. padding (floats compared as bytes, NaN-safe)
        assert!(<HandCarCreationNetData>::default().to_vec().iter().all(|&x| x == 0));
    }
}

/// size 0x28
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct InteractionObjectCreationNetData {
    /// DynamicObjectNetData @ 0x0
    pub dynamic_object_net_data: DynamicObjectNetData,
    /// uint64_t @ 0x18
    pub is_in_use_blockhead_unique_id: u64,
    /// uint16_t @ 0x20
    pub interaction_object_type: u16,
    /// uint8_t @ 0x22
    pub is_in_use: u8,
    /// uint8_t @ 0x23
    pub flipped: u8,
    /// uint16_t @ 0x24
    pub paint_color: u16,
    /// explicit padding at 0x26; preserved on read, zero by default
    pub padding: [u8; 2],
}
impl WireStruct for InteractionObjectCreationNetData {
    const SIZE: usize = 0x28;
    fn write_le(&self, out: &mut [u8]) {
        assert!(out.len() >= Self::SIZE);
        out[..Self::SIZE].fill(0);
        self.dynamic_object_net_data.write_le(&mut out[0x0..]);
        out[0x18..0x20].copy_from_slice(&self.is_in_use_blockhead_unique_id.to_le_bytes());
        out[0x20..0x22].copy_from_slice(&self.interaction_object_type.to_le_bytes());
        out[0x22..0x23].copy_from_slice(&self.is_in_use.to_le_bytes());
        out[0x23..0x24].copy_from_slice(&self.flipped.to_le_bytes());
        out[0x24..0x26].copy_from_slice(&self.paint_color.to_le_bytes());
        out[0x26..0x28].copy_from_slice(&self.padding);
    }
    fn read_le(b: &[u8]) -> Self {
        assert!(b.len() >= Self::SIZE);
        Self {
            dynamic_object_net_data: DynamicObjectNetData::read_le(&b[0x0..]),
            is_in_use_blockhead_unique_id: u64::from_le_bytes(b[0x18..0x20].try_into().unwrap()),
            interaction_object_type: u16::from_le_bytes(b[0x20..0x22].try_into().unwrap()),
            is_in_use: u8::from_le_bytes(b[0x22..0x23].try_into().unwrap()),
            flipped: u8::from_le_bytes(b[0x23..0x24].try_into().unwrap()),
            paint_color: u16::from_le_bytes(b[0x24..0x26].try_into().unwrap()),
            padding: b[0x26..0x28].try_into().unwrap(),
        }
    }
}
impl Default for InteractionObjectCreationNetData { fn default() -> Self { Self::read_le(&[0u8; 40]) } }
#[cfg(test)]
mod test_interaction_object_creation_net_data {
    use super::*;
    #[test]
    fn layout() {
        assert_eq!(core::mem::size_of::<InteractionObjectCreationNetData>(), 0x28);
        assert_eq!(<InteractionObjectCreationNetData as WireStruct>::SIZE, 0x28);
        assert_eq!(core::mem::offset_of!(InteractionObjectCreationNetData, dynamic_object_net_data), 0x0);
        assert_eq!(core::mem::offset_of!(InteractionObjectCreationNetData, is_in_use_blockhead_unique_id), 0x18);
        assert_eq!(core::mem::offset_of!(InteractionObjectCreationNetData, interaction_object_type), 0x20);
        assert_eq!(core::mem::offset_of!(InteractionObjectCreationNetData, is_in_use), 0x22);
        assert_eq!(core::mem::offset_of!(InteractionObjectCreationNetData, flipped), 0x23);
        assert_eq!(core::mem::offset_of!(InteractionObjectCreationNetData, paint_color), 0x24);
        assert_eq!(core::mem::offset_of!(InteractionObjectCreationNetData, padding), 0x26);
    }
    #[test]
    fn roundtrip() {
        let mut raw = [0u8; 0x28];
        for (i, x) in raw.iter_mut().enumerate() { *x = (i as u8).wrapping_mul(37).wrapping_add(11); }
        let v = InteractionObjectCreationNetData::read_le(&raw);
        let mut back = vec![0u8; raw.len()];
        v.write_le(&mut back);
        assert_eq!(&back[..], &raw[..]);   // byte-exact incl. padding (floats compared as bytes, NaN-safe)
        assert!(<InteractionObjectCreationNetData>::default().to_vec().iter().all(|&x| x == 0));
    }
}

/// size 0x20
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct LadderCreationNetData {
    /// DynamicObjectNetData @ 0x0
    pub dynamic_object_net_data: DynamicObjectNetData,
    /// uint16_t @ 0x18
    pub item_type: u16,
    /// uint16_t @ 0x1a
    pub paint_color: u16,
    /// explicit padding at 0x1c; preserved on read, zero by default
    pub padding: [u8; 4],
}
impl WireStruct for LadderCreationNetData {
    const SIZE: usize = 0x20;
    fn write_le(&self, out: &mut [u8]) {
        assert!(out.len() >= Self::SIZE);
        out[..Self::SIZE].fill(0);
        self.dynamic_object_net_data.write_le(&mut out[0x0..]);
        out[0x18..0x1a].copy_from_slice(&self.item_type.to_le_bytes());
        out[0x1a..0x1c].copy_from_slice(&self.paint_color.to_le_bytes());
        out[0x1c..0x20].copy_from_slice(&self.padding);
    }
    fn read_le(b: &[u8]) -> Self {
        assert!(b.len() >= Self::SIZE);
        Self {
            dynamic_object_net_data: DynamicObjectNetData::read_le(&b[0x0..]),
            item_type: u16::from_le_bytes(b[0x18..0x1a].try_into().unwrap()),
            paint_color: u16::from_le_bytes(b[0x1a..0x1c].try_into().unwrap()),
            padding: b[0x1c..0x20].try_into().unwrap(),
        }
    }
}
impl Default for LadderCreationNetData { fn default() -> Self { Self::read_le(&[0u8; 32]) } }
#[cfg(test)]
mod test_ladder_creation_net_data {
    use super::*;
    #[test]
    fn layout() {
        assert_eq!(core::mem::size_of::<LadderCreationNetData>(), 0x20);
        assert_eq!(<LadderCreationNetData as WireStruct>::SIZE, 0x20);
        assert_eq!(core::mem::offset_of!(LadderCreationNetData, dynamic_object_net_data), 0x0);
        assert_eq!(core::mem::offset_of!(LadderCreationNetData, item_type), 0x18);
        assert_eq!(core::mem::offset_of!(LadderCreationNetData, paint_color), 0x1a);
        assert_eq!(core::mem::offset_of!(LadderCreationNetData, padding), 0x1c);
    }
    #[test]
    fn roundtrip() {
        let mut raw = [0u8; 0x20];
        for (i, x) in raw.iter_mut().enumerate() { *x = (i as u8).wrapping_mul(37).wrapping_add(11); }
        let v = LadderCreationNetData::read_le(&raw);
        let mut back = vec![0u8; raw.len()];
        v.write_le(&mut back);
        assert_eq!(&back[..], &raw[..]);   // byte-exact incl. padding (floats compared as bytes, NaN-safe)
        assert!(<LadderCreationNetData>::default().to_vec().iter().all(|&x| x == 0));
    }
}

/// size 0x28
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct MirrorNetData {
    /// InteractionObjectCreationNetData @ 0x0
    pub interaction_object_creation_net_data: InteractionObjectCreationNetData,
}
impl WireStruct for MirrorNetData {
    const SIZE: usize = 0x28;
    fn write_le(&self, out: &mut [u8]) {
        assert!(out.len() >= Self::SIZE);
        out[..Self::SIZE].fill(0);
        self.interaction_object_creation_net_data.write_le(&mut out[0x0..]);
    }
    fn read_le(b: &[u8]) -> Self {
        assert!(b.len() >= Self::SIZE);
        Self {
            interaction_object_creation_net_data: InteractionObjectCreationNetData::read_le(&b[0x0..]),
        }
    }
}
impl Default for MirrorNetData { fn default() -> Self { Self::read_le(&[0u8; 40]) } }
#[cfg(test)]
mod test_mirror_net_data {
    use super::*;
    #[test]
    fn layout() {
        assert_eq!(core::mem::size_of::<MirrorNetData>(), 0x28);
        assert_eq!(<MirrorNetData as WireStruct>::SIZE, 0x28);
        assert_eq!(core::mem::offset_of!(MirrorNetData, interaction_object_creation_net_data), 0x0);
    }
    #[test]
    fn roundtrip() {
        let mut raw = [0u8; 0x28];
        for (i, x) in raw.iter_mut().enumerate() { *x = (i as u8).wrapping_mul(37).wrapping_add(11); }
        let v = MirrorNetData::read_le(&raw);
        let mut back = vec![0u8; raw.len()];
        v.write_le(&mut back);
        assert_eq!(&back[..], &raw[..]);   // byte-exact incl. padding (floats compared as bytes, NaN-safe)
        assert!(<MirrorNetData>::default().to_vec().iter().all(|&x| x == 0));
    }
}

/// size 0x48
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NPCCreationNetData {
    /// DynamicObjectNetData @ 0x0
    pub dynamic_object_net_data: DynamicObjectNetData,
    /// uint64_t @ 0x18
    pub kill_blockhead_id: u64,
    /// uint64_t @ 0x20
    pub rider_id: u64,
    /// uint64_t @ 0x28
    pub interact_blockhead_id: u64,
    /// uint16_t @ 0x30
    pub damage: u16,
    /// uint16_t @ 0x32
    pub hit_force: u16,
    /// uint8_t @ 0x34
    pub dead: u8,
    /// uint8_t @ 0x35
    pub breed: u8,
    /// uint8_t @ 0x36
    pub successful_feed_or_tame: u8,
    /// uint8_t @ 0x37
    pub tame_type: u8,
    /// int16_t @ 0x38
    pub fullness: i16,
    /// int16_t @ 0x3a
    pub age: i16,
    /// uint16_t @ 0x3c
    pub interaction_item_type: u16,
    /// int16_t @ 0x3e
    pub tame_cooldown_timer: i16,
    /// uint8_t @ 0x40
    pub interaction_type: u8,
    /// explicit padding at 0x41; preserved on read, zero by default
    pub padding: [u8; 7],
}
impl WireStruct for NPCCreationNetData {
    const SIZE: usize = 0x48;
    fn write_le(&self, out: &mut [u8]) {
        assert!(out.len() >= Self::SIZE);
        out[..Self::SIZE].fill(0);
        self.dynamic_object_net_data.write_le(&mut out[0x0..]);
        out[0x18..0x20].copy_from_slice(&self.kill_blockhead_id.to_le_bytes());
        out[0x20..0x28].copy_from_slice(&self.rider_id.to_le_bytes());
        out[0x28..0x30].copy_from_slice(&self.interact_blockhead_id.to_le_bytes());
        out[0x30..0x32].copy_from_slice(&self.damage.to_le_bytes());
        out[0x32..0x34].copy_from_slice(&self.hit_force.to_le_bytes());
        out[0x34..0x35].copy_from_slice(&self.dead.to_le_bytes());
        out[0x35..0x36].copy_from_slice(&self.breed.to_le_bytes());
        out[0x36..0x37].copy_from_slice(&self.successful_feed_or_tame.to_le_bytes());
        out[0x37..0x38].copy_from_slice(&self.tame_type.to_le_bytes());
        out[0x38..0x3a].copy_from_slice(&self.fullness.to_le_bytes());
        out[0x3a..0x3c].copy_from_slice(&self.age.to_le_bytes());
        out[0x3c..0x3e].copy_from_slice(&self.interaction_item_type.to_le_bytes());
        out[0x3e..0x40].copy_from_slice(&self.tame_cooldown_timer.to_le_bytes());
        out[0x40..0x41].copy_from_slice(&self.interaction_type.to_le_bytes());
        out[0x41..0x48].copy_from_slice(&self.padding);
    }
    fn read_le(b: &[u8]) -> Self {
        assert!(b.len() >= Self::SIZE);
        Self {
            dynamic_object_net_data: DynamicObjectNetData::read_le(&b[0x0..]),
            kill_blockhead_id: u64::from_le_bytes(b[0x18..0x20].try_into().unwrap()),
            rider_id: u64::from_le_bytes(b[0x20..0x28].try_into().unwrap()),
            interact_blockhead_id: u64::from_le_bytes(b[0x28..0x30].try_into().unwrap()),
            damage: u16::from_le_bytes(b[0x30..0x32].try_into().unwrap()),
            hit_force: u16::from_le_bytes(b[0x32..0x34].try_into().unwrap()),
            dead: u8::from_le_bytes(b[0x34..0x35].try_into().unwrap()),
            breed: u8::from_le_bytes(b[0x35..0x36].try_into().unwrap()),
            successful_feed_or_tame: u8::from_le_bytes(b[0x36..0x37].try_into().unwrap()),
            tame_type: u8::from_le_bytes(b[0x37..0x38].try_into().unwrap()),
            fullness: i16::from_le_bytes(b[0x38..0x3a].try_into().unwrap()),
            age: i16::from_le_bytes(b[0x3a..0x3c].try_into().unwrap()),
            interaction_item_type: u16::from_le_bytes(b[0x3c..0x3e].try_into().unwrap()),
            tame_cooldown_timer: i16::from_le_bytes(b[0x3e..0x40].try_into().unwrap()),
            interaction_type: u8::from_le_bytes(b[0x40..0x41].try_into().unwrap()),
            padding: b[0x41..0x48].try_into().unwrap(),
        }
    }
}
impl Default for NPCCreationNetData { fn default() -> Self { Self::read_le(&[0u8; 72]) } }
#[cfg(test)]
mod test_npccreation_net_data {
    use super::*;
    #[test]
    fn layout() {
        assert_eq!(core::mem::size_of::<NPCCreationNetData>(), 0x48);
        assert_eq!(<NPCCreationNetData as WireStruct>::SIZE, 0x48);
        assert_eq!(core::mem::offset_of!(NPCCreationNetData, dynamic_object_net_data), 0x0);
        assert_eq!(core::mem::offset_of!(NPCCreationNetData, kill_blockhead_id), 0x18);
        assert_eq!(core::mem::offset_of!(NPCCreationNetData, rider_id), 0x20);
        assert_eq!(core::mem::offset_of!(NPCCreationNetData, interact_blockhead_id), 0x28);
        assert_eq!(core::mem::offset_of!(NPCCreationNetData, damage), 0x30);
        assert_eq!(core::mem::offset_of!(NPCCreationNetData, hit_force), 0x32);
        assert_eq!(core::mem::offset_of!(NPCCreationNetData, dead), 0x34);
        assert_eq!(core::mem::offset_of!(NPCCreationNetData, breed), 0x35);
        assert_eq!(core::mem::offset_of!(NPCCreationNetData, successful_feed_or_tame), 0x36);
        assert_eq!(core::mem::offset_of!(NPCCreationNetData, tame_type), 0x37);
        assert_eq!(core::mem::offset_of!(NPCCreationNetData, fullness), 0x38);
        assert_eq!(core::mem::offset_of!(NPCCreationNetData, age), 0x3a);
        assert_eq!(core::mem::offset_of!(NPCCreationNetData, interaction_item_type), 0x3c);
        assert_eq!(core::mem::offset_of!(NPCCreationNetData, tame_cooldown_timer), 0x3e);
        assert_eq!(core::mem::offset_of!(NPCCreationNetData, interaction_type), 0x40);
        assert_eq!(core::mem::offset_of!(NPCCreationNetData, padding), 0x41);
    }
    #[test]
    fn roundtrip() {
        let mut raw = [0u8; 0x48];
        for (i, x) in raw.iter_mut().enumerate() { *x = (i as u8).wrapping_mul(37).wrapping_add(11); }
        let v = NPCCreationNetData::read_le(&raw);
        let mut back = vec![0u8; raw.len()];
        v.write_le(&mut back);
        assert_eq!(&back[..], &raw[..]);   // byte-exact incl. padding (floats compared as bytes, NaN-safe)
        assert!(<NPCCreationNetData>::default().to_vec().iter().all(|&x| x == 0));
    }
}

/// size 0x18
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NPCUpdateNetData {
    /// DynamicObjectNetData @ 0x0
    pub dynamic_object_net_data: DynamicObjectNetData,
}
impl WireStruct for NPCUpdateNetData {
    const SIZE: usize = 0x18;
    fn write_le(&self, out: &mut [u8]) {
        assert!(out.len() >= Self::SIZE);
        out[..Self::SIZE].fill(0);
        self.dynamic_object_net_data.write_le(&mut out[0x0..]);
    }
    fn read_le(b: &[u8]) -> Self {
        assert!(b.len() >= Self::SIZE);
        Self {
            dynamic_object_net_data: DynamicObjectNetData::read_le(&b[0x0..]),
        }
    }
}
impl Default for NPCUpdateNetData { fn default() -> Self { Self::read_le(&[0u8; 24]) } }
#[cfg(test)]
mod test_npcupdate_net_data {
    use super::*;
    #[test]
    fn layout() {
        assert_eq!(core::mem::size_of::<NPCUpdateNetData>(), 0x18);
        assert_eq!(<NPCUpdateNetData as WireStruct>::SIZE, 0x18);
        assert_eq!(core::mem::offset_of!(NPCUpdateNetData, dynamic_object_net_data), 0x0);
    }
    #[test]
    fn roundtrip() {
        let mut raw = [0u8; 0x18];
        for (i, x) in raw.iter_mut().enumerate() { *x = (i as u8).wrapping_mul(37).wrapping_add(11); }
        let v = NPCUpdateNetData::read_le(&raw);
        let mut back = vec![0u8; raw.len()];
        v.write_le(&mut back);
        assert_eq!(&back[..], &raw[..]);   // byte-exact incl. padding (floats compared as bytes, NaN-safe)
        assert!(<NPCUpdateNetData>::default().to_vec().iter().all(|&x| x == 0));
    }
}

/// size 0x20
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PaintingCreationNetData {
    /// DynamicObjectNetData @ 0x0
    pub dynamic_object_net_data: DynamicObjectNetData,
    /// uint16_t @ 0x18
    pub item_type: u16,
    /// explicit padding at 0x1a; preserved on read, zero by default
    pub padding: [u8; 6],
}
impl WireStruct for PaintingCreationNetData {
    const SIZE: usize = 0x20;
    fn write_le(&self, out: &mut [u8]) {
        assert!(out.len() >= Self::SIZE);
        out[..Self::SIZE].fill(0);
        self.dynamic_object_net_data.write_le(&mut out[0x0..]);
        out[0x18..0x1a].copy_from_slice(&self.item_type.to_le_bytes());
        out[0x1a..0x20].copy_from_slice(&self.padding);
    }
    fn read_le(b: &[u8]) -> Self {
        assert!(b.len() >= Self::SIZE);
        Self {
            dynamic_object_net_data: DynamicObjectNetData::read_le(&b[0x0..]),
            item_type: u16::from_le_bytes(b[0x18..0x1a].try_into().unwrap()),
            padding: b[0x1a..0x20].try_into().unwrap(),
        }
    }
}
impl Default for PaintingCreationNetData { fn default() -> Self { Self::read_le(&[0u8; 32]) } }
#[cfg(test)]
mod test_painting_creation_net_data {
    use super::*;
    #[test]
    fn layout() {
        assert_eq!(core::mem::size_of::<PaintingCreationNetData>(), 0x20);
        assert_eq!(<PaintingCreationNetData as WireStruct>::SIZE, 0x20);
        assert_eq!(core::mem::offset_of!(PaintingCreationNetData, dynamic_object_net_data), 0x0);
        assert_eq!(core::mem::offset_of!(PaintingCreationNetData, item_type), 0x18);
        assert_eq!(core::mem::offset_of!(PaintingCreationNetData, padding), 0x1a);
    }
    #[test]
    fn roundtrip() {
        let mut raw = [0u8; 0x20];
        for (i, x) in raw.iter_mut().enumerate() { *x = (i as u8).wrapping_mul(37).wrapping_add(11); }
        let v = PaintingCreationNetData::read_le(&raw);
        let mut back = vec![0u8; raw.len()];
        v.write_le(&mut back);
        assert_eq!(&back[..], &raw[..]);   // byte-exact incl. padding (floats compared as bytes, NaN-safe)
        assert!(<PaintingCreationNetData>::default().to_vec().iter().all(|&x| x == 0));
    }
}

/// size 0x68
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PassengerCarCreationNetData {
    /// TrainCarCreationNetData @ 0x0
    pub train_car_creation_net_data: TrainCarCreationNetData,
}
impl WireStruct for PassengerCarCreationNetData {
    const SIZE: usize = 0x68;
    fn write_le(&self, out: &mut [u8]) {
        assert!(out.len() >= Self::SIZE);
        out[..Self::SIZE].fill(0);
        self.train_car_creation_net_data.write_le(&mut out[0x0..]);
    }
    fn read_le(b: &[u8]) -> Self {
        assert!(b.len() >= Self::SIZE);
        Self {
            train_car_creation_net_data: TrainCarCreationNetData::read_le(&b[0x0..]),
        }
    }
}
impl Default for PassengerCarCreationNetData { fn default() -> Self { Self::read_le(&[0u8; 104]) } }
#[cfg(test)]
mod test_passenger_car_creation_net_data {
    use super::*;
    #[test]
    fn layout() {
        assert_eq!(core::mem::size_of::<PassengerCarCreationNetData>(), 0x68);
        assert_eq!(<PassengerCarCreationNetData as WireStruct>::SIZE, 0x68);
        assert_eq!(core::mem::offset_of!(PassengerCarCreationNetData, train_car_creation_net_data), 0x0);
    }
    #[test]
    fn roundtrip() {
        let mut raw = [0u8; 0x68];
        for (i, x) in raw.iter_mut().enumerate() { *x = (i as u8).wrapping_mul(37).wrapping_add(11); }
        let v = PassengerCarCreationNetData::read_le(&raw);
        let mut back = vec![0u8; raw.len()];
        v.write_le(&mut back);
        assert_eq!(&back[..], &raw[..]);   // byte-exact incl. padding (floats compared as bytes, NaN-safe)
        assert!(<PassengerCarCreationNetData>::default().to_vec().iter().all(|&x| x == 0));
    }
}

/// size 0x28
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PlantCreationNetData {
    /// DynamicObjectNetData @ 0x0
    pub dynamic_object_net_data: DynamicObjectNetData,
    /// uint16_t @ 0x18
    pub age: u16,
    /// uint16_t @ 0x1a
    pub display_gene_data_a: u16,
    /// uint16_t @ 0x1c
    pub display_gene_data_b: u16,
    /// uint16_t @ 0x1e
    pub display_gene_data_c: u16,
    /// int16_t @ 0x20
    pub extra_data: i16,
    /// uint8_t @ 0x22
    pub flowering: u8,
    /// explicit padding at 0x23; preserved on read, zero by default
    pub padding: [u8; 5],
}
impl WireStruct for PlantCreationNetData {
    const SIZE: usize = 0x28;
    fn write_le(&self, out: &mut [u8]) {
        assert!(out.len() >= Self::SIZE);
        out[..Self::SIZE].fill(0);
        self.dynamic_object_net_data.write_le(&mut out[0x0..]);
        out[0x18..0x1a].copy_from_slice(&self.age.to_le_bytes());
        out[0x1a..0x1c].copy_from_slice(&self.display_gene_data_a.to_le_bytes());
        out[0x1c..0x1e].copy_from_slice(&self.display_gene_data_b.to_le_bytes());
        out[0x1e..0x20].copy_from_slice(&self.display_gene_data_c.to_le_bytes());
        out[0x20..0x22].copy_from_slice(&self.extra_data.to_le_bytes());
        out[0x22..0x23].copy_from_slice(&self.flowering.to_le_bytes());
        out[0x23..0x28].copy_from_slice(&self.padding);
    }
    fn read_le(b: &[u8]) -> Self {
        assert!(b.len() >= Self::SIZE);
        Self {
            dynamic_object_net_data: DynamicObjectNetData::read_le(&b[0x0..]),
            age: u16::from_le_bytes(b[0x18..0x1a].try_into().unwrap()),
            display_gene_data_a: u16::from_le_bytes(b[0x1a..0x1c].try_into().unwrap()),
            display_gene_data_b: u16::from_le_bytes(b[0x1c..0x1e].try_into().unwrap()),
            display_gene_data_c: u16::from_le_bytes(b[0x1e..0x20].try_into().unwrap()),
            extra_data: i16::from_le_bytes(b[0x20..0x22].try_into().unwrap()),
            flowering: u8::from_le_bytes(b[0x22..0x23].try_into().unwrap()),
            padding: b[0x23..0x28].try_into().unwrap(),
        }
    }
}
impl Default for PlantCreationNetData { fn default() -> Self { Self::read_le(&[0u8; 40]) } }
#[cfg(test)]
mod test_plant_creation_net_data {
    use super::*;
    #[test]
    fn layout() {
        assert_eq!(core::mem::size_of::<PlantCreationNetData>(), 0x28);
        assert_eq!(<PlantCreationNetData as WireStruct>::SIZE, 0x28);
        assert_eq!(core::mem::offset_of!(PlantCreationNetData, dynamic_object_net_data), 0x0);
        assert_eq!(core::mem::offset_of!(PlantCreationNetData, age), 0x18);
        assert_eq!(core::mem::offset_of!(PlantCreationNetData, display_gene_data_a), 0x1a);
        assert_eq!(core::mem::offset_of!(PlantCreationNetData, display_gene_data_b), 0x1c);
        assert_eq!(core::mem::offset_of!(PlantCreationNetData, display_gene_data_c), 0x1e);
        assert_eq!(core::mem::offset_of!(PlantCreationNetData, extra_data), 0x20);
        assert_eq!(core::mem::offset_of!(PlantCreationNetData, flowering), 0x22);
        assert_eq!(core::mem::offset_of!(PlantCreationNetData, padding), 0x23);
    }
    #[test]
    fn roundtrip() {
        let mut raw = [0u8; 0x28];
        for (i, x) in raw.iter_mut().enumerate() { *x = (i as u8).wrapping_mul(37).wrapping_add(11); }
        let v = PlantCreationNetData::read_le(&raw);
        let mut back = vec![0u8; raw.len()];
        v.write_le(&mut back);
        assert_eq!(&back[..], &raw[..]);   // byte-exact incl. padding (floats compared as bytes, NaN-safe)
        assert!(<PlantCreationNetData>::default().to_vec().iter().all(|&x| x == 0));
    }
}

/// size 0x20
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct RailCreationNetData {
    /// DynamicObjectNetData @ 0x0
    pub dynamic_object_net_data: DynamicObjectNetData,
    /// uint16_t @ 0x18
    pub item_type: u16,
    /// uint8_t @ 0x1a
    pub configuration: u8,
    /// explicit padding at 0x1b; preserved on read, zero by default
    pub padding: [u8; 5],
}
impl WireStruct for RailCreationNetData {
    const SIZE: usize = 0x20;
    fn write_le(&self, out: &mut [u8]) {
        assert!(out.len() >= Self::SIZE);
        out[..Self::SIZE].fill(0);
        self.dynamic_object_net_data.write_le(&mut out[0x0..]);
        out[0x18..0x1a].copy_from_slice(&self.item_type.to_le_bytes());
        out[0x1a..0x1b].copy_from_slice(&self.configuration.to_le_bytes());
        out[0x1b..0x20].copy_from_slice(&self.padding);
    }
    fn read_le(b: &[u8]) -> Self {
        assert!(b.len() >= Self::SIZE);
        Self {
            dynamic_object_net_data: DynamicObjectNetData::read_le(&b[0x0..]),
            item_type: u16::from_le_bytes(b[0x18..0x1a].try_into().unwrap()),
            configuration: u8::from_le_bytes(b[0x1a..0x1b].try_into().unwrap()),
            padding: b[0x1b..0x20].try_into().unwrap(),
        }
    }
}
impl Default for RailCreationNetData { fn default() -> Self { Self::read_le(&[0u8; 32]) } }
#[cfg(test)]
mod test_rail_creation_net_data {
    use super::*;
    #[test]
    fn layout() {
        assert_eq!(core::mem::size_of::<RailCreationNetData>(), 0x20);
        assert_eq!(<RailCreationNetData as WireStruct>::SIZE, 0x20);
        assert_eq!(core::mem::offset_of!(RailCreationNetData, dynamic_object_net_data), 0x0);
        assert_eq!(core::mem::offset_of!(RailCreationNetData, item_type), 0x18);
        assert_eq!(core::mem::offset_of!(RailCreationNetData, configuration), 0x1a);
        assert_eq!(core::mem::offset_of!(RailCreationNetData, padding), 0x1b);
    }
    #[test]
    fn roundtrip() {
        let mut raw = [0u8; 0x20];
        for (i, x) in raw.iter_mut().enumerate() { *x = (i as u8).wrapping_mul(37).wrapping_add(11); }
        let v = RailCreationNetData::read_le(&raw);
        let mut back = vec![0u8; raw.len()];
        v.write_le(&mut back);
        assert_eq!(&back[..], &raw[..]);   // byte-exact incl. padding (floats compared as bytes, NaN-safe)
        assert!(<RailCreationNetData>::default().to_vec().iter().all(|&x| x == 0));
    }
}

/// size 0x28
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ScorpionUpdateNetData {
    /// NPCUpdateNetData @ 0x0
    pub npc_update_net_data: NPCUpdateNetData,
    /// uint32_t @ 0x18
    pub to_square_x: u32,
    /// uint32_t @ 0x1c
    pub to_square_y: u32,
    /// int16_t @ 0x20
    pub walk_speed: i16,
    /// int8_t @ 0x22
    pub movement_direction: i8,
    /// uint8_t @ 0x23
    pub clear_queue: u8,
    /// explicit padding at 0x24; preserved on read, zero by default
    pub padding: [u8; 4],
}
impl WireStruct for ScorpionUpdateNetData {
    const SIZE: usize = 0x28;
    fn write_le(&self, out: &mut [u8]) {
        assert!(out.len() >= Self::SIZE);
        out[..Self::SIZE].fill(0);
        self.npc_update_net_data.write_le(&mut out[0x0..]);
        out[0x18..0x1c].copy_from_slice(&self.to_square_x.to_le_bytes());
        out[0x1c..0x20].copy_from_slice(&self.to_square_y.to_le_bytes());
        out[0x20..0x22].copy_from_slice(&self.walk_speed.to_le_bytes());
        out[0x22..0x23].copy_from_slice(&self.movement_direction.to_le_bytes());
        out[0x23..0x24].copy_from_slice(&self.clear_queue.to_le_bytes());
        out[0x24..0x28].copy_from_slice(&self.padding);
    }
    fn read_le(b: &[u8]) -> Self {
        assert!(b.len() >= Self::SIZE);
        Self {
            npc_update_net_data: NPCUpdateNetData::read_le(&b[0x0..]),
            to_square_x: u32::from_le_bytes(b[0x18..0x1c].try_into().unwrap()),
            to_square_y: u32::from_le_bytes(b[0x1c..0x20].try_into().unwrap()),
            walk_speed: i16::from_le_bytes(b[0x20..0x22].try_into().unwrap()),
            movement_direction: i8::from_le_bytes(b[0x22..0x23].try_into().unwrap()),
            clear_queue: u8::from_le_bytes(b[0x23..0x24].try_into().unwrap()),
            padding: b[0x24..0x28].try_into().unwrap(),
        }
    }
}
impl Default for ScorpionUpdateNetData { fn default() -> Self { Self::read_le(&[0u8; 40]) } }
#[cfg(test)]
mod test_scorpion_update_net_data {
    use super::*;
    #[test]
    fn layout() {
        assert_eq!(core::mem::size_of::<ScorpionUpdateNetData>(), 0x28);
        assert_eq!(<ScorpionUpdateNetData as WireStruct>::SIZE, 0x28);
        assert_eq!(core::mem::offset_of!(ScorpionUpdateNetData, npc_update_net_data), 0x0);
        assert_eq!(core::mem::offset_of!(ScorpionUpdateNetData, to_square_x), 0x18);
        assert_eq!(core::mem::offset_of!(ScorpionUpdateNetData, to_square_y), 0x1c);
        assert_eq!(core::mem::offset_of!(ScorpionUpdateNetData, walk_speed), 0x20);
        assert_eq!(core::mem::offset_of!(ScorpionUpdateNetData, movement_direction), 0x22);
        assert_eq!(core::mem::offset_of!(ScorpionUpdateNetData, clear_queue), 0x23);
        assert_eq!(core::mem::offset_of!(ScorpionUpdateNetData, padding), 0x24);
    }
    #[test]
    fn roundtrip() {
        let mut raw = [0u8; 0x28];
        for (i, x) in raw.iter_mut().enumerate() { *x = (i as u8).wrapping_mul(37).wrapping_add(11); }
        let v = ScorpionUpdateNetData::read_le(&raw);
        let mut back = vec![0u8; raw.len()];
        v.write_le(&mut back);
        assert_eq!(&back[..], &raw[..]);   // byte-exact incl. padding (floats compared as bytes, NaN-safe)
        assert!(<ScorpionUpdateNetData>::default().to_vec().iter().all(|&x| x == 0));
    }
}

/// size 0x80
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ScorpionCreationNetData {
    /// NPCCreationNetData @ 0x0
    pub npc_creation_net_data: NPCCreationNetData,
    /// ScorpionUpdateNetData @ 0x48
    pub scorpion_update_net_data: ScorpionUpdateNetData,
    /// uint64_t @ 0x70
    pub attack_blockhead: u64,
    /// uint8_t @ 0x78
    pub attack_is_spitting: u8,
    /// explicit padding at 0x79; preserved on read, zero by default
    pub padding: [u8; 7],
}
impl WireStruct for ScorpionCreationNetData {
    const SIZE: usize = 0x80;
    fn write_le(&self, out: &mut [u8]) {
        assert!(out.len() >= Self::SIZE);
        out[..Self::SIZE].fill(0);
        self.npc_creation_net_data.write_le(&mut out[0x0..]);
        self.scorpion_update_net_data.write_le(&mut out[0x48..]);
        out[0x70..0x78].copy_from_slice(&self.attack_blockhead.to_le_bytes());
        out[0x78..0x79].copy_from_slice(&self.attack_is_spitting.to_le_bytes());
        out[0x79..0x80].copy_from_slice(&self.padding);
    }
    fn read_le(b: &[u8]) -> Self {
        assert!(b.len() >= Self::SIZE);
        Self {
            npc_creation_net_data: NPCCreationNetData::read_le(&b[0x0..]),
            scorpion_update_net_data: ScorpionUpdateNetData::read_le(&b[0x48..]),
            attack_blockhead: u64::from_le_bytes(b[0x70..0x78].try_into().unwrap()),
            attack_is_spitting: u8::from_le_bytes(b[0x78..0x79].try_into().unwrap()),
            padding: b[0x79..0x80].try_into().unwrap(),
        }
    }
}
impl Default for ScorpionCreationNetData { fn default() -> Self { Self::read_le(&[0u8; 128]) } }
#[cfg(test)]
mod test_scorpion_creation_net_data {
    use super::*;
    #[test]
    fn layout() {
        assert_eq!(core::mem::size_of::<ScorpionCreationNetData>(), 0x80);
        assert_eq!(<ScorpionCreationNetData as WireStruct>::SIZE, 0x80);
        assert_eq!(core::mem::offset_of!(ScorpionCreationNetData, npc_creation_net_data), 0x0);
        assert_eq!(core::mem::offset_of!(ScorpionCreationNetData, scorpion_update_net_data), 0x48);
        assert_eq!(core::mem::offset_of!(ScorpionCreationNetData, attack_blockhead), 0x70);
        assert_eq!(core::mem::offset_of!(ScorpionCreationNetData, attack_is_spitting), 0x78);
        assert_eq!(core::mem::offset_of!(ScorpionCreationNetData, padding), 0x79);
    }
    #[test]
    fn roundtrip() {
        let mut raw = [0u8; 0x80];
        for (i, x) in raw.iter_mut().enumerate() { *x = (i as u8).wrapping_mul(37).wrapping_add(11); }
        let v = ScorpionCreationNetData::read_le(&raw);
        let mut back = vec![0u8; raw.len()];
        v.write_le(&mut back);
        assert_eq!(&back[..], &raw[..]);   // byte-exact incl. padding (floats compared as bytes, NaN-safe)
        assert!(<ScorpionCreationNetData>::default().to_vec().iter().all(|&x| x == 0));
    }
}

/// size 0x28
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SharkUpdateNetData {
    /// NPCUpdateNetData @ 0x0
    pub npc_update_net_data: NPCUpdateNetData,
    /// uint32_t @ 0x18
    pub to_square_x: u32,
    /// uint32_t @ 0x1c
    pub to_square_y: u32,
    /// int16_t @ 0x20
    pub walk_speed: i16,
    /// uint16_t @ 0x22
    pub age: u16,
    /// int8_t @ 0x24
    pub movement_direction: i8,
    /// int8_t @ 0x25
    pub clear_queue: i8,
    /// explicit padding at 0x26; preserved on read, zero by default
    pub padding: [u8; 2],
}
impl WireStruct for SharkUpdateNetData {
    const SIZE: usize = 0x28;
    fn write_le(&self, out: &mut [u8]) {
        assert!(out.len() >= Self::SIZE);
        out[..Self::SIZE].fill(0);
        self.npc_update_net_data.write_le(&mut out[0x0..]);
        out[0x18..0x1c].copy_from_slice(&self.to_square_x.to_le_bytes());
        out[0x1c..0x20].copy_from_slice(&self.to_square_y.to_le_bytes());
        out[0x20..0x22].copy_from_slice(&self.walk_speed.to_le_bytes());
        out[0x22..0x24].copy_from_slice(&self.age.to_le_bytes());
        out[0x24..0x25].copy_from_slice(&self.movement_direction.to_le_bytes());
        out[0x25..0x26].copy_from_slice(&self.clear_queue.to_le_bytes());
        out[0x26..0x28].copy_from_slice(&self.padding);
    }
    fn read_le(b: &[u8]) -> Self {
        assert!(b.len() >= Self::SIZE);
        Self {
            npc_update_net_data: NPCUpdateNetData::read_le(&b[0x0..]),
            to_square_x: u32::from_le_bytes(b[0x18..0x1c].try_into().unwrap()),
            to_square_y: u32::from_le_bytes(b[0x1c..0x20].try_into().unwrap()),
            walk_speed: i16::from_le_bytes(b[0x20..0x22].try_into().unwrap()),
            age: u16::from_le_bytes(b[0x22..0x24].try_into().unwrap()),
            movement_direction: i8::from_le_bytes(b[0x24..0x25].try_into().unwrap()),
            clear_queue: i8::from_le_bytes(b[0x25..0x26].try_into().unwrap()),
            padding: b[0x26..0x28].try_into().unwrap(),
        }
    }
}
impl Default for SharkUpdateNetData { fn default() -> Self { Self::read_le(&[0u8; 40]) } }
#[cfg(test)]
mod test_shark_update_net_data {
    use super::*;
    #[test]
    fn layout() {
        assert_eq!(core::mem::size_of::<SharkUpdateNetData>(), 0x28);
        assert_eq!(<SharkUpdateNetData as WireStruct>::SIZE, 0x28);
        assert_eq!(core::mem::offset_of!(SharkUpdateNetData, npc_update_net_data), 0x0);
        assert_eq!(core::mem::offset_of!(SharkUpdateNetData, to_square_x), 0x18);
        assert_eq!(core::mem::offset_of!(SharkUpdateNetData, to_square_y), 0x1c);
        assert_eq!(core::mem::offset_of!(SharkUpdateNetData, walk_speed), 0x20);
        assert_eq!(core::mem::offset_of!(SharkUpdateNetData, age), 0x22);
        assert_eq!(core::mem::offset_of!(SharkUpdateNetData, movement_direction), 0x24);
        assert_eq!(core::mem::offset_of!(SharkUpdateNetData, clear_queue), 0x25);
        assert_eq!(core::mem::offset_of!(SharkUpdateNetData, padding), 0x26);
    }
    #[test]
    fn roundtrip() {
        let mut raw = [0u8; 0x28];
        for (i, x) in raw.iter_mut().enumerate() { *x = (i as u8).wrapping_mul(37).wrapping_add(11); }
        let v = SharkUpdateNetData::read_le(&raw);
        let mut back = vec![0u8; raw.len()];
        v.write_le(&mut back);
        assert_eq!(&back[..], &raw[..]);   // byte-exact incl. padding (floats compared as bytes, NaN-safe)
        assert!(<SharkUpdateNetData>::default().to_vec().iter().all(|&x| x == 0));
    }
}

/// size 0x78
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SharkCreationNetData {
    /// NPCCreationNetData @ 0x0
    pub npc_creation_net_data: NPCCreationNetData,
    /// SharkUpdateNetData @ 0x48
    pub shark_update_net_data: SharkUpdateNetData,
    /// uint64_t @ 0x70
    pub attack_blockhead: u64,
}
impl WireStruct for SharkCreationNetData {
    const SIZE: usize = 0x78;
    fn write_le(&self, out: &mut [u8]) {
        assert!(out.len() >= Self::SIZE);
        out[..Self::SIZE].fill(0);
        self.npc_creation_net_data.write_le(&mut out[0x0..]);
        self.shark_update_net_data.write_le(&mut out[0x48..]);
        out[0x70..0x78].copy_from_slice(&self.attack_blockhead.to_le_bytes());
    }
    fn read_le(b: &[u8]) -> Self {
        assert!(b.len() >= Self::SIZE);
        Self {
            npc_creation_net_data: NPCCreationNetData::read_le(&b[0x0..]),
            shark_update_net_data: SharkUpdateNetData::read_le(&b[0x48..]),
            attack_blockhead: u64::from_le_bytes(b[0x70..0x78].try_into().unwrap()),
        }
    }
}
impl Default for SharkCreationNetData { fn default() -> Self { Self::read_le(&[0u8; 120]) } }
#[cfg(test)]
mod test_shark_creation_net_data {
    use super::*;
    #[test]
    fn layout() {
        assert_eq!(core::mem::size_of::<SharkCreationNetData>(), 0x78);
        assert_eq!(<SharkCreationNetData as WireStruct>::SIZE, 0x78);
        assert_eq!(core::mem::offset_of!(SharkCreationNetData, npc_creation_net_data), 0x0);
        assert_eq!(core::mem::offset_of!(SharkCreationNetData, shark_update_net_data), 0x48);
        assert_eq!(core::mem::offset_of!(SharkCreationNetData, attack_blockhead), 0x70);
    }
    #[test]
    fn roundtrip() {
        let mut raw = [0u8; 0x78];
        for (i, x) in raw.iter_mut().enumerate() { *x = (i as u8).wrapping_mul(37).wrapping_add(11); }
        let v = SharkCreationNetData::read_le(&raw);
        let mut back = vec![0u8; raw.len()];
        v.write_le(&mut back);
        assert_eq!(&back[..], &raw[..]);   // byte-exact incl. padding (floats compared as bytes, NaN-safe)
        assert!(<SharkCreationNetData>::default().to_vec().iter().all(|&x| x == 0));
    }
}

/// size 0x30
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SignCreationNetData {
    /// InteractionObjectCreationNetData @ 0x0
    pub interaction_object_creation_net_data: InteractionObjectCreationNetData,
    /// uint8_t @ 0x28
    pub connection_type: u8,
    /// uint8_t @ 0x29
    pub offset_type: u8,
    /// explicit padding at 0x2a; preserved on read, zero by default
    pub padding: [u8; 6],
}
impl WireStruct for SignCreationNetData {
    const SIZE: usize = 0x30;
    fn write_le(&self, out: &mut [u8]) {
        assert!(out.len() >= Self::SIZE);
        out[..Self::SIZE].fill(0);
        self.interaction_object_creation_net_data.write_le(&mut out[0x0..]);
        out[0x28..0x29].copy_from_slice(&self.connection_type.to_le_bytes());
        out[0x29..0x2a].copy_from_slice(&self.offset_type.to_le_bytes());
        out[0x2a..0x30].copy_from_slice(&self.padding);
    }
    fn read_le(b: &[u8]) -> Self {
        assert!(b.len() >= Self::SIZE);
        Self {
            interaction_object_creation_net_data: InteractionObjectCreationNetData::read_le(&b[0x0..]),
            connection_type: u8::from_le_bytes(b[0x28..0x29].try_into().unwrap()),
            offset_type: u8::from_le_bytes(b[0x29..0x2a].try_into().unwrap()),
            padding: b[0x2a..0x30].try_into().unwrap(),
        }
    }
}
impl Default for SignCreationNetData { fn default() -> Self { Self::read_le(&[0u8; 48]) } }
#[cfg(test)]
mod test_sign_creation_net_data {
    use super::*;
    #[test]
    fn layout() {
        assert_eq!(core::mem::size_of::<SignCreationNetData>(), 0x30);
        assert_eq!(<SignCreationNetData as WireStruct>::SIZE, 0x30);
        assert_eq!(core::mem::offset_of!(SignCreationNetData, interaction_object_creation_net_data), 0x0);
        assert_eq!(core::mem::offset_of!(SignCreationNetData, connection_type), 0x28);
        assert_eq!(core::mem::offset_of!(SignCreationNetData, offset_type), 0x29);
        assert_eq!(core::mem::offset_of!(SignCreationNetData, padding), 0x2a);
    }
    #[test]
    fn roundtrip() {
        let mut raw = [0u8; 0x30];
        for (i, x) in raw.iter_mut().enumerate() { *x = (i as u8).wrapping_mul(37).wrapping_add(11); }
        let v = SignCreationNetData::read_le(&raw);
        let mut back = vec![0u8; raw.len()];
        v.write_le(&mut back);
        assert_eq!(&back[..], &raw[..]);   // byte-exact incl. padding (floats compared as bytes, NaN-safe)
        assert!(<SignCreationNetData>::default().to_vec().iter().all(|&x| x == 0));
    }
}

/// size 0x20
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct StairsCreationNetData {
    /// DynamicObjectNetData @ 0x0
    pub dynamic_object_net_data: DynamicObjectNetData,
    /// uint16_t @ 0x18
    pub item_type: u16,
    /// uint8_t @ 0x1a
    pub configuration: u8,
    /// uint8_t @ 0x1b
    pub padding_a: u8,
    /// uint16_t @ 0x1c
    pub paint_color: u16,
    /// explicit padding at 0x1e; preserved on read, zero by default
    pub padding: [u8; 2],
}
impl WireStruct for StairsCreationNetData {
    const SIZE: usize = 0x20;
    fn write_le(&self, out: &mut [u8]) {
        assert!(out.len() >= Self::SIZE);
        out[..Self::SIZE].fill(0);
        self.dynamic_object_net_data.write_le(&mut out[0x0..]);
        out[0x18..0x1a].copy_from_slice(&self.item_type.to_le_bytes());
        out[0x1a..0x1b].copy_from_slice(&self.configuration.to_le_bytes());
        out[0x1b..0x1c].copy_from_slice(&self.padding_a.to_le_bytes());
        out[0x1c..0x1e].copy_from_slice(&self.paint_color.to_le_bytes());
        out[0x1e..0x20].copy_from_slice(&self.padding);
    }
    fn read_le(b: &[u8]) -> Self {
        assert!(b.len() >= Self::SIZE);
        Self {
            dynamic_object_net_data: DynamicObjectNetData::read_le(&b[0x0..]),
            item_type: u16::from_le_bytes(b[0x18..0x1a].try_into().unwrap()),
            configuration: u8::from_le_bytes(b[0x1a..0x1b].try_into().unwrap()),
            padding_a: u8::from_le_bytes(b[0x1b..0x1c].try_into().unwrap()),
            paint_color: u16::from_le_bytes(b[0x1c..0x1e].try_into().unwrap()),
            padding: b[0x1e..0x20].try_into().unwrap(),
        }
    }
}
impl Default for StairsCreationNetData { fn default() -> Self { Self::read_le(&[0u8; 32]) } }
#[cfg(test)]
mod test_stairs_creation_net_data {
    use super::*;
    #[test]
    fn layout() {
        assert_eq!(core::mem::size_of::<StairsCreationNetData>(), 0x20);
        assert_eq!(<StairsCreationNetData as WireStruct>::SIZE, 0x20);
        assert_eq!(core::mem::offset_of!(StairsCreationNetData, dynamic_object_net_data), 0x0);
        assert_eq!(core::mem::offset_of!(StairsCreationNetData, item_type), 0x18);
        assert_eq!(core::mem::offset_of!(StairsCreationNetData, configuration), 0x1a);
        assert_eq!(core::mem::offset_of!(StairsCreationNetData, padding_a), 0x1b);
        assert_eq!(core::mem::offset_of!(StairsCreationNetData, paint_color), 0x1c);
        assert_eq!(core::mem::offset_of!(StairsCreationNetData, padding), 0x1e);
    }
    #[test]
    fn roundtrip() {
        let mut raw = [0u8; 0x20];
        for (i, x) in raw.iter_mut().enumerate() { *x = (i as u8).wrapping_mul(37).wrapping_add(11); }
        let v = StairsCreationNetData::read_le(&raw);
        let mut back = vec![0u8; raw.len()];
        v.write_le(&mut back);
        assert_eq!(&back[..], &raw[..]);   // byte-exact incl. padding (floats compared as bytes, NaN-safe)
        assert!(<StairsCreationNetData>::default().to_vec().iter().all(|&x| x == 0));
    }
}

/// size 0x88
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SteamTrainCreationNetData {
    /// TrainCarCreationNetData @ 0x0
    pub train_car_creation_net_data: TrainCarCreationNetData,
    /// int16_t @ 0x68
    pub fuel_fraction: i16,
    /// uint8_t @ 0x6a
    pub has_fuel: u8,
    /// uint8_t @ 0x6b
    pub whistle: u8,
    /// uint8_t @ 0x6c
    pub going_right: u8,
    /// uint8_t @ 0x6d
    pub stopped: u8,
    /// explicit padding at 0x6e; preserved on read, zero by default
    pub padding: [u8; 2],
    /// uint32_t @ 0x70
    pub left_train_pos_x: u32,
    /// uint32_t @ 0x74
    pub left_train_pos_y: u32,
    /// uint32_t @ 0x78
    pub right_train_pos_x: u32,
    /// uint32_t @ 0x7c
    pub right_train_pos_y: u32,
    /// uint32_t @ 0x80
    pub stop_at_goal_pos_x: u32,
    /// uint32_t @ 0x84
    pub stop_at_goal_pos_y: u32,
}
impl WireStruct for SteamTrainCreationNetData {
    const SIZE: usize = 0x88;
    fn write_le(&self, out: &mut [u8]) {
        assert!(out.len() >= Self::SIZE);
        out[..Self::SIZE].fill(0);
        self.train_car_creation_net_data.write_le(&mut out[0x0..]);
        out[0x68..0x6a].copy_from_slice(&self.fuel_fraction.to_le_bytes());
        out[0x6a..0x6b].copy_from_slice(&self.has_fuel.to_le_bytes());
        out[0x6b..0x6c].copy_from_slice(&self.whistle.to_le_bytes());
        out[0x6c..0x6d].copy_from_slice(&self.going_right.to_le_bytes());
        out[0x6d..0x6e].copy_from_slice(&self.stopped.to_le_bytes());
        out[0x6e..0x70].copy_from_slice(&self.padding);
        out[0x70..0x74].copy_from_slice(&self.left_train_pos_x.to_le_bytes());
        out[0x74..0x78].copy_from_slice(&self.left_train_pos_y.to_le_bytes());
        out[0x78..0x7c].copy_from_slice(&self.right_train_pos_x.to_le_bytes());
        out[0x7c..0x80].copy_from_slice(&self.right_train_pos_y.to_le_bytes());
        out[0x80..0x84].copy_from_slice(&self.stop_at_goal_pos_x.to_le_bytes());
        out[0x84..0x88].copy_from_slice(&self.stop_at_goal_pos_y.to_le_bytes());
    }
    fn read_le(b: &[u8]) -> Self {
        assert!(b.len() >= Self::SIZE);
        Self {
            train_car_creation_net_data: TrainCarCreationNetData::read_le(&b[0x0..]),
            fuel_fraction: i16::from_le_bytes(b[0x68..0x6a].try_into().unwrap()),
            has_fuel: u8::from_le_bytes(b[0x6a..0x6b].try_into().unwrap()),
            whistle: u8::from_le_bytes(b[0x6b..0x6c].try_into().unwrap()),
            going_right: u8::from_le_bytes(b[0x6c..0x6d].try_into().unwrap()),
            stopped: u8::from_le_bytes(b[0x6d..0x6e].try_into().unwrap()),
            padding: b[0x6e..0x70].try_into().unwrap(),
            left_train_pos_x: u32::from_le_bytes(b[0x70..0x74].try_into().unwrap()),
            left_train_pos_y: u32::from_le_bytes(b[0x74..0x78].try_into().unwrap()),
            right_train_pos_x: u32::from_le_bytes(b[0x78..0x7c].try_into().unwrap()),
            right_train_pos_y: u32::from_le_bytes(b[0x7c..0x80].try_into().unwrap()),
            stop_at_goal_pos_x: u32::from_le_bytes(b[0x80..0x84].try_into().unwrap()),
            stop_at_goal_pos_y: u32::from_le_bytes(b[0x84..0x88].try_into().unwrap()),
        }
    }
}
impl Default for SteamTrainCreationNetData { fn default() -> Self { Self::read_le(&[0u8; 136]) } }
#[cfg(test)]
mod test_steam_train_creation_net_data {
    use super::*;
    #[test]
    fn layout() {
        assert_eq!(core::mem::size_of::<SteamTrainCreationNetData>(), 0x88);
        assert_eq!(<SteamTrainCreationNetData as WireStruct>::SIZE, 0x88);
        assert_eq!(core::mem::offset_of!(SteamTrainCreationNetData, train_car_creation_net_data), 0x0);
        assert_eq!(core::mem::offset_of!(SteamTrainCreationNetData, fuel_fraction), 0x68);
        assert_eq!(core::mem::offset_of!(SteamTrainCreationNetData, has_fuel), 0x6a);
        assert_eq!(core::mem::offset_of!(SteamTrainCreationNetData, whistle), 0x6b);
        assert_eq!(core::mem::offset_of!(SteamTrainCreationNetData, going_right), 0x6c);
        assert_eq!(core::mem::offset_of!(SteamTrainCreationNetData, stopped), 0x6d);
        assert_eq!(core::mem::offset_of!(SteamTrainCreationNetData, padding), 0x6e);
        assert_eq!(core::mem::offset_of!(SteamTrainCreationNetData, left_train_pos_x), 0x70);
        assert_eq!(core::mem::offset_of!(SteamTrainCreationNetData, left_train_pos_y), 0x74);
        assert_eq!(core::mem::offset_of!(SteamTrainCreationNetData, right_train_pos_x), 0x78);
        assert_eq!(core::mem::offset_of!(SteamTrainCreationNetData, right_train_pos_y), 0x7c);
        assert_eq!(core::mem::offset_of!(SteamTrainCreationNetData, stop_at_goal_pos_x), 0x80);
        assert_eq!(core::mem::offset_of!(SteamTrainCreationNetData, stop_at_goal_pos_y), 0x84);
    }
    #[test]
    fn roundtrip() {
        let mut raw = [0u8; 0x88];
        for (i, x) in raw.iter_mut().enumerate() { *x = (i as u8).wrapping_mul(37).wrapping_add(11); }
        let v = SteamTrainCreationNetData::read_le(&raw);
        let mut back = vec![0u8; raw.len()];
        v.write_le(&mut back);
        assert_eq!(&back[..], &raw[..]);   // byte-exact incl. padding (floats compared as bytes, NaN-safe)
        assert!(<SteamTrainCreationNetData>::default().to_vec().iter().all(|&x| x == 0));
    }
}

/// size 0x20
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TorchCreationNetData {
    /// DynamicObjectNetData @ 0x0
    pub dynamic_object_net_data: DynamicObjectNetData,
    /// uint16_t @ 0x18
    pub item_type: u16,
    /// uint16_t @ 0x1a
    pub data_a: u16,
    /// uint16_t @ 0x1c
    pub data_b: u16,
    /// int8_t @ 0x1e
    pub connection_type: i8,
    /// explicit padding at 0x1f; preserved on read, zero by default
    pub padding: [u8; 1],
}
impl WireStruct for TorchCreationNetData {
    const SIZE: usize = 0x20;
    fn write_le(&self, out: &mut [u8]) {
        assert!(out.len() >= Self::SIZE);
        out[..Self::SIZE].fill(0);
        self.dynamic_object_net_data.write_le(&mut out[0x0..]);
        out[0x18..0x1a].copy_from_slice(&self.item_type.to_le_bytes());
        out[0x1a..0x1c].copy_from_slice(&self.data_a.to_le_bytes());
        out[0x1c..0x1e].copy_from_slice(&self.data_b.to_le_bytes());
        out[0x1e..0x1f].copy_from_slice(&self.connection_type.to_le_bytes());
        out[0x1f..0x20].copy_from_slice(&self.padding);
    }
    fn read_le(b: &[u8]) -> Self {
        assert!(b.len() >= Self::SIZE);
        Self {
            dynamic_object_net_data: DynamicObjectNetData::read_le(&b[0x0..]),
            item_type: u16::from_le_bytes(b[0x18..0x1a].try_into().unwrap()),
            data_a: u16::from_le_bytes(b[0x1a..0x1c].try_into().unwrap()),
            data_b: u16::from_le_bytes(b[0x1c..0x1e].try_into().unwrap()),
            connection_type: i8::from_le_bytes(b[0x1e..0x1f].try_into().unwrap()),
            padding: b[0x1f..0x20].try_into().unwrap(),
        }
    }
}
impl Default for TorchCreationNetData { fn default() -> Self { Self::read_le(&[0u8; 32]) } }
#[cfg(test)]
mod test_torch_creation_net_data {
    use super::*;
    #[test]
    fn layout() {
        assert_eq!(core::mem::size_of::<TorchCreationNetData>(), 0x20);
        assert_eq!(<TorchCreationNetData as WireStruct>::SIZE, 0x20);
        assert_eq!(core::mem::offset_of!(TorchCreationNetData, dynamic_object_net_data), 0x0);
        assert_eq!(core::mem::offset_of!(TorchCreationNetData, item_type), 0x18);
        assert_eq!(core::mem::offset_of!(TorchCreationNetData, data_a), 0x1a);
        assert_eq!(core::mem::offset_of!(TorchCreationNetData, data_b), 0x1c);
        assert_eq!(core::mem::offset_of!(TorchCreationNetData, connection_type), 0x1e);
        assert_eq!(core::mem::offset_of!(TorchCreationNetData, padding), 0x1f);
    }
    #[test]
    fn roundtrip() {
        let mut raw = [0u8; 0x20];
        for (i, x) in raw.iter_mut().enumerate() { *x = (i as u8).wrapping_mul(37).wrapping_add(11); }
        let v = TorchCreationNetData::read_le(&raw);
        let mut back = vec![0u8; raw.len()];
        v.write_le(&mut back);
        assert_eq!(&back[..], &raw[..]);   // byte-exact incl. padding (floats compared as bytes, NaN-safe)
        assert!(<TorchCreationNetData>::default().to_vec().iter().all(|&x| x == 0));
    }
}

/// size 0x30
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TradePortalNetData {
    /// InteractionObjectCreationNetData @ 0x0
    pub interaction_object_creation_net_data: InteractionObjectCreationNetData,
    /// uint8_t @ 0x28
    pub level: u8,
    /// explicit padding at 0x29; preserved on read, zero by default
    pub padding: [u8; 7],
}
impl WireStruct for TradePortalNetData {
    const SIZE: usize = 0x30;
    fn write_le(&self, out: &mut [u8]) {
        assert!(out.len() >= Self::SIZE);
        out[..Self::SIZE].fill(0);
        self.interaction_object_creation_net_data.write_le(&mut out[0x0..]);
        out[0x28..0x29].copy_from_slice(&self.level.to_le_bytes());
        out[0x29..0x30].copy_from_slice(&self.padding);
    }
    fn read_le(b: &[u8]) -> Self {
        assert!(b.len() >= Self::SIZE);
        Self {
            interaction_object_creation_net_data: InteractionObjectCreationNetData::read_le(&b[0x0..]),
            level: u8::from_le_bytes(b[0x28..0x29].try_into().unwrap()),
            padding: b[0x29..0x30].try_into().unwrap(),
        }
    }
}
impl Default for TradePortalNetData { fn default() -> Self { Self::read_le(&[0u8; 48]) } }
#[cfg(test)]
mod test_trade_portal_net_data {
    use super::*;
    #[test]
    fn layout() {
        assert_eq!(core::mem::size_of::<TradePortalNetData>(), 0x30);
        assert_eq!(<TradePortalNetData as WireStruct>::SIZE, 0x30);
        assert_eq!(core::mem::offset_of!(TradePortalNetData, interaction_object_creation_net_data), 0x0);
        assert_eq!(core::mem::offset_of!(TradePortalNetData, level), 0x28);
        assert_eq!(core::mem::offset_of!(TradePortalNetData, padding), 0x29);
    }
    #[test]
    fn roundtrip() {
        let mut raw = [0u8; 0x30];
        for (i, x) in raw.iter_mut().enumerate() { *x = (i as u8).wrapping_mul(37).wrapping_add(11); }
        let v = TradePortalNetData::read_le(&raw);
        let mut back = vec![0u8; raw.len()];
        v.write_le(&mut back);
        assert_eq!(&back[..], &raw[..]);   // byte-exact incl. padding (floats compared as bytes, NaN-safe)
        assert!(<TradePortalNetData>::default().to_vec().iter().all(|&x| x == 0));
    }
}

/// size 0x30
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TradingPostNetData {
    /// InteractionObjectCreationNetData @ 0x0
    pub interaction_object_creation_net_data: InteractionObjectCreationNetData,
    /// uint32_t @ 0x28
    pub coin_count: u32,
    /// uint16_t @ 0x2c
    pub price_tier: u16,
    /// explicit padding at 0x2e; preserved on read, zero by default
    pub padding: [u8; 2],
}
impl WireStruct for TradingPostNetData {
    const SIZE: usize = 0x30;
    fn write_le(&self, out: &mut [u8]) {
        assert!(out.len() >= Self::SIZE);
        out[..Self::SIZE].fill(0);
        self.interaction_object_creation_net_data.write_le(&mut out[0x0..]);
        out[0x28..0x2c].copy_from_slice(&self.coin_count.to_le_bytes());
        out[0x2c..0x2e].copy_from_slice(&self.price_tier.to_le_bytes());
        out[0x2e..0x30].copy_from_slice(&self.padding);
    }
    fn read_le(b: &[u8]) -> Self {
        assert!(b.len() >= Self::SIZE);
        Self {
            interaction_object_creation_net_data: InteractionObjectCreationNetData::read_le(&b[0x0..]),
            coin_count: u32::from_le_bytes(b[0x28..0x2c].try_into().unwrap()),
            price_tier: u16::from_le_bytes(b[0x2c..0x2e].try_into().unwrap()),
            padding: b[0x2e..0x30].try_into().unwrap(),
        }
    }
}
impl Default for TradingPostNetData { fn default() -> Self { Self::read_le(&[0u8; 48]) } }
#[cfg(test)]
mod test_trading_post_net_data {
    use super::*;
    #[test]
    fn layout() {
        assert_eq!(core::mem::size_of::<TradingPostNetData>(), 0x30);
        assert_eq!(<TradingPostNetData as WireStruct>::SIZE, 0x30);
        assert_eq!(core::mem::offset_of!(TradingPostNetData, interaction_object_creation_net_data), 0x0);
        assert_eq!(core::mem::offset_of!(TradingPostNetData, coin_count), 0x28);
        assert_eq!(core::mem::offset_of!(TradingPostNetData, price_tier), 0x2c);
        assert_eq!(core::mem::offset_of!(TradingPostNetData, padding), 0x2e);
    }
    #[test]
    fn roundtrip() {
        let mut raw = [0u8; 0x30];
        for (i, x) in raw.iter_mut().enumerate() { *x = (i as u8).wrapping_mul(37).wrapping_add(11); }
        let v = TradingPostNetData::read_le(&raw);
        let mut back = vec![0u8; raw.len()];
        v.write_le(&mut back);
        assert_eq!(&back[..], &raw[..]);   // byte-exact incl. padding (floats compared as bytes, NaN-safe)
        assert!(<TradingPostNetData>::default().to_vec().iter().all(|&x| x == 0));
    }
}

/// size 0x68
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TrainCarCreationNetData {
    /// DynamicObjectNetData @ 0x0
    pub dynamic_object_net_data: DynamicObjectNetData,
    /// uint64_t @ 0x18
    pub rider0_id: u64,
    /// uint64_t @ 0x20
    pub rider1_id: u64,
    /// uint64_t @ 0x28
    pub left_car_id: u64,
    /// uint64_t @ 0x30
    pub right_car_id: u64,
    /// uint64_t @ 0x38
    pub engine_car_id: u64,
    /// Float32 @ 0x40
    pub left_wheel_pos_x: f32,
    /// Float32 @ 0x44
    pub left_wheel_pos_y: f32,
    /// Float32 @ 0x48
    pub left_wheel_vel_x: f32,
    /// Float32 @ 0x4c
    pub left_wheel_vel_y: f32,
    /// Float32 @ 0x50
    pub right_wheel_pos_x: f32,
    /// Float32 @ 0x54
    pub right_wheel_pos_y: f32,
    /// Float32 @ 0x58
    pub right_wheel_vel_x: f32,
    /// Float32 @ 0x5c
    pub right_wheel_vel_y: f32,
    /// Float32 @ 0x60
    pub visual_velocity_x: f32,
    /// uint8_t @ 0x64
    pub engine_is_right: u8,
    /// explicit padding at 0x65; preserved on read, zero by default
    pub padding: [u8; 3],
}
impl WireStruct for TrainCarCreationNetData {
    const SIZE: usize = 0x68;
    fn write_le(&self, out: &mut [u8]) {
        assert!(out.len() >= Self::SIZE);
        out[..Self::SIZE].fill(0);
        self.dynamic_object_net_data.write_le(&mut out[0x0..]);
        out[0x18..0x20].copy_from_slice(&self.rider0_id.to_le_bytes());
        out[0x20..0x28].copy_from_slice(&self.rider1_id.to_le_bytes());
        out[0x28..0x30].copy_from_slice(&self.left_car_id.to_le_bytes());
        out[0x30..0x38].copy_from_slice(&self.right_car_id.to_le_bytes());
        out[0x38..0x40].copy_from_slice(&self.engine_car_id.to_le_bytes());
        out[0x40..0x44].copy_from_slice(&self.left_wheel_pos_x.to_le_bytes());
        out[0x44..0x48].copy_from_slice(&self.left_wheel_pos_y.to_le_bytes());
        out[0x48..0x4c].copy_from_slice(&self.left_wheel_vel_x.to_le_bytes());
        out[0x4c..0x50].copy_from_slice(&self.left_wheel_vel_y.to_le_bytes());
        out[0x50..0x54].copy_from_slice(&self.right_wheel_pos_x.to_le_bytes());
        out[0x54..0x58].copy_from_slice(&self.right_wheel_pos_y.to_le_bytes());
        out[0x58..0x5c].copy_from_slice(&self.right_wheel_vel_x.to_le_bytes());
        out[0x5c..0x60].copy_from_slice(&self.right_wheel_vel_y.to_le_bytes());
        out[0x60..0x64].copy_from_slice(&self.visual_velocity_x.to_le_bytes());
        out[0x64..0x65].copy_from_slice(&self.engine_is_right.to_le_bytes());
        out[0x65..0x68].copy_from_slice(&self.padding);
    }
    fn read_le(b: &[u8]) -> Self {
        assert!(b.len() >= Self::SIZE);
        Self {
            dynamic_object_net_data: DynamicObjectNetData::read_le(&b[0x0..]),
            rider0_id: u64::from_le_bytes(b[0x18..0x20].try_into().unwrap()),
            rider1_id: u64::from_le_bytes(b[0x20..0x28].try_into().unwrap()),
            left_car_id: u64::from_le_bytes(b[0x28..0x30].try_into().unwrap()),
            right_car_id: u64::from_le_bytes(b[0x30..0x38].try_into().unwrap()),
            engine_car_id: u64::from_le_bytes(b[0x38..0x40].try_into().unwrap()),
            left_wheel_pos_x: f32::from_le_bytes(b[0x40..0x44].try_into().unwrap()),
            left_wheel_pos_y: f32::from_le_bytes(b[0x44..0x48].try_into().unwrap()),
            left_wheel_vel_x: f32::from_le_bytes(b[0x48..0x4c].try_into().unwrap()),
            left_wheel_vel_y: f32::from_le_bytes(b[0x4c..0x50].try_into().unwrap()),
            right_wheel_pos_x: f32::from_le_bytes(b[0x50..0x54].try_into().unwrap()),
            right_wheel_pos_y: f32::from_le_bytes(b[0x54..0x58].try_into().unwrap()),
            right_wheel_vel_x: f32::from_le_bytes(b[0x58..0x5c].try_into().unwrap()),
            right_wheel_vel_y: f32::from_le_bytes(b[0x5c..0x60].try_into().unwrap()),
            visual_velocity_x: f32::from_le_bytes(b[0x60..0x64].try_into().unwrap()),
            engine_is_right: u8::from_le_bytes(b[0x64..0x65].try_into().unwrap()),
            padding: b[0x65..0x68].try_into().unwrap(),
        }
    }
}
impl Default for TrainCarCreationNetData { fn default() -> Self { Self::read_le(&[0u8; 104]) } }
#[cfg(test)]
mod test_train_car_creation_net_data {
    use super::*;
    #[test]
    fn layout() {
        assert_eq!(core::mem::size_of::<TrainCarCreationNetData>(), 0x68);
        assert_eq!(<TrainCarCreationNetData as WireStruct>::SIZE, 0x68);
        assert_eq!(core::mem::offset_of!(TrainCarCreationNetData, dynamic_object_net_data), 0x0);
        assert_eq!(core::mem::offset_of!(TrainCarCreationNetData, rider0_id), 0x18);
        assert_eq!(core::mem::offset_of!(TrainCarCreationNetData, rider1_id), 0x20);
        assert_eq!(core::mem::offset_of!(TrainCarCreationNetData, left_car_id), 0x28);
        assert_eq!(core::mem::offset_of!(TrainCarCreationNetData, right_car_id), 0x30);
        assert_eq!(core::mem::offset_of!(TrainCarCreationNetData, engine_car_id), 0x38);
        assert_eq!(core::mem::offset_of!(TrainCarCreationNetData, left_wheel_pos_x), 0x40);
        assert_eq!(core::mem::offset_of!(TrainCarCreationNetData, left_wheel_pos_y), 0x44);
        assert_eq!(core::mem::offset_of!(TrainCarCreationNetData, left_wheel_vel_x), 0x48);
        assert_eq!(core::mem::offset_of!(TrainCarCreationNetData, left_wheel_vel_y), 0x4c);
        assert_eq!(core::mem::offset_of!(TrainCarCreationNetData, right_wheel_pos_x), 0x50);
        assert_eq!(core::mem::offset_of!(TrainCarCreationNetData, right_wheel_pos_y), 0x54);
        assert_eq!(core::mem::offset_of!(TrainCarCreationNetData, right_wheel_vel_x), 0x58);
        assert_eq!(core::mem::offset_of!(TrainCarCreationNetData, right_wheel_vel_y), 0x5c);
        assert_eq!(core::mem::offset_of!(TrainCarCreationNetData, visual_velocity_x), 0x60);
        assert_eq!(core::mem::offset_of!(TrainCarCreationNetData, engine_is_right), 0x64);
        assert_eq!(core::mem::offset_of!(TrainCarCreationNetData, padding), 0x65);
    }
    #[test]
    fn roundtrip() {
        let mut raw = [0u8; 0x68];
        for (i, x) in raw.iter_mut().enumerate() { *x = (i as u8).wrapping_mul(37).wrapping_add(11); }
        let v = TrainCarCreationNetData::read_le(&raw);
        let mut back = vec![0u8; raw.len()];
        v.write_le(&mut back);
        assert_eq!(&back[..], &raw[..]);   // byte-exact incl. padding (floats compared as bytes, NaN-safe)
        assert!(<TrainCarCreationNetData>::default().to_vec().iter().all(|&x| x == 0));
    }
}

/// size 0x28
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TrainStationCreationNetData {
    /// InteractionObjectCreationNetData @ 0x0
    pub interaction_object_creation_net_data: InteractionObjectCreationNetData,
}
impl WireStruct for TrainStationCreationNetData {
    const SIZE: usize = 0x28;
    fn write_le(&self, out: &mut [u8]) {
        assert!(out.len() >= Self::SIZE);
        out[..Self::SIZE].fill(0);
        self.interaction_object_creation_net_data.write_le(&mut out[0x0..]);
    }
    fn read_le(b: &[u8]) -> Self {
        assert!(b.len() >= Self::SIZE);
        Self {
            interaction_object_creation_net_data: InteractionObjectCreationNetData::read_le(&b[0x0..]),
        }
    }
}
impl Default for TrainStationCreationNetData { fn default() -> Self { Self::read_le(&[0u8; 40]) } }
#[cfg(test)]
mod test_train_station_creation_net_data {
    use super::*;
    #[test]
    fn layout() {
        assert_eq!(core::mem::size_of::<TrainStationCreationNetData>(), 0x28);
        assert_eq!(<TrainStationCreationNetData as WireStruct>::SIZE, 0x28);
        assert_eq!(core::mem::offset_of!(TrainStationCreationNetData, interaction_object_creation_net_data), 0x0);
    }
    #[test]
    fn roundtrip() {
        let mut raw = [0u8; 0x28];
        for (i, x) in raw.iter_mut().enumerate() { *x = (i as u8).wrapping_mul(37).wrapping_add(11); }
        let v = TrainStationCreationNetData::read_le(&raw);
        let mut back = vec![0u8; raw.len()];
        v.write_le(&mut back);
        assert_eq!(&back[..], &raw[..]);   // byte-exact incl. padding (floats compared as bytes, NaN-safe)
        assert!(<TrainStationCreationNetData>::default().to_vec().iter().all(|&x| x == 0));
    }
}

/// size 0x20
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct WindowCreationNetData {
    /// DynamicObjectNetData @ 0x0
    pub dynamic_object_net_data: DynamicObjectNetData,
    /// uint16_t @ 0x18
    pub item_type: u16,
    /// explicit padding at 0x1a; preserved on read, zero by default
    pub padding: [u8; 6],
}
impl WireStruct for WindowCreationNetData {
    const SIZE: usize = 0x20;
    fn write_le(&self, out: &mut [u8]) {
        assert!(out.len() >= Self::SIZE);
        out[..Self::SIZE].fill(0);
        self.dynamic_object_net_data.write_le(&mut out[0x0..]);
        out[0x18..0x1a].copy_from_slice(&self.item_type.to_le_bytes());
        out[0x1a..0x20].copy_from_slice(&self.padding);
    }
    fn read_le(b: &[u8]) -> Self {
        assert!(b.len() >= Self::SIZE);
        Self {
            dynamic_object_net_data: DynamicObjectNetData::read_le(&b[0x0..]),
            item_type: u16::from_le_bytes(b[0x18..0x1a].try_into().unwrap()),
            padding: b[0x1a..0x20].try_into().unwrap(),
        }
    }
}
impl Default for WindowCreationNetData { fn default() -> Self { Self::read_le(&[0u8; 32]) } }
#[cfg(test)]
mod test_window_creation_net_data {
    use super::*;
    #[test]
    fn layout() {
        assert_eq!(core::mem::size_of::<WindowCreationNetData>(), 0x20);
        assert_eq!(<WindowCreationNetData as WireStruct>::SIZE, 0x20);
        assert_eq!(core::mem::offset_of!(WindowCreationNetData, dynamic_object_net_data), 0x0);
        assert_eq!(core::mem::offset_of!(WindowCreationNetData, item_type), 0x18);
        assert_eq!(core::mem::offset_of!(WindowCreationNetData, padding), 0x1a);
    }
    #[test]
    fn roundtrip() {
        let mut raw = [0u8; 0x20];
        for (i, x) in raw.iter_mut().enumerate() { *x = (i as u8).wrapping_mul(37).wrapping_add(11); }
        let v = WindowCreationNetData::read_le(&raw);
        let mut back = vec![0u8; raw.len()];
        v.write_le(&mut back);
        assert_eq!(&back[..], &raw[..]);   // byte-exact incl. padding (floats compared as bytes, NaN-safe)
        assert!(<WindowCreationNetData>::default().to_vec().iter().all(|&x| x == 0));
    }
}

/// size 0x20
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct WireCreationNetData {
    /// DynamicObjectNetData @ 0x0
    pub dynamic_object_net_data: DynamicObjectNetData,
    /// uint16_t @ 0x18
    pub item_type: u16,
    /// uint8_t @ 0x1a
    pub configuration: u8,
    /// uint8_t @ 0x1b
    pub solid_configuration: u8,
    /// explicit padding at 0x1c; preserved on read, zero by default
    pub padding: [u8; 4],
}
impl WireStruct for WireCreationNetData {
    const SIZE: usize = 0x20;
    fn write_le(&self, out: &mut [u8]) {
        assert!(out.len() >= Self::SIZE);
        out[..Self::SIZE].fill(0);
        self.dynamic_object_net_data.write_le(&mut out[0x0..]);
        out[0x18..0x1a].copy_from_slice(&self.item_type.to_le_bytes());
        out[0x1a..0x1b].copy_from_slice(&self.configuration.to_le_bytes());
        out[0x1b..0x1c].copy_from_slice(&self.solid_configuration.to_le_bytes());
        out[0x1c..0x20].copy_from_slice(&self.padding);
    }
    fn read_le(b: &[u8]) -> Self {
        assert!(b.len() >= Self::SIZE);
        Self {
            dynamic_object_net_data: DynamicObjectNetData::read_le(&b[0x0..]),
            item_type: u16::from_le_bytes(b[0x18..0x1a].try_into().unwrap()),
            configuration: u8::from_le_bytes(b[0x1a..0x1b].try_into().unwrap()),
            solid_configuration: u8::from_le_bytes(b[0x1b..0x1c].try_into().unwrap()),
            padding: b[0x1c..0x20].try_into().unwrap(),
        }
    }
}
impl Default for WireCreationNetData { fn default() -> Self { Self::read_le(&[0u8; 32]) } }
#[cfg(test)]
mod test_wire_creation_net_data {
    use super::*;
    #[test]
    fn layout() {
        assert_eq!(core::mem::size_of::<WireCreationNetData>(), 0x20);
        assert_eq!(<WireCreationNetData as WireStruct>::SIZE, 0x20);
        assert_eq!(core::mem::offset_of!(WireCreationNetData, dynamic_object_net_data), 0x0);
        assert_eq!(core::mem::offset_of!(WireCreationNetData, item_type), 0x18);
        assert_eq!(core::mem::offset_of!(WireCreationNetData, configuration), 0x1a);
        assert_eq!(core::mem::offset_of!(WireCreationNetData, solid_configuration), 0x1b);
        assert_eq!(core::mem::offset_of!(WireCreationNetData, padding), 0x1c);
    }
    #[test]
    fn roundtrip() {
        let mut raw = [0u8; 0x20];
        for (i, x) in raw.iter_mut().enumerate() { *x = (i as u8).wrapping_mul(37).wrapping_add(11); }
        let v = WireCreationNetData::read_le(&raw);
        let mut back = vec![0u8; raw.len()];
        v.write_le(&mut back);
        assert_eq!(&back[..], &raw[..]);   // byte-exact incl. padding (floats compared as bytes, NaN-safe)
        assert!(<WireCreationNetData>::default().to_vec().iter().all(|&x| x == 0));
    }
}

/// size 0x38
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct WorkbenchNetData {
    /// InteractionObjectCreationNetData @ 0x0
    pub interaction_object_creation_net_data: InteractionObjectCreationNetData,
    /// uint64_t @ 0x28
    pub is_in_use_fuel_blockhead_unique_id: u64,
    /// int16_t @ 0x30
    pub fuel_fraction: i16,
    /// uint8_t @ 0x32
    pub type_: u8,
    /// uint8_t @ 0x33
    pub level: u8,
    /// uint8_t @ 0x34
    pub has_fuel: u8,
    /// uint8_t @ 0x35
    pub is_in_use_fuel: u8,
    /// uint16_t @ 0x36
    pub available_electricity: u16,
}
impl WireStruct for WorkbenchNetData {
    const SIZE: usize = 0x38;
    fn write_le(&self, out: &mut [u8]) {
        assert!(out.len() >= Self::SIZE);
        out[..Self::SIZE].fill(0);
        self.interaction_object_creation_net_data.write_le(&mut out[0x0..]);
        out[0x28..0x30].copy_from_slice(&self.is_in_use_fuel_blockhead_unique_id.to_le_bytes());
        out[0x30..0x32].copy_from_slice(&self.fuel_fraction.to_le_bytes());
        out[0x32..0x33].copy_from_slice(&self.type_.to_le_bytes());
        out[0x33..0x34].copy_from_slice(&self.level.to_le_bytes());
        out[0x34..0x35].copy_from_slice(&self.has_fuel.to_le_bytes());
        out[0x35..0x36].copy_from_slice(&self.is_in_use_fuel.to_le_bytes());
        out[0x36..0x38].copy_from_slice(&self.available_electricity.to_le_bytes());
    }
    fn read_le(b: &[u8]) -> Self {
        assert!(b.len() >= Self::SIZE);
        Self {
            interaction_object_creation_net_data: InteractionObjectCreationNetData::read_le(&b[0x0..]),
            is_in_use_fuel_blockhead_unique_id: u64::from_le_bytes(b[0x28..0x30].try_into().unwrap()),
            fuel_fraction: i16::from_le_bytes(b[0x30..0x32].try_into().unwrap()),
            type_: u8::from_le_bytes(b[0x32..0x33].try_into().unwrap()),
            level: u8::from_le_bytes(b[0x33..0x34].try_into().unwrap()),
            has_fuel: u8::from_le_bytes(b[0x34..0x35].try_into().unwrap()),
            is_in_use_fuel: u8::from_le_bytes(b[0x35..0x36].try_into().unwrap()),
            available_electricity: u16::from_le_bytes(b[0x36..0x38].try_into().unwrap()),
        }
    }
}
impl Default for WorkbenchNetData { fn default() -> Self { Self::read_le(&[0u8; 56]) } }
#[cfg(test)]
mod test_workbench_net_data {
    use super::*;
    #[test]
    fn layout() {
        assert_eq!(core::mem::size_of::<WorkbenchNetData>(), 0x38);
        assert_eq!(<WorkbenchNetData as WireStruct>::SIZE, 0x38);
        assert_eq!(core::mem::offset_of!(WorkbenchNetData, interaction_object_creation_net_data), 0x0);
        assert_eq!(core::mem::offset_of!(WorkbenchNetData, is_in_use_fuel_blockhead_unique_id), 0x28);
        assert_eq!(core::mem::offset_of!(WorkbenchNetData, fuel_fraction), 0x30);
        assert_eq!(core::mem::offset_of!(WorkbenchNetData, type_), 0x32);
        assert_eq!(core::mem::offset_of!(WorkbenchNetData, level), 0x33);
        assert_eq!(core::mem::offset_of!(WorkbenchNetData, has_fuel), 0x34);
        assert_eq!(core::mem::offset_of!(WorkbenchNetData, is_in_use_fuel), 0x35);
        assert_eq!(core::mem::offset_of!(WorkbenchNetData, available_electricity), 0x36);
    }
    #[test]
    fn roundtrip() {
        let mut raw = [0u8; 0x38];
        for (i, x) in raw.iter_mut().enumerate() { *x = (i as u8).wrapping_mul(37).wrapping_add(11); }
        let v = WorkbenchNetData::read_le(&raw);
        let mut back = vec![0u8; raw.len()];
        v.write_le(&mut back);
        assert_eq!(&back[..], &raw[..]);   // byte-exact incl. padding (floats compared as bytes, NaN-safe)
        assert!(<WorkbenchNetData>::default().to_vec().iter().all(|&x| x == 0));
    }
}

/// size 0x10
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FillNetRequest {
    /// uint32_t @ 0x0
    pub pos_x: u32,
    /// uint32_t @ 0x4
    pub pos_y: u32,
    /// uint16_t @ 0x8
    pub type_: u16,
    /// uint16_t @ 0xa
    pub data_a: u16,
    /// uint16_t @ 0xc
    pub data_b: u16,
    /// uint8_t @ 0xe
    pub z_index: u8,
    /// explicit padding at 0xf; preserved on read, zero by default
    pub padding: [u8; 1],
}
impl WireStruct for FillNetRequest {
    const SIZE: usize = 0x10;
    fn write_le(&self, out: &mut [u8]) {
        assert!(out.len() >= Self::SIZE);
        out[..Self::SIZE].fill(0);
        out[0x0..0x4].copy_from_slice(&self.pos_x.to_le_bytes());
        out[0x4..0x8].copy_from_slice(&self.pos_y.to_le_bytes());
        out[0x8..0xa].copy_from_slice(&self.type_.to_le_bytes());
        out[0xa..0xc].copy_from_slice(&self.data_a.to_le_bytes());
        out[0xc..0xe].copy_from_slice(&self.data_b.to_le_bytes());
        out[0xe..0xf].copy_from_slice(&self.z_index.to_le_bytes());
        out[0xf..0x10].copy_from_slice(&self.padding);
    }
    fn read_le(b: &[u8]) -> Self {
        assert!(b.len() >= Self::SIZE);
        Self {
            pos_x: u32::from_le_bytes(b[0x0..0x4].try_into().unwrap()),
            pos_y: u32::from_le_bytes(b[0x4..0x8].try_into().unwrap()),
            type_: u16::from_le_bytes(b[0x8..0xa].try_into().unwrap()),
            data_a: u16::from_le_bytes(b[0xa..0xc].try_into().unwrap()),
            data_b: u16::from_le_bytes(b[0xc..0xe].try_into().unwrap()),
            z_index: u8::from_le_bytes(b[0xe..0xf].try_into().unwrap()),
            padding: b[0xf..0x10].try_into().unwrap(),
        }
    }
}
impl Default for FillNetRequest { fn default() -> Self { Self::read_le(&[0u8; 16]) } }
#[cfg(test)]
mod test_fill_net_request {
    use super::*;
    #[test]
    fn layout() {
        assert_eq!(core::mem::size_of::<FillNetRequest>(), 0x10);
        assert_eq!(<FillNetRequest as WireStruct>::SIZE, 0x10);
        assert_eq!(core::mem::offset_of!(FillNetRequest, pos_x), 0x0);
        assert_eq!(core::mem::offset_of!(FillNetRequest, pos_y), 0x4);
        assert_eq!(core::mem::offset_of!(FillNetRequest, type_), 0x8);
        assert_eq!(core::mem::offset_of!(FillNetRequest, data_a), 0xa);
        assert_eq!(core::mem::offset_of!(FillNetRequest, data_b), 0xc);
        assert_eq!(core::mem::offset_of!(FillNetRequest, z_index), 0xe);
        assert_eq!(core::mem::offset_of!(FillNetRequest, padding), 0xf);
    }
    #[test]
    fn roundtrip() {
        let mut raw = [0u8; 0x10];
        for (i, x) in raw.iter_mut().enumerate() { *x = (i as u8).wrapping_mul(37).wrapping_add(11); }
        let v = FillNetRequest::read_le(&raw);
        let mut back = vec![0u8; raw.len()];
        v.write_le(&mut back);
        assert_eq!(&back[..], &raw[..]);   // byte-exact incl. padding (floats compared as bytes, NaN-safe)
        assert!(<FillNetRequest>::default().to_vec().iter().all(|&x| x == 0));
    }
}

/// size 0x18
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PaintNetRequest {
    /// uint64_t @ 0x0
    pub paint_blockhead_id: u64,
    /// uint32_t @ 0x8
    pub pos_x: u32,
    /// uint32_t @ 0xc
    pub pos_y: u32,
    /// uint16_t @ 0x10
    pub color_index: u16,
    /// uint8_t @ 0x12
    pub face_index: u8,
    /// explicit padding at 0x13; preserved on read, zero by default
    pub padding: [u8; 5],
}
impl WireStruct for PaintNetRequest {
    const SIZE: usize = 0x18;
    fn write_le(&self, out: &mut [u8]) {
        assert!(out.len() >= Self::SIZE);
        out[..Self::SIZE].fill(0);
        out[0x0..0x8].copy_from_slice(&self.paint_blockhead_id.to_le_bytes());
        out[0x8..0xc].copy_from_slice(&self.pos_x.to_le_bytes());
        out[0xc..0x10].copy_from_slice(&self.pos_y.to_le_bytes());
        out[0x10..0x12].copy_from_slice(&self.color_index.to_le_bytes());
        out[0x12..0x13].copy_from_slice(&self.face_index.to_le_bytes());
        out[0x13..0x18].copy_from_slice(&self.padding);
    }
    fn read_le(b: &[u8]) -> Self {
        assert!(b.len() >= Self::SIZE);
        Self {
            paint_blockhead_id: u64::from_le_bytes(b[0x0..0x8].try_into().unwrap()),
            pos_x: u32::from_le_bytes(b[0x8..0xc].try_into().unwrap()),
            pos_y: u32::from_le_bytes(b[0xc..0x10].try_into().unwrap()),
            color_index: u16::from_le_bytes(b[0x10..0x12].try_into().unwrap()),
            face_index: u8::from_le_bytes(b[0x12..0x13].try_into().unwrap()),
            padding: b[0x13..0x18].try_into().unwrap(),
        }
    }
}
impl Default for PaintNetRequest { fn default() -> Self { Self::read_le(&[0u8; 24]) } }
#[cfg(test)]
mod test_paint_net_request {
    use super::*;
    #[test]
    fn layout() {
        assert_eq!(core::mem::size_of::<PaintNetRequest>(), 0x18);
        assert_eq!(<PaintNetRequest as WireStruct>::SIZE, 0x18);
        assert_eq!(core::mem::offset_of!(PaintNetRequest, paint_blockhead_id), 0x0);
        assert_eq!(core::mem::offset_of!(PaintNetRequest, pos_x), 0x8);
        assert_eq!(core::mem::offset_of!(PaintNetRequest, pos_y), 0xc);
        assert_eq!(core::mem::offset_of!(PaintNetRequest, color_index), 0x10);
        assert_eq!(core::mem::offset_of!(PaintNetRequest, face_index), 0x12);
        assert_eq!(core::mem::offset_of!(PaintNetRequest, padding), 0x13);
    }
    #[test]
    fn roundtrip() {
        let mut raw = [0u8; 0x18];
        for (i, x) in raw.iter_mut().enumerate() { *x = (i as u8).wrapping_mul(37).wrapping_add(11); }
        let v = PaintNetRequest::read_le(&raw);
        let mut back = vec![0u8; raw.len()];
        v.write_le(&mut back);
        assert_eq!(&back[..], &raw[..]);   // byte-exact incl. padding (floats compared as bytes, NaN-safe)
        assert!(<PaintNetRequest>::default().to_vec().iter().all(|&x| x == 0));
    }
}

/// size 0x18
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct RemoveNetRequest {
    /// uint64_t @ 0x0
    pub remove_blockhead_id: u64,
    /// uint32_t @ 0x8
    pub pos_x: u32,
    /// uint32_t @ 0xc
    pub pos_y: u32,
    /// uint8_t @ 0x10
    pub create_contents_freeblock_count: u8,
    /// uint8_t @ 0x11
    pub create_foreground_contents_freeblock_count: u8,
    /// uint8_t @ 0x12
    pub is_water_removal: u8,
    /// uint8_t @ 0x13
    pub only_remove_contents: u8,
    /// uint8_t @ 0x14
    pub only_remove_foreground_contents: u8,
    /// uint8_t @ 0x15
    pub z_index: u8,
    /// explicit padding at 0x16; preserved on read, zero by default
    pub padding: [u8; 2],
}
impl WireStruct for RemoveNetRequest {
    const SIZE: usize = 0x18;
    fn write_le(&self, out: &mut [u8]) {
        assert!(out.len() >= Self::SIZE);
        out[..Self::SIZE].fill(0);
        out[0x0..0x8].copy_from_slice(&self.remove_blockhead_id.to_le_bytes());
        out[0x8..0xc].copy_from_slice(&self.pos_x.to_le_bytes());
        out[0xc..0x10].copy_from_slice(&self.pos_y.to_le_bytes());
        out[0x10..0x11].copy_from_slice(&self.create_contents_freeblock_count.to_le_bytes());
        out[0x11..0x12].copy_from_slice(&self.create_foreground_contents_freeblock_count.to_le_bytes());
        out[0x12..0x13].copy_from_slice(&self.is_water_removal.to_le_bytes());
        out[0x13..0x14].copy_from_slice(&self.only_remove_contents.to_le_bytes());
        out[0x14..0x15].copy_from_slice(&self.only_remove_foreground_contents.to_le_bytes());
        out[0x15..0x16].copy_from_slice(&self.z_index.to_le_bytes());
        out[0x16..0x18].copy_from_slice(&self.padding);
    }
    fn read_le(b: &[u8]) -> Self {
        assert!(b.len() >= Self::SIZE);
        Self {
            remove_blockhead_id: u64::from_le_bytes(b[0x0..0x8].try_into().unwrap()),
            pos_x: u32::from_le_bytes(b[0x8..0xc].try_into().unwrap()),
            pos_y: u32::from_le_bytes(b[0xc..0x10].try_into().unwrap()),
            create_contents_freeblock_count: u8::from_le_bytes(b[0x10..0x11].try_into().unwrap()),
            create_foreground_contents_freeblock_count: u8::from_le_bytes(b[0x11..0x12].try_into().unwrap()),
            is_water_removal: u8::from_le_bytes(b[0x12..0x13].try_into().unwrap()),
            only_remove_contents: u8::from_le_bytes(b[0x13..0x14].try_into().unwrap()),
            only_remove_foreground_contents: u8::from_le_bytes(b[0x14..0x15].try_into().unwrap()),
            z_index: u8::from_le_bytes(b[0x15..0x16].try_into().unwrap()),
            padding: b[0x16..0x18].try_into().unwrap(),
        }
    }
}
impl Default for RemoveNetRequest { fn default() -> Self { Self::read_le(&[0u8; 24]) } }
#[cfg(test)]
mod test_remove_net_request {
    use super::*;
    #[test]
    fn layout() {
        assert_eq!(core::mem::size_of::<RemoveNetRequest>(), 0x18);
        assert_eq!(<RemoveNetRequest as WireStruct>::SIZE, 0x18);
        assert_eq!(core::mem::offset_of!(RemoveNetRequest, remove_blockhead_id), 0x0);
        assert_eq!(core::mem::offset_of!(RemoveNetRequest, pos_x), 0x8);
        assert_eq!(core::mem::offset_of!(RemoveNetRequest, pos_y), 0xc);
        assert_eq!(core::mem::offset_of!(RemoveNetRequest, create_contents_freeblock_count), 0x10);
        assert_eq!(core::mem::offset_of!(RemoveNetRequest, create_foreground_contents_freeblock_count), 0x11);
        assert_eq!(core::mem::offset_of!(RemoveNetRequest, is_water_removal), 0x12);
        assert_eq!(core::mem::offset_of!(RemoveNetRequest, only_remove_contents), 0x13);
        assert_eq!(core::mem::offset_of!(RemoveNetRequest, only_remove_foreground_contents), 0x14);
        assert_eq!(core::mem::offset_of!(RemoveNetRequest, z_index), 0x15);
        assert_eq!(core::mem::offset_of!(RemoveNetRequest, padding), 0x16);
    }
    #[test]
    fn roundtrip() {
        let mut raw = [0u8; 0x18];
        for (i, x) in raw.iter_mut().enumerate() { *x = (i as u8).wrapping_mul(37).wrapping_add(11); }
        let v = RemoveNetRequest::read_le(&raw);
        let mut back = vec![0u8; raw.len()];
        v.write_le(&mut back);
        assert_eq!(&back[..], &raw[..]);   // byte-exact incl. padding (floats compared as bytes, NaN-safe)
        assert!(<RemoveNetRequest>::default().to_vec().iter().all(|&x| x == 0));
    }
}

/// size 0x10
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct RemoveBackWallNetRequest {
    /// uint64_t @ 0x0
    pub remove_blockhead_id: u64,
    /// uint32_t @ 0x8
    pub pos_x: u32,
    /// uint32_t @ 0xc
    pub pos_y: u32,
}
impl WireStruct for RemoveBackWallNetRequest {
    const SIZE: usize = 0x10;
    fn write_le(&self, out: &mut [u8]) {
        assert!(out.len() >= Self::SIZE);
        out[..Self::SIZE].fill(0);
        out[0x0..0x8].copy_from_slice(&self.remove_blockhead_id.to_le_bytes());
        out[0x8..0xc].copy_from_slice(&self.pos_x.to_le_bytes());
        out[0xc..0x10].copy_from_slice(&self.pos_y.to_le_bytes());
    }
    fn read_le(b: &[u8]) -> Self {
        assert!(b.len() >= Self::SIZE);
        Self {
            remove_blockhead_id: u64::from_le_bytes(b[0x0..0x8].try_into().unwrap()),
            pos_x: u32::from_le_bytes(b[0x8..0xc].try_into().unwrap()),
            pos_y: u32::from_le_bytes(b[0xc..0x10].try_into().unwrap()),
        }
    }
}
impl Default for RemoveBackWallNetRequest { fn default() -> Self { Self::read_le(&[0u8; 16]) } }
#[cfg(test)]
mod test_remove_back_wall_net_request {
    use super::*;
    #[test]
    fn layout() {
        assert_eq!(core::mem::size_of::<RemoveBackWallNetRequest>(), 0x10);
        assert_eq!(<RemoveBackWallNetRequest as WireStruct>::SIZE, 0x10);
        assert_eq!(core::mem::offset_of!(RemoveBackWallNetRequest, remove_blockhead_id), 0x0);
        assert_eq!(core::mem::offset_of!(RemoveBackWallNetRequest, pos_x), 0x8);
        assert_eq!(core::mem::offset_of!(RemoveBackWallNetRequest, pos_y), 0xc);
    }
    #[test]
    fn roundtrip() {
        let mut raw = [0u8; 0x10];
        for (i, x) in raw.iter_mut().enumerate() { *x = (i as u8).wrapping_mul(37).wrapping_add(11); }
        let v = RemoveBackWallNetRequest::read_le(&raw);
        let mut back = vec![0u8; raw.len()];
        v.write_le(&mut back);
        assert_eq!(&back[..], &raw[..]);   // byte-exact incl. padding (floats compared as bytes, NaN-safe)
        assert!(<RemoveBackWallNetRequest>::default().to_vec().iter().all(|&x| x == 0));
    }
}

/// size 0x10
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PlaceWorkbenchNetRequest {
    /// uint32_t @ 0x0
    pub pos_x: u32,
    /// uint32_t @ 0x4
    pub pos_y: u32,
    /// uint8_t @ 0x8
    pub workbench_type: u8,
    /// explicit padding at 0x9; preserved on read, zero by default
    pub padding: [u8; 7],
}
impl WireStruct for PlaceWorkbenchNetRequest {
    const SIZE: usize = 0x10;
    fn write_le(&self, out: &mut [u8]) {
        assert!(out.len() >= Self::SIZE);
        out[..Self::SIZE].fill(0);
        out[0x0..0x4].copy_from_slice(&self.pos_x.to_le_bytes());
        out[0x4..0x8].copy_from_slice(&self.pos_y.to_le_bytes());
        out[0x8..0x9].copy_from_slice(&self.workbench_type.to_le_bytes());
        out[0x9..0x10].copy_from_slice(&self.padding);
    }
    fn read_le(b: &[u8]) -> Self {
        assert!(b.len() >= Self::SIZE);
        Self {
            pos_x: u32::from_le_bytes(b[0x0..0x4].try_into().unwrap()),
            pos_y: u32::from_le_bytes(b[0x4..0x8].try_into().unwrap()),
            workbench_type: u8::from_le_bytes(b[0x8..0x9].try_into().unwrap()),
            padding: b[0x9..0x10].try_into().unwrap(),
        }
    }
}
impl Default for PlaceWorkbenchNetRequest { fn default() -> Self { Self::read_le(&[0u8; 16]) } }
#[cfg(test)]
mod test_place_workbench_net_request {
    use super::*;
    #[test]
    fn layout() {
        assert_eq!(core::mem::size_of::<PlaceWorkbenchNetRequest>(), 0x10);
        assert_eq!(<PlaceWorkbenchNetRequest as WireStruct>::SIZE, 0x10);
        assert_eq!(core::mem::offset_of!(PlaceWorkbenchNetRequest, pos_x), 0x0);
        assert_eq!(core::mem::offset_of!(PlaceWorkbenchNetRequest, pos_y), 0x4);
        assert_eq!(core::mem::offset_of!(PlaceWorkbenchNetRequest, workbench_type), 0x8);
        assert_eq!(core::mem::offset_of!(PlaceWorkbenchNetRequest, padding), 0x9);
    }
    #[test]
    fn roundtrip() {
        let mut raw = [0u8; 0x10];
        for (i, x) in raw.iter_mut().enumerate() { *x = (i as u8).wrapping_mul(37).wrapping_add(11); }
        let v = PlaceWorkbenchNetRequest::read_le(&raw);
        let mut back = vec![0u8; raw.len()];
        v.write_le(&mut back);
        assert_eq!(&back[..], &raw[..]);   // byte-exact incl. padding (floats compared as bytes, NaN-safe)
        assert!(<PlaceWorkbenchNetRequest>::default().to_vec().iter().all(|&x| x == 0));
    }
}

/// size 0x8
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PlaceInteractionObjectNetRequest {
    /// uint32_t @ 0x0
    pub pos_x: u32,
    /// uint32_t @ 0x4
    pub pos_y: u32,
}
impl WireStruct for PlaceInteractionObjectNetRequest {
    const SIZE: usize = 0x8;
    fn write_le(&self, out: &mut [u8]) {
        assert!(out.len() >= Self::SIZE);
        out[..Self::SIZE].fill(0);
        out[0x0..0x4].copy_from_slice(&self.pos_x.to_le_bytes());
        out[0x4..0x8].copy_from_slice(&self.pos_y.to_le_bytes());
    }
    fn read_le(b: &[u8]) -> Self {
        assert!(b.len() >= Self::SIZE);
        Self {
            pos_x: u32::from_le_bytes(b[0x0..0x4].try_into().unwrap()),
            pos_y: u32::from_le_bytes(b[0x4..0x8].try_into().unwrap()),
        }
    }
}
impl Default for PlaceInteractionObjectNetRequest { fn default() -> Self { Self::read_le(&[0u8; 8]) } }
#[cfg(test)]
mod test_place_interaction_object_net_request {
    use super::*;
    #[test]
    fn layout() {
        assert_eq!(core::mem::size_of::<PlaceInteractionObjectNetRequest>(), 0x8);
        assert_eq!(<PlaceInteractionObjectNetRequest as WireStruct>::SIZE, 0x8);
        assert_eq!(core::mem::offset_of!(PlaceInteractionObjectNetRequest, pos_x), 0x0);
        assert_eq!(core::mem::offset_of!(PlaceInteractionObjectNetRequest, pos_y), 0x4);
    }
    #[test]
    fn roundtrip() {
        let mut raw = [0u8; 0x8];
        for (i, x) in raw.iter_mut().enumerate() { *x = (i as u8).wrapping_mul(37).wrapping_add(11); }
        let v = PlaceInteractionObjectNetRequest::read_le(&raw);
        let mut back = vec![0u8; raw.len()];
        v.write_le(&mut back);
        assert_eq!(&back[..], &raw[..]);   // byte-exact incl. padding (floats compared as bytes, NaN-safe)
        assert!(<PlaceInteractionObjectNetRequest>::default().to_vec().iter().all(|&x| x == 0));
    }
}

/// size 0x10
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct GatherNetRequest {
    /// uint32_t @ 0x0
    pub pos_x: u32,
    /// uint32_t @ 0x4
    pub pos_y: u32,
    /// uint8_t @ 0x8
    pub gather_progress: u8,
    /// explicit padding at 0x9; preserved on read, zero by default
    pub padding: [u8; 7],
}
impl WireStruct for GatherNetRequest {
    const SIZE: usize = 0x10;
    fn write_le(&self, out: &mut [u8]) {
        assert!(out.len() >= Self::SIZE);
        out[..Self::SIZE].fill(0);
        out[0x0..0x4].copy_from_slice(&self.pos_x.to_le_bytes());
        out[0x4..0x8].copy_from_slice(&self.pos_y.to_le_bytes());
        out[0x8..0x9].copy_from_slice(&self.gather_progress.to_le_bytes());
        out[0x9..0x10].copy_from_slice(&self.padding);
    }
    fn read_le(b: &[u8]) -> Self {
        assert!(b.len() >= Self::SIZE);
        Self {
            pos_x: u32::from_le_bytes(b[0x0..0x4].try_into().unwrap()),
            pos_y: u32::from_le_bytes(b[0x4..0x8].try_into().unwrap()),
            gather_progress: u8::from_le_bytes(b[0x8..0x9].try_into().unwrap()),
            padding: b[0x9..0x10].try_into().unwrap(),
        }
    }
}
impl Default for GatherNetRequest { fn default() -> Self { Self::read_le(&[0u8; 16]) } }
#[cfg(test)]
mod test_gather_net_request {
    use super::*;
    #[test]
    fn layout() {
        assert_eq!(core::mem::size_of::<GatherNetRequest>(), 0x10);
        assert_eq!(<GatherNetRequest as WireStruct>::SIZE, 0x10);
        assert_eq!(core::mem::offset_of!(GatherNetRequest, pos_x), 0x0);
        assert_eq!(core::mem::offset_of!(GatherNetRequest, pos_y), 0x4);
        assert_eq!(core::mem::offset_of!(GatherNetRequest, gather_progress), 0x8);
        assert_eq!(core::mem::offset_of!(GatherNetRequest, padding), 0x9);
    }
    #[test]
    fn roundtrip() {
        let mut raw = [0u8; 0x10];
        for (i, x) in raw.iter_mut().enumerate() { *x = (i as u8).wrapping_mul(37).wrapping_add(11); }
        let v = GatherNetRequest::read_le(&raw);
        let mut back = vec![0u8; raw.len()];
        v.write_le(&mut back);
        assert_eq!(&back[..], &raw[..]);   // byte-exact incl. padding (floats compared as bytes, NaN-safe)
        assert!(<GatherNetRequest>::default().to_vec().iter().all(|&x| x == 0));
    }
}

/// size 0x14
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ProjectileNetRequest {
    /// Float32 @ 0x0
    pub from_pos_x: f32,
    /// Float32 @ 0x4
    pub from_pos_y: f32,
    /// Float32 @ 0x8
    pub to_pos_x: f32,
    /// Float32 @ 0xc
    pub to_pos_y: f32,
    /// uint16_t @ 0x10
    pub item_type: u16,
    /// implicit padding at 0x12; preserved on read, zero by default
    pub gap_12: [u8; 2],
}
impl WireStruct for ProjectileNetRequest {
    const SIZE: usize = 0x14;
    fn write_le(&self, out: &mut [u8]) {
        assert!(out.len() >= Self::SIZE);
        out[..Self::SIZE].fill(0);
        out[0x0..0x4].copy_from_slice(&self.from_pos_x.to_le_bytes());
        out[0x4..0x8].copy_from_slice(&self.from_pos_y.to_le_bytes());
        out[0x8..0xc].copy_from_slice(&self.to_pos_x.to_le_bytes());
        out[0xc..0x10].copy_from_slice(&self.to_pos_y.to_le_bytes());
        out[0x10..0x12].copy_from_slice(&self.item_type.to_le_bytes());
        out[0x12..0x14].copy_from_slice(&self.gap_12);
    }
    fn read_le(b: &[u8]) -> Self {
        assert!(b.len() >= Self::SIZE);
        Self {
            from_pos_x: f32::from_le_bytes(b[0x0..0x4].try_into().unwrap()),
            from_pos_y: f32::from_le_bytes(b[0x4..0x8].try_into().unwrap()),
            to_pos_x: f32::from_le_bytes(b[0x8..0xc].try_into().unwrap()),
            to_pos_y: f32::from_le_bytes(b[0xc..0x10].try_into().unwrap()),
            item_type: u16::from_le_bytes(b[0x10..0x12].try_into().unwrap()),
            gap_12: b[0x12..0x14].try_into().unwrap(),
        }
    }
}
impl Default for ProjectileNetRequest { fn default() -> Self { Self::read_le(&[0u8; 20]) } }
#[cfg(test)]
mod test_projectile_net_request {
    use super::*;
    #[test]
    fn layout() {
        assert_eq!(core::mem::size_of::<ProjectileNetRequest>(), 0x14);
        assert_eq!(<ProjectileNetRequest as WireStruct>::SIZE, 0x14);
        assert_eq!(core::mem::offset_of!(ProjectileNetRequest, from_pos_x), 0x0);
        assert_eq!(core::mem::offset_of!(ProjectileNetRequest, from_pos_y), 0x4);
        assert_eq!(core::mem::offset_of!(ProjectileNetRequest, to_pos_x), 0x8);
        assert_eq!(core::mem::offset_of!(ProjectileNetRequest, to_pos_y), 0xc);
        assert_eq!(core::mem::offset_of!(ProjectileNetRequest, item_type), 0x10);
    }
    #[test]
    fn roundtrip() {
        let mut raw = [0u8; 0x14];
        for (i, x) in raw.iter_mut().enumerate() { *x = (i as u8).wrapping_mul(37).wrapping_add(11); }
        let v = ProjectileNetRequest::read_le(&raw);
        let mut back = vec![0u8; raw.len()];
        v.write_le(&mut back);
        assert_eq!(&back[..], &raw[..]);   // byte-exact incl. padding (floats compared as bytes, NaN-safe)
        assert!(<ProjectileNetRequest>::default().to_vec().iter().all(|&x| x == 0));
    }
}

/// size 0x10
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BlockheadDamageNetRequest {
    /// uint64_t @ 0x0
    pub blockhead_id: u64,
    /// Float32 @ 0x8
    pub damage: f32,
    /// uint8_t @ 0xc
    pub recoil: u8,
    /// implicit padding at 0xd; preserved on read, zero by default
    pub gap_d: [u8; 3],
}
impl WireStruct for BlockheadDamageNetRequest {
    const SIZE: usize = 0x10;
    fn write_le(&self, out: &mut [u8]) {
        assert!(out.len() >= Self::SIZE);
        out[..Self::SIZE].fill(0);
        out[0x0..0x8].copy_from_slice(&self.blockhead_id.to_le_bytes());
        out[0x8..0xc].copy_from_slice(&self.damage.to_le_bytes());
        out[0xc..0xd].copy_from_slice(&self.recoil.to_le_bytes());
        out[0xd..0x10].copy_from_slice(&self.gap_d);
    }
    fn read_le(b: &[u8]) -> Self {
        assert!(b.len() >= Self::SIZE);
        Self {
            blockhead_id: u64::from_le_bytes(b[0x0..0x8].try_into().unwrap()),
            damage: f32::from_le_bytes(b[0x8..0xc].try_into().unwrap()),
            recoil: u8::from_le_bytes(b[0xc..0xd].try_into().unwrap()),
            gap_d: b[0xd..0x10].try_into().unwrap(),
        }
    }
}
impl Default for BlockheadDamageNetRequest { fn default() -> Self { Self::read_le(&[0u8; 16]) } }
#[cfg(test)]
mod test_blockhead_damage_net_request {
    use super::*;
    #[test]
    fn layout() {
        assert_eq!(core::mem::size_of::<BlockheadDamageNetRequest>(), 0x10);
        assert_eq!(<BlockheadDamageNetRequest as WireStruct>::SIZE, 0x10);
        assert_eq!(core::mem::offset_of!(BlockheadDamageNetRequest, blockhead_id), 0x0);
        assert_eq!(core::mem::offset_of!(BlockheadDamageNetRequest, damage), 0x8);
        assert_eq!(core::mem::offset_of!(BlockheadDamageNetRequest, recoil), 0xc);
    }
    #[test]
    fn roundtrip() {
        let mut raw = [0u8; 0x10];
        for (i, x) in raw.iter_mut().enumerate() { *x = (i as u8).wrapping_mul(37).wrapping_add(11); }
        let v = BlockheadDamageNetRequest::read_le(&raw);
        let mut back = vec![0u8; raw.len()];
        v.write_le(&mut back);
        assert_eq!(&back[..], &raw[..]);   // byte-exact incl. padding (floats compared as bytes, NaN-safe)
        assert!(<BlockheadDamageNetRequest>::default().to_vec().iter().all(|&x| x == 0));
    }
}

/// size 0x10
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WorldHeartbeat {
    /// Float32 @ 0x0
    pub world_time: f32,
    /// Float32 @ 0x4
    pub no_rain_timer: f32,
    /// BOOL @ 0x8
    pub fast_forward: u8,
    /// BOOL @ 0x9
    pub local_paused: u8,
    /// BOOL @ 0xa
    pub all_paused: u8,
    /// BOOL @ 0xb
    pub pvp_disabled: u8,
    /// Float32 @ 0xc
    pub credit: f32,
}
impl WireStruct for WorldHeartbeat {
    const SIZE: usize = 0x10;
    fn write_le(&self, out: &mut [u8]) {
        assert!(out.len() >= Self::SIZE);
        out[..Self::SIZE].fill(0);
        out[0x0..0x4].copy_from_slice(&self.world_time.to_le_bytes());
        out[0x4..0x8].copy_from_slice(&self.no_rain_timer.to_le_bytes());
        out[0x8..0x9].copy_from_slice(&self.fast_forward.to_le_bytes());
        out[0x9..0xa].copy_from_slice(&self.local_paused.to_le_bytes());
        out[0xa..0xb].copy_from_slice(&self.all_paused.to_le_bytes());
        out[0xb..0xc].copy_from_slice(&self.pvp_disabled.to_le_bytes());
        out[0xc..0x10].copy_from_slice(&self.credit.to_le_bytes());
    }
    fn read_le(b: &[u8]) -> Self {
        assert!(b.len() >= Self::SIZE);
        Self {
            world_time: f32::from_le_bytes(b[0x0..0x4].try_into().unwrap()),
            no_rain_timer: f32::from_le_bytes(b[0x4..0x8].try_into().unwrap()),
            fast_forward: u8::from_le_bytes(b[0x8..0x9].try_into().unwrap()),
            local_paused: u8::from_le_bytes(b[0x9..0xa].try_into().unwrap()),
            all_paused: u8::from_le_bytes(b[0xa..0xb].try_into().unwrap()),
            pvp_disabled: u8::from_le_bytes(b[0xb..0xc].try_into().unwrap()),
            credit: f32::from_le_bytes(b[0xc..0x10].try_into().unwrap()),
        }
    }
}
impl Default for WorldHeartbeat { fn default() -> Self { Self::read_le(&[0u8; 16]) } }
#[cfg(test)]
mod test_world_heartbeat {
    use super::*;
    #[test]
    fn layout() {
        assert_eq!(core::mem::size_of::<WorldHeartbeat>(), 0x10);
        assert_eq!(<WorldHeartbeat as WireStruct>::SIZE, 0x10);
        assert_eq!(core::mem::offset_of!(WorldHeartbeat, world_time), 0x0);
        assert_eq!(core::mem::offset_of!(WorldHeartbeat, no_rain_timer), 0x4);
        assert_eq!(core::mem::offset_of!(WorldHeartbeat, fast_forward), 0x8);
        assert_eq!(core::mem::offset_of!(WorldHeartbeat, local_paused), 0x9);
        assert_eq!(core::mem::offset_of!(WorldHeartbeat, all_paused), 0xa);
        assert_eq!(core::mem::offset_of!(WorldHeartbeat, pvp_disabled), 0xb);
        assert_eq!(core::mem::offset_of!(WorldHeartbeat, credit), 0xc);
    }
    #[test]
    fn roundtrip() {
        let mut raw = [0u8; 0x10];
        for (i, x) in raw.iter_mut().enumerate() { *x = (i as u8).wrapping_mul(37).wrapping_add(11); }
        let v = WorldHeartbeat::read_le(&raw);
        let mut back = vec![0u8; raw.len()];
        v.write_le(&mut back);
        assert_eq!(&back[..], &raw[..]);   // byte-exact incl. padding (floats compared as bytes, NaN-safe)
        assert!(<WorldHeartbeat>::default().to_vec().iter().all(|&x| x == 0));
    }
}

/// size 0x8
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ElectricityParticleHeader {
    /// uint16_t @ 0x0
    pub size: u16,
    /// uint16_t @ 0x2
    pub path_count: u16,
    /// explicit padding at 0x4; preserved on read, zero by default
    pub padding: [u8; 4],
}
impl WireStruct for ElectricityParticleHeader {
    const SIZE: usize = 0x8;
    fn write_le(&self, out: &mut [u8]) {
        assert!(out.len() >= Self::SIZE);
        out[..Self::SIZE].fill(0);
        out[0x0..0x2].copy_from_slice(&self.size.to_le_bytes());
        out[0x2..0x4].copy_from_slice(&self.path_count.to_le_bytes());
        out[0x4..0x8].copy_from_slice(&self.padding);
    }
    fn read_le(b: &[u8]) -> Self {
        assert!(b.len() >= Self::SIZE);
        Self {
            size: u16::from_le_bytes(b[0x0..0x2].try_into().unwrap()),
            path_count: u16::from_le_bytes(b[0x2..0x4].try_into().unwrap()),
            padding: b[0x4..0x8].try_into().unwrap(),
        }
    }
}
impl Default for ElectricityParticleHeader { fn default() -> Self { Self::read_le(&[0u8; 8]) } }
#[cfg(test)]
mod test_electricity_particle_header {
    use super::*;
    #[test]
    fn layout() {
        assert_eq!(core::mem::size_of::<ElectricityParticleHeader>(), 0x8);
        assert_eq!(<ElectricityParticleHeader as WireStruct>::SIZE, 0x8);
        assert_eq!(core::mem::offset_of!(ElectricityParticleHeader, size), 0x0);
        assert_eq!(core::mem::offset_of!(ElectricityParticleHeader, path_count), 0x2);
        assert_eq!(core::mem::offset_of!(ElectricityParticleHeader, padding), 0x4);
    }
    #[test]
    fn roundtrip() {
        let mut raw = [0u8; 0x8];
        for (i, x) in raw.iter_mut().enumerate() { *x = (i as u8).wrapping_mul(37).wrapping_add(11); }
        let v = ElectricityParticleHeader::read_le(&raw);
        let mut back = vec![0u8; raw.len()];
        v.write_le(&mut back);
        assert_eq!(&back[..], &raw[..]);   // byte-exact incl. padding (floats compared as bytes, NaN-safe)
        assert!(<ElectricityParticleHeader>::default().to_vec().iter().all(|&x| x == 0));
    }
}

/// size 0x28
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct YakUpdateNetData {
    /// DonkeyLikeUpdateNetData @ 0x0
    pub donkey_like_update_net_data: DonkeyLikeUpdateNetData,
    /// int16_t @ 0x20
    pub milk: i16,
    /// int16_t @ 0x22
    pub hair: i16,
    /// explicit padding at 0x24; preserved on read, zero by default
    pub padding: [u8; 4],
}
impl WireStruct for YakUpdateNetData {
    const SIZE: usize = 0x28;
    fn write_le(&self, out: &mut [u8]) {
        assert!(out.len() >= Self::SIZE);
        out[..Self::SIZE].fill(0);
        self.donkey_like_update_net_data.write_le(&mut out[0x0..]);
        out[0x20..0x22].copy_from_slice(&self.milk.to_le_bytes());
        out[0x22..0x24].copy_from_slice(&self.hair.to_le_bytes());
        out[0x24..0x28].copy_from_slice(&self.padding);
    }
    fn read_le(b: &[u8]) -> Self {
        assert!(b.len() >= Self::SIZE);
        Self {
            donkey_like_update_net_data: DonkeyLikeUpdateNetData::read_le(&b[0x0..]),
            milk: i16::from_le_bytes(b[0x20..0x22].try_into().unwrap()),
            hair: i16::from_le_bytes(b[0x22..0x24].try_into().unwrap()),
            padding: b[0x24..0x28].try_into().unwrap(),
        }
    }
}
impl Default for YakUpdateNetData { fn default() -> Self { Self::read_le(&[0u8; 40]) } }
#[cfg(test)]
mod test_yak_update_net_data {
    use super::*;
    #[test]
    fn layout() {
        assert_eq!(core::mem::size_of::<YakUpdateNetData>(), 0x28);
        assert_eq!(<YakUpdateNetData as WireStruct>::SIZE, 0x28);
        assert_eq!(core::mem::offset_of!(YakUpdateNetData, donkey_like_update_net_data), 0x0);
        assert_eq!(core::mem::offset_of!(YakUpdateNetData, milk), 0x20);
        assert_eq!(core::mem::offset_of!(YakUpdateNetData, hair), 0x22);
        assert_eq!(core::mem::offset_of!(YakUpdateNetData, padding), 0x24);
    }
    #[test]
    fn roundtrip() {
        let mut raw = [0u8; 0x28];
        for (i, x) in raw.iter_mut().enumerate() { *x = (i as u8).wrapping_mul(37).wrapping_add(11); }
        let v = YakUpdateNetData::read_le(&raw);
        let mut back = vec![0u8; raw.len()];
        v.write_le(&mut back);
        assert_eq!(&back[..], &raw[..]);   // byte-exact incl. padding (floats compared as bytes, NaN-safe)
        assert!(<YakUpdateNetData>::default().to_vec().iter().all(|&x| x == 0));
    }
}

/// size 0x90
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct YakCreationNetData {
    /// DonkeyLikeCreationNetData @ 0x0
    pub donkey_like_creation_net_data: DonkeyLikeCreationNetData,
    /// YakUpdateNetData @ 0x68
    pub yak_update_net_data: YakUpdateNetData,
}
impl WireStruct for YakCreationNetData {
    const SIZE: usize = 0x90;
    fn write_le(&self, out: &mut [u8]) {
        assert!(out.len() >= Self::SIZE);
        out[..Self::SIZE].fill(0);
        self.donkey_like_creation_net_data.write_le(&mut out[0x0..]);
        self.yak_update_net_data.write_le(&mut out[0x68..]);
    }
    fn read_le(b: &[u8]) -> Self {
        assert!(b.len() >= Self::SIZE);
        Self {
            donkey_like_creation_net_data: DonkeyLikeCreationNetData::read_le(&b[0x0..]),
            yak_update_net_data: YakUpdateNetData::read_le(&b[0x68..]),
        }
    }
}
impl Default for YakCreationNetData { fn default() -> Self { Self::read_le(&[0u8; 144]) } }
#[cfg(test)]
mod test_yak_creation_net_data {
    use super::*;
    #[test]
    fn layout() {
        assert_eq!(core::mem::size_of::<YakCreationNetData>(), 0x90);
        assert_eq!(<YakCreationNetData as WireStruct>::SIZE, 0x90);
        assert_eq!(core::mem::offset_of!(YakCreationNetData, donkey_like_creation_net_data), 0x0);
        assert_eq!(core::mem::offset_of!(YakCreationNetData, yak_update_net_data), 0x68);
    }
    #[test]
    fn roundtrip() {
        let mut raw = [0u8; 0x90];
        for (i, x) in raw.iter_mut().enumerate() { *x = (i as u8).wrapping_mul(37).wrapping_add(11); }
        let v = YakCreationNetData::read_le(&raw);
        let mut back = vec![0u8; raw.len()];
        v.write_le(&mut back);
        assert_eq!(&back[..], &raw[..]);   // byte-exact incl. padding (floats compared as bytes, NaN-safe)
        assert!(<YakCreationNetData>::default().to_vec().iter().all(|&x| x == 0));
    }
}

