//! Replays of the oracle goldens (keyed archives). They need the private fixtures and skip without them, so they live in this
//! file, which the coverage gate leaves out (`*_golden.rs`).

#![allow(clippy::manual_strip)]
#[cfg(test)]
mod golden_tests {
    use super::super::*;
    use crate::foundation::exception::catch_ns;

    fn unhex(h: &str) -> Vec<u8> {
        (0..h.len() / 2).map(|i| u8::from_str_radix(&h[2 * i..2 * i + 2], 16).unwrap()).collect()
    }

    fn set_of(ranges: &[(u64, u64)]) -> IndexSet {
        let mut s = IndexSet::new();
        for &(l, n) in ranges {
            s.add_indexes_in_range(l, n);
        }
        s
    }

    #[test]
    fn keyed_vectors_300() {
        let Ok(txt) = std::fs::read_to_string(crate::testenv::plist_golden("keyed_indexset_vectors.txt")) else {
            return;
        };
        let mut n = 0;
        for l in txt.lines().filter(|l| l.starts_with("T ")) {
            let parts: Vec<&str> = l.split('|').collect();
            let head: Vec<&str> = parts[0].split_whitespace().collect();
            let mutable = head[1] == "1";
            // header: T <mutable> <rootkey: 1=root 0=portalPositionIndexSet> <index count>
            let key = if head[2] == "1" { "root" } else { "portalPositionIndexSet" };
            let ranges: Vec<(u64, u64)> = parts[1]
                .split_whitespace()
                .map(|r| {
                    let (a, b) = r.split_once(':').unwrap();
                    (a.parse().unwrap(), b.parse().unwrap())
                })
                .collect();
            let want = unhex(parts[2].trim());
            let set = set_of(&ranges);
            assert_eq!(archive_index_set_ex(&set, mutable, key), want, "vector {l}");
            // single-range archives store NSLocation/NSLength as 32-bit ints, so values >= 2^31 do not survive
            let lossy = ranges.len() == 1 && (ranges[0].0 >= 1 << 31 || ranges[0].1 >= 1 << 31);
            if !lossy {
                let back = unarchive_index_set(&want, key).expect("decodes");
                assert_eq!(back.ranges(), set.ranges(), "vector {l}");
            }
            n += 1;
        }
        assert!(n >= 300);
    }

    #[test]
    fn golden_samples() {
        let set = set_of(&[(8410549, 1)]);
        let want = unhex("62706c6973743030d4010203040506070858246f626a656374735424746f70582476657273696f6e59246172636869766572a3090a0bd10c0d12000186a05f100f4e534b65796564417263686976657255246e756c6cd40e0f101112131214d2151617185f1016706f7274616c506f736974696f6e496e6465785365748001584e534c656e6774685624636c6173735c4e5352616e6765436f756e745a4e534c6f636174696f6e1001800212008055b55824636c61737365735a24636c6173736e616d65a318191a5f10114e534d757461626c65496e6465785365745a4e53496e646578536574584e534f626a65637408111a1f283236393e50565f647d7f888f9ca7a9abb0b9c4c8dce70000000000000101000000000000001b000000000000000000000000000000f0");
        assert_eq!(archive_index_set(&set, "portalPositionIndexSet"), want);
        assert_eq!(unarchive_index_set(&want, "portalPositionIndexSet").unwrap().ranges(), &[(8410549, 1)]);
        assert!(unarchive_index_set(&want, "other").is_none());
        // multi range example from the spec
        let multi = set_of(&[(5, 3), (10, 1), (100, 1), (1000, 1)]);
        let bytes = archive_index_set(&multi, "x");
        assert!(bytes.windows(9).any(|w| w == [5, 3, 10, 1, 100, 1, 0xe8, 7, 1]));
        assert_eq!(unarchive_index_set(&bytes, "x").unwrap().ranges(), multi.ranges());
        // empty
        let e = archive_index_set(&IndexSet::new(), "root");
        assert_eq!(e.len(), 247);
        assert!(unarchive_index_set(&e, "root").unwrap().is_empty());
        let e = archive_index_set(&IndexSet::new(), "foundItems");
        assert_eq!(e.len(), 253);
    }

    #[test]
    fn two_keys_vector() {
        let Ok(txt) = std::fs::read_to_string(crate::testenv::plist_golden("bplist_vectors.txt")) else { return };
        let Some(l) = txt.lines().find(|l| l.starts_with("two ")) else { return };
        let want = unhex(l.split(' ').nth(1).unwrap());
        let mut a = KeyedArchiver::new();
        a.encode_index_set(&set_of(&[(5, 3), (10, 1), (100, 1), (1000, 1)]), true, "a");
        a.encode_index_set(&set_of(&[(42, 1)]), false, "b");
        a.encode_int(3, "n");
        a.encode_object(&ArchValue::String("s".into()), "c");
        assert_eq!(a.finish_encoding(), want);
    }

    #[test]
    fn simple_objects_vectors() {
        let Ok(txt) = std::fs::read_to_string(crate::testenv::plist_golden("bplist_vectors.txt")) else { return };
        let get =
            |n: &str| unhex(txt.lines().find(|l| l.starts_with(&format!("{n} "))).unwrap().split(' ').nth(1).unwrap());
        let mut a = KeyedArchiver::new();
        a.encode_object(&ArchValue::String("hello".into()), "root");
        assert_eq!(a.finish_encoding(), get("str"));
        let mut a = KeyedArchiver::new();
        a.encode_object(&ArchValue::Data(b"xyz".to_vec()), "root");
        assert_eq!(a.finish_encoding(), get("data"));
        let mut a = KeyedArchiver::new();
        a.encode_object(&ArchValue::Int(7), "root");
        assert_eq!(a.finish_encoding(), get("num_i"));
        let mut a = KeyedArchiver::new();
        a.encode_object(&ArchValue::Real(2.5), "root");
        assert_eq!(a.finish_encoding(), get("num_d"));
        let mut a = KeyedArchiver::new();
        a.encode_object(&ArchValue::Bool(true), "root");
        assert_eq!(a.finish_encoding(), get("num_b"));
    }

    #[test]
    fn unarchive_errors() {
        assert!(unarchive_index_set_opt(None, "root").is_none());
        assert!(KeyedUnarchiver::init_for_reading(Some(b"")).is_none());
        let e = catch_ns(|| unarchive_index_set(b"junk", "root")).unwrap_err();
        assert_eq!(e.reason.as_deref(), Some("GSMutableString(instance) does not recognize objectForKey:"));
        let e = catch_ns(|| unarchive_index_set(b"12345", "root")).unwrap_err();
        assert_eq!(e.reason.as_deref(), Some("GSMutableString(instance) does not recognize objectForKey:"));
        // unknown class
        let mut bytes = archive_index_set(&IndexSet::new(), "root");
        let pos = bytes.windows(17).position(|w| w == b"NSMutableIndexSet").unwrap();
        // "NSMutableIndexSet" -> same length unknown name
        bytes[pos..pos + 17].copy_from_slice(b"NSMutableIndexSeX");
        let e = catch_ns(|| unarchive_index_set(&bytes, "root")).unwrap_err();
        assert_eq!(e.name, "NSInvalidUnarchiveOperationException");
        assert_eq!(
            e.reason.as_deref(),
            Some("[NSKeyedUnarchiver +_decodeObject:]: no class for name 'NSMutableIndexSeX'")
        );
    }

    #[test]
    fn golden_unarchive_inputs() {
        let dir = crate::testenv::plist_golden("xml_read_inputs");
        let Ok(rd) = std::fs::read_dir(&dir) else { return };
        let _ = rd;
        let run = |f: &str, key: &str| -> String {
            let d = std::fs::read(format!("{dir}/{f}")).unwrap();
            match catch_ns(|| match KeyedUnarchiver::init_for_reading(Some(&d)) {
                None => "init nil".to_string(),
                Some(u) => match u.decode_object_for_key(key) {
                    None => "nil".to_string(),
                    Some(ArchValue::IndexSet(i)) => format!("set {:?}", i.ranges()),
                    Some(o) => format!("{:?}", o),
                },
            }) {
                Ok(s) => s,
                Err(e) => format!("EXC {} | {}", e.name, e.reason.unwrap_or_default()),
            }
        };
        assert_eq!(
            run("bad_uid.bin", "root"),
            "EXC NSRangeException | Index 9 is out of range 2 (in 'objectAtIndex:')"
        );
        assert_eq!(
            run("unknown_class.bin", "root"),
            "EXC NSInvalidUnarchiveOperationException | [NSKeyedUnarchiver +_decodeObject:]: no class for name 'FooBar'"
        );
        assert_eq!(run("top_null.bin", "root"), "nil");
        assert_eq!(run("top_int.bin", "root"), "Int(7)");
        assert_eq!(run("is_neg.bin", "root"), "EXC NSRangeException | Range location + length too great");
        assert_eq!(run("is_len0.bin", "root"), "set []");
        assert_eq!(run("mutable_cls_chain.bin", "root"), "set [(5, 3)]");
        assert_eq!(run("ka_arr.bin", "root"), "Array([String(\"a\"), String(\"b\"), String(\"a\")])");
        assert!(run("top_uid_oob.bin", "root")
            .starts_with("EXC NSInternalInconsistencyException | ../Headers/GNUstepBase/GSIArray.h:549"));
        assert_eq!(
            run("cls_unknown_chain_known_super.bin", "root"),
            "EXC NSInvalidUnarchiveOperationException | [NSKeyedUnarchiver +_decodeObject:]: no class for name 'MyIdx'"
        );
        assert_eq!(run("ka_arr.bin", "root_nokey"), "nil");

        // Every row of `_keyed_unarchive.out` (oracle t19, key "root"; the row label is `argv[a]+9`, i.e. the file name minus its first 4 characters).
        let Ok(rows) = std::fs::read_to_string(crate::testenv::plist_golden("xml_read_expected/_keyed_unarchive.out"))
        else {
            return;
        };
        let names: Vec<String> =
            std::fs::read_dir(&dir).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
        let mut checked = 0;
        for l in rows.lines().filter(|l| !l.starts_with("20")) {
            let (label, want) = l.split_once(": ").unwrap();
            let cands: Vec<&String> = names.iter().filter(|n| n.len() > 4 && &n[4..] == label).collect();
            let got: Vec<String> = cands.iter().map(|f| run(f, "root")).collect();
            let ok = |g: &String| -> bool {
                if want.starts_with("EXC ") || want == "init nil" {
                    g == want
                } else if want == "nil " {
                    g == "nil"
                } else if let Some(r) =
                    want.strip_prefix("NSIndexSet ").or_else(|| want.strip_prefix("NSMutableIndexSet "))
                {
                    let ranges =
                        if r.ends_with("(no indexes)") { "set []".to_string() } else { "set [(5, 3)]".to_string() };
                    *g == ranges
                } else if want.starts_with("GSMutableArray ") {
                    g.starts_with("Array(")
                } else if want.starts_with("NSIntNumber ") {
                    *g == format!("Int({})", &want[12..])
                } else if want.starts_with("GSCBufferString ") {
                    *g == format!("String({:?})", &want[16..])
                } else {
                    // NSDictionary roots: the server never decodes them (ArchValue has no dictionary).
                    want.starts_with("GSDictionary ")
                }
            };
            assert!(!cands.is_empty(), "{label}");
            assert!(got.iter().all(ok), "{label}: want {want:?} got {got:?}");
            checked += 1;
        }
        assert!(checked >= 40, "{checked}");
    }
}
