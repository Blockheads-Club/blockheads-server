//! Runs the real server binary through its command line: the exits that need no world and one full create / list /
//! rename / delete cycle against a private data directory. Needs no fixtures.

#![cfg(unix)]

use std::io::{Read, Write};
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

struct Out {
    stderr: String,
    code: i32,
}

/// Run the binary with `args` against `data`; with `stop_on` the console command `/stop` is typed once that text shows on stderr.
fn run(data: &Path, args: &[&str], stop_on: Option<&str>) -> Out {
    let mut child = Command::new(env!("CARGO_BIN_EXE_blockheads_server"))
        .args(args)
        .env("BLOCKHEADS_DATA_DIR", data)
        .env_remove("BH_VIRTUAL_CLOCK")
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn the server binary");
    let mut stdin = child.stdin.take();
    let mut se = child.stderr.take().unwrap();
    let buf = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let b2 = buf.clone();
    let reader = std::thread::spawn(move || {
        let mut chunk = [0u8; 4096];
        while let Ok(n) = se.read(&mut chunk) {
            if n == 0 {
                break;
            }
            b2.lock().unwrap().extend_from_slice(&chunk[..n]);
        }
    });
    let start = Instant::now();
    let mut typed = false;
    let status = loop {
        if let Some(s) = child.try_wait().unwrap() {
            break Some(s);
        }
        let seen = String::from_utf8_lossy(&buf.lock().unwrap()).into_owned();
        if let Some(t) = stop_on {
            if !typed && seen.contains(t) {
                stdin.as_mut().unwrap().write_all(b"/stop\n").unwrap();
                typed = true;
            }
        }
        if start.elapsed() > Duration::from_secs(120) {
            let _ = child.kill();
            let _ = child.wait();
            break None;
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    reader.join().unwrap();
    use std::os::unix::process::ExitStatusExt;
    let code = status.map_or(-999, |s| s.code().unwrap_or_else(|| 128 + s.signal().unwrap_or(0)));
    let stderr = String::from_utf8_lossy(&buf.lock().unwrap()).into_owned();
    Out { stderr, code }
}

fn tmp(tag: &str) -> std::path::PathBuf {
    let base = std::env::var_os("TMPDIR").map(std::path::PathBuf::from).unwrap_or_else(std::env::temp_dir);
    let d = base.join(format!("bh_cli_binary_{tag}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

#[test]
fn usage_exits_cleanly_with_a_message() {
    let d = tmp("usage");
    let help = run(&d, &["--help"], None);
    assert_eq!(help.code, 0);
    assert!(help.stderr.contains("Usage: blockheads_server [options]"), "{}", help.stderr);
    let missing = run(&d, &["--new"], None);
    assert_eq!(missing.code, 0);
    assert!(missing.stderr.contains("--new must be followed by the name of the world"), "{}", missing.stderr);
    let width = run(&d, &["--world_width", "4"], None);
    assert_eq!(width.code, 0);
    assert!(width.stderr.contains("--world_width must be used with --new."));
    let empty = run(&d, &["--list"], None);
    assert_eq!(empty.code, 0);
    assert!(empty.stderr.contains("There are currently no worlds."), "{}", empty.stderr);
    let _ = std::fs::remove_dir_all(d);
}

#[test]
fn create_list_rename_and_delete_a_world() {
    let d = tmp("cycle");
    let port = std::net::UdpSocket::bind("0.0.0.0:0").unwrap().local_addr().unwrap().port().to_string();
    let created = run(
        &d,
        &["--new", "cli cycle", "--world_width", "1/16", "--port", &port, "--owner_name", "Boss"],
        Some("World load complete."),
    );
    assert!(created.stderr.contains("Creating new world named \"CLI CYCLE\""), "{}", created.stderr);
    assert!(created.stderr.contains("World load complete."), "{}", created.stderr);
    assert_eq!(created.code, 0, "{}", created.stderr);
    let listed = run(&d, &["--list"], None);
    assert!(listed.stderr.contains("\"CLI CYCLE\" stored in directory named"), "{}", listed.stderr);
    let renamed = run(&d, &["--rename-from", "CLI CYCLE", "--rename-to", "second name"], None);
    assert_eq!(renamed.code, 0, "{}", renamed.stderr);
    assert!(run(&d, &["--list"], None).stderr.contains("\"SECOND NAME\""));
    let deleted = run(&d, &["--delete", "SECOND NAME", "--force"], None);
    assert_eq!(deleted.code, 0, "{}", deleted.stderr);
    assert!(run(&d, &["--list"], None).stderr.contains("There are currently no worlds."));
    let _ = std::fs::remove_dir_all(d);
}
