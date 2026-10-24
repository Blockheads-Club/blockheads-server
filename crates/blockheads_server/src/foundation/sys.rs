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

/// A process that dies by a signal never writes its coverage counters; coverage builds write them first.
#[cfg(coverage)]
#[allow(unexpected_cfgs)]
fn flush_coverage() {
    extern "C" {
        fn __llvm_profile_write_file() -> i32;
    }
    // SAFETY: the profiling runtime is linked into every coverage build; the call takes no arguments.
    unsafe {
        __llvm_profile_write_file();
    }
}
#[cfg(not(coverage))]
#[allow(unexpected_cfgs)]
fn flush_coverage() {}

/// Die like a NULL dereference in the original (SIGSEGV on Unix, abort elsewhere). Only used by the
/// `faithful-crashes` paths.
pub fn faithful_segv() -> ! {
    flush_coverage();
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
    flush_coverage();
    std::process::abort()
}

/// Host page size in bytes.
pub fn page_size() -> u64 {
    #[cfg(unix)]
    {
        // SAFETY: sysconf has no preconditions.
        let p = unsafe { libc::sysconf(libc::_SC_PAGESIZE) };
        u64::try_from(p).ok().filter(|p| *p > 0).unwrap_or(4096)
    }
    #[cfg(not(unix))]
    {
        4096
    }
}

/// Process memory as `(total, resident, shared)` in bytes on hosts without `/proc/self/statm`: the
/// equivalents of statm's size / resident / share fields. macOS: `task_info(MACH_TASK_BASIC_INFO)` virtual and
/// resident size; Windows: `GetProcessMemoryInfo` commit charge and working set. Neither reports a shared
/// (file-backed) resident figure, so that is 0. `None` where no query exists or it fails.
pub fn process_memory() -> Option<(u64, u64, u64)> {
    #[cfg(target_vendor = "apple")]
    {
        #[allow(deprecated)]
        // SAFETY: task_info writes at most `count` naturals into the correctly sized, zeroed struct.
        unsafe {
            let mut info: libc::mach_task_basic_info = std::mem::zeroed();
            let mut count = libc::MACH_TASK_BASIC_INFO_COUNT;
            let kr = libc::task_info(
                libc::mach_task_self(),
                libc::MACH_TASK_BASIC_INFO,
                &mut info as *mut libc::mach_task_basic_info as libc::task_info_t,
                &mut count,
            );
            if kr != libc::KERN_SUCCESS {
                return None;
            }
            Some((info.virtual_size, info.resident_size, 0))
        }
    }
    #[cfg(windows)]
    {
        #[repr(C)]
        struct ProcessMemoryCounters {
            cb: u32,
            page_fault_count: u32,
            peak_working_set_size: usize,
            working_set_size: usize,
            quota_peak_paged_pool_usage: usize,
            quota_paged_pool_usage: usize,
            quota_peak_non_paged_pool_usage: usize,
            quota_non_paged_pool_usage: usize,
            pagefile_usage: usize,
            peak_pagefile_usage: usize,
        }
        #[link(name = "kernel32")]
        extern "system" {
            fn GetCurrentProcess() -> *mut std::ffi::c_void;
            fn K32GetProcessMemoryInfo(process: *mut std::ffi::c_void, pmc: *mut ProcessMemoryCounters, cb: u32)
                -> i32;
        }
        let cb = std::mem::size_of::<ProcessMemoryCounters>() as u32;
        // SAFETY: the struct matches PROCESS_MEMORY_COUNTERS and `cb` is its size; the pseudo handle needs no close.
        unsafe {
            let mut pmc: ProcessMemoryCounters = std::mem::zeroed();
            pmc.cb = cb;
            if K32GetProcessMemoryInfo(GetCurrentProcess(), &mut pmc, cb) == 0 {
                return None;
            }
            Some((pmc.pagefile_usage as u64, pmc.working_set_size as u64, 0))
        }
    }
    #[cfg(not(any(target_vendor = "apple", windows)))]
    {
        None
    }
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
        let n = if rc != 0 { 0 } else { buf.iter().position(|&b| b == 0).unwrap_or(buf.len()) };
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
        let found = rc == 0 && !res.is_null() && !pwd.pw_dir.is_null();
        found.then(|| std::ffi::CStr::from_ptr(pwd.pw_dir).to_string_lossy().into_owned())
    }
    #[cfg(not(unix))]
    {
        None
    }
}

/// Wall clock as (unix seconds, microseconds).
pub fn wall_time() -> (i64, u32) {
    wall_parts(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH))
}

/// Seconds and microseconds of a time `Ok(after the epoch)` or `Err(before the epoch)`.
fn wall_parts(since_epoch: Result<std::time::Duration, std::time::SystemTimeError>) -> (i64, u32) {
    match since_epoch {
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

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// Re-runs this test binary on the `child` test below with `BH_TEST_CHILD=kind`: the functions that end the
    /// process cannot run in the test process itself.
    pub(crate) fn run_child(kind: &str) -> std::process::Output {
        std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "foundation::sys::tests::child", "--nocapture", "--test-threads=1"])
            .env("BH_TEST_CHILD", kind)
            .output()
            .unwrap()
    }

    /// Does nothing in the test run; the process-ending checks below start it with `BH_TEST_CHILD` set.
    #[test]
    fn child() {
        use crate::foundation::exception::*;
        let ex = NsException { name: "NSRangeException".into(), reason: Some("r".into()) };
        match std::env::var("BH_TEST_CHILD").as_deref() {
            Ok("segv") => faithful_segv(),
            Ok("abort") => abort(),
            Ok("block") => gnustep_block_abort(),
            Ok("exit") => exit(7),
            Ok("uncaught") => uncaught_exception_exit(&ex, "./bh"),
            Ok("trycatch") => try_catch_handler_perform("doIdle", || ns_raise("NSRangeException", "r")),
            Ok("hook") => {
                install_quiet_panic_hook();
                assert!(catch_ns(|| ns_raise("X", "y")).is_err());
                assert!(std::panic::catch_unwind(|| std::panic::panic_any(NsException {
                    name: "Z".into(),
                    reason: None
                }))
                .is_err());
                exit(if std::panic::catch_unwind(|| panic!("other")).is_err() { 3 } else { 4 })
            }
            _ => {}
        }
    }

    #[cfg(unix)]
    #[test]
    fn process_ending_functions() {
        use std::os::unix::process::ExitStatusExt;
        assert_eq!(run_child("segv").status.signal(), Some(libc::SIGSEGV));
        assert_eq!(run_child("abort").status.signal(), Some(libc::SIGABRT));
        let block = run_child("block");
        assert_eq!(block.status.signal(), Some(libc::SIGABRT));
        assert!(String::from_utf8_lossy(&block.stderr).contains("unknown type ?16"));
        assert_eq!(run_child("exit").status.code(), Some(7));
        let un = run_child("uncaught");
        assert_eq!(un.status.code(), Some(1));
        assert!(String::from_utf8_lossy(&un.stderr).contains("./bh: Uncaught exception NSRangeException, reason: r"));
        let tc = run_child("trycatch");
        assert_eq!(tc.status.code(), Some(0));
        assert!(String::from_utf8_lossy(&tc.stderr).contains("Exiting due to exception in 'doIdle'"));
        let hook = run_child("hook");
        assert_eq!(hook.status.code(), Some(3));
        assert!(String::from_utf8_lossy(&hook.stderr).contains("other"));
        assert!(!String::from_utf8_lossy(&hook.stderr).contains("NSRangeException"));
    }

    #[test]
    fn host_queries() {
        assert!(page_size().is_power_of_two());
        assert_eq!(process_memory(), None);
        assert!(!host_name().is_empty());
        assert!(passwd_home_dir().is_some());
        assert!(monotonic() > 0.0);
        let (y, m, d, ..) = local_time(0);
        assert!((1969..=1970).contains(&y) && (1..=12).contains(&m) && (1..=31).contains(&d));
        assert_eq!(os_str_bytes(std::ffi::OsStr::new("a b")), b"a b");
        write_stderr(b"");
        assert!(wall_time().0 > 1_600_000_000);
    }

    #[test]
    fn hosted_exit_unwinds_instead_of_exiting() {
        assert!(!is_hosted());
        set_hosted(true);
        let r = std::panic::catch_unwind(|| exit(3));
        set_hosted(false);
        assert_eq!(r.unwrap_err().downcast_ref::<HostExit>().map(|h| h.0), Some(3));
        assert!(!is_hosted());
    }

    #[test]
    fn error_text_and_pre_epoch_time() {
        assert_eq!(strerror(&std::io::Error::from_raw_os_error(2)), "No such file or directory");
        assert_eq!(strerror(&std::io::Error::other("plain")), "plain");
        let d = std::time::Duration::new(5, 250_000_000);
        assert_eq!(wall_parts(Ok(d)), (5, 250_000));
        let before = |d: std::time::Duration| std::time::UNIX_EPOCH.duration_since(std::time::UNIX_EPOCH + d);
        assert_eq!(wall_parts(before(d)), (-6, 750_000));
        assert_eq!(wall_parts(before(std::time::Duration::from_secs(3))), (-3, 0));
    }
}
