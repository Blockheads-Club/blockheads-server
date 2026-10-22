//! Zero-effect rule of the mod host: the parity solo session against our server binary with an empty mods
//! directory, and with one holding only a disabled mod, gives the same normalised trace and the same save as without
//! any mods directory. (The reference comparison itself is `parity_session.rs`, which needs `BH_PARITY=1`.)
//!
//! Needs the server binary (`cargo build -p blockheads_server`, or `BH_SERVER_BIN`); skipped otherwise.

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::time::Duration;

use bh_testclient::harness::normalize::*;
use bh_testclient::harness::refenv::*;
use bh_testclient::harness::save_diff::*;
use bh_testclient::harness::trace_diff::*;
use bh_testclient::harness::*;

const WORLD: &str = "e98c5bee38be74e00ad7748a3dd42119";
/// The parity solo flow without its `hb-old` / `wait 17` step, which times out against our server even without
/// the mod host compiled in (unrelated to mods).
const SOLO: &str = "client TESTER\ninitial-data\nwait 06\nwait 1e 6000\nuid\nblock 0\nwait 04\nsleep 700\nblock 100000 create=0\nwait 27\nquiet 1500\ndisconnect\n";

fn server_args() -> Vec<String> {
    [
        "--new",
        "PROTO",
        "--world_id",
        WORLD,
        "--world_width",
        "1/4",
        "--custom-rules",
        "{\"seed\":\"custom\",\"custom_seed_value\":\"12345\"}",
        // Keep running after the client leaves, so the two saves never race the auto-exit.
        "--no-exit",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect()
}

struct Run {
    trace: Vec<TraceEvent>,
    save: PathBuf,
    log: String,
    _h: OurServer,
}

fn run(tag: &str, mods_dir: Option<&Path>) -> Run {
    let env = mods_dir.map(|d| vec![("BLOCKHEADS_MODS_DIR".to_string(), d.display().to_string())]).unwrap_or_default();
    let spec = RunSpec { tag: tag.into(), args: server_args(), env, ..Default::default() };
    let mut h = OurServer::start(&spec).unwrap();
    h.wait_for_log("World load complete.", Duration::from_secs(90)).expect("load");
    let dir = TempDir::new("modtrace").unwrap();
    let addr: SocketAddr = format!("127.0.0.1:{}", h.port()).parse().unwrap();
    let trace = run_script(SOLO, addr, dir.path()).expect("script");
    h.wait_for_saves(WORLD, 2, Duration::from_secs(30)).unwrap();
    h.stop_sigint(Duration::from_secs(10)).unwrap();
    Run { trace, save: h.save_dir(WORLD), log: h.log(), _h: h }
}

#[test]
fn empty_or_disabled_mods_dir_is_byte_identical() {
    if our_server_bin().is_err() {
        eprintln!("SKIP: no blockheads_server binary (cargo build -p blockheads_server)");
        return;
    }
    let tmp = TempDir::new("modsdirs").unwrap();
    let empty = tmp.path().join("empty");
    std::fs::create_dir_all(&empty).unwrap();
    let disabled = tmp.path().join("disabled");
    let hello = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/mods/hello-lua");
    copy_dir_all(&hello, &disabled.join("hello-lua")).unwrap();
    std::fs::write(disabled.join("mods.toml"), "[hello-lua]\nenabled = false\n").unwrap();

    let base1 = run("nomods1", None);
    let base2 = run("nomods2", None);
    let mut opts = NormalizeOptions::default();
    let (n1, n2) = (normalize_trace(&base1.trace, &opts), normalize_trace(&base2.trace, &opts));
    opts.volatile = Some(VolatileMask::learn(&[n1, n2]));
    let reference = normalize_trace(&base1.trace, &opts);
    for (tag, dir) in [("emptymods", &empty), ("disabledmods", &disabled)] {
        let r = run(tag, Some(dir));
        let rep = diff_traces(&reference, &normalize_trace(&r.trace, &opts), DiffMode::Strict);
        assert!(rep.is_clean(), "{tag}:\n{}", rep.render());
        // Values that depend on how long the server ran (world time, growth) differ between any two runs, mods or
        // not; the structure must not: same records, same plist keys and value types, no extra files.
        let rep = diff_saves(&base1.save, &r.save, &SaveDiffOptions::parity_default()).unwrap();
        assert!(rep.compared_records > 0);
        for d in &rep.diffs {
            let structural = match d {
                RecordDiff::Block { .. } => false,
                RecordDiff::Plist { diffs, .. } => diffs.iter().any(|x| x.kind != PlistDiffKind::ValueMismatch),
                _ => true,
            };
            assert!(!structural, "{tag}: {:?}\n{}", d.id(), rep.render(80));
        }
        let names = |p: &Path| {
            let mut v: Vec<_> = std::fs::read_dir(p).unwrap().flatten().map(|e| e.file_name()).collect();
            v.sort();
            v
        };
        assert_eq!(names(&base1.save), names(&r.save), "{tag}");
        assert!(!r.log.contains("[mods"), "{tag}: {}", r.log);
        assert!(!r.save.join("mods").exists());
        let lines = |l: &str| l.lines().map(strip_nslog_prefix).map(str::to_string).collect::<Vec<_>>();
        assert_eq!(
            lines(&base1.log).len(),
            lines(&r.log).len(),
            "{tag}: log line count\n{}\n---\n{}",
            base1.log,
            r.log
        );
    }
}
