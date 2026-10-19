//! Blockheads Server host app: Tauri commands over [`core::Core`].

pub mod core;
pub mod notify;
pub mod stats;

use core::{Core, ListKind, NetAddr, NewWorld, Poll, RunOptions, Settings, World};
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager, State};
#[cfg(desktop)]
use tauri::{LogicalSize, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_hostservice::{HostServiceExt, ServiceInfo};

type Res<T> = Result<T, String>;

/// What the splash window shows; the hidden main window (which runs the boot work) pushes it.
#[derive(Clone, Default, Serialize, Deserialize)]
struct SplashState {
    /// "loading", "slow" or "error".
    boot: String,
    error: String,
    version: String,
    lang: String,
}

/// The splash stays up at least this long, so it is seen rather than flashed.
pub const SPLASH_MIN: Duration = Duration::from_millis(700);

/// Minimum splash time: `BHS_SPLASH_HOLD_MS` in milliseconds (0 = none), else [`SPLASH_MIN`].
fn splash_hold(var: Option<std::ffi::OsString>) -> Duration {
    var.and_then(|v| v.to_str().and_then(|s| s.trim().parse::<u64>().ok()))
        .map(Duration::from_millis)
        .unwrap_or(SPLASH_MIN)
}

/// The splash page has drawn (its first `splash_get`): the minimum splash time counts from then, not from the
/// app start, so the splash is seen for all of it.
fn splash_shown(started: &Mutex<Instant>, shown: &AtomicBool) {
    if !shown.swap(true, Ordering::SeqCst) {
        *started.lock().unwrap_or_else(|e| e.into_inner()) = Instant::now();
    }
}

/// The splash hand-over between the splash window and the hidden main window.
struct Splash {
    state: Mutex<Option<SplashState>>,
    /// When the splash page first drew (app start until then), see [`splash_shown`].
    started: Mutex<Instant>,
    shown: AtomicBool,
    hold: Duration,
    /// The main window is on screen: closing the splash is no longer a request to quit.
    revealed: AtomicBool,
}

impl Default for Splash {
    fn default() -> Self {
        Splash {
            state: Mutex::default(),
            started: Mutex::new(Instant::now()),
            shown: AtomicBool::new(false),
            hold: splash_hold(std::env::var_os("BHS_SPLASH_HOLD_MS")),
            revealed: AtomicBool::new(false),
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AppInfo {
    version: &'static str,
    data_dir: String,
    platform: &'static str,
    /// macOS draws the window buttons (overlay title bar).
    native_controls: bool,
}

#[tauri::command]
fn app_info(core: State<'_, Arc<Core>>) -> AppInfo {
    AppInfo {
        version: env!("CARGO_PKG_VERSION"),
        data_dir: core.data_dir().to_string_lossy().into_owned(),
        platform: std::env::consts::OS,
        native_controls: cfg!(target_os = "macos"),
    }
}

/// Start-up check of the data folder; the splash shows the error when it fails.
#[tauri::command]
fn boot(core: State<'_, Arc<Core>>) -> Res<()> {
    core.boot()
}

/// The main window keeps this up to date while it boots; the splash window renders it.
#[tauri::command]
fn splash_set(app: AppHandle, splash: State<'_, Splash>, state: SplashState) {
    *splash.state.lock().unwrap_or_else(|e| e.into_inner()) = Some(state.clone());
    let _ = app.emit_to("splash", "splash-state", state);
}

#[tauri::command]
fn splash_get(splash: State<'_, Splash>) -> Option<SplashState> {
    splash_shown(&splash.started, &splash.shown);
    splash.state.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

/// The splash's Try again button: the main window owns the boot work, so it is told to run it again.
#[tauri::command]
fn splash_retry(app: AppHandle) {
    let _ = app.emit_to("main", "splash-retry", ());
}

/// The splash's Choose folder button: pick the data folder, remember it and run the boot work again.
#[tauri::command]
async fn choose_data_folder(app: AppHandle, core: State<'_, Arc<Core>>) -> Res<bool> {
    #[cfg(desktop)]
    {
        use tauri_plugin_dialog::DialogExt;
        let start = core.data_dir();
        let picker = app.clone();
        let picked = tauri::async_runtime::spawn_blocking(move || {
            let mut d = picker.dialog().file();
            if let Some(parent) = start.parent().filter(|p| p.is_dir()) {
                d = d.set_directory(parent);
            }
            d.blocking_pick_folder()
        })
        .await
        .map_err(|e| e.to_string())?;
        let Some(dir) = picked else { return Ok(false) };
        let dir = dir.into_path().map_err(|e| e.to_string())?;
        core.set_data_dir(&dir)?;
        let _ = app.emit_to("main", "splash-retry", ());
        Ok(true)
    }
    #[cfg(not(desktop))]
    {
        let _ = (&app, &core);
        Err("unsupported".into())
    }
}

#[tauri::command]
fn app_quit(app: AppHandle) {
    app.exit(0);
}

/// The boot work is done: after the minimum splash time show the main window and close the splash.
#[tauri::command]
async fn main_ready(app: AppHandle, splash: State<'_, Splash>) -> Res<()> {
    let wait = splash.hold.saturating_sub(splash.started.lock().unwrap_or_else(|e| e.into_inner()).elapsed());
    if !wait.is_zero() {
        tauri::async_runtime::spawn_blocking(move || std::thread::sleep(wait)).await.map_err(|e| e.to_string())?;
    }
    splash.revealed.store(true, Ordering::SeqCst);
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        // Frameless windows on Wayland drop the config's size hints when GTK realizes them; set them again.
        #[cfg(desktop)]
        let _ = w.set_min_size(Some(LogicalSize::new(MIN_SIZE.0, MIN_SIZE.1)));
        let _ = w.set_focus();
    }
    if let Some(w) = app.get_webview_window("splash") {
        let _ = w.close();
    }
    Ok(())
}

/// The small frameless splash window (card size of the design). Transparent so the card's rounded corners show;
/// macOS cannot do that without the private API, so it gets a plain undecorated window there.
#[cfg(desktop)]
fn open_splash(app: &tauri::App) -> tauri::Result<()> {
    let b = WebviewWindowBuilder::new(app, "splash", WebviewUrl::App("splash".into()))
        .title("Blockheads Server")
        .inner_size(600.0, 380.0)
        .center()
        .decorations(false)
        .resizable(false)
        .maximizable(false)
        .minimizable(false)
        .shadow(false)
        .skip_taskbar(true)
        // Hidden until built: GTK warns when the frameless window gets its title bar after it was shown.
        .visible(false)
        .focused(true);
    #[cfg(not(target_os = "macos"))]
    let b = b.transparent(true);
    b.build()?.show()?;
    Ok(())
}

#[tauri::command]
async fn list_worlds(core: State<'_, Arc<Core>>) -> Res<Vec<World>> {
    let c = core.inner().clone();
    tauri::async_runtime::spawn_blocking(move || c.list_worlds()).await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn create_world(core: State<'_, Arc<Core>>, world: NewWorld) -> Res<World> {
    let c = core.inner().clone();
    tauri::async_runtime::spawn_blocking(move || c.create_world(&world)).await.map_err(|e| e.to_string())?
}

#[tauri::command]
async fn delete_world(core: State<'_, Arc<Core>>, id: String) -> Res<()> {
    let c = core.inner().clone();
    tauri::async_runtime::spawn_blocking(move || c.delete_world(&id)).await.map_err(|e| e.to_string())?
}

#[tauri::command]
async fn start_server(app: tauri::AppHandle, core: State<'_, Arc<Core>>, options: RunOptions) -> Res<()> {
    let c = core.inner().clone();
    let port = options.port;
    tauri::async_runtime::spawn_blocking(move || c.start(&options)).await.map_err(|e| e.to_string())??;
    let _ = app.hostservice().request_notification_permission();
    // Keeps the notification (state, player count, names) in step with the server and raises a crash alert. It
    // runs on its own thread so it works while the WebView is suspended, and ends with the server.
    let watcher_core = core.inner().clone();
    let watcher_app = app.clone();
    std::thread::spawn(move || watch_server(&watcher_app, &watcher_core, port));
    Ok(())
}

fn watch_server(app: &tauri::AppHandle, core: &Core, port: u16) {
    let mut next = 0;
    let mut last: Option<ServiceInfo> = None;
    let address = core.network_addrs(port).first().map(|a| a.endpoint.clone());
    loop {
        let p = core.poll(next);
        next = p.next;
        let settings = core.get_settings();
        let world = p
            .world_id
            .as_ref()
            .and_then(|id| core.list_worlds().into_iter().find(|w| &w.id == id))
            .map_or_else(|| "Blockheads".to_string(), |w| w.name);
        match notify::service_info(&settings.language, &p, &world, address.as_deref(), settings.run.max_players) {
            Some(info) => {
                if last.as_ref() != Some(&info) {
                    let _ = app.hostservice().start(info.clone());
                    last = Some(info);
                }
            }
            None => {
                // Stopped by the user or `/stop`: release the service. A failure also raises the alert.
                let _ = app.hostservice().stop();
                if p.state == "failed" {
                    let _ = app.hostservice().alert(notify::crash_alert(&settings.language));
                }
                return;
            }
        }
        std::thread::sleep(std::time::Duration::from_secs(1));
    }
}

#[tauri::command]
async fn stop_server(app: tauri::AppHandle, core: State<'_, Arc<Core>>) -> Res<()> {
    let c = core.inner().clone();
    let r = tauri::async_runtime::spawn_blocking(move || c.stop()).await.map_err(|e| e.to_string())?;
    let _ = app.hostservice().stop();
    r
}

#[tauri::command]
fn poll(core: State<'_, Arc<Core>>, since: u64) -> Poll {
    core.poll(since)
}

/// CPU and memory use of the app (the server runs in it), for the dashboard.
#[tauri::command]
fn stats(meter: State<'_, stats::Meter>) -> stats::Stats {
    meter.sample()
}

#[tauri::command]
fn send_command(core: State<'_, Arc<Core>>, line: String) -> Res<()> {
    core.send_command(&line)
}

#[tauri::command]
fn get_list(core: State<'_, Arc<Core>>, world_id: String, kind: String) -> Res<Vec<String>> {
    core.get_list(&world_id, ListKind::parse(&kind)?)
}

#[tauri::command]
fn set_list(core: State<'_, Arc<Core>>, world_id: String, kind: String, names: Vec<String>) -> Res<()> {
    core.set_list(&world_id, ListKind::parse(&kind)?, &names)
}

#[tauri::command]
fn network_info(core: State<'_, Arc<Core>>, port: u16) -> Vec<NetAddr> {
    core.network_addrs(port)
}

#[tauri::command]
fn get_settings(core: State<'_, Arc<Core>>) -> Settings {
    core.get_settings()
}

#[tauri::command]
fn set_settings(core: State<'_, Arc<Core>>, settings: Settings) -> Res<()> {
    core.set_settings(&settings)
}

/// Smallest main window (logical pixels): the size it opens at, as minWidth/minHeight in tauri.conf.json.
#[cfg(desktop)]
const MIN_SIZE: (f64, f64) = (1120.0, 760.0);

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default();
    #[cfg(desktop)]
    let builder = builder.plugin(tauri_plugin_dialog::init());
    builder
        .plugin(tauri_plugin_hostservice::init())
        .manage(Splash::default())
        .manage(stats::Meter::default())
        .setup(|app| {
            let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
            // The data folder is created and checked by `boot`, so a failure shows up on the splash.
            let _ = std::fs::create_dir_all(&dir);
            let core = Arc::new(Core::open(dir, std::env::var_os("BHS_DATA_DIR")));
            app.manage(core.clone());
            // "Stop server" in the Android notification: save and stop, then release the service.
            let handle = app.handle().clone();
            let _ = app.hostservice().on_stop_requested(move || {
                let (core, handle) = (core.clone(), handle.clone());
                std::thread::spawn(move || {
                    let _ = core.stop();
                    let _ = handle.hostservice().stop();
                });
            });
            // Desktop: the main window is created hidden (tauri.conf.json) and runs the boot work; `main_ready`
            // shows it. Only the splash is on screen until then. Should the splash fail to open, show the main
            // window. (Android has the system splash and an in-app error screen instead.)
            #[cfg(desktop)]
            if let Err(e) = open_splash(app) {
                eprintln!("splash window failed: {e}");
                if let Some(w) = app.get_webview_window("main") {
                    w.show()?;
                }
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            // Closing the splash (Alt+F4) before the app is up means the user wants out.
            let closing = window.label() == "splash" && matches!(event, tauri::WindowEvent::CloseRequested { .. });
            if closing && !window.state::<Splash>().revealed.load(Ordering::SeqCst) {
                window.app_handle().exit(0);
            }
            // Some compositors ignore the min-size hint of a frameless window: grow it back.
            #[cfg(desktop)]
            if let (tauri::WindowEvent::Resized(size), "main") = (event, window.label()) {
                let scale = window.scale_factor().unwrap_or(1.0);
                let (w, h) = (f64::from(size.width) / scale, f64::from(size.height) / scale);
                let maximized = window.is_maximized().unwrap_or(false) || window.is_fullscreen().unwrap_or(false);
                if !maximized && size.width > 0 && (w < MIN_SIZE.0 || h < MIN_SIZE.1) {
                    let _ = window.set_size(LogicalSize::new(w.max(MIN_SIZE.0), h.max(MIN_SIZE.1)));
                }
            }
            // Desktop: closing the window saves and stops the server.
            if let tauri::WindowEvent::Destroyed = event {
                if let Some(core) = window.app_handle().try_state::<Arc<Core>>() {
                    let _ = core.stop();
                }
                let _ = window.app_handle().hostservice().stop();
            }
        })
        .invoke_handler(tauri::generate_handler![
            app_info,
            boot,
            splash_set,
            splash_get,
            splash_retry,
            choose_data_folder,
            app_quit,
            main_ready,
            list_worlds,
            create_world,
            delete_world,
            start_server,
            stop_server,
            poll,
            stats,
            send_command,
            get_list,
            set_list,
            network_info,
            get_settings,
            set_settings
        ])
        .run(tauri::generate_context!())
        .expect("error while running the Blockheads Server app");
}

#[cfg(test)]
mod tests {

    #[test]
    fn the_splash_time_counts_from_the_first_draw() {
        let (started, shown) = (Mutex::new(Instant::now()), AtomicBool::new(false));
        std::thread::sleep(Duration::from_millis(30));
        splash_shown(&started, &shown);
        let first = *started.lock().unwrap();
        assert!(first.elapsed() < Duration::from_millis(30));
        std::thread::sleep(Duration::from_millis(5));
        splash_shown(&started, &shown);
        assert_eq!(*started.lock().unwrap(), first, "later calls keep the first draw");
    }
    use super::*;

    #[test]
    fn splash_hold_parses_milliseconds() {
        assert_eq!(splash_hold(None), SPLASH_MIN);
        assert_eq!(splash_hold(Some("".into())), SPLASH_MIN);
        assert_eq!(splash_hold(Some("soon".into())), SPLASH_MIN);
        assert_eq!(splash_hold(Some("0".into())), Duration::ZERO);
        assert_eq!(splash_hold(Some(" 2500 ".into())), Duration::from_millis(2500));
    }
}
