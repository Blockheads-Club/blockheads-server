import { beforeEach, describe, expect, it, vi } from 'vitest';

const invoke = vi.hoisted(() => vi.fn(async () => null));
vi.mock('@tauri-apps/api/core', () => ({ invoke }));

const listen = vi.hoisted(() => vi.fn(async (_name: string, _cb: (e: { payload: unknown }) => void) => () => {}));
vi.mock('@tauri-apps/api/event', () => ({ listen }));

import { api } from './api';

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
      'app_info', 'list_worlds', 'create_world', 'delete_world', 'start_server', 'stop_server', 'poll',
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
