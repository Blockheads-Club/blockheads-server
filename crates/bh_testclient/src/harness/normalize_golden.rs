//! Replays of the protocol vectors of the original. They need the private fixtures and skip without them, so they live in this
//! file, which the coverage gate leaves out (`*_golden.rs`).

#[cfg(test)]
mod golden_tests {
    use super::super::*;
    use std::path::PathBuf;

    fn opts() -> NormalizeOptions {
        NormalizeOptions {
            padding: PaddingMeta::load(&super::super::super::meta_json_path()).unwrap_or_default(),
            ..Default::default()
        }
    }
    fn vec_dir(name: &str) -> PathBuf {
        super::super::super::private_docs().join("protocol_vectors").join(name)
    }
    fn load_bins(name: &str, prefix_filter: &dyn Fn(&str) -> bool) -> Vec<TraceEvent> {
        let dir = vec_dir(name);
        let mut files: Vec<PathBuf> = match std::fs::read_dir(&dir) {
            Ok(r) => r.map(|e| e.unwrap().path()).filter(|p| p.extension().map_or(false, |x| x == "bin")).collect(),
            Err(_) => {
                eprintln!("SKIP: {} missing", dir.display());
                return vec![];
            }
        };
        files.sort();
        files
            .into_iter()
            .filter(|p| prefix_filter(p.file_name().unwrap().to_str().unwrap()))
            .enumerate()
            .map(|(i, p)| TraceEvent {
                t_ms: i as f64 * 100.0,
                dir: Dir::Rx,
                channel: 0,
                reliable: true,
                client: "TCA".into(),
                bytes: std::fs::read(p).unwrap(),
            })
            .collect()
    }
    /// S10 part 1: golden vectors normalise to themselves regardless of timestamps.
    #[test]
    fn s10_golden_time_independent() {
        for dir in ["solo", "two_clients"] {
            let ev = load_bins(dir, &|n| !n.starts_with("B_"));
            if ev.is_empty() {
                continue;
            }
            let a = normalize_trace(&ev, &opts());
            let mut ev2 = ev.clone();
            for (i, e) in ev2.iter_mut().enumerate() {
                e.t_ms = 5.0 + (i as f64) * 777.0;
            }
            let b = normalize_trace(&ev2, &opts());
            assert!(!a.is_empty());
            assert_eq!(a.len(), b.len());
            for (x, y) in a.iter().zip(b.iter()) {
                assert_eq!(x.sections, y.sections);
            }
            // block messages must have been split into sections
            if let Some(m) = a.iter().find(|m| m.ty == 0x04) {
                assert!(m.sections.iter().any(|(n, _)| n == "t.tiles"), "{}", m.text);
            }
        }
    }
}
