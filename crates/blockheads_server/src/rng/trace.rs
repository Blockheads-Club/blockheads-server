//! RNG trace log (feature `trace-rng`).
//!
//! Lines: `R <seq> <tag> <int>`, `M <seq> <tag> <%.17g>`, `S <seq> <tag> <seed>`.
//! Every record is flushed (LineWriter) because SIGINT runs no exit flush.

#[cfg(feature = "trace-rng")]
mod imp {
    use std::fs::File;
    use std::io::{LineWriter, Write};
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
    use std::sync::Mutex;

    static OUT: Mutex<Option<LineWriter<File>>> = Mutex::new(None);
    static ENABLED: AtomicBool = AtomicBool::new(false);
    static SEQ: AtomicU64 = AtomicU64::new(0);

    /// Open the trace file named by `BH_RNG_TRACE`, truncating. Unset or empty: tracing stays off, no file.
    pub fn init_from_env() {
        let path = match std::env::var("BH_RNG_TRACE") {
            Ok(p) if !p.is_empty() => p,
            _ => return,
        };
        if let Ok(f) = File::create(path) {
            *OUT.lock().unwrap() = Some(LineWriter::new(f));
            ENABLED.store(true, Ordering::Release);
        }
    }

    fn emit(kind: char, tag: &str, val: &str) {
        if !ENABLED.load(Ordering::Acquire) {
            return;
        }
        let mut g = OUT.lock().unwrap();
        if let Some(w) = g.as_mut() {
            let seq = SEQ.fetch_add(1, Ordering::Relaxed);
            let _ = writeln!(w, "{kind} {seq} {tag} {val}");
            let _ = w.flush();
        }
    }

    /// `%.17g` approximation: shortest round-trip digits are not required to match, 17 significant digits.
    fn fmt_g17(v: f64) -> String {
        let s = format!("{:.16e}", v);
        // convert to %g-like: use plain decimal when exponent in [-4,17)
        let (m, e) = s.split_once('e').unwrap();
        let e: i32 = e.parse().unwrap();
        if (-4..17).contains(&e) {
            let prec = (16 - e).max(0) as usize;
            let t = format!("{:.*}", prec, v);
            if t.contains('.') {
                t.trim_end_matches('0').trim_end_matches('.').to_string()
            } else {
                t
            }
        } else {
            let m = m.trim_end_matches('0').trim_end_matches('.');
            format!("{}e{}{:02}", m, if e < 0 { '-' } else { '+' }, e.abs())
        }
    }

    /// Record a glibc `rand()` result.
    pub fn record_glibc(tag: &'static str, value: i32) {
        emit('R', tag, &value.to_string());
    }
    /// Record an MT `fn_rand()` result.
    pub fn record_mt(tag: &'static str, value: f64) {
        emit('M', tag, &fmt_g17(value));
    }
    /// Record a `srand` call.
    pub fn record_srand(tag: &'static str, seed: u32) {
        emit('S', tag, &seed.to_string());
    }

    #[cfg(test)]
    mod tests {
        /// srand_tagged + rand record exactly two lines (S once, then R).
        #[test]
        fn records_once_per_primitive() {
            let p = std::env::temp_dir().join(format!("rngtrace_{}.log", std::process::id()));
            std::env::remove_var("BH_RNG_TRACE");
            super::init_from_env();
            std::env::set_var("BH_RNG_TRACE", &p);
            super::init_from_env();
            crate::rng::glibc_rand::srand_tagged("x", 5);
            let v = crate::rng::glibc_rand::rand();
            crate::rng::glibc_rand::srand(0);
            let t = std::fs::read_to_string(&p).unwrap();
            let _ = std::fs::remove_file(&p);
            // Other tests may draw concurrently into the global trace; check our own lines only.
            let sx = t.lines().filter(|l| l.ends_with(" x 5") && l.starts_with("S ")).count();
            assert_eq!(sx, 1, "tagged srand records exactly once");
            assert!(t.lines().any(|l| l.starts_with("R ") && l.ends_with(&format!(" rand {v}"))));
            assert!(t.lines().any(|l| l.starts_with("S ") && l.ends_with(" srand 0")));
        }

        #[test]
        fn g17_integer_valued_large() {
            assert_eq!(super::fmt_g17(1e16), "10000000000000000");
            super::record_mt("m", 0.5);
            assert_eq!(super::fmt_g17(-2.5e-5), "-2.5000000000000001e-05");
        }

        /// `fmt_g17` must equal C `printf("%.17g")`.
        #[cfg(target_os = "linux")]
        #[test]
        fn g17_matches_libc() {
            let vals = [
                0.0,
                1.0,
                0.8147236920927473,
                0.417021998534217,
                2.3283064370807974e-10,
                1e-5,
                1.5e-4,
                123456789012345680.0,
                1e17,
                0.5,
                0.09763202888370306,
                1e300,
            ];
            for v in vals {
                let mut buf = [0u8; 64];
                let n = unsafe { libc::snprintf(buf.as_mut_ptr() as *mut libc::c_char, 64, c"%.17g".as_ptr(), v) };
                let c = std::str::from_utf8(&buf[..n as usize]).unwrap();
                assert_eq!(super::fmt_g17(v), c, "{v}");
            }
        }
    }
}

#[cfg(feature = "trace-rng")]
pub use imp::*;

/// No-op without feature `trace-rng`.
#[cfg(not(feature = "trace-rng"))]
#[inline(always)]
pub fn init_from_env() {}
/// No-op without feature `trace-rng`.
#[cfg(not(feature = "trace-rng"))]
#[inline(always)]
pub fn record_glibc(_tag: &'static str, _value: i32) {}
/// No-op without feature `trace-rng`.
#[cfg(not(feature = "trace-rng"))]
#[inline(always)]
pub fn record_mt(_tag: &'static str, _value: f64) {}
/// No-op without feature `trace-rng`.
#[cfg(not(feature = "trace-rng"))]
#[inline(always)]
pub fn record_srand(_tag: &'static str, _seed: u32) {}
