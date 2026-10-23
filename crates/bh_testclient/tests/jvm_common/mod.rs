//! Shared by the JVM tests: builds the Java fixture mod and drives a live server.
#![allow(dead_code)]

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use bh_testclient::{ClientConfig, Identity, JoinOutcome, Session};
use blockheads_server::host::{self, HostConfig, ServerHandle, Status, WorldSelect};
use blockheads_server::mods::{ModsConfig, JVM_SDK_JAR};
use blockheads_server::plist::value::PlistValue;
use blockheads_server::protocol::codec::{plist_from, t};

fn tool(name: &str) -> PathBuf {
    for var in ["BHMOD_JAVA_HOME", "JAVA_HOME"] {
        if let Some(home) = std::env::var_os(var) {
            let p = Path::new(&home).join("bin").join(name);
            if p.is_file() {
                return p;
            }
        }
    }
    PathBuf::from(name)
}

fn run(cmd: &mut Command) {
    let out = cmd.output().expect("run the JDK tool (a JDK is needed for the JVM tests)");
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
}

/// Builds `tests/data/jvm/hello-java` into `<mods>/hello-java` (mod.toml, server/mod.jar).
pub fn build_mod(mods: &Path, work: &Path) {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/jvm/hello-java");
    let sdk = work.join("sdk.jar");
    std::fs::write(&sdk, JVM_SDK_JAR).unwrap();
    let classes = work.join("classes");
    std::fs::create_dir_all(&classes).unwrap();
    run(Command::new(tool("javac"))
        .args(["-nowarn", "-Xlint:-options", "--release", "8", "-cp"])
        .arg(&sdk)
        .arg("-d")
        .arg(&classes)
        .arg(src.join("src/club/example/hello/Hello.java")));
    let manifest = work.join("manifest.mf");
    std::fs::write(&manifest, "Manifest-Version: 1.0\nBhmod-Main: club.example.hello.Hello\n").unwrap();
    let dest = mods.join("hello-java");
    std::fs::create_dir_all(dest.join("server")).unwrap();
    std::fs::copy(src.join("mod.toml"), dest.join("mod.toml")).unwrap();
    std::fs::copy(src.join("LICENSE"), dest.join("LICENSE")).unwrap();
    run(Command::new(tool("jar"))
        .arg("cfm")
        .arg(dest.join("server/mod.jar"))
        .arg(&manifest)
        .arg("-C")
        .arg(&classes)
        .arg("."));
    std::fs::write(mods.join("mods.toml"), "[hello-java]\ngrant = [\"native\", \"players:chat\"]\n").unwrap();
}

pub struct Running {
    pub handle: ServerHandle,
    pub lines: Arc<Mutex<Vec<String>>>,
    pub port: u16,
}

impl Running {
    pub fn log(&self) -> String {
        self.lines.lock().unwrap().join("\n")
    }
    pub fn wait_log(&self, needle: &str, secs: u64) -> bool {
        let end = Instant::now() + Duration::from_secs(secs);
        while Instant::now() < end {
            if self.log().contains(needle) {
                return true;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        false
    }
    pub fn join(&self, alias: &str) -> Session {
        let cfg = ClientConfig { addr: SocketAddr::from(([127, 0, 0, 1], self.port)), ..Default::default() };
        let mut s = Session::connect(cfg, Identity::new(alias)).unwrap();
        assert_eq!(s.join(10_000).unwrap(), JoinOutcome::Approved, "{}", self.log());
        s
    }
}

pub fn start(data: &Path, world: &str, port: u16, mods: &Path) -> Running {
    let lines = Arc::new(Mutex::new(Vec::<String>::new()));
    let l2 = lines.clone();
    let cfg = HostConfig {
        data_dir: Some(data.to_path_buf()),
        world: WorldSelect::Load(world.to_string()),
        port: Some(port),
        mods: ModsConfig { dir: Some(mods.to_path_buf()), disabled: false },
        ..HostConfig::default()
    };
    let handle = host::start(cfg, Arc::new(move |s| l2.lock().unwrap().push(s.to_string()))).unwrap();
    let end = Instant::now() + Duration::from_secs(90);
    while handle.status() == Status::Starting && Instant::now() < end {
        std::thread::sleep(Duration::from_millis(50));
    }
    assert_eq!(handle.status(), Status::Running, "{:#?}", lines.lock().unwrap());
    Running { handle, lines, port }
}

fn text_of(data: &[u8]) -> Option<String> {
    if data.first() != Some(&t::CHAT_MESSAGE) {
        return None;
    }
    match plist_from(&data[1..])?.object_for_key("message") {
        Some(PlistValue::String(s)) => Some(s.s.clone()),
        _ => None,
    }
}

pub fn wait_chat(s: &mut Session, needle: &str, ms: u64) -> Vec<String> {
    let mut seen = Vec::new();
    let end = Instant::now() + Duration::from_millis(ms);
    while Instant::now() < end {
        s.run_for(50).unwrap();
        seen.extend(s.drain().iter().filter_map(|rx| text_of(&rx.data)));
        if seen.iter().any(|m| m.contains(needle)) {
            return seen;
        }
    }
    panic!("no chat line containing {needle:?}; saw {seen:?}");
}
