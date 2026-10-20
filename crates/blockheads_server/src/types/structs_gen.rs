// @generated layout table (wire/save format). DO NOT EDIT.
// Included via include!() from the hand-written sibling file; `WireStruct` must be in scope.

/// size 0x6c
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BlockheadState {
    /// BOOL @ 0x0
    pub interacting: u8,
    /// implicit padding at 0x1; preserved on read, zero by default
    pub gap_1: [u8; 3],
    /// enum InteractionType @ 0x4
    pub interaction_type: u32,
    /// enum InteractionType @ 0x8
    pub goal_interaction_type_while_walking: u32,
    /// intpair @ 0xc
    pub interacting_square: IntPair,
    /// float @ 0x14
    pub interaction_timer: f32,
    /// float @ 0x18
    pub gather_speed: f32,
    /// float @ 0x1c
    pub gather_timer: f32,
    /// float @ 0x20
    pub health: f32,
    /// float @ 0x24
    pub happiness: f32,
    /// float @ 0x28
    pub fullness: f32,
    /// float @ 0x2c
    pub energy: f32,
    /// float @ 0x30
    pub meditation_progress: f32,
    /// float @ 0x34
    pub drown_fraction: f32,
    /// float @ 0x38
    pub environment: f32,
    /// float @ 0x3c
    pub environment_temperature: f32,
    /// float @ 0x40
    pub environment_exposure: f32,
    /// float @ 0x44
    pub environment_light: f32,
    /// float @ 0x48
    pub heat: f32,
    /// enum BlockheadAnimationType @ 0x4c
    pub animation_type: u32,
    /// enum BlockheadSubAnimationType @ 0x50
    pub sub_animation_type: u32,
    /// float @ 0x54
    pub coffee_energy_timer: f32,
    /// float @ 0x58
    pub food_pause_timer: f32,
    /// float @ 0x5c
    pub death: f32,
    /// BOOL @ 0x60
    pub regenerating: u8,
    /// implicit padding at 0x61; preserved on read, zero by default
    pub gap_61: [u8; 3],
    /// float @ 0x64
    pub regeneration_progress: f32,
    /// BOOL @ 0x68
    pub on_trade_mission: u8,
    /// implicit padding at 0x69; preserved on read, zero by default
    pub gap_69: [u8; 3],
}
impl WireStruct for BlockheadState {
    const SIZE: usize = 0x6c;
    fn write_le(&self, out: &mut [u8]) {
        assert!(out.len() >= Self::SIZE);
        out[..Self::SIZE].fill(0);
        out[0x0..0x1].copy_from_slice(&self.interacting.to_le_bytes());
        out[0x1..0x4].copy_from_slice(&self.gap_1);
        out[0x4..0x8].copy_from_slice(&self.interaction_type.to_le_bytes());
        out[0x8..0xc].copy_from_slice(&self.goal_interaction_type_while_walking.to_le_bytes());
        self.interacting_square.write_le(&mut out[0xc..]);
        out[0x14..0x18].copy_from_slice(&self.interaction_timer.to_le_bytes());
        out[0x18..0x1c].copy_from_slice(&self.gather_speed.to_le_bytes());
        out[0x1c..0x20].copy_from_slice(&self.gather_timer.to_le_bytes());
        out[0x20..0x24].copy_from_slice(&self.health.to_le_bytes());
        out[0x24..0x28].copy_from_slice(&self.happiness.to_le_bytes());
        out[0x28..0x2c].copy_from_slice(&self.fullness.to_le_bytes());
        out[0x2c..0x30].copy_from_slice(&self.energy.to_le_bytes());
        out[0x30..0x34].copy_from_slice(&self.meditation_progress.to_le_bytes());
        out[0x34..0x38].copy_from_slice(&self.drown_fraction.to_le_bytes());
        out[0x38..0x3c].copy_from_slice(&self.environment.to_le_bytes());
        out[0x3c..0x40].copy_from_slice(&self.environment_temperature.to_le_bytes());
        out[0x40..0x44].copy_from_slice(&self.environment_exposure.to_le_bytes());
        out[0x44..0x48].copy_from_slice(&self.environment_light.to_le_bytes());
        out[0x48..0x4c].copy_from_slice(&self.heat.to_le_bytes());
        out[0x4c..0x50].copy_from_slice(&self.animation_type.to_le_bytes());
        out[0x50..0x54].copy_from_slice(&self.sub_animation_type.to_le_bytes());
        out[0x54..0x58].copy_from_slice(&self.coffee_energy_timer.to_le_bytes());
        out[0x58..0x5c].copy_from_slice(&self.food_pause_timer.to_le_bytes());
        out[0x5c..0x60].copy_from_slice(&self.death.to_le_bytes());
        out[0x60..0x61].copy_from_slice(&self.regenerating.to_le_bytes());
        out[0x61..0x64].copy_from_slice(&self.gap_61);
        out[0x64..0x68].copy_from_slice(&self.regeneration_progress.to_le_bytes());
        out[0x68..0x69].copy_from_slice(&self.on_trade_mission.to_le_bytes());
        out[0x69..0x6c].copy_from_slice(&self.gap_69);
    }
    fn read_le(b: &[u8]) -> Self {
        assert!(b.len() >= Self::SIZE);
        Self {
            interacting: u8::from_le_bytes(b[0x0..0x1].try_into().unwrap()),
            gap_1: b[0x1..0x4].try_into().unwrap(),
            interaction_type: u32::from_le_bytes(b[0x4..0x8].try_into().unwrap()),
            goal_interaction_type_while_walking: u32::from_le_bytes(b[0x8..0xc].try_into().unwrap()),
            interacting_square: IntPair::read_le(&b[0xc..]),
            interaction_timer: f32::from_le_bytes(b[0x14..0x18].try_into().unwrap()),
            gather_speed: f32::from_le_bytes(b[0x18..0x1c].try_into().unwrap()),
            gather_timer: f32::from_le_bytes(b[0x1c..0x20].try_into().unwrap()),
            health: f32::from_le_bytes(b[0x20..0x24].try_into().unwrap()),
            happiness: f32::from_le_bytes(b[0x24..0x28].try_into().unwrap()),
            fullness: f32::from_le_bytes(b[0x28..0x2c].try_into().unwrap()),
            energy: f32::from_le_bytes(b[0x2c..0x30].try_into().unwrap()),
            meditation_progress: f32::from_le_bytes(b[0x30..0x34].try_into().unwrap()),
            drown_fraction: f32::from_le_bytes(b[0x34..0x38].try_into().unwrap()),
            environment: f32::from_le_bytes(b[0x38..0x3c].try_into().unwrap()),
            environment_temperature: f32::from_le_bytes(b[0x3c..0x40].try_into().unwrap()),
            environment_exposure: f32::from_le_bytes(b[0x40..0x44].try_into().unwrap()),
            environment_light: f32::from_le_bytes(b[0x44..0x48].try_into().unwrap()),
            heat: f32::from_le_bytes(b[0x48..0x4c].try_into().unwrap()),
            animation_type: u32::from_le_bytes(b[0x4c..0x50].try_into().unwrap()),
            sub_animation_type: u32::from_le_bytes(b[0x50..0x54].try_into().unwrap()),
            coffee_energy_timer: f32::from_le_bytes(b[0x54..0x58].try_into().unwrap()),
            food_pause_timer: f32::from_le_bytes(b[0x58..0x5c].try_into().unwrap()),
            death: f32::from_le_bytes(b[0x5c..0x60].try_into().unwrap()),
            regenerating: u8::from_le_bytes(b[0x60..0x61].try_into().unwrap()),
            gap_61: b[0x61..0x64].try_into().unwrap(),
            regeneration_progress: f32::from_le_bytes(b[0x64..0x68].try_into().unwrap()),
            on_trade_mission: u8::from_le_bytes(b[0x68..0x69].try_into().unwrap()),
            gap_69: b[0x69..0x6c].try_into().unwrap(),
        }
    }
}
impl Default for BlockheadState { fn default() -> Self { Self::read_le(&[0u8; 108]) } }
#[cfg(test)]
mod test_blockhead_state {
    use super::*;
    #[test]
    fn layout() {
        assert_eq!(core::mem::size_of::<BlockheadState>(), 0x6c);
        assert_eq!(<BlockheadState as WireStruct>::SIZE, 0x6c);
        assert_eq!(core::mem::offset_of!(BlockheadState, interacting), 0x0);
        assert_eq!(core::mem::offset_of!(BlockheadState, interaction_type), 0x4);
        assert_eq!(core::mem::offset_of!(BlockheadState, goal_interaction_type_while_walking), 0x8);
        assert_eq!(core::mem::offset_of!(BlockheadState, interacting_square), 0xc);
        assert_eq!(core::mem::offset_of!(BlockheadState, interaction_timer), 0x14);
        assert_eq!(core::mem::offset_of!(BlockheadState, gather_speed), 0x18);
        assert_eq!(core::mem::offset_of!(BlockheadState, gather_timer), 0x1c);
        assert_eq!(core::mem::offset_of!(BlockheadState, health), 0x20);
        assert_eq!(core::mem::offset_of!(BlockheadState, happiness), 0x24);
        assert_eq!(core::mem::offset_of!(BlockheadState, fullness), 0x28);
        assert_eq!(core::mem::offset_of!(BlockheadState, energy), 0x2c);
        assert_eq!(core::mem::offset_of!(BlockheadState, meditation_progress), 0x30);
        assert_eq!(core::mem::offset_of!(BlockheadState, drown_fraction), 0x34);
        assert_eq!(core::mem::offset_of!(BlockheadState, environment), 0x38);
        assert_eq!(core::mem::offset_of!(BlockheadState, environment_temperature), 0x3c);
        assert_eq!(core::mem::offset_of!(BlockheadState, environment_exposure), 0x40);
        assert_eq!(core::mem::offset_of!(BlockheadState, environment_light), 0x44);
        assert_eq!(core::mem::offset_of!(BlockheadState, heat), 0x48);
        assert_eq!(core::mem::offset_of!(BlockheadState, animation_type), 0x4c);
        assert_eq!(core::mem::offset_of!(BlockheadState, sub_animation_type), 0x50);
        assert_eq!(core::mem::offset_of!(BlockheadState, coffee_energy_timer), 0x54);
        assert_eq!(core::mem::offset_of!(BlockheadState, food_pause_timer), 0x58);
        assert_eq!(core::mem::offset_of!(BlockheadState, death), 0x5c);
        assert_eq!(core::mem::offset_of!(BlockheadState, regenerating), 0x60);
        assert_eq!(core::mem::offset_of!(BlockheadState, regeneration_progress), 0x64);
        assert_eq!(core::mem::offset_of!(BlockheadState, on_trade_mission), 0x68);
    }
    #[test]
    fn roundtrip() {
        let mut raw = [0u8; 0x6c];
        for (i, x) in raw.iter_mut().enumerate() { *x = (i as u8).wrapping_mul(37).wrapping_add(11); }
        let v = BlockheadState::read_le(&raw);
        let mut back = vec![0u8; raw.len()];
        v.write_le(&mut back);
        assert_eq!(&back[..], &raw[..]);   // byte-exact incl. padding (floats compared as bytes, NaN-safe)
        assert!(<BlockheadState>::default().to_vec().iter().all(|&x| x == 0));
    }
}

/// size 0xc
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct InteractionTestResult {
    /// int @ 0x0
    pub good_or_bad: i32,
    /// ItemType @ 0x4
    pub render_item_type: u32,
    /// uint16_t @ 0x8
    pub render_item_data_a: u16,
    /// uint16_t @ 0xa
    pub render_item_data_b: u16,
}
impl WireStruct for InteractionTestResult {
    const SIZE: usize = 0xc;
    fn write_le(&self, out: &mut [u8]) {
        assert!(out.len() >= Self::SIZE);
        out[..Self::SIZE].fill(0);
        out[0x0..0x4].copy_from_slice(&self.good_or_bad.to_le_bytes());
        out[0x4..0x8].copy_from_slice(&self.render_item_type.to_le_bytes());
        out[0x8..0xa].copy_from_slice(&self.render_item_data_a.to_le_bytes());
        out[0xa..0xc].copy_from_slice(&self.render_item_data_b.to_le_bytes());
    }
    fn read_le(b: &[u8]) -> Self {
        assert!(b.len() >= Self::SIZE);
        Self {
            good_or_bad: i32::from_le_bytes(b[0x0..0x4].try_into().unwrap()),
            render_item_type: u32::from_le_bytes(b[0x4..0x8].try_into().unwrap()),
            render_item_data_a: u16::from_le_bytes(b[0x8..0xa].try_into().unwrap()),
            render_item_data_b: u16::from_le_bytes(b[0xa..0xc].try_into().unwrap()),
        }
    }
}
impl Default for InteractionTestResult { fn default() -> Self { Self::read_le(&[0u8; 12]) } }
#[cfg(test)]
mod test_interaction_test_result {
    use super::*;
    #[test]
    fn layout() {
        assert_eq!(core::mem::size_of::<InteractionTestResult>(), 0xc);
        assert_eq!(<InteractionTestResult as WireStruct>::SIZE, 0xc);
        assert_eq!(core::mem::offset_of!(InteractionTestResult, good_or_bad), 0x0);
        assert_eq!(core::mem::offset_of!(InteractionTestResult, render_item_type), 0x4);
        assert_eq!(core::mem::offset_of!(InteractionTestResult, render_item_data_a), 0x8);
        assert_eq!(core::mem::offset_of!(InteractionTestResult, render_item_data_b), 0xa);
    }
    #[test]
    fn roundtrip() {
        let mut raw = [0u8; 0xc];
        for (i, x) in raw.iter_mut().enumerate() { *x = (i as u8).wrapping_mul(37).wrapping_add(11); }
        let v = InteractionTestResult::read_le(&raw);
        let mut back = vec![0u8; raw.len()];
        v.write_le(&mut back);
        assert_eq!(&back[..], &raw[..]);   // byte-exact incl. padding (floats compared as bytes, NaN-safe)
        assert!(<InteractionTestResult>::default().to_vec().iter().all(|&x| x == 0));
    }
}

/// size 0x18
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FreeBlockCreationCount {
    /// int @ 0x0
    pub count: i32,
    /// int @ 0x4
    pub bonus_multiplier: i32,
    /// vec4 @ 0x8
    pub bonus_color: Vector,
}
impl WireStruct for FreeBlockCreationCount {
    const SIZE: usize = 0x18;
    fn write_le(&self, out: &mut [u8]) {
        assert!(out.len() >= Self::SIZE);
        out[..Self::SIZE].fill(0);
        out[0x0..0x4].copy_from_slice(&self.count.to_le_bytes());
        out[0x4..0x8].copy_from_slice(&self.bonus_multiplier.to_le_bytes());
        self.bonus_color.write_le(&mut out[0x8..]);
    }
    fn read_le(b: &[u8]) -> Self {
        assert!(b.len() >= Self::SIZE);
        Self {
            count: i32::from_le_bytes(b[0x0..0x4].try_into().unwrap()),
            bonus_multiplier: i32::from_le_bytes(b[0x4..0x8].try_into().unwrap()),
            bonus_color: Vector::read_le(&b[0x8..]),
        }
    }
}
impl Default for FreeBlockCreationCount { fn default() -> Self { Self::read_le(&[0u8; 24]) } }
#[cfg(test)]
mod test_free_block_creation_count {
    use super::*;
    #[test]
    fn layout() {
        assert_eq!(core::mem::size_of::<FreeBlockCreationCount>(), 0x18);
        assert_eq!(<FreeBlockCreationCount as WireStruct>::SIZE, 0x18);
        assert_eq!(core::mem::offset_of!(FreeBlockCreationCount, count), 0x0);
        assert_eq!(core::mem::offset_of!(FreeBlockCreationCount, bonus_multiplier), 0x4);
        assert_eq!(core::mem::offset_of!(FreeBlockCreationCount, bonus_color), 0x8);
    }
    #[test]
    fn roundtrip() {
        let mut raw = [0u8; 0x18];
        for (i, x) in raw.iter_mut().enumerate() { *x = (i as u8).wrapping_mul(37).wrapping_add(11); }
        let v = FreeBlockCreationCount::read_le(&raw);
        let mut back = vec![0u8; raw.len()];
        v.write_le(&mut back);
        assert_eq!(&back[..], &raw[..]);   // byte-exact incl. padding (floats compared as bytes, NaN-safe)
        assert!(<FreeBlockCreationCount>::default().to_vec().iter().all(|&x| x == 0));
    }
}

/// size 0x14
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct BlockheadSkinOptions {
    /// uint8_t @ 0x0
    pub is_male: u8,
    /// uint8_t @ 0x1
    pub face_index: u8,
    /// uint8_t @ 0x2
    pub hair_texture_index: u8,
    /// uint8_t @ 0x3
    pub hair_style_index: u8,
    /// MJColor8 @ 0x4
    pub skin_color: MJColor8,
    /// MJColor8 @ 0x8
    pub hair_color: MJColor8,
    /// MJColor8 @ 0xc
    pub underwear_color_a: MJColor8,
    /// MJColor8 @ 0x10
    pub underwear_color_b: MJColor8,
}
impl WireStruct for BlockheadSkinOptions {
    const SIZE: usize = 0x14;
    fn write_le(&self, out: &mut [u8]) {
        assert!(out.len() >= Self::SIZE);
        out[..Self::SIZE].fill(0);
        out[0x0..0x1].copy_from_slice(&self.is_male.to_le_bytes());
        out[0x1..0x2].copy_from_slice(&self.face_index.to_le_bytes());
        out[0x2..0x3].copy_from_slice(&self.hair_texture_index.to_le_bytes());
        out[0x3..0x4].copy_from_slice(&self.hair_style_index.to_le_bytes());
        self.skin_color.write_le(&mut out[0x4..]);
        self.hair_color.write_le(&mut out[0x8..]);
        self.underwear_color_a.write_le(&mut out[0xc..]);
        self.underwear_color_b.write_le(&mut out[0x10..]);
    }
    fn read_le(b: &[u8]) -> Self {
        assert!(b.len() >= Self::SIZE);
        Self {
            is_male: u8::from_le_bytes(b[0x0..0x1].try_into().unwrap()),
            face_index: u8::from_le_bytes(b[0x1..0x2].try_into().unwrap()),
            hair_texture_index: u8::from_le_bytes(b[0x2..0x3].try_into().unwrap()),
            hair_style_index: u8::from_le_bytes(b[0x3..0x4].try_into().unwrap()),
            skin_color: MJColor8::read_le(&b[0x4..]),
            hair_color: MJColor8::read_le(&b[0x8..]),
            underwear_color_a: MJColor8::read_le(&b[0xc..]),
            underwear_color_b: MJColor8::read_le(&b[0x10..]),
        }
    }
}
impl Default for BlockheadSkinOptions { fn default() -> Self { Self::read_le(&[0u8; 20]) } }
#[cfg(test)]
mod test_blockhead_skin_options {
    use super::*;
    #[test]
    fn layout() {
        assert_eq!(core::mem::size_of::<BlockheadSkinOptions>(), 0x14);
        assert_eq!(<BlockheadSkinOptions as WireStruct>::SIZE, 0x14);
        assert_eq!(core::mem::offset_of!(BlockheadSkinOptions, is_male), 0x0);
        assert_eq!(core::mem::offset_of!(BlockheadSkinOptions, face_index), 0x1);
        assert_eq!(core::mem::offset_of!(BlockheadSkinOptions, hair_texture_index), 0x2);
        assert_eq!(core::mem::offset_of!(BlockheadSkinOptions, hair_style_index), 0x3);
        assert_eq!(core::mem::offset_of!(BlockheadSkinOptions, skin_color), 0x4);
        assert_eq!(core::mem::offset_of!(BlockheadSkinOptions, hair_color), 0x8);
        assert_eq!(core::mem::offset_of!(BlockheadSkinOptions, underwear_color_a), 0xc);
        assert_eq!(core::mem::offset_of!(BlockheadSkinOptions, underwear_color_b), 0x10);
    }
    #[test]
    fn roundtrip() {
        let mut raw = [0u8; 0x14];
        for (i, x) in raw.iter_mut().enumerate() { *x = (i as u8).wrapping_mul(37).wrapping_add(11); }
        let v = BlockheadSkinOptions::read_le(&raw);
        let mut back = vec![0u8; raw.len()];
        v.write_le(&mut back);
        assert_eq!(&back[..], &raw[..]);   // byte-exact incl. padding (floats compared as bytes, NaN-safe)
        assert!(<BlockheadSkinOptions>::default().to_vec().iter().all(|&x| x == 0));
    }
}

/// size 0x24
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CaveTrollState {
    /// BOOL @ 0x0
    pub interacting: u8,
    /// implicit padding at 0x1; preserved on read, zero by default
    pub gap_1: [u8; 3],
    /// enum InteractionType @ 0x4
    pub interaction_type: u32,
    /// enum InteractionType @ 0x8
    pub goal_interaction_type_while_walking: u32,
    /// intpair @ 0xc
    pub interacting_square: IntPair,
    /// float @ 0x14
    pub interaction_timer: f32,
    /// float @ 0x18
    pub drown_fraction: f32,
    /// enum CaveTrollAnimationType @ 0x1c
    pub animation_type: u32,
    /// enum CaveTrollSubAnimationType @ 0x20
    pub sub_animation_type: u32,
}
impl WireStruct for CaveTrollState {
    const SIZE: usize = 0x24;
    fn write_le(&self, out: &mut [u8]) {
        assert!(out.len() >= Self::SIZE);
        out[..Self::SIZE].fill(0);
        out[0x0..0x1].copy_from_slice(&self.interacting.to_le_bytes());
        out[0x1..0x4].copy_from_slice(&self.gap_1);
        out[0x4..0x8].copy_from_slice(&self.interaction_type.to_le_bytes());
        out[0x8..0xc].copy_from_slice(&self.goal_interaction_type_while_walking.to_le_bytes());
        self.interacting_square.write_le(&mut out[0xc..]);
        out[0x14..0x18].copy_from_slice(&self.interaction_timer.to_le_bytes());
        out[0x18..0x1c].copy_from_slice(&self.drown_fraction.to_le_bytes());
        out[0x1c..0x20].copy_from_slice(&self.animation_type.to_le_bytes());
        out[0x20..0x24].copy_from_slice(&self.sub_animation_type.to_le_bytes());
    }
    fn read_le(b: &[u8]) -> Self {
        assert!(b.len() >= Self::SIZE);
        Self {
            interacting: u8::from_le_bytes(b[0x0..0x1].try_into().unwrap()),
            gap_1: b[0x1..0x4].try_into().unwrap(),
            interaction_type: u32::from_le_bytes(b[0x4..0x8].try_into().unwrap()),
            goal_interaction_type_while_walking: u32::from_le_bytes(b[0x8..0xc].try_into().unwrap()),
            interacting_square: IntPair::read_le(&b[0xc..]),
            interaction_timer: f32::from_le_bytes(b[0x14..0x18].try_into().unwrap()),
            drown_fraction: f32::from_le_bytes(b[0x18..0x1c].try_into().unwrap()),
            animation_type: u32::from_le_bytes(b[0x1c..0x20].try_into().unwrap()),
            sub_animation_type: u32::from_le_bytes(b[0x20..0x24].try_into().unwrap()),
        }
    }
}
impl Default for CaveTrollState { fn default() -> Self { Self::read_le(&[0u8; 36]) } }
#[cfg(test)]
mod test_cave_troll_state {
    use super::*;
    #[test]
    fn layout() {
        assert_eq!(core::mem::size_of::<CaveTrollState>(), 0x24);
        assert_eq!(<CaveTrollState as WireStruct>::SIZE, 0x24);
        assert_eq!(core::mem::offset_of!(CaveTrollState, interacting), 0x0);
        assert_eq!(core::mem::offset_of!(CaveTrollState, interaction_type), 0x4);
        assert_eq!(core::mem::offset_of!(CaveTrollState, goal_interaction_type_while_walking), 0x8);
        assert_eq!(core::mem::offset_of!(CaveTrollState, interacting_square), 0xc);
        assert_eq!(core::mem::offset_of!(CaveTrollState, interaction_timer), 0x14);
        assert_eq!(core::mem::offset_of!(CaveTrollState, drown_fraction), 0x18);
        assert_eq!(core::mem::offset_of!(CaveTrollState, animation_type), 0x1c);
        assert_eq!(core::mem::offset_of!(CaveTrollState, sub_animation_type), 0x20);
    }
    #[test]
    fn roundtrip() {
        let mut raw = [0u8; 0x24];
        for (i, x) in raw.iter_mut().enumerate() { *x = (i as u8).wrapping_mul(37).wrapping_add(11); }
        let v = CaveTrollState::read_le(&raw);
        let mut back = vec![0u8; raw.len()];
        v.write_le(&mut back);
        assert_eq!(&back[..], &raw[..]);   // byte-exact incl. padding (floats compared as bytes, NaN-safe)
        assert!(<CaveTrollState>::default().to_vec().iter().all(|&x| x == 0));
    }
}

/// size 0x7c
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CraftableItem {
    /// ItemType @ 0x0
    pub crafted_item_type: u32,
    /// int @ 0x4
    pub crafted_item_count: i32,
    /// ItemType @ 0x8
    pub required_items: [u32; 8],
    /// int @ 0x28
    pub required_item_counts: [i32; 8],
    /// int @ 0x48
    pub number_of_required_items: i32,
    /// int @ 0x4c
    pub seconds_required: i32,
    /// int @ 0x50
    pub upgrade_level: i32,
    /// uint16_t @ 0x54
    pub crafted_item_data_a: u16,
    /// uint16_t @ 0x56
    pub crafted_item_data_b: u16,
    /// ItemType @ 0x58
    pub secondary_indicator_item: u32,
    /// int @ 0x5c
    pub required_item_data_bs: [i32; 8],
}
impl WireStruct for CraftableItem {
    const SIZE: usize = 0x7c;
    fn write_le(&self, out: &mut [u8]) {
        assert!(out.len() >= Self::SIZE);
        out[..Self::SIZE].fill(0);
        out[0x0..0x4].copy_from_slice(&self.crafted_item_type.to_le_bytes());
        out[0x4..0x8].copy_from_slice(&self.crafted_item_count.to_le_bytes());
        for (k, e) in self.required_items.iter().enumerate() { out[0x8 + k * 4..0x8 + (k + 1) * 4].copy_from_slice(&e.to_le_bytes()); }
        for (k, e) in self.required_item_counts.iter().enumerate() { out[0x28 + k * 4..0x28 + (k + 1) * 4].copy_from_slice(&e.to_le_bytes()); }
        out[0x48..0x4c].copy_from_slice(&self.number_of_required_items.to_le_bytes());
        out[0x4c..0x50].copy_from_slice(&self.seconds_required.to_le_bytes());
        out[0x50..0x54].copy_from_slice(&self.upgrade_level.to_le_bytes());
        out[0x54..0x56].copy_from_slice(&self.crafted_item_data_a.to_le_bytes());
        out[0x56..0x58].copy_from_slice(&self.crafted_item_data_b.to_le_bytes());
        out[0x58..0x5c].copy_from_slice(&self.secondary_indicator_item.to_le_bytes());
        for (k, e) in self.required_item_data_bs.iter().enumerate() { out[0x5c + k * 4..0x5c + (k + 1) * 4].copy_from_slice(&e.to_le_bytes()); }
    }
    fn read_le(b: &[u8]) -> Self {
        assert!(b.len() >= Self::SIZE);
        Self {
            crafted_item_type: u32::from_le_bytes(b[0x0..0x4].try_into().unwrap()),
            crafted_item_count: i32::from_le_bytes(b[0x4..0x8].try_into().unwrap()),
            required_items: core::array::from_fn(|k| u32::from_le_bytes(b[0x8 + k * 4..0x8 + (k + 1) * 4].try_into().unwrap())),
            required_item_counts: core::array::from_fn(|k| i32::from_le_bytes(b[0x28 + k * 4..0x28 + (k + 1) * 4].try_into().unwrap())),
            number_of_required_items: i32::from_le_bytes(b[0x48..0x4c].try_into().unwrap()),
            seconds_required: i32::from_le_bytes(b[0x4c..0x50].try_into().unwrap()),
            upgrade_level: i32::from_le_bytes(b[0x50..0x54].try_into().unwrap()),
            crafted_item_data_a: u16::from_le_bytes(b[0x54..0x56].try_into().unwrap()),
            crafted_item_data_b: u16::from_le_bytes(b[0x56..0x58].try_into().unwrap()),
            secondary_indicator_item: u32::from_le_bytes(b[0x58..0x5c].try_into().unwrap()),
            required_item_data_bs: core::array::from_fn(|k| i32::from_le_bytes(b[0x5c + k * 4..0x5c + (k + 1) * 4].try_into().unwrap())),
        }
    }
}
impl Default for CraftableItem { fn default() -> Self { Self::read_le(&[0u8; 124]) } }
#[cfg(test)]
mod test_craftable_item {
    use super::*;
    #[test]
    fn layout() {
        assert_eq!(core::mem::size_of::<CraftableItem>(), 0x7c);
        assert_eq!(<CraftableItem as WireStruct>::SIZE, 0x7c);
        assert_eq!(core::mem::offset_of!(CraftableItem, crafted_item_type), 0x0);
        assert_eq!(core::mem::offset_of!(CraftableItem, crafted_item_count), 0x4);
        assert_eq!(core::mem::offset_of!(CraftableItem, required_items), 0x8);
        assert_eq!(core::mem::offset_of!(CraftableItem, required_item_counts), 0x28);
        assert_eq!(core::mem::offset_of!(CraftableItem, number_of_required_items), 0x48);
        assert_eq!(core::mem::offset_of!(CraftableItem, seconds_required), 0x4c);
        assert_eq!(core::mem::offset_of!(CraftableItem, upgrade_level), 0x50);
        assert_eq!(core::mem::offset_of!(CraftableItem, crafted_item_data_a), 0x54);
        assert_eq!(core::mem::offset_of!(CraftableItem, crafted_item_data_b), 0x56);
        assert_eq!(core::mem::offset_of!(CraftableItem, secondary_indicator_item), 0x58);
        assert_eq!(core::mem::offset_of!(CraftableItem, required_item_data_bs), 0x5c);
    }
    #[test]
    fn roundtrip() {
        let mut raw = [0u8; 0x7c];
        for (i, x) in raw.iter_mut().enumerate() { *x = (i as u8).wrapping_mul(37).wrapping_add(11); }
        let v = CraftableItem::read_le(&raw);
        let mut back = vec![0u8; raw.len()];
        v.write_le(&mut back);
        assert_eq!(&back[..], &raw[..]);   // byte-exact incl. padding (floats compared as bytes, NaN-safe)
        assert!(<CraftableItem>::default().to_vec().iter().all(|&x| x == 0));
    }
}

/// size 0x18
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct DPadPathResult {
    /// PathTestResult @ 0x0
    pub path_test_result: PathTestResult,
    /// intpair @ 0x10
    pub movement_pos: IntPair,
}
impl WireStruct for DPadPathResult {
    const SIZE: usize = 0x18;
    fn write_le(&self, out: &mut [u8]) {
        assert!(out.len() >= Self::SIZE);
        out[..Self::SIZE].fill(0);
        self.path_test_result.write_le(&mut out[0x0..]);
        self.movement_pos.write_le(&mut out[0x10..]);
    }
    fn read_le(b: &[u8]) -> Self {
        assert!(b.len() >= Self::SIZE);
        Self {
            path_test_result: PathTestResult::read_le(&b[0x0..]),
            movement_pos: IntPair::read_le(&b[0x10..]),
        }
    }
}
impl Default for DPadPathResult { fn default() -> Self { Self::read_le(&[0u8; 24]) } }
#[cfg(test)]
mod test_dpad_path_result {
    use super::*;
    #[test]
    fn layout() {
        assert_eq!(core::mem::size_of::<DPadPathResult>(), 0x18);
        assert_eq!(<DPadPathResult as WireStruct>::SIZE, 0x18);
        assert_eq!(core::mem::offset_of!(DPadPathResult, path_test_result), 0x0);
        assert_eq!(core::mem::offset_of!(DPadPathResult, movement_pos), 0x10);
    }
    #[test]
    fn roundtrip() {
        let mut raw = [0u8; 0x18];
        for (i, x) in raw.iter_mut().enumerate() { *x = (i as u8).wrapping_mul(37).wrapping_add(11); }
        let v = DPadPathResult::read_le(&raw);
        let mut back = vec![0u8; raw.len()];
        v.write_le(&mut back);
        assert_eq!(&back[..], &raw[..]);   // byte-exact incl. padding (floats compared as bytes, NaN-safe)
        assert!(<DPadPathResult>::default().to_vec().iter().all(|&x| x == 0));
    }
}

/// size 0x20
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WindowInfo {
    /// float @ 0x0
    pub screen_width: f32,
    /// float @ 0x4
    pub screen_height: f32,
    /// float @ 0x8
    pub half_screen_width: f32,
    /// float @ 0xc
    pub half_screen_height: f32,
    /// float @ 0x10
    pub padding_left: f32,
    /// float @ 0x14
    pub padding_right: f32,
    /// float @ 0x18
    pub padding_top: f32,
    /// float @ 0x1c
    pub padding_bottom: f32,
}
impl WireStruct for WindowInfo {
    const SIZE: usize = 0x20;
    fn write_le(&self, out: &mut [u8]) {
        assert!(out.len() >= Self::SIZE);
        out[..Self::SIZE].fill(0);
        out[0x0..0x4].copy_from_slice(&self.screen_width.to_le_bytes());
        out[0x4..0x8].copy_from_slice(&self.screen_height.to_le_bytes());
        out[0x8..0xc].copy_from_slice(&self.half_screen_width.to_le_bytes());
        out[0xc..0x10].copy_from_slice(&self.half_screen_height.to_le_bytes());
        out[0x10..0x14].copy_from_slice(&self.padding_left.to_le_bytes());
        out[0x14..0x18].copy_from_slice(&self.padding_right.to_le_bytes());
        out[0x18..0x1c].copy_from_slice(&self.padding_top.to_le_bytes());
        out[0x1c..0x20].copy_from_slice(&self.padding_bottom.to_le_bytes());
    }
    fn read_le(b: &[u8]) -> Self {
        assert!(b.len() >= Self::SIZE);
        Self {
            screen_width: f32::from_le_bytes(b[0x0..0x4].try_into().unwrap()),
            screen_height: f32::from_le_bytes(b[0x4..0x8].try_into().unwrap()),
            half_screen_width: f32::from_le_bytes(b[0x8..0xc].try_into().unwrap()),
            half_screen_height: f32::from_le_bytes(b[0xc..0x10].try_into().unwrap()),
            padding_left: f32::from_le_bytes(b[0x10..0x14].try_into().unwrap()),
            padding_right: f32::from_le_bytes(b[0x14..0x18].try_into().unwrap()),
            padding_top: f32::from_le_bytes(b[0x18..0x1c].try_into().unwrap()),
            padding_bottom: f32::from_le_bytes(b[0x1c..0x20].try_into().unwrap()),
        }
    }
}
impl Default for WindowInfo { fn default() -> Self { Self::read_le(&[0u8; 32]) } }
#[cfg(test)]
mod test_window_info {
    use super::*;
    #[test]
    fn layout() {
        assert_eq!(core::mem::size_of::<WindowInfo>(), 0x20);
        assert_eq!(<WindowInfo as WireStruct>::SIZE, 0x20);
        assert_eq!(core::mem::offset_of!(WindowInfo, screen_width), 0x0);
        assert_eq!(core::mem::offset_of!(WindowInfo, screen_height), 0x4);
        assert_eq!(core::mem::offset_of!(WindowInfo, half_screen_width), 0x8);
        assert_eq!(core::mem::offset_of!(WindowInfo, half_screen_height), 0xc);
        assert_eq!(core::mem::offset_of!(WindowInfo, padding_left), 0x10);
        assert_eq!(core::mem::offset_of!(WindowInfo, padding_right), 0x14);
        assert_eq!(core::mem::offset_of!(WindowInfo, padding_top), 0x18);
        assert_eq!(core::mem::offset_of!(WindowInfo, padding_bottom), 0x1c);
    }
    #[test]
    fn roundtrip() {
        let mut raw = [0u8; 0x20];
        for (i, x) in raw.iter_mut().enumerate() { *x = (i as u8).wrapping_mul(37).wrapping_add(11); }
        let v = WindowInfo::read_le(&raw);
        let mut back = vec![0u8; raw.len()];
        v.write_le(&mut back);
        assert_eq!(&back[..], &raw[..]);   // byte-exact incl. padding (floats compared as bytes, NaN-safe)
        assert!(<WindowInfo>::default().to_vec().iter().all(|&x| x == 0));
    }
}

/// size 0x40
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CustomRules {
    /// uint8_t @ 0x0
    pub custom_rules_enabled: u8,
    /// int8_t @ 0x1
    pub seed_type: i8,
    /// int8_t @ 0x2
    pub oceans: i8,
    /// int8_t @ 0x3
    pub temperature: i8,
    /// uint32_t @ 0x4
    pub custom_seed_value: u32,
    /// int8_t @ 0x8
    pub rainfall: i8,
    /// int8_t @ 0x9
    pub plants: i8,
    /// int8_t @ 0xa
    pub trees: i8,
    /// int8_t @ 0xb
    pub terrain_type: i8,
    /// int8_t @ 0xc
    pub caves: i8,
    /// int8_t @ 0xd
    pub lakes: i8,
    /// int8_t @ 0xe
    pub sky_islands: i8,
    /// int8_t @ 0xf
    pub ore_quantity: i8,
    /// int8_t @ 0x10
    pub ore_distribution: i8,
    /// int8_t @ 0x11
    pub gems: i8,
    /// int8_t @ 0x12
    pub treasure: i8,
    /// int8_t @ 0x13
    pub health: i8,
    /// int8_t @ 0x14
    pub hunger: i8,
    /// int8_t @ 0x15
    pub happiness: i8,
    /// int8_t @ 0x16
    pub trade_portals: i8,
    /// int8_t @ 0x17
    pub npc_difficulty: i8,
    /// int8_t @ 0x18
    pub spawn_item_counts: [i8; 8],
    /// uint16_t @ 0x20
    pub spawn_item_types: [u16; 8],
    /// int8_t @ 0x30
    pub clothing_decay: i8,
    /// int8_t @ 0x31
    pub tool_decay: i8,
    /// int8_t @ 0x32
    pub death: i8,
    /// int8_t @ 0x33
    pub max_blockheads: i8,
    /// int8_t @ 0x34
    pub day_night_cycle: i8,
    /// int8_t @ 0x35
    pub world_modifcation: i8,
    /// uint8_t @ 0x36
    pub sun_color: [u8; 3],
    /// int8_t @ 0x39
    pub death_drops: i8,
    /// int8_t @ 0x3a
    pub portal_prices: i8,
    /// int8_t @ 0x3b
    pub portal_discoverability: i8,
    /// int8_t @ 0x3c
    pub pole_items: i8,
    /// int8_t @ 0x3d
    pub teleportation: i8,
    /// int8_t @ 0x3e
    pub meditation: i8,
    /// implicit padding at 0x3f; preserved on read, zero by default
    pub gap_3f: [u8; 1],
}
impl WireStruct for CustomRules {
    const SIZE: usize = 0x40;
    fn write_le(&self, out: &mut [u8]) {
        assert!(out.len() >= Self::SIZE);
        out[..Self::SIZE].fill(0);
        out[0x0..0x1].copy_from_slice(&self.custom_rules_enabled.to_le_bytes());
        out[0x1..0x2].copy_from_slice(&self.seed_type.to_le_bytes());
        out[0x2..0x3].copy_from_slice(&self.oceans.to_le_bytes());
        out[0x3..0x4].copy_from_slice(&self.temperature.to_le_bytes());
        out[0x4..0x8].copy_from_slice(&self.custom_seed_value.to_le_bytes());
        out[0x8..0x9].copy_from_slice(&self.rainfall.to_le_bytes());
        out[0x9..0xa].copy_from_slice(&self.plants.to_le_bytes());
        out[0xa..0xb].copy_from_slice(&self.trees.to_le_bytes());
        out[0xb..0xc].copy_from_slice(&self.terrain_type.to_le_bytes());
        out[0xc..0xd].copy_from_slice(&self.caves.to_le_bytes());
        out[0xd..0xe].copy_from_slice(&self.lakes.to_le_bytes());
        out[0xe..0xf].copy_from_slice(&self.sky_islands.to_le_bytes());
        out[0xf..0x10].copy_from_slice(&self.ore_quantity.to_le_bytes());
        out[0x10..0x11].copy_from_slice(&self.ore_distribution.to_le_bytes());
        out[0x11..0x12].copy_from_slice(&self.gems.to_le_bytes());
        out[0x12..0x13].copy_from_slice(&self.treasure.to_le_bytes());
        out[0x13..0x14].copy_from_slice(&self.health.to_le_bytes());
        out[0x14..0x15].copy_from_slice(&self.hunger.to_le_bytes());
        out[0x15..0x16].copy_from_slice(&self.happiness.to_le_bytes());
        out[0x16..0x17].copy_from_slice(&self.trade_portals.to_le_bytes());
        out[0x17..0x18].copy_from_slice(&self.npc_difficulty.to_le_bytes());
        for (k, e) in self.spawn_item_counts.iter().enumerate() { out[0x18 + k * 1..0x18 + (k + 1) * 1].copy_from_slice(&e.to_le_bytes()); }
        for (k, e) in self.spawn_item_types.iter().enumerate() { out[0x20 + k * 2..0x20 + (k + 1) * 2].copy_from_slice(&e.to_le_bytes()); }
        out[0x30..0x31].copy_from_slice(&self.clothing_decay.to_le_bytes());
        out[0x31..0x32].copy_from_slice(&self.tool_decay.to_le_bytes());
        out[0x32..0x33].copy_from_slice(&self.death.to_le_bytes());
        out[0x33..0x34].copy_from_slice(&self.max_blockheads.to_le_bytes());
        out[0x34..0x35].copy_from_slice(&self.day_night_cycle.to_le_bytes());
        out[0x35..0x36].copy_from_slice(&self.world_modifcation.to_le_bytes());
        for (k, e) in self.sun_color.iter().enumerate() { out[0x36 + k * 1..0x36 + (k + 1) * 1].copy_from_slice(&e.to_le_bytes()); }
        out[0x39..0x3a].copy_from_slice(&self.death_drops.to_le_bytes());
        out[0x3a..0x3b].copy_from_slice(&self.portal_prices.to_le_bytes());
        out[0x3b..0x3c].copy_from_slice(&self.portal_discoverability.to_le_bytes());
        out[0x3c..0x3d].copy_from_slice(&self.pole_items.to_le_bytes());
        out[0x3d..0x3e].copy_from_slice(&self.teleportation.to_le_bytes());
        out[0x3e..0x3f].copy_from_slice(&self.meditation.to_le_bytes());
        out[0x3f..0x40].copy_from_slice(&self.gap_3f);
    }
    fn read_le(b: &[u8]) -> Self {
        assert!(b.len() >= Self::SIZE);
        Self {
            custom_rules_enabled: u8::from_le_bytes(b[0x0..0x1].try_into().unwrap()),
            seed_type: i8::from_le_bytes(b[0x1..0x2].try_into().unwrap()),
            oceans: i8::from_le_bytes(b[0x2..0x3].try_into().unwrap()),
            temperature: i8::from_le_bytes(b[0x3..0x4].try_into().unwrap()),
            custom_seed_value: u32::from_le_bytes(b[0x4..0x8].try_into().unwrap()),
            rainfall: i8::from_le_bytes(b[0x8..0x9].try_into().unwrap()),
            plants: i8::from_le_bytes(b[0x9..0xa].try_into().unwrap()),
            trees: i8::from_le_bytes(b[0xa..0xb].try_into().unwrap()),
            terrain_type: i8::from_le_bytes(b[0xb..0xc].try_into().unwrap()),
            caves: i8::from_le_bytes(b[0xc..0xd].try_into().unwrap()),
            lakes: i8::from_le_bytes(b[0xd..0xe].try_into().unwrap()),
            sky_islands: i8::from_le_bytes(b[0xe..0xf].try_into().unwrap()),
            ore_quantity: i8::from_le_bytes(b[0xf..0x10].try_into().unwrap()),
            ore_distribution: i8::from_le_bytes(b[0x10..0x11].try_into().unwrap()),
            gems: i8::from_le_bytes(b[0x11..0x12].try_into().unwrap()),
            treasure: i8::from_le_bytes(b[0x12..0x13].try_into().unwrap()),
            health: i8::from_le_bytes(b[0x13..0x14].try_into().unwrap()),
            hunger: i8::from_le_bytes(b[0x14..0x15].try_into().unwrap()),
            happiness: i8::from_le_bytes(b[0x15..0x16].try_into().unwrap()),
            trade_portals: i8::from_le_bytes(b[0x16..0x17].try_into().unwrap()),
            npc_difficulty: i8::from_le_bytes(b[0x17..0x18].try_into().unwrap()),
            spawn_item_counts: core::array::from_fn(|k| i8::from_le_bytes(b[0x18 + k * 1..0x18 + (k + 1) * 1].try_into().unwrap())),
            spawn_item_types: core::array::from_fn(|k| u16::from_le_bytes(b[0x20 + k * 2..0x20 + (k + 1) * 2].try_into().unwrap())),
            clothing_decay: i8::from_le_bytes(b[0x30..0x31].try_into().unwrap()),
            tool_decay: i8::from_le_bytes(b[0x31..0x32].try_into().unwrap()),
            death: i8::from_le_bytes(b[0x32..0x33].try_into().unwrap()),
            max_blockheads: i8::from_le_bytes(b[0x33..0x34].try_into().unwrap()),
            day_night_cycle: i8::from_le_bytes(b[0x34..0x35].try_into().unwrap()),
            world_modifcation: i8::from_le_bytes(b[0x35..0x36].try_into().unwrap()),
            sun_color: core::array::from_fn(|k| u8::from_le_bytes(b[0x36 + k * 1..0x36 + (k + 1) * 1].try_into().unwrap())),
            death_drops: i8::from_le_bytes(b[0x39..0x3a].try_into().unwrap()),
            portal_prices: i8::from_le_bytes(b[0x3a..0x3b].try_into().unwrap()),
            portal_discoverability: i8::from_le_bytes(b[0x3b..0x3c].try_into().unwrap()),
            pole_items: i8::from_le_bytes(b[0x3c..0x3d].try_into().unwrap()),
            teleportation: i8::from_le_bytes(b[0x3d..0x3e].try_into().unwrap()),
            meditation: i8::from_le_bytes(b[0x3e..0x3f].try_into().unwrap()),
            gap_3f: b[0x3f..0x40].try_into().unwrap(),
        }
    }
}
impl Default for CustomRules { fn default() -> Self { Self::read_le(&[0u8; 64]) } }
#[cfg(test)]
mod test_custom_rules {
    use super::*;
    #[test]
    fn layout() {
        assert_eq!(core::mem::size_of::<CustomRules>(), 0x40);
        assert_eq!(<CustomRules as WireStruct>::SIZE, 0x40);
        assert_eq!(core::mem::offset_of!(CustomRules, custom_rules_enabled), 0x0);
        assert_eq!(core::mem::offset_of!(CustomRules, seed_type), 0x1);
        assert_eq!(core::mem::offset_of!(CustomRules, oceans), 0x2);
        assert_eq!(core::mem::offset_of!(CustomRules, temperature), 0x3);
        assert_eq!(core::mem::offset_of!(CustomRules, custom_seed_value), 0x4);
        assert_eq!(core::mem::offset_of!(CustomRules, rainfall), 0x8);
        assert_eq!(core::mem::offset_of!(CustomRules, plants), 0x9);
        assert_eq!(core::mem::offset_of!(CustomRules, trees), 0xa);
        assert_eq!(core::mem::offset_of!(CustomRules, terrain_type), 0xb);
        assert_eq!(core::mem::offset_of!(CustomRules, caves), 0xc);
        assert_eq!(core::mem::offset_of!(CustomRules, lakes), 0xd);
        assert_eq!(core::mem::offset_of!(CustomRules, sky_islands), 0xe);
        assert_eq!(core::mem::offset_of!(CustomRules, ore_quantity), 0xf);
        assert_eq!(core::mem::offset_of!(CustomRules, ore_distribution), 0x10);
        assert_eq!(core::mem::offset_of!(CustomRules, gems), 0x11);
        assert_eq!(core::mem::offset_of!(CustomRules, treasure), 0x12);
        assert_eq!(core::mem::offset_of!(CustomRules, health), 0x13);
        assert_eq!(core::mem::offset_of!(CustomRules, hunger), 0x14);
        assert_eq!(core::mem::offset_of!(CustomRules, happiness), 0x15);
        assert_eq!(core::mem::offset_of!(CustomRules, trade_portals), 0x16);
        assert_eq!(core::mem::offset_of!(CustomRules, npc_difficulty), 0x17);
        assert_eq!(core::mem::offset_of!(CustomRules, spawn_item_counts), 0x18);
        assert_eq!(core::mem::offset_of!(CustomRules, spawn_item_types), 0x20);
        assert_eq!(core::mem::offset_of!(CustomRules, clothing_decay), 0x30);
        assert_eq!(core::mem::offset_of!(CustomRules, tool_decay), 0x31);
        assert_eq!(core::mem::offset_of!(CustomRules, death), 0x32);
        assert_eq!(core::mem::offset_of!(CustomRules, max_blockheads), 0x33);
        assert_eq!(core::mem::offset_of!(CustomRules, day_night_cycle), 0x34);
        assert_eq!(core::mem::offset_of!(CustomRules, world_modifcation), 0x35);
        assert_eq!(core::mem::offset_of!(CustomRules, sun_color), 0x36);
        assert_eq!(core::mem::offset_of!(CustomRules, death_drops), 0x39);
        assert_eq!(core::mem::offset_of!(CustomRules, portal_prices), 0x3a);
        assert_eq!(core::mem::offset_of!(CustomRules, portal_discoverability), 0x3b);
        assert_eq!(core::mem::offset_of!(CustomRules, pole_items), 0x3c);
        assert_eq!(core::mem::offset_of!(CustomRules, teleportation), 0x3d);
        assert_eq!(core::mem::offset_of!(CustomRules, meditation), 0x3e);
    }
    #[test]
    fn roundtrip() {
        let mut raw = [0u8; 0x40];
        for (i, x) in raw.iter_mut().enumerate() { *x = (i as u8).wrapping_mul(37).wrapping_add(11); }
        let v = CustomRules::read_le(&raw);
        let mut back = vec![0u8; raw.len()];
        v.write_le(&mut back);
        assert_eq!(&back[..], &raw[..]);   // byte-exact incl. padding (floats compared as bytes, NaN-safe)
        assert!(<CustomRules>::default().to_vec().iter().all(|&x| x == 0));
    }
}

/// size 0x20
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TradeMission {
    /// uint64_t @ 0x0
    pub unique_id: u64,
    /// uint32_t @ 0x8
    pub local_time_start: u32,
    /// uint32_t @ 0xc
    pub server_time_start: u32,
    /// uint32_t @ 0x10
    pub time_in_seconds: u32,
    /// uint16_t @ 0x14
    pub item_type: u16,
    /// uint16_t @ 0x16
    pub required_item_type: u16,
    /// uint8_t @ 0x18
    pub count: u8,
    /// uint8_t @ 0x19
    pub required_item_count: u8,
    /// uint8_t @ 0x1a
    pub level: u8,
    /// uint8_t @ 0x1b
    pub hurried: u8,
    /// explicit padding at 0x1c; preserved on read, zero by default
    pub padding: [u8; 4],
}
impl WireStruct for TradeMission {
    const SIZE: usize = 0x20;
    fn write_le(&self, out: &mut [u8]) {
        assert!(out.len() >= Self::SIZE);
        out[..Self::SIZE].fill(0);
        out[0x0..0x8].copy_from_slice(&self.unique_id.to_le_bytes());
        out[0x8..0xc].copy_from_slice(&self.local_time_start.to_le_bytes());
        out[0xc..0x10].copy_from_slice(&self.server_time_start.to_le_bytes());
        out[0x10..0x14].copy_from_slice(&self.time_in_seconds.to_le_bytes());
        out[0x14..0x16].copy_from_slice(&self.item_type.to_le_bytes());
        out[0x16..0x18].copy_from_slice(&self.required_item_type.to_le_bytes());
        out[0x18..0x19].copy_from_slice(&self.count.to_le_bytes());
        out[0x19..0x1a].copy_from_slice(&self.required_item_count.to_le_bytes());
        out[0x1a..0x1b].copy_from_slice(&self.level.to_le_bytes());
        out[0x1b..0x1c].copy_from_slice(&self.hurried.to_le_bytes());
        out[0x1c..0x20].copy_from_slice(&self.padding);
    }
    fn read_le(b: &[u8]) -> Self {
        assert!(b.len() >= Self::SIZE);
        Self {
            unique_id: u64::from_le_bytes(b[0x0..0x8].try_into().unwrap()),
            local_time_start: u32::from_le_bytes(b[0x8..0xc].try_into().unwrap()),
            server_time_start: u32::from_le_bytes(b[0xc..0x10].try_into().unwrap()),
            time_in_seconds: u32::from_le_bytes(b[0x10..0x14].try_into().unwrap()),
            item_type: u16::from_le_bytes(b[0x14..0x16].try_into().unwrap()),
            required_item_type: u16::from_le_bytes(b[0x16..0x18].try_into().unwrap()),
            count: u8::from_le_bytes(b[0x18..0x19].try_into().unwrap()),
            required_item_count: u8::from_le_bytes(b[0x19..0x1a].try_into().unwrap()),
            level: u8::from_le_bytes(b[0x1a..0x1b].try_into().unwrap()),
            hurried: u8::from_le_bytes(b[0x1b..0x1c].try_into().unwrap()),
            padding: b[0x1c..0x20].try_into().unwrap(),
        }
    }
}
impl Default for TradeMission { fn default() -> Self { Self::read_le(&[0u8; 32]) } }
#[cfg(test)]
mod test_trade_mission {
    use super::*;
    #[test]
    fn layout() {
        assert_eq!(core::mem::size_of::<TradeMission>(), 0x20);
        assert_eq!(<TradeMission as WireStruct>::SIZE, 0x20);
        assert_eq!(core::mem::offset_of!(TradeMission, unique_id), 0x0);
        assert_eq!(core::mem::offset_of!(TradeMission, local_time_start), 0x8);
        assert_eq!(core::mem::offset_of!(TradeMission, server_time_start), 0xc);
        assert_eq!(core::mem::offset_of!(TradeMission, time_in_seconds), 0x10);
        assert_eq!(core::mem::offset_of!(TradeMission, item_type), 0x14);
        assert_eq!(core::mem::offset_of!(TradeMission, required_item_type), 0x16);
        assert_eq!(core::mem::offset_of!(TradeMission, count), 0x18);
        assert_eq!(core::mem::offset_of!(TradeMission, required_item_count), 0x19);
        assert_eq!(core::mem::offset_of!(TradeMission, level), 0x1a);
        assert_eq!(core::mem::offset_of!(TradeMission, hurried), 0x1b);
        assert_eq!(core::mem::offset_of!(TradeMission, padding), 0x1c);
    }
    #[test]
    fn roundtrip() {
        let mut raw = [0u8; 0x20];
        for (i, x) in raw.iter_mut().enumerate() { *x = (i as u8).wrapping_mul(37).wrapping_add(11); }
        let v = TradeMission::read_le(&raw);
        let mut back = vec![0u8; raw.len()];
        v.write_le(&mut back);
        assert_eq!(&back[..], &raw[..]);   // byte-exact incl. padding (floats compared as bytes, NaN-safe)
        assert!(<TradeMission>::default().to_vec().iter().all(|&x| x == 0));
    }
}

/// size 0x8
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ItemSaveData {
    /// uint16_t @ 0x0
    pub item_type: u16,
    /// uint16_t @ 0x2
    pub data_a: u16,
    /// uint16_t @ 0x4
    pub data_b: u16,
    /// uint8_t @ 0x6
    pub selected_sub_item_index: u8,
    /// explicit padding at 0x7; preserved on read, zero by default
    pub padding: [u8; 1],
}
impl WireStruct for ItemSaveData {
    const SIZE: usize = 0x8;
    fn write_le(&self, out: &mut [u8]) {
        assert!(out.len() >= Self::SIZE);
        out[..Self::SIZE].fill(0);
        out[0x0..0x2].copy_from_slice(&self.item_type.to_le_bytes());
        out[0x2..0x4].copy_from_slice(&self.data_a.to_le_bytes());
        out[0x4..0x6].copy_from_slice(&self.data_b.to_le_bytes());
        out[0x6..0x7].copy_from_slice(&self.selected_sub_item_index.to_le_bytes());
        out[0x7..0x8].copy_from_slice(&self.padding);
    }
    fn read_le(b: &[u8]) -> Self {
        assert!(b.len() >= Self::SIZE);
        Self {
            item_type: u16::from_le_bytes(b[0x0..0x2].try_into().unwrap()),
            data_a: u16::from_le_bytes(b[0x2..0x4].try_into().unwrap()),
            data_b: u16::from_le_bytes(b[0x4..0x6].try_into().unwrap()),
            selected_sub_item_index: u8::from_le_bytes(b[0x6..0x7].try_into().unwrap()),
            padding: b[0x7..0x8].try_into().unwrap(),
        }
    }
}
impl Default for ItemSaveData { fn default() -> Self { Self::read_le(&[0u8; 8]) } }
#[cfg(test)]
mod test_item_save_data {
    use super::*;
    #[test]
    fn layout() {
        assert_eq!(core::mem::size_of::<ItemSaveData>(), 0x8);
        assert_eq!(<ItemSaveData as WireStruct>::SIZE, 0x8);
        assert_eq!(core::mem::offset_of!(ItemSaveData, item_type), 0x0);
        assert_eq!(core::mem::offset_of!(ItemSaveData, data_a), 0x2);
        assert_eq!(core::mem::offset_of!(ItemSaveData, data_b), 0x4);
        assert_eq!(core::mem::offset_of!(ItemSaveData, selected_sub_item_index), 0x6);
        assert_eq!(core::mem::offset_of!(ItemSaveData, padding), 0x7);
    }
    #[test]
    fn roundtrip() {
        let mut raw = [0u8; 0x8];
        for (i, x) in raw.iter_mut().enumerate() { *x = (i as u8).wrapping_mul(37).wrapping_add(11); }
        let v = ItemSaveData::read_le(&raw);
        let mut back = vec![0u8; raw.len()];
        v.write_le(&mut back);
        assert_eq!(&back[..], &raw[..]);   // byte-exact incl. padding (floats compared as bytes, NaN-safe)
        assert!(<ItemSaveData>::default().to_vec().iter().all(|&x| x == 0));
    }
}

/// size 0x8
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct IntPair {
    /// int @ 0x0
    pub x: i32,
    /// int @ 0x4
    pub y: i32,
}
impl WireStruct for IntPair {
    const SIZE: usize = 0x8;
    fn write_le(&self, out: &mut [u8]) {
        assert!(out.len() >= Self::SIZE);
        out[..Self::SIZE].fill(0);
        out[0x0..0x4].copy_from_slice(&self.x.to_le_bytes());
        out[0x4..0x8].copy_from_slice(&self.y.to_le_bytes());
    }
    fn read_le(b: &[u8]) -> Self {
        assert!(b.len() >= Self::SIZE);
        Self {
            x: i32::from_le_bytes(b[0x0..0x4].try_into().unwrap()),
            y: i32::from_le_bytes(b[0x4..0x8].try_into().unwrap()),
        }
    }
}
impl Default for IntPair { fn default() -> Self { Self::read_le(&[0u8; 8]) } }
#[cfg(test)]
mod test_int_pair {
    use super::*;
    #[test]
    fn layout() {
        assert_eq!(core::mem::size_of::<IntPair>(), 0x8);
        assert_eq!(<IntPair as WireStruct>::SIZE, 0x8);
        assert_eq!(core::mem::offset_of!(IntPair, x), 0x0);
        assert_eq!(core::mem::offset_of!(IntPair, y), 0x4);
    }
    #[test]
    fn roundtrip() {
        let mut raw = [0u8; 0x8];
        for (i, x) in raw.iter_mut().enumerate() { *x = (i as u8).wrapping_mul(37).wrapping_add(11); }
        let v = IntPair::read_le(&raw);
        let mut back = vec![0u8; raw.len()];
        v.write_le(&mut back);
        assert_eq!(&back[..], &raw[..]);   // byte-exact incl. padding (floats compared as bytes, NaN-safe)
        assert!(<IntPair>::default().to_vec().iter().all(|&x| x == 0));
    }
}

/// size 0x10
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MJColor {
    /// float @ 0x0
    pub r: f32,
    /// float @ 0x4
    pub g: f32,
    /// float @ 0x8
    pub b: f32,
    /// float @ 0xc
    pub a: f32,
}
impl WireStruct for MJColor {
    const SIZE: usize = 0x10;
    fn write_le(&self, out: &mut [u8]) {
        assert!(out.len() >= Self::SIZE);
        out[..Self::SIZE].fill(0);
        out[0x0..0x4].copy_from_slice(&self.r.to_le_bytes());
        out[0x4..0x8].copy_from_slice(&self.g.to_le_bytes());
        out[0x8..0xc].copy_from_slice(&self.b.to_le_bytes());
        out[0xc..0x10].copy_from_slice(&self.a.to_le_bytes());
    }
    fn read_le(b: &[u8]) -> Self {
        assert!(b.len() >= Self::SIZE);
        Self {
            r: f32::from_le_bytes(b[0x0..0x4].try_into().unwrap()),
            g: f32::from_le_bytes(b[0x4..0x8].try_into().unwrap()),
            b: f32::from_le_bytes(b[0x8..0xc].try_into().unwrap()),
            a: f32::from_le_bytes(b[0xc..0x10].try_into().unwrap()),
        }
    }
}
impl Default for MJColor { fn default() -> Self { Self::read_le(&[0u8; 16]) } }
#[cfg(test)]
mod test_mjcolor {
    use super::*;
    #[test]
    fn layout() {
        assert_eq!(core::mem::size_of::<MJColor>(), 0x10);
        assert_eq!(<MJColor as WireStruct>::SIZE, 0x10);
        assert_eq!(core::mem::offset_of!(MJColor, r), 0x0);
        assert_eq!(core::mem::offset_of!(MJColor, g), 0x4);
        assert_eq!(core::mem::offset_of!(MJColor, b), 0x8);
        assert_eq!(core::mem::offset_of!(MJColor, a), 0xc);
    }
    #[test]
    fn roundtrip() {
        let mut raw = [0u8; 0x10];
        for (i, x) in raw.iter_mut().enumerate() { *x = (i as u8).wrapping_mul(37).wrapping_add(11); }
        let v = MJColor::read_le(&raw);
        let mut back = vec![0u8; raw.len()];
        v.write_le(&mut back);
        assert_eq!(&back[..], &raw[..]);   // byte-exact incl. padding (floats compared as bytes, NaN-safe)
        assert!(<MJColor>::default().to_vec().iter().all(|&x| x == 0));
    }
}

/// size 0x4
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct MJColor8 {
    /// uint8_t @ 0x0
    pub r: u8,
    /// uint8_t @ 0x1
    pub g: u8,
    /// uint8_t @ 0x2
    pub b: u8,
    /// uint8_t @ 0x3
    pub a: u8,
}
impl WireStruct for MJColor8 {
    const SIZE: usize = 0x4;
    fn write_le(&self, out: &mut [u8]) {
        assert!(out.len() >= Self::SIZE);
        out[..Self::SIZE].fill(0);
        out[0x0..0x1].copy_from_slice(&self.r.to_le_bytes());
        out[0x1..0x2].copy_from_slice(&self.g.to_le_bytes());
        out[0x2..0x3].copy_from_slice(&self.b.to_le_bytes());
        out[0x3..0x4].copy_from_slice(&self.a.to_le_bytes());
    }
    fn read_le(b: &[u8]) -> Self {
        assert!(b.len() >= Self::SIZE);
        Self {
            r: u8::from_le_bytes(b[0x0..0x1].try_into().unwrap()),
            g: u8::from_le_bytes(b[0x1..0x2].try_into().unwrap()),
            b: u8::from_le_bytes(b[0x2..0x3].try_into().unwrap()),
            a: u8::from_le_bytes(b[0x3..0x4].try_into().unwrap()),
        }
    }
}
impl Default for MJColor8 { fn default() -> Self { Self::read_le(&[0u8; 4]) } }
#[cfg(test)]
mod test_mjcolor8 {
    use super::*;
    #[test]
    fn layout() {
        assert_eq!(core::mem::size_of::<MJColor8>(), 0x4);
        assert_eq!(<MJColor8 as WireStruct>::SIZE, 0x4);
        assert_eq!(core::mem::offset_of!(MJColor8, r), 0x0);
        assert_eq!(core::mem::offset_of!(MJColor8, g), 0x1);
        assert_eq!(core::mem::offset_of!(MJColor8, b), 0x2);
        assert_eq!(core::mem::offset_of!(MJColor8, a), 0x3);
    }
    #[test]
    fn roundtrip() {
        let mut raw = [0u8; 0x4];
        for (i, x) in raw.iter_mut().enumerate() { *x = (i as u8).wrapping_mul(37).wrapping_add(11); }
        let v = MJColor8::read_le(&raw);
        let mut back = vec![0u8; raw.len()];
        v.write_le(&mut back);
        assert_eq!(&back[..], &raw[..]);   // byte-exact incl. padding (floats compared as bytes, NaN-safe)
        assert!(<MJColor8>::default().to_vec().iter().all(|&x| x == 0));
    }
}

/// size 0x68
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Particle {
    /// intpair @ 0x0
    pub intp: IntPair,
    /// vec3 @ 0x8
    pub p: Vector,
    /// vec3 @ 0x18
    pub center: Vector,
    /// vec3 @ 0x28
    pub connection_goal: Vector,
    /// vec3 @ 0x38
    pub v: Vector,
    /// vec4 @ 0x48
    pub color: Vector,
    /// uint8_t @ 0x58
    pub gravity_type: u8,
    /// uint8_t @ 0x59
    pub has_back_wall: u8,
    /// implicit padding at 0x5a; preserved on read, zero by default
    pub gap_5a: [u8; 2],
    /// float @ 0x5c
    pub life: f32,
    /// float @ 0x60
    pub scale: f32,
    /// float @ 0x64
    pub other_timer: f32,
}
impl WireStruct for Particle {
    const SIZE: usize = 0x68;
    fn write_le(&self, out: &mut [u8]) {
        assert!(out.len() >= Self::SIZE);
        out[..Self::SIZE].fill(0);
        self.intp.write_le(&mut out[0x0..]);
        self.p.write_le(&mut out[0x8..]);
        self.center.write_le(&mut out[0x18..]);
        self.connection_goal.write_le(&mut out[0x28..]);
        self.v.write_le(&mut out[0x38..]);
        self.color.write_le(&mut out[0x48..]);
        out[0x58..0x59].copy_from_slice(&self.gravity_type.to_le_bytes());
        out[0x59..0x5a].copy_from_slice(&self.has_back_wall.to_le_bytes());
        out[0x5a..0x5c].copy_from_slice(&self.gap_5a);
        out[0x5c..0x60].copy_from_slice(&self.life.to_le_bytes());
        out[0x60..0x64].copy_from_slice(&self.scale.to_le_bytes());
        out[0x64..0x68].copy_from_slice(&self.other_timer.to_le_bytes());
    }
    fn read_le(b: &[u8]) -> Self {
        assert!(b.len() >= Self::SIZE);
        Self {
            intp: IntPair::read_le(&b[0x0..]),
            p: Vector::read_le(&b[0x8..]),
            center: Vector::read_le(&b[0x18..]),
            connection_goal: Vector::read_le(&b[0x28..]),
            v: Vector::read_le(&b[0x38..]),
            color: Vector::read_le(&b[0x48..]),
            gravity_type: u8::from_le_bytes(b[0x58..0x59].try_into().unwrap()),
            has_back_wall: u8::from_le_bytes(b[0x59..0x5a].try_into().unwrap()),
            gap_5a: b[0x5a..0x5c].try_into().unwrap(),
            life: f32::from_le_bytes(b[0x5c..0x60].try_into().unwrap()),
            scale: f32::from_le_bytes(b[0x60..0x64].try_into().unwrap()),
            other_timer: f32::from_le_bytes(b[0x64..0x68].try_into().unwrap()),
        }
    }
}
impl Default for Particle { fn default() -> Self { Self::read_le(&[0u8; 104]) } }
#[cfg(test)]
mod test_particle {
    use super::*;
    #[test]
    fn layout() {
        assert_eq!(core::mem::size_of::<Particle>(), 0x68);
        assert_eq!(<Particle as WireStruct>::SIZE, 0x68);
        assert_eq!(core::mem::offset_of!(Particle, intp), 0x0);
        assert_eq!(core::mem::offset_of!(Particle, p), 0x8);
        assert_eq!(core::mem::offset_of!(Particle, center), 0x18);
        assert_eq!(core::mem::offset_of!(Particle, connection_goal), 0x28);
        assert_eq!(core::mem::offset_of!(Particle, v), 0x38);
        assert_eq!(core::mem::offset_of!(Particle, color), 0x48);
        assert_eq!(core::mem::offset_of!(Particle, gravity_type), 0x58);
        assert_eq!(core::mem::offset_of!(Particle, has_back_wall), 0x59);
        assert_eq!(core::mem::offset_of!(Particle, life), 0x5c);
        assert_eq!(core::mem::offset_of!(Particle, scale), 0x60);
        assert_eq!(core::mem::offset_of!(Particle, other_timer), 0x64);
    }
    #[test]
    fn roundtrip() {
        let mut raw = [0u8; 0x68];
        for (i, x) in raw.iter_mut().enumerate() { *x = (i as u8).wrapping_mul(37).wrapping_add(11); }
        let v = Particle::read_le(&raw);
        let mut back = vec![0u8; raw.len()];
        v.write_le(&mut back);
        assert_eq!(&back[..], &raw[..]);   // byte-exact incl. padding (floats compared as bytes, NaN-safe)
        assert!(<Particle>::default().to_vec().iter().all(|&x| x == 0));
    }
}

/// size 0x28
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BonusParticle {
    /// vec3 @ 0x0
    pub p: Vector,
    /// vec4 @ 0x10
    pub color: Vector,
    /// int @ 0x20
    pub bonus_multiplier_index: i32,
    /// float @ 0x24
    pub life: f32,
}
impl WireStruct for BonusParticle {
    const SIZE: usize = 0x28;
    fn write_le(&self, out: &mut [u8]) {
        assert!(out.len() >= Self::SIZE);
        out[..Self::SIZE].fill(0);
        self.p.write_le(&mut out[0x0..]);
        self.color.write_le(&mut out[0x10..]);
        out[0x20..0x24].copy_from_slice(&self.bonus_multiplier_index.to_le_bytes());
        out[0x24..0x28].copy_from_slice(&self.life.to_le_bytes());
    }
    fn read_le(b: &[u8]) -> Self {
        assert!(b.len() >= Self::SIZE);
        Self {
            p: Vector::read_le(&b[0x0..]),
            color: Vector::read_le(&b[0x10..]),
            bonus_multiplier_index: i32::from_le_bytes(b[0x20..0x24].try_into().unwrap()),
            life: f32::from_le_bytes(b[0x24..0x28].try_into().unwrap()),
        }
    }
}
impl Default for BonusParticle { fn default() -> Self { Self::read_le(&[0u8; 40]) } }
#[cfg(test)]
mod test_bonus_particle {
    use super::*;
    #[test]
    fn layout() {
        assert_eq!(core::mem::size_of::<BonusParticle>(), 0x28);
        assert_eq!(<BonusParticle as WireStruct>::SIZE, 0x28);
        assert_eq!(core::mem::offset_of!(BonusParticle, p), 0x0);
        assert_eq!(core::mem::offset_of!(BonusParticle, color), 0x10);
        assert_eq!(core::mem::offset_of!(BonusParticle, bonus_multiplier_index), 0x20);
        assert_eq!(core::mem::offset_of!(BonusParticle, life), 0x24);
    }
    #[test]
    fn roundtrip() {
        let mut raw = [0u8; 0x28];
        for (i, x) in raw.iter_mut().enumerate() { *x = (i as u8).wrapping_mul(37).wrapping_add(11); }
        let v = BonusParticle::read_le(&raw);
        let mut back = vec![0u8; raw.len()];
        v.write_le(&mut back);
        assert_eq!(&back[..], &raw[..]);   // byte-exact incl. padding (floats compared as bytes, NaN-safe)
        assert!(<BonusParticle>::default().to_vec().iter().all(|&x| x == 0));
    }
}

/// size 0x8
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ElectrictyParticlePathIndex {
    /// uint32_t @ 0x0
    pub index: u32,
    /// uint8_t @ 0x4
    pub solid: u8,
    /// explicit padding at 0x5; preserved on read, zero by default
    pub padding: [u8; 3],
}
impl WireStruct for ElectrictyParticlePathIndex {
    const SIZE: usize = 0x8;
    fn write_le(&self, out: &mut [u8]) {
        assert!(out.len() >= Self::SIZE);
        out[..Self::SIZE].fill(0);
        out[0x0..0x4].copy_from_slice(&self.index.to_le_bytes());
        out[0x4..0x5].copy_from_slice(&self.solid.to_le_bytes());
        out[0x5..0x8].copy_from_slice(&self.padding);
    }
    fn read_le(b: &[u8]) -> Self {
        assert!(b.len() >= Self::SIZE);
        Self {
            index: u32::from_le_bytes(b[0x0..0x4].try_into().unwrap()),
            solid: u8::from_le_bytes(b[0x4..0x5].try_into().unwrap()),
            padding: b[0x5..0x8].try_into().unwrap(),
        }
    }
}
impl Default for ElectrictyParticlePathIndex { fn default() -> Self { Self::read_le(&[0u8; 8]) } }
#[cfg(test)]
mod test_electricty_particle_path_index {
    use super::*;
    #[test]
    fn layout() {
        assert_eq!(core::mem::size_of::<ElectrictyParticlePathIndex>(), 0x8);
        assert_eq!(<ElectrictyParticlePathIndex as WireStruct>::SIZE, 0x8);
        assert_eq!(core::mem::offset_of!(ElectrictyParticlePathIndex, index), 0x0);
        assert_eq!(core::mem::offset_of!(ElectrictyParticlePathIndex, solid), 0x4);
        assert_eq!(core::mem::offset_of!(ElectrictyParticlePathIndex, padding), 0x5);
    }
    #[test]
    fn roundtrip() {
        let mut raw = [0u8; 0x8];
        for (i, x) in raw.iter_mut().enumerate() { *x = (i as u8).wrapping_mul(37).wrapping_add(11); }
        let v = ElectrictyParticlePathIndex::read_le(&raw);
        let mut back = vec![0u8; raw.len()];
        v.write_le(&mut back);
        assert_eq!(&back[..], &raw[..]);   // byte-exact incl. padding (floats compared as bytes, NaN-safe)
        assert!(<ElectrictyParticlePathIndex>::default().to_vec().iter().all(|&x| x == 0));
    }
}

/// size 0x10
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PathTestResult {
    /// enum TileTraverseType @ 0x0
    pub traverse_type: u32,
    /// enum TileTraverseKeyFrameType @ 0x4
    pub traverse_to_key_frame: u32,
    /// int @ 0x8
    pub terrain_difficulty: i32,
    /// BOOL @ 0xc
    pub failed_due_to_no_tile: u8,
    /// implicit padding at 0xd; preserved on read, zero by default
    pub gap_d: [u8; 3],
}
impl WireStruct for PathTestResult {
    const SIZE: usize = 0x10;
    fn write_le(&self, out: &mut [u8]) {
        assert!(out.len() >= Self::SIZE);
        out[..Self::SIZE].fill(0);
        out[0x0..0x4].copy_from_slice(&self.traverse_type.to_le_bytes());
        out[0x4..0x8].copy_from_slice(&self.traverse_to_key_frame.to_le_bytes());
        out[0x8..0xc].copy_from_slice(&self.terrain_difficulty.to_le_bytes());
        out[0xc..0xd].copy_from_slice(&self.failed_due_to_no_tile.to_le_bytes());
        out[0xd..0x10].copy_from_slice(&self.gap_d);
    }
    fn read_le(b: &[u8]) -> Self {
        assert!(b.len() >= Self::SIZE);
        Self {
            traverse_type: u32::from_le_bytes(b[0x0..0x4].try_into().unwrap()),
            traverse_to_key_frame: u32::from_le_bytes(b[0x4..0x8].try_into().unwrap()),
            terrain_difficulty: i32::from_le_bytes(b[0x8..0xc].try_into().unwrap()),
            failed_due_to_no_tile: u8::from_le_bytes(b[0xc..0xd].try_into().unwrap()),
            gap_d: b[0xd..0x10].try_into().unwrap(),
        }
    }
}
impl Default for PathTestResult { fn default() -> Self { Self::read_le(&[0u8; 16]) } }
#[cfg(test)]
mod test_path_test_result {
    use super::*;
    #[test]
    fn layout() {
        assert_eq!(core::mem::size_of::<PathTestResult>(), 0x10);
        assert_eq!(<PathTestResult as WireStruct>::SIZE, 0x10);
        assert_eq!(core::mem::offset_of!(PathTestResult, traverse_type), 0x0);
        assert_eq!(core::mem::offset_of!(PathTestResult, traverse_to_key_frame), 0x4);
        assert_eq!(core::mem::offset_of!(PathTestResult, terrain_difficulty), 0x8);
        assert_eq!(core::mem::offset_of!(PathTestResult, failed_due_to_no_tile), 0xc);
    }
    #[test]
    fn roundtrip() {
        let mut raw = [0u8; 0x10];
        for (i, x) in raw.iter_mut().enumerate() { *x = (i as u8).wrapping_mul(37).wrapping_add(11); }
        let v = PathTestResult::read_le(&raw);
        let mut back = vec![0u8; raw.len()];
        v.write_le(&mut back);
        assert_eq!(&back[..], &raw[..]);   // byte-exact incl. padding (floats compared as bytes, NaN-safe)
        assert!(<PathTestResult>::default().to_vec().iter().all(|&x| x == 0));
    }
}

/// size 0xc
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TreeFruit {
    /// intpair @ 0x0
    pub pos: IntPair,
    /// BOOL @ 0x8
    pub has_created_free_block_this_season: u8,
    /// implicit padding at 0x9; preserved on read, zero by default
    pub gap_9: [u8; 3],
}
impl WireStruct for TreeFruit {
    const SIZE: usize = 0xc;
    fn write_le(&self, out: &mut [u8]) {
        assert!(out.len() >= Self::SIZE);
        out[..Self::SIZE].fill(0);
        self.pos.write_le(&mut out[0x0..]);
        out[0x8..0x9].copy_from_slice(&self.has_created_free_block_this_season.to_le_bytes());
        out[0x9..0xc].copy_from_slice(&self.gap_9);
    }
    fn read_le(b: &[u8]) -> Self {
        assert!(b.len() >= Self::SIZE);
        Self {
            pos: IntPair::read_le(&b[0x0..]),
            has_created_free_block_this_season: u8::from_le_bytes(b[0x8..0x9].try_into().unwrap()),
            gap_9: b[0x9..0xc].try_into().unwrap(),
        }
    }
}
impl Default for TreeFruit { fn default() -> Self { Self::read_le(&[0u8; 12]) } }
#[cfg(test)]
mod test_tree_fruit {
    use super::*;
    #[test]
    fn layout() {
        assert_eq!(core::mem::size_of::<TreeFruit>(), 0xc);
        assert_eq!(<TreeFruit as WireStruct>::SIZE, 0xc);
        assert_eq!(core::mem::offset_of!(TreeFruit, pos), 0x0);
        assert_eq!(core::mem::offset_of!(TreeFruit, has_created_free_block_this_season), 0x8);
    }
    #[test]
    fn roundtrip() {
        let mut raw = [0u8; 0xc];
        for (i, x) in raw.iter_mut().enumerate() { *x = (i as u8).wrapping_mul(37).wrapping_add(11); }
        let v = TreeFruit::read_le(&raw);
        let mut back = vec![0u8; raw.len()];
        v.write_le(&mut back);
        assert_eq!(&back[..], &raw[..]);   // byte-exact incl. padding (floats compared as bytes, NaN-safe)
        assert!(<TreeFruit>::default().to_vec().iter().all(|&x| x == 0));
    }
}

/// size 0x18
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Cloud {
    /// float @ 0x0
    pub x: f32,
    /// float @ 0x4
    pub y: f32,
    /// float @ 0x8
    pub z: f32,
    /// float @ 0xc
    pub z_fraction: f32,
    /// float @ 0x10
    pub scale: f32,
    /// int @ 0x14
    pub type_: i32,
}
impl WireStruct for Cloud {
    const SIZE: usize = 0x18;
    fn write_le(&self, out: &mut [u8]) {
        assert!(out.len() >= Self::SIZE);
        out[..Self::SIZE].fill(0);
        out[0x0..0x4].copy_from_slice(&self.x.to_le_bytes());
        out[0x4..0x8].copy_from_slice(&self.y.to_le_bytes());
        out[0x8..0xc].copy_from_slice(&self.z.to_le_bytes());
        out[0xc..0x10].copy_from_slice(&self.z_fraction.to_le_bytes());
        out[0x10..0x14].copy_from_slice(&self.scale.to_le_bytes());
        out[0x14..0x18].copy_from_slice(&self.type_.to_le_bytes());
    }
    fn read_le(b: &[u8]) -> Self {
        assert!(b.len() >= Self::SIZE);
        Self {
            x: f32::from_le_bytes(b[0x0..0x4].try_into().unwrap()),
            y: f32::from_le_bytes(b[0x4..0x8].try_into().unwrap()),
            z: f32::from_le_bytes(b[0x8..0xc].try_into().unwrap()),
            z_fraction: f32::from_le_bytes(b[0xc..0x10].try_into().unwrap()),
            scale: f32::from_le_bytes(b[0x10..0x14].try_into().unwrap()),
            type_: i32::from_le_bytes(b[0x14..0x18].try_into().unwrap()),
        }
    }
}
impl Default for Cloud { fn default() -> Self { Self::read_le(&[0u8; 24]) } }
#[cfg(test)]
mod test_cloud {
    use super::*;
    #[test]
    fn layout() {
        assert_eq!(core::mem::size_of::<Cloud>(), 0x18);
        assert_eq!(<Cloud as WireStruct>::SIZE, 0x18);
        assert_eq!(core::mem::offset_of!(Cloud, x), 0x0);
        assert_eq!(core::mem::offset_of!(Cloud, y), 0x4);
        assert_eq!(core::mem::offset_of!(Cloud, z), 0x8);
        assert_eq!(core::mem::offset_of!(Cloud, z_fraction), 0xc);
        assert_eq!(core::mem::offset_of!(Cloud, scale), 0x10);
        assert_eq!(core::mem::offset_of!(Cloud, type_), 0x14);
    }
    #[test]
    fn roundtrip() {
        let mut raw = [0u8; 0x18];
        for (i, x) in raw.iter_mut().enumerate() { *x = (i as u8).wrapping_mul(37).wrapping_add(11); }
        let v = Cloud::read_le(&raw);
        let mut back = vec![0u8; raw.len()];
        v.write_le(&mut back);
        assert_eq!(&back[..], &raw[..]);   // byte-exact incl. padding (floats compared as bytes, NaN-safe)
        assert!(<Cloud>::default().to_vec().iter().all(|&x| x == 0));
    }
}

/// size 0xc
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct WirePathTileProperties {
    /// int @ 0x0
    pub f: i32,
    /// int @ 0x4
    pub g: i32,
    /// int @ 0x8
    pub parent_index: i32,
}
impl WireStruct for WirePathTileProperties {
    const SIZE: usize = 0xc;
    fn write_le(&self, out: &mut [u8]) {
        assert!(out.len() >= Self::SIZE);
        out[..Self::SIZE].fill(0);
        out[0x0..0x4].copy_from_slice(&self.f.to_le_bytes());
        out[0x4..0x8].copy_from_slice(&self.g.to_le_bytes());
        out[0x8..0xc].copy_from_slice(&self.parent_index.to_le_bytes());
    }
    fn read_le(b: &[u8]) -> Self {
        assert!(b.len() >= Self::SIZE);
        Self {
            f: i32::from_le_bytes(b[0x0..0x4].try_into().unwrap()),
            g: i32::from_le_bytes(b[0x4..0x8].try_into().unwrap()),
            parent_index: i32::from_le_bytes(b[0x8..0xc].try_into().unwrap()),
        }
    }
}
impl Default for WirePathTileProperties { fn default() -> Self { Self::read_le(&[0u8; 12]) } }
#[cfg(test)]
mod test_wire_path_tile_properties {
    use super::*;
    #[test]
    fn layout() {
        assert_eq!(core::mem::size_of::<WirePathTileProperties>(), 0xc);
        assert_eq!(<WirePathTileProperties as WireStruct>::SIZE, 0xc);
        assert_eq!(core::mem::offset_of!(WirePathTileProperties, f), 0x0);
        assert_eq!(core::mem::offset_of!(WirePathTileProperties, g), 0x4);
        assert_eq!(core::mem::offset_of!(WirePathTileProperties, parent_index), 0x8);
    }
    #[test]
    fn roundtrip() {
        let mut raw = [0u8; 0xc];
        for (i, x) in raw.iter_mut().enumerate() { *x = (i as u8).wrapping_mul(37).wrapping_add(11); }
        let v = WirePathTileProperties::read_le(&raw);
        let mut back = vec![0u8; raw.len()];
        v.write_le(&mut back);
        assert_eq!(&back[..], &raw[..]);   // byte-exact incl. padding (floats compared as bytes, NaN-safe)
        assert!(<WirePathTileProperties>::default().to_vec().iter().all(|&x| x == 0));
    }
}

/// size 0x8
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct DrawBlock {
    /// int @ 0x0
    pub x_pos: i32,
    /// int @ 0x4
    pub y_pos: i32,
}
impl WireStruct for DrawBlock {
    const SIZE: usize = 0x8;
    fn write_le(&self, out: &mut [u8]) {
        assert!(out.len() >= Self::SIZE);
        out[..Self::SIZE].fill(0);
        out[0x0..0x4].copy_from_slice(&self.x_pos.to_le_bytes());
        out[0x4..0x8].copy_from_slice(&self.y_pos.to_le_bytes());
    }
    fn read_le(b: &[u8]) -> Self {
        assert!(b.len() >= Self::SIZE);
        Self {
            x_pos: i32::from_le_bytes(b[0x0..0x4].try_into().unwrap()),
            y_pos: i32::from_le_bytes(b[0x4..0x8].try_into().unwrap()),
        }
    }
}
impl Default for DrawBlock { fn default() -> Self { Self::read_le(&[0u8; 8]) } }
#[cfg(test)]
mod test_draw_block {
    use super::*;
    #[test]
    fn layout() {
        assert_eq!(core::mem::size_of::<DrawBlock>(), 0x8);
        assert_eq!(<DrawBlock as WireStruct>::SIZE, 0x8);
        assert_eq!(core::mem::offset_of!(DrawBlock, x_pos), 0x0);
        assert_eq!(core::mem::offset_of!(DrawBlock, y_pos), 0x4);
    }
    #[test]
    fn roundtrip() {
        let mut raw = [0u8; 0x8];
        for (i, x) in raw.iter_mut().enumerate() { *x = (i as u8).wrapping_mul(37).wrapping_add(11); }
        let v = DrawBlock::read_le(&raw);
        let mut back = vec![0u8; raw.len()];
        v.write_le(&mut back);
        assert_eq!(&back[..], &raw[..]);   // byte-exact incl. padding (floats compared as bytes, NaN-safe)
        assert!(<DrawBlock>::default().to_vec().iter().all(|&x| x == 0));
    }
}

