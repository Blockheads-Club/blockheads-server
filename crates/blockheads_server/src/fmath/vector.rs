//! `Vector2`, `Vector` (vec3 padded to vec4), `intpair` and the scalar interpolation helpers.
//!
//! All arithmetic is plain f32 with a rounding after every operation (no FMA, no reassociation).

use super::libm;
use std::ops::{Add, Mul, Sub};

/// `Vector2` ("{Vector2=[2f]}"), 8 bytes.
#[derive(Clone, Copy, PartialEq, Debug)]
#[repr(C)]
pub struct Vector2 {
    pub x: f32,
    pub y: f32,
}

/// `Vector` ("vec3"/"vec4"), 16 bytes with `w`.
#[derive(Clone, Copy, PartialEq, Debug)]
#[repr(C)]
pub struct Vector {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub w: f32,
}

/// `intpair`, 8 bytes.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
#[repr(C)]
pub struct IntPair {
    pub x: i32,
    pub y: i32,
}

/// `Vector2::Vector2()` produces (1, 1), not zero (`Vector2()`).
impl Default for Vector2 {
    fn default() -> Self {
        Vector2 { x: 1.0, y: 1.0 }
    }
}

/// `Vector::Vector()` produces (1, 1, 1, 1) (`Vector()`).
impl Default for Vector {
    fn default() -> Self {
        Vector { x: 1.0, y: 1.0, z: 1.0, w: 1.0 }
    }
}

impl Vector2 {
    /// `Vector2(x, y)`.
    #[inline]
    pub fn new(x: f32, y: f32) -> Self {
        Vector2 { x, y }
    }

    /// `Vector2::lengthSquared` = `x*x + y*y`.
    #[inline]
    pub fn length_squared(&self) -> f32 {
        self.x * self.x + self.y * self.y
    }

    /// `Vector2::length`: `s = lengthSquared(); !(s <= 0) ? sqrtf(s): 0` (f32 `sqrtf`;
    /// NaN gives NaN).
    #[inline]
    pub fn length(&self) -> f32 {
        let s = self.length_squared();
        if !(s <= 0.0) {
            libm::sqrtf(s)
        } else {
            0.0
        }
    }

    /// `Vector2::normal`: zero vector when `length() == 0`.
    #[inline]
    pub fn normal(&self) -> Vector2 {
        let len = self.length();
        if len == 0.0 {
            Vector2 { x: 0.0, y: 0.0 }
        } else {
            Vector2 { x: self.x / len, y: self.y / len }
        }
    }
}

impl Vector {
    /// `Vector(x, y, z)`: `w = 1.0`.
    #[inline]
    pub fn new(x: f32, y: f32, z: f32) -> Self {
        Vector { x, y, z, w: 1.0 }
    }

    /// `Vector(x, y, z, w)`.
    #[inline]
    pub fn new4(x: f32, y: f32, z: f32, w: f32) -> Self {
        Vector { x, y, z, w }
    }

    /// `Vector::lengthSquared` = `(x*x + y*y) + z*z` (3 lanes, `w` ignored).
    #[inline]
    pub fn length_squared(&self) -> f32 {
        (self.x * self.x + self.y * self.y) + self.z * self.z
    }

    /// `Vector::length`: `(float)sqrt((double)lengthSquared())` with the sqrt in f64.
    #[inline]
    pub fn length(&self) -> f32 {
        libm::sqrt(self.length_squared() as f64) as f32
    }

    /// `Vector::normal`: xyz zero when `length() == 0.0`, `w` is always 1.0.
    #[inline]
    pub fn normal(&self) -> Vector {
        let len = self.length();
        if len == 0.0 {
            Vector { x: 0.0, y: 0.0, z: 0.0, w: 1.0 }
        } else {
            Vector { x: self.x / len, y: self.y / len, z: self.z / len, w: 1.0 }
        }
    }
}

impl Add for Vector2 {
    type Output = Vector2;
    /// `Vector2::operator+`.
    #[inline]
    fn add(self, o: Vector2) -> Vector2 {
        Vector2 { x: self.x + o.x, y: self.y + o.y }
    }
}
impl Sub for Vector2 {
    type Output = Vector2;
    /// `Vector2::operator-`.
    #[inline]
    fn sub(self, o: Vector2) -> Vector2 {
        Vector2 { x: self.x - o.x, y: self.y - o.y }
    }
}
impl Mul<Vector2> for Vector2 {
    type Output = Vector2;
    /// `Vector2::operator*(Vector2)`: component-wise product.
    #[inline]
    fn mul(self, o: Vector2) -> Vector2 {
        Vector2 { x: self.x * o.x, y: self.y * o.y }
    }
}
impl Mul<f32> for Vector2 {
    type Output = Vector2;
    /// `Vector2::operator*(float)`.
    #[inline]
    fn mul(self, f: f32) -> Vector2 {
        Vector2 { x: self.x * f, y: self.y * f }
    }
}

impl Add for Vector {
    type Output = Vector;
    /// `Vector::operator+`, 4 lanes.
    #[inline]
    fn add(self, o: Vector) -> Vector {
        Vector { x: self.x + o.x, y: self.y + o.y, z: self.z + o.z, w: self.w + o.w }
    }
}
impl Sub for Vector {
    type Output = Vector;
    /// `Vector::operator-`, 4 lanes.
    #[inline]
    fn sub(self, o: Vector) -> Vector {
        Vector { x: self.x - o.x, y: self.y - o.y, z: self.z - o.z, w: self.w - o.w }
    }
}
impl Mul<Vector> for Vector {
    type Output = Vector;
    /// `Vector::operator*(Vector)`, 4 lanes component-wise.
    #[inline]
    fn mul(self, o: Vector) -> Vector {
        Vector { x: self.x * o.x, y: self.y * o.y, z: self.z * o.z, w: self.w * o.w }
    }
}
impl Mul<f32> for Vector {
    type Output = Vector;
    /// `Vector::operator*(float)`, 4 lanes.
    #[inline]
    fn mul(self, f: f32) -> Vector {
        Vector { x: self.x * f, y: self.y * f, z: self.z * f, w: self.w * f }
    }
}

/// `makeIntpair(x, y)`.
#[inline]
pub fn make_intpair(x: i32, y: i32) -> IntPair {
    IntPair { x, y }
}

/// `clamp`: `r = v; if max < v { r = max }; if v < min { r = min }` (both compares on
/// the original `v`, as in the unit specs /; for `min > max` a value below `min` yields `min`,
/// a value above `max` yields `max`). NaN in gives NaN out. Never `f32::clamp`.
#[inline]
pub fn clamp(value: f32, min: f32, max: f32) -> f32 {
    let mut r = value;
    if max < value {
        r = max;
    }
    if value < min {
        r = min;
    }
    r
}

/// `clampi`: same if-chain as [`clamp`] for `i32`.
#[inline]
pub fn clampi(value: i32, min: i32, max: i32) -> i32 {
    let mut r = value;
    if max < value {
        r = max;
    }
    if value < min {
        r = min;
    }
    r
}

/// `linearInterpolate` = `a + (b - a) * fraction`, f32 rounding after each operation.
#[inline]
pub fn linear_interpolate(a: f32, b: f32, fraction: f32) -> f32 {
    a + (b - a) * fraction
}

/// `linearInterpolatev` = `a + (b - a) * f` on 4 lanes.
#[inline]
pub fn linear_interpolatev(a: Vector, b: Vector, f: f32) -> Vector {
    a + (b - a) * f
}

/// `cardinalSplineInterpolate`. The evaluation order is fixed (no reassociation):
/// `h1 = ((t3 + t3) - t2*3) + 1`, `h2 = t3*(-2) + t2*3`, `h3 = (t3 - (t2 + t2)) + t`, `h4 = t3 - t2`,
/// `tan1 = (b - aa) * factor`, `tan2 = (bb - a) * factor`,
/// result `((a*h1 + b*h2) + tan1*h3) + tan2*h4`.
pub fn cardinal_spline_interpolate(aa: Vector2, a: Vector2, b: Vector2, bb: Vector2, t: f32, factor: f32) -> Vector2 {
    let t2 = t * t;
    let t3 = t2 * t;
    let h1 = ((t3 + t3) - t2 * 3.0f32) + 1.0f32;
    let h2 = t3 * -2.0f32 + t2 * 3.0f32;
    let h3 = (t3 - (t2 + t2)) + t;
    let h4 = t3 - t2;
    let tan1 = (b - aa) * factor;
    let tan2 = (bb - a) * factor;
    ((a * h1 + b * h2) + tan1 * h3) + tan2 * h4
}

/// `simpleSmoothStep(x) = x*x*(3 - 2*x)`, no clamp.
#[inline]
pub fn simple_smooth_step(x: f32) -> f32 {
    x * x * (3.0f32 - 2.0f32 * x)
}

/// `smoothStep(a, b, x)`: 0 below `a`, 1 at/after `b` (and for NaN `x`), else the
/// cubic of `t = (x - a) / (b - a)`.
#[inline]
pub fn smooth_step(a: f32, b: f32, x: f32) -> f32 {
    if x < a {
        0.0
    } else if x < b {
        let t = (x - a) / (b - a);
        t * t * (3.0f32 - 2.0f32 * t)
    } else {
        1.0
    }
}

/// `polarToRectangular(lat, lon)`: `x = (sin(lon)*cos(lat)) as f32`,
/// `y = sin(lat) as f32`, `z = (cos(lon)*cos(lat)) as f32`, `w = 1`; f64 libm calls.
pub fn polar_to_rectangular(lat: f64, lon: f64) -> Vector {
    let cos_lat = libm::cos(lat);
    let sin_lon = libm::sin(lon);
    let sin_lat = libm::sin(lat);
    let cos_lon = libm::cos(lon);
    Vector { x: (sin_lon * cos_lat) as f32, y: sin_lat as f32, z: (cos_lon * cos_lat) as f32, w: 1.0 }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_ones() {
        assert_eq!(Vector2::default(), Vector2 { x: 1.0, y: 1.0 });
        assert_eq!(Vector::default(), Vector { x: 1.0, y: 1.0, z: 1.0, w: 1.0 });
        assert_eq!(Vector::new(1.0, 2.0, 3.0).w, 1.0);
        assert_eq!(IntPair::default(), IntPair { x: 0, y: 0 });
        assert_eq!(std::mem::size_of::<Vector2>(), 8);
        assert_eq!(std::mem::size_of::<Vector>(), 16);
        assert_eq!(std::mem::size_of::<IntPair>(), 8);
    }

    #[test]
    fn vector_normal() {
        let n = Vector::new4(0.0, 0.0, 0.0, 5.0).normal();
        assert_eq!(n, Vector::new4(0.0, 0.0, 0.0, 1.0));
        let v = Vector::new4(3.0, 0.0, 4.0, 9.0);
        assert_eq!(v.length(), 5.0);
        let n = v.normal();
        assert_eq!(n.x, 3.0f32 / 5.0);
        assert_eq!(n.y, 0.0);
        assert_eq!(n.z, 4.0f32 / 5.0);
        assert_eq!(n.w, 1.0);
        let z = Vector::new(-0.0, 0.0, 0.0).normal();
        assert_eq!((z.x, z.y, z.z, z.w), (0.0, 0.0, 0.0, 1.0));
        let nn = Vector::new(f32::NAN, 1.0, 2.0).normal();
        assert!(nn.x.is_nan() && nn.y.is_nan() && nn.z.is_nan());
        assert_eq!(nn.w, 1.0);
    }

    #[test]
    fn vector2_length_and_normal() {
        assert_eq!(Vector2::new(0.0, 0.0).normal(), Vector2::new(0.0, 0.0));
        assert_eq!(Vector2::new(1e-30, 1e-30).length(), 0.0); // lengthSquared underflows to 0
        assert!(Vector2::new(f32::NAN, 1.0).length().is_nan());
        let n = Vector2::new(f32::NAN, 1.0).normal();
        assert!(n.x.is_nan() && n.y.is_nan());
        assert_eq!(Vector2::new(3.0, 4.0).length(), 5.0);
        assert_eq!(Vector2::new(3.0, 4.0).normal(), Vector2::new(0.6, 0.8));
    }

    #[test]
    fn clamps() {
        // C if-chain on the ORIGINAL value (see
        assert_eq!(clamp(5.0, 3.0, 1.0), 1.0);
        assert_eq!(clamp(0.0, 3.0, 1.0), 3.0);
        assert!(clamp(f32::NAN, 0.0, 1.0).is_nan());
        assert!(clamp(-0.0, 0.0, 1.0).is_sign_negative());
        assert_eq!(clampi(300, 0, 255), 255);
        assert_eq!(clampi(0, 1, 255), 1);
        assert_eq!(clampi(7, 5, 3), 3);
        assert_eq!(clampi(0, 5, 3), 5);
    }

    #[test]
    fn smooth_steps() {
        assert_eq!(smooth_step(0.0, 1.0, f32::NAN), 1.0);
        assert_eq!(smooth_step(1.0, 2.0, 0.5), 0.0);
        assert_eq!(smooth_step(1.0, 2.0, 2.0), 1.0);
        assert_eq!(smooth_step(0.0, 1.0, 0.5), 0.5);
        assert_eq!(simple_smooth_step(1.5), 0.0);
    }

    #[test]
    fn spline_endpoints() {
        let aa = Vector2::new(-1.0, 0.5);
        let a = Vector2::new(0.25, 3.0);
        let b = Vector2::new(2.0, -1.0);
        let bb = Vector2::new(5.0, 7.0);
        let r0 = cardinal_spline_interpolate(aa, a, b, bb, 0.0, 0.5);
        assert_eq!(r0.x.to_bits(), a.x.to_bits());
        assert_eq!(r0.y.to_bits(), a.y.to_bits());
        let z = Vector2::new(0.0, 0.0);
        let r1 = cardinal_spline_interpolate(z, a, b, z, 1.0, 0.5);
        assert_eq!(r1, b);
    }

    #[test]
    fn lerp_and_ops() {
        assert_eq!(linear_interpolate(1.0, 3.0, 0.5), 2.0);
        let a = Vector::new4(0.0, 1.0, 2.0, 3.0);
        let b = Vector::new4(10.0, 11.0, 12.0, 13.0);
        assert_eq!(linear_interpolatev(a, b, 0.5), Vector::new4(5.0, 6.0, 7.0, 8.0));
        assert_eq!(Vector2::new(1.0, 2.0) * Vector2::new(3.0, 4.0), Vector2::new(3.0, 8.0));
        assert_eq!(
            Vector::new4(1.0, 2.0, 3.0, 4.0) * Vector::new4(2.0, 2.0, 0.5, 0.0),
            Vector::new4(2.0, 4.0, 1.5, 0.0)
        );
        assert_eq!(Vector::new4(1.0, 2.0, 3.0, 4.0) * 2.0, Vector::new4(2.0, 4.0, 6.0, 8.0));
        assert_eq!(make_intpair(3, -4), IntPair { x: 3, y: -4 });
    }

    #[test]
    fn polar() {
        let v = polar_to_rectangular(0.0, 0.0);
        assert_eq!((v.x, v.y, v.z, v.w), (0.0, 0.0, 1.0, 1.0));
    }

    /// `-O0 -ffp-contract=off` in the reference image (`tools/libm_probe/vector_probe.c`).
    #[test]
    fn matches_gcc_golden() {
        let text = include_str!("../../tests/data/vector_golden.txt");
        let h = |s: &str| f32::from_bits(u32::from_str_radix(s, 16).unwrap());
        let mut counts = [0usize; 5];
        for line in text.lines() {
            let f: Vec<&str> = line.split_whitespace().collect();
            match f[0] {
                "L" => {
                    let r = linear_interpolate(h(f[1]), h(f[2]), h(f[3]));
                    assert_eq!(r.to_bits(), h(f[4]).to_bits(), "{line}");
                    counts[0] += 1;
                }
                "S" => {
                    let v = |i: usize| Vector2::new(h(f[1 + 2 * i]), h(f[2 + 2 * i]));
                    let r = cardinal_spline_interpolate(v(0), v(1), v(2), v(3), h(f[9]), h(f[10]));
                    assert_eq!((r.x.to_bits(), r.y.to_bits()), (h(f[11]).to_bits(), h(f[12]).to_bits()), "{line}");
                    counts[1] += 1;
                }
                "M" => {
                    assert_eq!(simple_smooth_step(h(f[1])).to_bits(), h(f[2]).to_bits(), "{line}");
                    counts[2] += 1;
                }
                "T" => {
                    assert_eq!(smooth_step(h(f[1]), h(f[2]), h(f[3])).to_bits(), h(f[4]).to_bits(), "{line}");
                    counts[3] += 1;
                }
                _ => {
                    assert_eq!(f[0], "N", "bad golden line");
                    let v = Vector::new(h(f[1]), h(f[2]), h(f[3]));
                    assert_eq!(v.length().to_bits(), h(f[4]).to_bits(), "{line}");
                    let n = v.normal();
                    assert_eq!(
                        (n.x.to_bits(), n.y.to_bits(), n.z.to_bits()),
                        (h(f[5]).to_bits(), h(f[6]).to_bits(), h(f[7]).to_bits()),
                        "{line}"
                    );
                    assert_eq!(Vector2::new(v.x, v.y).length().to_bits(), h(f[8]).to_bits(), "{line}");
                    counts[4] += 1;
                }
            }
        }
        assert!(counts.iter().all(|&c| c >= 1500 || c == 0 || c >= 1000), "{counts:?}");
    }
}
