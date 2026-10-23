//! glibc `rand()`/`srand()` (TYPE_3 additive feedback generator), bit-exact.
//!
//! State is per-thread (`thread_local!`); the original only
//! uses it from the main thread.

use std::cell::RefCell;

use super::trace;

/// glibc TYPE_3 generator state: 31-word ring, front/rear pointers.
struct Glibc {
    r: [u32; 31],
    fptr: usize,
    rptr: usize,
}

impl Glibc {
    /// `srandom_r` for TYPE_3: signed LCG fill, then 310 discards.
    fn seeded(seed: u32) -> Self {
        let seed = if seed == 0 { 1 } else { seed };
        let mut r = [0u32; 31];
        r[0] = seed;
        for i in 1..31 {
            let w = r[i - 1] as i32; // signed view, C truncating div/rem
            let hi = w / 127773;
            let lo = w % 127773;
            let mut v = 16807i32.wrapping_mul(lo).wrapping_sub(2836i32.wrapping_mul(hi));
            if v < 0 {
                v = v.wrapping_add(2147483647);
            }
            r[i] = v as u32;
        }
        let mut g = Glibc { r, fptr: 3, rptr: 0 };
        for _ in 0..310 {
            g.step();
        }
        g
    }

    /// One generator step: `r[f] += r[r]; out = r[f] >> 1`.
    fn step(&mut self) -> i32 {
        let v = self.r[self.fptr].wrapping_add(self.r[self.rptr]);
        self.r[self.fptr] = v;
        self.fptr = (self.fptr + 1) % 31;
        self.rptr = (self.rptr + 1) % 31;
        (v >> 1) as i32
    }
}

thread_local! {
    /// Default state is identical to `srand(1)`.
    static STATE: RefCell<Glibc> = RefCell::new(Glibc::seeded(1));
}

/// `srand(seed)`; seed 0 behaves as 1.
pub fn srand(seed: u32) {
    srand_core(seed);
    trace::record_srand("srand", seed);
}

fn srand_core(seed: u32) {
    STATE.with(|s| *s.borrow_mut() = Glibc::seeded(seed));
}

/// `srand` that also feeds the rng trace (`S` line) when feature `trace-rng` is on.
pub fn srand_tagged(tag: &'static str, seed: u32) {
    srand_core(seed);
    trace::record_srand(tag, seed);
}

/// `rand()`: value in `0..=2^31-1`.
pub fn rand() -> i32 {
    let v = rand_core();
    trace::record_glibc("rand", v);
    v
}

fn rand_core() -> i32 {
    STATE.with(|s| s.borrow_mut().step())
}

/// `rand()` with trace record (no-op overhead without feature `trace-rng`).
pub fn rand_tagged(tag: &'static str) -> i32 {
    let v = rand_core();
    trace::record_glibc(tag, v);
    v
}

/// `R01` conversion of a raw draw: `(float)r / 2147483648.0f` in f32.
#[inline]
pub fn r01_from(r: i32) -> f32 {
    (r as f32) / 2147483648.0f32
}

/// `R01 = (float)rand() / 2147483648.0f`; may equal exactly 1.0.
pub fn r01() -> f32 {
    r01_from(rand())
}

/// Tagged variant of [`r01`].
pub fn r01_tagged(tag: &'static str) -> f32 {
    r01_from(rand_tagged(tag))
}

/// `(double)rand()` (NOT `(double)(float)rand()`); only where a spec says `(double)r`.
pub fn rand_f64_raw() -> f64 {
    rand() as f64
}

/// Call-site helper: `rand_tagged` with a `module:line` tag.
#[macro_export]
macro_rules! rng_rand {
    () => {
        $crate::rng::glibc_rand::rand_tagged(concat!(module_path!(), ":", line!()))
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    const SEEDS: [u32; 9] = [1, 0, 2, 42, 12345, 0x7fffffff, 0x80000000, 0xdeadbeef, 0xffffffff];

    #[cfg(all(target_os = "linux", target_env = "gnu"))]
    #[test]
    fn matches_libc() {
        for &s in &SEEDS {
            srand(s);
            unsafe { libc::srand(s) };
            for i in 0..10000 {
                let a = rand();
                let b = unsafe { libc::rand() };
                assert_eq!(a, b, "seed {s:#x} draw {i}");
                assert!(a >= 0);
            }
        }
    }

    #[test]
    fn vectors() {
        srand(1);
        assert_eq!([rand(), rand(), rand()], [1804289383, 846930886, 1681692777]);
        srand(0);
        assert_eq!([rand(), rand(), rand()], [1804289383, 846930886, 1681692777]);
        srand(12345);
        assert_eq!([rand(), rand(), rand()], [383100999, 858300821, 357768173]);
    }

    #[test]
    fn default_and_reseed() {
        std::thread::spawn(|| {
            assert_eq!(rand(), 1804289383);
            rand();
            srand(1);
            assert_eq!(rand(), 1804289383);
        })
        .join()
        .unwrap();
    }

    #[test]
    fn r01_values() {
        assert_eq!(r01_from(i32::MAX), 1.0f32);
        assert_eq!(r01_from(0), 0.0);
        assert_eq!(r01_from(1 << 30), 0.5);
    }

    #[test]
    fn tagged_same() {
        srand(7);
        let a = rand_tagged("t");
        srand(7);
        assert_eq!(a, rand());
    }

    #[test]
    fn raw_double_draw() {
        srand(7);
        let a = rand();
        srand(7);
        assert_eq!(rand_f64_raw(), a as f64);
        assert!((0.0..=1.0).contains(&r01()));
        assert!((0.0..=1.0).contains(&r01_tagged("t")));
    }
}
