//! types_structs: exact-layout POD structs (hand-written part). Generated part: structs_gen.rs.
//! Layout source of truth:. All multi-byte fields are little-endian.
#![allow(clippy::all)]

/// Byte-exact (de)serialisation of a C struct. `write_le` writes exactly `SIZE` bytes
/// (padding fields are written from the stored value; they are zero unless a received buffer had other bytes).
/// `read_le` needs `b.len() >= SIZE` (extra bytes are ignored); use `from_slice` for a checked read.
pub trait WireStruct: Sized + Copy {
    const SIZE: usize;
    fn write_le(&self, out: &mut [u8]);
    fn read_le(b: &[u8]) -> Self;
    fn to_vec(&self) -> Vec<u8> {
        let mut v = vec![0u8; Self::SIZE];
        self.write_le(&mut v);
        v
    }
    /// append exactly SIZE bytes to `out`
    fn append_to(&self, out: &mut Vec<u8>) {
        let n = out.len();
        out.resize(n + Self::SIZE, 0);
        self.write_le(&mut out[n..]);
    }
    /// `None` when `b` is shorter than SIZE (the original would read past the buffer; callers decide what to do)
    fn from_slice(b: &[u8]) -> Option<Self> {
        if b.len() >= Self::SIZE {
            Some(Self::read_le(b))
        } else {
            None
        }
    }
}

// ---------------------------------------------------------------- Vector2 / Vector (C++ classes, not structs)

/// `class Vector2 { float _vector[2]; }` (8 bytes). **No `Default` impl on purpose**: the C++ default
/// constructor `Vector2::Vector2()` fills BOTH lanes with 1.0f. Use `cxx_new()` where the original
/// default-constructs (every `.cxx_construct` of a class with a Vector2 ivar) and `ZERO` for explicit zeros.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Vector2 {
    pub x: f32,
    pub y: f32,
}

impl Vector2 {
    pub const ZERO: Vector2 = Vector2 { x: 0.0, y: 0.0 };
    /// `Vector2::Vector2()` = (1.0, 1.0)
    pub fn cxx_new() -> Vector2 {
        Vector2 { x: 1.0, y: 1.0 }
    }
    pub fn new(x: f32, y: f32) -> Vector2 {
        Vector2 { x, y }
    }
    /// `operator*(Vector2)`: component-wise
    pub fn mul_vec(self, o: Vector2) -> Vector2 {
        Vector2 { x: self.x * o.x, y: self.y * o.y }
    }
    /// `operator*(float)`
    pub fn mul_f(self, f: f32) -> Vector2 {
        Vector2 { x: self.x * f, y: self.y * f }
    }
    /// `operator/(float)`: true division of each lane (not multiply by reciprocal)
    pub fn div_f(self, f: f32) -> Vector2 {
        Vector2 { x: self.x / f, y: self.y / f }
    }
    pub fn add(self, o: Vector2) -> Vector2 {
        Vector2 { x: self.x + o.x, y: self.y + o.y }
    }
    pub fn sub(self, o: Vector2) -> Vector2 {
        Vector2 { x: self.x - o.x, y: self.y - o.y }
    }
    /// `x*x + y*y` in f32 (mulss/addss).
    pub fn length_squared(self) -> f32 {
        self.x * self.x + self.y * self.y
    }
    /// `if !(lsq <= 0.0) sqrtf(lsq) else 0.0f`: NaN lsq gives NaN (compiled `ucomiss 0,lsq; jb` is taken when unordered).
    pub fn length(self) -> f32 {
        let l = self.length_squared();
        if !(l <= 0.0) {
            l.sqrt()
        } else {
            0.0
        }
    }
    /// `len = length(); if len == 0 (or -0) -> (0,0) else (x/len, y/len)` (f32 divss).
    pub fn normal(self) -> Vector2 {
        let len = self.length();
        if len == 0.0 {
            Vector2 { x: 0.0, y: 0.0 }
        } else {
            Vector2 { x: self.x / len, y: self.y / len }
        }
    }
}

impl WireStruct for Vector2 {
    const SIZE: usize = 8;
    fn write_le(&self, out: &mut [u8]) {
        out[0..4].copy_from_slice(&self.x.to_le_bytes());
        out[4..8].copy_from_slice(&self.y.to_le_bytes());
    }
    fn read_le(b: &[u8]) -> Self {
        Vector2 {
            x: f32::from_le_bytes(b[0..4].try_into().unwrap()),
            y: f32::from_le_bytes(b[4..8].try_into().unwrap()),
        }
    }
}

/// `class Vector { float _vector[4]; }` (16 bytes; `vec3` and `vec4` are both typedefs of it).
/// Default ctor = (1,1,1,1). The 3-float ctor sets `w = 1.0f`. **No `Default` impl**, see `Vector2`.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Vector {
    pub v: [f32; 4],
}

impl Vector {
    pub fn cxx_new() -> Vector {
        Vector { v: [1.0; 4] }
    }
    pub fn new3(x: f32, y: f32, z: f32) -> Vector {
        Vector { v: [x, y, z, 1.0] }
    }
    pub fn new4(x: f32, y: f32, z: f32, w: f32) -> Vector {
        Vector { v: [x, y, z, w] }
    }
    /// 4-lane operators (they include w)
    pub fn mul_vec(self, o: Vector) -> Vector {
        Vector { v: core::array::from_fn(|i| self.v[i] * o.v[i]) }
    }
    pub fn mul_f(self, f: f32) -> Vector {
        Vector { v: core::array::from_fn(|i| self.v[i] * f) }
    }
    pub fn add(self, o: Vector) -> Vector {
        Vector { v: core::array::from_fn(|i| self.v[i] + o.v[i]) }
    }
    pub fn sub(self, o: Vector) -> Vector {
        Vector { v: core::array::from_fn(|i| self.v[i] - o.v[i]) }
    }
    /// x*x + y*y + z*z (no w), evaluated left to right in f32.
    pub fn length_squared(self) -> f32 {
        self.v[0] * self.v[0] + self.v[1] * self.v[1] + self.v[2] * self.v[2]
    }
    /// `(float)sqrt((double)lengthSquared())`: NOTE double-precision sqrt, and NO `> 0` guard (unlike Vector2)
    pub fn length(self) -> f32 {
        (self.length_squared() as f64).sqrt() as f32
    }
    /// result starts as the default ctor (1,1,1,1); if len == 0 (or -0): xyz = 0; else xyz = v/len; **w stays 1.0**
    pub fn normal(self) -> Vector {
        let len = self.length();
        let mut r = Vector::cxx_new();
        if len == 0.0 {
            r.v[0] = 0.0;
            r.v[1] = 0.0;
            r.v[2] = 0.0;
        } else {
            r.v[0] = self.v[0] / len;
            r.v[1] = self.v[1] / len;
            r.v[2] = self.v[2] / len;
        }
        r
    }
}

impl WireStruct for Vector {
    const SIZE: usize = 16;
    fn write_le(&self, out: &mut [u8]) {
        for k in 0..4 {
            out[k * 4..k * 4 + 4].copy_from_slice(&self.v[k].to_le_bytes());
        }
    }
    fn read_le(b: &[u8]) -> Self {
        Vector { v: core::array::from_fn(|k| f32::from_le_bytes(b[k * 4..k * 4 + 4].try_into().unwrap())) }
    }
}

// ---------------------------------------------------------------- generated structs
// IntPair (= C `intpair`, 8 bytes), MJColor, MJColor8, BlockheadState (0x6c), CustomRules (0x40), BlockheadSkinOptions (0x14),
// WindowInfo, CraftableItem, ItemSaveData, TradeMission,... come from the generator.
include!("structs_gen.rs");

pub type Intpair = IntPair;
/// `makeIntpair(x, y)`.
pub fn make_intpair(x: i32, y: i32) -> IntPair {
    IntPair { x, y }
}

impl BlockheadState {
    /// Load of saveDict["state"]: `memcpy(&self.state, [d bytes], [d length])` with the length clamped to 0x6c
    /// (the original does not clamp, see Synthesis resolutions in). Bytes beyond `data.len()` keep their value.
    pub fn load_from_save_blob(&mut self, data: &[u8]) {
        let mut raw = self.to_vec();
        let n = data.len().min(Self::SIZE);
        raw[..n].copy_from_slice(&data[..n]);
        *self = Self::read_le(&raw);
    }
    /// `[NSData dataWithBytes:&state length:0x6c]`
    pub fn to_save_blob(&self) -> Vec<u8> {
        self.to_vec()
    }
}

impl CustomRules {
    /// `[unzipped getBytes:&clientRules length:min(len,0x40)]`: overwrite the first n bytes, rest keep `self`
    pub fn overlay_bytes(&mut self, data: &[u8]) {
        let mut raw = self.to_vec();
        let n = data.len().min(Self::SIZE);
        raw[..n].copy_from_slice(&data[..n]);
        *self = Self::read_le(&raw);
    }
}

#[cfg(test)]
mod hand_tests {
    use super::*;
    #[test]
    fn vector_semantics() {
        assert_eq!(Vector2::cxx_new(), Vector2::new(1.0, 1.0));
        assert_eq!(Vector2::new(3.0, 4.0).length(), 5.0);
        assert_eq!(Vector2::new(0.0, 0.0).normal(), Vector2::ZERO);
        assert!(Vector2::new(f32::NAN, 0.0).length().is_nan());
        assert!(Vector2::new(f32::NAN, 0.0).normal().x.is_nan());
        let n = Vector::new3(0.0, 3.0, 4.0).normal();
        assert_eq!(n.v, [0.0, 0.6, 0.8, 1.0]);
        assert_eq!(Vector::new3(0.0, 0.0, 0.0).normal().v, [0.0, 0.0, 0.0, 1.0]);
    }
    #[test]
    fn vector_wire_and_lanes() {
        let v2 = Vector2::new(1.5, -2.0);
        assert_eq!(v2.to_vec(), [0, 0, 0xc0, 0x3f, 0, 0, 0, 0xc0]);
        assert_eq!(Vector2::read_le(&v2.to_vec()), v2);
        assert_eq!(Vector2::from_slice(&[0; 7]), None);
        assert_eq!(Vector2::from_slice(&v2.to_vec()), Some(v2));
        let a = Vector::new4(1.0, 2.0, 3.0, 4.0);
        let b = Vector::new4(2.0, 2.0, 2.0, 2.0);
        assert_eq!(a.mul_vec(b).v, [2.0, 4.0, 6.0, 8.0]);
        assert_eq!(a.mul_f(0.5).v, [0.5, 1.0, 1.5, 2.0]);
        assert_eq!(a.add(b).v, [3.0, 4.0, 5.0, 6.0]);
        assert_eq!(a.sub(b).v, [-1.0, 0.0, 1.0, 2.0]);
        assert_eq!(a.length_squared(), 14.0);
        assert_eq!(Vector::read_le(&a.to_vec()), a);
        assert_eq!(a.to_vec().len(), 16);
    }
    #[test]
    fn blockheadstate_blob() {
        let mut s = BlockheadState::default();
        s.health = 1.0;
        s.on_trade_mission = 1;
        let b = s.to_save_blob();
        assert_eq!(b.len(), 0x6c);
        assert_eq!(&b[0x20..0x24], &1.0f32.to_le_bytes());
        assert_eq!(b[0x68], 1);
        let mut t = BlockheadState::default();
        t.load_from_save_blob(&b);
        assert_eq!(t, s);
        // longer blob: extra bytes ignored, no panic (UB-PARITY: the original overflows into later ivars)
        let mut long = b.clone();
        long.extend_from_slice(&[0xaa; 16]);
        let mut u = BlockheadState::default();
        u.load_from_save_blob(&long);
        assert_eq!(u.to_save_blob(), b);
        // shorter blob: the rest keeps its previous value
        let mut w = s;
        w.health = 7.0;
        w.load_from_save_blob(&[0u8; 4]);
        assert_eq!((w.interacting, w.health, w.on_trade_mission), (0, 7.0, 1));
    }
    #[test]
    fn custom_rules_overlay_short() {
        let mut base = CustomRules::default();
        base.sun_color = [255, 128, 64];
        base.meditation = 3;
        let before = base.to_vec();
        let mut r = base;
        r.overlay_bytes(&[1u8; 16]);
        let after = r.to_vec();
        assert_eq!(&after[..16], &[1u8; 16]);
        assert_eq!(&after[16..], &before[16..]);
        assert_eq!(after.len(), 0x40);
        assert_eq!(&after[0x36..0x39], &[255, 128, 64]);
        assert_eq!(after[0x3f], 0);
    }
    #[test]
    fn vector_edge_cases() {
        let n = Vector2::new(f32::NAN, 1.0).normal();
        assert!(n.x.is_nan() && n.y.is_nan());
        assert_eq!(Vector2::new(-0.0, 0.0).normal(), Vector2::ZERO);
        assert_eq!(Vector2::new(6.0, 8.0).div_f(2.0), Vector2::new(3.0, 4.0));
        assert_eq!(Vector::cxx_new().v, [1.0; 4]);
        assert_eq!(Vector::new4(1.0, 2.0, 2.0, 100.0).length(), 3.0); // w ignored
        let n = Vector::new3(f32::NAN, 1.0, 2.0).normal();
        assert!(n.v[0].is_nan() && n.v[1].is_nan() && n.v[2].is_nan() && n.v[3] == 1.0);
    }
}
