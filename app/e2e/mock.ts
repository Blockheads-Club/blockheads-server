import type { Page } from '@playwright/test';

/** Replaces the Tauri IPC with an in-page fake server so the built web UI can be exercised in a browser. */
export async function mockTauri(page: Page, opts: { worlds?: boolean; splash?: object } = {}) {
  await page.addInitScript(({ withWorlds, splash }) => {
    const s = {
      worlds: withWorlds ? [{ id: 'a'.repeat(32), name: 'Sunny Isle' }, { id: 'b'.repeat(32), name: 'Deep Cave Club' }] : ([] as { id: string; name: string }[]),
      state: 'stopped',
      startedAt: null as number | null,
      lines: [] as string[],
      times: [] as number[],
      lists: { admin: ['MIRA_K'], mod: [], white: [], black: [] } as Record<string, string[]>,
      settings: { language: 'en', run: { worldId: '', port: 15151, maxPlayers: 0, password: '', saveDelay: 10 } },
      players: [] as { name: string; ip: string; id: string; joinedAt: number }[]
    };
    const log = (t: string) => {
      s.lines.push(t);
      s.times.push(Date.now());
    };
    (window as any).__calls = [];
    (window as any).__TAURI_INTERNALS__ = {
      transformCallback: () => 0,
      invoke: async (cmd: string, a: any) => {
        (window as any).__calls.push([cmd, a]);
        switch (cmd) {
          case 'boot': case 'splash_set': case 'main_ready': case 'splash_retry': case 'app_quit': return;
          case 'splash_get': return splash;
          case 'choose_data_folder': return true;
          case 'plugin:event|listen': return 0;
          case 'app_info': return { version: '1.2.3', dataDir: '/data/TheBlockheads', platform: innerWidth < 600 ? 'android' : 'linux', nativeControls: false };
          case 'get_settings': return s.settings;
          case 'set_settings': s.settings = a.settings; return;
          case 'list_worlds': return s.worlds;
          case 'create_world': {
            const w = { id: String(s.worlds.length + 1).padStart(32, '0'), name: a.world.name.toUpperCase() };
            s.worlds.push(w);
            return w;
          }
          case 'delete_world': s.worlds = s.worlds.filter((w) => w.id !== a.id); return;
          case 'start_server':
            s.state = 'running';
            s.startedAt = Date.now() - 134 * 60000;
            log('World load complete.');
            log('W - Player Connected MIRA_K | 192.168.1.9 | 7c1e5b2a90f4');
            log('MIRA_K: hi everyone!');
            log('Save took 6.2 s, failed once');
            s.players = [{ name: 'MIRA_K', ip: '192.168.1.9', id: '7c1e5b2a90f4', joinedAt: Date.now() - 112 * 60000 }];
            return;
          case 'stop_server': s.state = 'stopped'; s.players = []; s.startedAt = null; return;
          case 'stats': return { cpu: s.state === 'running' ? 7 + (Date.now() / 1000) % 5 : 0, mem: s.state === 'running' ? 212 * 2 ** 20 : 0, threadCpu: true };
          case 'poll': {
            const lines = s.lines.slice(a.since);
            const times = s.times.slice(a.since);
            return {
              state: s.state, detail: null, worldId: s.state === 'stopped' ? null : s.settings.run.worldId || s.worlds[0]?.id || null,
              port: 15151, lines, times, next: s.lines.length, players: s.players, startedAt: s.startedAt
            };
          }
          case 'send_command': log('> ' + a.line); return;
          case 'get_list': return s.lists[a.kind] ?? [];
          case 'set_list': s.lists[a.kind] = a.names; return;
          case 'network_info': return [{ ip: '192.168.1.42', endpoint: `192.168.1.42:${a.port}` }];
        }
        throw new Error('unmocked ' + cmd);
      }
    };
  }, { withWorlds: opts.worlds ?? false, splash: opts.splash ?? null });
}
