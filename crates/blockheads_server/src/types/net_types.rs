// @generated layout table (wire/save format). DO NOT EDIT.
#![allow(non_upper_case_globals, dead_code)]

#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct BhPlayerState(pub u32);
impl BhPlayerState {
    pub const BHPlayerStateUnknown: BhPlayerState = BhPlayerState(0);
    pub const BHPlayerStateConnected: BhPlayerState = BhPlayerState(1);
    pub const BHPlayerStateDisconnected: BhPlayerState = BhPlayerState(2);
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
            0 => Some("BHPlayerStateUnknown"),
            1 => Some("BHPlayerStateConnected"),
            2 => Some("BHPlayerStateDisconnected"),
            _ => None,
        }
    }
}
impl core::fmt::Debug for BhPlayerState {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self.name() {
            Some(n) => write!(f, "{}", n),
            None => write!(f, "BhPlayerState({})", self.0),
        }
    }
}
impl From<u32> for BhPlayerState {
    fn from(v: u32) -> Self {
        Self(v)
    }
}
impl From<BhPlayerState> for u32 {
    fn from(v: BhPlayerState) -> u32 {
        v.0
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct BhMatchSendData(pub u32);
impl BhMatchSendData {
    pub const BHMatchSendDataReliable: BhMatchSendData = BhMatchSendData(0);
    pub const BHMatchSendDataUnreliable: BhMatchSendData = BhMatchSendData(1);
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
            0 => Some("BHMatchSendDataReliable"),
            1 => Some("BHMatchSendDataUnreliable"),
            _ => None,
        }
    }
}
impl core::fmt::Debug for BhMatchSendData {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self.name() {
            Some(n) => write!(f, "{}", n),
            None => write!(f, "BhMatchSendData({})", self.0),
        }
    }
}
impl From<u32> for BhMatchSendData {
    fn from(v: u32) -> Self {
        Self(v)
    }
}
impl From<BhMatchSendData> for u32 {
    fn from(v: BhMatchSendData) -> u32 {
        v.0
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct BhNetPrivacy(pub u64);
impl BhNetPrivacy {
    pub const BHNetPrivacyUndefined: BhNetPrivacy = BhNetPrivacy(0);
    pub const BHNetPrivacyPublic: BhNetPrivacy = BhNetPrivacy(1);
    pub const BHNetPrivacySearchable: BhNetPrivacy = BhNetPrivacy(2);
    pub const BHNetPrivacyPrivate: BhNetPrivacy = BhNetPrivacy(3);
    #[inline]
    pub const fn raw(self) -> u64 {
        self.0
    }
    #[inline]
    pub const fn from_raw(v: u64) -> Self {
        Self(v)
    }
    /// First-declared constant name for this value (None for unknown/unnamed values).
    pub fn name(self) -> Option<&'static str> {
        match self.0 {
            0 => Some("BHNetPrivacyUndefined"),
            1 => Some("BHNetPrivacyPublic"),
            2 => Some("BHNetPrivacySearchable"),
            3 => Some("BHNetPrivacyPrivate"),
            _ => None,
        }
    }
}
impl core::fmt::Debug for BhNetPrivacy {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self.name() {
            Some(n) => write!(f, "{}", n),
            None => write!(f, "BhNetPrivacy({})", self.0),
        }
    }
}
impl From<u64> for BhNetPrivacy {
    fn from(v: u64) -> Self {
        Self(v)
    }
}
impl From<BhNetPrivacy> for u64 {
    fn from(v: BhNetPrivacy) -> u64 {
        v.0
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct NetDataType(pub u32);
impl NetDataType {
    pub const BHNDT_UNDEFINED: NetDataType = NetDataType(0);
    pub const BHNDT_WORLD_SUMMARY: NetDataType = NetDataType(1);
    pub const BHNDT_CLIENT_JOIN_REQUEST: NetDataType = NetDataType(2);
    pub const BHNDT_CLIENT_BLOCK_REQUEST: NetDataType = NetDataType(3);
    pub const BHNDT_SERVER_BLOCK: NetDataType = NetDataType(4);
    pub const BHNDT_CLIENT_INITIAL_DATA_REQUEST: NetDataType = NetDataType(5);
    pub const BHNDT_SERVER_INITIAL_DATA_REPLY: NetDataType = NetDataType(6);
    pub const BHNDT_SERVER_DYNAMIC_OBJECT_CREATE: NetDataType = NetDataType(7);
    pub const BHNDT_SERVER_DYNAMIC_OBJECT_UPDATE: NetDataType = NetDataType(8);
    pub const BHNDT_SERVER_DYNAMIC_OBJECT_REMOVE: NetDataType = NetDataType(9);
    pub const BHNDT_CLIENT_DYNAMIC_OBJECT_CREATE: NetDataType = NetDataType(10);
    pub const BHNDT_CLIENT_DYNAMIC_OBJECT_UPDATE: NetDataType = NetDataType(11);
    pub const BHNDT_CLIENT_DYNAMIC_OBJECT_REMOVE: NetDataType = NetDataType(12);
    pub const BHNDT_CLIENT_BLOCK_REMOVAL_NOTIFICATION: NetDataType = NetDataType(13);
    pub const BHNDT_CLIENT_UNIQUE_ID_REQUEST: NetDataType = NetDataType(14);
    pub const BHNDT_SERVER_UNIQUE_ID_REPLY: NetDataType = NetDataType(15);
    pub const BHNDT_CLIENT_FILL_REQUEST: NetDataType = NetDataType(16);
    pub const BHNDT_CLIENT_REMOVE_REQUEST: NetDataType = NetDataType(17);
    pub const BHNDT_CLIENT_PICKUP_REQUEST: NetDataType = NetDataType(18);
    pub const BHNDT_SERVER_PICKUP_REPLY: NetDataType = NetDataType(19);
    pub const BHNDT_CLIENT_FREEBLOCK_CREATION_REQUEST: NetDataType = NetDataType(20);
    pub const BHNDT_CLIENT_PLACE_WORKBENCH_REQUEST: NetDataType = NetDataType(21);
    pub const BHNDT_CLIENT_PLACE_INTERACTION_OBJECT_REQUEST: NetDataType = NetDataType(22);
    pub const BHNDT_SERVER_WORLD_HEARTBEAT: NetDataType = NetDataType(23);
    pub const BHNDT_CLIENT_WORLD_HEARTBEAT_REQUEST: NetDataType = NetDataType(24);
    pub const BHNDT_CLIENT_PAINT_REQUEST: NetDataType = NetDataType(25);
    pub const BHNDT_GATHER_NOTIFICATION: NetDataType = NetDataType(26);
    pub const BHNDT_ZOOM_LOCATION_REQUEST: NetDataType = NetDataType(27);
    pub const BHNDT_ZOOM_LOCATION_RESPONSE: NetDataType = NetDataType(28);
    pub const BHNDT_SERVER_FILL_REPLY: NetDataType = NetDataType(29);
    pub const BHNDT_SERVER_PLAYER_LIST_CHANGED_NOTIFICATION: NetDataType = NetDataType(30);
    pub const BHNDT_CLIENT_PLAYER_INFORMATION: NetDataType = NetDataType(31);
    pub const BHNDT_CLIENT_BLOCKHEADS: NetDataType = NetDataType(32);
    pub const BHNDT_CLIENT_BLOCKHEAD_INVENTORY: NetDataType = NetDataType(33);
    pub const BHNDT_SERVER_INITIAL_BLOCKHEADS: NetDataType = NetDataType(34);
    pub const BHNDT_SERVER_INITIAL_CONTACT: NetDataType = NetDataType(35);
    pub const BHNDT_CHAT_MESSAGE: NetDataType = NetDataType(36);
    pub const BHNDT_CHAT_MESSAGE_HISTORY: NetDataType = NetDataType(37);
    pub const BHNDT_SERVER_CONNECTION_REJECTION: NetDataType = NetDataType(38);
    pub const BHNDT_SERVER_BLOCK_NOT_LOADED: NetDataType = NetDataType(39);
    pub const BHNDT_SERVER_LIGHT_BLOCK: NetDataType = NetDataType(40);
    pub const BHNDT_CLIENT_PROJECTILE_REQUEST: NetDataType = NetDataType(41);
    pub const BHNDT_CLIENT_BLOCKHEAD_DAMAGE_REQUEST: NetDataType = NetDataType(42);
    pub const BHNDT_SERVER_ADMIN_OR_MOD_CHANGED: NetDataType = NetDataType(43);
    pub const BHNDT_ELECTRICITY_PATH: NetDataType = NetDataType(44);
    pub const BHNDT_TIP_NOTIFICATION: NetDataType = NetDataType(45);
    pub const BHNDT_CLIENT_TRADE_TRANSACTION: NetDataType = NetDataType(46);
    pub const BHNDT_SERVER_WORLD_PRICE_OFFSET_LIST: NetDataType = NetDataType(47);
    pub const BHNDT_CLIENT_REQUEST_PAINTING_DATA: NetDataType = NetDataType(48);
    pub const BHNDT_SERVER_PAINTING_DATA: NetDataType = NetDataType(49);
    pub const BHNDT_SERVER_CHEST_INVENTORY_DATA: NetDataType = NetDataType(50);
    pub const BHNDT_SERVER_MOD_CHANGED_OLD_NOW_UNUSED: NetDataType = NetDataType(51);
    pub const BHNDT_CLIENT_WELCOME_MESSAGE_SET_REQUEST: NetDataType = NetDataType(52);
    pub const BHNDT_CLIENT_WELCOME_MESSAGE_GET_REQUEST: NetDataType = NetDataType(53);
    pub const BHNDT_SERVER_WELCOME_MESSAGE: NetDataType = NetDataType(54);
    pub const BHNDT_CLIENT_REPORT_USER: NetDataType = NetDataType(55);
    pub const BHNDT_CLIENT_PASSWORD_CHANGE_REQUEST: NetDataType = NetDataType(56);
    pub const BHNDT_CLIENT_PRIVACY_CHANGE_REQUEST: NetDataType = NetDataType(57);
    pub const BHNDT_CLIENT_REMOVE_BACK_WALL_REQUEST: NetDataType = NetDataType(58);
    pub const BHNDT_CLIENT_OWNERSHIP_SIGN_PLAYER_LIST_REQUEST: NetDataType = NetDataType(59);
    pub const BHNDT_SERVER_OWNERSHIP_SIGN_PLAYER_LIST_DATA: NetDataType = NetDataType(60);
    pub const BHNDT_SERVER_OWNERSHIP_SIGN_MODIFIED_NOTIFATION: NetDataType = NetDataType(61);
    pub const BHNDT_CLIENT_PORTAL_CHEST_TRANSACTION: NetDataType = NetDataType(62);
    pub const BHNDT_SERVER_PORTAL_CHEST_ACK: NetDataType = NetDataType(63);
    pub const BHNDT_CLIENT_PORTAL_CHEST_TRANSACTION_RESEND: NetDataType = NetDataType(64);
    pub const BHNDT_SERVER_PORTAL_CHEST_NOT_SAVED: NetDataType = NetDataType(65);
    pub const BHNDT_CLIENT_UPDATE_CUSTOM_RULE: NetDataType = NetDataType(66);
    pub const BHNDT_SERVER_UPDATE_CUSTOM_RULES: NetDataType = NetDataType(67);
    pub const BHNDT_CLIENT_WORLD_HEARTBEAT_REQUEST_WITH_CUSTOM_RULES: NetDataType = NetDataType(68);
    pub const BHNDT_CLIENT_DYNAMIC_OBJECT_CREATION_DATA_UPDATE: NetDataType = NetDataType(69);
    pub const BHNDT_SERVER_DYNAMIC_OBJECT_CREATION_DATA_UPDATE: NetDataType = NetDataType(70);
    pub const BHNDT_SERVER_CLEAR_CHAT: NetDataType = NetDataType(71);
    pub const BHNDT_CLIENT_FOUND_ITEM_LIST_CHANGED_OLD: NetDataType = NetDataType(72);
    pub const BHNDT_CLIENT_DYNAMIC_OBJECT_REMOVAL_REQUEST: NetDataType = NetDataType(73);
    pub const BHNDT_SERVER_TOGGLE_REPAIR_MODE: NetDataType = NetDataType(74);
    pub const BHNDT_CLIENT_REPAIR_TILE: NetDataType = NetDataType(75);
    pub const BHNDT_CLIENT_FOUND_ITEM_LIST_CHANGED_V2: NetDataType = NetDataType(76);
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
            0 => Some("BHNDT_UNDEFINED"),
            1 => Some("BHNDT_WORLD_SUMMARY"),
            2 => Some("BHNDT_CLIENT_JOIN_REQUEST"),
            3 => Some("BHNDT_CLIENT_BLOCK_REQUEST"),
            4 => Some("BHNDT_SERVER_BLOCK"),
            5 => Some("BHNDT_CLIENT_INITIAL_DATA_REQUEST"),
            6 => Some("BHNDT_SERVER_INITIAL_DATA_REPLY"),
            7 => Some("BHNDT_SERVER_DYNAMIC_OBJECT_CREATE"),
            8 => Some("BHNDT_SERVER_DYNAMIC_OBJECT_UPDATE"),
            9 => Some("BHNDT_SERVER_DYNAMIC_OBJECT_REMOVE"),
            10 => Some("BHNDT_CLIENT_DYNAMIC_OBJECT_CREATE"),
            11 => Some("BHNDT_CLIENT_DYNAMIC_OBJECT_UPDATE"),
            12 => Some("BHNDT_CLIENT_DYNAMIC_OBJECT_REMOVE"),
            13 => Some("BHNDT_CLIENT_BLOCK_REMOVAL_NOTIFICATION"),
            14 => Some("BHNDT_CLIENT_UNIQUE_ID_REQUEST"),
            15 => Some("BHNDT_SERVER_UNIQUE_ID_REPLY"),
            16 => Some("BHNDT_CLIENT_FILL_REQUEST"),
            17 => Some("BHNDT_CLIENT_REMOVE_REQUEST"),
            18 => Some("BHNDT_CLIENT_PICKUP_REQUEST"),
            19 => Some("BHNDT_SERVER_PICKUP_REPLY"),
            20 => Some("BHNDT_CLIENT_FREEBLOCK_CREATION_REQUEST"),
            21 => Some("BHNDT_CLIENT_PLACE_WORKBENCH_REQUEST"),
            22 => Some("BHNDT_CLIENT_PLACE_INTERACTION_OBJECT_REQUEST"),
            23 => Some("BHNDT_SERVER_WORLD_HEARTBEAT"),
            24 => Some("BHNDT_CLIENT_WORLD_HEARTBEAT_REQUEST"),
            25 => Some("BHNDT_CLIENT_PAINT_REQUEST"),
            26 => Some("BHNDT_GATHER_NOTIFICATION"),
            27 => Some("BHNDT_ZOOM_LOCATION_REQUEST"),
            28 => Some("BHNDT_ZOOM_LOCATION_RESPONSE"),
            29 => Some("BHNDT_SERVER_FILL_REPLY"),
            30 => Some("BHNDT_SERVER_PLAYER_LIST_CHANGED_NOTIFICATION"),
            31 => Some("BHNDT_CLIENT_PLAYER_INFORMATION"),
            32 => Some("BHNDT_CLIENT_BLOCKHEADS"),
            33 => Some("BHNDT_CLIENT_BLOCKHEAD_INVENTORY"),
            34 => Some("BHNDT_SERVER_INITIAL_BLOCKHEADS"),
            35 => Some("BHNDT_SERVER_INITIAL_CONTACT"),
            36 => Some("BHNDT_CHAT_MESSAGE"),
            37 => Some("BHNDT_CHAT_MESSAGE_HISTORY"),
            38 => Some("BHNDT_SERVER_CONNECTION_REJECTION"),
            39 => Some("BHNDT_SERVER_BLOCK_NOT_LOADED"),
            40 => Some("BHNDT_SERVER_LIGHT_BLOCK"),
            41 => Some("BHNDT_CLIENT_PROJECTILE_REQUEST"),
            42 => Some("BHNDT_CLIENT_BLOCKHEAD_DAMAGE_REQUEST"),
            43 => Some("BHNDT_SERVER_ADMIN_OR_MOD_CHANGED"),
            44 => Some("BHNDT_ELECTRICITY_PATH"),
            45 => Some("BHNDT_TIP_NOTIFICATION"),
            46 => Some("BHNDT_CLIENT_TRADE_TRANSACTION"),
            47 => Some("BHNDT_SERVER_WORLD_PRICE_OFFSET_LIST"),
            48 => Some("BHNDT_CLIENT_REQUEST_PAINTING_DATA"),
            49 => Some("BHNDT_SERVER_PAINTING_DATA"),
            50 => Some("BHNDT_SERVER_CHEST_INVENTORY_DATA"),
            51 => Some("BHNDT_SERVER_MOD_CHANGED_OLD_NOW_UNUSED"),
            52 => Some("BHNDT_CLIENT_WELCOME_MESSAGE_SET_REQUEST"),
            53 => Some("BHNDT_CLIENT_WELCOME_MESSAGE_GET_REQUEST"),
            54 => Some("BHNDT_SERVER_WELCOME_MESSAGE"),
            55 => Some("BHNDT_CLIENT_REPORT_USER"),
            56 => Some("BHNDT_CLIENT_PASSWORD_CHANGE_REQUEST"),
            57 => Some("BHNDT_CLIENT_PRIVACY_CHANGE_REQUEST"),
            58 => Some("BHNDT_CLIENT_REMOVE_BACK_WALL_REQUEST"),
            59 => Some("BHNDT_CLIENT_OWNERSHIP_SIGN_PLAYER_LIST_REQUEST"),
            60 => Some("BHNDT_SERVER_OWNERSHIP_SIGN_PLAYER_LIST_DATA"),
            61 => Some("BHNDT_SERVER_OWNERSHIP_SIGN_MODIFIED_NOTIFATION"),
            62 => Some("BHNDT_CLIENT_PORTAL_CHEST_TRANSACTION"),
            63 => Some("BHNDT_SERVER_PORTAL_CHEST_ACK"),
            64 => Some("BHNDT_CLIENT_PORTAL_CHEST_TRANSACTION_RESEND"),
            65 => Some("BHNDT_SERVER_PORTAL_CHEST_NOT_SAVED"),
            66 => Some("BHNDT_CLIENT_UPDATE_CUSTOM_RULE"),
            67 => Some("BHNDT_SERVER_UPDATE_CUSTOM_RULES"),
            68 => Some("BHNDT_CLIENT_WORLD_HEARTBEAT_REQUEST_WITH_CUSTOM_RULES"),
            69 => Some("BHNDT_CLIENT_DYNAMIC_OBJECT_CREATION_DATA_UPDATE"),
            70 => Some("BHNDT_SERVER_DYNAMIC_OBJECT_CREATION_DATA_UPDATE"),
            71 => Some("BHNDT_SERVER_CLEAR_CHAT"),
            72 => Some("BHNDT_CLIENT_FOUND_ITEM_LIST_CHANGED_OLD"),
            73 => Some("BHNDT_CLIENT_DYNAMIC_OBJECT_REMOVAL_REQUEST"),
            74 => Some("BHNDT_SERVER_TOGGLE_REPAIR_MODE"),
            75 => Some("BHNDT_CLIENT_REPAIR_TILE"),
            76 => Some("BHNDT_CLIENT_FOUND_ITEM_LIST_CHANGED_V2"),
            _ => None,
        }
    }
}
impl core::fmt::Debug for NetDataType {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self.name() {
            Some(n) => write!(f, "{}", n),
            None => write!(f, "NetDataType({})", self.0),
        }
    }
}
impl From<u32> for NetDataType {
    fn from(v: u32) -> Self {
        Self(v)
    }
}
impl From<NetDataType> for u32 {
    fn from(v: NetDataType) -> u32 {
        v.0
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct ServerConnectionRejection(pub u32);
impl ServerConnectionRejection {
    pub const SERVER_CONNECTION_REJECTION_UNDEFINED: ServerConnectionRejection = ServerConnectionRejection(0);
    pub const SERVER_CONNECTION_REJECTION_USERNAME_TAKEN: ServerConnectionRejection = ServerConnectionRejection(1);
    pub const SERVER_CONNECTION_REJECTION_USER_LOGGED_IN: ServerConnectionRejection = ServerConnectionRejection(2);
    pub const SERVER_CONNECTION_REJECTION_CLIENT_TOO_OLD: ServerConnectionRejection = ServerConnectionRejection(3);
    pub const SERVER_CONNECTION_REJECTION_CLIENT_TOO_NEW: ServerConnectionRejection = ServerConnectionRejection(4);
    pub const SERVER_CONNECTION_REJECTION_SERVER_FULL: ServerConnectionRejection = ServerConnectionRejection(5);
    pub const SERVER_CONNECTION_REJECTION_BLACKLISTED: ServerConnectionRejection = ServerConnectionRejection(6);
    pub const SERVER_CONNECTION_REJECTION_NOT_WHITELISTED: ServerConnectionRejection = ServerConnectionRejection(7);
    pub const SERVER_CONNECTION_REJECTION_DEVICE_ALREADY_IN_USE_WITH_DIFFERENT_NAME: ServerConnectionRejection =
        ServerConnectionRejection(8);
    pub const SERVER_CONNECTION_REJECTION_INCORRECT_CLOUD_KEY: ServerConnectionRejection = ServerConnectionRejection(9);
    pub const SERVER_CONNECTION_REJECTION_INCORRECT_PASSWORD: ServerConnectionRejection = ServerConnectionRejection(10);
    pub const SERVER_CONNECTION_REJECTION_KICKED: ServerConnectionRejection = ServerConnectionRejection(11);
    pub const SERVER_CONNECTION_REJECTION_BANNED: ServerConnectionRejection = ServerConnectionRejection(12);
    pub const SERVER_CONNECTION_REJECTION_NO_CREDIT: ServerConnectionRejection = ServerConnectionRejection(13);
    pub const SERVER_CONNECTION_REJECTION_INVALID_USERNAME: ServerConnectionRejection = ServerConnectionRejection(14);
    pub const SERVER_CONNECTION_REJECTION_STILL_LOADING: ServerConnectionRejection = ServerConnectionRejection(15);
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
            0 => Some("SERVER_CONNECTION_REJECTION_UNDEFINED"),
            1 => Some("SERVER_CONNECTION_REJECTION_USERNAME_TAKEN"),
            2 => Some("SERVER_CONNECTION_REJECTION_USER_LOGGED_IN"),
            3 => Some("SERVER_CONNECTION_REJECTION_CLIENT_TOO_OLD"),
            4 => Some("SERVER_CONNECTION_REJECTION_CLIENT_TOO_NEW"),
            5 => Some("SERVER_CONNECTION_REJECTION_SERVER_FULL"),
            6 => Some("SERVER_CONNECTION_REJECTION_BLACKLISTED"),
            7 => Some("SERVER_CONNECTION_REJECTION_NOT_WHITELISTED"),
            8 => Some("SERVER_CONNECTION_REJECTION_DEVICE_ALREADY_IN_USE_WITH_DIFFERENT_NAME"),
            9 => Some("SERVER_CONNECTION_REJECTION_INCORRECT_CLOUD_KEY"),
            10 => Some("SERVER_CONNECTION_REJECTION_INCORRECT_PASSWORD"),
            11 => Some("SERVER_CONNECTION_REJECTION_KICKED"),
            12 => Some("SERVER_CONNECTION_REJECTION_BANNED"),
            13 => Some("SERVER_CONNECTION_REJECTION_NO_CREDIT"),
            14 => Some("SERVER_CONNECTION_REJECTION_INVALID_USERNAME"),
            15 => Some("SERVER_CONNECTION_REJECTION_STILL_LOADING"),
            _ => None,
        }
    }
}
impl core::fmt::Debug for ServerConnectionRejection {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self.name() {
            Some(n) => write!(f, "{}", n),
            None => write!(f, "ServerConnectionRejection({})", self.0),
        }
    }
}
impl From<u32> for ServerConnectionRejection {
    fn from(v: u32) -> Self {
        Self(v)
    }
}
impl From<ServerConnectionRejection> for u32 {
    fn from(v: ServerConnectionRejection) -> u32 {
        v.0
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct BhNetNodeType(pub u32);
impl BhNetNodeType {
    pub const BHNetNodeTypeLocalServer: BhNetNodeType = BhNetNodeType(0);
    pub const BHNetNodeTypeLocalClient: BhNetNodeType = BhNetNodeType(1);
    pub const BHNetNodeTypeOnlineServer: BhNetNodeType = BhNetNodeType(2);
    pub const BHNetNodeTypeOnlineClient: BhNetNodeType = BhNetNodeType(3);
    pub const BHNetNodeTypeGameCenterServer: BhNetNodeType = BhNetNodeType(4);
    pub const BHNetNodeTypeGameCenterClient: BhNetNodeType = BhNetNodeType(5);
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
            0 => Some("BHNetNodeTypeLocalServer"),
            1 => Some("BHNetNodeTypeLocalClient"),
            2 => Some("BHNetNodeTypeOnlineServer"),
            3 => Some("BHNetNodeTypeOnlineClient"),
            4 => Some("BHNetNodeTypeGameCenterServer"),
            5 => Some("BHNetNodeTypeGameCenterClient"),
            _ => None,
        }
    }
}
impl core::fmt::Debug for BhNetNodeType {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self.name() {
            Some(n) => write!(f, "{}", n),
            None => write!(f, "BhNetNodeType({})", self.0),
        }
    }
}
impl From<u32> for BhNetNodeType {
    fn from(v: u32) -> Self {
        Self(v)
    }
}
impl From<BhNetNodeType> for u32 {
    fn from(v: BhNetNodeType) -> u32 {
        v.0
    }
}
