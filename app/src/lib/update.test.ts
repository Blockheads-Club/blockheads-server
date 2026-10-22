import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const mock = vi.hoisted(() => ({ api: {} as Record<string, any> }));
vi.mock('./api', () => ({ api: new Proxy({}, { get: (_t, k: string) => mock.api[k] }) }));

import { setLang, t } from './i18n.svelte';
import { W1, makeApi, makePoll, makeUpdate } from './mock-api';
import { store } from './store.svelte';
import { DEFAULT_PREFS, QUIT_DELAY, Updates, WARN_MS, updErr, updates } from './update.svelte';

const player = { name: 'amy', ip: '1.1.1.1', id: 'x', joinedAt: 1 };
const progress = (over: Record<string, unknown> = {}) => ({ stage: 'downloading', done: 10, total: 40, bytesPerS: 5, etaS: 6, error: null, ...over });

function fresh(over: Record<string, unknown> = {}) {
  mock.api = makeApi({ updateCheck: vi.fn(async () => makeUpdate()), ...over });
  setLang('en');
  store.info = { version: '9.9.9', dataDir: '/d', platform: 'linux', nativeControls: false };
  store.settings = { language: 'en', run: { worldId: W1, port: 15151, maxPlayers: 0, password: '', saveDelay: 10 } };
  store.state = 'stopped';
  store.players = [];
  store.error = '';
  return new Updates();
}

beforeEach(() => vi.useRealTimers());
afterEach(() => vi.useRealTimers());

describe('preferences and derived state', () => {
  it('fills in defaults and saves changes', async () => {
    const u = fresh();
    expect(u.prefs).toEqual(DEFAULT_PREFS);
    await u.setPrefs({ channel: 'beta' });
    expect(store.settings.updates?.channel).toBe('beta');
    expect(mock.api.setSettings).toHaveBeenCalled();
    expect(u.prefs.autoCheck).toBe(true);
  });

  it('knows when there is something to show and how far the download is', () => {
    const u = fresh();
    expect(u.active).toBe(false);
    for (const s of ['available', 'downloading', 'ready', 'installing'] as const) {
      u.stage = s;
      expect(u.active).toBe(true);
    }
    u.stage = 'error';
    u.failed = 'check';
    expect(u.active).toBe(false);
    u.failed = 'install';
    expect(u.active).toBe(true);
    expect(u.percent).toBe(0);
    u.progress = progress({ total: 0 }) as never;
    expect(u.percent).toBe(0);
    u.progress = progress() as never;
    expect(u.percent).toBe(25);
    u.progress = progress({ done: 90 }) as never;
    expect(u.percent).toBe(100);
    u.when = 'warn';
    expect(u.when).toBe('warn');
    expect(u.mobile).toBe(false);
    store.info = { ...store.info!, platform: 'android' };
    expect(u.mobile).toBe(true);
    store.info = null;
    expect(u.mobile).toBe(false);
  });

  it('maps backend error codes', () => {
    fresh();
    expect(updErr('checksum')).toBe(t('upd.err.checksum'));
    expect(updErr('running')).toBe(t('err.running'));
    expect(updErr('weird')).toBe('weird');
    expect(updates).toBeInstanceOf(Updates);
  });
});

describe('start, resume and the daily check', () => {
  it('checks at start, opens the popup once per version and keeps checking hourly', async () => {
    vi.useFakeTimers();
    const u = fresh();
    await u.start();
    expect(u.stage).toBe('available');
    expect(u.open).toBe(true);
    expect(u.prefs.seenVersion).toBe('2.0.0');
    expect(mock.api.updateCheck).toHaveBeenCalledWith(false);
    await u.start();
    expect(mock.api.updateCheck).toHaveBeenCalledTimes(1);
    // A day later the hourly timer checks again; the popup does not open again for the same version.
    u.stage = 'idle';
    u.open = false;
    store.settings.updates = { ...u.prefs, lastChecked: Date.now() - 25 * 3600 * 1000 };
    await vi.advanceTimersByTimeAsync(3600 * 1000);
    expect(mock.api.updateCheck).toHaveBeenCalledTimes(2);
    expect(u.open).toBe(false);
    u.stop();
    u.stop();
  });

  it('skips the check when it is off or recent', async () => {
    const u = fresh();
    store.settings.updates = { ...DEFAULT_PREFS, autoCheck: false };
    await u.autoCheck();
    store.settings.updates = { ...DEFAULT_PREFS, lastChecked: Date.now() };
    await u.autoCheck();
    expect(mock.api.updateCheck).not.toHaveBeenCalled();
  });

  it('reports an update restart and starts the server again', async () => {
    const u = fresh({ updateResume: vi.fn(async () => ({ version: '9.9.9', from: '1.0.0', at: Date.now() - 24000, wasRunning: true, worldId: W1 })) });
    await u.start();
    u.stop();
    expect(mock.api.startServer).toHaveBeenCalled();
    expect(u.stage).toBe('done');
    expect(u.done?.downtime).toBeGreaterThanOrEqual(24);
    expect(u.open).toBe(true);
    expect(mock.api.updateCheck).not.toHaveBeenCalled();
  });

  it('reports a restart without a running server, an installer that did not finish, and no note', async () => {
    let u = fresh({ updateResume: vi.fn(async () => ({ version: '9.9.9', from: '1.0.0', at: 1, wasRunning: false, worldId: '' })) });
    expect(await u.resume()).toBe(true);
    expect(u.done).toEqual({ version: '9.9.9', downtime: null });
    expect(mock.api.startServer).not.toHaveBeenCalled();

    u = fresh({ updateResume: vi.fn(async () => ({ version: '3.0.0', from: '9.9.9', at: 1, wasRunning: false, worldId: '' })) });
    store.info = null;
    expect(await u.resume()).toBe(true);
    expect(u.stage).toBe('error');
    expect(u.failed).toBe('install');
    expect(u.error).toContain('3.0.0');

    // Already running again (started by hand): not started twice.
    u = fresh({ updateResume: vi.fn(async () => ({ version: '9.9.9', from: '1', at: 1, wasRunning: true, worldId: '' })) });
    store.state = 'running';
    await u.resume();
    expect(mock.api.startServer).not.toHaveBeenCalled();

    u = fresh({ updateResume: vi.fn(async () => { throw 'x'; }) });
    expect(await u.resume()).toBe(false);
  });
});

describe('check', () => {
  it('finds nothing, fails softly, and never runs twice at once', async () => {
    const u = fresh({ updateCheck: vi.fn(async () => null), setSettings: vi.fn(async () => { throw 'disk'; }) });
    store.error = 'shown';
    await u.check();
    expect(store.error).toBe('shown');
    expect(u.stage).toBe('uptodate');
    expect(u.prefs.lastOk).toBeGreaterThan(0);
    mock.api.updateCheck = vi.fn(async () => { throw 'offline'; });
    await u.check();
    expect(u.stage).toBe('error');
    expect(u.failed).toBe('check');
    expect(u.error).toBe(t('upd.err.offline'));
    for (const s of ['checking', 'downloading', 'ready', 'installing'] as const) {
      u.stage = s;
      await u.check();
      expect(u.stage).toBe(s);
    }
    expect(mock.api.updateCheck).toHaveBeenCalledTimes(1);
  });

  it('uses the test channel and installs by itself only where it can', async () => {
    const u = fresh();
    store.settings.updates = { ...DEFAULT_PREFS, channel: 'beta', installWhenEmpty: true, seenVersion: '2.0.0' };
    mock.api.updateProgress = vi.fn(async () => progress({ stage: 'ready' }));
    await u.check(true);
    expect(mock.api.updateCheck).toHaveBeenCalledWith(true);
    expect(u.open).toBe(true); // the install itself opens the popup
    expect(mock.api.updateDownload).toHaveBeenCalled();
    expect(mock.api.updateInstall).toHaveBeenCalled();
    u.stop();

    const browser = fresh({ updateCheck: vi.fn(async () => makeUpdate({ mode: 'browser' })) });
    store.settings.updates = { ...DEFAULT_PREFS, installWhenEmpty: true, seenVersion: '2.0.0' };
    await browser.check(true);
    expect(browser.open).toBe(false);
    expect(mock.api.updateDownload).not.toHaveBeenCalled();

    const phone = fresh();
    store.info = { ...store.info!, platform: 'android' };
    store.settings.updates = { ...DEFAULT_PREFS, installWhenEmpty: true };
    await phone.check(true);
    expect(mock.api.updateDownload).not.toHaveBeenCalled();
  });

  it('opens the popup from the icon and closes it again', async () => {
    const u = fresh({ updateCheck: vi.fn(async () => null) });
    await u.openPopup();
    expect(u.open).toBe(true);
    expect(u.stage).toBe('uptodate');
    u.close();
    expect(u.stage).toBe('idle');
    for (const s of ['available', 'checking', 'done'] as const) {
      u.stage = s;
      await u.openPopup();
    }
    expect(mock.api.updateCheck).toHaveBeenCalledTimes(1);
    u.close();
    expect(u.stage).toBe('idle');
    u.stage = 'error';
    u.failed = 'check';
    u.close();
    expect(u.stage).toBe('idle');
    u.stage = 'available';
    u.close();
    expect(u.stage).toBe('available');
    expect(u.open).toBe(false);
  });
});

describe('download', () => {
  it('opens the browser for Android and unverifiable releases', async () => {
    const u = fresh();
    await u.download();
    expect(mock.api.updateDownload).not.toHaveBeenCalled();
    u.info = makeUpdate({ mode: 'browser' }) as never;
    u.open = true;
    await u.download();
    expect(mock.api.openUrl).toHaveBeenCalledWith(makeUpdate().asset.url);
    expect(u.open).toBe(false);
    u.info = makeUpdate({ mode: 'browser', asset: null }) as never;
    u.open = true;
    mock.api.openUrl = vi.fn(async () => { throw 'url'; });
    await u.download();
    expect(mock.api.openUrl).toHaveBeenCalledWith(makeUpdate().page);
    expect(u.open).toBe(true);
    expect(store.error).toBe('url');
  });

  it('follows the progress until ready, error or cancel', async () => {
    vi.useFakeTimers();
    const u = fresh();
    u.info = makeUpdate() as never;
    mock.api.updateProgress = vi.fn(async () => progress());
    await u.download();
    expect(u.stage).toBe('downloading');
    expect(u.progress?.done).toBe(10);
    mock.api.updateProgress = vi.fn(async () => { throw 'gone'; });
    await vi.advanceTimersByTimeAsync(400);
    expect(u.stage).toBe('downloading');
    mock.api.updateProgress = vi.fn(async () => progress({ stage: 'ready' }));
    await vi.advanceTimersByTimeAsync(400);
    expect(u.stage).toBe('ready');
    expect(mock.api.updateInstall).not.toHaveBeenCalled();

    mock.api.updateProgress = vi.fn(async () => progress());
    await u.download();
    mock.api.updateProgress = vi.fn(async () => progress({ stage: 'error', error: 'checksum' }));
    await vi.advanceTimersByTimeAsync(400);
    expect(u.stage).toBe('error');
    expect(u.error).toBe(t('upd.err.checksum'));
    mock.api.updateProgress = vi.fn(async () => progress({ stage: 'error', error: null }));
    await u.download();
    expect(u.error).toBe('');

    mock.api.updateProgress = vi.fn(async () => progress({ stage: 'cancelled' }));
    await u.download();
    expect(u.stage).toBe('available');
    // The poller stops once the stage moved on.
    await u.pollProgress();
    expect(u.stage).toBe('available');
  });

  it('reports a download that cannot start and can be cancelled', async () => {
    const u = fresh({ updateDownload: vi.fn(async () => { throw 'nothing'; }) });
    u.info = makeUpdate() as never;
    await u.download();
    expect(u.failed).toBe('download');
    expect(u.error).toBe(t('upd.err.nothing'));
    mock.api.updateCancel = vi.fn(async () => { throw 'x'; });
    u.stage = 'downloading';
    await u.cancel();
    expect(u.stage).toBe('available');
  });
});

describe('restart and install', () => {
  it('restarts now when nobody is online', async () => {
    vi.useFakeTimers();
    const u = fresh();
    u.info = makeUpdate() as never;
    u.stage = 'ready';
    const p = u.schedule('empty');
    await vi.advanceTimersByTimeAsync(0);
    expect(u.stage).toBe('installing');
    expect(u.step).toBe(3);
    expect(mock.api.updateInstall).toHaveBeenCalledWith(true);
    expect(mock.api.appQuit).not.toHaveBeenCalled();
    await vi.advanceTimersByTimeAsync(QUIT_DELAY);
    await p;
    expect(mock.api.appQuit).toHaveBeenCalled();
    mock.api.appQuit = vi.fn(async () => { throw 'x'; });
    const q = u.schedule('now');
    await vi.advanceTimersByTimeAsync(QUIT_DELAY);
    await q;
  });

  it('waits for the server to empty, or warns players and restarts after 5 minutes', async () => {
    vi.useFakeTimers();
    const u = fresh();
    u.stage = 'ready';
    store.state = 'running';
    store.players = [player];
    await u.tick();
    await u.schedule('empty');
    expect(u.scheduled?.kind).toBe('empty');
    expect(u.open).toBe(false);
    await u.tick();
    expect(mock.api.updateInstall).not.toHaveBeenCalled();
    u.stage = 'downloading';
    store.players = [];
    await u.tick();
    expect(mock.api.updateInstall).not.toHaveBeenCalled();
    u.stage = 'ready';
    void u.tick();
    await vi.advanceTimersByTimeAsync(0);
    expect(mock.api.updateInstall).toHaveBeenCalledTimes(1);

    const w = fresh({ sendCommand: vi.fn(async () => { throw 'x'; }) });
    w.stage = 'ready';
    store.state = 'running';
    store.players = [player];
    await w.schedule('warn');
    expect(mock.api.sendCommand).toHaveBeenCalledWith(t('upd.chat.warn'));
    await w.tick();
    vi.setSystemTime(Date.now() + WARN_MS - 30_000);
    await w.tick();
    await w.tick();
    expect(mock.api.sendCommand).toHaveBeenCalledTimes(2);
    expect(mock.api.sendCommand).toHaveBeenLastCalledWith(t('upd.chat.last'));
    vi.setSystemTime(Date.now() + 31_000);
    void w.tick();
    await vi.advanceTimersByTimeAsync(0);
    expect(mock.api.updateInstall).toHaveBeenCalled();
    w.unschedule();
    expect(w.scheduled).toBeNull();

    // The last warning is only sent while the server runs.
    const quiet = fresh();
    quiet.stage = 'ready';
    quiet.scheduled = { kind: 'warn', at: Date.now() + 30_000 };
    await quiet.tick();
    expect(mock.api.sendCommand).not.toHaveBeenCalled();
  });

  it('brings the server back when the install fails', async () => {
    const u = fresh({ updateInstall: vi.fn(async () => { throw 'launch'; }), poll: vi.fn(async () => makePoll({ state: 'stopped' })) });
    store.state = 'running';
    await u.install();
    expect(u.stage).toBe('error');
    expect(u.failed).toBe('install');
    expect(u.error).toBe(t('upd.err.launch'));
    expect(mock.api.startServer).toHaveBeenCalled();

    const idle = fresh({ updateInstall: vi.fn(async () => { throw 'backup'; }) });
    await idle.install();
    expect(mock.api.startServer).not.toHaveBeenCalled();
  });

  it('tries again what failed and opens the release page', async () => {
    const u = fresh({ updateInstall: vi.fn(async () => { throw 'launch'; }) });
    await u.retry();
    expect(mock.api.updateCheck).toHaveBeenCalledTimes(1);
    u.stage = 'error';
    u.failed = 'install';
    await u.retry();
    expect(mock.api.updateInstall).toHaveBeenCalled();
    u.failed = 'download';
    u.stage = 'error';
    mock.api.updateDownload = vi.fn(async () => { throw 'disk'; });
    await u.retry();
    expect(mock.api.updateDownload).toHaveBeenCalled();
    u.failed = 'check';
    await u.retry();
    expect(mock.api.updateCheck).toHaveBeenCalledTimes(2);
    u.openNotes();
    expect(mock.api.openUrl).toHaveBeenCalledWith(makeUpdate().page);
    u.info = null;
    u.openNotes();
    expect(mock.api.openUrl).toHaveBeenCalledTimes(1);
  });
});
