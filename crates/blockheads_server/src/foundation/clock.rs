//! Clock: `NSDate`, `time(NULL)`, `gettimeofday` and the run-loop monotonic time, with a Virtual mode.
//!
//!  No other module may call
//! `SystemTime::now()` / `Instant::now()`; everything goes through here.

#[cfg(not(test))]
use std::sync::Mutex;

/// 2001-01-01T00:00:00Z as Unix seconds (NSDate reference epoch).
pub const REFERENCE_EPOCH_UNIX: i64 = 978_307_200;

/// Clock state.
#[derive(Clone, Copy, Debug)]
enum State {
    Real,
    /// `start_unix` plus the virtual `elapsed` seconds.
    Virtual {
        start_unix: f64,
        elapsed: f64,
    },
}

#[cfg(not(test))]
static STATE: Mutex<State> = Mutex::new(State::Real);

// Under test each test thread gets its own clock so parallel tests cannot disturb each other.
#[cfg(test)]
thread_local! { static TSTATE: std::cell::Cell<State> = const { std::cell::Cell::new(State::Real) }; }

#[cfg(not(test))]
fn with_state<R>(f: impl FnOnce(&mut State) -> R) -> R {
    f(&mut STATE.lock().unwrap_or_else(std::sync::PoisonError::into_inner))
}
#[cfg(test)]
fn with_state<R>(f: impl FnOnce(&mut State) -> R) -> R {
    TSTATE.with(|c| {
        let mut s = c.get();
        let r = f(&mut s);
        c.set(s);
        r
    })
}

fn state() -> State {
    with_state(|s| *s)
}

/// Broken-down local time used by NSLog.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LocalStamp {
    pub year: i32,
    pub month: u32,
    pub day: u32,
    pub hour: u32,
    pub min: u32,
    pub sec: u32,
    pub millis: u32,
}

/// Current wall time as (unix seconds, microseconds) honouring the Virtual mode.
fn now_parts() -> (i64, u32) {
    match state() {
        State::Real => crate::foundation::sys::wall_time(),
        State::Virtual { start_unix, elapsed } => {
            let t = start_unix + elapsed;
            let sec = t.floor();
            let usec = ((t - sec) * 1_000_000.0) as u32;
            (sec as i64, usec.min(999_999))
        }
    }
}

/// `[[NSDate date] timeIntervalSinceReferenceDate]`: unix (f64) minus 978307200.0.
pub fn reference_time() -> f64 {
    match state() {
        State::Real => {
            let (s, us) = now_parts();
            (s as f64 + us as f64 / 1_000_000.0) - REFERENCE_EPOCH_UNIX as f64
        }
        State::Virtual { start_unix, elapsed } => start_unix - REFERENCE_EPOCH_UNIX as f64 + elapsed,
    }
}

/// `time(NULL)`.
pub fn unix_time() -> i64 {
    match state() {
        State::Real => now_parts().0,
        State::Virtual { start_unix, elapsed } => (start_unix + elapsed).floor() as i64,
    }
}

/// `localtime_r` of "now" (or of the virtual instant).
pub fn local_timestamp() -> LocalStamp {
    let (sec, usec) = now_parts();
    let (year, month, day, hour, min, sec) = crate::foundation::sys::local_time(sec);
    LocalStamp { year, month, day, hour, min, sec, millis: usec / 1000 }
}

/// Monotonic seconds for run-loop deadlines. Virtual: advances only through [`advance_virtual`].
pub fn monotonic_seconds() -> f64 {
    match state() {
        State::Real => crate::foundation::sys::monotonic(),
        State::Virtual { elapsed, .. } => elapsed,
    }
}

/// True when the Virtual clock is active.
pub fn is_virtual() -> bool {
    matches!(state(), State::Virtual { .. })
}

/// Select Virtual mode when `BH_VIRTUAL_CLOCK=<unix start>` is set. Call once at process start.
pub fn init_from_env() {
    if let Ok(v) = std::env::var("BH_VIRTUAL_CLOCK") {
        if let Ok(start) = v.trim().parse::<f64>() {
            set_virtual(Some(start));
        }
    }
}

/// Switch to Virtual (`Some(start_unix)`) or back to Real (`None`). Mostly for tests and `init_from_env`.
pub fn set_virtual(start_unix: Option<f64>) {
    with_state(|g| {
        *g = match start_unix {
            Some(s) => State::Virtual { start_unix: s, elapsed: 0.0 },
            None => State::Real,
        }
    });
}

/// Advance the Virtual clock by `dt` seconds; no-op in Real mode.
pub fn advance_virtual(dt: f64) {
    with_state(|g| {
        if let State::Virtual { elapsed, .. } = g {
            *elapsed += dt;
        }
    });
}

/// Real: sleep the thread; Virtual: [`advance_virtual`].
pub fn sleep_for(dt: f64) {
    if is_virtual() {
        advance_virtual(dt);
    } else if dt > 0.0 {
        std::thread::sleep(std::time::Duration::from_secs_f64(dt));
    }
}

/// Crate-wide lock serialising tests that touch the process-global virtual clock.
#[cfg(test)]
pub(crate) static TEST_CLOCK_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex as M;
    static L: &M<()> = &super::TEST_CLOCK_LOCK;

    #[test]
    fn reference_epoch_arithmetic() {
        let _g = L.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        set_virtual(None);
        let d = reference_time() - (unix_time() - REFERENCE_EPOCH_UNIX) as f64;
        assert!((0.0..1.5).contains(&d), "{d}");
    }

    #[test]
    fn virtual_clock() {
        let _g = L.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        set_virtual(Some(1_700_000_000.0));
        assert!(is_virtual());
        assert_eq!(unix_time(), 1_700_000_000);
        assert_eq!(reference_time(), 1_700_000_000.0 - 978_307_200.0);
        advance_virtual(1.25);
        assert_eq!(unix_time(), 1_700_000_001);
        assert_eq!(monotonic_seconds(), 1.25);
        sleep_for(0.5);
        assert_eq!(monotonic_seconds(), 1.75);
        assert_eq!(local_timestamp().millis, 750);
        set_virtual(None);
        assert!(!is_virtual());
    }

    #[test]
    fn real_clock_env_switch_and_sleep() {
        let _g = L.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        set_virtual(None);
        let a = monotonic_seconds();
        sleep_for(0.001);
        sleep_for(-1.0);
        assert!(monotonic_seconds() > a && !is_virtual());
        std::env::set_var("BH_VIRTUAL_CLOCK", "not a number");
        init_from_env();
        assert!(!is_virtual());
        std::env::set_var("BH_VIRTUAL_CLOCK", " 1000.5 ");
        init_from_env();
        std::env::remove_var("BH_VIRTUAL_CLOCK");
        assert!(is_virtual());
        sleep_for(2.0);
        assert_eq!(monotonic_seconds(), 2.0);
        assert_eq!(unix_time(), 1002);
        set_virtual(None);
    }
}
