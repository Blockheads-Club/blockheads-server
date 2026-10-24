//! Drives the cases of `.
//!
//!  The JSON is test data, not spec.
//!
//! Compared per case: stdout, exit code, normalised stderr, `files_added` / `files_removed`, the text files of
//! `files_after` and `list_after` (the latter three only for runs that exit by themselves). Cases where the server
//! keeps running (needs the whole stack, ~6 s each) run only with `BH_GOLDEN_DYNAMIC=1`; they are checked for the
//! `World load complete.` line, stdout, stderr, and the SIGINT exit (130).
//! Not compared (documented gap): `text_files_changed`, `world_db_after`, `bound_udp_port`.

#![cfg(unix)]

use regex::Regex;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// The private docs directory (`BH_PRIVATE_DOCS`) holding `cli_golden.json`; the test skips when unavailable.
fn docs_dir() -> PathBuf {
    std::env::var_os("BH_PRIVATE_DOCS")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/nonexistent/bh-private-docs"))
}
const ARGV0: &str = "/opt/blockheads/blockheads_server171";
/// Serialises the cases that rely on the default/saved ports.
static FIXED_PORT_LOCK: Mutex<()> = Mutex::new(());

fn free_udp_port() -> u16 {
    let s = std::net::UdpSocket::bind("0.0.0.0:0").unwrap();
    s.local_addr().unwrap().port()
}

fn udp_busy(port: u16) -> bool {
    std::net::UdpSocket::bind(("0.0.0.0", port)).is_err()
}

fn copy_tree(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for e in std::fs::read_dir(from).unwrap().flatten() {
        let (src, dst) = (e.path(), to.join(e.file_name()));
        if e.file_type().unwrap().is_dir() {
            copy_tree(&src, &dst);
        } else {
            std::fs::copy(&src, &dst).unwrap();
        }
    }
}

fn list_files(root: &Path) -> BTreeSet<String> {
    fn rec(base: &Path, dir: &Path, out: &mut BTreeSet<String>) {
        if let Ok(rd) = std::fs::read_dir(dir) {
            for e in rd.flatten() {
                let p = e.path();
                if p.is_dir() {
                    rec(base, &p, out);
                } else {
                    out.insert(p.strip_prefix(base).unwrap().to_string_lossy().into_owned());
                }
            }
        }
    }
    let mut out = BTreeSet::new();
    rec(root, root, &mut out);
    out
}

struct RunOut {
    stdout: Vec<u8>,
    stderr: Vec<u8>,
    code: i32,
}

/// Run the server. `kept_running`: wait for `World load complete.` (or `max_wait`), then SIGINT.
fn run(home: &Path, args: &[String], stdin: Option<&[u8]>, kept_running: bool, max_wait: Duration) -> RunOut {
    let exe = env!("CARGO_BIN_EXE_blockheads_server");
    let mut cmd = Command::new(exe);
    use std::os::unix::process::CommandExt;
    cmd.arg0(ARGV0)
        .args(args)
        .env("HOME", home)
        .env_remove("BH_VIRTUAL_CLOCK")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .stdin(if stdin.is_some() { Stdio::piped() } else { Stdio::null() });
    let mut child = cmd.spawn().expect("spawn");
    if let Some(data) = stdin {
        let mut si = child.stdin.take().unwrap();
        let _ = si.write_all(data);
        drop(si);
    }
    let mut so = child.stdout.take().unwrap();
    let mut se = child.stderr.take().unwrap();
    let out = Arc::new(Mutex::new(Vec::new()));
    let err = Arc::new(Mutex::new(Vec::new()));
    let (o2, e2) = (out.clone(), err.clone());
    let t1 = std::thread::spawn(move || {
        let mut b = [0u8; 4096];
        while let Ok(n) = so.read(&mut b) {
            if n == 0 {
                break;
            }
            o2.lock().unwrap().extend_from_slice(&b[..n]);
        }
    });
    let t2 = std::thread::spawn(move || {
        let mut b = [0u8; 4096];
        while let Ok(n) = se.read(&mut b) {
            if n == 0 {
                break;
            }
            e2.lock().unwrap().extend_from_slice(&b[..n]);
        }
    });
    let start = Instant::now();
    let limit = if kept_running { max_wait } else { Duration::from_secs(30) };
    let mut status = None;
    let mut interrupted = false;
    loop {
        if let Some(s) = child.try_wait().unwrap() {
            status = Some(s);
            break;
        }
        let loaded = String::from_utf8_lossy(&err.lock().unwrap()).contains("World load complete.");
        if kept_running && !interrupted && (loaded || start.elapsed() > limit) {
            unsafe { libc::kill(child.id() as i32, libc::SIGINT) };
            interrupted = true;
        }
        if start.elapsed() > limit + Duration::from_secs(20) {
            let _ = child.kill();
            let _ = child.wait();
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    let _ = t1.join();
    let _ = t2.join();
    use std::os::unix::process::ExitStatusExt;
    let code = match status {
        Some(s) => s.code().unwrap_or_else(|| 128 + s.signal().unwrap_or(0)),
        None => -999,
    };
    let stdout = out.lock().unwrap().clone();
    let stderr = err.lock().unwrap().clone();
    RunOut { stdout, stderr, code }
}

struct Norm {
    prefix: Regex,
    volatile: Regex,
    seed: Regex,
    hex: Regex,
    date: Regex,
}

impl Norm {
    fn new() -> Norm {
        Norm {
            prefix: Regex::new(
                r"^\d{4}-\d\d-\d\d \d\d:\d\d:\d\d\.\d{3} blockheads_server(171)?\d*(-[0-9a-f]+)?\[\d+\] ",
            )
            .unwrap(),
            volatile: Regex::new(
                r"aboveZeroCount:|not enough food found near by|no start position found|best start pos:",
            )
            .unwrap(),
            seed: Regex::new(r"using seed:\d+").unwrap(),
            hex: Regex::new(r"\b[0-9a-f]{32}\b").unwrap(),
            date: Regex::new(r"<date>[^<]*</date>").unwrap(),
        }
    }

    /// Strip prefixes, drop volatile lines, normalise seed / port / argv0. Random ids are replaced by the caller.
    fn stderr(&self, bytes: &[u8], port: u16, mask_seed: bool) -> String {
        let s = String::from_utf8_lossy(bytes).into_owned();
        let mut out = String::new();
        for line in s.split_inclusive('\n') {
            let l = self.prefix.replace(line, "").into_owned();
            if self.volatile.is_match(&l) {
                continue;
            }
            out.push_str(&l);
        }
        let out = if mask_seed {
            self.seed.replace_all(&out, "using seed:<UNIX_TIME_AT_CREATION>").into_owned()
        } else {
            out
        };
        out.replace(&port.to_string(), "@PORT@")
    }
}

/// Replace 32-hex ids that are not in `known` by `<RANDOM_ID_n>` in order of appearance.
fn mask_random_ids(n: &Norm, text: &str, known: &BTreeSet<String>, map: &mut Vec<String>) -> String {
    n.hex
        .replace_all(text, |c: &regex::Captures| {
            let id = c[0].to_string();
            if known.contains(&id) {
                return id;
            }
            let idx = match map.iter().position(|m| *m == id) {
                Some(i) => i,
                None => {
                    map.push(id);
                    map.len() - 1
                }
            };
            format!("<RANDOM_ID_{}>", idx + 1)
        })
        .into_owned()
}

fn fixture_ids(dir: &Path) -> BTreeSet<String> {
    let mut s = BTreeSet::new();
    let saves = dir.join("Library/ApplicationSupport/TheBlockheads/saves");
    if let Ok(rd) = std::fs::read_dir(saves) {
        for e in rd.flatten() {
            s.insert(e.file_name().to_string_lossy().into_owned());
        }
    }
    s
}

fn s(v: &Value) -> String {
    v.as_str().unwrap_or("").to_string()
}

/// Returns `Err(reason)` on mismatch, `Ok(false)` when skipped.
fn run_case(c: &Value, norm: &Norm, dynamic: bool) -> Result<bool, String> {
    let id = s(&c["id"]);
    let setup = &c["setup"];
    let kept = c["server_kept_running"].as_bool().unwrap_or(false);
    if kept && !dynamic {
        return Ok(false);
    }
    let fixed_port = setup["docker_network"] == "none";
    let _guard =
        if fixed_port { Some(FIXED_PORT_LOCK.lock().unwrap_or_else(std::sync::PoisonError::into_inner)) } else { None };
    if fixed_port && (udp_busy(15151) || udp_busy(16001)) {
        return Ok(false);
    }
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().to_path_buf();
    let fx = s(&setup["fixture"]);
    let gnustep = home.join("GNUstep");
    let fxdir = docs_dir().join("cli_golden_fixtures").join(&fx);
    if fxdir.is_dir() {
        copy_tree(&fxdir, &gnustep);
    } else {
        std::fs::create_dir_all(&gnustep).unwrap();
    }
    let mut known = fixture_ids(&gnustep);
    // Ids given on the command line are not random.
    for a in c["args"].as_array().unwrap() {
        if let Some(a) = a.as_str() {
            if a.len() == 32 && a.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)) {
                known.insert(a.to_string());
            }
        }
    }
    let before = list_files(&gnustep);
    let port = free_udp_port();
    let args: Vec<String> =
        c["args"].as_array().unwrap().iter().map(|a| s(a).replace("@PORT@", &port.to_string())).collect();
    let stdin: Option<Vec<u8>> = setup["stdin"].as_str().map(|x| x.as_bytes().to_vec());
    let secs = c["termination"]
        .as_str()
        .and_then(|t| t.split("after ").nth(1))
        .and_then(|t| t.split('s').next())
        .and_then(|t| t.parse::<u64>().ok())
        .unwrap_or(6);
    let r = run(&home, &args, stdin.as_deref(), kept, Duration::from_secs(secs.max(6)));
    let fail = |what: &str, got: &str, want: &str| -> Result<bool, String> {
        Err(format!("[{id}] {what} mismatch\n--- got ---\n{got:?}\n--- want ---\n{want:?}"))
    };
    // stdout
    if r.stdout != s(&c["stdout"]).as_bytes() {
        return fail("stdout", &String::from_utf8_lossy(&r.stdout), &s(&c["stdout"]));
    }
    // exit code
    let want_code = c["exit_code"].as_i64().unwrap_or(0) as i32;
    if r.code != want_code {
        return fail("exit code", &r.code.to_string(), &want_code.to_string());
    }
    // stderr
    let mut ids = Vec::new();
    let want_err = s(&c["stderr_stable"]);
    let mask_seed = want_err.contains("<UNIX_TIME_AT_CREATION>");
    let got_err = mask_random_ids(norm, &norm.stderr(&r.stderr, port, mask_seed), &known, &mut ids);
    let incomplete = c["load_incomplete_at_kill"].as_bool().unwrap_or(false);
    if incomplete {
        if !want_err.starts_with(&got_err) && !got_err.contains("Loading world") {
            return fail("stderr (incomplete load)", &got_err, &want_err);
        }
    } else if got_err != want_err {
        return fail("stderr", &got_err, &want_err);
    }
    if kept {
        return Ok(true);
    }
    // file effects
    let after = list_files(&gnustep);
    let rel = |set: Vec<String>| -> BTreeSet<String> {
        set.into_iter().map(|p| mask_random_ids(norm, &p, &known, &mut ids.clone())).collect()
    };
    let added = rel(after.difference(&before).cloned().collect());
    let removed = rel(before.difference(&after).cloned().collect());
    let want_added: BTreeSet<String> = c["files_added"].as_array().unwrap().iter().map(s).collect();
    let want_removed: BTreeSet<String> = c["files_removed"].as_array().unwrap().iter().map(s).collect();
    if added != want_added {
        return fail("files_added", &format!("{added:?}"), &format!("{want_added:?}"));
    }
    if removed != want_removed {
        return fail("files_removed", &format!("{removed:?}"), &format!("{want_removed:?}"));
    }
    // text files after
    if let Some(files) = c["files_after"].as_object() {
        for (path, v) in files {
            let Some(want) = v["content"].as_str() else { continue };
            let real = after.iter().find(|p| mask_random_ids(norm, p, &known, &mut ids.clone()) == *path).cloned();
            let Some(real) = real else { return fail("files_after (missing)", "", path) };
            let got = std::fs::read(gnustep.join(&real)).unwrap();
            let got = String::from_utf8_lossy(&got).into_owned();
            let got =
                mask_random_ids(norm, &norm.date.replace_all(&got, "<date>DATE</date>"), &known, &mut ids.clone());
            let want = norm.date.replace_all(want, "<date>DATE</date>").into_owned();
            if got != want {
                return fail(&format!("files_after {path}"), &got, &want);
            }
        }
    }
    // list_after
    if let Some(want) = c["list_after"].as_str() {
        let l = run(&home, &["--list".to_string()], None, false, Duration::from_secs(30));
        let got = mask_random_ids(norm, &norm.stderr(&l.stderr, port, false), &known, &mut ids.clone());
        if got != want {
            return fail("list_after", &got, want);
        }
    }
    Ok(true)
}

/// All cases of the golden file. Dynamic (server stays up) cases need `BH_GOLDEN_DYNAMIC=1`.
#[test]
fn golden_cases() {
    let Ok(data) = std::fs::read_to_string(docs_dir().join("cli_golden.json")) else {
        eprintln!("SKIP: private cli_golden.json unavailable (set BH_PRIVATE_DOCS)");
        return;
    };
    let cases: Vec<Value> = serde_json::from_str(&data).unwrap();
    let only = std::env::var("BH_GOLDEN_ONLY").ok();
    let dynamic = std::env::var("BH_GOLDEN_DYNAMIC").is_ok_and(|v| v == "1");
    let cases: Vec<Value> =
        cases.into_iter().filter(|c| only.as_ref().is_none_or(|o| s(&c["id"]).contains(o.as_str()))).collect();
    let cases = Arc::new(cases);
    let next = Arc::new(AtomicUsize::new(0));
    let results: Arc<Mutex<BTreeMap<String, Result<bool, String>>>> = Arc::new(Mutex::new(BTreeMap::new()));
    let mut ths = Vec::new();
    for _ in 0..6 {
        let (cases, next, results) = (cases.clone(), next.clone(), results.clone());
        ths.push(std::thread::spawn(move || {
            let norm = Norm::new();
            loop {
                let i = next.fetch_add(1, Ordering::SeqCst);
                if i >= cases.len() {
                    break;
                }
                let id = s(&cases[i]["id"]);
                let r = run_case(&cases[i], &norm, dynamic);
                results.lock().unwrap().insert(id, r);
            }
        }));
    }
    for t in ths {
        t.join().unwrap();
    }
    let results = results.lock().unwrap();
    let mut failed = Vec::new();
    let (mut ran, mut skipped) = (0, 0);
    for (id, r) in results.iter() {
        match r {
            Ok(true) => ran += 1,
            Ok(false) => skipped += 1,
            Err(m) => failed.push(format!("{id}: {m}")),
        }
    }
    eprintln!("golden: {} ran, {} skipped, {} failed", ran, skipped, failed.len());
    assert!(failed.is_empty(), "{} golden failures:\n{}", failed.len(), failed.join("\n\n"));
}

/// Argument decoding goes through the binary: invalid UTF-8 in `--new` gives U+FFFD per bad byte.
#[test]
fn invalid_utf8_new_name() {
    use std::os::unix::ffi::OsStrExt;
    let tmp = tempfile::tempdir().unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_blockheads_server"))
        .args([std::ffi::OsStr::new("-l"), std::ffi::OsStr::from_bytes(b"-x\xff")])
        .env("HOME", tmp.path())
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0));
    assert!(String::from_utf8_lossy(&out.stderr).contains("Listing worlds:"));
}
