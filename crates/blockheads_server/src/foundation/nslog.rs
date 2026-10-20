//! NSLog emulation: GNUstep timestamp prefix, one `write(2)` to fd 2.
//!

use crate::foundation::clock;
use std::sync::{Arc, Mutex, OnceLock};

pub use crate::nslog;

static PROCESS_NAME: OnceLock<Mutex<Option<String>>> = OnceLock::new();
/// Serialises tests that install the process-global test sink (shared by foundation and plist tests).
#[cfg(test)]
pub(crate) static TEST_SINK_LOCK: Mutex<()> = Mutex::new(());
static TEST_SINK: Mutex<Option<Arc<Mutex<Vec<u8>>>>> = Mutex::new(None);

/// `[[NSProcessInfo processInfo] processName]` = `lastPathComponent` of argv[0].
fn basename(argv0: &str) -> String {
    crate::foundation::paths::last_path_component(argv0)
}

/// Store the process name (basename of argv[0]). Call once from main.
pub fn set_process_name(argv0: &str) {
    let m = PROCESS_NAME.get_or_init(|| Mutex::new(None));
    *m.lock().unwrap_or_else(|e| e.into_inner()) = Some(basename(argv0));
}

/// Process name used in the prefix; default is the basename of `std::env::args().next()`.
pub fn process_name() -> String {
    let m = PROCESS_NAME.get_or_init(|| Mutex::new(None));
    let mut g = m.lock().unwrap_or_else(|e| e.into_inner());
    if g.is_none() {
        *g = Some(basename(&std::env::args().next().unwrap_or_default()));
    }
    g.clone().unwrap_or_default()
}

/// Latin-1 bytes when every UTF-16 unit is <= 0xFF, otherwise UTF-8.
pub fn encode_log_bytes(s: &str) -> Vec<u8> {
    if s.encode_utf16().all(|u| u <= 0xFF) {
        s.encode_utf16().map(|u| u as u8).collect()
    } else {
        s.as_bytes().to_vec()
    }
}

/// Test seam: when `Some`, finished lines are appended here instead of written to fd 2.
pub fn set_test_sink(sink: Option<Arc<Mutex<Vec<u8>>>>) {
    *TEST_SINK.lock().unwrap_or_else(|e| e.into_inner()) = sink;
}

fn prefix() -> String {
    let t = clock::local_timestamp();
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02}.{:03} {}[{}] ",
        t.year,
        t.month,
        t.day,
        t.hour,
        t.min,
        t.sec,
        t.millis,
        process_name(),
        std::process::id()
    )
}

/// Per-thread log sink used by embedded hosts: receives each finished line (without the trailing newline)
/// instead of stderr.
pub type LogSink = Arc<dyn Fn(&str) + Send + Sync>;

thread_local! {
    static THREAD_SINK: std::cell::RefCell<Option<LogSink>> = const { std::cell::RefCell::new(None) };
}

/// Install (or clear) the log sink of the current thread.
pub fn set_thread_sink(sink: Option<LogSink>) {
    THREAD_SINK.with(|s| *s.borrow_mut() = sink);
}

fn emit(msg: &str, newline: bool) {
    let sink = THREAD_SINK.with(|s| s.borrow().clone());
    if let Some(sink) = sink {
        sink(format!("{}{}", prefix(), msg).trim_end_matches('\n'));
        return;
    }
    let mut line = prefix().into_bytes();
    line.extend(encode_log_bytes(msg));
    if newline {
        line.push(b'\n');
    }
    {
        let g = TEST_SINK.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(s) = g.as_ref() {
            s.lock().unwrap_or_else(|e| e.into_inner()).extend_from_slice(&line);
            return;
        }
    }
    crate::foundation::sys::write_stderr(&line);
}

/// `NSLog(@"...")` where the C format does NOT end in `\n`: always appends `\n`.
pub fn nslog(msg: &str) {
    emit(msg, true);
}

/// NSLog where the original C format literal itself ends in `\n`: no extra newline.
pub fn nslog_fmt_nl(msg: &str) {
    emit(msg, false);
}

/// `nslog!("{}", x)`: Rust-format wrapper around [`nslog`].
#[macro_export]
macro_rules! nslog {
    ($($arg:tt)*) => { $crate::foundation::nslog::nslog(&format!($($arg)*)) };
}

#[cfg(test)]
mod tests {
    use super::*;

    use super::TEST_SINK_LOCK as L;

    fn capture(f: impl FnOnce()) -> Vec<u8> {
        let _g = L.lock().unwrap_or_else(|e| e.into_inner());
        let sink = Arc::new(Mutex::new(Vec::new()));
        set_test_sink(Some(sink.clone()));
        f();
        set_test_sink(None);
        let v = sink.lock().unwrap().clone();
        v
    }

    fn strip_prefix(b: &[u8]) -> &[u8] {
        // "YYYY-MM-DD HH:MM:SS.mmm name[pid] " -> after the first "] "
        let i = b.windows(2).position(|w| w == b"] ").unwrap();
        &b[i + 2..]
    }

    #[test]
    fn prefix_shape() {
        set_process_name("/usr/bin/bh_prog");
        let out = capture(|| nslog("hi"));
        let s = String::from_utf8(out).unwrap();
        let b = s.as_bytes();
        assert!(b[4] == b'-' && b[7] == b'-' && b[10] == b' ' && b[13] == b':' && b[16] == b':' && b[19] == b'.');
        assert!(b[..4].iter().all(u8::is_ascii_digit) && b[20..23].iter().all(u8::is_ascii_digit));
        assert_eq!(&s[23..], format!(" bh_prog[{}] hi\n", std::process::id()));
    }

    #[test]
    fn newline_rules() {
        assert_eq!(strip_prefix(&capture(|| nslog("x\n"))), b"x\n\n");
        assert_eq!(strip_prefix(&capture(|| nslog_fmt_nl("x\n"))), b"x\n");
    }

    #[test]
    fn empty_and_multiline() {
        assert_eq!(strip_prefix(&capture(|| nslog(""))), b"\n");
        let out = capture(|| nslog("a\nb\nc"));
        assert_eq!(strip_prefix(&out), b"a\nb\nc\n");
        assert_eq!(out.iter().filter(|&&c| c == b'[').count(), 1);
        // prefix ends with a single space before the message even for an empty message
        let out = capture(|| nslog(""));
        assert!(out.ends_with(b"] \n"));
    }

    #[test]
    fn encoding() {
        assert_eq!(encode_log_bytes("caf\u{e9}"), vec![0x63, 0x61, 0x66, 0xE9]);
        assert_eq!(encode_log_bytes("\u{65e5}\u{672c}"), "\u{65e5}\u{672c}".as_bytes());
        assert_eq!(encode_log_bytes("e\u{301}"), "e\u{301}".as_bytes());
        assert_eq!(encode_log_bytes("\u{ff}\u{100}"), "\u{ff}\u{100}".as_bytes());
        assert_eq!(strip_prefix(&capture(|| nslog("caf\u{e9}"))), &[0x63, 0x61, 0x66, 0xE9, b'\n']);
    }

    #[test]
    fn threads_do_not_interleave() {
        let _g = L.lock().unwrap_or_else(|e| e.into_inner());
        let sink = Arc::new(Mutex::new(Vec::new()));
        set_test_sink(Some(sink.clone()));
        let hs: Vec<_> = (0..4)
            .map(|t| {
                std::thread::spawn(move || (0..50).for_each(|i| nslog(&format!("thread{t}-{i}-{}", "z".repeat(40)))))
            })
            .collect();
        hs.into_iter().for_each(|h| h.join().unwrap());
        set_test_sink(None);
        let out = String::from_utf8(sink.lock().unwrap().clone()).unwrap();
        // Sink is process-global: ignore lines emitted by unrelated parallel tests.
        let lines: Vec<&str> = out.lines().filter(|l| l.contains("] thread")).collect();
        assert_eq!(lines.len(), 200);
        assert!(lines.iter().all(|l| l.ends_with(&"z".repeat(40)) && l.contains("] thread")));
    }

    #[test]
    fn macro_form() {
        let out = capture(|| crate::nslog!("{}: {}", "a", 3));
        assert_eq!(strip_prefix(&out), b"a: 3\n");
    }
}
