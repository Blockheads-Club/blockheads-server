// SPDX-License-Identifier: GPL-3.0-only AND LGPL-2.1-or-later
// Contains derived work; see THIRD_PARTY_NOTICES.md.
//! glibc 2.19 x86-64 libm parity layer.
//!
//!
//! * `f64` `sin cos pow atan2`: glibc 2.19 (IBM Accurate Mathematical Library) is correctly rounded,
//!   so any correctly rounded implementation is bit-identical (route (b)).
//!   They are implemented with a double-double fast path (sin/cos) and an exact arbitrary-precision
//!   slow path with a Ziv rounding test; see [`dd`] and [`big`].
//! * `f32` `sinf cosf`: instruction-level port of `sysdeps/x86_64/fpu/s_sinf.S` / `s_cosf.S`.
//! * `f32` `tanf powf atan2f`: ports of the fdlibm derived C in `sysdeps/ieee754/flt-32/`.
//! * `fmod fmodf sqrt sqrtf floor` are exact IEEE operations (std).
//!
//! The host libm is never called for the transcendental functions (no `f32::sin`, `f64::powf`,...).

#![allow(clippy::excessive_precision, clippy::approx_constant, clippy::many_single_char_names)]
#![allow(clippy::needless_range_loop, clippy::too_many_arguments, clippy::collapsible_else_if)]

// ---------------------------------------------------------------------------------------------
// simple exact operations
// ---------------------------------------------------------------------------------------------

/// `floor(x)`: exact.
#[inline]
pub fn floor(x: f64) -> f64 {
    if x.is_nan() {
        return x + x; // signalling NaN is quieted
    }
    x.floor()
}

/// C `fmod(x, y)` (result has the sign of `x`, `fmod(x, 0) = NaN`). Exact.
#[inline]
pub fn fmod(x: f64, y: f64) -> f64 {
    x86_nan(x % y, x.is_nan() || y.is_nan())
}

/// C `fmodf`. Exact.
#[inline]
pub fn fmodf(x: f32, y: f32) -> f32 {
    x86_nanf(x % y, x.is_nan() || y.is_nan())
}

/// `sqrtsd`.
#[inline]
pub fn sqrt(x: f64) -> f64 {
    x86_nan(x.sqrt(), x.is_nan())
}

/// `sqrtss`.
#[inline]
pub fn sqrtf(x: f32) -> f32 {
    x86_nanf(x.sqrt(), x.is_nan())
}

/// `fabsf`: clears the sign bit (also for NaN).
#[inline]
pub fn fabsf(x: f32) -> f32 {
    f32::from_bits(x.to_bits() & 0x7fff_ffff)
}

/// `fabs`: clears the sign bit (also for NaN).
#[inline]
pub fn fabs(x: f64) -> f64 {
    f64::from_bits(x.to_bits() & 0x7fff_ffff_ffff_ffff)
}

/// x86-64 SSE's default NaN (the "QNaN floating-point indefinite", sign bit set): what an invalid
/// operation without NaN operands produces there. AArch64 and most other hosts produce the positive
/// quiet NaN instead, so a NaN created from non-NaN inputs is rewritten to the x86 one. On x86-64 this is
/// the identity. NaN operands propagate the same way on both (quieted, payload kept).
#[inline]
fn x86_nan(r: f64, nan_input: bool) -> f64 {
    if r.is_nan() && !nan_input {
        f64::from_bits(0xfff8_0000_0000_0000)
    } else {
        r
    }
}

/// `f32` version of [`x86_nan`].
#[inline]
fn x86_nanf(r: f32, nan_input: bool) -> f32 {
    if r.is_nan() && !nan_input {
        f32::from_bits(0xffc0_0000)
    } else {
        r
    }
}

/// Result of the C expression `a + b` when at least one operand is NaN, with the operand priority
/// observed in glibc 2.19: a NaN `first` wins (quieted), otherwise `second` (quieted). Written out
/// explicitly because the operand order of a Rust `+` is not guaranteed in debug vs release builds.
#[inline]
fn nan_add32(first: f32, second: f32) -> f32 {
    let v = if first.is_nan() { first } else { second };
    f32::from_bits(v.to_bits() | 0x0040_0000)
}

#[inline]
fn fw(x: f32) -> i32 {
    x.to_bits() as i32
}
#[inline]
fn sfw(i: i32) -> f32 {
    f32::from_bits(i as u32)
}
#[inline]
fn f(bits: u32) -> f32 {
    f32::from_bits(bits)
}
#[inline]
fn d(bits: u64) -> f64 {
    f64::from_bits(bits)
}

// ---------------------------------------------------------------------------------------------
// sinf / cosf: port of sysdeps/x86_64/fpu/s_sinf.S, s_cosf.S (glibc 2.19)
// ---------------------------------------------------------------------------------------------

/// `L(PIO4J)`: table of `j*Pi/4`, j = 0..=10 (s_sinf.S).
const PIO4J: [u64; 11] = [
    0x0000000000000000,
    0x3fe921fb54442d18,
    0x3ff921fb54442d18,
    0x4002d97c7f3321d2,
    0x400921fb54442d18,
    0x400f6a7a2955385e,
    0x4012d97c7f3321d2,
    0x4015fdbbe9bba775,
    0x401921fb54442d18,
    0x401c463abeccb2bb,
    0x401f6a7a2955385e,
];
/// `L(_FPI)`: 4/Pi broken into a sum of DP values (s_sinf.S).
const FPI: [u64; 25] = [
    0x0000000000000000,
    0x3ff45f306c000000,
    0x3e3c9c882a000000,
    0x3c54fe13a8000000,
    0x3aaf47d4d0000000,
    0x38fbb81b6c000000,
    0x3714acc9e0000000,
    0x3560e4107c000000,
    0x33bca2c756000000,
    0x31fbd778ac000000,
    0x300b7246e0000000,
    0x2e5d2126e8000000,
    0x2c97003248000000,
    0x2ad77504e8000000,
    0x290921cfe0000000,
    0x274deb1cb0000000,
    0x25829a73e0000000,
    0x23fd1046be000000,
    0x2224baed10000000,
    0x20709d338e000000,
    0x1e535a2f80000000,
    0x1cef904e64000000,
    0x1b0d639830000000,
    0x1964ce7d24000000,
    0x17b908bf16000000,
];
const SIN2_0: u64 = 0xbfc555555543d49d;
const SIN2_1: u64 = 0x3f8110f475cec8c5;
const COS2_0: u64 = 0xbfdfffffff5cc6fd;
const COS2_1: u64 = 0x3fa55514b178dac5;
const S0: u64 = 0xbfc5555555551cd9;
const S1: u64 = 0x3f81111110c2688b;
const S2: u64 = 0xbf2a019f8b4bd1f9;
const S3: u64 = 0x3ec71d7264e6b5b4;
const S4: u64 = 0xbe5a947e1674b58a;
const C0: u64 = 0xbfdffffffffe98ae;
const C1: u64 = 0x3fa55555545c50c7;
const C2: u64 = 0xbf56c16b348b6874;
const C3: u64 = 0x3efa00eb9ac43cc0;
const C4: u64 = 0xbe923c97dd8844d7;
const DP_PIO4: u64 = 0x3fe921fb54442d18;
const DP_INVPIO4: u64 = 0x3ff45f306dc9c883;
const DP_PIO4HI: u64 = 0xbfe921fb54000000; // -(high part of Pi/4)
const DP_PIO4LO: u64 = 0xbe010b4611a62633; // -(low part of Pi/4)
const SP_INVPIO4: u32 = 0x3fa2f983;

/// `L(reconstruction)` shared by `__sinf`/`__cosf`. `sgn` is the `DP_ONES` index
/// (sinf: sign(x) ^ ((n>>2)&1), cosf: (n>>2)&1).
#[inline]
fn sincosf_reconstruct(n: u32, t: f64, sgn: usize) -> f32 {
    let ones = [1.0f64, -1.0f64];
    if n & 2 != 0 {
        // cos polynomial
        let y = t * t;
        let z = y * y;
        let mut x4 = d(C4) * z;
        let mut x3 = d(C3) * z;
        x4 += d(C2);
        x4 *= z;
        x3 += d(C1);
        x3 *= z;
        x4 += d(C0);
        x4 *= y;
        x3 += x4;
        x3 += 1.0;
        x3 *= ones[sgn];
        x3 as f32
    } else {
        // sin polynomial
        let mut x4 = t;
        let y = t * t;
        let z = y * y;
        let mut x2 = d(S4) * z;
        let mut x3 = d(S3) * z;
        x2 += d(S2);
        x2 *= z;
        x3 += d(S1);
        x3 *= z;
        x2 += d(S0);
        x2 *= y;
        x4 *= ones[sgn];
        x3 += x2;
        x3 *= x4;
        x3 += x4;
        x3 as f32
    }
}

/// `L(very_large_args)` of s_sinf.S / s_cosf.S: |x| >= 2^23 (finite). Returns `(k, t)` with
/// `t` already multiplied by Pi/4; the caller derives `n` from `k`.
fn sincosf_very_large(ax_bits: u32, xa: f64) -> (u32, f64) {
    let bitpos = (ax_bits >> 23) as i32 - 68;
    let j = (bitpos / 28) as usize;
    let mut tmp3 = d(FPI[j - 2]) * xa;
    let tmp2 = d(FPI[j - 1]) * xa;
    let mut tmp0 = d(FPI[j]) * xa;
    let tmp1 = d(FPI[j + 1]) * xa;
    if bitpos >= (j as i32) * 28 + 19 {
        let hi = d(tmp3.to_bits() & 0xffff_ffff_0000_0000);
        tmp3 -= hi;
    }
    let tmp5 = tmp2 + tmp3;
    let tmp6 = 4503599627370496.0f64 + tmp5; // + 2^52
    let mut k = tmp6.to_bits() as u32;
    let mut tmp4 = tmp6 + (-4503599627370496.0f64);
    if tmp4 > tmp5 {
        k = k.wrapping_sub(1);
        tmp4 += -1.0;
    }
    tmp3 -= tmp4;
    let zerone = [0.0f64, -1.0f64];
    tmp3 += zerone[(k & 1) as usize];
    tmp3 += tmp2;
    tmp0 += tmp3;
    tmp0 += tmp1;
    tmp0 *= d(DP_PIO4);
    (k, tmp0)
}

/// glibc 2.19 `sinf` (x86-64 assembly version, bit-exact port).
fn sinf_port(x: f32) -> f32 {
    let bits = x.to_bits();
    let ax = bits & 0x7fff_ffff;
    let xd = x as f64;
    if ax < 0x3f49_0fdb {
        // |x| < Pi/4
        if ax < 0x3d00_0000 {
            if ax < 0x3200_0000 {
                if ax == 0 {
                    return x;
                }
                // |x| < 2^-27: x - x*2^-50
                let t = xd * d(0x3cd0000000000000);
                return (xd - t) as f32;
            }
            // 2^-27 <= |x| < 2^-5
            let x2 = xd * xd;
            let mut r = d(SIN2_1) * x2;
            r += d(SIN2_0);
            r *= x2;
            r *= xd;
            r += xd;
            return r as f32;
        }
        let y = xd * xd;
        let z = y * y;
        let mut x4 = d(S4) * z;
        let mut x5 = d(S3) * z;
        x4 += d(S2);
        x4 *= z;
        x5 += d(S1);
        x5 *= z;
        x4 += d(S0);
        x4 *= y;
        x5 *= xd;
        x4 *= xd;
        x4 += x5;
        let r = xd + x4;
        return r as f32;
    }
    let xa = f64::from_bits(xd.to_bits() & 0x7fff_ffff_ffff_ffff);
    let xs = f32::from_bits(ax);
    let sign = (bits >> 31) as usize;
    if ax < 0x40e2_31d6 {
        // Pi/4 <= |x| < 9*Pi/4
        let q = xs * f(SP_INVPIO4);
        let k = q as i32; // cvttss2si, in range
        let n = (k + 1) as u32;
        let j = (n & 0x0e) as usize;
        let t = xa - d(PIO4J[j]);
        return sincosf_reconstruct(n, t, sign ^ (((n >> 2) & 1) as usize));
    }
    if ax >= 0x7f80_0000 {
        // Inf or NaN: x - x
        return x - x;
    }
    if ax < 0x4b00_0000 {
        // 9*Pi/4 <= |x| < 2^23
        let q = d(DP_INVPIO4) * xa;
        let k = q as i32;
        let n = (k + 1) as u32;
        let j = (n & 0xffff_fffe) as i32;
        let jd = j as f64;
        let mut t = xa + d(DP_PIO4HI) * jd;
        t += jd * d(DP_PIO4LO);
        return sincosf_reconstruct(n, t, sign ^ (((n >> 2) & 1) as usize));
    }
    let (k, t) = sincosf_very_large(ax, xa);
    let n = k.wrapping_add(1);
    sincosf_reconstruct(n, t, sign ^ (((n >> 2) & 1) as usize))
}

/// glibc 2.19 `cosf` (x86-64 assembly version, bit-exact port).
fn cosf_port(x: f32) -> f32 {
    let bits = x.to_bits();
    let ax = bits & 0x7fff_ffff;
    let xd = x as f64;
    if ax < 0x3f49_0fdb {
        if ax < 0x3d00_0000 {
            if ax < 0x3200_0000 {
                // 1.0f - |x|
                return 1.0f32 - f32::from_bits(ax);
            }
            let x2 = xd * xd;
            let mut r = d(COS2_1) * x2;
            r += d(COS2_0);
            r *= x2;
            r += 1.0;
            return r as f32;
        }
        let y = xd * xd;
        let z = y * y;
        let mut x3 = d(C4) * z;
        let mut x5 = d(C3) * z;
        x3 += d(C2);
        x3 *= z;
        x5 += d(C1);
        x5 *= z;
        x3 += d(C0);
        x3 *= y;
        x3 += x5;
        x3 += 1.0;
        return x3 as f32;
    }
    let xa = f64::from_bits(xd.to_bits() & 0x7fff_ffff_ffff_ffff);
    let xs = f32::from_bits(ax);
    if ax < 0x40e2_31d6 {
        let q = xs * f(SP_INVPIO4);
        let k = q as i32;
        let n1 = (k + 1) as u32;
        let j = (n1 & 0x0e) as usize;
        let n = n1.wrapping_add(2);
        let t = xa - d(PIO4J[j]);
        return sincosf_reconstruct(n, t, ((n >> 2) & 1) as usize);
    }
    if ax >= 0x7f80_0000 {
        return x - x;
    }
    if ax < 0x4b00_0000 {
        let q = d(DP_INVPIO4) * xa;
        let k = q as i32;
        let n1 = (k + 1) as u32;
        let j = (n1 & 0xffff_fffe) as i32;
        let jd = j as f64;
        let n = n1.wrapping_add(2);
        let mut t = xa + d(DP_PIO4HI) * jd;
        t += jd * d(DP_PIO4LO);
        return sincosf_reconstruct(n, t, ((n >> 2) & 1) as usize);
    }
    let (k, t) = sincosf_very_large(ax, xa);
    let n = k.wrapping_add(3);
    sincosf_reconstruct(n, t, ((n >> 2) & 1) as usize)
}

// ---------------------------------------------------------------------------------------------
// scalbnf / floorf helpers (flt-32/s_scalbnf.c)
// ---------------------------------------------------------------------------------------------

/// `__scalbnf` (flt-32/s_scalbnf.c).
fn scalbnf(mut x: f32, n: i32) -> f32 {
    let two25 = f(0x4c000000);
    let twom25 = f(0x33000000);
    let huge = 1.0e+30f32;
    let tiny = 1.0e-30f32;
    let mut ix = fw(x);
    let mut k = (ix & 0x7f80_0000) >> 23;
    if k == 0 {
        if (ix & 0x7fff_ffff) == 0 {
            return x;
        }
        x *= two25;
        ix = fw(x);
        k = ((ix & 0x7f80_0000) >> 23) - 25;
    }
    if k == 0xff {
        return x + x;
    }
    let copysign = |a: f32, b: f32| f32::from_bits((a.to_bits() & 0x7fff_ffff) | (b.to_bits() & 0x8000_0000));
    if n < -50000 {
        return tiny * copysign(tiny, x);
    }
    if n > 50000 || k + n > 0xfe {
        return huge * copysign(huge, x);
    }
    k += n;
    if k > 0 {
        return sfw((ix & 0x807f_ffffu32 as i32) | (k << 23));
    }
    if k <= -25 {
        return tiny * copysign(tiny, x);
    }
    k += 25;
    x = sfw((ix & 0x807f_ffffu32 as i32) | (k << 23));
    x * twom25
}

// ---------------------------------------------------------------------------------------------
// tanf: flt-32/s_tanf.c, k_tanf.c, e_rem_pio2f.c, k_rem_pio2f.c
// ---------------------------------------------------------------------------------------------

const TWO_OVER_PI: [i32; 198] = [
    0xA2, 0xF9, 0x83, 0x6E, 0x4E, 0x44, 0x15, 0x29, 0xFC, 0x27, 0x57, 0xD1, 0xF5, 0x34, 0xDD, 0xC0, 0xDB, 0x62, 0x95,
    0x99, 0x3C, 0x43, 0x90, 0x41, 0xFE, 0x51, 0x63, 0xAB, 0xDE, 0xBB, 0xC5, 0x61, 0xB7, 0x24, 0x6E, 0x3A, 0x42, 0x4D,
    0xD2, 0xE0, 0x06, 0x49, 0x2E, 0xEA, 0x09, 0xD1, 0x92, 0x1C, 0xFE, 0x1D, 0xEB, 0x1C, 0xB1, 0x29, 0xA7, 0x3E, 0xE8,
    0x82, 0x35, 0xF5, 0x2E, 0xBB, 0x44, 0x84, 0xE9, 0x9C, 0x70, 0x26, 0xB4, 0x5F, 0x7E, 0x41, 0x39, 0x91, 0xD6, 0x39,
    0x83, 0x53, 0x39, 0xF4, 0x9C, 0x84, 0x5F, 0x8B, 0xBD, 0xF9, 0x28, 0x3B, 0x1F, 0xF8, 0x97, 0xFF, 0xDE, 0x05, 0x98,
    0x0F, 0xEF, 0x2F, 0x11, 0x8B, 0x5A, 0x0A, 0x6D, 0x1F, 0x6D, 0x36, 0x7E, 0xCF, 0x27, 0xCB, 0x09, 0xB7, 0x4F, 0x46,
    0x3F, 0x66, 0x9E, 0x5F, 0xEA, 0x2D, 0x75, 0x27, 0xBA, 0xC7, 0xEB, 0xE5, 0xF1, 0x7B, 0x3D, 0x07, 0x39, 0xF7, 0x8A,
    0x52, 0x92, 0xEA, 0x6B, 0xFB, 0x5F, 0xB1, 0x1F, 0x8D, 0x5D, 0x08, 0x56, 0x03, 0x30, 0x46, 0xFC, 0x7B, 0x6B, 0xAB,
    0xF0, 0xCF, 0xBC, 0x20, 0x9A, 0xF4, 0x36, 0x1D, 0xA9, 0xE3, 0x91, 0x61, 0x5E, 0xE6, 0x1B, 0x08, 0x65, 0x99, 0x85,
    0x5F, 0x14, 0xA0, 0x68, 0x40, 0x8D, 0xFF, 0xD8, 0x80, 0x4D, 0x73, 0x27, 0x31, 0x06, 0x06, 0x15, 0x56, 0xCA, 0x73,
    0xA8, 0xC9, 0x60, 0xE2, 0x7B, 0xC0, 0x8C, 0x6B,
];

const NPIO2_HW: [i32; 32] = [
    0x3fc90f00, 0x40490f00, 0x4096cb00, 0x40c90f00, 0x40fb5300, 0x4116cb00, 0x412fed00, 0x41490f00, 0x41623100,
    0x417b5300, 0x418a3a00, 0x4196cb00, 0x41a35c00, 0x41afed00, 0x41bc7e00, 0x41c90f00, 0x41d5a000, 0x41e23100,
    0x41eec200, 0x41fb5300, 0x4203f200, 0x420a3a00, 0x42108300, 0x4216cb00, 0x421d1400, 0x42235c00, 0x4229a500,
    0x422fed00, 0x42363600, 0x423c7e00, 0x4242c700, 0x42490f00,
];

/// `__kernel_rem_pio2f` (flt-32/k_rem_pio2f.c) for `prec = 2` (the only use).
fn kernel_rem_pio2f(x: &[f32; 3], y: &mut [f32; 2], e0: i32, nx: i32) -> i32 {
    const INIT_JK: [i32; 3] = [4, 7, 9];
    let pio2: [f32; 11] = [
        1.5703125000e+00,
        4.5776367188e-04,
        2.5987625122e-05,
        7.5437128544e-08,
        6.0026650317e-11,
        7.3896444519e-13,
        5.3845816694e-15,
        5.6378512969e-18,
        8.3009228831e-20,
        3.2756352257e-22,
        6.3331015649e-25,
    ];
    let zero = 0.0f32;
    let one = 1.0f32;
    let two8 = 2.5600000000e+02f32;
    let twon8 = 3.9062500000e-03f32;
    let ipio2 = &TWO_OVER_PI;
    let mut iq = [0i32; 20];
    let mut fv = [0f32; 20];
    let mut fq = [0f32; 20];
    let mut q = [0f32; 20];

    let jk = INIT_JK[2];
    let jp = jk;
    let jx = nx - 1;
    let mut jv = (e0 - 3) / 8;
    if jv < 0 {
        jv = 0;
    }
    let mut q0 = e0 - 8 * (jv + 1);
    let mut j = jv - jx;
    let m = jx + jk;
    for i in 0..=m {
        fv[i as usize] = if j < 0 { zero } else { ipio2[j as usize] as f32 };
        j += 1;
    }
    for i in 0..=jk {
        let mut fwv = 0.0f32;
        for j in 0..=jx {
            fwv += x[j as usize] * fv[(jx + i - j) as usize];
        }
        q[i as usize] = fwv;
    }
    let mut jz = jk;
    let mut z: f32;
    let mut n: i32;
    let mut ih: i32;
    loop {
        // recompute:
        {
            let mut i = 0usize;
            let mut j = jz;
            z = q[jz as usize];
            while j > 0 {
                let fwv = ((twon8 * z) as i32) as f32;
                iq[i] = (z - two8 * fwv) as i32;
                z = q[(j - 1) as usize] + fwv;
                i += 1;
                j -= 1;
            }
        }
        z = scalbnf(z, q0);
        z -= 8.0f32 * (z * 0.125f32).floor();
        n = z as i32;
        z -= n as f32;
        ih = 0;
        if q0 > 0 {
            let i = iq[(jz - 1) as usize] >> (8 - q0);
            n += i;
            iq[(jz - 1) as usize] -= i << (8 - q0);
            ih = iq[(jz - 1) as usize] >> (7 - q0);
        } else if q0 == 0 {
            ih = iq[(jz - 1) as usize] >> 7;
        } else if z >= 0.5f32 {
            ih = 2;
        }
        if ih > 0 {
            n += 1;
            let mut carry = 0;
            for i in 0..jz as usize {
                let jj = iq[i];
                if carry == 0 {
                    if jj != 0 {
                        carry = 1;
                        iq[i] = 0x100 - jj;
                    }
                } else {
                    iq[i] = 0xff - jj;
                }
            }
            if q0 > 0 {
                match q0 {
                    1 => iq[(jz - 1) as usize] &= 0x7f,
                    2 => iq[(jz - 1) as usize] &= 0x3f,
                    _ => {}
                }
            }
            if ih == 2 {
                z = one - z;
                if carry != 0 {
                    z -= scalbnf(one, q0);
                }
            }
        }
        if z == zero {
            let mut jj = 0;
            let mut i = jz - 1;
            while i >= jk {
                jj |= iq[i as usize];
                i -= 1;
            }
            if jj == 0 {
                let mut k = 1;
                while iq[(jk - k) as usize] == 0 {
                    k += 1;
                }
                for i in (jz + 1)..=(jz + k) {
                    fv[(jx + i) as usize] = ipio2[(jv + i) as usize] as f32;
                    let mut fwv = 0.0f32;
                    for j in 0..=jx {
                        fwv += x[j as usize] * fv[(jx + i - j) as usize];
                    }
                    q[i as usize] = fwv;
                }
                jz += k;
                continue;
            }
        }
        break;
    }
    // chop off zero terms
    if z == 0.0f32 {
        jz -= 1;
        q0 -= 8;
        while iq[jz as usize] == 0 {
            jz -= 1;
            q0 -= 8;
        }
    } else {
        z = scalbnf(z, -q0);
        if z >= two8 {
            let fwv = ((twon8 * z) as i32) as f32;
            iq[jz as usize] = (z - two8 * fwv) as i32;
            jz += 1;
            q0 += 8;
            iq[jz as usize] = fwv as i32;
        } else {
            iq[jz as usize] = z as i32;
        }
    }
    let mut fwv = scalbnf(one, q0);
    let mut i = jz;
    while i >= 0 {
        q[i as usize] = fwv * (iq[i as usize] as f32);
        fwv *= twon8;
        i -= 1;
    }
    let mut i = jz;
    while i >= 0 {
        let mut acc = 0.0f32;
        let mut k = 0;
        while k <= jp && k <= jz - i {
            acc += pio2[k as usize] * q[(i + k) as usize];
            k += 1;
        }
        fq[(jz - i) as usize] = acc;
        i -= 1;
    }
    // prec == 2
    let mut fvv = 0.0f32;
    let mut i = jz;
    while i >= 0 {
        fvv += fq[i as usize];
        i -= 1;
    }
    y[0] = if ih == 0 { fvv } else { -fvv };
    fvv = fq[0] - fvv;
    for i in 1..=jz as usize {
        fvv += fq[i];
    }
    y[1] = if ih == 0 { fvv } else { -fvv };
    n & 7
}

/// `__ieee754_rem_pio2f` (flt-32/e_rem_pio2f.c).
fn ieee754_rem_pio2f(x: f32, y: &mut [f32; 2]) -> i32 {
    let half = 5.0000000000e-01f32;
    let two8 = 2.5600000000e+02f32;
    let invpio2 = 6.3661980629e-01f32;
    let pio2_1 = 1.5707855225e+00f32;
    let pio2_1t = 1.0804334124e-05f32;
    let pio2_2 = 1.0804273188e-05f32;
    let pio2_2t = 6.0770999344e-11f32;
    let pio2_3 = 6.0770943833e-11f32;
    let pio2_3t = 6.1232342629e-17f32;

    let hx = fw(x);
    let ix = hx & 0x7fff_ffff;
    if ix <= 0x3f49_0fd8 {
        y[0] = x;
        y[1] = 0.0;
        return 0;
    }
    if ix < 0x4016_cbe4 {
        if hx > 0 {
            let mut z = x - pio2_1;
            if (ix & 0xffff_fff0u32 as i32) != 0x3fc9_0fd0 {
                y[0] = z - pio2_1t;
                y[1] = (z - y[0]) - pio2_1t;
            } else {
                z -= pio2_2;
                y[0] = z - pio2_2t;
                y[1] = (z - y[0]) - pio2_2t;
            }
            return 1;
        } else {
            let mut z = x + pio2_1;
            if (ix & 0xffff_fff0u32 as i32) != 0x3fc9_0fd0 {
                y[0] = z + pio2_1t;
                y[1] = (z - y[0]) + pio2_1t;
            } else {
                z += pio2_2;
                y[0] = z + pio2_2t;
                y[1] = (z - y[0]) + pio2_2t;
            }
            return -1;
        }
    }
    if ix <= 0x4349_0f80 {
        let mut t = fabsf(x);
        let n = (t * invpio2 + half) as i32;
        let fnn = n as f32;
        let mut r = t - fnn * pio2_1;
        let mut w = fnn * pio2_1t;
        if n < 32 && ((ix & 0xffff_ff00u32 as i32) != NPIO2_HW[(n - 1) as usize]) {
            y[0] = r - w;
        } else {
            let j = ix >> 23;
            y[0] = r - w;
            let high = y[0].to_bits();
            let mut i = j - ((high >> 23) & 0xff) as i32;
            if i > 8 {
                t = r;
                w = fnn * pio2_2;
                r = t - w;
                w = fnn * pio2_2t - ((t - r) - w);
                y[0] = r - w;
                let high = y[0].to_bits();
                i = j - ((high >> 23) & 0xff) as i32;
                if i > 25 {
                    t = r;
                    w = fnn * pio2_3;
                    r = t - w;
                    w = fnn * pio2_3t - ((t - r) - w);
                    y[0] = r - w;
                }
            }
        }
        y[1] = (r - y[0]) - w;
        if hx < 0 {
            y[0] = -y[0];
            y[1] = -y[1];
            return -n;
        } else {
            return n;
        }
    }
    if ix >= 0x7f80_0000 {
        y[0] = x - x;
        y[1] = y[0];
        return 0;
    }
    let e0 = (ix >> 23) - 134;
    let mut z = sfw(ix - (e0 << 23));
    let mut tx = [0f32; 3];
    for i in 0..2 {
        tx[i] = ((z as i32) as f32) as f32;
        z = (z - tx[i]) * two8;
    }
    tx[2] = z;
    let mut nx = 3;
    while tx[nx - 1] == 0.0 {
        nx -= 1;
    }
    let n = kernel_rem_pio2f(&tx, y, e0, nx as i32);
    if hx < 0 {
        y[0] = -y[0];
        y[1] = -y[1];
        return -n;
    }
    n
}

/// `__kernel_tanf` (flt-32/k_tanf.c).
fn kernel_tanf(mut x: f32, mut y: f32, iy: i32) -> f32 {
    let one = 1.0f32;
    let pio4 = 7.8539812565e-01f32;
    let pio4lo = 3.7748947079e-08f32;
    let t: [f32; 13] = [
        3.3333334327e-01,
        1.3333334029e-01,
        5.3968254477e-02,
        2.1869488060e-02,
        8.8632395491e-03,
        3.5920790397e-03,
        1.4562094584e-03,
        5.8804126456e-04,
        2.4646313977e-04,
        7.8179444245e-05,
        7.1407252108e-05,
        -1.8558637748e-05,
        2.5907305826e-05,
    ];
    let hx = fw(x);
    let ix = hx & 0x7fff_ffff;
    if ix < 0x3180_0000 {
        // |x| < 2^-28: (int)x == 0
        if (ix | (iy + 1)) == 0 {
            return one / fabsf(x);
        } else {
            return if iy == 1 { x } else { -one / x };
        }
    }
    if ix >= 0x3f2c_a140 {
        if hx < 0 {
            x = -x;
            y = -y;
        }
        let z = pio4 - x;
        let w = pio4lo - y;
        x = z + w;
        y = 0.0;
        if fabsf(x) < f(0x3900_0000) {
            // 0x1p-13f
            let c = ((1 - ((hx >> 30) & 2)) * iy) as f32;
            return c * (1.0f32 - ((2 * iy) as f32) * x);
        }
    }
    let z = x * x;
    let w = z * z;
    let mut r = t[1] + w * (t[3] + w * (t[5] + w * (t[7] + w * (t[9] + w * t[11]))));
    let mut v = z * (t[2] + w * (t[4] + w * (t[6] + w * (t[8] + w * (t[10] + w * t[12])))));
    let s = z * x;
    r = y + z * (s * (r + v) + y);
    r += t[0] * s;
    let w = x + r;
    if ix >= 0x3f2c_a140 {
        v = iy as f32;
        return ((1 - ((hx >> 30) & 2)) as f32) * (v - 2.0f32 * (x - (w * w / (w + v) - r)));
    }
    if iy == 1 {
        return w;
    }
    let mut z = w;
    let i = fw(z);
    z = sfw(i & 0xffff_f000u32 as i32);
    v = r - (z - x);
    let a = -1.0f32 / w;
    let mut t = a;
    let i = fw(t);
    t = sfw(i & 0xffff_f000u32 as i32);
    let s = 1.0f32 + t * z;
    t + a * (s + t * v)
}

/// glibc 2.19 `tanf` (flt-32/s_tanf.c).
fn tanf_port(x: f32) -> f32 {
    let mut y = [0f32; 2];
    let ix = fw(x) & 0x7fff_ffff;
    if ix <= 0x3f49_0fda {
        return kernel_tanf(x, 0.0, 1);
    } else if ix >= 0x7f80_0000 {
        return x - x;
    }
    let n = ieee754_rem_pio2f(x, &mut y);
    kernel_tanf(y[0], y[1], 1 - ((n & 1) << 1))
}

// ---------------------------------------------------------------------------------------------
// atanf / atan2f: flt-32/s_atanf.c, e_atan2f.c
// ---------------------------------------------------------------------------------------------

/// `__atanf` (flt-32/s_atanf.c).
fn atanf(mut x: f32) -> f32 {
    let atanhi: [f32; 4] = [4.6364760399e-01, 7.8539812565e-01, 9.8279368877e-01, 1.5707962513e+00];
    let atanlo: [f32; 4] = [5.0121582440e-09, 3.7748947079e-08, 3.4473217170e-08, 7.5497894159e-08];
    let at: [f32; 11] = [
        3.3333334327e-01,
        -2.0000000298e-01,
        1.4285714924e-01,
        -1.1111110449e-01,
        9.0908870101e-02,
        -7.6918758452e-02,
        6.6610731184e-02,
        -5.8335702866e-02,
        4.9768779427e-02,
        -3.6531571299e-02,
        1.6285819933e-02,
    ];
    let one = 1.0f32;
    let huge = 1.0e30f32;
    let hx = fw(x);
    let ix = hx & 0x7fff_ffff;
    let id: i32;
    if ix >= 0x5080_0000 {
        if ix > 0x7f80_0000 {
            return x + x;
        }
        if hx > 0 {
            return atanhi[3] + atanlo[3];
        } else {
            return -atanhi[3] - atanlo[3];
        }
    }
    if ix < 0x3ee0_0000 {
        if ix < 0x3100_0000 && huge + x > one {
            return x;
        }
        id = -1;
    } else {
        x = fabsf(x);
        if ix < 0x3f98_0000 {
            if ix < 0x3f30_0000 {
                id = 0;
                x = (2.0f32 * x - one) / (2.0f32 + x);
            } else {
                id = 1;
                x = (x - one) / (x + one);
            }
        } else if ix < 0x401c_0000 {
            id = 2;
            x = (x - 1.5f32) / (one + 1.5f32 * x);
        } else {
            id = 3;
            x = -1.0f32 / x;
        }
    }
    let z = x * x;
    let w = z * z;
    let s1 = z * (at[0] + w * (at[2] + w * (at[4] + w * (at[6] + w * (at[8] + w * at[10])))));
    let s2 = w * (at[1] + w * (at[3] + w * (at[5] + w * (at[7] + w * at[9]))));
    if id < 0 {
        x - x * (s1 + s2)
    } else {
        let z = atanhi[id as usize] - ((x * (s1 + s2) - atanlo[id as usize]) - x);
        if hx < 0 {
            -z
        } else {
            z
        }
    }
}

/// glibc 2.19 `atan2f` (`__ieee754_atan2f`; the 2.19 wrapper only adds errno handling).
pub fn atan2f(y: f32, x: f32) -> f32 {
    let tiny = 1.0e-30f32;
    let zero = 0.0f32;
    let pi_o_4 = f(0x3f490fdb);
    let pi_o_2 = f(0x3fc90fdb);
    let pi = f(0x40490fdb);
    let pi_lo = f(0xb3bbbd2e);
    let hx = fw(x);
    let ix = hx & 0x7fff_ffff;
    let hy = fw(y);
    let iy = hy & 0x7fff_ffff;
    if ix > 0x7f80_0000 || iy > 0x7f80_0000 {
        return nan_add32(y, x); // C `x+y` compiled as y+x (observed: y wins)
    }
    if hx == 0x3f80_0000 {
        return atanf(y);
    }
    let m = ((hy >> 31) & 1) | ((hx >> 30) & 2);
    if iy == 0 {
        match m {
            0 | 1 => return y,
            2 => return pi + tiny,
            _ => return -pi - tiny,
        }
    }
    if ix == 0 {
        return if hy < 0 { -pi_o_2 - tiny } else { pi_o_2 + tiny };
    }
    if ix == 0x7f80_0000 {
        if iy == 0x7f80_0000 {
            match m {
                0 => return pi_o_4 + tiny,
                1 => return -pi_o_4 - tiny,
                2 => return 3.0f32 * pi_o_4 + tiny,
                _ => return -3.0f32 * pi_o_4 - tiny,
            }
        } else {
            match m {
                0 => return zero,
                1 => return -zero,
                2 => return pi + tiny,
                _ => return -pi - tiny,
            }
        }
    }
    if iy == 0x7f80_0000 {
        return if hy < 0 { -pi_o_2 - tiny } else { pi_o_2 + tiny };
    }
    let k = (iy - ix) >> 23;
    let z: f32 = if k > 60 {
        pi_o_2 + 0.5f32 * pi_lo
    } else if hx < 0 && k < -60 {
        0.0
    } else {
        atanf(fabsf(y / x))
    };
    match m {
        0 => z,
        1 => f32::from_bits(z.to_bits() ^ 0x8000_0000),
        2 => pi - (z - pi_lo),
        _ => (z - pi_lo) - pi,
    }
}

// ---------------------------------------------------------------------------------------------
// powf: flt-32/e_powf.c (the 2.19 w_powf wrapper only handles errno / matherr in _POSIX_ mode)
// ---------------------------------------------------------------------------------------------

/// `__ieee754_powf` (flt-32/e_powf.c), executed in f32 exactly like the C source.
fn ieee754_powf(x: f32, mut y: f32) -> f32 {
    let huge = 1.0e+30f32;
    let tiny = 1.0e-30f32;
    let bp: [f32; 2] = [1.0, 1.5];
    let dp_h: [f32; 2] = [0.0, 5.84960938e-01];
    let dp_l: [f32; 2] = [0.0, 1.56322085e-06];
    let zero = 0.0f32;
    let one = 1.0f32;
    let two = 2.0f32;
    let two24 = 16777216.0f32;
    let l1 = 6.0000002384e-01f32;
    let l2 = 4.2857143283e-01f32;
    let l3 = 3.3333334327e-01f32;
    let l4 = 2.7272811532e-01f32;
    let l5 = 2.3066075146e-01f32;
    let l6 = 2.0697501302e-01f32;
    let p1 = 1.6666667163e-01f32;
    let p2 = -2.7777778450e-03f32;
    let p3 = 6.6137559770e-05f32;
    let p4 = -1.6533901999e-06f32;
    let p5 = 4.1381369442e-08f32;
    let lg2 = 6.9314718246e-01f32;
    let lg2_h = 6.93145752e-01f32;
    let lg2_l = 1.42860654e-06f32;
    let ovt = 4.2995665694e-08f32;
    let cp = 9.6179670095e-01f32;
    let cp_h = 9.6179199219e-01f32;
    let cp_l = 4.7017383622e-06f32;
    let ivln2 = 1.4426950216e+00f32;
    let ivln2_h = 1.4426879883e+00f32;
    let ivln2_l = 7.0526075433e-06f32;

    let hx = fw(x);
    let hy = fw(y);
    let mut ix = hx & 0x7fff_ffff;
    let iy = hy & 0x7fff_ffff;

    if iy == 0 {
        return one;
    }
    if x == 1.0 {
        return one;
    }
    if x == -1.0 && y.is_infinite() {
        return one;
    }
    if ix > 0x7f80_0000 || iy > 0x7f80_0000 {
        return nan_add32(x, y);
    }
    let mut yisint = 0;
    if hx < 0 {
        if iy >= 0x4b80_0000 {
            yisint = 2;
        } else if iy >= 0x3f80_0000 {
            let k = (iy >> 23) - 0x7f;
            let j = iy >> (23 - k);
            if (j << (23 - k)) == iy {
                yisint = 2 - (j & 1);
            }
        }
    }
    if iy == 0x7f80_0000 {
        if ix == 0x3f80_0000 {
            return y - y;
        } else if ix > 0x3f80_0000 {
            return if hy >= 0 { y } else { zero };
        } else {
            return if hy < 0 { -y } else { zero };
        }
    }
    if iy == 0x3f80_0000 {
        return if hy < 0 { one / x } else { x };
    }
    if hy == 0x4000_0000 {
        return x * x;
    }
    if hy == 0x3f00_0000 && hx >= 0 {
        return x.sqrt();
    }
    let mut ax = fabsf(x);
    if ix == 0x7f80_0000 || ix == 0 || ix == 0x3f80_0000 {
        let mut z = ax;
        if hy < 0 {
            z = one / z;
        }
        if hx < 0 {
            if ((ix - 0x3f80_0000) | yisint) == 0 {
                z = (z - z) / (z - z);
            } else if yisint == 1 {
                z = -z;
            }
        }
        return z;
    }
    if ((((hx as u32) >> 31).wrapping_sub(1)) | (yisint as u32)) == 0 {
        return (x - x) / (x - x);
    }
    let t1: f32;
    let t2: f32;
    if iy > 0x4d00_0000 {
        if ix < 0x3f7f_fff8 {
            return if hy < 0 { huge * huge } else { tiny * tiny };
        }
        if ix > 0x3f80_0007 {
            return if hy > 0 { huge * huge } else { tiny * tiny };
        }
        let t = x - 1.0f32;
        let w = (t * t) * (0.5f32 - t * ((0.333333333333f64 as f32) - t * 0.25f32));
        let u = ivln2_h * t;
        let v = t * ivln2_l - w * ivln2;
        let mut tt1 = u + v;
        let is = fw(tt1);
        tt1 = sfw(is & 0xffff_f000u32 as i32);
        t1 = tt1;
        t2 = v - (t1 - u);
    } else {
        if iy < 0x2f80_0000 {
            y = sfw((hy & 0x8000_0000u32 as i32) | 0x2f80_0000);
        }
        let mut n: i32 = 0;
        if ix < 0x0080_0000 {
            ax *= two24;
            n -= 24;
            ix = fw(ax);
        }
        n += (ix >> 23) - 0x7f;
        let j = ix & 0x007f_ffff;
        ix = j | 0x3f80_0000;
        let k: i32;
        if j <= 0x1cc471 {
            k = 0;
        } else if j < 0x5db3d7 {
            k = 1;
        } else {
            k = 0;
            n += 1;
            ix -= 0x0080_0000;
        }
        ax = sfw(ix);
        let u = ax - bp[k as usize];
        let v = one / (ax + bp[k as usize]);
        let s = u * v;
        let mut s_h = s;
        let is = fw(s_h);
        s_h = sfw(is & 0xffff_f000u32 as i32);
        let mut t_h = sfw(((ix >> 1) | 0x2000_0000) + 0x0040000 + (k << 21));
        let t_l = ax - (t_h - bp[k as usize]);
        let s_l = v * ((u - s_h * t_h) - s_h * t_l);
        let mut s2 = s * s;
        let mut r = s2 * s2 * (l1 + s2 * (l2 + s2 * (l3 + s2 * (l4 + s2 * (l5 + s2 * l6)))));
        r += s_l * (s_h + s);
        s2 = s_h * s_h;
        t_h = 3.0f32 + s2 + r;
        let is = fw(t_h);
        t_h = sfw(is & 0xffff_f000u32 as i32);
        let t_l = r - ((t_h - 3.0f32) - s2);
        let u = s_h * t_h;
        let v = s_l * t_h + t_l * s;
        let mut p_h = u + v;
        let is = fw(p_h);
        p_h = sfw(is & 0xffff_f000u32 as i32);
        let p_l = v - (p_h - u);
        let z_h = cp_h * p_h;
        let z_l = cp_l * p_h + p_l * cp + dp_l[k as usize];
        let t = n as f32;
        let mut tt1 = ((z_h + z_l) + dp_h[k as usize]) + t;
        let is = fw(tt1);
        tt1 = sfw(is & 0xffff_f000u32 as i32);
        t1 = tt1;
        t2 = z_l - (((t1 - t) - dp_h[k as usize]) - z_h);
    }
    let mut s = one;
    if ((((hx as u32) >> 31).wrapping_sub(1)) | ((yisint - 1) as u32)) == 0 {
        s = -one;
    }
    let is = fw(y);
    let y1 = sfw(is & 0xffff_f000u32 as i32);
    let p_l = (y - y1) * t1 + y * t2;
    let mut p_h = y1 * t1;
    let mut z = p_l + p_h;
    let mut j = fw(z);
    if j > 0x4300_0000 {
        return s * huge * huge;
    } else if j == 0x4300_0000 {
        if p_l + ovt > z - p_h {
            return s * huge * huge;
        }
    } else if (j & 0x7fff_ffff) > 0x4316_0000 {
        return s * tiny * tiny;
    } else if (j as u32) == 0xc316_0000 && p_l <= z - p_h {
        return s * tiny * tiny;
    }
    let i = j & 0x7fff_ffff;
    let mut k = (i >> 23) - 0x7f;
    let mut n: i32 = 0;
    if i > 0x3f00_0000 {
        n = j.wrapping_add(0x0080_0000 >> (k + 1));
        k = ((n & 0x7fff_ffff) >> 23) - 0x7f;
        let t = sfw(n & !(0x007f_ffff >> k));
        n = ((n & 0x007f_ffff) | 0x0080_0000) >> (23 - k);
        if j < 0 {
            n = -n;
        }
        p_h -= t;
    }
    let mut t = p_l + p_h;
    let is = fw(t);
    t = sfw(is & 0xffff_f000u32 as i32);
    let u = t * lg2_h;
    let v = (p_l - (t - p_h)) * lg2 + t * lg2_l;
    z = u + v;
    let w = v - (z - u);
    t = z * z;
    let t1 = z - t * (p1 + t * (p2 + t * (p3 + t * (p4 + t * p5))));
    let r = (z * t1) / (t1 - two) - (w + z * w);
    z = one - (r - z);
    j = fw(z);
    j = j.wrapping_add(n << 23);
    if (j >> 23) <= 0 {
        z = scalbnf(z, n);
    } else {
        z = sfw(j);
    }
    s * z
}

/// glibc 2.19 `powf`.
fn powf_port(x: f32, y: f32) -> f32 {
    ieee754_powf(x, y)
}

// ---------------------------------------------------------------------------------------------
// f64: double-double fast path for sin / cos
// ---------------------------------------------------------------------------------------------

/// Double-double arithmetic used by the `sin`/`cos` fast path. Error bounds are deliberately
/// pessimistic; whenever the Ziv rounding test is not conclusive the exact [`big`] path is used.
mod dd {
    #[derive(Clone, Copy, Debug)]
    pub struct D {
        pub hi: f64,
        pub lo: f64,
    }

    #[inline]
    pub fn two_sum(a: f64, b: f64) -> (f64, f64) {
        let s = a + b;
        let bb = s - a;
        let e = (a - (s - bb)) + (b - bb);
        (s, e)
    }
    #[inline]
    fn fast_two_sum(a: f64, b: f64) -> (f64, f64) {
        let s = a + b;
        let e = b - (s - a);
        (s, e)
    }
    #[inline]
    fn split(a: f64) -> (f64, f64) {
        let t = 134217729.0 * a;
        let hi = t - (t - a);
        (hi, a - hi)
    }
    #[inline]
    fn two_prod(a: f64, b: f64) -> (f64, f64) {
        let p = a * b;
        let (ah, al) = split(a);
        let (bh, bl) = split(b);
        let e = ((ah * bh - p) + ah * bl + al * bh) + al * bl;
        (p, e)
    }
    #[inline]
    pub fn add(a: D, b: D) -> D {
        let (s, e) = two_sum(a.hi, b.hi);
        let (t, f) = two_sum(a.lo, b.lo);
        let e = e + t;
        let (s, e) = fast_two_sum(s, e);
        let e = e + f;
        let (hi, lo) = fast_two_sum(s, e);
        D { hi, lo }
    }
    #[inline]
    pub fn mul(a: D, b: D) -> D {
        let (p, e) = two_prod(a.hi, b.hi);
        let e = e + (a.hi * b.lo + a.lo * b.hi);
        let (hi, lo) = fast_two_sum(p, e);
        D { hi, lo }
    }
}

const PIO2_C1: u64 = 0x3ff921fb54400000;
const PIO2_C2: u64 = 0x3dd0b4611a600000;
const PIO2_C3: u64 = 0x3ba3198a2e000000;
const PIO2_C4: u64 = 0x397b839a25200000;
const TWO_OVER_PI_F64: u64 = 0x3fe45f306dc9c883;
const DD_SIN_COEF: [(u64, u64); 16] = [
    (0x3ff0000000000000, 0x0000000000000000),
    (0xbfc5555555555555, 0xbc65555555555555),
    (0x3f81111111111111, 0x3c01111111111111),
    (0xbf2a01a01a01a01a, 0xbb6a01a01a01a01a),
    (0x3ec71de3a556c734, 0xbb6c154f8ddc6c00),
    (0xbe5ae64567f544e4, 0x3afc062e06d1f209),
    (0x3de6124613a86d09, 0x3a8f28e0cc748ebe),
    (0xbd6ae7f3e733b81f, 0xb9e1d8656b0ee8cb),
    (0x3ce952c77030ad4a, 0x398ac981465ddc6c),
    (0xbc62f49b46814157, 0xb8f2650f61dbdcb4),
    (0x3bd71b8ef6dcf572, 0xb87d043ae40c4647),
    (0xbb4761b41316381a, 0x37d3423c7d91404f),
    (0x3ab3f3ccdd165fa9, 0xb7458ddadf344487),
    (0xba1d1ab1c2dccea3, 0xb6a054d0c78aea14),
    (0x398259f98b4358ad, 0x362eaf8c39dd9bc5),
    (0xb8e434d2e783f5bc, 0xb580b87b91be9aff),
];
const DD_COS_COEF: [(u64, u64); 16] = [
    (0x3ff0000000000000, 0x0000000000000000),
    (0xbfe0000000000000, 0x0000000000000000),
    (0x3fa5555555555555, 0x3c45555555555555),
    (0xbf56c16c16c16c17, 0x3bef49f49f49f49f),
    (0x3efa01a01a01a01a, 0x3b3a01a01a01a01a),
    (0xbe927e4fb7789f5c, 0xbb3cbbc05b4fa99a),
    (0x3e21eed8eff8d898, 0xbac2aec959e14c06),
    (0xbda93974a8c07c9d, 0xba305d6f8a2efd1f),
    (0x3d2ae7f3e733b81f, 0x39a1d8656b0ee8cb),
    (0xbca6827863b97d97, 0xb94eec01221a8b0b),
    (0x3c1e542ba4020225, 0x387ea72b4afe3c2f),
    (0xbb90ce396db7f853, 0x383aebcdbd20331c),
    (0x3aff2cf01972f578, 0xb789ada5fcc1ab14),
    (0xba688e85fc6a4e5a, 0x37071c37ebd16540),
    (0x39d0a18a2635085d, 0x366b9e2e28e1aa54),
    (0xb933932c5047d60e, 0xb5d832b7b530a627),
];

/// Evaluate sin (`want_cos == false`) or cos of `x` in double-double. Returns the correctly rounded
/// result when the Ziv test succeeds, `None` otherwise.
fn sincos_dd(x: f64, want_cos: bool) -> Option<f64> {
    use dd::{add, mul, two_sum, D};
    let ax = x.abs();
    if !(ax <= 1.6e6) {
        return None;
    }
    let (k, r): (i64, D) = if ax <= 0.7853981633974483 {
        (0, D { hi: x, lo: 0.0 })
    } else {
        let kf = (x * d(TWO_OVER_PI_F64)).round();
        let p1 = kf * d(PIO2_C1);
        let p2 = kf * d(PIO2_C2);
        let p3 = kf * d(PIO2_C3);
        let p4 = kf * d(PIO2_C4);
        let (s1, e1) = two_sum(x, -p1);
        let (s2, e2) = two_sum(s1, -p2);
        let (s3, e3) = two_sum(s2, -p3);
        let (s4, e4) = two_sum(s3, -p4);
        let lo = ((e1 + e2) + e3) + e4;
        let (hi, lo) = two_sum(s4, lo);
        (kf as i64, D { hi, lo })
    };
    let q = k.rem_euclid(4);
    // which function of r is needed
    let (use_cos, negate) = if want_cos {
        match q {
            0 => (true, false),
            1 => (false, true),
            2 => (true, true),
            _ => (false, false),
        }
    } else {
        match q {
            0 => (false, false),
            1 => (true, false),
            2 => (false, true),
            _ => (true, true),
        }
    };
    let u = mul(r, r);
    let coefs: &[(u64, u64); 16] = if use_cos { &DD_COS_COEF } else { &DD_SIN_COEF };
    let mut p = D { hi: d(coefs[15].0), lo: d(coefs[15].1) };
    for i in (0..15).rev() {
        p = add(mul(p, u), D { hi: d(coefs[i].0), lo: d(coefs[i].1) });
    }
    let res = if use_cos { p } else { mul(p, r) };
    let (mut hi, mut lo) = (res.hi, res.lo);
    if negate {
        hi = -hi;
        lo = -lo;
    }
    if !(hi.abs() > 1.0e-24) {
        return None;
    }
    // error bound: 2^-85 relative + 2^-97 absolute (reduction)
    let err = hi.abs() * 2.5849394142282115e-26 + 6.310887241768095e-30;
    let a = hi + (lo + err);
    let b = hi + (lo - err);
    if a == b {
        Some(a)
    } else {
        None
    }
}

// ---------------------------------------------------------------------------------------------
// f64: exact arbitrary-precision slow path (sin, cos, pow, atan2)
// ---------------------------------------------------------------------------------------------

/// Minimal arbitrary precision binary floating point (sign, i64 exponent, big mantissa) with
/// truncating arithmetic at a caller chosen precision `p` bits. Used by the correctly rounded
/// f64 functions; error budget analysis: every operation loses at most 1 ulp of `p` bits and the
/// callers keep at least 40 guard bits (see [`round_ziv`]).
mod big {
    use std::cmp::Ordering;
    use std::sync::OnceLock;

    type L = Vec<u64>;

    #[inline]
    fn trim(v: &mut L) {
        while let Some(&0) = v.last() {
            v.pop();
        }
    }
    pub fn bitlen(v: &[u64]) -> usize {
        match v.last() {
            None => 0,
            Some(&w) => 64 * (v.len() - 1) + (64 - w.leading_zeros() as usize),
        }
    }
    fn bit(v: &[u64], i: usize) -> bool {
        let w = i / 64;
        w < v.len() && (v[w] >> (i % 64)) & 1 == 1
    }
    /// any bit below position `n` set?
    fn low_nonzero(v: &[u64], n: usize) -> bool {
        let full = n / 64;
        for i in 0..full.min(v.len()) {
            if v[i] != 0 {
                return true;
            }
        }
        let rem = n % 64;
        if rem != 0 && full < v.len() && v[full] & ((1u64 << rem) - 1) != 0 {
            return true;
        }
        false
    }
    fn shl(v: &[u64], n: usize) -> L {
        if v.is_empty() {
            return vec![];
        }
        let limbs = n / 64;
        let b = n % 64;
        let mut r = vec![0u64; limbs];
        if b == 0 {
            r.extend_from_slice(v);
        } else {
            let mut carry = 0u64;
            for &w in v {
                r.push((w << b) | carry);
                carry = w >> (64 - b);
            }
            if carry != 0 {
                r.push(carry);
            }
        }
        r
    }
    fn shr(v: &[u64], n: usize) -> L {
        let limbs = n / 64;
        if limbs >= v.len() {
            return vec![];
        }
        let b = n % 64;
        let s = &v[limbs..];
        let mut r: L;
        if b == 0 {
            r = s.to_vec();
        } else {
            r = Vec::with_capacity(s.len());
            for i in 0..s.len() {
                let lo = s[i] >> b;
                let hi = if i + 1 < s.len() { s[i + 1] << (64 - b) } else { 0 };
                r.push(lo | hi);
            }
        }
        trim(&mut r);
        r
    }
    fn cmp_mag(a: &[u64], b: &[u64]) -> Ordering {
        if a.len() != b.len() {
            return a.len().cmp(&b.len());
        }
        for i in (0..a.len()).rev() {
            if a[i] != b[i] {
                return a[i].cmp(&b[i]);
            }
        }
        Ordering::Equal
    }
    fn add_mag(a: &[u64], b: &[u64]) -> L {
        let (a, b) = if a.len() >= b.len() { (a, b) } else { (b, a) };
        let mut r = Vec::with_capacity(a.len() + 1);
        let mut carry = 0u64;
        for i in 0..a.len() {
            let bi = if i < b.len() { b[i] } else { 0 };
            let (s1, c1) = a[i].overflowing_add(bi);
            let (s2, c2) = s1.overflowing_add(carry);
            r.push(s2);
            carry = (c1 as u64) + (c2 as u64);
        }
        if carry != 0 {
            r.push(carry);
        }
        r
    }
    /// `a - b`, requires `a >= b`.
    fn sub_mag(a: &[u64], b: &[u64]) -> L {
        let mut r = a.to_vec();
        sub_in_place(&mut r, b);
        r
    }
    fn sub_in_place(a: &mut L, b: &[u64]) {
        let mut borrow = 0u64;
        for i in 0..a.len() {
            let bi = if i < b.len() { b[i] } else { 0 };
            let (s1, c1) = a[i].overflowing_sub(bi);
            let (s2, c2) = s1.overflowing_sub(borrow);
            a[i] = s2;
            borrow = (c1 as u64) + (c2 as u64);
            if i + 1 >= b.len() && borrow == 0 {
                break;
            }
        }
        trim(a);
    }
    fn mul_mag(a: &[u64], b: &[u64]) -> L {
        if a.is_empty() || b.is_empty() {
            return vec![];
        }
        let mut r = vec![0u64; a.len() + b.len()];
        for i in 0..a.len() {
            let mut carry = 0u128;
            for j in 0..b.len() {
                let t = (a[i] as u128) * (b[j] as u128) + (r[i + j] as u128) + carry;
                r[i + j] = t as u64;
                carry = t >> 64;
            }
            r[i + b.len()] = carry as u64;
        }
        trim(&mut r);
        r
    }
    fn mul_small(a: &[u64], k: u64) -> L {
        let mut r = Vec::with_capacity(a.len() + 1);
        let mut carry = 0u128;
        for &w in a {
            let t = (w as u128) * (k as u128) + carry;
            r.push(t as u64);
            carry = t >> 64;
        }
        if carry != 0 {
            r.push(carry as u64);
        }
        trim(&mut r);
        r
    }
    fn divrem_small(a: &[u64], dv: u64) -> (L, u64) {
        let mut q = vec![0u64; a.len()];
        let mut rem = 0u128;
        for i in (0..a.len()).rev() {
            let cur = (rem << 64) | (a[i] as u128);
            q[i] = (cur / dv as u128) as u64;
            rem = cur % dv as u128;
        }
        trim(&mut q);
        (q, rem as u64)
    }
    /// floor(a / b), restoring binary long division.
    fn divide(a: &[u64], b: &[u64]) -> L {
        assert!(!b.is_empty());
        if cmp_mag(a, b) == Ordering::Less {
            return vec![];
        }
        let n = bitlen(a);
        let mut q = vec![0u64; a.len()];
        let mut r: L = Vec::with_capacity(b.len() + 1);
        for i in (0..n).rev() {
            // r = (r << 1) | bit
            let mut carry = bit(a, i) as u64;
            for w in r.iter_mut() {
                let nc = *w >> 63;
                *w = (*w << 1) | carry;
                carry = nc;
            }
            if carry != 0 {
                r.push(carry);
            }
            if cmp_mag(&r, b) != Ordering::Less {
                sub_in_place(&mut r, b);
                q[i / 64] |= 1u64 << (i % 64);
            }
        }
        trim(&mut q);
        q
    }
    /// floor(sqrt(n))
    fn isqrt(n: &[u64]) -> L {
        let bl = bitlen(n);
        if bl == 0 {
            return vec![];
        }
        let mut num = n.to_vec();
        let mut res: L = vec![];
        let mut bitv = shl(&[1], ((bl - 1) / 2) * 2);
        while !bitv.is_empty() {
            let t = add_mag(&res, &bitv);
            if cmp_mag(&num, &t) != Ordering::Less {
                sub_in_place(&mut num, &t);
                res = add_mag(&shr(&res, 1), &bitv);
            } else {
                res = shr(&res, 1);
            }
            bitv = shr(&bitv, 2);
        }
        res
    }

    /// value = (-1)^neg * m * 2^e; `m` empty means zero.
    #[derive(Clone, Debug)]
    pub struct F {
        pub neg: bool,
        pub e: i64,
        pub m: Vec<u64>,
    }

    impl F {
        pub fn zero() -> F {
            F { neg: false, e: 0, m: vec![] }
        }
        pub fn is_zero(&self) -> bool {
            self.m.is_empty()
        }
        pub fn from_u64(v: u64) -> F {
            if v == 0 {
                F::zero()
            } else {
                F { neg: false, e: 0, m: vec![v] }
            }
        }
        pub fn from_i64(v: i64) -> F {
            let mut f = F::from_u64(v.unsigned_abs());
            f.neg = v < 0 && !f.is_zero();
            f
        }
        /// exact conversion of a finite f64
        pub fn from_f64(x: f64) -> F {
            let bits = x.to_bits();
            let ef = ((bits >> 52) & 0x7ff) as i64;
            let frac = bits & 0x000f_ffff_ffff_ffff;
            let (m, e) = if ef == 0 { (frac, -1074) } else { (frac | (1u64 << 52), ef - 1075) };
            if m == 0 {
                return F::zero();
            }
            F { neg: (bits >> 63) != 0, e, m: vec![m] }
        }
        /// `top` such that 2^(top-1) <= |v| < 2^top (zero: very negative).
        pub fn top(&self) -> i64 {
            if self.m.is_empty() {
                i64::MIN / 4
            } else {
                self.e + bitlen(&self.m) as i64
            }
        }
        pub fn neg(mut self) -> F {
            if !self.m.is_empty() {
                self.neg = !self.neg;
            }
            self
        }
        pub fn abs(mut self) -> F {
            self.neg = false;
            self
        }
        pub fn ldexp(mut self, n: i64) -> F {
            if !self.m.is_empty() {
                self.e += n;
            }
            self
        }
        /// rough f64 value (top 64 bits); only for decisions on values of moderate magnitude
        pub fn approx(&self) -> f64 {
            let bl = bitlen(&self.m);
            if bl == 0 {
                return 0.0;
            }
            let (mant, e) = if bl > 64 {
                let sh = bl - 64;
                (shr(&self.m, sh)[0], self.e + sh as i64)
            } else {
                (self.m[0], self.e)
            };
            let mut v = mant as f64;
            let mut ee = e;
            while ee > 0 {
                let s = ee.min(1000);
                v *= f64::from_bits(((1023 + s) as u64) << 52);
                ee -= s;
            }
            while ee < 0 {
                let s = (-ee).min(1000);
                v *= f64::from_bits(((1023 - s) as u64) << 52);
                ee += s;
            }
            if self.neg {
                -v
            } else {
                v
            }
        }
    }

    pub struct Ctx {
        pub p: usize,
    }

    impl Ctx {
        pub fn norm(&self, mut x: F) -> F {
            let bl = bitlen(&x.m);
            if bl > self.p {
                let s = bl - self.p;
                x.m = shr(&x.m, s);
                x.e += s as i64;
            }
            if x.m.is_empty() {
                x.e = 0;
                x.neg = false;
            }
            x
        }
        pub fn add(&self, a: &F, b: &F) -> F {
            if a.m.is_empty() {
                return self.norm(b.clone());
            }
            if b.m.is_empty() {
                return self.norm(a.clone());
            }
            let (hi, lo) = if a.top() >= b.top() { (a, b) } else { (b, a) };
            if hi.top() - lo.top() > self.p as i64 + 8 {
                return self.norm(hi.clone());
            }
            let e = a.e.min(b.e);
            let ma = shl(&a.m, (a.e - e) as usize);
            let mb = shl(&b.m, (b.e - e) as usize);
            let r = if a.neg == b.neg {
                F { neg: a.neg, e, m: add_mag(&ma, &mb) }
            } else {
                match cmp_mag(&ma, &mb) {
                    Ordering::Equal => F::zero(),
                    Ordering::Greater => F { neg: a.neg, e, m: sub_mag(&ma, &mb) },
                    Ordering::Less => F { neg: b.neg, e, m: sub_mag(&mb, &ma) },
                }
            };
            self.norm(r)
        }
        pub fn sub(&self, a: &F, b: &F) -> F {
            let nb = F { neg: !b.neg && !b.m.is_empty(), e: b.e, m: b.m.clone() };
            self.add(a, &nb)
        }
        pub fn mul(&self, a: &F, b: &F) -> F {
            if a.m.is_empty() || b.m.is_empty() {
                return F::zero();
            }
            self.norm(F { neg: a.neg != b.neg, e: a.e + b.e, m: mul_mag(&a.m, &b.m) })
        }
        pub fn div(&self, a: &F, b: &F) -> F {
            assert!(!b.m.is_empty());
            if a.m.is_empty() {
                return F::zero();
            }
            let need = self.p + 2 + bitlen(&b.m);
            let s = need.saturating_sub(bitlen(&a.m));
            let q = divide(&shl(&a.m, s), &b.m);
            self.norm(F { neg: a.neg != b.neg, e: a.e - s as i64 - b.e, m: q })
        }
        pub fn mul_u64(&self, a: &F, k: u64) -> F {
            if a.m.is_empty() || k == 0 {
                return F::zero();
            }
            self.norm(F { neg: a.neg, e: a.e, m: mul_small(&a.m, k) })
        }
        pub fn div_u64(&self, a: &F, k: u64) -> F {
            if a.m.is_empty() {
                return F::zero();
            }
            let s = (self.p + 16).saturating_sub(bitlen(&a.m));
            let (q, _) = divrem_small(&shl(&a.m, s), k);
            self.norm(F { neg: a.neg, e: a.e - s as i64, m: q })
        }
        pub fn sqrt(&self, a: &F) -> F {
            if a.m.is_empty() {
                return F::zero();
            }
            let mut m = a.m.clone();
            let mut e = a.e;
            if e & 1 != 0 {
                m = shl(&m, 1);
                e -= 1;
            }
            let need = 2 * self.p + 4;
            let mut s = need.saturating_sub(bitlen(&m));
            if s & 1 != 0 {
                s += 1;
            }
            let m = shl(&m, s);
            e -= s as i64;
            self.norm(F { neg: false, e: e / 2, m: isqrt(&m) })
        }
        /// signed comparison
        pub fn cmp(&self, a: &F, b: &F) -> Ordering {
            let d = self.sub(a, b);
            if d.m.is_empty() {
                Ordering::Equal
            } else if d.neg {
                Ordering::Less
            } else {
                Ordering::Greater
            }
        }
    }

    /// Nearest integer (ties away from zero) of `a`: `(negative, magnitude)`.
    pub fn round_int(a: &F) -> (bool, L) {
        if a.m.is_empty() {
            return (false, vec![]);
        }
        let mag = if a.e >= 0 {
            shl(&a.m, a.e as usize)
        } else {
            let sh = (-a.e) as usize;
            let half = shl(&[1], sh - 1);
            shr(&add_mag(&a.m, &half), sh)
        };
        (a.neg && !mag.is_empty(), mag)
    }

    /// Round the positive value `a` to the nearest f64 (ties to even), including subnormals and
    /// overflow to infinity.
    pub fn rne(a: &F) -> f64 {
        if a.m.is_empty() {
            return 0.0;
        }
        let top = a.top();
        if top >= 1025 {
            return f64::INFINITY;
        }
        if top < -1077 {
            return 0.0;
        }
        let qpos = (top - 53).max(-1074);
        let shift = qpos - a.e;
        let k: u64 = if shift <= 0 {
            let v = shl(&a.m, (-shift) as usize);
            v.first().copied().unwrap_or(0)
        } else {
            let sh = shift as usize;
            let half = bit(&a.m, sh - 1);
            let sticky = low_nonzero(&a.m, sh - 1);
            let v = shr(&a.m, sh);
            let k0 = v.first().copied().unwrap_or(0);
            if half && (sticky || k0 & 1 == 1) {
                k0 + 1
            } else {
                k0
            }
        };
        if k == 0 {
            return 0.0;
        }
        let pw = if qpos >= -1022 {
            f64::from_bits(((qpos + 1023) as u64) << 52)
        } else {
            f64::from_bits(1u64 << (qpos + 1074))
        };
        (k as f64) * pw
    }

    const CPREC: usize = 2400;

    fn atan_inv(ctx: &Ctx, n: u64) -> F {
        // sum_{k>=0} (-1)^k / ((2k+1) n^(2k+1))
        let n2 = n * n;
        let mut term = ctx.div_u64(&F::from_u64(1), n);
        let mut sum = term.clone();
        let mut k = 1u64;
        loop {
            term = ctx.div_u64(&term, n2);
            if term.is_zero() {
                break;
            }
            let t = ctx.div_u64(&term, 2 * k + 1);
            sum = if k & 1 == 1 { ctx.sub(&sum, &t) } else { ctx.add(&sum, &t) };
            if term.top() < sum.top() - ctx.p as i64 - 8 {
                break;
            }
            k += 1;
        }
        sum
    }

    fn consts() -> &'static (F, F) {
        static C: OnceLock<(F, F)> = OnceLock::new();
        C.get_or_init(|| {
            let ctx = Ctx { p: CPREC };
            // pi = 16 atan(1/5) - 4 atan(1/239)
            let a = ctx.mul_u64(&atan_inv(&ctx, 5), 16);
            let b = ctx.mul_u64(&atan_inv(&ctx, 239), 4);
            let pi = ctx.sub(&a, &b);
            // ln2 = 2 atanh(1/3)
            let n2 = 9u64;
            let mut term = ctx.div_u64(&F::from_u64(1), 3);
            let mut sum = term.clone();
            let mut k = 1u64;
            loop {
                term = ctx.div_u64(&term, n2);
                if term.is_zero() {
                    break;
                }
                let t = ctx.div_u64(&term, 2 * k + 1);
                sum = ctx.add(&sum, &t);
                if term.top() < sum.top() - ctx.p as i64 - 8 {
                    break;
                }
                k += 1;
            }
            (pi, ctx.mul_u64(&sum, 2))
        })
    }
    pub fn pi(p: usize) -> F {
        Ctx { p }.norm(consts().0.clone())
    }
    pub fn ln2(p: usize) -> F {
        Ctx { p }.norm(consts().1.clone())
    }

    /// (sin r, cos r) by Taylor series, |r| < 1.
    fn sincos_series(ctx: &Ctx, r: &F) -> (F, F) {
        let r2 = ctx.mul(r, r);
        let guard = ctx.p as i64 + 12;
        // sin
        let mut term = r.clone();
        let mut s = r.clone();
        let mut i = 1u64;
        loop {
            term = ctx.div_u64(&ctx.mul(&term, &r2), (2 * i) * (2 * i + 1));
            if term.is_zero() {
                break;
            }
            term.neg = !term.neg;
            s = ctx.add(&s, &term);
            if term.top() < s.top() - guard {
                break;
            }
            i += 1;
        }
        // cos
        let mut term = F::from_u64(1);
        let mut c = F::from_u64(1);
        let mut i = 1u64;
        loop {
            term = ctx.div_u64(&ctx.mul(&term, &r2), (2 * i - 1) * (2 * i));
            if term.is_zero() {
                break;
            }
            term.neg = !term.neg;
            c = ctx.add(&c, &term);
            if term.top() < c.top() - guard {
                break;
            }
            i += 1;
        }
        (s, c)
    }

    /// sin(x) or cos(x) of a finite nonzero f64 as a value with ~p correct bits.
    pub fn sincos(x: f64, want_cos: bool, p: usize) -> F {
        let xf = F::from_f64(x);
        let top = xf.top().max(0) as usize;
        let pp = (top + p + 128).min(CPREC);
        let ctx = Ctx { p: pp };
        let pio2 = pi(pp).ldexp(-1);
        let q = ctx.div(&xf, &pio2);
        let (kneg, kmag) = round_int(&q);
        let k = F { neg: kneg, e: 0, m: kmag.clone() };
        let kp = ctx.mul(&k, &pio2);
        let r = ctx.sub(&xf, &kp);
        let low = kmag.first().copied().unwrap_or(0) & 3;
        let quad = if kneg { (4 - low) & 3 } else { low };
        let sctx = Ctx { p: p + 48 };
        let (s, c) = sincos_series(&sctx, &sctx.norm(r));
        let v = if want_cos {
            match quad {
                0 => c,
                1 => s.neg(),
                2 => c.neg(),
                _ => s,
            }
        } else {
            match quad {
                0 => s,
                1 => c,
                2 => s.neg(),
                _ => c.neg(),
            }
        };
        v
    }

    /// ln(a), a > 0
    pub fn ln(ctx: &Ctx, a: &F) -> F {
        let mut t = a.top();
        let mut f = a.clone().abs().ldexp(-t);
        if f.approx() < 0.7071067811865476 {
            f = f.ldexp(1);
            t -= 1;
        }
        let one = F::from_u64(1);
        let num = ctx.sub(&f, &one);
        let den = ctx.add(&f, &one);
        let s = ctx.div(&num, &den);
        let s2 = ctx.mul(&s, &s);
        let mut sum = s.clone();
        let mut pw = s.clone();
        let mut k = 1u64;
        if !s.is_zero() {
            loop {
                pw = ctx.mul(&pw, &s2);
                let term = ctx.div_u64(&pw, 2 * k + 1);
                if term.is_zero() {
                    break;
                }
                sum = ctx.add(&sum, &term);
                if term.top() < sum.top() - ctx.p as i64 - 12 {
                    break;
                }
                k += 1;
            }
        }
        let l = sum.ldexp(1);
        let tl = ctx.mul(&F::from_i64(t), &ln2(ctx.p));
        ctx.add(&tl, &l)
    }

    /// exp(z), |z| < 4096
    pub fn exp(ctx: &Ctx, z: &F) -> F {
        let l2 = ln2(ctx.p);
        let nf = ctx.div(z, &l2);
        let (nneg, nmag) = round_int(&nf);
        let n: i64 = {
            let m = nmag.first().copied().unwrap_or(0) as i64;
            if nneg {
                -m
            } else {
                m
            }
        };
        let r = ctx.sub(z, &ctx.mul(&F::from_i64(n), &l2));
        let r8 = r.ldexp(-8);
        let mut sum = F::from_u64(1);
        let mut term = F::from_u64(1);
        let mut i = 1u64;
        loop {
            term = ctx.div_u64(&ctx.mul(&term, &r8), i);
            if term.is_zero() {
                break;
            }
            sum = ctx.add(&sum, &term);
            if term.top() < -(ctx.p as i64) - 12 {
                break;
            }
            i += 1;
        }
        for _ in 0..8 {
            sum = ctx.mul(&sum, &sum);
        }
        sum.ldexp(n)
    }

    /// |x|^y for finite positive x != 1 and finite nonzero y. `Err(true)` overflow, `Err(false)` underflow.
    pub fn pow_pos(x: f64, y: f64, p: usize) -> Result<F, bool> {
        let ctx = Ctx { p: p + 48 };
        let lx = ln(&ctx, &F::from_f64(x));
        let z = ctx.mul(&F::from_f64(y), &lx);
        if z.is_zero() {
            return Ok(F::from_u64(1));
        }
        if z.top() > 12 {
            return Err(!z.neg);
        }
        Ok(exp(&ctx, &z))
    }

    /// atan(t), t >= 0. The second component is `-1` when the value is known to lie strictly
    /// below the returned approximation by less than the working precision resolves (tiny `t`,
    /// where atan(t) = t - t^3/3 +...), else 0.
    pub fn atan(ctx: &Ctx, t: &F) -> (F, i8) {
        if t.is_zero() {
            return (F::zero(), 0);
        }
        let one = F::from_u64(1);
        let inv = ctx.cmp(t, &one) == Ordering::Greater;
        if !inv && 2 * t.top() < -(ctx.p as i64) - 60 {
            return (t.clone(), -1);
        }
        let mut u = if inv { ctx.div(&one, t) } else { t.clone() };
        let mut halvings = 0i64;
        while u.top() > -3 {
            let u2 = ctx.mul(&u, &u);
            let s = ctx.sqrt(&ctx.add(&one, &u2));
            u = ctx.div(&u, &ctx.add(&one, &s));
            halvings += 1;
        }
        let u2 = ctx.mul(&u, &u);
        let mut sum = u.clone();
        let mut pw = u.clone();
        let mut k = 1u64;
        loop {
            pw = ctx.mul(&pw, &u2);
            let term = ctx.div_u64(&pw, 2 * k + 1);
            if term.is_zero() {
                break;
            }
            sum = if k & 1 == 1 { ctx.sub(&sum, &term) } else { ctx.add(&sum, &term) };
            if term.top() < sum.top() - ctx.p as i64 - 12 {
                break;
            }
            k += 1;
        }
        let res = sum.ldexp(halvings);
        if inv {
            (ctx.sub(&pi(ctx.p).ldexp(-1), &res), 0)
        } else {
            (res, 0)
        }
    }

    /// atan2 value for finite nonzero x, y: `atan2(y, x)` with ~p correct bits.
    pub fn atan2(y: f64, x: f64, p: usize) -> (F, i8) {
        let ctx = Ctx { p: p + 48 };
        let t = ctx.div(&F::from_f64(y).abs(), &F::from_f64(x).abs());
        let (a, bias) = atan(&ctx, &t);
        let (v, bias) = if x < 0.0 { (ctx.sub(&pi(ctx.p), &a), 0) } else { (a, bias) };
        (if y < 0.0 { v.neg() } else { v }, bias)
    }

    /// Ziv test: `v` has relative error below 2^-(p-40). Returns the correctly rounded f64 of
    /// `v` (sign included) when the rounding is unambiguous; with `last` an unresolvable case is
    /// treated as an exact midpoint (ties to even).
    pub fn round_ziv(v: &F, p: usize, last: bool, bias: i8) -> Option<f64> {
        if v.m.is_empty() {
            return Some(0.0);
        }
        let ctx = Ctx { p: p + 96 };
        let av = v.clone().abs();
        let eps = F { neg: false, e: av.top() - (p as i64 - 40), m: vec![1] };
        let lo = ctx.sub(&av, &eps);
        let hi = ctx.add(&av, &eps);
        let r1 = rne(&lo);
        let r2 = rne(&hi);
        let r = if r1.to_bits() == r2.to_bits() {
            r1
        } else if last {
            if bias < 0 {
                r1.min(r2)
            } else if bias > 0 {
                r1.max(r2)
            } else if r1.to_bits() & 1 == 0 {
                r1
            } else {
                r2
            }
        } else {
            return None;
        };
        Some(if v.neg { -r } else { r })
    }
}

/// Result of one precision level of a slow path evaluation.
enum Eval {
    /// approximation (relative error below 2^-(p-40)) plus tie bias (see `big::round_ziv`)
    Val(big::F, i8),
    /// already decided (overflow / underflow)
    Done(f64),
}

/// Correctly rounded evaluation helper: runs `g(p)` at increasing precision until the Ziv test passes.
fn ziv<G: Fn(usize) -> Eval>(g: G) -> f64 {
    let levels = [256usize, 512, 1024];
    for (i, &p) in levels.iter().enumerate() {
        match g(p) {
            Eval::Done(r) => return r,
            Eval::Val(v, bias) => {
                if let Some(r) = big::round_ziv(&v, p, i == levels.len() - 1, bias) {
                    return r;
                }
            }
        }
    }
    unreachable!("last Ziv level always decides")
}

/// Exact (slow-path only) `sin`; exposed for the parity tests, equals [`sin`].
#[doc(hidden)]
pub fn sin_exact(x: f64) -> f64 {
    if x.abs() < 1.4901161193847656e-8 || !x.is_finite() {
        return sin(x);
    }
    ziv(|p| Eval::Val(big::sincos(x, false, p), 0))
}
/// Exact (slow-path only) `cos`; exposed for the parity tests, equals [`cos`].
#[doc(hidden)]
pub fn cos_exact(x: f64) -> f64 {
    if x.abs() < 7.450580596923828e-9 || !x.is_finite() {
        return cos(x);
    }
    ziv(|p| Eval::Val(big::sincos(x, true, p), 0))
}

/// glibc 2.19 `sin` (correctly rounded).
fn sin_port(x: f64) -> f64 {
    let ax = x.abs();
    if !(ax < f64::INFINITY) {
        return x / x; // NaN (inf: invalid -> default NaN)
    }
    if ax < 1.4901161193847656e-8 {
        // |x| < 2^-26
        return x;
    }
    if let Some(r) = sincos_dd(x, false) {
        return r;
    }
    sin_exact(x)
}

/// glibc 2.19 `cos` (correctly rounded).
fn cos_port(x: f64) -> f64 {
    let ax = x.abs();
    if !(ax < f64::INFINITY) {
        return x / x;
    }
    if ax < 7.450580596923828e-9 {
        // |x| < 2^-27
        return 1.0;
    }
    if let Some(r) = sincos_dd(x, true) {
        return r;
    }
    cos_exact(x)
}

/// Exact `pow` core for finite `x > 0`, finite `y != 0`. Exposed for tests through [`pow_exact`].
fn pow_core_pos(x: f64, y: f64) -> f64 {
    if x == 1.0 {
        return 1.0;
    }
    ziv(|p| match big::pow_pos(x, y, p) {
        Ok(v) => Eval::Val(v, 0),
        Err(over) => Eval::Done(if over { f64::INFINITY } else { 0.0 }),
    })
}

/// `checkint` of sysdeps/ieee754/dbl-64/e_pow.c: 0 not an integer (or inf/NaN, or |x| < 2),
/// 1 even integer, -1 odd integer.
fn checkint(x: f64) -> i32 {
    let b = x.to_bits();
    let m = ((b >> 32) as u32 & 0x7fff_ffff) as i32;
    let n = b as u32 as i32;
    if m >= 0x7ff0_0000 {
        return 0;
    }
    if m >= 0x4340_0000 {
        return 1;
    }
    if m < 0x4000_0000 {
        return 0;
    }
    let k = (m >> 20) - 1023;
    if k == 52 {
        return if n & 1 != 0 { -1 } else { 1 };
    }
    if k > 20 {
        if n.wrapping_shl((k - 20) as u32) != 0 {
            return 0;
        }
        return if n.wrapping_shl((k - 21) as u32) != 0 { -1 } else { 1 };
    }
    if n != 0 {
        return 0;
    }
    if k == 20 {
        return if m & 1 != 0 { -1 } else { 1 };
    }
    if m.wrapping_shl((k + 12) as u32) != 0 {
        return 0;
    }
    if m.wrapping_shl((k + 11) as u32) != 0 {
        -1
    } else {
        1
    }
}

/// `__ieee754_pow` special case structure (sysdeps/ieee754/dbl-64/e_pow.c); the finite positive
/// core is the correctly rounded exact evaluation.
fn ieee754_pow(x: f64, y: f64) -> f64 {
    let xb = x.to_bits();
    let yb = y.to_bits();
    let (xh, xl) = ((xb >> 32) as u32, xb as u32);
    let (yh, yl) = ((yb >> 32) as u32, yb as u32);
    if yl == 0 {
        let qx = xh & 0x7fff_ffff;
        if (qx == 0x7ff0_0000 && xl != 0) || qx > 0x7ff0_0000 {
            return x;
        }
        if y == 1.0 {
            return x;
        }
        if y == 2.0 {
            return x * x;
        }
        if y == -1.0 {
            return 1.0 / x;
        }
        if y == 0.0 {
            return 1.0;
        }
    }
    if (((xh as i32) > 0 && xh < 0x7ff0_0000) || (xh == 0 && xl != 0)) && (yh & 0x7fff_ffff) < 0x4ff0_0000 {
        if y == 0.5 {
            return x.sqrt();
        }
        return pow_core_pos(x, y);
    }
    if x == 0.0 {
        if ((yh & 0x7fff_ffff) == 0x7ff0_0000 && yl != 0) || (yh & 0x7fff_ffff) > 0x7ff0_0000 {
            return y;
        }
        if y.abs() > 1.0e20 {
            return if y > 0.0 { 0.0 } else { f64::INFINITY };
        }
        let k = checkint(y);
        if k == -1 {
            return if y < 0.0 { 1.0 / x } else { x };
        } else {
            return if y < 0.0 { f64::INFINITY } else { 0.0 };
        }
    }
    let qx = xh & 0x7fff_ffff;
    let qy = yh & 0x7fff_ffff;
    if qx >= 0x7ff0_0000 && (qx > 0x7ff0_0000 || xl != 0) {
        return x;
    }
    if qy >= 0x7ff0_0000 && (qy > 0x7ff0_0000 || yl != 0) {
        return if x == 1.0 { 1.0 } else { y };
    }
    if (xh as i32) < 0 {
        let k = checkint(y);
        if k == 0 {
            if qy == 0x7ff0_0000 {
                if x == -1.0 {
                    return 1.0;
                } else if x > -1.0 {
                    return if (yh as i32) < 0 { f64::INFINITY } else { 0.0 };
                } else {
                    return if (yh as i32) < 0 { 0.0 } else { f64::INFINITY };
                }
            } else if qx == 0x7ff0_0000 {
                return if y < 0.0 { 0.0 } else { f64::INFINITY };
            }
            return (x - x) / (x - x); // y not integer and x < 0
        } else if qx == 0x7ff0_0000 {
            if k < 0 {
                return if y < 0.0 { -0.0 } else { f64::NEG_INFINITY };
            } else {
                return if y < 0.0 { 0.0 } else { f64::INFINITY };
            }
        }
        return if k == 1 { ieee754_pow(-x, y) } else { -ieee754_pow(-x, y) };
    }
    // x > 0
    if qx == 0x7ff0_0000 {
        return if y > 0.0 { x } else { 0.0 };
    }
    if qy > 0x45f0_0000 && qy < 0x7ff0_0000 {
        if x == 1.0 {
            return 1.0;
        }
        if y > 0.0 {
            return if x > 1.0 { f64::INFINITY } else { 0.0 };
        }
        if y < 0.0 {
            return if x < 1.0 { f64::INFINITY } else { 0.0 };
        }
    }
    if x == 1.0 {
        return 1.0;
    }
    if y > 0.0 {
        return if x > 1.0 { f64::INFINITY } else { 0.0 };
    }
    if y < 0.0 {
        return if x < 1.0 { f64::INFINITY } else { 0.0 };
    }
    0.0
}

/// glibc 2.19 `pow` (correctly rounded; special values of `__ieee754_pow` plus the `_POSIX_` mode
/// `w_pow.c` wrapper, whose only value change is `pow(NaN, 0) = 1`).
fn pow_port(x: f64, y: f64) -> f64 {
    let z = ieee754_pow(x, y);
    if !z.is_finite() && x.is_nan() && y == 0.0 {
        return 1.0;
    }
    z
}

/// Exact `pow` slow path only (finite `x > 0`, finite `y != 0`; tests).
#[doc(hidden)]
pub fn pow_exact(x: f64, y: f64) -> f64 {
    pow_core_pos(x, y)
}

/// glibc 2.19 `atan2` (correctly rounded, C99 special values).
pub fn atan2(y: f64, x: f64) -> f64 {
    const PI: f64 = f64::from_bits(0x400921fb54442d18);
    const HPI: f64 = f64::from_bits(0x3ff921fb54442d18);
    const QPI: f64 = f64::from_bits(0x3fe921fb54442d18);
    const TQPI: f64 = f64::from_bits(0x4002d97c7f3321d2);
    if x.is_nan() || y.is_nan() {
        // `x + y` of the C source: the first operand (x) wins, quieted.
        let q = |v: f64| f64::from_bits(v.to_bits() | 0x0008_0000_0000_0000);
        return if x.is_nan() { q(x) } else { q(y) };
    }
    let ys = y.is_sign_negative();
    let xs = x.is_sign_negative();
    let sgn = |v: f64| if ys { -v } else { v };
    if y == 0.0 {
        return if !xs { y } else { sgn(PI) };
    }
    if x == 0.0 {
        return sgn(HPI);
    }
    if x.is_infinite() {
        if y.is_infinite() {
            return if !xs { sgn(QPI) } else { sgn(TQPI) };
        }
        return if !xs { sgn(0.0) } else { sgn(PI) };
    }
    if y.is_infinite() {
        return sgn(HPI);
    }
    ziv(|p| {
        let (v, b) = big::atan2(y, x, p);
        Eval::Val(v, b)
    })
}

/// Exact `atan2` slow path only (tests).
#[doc(hidden)]
pub fn atan2_exact(y: f64, x: f64) -> f64 {
    ziv(|p| {
        let (v, b) = big::atan2(y, x, p);
        Eval::Val(v, b)
    })
}

/// Correctly rounded constants used by the special cases (tests).
#[doc(hidden)]
pub fn pi_rounded_multiples() -> [f64; 4] {
    let c = big::Ctx { p: 400 };
    let pi = big::pi(400);
    let q = pi.clone().ldexp(-2);
    let tq = c.mul_u64(&q, 3);
    [big::rne(&pi), big::rne(&pi.clone().ldexp(-1)), big::rne(&q), big::rne(&tq)]
}

// ---------------------------------------------------------------------------------------------
// public entry points of the ported functions: x86-64 NaN results on every host
// ---------------------------------------------------------------------------------------------

/// [`sin_port`] with x86-64's default NaN for NaNs it creates (see [`x86_nan`]).
pub fn sin(x: f64) -> f64 {
    x86_nan(sin_port(x), x.is_nan())
}

/// [`cos_port`] with x86-64's default NaN for NaNs it creates (see [`x86_nan`]).
pub fn cos(x: f64) -> f64 {
    x86_nan(cos_port(x), x.is_nan())
}

/// [`pow_port`] with x86-64's default NaN for NaNs it creates (see [`x86_nan`]).
pub fn pow(x: f64, y: f64) -> f64 {
    x86_nan(pow_port(x, y), x.is_nan() || y.is_nan())
}

/// [`sinf_port`] with x86-64's default NaN for NaNs it creates (see [`x86_nan`]).
pub fn sinf(x: f32) -> f32 {
    x86_nanf(sinf_port(x), x.is_nan())
}

/// [`cosf_port`] with x86-64's default NaN for NaNs it creates (see [`x86_nan`]).
pub fn cosf(x: f32) -> f32 {
    x86_nanf(cosf_port(x), x.is_nan())
}

/// [`tanf_port`] with x86-64's default NaN for NaNs it creates (see [`x86_nan`]).
pub fn tanf(x: f32) -> f32 {
    x86_nanf(tanf_port(x), x.is_nan())
}

/// [`powf_port`] with x86-64's default NaN for NaNs it creates (see [`x86_nan`]).
pub fn powf(x: f32, y: f32) -> f32 {
    x86_nanf(powf_port(x, y), x.is_nan() || y.is_nan())
}
