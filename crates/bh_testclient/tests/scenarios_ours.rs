//! Docker-free variants of the differential gameplay scenarios: our server runs in-process with its real
//! production wiring and every observation must match the reference values checked in under
//! `tests/fixtures/scenarios` (see the README there for how to regenerate them). Known, not yet fixed differences
//! are listed in `<scenario>.xfail` and must keep failing until they are fixed (then the entry must go).

mod gameplay;

use gameplay::{fixture_scenario, scenarios};

fn run(name: &str) {
    fixture_scenario(&scenarios::by_name(name).unwrap_or_else(|| panic!("no scenario {}", name)));
}

#[test]
fn chest_walk() {
    run("chest_walk");
}
#[test]
fn commands() {
    run("commands");
}
#[test]
fn paint_sign() {
    run("paint_sign");
}
#[test]
fn craft() {
    run("craft");
}
#[test]
fn sleep() {
    run("sleep");
}
#[test]
fn npc() {
    run("npc");
}
#[test]
fn vehicles() {
    run("vehicles");
}
#[test]
fn trade() {
    run("trade");
}
#[test]
fn portal_chest() {
    run("portal_chest");
}
#[test]
fn rules_welcome() {
    run("rules_welcome");
}
