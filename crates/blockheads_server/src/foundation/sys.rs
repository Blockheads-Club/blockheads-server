//! Small platform layer: everything that needs OS-specific calls lives here so the rest of the crate is portable.

use std::cell::Cell;
use std::io::Write;

/// Payload used to unwind a hosted server thread when the game code asks the process to exit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HostExit(pub i32);

thread_local! {
    static HOSTED: Cell<bool> = const { Cell::new(false) };
}

/// Mark the current thread as hosted (embedded in another program). While set, [`exit`] unwinds the thread
/// with a [`HostExit`] payload instead of terminating the whole process.
pub fn set_hosted(on: bool) {
    HOSTED.with(|h| h.set(on));
}

/// True when the current thread runs inside a host.
pub fn is_hosted() -> bool {
    HOSTED.with(|h| h.get())
}

/// `exit(code)` of the original. In a hosted thread it unwinds to the host instead.
pub fn exit(code: i32) -> ! {
    if is_hosted() {
        std::panic::resume_unwind(Box::new(HostExit(code)));
    }
    std::process::exit(code)
}

/// Write all bytes to fd 2 / the stderr handle, no buffering, errors ignored.
pub fn write_stderr(bytes: &[u8]) {
    let mut e = std::io::stderr().lock();
    let _ = e.write_all(bytes);
    let _ = e.flush();
}

/// Die like a NULL dereference in the original (SIGSEGV on Unix, abort elsewhere). Only used by the
/// `faithful-crashes` paths.
pub fn faithful_segv() -> ! {
    #[cfg(unix)]
    // SAFETY: restoring the default disposition (Rust's runtime installs a stack-overflow handler that would
    // swallow a raised SIGSEGV), then raising it.
    unsafe {
        libc::signal(libc::SIGSEGV, libc::SIG_DFL);
        libc::raise(libc::SIGSEGV);
    }
    std::process::abort()
}

/// `abort()`.
pub fn abort() -> ! {
    std::process::abort()
}

/// Host page size in bytes.
pub fn page_size() -> u64 {
    #[cfg(unix)]
    {
        // SAFETY: sysconf has no preconditions.
        let p = unsafe { libc::sysconf(libc::_SC_PAGESIZE) };
        if p > 0 {
            return p as u64;
        }
    }
    4096
}

/// `strerror(errno)` text (perror style, without Rust's "(os error N)" suffix).
pub fn strerror(e: &std::io::Error) -> String {
    #[cfg(unix)]
    {
        if let Some(errno) = e.raw_os_error() {
            // SAFETY: strerror returns a pointer to a static NUL-terminated message.
            return unsafe { std::ffi::CStr::from_ptr(libc::strerror(errno)) }.to_string_lossy().into_owned();
        }
    }
    e.to_string()
}

/// Bytes of a command line argument as the OS holds them.
pub fn os_str_bytes(a: &std::ffi::OsStr) -> Vec<u8> {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        a.as_bytes().to_vec()
    }
    #[cfg(not(unix))]
    {
        a.to_string_lossy().into_owned().into_bytes()
    }
}

/// Machine host name.
pub fn host_name() -> String {
    #[cfg(unix)]
    {
        let mut buf = [0u8; 256];
        // SAFETY: buffer length passed.
        let rc = unsafe { libc::gethostname(buf.as_mut_ptr() as *mut libc::c_char, buf.len()) };
        if rc != 0 {
            return String::new();
        }
        let n = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
        String::from_utf8_lossy(&buf[..n]).into_owned()
    }
    #[cfg(not(unix))]
    {
        std::env::var("COMPUTERNAME").unwrap_or_default()
    }
}

/// Passwd home directory of the current user (Unix fallback when `$HOME` is unset).
pub fn passwd_home_dir() -> Option<String> {
    #[cfg(unix)]
    // SAFETY: getpwuid_r with a local buffer.
    unsafe {
        let mut pwd: libc::passwd = std::mem::zeroed();
        let mut buf = vec![0u8; 4096];
        let mut res: *mut libc::passwd = std::ptr::null_mut();
        let rc = libc::getpwuid_r(libc::getuid(), &mut pwd, buf.as_mut_ptr() as *mut libc::c_char, buf.len(), &mut res);
        if rc == 0 && !res.is_null() && !pwd.pw_dir.is_null() {
            return Some(std::ffi::CStr::from_ptr(pwd.pw_dir).to_string_lossy().into_owned());
        }
    }
    None
}

/// Wall clock as (unix seconds, microseconds).
pub fn wall_time() -> (i64, u32) {
    match std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
        Ok(d) => (d.as_secs() as i64, d.subsec_micros()),
        Err(e) => {
            let d = e.duration();
            let (s, us) = (d.as_secs() as i64, d.subsec_micros());
            if us == 0 {
                (-s, 0)
            } else {
                (-s - 1, 1_000_000 - us)
            }
        }
    }
}

/// Monotonic seconds (arbitrary origin).
pub fn monotonic() -> f64 {
    #[cfg(unix)]
    {
        let mut ts = libc::timespec { tv_sec: 0, tv_nsec: 0 };
        // SAFETY: valid pointer.
        unsafe { libc::clock_gettime(libc::CLOCK_MONOTONIC, &mut ts) };
        ts.tv_sec as f64 + ts.tv_nsec as f64 / 1e9
    }
    #[cfg(not(unix))]
    {
        use std::sync::OnceLock;
        static ORIGIN: OnceLock<std::time::Instant> = OnceLock::new();
        ORIGIN.get_or_init(std::time::Instant::now).elapsed().as_secs_f64()
    }
}

/// Broken-down local time: (year, month 1-12, day, hour, min, sec).
pub fn local_time(sec: i64) -> (i32, u32, u32, u32, u32, u32) {
    #[cfg(unix)]
    {
        let t: libc::time_t = sec as libc::time_t;
        // SAFETY: tm is plain data, localtime_r writes into it.
        let mut tm: libc::tm = unsafe { std::mem::zeroed() };
        unsafe { libc::localtime_r(&t, &mut tm) };
        (
            tm.tm_year + 1900,
            (tm.tm_mon + 1) as u32,
            tm.tm_mday as u32,
            tm.tm_hour as u32,
            tm.tm_min as u32,
            tm.tm_sec as u32,
        )
    }
    #[cfg(not(unix))]
    {
        use chrono::{Datelike, TimeZone, Timelike};
        match chrono::Local.timestamp_opt(sec, 0).single() {
            Some(t) => (t.year(), t.month(), t.day(), t.hour(), t.minute(), t.second()),
            None => (1970, 1, 1, 0, 0, 0),
        }
    }
}
