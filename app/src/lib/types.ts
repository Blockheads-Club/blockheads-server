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
  /** Same name from any device (the name is the identity); missing in older settings = on. */
  sharedNames?: boolean;
}
export interface Settings {
  language: string;
  run: RunOptions;
  /** App update preferences (missing in settings saved before updates existed). */
  updates?: UpdatePrefs;
  /** Mod runtime options (missing in settings saved before they existed). */
  mods?: ModsPrefs;
}
export interface ModsPrefs {
  /** Run JVM (Java/Kotlin) mods: unsandboxed code, off by default. */
  jvm: boolean;
  /** The Java installation to use; empty = JAVA_HOME, then `java` on the PATH. */
  javaHome: string;
}
export type UpdateChannel = 'stable' | 'beta';
export interface UpdatePrefs {
  autoCheck: boolean;
  channel: UpdateChannel;
  /** ms since the Unix epoch, 0 = never; `lastOk` is the last check that worked. */
  lastChecked: number;
  lastOk: number;
  /** The popup opened by itself for this version already. */
  seenVersion: string;
}
/**
 * plugin: Tauri updater (signed, in place, relaunch); installer: download, verify, run the setup; replace: swap the
 * AppImage; browser: open the download page.
 */
export type UpdateMode = 'plugin' | 'installer' | 'replace' | 'browser';
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
  /** This build can run JVM mods (cargo feature `jvm`). */
  jvmBuilt?: boolean;
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
export type Screen = 'dashboard' | 'worlds' | 'console' | 'players' | 'mods' | 'access' | 'settings' | 'more';
export type ThemeChoice = 'system' | 'light' | 'dark';

// ---- mods (see src-tauri/src/mods.rs) ----

/** An error with its own message: `code` picks the text, the rest fills it in. */
export interface ModError {
  code: string;
  detail?: string;
  file?: string;
  expected?: string;
  got?: string;
}
export type ModStatus = 'running' | 'off' | 'failed' | 'budget' | 'dependency' | 'permissions' | 'ready' | 'restart' | 'starting';
export interface ModFault {
  at: number;
  kind: 'error' | 'slow' | 'start';
  text: string;
  ms?: number;
}
export interface ModLine {
  at: number;
  level: 'info' | 'warn' | 'error';
  text: string;
}
export interface ModDep {
  id: string;
  req: string;
  installed: string | null;
  ok: boolean;
  soft: boolean;
}
export interface ModSummary {
  id: string;
  name: Record<string, string>;
  description: Record<string, string>;
  version: string;
  authors: string[];
  license: string;
  homepage: string | null;
  sides: { server: boolean; client: boolean; clientRequired: boolean };
  runtime: string | null;
  native: boolean;
  permissions: string[];
  granted: string[];
  missing: string[];
  enabled: boolean;
  status: ModStatus;
  detail: string | null;
  dependencies: ModDep[];
  file: string;
  packed: boolean;
  sha256: string | null;
  size: number;
  reviewed: boolean;
  update: string | null;
  faults: ModFault[];
  lines: ModLine[];
  slowestMs: number | null;
  storageBytes: number | null;
  /** Settings the admin can change on the mod's Settings tab. */
  settings: number;
}
export interface ModChange {
  kind: 'installed' | 'removed' | 'updated' | 'enabled' | 'disabled' | 'permissions';
  id: string;
  version: string;
}
export interface ModsOverview {
  dir: string;
  size: number;
  mods: ModSummary[];
  problems: { file: string; message: string }[];
  clientMods: string[];
  changes: ModChange[];
  running: boolean;
  error: ModError | null;
  softMs: number;
  hardMs: number;
  hostLines: ModLine[];
}
export interface BrowseMod {
  id: string;
  name: string;
  description: string | null;
  owners: string[];
  repo: string;
  tags: string[];
  version: string;
  sides: string[];
  clientRequired: boolean;
  permissions: string[];
  native: boolean;
  depends: Record<string, string>;
  size: number | null;
  published: string;
  firstPublished: string;
  installed: string | null;
}
export interface Browse {
  generated: string;
  offline: boolean;
  error: ModError | null;
  mods: BrowseMod[];
  hidden: number;
}
export interface PlanItem {
  id: string;
  name: string;
  version: string;
  size: number | null;
  sides: string[];
  clientRequired: boolean;
  permissions: string[];
  native: boolean;
  neededBy: string | null;
  replaces: string | null;
  reviewed: boolean;
}
export interface Plan {
  items: PlanItem[];
  satisfied: { id: string; req: string; version: string }[];
  missing: { id: string; req: string; neededBy: string }[];
  totalBytes: number;
}
export interface Inspected {
  source: string;
  sha256: string;
  plan: Plan;
}
export interface InstallRequest {
  items: { id: string; version: string }[];
  grants: Record<string, string[]>;
  enable: boolean;
  local: boolean;
}
export interface Installed {
  ids: string[];
  restart: boolean;
}
export interface InstallProgress {
  step: 'download' | 'write' | 'done';
  done: number;
  total: number;
}

/** A JSON value as the mod settings commands send it. */
export type Json = null | boolean | number | string | Json[] | { [key: string]: Json };
export type SettingType = 'bool' | 'int' | 'float' | 'enum' | 'string' | 'roles' | 'matrix' | 'list';
/** One field of a mod's settings schema (`mod_settings_get`, keys as the contract spells them). */
export interface SettingField {
  key: string;
  type: SettingType;
  default: Json;
  label: Record<string, string>;
  help?: Record<string, string>;
  group?: string;
  scope: 'server' | 'client';
  restart: boolean;
  min?: number;
  max?: number;
  step?: number;
  values?: string[];
  max_length?: number;
  rows?: string[];
  cols?: string[];
  cell?: 'bool' | 'roles';
  /** List item fields: sub-key -> field without its key. */
  item?: Record<string, Omit<SettingField, 'key'>>;
  max_items?: number;
}
export interface ModSettings {
  fields: SettingField[];
  values: Record<string, Json>;
}
