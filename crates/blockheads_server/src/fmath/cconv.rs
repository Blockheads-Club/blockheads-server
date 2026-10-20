//! C float -> integer conversions with x86-64 `cvtt*2si` semantics.
//!
//!
//! `cvttss2si r32` / `cvttsd2si r32` truncate toward zero and return the "integer indefinite"
//! value `0x80000000` for NaN, infinities and any value whose truncation does not fit; the `r64`
//! forms return `0x8000000000000000`. Rust's `as` saturates instead, so every conversion here
//! range-tests first. Narrow and unsigned targets are derived the way GCC x86-64 compiles them.

const TWO_POW_31_F32: f32 = 2147483648.0;
const TWO_POW_63_F32: f32 = 9223372036854775808.0;
const TWO_POW_63_F64: f64 = 9223372036854775808.0;

/// `(int)x` for `float x` (`cvttss2si r32`).
#[inline]
pub fn f32_to_i32(x: f32) -> i32 {
    if x.is_nan() || x >= TWO_POW_31_F32 || x < -TWO_POW_31_F32 {
        i32::MIN
    } else {
        x as i32
    }
}

/// `(int)x` for `double x` (`cvttsd2si r32`).
#[inline]
pub fn f64_to_i32(x: f64) -> i32 {
    // truncation must lie in [-2^31, 2^31-1]; -2147483648.9 truncates to -2^31 (valid)
    if x.is_nan() || x >= 2147483648.0 || x <= -2147483649.0 {
        i32::MIN
    } else {
        x as i32
    }
}

/// `(long)x` for `float x` (`cvttss2si r64`).
#[inline]
pub fn f32_to_i64(x: f32) -> i64 {
    if x.is_nan() || x >= TWO_POW_63_F32 || x < -TWO_POW_63_F32 {
        i64::MIN
    } else {
        x as i64
    }
}

/// `(long)x` for `double x` (`cvttsd2si r64`).
#[inline]
pub fn f64_to_i64(x: f64) -> i64 {
    if x.is_nan() || x >= TWO_POW_63_F64 || x < -TWO_POW_63_F64 {
        i64::MIN
    } else {
        x as i64
    }
}

/// `(uint32_t)x` for `float x`: 64-bit truncation, low 32 bits.
#[inline]
pub fn f32_to_u32(x: f32) -> u32 {
    f32_to_i64(x) as u32
}

/// `(uint32_t)x` for `double x`: 64-bit truncation, low 32 bits.
#[inline]
pub fn f64_to_u32(x: f64) -> u32 {
    f64_to_i64(x) as u32
}

/// `(uint64_t)x` for `float x`, GCC expansion: `!(x >= 2^63) ? cvt(x): cvt(x - 2^63) ^ 2^63`.
/// NaN takes the first path (observed with gcc 4.8 `-O0` in the reference image: `(unsigned long)NAN`
/// is `0x8000000000000000`, which differs from the hand derivation in).
#[inline]
pub fn f32_to_u64(x: f32) -> u64 {
    if !(x >= TWO_POW_63_F32) {
        f32_to_i64(x) as u64
    } else {
        (f32_to_i64(x - TWO_POW_63_F32) as u64) ^ 0x8000_0000_0000_0000
    }
}

/// `(uint64_t)x` for `double x`, GCC expansion.
#[inline]
pub fn f64_to_u64(x: f64) -> u64 {
    if !(x >= TWO_POW_63_F64) {
        f64_to_i64(x) as u64
    } else {
        (f64_to_i64(x - TWO_POW_63_F64) as u64) ^ 0x8000_0000_0000_0000
    }
}

/// `(int16_t)x` for `float x`: `cvttss2si r32` then low 16 bits.
#[inline]
pub fn f32_to_i16(x: f32) -> i16 {
    f32_to_i32(x) as i16
}
/// `(uint16_t)x` for `float x`.
#[inline]
pub fn f32_to_u16(x: f32) -> u16 {
    f32_to_i32(x) as u16
}
/// `(int8_t)x` for `float x`.
#[inline]
pub fn f32_to_i8(x: f32) -> i8 {
    f32_to_i32(x) as i8
}
/// `(uint8_t)x` / `(uint8_t)(int)x` for `float x`.
#[inline]
pub fn f32_to_u8(x: f32) -> u8 {
    f32_to_i32(x) as u8
}
/// `(int16_t)x` for `double x`.
#[inline]
pub fn f64_to_i16(x: f64) -> i16 {
    f64_to_i32(x) as i16
}
/// `(uint16_t)x` for `double x`.
#[inline]
pub fn f64_to_u16(x: f64) -> u16 {
    f64_to_i32(x) as u16
}
/// `(int8_t)x` for `double x`.
#[inline]
pub fn f64_to_i8(x: f64) -> i8 {
    f64_to_i32(x) as i8
}
/// `(uint8_t)x` for `double x`.
#[inline]
pub fn f64_to_u8(x: f64) -> u8 {
    f64_to_i32(x) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn i32_conversions() {
        assert_eq!(f32_to_i32(f32::NAN), i32::MIN);
        assert_eq!(f32_to_i32(f32::INFINITY), i32::MIN);
        assert_eq!(f32_to_i32(f32::NEG_INFINITY), i32::MIN);
        assert_eq!(f32_to_i32(2147483520.0), 2147483520);
        assert_eq!(f32_to_i32(2147483648.0), i32::MIN);
        assert_eq!(f32_to_i32(-2147483648.0), i32::MIN);
        assert_eq!(f32_to_i32(-2147483904.0), i32::MIN);
        assert_eq!(f32_to_i32(3e9), i32::MIN);
        assert_eq!(f32_to_i32(0.999999), 0);
        assert_eq!(f32_to_i32(-0.999999), 0);
        assert_eq!(f32_to_i32(-0.9), 0);
        assert_eq!(f32_to_i32(-0.0), 0);
        assert_eq!(f32_to_i32(1e-45), 0);
        assert_eq!(f32_to_i32(8388607.5), 8388607);
        assert_eq!(f32_to_i32(-7.9), -7);
        assert_eq!(f64_to_i32(2147483647.9), 2147483647);
        assert_eq!(f64_to_i32(2147483648.0), i32::MIN);
        assert_eq!(f64_to_i32(-2147483648.9), i32::MIN); // valid result that coincides with indefinite
        assert_eq!(f64_to_i32(-2147483649.0), i32::MIN);
        assert_eq!(f64_to_i32(f64::NAN), i32::MIN);
        assert_eq!(f64_to_i32(-5.5), -5);
    }

    #[test]
    fn u32_conversions() {
        assert_eq!(f32_to_u32(-1.0), 0xFFFF_FFFF);
        assert_eq!(f64_to_u32(-1.0), 0xFFFF_FFFF);
        assert_eq!(f64_to_u32(4294967295.0), 4294967295);
        assert_eq!(f64_to_u32(4294967296.0), 0);
        assert_eq!(f32_to_u32(4.0e9), 4_000_000_000);
        assert_eq!(f32_to_u32(f32::NAN), 0);
        assert_eq!(f64_to_u32(f64::NAN), 0);
        assert_eq!(f32_to_u32(1e20), 0);
        assert_eq!(f64_to_u32(1e19), 0);
        assert_eq!(f64_to_u32(-3e9), 1294967296);
    }

    #[test]
    fn narrow_conversions() {
        assert_eq!(f32_to_i16(40000.0), -25536);
        assert_eq!(f32_to_u16(-1.0), 65535);
        assert_eq!(f32_to_u8(300.5), 44);
        assert_eq!(f32_to_u8(f32::NAN), 0);
        assert_eq!(f32_to_i8(200.0), -56);
        assert_eq!(f64_to_i16(40000.0), -25536);
        assert_eq!(f64_to_u16(-1.0), 65535);
        assert_eq!(f64_to_u8(300.5), 44);
        assert_eq!(f64_to_i8(f64::NAN), 0);
        assert_eq!(f32_to_u8(255.9), 255);
    }

    /// `tests/cconv_golden.rs`).
    #[test]
    fn u64_conversions() {
        assert_eq!(f32_to_u64(f32::NAN), 0x8000_0000_0000_0000);
        assert_eq!(f32_to_u64(18446744073709551616.0), 0);
        assert_eq!(f32_to_u64(9.223372e18), 9223372036854775808); // == 2^63 in f32
        assert_eq!(f32_to_u64(9223371487098961920.0), 9223371487098961920);
        assert_eq!(f64_to_u64(1.5e19), 15000000000000000000);
        assert_eq!(f64_to_u64(3.0), 3);
        assert_eq!(f64_to_i64(f64::INFINITY), i64::MIN);
        assert_eq!(f32_to_i64(-1.5), -1);
    }

    /// Golden check against gcc 4.8 `-O0` output inside the reference image (glibc 2.19 userland),
    /// produced by `tools/libm_probe/cconv_probe.c`; see `tests/data/cconv_golden.txt`.
    #[test]
    fn matches_gcc_golden() {
        let text = include_str!("../../tests/data/cconv_golden.txt");
        let mut n = 0;
        for line in text.lines() {
            let mut it = line.split_whitespace();
            let kind = it.next().unwrap();
            let bits = u64::from_str_radix(it.next().unwrap(), 16).unwrap();
            let v: std::collections::HashMap<&str, i128> = it
                .map(|kv| {
                    let (k, v) = kv.split_once('=').unwrap();
                    (k, v.parse::<i128>().unwrap())
                })
                .collect();
            if kind == "F" {
                let x = f32::from_bits(bits as u32);
                assert_eq!(f32_to_i32(x) as i128, v["i32"], "{line}");
                assert_eq!(f32_to_i64(x) as i128, v["i64"], "{line}");
                assert_eq!(f32_to_u32(x) as i128, v["u32"], "{line}");
                assert_eq!(f32_to_u64(x) as i128, v["u64"], "{line}");
                assert_eq!(f32_to_i16(x) as i128, v["i16"], "{line}");
                assert_eq!(f32_to_u16(x) as i128, v["u16"], "{line}");
                assert_eq!(f32_to_i8(x) as i128, v["i8"], "{line}");
                assert_eq!(f32_to_u8(x) as i128, v["u8"], "{line}");
            } else {
                let x = f64::from_bits(bits);
                assert_eq!(f64_to_i32(x) as i128, v["i32"], "{line}");
                assert_eq!(f64_to_i64(x) as i128, v["i64"], "{line}");
                assert_eq!(f64_to_u32(x) as i128, v["u32"], "{line}");
                assert_eq!(f64_to_u64(x) as i128, v["u64"], "{line}");
                assert_eq!(f64_to_i16(x) as i128, v["i16"], "{line}");
                assert_eq!(f64_to_u16(x) as i128, v["u16"], "{line}");
                assert_eq!(f64_to_i8(x) as i128, v["i8"], "{line}");
                assert_eq!(f64_to_u8(x) as i128, v["u8"], "{line}");
            }
            n += 1;
        }
        assert!(n > 30);
    }
}
