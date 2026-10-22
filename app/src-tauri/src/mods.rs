//! Mods: what is installed in `<data dir>/mods` (read with bhmod-core, the same code the server's mod host uses),
//! `mods.toml` (on/off and permission grants), live state from the server log, the public mod index and installs
//! with size and SHA-256 checks.
//!
//! The mod host runs on the server thread and loads mods when a world starts, so every change made while the server
//! runs is applied by a restart; [`Snapshot`] remembers what the running server loaded to say what is waiting.

use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};

use bhmod_core::config::{missing_permissions, required_permissions, ModsConfig, FILE_NAME};
use bhmod_core::discover::{discover, Package};
use bhmod_core::types::Side;
use bhmod_manifest::manifest::{DepSide, Dependency, Permissions};
use bhmod_manifest::package::{self, CheckOptions, ZipSource};
use bhmod_manifest::Manifest;
use semver::{Version, VersionReq};
use serde::{Deserialize, Serialize};

/// Where the public mod list is published.
pub const INDEX_URL: &str = "https://github.com/Blockheads-Club/mods/releases/download/index/index.json";
/// Largest package the index allows (and the largest download accepted).
pub const MAX_PACKAGE: u64 = 32 * 1024 * 1024;
/// Largest index file accepted.
const MAX_INDEX: u64 = 16 * 1024 * 1024;
/// Faults and log lines kept per mod.
const KEEP_FAULTS: usize = 20;
const KEEP_LINES: usize = 60;
/// The host's per-call budgets (bhmod-core `Budgets::default()`): a slower call is logged, a call is stopped at
/// the hard limit.
pub const SOFT_MS: f64 = 5.0;
pub const HARD_MS: f64 = 25.0;

/// An error the UI shows with its own text: `code` picks the message, the rest fills it in.
#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ModError {
    pub code: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub got: Option<String>,
}

impl ModError {
    pub fn new(code: &str) -> Self {
        ModError { code: code.into(), ..Default::default() }
    }
    pub fn with(code: &str, detail: impl Into<String>) -> Self {
        ModError { code: code.into(), detail: Some(detail.into()), ..Default::default() }
    }
}

fn io_err(e: std::io::Error) -> ModError {
    ModError::with("io", e.to_string())
}

pub type MRes<T> = Result<T, ModError>;

// ---------------------------------------------------------------------------------------------------------------
// mods.toml

/// One `mods.toml` table, written back in the same shape bhmod-core reads.
#[derive(Debug, Clone, Serialize)]
struct ConfigOut {
    #[serde(skip_serializing_if = "is_true")]
    enabled: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    grant: Vec<String>,
}

fn is_true(b: &bool) -> bool {
    *b
}

/// Reads `mods.toml`; a missing file is empty, an invalid one is an error (it is never overwritten).
pub fn read_config(dir: &Path) -> MRes<ModsConfig> {
    match std::fs::read_to_string(dir.join(FILE_NAME)) {
        Ok(t) => ModsConfig::parse(&t).map_err(|e| ModError::with("config", e)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(ModsConfig::default()),
        Err(e) => Err(io_err(e)),
    }
}

/// Changes `mods.toml` with `f`; entries that are back to the defaults are dropped.
pub fn edit_config(dir: &Path, f: impl FnOnce(&mut ModsConfig)) -> MRes<()> {
    let mut cfg = read_config(dir)?;
    f(&mut cfg);
    let out: BTreeMap<&String, ConfigOut> = cfg
        .mods
        .iter()
        .filter(|(_, c)| !c.enabled || !c.grant.is_empty())
        .map(|(id, c)| (id, ConfigOut { enabled: c.enabled, grant: c.grant.clone() }))
        .collect();
    let body = toml::to_string(&out).map_err(|e| ModError::with("io", e.to_string()))?;
    std::fs::create_dir_all(dir).map_err(io_err)?;
    let text = format!(
        "# Mod settings: on/off and granted permissions. Blockheads Server keeps this file up to date.\n\n{body}"
    );
    std::fs::write(dir.join(FILE_NAME), text).map_err(io_err)
}

// ---------------------------------------------------------------------------------------------------------------
// Live state from the server log

/// What the log said about one mod during the current run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LiveState {
    Enabled,
    Disabled(String),
    Failed(String),
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Fault {
    /// Milliseconds since the Unix epoch.
    pub at: u64,
    /// "error" (a trap or timeout), "slow" (over the soft budget) or "start" (did not start).
    pub kind: String,
    pub text: String,
    /// For "slow": how long the call took.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ms: Option<f64>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ModLine {
    pub at: u64,
    /// "info", "warn" or "error".
    pub level: String,
    pub text: String,
}

/// Mod events seen in the log of the current run.
#[derive(Debug, Default, Clone)]
pub struct Live {
    pub states: HashMap<String, LiveState>,
    pub faults: HashMap<String, VecDeque<Fault>>,
    pub lines: HashMap<String, VecDeque<ModLine>>,
    /// Host lines without a mod (load problems, "loaded N mod(s)").
    pub host: VecDeque<ModLine>,
}

fn push_cap<T>(q: &mut VecDeque<T>, v: T, cap: usize) {
    if q.len() >= cap {
        q.pop_front();
    }
    q.push_back(v);
}

pub fn is_mod_id(s: &str) -> bool {
    (2..=40).contains(&s.len())
        && s.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_' || b == b'-')
}

/// Splits a host log line (`<prefix> [mods] id: text`, `[mods WARN] ...`, `[mods ERROR] ...`) into level, mod id and
/// text. Only lines whose message starts with the tag count, so chat cannot imitate them.
pub fn parse_line(line: &str) -> Option<(&'static str, Option<&str>, &str)> {
    let msg = line.split_once("] ").map_or(line, |x| x.1);
    let rest = msg.strip_prefix("[mods")?;
    let (level, rest) = if let Some(r) = rest.strip_prefix(" WARN] ") {
        ("warn", r)
    } else if let Some(r) = rest.strip_prefix(" ERROR] ") {
        ("error", r)
    } else {
        ("info", rest.strip_prefix("] ")?)
    };
    match rest.split_once(": ") {
        Some((id, text)) if is_mod_id(id) => Some((level, Some(id), text)),
        _ => Some((level, None, rest)),
    }
}

/// `"tick took 7.5 ms (budget 5 ms)"` -> 7.5.
fn slow_ms(text: &str) -> Option<f64> {
    let (_, after) = text.rsplit_once(" took ")?;
    after.split_once(" ms")?.0.parse().ok()
}

impl Live {
    /// Takes one server log line (anything else is ignored).
    pub fn push(&mut self, line: &str, now: u64) {
        let Some((level, id, text)) = parse_line(line) else { return };
        let entry = ModLine { at: now, level: level.into(), text: text.to_string() };
        let Some(id) = id else {
            // `cannot start <id>: <error>`: the runtime refused the mod.
            if let Some((id, e)) = text.strip_prefix("cannot start ").and_then(|r| r.split_once(": ")) {
                self.states.insert(id.to_string(), LiveState::Failed(e.to_string()));
                let f = Fault { at: now, kind: "start".into(), text: e.to_string(), ms: None };
                push_cap(self.faults.entry(id.to_string()).or_default(), f, KEEP_FAULTS);
                push_cap(self.lines.entry(id.to_string()).or_default(), entry.clone(), KEEP_LINES);
            }
            push_cap(&mut self.host, entry, KEEP_LINES);
            return;
        };
        push_cap(self.lines.entry(id.to_string()).or_default(), entry, KEEP_LINES);
        if level == "info" && text == "enabled" {
            self.states.insert(id.into(), LiveState::Enabled);
        } else if let Some(reason) = text.strip_prefix("disabled: ") {
            let st = if reason.starts_with("on-enable failed") {
                LiveState::Failed(reason.to_string())
            } else {
                LiveState::Disabled(reason.to_string())
            };
            self.states.insert(id.into(), st);
        } else if level == "warn" && slow_ms(text).is_some() {
            let f = Fault { at: now, kind: "slow".into(), text: text.to_string(), ms: slow_ms(text) };
            push_cap(self.faults.entry(id.into()).or_default(), f, KEEP_FAULTS);
        } else if level == "error" {
            let f = Fault { at: now, kind: "error".into(), text: text.to_string(), ms: None };
            push_cap(self.faults.entry(id.into()).or_default(), f, KEEP_FAULTS);
        }
    }
}

// ---------------------------------------------------------------------------------------------------------------
// Installed mods

/// What the running server loaded (for "restart to apply").
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Snapshot {
    /// id -> (version, enabled, granted, package fingerprint)
    pub mods: BTreeMap<String, (String, bool, Vec<String>, String)>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Sides {
    pub server: bool,
    pub client: bool,
    pub client_required: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DepInfo {
    pub id: String,
    pub req: String,
    /// Installed version, if any.
    pub installed: Option<String>,
    /// Installed, on and the version matches.
    pub ok: bool,
    /// A soft dependency (used when present).
    pub soft: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ModSummary {
    pub id: String,
    pub name: BTreeMap<String, String>,
    pub description: BTreeMap<String, String>,
    pub version: String,
    pub authors: Vec<String>,
    pub license: String,
    pub homepage: Option<String>,
    pub sides: Sides,
    /// Server-side runtime (`lua`, `wasm`, `jvm`), if the mod has a server part.
    pub runtime: Option<String>,
    /// Unsandboxed native code.
    pub native: bool,
    /// Permission names the mod asks for, what is granted and what is missing.
    pub permissions: Vec<String>,
    pub granted: Vec<String>,
    pub missing: Vec<String>,
    /// On in `mods.toml`.
    pub enabled: bool,
    /// running | off | failed | budget | dependency | permissions | ready | restart | starting
    pub status: String,
    pub detail: Option<String>,
    pub dependencies: Vec<DepInfo>,
    /// File or directory name in the mods folder.
    pub file: String,
    pub packed: bool,
    pub sha256: Option<String>,
    pub size: u64,
    /// The package is a version from the public list (its SHA-256 matches).
    pub reviewed: bool,
    /// A newer compatible version on the public list.
    pub update: Option<String>,
    pub faults: Vec<Fault>,
    pub lines: Vec<ModLine>,
    /// Slowest call logged in this run (calls under the soft budget are not logged).
    pub slowest_ms: Option<f64>,
    /// Bytes in the mod's storage for the loaded world; `None` while the server runs (the database is in use).
    pub storage_bytes: Option<u64>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Problem {
    pub file: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Change {
    /// installed | removed | updated | enabled | disabled | permissions
    pub kind: String,
    pub id: String,
    pub version: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Overview {
    pub dir: String,
    pub size: u64,
    pub mods: Vec<ModSummary>,
    pub problems: Vec<Problem>,
    /// On mods that also run on players' devices: players need the mod loader.
    pub client_mods: Vec<String>,
    /// Changes the running server has not loaded yet.
    pub changes: Vec<Change>,
    pub running: bool,
    /// The folder or `mods.toml` cannot be read.
    pub error: Option<ModError>,
    pub soft_ms: f64,
    pub hard_ms: f64,
    pub host_lines: Vec<ModLine>,
}

fn file_name(p: &Path) -> String {
    p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}

fn dir_size(p: &Path) -> u64 {
    match std::fs::symlink_metadata(p) {
        Ok(m) if m.is_dir() => std::fs::read_dir(p).map_or(0, |rd| rd.flatten().map(|e| dir_size(&e.path())).sum()),
        Ok(m) => m.len(),
        Err(_) => 0,
    }
}

fn dep_needed_on_server(d: &Dependency) -> bool {
    match d {
        Dependency::Req(_) => true,
        Dependency::Detailed { side, .. } => *side != DepSide::Client,
    }
}

fn runtime_name(m: &Manifest) -> Option<String> {
    let e = m.entry.server.as_ref()?;
    Some(format!("{:?}", e.runtime).to_lowercase())
}

/// A stable fingerprint of a package (content hash for zips, manifest text for directories).
fn fingerprint(p: &Package) -> String {
    p.sha256.clone().unwrap_or_else(|| {
        std::fs::read(p.origin.join("mod.toml")).map(|b| bhmod_manifest::pack::sha256_hex(&b)).unwrap_or_default()
    })
}

/// What `dir` holds now, in the shape of a [`Snapshot`] (taken when the server starts).
pub fn snapshot(dir: &Path) -> Snapshot {
    let cfg = read_config(dir).unwrap_or_default();
    let (pkgs, _) = discover(dir);
    let mods = pkgs
        .iter()
        .map(|p| {
            let c = cfg.get(&p.manifest.id);
            (p.manifest.id.clone(), (p.manifest.version.clone(), c.enabled, c.grant.clone(), fingerprint(p)))
        })
        .collect();
    Snapshot { mods }
}

/// Differences between what the server loaded and what is in the folder now.
pub fn changes(loaded: &Snapshot, now: &Snapshot) -> Vec<Change> {
    let mut out = Vec::new();
    let ch = |kind: &str, id: &str, v: &str| Change { kind: kind.into(), id: id.into(), version: v.into() };
    for (id, (v, en, gr, fp)) in &now.mods {
        match loaded.mods.get(id) {
            None => out.push(ch("installed", id, v)),
            Some((v0, en0, gr0, fp0)) => {
                if fp != fp0 {
                    out.push(ch("updated", id, v));
                } else if en != en0 {
                    out.push(ch(if *en { "enabled" } else { "disabled" }, id, v));
                } else if gr != gr0 && *en {
                    out.push(ch("permissions", id, v));
                } else {
                    let _ = v0;
                }
            }
        }
    }
    for (id, (v, ..)) in &loaded.mods {
        if !now.mods.contains_key(id) {
            out.push(ch("removed", id, v));
        }
    }
    out
}

/// Inputs of [`overview`] besides the folder.
#[derive(Default)]
pub struct Context<'a> {
    pub running: bool,
    pub live: Option<&'a Live>,
    pub loaded: Option<&'a Snapshot>,
    pub index: Option<&'a IndexFile>,
    /// `saves/<world>/mods/storage` of the selected world.
    pub storage: Option<PathBuf>,
}

/// Every installed mod with its status.
pub fn overview(dir: &Path, ctx: &Context) -> Overview {
    let mut ov = Overview {
        dir: dir.to_string_lossy().into_owned(),
        size: 0,
        mods: Vec::new(),
        problems: Vec::new(),
        client_mods: Vec::new(),
        changes: Vec::new(),
        running: ctx.running,
        error: None,
        soft_ms: SOFT_MS,
        hard_ms: HARD_MS,
        host_lines: ctx.live.map(|l| l.host.iter().cloned().collect()).unwrap_or_default(),
    };
    if let Err(e) = std::fs::read_dir(dir) {
        if e.kind() != std::io::ErrorKind::NotFound {
            ov.error = Some(ModError { file: Some(ov.dir.clone()), ..io_err(e) });
        }
        return ov;
    }
    ov.size = dir_size(dir);
    let cfg = match read_config(dir) {
        Ok(c) => c,
        Err(e) => {
            ov.error = Some(e);
            ModsConfig::default()
        }
    };
    let (pkgs, problems) = discover(dir);
    ov.problems = problems.into_iter().map(|p| Problem { file: file_name(&p.origin), message: p.message }).collect();
    let kinds: Vec<_> = bhmod_core::default_runtimes().iter().map(|r| r.kind()).collect();
    let (ordered, resolve_problems, _) = bhmod_core::resolve::resolve(pkgs.clone(), &cfg, Side::Server, &kinds);
    let ok: BTreeSet<&str> = ordered.iter().map(|p| p.manifest.id.as_str()).collect();
    let ok_origins: BTreeSet<&Path> = ordered.iter().map(|p| p.origin.as_path()).collect();
    let messages: HashMap<PathBuf, String> = resolve_problems.into_iter().map(|p| (p.origin, p.message)).collect();
    let versions: HashMap<&str, &str> =
        pkgs.iter().map(|p| (p.manifest.id.as_str(), p.manifest.version.as_str())).collect();
    let storage = if ctx.running { None } else { ctx.storage.as_deref().map(StorageReader::open) };
    let loaded_ids: Option<BTreeSet<&String>> = ctx.loaded.map(|s| s.mods.keys().collect());
    let changes = match ctx.loaded {
        Some(l) if ctx.running => changes(l, &snapshot(dir)),
        _ => Vec::new(),
    };
    let changed: BTreeSet<&str> = changes.iter().map(|c| c.id.as_str()).collect();
    for p in &pkgs {
        let m = &p.manifest;
        let c = cfg.get(&m.id);
        let required = required_permissions(&m.permissions);
        let missing = missing_permissions(&required, &c.grant);
        let mut deps = Vec::new();
        for (soft, map) in [(false, &m.dependencies), (true, &m.soft_dependencies)] {
            for (id, d) in map {
                if !dep_needed_on_server(d) {
                    continue;
                }
                let installed = versions.get(id.as_str()).map(|v| v.to_string());
                let matches = VersionReq::parse(d.version())
                    .ok()
                    .zip(installed.as_deref().and_then(|v| Version::parse(v).ok()))
                    .is_some_and(|(r, v)| r.matches(&v));
                deps.push(DepInfo {
                    id: id.clone(),
                    req: d.version().to_string(),
                    installed,
                    ok: matches && ok.contains(id.as_str()),
                    soft,
                });
            }
        }
        let live = ctx.live.and_then(|l| l.states.get(&m.id));
        let has_server = m.entry.server.is_some();
        let (status, detail) = if !c.enabled {
            ("off", None)
        } else if !has_server {
            // Client-only: the server sends it to players; nothing runs here.
            (if ctx.running { "running" } else { "ready" }, None)
        } else if !missing.is_empty() {
            ("permissions", Some(missing.join(", ")))
        } else if let Some(d) = deps.iter().find(|d| !d.ok && !d.soft) {
            ("dependency", Some(format!("{} {}", d.id, d.req)))
        } else if !ok_origins.contains(p.origin.as_path()) {
            ("failed", messages.get(&p.origin).cloned())
        } else if !ctx.running {
            ("ready", None)
        } else if changed.contains(m.id.as_str()) || loaded_ids.as_ref().is_some_and(|l| !l.contains(&m.id)) {
            ("restart", None)
        } else {
            match live {
                Some(LiveState::Enabled) => ("running", None),
                Some(LiveState::Disabled(r)) if r.contains("faults within") => ("budget", Some(r.clone())),
                Some(LiveState::Disabled(r)) => ("off", Some(r.clone())),
                Some(LiveState::Failed(e)) => ("failed", Some(e.clone())),
                None => ("starting", None),
            }
        };
        let faults: Vec<Fault> =
            ctx.live.and_then(|l| l.faults.get(&m.id)).map(|q| q.iter().cloned().collect()).unwrap_or_default();
        let slowest_ms = faults.iter().filter_map(|f| f.ms).reduce(f64::max);
        let index_mod = ctx.index.and_then(|i| i.mods.iter().find(|x| x.id == m.id));
        let reviewed = p
            .sha256
            .as_ref()
            .is_some_and(|s| index_mod.is_some_and(|im| im.versions.iter().any(|v| v.sha256.eq_ignore_ascii_case(s))));
        let update = index_mod.and_then(|im| im.latest()).and_then(|v| {
            let newer = Version::parse(&v.version).ok()? > m.semver();
            newer.then(|| v.version.clone())
        });
        if c.enabled && m.client_required() {
            ov.client_mods.push(m.id.clone());
        }
        ov.mods.push(ModSummary {
            id: m.id.clone(),
            name: m.name.clone(),
            description: m.description.clone(),
            version: m.version.clone(),
            authors: m.authors.clone(),
            license: m.license.clone(),
            homepage: m.homepage.clone(),
            sides: Sides { server: m.sides.server, client: m.sides.client, client_required: m.client_required() },
            runtime: runtime_name(m),
            native: m.permissions.native,
            permissions: required,
            granted: c.grant.clone(),
            missing,
            enabled: c.enabled,
            status: status.into(),
            detail,
            dependencies: deps,
            file: file_name(&p.origin),
            packed: p.sha256.is_some(),
            sha256: p.sha256.clone(),
            size: dir_size(&p.origin),
            reviewed,
            update,
            faults,
            lines: ctx.live.and_then(|l| l.lines.get(&m.id)).map(|q| q.iter().cloned().collect()).unwrap_or_default(),
            slowest_ms,
            storage_bytes: storage.as_ref().and_then(|s| s.size(&m.id)),
        });
    }
    ov.changes = changes;
    ov
}

/// Sizes and clearing of mod storage (`saves/<world>/mods/storage`, one LMDB database per mod). Only used while the
/// server is stopped: LMDB must not be opened twice in one process.
pub struct StorageReader {
    env: Option<std::rc::Rc<blockheads_server::storage::lmdb::DatabaseEnvironment>>,
}

impl StorageReader {
    pub fn open(path: &Path) -> StorageReader {
        use blockheads_server::storage::lmdb::DatabaseEnvironment;
        let env = path.join("data.mdb").is_file().then(|| DatabaseEnvironment::new(&path.to_string_lossy(), 256, 1024));
        StorageReader { env: env.flatten() }
    }

    fn db(&self, id: &str) -> Option<blockheads_server::storage::lmdb::Database> {
        blockheads_server::storage::lmdb::Database::new(self.env.as_ref()?, id)
    }

    /// Bytes of keys and values; `Some(0)` when the world has no mod storage yet.
    pub fn size(&self, id: &str) -> Option<u64> {
        if self.env.is_none() {
            return Some(0);
        }
        Some(self.db(id)?.entries().iter().map(|(k, v)| (k.len() + v.len()) as u64).sum())
    }

    /// Deletes every key of the mod.
    pub fn clear(&self, id: &str) -> bool {
        if self.env.is_none() {
            return true;
        }
        let Some(db) = self.db(id) else { return false };
        db.entries().iter().all(|(k, _)| db.remove(&String::from_utf8_lossy(k)))
    }
}

/// The package (file or directory) of an installed mod.
pub fn find_package(dir: &Path, id: &str) -> MRes<Package> {
    discover(dir).0.into_iter().find(|p| p.manifest.id == id).ok_or_else(|| ModError::with("missing", id))
}

/// A direct child of the mods folder by name (never a path).
pub fn child(dir: &Path, name: &str) -> MRes<PathBuf> {
    let bad = name.is_empty() || name.starts_with('.') || name.contains(['/', '\\']) || name == FILE_NAME;
    if bad {
        return Err(ModError::with("missing", name));
    }
    let p = dir.join(name);
    if p.symlink_metadata().is_err() {
        return Err(ModError::with("missing", name));
    }
    Ok(p)
}

pub fn set_enabled(dir: &Path, id: &str, on: bool) -> MRes<()> {
    find_package(dir, id)?;
    edit_config(dir, |c| c.mods.entry(id.into()).or_default().enabled = on)
}

/// Grants or revokes one permission the mod asks for.
pub fn set_grant(dir: &Path, id: &str, perm: &str, on: bool) -> MRes<()> {
    let p = find_package(dir, id)?;
    if on && !required_permissions(&p.manifest.permissions).iter().any(|r| r == perm) {
        return Err(ModError::with("permission", perm));
    }
    edit_config(dir, |c| {
        let g = &mut c.mods.entry(id.into()).or_default().grant;
        g.retain(|x| x != perm);
        if on {
            g.push(perm.into());
            g.sort();
        }
    })
}

/// Deletes a package (by its file or directory name) and its `mods.toml` entry. Mod storage is kept.
pub fn remove(dir: &Path, file: &str) -> MRes<()> {
    let p = child(dir, file)?;
    let id = discover(dir).0.into_iter().find(|x| x.origin == p).map(|x| x.manifest.id);
    if p.is_dir() {
        std::fs::remove_dir_all(&p).map_err(io_err)?;
    } else {
        std::fs::remove_file(&p).map_err(io_err)?;
    }
    match id {
        Some(id) => edit_config(dir, |c| {
            c.mods.remove(&id);
        }),
        None => Ok(()),
    }
}

// ---------------------------------------------------------------------------------------------------------------
// The public index

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub struct IndexFile {
    pub schema: u32,
    pub generated: String,
    #[serde(default)]
    pub api: Vec<String>,
    pub mods: Vec<IndexMod>,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub struct IndexMod {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub owners: Vec<String>,
    #[serde(default)]
    pub repo: String,
    #[serde(default)]
    pub tags: Vec<String>,
    pub versions: Vec<IndexVersion>,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub struct IndexVersion {
    pub version: String,
    pub url: String,
    #[serde(default)]
    pub mirror: Option<String>,
    pub sha256: String,
    #[serde(default)]
    pub size: Option<u64>,
    #[serde(default)]
    pub sides: Vec<String>,
    #[serde(default)]
    pub client_required: bool,
    pub api: String,
    #[serde(default)]
    pub depends: BTreeMap<String, String>,
    #[serde(default)]
    pub soft_depends: BTreeMap<String, String>,
    #[serde(default)]
    pub permissions: Permissions,
    #[serde(default)]
    pub published: String,
    #[serde(default)]
    pub yanked: bool,
}

fn api_ok(req: &str) -> bool {
    let api = Version::parse(bhmod_core::API_VERSION).expect("API_VERSION is semver");
    VersionReq::parse(req).is_ok_and(|r| r.matches(&api))
}

impl IndexVersion {
    fn usable(&self) -> bool {
        !self.yanked && api_ok(&self.api) && Version::parse(&self.version).is_ok()
    }
    fn semver(&self) -> Version {
        Version::parse(&self.version).unwrap_or(Version::new(0, 0, 0))
    }
}

impl IndexMod {
    /// The newest version this server can run.
    pub fn latest(&self) -> Option<&IndexVersion> {
        self.versions.iter().filter(|v| v.usable()).max_by_key(|v| v.semver())
    }
    /// The newest usable version matching `req`.
    fn best(&self, req: &VersionReq) -> Option<&IndexVersion> {
        self.versions.iter().filter(|v| v.usable() && req.matches(&v.semver())).max_by_key(|v| v.semver())
    }
}

pub fn parse_index(bytes: &[u8]) -> MRes<IndexFile> {
    let idx: IndexFile = serde_json::from_slice(bytes).map_err(|e| ModError::with("index", e.to_string()))?;
    if idx.schema != 1 {
        return Err(ModError::with("index", format!("unsupported schema {}", idx.schema)));
    }
    Ok(idx)
}

/// Downloads `url` (HTTPS only; `file://` and absolute paths are read from disk, for fixtures), at most `limit`
/// bytes.
pub fn fetch(url: &str, limit: u64) -> MRes<Vec<u8>> {
    let local = url.strip_prefix("file://").or_else(|| url.starts_with('/').then_some(url));
    if let Some(path) = local {
        let mut buf = Vec::new();
        let f = std::fs::File::open(path).map_err(|e| ModError::with("download", e.to_string()))?;
        f.take(limit + 1).read_to_end(&mut buf).map_err(|e| ModError::with("download", e.to_string()))?;
        return check_limit(buf, limit);
    }
    if !url.starts_with("https://") {
        return Err(ModError::with("insecure", url));
    }
    http_get(url, limit)
}

fn check_limit(buf: Vec<u8>, limit: u64) -> MRes<Vec<u8>> {
    if buf.len() as u64 > limit {
        return Err(ModError::with("tooLarge", limit.to_string()));
    }
    Ok(buf)
}

#[cfg(not(test))]
fn http_get(url: &str, limit: u64) -> MRes<Vec<u8>> {
    let resp = ureq::get(url)
        .timeout(std::time::Duration::from_secs(60))
        .set("User-Agent", concat!("blockheads-server-app/", env!("CARGO_PKG_VERSION")))
        .call()
        .map_err(|e| ModError::with("download", e.to_string()))?;
    let mut buf = Vec::new();
    resp.into_reader().take(limit + 1).read_to_end(&mut buf).map_err(|e| ModError::with("download", e.to_string()))?;
    check_limit(buf, limit)
}

/// Tests never touch the network.
#[cfg(test)]
fn http_get(url: &str, _limit: u64) -> MRes<Vec<u8>> {
    Err(ModError::with("download", format!("offline test: {url}")))
}

/// The index from `url`, falling back to the copy in `cache` when it cannot be fetched (`true` = offline copy).
pub fn load_index(url: &str, cache: &Path) -> MRes<(IndexFile, bool, Option<ModError>)> {
    match fetch(url, MAX_INDEX).and_then(|b| parse_index(&b).map(|i| (i, b))) {
        Ok((idx, bytes)) => {
            let _ = std::fs::write(cache, bytes);
            Ok((idx, false, None))
        }
        Err(e) => match std::fs::read(cache).ok().and_then(|b| parse_index(&b).ok()) {
            Some(idx) => Ok((idx, true, Some(e))),
            None => Err(ModError { code: "offline".into(), detail: e.detail.or(Some(e.code)), ..Default::default() }),
        },
    }
}

/// The cached index, if any.
pub fn cached_index(cache: &Path) -> Option<IndexFile> {
    std::fs::read(cache).ok().and_then(|b| parse_index(&b).ok())
}

/// One mod of the public list as the browse screen shows it.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BrowseMod {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub owners: Vec<String>,
    pub repo: String,
    pub tags: Vec<String>,
    pub version: String,
    pub sides: Vec<String>,
    pub client_required: bool,
    pub permissions: Vec<String>,
    pub native: bool,
    pub depends: BTreeMap<String, String>,
    pub size: Option<u64>,
    pub published: String,
    /// When the first version was published.
    pub first_published: String,
    pub installed: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Browse {
    pub generated: String,
    pub offline: bool,
    pub error: Option<ModError>,
    pub mods: Vec<BrowseMod>,
    /// Mods on the list that this server cannot run (API or yanked).
    pub hidden: usize,
}

pub fn browse(idx: &IndexFile, installed: &HashMap<String, String>, offline: bool, error: Option<ModError>) -> Browse {
    let mut mods = Vec::new();
    for m in &idx.mods {
        let Some(v) = m.latest() else { continue };
        let first = m.versions.iter().map(|v| v.published.as_str()).filter(|p| !p.is_empty()).min().unwrap_or("");
        mods.push(BrowseMod {
            id: m.id.clone(),
            name: m.name.clone(),
            description: m.description.clone(),
            owners: m.owners.clone(),
            repo: m.repo.clone(),
            tags: m.tags.clone(),
            version: v.version.clone(),
            sides: v.sides.clone(),
            client_required: v.client_required,
            permissions: required_permissions(&v.permissions),
            native: v.permissions.native,
            depends: v.depends.clone(),
            size: v.size,
            published: v.published.clone(),
            first_published: first.to_string(),
            installed: installed.get(&m.id).cloned(),
        });
    }
    let hidden = idx.mods.len() - mods.len();
    Browse { generated: idx.generated.clone(), offline, error, mods, hidden }
}

// ---------------------------------------------------------------------------------------------------------------
// Install plans

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PlanItem {
    pub id: String,
    pub name: String,
    pub version: String,
    pub size: Option<u64>,
    pub sides: Vec<String>,
    pub client_required: bool,
    pub permissions: Vec<String>,
    pub native: bool,
    /// Pulled in as a dependency of this mod.
    pub needed_by: Option<String>,
    /// Replaces this installed version.
    pub replaces: Option<String>,
    /// From the public list (false: the file or link the admin gave).
    pub reviewed: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Satisfied {
    pub id: String,
    pub req: String,
    pub version: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Missing {
    pub id: String,
    pub req: String,
    pub needed_by: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Plan {
    pub items: Vec<PlanItem>,
    pub satisfied: Vec<Satisfied>,
    pub missing: Vec<Missing>,
    pub total_bytes: u64,
}

fn plan_item(
    m: &IndexMod,
    v: &IndexVersion,
    needed_by: Option<String>,
    installed: &HashMap<String, String>,
) -> PlanItem {
    PlanItem {
        id: m.id.clone(),
        name: m.name.clone(),
        version: v.version.clone(),
        size: v.size,
        sides: v.sides.clone(),
        client_required: v.client_required,
        permissions: required_permissions(&v.permissions),
        native: v.permissions.native,
        needed_by,
        replaces: installed.get(&m.id).cloned(),
        reviewed: true,
    }
}

/// Adds the dependencies of `root` (and theirs) from the index: installed ones that match are kept, others are
/// installed or updated.
fn add_deps(
    idx: Option<&IndexFile>,
    installed: &HashMap<String, String>,
    root: (&str, &BTreeMap<String, String>),
    plan: &mut Plan,
) {
    let mut queue: VecDeque<(String, BTreeMap<String, String>)> = VecDeque::new();
    queue.push_back((root.0.to_string(), root.1.clone()));
    while let Some((owner, deps)) = queue.pop_front() {
        for (id, req_text) in deps {
            if plan.items.iter().any(|i| i.id == id) || plan.satisfied.iter().any(|s| s.id == id) {
                continue;
            }
            let req = VersionReq::parse(&req_text).unwrap_or(VersionReq::STAR);
            if let Some(v) = installed.get(&id).filter(|v| Version::parse(v).is_ok_and(|v| req.matches(&v))) {
                plan.satisfied.push(Satisfied { id, req: req_text, version: v.clone() });
                continue;
            }
            match idx.and_then(|i| i.mods.iter().find(|m| m.id == id)).and_then(|m| m.best(&req).map(|v| (m, v))) {
                Some((m, v)) => {
                    plan.items.push(plan_item(m, v, Some(owner.clone()), installed));
                    queue.push_back((id, v.depends.clone()));
                }
                None => plan.missing.push(Missing { id, req: req_text, needed_by: owner.clone() }),
            }
        }
    }
}

fn finish(mut plan: Plan) -> Plan {
    plan.total_bytes = plan.items.iter().filter_map(|i| i.size).sum();
    plan
}

/// What installing `id` (newest usable version, or `version`) from the index involves.
pub fn plan_index(idx: &IndexFile, installed: &HashMap<String, String>, id: &str, version: Option<&str>) -> MRes<Plan> {
    let m = idx.mods.iter().find(|m| m.id == id).ok_or_else(|| ModError::with("notListed", id))?;
    let v = match version {
        Some(want) => m.versions.iter().find(|v| v.version == want && v.usable()),
        None => m.latest(),
    }
    .ok_or_else(|| ModError::with("notListed", id))?;
    let mut plan =
        Plan { items: vec![plan_item(m, v, None, installed)], satisfied: vec![], missing: vec![], total_bytes: 0 };
    add_deps(Some(idx), installed, (id, &v.depends), &mut plan);
    Ok(finish(plan))
}

/// A package the admin picked (file or link), checked and waiting for the install step.
#[derive(Debug, Clone)]
pub struct Candidate {
    pub manifest: Manifest,
    pub bytes: Vec<u8>,
    pub sha256: String,
    /// Where it came from (file name or link), for messages.
    pub source: String,
}

/// Checks a package with the same rules as `bhmod check` and the mod host.
pub fn inspect(bytes: Vec<u8>, source: &str) -> MRes<Candidate> {
    if bytes.len() as u64 > MAX_PACKAGE {
        return Err(ModError::with("tooLarge", MAX_PACKAGE.to_string()));
    }
    let len = bytes.len() as u64;
    let mut src = ZipSource::open(Cursor::new(bytes.as_slice()), len).map_err(|e| ModError {
        file: Some(source.into()),
        ..ModError::with("invalid", format!("not a valid zip: {e}"))
    })?;
    let report = package::check(&mut src, CheckOptions::default());
    let manifest = match report.manifest {
        Some(m) if report.errors.is_empty() => m,
        _ => {
            let msgs: Vec<String> = report.errors.iter().map(|i| i.to_string()).collect();
            return Err(ModError { file: Some(source.into()), ..ModError::with("invalid", msgs.join("; ")) });
        }
    };
    let sha256 = bhmod_manifest::pack::sha256_hex(&bytes);
    Ok(Candidate { manifest, bytes, sha256, source: source.into() })
}

/// The plan for a candidate: the package itself plus its dependencies from the index.
pub fn plan_candidate(c: &Candidate, idx: Option<&IndexFile>, installed: &HashMap<String, String>) -> Plan {
    let m = &c.manifest;
    let reviewed = idx
        .and_then(|i| i.mods.iter().find(|x| x.id == m.id))
        .is_some_and(|x| x.versions.iter().any(|v| v.sha256.eq_ignore_ascii_case(&c.sha256)));
    let mut sides = Vec::new();
    if m.sides.server {
        sides.push("server".to_string());
    }
    if m.sides.client {
        sides.push("client".to_string());
    }
    let item = PlanItem {
        id: m.id.clone(),
        name: m.name.get("en").or_else(|| m.name.values().next()).cloned().unwrap_or_else(|| m.id.clone()),
        version: m.version.clone(),
        size: Some(c.bytes.len() as u64),
        sides,
        client_required: m.client_required(),
        permissions: required_permissions(&m.permissions),
        native: m.permissions.native,
        needed_by: None,
        replaces: installed.get(&m.id).cloned(),
        reviewed,
    };
    let deps: BTreeMap<String, String> = m
        .dependencies
        .iter()
        .filter(|(_, d)| dep_needed_on_server(d))
        .map(|(k, d)| (k.clone(), d.version().to_string()))
        .collect();
    let mut plan = Plan { items: vec![item], satisfied: vec![], missing: vec![], total_bytes: 0 };
    add_deps(idx, installed, (&m.id, &deps), &mut plan);
    finish(plan)
}

/// Downloads one index version and checks its size and SHA-256 (`mirror` is tried when `url` fails).
pub fn download(v: &IndexVersion, name: &str) -> MRes<Candidate> {
    let file = format!("{name}-{}.bhmod", v.version);
    let limit = v.size.unwrap_or(MAX_PACKAGE).min(MAX_PACKAGE);
    let bytes = match fetch(&v.url, limit) {
        Ok(b) => b,
        Err(first) => match &v.mirror {
            Some(m) => fetch(m, limit).map_err(|_| first)?,
            None => return Err(first),
        },
    };
    if let Some(size) = v.size {
        if bytes.len() as u64 != size {
            return Err(ModError {
                file: Some(file),
                expected: Some(size.to_string()),
                got: Some(bytes.len().to_string()),
                ..ModError::new("size")
            });
        }
    }
    let got = bhmod_manifest::pack::sha256_hex(&bytes);
    if !got.eq_ignore_ascii_case(&v.sha256) {
        return Err(ModError {
            file: Some(file),
            expected: Some(v.sha256.clone()),
            got: Some(got),
            ..ModError::new("checksum")
        });
    }
    let c = inspect(bytes, &file)?;
    if c.manifest.id != name || c.manifest.version != v.version {
        return Err(ModError {
            file: Some(file),
            expected: Some(format!("{name} {}", v.version)),
            got: Some(format!("{} {}", c.manifest.id, c.manifest.version)),
            ..ModError::new("mismatch")
        });
    }
    Ok(c)
}

/// Writes checked packages into the mods folder (replacing other versions of the same mods) and records the
/// choices in `mods.toml`: `grants` per mod, on when `enable`.
pub fn write_packages(
    dir: &Path,
    cands: &[Candidate],
    grants: &BTreeMap<String, Vec<String>>,
    enable: bool,
) -> MRes<()> {
    read_config(dir)?;
    std::fs::create_dir_all(dir).map_err(io_err)?;
    let (old, _) = discover(dir);
    for c in cands {
        let m = &c.manifest;
        let name = format!("{}-{}.bhmod", m.id, m.version);
        let part = dir.join(format!(".{name}.part"));
        std::fs::write(&part, &c.bytes).map_err(io_err)?;
        for p in old.iter().filter(|p| p.manifest.id == m.id && p.origin != dir.join(&name)) {
            if p.origin.is_dir() {
                std::fs::remove_dir_all(&p.origin).map_err(io_err)?;
            } else {
                std::fs::remove_file(&p.origin).map_err(io_err)?;
            }
        }
        std::fs::rename(&part, dir.join(&name)).map_err(io_err)?;
    }
    edit_config(dir, |cfg| {
        for c in cands {
            let m = &c.manifest;
            let asked = required_permissions(&m.permissions);
            let e = cfg.mods.entry(m.id.clone()).or_default();
            e.enabled = enable;
            let mut g: Vec<String> =
                grants.get(&m.id).into_iter().flatten().filter(|p| asked.contains(p)).cloned().collect();
            g.sort();
            g.dedup();
            e.grant = g;
        }
    })
}

/// Installed mods: id -> version.
pub fn installed_versions(dir: &Path) -> HashMap<String, String> {
    discover(dir).0.into_iter().map(|p| (p.manifest.id, p.manifest.version)).collect()
}

#[cfg(test)]
pub(crate) mod tests;
