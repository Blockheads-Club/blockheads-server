import type { Page } from '@playwright/test';

/** Replaces the Tauri IPC with an in-page fake server so the built web UI can be exercised in a browser. */
export async function mockTauri(
  page: Page,
  opts: { worlds?: boolean; splash?: object; lang?: string; update?: boolean; mods?: 'demo' | 'none' } = {}
) {
  await page.addInitScript(({ withWorlds, splash, lang, update, demo }) => {
    const s = {
      worlds: withWorlds ? [{ id: 'a'.repeat(32), name: 'Sunny Isle' }, { id: 'b'.repeat(32), name: 'Deep Cave Club' }] : ([] as { id: string; name: string }[]),
      state: 'stopped',
      startedAt: null as number | null,
      lines: [] as string[],
      times: [] as number[],
      lists: { admin: ['MIRA_K'], mod: [], white: [], black: [] } as Record<string, string[]>,
      settings: { language: lang, run: { worldId: '', port: 15151, maxPlayers: 0, password: '', saveDelay: 10 } },
      players: [] as { name: string; ip: string; id: string; joinedAt: number }[]
    };
    // ---- mods: a small fake of src-tauri/src/mods.rs ----
    type M = {
      id: string; name: string; version: string; author: string; perms: string[]; granted: string[]; enabled: boolean;
      server: boolean; client: boolean; native?: boolean; deps?: Record<string, string>; fail?: string; budget?: boolean; required?: boolean;
      reviewed?: boolean; update?: string;
    };
    const mk = (id: string, name: string, version: string, author: string, perms: string[], extra: Partial<M> = {}): M =>
      ({ id, name, version, author, perms, granted: perms.filter((p) => p !== 'native'), enabled: true, server: true, client: false, reviewed: true, ...extra });
    const mods: M[] = demo
      ? [
          mk('better-chat', 'Better Chat', '2.1.0', 'mira_k', ['players:chat', 'channels']),
          mk('land-claims', 'Land Claims', '1.4.2', 'tobyblock', ['world:read', 'world:write', 'players:teleport'], { update: '1.5.0' }),
          mk('minimap', 'Minimap', '0.9.1', 'pixelpan', ['world:read'], { client: true }),
          mk('weather-plus', 'Weather Plus', '3.0.0', 'sunnyside', ['world:write'], { budget: true }),
          mk('shop-signs', 'Shop Signs', '1.2.0', 'mira_k', ['world:read', 'players:inventory'], { deps: { 'economy-core': '^2' } }),
          { ...mk('discord-bridge', 'Discord Bridge', '1.0.3', 'hexwright', ['players:chat', 'net-http:discord.com']), granted: ['players:chat'] },
          { ...mk('fast-tools', 'Fast Tools', '0.4.0', 'blockmonk', ['native'], { native: true, fail: 'missing symbol bh_world_get', reviewed: false }), granted: ['native'] },
          { ...mk('hats-capes', 'Hats & Capes', '1.1.0', 'pixelpan', []), server: false, client: true, enabled: false }
        ]
      : [];
    const vjson = (id: string, version: string, size: number, permissions: object, more: object = {}) => ({
      version, url: `https://example.invalid/${id}.bhmod`, sha256: 'a'.repeat(64), size, sides: ['server'], api: '^0.1', permissions, published: '2026-10-15', ...more
    });
    const index = [
      { id: 'economy-core', name: 'Economy Core', description: 'Coins, wallets and payments. Other mods use it for shops and jobs.', owners: ['mira_k'], tags: ['economy'], v: vjson('economy-core', '2.3.1', 226000, { players: ['chat', 'inventory'] }) },
      { id: 'auction-house', name: 'Auction House', description: 'Players sell items to each other with timed auctions.', owners: ['tobyblock'], tags: ['economy'], v: vjson('auction-house', '1.3.0', 186000, { players: ['inventory', 'chat'] }, { depends: { 'economy-core': '^2' } }) },
      { id: 'pet-companions', name: 'Pet Companions', description: 'Tame a dodo and it follows you, with its own little hat.', owners: ['pixelpan'], tags: ['cosmetic'], v: vjson('pet-companions', '1.0.0', 312000, { world: 'read', channels: true }, { sides: ['server', 'client'], clientRequired: true }) },
      { id: 'turbo-lighting', name: 'Turbo Lighting', description: 'Smooth lighting and coloured torches. Uses native code for speed.', owners: ['lumen'], tags: ['world'], v: vjson('turbo-lighting', '0.8.0', 540000, { native: true }, { sides: ['server', 'client'], clientRequired: true }) },
      { id: 'land-claims', name: 'Land Claims', description: 'Protect your land with a claim stick.', owners: ['tobyblock'], tags: ['building'], v: vjson('land-claims', '1.5.0', 200000, { world: 'write', players: ['teleport'] }) }
    ];
    const permNames = (p: any) => [
      ...(p.world ? [`world:${p.world}`] : []), ...(p.players ?? []).map((x: string) => `players:${x}`), ...(p.channels ? ['channels'] : []), ...(p.native ? ['native'] : [])
    ];
    const mstatus = (m: M) => {
      const missing = m.perms.filter((p) => !m.granted.includes(p));
      const dep = Object.keys(m.deps ?? {}).find((d) => !mods.some((x) => x.id === d && x.enabled));
      if (!m.enabled) return ['off', null];
      if (!m.server) return [s.state === 'running' ? 'running' : 'ready', null];
      if (missing.length) return ['permissions', missing.join(', ')];
      if (dep) return ['dependency', `${dep} ${m.deps![dep]}`];
      if (m.fail) return ['failed', m.fail];
      if (s.state !== 'running') return ['ready', null];
      if (m.budget) return ['budget', '3 faults within 60 s'];
      return ['running', null];
    };
    const summary = (m: M) => {
      const [status, detail] = mstatus(m);
      return {
        id: m.id, name: { en: m.name }, description: { en: index.find((i) => i.id === m.id)?.description ?? `${m.name} for your server.` },
        version: m.version, authors: [m.author], license: 'MIT', homepage: `https://github.com/${m.author}/${m.id}`,
        sides: { server: m.server, client: m.client, clientRequired: m.client && m.required !== false, clientRequiredDefault: m.client, clientRequiredLocked: !m.client }, runtime: m.native ? 'jvm' : 'lua', native: !!m.native,
        permissions: m.perms, granted: m.granted, missing: m.perms.filter((p) => !m.granted.includes(p)), enabled: m.enabled, status, detail,
        dependencies: Object.entries(m.deps ?? {}).map(([id, req]) => ({ id, req, installed: mods.find((x) => x.id === id)?.version ?? null, ok: mods.some((x) => x.id === id && x.enabled), soft: false })),
        file: `${m.id}-${m.version}.bhmod`, packed: true, sha256: 'a'.repeat(64), size: 186000, reviewed: !!m.reviewed, update: m.update ?? null,
        faults: m.budget && s.state === 'running' ? [{ at: Date.now() - 60000, kind: 'slow', text: 'tick took 7.9 ms (budget 5 ms)', ms: 7.9 }, { at: Date.now() - 30000, kind: 'error', text: 'tick: timeout' }] : [],
        lines: s.state === 'running' ? [{ at: Date.now() - 90000, level: 'info', text: 'enabled' }, ...(m.budget ? [{ at: Date.now() - 30000, level: 'warn', text: 'tick took 7.9 ms (budget 5 ms)' }, { at: Date.now() - 1000, level: 'info', text: 'disabled: 3 faults within 60 s' }] : [])] : [],
        slowestMs: s.state === 'running' && m.server && !m.native ? (m.budget ? 7.9 : 1.2) : null, storageBytes: s.state === 'running' ? null : 2200000,
        settings: m.id === 'better-chat' ? chatSettings.fields.filter((f) => f.scope === 'server').length : 0
      };
    };
    // Settings of Better Chat in mod-sdk's canonical JSON (the contract of mod_settings_get).
    const lbl = (en: string) => ({ en });
    const chatSettings = {
      fields: [
        { key: 'filter', type: 'bool', default: true, label: lbl('Filter bad words'), help: lbl('Replaces listed words with ***.'), group: 'Chat', scope: 'server', restart: false },
        { key: 'slowmode', type: 'int', default: 0, min: 0, max: 60, step: 5, label: lbl('Slow mode (seconds)'), group: 'Chat', scope: 'server', restart: false },
        { key: 'format', type: 'enum', default: 'name', values: ['name', 'name-color', 'tag'], label: lbl('Name style'), group: 'Chat', scope: 'server', restart: true },
        { key: 'motd', type: 'string', default: '', max_length: 80, label: lbl('Message of the day'), group: 'Chat', scope: 'server', restart: false },
        { key: 'colors', type: 'roles', default: ['admin'], label: lbl('Who may use colors'), group: 'Roles', scope: 'server', restart: false },
        { key: 'commands', type: 'matrix', default: { mute: { owner: false, admin: true, mod: true, everyone: false }, clear: { owner: false, admin: false, mod: false, everyone: false }, announce: { owner: false, admin: false, mod: false, everyone: false } }, rows: ['mute', 'clear', 'announce'], cols: ['owner', 'admin', 'mod', 'everyone'], cell: 'bool', label: lbl('Commands'), group: 'Roles', scope: 'server', restart: false },
        { key: 'channels', type: 'list', default: [], max_items: 3, label: lbl('Channels'), scope: 'server', restart: false,
          item: [{ key: 'name', type: 'string', default: '', max_length: 12, label: lbl('Name') }, { key: 'color', type: 'enum', default: 'white', values: ['white', 'red', 'blue'], label: lbl('Color') }] },
        { key: 'mention_sound', type: 'bool', default: true, label: lbl('Mention sound'), scope: 'client', restart: false }
      ],
      values: { motd: 'Welcome!' } as Record<string, unknown>
    };
    (window as any).__mods = mods;
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
          case 'update_check':
            return update ? {
              version: '2.0.0', current: '1.2.3', notes: '## New\n- Faster saves\n- **Safer** restarts', prerelease: false, publishedAt: null,
              page: 'https://github.com/Blockheads-Club/blockheads-server/releases/tag/v2.0.0',
              asset: { name: 'a', url: 'https://github.com/Blockheads-Club/blockheads-server/releases/download/v2.0.0/a', size: 33_000_000 },
              mode: 'installer'
            } : null;
          case 'update_download': (s as any).dl = 0; return;
          case 'update_progress': {
            const done = Math.min(4, ((s as any).dl = ((s as any).dl ?? 0) + 1));
            return { stage: done >= 4 ? 'ready' : 'downloading', done: done * 8_250_000, total: 33_000_000, bytesPerS: 2_000_000, etaS: 4 - done, error: null };
          }
          case 'update_cancel': case 'update_install': case 'open_url': case 'app_restart': return;
          case 'update_resume': return null;
          case 'mods_overview':
            return {
              dir: '/data/TheBlockheads/mods', size: mods.length * 186000, mods: mods.map(summary), problems: [],
              clientMods: mods.filter((m) => m.enabled && m.client).map((m) => m.id), changes: [], running: s.state === 'running',
              error: null, softMs: 5, hardMs: 25, hostLines: []
            };
          case 'mods_set_enabled': mods.find((m) => m.id === a.id)!.enabled = a.on; return;
          case 'mods_set_client_required': {
            const m = mods.find((x) => x.id === a.id)!;
            m.required = a.on;
            return;
          }
          case 'mods_set_grant': {
            const m = mods.find((x) => x.id === a.id)!;
            m.granted = a.on ? [...m.granted, a.perm] : m.granted.filter((p) => p !== a.perm);
            return;
          }
          case 'mods_remove': mods.splice(mods.findIndex((m) => `${m.id}-${m.version}.bhmod` === a.file), 1); return;
          case 'mods_clear_data': case 'mods_open_folder': return;
          case 'mod_settings_get': return JSON.parse(JSON.stringify(chatSettings));
          case 'mod_settings_set': {
            if (a.key === 'motd' && /https?:/.test(a.value)) throw { code: 'setting', detail: 'other' };
            chatSettings.values[a.key] = a.value;
            return;
          }
          case 'mods_browse':
            return {
              generated: '2026-10-21T12:00:00Z', offline: false, error: null, hidden: 0,
              mods: index.map((i) => ({
                id: i.id, name: i.name, description: i.description, owners: i.owners, repo: '', tags: i.tags, version: i.v.version,
                sides: i.v.sides, clientRequired: !!(i.v as any).clientRequired, permissions: permNames(i.v.permissions), native: !!(i.v.permissions as any).native,
                depends: (i.v as any).depends ?? {}, size: i.v.size, published: i.v.published, firstPublished: '2026-09-01',
                installed: mods.find((m) => m.id === i.id)?.version ?? null
              }))
            };
          case 'mods_plan': {
            const root = index.find((i) => i.id === a.id)!;
            const item = (i: typeof root, neededBy: string | null) => ({
              id: i.id, name: i.name, version: i.v.version, size: i.v.size, sides: i.v.sides, clientRequired: !!(i.v as any).clientRequired,
              permissions: permNames(i.v.permissions), native: !!(i.v.permissions as any).native, neededBy, replaces: mods.find((m) => m.id === i.id)?.version ?? null, reviewed: true
            });
            const items = [item(root, null)];
            for (const d of Object.keys((root.v as any).depends ?? {})) if (!mods.some((m) => m.id === d)) items.push(item(index.find((i) => i.id === d)!, root.id));
            return { items, satisfied: [], missing: [], totalBytes: items.reduce((n, i) => n + i.size, 0) };
          }
          case 'mods_inspect_url': throw { code: 'checksum', file: 'fast-tools-0.4.0.bhmod', expected: '9f2c' + 'a'.repeat(56) + 'a71e', got: '41b0' + 'b'.repeat(56) + '0c3d' };
          case 'mods_install': {
            for (const it of a.request.items) {
              const i = index.find((x) => x.id === it.id)!;
              const old = mods.findIndex((m) => m.id === it.id);
              const m = mk(i.id, i.name, i.v.version, i.owners[0], permNames(i.v.permissions), { client: !!(i.v as any).clientRequired, native: !!(i.v.permissions as any).native, deps: (i.v as any).depends });
              m.granted = a.request.grants[i.id] ?? [];
              m.enabled = a.request.enable;
              if (old >= 0) mods[old] = m;
              else mods.push(m);
            }
            return { ids: a.request.items.map((i: any) => i.id), restart: s.state === 'running' };
          }
        }
        throw new Error('unmocked ' + cmd);
      }
    };
  }, { withWorlds: opts.worlds ?? false, splash: opts.splash ?? null, lang: opts.lang ?? 'en', update: opts.update ?? false, demo: opts.mods === 'demo' });
}
