//! `NSNumber` semantics: class selection, the small-integer cache, hash, equality, description.
//!

use crate::foundation::cfmt;
use crate::foundation::gsdict::GsKey;

/// Lowest cached integer.
pub const CACHE_MIN: i64 = -1;
/// Highest cached integer.
pub const CACHE_MAX: i64 = 12;

/// True when integer constructors return a shared object for `v`.
pub fn is_cached_int(v: i64) -> bool {
    (CACHE_MIN..=CACHE_MAX).contains(&v)
}

/// Would two separate `numberWithX:` constructions be pointer-equal?
pub fn pointer_equal_after_separate_construction(a: i64, b: i64) -> bool {
    a == b && is_cached_int(a)
}

/// Class name selected for an integer value.
pub fn int_class_name(v: i128) -> &'static str {
    if (i32::MIN as i128..=i32::MAX as i128).contains(&v) {
        "NSIntNumber"
    } else if (i64::MIN as i128..=i64::MAX as i128).contains(&v) {
        "NSLongLongNumber"
    } else {
        "NSUnsignedLongLongNumber"
    }
}

/// A boxed number.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum NsNumber {
    Bool(bool),
    Int(i64),
    UInt(u64),
    Float(f32),
    Double(f64),
}

/// `cvttsd2si` (32-bit): out of range and NaN give `i32::MIN`.
fn f64_to_i32_trunc(v: f64) -> i32 {
    if v.is_nan() || v >= 2147483648.0 || v <= -2147483649.0 {
        i32::MIN
    } else {
        v as i32
    }
}

impl NsNumber {
    /// `-hash`: integers = low 32 bits unsigned. Double/float: low 32 bits of the x86 `cvttsd2si` truncation;
    /// out of range / NaN / inf -> 0 (observed).
    pub fn gs_hash(&self) -> u64 {
        fn dh(d: f64) -> u64 {
            if !(d > -9.223372036854775809e18 && d < 9.223372036854775808e18) {
                0
            } else {
                ((d as i64) as u32) as u64
            }
        }
        match *self {
            NsNumber::Bool(b) => b as u64,
            NsNumber::Int(v) => (v as u32) as u64,
            NsNumber::UInt(v) => (v as u32) as u64,
            NsNumber::Float(f) => dh(f as f64),
            NsNumber::Double(d) => dh(d),
        }
    }

    fn as_wide_int(&self) -> Option<i128> {
        match *self {
            NsNumber::Bool(b) => Some(b as i128),
            NsNumber::Int(v) => Some(v as i128),
            NsNumber::UInt(v) => Some(v as i128),
            _ => None,
        }
    }

    /// `isEqual:`: numeric across integer types.
    pub fn is_equal(&self, o: &NsNumber) -> bool {
        match (self.as_wide_int(), o.as_wide_int()) {
            (Some(a), Some(b)) => a == b,
            _ => self.double_value() == o.double_value(),
        }
    }

    /// GNUstep class name.
    pub fn class_name(&self) -> &'static str {
        match *self {
            NsNumber::Bool(_) => "NSBoolNumber",
            NsNumber::Int(v) => int_class_name(v as i128),
            NsNumber::UInt(v) => int_class_name(v as i128),
            NsNumber::Float(_) => "NSFloatNumber",
            NsNumber::Double(_) => "NSDoubleNumber",
        }
    }

    /// `intValue` (C cast semantics).
    pub fn int_value(&self) -> i32 {
        match *self {
            NsNumber::Bool(b) => b as i32,
            NsNumber::Int(v) => v as i32,
            NsNumber::UInt(v) => v as i32,
            NsNumber::Float(f) => f64_to_i32_trunc(f as f64),
            NsNumber::Double(d) => f64_to_i32_trunc(d),
        }
    }

    /// `doubleValue`.
    pub fn double_value(&self) -> f64 {
        match *self {
            NsNumber::Bool(b) => b as i32 as f64,
            NsNumber::Int(v) => v as f64,
            NsNumber::UInt(v) => v as f64,
            NsNumber::Float(f) => f as f64,
            NsNumber::Double(d) => d,
        }
    }

    /// `description`: decimal integers, `1`/`0` for bool, shortest round-trip `%.15g..%.17g` for doubles.
    /// Float description is UNVERIFIED (shortest of 6..9 digits that round-trips as f32).
    pub fn description(&self) -> String {
        match *self {
            NsNumber::Bool(b) => (b as i32).to_string(),
            NsNumber::Int(v) => v.to_string(),
            NsNumber::UInt(v) => v.to_string(),
            NsNumber::Double(d) => cfmt::ns_description_double(d),
            NsNumber::Float(f) => cfmt::ns_description_float(f),
        }
    }
}

/// NSNumber used as a dictionary key (GsKey).
#[derive(Clone, Copy, Debug)]
pub struct NsNumberKey(pub NsNumber);

impl GsKey for NsNumberKey {
    fn gs_hash(&self) -> u64 {
        self.0.gs_hash()
    }
    fn gs_eq(&self, other: &Self) -> bool {
        self.0.is_equal(&other.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::foundation::gsdict::GsDict;

    #[test]
    fn cache_range() {
        assert!(pointer_equal_after_separate_construction(-1, -1));
        assert!(pointer_equal_after_separate_construction(12, 12));
        assert!(!pointer_equal_after_separate_construction(13, 13));
        assert!(!pointer_equal_after_separate_construction(-2, -2));
        assert!(!pointer_equal_after_separate_construction(5, 6));
    }

    #[test]
    fn class_names() {
        assert_eq!(NsNumber::Int(5).class_name(), "NSIntNumber");
        assert_eq!(NsNumber::UInt(3_000_000_000).class_name(), "NSLongLongNumber");
        assert_eq!(NsNumber::Int(5_000_000_000).class_name(), "NSLongLongNumber");
        assert_eq!(NsNumber::UInt(u64::MAX).class_name(), "NSUnsignedLongLongNumber");
        assert_eq!(NsNumber::UInt(5).class_name(), "NSIntNumber");
        assert_eq!(NsNumber::Bool(true).class_name(), "NSBoolNumber");
        assert_eq!(NsNumber::Double(1.0).class_name(), "NSDoubleNumber");
        assert_eq!(NsNumber::Float(1.0).class_name(), "NSFloatNumber");
    }

    #[test]
    fn hashes() {
        assert_eq!(NsNumber::Int(-1).gs_hash(), 4294967295);
        assert_eq!(NsNumber::Int(-5).gs_hash(), 4294967291);
        assert_eq!(NsNumber::Int(100000).gs_hash(), 100000);
        assert_eq!(NsNumber::Int(5_000_000_000).gs_hash(), 705032704);
        assert_eq!(NsNumber::UInt(4294967295).gs_hash(), 4294967295);
        assert_eq!((NsNumber::Bool(false).gs_hash(), NsNumber::Bool(true).gs_hash()), (0, 1));
        assert!(NsNumber::Int(5).is_equal(&NsNumber::UInt(5)));
        assert!(!NsNumber::Int(5).is_equal(&NsNumber::Int(6)));
    }

    #[test]
    fn values_and_description() {
        assert_eq!(NsNumber::Double(1e10).int_value(), i32::MIN);
        assert_eq!(NsNumber::Double(f64::NAN).int_value(), i32::MIN);
        assert_eq!(NsNumber::Double(-3.9).int_value(), -3);
        assert_eq!(NsNumber::Int(-7).description(), "-7");
        assert_eq!(NsNumber::Bool(true).description(), "1");
        assert_eq!(NsNumber::Double(0.7000000000000001).description(), "0.7000000000000001");
        assert_eq!(NsNumber::Float(0.1).description(), "0.1");
        assert_eq!(NsNumber::Double(0.1 + 0.2).description(), "0.3");
        assert_eq!(NsNumber::Float(1.23456789).description(), "1.234568");
        assert_eq!(NsNumber::Float(16777216.0).description(), "1.677722e+07");
        assert_eq!(NsNumber::Float(1e8).description(), "1e+08");
    }

    /// observed double/float hash.
    #[test]
    fn double_hash() {
        let h = |d: f64| NsNumber::Double(d).gs_hash();
        assert_eq!((h(0.5), h(-0.0), h(1.0), h(2.5)), (0, 0, 1, 2));
        assert_eq!((h(-1.0), h(-1.5)), (4294967295, 4294967295));
        assert_eq!((h(1e10), h(3e9), h(4294967296.0)), (1410065408, 3000000000, 0));
        assert_eq!(h(123456789012.0), 3197704724);
        assert_eq!((h(1e20), h(f64::INFINITY), h(f64::NAN), h(-9.223372036854775808e18)), (0, 0, 0, 0));
        assert_eq!(NsNumber::Float(-3e9).gs_hash(), 1294967296);
    }

    #[test]
    fn number_keys() {
        let mut d = GsDict::new();
        for i in 0..41u64 {
            d.insert(NsNumberKey(NsNumber::UInt(i)), i);
        }
        assert_eq!(d.bucket_count(), 35);
        let order: Vec<u64> = d.keys_in_order().iter().map(|k| k.0.gs_hash()).collect();
        // hash = value, bucket = value % 35, newest first inside a bucket.
        let mut want = vec![];
        for b in 0..35u64 {
            if b + 35 <= 40 {
                want.push(b + 35);
            }
            want.push(b);
        }
        assert_eq!(order, want);
        assert_eq!(d.get(&NsNumberKey(NsNumber::Int(7))), Some(&7));
    }

    #[test]
    fn value_conversions_per_kind() {
        assert!(NsNumber::Bool(true).is_equal(&NsNumber::UInt(1)));
        assert!(NsNumber::Int(2).is_equal(&NsNumber::Double(2.0)));
        assert!(!NsNumber::Float(0.5).is_equal(&NsNumber::Double(0.25)));
        assert!(!NsNumber::Int(2).is_equal(&NsNumber::UInt(3)));
        assert_eq!(NsNumber::Bool(true).int_value(), 1);
        assert_eq!(NsNumber::Int(-5).int_value(), -5);
        assert_eq!(NsNumber::UInt(7).int_value(), 7);
        assert_eq!(NsNumber::Float(2.75).int_value(), 2);
        assert_eq!(NsNumber::Double(-2.75).int_value(), -2);
        assert_eq!(NsNumber::Bool(true).double_value(), 1.0);
        assert_eq!(NsNumber::Int(-5).double_value(), -5.0);
        assert_eq!(NsNumber::UInt(7).double_value(), 7.0);
        assert_eq!(NsNumber::Float(0.5).double_value(), 0.5);
        assert_eq!(NsNumber::Double(0.25).double_value(), 0.25);
        assert_eq!(NsNumber::UInt(u64::MAX).description(), "18446744073709551615");
        assert_eq!(NsNumber::Bool(false).description(), "0");
        assert_eq!(NsNumber::Int(-3).description(), "-3");
        assert_eq!(NsNumber::Double(1.5).description(), "1.5");
        assert_eq!(NsNumber::Float(1.5).description(), "1.5");
    }
}
