//! Replays of the oracle JSON corpus. It needs the private fixtures and skips without them, so it lives in this file, which
//! the coverage gate leaves out (`*_golden.rs`).

mod testutil {

    /// `[(input bytes, expected)]` from `json_corpus.json` (tiny extractor for the fixed shape).
    pub fn load_json_corpus() -> Option<Vec<(Vec<u8>, String)>> {
        let txt = std::fs::read_to_string(crate::testenv::plist_golden("json_corpus.json")).ok()?;
        let mut rows = Vec::new();
        let mut rest = txt.as_str();
        while let Some(p) = rest.find("{\"in_hex\": \"") {
            rest = &rest[p + 12..];
            let e = rest.find('"').unwrap();
            let h = &rest[..e];
            let bytes: Vec<u8> =
                (0..h.len() / 2).map(|i| u8::from_str_radix(&h[2 * i..2 * i + 2], 16).unwrap()).collect();
            rest = &rest[e..];
            let p = rest.find("\"expected\": \"").unwrap();
            rest = &rest[p + 13..];
            // JSON string with escapes
            let mut out = String::new();
            let mut it = rest.char_indices();
            let mut end = 0;
            let mut units: Vec<u16> = Vec::new();
            let flush = |units: &mut Vec<u16>, out: &mut String| {
                if !units.is_empty() {
                    out.push_str(&String::from_utf16_lossy(units));
                    units.clear();
                }
            };
            while let Some((i, c)) = it.next() {
                match c {
                    '"' => {
                        end = i;
                        break;
                    }
                    '\\' => {
                        let (_, e) = it.next().unwrap();
                        match e {
                            'u' => {
                                let hex: String = (0..4).map(|_| it.next().unwrap().1).collect();
                                units.push(u16::from_str_radix(&hex, 16).unwrap());
                            }
                            'n' => {
                                flush(&mut units, &mut out);
                                out.push('\n')
                            }
                            't' => {
                                flush(&mut units, &mut out);
                                out.push('\t')
                            }
                            'r' => {
                                flush(&mut units, &mut out);
                                out.push('\r')
                            }
                            other => {
                                flush(&mut units, &mut out);
                                out.push(other)
                            }
                        }
                    }
                    c => {
                        flush(&mut units, &mut out);
                        out.push(c)
                    }
                }
            }
            flush(&mut units, &mut out);
            rest = &rest[end + 1..];
            rows.push((bytes, out));
        }
        Some(rows)
    }
}

#[cfg(test)]
mod tests {
    use super::super::*;
    use super::testutil;

    fn canon(v: &PlistValue, out: &mut String) {
        match v {
            PlistValue::Bool(b) => out.push_str(&format!("B{}", *b as i32)),
            PlistValue::Null => out.push_str("null"),
            PlistValue::Real { value, .. } => out.push_str(&format!("N{:016x}", value.to_bits())),
            PlistValue::String(s) => {
                out.push('S');
                out.push('"');
                for u in s.s.encode_utf16() {
                    out.push_str(&format!("{:04x}", u));
                }
                out.push('"');
            }
            PlistValue::Array(a) => {
                out.push('[');
                for x in a.items.iter() {
                    canon(x, out);
                    out.push(',');
                }
                out.push(']');
            }
            PlistValue::Dict(d) => {
                let mut ks: Vec<&String> = d.map.iter().map(|(k, _)| k).collect();
                ks.sort_by(|a, b| crate::foundation::nsstring::compare(a, b).cmp(&0));
                out.push('{');
                for k in ks {
                    canon(&PlistValue::string(k.clone()), out);
                    out.push(':');
                    canon(d.map.get(k).unwrap(), out);
                    out.push(',');
                }
                out.push('}');
            }
            _ => out.push('?'),
        }
    }

    fn run(data: &[u8]) -> String {
        match crate::foundation::exception::catch_ns(|| parse(data)) {
            Ok(Ok(v)) => {
                let mut s = String::from("OK ");
                canon(&v, &mut s);
                s
            }
            Ok(Err(JErr::Parse(r))) => format!("ERR JSON Parse error | {}", r),
            Ok(Err(JErr::Nil)) => "ERR (null) | ".to_string(),
            Ok(Err(JErr::Exc(r))) => format!("EXC NSRangeException | {}", r),
            Err(e) => panic!("{:?}", e),
        }
    }

    #[test]
    fn corpus() {
        let Some(rows) = testutil::load_json_corpus() else { return };
        assert!(rows.len() >= 15000);
        let mut bad = 0;
        for (inp, want) in &rows {
            let got = run(inp);
            if &got != want {
                bad += 1;
                if bad < 10 {
                    eprintln!("MISMATCH in={:?}\n  got  {}\n  want {}", String::from_utf8_lossy(inp), got, want);
                }
            }
        }
        assert_eq!(bad, 0);
    }

    #[test]
    fn cli_cases() {
        let r = json_object_with_data(b"not json");
        assert!(r.value.is_none());
        assert_eq!(r.error_description.as_deref(), Some("JSON Parse error"));
        let r = json_object_with_data(br#"{"a":true}"#);
        assert_eq!(r.error_description.as_deref(), Some("JSON Parse error"));
        let r = json_object_with_data(br#"{"a":"\ud83d"}"#);
        assert!(r.value.is_none() && r.error_description.is_none());
        let r = json_object_with_data(br#"{"seed":"custom","sunColor_0":255}"#);
        let v = r.value.unwrap();
        assert_eq!(v.class_name(), "GSDictionary");
        assert!(
            matches!(v.object_for_key("sunColor_0"), Some(PlistValue::Real { value, is_float: false }) if *value == 255.0)
        );
        let r = json_object_with_data(b"[1,2]");
        assert_eq!(r.value.unwrap().class_name(), "GSArray");
        let e = crate::foundation::exception::catch_ns(|| json_object_with_data(br#"{"a":"x"#)).unwrap_err();
        assert_eq!(e.name, "NSRangeException");
        assert_eq!(e.reason.as_deref(), Some("in getCharacters:range:, range { 8, 64 } extends beyond size (7)"));
    }
}
