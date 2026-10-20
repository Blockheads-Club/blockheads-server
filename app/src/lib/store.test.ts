import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const mock = vi.hoisted(() => ({ api: {} as Record<string, any> }));
vi.mock('./api', () => ({ api: new Proxy({}, { get: (_t, k: string) => mock.api[k] }) }));

import { setLang } from './i18n.svelte';
import { W1, makeApi, makePoll } from './mock-api';
import { AppStore } from './store.svelte';
import { toasts } from './ui/toast.svelte';

const player = (name: string, id = name) => ({ name, ip: '1.1.1.1', id, joinedAt: 1 });

function fresh(over = {}) {
  mock.api = makeApi(over);
  setLang('en');
  toasts.items = [];
  localStorage.clear();
  const s = new AppStore();
  s.settings.run.worldId = W1;
  return s;
}

beforeEach(() => vi.useRealTimers());
afterEach(() => vi.useRealTimers());

describe('init and polling', () => {
  it('loads settings, worlds, lists and polls', async () => {
    mock.api = makeApi({
      listWorlds: vi.fn(async () => [{ id: 'z', name: 'Z' }]),
      getSettings: vi.fn(async () => ({ language: 'tr', run: { worldId: 'gone', port: 1, maxPlayers: 0, password: '', saveDelay: 10 } }))
    });
    const s = new AppStore();
    await s.init();
    s.stopPolling();
    expect(s.ready).toBe(true);
    expect(s.settings.run.worldId).toBe('z');
    expect(s.info?.version).toBe('9.9.9');
    expect(s.lists.admin).toEqual(['alice']);
    expect(s.currentWorld?.name).toBe('Z');
    expect(mock.api.setSettings).toHaveBeenCalled();
    setLang('en');
  });

  it('creates and loads a world when there is none', async () => {
    const list: { id: string; name: string }[] = [];
    const s = fresh({
      listWorlds: vi.fn(async () => list),
      createWorld: vi.fn(async (w: { name: string }) => { const x = { id: 'n1', name: w.name.toUpperCase() }; list.push(x); return x; })
    });
    await s.init();
    s.stopPolling();
    expect(mock.api.createWorld).toHaveBeenCalledWith({ name: 'My World', size: 'normal', expert: false });
    expect(s.settings.run.worldId).toBe('n1');
    expect(mock.api.setSettings).toHaveBeenCalled();
    expect(toasts.items.map((x) => x.message)).toContain('Created and loaded the world “MY WORLD”');
  });

  it('saves the language picked from the OS locale on first launch', async () => {
    const spy = vi.spyOn(navigator, 'languages', 'get').mockReturnValue(['ru-RU']);
    const s = fresh({
      listWorlds: vi.fn(async () => [{ id: W1, name: 'A' }]),
      getSettings: vi.fn(async () => ({ language: '', run: { worldId: W1, port: 1, maxPlayers: 0, password: '', saveDelay: 10 } }))
    });
    await s.init();
    s.stopPolling();
    expect(s.settings.language).toBe('ru');
    expect(mock.api.setSettings).toHaveBeenCalledWith(expect.objectContaining({ language: 'ru' }));
    spy.mockRestore();
    setLang('en');
  });

  it('leaves a valid selection as it is', async () => {
    const s = fresh({
      listWorlds: vi.fn(async () => [{ id: W1, name: 'A' }]),
      getSettings: vi.fn(async () => ({ language: 'en', run: { worldId: W1, port: 1, maxPlayers: 0, password: '', saveDelay: 10 } }))
    });
    await s.init();
    s.stopPolling();
    expect(mock.api.createWorld).not.toHaveBeenCalled();
    expect(mock.api.setSettings).not.toHaveBeenCalled();
  });

  it('keeps an empty selection without worlds and survives a failing init', async () => {
    const s = fresh({ getSettings: vi.fn(async () => { throw 'running'; }) });
    await s.init();
    s.stopPolling();
    expect(s.error).toBe('Stop the server first.');
    mock.api = makeApi();
    await s.refreshWorlds();
    expect(s.settings.run.worldId).toBe('');
    await s.loadLists();
    expect(s.lists.admin).toEqual([]);
  });

  it('reports a data folder that cannot be opened, goes slow, and boots again after a retry', async () => {
    vi.useFakeTimers();
    let open = false;
    const s = fresh({
      boot: vi.fn(async () => {
        await new Promise((r) => setTimeout(r, 5000));
        if (!open) throw '/data: Permission denied (os error 13)';
      })
    });
    const run = s.init();
    expect(s.boot).toBe('loading');
    await vi.advanceTimersByTimeAsync(4100);
    expect(s.boot).toBe('slow');
    await vi.advanceTimersByTimeAsync(1500);
    await run;
    expect(s.boot).toBe('error');
    expect(s.bootError).toBe('/data: Permission denied (os error 13)');
    expect(s.info?.version).toBe('9.9.9');
    expect(s.ready).toBe(false);
    expect(mock.api.getSettings).not.toHaveBeenCalled();
    open = true;
    const again = s.init();
    await vi.advanceTimersByTimeAsync(5100);
    await again;
    s.stopPolling();
    expect(s.boot).toBe('done');
    expect(s.bootError).toBe('');
    expect(s.ready).toBe(true);
  });

  it('boots without app info and fetches it once the settings are in', async () => {
    let calls = 0;
    const s = fresh({ appInfo: vi.fn(async () => { if (calls++ === 0) throw 'x'; return { version: '1.0.0', dataDir: '/d', platform: 'linux', nativeControls: false }; }) });
    await s.init();
    s.stopPolling();
    expect(s.info?.version).toBe('1.0.0');
    expect(s.boot).toBe('done');
  });

  it('polls on a timer and stops', async () => {
    vi.useFakeTimers();
    const s = fresh();
    s.startPolling(10);
    await vi.advanceTimersByTimeAsync(25);
    expect(mock.api.poll).toHaveBeenCalled();
    s.stopPolling();
    s.stopPolling();
  });

  it('appends classified log lines with their times and caps the buffer', async () => {
    const s = fresh();
    mock.api.poll = vi.fn(async () => makePoll({
      state: 'running', worldId: W1, startedAt: Date.now() - 60000,
      lines: Array.from({ length: 1600 }, (_, i) => (i === 1599 ? 'MIRA: hi' : `l${i}`)),
      times: [5], next: 1600
    }));
    await s.pollOnce();
    expect(s.log).toHaveLength(1500);
    const last = s.log.at(-1)!;
    expect(last.level).toBe('chat');
    expect(last.at).toBe(s.now);
    expect(s.active).toBe(true);
    expect(s.uptimeMs).toBeGreaterThanOrEqual(60000);
    s.clearLog();
    expect(s.log).toHaveLength(0);
    expect(s.runningWorldId).toBe(W1);
  });

  it('reports offline and recovers', async () => {
    const s = fresh();
    mock.api.poll = vi.fn(async () => { throw new Error('x'); });
    await s.pollOnce();
    expect(s.offline).toBe(true);
    mock.api.poll = vi.fn(async () => makePoll());
    await s.pollOnce();
    expect(s.offline).toBe(false);
    expect(s.uptimeMs).toBeNull();
    expect(s.currentWorld).toBeUndefined();
  });

  it('remembers players across sessions', async () => {
    const s = fresh();
    mock.api.poll = vi.fn(async () => makePoll({ state: 'running', players: [player('bob'), player('amy')] }));
    await s.pollOnce();
    await s.pollOnce();
    mock.api.poll = vi.fn(async () => makePoll({ state: 'running', players: [player('bob')] }));
    await s.pollOnce();
    mock.api.poll = vi.fn(async () => makePoll());
    await s.pollOnce();
    expect(s.known.map((k) => k.name).sort()).toEqual(['amy', 'bob']);
    const again = new AppStore();
    mock.api = makeApi();
    await again.init();
    again.stopPolling();
    expect(again.known).toHaveLength(2);
  });

  it('navigates', () => {
    const s = fresh();
    s.navigate('players');
    expect(s.screen).toBe('players');
  });
});

describe('worlds and settings', () => {
  it('saves language and settings', async () => {
    const s = fresh();
    await s.setLanguage('tr');
    expect(mock.api.setSettings).toHaveBeenCalledWith(expect.objectContaining({ language: 'tr' }));
    setLang('en');
  });

  it('creates a world and loads it, or only creates it', async () => {
    const s = fresh({ listWorlds: vi.fn(async () => [{ id: W1, name: 'A' }, { id: 'id1', name: 'B' }]) });
    expect(await s.createWorld({ name: 'b', size: 'normal', expert: false }, true)).toBe(true);
    expect(s.settings.run.worldId).toBe('id1');
    s.settings.run.worldId = W1;
    expect(await s.createWorld({ name: 'b', size: 'normal', expert: false }, false)).toBe(true);
    expect(s.settings.run.worldId).toBe(W1);
  });

  it('reports a failing create', async () => {
    const s = fresh({ createWorld: vi.fn(async () => { throw 'create'; }) });
    expect(await s.createWorld({ name: 'b', size: 'tiny', expert: true }, true)).toBe(false);
    expect(s.error).toBe('Could not create the world.');
  });

  it('deletes a world and confirms with a toast', async () => {
    const s = fresh();
    await s.deleteWorld('x');
    expect(mock.api.deleteWorld).toHaveBeenCalledWith('x');
    // the last world went, so a new one is created and loaded
    expect(mock.api.createWorld).toHaveBeenCalled();
    expect(toasts.items).toHaveLength(2);
    mock.api.deleteWorld = vi.fn(async () => { throw 'running'; });
    await s.deleteWorld('x');
    expect(s.error).toBe('Stop the server first.');
  });

  it('loads a world and restarts a running server', async () => {
    const s = fresh();
    await s.loadWorld('w2');
    expect(mock.api.startServer).not.toHaveBeenCalled();
    s.state = 'running';
    await s.loadWorld('w3');
    expect(mock.api.stopServer).toHaveBeenCalled();
    expect(mock.api.startServer).toHaveBeenCalledWith(expect.objectContaining({ worldId: 'w3' }));
  });

  it('applies run options with or without a restart', async () => {
    const s = fresh();
    expect(await s.applyRun({ ...s.settings.run, port: 2000 })).toBe(true);
    expect(s.settings.run.port).toBe(2000);
    expect(mock.api.startServer).not.toHaveBeenCalled();
    mock.api.setSettings = vi.fn(async () => { throw 'running'; });
    expect(await s.applyRun({ ...s.settings.run })).toBe(false);
    mock.api.setSettings = vi.fn(async () => {});
    s.state = 'running';
    expect(await s.applyRun({ ...s.settings.run, port: 2001 })).toBe(true);
    expect(mock.api.startServer).toHaveBeenCalled();
  });
});

describe('run and console', () => {
  it('starts, stops and reports failures', async () => {
    const s = fresh();
    expect(await s.start()).toBe(true);
    await s.stop();
    expect(mock.api.stopServer).toHaveBeenCalled();
    mock.api.startServer = vi.fn(async () => { throw 'world'; });
    expect(await s.start()).toBe(false);
    expect(s.error).toBe('Select a world.');
  });

  it('keeps command history and echoes commands', async () => {
    const s = fresh();
    expect(await s.sendCommand('  ')).toBe(false);
    await s.sendCommand('/players');
    await s.sendCommand('/help');
    await s.sendCommand('/players');
    expect(s.history).toEqual(['/players', '/help']);
    expect(s.log.at(-1)).toMatchObject({ level: 'cmd', text: '/players' });
    expect(mock.api.sendCommand).toHaveBeenCalledWith('/players');
  });
});

describe('lists and moderation', () => {
  it('adds, removes and checks names case-insensitively', async () => {
    const s = fresh();
    await s.loadLists();
    expect(s.has('admin', 'ALICE')).toBe(true);
    expect(await s.addName('admin', 'alice')).toBe(false);
    expect(await s.addName('mod', 'Bob')).toBe(true);
    expect(mock.api.setList).toHaveBeenCalledWith(W1, 'mod', ['alice', 'Bob']);
    expect(await s.removeName('mod', 'alice')).toBe(true);
    expect(s.roleOf('alice')).toBe('admin');
    expect(s.roleOf('zed')).toBeUndefined();
    s.lists.admin = [];
    expect(s.roleOf('Bob')).toBe('mod');
  });

  it('reloads lists shortly after a change on a running server and fails cleanly', async () => {
    vi.useFakeTimers();
    const s = fresh();
    s.state = 'running';
    await s.commitList('admin', ['x']);
    await vi.advanceTimersByTimeAsync(900);
    expect(mock.api.getList).toHaveBeenCalled();
    mock.api.setList = vi.fn(async () => { throw 'missing'; });
    expect(await s.commitList('admin', ['y'])).toBe(false);
    expect(s.lists.admin).not.toEqual(['y']);
  });

  it('survives unreadable lists', async () => {
    const s = fresh({ getList: vi.fn(async () => { throw new Error('x'); }) });
    await s.loadLists();
    expect(s.lists.white).toEqual([]);
  });

  it('kicks and bans', async () => {
    const s = fresh();
    await s.loadLists();
    await s.kick('bob');
    expect(mock.api.sendCommand).toHaveBeenCalledWith('/kick bob');
    await s.ban('bob', '1.2.3.4');
    expect(mock.api.setList).toHaveBeenCalledWith(W1, 'black', ['alice', 'bob', '1.2.3.4']);
    await s.ban('ALICE');
    expect(toasts.items.length).toBeGreaterThanOrEqual(2);
    mock.api.sendCommand = vi.fn(async () => { throw 'stopped'; });
    const before = toasts.items.length;
    await s.kick('x');
    expect(toasts.items).toHaveLength(before);
    mock.api.setList = vi.fn(async () => { throw 'stopped'; });
    await s.ban('q');
    expect(toasts.items).toHaveLength(before);
  });
});
