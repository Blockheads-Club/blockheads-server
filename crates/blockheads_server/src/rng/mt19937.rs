//! Mersenne Twister variant used by `NoiseFunction` (bit-exact).
//!
//! Provides an owned [`Mt19937`] and thread-local global wrappers with the original names.

use std::cell::RefCell;

use super::trace;

/// `fn_rand` scale = 1/(2^32-1).
const SCALE_RAND: f64 = 2.3283064370807974e-10;
/// `genrand_real2/3` scale = 2^-32.
const SCALE_REAL: f64 = 2.3283064365386963e-10;
/// `genrand_res53` scale = 2^-53.
const SCALE_53: f64 = 1.1102230246251565e-16;

const N: usize = 624;

/// MT state: `state`, `left`, `initf`, `next`.
#[derive(Clone)]
pub struct Mt19937 {
    state: [u64; N],
    left: i32,
    initf: bool,
    next: usize,
}

impl Default for Mt19937 {
    fn default() -> Self {
        Self::new_unseeded()
    }
}

impl Mt19937 {
    /// Unseeded generator (`initf == 0`); first draw seeds with 0x1571.
    pub fn new_unseeded() -> Self {
        // Note: says the unseeded first draw runs next_state(); a literal C left=0 would not. left=1 forces it.
        Mt19937 { state: [0; N], left: 1, initf: false, next: 0 }
    }

    /// Generator seeded via `fn_seed_rand(s)`.
    pub fn with_seed(s: u64) -> Self {
        let mut m = Self::new_unseeded();
        m.fn_seed_rand(s);
        m
    }

    /// `fn_seed_rand` (init_genrand), A. Seed masked to 32 bits.
    pub fn fn_seed_rand(&mut self, s: u64) {
        self.state[0] = s & 0xffff_ffff;
        for j in 1..N {
            let p = self.state[j - 1];
            self.state[j] = ((p ^ (p >> 30)).wrapping_mul(0x6c07_8965).wrapping_add(j as u64)) & 0xffff_ffff;
        }
        self.left = 1;
        self.initf = true;
    }

    /// `init_by_array` incl. `state[0]=state[623]` copy on wrap.
    pub fn init_by_array(&mut self, key: &[u64]) {
        self.fn_seed_rand(0x12bd6aa);
        let kl = key.len();
        let (mut i, mut j) = (1usize, 0usize);
        let k = N.max(kl);
        for _ in 0..k {
            let p = self.state[i - 1];
            self.state[i] = ((self.state[i] ^ ((p ^ (p >> 30)).wrapping_mul(0x19660d)))
                .wrapping_add(key[j])
                .wrapping_add(j as u64))
                & 0xffff_ffff;
            i += 1;
            j += 1;
            if i >= N {
                self.state[0] = self.state[N - 1];
                i = 1;
            }
            if j >= kl {
                j = 0;
            }
        }
        for _ in 0..N - 1 {
            let p = self.state[i - 1];
            self.state[i] =
                ((self.state[i] ^ ((p ^ (p >> 30)).wrapping_mul(0x5d58_8b65))).wrapping_sub(i as u64)) & 0xffff_ffff;
            i += 1;
            if i >= N {
                self.state[0] = self.state[N - 1];
                i = 1;
            }
        }
        self.state[0] = 0x8000_0000;
        self.left = 1;
        self.initf = true;
    }

    /// `next_state` (generate 624 words), A.
    fn next_state(&mut self) {
        if !self.initf {
            self.fn_seed_rand(0x1571);
        }
        self.left = N as i32;
        self.next = 0;
        let s = &mut self.state;
        let mag = |v: u64| if v & 1 != 0 { 0x9908_b0dfu64 } else { 0 };
        for p in 0..227 {
            s[p] = s[p + 397] ^ (((s[p] & 0x8000_0000) | (s[p + 1] & 0x7fff_ffff)) >> 1) ^ mag(s[p + 1]);
        }
        for p in 227..623 {
            s[p] = s[p - 227] ^ (((s[p] & 0x8000_0000) | (s[p + 1] & 0x7fff_ffff)) >> 1) ^ mag(s[p + 1]);
        }
        s[623] = s[396] ^ (((s[623] & 0x8000_0000) | (s[0] & 0x7fff_ffff)) >> 1) ^ mag(s[0]);
    }

    /// Common draw + tempering.
    fn draw(&mut self) -> u32 {
        self.left -= 1;
        if self.left == 0 {
            self.next_state();
        }
        let mut y = self.state[self.next];
        self.next += 1;
        y ^= y >> 11;
        y ^= (y & 0x13a58ad) << 7;
        y ^= (y & 0x1df8c) << 15;
        y ^= y >> 18;
        y as u32
    }

    /// `genrand_int32`.
    pub fn genrand_int32(&mut self) -> u32 {
        self.draw()
    }
    /// `genrand_int31`: `y >> 1`.
    pub fn genrand_int31(&mut self) -> i64 {
        (self.draw() >> 1) as i64
    }
    /// `genrand_real2`: [0,1).
    pub fn genrand_real2(&mut self) -> f64 {
        self.draw() as f64 * SCALE_REAL
    }
    /// `genrand_real3`: (0,1).
    pub fn genrand_real3(&mut self) -> f64 {
        (self.draw() as f64 + 0.5) * SCALE_REAL
    }
    /// `genrand_res53`: first word >>5, second >>6.
    pub fn genrand_res53(&mut self) -> f64 {
        let a = (self.genrand_int32() >> 5) as f64;
        let b = (self.genrand_int32() >> 6) as f64;
        (a * 67108864.0 + b) * SCALE_53
    }
    /// `fn_rand`: [0,1] inclusive, scale 1/(2^32-1).
    pub fn fn_rand(&mut self) -> f64 {
        self.draw() as f64 * SCALE_RAND
    }
}

thread_local! {
    /// Global MT state (main thread only in the original).
    static GLOBAL: RefCell<Mt19937> = RefCell::new(Mt19937::new_unseeded());
}

/// Global `fn_seed_rand`; callers pass `(long)int_seed` sign-extended.
pub fn fn_seed_rand(s: u64) {
    GLOBAL.with(|g| g.borrow_mut().fn_seed_rand(s));
}
/// Global `init_by_array`.
pub fn init_by_array(key: &[u64]) {
    GLOBAL.with(|g| g.borrow_mut().init_by_array(key));
}
/// Global `genrand_int32`.
pub fn genrand_int32() -> u32 {
    GLOBAL.with(|g| g.borrow_mut().genrand_int32())
}
/// Global `genrand_int31`.
pub fn genrand_int31() -> i64 {
    GLOBAL.with(|g| g.borrow_mut().genrand_int31())
}
/// Global `genrand_real2`.
pub fn genrand_real2() -> f64 {
    GLOBAL.with(|g| g.borrow_mut().genrand_real2())
}
/// Global `genrand_real3`.
pub fn genrand_real3() -> f64 {
    GLOBAL.with(|g| g.borrow_mut().genrand_real3())
}
/// Global `genrand_res53`.
pub fn genrand_res53() -> f64 {
    GLOBAL.with(|g| g.borrow_mut().genrand_res53())
}
/// Global `fn_rand`.
pub fn fn_rand() -> f64 {
    let v = fn_rand_core();
    trace::record_mt("fn_rand", v);
    v
}

fn fn_rand_core() -> f64 {
    GLOBAL.with(|g| g.borrow_mut().fn_rand())
}
/// `fn_rand` with trace record.
pub fn fn_rand_tagged(tag: &'static str) -> f64 {
    let v = fn_rand_core();
    trace::record_mt(tag, v);
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    fn draws(seed: u64) -> Vec<u32> {
        let mut m = Mt19937::with_seed(seed);
        (0..626).map(|_| m.genrand_int32()).collect()
    }

    #[test]
    fn vectors() {
        let rows: [(u64, [u32; 3], [u32; 3], f64); 4] = [
            (5489, [3499211612, 581869302, 3890346734], [4020325887, 4178893912, 610818241], 0.8147236920927473),
            (1, [1791095845, 4282876139, 3093770124], [2006116153, 1104314680, 939235918], 0.417021998534217),
            (12345, [3992670690, 3823185381, 1358822685], [2940097750, 3957348375, 3514278914], 0.9296160868670829),
            (0xffffffff, [419326371, 479346978, 3918654476], [1027084080, 3860652269, 657474326], 0.09763202888370306),
        ];
        for (seed, first, bound, fr) in rows {
            let d = draws(seed);
            assert_eq!(&d[0..3], &first, "seed {seed}");
            assert_eq!(&d[623..626], &bound, "seed {seed}");
            let mut m = Mt19937::with_seed(seed);
            assert_eq!(m.fn_rand(), fr, "seed {seed}");
        }
    }

    #[test]
    fn unseeded_and_signext() {
        let mut m = Mt19937::new_unseeded();
        assert_eq!(m.genrand_int32(), 3499211612);
        let mut a = Mt19937::with_seed((-1i32) as i64 as u64);
        assert_eq!(a.genrand_int32(), 419326371);
    }

    #[test]
    fn init_by_array_vec() {
        let mut m = Mt19937::new_unseeded();
        m.init_by_array(&[5489]);
        assert_eq!([m.genrand_int32(), m.genrand_int32(), m.genrand_int32()], [3382763572, 956215839, 417760592]);
        // key_length 700 > 624 (both wrap copies exercised); vector from CPython
        // `random.Random().seed(sum(i << (32*i) for i in range(700)))` + getrandbits(32).
        let key: Vec<u64> = (0..700).collect();
        m.init_by_array(&key);
        assert_eq!([m.genrand_int32(), m.genrand_int32(), m.genrand_int32()], [3727595200, 1914792892, 3929396303]);
    }

    #[test]
    fn scales() {
        assert_eq!(0xffff_ffffu32 as f64 * SCALE_RAND, 1.0);
        assert!(0xffff_ffffu32 as f64 * SCALE_REAL < 1.0);
    }

    #[test]
    fn res53_consumes_two() {
        let mut a = Mt19937::with_seed(9);
        let mut b = Mt19937::with_seed(9);
        let x = a.genrand_res53();
        let w1 = b.genrand_int32() >> 5;
        let w2 = b.genrand_int32() >> 6;
        assert_eq!(x, (w1 as f64 * 67108864.0 + w2 as f64) * SCALE_53);
        assert_eq!(a.genrand_int32(), b.genrand_int32());
    }

    #[test]
    fn noise_consumption() {
        let seed = 777u64;
        let mut m = Mt19937::with_seed(seed);
        for _ in 0..6143 {
            m.fn_rand();
        }
        let next = m.genrand_int32();
        let d = {
            let mut p = Mt19937::with_seed(seed);
            for _ in 0..6143 {
                p.genrand_int32();
            }
            p.genrand_int32()
        };
        assert_eq!(next, d);
    }

    #[test]
    fn global_wrappers() {
        fn_seed_rand(1);
        assert_eq!(genrand_int32(), 1791095845);
    }
}
