//! Replays of the oracle goldens (bplist vectors). They need the private fixtures and skip without them, so they live in
//! this file, which the coverage gate leaves out (`*_golden.rs`).

#[cfg(test)]
mod golden_vectors {
    use super::super::super::value::PlistValue as V;
    use super::super::*;

    fn vec_hex(name: &str) -> Option<Vec<u8>> {
        let txt = std::fs::read_to_string(crate::testenv::plist_golden("bplist_vectors.txt")).ok()?;
        for l in txt.lines() {
            let mut it = l.split(' ');
            if it.next() == Some(name) {
                let h = it.next()?;
                return Some((0..h.len() / 2).map(|i| u8::from_str_radix(&h[2 * i..2 * i + 2], 16).unwrap()).collect());
            }
        }
        None
    }

    fn arr(items: Vec<V>) -> V {
        V::array(items)
    }

    #[test]
    fn generic_vectors() {
        let Some(want) = vec_hex("ints") else { return };
        let ints =
            [0i64, 1, 255, 256, 65535, 65536, 4294967295, 4294967296, -1, -128, -129, -32768, -2147483648, i64::MAX];
        assert_eq!(write_bplist(&arr(ints.iter().map(|&i| V::int(i)).collect())), want);

        let reals = vec![
            V::real_f64(1.5),
            V::real_f32(1.5),
            V::real_f64(0.1),
            V::real_f32(0.1),
            V::real_f64(2.0),
            V::real_f64(-0.0),
            V::Bool(true),
            V::Bool(false),
            V::int(1),
            V::real_f64(1.0),
            V::UInt(u64::MAX),
        ];
        assert_eq!(write_bplist(&arr(reals)), vec_hex("reals").unwrap());

        let strs = vec![
            V::string(""),
            V::string("abc"),
            V::string("\u{20ac}a"),
            V::string("a\u{e9}"),
            V::string("abc"),
            V::string(format!("{:0100}", 1)),
        ];
        assert_eq!(write_bplist(&arr(strs)), vec_hex("strs").unwrap());

        let misc = vec![
            V::date(0.0),
            V::date(1760000000.5 - 978307200.0),
            V::data(vec![]),
            V::data(b"abc".to_vec()),
            V::data(b"abc".to_vec()),
            arr(vec![]),
            arr(vec![]),
            V::dict(GsDict::new()),
            arr(vec![V::string("abc")]),
            arr(vec![V::string("abc")]),
        ];
        assert_eq!(write_bplist(&arr(misc)), vec_hex("misc").unwrap());
        assert_eq!(write_bplist(&arr(vec![])), vec_hex("emptyarr").unwrap());
        assert_eq!(write_bplist(&V::string("x")), vec_hex("topstr").unwrap());
        assert_eq!(write_bplist(&V::int(300)), vec_hex("topint").unwrap());
        let mut big: Vec<V> = (0..20).map(V::int).collect();
        big.push(V::string("0123456789abcdefghij"));
        assert_eq!(write_bplist(&arr(big)), vec_hex("bigarr").unwrap());
        let a300 = arr((0..300).map(|i| V::int(1000 + i)).collect());
        let got = write_bplist(&a300);
        assert_eq!(&got[..40], &vec_hex("arr300_head").unwrap()[..]);
    }
}

#[cfg(test)]
mod golden_read {
    use super::super::*;
    #[test]
    fn corrupt_trailers() {
        let dir = crate::testenv::plist_golden("xml_read_inputs");
        for (f, want) in [
            ("z_badtrailer.bin", "Unknown offset size 255"),
            ("z_trunc.bin", "Unknown offset size 0"),
            ("arr300_head_821.bin", "Unknown offset size 0"),
        ] {
            let Ok(d) = std::fs::read(format!("{dir}/{f}")) else { return };
            let e = read_bplist(&d, 0).unwrap_err();
            assert_eq!(e.reason.as_deref(), Some(want), "{f}");
        }
        for f in [
            "apple_bin.plist",
            "apple_bin_uid.plist",
            "ka_arr.bin",
            "ka_dict.bin",
            "top_string.bin",
            "reals_751.bin",
            "strs_633.bin",
            "misc_190.bin",
            "ints_586.bin",
            "bigarr_716.bin",
            "dict_996.bin",
        ] {
            let Ok(d) = std::fs::read(format!("{dir}/{f}")) else { return };
            assert!(read_bplist(&d, 0).unwrap().is_some(), "{f}");
        }
    }
}

#[cfg(test)]
mod golden_dicts {
    //! The 12 `dict` rows of `bplist_vectors.txt` (oracle t8: `[NSMutableDictionary dictionary]`, keys `key<i>_<r>` inserted for
    //! i = 0..n with value i). Exercises BFS keys-then-values order in `GsDict` enumeration order.
    use super::super::*;
    #[test]
    fn dict_rows() {
        let Ok(txt) = std::fs::read_to_string(crate::testenv::plist_golden("bplist_vectors.txt")) else { return };
        let mut n = 0;
        for l in txt.lines().filter(|l| l.starts_with("dict ")) {
            let h = l.split(' ').nth(1).unwrap();
            let want: Vec<u8> =
                (0..h.len() / 2).map(|i| u8::from_str_radix(&h[2 * i..2 * i + 2], 16).unwrap()).collect();
            let Some(PlistValue::Dict(d)) = read_bplist(&want, 0).unwrap() else { panic!("{l}") };
            if d.map.contains_key(&"$objects".to_string()) {
                continue; // the keyed-archive `dict` row (NSDictionary archive), not a t8 row
            }
            let mut keys: Vec<(i64, String)> = d.map.iter().map(|(k, v)| (v.long_long_value(), k.clone())).collect();
            keys.sort();
            let mut m = GsDict::new();
            for (i, k) in keys {
                m.insert(k, PlistValue::int(i));
            }
            assert_eq!(write_bplist(&PlistValue::dict(m)), want, "{l}");
            n += 1;
        }
        assert_eq!(n, 12);
    }
}
