//! owned by worldgen
//!
//! `NoiseFunction` (Perlin parameter holder) and its `NoiseFunctionStruct` table block.
//!

use std::rc::Rc;

use crate::fmath::cconv::f64_to_i32;
use crate::fmath::libm::sqrt;
use crate::rng::mt19937;

use super::perlin::{perlin_noise_2d, perlin_noise_3d};

/// `NoiseFunctionStruct` (C: 0x16080 bytes).
///
/// `p`, `g3`, `g2` hold 2050 entries: the first 1024 are generated, `[1024 + k] = [k]` for `k` in `0..=1025`
/// (R1, deterministic because the duplicate loop runs ascending).
pub struct NoiseFunctionStruct {
    /// Permutation table `p[2050]`.
    pub p: [i32; 2050],
    /// 3D gradients `g3[2050][3]`.
    pub g3: [[f64; 3]; 2050],
    /// 2D gradients `g2[2050][2]`.
    pub g2: [[f64; 2]; 2050],
    /// `frequencyX` (clamped to >= 1 by the initialisers).
    pub frequency_x: i32,
    /// `frequencyY`.
    pub frequency_y: i32,
    /// `frequencyZ`.
    pub frequency_z: i32,
    /// `amplitude`.
    pub amplitude: f64,
    /// `persistance` (stored as `0.5 * argument`, R3).
    pub persistance: f64,
    /// `tilingX`.
    pub tiling_x: bool,
    /// `tilingY`.
    pub tiling_y: bool,
    /// `loop` (tiling along Z).
    pub loop_: bool,
}

impl NoiseFunctionStruct {
    /// Zeroed block on the heap (the original `malloc`s without zeroing; see UB-PARITY note in the init).
    fn new_boxed() -> Box<NoiseFunctionStruct> {
        Box::new(NoiseFunctionStruct {
            p: [0; 2050],
            g3: [[0.0; 3]; 2050],
            g2: [[0.0; 2]; 2050],
            frequency_x: 0,
            frequency_y: 0,
            frequency_z: 0,
            amplitude: 0.0,
            persistance: 0.0,
            tiling_x: false,
            tiling_y: false,
            loop_: false,
        })
    }
}

/// `NoiseFunction` object (ivars `_tileable` (never written) and `_structPtr`). Shared as `Rc<NoiseFunction>`.
pub struct NoiseFunction {
    s: Box<NoiseFunctionStruct>,
}

impl NoiseFunction {
    /// `-initWithFrequencyX:frequencyY:frequencyZ:amplitude:seed:tileableX:tileableY:loop:persistance:`.
    ///
    /// Side effect: reseeds the global Mersenne Twister with `(long)seed` and draws exactly 6143 `fn_rand()` values
    /// (per `i < 1024`: 2 for `g2`, then 3 for `g3`; then 1023 for the shuffle).
    pub fn init_with_frequency_x_frequency_y_frequency_z_amplitude_seed_tileable_x_tileable_y_loop_persistance(
        fx: i32,
        fy: i32,
        fz: i32,
        amplitude: f64,
        seed: i32,
        tileable_x: bool,
        tileable_y: bool,
        loop_: bool,
        persistance: f64,
    ) -> NoiseFunction {
        let mut s = NoiseFunctionStruct::new_boxed();
        s.frequency_x = if fx < 2 { 1 } else { fx };
        s.frequency_y = if fy < 2 { 1 } else { fy };
        s.frequency_z = if fz < 2 { 1 } else { fz };
        s.amplitude = amplitude;
        s.tiling_x = tileable_x;
        s.tiling_y = tileable_y;
        s.loop_ = loop_;
        s.persistance = 0.5 * persistance;
        // `fn_seed_rand((long)seed)`: sign-extended.
        mt19937::fn_seed_rand(seed as i64 as u64);
        for i in 0..0x400usize {
            s.p[i] = i as i32;
            for j in 0..2 {
                s.g2[i][j] = (mt19937::fn_rand_tagged("worldgen::noise_init_g2") * 2048.0 - 1024.0) / 1024.0;
            }
            let len = sqrt(s.g2[i][0] * s.g2[i][0] + s.g2[i][1] * s.g2[i][1]);
            let sc = 1.0 / len;
            s.g2[i][0] *= sc;
            s.g2[i][1] *= sc;
            for j in 0..3 {
                s.g3[i][j] = (mt19937::fn_rand_tagged("worldgen::noise_init_g3") * 2048.0 - 1024.0) / 1024.0;
            }
            let len = sqrt(s.g3[i][0] * s.g3[i][0] + s.g3[i][1] * s.g3[i][1] + s.g3[i][2] * s.g3[i][2]);
            let sc = 1.0 / len;
            s.g3[i][0] *= sc;
            s.g3[i][1] *= sc;
            s.g3[i][2] *= sc;
        }
        // Shuffle: i runs 1023 down to 1.
        let mut i = 0x400usize;
        loop {
            i -= 1;
            if i == 0 {
                break;
            }
            let k = s.p[i];
            // `fn_rand()` is in [0,1] inclusive: j may be 1024 (probability 2^-32).
            // UB-PARITY: the original reads uninitialised `p[1024]` there; the zeroed table gives 0 (R1).
            let j = f64_to_i32(mt19937::fn_rand_tagged("worldgen::noise_init_shuffle") * 1024.0) as usize;
            s.p[i] = s.p[j];
            s.p[j] = k;
        }
        // Duplicate tables, ascending (R1: `p[1024]` is overwritten with `p[0]` before it is copied on).
        for i in 0..0x402usize {
            s.p[i + 0x400] = s.p[i];
            s.g2[i + 0x400] = s.g2[i];
            s.g3[i + 0x400] = s.g3[i];
        }
        NoiseFunction { s }
    }

    /// `-initWithFrequencyX:frequencyY:frequencyZ:amplitude:seed:tileable:loop:persistance:`:
    /// `tileableX = tileableY = tileable`.
    pub fn init_with_frequency_x_frequency_y_frequency_z_amplitude_seed_tileable_loop_persistance(
        fx: i32,
        fy: i32,
        fz: i32,
        amplitude: f64,
        seed: i32,
        tileable: bool,
        loop_: bool,
        persistance: f64,
    ) -> NoiseFunction {
        Self::init_with_frequency_x_frequency_y_frequency_z_amplitude_seed_tileable_x_tileable_y_loop_persistance(
            fx,
            fy,
            fz,
            amplitude,
            seed,
            tileable,
            tileable,
            loop_,
            persistance,
        )
    }

    /// The table block (read only after init).
    pub fn struct_ref(&self) -> &NoiseFunctionStruct {
        &self.s
    }

    /// `-getX:Y:octaves:`: `octaves < 2 -> 1`; `perlin_noise_2D` on a scratch `[x, y]`.
    pub fn get_x_y_octaves(&self, x: f64, y: f64, octaves: i32) -> f64 {
        let octaves = if octaves < 2 { 1 } else { octaves };
        let mut vec = [x, y];
        perlin_noise_2d(&mut vec, &self.s, octaves)
    }

    /// `-getX:Y:Z:octaves:`: `octaves < 2 -> 1`; `perlin_noise_3D` on a scratch `[x, y, z]`.
    pub fn get_x_y_z_octaves(&self, x: f64, y: f64, z: f64, octaves: i32) -> f64 {
        let octaves = if octaves < 2 { 1 } else { octaves };
        let mut vec = [x, y, z];
        perlin_noise_3d(&mut vec, &self.s, octaves)
    }
}

/// Nil-safe message send `[nf getX:y:octaves:]`: a nil receiver gives `0.0`.
pub fn noise_xy(nf: &Option<Rc<NoiseFunction>>, x: f64, y: f64, octaves: i32) -> f64 {
    match nf {
        Some(n) => n.get_x_y_octaves(x, y, octaves),
        None => 0.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// : heightNoiseFunctionA of the W=32 seed-12345 world.
    fn height_a() -> NoiseFunction {
        NoiseFunction::init_with_frequency_x_frequency_y_frequency_z_amplitude_seed_tileable_loop_persistance(
            -1, 3, 2, 0.9, 12346, true, false, 1.2,
        )
    }

    #[test]
    fn init_tables_match_golden() {
        let n = height_a();
        let s = n.struct_ref();
        assert_eq!((s.frequency_x, s.frequency_y, s.frequency_z), (1, 3, 2));
        assert_eq!(s.amplitude, 0.9);
        assert_eq!(s.persistance, 0.6);
        assert!(s.tiling_x && s.tiling_y && !s.loop_);
        assert_eq!(&s.p[0..6], &[112, 725, 588, 729, 83, 988]);
        assert_eq!(&s.p[1020..1028], &[698, 973, 335, 1022, 112, 725, 588, 729]);
        assert_eq!(&s.p[2046..2050], &[335, 1022, 112, 725]);
        assert_eq!(s.p[2048], s.p[0]);
        for k in 0..=1025 {
            assert_eq!(s.p[1024 + k], s.p[k]);
        }
        assert_eq!(s.g2[0][0], 0.99131237961169905);
        assert_eq!(s.g2[0][1], -0.13152857495081008);
        assert_eq!(s.g2[1], [0.7942675197077782, 0.60756819134748496]);
        assert_eq!(s.g2[1023], [-0.64698711312130597, -0.76250093472399005]);
        assert_eq!(s.g2[2049], s.g2[1]);
        assert_eq!(s.g3[0], [-0.090474445112369975, -0.87063250106122547, -0.48354257607524492]);
        assert_eq!(s.g3[1023], [-0.6249169069357553, 0.69556596921768654, 0.35449519304513705]);
    }

    #[test]
    fn init_consumes_6143_draws() {
        let _ = height_a();
        let next = mt19937::fn_rand();
        mt19937::fn_seed_rand(12346);
        let mut last = 0.0;
        for _ in 0..6144 {
            last = mt19937::fn_rand();
        }
        assert_eq!(next.to_bits(), last.to_bits());
    }

    #[test]
    fn height_b_table() {
        let b = NoiseFunction::init_with_frequency_x_frequency_y_frequency_z_amplitude_seed_tileable_loop_persistance(
            0, 4, 4, 0.9, 12348, true, false, 1.2,
        );
        let s = b.struct_ref();
        assert_eq!(&s.p[0..6], &[698, 945, 89, 166, 180, 600]);
        assert_eq!(s.g2[0], [-0.44962509128409672, -0.8932173740404783]);
        assert_eq!(s.g3[0], [0.23577174209939294, -0.74732402424437461, -0.62122338044749326]);
    }

    #[test]
    fn get_x_y_octaves_golden() {
        // : both objects exist at the same time (re-initialising reseeds the global MT, harmless).
        let a = height_a();
        let b = NoiseFunction::init_with_frequency_x_frequency_y_frequency_z_amplitude_seed_tileable_loop_persistance(
            0, 4, 4, 0.9, 12348, true, false, 1.2,
        );
        let chk = |got: f64, want: f64| assert_eq!(got.to_bits(), want.to_bits(), "{got:?} vs {want:?}");
        chk(a.get_x_y_octaves(0.10000000149011612, 0.5, 3), 0.18642225994889272);
        chk(a.get_x_y_octaves(0.10097656399011612, 0.5, 3), 0.19191435990697214);
        chk(b.get_x_y_octaves(0.070000000298023224, 0.5, 5), -0.13822985828687345);
        chk(b.get_x_y_octaves(0.070976562798023224, 0.5, 5), -0.14349230878186828);
        chk(b.get_x_y_octaves(0.05000000074505806, 0.75, 9), -0.064597337309828862);
        chk(b.get_x_y_octaves(0.05097656324505806, 0.75, 9), -0.056800798828157084);
        chk(b.get_x_y_octaves(0.070000000298023224, 0.5, 3), -0.11482927253187486);
        chk(b.get_x_y_octaves(0.076835937798023224, 0.5, 5), -0.16766140594757284);
        chk(a.get_x_y_octaves(0.10683593899011612, 0.5, 3), 0.22528378385995917);
        chk(b.get_x_y_octaves(0.05683593824505806, 0.75, 9), -0.044521982964978474);
    }

    #[test]
    fn nil_noise_is_zero() {
        assert_eq!(noise_xy(&None, 1.0, 2.0, 3), 0.0);
    }
}
