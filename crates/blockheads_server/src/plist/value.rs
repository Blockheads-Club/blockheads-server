//! Typed property-list tree (`PlistValue`) carrying the GNUstep class identity needed by exception texts.
//!

use crate::foundation::gsdict::GsDict;
use crate::foundation::nsnumber::NsNumber;
use crate::foundation::nsstring;
use std::ops::{Deref, DerefMut};

/// Which parser produced a node (determines GNUstep class names).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Origin {
    #[default]
    Built,
    Xml,
    Binary,
    OpenStep,
    Json,
}

/// Origin plus mutability (`NSPropertyListMutable*` option).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct ClassTag {
    pub origin: Origin,
    pub mutable: bool,
    /// XML-parsed leaves only (observed, plist_golden/xml_read_expected): for a string, its source text contained a
    /// predefined/numeric entity or a TAB, which makes GNUstep store it as `GSUnicodeInlineString` even when every unit
    /// is <= 0xFF (in5_x.plist.out); for data, its source text was not blank, so an empty decode result is
    /// `NSDataMalloc` rather than `NSDataStatic` (data_bad.plist.out vs data_ws.plist.out).
    pub alt: bool,
}

/// `NSString`.
#[derive(Clone, Debug)]
pub struct PlistStr {
    pub s: String,
    pub tag: ClassTag,
}
/// `NSData`.
#[derive(Clone, Debug)]
pub struct PlistData {
    pub bytes: Vec<u8>,
    pub tag: ClassTag,
}
/// `NSArray`.
#[derive(Clone, Debug)]
pub struct PlistArray {
    pub items: Vec<PlistValue>,
    pub tag: ClassTag,
}
/// `NSDictionary` (string keys only).
#[derive(Clone, Debug)]
pub struct PlistDict {
    pub map: GsDict<String, PlistValue>,
    pub tag: ClassTag,
}

impl Deref for PlistStr {
    type Target = str;
    fn deref(&self) -> &str {
        &self.s
    }
}
impl From<&str> for PlistStr {
    fn from(s: &str) -> Self {
        PlistStr { s: s.to_string(), tag: ClassTag::default() }
    }
}
impl From<String> for PlistStr {
    fn from(s: String) -> Self {
        PlistStr { s, tag: ClassTag::default() }
    }
}
impl Deref for PlistData {
    type Target = [u8];
    fn deref(&self) -> &[u8] {
        &self.bytes
    }
}
impl Deref for PlistArray {
    type Target = Vec<PlistValue>;
    fn deref(&self) -> &Vec<PlistValue> {
        &self.items
    }
}
impl DerefMut for PlistArray {
    fn deref_mut(&mut self) -> &mut Vec<PlistValue> {
        &mut self.items
    }
}
impl Deref for PlistDict {
    type Target = GsDict<String, PlistValue>;
    fn deref(&self) -> &GsDict<String, PlistValue> {
        &self.map
    }
}
impl DerefMut for PlistDict {
    fn deref_mut(&mut self) -> &mut GsDict<String, PlistValue> {
        &mut self.map
    }
}

/// A property-list object.
#[derive(Clone, Debug)]
pub enum PlistValue {
    String(PlistStr),
    /// Any integer NSNumber whose value fits i64.
    Int(i64),
    /// Only values > i64::MAX (`NSUnsignedLongLongNumber`).
    UInt(u64),
    /// `NSFloatNumber` (`is_float`) or `NSDoubleNumber`.
    Real {
        value: f64,
        is_float: bool,
    },
    /// `NSBoolNumber`, written `<true/>`/`<false/>`.
    Bool(bool),
    /// Seconds since 2001-01-01T00:00:00Z.
    Date(f64),
    Data(PlistData),
    Array(PlistArray),
    Dict(PlistDict),
    /// `NSNull`.
    Null,
}

/// Class name of an integer NSNumber.
pub fn ns_number_class_name_i64(v: i64) -> &'static str {
    if (-2147483647..=2147483646).contains(&v) {
        "NSIntNumber"
    } else {
        "NSLongLongNumber"
    }
}

/// `cvttsd2si` 32-bit: NaN/out of range give `i32::MIN`.
fn f64_to_i32(v: f64) -> i32 {
    if v.is_nan() || v >= 2147483648.0 || v <= -2147483649.0 {
        i32::MIN
    } else {
        v as i32
    }
}
/// `cvttsd2si` 64-bit.
fn f64_to_i64(v: f64) -> i64 {
    if v.is_nan() || v >= 9223372036854775808.0 || v < -9223372036854775808.0 {
        i64::MIN
    } else {
        v as i64
    }
}
/// C `(unsigned long long)double` as compiled for x86-64.
fn f64_to_u64(v: f64) -> u64 {
    if v < 9223372036854775808.0 {
        f64_to_i64(v) as u64
    } else {
        (f64_to_i64(v - 9223372036854775808.0) as u64) ^ (1u64 << 63)
    }
}

impl PlistValue {
    /// `[NSString stringWithString:]`-like constructor (tag Built).
    pub fn string(s: impl Into<String>) -> Self {
        PlistValue::String(PlistStr { s: s.into(), tag: ClassTag::default() })
    }
    /// `numberWithLongLong:`-family.
    pub fn int(v: i64) -> Self {
        PlistValue::Int(v)
    }
    /// `numberWithFloat:`.
    pub fn real_f32(v: f32) -> Self {
        PlistValue::Real { value: v as f64, is_float: true }
    }
    /// `numberWithDouble:`.
    pub fn real_f64(v: f64) -> Self {
        PlistValue::Real { value: v, is_float: false }
    }
    /// `numberWithBool:`.
    pub fn boolean(b: bool) -> Self {
        PlistValue::Bool(b)
    }
    /// `NSDate` from reference-date seconds.
    pub fn date(ref_secs: f64) -> Self {
        PlistValue::Date(ref_secs)
    }
    /// `NSData`.
    pub fn data(b: impl Into<Vec<u8>>) -> Self {
        PlistValue::Data(PlistData { bytes: b.into(), tag: ClassTag::default() })
    }
    /// Immutable `NSArray`.
    pub fn array(v: Vec<PlistValue>) -> Self {
        PlistValue::Array(PlistArray { items: v, tag: ClassTag::default() })
    }
    /// Immutable `NSDictionary`.
    pub fn dict(m: GsDict<String, PlistValue>) -> Self {
        PlistValue::Dict(PlistDict { map: m, tag: ClassTag::default() })
    }
    /// `NSMutableArray`.
    pub fn mutable_array() -> Self {
        PlistValue::Array(PlistArray {
            items: Vec::new(),
            tag: ClassTag { origin: Origin::Built, mutable: true, alt: false },
        })
    }
    /// `NSMutableDictionary` (empty, 1 bucket).
    pub fn mutable_dict() -> Self {
        PlistValue::Dict(PlistDict {
            map: GsDict::new(),
            tag: ClassTag { origin: Origin::Built, mutable: true, alt: false },
        })
    }

    /// GNUstep class name.
    pub fn class_name(&self) -> &'static str {
        match self {
            PlistValue::Dict(d) => {
                if d.tag.mutable {
                    "GSMutableDictionary"
                } else {
                    "GSDictionary"
                }
            }
            PlistValue::Array(a) => {
                if a.tag.mutable {
                    "GSMutableArray"
                } else if a.tag.origin == Origin::Binary {
                    "GSInlineArray"
                } else {
                    "GSArray"
                }
            }
            PlistValue::String(s) => {
                if s.tag.mutable || s.tag.origin == Origin::OpenStep && s.s.encode_utf16().all(|u| u <= 0xFF) {
                    return "GSMutableString";
                }
                let latin1 = s.s.encode_utf16().all(|u| u <= 0xFF);
                match s.tag.origin {
                    Origin::Xml => {
                        if latin1 && !s.tag.alt {
                            "GSCInlineString"
                        } else {
                            "GSUnicodeInlineString"
                        }
                    }
                    Origin::Binary => {
                        if s.s.is_ascii() {
                            "GSCBufferString"
                        } else if latin1 {
                            "GSCInlineString"
                        } else {
                            "GSUnicodeBufferString"
                        }
                    }
                    Origin::Json => {
                        if latin1 {
                            "GSCBufferString"
                        } else {
                            "GSUnicodeBufferString"
                        }
                    }
                    Origin::OpenStep => "GSUnicodeBufferString",
                    Origin::Built => {
                        if latin1 {
                            "GSCInlineString"
                        } else {
                            "GSUnicodeInlineString"
                        }
                    }
                }
            }
            PlistValue::Data(d) => {
                if d.tag.origin == Origin::OpenStep || d.tag.mutable {
                    "NSMutableDataMalloc"
                } else if d.tag.origin == Origin::Xml && d.bytes.is_empty() && !d.tag.alt {
                    "NSDataStatic"
                } else {
                    "NSDataMalloc"
                }
            }
            PlistValue::Date(_) => "NSGDate",
            PlistValue::Int(v) => ns_number_class_name_i64(*v),
            PlistValue::UInt(_) => "NSUnsignedLongLongNumber",
            PlistValue::Real { is_float, .. } => {
                if *is_float {
                    "NSFloatNumber"
                } else {
                    "NSDoubleNumber"
                }
            }
            PlistValue::Bool(_) => "NSBoolNumber",
            PlistValue::Null => "NSNull",
        }
    }

    fn as_number(&self) -> Option<NsNumber> {
        match self {
            PlistValue::Int(v) => Some(NsNumber::Int(*v)),
            PlistValue::UInt(v) => Some(NsNumber::UInt(*v)),
            PlistValue::Real { value, is_float } => {
                Some(if *is_float { NsNumber::Float(*value as f32) } else { NsNumber::Double(*value) })
            }
            PlistValue::Bool(b) => Some(NsNumber::Bool(*b)),
            _ => None,
        }
    }

    /// `-isEqual:`.
    pub fn is_equal(&self, other: &PlistValue) -> bool {
        if let (Some(a), Some(b)) = (self.as_number(), other.as_number()) {
            return a.is_equal(&b);
        }
        match (self, other) {
            (PlistValue::String(a), PlistValue::String(b)) => a.s == b.s,
            (PlistValue::Data(a), PlistValue::Data(b)) => a.bytes == b.bytes,
            (PlistValue::Date(a), PlistValue::Date(b)) => a == b,
            (PlistValue::Null, PlistValue::Null) => true,
            (PlistValue::Array(a), PlistValue::Array(b)) => {
                a.items.len() == b.items.len() && a.items.iter().zip(b.items.iter()).all(|(x, y)| x.is_equal(y))
            }
            (PlistValue::Dict(a), PlistValue::Dict(b)) => {
                a.map.len() == b.map.len() && a.map.iter().all(|(k, v)| b.map.get(k).is_some_and(|w| v.is_equal(w)))
            }
            _ => false,
        }
    }

    /// `-description` (numbers/strings exact; containers best effort).
    pub fn description(&self) -> String {
        if let PlistValue::Real { value, is_float } = self {
            // GNUstep 1.24 -[NSNumber description]: "%0.16g" for doubles (observed on 8 192 NSDoubleNumber lines of
            // plist_golden/xml_read_expected, e.g. 0.127783514 whose shortest round-trip form needs 17 digits) and
            // "%0.7g" for floats (`NSFloatNumber 0.1`, `1.5`).
            return super::xml_write::format_real(*value, *is_float);
        }
        match self {
            PlistValue::String(s) => s.s.clone(),
            PlistValue::Null => "<null>".to_string(),
            PlistValue::Date(d) => format!("{}", d),
            PlistValue::Data(d) => {
                let mut s = String::from("<");
                for (i, b) in d.bytes.iter().enumerate() {
                    if i > 0 && i % 4 == 0 {
                        s.push(' ');
                    }
                    s.push_str(&format!("{:02x}", b));
                }
                s.push('>');
                s
            }
            PlistValue::Array(a) => {
                let parts: Vec<String> = a.items.iter().map(|v| v.description()).collect();
                format!("({})", parts.join(", "))
            }
            PlistValue::Dict(d) => {
                let mut s = String::from("{");
                for (k, v) in d.map.iter() {
                    s.push_str(&format!("{} = {}; ", k, v.description()));
                }
                s.push('}');
                s
            }
            _ => self.as_number().map(|n| n.description()).unwrap_or_default(),
        }
    }

    /// `Some` only for `String`.
    pub fn as_str(&self) -> Option<&str> {
        match self {
            PlistValue::String(s) => Some(&s.s),
            _ => None,
        }
    }

    /// `objectForKey:` on a dictionary; `None` if missing or not a dictionary.
    pub fn object_for_key(&self, k: &str) -> Option<&PlistValue> {
        match self {
            PlistValue::Dict(d) => d.map.get(&k.to_string()),
            _ => None,
        }
    }

    /// `-intValue`.
    pub fn int_value(&self) -> i32 {
        match self {
            PlistValue::String(s) => nsstring::int_value(&s.s),
            PlistValue::Real { value, is_float } => {
                let v = if *is_float { *value as f32 as f64 } else { *value };
                f64_to_i32(v)
            }
            PlistValue::Int(v) => *v as i32,
            PlistValue::UInt(v) => *v as i32,
            PlistValue::Bool(b) => *b as i32,
            _ => 0,
        }
    }
    /// `-unsignedIntValue`.
    pub fn unsigned_int_value(&self) -> u32 {
        match self {
            PlistValue::String(s) => nsstring::int_value(&s.s) as u32,
            PlistValue::Real { value, .. } => f64_to_i64(*value) as u32,
            _ => self.int_value() as u32,
        }
    }
    /// `-longLongValue`.
    pub fn long_long_value(&self) -> i64 {
        match self {
            PlistValue::String(s) => nsstring::long_long_value(&s.s),
            PlistValue::Real { value, is_float } => {
                let v = if *is_float { *value as f32 as f64 } else { *value };
                f64_to_i64(v)
            }
            PlistValue::Int(v) => *v,
            PlistValue::UInt(v) => *v as i64,
            PlistValue::Bool(b) => *b as i64,
            _ => 0,
        }
    }
    /// `-unsignedLongValue`.
    pub fn unsigned_long_value(&self) -> u64 {
        match self {
            PlistValue::Real { value, .. } => f64_to_u64(*value),
            PlistValue::UInt(v) => *v,
            _ => self.long_long_value() as u64,
        }
    }
    /// `-floatValue`.
    pub fn float_value(&self) -> f32 {
        match self {
            PlistValue::String(s) => nsstring::float_value(&s.s),
            PlistValue::Real { value, .. } => *value as f32,
            PlistValue::Int(v) => *v as f32,
            PlistValue::UInt(v) => *v as f32,
            PlistValue::Bool(b) => *b as i32 as f32,
            _ => 0.0,
        }
    }
    /// `-doubleValue`.
    pub fn double_value(&self) -> f64 {
        match self {
            PlistValue::String(s) => nsstring::double_value(&s.s),
            PlistValue::Real { value, .. } => *value,
            PlistValue::Int(v) => *v as f64,
            PlistValue::UInt(v) => *v as f64,
            PlistValue::Bool(b) => *b as i32 as f64,
            _ => 0.0,
        }
    }
    /// `-boolValue`.
    pub fn bool_value(&self) -> bool {
        match self {
            PlistValue::String(s) => nsstring::bool_value(&s.s),
            PlistValue::Real { value, .. } => *value != 0.0,
            PlistValue::Int(v) => *v != 0,
            PlistValue::UInt(v) => *v != 0,
            PlistValue::Bool(b) => *b,
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn int_class_rule() {
        assert_eq!(ns_number_class_name_i64(2147483646), "NSIntNumber");
        assert_eq!(ns_number_class_name_i64(2147483647), "NSLongLongNumber");
        assert_eq!(ns_number_class_name_i64(-2147483647), "NSIntNumber");
        assert_eq!(ns_number_class_name_i64(-2147483648), "NSLongLongNumber");
    }

    #[test]
    fn equality_and_accessors() {
        assert!(PlistValue::int(1).is_equal(&PlistValue::boolean(true)));
        assert!(PlistValue::int(1).is_equal(&PlistValue::real_f64(1.0)));
        assert!(!PlistValue::real_f32(0.1).is_equal(&PlistValue::real_f64(0.1)));
        assert_eq!(PlistValue::real_f64(f64::NAN).int_value(), i32::MIN);
        assert_eq!(PlistValue::string("12abc").int_value(), 12);
        assert_eq!(PlistValue::mutable_array().class_name(), "GSMutableArray");
        assert_eq!(PlistValue::array(vec![]).class_name(), "GSArray");
    }

    fn tag(origin: Origin, mutable: bool, alt: bool) -> ClassTag {
        ClassTag { origin, mutable, alt }
    }
    fn st(s: &str, origin: Origin, mutable: bool, alt: bool) -> PlistValue {
        PlistValue::String(PlistStr { s: s.to_string(), tag: tag(origin, mutable, alt) })
    }
    fn da(b: &[u8], origin: Origin, mutable: bool, alt: bool) -> PlistValue {
        PlistValue::Data(PlistData { bytes: b.to_vec(), tag: tag(origin, mutable, alt) })
    }

    #[test]
    fn wrapper_conversions() {
        let s: PlistStr = "ab".into();
        assert_eq!(&*s, "ab");
        let s: PlistStr = String::from("cd").into();
        assert_eq!(s.len(), 2);
        let d = PlistData { bytes: vec![1, 2], tag: ClassTag::default() };
        assert_eq!(&*d, &[1, 2]);
        let mut a = PlistArray { items: vec![PlistValue::Null], tag: ClassTag::default() };
        assert_eq!(a.len(), 1);
        a.push(PlistValue::Null);
        assert_eq!(a.items.len(), 2);
        let mut pd = PlistDict { map: GsDict::new(), tag: ClassTag::default() };
        pd.insert("k".to_string(), PlistValue::Null);
        assert_eq!(pd.len(), 1);
        assert_eq!(PlistValue::mutable_dict().class_name(), "GSMutableDictionary");
        assert_eq!(PlistValue::dict(GsDict::new()).class_name(), "GSDictionary");
    }

    #[test]
    fn float_conversions_follow_cvttsd2si() {
        assert_eq!(f64_to_i32(3e10), i32::MIN);
        assert_eq!(f64_to_i32(-2.5), -2);
        assert_eq!(f64_to_i64(f64::NAN), i64::MIN);
        assert_eq!(f64_to_i64(1e30), i64::MIN);
        assert_eq!(f64_to_i64(-7.9), -7);
        assert_eq!(f64_to_u64(5.0), 5);
        assert_eq!(f64_to_u64(1.8446744073709552e19 / 2.0 + 4096.0), (1u64 << 63) + 4096);
    }

    #[test]
    fn class_names_by_origin() {
        use Origin::*;
        assert_eq!(st("a", Xml, false, false).class_name(), "GSCInlineString");
        assert_eq!(st("a", Xml, false, true).class_name(), "GSUnicodeInlineString");
        assert_eq!(st("\u{20ac}", Xml, false, false).class_name(), "GSUnicodeInlineString");
        assert_eq!(st("a", Binary, false, false).class_name(), "GSCBufferString");
        assert_eq!(st("\u{e9}", Binary, false, false).class_name(), "GSCInlineString");
        assert_eq!(st("\u{20ac}", Binary, false, false).class_name(), "GSUnicodeBufferString");
        assert_eq!(st("a", Json, false, false).class_name(), "GSCBufferString");
        assert_eq!(st("\u{20ac}", Json, false, false).class_name(), "GSUnicodeBufferString");
        assert_eq!(st("a", OpenStep, false, false).class_name(), "GSMutableString");
        assert_eq!(st("\u{20ac}", OpenStep, false, false).class_name(), "GSUnicodeBufferString");
        assert_eq!(st("a", Built, false, false).class_name(), "GSCInlineString");
        assert_eq!(st("\u{20ac}", Built, false, false).class_name(), "GSUnicodeInlineString");
        assert_eq!(st("a", Built, true, false).class_name(), "GSMutableString");
        assert_eq!(da(b"", OpenStep, false, false).class_name(), "NSMutableDataMalloc");
        assert_eq!(da(b"", Xml, false, false).class_name(), "NSDataStatic");
        assert_eq!(da(b"", Xml, false, true).class_name(), "NSDataMalloc");
        assert_eq!(da(b"x", Xml, false, false).class_name(), "NSDataMalloc");
        assert_eq!(PlistValue::date(1.0).class_name(), "NSGDate");
        assert_eq!(PlistValue::UInt(u64::MAX).class_name(), "NSUnsignedLongLongNumber");
        assert_eq!(PlistValue::real_f32(1.0).class_name(), "NSFloatNumber");
        assert_eq!(PlistValue::real_f64(1.0).class_name(), "NSDoubleNumber");
        assert_eq!(PlistValue::Bool(true).class_name(), "NSBoolNumber");
        assert_eq!(PlistValue::Null.class_name(), "NSNull");
    }

    #[test]
    fn equality_across_kinds() {
        let a = PlistValue::array(vec![PlistValue::string("x"), PlistValue::data(vec![1])]);
        let b = PlistValue::array(vec![PlistValue::string("x"), PlistValue::data(vec![1])]);
        assert!(a.is_equal(&b));
        assert!(!a.is_equal(&PlistValue::array(vec![])));
        assert!(PlistValue::date(2.0).is_equal(&PlistValue::date(2.0)));
        assert!(PlistValue::Null.is_equal(&PlistValue::Null));
        assert!(!PlistValue::Null.is_equal(&PlistValue::string("x")));
        assert!(PlistValue::UInt(u64::MAX).is_equal(&PlistValue::UInt(u64::MAX)));
        let mut m1 = GsDict::new();
        m1.insert("k".to_string(), PlistValue::int(1));
        let mut m2 = GsDict::new();
        m2.insert("k".to_string(), PlistValue::int(1));
        let mut m3 = GsDict::new();
        m3.insert("k".to_string(), PlistValue::int(2));
        assert!(PlistValue::dict(m1.clone()).is_equal(&PlistValue::dict(m2)));
        assert!(!PlistValue::dict(m1.clone()).is_equal(&PlistValue::dict(m3)));
        assert!(!PlistValue::dict(m1).is_equal(&PlistValue::dict(GsDict::new())));
    }

    #[test]
    fn descriptions() {
        assert_eq!(PlistValue::real_f64(1.5).description(), "1.5");
        assert_eq!(PlistValue::int(7).description(), "7");
        assert_eq!(PlistValue::string("s").description(), "s");
        assert_eq!(PlistValue::Null.description(), "<null>");
        assert_eq!(PlistValue::date(2.0).description(), "2");
        assert_eq!(PlistValue::data(vec![0, 1, 2, 3, 4]).description(), "<00010203 04>");
        assert_eq!(PlistValue::array(vec![PlistValue::int(1), PlistValue::int(2)]).description(), "(1, 2)");
        let mut m = GsDict::new();
        m.insert("a".to_string(), PlistValue::int(1));
        assert_eq!(PlistValue::dict(m).description(), "{a = 1; }");
    }

    #[test]
    fn value_accessors_by_kind() {
        let d = PlistValue::dict(GsDict::new());
        assert!(d.object_for_key("k").is_none());
        assert!(PlistValue::Null.object_for_key("k").is_none());
        let u = PlistValue::UInt(7);
        assert_eq!((u.int_value(), u.unsigned_int_value(), u.long_long_value()), (7, 7, 7));
        assert_eq!(u.unsigned_long_value(), 7);
        assert_eq!((u.float_value(), u.double_value(), u.bool_value()), (7.0, 7.0, true));
        let r = PlistValue::real_f64(2.75);
        assert_eq!((r.unsigned_int_value(), r.long_long_value(), r.unsigned_long_value()), (2, 2, 2));
        assert_eq!((r.float_value(), r.double_value(), r.bool_value()), (2.75, 2.75, true));
        assert_eq!(PlistValue::real_f32(0.5).long_long_value(), 0);
        let s = PlistValue::string("42.5");
        assert_eq!((s.unsigned_int_value(), s.long_long_value()), (42, 42));
        assert_eq!((s.float_value(), s.double_value()), (42.5, 42.5));
        assert!(s.bool_value() && !PlistValue::string("0").bool_value());
        assert!(PlistValue::string("YES").bool_value());
        let b = PlistValue::Bool(true);
        assert_eq!((b.int_value(), b.long_long_value(), b.float_value(), b.double_value()), (1, 1, 1.0, 1.0));
        let i = PlistValue::int(-3);
        assert_eq!((i.float_value(), i.double_value(), i.bool_value()), (-3.0, -3.0, true));
        assert_eq!(PlistValue::int(5).as_str(), None);
        assert_eq!((PlistValue::int(5).int_value(), PlistValue::UInt(9).int_value(), b.bool_value()), (5, 9, true));
        let n = PlistValue::Null;
        assert_eq!(
            (n.int_value(), n.long_long_value(), n.float_value(), n.double_value(), n.bool_value()),
            (0, 0, 0.0, 0.0, false)
        );
        assert_eq!(n.unsigned_int_value(), 0);
        assert_eq!(n.unsigned_long_value(), 0);
        assert_eq!(PlistValue::Bool(true).unsigned_long_value(), 1);
    }
}
