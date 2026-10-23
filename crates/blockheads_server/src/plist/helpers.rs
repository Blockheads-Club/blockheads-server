//! The C helper functions of the binary: `propertyListFromData*`, `dataFromPropertyList`.
//!

use super::value::PlistValue;
use super::xml_read::property_list_with_data;
use super::xml_write::write_xml;
use crate::foundation::exception::catch_ns;
use crate::nslog;

/// `propertyListFromData(NSData*)` == `...WithOptions(d, 0)`.
pub fn property_list_from_data(d: &[u8]) -> Option<PlistValue> {
    property_list_from_data_with_options(d, 0)
}

/// Same for a possibly-nil `NSData` (nil gives nil, no log).
pub fn property_list_from_data_opt(d: Option<&[u8]>) -> Option<PlistValue> {
    property_list_from_data_with_options_opt(d, 0)
}

/// `propertyListFromDataWithOptions`: the `@try` site; exceptions are logged and give nil.
pub fn property_list_from_data_with_options(d: &[u8], opts: u32) -> Option<PlistValue> {
    match catch_ns(|| property_list_with_data(d, opts).value) {
        Ok(v) => v,
        Err(e) => {
            nslog!(
                "Failed to create plist due to exception in propertyListFromDataWithOptions. Exception:'{}' (reason: '{}')",
                e.name,
                e.reason_or_null()
            );
            None
        }
    }
}

/// Nil `NSData` gives nil without a log.
pub fn property_list_from_data_with_options_opt(d: Option<&[u8]>, opts: u32) -> Option<PlistValue> {
    property_list_from_data_with_options(d?, opts)
}

/// Variant without `@try` (blockhead inventory loads): exceptions propagate.
pub fn property_list_with_data_raw(d: &[u8], opts: u32) -> Option<PlistValue> {
    property_list_with_data(d, opts).value
}

/// `dataFromPropertyList`: XML (format 100); exceptions are logged and give nil. The word "load" is the original's.
pub fn data_from_property_list(v: &PlistValue) -> Option<Vec<u8>> {
    data_from_property_list_hooked(v, || {})
}

/// Implementation with a test hook run inside the `@try`.
pub(crate) fn data_from_property_list_hooked(v: &PlistValue, hook: impl FnOnce()) -> Option<Vec<u8>> {
    match catch_ns(|| {
        hook();
        write_xml(v)
    }) {
        Ok(b) => Some(b),
        Err(e) => {
            nslog!(
                "Failed to load plist due to exception in dataFromPropertyList. Exception:'{}' (reason: '{}')",
                e.name,
                e.reason_or_null()
            );
            None
        }
    }
}

/// Nil value gives nil.
pub fn data_from_property_list_opt(v: Option<&PlistValue>) -> Option<Vec<u8>> {
    data_from_property_list(v?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::foundation::nslog::set_test_sink;
    use std::sync::{Arc, Mutex};

    use crate::foundation::nslog::TEST_SINK_LOCK as L;

    fn capture(f: impl FnOnce()) -> String {
        let _g = L.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let sink = Arc::new(Mutex::new(Vec::new()));
        set_test_sink(Some(sink.clone()));
        f();
        set_test_sink(None);
        let v = sink.lock().unwrap().clone();
        // The sink is process-global: drop lines that parallel reader tests emit (unknown tags/entities, GSIArray).
        String::from_utf8_lossy(&v)
            .split_inclusive('\n')
            .filter(|l| !(l.contains("unrecognized tag <") || l.contains("NSXMLParser: ") || l.contains("GSIArray.h")))
            .collect()
    }

    #[test]
    fn nil_and_empty_are_silent() {
        let log = capture(|| {
            assert!(property_list_from_data_opt(None).is_none());
            assert!(property_list_from_data(b"").is_none());
            assert!(data_from_property_list_opt(None).is_none());
        });
        assert_eq!(log, "");
    }

    #[test]
    fn exception_logged() {
        let bad = b"<?xml version=\"1.0\"?><plist><array><date>bad</date></array></plist>";
        let log = capture(|| assert!(property_list_from_data(bad).is_none()));
        assert!(log.ends_with(
            "Failed to create plist due to exception in propertyListFromDataWithOptions. Exception:'NSInvalidArgumentException' (reason: 'Tried to add nil to array')\n"
        ), "{log}");
        let log = capture(|| {
            let r = data_from_property_list_hooked(&PlistValue::Null, || {
                crate::foundation::exception::ns_raise("NSInvalidArgumentException", "boom")
            });
            assert!(r.is_none());
        });
        assert!(log.ends_with("Failed to load plist due to exception in dataFromPropertyList. Exception:'NSInvalidArgumentException' (reason: 'boom')\n"), "{log}");
    }

    #[test]
    fn writer_never_logs_and_roundtrips() {
        let log = capture(|| {
            let v = PlistValue::array(vec![
                PlistValue::int(5),
                PlistValue::real_f64(1.5),
                PlistValue::Null,
                PlistValue::string("x"),
            ]);
            let b = data_from_property_list(&v).unwrap();
            let back = property_list_from_data(&b).unwrap();
            let PlistValue::Array(a) = back else { panic!() };
            assert_eq!(a.items[0].long_long_value(), 5);
            assert_eq!(a.items[3].as_str(), Some("x"));
        });
        assert_eq!(log, "");
    }

    #[test]
    fn raw_variant_lets_exceptions_through() {
        let ok = b"<?xml version=\"1.0\"?><plist><integer>3</integer></plist>";
        assert_eq!(property_list_with_data_raw(ok, 0).unwrap().long_long_value(), 3);
        let bad = b"<?xml version=\"1.0\"?><plist><array><date>bad</date></array></plist>";
        assert!(catch_ns(|| property_list_with_data_raw(bad, 0)).is_err());
    }
}
