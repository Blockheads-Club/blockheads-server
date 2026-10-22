//! Runners for the reference server (docker) and our server, with guard-object cleanup.
//!
//! the reference environment, the reference environment.

use std::fs::File;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant, SystemTime};

use super::{free_udp_port, our_server_bin, refenv_dir, strip_nslog_prefix, HarnessError, Result, TempDir};

/// Common interface of the three runner kinds.
pub trait ServerHandle {
    fn name(&self) -> &str;
    fn port(&self) -> u16;
    /// `<data root>/Library/ApplicationSupport/TheBlockheads/saves`
    fn saves_dir(&self) -> PathBuf;
    fn save_dir(&self, id: &str) -> PathBuf {
        self.saves_dir().join(id)
    }
    /// stdout+stderr captured so far
    fn log(&self) -> String;
    fn is_running(&mut self) -> bool;
    /// Poll the log every 50 ms for a line whose NSLog-stripped text contains `needle`.
    fn wait_for_log(&mut self, needle: &str, timeout: Duration) -> Result<()>;
    /// Wait until `world_db/data.mdb` changed `n` times after this call (one change = one LMDB commit).
    fn wait_for_saves(&mut self, world_id: &str, n: u32, timeout: Duration) -> Result<()>;
    /// SIGINT, wait up to `grace`, then SIGKILL. Returns the exit code (130 when stopped by SIGINT).
    fn stop_sigint(&mut self, grace: Duration) -> Result<Option<i32>>;
    /// Data root (the reference's `DATA_DIR`, our `HOME/GNUstep`).
    fn data_dir(&self) -> PathBuf;
    /// Type a line into the server console (needs `RunSpec::console`).
    fn send_console(&mut self, _line: &str) -> Result<()> {
        Err(HarnessError::Usage(format!("{}: no console (RunSpec::console not set)", self.name())))
    }
    /// Do not delete the data dir when the handle is dropped; returns its path.
    fn keep_data(&mut self) -> PathBuf;
}

/// What to start.
#[derive(Clone, Debug, Default)]
pub struct RunSpec {
    pub tag: String,
    /// server args WITHOUT `--port` (the runner appends `--port` and `--save_delay 1` unless present)
    pub args: Vec<String>,
    pub data_dir: Option<PathBuf>,
    /// extra env for OUR server; ignored for the reference
    pub env: Vec<(String, String)>,
    /// keep the server's stdin open for [`ServerHandle::send_console`] (the reference then runs `docker run -i`
    /// directly instead of `run_ref.sh`, which attaches no stdin)
    pub console: bool,
}

/// Docker image of the reference server.
pub const REF_IMAGE: &str = "blockheads-ref:1.7.1";

static COUNTER: AtomicU64 = AtomicU64::new(1);

/// `<prefix>-<tag>-<pid>-<counter>`, at most 60 chars, `[a-z0-9-]` only.
pub fn unique_name(prefix: &str, tag: &str) -> String {
    let clean = |s: &str| -> String {
        s.chars().map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '-' }).collect()
    };
    let suffix = format!("-{}-{}", std::process::id(), COUNTER.fetch_add(1, Ordering::SeqCst));
    let mut head = format!("{}-{}", clean(prefix), clean(tag));
    let max = 60usize.saturating_sub(suffix.len());
    head.truncate(max);
    format!("{}{}", head, suffix)
}

/// `docker info` exits 0.
pub fn docker_available() -> bool {
    Command::new("docker")
        .arg("info")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}
/// `docker image inspect TAG` exits 0.
pub fn image_present(tag: &str) -> bool {
    Command::new("docker")
        .args(["image", "inspect", tag])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Recursive directory copy (used to give `--load` runs their own data dir).
pub fn copy_dir_all(from: &Path, to: &Path) -> Result<()> {
    std::fs::create_dir_all(to)?;
    for e in std::fs::read_dir(from)? {
        let e = e?;
        let t = to.join(e.file_name());
        if e.file_type()?.is_dir() {
            copy_dir_all(&e.path(), &t)?;
        } else {
            std::fs::copy(e.path(), &t)?;
        }
    }
    Ok(())
}

fn saves_of(data: &Path) -> PathBuf {
    data.join("Library/ApplicationSupport/TheBlockheads/saves")
}

fn with_defaults(args: &[String], port: u16) -> Vec<String> {
    let mut v = args.to_vec();
    v.push("--port".into());
    v.push(port.to_string());
    if !args.iter().any(|a| a == "--save_delay") {
        v.push("--save_delay".into());
        v.push("1".into());
    }
    v
}

fn tail(log: &str, n: usize) -> String {
    let l: Vec<&str> = log.lines().collect();
    l[l.len().saturating_sub(n)..].join("\n")
}

/// Shared process bookkeeping behind the three public handle types.
struct Proc {
    name: String,
    port: u16,
    data: PathBuf,
    /// owned temp dir (removed on drop unless kept)
    tmp: Option<TempDir>,
    child: Option<Child>,
    log_path: PathBuf,
    docker: bool,
    exit: Option<Option<i32>>,
    own_log: bool,
    stdin: Option<std::process::ChildStdin>,
}

fn status_code(s: std::process::ExitStatus) -> Option<i32> {
    use std::os::unix::process::ExitStatusExt;
    s.code().or_else(|| s.signal().map(|g| 128 + g))
}

impl Proc {
    fn log(&self) -> String {
        std::fs::read(&self.log_path).map(|b| String::from_utf8_lossy(&b).to_string()).unwrap_or_default()
    }
    fn is_running(&mut self) -> bool {
        match self.child.as_mut() {
            None => false,
            Some(c) => match c.try_wait() {
                Ok(None) => true,
                Ok(Some(s)) => {
                    self.exit.get_or_insert(status_code(s));
                    false
                }
                Err(_) => false,
            },
        }
    }
    fn wait_for_log(&mut self, needle: &str, timeout: Duration) -> Result<()> {
        let end = Instant::now() + timeout;
        loop {
            let log = self.log();
            if log.lines().any(|l| strip_nslog_prefix(l).contains(needle)) {
                return Ok(());
            }
            let running = self.is_running();
            if !running {
                // one last read: output may have been flushed just before exit
                let log = self.log();
                if log.lines().any(|l| strip_nslog_prefix(l).contains(needle)) {
                    return Ok(());
                }
                return Err(HarnessError::Timeout(format!(
                    "{} exited before log contained '{}' (exit {:?}): {}",
                    self.name,
                    needle,
                    self.exit,
                    tail(&log, 20)
                )));
            }
            if Instant::now() >= end {
                return Err(HarnessError::Timeout(format!(
                    "waiting for log '{}' on {}: {}",
                    needle,
                    self.name,
                    tail(&log, 20)
                )));
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }
    fn db_stamp(&self, world_id: &str) -> Option<(SystemTime, u64)> {
        let m = std::fs::metadata(saves_of(&self.data).join(world_id).join("world_db/data.mdb")).ok()?;
        Some((m.modified().ok()?, m.len()))
    }
    fn wait_for_saves(&mut self, world_id: &str, n: u32, timeout: Duration) -> Result<()> {
        let end = Instant::now() + timeout;
        let mut last = self.db_stamp(world_id);
        let mut seen = 0;
        while seen < n {
            std::thread::sleep(Duration::from_millis(50));
            let now = self.db_stamp(world_id);
            if now != last {
                if now.is_some() {
                    seen += 1;
                }
                last = now;
            }
            if seen >= n {
                break;
            }
            if !self.is_running() {
                return Err(HarnessError::Timeout(format!(
                    "{} exited while waiting for saves ({}/{}): {}",
                    self.name,
                    seen,
                    n,
                    tail(&self.log(), 20)
                )));
            }
            if Instant::now() >= end {
                return Err(HarnessError::Timeout(format!(
                    "wait_for_saves {}/{} on {}: {}",
                    seen,
                    n,
                    self.name,
                    tail(&self.log(), 20)
                )));
            }
        }
        Ok(())
    }
    fn signal(&mut self, sig: i32) {
        if self.docker {
            let s = if sig == libc::SIGINT { "INT" } else { "KILL" };
            let _ = Command::new("docker")
                .args(["kill", "-s", s, &self.name])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
        } else if let Some(c) = &self.child {
            // SAFETY: plain kill(2) on a child pid we own.
            unsafe {
                libc::kill(c.id() as i32, sig);
            }
        }
    }
    fn stop_sigint(&mut self, grace: Duration) -> Result<Option<i32>> {
        if !self.is_running() {
            return Ok(self.exit.flatten());
        }
        self.signal(libc::SIGINT);
        let end = Instant::now() + grace;
        while Instant::now() < end {
            if !self.is_running() {
                return Ok(self.exit.flatten());
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        self.signal(libc::SIGKILL);
        if let Some(c) = self.child.as_mut() {
            let _ = c.kill();
            if let Ok(s) = c.wait() {
                self.exit.get_or_insert(status_code(s));
            }
        }
        Ok(self.exit.flatten())
    }
    fn cleanup(&mut self) {
        if self.docker {
            let _ = Command::new("docker")
                .args(["rm", "-f", &self.name])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
        }
        if let Some(c) = self.child.as_mut() {
            let _ = c.kill();
            let _ = c.wait();
        }
        if self.own_log && self.tmp.is_none() {
            let _ = std::fs::remove_file(&self.log_path);
        }
    }
    fn send_console(&mut self, line: &str) -> Result<()> {
        use std::io::Write;
        let name = self.name.clone();
        let w = self.stdin.as_mut().ok_or_else(|| HarnessError::Usage(format!("{}: no console", name)))?;
        w.write_all(format!("{}\n", line).as_bytes())?;
        w.flush()?;
        Ok(())
    }
    fn keep_data(&mut self) -> PathBuf {
        if let Some(t) = self.tmp.take() {
            let p = t.keep();
            // the data dir may be a subdirectory of the kept dir
            let _ = p;
        }
        self.own_log = false;
        self.data.clone()
    }
}

macro_rules! delegate_handle {
    ($t:ty) => {
        impl ServerHandle for $t {
            fn name(&self) -> &str {
                &self.p.name
            }
            fn port(&self) -> u16 {
                self.p.port
            }
            fn saves_dir(&self) -> PathBuf {
                saves_of(&self.p.data)
            }
            fn log(&self) -> String {
                self.p.log()
            }
            fn is_running(&mut self) -> bool {
                self.p.is_running()
            }
            fn wait_for_log(&mut self, needle: &str, timeout: Duration) -> Result<()> {
                self.p.wait_for_log(needle, timeout)
            }
            fn wait_for_saves(&mut self, world_id: &str, n: u32, timeout: Duration) -> Result<()> {
                self.p.wait_for_saves(world_id, n, timeout)
            }
            fn stop_sigint(&mut self, grace: Duration) -> Result<Option<i32>> {
                self.p.stop_sigint(grace)
            }
            fn data_dir(&self) -> PathBuf {
                self.p.data.clone()
            }
            fn keep_data(&mut self) -> PathBuf {
                self.p.keep_data()
            }
            fn send_console(&mut self, line: &str) -> Result<()> {
                self.p.send_console(line)
            }
        }
        impl Drop for $t {
            fn drop(&mut self) {
                self.p.cleanup();
            }
        }
    };
}

fn prepare_data(spec: &RunSpec) -> Result<(PathBuf, Option<TempDir>)> {
    match &spec.data_dir {
        Some(d) => {
            std::fs::create_dir_all(d)?;
            Ok((d.clone(), None))
        }
        None => {
            let t = TempDir::new(&spec.tag)?;
            let d = t.path().join("data");
            std::fs::create_dir_all(&d)?;
            Ok((d, Some(t)))
        }
    }
}

fn start_docker(script: &str, prefix: &str, spec: &RunSpec, gdb: bool) -> Result<Proc> {
    let (data, tmp) = prepare_data(spec)?;
    let port = free_udp_port();
    let name = unique_name(prefix, &spec.tag);
    let log_path = match &tmp {
        Some(t) => t.path().join("server.log"),
        None => PathBuf::from(format!("{}.log", data.display())),
    };
    let log = File::create(&log_path)?;
    let mut cmd = if spec.console && !gdb {
        // what run_ref.sh does, plus -i so that the console can be typed into
        let mut c = Command::new("docker");
        c.args(["run", "--rm", "--init", "-i", "--name", &name, "--network", "host"])
            .arg("--user")
            .arg(format!("{}:{}", unsafe { libc::getuid() }, unsafe { libc::getgid() }))
            .args(["-e", "HOME=/home/bh", "-v"])
            .arg(format!("{}:/home/bh/GNUstep", data.display()))
            .arg(REF_IMAGE);
        c
    } else {
        Command::new(refenv_dir().join(script))
    };
    cmd.args(with_defaults(&spec.args, port))
        .env("DATA_DIR", &data)
        .env("NAME", &name)
        .env("PORT", port.to_string())
        .stdin(if spec.console { Stdio::piped() } else { Stdio::null() })
        .stdout(Stdio::from(log.try_clone()?))
        .stderr(Stdio::from(log));
    if gdb {
        cmd.env("GDBPORT", free_udp_port().to_string());
    }
    let mut child = cmd.spawn().map_err(|e| HarnessError::Docker(format!("spawn {}: {}", script, e)))?;
    let stdin = child.stdin.take();
    Ok(Proc { name, port, data, tmp, child: Some(child), log_path, docker: true, exit: None, own_log: true, stdin })
}

/// The reference server in a container.
pub struct RefServer {
    p: Proc,
}
impl RefServer {
    /// `DATA_DIR=<dir> NAME=bh-diff-<tag>-<pid>-<seq> PORT=<port> the reference environment <args> --port <port> --save_delay 1`.
    pub fn start(spec: &RunSpec) -> Result<RefServer> {
        Ok(RefServer { p: start_docker("run_ref.sh", "bh-diff", spec, false)? })
    }
}
delegate_handle!(RefServer);

/// The reference server under gdbserver (paused until gdb attaches).
pub struct RefDebugServer {
    p: Proc,
    gdb_port: u16,
}
impl RefDebugServer {
    /// Starts the container and waits (<= 30 s) for gdbserver's "Listening on port".
    pub fn start(spec: &RunSpec) -> Result<RefDebugServer> {
        let (data, tmp) = prepare_data(spec)?;
        let port = free_udp_port();
        let gdb_port = free_udp_port();
        let name = unique_name("bh-dbg-diff", &spec.tag);
        let log_path = match &tmp {
            Some(t) => t.path().join("server.log"),
            None => PathBuf::from(format!("{}.log", data.display())),
        };
        let log = File::create(&log_path)?;
        let child = Command::new(refenv_dir().join("run_ref_debug.sh"))
            .args(with_defaults(&spec.args, port))
            .env("DATA_DIR", &data)
            .env("NAME", &name)
            .env("GDBPORT", gdb_port.to_string())
            .stdin(Stdio::null())
            .stdout(Stdio::from(log.try_clone()?))
            .stderr(Stdio::from(log))
            .spawn()
            .map_err(|e| HarnessError::Docker(format!("spawn run_ref_debug.sh: {}", e)))?;
        let mut s = RefDebugServer {
            p: Proc {
                name,
                port,
                data,
                tmp,
                child: Some(child),
                log_path,
                docker: true,
                exit: None,
                own_log: true,
                stdin: None,
            },
            gdb_port,
        };
        s.p.wait_for_log("Listening on port", Duration::from_secs(30))?;
        Ok(s)
    }
    pub fn gdb_port(&self) -> u16 {
        self.gdb_port
    }
}
delegate_handle!(RefDebugServer);

/// Our server as a child process with an isolated `HOME`.
pub struct OurServer {
    p: Proc,
    _home: Option<TempDir>,
}
impl OurServer {
    /// `our_server_bin() <args> --port <port> --save_delay 1` with `HOME=<tmp>`; data dir = `<tmp>/GNUstep`.
    pub fn start(spec: &RunSpec) -> Result<OurServer> {
        let home = TempDir::new(&spec.tag)?;
        let gnustep = home.path().join("GNUstep");
        match &spec.data_dir {
            Some(d) => {
                std::fs::create_dir_all(d)?;
                std::os::unix::fs::symlink(std::fs::canonicalize(d)?, &gnustep)?;
            }
            None => std::fs::create_dir_all(&gnustep)?,
        }
        let port = free_udp_port();
        let name = format!("ours-{}", std::process::id());
        let log_path = home.path().join("server.log");
        let log = File::create(&log_path)?;
        let mut cmd = Command::new(our_server_bin()?);
        cmd.args(with_defaults(&spec.args, port))
            .env("HOME", home.path())
            .stdin(if spec.console { Stdio::piped() } else { Stdio::null() })
            .stdout(Stdio::from(log.try_clone()?))
            .stderr(Stdio::from(log));
        for (k, v) in &spec.env {
            cmd.env(k, v);
        }
        // SAFETY: only async-signal-safe signal(2) between fork and exec; a harness started from a shell with SIGINT ignored
        // must not pass that disposition on (the original has no handler, SIGINT must kill it with status 130).
        unsafe {
            use std::os::unix::process::CommandExt;
            cmd.pre_exec(|| {
                libc::signal(libc::SIGINT, libc::SIG_DFL);
                Ok(())
            });
        }
        let mut child = cmd.spawn()?;
        let stdin = child.stdin.take();
        let data = spec.data_dir.clone().unwrap_or_else(|| gnustep.clone());
        Ok(OurServer {
            p: Proc {
                name,
                port,
                data,
                tmp: None,
                child: Some(child),
                log_path,
                docker: false,
                exit: None,
                own_log: false,
                stdin,
            },
            _home: Some(home),
        })
    }
}
impl OurServer {
    fn p_keep(&mut self) -> PathBuf {
        if let Some(h) = self._home.take() {
            let _ = h.keep();
        }
        self.p.data.clone()
    }
}
impl ServerHandle for OurServer {
    fn name(&self) -> &str {
        &self.p.name
    }
    fn port(&self) -> u16 {
        self.p.port
    }
    fn saves_dir(&self) -> PathBuf {
        saves_of(&self.p.data)
    }
    fn log(&self) -> String {
        self.p.log()
    }
    fn is_running(&mut self) -> bool {
        self.p.is_running()
    }
    fn wait_for_log(&mut self, needle: &str, timeout: Duration) -> Result<()> {
        self.p.wait_for_log(needle, timeout)
    }
    fn wait_for_saves(&mut self, world_id: &str, n: u32, timeout: Duration) -> Result<()> {
        self.p.wait_for_saves(world_id, n, timeout)
    }
    fn stop_sigint(&mut self, grace: Duration) -> Result<Option<i32>> {
        self.p.stop_sigint(grace)
    }
    fn data_dir(&self) -> PathBuf {
        self.p.data.clone()
    }
    fn keep_data(&mut self) -> PathBuf {
        self.p_keep()
    }
    fn send_console(&mut self, line: &str) -> Result<()> {
        self.p.send_console(line)
    }
}
impl Drop for OurServer {
    fn drop(&mut self) {
        self.p.cleanup();
    }
}

/// Run to a stable state and stop: `World load complete.`, two saves, SIGINT (no final save in the original).
pub fn run_to_stable(h: &mut dyn ServerHandle, world_id: &str, load_timeout: Duration) -> Result<()> {
    h.wait_for_log("World load complete.", load_timeout)?;
    h.wait_for_saves(world_id, 2, Duration::from_secs(30))?;
    h.stop_sigint(Duration::from_secs(10))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_unique_and_clean() {
        let a = unique_name("bh-diff", "Tag_X");
        let b = unique_name("bh-diff", "Tag_X");
        assert_ne!(a, b);
        assert!(a.starts_with("bh-diff-tag-x-"));
        assert!(a.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'));
        assert!(unique_name("bh-diff", &"x".repeat(200)).len() <= 60);
    }

    #[test]
    fn args_defaults() {
        let v = with_defaults(&["--new".into(), "X".into()], 1234);
        assert_eq!(v, ["--new", "X", "--port", "1234", "--save_delay", "1"]);
        let v = with_defaults(&["--save_delay".into(), "5".into()], 1);
        assert_eq!(v.iter().filter(|x| *x == "--save_delay").count(), 1);
    }

    /// R6-like check with a fake server (no docker): HOME isolation, log capture, SIGINT exit code 130.
    #[test]
    fn our_server_with_fake_binary() {
        let tmp = TempDir::new("fake").unwrap();
        let script = tmp.path().join("fake_server.sh");
        std::fs::write(&script, "#!/bin/sh\ntrap 'exit 130' INT\necho \"2026-10-16 20:30:41.724 blockheads_server171[6] World load complete.\" >&2\nmkdir -p \"$HOME/GNUstep/Library/ApplicationSupport/TheBlockheads/saves/abc/world_db\"\nwhile true; do echo x >> \"$HOME/GNUstep/Library/ApplicationSupport/TheBlockheads/saves/abc/world_db/data.mdb\"; sleep 0.2; done\n").unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        // our_server_bin reads the env at call time
        std::env::set_var("BH_SERVER_BIN", &script);
        let mut s = OurServer::start(&RunSpec {
            tag: "fake".into(),
            args: vec!["--new".into(), "X".into()],
            ..Default::default()
        })
        .unwrap();
        s.wait_for_log("World load complete.", Duration::from_secs(5)).unwrap();
        s.wait_for_saves("abc", 2, Duration::from_secs(5)).unwrap();
        assert!(s.save_dir("abc").starts_with(s.data_dir()));
        let code = s.stop_sigint(Duration::from_secs(5)).unwrap();
        assert_eq!(code, Some(130));
        assert!(!s.is_running());
        let e = s.wait_for_log("never printed", Duration::from_millis(200));
        assert!(matches!(e, Err(HarnessError::Timeout(_))));
    }
}
