import { cleanup, fireEvent, render, screen } from '@testing-library/svelte';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { setLang } from '../i18n.svelte';
import type { SplashState } from '../types';
import Splash from './Splash.svelte';

const st = (over: Partial<SplashState> = {}): SplashState => ({ boot: 'loading', error: '', version: '0.1.0', lang: 'en', ...over });
afterEach(() => {
  cleanup();
  setLang('en');
});

describe('Splash', () => {
  it('shows the loading state with the version and the project line', () => {
    render(Splash, { state: st(), onretry: vi.fn() });
    expect(screen.getByRole('status', { name: 'Blockheads Server is starting' })).not.toHaveClass('sq');
    expect(screen.getByText('Loading worlds and settings…')).toBeVisible();
    expect(screen.getByText('A Blockheads Club project · unofficial')).toBeVisible();
    expect(screen.getByText('0.1.0')).toBeVisible();
    expect(screen.queryByRole('alert')).toBeNull();
  });

  it('says so when opening the data folder is slow, in Turkish too', () => {
    setLang('tr');
    render(Splash, { state: st({ boot: 'slow' }), onretry: vi.fn() });
    expect(screen.getByText('Büyük bir veri klasörü açılıyor — biraz sürebilir.')).toBeVisible();
  });

  it('offers Try again, Choose folder and Quit on error, with the reason', async () => {
    const [onretry, onchoose, onquit] = [vi.fn(), vi.fn(), vi.fn()];
    render(Splash, { state: st({ boot: 'error', error: '/d: denied' }), canChoose: true, onretry, onchoose, onquit });
    expect(screen.getByRole('alert')).toHaveTextContent('The Server couldn’t open its data folder. Your worlds were not changed.');
    expect(screen.getByText('/d: denied')).toBeVisible();
    await fireEvent.click(screen.getByRole('button', { name: 'Try again' }));
    await fireEvent.click(screen.getByRole('button', { name: 'Choose folder…' }));
    await fireEvent.click(screen.getByRole('button', { name: 'Quit' }));
    expect([onretry, onchoose, onquit].map((f) => f.mock.calls.length)).toEqual([1, 1, 1]);
  });

  it('hides Choose folder when it is not offered, and the detail when there is none; buttons without handlers are inert', async () => {
    render(Splash, { state: st({ boot: 'error' }), onretry: vi.fn() });
    expect(screen.queryByRole('button', { name: 'Choose folder…' })).toBeNull();
    await fireEvent.click(screen.getByRole('button', { name: 'Quit' }));
    expect(document.querySelector('.detail')).toBeNull();
    cleanup();
    render(Splash, { state: st({ boot: 'error' }), canChoose: true, onretry: vi.fn() });
    await fireEvent.click(screen.getByRole('button', { name: 'Choose folder…' }));
  });

  it('prefers the detail of a failed action over the boot error', () => {
    render(Splash, { state: st({ boot: 'error', error: 'old' }), detail: 'new', onretry: vi.fn() });
    expect(screen.getByText('new')).toBeVisible();
    expect(screen.queryByText('old')).toBeNull();
  });

  it('is square on macOS', () => {
    render(Splash, { state: st(), square: true, onretry: vi.fn() });
    expect(screen.getByRole('status')).toHaveClass('sq');
  });

  it('phone variant: only the short error and Try again', async () => {
    const onretry = vi.fn();
    const ok = render(Splash, { state: st(), variant: 'phone', onretry });
    expect(screen.queryByText('Loading worlds and settings…')).toBeNull();
    ok.unmount();
    render(Splash, { state: st({ boot: 'error', error: 'x' }), variant: 'phone', canChoose: true, onretry });
    expect(screen.getByRole('alert')).toHaveTextContent('Couldn’t open the data folder.');
    expect(screen.queryByRole('button', { name: 'Quit' })).toBeNull();
    expect(screen.queryByRole('button', { name: 'Choose folder…' })).toBeNull();
    await fireEvent.click(screen.getByRole('button', { name: 'Try again' }));
    expect(onretry).toHaveBeenCalled();
  });
});
