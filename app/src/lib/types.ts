export interface World {
  id: string;
  name: string;
}
export type WorldSize = 'tiny' | 'small' | 'normal' | 'large' | 'huge';
export interface NewWorld {
  name: string;
  size: WorldSize;
  expert: boolean;
}
export interface RunOptions {
  worldId: string;
  port: number;
  maxPlayers: number;
  password: string;
  saveDelay: number;
}
export interface Settings {
  language: string;
  run: RunOptions;
  /** App update preferences (missing in settings saved before updates existed). */
  updates?: UpdatePrefs;
}
export type UpdateChannel = 'stable' | 'beta';
export interface UpdatePrefs {
  autoCheck: boolean;
  channel: UpdateChannel;
  installWhenEmpty: boolean;
  /** ms since the Unix epoch, 0 = never; `lastOk` is the last check that worked. */
  lastChecked: number;
  lastOk: number;
  /** The popup opened by itself for this version already. */
  seenVersion: string;
}
/** installer: download, verify, run the setup; replace: swap the AppImage; browser: open the download page. */
export type UpdateMode = 'installer' | 'replace' | 'browser';
export interface UpdateInfo {
  version: string;
  current: string;
  /** Release notes (Markdown). */
  notes: string;
  page: string;
  publishedAt: string | null;
  prerelease: boolean;
  asset: { name: string; url: string; size: number } | null;
  mode: UpdateMode;
}
export interface UpdateProgress {
  stage: 'idle' | 'downloading' | 'verifying' | 'ready' | 'error' | 'cancelled';
  done: number;
  total: number;
  bytesPerS: number;
  etaS: number;
  error: string | null;
}
/** Left by the app before it restarted for an update. */
export interface UpdatePending {
  version: string;
  from: string;
  at: number;
  wasRunning: boolean;
  worldId: string;
}
export interface Player {
  name: string;
  ip: string;
  id: string;
  /** Milliseconds since the Unix epoch. */
  joinedAt: number;
}
export type RunState = 'stopped' | 'starting' | 'running' | 'stopping' | 'failed';
export interface Poll {
  state: RunState;
  detail: string | null;
  worldId: string | null;
  port: number | null;
  lines: string[];
  /** Arrival time (ms since the Unix epoch) of each entry of `lines`. */
  times: number[];
  next: number;
  players: Player[];
  /** Start of the current run (ms since the Unix epoch), null while stopped. */
  startedAt: number | null;
}
/** What the game server uses. */
export interface Stats {
  /** Share of the machine's CPU, 0-100. */
  cpu: number;
  /** Memory the server holds, bytes. */
  mem: number;
  /** `cpu` is the server thread alone (Linux, Android); false: the whole app (Windows, macOS). */
  threadCpu: boolean;
}
export interface NetAddr {
  ip: string;
  endpoint: string;
}
export interface AppInfo {
  version: string;
  dataDir: string;
  platform: string;
  /** macOS draws the window buttons itself (overlay title bar). */
  nativeControls: boolean;
}
/** What the hidden main window tells the splash window while it boots. */
export interface SplashState {
  boot: 'loading' | 'slow' | 'error';
  error: string;
  version: string;
  lang: string;
}
export type BootPhase = 'loading' | 'slow' | 'error' | 'done';
export type ListKind = 'admin' | 'mod' | 'white' | 'black';

export type LogLevel = 'info' | 'chat' | 'join' | 'warn' | 'error' | 'cmd';
export interface LogEntry {
  id: number;
  at: number;
  level: LogLevel;
  text: string;
}
export type Screen = 'dashboard' | 'worlds' | 'console' | 'players' | 'access' | 'settings' | 'more';
export type ThemeChoice = 'system' | 'light' | 'dark';
