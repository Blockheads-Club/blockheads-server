//! Wire codec primitives: type bytes, framing, little-endian reader/writer, gzip + plist wrappers, index-set form.
//!

use crate::foundation::{gzip, indexset::SERIALIZATION_MAGIC};
use crate::plist::helpers;
use crate::plist::value::PlistValue as PV;

/// `BHNDT_*` type bytes without the prefix.
#[allow(dead_code)]
pub mod t {
    pub const UNDEFINED: u8 = 0x00;
    pub const WORLD_SUMMARY: u8 = 0x01;
    pub const CLIENT_JOIN_REQUEST: u8 = 0x02;
    pub const CLIENT_BLOCK_REQUEST: u8 = 0x03;
    pub const SERVER_BLOCK: u8 = 0x04;
    pub const CLIENT_INITIAL_DATA_REQUEST: u8 = 0x05;
    pub const SERVER_INITIAL_DATA_REPLY: u8 = 0x06;
    pub const SERVER_DYNAMIC_OBJECT_CREATE: u8 = 0x07;
    pub const SERVER_DYNAMIC_OBJECT_UPDATE: u8 = 0x08;
    pub const SERVER_DYNAMIC_OBJECT_REMOVE: u8 = 0x09;
    pub const CLIENT_DYNAMIC_OBJECT_CREATE: u8 = 0x0a;
    pub const CLIENT_DYNAMIC_OBJECT_UPDATE: u8 = 0x0b;
    pub const CLIENT_DYNAMIC_OBJECT_REMOVE: u8 = 0x0c;
    pub const CLIENT_BLOCK_REMOVAL_NOTIFICATION: u8 = 0x0d;
    pub const CLIENT_UNIQUE_ID_REQUEST: u8 = 0x0e;
    pub const SERVER_UNIQUE_ID_REPLY: u8 = 0x0f;
    pub const CLIENT_FILL_REQUEST: u8 = 0x10;
    pub const CLIENT_REMOVE_REQUEST: u8 = 0x11;
    pub const CLIENT_PICKUP_REQUEST: u8 = 0x12;
    pub const SERVER_PICKUP_REPLY: u8 = 0x13;
    pub const CLIENT_FREEBLOCK_CREATION_REQUEST: u8 = 0x14;
    pub const CLIENT_PLACE_WORKBENCH_REQUEST: u8 = 0x15;
    pub const CLIENT_PLACE_INTERACTION_OBJECT_REQUEST: u8 = 0x16;
    pub const SERVER_WORLD_HEARTBEAT: u8 = 0x17;
    pub const CLIENT_WORLD_HEARTBEAT_REQUEST: u8 = 0x18;
    pub const CLIENT_PAINT_REQUEST: u8 = 0x19;
    pub const GATHER_NOTIFICATION: u8 = 0x1a;
    pub const ZOOM_LOCATION_REQUEST: u8 = 0x1b;
    pub const ZOOM_LOCATION_RESPONSE: u8 = 0x1c;
    pub const SERVER_FILL_REPLY: u8 = 0x1d;
    pub const SERVER_PLAYER_LIST_CHANGED_NOTIFICATION: u8 = 0x1e;
    pub const CLIENT_PLAYER_INFORMATION: u8 = 0x1f;
    pub const CLIENT_BLOCKHEADS: u8 = 0x20;
    pub const CLIENT_BLOCKHEAD_INVENTORY: u8 = 0x21;
    pub const SERVER_INITIAL_BLOCKHEADS: u8 = 0x22;
    pub const SERVER_INITIAL_CONTACT: u8 = 0x23;
    pub const CHAT_MESSAGE: u8 = 0x24;
    pub const CHAT_MESSAGE_HISTORY: u8 = 0x25;
    pub const SERVER_CONNECTION_REJECTION: u8 = 0x26;
    pub const SERVER_BLOCK_NOT_LOADED: u8 = 0x27;
    pub const SERVER_LIGHT_BLOCK: u8 = 0x28;
    pub const CLIENT_PROJECTILE_REQUEST: u8 = 0x29;
    pub const CLIENT_BLOCKHEAD_DAMAGE_REQUEST: u8 = 0x2a;
    pub const SERVER_ADMIN_OR_MOD_CHANGED: u8 = 0x2b;
    pub const ELECTRICITY_PATH: u8 = 0x2c;
    pub const TIP_NOTIFICATION: u8 = 0x2d;
    pub const CLIENT_TRADE_TRANSACTION: u8 = 0x2e;
    pub const SERVER_WORLD_PRICE_OFFSET_LIST: u8 = 0x2f;
    pub const CLIENT_REQUEST_PAINTING_DATA: u8 = 0x30;
    pub const SERVER_PAINTING_DATA: u8 = 0x31;
    pub const SERVER_CHEST_INVENTORY_DATA: u8 = 0x32;
    pub const SERVER_MOD_CHANGED_OLD_NOW_UNUSED: u8 = 0x33;
    pub const CLIENT_WELCOME_MESSAGE_SET_REQUEST: u8 = 0x34;
    pub const CLIENT_WELCOME_MESSAGE_GET_REQUEST: u8 = 0x35;
    pub const SERVER_WELCOME_MESSAGE: u8 = 0x36;
    pub const CLIENT_REPORT_USER: u8 = 0x37;
    pub const CLIENT_PASSWORD_CHANGE_REQUEST: u8 = 0x38;
    pub const CLIENT_PRIVACY_CHANGE_REQUEST: u8 = 0x39;
    pub const CLIENT_REMOVE_BACK_WALL_REQUEST: u8 = 0x3a;
    pub const CLIENT_OWNERSHIP_SIGN_PLAYER_LIST_REQUEST: u8 = 0x3b;
    pub const SERVER_OWNERSHIP_SIGN_PLAYER_LIST_DATA: u8 = 0x3c;
    pub const SERVER_OWNERSHIP_SIGN_MODIFIED_NOTIFATION: u8 = 0x3d;
    pub const CLIENT_PORTAL_CHEST_TRANSACTION: u8 = 0x3e;
    pub const SERVER_PORTAL_CHEST_ACK: u8 = 0x3f;
    pub const CLIENT_PORTAL_CHEST_TRANSACTION_RESEND: u8 = 0x40;
    pub const SERVER_PORTAL_CHEST_NOT_SAVED: u8 = 0x41;
    pub const CLIENT_UPDATE_CUSTOM_RULE: u8 = 0x42;
    pub const SERVER_UPDATE_CUSTOM_RULES: u8 = 0x43;
    pub const CLIENT_WORLD_HEARTBEAT_REQUEST_WITH_CUSTOM_RULES: u8 = 0x44;
    pub const CLIENT_DYNAMIC_OBJECT_CREATION_DATA_UPDATE: u8 = 0x45;
    pub const SERVER_DYNAMIC_OBJECT_CREATION_DATA_UPDATE: u8 = 0x46;
    pub const SERVER_CLEAR_CHAT: u8 = 0x47;
    pub const CLIENT_FOUND_ITEM_LIST_CHANGED_OLD: u8 = 0x48;
    pub const CLIENT_DYNAMIC_OBJECT_REMOVAL_REQUEST: u8 = 0x49;
    pub const SERVER_TOGGLE_REPAIR_MODE: u8 = 0x4a;
    pub const CLIENT_REPAIR_TILE: u8 = 0x4b;
    pub const CLIENT_FOUND_ITEM_LIST_CHANGED_V2: u8 = 0x4c;
}

/// Rejection reasons (second byte of 0x26).
pub mod reject {
    pub const USERNAME_TAKEN: u8 = 1;
    pub const USER_LOGGED_IN: u8 = 2;
    pub const CLIENT_TOO_OLD: u8 = 3;
    pub const CLIENT_TOO_NEW: u8 = 4;
    pub const SERVER_FULL: u8 = 5;
    pub const BLACKLISTED: u8 = 6;
    pub const NOT_WHITELISTED: u8 = 7;
    pub const DEVICE_ALREADY_IN_USE_WITH_DIFFERENT_NAME: u8 = 8;
    pub const INCORRECT_CLOUD_KEY: u8 = 9;
    pub const INCORRECT_PASSWORD: u8 = 10;
    pub const KICKED: u8 = 11;
    pub const BANNED: u8 = 12;
    pub const NO_CREDIT: u8 = 13;
    pub const INVALID_USERNAME: u8 = 14;
    pub const STILL_LOADING: u8 = 15;
}

/// What the original does with a short or garbage payload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Malformed {
    /// Original raised an ObjC exception inside pollNetEvents (caller does `ns_raise(name, reason)`).
    Exception { name: &'static str, reason: String },
    /// Original crashed or read garbage; Rust continues with zero-filled fields (UB-PARITY, section 7).
    Ub(&'static str),
    /// Packet ignored without effect.
    Ignore,
}
/// Result of a wire decode.
pub type WireResult<T> = Result<T, Malformed>;

impl Malformed {
    /// `NSMallocException` / "Default zone has run out of memory" (negative `len-k` NSData).
    pub fn malloc() -> Self {
        Malformed::Exception { name: "NSMallocException", reason: "Default zone has run out of memory".to_string() }
    }
    /// `NSRangeException` / "Range location + length too great" (`subdataWithRange:`).
    pub fn range_too_great() -> Self {
        Malformed::Exception { name: "NSRangeException", reason: "Range location + length too great".to_string() }
    }
    /// `NSRangeException` of `getBytes:range:{0,8}` with the payload length `n`.
    pub fn get_bytes_range8(n: usize) -> Self {
        Malformed::Exception {
            name: "NSRangeException",
            reason: format!("in getBytes:range:, range {{ 0, 8 }} extends beyond size ({n})"),
        }
    }
}

/// Split a packet into (type byte, payload). `None` for an empty packet.
pub fn split_type(packet: &[u8]) -> Option<(u8, &[u8])> {
    packet.split_first().map(|(t, r)| (*t, r))
}
/// `[ty] ++ payload`.
pub fn frame(ty: u8, payload: &[u8]) -> Vec<u8> {
    let mut v = Vec::with_capacity(1 + payload.len());
    v.push(ty);
    v.extend_from_slice(payload);
    v
}
/// `[ty][b1] ++ rest`.
pub fn frame2(ty: u8, b1: u8, rest: &[u8]) -> Vec<u8> {
    let mut v = Vec::with_capacity(2 + rest.len());
    v.push(ty);
    v.push(b1);
    v.extend_from_slice(rest);
    v
}

/// Little-endian cursor. Reads past the end return `None`; `*_or0` variants zero-fill
/// (`getBytes:length:` clamp / UB-PARITY).
#[derive(Clone, Debug)]
pub struct Rd<'a> {
    buf: &'a [u8],
    pos: usize,
}
macro_rules! rd_num {
    ($name:ident, $or0:ident, $t:ty, $n:expr) => {
        #[doc = concat!("Read a little-endian `", stringify!($t), "`; `None` if short (cursor unchanged).")]
        pub fn $name(&mut self) -> Option<$t> {
            let b = self.bytes($n)?;
            Some(<$t>::from_le_bytes(b.try_into().unwrap()))
        }
        #[doc = concat!("Zero-filling `", stringify!($t), "` read (missing bytes are 0); consumes what is there.")]
        pub fn $or0(&mut self) -> $t {
            let avail = self.remaining().min($n);
            let mut a = [0u8; $n];
            a[..avail].copy_from_slice(&self.buf[self.pos..self.pos + avail]);
            self.pos += avail;
            <$t>::from_le_bytes(a)
        }
    };
}
impl<'a> Rd<'a> {
    pub fn new(b: &'a [u8]) -> Self {
        Rd { buf: b, pos: 0 }
    }
    rd_num!(u8, u8_or0, u8, 1);
    rd_num!(u16, u16_or0, u16, 2);
    rd_num!(u32, u32_or0, u32, 4);
    rd_num!(i32, i32_or0, i32, 4);
    rd_num!(u64, u64_or0, u64, 8);
    rd_num!(f32, f32_or0, f32, 4);
    /// Read `n` bytes; `None` if fewer remain.
    pub fn bytes(&mut self, n: usize) -> Option<&'a [u8]> {
        if self.remaining() < n {
            return None;
        }
        let s = &self.buf[self.pos..self.pos + n];
        self.pos += n;
        Some(s)
    }
    /// Unread tail.
    pub fn rest(&self) -> &'a [u8] {
        &self.buf[self.pos.min(self.buf.len())..]
    }
    pub fn remaining(&self) -> usize {
        self.buf.len().saturating_sub(self.pos)
    }
    /// Skip up to `n` bytes.
    pub fn skip(&mut self, n: usize) {
        self.pos = (self.pos + n).min(self.buf.len());
    }
}

/// Little-endian builder.
#[derive(Clone, Debug, Default)]
pub struct Wr(pub Vec<u8>);
impl Wr {
    pub fn new() -> Self {
        Wr(Vec::new())
    }
    pub fn with_type(ty: u8) -> Self {
        Wr(vec![ty])
    }
    pub fn u8(&mut self, v: u8) -> &mut Self {
        self.0.push(v);
        self
    }
    pub fn u16(&mut self, v: u16) -> &mut Self {
        self.0.extend_from_slice(&v.to_le_bytes());
        self
    }
    pub fn u32(&mut self, v: u32) -> &mut Self {
        self.0.extend_from_slice(&v.to_le_bytes());
        self
    }
    pub fn i32(&mut self, v: i32) -> &mut Self {
        self.0.extend_from_slice(&v.to_le_bytes());
        self
    }
    pub fn u64(&mut self, v: u64) -> &mut Self {
        self.0.extend_from_slice(&v.to_le_bytes());
        self
    }
    pub fn f32(&mut self, v: f32) -> &mut Self {
        self.0.extend_from_slice(&v.to_le_bytes());
        self
    }
    pub fn bytes(&mut self, b: &[u8]) -> &mut Self {
        self.0.extend_from_slice(b);
        self
    }
    pub fn zeros(&mut self, n: usize) -> &mut Self {
        self.0.resize(self.0.len() + n, 0);
        self
    }
    pub fn finish(self) -> Vec<u8> {
        self.0
    }
}

/// `NSData gzipDeflate` (empty -> empty).
pub fn gz(data: &[u8]) -> Vec<u8> {
    gzip::gzip_deflate(data)
}
/// `NSData gzipInflate` (empty -> `Some(empty)`, corrupt -> `None`).
pub fn gunz(data: &[u8]) -> Option<Vec<u8>> {
    gzip::gzip_inflate(data)
}
/// `dataFromPropertyList` (GNUstep XML).
pub fn plist_xml(v: &PV) -> Option<Vec<u8>> {
    helpers::data_from_property_list(v)
}
/// `gzipDeflate(dataFromPropertyList(v))`.
pub fn plist_gz(v: &PV) -> Option<Vec<u8>> {
    plist_xml(v).map(|x| gz(&x))
}
/// `propertyListFromData`.
pub fn plist_from(data: &[u8]) -> Option<PV> {
    helpers::property_list_from_data(data)
}
/// `propertyListFromDataWithOptions(data, mutable)` (chat, resetList).
pub fn plist_from_mutable(data: &[u8]) -> Option<PV> {
    helpers::property_list_from_data_with_options(data, 1)
}
/// `propertyListFromData(gzipInflate(data))`; inflate failure is a nil plist.
pub fn gz_plist_from(data: &[u8]) -> Option<PV> {
    plist_from(&gunz(data)?)
}

/// `NSIndexSet(SerializationMethods)` decode: LE u32 words `[0x171e8f, (location,length)*]`.
/// `len > 4 && w0 == magic` else empty.
pub fn index_set_decode(data: &[u8]) -> Vec<(u32, u32)> {
    if data.len() <= 4 {
        return Vec::new();
    }
    let mut r = Rd::new(data);
    if r.u32_or0() != SERIALIZATION_MAGIC {
        return Vec::new();
    }
    let mut out = Vec::new();
    while r.remaining() >= 8 {
        let loc = r.u32_or0();
        let len = r.u32_or0();
        out.push((loc, len));
    }
    out
}
/// Inverse of [`index_set_decode`].
pub fn index_set_encode(ranges: &[(u32, u32)]) -> Vec<u8> {
    let mut w = Wr::new();
    w.u32(SERIALIZATION_MAGIC);
    for &(l, n) in ranges {
        w.u32(l).u32(n);
    }
    w.finish()
}
