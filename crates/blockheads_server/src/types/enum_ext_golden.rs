//! Needs the private docs (skipped without them); left out of the coverage gate (`*_golden.rs`).
use super::tests::assert_rows_present;

/// Every `| `NAME` | value |` row of (non-ENet/MDB) must exist with same value
/// somewhere in the generated sources.
#[test]
fn enums_md_rows_present() {
    let md = crate::testenv::private_docs().join("enums.md");
    if md.exists() {
        assert_rows_present(&md, 401);
    }
}
