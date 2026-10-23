//! A Java mod on a live server: off unless `BLOCKHEADS_JVM=1`, then it behaves like its Luau twin.
//!
//! The mod is compiled here with `javac` against the SDK jar embedded in the server (needs a JDK).
#![cfg(feature = "jvm")]

mod jvm_common;

use blockheads_server::host::{self, Status};
use jvm_common::*;

#[test]
fn java_mod_is_opt_in_and_then_works() {
    let tmp = tempfile::tempdir().unwrap();
    let data = tmp.path().join("TheBlockheads");
    let w = host::create_world(Some(&data), "Java", 104, false).expect("create");
    let mods = tmp.path().join("mods");
    build_mod(&mods, tmp.path());

    // Off by default: the mod is refused with a readable reason and the server runs vanilla.
    std::env::remove_var("BLOCKHEADS_JVM");
    let mut srv = start(&data, &w.id, 15420, &mods);
    assert!(srv.wait_log("runtime Jvm is not available in this host", 10), "{}", srv.log());
    assert!(!srv.log().contains("JVM mods are enabled"), "{}", srv.log());
    assert_eq!(srv.handle.stop(), Status::Stopped(0));

    // On: the Java mod loads, greets, answers commands and stores its counter.
    std::env::set_var("BLOCKHEADS_JVM", "1");
    let mut srv = start(&data, &w.id, 15422, &mods);
    assert!(srv.wait_log("[mods WARN] JVM mods are enabled", 10), "{}", srv.log());
    assert!(srv.wait_log("[mods] hello-java: hello-java enabled", 20), "{}", srv.log());
    let mut alice = srv.join("ALICE");
    wait_chat(&mut alice, "Hello, ALICE! You are greeting number 1.", 5000);
    alice.chat("/hello").unwrap();
    let a = wait_chat(&mut alice, "ALICE says hello to everyone!", 5000);
    assert!(a.contains(&"Hello, ALICE!".to_string()), "{a:?}");
    // The Java chat handler cancels "lua is bad": the other player only sees the next line.
    let mut bob = srv.join("BOB");
    wait_chat(&mut bob, "Hello, BOB! You are greeting number 2.", 5000);
    alice.chat("lua is bad").unwrap();
    alice.chat("still here").unwrap();
    let b = wait_chat(&mut bob, "still here", 5000);
    assert!(!b.iter().any(|m| m.contains("lua is bad")), "{b:?}");
    bob.disconnect();
    srv.handle.send_command("/hello");
    assert!(srv.wait_log("Hello, console!", 5), "{}", srv.log());
    alice.disconnect();
    assert_eq!(srv.handle.stop(), Status::Stopped(0), "{}", srv.log());
    assert!(srv.log().contains("[mods] hello-java: hello-java disabled"), "{}", srv.log());
    std::env::remove_var("BLOCKHEADS_JVM");
}
