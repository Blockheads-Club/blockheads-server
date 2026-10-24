//! Command layer of the host app: everything the UI can do, independent of Tauri so it can be unit tested.
//!
//! The server runs in-process on a background thread (see `blockheads_server::host`). This module owns that
//! handle, keeps a bounded log buffer, tracks connected players from the log, edits the access lists and
//! persists the app settings.

use std::collections::VecDeque;
use std::net::IpAddr;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, RwLock};

use blockheads_server::host::{self, HostConfig, ServerHandle, Status, WorldSelect};
use serde::{Deserialize, Serialize};

use crate::mods;

const LOG_CAP: usize = 3000;
pub const DEFAULT_PORT: u16 = 15151;

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct World {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewWorld {
    pub name: String,
    /// "tiny" (1/16), "small" (1/4), "normal", "large" (4x), "huge" (16x).
    pub size: String,
    pub expert: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RunOptions {
    pub world_id: String,
    pub port: u16,
    pub max_players: i32,
    pub password: String,
    pub save_delay: i32,
}

impl Default for RunOptions {
    fn default() -> Self {
        RunOptions {
            world_id: String::new(),
            port: DEFAULT_PORT,
            max_players: 0,
            password: String::new(),
            save_delay: 10,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    /// UI language code (`en`, `tr`, `ru`, `kk`); empty until the app has picked one from the OS locale.
    pub language: String,
    pub run: RunOptions,
    /// App update preferences (older settings files have none: defaults).
    #[serde(default)]
    pub updates: UpdatePrefs,
    /// Mod runtime options (older settings files have none: defaults, everything off).
    #[serde(default)]
    pub mods: ModsPrefs,
}

/// Mod runtime options. JVM mods are unsandboxed code, so they are off unless the owner turns them on.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", default)]
pub struct ModsPrefs {
    /// Run JVM (Java/Kotlin) mods. Only does something in a build with the `jvm` feature.
    pub jvm: bool,
    /// The Java installation to use (empty: `JAVA_HOME`, then `java` on the `PATH`).
    pub java_home: String,
}

/// The server reads its JVM options from the environment (`BLOCKHEADS_JVM`, `BLOCKHEADS_JAVA_HOME`) when a world
/// loads; the app owns those two variables and sets them from the settings before it starts the server.
pub fn jvm_env(p: &ModsPrefs) -> [(&'static str, Option<String>); 2] {
    let home = p.java_home.trim();
    [
        ("BLOCKHEADS_JVM", p.jvm.then(|| "1".to_string())),
        ("BLOCKHEADS_JAVA_HOME", (p.jvm && !home.is_empty()).then(|| home.to_string())),
    ]
}

fn apply_jvm_env(p: &ModsPrefs) {
    for (key, value) in jvm_env(p) {
        match value {
            Some(v) => std::env::set_var(key, v),
            None => std::env::remove_var(key),
        }
    }
}

/// How the app looks for its own updates (the game server is not involved).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", default)]
pub struct UpdatePrefs {
    /// Check at start and once a day.
    pub auto_check: bool,
    /// "stable" (plain vX.Y.Z releases) or "beta" (test versions too).
    pub channel: String,
    /// Last check (ms since the Unix epoch), 0 = never; `last_ok` is the last one that worked.
    pub last_checked: u64,
    pub last_ok: u64,
    /// The newest version the popup has already opened by itself for.
    pub seen_version: String,
}

impl Default for UpdatePrefs {
    fn default() -> Self {
        UpdatePrefs {
            auto_check: true,
            channel: "stable".into(),
            last_checked: 0,
            last_ok: 0,
            seen_version: String::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Player {
    pub name: String,
    pub ip: String,
    pub id: String,
    /// When the player connected, in milliseconds since the Unix epoch.
    #[serde(default)]
    pub joined_at: u64,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Poll {
    /// "stopped" | "starting" | "running" | "stopping" | "failed"
    pub state: String,
    pub detail: Option<String>,
    pub world_id: Option<String>,
    pub port: Option<u16>,
    pub lines: Vec<String>,
    /// Arrival time (ms since the Unix epoch) of each entry of `lines`.
    pub times: Vec<u64>,
    pub next: u64,
    pub players: Vec<Player>,
    /// When the current server run started (ms since the Unix epoch); `None` while stopped.
    pub started_at: Option<u64>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct NetAddr {
    pub ip: String,
    pub endpoint: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListKind {
    Admin,
    Mod,
    White,
    Black,
}

impl ListKind {
    pub fn parse(s: &str) -> Result<Self, String> {
        match s {
            "admin" => Ok(Self::Admin),
            "mod" => Ok(Self::Mod),
            "white" => Ok(Self::White),
            "black" => Ok(Self::Black),
            other => Err(format!("unknown list: {other}")),
        }
    }
    fn file(self) -> &'static str {
        match self {
            Self::Admin => "adminlist.txt",
            Self::Mod => "modlist.txt",
            Self::White => "whitelist.txt",
            Self::Black => "blacklist.txt",
        }
    }
    fn header(self) -> &'static str {
        use blockheads_server::storage::lists as l;
        match self {
            Self::Admin => l::ADMIN_HEADER,
            Self::Mod => l::MOD_HEADER,
            Self::White => l::WHITE_HEADER,
            Self::Black => l::BLACK_HEADER,
        }
    }
    /// Console commands (add, remove) used while the server is running.
    fn commands(self) -> (&'static str, &'static str) {
        match self {
            Self::Admin => ("admin", "unadmin"),
            Self::Mod => ("mod", "unmod"),
            Self::White => ("whitelist", "unwhitelist"),
            Self::Black => ("ban", "unban"),
        }
    }
}

#[derive(Default)]
struct Shared {
    lines: VecDeque<String>,
    /// Arrival time of each line in `lines` (ms since the Unix epoch).
    times: VecDeque<u64>,
    /// Sequence number of the first line in `lines`.
    first: u64,
    players: Vec<Player>,
    /// Mod events of the current run.
    mods: crate::mods::Live,
}

impl Shared {
    fn push(&mut self, line: &str) {
        if self.lines.len() >= LOG_CAP {
            self.lines.pop_front();
            self.times.pop_front();
            self.first += 1;
        }
        self.lines.push_back(line.to_string());
        self.times.push_back(now_ms());
        track_player(&mut self.players, line);
        self.mods.push(line, now_ms());
    }
    fn next(&self) -> u64 {
        self.first + self.lines.len() as u64
    }
}

pub fn now_ms() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64)
}

/// Maintain the connected-player list from the server log.
/// `<world> - Player Connected <alias> | <ip> | <id>` and `<world> - Player Disconnected <alias>`.
pub fn track_player(players: &mut Vec<Player>, line: &str) {
    if let Some(rest) = line.split_once(" - Player Connected ").map(|x| x.1) {
        let parts: Vec<&str> = rest.splitn(3, " | ").collect();
        if parts.len() == 3 {
            let p = Player {
                name: parts[0].to_string(),
                ip: parts[1].to_string(),
                id: parts[2].trim().to_string(),
                joined_at: now_ms(),
            };
            players.retain(|q| q.id != p.id);
            players.push(p);
        }
    } else if let Some(alias) = line.split_once(" - Player Disconnected ").map(|x| x.1) {
        let alias = alias.trim();
        if let Some(i) = players.iter().position(|q| q.name == alias) {
            players.remove(i);
        }
    }
}

struct Running {
    handle: ServerHandle,
    world_id: String,
    port: u16,
    started_at: u64,
}

/// Application core. One per process.
pub struct Core {
    app_dir: PathBuf,
    data_dir: RwLock<PathBuf>,
    shared: Arc<Mutex<Shared>>,
    running: Mutex<Option<Running>>,
    /// The mods the running server loaded (taken at start).
    mods_loaded: Mutex<Option<crate::mods::Snapshot>>,
    /// A package picked from a file or link, waiting for the install step.
    candidate: Mutex<Option<crate::mods::Candidate>>,
    /// The public mod list. `BHS_MODS_INDEX` overrides it with a URL, a `file://` URL or an absolute path, e.g.
    /// `src-tauri/fixtures/mods-index.json` (sample entries for development; their packages do not exist).
    index_url: String,
}

impl Core {
    /// `app_dir`: private app storage. The server data lives in the folder the user chose (see
    /// [`Core::set_data_dir`]), else in `<app_dir>/TheBlockheads`. Nothing is touched on disk until [`Core::boot`].
    pub fn new(app_dir: PathBuf) -> Self {
        Self::open(app_dir, None)
    }

    /// Like [`Core::new`]; `override_dir` (the `BHS_DATA_DIR` variable) beats the saved choice.
    pub fn open(app_dir: PathBuf, override_dir: Option<std::ffi::OsString>) -> Self {
        let data_dir = override_dir
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
            .or_else(|| saved_data_dir(&app_dir))
            .unwrap_or_else(|| app_dir.join("TheBlockheads"));
        let index_url = std::env::var("BHS_MODS_INDEX")
            .ok()
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| crate::mods::INDEX_URL.to_string());
        Core {
            app_dir,
            data_dir: RwLock::new(data_dir),
            shared: Arc::default(),
            running: Mutex::new(None),
            mods_loaded: Mutex::new(None),
            candidate: Mutex::new(None),
            index_url,
        }
    }

    pub fn data_dir(&self) -> PathBuf {
        self.data_dir.read().unwrap_or_else(|e| e.into_inner()).clone()
    }

    /// Start-up check: the data folder exists (it is created) and can be written to.
    pub fn boot(&self) -> Result<(), String> {
        check_dir(&self.data_dir())
    }

    /// Use another data folder from now on and remember the choice. Refused while a server runs, and when the
    /// folder cannot be used (the current choice then stays).
    pub fn set_data_dir(&self, dir: &Path) -> Result<(), String> {
        if lock(&self.running).as_ref().is_some_and(|r| r.handle.is_alive()) {
            return Err("running".into());
        }
        check_dir(dir)?;
        std::fs::create_dir_all(&self.app_dir).map_err(|e| e.to_string())?;
        std::fs::write(self.app_dir.join(DATA_DIR_FILE), dir.to_string_lossy().as_bytes())
            .map_err(|e| e.to_string())?;
        *self.data_dir.write().unwrap_or_else(|e| e.into_inner()) = dir.to_path_buf();
        Ok(())
    }

    // ---- worlds ----

    pub fn list_worlds(&self) -> Vec<World> {
        host::list_worlds(Some(&self.data_dir())).into_iter().map(|w| World { id: w.id, name: w.name }).collect()
    }

    pub fn create_world(&self, w: &NewWorld) -> Result<World, String> {
        let name = w.name.trim();
        if name.is_empty() {
            return Err("name".into());
        }
        let width = width_for(&w.size)?;
        host::create_world(Some(&self.data_dir()), name, width, w.expert)
            .map(|s| World { id: s.id, name: s.name })
            .ok_or_else(|| "create".to_string())
    }

    pub fn delete_world(&self, id: &str) -> Result<(), String> {
        if lock(&self.running).as_ref().is_some_and(|r| r.handle.is_alive()) {
            return Err("running".into());
        }
        if host::delete_world(Some(&self.data_dir()), id) {
            Ok(())
        } else {
            Err("missing".into())
        }
    }

    // ---- run ----

    pub fn start(&self, o: &RunOptions) -> Result<(), String> {
        let mut run = lock(&self.running);
        if run.as_ref().is_some_and(|r| r.handle.is_alive()) {
            return Err("running".into());
        }
        if o.world_id.is_empty() {
            return Err("world".into());
        }
        if o.password.chars().count() >= 16 {
            return Err("password".into());
        }
        if !(0..=64).contains(&o.max_players) {
            return Err("maxPlayers".into());
        }
        if !(1..=3600).contains(&o.save_delay) {
            return Err("saveDelay".into());
        }
        {
            let mut s = lock(&self.shared);
            s.players.clear();
            s.mods = crate::mods::Live::default();
            s.push(&format!("--- starting world {} on port {} ---", o.world_id, o.port));
        }
        apply_jvm_env(&self.get_settings().mods);
        let shared = self.shared.clone();
        *lock(&self.mods_loaded) = Some(crate::mods::snapshot(&self.mods_dir()));
        let cfg = HostConfig {
            data_dir: Some(self.data_dir()),
            world: WorldSelect::Load(o.world_id.clone()),
            port: Some(o.port),
            max_players: o.max_players,
            save_delay: o.save_delay,
            // Applied by the server thread itself right after the world loaded, so it never depends on the UI
            // (a backgrounded Android WebView stops polling).
            on_ready_commands: if o.password.is_empty() {
                Vec::new()
            } else {
                vec![format!("/set-password {}", o.password)]
            },
            mods: blockheads_server::mods::ModsConfig { dir: Some(self.mods_dir()), disabled: false },
            ..HostConfig::default()
        };
        let handle = host::start(cfg, Arc::new(move |l: &str| lock(&shared).push(l))).map_err(|e| e.to_string())?;
        *run = Some(Running { handle, world_id: o.world_id.clone(), port: o.port, started_at: now_ms() });
        Ok(())
    }

    /// Save and stop; blocks until the server thread ended.
    pub fn stop(&self) -> Result<(), String> {
        let r = lock(&self.running).take();
        match r {
            Some(mut r) => {
                r.handle.stop();
                lock(&self.shared).players.clear();
                Ok(())
            }
            None => Ok(()),
        }
    }

    /// True while a server thread is starting, running or stopping.
    pub fn is_active(&self) -> bool {
        lock(&self.running).as_ref().is_some_and(|r| r.handle.is_alive())
    }

    pub fn send_command(&self, line: &str) -> Result<(), String> {
        let line = line.trim();
        if line.is_empty() {
            return Ok(());
        }
        match lock(&self.running).as_ref() {
            Some(r) if r.handle.is_alive() => {
                r.handle.send_command(line);
                Ok(())
            }
            _ => Err("stopped".into()),
        }
    }

    pub fn poll(&self, since: u64) -> Poll {
        let running = lock(&self.running);
        let (state, detail, world_id, port, started_at) = match running.as_ref() {
            None => ("stopped".to_string(), None, None, None, None),
            Some(r) => {
                let (s, d) = match r.handle.status() {
                    Status::Starting => ("starting", None),
                    Status::Running => ("running", None),
                    Status::Stopping => ("stopping", None),
                    Status::Stopped(0) => ("stopped", None),
                    Status::Stopped(c) => ("failed", Some(format!("exit code {c}"))),
                    Status::Failed(m) => ("failed", Some(m)),
                };
                (s.to_string(), d, Some(r.world_id.clone()), Some(r.port), Some(r.started_at))
            }
        };
        drop(running);
        let sh = lock(&self.shared);
        let start = since.max(sh.first);
        let skip = (start - sh.first) as usize;
        let lines: Vec<String> = sh.lines.iter().skip(skip).cloned().collect();
        let times: Vec<u64> = sh.times.iter().skip(skip).copied().collect();
        let players = if state == "running" { sh.players.clone() } else { Vec::new() };
        Poll { state, detail, world_id, port, lines, times, next: sh.next(), players, started_at }
    }

    // ---- lists ----

    fn list_path(&self, world_id: &str, kind: ListKind) -> Result<PathBuf, String> {
        if !valid_world_id(world_id) {
            return Err("world".into());
        }
        Ok(self.data_dir().join("saves").join(world_id).join(kind.file()))
    }

    pub fn get_list(&self, world_id: &str, kind: ListKind) -> Result<Vec<String>, String> {
        let p = self.list_path(world_id, kind)?;
        Ok(match std::fs::read_to_string(&p) {
            Ok(t) => parse_list(&t),
            Err(_) => Vec::new(),
        })
    }

    /// Replace a list. While the world runs the change is applied through console commands (so connected players
    /// are kicked/authorised like the real server does); otherwise the file is rewritten.
    pub fn set_list(&self, world_id: &str, kind: ListKind, names: &[String]) -> Result<(), String> {
        let p = self.list_path(world_id, kind)?;
        let names = clean_names(names);
        {
            let run = lock(&self.running);
            if let Some(r) = run.as_ref().filter(|r| r.world_id == world_id && r.handle.is_alive()) {
                let old = self.get_list(world_id, kind)?;
                let (add, rem) = kind.commands();
                for n in names.iter().filter(|n| !contains_ci(&old, n)) {
                    r.handle.send_command(&format!("/{add} {n}"));
                }
                for n in old.iter().filter(|n| !contains_ci(&names, n)) {
                    r.handle.send_command(&format!("/{rem} {n}"));
                }
                return Ok(());
            }
        }
        if let Some(dir) = p.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        std::fs::write(&p, render_list(kind, &names)).map_err(|e| e.to_string())
    }

    // ---- network ----

    pub fn network_addrs(&self, port: u16) -> Vec<NetAddr> {
        let mut out: Vec<NetAddr> = Vec::new();
        if let Ok(list) = local_ip_address::list_afinet_netifas() {
            for (_, ip) in list {
                if let IpAddr::V4(v4) = ip {
                    if !v4.is_loopback() && !v4.is_link_local() && !v4.is_unspecified() {
                        out.push(NetAddr { ip: v4.to_string(), endpoint: format!("{v4}:{port}") });
                    }
                }
            }
        }
        out.sort_by(|a, b| a.ip.cmp(&b.ip));
        out.dedup_by(|a, b| a.ip == b.ip);
        out
    }

    // ---- settings ----

    fn settings_path(&self) -> PathBuf {
        self.app_dir.join("app-settings.json")
    }

    pub fn get_settings(&self) -> Settings {
        std::fs::read(self.settings_path()).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
    }

    pub fn set_settings(&self, s: &Settings) -> Result<(), String> {
        let _ = std::fs::create_dir_all(&self.app_dir);
        let json = serde_json::to_vec_pretty(s).map_err(|e| e.to_string())?;
        std::fs::write(self.settings_path(), json).map_err(|e| e.to_string())
    }
}

const DATA_DIR_FILE: &str = "data-dir.txt";

/// One mod version to install from the public list.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct InstallItem {
    pub id: String,
    pub version: String,
}

/// An install the admin confirmed: the packages (dependencies first or not, order does not matter), the permissions
/// granted per mod and whether the mods are turned on. With `local`, the package picked from a file or link is one
/// of `items` and is not downloaded.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct InstallRequest {
    pub items: Vec<InstallItem>,
    #[serde(default)]
    pub grants: std::collections::BTreeMap<String, Vec<String>>,
    pub enable: bool,
    #[serde(default)]
    pub local: bool,
}

/// The package picked from a file or link and what installing it involves.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Inspected {
    pub source: String,
    pub sha256: String,
    pub plan: mods::Plan,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Installed {
    pub ids: Vec<String>,
    /// The server is running: the new mods load after a restart.
    pub restart: bool,
}

impl Core {
    /// `<data dir>/mods`, the folder the server loads mods from.
    pub fn mods_dir(&self) -> PathBuf {
        self.data_dir().join("mods")
    }

    fn index_cache(&self) -> PathBuf {
        self.app_dir.join("mods-index.json")
    }

    /// Mod storage of the loaded (or selected) world.
    fn mods_storage(&self) -> Option<PathBuf> {
        let id = lock(&self.running)
            .as_ref()
            .map(|r| r.world_id.clone())
            .unwrap_or_else(|| self.get_settings().run.world_id);
        valid_world_id(&id).then(|| self.data_dir().join("saves").join(id).join("mods/storage"))
    }

    pub fn mods_overview(&self) -> mods::Overview {
        let running = self.is_active();
        let index = mods::cached_index(&self.index_cache());
        let loaded = lock(&self.mods_loaded).clone();
        let shared = lock(&self.shared);
        let ctx = mods::Context {
            running,
            live: running.then_some(&shared.mods),
            loaded: loaded.as_ref(),
            index: index.as_ref(),
            storage: self.mods_storage(),
        };
        mods::overview(&self.mods_dir(), &ctx)
    }

    pub fn mods_set_enabled(&self, id: &str, on: bool) -> mods::MRes<()> {
        mods::set_enabled(&self.mods_dir(), id, on)
    }

    pub fn mods_set_grant(&self, id: &str, perm: &str, on: bool) -> mods::MRes<()> {
        mods::set_grant(&self.mods_dir(), id, perm, on)
    }

    pub fn mods_remove(&self, file: &str) -> mods::MRes<()> {
        mods::remove(&self.mods_dir(), file)
    }

    /// Deletes a mod's stored data for the selected world (only while the server is stopped).
    pub fn mods_clear_data(&self, id: &str) -> mods::MRes<()> {
        if self.is_active() {
            return Err(mods::ModError::new("running"));
        }
        let Some(dir) = self.mods_storage() else { return Ok(()) };
        if mods::StorageReader::open(&dir).clear(id) {
            Ok(())
        } else {
            Err(mods::ModError::with("io", "storage"))
        }
    }

    /// The folder to show for a mod (its unpacked directory, else the mods folder), created when missing.
    pub fn mods_folder(&self, id: Option<&str>) -> mods::MRes<PathBuf> {
        let dir = self.mods_dir();
        if let Some(id) = id {
            let p = mods::find_package(&dir, id)?;
            if p.origin.is_dir() {
                return Ok(p.origin);
            }
        }
        std::fs::create_dir_all(&dir).map_err(|e| mods::ModError::with("io", e.to_string()))?;
        Ok(dir)
    }

    /// The public list (fetched; the cached copy when offline).
    pub fn mods_browse(&self) -> mods::MRes<mods::Browse> {
        let (idx, offline, err) = mods::load_index(&self.index_url, &self.index_cache())?;
        Ok(mods::browse(&idx, &mods::installed_versions(&self.mods_dir()), offline, err))
    }

    fn index(&self) -> mods::MRes<mods::IndexFile> {
        match mods::cached_index(&self.index_cache()) {
            Some(i) => Ok(i),
            None => mods::load_index(&self.index_url, &self.index_cache()).map(|x| x.0),
        }
    }

    pub fn mods_plan(&self, id: &str, version: Option<&str>) -> mods::MRes<mods::Plan> {
        mods::plan_index(&self.index()?, &mods::installed_versions(&self.mods_dir()), id, version)
    }

    fn keep_candidate(&self, c: mods::Candidate) -> Inspected {
        let plan = mods::plan_candidate(&c, self.index().ok().as_ref(), &mods::installed_versions(&self.mods_dir()));
        let out = Inspected { source: c.source.clone(), sha256: c.sha256.clone(), plan };
        *lock(&self.candidate) = Some(c);
        out
    }

    /// Checks a package file the admin picked (bytes from the file chooser or a drop).
    pub fn mods_inspect_bytes(&self, name: &str, bytes: Vec<u8>) -> mods::MRes<Inspected> {
        Ok(self.keep_candidate(mods::inspect(bytes, name)?))
    }

    /// Downloads and checks a package from a link (HTTPS).
    pub fn mods_inspect_url(&self, url: &str) -> mods::MRes<Inspected> {
        let url = url.trim();
        let name = url.rsplit('/').next().unwrap_or(url).to_string();
        let bytes =
            mods::fetch(url, mods::MAX_PACKAGE).map_err(|e| mods::ModError { file: Some(name.clone()), ..e })?;
        Ok(self.keep_candidate(mods::inspect(bytes, &name)?))
    }

    /// Downloads and checks every package first, then writes them all: a failed check installs nothing.
    pub fn mods_install(&self, req: &InstallRequest, progress: &dyn Fn(&str, usize, usize)) -> mods::MRes<Installed> {
        let local = if req.local { lock(&self.candidate).clone() } else { None };
        if req.local && local.is_none() {
            return Err(mods::ModError::new("noFile"));
        }
        let downloads: Vec<&InstallItem> =
            req.items.iter().filter(|i| local.as_ref().is_none_or(|c| c.manifest.id != i.id)).collect();
        let index = if downloads.is_empty() { None } else { Some(self.index()?) };
        let total = downloads.len();
        let mut cands: Vec<mods::Candidate> = local.into_iter().collect();
        for (n, it) in downloads.iter().enumerate() {
            progress("download", n, total);
            let m = index.as_ref().and_then(|i| i.mods.iter().find(|m| m.id == it.id));
            let v = m
                .and_then(|m| m.versions.iter().find(|v| v.version == it.version))
                .ok_or_else(|| mods::ModError::with("notListed", &it.id))?;
            cands.push(mods::download(v, &it.id)?);
        }
        progress("write", total, total);
        mods::write_packages(&self.mods_dir(), &cands, &req.grants, req.enable)?;
        if req.local {
            *lock(&self.candidate) = None;
        }
        progress("done", total, total);
        Ok(Installed { ids: cands.into_iter().map(|c| c.manifest.id).collect(), restart: self.is_active() })
    }
}

fn saved_data_dir(app_dir: &Path) -> Option<PathBuf> {
    let text = std::fs::read_to_string(app_dir.join(DATA_DIR_FILE)).ok()?;
    let text = text.trim();
    (!text.is_empty()).then(|| PathBuf::from(text))
}

/// Creates `dir` if needed and proves it is writable. The error names the folder and the OS reason.
fn check_dir(dir: &Path) -> Result<(), String> {
    let fail = |e: std::io::Error| format!("{}: {e}", dir.display());
    std::fs::create_dir_all(dir).map_err(fail)?;
    let probe = dir.join(".bhs-write-test");
    std::fs::write(&probe, b"").map_err(fail)?;
    let _ = std::fs::remove_file(probe);
    Ok(())
}

/// World ids are the server's save ids: 32 lowercase hex characters. Anything else (path separators, `..`,
/// drive prefixes such as `C:x`) is rejected so list files can never escape the data directory.
pub fn valid_world_id(id: &str) -> bool {
    id.len() == 32 && id.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

pub fn width_for(size: &str) -> Result<i32, String> {
    Ok(match size {
        "tiny" => 116,
        "small" => 104,
        "normal" => 1,
        "large" => 4,
        "huge" => 16,
        _ => return Err("size".into()),
    })
}

/// Names of a list file: the first line is a header and ignored, then one name per line.
pub fn parse_list(text: &str) -> Vec<String> {
    text.lines().skip(1).map(|l| l.trim().to_string()).filter(|l| !l.is_empty()).collect()
}

pub fn render_list(kind: ListKind, names: &[String]) -> String {
    let mut s = kind.header().to_string();
    for n in names {
        s.push_str(n);
        s.push('\n');
    }
    s
}

fn clean_names(names: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for n in names {
        let n = n.trim().replace(['\n', '\r'], " ");
        if !n.is_empty() && !contains_ci(&out, &n) {
            out.push(n);
        }
    }
    out
}

fn contains_ci(v: &[String], n: &str) -> bool {
    v.iter().any(|x| x.eq_ignore_ascii_case(n))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tracks_players() {
        let mut p = Vec::new();
        track_player(&mut p, "W - Player Connected alice | 10.0.0.2 | pid1");
        track_player(&mut p, "W - Player Connected bob | 10.0.0.3 | pid2");
        assert_eq!(p.len(), 2);
        track_player(&mut p, "W - Player Connected alice | 10.0.0.2 | pid1");
        assert_eq!(p.len(), 2);
        track_player(&mut p, "W - Player Disconnected alice");
        assert_eq!(p.len(), 1);
        assert_eq!((p[0].name.as_str(), p[0].ip.as_str(), p[0].id.as_str()), ("bob", "10.0.0.3", "pid2"));
        assert!(p[0].joined_at > 0);
        track_player(&mut p, "unrelated");
        track_player(&mut p, "W - Player Connected broken");
        assert_eq!(p.len(), 1);
    }

    #[test]
    fn log_ring_buffer() {
        let mut s = Shared::default();
        for i in 0..LOG_CAP + 5 {
            s.push(&format!("l{i}"));
        }
        assert_eq!(s.lines.len(), LOG_CAP);
        assert_eq!(s.first, 5);
        assert_eq!(s.next(), (LOG_CAP + 5) as u64);
        assert_eq!(s.lines[0], "l5");
        assert_eq!(s.times.len(), LOG_CAP);
    }

    #[test]
    fn list_files_roundtrip() {
        let t = render_list(ListKind::Admin, &["Alice".into(), "bob".into()]);
        assert_eq!(parse_list(&t), vec!["Alice", "bob"]);
        assert_eq!(parse_list(ListKind::White.header()), Vec::<String>::new());
        assert_eq!(clean_names(&[" a ".into(), "A".into(), "".into(), "b\nc".into()]), vec!["a", "b c"]);
        assert!(ListKind::parse("x").is_err());
        assert_eq!(ListKind::parse("black").unwrap().commands(), ("ban", "unban"));
    }

    #[test]
    fn sizes() {
        assert_eq!(width_for("normal"), Ok(1));
        assert_eq!(width_for("tiny"), Ok(116));
        assert!(width_for("x").is_err());
    }

    #[test]
    fn jvm_prefs_set_the_server_environment() {
        assert_eq!(ModsPrefs::default(), ModsPrefs { jvm: false, java_home: String::new() });
        let off = jvm_env(&ModsPrefs { jvm: false, java_home: "/opt/jdk".into() });
        assert_eq!(off, [("BLOCKHEADS_JVM", None), ("BLOCKHEADS_JAVA_HOME", None)]);
        let on = jvm_env(&ModsPrefs { jvm: true, java_home: " /opt/jdk ".into() });
        assert_eq!(on, [("BLOCKHEADS_JVM", Some("1".into())), ("BLOCKHEADS_JAVA_HOME", Some("/opt/jdk".into()))]);
        let on = jvm_env(&ModsPrefs { jvm: true, java_home: "  ".into() });
        assert_eq!(on, [("BLOCKHEADS_JVM", Some("1".into())), ("BLOCKHEADS_JAVA_HOME", None)]);
        let saved: Vec<_> = ["BLOCKHEADS_JVM", "BLOCKHEADS_JAVA_HOME"].iter().map(|k| (*k, std::env::var_os(k))).collect();
        apply_jvm_env(&ModsPrefs { jvm: true, java_home: "/opt/jdk".into() });
        assert_eq!(std::env::var("BLOCKHEADS_JVM").unwrap(), "1");
        assert_eq!(std::env::var("BLOCKHEADS_JAVA_HOME").unwrap(), "/opt/jdk");
        apply_jvm_env(&ModsPrefs::default());
        assert!(std::env::var_os("BLOCKHEADS_JVM").is_none() && std::env::var_os("BLOCKHEADS_JAVA_HOME").is_none());
        for (k, v) in saved {
            match v {
                Some(v) => std::env::set_var(k, v),
                None => std::env::remove_var(k),
            }
        }
    }

    #[test]
    fn settings_persist_and_default() {
        let d = tempfile::tempdir().unwrap();
        let c = Core::new(d.path().to_path_buf());
        assert_eq!(c.get_settings(), Settings::default());
        assert_eq!(Settings::default().language, "");
        let mut s = Settings { language: "tr".into(), ..Default::default() };
        s.run.port = 15200;
        c.set_settings(&s).unwrap();
        assert_eq!(c.get_settings(), s);
        std::fs::write(d.path().join("app-settings.json"), b"garbage").unwrap();
        assert_eq!(c.get_settings(), Settings::default());
        // Settings from before app updates: the update preferences take their defaults.
        let old = br#"{"language":"en","run":{"worldId":"","port":15151,"maxPlayers":0,"password":"","saveDelay":10}}"#;
        std::fs::write(d.path().join("app-settings.json"), old).unwrap();
        let u = c.get_settings().updates;
        assert!(u.auto_check);
        assert_eq!((u.channel.as_str(), u.last_checked), ("stable", 0));
    }

    #[test]
    fn validation_errors() {
        let d = tempfile::tempdir().unwrap();
        let c = Core::new(d.path().to_path_buf());
        let w = |n: &str, s: &str| NewWorld { name: n.into(), size: s.into(), expert: false };
        assert_eq!(c.create_world(&w("  ", "normal")).unwrap_err(), "name");
        assert_eq!(c.create_world(&w("a", "bogus")).unwrap_err(), "size");
        let mut o = RunOptions::default();
        assert_eq!(c.start(&o).unwrap_err(), "world");
        o.world_id = "x".into();
        o.password = "x".repeat(16);
        assert_eq!(c.start(&o).unwrap_err(), "password");
        o.password.clear();
        o.max_players = 99;
        assert_eq!(c.start(&o).unwrap_err(), "maxPlayers");
        o.max_players = 0;
        o.save_delay = 0;
        assert_eq!(c.start(&o).unwrap_err(), "saveDelay");
        assert_eq!(c.send_command("/list").unwrap_err(), "stopped");
        assert!(c.send_command("  ").is_ok());
        assert_eq!(c.get_list("../x", ListKind::Admin).unwrap_err(), "world");
        assert_eq!(c.get_list("C:x", ListKind::Admin).unwrap_err(), "world");
        assert_eq!(c.get_list("ABCDEF0123456789ABCDEF0123456789", ListKind::Admin).unwrap_err(), "world");
        assert!(valid_world_id("0123456789abcdef0123456789abcdef"));
        assert_eq!(c.poll(0).state, "stopped");
        assert_eq!(c.delete_world("nope").unwrap_err(), "missing");
        assert!(c.stop().is_ok());
    }

    #[test]
    fn data_dir_boot_choice_and_override() {
        let d = tempfile::tempdir().unwrap();
        let app = d.path().join("app");
        let c = Core::new(app.clone());
        assert_eq!(c.data_dir(), app.join("TheBlockheads"));
        assert!(!app.exists(), "nothing is created before boot");
        c.boot().unwrap();
        assert!(app.join("TheBlockheads").is_dir());

        // A chosen folder is used at once and remembered by the next start.
        let other = d.path().join("worlds");
        c.set_data_dir(&other).unwrap();
        assert_eq!(c.data_dir(), other);
        assert_eq!(Core::new(app.clone()).data_dir(), other);
        // The override beats the saved choice; an empty one is ignored.
        assert_eq!(Core::open(app.clone(), Some("/x/y".into())).data_dir(), PathBuf::from("/x/y"));
        assert_eq!(Core::open(app.clone(), Some("".into())).data_dir(), other);

        // A folder that cannot be used is refused and the choice stays; boot reports it too.
        let file = d.path().join("file");
        std::fs::write(&file, b"x").unwrap();
        let err = c.set_data_dir(&file.join("sub")).unwrap_err();
        assert!(err.contains("sub"), "{err}");
        assert_eq!(c.data_dir(), other);
        assert!(Core::open(app, Some(file.join("sub").into())).boot().is_err());
        assert!(saved_data_dir(d.path()).is_none());
        std::fs::write(d.path().join(DATA_DIR_FILE), b"  \n").unwrap();
        assert!(saved_data_dir(d.path()).is_none());
    }

    #[test]
    fn network_lists_ipv4_only() {
        let d = tempfile::tempdir().unwrap();
        let c = Core::new(d.path().to_path_buf());
        for a in c.network_addrs(15151) {
            assert!(a.endpoint.ends_with(":15151"));
            assert!(!a.ip.starts_with("127."));
        }
    }

    #[test]
    fn world_lifecycle_lists_and_run() {
        let d = tempfile::tempdir().unwrap();
        let c = Core::new(d.path().to_path_buf());
        let w = c.create_world(&NewWorld { name: "Test".into(), size: "small".into(), expert: false }).unwrap();
        assert_eq!(w.name, "TEST");
        assert_eq!(c.list_worlds(), vec![w.clone()]);

        // Stopped: lists are plain files.
        assert!(c.get_list(&w.id, ListKind::Black).unwrap().is_empty());
        c.set_list(&w.id, ListKind::Admin, &["Alice".into()]).unwrap();
        assert_eq!(c.get_list(&w.id, ListKind::Admin).unwrap(), vec!["Alice"]);

        // Run it.
        let o = RunOptions { world_id: w.id.clone(), port: 15390, password: "secret".into(), ..RunOptions::default() };
        c.start(&o).unwrap();
        assert_eq!(c.start(&o).unwrap_err(), "running");
        assert_eq!(c.delete_world(&w.id).unwrap_err(), "running");
        let end = std::time::Instant::now() + std::time::Duration::from_secs(60);
        while c.poll(0).state != "running" {
            assert!(std::time::Instant::now() < end, "never ran: {:?}", c.poll(0));
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        let p = c.poll(0);
        assert_eq!(p.world_id.as_deref(), Some(w.id.as_str()));
        assert!(p.lines.iter().any(|l| l.contains("World load complete.")));
        c.set_list(&w.id, ListKind::Mod, &["Zed".into()]).unwrap();
        c.send_command("/players").unwrap();
        std::thread::sleep(std::time::Duration::from_millis(1500));
        let after = c.poll(p.next);
        assert!(after.next >= p.next);
        c.stop().unwrap();
        assert_eq!(c.poll(0).state, "stopped");
        assert!(c.poll(0).lines.iter().any(|l| l.contains("Password set") || l.contains("World load complete.")));
        // Console-applied list change was persisted by the server.
        assert!(c.get_list(&w.id, ListKind::Mod).unwrap().iter().any(|n| n.eq_ignore_ascii_case("zed")));
        c.delete_world(&w.id).unwrap();
        assert!(c.list_worlds().is_empty());
    }

    #[test]
    fn mods_through_the_core() {
        use crate::mods::tests::{fixture_index, spec, write_dir, Spec};
        use std::collections::BTreeMap;
        let d = tempfile::tempdir().unwrap();
        let fx = tempfile::tempdir().unwrap();
        let (_, shop) = fixture_index(fx.path());
        // The fixture index again, as a file the core reads through BHS_MODS_INDEX (tests run single-threaded).
        let index = fx.path().join("index.json");
        {
            use crate::mods::tests::{index_json, mod_json, version_json};
            let base = std::fs::read(fx.path().join("base.bhmod")).unwrap();
            let url = fx.path().join("base.bhmod").to_string_lossy().into_owned();
            std::fs::write(&index, index_json(&[mod_json("base", &[version_json("1.0.0", &url, &base, "")])])).unwrap();
        }
        std::env::set_var("BHS_MODS_INDEX", &index);
        let c = Core::new(d.path().join("app"));
        std::env::remove_var("BHS_MODS_INDEX");
        c.set_data_dir(&d.path().join("data")).unwrap();
        let o = c.mods_overview();
        assert!(o.mods.is_empty() && !o.running);

        // The public list and an install from it.
        // Nothing cached yet: the plan fetches the list.
        assert_eq!(c.mods_plan("base", None).unwrap().items.len(), 1);
        let b = c.mods_browse().unwrap();
        assert_eq!(b.mods[0].id, "base");
        let steps = std::cell::RefCell::new(Vec::new());
        let req = InstallRequest {
            items: vec![InstallItem { id: "base".into(), version: "1.0.0".into() }],
            grants: BTreeMap::new(),
            enable: true,
            local: false,
        };
        let done = c.mods_install(&req, &|s, n, t| steps.borrow_mut().push(format!("{s} {n}/{t}"))).unwrap();
        assert_eq!(done, Installed { ids: vec!["base".into()], restart: false });
        assert_eq!(*steps.borrow(), vec!["download 0/1", "write 1/1", "done 1/1"]);
        let bad = InstallRequest { items: vec![InstallItem { id: "nope".into(), version: "1".into() }], ..req.clone() };
        assert_eq!(c.mods_install(&bad, &|_, _, _| {}).unwrap_err().code, "notListed");
        assert_eq!(
            c.mods_install(&InstallRequest { local: true, ..req.clone() }, &|_, _, _| {}).unwrap_err().code,
            "noFile"
        );

        // A file the admin picked: its dependencies come from the list.
        let ins = c.mods_inspect_bytes("shop.bhmod", shop.clone()).unwrap();
        assert_eq!(ins.plan.items[0].id, "shop");
        assert!(!ins.plan.items[0].reviewed, "the list the core reads does not have it");
        assert_eq!(ins.plan.missing.len(), 1, "economy is not on this list");
        let mut grants = BTreeMap::new();
        grants.insert("shop".to_string(), vec![]);
        let local = InstallRequest {
            items: vec![InstallItem { id: "shop".into(), version: "1.0.0".into() }],
            grants,
            enable: false,
            local: true,
        };
        assert_eq!(c.mods_install(&local, &|_, _, _| {}).unwrap().ids, vec!["shop"]);
        assert_eq!(c.mods_inspect_bytes("x", b"junk".to_vec()).unwrap_err().code, "invalid");
        let url = fx.path().join("shop.bhmod");
        assert_eq!(c.mods_inspect_url(&format!(" {} ", url.display())).unwrap().plan.items[0].id, "shop");
        let e = c.mods_inspect_url("http://example.com/x.bhmod").unwrap_err();
        assert_eq!((e.code.as_str(), e.file.as_deref()), ("insecure", Some("x.bhmod")));

        let o = c.mods_overview();
        let ids: Vec<(&str, &str)> = o.mods.iter().map(|m| (m.id.as_str(), m.status.as_str())).collect();
        assert_eq!(ids, vec![("base", "ready"), ("shop", "off")]);
        assert!(o.mods[0].reviewed);
        c.mods_set_enabled("shop", true).unwrap();
        assert_eq!(c.mods_overview().mods[1].status, "dependency");
        assert_eq!(c.mods_set_grant("shop", "native", true).unwrap_err().code, "permission");
        assert_eq!(c.mods_folder(None).unwrap(), c.mods_dir());
        assert_eq!(c.mods_folder(Some("base")).unwrap(), c.mods_dir(), "a packed mod shows the mods folder");
        assert_eq!(c.mods_folder(Some("nope")).unwrap_err().code, "missing");
        c.mods_remove("shop-1.0.0.bhmod").unwrap();
        // No world selected: nothing to clear.
        c.mods_clear_data("base").unwrap();

        // A Luau mod on a running server: live state from the log, changes wait for a restart.
        let dir = c.mods_dir();
        let main = "local m = {}\nfunction m.on_enable() end\nreturn m\n";
        let h = write_dir(&dir, "hello", &Spec { extra: "[permissions]\nplayers = [\"chat\"]\n", ..spec("hello") });
        std::fs::write(h.join("server/main.luau"), main).unwrap();
        c.mods_set_grant("hello", "players:chat", true).unwrap();
        assert_eq!(c.mods_folder(Some("hello")).unwrap(), h);
        let w = c.create_world(&NewWorld { name: "Modded".into(), size: "tiny".into(), expert: false }).unwrap();
        c.set_settings(&Settings {
            language: "en".into(),
            run: RunOptions { world_id: w.id.clone(), ..RunOptions::default() },
            ..Settings::default()
        })
        .unwrap();
        c.mods_clear_data("hello").unwrap();
        let o = RunOptions { world_id: w.id.clone(), port: 15391, ..RunOptions::default() };
        c.start(&o).unwrap();
        let end = std::time::Instant::now() + std::time::Duration::from_secs(60);
        let status = |c: &Core| c.mods_overview().mods.iter().find(|m| m.id == "hello").map(|m| m.status.clone());
        while status(&c).as_deref() != Some("running") {
            assert!(std::time::Instant::now() < end, "never ran: {:?} {:?}", c.mods_overview(), c.poll(0).lines);
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        let ov = c.mods_overview();
        assert!(ov.running && ov.changes.is_empty());
        assert!(ov.mods.iter().all(|m| m.storage_bytes.is_none()));
        assert_eq!(c.mods_clear_data("hello").unwrap_err().code, "running");
        c.mods_set_enabled("base", false).unwrap();
        let ov = c.mods_overview();
        assert_eq!(ov.changes.len(), 1);
        let again = InstallRequest { enable: true, ..req };
        assert!(c.mods_install(&again, &|_, _, _| {}).unwrap().restart);
        c.stop().unwrap();
        assert!(!c.mods_overview().running);
    }
}
