import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { createRawSnippet } from 'svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const mock = vi.hoisted(() => ({ api: {} as Record<string, any> }));
vi.mock('$lib/api', () => ({ api: new Proxy({}, { get: (_t, k: string) => mock.api[k] }) }));

import { setLang } from '$lib/i18n.svelte';
import { W1, makeApi, makePoll } from '$lib/mock-api';
import { store } from '$lib/store.svelte';
import { theme } from '$lib/theme.svelte';
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
    mock.api.appInfo = vi.fn(async () => ({ version: '9.9.9', dataDir: '/d', platform: 'android' }));
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
    await fireEvent.click(screen.getByRole('button', { name: 'Console' }));
    expect(await screen.findByRole('heading', { name: 'Console' })).toBeInTheDocument();
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
