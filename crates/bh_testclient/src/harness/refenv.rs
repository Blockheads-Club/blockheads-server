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
    fn send_console(&mut self, line: &str) -> Result<()>;
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
                other => {
                    other.ok().flatten().into_iter().for_each(|s| {
                        self.exit.get_or_insert(status_code(s));
                    });
                    false
                }
            },
        }
    }
    fn wait_for_log(&mut self, needle: &str, timeout: Duration) -> Result<()> {
        let end = Instant::now() + timeout;
        loop {
            // the exit is checked before the log is read, so output flushed just before the exit is not missed
            let running = self.is_running();
            let log = self.log();
            if log.lines().any(|l| strip_nslog_prefix(l).contains(needle)) {
                return Ok(());
            }
            if !running {
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
        } else {
            // SAFETY: plain kill(2) on a child pid we own.
            self.child.iter().for_each(|c| unsafe {
                libc::kill(c.id() as i32, sig);
            });
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
        let waited = self.child.as_mut().and_then(|c| {
            let _ = c.kill();
            c.wait().ok()
        });
        waited.into_iter().for_each(|s| {
            self.exit.get_or_insert(status_code(s));
        });
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

fn start_docker(script: &str, prefix: &str, spec: &RunSpec) -> Result<Proc> {
    let (data, tmp) = prepare_data(spec)?;
    let port = free_udp_port();
    let name = unique_name(prefix, &spec.tag);
    let log_path = match &tmp {
        Some(t) => t.path().join("server.log"),
        None => PathBuf::from(format!("{}.log", data.display())),
    };
    let log = File::create(&log_path)?;
    let mut cmd = if spec.console {
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
        Ok(RefServer { p: start_docker("run_ref.sh", "bh-diff", spec)? })
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

/// Runs in the forked child before `exec` (async-signal-safe: one `signal(2)` call).
fn reset_sigint() -> std::io::Result<()> {
    // SAFETY: restoring the default disposition of SIGINT.
    unsafe {
        libc::signal(libc::SIGINT, libc::SIG_DFL);
    }
    Ok(())
}

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
            cmd.pre_exec(reset_sigint);
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
    fn directory_copies_and_helpers() {
        reset_sigint().unwrap();
        let mut no_child = Proc { child: None, ..fake_proc_parts() };
        no_child.signal(libc::SIGINT);
        assert!(!no_child.is_running());
        let tmp = TempDir::new("copy").unwrap();
        let from = tmp.path().join("from");
        std::fs::create_dir_all(from.join("sub/deeper")).unwrap();
        std::fs::write(from.join("a.txt"), "a").unwrap();
        std::fs::write(from.join("sub/deeper/b.txt"), "b").unwrap();
        let to = tmp.path().join("to");
        copy_dir_all(&from, &to).unwrap();
        assert_eq!(std::fs::read_to_string(to.join("sub/deeper/b.txt")).unwrap(), "b");
        assert!(copy_dir_all(&tmp.path().join("missing"), &to).is_err());
        assert_eq!(tail("a\nb\nc", 2), "b\nc");
        assert_eq!(tail("a", 5), "a");
        let ok = std::process::Command::new("sh").args(["-c", "kill -9 $$"]).status().unwrap();
        assert_eq!(status_code(ok), Some(137));
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
        let _l = env_lock();
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

    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    /// The environment is process-wide: tests that change it run one at a time.
    fn env_lock() -> std::sync::MutexGuard<'static, ()> {
        ENV_LOCK.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn script(dir: &Path, name: &str, body: &str) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let p = dir.join(name);
        std::fs::write(&p, format!("#!/bin/sh\n{}", body)).unwrap();
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
        p
    }

    /// Prints the load line, then echoes every console line it reads.
    const CONSOLE: &str = "echo \"2026-10-16 20:30:41.724 x[6] World load complete.\" >&2\n\
                           while read l; do echo \"console: $l\" >&2; done\nsleep 30\n";

    /// Restores an environment variable when dropped (the test binaries run single-threaded).
    struct EnvGuard(&'static str, Option<std::ffi::OsString>);
    impl EnvGuard {
        fn set(k: &'static str, v: impl AsRef<std::ffi::OsStr>) -> EnvGuard {
            let g = EnvGuard(k, std::env::var_os(k));
            std::env::set_var(k, v);
            g
        }
    }
    impl Drop for EnvGuard {
        fn drop(&mut self) {
            match &self.1 {
                Some(v) => std::env::set_var(self.0, v),
                None => std::env::remove_var(self.0),
            }
        }
    }

    /// Console input reaches our server's stdin; without `RunSpec::console` there is none.
    #[test]
    fn our_server_console_with_fake_binary() {
        let _l = env_lock();
        let tmp = TempDir::new("fake-console").unwrap();
        let bin = script(tmp.path(), "fake_server.sh", CONSOLE);
        let _g = EnvGuard::set("BH_SERVER_BIN", &bin);
        let spec = RunSpec { tag: "fake-console".into(), console: true, ..Default::default() };
        let mut s = OurServer::start(&spec).unwrap();
        s.wait_for_log("World load complete.", Duration::from_secs(5)).unwrap();
        s.send_console("/list").unwrap();
        s.wait_for_log("console: /list", Duration::from_secs(5)).unwrap();
        let mut s = OurServer::start(&RunSpec { console: false, ..spec }).unwrap();
        assert!(matches!(s.send_console("/list"), Err(HarnessError::Usage(_))));
        // the RNG trace runner stops when the server exits without writing a trace
        let _g2 = EnvGuard::set("BH_SERVER_BIN", script(tmp.path(), "exits.sh", "exit 0\n"));
        let out = tmp.path().join("rng.txt");
        let _ = crate::harness::gdb_rng::run_our_trace(&[], Some(1), 10, &out);
    }

    /// The reference launchers with a fake reference environment and a fake `docker` on `PATH`: `run_ref.sh`,
    /// `docker run -i` for a console, and `run_ref_debug.sh`.
    #[test]
    fn reference_launchers_with_fakes() {
        let _l = env_lock();
        let tmp = TempDir::new("fake-ref").unwrap();
        let bin = tmp.path().join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        script(&bin, "docker", &format!("if [ \"$1\" = run ]; then\n{}fi\nexit 0\n", CONSOLE));
        script(tmp.path(), "run_ref.sh", "echo \"x[1] World load complete.\" >&2\nsleep 30\n");
        script(tmp.path(), "run_ref_debug.sh", "echo \"Listening on port $GDBPORT\" >&2\nsleep 30\n");
        let path = format!("{}:{}", bin.display(), std::env::var("PATH").unwrap_or_default());
        let _p = EnvGuard::set("PATH", path);
        let _r = EnvGuard::set("BH_REFENV", tmp.path());
        let spec = RunSpec { tag: "fake-ref".into(), ..Default::default() };
        let mut r = RefServer::start(&spec).unwrap();
        r.wait_for_log("World load complete.", Duration::from_secs(5)).unwrap();
        assert!(matches!(r.send_console("/list"), Err(HarnessError::Usage(_))));
        let mut c = RefServer::start(&RunSpec { console: true, ..spec.clone() }).unwrap();
        c.wait_for_log("World load complete.", Duration::from_secs(5)).unwrap();
        c.send_console("/list").unwrap();
        c.wait_for_log("console: /list", Duration::from_secs(5)).unwrap();
        let d = RefDebugServer::start(&spec).unwrap();
        assert!(d.gdb_port() > 0);
    }
    fn fake_proc_parts() -> Proc {
        Proc {
            name: "parts".into(),
            port: 1,
            data: PathBuf::from("/nonexistent"),
            tmp: None,
            child: None,
            log_path: PathBuf::from("/nonexistent/log"),
            docker: false,
            exit: None,
            own_log: false,
            stdin: None,
        }
    }

    fn fake_proc(dir: &Path, body: &str, docker: bool) -> Proc {
        let bin = script(dir, "child.sh", body);
        let log_path = dir.join("p.log");
        let log = File::create(&log_path).unwrap();
        let child = Command::new(bin)
            .stdin(Stdio::null())
            .stdout(Stdio::from(log.try_clone().unwrap()))
            .stderr(Stdio::from(log))
            .spawn()
            .unwrap();
        Proc {
            name: "fake-proc".into(),
            port: 1,
            data: dir.join("data"),
            tmp: None,
            child: Some(child),
            log_path,
            docker,
            exit: None,
            own_log: false,
            stdin: None,
        }
    }

    #[test]
    fn docker_probes_follow_the_exit_status() {
        let _l = env_lock();
        let tmp = TempDir::new("fake-docker").unwrap();
        let ok = tmp.path().join("ok");
        let bad = tmp.path().join("bad");
        std::fs::create_dir_all(&ok).unwrap();
        std::fs::create_dir_all(&bad).unwrap();
        script(&ok, "docker", "exit 0\n");
        script(&bad, "docker", "exit 1\n");
        let with_path =
            |d: &Path| EnvGuard::set("PATH", format!("{}:{}", d.display(), std::env::var("PATH").unwrap_or_default()));
        {
            let _p = with_path(&ok);
            assert!(docker_available() && image_present("x"));
        }
        {
            let _p = with_path(&bad);
            assert!(!docker_available() && !image_present("x"));
        }
        let _p = EnvGuard::set("PATH", tmp.path().join("nothing-here"));
        assert!(!docker_available() && !image_present("x"));
    }

    #[test]
    fn process_state_and_waiting() {
        let tmp = TempDir::new("proc-state").unwrap();
        // no child at all
        let mut none = fake_proc(tmp.path(), "exit 0\n", false);
        let mut c = none.child.take().unwrap();
        c.wait().unwrap();
        assert!(!none.is_running());
        // exits right after printing: the last read finds the line, a missing one reports the exit
        let mut quick = fake_proc(tmp.path(), "echo 'x[1] late line'\nexit 3\n", false);
        quick.wait_for_log("late line", Duration::from_secs(5)).unwrap();
        let err = quick.wait_for_log("never", Duration::from_secs(5)).unwrap_err().to_string();
        assert!(err.contains("exited before log contained 'never'") && err.contains("Some(3)"), "{err}");
        assert!(quick
            .wait_for_saves("w", 1, Duration::from_secs(5))
            .unwrap_err()
            .to_string()
            .contains("exited while waiting for saves (0/1)"));
        assert_eq!(quick.stop_sigint(Duration::from_secs(1)).unwrap(), Some(3));
        // still running: timeouts
        let mut slow = fake_proc(tmp.path(), "sleep 30\n", false);
        assert!(slow.is_running());
        let err = slow.wait_for_log("never", Duration::from_millis(100)).unwrap_err().to_string();
        assert!(err.contains("waiting for log 'never' on fake-proc"), "{err}");
        let err = slow.wait_for_saves("w", 1, Duration::from_millis(100)).unwrap_err().to_string();
        assert!(err.contains("wait_for_saves 0/1 on fake-proc"), "{err}");
        // a world database that appears counts as one save
        let db = tmp.path().join("data/Library/ApplicationSupport/TheBlockheads/saves/w/world_db");
        std::fs::create_dir_all(&db).unwrap();
        let d2 = db.clone();
        let writer = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(200));
            std::fs::write(d2.join("data.mdb"), b"x").unwrap();
        });
        slow.wait_for_saves("w", 1, Duration::from_secs(5)).unwrap();
        writer.join().unwrap();
        // SIGINT is ignored here, so the grace period ends in SIGKILL
        let mut stubborn = fake_proc(tmp.path(), "trap '' INT\nwhile true; do sleep 0.1; done\n", false);
        std::thread::sleep(Duration::from_millis(200));
        assert_eq!(stubborn.stop_sigint(Duration::from_millis(200)).unwrap(), Some(137));
        slow.cleanup();
    }

    #[test]
    fn docker_signals_cleanup_and_keeping_data() {
        let _l = env_lock();
        let tmp = TempDir::new("proc-docker").unwrap();
        let bin = tmp.path().join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        let calls = tmp.path().join("calls.txt");
        script(&bin, "docker", &format!("echo \"$@\" >> {}\nexit 0\n", calls.display()));
        let _p = EnvGuard::set("PATH", format!("{}:{}", bin.display(), std::env::var("PATH").unwrap_or_default()));
        let mut p = fake_proc(tmp.path(), "trap 'exit 130' INT\nwhile true; do sleep 0.1; done\n", true);
        std::thread::sleep(Duration::from_millis(200));
        p.signal(libc::SIGINT);
        p.signal(libc::SIGKILL);
        p.own_log = true;
        p.tmp = Some(TempDir::new("kept").unwrap());
        let kept = p.tmp.as_ref().unwrap().path().to_path_buf();
        assert_eq!(p.keep_data(), p.data);
        assert!(kept.is_dir() && !p.own_log);
        std::fs::remove_dir_all(&kept).unwrap();
        p.own_log = true;
        p.cleanup();
        assert!(!p.log_path.exists());
        let text = std::fs::read_to_string(&calls).unwrap();
        assert!(
            text.contains("kill -s INT fake-proc")
                && text.contains("kill -s KILL fake-proc")
                && text.contains("rm -f fake-proc"),
            "{text}"
        );
    }

    #[test]
    fn handles_delegate_to_the_process() {
        let _l = env_lock();
        let tmp = TempDir::new("handles").unwrap();
        let bin = tmp.path().join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        script(&bin, "docker", "exit 0\n");
        script(tmp.path(), "run_ref.sh", "echo \"x[1] World load complete.\" >&2\nexit 0\n");
        script(tmp.path(), "run_ref_debug.sh", "echo \"Listening on port $GDBPORT\" >&2\nsleep 30\n");
        let _p = EnvGuard::set("PATH", format!("{}:{}", bin.display(), std::env::var("PATH").unwrap_or_default()));
        let _r = EnvGuard::set("BH_REFENV", tmp.path());
        let given = tmp.path().join("given-data");
        let spec = RunSpec { tag: "handles".into(), data_dir: Some(given.clone()), ..Default::default() };
        let mut r = RefServer::start(&spec).unwrap();
        assert!(r.name().starts_with("bh-diff-handles-") && r.port() > 0);
        assert_eq!(r.data_dir(), given);
        assert_eq!(r.saves_dir(), given.join("Library/ApplicationSupport/TheBlockheads/saves"));
        r.wait_for_log("World load complete.", Duration::from_secs(5)).unwrap();
        assert!(r.log().contains("World load complete."));
        assert!(r.wait_for_saves("w", 1, Duration::from_secs(5)).is_err());
        let _ = r.is_running();
        assert_eq!(r.stop_sigint(Duration::from_secs(1)).unwrap(), Some(0));
        assert_eq!(r.keep_data(), given);
        // the log of a given data dir sits next to it and is removed with the handle
        let log = PathBuf::from(format!("{}.log", given.display()));
        assert!(log.exists());
        drop(r);
        let mut d = RefDebugServer::start(&spec).unwrap();
        assert_eq!((d.data_dir(), d.name().starts_with("bh-dbg-diff-handles-")), (given.clone(), true));
        assert!(d.port() > 0 && d.gdb_port() > 0 && d.saves_dir().ends_with("saves") && d.log().contains("Listening"));
        let _ = (d.is_running(), d.keep_data());
        assert!(
            d.wait_for_saves("w", 1, Duration::from_millis(50)).is_err()
                && d.stop_sigint(Duration::from_millis(50)).is_ok()
        );
        let _ = std::fs::remove_file(&log);
        // without the launcher scripts nothing starts
        let empty = TempDir::new("no-launchers").unwrap();
        let _r2 = EnvGuard::set("BH_REFENV", empty.path());
        let plain = RunSpec { tag: "nolaunch".into(), ..Default::default() };
        assert!(matches!(RefServer::start(&plain), Err(HarnessError::Docker(_))));
        assert!(matches!(RefDebugServer::start(&plain), Err(HarnessError::Docker(_))));
    }

    #[test]
    fn our_server_with_given_data_and_every_accessor() {
        let _l = env_lock();
        let tmp = TempDir::new("ours-given").unwrap();
        let bin = script(
            tmp.path(),
            "fake_server.sh",
            "trap 'exit 130' INT\necho \"x[1] World load complete.\" >&2\nd=\"$HOME/GNUstep/Library/ApplicationSupport/TheBlockheads/saves/w/world_db\"\nmkdir -p \"$d\"\nwhile true; do echo x >> \"$d/data.mdb\"; sleep 0.1; done\n",
        );
        let _g = EnvGuard::set("BH_SERVER_BIN", &bin);
        let given = tmp.path().join("given");
        let spec = RunSpec {
            tag: "ours-given".into(),
            data_dir: Some(given.clone()),
            env: vec![("A".into(), "b".into())],
            ..Default::default()
        };
        let mut s = OurServer::start(&spec).unwrap();
        assert!(s.name().starts_with("ours-") && s.port() > 0);
        assert_eq!(s.data_dir(), given);
        assert!(s.saves_dir().starts_with(&given));
        s.wait_for_log("World load complete.", Duration::from_secs(5)).unwrap();
        assert!(s.log().contains("World load"));
        s.wait_for_saves("w", 1, Duration::from_secs(5)).unwrap();
        assert!(s.is_running());
        assert_eq!(s.keep_data(), given);
        assert_eq!(s.stop_sigint(Duration::from_secs(2)).unwrap(), Some(130));
        let mut stable = OurServer::start(&spec).unwrap();
        run_to_stable(&mut stable, "w", Duration::from_secs(5)).unwrap();
        assert!(!stable.is_running());
        let mut kept = OurServer::start(&RunSpec { data_dir: None, ..spec }).unwrap();
        let data = kept.keep_data();
        assert!(data.ends_with("GNUstep"));
        kept.stop_sigint(Duration::from_secs(2)).unwrap();
        std::fs::remove_dir_all(data.parent().unwrap()).unwrap();
    }
}
