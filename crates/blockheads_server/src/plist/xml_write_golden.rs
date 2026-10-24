//! Replays of the oracle goldens. They need the private fixtures and skip without them, so they live in this file, which
//! the coverage gate leaves out (`*_golden.rs`).

#[cfg(test)]
mod golden_tests {
    use super::super::*;

    #[test]
    fn real_double_and_float_vectors() {
        for (file, is_float) in [("xml_real_double.txt", false), ("xml_real_float.txt", true)] {
            let Ok(txt) = std::fs::read_to_string(crate::testenv::plist_golden(file)) else { return };
            let mut n = 0;
            for l in txt.lines().filter(|l| !l.starts_with('#') && !l.is_empty()) {
                let mut it = l.split(' ');
                let hex = it.next().unwrap();
                let bits = u64::from_str_radix(hex, 16).unwrap();
                let text = it.next().unwrap();
                let v = if hex.len() == 8 { f32::from_bits(bits as u32) as f64 } else { f64::from_bits(bits) };
                assert_eq!(format_real(v, is_float), text, "{l}");
                n += 1;
            }
            assert!(n > 1000);
        }
    }

    #[test]
    fn key_order_pairs() {
        let Ok(txt) = std::fs::read_to_string(crate::testenv::plist_golden("nscompare_pairs.txt")) else { return };
        let dec = |s: &str| -> String {
            let u: Vec<u16> =
                s.split(' ').filter(|x| !x.is_empty()).map(|h| u16::from_str_radix(h, 16).unwrap()).collect();
            String::from_utf16_lossy(&u)
        };
        let mut bad = 0;
        let mut n = 0;
        for l in txt.lines().filter(|l| !l.starts_with('#') && !l.is_empty()) {
            let (r, rest) = l.split_once(' ').unwrap();
            let (a, b) = rest.split_once('|').unwrap();
            let (a, b) = (dec(a), dec(b));
            if a.contains('\u{FFFD}') || b.contains('\u{FFFD}') {
                continue;
            }
            n += 1;
            let want: i32 = r.parse().unwrap();
            let got = match ns_string_compare(&a, &b) {
                std::cmp::Ordering::Less => -1,
                std::cmp::Ordering::Equal => 0,
                std::cmp::Ordering::Greater => 1,
            };
            if got != want {
                bad += 1;
            }
        }
        assert!(n > 10000);
        assert_eq!(bad, 0);
    }

    #[test]
    fn all_bmp_strings_file() {
        let Ok(want) = std::fs::read(crate::testenv::plist_golden("xml_all_bmp_strings.xml")) else { return };
        let mut items = Vec::new();
        for c in 0u32..=0xFFFF {
            if (0xD800..0xE000).contains(&c) || (0xFDD0..=0xFDEF).contains(&c) || c == 0xFFFE || c == 0xFFFF {
                continue;
            }
            items.push(PlistValue::string(format!("X{}Y", char::from_u32(c).unwrap())));
        }
        assert_eq!(write_xml(&PlistValue::array(items)), want);
    }
}
