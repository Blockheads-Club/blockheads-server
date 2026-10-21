import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const mock = vi.hoisted(() => ({ api: {} as Record<string, any> }));
vi.mock('$lib/api', () => ({ api: new Proxy({}, { get: (_t, k: string) => mock.api[k] }) }));

import { setLang } from '$lib/i18n.svelte';
import { makeApi } from '$lib/mock-api';
import type { SplashState } from '$lib/types';
import Page from './+page.svelte';

const st = (over: Partial<SplashState> = {}): SplashState => ({ boot: 'loading', error: '', version: '1.2.3', lang: 'en', ...over });

beforeEach(() => {
  mock.api = makeApi();
});
afterEach(() => {
  cleanup();
  setLang('en');
});

describe('splash window page', () => {
  it('is empty until the main window has sent a state, then follows events', async () => {
    let push: (s: SplashState) => void = () => {};
    mock.api.onSplashState = vi.fn(async (cb: (s: SplashState) => void) => { push = cb; return () => {}; });
    render(Page);
    expect(screen.queryByRole('status')).toBeNull();
    await waitFor(() => expect(mock.api.onSplashState).toHaveBeenCalled());
    push(st({ lang: 'tr' }));
    expect(await screen.findByRole('status', { name: 'Blockheads Sunucusu başlıyor' })).toBeVisible();
  });

  it('picks up the state set before it opened and wires its buttons', async () => {
    mock.api.splashGet = vi.fn(async () => st({ boot: 'error', error: '/data: denied' }));
    render(Page);
    expect(await screen.findByRole('alert')).toBeVisible();
    await fireEvent.click(screen.getByRole('button', { name: 'Try again' }));
    await fireEvent.click(screen.getByRole('button', { name: 'Choose folder…' }));
    await fireEvent.click(screen.getByRole('button', { name: 'Quit' }));
    expect(mock.api.splashRetry).toHaveBeenCalled();
    expect(mock.api.chooseDataFolder).toHaveBeenCalled();
    expect(mock.api.appQuit).toHaveBeenCalled();
  });

  it('shows why choosing a folder failed, until the next state arrives', async () => {
    let push: (s: SplashState) => void = () => {};
    mock.api.onSplashState = vi.fn(async (cb: (s: SplashState) => void) => { push = cb; return () => {}; });
    mock.api.splashGet = vi.fn(async () => st({ boot: 'error', error: '/data: denied' }));
    mock.api.chooseDataFolder = vi.fn(async () => { throw '/new: read-only'; });
    render(Page);
    await fireEvent.click(await screen.findByRole('button', { name: 'Choose folder…' }));
    expect(await screen.findByText('/new: read-only')).toBeVisible();
    push(st({ boot: 'error', error: '/data: denied' }));
    await waitFor(() => expect(screen.queryByText('/new: read-only')).toBeNull());
  });

  it('prefers a pushed state over the stored one and survives IPC errors', async () => {
    let push: (s: SplashState) => void = () => {};
    let release: (s: SplashState | null) => void = () => {};
    mock.api.onSplashState = vi.fn(async (cb: (s: SplashState) => void) => { push = cb; return () => {}; });
    mock.api.splashGet = vi.fn(() => new Promise((r) => (release = r)));
    render(Page);
    await waitFor(() => expect(mock.api.onSplashState).toHaveBeenCalled());
    push(st({ boot: 'slow' }));
    release(st());
    expect(await screen.findByText('Opening a big data folder — this can take a moment.')).toBeVisible();
    cleanup();
    mock.api.onSplashState = vi.fn(async () => { throw 'x'; });
    mock.api.splashGet = vi.fn(async () => { throw 'x'; });
    render(Page);
    await waitFor(() => expect(mock.api.splashGet).toHaveBeenCalled());
  });

  it('releases the listener it registered, even if unmounted first', async () => {
    const off = vi.fn();
    mock.api.onSplashState = vi.fn(async () => off);
    const a = render(Page);
    await waitFor(() => expect(mock.api.onSplashState).toHaveBeenCalled());
    await Promise.resolve();
    a.unmount();
    expect(off).toHaveBeenCalled();
    const b = render(Page);
    b.unmount();
    await waitFor(() => expect(off).toHaveBeenCalledTimes(2));
  });

  it('uses square corners on macOS', async () => {
    const ua = vi.spyOn(navigator, 'userAgent', 'get').mockReturnValue('Mozilla (Macintosh)');
    mock.api.splashGet = vi.fn(async () => st());
    render(Page);
    expect(await screen.findByRole('status')).toHaveClass('sq');
    ua.mockRestore();
  });
});
