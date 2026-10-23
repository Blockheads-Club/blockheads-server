//! `NSJSONSerialization` subset: GNUstep 1.24's hand-rolled parser including its bugs.
//!

use super::value::{ClassTag, Origin, PlistArray, PlistDict, PlistStr, PlistValue};
use crate::foundation::exception::ns_raise;
use crate::foundation::gsdict::GsDict;

/// Result of `JSONObjectWithData:options:error:`.
#[derive(Debug)]
pub struct JsonResult {
    pub value: Option<PlistValue>,
    /// `[err localizedDescription]`; `None` when `err` stayed nil.
    pub error_description: Option<String>,
}

/// Internal failure kinds.
#[derive(Debug, PartialEq)]
enum JErr {
    /// "JSON Parse error" with the (unused by the server) failure reason.
    Parse(String),
    /// NSString creation returned nil: no error object (6.2 rule 8).
    Nil,
    /// `NSRangeException` with this reason (6.2 rule 9).
    Exc(String),
}

const WS: [u16; 6] = [0x20, 0x09, 0x0a, 0x0d, 0x0b, 0x0c];

/// Unicode noncharacter / invalid-sequence check done when an `NSString` is created.
fn bad_units(u: &[u16]) -> bool {
    let mut i = 0;
    while i < u.len() {
        let c = u[i];
        if (0xD800..0xDC00).contains(&c) {
            if i + 1 < u.len() && (0xDC00..0xE000).contains(&u[i + 1]) {
                let cp = 0x10000 + (((c as u32) - 0xD800) << 10) + ((u[i + 1] as u32) - 0xDC00);
                if cp & 0xFFFE == 0xFFFE {
                    return true;
                }
                i += 2;
                continue;
            }
            return true;
        }
        if (0xDC00..0xE000).contains(&c) || (0xFDD0..=0xFDEF).contains(&c) || c == 0xFFFE || c == 0xFFFF {
            return true;
        }
        i += 1;
    }
    false
}

/// Encoding detection and decoding to UTF-16 units. Failure gives an empty text.
fn decode(data: &[u8]) -> Vec<u16> {
    let n = data.len();
    let mut b = [0xffu8; 4];
    for (k, x) in data.iter().take(4).enumerate() {
        b[k] = *x;
    }
    #[derive(PartialEq)]
    enum Enc {
        U8,
        U16Le,
        U16Be,
        U32Le,
        U32Be,
    }
    let mut strip = false;
    let enc = if data.starts_with(&[0xef, 0xbb, 0xbf]) {
        strip = true;
        Enc::U8
    } else if data.starts_with(&[0xff, 0xfe, 0, 0]) {
        Enc::U32Le
    } else if data.starts_with(&[0, 0, 0xfe, 0xff]) {
        Enc::U32Be
    } else if data.starts_with(&[0xff, 0xfe]) {
        Enc::U16Le
    } else if data.starts_with(&[0xfe, 0xff]) {
        Enc::U16Be
    } else if b[0] == 0 {
        if b[1] == 0 {
            Enc::U32Be
        } else {
            Enc::U16Be
        }
    } else if b[1] == 0 {
        if b[2] == 0 && b[3] == 0 {
            Enc::U32Le
        } else {
            Enc::U16Le
        }
    } else {
        Enc::U8
    };
    let text: Option<String> = match enc {
        Enc::U8 => {
            let body = if strip { &data[3..] } else { data };
            std::str::from_utf8(body).ok().map(|t| {
                let t = if strip { t.strip_prefix('\u{feff}').unwrap_or(t) } else { t };
                t.to_string()
            })
        }
        Enc::U16Le | Enc::U16Be => {
            if n % 2 != 0 {
                None
            } else {
                let u: Vec<u16> = data
                    .chunks(2)
                    .map(|c| {
                        if enc == Enc::U16Le {
                            u16::from_le_bytes([c[0], c[1]])
                        } else {
                            u16::from_be_bytes([c[0], c[1]])
                        }
                    })
                    .collect();
                String::from_utf16(&u).ok()
            }
        }
        Enc::U32Le | Enc::U32Be => {
            if n % 4 != 0 {
                None
            } else {
                data.chunks(4)
                    .map(|c| {
                        let v = if enc == Enc::U32Le {
                            u32::from_le_bytes([c[0], c[1], c[2], c[3]])
                        } else {
                            u32::from_be_bytes([c[0], c[1], c[2], c[3]])
                        };
                        char::from_u32(v)
                    })
                    .collect::<Option<String>>()
            }
        }
    };
    match text {
        Some(t) => t.encode_utf16().collect(),
        None => Vec::new(),
    }
}

struct P {
    src: Vec<u16>,
    i: usize,
    n: usize,
}

fn tag() -> ClassTag {
    ClassTag { origin: Origin::Json, mutable: false, alt: false }
}

impl P {
    fn ch(&self, i: usize) -> u16 {
        if i < self.n {
            self.src[i]
        } else {
            0
        }
    }
    fn cur(&self) -> u16 {
        self.ch(self.i)
    }
    fn err(&self, idx: usize) -> JErr {
        let c = self.ch(idx) & 0xff;
        let shown = if c == 0 { "\\000".to_string() } else { char::from_u32(c as u32).unwrap().to_string() };
        JErr::Parse(format!("Unexpected character {} at index {}", shown, idx))
    }
    fn beyond(&self) -> JErr {
        JErr::Exc(format!("in getCharacters:range:, range {{ {}, 64 }} extends beyond size ({})", self.n + 1, self.n))
    }
    fn skip(&mut self) {
        while self.i < self.n && WS.contains(&self.src[self.i]) {
            self.i += 1;
        }
    }
    fn top(&mut self) -> Result<PlistValue, JErr> {
        self.skip();
        self.value(true)
    }
    fn value(&mut self, top: bool) -> Result<PlistValue, JErr> {
        self.skip();
        let c = self.cur();
        if c == 0x22 {
            return self.string().map(|s| PlistValue::String(PlistStr { s, tag: tag() }));
        }
        if c == 0x5b {
            return self.array();
        }
        if c == 0x7b {
            return self.object();
        }
        if c == 0x2d || (0x30..=0x39).contains(&c) {
            return self.number();
        }
        for lit in ["true", "false", "null"] {
            if c == lit.as_bytes()[0] as u16 {
                for (k, lc) in lit.bytes().enumerate().skip(1) {
                    if self.ch(self.i + k) != lc as u16 {
                        return Err(self.err(self.i + k));
                    }
                }
                if !top {
                    return Err(self.err(self.i + lit.len() - 1));
                }
                self.i += lit.len();
                return Ok(match lit {
                    "true" => PlistValue::Bool(true),
                    "false" => PlistValue::Bool(false),
                    _ => PlistValue::Null,
                });
            }
        }
        Err(self.err(self.i))
    }
    fn digits(&mut self) {
        while (0x30..=0x39).contains(&self.cur()) {
            self.i += 1;
        }
    }
    fn number(&mut self) -> Result<PlistValue, JErr> {
        let st = self.i;
        if self.cur() == 0x2d {
            self.i += 1;
        }
        self.digits();
        if self.cur() == 0x2e {
            self.i += 1;
            self.digits();
        }
        if self.cur() == 0x65 || self.cur() == 0x45 {
            self.i += 1;
            if self.i >= self.n {
                return Err(self.beyond());
            }
            // one arbitrary character is swallowed (observed bug)
            self.i += 1;
            self.digits();
        }
        let t: String =
            self.src[st..self.i.min(self.n)].iter().map(|&c| char::from_u32(c as u32).unwrap_or('\u{fffd}')).collect();
        Ok(PlistValue::Real { value: parse_number_text(&t), is_float: false })
    }
    fn string(&mut self) -> Result<String, JErr> {
        self.i += 1;
        let mut out: Vec<u16> = Vec::new();
        loop {
            if self.i >= self.n {
                return Err(self.beyond());
            }
            let c = self.src[self.i];
            if c == 0 {
                if out.is_empty() {
                    return Err(JErr::Parse("Unexpected character \\000 at index 0".to_string()));
                }
                self.i += 1;
                if bad_units(&out) {
                    return Err(JErr::Nil);
                }
                return Ok(String::from_utf16_lossy(&out));
            }
            if c == 0x22 {
                self.i += 1;
                if bad_units(&out) {
                    return Err(JErr::Nil);
                }
                if out.first() == Some(&0xfeff) {
                    out.remove(0);
                }
                return Ok(String::from_utf16_lossy(&out));
            }
            if c == 0x5c {
                self.i += 1;
                let c = self.cur();
                if c == 0x75 {
                    let mut v: u32 = 0;
                    for _ in 0..4 {
                        self.i += 1;
                        let h = self.cur();
                        let d = match h {
                            0x30..=0x39 => h - 0x30,
                            0x41..=0x46 => h - 0x41 + 10,
                            0x61..=0x66 => h - 0x61 + 10,
                            _ => return Err(self.err(self.i)),
                        };
                        v = v * 16 + d as u32;
                    }
                    out.push(v as u16);
                } else {
                    out.push(match c {
                        0x6e => 10,
                        0x74 => 9,
                        0x72 => 13,
                        0x62 => 8,
                        0x66 => 12,
                        o => o,
                    });
                }
                self.i += 1;
                continue;
            }
            out.push(c);
            self.i += 1;
        }
    }
    fn array(&mut self) -> Result<PlistValue, JErr> {
        self.i += 1;
        let mut items = Vec::new();
        loop {
            self.skip();
            if self.cur() == 0x5d {
                self.i += 1;
                return Ok(PlistValue::Array(PlistArray { items, tag: tag() }));
            }
            items.push(self.value(false)?);
            self.skip();
            if self.cur() == 0x2c {
                self.i += 1;
            }
        }
    }
    fn object(&mut self) -> Result<PlistValue, JErr> {
        self.i += 1;
        let mut map = GsDict::json_parsed();
        loop {
            self.skip();
            if self.cur() == 0x7d {
                self.i += 1;
                return Ok(PlistValue::Dict(PlistDict { map, tag: tag() }));
            }
            if self.cur() != 0x22 {
                return Err(self.err(self.i));
            }
            let k = self.string()?;
            self.skip();
            if self.cur() != 0x3a {
                return Err(self.err(self.i));
            }
            self.i += 1;
            let v = self.value(false)?;
            map.insert(k, v);
            self.skip();
            if self.cur() == 0x2c {
                self.i += 1;
            }
        }
    }
}

/// strtod result of the consumed number text (`float()` first, then the integer-prefix fallback of the model).
fn parse_number_text(t: &str) -> f64 {
    if let Ok(v) = t.parse::<f64>() {
        if !t.is_empty() && t.bytes().all(|b| b.is_ascii_digit() || b"+-.eE".contains(&b)) {
            return v;
        }
    }
    // regex -?\d+(\.\d*)?
    let b = t.as_bytes();
    let mut i = 0;
    if i < b.len() && b[i] == b'-' {
        i += 1;
    }
    let ds = i;
    while i < b.len() && b[i].is_ascii_digit() {
        i += 1;
    }
    if i == ds {
        return 0.0;
    }
    if i < b.len() && b[i] == b'.' {
        i += 1;
        while i < b.len() && b[i].is_ascii_digit() {
            i += 1;
        }
    }
    t[..i].parse::<f64>().unwrap_or(0.0)
}

fn parse(data: &[u8]) -> Result<PlistValue, JErr> {
    let src = decode(data);
    let n = src.len();
    P { src, i: 0, n }.top()
}

/// `[NSJSONSerialization JSONObjectWithData:data options:0 error:&err]`. May raise `NSRangeException`.
pub fn json_object_with_data(data: &[u8]) -> JsonResult {
    match parse(data) {
        Ok(v) => JsonResult { value: Some(v), error_description: None },
        Err(JErr::Parse(_)) => JsonResult { value: None, error_description: Some("JSON Parse error".to_string()) },
        Err(JErr::Nil) => JsonResult { value: None, error_description: None },
        Err(JErr::Exc(r)) => ns_raise("NSRangeException", &r),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn utf16(s: &str, le: bool, bom: bool) -> Vec<u8> {
        let mut u: Vec<u16> = s.encode_utf16().collect();
        if bom {
            u.insert(0, 0xfeff);
        }
        u.iter().flat_map(|x| if le { x.to_le_bytes() } else { x.to_be_bytes() }).collect()
    }
    fn utf32(s: &str, le: bool, bom: bool) -> Vec<u8> {
        let mut u: Vec<u32> = s.chars().map(|c| c as u32).collect();
        if bom {
            u.insert(0, 0xfeff);
        }
        u.iter().flat_map(|x| if le { x.to_le_bytes() } else { x.to_be_bytes() }).collect()
    }
    fn arr_len(d: &[u8]) -> usize {
        let PlistValue::Array(a) = parse(d).unwrap() else { panic!() };
        a.items.len()
    }

    #[test]
    fn encodings_are_detected() {
        let mut u8bom = vec![0xef, 0xbb, 0xbf];
        u8bom.extend_from_slice(b"[1,2]");
        assert_eq!(arr_len(&u8bom), 2);
        // the byte order mark of the wide encodings stays in the text and is not a JSON character
        for d in [
            utf16("[1,2]", true, true),
            utf16("[1,2]", false, true),
            utf32("[1,2]", true, true),
            utf32("[1,2]", false, true),
        ] {
            assert!(matches!(parse(&d), Err(JErr::Parse(_))));
        }
        assert_eq!(arr_len(&utf16("[1,2]", false, false)), 2);
        assert_eq!(arr_len(&utf16("[1,2]", true, false)), 2);
        assert_eq!(arr_len(&utf32("[1,2]", false, false)), 2);
        assert_eq!(arr_len(&utf32("[1,2]", true, false)), 2);
    }

    #[test]
    fn undecodable_text_is_empty() {
        for bad in [
            vec![0xef, 0xbb, 0xbf, 0xff, 0xfe],
            vec![0xff, 0xfe, 0x5b],
            vec![0xff, 0xfe, 0x00, 0xd8, 0x5b, 0x00],
            vec![0xff, 0xfe, 0, 0, 0x5b, 0, 0],
            vec![0xff, 0xfe, 0, 0, 0, 0, 0x11, 0],
        ] {
            assert!(parse(&bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn literals_and_numbers() {
        assert!(matches!(parse(b" true"), Ok(PlistValue::Bool(true))));
        assert!(matches!(parse(b"false"), Ok(PlistValue::Bool(false))));
        assert!(matches!(parse(b"null"), Ok(PlistValue::Null)));
        assert!(matches!(parse(b"[null]"), Err(JErr::Parse(_))));
        assert!(matches!(parse(b"tru"), Err(JErr::Parse(_))));
        assert!(matches!(parse(b"x"), Err(JErr::Parse(_))));
        let num = |s: &str| {
            let PlistValue::Real { value, .. } = parse(s.as_bytes()).unwrap() else { panic!() };
            value
        };
        assert_eq!(num("-"), 0.0);
        assert_eq!(num("1.5"), 1.5);
        assert_eq!(num("1e5x"), 100000.0);
        assert_eq!(num("1ex2"), 1.0);
        assert!(matches!(parse(b"1e"), Err(JErr::Exc(_))));
        assert_eq!(parse_number_text("-3.25abc"), -3.25);
        assert_eq!(parse_number_text("abc"), 0.0);
        assert_eq!(parse_number_text("inf"), 0.0);
        assert_eq!(parse_number_text(".5"), 0.5);
        assert_eq!(parse_number_text("7."), 7.0);
    }

    #[test]
    fn strings_escapes_and_nonchars() {
        let s = |d: &[u8]| {
            let PlistValue::String(s) = parse(d).unwrap() else { panic!() };
            s.s
        };
        assert_eq!(s(br#""a\nb\tc\rd\be\ff\/g\qh""#), "a\nb\tc\rd\u{8}e\u{c}f/gqh");
        assert_eq!(s(br#""\u0041\u00e9""#), "A\u{e9}");
        assert_eq!(s(br#""\uFEFFabc""#), "abc");
        assert_eq!(s("\"\u{1f600}\"".as_bytes()), "\u{1f600}");
        assert_eq!(s(b"\"ab\0"), "ab");
        assert!(matches!(parse(b" \"\0"), Err(JErr::Parse(_))));
        assert!(matches!(parse(b"\"unterminated"), Err(JErr::Exc(_))));
        assert!(matches!(parse(br#""\u12G4""#), Err(JErr::Parse(_))));
        for bad in [&br#""\uFDD0""#[..], br#""\uDC00""#, br#""\uD800""#, br#""\uFFFF""#, "\"\u{10fffe}\"".as_bytes()] {
            assert!(matches!(parse(bad), Err(JErr::Nil)), "{}", String::from_utf8_lossy(bad));
        }
        assert!(matches!(parse(b"\"\\uFDD0\0"), Err(JErr::Nil)));
        assert!(bad_units(&[0xd800, 0x41]));
        assert!(!bad_units(&[0xd83d, 0xde00]));
        assert!(bad_units(&[0xdbff, 0xdfff]));
    }

    #[test]
    fn containers() {
        let v = parse(br#" { "a" : [1, 2 ,3] , "b":{} "c":"x" } "#).unwrap();
        let PlistValue::Dict(d) = v else { panic!() };
        assert_eq!(d.map.len(), 3);
        assert!(matches!(parse(br#"{"a" 1}"#), Err(JErr::Parse(_))));
        assert!(matches!(parse(b"{1:2}"), Err(JErr::Parse(_))));
        assert!(matches!(parse(b"[1 2"), Err(JErr::Parse(_))));
        assert_eq!(arr_len(b"[1 2]"), 2);
    }

    #[test]
    fn public_entry_point() {
        let ok = json_object_with_data(b"[1]");
        assert!(ok.value.is_some() && ok.error_description.is_none());
        let bad = json_object_with_data(b"x");
        assert_eq!(bad.error_description.as_deref(), Some("JSON Parse error"));
        let nil = json_object_with_data(br#""\uFFFF""#);
        assert!(nil.value.is_none() && nil.error_description.is_none());
        let e = crate::foundation::exception::catch_ns(|| json_object_with_data(b"[1e")).unwrap_err();
        assert_eq!(e.name, "NSRangeException");
    }
}

#[cfg(test)]
#[path = "json_golden.rs"]
mod golden;
