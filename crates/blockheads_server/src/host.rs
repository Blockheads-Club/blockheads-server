//! Embedding API: run the dedicated server inside another program (desktop GUI, Android app, tests).
//!
//! The server keeps its state in thread-locals, so every run lives on its own thread and a fresh thread is a
//! clean restart. `process::exit` calls of the game code unwind the thread instead of ending the host process.
//!
//! ```no_run
//! use blockheads_server::host::{self, HostConfig, WorldSelect};
//! let cfg = HostConfig { world: WorldSelect::New { name: "MyWorld".into(), width_multiplier: 1 }, ..HostConfig::default() };
//! let mut server = host::start(cfg, std::sync::Arc::new(|line| println!("{line}"))).unwrap();
//! server.stop();
//! ```

use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Sender};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use crate::app::delegate::{self, CommandLineDelegate, InitParams};
use crate::app::run_loop::RunLoop;
use crate::app::stdin::StdinHandle;
use crate::foundation::exception::{catch_ns, NsException};
use crate::foundation::sys::{self, HostExit};
use crate::foundation::{nslog, paths};

pub use crate::app::delegate::SavedWorld;

/// Callback receiving each log line (NSLog format, no trailing newline). Called from the server thread.
pub type LogCallback = Arc<dyn Fn(&str) + Send + Sync>;

/// Which world to run.
#[derive(Clone, Debug)]
pub enum WorldSelect {
    /// An existing world, by id or name.
    Load(String),
    /// Create a world (name is uppercased like `--new`) and run it. `width_multiplier`: 1, 4, 16, 104, 116.
    New { name: String, width_multiplier: i32 },
}

/// Server settings. Defaults match the CLI defaults.
#[derive(Clone, Debug)]
pub struct HostConfig {
    /// Data directory (the `TheBlockheads` application-support directory). `None`: `BLOCKHEADS_DATA_DIR`, else
    /// `<home>/GNUstep/Library/ApplicationSupport/TheBlockheads`.
    pub data_dir: Option<PathBuf>,
    pub world: WorldSelect,
    /// UDP port; `None` keeps the world's stored port (15151 for new worlds).
    pub port: Option<u16>,
    /// 0 keeps the world's setting.
    pub max_players: i32,
    pub save_delay: i32,
    pub owner_name: Option<String>,
    pub privacy: Option<String>,
    pub expert_mode: bool,
    /// `--custom-rules <json>` (only with [`WorldSelect::New`]), e.g. `{"seed":"custom","custom_seed_value":"1"}`.
    pub custom_rules_json: Option<String>,
    /// Console lines sent once, on the server thread, right after `World load complete.` and before any other
    /// console input is read (e.g. `/set-password ...`). Independent of any UI polling.
    pub on_ready_commands: Vec<String>,
    /// `--world_id` of a new world (`None`: a random id).
    pub world_id: Option<String>,
    /// Mods (not in the original). The default loads mods from `BLOCKHEADS_MODS_DIR` or `<data dir>/mods` when
    /// that directory exists; without mods the server behaves exactly like vanilla.
    pub mods: crate::mods::ModsConfig,
    /// The CLI's `-N`: do not stop the server when the last client has left (the vanilla server exits then).
    pub no_exit: bool,
}

impl Default for HostConfig {
    fn default() -> Self {
        HostConfig {
            data_dir: None,
            world: WorldSelect::New { name: "WORLD".into(), width_multiplier: 1 },
            port: None,
            max_players: 0,
            save_delay: 10,
            owner_name: None,
            privacy: None,
            expert_mode: false,
            custom_rules_json: None,
            on_ready_commands: Vec::new(),
            world_id: None,
            mods: crate::mods::ModsConfig::default(),
            no_exit: false,
        }
    }
}

/// Lifecycle of a hosted server.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Status {
    /// Thread started, world still loading.
    Starting,
    /// `World load complete.` was logged.
    Running,
    /// A stop was requested.
    Stopping,
    /// The server ended (exit code as the CLI would have returned it).
    Stopped(i32),
    /// The server died with an error (message).
    Failed(String),
}

/// Handle of a running server.
pub struct ServerHandle {
    cmd: Sender<Vec<u8>>,
    status: Arc<Mutex<Status>>,
    join: Option<JoinHandle<()>>,
}

fn set_status(s: &Mutex<Status>, v: Status) {
    *s.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = v;
}

fn describe(ex: &NsException) -> String {
    format!("{}: {}", ex.name, ex.reason_or_null())
}

/// The first `World load complete.` line turns `Starting` into `Running` and releases the commands queued for
/// that moment; later lines, and a server that is already stopping, change nothing.
fn note_ready(status: &Mutex<Status>, line: &str, ready_cmds: &[String], tx: &Sender<Vec<u8>>) {
    if line.ends_with("World load complete.") {
        let mut g = status.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        if *g == Status::Starting {
            *g = Status::Running;
            for c in ready_cmds {
                let mut b = c.as_bytes().to_vec();
                b.push(b'\n');
                let _ = tx.send(b);
            }
        }
    }
}

/// How a server thread ended: an exit unwinds with [`HostExit`], an escaped exception or panic is a failure.
fn end_status(result: std::thread::Result<Result<bool, NsException>>) -> Status {
    match result {
        Ok(Ok(_)) => Status::Stopped(0),
        Ok(Err(ex)) => Status::Failed(describe(&ex)),
        Err(payload) => match payload.downcast::<HostExit>() {
            Ok(code) => Status::Stopped(code.0),
            Err(other) => {
                let msg = other
                    .downcast_ref::<&str>()
                    .map(|s| s.to_string())
                    .or_else(|| other.downcast_ref::<String>().cloned())
                    .unwrap_or_else(|| "panic".to_string());
                Status::Failed(msg)
            }
        },
    }
}

fn prepare_thread(data_dir: Option<&Path>, log: Option<LogCallback>) {
    sys::set_hosted(true);
    paths::set_data_dir_override(data_dir.map(|d| d.to_string_lossy().into_owned()));
    nslog::set_thread_sink(log);
}

/// A sink that drops every line (worker threads of the world management calls have no log).
fn silent_sink() -> LogCallback {
    Arc::new(|_| {})
}

/// Run `f` on a fresh hosted thread and return its result (`None` if it panicked or exited).
fn on_worker<T: Send + 'static>(data_dir: Option<PathBuf>, f: impl FnOnce() -> T + Send + 'static) -> Option<T> {
    std::thread::Builder::new()
        .name("bh-host-op".into())
        .spawn(move || {
            prepare_thread(data_dir.as_deref(), Some(silent_sink()));
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)).ok()
        })
        .ok()?
        .join()
        .ok()?
}

/// Start a server on its own thread. Returns as soon as the thread runs; watch [`ServerHandle::status`].
pub fn start(cfg: HostConfig, log: LogCallback) -> std::io::Result<ServerHandle> {
    let status = Arc::new(Mutex::new(Status::Starting));
    let (tx, rx) = channel::<Vec<u8>>();
    let tx_ready = tx.clone();
    let st = status.clone();
    let join = std::thread::Builder::new().name("bh-server".into()).spawn(move || {
        let st_log = st.clone();
        let user_log = log.clone();
        let ready_tx = tx_ready;
        let ready_cmds = cfg.on_ready_commands.clone();
        let sink: LogCallback = Arc::new(move |line: &str| {
            note_ready(&st_log, line, &ready_cmds, &ready_tx);
            user_log(line);
        });
        prepare_thread(cfg.data_dir.as_deref(), Some(sink));
        crate::mods::set_config(cfg.mods.clone());
        nslog::set_process_name("blockheads_server");
        let (new_name, load_name, width) = match &cfg.world {
            WorldSelect::Load(n) => (None, Some(n.clone()), -1),
            WorldSelect::New { name, width_multiplier } => {
                (Some(crate::foundation::nsstring::uppercase_string(name)), None, *width_multiplier)
            }
        };
        let p = InitParams {
            new_name,
            load_name,
            port: cfg.port.map(|p| p.to_string()),
            max_players: cfg.max_players,
            save_delay: cfg.save_delay,
            width_multiplier: width,
            credit: -1,
            owner_name: cfg.owner_name.clone(),
            privacy: cfg.privacy.clone(),
            expert_mode: cfg.expert_mode,
            world_id: cfg.world_id.clone(),
            custom_rules_json: cfg.custom_rules_json.clone(),
            no_exit: cfg.no_exit,
            ..InitParams::default()
        };
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
            catch_ns(move || {
                let delegate = CommandLineDelegate::init_with_new_world_name(p);
                if let Some(d) = &delegate {
                    // Console: commands injected through the handle.
                    let handle = StdinHandle::from_channel(rx);
                    d.attach_console(handle);
                    let rl = RunLoop::current();
                    while !d.done() && rl.run_mode_default_before_distant_future() {
                        if cfg.no_exit {
                            d.exit_when_stopped();
                        }
                    }
                }
                delegate.is_some()
            })
        }));
        set_status(&st, end_status(result));
        nslog::set_thread_sink(None);
    });
    join.map(|join| ServerHandle { cmd: tx, status, join: Some(join) })
}

impl ServerHandle {
    /// Current status.
    pub fn status(&self) -> Status {
        self.status.lock().unwrap_or_else(std::sync::PoisonError::into_inner).clone()
    }

    /// True while the server thread is alive.
    pub fn is_alive(&self) -> bool {
        matches!(self.status(), Status::Starting | Status::Running | Status::Stopping)
    }

    /// Send a console line (`/list`, `/stop`, chat text,...). A newline is appended.
    pub fn send_command(&self, line: &str) {
        let mut b = line.as_bytes().to_vec();
        b.push(b'\n');
        let _ = self.cmd.send(b);
    }

    /// Ask the server to save and stop; does not wait.
    pub fn request_stop(&self) {
        {
            let mut g = self.status.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
            if matches!(*g, Status::Starting | Status::Running) {
                *g = Status::Stopping;
            }
        }
        self.send_command("/stop");
    }

    /// Wait for the server thread to end and return the final status.
    pub fn wait(&mut self) -> Status {
        if let Some(j) = self.join.take() {
            let _ = j.join();
        }
        self.status()
    }

    /// Stop the server (saves the world) and wait for it.
    pub fn stop(&mut self) -> Status {
        self.request_stop();
        self.wait()
    }
}

impl Drop for ServerHandle {
    fn drop(&mut self) {
        if self.join.is_some() {
            self.stop();
        }
    }
}

/// Worlds in the data directory, newest first.
pub fn list_worlds(data_dir: Option<&Path>) -> Vec<SavedWorld> {
    on_worker(data_dir.map(Path::to_path_buf), delegate::list_saved_worlds).unwrap_or_default()
}

/// Create a world without starting it. `width_multiplier`: 1, 4, 16, 104 or 116.
pub fn create_world(
    data_dir: Option<&Path>,
    name: &str,
    width_multiplier: i32,
    expert_mode: bool,
) -> Option<SavedWorld> {
    let name = name.to_string();
    on_worker(data_dir.map(Path::to_path_buf), move || {
        delegate::create_saved_world(&name, width_multiplier, expert_mode)
    })
    .flatten()
}

/// Delete a world by id or name. False when it does not exist.
pub fn delete_world(data_dir: Option<&Path>, id_or_name: &str) -> bool {
    let n = id_or_name.to_string();
    on_worker(data_dir.map(Path::to_path_buf), move || delegate::delete_saved_world(&n)).unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_first_ready_line_runs_the_queued_commands_once() {
        let (tx, rx) = channel::<Vec<u8>>();
        let status = Mutex::new(Status::Starting);
        let cmds = vec!["/a".to_string(), "/b".to_string()];
        note_ready(&status, "something else", &cmds, &tx);
        assert_eq!(*status.lock().unwrap(), Status::Starting);
        note_ready(&status, "2026 [1] World load complete.", &cmds, &tx);
        assert_eq!(*status.lock().unwrap(), Status::Running);
        assert_eq!(rx.try_iter().collect::<Vec<_>>(), [b"/a\n".to_vec(), b"/b\n".to_vec()]);
        note_ready(&status, "World load complete.", &cmds, &tx);
        assert!(rx.try_recv().is_err());
        *status.lock().unwrap() = Status::Stopping;
        note_ready(&status, "World load complete.", &cmds, &tx);
        assert_eq!(*status.lock().unwrap(), Status::Stopping);
    }

    #[test]
    fn every_way_a_server_thread_can_end() {
        let ex = NsException { name: "NSRangeException".into(), reason: Some("r".into()) };
        assert_eq!(end_status(Ok(Ok(true))), Status::Stopped(0));
        assert_eq!(end_status(Ok(Err(ex))), Status::Failed("NSRangeException: r".to_string()));
        assert_eq!(end_status(Err(Box::new(HostExit(7)))), Status::Stopped(7));
        assert_eq!(end_status(Err(Box::new("static message"))), Status::Failed("static message".to_string()));
        assert_eq!(
            end_status(Err(Box::new(String::from("owned message")))),
            Status::Failed("owned message".to_string())
        );
        assert_eq!(end_status(Err(Box::new(5u8))), Status::Failed("panic".to_string()));
    }

    #[test]
    fn worker_operations_swallow_panics_and_log_into_the_void() {
        let dir = std::env::temp_dir().join(format!("bh_host_unit_{}", std::process::id()));
        assert_eq!(on_worker(Some(dir.clone()), || 5), Some(5));
        assert_eq!(on_worker(Some(dir.clone()), || crate::nslog!("goes to the silent sink")), Some(()));
        assert_eq!(on_worker(None, || std::hint::black_box(None::<i32>).unwrap()), None);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn worlds_can_be_managed_without_a_server() {
        let dir = std::env::temp_dir().join(format!("bh_host_worlds_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        assert!(list_worlds(Some(&dir)).is_empty());
        let w = create_world(Some(&dir), "managed", 116, false).unwrap();
        assert_eq!(w.name, "MANAGED");
        assert_eq!(list_worlds(Some(&dir)), vec![w.clone()]);
        assert!(!delete_world(Some(&dir), "nothing"));
        assert!(delete_world(Some(&dir), &w.id));
        assert!(list_worlds(Some(&dir)).is_empty());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn dropping_a_running_handle_stops_it() {
        let dir = std::env::temp_dir().join(format!("bh_host_drop_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let cfg = HostConfig {
            data_dir: Some(dir.clone()),
            world: WorldSelect::New { name: "drop".into(), width_multiplier: 116 },
            port: Some(std::net::UdpSocket::bind("0.0.0.0:0").unwrap().local_addr().unwrap().port()),
            ..HostConfig::default()
        };
        let seen = Arc::new(Mutex::new(Vec::<String>::new()));
        let s2 = seen.clone();
        let h = start(cfg, Arc::new(move |l: &str| s2.lock().unwrap().push(l.to_string()))).unwrap();
        // A load of an unknown world ends by itself without a failure.
        let mut gone = start(
            HostConfig {
                data_dir: Some(dir.clone()),
                world: WorldSelect::Load("nothing".into()),
                ..HostConfig::default()
            },
            Arc::new(|_| {}),
        )
        .unwrap();
        assert_eq!(gone.wait(), Status::Stopped(0));
        assert!(!gone.is_alive());
        drop(h); // stops the server and waits for it
        assert!(seen.lock().unwrap().iter().any(|l| l.contains("Exiting World.")));
        let _ = std::fs::remove_dir_all(dir);
    }
}
