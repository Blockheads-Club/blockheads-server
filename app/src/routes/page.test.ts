import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { createRawSnippet } from 'svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const mock = vi.hoisted(() => ({ api: {} as Record<string, any>, win: {} as Record<string, any> }));
vi.mock('$lib/api', () => ({
  api: new Proxy({}, { get: (_t, k: string) => mock.api[k] }),
  win: new Proxy({}, { get: (_t, k: string) => mock.win[k] })
}));

import { setLang } from '$lib/i18n.svelte';
import { W1, makeApi, makePoll, makeWin } from '$lib/mock-api';
import { store } from '$lib/store.svelte';
import { theme } from '$lib/theme.svelte';
import { updates } from '$lib/update.svelte';
import { makeUpdate } from '$lib/mock-api';
import { viewport } from '$lib/viewport.svelte';
import Page from './+page.svelte';
import Layout from './+layout.svelte';
import * as load from './+layout';

const flags = { phone: false, rail: false };
const listeners: (() => void)[] = [];
function mq(next: Partial<typeof flags>) {
  Object.assign(flags, { phone: false, rail: false }, next);
  Object.defineProperty(window, 'matchMedia', {
    writable: true,
    value: (query: string) => ({
      get matches() {
        return query.includes('max-width: 599px') ? flags.phone : flags.rail;
      },
      addEventListener: (_: string, l: () => void) => listeners.push(l),
      removeEventListener: vi.fn()
    })
  });
}

beforeEach(() => {
  mock.win = makeWin();
  mock.api = makeApi({
    listWorlds: vi.fn(async () => [{ id: W1, name: 'Sunny' }]),
    getSettings: vi.fn(async () => ({ language: 'en', run: { worldId: W1, port: 15151, maxPlayers: 0, password: '', saveDelay: 10 } }))
  });
  setLang('en');
  localStorage.clear();
  store.ready = false;
  store.boot = 'loading';
  store.bootError = '';
  store.info = null;
  store.error = '';
  store.screen = 'dashboard';
  store.state = 'stopped';
  store.players = [];
  store.log = [];
  viewport.phone = false;
  viewport.rail = false;
  mq({});
});
afterEach(() => {
  cleanup();
  store.stopPolling();
});

describe('updates in the shell', () => {
  it('checks after start-up and keeps the update state in the title bar while the popup is hidden', async () => {
    mock.api.updateCheck = vi.fn(async () => makeUpdate());
    render(Page);
    await screen.findByRole('heading', { name: 'Dashboard' });
    expect(await screen.findByRole('dialog', { name: 'Server 2.0.0 is here' })).toBeInTheDocument();
    await fireEvent.click(screen.getByRole('button', { name: 'Later' }));
    const chip = (label: string) => screen.findByRole('button', { name: `${label}: open the update window` });
    await fireEvent.click(await chip('Update available'));
    expect(await screen.findByRole('dialog')).toBeInTheDocument();
    updates.open = false;
    const states: [Record<string, unknown>, string][] = [
      [{ stage: 'downloading', progress: { stage: 'downloading', done: 1, total: 4, bytesPerS: 0, etaS: 0, error: null } }, 'Updating 25%'],
      [{ stage: 'installing' }, 'Installing update…'],
      [{ stage: 'ready' }, 'Restart to update'],
      [{ stage: 'error', failed: 'install' }, 'Update failed']
    ];
    for (const [patch, label] of states) {
      Object.assign(updates, patch);
      expect(await chip(label)).toBeInTheDocument();
    }
    updates.failed = 'check';
    await waitFor(() => expect(document.querySelector('.chip')).toBeNull());
    Object.assign(updates, { stage: 'idle', failed: null, info: null });
  });
});

describe('desktop window frame', () => {
  it('draws the title bar with the screen as context, the running server and working buttons', async () => {
    mock.api.poll = vi.fn(async () => makePoll({ state: 'running', worldId: W1 }));
    render(Page);
    await screen.findByRole('heading', { name: 'Dashboard' });
    expect(screen.getByRole('banner', { name: 'Window title bar' })).toHaveTextContent('Blockheads Server · Dashboard');
    expect(document.documentElement).toHaveClass('desktop');
    expect(document.title).toBe('Blockheads Server · Dashboard');
    expect(await screen.findByText('Running · Sunny')).toBeInTheDocument();
    await fireEvent.click(screen.getByRole('button', { name: 'Minimize window' }));
    await fireEvent.click(screen.getByRole('button', { name: 'Close window' }));
    expect(mock.win.minimize).toHaveBeenCalled();
    expect(mock.win.close).toHaveBeenCalled();
    await fireEvent.click(screen.getByRole('button', { name: 'Maximize window' }));
    expect(mock.win.toggleMaximize).toHaveBeenCalled();
    expect(document.querySelector('.frame')).toHaveClass('round');
  });

  it('shows a starting and a failed server in the title bar', async () => {
    mock.api.poll = vi.fn(async () => makePoll({ state: 'starting' }));
    mock.api.listWorlds = vi.fn(async () => []);
    mock.api.getSettings = vi.fn(async () => ({ language: 'en', run: { worldId: '', port: 15151, maxPlayers: 0, password: '', saveDelay: 10 } }));
    const first = render(Page);
    await waitFor(() => expect(document.querySelector('.run.warn')).toHaveTextContent(/^Starting$/));
    first.unmount();
    store.state = 'stopped';
    mock.api.poll = vi.fn(async () => makePoll({ state: 'failed' }));
    render(Page);
    await waitFor(() => expect(document.querySelector('.run.bad')).toHaveTextContent('Crashed'));
  });

  it('is square and tracks maximize and focus; window errors are ignored', async () => {
    const resized: (() => void)[] = [];
    let focus: (f: boolean) => void = () => {};
    let max = true;
    mock.win = makeWin({
      isMaximized: vi.fn(async () => max),
      onResized: vi.fn(async (cb: () => void) => { resized.push(cb); return () => {}; }),
      onFocus: vi.fn(async (cb: (f: boolean) => void) => { focus = cb; return () => {}; })
    });
    const r = render(Page);
    await screen.findByRole('heading', { name: 'Dashboard' });
    await waitFor(() => expect(document.querySelector('.frame')).not.toHaveClass('round'));
    expect(screen.getByRole('button', { name: 'Restore window' })).toBeInTheDocument();
    max = false;
    resized.forEach((cb) => cb());
    await waitFor(() => expect(document.querySelector('.frame')).toHaveClass('round'));
    focus(false);
    await waitFor(() => expect(document.querySelector('.bar')).toHaveClass('inactive'));
    r.unmount();
    mock.win = makeWin({
      isMaximized: vi.fn(async () => { throw 'x'; }),
      onResized: vi.fn(async () => { throw 'x'; }),
      onFocus: vi.fn(async () => { throw 'x'; }),
      minimize: vi.fn(async () => { throw 'x'; }),
      toggleMaximize: vi.fn(async () => { throw 'x'; }),
      close: vi.fn(async () => { throw 'x'; })
    });
    render(Page);
    await screen.findByRole('heading', { name: 'Dashboard' });
    for (const n of ['Minimize window', 'Maximize window', 'Close window']) await fireEvent.click(screen.getByRole('button', { name: n }));
  });

  it('releases the window listeners even when it unmounts before they are registered', async () => {
    const off = vi.fn();
    mock.win = makeWin({ onResized: vi.fn(async () => off), onFocus: vi.fn(async () => off) });
    store.info = { version: '1', dataDir: '/d', platform: 'linux', nativeControls: false };
    render(Page).unmount();
    await waitFor(() => expect(off).toHaveBeenCalledTimes(2));
  });

  it('adapts to Windows and macOS, and not at all on Android', async () => {
    for (const [platform, controls] of [['windows', 3], ['macos', 0]] as const) {
      mock.win = makeWin({ isMaximized: vi.fn(async () => platform === 'windows') });
      store.info = null;
      mock.api.appInfo = vi.fn(async () => ({ version: '9.9.9', dataDir: '/d', platform, nativeControls: platform === 'macos' }));
      const r = render(Page);
      await waitFor(() => expect(document.querySelector('.frame')).not.toBeNull());
      expect(document.querySelectorAll('.bar button')).toHaveLength(controls);
      // Windows is maximized here, macOS has native rounded corners: neither draws its own.
      await waitFor(() => expect(document.querySelector('.frame')).not.toHaveClass('round'));
      r.unmount();
    }
    store.info = null;
    mock.api.appInfo = vi.fn(async () => ({ version: '9.9.9', dataDir: '/d', platform: 'android', nativeControls: false }));
    render(Page);
    await screen.findByRole('heading', { name: 'Dashboard' });
    expect(document.querySelector('.bar')).toBeNull();
    expect(document.querySelector('.frame')).toBeNull();
    await waitFor(() => expect(document.documentElement).not.toHaveClass('desktop'));
  });
});

describe('app shell', () => {
  it('loads, shows the dashboard and navigates every screen from the sidebar', async () => {
    render(Page);
    expect(screen.getByText('Loading…')).toBeInTheDocument();
    expect(await screen.findByRole('heading', { name: 'Dashboard' })).toBeInTheDocument();
    for (const [name, heading] of [['Worlds', 'Worlds'], ['Console', 'Console'], ['Players', 'Players'], ['Access lists', 'Access lists'], ['Settings', 'Settings'], ['Dashboard', 'Dashboard']]) {
      await fireEvent.click(screen.getByRole('button', { name }));
      expect(await screen.findByRole('heading', { level: 1, name: heading })).toBeInTheDocument();
    }
    expect(screen.getByRole('button', { name: 'Dashboard' })).toHaveAttribute('aria-current', 'page');
    expect(screen.getByRole('link', { name: 'Skip to content' })).toHaveAttribute('href', '#main');
    expect(document.documentElement.getAttribute('data-theme')).toBeNull();
  });

  it('restores a saved theme and shows the player count badge', async () => {
    localStorage.setItem('bh.theme', 'dark');
    mock.api.poll = vi.fn(async () => makePoll({ state: 'running', worldId: W1, players: [{ name: 'bob', ip: '1', id: 'i', joinedAt: 1 }] }));
    render(Page);
    expect(await screen.findByRole('heading', { name: 'Dashboard' })).toBeInTheDocument();
    expect(theme.choice).toBe('dark');
    expect(await screen.findByLabelText('1 player online')).toBeInTheDocument();
  });

  it('shows and dismisses backend errors', async () => {
    mock.api.getSettings = vi.fn(async () => { throw 'running'; });
    render(Page);
    const err = await screen.findByTestId('error');
    expect(err).toHaveTextContent('Stop the server first.');
    await fireEvent.click(screen.getByRole('button', { name: 'Close' }));
    expect(screen.queryByTestId('error')).toBeNull();
  });

  it('pushes the boot state to the splash window, then reveals the window once', async () => {
    mock.api.getSettings = vi.fn(async () => ({ language: 'tr', run: { worldId: W1, port: 15151, maxPlayers: 0, password: '', saveDelay: 10 } }));
    render(Page);
    await waitFor(() => expect(mock.api.splashSet).toHaveBeenCalledWith({ boot: 'loading', error: '', version: '9.9.9', lang: 'en' }));
    await screen.findByRole('heading', { name: 'Panel' });
    await waitFor(() => expect(mock.api.mainReady).toHaveBeenCalledTimes(1));
    setLang('en');
  });

  it('boots again when the splash asks for it, and survives IPC errors', async () => {
    let retry: () => void = () => {};
    mock.api.onSplashRetry = vi.fn(async (cb: () => void) => { retry = cb; return () => {}; });
    mock.api.splashSet = vi.fn(async () => { throw 'x'; });
    mock.api.mainReady = vi.fn(async () => { throw 'x'; });
    const first = render(Page);
    await screen.findByRole('heading', { name: 'Dashboard' });
    retry();
    await waitFor(() => expect(mock.api.boot).toHaveBeenCalledTimes(2));
    first.unmount();
    mock.api.onSplashRetry = vi.fn(async () => { throw 'x'; });
    render(Page);
    await screen.findByRole('heading', { name: 'Dashboard' });
  });

  it('keeps the main window hidden and the data folder error on the splash window (desktop)', async () => {
    mock.api.boot = vi.fn(async () => { throw '/data: denied'; });
    render(Page);
    await waitFor(() => expect(mock.api.splashSet).toHaveBeenCalledWith(expect.objectContaining({ boot: 'error', error: '/data: denied' })));
    expect(mock.api.mainReady).not.toHaveBeenCalled();
    expect(screen.queryByRole('alert')).toBeNull();
    expect(screen.getByRole('link', { name: 'Skip to content' })).toBeInTheDocument();
  });

  it('shows the in-app error screen on Android and retries from it', async () => {
    let fail = true;
    mock.api.appInfo = vi.fn(async () => ({ version: '9.9.9', dataDir: '/d', platform: 'android', nativeControls: false }));
    mock.api.boot = vi.fn(async () => { if (fail) throw 'no space'; });
    render(Page);
    expect(await screen.findByRole('alert')).toHaveTextContent('Couldn’t open the data folder.');
    expect(screen.queryByRole('link', { name: 'Skip to content' })).toBeNull();
    expect(screen.queryByRole('button', { name: 'Quit' })).toBeNull();
    fail = false;
    await fireEvent.click(screen.getByRole('button', { name: 'Try again' }));
    expect(await screen.findByRole('heading', { name: 'Dashboard' })).toBeInTheDocument();
  });

  it('detects Android from the user agent when the app info is unavailable', async () => {
    const ua = vi.spyOn(navigator, 'userAgent', 'get').mockReturnValue('Mozilla (Linux; Android 14)');
    mock.api.appInfo = vi.fn(async () => { throw 'x'; });
    mock.api.boot = vi.fn(async () => { throw 'x'; });
    render(Page);
    expect(await screen.findByRole('alert')).toBeVisible();
    ua.mockRestore();
  });

  it('uses the icon rail on tablets', async () => {
    mq({ rail: true });
    render(Page);
    await screen.findByRole('heading', { name: 'Dashboard' });
    expect(viewport.rail).toBe(true);
    expect(screen.queryByText('Blockheads Club')).toBeNull();
  });

  it('uses the bottom bar on phones and highlights More for its screens', async () => {
    mq({ phone: true });
    render(Page);
    expect(await screen.findByRole('heading', { name: 'Server' })).toBeInTheDocument();
    await fireEvent.click(screen.getByRole('button', { name: 'More' }));
    expect(await screen.findByRole('heading', { name: 'More' })).toBeInTheDocument();
    await fireEvent.click(screen.getByRole('button', { name: /Settings/ }));
    expect(await screen.findByRole('heading', { name: 'Settings' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'More' })).toHaveAttribute('aria-current', 'page');
    await fireEvent.click(screen.getByRole('button', { name: 'More' }));
    await fireEvent.click(await screen.findByRole('button', { name: /Console/ }));
    expect(await screen.findByRole('heading', { name: 'Console' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'More' })).toHaveAttribute('aria-current', 'page');
    await fireEvent.click(screen.getByRole('button', { name: 'Mods' }));
    expect(await screen.findByRole('heading', { name: 'Mods' })).toBeInTheDocument();
  });

  it('reacts to viewport changes', async () => {
    mq({});
    render(Page);
    await screen.findByRole('heading', { name: 'Dashboard' });
    mq({ phone: true });
    listeners.forEach((l) => l());
    await waitFor(() => expect(viewport.phone).toBe(true));
  });

  it('layout renders its children', () => {
    render(Layout, { children: createRawSnippet(() => ({ render: () => '<p>child</p>' })) });
    expect(screen.getByText('child')).toBeInTheDocument();
    expect(load.ssr).toBe(false);
  });
});
