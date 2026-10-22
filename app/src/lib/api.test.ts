import { beforeEach, describe, expect, it, vi } from 'vitest';

const invoke = vi.hoisted(() => vi.fn(async () => null));
vi.mock('@tauri-apps/api/core', () => ({ invoke }));

const listen = vi.hoisted(() => vi.fn(async (_name: string, _cb: (e: { payload: unknown }) => void) => () => {}));
vi.mock('@tauri-apps/api/event', () => ({ listen }));

const w = vi.hoisted(() => {
  const handlers = { onFocusChanged: (cb: (e: { payload: boolean }) => void) => (cb({ payload: true }), 'f') };
  return {
    minimize: vi.fn(async () => 'min'), toggleMaximize: vi.fn(async () => 'tog'), close: vi.fn(async () => 'cl'),
    isMaximized: vi.fn(async () => true), isFullscreen: vi.fn(async () => false),
    onResized: vi.fn(async (_cb: () => void) => () => {}), ...handlers
  };
});
vi.mock('@tauri-apps/api/window', () => ({ getCurrentWindow: () => w }));

import { api, win } from './api';

describe('api', () => {
  beforeEach(() => invoke.mockClear());

  it('maps every call onto its Tauri command and arguments', async () => {
    const run = { worldId: 'w', port: 1, maxPlayers: 0, password: '', saveDelay: 10 };
    await api.appInfo();
    await api.listWorlds();
    await api.createWorld({ name: 'n', size: 'tiny', expert: false });
    await api.deleteWorld('w');
    await api.startServer(run);
    await api.stopServer();
    await api.poll(3);
    await api.stats();
    await api.sendCommand('/help');
    await api.getList('w', 'admin');
    await api.setList('w', 'mod', ['a']);
    await api.networkInfo(15151);
    await api.getSettings();
    await api.setSettings({ language: 'en', run });
    await api.boot();
    await api.splashSet({ boot: 'loading', error: '', version: '1', lang: 'en' });
    await api.splashGet();
    await api.splashRetry();
    await api.chooseDataFolder();
    await api.appQuit();
    await api.mainReady();
    expect(invoke.mock.calls.map((c: any[]) => c[0])).toEqual([
      'app_info', 'list_worlds', 'create_world', 'delete_world', 'start_server', 'stop_server', 'poll', 'stats',
      'send_command', 'get_list', 'set_list', 'network_info', 'get_settings', 'set_settings',
      'boot', 'splash_set', 'splash_get', 'splash_retry', 'choose_data_folder', 'app_quit', 'main_ready'
    ]);
    expect(invoke).toHaveBeenCalledWith('set_list', { worldId: 'w', kind: 'mod', names: ['a'] });
    expect(invoke).toHaveBeenCalledWith('poll', { since: 3 });
  });

  it('listens to the splash events and hands over their payloads', async () => {
    const cb = vi.fn();
    await api.onSplashState(cb);
    await api.onSplashRetry(cb);
    expect(listen.mock.calls.map((c) => c[0])).toEqual(['splash-state', 'splash-retry']);
    listen.mock.calls[0][1]({ payload: { boot: 'slow' } });
    listen.mock.calls[1][1]({ payload: null });
    expect(cb.mock.calls).toEqual([[{ boot: 'slow' }], []]);
  });
});

describe('mods api', () => {
  beforeEach(() => {
    invoke.mockClear();
    listen.mockClear();
  });

  it('maps the mod calls onto their commands', async () => {
    await api.modsOverview();
    await api.modsSetEnabled('a', true);
    await api.modsSetGrant('a', 'players:chat', false);
    await api.modsRemove('a.bhmod');
    await api.modsClearData('a');
    await api.modsOpenFolder();
    await api.modsOpenFolder('a');
    await api.modsBrowse();
    await api.modsPlan('a');
    await api.modsPlan('a', '1.0.0');
    await api.modsInspectUrl('https://x/a.bhmod');
    const bytes = new Uint8Array([1, 2]);
    await api.modsInspectFile('fast tools.bhmod', bytes);
    const request = { items: [{ id: 'a', version: '1.0.0' }], grants: {}, enable: true, local: false };
    await api.modsInstall(request);
    expect(invoke.mock.calls.map((c: any[]) => c[0])).toEqual([
      'mods_overview', 'mods_set_enabled', 'mods_set_grant', 'mods_remove', 'mods_clear_data', 'mods_open_folder',
      'mods_open_folder', 'mods_browse', 'mods_plan', 'mods_plan', 'mods_inspect_url', 'mods_inspect_file', 'mods_install'
    ]);
    expect(invoke).toHaveBeenCalledWith('mods_open_folder', { id: null });
    expect(invoke).toHaveBeenCalledWith('mods_plan', { id: 'a', version: null });
    expect(invoke).toHaveBeenCalledWith('mods_plan', { id: 'a', version: '1.0.0' });
    expect(invoke).toHaveBeenCalledWith('mods_inspect_file', bytes, { headers: { 'x-file-name': 'fast%20tools.bhmod' } });
    expect(invoke).toHaveBeenCalledWith('mods_install', { request });
  });

  it('hands install progress over', async () => {
    const cb = vi.fn();
    await api.onModsProgress(cb);
    expect(listen.mock.calls[0][0]).toBe('mods-progress');
    listen.mock.calls[0][1]({ payload: { step: 'download', done: 0, total: 2 } });
    expect(cb).toHaveBeenCalledWith({ step: 'download', done: 0, total: 2 });
  });
});

describe('win', () => {
  it('forwards to the current window', async () => {
    expect(await win.minimize()).toBe('min');
    expect(await win.toggleMaximize()).toBe('tog');
    expect(await win.close()).toBe('cl');
    expect(await win.isMaximized()).toBe(true);
    expect(await win.isFullscreen()).toBe(false);
    await win.onResized(() => {});
    expect(w.onResized).toHaveBeenCalled();
    const seen: boolean[] = [];
    await win.onFocus((f) => seen.push(f));
    expect(seen).toEqual([true]);
  });
});
