//! Differential gameplay scenarios: each scenario of `bh_testclient::gameplay::scenarios` runs against the
//! reference server (refenv docker image, several runs to learn which observations are noise) and against ours,
//! all from the same seeded `--new` world, and every observation that is stable across the reference runs must
//! match. Saves are diffed too (report only).
//!
//! Needs `BH_PARITY=1`, docker and the image `blockheads-ref:1.7.1` (built from `BH_REFENV`); skipped otherwise.
//! * `BH_DIFF_OUT=<dir>` keeps every artifact (traces, observations, logs, saves, `report.txt`) per scenario.
//! * `BH_DIFF_REF_RUNS=<n>` reference runs for the noise mask (default 2).
//! * `BH_DIFF_OURS=host` runs ours in-process instead of the `blockheads_server` binary (`BH_SERVER_BIN`).
//! * `BH_DIFF_WALK=<tiles>` walk length of `chest_walk` (fixtures are captured with the default 160).
//! * `BH_SCENARIO_BLESS=1` writes the learned masks to `tests/fixtures/scenarios/<name>.txt` (used by the
//!   docker-free `scenarios_ours` tests).

use bh_testclient::gameplay::{diff_scenario, scenarios};

fn run(name: &str) {
    diff_scenario(&scenarios::by_name(name).unwrap_or_else(|| panic!("no scenario {}", name)));
}

#[test]
fn diff_gameplay() {
    run("chest_walk");
}
#[test]
fn diff_commands() {
    run("commands");
}
#[test]
fn diff_paint_sign() {
    run("paint_sign");
}
#[test]
fn diff_craft() {
    run("craft");
}
#[test]
fn diff_sleep() {
    run("sleep");
}
#[test]
fn diff_npc() {
    run("npc");
}
#[test]
fn diff_vehicles() {
    run("vehicles");
}
#[test]
fn diff_trade() {
    run("trade");
}
#[test]
fn diff_portal_chest() {
    run("portal_chest");
}
#[test]
fn diff_rules_welcome() {
    run("rules_welcome");
}
