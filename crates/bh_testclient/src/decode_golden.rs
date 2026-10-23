//! Replays of the captured protocol vectors. They need the private fixtures and skip without them, so they live in this file,
//! which the coverage gate leaves out (`*_golden.rs`).

#[cfg(test)]
mod golden_tests {
    use super::super::*;

    fn vectors_dir() -> Option<std::path::PathBuf> {
        let p = crate::harness::private_docs().join("protocol_vectors");
        p.is_dir().then_some(p)
    }

    /// U2: every captured server packet decodes without MALFORMED; the solo 0x04 vector prints the known hash.
    #[test]
    fn u2_vectors() {
        let Some(dir) = vectors_dir() else { return };
        let mut n = 0;
        for sub in ["solo", "two_clients"] {
            let Ok(rd) = std::fs::read_dir(dir.join(sub)) else { continue };
            for e in rd.flatten() {
                let name = e.file_name().to_string_lossy().into_owned();
                if !name.ends_with(".bin") || name.starts_with("A_") && false {
                    continue;
                }
                let data = std::fs::read(e.path()).unwrap();
                let d = decode_packet(Dir::Rx, &data);
                assert!(!d.summary.contains("MALFORMED"), "{}: {}", name, d.summary);
                if sub == "solo" && name.starts_with("007_") {
                    assert!(d.summary.contains("idx=0 "), "{}", d.summary);
                    assert!(d.summary.contains("ver=0 upd=0"), "{}", d.summary);
                    assert!(d.summary.contains("hash=1128c17e53085db291e4b0d5393d0420"), "{}", d.summary);
                }
                n += 1;
            }
        }
        assert!(n > 0);
    }
}
