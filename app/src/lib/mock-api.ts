import { vi } from 'vitest';
import type { Poll, Settings, World } from './types';

export const W1 = 'a'.repeat(32);

export function makePoll(over: Partial<Poll> = {}): Poll {
  return { state: 'stopped', detail: null, worldId: null, port: null, lines: [], times: [], next: 0, players: [], startedAt: null, ...over };
}

export function makeApi(over: Partial<Record<string, unknown>> = {}) {
  const settings: Settings = {
    language: 'en',
    run: { worldId: '', port: 15151, maxPlayers: 0, password: '', saveDelay: 10 }
  };
  const worlds: World[] = [];
  return {
    appInfo: vi.fn(async () => ({ version: '9.9.9', dataDir: '/data', platform: 'linux', nativeControls: false })),
    boot: vi.fn(async () => {}),
    splashSet: vi.fn(async () => {}),
    splashGet: vi.fn(async () => null as unknown),
    splashRetry: vi.fn(async () => {}),
    chooseDataFolder: vi.fn(async () => true),
    appQuit: vi.fn(async () => {}),
    mainReady: vi.fn(async () => {}),
    onSplashState: vi.fn(async () => () => {}),
    onSplashRetry: vi.fn(async () => () => {}),
    listWorlds: vi.fn(async () => worlds),
    createWorld: vi.fn(async (w: { name: string }) => ({ id: 'id1', name: w.name.toUpperCase() })),
    deleteWorld: vi.fn(async () => {}),
    startServer: vi.fn(async () => {}),
    stopServer: vi.fn(async () => {}),
    poll: vi.fn(async () => makePoll()),
    stats: vi.fn(async () => ({ cpu: 12.5, mem: 180 * 2 ** 20, threadCpu: true })),
    sendCommand: vi.fn(async () => {}),
    getList: vi.fn(async () => ['alice']),
    setList: vi.fn(async () => {}),
    networkInfo: vi.fn(async (port: number) => [{ ip: '192.168.1.5', endpoint: `192.168.1.5:${port}` }]),
    getSettings: vi.fn(async () => settings),
    setSettings: vi.fn(async () => {}),
    updateCheck: vi.fn(async () => null as unknown),
    updateDownload: vi.fn(async () => {}),
    updateProgress: vi.fn(async () => ({ stage: 'idle', done: 0, total: 0, bytesPerS: 0, etaS: 0, error: null }) as unknown),
    updateCancel: vi.fn(async () => {}),
    updateInstall: vi.fn(async () => {}),
    updateResume: vi.fn(async () => null as unknown),
    appRestart: vi.fn(async () => {}),
    openUrl: vi.fn(async () => {}),
    ...over
  };
}

export function makeWin(over: Partial<Record<string, unknown>> = {}) {
  return {
    minimize: vi.fn(async () => {}),
    toggleMaximize: vi.fn(async () => {}),
    close: vi.fn(async () => {}),
    isMaximized: vi.fn(async () => false),
    isFullscreen: vi.fn(async () => false),
    onResized: vi.fn(async () => () => {}),
    onFocus: vi.fn(async () => () => {}),
    ...over
  };
}

export function makeUpdate(over: Record<string, unknown> = {}) {
  return {
    version: '2.0.0',
    current: '9.9.9',
    notes: '## New\n- Faster saves\n- **Bold** fix with `code`\n\nSee [the page](https://x).',
    page: 'https://github.com/Blockheads-Club/blockheads-server/releases/tag/v2.0.0',
    publishedAt: null,
    prerelease: false,
    asset: { name: 'setup.exe', url: 'https://github.com/Blockheads-Club/blockheads-server/releases/download/v2.0.0/setup.exe', size: 31.9 * 2 ** 20 },
    mode: 'installer',
    ...over
  };
}
