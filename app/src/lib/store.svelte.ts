import { api } from './api';
import { errText, i18n, setLang, t } from './i18n.svelte';
import { toast } from './ui/toast.svelte';
import type { AppInfo, BootPhase, ListKind, LogEntry, ModsPrefs, NewWorld, Player, RunOptions, RunState, Screen, Settings, Stats, World } from './types';
import { classify, storage } from './util';

/** After this long the splash says that opening the data folder is slow. */
const SLOW_MS = 4000;
const MAX_LINES = 1500;
const MAX_HISTORY = 30;
const MAX_KNOWN = 200;
const KNOWN_KEY = 'bh.known';
/** JVM mods are unsandboxed code: off until the owner turns them on. */
export const DEFAULT_MODS: ModsPrefs = { jvm: false, javaHome: '' };
const DEFAULT_RUN: RunOptions = { worldId: '', port: 15151, maxPlayers: 0, password: '', saveDelay: 10 };
const KINDS: ListKind[] = ['admin', 'mod', 'white', 'black'];

export interface KnownPlayer {
  name: string;
  lastSeen: number;
}

/** Samples kept for the dashboard's performance graphs (one per second: a minute). */
export const STATS_KEEP = 60;

const emptyLists = (): Record<ListKind, string[]> => ({ admin: [], mod: [], white: [], black: [] });

/** Shared UI state; one instance for the whole app. */
export class AppStore {
  worlds = $state<World[]>([]);
  settings = $state<Settings>({ language: 'en', run: { ...DEFAULT_RUN } });
  info = $state<AppInfo | null>(null);
  state = $state<RunState>('stopped');
  detail = $state<string | null>(null);
  runningWorldId = $state<string | null>(null);
  players = $state<Player[]>([]);
  log = $state<LogEntry[]>([]);
  startedAt = $state<number | null>(null);
  /** Wall clock, refreshed on every poll; drives uptime and "joined 6 min ago". */
  now = $state(Date.now());
  error = $state('');
  ready = $state(false);
  /** Start-up progress: the splash shows it until `done`. */
  boot = $state<BootPhase>('loading');
  bootError = $state('');
  private slow: ReturnType<typeof setTimeout> | undefined;
  /** The backend did not answer the last poll. */
  offline = $state(false);
  screen = $state<Screen>('dashboard');
  lists = $state<Record<ListKind, string[]>>(emptyLists());
  /** The selected world's welcome message (empty: none). */
  welcome = $state('');
  history = $state<string[]>([]);
  known = $state<KnownPlayer[]>([]);
  /** Latest CPU / memory sample and the last STATS_KEEP of them (one per poll), oldest first. */
  stats = $state<Stats | null>(null);
  statsHistory = $state<Stats[]>([]);
  /** Bumped when the mods may have changed state (a `[mods]` log line, the server started or stopped). */
  modsDirty = $state(0);
  /** Restart as soon as nobody is online (mods waiting for a restart). */
  restartWhenEmpty = $state(false);
  /** A search for the console to start with (set by "Open in console" on a mod). */
  consoleQuery = $state('');
  private next = 0;
  private seq = 0;
  private timer: ReturnType<typeof setInterval> | null = null;
  private online = '';

  get active() {
    return this.state === 'starting' || this.state === 'running' || this.state === 'stopping';
  }

  /** The world that is loaded now (or, when stopped, the selected one). */
  get currentWorld(): World | undefined {
    return this.worlds.find((w) => w.id === (this.runningWorldId ?? this.settings.run.worldId));
  }

  get uptimeMs(): number | null {
    return this.state === 'running' && this.startedAt ? Math.max(0, this.now - this.startedAt) : null;
  }

  navigate(screen: Screen) {
    this.screen = screen;
  }

  async init() {
    this.boot = 'loading';
    this.bootError = '';
    clearTimeout(this.slow);
    // Every way out of the loading phase clears this timer first, so it only ever fires while loading.
    this.slow = setTimeout(() => (this.boot = 'slow'), SLOW_MS);
    this.info = await api.appInfo().catch(() => null);
    try {
      await api.boot();
    } catch (e) {
      clearTimeout(this.slow);
      this.bootError = errText(e);
      this.boot = 'error';
      return;
    }
    this.known = storage((s) => JSON.parse(s.getItem(KNOWN_KEY) ?? '[]') as KnownPlayer[], []);
    try {
      const settings = await api.getSettings();
      this.info ??= await api.appInfo();
      this.settings = { ...settings, run: { ...DEFAULT_RUN, ...settings.run }, mods: { ...DEFAULT_MODS, ...settings.mods } };
      setLang(settings.language);
      // First launch (no language saved yet): keep the one picked from the OS locale, so the Android
      // notification that Rust builds speaks it too.
      if (settings.language !== i18n.lang) {
        this.settings.language = i18n.lang;
        await api.setSettings($state.snapshot(this.settings));
      }
      await this.ensureWorld(await this.refreshWorlds());
      await this.loadLists();
      await this.pollOnce();
    } catch (e) {
      this.error = errText(e);
    }
    clearTimeout(this.slow);
    this.boot = 'done';
    this.ready = true;
    this.startPolling();
  }

  startPolling(ms = 700) {
    this.stopPolling();
    this.timer = setInterval(() => void this.pollOnce(), ms);
  }

  stopPolling() {
    if (this.timer) clearInterval(this.timer);
    this.timer = null;
  }

  async pollOnce() {
    try {
      const p = await api.poll(this.next);
      this.offline = false;
      this.now = Date.now();
      if (p.state !== this.state || p.lines.some((l) => l.includes('[mods'))) this.modsDirty++;
      this.state = p.state;
      this.detail = p.detail;
      this.runningWorldId = p.worldId;
      this.startedAt = p.startedAt;
      this.players = p.players;
      this.trackKnown(p.players);
      if (p.lines.length) {
        const fresh = p.lines.map((text, i) => ({ id: ++this.seq, at: p.times[i] ?? this.now, level: classify(text), text }));
        this.log = [...this.log, ...fresh].slice(-MAX_LINES);
      }
      this.next = p.next;
      if (this.restartWhenEmpty && p.state === 'running' && p.players.length === 0) {
        this.restartWhenEmpty = false;
        void this.restart();
      }
    } catch {
      this.offline = true;
    }
    await this.sampleStats();
  }

  async sampleStats() {
    try {
      const s = await api.stats();
      this.stats = s;
      this.statsHistory = [...this.statsHistory, s].slice(-STATS_KEEP);
    } catch {
      // no sample this time; the card keeps the last one
    }
  }

  clearLog() {
    this.log = [];
  }

  private addLocalLine(level: LogEntry['level'], text: string) {
    this.log = [...this.log, { id: ++this.seq, at: Date.now(), level, text }].slice(-MAX_LINES);
  }

  // ---- known players (remembered on this device only) ----

  private trackKnown(players: Player[]) {
    for (const p of players) {
      const k = this.known.find((x) => x.name === p.name);
      if (k) k.lastSeen = this.now;
      else this.known.push({ name: p.name, lastSeen: this.now });
    }
    const key = players.map((p) => p.name).sort().join('\n');
    if (key === this.online) return;
    this.online = key;
    this.known = [...this.known].sort((a, b) => b.lastSeen - a.lastSeen).slice(0, MAX_KNOWN);
    storage((s) => s.setItem(KNOWN_KEY, JSON.stringify($state.snapshot(this.known))), undefined);
  }

  // ---- worlds ----

  /** Reload the world list; a selection that no longer exists falls back to the first world. True if it changed. */
  async refreshWorlds(): Promise<boolean> {
    this.worlds = await api.listWorlds();
    const r = this.settings.run;
    if (this.worlds.some((w) => w.id === r.worldId)) return false;
    r.worldId = this.worlds[0]?.id ?? '';
    return true;
  }

  /** There is always a loaded world: with none, one is created and loaded; a changed selection is saved. */
  async ensureWorld(changed: boolean) {
    if (!this.worlds.length) {
      const w = await api.createWorld({ name: t('worlds.defaultName'), size: 'normal', expert: false });
      await this.refreshWorlds();
      this.settings.run.worldId = w.id;
      toast(t('worlds.autoCreated', { name: w.name }));
      changed = true;
    }
    if (changed) await api.setSettings($state.snapshot(this.settings));
  }

  /** Run an action, surfacing a backend error code as a message. Returns true on success. */
  async guard(fn: () => Promise<unknown>): Promise<boolean> {
    this.error = '';
    try {
      await fn();
      return true;
    } catch (e) {
      this.error = errText(e);
      return false;
    }
  }

  async saveSettings() {
    await this.guard(() => api.setSettings($state.snapshot(this.settings)));
  }

  async setLanguage(code: string) {
    this.settings.language = code;
    setLang(code);
    await this.saveSettings();
  }

  /** Create a world; with `loadNow` it becomes the current one (a running server restarts on it). */
  async createWorld(form: NewWorld, loadNow: boolean) {
    let id = '';
    const ok = await this.guard(async () => {
      id = (await api.createWorld(form)).id;
      await this.refreshWorlds();
    });
    if (!ok) return false;
    if (loadNow) await this.loadWorld(id);
    else await this.saveSettings();
    return true;
  }

  async deleteWorld(id: string) {
    const ok = await this.guard(async () => {
      await api.deleteWorld(id);
      await this.ensureWorld(await this.refreshWorlds());
      await this.loadLists();
    });
    if (ok) toast(t('worlds.deleted'));
  }

  /** Make a world the current one; a running server is restarted on it. */
  async loadWorld(id: string) {
    this.settings.run.worldId = id;
    await this.saveSettings();
    await this.loadLists();
    if (this.active) await this.restart();
  }

  // ---- run ----

  async start() {
    const ok = await this.guard(async () => {
      await api.setSettings($state.snapshot(this.settings));
      await api.startServer($state.snapshot(this.settings.run));
    });
    if (ok) await this.pollOnce();
    return ok;
  }

  async stop() {
    this.state = 'stopping';
    await this.guard(() => api.stopServer());
    await this.pollOnce();
  }

  async restart() {
    await this.stop();
    return this.start();
  }

  /** Persist edited run options; a running server is restarted so they take effect. */
  async applyRun(run: RunOptions) {
    this.settings.run = { ...run };
    if (this.active) return this.restart();
    await this.saveSettings();
    return !this.error;
  }

  async sendCommand(raw: string) {
    const line = raw.trim();
    if (!line) return false;
    this.history = [line, ...this.history.filter((h) => h !== line)].slice(0, MAX_HISTORY);
    this.addLocalLine('cmd', line);
    return this.guard(() => api.sendCommand(line));
  }

  // ---- access lists ----

  async loadLists() {
    const id = this.settings.run.worldId;
    if (!id) {
      this.lists = emptyLists();
      this.welcome = '';
      return;
    }
    const entries = await Promise.all(KINDS.map(async (k) => [k, await api.getList(id, k).catch(() => [] as string[])] as const));
    this.lists = Object.fromEntries(entries) as Record<ListKind, string[]>;
    this.welcome = await api.getWelcome(id).catch(() => '');
  }

  /** Set the selected world's welcome message (blank: none); a running world gets it at once. */
  async saveWelcome(text: string) {
    const ok = await this.guard(() => api.setWelcome(this.settings.run.worldId, text));
    if (ok) this.welcome = text.trim() ? text.trimEnd() : '';
    return ok;
  }

  async commitList(kind: ListKind, next: string[]) {
    const ok = await this.guard(() => api.setList(this.settings.run.worldId, kind, next));
    if (ok) {
      // The running server applies console commands asynchronously; show the intended list right away.
      this.lists[kind] = next;
      if (this.active) setTimeout(() => void this.loadLists(), 800);
    }
    return ok;
  }

  has(kind: ListKind, name: string) {
    return this.lists[kind].some((x) => x.toLowerCase() === name.toLowerCase());
  }

  async addName(kind: ListKind, name: string) {
    if (this.has(kind, name)) return false;
    return this.commitList(kind, [...this.lists[kind], name]);
  }

  async removeName(kind: ListKind, name: string) {
    return this.commitList(kind, this.lists[kind].filter((x) => x !== name));
  }

  roleOf(name: string): 'admin' | 'mod' | undefined {
    return this.has('admin', name) ? 'admin' : this.has('mod', name) ? 'mod' : undefined;
  }

  async kick(name: string) {
    if (await this.sendCommand(`/kick ${name}`)) toast(t('players.kicked', { name }));
  }

  /** Ban a name and, optionally, the address it joined from. */
  async ban(name: string, ip?: string) {
    const next = [...this.lists.black];
    for (const n of ip ? [name, ip] : [name]) if (!this.has('black', n)) next.push(n);
    if (await this.commitList('black', next)) toast(t('players.banned', { name }));
  }
}

export const store = new AppStore();
