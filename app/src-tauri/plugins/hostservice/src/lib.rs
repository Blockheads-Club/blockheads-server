//! Tauri plugin: starts/stops the Android foreground service that hosts the server process in the background.
//! On desktop every command is a no-op.

use serde::{Deserialize, Serialize};
use tauri::{
    plugin::{Builder, TauriPlugin},
    Manager, Runtime,
};

#[cfg(target_os = "android")]
use tauri::plugin::PluginHandle;

#[cfg(target_os = "android")]
const PLUGIN_IDENTIFIER: &str = "club.blockheads.hostservice";

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ServiceInfo {
    pub title: String,
    pub text: String,
    /// Expanded notification body (for example the connected player names).
    pub big_text: Option<String>,
    /// When the server started (ms since the Unix epoch); drives the notification's elapsed-time counter.
    pub started_at: Option<u64>,
    /// Show an indeterminate progress bar (the world is still loading).
    pub loading: bool,
    /// The address the "copy" action puts on the clipboard.
    pub address: Option<String>,
    /// Labels of the notification actions (already translated).
    pub stop_label: String,
    pub copy_label: String,
    pub open_label: String,
}

/// A one-off alert on a separate high-importance channel (for example "server stopped unexpectedly").
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct AlertInfo {
    pub title: String,
    pub text: String,
    pub open_label: String,
}

/// Handle to the (Android only) native side.
pub struct HostService<R: Runtime> {
    #[cfg(target_os = "android")]
    handle: PluginHandle<R>,
    #[cfg(not(target_os = "android"))]
    _marker: std::marker::PhantomData<fn() -> R>,
}

impl<R: Runtime> HostService<R> {
    /// Start the foreground service (or update its notification text).
    pub fn start(&self, info: ServiceInfo) -> Result<(), String> {
        #[cfg(target_os = "android")]
        {
            self.handle.run_mobile_plugin::<serde_json::Value>("start", info).map(|_| ()).map_err(|e| e.to_string())
        }
        #[cfg(not(target_os = "android"))]
        {
            let _ = info;
            Ok(())
        }
    }

    /// Post (or replace) the crash alert notification.
    pub fn alert(&self, info: AlertInfo) -> Result<(), String> {
        #[cfg(target_os = "android")]
        {
            self.handle.run_mobile_plugin::<serde_json::Value>("alert", info).map(|_| ()).map_err(|e| e.to_string())
        }
        #[cfg(not(target_os = "android"))]
        {
            let _ = info;
            Ok(())
        }
    }

    /// Register what happens when the user taps "Stop server" in the notification. The callback runs on a
    /// plugin thread, independent of the WebView (which Android may have suspended).
    pub fn on_stop_requested(&self, callback: impl Fn() + Send + Sync + 'static) -> Result<(), String> {
        #[cfg(target_os = "android")]
        {
            #[derive(Serialize)]
            struct Args {
                channel: tauri::ipc::Channel<serde_json::Value>,
            }
            let channel = tauri::ipc::Channel::new(move |_| {
                callback();
                Ok(())
            });
            self.handle
                .run_mobile_plugin::<serde_json::Value>("setStopHandler", Args { channel })
                .map(|_| ())
                .map_err(|e| e.to_string())
        }
        #[cfg(not(target_os = "android"))]
        {
            let _ = callback;
            Ok(())
        }
    }

    pub fn stop(&self) -> Result<(), String> {
        #[cfg(target_os = "android")]
        {
            self.handle.run_mobile_plugin::<serde_json::Value>("stop", ()).map(|_| ()).map_err(|e| e.to_string())
        }
        #[cfg(not(target_os = "android"))]
        {
            Ok(())
        }
    }

    /// Whether this app may install packages ("install unknown apps"). When it may not, the system settings screen
    /// for that opens and `false` comes back. Always `true` off Android.
    pub fn install_allowed(&self) -> Result<bool, String> {
        #[cfg(target_os = "android")]
        {
            #[derive(Deserialize)]
            struct Res {
                allowed: bool,
            }
            self.handle.run_mobile_plugin::<Res>("installAllowed", ()).map(|r| r.allowed).map_err(|e| e.to_string())
        }
        #[cfg(not(target_os = "android"))]
        {
            Ok(true)
        }
    }

    /// The folder for downloaded update APKs on Android: `<filesDir>/updates`, the folder the app's FileProvider
    /// shares with the system installer (Tauri's app data folder is elsewhere). `None` off Android.
    pub fn updates_dir(&self) -> Option<String> {
        #[cfg(target_os = "android")]
        {
            #[derive(Deserialize)]
            struct Res {
                path: String,
            }
            self.handle.run_mobile_plugin::<Res>("updatesDir", ()).map(|r| r.path).ok()
        }
        #[cfg(not(target_os = "android"))]
        {
            None
        }
    }

    /// Hand a downloaded APK (under the app's `updates` folder) to the system package installer. The user
    /// confirms there; the app is replaced and closed by it. Fails off Android.
    pub fn install_apk(&self, path: &str) -> Result<(), String> {
        #[cfg(target_os = "android")]
        {
            #[derive(Serialize)]
            struct Args<'a> {
                path: &'a str,
            }
            self.handle
                .run_mobile_plugin::<serde_json::Value>("installApk", Args { path })
                .map(|_| ())
                .map_err(|e| e.to_string())
        }
        #[cfg(not(target_os = "android"))]
        {
            let _ = path;
            Err("not android".into())
        }
    }

    /// Ask for the Android 13+ notification permission (no-op elsewhere).
    pub fn request_notification_permission(&self) -> Result<(), String> {
        #[cfg(target_os = "android")]
        {
            self.handle
                .run_mobile_plugin::<serde_json::Value>("requestNotificationPermission", ())
                .map(|_| ())
                .map_err(|e| e.to_string())
        }
        #[cfg(not(target_os = "android"))]
        {
            Ok(())
        }
    }
}

pub trait HostServiceExt<R: Runtime> {
    fn hostservice(&self) -> &HostService<R>;
}

impl<R: Runtime, T: Manager<R>> HostServiceExt<R> for T {
    fn hostservice(&self) -> &HostService<R> {
        self.state::<HostService<R>>().inner()
    }
}

pub fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("hostservice")
        .setup(|app, api| {
            #[cfg(target_os = "android")]
            let handle = api.register_android_plugin(PLUGIN_IDENTIFIER, "HostServicePlugin")?;
            #[cfg(target_os = "android")]
            app.manage(HostService::<R> { handle });
            #[cfg(not(target_os = "android"))]
            {
                let _ = api;
                app.manage(HostService::<R> { _marker: std::marker::PhantomData });
            }
            Ok(())
        })
        .build()
}
