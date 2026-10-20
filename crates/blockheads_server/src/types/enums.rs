// @generated layout table (wire/save format). DO NOT EDIT.
#![allow(non_upper_case_globals, dead_code)]

#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct LightDirectionType(pub u32);
impl LightDirectionType {
    pub const LIGHT_DIRECTION_ALL: LightDirectionType = LightDirectionType(0);
    pub const LIGHT_DIRECTION_DOWN: LightDirectionType = LightDirectionType(1);
    pub const LIGHT_DIRECTION_UP: LightDirectionType = LightDirectionType(2);
    #[inline]
    pub const fn raw(self) -> u32 {
        self.0
    }
    #[inline]
    pub const fn from_raw(v: u32) -> Self {
        Self(v)
    }
    /// First-declared constant name for this value (None for unknown/unnamed values).
    pub fn name(self) -> Option<&'static str> {
        match self.0 {
            0 => Some("LIGHT_DIRECTION_ALL"),
            1 => Some("LIGHT_DIRECTION_DOWN"),
            2 => Some("LIGHT_DIRECTION_UP"),
            _ => None,
        }
    }
}
impl core::fmt::Debug for LightDirectionType {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self.name() {
            Some(n) => write!(f, "{}", n),
            None => write!(f, "LightDirectionType({})", self.0),
        }
    }
}
impl From<u32> for LightDirectionType {
    fn from(v: u32) -> Self {
        Self(v)
    }
}
impl From<LightDirectionType> for u32 {
    fn from(v: LightDirectionType) -> u32 {
        v.0
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct ListType(pub u32);
impl ListType {
    pub const LIST_TYPE_BLACKLIST: ListType = ListType(0);
    pub const LIST_TYPE_WHITELIST: ListType = ListType(1);
    pub const LIST_TYPE_ADMIN: ListType = ListType(2);
    pub const LIST_TYPE_MOD: ListType = ListType(3);
    #[inline]
    pub const fn raw(self) -> u32 {
        self.0
    }
    #[inline]
    pub const fn from_raw(v: u32) -> Self {
        Self(v)
    }
    /// First-declared constant name for this value (None for unknown/unnamed values).
    pub fn name(self) -> Option<&'static str> {
        match self.0 {
            0 => Some("LIST_TYPE_BLACKLIST"),
            1 => Some("LIST_TYPE_WHITELIST"),
            2 => Some("LIST_TYPE_ADMIN"),
            3 => Some("LIST_TYPE_MOD"),
            _ => None,
        }
    }
}
impl core::fmt::Debug for ListType {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self.name() {
            Some(n) => write!(f, "{}", n),
            None => write!(f, "ListType({})", self.0),
        }
    }
}
impl From<u32> for ListType {
    fn from(v: u32) -> Self {
        Self(v)
    }
}
impl From<ListType> for u32 {
    fn from(v: ListType) -> u32 {
        v.0
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct BlockheadAnimationType(pub u32);
impl BlockheadAnimationType {
    pub const VA_STAND: BlockheadAnimationType = BlockheadAnimationType(0);
    pub const VA_WALK: BlockheadAnimationType = BlockheadAnimationType(1);
    pub const VA_STAND_TO_HANG_JUMP: BlockheadAnimationType = BlockheadAnimationType(2);
    pub const VA_HANG_TO_STAND_JUMP: BlockheadAnimationType = BlockheadAnimationType(3);
    pub const VA_CLIMB: BlockheadAnimationType = BlockheadAnimationType(4);
    pub const VA_SWIM: BlockheadAnimationType = BlockheadAnimationType(5);
    pub const VA_CRAWL: BlockheadAnimationType = BlockheadAnimationType(6);
    pub const VA_HANG: BlockheadAnimationType = BlockheadAnimationType(7);
    pub const VA_HANG_FALL_BALANCE: BlockheadAnimationType = BlockheadAnimationType(8);
    pub const VA_CROUCH_UP: BlockheadAnimationType = BlockheadAnimationType(9);
    pub const VA_CROUCH_DOWN: BlockheadAnimationType = BlockheadAnimationType(10);
    pub const VA_CROUCH_CRAWL_UP: BlockheadAnimationType = BlockheadAnimationType(11);
    pub const VA_CROUCH_CRAWL_DOWN: BlockheadAnimationType = BlockheadAnimationType(12);
    pub const VA_MONKEY_SWING: BlockheadAnimationType = BlockheadAnimationType(13);
    pub const VA_MONKEY_HANG: BlockheadAnimationType = BlockheadAnimationType(14);
    pub const VA_STAND_TO_HANG_DOUBLE_JUMP: BlockheadAnimationType = BlockheadAnimationType(15);
    pub const VA_STAND_TO_MONKEY_HANG_JUMP: BlockheadAnimationType = BlockheadAnimationType(16);
    pub const VA_MONKEY_HANG_TO_STAND_JUMP: BlockheadAnimationType = BlockheadAnimationType(17);
    pub const VA_CLIMB_DOWN_TO_MONKEY_SWING: BlockheadAnimationType = BlockheadAnimationType(18);
    pub const VA_MONKEY_SWING_UP_TO_CLIMB: BlockheadAnimationType = BlockheadAnimationType(19);
    pub const VA_HANG_TO_MONKEY_SWING_DIAGONAL_JUMP_UP: BlockheadAnimationType = BlockheadAnimationType(20);
    pub const VA_MONKEY_SWING_TO_HANG_DIAGONAL_JUMP_DOWN: BlockheadAnimationType = BlockheadAnimationType(21);
    pub const VA_HANG_TO_HANG_JUMP: BlockheadAnimationType = BlockheadAnimationType(22);
    pub const VA_LIE: BlockheadAnimationType = BlockheadAnimationType(23);
    pub const VA_BOB: BlockheadAnimationType = BlockheadAnimationType(24);
    pub const VA_FALL: BlockheadAnimationType = BlockheadAnimationType(25);
    pub const VA_JUMP_OVER_GAP: BlockheadAnimationType = BlockheadAnimationType(26);
    pub const VA_RIDE: BlockheadAnimationType = BlockheadAnimationType(27);
    pub const VA_RIDE_HANDCAR: BlockheadAnimationType = BlockheadAnimationType(28);
    pub const VA_RIDE_LOCOMOTIVE: BlockheadAnimationType = BlockheadAnimationType(29);
    pub const VA_RIDE_PASSENGER_CAR: BlockheadAnimationType = BlockheadAnimationType(30);
    pub const VA_RIDE_ELEVATOR: BlockheadAnimationType = BlockheadAnimationType(31);
    pub const VA_FLY_JETPACK: BlockheadAnimationType = BlockheadAnimationType(32);
    pub const VA_RIDE_ARMS_DOWN: BlockheadAnimationType = BlockheadAnimationType(33);
    #[inline]
    pub const fn raw(self) -> u32 {
        self.0
    }
    #[inline]
    pub const fn from_raw(v: u32) -> Self {
        Self(v)
    }
    /// First-declared constant name for this value (None for unknown/unnamed values).
    pub fn name(self) -> Option<&'static str> {
        match self.0 {
            0 => Some("VA_STAND"),
            1 => Some("VA_WALK"),
            2 => Some("VA_STAND_TO_HANG_JUMP"),
            3 => Some("VA_HANG_TO_STAND_JUMP"),
            4 => Some("VA_CLIMB"),
            5 => Some("VA_SWIM"),
            6 => Some("VA_CRAWL"),
            7 => Some("VA_HANG"),
            8 => Some("VA_HANG_FALL_BALANCE"),
            9 => Some("VA_CROUCH_UP"),
            10 => Some("VA_CROUCH_DOWN"),
            11 => Some("VA_CROUCH_CRAWL_UP"),
            12 => Some("VA_CROUCH_CRAWL_DOWN"),
            13 => Some("VA_MONKEY_SWING"),
            14 => Some("VA_MONKEY_HANG"),
            15 => Some("VA_STAND_TO_HANG_DOUBLE_JUMP"),
            16 => Some("VA_STAND_TO_MONKEY_HANG_JUMP"),
            17 => Some("VA_MONKEY_HANG_TO_STAND_JUMP"),
            18 => Some("VA_CLIMB_DOWN_TO_MONKEY_SWING"),
            19 => Some("VA_MONKEY_SWING_UP_TO_CLIMB"),
            20 => Some("VA_HANG_TO_MONKEY_SWING_DIAGONAL_JUMP_UP"),
            21 => Some("VA_MONKEY_SWING_TO_HANG_DIAGONAL_JUMP_DOWN"),
            22 => Some("VA_HANG_TO_HANG_JUMP"),
            23 => Some("VA_LIE"),
            24 => Some("VA_BOB"),
            25 => Some("VA_FALL"),
            26 => Some("VA_JUMP_OVER_GAP"),
            27 => Some("VA_RIDE"),
            28 => Some("VA_RIDE_HANDCAR"),
            29 => Some("VA_RIDE_LOCOMOTIVE"),
            30 => Some("VA_RIDE_PASSENGER_CAR"),
            31 => Some("VA_RIDE_ELEVATOR"),
            32 => Some("VA_FLY_JETPACK"),
            33 => Some("VA_RIDE_ARMS_DOWN"),
            _ => None,
        }
    }
}
impl core::fmt::Debug for BlockheadAnimationType {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self.name() {
            Some(n) => write!(f, "{}", n),
            None => write!(f, "BlockheadAnimationType({})", self.0),
        }
    }
}
impl From<u32> for BlockheadAnimationType {
    fn from(v: u32) -> Self {
        Self(v)
    }
}
impl From<BlockheadAnimationType> for u32 {
    fn from(v: BlockheadAnimationType) -> u32 {
        v.0
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct BlockheadSubAnimationType(pub u32);
impl BlockheadSubAnimationType {
    pub const VSA_NONE: BlockheadSubAnimationType = BlockheadSubAnimationType(0);
    pub const VSA_DIG: BlockheadSubAnimationType = BlockheadSubAnimationType(1);
    pub const VSA_CRAFT: BlockheadSubAnimationType = BlockheadSubAnimationType(2);
    pub const VSA_STAB: BlockheadSubAnimationType = BlockheadSubAnimationType(3);
    pub const VSA_LOOK_LEFT_RIGHT_UP: BlockheadSubAnimationType = BlockheadSubAnimationType(4);
    pub const VSA_LIE_ON_BED: BlockheadSubAnimationType = BlockheadSubAnimationType(5);
    pub const VSA_LIE_ON_GROUND: BlockheadSubAnimationType = BlockheadSubAnimationType(6);
    pub const VSA_MEDITATING: BlockheadSubAnimationType = BlockheadSubAnimationType(7);
    pub const VSA_FIRE_ARROW: BlockheadSubAnimationType = BlockheadSubAnimationType(8);
    #[inline]
    pub const fn raw(self) -> u32 {
        self.0
    }
    #[inline]
    pub const fn from_raw(v: u32) -> Self {
        Self(v)
    }
    /// First-declared constant name for this value (None for unknown/unnamed values).
    pub fn name(self) -> Option<&'static str> {
        match self.0 {
            0 => Some("VSA_NONE"),
            1 => Some("VSA_DIG"),
            2 => Some("VSA_CRAFT"),
            3 => Some("VSA_STAB"),
            4 => Some("VSA_LOOK_LEFT_RIGHT_UP"),
            5 => Some("VSA_LIE_ON_BED"),
            6 => Some("VSA_LIE_ON_GROUND"),
            7 => Some("VSA_MEDITATING"),
            8 => Some("VSA_FIRE_ARROW"),
            _ => None,
        }
    }
}
impl core::fmt::Debug for BlockheadSubAnimationType {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self.name() {
            Some(n) => write!(f, "{}", n),
            None => write!(f, "BlockheadSubAnimationType({})", self.0),
        }
    }
}
impl From<u32> for BlockheadSubAnimationType {
    fn from(v: u32) -> Self {
        Self(v)
    }
}
impl From<BlockheadSubAnimationType> for u32 {
    fn from(v: BlockheadSubAnimationType) -> u32 {
        v.0
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct BlockheadUnableToWorkReason(pub u32);
impl BlockheadUnableToWorkReason {
    pub const WBBH_ABLE: BlockheadUnableToWorkReason = BlockheadUnableToWorkReason(0);
    pub const WBBH_TIRED: BlockheadUnableToWorkReason = BlockheadUnableToWorkReason(1);
    pub const WBBH_STARVING: BlockheadUnableToWorkReason = BlockheadUnableToWorkReason(2);
    #[inline]
    pub const fn raw(self) -> u32 {
        self.0
    }
    #[inline]
    pub const fn from_raw(v: u32) -> Self {
        Self(v)
    }
    /// First-declared constant name for this value (None for unknown/unnamed values).
    pub fn name(self) -> Option<&'static str> {
        match self.0 {
            0 => Some("WBBH_ABLE"),
            1 => Some("WBBH_TIRED"),
            2 => Some("WBBH_STARVING"),
            _ => None,
        }
    }
}
impl core::fmt::Debug for BlockheadUnableToWorkReason {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self.name() {
            Some(n) => write!(f, "{}", n),
            None => write!(f, "BlockheadUnableToWorkReason({})", self.0),
        }
    }
}
impl From<u32> for BlockheadUnableToWorkReason {
    fn from(v: u32) -> Self {
        Self(v)
    }
}
impl From<BlockheadUnableToWorkReason> for u32 {
    fn from(v: BlockheadUnableToWorkReason) -> u32 {
        v.0
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct ActionFoundType(pub u32);
impl ActionFoundType {
    pub const AI_ACTION_SLEEP_IN_BED_IF_NEEDED: ActionFoundType = ActionFoundType(0);
    pub const AI_ACTION_SLEEP_ON_SPOT_IF_NEEDED: ActionFoundType = ActionFoundType(1);
    pub const AI_ACTION_GET_AIR: ActionFoundType = ActionFoundType(2);
    pub const AI_ACTION_DIG_HOLE: ActionFoundType = ActionFoundType(3);
    pub const AI_ACTION_GATHER_GEMS: ActionFoundType = ActionFoundType(4);
    pub const AI_ACTION_PLACE_TORCH: ActionFoundType = ActionFoundType(5);
    pub const AI_ACTION_MINE_RESOURCE_WITH_TOOL: ActionFoundType = ActionFoundType(6);
    pub const AI_ACTION_GATHER_FOOD: ActionFoundType = ActionFoundType(7);
    pub const AI_ACTION_GATHER_FOOD_WITH_TOOL: ActionFoundType = ActionFoundType(8);
    pub const AI_ACTION_MOVE: ActionFoundType = ActionFoundType(9);
    pub const AI_ACTION_PLANT_FOOD: ActionFoundType = ActionFoundType(10);
    pub const AI_ACTION_EXPLORE_UNEXPLORED_AREAS: ActionFoundType = ActionFoundType(11);
    pub const AI_ACTION_GATHER_FOOD_WITH_HANDS: ActionFoundType = ActionFoundType(12);
    pub const AI_ACTION_GET_PINE_STICKS_WITH_TOOL: ActionFoundType = ActionFoundType(13);
    pub const AI_ACTION_GET_PINE_STICKS: ActionFoundType = ActionFoundType(14);
    pub const AI_ACTION_MINE_RESOURCE_WITH_HANDS: ActionFoundType = ActionFoundType(15);
    pub const AI_MAX_ACTIONS: ActionFoundType = ActionFoundType(16);
    #[inline]
    pub const fn raw(self) -> u32 {
        self.0
    }
    #[inline]
    pub const fn from_raw(v: u32) -> Self {
        Self(v)
    }
    /// First-declared constant name for this value (None for unknown/unnamed values).
    pub fn name(self) -> Option<&'static str> {
        match self.0 {
            0 => Some("AI_ACTION_SLEEP_IN_BED_IF_NEEDED"),
            1 => Some("AI_ACTION_SLEEP_ON_SPOT_IF_NEEDED"),
            2 => Some("AI_ACTION_GET_AIR"),
            3 => Some("AI_ACTION_DIG_HOLE"),
            4 => Some("AI_ACTION_GATHER_GEMS"),
            5 => Some("AI_ACTION_PLACE_TORCH"),
            6 => Some("AI_ACTION_MINE_RESOURCE_WITH_TOOL"),
            7 => Some("AI_ACTION_GATHER_FOOD"),
            8 => Some("AI_ACTION_GATHER_FOOD_WITH_TOOL"),
            9 => Some("AI_ACTION_MOVE"),
            10 => Some("AI_ACTION_PLANT_FOOD"),
            11 => Some("AI_ACTION_EXPLORE_UNEXPLORED_AREAS"),
            12 => Some("AI_ACTION_GATHER_FOOD_WITH_HANDS"),
            13 => Some("AI_ACTION_GET_PINE_STICKS_WITH_TOOL"),
            14 => Some("AI_ACTION_GET_PINE_STICKS"),
            15 => Some("AI_ACTION_MINE_RESOURCE_WITH_HANDS"),
            16 => Some("AI_MAX_ACTIONS"),
            _ => None,
        }
    }
}
impl core::fmt::Debug for ActionFoundType {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self.name() {
            Some(n) => write!(f, "{}", n),
            None => write!(f, "ActionFoundType({})", self.0),
        }
    }
}
impl From<u32> for ActionFoundType {
    fn from(v: u32) -> Self {
        Self(v)
    }
}
impl From<ActionFoundType> for u32 {
    fn from(v: ActionFoundType) -> u32 {
        v.0
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct CaveTrollAnimationType(pub u32);
impl CaveTrollAnimationType {
    pub const CTA_STAND: CaveTrollAnimationType = CaveTrollAnimationType(0);
    pub const CTA_WALK: CaveTrollAnimationType = CaveTrollAnimationType(1);
    pub const CTA_STAND_TO_HANG_JUMP: CaveTrollAnimationType = CaveTrollAnimationType(2);
    pub const CTA_HANG_TO_STAND_JUMP: CaveTrollAnimationType = CaveTrollAnimationType(3);
    pub const CTA_CLIMB: CaveTrollAnimationType = CaveTrollAnimationType(4);
    pub const CTA_SWIM: CaveTrollAnimationType = CaveTrollAnimationType(5);
    pub const CTA_CRAWL: CaveTrollAnimationType = CaveTrollAnimationType(6);
    pub const CTA_HANG: CaveTrollAnimationType = CaveTrollAnimationType(7);
    pub const CTA_HANG_FALL_BALANCE: CaveTrollAnimationType = CaveTrollAnimationType(8);
    pub const CTA_CROUCH_UP: CaveTrollAnimationType = CaveTrollAnimationType(9);
    pub const CTA_CROUCH_DOWN: CaveTrollAnimationType = CaveTrollAnimationType(10);
    pub const CTA_CROUCH_CRAWL_UP: CaveTrollAnimationType = CaveTrollAnimationType(11);
    pub const CTA_CROUCH_CRAWL_DOWN: CaveTrollAnimationType = CaveTrollAnimationType(12);
    pub const CTA_MONKEY_SWING: CaveTrollAnimationType = CaveTrollAnimationType(13);
    pub const CTA_MONKEY_HANG: CaveTrollAnimationType = CaveTrollAnimationType(14);
    pub const CTA_STAND_TO_HANG_DOUBLE_JUMP: CaveTrollAnimationType = CaveTrollAnimationType(15);
    pub const CTA_STAND_TO_MONKEY_HANG_JUMP: CaveTrollAnimationType = CaveTrollAnimationType(16);
    pub const CTA_MONKEY_HANG_TO_STAND_JUMP: CaveTrollAnimationType = CaveTrollAnimationType(17);
    pub const CTA_CLIMB_DOWN_TO_MONKEY_SWING: CaveTrollAnimationType = CaveTrollAnimationType(18);
    pub const CTA_MONKEY_SWING_UP_TO_CLIMB: CaveTrollAnimationType = CaveTrollAnimationType(19);
    pub const CTA_HANG_TO_MONKEY_SWING_DIAGONAL_JUMP_UP: CaveTrollAnimationType = CaveTrollAnimationType(20);
    pub const CTA_MONKEY_SWING_TO_HANG_DIAGONAL_JUMP_DOWN: CaveTrollAnimationType = CaveTrollAnimationType(21);
    pub const CTA_HANG_TO_HANG_JUMP: CaveTrollAnimationType = CaveTrollAnimationType(22);
    pub const CTA_LIE: CaveTrollAnimationType = CaveTrollAnimationType(23);
    pub const CTA_BOB: CaveTrollAnimationType = CaveTrollAnimationType(24);
    pub const CTA_FALL: CaveTrollAnimationType = CaveTrollAnimationType(25);
    pub const CTA_JUMP_OVER_GAP: CaveTrollAnimationType = CaveTrollAnimationType(26);
    pub const CTA_RIDE: CaveTrollAnimationType = CaveTrollAnimationType(27);
    #[inline]
    pub const fn raw(self) -> u32 {
        self.0
    }
    #[inline]
    pub const fn from_raw(v: u32) -> Self {
        Self(v)
    }
    /// First-declared constant name for this value (None for unknown/unnamed values).
    pub fn name(self) -> Option<&'static str> {
        match self.0 {
            0 => Some("CTA_STAND"),
            1 => Some("CTA_WALK"),
            2 => Some("CTA_STAND_TO_HANG_JUMP"),
            3 => Some("CTA_HANG_TO_STAND_JUMP"),
            4 => Some("CTA_CLIMB"),
            5 => Some("CTA_SWIM"),
            6 => Some("CTA_CRAWL"),
            7 => Some("CTA_HANG"),
            8 => Some("CTA_HANG_FALL_BALANCE"),
            9 => Some("CTA_CROUCH_UP"),
            10 => Some("CTA_CROUCH_DOWN"),
            11 => Some("CTA_CROUCH_CRAWL_UP"),
            12 => Some("CTA_CROUCH_CRAWL_DOWN"),
            13 => Some("CTA_MONKEY_SWING"),
            14 => Some("CTA_MONKEY_HANG"),
            15 => Some("CTA_STAND_TO_HANG_DOUBLE_JUMP"),
            16 => Some("CTA_STAND_TO_MONKEY_HANG_JUMP"),
            17 => Some("CTA_MONKEY_HANG_TO_STAND_JUMP"),
            18 => Some("CTA_CLIMB_DOWN_TO_MONKEY_SWING"),
            19 => Some("CTA_MONKEY_SWING_UP_TO_CLIMB"),
            20 => Some("CTA_HANG_TO_MONKEY_SWING_DIAGONAL_JUMP_UP"),
            21 => Some("CTA_MONKEY_SWING_TO_HANG_DIAGONAL_JUMP_DOWN"),
            22 => Some("CTA_HANG_TO_HANG_JUMP"),
            23 => Some("CTA_LIE"),
            24 => Some("CTA_BOB"),
            25 => Some("CTA_FALL"),
            26 => Some("CTA_JUMP_OVER_GAP"),
            27 => Some("CTA_RIDE"),
            _ => None,
        }
    }
}
impl core::fmt::Debug for CaveTrollAnimationType {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self.name() {
            Some(n) => write!(f, "{}", n),
            None => write!(f, "CaveTrollAnimationType({})", self.0),
        }
    }
}
impl From<u32> for CaveTrollAnimationType {
    fn from(v: u32) -> Self {
        Self(v)
    }
}
impl From<CaveTrollAnimationType> for u32 {
    fn from(v: CaveTrollAnimationType) -> u32 {
        v.0
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct CaveTrollSubAnimationType(pub u32);
impl CaveTrollSubAnimationType {
    pub const CTSA_NONE: CaveTrollSubAnimationType = CaveTrollSubAnimationType(0);
    pub const CTSA_DIG: CaveTrollSubAnimationType = CaveTrollSubAnimationType(1);
    pub const CTSA_CRAFT: CaveTrollSubAnimationType = CaveTrollSubAnimationType(2);
    pub const CTSA_STAB: CaveTrollSubAnimationType = CaveTrollSubAnimationType(3);
    pub const CTSA_LOOK_LEFT_RIGHT_UP: CaveTrollSubAnimationType = CaveTrollSubAnimationType(4);
    pub const CTSA_LIE_ON_BED: CaveTrollSubAnimationType = CaveTrollSubAnimationType(5);
    pub const CTSA_LIE_ON_GROUND: CaveTrollSubAnimationType = CaveTrollSubAnimationType(6);
    pub const CTSA_MEDITATING: CaveTrollSubAnimationType = CaveTrollSubAnimationType(7);
    #[inline]
    pub const fn raw(self) -> u32 {
        self.0
    }
    #[inline]
    pub const fn from_raw(v: u32) -> Self {
        Self(v)
    }
    /// First-declared constant name for this value (None for unknown/unnamed values).
    pub fn name(self) -> Option<&'static str> {
        match self.0 {
            0 => Some("CTSA_NONE"),
            1 => Some("CTSA_DIG"),
            2 => Some("CTSA_CRAFT"),
            3 => Some("CTSA_STAB"),
            4 => Some("CTSA_LOOK_LEFT_RIGHT_UP"),
            5 => Some("CTSA_LIE_ON_BED"),
            6 => Some("CTSA_LIE_ON_GROUND"),
            7 => Some("CTSA_MEDITATING"),
            _ => None,
        }
    }
}
impl core::fmt::Debug for CaveTrollSubAnimationType {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self.name() {
            Some(n) => write!(f, "{}", n),
            None => write!(f, "CaveTrollSubAnimationType({})", self.0),
        }
    }
}
impl From<u32> for CaveTrollSubAnimationType {
    fn from(v: u32) -> Self {
        Self(v)
    }
}
impl From<CaveTrollSubAnimationType> for u32 {
    fn from(v: CaveTrollSubAnimationType) -> u32 {
        v.0
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct ChestType(pub u32);
impl ChestType {
    pub const CHEST_STANDARD: ChestType = ChestType(0);
    pub const CHEST_SAFE: ChestType = ChestType(1);
    pub const CHEST_SHELF: ChestType = ChestType(2);
    pub const CHEST_GOLD: ChestType = ChestType(3);
    pub const CHEST_PORTAL: ChestType = ChestType(4);
    pub const CHEST_DISPLAY_CABINET: ChestType = ChestType(5);
    pub const CHEST_FEEDER: ChestType = ChestType(6);
    #[inline]
    pub const fn raw(self) -> u32 {
        self.0
    }
    #[inline]
    pub const fn from_raw(v: u32) -> Self {
        Self(v)
    }
    /// First-declared constant name for this value (None for unknown/unnamed values).
    pub fn name(self) -> Option<&'static str> {
        match self.0 {
            0 => Some("CHEST_STANDARD"),
            1 => Some("CHEST_SAFE"),
            2 => Some("CHEST_SHELF"),
            3 => Some("CHEST_GOLD"),
            4 => Some("CHEST_PORTAL"),
            5 => Some("CHEST_DISPLAY_CABINET"),
            6 => Some("CHEST_FEEDER"),
            _ => None,
        }
    }
}
impl core::fmt::Debug for ChestType {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self.name() {
            Some(n) => write!(f, "{}", n),
            None => write!(f, "ChestType({})", self.0),
        }
    }
}
impl From<u32> for ChestType {
    fn from(v: u32) -> Self {
        Self(v)
    }
}
impl From<ChestType> for u32 {
    fn from(v: ChestType) -> u32 {
        v.0
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct ColumnConfiguration(pub u32);
impl ColumnConfiguration {
    pub const COLUMN_UNDEFINED: ColumnConfiguration = ColumnConfiguration(0);
    pub const COLUMN_NO_PLINTH: ColumnConfiguration = ColumnConfiguration(1);
    pub const COLUMN_PLINTH_BELOW: ColumnConfiguration = ColumnConfiguration(2);
    pub const COLUMN_PLINTH_ABOVE: ColumnConfiguration = ColumnConfiguration(3);
    pub const COLUMN_PLINTH_ABOVE_AND_BELOW: ColumnConfiguration = ColumnConfiguration(4);
    #[inline]
    pub const fn raw(self) -> u32 {
        self.0
    }
    #[inline]
    pub const fn from_raw(v: u32) -> Self {
        Self(v)
    }
    /// First-declared constant name for this value (None for unknown/unnamed values).
    pub fn name(self) -> Option<&'static str> {
        match self.0 {
            0 => Some("COLUMN_UNDEFINED"),
            1 => Some("COLUMN_NO_PLINTH"),
            2 => Some("COLUMN_PLINTH_BELOW"),
            3 => Some("COLUMN_PLINTH_ABOVE"),
            4 => Some("COLUMN_PLINTH_ABOVE_AND_BELOW"),
            _ => None,
        }
    }
}
impl core::fmt::Debug for ColumnConfiguration {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self.name() {
            Some(n) => write!(f, "{}", n),
            None => write!(f, "ColumnConfiguration({})", self.0),
        }
    }
}
impl From<u32> for ColumnConfiguration {
    fn from(v: u32) -> Self {
        Self(v)
    }
}
impl From<ColumnConfiguration> for u32 {
    fn from(v: ColumnConfiguration) -> u32 {
        v.0
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct DodoBreed(pub u16);
impl DodoBreed {
    pub const DODO_BREED_STANDARD: DodoBreed = DodoBreed(0);
    pub const DODO_BREED_STONE: DodoBreed = DodoBreed(1);
    pub const DODO_BREED_LIMESTONE: DodoBreed = DodoBreed(2);
    pub const DODO_BREED_SANDSTONE: DodoBreed = DodoBreed(3);
    pub const DODO_BREED_MARBLE: DodoBreed = DodoBreed(4);
    pub const DODO_BREED_RED_MARBLE: DodoBreed = DodoBreed(5);
    pub const DODO_BREED_LAPIS: DodoBreed = DodoBreed(6);
    pub const DODO_BREED_DIRT: DodoBreed = DodoBreed(7);
    pub const DODO_BREED_COMPOST: DodoBreed = DodoBreed(8);
    pub const DODO_BREED_WOOD: DodoBreed = DodoBreed(9);
    pub const DODO_BREED_GRAVEL: DodoBreed = DodoBreed(10);
    pub const DODO_BREED_SAND: DodoBreed = DodoBreed(11);
    pub const DODO_BREED_BLACK_SAND: DodoBreed = DodoBreed(12);
    pub const DODO_BREED_GLASS: DodoBreed = DodoBreed(13);
    pub const DODO_BREED_BLACK_GLASS: DodoBreed = DodoBreed(14);
    pub const DODO_BREED_CLAY: DodoBreed = DodoBreed(15);
    pub const DODO_BREED_RED_BRICK: DodoBreed = DodoBreed(16);
    pub const DODO_BREED_FLINT: DodoBreed = DodoBreed(17);
    pub const DODO_BREED_COAL: DodoBreed = DodoBreed(18);
    pub const DODO_BREED_OIL: DodoBreed = DodoBreed(19);
    pub const DODO_BREED_FUEL: DodoBreed = DodoBreed(20);
    pub const DODO_BREED_COPPER: DodoBreed = DodoBreed(21);
    pub const DODO_BREED_TIN: DodoBreed = DodoBreed(22);
    pub const DODO_BREED_IRON: DodoBreed = DodoBreed(23);
    pub const DODO_BREED_GOLD: DodoBreed = DodoBreed(24);
    pub const DODO_BREED_TITANIUM: DodoBreed = DodoBreed(25);
    pub const DODO_BREED_PLATINUM: DodoBreed = DodoBreed(26);
    pub const DODO_BREED_AMETHYST: DodoBreed = DodoBreed(27);
    pub const DODO_BREED_SAPPHIRE: DodoBreed = DodoBreed(28);
    pub const DODO_BREED_EMERALD: DodoBreed = DodoBreed(29);
    pub const DODO_BREED_RUBY: DodoBreed = DodoBreed(30);
    pub const DODO_BREED_DIAMOND: DodoBreed = DodoBreed(31);
    pub const DODO_BREED_RAINBOW: DodoBreed = DodoBreed(32);
    pub const MAX_DODO_BREEDS: DodoBreed = DodoBreed(33);
    #[inline]
    pub const fn raw(self) -> u16 {
        self.0
    }
    #[inline]
    pub const fn from_raw(v: u16) -> Self {
        Self(v)
    }
    /// First-declared constant name for this value (None for unknown/unnamed values).
    pub fn name(self) -> Option<&'static str> {
        match self.0 {
            0 => Some("DODO_BREED_STANDARD"),
            1 => Some("DODO_BREED_STONE"),
            2 => Some("DODO_BREED_LIMESTONE"),
            3 => Some("DODO_BREED_SANDSTONE"),
            4 => Some("DODO_BREED_MARBLE"),
            5 => Some("DODO_BREED_RED_MARBLE"),
            6 => Some("DODO_BREED_LAPIS"),
            7 => Some("DODO_BREED_DIRT"),
            8 => Some("DODO_BREED_COMPOST"),
            9 => Some("DODO_BREED_WOOD"),
            10 => Some("DODO_BREED_GRAVEL"),
            11 => Some("DODO_BREED_SAND"),
            12 => Some("DODO_BREED_BLACK_SAND"),
            13 => Some("DODO_BREED_GLASS"),
            14 => Some("DODO_BREED_BLACK_GLASS"),
            15 => Some("DODO_BREED_CLAY"),
            16 => Some("DODO_BREED_RED_BRICK"),
            17 => Some("DODO_BREED_FLINT"),
            18 => Some("DODO_BREED_COAL"),
            19 => Some("DODO_BREED_OIL"),
            20 => Some("DODO_BREED_FUEL"),
            21 => Some("DODO_BREED_COPPER"),
            22 => Some("DODO_BREED_TIN"),
            23 => Some("DODO_BREED_IRON"),
            24 => Some("DODO_BREED_GOLD"),
            25 => Some("DODO_BREED_TITANIUM"),
            26 => Some("DODO_BREED_PLATINUM"),
            27 => Some("DODO_BREED_AMETHYST"),
            28 => Some("DODO_BREED_SAPPHIRE"),
            29 => Some("DODO_BREED_EMERALD"),
            30 => Some("DODO_BREED_RUBY"),
            31 => Some("DODO_BREED_DIAMOND"),
            32 => Some("DODO_BREED_RAINBOW"),
            33 => Some("MAX_DODO_BREEDS"),
            _ => None,
        }
    }
}
impl core::fmt::Debug for DodoBreed {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self.name() {
            Some(n) => write!(f, "{}", n),
            None => write!(f, "DodoBreed({})", self.0),
        }
    }
}
impl From<u16> for DodoBreed {
    fn from(v: u16) -> Self {
        Self(v)
    }
}
impl From<DodoBreed> for u16 {
    fn from(v: DodoBreed) -> u16 {
        v.0
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct DonkeyBreed(pub u16);
impl DonkeyBreed {
    pub const DONKEY_BREED_STANDARD: DonkeyBreed = DonkeyBreed(0);
    pub const DONKEY_BREED_BROWN: DonkeyBreed = DonkeyBreed(1);
    pub const DONKEY_BREED_BLACK: DonkeyBreed = DonkeyBreed(2);
    pub const DONKEY_BREED_BLUE: DonkeyBreed = DonkeyBreed(3);
    pub const DONKEY_BREED_GREEN: DonkeyBreed = DonkeyBreed(4);
    pub const DONKEY_BREED_YELLOW: DonkeyBreed = DonkeyBreed(5);
    pub const DONKEY_BREED_ORANGE: DonkeyBreed = DonkeyBreed(6);
    pub const DONKEY_BREED_RED: DonkeyBreed = DonkeyBreed(7);
    pub const DONKEY_BREED_PURPLE: DonkeyBreed = DonkeyBreed(8);
    pub const DONKEY_BREED_PINK: DonkeyBreed = DonkeyBreed(9);
    pub const DONKEY_BREED_WHITE: DonkeyBreed = DonkeyBreed(10);
    pub const DONKEY_BREED_RAINBOW: DonkeyBreed = DonkeyBreed(11);
    pub const DONKEY_BREED_UNICORN_GREY: DonkeyBreed = DonkeyBreed(12);
    pub const DONKEY_BREED_UNICORN_BROWN: DonkeyBreed = DonkeyBreed(13);
    pub const DONKEY_BREED_UNICORN_BLACK: DonkeyBreed = DonkeyBreed(14);
    pub const DONKEY_BREED_UNICORN_BLUE: DonkeyBreed = DonkeyBreed(15);
    pub const DONKEY_BREED_UNICORN_GREEN: DonkeyBreed = DonkeyBreed(16);
    pub const DONKEY_BREED_UNICORN_YELLOW: DonkeyBreed = DonkeyBreed(17);
    pub const DONKEY_BREED_UNICORN_ORANGE: DonkeyBreed = DonkeyBreed(18);
    pub const DONKEY_BREED_UNICORN_RED: DonkeyBreed = DonkeyBreed(19);
    pub const DONKEY_BREED_UNICORN_PURPLE: DonkeyBreed = DonkeyBreed(20);
    pub const DONKEY_BREED_UNICORN_PINK: DonkeyBreed = DonkeyBreed(21);
    pub const DONKEY_BREED_UNICORN_WHITE: DonkeyBreed = DonkeyBreed(22);
    pub const DONKEY_BREED_UNICORN_RAINBOW: DonkeyBreed = DonkeyBreed(23);
    pub const MAX_DONKEY_BREEDS: DonkeyBreed = DonkeyBreed(24);
    #[inline]
    pub const fn raw(self) -> u16 {
        self.0
    }
    #[inline]
    pub const fn from_raw(v: u16) -> Self {
        Self(v)
    }
    /// First-declared constant name for this value (None for unknown/unnamed values).
    pub fn name(self) -> Option<&'static str> {
        match self.0 {
            0 => Some("DONKEY_BREED_STANDARD"),
            1 => Some("DONKEY_BREED_BROWN"),
            2 => Some("DONKEY_BREED_BLACK"),
            3 => Some("DONKEY_BREED_BLUE"),
            4 => Some("DONKEY_BREED_GREEN"),
            5 => Some("DONKEY_BREED_YELLOW"),
            6 => Some("DONKEY_BREED_ORANGE"),
            7 => Some("DONKEY_BREED_RED"),
            8 => Some("DONKEY_BREED_PURPLE"),
            9 => Some("DONKEY_BREED_PINK"),
            10 => Some("DONKEY_BREED_WHITE"),
            11 => Some("DONKEY_BREED_RAINBOW"),
            12 => Some("DONKEY_BREED_UNICORN_GREY"),
            13 => Some("DONKEY_BREED_UNICORN_BROWN"),
            14 => Some("DONKEY_BREED_UNICORN_BLACK"),
            15 => Some("DONKEY_BREED_UNICORN_BLUE"),
            16 => Some("DONKEY_BREED_UNICORN_GREEN"),
            17 => Some("DONKEY_BREED_UNICORN_YELLOW"),
            18 => Some("DONKEY_BREED_UNICORN_ORANGE"),
            19 => Some("DONKEY_BREED_UNICORN_RED"),
            20 => Some("DONKEY_BREED_UNICORN_PURPLE"),
            21 => Some("DONKEY_BREED_UNICORN_PINK"),
            22 => Some("DONKEY_BREED_UNICORN_WHITE"),
            23 => Some("DONKEY_BREED_UNICORN_RAINBOW"),
            24 => Some("MAX_DONKEY_BREEDS"),
            _ => None,
        }
    }
}
impl core::fmt::Debug for DonkeyBreed {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self.name() {
            Some(n) => write!(f, "{}", n),
            None => write!(f, "DonkeyBreed({})", self.0),
        }
    }
}
impl From<u16> for DonkeyBreed {
    fn from(v: u16) -> Self {
        Self(v)
    }
}
impl From<DonkeyBreed> for u16 {
    fn from(v: DonkeyBreed) -> u16 {
        v.0
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct DynamicObjectType(pub u32);
impl DynamicObjectType {
    pub const DYNAMIC_OBJECT_TYPE_UNDEFINED: DynamicObjectType = DynamicObjectType(0);
    pub const DYNAMIC_OBJECT_TYPE_APPLE_TREE: DynamicObjectType = DynamicObjectType(1);
    pub const DYNAMIC_OBJECT_TYPE_MAPLE_TREE: DynamicObjectType = DynamicObjectType(2);
    pub const DYNAMIC_OBJECT_TYPE_MANGO_TREE: DynamicObjectType = DynamicObjectType(3);
    pub const DYNAMIC_OBJECT_TYPE_PINE_TREE: DynamicObjectType = DynamicObjectType(4);
    pub const DYNAMIC_OBJECT_TYPE_CACTUS_TREE: DynamicObjectType = DynamicObjectType(5);
    pub const DYNAMIC_OBJECT_TYPE_COCONUT_TREE: DynamicObjectType = DynamicObjectType(6);
    pub const DYNAMIC_OBJECT_TYPE_ORANGE_TREE: DynamicObjectType = DynamicObjectType(7);
    pub const DYNAMIC_OBJECT_TYPE_CHERRY_TREE: DynamicObjectType = DynamicObjectType(8);
    pub const DYNAMIC_OBJECT_TYPE_COFFEE_TREE: DynamicObjectType = DynamicObjectType(9);
    pub const DYNAMIC_OBJECT_TYPE_FLAX_PLANT: DynamicObjectType = DynamicObjectType(10);
    pub const DYNAMIC_OBJECT_TYPE_SUNFLOWER_PLANT: DynamicObjectType = DynamicObjectType(11);
    pub const DYNAMIC_OBJECT_TYPE_CORN_PLANT: DynamicObjectType = DynamicObjectType(12);
    pub const DYNAMIC_OBJECT_TYPE_NPC_DODO: DynamicObjectType = DynamicObjectType(13);
    pub const DYNAMIC_OBJECT_TYPE_FREE_BLOCK: DynamicObjectType = DynamicObjectType(14);
    pub const DYNAMIC_OBJECT_TYPE_INTERACTION_OBJECT_DEPRECATED: DynamicObjectType = DynamicObjectType(15);
    pub const DYNAMIC_OBJECT_TYPE_FIRE: DynamicObjectType = DynamicObjectType(16);
    pub const DYNAMIC_OBJECT_TYPE_TORCH: DynamicObjectType = DynamicObjectType(17);
    pub const DYNAMIC_OBJECT_TYPE_GLOWBLOCK: DynamicObjectType = DynamicObjectType(18);
    pub const DYNAMIC_OBJECT_TYPE_LADDER: DynamicObjectType = DynamicObjectType(19);
    pub const DYNAMIC_OBJECT_TYPE_DOOR: DynamicObjectType = DynamicObjectType(20);
    pub const DYNAMIC_OBJECT_TYPE_ARTIFICIAL_LIGHT: DynamicObjectType = DynamicObjectType(21);
    pub const DYNAMIC_OBJECT_TYPE_SURFACEBLOCK: DynamicObjectType = DynamicObjectType(22);
    pub const DYNAMIC_OBJECT_TYPE_BED: DynamicObjectType = DynamicObjectType(23);
    pub const DYNAMIC_OBJECT_TYPE_BLOCKHEAD: DynamicObjectType = DynamicObjectType(24);
    pub const DYNAMIC_OBJECT_TYPE_NPC_DROPBEAR: DynamicObjectType = DynamicObjectType(25);
    pub const DYNAMIC_OBJECT_TYPE_GATHERBLOCK: DynamicObjectType = DynamicObjectType(26);
    pub const DYNAMIC_OBJECT_TYPE_CARROT_PLANT: DynamicObjectType = DynamicObjectType(27);
    pub const DYNAMIC_OBJECT_TYPE_NPC_DONKEY: DynamicObjectType = DynamicObjectType(28);
    pub const DYNAMIC_OBJECT_TYPE_SNOW_SURFACEBLOCK: DynamicObjectType = DynamicObjectType(29);
    pub const DYNAMIC_OBJECT_TYPE_EGG: DynamicObjectType = DynamicObjectType(30);
    pub const DYNAMIC_OBJECT_TYPE_WINDOW: DynamicObjectType = DynamicObjectType(31);
    pub const DYNAMIC_OBJECT_TYPE_BOAT: DynamicObjectType = DynamicObjectType(32);
    pub const DYNAMIC_OBJECT_TYPE_CHILLI_PLANT: DynamicObjectType = DynamicObjectType(33);
    pub const DYNAMIC_OBJECT_TYPE_KELP_PLANT: DynamicObjectType = DynamicObjectType(34);
    pub const DYNAMIC_OBJECT_TYPE_NPC_CLOWNFISH: DynamicObjectType = DynamicObjectType(35);
    pub const DYNAMIC_OBJECT_TYPE_NPC_SHARK: DynamicObjectType = DynamicObjectType(36);
    pub const DYNAMIC_OBJECT_TYPE_LIME_TREE: DynamicObjectType = DynamicObjectType(37);
    pub const DYNAMIC_OBJECT_TYPE_WIRE: DynamicObjectType = DynamicObjectType(38);
    pub const DYNAMIC_OBJECT_TYPE_NPC_CAVETROLL: DynamicObjectType = DynamicObjectType(39);
    pub const DYNAMIC_OBJECT_TYPE_RAIL: DynamicObjectType = DynamicObjectType(40);
    pub const DYNAMIC_OBJECT_TYPE_TRAIN_CAR_HANDCAR: DynamicObjectType = DynamicObjectType(41);
    pub const DYNAMIC_OBJECT_TYPE_TRAIN_CAR_STEAM_ENGINE: DynamicObjectType = DynamicObjectType(42);
    pub const DYNAMIC_OBJECT_TYPE_TRAIN_CAR_FREIGHT: DynamicObjectType = DynamicObjectType(43);
    pub const DYNAMIC_OBJECT_TYPE_TRAIN_CAR_PASSENGER: DynamicObjectType = DynamicObjectType(44);
    pub const DYNAMIC_OBJECT_TYPE_WORKBENCH: DynamicObjectType = DynamicObjectType(45);
    pub const DYNAMIC_OBJECT_TYPE_CHEST: DynamicObjectType = DynamicObjectType(46);
    pub const DYNAMIC_OBJECT_TYPE_SIGN: DynamicObjectType = DynamicObjectType(47);
    pub const DYNAMIC_OBJECT_TYPE_TRADING_POST: DynamicObjectType = DynamicObjectType(48);
    pub const DYNAMIC_OBJECT_TYPE_TRAIN_STATION: DynamicObjectType = DynamicObjectType(49);
    pub const DYNAMIC_OBJECT_TYPE_TRADE_PORTAL: DynamicObjectType = DynamicObjectType(50);
    pub const DYNAMIC_OBJECT_TYPE_NPC_SCORPION: DynamicObjectType = DynamicObjectType(51);
    pub const DYNAMIC_OBJECT_TYPE_PAINTING: DynamicObjectType = DynamicObjectType(52);
    pub const DYNAMIC_OBJECT_TYPE_COLUMN: DynamicObjectType = DynamicObjectType(53);
    pub const DYNAMIC_OBJECT_TYPE_STAIRS: DynamicObjectType = DynamicObjectType(54);
    pub const DYNAMIC_OBJECT_TYPE_ELEVATOR_MOTOR: DynamicObjectType = DynamicObjectType(55);
    pub const DYNAMIC_OBJECT_TYPE_ELEVATOR_SHAFT: DynamicObjectType = DynamicObjectType(56);
    pub const DYNAMIC_OBJECT_TYPE_GEM_TREE: DynamicObjectType = DynamicObjectType(57);
    pub const DYNAMIC_OBJECT_TYPE_VINE_PLANT: DynamicObjectType = DynamicObjectType(58);
    pub const DYNAMIC_OBJECT_TYPE_TULIP_PLANT: DynamicObjectType = DynamicObjectType(59);
    pub const DYNAMIC_OBJECT_TYPE_OWNERSHIP_SIGN: DynamicObjectType = DynamicObjectType(60);
    pub const DYNAMIC_OBJECT_TYPE_WHEAT_PLANT: DynamicObjectType = DynamicObjectType(61);
    pub const DYNAMIC_OBJECT_TYPE_TOMATO_PLANT: DynamicObjectType = DynamicObjectType(62);
    pub const DYNAMIC_OBJECT_TYPE_NPC_YAK: DynamicObjectType = DynamicObjectType(63);
    pub const DYNAMIC_OBJECT_TYPE_MIRROR: DynamicObjectType = DynamicObjectType(64);
    pub const MAX_DYNAMIC_OBJECT_TYPES: DynamicObjectType = DynamicObjectType(65);
    #[inline]
    pub const fn raw(self) -> u32 {
        self.0
    }
    #[inline]
    pub const fn from_raw(v: u32) -> Self {
        Self(v)
    }
    /// First-declared constant name for this value (None for unknown/unnamed values).
    pub fn name(self) -> Option<&'static str> {
        match self.0 {
            0 => Some("DYNAMIC_OBJECT_TYPE_UNDEFINED"),
            1 => Some("DYNAMIC_OBJECT_TYPE_APPLE_TREE"),
            2 => Some("DYNAMIC_OBJECT_TYPE_MAPLE_TREE"),
            3 => Some("DYNAMIC_OBJECT_TYPE_MANGO_TREE"),
            4 => Some("DYNAMIC_OBJECT_TYPE_PINE_TREE"),
            5 => Some("DYNAMIC_OBJECT_TYPE_CACTUS_TREE"),
            6 => Some("DYNAMIC_OBJECT_TYPE_COCONUT_TREE"),
            7 => Some("DYNAMIC_OBJECT_TYPE_ORANGE_TREE"),
            8 => Some("DYNAMIC_OBJECT_TYPE_CHERRY_TREE"),
            9 => Some("DYNAMIC_OBJECT_TYPE_COFFEE_TREE"),
            10 => Some("DYNAMIC_OBJECT_TYPE_FLAX_PLANT"),
            11 => Some("DYNAMIC_OBJECT_TYPE_SUNFLOWER_PLANT"),
            12 => Some("DYNAMIC_OBJECT_TYPE_CORN_PLANT"),
            13 => Some("DYNAMIC_OBJECT_TYPE_NPC_DODO"),
            14 => Some("DYNAMIC_OBJECT_TYPE_FREE_BLOCK"),
            15 => Some("DYNAMIC_OBJECT_TYPE_INTERACTION_OBJECT_DEPRECATED"),
            16 => Some("DYNAMIC_OBJECT_TYPE_FIRE"),
            17 => Some("DYNAMIC_OBJECT_TYPE_TORCH"),
            18 => Some("DYNAMIC_OBJECT_TYPE_GLOWBLOCK"),
            19 => Some("DYNAMIC_OBJECT_TYPE_LADDER"),
            20 => Some("DYNAMIC_OBJECT_TYPE_DOOR"),
            21 => Some("DYNAMIC_OBJECT_TYPE_ARTIFICIAL_LIGHT"),
            22 => Some("DYNAMIC_OBJECT_TYPE_SURFACEBLOCK"),
            23 => Some("DYNAMIC_OBJECT_TYPE_BED"),
            24 => Some("DYNAMIC_OBJECT_TYPE_BLOCKHEAD"),
            25 => Some("DYNAMIC_OBJECT_TYPE_NPC_DROPBEAR"),
            26 => Some("DYNAMIC_OBJECT_TYPE_GATHERBLOCK"),
            27 => Some("DYNAMIC_OBJECT_TYPE_CARROT_PLANT"),
            28 => Some("DYNAMIC_OBJECT_TYPE_NPC_DONKEY"),
            29 => Some("DYNAMIC_OBJECT_TYPE_SNOW_SURFACEBLOCK"),
            30 => Some("DYNAMIC_OBJECT_TYPE_EGG"),
            31 => Some("DYNAMIC_OBJECT_TYPE_WINDOW"),
            32 => Some("DYNAMIC_OBJECT_TYPE_BOAT"),
            33 => Some("DYNAMIC_OBJECT_TYPE_CHILLI_PLANT"),
            34 => Some("DYNAMIC_OBJECT_TYPE_KELP_PLANT"),
            35 => Some("DYNAMIC_OBJECT_TYPE_NPC_CLOWNFISH"),
            36 => Some("DYNAMIC_OBJECT_TYPE_NPC_SHARK"),
            37 => Some("DYNAMIC_OBJECT_TYPE_LIME_TREE"),
            38 => Some("DYNAMIC_OBJECT_TYPE_WIRE"),
            39 => Some("DYNAMIC_OBJECT_TYPE_NPC_CAVETROLL"),
            40 => Some("DYNAMIC_OBJECT_TYPE_RAIL"),
            41 => Some("DYNAMIC_OBJECT_TYPE_TRAIN_CAR_HANDCAR"),
            42 => Some("DYNAMIC_OBJECT_TYPE_TRAIN_CAR_STEAM_ENGINE"),
            43 => Some("DYNAMIC_OBJECT_TYPE_TRAIN_CAR_FREIGHT"),
            44 => Some("DYNAMIC_OBJECT_TYPE_TRAIN_CAR_PASSENGER"),
            45 => Some("DYNAMIC_OBJECT_TYPE_WORKBENCH"),
            46 => Some("DYNAMIC_OBJECT_TYPE_CHEST"),
            47 => Some("DYNAMIC_OBJECT_TYPE_SIGN"),
            48 => Some("DYNAMIC_OBJECT_TYPE_TRADING_POST"),
            49 => Some("DYNAMIC_OBJECT_TYPE_TRAIN_STATION"),
            50 => Some("DYNAMIC_OBJECT_TYPE_TRADE_PORTAL"),
            51 => Some("DYNAMIC_OBJECT_TYPE_NPC_SCORPION"),
            52 => Some("DYNAMIC_OBJECT_TYPE_PAINTING"),
            53 => Some("DYNAMIC_OBJECT_TYPE_COLUMN"),
            54 => Some("DYNAMIC_OBJECT_TYPE_STAIRS"),
            55 => Some("DYNAMIC_OBJECT_TYPE_ELEVATOR_MOTOR"),
            56 => Some("DYNAMIC_OBJECT_TYPE_ELEVATOR_SHAFT"),
            57 => Some("DYNAMIC_OBJECT_TYPE_GEM_TREE"),
            58 => Some("DYNAMIC_OBJECT_TYPE_VINE_PLANT"),
            59 => Some("DYNAMIC_OBJECT_TYPE_TULIP_PLANT"),
            60 => Some("DYNAMIC_OBJECT_TYPE_OWNERSHIP_SIGN"),
            61 => Some("DYNAMIC_OBJECT_TYPE_WHEAT_PLANT"),
            62 => Some("DYNAMIC_OBJECT_TYPE_TOMATO_PLANT"),
            63 => Some("DYNAMIC_OBJECT_TYPE_NPC_YAK"),
            64 => Some("DYNAMIC_OBJECT_TYPE_MIRROR"),
            65 => Some("MAX_DYNAMIC_OBJECT_TYPES"),
            _ => None,
        }
    }
}
impl core::fmt::Debug for DynamicObjectType {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self.name() {
            Some(n) => write!(f, "{}", n),
            None => write!(f, "DynamicObjectType({})", self.0),
        }
    }
}
impl From<u32> for DynamicObjectType {
    fn from(v: u32) -> Self {
        Self(v)
    }
}
impl From<DynamicObjectType> for u32 {
    fn from(v: DynamicObjectType) -> u32 {
        v.0
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct TileType(pub u32);
impl TileType {
    pub const TILE_ROCK: TileType = TileType(1);
    pub const TILE_AIR: TileType = TileType(2);
    pub const TILE_WATER: TileType = TileType(3);
    pub const TILE_ICE: TileType = TileType(4);
    pub const TILE_SNOW: TileType = TileType(5);
    pub const TILE_DIRT: TileType = TileType(6);
    pub const TILE_SAND: TileType = TileType(7);
    pub const TILE_SAND_BEACH: TileType = TileType(8);
    pub const TILE_WOOD: TileType = TileType(9);
    pub const TILE_COBBLESTONE: TileType = TileType(10);
    pub const TILE_RED_BRICKS: TileType = TileType(11);
    pub const TILE_LIMESTONE: TileType = TileType(12);
    pub const TILE_LIMESTONE_BLOCK: TileType = TileType(13);
    pub const TILE_MARBLE: TileType = TileType(14);
    pub const TILE_MARBLE_BLOCK: TileType = TileType(15);
    pub const TILE_TIME_CRYSTAL: TileType = TileType(16);
    pub const TILE_SANDSTONE: TileType = TileType(17);
    pub const TILE_SANDSTONE_BLOCK: TileType = TileType(18);
    pub const TILE_RED_MARBLE: TileType = TileType(19);
    pub const TILE_RED_MARBLE_BLOCK: TileType = TileType(20);
    pub const TILE_FLAX_MAT: TileType = TileType(21);
    pub const TILE_FLAX_MAT_YELLOW: TileType = TileType(22);
    pub const TILE_FLAX_MAT_RED: TileType = TileType(23);
    pub const TILE_GLASS: TileType = TileType(24);
    pub const TILE_INITIAL_PORTAL_BASE_STONE: TileType = TileType(25);
    pub const TILE_GOLD_BLOCK: TileType = TileType(26);
    pub const TILE_DIRT_GRASS: TileType = TileType(27);
    pub const TILE_DIRT_GRASS_FROZEN: TileType = TileType(28);
    pub const TILE_LAPIS: TileType = TileType(29);
    pub const TILE_LAPIS_BLOCK: TileType = TileType(30);
    pub const TILE_LAVA: TileType = TileType(31);
    pub const TILE_WOODEN_PLATFORM: TileType = TileType(32);
    pub const TILE_INITIAL_PORTAL_BASE_STONE_AMETHYST: TileType = TileType(33);
    pub const TILE_INITIAL_PORTAL_BASE_STONE_SAPPHIRE: TileType = TileType(34);
    pub const TILE_INITIAL_PORTAL_BASE_STONE_EMERALD: TileType = TileType(35);
    pub const TILE_INITIAL_PORTAL_BASE_STONE_RUBY: TileType = TileType(36);
    pub const TILE_INITIAL_PORTAL_BASE_STONE_DIAMOND: TileType = TileType(37);
    pub const TILE_NORTH_POLE: TileType = TileType(38);
    pub const TILE_SOUTH_POLE: TileType = TileType(39);
    pub const TILE_WEST_POLE: TileType = TileType(40);
    pub const TILE_EAST_POLE: TileType = TileType(41);
    pub const TILE_PLACED_PORTAL_BASE_STONE: TileType = TileType(42);
    pub const TILE_PLACED_PORTAL_BASE_STONE_AMETHYST: TileType = TileType(43);
    pub const TILE_PLACED_PORTAL_BASE_STONE_SAPPHIRE: TileType = TileType(44);
    pub const TILE_PLACED_PORTAL_BASE_STONE_EMERALD: TileType = TileType(45);
    pub const TILE_PLACED_PORTAL_BASE_STONE_RUBY: TileType = TileType(46);
    pub const TILE_PLACED_PORTAL_BASE_STONE_DIAMOND: TileType = TileType(47);
    pub const TILE_COMPOST: TileType = TileType(48);
    pub const TILE_COMPOST_GRASS: TileType = TileType(49);
    pub const TILE_COMPOST_GRASS_FROZEN: TileType = TileType(50);
    pub const TILE_BASALT: TileType = TileType(51);
    pub const TILE_BASALT_BLOCK: TileType = TileType(52);
    pub const TILE_COPPER_BLOCK: TileType = TileType(53);
    pub const TILE_TIN_BLOCK: TileType = TileType(54);
    pub const TILE_BRONZE_BLOCK: TileType = TileType(55);
    pub const TILE_IRON_BLOCK: TileType = TileType(56);
    pub const TILE_STEEL_BLOCK: TileType = TileType(57);
    pub const TILE_BLACK_SAND: TileType = TileType(58);
    pub const TILE_BLACK_GLASS: TileType = TileType(59);
    pub const TILE_TRADE_PORTAL_BASE_STONE: TileType = TileType(60);
    pub const TILE_TRADE_PORTAL_BASE_STONE_AMETHYST: TileType = TileType(61);
    pub const TILE_TRADE_PORTAL_BASE_STONE_SAPPHIRE: TileType = TileType(62);
    pub const TILE_TRADE_PORTAL_BASE_STONE_EMERALD: TileType = TileType(63);
    pub const TILE_TRADE_PORTAL_BASE_STONE_RUBY: TileType = TileType(64);
    pub const TILE_TRADE_PORTAL_BASE_STONE_DIAMOND: TileType = TileType(65);
    pub const TILE_FALLEN_LEAVES: TileType = TileType(66);
    pub const TILE_PLATINUM_BLOCK: TileType = TileType(67);
    pub const TILE_TITANIUM_BLOCK: TileType = TileType(68);
    pub const TILE_CARBON_FIBER_BLOCK: TileType = TileType(69);
    pub const TILE_GRAVEL: TileType = TileType(70);
    pub const TILE_AMETHYST_BLOCK: TileType = TileType(71);
    pub const TILE_SAPPHIRE_BLOCK: TileType = TileType(72);
    pub const TILE_EMERALD_BLOCK: TileType = TileType(73);
    pub const TILE_RUBY_BLOCK: TileType = TileType(74);
    pub const TILE_DIAMOND_BLOCK: TileType = TileType(75);
    pub const TILE_PLASTER: TileType = TileType(76);
    pub const TILE_LUMINOUS_PLASTER: TileType = TileType(77);
    pub const MAX_TILE_TYPES: TileType = TileType(78);
    #[inline]
    pub const fn raw(self) -> u32 {
        self.0
    }
    #[inline]
    pub const fn from_raw(v: u32) -> Self {
        Self(v)
    }
    /// First-declared constant name for this value (None for unknown/unnamed values).
    pub fn name(self) -> Option<&'static str> {
        match self.0 {
            1 => Some("TILE_ROCK"),
            2 => Some("TILE_AIR"),
            3 => Some("TILE_WATER"),
            4 => Some("TILE_ICE"),
            5 => Some("TILE_SNOW"),
            6 => Some("TILE_DIRT"),
            7 => Some("TILE_SAND"),
            8 => Some("TILE_SAND_BEACH"),
            9 => Some("TILE_WOOD"),
            10 => Some("TILE_COBBLESTONE"),
            11 => Some("TILE_RED_BRICKS"),
            12 => Some("TILE_LIMESTONE"),
            13 => Some("TILE_LIMESTONE_BLOCK"),
            14 => Some("TILE_MARBLE"),
            15 => Some("TILE_MARBLE_BLOCK"),
            16 => Some("TILE_TIME_CRYSTAL"),
            17 => Some("TILE_SANDSTONE"),
            18 => Some("TILE_SANDSTONE_BLOCK"),
            19 => Some("TILE_RED_MARBLE"),
            20 => Some("TILE_RED_MARBLE_BLOCK"),
            21 => Some("TILE_FLAX_MAT"),
            22 => Some("TILE_FLAX_MAT_YELLOW"),
            23 => Some("TILE_FLAX_MAT_RED"),
            24 => Some("TILE_GLASS"),
            25 => Some("TILE_INITIAL_PORTAL_BASE_STONE"),
            26 => Some("TILE_GOLD_BLOCK"),
            27 => Some("TILE_DIRT_GRASS"),
            28 => Some("TILE_DIRT_GRASS_FROZEN"),
            29 => Some("TILE_LAPIS"),
            30 => Some("TILE_LAPIS_BLOCK"),
            31 => Some("TILE_LAVA"),
            32 => Some("TILE_WOODEN_PLATFORM"),
            33 => Some("TILE_INITIAL_PORTAL_BASE_STONE_AMETHYST"),
            34 => Some("TILE_INITIAL_PORTAL_BASE_STONE_SAPPHIRE"),
            35 => Some("TILE_INITIAL_PORTAL_BASE_STONE_EMERALD"),
            36 => Some("TILE_INITIAL_PORTAL_BASE_STONE_RUBY"),
            37 => Some("TILE_INITIAL_PORTAL_BASE_STONE_DIAMOND"),
            38 => Some("TILE_NORTH_POLE"),
            39 => Some("TILE_SOUTH_POLE"),
            40 => Some("TILE_WEST_POLE"),
            41 => Some("TILE_EAST_POLE"),
            42 => Some("TILE_PLACED_PORTAL_BASE_STONE"),
            43 => Some("TILE_PLACED_PORTAL_BASE_STONE_AMETHYST"),
            44 => Some("TILE_PLACED_PORTAL_BASE_STONE_SAPPHIRE"),
            45 => Some("TILE_PLACED_PORTAL_BASE_STONE_EMERALD"),
            46 => Some("TILE_PLACED_PORTAL_BASE_STONE_RUBY"),
            47 => Some("TILE_PLACED_PORTAL_BASE_STONE_DIAMOND"),
            48 => Some("TILE_COMPOST"),
            49 => Some("TILE_COMPOST_GRASS"),
            50 => Some("TILE_COMPOST_GRASS_FROZEN"),
            51 => Some("TILE_BASALT"),
            52 => Some("TILE_BASALT_BLOCK"),
            53 => Some("TILE_COPPER_BLOCK"),
            54 => Some("TILE_TIN_BLOCK"),
            55 => Some("TILE_BRONZE_BLOCK"),
            56 => Some("TILE_IRON_BLOCK"),
            57 => Some("TILE_STEEL_BLOCK"),
            58 => Some("TILE_BLACK_SAND"),
            59 => Some("TILE_BLACK_GLASS"),
            60 => Some("TILE_TRADE_PORTAL_BASE_STONE"),
            61 => Some("TILE_TRADE_PORTAL_BASE_STONE_AMETHYST"),
            62 => Some("TILE_TRADE_PORTAL_BASE_STONE_SAPPHIRE"),
            63 => Some("TILE_TRADE_PORTAL_BASE_STONE_EMERALD"),
            64 => Some("TILE_TRADE_PORTAL_BASE_STONE_RUBY"),
            65 => Some("TILE_TRADE_PORTAL_BASE_STONE_DIAMOND"),
            66 => Some("TILE_FALLEN_LEAVES"),
            67 => Some("TILE_PLATINUM_BLOCK"),
            68 => Some("TILE_TITANIUM_BLOCK"),
            69 => Some("TILE_CARBON_FIBER_BLOCK"),
            70 => Some("TILE_GRAVEL"),
            71 => Some("TILE_AMETHYST_BLOCK"),
            72 => Some("TILE_SAPPHIRE_BLOCK"),
            73 => Some("TILE_EMERALD_BLOCK"),
            74 => Some("TILE_RUBY_BLOCK"),
            75 => Some("TILE_DIAMOND_BLOCK"),
            76 => Some("TILE_PLASTER"),
            77 => Some("TILE_LUMINOUS_PLASTER"),
            78 => Some("MAX_TILE_TYPES"),
            _ => None,
        }
    }
}
impl core::fmt::Debug for TileType {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self.name() {
            Some(n) => write!(f, "{}", n),
            None => write!(f, "TileType({})", self.0),
        }
    }
}
impl From<u32> for TileType {
    fn from(v: u32) -> Self {
        Self(v)
    }
}
impl From<TileType> for u32 {
    fn from(v: TileType) -> u32 {
        v.0
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct TileContents(pub u32);
impl TileContents {
    pub const CONTENTS_NOTHING: TileContents = TileContents(0);
    pub const CONTENTS_FLINT: TileContents = TileContents(1);
    pub const CONTENTS_CLAY: TileContents = TileContents(2);
    pub const CONTENTS_APPLE_TREE_BUSH: TileContents = TileContents(3);
    pub const CONTENTS_APPLE_TREE_TRUNK: TileContents = TileContents(4);
    pub const CONTENTS_APPLE_TREE_TRUNK_BUSH: TileContents = TileContents(5);
    pub const CONTENTS_PINE_TREE_BUSH: TileContents = TileContents(6);
    pub const CONTENTS_PINE_TREE_TRUNK: TileContents = TileContents(7);
    pub const CONTENTS_PINE_TREE_TRUNK_BUSH: TileContents = TileContents(8);
    pub const CONTENTS_MAPLE_TREE_BUSH: TileContents = TileContents(9);
    pub const CONTENTS_MAPLE_TREE_TRUNK: TileContents = TileContents(10);
    pub const CONTENTS_MAPLE_TREE_TRUNK_BUSH: TileContents = TileContents(11);
    pub const CONTENTS_MANGO_TREE_BUSH: TileContents = TileContents(12);
    pub const CONTENTS_MANGO_TREE_TRUNK: TileContents = TileContents(13);
    pub const CONTENTS_MANGO_TREE_TRUNK_BUSH: TileContents = TileContents(14);
    pub const CONTENTS_COCONUT_TREE_BUSH: TileContents = TileContents(15);
    pub const CONTENTS_COCONUT_TREE_TRUNK: TileContents = TileContents(16);
    pub const CONTENTS_COCONUT_TREE_TRUNK_BUSH: TileContents = TileContents(17);
    pub const CONTENTS_ORANGE_TREE_BUSH: TileContents = TileContents(18);
    pub const CONTENTS_ORANGE_TREE_TRUNK: TileContents = TileContents(19);
    pub const CONTENTS_ORANGE_TREE_TRUNK_BUSH: TileContents = TileContents(20);
    pub const CONTENTS_CHERRY_TREE_BUSH: TileContents = TileContents(21);
    pub const CONTENTS_CHERRY_TREE_TRUNK: TileContents = TileContents(22);
    pub const CONTENTS_CHERRY_TREE_TRUNK_BUSH: TileContents = TileContents(23);
    pub const CONTENTS_COFFEE_TREE_BUSH: TileContents = TileContents(24);
    pub const CONTENTS_COFFEE_TREE_TRUNK: TileContents = TileContents(25);
    pub const CONTENTS_COFFEE_TREE_TRUNK_BUSH: TileContents = TileContents(26);
    pub const CONTENTS_APPLE_TREE_BUSH_DEAD: TileContents = TileContents(27);
    pub const CONTENTS_APPLE_TREE_TRUNK_DEAD: TileContents = TileContents(28);
    pub const CONTENTS_PINE_TREE_TRUNK_DEAD: TileContents = TileContents(29);
    pub const CONTENTS_MAPLE_TREE_BUSH_DEAD: TileContents = TileContents(30);
    pub const CONTENTS_MAPLE_TREE_TRUNK_DEAD: TileContents = TileContents(31);
    pub const CONTENTS_MANGO_TREE_BUSH_DEAD: TileContents = TileContents(32);
    pub const CONTENTS_MANGO_TREE_TRUNK_DEAD: TileContents = TileContents(33);
    pub const CONTENTS_PINE_TREE_BUSH_DEAD: TileContents = TileContents(34);
    pub const CONTENTS_COCONUT_TREE_BUSH_DEAD: TileContents = TileContents(35);
    pub const CONTENTS_COCONUT_TREE_TRUNK_DEAD: TileContents = TileContents(36);
    pub const CONTENTS_ORANGE_TREE_BUSH_DEAD: TileContents = TileContents(37);
    pub const CONTENTS_ORANGE_TREE_TRUNK_DEAD: TileContents = TileContents(38);
    pub const CONTENTS_CHERRY_TREE_BUSH_DEAD: TileContents = TileContents(39);
    pub const CONTENTS_CHERRY_TREE_TRUNK_DEAD: TileContents = TileContents(40);
    pub const CONTENTS_COFFEE_TREE_BUSH_DEAD: TileContents = TileContents(41);
    pub const CONTENTS_COFFEE_TREE_TRUNK_DEAD: TileContents = TileContents(42);
    pub const CONTENTS_CACTUS: TileContents = TileContents(43);
    pub const CONTENTS_CACTUS_DEAD: TileContents = TileContents(44);
    pub const CONTENTS_CHARCOAL: TileContents = TileContents(45);
    pub const CONTENTS_WORKBENCH: TileContents = TileContents(46);
    pub const CONTENTS_WORKBENCH_SPRITE: TileContents = TileContents(47);
    pub const CONTENTS_INTERACTION_OBJECT: TileContents = TileContents(48);
    pub const CONTENTS_BASIC_TORCH_SPRITE: TileContents = TileContents(49);
    pub const CONTENTS_CLAY_LANTERN_SPRITE: TileContents = TileContents(50);
    pub const CONTENTS_RUBY: TileContents = TileContents(51);
    pub const CONTENTS_RUBY_PROMISE: TileContents = TileContents(52);
    pub const CONTENTS_EMERALD: TileContents = TileContents(53);
    pub const CONTENTS_EMERALD_PROMISE: TileContents = TileContents(54);
    pub const CONTENTS_SAPPHIRE: TileContents = TileContents(55);
    pub const CONTENTS_SAPPHIRE_PROMISE: TileContents = TileContents(56);
    pub const CONTENTS_AMETHYST: TileContents = TileContents(57);
    pub const CONTENTS_AMETHYST_PROMISE: TileContents = TileContents(58);
    pub const CONTENTS_DIAMOND: TileContents = TileContents(59);
    pub const CONTENTS_DIAMOND_PROMISE: TileContents = TileContents(60);
    pub const CONTENTS_COPPER: TileContents = TileContents(61);
    pub const CONTENTS_TIN: TileContents = TileContents(62);
    pub const CONTENTS_IRON: TileContents = TileContents(63);
    pub const CONTENTS_OIL: TileContents = TileContents(64);
    pub const CONTENTS_COAL: TileContents = TileContents(65);
    pub const CONTENTS_LADDER: TileContents = TileContents(66);
    pub const CONTENTS_FLAX_PLANT: TileContents = TileContents(67);
    pub const CONTENTS_FLAX_FLOWER: TileContents = TileContents(68);
    pub const CONTENTS_WINDOW: TileContents = TileContents(69);
    pub const CONTENTS_DOOR: TileContents = TileContents(70);
    pub const CONTENTS_SUNFLOWER_PLANT: TileContents = TileContents(71);
    pub const CONTENTS_SUNFLOWER_FLOWER: TileContents = TileContents(72);
    pub const CONTENTS_CORN_PLANT: TileContents = TileContents(73);
    pub const CONTENTS_CORN_FLOWER: TileContents = TileContents(74);
    pub const CONTENTS_TRAPDOOR: TileContents = TileContents(75);
    pub const CONTENTS_CARROT_PLANT: TileContents = TileContents(76);
    pub const CONTENTS_GOLD: TileContents = TileContents(77);
    pub const CONTENTS_DODO_EGG: TileContents = TileContents(78);
    pub const CONTENTS_CHILLI_PLANT: TileContents = TileContents(79);
    pub const CONTENTS_CHILLI_FLOWER: TileContents = TileContents(80);
    pub const CONTENTS_KELP_PLANT: TileContents = TileContents(81);
    pub const CONTENTS_AMETHYST_CHANDELIER_SPRITE: TileContents = TileContents(82);
    pub const CONTENTS_SAPPHIRE_CHANDELIER_SPRITE: TileContents = TileContents(83);
    pub const CONTENTS_EMERALD_CHANDELIER_SPRITE: TileContents = TileContents(84);
    pub const CONTENTS_RUBY_CHANDELIER_SPRITE: TileContents = TileContents(85);
    pub const CONTENTS_DIAMOND_CHANDELIER_SPRITE: TileContents = TileContents(86);
    pub const CONTENTS_STEEL_LANTERN_SPRITE: TileContents = TileContents(87);
    pub const CONTENTS_SHARK_JAW: TileContents = TileContents(88);
    pub const CONTENTS_LIME_TREE_BUSH: TileContents = TileContents(89);
    pub const CONTENTS_LIME_TREE_TRUNK: TileContents = TileContents(90);
    pub const CONTENTS_LIME_TREE_TRUNK_BUSH: TileContents = TileContents(91);
    pub const CONTENTS_LIME_TREE_BUSH_DEAD: TileContents = TileContents(92);
    pub const CONTENTS_LIME_TREE_TRUNK_DEAD: TileContents = TileContents(93);
    pub const CONTENTS_TREASURE_CHEST_PROMISE: TileContents = TileContents(94);
    pub const CONTENTS_BLACK_WINDOW: TileContents = TileContents(95);
    pub const CONTENTS_COPPER_WIRE: TileContents = TileContents(96);
    pub const CONTENTS_ICE_TORCH_SPRITE: TileContents = TileContents(97);
    pub const CONTENTS_RAIL: TileContents = TileContents(98);
    pub const CONTENTS_PAINTING: TileContents = TileContents(99);
    pub const CONTENTS_COLUMN: TileContents = TileContents(100);
    pub const CONTENTS_STAIRS: TileContents = TileContents(101);
    pub const CONTENTS_STEEL_DOWNLIGHT: TileContents = TileContents(102);
    pub const CONTENTS_ELEVATOR_SHAFT: TileContents = TileContents(103);
    pub const CONTENTS_ELEVATOR_MOTOR: TileContents = TileContents(104);
    pub const CONTENTS_STEEL_UPLIGHT: TileContents = TileContents(105);
    pub const CONTENTS_PLATINUM: TileContents = TileContents(106);
    pub const CONTENTS_TITANIUM: TileContents = TileContents(107);
    pub const CONTENTS_AMETHYST_TREE_PROMISE: TileContents = TileContents(108);
    pub const CONTENTS_AMETHYST_TREE_TRUNK: TileContents = TileContents(109);
    pub const CONTENTS_AMETHYST_TREE_BUSH: TileContents = TileContents(110);
    pub const CONTENTS_AMETHYST_TREE_TRUNK_BUSH: TileContents = TileContents(111);
    pub const CONTENTS_SAPPHIRE_TREE_TRUNK: TileContents = TileContents(112);
    pub const CONTENTS_SAPPHIRE_TREE_BUSH: TileContents = TileContents(113);
    pub const CONTENTS_SAPPHIRE_TREE_TRUNK_BUSH: TileContents = TileContents(114);
    pub const CONTENTS_EMERALD_TREE_TRUNK: TileContents = TileContents(115);
    pub const CONTENTS_EMERALD_TREE_BUSH: TileContents = TileContents(116);
    pub const CONTENTS_EMERALD_TREE_TRUNK_BUSH: TileContents = TileContents(117);
    pub const CONTENTS_RUBY_TREE_TRUNK: TileContents = TileContents(118);
    pub const CONTENTS_RUBY_TREE_BUSH: TileContents = TileContents(119);
    pub const CONTENTS_RUBY_TREE_TRUNK_BUSH: TileContents = TileContents(120);
    pub const CONTENTS_DIAMOND_TREE_TRUNK: TileContents = TileContents(121);
    pub const CONTENTS_DIAMOND_TREE_BUSH: TileContents = TileContents(122);
    pub const CONTENTS_DIAMOND_TREE_TRUNK_BUSH: TileContents = TileContents(123);
    pub const CONTENTS_VINE: TileContents = TileContents(124);
    pub const CONTENTS_TULIP_PLANT: TileContents = TileContents(125);
    pub const CONTENTS_TULIP_PLANT_PROMISE_PLAIN: TileContents = TileContents(126);
    pub const CONTENTS_VINE_PROMISE: TileContents = TileContents(127);
    pub const CONTENTS_SAPPHIRE_TREE_PROMISE: TileContents = TileContents(128);
    pub const CONTENTS_EMERALD_TREE_PROMISE: TileContents = TileContents(129);
    pub const CONTENTS_RUBY_TREE_PROMISE: TileContents = TileContents(130);
    pub const CONTENTS_DIAMOND_TREE_PROMISE: TileContents = TileContents(131);
    pub const CONTENTS_APPLE_TREE_PROMISE: TileContents = TileContents(132);
    pub const CONTENTS_ORANGE_TREE_PROMISE: TileContents = TileContents(133);
    pub const CONTENTS_CHERRY_TREE_PROMISE: TileContents = TileContents(134);
    pub const CONTENTS_PINE_TREE_PROMISE: TileContents = TileContents(135);
    pub const CONTENTS_CACTUS_TREE_PROMISE: TileContents = TileContents(136);
    pub const CONTENTS_LIME_TREE_PROMISE: TileContents = TileContents(137);
    pub const CONTENTS_COFFEE_TREE_PROMISE: TileContents = TileContents(138);
    pub const CONTENTS_COCONUT_TREE_PROMISE: TileContents = TileContents(139);
    pub const CONTENTS_MAPLE_TREE_PROMISE: TileContents = TileContents(140);
    pub const CONTENTS_MANGO_TREE_PROMISE: TileContents = TileContents(141);
    pub const CONTENTS_TULIP_PLANT_PROMISE_RARE: TileContents = TileContents(142);
    pub const CONTENTS_TULIP_PLANT_PROMISE_RAREST: TileContents = TileContents(143);
    pub const CONTENTS_CAVE_TROLL_PROMISE_NO_TREASURE: TileContents = TileContents(144);
    pub const CONTENTS_TREASURE_CHEST_PROMISE_NO_CAVE_TROLL: TileContents = TileContents(145);
    pub const CONTENTS_GATE: TileContents = TileContents(146);
    pub const CONTENTS_WHEAT_PLANT: TileContents = TileContents(147);
    pub const CONTENTS_TOMATO_PLANT: TileContents = TileContents(148);
    #[inline]
    pub const fn raw(self) -> u32 {
        self.0
    }
    #[inline]
    pub const fn from_raw(v: u32) -> Self {
        Self(v)
    }
    /// First-declared constant name for this value (None for unknown/unnamed values).
    pub fn name(self) -> Option<&'static str> {
        match self.0 {
            0 => Some("CONTENTS_NOTHING"),
            1 => Some("CONTENTS_FLINT"),
            2 => Some("CONTENTS_CLAY"),
            3 => Some("CONTENTS_APPLE_TREE_BUSH"),
            4 => Some("CONTENTS_APPLE_TREE_TRUNK"),
            5 => Some("CONTENTS_APPLE_TREE_TRUNK_BUSH"),
            6 => Some("CONTENTS_PINE_TREE_BUSH"),
            7 => Some("CONTENTS_PINE_TREE_TRUNK"),
            8 => Some("CONTENTS_PINE_TREE_TRUNK_BUSH"),
            9 => Some("CONTENTS_MAPLE_TREE_BUSH"),
            10 => Some("CONTENTS_MAPLE_TREE_TRUNK"),
            11 => Some("CONTENTS_MAPLE_TREE_TRUNK_BUSH"),
            12 => Some("CONTENTS_MANGO_TREE_BUSH"),
            13 => Some("CONTENTS_MANGO_TREE_TRUNK"),
            14 => Some("CONTENTS_MANGO_TREE_TRUNK_BUSH"),
            15 => Some("CONTENTS_COCONUT_TREE_BUSH"),
            16 => Some("CONTENTS_COCONUT_TREE_TRUNK"),
            17 => Some("CONTENTS_COCONUT_TREE_TRUNK_BUSH"),
            18 => Some("CONTENTS_ORANGE_TREE_BUSH"),
            19 => Some("CONTENTS_ORANGE_TREE_TRUNK"),
            20 => Some("CONTENTS_ORANGE_TREE_TRUNK_BUSH"),
            21 => Some("CONTENTS_CHERRY_TREE_BUSH"),
            22 => Some("CONTENTS_CHERRY_TREE_TRUNK"),
            23 => Some("CONTENTS_CHERRY_TREE_TRUNK_BUSH"),
            24 => Some("CONTENTS_COFFEE_TREE_BUSH"),
            25 => Some("CONTENTS_COFFEE_TREE_TRUNK"),
            26 => Some("CONTENTS_COFFEE_TREE_TRUNK_BUSH"),
            27 => Some("CONTENTS_APPLE_TREE_BUSH_DEAD"),
            28 => Some("CONTENTS_APPLE_TREE_TRUNK_DEAD"),
            29 => Some("CONTENTS_PINE_TREE_TRUNK_DEAD"),
            30 => Some("CONTENTS_MAPLE_TREE_BUSH_DEAD"),
            31 => Some("CONTENTS_MAPLE_TREE_TRUNK_DEAD"),
            32 => Some("CONTENTS_MANGO_TREE_BUSH_DEAD"),
            33 => Some("CONTENTS_MANGO_TREE_TRUNK_DEAD"),
            34 => Some("CONTENTS_PINE_TREE_BUSH_DEAD"),
            35 => Some("CONTENTS_COCONUT_TREE_BUSH_DEAD"),
            36 => Some("CONTENTS_COCONUT_TREE_TRUNK_DEAD"),
            37 => Some("CONTENTS_ORANGE_TREE_BUSH_DEAD"),
            38 => Some("CONTENTS_ORANGE_TREE_TRUNK_DEAD"),
            39 => Some("CONTENTS_CHERRY_TREE_BUSH_DEAD"),
            40 => Some("CONTENTS_CHERRY_TREE_TRUNK_DEAD"),
            41 => Some("CONTENTS_COFFEE_TREE_BUSH_DEAD"),
            42 => Some("CONTENTS_COFFEE_TREE_TRUNK_DEAD"),
            43 => Some("CONTENTS_CACTUS"),
            44 => Some("CONTENTS_CACTUS_DEAD"),
            45 => Some("CONTENTS_CHARCOAL"),
            46 => Some("CONTENTS_WORKBENCH"),
            47 => Some("CONTENTS_WORKBENCH_SPRITE"),
            48 => Some("CONTENTS_INTERACTION_OBJECT"),
            49 => Some("CONTENTS_BASIC_TORCH_SPRITE"),
            50 => Some("CONTENTS_CLAY_LANTERN_SPRITE"),
            51 => Some("CONTENTS_RUBY"),
            52 => Some("CONTENTS_RUBY_PROMISE"),
            53 => Some("CONTENTS_EMERALD"),
            54 => Some("CONTENTS_EMERALD_PROMISE"),
            55 => Some("CONTENTS_SAPPHIRE"),
            56 => Some("CONTENTS_SAPPHIRE_PROMISE"),
            57 => Some("CONTENTS_AMETHYST"),
            58 => Some("CONTENTS_AMETHYST_PROMISE"),
            59 => Some("CONTENTS_DIAMOND"),
            60 => Some("CONTENTS_DIAMOND_PROMISE"),
            61 => Some("CONTENTS_COPPER"),
            62 => Some("CONTENTS_TIN"),
            63 => Some("CONTENTS_IRON"),
            64 => Some("CONTENTS_OIL"),
            65 => Some("CONTENTS_COAL"),
            66 => Some("CONTENTS_LADDER"),
            67 => Some("CONTENTS_FLAX_PLANT"),
            68 => Some("CONTENTS_FLAX_FLOWER"),
            69 => Some("CONTENTS_WINDOW"),
            70 => Some("CONTENTS_DOOR"),
            71 => Some("CONTENTS_SUNFLOWER_PLANT"),
            72 => Some("CONTENTS_SUNFLOWER_FLOWER"),
            73 => Some("CONTENTS_CORN_PLANT"),
            74 => Some("CONTENTS_CORN_FLOWER"),
            75 => Some("CONTENTS_TRAPDOOR"),
            76 => Some("CONTENTS_CARROT_PLANT"),
            77 => Some("CONTENTS_GOLD"),
            78 => Some("CONTENTS_DODO_EGG"),
            79 => Some("CONTENTS_CHILLI_PLANT"),
            80 => Some("CONTENTS_CHILLI_FLOWER"),
            81 => Some("CONTENTS_KELP_PLANT"),
            82 => Some("CONTENTS_AMETHYST_CHANDELIER_SPRITE"),
            83 => Some("CONTENTS_SAPPHIRE_CHANDELIER_SPRITE"),
            84 => Some("CONTENTS_EMERALD_CHANDELIER_SPRITE"),
            85 => Some("CONTENTS_RUBY_CHANDELIER_SPRITE"),
            86 => Some("CONTENTS_DIAMOND_CHANDELIER_SPRITE"),
            87 => Some("CONTENTS_STEEL_LANTERN_SPRITE"),
            88 => Some("CONTENTS_SHARK_JAW"),
            89 => Some("CONTENTS_LIME_TREE_BUSH"),
            90 => Some("CONTENTS_LIME_TREE_TRUNK"),
            91 => Some("CONTENTS_LIME_TREE_TRUNK_BUSH"),
            92 => Some("CONTENTS_LIME_TREE_BUSH_DEAD"),
            93 => Some("CONTENTS_LIME_TREE_TRUNK_DEAD"),
            94 => Some("CONTENTS_TREASURE_CHEST_PROMISE"),
            95 => Some("CONTENTS_BLACK_WINDOW"),
            96 => Some("CONTENTS_COPPER_WIRE"),
            97 => Some("CONTENTS_ICE_TORCH_SPRITE"),
            98 => Some("CONTENTS_RAIL"),
            99 => Some("CONTENTS_PAINTING"),
            100 => Some("CONTENTS_COLUMN"),
            101 => Some("CONTENTS_STAIRS"),
            102 => Some("CONTENTS_STEEL_DOWNLIGHT"),
            103 => Some("CONTENTS_ELEVATOR_SHAFT"),
            104 => Some("CONTENTS_ELEVATOR_MOTOR"),
            105 => Some("CONTENTS_STEEL_UPLIGHT"),
            106 => Some("CONTENTS_PLATINUM"),
            107 => Some("CONTENTS_TITANIUM"),
            108 => Some("CONTENTS_AMETHYST_TREE_PROMISE"),
            109 => Some("CONTENTS_AMETHYST_TREE_TRUNK"),
            110 => Some("CONTENTS_AMETHYST_TREE_BUSH"),
            111 => Some("CONTENTS_AMETHYST_TREE_TRUNK_BUSH"),
            112 => Some("CONTENTS_SAPPHIRE_TREE_TRUNK"),
            113 => Some("CONTENTS_SAPPHIRE_TREE_BUSH"),
            114 => Some("CONTENTS_SAPPHIRE_TREE_TRUNK_BUSH"),
            115 => Some("CONTENTS_EMERALD_TREE_TRUNK"),
            116 => Some("CONTENTS_EMERALD_TREE_BUSH"),
            117 => Some("CONTENTS_EMERALD_TREE_TRUNK_BUSH"),
            118 => Some("CONTENTS_RUBY_TREE_TRUNK"),
            119 => Some("CONTENTS_RUBY_TREE_BUSH"),
            120 => Some("CONTENTS_RUBY_TREE_TRUNK_BUSH"),
            121 => Some("CONTENTS_DIAMOND_TREE_TRUNK"),
            122 => Some("CONTENTS_DIAMOND_TREE_BUSH"),
            123 => Some("CONTENTS_DIAMOND_TREE_TRUNK_BUSH"),
            124 => Some("CONTENTS_VINE"),
            125 => Some("CONTENTS_TULIP_PLANT"),
            126 => Some("CONTENTS_TULIP_PLANT_PROMISE_PLAIN"),
            127 => Some("CONTENTS_VINE_PROMISE"),
            128 => Some("CONTENTS_SAPPHIRE_TREE_PROMISE"),
            129 => Some("CONTENTS_EMERALD_TREE_PROMISE"),
            130 => Some("CONTENTS_RUBY_TREE_PROMISE"),
            131 => Some("CONTENTS_DIAMOND_TREE_PROMISE"),
            132 => Some("CONTENTS_APPLE_TREE_PROMISE"),
            133 => Some("CONTENTS_ORANGE_TREE_PROMISE"),
            134 => Some("CONTENTS_CHERRY_TREE_PROMISE"),
            135 => Some("CONTENTS_PINE_TREE_PROMISE"),
            136 => Some("CONTENTS_CACTUS_TREE_PROMISE"),
            137 => Some("CONTENTS_LIME_TREE_PROMISE"),
            138 => Some("CONTENTS_COFFEE_TREE_PROMISE"),
            139 => Some("CONTENTS_COCONUT_TREE_PROMISE"),
            140 => Some("CONTENTS_MAPLE_TREE_PROMISE"),
            141 => Some("CONTENTS_MANGO_TREE_PROMISE"),
            142 => Some("CONTENTS_TULIP_PLANT_PROMISE_RARE"),
            143 => Some("CONTENTS_TULIP_PLANT_PROMISE_RAREST"),
            144 => Some("CONTENTS_CAVE_TROLL_PROMISE_NO_TREASURE"),
            145 => Some("CONTENTS_TREASURE_CHEST_PROMISE_NO_CAVE_TROLL"),
            146 => Some("CONTENTS_GATE"),
            147 => Some("CONTENTS_WHEAT_PLANT"),
            148 => Some("CONTENTS_TOMATO_PLANT"),
            _ => None,
        }
    }
}
impl core::fmt::Debug for TileContents {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self.name() {
            Some(n) => write!(f, "{}", n),
            None => write!(f, "TileContents({})", self.0),
        }
    }
}
impl From<u32> for TileContents {
    fn from(v: u32) -> Self {
        Self(v)
    }
}
impl From<TileContents> for u32 {
    fn from(v: TileContents) -> u32 {
        v.0
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct ImageType(pub u32);
impl ImageType {
    pub const IMAGE_AIR: ImageType = ImageType(0);
    pub const IMAGE_ROCK_COPPER: ImageType = ImageType(1);
    pub const IMAGE_ROCK_IRON: ImageType = ImageType(2);
    pub const IMAGE_ROCK_TIN: ImageType = ImageType(3);
    pub const IMAGE_FIRE_SMOKE_0: ImageType = ImageType(4);
    pub const IMAGE_FIRE_SMOKE_1: ImageType = ImageType(5);
    pub const IMAGE_FIRE_SMOKE_2: ImageType = ImageType(6);
    pub const IMAGE_FIRE_SMOKE_3: ImageType = ImageType(7);
    pub const IMAGE_FIRE_SMOKE_4: ImageType = ImageType(8);
    pub const IMAGE_FIRE_SMOKE_5: ImageType = ImageType(9);
    pub const IMAGE_FIRE_SMOKE_6: ImageType = ImageType(10);
    pub const IMAGE_FIRE_SMOKE_7: ImageType = ImageType(11);
    pub const IMAGE_DOOR_TOP: ImageType = ImageType(12);
    pub const IMAGE_TRAPDOOR_TOP: ImageType = ImageType(13);
    pub const IMAGE_WATER_0: ImageType = ImageType(14);
    pub const IMAGE_WATER_1: ImageType = ImageType(15);
    pub const IMAGE_WATER_2: ImageType = ImageType(16);
    pub const IMAGE_WATER_3: ImageType = ImageType(17);
    pub const IMAGE_WATER_4: ImageType = ImageType(18);
    pub const IMAGE_WATER_5: ImageType = ImageType(19);
    pub const IMAGE_WATER_6: ImageType = ImageType(20);
    pub const IMAGE_WATER_7: ImageType = ImageType(21);
    pub const IMAGE_ROCK: ImageType = ImageType(32);
    pub const IMAGE_COBBLESTONE: ImageType = ImageType(33);
    pub const IMAGE_RED_BRICKS: ImageType = ImageType(34);
    pub const IMAGE_LIMESTONE_BLOCK: ImageType = ImageType(35);
    pub const IMAGE_FIRE_FLAME_0: ImageType = ImageType(36);
    pub const IMAGE_FIRE_FLAME_1: ImageType = ImageType(37);
    pub const IMAGE_FIRE_FLAME_2: ImageType = ImageType(38);
    pub const IMAGE_FIRE_FLAME_3: ImageType = ImageType(39);
    pub const IMAGE_FIRE_FLAME_4: ImageType = ImageType(40);
    pub const IMAGE_FIRE_FLAME_5: ImageType = ImageType(41);
    pub const IMAGE_FIRE_FLAME_6: ImageType = ImageType(42);
    pub const IMAGE_FIRE_FLAME_7: ImageType = ImageType(43);
    pub const IMAGE_DOOR: ImageType = ImageType(44);
    pub const IMAGE_TRAPDOOR: ImageType = ImageType(45);
    pub const IMAGE_LAVA_0: ImageType = ImageType(46);
    pub const IMAGE_PORTAL_NET: ImageType = ImageType(62);
    pub const IMAGE_DIRT: ImageType = ImageType(64);
    pub const IMAGE_SAND: ImageType = ImageType(65);
    pub const IMAGE_LIMESTONE: ImageType = ImageType(66);
    pub const IMAGE_MARBLE: ImageType = ImageType(67);
    pub const IMAGE_MARBLE_BLOCK: ImageType = ImageType(76);
    pub const IMAGE_TIME_CRYSTAL_BLOCK: ImageType = ImageType(77);
    pub const IMAGE_OIL_IN_BLOCK: ImageType = ImageType(78);
    pub const IMAGE_SANDSTONE: ImageType = ImageType(79);
    pub const IMAGE_SANDSTONE_BLOCK: ImageType = ImageType(80);
    pub const IMAGE_RED_MARBLE: ImageType = ImageType(81);
    pub const IMAGE_RED_MARBLE_BLOCK: ImageType = ImageType(82);
    pub const IMAGE_ROCK_COAL: ImageType = ImageType(83);
    pub const IMAGE_ROCK_GOLD: ImageType = ImageType(84);
    pub const IMAGE_GOLD_BLOCK: ImageType = ImageType(85);
    pub const IMAGE_LAPIS: ImageType = ImageType(86);
    pub const IMAGE_LAPIS_BLOCK: ImageType = ImageType(87);
    pub const IMAGE_BASALT: ImageType = ImageType(88);
    pub const IMAGE_BLACK_SAND: ImageType = ImageType(89);
    pub const IMAGE_WATER: ImageType = ImageType(96);
    pub const IMAGE_COMPOST: ImageType = ImageType(97);
    pub const IMAGE_COMPOST_GRASS: ImageType = ImageType(98);
    pub const IMAGE_PORTAL_0: ImageType = ImageType(100);
    pub const IMAGE_PORTAL_1: ImageType = ImageType(101);
    pub const IMAGE_PORTAL_2: ImageType = ImageType(102);
    pub const IMAGE_PORTAL_3: ImageType = ImageType(103);
    pub const IMAGE_PORTAL_4: ImageType = ImageType(104);
    pub const IMAGE_PORTAL_5: ImageType = ImageType(105);
    pub const IMAGE_PORTAL_6: ImageType = ImageType(106);
    pub const IMAGE_PORTAL_7: ImageType = ImageType(107);
    pub const IMAGE_GLASS: ImageType = ImageType(108);
    pub const IMAGE_WINDOW: ImageType = ImageType(109);
    pub const IMAGE_ICE: ImageType = ImageType(110);
    pub const IMAGE_UNUSED_ICEPLUSONE: ImageType = ImageType(111);
    pub const IMAGE_COPPER_BLOCK: ImageType = ImageType(112);
    pub const IMAGE_TIN_BLOCK: ImageType = ImageType(113);
    pub const IMAGE_BRONZE_BLOCK: ImageType = ImageType(114);
    pub const IMAGE_IRON_BLOCK: ImageType = ImageType(115);
    pub const IMAGE_STEEL_BLOCK: ImageType = ImageType(116);
    pub const IMAGE_BLACK_GLASS: ImageType = ImageType(117);
    pub const IMAGE_BLACK_WINDOW: ImageType = ImageType(118);
    pub const IMAGE_DISPLAY_CABINET_FRONT: ImageType = ImageType(119);
    pub const IMAGE_IRON_DOOR_TOP: ImageType = ImageType(120);
    pub const IMAGE_SAPPHIRE: ImageType = ImageType(121);
    pub const IMAGE_AMETHYST: ImageType = ImageType(122);
    pub const IMAGE_DIAMOND: ImageType = ImageType(123);
    pub const IMAGE_RUBY: ImageType = ImageType(124);
    pub const IMAGE_EMERALD: ImageType = ImageType(125);
    pub const IMAGE_CLOUD: ImageType = ImageType(128);
    pub const IMAGE_SNOW: ImageType = ImageType(129);
    pub const IMAGE_COMPOST_GRASS_FROZEN: ImageType = ImageType(130);
    pub const IMAGE_PORTAL_ADD: ImageType = ImageType(131);
    pub const IMAGE_INITIAL_PORTAL_BASE_STONE: ImageType = ImageType(132);
    pub const IMAGE_ROCK_PAINTED: ImageType = ImageType(133);
    pub const IMAGE_WOOD_PAINTED: ImageType = ImageType(134);
    pub const IMAGE_LIMESTONE_BLOCK_PAINTED: ImageType = ImageType(135);
    pub const IMAGE_BRICK_PAINTED: ImageType = ImageType(136);
    pub const IMAGE_COBBLESTONE_PAINTED: ImageType = ImageType(137);
    pub const IMAGE_MARBLE_PAINTED: ImageType = ImageType(138);
    pub const IMAGE_SANDSTONE_PAINTED: ImageType = ImageType(139);
    pub const IMAGE_DIRT_PAINTED: ImageType = ImageType(140);
    pub const IMAGE_WOODEN_PLATFORM_PAINTED: ImageType = ImageType(141);
    pub const IMAGE_SAND_PAINTED: ImageType = ImageType(142);
    pub const IMAGE_BASALT_PAINTED: ImageType = ImageType(143);
    pub const IMAGE_COPPER_PAINTED: ImageType = ImageType(144);
    pub const IMAGE_TIN_PAINTED: ImageType = ImageType(145);
    pub const IMAGE_BRONZE_PAINTED: ImageType = ImageType(146);
    pub const IMAGE_IRON_PAINTED: ImageType = ImageType(147);
    pub const IMAGE_SIGN_DECORATION_LEFT: ImageType = ImageType(148);
    pub const IMAGE_SIGN_DECORATION_RIGHT: ImageType = ImageType(149);
    pub const IMAGE_BLACK_SAND_PAINTED: ImageType = ImageType(150);
    pub const IMAGE_UNUSED_PAINT_F: ImageType = ImageType(151);
    pub const IMAGE_IRON_DOOR: ImageType = ImageType(152);
    pub const IMAGE_IRON_TRAPDOOR: ImageType = ImageType(153);
    pub const IMAGE_DODO_EGG: ImageType = ImageType(154);
    pub const IMAGE_DISPLAY_CABINET_FRONT_PAINTED: ImageType = ImageType(155);
    pub const IMAGE_ELEVATOR_SHAFT_PAINTED: ImageType = ImageType(156);
    pub const IMAGE_PLATINUM_ORE: ImageType = ImageType(157);
    pub const IMAGE_PLATINUM_BLOCK_OLD: ImageType = ImageType(158);
    pub const IMAGE_GRASS_SIDE: ImageType = ImageType(160);
    pub const IMAGE_GRASS_TOP: ImageType = ImageType(161);
    pub const IMAGE_GRASS_SIDE_FROZEN: ImageType = ImageType(162);
    pub const IMAGE_GRASS_TOP_FROZEN: ImageType = ImageType(163);
    pub const IMAGE_WOODEN_PLATFORM: ImageType = ImageType(164);
    pub const IMAGE_INITIAL_PORTAL_BASE_STONE_AMETHYST: ImageType = ImageType(165);
    pub const IMAGE_INITIAL_PORTAL_BASE_STONE_SAPPHIRE: ImageType = ImageType(166);
    pub const IMAGE_INITIAL_PORTAL_BASE_STONE_EMERALD: ImageType = ImageType(167);
    pub const IMAGE_INITIAL_PORTAL_BASE_STONE_RUBY: ImageType = ImageType(168);
    pub const IMAGE_INITIAL_PORTAL_BASE_STONE_DIAMOND: ImageType = ImageType(169);
    pub const IMAGE_PLACED_PORTAL_BASE_STONE: ImageType = ImageType(170);
    pub const IMAGE_PLACED_PORTAL_BASE_STONE_AMETHYST: ImageType = ImageType(171);
    pub const IMAGE_PLACED_PORTAL_BASE_STONE_SAPPHIRE: ImageType = ImageType(172);
    pub const IMAGE_PLACED_PORTAL_BASE_STONE_EMERALD: ImageType = ImageType(173);
    pub const IMAGE_PLACED_PORTAL_BASE_STONE_RUBY: ImageType = ImageType(174);
    pub const IMAGE_PLACED_PORTAL_BASE_STONE_DIAMOND: ImageType = ImageType(175);
    pub const IMAGE_SIGN: ImageType = ImageType(176);
    pub const IMAGE_SIGN_RIGHT: ImageType = ImageType(177);
    pub const IMAGE_SIGN_GROUND_DOUBLE: ImageType = ImageType(178);
    pub const IMAGE_SIGN_GROUND_DOUBLE_RIGHT: ImageType = ImageType(179);
    pub const IMAGE_SIGN_HANG: ImageType = ImageType(180);
    pub const IMAGE_SIGN_HANG_RIGHT: ImageType = ImageType(181);
    pub const IMAGE_SIGN_GROUND_SINGLE: ImageType = ImageType(182);
    pub const IMAGE_SIGN_GROUND_SINGLE_RIGHT: ImageType = ImageType(183);
    pub const IMAGE_SIGN_SINGLE_LINE: ImageType = ImageType(184);
    pub const IMAGE_SIGN_SINGLE_LINE_DOUBLE_LEFT: ImageType = ImageType(185);
    pub const IMAGE_SIGN_SINGLE_LINE_DOUBLE_CENTER: ImageType = ImageType(186);
    pub const IMAGE_SIGN_SINGLE_LINE_DOUBLE_RIGHT: ImageType = ImageType(187);
    pub const IMAGE_CARBON_FIBER_BLOCK: ImageType = ImageType(188);
    pub const IMAGE_REFINERY_CHIMNEY: ImageType = ImageType(189);
    pub const IMAGE_TITANIUM_BLOCK: ImageType = ImageType(190);
    pub const IMAGE_TRUNK: ImageType = ImageType(192);
    pub const IMAGE_TRUNK_TOP: ImageType = ImageType(193);
    pub const IMAGE_TRUNK_DEAD: ImageType = ImageType(194);
    pub const IMAGE_TRUNK_TOP_DEAD: ImageType = ImageType(195);
    pub const IMAGE_WOOD_PLANKS: ImageType = ImageType(196);
    pub const IMAGE_LADDER: ImageType = ImageType(197);
    pub const IMAGE_BEDDING: ImageType = ImageType(198);
    pub const IMAGE_BEDDING_FOOT: ImageType = ImageType(199);
    pub const IMAGE_FLAX_MAT_RED: ImageType = ImageType(200);
    pub const IMAGE_WOOD_BED: ImageType = ImageType(201);
    pub const IMAGE_WOOD_BED_FOOT: ImageType = ImageType(202);
    pub const IMAGE_WOOD_BED_TOP: ImageType = ImageType(203);
    pub const IMAGE_WOOD_BED_TOP_FOOT: ImageType = ImageType(204);
    pub const IMAGE_WOOD_BED_HEAD: ImageType = ImageType(205);
    pub const IMAGE_TITANIUM_ORE: ImageType = ImageType(219);
    pub const IMAGE_AMETHYST_TRUNK: ImageType = ImageType(220);
    pub const IMAGE_SAPPHIRE_TRUNK: ImageType = ImageType(221);
    pub const IMAGE_EMERALD_TRUNK: ImageType = ImageType(222);
    pub const IMAGE_TRUNK_BUSH: ImageType = ImageType(224);
    pub const IMAGE_TRUNK_BUSH_MANGO: ImageType = ImageType(232);
    pub const IMAGE_BUSH_MANGO: ImageType = ImageType(233);
    pub const IMAGE_CACTUS: ImageType = ImageType(234);
    pub const IMAGE_CACTUS_DEAD: ImageType = ImageType(235);
    pub const IMAGE_PINE_TRUNK_BUSH: ImageType = ImageType(236);
    pub const IMAGE_PINE_BUSH: ImageType = ImageType(237);
    pub const IMAGE_PINE_BUSH_REAR: ImageType = ImageType(238);
    pub const IMAGE_FLAX_PLANT: ImageType = ImageType(239);
    pub const IMAGE_FLAX_PLANT_FLOWER: ImageType = ImageType(240);
    pub const IMAGE_SUNFLOWER_PLANT: ImageType = ImageType(241);
    pub const IMAGE_SUNFLOWER_PLANT_FLOWER: ImageType = ImageType(242);
    pub const IMAGE_CORN_PLANT: ImageType = ImageType(243);
    pub const IMAGE_CORN_PLANT_FLOWER: ImageType = ImageType(244);
    pub const IMAGE_CARROT_PLANT: ImageType = ImageType(245);
    pub const IMAGE_CARROT_FLOWER: ImageType = ImageType(246);
    pub const IMAGE_CHILLI_PLANT: ImageType = ImageType(247);
    pub const IMAGE_CHILLI_PLANT_FLOWER: ImageType = ImageType(248);
    pub const IMAGE_KELP_PLANT: ImageType = ImageType(249);
    pub const IMAGE_KELP_PLANT_TOP: ImageType = ImageType(250);
    pub const IMAGE_CRAFT_BENCH_LEVEL_4: ImageType = ImageType(251);
    pub const IMAGE_CRAFT_BENCH_LEVEL_4_TOP: ImageType = ImageType(252);
    pub const IMAGE_RUBY_TRUNK: ImageType = ImageType(253);
    pub const IMAGE_DIAMOND_TRUNK: ImageType = ImageType(254);
    pub const IMAGE_BUSH: ImageType = ImageType(256);
    pub const IMAGE_APPLE_TRUNK: ImageType = ImageType(264);
    pub const IMAGE_CHERRY_BUSH: ImageType = ImageType(265);
    pub const IMAGE_CHERRY_TRUNK: ImageType = ImageType(273);
    pub const IMAGE_CHERRY_TRUNK_BUSH: ImageType = ImageType(274);
    pub const IMAGE_COFFEE_TRUNK: ImageType = ImageType(282);
    pub const IMAGE_COFFEE_BUSH: ImageType = ImageType(283);
    pub const IMAGE_COFFEE_TRUNK_BUSH: ImageType = ImageType(284);
    pub const IMAGE_ELECTRIC_KILN_ELEMENT: ImageType = ImageType(285);
    pub const IMAGE_ELECTRIC_INIDCATOR_LIGHT: ImageType = ImageType(286);
    pub const IMAGE_DIRT_FLINT: ImageType = ImageType(288);
    pub const IMAGE_DIRT_FLINT_GRASS_SIDE: ImageType = ImageType(289);
    pub const IMAGE_DIRT_CLAY: ImageType = ImageType(290);
    pub const IMAGE_DIRT_CLAY_GRASS_SIDE: ImageType = ImageType(291);
    pub const IMAGE_DIRT_FLINT_GRASS_SIDE_FROZEN: ImageType = ImageType(292);
    pub const IMAGE_DIRT_CLAY_GRASS_SIDE_FROZEN: ImageType = ImageType(293);
    pub const IMAGE_COMPOST_GRASS_TOP: ImageType = ImageType(294);
    pub const IMAGE_UNUSED: ImageType = ImageType(295);
    pub const IMAGE_WORKBENCH_PRESS_2: ImageType = ImageType(296);
    pub const IMAGE_WORKBENCH_PRESS_2_TOP: ImageType = ImageType(297);
    pub const IMAGE_SAFE: ImageType = ImageType(298);
    pub const IMAGE_SAFE_TOP: ImageType = ImageType(299);
    pub const IMAGE_METALWORK_BENCH: ImageType = ImageType(300);
    pub const IMAGE_METALWORK_BENCH_TOP: ImageType = ImageType(301);
    pub const IMAGE_CHEST_GOLD: ImageType = ImageType(302);
    pub const IMAGE_CHEST_GOLD_TOP: ImageType = ImageType(303);
    pub const IMAGE_TRADING_POST: ImageType = ImageType(304);
    pub const IMAGE_TRADING_POST_TOP: ImageType = ImageType(305);
    pub const IMAGE_GOLD_BED: ImageType = ImageType(306);
    pub const IMAGE_GOLD_BED_FOOT: ImageType = ImageType(307);
    pub const IMAGE_GOLD_BED_TOP: ImageType = ImageType(308);
    pub const IMAGE_GOLD_BED_TOP_FOOT: ImageType = ImageType(309);
    pub const IMAGE_GOLD_BED_HEAD: ImageType = ImageType(310);
    pub const IMAGE_CHEST_PORTAL: ImageType = ImageType(311);
    pub const IMAGE_CHEST_PORTAL_TOP: ImageType = ImageType(312);
    pub const IMAGE_WORKBENCH_LEVEL_4: ImageType = ImageType(313);
    pub const IMAGE_WORKBENCH_LEVEL_4_TOP: ImageType = ImageType(314);
    pub const IMAGE_STEAM_GENERATOR: ImageType = ImageType(315);
    pub const IMAGE_STEAM_GENERATOR_TOP: ImageType = ImageType(316);
    pub const IMAGE_ELECTRIC_KILN: ImageType = ImageType(317);
    pub const IMAGE_ELECTRIC_KILN_TOP: ImageType = ImageType(318);
    pub const IMAGE_WORKBENCH_LEVEL_1: ImageType = ImageType(320);
    pub const IMAGE_WORKBENCH_LEVEL_1_TOP: ImageType = ImageType(321);
    pub const IMAGE_WORKBENCH_TOOL_1: ImageType = ImageType(322);
    pub const IMAGE_WORKBENCH_TOOL_1_TOP: ImageType = ImageType(323);
    pub const IMAGE_FURNACE_1: ImageType = ImageType(324);
    pub const IMAGE_FURNACE_1_TOP: ImageType = ImageType(325);
    pub const IMAGE_WORKBENCH_LEVEL_2: ImageType = ImageType(326);
    pub const IMAGE_WORKBENCH_LEVEL_2_TOP: ImageType = ImageType(327);
    pub const IMAGE_FURNACE_2: ImageType = ImageType(328);
    pub const IMAGE_FURNACE_2_TOP: ImageType = ImageType(329);
    pub const IMAGE_WORKBENCH_TOOL_3: ImageType = ImageType(330);
    pub const IMAGE_WORKBENCH_TOOL_3_TOP: ImageType = ImageType(331);
    pub const IMAGE_WORKBENCH_TOOL_2: ImageType = ImageType(332);
    pub const IMAGE_WORKBENCH_TOOL_2_TOP: ImageType = ImageType(333);
    pub const IMAGE_WORKBENCH_WEAVE_2: ImageType = ImageType(334);
    pub const IMAGE_WORKBENCH_WEAVE_2_TOP: ImageType = ImageType(335);
    pub const IMAGE_WORKBENCH_WOOD_1: ImageType = ImageType(336);
    pub const IMAGE_WORKBENCH_WOOD_1_TOP: ImageType = ImageType(337);
    pub const IMAGE_WORKBENCH_WEAVE_1: ImageType = ImageType(338);
    pub const IMAGE_WORKBENCH_WEAVE_1_TOP: ImageType = ImageType(339);
    pub const IMAGE_WORKBENCH_PRESS_1: ImageType = ImageType(340);
    pub const IMAGE_WORKBENCH_PRESS_1_TOP: ImageType = ImageType(341);
    pub const IMAGE_CHEST: ImageType = ImageType(342);
    pub const IMAGE_CHEST_TOP: ImageType = ImageType(343);
    pub const IMAGE_KILN: ImageType = ImageType(344);
    pub const IMAGE_KILN_TOP: ImageType = ImageType(345);
    pub const IMAGE_FURNACE_3: ImageType = ImageType(346);
    pub const IMAGE_FURNACE_3_TOP: ImageType = ImageType(347);
    pub const IMAGE_WORKBENCH_TOOL_4: ImageType = ImageType(348);
    pub const IMAGE_WORKBENCH_TOOL_4_TOP: ImageType = ImageType(349);
    pub const IMAGE_FALLEN_LEAVES: ImageType = ImageType(350);
    pub const IMAGE_CAMPFIRE_0: ImageType = ImageType(352);
    pub const IMAGE_CAMPFIRE_1: ImageType = ImageType(353);
    pub const IMAGE_CAMPFIRE_2: ImageType = ImageType(354);
    pub const IMAGE_CAMPFIRE_3: ImageType = ImageType(355);
    pub const IMAGE_CAMPFIRE_4: ImageType = ImageType(356);
    pub const IMAGE_CAMPFIRE_5: ImageType = ImageType(357);
    pub const IMAGE_CAMPFIRE_6: ImageType = ImageType(358);
    pub const IMAGE_CAMPFIRE_7: ImageType = ImageType(359);
    pub const IMAGE_STONE_KILN_0: ImageType = ImageType(360);
    pub const IMAGE_STONE_KILN_1: ImageType = ImageType(361);
    pub const IMAGE_STONE_KILN_2: ImageType = ImageType(362);
    pub const IMAGE_STONE_KILN_3: ImageType = ImageType(363);
    pub const IMAGE_STONE_KILN_4: ImageType = ImageType(364);
    pub const IMAGE_STONE_KILN_5: ImageType = ImageType(365);
    pub const IMAGE_STONE_KILN_6: ImageType = ImageType(366);
    pub const IMAGE_STONE_KILN_7: ImageType = ImageType(367);
    pub const IMAGE_WORKBENCH_TOOL_6: ImageType = ImageType(368);
    pub const IMAGE_WORKBENCH_TOOL_6_TOP: ImageType = ImageType(369);
    pub const IMAGE_WORKBENCH_TOOL_5: ImageType = ImageType(370);
    pub const IMAGE_WORKBENCH_TOOL_5_TOP: ImageType = ImageType(371);
    pub const IMAGE_CRAFT_BENCH_LEVEL_2: ImageType = ImageType(372);
    pub const IMAGE_CRAFT_BENCH_LEVEL_2_TOP: ImageType = ImageType(373);
    pub const IMAGE_MIX_BENCH_LEVEL_1: ImageType = ImageType(374);
    pub const IMAGE_MIX_BENCH_LEVEL_1_TOP: ImageType = ImageType(375);
    pub const IMAGE_DYE_BENCH_LEVEL_1: ImageType = ImageType(376);
    pub const IMAGE_DYE_BENCH_LEVEL_1_TOP: ImageType = ImageType(377);
    pub const IMAGE_CRAFT_BENCH_LEVEL_1: ImageType = ImageType(378);
    pub const IMAGE_CRAFT_BENCH_LEVEL_1_TOP: ImageType = ImageType(379);
    pub const IMAGE_CRAFT_BENCH_LEVEL_3: ImageType = ImageType(380);
    pub const IMAGE_CRAFT_BENCH_LEVEL_3_TOP: ImageType = ImageType(381);
    pub const IMAGE_VINE_PLANT_TOP: ImageType = ImageType(382);
    pub const IMAGE_CAMPFIRE_OUT_0: ImageType = ImageType(384);
    pub const IMAGE_CAMPFIRE_OUT_1: ImageType = ImageType(385);
    pub const IMAGE_CAMPFIRE_OUT_2: ImageType = ImageType(386);
    pub const IMAGE_CAMPFIRE_OUT_3: ImageType = ImageType(387);
    pub const IMAGE_CAMPFIRE_OUT_4: ImageType = ImageType(388);
    pub const IMAGE_CAMPFIRE_OUT_5: ImageType = ImageType(389);
    pub const IMAGE_CAMPFIRE_OUT_6: ImageType = ImageType(390);
    pub const IMAGE_CAMPFIRE_OUT_7: ImageType = ImageType(391);
    pub const IMAGE_ICE_TORCH_0: ImageType = ImageType(392);
    pub const IMAGE_BASIC_TORCH_0: ImageType = ImageType(416);
    pub const IMAGE_BASIC_TORCH_1: ImageType = ImageType(417);
    pub const IMAGE_BASIC_TORCH_2: ImageType = ImageType(418);
    pub const IMAGE_BASIC_TORCH_3: ImageType = ImageType(419);
    pub const IMAGE_BASIC_TORCH_4: ImageType = ImageType(420);
    pub const IMAGE_BASIC_TORCH_5: ImageType = ImageType(421);
    pub const IMAGE_BASIC_TORCH_6: ImageType = ImageType(422);
    pub const IMAGE_BASIC_TORCH_7: ImageType = ImageType(423);
    pub const IMAGE_CLAY_LANTERN_0: ImageType = ImageType(424);
    pub const IMAGE_CLAY_LANTERN_1: ImageType = ImageType(425);
    pub const IMAGE_CLAY_LANTERN_2: ImageType = ImageType(426);
    pub const IMAGE_CLAY_LANTERN_3: ImageType = ImageType(427);
    pub const IMAGE_CLAY_LANTERN_4: ImageType = ImageType(428);
    pub const IMAGE_CLAY_LANTERN_5: ImageType = ImageType(429);
    pub const IMAGE_CLAY_LANTERN_6: ImageType = ImageType(430);
    pub const IMAGE_CLAY_LANTERN_7: ImageType = ImageType(431);
    pub const IMAGE_PORTAL_DIAMOND_0: ImageType = ImageType(432);
    pub const IMAGE_EMERALD_CHANDELIER: ImageType = ImageType(440);
    pub const IMAGE_RUBY_CHANDELIER: ImageType = ImageType(442);
    pub const IMAGE_DIAMOND_CHANDELIER: ImageType = ImageType(444);
    pub const IMAGE_VINE_PLANT: ImageType = ImageType(446);
    pub const IMAGE_TRUNK_BUSH_DEAD: ImageType = ImageType(448);
    pub const IMAGE_BUSH_DEAD: ImageType = ImageType(449);
    pub const IMAGE_BUSH_DEAD_REAR: ImageType = ImageType(450);
    pub const IMAGE_CAMPFIRE_UPGRADE: ImageType = ImageType(451);
    pub const IMAGE_CAMPFIRE_UPGRADED_COMBINED: ImageType = ImageType(452);
    pub const IMAGE_STEEL_LANTERN_0: ImageType = ImageType(456);
    pub const IMAGE_ELECTRIC_FURNACE: ImageType = ImageType(474);
    pub const IMAGE_ELECTRIC_FURNACE_TOP: ImageType = ImageType(475);
    pub const IMAGE_ELECTRIC_METALWORK_BENCH: ImageType = ImageType(476);
    pub const IMAGE_ELECTRIC_METALWORK_BENCH_TOP: ImageType = ImageType(477);
    pub const IMAGE_GRAVEL: ImageType = ImageType(478);
    pub const IMAGE_MAPLE_TRUNK_BUSH: ImageType = ImageType(480);
    pub const IMAGE_MAPLE_TRUNK: ImageType = ImageType(488);
    pub const IMAGE_COCONUT_BUSH: ImageType = ImageType(489);
    pub const IMAGE_COCONUT_TRUNK: ImageType = ImageType(490);
    pub const IMAGE_COCONUT_BUSH_DEAD: ImageType = ImageType(491);
    pub const IMAGE_COCONUT_TRUNK_DEAD: ImageType = ImageType(492);
    pub const IMAGE_ORANGE_BUSH: ImageType = ImageType(493);
    pub const IMAGE_ORANGE_TRUNK_BUSH: ImageType = ImageType(494);
    pub const IMAGE_ORANGE_TRUNK: ImageType = ImageType(495);
    pub const IMAGE_PORTAL_RUBY_0: ImageType = ImageType(496);
    pub const IMAGE_SHARK_JAW: ImageType = ImageType(504);
    pub const IMAGE_SHARK_JAW_RIGHT: ImageType = ImageType(505);
    pub const IMAGE_ARMOR_BENCH: ImageType = ImageType(506);
    pub const IMAGE_ARMOR_BENCH_TOP: ImageType = ImageType(507);
    pub const IMAGE_ARMOR_BENCH_LEVEL_2: ImageType = ImageType(508);
    pub const IMAGE_ARMOR_BENCH_LEVEL_2_TOP: ImageType = ImageType(509);
    pub const IMAGE_GRAVEL_PAINTED: ImageType = ImageType(510);
    pub const IMAGE_MAPLE_BUSH: ImageType = ImageType(512);
    pub const IMAGE_LIME_BUSH: ImageType = ImageType(520);
    pub const IMAGE_LIME_TRUNK_BUSH: ImageType = ImageType(521);
    pub const IMAGE_LIME_TRUNK: ImageType = ImageType(522);
    pub const IMAGE_STEEL_DOWNLIGHT: ImageType = ImageType(527);
    pub const IMAGE_TRAIN_YARD: ImageType = ImageType(536);
    pub const IMAGE_TRAIN_YARD_TOP: ImageType = ImageType(537);
    pub const IMAGE_WORKBENCH_LEVEL_3: ImageType = ImageType(538);
    pub const IMAGE_WORKBENCH_LEVEL_3_TOP: ImageType = ImageType(539);
    pub const IMAGE_BUILDERS_BENCH_LEVEL_1: ImageType = ImageType(540);
    pub const IMAGE_BUILDERS_BENCH_LEVEL_1_TOP: ImageType = ImageType(541);
    pub const IMAGE_DODO_EGG_BREEDS: ImageType = ImageType(542);
    pub const IMAGE_ELECTRIC_STOVE: ImageType = ImageType(544);
    pub const IMAGE_ELECTRIC_STOVE_TOP: ImageType = ImageType(545);
    pub const IMAGE_ELECTRIC_STOVE_TOP_GLOW: ImageType = ImageType(546);
    pub const IMAGE_SOLAR_PANEL: ImageType = ImageType(547);
    pub const IMAGE_SOLAR_PANEL_TOP: ImageType = ImageType(548);
    pub const IMAGE_FLYWHEEL: ImageType = ImageType(549);
    pub const IMAGE_FLYWHEEL_TOP: ImageType = ImageType(550);
    pub const IMAGE_FLYWHEEL_WHEEL: ImageType = ImageType(551);
    pub const IMAGE_NORTH_POLE: ImageType = ImageType(552);
    pub const IMAGE_SOUTH_POLE: ImageType = ImageType(553);
    pub const IMAGE_EQUATOR_POLE: ImageType = ImageType(554);
    pub const IMAGE_AMETHYST_CHANDELIER: ImageType = ImageType(555);
    pub const IMAGE_AMETHYST_CHANDELIER_RIGHT: ImageType = ImageType(556);
    pub const IMAGE_SAPPHIRE_CHANDELIER: ImageType = ImageType(557);
    pub const IMAGE_SAPPHIRE_CHANDELIER_RIGHT: ImageType = ImageType(558);
    pub const IMAGE_STEEL_UPLIGHT: ImageType = ImageType(559);
    pub const IMAGE_PORTAL_EMERALD_0: ImageType = ImageType(560);
    pub const IMAGE_ARMOR_BENCH_LEVEL_3: ImageType = ImageType(568);
    pub const IMAGE_ARMOR_BENCH_LEVEL_3_TOP: ImageType = ImageType(569);
    pub const IMAGE_AMETHYST_BUSH: ImageType = ImageType(570);
    pub const IMAGE_SAPPHIRE_BUSH: ImageType = ImageType(571);
    pub const IMAGE_EMERALD_BUSH: ImageType = ImageType(572);
    pub const IMAGE_RUBY_BUSH: ImageType = ImageType(573);
    pub const IMAGE_DIAMOND_BUSH: ImageType = ImageType(574);
    pub const IMAGE_SOLAR_INDICATOR_LIGHTS: ImageType = ImageType(576);
    pub const IMAGE_METALWORK_BENCH_LEVEL_2: ImageType = ImageType(577);
    pub const IMAGE_METALWORK_BENCH_LEVEL_2_TOP: ImageType = ImageType(578);
    pub const IMAGE_STEAM_LOCO_LEFT: ImageType = ImageType(579);
    pub const IMAGE_STEAM_LOCO_RIGHT: ImageType = ImageType(580);
    pub const IMAGE_STEAM_LOCO_BACK_LEFT: ImageType = ImageType(581);
    pub const IMAGE_STEAM_LOCO_BACK_RIGHT: ImageType = ImageType(582);
    pub const IMAGE_ELEVATOR_SHAFT: ImageType = ImageType(583);
    pub const IMAGE_ELEVATOR_MOTOR: ImageType = ImageType(584);
    pub const IMAGE_CHEST_PAINTED: ImageType = ImageType(585);
    pub const IMAGE_CHEST_TOP_PAINTED: ImageType = ImageType(586);
    pub const IMAGE_WOOD_BED_PAINTED: ImageType = ImageType(587);
    pub const IMAGE_WOOD_BED_FOOT_PAINTED: ImageType = ImageType(588);
    pub const IMAGE_WOOD_BED_TOP_PAINTED: ImageType = ImageType(589);
    pub const IMAGE_WOOD_BED_TOP_FOOT_PAINTED: ImageType = ImageType(590);
    pub const IMAGE_WOOD_BED_HEAD_PAINTED: ImageType = ImageType(591);
    pub const IMAGE_TULIP_PLANT_SPROUT: ImageType = ImageType(600);
    pub const IMAGE_TULIP_PLANT_MATURE: ImageType = ImageType(601);
    pub const IMAGE_AMETHYST_TRUNK_BUSH: ImageType = ImageType(602);
    pub const IMAGE_SAPPHIRE_TRUNK_BUSH: ImageType = ImageType(603);
    pub const IMAGE_EMERALD_TRUNK_BUSH: ImageType = ImageType(604);
    pub const IMAGE_RUBY_TRUNK_BUSH: ImageType = ImageType(605);
    pub const IMAGE_DIAMOND_TRUNK_BUSH: ImageType = ImageType(606);
    pub const IMAGE_SIGN_PAINTED: ImageType = ImageType(608);
    pub const IMAGE_SIGN_RIGHT_PAINTED: ImageType = ImageType(609);
    pub const IMAGE_SIGN_GROUND_DOUBLE_PAINTED: ImageType = ImageType(610);
    pub const IMAGE_SIGN_GROUND_DOUBLE_RIGHT_PAINTED: ImageType = ImageType(611);
    pub const IMAGE_SIGN_HANG_PAINTED: ImageType = ImageType(612);
    pub const IMAGE_SIGN_HANG_RIGHT_PAINTED: ImageType = ImageType(613);
    pub const IMAGE_SIGN_GROUND_SINGLE_PAINTED: ImageType = ImageType(614);
    pub const IMAGE_SIGN_GROUND_SINGLE_RIGHT_PAINTED: ImageType = ImageType(615);
    pub const IMAGE_SAFE_PAINTED: ImageType = ImageType(616);
    pub const IMAGE_SAFE_TOP_PAINTED: ImageType = ImageType(617);
    pub const IMAGE_LADDER_PAINTED: ImageType = ImageType(618);
    pub const IMAGE_GOLD_BED_PAINTED: ImageType = ImageType(619);
    pub const IMAGE_GOLD_BED_FOOT_PAINTED: ImageType = ImageType(620);
    pub const IMAGE_GOLD_BED_TOP_PAINTED: ImageType = ImageType(621);
    pub const IMAGE_GOLD_BED_TOP_FOOT_PAINTED: ImageType = ImageType(622);
    pub const IMAGE_GOLD_BED_HEAD_PAINTED: ImageType = ImageType(623);
    pub const IMAGE_PORTAL_SAPPHIRE_0: ImageType = ImageType(624);
    pub const IMAGE_BUILDERS_BENCH_LEVEL_2: ImageType = ImageType(632);
    pub const IMAGE_BUILDERS_BENCH_LEVEL_2_TOP: ImageType = ImageType(633);
    pub const IMAGE_WORKBENCH_TOOL_7: ImageType = ImageType(634);
    pub const IMAGE_WORKBENCH_TOOL_7_TOP: ImageType = ImageType(635);
    pub const IMAGE_ELECTRIC_PRESS: ImageType = ImageType(636);
    pub const IMAGE_ELECTRIC_PRESS_TOP: ImageType = ImageType(637);
    pub const IMAGE_DODO_EGG_BREEDS_ITEM: ImageType = ImageType(638);
    pub const IMAGE_SIGN_SINGLE_LINE_PAINTED: ImageType = ImageType(640);
    pub const IMAGE_SIGN_SINGLE_LINE_DOUBLE_LEFT_PAINTED: ImageType = ImageType(641);
    pub const IMAGE_SIGN_SINGLE_LINE_DOUBLE_CENTER_PAINTED: ImageType = ImageType(642);
    pub const IMAGE_SIGN_SINGLE_LINE_DOUBLE_RIGHT_PAINTED: ImageType = ImageType(643);
    pub const IMAGE_TRADING_POST_PAINTED: ImageType = ImageType(644);
    pub const IMAGE_CARBON_FIBER_BLOCK_PAINTED: ImageType = ImageType(645);
    pub const IMAGE_TITANIUM_BLOCK_PAINTED: ImageType = ImageType(646);
    pub const IMAGE_GEM_TRUNK_PAINTED: ImageType = ImageType(647);
    pub const IMAGE_OWNERSHIP_SIGN: ImageType = ImageType(648);
    pub const IMAGE_OWNERSHIP_SIGN_RIGHT: ImageType = ImageType(649);
    pub const IMAGE_OWNERSHIP_SIGN_GROUND_DOUBLE: ImageType = ImageType(650);
    pub const IMAGE_OWNERSHIP_SIGN_GROUND_DOUBLE_RIGHT: ImageType = ImageType(651);
    pub const IMAGE_OWNERSHIP_SIGN_HANG: ImageType = ImageType(652);
    pub const IMAGE_OWNERSHIP_SIGN_HANG_RIGHT: ImageType = ImageType(653);
    pub const IMAGE_OWNERSHIP_SIGN_GROUND_SINGLE: ImageType = ImageType(654);
    pub const IMAGE_OWNERSHIP_SIGN_GROUND_SINGLE_RIGHT: ImageType = ImageType(655);
    pub const IMAGE_COMPOST_BIN: ImageType = ImageType(664);
    pub const IMAGE_COMPOST_BIN_TOP: ImageType = ImageType(665);
    pub const IMAGE_FLINT_BLOCK: ImageType = ImageType(666);
    pub const IMAGE_CLAY_BLOCK: ImageType = ImageType(667);
    pub const IMAGE_OIL_BLOCK: ImageType = ImageType(668);
    pub const IMAGE_FUEL_BLOCK: ImageType = ImageType(669);
    pub const IMAGE_COAL_BLOCK: ImageType = ImageType(670);
    pub const IMAGE_PLATINUM_0: ImageType = ImageType(672);
    pub const IMAGE_PORTAL_AMETHYST_0: ImageType = ImageType(688);
    pub const IMAGE_GATE: ImageType = ImageType(696);
    pub const IMAGE_AMETHYST_BLOCK: ImageType = ImageType(697);
    pub const IMAGE_SAPPHIRE_BLOCK: ImageType = ImageType(698);
    pub const IMAGE_EMERALD_BLOCK: ImageType = ImageType(699);
    pub const IMAGE_RUBY_BLOCK: ImageType = ImageType(700);
    pub const IMAGE_DIAMOND_BLOCK: ImageType = ImageType(701);
    pub const IMAGE_RAINBOW_BLOCK: ImageType = ImageType(702);
    pub const IMAGE_PLATINUM_PAINTED_0: ImageType = ImageType(704);
    pub const IMAGE_GEM_BLOCK_PAINTED: ImageType = ImageType(733);
    pub const IMAGE_EGG_EXTRACTOR: ImageType = ImageType(736);
    pub const IMAGE_EGG_EXTRACTOR_TOP: ImageType = ImageType(737);
    pub const IMAGE_EGG_EXTRACTOR_WHEEL: ImageType = ImageType(738);
    pub const IMAGE_PIZZA_OVEN: ImageType = ImageType(739);
    pub const IMAGE_PIZZA_OVEN_TOP: ImageType = ImageType(740);
    pub const IMAGE_PLASTER: ImageType = ImageType(741);
    pub const IMAGE_CHEST_FEEDER: ImageType = ImageType(742);
    pub const IMAGE_CHEST_FEEDER_TOP: ImageType = ImageType(743);
    pub const IMAGE_CHEST_FEEDER_PAINTED: ImageType = ImageType(744);
    pub const IMAGE_CHEST_FEEDER_TOP_PAINTED: ImageType = ImageType(745);
    pub const IMAGE_LUMINOUS_PLASTER: ImageType = ImageType(746);
    pub const IMAGE_DODO_EGG_ITEM: ImageType = ImageType(747);
    pub const IMAGE_TRADE_PORTAL_0: ImageType = ImageType(752);
    pub const IMAGE_WHEAT_PLANT: ImageType = ImageType(760);
    pub const IMAGE_WHEAT_PLANT_FLOWER: ImageType = ImageType(761);
    pub const IMAGE_TOMATO_PLANT: ImageType = ImageType(762);
    pub const IMAGE_TOMATO_PLANT_FLOWER: ImageType = ImageType(763);
    pub const IMAGE_MIRROR: ImageType = ImageType(764);
    pub const IMAGE_OWNERSHIP_SIGN_PAINTED: ImageType = ImageType(768);
    pub const IMAGE_OWNERSHIP_SIGN_RIGHT_PAINTED: ImageType = ImageType(769);
    pub const IMAGE_OWNERSHIP_SIGN_GROUND_DOUBLE_PAINTED: ImageType = ImageType(770);
    pub const IMAGE_OWNERSHIP_SIGN_GROUND_DOUBLE_RIGHT_PAINTED: ImageType = ImageType(771);
    pub const IMAGE_OWNERSHIP_SIGN_HANG_PAINTED: ImageType = ImageType(772);
    pub const IMAGE_OWNERSHIP_SIGN_HANG_RIGHT_PAINTED: ImageType = ImageType(773);
    pub const IMAGE_OWNERSHIP_SIGN_GROUND_SINGLE_PAINTED: ImageType = ImageType(774);
    pub const IMAGE_OWNERSHIP_SIGN_GROUND_SINGLE_RIGHT_PAINTED: ImageType = ImageType(775);
    pub const IMAGE_KILN_LEVEL_2: ImageType = ImageType(776);
    pub const IMAGE_KILN_LEVEL_2_TOP: ImageType = ImageType(777);
    pub const IMAGE_CRAFT_BENCH_EXPERT_LEVEL_3: ImageType = ImageType(778);
    pub const IMAGE_CRAFT_BENCH_EXPERT_LEVEL_3_TOP: ImageType = ImageType(779);
    pub const IMAGE_CRAFT_BENCH_EXPERT_LEVEL_4: ImageType = ImageType(780);
    pub const IMAGE_CRAFT_BENCH_EXPERT_LEVEL_4_TOP: ImageType = ImageType(781);
    pub const IMAGE_CRAFT_BENCH_EXPERT_LEVEL_5: ImageType = ImageType(782);
    pub const IMAGE_CRAFT_BENCH_EXPERT_LEVEL_5_TOP: ImageType = ImageType(783);
    pub const IMAGE_CRAFT_BENCH_EXPERT_LEVEL_6: ImageType = ImageType(784);
    pub const IMAGE_CRAFT_BENCH_EXPERT_LEVEL_6_TOP: ImageType = ImageType(785);
    pub const IMAGE_CRAFT_BENCH_EXPERT_LEVEL_7: ImageType = ImageType(786);
    pub const IMAGE_CRAFT_BENCH_EXPERT_LEVEL_7_TOP: ImageType = ImageType(787);
    pub const IMAGE_WORKBENCH_PRESS_3: ImageType = ImageType(788);
    pub const IMAGE_WORKBENCH_PRESS_4: ImageType = ImageType(789);
    pub const IMAGE_WORKBENCH_PRESS_5: ImageType = ImageType(790);
    pub const IMAGE_WORKBENCH_PRESS_6: ImageType = ImageType(791);
    pub const IMAGE_WORKBENCH_PRESS_7: ImageType = ImageType(792);
    pub const IMAGE_METALWORK_BENCH_EXPERT_2: ImageType = ImageType(793);
    pub const IMAGE_METALWORK_BENCH_EXPERT_3: ImageType = ImageType(794);
    pub const IMAGE_METALWORK_BENCH_EXPERT_4: ImageType = ImageType(795);
    pub const IMAGE_METALWORK_BENCH_EXPERT_5: ImageType = ImageType(796);
    pub const IMAGE_METALWORK_BENCH_EXPERT_6: ImageType = ImageType(797);
    pub const IMAGE_WORKBENCH_EXPERT_1: ImageType = ImageType(800);
    pub const IMAGE_WORKBENCH_EXPERT_2: ImageType = ImageType(801);
    pub const IMAGE_WORKBENCH_EXPERT_3: ImageType = ImageType(802);
    pub const IMAGE_WORKBENCH_EXPERT_4: ImageType = ImageType(803);
    pub const IMAGE_WORKBENCH_EXPERT_5: ImageType = ImageType(804);
    pub const IMAGE_WORKBENCH_EXPERT_6: ImageType = ImageType(805);
    pub const IMAGE_WORKBENCH_EXPERT_7: ImageType = ImageType(806);
    pub const IMAGE_WORKBENCH_EXPERT_8: ImageType = ImageType(807);
    pub const IMAGE_WORKBENCH_EXPERT_9: ImageType = ImageType(808);
    pub const IMAGE_WORKBENCH_EXPERT_11: ImageType = ImageType(809);
    pub const IMAGE_WORKBENCH_EXPERT_12: ImageType = ImageType(810);
    pub const IMAGE_WORKBENCH_EXPERT_13: ImageType = ImageType(811);
    pub const IMAGE_WORKBENCH_EXPERT_14: ImageType = ImageType(812);
    pub const IMAGE_WORKBENCH_EXPERT_15: ImageType = ImageType(813);
    pub const IMAGE_WORKBENCH_EXPERT_16: ImageType = ImageType(814);
    pub const IMAGE_WORKBENCH_EXPERT_17: ImageType = ImageType(815);
    pub const IMAGE_WORKBENCH_EXPERT_18: ImageType = ImageType(816);
    pub const IMAGE_WORKBENCH_EXPERT_20: ImageType = ImageType(817);
    pub const IMAGE_WORKBENCH_EXPERT_21: ImageType = ImageType(818);
    pub const IMAGE_WORKBENCH_EXPERT_22: ImageType = ImageType(819);
    pub const IMAGE_WORKBENCH_EXPERT_23: ImageType = ImageType(820);
    pub const IMAGE_WORKBENCH_EXPERT_24: ImageType = ImageType(821);
    pub const IMAGE_WORKBENCH_EXPERT_25: ImageType = ImageType(822);
    pub const IMAGE_WORKBENCH_EXPERT_26: ImageType = ImageType(823);
    pub const IMAGE_WORKBENCH_EXPERT_27: ImageType = ImageType(824);
    #[inline]
    pub const fn raw(self) -> u32 {
        self.0
    }
    #[inline]
    pub const fn from_raw(v: u32) -> Self {
        Self(v)
    }
    /// First-declared constant name for this value (None for unknown/unnamed values).
    pub fn name(self) -> Option<&'static str> {
        match self.0 {
            0 => Some("IMAGE_AIR"),
            1 => Some("IMAGE_ROCK_COPPER"),
            2 => Some("IMAGE_ROCK_IRON"),
            3 => Some("IMAGE_ROCK_TIN"),
            4 => Some("IMAGE_FIRE_SMOKE_0"),
            5 => Some("IMAGE_FIRE_SMOKE_1"),
            6 => Some("IMAGE_FIRE_SMOKE_2"),
            7 => Some("IMAGE_FIRE_SMOKE_3"),
            8 => Some("IMAGE_FIRE_SMOKE_4"),
            9 => Some("IMAGE_FIRE_SMOKE_5"),
            10 => Some("IMAGE_FIRE_SMOKE_6"),
            11 => Some("IMAGE_FIRE_SMOKE_7"),
            12 => Some("IMAGE_DOOR_TOP"),
            13 => Some("IMAGE_TRAPDOOR_TOP"),
            14 => Some("IMAGE_WATER_0"),
            15 => Some("IMAGE_WATER_1"),
            16 => Some("IMAGE_WATER_2"),
            17 => Some("IMAGE_WATER_3"),
            18 => Some("IMAGE_WATER_4"),
            19 => Some("IMAGE_WATER_5"),
            20 => Some("IMAGE_WATER_6"),
            21 => Some("IMAGE_WATER_7"),
            32 => Some("IMAGE_ROCK"),
            33 => Some("IMAGE_COBBLESTONE"),
            34 => Some("IMAGE_RED_BRICKS"),
            35 => Some("IMAGE_LIMESTONE_BLOCK"),
            36 => Some("IMAGE_FIRE_FLAME_0"),
            37 => Some("IMAGE_FIRE_FLAME_1"),
            38 => Some("IMAGE_FIRE_FLAME_2"),
            39 => Some("IMAGE_FIRE_FLAME_3"),
            40 => Some("IMAGE_FIRE_FLAME_4"),
            41 => Some("IMAGE_FIRE_FLAME_5"),
            42 => Some("IMAGE_FIRE_FLAME_6"),
            43 => Some("IMAGE_FIRE_FLAME_7"),
            44 => Some("IMAGE_DOOR"),
            45 => Some("IMAGE_TRAPDOOR"),
            46 => Some("IMAGE_LAVA_0"),
            62 => Some("IMAGE_PORTAL_NET"),
            64 => Some("IMAGE_DIRT"),
            65 => Some("IMAGE_SAND"),
            66 => Some("IMAGE_LIMESTONE"),
            67 => Some("IMAGE_MARBLE"),
            76 => Some("IMAGE_MARBLE_BLOCK"),
            77 => Some("IMAGE_TIME_CRYSTAL_BLOCK"),
            78 => Some("IMAGE_OIL_IN_BLOCK"),
            79 => Some("IMAGE_SANDSTONE"),
            80 => Some("IMAGE_SANDSTONE_BLOCK"),
            81 => Some("IMAGE_RED_MARBLE"),
            82 => Some("IMAGE_RED_MARBLE_BLOCK"),
            83 => Some("IMAGE_ROCK_COAL"),
            84 => Some("IMAGE_ROCK_GOLD"),
            85 => Some("IMAGE_GOLD_BLOCK"),
            86 => Some("IMAGE_LAPIS"),
            87 => Some("IMAGE_LAPIS_BLOCK"),
            88 => Some("IMAGE_BASALT"),
            89 => Some("IMAGE_BLACK_SAND"),
            96 => Some("IMAGE_WATER"),
            97 => Some("IMAGE_COMPOST"),
            98 => Some("IMAGE_COMPOST_GRASS"),
            100 => Some("IMAGE_PORTAL_0"),
            101 => Some("IMAGE_PORTAL_1"),
            102 => Some("IMAGE_PORTAL_2"),
            103 => Some("IMAGE_PORTAL_3"),
            104 => Some("IMAGE_PORTAL_4"),
            105 => Some("IMAGE_PORTAL_5"),
            106 => Some("IMAGE_PORTAL_6"),
            107 => Some("IMAGE_PORTAL_7"),
            108 => Some("IMAGE_GLASS"),
            109 => Some("IMAGE_WINDOW"),
            110 => Some("IMAGE_ICE"),
            111 => Some("IMAGE_UNUSED_ICEPLUSONE"),
            112 => Some("IMAGE_COPPER_BLOCK"),
            113 => Some("IMAGE_TIN_BLOCK"),
            114 => Some("IMAGE_BRONZE_BLOCK"),
            115 => Some("IMAGE_IRON_BLOCK"),
            116 => Some("IMAGE_STEEL_BLOCK"),
            117 => Some("IMAGE_BLACK_GLASS"),
            118 => Some("IMAGE_BLACK_WINDOW"),
            119 => Some("IMAGE_DISPLAY_CABINET_FRONT"),
            120 => Some("IMAGE_IRON_DOOR_TOP"),
            121 => Some("IMAGE_SAPPHIRE"),
            122 => Some("IMAGE_AMETHYST"),
            123 => Some("IMAGE_DIAMOND"),
            124 => Some("IMAGE_RUBY"),
            125 => Some("IMAGE_EMERALD"),
            128 => Some("IMAGE_CLOUD"),
            129 => Some("IMAGE_SNOW"),
            130 => Some("IMAGE_COMPOST_GRASS_FROZEN"),
            131 => Some("IMAGE_PORTAL_ADD"),
            132 => Some("IMAGE_INITIAL_PORTAL_BASE_STONE"),
            133 => Some("IMAGE_ROCK_PAINTED"),
            134 => Some("IMAGE_WOOD_PAINTED"),
            135 => Some("IMAGE_LIMESTONE_BLOCK_PAINTED"),
            136 => Some("IMAGE_BRICK_PAINTED"),
            137 => Some("IMAGE_COBBLESTONE_PAINTED"),
            138 => Some("IMAGE_MARBLE_PAINTED"),
            139 => Some("IMAGE_SANDSTONE_PAINTED"),
            140 => Some("IMAGE_DIRT_PAINTED"),
            141 => Some("IMAGE_WOODEN_PLATFORM_PAINTED"),
            142 => Some("IMAGE_SAND_PAINTED"),
            143 => Some("IMAGE_BASALT_PAINTED"),
            144 => Some("IMAGE_COPPER_PAINTED"),
            145 => Some("IMAGE_TIN_PAINTED"),
            146 => Some("IMAGE_BRONZE_PAINTED"),
            147 => Some("IMAGE_IRON_PAINTED"),
            148 => Some("IMAGE_SIGN_DECORATION_LEFT"),
            149 => Some("IMAGE_SIGN_DECORATION_RIGHT"),
            150 => Some("IMAGE_BLACK_SAND_PAINTED"),
            151 => Some("IMAGE_UNUSED_PAINT_F"),
            152 => Some("IMAGE_IRON_DOOR"),
            153 => Some("IMAGE_IRON_TRAPDOOR"),
            154 => Some("IMAGE_DODO_EGG"),
            155 => Some("IMAGE_DISPLAY_CABINET_FRONT_PAINTED"),
            156 => Some("IMAGE_ELEVATOR_SHAFT_PAINTED"),
            157 => Some("IMAGE_PLATINUM_ORE"),
            158 => Some("IMAGE_PLATINUM_BLOCK_OLD"),
            160 => Some("IMAGE_GRASS_SIDE"),
            161 => Some("IMAGE_GRASS_TOP"),
            162 => Some("IMAGE_GRASS_SIDE_FROZEN"),
            163 => Some("IMAGE_GRASS_TOP_FROZEN"),
            164 => Some("IMAGE_WOODEN_PLATFORM"),
            165 => Some("IMAGE_INITIAL_PORTAL_BASE_STONE_AMETHYST"),
            166 => Some("IMAGE_INITIAL_PORTAL_BASE_STONE_SAPPHIRE"),
            167 => Some("IMAGE_INITIAL_PORTAL_BASE_STONE_EMERALD"),
            168 => Some("IMAGE_INITIAL_PORTAL_BASE_STONE_RUBY"),
            169 => Some("IMAGE_INITIAL_PORTAL_BASE_STONE_DIAMOND"),
            170 => Some("IMAGE_PLACED_PORTAL_BASE_STONE"),
            171 => Some("IMAGE_PLACED_PORTAL_BASE_STONE_AMETHYST"),
            172 => Some("IMAGE_PLACED_PORTAL_BASE_STONE_SAPPHIRE"),
            173 => Some("IMAGE_PLACED_PORTAL_BASE_STONE_EMERALD"),
            174 => Some("IMAGE_PLACED_PORTAL_BASE_STONE_RUBY"),
            175 => Some("IMAGE_PLACED_PORTAL_BASE_STONE_DIAMOND"),
            176 => Some("IMAGE_SIGN"),
            177 => Some("IMAGE_SIGN_RIGHT"),
            178 => Some("IMAGE_SIGN_GROUND_DOUBLE"),
            179 => Some("IMAGE_SIGN_GROUND_DOUBLE_RIGHT"),
            180 => Some("IMAGE_SIGN_HANG"),
            181 => Some("IMAGE_SIGN_HANG_RIGHT"),
            182 => Some("IMAGE_SIGN_GROUND_SINGLE"),
            183 => Some("IMAGE_SIGN_GROUND_SINGLE_RIGHT"),
            184 => Some("IMAGE_SIGN_SINGLE_LINE"),
            185 => Some("IMAGE_SIGN_SINGLE_LINE_DOUBLE_LEFT"),
            186 => Some("IMAGE_SIGN_SINGLE_LINE_DOUBLE_CENTER"),
            187 => Some("IMAGE_SIGN_SINGLE_LINE_DOUBLE_RIGHT"),
            188 => Some("IMAGE_CARBON_FIBER_BLOCK"),
            189 => Some("IMAGE_REFINERY_CHIMNEY"),
            190 => Some("IMAGE_TITANIUM_BLOCK"),
            192 => Some("IMAGE_TRUNK"),
            193 => Some("IMAGE_TRUNK_TOP"),
            194 => Some("IMAGE_TRUNK_DEAD"),
            195 => Some("IMAGE_TRUNK_TOP_DEAD"),
            196 => Some("IMAGE_WOOD_PLANKS"),
            197 => Some("IMAGE_LADDER"),
            198 => Some("IMAGE_BEDDING"),
            199 => Some("IMAGE_BEDDING_FOOT"),
            200 => Some("IMAGE_FLAX_MAT_RED"),
            201 => Some("IMAGE_WOOD_BED"),
            202 => Some("IMAGE_WOOD_BED_FOOT"),
            203 => Some("IMAGE_WOOD_BED_TOP"),
            204 => Some("IMAGE_WOOD_BED_TOP_FOOT"),
            205 => Some("IMAGE_WOOD_BED_HEAD"),
            219 => Some("IMAGE_TITANIUM_ORE"),
            220 => Some("IMAGE_AMETHYST_TRUNK"),
            221 => Some("IMAGE_SAPPHIRE_TRUNK"),
            222 => Some("IMAGE_EMERALD_TRUNK"),
            224 => Some("IMAGE_TRUNK_BUSH"),
            232 => Some("IMAGE_TRUNK_BUSH_MANGO"),
            233 => Some("IMAGE_BUSH_MANGO"),
            234 => Some("IMAGE_CACTUS"),
            235 => Some("IMAGE_CACTUS_DEAD"),
            236 => Some("IMAGE_PINE_TRUNK_BUSH"),
            237 => Some("IMAGE_PINE_BUSH"),
            238 => Some("IMAGE_PINE_BUSH_REAR"),
            239 => Some("IMAGE_FLAX_PLANT"),
            240 => Some("IMAGE_FLAX_PLANT_FLOWER"),
            241 => Some("IMAGE_SUNFLOWER_PLANT"),
            242 => Some("IMAGE_SUNFLOWER_PLANT_FLOWER"),
            243 => Some("IMAGE_CORN_PLANT"),
            244 => Some("IMAGE_CORN_PLANT_FLOWER"),
            245 => Some("IMAGE_CARROT_PLANT"),
            246 => Some("IMAGE_CARROT_FLOWER"),
            247 => Some("IMAGE_CHILLI_PLANT"),
            248 => Some("IMAGE_CHILLI_PLANT_FLOWER"),
            249 => Some("IMAGE_KELP_PLANT"),
            250 => Some("IMAGE_KELP_PLANT_TOP"),
            251 => Some("IMAGE_CRAFT_BENCH_LEVEL_4"),
            252 => Some("IMAGE_CRAFT_BENCH_LEVEL_4_TOP"),
            253 => Some("IMAGE_RUBY_TRUNK"),
            254 => Some("IMAGE_DIAMOND_TRUNK"),
            256 => Some("IMAGE_BUSH"),
            264 => Some("IMAGE_APPLE_TRUNK"),
            265 => Some("IMAGE_CHERRY_BUSH"),
            273 => Some("IMAGE_CHERRY_TRUNK"),
            274 => Some("IMAGE_CHERRY_TRUNK_BUSH"),
            282 => Some("IMAGE_COFFEE_TRUNK"),
            283 => Some("IMAGE_COFFEE_BUSH"),
            284 => Some("IMAGE_COFFEE_TRUNK_BUSH"),
            285 => Some("IMAGE_ELECTRIC_KILN_ELEMENT"),
            286 => Some("IMAGE_ELECTRIC_INIDCATOR_LIGHT"),
            288 => Some("IMAGE_DIRT_FLINT"),
            289 => Some("IMAGE_DIRT_FLINT_GRASS_SIDE"),
            290 => Some("IMAGE_DIRT_CLAY"),
            291 => Some("IMAGE_DIRT_CLAY_GRASS_SIDE"),
            292 => Some("IMAGE_DIRT_FLINT_GRASS_SIDE_FROZEN"),
            293 => Some("IMAGE_DIRT_CLAY_GRASS_SIDE_FROZEN"),
            294 => Some("IMAGE_COMPOST_GRASS_TOP"),
            295 => Some("IMAGE_UNUSED"),
            296 => Some("IMAGE_WORKBENCH_PRESS_2"),
            297 => Some("IMAGE_WORKBENCH_PRESS_2_TOP"),
            298 => Some("IMAGE_SAFE"),
            299 => Some("IMAGE_SAFE_TOP"),
            300 => Some("IMAGE_METALWORK_BENCH"),
            301 => Some("IMAGE_METALWORK_BENCH_TOP"),
            302 => Some("IMAGE_CHEST_GOLD"),
            303 => Some("IMAGE_CHEST_GOLD_TOP"),
            304 => Some("IMAGE_TRADING_POST"),
            305 => Some("IMAGE_TRADING_POST_TOP"),
            306 => Some("IMAGE_GOLD_BED"),
            307 => Some("IMAGE_GOLD_BED_FOOT"),
            308 => Some("IMAGE_GOLD_BED_TOP"),
            309 => Some("IMAGE_GOLD_BED_TOP_FOOT"),
            310 => Some("IMAGE_GOLD_BED_HEAD"),
            311 => Some("IMAGE_CHEST_PORTAL"),
            312 => Some("IMAGE_CHEST_PORTAL_TOP"),
            313 => Some("IMAGE_WORKBENCH_LEVEL_4"),
            314 => Some("IMAGE_WORKBENCH_LEVEL_4_TOP"),
            315 => Some("IMAGE_STEAM_GENERATOR"),
            316 => Some("IMAGE_STEAM_GENERATOR_TOP"),
            317 => Some("IMAGE_ELECTRIC_KILN"),
            318 => Some("IMAGE_ELECTRIC_KILN_TOP"),
            320 => Some("IMAGE_WORKBENCH_LEVEL_1"),
            321 => Some("IMAGE_WORKBENCH_LEVEL_1_TOP"),
            322 => Some("IMAGE_WORKBENCH_TOOL_1"),
            323 => Some("IMAGE_WORKBENCH_TOOL_1_TOP"),
            324 => Some("IMAGE_FURNACE_1"),
            325 => Some("IMAGE_FURNACE_1_TOP"),
            326 => Some("IMAGE_WORKBENCH_LEVEL_2"),
            327 => Some("IMAGE_WORKBENCH_LEVEL_2_TOP"),
            328 => Some("IMAGE_FURNACE_2"),
            329 => Some("IMAGE_FURNACE_2_TOP"),
            330 => Some("IMAGE_WORKBENCH_TOOL_3"),
            331 => Some("IMAGE_WORKBENCH_TOOL_3_TOP"),
            332 => Some("IMAGE_WORKBENCH_TOOL_2"),
            333 => Some("IMAGE_WORKBENCH_TOOL_2_TOP"),
            334 => Some("IMAGE_WORKBENCH_WEAVE_2"),
            335 => Some("IMAGE_WORKBENCH_WEAVE_2_TOP"),
            336 => Some("IMAGE_WORKBENCH_WOOD_1"),
            337 => Some("IMAGE_WORKBENCH_WOOD_1_TOP"),
            338 => Some("IMAGE_WORKBENCH_WEAVE_1"),
            339 => Some("IMAGE_WORKBENCH_WEAVE_1_TOP"),
            340 => Some("IMAGE_WORKBENCH_PRESS_1"),
            341 => Some("IMAGE_WORKBENCH_PRESS_1_TOP"),
            342 => Some("IMAGE_CHEST"),
            343 => Some("IMAGE_CHEST_TOP"),
            344 => Some("IMAGE_KILN"),
            345 => Some("IMAGE_KILN_TOP"),
            346 => Some("IMAGE_FURNACE_3"),
            347 => Some("IMAGE_FURNACE_3_TOP"),
            348 => Some("IMAGE_WORKBENCH_TOOL_4"),
            349 => Some("IMAGE_WORKBENCH_TOOL_4_TOP"),
            350 => Some("IMAGE_FALLEN_LEAVES"),
            352 => Some("IMAGE_CAMPFIRE_0"),
            353 => Some("IMAGE_CAMPFIRE_1"),
            354 => Some("IMAGE_CAMPFIRE_2"),
            355 => Some("IMAGE_CAMPFIRE_3"),
            356 => Some("IMAGE_CAMPFIRE_4"),
            357 => Some("IMAGE_CAMPFIRE_5"),
            358 => Some("IMAGE_CAMPFIRE_6"),
            359 => Some("IMAGE_CAMPFIRE_7"),
            360 => Some("IMAGE_STONE_KILN_0"),
            361 => Some("IMAGE_STONE_KILN_1"),
            362 => Some("IMAGE_STONE_KILN_2"),
            363 => Some("IMAGE_STONE_KILN_3"),
            364 => Some("IMAGE_STONE_KILN_4"),
            365 => Some("IMAGE_STONE_KILN_5"),
            366 => Some("IMAGE_STONE_KILN_6"),
            367 => Some("IMAGE_STONE_KILN_7"),
            368 => Some("IMAGE_WORKBENCH_TOOL_6"),
            369 => Some("IMAGE_WORKBENCH_TOOL_6_TOP"),
            370 => Some("IMAGE_WORKBENCH_TOOL_5"),
            371 => Some("IMAGE_WORKBENCH_TOOL_5_TOP"),
            372 => Some("IMAGE_CRAFT_BENCH_LEVEL_2"),
            373 => Some("IMAGE_CRAFT_BENCH_LEVEL_2_TOP"),
            374 => Some("IMAGE_MIX_BENCH_LEVEL_1"),
            375 => Some("IMAGE_MIX_BENCH_LEVEL_1_TOP"),
            376 => Some("IMAGE_DYE_BENCH_LEVEL_1"),
            377 => Some("IMAGE_DYE_BENCH_LEVEL_1_TOP"),
            378 => Some("IMAGE_CRAFT_BENCH_LEVEL_1"),
            379 => Some("IMAGE_CRAFT_BENCH_LEVEL_1_TOP"),
            380 => Some("IMAGE_CRAFT_BENCH_LEVEL_3"),
            381 => Some("IMAGE_CRAFT_BENCH_LEVEL_3_TOP"),
            382 => Some("IMAGE_VINE_PLANT_TOP"),
            384 => Some("IMAGE_CAMPFIRE_OUT_0"),
            385 => Some("IMAGE_CAMPFIRE_OUT_1"),
            386 => Some("IMAGE_CAMPFIRE_OUT_2"),
            387 => Some("IMAGE_CAMPFIRE_OUT_3"),
            388 => Some("IMAGE_CAMPFIRE_OUT_4"),
            389 => Some("IMAGE_CAMPFIRE_OUT_5"),
            390 => Some("IMAGE_CAMPFIRE_OUT_6"),
            391 => Some("IMAGE_CAMPFIRE_OUT_7"),
            392 => Some("IMAGE_ICE_TORCH_0"),
            416 => Some("IMAGE_BASIC_TORCH_0"),
            417 => Some("IMAGE_BASIC_TORCH_1"),
            418 => Some("IMAGE_BASIC_TORCH_2"),
            419 => Some("IMAGE_BASIC_TORCH_3"),
            420 => Some("IMAGE_BASIC_TORCH_4"),
            421 => Some("IMAGE_BASIC_TORCH_5"),
            422 => Some("IMAGE_BASIC_TORCH_6"),
            423 => Some("IMAGE_BASIC_TORCH_7"),
            424 => Some("IMAGE_CLAY_LANTERN_0"),
            425 => Some("IMAGE_CLAY_LANTERN_1"),
            426 => Some("IMAGE_CLAY_LANTERN_2"),
            427 => Some("IMAGE_CLAY_LANTERN_3"),
            428 => Some("IMAGE_CLAY_LANTERN_4"),
            429 => Some("IMAGE_CLAY_LANTERN_5"),
            430 => Some("IMAGE_CLAY_LANTERN_6"),
            431 => Some("IMAGE_CLAY_LANTERN_7"),
            432 => Some("IMAGE_PORTAL_DIAMOND_0"),
            440 => Some("IMAGE_EMERALD_CHANDELIER"),
            442 => Some("IMAGE_RUBY_CHANDELIER"),
            444 => Some("IMAGE_DIAMOND_CHANDELIER"),
            446 => Some("IMAGE_VINE_PLANT"),
            448 => Some("IMAGE_TRUNK_BUSH_DEAD"),
            449 => Some("IMAGE_BUSH_DEAD"),
            450 => Some("IMAGE_BUSH_DEAD_REAR"),
            451 => Some("IMAGE_CAMPFIRE_UPGRADE"),
            452 => Some("IMAGE_CAMPFIRE_UPGRADED_COMBINED"),
            456 => Some("IMAGE_STEEL_LANTERN_0"),
            474 => Some("IMAGE_ELECTRIC_FURNACE"),
            475 => Some("IMAGE_ELECTRIC_FURNACE_TOP"),
            476 => Some("IMAGE_ELECTRIC_METALWORK_BENCH"),
            477 => Some("IMAGE_ELECTRIC_METALWORK_BENCH_TOP"),
            478 => Some("IMAGE_GRAVEL"),
            480 => Some("IMAGE_MAPLE_TRUNK_BUSH"),
            488 => Some("IMAGE_MAPLE_TRUNK"),
            489 => Some("IMAGE_COCONUT_BUSH"),
            490 => Some("IMAGE_COCONUT_TRUNK"),
            491 => Some("IMAGE_COCONUT_BUSH_DEAD"),
            492 => Some("IMAGE_COCONUT_TRUNK_DEAD"),
            493 => Some("IMAGE_ORANGE_BUSH"),
            494 => Some("IMAGE_ORANGE_TRUNK_BUSH"),
            495 => Some("IMAGE_ORANGE_TRUNK"),
            496 => Some("IMAGE_PORTAL_RUBY_0"),
            504 => Some("IMAGE_SHARK_JAW"),
            505 => Some("IMAGE_SHARK_JAW_RIGHT"),
            506 => Some("IMAGE_ARMOR_BENCH"),
            507 => Some("IMAGE_ARMOR_BENCH_TOP"),
            508 => Some("IMAGE_ARMOR_BENCH_LEVEL_2"),
            509 => Some("IMAGE_ARMOR_BENCH_LEVEL_2_TOP"),
            510 => Some("IMAGE_GRAVEL_PAINTED"),
            512 => Some("IMAGE_MAPLE_BUSH"),
            520 => Some("IMAGE_LIME_BUSH"),
            521 => Some("IMAGE_LIME_TRUNK_BUSH"),
            522 => Some("IMAGE_LIME_TRUNK"),
            527 => Some("IMAGE_STEEL_DOWNLIGHT"),
            536 => Some("IMAGE_TRAIN_YARD"),
            537 => Some("IMAGE_TRAIN_YARD_TOP"),
            538 => Some("IMAGE_WORKBENCH_LEVEL_3"),
            539 => Some("IMAGE_WORKBENCH_LEVEL_3_TOP"),
            540 => Some("IMAGE_BUILDERS_BENCH_LEVEL_1"),
            541 => Some("IMAGE_BUILDERS_BENCH_LEVEL_1_TOP"),
            542 => Some("IMAGE_DODO_EGG_BREEDS"),
            544 => Some("IMAGE_ELECTRIC_STOVE"),
            545 => Some("IMAGE_ELECTRIC_STOVE_TOP"),
            546 => Some("IMAGE_ELECTRIC_STOVE_TOP_GLOW"),
            547 => Some("IMAGE_SOLAR_PANEL"),
            548 => Some("IMAGE_SOLAR_PANEL_TOP"),
            549 => Some("IMAGE_FLYWHEEL"),
            550 => Some("IMAGE_FLYWHEEL_TOP"),
            551 => Some("IMAGE_FLYWHEEL_WHEEL"),
            552 => Some("IMAGE_NORTH_POLE"),
            553 => Some("IMAGE_SOUTH_POLE"),
            554 => Some("IMAGE_EQUATOR_POLE"),
            555 => Some("IMAGE_AMETHYST_CHANDELIER"),
            556 => Some("IMAGE_AMETHYST_CHANDELIER_RIGHT"),
            557 => Some("IMAGE_SAPPHIRE_CHANDELIER"),
            558 => Some("IMAGE_SAPPHIRE_CHANDELIER_RIGHT"),
            559 => Some("IMAGE_STEEL_UPLIGHT"),
            560 => Some("IMAGE_PORTAL_EMERALD_0"),
            568 => Some("IMAGE_ARMOR_BENCH_LEVEL_3"),
            569 => Some("IMAGE_ARMOR_BENCH_LEVEL_3_TOP"),
            570 => Some("IMAGE_AMETHYST_BUSH"),
            571 => Some("IMAGE_SAPPHIRE_BUSH"),
            572 => Some("IMAGE_EMERALD_BUSH"),
            573 => Some("IMAGE_RUBY_BUSH"),
            574 => Some("IMAGE_DIAMOND_BUSH"),
            576 => Some("IMAGE_SOLAR_INDICATOR_LIGHTS"),
            577 => Some("IMAGE_METALWORK_BENCH_LEVEL_2"),
            578 => Some("IMAGE_METALWORK_BENCH_LEVEL_2_TOP"),
            579 => Some("IMAGE_STEAM_LOCO_LEFT"),
            580 => Some("IMAGE_STEAM_LOCO_RIGHT"),
            581 => Some("IMAGE_STEAM_LOCO_BACK_LEFT"),
            582 => Some("IMAGE_STEAM_LOCO_BACK_RIGHT"),
            583 => Some("IMAGE_ELEVATOR_SHAFT"),
            584 => Some("IMAGE_ELEVATOR_MOTOR"),
            585 => Some("IMAGE_CHEST_PAINTED"),
            586 => Some("IMAGE_CHEST_TOP_PAINTED"),
            587 => Some("IMAGE_WOOD_BED_PAINTED"),
            588 => Some("IMAGE_WOOD_BED_FOOT_PAINTED"),
            589 => Some("IMAGE_WOOD_BED_TOP_PAINTED"),
            590 => Some("IMAGE_WOOD_BED_TOP_FOOT_PAINTED"),
            591 => Some("IMAGE_WOOD_BED_HEAD_PAINTED"),
            600 => Some("IMAGE_TULIP_PLANT_SPROUT"),
            601 => Some("IMAGE_TULIP_PLANT_MATURE"),
            602 => Some("IMAGE_AMETHYST_TRUNK_BUSH"),
            603 => Some("IMAGE_SAPPHIRE_TRUNK_BUSH"),
            604 => Some("IMAGE_EMERALD_TRUNK_BUSH"),
            605 => Some("IMAGE_RUBY_TRUNK_BUSH"),
            606 => Some("IMAGE_DIAMOND_TRUNK_BUSH"),
            608 => Some("IMAGE_SIGN_PAINTED"),
            609 => Some("IMAGE_SIGN_RIGHT_PAINTED"),
            610 => Some("IMAGE_SIGN_GROUND_DOUBLE_PAINTED"),
            611 => Some("IMAGE_SIGN_GROUND_DOUBLE_RIGHT_PAINTED"),
            612 => Some("IMAGE_SIGN_HANG_PAINTED"),
            613 => Some("IMAGE_SIGN_HANG_RIGHT_PAINTED"),
            614 => Some("IMAGE_SIGN_GROUND_SINGLE_PAINTED"),
            615 => Some("IMAGE_SIGN_GROUND_SINGLE_RIGHT_PAINTED"),
            616 => Some("IMAGE_SAFE_PAINTED"),
            617 => Some("IMAGE_SAFE_TOP_PAINTED"),
            618 => Some("IMAGE_LADDER_PAINTED"),
            619 => Some("IMAGE_GOLD_BED_PAINTED"),
            620 => Some("IMAGE_GOLD_BED_FOOT_PAINTED"),
            621 => Some("IMAGE_GOLD_BED_TOP_PAINTED"),
            622 => Some("IMAGE_GOLD_BED_TOP_FOOT_PAINTED"),
            623 => Some("IMAGE_GOLD_BED_HEAD_PAINTED"),
            624 => Some("IMAGE_PORTAL_SAPPHIRE_0"),
            632 => Some("IMAGE_BUILDERS_BENCH_LEVEL_2"),
            633 => Some("IMAGE_BUILDERS_BENCH_LEVEL_2_TOP"),
            634 => Some("IMAGE_WORKBENCH_TOOL_7"),
            635 => Some("IMAGE_WORKBENCH_TOOL_7_TOP"),
            636 => Some("IMAGE_ELECTRIC_PRESS"),
            637 => Some("IMAGE_ELECTRIC_PRESS_TOP"),
            638 => Some("IMAGE_DODO_EGG_BREEDS_ITEM"),
            640 => Some("IMAGE_SIGN_SINGLE_LINE_PAINTED"),
            641 => Some("IMAGE_SIGN_SINGLE_LINE_DOUBLE_LEFT_PAINTED"),
            642 => Some("IMAGE_SIGN_SINGLE_LINE_DOUBLE_CENTER_PAINTED"),
            643 => Some("IMAGE_SIGN_SINGLE_LINE_DOUBLE_RIGHT_PAINTED"),
            644 => Some("IMAGE_TRADING_POST_PAINTED"),
            645 => Some("IMAGE_CARBON_FIBER_BLOCK_PAINTED"),
            646 => Some("IMAGE_TITANIUM_BLOCK_PAINTED"),
            647 => Some("IMAGE_GEM_TRUNK_PAINTED"),
            648 => Some("IMAGE_OWNERSHIP_SIGN"),
            649 => Some("IMAGE_OWNERSHIP_SIGN_RIGHT"),
            650 => Some("IMAGE_OWNERSHIP_SIGN_GROUND_DOUBLE"),
            651 => Some("IMAGE_OWNERSHIP_SIGN_GROUND_DOUBLE_RIGHT"),
            652 => Some("IMAGE_OWNERSHIP_SIGN_HANG"),
            653 => Some("IMAGE_OWNERSHIP_SIGN_HANG_RIGHT"),
            654 => Some("IMAGE_OWNERSHIP_SIGN_GROUND_SINGLE"),
            655 => Some("IMAGE_OWNERSHIP_SIGN_GROUND_SINGLE_RIGHT"),
            664 => Some("IMAGE_COMPOST_BIN"),
            665 => Some("IMAGE_COMPOST_BIN_TOP"),
            666 => Some("IMAGE_FLINT_BLOCK"),
            667 => Some("IMAGE_CLAY_BLOCK"),
            668 => Some("IMAGE_OIL_BLOCK"),
            669 => Some("IMAGE_FUEL_BLOCK"),
            670 => Some("IMAGE_COAL_BLOCK"),
            672 => Some("IMAGE_PLATINUM_0"),
            688 => Some("IMAGE_PORTAL_AMETHYST_0"),
            696 => Some("IMAGE_GATE"),
            697 => Some("IMAGE_AMETHYST_BLOCK"),
            698 => Some("IMAGE_SAPPHIRE_BLOCK"),
            699 => Some("IMAGE_EMERALD_BLOCK"),
            700 => Some("IMAGE_RUBY_BLOCK"),
            701 => Some("IMAGE_DIAMOND_BLOCK"),
            702 => Some("IMAGE_RAINBOW_BLOCK"),
            704 => Some("IMAGE_PLATINUM_PAINTED_0"),
            733 => Some("IMAGE_GEM_BLOCK_PAINTED"),
            736 => Some("IMAGE_EGG_EXTRACTOR"),
            737 => Some("IMAGE_EGG_EXTRACTOR_TOP"),
            738 => Some("IMAGE_EGG_EXTRACTOR_WHEEL"),
            739 => Some("IMAGE_PIZZA_OVEN"),
            740 => Some("IMAGE_PIZZA_OVEN_TOP"),
            741 => Some("IMAGE_PLASTER"),
            742 => Some("IMAGE_CHEST_FEEDER"),
            743 => Some("IMAGE_CHEST_FEEDER_TOP"),
            744 => Some("IMAGE_CHEST_FEEDER_PAINTED"),
            745 => Some("IMAGE_CHEST_FEEDER_TOP_PAINTED"),
            746 => Some("IMAGE_LUMINOUS_PLASTER"),
            747 => Some("IMAGE_DODO_EGG_ITEM"),
            752 => Some("IMAGE_TRADE_PORTAL_0"),
            760 => Some("IMAGE_WHEAT_PLANT"),
            761 => Some("IMAGE_WHEAT_PLANT_FLOWER"),
            762 => Some("IMAGE_TOMATO_PLANT"),
            763 => Some("IMAGE_TOMATO_PLANT_FLOWER"),
            764 => Some("IMAGE_MIRROR"),
            768 => Some("IMAGE_OWNERSHIP_SIGN_PAINTED"),
            769 => Some("IMAGE_OWNERSHIP_SIGN_RIGHT_PAINTED"),
            770 => Some("IMAGE_OWNERSHIP_SIGN_GROUND_DOUBLE_PAINTED"),
            771 => Some("IMAGE_OWNERSHIP_SIGN_GROUND_DOUBLE_RIGHT_PAINTED"),
            772 => Some("IMAGE_OWNERSHIP_SIGN_HANG_PAINTED"),
            773 => Some("IMAGE_OWNERSHIP_SIGN_HANG_RIGHT_PAINTED"),
            774 => Some("IMAGE_OWNERSHIP_SIGN_GROUND_SINGLE_PAINTED"),
            775 => Some("IMAGE_OWNERSHIP_SIGN_GROUND_SINGLE_RIGHT_PAINTED"),
            776 => Some("IMAGE_KILN_LEVEL_2"),
            777 => Some("IMAGE_KILN_LEVEL_2_TOP"),
            778 => Some("IMAGE_CRAFT_BENCH_EXPERT_LEVEL_3"),
            779 => Some("IMAGE_CRAFT_BENCH_EXPERT_LEVEL_3_TOP"),
            780 => Some("IMAGE_CRAFT_BENCH_EXPERT_LEVEL_4"),
            781 => Some("IMAGE_CRAFT_BENCH_EXPERT_LEVEL_4_TOP"),
            782 => Some("IMAGE_CRAFT_BENCH_EXPERT_LEVEL_5"),
            783 => Some("IMAGE_CRAFT_BENCH_EXPERT_LEVEL_5_TOP"),
            784 => Some("IMAGE_CRAFT_BENCH_EXPERT_LEVEL_6"),
            785 => Some("IMAGE_CRAFT_BENCH_EXPERT_LEVEL_6_TOP"),
            786 => Some("IMAGE_CRAFT_BENCH_EXPERT_LEVEL_7"),
            787 => Some("IMAGE_CRAFT_BENCH_EXPERT_LEVEL_7_TOP"),
            788 => Some("IMAGE_WORKBENCH_PRESS_3"),
            789 => Some("IMAGE_WORKBENCH_PRESS_4"),
            790 => Some("IMAGE_WORKBENCH_PRESS_5"),
            791 => Some("IMAGE_WORKBENCH_PRESS_6"),
            792 => Some("IMAGE_WORKBENCH_PRESS_7"),
            793 => Some("IMAGE_METALWORK_BENCH_EXPERT_2"),
            794 => Some("IMAGE_METALWORK_BENCH_EXPERT_3"),
            795 => Some("IMAGE_METALWORK_BENCH_EXPERT_4"),
            796 => Some("IMAGE_METALWORK_BENCH_EXPERT_5"),
            797 => Some("IMAGE_METALWORK_BENCH_EXPERT_6"),
            800 => Some("IMAGE_WORKBENCH_EXPERT_1"),
            801 => Some("IMAGE_WORKBENCH_EXPERT_2"),
            802 => Some("IMAGE_WORKBENCH_EXPERT_3"),
            803 => Some("IMAGE_WORKBENCH_EXPERT_4"),
            804 => Some("IMAGE_WORKBENCH_EXPERT_5"),
            805 => Some("IMAGE_WORKBENCH_EXPERT_6"),
            806 => Some("IMAGE_WORKBENCH_EXPERT_7"),
            807 => Some("IMAGE_WORKBENCH_EXPERT_8"),
            808 => Some("IMAGE_WORKBENCH_EXPERT_9"),
            809 => Some("IMAGE_WORKBENCH_EXPERT_11"),
            810 => Some("IMAGE_WORKBENCH_EXPERT_12"),
            811 => Some("IMAGE_WORKBENCH_EXPERT_13"),
            812 => Some("IMAGE_WORKBENCH_EXPERT_14"),
            813 => Some("IMAGE_WORKBENCH_EXPERT_15"),
            814 => Some("IMAGE_WORKBENCH_EXPERT_16"),
            815 => Some("IMAGE_WORKBENCH_EXPERT_17"),
            816 => Some("IMAGE_WORKBENCH_EXPERT_18"),
            817 => Some("IMAGE_WORKBENCH_EXPERT_20"),
            818 => Some("IMAGE_WORKBENCH_EXPERT_21"),
            819 => Some("IMAGE_WORKBENCH_EXPERT_22"),
            820 => Some("IMAGE_WORKBENCH_EXPERT_23"),
            821 => Some("IMAGE_WORKBENCH_EXPERT_24"),
            822 => Some("IMAGE_WORKBENCH_EXPERT_25"),
            823 => Some("IMAGE_WORKBENCH_EXPERT_26"),
            824 => Some("IMAGE_WORKBENCH_EXPERT_27"),
            _ => None,
        }
    }
}
impl core::fmt::Debug for ImageType {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self.name() {
            Some(n) => write!(f, "{}", n),
            None => write!(f, "ImageType({})", self.0),
        }
    }
}
impl From<u32> for ImageType {
    fn from(v: u32) -> Self {
        Self(v)
    }
}
impl From<ImageType> for u32 {
    fn from(v: ImageType) -> u32 {
        v.0
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct TileTraverseType(pub u32);
impl TileTraverseType {
    pub const TRAVERSE_NONE: TileTraverseType = TileTraverseType(0);
    pub const TRAVERSE_WALK: TileTraverseType = TileTraverseType(1);
    pub const TRAVERSE_SWIM: TileTraverseType = TileTraverseType(2);
    pub const TRAVERSE_CRAWL: TileTraverseType = TileTraverseType(3);
    pub const TRAVERSE_CLIMB: TileTraverseType = TileTraverseType(4);
    pub const TRAVERSE_STAND_TO_HANG_JUMP: TileTraverseType = TileTraverseType(5);
    pub const TRAVERSE_HANG_TO_STAND_JUMP: TileTraverseType = TileTraverseType(6);
    pub const TRAVERSE_HANG_TO_HANG_JUMP: TileTraverseType = TileTraverseType(7);
    pub const TRAVERSE_STAND_TO_HANG_DOUBLE_JUMP: TileTraverseType = TileTraverseType(8);
    pub const TRAVERSE_STAND_TO_HANG_DIAGONAL_DOUBLE_JUMP: TileTraverseType = TileTraverseType(9);
    pub const TRAVERSE_STAND_TO_HANG_DIAGONAL_DOUBLE_JUMP_REVERSE: TileTraverseType = TileTraverseType(10);
    pub const TRAVERSE_STAND_TO_MONKEY_HANG_JUMP: TileTraverseType = TileTraverseType(11);
    pub const TRAVERSE_MONKEY_HANG_TO_STAND_JUMP: TileTraverseType = TileTraverseType(12);
    pub const TRAVERSE_CROUCH_UP: TileTraverseType = TileTraverseType(13);
    pub const TRAVERSE_CROUCH_DOWN: TileTraverseType = TileTraverseType(14);
    pub const TRAVERSE_CROUCH_CRAWL_UP: TileTraverseType = TileTraverseType(15);
    pub const TRAVERSE_CROUCH_CRAWL_DOWN: TileTraverseType = TileTraverseType(16);
    pub const TRAVERSE_FALL: TileTraverseType = TileTraverseType(17);
    pub const TRAVERSE_JUMP: TileTraverseType = TileTraverseType(18);
    pub const TRAVERSE_MONKEY_SWING: TileTraverseType = TileTraverseType(19);
    pub const TRAVERSE_CLIMB_DOWN_TO_MONKEY_SWING: TileTraverseType = TileTraverseType(20);
    pub const TRAVERSE_MONKEY_SWING_UP_TO_CLIMB: TileTraverseType = TileTraverseType(21);
    pub const TRAVERSE_HANG_TO_MONKEY_SWING_DIAGONAL_JUMP_UP: TileTraverseType = TileTraverseType(22);
    pub const TRAVERSE_MONKEY_SWING_TO_HANG_DIAGONAL_JUMP_DOWN: TileTraverseType = TileTraverseType(23);
    pub const TRAVERSE_JUMP_OVER_GAP: TileTraverseType = TileTraverseType(24);
    pub const TRAVERSE_INACCESSIBLE: TileTraverseType = TileTraverseType(25);
    pub const TRAVERSE_STAIRS: TileTraverseType = TileTraverseType(26);
    pub const TRAVERSE_ELEVATOR: TileTraverseType = TileTraverseType(27);
    pub const TRAVERSE_FLY: TileTraverseType = TileTraverseType(28);
    #[inline]
    pub const fn raw(self) -> u32 {
        self.0
    }
    #[inline]
    pub const fn from_raw(v: u32) -> Self {
        Self(v)
    }
    /// First-declared constant name for this value (None for unknown/unnamed values).
    pub fn name(self) -> Option<&'static str> {
        match self.0 {
            0 => Some("TRAVERSE_NONE"),
            1 => Some("TRAVERSE_WALK"),
            2 => Some("TRAVERSE_SWIM"),
            3 => Some("TRAVERSE_CRAWL"),
            4 => Some("TRAVERSE_CLIMB"),
            5 => Some("TRAVERSE_STAND_TO_HANG_JUMP"),
            6 => Some("TRAVERSE_HANG_TO_STAND_JUMP"),
            7 => Some("TRAVERSE_HANG_TO_HANG_JUMP"),
            8 => Some("TRAVERSE_STAND_TO_HANG_DOUBLE_JUMP"),
            9 => Some("TRAVERSE_STAND_TO_HANG_DIAGONAL_DOUBLE_JUMP"),
            10 => Some("TRAVERSE_STAND_TO_HANG_DIAGONAL_DOUBLE_JUMP_REVERSE"),
            11 => Some("TRAVERSE_STAND_TO_MONKEY_HANG_JUMP"),
            12 => Some("TRAVERSE_MONKEY_HANG_TO_STAND_JUMP"),
            13 => Some("TRAVERSE_CROUCH_UP"),
            14 => Some("TRAVERSE_CROUCH_DOWN"),
            15 => Some("TRAVERSE_CROUCH_CRAWL_UP"),
            16 => Some("TRAVERSE_CROUCH_CRAWL_DOWN"),
            17 => Some("TRAVERSE_FALL"),
            18 => Some("TRAVERSE_JUMP"),
            19 => Some("TRAVERSE_MONKEY_SWING"),
            20 => Some("TRAVERSE_CLIMB_DOWN_TO_MONKEY_SWING"),
            21 => Some("TRAVERSE_MONKEY_SWING_UP_TO_CLIMB"),
            22 => Some("TRAVERSE_HANG_TO_MONKEY_SWING_DIAGONAL_JUMP_UP"),
            23 => Some("TRAVERSE_MONKEY_SWING_TO_HANG_DIAGONAL_JUMP_DOWN"),
            24 => Some("TRAVERSE_JUMP_OVER_GAP"),
            25 => Some("TRAVERSE_INACCESSIBLE"),
            26 => Some("TRAVERSE_STAIRS"),
            27 => Some("TRAVERSE_ELEVATOR"),
            28 => Some("TRAVERSE_FLY"),
            _ => None,
        }
    }
}
impl core::fmt::Debug for TileTraverseType {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self.name() {
            Some(n) => write!(f, "{}", n),
            None => write!(f, "TileTraverseType({})", self.0),
        }
    }
}
impl From<u32> for TileTraverseType {
    fn from(v: u32) -> Self {
        Self(v)
    }
}
impl From<TileTraverseType> for u32 {
    fn from(v: TileTraverseType) -> u32 {
        v.0
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct TileTraverseKeyFrameType(pub u32);
impl TileTraverseKeyFrameType {
    pub const TRAVERSE_KF_STAND: TileTraverseKeyFrameType = TileTraverseKeyFrameType(0);
    pub const TRAVERSE_KF_BOB: TileTraverseKeyFrameType = TileTraverseKeyFrameType(1);
    pub const TRAVERSE_KF_CRAWL: TileTraverseKeyFrameType = TileTraverseKeyFrameType(2);
    pub const TRAVERSE_KF_HANG_BACK: TileTraverseKeyFrameType = TileTraverseKeyFrameType(3);
    pub const TRAVERSE_KF_HANG_RIGHT: TileTraverseKeyFrameType = TileTraverseKeyFrameType(4);
    pub const TRAVERSE_KF_HANG_LEFT: TileTraverseKeyFrameType = TileTraverseKeyFrameType(5);
    pub const TRAVERSE_KF_MONKEY_HANG: TileTraverseKeyFrameType = TileTraverseKeyFrameType(6);
    pub const TRAVERSE_KF_FALLING: TileTraverseKeyFrameType = TileTraverseKeyFrameType(7);
    pub const TRAVERSE_KF_INACCESSIBLE: TileTraverseKeyFrameType = TileTraverseKeyFrameType(8);
    pub const TRAVERSE_KF_STAIRS: TileTraverseKeyFrameType = TileTraverseKeyFrameType(9);
    pub const TRAVERSE_KF_STAND_ELEVATOR: TileTraverseKeyFrameType = TileTraverseKeyFrameType(10);
    pub const TRAVERSE_KF_FLY: TileTraverseKeyFrameType = TileTraverseKeyFrameType(11);
    #[inline]
    pub const fn raw(self) -> u32 {
        self.0
    }
    #[inline]
    pub const fn from_raw(v: u32) -> Self {
        Self(v)
    }
    /// First-declared constant name for this value (None for unknown/unnamed values).
    pub fn name(self) -> Option<&'static str> {
        match self.0 {
            0 => Some("TRAVERSE_KF_STAND"),
            1 => Some("TRAVERSE_KF_BOB"),
            2 => Some("TRAVERSE_KF_CRAWL"),
            3 => Some("TRAVERSE_KF_HANG_BACK"),
            4 => Some("TRAVERSE_KF_HANG_RIGHT"),
            5 => Some("TRAVERSE_KF_HANG_LEFT"),
            6 => Some("TRAVERSE_KF_MONKEY_HANG"),
            7 => Some("TRAVERSE_KF_FALLING"),
            8 => Some("TRAVERSE_KF_INACCESSIBLE"),
            9 => Some("TRAVERSE_KF_STAIRS"),
            10 => Some("TRAVERSE_KF_STAND_ELEVATOR"),
            11 => Some("TRAVERSE_KF_FLY"),
            _ => None,
        }
    }
}
impl core::fmt::Debug for TileTraverseKeyFrameType {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self.name() {
            Some(n) => write!(f, "{}", n),
            None => write!(f, "TileTraverseKeyFrameType({})", self.0),
        }
    }
}
impl From<u32> for TileTraverseKeyFrameType {
    fn from(v: u32) -> Self {
        Self(v)
    }
}
impl From<TileTraverseKeyFrameType> for u32 {
    fn from(v: TileTraverseKeyFrameType) -> u32 {
        v.0
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct SimulationEvent(pub u32);
impl SimulationEvent {
    pub const SE_SLEEP: SimulationEvent = SimulationEvent(0);
    pub const SE_CRAFT: SimulationEvent = SimulationEvent(1);
    pub const SE_GOT: SimulationEvent = SimulationEvent(2);
    pub const SE_PLACE: SimulationEvent = SimulationEvent(3);
    pub const SE_DAMAGE: SimulationEvent = SimulationEvent(4);
    pub const SE_COLLAPSE: SimulationEvent = SimulationEvent(5);
    #[inline]
    pub const fn raw(self) -> u32 {
        self.0
    }
    #[inline]
    pub const fn from_raw(v: u32) -> Self {
        Self(v)
    }
    /// First-declared constant name for this value (None for unknown/unnamed values).
    pub fn name(self) -> Option<&'static str> {
        match self.0 {
            0 => Some("SE_SLEEP"),
            1 => Some("SE_CRAFT"),
            2 => Some("SE_GOT"),
            3 => Some("SE_PLACE"),
            4 => Some("SE_DAMAGE"),
            5 => Some("SE_COLLAPSE"),
            _ => None,
        }
    }
}
impl core::fmt::Debug for SimulationEvent {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self.name() {
            Some(n) => write!(f, "{}", n),
            None => write!(f, "SimulationEvent({})", self.0),
        }
    }
}
impl From<u32> for SimulationEvent {
    fn from(v: u32) -> Self {
        Self(v)
    }
}
impl From<SimulationEvent> for u32 {
    fn from(v: SimulationEvent) -> u32 {
        v.0
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct HideUIType(pub u32);
impl HideUIType {
    pub const HIDE_UI_NONE: HideUIType = HideUIType(0);
    pub const HIDE_UI_CAMERA_UI: HideUIType = HideUIType(1);
    pub const HIDE_UI_CAMERA_SCREENSHOT: HideUIType = HideUIType(2);
    pub const HIDE_UI_PORTAL_SCREENSHOT: HideUIType = HideUIType(3);
    pub const HIDE_UI_TEASER_FRAMES: HideUIType = HideUIType(4);
    #[inline]
    pub const fn raw(self) -> u32 {
        self.0
    }
    #[inline]
    pub const fn from_raw(v: u32) -> Self {
        Self(v)
    }
    /// First-declared constant name for this value (None for unknown/unnamed values).
    pub fn name(self) -> Option<&'static str> {
        match self.0 {
            0 => Some("HIDE_UI_NONE"),
            1 => Some("HIDE_UI_CAMERA_UI"),
            2 => Some("HIDE_UI_CAMERA_SCREENSHOT"),
            3 => Some("HIDE_UI_PORTAL_SCREENSHOT"),
            4 => Some("HIDE_UI_TEASER_FRAMES"),
            _ => None,
        }
    }
}
impl core::fmt::Debug for HideUIType {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self.name() {
            Some(n) => write!(f, "{}", n),
            None => write!(f, "HideUIType({})", self.0),
        }
    }
}
impl From<u32> for HideUIType {
    fn from(v: u32) -> Self {
        Self(v)
    }
}
impl From<HideUIType> for u32 {
    fn from(v: HideUIType) -> u32 {
        v.0
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct InteractionType(pub u32);
impl InteractionType {
    pub const INTERACTION_NONE: InteractionType = InteractionType(0);
    pub const INTERACTION_DIG: InteractionType = InteractionType(1);
    pub const INTERACTION_PLACE_ITEM: InteractionType = InteractionType(2);
    pub const INTERACTION_PLACE_SOLID_BLOCK: InteractionType = InteractionType(3);
    pub const INTERACTION_CRAFT: InteractionType = InteractionType(4);
    pub const INTERACTION_USE_INTERACTIVE_OBJECT: InteractionType = InteractionType(5);
    pub const INTERACTION_SOW: InteractionType = InteractionType(6);
    pub const INTERACTION_PICKUP: InteractionType = InteractionType(7);
    pub const INTERACTION_PICKUP_FREEBLOCK: InteractionType = InteractionType(8);
    pub const INTERACTION_BURN: InteractionType = InteractionType(9);
    pub const INTERACTION_STAB: InteractionType = InteractionType(10);
    pub const INTERACTION_UNUSED: InteractionType = InteractionType(11);
    pub const INTERACTION_RIDE: InteractionType = InteractionType(12);
    pub const INTERACTION_ADD_FUEL: InteractionType = InteractionType(13);
    pub const INTERACTION_FILL_BUCKET: InteractionType = InteractionType(14);
    pub const INTERACTION_PAINT: InteractionType = InteractionType(15);
    pub const INTERACTION_STRIP_PAINT: InteractionType = InteractionType(16);
    pub const INTERACTION_RIDE_BOAT: InteractionType = InteractionType(17);
    pub const INTERACTION_REMOVE_BOAT: InteractionType = InteractionType(18);
    pub const INTERACTION_CAPTURE: InteractionType = InteractionType(19);
    pub const INTERACTION_FIRE_RANGED_WEAPON: InteractionType = InteractionType(20);
    pub const INTERACTION_RIDE_TRAIN_CAR: InteractionType = InteractionType(21);
    pub const INTERACTION_REMOVE_TRAIN_CAR: InteractionType = InteractionType(22);
    pub const INTERACTION_DIG_FOREGROUND: InteractionType = InteractionType(23);
    pub const INTERACTION_DIG_BACK_WALL: InteractionType = InteractionType(24);
    pub const INTERACTION_FEED_NPC: InteractionType = InteractionType(25);
    pub const INTERACTION_INSPECT_NPC: InteractionType = InteractionType(26);
    pub const INTERACTION_HARVEST_NPC: InteractionType = InteractionType(27);
    pub const INTERACTION_SET_NPC_FREE: InteractionType = InteractionType(28);
    #[inline]
    pub const fn raw(self) -> u32 {
        self.0
    }
    #[inline]
    pub const fn from_raw(v: u32) -> Self {
        Self(v)
    }
    /// First-declared constant name for this value (None for unknown/unnamed values).
    pub fn name(self) -> Option<&'static str> {
        match self.0 {
            0 => Some("INTERACTION_NONE"),
            1 => Some("INTERACTION_DIG"),
            2 => Some("INTERACTION_PLACE_ITEM"),
            3 => Some("INTERACTION_PLACE_SOLID_BLOCK"),
            4 => Some("INTERACTION_CRAFT"),
            5 => Some("INTERACTION_USE_INTERACTIVE_OBJECT"),
            6 => Some("INTERACTION_SOW"),
            7 => Some("INTERACTION_PICKUP"),
            8 => Some("INTERACTION_PICKUP_FREEBLOCK"),
            9 => Some("INTERACTION_BURN"),
            10 => Some("INTERACTION_STAB"),
            11 => Some("INTERACTION_UNUSED"),
            12 => Some("INTERACTION_RIDE"),
            13 => Some("INTERACTION_ADD_FUEL"),
            14 => Some("INTERACTION_FILL_BUCKET"),
            15 => Some("INTERACTION_PAINT"),
            16 => Some("INTERACTION_STRIP_PAINT"),
            17 => Some("INTERACTION_RIDE_BOAT"),
            18 => Some("INTERACTION_REMOVE_BOAT"),
            19 => Some("INTERACTION_CAPTURE"),
            20 => Some("INTERACTION_FIRE_RANGED_WEAPON"),
            21 => Some("INTERACTION_RIDE_TRAIN_CAR"),
            22 => Some("INTERACTION_REMOVE_TRAIN_CAR"),
            23 => Some("INTERACTION_DIG_FOREGROUND"),
            24 => Some("INTERACTION_DIG_BACK_WALL"),
            25 => Some("INTERACTION_FEED_NPC"),
            26 => Some("INTERACTION_INSPECT_NPC"),
            27 => Some("INTERACTION_HARVEST_NPC"),
            28 => Some("INTERACTION_SET_NPC_FREE"),
            _ => None,
        }
    }
}
impl core::fmt::Debug for InteractionType {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self.name() {
            Some(n) => write!(f, "{}", n),
            None => write!(f, "InteractionType({})", self.0),
        }
    }
}
impl From<u32> for InteractionType {
    fn from(v: u32) -> Self {
        Self(v)
    }
}
impl From<InteractionType> for u32 {
    fn from(v: InteractionType) -> u32 {
        v.0
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct PathType(pub u32);
impl PathType {
    pub const PATH_TYPE_WORLD_TAPPED: PathType = PathType(0);
    pub const PATH_TYPE_WORLD_TAPPED_MOVE_ONLY: PathType = PathType(1);
    pub const PATH_TYPE_RECALCULATED_DUE_TO_CHANGE: PathType = PathType(2);
    pub const PATH_TYPE_NPC_CHASE: PathType = PathType(3);
    pub const PATH_TYPE_RIDE_OBJECT_CHASE: PathType = PathType(4);
    pub const PATH_TYPE_FALL: PathType = PathType(5);
    pub const PATH_TYPE_FALL_INTENTIONAL: PathType = PathType(6);
    #[inline]
    pub const fn raw(self) -> u32 {
        self.0
    }
    #[inline]
    pub const fn from_raw(v: u32) -> Self {
        Self(v)
    }
    /// First-declared constant name for this value (None for unknown/unnamed values).
    pub fn name(self) -> Option<&'static str> {
        match self.0 {
            0 => Some("PATH_TYPE_WORLD_TAPPED"),
            1 => Some("PATH_TYPE_WORLD_TAPPED_MOVE_ONLY"),
            2 => Some("PATH_TYPE_RECALCULATED_DUE_TO_CHANGE"),
            3 => Some("PATH_TYPE_NPC_CHASE"),
            4 => Some("PATH_TYPE_RIDE_OBJECT_CHASE"),
            5 => Some("PATH_TYPE_FALL"),
            6 => Some("PATH_TYPE_FALL_INTENTIONAL"),
            _ => None,
        }
    }
}
impl core::fmt::Debug for PathType {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self.name() {
            Some(n) => write!(f, "{}", n),
            None => write!(f, "PathType({})", self.0),
        }
    }
}
impl From<u32> for PathType {
    fn from(v: u32) -> Self {
        Self(v)
    }
}
impl From<PathType> for u32 {
    fn from(v: PathType) -> u32 {
        v.0
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct CustomRuleSeed(pub u32);
impl CustomRuleSeed {
    pub const CUSTOM_RULE_SEED_RANDOM: CustomRuleSeed = CustomRuleSeed(0);
    pub const CUSTOM_RULE_SEED_CUSTOM: CustomRuleSeed = CustomRuleSeed(1);
    #[inline]
    pub const fn raw(self) -> u32 {
        self.0
    }
    #[inline]
    pub const fn from_raw(v: u32) -> Self {
        Self(v)
    }
    /// First-declared constant name for this value (None for unknown/unnamed values).
    pub fn name(self) -> Option<&'static str> {
        match self.0 {
            0 => Some("CUSTOM_RULE_SEED_RANDOM"),
            1 => Some("CUSTOM_RULE_SEED_CUSTOM"),
            _ => None,
        }
    }
}
impl core::fmt::Debug for CustomRuleSeed {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self.name() {
            Some(n) => write!(f, "{}", n),
            None => write!(f, "CustomRuleSeed({})", self.0),
        }
    }
}
impl From<u32> for CustomRuleSeed {
    fn from(v: u32) -> Self {
        Self(v)
    }
}
impl From<CustomRuleSeed> for u32 {
    fn from(v: CustomRuleSeed) -> u32 {
        v.0
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct CustomRuleOceans(pub u32);
impl CustomRuleOceans {
    pub const CUSTOM_RULE_OCEANS_MIN: CustomRuleOceans = CustomRuleOceans(0);
    pub const CUSTOM_RULE_OCEANS_SMALL: CustomRuleOceans = CustomRuleOceans(1);
    pub const CUSTOM_RULE_OCEANS_NORMAL: CustomRuleOceans = CustomRuleOceans(2);
    pub const CUSTOM_RULE_OCEANS_LARGE: CustomRuleOceans = CustomRuleOceans(3);
    pub const CUSTOM_RULE_OCEANS_MAX: CustomRuleOceans = CustomRuleOceans(4);
    #[inline]
    pub const fn raw(self) -> u32 {
        self.0
    }
    #[inline]
    pub const fn from_raw(v: u32) -> Self {
        Self(v)
    }
    /// First-declared constant name for this value (None for unknown/unnamed values).
    pub fn name(self) -> Option<&'static str> {
        match self.0 {
            0 => Some("CUSTOM_RULE_OCEANS_MIN"),
            1 => Some("CUSTOM_RULE_OCEANS_SMALL"),
            2 => Some("CUSTOM_RULE_OCEANS_NORMAL"),
            3 => Some("CUSTOM_RULE_OCEANS_LARGE"),
            4 => Some("CUSTOM_RULE_OCEANS_MAX"),
            _ => None,
        }
    }
}
impl core::fmt::Debug for CustomRuleOceans {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self.name() {
            Some(n) => write!(f, "{}", n),
            None => write!(f, "CustomRuleOceans({})", self.0),
        }
    }
}
impl From<u32> for CustomRuleOceans {
    fn from(v: u32) -> Self {
        Self(v)
    }
}
impl From<CustomRuleOceans> for u32 {
    fn from(v: CustomRuleOceans) -> u32 {
        v.0
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct CustomRuleTemperature(pub u32);
impl CustomRuleTemperature {
    pub const CUSTOM_RULE_TEMP_VERY_COLD: CustomRuleTemperature = CustomRuleTemperature(0);
    pub const CUSTOM_RULE_TEMP_COLD: CustomRuleTemperature = CustomRuleTemperature(1);
    pub const CUSTOM_RULE_TEMP_NORMAL: CustomRuleTemperature = CustomRuleTemperature(2);
    pub const CUSTOM_RULE_TEMP_WARM: CustomRuleTemperature = CustomRuleTemperature(3);
    pub const CUSTOM_RULE_TEMP_HOT: CustomRuleTemperature = CustomRuleTemperature(4);
    pub const CUSTOM_RULE_TEMP_VERY_HOT: CustomRuleTemperature = CustomRuleTemperature(5);
    #[inline]
    pub const fn raw(self) -> u32 {
        self.0
    }
    #[inline]
    pub const fn from_raw(v: u32) -> Self {
        Self(v)
    }
    /// First-declared constant name for this value (None for unknown/unnamed values).
    pub fn name(self) -> Option<&'static str> {
        match self.0 {
            0 => Some("CUSTOM_RULE_TEMP_VERY_COLD"),
            1 => Some("CUSTOM_RULE_TEMP_COLD"),
            2 => Some("CUSTOM_RULE_TEMP_NORMAL"),
            3 => Some("CUSTOM_RULE_TEMP_WARM"),
            4 => Some("CUSTOM_RULE_TEMP_HOT"),
            5 => Some("CUSTOM_RULE_TEMP_VERY_HOT"),
            _ => None,
        }
    }
}
impl core::fmt::Debug for CustomRuleTemperature {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self.name() {
            Some(n) => write!(f, "{}", n),
            None => write!(f, "CustomRuleTemperature({})", self.0),
        }
    }
}
impl From<u32> for CustomRuleTemperature {
    fn from(v: u32) -> Self {
        Self(v)
    }
}
impl From<CustomRuleTemperature> for u32 {
    fn from(v: CustomRuleTemperature) -> u32 {
        v.0
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct CustomRuleRainfall(pub u32);
impl CustomRuleRainfall {
    pub const CUSTOM_RULE_RAINFALL_VERY_DRY: CustomRuleRainfall = CustomRuleRainfall(0);
    pub const CUSTOM_RULE_RAINFALL_DRY: CustomRuleRainfall = CustomRuleRainfall(1);
    pub const CUSTOM_RULE_RAINFALL_NORMAL: CustomRuleRainfall = CustomRuleRainfall(2);
    pub const CUSTOM_RULE_RAINFALL_WET: CustomRuleRainfall = CustomRuleRainfall(3);
    pub const CUSTOM_RULE_RAINFALL_VERY_WET: CustomRuleRainfall = CustomRuleRainfall(4);
    #[inline]
    pub const fn raw(self) -> u32 {
        self.0
    }
    #[inline]
    pub const fn from_raw(v: u32) -> Self {
        Self(v)
    }
    /// First-declared constant name for this value (None for unknown/unnamed values).
    pub fn name(self) -> Option<&'static str> {
        match self.0 {
            0 => Some("CUSTOM_RULE_RAINFALL_VERY_DRY"),
            1 => Some("CUSTOM_RULE_RAINFALL_DRY"),
            2 => Some("CUSTOM_RULE_RAINFALL_NORMAL"),
            3 => Some("CUSTOM_RULE_RAINFALL_WET"),
            4 => Some("CUSTOM_RULE_RAINFALL_VERY_WET"),
            _ => None,
        }
    }
}
impl core::fmt::Debug for CustomRuleRainfall {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self.name() {
            Some(n) => write!(f, "{}", n),
            None => write!(f, "CustomRuleRainfall({})", self.0),
        }
    }
}
impl From<u32> for CustomRuleRainfall {
    fn from(v: u32) -> Self {
        Self(v)
    }
}
impl From<CustomRuleRainfall> for u32 {
    fn from(v: CustomRuleRainfall) -> u32 {
        v.0
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct CustomRulePlants(pub u32);
impl CustomRulePlants {
    pub const CUSTOM_RULE_PLANTS_NONE: CustomRulePlants = CustomRulePlants(0);
    pub const CUSTOM_RULE_PLANTS_LESS: CustomRulePlants = CustomRulePlants(1);
    pub const CUSTOM_RULE_PLANTS_NORMAL: CustomRulePlants = CustomRulePlants(2);
    pub const CUSTOM_RULE_PLANTS_MORE: CustomRulePlants = CustomRulePlants(3);
    #[inline]
    pub const fn raw(self) -> u32 {
        self.0
    }
    #[inline]
    pub const fn from_raw(v: u32) -> Self {
        Self(v)
    }
    /// First-declared constant name for this value (None for unknown/unnamed values).
    pub fn name(self) -> Option<&'static str> {
        match self.0 {
            0 => Some("CUSTOM_RULE_PLANTS_NONE"),
            1 => Some("CUSTOM_RULE_PLANTS_LESS"),
            2 => Some("CUSTOM_RULE_PLANTS_NORMAL"),
            3 => Some("CUSTOM_RULE_PLANTS_MORE"),
            _ => None,
        }
    }
}
impl core::fmt::Debug for CustomRulePlants {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self.name() {
            Some(n) => write!(f, "{}", n),
            None => write!(f, "CustomRulePlants({})", self.0),
        }
    }
}
impl From<u32> for CustomRulePlants {
    fn from(v: u32) -> Self {
        Self(v)
    }
}
impl From<CustomRulePlants> for u32 {
    fn from(v: CustomRulePlants) -> u32 {
        v.0
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct CustomRuleTrees(pub u32);
impl CustomRuleTrees {
    pub const CUSTOM_RULE_TREES_NONE: CustomRuleTrees = CustomRuleTrees(0);
    pub const CUSTOM_RULE_TREES_GEM_ONLY: CustomRuleTrees = CustomRuleTrees(1);
    pub const CUSTOM_RULE_TREES_FLOATING_ISLAND_ONLY: CustomRuleTrees = CustomRuleTrees(2);
    pub const CUSTOM_RULE_TREES_LESS: CustomRuleTrees = CustomRuleTrees(3);
    pub const CUSTOM_RULE_TREES_NORMAL: CustomRuleTrees = CustomRuleTrees(4);
    pub const CUSTOM_RULE_TREES_MORE: CustomRuleTrees = CustomRuleTrees(5);
    #[inline]
    pub const fn raw(self) -> u32 {
        self.0
    }
    #[inline]
    pub const fn from_raw(v: u32) -> Self {
        Self(v)
    }
    /// First-declared constant name for this value (None for unknown/unnamed values).
    pub fn name(self) -> Option<&'static str> {
        match self.0 {
            0 => Some("CUSTOM_RULE_TREES_NONE"),
            1 => Some("CUSTOM_RULE_TREES_GEM_ONLY"),
            2 => Some("CUSTOM_RULE_TREES_FLOATING_ISLAND_ONLY"),
            3 => Some("CUSTOM_RULE_TREES_LESS"),
            4 => Some("CUSTOM_RULE_TREES_NORMAL"),
            5 => Some("CUSTOM_RULE_TREES_MORE"),
            _ => None,
        }
    }
}
impl core::fmt::Debug for CustomRuleTrees {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self.name() {
            Some(n) => write!(f, "{}", n),
            None => write!(f, "CustomRuleTrees({})", self.0),
        }
    }
}
impl From<u32> for CustomRuleTrees {
    fn from(v: u32) -> Self {
        Self(v)
    }
}
impl From<CustomRuleTrees> for u32 {
    fn from(v: CustomRuleTrees) -> u32 {
        v.0
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct CustomRuleTerrain(pub u32);
impl CustomRuleTerrain {
    pub const CUSTOM_RULE_TERRAIN_TOTALLY_FLAT: CustomRuleTerrain = CustomRuleTerrain(0);
    pub const CUSTOM_RULE_TERRAIN_QUITE_FLAT: CustomRuleTerrain = CustomRuleTerrain(1);
    pub const CUSTOM_RULE_TERRAIN_NORMAL: CustomRuleTerrain = CustomRuleTerrain(2);
    pub const CUSTOM_RULE_TERRAIN_MOUNTAINOUS: CustomRuleTerrain = CustomRuleTerrain(3);
    #[inline]
    pub const fn raw(self) -> u32 {
        self.0
    }
    #[inline]
    pub const fn from_raw(v: u32) -> Self {
        Self(v)
    }
    /// First-declared constant name for this value (None for unknown/unnamed values).
    pub fn name(self) -> Option<&'static str> {
        match self.0 {
            0 => Some("CUSTOM_RULE_TERRAIN_TOTALLY_FLAT"),
            1 => Some("CUSTOM_RULE_TERRAIN_QUITE_FLAT"),
            2 => Some("CUSTOM_RULE_TERRAIN_NORMAL"),
            3 => Some("CUSTOM_RULE_TERRAIN_MOUNTAINOUS"),
            _ => None,
        }
    }
}
impl core::fmt::Debug for CustomRuleTerrain {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self.name() {
            Some(n) => write!(f, "{}", n),
            None => write!(f, "CustomRuleTerrain({})", self.0),
        }
    }
}
impl From<u32> for CustomRuleTerrain {
    fn from(v: u32) -> Self {
        Self(v)
    }
}
impl From<CustomRuleTerrain> for u32 {
    fn from(v: CustomRuleTerrain) -> u32 {
        v.0
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct CustomRuleCaves(pub u32);
impl CustomRuleCaves {
    pub const CUSTOM_RULE_CAVES_NONE: CustomRuleCaves = CustomRuleCaves(0);
    pub const CUSTOM_RULE_CAVES_NORMAL: CustomRuleCaves = CustomRuleCaves(1);
    pub const CUSTOM_RULE_CAVES_MORE: CustomRuleCaves = CustomRuleCaves(2);
    #[inline]
    pub const fn raw(self) -> u32 {
        self.0
    }
    #[inline]
    pub const fn from_raw(v: u32) -> Self {
        Self(v)
    }
    /// First-declared constant name for this value (None for unknown/unnamed values).
    pub fn name(self) -> Option<&'static str> {
        match self.0 {
            0 => Some("CUSTOM_RULE_CAVES_NONE"),
            1 => Some("CUSTOM_RULE_CAVES_NORMAL"),
            2 => Some("CUSTOM_RULE_CAVES_MORE"),
            _ => None,
        }
    }
}
impl core::fmt::Debug for CustomRuleCaves {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self.name() {
            Some(n) => write!(f, "{}", n),
            None => write!(f, "CustomRuleCaves({})", self.0),
        }
    }
}
impl From<u32> for CustomRuleCaves {
    fn from(v: u32) -> Self {
        Self(v)
    }
}
impl From<CustomRuleCaves> for u32 {
    fn from(v: CustomRuleCaves) -> u32 {
        v.0
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct CustomRuleLakes(pub u32);
impl CustomRuleLakes {
    pub const CUSTOM_RULE_LAKES_NONE: CustomRuleLakes = CustomRuleLakes(0);
    pub const CUSTOM_RULE_LAKES_NORMAL: CustomRuleLakes = CustomRuleLakes(1);
    pub const CUSTOM_RULE_LAKES_MORE: CustomRuleLakes = CustomRuleLakes(2);
    #[inline]
    pub const fn raw(self) -> u32 {
        self.0
    }
    #[inline]
    pub const fn from_raw(v: u32) -> Self {
        Self(v)
    }
    /// First-declared constant name for this value (None for unknown/unnamed values).
    pub fn name(self) -> Option<&'static str> {
        match self.0 {
            0 => Some("CUSTOM_RULE_LAKES_NONE"),
            1 => Some("CUSTOM_RULE_LAKES_NORMAL"),
            2 => Some("CUSTOM_RULE_LAKES_MORE"),
            _ => None,
        }
    }
}
impl core::fmt::Debug for CustomRuleLakes {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self.name() {
            Some(n) => write!(f, "{}", n),
            None => write!(f, "CustomRuleLakes({})", self.0),
        }
    }
}
impl From<u32> for CustomRuleLakes {
    fn from(v: u32) -> Self {
        Self(v)
    }
}
impl From<CustomRuleLakes> for u32 {
    fn from(v: CustomRuleLakes) -> u32 {
        v.0
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct CustomRuleSkyIslands(pub u32);
impl CustomRuleSkyIslands {
    pub const CUSTOM_RULE_SKY_ISLANDS_NONE: CustomRuleSkyIslands = CustomRuleSkyIslands(0);
    pub const CUSTOM_RULE_SKY_ISLANDS_LESS: CustomRuleSkyIslands = CustomRuleSkyIslands(1);
    pub const CUSTOM_RULE_SKY_ISLANDS_NORMAL: CustomRuleSkyIslands = CustomRuleSkyIslands(2);
    pub const CUSTOM_RULE_SKY_ISLANDS_MORE: CustomRuleSkyIslands = CustomRuleSkyIslands(3);
    #[inline]
    pub const fn raw(self) -> u32 {
        self.0
    }
    #[inline]
    pub const fn from_raw(v: u32) -> Self {
        Self(v)
    }
    /// First-declared constant name for this value (None for unknown/unnamed values).
    pub fn name(self) -> Option<&'static str> {
        match self.0 {
            0 => Some("CUSTOM_RULE_SKY_ISLANDS_NONE"),
            1 => Some("CUSTOM_RULE_SKY_ISLANDS_LESS"),
            2 => Some("CUSTOM_RULE_SKY_ISLANDS_NORMAL"),
            3 => Some("CUSTOM_RULE_SKY_ISLANDS_MORE"),
            _ => None,
        }
    }
}
impl core::fmt::Debug for CustomRuleSkyIslands {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self.name() {
            Some(n) => write!(f, "{}", n),
            None => write!(f, "CustomRuleSkyIslands({})", self.0),
        }
    }
}
impl From<u32> for CustomRuleSkyIslands {
    fn from(v: u32) -> Self {
        Self(v)
    }
}
impl From<CustomRuleSkyIslands> for u32 {
    fn from(v: CustomRuleSkyIslands) -> u32 {
        v.0
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct CustomRuleOre(pub u32);
impl CustomRuleOre {
    pub const CUSTOM_RULE_ORE_NONE: CustomRuleOre = CustomRuleOre(0);
    pub const CUSTOM_RULE_ORE_LESS: CustomRuleOre = CustomRuleOre(1);
    pub const CUSTOM_RULE_ORE_NORMAL: CustomRuleOre = CustomRuleOre(2);
    pub const CUSTOM_RULE_ORE_MORE: CustomRuleOre = CustomRuleOre(3);
    #[inline]
    pub const fn raw(self) -> u32 {
        self.0
    }
    #[inline]
    pub const fn from_raw(v: u32) -> Self {
        Self(v)
    }
    /// First-declared constant name for this value (None for unknown/unnamed values).
    pub fn name(self) -> Option<&'static str> {
        match self.0 {
            0 => Some("CUSTOM_RULE_ORE_NONE"),
            1 => Some("CUSTOM_RULE_ORE_LESS"),
            2 => Some("CUSTOM_RULE_ORE_NORMAL"),
            3 => Some("CUSTOM_RULE_ORE_MORE"),
            _ => None,
        }
    }
}
impl core::fmt::Debug for CustomRuleOre {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self.name() {
            Some(n) => write!(f, "{}", n),
            None => write!(f, "CustomRuleOre({})", self.0),
        }
    }
}
impl From<u32> for CustomRuleOre {
    fn from(v: u32) -> Self {
        Self(v)
    }
}
impl From<CustomRuleOre> for u32 {
    fn from(v: CustomRuleOre) -> u32 {
        v.0
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct CustomRuleOreDistribution(pub u32);
impl CustomRuleOreDistribution {
    pub const CUSTOM_RULE_ORE_DISTRIBUTION_SMALL_VEINS: CustomRuleOreDistribution = CustomRuleOreDistribution(0);
    pub const CUSTOM_RULE_ORE_DISTRIBUTION_NORMAL: CustomRuleOreDistribution = CustomRuleOreDistribution(1);
    pub const CUSTOM_RULE_ORE_DISTRIBUTION_LARGE_VEINS: CustomRuleOreDistribution = CustomRuleOreDistribution(2);
    #[inline]
    pub const fn raw(self) -> u32 {
        self.0
    }
    #[inline]
    pub const fn from_raw(v: u32) -> Self {
        Self(v)
    }
    /// First-declared constant name for this value (None for unknown/unnamed values).
    pub fn name(self) -> Option<&'static str> {
        match self.0 {
            0 => Some("CUSTOM_RULE_ORE_DISTRIBUTION_SMALL_VEINS"),
            1 => Some("CUSTOM_RULE_ORE_DISTRIBUTION_NORMAL"),
            2 => Some("CUSTOM_RULE_ORE_DISTRIBUTION_LARGE_VEINS"),
            _ => None,
        }
    }
}
impl core::fmt::Debug for CustomRuleOreDistribution {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self.name() {
            Some(n) => write!(f, "{}", n),
            None => write!(f, "CustomRuleOreDistribution({})", self.0),
        }
    }
}
impl From<u32> for CustomRuleOreDistribution {
    fn from(v: u32) -> Self {
        Self(v)
    }
}
impl From<CustomRuleOreDistribution> for u32 {
    fn from(v: CustomRuleOreDistribution) -> u32 {
        v.0
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct CustomRuleGems(pub u32);
impl CustomRuleGems {
    pub const CUSTOM_RULE_GEMS_NONE: CustomRuleGems = CustomRuleGems(0);
    pub const CUSTOM_RULE_GEMS_LESS: CustomRuleGems = CustomRuleGems(1);
    pub const CUSTOM_RULE_GEMS_NORMAL: CustomRuleGems = CustomRuleGems(2);
    pub const CUSTOM_RULE_GEMS_MORE: CustomRuleGems = CustomRuleGems(3);
    #[inline]
    pub const fn raw(self) -> u32 {
        self.0
    }
    #[inline]
    pub const fn from_raw(v: u32) -> Self {
        Self(v)
    }
    /// First-declared constant name for this value (None for unknown/unnamed values).
    pub fn name(self) -> Option<&'static str> {
        match self.0 {
            0 => Some("CUSTOM_RULE_GEMS_NONE"),
            1 => Some("CUSTOM_RULE_GEMS_LESS"),
            2 => Some("CUSTOM_RULE_GEMS_NORMAL"),
            3 => Some("CUSTOM_RULE_GEMS_MORE"),
            _ => None,
        }
    }
}
impl core::fmt::Debug for CustomRuleGems {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self.name() {
            Some(n) => write!(f, "{}", n),
            None => write!(f, "CustomRuleGems({})", self.0),
        }
    }
}
impl From<u32> for CustomRuleGems {
    fn from(v: u32) -> Self {
        Self(v)
    }
}
impl From<CustomRuleGems> for u32 {
    fn from(v: CustomRuleGems) -> u32 {
        v.0
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct CustomRuleTreasure(pub u32);
impl CustomRuleTreasure {
    pub const CUSTOM_RULE_TREASURE_NONE: CustomRuleTreasure = CustomRuleTreasure(0);
    pub const CUSTOM_RULE_TREASURE_LESS: CustomRuleTreasure = CustomRuleTreasure(1);
    pub const CUSTOM_RULE_TREASURE_NORMAL: CustomRuleTreasure = CustomRuleTreasure(2);
    pub const CUSTOM_RULE_TREASURE_MORE: CustomRuleTreasure = CustomRuleTreasure(3);
    #[inline]
    pub const fn raw(self) -> u32 {
        self.0
    }
    #[inline]
    pub const fn from_raw(v: u32) -> Self {
        Self(v)
    }
    /// First-declared constant name for this value (None for unknown/unnamed values).
    pub fn name(self) -> Option<&'static str> {
        match self.0 {
            0 => Some("CUSTOM_RULE_TREASURE_NONE"),
            1 => Some("CUSTOM_RULE_TREASURE_LESS"),
            2 => Some("CUSTOM_RULE_TREASURE_NORMAL"),
            3 => Some("CUSTOM_RULE_TREASURE_MORE"),
            _ => None,
        }
    }
}
impl core::fmt::Debug for CustomRuleTreasure {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self.name() {
            Some(n) => write!(f, "{}", n),
            None => write!(f, "CustomRuleTreasure({})", self.0),
        }
    }
}
impl From<u32> for CustomRuleTreasure {
    fn from(v: u32) -> Self {
        Self(v)
    }
}
impl From<CustomRuleTreasure> for u32 {
    fn from(v: CustomRuleTreasure) -> u32 {
        v.0
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct CustomRuleHealth(pub u32);
impl CustomRuleHealth {
    pub const CUSTOM_RULE_HEALTH_ONE_HIT: CustomRuleHealth = CustomRuleHealth(0);
    pub const CUSTOM_RULE_HEALTH_HARD: CustomRuleHealth = CustomRuleHealth(1);
    pub const CUSTOM_RULE_HEALTH_NORMAL: CustomRuleHealth = CustomRuleHealth(2);
    pub const CUSTOM_RULE_HEALTH_EASY: CustomRuleHealth = CustomRuleHealth(3);
    pub const CUSTOM_RULE_HEALTH_INFINITE: CustomRuleHealth = CustomRuleHealth(4);
    #[inline]
    pub const fn raw(self) -> u32 {
        self.0
    }
    #[inline]
    pub const fn from_raw(v: u32) -> Self {
        Self(v)
    }
    /// First-declared constant name for this value (None for unknown/unnamed values).
    pub fn name(self) -> Option<&'static str> {
        match self.0 {
            0 => Some("CUSTOM_RULE_HEALTH_ONE_HIT"),
            1 => Some("CUSTOM_RULE_HEALTH_HARD"),
            2 => Some("CUSTOM_RULE_HEALTH_NORMAL"),
            3 => Some("CUSTOM_RULE_HEALTH_EASY"),
            4 => Some("CUSTOM_RULE_HEALTH_INFINITE"),
            _ => None,
        }
    }
}
impl core::fmt::Debug for CustomRuleHealth {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self.name() {
            Some(n) => write!(f, "{}", n),
            None => write!(f, "CustomRuleHealth({})", self.0),
        }
    }
}
impl From<u32> for CustomRuleHealth {
    fn from(v: u32) -> Self {
        Self(v)
    }
}
impl From<CustomRuleHealth> for u32 {
    fn from(v: CustomRuleHealth) -> u32 {
        v.0
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct CustomRuleHunger(pub u32);
impl CustomRuleHunger {
    pub const CUSTOM_RULE_HUNGER_HUNGRIER: CustomRuleHunger = CustomRuleHunger(0);
    pub const CUSTOM_RULE_HUNGER_NORMAL: CustomRuleHunger = CustomRuleHunger(1);
    pub const CUSTOM_RULE_HUNGER_FULLER: CustomRuleHunger = CustomRuleHunger(2);
    pub const CUSTOM_RULE_HUNGER_ALWAYS_FULL: CustomRuleHunger = CustomRuleHunger(3);
    #[inline]
    pub const fn raw(self) -> u32 {
        self.0
    }
    #[inline]
    pub const fn from_raw(v: u32) -> Self {
        Self(v)
    }
    /// First-declared constant name for this value (None for unknown/unnamed values).
    pub fn name(self) -> Option<&'static str> {
        match self.0 {
            0 => Some("CUSTOM_RULE_HUNGER_HUNGRIER"),
            1 => Some("CUSTOM_RULE_HUNGER_NORMAL"),
            2 => Some("CUSTOM_RULE_HUNGER_FULLER"),
            3 => Some("CUSTOM_RULE_HUNGER_ALWAYS_FULL"),
            _ => None,
        }
    }
}
impl core::fmt::Debug for CustomRuleHunger {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self.name() {
            Some(n) => write!(f, "{}", n),
            None => write!(f, "CustomRuleHunger({})", self.0),
        }
    }
}
impl From<u32> for CustomRuleHunger {
    fn from(v: u32) -> Self {
        Self(v)
    }
}
impl From<CustomRuleHunger> for u32 {
    fn from(v: CustomRuleHunger) -> u32 {
        v.0
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct CustomRuleHappiness(pub u32);
impl CustomRuleHappiness {
    pub const CUSTOM_RULE_HAPPINESS_SADDER: CustomRuleHappiness = CustomRuleHappiness(0);
    pub const CUSTOM_RULE_HAPPINESS_NORMAL: CustomRuleHappiness = CustomRuleHappiness(1);
    pub const CUSTOM_RULE_HAPPINESS_HAPPIER: CustomRuleHappiness = CustomRuleHappiness(2);
    pub const CUSTOM_RULE_HAPPINESS_ALWAYS_HAPPY: CustomRuleHappiness = CustomRuleHappiness(3);
    #[inline]
    pub const fn raw(self) -> u32 {
        self.0
    }
    #[inline]
    pub const fn from_raw(v: u32) -> Self {
        Self(v)
    }
    /// First-declared constant name for this value (None for unknown/unnamed values).
    pub fn name(self) -> Option<&'static str> {
        match self.0 {
            0 => Some("CUSTOM_RULE_HAPPINESS_SADDER"),
            1 => Some("CUSTOM_RULE_HAPPINESS_NORMAL"),
            2 => Some("CUSTOM_RULE_HAPPINESS_HAPPIER"),
            3 => Some("CUSTOM_RULE_HAPPINESS_ALWAYS_HAPPY"),
            _ => None,
        }
    }
}
impl core::fmt::Debug for CustomRuleHappiness {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self.name() {
            Some(n) => write!(f, "{}", n),
            None => write!(f, "CustomRuleHappiness({})", self.0),
        }
    }
}
impl From<u32> for CustomRuleHappiness {
    fn from(v: u32) -> Self {
        Self(v)
    }
}
impl From<CustomRuleHappiness> for u32 {
    fn from(v: CustomRuleHappiness) -> u32 {
        v.0
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct CustomRuleTradePortal(pub u32);
impl CustomRuleTradePortal {
    pub const CUSTOM_RULE_TRADE_PORTAL_NORMAL: CustomRuleTradePortal = CustomRuleTradePortal(0);
    pub const CUSTOM_RULE_TRADE_PORTAL_DISABLED: CustomRuleTradePortal = CustomRuleTradePortal(1);
    pub const CUSTOM_RULE_TRADE_PORTAL_BUY_ONLY: CustomRuleTradePortal = CustomRuleTradePortal(2);
    pub const CUSTOM_RULE_TRADE_PORTAL_SELL_ONLY: CustomRuleTradePortal = CustomRuleTradePortal(3);
    pub const CUSTOM_RULE_TRADE_PORTAL_EVERYTHING_FREE: CustomRuleTradePortal = CustomRuleTradePortal(4);
    pub const CUSTOM_RULE_TRADE_PORTAL_MISSIONS_ONLY: CustomRuleTradePortal = CustomRuleTradePortal(5);
    #[inline]
    pub const fn raw(self) -> u32 {
        self.0
    }
    #[inline]
    pub const fn from_raw(v: u32) -> Self {
        Self(v)
    }
    /// First-declared constant name for this value (None for unknown/unnamed values).
    pub fn name(self) -> Option<&'static str> {
        match self.0 {
            0 => Some("CUSTOM_RULE_TRADE_PORTAL_NORMAL"),
            1 => Some("CUSTOM_RULE_TRADE_PORTAL_DISABLED"),
            2 => Some("CUSTOM_RULE_TRADE_PORTAL_BUY_ONLY"),
            3 => Some("CUSTOM_RULE_TRADE_PORTAL_SELL_ONLY"),
            4 => Some("CUSTOM_RULE_TRADE_PORTAL_EVERYTHING_FREE"),
            5 => Some("CUSTOM_RULE_TRADE_PORTAL_MISSIONS_ONLY"),
            _ => None,
        }
    }
}
impl core::fmt::Debug for CustomRuleTradePortal {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self.name() {
            Some(n) => write!(f, "{}", n),
            None => write!(f, "CustomRuleTradePortal({})", self.0),
        }
    }
}
impl From<u32> for CustomRuleTradePortal {
    fn from(v: u32) -> Self {
        Self(v)
    }
}
impl From<CustomRuleTradePortal> for u32 {
    fn from(v: CustomRuleTradePortal) -> u32 {
        v.0
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct CustomRuleNpc(pub u32);
impl CustomRuleNpc {
    pub const CUSTOM_RULE_NPC_HARD: CustomRuleNpc = CustomRuleNpc(0);
    pub const CUSTOM_RULE_NPC_NORMAL: CustomRuleNpc = CustomRuleNpc(1);
    pub const CUSTOM_RULE_NPC_EASY: CustomRuleNpc = CustomRuleNpc(2);
    pub const CUSTOM_RULE_NPC_PASSIVE: CustomRuleNpc = CustomRuleNpc(3);
    #[inline]
    pub const fn raw(self) -> u32 {
        self.0
    }
    #[inline]
    pub const fn from_raw(v: u32) -> Self {
        Self(v)
    }
    /// First-declared constant name for this value (None for unknown/unnamed values).
    pub fn name(self) -> Option<&'static str> {
        match self.0 {
            0 => Some("CUSTOM_RULE_NPC_HARD"),
            1 => Some("CUSTOM_RULE_NPC_NORMAL"),
            2 => Some("CUSTOM_RULE_NPC_EASY"),
            3 => Some("CUSTOM_RULE_NPC_PASSIVE"),
            _ => None,
        }
    }
}
impl core::fmt::Debug for CustomRuleNpc {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self.name() {
            Some(n) => write!(f, "{}", n),
            None => write!(f, "CustomRuleNpc({})", self.0),
        }
    }
}
impl From<u32> for CustomRuleNpc {
    fn from(v: u32) -> Self {
        Self(v)
    }
}
impl From<CustomRuleNpc> for u32 {
    fn from(v: CustomRuleNpc) -> u32 {
        v.0
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct CustomRuleClothingDecay(pub u32);
impl CustomRuleClothingDecay {
    pub const CUSTOM_RULE_CLOTHING_DECAY_NONE: CustomRuleClothingDecay = CustomRuleClothingDecay(0);
    pub const CUSTOM_RULE_CLOTHING_DECAY_SLOW: CustomRuleClothingDecay = CustomRuleClothingDecay(1);
    pub const CUSTOM_RULE_CLOTHING_DECAY_NORMAL: CustomRuleClothingDecay = CustomRuleClothingDecay(2);
    pub const CUSTOM_RULE_CLOTHING_DECAY_FAST: CustomRuleClothingDecay = CustomRuleClothingDecay(3);
    #[inline]
    pub const fn raw(self) -> u32 {
        self.0
    }
    #[inline]
    pub const fn from_raw(v: u32) -> Self {
        Self(v)
    }
    /// First-declared constant name for this value (None for unknown/unnamed values).
    pub fn name(self) -> Option<&'static str> {
        match self.0 {
            0 => Some("CUSTOM_RULE_CLOTHING_DECAY_NONE"),
            1 => Some("CUSTOM_RULE_CLOTHING_DECAY_SLOW"),
            2 => Some("CUSTOM_RULE_CLOTHING_DECAY_NORMAL"),
            3 => Some("CUSTOM_RULE_CLOTHING_DECAY_FAST"),
            _ => None,
        }
    }
}
impl core::fmt::Debug for CustomRuleClothingDecay {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self.name() {
            Some(n) => write!(f, "{}", n),
            None => write!(f, "CustomRuleClothingDecay({})", self.0),
        }
    }
}
impl From<u32> for CustomRuleClothingDecay {
    fn from(v: u32) -> Self {
        Self(v)
    }
}
impl From<CustomRuleClothingDecay> for u32 {
    fn from(v: CustomRuleClothingDecay) -> u32 {
        v.0
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct CustomRuleToolDecay(pub u32);
impl CustomRuleToolDecay {
    pub const CUSTOM_RULE_TOOL_DECAY_NONE: CustomRuleToolDecay = CustomRuleToolDecay(0);
    pub const CUSTOM_RULE_TOOL_DECAY_SLOW: CustomRuleToolDecay = CustomRuleToolDecay(1);
    pub const CUSTOM_RULE_TOOL_DECAY_NORMAL: CustomRuleToolDecay = CustomRuleToolDecay(2);
    pub const CUSTOM_RULE_TOOL_DECAY_FAST: CustomRuleToolDecay = CustomRuleToolDecay(3);
    #[inline]
    pub const fn raw(self) -> u32 {
        self.0
    }
    #[inline]
    pub const fn from_raw(v: u32) -> Self {
        Self(v)
    }
    /// First-declared constant name for this value (None for unknown/unnamed values).
    pub fn name(self) -> Option<&'static str> {
        match self.0 {
            0 => Some("CUSTOM_RULE_TOOL_DECAY_NONE"),
            1 => Some("CUSTOM_RULE_TOOL_DECAY_SLOW"),
            2 => Some("CUSTOM_RULE_TOOL_DECAY_NORMAL"),
            3 => Some("CUSTOM_RULE_TOOL_DECAY_FAST"),
            _ => None,
        }
    }
}
impl core::fmt::Debug for CustomRuleToolDecay {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self.name() {
            Some(n) => write!(f, "{}", n),
            None => write!(f, "CustomRuleToolDecay({})", self.0),
        }
    }
}
impl From<u32> for CustomRuleToolDecay {
    fn from(v: u32) -> Self {
        Self(v)
    }
}
impl From<CustomRuleToolDecay> for u32 {
    fn from(v: CustomRuleToolDecay) -> u32 {
        v.0
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct CustomRuleDeath(pub u32);
impl CustomRuleDeath {
    pub const CUSTOM_RULE_DEATH_NONE: CustomRuleDeath = CustomRuleDeath(0);
    pub const CUSTOM_RULE_DEATH_NO_RIP: CustomRuleDeath = CustomRuleDeath(1);
    pub const CUSTOM_RULE_DEATH_NORMAL: CustomRuleDeath = CustomRuleDeath(2);
    pub const CUSTOM_RULE_DEATH_NO_REGEN: CustomRuleDeath = CustomRuleDeath(3);
    pub const CUSTOM_RULE_DEATH_DIE_ON_EXIT: CustomRuleDeath = CustomRuleDeath(4);
    #[inline]
    pub const fn raw(self) -> u32 {
        self.0
    }
    #[inline]
    pub const fn from_raw(v: u32) -> Self {
        Self(v)
    }
    /// First-declared constant name for this value (None for unknown/unnamed values).
    pub fn name(self) -> Option<&'static str> {
        match self.0 {
            0 => Some("CUSTOM_RULE_DEATH_NONE"),
            1 => Some("CUSTOM_RULE_DEATH_NO_RIP"),
            2 => Some("CUSTOM_RULE_DEATH_NORMAL"),
            3 => Some("CUSTOM_RULE_DEATH_NO_REGEN"),
            4 => Some("CUSTOM_RULE_DEATH_DIE_ON_EXIT"),
            _ => None,
        }
    }
}
impl core::fmt::Debug for CustomRuleDeath {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self.name() {
            Some(n) => write!(f, "{}", n),
            None => write!(f, "CustomRuleDeath({})", self.0),
        }
    }
}
impl From<u32> for CustomRuleDeath {
    fn from(v: u32) -> Self {
        Self(v)
    }
}
impl From<CustomRuleDeath> for u32 {
    fn from(v: CustomRuleDeath) -> u32 {
        v.0
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct CustomRuleDeathDrops(pub u32);
impl CustomRuleDeathDrops {
    pub const CUSTOM_RULE_DEATH_DROPS_ALL_ITEMS: CustomRuleDeathDrops = CustomRuleDeathDrops(0);
    pub const CUSTOM_RULE_DEATH_DROPS_SOME_ITEMS: CustomRuleDeathDrops = CustomRuleDeathDrops(1);
    pub const CUSTOM_RULE_DEATH_DROPS_NO_ITEMS: CustomRuleDeathDrops = CustomRuleDeathDrops(2);
    #[inline]
    pub const fn raw(self) -> u32 {
        self.0
    }
    #[inline]
    pub const fn from_raw(v: u32) -> Self {
        Self(v)
    }
    /// First-declared constant name for this value (None for unknown/unnamed values).
    pub fn name(self) -> Option<&'static str> {
        match self.0 {
            0 => Some("CUSTOM_RULE_DEATH_DROPS_ALL_ITEMS"),
            1 => Some("CUSTOM_RULE_DEATH_DROPS_SOME_ITEMS"),
            2 => Some("CUSTOM_RULE_DEATH_DROPS_NO_ITEMS"),
            _ => None,
        }
    }
}
impl core::fmt::Debug for CustomRuleDeathDrops {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self.name() {
            Some(n) => write!(f, "{}", n),
            None => write!(f, "CustomRuleDeathDrops({})", self.0),
        }
    }
}
impl From<u32> for CustomRuleDeathDrops {
    fn from(v: u32) -> Self {
        Self(v)
    }
}
impl From<CustomRuleDeathDrops> for u32 {
    fn from(v: CustomRuleDeathDrops) -> u32 {
        v.0
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct CustomRuleDayNight(pub u32);
impl CustomRuleDayNight {
    pub const CUSTOM_RULE_DAY_NIGHT_SLOWER: CustomRuleDayNight = CustomRuleDayNight(0);
    pub const CUSTOM_RULE_DAY_NIGHT_NORMAL: CustomRuleDayNight = CustomRuleDayNight(1);
    pub const CUSTOM_RULE_DAY_NIGHT_FASTER: CustomRuleDayNight = CustomRuleDayNight(2);
    #[inline]
    pub const fn raw(self) -> u32 {
        self.0
    }
    #[inline]
    pub const fn from_raw(v: u32) -> Self {
        Self(v)
    }
    /// First-declared constant name for this value (None for unknown/unnamed values).
    pub fn name(self) -> Option<&'static str> {
        match self.0 {
            0 => Some("CUSTOM_RULE_DAY_NIGHT_SLOWER"),
            1 => Some("CUSTOM_RULE_DAY_NIGHT_NORMAL"),
            2 => Some("CUSTOM_RULE_DAY_NIGHT_FASTER"),
            _ => None,
        }
    }
}
impl core::fmt::Debug for CustomRuleDayNight {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self.name() {
            Some(n) => write!(f, "{}", n),
            None => write!(f, "CustomRuleDayNight({})", self.0),
        }
    }
}
impl From<u32> for CustomRuleDayNight {
    fn from(v: u32) -> Self {
        Self(v)
    }
}
impl From<CustomRuleDayNight> for u32 {
    fn from(v: CustomRuleDayNight) -> u32 {
        v.0
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct CustomRuleWorldModification(pub u32);
impl CustomRuleWorldModification {
    pub const CUSTOM_RULE_WORLD_MODIFCATION_ENABLED: CustomRuleWorldModification = CustomRuleWorldModification(0);
    pub const CUSTOM_RULE_WORLD_MODIFCATION_ADMIN_ONLY: CustomRuleWorldModification = CustomRuleWorldModification(1);
    pub const CUSTOM_RULE_WORLD_MODIFCATION_DISABLED: CustomRuleWorldModification = CustomRuleWorldModification(2);
    #[inline]
    pub const fn raw(self) -> u32 {
        self.0
    }
    #[inline]
    pub const fn from_raw(v: u32) -> Self {
        Self(v)
    }
    /// First-declared constant name for this value (None for unknown/unnamed values).
    pub fn name(self) -> Option<&'static str> {
        match self.0 {
            0 => Some("CUSTOM_RULE_WORLD_MODIFCATION_ENABLED"),
            1 => Some("CUSTOM_RULE_WORLD_MODIFCATION_ADMIN_ONLY"),
            2 => Some("CUSTOM_RULE_WORLD_MODIFCATION_DISABLED"),
            _ => None,
        }
    }
}
impl core::fmt::Debug for CustomRuleWorldModification {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self.name() {
            Some(n) => write!(f, "{}", n),
            None => write!(f, "CustomRuleWorldModification({})", self.0),
        }
    }
}
impl From<u32> for CustomRuleWorldModification {
    fn from(v: u32) -> Self {
        Self(v)
    }
}
impl From<CustomRuleWorldModification> for u32 {
    fn from(v: CustomRuleWorldModification) -> u32 {
        v.0
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct CustomRulePoleItems(pub u32);
impl CustomRulePoleItems {
    pub const CUSTOM_RULE_POLE_ITEMS_DEGRADE_AND_RESPAWN: CustomRulePoleItems = CustomRulePoleItems(0);
    pub const CUSTOM_RULE_POLE_ITEMS_INFINITE: CustomRulePoleItems = CustomRulePoleItems(1);
    #[inline]
    pub const fn raw(self) -> u32 {
        self.0
    }
    #[inline]
    pub const fn from_raw(v: u32) -> Self {
        Self(v)
    }
    /// First-declared constant name for this value (None for unknown/unnamed values).
    pub fn name(self) -> Option<&'static str> {
        match self.0 {
            0 => Some("CUSTOM_RULE_POLE_ITEMS_DEGRADE_AND_RESPAWN"),
            1 => Some("CUSTOM_RULE_POLE_ITEMS_INFINITE"),
            _ => None,
        }
    }
}
impl core::fmt::Debug for CustomRulePoleItems {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self.name() {
            Some(n) => write!(f, "{}", n),
            None => write!(f, "CustomRulePoleItems({})", self.0),
        }
    }
}
impl From<u32> for CustomRulePoleItems {
    fn from(v: u32) -> Self {
        Self(v)
    }
}
impl From<CustomRulePoleItems> for u32 {
    fn from(v: CustomRulePoleItems) -> u32 {
        v.0
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct CustomRuleTeleportation(pub u32);
impl CustomRuleTeleportation {
    pub const CUSTOM_RULE_TELPORTATION_NORMAL: CustomRuleTeleportation = CustomRuleTeleportation(0);
    pub const CUSTOM_RULE_TELPORTATION_REQUIRES_DIAMOND_PORTAL: CustomRuleTeleportation = CustomRuleTeleportation(1);
    pub const CUSTOM_RULE_TELPORTATION_DISABLED: CustomRuleTeleportation = CustomRuleTeleportation(2);
    #[inline]
    pub const fn raw(self) -> u32 {
        self.0
    }
    #[inline]
    pub const fn from_raw(v: u32) -> Self {
        Self(v)
    }
    /// First-declared constant name for this value (None for unknown/unnamed values).
    pub fn name(self) -> Option<&'static str> {
        match self.0 {
            0 => Some("CUSTOM_RULE_TELPORTATION_NORMAL"),
            1 => Some("CUSTOM_RULE_TELPORTATION_REQUIRES_DIAMOND_PORTAL"),
            2 => Some("CUSTOM_RULE_TELPORTATION_DISABLED"),
            _ => None,
        }
    }
}
impl core::fmt::Debug for CustomRuleTeleportation {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self.name() {
            Some(n) => write!(f, "{}", n),
            None => write!(f, "CustomRuleTeleportation({})", self.0),
        }
    }
}
impl From<u32> for CustomRuleTeleportation {
    fn from(v: u32) -> Self {
        Self(v)
    }
}
impl From<CustomRuleTeleportation> for u32 {
    fn from(v: CustomRuleTeleportation) -> u32 {
        v.0
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct CustomRuleTradePortalPrices(pub u32);
impl CustomRuleTradePortalPrices {
    pub const CUSTOM_RULE_TRADE_PORTAL_PRICES_4X: CustomRuleTradePortalPrices = CustomRuleTradePortalPrices(0);
    pub const CUSTOM_RULE_TRADE_PORTAL_PRICES_10PERCENT: CustomRuleTradePortalPrices = CustomRuleTradePortalPrices(1);
    pub const CUSTOM_RULE_TRADE_PORTAL_PRICES_EQUAL: CustomRuleTradePortalPrices = CustomRuleTradePortalPrices(2);
    #[inline]
    pub const fn raw(self) -> u32 {
        self.0
    }
    #[inline]
    pub const fn from_raw(v: u32) -> Self {
        Self(v)
    }
    /// First-declared constant name for this value (None for unknown/unnamed values).
    pub fn name(self) -> Option<&'static str> {
        match self.0 {
            0 => Some("CUSTOM_RULE_TRADE_PORTAL_PRICES_4X"),
            1 => Some("CUSTOM_RULE_TRADE_PORTAL_PRICES_10PERCENT"),
            2 => Some("CUSTOM_RULE_TRADE_PORTAL_PRICES_EQUAL"),
            _ => None,
        }
    }
}
impl core::fmt::Debug for CustomRuleTradePortalPrices {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self.name() {
            Some(n) => write!(f, "{}", n),
            None => write!(f, "CustomRuleTradePortalPrices({})", self.0),
        }
    }
}
impl From<u32> for CustomRuleTradePortalPrices {
    fn from(v: u32) -> Self {
        Self(v)
    }
}
impl From<CustomRuleTradePortalPrices> for u32 {
    fn from(v: CustomRuleTradePortalPrices) -> u32 {
        v.0
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct CustomRuleTradePortalDiscovered(pub u32);
impl CustomRuleTradePortalDiscovered {
    pub const CUSTOM_RULE_TRADE_PORTAL_DISCOVERED_REQUIRED: CustomRuleTradePortalDiscovered =
        CustomRuleTradePortalDiscovered(0);
    pub const CUSTOM_RULE_TRADE_PORTAL_ALL_AVAILABLE: CustomRuleTradePortalDiscovered =
        CustomRuleTradePortalDiscovered(1);
    #[inline]
    pub const fn raw(self) -> u32 {
        self.0
    }
    #[inline]
    pub const fn from_raw(v: u32) -> Self {
        Self(v)
    }
    /// First-declared constant name for this value (None for unknown/unnamed values).
    pub fn name(self) -> Option<&'static str> {
        match self.0 {
            0 => Some("CUSTOM_RULE_TRADE_PORTAL_DISCOVERED_REQUIRED"),
            1 => Some("CUSTOM_RULE_TRADE_PORTAL_ALL_AVAILABLE"),
            _ => None,
        }
    }
}
impl core::fmt::Debug for CustomRuleTradePortalDiscovered {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self.name() {
            Some(n) => write!(f, "{}", n),
            None => write!(f, "CustomRuleTradePortalDiscovered({})", self.0),
        }
    }
}
impl From<u32> for CustomRuleTradePortalDiscovered {
    fn from(v: u32) -> Self {
        Self(v)
    }
}
impl From<CustomRuleTradePortalDiscovered> for u32 {
    fn from(v: CustomRuleTradePortalDiscovered) -> u32 {
        v.0
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct CustomRuleMeditation(pub u32);
impl CustomRuleMeditation {
    pub const CUSTOM_RULE_MEDITATION_ENABLED: CustomRuleMeditation = CustomRuleMeditation(0);
    pub const CUSTOM_RULE_MEDITATION_DISABLED: CustomRuleMeditation = CustomRuleMeditation(1);
    #[inline]
    pub const fn raw(self) -> u32 {
        self.0
    }
    #[inline]
    pub const fn from_raw(v: u32) -> Self {
        Self(v)
    }
    /// First-declared constant name for this value (None for unknown/unnamed values).
    pub fn name(self) -> Option<&'static str> {
        match self.0 {
            0 => Some("CUSTOM_RULE_MEDITATION_ENABLED"),
            1 => Some("CUSTOM_RULE_MEDITATION_DISABLED"),
            _ => None,
        }
    }
}
impl core::fmt::Debug for CustomRuleMeditation {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self.name() {
            Some(n) => write!(f, "{}", n),
            None => write!(f, "CustomRuleMeditation({})", self.0),
        }
    }
}
impl From<u32> for CustomRuleMeditation {
    fn from(v: u32) -> Self {
        Self(v)
    }
}
impl From<CustomRuleMeditation> for u32 {
    fn from(v: CustomRuleMeditation) -> u32 {
        v.0
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct CustomRuleCategory(pub u32);
impl CustomRuleCategory {
    pub const CUSTOM_RULE_CATEGORY_RANDOM_SEED: CustomRuleCategory = CustomRuleCategory(1);
    pub const CUSTOM_RULE_CATEGORY_OCEANS: CustomRuleCategory = CustomRuleCategory(2);
    pub const CUSTOM_RULE_CATEGORY_TEMPERATURE: CustomRuleCategory = CustomRuleCategory(3);
    pub const CUSTOM_RULE_CATEGORY_RAINFALL: CustomRuleCategory = CustomRuleCategory(4);
    pub const CUSTOM_RULE_CATEGORY_PLANTS: CustomRuleCategory = CustomRuleCategory(5);
    pub const CUSTOM_RULE_CATEGORY_TREES: CustomRuleCategory = CustomRuleCategory(6);
    pub const CUSTOM_RULE_CATEGORY_TERRAIN_TYPE: CustomRuleCategory = CustomRuleCategory(7);
    pub const CUSTOM_RULE_CATEGORY_CAVES: CustomRuleCategory = CustomRuleCategory(8);
    pub const CUSTOM_RULE_CATEGORY_LAKES: CustomRuleCategory = CustomRuleCategory(9);
    pub const CUSTOM_RULE_CATEGORY_SKY_ISLANDS: CustomRuleCategory = CustomRuleCategory(10);
    pub const CUSTOM_RULE_CATEGORY_ORE_QUANTITY: CustomRuleCategory = CustomRuleCategory(11);
    pub const CUSTOM_RULE_CATEGORY_ORE_DISTRIBUTION: CustomRuleCategory = CustomRuleCategory(12);
    pub const CUSTOM_RULE_CATEGORY_GEMS: CustomRuleCategory = CustomRuleCategory(13);
    pub const CUSTOM_RULE_CATEGORY_TREASURE: CustomRuleCategory = CustomRuleCategory(14);
    pub const CUSTOM_RULE_CATEGORY_HEALTH: CustomRuleCategory = CustomRuleCategory(999);
    pub const CUSTOM_RULE_CATEGORY_HUNGER: CustomRuleCategory = CustomRuleCategory(1000);
    pub const CUSTOM_RULE_CATEGORY_SPAWN_ITEMS: CustomRuleCategory = CustomRuleCategory(1001);
    pub const CUSTOM_RULE_CATEGORY_HAPPINESS: CustomRuleCategory = CustomRuleCategory(1002);
    pub const CUSTOM_RULE_CATEGORY_TRADE_PORTALS: CustomRuleCategory = CustomRuleCategory(1003);
    pub const CUSTOM_RULE_CATEGORY_TRADE_PORTAL_PRICES: CustomRuleCategory = CustomRuleCategory(1004);
    pub const CUSTOM_RULE_CATEGORY_TRADE_PORTAL_REQUIRE_DISCOVERY: CustomRuleCategory = CustomRuleCategory(1005);
    pub const CUSTOM_RULE_CATEGORY_NPCS: CustomRuleCategory = CustomRuleCategory(1006);
    pub const CUSTOM_RULE_CATEGORY_CLOTHING_DECAY: CustomRuleCategory = CustomRuleCategory(1007);
    pub const CUSTOM_RULE_CATEGORY_TOOL_DECAY: CustomRuleCategory = CustomRuleCategory(1008);
    pub const CUSTOM_RULE_CATEGORY_DEATH: CustomRuleCategory = CustomRuleCategory(1009);
    pub const CUSTOM_RULE_CATEGORY_DEATH_DROPS: CustomRuleCategory = CustomRuleCategory(1010);
    pub const CUSTOM_RULE_CATEGORY_MAX_BLOCKHEADS: CustomRuleCategory = CustomRuleCategory(1011);
    pub const CUSTOM_RULE_CATEGORY_DAY_NIGHT_CYCLE: CustomRuleCategory = CustomRuleCategory(1012);
    pub const CUSTOM_RULE_CATEGORY_SUN_COLOR: CustomRuleCategory = CustomRuleCategory(1013);
    pub const CUSTOM_RULE_CATEGORY_WORLD_MODIFICATION: CustomRuleCategory = CustomRuleCategory(1014);
    pub const CUSTOM_RULE_CATEGORY_POLE_ITEMS: CustomRuleCategory = CustomRuleCategory(1015);
    pub const CUSTOM_RULE_CATEGORY_TELEPORTATION: CustomRuleCategory = CustomRuleCategory(1016);
    pub const CUSTOM_RULE_CATEGORY_MEDITATION: CustomRuleCategory = CustomRuleCategory(1017);
    #[inline]
    pub const fn raw(self) -> u32 {
        self.0
    }
    #[inline]
    pub const fn from_raw(v: u32) -> Self {
        Self(v)
    }
    /// First-declared constant name for this value (None for unknown/unnamed values).
    pub fn name(self) -> Option<&'static str> {
        match self.0 {
            1 => Some("CUSTOM_RULE_CATEGORY_RANDOM_SEED"),
            2 => Some("CUSTOM_RULE_CATEGORY_OCEANS"),
            3 => Some("CUSTOM_RULE_CATEGORY_TEMPERATURE"),
            4 => Some("CUSTOM_RULE_CATEGORY_RAINFALL"),
            5 => Some("CUSTOM_RULE_CATEGORY_PLANTS"),
            6 => Some("CUSTOM_RULE_CATEGORY_TREES"),
            7 => Some("CUSTOM_RULE_CATEGORY_TERRAIN_TYPE"),
            8 => Some("CUSTOM_RULE_CATEGORY_CAVES"),
            9 => Some("CUSTOM_RULE_CATEGORY_LAKES"),
            10 => Some("CUSTOM_RULE_CATEGORY_SKY_ISLANDS"),
            11 => Some("CUSTOM_RULE_CATEGORY_ORE_QUANTITY"),
            12 => Some("CUSTOM_RULE_CATEGORY_ORE_DISTRIBUTION"),
            13 => Some("CUSTOM_RULE_CATEGORY_GEMS"),
            14 => Some("CUSTOM_RULE_CATEGORY_TREASURE"),
            999 => Some("CUSTOM_RULE_CATEGORY_HEALTH"),
            1000 => Some("CUSTOM_RULE_CATEGORY_HUNGER"),
            1001 => Some("CUSTOM_RULE_CATEGORY_SPAWN_ITEMS"),
            1002 => Some("CUSTOM_RULE_CATEGORY_HAPPINESS"),
            1003 => Some("CUSTOM_RULE_CATEGORY_TRADE_PORTALS"),
            1004 => Some("CUSTOM_RULE_CATEGORY_TRADE_PORTAL_PRICES"),
            1005 => Some("CUSTOM_RULE_CATEGORY_TRADE_PORTAL_REQUIRE_DISCOVERY"),
            1006 => Some("CUSTOM_RULE_CATEGORY_NPCS"),
            1007 => Some("CUSTOM_RULE_CATEGORY_CLOTHING_DECAY"),
            1008 => Some("CUSTOM_RULE_CATEGORY_TOOL_DECAY"),
            1009 => Some("CUSTOM_RULE_CATEGORY_DEATH"),
            1010 => Some("CUSTOM_RULE_CATEGORY_DEATH_DROPS"),
            1011 => Some("CUSTOM_RULE_CATEGORY_MAX_BLOCKHEADS"),
            1012 => Some("CUSTOM_RULE_CATEGORY_DAY_NIGHT_CYCLE"),
            1013 => Some("CUSTOM_RULE_CATEGORY_SUN_COLOR"),
            1014 => Some("CUSTOM_RULE_CATEGORY_WORLD_MODIFICATION"),
            1015 => Some("CUSTOM_RULE_CATEGORY_POLE_ITEMS"),
            1016 => Some("CUSTOM_RULE_CATEGORY_TELEPORTATION"),
            1017 => Some("CUSTOM_RULE_CATEGORY_MEDITATION"),
            _ => None,
        }
    }
}
impl core::fmt::Debug for CustomRuleCategory {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self.name() {
            Some(n) => write!(f, "{}", n),
            None => write!(f, "CustomRuleCategory({})", self.0),
        }
    }
}
impl From<u32> for CustomRuleCategory {
    fn from(v: u32) -> Self {
        Self(v)
    }
}
impl From<CustomRuleCategory> for u32 {
    fn from(v: CustomRuleCategory) -> u32 {
        v.0
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct InteractionObjectType(pub u32);
impl InteractionObjectType {
    pub const INTERACTION_OBJECT_TYPE_UNDEFINED: InteractionObjectType = InteractionObjectType(0);
    pub const INTERACTION_OBJECT_TYPE_WORKBENCH: InteractionObjectType = InteractionObjectType(1);
    pub const INTERACTION_OBJECT_TYPE_CHEST: InteractionObjectType = InteractionObjectType(2);
    pub const INTERACTION_OBJECT_TYPE_BED: InteractionObjectType = InteractionObjectType(3);
    pub const INTERACTION_OBJECT_TYPE_SIGN: InteractionObjectType = InteractionObjectType(4);
    pub const INTERACTION_OBJECT_TYPE_TRADING_POST: InteractionObjectType = InteractionObjectType(5);
    pub const INTERACTION_OBJECT_TYPE_TRAIN_STATION: InteractionObjectType = InteractionObjectType(6);
    pub const INTERACTION_OBJECT_TYPE_TRADE_PORTAL: InteractionObjectType = InteractionObjectType(7);
    pub const INTERACTION_OBJECT_TYPE_OWNERSHIP_SIGN: InteractionObjectType = InteractionObjectType(8);
    pub const INTERACTION_OBJECT_TYPE_MIRROR: InteractionObjectType = InteractionObjectType(9);
    #[inline]
    pub const fn raw(self) -> u32 {
        self.0
    }
    #[inline]
    pub const fn from_raw(v: u32) -> Self {
        Self(v)
    }
    /// First-declared constant name for this value (None for unknown/unnamed values).
    pub fn name(self) -> Option<&'static str> {
        match self.0 {
            0 => Some("INTERACTION_OBJECT_TYPE_UNDEFINED"),
            1 => Some("INTERACTION_OBJECT_TYPE_WORKBENCH"),
            2 => Some("INTERACTION_OBJECT_TYPE_CHEST"),
            3 => Some("INTERACTION_OBJECT_TYPE_BED"),
            4 => Some("INTERACTION_OBJECT_TYPE_SIGN"),
            5 => Some("INTERACTION_OBJECT_TYPE_TRADING_POST"),
            6 => Some("INTERACTION_OBJECT_TYPE_TRAIN_STATION"),
            7 => Some("INTERACTION_OBJECT_TYPE_TRADE_PORTAL"),
            8 => Some("INTERACTION_OBJECT_TYPE_OWNERSHIP_SIGN"),
            9 => Some("INTERACTION_OBJECT_TYPE_MIRROR"),
            _ => None,
        }
    }
}
impl core::fmt::Debug for InteractionObjectType {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self.name() {
            Some(n) => write!(f, "{}", n),
            None => write!(f, "InteractionObjectType({})", self.0),
        }
    }
}
impl From<u32> for InteractionObjectType {
    fn from(v: u32) -> Self {
        Self(v)
    }
}
impl From<InteractionObjectType> for u32 {
    fn from(v: InteractionObjectType) -> u32 {
        v.0
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct FoodType(pub u32);
impl FoodType {
    pub const FOOD_APPLE: FoodType = FoodType(0);
    pub const FOOD_MANGO: FoodType = FoodType(1);
    pub const FOOD_PRICKLY_PEAR: FoodType = FoodType(2);
    pub const FOOD_RAW_DODO_MEAT: FoodType = FoodType(3);
    pub const FOOD_COOKED_DODO_MEAT: FoodType = FoodType(4);
    pub const FOOD_COCONUT: FoodType = FoodType(5);
    pub const FOOD_ORANGE: FoodType = FoodType(6);
    pub const FOOD_FLAX_SEED: FoodType = FoodType(7);
    pub const FOOD_SUNFLOWER_SEED: FoodType = FoodType(8);
    pub const FOOD_CORN: FoodType = FoodType(9);
    pub const FOOD_CARROT: FoodType = FoodType(10);
    pub const FOOD_CHERRY: FoodType = FoodType(11);
    pub const FOOD_COFFEE_CHERRY: FoodType = FoodType(12);
    pub const FOOD_DODO_EGG: FoodType = FoodType(13);
    pub const FOOD_CHILLI: FoodType = FoodType(14);
    pub const FOOD_COFFEE: FoodType = FoodType(15);
    pub const FOOD_KELP: FoodType = FoodType(16);
    pub const FOOD_RAW_FISH: FoodType = FoodType(17);
    pub const FOOD_COOKED_FISH: FoodType = FoodType(18);
    pub const FOOD_WORM: FoodType = FoodType(19);
    pub const FOOD_LIME: FoodType = FoodType(20);
    pub const FOOD_FISH_CURRY: FoodType = FoodType(21);
    pub const FOOD_DODO_STEW: FoodType = FoodType(22);
    pub const FOOD_MAPLE_SEED: FoodType = FoodType(23);
    pub const FOOD_TOMATO: FoodType = FoodType(24);
    pub const FOOD_BREAD: FoodType = FoodType(25);
    pub const FOOD_PIZZA: FoodType = FoodType(26);
    pub const FOOD_FLATBREAD: FoodType = FoodType(27);
    pub const FOOD_MILK: FoodType = FoodType(28);
    pub const FOOD_MOZZARELLA: FoodType = FoodType(29);
    pub const FOOD_RAINBOW_CAKE: FoodType = FoodType(30);
    pub const FOOD_TYPE_COUNT: FoodType = FoodType(31);
    pub const FOOD_NONE: FoodType = FoodType(32);
    #[inline]
    pub const fn raw(self) -> u32 {
        self.0
    }
    #[inline]
    pub const fn from_raw(v: u32) -> Self {
        Self(v)
    }
    /// First-declared constant name for this value (None for unknown/unnamed values).
    pub fn name(self) -> Option<&'static str> {
        match self.0 {
            0 => Some("FOOD_APPLE"),
            1 => Some("FOOD_MANGO"),
            2 => Some("FOOD_PRICKLY_PEAR"),
            3 => Some("FOOD_RAW_DODO_MEAT"),
            4 => Some("FOOD_COOKED_DODO_MEAT"),
            5 => Some("FOOD_COCONUT"),
            6 => Some("FOOD_ORANGE"),
            7 => Some("FOOD_FLAX_SEED"),
            8 => Some("FOOD_SUNFLOWER_SEED"),
            9 => Some("FOOD_CORN"),
            10 => Some("FOOD_CARROT"),
            11 => Some("FOOD_CHERRY"),
            12 => Some("FOOD_COFFEE_CHERRY"),
            13 => Some("FOOD_DODO_EGG"),
            14 => Some("FOOD_CHILLI"),
            15 => Some("FOOD_COFFEE"),
            16 => Some("FOOD_KELP"),
            17 => Some("FOOD_RAW_FISH"),
            18 => Some("FOOD_COOKED_FISH"),
            19 => Some("FOOD_WORM"),
            20 => Some("FOOD_LIME"),
            21 => Some("FOOD_FISH_CURRY"),
            22 => Some("FOOD_DODO_STEW"),
            23 => Some("FOOD_MAPLE_SEED"),
            24 => Some("FOOD_TOMATO"),
            25 => Some("FOOD_BREAD"),
            26 => Some("FOOD_PIZZA"),
            27 => Some("FOOD_FLATBREAD"),
            28 => Some("FOOD_MILK"),
            29 => Some("FOOD_MOZZARELLA"),
            30 => Some("FOOD_RAINBOW_CAKE"),
            31 => Some("FOOD_TYPE_COUNT"),
            32 => Some("FOOD_NONE"),
            _ => None,
        }
    }
}
impl core::fmt::Debug for FoodType {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self.name() {
            Some(n) => write!(f, "{}", n),
            None => write!(f, "FoodType({})", self.0),
        }
    }
}
impl From<u32> for FoodType {
    fn from(v: u32) -> Self {
        Self(v)
    }
}
impl From<FoodType> for u32 {
    fn from(v: FoodType) -> u32 {
        v.0
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct WorkbenchType(pub u32);
impl WorkbenchType {
    pub const WORKBENCH_UNDEFINED: WorkbenchType = WorkbenchType(0);
    pub const WORKBENCH_BASIC_PORTAL: WorkbenchType = WorkbenchType(1);
    pub const WORKBENCH_WORKBENCH: WorkbenchType = WorkbenchType(2);
    pub const WORKBENCH_CAMPFIRE: WorkbenchType = WorkbenchType(3);
    pub const WORKBENCH_WEAVE: WorkbenchType = WorkbenchType(4);
    pub const WORKBENCH_WOOD: WorkbenchType = WorkbenchType(5);
    pub const WORKBENCH_TOOL: WorkbenchType = WorkbenchType(6);
    pub const WORKBENCH_PRESS: WorkbenchType = WorkbenchType(7);
    pub const WORKBENCH_KILN: WorkbenchType = WorkbenchType(8);
    pub const WORKBENCH_FURNACE: WorkbenchType = WorkbenchType(9);
    pub const WORKBENCH_CRAFT: WorkbenchType = WorkbenchType(10);
    pub const WORKBENCH_MIX: WorkbenchType = WorkbenchType(11);
    pub const WORKBENCH_DYE: WorkbenchType = WorkbenchType(12);
    pub const WORKBENCH_PLACED_PORTAL: WorkbenchType = WorkbenchType(13);
    pub const WORKBENCH_METALWORK: WorkbenchType = WorkbenchType(14);
    pub const WORKBENCH_STEAM_GENERATOR: WorkbenchType = WorkbenchType(15);
    pub const WORKBENCH_ELECTRIC_KILN: WorkbenchType = WorkbenchType(16);
    pub const WORKBENCH_ELECTRIC_FURNACE: WorkbenchType = WorkbenchType(17);
    pub const WORKBENCH_ELECTRIC_METALWORK_BENCH: WorkbenchType = WorkbenchType(18);
    pub const WORKBENCH_ELECTRIC_STOVE: WorkbenchType = WorkbenchType(19);
    pub const WORKBENCH_SOLAR_PANEL: WorkbenchType = WorkbenchType(20);
    pub const WORKBENCH_FLYWHEEL: WorkbenchType = WorkbenchType(21);
    pub const WORKBENCH_ARMOR_BENCH: WorkbenchType = WorkbenchType(22);
    pub const WORKBENCH_TRAIN_YARD: WorkbenchType = WorkbenchType(23);
    pub const WORKBENCH_EASEL: WorkbenchType = WorkbenchType(24);
    pub const WORKBENCH_BUILD: WorkbenchType = WorkbenchType(25);
    pub const WORKBENCH_REFINERY: WorkbenchType = WorkbenchType(26);
    pub const WORKBENCH_ELECTRIC_PRESS: WorkbenchType = WorkbenchType(27);
    pub const WORKBENCH_COMPOST_BIN: WorkbenchType = WorkbenchType(28);
    pub const WORKBENCH_SLUICE: WorkbenchType = WorkbenchType(29);
    pub const WORKBENCH_EGG_EXTRACTOR: WorkbenchType = WorkbenchType(30);
    pub const WORKBENCH_PIZZA_OVEN: WorkbenchType = WorkbenchType(31);
    #[inline]
    pub const fn raw(self) -> u32 {
        self.0
    }
    #[inline]
    pub const fn from_raw(v: u32) -> Self {
        Self(v)
    }
    /// First-declared constant name for this value (None for unknown/unnamed values).
    pub fn name(self) -> Option<&'static str> {
        match self.0 {
            0 => Some("WORKBENCH_UNDEFINED"),
            1 => Some("WORKBENCH_BASIC_PORTAL"),
            2 => Some("WORKBENCH_WORKBENCH"),
            3 => Some("WORKBENCH_CAMPFIRE"),
            4 => Some("WORKBENCH_WEAVE"),
            5 => Some("WORKBENCH_WOOD"),
            6 => Some("WORKBENCH_TOOL"),
            7 => Some("WORKBENCH_PRESS"),
            8 => Some("WORKBENCH_KILN"),
            9 => Some("WORKBENCH_FURNACE"),
            10 => Some("WORKBENCH_CRAFT"),
            11 => Some("WORKBENCH_MIX"),
            12 => Some("WORKBENCH_DYE"),
            13 => Some("WORKBENCH_PLACED_PORTAL"),
            14 => Some("WORKBENCH_METALWORK"),
            15 => Some("WORKBENCH_STEAM_GENERATOR"),
            16 => Some("WORKBENCH_ELECTRIC_KILN"),
            17 => Some("WORKBENCH_ELECTRIC_FURNACE"),
            18 => Some("WORKBENCH_ELECTRIC_METALWORK_BENCH"),
            19 => Some("WORKBENCH_ELECTRIC_STOVE"),
            20 => Some("WORKBENCH_SOLAR_PANEL"),
            21 => Some("WORKBENCH_FLYWHEEL"),
            22 => Some("WORKBENCH_ARMOR_BENCH"),
            23 => Some("WORKBENCH_TRAIN_YARD"),
            24 => Some("WORKBENCH_EASEL"),
            25 => Some("WORKBENCH_BUILD"),
            26 => Some("WORKBENCH_REFINERY"),
            27 => Some("WORKBENCH_ELECTRIC_PRESS"),
            28 => Some("WORKBENCH_COMPOST_BIN"),
            29 => Some("WORKBENCH_SLUICE"),
            30 => Some("WORKBENCH_EGG_EXTRACTOR"),
            31 => Some("WORKBENCH_PIZZA_OVEN"),
            _ => None,
        }
    }
}
impl core::fmt::Debug for WorkbenchType {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self.name() {
            Some(n) => write!(f, "{}", n),
            None => write!(f, "WorkbenchType({})", self.0),
        }
    }
}
impl From<u32> for WorkbenchType {
    fn from(v: u32) -> Self {
        Self(v)
    }
}
impl From<WorkbenchType> for u32 {
    fn from(v: WorkbenchType) -> u32 {
        v.0
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct ItemCategoryType(pub u32);
impl ItemCategoryType {
    pub const ITEM_CATEGORY_UNDEFINED: ItemCategoryType = ItemCategoryType(0);
    pub const ITEM_CATEGORY_BLOCK: ItemCategoryType = ItemCategoryType(1);
    pub const ITEM_CATEGORY_ORGANIC: ItemCategoryType = ItemCategoryType(2);
    pub const ITEM_CATEGORY_TOOL: ItemCategoryType = ItemCategoryType(3);
    pub const ITEM_CATEGORY_CLOTHING: ItemCategoryType = ItemCategoryType(4);
    pub const ITEM_CATEGORY_MINERAL: ItemCategoryType = ItemCategoryType(5);
    pub const ITEM_CATEGORY_CRAFTED: ItemCategoryType = ItemCategoryType(6);
    pub const ITEM_CATEGORY_TECH: ItemCategoryType = ItemCategoryType(7);
    pub const ITEM_CATEGORY_COIN: ItemCategoryType = ItemCategoryType(8);
    pub const ITEM_CATEGORY_MAX_CATEGORIES: ItemCategoryType = ItemCategoryType(9);
    #[inline]
    pub const fn raw(self) -> u32 {
        self.0
    }
    #[inline]
    pub const fn from_raw(v: u32) -> Self {
        Self(v)
    }
    /// First-declared constant name for this value (None for unknown/unnamed values).
    pub fn name(self) -> Option<&'static str> {
        match self.0 {
            0 => Some("ITEM_CATEGORY_UNDEFINED"),
            1 => Some("ITEM_CATEGORY_BLOCK"),
            2 => Some("ITEM_CATEGORY_ORGANIC"),
            3 => Some("ITEM_CATEGORY_TOOL"),
            4 => Some("ITEM_CATEGORY_CLOTHING"),
            5 => Some("ITEM_CATEGORY_MINERAL"),
            6 => Some("ITEM_CATEGORY_CRAFTED"),
            7 => Some("ITEM_CATEGORY_TECH"),
            8 => Some("ITEM_CATEGORY_COIN"),
            9 => Some("ITEM_CATEGORY_MAX_CATEGORIES"),
            _ => None,
        }
    }
}
impl core::fmt::Debug for ItemCategoryType {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self.name() {
            Some(n) => write!(f, "{}", n),
            None => write!(f, "ItemCategoryType({})", self.0),
        }
    }
}
impl From<u32> for ItemCategoryType {
    fn from(v: u32) -> Self {
        Self(v)
    }
}
impl From<ItemCategoryType> for u32 {
    fn from(v: ItemCategoryType) -> u32 {
        v.0
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct NPCType(pub u32);
impl NPCType {
    pub const NPC_NOTHING: NPCType = NPCType(0);
    pub const NPC_DODO: NPCType = NPCType(1);
    pub const NPC_DROPBEAR: NPCType = NPCType(2);
    pub const NPC_DONKEY: NPCType = NPCType(3);
    pub const NPC_CLOWNFISH: NPCType = NPCType(4);
    pub const NPC_SHARK: NPCType = NPCType(5);
    pub const NPC_CAVETROLL: NPCType = NPCType(6);
    pub const NPC_SCORPION: NPCType = NPCType(7);
    pub const NPC_YAK: NPCType = NPCType(8);
    #[inline]
    pub const fn raw(self) -> u32 {
        self.0
    }
    #[inline]
    pub const fn from_raw(v: u32) -> Self {
        Self(v)
    }
    /// First-declared constant name for this value (None for unknown/unnamed values).
    pub fn name(self) -> Option<&'static str> {
        match self.0 {
            0 => Some("NPC_NOTHING"),
            1 => Some("NPC_DODO"),
            2 => Some("NPC_DROPBEAR"),
            3 => Some("NPC_DONKEY"),
            4 => Some("NPC_CLOWNFISH"),
            5 => Some("NPC_SHARK"),
            6 => Some("NPC_CAVETROLL"),
            7 => Some("NPC_SCORPION"),
            8 => Some("NPC_YAK"),
            _ => None,
        }
    }
}
impl core::fmt::Debug for NPCType {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self.name() {
            Some(n) => write!(f, "{}", n),
            None => write!(f, "NPCType({})", self.0),
        }
    }
}
impl From<u32> for NPCType {
    fn from(v: u32) -> Self {
        Self(v)
    }
}
impl From<NPCType> for u32 {
    fn from(v: NPCType) -> u32 {
        v.0
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct NetNPCTameType(pub u32);
impl NetNPCTameType {
    pub const NPC_NOT_TAMED: NetNPCTameType = NetNPCTameType(0);
    pub const NPC_TAMED_BY_THIS_CLIENT: NetNPCTameType = NetNPCTameType(1);
    pub const NPC_TAMED_BY_OTHER_CLIENT: NetNPCTameType = NetNPCTameType(2);
    #[inline]
    pub const fn raw(self) -> u32 {
        self.0
    }
    #[inline]
    pub const fn from_raw(v: u32) -> Self {
        Self(v)
    }
    /// First-declared constant name for this value (None for unknown/unnamed values).
    pub fn name(self) -> Option<&'static str> {
        match self.0 {
            0 => Some("NPC_NOT_TAMED"),
            1 => Some("NPC_TAMED_BY_THIS_CLIENT"),
            2 => Some("NPC_TAMED_BY_OTHER_CLIENT"),
            _ => None,
        }
    }
}
impl core::fmt::Debug for NetNPCTameType {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self.name() {
            Some(n) => write!(f, "{}", n),
            None => write!(f, "NetNPCTameType({})", self.0),
        }
    }
}
impl From<u32> for NetNPCTameType {
    fn from(v: u32) -> Self {
        Self(v)
    }
}
impl From<NetNPCTameType> for u32 {
    fn from(v: NetNPCTameType) -> u32 {
        v.0
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct SendState(pub u32);
impl SendState {
    pub const SEND_STATE_NONE: SendState = SendState(0);
    pub const SEND_STATE_SEND: SendState = SendState(1);
    pub const SEND_STATE_SENT: SendState = SendState(2);
    #[inline]
    pub const fn raw(self) -> u32 {
        self.0
    }
    #[inline]
    pub const fn from_raw(v: u32) -> Self {
        Self(v)
    }
    /// First-declared constant name for this value (None for unknown/unnamed values).
    pub fn name(self) -> Option<&'static str> {
        match self.0 {
            0 => Some("SEND_STATE_NONE"),
            1 => Some("SEND_STATE_SEND"),
            2 => Some("SEND_STATE_SENT"),
            _ => None,
        }
    }
}
impl core::fmt::Debug for SendState {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self.name() {
            Some(n) => write!(f, "{}", n),
            None => write!(f, "SendState({})", self.0),
        }
    }
}
impl From<u32> for SendState {
    fn from(v: u32) -> Self {
        Self(v)
    }
}
impl From<SendState> for u32 {
    fn from(v: SendState) -> u32 {
        v.0
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct NetNPCInteractionType(pub u32);
impl NetNPCInteractionType {
    pub const NET_NPC_INTERACTION_UNDEFINED: NetNPCInteractionType = NetNPCInteractionType(0);
    pub const NET_NPC_INTERACTION_FEED: NetNPCInteractionType = NetNPCInteractionType(1);
    pub const NET_NPC_INTERACTION_SET_FREE: NetNPCInteractionType = NetNPCInteractionType(2);
    pub const NET_NPC_INTERACTION_CAPTURE: NetNPCInteractionType = NetNPCInteractionType(3);
    pub const NET_NPC_INTERACTION_HARVEST_MILK: NetNPCInteractionType = NetNPCInteractionType(4);
    pub const NET_NPC_INTERACTION_HARVEST_SHAVE: NetNPCInteractionType = NetNPCInteractionType(5);
    #[inline]
    pub const fn raw(self) -> u32 {
        self.0
    }
    #[inline]
    pub const fn from_raw(v: u32) -> Self {
        Self(v)
    }
    /// First-declared constant name for this value (None for unknown/unnamed values).
    pub fn name(self) -> Option<&'static str> {
        match self.0 {
            0 => Some("NET_NPC_INTERACTION_UNDEFINED"),
            1 => Some("NET_NPC_INTERACTION_FEED"),
            2 => Some("NET_NPC_INTERACTION_SET_FREE"),
            3 => Some("NET_NPC_INTERACTION_CAPTURE"),
            4 => Some("NET_NPC_INTERACTION_HARVEST_MILK"),
            5 => Some("NET_NPC_INTERACTION_HARVEST_SHAVE"),
            _ => None,
        }
    }
}
impl core::fmt::Debug for NetNPCInteractionType {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self.name() {
            Some(n) => write!(f, "{}", n),
            None => write!(f, "NetNPCInteractionType({})", self.0),
        }
    }
}
impl From<u32> for NetNPCInteractionType {
    fn from(v: u32) -> Self {
        Self(v)
    }
}
impl From<NetNPCInteractionType> for u32 {
    fn from(v: NetNPCInteractionType) -> u32 {
        v.0
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct PigmentColor(pub u32);
impl PigmentColor {
    pub const PIGMENT_UNDEFINED: PigmentColor = PigmentColor(0);
    pub const PIGMENT_WHITE: PigmentColor = PigmentColor(1);
    pub const PIGMENT_BLACK: PigmentColor = PigmentColor(2);
    pub const PIGMENT_RED_OCHRE: PigmentColor = PigmentColor(3);
    pub const PIGMENT_YELLOW_INDIAN: PigmentColor = PigmentColor(4);
    pub const PIGMENT_BLUE_ULTRAMARINE: PigmentColor = PigmentColor(5);
    pub const PIGMENT_EMERALD_GREEN: PigmentColor = PigmentColor(6);
    pub const PIGMENT_TYRIAN_PURPLE: PigmentColor = PigmentColor(7);
    pub const PIGMENT_BLUE_COPPER: PigmentColor = PigmentColor(8);
    pub const PIGMENT_COLOR_COUNT: PigmentColor = PigmentColor(9);
    #[inline]
    pub const fn raw(self) -> u32 {
        self.0
    }
    #[inline]
    pub const fn from_raw(v: u32) -> Self {
        Self(v)
    }
    /// First-declared constant name for this value (None for unknown/unnamed values).
    pub fn name(self) -> Option<&'static str> {
        match self.0 {
            0 => Some("PIGMENT_UNDEFINED"),
            1 => Some("PIGMENT_WHITE"),
            2 => Some("PIGMENT_BLACK"),
            3 => Some("PIGMENT_RED_OCHRE"),
            4 => Some("PIGMENT_YELLOW_INDIAN"),
            5 => Some("PIGMENT_BLUE_ULTRAMARINE"),
            6 => Some("PIGMENT_EMERALD_GREEN"),
            7 => Some("PIGMENT_TYRIAN_PURPLE"),
            8 => Some("PIGMENT_BLUE_COPPER"),
            9 => Some("PIGMENT_COLOR_COUNT"),
            _ => None,
        }
    }
}
impl core::fmt::Debug for PigmentColor {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self.name() {
            Some(n) => write!(f, "{}", n),
            None => write!(f, "PigmentColor({})", self.0),
        }
    }
}
impl From<u32> for PigmentColor {
    fn from(v: u32) -> Self {
        Self(v)
    }
}
impl From<PigmentColor> for u32 {
    fn from(v: PigmentColor) -> u32 {
        v.0
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct ParticleGravityType(pub u32);
impl ParticleGravityType {
    pub const GRAVITY_NONE: ParticleGravityType = ParticleGravityType(0);
    pub const GRAVITY_FALL: ParticleGravityType = ParticleGravityType(1);
    pub const GRAVITY_SPIRAL_UP: ParticleGravityType = ParticleGravityType(2);
    pub const GRAVITY_SPIRAL_CONNECTION: ParticleGravityType = ParticleGravityType(3);
    pub const GRAVITY_LEAF: ParticleGravityType = ParticleGravityType(4);
    pub const GRAVITY_SMOKE: ParticleGravityType = ParticleGravityType(5);
    pub const GRAVITY_LEAF_SPARKLE: ParticleGravityType = ParticleGravityType(6);
    pub const GRAVITY_LEAF_SPARKLE_ON: ParticleGravityType = ParticleGravityType(7);
    pub const GRAVITY_LEAF_WIND_ONLY: ParticleGravityType = ParticleGravityType(8);
    pub const GRAVITY_SURFACE_WIND: ParticleGravityType = ParticleGravityType(9);
    pub const GRAVITY_LEAF_SPARKLE_UNICORN: ParticleGravityType = ParticleGravityType(10);
    pub const GRAVITY_LEAF_SPARKLE_UNICORN_ON: ParticleGravityType = ParticleGravityType(11);
    #[inline]
    pub const fn raw(self) -> u32 {
        self.0
    }
    #[inline]
    pub const fn from_raw(v: u32) -> Self {
        Self(v)
    }
    /// First-declared constant name for this value (None for unknown/unnamed values).
    pub fn name(self) -> Option<&'static str> {
        match self.0 {
            0 => Some("GRAVITY_NONE"),
            1 => Some("GRAVITY_FALL"),
            2 => Some("GRAVITY_SPIRAL_UP"),
            3 => Some("GRAVITY_SPIRAL_CONNECTION"),
            4 => Some("GRAVITY_LEAF"),
            5 => Some("GRAVITY_SMOKE"),
            6 => Some("GRAVITY_LEAF_SPARKLE"),
            7 => Some("GRAVITY_LEAF_SPARKLE_ON"),
            8 => Some("GRAVITY_LEAF_WIND_ONLY"),
            9 => Some("GRAVITY_SURFACE_WIND"),
            10 => Some("GRAVITY_LEAF_SPARKLE_UNICORN"),
            11 => Some("GRAVITY_LEAF_SPARKLE_UNICORN_ON"),
            _ => None,
        }
    }
}
impl core::fmt::Debug for ParticleGravityType {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self.name() {
            Some(n) => write!(f, "{}", n),
            None => write!(f, "ParticleGravityType({})", self.0),
        }
    }
}
impl From<u32> for ParticleGravityType {
    fn from(v: u32) -> Self {
        Self(v)
    }
}
impl From<ParticleGravityType> for u32 {
    fn from(v: ParticleGravityType) -> u32 {
        v.0
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct PlantType(pub u32);
impl PlantType {
    pub const PLANT_NOTHING: PlantType = PlantType(0);
    pub const PLANT_FLAX: PlantType = PlantType(1);
    pub const PLANT_SUNFLOWER: PlantType = PlantType(2);
    pub const PLANT_CORN: PlantType = PlantType(3);
    pub const PLANT_CARROT: PlantType = PlantType(4);
    pub const PLANT_CHILLI: PlantType = PlantType(5);
    pub const PLANT_KELP: PlantType = PlantType(6);
    pub const PLANT_VINE: PlantType = PlantType(7);
    pub const PLANT_TULIP: PlantType = PlantType(8);
    pub const PLANT_WHEAT: PlantType = PlantType(9);
    pub const PLANT_TOMATO: PlantType = PlantType(10);
    #[inline]
    pub const fn raw(self) -> u32 {
        self.0
    }
    #[inline]
    pub const fn from_raw(v: u32) -> Self {
        Self(v)
    }
    /// First-declared constant name for this value (None for unknown/unnamed values).
    pub fn name(self) -> Option<&'static str> {
        match self.0 {
            0 => Some("PLANT_NOTHING"),
            1 => Some("PLANT_FLAX"),
            2 => Some("PLANT_SUNFLOWER"),
            3 => Some("PLANT_CORN"),
            4 => Some("PLANT_CARROT"),
            5 => Some("PLANT_CHILLI"),
            6 => Some("PLANT_KELP"),
            7 => Some("PLANT_VINE"),
            8 => Some("PLANT_TULIP"),
            9 => Some("PLANT_WHEAT"),
            10 => Some("PLANT_TOMATO"),
            _ => None,
        }
    }
}
impl core::fmt::Debug for PlantType {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self.name() {
            Some(n) => write!(f, "{}", n),
            None => write!(f, "PlantType({})", self.0),
        }
    }
}
impl From<u32> for PlantType {
    fn from(v: u32) -> Self {
        Self(v)
    }
}
impl From<PlantType> for u32 {
    fn from(v: PlantType) -> u32 {
        v.0
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct RailConfiguration(pub u32);
impl RailConfiguration {
    pub const RAIL_UNDEFINED: RailConfiguration = RailConfiguration(0);
    pub const RAIL_FLAT: RailConfiguration = RailConfiguration(1);
    pub const RAIL_DIAGONAL_UP_LEFT: RailConfiguration = RailConfiguration(2);
    pub const RAIL_DIAGONAL_HALF_UP_LEFT_BOT: RailConfiguration = RailConfiguration(3);
    pub const RAIL_DIAGONAL_HALF_UP_LEFT_TOP: RailConfiguration = RailConfiguration(4);
    pub const RAIL_DIAGONAL_UP_RIGHT: RailConfiguration = RailConfiguration(5);
    pub const RAIL_DIAGONAL_HALF_UP_RIGHT_BOT: RailConfiguration = RailConfiguration(6);
    pub const RAIL_DIAGONAL_HALF_UP_RIGHT_TOP: RailConfiguration = RailConfiguration(7);
    pub const RAIL_DIAGONAL_HALF_FLAT: RailConfiguration = RailConfiguration(8);
    #[inline]
    pub const fn raw(self) -> u32 {
        self.0
    }
    #[inline]
    pub const fn from_raw(v: u32) -> Self {
        Self(v)
    }
    /// First-declared constant name for this value (None for unknown/unnamed values).
    pub fn name(self) -> Option<&'static str> {
        match self.0 {
            0 => Some("RAIL_UNDEFINED"),
            1 => Some("RAIL_FLAT"),
            2 => Some("RAIL_DIAGONAL_UP_LEFT"),
            3 => Some("RAIL_DIAGONAL_HALF_UP_LEFT_BOT"),
            4 => Some("RAIL_DIAGONAL_HALF_UP_LEFT_TOP"),
            5 => Some("RAIL_DIAGONAL_UP_RIGHT"),
            6 => Some("RAIL_DIAGONAL_HALF_UP_RIGHT_BOT"),
            7 => Some("RAIL_DIAGONAL_HALF_UP_RIGHT_TOP"),
            8 => Some("RAIL_DIAGONAL_HALF_FLAT"),
            _ => None,
        }
    }
}
impl core::fmt::Debug for RailConfiguration {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self.name() {
            Some(n) => write!(f, "{}", n),
            None => write!(f, "RailConfiguration({})", self.0),
        }
    }
}
impl From<u32> for RailConfiguration {
    fn from(v: u32) -> Self {
        Self(v)
    }
}
impl From<RailConfiguration> for u32 {
    fn from(v: RailConfiguration) -> u32 {
        v.0
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct BlockRequestType(pub u32);
impl BlockRequestType {
    pub const BLOCK_REQUEST_TYPE_NONE: BlockRequestType = BlockRequestType(0);
    pub const BLOCK_REQUEST_TYPE_CREATE: BlockRequestType = BlockRequestType(1);
    pub const BLOCK_REQUEST_TYPE_DONT_CREATE: BlockRequestType = BlockRequestType(2);
    #[inline]
    pub const fn raw(self) -> u32 {
        self.0
    }
    #[inline]
    pub const fn from_raw(v: u32) -> Self {
        Self(v)
    }
    /// First-declared constant name for this value (None for unknown/unnamed values).
    pub fn name(self) -> Option<&'static str> {
        match self.0 {
            0 => Some("BLOCK_REQUEST_TYPE_NONE"),
            1 => Some("BLOCK_REQUEST_TYPE_CREATE"),
            2 => Some("BLOCK_REQUEST_TYPE_DONT_CREATE"),
            _ => None,
        }
    }
}
impl core::fmt::Debug for BlockRequestType {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self.name() {
            Some(n) => write!(f, "{}", n),
            None => write!(f, "BlockRequestType({})", self.0),
        }
    }
}
impl From<u32> for BlockRequestType {
    fn from(v: u32) -> Self {
        Self(v)
    }
}
impl From<BlockRequestType> for u32 {
    fn from(v: BlockRequestType) -> u32 {
        v.0
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct SignConnectionType(pub i64);
impl SignConnectionType {
    pub const SIGN_CONNECTION_UNDEFINED: SignConnectionType = SignConnectionType(0);
    pub const SIGN_CONNECTION_GROUND_DOUBLE: SignConnectionType = SignConnectionType(1);
    pub const SIGN_CONNECTION_GROUND_SINGLE: SignConnectionType = SignConnectionType(2);
    pub const SIGN_CONNECTION_SOLID_BLOCK: SignConnectionType = SignConnectionType(3);
    pub const SIGN_CONNECTION_SIDE_WALL: SignConnectionType = SignConnectionType(4);
    pub const SIGN_CONNECTION_HANG: SignConnectionType = SignConnectionType(5);
    #[inline]
    pub const fn raw(self) -> i64 {
        self.0
    }
    #[inline]
    pub const fn from_raw(v: i64) -> Self {
        Self(v)
    }
    /// First-declared constant name for this value (None for unknown/unnamed values).
    pub fn name(self) -> Option<&'static str> {
        match self.0 {
            0 => Some("SIGN_CONNECTION_UNDEFINED"),
            1 => Some("SIGN_CONNECTION_GROUND_DOUBLE"),
            2 => Some("SIGN_CONNECTION_GROUND_SINGLE"),
            3 => Some("SIGN_CONNECTION_SOLID_BLOCK"),
            4 => Some("SIGN_CONNECTION_SIDE_WALL"),
            5 => Some("SIGN_CONNECTION_HANG"),
            _ => None,
        }
    }
}
impl core::fmt::Debug for SignConnectionType {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self.name() {
            Some(n) => write!(f, "{}", n),
            None => write!(f, "SignConnectionType({})", self.0),
        }
    }
}
impl From<i64> for SignConnectionType {
    fn from(v: i64) -> Self {
        Self(v)
    }
}
impl From<SignConnectionType> for i64 {
    fn from(v: SignConnectionType) -> i64 {
        v.0
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct SignOffsetType(pub i64);
impl SignOffsetType {
    pub const SIGN_OFFSET_CENTERED: SignOffsetType = SignOffsetType(0);
    pub const SIGN_OFFSET_SHIFT_RIGHT: SignOffsetType = SignOffsetType(1);
    pub const SIGN_OFFSET_SHIFT_LEFT: SignOffsetType = SignOffsetType(2);
    #[inline]
    pub const fn raw(self) -> i64 {
        self.0
    }
    #[inline]
    pub const fn from_raw(v: i64) -> Self {
        Self(v)
    }
    /// First-declared constant name for this value (None for unknown/unnamed values).
    pub fn name(self) -> Option<&'static str> {
        match self.0 {
            0 => Some("SIGN_OFFSET_CENTERED"),
            1 => Some("SIGN_OFFSET_SHIFT_RIGHT"),
            2 => Some("SIGN_OFFSET_SHIFT_LEFT"),
            _ => None,
        }
    }
}
impl core::fmt::Debug for SignOffsetType {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self.name() {
            Some(n) => write!(f, "{}", n),
            None => write!(f, "SignOffsetType({})", self.0),
        }
    }
}
impl From<i64> for SignOffsetType {
    fn from(v: i64) -> Self {
        Self(v)
    }
}
impl From<SignOffsetType> for i64 {
    fn from(v: SignOffsetType) -> i64 {
        v.0
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct StairsConfiguration(pub u32);
impl StairsConfiguration {
    pub const STAIRS_UNDEFINED: StairsConfiguration = StairsConfiguration(0);
    pub const STAIRS_HIGH_RIGHT_SOLID: StairsConfiguration = StairsConfiguration(1);
    pub const STAIRS_HIGH_LEFT_SOLID: StairsConfiguration = StairsConfiguration(2);
    pub const STAIRS_HIGH_RIGHT_FLOATING: StairsConfiguration = StairsConfiguration(3);
    pub const STAIRS_HIGH_LEFT_FLOATING: StairsConfiguration = StairsConfiguration(4);
    #[inline]
    pub const fn raw(self) -> u32 {
        self.0
    }
    #[inline]
    pub const fn from_raw(v: u32) -> Self {
        Self(v)
    }
    /// First-declared constant name for this value (None for unknown/unnamed values).
    pub fn name(self) -> Option<&'static str> {
        match self.0 {
            0 => Some("STAIRS_UNDEFINED"),
            1 => Some("STAIRS_HIGH_RIGHT_SOLID"),
            2 => Some("STAIRS_HIGH_LEFT_SOLID"),
            3 => Some("STAIRS_HIGH_RIGHT_FLOATING"),
            4 => Some("STAIRS_HIGH_LEFT_FLOATING"),
            _ => None,
        }
    }
}
impl core::fmt::Debug for StairsConfiguration {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self.name() {
            Some(n) => write!(f, "{}", n),
            None => write!(f, "StairsConfiguration({})", self.0),
        }
    }
}
impl From<u32> for StairsConfiguration {
    fn from(v: u32) -> Self {
        Self(v)
    }
}
impl From<StairsConfiguration> for u32 {
    fn from(v: StairsConfiguration) -> u32 {
        v.0
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct TipType(pub u32);
impl TipType {
    pub const TIP_TYPE_NONE: TipType = TipType(0);
    pub const TIP_TYPE_COLLAPSE_EXHAUSTED: TipType = TipType(1);
    pub const TIP_TYPE_COLLAPSE_STARVATION: TipType = TipType(2);
    pub const TIP_TYPE_REFUSES_TO_WORK_EXHAUSTED: TipType = TipType(3);
    pub const TIP_TYPE_REFUSES_TO_WORK_STARVATION: TipType = TipType(4);
    pub const TIP_TYPE_NO_FUEL: TipType = TipType(5);
    pub const TIP_TYPE_NO_ELECTRICITY: TipType = TipType(6);
    pub const TIP_TYPE_SUFFER_DAMAGE: TipType = TipType(7);
    pub const TIP_TYPE_REGENERATING: TipType = TipType(8);
    pub const TIP_TYPE_DYING: TipType = TipType(9);
    #[inline]
    pub const fn raw(self) -> u32 {
        self.0
    }
    #[inline]
    pub const fn from_raw(v: u32) -> Self {
        Self(v)
    }
    /// First-declared constant name for this value (None for unknown/unnamed values).
    pub fn name(self) -> Option<&'static str> {
        match self.0 {
            0 => Some("TIP_TYPE_NONE"),
            1 => Some("TIP_TYPE_COLLAPSE_EXHAUSTED"),
            2 => Some("TIP_TYPE_COLLAPSE_STARVATION"),
            3 => Some("TIP_TYPE_REFUSES_TO_WORK_EXHAUSTED"),
            4 => Some("TIP_TYPE_REFUSES_TO_WORK_STARVATION"),
            5 => Some("TIP_TYPE_NO_FUEL"),
            6 => Some("TIP_TYPE_NO_ELECTRICITY"),
            7 => Some("TIP_TYPE_SUFFER_DAMAGE"),
            8 => Some("TIP_TYPE_REGENERATING"),
            9 => Some("TIP_TYPE_DYING"),
            _ => None,
        }
    }
}
impl core::fmt::Debug for TipType {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self.name() {
            Some(n) => write!(f, "{}", n),
            None => write!(f, "TipType({})", self.0),
        }
    }
}
impl From<u32> for TipType {
    fn from(v: u32) -> Self {
        Self(v)
    }
}
impl From<TipType> for u32 {
    fn from(v: TipType) -> u32 {
        v.0
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct TreeType(pub u32);
impl TreeType {
    pub const TREE_NOTHING: TreeType = TreeType(0);
    pub const TREE_APPLE: TreeType = TreeType(1);
    pub const TREE_MANGO: TreeType = TreeType(2);
    pub const TREE_MAPLE: TreeType = TreeType(3);
    pub const TREE_PINE: TreeType = TreeType(4);
    pub const TREE_CACTUS: TreeType = TreeType(5);
    pub const TREE_COCONUT: TreeType = TreeType(6);
    pub const TREE_ORANGE: TreeType = TreeType(7);
    pub const TREE_CHERRY: TreeType = TreeType(8);
    pub const TREE_COFFEE: TreeType = TreeType(9);
    pub const TREE_LIME: TreeType = TreeType(10);
    pub const TREE_GEM_AMETHYST: TreeType = TreeType(11);
    pub const TREE_GEM_SAPPHIRE: TreeType = TreeType(12);
    pub const TREE_GEM_EMERALD: TreeType = TreeType(13);
    pub const TREE_GEM_RUBY: TreeType = TreeType(14);
    pub const TREE_GEM_DIAMOND: TreeType = TreeType(15);
    #[inline]
    pub const fn raw(self) -> u32 {
        self.0
    }
    #[inline]
    pub const fn from_raw(v: u32) -> Self {
        Self(v)
    }
    /// First-declared constant name for this value (None for unknown/unnamed values).
    pub fn name(self) -> Option<&'static str> {
        match self.0 {
            0 => Some("TREE_NOTHING"),
            1 => Some("TREE_APPLE"),
            2 => Some("TREE_MANGO"),
            3 => Some("TREE_MAPLE"),
            4 => Some("TREE_PINE"),
            5 => Some("TREE_CACTUS"),
            6 => Some("TREE_COCONUT"),
            7 => Some("TREE_ORANGE"),
            8 => Some("TREE_CHERRY"),
            9 => Some("TREE_COFFEE"),
            10 => Some("TREE_LIME"),
            11 => Some("TREE_GEM_AMETHYST"),
            12 => Some("TREE_GEM_SAPPHIRE"),
            13 => Some("TREE_GEM_EMERALD"),
            14 => Some("TREE_GEM_RUBY"),
            15 => Some("TREE_GEM_DIAMOND"),
            _ => None,
        }
    }
}
impl core::fmt::Debug for TreeType {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self.name() {
            Some(n) => write!(f, "{}", n),
            None => write!(f, "TreeType({})", self.0),
        }
    }
}
impl From<u32> for TreeType {
    fn from(v: u32) -> Self {
        Self(v)
    }
}
impl From<TreeType> for u32 {
    fn from(v: TreeType) -> u32 {
        v.0
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct Vec2Axis(pub u32);
impl Vec2Axis {
    pub const VX: Vec2Axis = Vec2Axis(0);
    pub const VY: Vec2Axis = Vec2Axis(1);
    pub const VZ: Vec2Axis = Vec2Axis(2);
    pub const VW: Vec2Axis = Vec2Axis(3);
    #[inline]
    pub const fn raw(self) -> u32 {
        self.0
    }
    #[inline]
    pub const fn from_raw(v: u32) -> Self {
        Self(v)
    }
    /// First-declared constant name for this value (None for unknown/unnamed values).
    pub fn name(self) -> Option<&'static str> {
        match self.0 {
            0 => Some("VX"),
            1 => Some("VY"),
            2 => Some("VZ"),
            3 => Some("VW"),
            _ => None,
        }
    }
}
impl core::fmt::Debug for Vec2Axis {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self.name() {
            Some(n) => write!(f, "{}", n),
            None => write!(f, "Vec2Axis({})", self.0),
        }
    }
}
impl From<u32> for Vec2Axis {
    fn from(v: u32) -> Self {
        Self(v)
    }
}
impl From<Vec2Axis> for u32 {
    fn from(v: Vec2Axis) -> u32 {
        v.0
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct WireConfiguration(pub u32);
impl WireConfiguration {
    pub const WIRE_UNDEFINED: WireConfiguration = WireConfiguration(0);
    pub const WIRE_ALL_CONNECTIONS: WireConfiguration = WireConfiguration(1);
    pub const WIRE_NO_CONNECTIONS: WireConfiguration = WireConfiguration(2);
    pub const WIRE_ABOVE_BELOW_ONLY: WireConfiguration = WireConfiguration(3);
    pub const WIRE_ABOVE_BELOW_LEFT: WireConfiguration = WireConfiguration(4);
    pub const WIRE_ABOVE_BELOW_RIGHT: WireConfiguration = WireConfiguration(5);
    pub const WIRE_LEFT_RIGHT_ONLY: WireConfiguration = WireConfiguration(6);
    pub const WIRE_LEFT_RIGHT_UP: WireConfiguration = WireConfiguration(7);
    pub const WIRE_LEFT_RIGHT_DOWN: WireConfiguration = WireConfiguration(8);
    pub const WIRE_LEFT_DOWN: WireConfiguration = WireConfiguration(9);
    pub const WIRE_LEFT_UP: WireConfiguration = WireConfiguration(10);
    pub const WIRE_RIGHT_DOWN: WireConfiguration = WireConfiguration(11);
    pub const WIRE_RIGHT_UP: WireConfiguration = WireConfiguration(12);
    #[inline]
    pub const fn raw(self) -> u32 {
        self.0
    }
    #[inline]
    pub const fn from_raw(v: u32) -> Self {
        Self(v)
    }
    /// First-declared constant name for this value (None for unknown/unnamed values).
    pub fn name(self) -> Option<&'static str> {
        match self.0 {
            0 => Some("WIRE_UNDEFINED"),
            1 => Some("WIRE_ALL_CONNECTIONS"),
            2 => Some("WIRE_NO_CONNECTIONS"),
            3 => Some("WIRE_ABOVE_BELOW_ONLY"),
            4 => Some("WIRE_ABOVE_BELOW_LEFT"),
            5 => Some("WIRE_ABOVE_BELOW_RIGHT"),
            6 => Some("WIRE_LEFT_RIGHT_ONLY"),
            7 => Some("WIRE_LEFT_RIGHT_UP"),
            8 => Some("WIRE_LEFT_RIGHT_DOWN"),
            9 => Some("WIRE_LEFT_DOWN"),
            10 => Some("WIRE_LEFT_UP"),
            11 => Some("WIRE_RIGHT_DOWN"),
            12 => Some("WIRE_RIGHT_UP"),
            _ => None,
        }
    }
}
impl core::fmt::Debug for WireConfiguration {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self.name() {
            Some(n) => write!(f, "{}", n),
            None => write!(f, "WireConfiguration({})", self.0),
        }
    }
}
impl From<u32> for WireConfiguration {
    fn from(v: u32) -> Self {
        Self(v)
    }
}
impl From<WireConfiguration> for u32 {
    fn from(v: WireConfiguration) -> u32 {
        v.0
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct WireSolidConfiguration(pub u32);
impl WireSolidConfiguration {
    pub const WIRE_REQUIRES_Z_UNDEFINED: WireSolidConfiguration = WireSolidConfiguration(0);
    pub const WIRE_REQUIRES_Z_NOT_SOLID: WireSolidConfiguration = WireSolidConfiguration(1);
    pub const WIRE_REQUIRES_Z_ALL_CONNECTIONS: WireSolidConfiguration = WireSolidConfiguration(2);
    pub const WIRE_REQUIRES_Z_THIS_TILE_ONLY: WireSolidConfiguration = WireSolidConfiguration(3);
    pub const WIRE_REQUIRES_Z_ABOVE_BELOW_ONLY: WireSolidConfiguration = WireSolidConfiguration(4);
    pub const WIRE_REQUIRES_Z_ABOVE_BELOW_LEFT: WireSolidConfiguration = WireSolidConfiguration(5);
    pub const WIRE_REQUIRES_Z_ABOVE_BELOW_RIGHT: WireSolidConfiguration = WireSolidConfiguration(6);
    pub const WIRE_REQUIRES_Z_LEFT_RIGHT_ONLY: WireSolidConfiguration = WireSolidConfiguration(7);
    pub const WIRE_REQUIRES_Z_LEFT_ONLY: WireSolidConfiguration = WireSolidConfiguration(8);
    pub const WIRE_REQUIRES_Z_LEFT_RIGHT_UP: WireSolidConfiguration = WireSolidConfiguration(9);
    pub const WIRE_REQUIRES_Z_LEFT_RIGHT_DOWN: WireSolidConfiguration = WireSolidConfiguration(10);
    pub const WIRE_REQUIRES_Z_LEFT_DOWN: WireSolidConfiguration = WireSolidConfiguration(11);
    pub const WIRE_REQUIRES_Z_LEFT_UP: WireSolidConfiguration = WireSolidConfiguration(12);
    pub const WIRE_REQUIRES_Z_RIGHT_DOWN: WireSolidConfiguration = WireSolidConfiguration(13);
    pub const WIRE_REQUIRES_Z_RIGHT_UP: WireSolidConfiguration = WireSolidConfiguration(14);
    pub const WIRE_REQUIRES_Z_RIGHT_ONLY: WireSolidConfiguration = WireSolidConfiguration(15);
    pub const WIRE_REQUIRES_Z_UP_ONLY: WireSolidConfiguration = WireSolidConfiguration(16);
    pub const WIRE_REQUIRES_Z_DOWN_ONLY: WireSolidConfiguration = WireSolidConfiguration(17);
    #[inline]
    pub const fn raw(self) -> u32 {
        self.0
    }
    #[inline]
    pub const fn from_raw(v: u32) -> Self {
        Self(v)
    }
    /// First-declared constant name for this value (None for unknown/unnamed values).
    pub fn name(self) -> Option<&'static str> {
        match self.0 {
            0 => Some("WIRE_REQUIRES_Z_UNDEFINED"),
            1 => Some("WIRE_REQUIRES_Z_NOT_SOLID"),
            2 => Some("WIRE_REQUIRES_Z_ALL_CONNECTIONS"),
            3 => Some("WIRE_REQUIRES_Z_THIS_TILE_ONLY"),
            4 => Some("WIRE_REQUIRES_Z_ABOVE_BELOW_ONLY"),
            5 => Some("WIRE_REQUIRES_Z_ABOVE_BELOW_LEFT"),
            6 => Some("WIRE_REQUIRES_Z_ABOVE_BELOW_RIGHT"),
            7 => Some("WIRE_REQUIRES_Z_LEFT_RIGHT_ONLY"),
            8 => Some("WIRE_REQUIRES_Z_LEFT_ONLY"),
            9 => Some("WIRE_REQUIRES_Z_LEFT_RIGHT_UP"),
            10 => Some("WIRE_REQUIRES_Z_LEFT_RIGHT_DOWN"),
            11 => Some("WIRE_REQUIRES_Z_LEFT_DOWN"),
            12 => Some("WIRE_REQUIRES_Z_LEFT_UP"),
            13 => Some("WIRE_REQUIRES_Z_RIGHT_DOWN"),
            14 => Some("WIRE_REQUIRES_Z_RIGHT_UP"),
            15 => Some("WIRE_REQUIRES_Z_RIGHT_ONLY"),
            16 => Some("WIRE_REQUIRES_Z_UP_ONLY"),
            17 => Some("WIRE_REQUIRES_Z_DOWN_ONLY"),
            _ => None,
        }
    }
}
impl core::fmt::Debug for WireSolidConfiguration {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self.name() {
            Some(n) => write!(f, "{}", n),
            None => write!(f, "WireSolidConfiguration({})", self.0),
        }
    }
}
impl From<u32> for WireSolidConfiguration {
    fn from(v: u32) -> Self {
        Self(v)
    }
}
impl From<WireSolidConfiguration> for u32 {
    fn from(v: WireSolidConfiguration) -> u32 {
        v.0
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct IslandType(pub u32);
impl IslandType {
    pub const ISLAND_NONE: IslandType = IslandType(0);
    pub const ISLAND_AMETHYST: IslandType = IslandType(1);
    pub const ISLAND_SAPPHIRE: IslandType = IslandType(2);
    pub const ISLAND_EMERALD: IslandType = IslandType(3);
    pub const ISLAND_RUBY: IslandType = IslandType(4);
    pub const ISLAND_DIAMOND: IslandType = IslandType(5);
    pub const ISLAND_APPLE: IslandType = IslandType(6);
    pub const ISLAND_COCONUT: IslandType = IslandType(7);
    pub const ISLAND_ORANGE: IslandType = IslandType(8);
    pub const ISLAND_PINE: IslandType = IslandType(9);
    pub const ISLAND_LIME: IslandType = IslandType(10);
    pub const ISLAND_CACTUS: IslandType = IslandType(11);
    pub const ISLAND_CHERRY: IslandType = IslandType(12);
    pub const ISLAND_COFFEE: IslandType = IslandType(13);
    pub const ISLAND_MAPLE: IslandType = IslandType(14);
    pub const ISLAND_MANGO: IslandType = IslandType(15);
    #[inline]
    pub const fn raw(self) -> u32 {
        self.0
    }
    #[inline]
    pub const fn from_raw(v: u32) -> Self {
        Self(v)
    }
    /// First-declared constant name for this value (None for unknown/unnamed values).
    pub fn name(self) -> Option<&'static str> {
        match self.0 {
            0 => Some("ISLAND_NONE"),
            1 => Some("ISLAND_AMETHYST"),
            2 => Some("ISLAND_SAPPHIRE"),
            3 => Some("ISLAND_EMERALD"),
            4 => Some("ISLAND_RUBY"),
            5 => Some("ISLAND_DIAMOND"),
            6 => Some("ISLAND_APPLE"),
            7 => Some("ISLAND_COCONUT"),
            8 => Some("ISLAND_ORANGE"),
            9 => Some("ISLAND_PINE"),
            10 => Some("ISLAND_LIME"),
            11 => Some("ISLAND_CACTUS"),
            12 => Some("ISLAND_CHERRY"),
            13 => Some("ISLAND_COFFEE"),
            14 => Some("ISLAND_MAPLE"),
            15 => Some("ISLAND_MANGO"),
            _ => None,
        }
    }
}
impl core::fmt::Debug for IslandType {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self.name() {
            Some(n) => write!(f, "{}", n),
            None => write!(f, "IslandType({})", self.0),
        }
    }
}
impl From<u32> for IslandType {
    fn from(v: u32) -> Self {
        Self(v)
    }
}
impl From<IslandType> for u32 {
    fn from(v: IslandType) -> u32 {
        v.0
    }
}
