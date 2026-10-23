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
    run_with(data, args, stop_on, b"")
}

/// Like `run`, with `input` already waiting on stdin (the delete confirmation).
fn run_with(data: &Path, args: &[&str], stop_on: Option<&str>, input: &[u8]) -> Out {
    run_typing(data, args, stop_on, input, &[])
}

/// Like `run_with`; once `stop_on` shows, every entry of `typed` is written to stdin on its own before `/stop`.
fn run_typing(data: &Path, args: &[&str], stop_on: Option<&str>, input: &[u8], typed_lines: &[&[u8]]) -> Out {
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
    if !input.is_empty() {
        stdin.as_mut().unwrap().write_all(input).unwrap();
    }
    if stop_on.is_none() {
        stdin = None; // nothing more to type: the child sees end of input
    }
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
                for line in typed_lines {
                    stdin.as_mut().unwrap().write_all(line).unwrap();
                    std::thread::sleep(Duration::from_millis(150));
                }
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
    let fresh = d.join("not/yet/there");
    let empty = run(&fresh, &["--list"], None);
    assert!(fresh.is_dir());
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

const LOADED: Option<&str> = Some("World load complete.");

#[test]
fn load_a_world_with_every_override() {
    let d = tmp("load");
    let free = || std::net::UdpSocket::bind("0.0.0.0:0").unwrap().local_addr().unwrap().port().to_string();
    let p1 = free();
    let made = run(
        &d,
        &["-n", "first", "-i", "wid1", "-w", "1/16", "-p", &p1, "-e", "-C", "{}", "-u", "Boss", "-P", "private"],
        LOADED,
    );
    assert!(made.stderr.contains("custom rules world"), "{}", made.stderr);
    assert!(made.stderr.contains("with id:wid1"), "{}", made.stderr);
    assert!(made.stderr.contains("expertMode:1"), "{}", made.stderr);
    assert_eq!(made.code, 0, "{}", made.stderr);

    // Load again: rename on load, another port and player limit, credit, salt, privacy, save delay.
    let p2 = free();
    let loaded = run_typing(
        &d,
        &[
            "-o", "wid1", "-t", "renamed", "-p", &p2, "-m", "40", "-c", "7", "-k", "salt", "-u", "Boss", "-P",
            "public", "-s", "3", "-V",
        ],
        LOADED,
        b"",
        &[b"\n", b"\xff\xfe\n", b"hello there\n", b"/players\n", b"/clear\n"],
    );
    assert!(loaded.stderr.contains("SERVER: hello there"), "{}", loaded.stderr);
    assert!(
        loaded.stderr.contains(&format!("Loading world named \"RENAMED\" on port {p2} with id:wid1")),
        "{}",
        loaded.stderr
    );
    assert!(loaded.stderr.contains("save delay:3"), "{}", loaded.stderr);
    assert_eq!(loaded.code, 0, "{}", loaded.stderr);

    // The same name again changes nothing; the stored port is used; a vanilla world gets converted.
    let again = run(&d, &["-o", "wid1", "-t", "renamed", "-p", "0"], LOADED);
    assert!(again.stderr.contains("Loading world named \"RENAMED\" on port"), "{}", again.stderr);
    assert_eq!(again.code, 0);

    let vanilla = run(&d, &["-n", "plain", "-i", "wid2", "-w", "1/16", "-p", &free()], LOADED);
    assert_eq!(vanilla.code, 0, "{}", vanilla.stderr);
    let convert = run(&d, &["-o", "wid2", "-V", "-p", &free()], LOADED);
    assert!(convert.stderr.contains("Converting world to custom rules."), "{}", convert.stderr);
    assert_eq!(convert.code, 0, "{}", convert.stderr);

    let missing = run(&d, &["-o", "nothing"], None);
    assert!(missing.stderr.contains("No world found with id \"nothing\""), "{}", missing.stderr);
    let none = run(&d, &[], None);
    assert!(none.stderr.contains("Please supply a world id to load."), "{}", none.stderr);
    let _ = std::fs::remove_dir_all(d);
}

#[test]
fn delete_and_rename_paths() {
    let d = tmp("delete");
    let made = run(&d, &["-n", "doomed", "-i", "wid9", "-w", "1/16"], LOADED);
    assert_eq!(made.code, 0, "{}", made.stderr);
    let no_delete = run(&d, &["-d", "nothing"], None);
    assert!(no_delete.stderr.contains("No world with the id or name nothing was found."), "{}", no_delete.stderr);
    let no_world = run(&d, &["-r", "nothing", "-t", "x"], None);
    assert!(no_world.stderr.contains("No world with the id nothing was found."));
    // Declined, unreadable and empty answers keep the world; only `yes` (any case) deletes it.
    for answer in [&b"no\n"[..], b"", b"yes please this is far too long\n"] {
        let kept = run_with(&d, &["-d", "doomed"], None, answer);
        assert!(kept.stderr.contains("Exiting, leaving world in tact."), "{}", kept.stderr);
    }
    let gone = run_with(&d, &["-d", "wid9"], None, b"YES\n");
    assert!(gone.stderr.contains("world \"WID9\" Deleted"), "{}", gone.stderr);
    assert!(run(&d, &["-l"], None).stderr.contains("There are currently no worlds."));
    let _ = std::fs::remove_dir_all(d);
}

#[test]
fn load_survives_a_save_that_cannot_be_renamed() {
    let d = tmp("norename");
    // The stored id differs from the directory name, so the rename cannot be written and the reload finds nothing.
    std::fs::create_dir_all(d.join("saves/dir")).unwrap();
    std::fs::write(d.join("saves/dir/world"), b"{saveVersion=1100; saveID=other; worldName=DIR;}").unwrap();
    let out = run(&d, &["-o", "dir", "-t", "newname"], None);
    assert!(out.stderr.contains("Unable to load world data to update"), "{}", out.stderr);
    assert!(out.stderr.contains("Loading world named \"(null)\""), "{}", out.stderr);
    assert_eq!(out.code, 0, "{}", out.stderr);
    let _ = std::fs::remove_dir_all(d);
}
