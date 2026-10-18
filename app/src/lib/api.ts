import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import type {
  AppInfo, ListKind, NetAddr, NewWorld, Poll, RunOptions, Settings, SplashState, World
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
  sendCommand: (line: string) => invoke<void>('send_command', { line }),
  getList: (worldId: string, kind: ListKind) => invoke<string[]>('get_list', { worldId, kind }),
  setList: (worldId: string, kind: ListKind, names: string[]) =>
    invoke<void>('set_list', { worldId, kind, names }),
  networkInfo: (port: number) => invoke<NetAddr[]>('network_info', { port }),
  getSettings: () => invoke<Settings>('get_settings'),
  setSettings: (settings: Settings) => invoke<void>('set_settings', { settings })
};
