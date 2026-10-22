import { api } from './api';
import { errText, t } from './i18n.svelte';
import { en, type Key } from './messages';
import { store } from './store.svelte';
import type { UpdateInfo, UpdatePrefs, UpdateProgress } from './types';

/**
 * In-app updates of the Blockheads Server app (the release feed, download and installer hand-off live in
 * src-tauri/src/update.rs). Flow: check → available → download → ready → restart (now, when the server is empty,
 * or in 5 minutes with a chat warning) → installing → the app restarts → done. Like the Blockheads PC launcher, a
 * check only tells: nothing is downloaded or installed until the user asks for it. A failed check never blocks the app.
 */
export type UpdateStage = 'idle' | 'checking' | 'uptodate' | 'available' | 'downloading' | 'ready' | 'installing' | 'done' | 'error';
export type RestartWhen = 'empty' | 'warn' | 'now';

const HOUR = 3600 * 1000;
const DAY = 24 * HOUR;
/** "In 5 minutes, with a warning", and a last warning one minute before. */
export const WARN_MS = 5 * 60 * 1000;
const LAST_WARN_MS = 60 * 1000;
/** How long the "installing" steps stay up before the app quits and the installer takes over. */
export const QUIT_DELAY = 1500;

export const DEFAULT_PREFS: UpdatePrefs = { autoCheck: true, channel: 'stable', lastChecked: 0, lastOk: 0, seenVersion: '' };

export class Updates {
  stage = $state<UpdateStage>('idle');
  info = $state<UpdateInfo | null>(null);
  progress = $state<UpdateProgress | null>(null);
  /** What failed: a check (red dot only), a download or the install. */
  failed = $state<'check' | 'download' | 'install' | null>(null);
  error = $state('');
  /** The popup is open. Hide / Later / close only close it; the state stays on the icon and the title-bar chip. */
  open = $state(false);
  when = $state<RestartWhen>('empty');
  scheduled = $state<{ kind: 'empty' | 'warn'; at: number } | null>(null);
  /** Install steps done (see UpdateDialog). */
  step = $state(0);
  /** After an update restart: the version that is running now and how long the server was down (s). */
  done = $state<{ version: string; downtime: number | null } | null>(null);
  private started = false;
  private timers: ReturnType<typeof setInterval>[] = [];
  private poller: ReturnType<typeof setInterval> | null = null;
  private lastWarned = false;

  /** The saved preferences with defaults filled in (read only: change them with `setPrefs`). */
  get prefs(): UpdatePrefs {
    return { ...DEFAULT_PREFS, ...store.settings.updates };
  }

  private edit(patch: Partial<UpdatePrefs>) {
    store.settings.updates = { ...this.prefs, ...patch };
  }

  /** Android hands the APK to the system installer through the browser; nothing installs by itself there. */
  get mobile() {
    return store.info?.platform === 'android';
  }

  /** The dot / chip has something to show. */
  get active() {
    const s = this.stage;
    return s === 'available' || s === 'downloading' || s === 'ready' || s === 'installing' || (s === 'error' && this.failed !== 'check');
  }

  get percent() {
    const p = this.progress;
    return p && p.total > 0 ? Math.min(100, Math.floor((p.done * 100) / p.total)) : 0;
  }

  /** After the main UI is up: finish an update restart, then the automatic check and the timers. */
  async start() {
    if (this.started) return;
    this.started = true;
    const resumed = await this.resume();
    if (!resumed) await this.autoCheck();
    this.timers.push(setInterval(() => void this.autoCheck(), HOUR));
    this.timers.push(setInterval(() => void this.tick(), 1000));
  }

  stop() {
    this.timers.forEach(clearInterval);
    this.timers = [];
    this.stopPolling();
    this.started = false;
  }

  /** The app restarted for an update: say how it went and start the server again if it was running. */
  async resume(): Promise<boolean> {
    const note = await api.updateResume().catch(() => null);
    if (!note) return false;
    const now = store.info?.version ?? '';
    if (note.wasRunning && !store.active) await store.start();
    if (note.version === now) {
      this.done = { version: now, downtime: note.wasRunning ? Math.max(1, Math.round((Date.now() - note.at) / 1000)) : null };
      this.stage = 'done';
    } else {
      this.stage = 'error';
      this.failed = 'install';
      this.error = t('upd.err.notInstalled', { v: note.version });
    }
    this.open = true;
    return true;
  }

  async autoCheck() {
    if (!this.prefs.autoCheck || Date.now() - this.prefs.lastChecked < DAY) return;
    await this.check(true);
  }

  private busy() {
    const s = this.stage;
    return s === 'checking' || s === 'downloading' || s === 'ready' || s === 'installing';
  }

  /** `auto`: the daily check. The popup then opens by itself only once per new version. */
  async check(auto = false) {
    if (this.busy()) return;
    this.stage = 'checking';
    this.failed = null;
    this.error = '';
    const prefs = this.prefs;
    try {
      const info = await api.updateCheck(prefs.channel === 'beta');
      const now = Date.now();
      this.edit({ lastChecked: now, lastOk: now });
      this.info = info;
      this.stage = info ? 'available' : 'uptodate';
      if (info && auto && info.version !== prefs.seenVersion) {
        this.edit({ seenVersion: info.version });
        this.open = true;
      }
    } catch (e) {
      this.edit({ lastChecked: Date.now() });
      this.stage = 'error';
      this.failed = 'check';
      this.error = updErr(e);
    }
    // Quietly: a background check must not replace an error the user is looking at.
    await api.setSettings($state.snapshot(store.settings)).catch(() => {});
  }

  /** The update icon: open the popup; with nothing to show yet, check first. */
  async openPopup() {
    this.open = true;
    if (!this.active && this.stage !== 'checking' && this.stage !== 'done') await this.check();
  }

  close() {
    this.open = false;
    if (this.stage === 'uptodate' || this.stage === 'done' || (this.stage === 'error' && this.failed === 'check')) this.stage = 'idle';
  }

  /** Download (desktop) or open the download in the browser (Android, or no verifiable asset). */
  async download() {
    const info = this.info;
    if (!info) return;
    if (info.mode === 'browser') {
      if (!(await store.guard(() => api.openUrl(info.asset?.url ?? info.page)))) return;
      this.open = false;
      return;
    }
    this.stage = 'downloading';
    this.failed = null;
    this.progress = null;
    try {
      await api.updateDownload();
    } catch (e) {
      this.fail('download', e);
      return;
    }
    this.stopPolling();
    this.poller = setInterval(() => void this.pollProgress(), 400);
    await this.pollProgress();
  }

  async pollProgress() {
    let p: UpdateProgress;
    try {
      p = await api.updateProgress();
    } catch {
      return;
    }
    if (this.stage !== 'downloading') return this.stopPolling();
    this.progress = p;
    if (p.stage === 'ready') {
      this.stopPolling();
      this.stage = 'ready';
    } else if (p.stage === 'error') {
      this.fail('download', p.error ?? '');
    } else if (p.stage === 'cancelled') {
      this.stopPolling();
      this.stage = 'available';
    }
  }

  private stopPolling() {
    if (this.poller) clearInterval(this.poller);
    this.poller = null;
  }

  private fail(kind: 'download' | 'install', e: unknown) {
    this.stopPolling();
    this.stage = 'error';
    this.failed = kind;
    this.error = updErr(e);
  }

  async cancel() {
    await api.updateCancel().catch(() => {});
    this.stopPolling();
    this.stage = 'available';
  }

  /** The restart choice of the ready popup. With the server stopped or empty, "now" and "when empty" are the same. */
  async schedule(when: RestartWhen) {
    const empty = !store.active || store.players.length === 0;
    if (when === 'now' || (when === 'empty' && empty)) return this.install();
    this.scheduled = { kind: when, at: Date.now() + (when === 'warn' ? WARN_MS : 0) };
    this.lastWarned = false;
    this.open = false;
    if (when === 'warn') await this.say(t('upd.chat.warn'));
  }

  unschedule() {
    this.scheduled = null;
  }

  private async say(text: string) {
    await api.sendCommand(text).catch(() => {});
  }

  /** Once a second: run a scheduled restart when its time has come. */
  async tick() {
    const s = this.scheduled;
    if (!s || this.stage !== 'ready') return;
    if (s.kind === 'empty') {
      if (!store.active || store.players.length === 0) await this.install();
      return;
    }
    const left = s.at - Date.now();
    if (left <= 0) await this.install();
    else if (left <= LAST_WARN_MS && !this.lastWarned) {
      this.lastWarned = true;
      if (store.active) await this.say(t('upd.chat.last'));
    }
  }

  /**
   * Save and stop the server, back up the world and install. In place (updater plugin) the app then restarts into
   * the new version; otherwise the installer takes over and the app quits.
   */
  async install() {
    this.scheduled = null;
    const wasActive = store.active;
    this.stage = 'installing';
    this.step = 0;
    this.open = true;
    try {
      await api.updateInstall(true);
    } catch (e) {
      this.fail('install', e);
      // The server was stopped for the install: bring it back.
      await store.pollOnce();
      if (wasActive && !store.active) await store.start();
      return;
    }
    this.step = 3;
    await new Promise((r) => setTimeout(r, QUIT_DELAY));
    await (this.info?.mode === 'plugin' ? api.appRestart() : api.appQuit()).catch(() => {});
  }

  /** Try again after a failure: a failed check checks again, a failed download downloads again. */
  async retry() {
    if (this.failed === 'check' || !this.info) return this.check();
    if (this.failed === 'install') {
      this.stage = 'ready';
      return this.install();
    }
    return this.download();
  }

  async setPrefs(patch: Partial<UpdatePrefs>) {
    this.edit(patch);
    await store.saveSettings();
  }

  openNotes() {
    const page = this.info?.page;
    if (page) void store.guard(() => api.openUrl(page));
  }
}

/** A backend error code (`offline`, `checksum`, …) as a message; anything else as it came. */
export function updErr(e: unknown): string {
  const k = `upd.err.${String(e)}` as Key;
  return k in en ? t(k) : errText(e);
}

export const updates = new Updates();
