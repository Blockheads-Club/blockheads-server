//! NSException emulation: `ns_raise` / `catch_ns`, GNUstep reason strings, TryCatchHandler, uncaught-exception exit.
//!

use crate::foundation::nslog::nslog;
use std::panic::{catch_unwind, resume_unwind, AssertUnwindSafe};

/// A raised `NSException` (the panic payload).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NsException {
    pub name: String,
    pub reason: Option<String>,
}

impl NsException {
    /// `[ex reason]` printed with `%@`: `(null)` when nil.
    pub fn reason_or_null(&self) -> &str {
        self.reason.as_deref().unwrap_or("(null)")
    }
}

/// `@throw` with name and reason; unwinds without invoking the panic hook.
pub fn ns_raise(name: &str, reason: &str) -> ! {
    resume_unwind(Box::new(NsException { name: name.to_string(), reason: Some(reason.to_string()) }))
}

/// `@throw` of an exception whose reason is nil.
pub fn ns_raise_nil_reason(name: &str) -> ! {
    resume_unwind(Box::new(NsException { name: name.to_string(), reason: None }))
}

/// `@try { f } @catch (NSException*)`. Any other panic payload is resumed.
pub fn catch_ns<T>(f: impl FnOnce() -> T) -> Result<T, NsException> {
    catch_unwind(AssertUnwindSafe(f)).map_err(ns_payload)
}

/// Non-generic payload handling for `catch_ns`: NSException is returned, anything else resumes unwinding.
fn ns_payload(p: Box<dyn std::any::Any + Send>) -> NsException {
    match p.downcast::<NsException>() {
        Ok(ex) => *ex,
        Err(other) => resume_unwind(other),
    }
}

/// Reason for NSRangeException from array index checks.
pub fn range_exception_index_reason(index: u64, count: u64, selector: &str) -> String {
    format!("Index {} is out of range {} (in '{}')", index, count, selector)
}

/// Reason for NSRangeException "extends beyond size".
pub fn range_exception_beyond_size_reason(selector: &str, loc: u64, len: u64, size: u64) -> String {
    format!("in {}, range {{ {}, {} }} extends beyond size ({})", selector, loc, len, size)
}

/// Raise `NSRangeException` "Index {i} is out of range {n} (in '{sel}')".
pub fn range_exception_index(index: u64, count: u64, selector: &str) -> ! {
    ns_raise("NSRangeException", &range_exception_index_reason(index, count, selector))
}

/// Raise `NSRangeException` "in {sel}, range { {loc}, {len} } extends beyond size ({size})".
pub fn range_exception_beyond_size(selector: &str, loc: u64, len: u64, size: u64) -> ! {
    ns_raise("NSRangeException", &range_exception_beyond_size_reason(selector, loc, len, size))
}

/// Raise `NSInvalidArgumentException`.
pub fn invalid_argument(reason: &str) -> ! {
    ns_raise("NSInvalidArgumentException", reason)
}

/// Reason text for an unrecognised instance selector; `length`/`count` use the "Can not determine type
/// information" form.
pub fn unrecognized_selector_reason(class_name: &str, selector: &str) -> String {
    if selector == "length" || selector == "count" {
        format!("Can not determine type information for -[{} (null)]", class_name)
    } else {
        format!("{}(instance) does not recognize {}", class_name, selector)
    }
}

/// Raise the unrecognised-selector exception.
pub fn unrecognized_selector(class_name: &str, selector: &str) -> ! {
    invalid_argument(&unrecognized_selector_reason(class_name, selector))
}

/// Raise "[{Class} -{selector}] nil string argument".
pub fn nil_argument(class_name: &str, selector: &str) -> ! {
    invalid_argument(&format!("[{} -{}] nil string argument", class_name, selector))
}

/// Best-effort GNUstep class name of a string.
pub fn class_name_for_string(s: &str, mutable: bool, _from_plist: bool) -> &'static str {
    if mutable {
        "GSMutableString"
    } else if s.encode_utf16().all(|u| u <= 0xFF) {
        "GSCInlineString"
    } else {
        "GSUnicodeBufferString"
    }
}

/// The NSLog message of `+[TryCatchHandler performSelector:target:]`.
pub fn try_catch_message(selector_name: &str, ex: &NsException) -> String {
    format!(
        "Exiting due to exception in '{}'. Exception:'{}' (reason: '{}')",
        selector_name,
        ex.name,
        ex.reason_or_null()
    )
}

/// `+[TryCatchHandler performSelector:target:]`: run `f`; on NSException log and `exit(0)`.
pub fn try_catch_handler_perform(selector_name: &str, f: impl FnOnce()) {
    if let Err(ex) = catch_ns(f) {
        nslog(&try_catch_message(selector_name, &ex));
        crate::foundation::sys::exit(0);
    }
}

/// Text printed by the runtime for an exception with no catch site.
pub fn uncaught_exception_text(ex: &NsException, argv0: &str) -> String {
    format!("{}: Uncaught exception {}, reason: {}\n", argv0, ex.name, ex.reason_or_null())
}

use crate::foundation::sys::write_stderr;

/// No catch site: print the uncaught text (no NSLog prefix) and exit with status 1.
pub fn uncaught_exception_exit(ex: &NsException, argv0: &str) -> ! {
    write_stderr(&crate::foundation::nslog::encode_log_bytes(&uncaught_exception_text(ex, argv0)));
    crate::foundation::sys::exit(1)
}

/// Faithful crash of the original (`unknown type ?16` then abort, exit 134).
///.
pub fn gnustep_block_abort() -> ! {
    write_stderr(b"unknown type ?16\n");
    crate::foundation::sys::abort()
}

/// Install a panic hook that stays silent for `NsException` payloads and defers to the default hook otherwise.
pub fn install_quiet_panic_hook() {
    let default = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        if info.payload().downcast_ref::<NsException>().is_some() {
            return;
        }
        default(info);
    }));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raise_and_catch() {
        let r = catch_ns(|| -> i32 { ns_raise("NSRangeException", "boom") });
        let ex = r.unwrap_err();
        assert_eq!(ex.name, "NSRangeException");
        assert_eq!(ex.reason.as_deref(), Some("boom"));
        assert_eq!(catch_ns(|| 5).unwrap(), 5);
    }

    #[test]
    fn foreign_panic_passes_through() {
        let r = catch_unwind(AssertUnwindSafe(|| {
            let _ = catch_ns(|| -> () { panic!("rust bug") });
        }));
        assert!(r.is_err());
    }

    #[test]
    fn try_catch_text() {
        let ex = NsException {
            name: "NSInvalidArgumentException".into(),
            reason: Some("GSArray(instance) does not recognize keyEnumerator".into()),
        };
        assert_eq!(
            try_catch_message("doIdleSafe", &ex),
            "Exiting due to exception in 'doIdleSafe'. Exception:'NSInvalidArgumentException' (reason: 'GSArray(instance) does not recognize keyEnumerator')"
        );
        let ex2 = NsException { name: "X".into(), reason: None };
        assert!(try_catch_message("s", &ex2).ends_with("(reason: '(null)')"));
        assert_eq!(uncaught_exception_text(&ex2, "./bh"), "./bh: Uncaught exception X, reason: (null)\n");
    }

    #[test]
    fn reason_texts() {
        assert_eq!(
            range_exception_index_reason(5, 2, "objectAtIndex:"),
            "Index 5 is out of range 2 (in 'objectAtIndex:')"
        );
        assert_eq!(
            range_exception_beyond_size_reason("subdataWithRange:", 5, 0, 3),
            "in subdataWithRange:, range { 5, 0 } extends beyond size (3)"
        );
        assert_eq!(
            unrecognized_selector_reason("GSArray", "keyEnumerator"),
            "GSArray(instance) does not recognize keyEnumerator"
        );
        assert_eq!(
            unrecognized_selector_reason("NSDoubleNumber", "length"),
            "Can not determine type information for -[NSDoubleNumber (null)]"
        );
        assert_eq!(
            unrecognized_selector_reason("GSArray", "count"),
            "Can not determine type information for -[GSArray (null)]"
        );
        let e = catch_ns(|| nil_argument("GSCBufferString", "rangeOfString:options:range:")).unwrap_err();
        assert_eq!(e.reason.unwrap(), "[GSCBufferString -rangeOfString:options:range:] nil string argument");
    }

    /// Child half of the subprocess test: only acts when the env var is set.
    #[test]
    fn child_uncaught() {
        if std::env::var("BH_EXC_CHILD").as_deref() == Ok("uncaught") {
            let ex = NsException { name: "NSRangeException".into(), reason: Some("r".into()) };
            uncaught_exception_exit(&ex, "./prog");
        }
        if std::env::var("BH_EXC_CHILD").as_deref() == Ok("trycatch") {
            try_catch_handler_perform("doIdleSafe", || ns_raise("NSInvalidArgumentException", "why"));
            unreachable!();
        }
    }

    fn run_child(mode: &str) -> std::process::Output {
        std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "foundation::exception::tests::child_uncaught", "--nocapture", "--test-threads=1"])
            .env("BH_EXC_CHILD", mode)
            .output()
            .unwrap()
    }

    #[test]
    fn uncaught_exit_status() {
        let o = run_child("uncaught");
        assert_eq!(o.status.code(), Some(1));
        assert!(String::from_utf8_lossy(&o.stderr).contains("./prog: Uncaught exception NSRangeException, reason: r\n"));
    }

    #[test]
    fn trycatch_exit_status() {
        let o = run_child("trycatch");
        assert_eq!(o.status.code(), Some(0));
        let e = String::from_utf8_lossy(&o.stderr);
        assert!(
            e.contains(
                "Exiting due to exception in 'doIdleSafe'. Exception:'NSInvalidArgumentException' (reason: 'why')\n"
            ),
            "{e}"
        );
    }

    #[test]
    fn nil_reason_and_class_names() {
        let ex = catch_ns(|| ns_raise_nil_reason("NSGenericException")).unwrap_err();
        assert_eq!((ex.name.as_str(), ex.reason), ("NSGenericException", None));
        try_catch_handler_perform("quiet", || {});
        let name_reason = |f: fn()| {
            let ex = catch_ns(f).unwrap_err();
            (ex.name, ex.reason.unwrap())
        };
        assert_eq!(name_reason(|| range_exception_index(5, 2, "objectAtIndex:")).0, "NSRangeException");
        assert_eq!(name_reason(|| range_exception_beyond_size("subdataWithRange:", 5, 0, 3)).0, "NSRangeException");
        assert!(name_reason(|| unrecognized_selector("GSArray", "foo")).1.contains("does not recognize foo"));
        assert!(name_reason(|| nil_argument("NSString", "length")).1.contains("nil string argument"));
        assert_eq!(class_name_for_string("a", true, false), "GSMutableString");
        assert_eq!(class_name_for_string("a", false, false), "GSCInlineString");
        assert_eq!(class_name_for_string("\u{20ac}", false, true), "GSUnicodeBufferString");
    }
}
