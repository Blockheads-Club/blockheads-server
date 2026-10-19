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
/** CPU and memory use of the app process (the server runs in it). */
export interface Stats {
  /** Share of the whole machine's CPU, 0-100. */
  cpu: number;
  cores: number;
  /** Resident memory, bytes. */
  mem: number;
  memTotal: number;
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
