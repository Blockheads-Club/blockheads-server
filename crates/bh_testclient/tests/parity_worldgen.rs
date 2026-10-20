//! Fixed-seed world generation parity matrix: reference vs ours, `diff_saves` must be clean.
//!
//!  Needs docker and `BH_PARITY=1`; skipped otherwise.
//! Requires `cargo build -p blockheads_server` first (or `BH_SERVER_BIN`).

use std::path::Path;
use std::time::Duration;

use bh_testclient::harness::refenv::*;
use bh_testclient::harness::save_diff::*;
use bh_testclient::harness::*;

const ID: &str = "cccccccccccccccccccccccccccccc01";

fn rules(extra: &str) -> String {
    let mut s = String::from("{\"seed\":\"custom\",\"custom_seed_value\":\"12345\"");
    if !extra.is_empty() {
        s.push(',');
        s.push_str(extra);
    }
    s.push('}');
    s
}

fn args(width: &str, expert: bool, extra_rules: &str) -> Vec<String> {
    let mut a: Vec<String> =
        ["--new", "DH", "--world_id", ID, "--world_width", width].iter().map(|s| s.to_string()).collect();
    if expert {
        a.push("--expert-mode".into());
    }
    a.push("--custom-rules".into());
    a.push(rules(extra_rules));
    a
}

fn load_timeout(width: &str) -> Duration {
    Duration::from_secs(if width == "16" || width == "4" { 120 } else { 60 })
}

fn assert_clean(tag: &str, ref_srv: &mut dyn ServerHandle, our: &mut dyn ServerHandle, rep: &SaveDiffReport) {
    let blocks = rep.per_db.get("world_db/blocks").copied().unwrap_or((0, 0));
    let ok = rep.is_clean() && rep.compared_records > 0 && blocks.0 > 0 && blocks.0 == blocks.1;
    if !ok {
        let (rd, od) = (ref_srv.keep_data(), our.keep_data());
        panic!(
            "{}: parity failure\n{}\nblocks A/B = {:?}\nref data: {}\nour data: {}\nre-run: bh_savediff {} {}\nour log tail:\n{}",
            tag,
            rep.render(60),
            blocks,
            rd.display(),
            od.display(),
            ref_srv.save_dir(ID).display(),
            our.save_dir(ID).display(),
            our.log().lines().rev().take(20).collect::<Vec<_>>().join("\n")
        );
    }
}

/// One matrix row: both servers generate the same world, snapshot after 2 saves, SIGINT, diff.
fn case(tag: &str, width: &str, expert: bool, extra_rules: &str) {
    if !parity_enabled() {
        eprintln!("SKIP: {} (BH_PARITY=1 + docker + image blockheads-ref:1.7.1 required)", tag);
        return;
    }
    let a = args(width, expert, extra_rules);
    let mut r =
        RefServer::start(&RunSpec { tag: tag.into(), args: a.clone(), ..Default::default() }).expect("start reference");
    run_to_stable(&mut r, ID, load_timeout(width)).expect("reference run");
    let mut o = OurServer::start(&RunSpec { tag: tag.into(), args: a, ..Default::default() }).expect("start ours");
    run_to_stable(&mut o, ID, load_timeout(width)).expect("our run");
    let rep = diff_saves(&r.save_dir(ID), &o.save_dir(ID), &SaveDiffOptions::parity_default()).expect("diff");
    assert_clean(tag, &mut r, &mut o, &rep);
}

fn full_only(tag: &str, width: &str, expert: bool) {
    if !parity_full() {
        eprintln!("SKIP: {} (BH_PARITY_FULL=1)", tag);
        return;
    }
    case(tag, width, expert, "");
}

#[test]
fn w1_16th() {
    case("w1-16th", "1/16", false, "");
}
#[test]
fn w1_16th_expert() {
    case("w1-16th-x", "1/16", true, "");
}
#[test]
fn w1_4th() {
    case("w1-4th", "1/4", false, "");
}
#[test]
fn w1_4th_expert() {
    case("w1-4th-x", "1/4", true, "");
}
#[test]
fn w1() {
    case("w1", "1", false, "");
}
#[test]
fn w1_expert() {
    case("w1-x", "1", true, "");
}
#[test]
fn w4() {
    full_only("w4", "4", false);
}
#[test]
fn w4_expert() {
    full_only("w4-x", "4", true);
}
#[test]
fn w16() {
    full_only("w16", "16", false);
}
#[test]
fn w16_expert() {
    full_only("w16-x", "16", true);
}
#[test]
fn rules_ocean_hot() {
    case("ocean-hot", "1/16", false, "\"oceans\":\"ocean world\",\"temperature\":\"hot\"");
}
#[test]
fn rules_hash_seed() {
    if !parity_enabled() {
        eprintln!("SKIP: rules_hash_seed");
        return;
    }
    // seed value "hello world" -> hash seed 166199516 (P17); the 8-key set of sample `custom` is not
    // reproduced here, the seed alone exercises the string hash path.
    let a: Vec<String> = [
        "--new",
        "DH",
        "--world_id",
        ID,
        "--world_width",
        "1/16",
        "--custom-rules",
        "{\"seed\":\"custom\",\"custom_seed_value\":\"hello world\"}",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    let mut r = RefServer::start(&RunSpec { tag: "hash-seed".into(), args: a.clone(), ..Default::default() }).unwrap();
    run_to_stable(&mut r, ID, load_timeout("1/16")).unwrap();
    let mut o = OurServer::start(&RunSpec { tag: "hash-seed".into(), args: a, ..Default::default() }).unwrap();
    run_to_stable(&mut o, ID, load_timeout("1/16")).unwrap();
    assert!(o.log().contains("converted to integer:166199516"), "{}", o.log());
    assert!(o.log().contains("using seed:166199516"));
    let rep = diff_saves(&r.save_dir(ID), &o.save_dir(ID), &SaveDiffOptions::parity_default()).unwrap();
    assert_clean("hash-seed", &mut r, &mut o, &rep);
}
#[test]
fn rules_game_rules() {
    case("game-rules", "1/16", false, "\"health\":\"easy\",\"day/night cycle\":\"faster\",\"world modification\":\"admin only\",\"sunColor_0\":255,\"sunColor_1\":128,\"sunColor_2\":64");
}
#[test]
fn rules_spawn_items() {
    let items: Vec<String> = (1..=7).map(|i| format!("{{\"type\":{},\"count\":{}}}", i, i * 3)).collect();
    case("spawn-items", "1/16", false, &format!("\"spawnItemSlots\":[{}]", items.join(",")));
}
#[test]
fn rules_terrain_extremes() {
    let cases = [
        "\"terrain\":\"totally flat\"",
        "\"terrain\":\"mountainous\"",
        "\"caves\":\"no caves\"",
        "\"caves\":\"more caves\"",
        "\"lakes\":\"no lakes\"",
        "\"sky islands\":\"none\"",
        "\"ore quantity\":\"none\"",
        "\"gems\":\"none\"",
        "\"treasure\":\"none\"",
        "\"trees\":\"none\"",
        "\"plants\":\"none\"",
        "\"rainfall\":\"none\"",
    ];
    for (i, c) in cases.iter().enumerate() {
        case(&format!("terrain-{}", i), "1/16", false, c);
    }
}

#[test]
fn load_roundtrip() {
    if !parity_enabled() {
        eprintln!("SKIP: load_roundtrip");
        return;
    }
    let mut r =
        RefServer::start(&RunSpec { tag: "lr-ref".into(), args: args("1/16", false, ""), ..Default::default() })
            .unwrap();
    run_to_stable(&mut r, ID, load_timeout("1/16")).unwrap();
    let tmp = TempDir::new("lr").unwrap();
    let data = tmp.path().join("GNUstep");
    copy_dir_all(&r.data_dir(), &data).unwrap();
    let load_args: Vec<String> = ["--load", ID].iter().map(|s| s.to_string()).collect();
    let mut o = OurServer::start(&RunSpec {
        tag: "lr-ours".into(),
        args: load_args,
        data_dir: Some(data),
        ..Default::default()
    })
    .unwrap();
    run_to_stable(&mut o, ID, load_timeout("1/16")).unwrap();
    let rep = diff_saves(&r.save_dir(ID), &o.save_dir(ID), &SaveDiffOptions::parity_default()).unwrap();
    assert_clean("load-roundtrip", &mut r, &mut o, &rep);
    let _: &Path = tmp.path();
}

/// Time-seeded world (no custom seed): ours runs with `BH_VIRTUAL_CLOCK` = the seed printed by the reference (risk 4).
#[test]
fn time_seed() {
    if !parity_enabled() || !parity_full() {
        eprintln!("SKIP: time_seed (BH_PARITY=1 and BH_PARITY_FULL=1)");
        return;
    }
    let a: Vec<String> =
        ["--new", "DH", "--world_id", ID, "--world_width", "1/16"].iter().map(|s| s.to_string()).collect();
    let mut r = RefServer::start(&RunSpec { tag: "ts-ref".into(), args: a.clone(), ..Default::default() }).unwrap();
    run_to_stable(&mut r, ID, load_timeout("1/16")).unwrap();
    let seed: u64 = r
        .log()
        .lines()
        .find_map(|l| strip_nslog_prefix(l).strip_prefix("using seed:").and_then(|s| s.trim().parse().ok()))
        .expect("'using seed:N' in reference log");
    let mut o = OurServer::start(&RunSpec {
        tag: "ts-ours".into(),
        args: a,
        data_dir: None,
        env: vec![("BH_VIRTUAL_CLOCK".into(), seed.to_string())],
    })
    .unwrap();
    run_to_stable(&mut o, ID, load_timeout("1/16")).unwrap();
    let rep = diff_saves(&r.save_dir(ID), &o.save_dir(ID), &SaveDiffOptions::parity_default()).unwrap();
    assert_clean("time-seed", &mut r, &mut o, &rep);
}
