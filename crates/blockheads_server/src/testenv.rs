//! Test-only access to private reference material (never shipped, never required).
//!
//! Some tests compare against golden data derived from the original server. That material is not part of
//! this repository. Point `BH_PRIVATE_DOCS` at a directory holding the optional comparison fixtures
//! (`protocol_vectors/`, `plist_golden/`, `app_cli_getopt/`, `cli_golden.json`, `enums.md`, ...), and
//! `BH_REFENV` at the reference-environment directory; when unset, those tests skip cleanly
//! (the returned paths do not exist, and every consumer treats a missing file as "skip").

use std::path::PathBuf;

const MISSING: &str = "/nonexistent/bh-private-material";

/// Root of the optional fixtures directory (`BH_PRIVATE_DOCS`), or a path that does not exist.
pub fn private_docs() -> PathBuf {
    std::env::var_os("BH_PRIVATE_DOCS").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(MISSING))
}

/// `<fixtures>/<rel>` as a string.
pub fn modules(rel: &str) -> String {
    private_docs().join(rel).to_string_lossy().into_owned()
}

/// `<fixtures>/plist_golden/<rel>` as a string.
pub fn plist_golden(rel: &str) -> String {
    modules(&format!("plist_golden/{rel}"))
}

/// Reference-environment sample directory (`BH_REFENV`/samples), or a path that does not exist.
pub fn refenv_samples() -> String {
    std::env::var_os("BH_REFENV")
        .map(|p| PathBuf::from(p).join("samples"))
        .unwrap_or_else(|| PathBuf::from(MISSING))
        .to_string_lossy()
        .into_owned()
}
