//! Test-only access to private reference material (never shipped, never required).
//!
//! Some tests compare against golden data derived from the original server. That material is not part of
//! this repository. Point `BH_PRIVATE_DOCS` at a directory holding the optional comparison fixtures
//! (`protocol_vectors/`, `plist_golden/`, `app_cli_getopt/`, `cli_golden.json`, `enums.md`, ...), and
//! `BH_REFENV` at the reference-environment directory; when unset, those tests skip cleanly
//! (the returned paths do not exist, and every consumer treats a missing file as "skip").

use std::ffi::OsString;
use std::path::PathBuf;

const MISSING: &str = "/nonexistent/bh-private-material";

/// Root of the optional fixtures directory (`BH_PRIVATE_DOCS`), or a path that does not exist.
pub fn private_docs() -> PathBuf {
    private_docs_in(std::env::var_os("BH_PRIVATE_DOCS"))
}

fn private_docs_in(var: Option<OsString>) -> PathBuf {
    match var {
        Some(p) => PathBuf::from(p),
        None => PathBuf::from(MISSING),
    }
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
    refenv_samples_in(std::env::var_os("BH_REFENV"))
}

fn refenv_samples_in(var: Option<OsString>) -> String {
    match var {
        Some(p) => PathBuf::from(p).join("samples"),
        None => PathBuf::from(MISSING),
    }
    .to_string_lossy()
    .into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_follow_the_variables() {
        assert_eq!(private_docs_in(Some("/docs".into())), PathBuf::from("/docs"));
        assert_eq!(private_docs_in(None), PathBuf::from(MISSING));
        assert_eq!(refenv_samples_in(Some("/ref".into())), "/ref/samples");
        assert_eq!(refenv_samples_in(None), MISSING);
        assert_eq!(modules("a/b"), private_docs().join("a/b").to_string_lossy());
        assert_eq!(plist_golden("x.json"), private_docs().join("plist_golden/x.json").to_string_lossy());
        assert!(!refenv_samples().is_empty());
    }
}
