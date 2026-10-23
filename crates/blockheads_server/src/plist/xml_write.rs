//! GNUstep XML property-list writer (format 100).
//!

use super::value::PlistValue;
use base64::Engine;

pub use crate::foundation::cfmt::fmt_g;

const HEADER: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE plist PUBLIC \"-//GNUstep//DTD plist 0.9//EN\" \"http://www.gnustep.org/plist-0_9.xml\">\n<plist version=\"0.9\">\n";

/// `-[NSString compare:]` ordering used for dictionary keys.
pub fn ns_string_compare(a: &str, b: &str) -> std::cmp::Ordering {
    if a.is_ascii() && b.is_ascii() {
        return a.as_bytes().cmp(b.as_bytes());
    }
    crate::foundation::nsstring::compare(a, b).cmp(&0)
}

/// Escape text exactly like the GNUstep writer (UTF-16 unit based).
pub fn escape_xml_text(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            '\u{01}'..='\u{08}' | '\u{0B}' | '\u{0C}' | '\u{0E}'..='\u{1F}' => {
                out.push_str(&format!("\\U{:04X}", ch as u32));
            }
            c if (c as u32) > 0xFFFF => {
                let mut buf = [0u16; 2];
                for u in c.encode_utf16(&mut buf) {
                    out.push_str(&format!("\\U{:04X}", u));
                }
            }
            c => out.push(c),
        }
    }
    out
}

/// `<real>` text: `%.7g` of the float-rounded value for `NSFloatNumber`, else `%.16g`.
pub fn format_real(value: f64, is_float: bool) -> String {
    if is_float {
        fmt_g(value as f32 as f64, 7)
    } else {
        fmt_g(value, 16)
    }
}

fn indent(out: &mut String, level: usize) {
    for _ in 0..level / 2 {
        out.push('\t');
    }
    if level % 2 == 1 {
        out.push_str("    ");
    }
}

/// Days since 1970-01-01 to (year, month, day). Proleptic Gregorian.
fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719468;
    let era = z.div_euclid(146097);
    let doe = z.rem_euclid(146097);
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// `<date>` text `YYYY-MM-DDTHH:MM:SSZ` (UTC, floor to the second).
pub fn format_date(ref_secs: f64) -> String {
    let unix = ref_secs.floor() + 978307200.0;
    let t = if unix.is_finite() { unix as i64 } else { 0 };
    let days = t.div_euclid(86400);
    let sod = t.rem_euclid(86400);
    let (y, m, d) = civil_from_days(days);
    format!("{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z", y, m, d, sod / 3600, (sod / 60) % 60, sod % 60)
}

fn write_node(out: &mut String, v: &PlistValue, level: usize) {
    indent(out, level);
    match v {
        PlistValue::String(s) => {
            out.push_str("<string>");
            out.push_str(&escape_xml_text(&s.s));
            out.push_str("</string>\n");
        }
        PlistValue::Int(i) => out.push_str(&format!("<integer>{}</integer>\n", i)),
        PlistValue::UInt(u) => out.push_str(&format!("<integer>{}</integer>\n", u)),
        PlistValue::Real { value, is_float } => {
            out.push_str(&format!("<real>{}</real>\n", format_real(*value, *is_float)))
        }
        PlistValue::Bool(b) => out.push_str(if *b { "<true/>\n" } else { "<false/>\n" }),
        PlistValue::Date(d) => out.push_str(&format!("<date>{}</date>\n", format_date(*d))),
        PlistValue::Data(d) => {
            out.push_str("<data>\n");
            out.push_str(&base64::engine::general_purpose::STANDARD.encode(&d.bytes));
            out.push_str("</data>\n");
        }
        PlistValue::Array(a) => {
            out.push_str("<array>\n");
            for it in a.items.iter() {
                write_node(out, it, level + 1);
            }
            indent(out, level);
            out.push_str("</array>\n");
        }
        PlistValue::Dict(d) => {
            out.push_str("<dict>\n");
            let mut keys: Vec<&String> = d.map.iter().map(|(k, _)| k).collect();
            keys.sort_by(|a, b| ns_string_compare(a, b));
            for k in keys {
                indent(out, level + 1);
                out.push_str("<key>");
                out.push_str(&escape_xml_text(k));
                out.push_str("</key>\n");
                if let Some(val) = d.map.get(k) {
                    write_node(out, val, level + 1);
                }
            }
            indent(out, level);
            out.push_str("</dict>\n");
        }
        PlistValue::Null => {
            // observed quirk of the default branch: no trailing newline.
            out.push_str("<string>");
            out.push_str(&escape_xml_text(&v.description()));
            out.push_str("</string>");
        }
    }
}

/// GNUstep XML document (format 100).
pub fn write_xml(v: &PlistValue) -> Vec<u8> {
    let mut out = String::from(HEADER);
    write_node(&mut out, v, 0);
    out.push_str("</plist>");
    out.into_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::foundation::gsdict::GsDict;

    #[test]
    fn real_edge_cases() {
        assert_eq!(format_real(0.0, false), "0");
        assert_eq!(format_real(-0.0, false), "-0");
        assert_eq!(format_real(1e16, false), "1e+16");
        assert_eq!(format_real(1e-5, false), "1e-05");
        assert_eq!(format_real(f64::INFINITY, false), "inf");
        assert_eq!(format_real(f64::NEG_INFINITY, false), "-inf");
        assert_eq!(format_real(f64::from_bits(0xfff8000000000000), false), "-nan");
        assert_eq!(format_real(8145.54f32 as f64, true), "8145.54");
        assert_eq!(format_real(0.1f32 as f64, true), "0.1");
        assert_eq!(format_real(1.0 / 3.0, false), "0.3333333333333333");
        assert_eq!(fmt_g(100000.0, 6), "100000");
        assert_eq!(fmt_g(1e6, 6), "1e+06");
    }

    #[test]
    fn dates() {
        assert_eq!(format_date(1.5 - 978307200.0), "1970-01-01T00:00:01Z");
        assert_eq!(format_date(-0.5 - 978307200.0), "1969-12-31T23:59:59Z");
        assert_eq!(format_date(253402300800.0 - 978307200.0), "10000-01-01T00:00:00Z");
    }

    #[test]
    fn escapes_and_layout() {
        assert_eq!(escape_xml_text("a&<>\"'"), "a&amp;&lt;&gt;&quot;&apos;");
        assert_eq!(escape_xml_text("\u{1}\u{B}\t\n\r\0\u{7f}"), "\\U0001\\U000B\t\n\r\0\u{7f}");
        assert_eq!(escape_xml_text("\u{1F600}"), "\\UD83D\\UDE00");
        let mut d: GsDict<String, PlistValue> = GsDict::new();
        d.insert(
            "b".into(),
            PlistValue::array(vec![
                PlistValue::array(vec![PlistValue::int(1)]),
                PlistValue::Null,
                PlistValue::data(vec![]),
            ]),
        );
        d.insert("a".into(), PlistValue::Bool(true));
        let s = String::from_utf8(write_xml(&PlistValue::dict(d))).unwrap();
        let body = s.strip_prefix(HEADER).unwrap();
        assert_eq!(
            body,
            "<dict>\n    <key>a</key>\n    <true/>\n    <key>b</key>\n    <array>\n\t<array>\n\t    <integer>1</integer>\n\t</array>\n\t<string>&lt;null&gt;</string>\t<data>\n</data>\n    </array>\n</dict>\n</plist>"
        );
        let top = String::from_utf8(write_xml(&PlistValue::string("top"))).unwrap();
        assert!(top.ends_with("<plist version=\"0.9\">\n<string>top</string>\n</plist>"));
    }

    #[test]
    fn every_node_kind_and_key_order() {
        let mut d: GsDict<String, PlistValue> = GsDict::new();
        d.insert("\u{e9}".into(), PlistValue::UInt(u64::MAX));
        d.insert("z".into(), PlistValue::real_f64(1.5));
        d.insert("a".into(), PlistValue::date(0.0));
        d.insert("m".into(), PlistValue::data(vec![1, 2, 3]));
        d.insert("n".into(), PlistValue::string("x"));
        d.insert("b".into(), PlistValue::Bool(false));
        let s = String::from_utf8(write_xml(&PlistValue::dict(d))).unwrap();
        let pos = |k: &str| s.find(&format!("<key>{k}</key>")).unwrap();
        assert!(pos("a") < pos("b") && pos("b") < pos("m") && pos("m") < pos("n") && pos("n") < pos("z"));
        assert!(s.contains("<integer>18446744073709551615</integer>"));
        assert!(s.contains("<real>1.5</real>"));
        assert!(s.contains("<date>2001-01-01T00:00:00Z</date>"));
        assert!(s.contains("<data>\nAQID</data>"));
        assert!(s.contains("<false/>"));
        assert_eq!(ns_string_compare("\u{e9}", "e"), std::cmp::Ordering::Greater);
        assert_eq!(ns_string_compare("e", "\u{e9}"), std::cmp::Ordering::Less);
        assert_eq!(ns_string_compare("\u{e9}", "\u{e9}"), std::cmp::Ordering::Equal);
    }
}

#[cfg(test)]
#[path = "xml_write_golden.rs"]
mod golden;
