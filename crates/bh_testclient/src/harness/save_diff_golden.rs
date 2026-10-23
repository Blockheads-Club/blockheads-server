//! Replays of the reference saves. They need the private reference environment and skip without it, so they live in this
//! file, which the coverage gate leaves out (`*_golden.rs`).

#[cfg(test)]
mod golden_tests {
    use super::super::*;
    use std::path::{Path, PathBuf};

    fn sample(rel: &str) -> Option<PathBuf> {
        let p = super::super::super::refenv_dir().join("samples").join(rel);
        if p.exists() {
            Some(p)
        } else {
            eprintln!("SKIP: sample {} missing", rel);
            None
        }
    }
    fn only_child(p: &Path) -> PathBuf {
        resolve_save(p).unwrap_or_else(|_| {
            let mut v: Vec<PathBuf> =
                std::fs::read_dir(p).unwrap().map(|e| e.unwrap().path()).filter(|x| x.is_dir()).collect();
            v.sort();
            v.remove(0)
        })
    }
    fn save_of(rel: &str) -> Option<PathBuf> {
        sample(rel).map(|p| only_child(&p))
    }

    /// S1.
    #[test]
    fn s1_rep1_rep2() {
        let (Some(a), Some(b)) = (save_of("rep1"), save_of("rep2")) else { return };
        let r = diff_saves(&a, &b, &SaveDiffOptions::default()).unwrap();
        assert!(r.compared_records > 0);
        assert_eq!(r.diffs.len(), 1, "{}", r.render(50));
        match &r.diffs[0] {
            RecordDiff::Plist { id, diffs } => {
                assert_eq!(id.to_string(), "world_db/main/worldv2");
                for d in diffs {
                    assert!(["/creationDate", "/saveDate", "/hostPort"].contains(&d.path.as_str()), "{}", d.path);
                }
            }
            o => panic!("{:?}", o),
        }
        assert_eq!(r.identical, 416);
        let c = diff_saves(&a, &b, &SaveDiffOptions::parity_default()).unwrap();
        assert!(c.is_clean(), "{}", c.render(50));
    }

    /// S2/S3.
    #[test]
    fn s2_s3_seeds() {
        let (Some(a), Some(b)) = (save_of("seedA"), save_of("seedB")) else { return };
        let r = diff_saves(&a, &b, &SaveDiffOptions::default()).unwrap();
        let ids: Vec<String> = r.diffs.iter().map(|d| d.id().to_string()).collect();
        assert_eq!(ids, vec!["file//worldv2".to_string(), "world_db/main/worldv2".to_string()], "{}", r.render(50));
        assert_eq!(r.identical, 326);
        assert!(diff_saves(&a, &b, &SaveDiffOptions::parity_default()).unwrap().is_clean());
        if let Some(c) = save_of("seedC") {
            let r = diff_saves(&a, &c, &SaveDiffOptions::default()).unwrap();
            for d in &r.diffs {
                let id = d.id().to_string();
                assert!(id == "file//worldv2" || id == "world_db/main/worldv2", "{}", id);
            }
            assert!(r.identical >= 320, "{}", r.render(20));
        }
    }

    /// S4 + S5 + S7.
    #[test]
    fn s4_s5_s7() {
        let (Some(a), Some(b)) = (save_of("w1"), save_of("expert")) else { return };
        let r = diff_saves(&a, &b, &SaveDiffOptions::parity_default()).unwrap();
        assert!(!r.is_clean());
        assert!(r.diffs.iter().any(|d| matches!(d, RecordDiff::Block { differing_tiles, .. } if *differing_tiles > 0)));
        let (ok, bad) = regzip_check(&a).unwrap();
        assert_eq!((ok, bad.len()), (398, 0));
        // S7: read-only open leaves lock.mdb alone
        let lock = a.join("world_db/lock.mdb");
        let before = (std::fs::metadata(&lock).unwrap().modified().unwrap(), std::fs::read(&lock).unwrap());
        let _ = load_records(&a).unwrap();
        let after = (std::fs::metadata(&lock).unwrap().modified().unwrap(), std::fs::read(&lock).unwrap());
        assert!(before == after);
        let s = render_block_grid(&a, 198, 18, "typeIndex").unwrap();
        assert!(s.lines().count() == 33, "{}", s);
        assert!(summary(&a).unwrap().contains("db \"blocks\""));
    }

    /// S6. Mutation tests on a copy of w1.
    #[test]
    fn s6_mutations() {
        use heed::types::Bytes;
        let Some(w1) = save_of("w1") else { return };
        let tmp = super::super::super::TempDir::new("s6").unwrap();
        let copy = tmp.path().join("save");
        fn cp(a: &Path, b: &Path) {
            std::fs::create_dir_all(b).unwrap();
            for e in std::fs::read_dir(a).unwrap() {
                let e = e.unwrap();
                let t = b.join(e.file_name());
                if e.file_type().unwrap().is_dir() {
                    cp(&e.path(), &t);
                } else {
                    std::fs::copy(e.path(), t).unwrap();
                }
            }
        }
        cp(&w1, &copy);
        let o = SaveDiffOptions::default();
        let put = |db: &str, key: &str, f: &dyn Fn(Vec<u8>) -> Option<Vec<u8>>| {
            // SAFETY: private temp copy, no other process uses it.
            let env = unsafe { heed::EnvOpenOptions::new().max_dbs(16).map_size(1 << 30).open(copy.join("world_db")) }
                .unwrap();
            let mut w = env.write_txn().unwrap();
            let d = env.create_database::<Bytes, Bytes>(&mut w, Some(db)).unwrap();
            let old = d.get(&w, key.as_bytes()).unwrap().unwrap().to_vec();
            match f(old) {
                Some(n) => d.put(&mut w, key.as_bytes(), &n).unwrap(),
                None => {
                    d.delete(&mut w, key.as_bytes()).unwrap();
                }
            }
            w.commit().unwrap();
        };
        // block tile byte
        put("blocks", "198_18", &|v| {
            let mut raw = gzip_inflate(&v).unwrap();
            raw[(2 * 32 + 14) * 64] = raw[(2 * 32 + 14) * 64].wrapping_add(1);
            Some(gzip_deflate(&raw))
        });
        let r = diff_saves(&w1, &copy, &o).unwrap();
        assert_eq!(r.diffs.len(), 1);
        let txt = r.render(20);
        assert!(txt.contains("blocks 198_18 tile(14,2) world(6350,578) typeIndex"), "{}", txt);
        // delete record
        put("blocks", "198_18", &|_| None);
        let r = diff_saves(&w1, &copy, &o).unwrap();
        assert!(r.diffs.iter().any(|d| matches!(d, RecordDiff::OnlyInA(_))));
    }
}
