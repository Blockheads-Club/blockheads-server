//! owned by worldgen
//!
//! Perlin noise kernels: `perlin_noise_2D`, `noise2`, `perlin_noise_3D`, `noise3`.
//!
//!  perlin_noise_2D";. Noise"
//! (3.1 `noise3`, 3.2 `noise2`, 3.3 `perlin_noise_3D`.

use crate::fmath::cconv::f64_to_i32;

use super::noise::NoiseFunctionStruct;

/// Per-axis setup shared by `noise2` / `noise3`: `(b0, b1, r0, r1)`.
///
/// `t = 4096.0 + v`; untiled: `b0 = (int)t & 0x3ff`, `b1 = (b0 + 1) & 0x3ff`; tiled: both reduced `% level`
/// (`b1` uses the already reduced `b0`; signed 64-bit modulo, `level >= 1`).
#[inline]
fn axis(v: f64, tiling: bool, level: i32) -> (usize, usize, f64, f64) {
    let t = 4096.0 + v;
    let ti = f64_to_i32(t);
    let (b0, b1) = if !tiling {
        let b0 = ti & 0x3ff;
        (b0, (b0 + 1) & 0x3ff)
    } else {
        let l = level as i64;
        let b0 = ((ti & 0x3ff) as i64 % l) as i32;
        (b0, ((((b0 + 1) & 0x3ff) as i64) % l) as i32)
    };
    let r0 = t - (ti as f64);
    let r1 = r0 - 1.0;
    (b0 as usize, b1 as usize, r0, r1)
}

/// `noise2(vec, s, xlevel, ylevel)`.
pub fn noise2(vec: &[f64; 2], s: &NoiseFunctionStruct, xlevel: i32, ylevel: i32) -> f64 {
    let (bx0, bx1, rx0, rx1) = axis(vec[0], s.tiling_x, xlevel);
    let (by0, by1, ry0, ry1) = axis(vec[1], s.tiling_y, ylevel);
    let sx = rx0 * rx0 * (3.0 - 2.0 * rx0);
    let sy = ry0 * ry0 * (3.0 - 2.0 * ry0);
    let i0 = s.p[bx0] as usize;
    let i1 = s.p[bx1] as usize;
    let g00 = &s.g2[s.p[i0 + by0] as usize];
    let g10 = &s.g2[s.p[i1 + by0] as usize];
    let g01 = &s.g2[s.p[i0 + by1] as usize];
    let g11 = &s.g2[s.p[i1 + by1] as usize];
    let mut a = rx0 * g00[0] + ry0 * g00[1];
    a = a + sx * ((rx1 * g10[0] + ry0 * g10[1]) - a);
    let b = rx0 * g01[0] + ry1 * g01[1];
    a + sy * ((b + sx * ((rx1 * g11[0] + ry1 * g11[1]) - b)) - a)
}

/// `perlin_noise_2D(vec, s, octaves)`: mutates `vec` (scratch). `octaves` is cast to `u32` as in the
/// original (a non-positive count loops ~4e9 times; callers clamp).
pub fn perlin_noise_2d(vec: &mut [f64; 2], s: &NoiseFunctionStruct, octaves: i32) -> f64 {
    let mut result = 0.0f64;
    let mut amp = s.amplitude;
    let fx: u32 = 1u32 << ((s.frequency_x as u32) & 31);
    let fy: u32 = 1u32 << ((s.frequency_y as u32) & 31);
    vec[0] *= fx as f64;
    vec[1] *= fy as f64;
    let n = octaves as u32;
    let mut i: u32 = 0;
    while i < n {
        let mut xl = fx.wrapping_shl(i & 31) as i32;
        let mut yl = fy.wrapping_shl(i & 31) as i32;
        if xl < 1 {
            xl = 1;
        }
        if yl < 1 {
            yl = 1;
        }
        result += noise2(vec, s, xl, yl) * amp;
        vec[0] *= 2.0;
        vec[1] *= 2.0;
        amp *= s.persistance;
        i += 1;
    }
    result
}

/// `noise3(vec, s, xlevel, ylevel, zlevel)`.
pub fn noise3(vec: &[f64; 3], s: &NoiseFunctionStruct, xlevel: i32, ylevel: i32, zlevel: i32) -> f64 {
    let (bx0, bx1, rx0, rx1) = axis(vec[0], s.tiling_x, xlevel);
    let (by0, by1, ry0, ry1) = axis(vec[1], s.tiling_y, ylevel);
    let (bz0, bz1, rz0, rz1) = axis(vec[2], s.loop_, zlevel);
    let sx = rx0 * rx0 * (3.0 - 2.0 * rx0);
    let sy = ry0 * ry0 * (3.0 - 2.0 * ry0);
    let sz = rz0 * rz0 * (3.0 - 2.0 * rz0);
    // G(ix, iy, iz) = &g3[p[p[ix] + iy] + iz]
    let g = |ix: usize, iy: usize, iz: usize| -> &[f64; 3] { &s.g3[s.p[s.p[ix] as usize + iy] as usize + iz] };
    let d = |g: &[f64; 3], rx: f64, ry: f64, rz: f64| -> f64 { rx * g[0] + ry * g[1] + rz * g[2] };

    let mut a = d(g(bx0, by0, bz0), rx0, ry0, rz0);
    a = a + sx * (d(g(bx1, by0, bz0), rx1, ry0, rz0) - a);
    let c = d(g(bx0, by1, bz0), rx0, ry1, rz0);
    let c2 = d(g(bx1, by1, bz0), rx1, ry1, rz0);
    a = a + sy * ((c + sx * (c2 - c)) - a);
    let mut dd = d(g(bx0, by0, bz1), rx0, ry0, rz1);
    dd = dd + sx * (d(g(bx1, by0, bz1), rx1, ry0, rz1) - dd);
    let e = d(g(bx0, by1, bz1), rx0, ry1, rz1);
    let e2 = d(g(bx1, by1, bz1), rx1, ry1, rz1);
    let b = dd + sy * ((e + sx * (e2 - e)) - dd);
    a + sz * (b - a)
}

/// `perlin_noise_3D(vec, s, octaves)`: mutates `vec` (scratch).
///
/// The seamless-tiling blend (`div > i`, negative frequencies) is dead code for every `NoiseFunction` because the
/// initialisers clamp frequencies to >= 1 (R2), so only the plain path is implemented.
pub fn perlin_noise_3d(vec: &mut [f64; 3], s: &NoiseFunctionStruct, octaves: i32) -> f64 {
    debug_assert!(s.frequency_x >= 1 && s.frequency_y >= 1 && s.frequency_z >= 1);
    let mut result = 0.0f64;
    let mut amp = s.amplitude;
    let px: u32 = 1u32 << ((s.frequency_x as u32) & 31);
    let py: u32 = 1u32 << ((s.frequency_y as u32) & 31);
    let pz: u32 = 1u32 << ((s.frequency_z as u32) & 31);
    vec[0] *= px as f64;
    vec[1] *= py as f64;
    vec[2] *= pz as f64;
    let mut i: i32 = 0;
    while i < octaves {
        let sh = (i as u32) & 0x1f;
        let mut lx = px.wrapping_shl(sh) as i32;
        let mut ly = py.wrapping_shl(sh) as i32;
        let mut lz = pz.wrapping_shl(sh) as i32;
        if lx < 1 {
            lx = 1;
        }
        if ly < 1 {
            ly = 1;
        }
        if lz < 1 {
            lz = 1;
        }
        let value = noise3(vec, s, lx, ly, lz);
        result += value * amp;
        vec[0] *= 2.0;
        vec[1] *= 2.0;
        vec[2] *= 2.0;
        amp *= s.persistance;
        i += 1;
    }
    result
}

#[cfg(test)]
mod tests {
    use super::super::noise::NoiseFunction;
    use super::*;

    fn nf(tile: bool) -> NoiseFunction {
        NoiseFunction::init_with_frequency_x_frequency_y_frequency_z_amplitude_seed_tileable_loop_persistance(
            4, 4, 4, 1.0, 777, tile, false, 1.0,
        )
    }

    /// : with tiling and level 1 the lattice indices collapse to 0; without it they do not.
    #[test]
    fn noise2_tiling_levels() {
        let t = nf(true);
        let u = nf(false);
        let v = [3.37, 5.81];
        let tiled = noise2(&v, t.struct_ref(), 1, 1);
        assert!(tiled.is_finite());
        let plain = noise2(&v, u.struct_ref(), 1, 1);
        // same seed => same tables, different indexing
        assert_ne!(tiled.to_bits(), plain.to_bits());
    }

    #[test]
    fn perlin_2d_consumes_vec() {
        let n = nf(true);
        let mut v = [0.25, 0.5];
        let r = perlin_noise_2d(&mut v, n.struct_ref(), 3);
        assert!(r.is_finite());
        assert_eq!(v[0], 0.25 * 16.0 * 8.0);
        assert_eq!(v[1], 0.5 * 16.0 * 8.0);
    }

    #[test]
    fn perlin_3d_runs() {
        let n = nf(true);
        let a = n.get_x_y_z_octaves(0.1, 0.2, 0.3, 4);
        let b = n.get_x_y_z_octaves(0.1, 0.2, 0.3, 4);
        assert_eq!(a.to_bits(), b.to_bits());
        assert!(a.abs() < 4.0);
    }

    /// Many octaves shift the lattice size out of range: the sizes are clamped to one.
    #[test]
    fn deep_octaves_clamp_the_lattice() {
        let n = nf(false);
        let mut v = [0.25, 0.5];
        assert!(perlin_noise_2d(&mut v, n.struct_ref(), 40).is_finite());
        let mut v3 = [0.25, 0.5, 0.75];
        assert!(perlin_noise_3d(&mut v3, n.struct_ref(), 40).is_finite());
    }
}
