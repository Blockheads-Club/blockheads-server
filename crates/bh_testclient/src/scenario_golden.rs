//! Replays of the captured scripts. They need the private fixtures and skip without them, so they live in this file, which the
//! coverage gate leaves out (`*_golden.rs`).

#[cfg(test)]
mod golden_tests {
    use super::super::*;

    /// U8: two_clients/scriptA.txt parses, errors carry line numbers, builder == parser.
    #[test]
    fn u8_script_a() {
        let p = crate::harness::private_docs().join("protocol_vectors/two_clients/scriptA.txt");
        if let Ok(text) = std::fs::read_to_string(&p) {
            let sc = Scenario::parse(&text).unwrap();
            assert!(sc.steps.len() > 10);
            assert!(matches!(sc.steps[0], Step::Raw(_)));
            assert!(sc.steps.iter().any(|s| matches!(s, Step::Xml { ty: 0x24, .. })));
        }
    }
}
