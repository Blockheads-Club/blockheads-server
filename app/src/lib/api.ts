import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { getCurrentWindow } from '@tauri-apps/api/window';
import type {
  AppInfo,
  Browse,
  Inspected,
  Installed,
  InstallProgress,
  InstallRequest,
  ListKind,
  ModSettings,
  ModsOverview,
  Json,
  NetAddr,
  NewWorld,
  Plan,
  Poll,
  RunOptions,
  Settings,
  SplashState,
  Stats,
  UpdateInfo,
  UpdatePending,
  UpdateProgress,
  World
} from './types';

/** Thin typed wrapper over the Tauri commands (see src-tauri/src/lib.rs). */
export const api = {
  appInfo: () => invoke<AppInfo>('app_info'),
  /** Start-up check of the data folder (rejects with the reason when it cannot be opened). */
  boot: () => invoke<void>('boot'),
  splashSet: (state: SplashState) => invoke<void>('splash_set', { state }),
  splashGet: () => invoke<SplashState | null>('splash_get'),
  splashRetry: () => invoke<void>('splash_retry'),
  /** Opens the folder picker; true when a folder was chosen (the main window then boots again). */
  chooseDataFolder: () => invoke<boolean>('choose_data_folder'),
  appQuit: () => invoke<void>('app_quit'),
  /** The boot work is done: show the main window, close the splash. */
  mainReady: () => invoke<void>('main_ready'),
  onSplashState: (cb: (s: SplashState) => void) => listen<SplashState>('splash-state', (e) => cb(e.payload)),
  onSplashRetry: (cb: () => void) => listen('splash-retry', () => cb()),
  listWorlds: () => invoke<World[]>('list_worlds'),
  createWorld: (world: NewWorld) => invoke<World>('create_world', { world }),
  deleteWorld: (id: string) => invoke<void>('delete_world', { id }),
  startServer: (options: RunOptions) => invoke<void>('start_server', { options }),
  stopServer: () => invoke<void>('stop_server'),
  poll: (since: number) => invoke<Poll>('poll', { since }),
  stats: () => invoke<Stats>('stats'),
  sendCommand: (line: string) => invoke<void>('send_command', { line }),
  getList: (worldId: string, kind: ListKind) => invoke<string[]>('get_list', { worldId, kind }),
  setList: (worldId: string, kind: ListKind, names: string[]) =>
    invoke<void>('set_list', { worldId, kind, names }),
  /** The world's welcome message (empty: none). */
  getWelcome: (worldId: string) => invoke<string>('get_welcome', { worldId }),
  setWelcome: (worldId: string, text: string) => invoke<void>('set_welcome', { worldId, text }),
  networkInfo: (port: number) => invoke<NetAddr[]>('network_info', { port }),
  getSettings: () => invoke<Settings>('get_settings'),
  setSettings: (settings: Settings) => invoke<void>('set_settings', { settings }),
  /** Newer release of the app, or null when up to date. `beta` also accepts test versions. */
  updateCheck: (beta: boolean) => invoke<UpdateInfo | null>('update_check', { beta }),
  updateDownload: () => invoke<void>('update_download'),
  updateProgress: () => invoke<UpdateProgress>('update_progress'),
  updateCancel: () => invoke<void>('update_cancel'),
  /** Stop the server, back up the world, start the installer; the app quits afterwards. */
  updateInstall: (backup: boolean) => invoke<void>('update_install', { backup }),
  updateResume: () => invoke<UpdatePending | null>('update_resume'),
  /** After an in-place update: start the new version. */
  appRestart: () => invoke<void>('app_restart'),
  /** Release pages and downloads of the server repository only. */
  openUrl: (url: string) => invoke<void>('open_url', { url }),
  // Mods: rejections carry a ModError ({ code, detail, ... }).
  modsOverview: () => invoke<ModsOverview>('mods_overview'),
  modsSetEnabled: (id: string, on: boolean) => invoke<void>('mods_set_enabled', { id, on }),
  modsSetGrant: (id: string, perm: string, on: boolean) => invoke<void>('mods_set_grant', { id, perm, on }),
  modsSetClientRequired: (id: string, on: boolean) => invoke<void>('mods_set_client_required', { id, on }),
  modsRemove: (file: string) => invoke<void>('mods_remove', { file }),
  modsClearData: (id: string) => invoke<void>('mods_clear_data', { id }),
  modsOpenFolder: (id?: string) => invoke<void>('mods_open_folder', { id: id ?? null }),
  modsBrowse: () => invoke<Browse>('mods_browse'),
  modsPlan: (id: string, version?: string) => invoke<Plan>('mods_plan', { id, version: version ?? null }),
  modsInspectUrl: (url: string) => invoke<Inspected>('mods_inspect_url', { url }),
  /** The file's bytes go as the raw request body; the name in a header. */
  modsInspectFile: (name: string, bytes: Uint8Array) =>
    invoke<Inspected>('mods_inspect_file', bytes, { headers: { 'x-file-name': encodeURIComponent(name) } }),
  modsInstall: (request: InstallRequest) => invoke<Installed>('mods_install', { request }),
  /** A mod's settings schema and values. */
  modSettingsGet: (id: string) => invoke<ModSettings>('mod_settings_get', { id }),
  /** Rejects with a ModError (`setting`: the value breaks the field's rules). */
  modSettingsSet: (id: string, key: string, value: Json) => invoke<void>('mod_settings_set', { id, key, value }),
  onModsProgress: (cb: (p: InstallProgress) => void) => listen<InstallProgress>('mods-progress', (e) => cb(e.payload))
};

/** The main window, for the custom title bar (desktop only). Async so a missing Tauri runtime rejects instead of throwing. */
export const win = {
  minimize: async () => getCurrentWindow().minimize(),
  toggleMaximize: async () => getCurrentWindow().toggleMaximize(),
  close: async () => getCurrentWindow().close(),
  isMaximized: async () => getCurrentWindow().isMaximized(),
  isFullscreen: async () => getCurrentWindow().isFullscreen(),
  onResized: async (cb: () => void) => getCurrentWindow().onResized(cb),
  onFocus: async (cb: (focused: boolean) => void) => getCurrentWindow().onFocusChanged((e) => cb(e.payload))
};
