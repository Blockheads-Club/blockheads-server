//! App updates from the GitHub release feed of the Blockheads Server repository.
//!
//! App only: the game server code is not touched, and nothing is downloaded or installed until the user asks for
//! it. A check reads the release list and picks the newest release above the running version (plain `vX.Y.Z` tags;
//! test versions only when the user allows them). Windows, macOS and AppImage builds then update in place through
//! the Tauri updater plugin from the release's signed `latest.json` ([`Mode::Plugin`]). Elsewhere, or for a release
//! without `latest.json`, the platform asset is downloaded, verified against `SHA256SUMS.txt` and handed to the
//! system installer; on Android that is the package installer, started on the user's tap. Without a `SHA256SUMS.txt`
//! the browser takes over. See docs/development.md ("App updates").

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Releases of the server repository, newest first.
pub const FEED: &str = "https://api.github.com/repos/Blockheads-Club/blockheads-server/releases?per_page=20";
/// Only pages and downloads under this prefix are opened from the update popup.
pub const SITE: &str = "https://github.com/Blockheads-Club/blockheads-server/";
const SUMS: &str = "SHA256SUMS.txt";
const LATEST: &str = "latest.json";
const AGENT: &str = concat!("blockheads-server-app/", env!("CARGO_PKG_VERSION"));

#[derive(Debug, Clone, Deserialize)]
pub struct GhRelease {
    pub tag_name: String,
    #[serde(default)]
    pub body: Option<String>,
    #[serde(default)]
    pub draft: bool,
    #[serde(default)]
    pub prerelease: bool,
    #[serde(default)]
    pub html_url: String,
    #[serde(default)]
    pub published_at: Option<String>,
    #[serde(default)]
    pub assets: Vec<GhAsset>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GhAsset {
    pub name: String,
    pub browser_download_url: String,
    #[serde(default)]
    pub size: u64,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Asset {
    pub name: String,
    pub url: String,
    pub size: u64,
}

/// How this platform takes the update.
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    /// Download, verify, then start the platform installer (Windows setup, macOS disk image, Linux package, Android APK).
    Installer,
    /// Download, verify, then swap the running AppImage for the new one.
    Replace,
    /// Open the download or the release page in the browser (no verifiable asset).
    Browser,
    /// The Tauri updater plugin: signed download, in-place install, relaunch.
    Plugin,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    pub version: String,
    pub current: String,
    /// Release notes as written on GitHub (Markdown).
    pub notes: String,
    /// The release page.
    pub page: String,
    pub published_at: Option<String>,
    pub prerelease: bool,
    pub asset: Option<Asset>,
    /// `SHA256SUMS.txt` of the release; required for [`Mode::Installer`] and [`Mode::Replace`].
    #[serde(skip)]
    pub sums: Option<String>,
    /// The release's `latest.json` for [`Mode::Plugin`].
    #[serde(skip)]
    pub feed: Option<String>,
    pub mode: Mode,
}

/// What the update targets: OS, CPU and, on Linux, how the app was installed.
#[derive(Debug, Clone, Default)]
pub struct Platform {
    pub os: String,
    pub arch: String,
    /// The running AppImage (`$APPIMAGE`), if any.
    pub appimage: Option<PathBuf>,
    /// Linux package format of the system: "deb", "rpm" or "".
    pub pkg: String,
    /// The updater plugin can install here (desktop Windows, macOS, AppImage).
    pub plugin: bool,
}

impl Platform {
    pub fn current() -> Self {
        let pkg = if Path::new("/etc/debian_version").exists() {
            "deb"
        } else if Path::new("/etc/redhat-release").exists() || Path::new("/etc/fedora-release").exists() {
            "rpm"
        } else {
            ""
        };
        let os = std::env::consts::OS;
        let appimage = std::env::var_os("APPIMAGE").filter(|v| !v.is_empty()).map(PathBuf::from);
        let plugin = os == "windows" || os == "macos" || (os == "linux" && appimage.is_some());
        Platform { os: os.into(), arch: std::env::consts::ARCH.into(), appimage, pkg: pkg.into(), plugin }
    }
}

/// `v1.2.3` → 1.2.3. Stable releases are plain `vX.Y.Z`; `beta` also accepts `vX.Y.Z-pre.N` style tags.
pub fn version_of(tag: &str, beta: bool) -> Option<semver::Version> {
    let v = semver::Version::parse(tag.strip_prefix('v')?).ok()?;
    (v.build.is_empty() && (beta || v.pre.is_empty())).then_some(v)
}

/// The newest published release above `current`. Drafts never count; prereleases only with `beta`.
pub fn pick<'a>(releases: &'a [GhRelease], current: &semver::Version, beta: bool) -> Option<&'a GhRelease> {
    releases
        .iter()
        .filter(|r| !r.draft && (beta || !r.prerelease))
        .filter_map(|r| version_of(&r.tag_name, beta).map(|v| (v, r)))
        .filter(|(v, _)| v > current)
        .max_by(|a, b| a.0.cmp(&b.0))
        .map(|(_, r)| r)
}

/// File name endings of the release assets (Tauri bundle names, see `.github/workflows/release.yml`).
fn suffixes(p: &Platform) -> Vec<(String, Mode)> {
    let a = |x64: &str, arm: &str| if p.arch == "aarch64" { arm.to_string() } else { x64.to_string() };
    match p.os.as_str() {
        // One universal APK (arm64-v8a, armeabi-v7a, x86_64), signed with the release key: installs over the app.
        "android" => vec![("-android.apk".into(), Mode::Installer)],
        "windows" => vec![(a("_x64-setup.exe", "_arm64-setup.exe"), Mode::Installer)],
        "macos" => vec![(a("_x64.dmg", "_aarch64.dmg"), Mode::Installer)],
        "linux" => {
            let image = a("_amd64.appimage", "_aarch64.appimage");
            if p.appimage.is_some() {
                vec![(image, Mode::Replace)]
            } else if p.pkg == "deb" {
                vec![(a("_amd64.deb", "_arm64.deb"), Mode::Installer)]
            } else if p.pkg == "rpm" {
                vec![(a(".x86_64.rpm", ".aarch64.rpm"), Mode::Installer)]
            } else {
                vec![(image, Mode::Browser)]
            }
        }
        _ => Vec::new(),
    }
}

/// The asset for this platform and how to install it.
pub fn asset_for<'a>(p: &Platform, assets: &'a [GhAsset]) -> Option<(&'a GhAsset, Mode)> {
    suffixes(p)
        .into_iter()
        .find_map(|(end, mode)| assets.iter().find(|a| a.name.to_lowercase().ends_with(&end)).map(|a| (a, mode)))
}

pub fn info_for(r: &GhRelease, current: &str, p: &Platform) -> UpdateInfo {
    let url = |name: &str| r.assets.iter().find(|a| a.name == name).map(|a| a.browser_download_url.clone());
    let (sums, latest) = (url(SUMS), url(LATEST));
    let found = asset_for(p, &r.assets);
    // Without a checksum list nothing is downloaded by the app: the browser takes over.
    let mode = match found {
        _ if p.plugin && latest.is_some() => Mode::Plugin,
        Some((_, m)) if m == Mode::Browser || sums.is_some() => m,
        _ => Mode::Browser,
    };
    let feed = if mode == Mode::Plugin { latest } else { None };
    UpdateInfo {
        version: r.tag_name.trim_start_matches('v').to_string(),
        current: current.to_string(),
        notes: r.body.clone().unwrap_or_default(),
        page: r.html_url.clone(),
        published_at: r.published_at.clone(),
        prerelease: r.prerelease,
        asset: found.map(|(a, _)| Asset { name: a.name.clone(), url: a.browser_download_url.clone(), size: a.size }),
        sums,
        feed,
        mode,
    }
}

/// Download speed (bytes/s) and seconds left after `done` of `total` bytes in `elapsed`.
pub fn rate(done: u64, total: u64, elapsed: Duration) -> (u64, u64) {
    let speed = (done as f64 / elapsed.as_secs_f64().max(0.001)) as u64;
    let eta = if speed > 0 { total.saturating_sub(done) / speed } else { 0 };
    (speed, eta)
}

/// An updater plugin error as a code for the UI.
pub fn plugin_err(e: &str) -> String {
    let e = e.to_ascii_lowercase();
    if e.contains("signature") || e.contains("minisign") {
        "signature".into()
    } else if e.contains("format") || e.contains("platform") || e.contains("target") {
        "nothing".into()
    } else {
        "offline".into()
    }
}

/// The checksum of `name` in a `sha256sum` listing (`<hex>  <name>` or `<hex> *<name>`), lower case.
pub fn parse_sums(text: &str, name: &str) -> Option<String> {
    text.lines().find_map(|l| {
        let (hash, file) = l.trim().split_once(char::is_whitespace)?;
        let file = file.trim_start().trim_start_matches('*');
        (file == name && hash.len() == 64 && hash.bytes().all(|b| b.is_ascii_hexdigit()))
            .then(|| hash.to_ascii_lowercase())
    })
}

pub fn sha256_file(path: &Path) -> std::io::Result<String> {
    let mut f = std::fs::File::open(path)?;
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(h.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

/// Only release pages and downloads of this repository are opened.
pub fn allowed_url(url: &str) -> bool {
    url.starts_with(SITE) && !url.contains("..") && !url.chars().any(char::is_whitespace)
}

/// Download state, polled by the UI.
#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    /// "idle", "downloading", "verifying", "ready", "error" or "cancelled".
    pub stage: String,
    pub done: u64,
    pub total: u64,
    pub bytes_per_s: u64,
    pub eta_s: u64,
    /// Error code ("offline", "checksum", "disk", …) or message.
    pub error: Option<String>,
}

/// After the app restarted for an update, the new process reads this note (see [`Updater::resume`]).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Pending {
    pub version: String,
    pub from: String,
    /// When the server went down for the update (ms since the Unix epoch).
    pub at: u64,
    /// The server ran before: start it again.
    pub was_running: bool,
    pub world_id: String,
}

fn net_err(e: ureq::Error) -> String {
    match e {
        ureq::Error::StatusCode(c) => format!("http {c}"),
        ureq::Error::Io(_) | ureq::Error::HostNotFound | ureq::Error::ConnectionFailed | ureq::Error::Timeout(_) => {
            "offline".into()
        }
        other => other.to_string(),
    }
}

fn agent(global: Option<Duration>) -> ureq::Agent {
    ureq::Agent::config_builder()
        .user_agent(AGENT)
        .timeout_connect(Some(Duration::from_secs(15)))
        .timeout_global(global)
        .build()
        .into()
}

/// One per app: the downloaded update and its progress.
pub struct Updater {
    dir: PathBuf,
    feed: String,
    progress: Mutex<Progress>,
    cancel: AtomicBool,
    last: Mutex<Option<UpdateInfo>>,
    file: Mutex<Option<(PathBuf, UpdateInfo)>>,
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

impl Updater {
    /// `dir`: where downloads and the restart note go. `feed`: the release list URL (tests point it elsewhere).
    pub fn new(dir: PathBuf, feed: impl Into<String>) -> Self {
        Updater {
            dir,
            feed: feed.into(),
            progress: Mutex::new(Progress { stage: "idle".into(), ..Progress::default() }),
            cancel: AtomicBool::new(false),
            last: Mutex::default(),
            file: Mutex::default(),
        }
    }

    /// Ask the feed for a newer release. `Ok(None)`: up to date.
    pub fn check(&self, current: &str, beta: bool, p: &Platform) -> Result<Option<UpdateInfo>, String> {
        let cur = semver::Version::parse(current).map_err(|e| e.to_string())?;
        let mut resp = agent(Some(Duration::from_secs(20)))
            .get(&self.feed)
            .header("Accept", "application/vnd.github+json")
            .call()
            .map_err(net_err)?;
        let releases: Vec<GhRelease> = resp.body_mut().read_json().map_err(|_| "feed".to_string())?;
        let info = pick(&releases, &cur, beta).map(|r| info_for(r, current, p));
        *lock(&self.last) = info.clone();
        Ok(info)
    }

    pub fn last(&self) -> Option<UpdateInfo> {
        lock(&self.last).clone()
    }

    pub fn progress(&self) -> Progress {
        lock(&self.progress).clone()
    }

    /// Mark a download as started; false when one is already running.
    pub fn begin(&self) -> bool {
        let mut p = lock(&self.progress);
        if p.stage == "downloading" || p.stage == "verifying" {
            return false;
        }
        *p = Progress { stage: "downloading".into(), ..Progress::default() };
        true
    }

    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::SeqCst);
    }

    /// The verified download of the last check, if there is one.
    pub fn downloaded(&self) -> Option<(PathBuf, UpdateInfo)> {
        lock(&self.file).clone().filter(|(p, _)| p.is_file())
    }

    /// Change the progress (the updater plugin path reports through this too).
    pub fn report(&self, f: impl FnOnce(&mut Progress)) -> Progress {
        self.set(f)
    }

    /// The download ended: ready, cancelled or failed.
    pub fn finish<T>(&self, r: &Result<T, String>) -> Progress {
        self.set(|p| match r {
            Ok(_) => p.stage = "ready".into(),
            Err(e) if e == "cancelled" => {
                p.stage = "cancelled".into();
                p.error = None;
            }
            Err(e) => {
                p.stage = "error".into();
                p.error = Some(e.clone());
            }
        })
    }

    fn set(&self, f: impl FnOnce(&mut Progress)) -> Progress {
        let mut p = lock(&self.progress);
        f(&mut p);
        p.clone()
    }

    /// Download and verify the asset of the last check (blocking; run it on a worker thread). `tick` sees every
    /// progress step (taskbar progress on desktop).
    pub fn download(&self, tick: &dyn Fn(&Progress)) -> Result<PathBuf, String> {
        self.cancel.store(false, Ordering::SeqCst);
        let r = self.fetch(tick);
        let p = self.finish(&r);
        tick(&p);
        r
    }

    fn fetch(&self, tick: &dyn Fn(&Progress)) -> Result<PathBuf, String> {
        let info = self.last().ok_or("nothing")?;
        let (Some(asset), Some(sums)) = (info.asset.clone(), info.sums.clone()) else { return Err("nothing".into()) };
        if info.mode == Mode::Browser || asset.name.contains(['/', '\\']) || asset.name.starts_with('.') {
            return Err("nothing".into());
        }
        tick(&self.set(|p| *p = Progress { stage: "downloading".into(), total: asset.size, ..Progress::default() }));
        let a = agent(None);
        let mut sums_resp = a.get(&sums).call().map_err(net_err)?;
        let sums_text = sums_resp.body_mut().with_config().limit(1 << 20).read_to_string().map_err(net_err)?;
        let want = parse_sums(&sums_text, &asset.name).ok_or("checksum")?;
        let mut resp = a.get(&asset.url).call().map_err(net_err)?;
        let total = resp.body().content_length().unwrap_or(asset.size);
        std::fs::create_dir_all(&self.dir).map_err(|_| "disk")?;
        let part = self.dir.join(format!("{}.part", asset.name));
        let mut out = std::fs::File::create(&part).map_err(|_| "disk")?;
        let mut body = resp.body_mut().as_reader();
        let mut buf = vec![0u8; 64 * 1024];
        let (start, mut done, mut shown) = (Instant::now(), 0u64, Instant::now());
        loop {
            if self.cancel.load(Ordering::SeqCst) {
                drop(out);
                let _ = std::fs::remove_file(&part);
                return Err("cancelled".into());
            }
            let n = body.read(&mut buf).map_err(|_| "offline")?;
            if n == 0 {
                break;
            }
            out.write_all(&buf[..n]).map_err(|_| "disk")?;
            done += n as u64;
            if shown.elapsed() >= Duration::from_millis(200) {
                shown = Instant::now();
                let (speed, eta) = rate(done, total, start.elapsed());
                tick(&self.set(|p| {
                    p.done = done;
                    p.total = total;
                    p.bytes_per_s = speed;
                    p.eta_s = eta;
                }));
            }
        }
        out.flush().map_err(|_| "disk")?;
        drop(out);
        tick(&self.set(|p| {
            p.stage = "verifying".into();
            p.done = done;
            p.total = done.max(total);
            p.eta_s = 0;
        }));
        if sha256_file(&part).map_err(|_| "disk")? != want {
            let _ = std::fs::remove_file(&part);
            return Err("checksum".into());
        }
        let file = self.dir.join(&asset.name);
        std::fs::rename(&part, &file).map_err(|_| "disk")?;
        *lock(&self.file) = Some((file.clone(), info));
        Ok(file)
    }

    fn note_path(&self) -> PathBuf {
        self.dir.join("pending.json")
    }

    /// Remember the update across the restart.
    pub fn note(&self, p: &Pending) -> Result<(), String> {
        std::fs::create_dir_all(&self.dir).map_err(|e| e.to_string())?;
        std::fs::write(self.note_path(), serde_json::to_vec(p).map_err(|e| e.to_string())?).map_err(|e| e.to_string())
    }

    /// Drop the note again (the install did not start).
    pub fn forget(&self) {
        let _ = std::fs::remove_file(self.note_path());
    }

    /// The note of an update that restarted the app (read once), and the downloads are cleaned up.
    pub fn resume(&self) -> Option<Pending> {
        let path = self.note_path();
        let note = std::fs::read(&path).ok().and_then(|b| serde_json::from_slice(&b).ok());
        let _ = std::fs::remove_file(&path);
        if note.is_some() {
            if let Ok(entries) = std::fs::read_dir(&self.dir) {
                for e in entries.flatten() {
                    let _ = std::fs::remove_file(e.path());
                }
            }
        }
        note
    }
}

/// Copy a world's save folder to `<data>/update-backups/<world>-<stamp>` before an update restart. The newest
/// three backups of each world are kept.
pub fn backup_world(data_dir: &Path, world_id: &str, stamp: u64) -> Result<Option<PathBuf>, String> {
    let src = data_dir.join("saves").join(world_id);
    if world_id.is_empty() || !src.is_dir() {
        return Ok(None);
    }
    let root = data_dir.join("update-backups");
    let dst = root.join(format!("{world_id}-{stamp}"));
    copy_dir(&src, &dst).map_err(|e| e.to_string())?;
    let mut old: Vec<PathBuf> = std::fs::read_dir(&root)
        .map_err(|e| e.to_string())?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with(&format!("{world_id}-"))))
        .collect();
    old.sort();
    while old.len() > 3 {
        let _ = std::fs::remove_dir_all(old.remove(0));
    }
    Ok(Some(dst))
}

fn copy_dir(src: &Path, dst: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for e in std::fs::read_dir(src)? {
        let e = e?;
        let to = dst.join(e.file_name());
        if e.file_type()?.is_dir() {
            copy_dir(&e.path(), &to)?;
        } else {
            std::fs::copy(e.path(), to)?;
        }
    }
    Ok(())
}

/// Swap the AppImage at `target` for `new` (written beside it, then renamed, so a failure keeps the old one).
pub fn replace_file(new: &Path, target: &Path) -> Result<(), String> {
    let tmp = target.with_extension("update");
    std::fs::copy(new, &tmp).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o755)).map_err(|e| e.to_string())?;
    }
    std::fs::rename(&tmp, target).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        e.to_string()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;

    fn rel(tag: &str, draft: bool, pre: bool, assets: &[&str]) -> GhRelease {
        GhRelease {
            tag_name: tag.into(),
            body: Some("- one\n- two".into()),
            draft,
            prerelease: pre,
            html_url: format!("{SITE}releases/tag/{tag}"),
            published_at: None,
            assets: assets
                .iter()
                .map(|n| GhAsset { name: n.to_string(), browser_download_url: format!("http://x/{n}"), size: 10 })
                .collect(),
        }
    }

    fn plat(os: &str, arch: &str) -> Platform {
        Platform { os: os.into(), arch: arch.into(), ..Platform::default() }
    }

    #[test]
    fn versions_and_channels() {
        assert_eq!(version_of("v1.2.3", false), Some(semver::Version::new(1, 2, 3)));
        assert_eq!(version_of("1.2.3", false), None, "tags start with v");
        assert_eq!(version_of("v1.2.3-rc.1", false), None);
        assert!(version_of("v1.2.3-rc.1", true).is_some());
        assert_eq!(version_of("v1.2.3+meta", true), None);
        assert_eq!(version_of("nightly", true), None);
    }

    #[test]
    fn picks_the_newest_published_release_above_the_current_one() {
        let rs = vec![
            rel("v0.3.0", true, false, &[]),
            rel("v0.2.1-rc.1", false, true, &[]),
            rel("v0.2.0", false, false, &[]),
            rel("v0.1.5", false, false, &[]),
            rel("junk", false, false, &[]),
        ];
        let cur = semver::Version::new(0, 1, 5);
        assert_eq!(pick(&rs, &cur, false).unwrap().tag_name, "v0.2.0");
        assert_eq!(pick(&rs, &cur, true).unwrap().tag_name, "v0.2.1-rc.1");
        assert!(pick(&rs, &semver::Version::new(0, 2, 0), false).is_none());
        // A prerelease flag on a plain tag still needs the test channel.
        let flagged = vec![rel("v9.0.0", false, true, &[])];
        assert!(pick(&flagged, &cur, false).is_none());
    }

    #[test]
    fn picks_the_asset_for_each_platform() {
        let names = [
            "Blockheads Server_0.2.0_x64-setup.exe",
            "Blockheads Server_0.2.0_arm64-setup.exe",
            "Blockheads Server_0.2.0_aarch64.dmg",
            "Blockheads Server_0.2.0_x64.dmg",
            "blockheads-server_0.2.0_amd64.AppImage",
            "blockheads-server_0.2.0_aarch64.AppImage",
            "blockheads-server_0.2.0_amd64.deb",
            "blockheads-server_0.2.0_arm64.deb",
            "blockheads-server-0.2.0-1.x86_64.rpm",
            "blockheads-server-v0.2.0-android.apk",
            SUMS,
        ];
        let r = rel("v0.2.0", false, false, &names);
        let get = |p: &Platform| asset_for(p, &r.assets).map(|(a, m)| (a.name.clone(), m));
        assert_eq!(get(&plat("windows", "x86_64")), Some((names[0].into(), Mode::Installer)));
        assert_eq!(get(&plat("windows", "aarch64")), Some((names[1].into(), Mode::Installer)));
        assert_eq!(get(&plat("macos", "aarch64")), Some((names[2].into(), Mode::Installer)));
        assert_eq!(get(&plat("macos", "x86_64")), Some((names[3].into(), Mode::Installer)));
        let mut img = plat("linux", "x86_64");
        img.appimage = Some("/opt/bhs.AppImage".into());
        assert_eq!(get(&img), Some((names[4].into(), Mode::Replace)));
        let mut deb = plat("linux", "aarch64");
        deb.pkg = "deb".into();
        assert_eq!(get(&deb), Some((names[7].into(), Mode::Installer)));
        let mut rpm = plat("linux", "x86_64");
        rpm.pkg = "rpm".into();
        assert_eq!(get(&rpm), Some((names[8].into(), Mode::Installer)));
        assert_eq!(get(&plat("linux", "aarch64")), Some((names[5].into(), Mode::Browser)));
        assert_eq!(get(&plat("android", "aarch64")), Some((names[9].into(), Mode::Installer)));
        assert_eq!(get(&plat("android", "armv7")), Some((names[9].into(), Mode::Installer)));
        assert_eq!(get(&plat("ios", "aarch64")), None);
        let mut rpm_arm = plat("linux", "aarch64");
        rpm_arm.pkg = "rpm".into();
        assert_eq!(get(&rpm_arm), None);

        // With a signed latest.json the updater plugin takes over where it can install.
        let mut with_latest = r.clone();
        with_latest.assets.push(GhAsset {
            name: LATEST.into(),
            browser_download_url: "http://x/latest.json".into(),
            size: 1,
        });
        let mut win = plat("windows", "x86_64");
        win.plugin = true;
        let pi = info_for(&with_latest, "0.1.0", &win);
        assert_eq!((pi.mode, pi.feed.as_deref()), (Mode::Plugin, Some("http://x/latest.json")));
        let mut deb_latest = deb.clone();
        deb_latest.pkg = "deb".into();
        let di = info_for(&with_latest, "0.1.0", &deb_latest);
        assert_eq!((di.mode, di.feed), (Mode::Installer, None));
        assert_eq!(info_for(&r, "0.1.0", &win).mode, Mode::Installer, "no latest.json: the installer hand-off");

        let info = info_for(&r, "0.1.0", &plat("windows", "x86_64"));
        assert_eq!(info.version, "0.2.0");
        assert_eq!(info.mode, Mode::Installer);
        assert!(info.sums.is_some());
        assert_eq!(info.notes, "- one\n- two");
        // No checksum list: the browser takes over. No asset at all: the release page.
        let bare = rel("v0.2.0", false, false, &names[..1]);
        assert_eq!(info_for(&bare, "0.1.0", &plat("windows", "x86_64")).mode, Mode::Browser);
        let none = info_for(&rel("v0.2.0", false, false, &[]), "0.1.0", &plat("windows", "x86_64"));
        assert_eq!((none.mode, none.asset), (Mode::Browser, None));
    }

    #[test]
    fn rates_and_plugin_errors() {
        assert_eq!(rate(50, 150, Duration::from_secs(10)), (5, 20));
        assert_eq!(rate(0, 10, Duration::ZERO), (0, 0));
        assert_eq!(plugin_err("the signature verification failed"), "signature");
        assert_eq!(plugin_err("Minisign error"), "signature");
        assert_eq!(plugin_err("invalid updater binary format"), "nothing");
        assert_eq!(plugin_err("target not found"), "nothing");
        assert_eq!(plugin_err("network error"), "offline");
    }

    #[test]
    fn sums_urls_and_hashes() {
        let h = "a".repeat(64);
        let text = format!("{h}  x.exe\n{}  y.exe\nbad line\n{h} *z.dmg\n", "B".repeat(64));
        assert_eq!(parse_sums(&text, "x.exe"), Some(h.clone()));
        assert_eq!(parse_sums(&text, "y.exe"), Some("b".repeat(64)));
        assert_eq!(parse_sums(&text, "z.dmg"), Some(h));
        assert_eq!(parse_sums(&text, "w"), None);
        assert_eq!(parse_sums("abc  w", "w"), None);

        assert!(allowed_url(&format!("{SITE}releases/tag/v1.0.0")));
        assert!(!allowed_url("https://example.com/x"));
        assert!(!allowed_url(&format!("{SITE}../../evil")));
        assert!(!allowed_url(&format!("{SITE}a b")));

        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("f");
        std::fs::write(&f, b"abc").unwrap();
        assert_eq!(sha256_file(&f).unwrap(), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
    }

    /// A small HTTP server for `n` requests; `routes` gets its base URL and maps paths to bodies (else 404).
    fn serve(n: usize, routes: impl FnOnce(&str) -> Vec<(String, Vec<u8>)>) -> String {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", l.local_addr().unwrap());
        let routes = routes(&base);
        std::thread::spawn(move || {
            for s in l.incoming().take(n) {
                let mut s = s.unwrap();
                let mut req = Vec::new();
                let mut b = [0u8; 1];
                while !req.ends_with(b"\r\n\r\n") && s.read(&mut b).unwrap_or(0) == 1 {
                    req.push(b[0]);
                }
                let path = String::from_utf8_lossy(&req).split(' ').nth(1).unwrap_or("/").to_string();
                let (status, body) = match routes.iter().find(|(p, _)| *p == path) {
                    Some((_, b)) => ("200 OK", b.clone()),
                    None => ("404 Not Found", Vec::new()),
                };
                let head = format!("HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len());
                let _ = s.write_all(head.as_bytes());
                let _ = s.write_all(&body);
            }
        });
        base
    }

    const SETUP: &str = "Blockheads Server_0.2.0_x64-setup.exe";

    fn feed_json(base: &str) -> Vec<u8> {
        serde_json::to_vec(&serde_json::json!([
            { "tag_name": "v0.2.0", "body": "- new", "draft": false, "prerelease": false,
              "html_url": format!("{SITE}releases/tag/v0.2.0"), "assets": [
                { "name": SETUP, "browser_download_url": format!("{base}/setup.exe"), "size": 5 },
                { "name": SUMS, "browser_download_url": format!("{base}/sums"), "size": 1 }
              ] }
        ]))
        .unwrap()
    }

    fn release_server(sums: String, payload: &[u8]) -> String {
        let payload = payload.to_vec();
        serve(3, move |b| {
            vec![
                ("/feed".to_string(), feed_json(b)),
                ("/sums".to_string(), sums.into_bytes()),
                ("/setup.exe".to_string(), payload),
            ]
        })
    }

    #[test]
    fn checks_downloads_and_verifies() {
        let dir = tempfile::tempdir().unwrap();
        let payload = b"hello";
        let hex: String = Sha256::digest(payload).iter().map(|b| format!("{b:02x}")).collect();
        let base = release_server(format!("{hex}  {SETUP}\n"), payload);
        let u = Updater::new(dir.path().join("updates"), format!("{base}/feed"));
        let info = u.check("0.1.0", false, &plat("windows", "x86_64")).unwrap().unwrap();
        assert_eq!((info.version.as_str(), info.mode), ("0.2.0", Mode::Installer));
        assert!(u.downloaded().is_none());
        let ticks = Mutex::new(Vec::new());
        let file = u.download(&|pr| lock(&ticks).push(pr.stage.clone())).unwrap();
        assert_eq!(std::fs::read(&file).unwrap(), payload);
        assert_eq!(u.progress().stage, "ready");
        assert!(lock(&ticks).contains(&"verifying".to_string()));
        assert_eq!(u.downloaded().unwrap().1.version, "0.2.0");
    }

    #[test]
    fn a_wrong_checksum_or_a_missing_entry_fails_the_download() {
        let dir = tempfile::tempdir().unwrap();
        for sums in [format!("{}  {SETUP}\n", "0".repeat(64)), String::new()] {
            let base = release_server(sums, b"data");
            let u = Updater::new(dir.path().join("u"), format!("{base}/feed"));
            u.check("0.1.0", false, &plat("windows", "x86_64")).unwrap().unwrap();
            assert_eq!(u.download(&|_| {}).unwrap_err(), "checksum");
            assert_eq!(u.progress().stage, "error");
            assert_eq!(u.progress().error.as_deref(), Some("checksum"));
            assert!(u.downloaded().is_none());
        }
    }

    #[test]
    fn check_errors_and_nothing_to_download() {
        let dir = tempfile::tempdir().unwrap();
        let p = plat("windows", "x86_64");
        // Up to date, then a broken feed, then a missing feed, then no server at all.
        let base = serve(3, |_| vec![("/feed".into(), b"[]".to_vec()), ("/bad".into(), b"{".to_vec())]);
        assert_eq!(Updater::new(dir.path().into(), format!("{base}/feed")).check("0.1.0", false, &p), Ok(None));
        assert_eq!(
            Updater::new(dir.path().into(), format!("{base}/bad")).check("0.1.0", false, &p),
            Err("feed".into())
        );
        assert_eq!(
            Updater::new(dir.path().into(), format!("{base}/none")).check("0.1.0", false, &p),
            Err("http 404".into())
        );
        let closed = TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap();
        assert_eq!(
            Updater::new(dir.path().into(), format!("http://{closed}/f")).check("0.1.0", false, &p),
            Err("offline".into())
        );
        let u = Updater::new(dir.path().into(), "http://127.0.0.1:9/");
        assert!(u.check("not a version", false, &p).is_err());
        // Nothing checked yet, or a browser-only update: nothing to download.
        assert_eq!(u.download(&|_| {}), Err("nothing".into()));
        *lock(&u.last) = Some(info_for(&rel("v1.0.0", false, false, &[]), "0.1.0", &p));
        assert_eq!(u.download(&|_| {}), Err("nothing".into()));
    }

    #[test]
    fn a_download_can_be_cancelled() {
        let dir = tempfile::tempdir().unwrap();
        let base = serve(2, |_| {
            vec![("/s".into(), format!("{}  a.exe", "0".repeat(64)).into_bytes()), ("/a".into(), vec![0; 10])]
        });
        let p = plat("windows", "x86_64");
        let mut info = info_for(&rel("v1.0.0", false, false, &[]), "0.1.0", &p);
        info.asset = Some(Asset { name: "a.exe".into(), url: format!("{base}/a"), size: 10 });
        info.sums = Some(format!("{base}/s"));
        info.mode = Mode::Installer;
        let u = Updater::new(dir.path().into(), "x");
        *lock(&u.last) = Some(info);
        let first = AtomicBool::new(true);
        let r = u.download(&|_| {
            if first.swap(false, Ordering::SeqCst) {
                u.cancel();
            }
        });
        assert_eq!(r, Err("cancelled".into()));
        assert_eq!(u.progress().stage, "cancelled");
        assert_eq!(u.report(|p| p.done = 3).done, 3);
        assert!(u.begin());
        assert!(!u.begin(), "one download at a time");
        assert!(!dir.path().join("a.exe.part").exists());
    }

    #[test]
    fn the_restart_note_is_read_once_and_cleans_up() {
        let dir = tempfile::tempdir().unwrap();
        let u = Updater::new(dir.path().join("updates"), "x");
        assert_eq!(u.resume(), None);
        let note =
            Pending { version: "0.2.0".into(), from: "0.1.0".into(), at: 5, was_running: true, world_id: "w".into() };
        u.note(&note).unwrap();
        std::fs::write(dir.path().join("updates").join("old.exe"), b"x").unwrap();
        assert_eq!(u.resume(), Some(note));
        assert_eq!(u.resume(), None);
        u.note(&Pending::default()).unwrap();
        u.forget();
        assert_eq!(u.resume(), None);
        assert!(!dir.path().join("updates").join("old.exe").exists());
    }

    #[test]
    fn backs_up_worlds_and_keeps_three() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(backup_world(dir.path(), "w", 1), Ok(None));
        assert_eq!(backup_world(dir.path(), "", 1), Ok(None));
        let w = dir.path().join("saves").join("w");
        std::fs::create_dir_all(w.join("sub")).unwrap();
        std::fs::write(w.join("world_db"), b"db").unwrap();
        std::fs::write(w.join("sub").join("x"), b"x").unwrap();
        for stamp in 1..=5 {
            let b = backup_world(dir.path(), "w", stamp).unwrap().unwrap();
            assert_eq!(std::fs::read(b.join("sub").join("x")).unwrap(), b"x");
        }
        let mut left: Vec<String> = std::fs::read_dir(dir.path().join("update-backups"))
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        left.sort();
        assert_eq!(left, ["w-3", "w-4", "w-5"]);
    }

    #[test]
    fn replaces_a_file_in_place() {
        let dir = tempfile::tempdir().unwrap();
        let (new, target) = (dir.path().join("new"), dir.path().join("app.AppImage"));
        std::fs::write(&new, b"v2").unwrap();
        std::fs::write(&target, b"v1").unwrap();
        replace_file(&new, &target).unwrap();
        assert_eq!(std::fs::read(&target).unwrap(), b"v2");
        assert!(replace_file(&dir.path().join("missing"), &target).is_err());
        assert!(Platform::current().os == std::env::consts::OS);
    }
}
