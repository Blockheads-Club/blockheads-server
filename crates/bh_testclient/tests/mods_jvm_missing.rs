//! JVM mods turned on without a usable JRE: reported, not fatal. Its own test binary because a failed JVM start is
//! cached for the process.
#![cfg(feature = "jvm")]

mod jvm_common;

use jvm_common::*;

#[test]
fn a_missing_jre_is_reported_not_fatal() {
    let tmp = tempfile::tempdir().unwrap();
    let data = tmp.path().join("TheBlockheads");
    let w = blockheads_server::host::create_world(Some(&data), "NoJre", 104, false).expect("create");
    let mods = tmp.path().join("mods");
    build_mod(&mods, tmp.path());
    std::env::set_var("BLOCKHEADS_JVM", "1");
    // Nothing the search looks at leads to a JRE.
    std::env::set_var("BLOCKHEADS_JAVA_HOME", tmp.path().join("no-jre"));
    std::env::set_var("JAVA_HOME", tmp.path().join("no-jre"));
    std::env::set_var("PATH", tmp.path());
    let mut srv = start(&data, &w.id, 15421, &mods);
    assert!(
        srv.wait_log("JVM mods are on but the JVM could not be started: no Java runtime found", 10),
        "{}",
        srv.log()
    );
    assert!(srv.wait_log("runtime Jvm is not available in this host", 10), "{}", srv.log());
    assert_eq!(srv.handle.stop(), blockheads_server::host::Status::Stopped(0));
}
